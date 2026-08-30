//! Runtime wrapper for the Oric-1 / Atmos.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_oric_atmos::{FB_HEIGHT, FB_WIDTH, OricAtmos, OricModel};

use crate::input::apply_input_event;
use crate::profiles::{BIOS_FIRMWARE_ID, Model, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

/// Framebuffer pixels per second.
const PIXEL_CLOCK_HZ: f64 = 6_000_000.0;

const BIOS_SIZE: usize = 16 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Characters the Oric puts on a shifted legend, paired with the keycap that
/// carries them. Without this the machine could type only its unshifted
/// keycaps, so every one of these was refused (#1206) — including `+`, which
/// made `PRINT 2+3` untypeable.
///
/// Read off the machine, not off a layout diagram: all 64 cells were pressed
/// with and without shift on a UK Atmos ROM and the echoed character taken
/// from screen RAM. See `input.rs` for where that disagrees with MAME.
const SHIFTED_LEGENDS: &[(char, &str)] = &[
    ('!', "1"),
    ('"', "2"),
    ('_', "3"),
    ('$', "4"),
    ('%', "5"),
    ('^', "6"),
    ('&', "7"),
    ('*', "8"),
    ('(', "9"),
    (')', "0"),
    ('<', ","),
    ('>', "."),
    (':', ";"),
    ('@', "'"),
    ('|', "\\"),
    ('~', "#"),
    ('?', "/"),
    ('+', "="),
    ('{', "["),
    ('}', "]"),
];

/// This machine's keyboard for the shared `press_key` / `type_string` tools:
/// the standard layout, backed by this machine's own key-name resolver so a
/// character it cannot type is refused rather than silently dropped (#1196).
static KEYBOARD: emu198x_shell::StandardKeyboard = emu198x_shell::StandardKeyboard::with_legends(
    emu198x_shell::STANDARD_KEY_TIMING,
    crate::input::knows_key_name,
    "shift",
    SHIFTED_LEGENDS,
);

pub struct OricRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<OricAtmos>,
    bios_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    controller_cache: crate::input::ControllerCache,
    tape_bytes: Option<Vec<u8>>,
}

impl OricRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            bios_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: vec![0; (FB_WIDTH * FB_HEIGHT * 4) as usize],
            controller_cache: crate::input::ControllerCache::default(),
            tape_bytes: None,
        }
    }

    /// Build directly from a 16 KB BASIC + OS ROM.
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
        self.rebuild_machine();
        Ok(())
    }

    #[must_use]
    pub fn machine(&self) -> Option<&OricAtmos> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut OricAtmos> {
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
    /// restore path so the resumed machine keeps its CPU/VIA/PSG/RAM state.
    ///
    /// The Oric framebuffer is a fixed `FB_WIDTH × FB_HEIGHT`, allocated once
    /// in `blank`; `rebuild_machine` never resizes it, so neither does this —
    /// it only refills the RGBA buffer from the restored machine's pixels.
    pub(crate) fn set_machine(&mut self, machine: Option<OricAtmos>) {
        self.machine = machine;
        self.update_rgba_framebuffer();
    }

    fn rebuild_machine(&mut self) {
        let Some(bios) = self.bios_bytes.clone() else {
            self.machine = None;
            return;
        };
        let oric_model = match self.model {
            Model::Oric1 => OricModel::Oric1,
            Model::Atmos => OricModel::Atmos,
        };
        let mut machine = OricAtmos::new(bios, oric_model);
        if let Some(bytes) = &self.tape_bytes {
            machine.insert_tape(bytes.clone());
        }
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

impl MachineCore for OricRuntime {
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
                    format198x_tangerine_oric_tap::decode(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason: reason.to_string(),
                        }
                    })?;
                    let bytes = image.bytes.to_vec();
                    if let Some(machine) = self.machine.as_mut() {
                        machine.insert_tape(bytes.clone());
                    }
                    self.tape_bytes = Some(bytes);
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
                width: FB_WIDTH,
                height: FB_HEIGHT,
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
    /// Six pixels per 1 MHz character, forty characters a line. Sixty-four
    /// cycles at that rate is exactly PAL's 64 µs line, which is the check
    /// that the six is right.
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

emu198x_shell::impl_6502_debug_primitives!(OricRuntime);

// AY register-write watch. The Oric has no memory-write watch, so only the AY
// surface is implemented; the shared `watch_ay_*` tools drive it.
impl emu198x_shell::WatchTarget for OricRuntime {
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

    /// A legend naming a key the layout does not have is refused rather than
    /// typed, so a typo here costs a character and reports nothing. Check the
    /// whole table resolves, and that shift itself does.
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

    /// `+` is the one that made this visible: `PRINT 2+3` was untypeable, so
    /// the machine could not be asked to do arithmetic at all (#1206).
    #[test]
    fn the_arithmetic_operators_are_reachable() {
        for ch in ['+', '*', '(', ')', ':', '?', '<', '>'] {
            assert!(
                SHIFTED_LEGENDS.iter().any(|(c, _)| *c == ch),
                "{ch:?} is still unreachable"
            );
        }
    }
}
