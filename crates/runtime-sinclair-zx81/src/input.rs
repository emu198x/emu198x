//! ZX81 keyboard input mapping.

use emu198x_shell::InputEvent;
use machine_sinclair_zx81::{Zx81, Zx81Key};

pub(crate) fn apply_input_event(machine: &mut Zx81, event: &InputEvent) {
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
fn key_from_name(name: &str) -> Option<Zx81Key> {
    Some(match name.to_ascii_lowercase().as_str() {
        "shift" | "lshift" | "rshift" => Zx81Key::Shift,
        "newline" | "enter" | "return" => Zx81Key::Newline,
        "space" | " " => Zx81Key::Space,
        "." | "period" => Zx81Key::Period,
        "0" => Zx81Key::N0,
        "1" => Zx81Key::N1,
        "2" => Zx81Key::N2,
        "3" => Zx81Key::N3,
        "4" => Zx81Key::N4,
        "5" => Zx81Key::N5,
        "6" => Zx81Key::N6,
        "7" => Zx81Key::N7,
        "8" => Zx81Key::N8,
        "9" => Zx81Key::N9,
        "a" => Zx81Key::A,
        "b" => Zx81Key::B,
        "c" => Zx81Key::C,
        "d" => Zx81Key::D,
        "e" => Zx81Key::E,
        "f" => Zx81Key::F,
        "g" => Zx81Key::G,
        "h" => Zx81Key::H,
        "i" => Zx81Key::I,
        "j" => Zx81Key::J,
        "k" => Zx81Key::K,
        "l" => Zx81Key::L,
        "m" => Zx81Key::M,
        "n" => Zx81Key::N,
        "o" => Zx81Key::O,
        "p" => Zx81Key::P,
        "q" => Zx81Key::Q,
        "r" => Zx81Key::R,
        "s" => Zx81Key::S,
        "t" => Zx81Key::T,
        "u" => Zx81Key::U,
        "v" => Zx81Key::V,
        "w" => Zx81Key::W,
        "x" => Zx81Key::X,
        "y" => Zx81Key::Y,
        "z" => Zx81Key::Z,
        _ => return None,
    })
}
