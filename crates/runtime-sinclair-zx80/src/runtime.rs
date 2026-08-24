//! Runtime wrapper for the Sinclair ZX80.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, MediaTransportAction,
    PixelFormat, ResetKind, RunResult, StopReason,
};
use machine_sinclair_zx80::Zx80;

use crate::input::apply_input_event;
use crate::profiles::{Model, ROM_FIRMWARE_ID, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;
use format_sinclair_zx80_o::Zx80Image;

/// Framebuffer pixels per second: two per 3.25 MHz T-state.
const PIXEL_CLOCK_HZ: f64 = 6_500_000.0;

const ROM_SIZE: usize = 4 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;
const TAPE_SLOT: &str = "tape-1";

pub struct Zx80Runtime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Zx80>,
    rom_bytes: Option<Vec<u8>>,
    ram_bytes: usize,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    /// The cassette sitting in the deck, encoded but not playing. Threading
    /// it onto the machine is `MediaTransportAction::Start`'s job, not
    /// `load_media`'s — see [`Zx80Runtime::load_media`].
    tape: Option<Vec<u64>>,
}

impl Zx80Runtime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            rom_bytes: None,
            // The profile decides: selecting the RAM-pack model is what
            // asks for 16 KB. `set_ram_bytes` still overrides for anything
            // in between.
            ram_bytes: model.ram_bytes(),
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            tape: None,
        }
    }

    /// Build directly from an explicit 4 KB ROM.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the ROM is not 4 KB.
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
    pub fn machine(&self) -> Option<&Zx80> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Zx80> {
        self.machine.as_mut()
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
    /// Mirrors `rebuild_machine`'s framebuffer sizing: the ZX80 framebuffer is
    /// sized from the machine's width/height getters, so do the same here.
    pub(crate) fn set_machine(&mut self, machine: Option<Zx80>) {
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
            Zx80::new(rom, self.ram_bytes).map_err(|reason| MachineError::InvalidFirmware {
                id: ROM_FIRMWARE_ID.to_owned(),
                reason,
            })?;
        // The board strap is the whole of the region difference: the ROM reads
        // D6 and pads a shorter field for 60 Hz. Set it before the height is
        // read, because the window is sized from it (#1133).
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

impl MachineCore for Zx80Runtime {
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

    /// Puts a cassette in the deck. It does not press PLAY.
    ///
    /// Loading and playing are separate on purpose, because the ZX80's
    /// loader makes them separate in practice. `$0207` will not start
    /// decoding until the line has been quiet for a `$5712` countdown, and
    /// **any** high resets it. The encoder's lead-in supplies exactly that
    /// quiet run — so it has to arrive *after* the user has typed `LOAD`
    /// (the `W` key). A tape threaded at load time spends its lead-in during
    /// boot and typing, and its data pulses then land inside the leader
    /// search, resetting the countdown until the tape runs out. The loader
    /// waits forever for a signal that has already gone.
    ///
    /// Press PLAY with `MediaTransportAction::Start` once `LOAD` is typed.
    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            if image.kind != MediaKind::Tape {
                return Err(MachineError::UnknownMediaSlot {
                    slot: image.slot.as_ref().to_owned(),
                });
            }
            let parsed =
                Zx80Image::parse(image.bytes).map_err(|error| MachineError::InvalidMedia {
                    slot: image.slot.as_ref().to_owned(),
                    reason: error.to_string(),
                })?;
            self.tape = Some(parsed.to_pulses());
        }
        Ok(())
    }

    /// Presses PLAY or STOP on the cassette deck.
    ///
    /// `Start` threads the loaded tape onto the machine from its beginning,
    /// lead-in first; `Stop` lifts it off. Rewinding on every press is what
    /// a `.o` deck does — the image is one program, not a position on a
    /// longer tape.
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
            MediaTransportAction::Stop => machine.insert_tape(&[]),
            _ => {
                return Err(MachineError::UnsupportedOperation {
                    operation: command.operation_name(),
                });
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

    /// Two pixels per 3.25 MHz T-state, filling PAL's 288 active lines once.
    /// Works out at about 1.14: the raster puts 256 pixels of characters
    /// across roughly three quarters of a set's width but only 192 lines down
    /// two thirds of its height, so the pixels are wider than they are tall.
    fn display(&self) -> Option<Display> {
        Some(Display::Television {
            region: self.profile.region,
            pixel_clock_hz: PIXEL_CLOCK_HZ,
            // Follows the region rather than being PAL for everything, which
            // is what let a USA board go unrepresentable (#1133).
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
            .then_some(&emu198x_shell::STANDARD_KEYBOARD as &dyn emu198x_shell::KeyboardTarget)
    }
}

emu198x_shell::impl_z80_debug_primitives!(Zx80Runtime);
