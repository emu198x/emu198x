//! MCP (Model Context Protocol) server for the Amiga emulator.
//!
//! Exposes the emulator as a JSON-RPC 2.0 server over stdin/stdout.

#![allow(clippy::cast_possible_truncation, clippy::redundant_closure_for_method_calls)]
#![allow(clippy::too_many_lines, clippy::match_same_arms)]

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use emu_core::{Cpu, Observable, Tickable};

use crate::config::AmigaConfig;
use crate::Amiga;

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

/// MCP server wrapping a headless Amiga instance.
pub struct McpServer {
    amiga: Option<Amiga>,
    kickstart_path: Option<PathBuf>,
}

impl McpServer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            amiga: None,
            kickstart_path: None,
        }
    }

    /// Set a default Kickstart path (from CLI --kickstart argument).
    pub fn set_kickstart_path(&mut self, path: PathBuf) {
        self.kickstart_path = Some(path);
    }

    /// Run the server loop.
    pub fn run(&mut self) {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut stdout = stdout.lock();

        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };

            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let request: RpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let resp = RpcResponse::error(
                        JsonValue::Null,
                        -32700,
                        format!("Parse error: {e}"),
                    );
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
                let resp = RpcResponse::error(
                    request.id,
                    -32600,
                    "Invalid JSON-RPC version".to_string(),
                );
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
            "run_frames" => self.handle_run_frames(params, id),
            "step_ticks" => self.handle_step_ticks(params, id),
            "screenshot" => self.handle_screenshot(id),
            "query" => self.handle_query(params, id),
            "poke" => self.handle_poke(params, id),
            _ => RpcResponse::error(id, -32601, format!("Unknown method: {method}")),
        }
    }

    fn require_amiga(&mut self, id: &JsonValue) -> Result<&mut Amiga, RpcResponse> {
        if self.amiga.is_some() {
            Ok(self.amiga.as_mut().expect("checked is_some"))
        } else {
            Err(RpcResponse::error(
                id.clone(),
                -32000,
                "No Amiga instance. Call 'boot' first.".to_string(),
            ))
        }
    }

    fn handle_boot(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let kickstart_data = if let Some(b64) = params.get("data").and_then(|v| v.as_str()) {
            match base64::engine::general_purpose::STANDARD.decode(b64) {
                Ok(d) => d,
                Err(e) => return RpcResponse::error(id, -32602, format!("Invalid base64: {e}")),
            }
        } else if let Some(path) = params
            .get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.kickstart_path.clone())
        {
            match std::fs::read(&path) {
                Ok(d) => d,
                Err(e) => {
                    return RpcResponse::error(
                        id,
                        -32000,
                        format!("Cannot read Kickstart: {e}"),
                    )
                }
            }
        } else {
            return RpcResponse::error(
                id,
                -32602,
                "Provide 'data' (base64), 'path', or --kickstart CLI argument".to_string(),
            );
        };

        let config = AmigaConfig {
            kickstart: kickstart_data,
        };
        match Amiga::new(&config) {
            Ok(amiga) => {
                self.amiga = Some(amiga);
                RpcResponse::success(id, serde_json::json!({"status": "ok"}))
            }
            Err(e) => RpcResponse::error(id, -32000, format!("Boot failed: {e}")),
        }
    }

    fn handle_reset(&mut self, id: JsonValue) -> RpcResponse {
        match self.require_amiga(&id) {
            Ok(amiga) => {
                amiga.cpu_mut().reset();
                RpcResponse::success(id, serde_json::json!({"status": "ok"}))
            }
            Err(e) => e,
        }
    }

    fn handle_run_frames(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);

        let mut total_ticks = 0u64;
        for _ in 0..count {
            total_ticks += amiga.run_frame();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "frames": count,
                "ticks": total_ticks,
                "frame_count": amiga.frame_count(),
            }),
        )
    }

    fn handle_step_ticks(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);

        for _ in 0..count {
            amiga.tick();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "pc": format!("${:08X}", amiga.cpu().regs.pc),
            }),
        )
    }

    fn handle_screenshot(&mut self, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let width = amiga.framebuffer_width();
        let height = amiga.framebuffer_height();
        let fb = amiga.framebuffer();

        let mut png_buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_buf, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = match encoder.write_header() {
                Ok(w) => w,
                Err(e) => {
                    return RpcResponse::error(id, -32000, format!("PNG encode error: {e}"))
                }
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
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let Some(path) = params.get("path").and_then(|v| v.as_str()) else {
            return RpcResponse::error(id, -32602, "Missing 'path' parameter".to_string());
        };

        match amiga.query(path) {
            Some(value) => {
                let json_val = observable_to_json(&value);
                RpcResponse::success(
                    id,
                    serde_json::json!({"path": path, "value": json_val}),
                )
            }
            None => RpcResponse::error(
                id,
                -32000,
                format!("Unknown query path: {path}"),
            ),
        }
    }

    fn handle_poke(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= crate::memory::CHIP_RAM_MASK as u64 => a as u32,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'address' (0-524287, chip RAM only)".to_string(),
                )
            }
        };

        let value = match params.get("value").and_then(|v| v.as_u64()) {
            Some(v) if v <= 0xFF => v as u8,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'value' (0-255)".to_string(),
                )
            }
        };

        amiga.bus_mut().memory.chip_ram[addr as usize] = value;
        RpcResponse::success(
            id,
            serde_json::json!({"address": addr, "value": value}),
        )
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
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
