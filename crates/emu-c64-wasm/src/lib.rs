//! Commodore 64 WASM build for browser embedding.
//!
//! Wraps the `emu-c64` crate in a `wasm_bindgen` API. The Kernal, BASIC,
//! and character ROMs must be provided from JavaScript via `fetch`.

use wasm_bindgen::prelude::*;

use emu_c64::{C64, C64Config, C64Key, C64Model, config::SidModel};

/// C64 emulator for the browser.
#[wasm_bindgen]
pub struct C64Emulator {
    system: C64,
    rgba_buf: Vec<u8>,
    audio_buf: Vec<f32>,
    w: u32,
    h: u32,
}

#[wasm_bindgen]
impl C64Emulator {
    /// Create a new C64 with Kernal, BASIC, and character ROMs.
    #[wasm_bindgen(constructor)]
    pub fn new(kernal: &[u8], basic: &[u8], chargen: &[u8]) -> Self {
        let config = C64Config {
            model: C64Model::C64Pal,
            sid_model: SidModel::Sid6581,
            kernal_rom: kernal.to_vec(),
            basic_rom: basic.to_vec(),
            char_rom: chargen.to_vec(),
            drive_rom: None,
            reu_size: None,
        };
        let system = C64::new(&config);
        let w = system.framebuffer_width();
        let h = system.framebuffer_height();
        Self {
            system,
            rgba_buf: vec![0u8; (w * h * 4) as usize],
            audio_buf: Vec::with_capacity(960),
            w,
            h,
        }
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> u32 {
        self.w
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> u32 {
        self.h
    }

    /// Run one emulation frame.
    pub fn run_frame(&mut self) {
        self.system.run_frame();

        let fb = self.system.framebuffer();
        let px_count = (self.w * self.h) as usize;
        for i in 0..px_count.min(fb.len()) {
            let argb = fb[i];
            let offset = i * 4;
            self.rgba_buf[offset] = ((argb >> 16) & 0xFF) as u8;
            self.rgba_buf[offset + 1] = ((argb >> 8) & 0xFF) as u8;
            self.rgba_buf[offset + 2] = (argb & 0xFF) as u8;
            self.rgba_buf[offset + 3] = 0xFF;
        }

        let samples = self.system.take_audio_buffer();
        self.audio_buf.clear();
        self.audio_buf.extend_from_slice(&samples);
    }

    /// Pointer to the RGBA framebuffer.
    pub fn framebuffer_rgba_ptr(&self) -> *const u8 {
        self.rgba_buf.as_ptr()
    }

    /// Pointer to the audio sample buffer (mono f32, 48 kHz).
    pub fn audio_buffer_ptr(&self) -> *const f32 {
        self.audio_buf.as_ptr()
    }

    /// Number of audio samples produced this frame.
    pub fn audio_buffer_len(&self) -> usize {
        self.audio_buf.len()
    }

    /// Press a key. Uses DOM `KeyboardEvent.code` strings.
    pub fn key_down(&mut self, code: &str) {
        if let Some(key) = map_key(code) {
            self.system.press_key(key);
        }
    }

    /// Release a key.
    pub fn key_up(&mut self, code: &str) {
        if let Some(key) = map_key(code) {
            self.system.release_key(key);
        }
    }

    /// Reset the C64.
    pub fn reset(&mut self) {
        use emu_core::Cpu;
        self.system.cpu_mut().reset();
    }
}

/// Map DOM `KeyboardEvent.code` to `C64Key`.
fn map_key(code: &str) -> Option<C64Key> {
    Some(match code {
        // Letters
        "KeyA" => C64Key::A,
        "KeyB" => C64Key::B,
        "KeyC" => C64Key::C,
        "KeyD" => C64Key::D,
        "KeyE" => C64Key::E,
        "KeyF" => C64Key::F,
        "KeyG" => C64Key::G,
        "KeyH" => C64Key::H,
        "KeyI" => C64Key::I,
        "KeyJ" => C64Key::J,
        "KeyK" => C64Key::K,
        "KeyL" => C64Key::L,
        "KeyM" => C64Key::M,
        "KeyN" => C64Key::N,
        "KeyO" => C64Key::O,
        "KeyP" => C64Key::P,
        "KeyQ" => C64Key::Q,
        "KeyR" => C64Key::R,
        "KeyS" => C64Key::S,
        "KeyT" => C64Key::T,
        "KeyU" => C64Key::U,
        "KeyV" => C64Key::V,
        "KeyW" => C64Key::W,
        "KeyX" => C64Key::X,
        "KeyY" => C64Key::Y,
        "KeyZ" => C64Key::Z,
        // Numbers
        "Digit0" => C64Key::N0,
        "Digit1" => C64Key::N1,
        "Digit2" => C64Key::N2,
        "Digit3" => C64Key::N3,
        "Digit4" => C64Key::N4,
        "Digit5" => C64Key::N5,
        "Digit6" => C64Key::N6,
        "Digit7" => C64Key::N7,
        "Digit8" => C64Key::N8,
        "Digit9" => C64Key::N9,
        // Special keys
        "Enter" => C64Key::Return,
        "Space" => C64Key::Space,
        "Backspace" => C64Key::Delete,
        "ShiftLeft" => C64Key::LShift,
        "ShiftRight" => C64Key::RShift,
        "ControlLeft" | "ControlRight" => C64Key::Ctrl,
        "ArrowUp" | "ArrowDown" => C64Key::CursorDown,
        "ArrowLeft" | "ArrowRight" => C64Key::CursorRight,
        "F1" => C64Key::F1,
        "F3" => C64Key::F3,
        "F5" => C64Key::F5,
        "F7" => C64Key::F7,
        "Escape" => C64Key::RunStop,
        _ => return None,
    })
}
