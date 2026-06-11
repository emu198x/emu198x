//! BBC Micro keyboard and joystick input mapping.
//!
//! BBC keyboard matrix is 10×8 (col × row), read via the System VIA at
//! `$FE40`. The mapping here covers the standard Model B layout.
//!
//! The BBC's two analogue joysticks split across two chips: the **fire
//! buttons** are switches on System VIA PB4 (joy 1) / PB5 (joy 2), wired here
//! from [`InputEvent::Button`] on the matching port; the **X/Y axes** are read
//! through the μPD7002 ADC (`InputEvent::Axis`, a separate path).

use emu198x_shell::InputEvent;
use machine_acorn_bbc_micro::BbcMicro;

pub(crate) fn apply_input_event(machine: &mut BbcMicro, event: &InputEvent) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some((col, row)) = key_to_matrix(name.as_ref()) {
                if *pressed {
                    machine.press_key(col, row);
                } else {
                    machine.release_key(col, row);
                }
            }
        }
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            apply_button(machine, *port, &name.to_ascii_lowercase(), *pressed);
        }
        InputEvent::Axis { port, name, value } => {
            apply_axis(machine, *port, &name.to_ascii_lowercase(), *value);
        }
        _ => {}
    }
}

/// Apply a `Button` event: a fire / trigger name drives the fire switch on the
/// joystick's port; other names are ignored.
fn apply_button(machine: &mut BbcMicro, port: u8, name: &str, pressed: bool) {
    if matches!(name, "fire" | "fire1" | "trigger" | "button") {
        machine.set_fire_button(port, pressed);
    }
}

/// Apply an `Axis` event to the μPD7002 ADC. The two joysticks map onto the
/// four ADC channels: port 1 → channels 0 (X) / 1 (Y), port 2 → channels 2 / 3.
/// The signed host axis is scaled to the 12-bit pot range (`0..=0x0FFF`, centre
/// `0x0800`). If a title reads inverted, flip the value at the source — the
/// scaling here is orientation-agnostic.
fn apply_axis(machine: &mut BbcMicro, port: u8, name: &str, value: i16) {
    let axis = match name {
        "x" | "horizontal" | "pot0" => 0,
        "y" | "vertical" | "pot1" => 1,
        _ => return,
    };
    let channel = (port.clamp(1, 2) - 1) * 2 + axis;
    machine.set_adc_channel(channel, axis_to_pot12(value));
}

/// Map a normalized signed axis value (`i16::MIN..=i16::MAX`) onto the μPD7002's
/// 12-bit pot range (`0..=0x0FFF`); `0` lands near centre (`0x0800`).
fn axis_to_pot12(value: i16) -> u16 {
    let shifted = i32::from(value) - i32::from(i16::MIN); // 0..=65535
    u16::try_from((shifted * 0x0FFF) / 65535).unwrap_or(0x0FFF)
}

#[must_use]
fn key_to_matrix(name: &str) -> Option<(usize, usize)> {
    // The authoritative BBC Micro 10×8 keyboard matrix, as `(column, row)`.
    // Internal key code = `row*16 + column` (so `column = code & 0x0F`,
    // `row = code >> 4`), matching how the machine decodes the scan code on
    // System VIA PA0-6. Verified against BeebWiki / the BBC AUG; the prior
    // table was a fictional "logical QWERTY rows" layout that never matched the
    // hardware scan, so keyboard input did not actually work.
    Some(match name.to_ascii_lowercase().as_str() {
        // Row 0 — modifiers (read directly).
        "shift" | "lshift" | "rshift" => (0, 0),
        "ctrl" | "control" => (1, 0),
        // Row 1.
        "q" => (0, 1),
        "3" => (1, 1),
        "4" => (2, 1),
        "5" => (3, 1),
        "f4" => (4, 1),
        "8" => (5, 1),
        "f7" => (6, 1),
        "-" | "minus" => (7, 1),
        "^" | "caret" => (8, 1),
        "left" | "arrowleft" => (9, 1),
        // Row 2.
        "f0" => (0, 2),
        "w" => (1, 2),
        "e" => (2, 2),
        "t" => (3, 2),
        "7" => (4, 2),
        "i" => (5, 2),
        "9" => (6, 2),
        "0" => (7, 2),
        "_" | "underscore" | "pound" => (8, 2),
        "down" | "arrowdown" => (9, 2),
        // Row 3.
        "1" => (0, 3),
        "2" => (1, 3),
        "d" => (2, 3),
        "r" => (3, 3),
        "6" => (4, 3),
        "u" => (5, 3),
        "o" => (6, 3),
        "p" => (7, 3),
        "[" | "leftbracket" => (8, 3),
        "up" | "arrowup" => (9, 3),
        // Row 4.
        "caps" | "capslock" => (0, 4),
        "a" => (1, 4),
        "x" => (2, 4),
        "f" => (3, 4),
        "y" => (4, 4),
        "j" => (5, 4),
        "k" => (6, 4),
        "@" | "at" => (7, 4),
        ":" | "colon" => (8, 4),
        "return" | "enter" => (9, 4),
        // Row 5.
        "shiftlock" => (0, 5),
        "s" => (1, 5),
        "c" => (2, 5),
        "g" => (3, 5),
        "h" => (4, 5),
        "n" => (5, 5),
        "l" => (6, 5),
        ";" | "semicolon" => (7, 5),
        "]" | "rightbracket" => (8, 5),
        "delete" | "del" | "backspace" | "bs" => (9, 5),
        // Row 6.
        "tab" => (0, 6),
        "z" => (1, 6),
        "space" | " " => (2, 6),
        "v" => (3, 6),
        "b" => (4, 6),
        "m" => (5, 6),
        "," | "comma" => (6, 6),
        "." | "period" => (7, 6),
        "/" | "slash" => (8, 6),
        "copy" => (9, 6),
        // Row 7.
        "escape" | "esc" => (0, 7),
        "f1" => (1, 7),
        "f2" => (2, 7),
        "f3" => (3, 7),
        "f5" => (4, 7),
        "f6" => (5, 7),
        "f8" => (6, 7),
        "f9" => (7, 7),
        "\\" | "backslash" => (8, 7),
        "right" | "arrowright" => (9, 7),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn make_bbc() -> BbcMicro {
        // A zero MOS ROM is enough: these tests drive input directly and never
        // run the CPU.
        BbcMicro::new(vec![0u8; 0x4000])
    }

    fn axis(port: u8, name: &str, value: i16) -> InputEvent {
        InputEvent::Axis {
            port,
            name: Cow::Owned(name.to_owned()),
            value,
        }
    }

    #[test]
    fn axis_scales_to_the_12bit_pot_range() {
        assert_eq!(axis_to_pot12(i16::MIN), 0);
        assert_eq!(axis_to_pot12(i16::MAX), 0x0FFF);
        assert!((0x07F0..=0x0810).contains(&axis_to_pot12(0)));
    }

    #[test]
    fn axes_route_to_the_right_adc_channels() {
        let mut sys = make_bbc();
        // Port 1 X/Y → channels 0/1; port 2 X/Y → channels 2/3.
        apply_input_event(&mut sys, &axis(1, "x", i16::MAX));
        apply_input_event(&mut sys, &axis(1, "y", i16::MIN));
        apply_input_event(&mut sys, &axis(2, "vertical", i16::MAX));
        assert_eq!(sys.adc_channel(0), 0x0FFF, "p1 X → ch0 max");
        assert_eq!(sys.adc_channel(1), 0, "p1 Y → ch1 min");
        assert_eq!(sys.adc_channel(3), 0x0FFF, "p2 Y → ch3 max");
        assert_eq!(sys.adc_channel(2), 0x0800, "p2 X untouched (centre)");
    }

    #[test]
    fn unknown_axis_name_is_ignored() {
        let mut sys = make_bbc();
        apply_input_event(&mut sys, &axis(1, "throttle", i16::MAX));
        assert_eq!(sys.adc_channel(0), 0x0800, "channels stay centred");
    }

    #[test]
    fn keyboard_matrix_is_the_authoritative_bbc_layout() {
        use std::collections::HashSet;
        // One canonical name per physical key — every cell must be distinct.
        let keys = [
            "shift",
            "ctrl",
            "q",
            "3",
            "4",
            "5",
            "f4",
            "8",
            "f7",
            "-",
            "^",
            "left",
            "f0",
            "w",
            "e",
            "t",
            "7",
            "i",
            "9",
            "0",
            "_",
            "down",
            "1",
            "2",
            "d",
            "r",
            "6",
            "u",
            "o",
            "p",
            "[",
            "up",
            "caps",
            "a",
            "x",
            "f",
            "y",
            "j",
            "k",
            "@",
            ":",
            "return",
            "shiftlock",
            "s",
            "c",
            "g",
            "h",
            "n",
            "l",
            ";",
            "]",
            "delete",
            "tab",
            "z",
            "space",
            "v",
            "b",
            "m",
            ",",
            ".",
            "/",
            "copy",
            "escape",
            "f1",
            "f2",
            "f3",
            "f5",
            "f6",
            "f8",
            "f9",
            "\\",
            "right",
        ];
        let mut cells = HashSet::new();
        for k in keys {
            let cell = key_to_matrix(k).expect("key resolves");
            assert!(cells.insert(cell), "`{k}` collides at {cell:?}");
        }

        // Spot-checks against the reference (column, row).
        assert_eq!(key_to_matrix("a"), Some((1, 4)));
        assert_eq!(key_to_matrix("1"), Some((0, 3)));
        assert_eq!(key_to_matrix("space"), Some((2, 6)));
        assert_eq!(key_to_matrix("return"), Some((9, 4)));
        // The four cursor keys live in column 9.
        assert_eq!(key_to_matrix("left"), Some((9, 1)));
        assert_eq!(key_to_matrix("down"), Some((9, 2)));
        assert_eq!(key_to_matrix("up"), Some((9, 3)));
        assert_eq!(key_to_matrix("right"), Some((9, 7)));

        // Regressions for #463: previously-colliding keys are now distinct,
        // and the missing function keys are present.
        assert_ne!(key_to_matrix("up"), key_to_matrix("tab"));
        assert_ne!(key_to_matrix("delete"), key_to_matrix("p"));
        assert_eq!(key_to_matrix("f4"), Some((4, 1)));
        assert_eq!(key_to_matrix("f7"), Some((6, 1)));
        assert_eq!(key_to_matrix("f0"), Some((0, 2)));
    }
}
