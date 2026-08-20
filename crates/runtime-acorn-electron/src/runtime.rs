//! Runtime wrapper for the Acorn Electron.
//!
//! Construction needs both 16 KB ROMs (OS + BASIC). The runtime defers
//! the machine until both arrive — via `set_roms` / `from_firmware` /
//! the MCP `load_media` path. Electron audio is the ULA's 1-bit sound
//! generator; the chip crate exposes `take_audio_buffer` so the runtime
//! pushes those samples per frame at 48 kHz.

use common_acorn_cassette::TapePulse;
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_acorn_electron::AcornElectron;

use crate::input::apply_input_event;
use crate::profiles::{BASIC_FIRMWARE_ID, Model, OS_FIRMWARE_ID, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

/// Framebuffer pixels per second.
const PIXEL_CLOCK_HZ: f64 = 16_000_000.0;

const ROM_SIZE: usize = 16 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct ElectronRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<AcornElectron>,
    os_bytes: Option<Vec<u8>>,
    basic_bytes: Option<Vec<u8>>,
    /// The mounted cassette's decoded waveform, kept so it survives a reset's
    /// machine rebuild (the tape stays in the deck across a reset).
    tape_pulses: Option<Vec<TapePulse>>,
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
            tape_pulses: None,
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

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its 6502 / ULA / RAM state.
    /// Sizes `rgba_framebuffer` from the machine's framebuffer dimensions
    /// exactly as `rebuild_machine` does before calling `update_rgba_framebuffer`.
    pub(crate) fn set_machine(&mut self, machine: Option<AcornElectron>) {
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
        let (Some(os), Some(basic)) = (self.os_bytes.clone(), self.basic_bytes.clone()) else {
            self.machine = None;
            return;
        };
        let mut machine = AcornElectron::new(os, basic);
        // Re-mount the cassette so a reset doesn't eject the tape.
        if let Some(pulses) = &self.tape_pulses {
            machine.insert_tape(pulses.clone());
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

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            let slot = image.slot.as_ref();
            match image.kind {
                MediaKind::Tape if slot == "tape-1" => {
                    let tape = format_acorn_uef::parse(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason: reason.to_string(),
                        }
                    })?;
                    if let Some(machine) = self.machine.as_mut() {
                        machine.insert_tape(tape.pulses.clone());
                    }
                    self.tape_pulses = Some(tape.pulses);
                }
                _ => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
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

    /// 16 MHz, as on the BBC — the ULA keeps the same dot rate and the core
    /// scales every mode into one 640-wide buffer.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(self.profile().region, PIXEL_CLOCK_HZ, PIXEL_CLOCK_HZ)
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.machine
            .is_some()
            .then_some(&emu198x_shell::STANDARD_KEYBOARD as &dyn emu198x_shell::KeyboardTarget)
    }
}

emu198x_shell::impl_6502_debug_primitives!(ElectronRuntime);
