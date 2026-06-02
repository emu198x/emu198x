//! VIC-20 keyboard input mapping.

use emu198x_shell::InputEvent;
use machine_commodore_vic_20::{Vic20, Vic20Key};

pub(crate) fn apply_input_event(machine: &mut Vic20, event: &InputEvent) {
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
fn key_from_name(name: &str) -> Option<Vic20Key> {
    Some(match name.to_ascii_lowercase().as_str() {
        "return" | "enter" => Vic20Key::Return,
        "space" | " " => Vic20Key::Space,
        "stop" | "esc" | "escape" => Vic20Key::Stop,
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
