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
//! Joystick events route directly to the machine via
//! [`SpectrumMachine::set_kempston_button`] — Kempston state is held
//! on the peripheral and read through the IO port, so it doesn't need
//! the cache-then-push dance the keyboard uses. Amstrad-class machines
//! (+2A / +2B / +3) inherit the default no-op `set_kempston_button`
//! and silently drop joystick events: the rear-connector pinout
//! changed in '87, so a Kempston interface cannot physically attach
//! to those machines.
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
/// - Higher-port events (port >= 1), pointer events, and unrecognised
///   names are silently dropped — port 1 is reserved for an eventual
///   Sinclair Interface 2 second port and is currently unimplemented.
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
                let (pos_pressed, neg_pressed) = if *value > AXIS_THRESHOLD {
                    (true, false)
                } else if *value < -AXIS_THRESHOLD {
                    (false, true)
                } else {
                    (false, false)
                };
                runtime.machine_mut().set_kempston_button(pos, pos_pressed);
                runtime.machine_mut().set_kempston_button(neg, neg_pressed);
            }
        }
        _ => {}
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
}
