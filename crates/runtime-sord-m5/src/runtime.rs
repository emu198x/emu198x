//! Runtime wrapper for the Sord M5.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_sord_m5::{M5Region, SordM5};

use crate::input::apply_input_event;
use crate::profiles::{Model, ROM_FIRMWARE_ID, profile_for};
use crate::snapshot;

const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct M5Runtime {
    profile: MachineProfile,
    model: Model,
    machine: Option<SordM5>,
    rom_bytes: Option<Vec<u8>>,
    cart_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
}

impl M5Runtime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            rom_bytes: None,
            cart_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
        }
    }

    /// Build from explicit BIOS ROM.
    #[must_use]
    pub fn new(model: Model, rom: Vec<u8>) -> Self {
        let mut runtime = Self::blank(model);
        runtime.rom_bytes = Some(rom);
        runtime.rebuild_machine();
        runtime
    }

    /// Build from a firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or ROM is missing.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let bytes =
            firmware
                .bytes(ROM_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: ROM_FIRMWARE_ID.to_owned(),
                })?;
        Ok(Self::new(model, bytes.to_vec()))
    }

    /// Replace the BIOS ROM and rebuild.
    pub fn set_rom(&mut self, rom: Vec<u8>) {
        self.rom_bytes = Some(rom);
        self.rebuild_machine();
    }

    /// Insert a cartridge ROM.
    pub fn insert_cartridge(&mut self, rom: Vec<u8>) {
        self.cart_bytes = Some(rom);
        self.rebuild_machine();
    }

    #[must_use]
    pub fn machine(&self) -> Option<&SordM5> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut SordM5> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn set_rom_bytes(&mut self, bytes: Option<Vec<u8>>) {
        self.rom_bytes = bytes;
    }

    pub(crate) fn set_cart_bytes(&mut self, bytes: Option<Vec<u8>>) {
        self.cart_bytes = bytes;
    }

    pub(crate) fn rom_bytes(&self) -> Option<&[u8]> {
        self.rom_bytes.as_deref()
    }

    pub(crate) fn cart_bytes(&self) -> Option<&[u8]> {
        self.cart_bytes.as_deref()
    }

    pub(crate) fn rebuild_after_restore(&mut self) {
        self.rebuild_machine();
    }

    fn rebuild_machine(&mut self) {
        let Some(rom) = self.rom_bytes.clone() else {
            self.machine = None;
            return;
        };
        let region = match self.model.region() {
            emu198x_shell::Region::Pal => M5Region::Pal,
            _ => M5Region::Ntsc,
        };
        let cart = self.cart_bytes.clone().unwrap_or_default();
        let machine = SordM5::new(rom, cart, region);
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

impl MachineCore for M5Runtime {
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
