//! Runtime wrapper for the Commodore PET.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_commodore_pet::Pet;

use crate::input::apply_input_event;
use crate::profiles::{
    BASIC_FIRMWARE_ID, CHAR_FIRMWARE_ID, EDITOR_FIRMWARE_ID, KERNAL_FIRMWARE_ID, Model, profile_for,
};
use crate::snapshot;
use emu198x_shell::display::Display;

const KERNAL_SIZE: usize = 4 * 1024;
const BASIC_SIZE: usize = 8 * 1024;
const EDITOR_SIZE: usize = 2 * 1024;
const CHAR_SIZE: usize = 4 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Frames of boot before a queued PRG is injected. The KERNAL reaches a usable
/// editor well inside this; the machine crate's keyboard tests use the same
/// 120-frame budget before typing.
const PRG_AUTOLOAD_FRAME: u64 = 120;

// BASIC's zero-page pointers on the Rev 3 (BASIC 2.0/4.0) ROMs this runtime
// models. They are *not* the C64/VIC-20 addresses — those sit two bytes further
// on — and the keyboard buffer moved too: $0277 is the C64's buffer but the
// ninth byte of the PET's.
//
// Source: Commodore, *PET/CBM Personal Computer Guide* (1980), Table 6-2 "PET
// Memory Map (Rev. 3 ROMs)", distilled in
// reference/by-system/commodore-pet/pet-reference.md.
/// Start of BASIC text; the user program area begins at $0401.
const BASIC_TXTTAB: u16 = 0x0028;
/// Start of variables, end of variables, end of arrays — all set just past a
/// loaded program so RUN and variable allocation agree about where it ends.
const BASIC_VARTAB: u16 = 0x002A;
const BASIC_ARYTAB: u16 = 0x002C;
const BASIC_STREND: u16 = 0x002E;
/// Keyboard buffer (10 bytes) and the count of characters in it.
const KEYBOARD_BUFFER: u16 = 0x026F;
const KEYBOARD_COUNT: u16 = 0x009E;
/// The buffer holds ten characters; a longer launch command would wrap.
const KEYBOARD_BUFFER_LEN: usize = 10;

/// This machine's keyboard for the shared `press_key` / `type_string` tools:
/// the standard layout, backed by this machine's own key-name resolver so a
/// character it cannot type is refused rather than silently dropped (#1196).
static KEYBOARD: emu198x_shell::StandardKeyboard = emu198x_shell::StandardKeyboard::new(
    emu198x_shell::STANDARD_KEY_TIMING,
    crate::input::knows_key_name,
);

pub struct PetRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Pet>,
    kernal_bytes: Option<Vec<u8>>,
    basic_bytes: Option<Vec<u8>>,
    editor_bytes: Option<Vec<u8>>,
    char_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    /// PRG image waiting for the machine to finish booting.
    pending_prg: Option<Vec<u8>>,
}

impl PetRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            kernal_bytes: None,
            basic_bytes: None,
            editor_bytes: None,
            char_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            pending_prg: None,
        }
    }

    /// Build from explicit ROMs.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if any size is wrong.
    pub fn new(
        model: Model,
        kernal: Vec<u8>,
        basic: Vec<u8>,
        editor: Vec<u8>,
        char_rom: Vec<u8>,
    ) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_roms(kernal, basic, editor, char_rom)?;
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
        let editor =
            firmware
                .bytes(EDITOR_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: EDITOR_FIRMWARE_ID.to_owned(),
                })?;
        let char_rom =
            firmware
                .bytes(CHAR_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: CHAR_FIRMWARE_ID.to_owned(),
                })?;
        Self::new(
            model,
            kernal.to_vec(),
            basic.to_vec(),
            editor.to_vec(),
            char_rom.to_vec(),
        )
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
        editor: Vec<u8>,
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
        if editor.len() != EDITOR_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: EDITOR_FIRMWARE_ID.to_owned(),
                reason: format!("editor is {} bytes; expected {EDITOR_SIZE}", editor.len()),
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
        self.editor_bytes = Some(editor);
        self.char_bytes = Some(char_rom);
        self.rebuild_machine();
        Ok(())
    }

    /// Inject a `.prg` image into RAM and queue `RUN` so it starts itself.
    ///
    /// The first two bytes are the little-endian load address; the rest is
    /// copied there. BASIC's start-of-variables, end-of-variables and
    /// end-of-arrays pointers are then set just past the program, and `RUN` is
    /// placed in the KERNAL's keyboard buffer so the editor runs it on reaching
    /// READY.
    ///
    /// The machine must already be booted to READY. A program loading at
    /// `$0401` — the ordinary case, since that is where PET BASIC text starts —
    /// also has TXTTAB pointed at it, so a PRG saved from a machine with a
    /// different bottom of memory still runs.
    ///
    /// # Errors
    ///
    /// Returns an error when no machine is loaded, the image is too short, or
    /// the launch command would not fit the ten-character keyboard buffer.
    pub fn autoload_prg(&mut self, bytes: &[u8]) -> Result<(), String> {
        let machine = self.machine.as_mut().ok_or("PET not initialised")?;
        if bytes.len() < 3 {
            return Err("PRG image too short".into());
        }
        let load = u16::from(bytes[0]) | (u16::from(bytes[1]) << 8);
        for (i, &byte) in bytes[2..].iter().enumerate() {
            let offset = u16::try_from(i).map_err(|_| "PRG larger than 64 KB")?;
            machine.poke(load.wrapping_add(offset), byte);
        }
        let body = u16::try_from(bytes.len() - 2).map_err(|_| "PRG larger than 64 KB")?;
        let end = load.wrapping_add(body);

        let [lo, hi] = load.to_le_bytes();
        machine.poke(BASIC_TXTTAB, lo);
        machine.poke(BASIC_TXTTAB + 1, hi);
        let [lo, hi] = end.to_le_bytes();
        for base in [BASIC_VARTAB, BASIC_ARYTAB, BASIC_STREND] {
            machine.poke(base, lo);
            machine.poke(base + 1, hi);
        }

        let command = b"RUN\r";
        if command.len() > KEYBOARD_BUFFER_LEN {
            return Err("launch command longer than the keyboard buffer".into());
        }
        for (i, &byte) in command.iter().enumerate() {
            let offset = u16::try_from(i).map_err(|_| "launch command too long")?;
            machine.poke(KEYBOARD_BUFFER + offset, byte);
        }
        let count = u8::try_from(command.len()).map_err(|_| "launch command too long")?;
        machine.poke(KEYBOARD_COUNT, count);
        Ok(())
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Pet> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Pet> {
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
    /// restore path so the resumed machine keeps its 6502/CRTC/PIA/VIA/RAM
    /// state. Mirrors `rebuild_machine`'s rgba-buffer sizing exactly so
    /// `update_rgba_framebuffer` cannot index out of bounds.
    pub(crate) fn set_machine(&mut self, machine: Option<Pet>) {
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
        let (Some(kernal), Some(basic), Some(editor), Some(char_rom)) = (
            self.kernal_bytes.clone(),
            self.basic_bytes.clone(),
            self.editor_bytes.clone(),
            self.char_bytes.clone(),
        ) else {
            self.machine = None;
            return;
        };
        let machine = Pet::new(kernal, basic, editor, char_rom, self.model.screen_chars());
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

impl MachineCore for PetRuntime {
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
                MediaKind::Program if slot == "program-1" => {
                    if image.bytes.len() < 3 {
                        return Err(MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason: "PRG image too short".to_owned(),
                        });
                    }
                    self.pending_prg = Some(image.bytes.to_vec());
                }
                MediaKind::Program => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                kind => return Err(MachineError::UnsupportedMediaKind { kind }),
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
            let machine = self.machine.as_mut().expect("machine checked above");
            let ticks = machine.run_frame();
            let inject_prg =
                self.pending_prg.is_some() && machine.frame_count() >= PRG_AUTOLOAD_FRAME;
            self.time = self.time.saturating_add(ticks);
            if inject_prg {
                let bytes = self.pending_prg.take().expect("checked above");
                self.autoload_prg(&bytes)
                    .map_err(|reason| MachineError::InvalidMedia {
                        slot: "program-1".to_owned(),
                        reason,
                    })?;
            }
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

    fn display(&self) -> Option<Display> {
        Some(Display::Monitor { aspect: 4.0 / 3.0 })
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

emu198x_shell::impl_6502_debug_primitives!(PetRuntime);
