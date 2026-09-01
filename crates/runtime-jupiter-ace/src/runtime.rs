//! Runtime wrapper for the Jupiter Ace.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_jupiter_ace::JupiterAce;

use crate::input::apply_input_event;
use crate::profiles::{BIOS_FIRMWARE_ID, Model, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

/// Framebuffer pixels per second.
const PIXEL_CLOCK_HZ: f64 = 6_500_000.0;

const BIOS_SIZE: usize = 8 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// This machine's keyboard for the shared `press_key` / `type_string` tools:
/// the standard layout, backed by this machine's own key-name resolver so a
/// character it cannot type is refused rather than silently dropped (#1196).
/// Characters the Ace puts on a red Symbol Shift legend, paired with the
/// keycap that carries them. The modifier is Symbol Shift, not Caps Shift —
/// on this keyboard Caps Shift only selects upper case, so every punctuation
/// mark lives here. Without the table `type_string` could type letters,
/// digits and space and nothing else (#1206), which on a Forth machine means
/// it could not enter a single word definition.
///
/// Measured against the real ROM: Symbol Shift chorded with every key, the
/// echoed character read out of the Ace's character RAM. Every entry agrees
/// with MAME's `cantab/jupace.cpp` third `PORT_CHAR`.
///
/// `q`, `w` and `e` carry no red legend — the ROM falls through and returns
/// the capital letter — so they are absent here rather than mapped to
/// something they do not produce. `©` (Symbol Shift + `i`) is absent too:
/// the cell is right and MAME names the glyph, but nothing in this tree
/// verifies what the Ace's `$7F` actually draws, so it stays out until it
/// can be checked.
const SYMBOL_LEGENDS: &[(char, &str)] = &[
    ('!', "1"),
    ('@', "2"),
    ('#', "3"),
    ('$', "4"),
    ('%', "5"),
    ('&', "6"),
    ('\'', "7"),
    ('(', "8"),
    (')', "9"),
    ('_', "0"),
    ('~', "a"),
    ('*', "b"),
    ('?', "c"),
    ('\\', "d"),
    ('{', "f"),
    ('}', "g"),
    ('^', "h"),
    ('-', "j"),
    ('+', "k"),
    ('=', "l"),
    ('.', "m"),
    (',', "n"),
    (';', "o"),
    ('"', "p"),
    ('<', "r"),
    ('|', "s"),
    ('>', "t"),
    (']', "u"),
    ('/', "v"),
    ('£', "x"),
    ('[', "y"),
    (':', "z"),
];

/// This machine's keyboard for the shared `press_key` / `type_string` tools:
/// the standard layout, backed by this machine's own key-name resolver so a
/// character it cannot type is refused rather than silently dropped (#1196).
static KEYBOARD: emu198x_shell::StandardKeyboard = emu198x_shell::StandardKeyboard::with_legends(
    emu198x_shell::STANDARD_KEY_TIMING,
    crate::input::knows_key_name,
    "symbol",
    SYMBOL_LEGENDS,
);

pub struct JupiterAceRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<JupiterAce>,
    bios_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
}

impl JupiterAceRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            bios_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
        }
    }

    /// Build directly from an 8 KB Forth ROM.
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
        self.rebuild_machine()
    }

    #[must_use]
    pub fn machine(&self) -> Option<&JupiterAce> {
        self.machine.as_ref()
    }

    /// Restore an ACE32 `.ace` snapshot into the live machine.
    ///
    /// The image is written from `$2000` in address order, which is what makes
    /// the container's redundancy work for us rather than against us: the file
    /// does not reproduce the Ace's address aliases, so the ACE32 configuration
    /// block it keeps at `$2000-$23FF` is overwritten when the real video RAM at
    /// `$2400` is written, the `$2800` block is overwritten by `$2C00`, and the
    /// `$3000`/`$3400`/`$3800` mirrors collapse onto `$3C00`. See
    /// `reference/by-system/jupiter-ace/jupiter-ace-ace-snapshot-format.md` §5.
    ///
    /// Registers are restored last, so the machine resumes where the snapshot
    /// was taken rather than restarting.
    ///
    /// # Errors
    ///
    /// Returns an error when no machine is loaded, or the image is not a
    /// well-formed `.ace`.
    pub fn load_ace_snapshot(&mut self, bytes: &[u8]) -> Result<(), String> {
        let snapshot = format_jupiter_ace_ace::Snapshot::parse(bytes)?;
        // A snapshot taken on a larger machine would have its tail silently
        // dropped by the memory map, leaving a half-loaded program that looks
        // like a bad dump. Refuse it and say which machine it wants.
        let top = snapshot.top_address();
        let fitted = match self.model.expansion_ram_kb() {
            0 => 0x3FFF,
            16 => 0x7FFF,
            _ => 0xBFFF,
        };
        if top > fitted {
            return Err(format!(
                "snapshot covers $2000-${top:04X} but this machine has RAM to ${fitted:04X}; \
                 it needs the {} KB model",
                if top > 0x7FFF { 48 } else { 16 }
            ));
        }
        let machine = self.machine.as_mut().ok_or("Jupiter Ace not initialised")?;
        for (offset, &byte) in snapshot.memory.iter().enumerate() {
            let addr = u16::try_from(offset)
                .ok()
                .and_then(|o| format_jupiter_ace_ace::LOAD_ADDRESS.checked_add(o))
                .ok_or("snapshot runs past the top of memory")?;
            machine.poke(addr, byte);
        }
        let regs = &mut machine.cpu_mut().regs;
        let r = snapshot.registers;
        regs.af = r.af;
        regs.bc = r.bc;
        regs.de = r.de;
        regs.hl = r.hl;
        regs.ix = r.ix;
        regs.iy = r.iy;
        regs.sp = r.sp;
        regs.pc = r.pc;
        regs.af_alt = r.af_alt;
        regs.bc_alt = r.bc_alt;
        regs.de_alt = r.de_alt;
        regs.hl_alt = r.hl_alt;
        self.update_rgba_framebuffer();
        Ok(())
    }

    pub fn machine_mut(&mut self) -> Option<&mut JupiterAce> {
        self.machine.as_mut()
    }

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its CPU/RAM/display state.
    pub(crate) fn set_machine(&mut self, machine: Option<JupiterAce>) {
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

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        let Some(bios) = self.bios_bytes.clone() else {
            self.machine = None;
            return Ok(());
        };
        let machine =
            JupiterAce::new(bios, self.model.expansion_ram_kb() * 1024).map_err(|reason| {
                MachineError::InvalidFirmware {
                    id: BIOS_FIRMWARE_ID.to_owned(),
                    reason,
                }
            })?;
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

impl MachineCore for JupiterAceRuntime {
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
    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            let slot = image.slot.as_ref();
            match image.kind {
                MediaKind::Snapshot if slot == "snapshot-1" => {
                    self.load_ace_snapshot(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason,
                        }
                    })?;
                }
                MediaKind::Snapshot => {
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
    /// Two pixels per 3.25 MHz T-state, and 208 T-states over 312 lines — the
    /// same raster as a ZX80, and the same 1.14.
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
            .then_some(&KEYBOARD as &dyn emu198x_shell::KeyboardTarget)
    }
}

emu198x_shell::impl_z80_debug_primitives!(JupiterAceRuntime);

#[cfg(test)]
mod tests {
    use super::SYMBOL_LEGENDS;
    use crate::input::knows_key_name;

    /// A legend naming a key the layout does not have is refused rather than
    /// typed, so a typo here costs a character and reports nothing.
    #[test]
    fn every_legend_names_a_key_this_machine_has() {
        assert!(
            knows_key_name("symbol"),
            "the legend chord is Symbol Shift, so that name must resolve"
        );
        for (legend, key) in SYMBOL_LEGENDS {
            assert!(
                knows_key_name(key),
                "legend {legend:?} names {key:?}, which the layout does not have"
            );
        }
    }

    /// Two legends on one keycap would mean the second is unreachable, and
    /// two keycaps for one character means one of them is wrong.
    #[test]
    fn legends_are_one_to_one() {
        let mut chars: Vec<char> = SYMBOL_LEGENDS.iter().map(|(c, _)| *c).collect();
        chars.sort_unstable();
        let before = chars.len();
        chars.dedup();
        assert_eq!(before, chars.len(), "a character is listed twice");

        let mut keys: Vec<&str> = SYMBOL_LEGENDS.iter().map(|(_, k)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "a keycap carries two legends");
    }

    /// This is a Forth machine, so punctuation is not decoration: without
    /// these the Ace cannot be given a word definition, a stack operator or
    /// a print. `.` and `+` alone are the difference between `2 3 + .` and
    /// nothing at all.
    #[test]
    fn the_forth_punctuation_is_reachable() {
        for ch in ['.', '+', '-', '*', '/', ';', ':', '"', '<', '>', '=', ','] {
            assert!(
                SYMBOL_LEGENDS.iter().any(|(c, _)| *c == ch),
                "{ch:?} is still unreachable, so Forth cannot be typed"
            );
        }
    }

    /// `q`, `w` and `e` carry no red legend — the ROM falls through to the
    /// capital letter. Listing them would map a character onto a chord that
    /// does not produce it.
    #[test]
    fn keys_without_a_red_legend_are_not_listed() {
        for key in ["q", "w", "e"] {
            assert!(
                !SYMBOL_LEGENDS.iter().any(|(_, k)| *k == key),
                "{key:?} is listed as carrying a symbol legend, but it does not"
            );
        }
    }
}
