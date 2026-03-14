//! Amiga WASM build for browser embedding.
//!
//! Wraps the `machine-amiga` crate in a `wasm_bindgen` API. The Kickstart
//! ROM must be provided from JavaScript via `fetch`.

use wasm_bindgen::prelude::*;

use machine_amiga::{
    Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion,
    commodore_denise_ocs::ViewportPreset,
};

/// Amiga emulator for the browser.
#[wasm_bindgen]
pub struct AmigaEmulator {
    amiga: Amiga,
    rgba_buf: Vec<u8>,
    audio_buf: Vec<f32>,
    pal: bool,
    viewport_w: u32,
    viewport_h: u32,
}

#[wasm_bindgen]
impl AmigaEmulator {
    /// Create a new Amiga 500 with Kickstart ROM.
    #[wasm_bindgen(constructor)]
    pub fn new(kickstart: &[u8]) -> Self {
        let amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Pal,
            kickstart: kickstart.to_vec(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });
        let pal = amiga.region == AmigaRegion::Pal;

        // Extract initial viewport to determine dimensions.
        let viewport = amiga.denise.extract_viewport(
            ViewportPreset::Standard,
            pal,
            true,
        );
        let viewport_w = viewport.width;
        let viewport_h = viewport.height;

        Self {
            amiga,
            rgba_buf: vec![0u8; (viewport_w * viewport_h * 4) as usize],
            audio_buf: Vec::with_capacity(960),
            pal,
            viewport_w,
            viewport_h,
        }
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> u32 {
        self.viewport_w
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> u32 {
        self.viewport_h
    }

    /// Run one emulation frame.
    pub fn run_frame(&mut self) {
        self.amiga.run_frame();

        // Extract the standard viewport.
        let viewport = self.amiga.denise.extract_viewport(
            ViewportPreset::Standard,
            self.pal,
            true,
        );

        // Update dimensions if viewport changed.
        if viewport.width != self.viewport_w || viewport.height != self.viewport_h {
            self.viewport_w = viewport.width;
            self.viewport_h = viewport.height;
            self.rgba_buf.resize((self.viewport_w * self.viewport_h * 4) as usize, 0);
        }

        // Convert ARGB32 to RGBA.
        for (i, &argb) in viewport.pixels.iter().enumerate() {
            let offset = i * 4;
            if offset + 3 < self.rgba_buf.len() {
                self.rgba_buf[offset] = ((argb >> 16) & 0xFF) as u8;
                self.rgba_buf[offset + 1] = ((argb >> 8) & 0xFF) as u8;
                self.rgba_buf[offset + 2] = (argb & 0xFF) as u8;
                self.rgba_buf[offset + 3] = 0xFF;
            }
        }

        // Audio: interleaved stereo from Paula, mix to mono.
        let stereo = self.amiga.take_audio_buffer();
        self.audio_buf.clear();
        let mut i = 0;
        while i + 1 < stereo.len() {
            self.audio_buf.push((stereo[i] + stereo[i + 1]) * 0.5);
            i += 2;
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

    /// Press a key. Uses DOM `KeyboardEvent.code` strings mapped to
    /// Amiga keycodes.
    pub fn key_down(&mut self, code: &str) {
        if let Some(keycode) = map_key(code) {
            self.amiga.keyboard.key_event(keycode, true);
        }
    }

    /// Release a key.
    pub fn key_up(&mut self, code: &str) {
        if let Some(keycode) = map_key(code) {
            self.amiga.keyboard.key_event(keycode, false);
        }
    }

    /// Reset the Amiga (warm reset — re-reads SSP and PC from ROM).
    pub fn reset(&mut self) {
        // A warm reset reads SSP from $FC0000 and PC from $FC0004 (Kickstart vectors).
        // For simplicity, just reset to address 0 which will read vectors from ROM.
        self.amiga.cpu.reset_to(0, 0);
    }
}

// Amiga keycodes (from WinUAE convention).
const AK_SPACE: u8 = 0x40;
const AK_TAB: u8 = 0x42;
const AK_RETURN: u8 = 0x44;
const AK_ESCAPE: u8 = 0x45;
const AK_BACKSPACE: u8 = 0x41;
const AK_DELETE: u8 = 0x46;
const AK_CURSOR_UP: u8 = 0x4C;
const AK_CURSOR_DOWN: u8 = 0x4D;
const AK_CURSOR_RIGHT: u8 = 0x4E;
const AK_CURSOR_LEFT: u8 = 0x4F;
const AK_LSHIFT: u8 = 0x60;
const AK_RSHIFT: u8 = 0x61;
const AK_CAPSLOCK: u8 = 0x62;
const AK_CTRL: u8 = 0x63;
const AK_LALT: u8 = 0x64;
const AK_RALT: u8 = 0x65;

/// Map DOM `KeyboardEvent.code` to Amiga keycode.
fn map_key(code: &str) -> Option<u8> {
    Some(match code {
        // Letters (Amiga keycodes $20-$39 for A-Z follow QWERTY layout)
        "KeyA" => 0x20,
        "KeyB" => 0x35,
        "KeyC" => 0x33,
        "KeyD" => 0x22,
        "KeyE" => 0x12,
        "KeyF" => 0x23,
        "KeyG" => 0x24,
        "KeyH" => 0x25,
        "KeyI" => 0x17,
        "KeyJ" => 0x26,
        "KeyK" => 0x27,
        "KeyL" => 0x28,
        "KeyM" => 0x37,
        "KeyN" => 0x36,
        "KeyO" => 0x18,
        "KeyP" => 0x19,
        "KeyQ" => 0x10,
        "KeyR" => 0x13,
        "KeyS" => 0x21,
        "KeyT" => 0x14,
        "KeyU" => 0x16,
        "KeyV" => 0x34,
        "KeyW" => 0x11,
        "KeyX" => 0x32,
        "KeyY" => 0x15,
        "KeyZ" => 0x31,
        // Numbers
        "Digit1" => 0x01,
        "Digit2" => 0x02,
        "Digit3" => 0x03,
        "Digit4" => 0x04,
        "Digit5" => 0x05,
        "Digit6" => 0x06,
        "Digit7" => 0x07,
        "Digit8" => 0x08,
        "Digit9" => 0x09,
        "Digit0" => 0x0A,
        // Special keys
        "Space" => AK_SPACE,
        "Tab" => AK_TAB,
        "Enter" => AK_RETURN,
        "Escape" => AK_ESCAPE,
        "Backspace" => AK_BACKSPACE,
        "Delete" => AK_DELETE,
        "ArrowUp" => AK_CURSOR_UP,
        "ArrowDown" => AK_CURSOR_DOWN,
        "ArrowRight" => AK_CURSOR_RIGHT,
        "ArrowLeft" => AK_CURSOR_LEFT,
        "ShiftLeft" => AK_LSHIFT,
        "ShiftRight" => AK_RSHIFT,
        "CapsLock" => AK_CAPSLOCK,
        "ControlLeft" | "ControlRight" => AK_CTRL,
        "AltLeft" => AK_LALT,
        "AltRight" => AK_RALT,
        // Punctuation
        "Minus" => 0x0B,
        "Equal" => 0x0C,
        "BracketLeft" => 0x1A,
        "BracketRight" => 0x1B,
        "Semicolon" => 0x29,
        "Quote" => 0x2A,
        "Backslash" => 0x0D,
        "Comma" => 0x38,
        "Period" => 0x39,
        "Slash" => 0x3A,
        "Backquote" => 0x00,
        // Function keys
        "F1" => 0x50,
        "F2" => 0x51,
        "F3" => 0x52,
        "F4" => 0x53,
        "F5" => 0x54,
        "F6" => 0x55,
        "F7" => 0x56,
        "F8" => 0x57,
        "F9" => 0x58,
        "F10" => 0x59,
        _ => return None,
    })
}
