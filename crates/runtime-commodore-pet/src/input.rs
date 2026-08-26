//! PET keyboard input mapping.

use emu198x_shell::InputEvent;
use machine_commodore_pet::{Pet, PetKey};

pub(crate) fn apply_input_event(machine: &mut Pet, event: &InputEvent) {
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
fn key_from_name(name: &str) -> Option<PetKey> {
    Some(match name.to_ascii_lowercase().as_str() {
        "return" | "enter" => PetKey::Return,
        "space" | " " => PetKey::Space,
        "right" | "crsr-right" => PetKey::CursorRight,
        "." | "period" => PetKey::Period,
        "," | "comma" => PetKey::Comma,
        ";" | "semicolon" => PetKey::Semicolon,
        ":" | "colon" => PetKey::Colon,
        "/" | "slash" => PetKey::Slash,
        "*" | "asterisk" => PetKey::Asterisk,
        "+" | "plus" => PetKey::Plus,
        "=" | "equal" | "equals" => PetKey::Equal,
        "-" | "minus" => PetKey::Minus,
        "@" | "at" => PetKey::At,
        "?" | "question" => PetKey::Question,
        "<" | "less" => PetKey::Less,
        ">" | "greater" => PetKey::Greater,
        "!" | "exclaim" => PetKey::Exclaim,
        "#" | "hash" => PetKey::Hash,
        "$" | "dollar" => PetKey::Dollar,
        "%" | "percent" => PetKey::Percent,
        "&" | "ampersand" => PetKey::Ampersand,
        "'" | "apostrophe" => PetKey::Apostrophe,
        "\"" | "quote" => PetKey::Quote,
        "(" | "parenleft" => PetKey::ParenLeft,
        ")" | "parenright" => PetKey::ParenRight,
        "0" => PetKey::Num0,
        "1" => PetKey::Num1,
        "2" => PetKey::Num2,
        "3" => PetKey::Num3,
        "4" => PetKey::Num4,
        "5" => PetKey::Num5,
        "6" => PetKey::Num6,
        "7" => PetKey::Num7,
        "8" => PetKey::Num8,
        "9" => PetKey::Num9,
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

/// Whether this machine's input layer can deliver `name`.
///
/// This is the same lookup [`apply_input_event`] performs before injecting a
/// keystroke, exposed so the shared keyboard can refuse a character the
/// machine cannot type instead of counting one it silently dropped (#1196).
pub(crate) fn knows_key_name(name: &str) -> bool {
    key_from_name(name).is_some()
}
