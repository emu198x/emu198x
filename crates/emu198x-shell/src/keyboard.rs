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
    /// `["CapsShift", "A"]`, `'a'` → `["A"]`). Returns `None` for a character
    /// with no single-keystroke equivalent on this machine.
    ///
    /// A `None` is an error, not a skip: `type_string` refuses the whole step
    /// rather than typing the rest. Silently dropping a character means a
    /// script asks for one string and the machine receives another, which on
    /// the C64 turned a comparison into a variable reference and produced a
    /// program that ran and gave the wrong answer (#916).
    fn keys_for_char(&self, ch: char) -> Option<Vec<String>>;

    /// Frame timing tuned to this machine's keyboard scan.
    fn key_timing(&self) -> KeyTiming;

    /// Expand a friendly compound-key name into the simultaneous chord that
    /// produces it — e.g. the Spectrum `"Edit"` → `["CapsShift", "1"]`. This
    /// lets `press_key("Edit")` stand in for the equivalent `press_keys` chord
    /// on machines whose legends are shift combinations rather than dedicated
    /// keys. Returns `None` for a name the machine drives as a single keystroke
    /// (the common case), so the default recognises no compound names.
    fn expand_named_key(&self, _name: &str) -> Option<Vec<String>> {
        None
    }
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
/// (lowercased), digits, space (`"space"`), and Enter (`"enter"`) — and offers
/// any other printable ASCII as its own one-character name, *if the machine
/// says it has that key*. A machine whose symbols need a shifted keycap (e.g.
/// the Spectrum) implements [`KeyboardTarget`] by hand instead.
///
/// The machine answers through `knows_name`, which is its own key-name
/// resolver: the same lookup `apply_input_event` performs before injecting a
/// keystroke. Asking it here is what makes the refusal in
/// [`KeyboardTarget::keys_for_char`] true. This used to pass every printable
/// character through and let the input layer drop the ones it did not
/// recognise, so `type_string` counted characters that never reached the
/// machine — twelve reported and ten delivered on the BBC, with `CHAIN""`
/// arriving as `CHAIN` (#1196). That is precisely the outcome #916 refused,
/// bypassed one layer further down where nothing was checking.
pub struct StandardKeyboard {
    timing: KeyTiming,
    knows_name: fn(&str) -> bool,
    legends: &'static [(char, &'static str)],
    shift_name: &'static str,
}

impl StandardKeyboard {
    /// Build a standard keyboard with the given timing, backed by the
    /// machine's own key-name resolver.
    ///
    /// `knows_name` must answer for the *machine*, not for the shape of the
    /// string: wrap whatever `apply_input_event` uses to turn a name into a
    /// key, so the two cannot disagree.
    ///
    /// A machine built this way can type only what its keycaps carry
    /// unshifted. Most 198x keyboards put half of printable ASCII on a
    /// shifted legend, so prefer [`Self::with_legends`].
    #[must_use]
    pub const fn new(timing: KeyTiming, knows_name: fn(&str) -> bool) -> Self {
        Self {
            timing,
            knows_name,
            legends: &[],
            shift_name: "shift",
        }
    }

    /// Build a standard keyboard that can also reach its shifted legends.
    ///
    /// `legends` pairs each character with the key that carries it, and the
    /// chord becomes `[shift_name, key]`. The machine's resolver still has
    /// the last word: a legend naming a key the layout does not have is
    /// refused rather than typed into the void.
    ///
    /// Establish the pairings by asking the machine, not from recollection:
    /// hold shift with each key in turn, let BASIC echo the result, and read
    /// it back off the screen. That is how every table in the tree was built,
    /// and it is what caught SHIFT-0 on the Dragon being a case lock rather
    /// than a symbol.
    #[must_use]
    pub const fn with_legends(
        timing: KeyTiming,
        knows_name: fn(&str) -> bool,
        shift_name: &'static str,
        legends: &'static [(char, &'static str)],
    ) -> Self {
        Self {
            timing,
            knows_name,
            legends,
            shift_name,
        }
    }
}

impl KeyboardTarget for StandardKeyboard {
    fn key_name_is_valid(&self, name: &str) -> bool {
        !name.is_empty() && (self.knows_name)(name)
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
            c if c.is_ascii_graphic() => {
                // A shifted legend is a chord, so it resolves against the key
                // that carries the symbol rather than the symbol itself.
                if let Some((_, key)) = self.legends.iter().find(|(legend, _)| *legend == c) {
                    return ((self.knows_name)(key) && (self.knows_name)(self.shift_name))
                        .then(|| vec![self.shift_name.to_owned(), (*key).to_owned()]);
                }
                c.to_string()
            }
            _ => return None,
        };
        // Ask the machine before promising the caller. A key this layout
        // does not have is a refusal, not a silent drop (#1196).
        (self.knows_name)(&name).then(|| vec![name])
    }

    fn key_timing(&self) -> KeyTiming {
        self.timing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layout with the universal core plus `-`, and nothing else — the
    /// shape of a real 198x keyboard whose symbol set is narrower than
    /// printable ASCII.
    fn narrow_layout(name: &str) -> bool {
        matches!(name, "space" | "enter" | "-" | "shift")
            || (name.len() == 1
                && name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
    }

    fn narrow_keyboard() -> StandardKeyboard {
        StandardKeyboard::new(STANDARD_KEY_TIMING, narrow_layout)
    }

    #[test]
    fn standard_keyboard_maps_the_universal_core() {
        let kb = narrow_keyboard();
        // Letters map to the lowercased bare keycap (uppercase charset).
        assert_eq!(kb.keys_for_char('A'), Some(vec!["a".to_owned()]));
        assert_eq!(kb.keys_for_char('a'), Some(vec!["a".to_owned()]));
        // Digits, space, and both newline forms.
        assert_eq!(kb.keys_for_char('7'), Some(vec!["7".to_owned()]));
        assert_eq!(kb.keys_for_char(' '), Some(vec!["space".to_owned()]));
        assert_eq!(kb.keys_for_char('\n'), Some(vec!["enter".to_owned()]));
        assert_eq!(kb.keys_for_char('\r'), Some(vec!["enter".to_owned()]));
        // A symbol the layout does have still passes through by name.
        assert_eq!(kb.keys_for_char('-'), Some(vec!["-".to_owned()]));
        // Non-printable / control characters are skipped.
        assert_eq!(kb.keys_for_char('\t'), None);
        assert!(!kb.key_name_is_valid(""));
    }

    /// The reported case. `"` reached the BBC as the key name `"`, which
    /// its layout does not have, so the input layer dropped it — and
    /// `type_string` had already counted it. `CHAIN""HELLO` reported
    /// twelve characters typed and put `CHAINHELLO` on screen (#1196).
    #[test]
    fn a_character_the_layout_lacks_is_refused_not_counted() {
        let kb = narrow_keyboard();
        assert_eq!(
            kb.keys_for_char('"'),
            None,
            "a printable character the machine cannot type must refuse, \
             so type_string stops instead of miscounting"
        );
        assert_eq!(kb.keys_for_char('*'), None);
    }

    /// A shifted legend resolves against the key that carries the symbol,
    /// not the symbol itself — the layout has no key called `"`.
    #[test]
    fn a_shifted_legend_types_as_a_chord() {
        const LEGENDS: &[(char, &str)] = &[('"', "2"), ('*', ":")];
        let kb =
            StandardKeyboard::with_legends(STANDARD_KEY_TIMING, narrow_layout, "shift", LEGENDS);
        // `2` is in the narrow layout, so the chord resolves.
        assert_eq!(
            kb.keys_for_char('"'),
            Some(vec!["shift".to_owned(), "2".to_owned()])
        );
        // `:` is not, so the legend is refused rather than typed into the
        // void — the machine still has the last word.
        assert_eq!(kb.keys_for_char('*'), None);
        // Unlisted characters behave as before.
        assert_eq!(kb.keys_for_char('a'), Some(vec!["a".to_owned()]));
        assert_eq!(kb.keys_for_char('#'), None);
    }

    /// A legend is useless if the machine has no shift key by that name --
    /// the PET's input layer has none, and would have produced a chord whose
    /// first keystroke went nowhere.
    #[test]
    fn a_legend_needs_the_shift_key_to_exist_too() {
        const LEGENDS: &[(char, &str)] = &[('"', "2")];
        let kb = StandardKeyboard::with_legends(
            STANDARD_KEY_TIMING,
            narrow_layout,
            "nosuchshift",
            LEGENDS,
        );
        assert_eq!(kb.keys_for_char('"'), None);
    }

    /// `press_key` took any non-empty name and let the machine drop it,
    /// so a typo reported success and did nothing.
    #[test]
    fn press_key_validates_against_the_layout() {
        let kb = narrow_keyboard();
        assert!(kb.key_name_is_valid("space"));
        assert!(kb.key_name_is_valid("a"));
        assert!(
            !kb.key_name_is_valid("return"),
            "this layout calls it enter"
        );
        assert!(!kb.key_name_is_valid("nosuchkey"));
    }
}
