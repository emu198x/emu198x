//! Atari 2600 WASM build for browser embedding.
//!
//! Wraps the `emu-atari-2600` crate in a `wasm_bindgen` API. The ROM
//! cartridge must be provided from JavaScript via `fetch`.

use wasm_bindgen::prelude::*;

use emu_atari_2600::{Atari2600, Atari2600Config, Atari2600Region};

/// Atari 2600 emulator for the browser.
#[wasm_bindgen]
pub struct Atari2600Emulator {
    system: Atari2600,
    rgba_buf: Vec<u8>,
    w: u32,
    h: u32,
    /// RIOT port A (joystick directions, active-low).
    joy_input: u8,
    /// RIOT port B (console switches, active-low).
    switch_input: u8,
    /// Fire button state.
    fire: bool,
}

#[wasm_bindgen]
impl Atari2600Emulator {
    /// Create a new Atari 2600 emulator with the given cartridge ROM.
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8]) -> Result<Atari2600Emulator, JsError> {
        let config = Atari2600Config {
            rom_data: rom.to_vec(),
            region: Atari2600Region::Ntsc,
        };
        let system = Atari2600::new(&config).map_err(|e| JsError::new(&e))?;
        let w = system.framebuffer_width();
        let h = system.framebuffer_height();
        Ok(Self {
            system,
            rgba_buf: vec![0u8; (w * h * 4) as usize],
            w,
            h,
            joy_input: 0xFF,
            switch_input: 0xFF,
            fire: false,
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
        self.system.set_joystick_input(self.joy_input);
        self.system.set_switch_input(self.switch_input);
        self.system.set_fire_button_p0(self.fire);
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
    }

    /// Pointer to the RGBA framebuffer.
    pub fn framebuffer_rgba_ptr(&self) -> *const u8 {
        self.rgba_buf.as_ptr()
    }

    /// Pointer to the audio sample buffer (not yet implemented).
    pub fn audio_buffer_ptr(&self) -> *const f32 {
        [].as_ptr()
    }

    /// Number of audio samples produced this frame.
    pub fn audio_buffer_len(&self) -> usize {
        0
    }

    /// Press a key (mapped to joystick/fire/switches).
    pub fn key_down(&mut self, code: &str) {
        self.apply_key(code, true);
    }

    /// Release a key.
    pub fn key_up(&mut self, code: &str) {
        self.apply_key(code, false);
    }

    /// Reset the system.
    pub fn reset(&mut self) {
        use emu_core::Cpu;
        self.system.cpu_mut().reset();
    }
}

impl Atari2600Emulator {
    fn apply_key(&mut self, code: &str, pressed: bool) {
        match code {
            "ArrowUp" => set_bit(&mut self.joy_input, 0x10, pressed),
            "ArrowDown" => set_bit(&mut self.joy_input, 0x20, pressed),
            "ArrowLeft" => set_bit(&mut self.joy_input, 0x40, pressed),
            "ArrowRight" => set_bit(&mut self.joy_input, 0x80, pressed),
            "KeyZ" | "Space" => self.fire = pressed,
            "KeyR" => set_bit(&mut self.switch_input, 0x01, pressed), // Reset
            "KeyS" => set_bit(&mut self.switch_input, 0x02, pressed), // Select
            _ => {}
        }
    }
}

/// Active-low: pressed clears the bit, released sets it.
fn set_bit(port: &mut u8, mask: u8, pressed: bool) {
    if pressed {
        *port &= !mask;
    } else {
        *port |= mask;
    }
}
