//! C64 keyboard / joystick / paddle / 1351-mouse input mapping.
//!
//! Splits the keyboard-matrix lookup table out of `runtime.rs` so the
//! 70+ key entries don't dominate the file. The matrix is the
//! standard PAL breadbin layout (HRM Appendix C); shifted symbols
//! land on the right keycap on a UK/US keyboard.
//!
//! `PointerMotion` / `PointerButton` events tagged `mouse-1` drive a
//! 1351 proportional mouse when one is plugged in (see the runtime's
//! `set_mouse_1351`); they are dropped otherwise.
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
/// Returning `None` drops button/axis events on ports we don't model.
/// Paddles share this port mapping. (The 1351 mouse rides the separate
/// `PointerMotion`/`PointerButton` path, not this joystick/paddle map.)
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
/// the named port, and — when a 1351 mouse is plugged in — pointer
/// motion and buttons drive it. Pointer events are dropped when no
/// mouse is attached.
pub(crate) fn apply_input_event(machine: &mut C64, event: &InputEvent) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some((row, col)) = c64_key_position(name.as_ref()) {
                machine.keyboard_mut().set_key(row, col, *pressed);
            } else if name.as_ref().eq_ignore_ascii_case("restore") {
                // RESTORE is not on the matrix — it pulses the CPU /NMI.
                machine.set_restore(*pressed);
            } else if name.as_ref().eq_ignore_ascii_case("freeze") {
                // A freeze-cartridge button (Action Replay, Final Cartridge III)
                // — also off-matrix, latching the cartridge's /NMI.
                machine.set_cart_freeze(*pressed);
            }
        }
        InputEvent::PointerMotion { device, dx, dy } if device.as_ref() == "mouse-1" => {
            if let Some(port) = attached_mouse_port(machine) {
                machine.move_mouse_1351(port, *dx, *dy);
            }
        }
        InputEvent::PointerButton {
            device,
            button,
            pressed,
        } if device.as_ref() == "mouse-1" => {
            if let Some(port) = attached_mouse_port(machine) {
                machine.set_mouse_1351_button(port, button.as_ref(), *pressed);
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
        InputEvent::Axis { port, name, value } => {
            let Some(machine_port) = machine_port(*port) else {
                return;
            };
            let Some(axis) = paddle_axis(&name.to_ascii_lowercase()) else {
                return;
            };
            // The paddle pot is an 8-bit reading on the selected control port;
            // the SID surfaces it at POTX/POTY once the CIA #1 mux selects that
            // port. Flip at the source if a title reads inverted.
            let _ = machine.set_paddle(machine_port, axis, axis_to_pot8(*value));
        }
        _ => {}
    }
}

/// The control port a 1351 mouse is plugged into, checking the case-labelled
/// ports in order (1, then 2). The C64 wires a mouse to whichever port the
/// user plugs it into; the host has a single pointer, so the first attached
/// port receives its motion and buttons.
fn attached_mouse_port(machine: &C64) -> Option<u8> {
    [1, 2]
        .into_iter()
        .find(|&port| machine.has_mouse_1351(port))
}

/// Map an axis name to a paddle pot index: 0 = X (POTX), 1 = Y (POTY).
fn paddle_axis(name: &str) -> Option<u8> {
    match name {
        "x" | "horizontal" | "potx" | "pot0" => Some(0),
        "y" | "vertical" | "poty" | "pot1" => Some(1),
        _ => None,
    }
}

/// Scale a normalized signed axis (`i16::MIN..=i16::MAX`) onto the 8-bit paddle
/// pot range (`0..=255`); `0` lands near centre (`128`).
fn axis_to_pot8(value: i16) -> u8 {
    let shifted = i32::from(value) - i32::from(i16::MIN); // 0..=65535
    u8::try_from((shifted * 255) / 65535).unwrap_or(255)
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

/// Returns `true` when `name` maps to a C64 keyboard-matrix position.
///
/// Used by the `press_key` MCP tool to reject a typo with a clear error
/// rather than silently dropping the keystroke (the live input path drops
/// unknown names on purpose, which is the wrong behaviour for a tool the
/// curriculum author drives by hand).
#[must_use]
pub fn key_name_is_valid(name: &str) -> bool {
    // RESTORE and the cartridge FREEZE button are real keys but live on the
    // /NMI line, not the matrix.
    c64_key_position(name).is_some()
        || name.eq_ignore_ascii_case("restore")
        || name.eq_ignore_ascii_case("freeze")
}

/// C64 keycap names for the letters `A`–`Z`, indexed `0..26`.
const LETTER_KEYS: [&str; 26] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z",
];

/// C64 keycap names for the digits `0`–`9`, indexed `0..10`.
const DIGIT_KEYS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// Maps one source character to the C64 key-name chord that produces it
/// on a freshly-booted machine.
///
/// The default charset renders *unshifted* letter keys in upper case, so
/// both `'A'` and `'a'` map to the bare letter keycap — exactly what BASIC
/// keywords and `INPUT` responses expect. Characters that need a shifted
/// keycap (`"`, `?`, brackets, `$`) return a two-key chord led by
/// `lshift`. Returns `None` for characters with no single-keystroke C64
/// equivalent; the caller skips those, matching the Spectrum `type_string`
/// behaviour.
#[must_use]
pub fn keys_for_char(ch: char) -> Option<Vec<&'static str>> {
    let upper = ch.to_ascii_uppercase();
    if upper.is_ascii_uppercase() {
        return Some(vec![LETTER_KEYS[(upper as u8 - b'A') as usize]]);
    }
    if ch.is_ascii_digit() {
        return Some(vec![DIGIT_KEYS[(ch as u8 - b'0') as usize]]);
    }
    Some(match ch {
        ' ' => vec!["space"],
        '\n' | '\r' => vec!["return"],
        '.' => vec!["."],
        ',' => vec![","],
        ':' => vec![":"],
        ';' => vec!["semicolon"],
        '/' => vec!["/"],
        '=' => vec!["="],
        '+' => vec!["plus"],
        '-' => vec!["minus"],
        '@' => vec!["at"],
        '*' => vec!["asterisk"],
        // The shifted number row, in keycap order. Five of these were here and
        // four were not, which is the shape a hand-written table decays into:
        // the ones somebody needed got added, the rest were never missed.
        '!' => vec!["lshift", "1"],
        '"' => vec!["lshift", "2"],
        '#' => vec!["lshift", "3"],
        '$' => vec!["lshift", "4"],
        '%' => vec!["lshift", "5"],
        '&' => vec!["lshift", "6"],
        '\'' => vec!["lshift", "7"],
        '(' => vec!["lshift", "8"],
        ')' => vec!["lshift", "9"],
        // Shifted punctuation. `<` and `>` are the costly omissions: dropping
        // one usually leaves valid BASIC, so `H=48+C+(C>9)*57` was entered as
        // `H=48+C+(C9)*57` — `C9` being a legal variable of value 0 — and ran
        // without complaint, producing a plausible wrong answer. See #916.
        '<' => vec!["lshift", ","],
        '>' => vec!["lshift", "."],
        '[' => vec!["lshift", ":"],
        ']' => vec!["lshift", "semicolon"],
        '?' => vec!["lshift", "/"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{apply_input_event, axis_to_pot8, canonical_control, machine_port, paddle_axis};
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

    fn axis_event(port: u8, name: &str, value: i16) -> InputEvent {
        InputEvent::Axis {
            port,
            name: Cow::Owned(name.to_string()),
            value,
        }
    }

    fn pointer_motion(device: &str, dx: i32, dy: i32) -> InputEvent {
        InputEvent::PointerMotion {
            device: Cow::Owned(device.to_string()),
            dx,
            dy,
        }
    }

    fn pointer_button(device: &str, button: &str, pressed: bool) -> InputEvent {
        InputEvent::PointerButton {
            device: Cow::Owned(device.to_string()),
            button: Cow::Owned(button.to_string()),
            pressed,
        }
    }

    #[test]
    fn pointer_events_drive_an_attached_1351_mouse() {
        let mut m = make_machine();
        m.cpu_write(0xDC02, 0xFF); // CIA1 DDRA: PA6/PA7 outputs
        m.attach_mouse_1351(2);
        m.cpu_write(0xDC00, 0x80); // select control port 2

        // Motion accumulates into the POT counters (POTX = (dx & 0x7f) + 0x40).
        apply_input_event(&mut m, &pointer_motion("mouse-1", 5, 0));
        assert_eq!(m.cpu_read(0xD419), 0x45, "pointer motion drives POTX");

        // Left button pulls FIRE (bit 4) low on the main gameport.
        apply_input_event(&mut m, &pointer_button("mouse-1", "left", true));
        assert_eq!(m.cpu_read(0xDC00) & 0x10, 0x00, "left button → FIRE low");
    }

    #[test]
    fn pointer_events_are_dropped_without_a_mouse() {
        let mut m = make_machine();
        m.cpu_write(0xDC02, 0xFF);
        m.cpu_write(0xDC00, 0x80);
        // No mouse attached — the pot stays open and nothing panics.
        apply_input_event(&mut m, &pointer_motion("mouse-1", 5, 0));
        assert_eq!(m.cpu_read(0xD419), 0xFF, "no mouse → POT line open");
        // A foreign device id is ignored even with a mouse present.
        m.attach_mouse_1351(2);
        apply_input_event(&mut m, &pointer_motion("mouse-2", 9, 0));
        assert_eq!(m.cpu_read(0xD419), 0x40, "unknown device leaves the mouse");
    }

    #[test]
    fn axis_scales_to_the_8bit_pot_range() {
        assert_eq!(axis_to_pot8(i16::MIN), 0);
        assert_eq!(axis_to_pot8(i16::MAX), 255);
        assert!((120..=136).contains(&axis_to_pot8(0)));
    }

    #[test]
    fn paddle_axis_names_map_to_pot_indices() {
        assert_eq!(paddle_axis("x"), Some(0));
        assert_eq!(paddle_axis("vertical"), Some(1));
        assert_eq!(paddle_axis("throttle"), None);
    }

    #[test]
    fn axis_events_drive_the_sid_pots_on_the_selected_port() {
        let mut m = make_machine();
        m.cpu_write(0xDC02, 0xFF); // CIA1 DDRA: PA6/PA7 outputs

        // Port 2 X/Y to the extremes.
        apply_input_event(&mut m, &axis_event(2, "x", i16::MAX));
        apply_input_event(&mut m, &axis_event(2, "y", i16::MIN));

        // Select control port 2 (PA7 = 1 → mux mask 2).
        m.cpu_write(0xDC00, 0x80);
        assert_eq!(m.cpu_read(0xD419), 255, "port 2 X → POTX max");
        assert_eq!(m.cpu_read(0xD41A), 0, "port 2 Y → POTY min");

        // The port-0 alias lands on the same main gameport (2).
        apply_input_event(&mut m, &axis_event(0, "x", i16::MIN));
        assert_eq!(m.cpu_read(0xD419), 0, "port 0 aliases gameport 2 X");
    }

    #[test]
    fn unknown_axis_name_is_dropped() {
        let mut m = make_machine();
        m.cpu_write(0xDC02, 0xFF);
        m.cpu_write(0xDC00, 0x80);
        // A centred pot reads ~128 by default; an unknown axis leaves it.
        apply_input_event(&mut m, &axis_event(2, "throttle", i16::MAX));
        assert_eq!(m.cpu_read(0xD419), 0xFF, "open line unchanged");
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

    #[test]
    fn key_name_validity_matches_matrix() {
        assert!(super::key_name_is_valid("a"));
        assert!(super::key_name_is_valid("RETURN"));
        assert!(super::key_name_is_valid("space"));
        assert!(super::key_name_is_valid("runstop"));
        assert!(!super::key_name_is_valid("escape"));
        assert!(!super::key_name_is_valid(""));
    }

    #[test]
    fn letters_map_to_unshifted_keycaps_in_either_case() {
        assert_eq!(super::keys_for_char('A'), Some(vec!["a"]));
        assert_eq!(super::keys_for_char('a'), Some(vec!["a"]));
        assert_eq!(super::keys_for_char('Z'), Some(vec!["z"]));
        assert_eq!(super::keys_for_char('z'), Some(vec!["z"]));
    }

    #[test]
    fn digits_space_and_newline_map() {
        assert_eq!(super::keys_for_char('0'), Some(vec!["0"]));
        assert_eq!(super::keys_for_char('9'), Some(vec!["9"]));
        assert_eq!(super::keys_for_char(' '), Some(vec!["space"]));
        assert_eq!(super::keys_for_char('\n'), Some(vec!["return"]));
        assert_eq!(super::keys_for_char('\r'), Some(vec!["return"]));
    }

    #[test]
    fn shifted_punctuation_returns_a_chord() {
        assert_eq!(super::keys_for_char('"'), Some(vec!["lshift", "2"]));
        assert_eq!(super::keys_for_char('?'), Some(vec!["lshift", "/"]));
    }

    #[test]
    fn every_printable_ascii_character_on_the_keyboard_maps() {
        // The table was hand-written and eight characters were missing from
        // it, which `type_string` skipped in silence — see #916. Enumerating
        // the whole keycap set is what stops that recurring: a gap fails here
        // rather than in somebody's BASIC listing weeks later.
        //
        // Excluded deliberately: characters with no C64 keycap and no Shift
        // chord that reaches them. Backslash, braces, backtick, tilde,
        // underscore, caret and bar simply are not on the keyboard.
        const UNREACHABLE: &str = "\\{}`~_^|";
        for ch in (0x20u8..0x7F).map(char::from) {
            if UNREACHABLE.contains(ch) {
                assert_eq!(
                    super::keys_for_char(ch),
                    None,
                    "{ch:?} is not on a C64 keyboard and should report so"
                );
                continue;
            }
            assert!(
                super::keys_for_char(ch).is_some(),
                "{ch:?} is a C64 keycap but has no mapping — type_string would \
                 drop it silently"
            );
        }
    }

    #[test]
    fn the_comparison_operators_map() {
        // Called out separately because these are the ones that corrupt
        // quietly: removing `>` from `(C>9)` leaves `(C9)`, a legal variable
        // reference, so the program runs and gives a wrong answer instead of a
        // syntax error.
        assert_eq!(super::keys_for_char('<'), Some(vec!["lshift", ","]));
        assert_eq!(super::keys_for_char('>'), Some(vec!["lshift", "."]));
    }

    #[test]
    fn unmapped_character_is_skipped() {
        assert_eq!(super::keys_for_char('£'), None);
        assert_eq!(super::keys_for_char('~'), None);
    }
}
