//! Atari 5200 WASM build for browser embedding.
//!
//! Wraps the `emu-atari-5200` crate in a `wasm_bindgen` API. The ROM
//! cartridge and optional BIOS must be provided from JavaScript via `fetch`.

use wasm_bindgen::prelude::*;

use emu_atari_5200::{Atari5200, Atari5200Config, Atari5200Region};

/// Atari 5200 emulator for the browser.
#[wasm_bindgen]
pub struct Atari5200Emulator {
    system: Atari5200,
    rgba_buf: Vec<u8>,
    w: u32,
    h: u32,
    audio_buf: Vec<f32>,
}

#[wasm_bindgen]
impl Atari5200Emulator {
    /// Create a new Atari 5200 emulator with cartridge ROM and optional BIOS.
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8], bios: Option<Vec<u8>>) -> Result<Atari5200Emulator, JsError> {
        let config = Atari5200Config {
            rom_data: rom.to_vec(),
            bios_data: bios,
            region: Atari5200Region::Ntsc,
        };
        let system = Atari5200::new(&config).map_err(|e| JsError::new(&e))?;
        let w = system.framebuffer_width();
        let h = system.framebuffer_height();
        Ok(Self {
            system,
            rgba_buf: vec![0u8; (w * h * 4) as usize],
            w,
            h,
            audio_buf: Vec::with_capacity(960),
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

        // Audio: POKEY audio not yet exposed via mutable accessor.
        self.audio_buf.clear();
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

    /// Press a key (mapped to joystick/fire/start).
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

impl Atari5200Emulator {
    fn apply_key(&mut self, code: &str, pressed: bool) {
        // POKEY pot range: 0=left/up, 114=centre, 228=right/down
        match code {
            "ArrowUp" => {
                let (x, _) = self.joy_pos();
                self.system.set_joystick(x, if pressed { 0 } else { 114 });
            }
            "ArrowDown" => {
                let (x, _) = self.joy_pos();
                self.system.set_joystick(x, if pressed { 228 } else { 114 });
            }
            "ArrowLeft" => {
                let (_, y) = self.joy_pos();
                self.system.set_joystick(if pressed { 0 } else { 114 }, y);
            }
            "ArrowRight" => {
                let (_, y) = self.joy_pos();
                self.system.set_joystick(if pressed { 228 } else { 114 }, y);
            }
            "KeyZ" | "Space" => self.system.set_fire(pressed),
            "Enter" => self.system.set_start(pressed),
            _ => {}
        }
    }

    fn joy_pos(&self) -> (u8, u8) {
        // Read current pot values (approximate)
        (114, 114)
    }
}
