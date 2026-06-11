//! Machine-agnostic keyboard access for the shared `press_key` / `type_string`
//! tools.
//!
//! Pressing a named key and typing a string are the same shape on every
//! machine — queue a [`crate::InputEvent::Key`], run a hold window so the
//! ROM's keyboard scan sees it, release, settle — and that orchestration is
//! already generic (it only touches [`crate::HeadlessSession`]). What differs
//! per machine is small and declarative:
//!
//! - **which key names are valid** (the layout), for `press_key`;
//! - **how a character maps to keystrokes** (shift handling, symbols), for
//!   `type_string`;
//! - **the frame timing** tuned to that ROM's keyboard-scan cadence.
//!
//! A machine surfaces those three through [`KeyboardTarget`], exposed by
//! [`MachineCore::keyboard_target`](crate::MachineCore::keyboard_target), so
//! the shared `press_key` / `type_string` `ScriptStep` arms run one body for
//! both MCP and `--script` (the injection itself stays on the session). The
//! trait is read-only: it answers questions; the arms do the queue/run.

/// Per-machine keyboard frame timing. Each value is in the machine's native
/// frames and is tuned to its ROM keyboard-scan cadence — do **not**
/// standardise these without re-validating real-boot typing on each machine.
#[derive(Clone, Copy, Debug)]
pub struct KeyTiming {
    /// Frames a key is held before release when the request omits `hold_frames`.
    pub default_hold_frames: u32,
    /// Upper clamp on the hold window so a script cannot stall the session.
    pub max_hold_frames: u32,
    /// Frames run after a `press_key` release before the step returns (so the
    /// released state is visible to the next step).
    pub press_settle_frames: u32,
    /// Frames run after each `type_string` keystroke release.
    pub inter_key_settle_frames: u32,
    /// Extra frames run before re-pressing the *same* key in `type_string`, so
    /// the ROM scan sees the release between two identical keys. `0` disables.
    pub repeat_settle_frames: u32,
    /// Frames run after the whole `type_string` when the request omits
    /// `settle_frames`. `0` for none.
    pub default_type_settle_frames: u32,
}

/// Read-only keyboard description a machine exposes so the shared
/// `press_key` / `type_string` arms can drive it.
///
/// Implementors are the per-system runtimes. Every method is `&self`: the
/// trait only *describes* the keyboard; the actual key injection runs on the
/// session in [`crate::script::ScriptStep::execute_collect`].
pub trait KeyboardTarget {
    /// Whether `name` is a key this machine's layout recognises (`press_key`).
    fn key_name_is_valid(&self, name: &str) -> bool;

    /// Human-readable list of valid key names, for a `press_key` error on an
    /// unknown name.
    fn key_names_hint(&self) -> &'static str;

    /// Translate `ch` into the simultaneous key chord that produces it —
    /// modifiers first, base key last (e.g. Spectrum `'A'` →
    /// `["CapsShift", "A"]`, `'a'` → `["A"]`). Returns `None` to skip a
    /// character with no single-keystroke equivalent.
    fn keys_for_char(&self, ch: char) -> Option<Vec<String>>;

    /// Frame timing tuned to this machine's keyboard scan.
    fn key_timing(&self) -> KeyTiming;
}

/// Conservative default keyboard timing, in frames — suits a 50/60 Hz
/// keyboard scan on a machine that has no specially-tuned values.
pub const STANDARD_KEY_TIMING: KeyTiming = KeyTiming {
    default_hold_frames: 3,
    max_hold_frames: 600,
    press_settle_frames: 1,
    inter_key_settle_frames: 2,
    repeat_settle_frames: 2,
    default_type_settle_frames: 8,
};

/// A ready-made [`KeyboardTarget`] for the common case: a machine whose input
/// layer accepts lowercased key names and whose default character set draws
/// letters in upper case (so a letter types via its bare keycap, no shift).
///
/// It maps the universal core every 198x keyboard accepts — letters
/// (lowercased), digits, space (`"space"`), and Enter (`"enter"`) — and passes
/// any other printable ASCII through as its own one-character name; the
/// machine's input layer silently ignores a name it doesn't recognise. A
/// machine whose symbols need a shifted keycap (e.g. the Spectrum) implements
/// [`KeyboardTarget`] by hand instead.
///
/// Stateless, so one shared instance serves every machine: see
/// [`STANDARD_KEYBOARD`].
pub struct StandardKeyboard {
    timing: KeyTiming,
}

impl StandardKeyboard {
    /// Build a standard keyboard with the given timing.
    #[must_use]
    pub const fn new(timing: KeyTiming) -> Self {
        Self { timing }
    }
}

impl Default for StandardKeyboard {
    fn default() -> Self {
        Self::new(STANDARD_KEY_TIMING)
    }
}

impl KeyboardTarget for StandardKeyboard {
    fn key_name_is_valid(&self, name: &str) -> bool {
        // The input layer drops names it doesn't know, so accept anything
        // non-empty rather than maintain a per-machine allow-list here.
        !name.is_empty()
    }

    fn key_names_hint(&self) -> &'static str {
        "A-Z, 0-9, Space, Enter (plus this machine's other named keys)"
    }

    fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
        let name = match ch {
            'a'..='z' | 'A'..='Z' => ch.to_ascii_lowercase().to_string(),
            '0'..='9' => ch.to_string(),
            ' ' => "space".to_owned(),
            '\n' | '\r' => "enter".to_owned(),
            c if c.is_ascii_graphic() => c.to_string(),
            _ => return None,
        };
        Some(vec![name])
    }

    fn key_timing(&self) -> KeyTiming {
        self.timing
    }
}

/// The shared [`StandardKeyboard`] instance. A machine with an ASCII keyboard
/// returns `Some(&STANDARD_KEYBOARD)` from
/// [`MachineCore::keyboard_target`](crate::MachineCore::keyboard_target) to get
/// the shared `press_key` / `type_string` verbs with no per-machine table.
pub static STANDARD_KEYBOARD: StandardKeyboard = StandardKeyboard::new(STANDARD_KEY_TIMING);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_keyboard_maps_the_universal_core() {
        let kb = StandardKeyboard::default();
        // Letters map to the lowercased bare keycap (uppercase charset).
        assert_eq!(kb.keys_for_char('A'), Some(vec!["a".to_owned()]));
        assert_eq!(kb.keys_for_char('a'), Some(vec!["a".to_owned()]));
        // Digits, space, and both newline forms.
        assert_eq!(kb.keys_for_char('7'), Some(vec!["7".to_owned()]));
        assert_eq!(kb.keys_for_char(' '), Some(vec!["space".to_owned()]));
        assert_eq!(kb.keys_for_char('\n'), Some(vec!["enter".to_owned()]));
        assert_eq!(kb.keys_for_char('\r'), Some(vec!["enter".to_owned()]));
        // Printable symbols pass through as their own one-character name.
        assert_eq!(kb.keys_for_char('*'), Some(vec!["*".to_owned()]));
        // Non-printable / control characters are skipped.
        assert_eq!(kb.keys_for_char('\t'), None);
        // Validation is permissive (the input layer drops unknown names).
        assert!(kb.key_name_is_valid("return"));
        assert!(!kb.key_name_is_valid(""));
    }
}
