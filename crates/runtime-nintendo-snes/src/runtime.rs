//! The Super NES runtime: the machine behind the shell's traits.
//!
//! Shaped like the Mega Drive's: the cartridge image is kept so a reset or
//! a model switch can rebuild the console, and the PPU's ARGB picture is
//! converted to the host's RGBA after every frame. Time is counted in master
//! clocks.
//!
//! The picture is 512 pixels wide, two per dot, so hi-res modes fit, and 240
//! rows, or 480 while the PPU runs an interlaced frame.

use crate::machine::SnesMachine as Snes;
use common_nintendo_snes::{AUDIO_SAMPLE_RATE, ipl};
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};

use crate::Model;
use crate::cartridge;
use crate::input::apply_input_event;
use crate::profiles::profile_for;
use crate::snapshot;
use emu198x_shell::display::Display;

/// The PPU's framebuffer stride.
const FRAME_WIDTH: u32 = 512;

pub struct SnesRuntime {
    model: Model,
    profile: MachineProfile,
    pub(crate) machine: Option<Snes>,
    cart_image: Option<Vec<u8>>,
    ipl: Option<[u8; 64]>,
    pub(crate) time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_height: u32,
    /// Why the last cartridge was refused, if it was.
    refusal: Option<String>,
}

impl SnesRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            model,
            profile: profile_for(model),
            machine: None,
            cart_image: None,
            ipl: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_height: 0,
            refusal: None,
        }
    }

    #[must_use]
    pub fn new(model: Model, image: Vec<u8>) -> Self {
        let mut runtime = Self::blank(model);
        // A refused image leaves the runtime blank; `load_media` reports it.
        let _ = runtime.insert_cartridge(image);
        runtime
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    /// Whether the original IPL ROM was supplied, rather than the built-in
    /// clean-room program.
    #[must_use]
    pub fn has_original_ipl(&self) -> bool {
        self.ipl.is_some_and(|rom| rom != ipl::CLEAN_ROOM)
    }

    /// Insert a cartridge image and power the console on.
    ///
    /// # Errors
    ///
    /// The board needs a coprocessor this machine does not model, or the
    /// image is empty.
    pub fn insert_cartridge(&mut self, image: Vec<u8>) -> Result<(), MachineError> {
        if let Err(error) = cartridge::board(&image) {
            self.refusal = Some(error.to_string());
            return Err(MachineError::InvalidMedia {
                slot: "cartridge-1".to_owned(),
                reason: error.to_string(),
            });
        }
        self.refusal = None;
        self.cart_image = Some(image);
        self.rebuild_machine();
        Ok(())
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Snes> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Snes> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model_id(&self) -> &'static str {
        self.model.model_id()
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn cart_loaded(&self) -> bool {
        self.cart_image.is_some()
    }

    /// Install a machine restored from a snapshot. Its cartridge becomes
    /// the one a later reset rebuilds from, since a snapshot can be
    /// restored into a runtime that never had one.
    pub(crate) fn set_machine(&mut self, machine: Option<Snes>) {
        self.cart_image = machine.as_ref().map(|m| m.cart.rom().to_vec());
        self.ipl = machine
            .as_ref()
            .map(|m| {
                // Snapshot validation checks this length before installation.
                let mut bytes = [0; 64];
                bytes.copy_from_slice(&m.apu.ipl);
                bytes
            })
            .filter(|bytes| *bytes != ipl::CLEAN_ROOM);
        self.machine = machine;
        self.update_rgba_framebuffer();
    }

    pub(crate) fn snapshot_ipl(&self) -> Option<&[u8]> {
        self.machine
            .as_ref()
            .map(|m| m.apu.ipl.as_slice())
            .or_else(|| self.ipl.as_ref().map(|rom| rom.as_slice()))
            .filter(|rom| *rom != ipl::CLEAN_ROOM)
    }

    pub(crate) fn set_snapshot_ipl(&mut self, rom: Option<[u8; 64]>) {
        self.ipl = rom.filter(|rom| *rom != ipl::CLEAN_ROOM);
    }

    fn rebuild_machine(&mut self) {
        let Some(image) = self.cart_image.as_deref() else {
            self.machine = None;
            return;
        };
        let Ok(mut cart) = cartridge::board(image) else {
            self.machine = None;
            return;
        };
        if let Some(previous) = self.machine.as_ref().filter(|m| m.cart.rom() == image) {
            cart.load_ram(previous.cart.ram());
        }
        self.machine = Some(Snes::new(self.model, cart, self.ipl));
        self.time = MachineTime::default();
        self.update_rgba_framebuffer();
    }

    fn frame_height(&self) -> u32 {
        match &self.machine {
            Some(m) if m.ppu.interlaced() => 480,
            _ => 240,
        }
    }

    /// Convert the PPU's `0xAARRGGBB` picture to packed RGBA bytes.
    pub(crate) fn update_rgba_framebuffer(&mut self) {
        let height = self.frame_height();
        if height != self.rgba_height || self.rgba_framebuffer.is_empty() {
            self.rgba_height = height;
            self.rgba_framebuffer = vec![0; (FRAME_WIDTH * height * 4) as usize];
        }
        let Some(machine) = self.machine.as_ref() else {
            self.rgba_framebuffer.fill(0);
            return;
        };
        let pixels = &machine.framebuffer()[..(FRAME_WIDTH * height) as usize];
        for (rgba, &pixel) in self
            .rgba_framebuffer
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(pixels)
        {
            *rgba = [
                (pixel >> 16) as u8,
                (pixel >> 8) as u8,
                pixel as u8,
                (pixel >> 24) as u8,
            ];
        }
    }
}

impl emu198x_shell::FamilyRuntime for SnesRuntime {
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
            env_var: Some("EMU198X_SNES_ROM_DIR"),
            dirs: &["nintendo-snes", "snes"],
        }
    }
    fn firmware_sources(model: Model) -> Vec<emu198x_shell::FirmwareSource> {
        model.firmware_sources()
    }
    /// The IPL ROM is optional: supplied, it must be 64 bytes; absent, the
    /// clean-room program boots.
    fn from_firmware(
        model: Model,
        firmware: &emu198x_shell::FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        firmware.validate_for_profile(&profile_for(model))?;
        let mut runtime = Self::blank(model);
        if let Some(bytes) = firmware.bytes(ipl::FIRMWARE_ID) {
            let rom: [u8; 64] = bytes
                .try_into()
                .map_err(|_| MachineError::InvalidFirmware {
                    id: ipl::FIRMWARE_ID.to_owned(),
                    reason: format!("the IPL ROM is 64 bytes; this file is {}", bytes.len()),
                })?;
            runtime.ipl = Some(rom);
        }
        Ok(runtime)
    }
    fn replacement(
        &self,
        model: Model,
        firmware: &emu198x_shell::FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        let mut replacement = Self::from_firmware(model, firmware)?;
        if replacement.ipl.is_none() {
            replacement.ipl = self.ipl;
        }
        if let Some(image) = &self.cart_image {
            replacement.insert_cartridge(image.clone())?;
            if let (Some(previous), Some(next)) =
                (self.machine.as_ref(), replacement.machine.as_mut())
            {
                next.load_cartridge_ram(previous.cart.ram());
            }
        }
        Ok(replacement)
    }
    fn native_frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }
}

impl SnesRuntime {
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
            let machine = self.machine.as_mut().expect("machine checked above");
            let ticks = machine.run_frame();
            self.time = self.time.saturating_add(ticks);
            self.update_rgba_framebuffer();
            host.frame_sink.push_frame(FramePacket {
                signal: None,
                timestamp: self.time,
                format: PixelFormat::Rgba8888,
                width: FRAME_WIDTH,
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

impl MachineCore for SnesRuntime {
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

    /// A soft reset is the reset button: the CPUs restart, the RAMs keep
    /// what they had. Any other powers the console off and on.
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
                    self.insert_cartridge(image.bytes.to_vec())?;
                }
                (slot, MediaKind::Cartridge) => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                (_, kind) => return Err(MachineError::UnsupportedMediaKind { kind }),
            }
        }
        Ok(())
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

    /// The framebuffer holds two pixels per dot, so its pixel clock is the
    /// master clock over 2; an interlaced frame's 480 rows halve each
    /// pixel's height, which is the same as halving that clock.
    fn display(&self) -> Option<Display> {
        let master = self.model.master_hz() as f64;
        let divider = if self.frame_height() == 480 { 4.0 } else { 2.0 };
        Display::television_for_region(self.profile().region, master / divider, master / divider)
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
}
