//! Runtime wrapper for the Atari 800XL.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

use crate::profiles::{Model, profile_for};
use crate::snapshot;

const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct Atari800xlRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Atari800xl>,
    os_bytes: Option<Vec<u8>>,
    basic_bytes: Option<Vec<u8>>,
    cart_bytes: Option<Vec<u8>>,
    basic_enabled: bool,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: crate::input::ControllerCache,
}

impl Atari800xlRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            os_bytes: None,
            basic_bytes: None,
            cart_bytes: None,
            basic_enabled: false,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            controller_cache: crate::input::ControllerCache::default(),
        }
    }

    /// Build with optional OS / BASIC / cart.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidMedia` if the cart fails to parse.
    pub fn new(
        model: Model,
        os: Option<Vec<u8>>,
        basic: Option<Vec<u8>>,
        cart: Option<Vec<u8>>,
        basic_enabled: bool,
    ) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.os_bytes = os;
        runtime.basic_bytes = basic;
        runtime.cart_bytes = cart;
        runtime.basic_enabled = basic_enabled;
        runtime.rebuild_machine()?;
        Ok(runtime)
    }

    pub fn set_os(&mut self, os: Option<Vec<u8>>) -> Result<(), MachineError> {
        self.os_bytes = os;
        self.rebuild_machine()
    }

    pub fn set_basic(&mut self, basic: Option<Vec<u8>>) -> Result<(), MachineError> {
        self.basic_bytes = basic;
        self.rebuild_machine()
    }

    pub fn set_basic_enabled(&mut self, enabled: bool) -> Result<(), MachineError> {
        self.basic_enabled = enabled;
        self.rebuild_machine()
    }

    pub fn insert_cartridge(&mut self, rom: Option<Vec<u8>>) -> Result<(), MachineError> {
        self.cart_bytes = rom;
        self.rebuild_machine()
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Atari800xl> {
        self.machine.as_ref()
    }
    pub fn machine_mut(&mut self) -> Option<&mut Atari800xl> {
        self.machine.as_mut()
    }
    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its CPU/ANTIC/GTIA/POKEY/PIA
    /// and 64 KB RAM state. The framebuffer sizing mirrors `rebuild_machine`
    /// exactly (same `framebuffer_width()` / `framebuffer_height()` getters)
    /// so a runtime whose `blank()` starts with an empty framebuffer Vec does
    /// not panic when the first frame paints.
    pub(crate) fn set_machine(&mut self, machine: Option<Atari800xl>) {
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

    pub(crate) fn os_bytes(&self) -> Option<&[u8]> {
        self.os_bytes.as_deref()
    }
    pub(crate) fn basic_bytes(&self) -> Option<&[u8]> {
        self.basic_bytes.as_deref()
    }
    pub(crate) fn cart_bytes(&self) -> Option<&[u8]> {
        self.cart_bytes.as_deref()
    }
    pub(crate) fn basic_enabled(&self) -> bool {
        self.basic_enabled
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        // The 800XL needs at least a cart OR an OS to boot meaningfully.
        if self.cart_bytes.is_none() && self.os_bytes.is_none() {
            self.machine = None;
            return Ok(());
        }
        let region = match self.model.region() {
            emu198x_shell::Region::Pal => Atari800xlRegion::Pal,
            _ => Atari800xlRegion::Ntsc,
        };
        let machine = Atari800xl::new(
            self.os_bytes.clone(),
            self.basic_bytes.clone(),
            self.cart_bytes.clone(),
            region,
            self.basic_enabled,
        )
        .map_err(|reason| MachineError::InvalidMedia {
            slot: "cartridge-1".to_owned(),
            reason,
        })?;
        let width = machine.framebuffer_width();
        let height = machine.framebuffer_height();
        self.rgba_width = width;
        self.rgba_height = height;
        self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
        Ok(())
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

impl MachineCore for Atari800xlRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }
    fn time(&self) -> MachineTime {
        self.time
    }
    fn reset(&mut self, _kind: ResetKind) {
        let _ = self.rebuild_machine();
        self.time = MachineTime::default();
    }
    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            match (image.slot.as_ref(), image.kind) {
                ("cartridge-1", MediaKind::Cartridge) => {
                    self.insert_cartridge(Some(image.bytes.to_vec()))?;
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
        // Apply queued host input (keyboard + joystick) before running the
        // frame batch. A key press latches in POKEY and persists across these
        // frames; a later call delivers the matching release event.
        if let Some(machine) = self.machine.as_mut() {
            for event in host.input_events {
                crate::input::apply_input_event(machine, &mut self.controller_cache, event);
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

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.machine
            .is_some()
            .then_some(self as &dyn emu198x_shell::KeyboardTarget)
    }
}

emu198x_shell::impl_6502_debug_primitives!(Atari800xlRuntime);

// Keyboard description for the shared `press_key` / `type_string` tools. The
// 800XL types a character by pressing the keycap of the same name (newline →
// `Return`); the input layer drops names it doesn't recognise, so any
// single character is accepted. Hold 3 / settle 6 match the prior tool.
impl emu198x_shell::KeyboardTarget for Atari800xlRuntime {
    fn key_name_is_valid(&self, name: &str) -> bool {
        !name.is_empty()
    }

    fn key_names_hint(&self) -> &'static str {
        "a single character (case-insensitive) or Return"
    }

    fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
        Some(vec![if ch == '\n' || ch == '\r' {
            "Return".to_owned()
        } else {
            ch.to_string()
        }])
    }

    fn key_timing(&self) -> emu198x_shell::KeyTiming {
        emu198x_shell::KeyTiming {
            default_hold_frames: 3,
            max_hold_frames: 600,
            press_settle_frames: 6,
            inter_key_settle_frames: 6,
            repeat_settle_frames: 6,
            default_type_settle_frames: 0,
        }
    }
}
