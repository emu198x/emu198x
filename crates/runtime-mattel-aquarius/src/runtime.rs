//! Runtime wrapper for the Mattel Aquarius.
//!
//! The Aquarius needs an 8 KB Microsoft-BASIC ROM at construction time.
//! The runtime stays in `Option` until firmware arrives via
//! `set_bios` or `from_firmware`.
//!
//! The Aquarius's 1-bit speaker isn't yet routed through a host-side
//! audio buffer (the chip exposes only `speaker_bit()`); the runtime
//! emits empty audio packets per frame. Real PWM-style speaker
//! resampling is a follow-up.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_mattel_aquarius::{Aquarius, AquariusRegion};

use crate::input::apply_input_event;
use crate::profiles::{BIOS_FIRMWARE_ID, CHAR_FIRMWARE_ID, Model, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

const NTSC_PIXEL_CLOCK_HZ: f64 = 7_159_090.0;

/// Framebuffer pixels per second on PAL.
const PAL_PIXEL_CLOCK_HZ: f64 = 7_093_788.0;

const BIOS_SIZE: usize = 8 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// This machine's keyboard for the shared `press_key` / `type_string` tools:
/// the standard layout, backed by this machine's own key-name resolver so a
/// character it cannot type is refused rather than silently dropped (#1196).
/// Characters the Aquarius puts on a shifted legend, paired with the keycap
/// that carries them. Without this the machine could type only its unshifted
/// keycaps, so `+`, `*`, `<`, `>`, `?`, `@`, `^`, `_` and the whole
/// shifted-digit row were refused (#1206) — `PRINT 2+3` included.
///
/// Read off the machine: all 48 matrix cells pressed with and without shift,
/// in BASIC on the real ROM, taking the echoed character out of the screen
/// RAM at `$3000`.
///
/// `\` is genuinely shift-Backspace on this keyboard — an odd pairing, but
/// that is what the ROM returns.
const SHIFTED_LEGENDS: &[(char, &str)] = &[
    ('!', "1"),
    ('"', "2"),
    ('#', "3"),
    ('$', "4"),
    ('%', "5"),
    ('&', "6"),
    ('\'', "7"),
    ('(', "8"),
    (')', "9"),
    ('?', "0"),
    ('+', "="),
    ('*', ":"),
    ('@', ";"),
    ('>', "."),
    ('_', "-"),
    ('^', "/"),
    ('<', ","),
    ('\\', "backspace"),
];

static KEYBOARD: emu198x_shell::StandardKeyboard = emu198x_shell::StandardKeyboard::with_legends(
    emu198x_shell::STANDARD_KEY_TIMING,
    crate::input::knows_key_name,
    "shift",
    SHIFTED_LEGENDS,
);

pub struct AquariusRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Aquarius>,
    bios_bytes: Option<Vec<u8>>,
    char_rom_bytes: Option<Vec<u8>>,
    cart_bytes: Option<Vec<u8>>,
    expansion_kb: usize,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: crate::input::ControllerCache,
}

impl AquariusRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            bios_bytes: None,
            char_rom_bytes: None,
            cart_bytes: None,
            expansion_kb: 0,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            controller_cache: crate::input::ControllerCache::default(),
        }
    }

    /// Build directly from an 8 KB BASIC ROM.
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
        let bios =
            firmware
                .bytes(BIOS_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: BIOS_FIRMWARE_ID.to_owned(),
                })?;
        let char_rom =
            firmware
                .bytes(CHAR_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: CHAR_FIRMWARE_ID.to_owned(),
                })?;
        let mut runtime = Self::new(model, bios.to_vec())?;
        runtime.set_char_rom(char_rom.to_vec())?;
        Ok(runtime)
    }

    /// Replace the BIOS image and rebuild the wrapped machine.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the ROM size is wrong.
    pub fn set_bios(&mut self, bios: Vec<u8>) -> Result<(), MachineError> {
        if bios.len() != BIOS_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: BIOS_FIRMWARE_ID.to_owned(),
                reason: format!("BIOS is {} bytes; expected {BIOS_SIZE}", bios.len()),
            });
        }
        self.bios_bytes = Some(bios);
        self.rebuild_machine();
        Ok(())
    }

    /// Supply the 2 KB character-generator ROM and rebuild so glyphs render
    /// from it. Without it the display is garbage (the font is a separate chip,
    /// not part of the BASIC ROM).
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the ROM is not 2 KB.
    pub fn set_char_rom(&mut self, char_rom: Vec<u8>) -> Result<(), MachineError> {
        if char_rom.len() != 2048 {
            return Err(MachineError::InvalidFirmware {
                id: CHAR_FIRMWARE_ID.to_owned(),
                reason: format!("character ROM is {} bytes; expected 2048", char_rom.len()),
            });
        }
        self.char_rom_bytes = Some(char_rom);
        self.rebuild_machine();
        Ok(())
    }

    /// Set RAM expansion (0..=16 KB).
    pub fn set_expansion_kb(&mut self, kb: usize) {
        self.expansion_kb = kb.min(16);
        self.rebuild_machine();
    }

    /// Insert a cart ROM (replaces any existing).
    pub fn insert_cartridge(&mut self, rom: Vec<u8>) {
        self.cart_bytes = Some(rom.clone());
        if let Some(machine) = self.machine.as_mut() {
            machine.insert_cart(rom);
        }
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Aquarius> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Aquarius> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn cart_bytes(&self) -> Option<&[u8]> {
        self.cart_bytes.as_deref()
    }

    pub(crate) fn expansion_kb(&self) -> usize {
        self.expansion_kb
    }

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its CPU/RAM/PSG/display state.
    pub(crate) fn set_machine(&mut self, machine: Option<Aquarius>) {
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
        let Some(bios) = self.bios_bytes.clone() else {
            self.machine = None;
            return;
        };
        let mut machine = Aquarius::new(bios, self.expansion_kb, AquariusRegion::Ntsc);
        if let Some(char_rom) = self.char_rom_bytes.clone() {
            machine.set_char_rom(char_rom);
        }
        if let Some(rom) = self.cart_bytes.clone() {
            machine.insert_cart(rom);
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

impl MachineCore for AquariusRuntime {
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
            match (image.slot.as_ref(), image.kind) {
                ("cartridge-1", MediaKind::Cartridge) => {
                    self.insert_cartridge(image.bytes.to_vec());
                }
                (slot, MediaKind::Cartridge) => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                (_, kind) => {
                    return Err(MachineError::UnsupportedMediaKind { kind });
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

            // Speaker bit not yet PWM-resampled — emit empty packets.
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

    /// Two dots per T-state — the core's own frame arithmetic divides its dot
    /// count by two to get T-states — putting the buffer at twice the
    /// 3.58 MHz CPU clock.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(
            self.profile().region,
            PAL_PIXEL_CLOCK_HZ,
            NTSC_PIXEL_CLOCK_HZ,
        )
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

    fn watch_target(&self) -> Option<&dyn emu198x_shell::WatchTarget> {
        self.machine
            .is_some()
            .then_some(self as &dyn emu198x_shell::WatchTarget)
    }
    fn watch_target_mut(&mut self) -> Option<&mut dyn emu198x_shell::WatchTarget> {
        if self.machine.is_some() {
            Some(self as &mut dyn emu198x_shell::WatchTarget)
        } else {
            None
        }
    }
}

emu198x_shell::impl_z80_debug_primitives!(AquariusRuntime);

// AY register-write watch. The Aquarius has no memory-write watch, so only the
// AY surface is implemented; the shared `watch_ay_*` tools drive it.
impl emu198x_shell::WatchTarget for AquariusRuntime {
    fn supports_ay_watch(&self) -> bool {
        true
    }

    fn start_ay_watch(&mut self) -> Result<u32, emu198x_shell::WatchError> {
        match self.machine.as_mut() {
            Some(m) => Ok(m.start_ay_write_watch()),
            None => Err(emu198x_shell::WatchError::Unsupported),
        }
    }

    fn clear_ay_watch(&mut self) -> (bool, u32) {
        let Some(m) = self.machine.as_mut() else {
            return (false, 0);
        };
        let captured = m.ay_write_watch_records().map_or(0, |r| r.len() as u32);
        let had_watch = m.ay_write_watch_records().is_some();
        m.stop_ay_write_watch();
        (had_watch, captured)
    }

    fn ay_watch_records(&self) -> Option<Vec<emu198x_shell::WatchAyRecord>> {
        self.machine
            .as_ref()?
            .ay_write_watch_records()
            .map(|records| {
                records
                    .iter()
                    .map(|r| emu198x_shell::WatchAyRecord {
                        pc: u32::from(r.pc),
                        register: r.register,
                        value: r.value,
                    })
                    .collect()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::SHIFTED_LEGENDS;
    use crate::input::knows_key_name;
    use crate::profiles::{BIOS_FIRMWARE_ID, CHAR_FIRMWARE_ID, Model};
    use crate::runtime::AquariusRuntime;
    use emu198x_shell::{FirmwareImage, FirmwareSet, MachineError};

    #[test]
    fn firmware_set_requires_the_separate_character_rom() {
        let bios = vec![0; 8192];
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &bios));

        match AquariusRuntime::from_firmware(Model::Aquarius, &firmware) {
            Err(MachineError::MissingFirmware { id }) => assert_eq!(id, CHAR_FIRMWARE_ID),
            Err(other) => panic!("expected missing character ROM, got {other:?}"),
            Ok(_) => panic!("expected missing character ROM"),
        }
    }

    #[test]
    fn firmware_set_builds_with_both_physical_roms() {
        let bios = vec![0; 8192];
        let char_rom = vec![0xff; 2048];
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(BIOS_FIRMWARE_ID, &bios));
        firmware.push(FirmwareImage::new(CHAR_FIRMWARE_ID, &char_rom));

        let runtime = AquariusRuntime::from_firmware(Model::Aquarius, &firmware)
            .expect("both valid ROMs build the runtime");
        assert!(runtime.machine().is_some());
    }

    /// A legend naming a key the layout does not have is refused rather than
    /// typed, so a typo here costs a character and reports nothing.
    #[test]
    fn every_legend_names_a_key_this_machine_has() {
        assert!(knows_key_name("shift"), "the shift chord needs a shift key");
        for (legend, key) in SHIFTED_LEGENDS {
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
        let mut chars: Vec<char> = SHIFTED_LEGENDS.iter().map(|(c, _)| *c).collect();
        chars.sort_unstable();
        let before = chars.len();
        chars.dedup();
        assert_eq!(before, chars.len(), "a character is listed twice");

        let mut keys: Vec<&str> = SHIFTED_LEGENDS.iter().map(|(_, k)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "a keycap carries two legends");
    }

    /// The operators #1206 was about — `PRINT 2+3*4` needs both, and neither
    /// has an unshifted keycap here.
    #[test]
    fn the_arithmetic_operators_are_reachable() {
        for ch in ['+', '*', '<', '>', '?', '@', '^', '_'] {
            assert!(
                SHIFTED_LEGENDS.iter().any(|(c, _)| *c == ch),
                "{ch:?} is still unreachable"
            );
        }
    }
}
