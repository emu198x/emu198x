//! MCP (Model Context Protocol) server for the ZX Spectrum emulator.
//!
//! Exposes the emulator as a JSON-RPC 2.0 server over stdin/stdout.
//! Tools allow AI agents and scripts to boot, control, observe, and
//! capture the emulator programmatically.
//!
//! # Protocol
//!
//! Reads newline-delimited JSON-RPC 2.0 requests from stdin, writes
//! responses to stdout. No window or audio output — purely headless.

#![allow(clippy::cast_possible_truncation)]

use std::io::{self, BufRead, Write};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use emu_core::{Observable, Tickable};

use crate::Spectrum;
use crate::config::{SpectrumConfig, SpectrumModel};
use crate::input::SpectrumKey;
use crate::sna::load_sna;
use crate::tap::TapFile;

/// Embedded 48K ROM.
const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: JsonValue,
    id: JsonValue,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    id: JsonValue,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl RpcResponse {
    fn success(id: JsonValue, result: JsonValue) -> Self {
        Self {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }

    fn error(id: JsonValue, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(RpcError { code, message }),
            id,
        }
    }
}

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

/// MCP server wrapping a headless Spectrum instance.
pub struct McpServer {
    spectrum: Option<Spectrum>,
}

impl McpServer {
    #[must_use]
    pub fn new() -> Self {
        Self { spectrum: None }
    }

    /// Run the server loop: read JSON-RPC from stdin, write responses to stdout.
    pub fn run(&mut self) {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut stdout = stdout.lock();

        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let request: RpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let resp =
                        RpcResponse::error(JsonValue::Null, -32700, format!("Parse error: {e}"));
                    let _ = writeln!(
                        stdout,
                        "{}",
                        serde_json::to_string(&resp).unwrap_or_default()
                    );
                    let _ = stdout.flush();
                    continue;
                }
            };

            if request.jsonrpc != "2.0" {
                let resp =
                    RpcResponse::error(request.id, -32600, "Invalid JSON-RPC version".to_string());
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string(&resp).unwrap_or_default()
                );
                let _ = stdout.flush();
                continue;
            }

            let response = self.dispatch(&request.method, &request.params, request.id.clone());
            let _ = writeln!(
                stdout,
                "{}",
                serde_json::to_string(&response).unwrap_or_default()
            );
            let _ = stdout.flush();
        }
    }

    /// Dispatch a method call to the appropriate handler.
    fn dispatch(&mut self, method: &str, params: &JsonValue, id: JsonValue) -> RpcResponse {
        match method {
            "boot" => self.handle_boot(id),
            "reset" => self.handle_reset(id),
            "load_sna" => self.handle_load_sna(params, id),
            "load_tap" => self.handle_load_tap(params, id),
            "run_frames" => self.handle_run_frames(params, id),
            "step_instruction" => self.handle_step_instruction(id),
            "step_ticks" => self.handle_step_ticks(params, id),
            "screenshot" => self.handle_screenshot(id),
            "audio_capture" => self.handle_audio_capture(params, id),
            "query" => self.handle_query(params, id),
            "poke" => self.handle_poke(params, id),
            "press_key" => self.handle_press_key(params, id),
            "release_key" => self.handle_release_key(params, id),
            "type_text" => self.handle_type_text(params, id),
            "set_breakpoint" => self.handle_set_breakpoint(params, id),
            "get_screen_text" => self.handle_get_screen_text(id),
            _ => RpcResponse::error(id, -32601, format!("Unknown method: {method}")),
        }
    }

    /// Ensure a Spectrum instance exists, returning a mutable reference.
    fn require_spectrum(&mut self, id: &JsonValue) -> Result<&mut Spectrum, RpcResponse> {
        if self.spectrum.is_some() {
            Ok(self.spectrum.as_mut().unwrap())
        } else {
            Err(RpcResponse::error(
                id.clone(),
                -32000,
                "No Spectrum instance. Call 'boot' first.".to_string(),
            ))
        }
    }

    // === Tool handlers ===

    fn handle_boot(&mut self, id: JsonValue) -> RpcResponse {
        let config = SpectrumConfig {
            model: SpectrumModel::Spectrum48K,
            rom: ROM_48K.to_vec(),
        };
        self.spectrum = Some(Spectrum::new(&config));
        RpcResponse::success(id, serde_json::json!({"status": "ok"}))
    }

    fn handle_reset(&mut self, id: JsonValue) -> RpcResponse {
        match self.require_spectrum(&id) {
            Ok(spec) => {
                use emu_core::Cpu;
                spec.cpu_mut().reset();
                RpcResponse::success(id, serde_json::json!({"status": "ok"}))
            }
            Err(e) => e,
        }
    }

    fn handle_load_sna(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let data = if let Some(b64) = params.get("data").and_then(|v| v.as_str()) {
            match base64::engine::general_purpose::STANDARD.decode(b64) {
                Ok(d) => d,
                Err(e) => return RpcResponse::error(id, -32602, format!("Invalid base64: {e}")),
            }
        } else if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
            match std::fs::read(path) {
                Ok(d) => d,
                Err(e) => return RpcResponse::error(id, -32602, format!("Cannot read file: {e}")),
            }
        } else {
            return RpcResponse::error(id, -32602, "Provide 'data' (base64) or 'path'".to_string());
        };

        match load_sna(spec, &data) {
            Ok(()) => RpcResponse::success(id, serde_json::json!({"status": "ok"})),
            Err(e) => RpcResponse::error(id, -32000, format!("SNA load failed: {e}")),
        }
    }

    fn handle_load_tap(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let data = if let Some(b64) = params.get("data").and_then(|v| v.as_str()) {
            match base64::engine::general_purpose::STANDARD.decode(b64) {
                Ok(d) => d,
                Err(e) => return RpcResponse::error(id, -32602, format!("Invalid base64: {e}")),
            }
        } else if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
            match std::fs::read(path) {
                Ok(d) => d,
                Err(e) => return RpcResponse::error(id, -32602, format!("Cannot read file: {e}")),
            }
        } else {
            return RpcResponse::error(id, -32602, "Provide 'data' (base64) or 'path'".to_string());
        };

        match TapFile::parse(&data) {
            Ok(tap) => {
                let blocks = tap.blocks.len();
                spec.insert_tap(tap);
                RpcResponse::success(id, serde_json::json!({"status": "ok", "blocks": blocks}))
            }
            Err(e) => RpcResponse::error(id, -32000, format!("TAP parse failed: {e}")),
        }
    }

    fn handle_run_frames(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(1);

        let mut total_tstates = 0u64;
        for _ in 0..count {
            total_tstates += spec.run_frame();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "frames": count,
                "tstates": total_tstates,
                "frame_count": spec.frame_count(),
            }),
        )
    }

    fn handle_step_instruction(&mut self, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        // Run ticks until PC changes (indicating instruction boundary).
        let start_pc = spec.cpu().regs.pc;
        let mut tstates = 0u64;
        let max_tstates = 100;

        loop {
            spec.tick();
            // Each tick is 1 master clock; CPU runs every 4 master clocks
            if spec.master_clock() % 4 == 0 {
                tstates += 1;
            }
            if spec.cpu().regs.pc != start_pc || tstates >= max_tstates {
                break;
            }
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "pc": format!("${:04X}", spec.cpu().regs.pc),
                "tstates": tstates,
            }),
        )
    }

    fn handle_step_ticks(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(1);

        // Each CPU T-state = 4 master clock ticks
        for _ in 0..(count * 4) {
            spec.tick();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "pc": format!("${:04X}", spec.cpu().regs.pc),
            }),
        )
    }

    fn handle_screenshot(&mut self, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let width = spec.framebuffer_width();
        let height = spec.framebuffer_height();
        let fb = spec.framebuffer();

        // Encode framebuffer as PNG in memory
        let mut png_buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_buf, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = match encoder.write_header() {
                Ok(w) => w,
                Err(e) => return RpcResponse::error(id, -32000, format!("PNG encode error: {e}")),
            };

            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for &pixel in fb {
                rgba.push(((pixel >> 16) & 0xFF) as u8); // R
                rgba.push(((pixel >> 8) & 0xFF) as u8); // G
                rgba.push((pixel & 0xFF) as u8); // B
                rgba.push(0xFF); // A
            }

            if let Err(e) = writer.write_image_data(&rgba) {
                return RpcResponse::error(id, -32000, format!("PNG write error: {e}"));
            }
        }

        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_buf);
        RpcResponse::success(
            id,
            serde_json::json!({
                "format": "png",
                "width": width,
                "height": height,
                "data": b64,
            }),
        )
    }

    fn handle_audio_capture(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let frames = params.get("frames").and_then(|v| v.as_u64()).unwrap_or(50);

        let mut all_audio = Vec::new();
        for _ in 0..frames {
            spec.run_frame();
            all_audio.extend_from_slice(&spec.take_audio_buffer());
        }

        // Encode as WAV in memory
        let spec_wav = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut wav_buf = Vec::new();
        {
            let cursor = io::Cursor::new(&mut wav_buf);
            let mut writer = match hound::WavWriter::new(cursor, spec_wav) {
                Ok(w) => w,
                Err(e) => return RpcResponse::error(id, -32000, format!("WAV encode error: {e}")),
            };
            for &sample in &all_audio {
                let clamped = sample.clamp(-1.0, 1.0);
                let scaled = (clamped * f32::from(i16::MAX)) as i16;
                if let Err(e) = writer.write_sample(scaled) {
                    return RpcResponse::error(id, -32000, format!("WAV write error: {e}"));
                }
            }
            if let Err(e) = writer.finalize() {
                return RpcResponse::error(id, -32000, format!("WAV finalize error: {e}"));
            }
        }

        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_buf);
        RpcResponse::success(
            id,
            serde_json::json!({
                "format": "wav",
                "samples": all_audio.len(),
                "frames": frames,
                "data": b64,
            }),
        )
    }

    fn handle_query(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let path = match params.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return RpcResponse::error(id, -32602, "Missing 'path' parameter".to_string()),
        };

        match spec.query(path) {
            Some(value) => {
                let json_val = match value {
                    emu_core::Value::U8(v) => serde_json::json!(v),
                    emu_core::Value::U16(v) => serde_json::json!(v),
                    emu_core::Value::U32(v) => serde_json::json!(v),
                    emu_core::Value::U64(v) => serde_json::json!(v),
                    emu_core::Value::I8(v) => serde_json::json!(v),
                    emu_core::Value::Bool(v) => serde_json::json!(v),
                    emu_core::Value::String(v) => serde_json::json!(v),
                    emu_core::Value::Array(v) => serde_json::json!(format!("{v:?}")),
                    emu_core::Value::Map(v) => serde_json::json!(format!("{v:?}")),
                };
                RpcResponse::success(id, serde_json::json!({"path": path, "value": json_val}))
            }
            None => RpcResponse::error(id, -32000, format!("Unknown query path: {path}")),
        }
    }

    fn handle_poke(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0xFFFF => a as u16,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'address' (0-65535)".to_string(),
                );
            }
        };

        let value = match params.get("value").and_then(|v| v.as_u64()) {
            Some(v) if v <= 0xFF => v as u8,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'value' (0-255)".to_string(),
                );
            }
        };

        spec.bus_mut().memory.write(addr, value);
        RpcResponse::success(id, serde_json::json!({"address": addr, "value": value}))
    }

    fn handle_press_key(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let key_name = match params.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return RpcResponse::error(id, -32602, "Missing 'key' parameter".to_string()),
        };

        match parse_key_name(key_name) {
            Some(key) => {
                spec.press_key(key);
                RpcResponse::success(id, serde_json::json!({"key": key_name, "pressed": true}))
            }
            None => RpcResponse::error(id, -32602, format!("Unknown key: {key_name}")),
        }
    }

    fn handle_release_key(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let key_name = match params.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return RpcResponse::error(id, -32602, "Missing 'key' parameter".to_string()),
        };

        match parse_key_name(key_name) {
            Some(key) => {
                spec.release_key(key);
                RpcResponse::success(id, serde_json::json!({"key": key_name, "pressed": false}))
            }
            None => RpcResponse::error(id, -32602, format!("Unknown key: {key_name}")),
        }
    }

    fn handle_type_text(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let text = match params.get("text").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => return RpcResponse::error(id, -32602, "Missing 'text' parameter".to_string()),
        };

        let at_frame = params
            .get("at_frame")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| spec.frame_count());

        let end_frame = spec.input_queue().enqueue_text(&text, at_frame);
        RpcResponse::success(
            id,
            serde_json::json!({
                "text": text,
                "start_frame": at_frame,
                "end_frame": end_frame,
            }),
        )
    }

    fn handle_set_breakpoint(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0xFFFF => a as u16,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'address' (0-65535)".to_string(),
                );
            }
        };

        let max_frames = params
            .get("max_frames")
            .and_then(|v| v.as_u64())
            .unwrap_or(10_000);

        let mut frames_run = 0u64;
        let mut hit = false;

        'outer: for _ in 0..max_frames {
            // Run one frame
            loop {
                spec.tick();
                if spec.cpu().regs.pc == addr {
                    hit = true;
                    break 'outer;
                }
                if spec.bus_mut().ula.take_frame_complete() {
                    frames_run += 1;
                    break;
                }
            }
        }

        if hit {
            RpcResponse::success(
                id,
                serde_json::json!({
                    "hit": true,
                    "pc": format!("${:04X}", addr),
                    "frames_run": frames_run,
                }),
            )
        } else {
            RpcResponse::success(
                id,
                serde_json::json!({
                    "hit": false,
                    "pc": format!("${:04X}", spec.cpu().regs.pc),
                    "frames_run": frames_run,
                }),
            )
        }
    }

    fn handle_get_screen_text(&mut self, id: JsonValue) -> RpcResponse {
        let spec = match self.require_spectrum(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        // Read the Spectrum character set from ROM and decode the screen.
        // The Spectrum screen is 32×24 characters. Each character cell is
        // 8×8 pixels. We try to match each bitmap byte pattern against the
        // ROM character set at $3D00-$3FFF.
        let mut lines = Vec::new();

        for row in 0..24u8 {
            let mut line = String::with_capacity(32);
            for col in 0..32u8 {
                let ch = read_screen_char(spec, row, col);
                line.push(ch);
            }
            lines.push(line);
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "rows": 24,
                "cols": 32,
                "lines": lines,
            }),
        )
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a key name string into a `SpectrumKey`.
fn parse_key_name(name: &str) -> Option<SpectrumKey> {
    match name.to_lowercase().as_str() {
        "caps_shift" | "capsshift" | "shift" => Some(SpectrumKey::CapsShift),
        "sym_shift" | "symshift" | "symbol" => Some(SpectrumKey::SymShift),
        "enter" | "return" => Some(SpectrumKey::Enter),
        "space" => Some(SpectrumKey::Space),
        "a" => Some(SpectrumKey::A),
        "b" => Some(SpectrumKey::B),
        "c" => Some(SpectrumKey::C),
        "d" => Some(SpectrumKey::D),
        "e" => Some(SpectrumKey::E),
        "f" => Some(SpectrumKey::F),
        "g" => Some(SpectrumKey::G),
        "h" => Some(SpectrumKey::H),
        "i" => Some(SpectrumKey::I),
        "j" => Some(SpectrumKey::J),
        "k" => Some(SpectrumKey::K),
        "l" => Some(SpectrumKey::L),
        "m" => Some(SpectrumKey::M),
        "n" => Some(SpectrumKey::N),
        "o" => Some(SpectrumKey::O),
        "p" => Some(SpectrumKey::P),
        "q" => Some(SpectrumKey::Q),
        "r" => Some(SpectrumKey::R),
        "s" => Some(SpectrumKey::S),
        "t" => Some(SpectrumKey::T),
        "u" => Some(SpectrumKey::U),
        "v" => Some(SpectrumKey::V),
        "w" => Some(SpectrumKey::W),
        "x" => Some(SpectrumKey::X),
        "y" => Some(SpectrumKey::Y),
        "z" => Some(SpectrumKey::Z),
        "0" => Some(SpectrumKey::N0),
        "1" => Some(SpectrumKey::N1),
        "2" => Some(SpectrumKey::N2),
        "3" => Some(SpectrumKey::N3),
        "4" => Some(SpectrumKey::N4),
        "5" => Some(SpectrumKey::N5),
        "6" => Some(SpectrumKey::N6),
        "7" => Some(SpectrumKey::N7),
        "8" => Some(SpectrumKey::N8),
        "9" => Some(SpectrumKey::N9),
        _ => None,
    }
}

/// Read a character cell from the screen bitmap and match it against the ROM
/// character set. Returns the best matching ASCII character, or ' ' if no match.
fn read_screen_char(spectrum: &Spectrum, row: u8, col: u8) -> char {
    let mem = &*spectrum.bus().memory;

    // Read the 8 bitmap bytes for this character cell.
    let screen_y = row * 8;
    let mut cell = [0u8; 8];
    for py in 0..8u8 {
        let y = screen_y + py;
        let y7y6 = (y >> 6) & 0x03;
        let y5y4y3 = (y >> 3) & 0x07;
        let y2y1y0 = y & 0x07;
        let addr: u16 = 0x4000
            | (u16::from(y7y6) << 11)
            | (u16::from(y2y1y0) << 8)
            | (u16::from(y5y4y3) << 5)
            | u16::from(col);
        cell[py as usize] = mem.peek(addr);
    }

    // The ROM character set is at $3D00 for chars 32-127 (space through DEL).
    // Each character is 8 bytes.
    let mut best_char = ' ';
    let mut best_match = 0u32;

    for ch_idx in 0..96u16 {
        let rom_addr = 0x3D00 + ch_idx * 8;
        let mut matching_bits = 0u32;
        let mut all_match = true;

        for py in 0..8usize {
            let rom_byte = mem.peek(rom_addr + py as u16);
            if rom_byte == cell[py] {
                matching_bits += 8;
            } else {
                all_match = false;
                // Count matching bits
                let diff = rom_byte ^ cell[py];
                matching_bits += 8 - u32::from(diff.count_ones() as u8);
            }
        }

        if all_match {
            return char::from(ch_idx as u8 + 32);
        }

        if matching_bits > best_match {
            best_match = matching_bits;
            best_char = char::from(ch_idx as u8 + 32);
        }
    }

    // Only return a match if it's reasonably confident (>75% bits match)
    if best_match >= 48 { best_char } else { ' ' }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_names() {
        assert_eq!(parse_key_name("a"), Some(SpectrumKey::A));
        assert_eq!(parse_key_name("A"), Some(SpectrumKey::A));
        assert_eq!(parse_key_name("enter"), Some(SpectrumKey::Enter));
        assert_eq!(parse_key_name("return"), Some(SpectrumKey::Enter));
        assert_eq!(parse_key_name("space"), Some(SpectrumKey::Space));
        assert_eq!(parse_key_name("caps_shift"), Some(SpectrumKey::CapsShift));
        assert_eq!(parse_key_name("0"), Some(SpectrumKey::N0));
        assert_eq!(parse_key_name("9"), Some(SpectrumKey::N9));
        assert_eq!(parse_key_name("unknown"), None);
    }

    #[test]
    fn boot_creates_spectrum() {
        let mut server = McpServer::new();
        assert!(server.spectrum.is_none());

        let resp = server.dispatch("boot", &JsonValue::Null, JsonValue::from(1));
        assert!(resp.error.is_none());
        assert!(server.spectrum.is_some());
    }

    #[test]
    fn run_frames_without_boot_returns_error() {
        let mut server = McpServer::new();
        let resp = server.dispatch(
            "run_frames",
            &serde_json::json!({"count": 1}),
            JsonValue::from(1),
        );
        assert!(resp.error.is_some());
    }

    #[test]
    fn boot_and_run_frames() {
        let mut server = McpServer::new();
        server.dispatch("boot", &JsonValue::Null, JsonValue::from(1));

        let resp = server.dispatch(
            "run_frames",
            &serde_json::json!({"count": 10}),
            JsonValue::from(2),
        );
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["frames"], 10);
    }

    #[test]
    fn screenshot_returns_base64_png() {
        let mut server = McpServer::new();
        server.dispatch("boot", &JsonValue::Null, JsonValue::from(1));
        server.dispatch(
            "run_frames",
            &serde_json::json!({"count": 5}),
            JsonValue::from(2),
        );

        let resp = server.dispatch("screenshot", &JsonValue::Null, JsonValue::from(3));
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["format"], "png");
        assert_eq!(result["width"], 320);
        assert_eq!(result["height"], 288);
        assert!(result["data"].as_str().unwrap().len() > 100);
    }

    #[test]
    fn query_cpu_pc() {
        let mut server = McpServer::new();
        server.dispatch("boot", &JsonValue::Null, JsonValue::from(1));

        let resp = server.dispatch(
            "query",
            &serde_json::json!({"path": "cpu.pc"}),
            JsonValue::from(2),
        );
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["path"], "cpu.pc");
    }

    #[test]
    fn poke_memory() {
        let mut server = McpServer::new();
        server.dispatch("boot", &JsonValue::Null, JsonValue::from(1));

        let resp = server.dispatch(
            "poke",
            &serde_json::json!({"address": 0x8000, "value": 0xAB}),
            JsonValue::from(2),
        );
        assert!(resp.error.is_none());

        // Verify with query
        let resp = server.dispatch(
            "query",
            &serde_json::json!({"path": "memory.0x8000"}),
            JsonValue::from(3),
        );
        let result = resp.result.unwrap();
        assert_eq!(result["value"], 0xAB);
    }

    #[test]
    fn press_and_release_key() {
        let mut server = McpServer::new();
        server.dispatch("boot", &JsonValue::Null, JsonValue::from(1));

        let resp = server.dispatch(
            "press_key",
            &serde_json::json!({"key": "a"}),
            JsonValue::from(2),
        );
        assert!(resp.error.is_none());

        let resp = server.dispatch(
            "release_key",
            &serde_json::json!({"key": "a"}),
            JsonValue::from(3),
        );
        assert!(resp.error.is_none());
    }

    #[test]
    fn unknown_method_returns_error() {
        let mut server = McpServer::new();
        let resp = server.dispatch("nonexistent", &JsonValue::Null, JsonValue::from(1));
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn get_screen_text_after_boot() {
        let mut server = McpServer::new();
        server.dispatch("boot", &JsonValue::Null, JsonValue::from(1));
        server.dispatch(
            "run_frames",
            &serde_json::json!({"count": 200}),
            JsonValue::from(2),
        );

        let resp = server.dispatch("get_screen_text", &JsonValue::Null, JsonValue::from(3));
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["rows"], 24);
        assert_eq!(result["cols"], 32);
        // The copyright message should be somewhere in the text
        let lines: Vec<String> = result["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let all_text = lines.join("\n");
        assert!(
            all_text.contains("1982"),
            "Screen should show copyright year: {}",
            all_text
        );
    }
}
