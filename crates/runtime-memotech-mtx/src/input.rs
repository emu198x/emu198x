//! MTX keyboard and joystick input mapping.
//!
//! Joystick input arrives as [`InputEvent::Button`] on a numbered port. The
//! MTX joysticks share the keyboard matrix sense lines, so the machine maps
//! each control to a fixed matrix position; this module mirrors the held
//! state per port in a [`ControllerCache`] and re-applies it via
//! [`Mtx::set_joystick`] on each event.

use emu198x_shell::InputEvent;
use machine_memotech_mtx::{Mtx, MtxKey};

/// Host-side mirror of one MTX control port: four directions plus the fire
/// button, re-applied via [`Mtx::set_joystick`] on every event.
#[derive(Clone, Copy, Debug, Default)]
struct JoystickCache {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    fire: bool,
}

impl JoystickCache {
    /// Record a digital control by name. Returns `true` when the name maps to
    /// a joystick direction or the fire button.
    fn set_control(&mut self, name: &str, pressed: bool) -> bool {
        match name {
            "up" | "arrowup" => self.up = pressed,
            "down" | "arrowdown" => self.down = pressed,
            "left" | "arrowleft" => self.left = pressed,
            "right" | "arrowright" => self.right = pressed,
            "fire" | "fire1" | "trigger" | "button" => self.fire = pressed,
            _ => return false,
        }
        true
    }
}

/// Host-side mirror of both MTX control ports (1 and 2), indexed `port - 1`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControllerCache {
    ports: [JoystickCache; 2],
}

impl ControllerCache {
    /// Apply a `Button` event for `port` (1 or 2): record the control and push
    /// the whole port state to the machine. Out-of-range ports clamp to the
    /// valid pair, matching [`Mtx::set_joystick`].
    fn apply_button(&mut self, machine: &mut Mtx, port: u8, name: &str, pressed: bool) {
        let port = port.clamp(1, 2);
        let cache = &mut self.ports[usize::from(port - 1)];
        if cache.set_control(name, pressed) {
            machine.set_joystick(
                port,
                cache.up,
                cache.down,
                cache.left,
                cache.right,
                cache.fire,
            );
        }
    }
}

/// Apply one host input event. `Key` events drive the keyboard matrix;
/// `Button` events drive the joystick on their numbered port.
pub(crate) fn apply_input_event(
    machine: &mut Mtx,
    cache: &mut ControllerCache,
    event: &InputEvent,
) {
    match event {
        InputEvent::Key { name, pressed } => {
            if let Some(key) = key_from_name(name.as_ref()) {
                if *pressed {
                    machine.press_key(key);
                } else {
                    machine.release_key(key);
                }
            }
        }
        InputEvent::Button {
            port,
            name,
            pressed,
        } => {
            cache.apply_button(machine, *port, &name.to_ascii_lowercase(), *pressed);
        }
        _ => {}
    }
}

#[must_use]
fn key_from_name(name: &str) -> Option<MtxKey> {
    Some(match name.to_ascii_lowercase().as_str() {
        // Digits.
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
        // Punctuation keys (unshifted legends). The MTX has no dedicated `=`,
        // `'` or `#` key — those are shifted forms — so they are not listed.
        "-" | "minus" => MtxKey::Minus,
        "\\" | "backslash" => MtxKey::Backslash,
        "^" | "caret" => MtxKey::Caret,
        "@" | "at" => MtxKey::At,
        "[" | "leftbracket" => MtxKey::BracketLeft,
        "]" | "rightbracket" => MtxKey::BracketRight,
        ";" | "semicolon" => MtxKey::Semicolon,
        ":" | "colon" => MtxKey::Colon,
        "," | "comma" => MtxKey::Comma,
        "." | "period" => MtxKey::Period,
        "/" | "slash" => MtxKey::Slash,
        "_" | "underscore" => MtxKey::Underscore,
        // Modifiers and editing.
        "delete" | "del" => MtxKey::Delete,
        "backspace" | "bs" => MtxKey::Backspace,
        "ctrl" | "control" | "lctrl" => MtxKey::Ctrl,
        "shift" | "lshift" => MtxKey::ShiftLeft,
        "rshift" => MtxKey::ShiftRight,
        "enter" | "return" => MtxKey::Enter,
        "linefeed" | "lf" => MtxKey::LineFeed,
        "escape" | "esc" => MtxKey::Escape,
        "tab" => MtxKey::Tab,
        "caps" | "capslock" | "alphalock" => MtxKey::CapsLock,
        "space" | " " => MtxKey::Space,
        // Cursor keys (numeric-keypad legends).
        "left" | "arrowleft" => MtxKey::Left,
        "right" | "arrowright" => MtxKey::Right,
        "up" | "arrowup" => MtxKey::Up,
        "down" | "arrowdown" => MtxKey::Down,
        // Other numeric-keypad keys.
        "home" => MtxKey::Home,
        "insert" | "ins" => MtxKey::Insert,
        "page" | "pagedown" => MtxKey::Page,
        "break" => MtxKey::Break,
        "eol" | "endofline" => MtxKey::EndOfLine,
        "cls" | "keypadenter" | "kpenter" => MtxKey::KeypadEnter,
        // Function keys.
        "f1" => MtxKey::F1,
        "f2" => MtxKey::F2,
        "f3" => MtxKey::F3,
        "f4" => MtxKey::F4,
        "f5" => MtxKey::F5,
        "f6" => MtxKey::F6,
        "f7" => MtxKey::F7,
        "f8" => MtxKey::F8,
        // Letters.
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

#[cfg(test)]
mod tests {
    use super::*;
    use machine_memotech_mtx::MtxModel;
    use std::borrow::Cow;

    fn make_mtx() -> Mtx {
        let mut rom = vec![0u8; 0x4000];
        rom[0x0000] = 0x18; // JR -2 trap at reset
        rom[0x0001] = 0xFE;
        Mtx::new(rom, MtxModel::Mtx500).expect("init")
    }

    fn button(port: u8, name: &str, pressed: bool) -> InputEvent {
        InputEvent::Button {
            port,
            name: Cow::Owned(name.to_owned()),
            pressed,
        }
    }

    fn key(name: &str, pressed: bool) -> InputEvent {
        InputEvent::Key {
            name: Cow::Owned(name.to_owned()),
            pressed,
        }
    }

    /// True when the key named `name` pulls sense `bit` of drive column `col`
    /// low — i.e. it resolves to matrix cell `(col, bit)`. Bits 0-7 read on the
    /// `$05` low byte, bits 8-9 on the `$06` high byte.
    fn key_closes_cell(name: &str, col: usize, bit: u8) -> bool {
        let mut m = make_mtx();
        let mut cache = ControllerCache::default();
        apply_input_event(&mut m, &mut cache, &key(name, true));
        let (lo, hi) = m.sense(!(1 << col));
        if bit < 8 {
            lo & (1 << bit) == 0
        } else {
            hi & (1 << (bit - 8)) == 0
        }
    }

    #[test]
    fn key_names_close_their_real_hardware_matrix_cells() {
        // Letters/digits/space/enter at their MAME-sourced positions; the bug
        // was that these resolved to fabricated cells (typing ABCDE → @uf11).
        assert!(key_closes_cell("a", 5, 0), "a → col5 bit0");
        assert!(key_closes_cell("1", 0, 0), "1 → col0 bit0");
        assert!(key_closes_cell("2", 1, 1), "2 → col1 bit1");
        assert!(key_closes_cell("space", 7, 8), "space → col7 bit8 ($06)");
        assert!(key_closes_cell("enter", 5, 6), "enter → col5 bit6");
        assert!(key_closes_cell("tab", 2, 8), "tab → col2 bit8 ($06)");

        // The cursor keys, including the previously-missing Down (#465).
        assert!(key_closes_cell("up", 2, 7), "up → col2 bit7");
        assert!(key_closes_cell("left", 3, 7), "left → col3 bit7");
        assert!(key_closes_cell("right", 4, 7), "right → col4 bit7");
        assert!(key_closes_cell("down", 6, 7), "down → col6 bit7");
        assert!(
            key_closes_cell("arrowdown", 6, 7),
            "arrowdown alias → col6 bit7"
        );
    }

    #[test]
    fn releasing_a_key_restores_its_sense_line() {
        let mut m = make_mtx();
        let mut cache = ControllerCache::default();
        apply_input_event(&mut m, &mut cache, &key("down", true));
        assert_eq!(m.sense(!(1 << 6)).0 & 0x80, 0, "down held");
        apply_input_event(&mut m, &mut cache, &key("down", false));
        assert_eq!(m.sense(!(1 << 6)).0 & 0x80, 0x80, "down released");
    }

    #[test]
    fn button_events_drive_the_joystick_matrix_lines() {
        let mut m = make_mtx();
        let mut cache = ControllerCache::default();

        // Player 1 up → column 2, sense bit 7 (low byte, port $05).
        apply_input_event(&mut m, &mut cache, &button(1, "up", true));
        assert_eq!(
            m.sense(!(1 << 2)).0 & 0x80,
            0,
            "P1 up pulls col 2 bit 7 low"
        );

        // Player 2 fire → column 7, sense bit 8 (high byte, port $06).
        apply_input_event(&mut m, &mut cache, &button(2, "fire", true));
        assert_eq!(
            m.sense(!(1 << 7)).1 & 0x01,
            0,
            "P2 fire pulls col 7 bit 8 low"
        );

        // Releasing P1 up restores its line; P2 fire stays held.
        apply_input_event(&mut m, &mut cache, &button(1, "up", false));
        assert_eq!(m.sense(!(1 << 2)).0 & 0x80, 0x80, "P1 up released");
        assert_eq!(m.sense(!(1 << 7)).1 & 0x01, 0, "P2 fire still held");
    }
}

/// Whether this machine's input layer can deliver `name`.
///
/// This is the same lookup [`apply_input_event`] performs before injecting a
/// keystroke, exposed so the shared keyboard can refuse a character the
/// machine cannot type instead of counting one it silently dropped (#1196).
pub(crate) fn knows_key_name(name: &str) -> bool {
    key_from_name(name).is_some()
}
