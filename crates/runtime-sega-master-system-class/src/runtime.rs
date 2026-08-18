//! Runtime wrapper shared by every machine in the Master System class.
//!
//! The Master System and the Game Gear are separate machines that happen to
//! run the same wrapper: same silicon, same snapshot envelope, same query
//! surface. This crate holds that shared half so each machine can own its
//! own runtime crate with a single `machine_id` (#998).
//!
//! The machine's identity arrives as data — a profile, a hardware variant
//! and a model id — rather than as a type parameter. A generic
//! `SmsRuntime<M>` would put `MachineCore` and `DebugPrimitives` impls out
//! of reach of the per-machine crates: both traits are foreign, and
//! `SmsRuntime<LocalModel>` is not a local type for the orphan rule.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_sega_master_system::{Sms, SmsVariant};

use crate::input::{ControllerCache, apply_input_event};
use crate::snapshot;

const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct SmsRuntime {
    profile: MachineProfile,
    variant: SmsVariant,
    /// Stable per-model identifier, checked on snapshot restore so a Game
    /// Gear state cannot be loaded into a Master System.
    model_id: &'static str,
    machine: Option<Sms>,
    cart_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: ControllerCache,
}

impl SmsRuntime {
    #[must_use]
    pub fn blank(profile: MachineProfile, variant: SmsVariant, model_id: &'static str) -> Self {
        Self {
            profile,
            variant,
            model_id,
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
    pub fn new(
        profile: MachineProfile,
        variant: SmsVariant,
        model_id: &'static str,
        cart_rom: Vec<u8>,
    ) -> Self {
        let mut runtime = Self::blank(profile, variant, model_id);
        runtime.insert_cartridge(cart_rom);
        runtime
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

    /// The hardware variant this runtime drives.
    #[must_use]
    pub fn variant(&self) -> SmsVariant {
        self.variant
    }

    /// The model id recorded in snapshots.
    #[must_use]
    pub fn model_id(&self) -> &'static str {
        self.model_id
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
        let Some(rom) = self.cart_bytes.clone() else {
            self.machine = None;
            return;
        };
        let machine = Sms::new(rom, self.variant);
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

impl MachineCore for SmsRuntime {
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

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
}

emu198x_shell::impl_z80_debug_primitives!(SmsRuntime);
