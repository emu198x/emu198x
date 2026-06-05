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
