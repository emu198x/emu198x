//! ColecoVision WASM build for browser embedding.
//!
//! Wraps the `emu-colecovision` crate in a `wasm_bindgen` API. Both the
//! BIOS ROM and cartridge ROM must be provided from JavaScript via `fetch`.

use wasm_bindgen::prelude::*;

use emu_colecovision::{ColecoVision, CvRegion};

/// ColecoVision emulator for the browser.
#[wasm_bindgen]
pub struct ColecoVisionEmulator {
    system: ColecoVision,
    rgba_buf: Vec<u8>,
    audio_buf: Vec<f32>,
}

#[wasm_bindgen]
impl ColecoVisionEmulator {
    /// Create a new ColecoVision emulator with BIOS and cartridge ROM.
    #[wasm_bindgen(constructor)]
    pub fn new(bios: &[u8], cart: &[u8]) -> Self {
        let system = ColecoVision::new(bios.to_vec(), cart.to_vec(), CvRegion::Ntsc);
        Self {
            system,
            rgba_buf: vec![0u8; 256 * 192 * 4],
            audio_buf: Vec::with_capacity(960),
        }
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

    /// Press a key (mapped to controller).
    pub fn key_down(&mut self, code: &str) {
        apply_key(&mut self.system, code, true);
    }

    /// Release a key.
    pub fn key_up(&mut self, code: &str) {
        apply_key(&mut self.system, code, false);
    }

    /// Reset the system.
    pub fn reset(&mut self) {
        use emu_core::Cpu;
        self.system.cpu_mut().reset();
    }
}

fn apply_key(system: &mut ColecoVision, code: &str, pressed: bool) {
    let ctrl = system.controller1_mut();
    match code {
        "ArrowUp" => ctrl.up = pressed,
        "ArrowDown" => ctrl.down = pressed,
        "ArrowLeft" => ctrl.left = pressed,
        "ArrowRight" => ctrl.right = pressed,
        "KeyZ" => ctrl.left_button = pressed,
        "KeyX" => ctrl.right_button = pressed,
        // Keypad
        "Digit0" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K0) } else { None },
        "Digit1" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K1) } else { None },
        "Digit2" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K2) } else { None },
        "Digit3" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K3) } else { None },
        "Digit4" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K4) } else { None },
        "Digit5" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K5) } else { None },
        "Digit6" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K6) } else { None },
        "Digit7" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K7) } else { None },
        "Digit8" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K8) } else { None },
        "Digit9" => ctrl.keypad = if pressed { Some(emu_colecovision::KeypadKey::K9) } else { None },
        _ => {}
    }
}
