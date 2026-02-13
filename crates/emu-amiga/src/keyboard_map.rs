//! Host keyboard → Amiga raw keycode mapping (US layout).
//!
//! Returns raw keycodes for key-down. Key-up is handled by the caller.

use winit::keyboard::KeyCode;

/// Left shift raw keycode.
pub const SHIFT_CODE: u8 = 0x60;
/// Return key raw keycode.
pub const KEY_RETURN: u8 = 0x44;
/// F1 raw keycode.
pub const KEY_F1: u8 = 0x50;

/// Key mapping for a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharKey {
    pub code: u8,
    pub shift: bool,
}

/// Map a host key to an Amiga raw keycode.
///
/// Returns `None` for unmapped keys.
#[must_use]
pub fn map_keycode(key: KeyCode) -> Option<u8> {
    match key {
        // Top row / punctuation
        KeyCode::Backquote => Some(0x00),
        KeyCode::Digit1 => Some(0x01),
        KeyCode::Digit2 => Some(0x02),
        KeyCode::Digit3 => Some(0x03),
        KeyCode::Digit4 => Some(0x04),
        KeyCode::Digit5 => Some(0x05),
        KeyCode::Digit6 => Some(0x06),
        KeyCode::Digit7 => Some(0x07),
        KeyCode::Digit8 => Some(0x08),
        KeyCode::Digit9 => Some(0x09),
        KeyCode::Digit0 => Some(0x0A),
        KeyCode::Minus => Some(0x0B),
        KeyCode::Equal => Some(0x0C),
        KeyCode::Backslash => Some(0x0D),
        KeyCode::IntlYen => Some(0x0E),

        // Letters
        KeyCode::KeyQ => Some(0x10),
        KeyCode::KeyW => Some(0x11),
        KeyCode::KeyE => Some(0x12),
        KeyCode::KeyR => Some(0x13),
        KeyCode::KeyT => Some(0x14),
        KeyCode::KeyY => Some(0x15),
        KeyCode::KeyU => Some(0x16),
        KeyCode::KeyI => Some(0x17),
        KeyCode::KeyO => Some(0x18),
        KeyCode::KeyP => Some(0x19),
        KeyCode::BracketLeft => Some(0x1A),
        KeyCode::BracketRight => Some(0x1B),

        KeyCode::KeyA => Some(0x20),
        KeyCode::KeyS => Some(0x21),
        KeyCode::KeyD => Some(0x22),
        KeyCode::KeyF => Some(0x23),
        KeyCode::KeyG => Some(0x24),
        KeyCode::KeyH => Some(0x25),
        KeyCode::KeyJ => Some(0x26),
        KeyCode::KeyK => Some(0x27),
        KeyCode::KeyL => Some(0x28),
        KeyCode::Semicolon => Some(0x29),
        KeyCode::Quote => Some(0x2A),
        KeyCode::IntlBackslash => Some(0x2B),

        KeyCode::IntlRo => Some(0x30),
        KeyCode::KeyZ => Some(0x31),
        KeyCode::KeyX => Some(0x32),
        KeyCode::KeyC => Some(0x33),
        KeyCode::KeyV => Some(0x34),
        KeyCode::KeyB => Some(0x35),
        KeyCode::KeyN => Some(0x36),
        KeyCode::KeyM => Some(0x37),
        KeyCode::Comma => Some(0x38),
        KeyCode::Period => Some(0x39),
        KeyCode::Slash => Some(0x3A),

        // Space / editing
        KeyCode::Space => Some(0x40),
        KeyCode::Backspace => Some(0x41),
        KeyCode::Tab => Some(0x42),
        KeyCode::Enter => Some(0x44),
        KeyCode::Escape => Some(0x45),
        KeyCode::Delete => Some(0x46),
        KeyCode::Insert => Some(0x47),
        KeyCode::PageUp => Some(0x48),
        KeyCode::PageDown => Some(0x49),

        // Cursor keys
        KeyCode::ArrowUp => Some(0x4C),
        KeyCode::ArrowDown => Some(0x4D),
        KeyCode::ArrowRight => Some(0x4E),
        KeyCode::ArrowLeft => Some(0x4F),

        // Function keys
        KeyCode::F1 => Some(0x50),
        KeyCode::F2 => Some(0x51),
        KeyCode::F3 => Some(0x52),
        KeyCode::F4 => Some(0x53),
        KeyCode::F5 => Some(0x54),
        KeyCode::F6 => Some(0x55),
        KeyCode::F7 => Some(0x56),
        KeyCode::F8 => Some(0x57),
        KeyCode::F9 => Some(0x58),
        KeyCode::F10 => Some(0x59),
        KeyCode::F11 => Some(0x4B),
        KeyCode::F12 => Some(0x6F),

        // Numpad
        KeyCode::Numpad0 => Some(0x0F),
        KeyCode::Numpad1 => Some(0x1D),
        KeyCode::Numpad2 => Some(0x1E),
        KeyCode::Numpad3 => Some(0x1F),
        KeyCode::Numpad4 => Some(0x2D),
        KeyCode::Numpad5 => Some(0x2E),
        KeyCode::Numpad6 => Some(0x2F),
        KeyCode::Numpad7 => Some(0x3D),
        KeyCode::Numpad8 => Some(0x3E),
        KeyCode::Numpad9 => Some(0x3F),
        KeyCode::NumpadDecimal | KeyCode::NumpadComma => Some(0x3C),
        KeyCode::NumpadEnter => Some(0x43),
        KeyCode::NumpadSubtract => Some(0x4A),
        KeyCode::NumpadParenLeft => Some(0x5A),
        KeyCode::NumpadParenRight => Some(0x5B),
        KeyCode::NumpadDivide => Some(0x5C),
        KeyCode::NumpadMultiply => Some(0x5D),
        KeyCode::NumpadAdd => Some(0x5E),

        // Special / modifiers
        KeyCode::Help => Some(0x5F),
        KeyCode::ShiftLeft => Some(0x60),
        KeyCode::ShiftRight => Some(0x61),
        KeyCode::CapsLock => Some(0x62),
        KeyCode::ControlLeft | KeyCode::ControlRight => Some(0x63),
        KeyCode::AltLeft => Some(0x64),
        KeyCode::AltRight => Some(0x65),
        KeyCode::SuperLeft => Some(0x66),
        KeyCode::SuperRight => Some(0x67),
        KeyCode::ContextMenu => Some(0x6B),
        KeyCode::PrintScreen => Some(0x6D),
        KeyCode::Pause => Some(0x6E),
        KeyCode::Home => Some(0x70),
        KeyCode::End => Some(0x71),

        _ => None,
    }
}

/// Map a printable character to a raw keycode + shift flag.
#[must_use]
pub fn char_to_key(ch: char) -> Option<CharKey> {
    use CharKey as CK;

    let direct = |code| CK { code, shift: false };
    let shifted = |code| CK { code, shift: true };

    Some(match ch {
        // Whitespace / control
        ' ' => direct(0x40),
        '\t' => direct(0x42),
        '\n' | '\r' => direct(KEY_RETURN),
        '\x08' => direct(0x41),
        '\x1b' => direct(0x45),

        // Letters
        'a' => direct(0x20),
        'b' => direct(0x35),
        'c' => direct(0x33),
        'd' => direct(0x22),
        'e' => direct(0x12),
        'f' => direct(0x23),
        'g' => direct(0x24),
        'h' => direct(0x25),
        'i' => direct(0x17),
        'j' => direct(0x26),
        'k' => direct(0x27),
        'l' => direct(0x28),
        'm' => direct(0x37),
        'n' => direct(0x36),
        'o' => direct(0x18),
        'p' => direct(0x19),
        'q' => direct(0x10),
        'r' => direct(0x13),
        's' => direct(0x21),
        't' => direct(0x14),
        'u' => direct(0x16),
        'v' => direct(0x34),
        'w' => direct(0x11),
        'x' => direct(0x32),
        'y' => direct(0x15),
        'z' => direct(0x31),

        'A' => shifted(0x20),
        'B' => shifted(0x35),
        'C' => shifted(0x33),
        'D' => shifted(0x22),
        'E' => shifted(0x12),
        'F' => shifted(0x23),
        'G' => shifted(0x24),
        'H' => shifted(0x25),
        'I' => shifted(0x17),
        'J' => shifted(0x26),
        'K' => shifted(0x27),
        'L' => shifted(0x28),
        'M' => shifted(0x37),
        'N' => shifted(0x36),
        'O' => shifted(0x18),
        'P' => shifted(0x19),
        'Q' => shifted(0x10),
        'R' => shifted(0x13),
        'S' => shifted(0x21),
        'T' => shifted(0x14),
        'U' => shifted(0x16),
        'V' => shifted(0x34),
        'W' => shifted(0x11),
        'X' => shifted(0x32),
        'Y' => shifted(0x15),
        'Z' => shifted(0x31),

        // Digits
        '1' => direct(0x01),
        '2' => direct(0x02),
        '3' => direct(0x03),
        '4' => direct(0x04),
        '5' => direct(0x05),
        '6' => direct(0x06),
        '7' => direct(0x07),
        '8' => direct(0x08),
        '9' => direct(0x09),
        '0' => direct(0x0A),

        // Shifted digits
        '!' => shifted(0x01),
        '@' => shifted(0x02),
        '#' => shifted(0x03),
        '$' => shifted(0x04),
        '%' => shifted(0x05),
        '^' => shifted(0x06),
        '&' => shifted(0x07),
        '*' => shifted(0x08),
        '(' => shifted(0x09),
        ')' => shifted(0x0A),

        // Punctuation
        '`' => direct(0x00),
        '~' => shifted(0x00),
        '-' => direct(0x0B),
        '_' => shifted(0x0B),
        '=' => direct(0x0C),
        '+' => shifted(0x0C),
        '\\' => direct(0x0D),
        '|' => shifted(0x0D),
        '[' => direct(0x1A),
        '{' => shifted(0x1A),
        ']' => direct(0x1B),
        '}' => shifted(0x1B),
        ';' => direct(0x29),
        ':' => shifted(0x29),
        '\'' => direct(0x2A),
        '"' => shifted(0x2A),
        ',' => direct(0x38),
        '<' => shifted(0x38),
        '.' => direct(0x39),
        '>' => shifted(0x39),
        '/' => direct(0x3A),
        '?' => shifted(0x3A),

        _ => return None,
    })
}

/// Parse a key name or raw code into an Amiga raw keycode.
#[must_use]
pub fn parse_key_name(name: &str) -> Option<u8> {
    let name = name.trim();
    if let Some(hex) = name.strip_prefix("0x").or_else(|| name.strip_prefix("0X")) {
        return u8::from_str_radix(hex, 16).ok().filter(|v| *v < 0x80);
    }
    if let Ok(value) = name.parse::<u8>() {
        if value < 0x80 {
            return Some(value);
        }
    }

    match name.to_lowercase().as_str() {
        "`" | "backquote" | "grave" => Some(0x00),
        "1" => Some(0x01),
        "2" => Some(0x02),
        "3" => Some(0x03),
        "4" => Some(0x04),
        "5" => Some(0x05),
        "6" => Some(0x06),
        "7" => Some(0x07),
        "8" => Some(0x08),
        "9" => Some(0x09),
        "0" => Some(0x0A),
        "-" | "minus" => Some(0x0B),
        "=" | "equal" => Some(0x0C),
        "\\" | "backslash" => Some(0x0D),
        "yen" => Some(0x0E),

        "q" => Some(0x10),
        "w" => Some(0x11),
        "e" => Some(0x12),
        "r" => Some(0x13),
        "t" => Some(0x14),
        "y" => Some(0x15),
        "u" => Some(0x16),
        "i" => Some(0x17),
        "o" => Some(0x18),
        "p" => Some(0x19),
        "[" | "bracketleft" => Some(0x1A),
        "]" | "bracketright" => Some(0x1B),

        "a" => Some(0x20),
        "s" => Some(0x21),
        "d" => Some(0x22),
        "f" => Some(0x23),
        "g" => Some(0x24),
        "h" => Some(0x25),
        "j" => Some(0x26),
        "k" => Some(0x27),
        "l" => Some(0x28),
        ";" | "semicolon" => Some(0x29),
        "'" | "quote" | "apostrophe" => Some(0x2A),
        "intl1" => Some(0x2B),

        "intl2" => Some(0x30),
        "z" => Some(0x31),
        "x" => Some(0x32),
        "c" => Some(0x33),
        "v" => Some(0x34),
        "b" => Some(0x35),
        "n" => Some(0x36),
        "m" => Some(0x37),
        "," | "comma" => Some(0x38),
        "." | "period" | "dot" => Some(0x39),
        "/" | "slash" => Some(0x3A),

        "space" => Some(0x40),
        "backspace" => Some(0x41),
        "tab" => Some(0x42),
        "enter" | "return" => Some(0x44),
        "numpadenter" | "npenter" => Some(0x43),
        "esc" | "escape" => Some(0x45),
        "del" | "delete" => Some(0x46),
        "insert" | "ins" => Some(0x47),
        "pageup" | "pgup" => Some(0x48),
        "pagedown" | "pgdn" => Some(0x49),
        "np-" | "numpad-" | "numpadsubtract" => Some(0x4A),
        "f11" => Some(0x4B),
        "up" | "arrowup" => Some(0x4C),
        "down" | "arrowdown" => Some(0x4D),
        "right" | "arrowright" => Some(0x4E),
        "left" | "arrowleft" => Some(0x4F),

        "f1" => Some(0x50),
        "f2" => Some(0x51),
        "f3" => Some(0x52),
        "f4" => Some(0x53),
        "f5" => Some(0x54),
        "f6" => Some(0x55),
        "f7" => Some(0x56),
        "f8" => Some(0x57),
        "f9" => Some(0x58),
        "f10" => Some(0x59),
        "np(" | "numpad(" | "numpadparenleft" => Some(0x5A),
        "np)" | "numpad)" | "numpadparenright" => Some(0x5B),
        "np/" | "numpad/" | "numpaddivide" => Some(0x5C),
        "np*" | "numpad*" | "numpadmultiply" => Some(0x5D),
        "np+" | "numpad+" | "numpadadd" => Some(0x5E),
        "help" => Some(0x5F),

        "lshift" | "left_shift" => Some(0x60),
        "rshift" | "right_shift" => Some(0x61),
        "capslock" => Some(0x62),
        "ctrl" | "control" => Some(0x63),
        "lalt" | "left_alt" => Some(0x64),
        "ralt" | "right_alt" => Some(0x65),
        "lamiga" | "left_amiga" | "left_super" => Some(0x66),
        "ramiga" | "right_amiga" | "right_super" => Some(0x67),
        "menu" | "contextmenu" => Some(0x6B),
        "printscreen" => Some(0x6D),
        "break" | "pause" => Some(0x6E),
        "f12" => Some(0x6F),
        "home" => Some(0x70),
        "end" => Some(0x71),

        "np0" | "numpad0" => Some(0x0F),
        "np1" | "numpad1" => Some(0x1D),
        "np2" | "numpad2" => Some(0x1E),
        "np3" | "numpad3" => Some(0x1F),
        "np4" | "numpad4" => Some(0x2D),
        "np5" | "numpad5" => Some(0x2E),
        "np6" | "numpad6" => Some(0x2F),
        "np7" | "numpad7" => Some(0x3D),
        "np8" | "numpad8" => Some(0x3E),
        "np9" | "numpad9" => Some(0x3F),
        "np." | "numpad." | "numpaddecimal" => Some(0x3C),

        _ => None,
    }
}
