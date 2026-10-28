//! Runtime wrapper for the Acorn Electron.
//!
//! Construction needs both 16 KB ROMs (OS + BASIC). The runtime defers
//! the machine until both arrive — via `set_roms` / `from_firmware` /
//! the MCP `load_media` path. Electron audio is the ULA's 1-bit sound
//! generator; the chip crate exposes `take_audio_buffer` so the runtime
//! pushes those samples per frame at 48 kHz.
//!
//! The Plus 3 model also fits the disc expansion. Its ADFS ROM is optional
//! firmware, and its two drives are the `drive-0` and `drive-1` media slots:
//! ADFS `.adf` and `.adl` images load, eject and export as the image they
//! are, with whatever the machine has written.

use common_acorn_cassette::TapePulse;
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, MediaTransportAction,
    PixelFormat, ResetKind, RunResult, StopReason,
};
use machine_acorn_electron::AcornElectron;

use crate::input::apply_input_event;
use crate::profiles::{
    ADFS_FIRMWARE_ID, BASIC_FIRMWARE_ID, DISK_SLOTS, Model, OS_FIRMWARE_ID, profile_for,
};
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
    /// The Plus 3's ADFS ROM, on the Plus 3 model. A Plus 3 without one still
    /// has its drives and controller; nothing pages into ROM 4.
    adfs_bytes: Option<Vec<u8>>,
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
            adfs_bytes: None,
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
        let mut runtime = Self::new(model, os.to_vec(), basic.to_vec())?;
        if model.has_plus3()
            && let Some(adfs) = firmware.bytes(ADFS_FIRMWARE_ID)
        {
            runtime.set_adfs(adfs.to_vec())?;
        }
        Ok(runtime)
    }

    /// Fit the Plus 3's ADFS ROM and rebuild the machine. The discs stay in
    /// their drives.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the ROM is not 16 KB, and
    /// `MachineError::UnsupportedOperation` on a model without a Plus 3.
    pub fn set_adfs(&mut self, adfs_rom: Vec<u8>) -> Result<(), MachineError> {
        if !self.model.has_plus3() {
            return Err(MachineError::UnsupportedOperation {
                operation: "set_adfs",
            });
        }
        if adfs_rom.len() != ROM_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: ADFS_FIRMWARE_ID.to_owned(),
                reason: format!("ADFS ROM is {} bytes; expected {ROM_SIZE}", adfs_rom.len()),
            });
        }
        self.adfs_bytes = Some(adfs_rom);
        self.rebuild_machine();
        Ok(())
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
        if self.model.has_plus3() {
            machine.fit_plus3(self.adfs_bytes.clone().unwrap_or_default());
            // Nor the discs, which keep whatever the machine wrote to them.
            if let Some(old) = self.machine.as_ref() {
                for drive in 0..DISK_SLOTS.len() {
                    if let Some(disk) = old.disk(drive) {
                        machine.insert_disk(drive, disk.clone());
                    }
                }
            }
        }
        let width = machine.framebuffer_width();
        let height = machine.framebuffer_height();
        self.rgba_width = width;
        self.rgba_height = height;
        self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
    }

    /// The Plus 3 drive a disc slot names, on the Plus 3 model.
    fn disk_drive(&self, slot: &str) -> Result<usize, MachineError> {
        DISK_SLOTS
            .iter()
            .position(|&name| name == slot)
            .filter(|_| self.model.has_plus3())
            .ok_or_else(|| MachineError::UnknownMediaSlot {
                slot: slot.to_owned(),
            })
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

impl emu198x_shell::FamilyRuntime for ElectronRuntime {
    type Model = Model;

    fn variant_ids() -> &'static [&'static str] {
        &Model::VARIANT_IDS
    }

    fn model_from_id(id: &str) -> Option<Model> {
        Model::from_variant_id(id)
    }

    fn variant_id(model: Model) -> &'static str {
        model.variant_id()
    }

    fn profile_for(model: Model) -> MachineProfile {
        profile_for(model)
    }

    fn rom_convention() -> emu198x_shell::RomConvention {
        emu198x_shell::RomConvention {
            env_var: Some("EMU198X_ELECTRON_ROM_DIR"),
            dirs: &["acorn-electron"],
        }
    }

    fn firmware_sources(model: Model) -> Vec<emu198x_shell::FirmwareSource> {
        model.firmware_sources()
    }

    fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        Self::from_firmware(model, firmware)
    }

    fn native_frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }
}

impl ElectronRuntime {
    fn run_window(
        &mut self,
        target: MachineTime,
        one_frame: bool,
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
            let (ticks, completed) = if one_frame {
                self.machine
                    .as_mut()
                    .expect("machine checked above")
                    .run_native_frame()
            } else {
                (
                    self.machine
                        .as_mut()
                        .expect("machine checked above")
                        .run_frame(),
                    true,
                )
            };
            self.time = self.time.saturating_add(ticks);
            self.update_rgba_framebuffer();

            if !one_frame || completed {
                host.frame_sink.push_frame(FramePacket {
                    signal: None,
                    timestamp: self.time,
                    format: PixelFormat::Rgba8888,
                    width: self.rgba_width,
                    height: self.rgba_height,
                    palette: None,
                    pixels: &self.rgba_framebuffer,
                })?;
            }

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
            if one_frame {
                return Ok(RunResult::new(
                    self.time,
                    if completed {
                        StopReason::ReachedTarget
                    } else {
                        StopReason::NoFrame
                    },
                ));
            }
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
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
                MediaKind::Disk => {
                    let drive = self.disk_drive(slot)?;
                    let disk = machine_acorn_electron::adfs_disk(image.bytes, image.writable)
                        .map_err(|reason| MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason: reason.to_string(),
                        })?;
                    let Some(machine) = self.machine.as_mut() else {
                        return Err(MachineError::InvalidRequest {
                            reason: "load the OS and BASIC ROMs before a disc".to_owned(),
                        });
                    };
                    machine.insert_disk(drive, disk);
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

    fn eject_media(&mut self, slot: &str) -> Result<(), MachineError> {
        if slot == "tape-1" {
            self.tape_pulses = None;
            if let Some(machine) = self.machine.as_mut() {
                machine.eject_tape();
            }
            return Ok(());
        }
        let drive = self.disk_drive(slot)?;
        if let Some(machine) = self.machine.as_mut() {
            machine.eject_disk(drive);
        }
        Ok(())
    }

    /// A disc exports as the ADFS image it is: `adf` for one side, `adl`
    /// for two (interleaved by track), holding whatever the machine wrote.
    fn export_media(&self, slot: &str, format: &str) -> Result<Vec<u8>, MachineError> {
        let drive = self.disk_drive(slot)?;
        let invalid = |reason: String| MachineError::InvalidMedia {
            slot: slot.to_owned(),
            reason,
        };
        let disk = self
            .machine
            .as_ref()
            .and_then(|machine| machine.disk(drive))
            .ok_or_else(|| invalid("no disc in the drive".to_owned()))?;
        let native = if disk.sides() == 2 { "adl" } else { "adf" };
        if format != native {
            return Err(invalid(format!(
                "this disc exports as {native}, not {format}"
            )));
        }
        Ok(disk.data().to_vec())
    }

    fn run_frame(&mut self, host: &mut HostIo<'_>) -> Result<RunResult, MachineError> {
        self.run_window(MachineTime::new(u64::MAX), true, host)
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        self.run_window(target, false, host)
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        snapshot::encode(self)
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        snapshot::decode(self, bytes)
    }

    /// Start or stop the deck.
    ///
    /// The tape used to advance purely on the guest's motor line, so a script
    /// could not stop it, could not re-position it, and could not inspect a
    /// stalled load without the deck running on underneath (#1198). The host
    /// gate ANDs with the motor line, so a running deck still only moves when
    /// the machine asks.
    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        match command {
            ControlCommand::MediaTransport(cmd) => {
                if cmd.slot.as_ref() != "tape-1" {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: cmd.slot.as_ref().to_owned(),
                    });
                }
                let running = match cmd.action {
                    MediaTransportAction::Start => true,
                    MediaTransportAction::Stop => false,
                    _ => {
                        return Err(MachineError::UnsupportedOperation {
                            operation: "media-transport",
                        });
                    }
                };
                let Some(machine) = self.machine.as_mut() else {
                    return Err(MachineError::InvalidRequest {
                        reason: "no machine is running, so there is no deck to drive".to_owned(),
                    });
                };
                machine.set_deck_running(running);
                Ok(())
            }
            _ => Err(MachineError::UnsupportedOperation {
                operation: command.operation_name(),
            }),
        }
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
            .then_some(&crate::input::ElectronKeyboard as &dyn emu198x_shell::KeyboardTarget)
    }
}

emu198x_shell::impl_6502_debug_primitives!(ElectronRuntime);
