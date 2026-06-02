//! PET keyboard input mapping.

use emu198x_shell::InputEvent;
use machine_commodore_pet::{Pet, PetKey};

pub(crate) fn apply_input_event(machine: &mut Pet, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event
        && let Some(key) = key_from_name(name.as_ref()) {
            if *pressed {
                machine.press_key(key);
            } else {
                machine.release_key(key);
            }
        }
}

#[must_use]
fn key_from_name(name: &str) -> Option<PetKey> {
    Some(match name.to_ascii_lowercase().as_str() {
        "return" | "enter" => PetKey::Return,
        "space" | " " => PetKey::Space,
        "escape" | "esc" => PetKey::Escape,
        "shift" | "lshift" => PetKey::LeftShift,
        "rshift" => PetKey::RightShift,
        "tab" => PetKey::Tab,
        "del" | "delete" | "backspace" | "bs" => PetKey::Del,
        "home" => PetKey::Home,
        "stop" | "runstop" => PetKey::RunStop,
        "up" | "arrowup" => PetKey::CursorUp,
        "down" | "arrowdown" => PetKey::CursorDown,
        "left" | "arrowleft" => PetKey::CursorLeft,
        "." | "period" => PetKey::Period,
        "," | "comma" => PetKey::Comma,
        ";" | "semicolon" => PetKey::SemiColon,
        ":" => PetKey::Colon,
        "/" | "slash" => PetKey::Slash,
        "\\" | "backslash" => PetKey::BackSlash,
        "[" | "leftbracket" => PetKey::BracketOpen,
        "]" | "rightbracket" => PetKey::BracketClose,
        "-" | "minus" => PetKey::Minus,
        "+" => PetKey::Plus,
        "=" | "equals" => PetKey::Equals,
        "@" | "at" => PetKey::At,
        "0" => PetKey::N0,
        "1" => PetKey::N1,
        "2" => PetKey::N2,
        "3" => PetKey::N3,
        "4" => PetKey::N4,
        "5" => PetKey::N5,
        "6" => PetKey::N6,
        "7" => PetKey::N7,
        "8" => PetKey::N8,
        "9" => PetKey::N9,
        "a" => PetKey::A,
        "b" => PetKey::B,
        "c" => PetKey::C,
        "d" => PetKey::D,
        "e" => PetKey::E,
        "f" => PetKey::F,
        "g" => PetKey::G,
        "h" => PetKey::H,
        "i" => PetKey::I,
        "j" => PetKey::J,
        "k" => PetKey::K,
        "l" => PetKey::L,
        "m" => PetKey::M,
        "n" => PetKey::N,
        "o" => PetKey::O,
        "p" => PetKey::P,
        "q" => PetKey::Q,
        "r" => PetKey::R,
        "s" => PetKey::S,
        "t" => PetKey::T,
        "u" => PetKey::U,
        "v" => PetKey::V,
        "w" => PetKey::W,
        "x" => PetKey::X,
        "y" => PetKey::Y,
        "z" => PetKey::Z,
        _ => return None,
    })
}
