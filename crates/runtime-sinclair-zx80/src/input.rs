//! ZX80 keyboard input mapping.

use emu198x_shell::InputEvent;
use machine_sinclair_zx80::{Zx80, Zx80Key};

pub(crate) fn apply_input_event(machine: &mut Zx80, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some(key) = key_from_name(name.as_ref())
    {
        if *pressed {
            machine.press_key(key);
        } else {
            machine.release_key(key);
        }
    }
}

#[must_use]
fn key_from_name(name: &str) -> Option<Zx80Key> {
    Some(match name.to_ascii_lowercase().as_str() {
        "shift" | "lshift" | "rshift" => Zx80Key::Shift,
        "newline" | "enter" | "return" => Zx80Key::Newline,
        "space" | " " => Zx80Key::Space,
        "." | "period" => Zx80Key::Period,
        "0" => Zx80Key::N0,
        "1" => Zx80Key::N1,
        "2" => Zx80Key::N2,
        "3" => Zx80Key::N3,
        "4" => Zx80Key::N4,
        "5" => Zx80Key::N5,
        "6" => Zx80Key::N6,
        "7" => Zx80Key::N7,
        "8" => Zx80Key::N8,
        "9" => Zx80Key::N9,
        "a" => Zx80Key::A,
        "b" => Zx80Key::B,
        "c" => Zx80Key::C,
        "d" => Zx80Key::D,
        "e" => Zx80Key::E,
        "f" => Zx80Key::F,
        "g" => Zx80Key::G,
        "h" => Zx80Key::H,
        "i" => Zx80Key::I,
        "j" => Zx80Key::J,
        "k" => Zx80Key::K,
        "l" => Zx80Key::L,
        "m" => Zx80Key::M,
        "n" => Zx80Key::N,
        "o" => Zx80Key::O,
        "p" => Zx80Key::P,
        "q" => Zx80Key::Q,
        "r" => Zx80Key::R,
        "s" => Zx80Key::S,
        "t" => Zx80Key::T,
        "u" => Zx80Key::U,
        "v" => Zx80Key::V,
        "w" => Zx80Key::W,
        "x" => Zx80Key::X,
        "y" => Zx80Key::Y,
        "z" => Zx80Key::Z,
        _ => return None,
    })
}
