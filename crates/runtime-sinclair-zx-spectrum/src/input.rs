//! Spectrum keyboard + joystick input mapping.
//!
//! Splits the per-event keyboard handling out of `runtime.rs`. The
//! Spectrum's keyboard matrix is a runtime-owned cache that survives
//! across `run_until` calls — the matrix is updated incrementally as
//! events arrive, and `run_until` pushes the cached rows into the
//! machine once before stepping the frame. The runtime is therefore
//! the natural argument for this free function: it owns both the
//! cache and the machine, and Spectrum input is uniform across every
//! variant in the family (each variant's `set_keyboard_rows` is on
//! the `SpectrumMachine` trait).
//!
//! Three logical input ports:
//!
//! - **Port 0 — Kempston.** Routes to the machine's `KempstonJoystick`
//!   peripheral via [`SpectrumMachine::set_kempston_button`]. State is
//!   held on the peripheral and read through IO port `$1F`, so it
//!   doesn't need the cache-then-push dance the keyboard uses.
//!   Amstrad-class machines (+2A / +2B / +3) inherit the default no-op
//!   `set_kempston_button` and silently drop port-0 events: the
//!   rear-connector pinout changed in '87, so a Kempston interface
//!   cannot physically attach.
//!
//! - **Port 1 — Sinclair Interface 2 first port** (Grussu's "Port 1";
//!   keys `6`/`7`/`8`/`9`/`0` for left/right/down/up/fire). Hardware
//!   either-or: a real grey +2 / +2A / +2B / +3 has this wired
//!   internally to the keyboard matrix; a real 48K / 128K user attaches
//!   the 1983 Sinclair Interface 2 cartridge. Either way the host
//!   abstraction is "press this joystick → close those keyboard
//!   contacts", and the runtime input layer routes uniformly across
//!   variants because the keyboard matrix is universal.
//!
//! - **Port 2 — Sinclair Interface 2 second port** (Grussu's "Port 2";
//!   keys `1`/`2`/`3`/`4`/`5` for left/right/down/up/fire). Same
//!   routing path as port 1, different keys.
//!
//! Port mapping source: Grussu, *Spectrumpedia Volume 1* p. 140
//! ("Sinclair system" port table, quoted verbatim in
//! `knowledge/decisions/spectrum-architecture-review.md` and
//! `knowledge/decisions/spectrum-joystick-architecture.md`).
//!
//! The `<M: SpectrumMachine>` bound threads through so the function
//! works for every `SpectrumRuntime<M>` instantiation.

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use emu198x_shell::InputEvent;

use crate::runtime::{SpectrumMachine, SpectrumRuntime};

/// Threshold for axis-to-button conversion. The Kempston is a digital
/// joystick (5 buttons, no analogue range), so we discretise host axes
/// at 25% of full deflection — generous enough that a real gamepad's
/// resting jitter doesn't fire spurious directional events, tight
/// enough that intentional movement registers immediately. The
/// threshold lives at the input-routing boundary, not the peripheral,
/// because it's host-tuning, not silicon behaviour.
const AXIS_THRESHOLD: i16 = 8192;

/// Apply one host input event to the runtime's input state.
///
/// - `InputEvent::Key { name, pressed }` → recognised key names update
///   the matching cell in the keyboard cache.
/// - `InputEvent::Button { port: 0, name, pressed }` → recognised
///   Kempston button names update the joystick state on the machine.
/// - `InputEvent::Axis { port: 0, name, value }` → recognised axis
///   names update the corresponding directional pair on the joystick
///   (e.g. "horizontal" drives left/right with a deadzone).
/// - `InputEvent::Button { port: 1 | 2, name, pressed }` → recognised
///   IF2 button names map to keyboard-matrix entries per Grussu's
///   table (port 1 = keys 6/7/8/9/0; port 2 = keys 1/2/3/4/5).
/// - `InputEvent::Axis { port: 1 | 2, name, value }` → axis-to-button
///   conversion with the same 25% deadzone as Kempston, routed to
///   the IF2 key positions.
/// - Higher-port events (port >= 3), pointer events, and unrecognised
///   names are silently dropped.
///
/// The cached keyboard rows are pushed to the machine separately by
/// `run_until` to preserve the original "decode N events, push once"
/// semantics. Joystick state is pushed inline because the peripheral
/// is read on every port-IO and has no row-buffer equivalent.
pub(crate) fn apply_input_event<M: SpectrumMachine>(
    runtime: &mut SpectrumRuntime<M>,
    event: &InputEvent,
) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some(key) = SpectrumKey::from_name(name.as_ref()) {
                runtime.keyboard_mut().set_key(key, *pressed);
            }
        }
        InputEvent::Button { port: 0, name, pressed } => {
            if let Some(button) = kempston_button_from_name(name.as_ref()) {
                runtime.machine_mut().set_kempston_button(button, *pressed);
            }
        }
        InputEvent::Axis { port: 0, name, value } => {
            if let Some((neg, pos)) = kempston_axis_pair(name.as_ref()) {
                let (pos_pressed, neg_pressed) = axis_to_button_pair(*value);
                runtime.machine_mut().set_kempston_button(pos, pos_pressed);
                runtime.machine_mut().set_kempston_button(neg, neg_pressed);
            }
        }
        InputEvent::Button { port: port @ (1 | 2), name, pressed } => {
            if let Some(key) = if2_button_to_key(*port, name.as_ref()) {
                runtime.keyboard_mut().set_key(key, *pressed);
            }
        }
        InputEvent::Axis { port: port @ (1 | 2), name, value } => {
            if let Some((neg, pos)) = if2_axis_key_pair(*port, name.as_ref()) {
                let (pos_pressed, neg_pressed) = axis_to_button_pair(*value);
                runtime.keyboard_mut().set_key(pos, pos_pressed);
                runtime.keyboard_mut().set_key(neg, neg_pressed);
            }
        }
        _ => {}
    }
}

/// Discretises a signed-16-bit host axis value to a (positive,
/// negative) press pair using [`AXIS_THRESHOLD`]. Shared between
/// Kempston (port 0) and IF2 (port 1/2) so the deadzone behaviour is
/// identical regardless of which port the host event arrives on.
fn axis_to_button_pair(value: i16) -> (bool, bool) {
    if value > AXIS_THRESHOLD {
        (true, false)
    } else if value < -AXIS_THRESHOLD {
        (false, true)
    } else {
        (false, false)
    }
}

/// Resolves a host button name to a Kempston button index. Accepts the
/// canonical Kempston direction names (`right` / `left` / `down` /
/// `up`), the standard `fire` label, and `button1` / `a` as common
/// gamepad aliases for fire — controllers vary wildly in what they
/// emit, and the host-side shell normalises only the names; the
/// runtime is the right place to map those to the joystick's
/// 5-button alphabet.
fn kempston_button_from_name(name: &str) -> Option<u8> {
    match name {
        "right" => Some(0),
        "left" => Some(1),
        "down" => Some(2),
        "up" => Some(3),
        "fire" | "button1" | "a" => Some(4),
        _ => None,
    }
}

/// Resolves a host axis name to a (negative, positive) Kempston
/// button index pair. The horizontal axis drives left (negative) and
/// right (positive); the vertical axis drives up (negative) and down
/// (positive) — matching the screen-coordinate convention where
/// positive-y is downward.
fn kempston_axis_pair(name: &str) -> Option<(u8, u8)> {
    match name {
        "horizontal" | "x" => Some((1, 0)), // left, right
        "vertical" | "y" => Some((3, 2)),   // up, down
        _ => None,
    }
}

/// Resolves an IF2 button event to the `SpectrumKey` it closes on the
/// keyboard matrix. Per Grussu's table:
///
/// |         | left | right | down | up | fire |
/// |---------|------|-------|------|----|------|
/// | port 1  |  6   |   7   |  8   |  9 |   0  |
/// | port 2  |  1   |   2   |  3   |  4 |   5  |
///
/// Returns `None` for unrecognised port/name combinations, mirroring
/// the Kempston resolver's "silently drop unknown" contract.
fn if2_button_to_key(port: u8, name: &str) -> Option<SpectrumKey> {
    match (port, name) {
        (1, "left") => Some(SpectrumKey::Num6),
        (1, "right") => Some(SpectrumKey::Num7),
        (1, "down") => Some(SpectrumKey::Num8),
        (1, "up") => Some(SpectrumKey::Num9),
        (1, "fire" | "button1" | "a") => Some(SpectrumKey::Num0),
        (2, "left") => Some(SpectrumKey::Num1),
        (2, "right") => Some(SpectrumKey::Num2),
        (2, "down") => Some(SpectrumKey::Num3),
        (2, "up") => Some(SpectrumKey::Num4),
        (2, "fire" | "button1" | "a") => Some(SpectrumKey::Num5),
        _ => None,
    }
}

/// Resolves a host axis name to a (negative, positive) `SpectrumKey`
/// pair on the IF2 port's keyboard-matrix row. Horizontal drives
/// left (negative) and right (positive); vertical drives up
/// (negative) and down (positive) — same convention as Kempston.
fn if2_axis_key_pair(port: u8, name: &str) -> Option<(SpectrumKey, SpectrumKey)> {
    match (port, name) {
        (1, "horizontal" | "x") => Some((SpectrumKey::Num6, SpectrumKey::Num7)),
        (1, "vertical" | "y") => Some((SpectrumKey::Num9, SpectrumKey::Num8)),
        (2, "horizontal" | "x") => Some((SpectrumKey::Num1, SpectrumKey::Num2)),
        (2, "vertical" | "y") => Some((SpectrumKey::Num4, SpectrumKey::Num3)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_name_resolution_covers_canonical_kempston_layout() {
        assert_eq!(kempston_button_from_name("right"), Some(0));
        assert_eq!(kempston_button_from_name("left"), Some(1));
        assert_eq!(kempston_button_from_name("down"), Some(2));
        assert_eq!(kempston_button_from_name("up"), Some(3));
        assert_eq!(kempston_button_from_name("fire"), Some(4));
    }

    #[test]
    fn fire_button_aliases_resolve_to_the_same_index() {
        // Controllers vary in how they label the primary action button;
        // the runtime accepts the common spellings without forcing the
        // host shell to normalise every gamepad SDK in the world.
        assert_eq!(kempston_button_from_name("button1"), Some(4));
        assert_eq!(kempston_button_from_name("a"), Some(4));
    }

    #[test]
    fn unrecognised_button_names_return_none() {
        assert_eq!(kempston_button_from_name(""), None);
        assert_eq!(kempston_button_from_name("RIGHT"), None); // case-sensitive
        assert_eq!(kempston_button_from_name("trigger"), None);
    }

    #[test]
    fn axis_pair_matches_screen_coordinate_convention() {
        assert_eq!(kempston_axis_pair("horizontal"), Some((1, 0)));
        assert_eq!(kempston_axis_pair("x"), Some((1, 0)));
        assert_eq!(kempston_axis_pair("vertical"), Some((3, 2)));
        assert_eq!(kempston_axis_pair("y"), Some((3, 2)));
        assert_eq!(kempston_axis_pair("z"), None);
    }

    #[test]
    fn if2_port_1_button_mapping_matches_grussu_table() {
        assert_eq!(if2_button_to_key(1, "left"), Some(SpectrumKey::Num6));
        assert_eq!(if2_button_to_key(1, "right"), Some(SpectrumKey::Num7));
        assert_eq!(if2_button_to_key(1, "down"), Some(SpectrumKey::Num8));
        assert_eq!(if2_button_to_key(1, "up"), Some(SpectrumKey::Num9));
        assert_eq!(if2_button_to_key(1, "fire"), Some(SpectrumKey::Num0));
    }

    #[test]
    fn if2_port_2_button_mapping_matches_grussu_table() {
        assert_eq!(if2_button_to_key(2, "left"), Some(SpectrumKey::Num1));
        assert_eq!(if2_button_to_key(2, "right"), Some(SpectrumKey::Num2));
        assert_eq!(if2_button_to_key(2, "down"), Some(SpectrumKey::Num3));
        assert_eq!(if2_button_to_key(2, "up"), Some(SpectrumKey::Num4));
        assert_eq!(if2_button_to_key(2, "fire"), Some(SpectrumKey::Num5));
    }

    #[test]
    fn if2_accepts_fire_aliases() {
        assert_eq!(if2_button_to_key(1, "button1"), Some(SpectrumKey::Num0));
        assert_eq!(if2_button_to_key(1, "a"), Some(SpectrumKey::Num0));
        assert_eq!(if2_button_to_key(2, "button1"), Some(SpectrumKey::Num5));
        assert_eq!(if2_button_to_key(2, "a"), Some(SpectrumKey::Num5));
    }

    #[test]
    fn if2_unrecognised_combinations_return_none() {
        assert_eq!(if2_button_to_key(0, "fire"), None); // port 0 is Kempston, not IF2
        assert_eq!(if2_button_to_key(3, "fire"), None); // no port 3
        assert_eq!(if2_button_to_key(1, "trigger"), None);
        assert_eq!(if2_button_to_key(1, "LEFT"), None); // case-sensitive
    }

    #[test]
    fn if2_axis_pairs_drive_the_directional_keys() {
        // Port 1: horizontal = (left=6, right=7); vertical = (up=9, down=8).
        assert_eq!(
            if2_axis_key_pair(1, "horizontal"),
            Some((SpectrumKey::Num6, SpectrumKey::Num7))
        );
        assert_eq!(
            if2_axis_key_pair(1, "vertical"),
            Some((SpectrumKey::Num9, SpectrumKey::Num8))
        );
        // Port 2: horizontal = (left=1, right=2); vertical = (up=4, down=3).
        assert_eq!(
            if2_axis_key_pair(2, "horizontal"),
            Some((SpectrumKey::Num1, SpectrumKey::Num2))
        );
        assert_eq!(
            if2_axis_key_pair(2, "vertical"),
            Some((SpectrumKey::Num4, SpectrumKey::Num3))
        );
        assert_eq!(if2_axis_key_pair(0, "horizontal"), None);
        assert_eq!(if2_axis_key_pair(1, "z"), None);
    }

    #[test]
    fn axis_to_button_pair_respects_deadzone() {
        // Centre / inside deadzone: nothing pressed.
        assert_eq!(axis_to_button_pair(0), (false, false));
        assert_eq!(axis_to_button_pair(AXIS_THRESHOLD), (false, false));
        assert_eq!(axis_to_button_pair(-AXIS_THRESHOLD), (false, false));

        // Past threshold: only the matching direction.
        assert_eq!(axis_to_button_pair(AXIS_THRESHOLD + 1), (true, false));
        assert_eq!(axis_to_button_pair(i16::MAX), (true, false));
        assert_eq!(axis_to_button_pair(-AXIS_THRESHOLD - 1), (false, true));
        assert_eq!(axis_to_button_pair(i16::MIN), (false, true));
    }
}
