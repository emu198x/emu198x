//! MCP (Model Context Protocol) server for the NES emulator.
//!
//! Exposes the emulator as a JSON-RPC 2.0 server over stdin/stdout.
//! Tools allow AI agents and scripts to boot, control, observe, and
//! capture the emulator programmatically.

#![allow(
    clippy::cast_possible_truncation,
    clippy::redundant_closure_for_method_calls
)]
#![allow(clippy::too_many_lines, clippy::match_same_arms)]

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use emu_core::{Cpu, Observable, Tickable};

use crate::Nes;
use crate::config::{NesConfig, NesRegion};
use crate::input::NesButton;

fn parse_region(params: &JsonValue) -> NesRegion {
    match params.get("region").and_then(|v| v.as_str()) {
        Some("pal") => NesRegion::Pal,
        _ => NesRegion::Ntsc,
    }
}

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

/// MCP server wrapping a headless NES instance.
pub struct McpServer {
    nes: Option<Nes>,
    rom_path: Option<PathBuf>,
}

impl McpServer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            nes: None,
            rom_path: None,
        }
    }

    /// Set a default ROM path (from CLI --rom argument).
    pub fn set_rom_path(&mut self, path: PathBuf) {
        self.rom_path = Some(path);
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
            "boot" => self.handle_boot(params, id),
            "reset" => self.handle_reset(id),
            "load_rom" => self.handle_load_rom(params, id),
            "run_frames" => self.handle_run_frames(params, id),
            "step_instruction" => self.handle_step_instruction(id),
            "step_ticks" => self.handle_step_ticks(params, id),
            "screenshot" => self.handle_screenshot(id),
            "query" => self.handle_query(params, id),
            "poke" => self.handle_poke(params, id),
            "press_button" => self.handle_press_button(params, id),
            "release_button" => self.handle_release_button(params, id),
            "input_sequence" => self.handle_input_sequence(params, id),
            "set_breakpoint" => self.handle_set_breakpoint(params, id),
            "query_memory" => self.handle_query_memory(params, id),
            _ => RpcResponse::error(id, -32601, format!("Unknown method: {method}")),
        }
    }

    fn require_nes(&mut self, id: &JsonValue) -> Result<&mut Nes, RpcResponse> {
        if self.nes.is_some() {
            Ok(self.nes.as_mut().expect("checked is_some"))
        } else {
            Err(RpcResponse::error(
                id.clone(),
                -32000,
                "No NES instance. Call 'boot' first.".to_string(),
            ))
        }
    }

    // === Tool handlers ===

    fn handle_boot(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        // Load ROM from params, path, or default
        let rom_data = if let Some(b64) = params.get("data").and_then(|v| v.as_str()) {
            match base64::engine::general_purpose::STANDARD.decode(b64) {
                Ok(d) => d,
                Err(e) => return RpcResponse::error(id, -32602, format!("Invalid base64: {e}")),
            }
        } else if let Some(path) = params
            .get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.rom_path.clone())
        {
            match std::fs::read(&path) {
                Ok(d) => d,
                Err(e) => return RpcResponse::error(id, -32000, format!("Cannot read ROM: {e}")),
            }
        } else {
            return RpcResponse::error(
                id,
                -32602,
                "Provide 'data' (base64), 'path', or --rom CLI argument".to_string(),
            );
        };

        let config = NesConfig { rom_data, region: parse_region(params) };
        match Nes::new(&config) {
            Ok(nes) => {
                self.nes = Some(nes);
                RpcResponse::success(id, serde_json::json!({"status": "ok"}))
            }
            Err(e) => RpcResponse::error(id, -32000, format!("Boot failed: {e}")),
        }
    }

    fn handle_reset(&mut self, id: JsonValue) -> RpcResponse {
        match self.require_nes(&id) {
            Ok(nes) => {
                nes.cpu_mut().reset();
                RpcResponse::success(id, serde_json::json!({"status": "ok"}))
            }
            Err(e) => e,
        }
    }

    fn handle_load_rom(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let rom_data = if let Some(b64) = params.get("data").and_then(|v| v.as_str()) {
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

        let config = NesConfig { rom_data, region: parse_region(params) };
        match Nes::new(&config) {
            Ok(nes) => {
                self.nes = Some(nes);
                RpcResponse::success(id, serde_json::json!({"status": "ok"}))
            }
            Err(e) => RpcResponse::error(id, -32000, format!("ROM load failed: {e}")),
        }
    }

    fn handle_run_frames(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params
            .get("count")
            .and_then(|v| v.as_u64())
            .or_else(|| params.get("frames").and_then(|v| v.as_u64()))
            .unwrap_or(1);

        let mut total_ticks = 0u64;
        for _ in 0..count {
            total_ticks += nes.run_frame();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "frames": count,
                "ticks": total_ticks,
                "frame_count": nes.frame_count(),
            }),
        )
    }

    fn handle_step_instruction(&mut self, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let mut ticks = 0u64;
        let max_ticks = 200 * 12; // 200 CPU cycles worth
        let mut started = false;

        loop {
            nes.tick();
            ticks += 1;

            if nes.cpu().is_instruction_complete() {
                if started {
                    break;
                }
            } else {
                started = true;
            }

            if ticks >= max_ticks {
                break;
            }
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "pc": format!("${:04X}", nes.cpu().regs.pc),
                "ticks": ticks,
            }),
        )
    }

    fn handle_step_ticks(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(1);

        for _ in 0..count {
            nes.tick();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "pc": format!("${:04X}", nes.cpu().regs.pc),
            }),
        )
    }

    fn handle_screenshot(&mut self, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let width = nes.framebuffer_width();
        let height = nes.framebuffer_height();
        let fb = nes.framebuffer();

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

    fn handle_query(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let path = match params.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return RpcResponse::error(id, -32602, "Missing 'path' parameter".to_string()),
        };

        match nes.query(path) {
            Some(value) => {
                let json_val = observable_to_json(&value);
                RpcResponse::success(id, serde_json::json!({"path": path, "value": json_val}))
            }
            None => RpcResponse::error(id, -32000, format!("Unknown query path: {path}")),
        }
    }

    fn handle_poke(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0x07FF => a as u16,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'address' (0-2047, RAM only)".to_string(),
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

        nes.bus_mut().ram[addr as usize] = value;
        RpcResponse::success(id, serde_json::json!({"address": addr, "value": value}))
    }

    fn handle_press_button(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let name = match params.get("button").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                return RpcResponse::error(id, -32602, "Missing 'button' parameter".to_string());
            }
        };

        match parse_button_name(name) {
            Some(button) => {
                nes.press_button(button);
                RpcResponse::success(id, serde_json::json!({"button": name, "pressed": true}))
            }
            None => RpcResponse::error(id, -32602, format!("Unknown button: {name}")),
        }
    }

    fn handle_release_button(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let name = match params.get("button").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                return RpcResponse::error(id, -32602, "Missing 'button' parameter".to_string());
            }
        };

        match parse_button_name(name) {
            Some(button) => {
                nes.release_button(button);
                RpcResponse::success(id, serde_json::json!({"button": name, "pressed": false}))
            }
            None => RpcResponse::error(id, -32602, format!("Unknown button: {name}")),
        }
    }

    fn handle_input_sequence(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let sequence = match params.get("sequence").and_then(|v| v.as_array()) {
            Some(s) => s,
            None => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing 'sequence' array parameter".to_string(),
                );
            }
        };

        let hold_frames = params
            .get("hold_frames")
            .and_then(|v| v.as_u64())
            .unwrap_or(3);

        let gap_frames = params
            .get("gap_frames")
            .and_then(|v| v.as_u64())
            .unwrap_or(3);

        let start_frame = params
            .get("at_frame")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| nes.frame_count());

        let mut frame = start_frame;
        let mut count = 0u64;

        for item in sequence {
            let name = match item.as_str() {
                Some(n) => n,
                None => continue,
            };
            if let Some(button) = parse_button_name(name) {
                nes.input_queue().enqueue_button(button, frame, hold_frames);
                frame += hold_frames + gap_frames;
                count += 1;
            }
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "buttons_queued": count,
                "start_frame": start_frame,
                "end_frame": frame,
            }),
        )
    }

    fn handle_set_breakpoint(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
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

        let ticks_per_frame = 341 * 262 * 4;
        let mut ticks_run = 0u64;
        let mut hit = false;

        let max_ticks = max_frames * ticks_per_frame;

        while ticks_run < max_ticks {
            nes.tick();
            ticks_run += 1;

            if nes.cpu().regs.pc == addr && nes.cpu().is_instruction_complete() {
                hit = true;
                break;
            }
        }

        let frames_run = ticks_run / ticks_per_frame;

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
                    "pc": format!("${:04X}", nes.cpu().regs.pc),
                    "frames_run": frames_run,
                }),
            )
        }
    }

    fn handle_query_memory(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let nes = match self.require_nes(&id) {
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
            .map(|i| nes.bus().peek_ram(address.wrapping_add(i as u16)))
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

fn parse_button_name(name: &str) -> Option<NesButton> {
    match name.to_lowercase().as_str() {
        "a" => Some(NesButton::A),
        "b" => Some(NesButton::B),
        "select" => Some(NesButton::Select),
        "start" => Some(NesButton::Start),
        "up" => Some(NesButton::Up),
        "down" => Some(NesButton::Down),
        "left" => Some(NesButton::Left),
        "right" => Some(NesButton::Right),
        _ => None,
    }
}

fn observable_to_json(value: &emu_core::Value) -> JsonValue {
    match value {
        emu_core::Value::U8(v) => serde_json::json!(v),
        emu_core::Value::U16(v) => serde_json::json!(v),
        emu_core::Value::U32(v) => serde_json::json!(v),
        emu_core::Value::U64(v) => serde_json::json!(v),
        emu_core::Value::I8(v) => serde_json::json!(v),
        emu_core::Value::Bool(v) => serde_json::json!(v),
        emu_core::Value::String(v) => serde_json::json!(v),
        emu_core::Value::Array(v) => serde_json::json!(format!("{v:?}")),
        emu_core::Value::Map(v) => serde_json::json!(format!("{v:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_button_names() {
        assert_eq!(parse_button_name("a"), Some(NesButton::A));
        assert_eq!(parse_button_name("A"), Some(NesButton::A));
        assert_eq!(parse_button_name("start"), Some(NesButton::Start));
        assert_eq!(parse_button_name("select"), Some(NesButton::Select));
        assert_eq!(parse_button_name("up"), Some(NesButton::Up));
        assert_eq!(parse_button_name("unknown"), None);
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
}
