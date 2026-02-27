//! MCP (Model Context Protocol) server for the Amiga emulator.
//!
//! Exposes the emulator as a JSON-RPC 2.0 server over stdin/stdout.
//! Tools allow AI agents and scripts to boot, control, observe, and
//! capture the emulator programmatically.

#![allow(clippy::cast_possible_truncation)]

use std::io::{self, BufRead, Write};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use emu_core::Observable;

use crate::config::{AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use crate::format_adf::Adf;
use crate::{Amiga, PAL_FRAME_TICKS};

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
}

impl McpServer {
    #[must_use]
    pub fn new() -> Self {
        Self { amiga: None }
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
            "run_frames" => self.handle_run_frames(params, id),
            "step_instruction" => self.handle_step_instruction(id),
            "step_ticks" => self.handle_step_ticks(params, id),
            "screenshot" => self.handle_screenshot(id),
            "audio_capture" => self.handle_audio_capture(params, id),
            "query" => self.handle_query(params, id),
            "query_memory" => self.handle_query_memory(params, id),
            "poke" => self.handle_poke(params, id),
            "set_breakpoint" => self.handle_set_breakpoint(params, id),
            "insert_disk" => self.handle_insert_disk(params, id),
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

    // === Tool handlers ===

    fn handle_boot(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let kickstart = match load_kickstart(params) {
            Ok(data) => data,
            Err(e) => return RpcResponse::error(id, -32000, e),
        };

        let model = match params.get("model").and_then(|v| v.as_str()) {
            Some("a500plus") => AmigaModel::A500Plus,
            _ => AmigaModel::A500,
        };

        let chipset = match params.get("chipset").and_then(|v| v.as_str()) {
            Some("ecs") => AmigaChipset::Ecs,
            _ => AmigaChipset::Ocs,
        };

        let region = match params.get("region").and_then(|v| v.as_str()) {
            Some("ntsc") => AmigaRegion::Ntsc,
            _ => AmigaRegion::Pal,
        };

        let config = AmigaConfig {
            model,
            chipset,
            region,
            kickstart,
        };

        self.amiga = Some(Amiga::new_with_config(config));
        RpcResponse::success(id, serde_json::json!({"status": "ok"}))
    }

    fn handle_reset(&mut self, id: JsonValue) -> RpcResponse {
        match self.require_amiga(&id) {
            Ok(amiga) => {
                // 68000 reset: read SSP from vector 0, PC from vector 4
                let ssp = u32::from(amiga.memory.read_byte(0)) << 24
                    | u32::from(amiga.memory.read_byte(1)) << 16
                    | u32::from(amiga.memory.read_byte(2)) << 8
                    | u32::from(amiga.memory.read_byte(3));
                let pc = u32::from(amiga.memory.read_byte(4)) << 24
                    | u32::from(amiga.memory.read_byte(5)) << 16
                    | u32::from(amiga.memory.read_byte(6)) << 8
                    | u32::from(amiga.memory.read_byte(7));
                amiga.cpu.reset_to(ssp, pc);
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
            .or_else(|| params.get("frames").and_then(|v| v.as_u64()))
            .unwrap_or(1);

        for _ in 0..count {
            amiga.run_frame();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "frames": count,
                "master_clock": amiga.master_clock,
            }),
        )
    }

    fn handle_step_instruction(&mut self, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        // Tick until the CPU returns to idle (instruction boundary).
        // The 68000 enters Idle after completing each instruction's micro-ops.
        let mut ticks = 0u64;
        let max_ticks = 10_000;
        let mut started = false;

        loop {
            amiga.tick();
            ticks += 1;

            if amiga.cpu.is_idle() {
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
                "pc": format!("${:08X}", amiga.cpu.regs.pc),
                "ticks": ticks,
            }),
        )
    }

    fn handle_step_ticks(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(1);

        for _ in 0..count {
            amiga.tick();
        }

        RpcResponse::success(
            id,
            serde_json::json!({
                "pc": format!("${:08X}", amiga.cpu.regs.pc),
            }),
        )
    }

    fn handle_screenshot(&mut self, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let pal = matches!(amiga.region, AmigaRegion::Pal);
        let viewport = amiga
            .denise
            .as_inner()
            .extract_viewport(crate::commodore_denise_ocs::ViewportPreset::Standard, pal, true);

        let width = viewport.width;
        let height = viewport.height;

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
            for &pixel in &viewport.pixels {
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
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let frames = params.get("frames").and_then(|v| v.as_u64()).unwrap_or(50);

        let mut all_audio = Vec::new();
        for _ in 0..frames {
            amiga.run_frame();
            all_audio.extend_from_slice(&amiga.take_audio_buffer());
        }

        // Encode as WAV (stereo interleaved from Paula)
        let wav_spec = hound::WavSpec {
            channels: 2,
            sample_rate: crate::AUDIO_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut wav_buf = Vec::new();
        {
            let cursor = io::Cursor::new(&mut wav_buf);
            let mut writer = match hound::WavWriter::new(cursor, wav_spec) {
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
                "samples": all_audio.len() / 2, // stereo pairs
                "frames": frames,
                "data": b64,
            }),
        )
    }

    fn handle_query(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let path = match params.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return RpcResponse::error(id, -32602, "Missing 'path' parameter".to_string()),
        };

        match amiga.query(path) {
            Some(value) => {
                let json_val = observable_to_json(&value);
                RpcResponse::success(id, serde_json::json!({"path": path, "value": json_val}))
            }
            None => RpcResponse::error(id, -32000, format!("Unknown query path: {path}")),
        }
    }

    fn handle_query_memory(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let address = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0x00FF_FFFF => a as u32,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'address' (0-16777215, 24-bit)".to_string(),
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
            .map(|i| amiga.memory.read_byte(address.wrapping_add(i as u32) & 0x00FF_FFFF))
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

    fn handle_poke(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0x00FF_FFFF => a as u32,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'address' (0-16777215, 24-bit)".to_string(),
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

        amiga.memory.write_byte(addr, value);
        RpcResponse::success(id, serde_json::json!({"address": addr, "value": value}))
    }

    fn handle_set_breakpoint(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0x00FF_FFFF => a as u32,
            _ => {
                return RpcResponse::error(
                    id,
                    -32602,
                    "Missing or invalid 'address' (0-16777215, 24-bit)".to_string(),
                );
            }
        };

        let max_frames = params
            .get("max_frames")
            .and_then(|v| v.as_u64())
            .unwrap_or(10_000);

        let max_ticks = max_frames * PAL_FRAME_TICKS;
        let mut ticks_run = 0u64;
        let mut hit = false;

        while ticks_run < max_ticks {
            amiga.tick();
            ticks_run += 1;

            if amiga.cpu.regs.pc == addr && amiga.cpu.is_idle() {
                hit = true;
                break;
            }
        }

        let frames_run = ticks_run / PAL_FRAME_TICKS;

        if hit {
            RpcResponse::success(
                id,
                serde_json::json!({
                    "hit": true,
                    "pc": format!("${:08X}", addr),
                    "frames_run": frames_run,
                }),
            )
        } else {
            RpcResponse::success(
                id,
                serde_json::json!({
                    "hit": false,
                    "pc": format!("${:08X}", amiga.cpu.regs.pc),
                    "frames_run": frames_run,
                }),
            )
        }
    }

    fn handle_insert_disk(&mut self, params: &JsonValue, id: JsonValue) -> RpcResponse {
        let amiga = match self.require_amiga(&id) {
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

        match Adf::from_bytes(data) {
            Ok(adf) => {
                amiga.insert_disk(adf);
                RpcResponse::success(id, serde_json::json!({"status": "ok"}))
            }
            Err(e) => RpcResponse::error(id, -32000, format!("ADF load failed: {e}")),
        }
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

fn load_kickstart(params: &JsonValue) -> Result<Vec<u8>, String> {
    // Try params first
    if let Some(b64) = params.get("kickstart").and_then(|v| v.as_str()) {
        return base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("Invalid base64 kickstart: {e}"));
    }

    if let Some(path) = params.get("kickstart_path").and_then(|v| v.as_str()) {
        return std::fs::read(path).map_err(|e| format!("Cannot read kickstart: {e}"));
    }

    // Try environment variable
    if let Ok(path) = std::env::var("AMIGA_KS13_ROM") {
        return std::fs::read(&path)
            .map_err(|e| format!("Cannot read kickstart from AMIGA_KS13_ROM ({path}): {e}"));
    }

    // Try roms/ directory
    let roms_dir = find_roms_dir();
    for name in &["kick13.rom", "kick.rom"] {
        let path = roms_dir.join(name);
        if path.exists() {
            return std::fs::read(&path)
                .map_err(|e| format!("Cannot read {}: {e}", path.display()));
        }
    }

    Err("No kickstart ROM found. Provide 'kickstart_path', set AMIGA_KS13_ROM, or place kick13.rom in roms/".to_string())
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

    #[test]
    fn query_without_boot_returns_error() {
        let mut server = McpServer::new();
        let resp = server.dispatch(
            "query",
            &serde_json::json!({"path": "cpu.pc"}),
            JsonValue::from(1),
        );
        assert!(resp.error.is_some());
    }
}
