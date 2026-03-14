//! BBC Micro WASM build for browser embedding.
//!
//! Wraps the `emu-bbc-micro` crate in a `wasm_bindgen` API. The MOS ROM
//! must be provided from JavaScript via `fetch`.

use wasm_bindgen::prelude::*;

use emu_bbc_micro::{BbcMicro, FB_HEIGHT, FB_WIDTH};

/// BBC Micro emulator for the browser.
#[wasm_bindgen]
pub struct BbcMicroEmulator {
    system: BbcMicro,
    rgba_buf: Vec<u8>,
    audio_buf: Vec<f32>,
}

#[wasm_bindgen]
impl BbcMicroEmulator {
    /// Create a new BBC Micro with MOS ROM.
    #[wasm_bindgen(constructor)]
    pub fn new(mos_rom: &[u8]) -> Self {
        let system = BbcMicro::new(mos_rom.to_vec());
        Self {
            system,
            rgba_buf: vec![0u8; (FB_WIDTH * FB_HEIGHT * 4) as usize],
            audio_buf: Vec::with_capacity(960),
        }
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> u32 {
        FB_HEIGHT
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

    /// Press a key. Uses DOM `KeyboardEvent.code` strings mapped to the
    /// BBC Micro keyboard matrix (column, row).
    pub fn key_down(&mut self, code: &str) {
        if let Some((col, row)) = map_key(code) {
            self.system.press_key(col, row);
        }
    }

    /// Release a key.
    pub fn key_up(&mut self, code: &str) {
        if let Some((col, row)) = map_key(code) {
            self.system.release_key(col, row);
        }
    }

    /// Reset the system.
    pub fn reset(&mut self) {
        use emu_core::Cpu;
        self.system.cpu_mut().reset();
    }
}

/// Map DOM `KeyboardEvent.code` to BBC Micro keyboard matrix (column, row).
fn map_key(code: &str) -> Option<(usize, usize)> {
    Some(match code {
        // Letters (approximate BBC Micro matrix positions)
        "KeyA" => (4, 1),
        "KeyB" => (6, 4),
        "KeyC" => (5, 2),
        "KeyD" => (3, 2),
        "KeyE" => (2, 2),
        "KeyF" => (4, 3),
        "KeyG" => (5, 3),
        "KeyH" => (5, 4),
        "KeyI" => (2, 5),
        "KeyJ" => (4, 5),
        "KeyK" => (4, 6),
        "KeyL" => (5, 6),
        "KeyM" => (6, 5),
        "KeyN" => (5, 5),
        "KeyO" => (3, 6),
        "KeyP" => (3, 7),
        "KeyQ" => (1, 0),
        "KeyR" => (3, 3),
        "KeyS" => (5, 1),
        "KeyT" => (2, 3),
        "KeyU" => (3, 5),
        "KeyV" => (6, 3),
        "KeyW" => (2, 1),
        "KeyX" => (4, 2),
        "KeyY" => (4, 4),
        "KeyZ" => (6, 1),
        // Numbers
        "Digit0" => (2, 7),
        "Digit1" => (3, 0),
        "Digit2" => (3, 1),
        "Digit3" => (1, 1),
        "Digit4" => (1, 2),
        "Digit5" => (1, 3),
        "Digit6" => (3, 4),
        "Digit7" => (2, 4),
        "Digit8" => (1, 5),
        "Digit9" => (2, 6),
        // Special
        "Space" => (6, 2),
        "Enter" => (4, 9),
        "Backspace" => (5, 9),
        "ShiftLeft" => (0, 0),
        "ShiftRight" => (0, 0),
        "ControlLeft" | "ControlRight" => (0, 1),
        "Escape" => (7, 0),
        "ArrowUp" => (3, 9),
        "ArrowDown" => (2, 9),
        "ArrowLeft" => (1, 9),
        "ArrowRight" => (7, 9),
        _ => return None,
    })
}
