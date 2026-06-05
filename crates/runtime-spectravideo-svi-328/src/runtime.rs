//! Runtime wrapper for the Spectravideo SVI-328.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_spectravideo_svi_328::{Svi328, SviRegion};
use ti_tms9918::{FB_HEIGHT as VDP_FB_HEIGHT, FB_WIDTH as VDP_FB_WIDTH};

use crate::input::apply_input_event;
use crate::profiles::{BIOS_FIRMWARE_ID, Model, profile_for};
use crate::snapshot;

const BIOS_SIZE: usize = 32 * 1024;
const CART_CEILING: usize = 16 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct Svi328Runtime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Svi328>,
    bios_bytes: Option<Vec<u8>>,
    cart_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    controller_cache: crate::input::ControllerCache,
}

impl Svi328Runtime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            bios_bytes: None,
            cart_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: vec![0; (VDP_FB_WIDTH * VDP_FB_HEIGHT * 4) as usize],
            controller_cache: crate::input::ControllerCache::default(),
        }
    }

    /// Build directly from a 32 KB BASIC/OS ROM.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the ROM size is wrong.
    pub fn new(model: Model, bios: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_bios(bios)?;
        Ok(runtime)
    }

    /// Build from a profile firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if firmware validation fails.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let bytes =
            firmware
                .bytes(BIOS_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: BIOS_FIRMWARE_ID.to_owned(),
                })?;
        Self::new(model, bytes.to_vec())
    }

    /// Replace the ROM image and rebuild the wrapped machine.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the ROM size is wrong.
    pub fn set_bios(&mut self, bios: Vec<u8>) -> Result<(), MachineError> {
        if bios.len() != BIOS_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: BIOS_FIRMWARE_ID.to_owned(),
                reason: format!("ROM is {} bytes; expected {BIOS_SIZE}", bios.len()),
            });
        }
        self.bios_bytes = Some(bios);
        self.rebuild_machine();
        Ok(())
    }

    /// Insert a cart ROM (replaces any existing).
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidMedia` if the cart exceeds the 16 KB ceiling.
    pub fn insert_cartridge(&mut self, rom: Vec<u8>) -> Result<(), MachineError> {
        if rom.len() > CART_CEILING {
            return Err(MachineError::InvalidMedia {
                slot: "cartridge-1".to_owned(),
                reason: format!(
                    "cart is {} bytes; SVI-328 ceiling is {CART_CEILING}",
                    rom.len()
                ),
            });
        }
        self.cart_bytes = Some(rom.clone());
        if let Some(machine) = self.machine.as_mut() {
            machine.insert_cart(rom);
        }
        Ok(())
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Svi328> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Svi328> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn set_bios_bytes(&mut self, bytes: Option<Vec<u8>>) {
        self.bios_bytes = bytes;
    }

    pub(crate) fn set_cart_bytes(&mut self, bytes: Option<Vec<u8>>) {
        self.cart_bytes = bytes;
    }

    pub(crate) fn bios_bytes(&self) -> Option<&[u8]> {
        self.bios_bytes.as_deref()
    }

    pub(crate) fn cart_bytes(&self) -> Option<&[u8]> {
        self.cart_bytes.as_deref()
    }

    pub(crate) fn rebuild_after_restore(&mut self) {
        self.rebuild_machine();
    }

    fn rebuild_machine(&mut self) {
        let Some(bios) = self.bios_bytes.clone() else {
            self.machine = None;
            return;
        };
        let region = match self.model.region() {
            emu198x_shell::Region::Pal => SviRegion::Pal,
            _ => SviRegion::Ntsc,
        };
        let mut machine = Svi328::new(bios, region);
        if let Some(rom) = self.cart_bytes.clone() {
            machine.insert_cart(rom);
        }
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

impl MachineCore for Svi328Runtime {
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
                width: VDP_FB_WIDTH,
                height: VDP_FB_HEIGHT,
                palette: None,
                pixels: &self.rgba_framebuffer,
            })?;
            // AY PSG output not yet exposed by machine-svi-328.
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE,
                channels: 1,
                samples: &[],
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

emu198x_shell::impl_z80_debug_primitives!(Svi328Runtime);
