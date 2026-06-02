//! MTX keyboard input mapping.

use emu198x_shell::InputEvent;
use machine_memotech_mtx::{Mtx, MtxKey};

pub(crate) fn apply_input_event(machine: &mut Mtx, event: &InputEvent) {
    if let InputEvent::Key { name, pressed } = event {
        if let Some(key) = key_from_name(name.as_ref()) {
            if *pressed {
                machine.press_key(key);
            } else {
                machine.release_key(key);
            }
        }
    }
}

#[must_use]
fn key_from_name(name: &str) -> Option<MtxKey> {
    Some(match name.to_ascii_lowercase().as_str() {
        "1" => MtxKey::N1,
        "2" => MtxKey::N2,
        "3" => MtxKey::N3,
        "4" => MtxKey::N4,
        "5" => MtxKey::N5,
        "6" => MtxKey::N6,
        "7" => MtxKey::N7,
        "8" => MtxKey::N8,
        "9" => MtxKey::N9,
        "0" => MtxKey::N0,
        "-" | "minus" => MtxKey::Minus,
        "=" | "equals" => MtxKey::Equal,
        "\\" | "backslash" => MtxKey::Backslash,
        "[" | "leftbracket" => MtxKey::BracketLeft,
        "]" | "rightbracket" => MtxKey::BracketRight,
        ";" | "semicolon" => MtxKey::Semicolon,
        "'" | "quote" | "apostrophe" => MtxKey::Quote,
        "," | "comma" => MtxKey::Comma,
        "." | "period" => MtxKey::Period,
        "/" | "slash" => MtxKey::Slash,
        "pound" | "#" => MtxKey::Pound,
        "delete" | "del" | "backspace" | "bs" => MtxKey::Delete,
        "ctrl" | "control" | "lctrl" => MtxKey::CtrlLeft,
        "shift" | "lshift" | "rshift" => MtxKey::Shift,
        "enter" | "return" => MtxKey::Enter,
        "escape" | "esc" => MtxKey::Escape,
        "tab" => MtxKey::Tab,
        "caps" | "capslock" => MtxKey::CapsLock,
        "space" | " " => MtxKey::Space,
        "f1" => MtxKey::F1,
        "f2" => MtxKey::F2,
        "f3" => MtxKey::F3,
        "f4" => MtxKey::F4,
        "f5" => MtxKey::F5,
        "left" | "arrowleft" => MtxKey::Left,
        "right" | "arrowright" => MtxKey::Right,
        "up" | "arrowup" => MtxKey::Up,
        "a" => MtxKey::A,
        "b" => MtxKey::B,
        "c" => MtxKey::C,
        "d" => MtxKey::D,
        "e" => MtxKey::E,
        "f" => MtxKey::F,
        "g" => MtxKey::G,
        "h" => MtxKey::H,
        "i" => MtxKey::I,
        "j" => MtxKey::J,
        "k" => MtxKey::K,
        "l" => MtxKey::L,
        "m" => MtxKey::M,
        "n" => MtxKey::N,
        "o" => MtxKey::O,
        "p" => MtxKey::P,
        "q" => MtxKey::Q,
        "r" => MtxKey::R,
        "s" => MtxKey::S,
        "t" => MtxKey::T,
        "u" => MtxKey::U,
        "v" => MtxKey::V,
        "w" => MtxKey::W,
        "x" => MtxKey::X,
        "y" => MtxKey::Y,
        "z" => MtxKey::Z,
        _ => return None,
    })
}
