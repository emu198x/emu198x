//! Runtime wrapper for the Amstrad CPC.

use crate::machine::AmstradCpc;
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use emu198x_tape_playback::TapeSpan;

use crate::input::apply_input_event;
use crate::profiles::{AMSDOS_FIRMWARE_ID, Model, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

/// Framebuffer pixels per second.
const PIXEL_CLOCK_HZ: f64 = 16_000_000.0;

/// 16 KB of OS plus 16 KB of BASIC, the layout of MAME's `cpc464.rom`.
const FIRMWARE_SIZE: usize = 32 * 1024;
/// AMSDOS: one 16 KB upper ROM.
const AMSDOS_SIZE: usize = 16 * 1024;
/// The upper ROM number AMSDOS answers to.
const AMSDOS_ROM_NUMBER: u8 = 7;
/// The disc slots, by drive.
const DISK_SLOTS: [&str; 2] = ["disk-a", "disk-b"];
const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct AmstradCpcRuntime {
    profile: MachineProfile,
    model: Model,
    pub(crate) machine: Option<AmstradCpc>,
    firmware_bytes: Option<Vec<u8>>,
    /// AMSDOS, fitted as upper ROM 7 on a model with a disc.
    amsdos_bytes: Option<Vec<u8>>,
    /// Kept so a reset re-inserts the tape rather than ejecting it — a reset
    /// on the real machine does not empty the cassette deck.
    tape_spans: Option<Vec<TapeSpan>>,
    pub(crate) time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
}

impl AmstradCpcRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            firmware_bytes: None,
            amsdos_bytes: None,
            tape_spans: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
        }
    }

    /// Build from an explicit 32 KB firmware image.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` unless the image is 32 KB.
    pub fn new(model: Model, firmware: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_firmware(firmware)?;
        Ok(runtime)
    }

    /// Build from a firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or the firmware is missing.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let required = |id: &str| {
            firmware
                .bytes(id)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| MachineError::MissingFirmware { id: id.to_owned() })
        };
        let mut runtime = Self::blank(model);
        if model.has_disc() {
            runtime.amsdos_bytes = Some(Self::checked_amsdos(required(AMSDOS_FIRMWARE_ID)?)?);
        }
        runtime.set_firmware(required(model.firmware_id())?)?;
        Ok(runtime)
    }

    fn checked_amsdos(amsdos: Vec<u8>) -> Result<Vec<u8>, MachineError> {
        if amsdos.len() == AMSDOS_SIZE {
            Ok(amsdos)
        } else {
            Err(MachineError::InvalidFirmware {
                id: AMSDOS_FIRMWARE_ID.to_owned(),
                reason: format!("AMSDOS is {} bytes; expected {AMSDOS_SIZE}", amsdos.len()),
            })
        }
    }

    /// Fit AMSDOS, upper ROM 7, and rebuild. A model without a disc ignores
    /// it, as a 464 has nothing for it to drive.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` unless the image is 16 KB.
    pub fn set_amsdos(&mut self, amsdos: Vec<u8>) -> Result<(), MachineError> {
        self.amsdos_bytes = Some(Self::checked_amsdos(amsdos)?);
        self.rebuild_machine()
    }

    /// Replace the firmware and rebuild.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` unless the image is 32 KB.
    pub fn set_firmware(&mut self, firmware: Vec<u8>) -> Result<(), MachineError> {
        if firmware.len() != FIRMWARE_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: self.model.firmware_id().to_owned(),
                reason: format!(
                    "CPC firmware is {} bytes; expected {FIRMWARE_SIZE} (16 KB OS + 16 KB BASIC)",
                    firmware.len()
                ),
            });
        }
        self.firmware_bytes = Some(firmware);
        self.rebuild_machine()
    }

    #[must_use]
    pub fn machine(&self) -> Option<&AmstradCpc> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut AmstradCpc> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    /// Install a machine restored from a snapshot, sizing the host RGBA buffer
    /// from it before repainting — `blank()` starts with an empty buffer, so
    /// skipping this would panic on the first repaint.
    pub(crate) fn set_machine(&mut self, machine: Option<AmstradCpc>) {
        if let Some(machine) = &machine {
            // Reset must use the restored hardware, even in a blank runtime or
            // when the previous session had different firmware and media.
            self.firmware_bytes = Some(machine.firmware());
            self.amsdos_bytes = machine.upper_roms().get(&AMSDOS_ROM_NUMBER).cloned();
            self.tape_spans = Some(machine.tape().spans().to_vec());
            self.size_framebuffer_for(machine);
        }
        self.machine = machine;
        self.update_rgba_framebuffer();
    }

    fn size_framebuffer_for(&mut self, machine: &AmstradCpc) {
        let width = machine.framebuffer_width();
        let height = machine.framebuffer_height();
        self.rgba_width = width;
        self.rgba_height = height;
        self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        let Some(firmware) = self.firmware_bytes.clone() else {
            self.machine = None;
            return Ok(());
        };
        let mut machine =
            AmstradCpc::with_model(&firmware, self.model.machine_model()).map_err(|reason| {
                MachineError::InvalidFirmware {
                    id: self.model.firmware_id().to_owned(),
                    reason,
                }
            })?;
        if let Some(old) = self.machine.as_ref() {
            for (&number, image) in old.upper_roms() {
                machine.fit_upper_rom(number, image).map_err(|reason| {
                    MachineError::InvalidFirmware {
                        id: format!("upper-rom-{number}"),
                        reason,
                    }
                })?;
            }
        }
        if let (true, Some(amsdos)) = (self.model.has_disc(), self.amsdos_bytes.as_ref()) {
            machine
                .fit_upper_rom(AMSDOS_ROM_NUMBER, amsdos)
                .map_err(|reason| MachineError::InvalidFirmware {
                    id: AMSDOS_FIRMWARE_ID.to_owned(),
                    reason,
                })?;
        }
        // A reset does not eject the cassette, or the discs, which keep
        // whatever the machine wrote to them.
        if let Some(spans) = self.tape_spans.clone() {
            machine.insert_tape(spans);
        }
        if let Some(old) = self.machine.as_ref() {
            for drive in 0..DISK_SLOTS.len() {
                if let Some(disk) = old.disk(drive) {
                    let writable = !old.fdc().is_write_protected(drive);
                    machine.insert_disk(drive, disk.clone(), writable);
                }
            }
        }
        self.size_framebuffer_for(&machine);
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
        Ok(())
    }

    /// The drive a disc slot names, on a model that has a drive.
    fn disk_drive(&self, slot: &str) -> Result<usize, MachineError> {
        DISK_SLOTS
            .iter()
            .position(|&name| name == slot)
            .filter(|_| self.model.has_disc())
            .ok_or_else(|| MachineError::UnknownMediaSlot {
                slot: slot.to_owned(),
            })
    }

    pub(crate) fn update_rgba_framebuffer(&mut self) {
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

impl emu198x_shell::FamilyRuntime for AmstradCpcRuntime {
    type Model = Model;

    fn variant_ids() -> &'static [&'static str] {
        &Model::VARIANT_IDS
    }

    fn model_from_id(id: &str) -> Option<Model> {
        Model::from_variant_id(id)
    }

    fn variant_id(model: Model) -> &'static str {
        model.profile_id()
    }

    fn profile_for(model: Model) -> MachineProfile {
        profile_for(model)
    }

    fn rom_convention() -> emu198x_shell::RomConvention {
        emu198x_shell::RomConvention {
            env_var: Some("EMU198X_CPC_ROM_DIR"),
            dirs: &["amstrad-cpc"],
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

impl AmstradCpcRuntime {
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

impl MachineCore for AmstradCpcRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }
    fn time(&self) -> MachineTime {
        self.time
    }
    fn reset(&mut self, _kind: ResetKind) {
        // The firmware was accepted when it was set, so a rebuild cannot fail
        // for a reason a reset could report; drop the machine if it somehow
        // does rather than leave a stale one running.
        if self.rebuild_machine().is_err() {
            self.machine = None;
        }
        self.time = MachineTime::default();
    }
    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            let slot = image.slot.as_ref();
            match image.kind {
                MediaKind::Tape if slot == "tape-1" => {
                    // Already scaled from the TZX reference clock to the CPC's
                    // 4 MHz by the parser, so these spans are in the same
                    // T-states the machine counts.
                    let spans =
                        format_amstrad_cpc_cdt::cdt_to_stream(image.bytes).map_err(|reason| {
                            MachineError::InvalidMedia {
                                slot: slot.to_owned(),
                                reason,
                            }
                        })?;
                    if let Some(machine) = self.machine.as_mut() {
                        machine.insert_tape(spans.clone());
                    }
                    self.tape_spans = Some(spans);
                }
                MediaKind::Disk => {
                    let drive = self.disk_drive(slot)?;
                    let disk = format_amstrad_dsk::parse(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason,
                        }
                    })?;
                    if let Some(machine) = self.machine.as_mut() {
                        machine.insert_disk(drive, disk, image.writable);
                    }
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
        let drive = self.disk_drive(slot)?;
        if let Some(machine) = self.machine.as_mut() {
            machine.eject_disk(drive);
        }
        Ok(())
    }
    fn export_media(&self, slot: &str, format: &str) -> Result<Vec<u8>, MachineError> {
        let drive = self.disk_drive(slot)?;
        let invalid = |reason: String| MachineError::InvalidMedia {
            slot: slot.to_owned(),
            reason,
        };
        if !matches!(format, "dsk" | "edsk") {
            return Err(invalid(format!(
                "cannot export a disc as {format}; use edsk"
            )));
        }
        let disk = self
            .machine
            .as_ref()
            .and_then(|machine| machine.disk(drive))
            .ok_or_else(|| invalid("no disc in the drive".to_owned()))?;
        format_amstrad_dsk::encode_extended(disk).map_err(invalid)
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
    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: command.operation_name(),
        })
    }
    /// Sixteen dots per character over sixty-four characters is 1024 dots a
    /// line, which at 16 MHz is PAL's 64 µs. That the existing comment here
    /// already named the same clock is corroboration, not coincidence.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(self.profile().region, PIXEL_CLOCK_HZ, PIXEL_CLOCK_HZ)
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.cpc_keyboard()
    }
}
