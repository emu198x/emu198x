//! Sega Master System WASM build for browser embedding.
//!
//! Wraps the `emu-sms` crate in a `wasm_bindgen` API. The cartridge ROM
//! must be provided from JavaScript via `fetch`.

use wasm_bindgen::prelude::*;

use emu_sms::{Sms, SmsVariant};

/// SMS emulator for the browser.
#[wasm_bindgen]
pub struct SmsEmulator {
    system: Sms,
    rgba_buf: Vec<u8>,
    audio_buf: Vec<f32>,
    /// Active-low button state for port $DC.
    port_dc: u8,
}

#[wasm_bindgen]
impl SmsEmulator {
    /// Create a new SMS emulator with the given cartridge ROM.
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8]) -> Self {
        let system = Sms::new(rom.to_vec(), SmsVariant::SmsNtsc);
        let w = 256usize;
        let h = 192usize;
        Self {
            system,
            rgba_buf: vec![0u8; w * h * 4],
            audio_buf: Vec::with_capacity(960),
            port_dc: 0xFF,
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
        self.system.set_port_dc(self.port_dc);
        self.system.run_frame();

        let fb = self.system.framebuffer();
        let px_count = 256 * 192;
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

    /// Press a key (mapped to SMS controller port $DC).
    pub fn key_down(&mut self, code: &str) {
        apply_key(&mut self.port_dc, code, true);
    }

    /// Release a key.
    pub fn key_up(&mut self, code: &str) {
        apply_key(&mut self.port_dc, code, false);
    }

    /// Reset the system.
    pub fn reset(&mut self) {
        use emu_core::Cpu;
        self.system.cpu_mut().reset();
    }
}

/// Map DOM key codes to SMS port $DC bits (active-low).
fn apply_key(port_dc: &mut u8, code: &str, pressed: bool) {
    let bit = match code {
        "ArrowUp" => 0x01,
        "ArrowDown" => 0x02,
        "ArrowLeft" => 0x04,
        "ArrowRight" => 0x08,
        "KeyZ" => 0x10, // Button 1
        "KeyX" => 0x20, // Button 2
        _ => return,
    };
    if pressed {
        *port_dc &= !bit;
    } else {
        *port_dc |= bit;
    }
}
