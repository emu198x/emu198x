//! MCP (Model Context Protocol) server for the C64 emulator.
//!
//! Exposes the emulator as a JSON-RPC 2.0 server over stdin/stdout.
//! Tools allow AI agents and scripts to boot, control, observe, and
//! capture the emulator programmatically.

#![allow(clippy::cast_possible_truncation)]

use std::io::{self, BufRead, Write};
use std::path::Path;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use emu_core::{Cpu, Observable, Tickable};

use crate::C64;
use crate::config::{C64Config, C64Model};
use crate::input::C64Key;

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

/// MCP server wrapping a headless C64 instance.
pub struct McpServer {
    c64: Option<C64>,
}

impl McpServer {
    #[must_use]
    pub fn new() -> Self {
        Self { c64: None }
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

    fn dispatch(&mut self, method: &str, params: &JsonValue, id: JsonValue) -> RpcResponse {
        match method {
            "boot" => self.handle_boot(id),
            "reset" => self.handle_reset(id),
            "load_prg" => self.handle_load_prg(params, id),
            "run_frames" => self.handle_run_frames(params, id),
            "step_instruction" => self.handle_step_instruction(id),
            "step_ticks" => self.handle_step_ticks(params, id),
            "screenshot" => self.handle_screenshot(id),
            "audio_capture" => self.handle_audio_capture(params, id),
            "query" => self.handle_query(params, id),
            "boot_detected" => self.handle_boot_detected(id),
            "boot_status" => self.handle_boot_status(id),
            "poke" => self.handle_poke(params, id),
            "press_key" => self.handle_press_key(params, id),
            "release_key" => self.handle_release_key(params, id),
            "type_text" => self.handle_type_text(params, id),
            "set_breakpoint" => self.handle_set_breakpoint(params, id),
            "get_screen_text" => self.handle_get_screen_text(id),
            "query_memory" => self.handle_query_memory(params, id),
            "load_d64" => self.handle_load_d64(params, id),
            _ => RpcResponse::error(id, -32601, format!("Unknown method: {method}")),
        }
    }

    fn require_c64(&mut self, id: &JsonValue) -> Result<&mut C64, RpcResponse> {
        if self.c64.is_some() {
            Ok(self.c64.as_mut().expect("checked is_some"))
        } else {
            Err(RpcResponse::error(
                id.clone(),
                -32000,
                "No C64 instance. Call 'boot' first.".to_string(),
            ))
        }
    }

    // === Tool handlers ===

    fn handle_boot(&mut self, id: JsonValue) -> RpcResponse {
        let config = match load_c64_config() {
            Ok(c) => c,
            Err(e) => return RpcResponse::error(id, -32000, e),
        };
        self.c64 = Some(C64::new(&config));
        RpcResponse::success(id, serde_json::json!({"status": "ok"}))
    }

    fn handle_reset(&mut self, id: JsonValue) -> RpcResponse {
        match self.require_c64(&id) {
            Ok(c64) => {
                c64.cpu_mut().reset();
                RpcResponse::success(id, serde_json::json!({"status": "ok"}))
            }
            Err(e) => e,
        }
    }

    fn handle_load_prg(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
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

        match c64.load_prg(&data) {
            Ok(addr) => RpcResponse::success(
                id,
                serde_json::json!({"status": "ok", "load_address": format!("${addr:04X}")}),
            ),
            Err(e) => RpcResponse::error(id, -32000, format!("PRG load failed: {e}")),
        }
    }

    fn handle_run_frames(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params
            .get("count")
            .and_then(|v| v.as_u64())
            .or_else(|| params.get("frames").and_then(|v| v.as_u64()))
            .unwrap_or(1);

        let mut total_cycles = 0u64;
        for _ in 0..count {
            total_cycles += c64.run_frame();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "frames": count,
                "cycles": total_cycles,
                "frame_count": c64.frame_count(),
            }),
        )
    }

    fn handle_step_instruction(&mut self, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let mut cycles = 0u64;
        let max_cycles = 200;
        let mut started = false;

        loop {
            c64.tick();
            cycles += 1;

            if c64.cpu().is_instruction_complete() {
                if started {
                    break;
                }
            } else {
                started = true;
            }

            if cycles >= max_cycles {
                break;
            }
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "pc": format!("${:04X}", c64.cpu().regs.pc),
                "cycles": cycles,
            }),
        )
    }

    fn handle_step_ticks(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(1);

        for _ in 0..count {
            c64.tick();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "pc": format!("${:04X}", c64.cpu().regs.pc),
            }),
        )
    }

    fn handle_screenshot(&mut self, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let width = c64.framebuffer_width();
        let height = c64.framebuffer_height();
        let fb = c64.framebuffer();

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
                rgba.push(((pixel >> 16) & 0xFF) as u8);
                rgba.push(((pixel >> 8) & 0xFF) as u8);
                rgba.push((pixel & 0xFF) as u8);
                rgba.push(0xFF);
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
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let frames = params.get("frames").and_then(|v| v.as_u64()).unwrap_or(50);

        for _ in 0..frames {
            c64.run_frame();
        }

        // SID audio is stubbed — return empty audio
        RpcResponse::success(
            id,
            serde_json::json!({
                "format": "wav",
                "samples": 0,
                "frames": frames,
                "data": "",
            }),
        )
    }

    fn handle_query(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let path = match params.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return RpcResponse::error(id, -32602, "Missing 'path' parameter".to_string()),
        };

        match c64.query(path) {
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
        let c64 = match self.require_c64(&id) {
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

        c64.bus_mut().memory.ram_write(addr, value);
        RpcResponse::success(id, serde_json::json!({"address": addr, "value": value}))
    }

    fn handle_press_key(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let key_name = match params.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return RpcResponse::error(id, -32602, "Missing 'key' parameter".to_string()),
        };

        match parse_key_name(key_name) {
            Some(key) => {
                c64.press_key(key);
                RpcResponse::success(id, serde_json::json!({"key": key_name, "pressed": true}))
            }
            None => RpcResponse::error(id, -32602, format!("Unknown key: {key_name}")),
        }
    }

    fn handle_release_key(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let key_name = match params.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return RpcResponse::error(id, -32602, "Missing 'key' parameter".to_string()),
        };

        match parse_key_name(key_name) {
            Some(key) => {
                c64.release_key(key);
                RpcResponse::success(id, serde_json::json!({"key": key_name, "pressed": false}))
            }
            None => RpcResponse::error(id, -32602, format!("Unknown key: {key_name}")),
        }
    }

    fn handle_type_text(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
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
            .unwrap_or_else(|| c64.frame_count());

        let end_frame = c64.input_queue().enqueue_text(&text, at_frame);
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
        let c64 = match self.require_c64(&id) {
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
            loop {
                c64.tick();
                if c64.cpu().regs.pc == addr {
                    hit = true;
                    break 'outer;
                }
                if c64.bus_mut().vic.take_frame_complete() {
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
                    "pc": format!("${:04X}", c64.cpu().regs.pc),
                    "frames_run": frames_run,
                }),
            )
        }
    }

    fn handle_get_screen_text(&mut self, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let lines = read_screen_lines(c64);

        RpcResponse::success(
            id,
            serde_json::json!({
                "rows": 25,
                "cols": 40,
                "lines": lines,
            }),
        )
    }

    fn handle_boot_detected(&mut self, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let lines = read_screen_lines(c64);
        let (detected, reason) = detect_boot(&lines);

        RpcResponse::success(
            id,
            serde_json::json!({
                "boot_detected": detected,
                "reason": reason,
            }),
        )
    }

    fn handle_boot_status(&mut self, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let lines = read_screen_lines(c64);
        let (detected, reason) = detect_boot(&lines);

        RpcResponse::success(
            id,
            serde_json::json!({
                "boot_detected": detected,
                "reason": reason,
                "frame_count": c64.frame_count(),
                "rows": 25,
                "cols": 40,
                "lines": lines,
            }),
        )
    }

    fn handle_query_memory(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let address = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0xFFFF => a as u16,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'address' (0-65535)".to_string(),
                );
            }
        };

        let length = match params.get("length").and_then(|v| v.as_u64()) {
            Some(l) if l >= 1 && l <= 65536 => l as usize,
            Some(_) => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Invalid 'length' (1-65536)".to_string(),
                );
            }
            None => {
                return RpcResponse::error(id, -32602, "Missing 'length' parameter".to_string());
            }
        };

        let bytes: Vec<u8> = (0..length)
            .map(|i| c64.bus().memory.peek(address.wrapping_add(i as u16)))
            .collect();

        RpcResponse::success(
            id,
            serde_json::json!({
                "address": address,
                "length": length,
                "data": bytes,
            }),
        )
    }

    fn handle_load_d64(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let c64 = match self.require_c64(&id) {
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

        match c64.load_d64(&data) {
            Ok(()) => RpcResponse::success(
                id,
                serde_json::json!({"status": "ok", "size": data.len()}),
            ),
            Err(e) => RpcResponse::error(id, -32000, format!("D64 load failed: {e}")),
        }
    }

    /// Run a script file: read a JSON array of simplified RPC requests, dispatch
    /// each in order, and write JSON-line responses to stdout.
    pub fn run_script(&mut self, path: &Path) -> io::Result<()> {
        let data = std::fs::read_to_string(path)?;
        let steps: Vec<ScriptStep> = serde_json::from_str(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let stdout = io::stdout();
        let mut stdout = stdout.lock();

        for (i, step) in steps.iter().enumerate() {
            let id = JsonValue::from(i as u64 + 1);
            let params = step.params.clone().unwrap_or(JsonValue::Object(Default::default()));
            let response = self.dispatch(&step.method, &params, id);

            let _ = writeln!(stdout, "{}", serde_json::to_string(&response).unwrap_or_default());
            let _ = stdout.flush();

            if let Some(save_path) = params.get("save_path").and_then(|v| v.as_str()) {
                if let Some(ref result) = response.result {
                    if let Some(data_b64) = result.get("data").and_then(|v| v.as_str()) {
                        if let Err(e) = save_capture_data(save_path, data_b64) {
                            eprintln!("Failed to save {save_path}: {e}");
                        } else {
                            eprintln!("Saved {save_path}");
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// A single step in a script file.
#[derive(Deserialize)]
struct ScriptStep {
    method: String,
    #[serde(default)]
    params: Option<JsonValue>,
}

/// Decode base64 capture data and write to a file.
fn save_capture_data(path: &str, data_b64: &str) -> io::Result<()> {
    if data_b64.is_empty() {
        return Ok(());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, bytes)
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a key name string into a `C64Key`.
fn parse_key_name(name: &str) -> Option<C64Key> {
    match name.to_lowercase().as_str() {
        "a" => Some(C64Key::A),
        "b" => Some(C64Key::B),
        "c" => Some(C64Key::C),
        "d" => Some(C64Key::D),
        "e" => Some(C64Key::E),
        "f" => Some(C64Key::F),
        "g" => Some(C64Key::G),
        "h" => Some(C64Key::H),
        "i" => Some(C64Key::I),
        "j" => Some(C64Key::J),
        "k" => Some(C64Key::K),
        "l" => Some(C64Key::L),
        "m" => Some(C64Key::M),
        "n" => Some(C64Key::N),
        "o" => Some(C64Key::O),
        "p" => Some(C64Key::P),
        "q" => Some(C64Key::Q),
        "r" => Some(C64Key::R),
        "s" => Some(C64Key::S),
        "t" => Some(C64Key::T),
        "u" => Some(C64Key::U),
        "v" => Some(C64Key::V),
        "w" => Some(C64Key::W),
        "x" => Some(C64Key::X),
        "y" => Some(C64Key::Y),
        "z" => Some(C64Key::Z),
        "0" => Some(C64Key::N0),
        "1" => Some(C64Key::N1),
        "2" => Some(C64Key::N2),
        "3" => Some(C64Key::N3),
        "4" => Some(C64Key::N4),
        "5" => Some(C64Key::N5),
        "6" => Some(C64Key::N6),
        "7" => Some(C64Key::N7),
        "8" => Some(C64Key::N8),
        "9" => Some(C64Key::N9),
        "return" | "enter" => Some(C64Key::Return),
        "space" => Some(C64Key::Space),
        "delete" | "backspace" | "del" => Some(C64Key::Delete),
        "lshift" | "left_shift" => Some(C64Key::LShift),
        "rshift" | "right_shift" => Some(C64Key::RShift),
        "ctrl" | "control" => Some(C64Key::Ctrl),
        "commodore" | "c=" => Some(C64Key::Commodore),
        "f1" => Some(C64Key::F1),
        "f3" => Some(C64Key::F3),
        "f5" => Some(C64Key::F5),
        "f7" => Some(C64Key::F7),
        "home" => Some(C64Key::Home),
        "runstop" | "run_stop" | "stop" => Some(C64Key::RunStop),
        "cursor_down" | "down" => Some(C64Key::CursorDown),
        "cursor_right" | "right" => Some(C64Key::CursorRight),
        "plus" | "+" => Some(C64Key::Plus),
        "minus" | "-" => Some(C64Key::Minus),
        "period" | "." => Some(C64Key::Period),
        "comma" | "," => Some(C64Key::Comma),
        "colon" | ":" => Some(C64Key::Colon),
        "semicolon" | ";" => Some(C64Key::Semicolon),
        "equals" | "=" => Some(C64Key::Equals),
        "slash" | "/" => Some(C64Key::Slash),
        "asterisk" | "*" => Some(C64Key::Asterisk),
        "at" | "@" => Some(C64Key::At),
        _ => None,
    }
}

/// Find the roms/ directory and load C64 ROM files.
fn load_c64_config() -> Result<C64Config, String> {
    let roms_dir = find_roms_dir();
    let kernal = load_rom_file(&roms_dir.join("kernal.rom"), "Kernal", 8192)?;
    let basic = load_rom_file(&roms_dir.join("basic.rom"), "BASIC", 8192)?;
    let chargen = load_rom_file(&roms_dir.join("chargen.rom"), "Character", 4096)?;
    Ok(C64Config {
        model: C64Model::C64Pal,
        sid_model: crate::config::SidModel::Sid6581,
        kernal_rom: kernal,
        basic_rom: basic,
        char_rom: chargen,
        drive_rom: None,
        reu_size: None,
    })
}

fn find_roms_dir() -> std::path::PathBuf {
    use std::path::Path;
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        for _ in 0..5 {
            if let Some(ref d) = dir {
                let roms = d.join("roms");
                if roms.is_dir() {
                    return roms;
                }
                dir = d.parent().map(Path::to_path_buf);
            }
        }
    }
    std::path::PathBuf::from("roms")
}

fn load_rom_file(
    path: &std::path::Path,
    name: &str,
    expected_size: usize,
) -> Result<Vec<u8>, String> {
    let data = std::fs::read(path)
        .map_err(|e| format!("Cannot read {name} ROM at {}: {e}", path.display()))?;
    if data.len() != expected_size {
        return Err(format!(
            "{name} ROM is {} bytes, expected {expected_size}",
            data.len()
        ));
    }
    Ok(data)
}

/// Convert a C64 screen code to an ASCII character.
///
/// Screen codes differ from PETSCII: 0-31 = @A-Z[\]^_, 32-63 = space to ?,
/// etc. This handles the common printable range.
fn screen_code_to_ascii(code: u8) -> char {
    match code {
        0x00 => '@',
        0x01..=0x1A => (b'A' + code - 1) as char,
        0x1B => '[',
        0x1C => '\\',
        0x1D => ']',
        0x1E => '^',
        0x1F => '_',
        0x20 => ' ',
        0x21..=0x3F => (b'!' + code - 0x21) as char,
        // Reverse video versions (same characters)
        0x80 => '@',
        0x81..=0x9A => (b'A' + code - 0x81) as char,
        0xA0 => ' ',
        0xA1..=0xBF => (b'!' + code - 0xA1) as char,
        _ => ' ', // Graphics characters → space
    }
}

fn read_screen_lines(c64: &C64) -> Vec<String> {
    let mut lines = Vec::new();
    let vic = &c64.bus().vic;
    let d018 = vic.peek(0x18);
    let screen_base = u16::from((d018 >> 4) & 0x0F) * 0x0400;
    let bank_offset = u16::from(vic.bank()) * 0x4000;
    let screen_start = bank_offset + screen_base;

    for row in 0..25u16 {
        let mut line = String::with_capacity(40);
        for col in 0..40u16 {
            let addr = screen_start + row * 40 + col;
            let screen_code = c64.bus().memory.ram_read(addr);
            line.push(screen_code_to_ascii(screen_code));
        }
        lines.push(line);
    }

    lines
}

fn detect_boot(lines: &[String]) -> (bool, &'static str) {
    for line in lines {
        if line.contains("COMMODORE 64 BASIC") {
            return (true, "found COMMODORE 64 BASIC banner");
        }
        if line.contains("READY.") {
            return (true, "found READY.");
        }
        if line.contains("BASIC BYTES FREE") {
            return (true, "found BASIC BYTES FREE line");
        }
    }
    (false, "no boot banner detected")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_names() {
        assert_eq!(parse_key_name("a"), Some(C64Key::A));
        assert_eq!(parse_key_name("A"), Some(C64Key::A));
        assert_eq!(parse_key_name("return"), Some(C64Key::Return));
        assert_eq!(parse_key_name("enter"), Some(C64Key::Return));
        assert_eq!(parse_key_name("space"), Some(C64Key::Space));
        assert_eq!(parse_key_name("0"), Some(C64Key::N0));
        assert_eq!(parse_key_name("f1"), Some(C64Key::F1));
        assert_eq!(parse_key_name("unknown"), None);
    }

    #[test]
    fn screen_code_conversion() {
        assert_eq!(screen_code_to_ascii(0x00), '@');
        assert_eq!(screen_code_to_ascii(0x01), 'A');
        assert_eq!(screen_code_to_ascii(0x1A), 'Z');
        assert_eq!(screen_code_to_ascii(0x20), ' ');
        assert_eq!(screen_code_to_ascii(0x30), '0');
        assert_eq!(screen_code_to_ascii(0x39), '9');
    }

    #[test]
    fn unknown_method_returns_error() {
        let mut server = McpServer::new();
        let resp = server.dispatch("nonexistent", &JsonValue::Null, JsonValue::from(1));
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().map(|e| e.code), Some(-32601));
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
    fn detect_boot_reports_ready() {
        let lines = vec!["READY.".to_string()];
        let (detected, reason) = detect_boot(&lines);
        assert!(detected);
        assert!(reason.contains("READY"));
    }

    #[test]
    fn detect_boot_reports_banner() {
        let lines = vec!["**** COMMODORE 64 BASIC V2 ****".to_string()];
        let (detected, reason) = detect_boot(&lines);
        assert!(detected);
        assert!(reason.contains("COMMODORE"));
    }

    #[test]
    fn detect_boot_reports_missing() {
        let lines = vec!["HELLO WORLD".to_string()];
        let (detected, reason) = detect_boot(&lines);
        assert!(!detected);
        assert!(reason.contains("no boot"));
    }
}
