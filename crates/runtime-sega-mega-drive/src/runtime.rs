//! The Mega Drive runtime: the machine behind the shell's traits.
//!
//! Shaped like the Master System's: the cartridge bytes are kept so a reset
//! or a model switch can rebuild the console, and the VDP's ARGB picture is
//! converted to the host's RGBA after every frame. Time is counted in master
//! clocks.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_sega_mega_drive::{AUDIO_SAMPLE_RATE, FRAME_STRIDE, MegaDrive};

use crate::Model;
use crate::cartridge;
use crate::input::apply_input_event;
use crate::profiles::profile_for;
use crate::snapshot;
use emu198x_shell::display::Display;

/// The cartridge type that names a Mega Drive cartridge.
pub const MEGA_DRIVE: &str = "mega-drive";
/// The cartridge type that names a Master System cartridge, run through
/// the Power Base Converter.
pub const MASTER_SYSTEM: &str = "master-system";

pub struct MegaDriveRuntime {
    model: Model,
    profile: MachineProfile,
    pub(crate) machine: Option<MegaDrive>,
    cart_bytes: Option<Vec<u8>>,
    /// Whether the cartridge goes in through the Power Base Converter.
    power_base: bool,
    pub(crate) time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
}

impl MegaDriveRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            model,
            profile: profile_for(model),
            machine: None,
            cart_bytes: None,
            power_base: false,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
        }
    }

    #[must_use]
    pub fn new(model: Model, cart_rom: Vec<u8>) -> Self {
        let mut runtime = Self::blank(model);
        runtime.insert_cartridge(cart_rom);
        runtime
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    /// Insert a cartridge image, raw or `.smd`, and power the console on.
    /// A Master System image, told by its headers, goes in through the
    /// Power Base Converter.
    pub fn insert_cartridge(&mut self, image: Vec<u8>) {
        let rom = cartridge::normalise(image);
        if cartridge::is_master_system(&rom) {
            self.insert_master_system_cartridge(rom);
        } else {
            self.insert_mega_drive_cartridge(rom);
        }
    }

    /// Insert a Mega Drive cartridge, raw or `.smd`, whatever its headers
    /// say.
    pub fn insert_mega_drive_cartridge(&mut self, image: Vec<u8>) {
        self.cart_bytes = Some(cartridge::normalise(image));
        self.power_base = false;
        self.rebuild_machine();
    }

    /// Insert a Master System cartridge through the Power Base Converter,
    /// whatever its headers say: the console starts in Master System mode.
    pub fn insert_master_system_cartridge(&mut self, image: Vec<u8>) {
        self.cart_bytes = Some(cartridge::normalise_master_system(image));
        self.power_base = true;
        self.rebuild_machine();
    }

    /// Whether the cartridge runs through the Power Base Converter.
    #[must_use]
    pub fn power_base(&self) -> bool {
        self.power_base
    }

    #[must_use]
    pub fn machine(&self) -> Option<&MegaDrive> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut MegaDrive> {
        self.machine.as_mut()
    }

    /// The model id recorded in snapshots.
    #[must_use]
    pub fn model_id(&self) -> &'static str {
        self.model.model_id()
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn cart_bytes(&self) -> Option<&[u8]> {
        self.cart_bytes.as_deref()
    }

    /// Install a machine restored from a snapshot. Its ROM becomes the
    /// cartridge a later reset or model switch rebuilds from, since a
    /// snapshot can be restored into a runtime that never had one.
    pub(crate) fn set_machine(&mut self, machine: Option<MegaDrive>) {
        if let Some(machine) = &machine {
            self.cart_bytes = Some(machine.rom().to_vec());
            self.power_base = machine.master_system_mode();
        }
        self.machine = machine;
        self.update_rgba_framebuffer();
    }

    fn rebuild_machine(&mut self) {
        let Some(rom) = self.cart_bytes.clone() else {
            self.machine = None;
            return;
        };
        let region = self.model.machine_region();
        self.machine = Some(if self.power_base {
            MegaDrive::new_power_base(rom, region)
        } else {
            MegaDrive::new(rom, region)
        });
        self.update_rgba_framebuffer();
    }

    /// Convert the VDP's `0xAARRGGBB` picture, at its stride, to packed
    /// RGBA bytes. The size follows the VDP, which changes between H32 and
    /// H40 as a game sets its mode.
    pub(crate) fn update_rgba_framebuffer(&mut self) {
        let Some(machine) = self.machine.as_ref() else {
            self.rgba_framebuffer.fill(0);
            return;
        };
        let (width, height) = (machine.frame_width(), machine.frame_height());
        if (width, height) != (self.rgba_width, self.rgba_height) {
            self.rgba_width = width;
            self.rgba_height = height;
            self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        }
        let pixels = machine.framebuffer();
        for (row, out) in self
            .rgba_framebuffer
            .chunks_exact_mut(width as usize * 4)
            .enumerate()
        {
            let line = &pixels[row * FRAME_STRIDE..row * FRAME_STRIDE + width as usize];
            for (rgba, &pixel) in out.as_chunks_mut::<4>().0.iter_mut().zip(line) {
                *rgba = [
                    (pixel >> 16) as u8,
                    (pixel >> 8) as u8,
                    pixel as u8,
                    (pixel >> 24) as u8,
                ];
            }
        }
    }
}

impl emu198x_shell::FamilyRuntime for MegaDriveRuntime {
    type Model = Model;
    fn variant_ids() -> &'static [&'static str] {
        &Model::VARIANT_IDS
    }
    fn model_from_id(id: &str) -> Option<Model> {
        Model::from_variant_id(id)
    }
    fn variant_id(model: Model) -> &'static str {
        model.variant_id()
    }
    fn profile_for(model: Model) -> MachineProfile {
        profile_for(model)
    }
    fn rom_convention() -> emu198x_shell::RomConvention {
        emu198x_shell::RomConvention {
            env_var: None,
            dirs: &[],
        }
    }
    fn firmware_sources(_model: Model) -> Vec<emu198x_shell::FirmwareSource> {
        Vec::new()
    }
    fn from_firmware(
        model: Model,
        firmware: &emu198x_shell::FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        firmware.validate_for_profile(&profile_for(model))?;
        Ok(Self::blank(model))
    }
    /// The machine is not `Clone`, so a switch cold-boots the new model
    /// from the kept cartridge bytes.
    fn replacement(
        &self,
        model: Model,
        firmware: &emu198x_shell::FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        let mut replacement = Self::from_firmware(model, firmware)?;
        if let Some(cart) = &self.cart_bytes {
            if self.power_base {
                replacement.insert_master_system_cartridge(cart.clone());
            } else {
                replacement.insert_mega_drive_cartridge(cart.clone());
            }
        }
        Ok(replacement)
    }
    fn native_frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }
}

impl MegaDriveRuntime {
    fn run_window(
        &mut self,
        target: MachineTime,
        one_frame: bool,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        if self.machine.is_none() {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        }

        for event in host.input_events {
            if let Some(machine) = self.machine.as_mut() {
                apply_input_event(machine, event);
            }
        }

        while self.time < target {
            let ticks = self
                .machine
                .as_mut()
                .expect("machine checked above")
                .run_frame();
            self.time = self.time.saturating_add(ticks);
            self.update_rgba_framebuffer();

            host.frame_sink.push_frame(FramePacket {
                signal: None,
                timestamp: self.time,
                format: PixelFormat::Rgba8888,
                width: self.rgba_width,
                height: self.rgba_height,
                palette: None,
                pixels: &self.rgba_framebuffer,
            })?;

            let audio = self
                .machine
                .as_mut()
                .expect("machine checked above")
                .take_audio();
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE,
                channels: 2,
                samples: &audio,
            })?;
            if one_frame {
                break;
            }
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }
}

impl MachineCore for MegaDriveRuntime {
    fn set_machine<Q: emu198x_shell::SessionQueryProvider<Self>>(
        session: &mut emu198x_shell::HeadlessSession<Self, Q>,
        machine: &str,
    ) -> Result<emu198x_shell::VariantSwitched, emu198x_shell::LoaderError> {
        emu198x_shell::swap_variant(session, machine)
    }

    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    /// A soft reset is the reset button: the 68000 restarts and the Z80 is
    /// held, but the VDP and RAM keep what they had. Any other powers the
    /// console off and on.
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Soft {
            if let Some(machine) = self.machine.as_mut() {
                machine.reset();
            }
        } else {
            self.rebuild_machine();
            self.time = MachineTime::default();
        }
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            match (image.slot.as_ref(), image.kind) {
                ("cartridge-1", MediaKind::Cartridge) => {
                    let bytes = image.bytes.to_vec();
                    match image.cart_type.as_deref() {
                        Some(MASTER_SYSTEM) => self.insert_master_system_cartridge(bytes),
                        Some(_) => self.insert_mega_drive_cartridge(bytes),
                        None => self.insert_cartridge(bytes),
                    }
                }
                (slot, MediaKind::Cartridge) => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                (_, kind) => {
                    return Err(MachineError::UnsupportedMediaKind { kind });
                }
            }
        }
        Ok(())
    }

    /// A Mega Drive cartridge, or a Master System one through the Power
    /// Base Converter, for an image whose headers do not say which.
    fn cartridge_types(&self) -> &'static [&'static str] {
        &[MEGA_DRIVE, MASTER_SYSTEM]
    }

    fn run_frame(&mut self, host: &mut HostIo<'_>) -> Result<RunResult, MachineError> {
        self.run_window(MachineTime::new(u64::MAX), true, host)
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        self.run_window(target, false, host)
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        snapshot::encode(self)
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        snapshot::decode(self, bytes)
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: command.operation_name(),
        })
    }

    /// The VDP's dot clock is the master clock over 8 in H40 and over 10
    /// in H32, so the pixel aspect follows the mode the game set. An H40
    /// line fills the framebuffer's whole stride.
    fn display(&self) -> Option<Display> {
        let master = self.model.master_hz() as f64;
        let h32 = self
            .machine
            .as_ref()
            .is_some_and(|machine| machine.frame_width() as usize != FRAME_STRIDE);
        let divider = if h32 { 10.0 } else { 8.0 };
        Display::television_for_region(self.profile().region, master / divider, master / divider)
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
}
