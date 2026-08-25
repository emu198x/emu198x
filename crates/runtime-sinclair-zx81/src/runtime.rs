//! Runtime wrapper for the Sinclair ZX81.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, MediaTransportAction,
    PixelFormat, ResetKind, RunResult, StopReason,
};
use format_sinclair_zx81_p::Zx81Image;
use machine_sinclair_zx81::Zx81;

use crate::input::apply_input_event;
use crate::profiles::{Model, ROM_FIRMWARE_ID, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

/// Framebuffer pixels per second: two per 3.25 MHz T-state.
const PIXEL_CLOCK_HZ: f64 = 6_500_000.0;

const ROM_SIZE: usize = 8 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;
/// The tape slot this runtime answers for, matching the profile.
const TAPE_SLOT: &str = "tape-1";

/// ZX81 character code for `A`, used as the name in a generated waveform.
const TAPE_NAME: u8 = 0x26;

/// This machine's keyboard for the shared `press_key` / `type_string` tools:
/// the standard layout, backed by this machine's own key-name resolver so a
/// character it cannot type is refused rather than silently dropped (#1196).
static KEYBOARD: emu198x_shell::StandardKeyboard = emu198x_shell::StandardKeyboard::new(
    emu198x_shell::STANDARD_KEY_TIMING,
    crate::input::knows_key_name,
);

pub struct Zx81Runtime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Zx81>,
    rom_bytes: Option<Vec<u8>>,
    ram_bytes: usize,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    /// The threaded tape's waveform, waiting for PLAY.
    tape: Option<Vec<u64>>,
}

impl Zx81Runtime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            rom_bytes: None,
            ram_bytes: model.ram_bytes(),
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            tape: None,
        }
    }

    /// Build directly from an explicit 8 KB ROM.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the ROM is not 8 KB.
    pub fn new(model: Model, rom: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_rom(rom)?;
        Ok(runtime)
    }

    /// Build from a profile firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or the ROM is missing.
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

    /// Replace the ROM and rebuild.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the size is wrong.
    pub fn set_rom(&mut self, rom: Vec<u8>) -> Result<(), MachineError> {
        if rom.len() != ROM_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: ROM_FIRMWARE_ID.to_owned(),
                reason: format!("ROM is {} bytes; expected {ROM_SIZE}", rom.len()),
            });
        }
        self.rom_bytes = Some(rom);
        self.rebuild_machine()
    }

    /// Set RAM size (must be a power of two ≤ 16384).
    ///
    /// # Errors
    ///
    /// Returns an error if the size is invalid or the machine refuses it.
    pub fn set_ram_bytes(&mut self, ram_bytes: usize) -> Result<(), MachineError> {
        self.ram_bytes = ram_bytes;
        self.rebuild_machine()
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Zx81> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Zx81> {
        self.machine.as_mut()
    }

    /// The loaded ROM, if one has been set. Lets a caller rebuild on another
    /// board strap without going back to disk — both boards run the same
    /// monitor.
    #[must_use]
    pub fn rom_bytes(&self) -> Option<&[u8]> {
        self.rom_bytes.as_deref()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn ram_bytes(&self) -> usize {
        self.ram_bytes
    }

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its CPU/ULA/RAM state.
    ///
    /// Mirrors `rebuild_machine`'s framebuffer sizing: the ZX81 framebuffer is
    /// sized from the machine's width/height getters, so do the same here.
    pub(crate) fn set_machine(&mut self, machine: Option<Zx81>) {
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

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        let Some(rom) = self.rom_bytes.clone() else {
            self.machine = None;
            return Ok(());
        };
        let mut machine =
            Zx81::new(rom, self.ram_bytes).map_err(|reason| MachineError::InvalidFirmware {
                id: ROM_FIRMWARE_ID.to_owned(),
                reason,
            })?;
        // The board strap is the whole of the region difference: the ROM reads
        // it on port bit 6 and lays out a shorter field for 60 Hz.
        machine.set_television_standard(self.model.television_standard());
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

impl MachineCore for Zx81Runtime {
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

    /// Threads a `.p` image onto the deck. It does not start playing: the
    /// ROM has to be sitting in `LOAD` first, so PLAY is a separate press.
    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            if image.kind != MediaKind::Tape {
                return Err(MachineError::UnknownMediaSlot {
                    slot: image.slot.as_ref().to_owned(),
                });
            }
            let parsed =
                Zx81Image::parse(image.bytes).map_err(|error| MachineError::InvalidMedia {
                    slot: image.slot.as_ref().to_owned(),
                    reason: error.to_string(),
                })?;
            // A one-character name. `LOAD ""` takes whatever it finds, but the
            // stream still needs a name for the ROM to read past.
            self.tape = Some(parsed.to_pulses(&[TAPE_NAME]));
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

            // The ZX81 has no sound chip. What a host hears is the ULA pin
            // that carries both the video signal and the cassette output, so
            // the machine generates it and this drains it (#303).
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

    /// Presses PLAY or STOP on the cassette deck.
    ///
    /// `Start` threads the loaded waveform on from its beginning, lead-in
    /// first. Rewinding on every press is what a `.p` deck does -- the image
    /// is one program, not a position on a longer tape.
    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        let ControlCommand::MediaTransport(transport) = command else {
            return Err(MachineError::UnsupportedOperation {
                operation: command.operation_name(),
            });
        };
        if transport.slot.as_ref() != TAPE_SLOT {
            return Err(MachineError::UnknownMediaSlot {
                slot: transport.slot.as_ref().to_owned(),
            });
        }
        let machine = self
            .machine
            .as_mut()
            .ok_or_else(|| MachineError::MissingFirmware {
                id: ROM_FIRMWARE_ID.to_owned(),
            })?;
        match transport.action {
            MediaTransportAction::Start => {
                let pulses =
                    self.tape
                        .as_deref()
                        .ok_or_else(|| MachineError::UnknownMediaSlot {
                            slot: TAPE_SLOT.to_owned(),
                        })?;
                machine.insert_tape(pulses);
            }
            MediaTransportAction::Stop => machine.eject_tape(),
            _ => {
                return Err(MachineError::UnsupportedOperation {
                    operation: command.operation_name(),
                });
            }
        }
        Ok(())
    }

    /// Two pixels per 3.25 MHz T-state, over the set's active lines.
    ///
    /// The line count has to follow the region rather than be written here:
    /// a 60 Hz board is scanning an NTSC set with 240 active lines, not 288,
    /// and hardcoding PAL made the 60 Hz variant claim a PAL raster while
    /// reporting an NTSC region. `active_lines` exists to stop exactly that,
    /// and the mistake is silent -- the picture is merely the wrong shape.
    fn display(&self) -> Option<Display> {
        Some(Display::Television {
            region: self.profile.region,
            pixel_clock_hz: PIXEL_CLOCK_HZ,
            lines_per_tv_height: emu198x_shell::display::active_lines(self.profile.region)?,
        })
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.machine
            .is_some()
            .then_some(&KEYBOARD as &dyn emu198x_shell::KeyboardTarget)
    }
}

emu198x_shell::impl_z80_debug_primitives!(Zx81Runtime);
