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
    Some(match name.to_ascii_lowercase().as_str() {
        // Function and control keys
        "escape" | "esc" => (0, 7),
        "f1" => (1, 7),
        "f2" => (2, 7),
        "f3" => (3, 7),
        "f5" => (4, 7),
        "f6" => (5, 7),
        "f8" => (6, 7),
        "f9" => (7, 7),
        // Digits + symbols
        "1" => (0, 6),
        "2" => (1, 6),
        "3" => (2, 6),
        "4" => (3, 6),
        "5" => (4, 6),
        "6" => (5, 6),
        "7" => (6, 6),
        "8" => (7, 6),
        "9" => (8, 6),
        "0" => (9, 6),
        // QWERTY row
        "q" => (0, 5),
        "w" => (1, 5),
        "e" => (2, 5),
        "r" => (3, 5),
        "t" => (4, 5),
        "y" => (5, 5),
        "u" => (6, 5),
        "i" => (7, 5),
        "o" => (8, 5),
        "p" => (9, 5),
        // ASDF row
        "a" => (1, 4),
        "s" => (2, 4),
        "d" => (3, 4),
        "f" => (4, 4),
        "g" => (5, 4),
        "h" => (6, 4),
        "j" => (7, 4),
        "k" => (8, 4),
        "l" => (9, 4),
        ";" | "semicolon" => (8, 3),
        ":" => (9, 3),
        // ZXCV row
        "shift" | "lshift" | "rshift" => (0, 1),
        "z" => (1, 3),
        "x" => (2, 3),
        "c" => (3, 3),
        "v" => (4, 3),
        "b" => (5, 3),
        "n" => (6, 3),
        "m" => (7, 3),
        "," | "comma" => (8, 2),
        "." | "period" => (9, 2),
        // Special keys
        "space" | " " => (2, 1),
        "return" | "enter" => (9, 1),
        "caps" | "capslock" => (4, 1),
        "ctrl" | "control" => (1, 1),
        "tab" => (3, 1),
        "delete" | "del" | "backspace" | "bs" => (9, 5),
        "up" | "arrowup" => (3, 1),
        "down" | "arrowdown" => (2, 2),
        "left" | "arrowleft" => (1, 1),
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
}
