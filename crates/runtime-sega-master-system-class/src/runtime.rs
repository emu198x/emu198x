//! Runtime wrapper shared by every machine in the Master System class.
//!
//! The Master System and the Game Gear are separate machines that happen to
//! run the same wrapper: same silicon, same snapshot envelope, same query
//! surface. This crate holds that shared half so each machine can own its
//! own runtime crate with a single `machine_id` (#998).
//!
//! Each system supplies its model catalogue. The class owns the generic runtime
//! and all trait implementations, so the per-system crates need only implement
//! `SmsModel` and publish a concrete runtime alias.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_sega_master_system::{Sms, SmsVariant};

use crate::input::{ControllerCache, apply_input_event};
use crate::snapshot;
use emu198x_shell::display::Display;

const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Model metadata supplied by each system's runtime catalogue.
pub trait SmsModel: Copy + 'static {
    const VARIANT_IDS: &'static [&'static str];
    fn from_variant_id(id: &str) -> Option<Self>;
    fn variant_id(self) -> &'static str;
    fn model_id(self) -> &'static str;
    fn profile(self) -> MachineProfile;
    fn variant(self) -> SmsVariant;
    fn frame_ticks(self) -> u64;
}

pub struct SmsRuntime<M: SmsModel> {
    model: M,
    profile: MachineProfile,
    machine: Option<Sms>,
    cart_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: ControllerCache,
}

impl<M: SmsModel> SmsRuntime<M> {
    #[must_use]
    pub fn blank(model: M) -> Self {
        Self {
            model,
            profile: model.profile(),
            machine: None,
            cart_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            controller_cache: ControllerCache::default(),
        }
    }

    #[must_use]
    pub fn new(model: M, cart_rom: Vec<u8>) -> Self {
        let mut runtime = Self::blank(model);
        runtime.insert_cartridge(cart_rom);
        runtime
    }

    #[must_use]
    pub fn model(&self) -> M {
        self.model
    }

    pub fn insert_cartridge(&mut self, rom: Vec<u8>) {
        self.cart_bytes = Some(rom);
        self.rebuild_machine();
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Sms> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Sms> {
        self.machine.as_mut()
    }

    /// Returns a changed battery-save image suitable for a `.sav` sidecar.
    #[must_use]
    pub fn cartridge_save_image(&self) -> Option<&[u8]> {
        self.machine
            .as_ref()
            .filter(|machine| machine.cartridge_ram_dirty())
            .map(Sms::cartridge_ram)
    }

    /// Restore a 32 KB cartridge SRAM sidecar.
    pub fn restore_cartridge_save_image(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let machine = self
            .machine
            .as_mut()
            .ok_or_else(|| MachineError::InvalidMedia {
                slot: "cartridge-1".to_owned(),
                reason: "no cartridge is loaded".to_owned(),
            })?;
        if !machine.restore_cartridge_ram(bytes, false) {
            return Err(MachineError::InvalidMedia {
                slot: "cartridge-1".to_owned(),
                reason: format!("save RAM length {} does not match 32768", bytes.len()),
            });
        }
        Ok(())
    }

    /// The hardware variant this runtime drives.
    #[must_use]
    pub fn variant(&self) -> SmsVariant {
        self.model.variant()
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

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its CPU/VDP/PSG/RAM/mapper
    /// state.
    pub(crate) fn set_machine(&mut self, machine: Option<Sms>) {
        if let Some(machine) = &machine {
            let width = machine.framebuffer_width();
            let height = machine.framebuffer_height();
            self.rgba_width = width;
            self.rgba_height = height;
            self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        }
        self.machine = machine;
        self.update_rgba_framebuffer();
    }

    fn rebuild_machine(&mut self) {
        let preserved_ram = self.machine.as_ref().map(|machine| {
            (
                machine.cartridge_ram().to_vec(),
                machine.cartridge_ram_dirty(),
            )
        });
        let Some(rom) = self.cart_bytes.clone() else {
            self.machine = None;
            return;
        };
        let mut machine = Sms::new(rom, self.variant());
        if let Some((ram, dirty)) = preserved_ram {
            let restored = machine.restore_cartridge_ram(&ram, dirty);
            debug_assert!(restored);
        }
        let width = machine.framebuffer_width();
        let height = machine.framebuffer_height();
        self.rgba_width = width;
        self.rgba_height = height;
        self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
    }

    fn update_rgba_framebuffer(&mut self) {
        let Some(machine) = self.machine.as_ref() else {
            self.rgba_framebuffer.fill(0);
            return;
        };
        for (index, &pixel) in machine.framebuffer().iter().enumerate() {
            let base = index * 4;
            self.rgba_framebuffer[base] = ((pixel >> 16) & 0xff) as u8;
            self.rgba_framebuffer[base + 1] = ((pixel >> 8) & 0xff) as u8;
            self.rgba_framebuffer[base + 2] = (pixel & 0xff) as u8;
            self.rgba_framebuffer[base + 3] = ((pixel >> 24) & 0xff) as u8;
        }
    }
}

impl<M: SmsModel> emu198x_shell::FamilyRuntime for SmsRuntime<M> {
    type Model = M;
    fn variant_ids() -> &'static [&'static str] {
        M::VARIANT_IDS
    }
    fn model_from_id(id: &str) -> Option<M> {
        M::from_variant_id(id)
    }
    fn variant_id(model: M) -> &'static str {
        model.variant_id()
    }
    fn profile_for(model: M) -> MachineProfile {
        model.profile()
    }
    fn rom_convention() -> emu198x_shell::RomConvention {
        emu198x_shell::RomConvention {
            env_var: None,
            dirs: &[],
        }
    }
    fn firmware_sources(_model: M) -> Vec<emu198x_shell::FirmwareSource> {
        Vec::new()
    }
    fn from_firmware(
        model: M,
        firmware: &emu198x_shell::FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        firmware.validate_for_profile(&model.profile())?;
        Ok(Self::blank(model))
    }
    fn replacement(
        &self,
        model: M,
        firmware: &emu198x_shell::FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        let mut replacement = Self::from_firmware(model, firmware)?;
        if let Some(cart) = &self.cart_bytes {
            replacement.insert_cartridge(cart.clone());
        }
        if let (Some(old), Some(new)) = (self.machine(), replacement.machine_mut()) {
            let restored =
                new.restore_cartridge_ram(old.cartridge_ram(), old.cartridge_ram_dirty());
            debug_assert!(restored);
        }
        Ok(replacement)
    }
    fn native_frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }
}

impl<M: SmsModel> MachineCore for SmsRuntime<M> {
    fn set_machine<Q: emu198x_shell::SessionQueryProvider<Self>>(
        session: &mut emu198x_shell::HeadlessSession<Self, Q>,
        machine: &str,
    ) -> Result<emu198x_shell::VariantSwitched, emu198x_shell::LoaderError> {
        if M::VARIANT_IDS.len() < 2 {
            return Err(emu198x_shell::LoaderError::Unsupported {
                step: "set_machine",
            });
        }
        emu198x_shell::swap_variant(session, machine)
    }

    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine();
        self.time = MachineTime::default();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            match (image.slot.as_ref(), image.kind) {
                ("cartridge-1", MediaKind::Cartridge) => {
                    self.insert_cartridge(image.bytes.to_vec());
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

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        if self.machine.is_none() {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        }

        for event in host.input_events {
            if let Some(machine) = self.machine.as_mut() {
                apply_input_event(machine, &mut self.controller_cache, event);
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
                .take_audio_buffer();
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE,
                channels: 1,
                samples: &audio,
            })?;
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
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

    /// One runtime, two machines, two kinds of display. A Master System's
    /// VDP feeds a television and its dots are 8:7 on NTSC; the same silicon
    /// in a Game Gear feeds a 160x144 LCD whose pixels are square.
    ///
    /// The Game Gear's profile still reports `Region::Ntsc`, which is true of
    /// its timing and says nothing about the panel — so this branches on the
    /// variant, not the region. See
    /// `knowledge/decisions/pixel-aspect-comes-from-the-raster.md`.
    fn display(&self) -> Option<Display> {
        match self.variant() {
            SmsVariant::GameGear => Some(Display::Lcd),
            _ => Display::television_for_region(
                self.profile().region,
                sega_vdp::PAL_DOT_CLOCK_HZ,
                sega_vdp::NTSC_DOT_CLOCK_HZ,
            ),
        }
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
}

emu198x_shell::impl_z80_debug_primitives!(impl<M: SmsModel> SmsRuntime<M>);
