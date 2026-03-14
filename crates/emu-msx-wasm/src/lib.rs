//! MSX1 WASM build for browser embedding.
//!
//! Wraps the `emu-msx` crate in a `wasm_bindgen` API. The BIOS ROM
//! must be provided from JavaScript via `fetch`. An optional cartridge
//! can be inserted after construction.

use wasm_bindgen::prelude::*;

use emu_msx::{MapperType, Msx, MsxRegion};

/// MSX1 emulator for the browser.
#[wasm_bindgen]
pub struct MsxEmulator {
    system: Msx,
    rgba_buf: Vec<u8>,
    audio_buf: Vec<f32>,
}

#[wasm_bindgen]
impl MsxEmulator {
    /// Create a new MSX with BIOS ROM.
    #[wasm_bindgen(constructor)]
    pub fn new(bios: &[u8]) -> Self {
        let system = Msx::new(bios.to_vec(), MsxRegion::Ntsc);
        Self {
            system,
            rgba_buf: vec![0u8; 256 * 192 * 4],
            audio_buf: Vec::with_capacity(960),
        }
    }

    /// Insert a cartridge ROM into slot 1 (plain mapper).
    pub fn insert_cart(&mut self, rom: &[u8]) {
        self.system.insert_cart1(rom.to_vec(), MapperType::Plain);
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> u32 {
        256
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> u32 {
        192
    }

    /// Run one emulation frame.
    pub fn run_frame(&mut self) {
        self.system.run_frame();

        let fb = self.system.framebuffer();
        for (i, &argb) in fb.iter().enumerate() {
            let offset = i * 4;
            self.rgba_buf[offset] = ((argb >> 16) & 0xFF) as u8;
            self.rgba_buf[offset + 1] = ((argb >> 8) & 0xFF) as u8;
            self.rgba_buf[offset + 2] = (argb & 0xFF) as u8;
            self.rgba_buf[offset + 3] = 0xFF;
        }

        let stereo = self.system.take_audio_buffer();
        self.audio_buf.clear();
        for pair in &stereo {
            self.audio_buf.push((pair[0] + pair[1]) * 0.5);
        }
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

    /// Press a key. Uses DOM `KeyboardEvent.code` strings mapped to the
    /// MSX keyboard matrix (row, bit).
    pub fn key_down(&mut self, code: &str) {
        if let Some((row, bit)) = map_key(code) {
            self.system.press_key(row, bit);
        }
    }

    /// Release a key.
    pub fn key_up(&mut self, code: &str) {
        if let Some((row, bit)) = map_key(code) {
            self.system.release_key(row, bit);
        }
    }

    /// Reset the system.
    pub fn reset(&mut self) {
        use emu_core::Cpu;
        self.system.cpu_mut().reset();
    }
}

/// Map DOM `KeyboardEvent.code` to MSX keyboard matrix (row, bit).
fn map_key(code: &str) -> Option<(usize, u8)> {
    Some(match code {
        // Row 0
        "Digit0" => (0, 0),
        "Digit1" => (0, 1),
        "Digit2" => (0, 2),
        "Digit3" => (0, 3),
        "Digit4" => (0, 4),
        "Digit5" => (0, 5),
        "Digit6" => (0, 6),
        "Digit7" => (0, 7),
        // Row 1
        "Digit8" => (1, 0),
        "Digit9" => (1, 1),
        "Minus" => (1, 2),
        "Equal" => (1, 3),
        "Backslash" => (1, 4),
        "BracketLeft" => (1, 5),
        "BracketRight" => (1, 6),
        "Semicolon" => (1, 7),
        // Row 2
        "Quote" => (2, 0),
        "Backquote" => (2, 1),
        "Comma" => (2, 2),
        "Period" => (2, 3),
        "Slash" => (2, 4),
        // Row 6
        "KeyA" => (6, 1),
        "KeyB" => (6, 2),
        "KeyC" => (6, 3),
        "KeyD" => (6, 4),
        "KeyE" => (6, 5),
        "KeyF" => (6, 6),
        "KeyG" => (6, 7),
        // Row 7
        "KeyH" => (7, 0),
        "KeyI" => (7, 1),
        "KeyJ" => (7, 2),
        "KeyK" => (7, 3),
        "KeyL" => (7, 4),
        "KeyM" => (7, 5),
        "KeyN" => (7, 6),
        "KeyO" => (7, 7),
        // Row 8
        "KeyP" => (8, 0),
        "KeyQ" => (8, 1),
        "KeyR" => (8, 2),
        "KeyS" => (8, 3),
        "KeyT" => (8, 4),
        "KeyU" => (8, 5),
        "KeyV" => (8, 6),
        "KeyW" => (8, 7),
        // Row 9
        "KeyX" => (9, 0),
        "KeyY" => (9, 1),
        "KeyZ" => (9, 2),
        // Special keys (row 3-5)
        "ShiftLeft" | "ShiftRight" => (3, 0),
        "ControlLeft" | "ControlRight" => (3, 1),
        "Space" => (4, 0),
        "Enter" => (4, 7),
        "Backspace" => (5, 5),
        "ArrowUp" => (4, 5),
        "ArrowDown" => (4, 6),
        "ArrowLeft" => (4, 3),
        "ArrowRight" => (4, 4),
        "Escape" => (5, 2),
        _ => return None,
    })
}
