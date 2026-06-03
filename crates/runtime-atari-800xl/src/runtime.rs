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

    pub(crate) fn set_state(
        &mut self,
        os: Option<Vec<u8>>,
        basic: Option<Vec<u8>>,
        cart: Option<Vec<u8>>,
        basic_enabled: bool,
    ) {
        self.os_bytes = os;
        self.basic_bytes = basic;
        self.cart_bytes = cart;
        self.basic_enabled = basic_enabled;
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

    pub(crate) fn rebuild_after_restore(&mut self) -> Result<(), MachineError> {
        self.rebuild_machine()
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
        // Apply queued host input (keyboard) before running the frame batch.
        // A key press latches in POKEY and persists across these frames; a
        // later call delivers the matching release event.
        if let Some(machine) = self.machine.as_mut() {
            for event in host.input_events {
                crate::input::apply_input_event(machine, event);
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
}
