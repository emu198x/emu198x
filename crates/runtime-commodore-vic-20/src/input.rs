//! VIC-20 keyboard input mapping.

use emu198x_shell::InputEvent;
use machine_commodore_vic_20::{Vic20, Vic20Key};

pub(crate) fn apply_input_event(machine: &mut Vic20, event: &InputEvent) {
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
fn key_from_name(name: &str) -> Option<Vic20Key> {
    Some(match name.to_ascii_lowercase().as_str() {
        "return" | "enter" => Vic20Key::Return,
        "space" | " " => Vic20Key::Space,
        "stop" | "runstop" | "run/stop" => Vic20Key::Stop,
        "delete" | "del" | "backspace" => Vic20Key::Delete,
        "home" | "clr" => Vic20Key::Home,
        "ctrl" | "control" => Vic20Key::Ctrl,
        "commodore" | "cbm" => Vic20Key::Commodore,
        "lshift" | "shift" => Vic20Key::ShiftLeft,
        "rshift" => Vic20Key::ShiftRight,
        "crsr-right" | "right" => Vic20Key::CursorRight,
        "crsr-down" | "down" => Vic20Key::CursorDown,
        "f1" => Vic20Key::F1,
        "f3" => Vic20Key::F3,
        "f5" => Vic20Key::F5,
        "f7" => Vic20Key::F7,
        "0" => Vic20Key::Num0,
        "1" => Vic20Key::Num1,
        "2" => Vic20Key::Num2,
        "3" => Vic20Key::Num3,
        "4" => Vic20Key::Num4,
        "5" => Vic20Key::Num5,
        "6" => Vic20Key::Num6,
        "7" => Vic20Key::Num7,
        "8" => Vic20Key::Num8,
        "9" => Vic20Key::Num9,
        "+" | "plus" => Vic20Key::Plus,
        "-" | "minus" => Vic20Key::Minus,
        "*" | "asterisk" => Vic20Key::Asterisk,
        "/" | "slash" => Vic20Key::Slash,
        "=" | "equal" => Vic20Key::Equal,
        ":" | "colon" => Vic20Key::Colon,
        ";" | "semicolon" => Vic20Key::Semicolon,
        "," | "comma" => Vic20Key::Comma,
        "." | "period" => Vic20Key::Period,
        "@" | "at" => Vic20Key::At,
        "pound" | "sterling" => Vic20Key::Pound,
        "arrowup" | "up" => Vic20Key::ArrowUp,
        "arrowleft" | "left" => Vic20Key::ArrowLeft,
        "a" => Vic20Key::A,
        "b" => Vic20Key::B,
        "c" => Vic20Key::C,
        "d" => Vic20Key::D,
        "e" => Vic20Key::E,
        "f" => Vic20Key::F,
        "g" => Vic20Key::G,
        "h" => Vic20Key::H,
        "i" => Vic20Key::I,
        "j" => Vic20Key::J,
        "k" => Vic20Key::K,
        "l" => Vic20Key::L,
        "m" => Vic20Key::M,
        "n" => Vic20Key::N,
        "o" => Vic20Key::O,
        "p" => Vic20Key::P,
        "q" => Vic20Key::Q,
        "r" => Vic20Key::R,
        "s" => Vic20Key::S,
        "t" => Vic20Key::T,
        "u" => Vic20Key::U,
        "v" => Vic20Key::V,
        "w" => Vic20Key::W,
        "x" => Vic20Key::X,
        "y" => Vic20Key::Y,
        "z" => Vic20Key::Z,
        _ => return None,
    })
}
