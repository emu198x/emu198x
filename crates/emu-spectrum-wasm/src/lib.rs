//! ZX Spectrum WASM build for browser embedding.
//!
//! Wraps the `emu-spectrum` crate in a `wasm_bindgen` API for use from
//! JavaScript. The Spectrum 48K ROM is embedded — no external files needed
//! to boot.
//!
//! ```js
//! import init, { SpectrumEmulator } from './pkg/emu_spectrum_wasm.js';
//!
//! const wasm = await init();
//! const emu = new SpectrumEmulator();
//!
//! function frame() {
//!     emu.run_frame();
//!     const rgba = new Uint8ClampedArray(
//!         wasm.memory.buffer,
//!         emu.framebuffer_rgba_ptr(),
//!         emu.width() * emu.height() * 4,
//!     );
//!     const imageData = new ImageData(rgba, emu.width(), emu.height());
//!     ctx.putImageData(imageData, 0, 0);
//!     requestAnimationFrame(frame);
//! }
//! requestAnimationFrame(frame);
//! ```

use wasm_bindgen::prelude::*;

use emu_spectrum::{Spectrum, SpectrumConfig, SpectrumKey, SpectrumModel};

/// Embedded 48K ROM — no external files needed.
const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");

/// ZX Spectrum emulator for the browser.
#[wasm_bindgen]
pub struct SpectrumEmulator {
    spectrum: Spectrum,
    rgba_buf: Vec<u8>,
    audio_buf: Vec<f32>,
}

#[wasm_bindgen]
impl SpectrumEmulator {
    /// Create a new Spectrum 48K emulator.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let config = SpectrumConfig {
            model: SpectrumModel::Spectrum48K,
            rom: ROM_48K.to_vec(),
        };
        let spectrum = Spectrum::new(&config);
        let w = spectrum.framebuffer_width() as usize;
        let h = spectrum.framebuffer_height() as usize;
        Self {
            spectrum,
            rgba_buf: vec![0u8; w * h * 4],
            audio_buf: Vec::with_capacity(960 * 2), // ~1 frame stereo at 48kHz/50Hz
        }
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> u32 {
        self.spectrum.framebuffer_width()
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> u32 {
        self.spectrum.framebuffer_height()
    }

    /// Run one emulation frame (~20ms of Spectrum time at 50 Hz).
    pub fn run_frame(&mut self) {
        self.spectrum.run_frame();

        // Convert ARGB32 framebuffer to RGBA bytes for canvas ImageData.
        let fb = self.spectrum.framebuffer();
        for (i, &argb) in fb.iter().enumerate() {
            let offset = i * 4;
            self.rgba_buf[offset] = ((argb >> 16) & 0xFF) as u8;     // R
            self.rgba_buf[offset + 1] = ((argb >> 8) & 0xFF) as u8;  // G
            self.rgba_buf[offset + 2] = (argb & 0xFF) as u8;         // B
            self.rgba_buf[offset + 3] = 0xFF;                        // A (opaque)
        }

        // Flatten stereo audio [L, R] pairs to interleaved f32.
        let stereo = self.spectrum.take_audio_buffer();
        self.audio_buf.clear();
        for pair in &stereo {
            // Mix to mono for simplicity (average L+R).
            self.audio_buf.push((pair[0] + pair[1]) * 0.5);
        }
    }

    /// Pointer to the RGBA framebuffer for zero-copy ImageData creation.
    pub fn framebuffer_rgba_ptr(&self) -> *const u8 {
        self.rgba_buf.as_ptr()
    }

    /// Pointer to the mono audio sample buffer (f32, 48 kHz).
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
            self.spectrum.press_key(key);
        }
    }

    /// Release a key.
    pub fn key_up(&mut self, code: &str) {
        if let Some(key) = map_key(code) {
            self.spectrum.release_key(key);
        }
    }

    /// Reset the Spectrum.
    pub fn reset(&mut self) {
        use emu_core::Cpu;
        self.spectrum.cpu_mut().reset();
    }
}

/// Map DOM `KeyboardEvent.code` to `SpectrumKey`.
fn map_key(code: &str) -> Option<SpectrumKey> {
    Some(match code {
        // Letters
        "KeyA" => SpectrumKey::A,
        "KeyB" => SpectrumKey::B,
        "KeyC" => SpectrumKey::C,
        "KeyD" => SpectrumKey::D,
        "KeyE" => SpectrumKey::E,
        "KeyF" => SpectrumKey::F,
        "KeyG" => SpectrumKey::G,
        "KeyH" => SpectrumKey::H,
        "KeyI" => SpectrumKey::I,
        "KeyJ" => SpectrumKey::J,
        "KeyK" => SpectrumKey::K,
        "KeyL" => SpectrumKey::L,
        "KeyM" => SpectrumKey::M,
        "KeyN" => SpectrumKey::N,
        "KeyO" => SpectrumKey::O,
        "KeyP" => SpectrumKey::P,
        "KeyQ" => SpectrumKey::Q,
        "KeyR" => SpectrumKey::R,
        "KeyS" => SpectrumKey::S,
        "KeyT" => SpectrumKey::T,
        "KeyU" => SpectrumKey::U,
        "KeyV" => SpectrumKey::V,
        "KeyW" => SpectrumKey::W,
        "KeyX" => SpectrumKey::X,
        "KeyY" => SpectrumKey::Y,
        "KeyZ" => SpectrumKey::Z,
        // Numbers
        "Digit0" => SpectrumKey::N0,
        "Digit1" => SpectrumKey::N1,
        "Digit2" => SpectrumKey::N2,
        "Digit3" => SpectrumKey::N3,
        "Digit4" => SpectrumKey::N4,
        "Digit5" => SpectrumKey::N5,
        "Digit6" => SpectrumKey::N6,
        "Digit7" => SpectrumKey::N7,
        "Digit8" => SpectrumKey::N8,
        "Digit9" => SpectrumKey::N9,
        // Special keys
        "Enter" => SpectrumKey::Enter,
        "Space" => SpectrumKey::Space,
        "ShiftLeft" => SpectrumKey::CapsShift,
        "ShiftRight" => SpectrumKey::SymShift,
        _ => return None,
    })
}
