//! Minimal windowed runner for the Amiga machine core.
//!
//! Scope: video output only (no host audio yet). Loads a Kickstart ROM and
//! optionally inserts an ADF into DF0:, then continuously runs the machine and
//! displays the raw 320x256 framebuffer.

#![allow(clippy::cast_possible_truncation)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use machine_amiga::format_adf::Adf;
use machine_amiga::{
    Amiga, AmigaConfig, AmigaModel, commodore_denise_ocs,
};
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const FB_WIDTH: u32 = commodore_denise_ocs::FB_WIDTH;
const FB_HEIGHT: u32 = commodore_denise_ocs::FB_HEIGHT;
const SCALE: u32 = 3;
const FRAME_DURATION: Duration = Duration::from_millis(20); // PAL ~50 Hz

// Amiga raw keycodes (US keyboard positional defaults)
const AK_SPACE: u8 = 0x40;
const AK_TAB: u8 = 0x42;
const AK_RETURN: u8 = 0x44;
const AK_ESCAPE: u8 = 0x45;
const AK_BACKSPACE: u8 = 0x41;
const AK_DELETE: u8 = 0x46;
const AK_CURSOR_UP: u8 = 0x4C;
const AK_CURSOR_DOWN: u8 = 0x4D;
const AK_CURSOR_RIGHT: u8 = 0x4E;
const AK_CURSOR_LEFT: u8 = 0x4F;
const AK_LSHIFT: u8 = 0x60;
const AK_RSHIFT: u8 = 0x61;
const AK_CAPSLOCK: u8 = 0x62;
const AK_CTRL: u8 = 0x63;
const AK_LALT: u8 = 0x64;
const AK_RALT: u8 = 0x65;
const AK_LAMIGA: u8 = 0x66;
const AK_RAMIGA: u8 = 0x67;

#[derive(Debug, Clone, Copy)]
struct ActiveKeyMapping {
    raw_keycode: u8,
    synthetic_left_shift: bool,
}

struct CliArgs {
    rom_path: PathBuf,
    adf_path: Option<PathBuf>,
}

fn print_usage_and_exit(code: i32) -> ! {
    eprintln!("Usage: amiga-runner [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --rom <file>   Kickstart ROM file (or use AMIGA_KS13_ROM env var)");
    eprintln!("  --adf <file>   Optional ADF disk image to insert into DF0:");
    eprintln!("  -h, --help     Show this help");
    process::exit(code);
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut rom_path: Option<PathBuf> = None;
    let mut adf_path: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--rom" => {
                i += 1;
                rom_path = args.get(i).map(PathBuf::from);
            }
            "--adf" => {
                i += 1;
                adf_path = args.get(i).map(PathBuf::from);
            }
            "-h" | "--help" => print_usage_and_exit(0),
            other => {
                eprintln!("Unknown argument: {other}");
                print_usage_and_exit(1);
            }
        }
        i += 1;
    }

    let rom_path = rom_path
        .or_else(|| std::env::var_os("AMIGA_KS13_ROM").map(PathBuf::from))
        .unwrap_or_else(|| {
            eprintln!("No Kickstart ROM specified.");
            print_usage_and_exit(1);
        });

    CliArgs { rom_path, adf_path }
}

fn make_amiga(cli: &CliArgs) -> Amiga {
    let kickstart = match std::fs::read(&cli.rom_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!(
                "Failed to read Kickstart ROM {}: {e}",
                cli.rom_path.display()
            );
            process::exit(1);
        }
    };

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A500,
        kickstart,
    });

    if let Some(adf_path) = &cli.adf_path {
        let adf_bytes = match std::fs::read(adf_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read ADF {}: {e}", adf_path.display());
                process::exit(1);
            }
        };
        let adf = match Adf::from_bytes(adf_bytes) {
            Ok(adf) => adf,
            Err(e) => {
                eprintln!("Invalid ADF {}: {e}", adf_path.display());
                process::exit(1);
            }
        };
        amiga.insert_disk(adf);
        eprintln!("Inserted disk: {}", adf_path.display());
    }

    eprintln!("Loaded Kickstart ROM: {}", cli.rom_path.display());
    amiga
}

struct App {
    amiga: Amiga,
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    last_frame_time: Instant,
    active_keys: HashMap<KeyCode, ActiveKeyMapping>,
    host_left_shift_down: bool,
    host_right_shift_down: bool,
}

impl App {
    fn new(amiga: Amiga) -> Self {
        Self {
            amiga,
            window: None,
            pixels: None,
            last_frame_time: Instant::now(),
            active_keys: HashMap::new(),
            host_left_shift_down: false,
            host_right_shift_down: false,
        }
    }

    fn host_shift_down(&self) -> bool {
        self.host_left_shift_down || self.host_right_shift_down
    }

    fn send_amiga_key(&mut self, raw_keycode: u8, pressed: bool) {
        self.amiga.key_event(raw_keycode, pressed);
    }

    fn update_host_shift_state(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::ShiftLeft => self.host_left_shift_down = pressed,
            KeyCode::ShiftRight => self.host_right_shift_down = pressed,
            _ => {}
        }
    }

    fn resolve_key_press(&self, code: KeyCode, logical_key: &Key) -> Option<ActiveKeyMapping> {
        if let Some(raw_keycode) = map_special_physical_key(code, logical_key) {
            return Some(ActiveKeyMapping {
                raw_keycode,
                synthetic_left_shift: false,
            });
        }

        if let Some((raw_keycode, needs_shift)) = map_logical_char_key(logical_key) {
            return Some(ActiveKeyMapping {
                raw_keycode,
                synthetic_left_shift: needs_shift && !self.host_shift_down(),
            });
        }

        map_printable_physical_key(code).map(|raw_keycode| ActiveKeyMapping {
            raw_keycode,
            synthetic_left_shift: false,
        })
    }

    fn handle_keyboard_input(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let pressed = event.state == ElementState::Pressed;

        // Runner hotkey: keep F12 reserved for quit so Escape remains usable in the Amiga.
        if code == KeyCode::F12 && pressed {
            event_loop.exit();
            return;
        }

        self.update_host_shift_state(code, pressed);

        if pressed {
            if event.repeat || self.active_keys.contains_key(&code) {
                return;
            }

            let Some(mapping) = self.resolve_key_press(code, &event.logical_key) else {
                return;
            };

            if mapping.synthetic_left_shift {
                self.send_amiga_key(AK_LSHIFT, true);
            }
            self.send_amiga_key(mapping.raw_keycode, true);
            self.active_keys.insert(code, mapping);
            return;
        }

        let Some(mapping) = self.active_keys.remove(&code) else {
            return;
        };
        self.send_amiga_key(mapping.raw_keycode, false);
        if mapping.synthetic_left_shift {
            self.send_amiga_key(AK_LSHIFT, false);
        }
    }

    fn update_pixels(&mut self) {
        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };

        let frame = pixels.frame_mut();
        let fb = self.amiga.framebuffer();

        for (i, &argb) in fb.iter().enumerate() {
            let o = i * 4;
            frame[o] = ((argb >> 16) & 0xFF) as u8; // R
            frame[o + 1] = ((argb >> 8) & 0xFF) as u8; // G
            frame[o + 2] = (argb & 0xFF) as u8; // B
            frame[o + 3] = ((argb >> 24) & 0xFF) as u8; // A
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let size = winit::dpi::LogicalSize::new(FB_WIDTH * SCALE, FB_HEIGHT * SCALE);
        let attrs = WindowAttributes::default()
            .with_title("Amiga Runner (A500/OCS)")
            .with_inner_size(size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window: &'static Window = Box::leak(Box::new(window));
                let inner = window.inner_size();
                let surface = SurfaceTexture::new(inner.width, inner.height, window);
                let pixels = match Pixels::new(FB_WIDTH, FB_HEIGHT, surface) {
                    Ok(pixels) => pixels,
                    Err(e) => {
                        eprintln!("Failed to create pixels surface: {e}");
                        event_loop.exit();
                        return;
                    }
                };

                self.pixels = Some(pixels);
                self.window = Some(window);
            }
            Err(e) => {
                eprintln!("Failed to create window: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_keyboard_input(event_loop, event);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= FRAME_DURATION {
                    self.amiga.run_frame();
                    self.update_pixels();
                    self.last_frame_time = now;
                }

                if let Some(pixels) = self.pixels.as_ref()
                    && let Err(e) = pixels.render()
                {
                    eprintln!("Render error: {e}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window {
            window.request_redraw();
        }
    }
}

fn map_special_physical_key(code: KeyCode, logical_key: &Key) -> Option<u8> {
    let raw = match code {
        KeyCode::Space => AK_SPACE,
        KeyCode::Tab => AK_TAB,
        KeyCode::Enter => AK_RETURN,
        KeyCode::NumpadEnter => 0x43,
        KeyCode::Escape => AK_ESCAPE,
        KeyCode::Backspace => AK_BACKSPACE,
        KeyCode::Delete => AK_DELETE,
        KeyCode::ArrowUp => AK_CURSOR_UP,
        KeyCode::ArrowDown => AK_CURSOR_DOWN,
        KeyCode::ArrowRight => AK_CURSOR_RIGHT,
        KeyCode::ArrowLeft => AK_CURSOR_LEFT,
        KeyCode::F1 => 0x50,
        KeyCode::F2 => 0x51,
        KeyCode::F3 => 0x52,
        KeyCode::F4 => 0x53,
        KeyCode::F5 => 0x54,
        KeyCode::F6 => 0x55,
        KeyCode::F7 => 0x56,
        KeyCode::F8 => 0x57,
        KeyCode::F9 => 0x58,
        KeyCode::F10 => 0x59,
        KeyCode::ShiftLeft => AK_LSHIFT,
        KeyCode::ShiftRight => AK_RSHIFT,
        KeyCode::CapsLock => AK_CAPSLOCK,
        KeyCode::ControlLeft | KeyCode::ControlRight => AK_CTRL,
        KeyCode::AltLeft => AK_LALT,
        KeyCode::AltRight => {
            if matches!(logical_key, Key::Named(NamedKey::AltGraph)) {
                return None;
            }
            AK_RALT
        }
        KeyCode::SuperLeft => AK_LAMIGA,
        KeyCode::SuperRight => AK_RAMIGA,
        KeyCode::Numpad0 => 0x0F,
        KeyCode::Numpad1 => 0x1D,
        KeyCode::Numpad2 => 0x1E,
        KeyCode::Numpad3 => 0x1F,
        KeyCode::Numpad4 => 0x2D,
        KeyCode::Numpad5 => 0x2E,
        KeyCode::Numpad6 => 0x2F,
        KeyCode::Numpad7 => 0x3D,
        KeyCode::Numpad8 => 0x3E,
        KeyCode::Numpad9 => 0x3F,
        KeyCode::NumpadDecimal => 0x3C,
        KeyCode::NumpadSubtract => 0x4A,
        KeyCode::NumpadAdd => 0x5E,
        KeyCode::NumpadDivide => 0x5C,
        KeyCode::NumpadMultiply => 0x5D,
        KeyCode::NumpadParenLeft => 0x5A,
        KeyCode::NumpadParenRight => 0x5B,
        _ => return None,
    };
    Some(raw)
}

fn map_logical_char_key(logical_key: &Key) -> Option<(u8, bool)> {
    let Key::Character(text) = logical_key else {
        return None;
    };

    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    map_char_to_amiga_key(ch)
}

fn map_char_to_amiga_key(ch: char) -> Option<(u8, bool)> {
    let lowered = ch.to_ascii_lowercase();
    let is_uppercase_ascii = ch.is_ascii_alphabetic() && ch.is_ascii_uppercase();

    let (raw, needs_shift) = match lowered {
        '`' => (0x00, false),
        '1' => (0x01, false),
        '2' => (0x02, false),
        '3' => (0x03, false),
        '4' => (0x04, false),
        '5' => (0x05, false),
        '6' => (0x06, false),
        '7' => (0x07, false),
        '8' => (0x08, false),
        '9' => (0x09, false),
        '0' => (0x0A, false),
        '-' => (0x0B, false),
        '=' => (0x0C, false),
        '\\' => (0x0D, false),
        'q' => (0x10, false),
        'w' => (0x11, false),
        'e' => (0x12, false),
        'r' => (0x13, false),
        't' => (0x14, false),
        'y' => (0x15, false),
        'u' => (0x16, false),
        'i' => (0x17, false),
        'o' => (0x18, false),
        'p' => (0x19, false),
        '[' => (0x1A, false),
        ']' => (0x1B, false),
        'a' => (0x20, false),
        's' => (0x21, false),
        'd' => (0x22, false),
        'f' => (0x23, false),
        'g' => (0x24, false),
        'h' => (0x25, false),
        'j' => (0x26, false),
        'k' => (0x27, false),
        'l' => (0x28, false),
        ';' => (0x29, false),
        '\'' => (0x2A, false),
        'z' => (0x31, false),
        'x' => (0x32, false),
        'c' => (0x33, false),
        'v' => (0x34, false),
        'b' => (0x35, false),
        'n' => (0x36, false),
        'm' => (0x37, false),
        ',' => (0x38, false),
        '.' => (0x39, false),
        '/' => (0x3A, false),
        ' ' => (AK_SPACE, false),

        '~' => (0x00, true),
        '!' => (0x01, true),
        '@' => (0x02, true),
        '#' => (0x03, true),
        '$' => (0x04, true),
        '%' => (0x05, true),
        '^' => (0x06, true),
        '&' => (0x07, true),
        '*' => (0x08, true),
        '(' => (0x09, true),
        ')' => (0x0A, true),
        '_' => (0x0B, true),
        '+' => (0x0C, true),
        '|' => (0x0D, true),
        '{' => (0x1A, true),
        '}' => (0x1B, true),
        ':' => (0x29, true),
        '"' => (0x2A, true),
        '<' => (0x38, true),
        '>' => (0x39, true),
        '?' => (0x3A, true),
        _ => return None,
    };

    Some((raw, needs_shift || is_uppercase_ascii))
}

fn map_printable_physical_key(code: KeyCode) -> Option<u8> {
    let raw = match code {
        KeyCode::Backquote => 0x00,
        KeyCode::Digit1 => 0x01,
        KeyCode::Digit2 => 0x02,
        KeyCode::Digit3 => 0x03,
        KeyCode::Digit4 => 0x04,
        KeyCode::Digit5 => 0x05,
        KeyCode::Digit6 => 0x06,
        KeyCode::Digit7 => 0x07,
        KeyCode::Digit8 => 0x08,
        KeyCode::Digit9 => 0x09,
        KeyCode::Digit0 => 0x0A,
        KeyCode::Minus => 0x0B,
        KeyCode::Equal => 0x0C,
        KeyCode::Backslash => 0x0D,
        KeyCode::KeyQ => 0x10,
        KeyCode::KeyW => 0x11,
        KeyCode::KeyE => 0x12,
        KeyCode::KeyR => 0x13,
        KeyCode::KeyT => 0x14,
        KeyCode::KeyY => 0x15,
        KeyCode::KeyU => 0x16,
        KeyCode::KeyI => 0x17,
        KeyCode::KeyO => 0x18,
        KeyCode::KeyP => 0x19,
        KeyCode::BracketLeft => 0x1A,
        KeyCode::BracketRight => 0x1B,
        KeyCode::KeyA => 0x20,
        KeyCode::KeyS => 0x21,
        KeyCode::KeyD => 0x22,
        KeyCode::KeyF => 0x23,
        KeyCode::KeyG => 0x24,
        KeyCode::KeyH => 0x25,
        KeyCode::KeyJ => 0x26,
        KeyCode::KeyK => 0x27,
        KeyCode::KeyL => 0x28,
        KeyCode::Semicolon => 0x29,
        KeyCode::Quote => 0x2A,
        KeyCode::IntlBackslash => 0x30, // international cut-out key
        KeyCode::KeyZ => 0x31,
        KeyCode::KeyX => 0x32,
        KeyCode::KeyC => 0x33,
        KeyCode::KeyV => 0x34,
        KeyCode::KeyB => 0x35,
        KeyCode::KeyN => 0x36,
        KeyCode::KeyM => 0x37,
        KeyCode::Comma => 0x38,
        KeyCode::Period => 0x39,
        KeyCode::Slash => 0x3A,
        _ => return None,
    };
    Some(raw)
}

#[cfg(test)]
mod tests {
    use super::{map_char_to_amiga_key, map_printable_physical_key};
    use winit::keyboard::KeyCode;

    #[test]
    fn shifted_digit_two_maps_to_amiga_at() {
        assert_eq!(map_char_to_amiga_key('@'), Some((0x02, true)));
    }

    #[test]
    fn uppercase_letter_requires_shift() {
        assert_eq!(map_char_to_amiga_key('A'), Some((0x20, true)));
        assert_eq!(map_char_to_amiga_key('a'), Some((0x20, false)));
    }

    #[test]
    fn physical_fallback_keeps_position_for_digit_two() {
        assert_eq!(map_printable_physical_key(KeyCode::Digit2), Some(0x02));
    }
}

fn main() {
    let cli = parse_args();
    let amiga = make_amiga(&cli);
    let mut app = App::new(amiga);

    let event_loop = match EventLoop::new() {
        Ok(loop_) => loop_,
        Err(e) => {
            eprintln!("Failed to create event loop: {e}");
            process::exit(1);
        }
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Event loop error: {e}");
        process::exit(1);
    }
}
