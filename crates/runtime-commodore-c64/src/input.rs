//! C64 keyboard / joystick input mapping.
//!
//! Splits the keyboard-matrix lookup table out of `runtime.rs` so the
//! 70+ key entries don't dominate the file. The matrix is the
//! standard PAL breadbin layout (HRM Appendix C); shifted symbols
//! land on the right keycap on a UK/US keyboard.
//!
//! ## Joystick port convention (Seam 2)
//!
//! The C64 has two 9-pin DIN joystick ports, both wired to CIA1. The
//! "main" gameport (port 2 on the case, scanned via CIA1 PA at
//! `$DC00`) is the one most software polls — port 1 (CIA1 PB at
//! `$DC01`) shares wiring with the keyboard column lines and
//! produces phantom key presses when used.
//!
//! Runtime input convention — the C64's two control ports are
//! silkscreened "1" and "2" on the case, so those hardware numbers map
//! straight through; port 0 is the cross-system "primary stick" alias
//! (mirrors the Spectrum's port-0-is-default):
//! - `InputEvent::Button { port: 2, … }` → C64 gameport 2 (CIA1 PA,
//!   main gameport — what `LDA $DC00` reads, and the port nearly all
//!   software polls). This is the hardware-faithful number.
//! - `InputEvent::Button { port: 0, … }` → the same main gameport 2,
//!   as the cross-system default-stick alias.
//! - `InputEvent::Button { port: 1, … }` → C64 gameport 1 (CIA1 PB,
//!   keyboard-shared — `LDA $DC01`). The keyboard conflict is
//!   unavoidable in hardware; software polling the keyboard while a
//!   stick is plugged into port 1 will see phantom presses.
//!
//! Control names supported (case-insensitive):
//!     `up`, `down`, `left`, `right`, `fire`.
//!
//! Gamepad SDK aliases (so native code is neutral):
//!     `south` / `cross` / `east` / `circle` / `west` / `north` /
//!     `button{1,2,3,4}` → `fire` (single-fire joystick, every face
//!     button routes to FIRE).

use emu198x_shell::InputEvent;
use machine_commodore_c64::C64;

/// Map a Seam-2 input port onto the C64's case-labelled control port.
/// The two ports are silkscreened "1" and "2" (CIA1 PA = "Control Port
/// 2" at `$DC00`; CIA1 PB = "Control Port 1" at `$DC01`), so those
/// hardware numbers are honoured directly. Port 0 is the cross-system
/// "primary stick" alias (mirrors the Spectrum's port-0-is-default),
/// which on the C64 is the main gameport — port 2.
///
/// Returning `None` drops events on ports we don't model (paddle,
/// mouse 1351, light pen — all post-October).
fn machine_port(input_port: u8) -> Option<u8> {
    match input_port {
        2 => Some(2), // Control Port 2 (CIA1 PA, $DC00) — the main gameport
        1 => Some(1), // Control Port 1 (CIA1 PB, $DC01) — keyboard-shared
        0 => Some(2), // cross-system alias: primary stick → main gameport 2
        _ => None,
    }
}

/// Canonical C64 joystick control name for a host-level button name.
/// Returns `None` for names that don't map; the caller drops those
/// events silently.
fn canonical_control(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "up" => "UP",
        "down" => "DOWN",
        "left" => "LEFT",
        "right" => "RIGHT",
        // The C64 joystick has a single fire button; gamepad face
        // buttons all route to it so the host-side mapper stays
        // neutral about vendor labels.
        "fire" | "south" | "cross" | "button1" | "east" | "circle" | "button2" | "west"
        | "square" | "button3" | "north" | "triangle" | "button4" => "FIRE",
        _ => return None,
    })
}

/// Apply one host input event to the machine: keys land in the
/// keyboard matrix, joystick buttons land on the named control of
/// the named port. Other event kinds (mouse motion, etc.) are
/// ignored — the C64 has no mouse input surface in this runtime.
pub(crate) fn apply_input_event(machine: &mut C64, event: &InputEvent) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some((row, col)) = c64_key_position(name.as_ref()) {
                machine.keyboard_mut().set_key(row, col, *pressed);
            }
        }
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            let Some(machine_port) = machine_port(*port) else {
                return;
            };
            let Some(control) = canonical_control(name.as_ref()) else {
                return;
            };
            let _ = machine.set_joystick_control(machine_port, control, *pressed);
        }
        _ => {}
    }
}

/// Look up `(row, col)` in the C64 keyboard matrix for a host-level
/// key name. Returns `None` for keys that don't have a C64 keycap;
/// the caller silently drops those events.
fn c64_key_position(name: &str) -> Option<(u8, u8)> {
    let upper = name.to_ascii_uppercase();
    match upper.as_str() {
        "DELETE" | "DEL" | "BACKSPACE" => Some((0, 0)),
        "RETURN" | "ENTER" => Some((0, 1)),
        "RIGHT" | "CRSRRIGHT" => Some((0, 2)),
        "F7" => Some((0, 3)),
        "F1" => Some((0, 4)),
        "F3" => Some((0, 5)),
        "F5" => Some((0, 6)),
        "DOWN" | "CRSRDOWN" => Some((0, 7)),
        "3" => Some((1, 0)),
        "W" => Some((1, 1)),
        "A" => Some((1, 2)),
        "4" => Some((1, 3)),
        "Z" => Some((1, 4)),
        "S" => Some((1, 5)),
        "E" => Some((1, 6)),
        "LSHIFT" => Some((1, 7)),
        "5" => Some((2, 0)),
        "R" => Some((2, 1)),
        "D" => Some((2, 2)),
        "6" => Some((2, 3)),
        "C" => Some((2, 4)),
        "F" => Some((2, 5)),
        "T" => Some((2, 6)),
        "X" => Some((2, 7)),
        "7" => Some((3, 0)),
        "Y" => Some((3, 1)),
        "G" => Some((3, 2)),
        "8" => Some((3, 3)),
        "B" => Some((3, 4)),
        "H" => Some((3, 5)),
        "U" => Some((3, 6)),
        "V" => Some((3, 7)),
        "9" => Some((4, 0)),
        "I" => Some((4, 1)),
        "J" => Some((4, 2)),
        "0" => Some((4, 3)),
        "M" => Some((4, 4)),
        "K" => Some((4, 5)),
        "O" => Some((4, 6)),
        "N" => Some((4, 7)),
        "PLUS" => Some((5, 0)),
        "P" => Some((5, 1)),
        "L" => Some((5, 2)),
        "MINUS" => Some((5, 3)),
        "." | "PERIOD" => Some((5, 4)),
        ":" | "COLON" => Some((5, 5)),
        "@" | "AT" => Some((5, 6)),
        "," | "COMMA" => Some((5, 7)),
        "POUND" | "STERLING" => Some((6, 0)),
        "ASTERISK" | "STAR" => Some((6, 1)),
        "SEMICOLON" => Some((6, 2)),
        "HOME" => Some((6, 3)),
        "RSHIFT" => Some((6, 4)),
        "=" | "EQUALS" | "EQUAL" => Some((6, 5)),
        "UP" | "CRSRUP" => Some((6, 6)),
        "/" | "SLASH" => Some((6, 7)),
        "1" => Some((7, 0)),
        "LEFTARROW" => Some((7, 1)),
        "CTRL" | "CONTROL" => Some((7, 2)),
        "2" => Some((7, 3)),
        "SPACE" => Some((7, 4)),
        "COMMODORE" | "CBM" => Some((7, 5)),
        "Q" => Some((7, 6)),
        "RUNSTOP" | "RUN/STOP" => Some((7, 7)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_input_event, canonical_control, machine_port};
    use emu198x_shell::InputEvent;
    use machine_commodore_c64::{C64, C64Config, C64Model};
    use std::borrow::Cow;

    fn make_machine() -> C64 {
        let mut kernal = [0xEA; 0x2000];
        kernal[0x1FFC] = 0x00;
        kernal[0x1FFD] = 0xE0;
        C64::new(C64Config {
            model: C64Model::PalBreadbin,
            kernal_rom: &kernal,
            basic_rom: &[0xBB; 0x2000],
            character_rom: &[0xCC; 0x1000],
        })
        .expect("stub ROM sizes valid")
    }

    fn button_event(port: u8, name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: Cow::Owned(name.to_string()),
            pressed,
        }
    }

    #[test]
    fn input_port_zero_maps_to_main_gameport_two() {
        assert_eq!(machine_port(0), Some(2));
    }

    #[test]
    fn input_port_one_maps_to_keyboard_shared_gameport_one() {
        assert_eq!(machine_port(1), Some(1));
    }

    #[test]
    fn input_port_two_maps_to_main_gameport() {
        // Hardware-faithful: the case is silkscreened "Control Port 2".
        assert_eq!(machine_port(2), Some(2));
    }

    #[test]
    fn input_port_zero_aliases_the_main_gameport() {
        // Cross-system "primary stick" alias resolves to the main gameport.
        assert_eq!(machine_port(0), Some(2));
        assert_eq!(machine_port(0), machine_port(2));
    }

    #[test]
    fn unmapped_input_ports_are_dropped() {
        // Only the two real control ports (1, 2) and the 0 alias map; the
        // C64 has no third gameport.
        assert_eq!(machine_port(3), None);
        assert_eq!(machine_port(255), None);
    }

    #[test]
    fn gamepad_aliases_route_to_fire() {
        assert_eq!(canonical_control("south"), Some("FIRE"));
        assert_eq!(canonical_control("east"), Some("FIRE"));
        assert_eq!(canonical_control("west"), Some("FIRE"));
        assert_eq!(canonical_control("north"), Some("FIRE"));
        assert_eq!(canonical_control("cross"), Some("FIRE"));
        assert_eq!(canonical_control("circle"), Some("FIRE"));
        assert_eq!(canonical_control("button1"), Some("FIRE"));
    }

    #[test]
    fn directions_pass_through() {
        assert_eq!(canonical_control("up"), Some("UP"));
        assert_eq!(canonical_control("DOWN"), Some("DOWN"));
        assert_eq!(canonical_control("left"), Some("LEFT"));
        assert_eq!(canonical_control("Right"), Some("RIGHT"));
    }

    #[test]
    fn unknown_control_returns_none() {
        assert_eq!(canonical_control("axis_x"), None);
        assert_eq!(canonical_control(""), None);
    }

    /// Port-0 events should pull CIA1 PA bits low when active. Fire =
    /// bit 4 (0x10), so a fire press on port 0 should clear bit 4 of
    /// the input pulled into PA.
    #[test]
    fn port_zero_fire_lands_on_cia1_pa() {
        let mut m = make_machine();
        apply_input_event(&mut m, &button_event(0, "fire", true));
        // joystick_input(2) is private but we can advance one tick and
        // observe via cia1_port_b_input. Easier: walk the machine one
        // tick so the keyboard-scan path runs, then read PB.
        m.tick();
        // pb_in is the AND of keyboard scan and joystick 1 (CIA1 PB).
        // We want to confirm port 0 didn't accidentally touch PB.
        let pb = m.cia1_port_b_input();
        assert_eq!(
            pb & 0x10,
            0x10,
            "PB bit 4 should be high (port 0 → PA, not PB)"
        );
    }

    /// Port-1 events should pull CIA1 PB bits low. Fire on port 1
    /// pulls PB bit 4 low. The keyboard scan also affects PB, but we
    /// program the machine with no keys pressed.
    #[test]
    fn port_one_fire_lands_on_cia1_pb() {
        let mut m = make_machine();
        // Program CIA1 PA = all-bits-driven-high so PB sees the scan
        // for keys with PA columns all high.
        m.cpu_write(0xDC02, 0xFF);
        m.cpu_write(0xDC00, 0xFF);
        apply_input_event(&mut m, &button_event(1, "fire", true));
        m.tick();
        let pb = m.cia1_port_b_input();
        assert_eq!(pb & 0x10, 0, "port 1 fire should pull PB bit 4 low");
    }

    /// Port-2 events are the hardware-faithful main gameport (CIA1 PA),
    /// the same destination as the port-0 alias. Fire must therefore
    /// land on PA, not PB — so PB bit 4 stays high, exactly as for port 0.
    #[test]
    fn port_two_fire_lands_on_cia1_pa_like_port_zero() {
        let mut m = make_machine();
        apply_input_event(&mut m, &button_event(2, "fire", true));
        m.tick();
        let pb = m.cia1_port_b_input();
        assert_eq!(
            pb & 0x10,
            0x10,
            "port 2 fire should land on PA (main gameport), leaving PB untouched"
        );
    }
}

#[cfg(test)]
mod key_position_tests {
    use super::c64_key_position;

    /// Spec invariant: every key the native shell sends has a matrix
    /// position. Catches regressions where a rename or re-cased lookup
    /// silently stops mapping a host key. The C64 has no dedicated
    /// LEFT key — host LEFT is handled at the shell layer as
    /// RSHIFT+RIGHT — so it's deliberately absent from this list.
    #[test]
    fn input_mapping_covers_native_shell_keys() {
        for key in [
            "RETURN",
            "BACKSPACE",
            "SPACE",
            "LSHIFT",
            "RSHIFT",
            "CTRL",
            "RUNSTOP",
            "F1",
            "F3",
            "F5",
            "F7",
            "UP",
            "DOWN",
            "RIGHT",
            "A",
            "Z",
            "0",
            "9",
            ":",
            "@",
            ",",
            ".",
        ] {
            assert!(
                c64_key_position(key).is_some(),
                "native shell key {key:?} should map"
            );
        }
        // Case-insensitive lookups are part of the contract — the
        // native shell sends lowercase keycodes for character keys
        // and reserves uppercase for special names.
        assert_eq!(c64_key_position("delete"), Some((0, 0)));
        assert_eq!(c64_key_position("right"), Some((0, 2)));
        assert_eq!(c64_key_position("f1"), Some((0, 4)));
        assert_eq!(c64_key_position("f7"), Some((0, 3)));
        assert_eq!(c64_key_position("plus"), Some((5, 0)));
        assert_eq!(c64_key_position("home"), Some((6, 3)));
        assert_eq!(c64_key_position("equals"), Some((6, 5)));
        assert_eq!(c64_key_position("up"), Some((6, 6)));
        assert_eq!(c64_key_position("commodore"), Some((7, 5)));
        assert_eq!(c64_key_position("runstop"), Some((7, 7)));
        assert_eq!(c64_key_position("LEFT"), None);
        assert_eq!(c64_key_position("UNKNOWN"), None);
    }
}
