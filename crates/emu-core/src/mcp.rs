//! Shared MCP (Model Context Protocol) server for Emu198x emulators.
//!
//! Wraps any emulator's tool dispatch behind the MCP protocol. Handles
//! `initialize`, `tools/list`, and `tools/call` so each emulator only
//! needs to provide tool definitions and a dispatch function.
//!
//! Wire format: newline-delimited JSON-RPC 2.0 over stdin/stdout.

#![allow(clippy::module_name_repetitions)]

use std::io::{self, BufRead, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::Value;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single tool definition for `tools/list`.
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: JsonValue,
}

/// Result of dispatching a tool call.
pub enum ToolResult {
    /// Successful execution — the JSON value is returned as text content.
    Success(JsonValue),
    /// Tool-level error — returned as `isError: true` in MCP, or as a
    /// JSON-RPC error in raw/script mode.
    Error { code: i32, message: String },
}

/// Trait that each emulator implements to plug into the MCP server.
pub trait McpEmulator {
    /// Dispatch a single tool call by name and return the result.
    fn dispatch_tool(&mut self, name: &str, arguments: &JsonValue) -> ToolResult;

    /// Return all tool definitions (for `tools/list`).
    fn tool_definitions(&self) -> Vec<ToolDefinition>;

    /// Server name shown in the `initialize` response.
    fn server_name(&self) -> &str;

    /// Server version shown in the `initialize` response.
    fn server_version(&self) -> &str;
}

// ---------------------------------------------------------------------------
// JSON-RPC message types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RpcMessage {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: JsonValue,
    /// `None` for notifications (e.g. `notifications/initialized`).
    id: Option<JsonValue>,
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

/// MCP protocol server wrapping any `McpEmulator`.
pub struct McpServer<T> {
    inner: T,
}

impl<T: McpEmulator> McpServer<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Run the MCP protocol loop over stdin/stdout.
    pub fn run(&mut self) {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut stdout = stdout.lock();

        for line in stdin.lock().lines() {
            let Ok(line) = line else {
                break;
            };

            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let msg: RpcMessage = match serde_json::from_str(&line) {
                Ok(m) => m,
                Err(e) => {
                    let resp =
                        RpcResponse::error(JsonValue::Null, -32700, format!("Parse error: {e}"));
                    write_response(&mut stdout, &resp);
                    continue;
                }
            };

            // Notifications have no id — don't send a response.
            if msg.id.is_none() {
                continue;
            }

            let id = msg.id.unwrap_or(JsonValue::Null);
            let response = self.handle(&msg.method, &msg.params, id);
            write_response(&mut stdout, &response);
        }
    }

    /// Run a script file (JSON array of `{method, params}` steps).
    /// Dispatches directly to tools (no MCP framing).
    pub fn run_script(&mut self, path: &Path) -> io::Result<()> {
        let data = std::fs::read_to_string(path)?;
        let steps: Vec<ScriptStep> = serde_json::from_str(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let stdout = io::stdout();
        let mut stdout = stdout.lock();

        for (i, step) in steps.iter().enumerate() {
            let id = JsonValue::from(i as u64 + 1);
            let params = step
                .params
                .clone()
                .unwrap_or(JsonValue::Object(serde_json::Map::default()));

            let response = match self.inner.dispatch_tool(&step.method, &params) {
                ToolResult::Success(val) => RpcResponse::success(id, val.clone()),
                ToolResult::Error { code, message } => RpcResponse::error(id, code, message),
            };

            write_response(&mut stdout, &response);

            // Script mode: if save_path was provided, save base64 data to file.
            if let Some(save_path) = params.get("save_path").and_then(|v| v.as_str())
                && let Some(ref result) = response.result
                && let Some(data_b64) = result.get("data").and_then(|v| v.as_str())
            {
                if let Err(e) = save_base64_to_file(save_path, data_b64) {
                    eprintln!("Failed to save {save_path}: {e}");
                } else {
                    eprintln!("Saved {save_path}");
                }
            }
        }

        Ok(())
    }

    fn handle(&mut self, method: &str, params: &JsonValue, id: JsonValue) -> RpcResponse {
        match method {
            "initialize" => self.handle_initialize(id),
            "tools/list" => self.handle_tools_list(id),
            "tools/call" => self.handle_tools_call(params, id),
            _ => RpcResponse::error(id, -32601, format!("Unknown method: {method}")),
        }
    }

    fn handle_initialize(&self, id: JsonValue) -> RpcResponse {
        RpcResponse::success(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": self.inner.server_name(),
                    "version": self.inner.server_version()
                }
            }),
        )
    }

    fn handle_tools_list(&self, id: JsonValue) -> RpcResponse {
        let tools: Vec<JsonValue> = self
            .inner
            .tool_definitions()
            .into_iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })
            })
            .collect();

        RpcResponse::success(id, serde_json::json!({ "tools": tools }))
    }

    fn handle_tools_call(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let Some(name) = params.get("name").and_then(|v| v.as_str()) else {
            return RpcResponse::error(
                id,
                -32602,
                "Missing 'name' in tools/call params".to_string(),
            );
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(JsonValue::Object(serde_json::Map::default()));

        match self.inner.dispatch_tool(name, &arguments) {
            ToolResult::Success(val) => {
                let text = serde_json::to_string(&val).unwrap_or_default();
                RpcResponse::success(
                    id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": text }]
                    }),
                )
            }
            ToolResult::Error { message, .. } => RpcResponse::success(
                id,
                serde_json::json!({
                    "content": [{ "type": "text", "text": message }],
                    "isError": true
                }),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Script step
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ScriptStep {
    method: String,
    #[serde(default)]
    params: Option<JsonValue>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_response(out: &mut impl Write, resp: &RpcResponse) {
    let _ = writeln!(out, "{}", serde_json::to_string(resp).unwrap_or_default());
    let _ = out.flush();
}

/// Decode base64 data and write to a file.
pub fn save_base64_to_file(path: &str, data_b64: &str) -> io::Result<()> {
    use base64::Engine;
    if data_b64.is_empty() {
        return Ok(());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, bytes)
}

/// Convert an `emu_core::Value` to a `serde_json::Value`.
#[must_use]
pub fn observable_to_json(value: &Value) -> JsonValue {
    match value {
        Value::U8(v) => serde_json::json!(v),
        Value::U16(v) => serde_json::json!(v),
        Value::U32(v) => serde_json::json!(v),
        Value::U64(v) => serde_json::json!(v),
        Value::I8(v) => serde_json::json!(v),
        Value::Bool(v) => serde_json::json!(v),
        Value::String(v) => serde_json::json!(v),
        Value::Array(v) => serde_json::json!(format!("{v:?}")),
        Value::Map(v) => serde_json::json!(format!("{v:?}")),
    }
}

/// Calculate display dimensions for a 4:3 aspect ratio with 2× pre-scale.
///
/// Scales both axes by 2× first, then applies the fractional aspect
/// correction. This spreads nearest-neighbour rounding across twice as
/// many source pixels, avoiding the ugly column/row doubling visible
/// when correcting at native resolution.
#[must_use]
pub fn display_size_4_3(width: u32, height: u32) -> (u32, u32) {
    let sw = width * 2;
    let sh = height * 2;
    let target_w = (sh * 4 + 2) / 3;
    if sw <= target_w {
        (round_even(target_w), round_even(sh))
    } else {
        let target_h = (sw * 3 + 2) / 4;
        (round_even(sw), round_even(target_h))
    }
}

/// Round to the nearest even number (required by H.264 / PNG alignment).
fn round_even(v: u32) -> u32 {
    (v + 1) & !1
}

/// Nearest-neighbour scale a `u32` pixel buffer (allocating).
#[must_use]
pub fn scale_nearest(src: &[u32], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u32> {
    if src_w == dst_w && src_h == dst_h {
        return src.to_vec();
    }
    let mut dst = vec![0u32; (dst_w * dst_h) as usize];
    scale_nearest_into(src, src_w, src_h, &mut dst, dst_w, dst_h);
    dst
}

/// Nearest-neighbour scale into an existing buffer (no allocation).
pub fn scale_nearest_into(
    src: &[u32],
    src_w: u32,
    src_h: u32,
    dst: &mut [u32],
    dst_w: u32,
    dst_h: u32,
) {
    for y in 0..dst_h {
        let src_y = (y * src_h / dst_h).min(src_h - 1);
        for x in 0..dst_w {
            let src_x = (x * src_w / dst_w).min(src_w - 1);
            dst[(y * dst_w + x) as usize] = src[(src_y * src_w + src_x) as usize];
        }
    }
}

/// Encode a framebuffer as PNG. Returns raw PNG bytes.
///
/// `pixels` should be packed 0x00RRGGBB (or 0xAARRGGBB — alpha is ignored).
pub fn encode_png(width: u32, height: u32, pixels: &[u32]) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| format!("PNG header error: {e}"))?;

    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for &pixel in pixels {
        rgba.push(((pixel >> 16) & 0xFF) as u8);
        rgba.push(((pixel >> 8) & 0xFF) as u8);
        rgba.push((pixel & 0xFF) as u8);
        rgba.push(0xFF);
    }

    writer
        .write_image_data(&rgba)
        .map_err(|e| format!("PNG write error: {e}"))?;
    drop(writer);
    Ok(buf)
}

/// Video recording result helper.
///
/// Returns metadata about the recorded MP4 file (path, duration, frame count).
#[cfg(feature = "video")]
#[must_use]
pub fn video_result(save_path: &str, frames: u64, fps: u32) -> ToolResult {
    let size = std::fs::metadata(save_path).map(|m| m.len()).unwrap_or(0);
    let duration = frames as f64 / f64::from(fps);

    ToolResult::Success(serde_json::json!({
        "format": "mp4",
        "codec_video": "h264",
        "codec_audio": "aac",
        "frames": frames,
        "fps": fps,
        "duration": duration,
        "path": save_path,
        "size": size,
    }))
}

/// Screenshot helper: encode framebuffer and either save to disk or return base64.
///
/// If `save_path` is `Some`, writes PNG to that path and returns metadata only.
/// Otherwise returns base64-encoded PNG data in the response.
///
/// When `display_size` is `Some`, the framebuffer is scaled with
/// nearest-neighbour interpolation before encoding.
#[must_use]
pub fn screenshot_result(
    width: u32,
    height: u32,
    pixels: &[u32],
    save_path: Option<&str>,
    display_size: Option<(u32, u32)>,
) -> ToolResult {
    let (enc_w, enc_h, enc_pixels);
    if let Some((dw, dh)) = display_size
        && (dw != width || dh != height)
    {
        enc_pixels = scale_nearest(pixels, width, height, dw, dh);
        enc_w = dw;
        enc_h = dh;
    } else {
        enc_pixels = pixels.to_vec();
        enc_w = width;
        enc_h = height;
    }

    let png_bytes = match encode_png(enc_w, enc_h, &enc_pixels) {
        Ok(b) => b,
        Err(e) => {
            return ToolResult::Error {
                code: -32000,
                message: e,
            };
        }
    };

    if let Some(path) = save_path {
        if let Err(e) = std::fs::write(path, &png_bytes) {
            return ToolResult::Error {
                code: -32000,
                message: format!("Failed to write screenshot: {e}"),
            };
        }
        ToolResult::Success(serde_json::json!({
            "format": "png",
            "width": enc_w,
            "height": enc_h,
            "path": path,
            "size": png_bytes.len(),
        }))
    } else {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        ToolResult::Success(serde_json::json!({
            "format": "png",
            "width": enc_w,
            "height": enc_h,
            "data": b64,
        }))
    }
}
