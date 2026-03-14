//! NES WASM build for browser embedding.
//!
//! Wraps the `emu-nes` crate in a `wasm_bindgen` API. The iNES ROM file
//! must be provided from JavaScript via `fetch`.

use wasm_bindgen::prelude::*;

use emu_nes::{Nes, NesButton, NesConfig, NesRegion};

/// NES emulator for the browser.
#[wasm_bindgen]
pub struct NesEmulator {
    system: Nes,
    rgba_buf: Vec<u8>,
    audio_buf: Vec<f32>,
    w: u32,
    h: u32,
}

#[wasm_bindgen]
impl NesEmulator {
    /// Create a new NES emulator with the given iNES ROM.
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8]) -> Result<NesEmulator, JsError> {
        let config = NesConfig {
            rom_data: rom.to_vec(),
            region: NesRegion::Ntsc,
        };
        let system = Nes::new(&config).map_err(|e| JsError::new(&e))?;
        let w = system.framebuffer_width();
        let h = system.framebuffer_height();
        Ok(Self {
            system,
            rgba_buf: vec![0u8; (w * h * 4) as usize],
            audio_buf: Vec::with_capacity(960),
            w,
            h,
        })
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

    /// Press a key (mapped to NES controller buttons).
    pub fn key_down(&mut self, code: &str) {
        if let Some(button) = map_key(code) {
            self.system.bus_mut().controller1.set_button(button.bit(), true);
        }
    }

    /// Release a key.
    pub fn key_up(&mut self, code: &str) {
        if let Some(button) = map_key(code) {
            self.system.bus_mut().controller1.set_button(button.bit(), false);
        }
    }

    /// Reset the NES.
    pub fn reset(&mut self) {
        use emu_core::Cpu;
        self.system.cpu_mut().reset();
    }
}

/// Map DOM `KeyboardEvent.code` to NES button.
fn map_key(code: &str) -> Option<NesButton> {
    Some(match code {
        "ArrowUp" => NesButton::Up,
        "ArrowDown" => NesButton::Down,
        "ArrowLeft" => NesButton::Left,
        "ArrowRight" => NesButton::Right,
        "KeyZ" => NesButton::A,
        "KeyX" => NesButton::B,
        "Enter" => NesButton::Start,
        "ShiftRight" => NesButton::Select,
        _ => return None,
    })
}
