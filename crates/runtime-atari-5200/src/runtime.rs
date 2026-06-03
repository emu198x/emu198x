//! Runtime wrapper for the Atari 5200.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_atari_5200::{Atari5200, Atari5200Region};

use crate::profiles::{Model, profile_for};
use crate::snapshot;

const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct Atari5200Runtime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Atari5200>,
    cart_bytes: Option<Vec<u8>>,
    bios_bytes: Vec<u8>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
}

impl Atari5200Runtime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            cart_bytes: None,
            bios_bytes: Vec::new(),
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
        }
    }

    /// Build from cart + optional BIOS.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidMedia` if the cart fails to parse.
    pub fn new(model: Model, cart: Vec<u8>, bios: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.bios_bytes = bios;
        runtime.insert_cartridge(cart)?;
        Ok(runtime)
    }

    /// Replace the BIOS image.
    pub fn set_bios(&mut self, bios: Vec<u8>) -> Result<(), MachineError> {
        self.bios_bytes = bios;
        self.rebuild_machine()
    }

    /// Insert a cartridge.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidMedia` if the cart fails to parse.
    pub fn insert_cartridge(&mut self, rom: Vec<u8>) -> Result<(), MachineError> {
        self.cart_bytes = Some(rom);
        self.rebuild_machine()
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Atari5200> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Atari5200> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn set_cart_bytes(&mut self, bytes: Option<Vec<u8>>) {
        self.cart_bytes = bytes;
    }

    pub(crate) fn set_bios_bytes(&mut self, bytes: Vec<u8>) {
        self.bios_bytes = bytes;
    }

    pub(crate) fn cart_bytes(&self) -> Option<&[u8]> {
        self.cart_bytes.as_deref()
    }

    pub(crate) fn bios_bytes(&self) -> &[u8] {
        &self.bios_bytes
    }

    pub(crate) fn rebuild_after_restore(&mut self) -> Result<(), MachineError> {
        self.rebuild_machine()
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        let Some(rom) = self.cart_bytes.clone() else {
            self.machine = None;
            return Ok(());
        };
        let region = match self.model.region() {
            emu198x_shell::Region::Pal => Atari5200Region::Pal,
            _ => Atari5200Region::Ntsc,
        };
        let machine = Atari5200::new(rom, self.bios_bytes.clone(), region).map_err(|reason| {
            MachineError::InvalidMedia {
                slot: "cartridge-1".to_owned(),
                reason,
            }
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

impl MachineCore for Atari5200Runtime {
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
                    self.insert_cartridge(image.bytes.to_vec())?;
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

emu198x_shell::impl_6502_debug_target!(Atari5200Runtime);
