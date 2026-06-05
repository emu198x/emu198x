//! Runtime wrapper for the Acorn Electron.
//!
//! Construction needs both 16 KB ROMs (OS + BASIC). The runtime defers
//! the machine until both arrive — via `set_roms` / `from_firmware` /
//! the MCP `load_media` path. Electron audio is the ULA's 1-bit sound
//! generator; the chip crate exposes `take_audio_buffer` so the runtime
//! pushes those samples per frame at 48 kHz.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_acorn_electron::AcornElectron;

use crate::input::apply_input_event;
use crate::profiles::{BASIC_FIRMWARE_ID, Model, OS_FIRMWARE_ID, profile_for};
use crate::snapshot;

const ROM_SIZE: usize = 16 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct ElectronRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<AcornElectron>,
    os_bytes: Option<Vec<u8>>,
    basic_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
}

impl ElectronRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            os_bytes: None,
            basic_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
        }
    }

    /// Build directly from explicit OS + BASIC ROMs.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if either ROM is not 16 KB.
    pub fn new(model: Model, os_rom: Vec<u8>, basic_rom: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_roms(os_rom, basic_rom)?;
        Ok(runtime)
    }

    /// Build from a profile firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if firmware validation fails or either ROM is missing.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let os = firmware
            .bytes(OS_FIRMWARE_ID)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: OS_FIRMWARE_ID.to_owned(),
            })?;
        let basic =
            firmware
                .bytes(BASIC_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: BASIC_FIRMWARE_ID.to_owned(),
                })?;
        Self::new(model, os.to_vec(), basic.to_vec())
    }

    /// Replace both ROMs and rebuild the wrapped machine.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if either ROM is not 16 KB.
    pub fn set_roms(&mut self, os_rom: Vec<u8>, basic_rom: Vec<u8>) -> Result<(), MachineError> {
        if os_rom.len() != ROM_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: OS_FIRMWARE_ID.to_owned(),
                reason: format!("OS ROM is {} bytes; expected {ROM_SIZE}", os_rom.len()),
            });
        }
        if basic_rom.len() != ROM_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: BASIC_FIRMWARE_ID.to_owned(),
                reason: format!(
                    "BASIC ROM is {} bytes; expected {ROM_SIZE}",
                    basic_rom.len()
                ),
            });
        }
        self.os_bytes = Some(os_rom);
        self.basic_bytes = Some(basic_rom);
        self.rebuild_machine();
        Ok(())
    }

    #[must_use]
    pub fn machine(&self) -> Option<&AcornElectron> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut AcornElectron> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn set_rom_bytes(&mut self, os: Option<Vec<u8>>, basic: Option<Vec<u8>>) {
        self.os_bytes = os;
        self.basic_bytes = basic;
    }

    pub(crate) fn os_bytes(&self) -> Option<&[u8]> {
        self.os_bytes.as_deref()
    }

    pub(crate) fn basic_bytes(&self) -> Option<&[u8]> {
        self.basic_bytes.as_deref()
    }

    pub(crate) fn rebuild_after_restore(&mut self) {
        self.rebuild_machine();
    }

    fn rebuild_machine(&mut self) {
        let (Some(os), Some(basic)) = (self.os_bytes.clone(), self.basic_bytes.clone()) else {
            self.machine = None;
            return;
        };
        let machine = AcornElectron::new(os, basic);
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

impl MachineCore for ElectronRuntime {
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
        // Cassette tape loading is not yet wired; the Electron currently
        // boots only with no media. Tape support is a follow-up.
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
    emu198x_shell::debug_target_hooks!();
}

emu198x_shell::impl_6502_debug_primitives!(ElectronRuntime);
