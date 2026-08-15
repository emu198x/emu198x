//! Runtime wrapper for the Amstrad CPC.

use common_tape::TapeSpan;
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_amstrad_cpc::AmstradCpc;

use crate::input::apply_input_event;
use crate::profiles::{Model, ROM_FIRMWARE_ID, profile_for};
use crate::snapshot;

/// 16 KB of OS plus 16 KB of BASIC, the layout of MAME's `cpc464.rom`.
const FIRMWARE_SIZE: usize = 32 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct AmstradCpcRuntime {
    profile: MachineProfile,
    model: Model,
    pub(crate) machine: Option<AmstradCpc>,
    firmware_bytes: Option<Vec<u8>>,
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
        let bytes =
            firmware
                .bytes(ROM_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: ROM_FIRMWARE_ID.to_owned(),
                })?;
        Self::new(model, bytes.to_vec())
    }

    /// Replace the firmware and rebuild.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` unless the image is 32 KB.
    pub fn set_firmware(&mut self, firmware: Vec<u8>) -> Result<(), MachineError> {
        if firmware.len() != FIRMWARE_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: ROM_FIRMWARE_ID.to_owned(),
                reason: format!(
                    "CPC464 firmware is {} bytes; expected {FIRMWARE_SIZE} (16 KB OS + 16 KB BASIC)",
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
            AmstradCpc::new(&firmware).map_err(|reason| MachineError::InvalidFirmware {
                id: ROM_FIRMWARE_ID.to_owned(),
                reason,
            })?;
        // A reset does not eject the cassette.
        if let Some(spans) = self.tape_spans.clone() {
            machine.insert_tape(spans);
        }
        self.size_framebuffer_for(&machine);
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
        Ok(())
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
    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.cpc_keyboard()
    }
}
