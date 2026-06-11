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
