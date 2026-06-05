//! Runtime wrapper for the Commodore VIC-20.
//!
//! The VIC-20 needs three ROMs at construction (KERNAL, BASIC, char ROM).
//! The runtime defers construction until all three arrive via
//! `set_roms` / `from_firmware`. The VIC chip's audio output is not
//! yet routed through a host buffer; the runtime emits empty audio
//! packets per frame.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_commodore_vic_20::{Vic20, Vic20Model};

use crate::input::apply_input_event;
use crate::profiles::{
    BASIC_FIRMWARE_ID, CHAR_FIRMWARE_ID, KERNAL_FIRMWARE_ID, Model, profile_for,
};
use crate::snapshot;

const KERNAL_SIZE: usize = 8 * 1024;
const BASIC_SIZE: usize = 8 * 1024;
const CHAR_SIZE: usize = 4 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct Vic20Runtime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Vic20>,
    kernal_bytes: Option<Vec<u8>>,
    basic_bytes: Option<Vec<u8>>,
    char_bytes: Option<Vec<u8>>,
    ram_expansion_kb: usize,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: crate::input::ControllerCache,
}

impl Vic20Runtime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            kernal_bytes: None,
            basic_bytes: None,
            char_bytes: None,
            ram_expansion_kb: 0,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            controller_cache: crate::input::ControllerCache::default(),
        }
    }

    /// Build directly from explicit KERNAL + BASIC + char ROMs.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if any ROM size is wrong.
    pub fn new(
        model: Model,
        kernal: Vec<u8>,
        basic: Vec<u8>,
        char_rom: Vec<u8>,
    ) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_roms(kernal, basic, char_rom)?;
        Ok(runtime)
    }

    /// Build from a firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or any ROM is missing.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let kernal =
            firmware
                .bytes(KERNAL_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: KERNAL_FIRMWARE_ID.to_owned(),
                })?;
        let basic =
            firmware
                .bytes(BASIC_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: BASIC_FIRMWARE_ID.to_owned(),
                })?;
        let char_rom =
            firmware
                .bytes(CHAR_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: CHAR_FIRMWARE_ID.to_owned(),
                })?;
        Self::new(model, kernal.to_vec(), basic.to_vec(), char_rom.to_vec())
    }

    /// Replace ROMs and rebuild.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if any size is wrong.
    pub fn set_roms(
        &mut self,
        kernal: Vec<u8>,
        basic: Vec<u8>,
        char_rom: Vec<u8>,
    ) -> Result<(), MachineError> {
        if kernal.len() != KERNAL_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: KERNAL_FIRMWARE_ID.to_owned(),
                reason: format!("KERNAL is {} bytes; expected {KERNAL_SIZE}", kernal.len()),
            });
        }
        if basic.len() != BASIC_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: BASIC_FIRMWARE_ID.to_owned(),
                reason: format!("BASIC is {} bytes; expected {BASIC_SIZE}", basic.len()),
            });
        }
        if char_rom.len() != CHAR_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: CHAR_FIRMWARE_ID.to_owned(),
                reason: format!(
                    "character ROM is {} bytes; expected {CHAR_SIZE}",
                    char_rom.len()
                ),
            });
        }
        self.kernal_bytes = Some(kernal);
        self.basic_bytes = Some(basic);
        self.char_bytes = Some(char_rom);
        self.rebuild_machine();
        Ok(())
    }

    /// Set RAM expansion (0 = unexpanded, 3 = full low expansion, 3+N
    /// for high expansion up to 24 KB).
    pub fn set_ram_expansion_kb(&mut self, kb: usize) {
        self.ram_expansion_kb = kb;
        self.rebuild_machine();
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Vic20> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Vic20> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn set_rom_bytes(
        &mut self,
        kernal: Option<Vec<u8>>,
        basic: Option<Vec<u8>>,
        char_rom: Option<Vec<u8>>,
    ) {
        self.kernal_bytes = kernal;
        self.basic_bytes = basic;
        self.char_bytes = char_rom;
    }

    pub(crate) fn set_ram_expansion_internal(&mut self, kb: usize) {
        self.ram_expansion_kb = kb;
    }

    pub(crate) fn kernal_bytes(&self) -> Option<&[u8]> {
        self.kernal_bytes.as_deref()
    }
    pub(crate) fn basic_bytes(&self) -> Option<&[u8]> {
        self.basic_bytes.as_deref()
    }
    pub(crate) fn char_bytes(&self) -> Option<&[u8]> {
        self.char_bytes.as_deref()
    }
    pub(crate) fn ram_expansion_kb(&self) -> usize {
        self.ram_expansion_kb
    }

    pub(crate) fn rebuild_after_restore(&mut self) {
        self.rebuild_machine();
    }

    fn rebuild_machine(&mut self) {
        let (Some(kernal), Some(basic), Some(char_rom)) = (
            self.kernal_bytes.clone(),
            self.basic_bytes.clone(),
            self.char_bytes.clone(),
        ) else {
            self.machine = None;
            return;
        };
        let vic_model = match self.model.region() {
            emu198x_shell::Region::Pal => Vic20Model::Pal,
            _ => Vic20Model::Ntsc,
        };
        let machine = Vic20::new(kernal, basic, char_rom, vic_model, self.ram_expansion_kb);
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

impl MachineCore for Vic20Runtime {
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

    fn load_media(&mut self, _media: &MediaSet<'_>) -> Result<(), MachineError> {
        // Cartridge / tape loading is a follow-up — the machine
        // currently boots only with ROMs in place.
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

            // VIC audio not yet routed.
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

// 6502 debug target via the shared macro (lazy `machine: Option<Vic20>`).
emu198x_shell::impl_6502_debug_primitives!(Vic20Runtime);
