//! Atari 800XL WASM build for browser embedding.
//!
//! Wraps the `emu-atari-800xl` crate in a `wasm_bindgen` API. The OS ROM
//! must be provided from JavaScript via `fetch`. An optional cartridge can
//! be loaded at construction time.

use wasm_bindgen::prelude::*;

use emu_atari_800xl::{
    Atari800xl, Atari800xlConfig, Atari800xlRegion, Atari8bitModel,
};

/// Atari 800XL emulator for the browser.
#[wasm_bindgen]
pub struct Atari800xlEmulator {
    system: Atari800xl,
    rgba_buf: Vec<u8>,
    w: u32,
    h: u32,
}

#[wasm_bindgen]
impl Atari800xlEmulator {
    /// Create a new Atari 800XL with OS ROM and optional cartridge.
    #[wasm_bindgen(constructor)]
    pub fn new(os_rom: &[u8], cart: Option<Vec<u8>>) -> Result<Atari800xlEmulator, JsError> {
        let config = Atari800xlConfig {
            model: Atari8bitModel::A800XL,
            rom_data: cart,
            os_rom: Some(os_rom.to_vec()),
            basic_rom: None,
            region: Atari800xlRegion::Ntsc,
            basic_enabled: false,
        };
        let system = Atari800xl::new(&config).map_err(|e| JsError::new(&e))?;
        let w = system.framebuffer_width();
        let h = system.framebuffer_height();
        Ok(Self {
            system,
            rgba_buf: vec![0u8; (w * h * 4) as usize],
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

    /// Press a key (mapped to joystick/fire/console keys).
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

impl Atari800xlEmulator {
    fn apply_key(&mut self, code: &str, pressed: bool) {
        match code {
            "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" => {
                let up = code == "ArrowUp" && pressed;
                let down = code == "ArrowDown" && pressed;
                let left = code == "ArrowLeft" && pressed;
                let right = code == "ArrowRight" && pressed;
                self.system.set_joystick(up, down, left, right);
            }
            "KeyZ" | "Space" => self.system.set_fire(pressed),
            "F2" => self.system.set_console_keys(pressed, false, false), // Start
            "F3" => self.system.set_console_keys(false, pressed, false), // Select
            "F4" => self.system.set_console_keys(false, false, pressed), // Option
            _ => {}
        }
    }
}
