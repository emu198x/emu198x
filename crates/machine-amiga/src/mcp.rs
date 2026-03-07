//! MCP (Model Context Protocol) server for the Amiga emulator.
//!
//! Implements `McpEmulator` to expose the Amiga as tool calls over the
//! shared MCP protocol layer. Run with `--mcp` for MCP mode or
//! `--script` for batch mode.

#![allow(clippy::cast_possible_truncation)]

use base64::Engine;
use serde_json::Value as JsonValue;

use emu_core::Observable;
use emu_core::mcp::{self, McpEmulator, ToolDefinition, ToolResult};

use crate::config::{AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use crate::format_adf::Adf;
use crate::{Amiga, PAL_FRAME_TICKS};

// ---------------------------------------------------------------------------
// Public re-export: the MCP server type for main.rs
// ---------------------------------------------------------------------------

pub type McpServer = mcp::McpServer<AmigaMcp>;

// ---------------------------------------------------------------------------
// Amiga MCP implementation
// ---------------------------------------------------------------------------

pub struct AmigaMcp {
    amiga: Option<Amiga>,
}

impl AmigaMcp {
    #[must_use]
    pub fn new() -> Self {
        Self { amiga: None }
    }

    fn require_amiga(&mut self) -> Result<&mut Amiga, ToolResult> {
        if let Some(ref mut amiga) = self.amiga {
            Ok(amiga)
        } else {
            Err(ToolResult::Error {
                code: -32000,
                message: "No Amiga instance. Call 'boot' first.".to_string(),
            })
        }
    }
}

impl Default for AmigaMcp {
    fn default() -> Self {
        Self::new()
    }
}

impl McpEmulator for AmigaMcp {
    fn server_name(&self) -> &str {
        "emu-amiga"
    }

    fn server_version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "boot",
                description: "Boot the Amiga with a Kickstart ROM",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "kickstart": { "type": "string", "description": "Base64-encoded Kickstart ROM" },
                        "kickstart_path": { "type": "string", "description": "Path to Kickstart ROM file" },
                        "model": { "type": "string", "description": "a1000, a500 (default), a500plus, a600, a1200, a2000, a3000, or a4000" },
                        "region": { "type": "string", "description": "pal (default) or ntsc" },
                        "slow_ram": { "type": "integer", "description": "Slow RAM in KB (default 0)" }
                    }
                }),
            },
            ToolDefinition {
                name: "reset",
                description: "Reset the CPU (read SSP/PC from vectors 0/4)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "run_frames",
                description: "Run the emulator for N frames (50fps PAL)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "description": "Number of frames to run", "default": 1 }
                    }
                }),
            },
            ToolDefinition {
                name: "step_instruction",
                description: "Execute one CPU instruction (tick until idle)",
                input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            },
            ToolDefinition {
                name: "step_ticks",
                description: "Advance the master clock by N ticks",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "default": 1 }
                    }
                }),
            },
            ToolDefinition {
                name: "screenshot",
                description: "Capture the current screen as PNG (viewport extraction)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "save_path": { "type": "string", "description": "If set, save PNG to this path and return metadata only" },
                        "correct_aspect": { "type": "boolean", "description": "Scale to 4:3 display aspect ratio (default: true)" }
                    }
                }),
            },
            ToolDefinition {
                name: "audio_capture",
                description: "Run N frames and capture stereo audio as WAV",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "frames": { "type": "integer", "default": 50 },
                        "save_path": { "type": "string", "description": "Save WAV to this path" }
                    }
                }),
            },
            ToolDefinition {
                name: "query",
                description: "Query an observable value (e.g. cpu.pc, agnus.beamcon0, denise.palette.0)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Dot-separated query path" }
                    },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "query_paths",
                description: "List available observable query paths, optionally filtered by prefix",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prefix": { "type": "string", "description": "Optional path prefix filter, e.g. agnus. or denise.mode." }
                    }
                }),
            },
            ToolDefinition {
                name: "query_memory",
                description: "Read a range of bytes from memory (24-bit address space)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": { "type": "integer", "description": "0-16777215 (24-bit)" },
                        "length": { "type": "integer", "description": "1-65536" }
                    },
                    "required": ["address", "length"]
                }),
            },
            ToolDefinition {
                name: "poke",
                description: "Write a byte to memory",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": { "type": "integer", "description": "0-16777215 (24-bit)" },
                        "value": { "type": "integer", "description": "0-255" }
                    },
                    "required": ["address", "value"]
                }),
            },
            ToolDefinition {
                name: "set_breakpoint",
                description: "Run until PC reaches an address (or max frames elapsed)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": { "type": "integer", "description": "24-bit address" },
                        "max_frames": { "type": "integer", "default": 10000 }
                    },
                    "required": ["address"]
                }),
            },
            ToolDefinition {
                name: "insert_disk",
                description: "Insert an ADF or IPF disk image into DF0:",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to ADF/IPF file" },
                        "data": { "type": "string", "description": "Base64-encoded disk image" }
                    }
                }),
            },
            ToolDefinition {
                name: "press_key",
                description: "Press a key on the Amiga keyboard",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "key": { "type": "string", "description": "Key name (a-z, 0-9, return, space, f1, etc.)" }
                    },
                    "required": ["key"]
                }),
            },
            ToolDefinition {
                name: "release_key",
                description: "Release a key on the Amiga keyboard",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "key": { "type": "string" }
                    },
                    "required": ["key"]
                }),
            },
            ToolDefinition {
                name: "record_video",
                description: "Record N frames as MP4 video with audio",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "frames": { "type": "integer", "description": "Number of frames to record" },
                        "save_path": { "type": "string", "description": "Write MP4 to this path" },
                        "correct_aspect": { "type": "boolean", "description": "Scale to 4:3 display aspect ratio (default: true)" }
                    },
                    "required": ["frames", "save_path"]
                }),
            },
        ]
    }

    fn dispatch_tool(&mut self, name: &str, arguments: &JsonValue) -> ToolResult {
        match name {
            "boot" => self.handle_boot(arguments),
            "reset" => self.handle_reset(),
            "run_frames" => self.handle_run_frames(arguments),
            "step_instruction" => self.handle_step_instruction(),
            "step_ticks" => self.handle_step_ticks(arguments),
            "screenshot" => self.handle_screenshot(arguments),
            "audio_capture" => self.handle_audio_capture(arguments),
            "query" => self.handle_query(arguments),
            "query_paths" => self.handle_query_paths(arguments),
            "query_memory" => self.handle_query_memory(arguments),
            "poke" => self.handle_poke(arguments),
            "set_breakpoint" => self.handle_set_breakpoint(arguments),
            "insert_disk" => self.handle_insert_disk(arguments),
            "press_key" => self.handle_press_key(arguments),
            "release_key" => self.handle_release_key(arguments),
            "record_video" => self.handle_record_video(arguments),
            _ => ToolResult::Error {
                code: -32601,
                message: format!("Unknown tool: {name}"),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Tool handlers
// ---------------------------------------------------------------------------

impl AmigaMcp {
    fn handle_boot(&mut self, params: &JsonValue) -> ToolResult {
        if params.get("chipset").is_some() {
            return ToolResult::Error {
                code: -32602,
                message: "chipset is derived from model; omit 'chipset'".to_string(),
            };
        }

        let model = match params.get("model").and_then(|v| v.as_str()) {
            Some(value) => match parse_model_arg(value) {
                Ok(model) => model,
                Err(message) => {
                    return ToolResult::Error {
                        code: -32602,
                        message,
                    };
                }
            },
            None => AmigaModel::A500,
        };

        let kickstart = match load_kickstart(params) {
            Ok(data) => data,
            Err(e) => {
                return ToolResult::Error {
                    code: -32000,
                    message: e,
                };
            }
        };

        let chipset = chipset_for_model(model);

        let region = match params.get("region").and_then(|v| v.as_str()) {
            Some("ntsc") => AmigaRegion::Ntsc,
            _ => AmigaRegion::Pal,
        };

        let slow_ram_size = params
            .get("slow_ram")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize * 1024)
            .unwrap_or(0);

        let config = AmigaConfig {
            model,
            chipset,
            region,
            kickstart,
            slow_ram_size,
        };

        self.amiga = Some(Amiga::new_with_config(config));
        ToolResult::Success(serde_json::json!({"status": "ok"}))
    }

    fn handle_reset(&mut self) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

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
        ToolResult::Success(serde_json::json!({"status": "ok"}))
    }

    fn handle_run_frames(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
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

        ToolResult::Success(serde_json::json!({
            "frames": count,
            "master_clock": amiga.master_clock,
        }))
    }

    fn handle_step_instruction(&mut self) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
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

        ToolResult::Success(serde_json::json!({
            "pc": format!("${:08X}", amiga.cpu.regs.pc),
            "ticks": ticks,
        }))
    }

    fn handle_step_ticks(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(1);
        for _ in 0..count {
            amiga.tick();
        }

        ToolResult::Success(serde_json::json!({
            "pc": format!("${:08X}", amiga.cpu.regs.pc),
        }))
    }

    fn handle_screenshot(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let pal = matches!(amiga.region, AmigaRegion::Pal);
        let viewport = amiga.denise.as_inner().extract_viewport(
            crate::commodore_denise_ocs::ViewportPreset::Standard,
            pal,
            true,
        );

        let save_path = params.get("save_path").and_then(|v| v.as_str());
        let display = parse_display_size(params, viewport.width, viewport.height);
        mcp::screenshot_result(
            viewport.width,
            viewport.height,
            &viewport.pixels,
            save_path,
            display,
        )
    }

    fn handle_audio_capture(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let frames = params.get("frames").and_then(|v| v.as_u64()).unwrap_or(50);

        let mut all_audio: Vec<f32> = Vec::new();
        for _ in 0..frames {
            amiga.run_frame();
            all_audio.extend_from_slice(&amiga.take_audio_buffer());
        }

        if let Some(save_path) = params.get("save_path").and_then(|v| v.as_str()) {
            // Encode as WAV and save directly
            let wav_bytes = match encode_wav_stereo(&all_audio) {
                Ok(b) => b,
                Err(e) => {
                    return ToolResult::Error {
                        code: -32000,
                        message: format!("WAV encode error: {e}"),
                    };
                }
            };
            if let Err(e) = std::fs::write(save_path, &wav_bytes) {
                return ToolResult::Error {
                    code: -32000,
                    message: format!("Failed to save audio: {e}"),
                };
            }
        }

        let b64 = if all_audio.is_empty() {
            String::new()
        } else {
            let wav_bytes = match encode_wav_stereo(&all_audio) {
                Ok(b) => b,
                Err(e) => {
                    return ToolResult::Error {
                        code: -32000,
                        message: format!("WAV encode error: {e}"),
                    };
                }
            };
            base64::engine::general_purpose::STANDARD.encode(&wav_bytes)
        };

        ToolResult::Success(serde_json::json!({
            "format": "wav",
            "samples": all_audio.len() / 2, // stereo pairs
            "frames": frames,
            "data": b64,
        }))
    }

    fn handle_query(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let path = match params.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing 'path' parameter".to_string(),
                };
            }
        };

        match amiga.query(path) {
            Some(value) => {
                let json_val = mcp::observable_to_json(&value);
                ToolResult::Success(serde_json::json!({"path": path, "value": json_val}))
            }
            None => ToolResult::Error {
                code: -32000,
                message: format!("Unknown query path: {path}"),
            },
        }
    }

    fn handle_query_paths(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let prefix = params.get("prefix").and_then(|v| v.as_str());
        let paths: Vec<&str> = amiga
            .query_paths()
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .collect();

        ToolResult::Success(serde_json::json!({
            "prefix": prefix,
            "paths": paths,
        }))
    }

    fn handle_query_memory(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let address = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0x00FF_FFFF => a as u32,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'address' (0-16777215, 24-bit)".to_string(),
                };
            }
        };

        let length = match params.get("length").and_then(|v| v.as_u64()) {
            Some(l) if (1..=65536).contains(&l) => l as usize,
            Some(_) => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Invalid 'length' (1-65536)".to_string(),
                };
            }
            None => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing 'length' parameter".to_string(),
                };
            }
        };

        let bytes: Vec<u8> = (0..length)
            .map(|i| {
                amiga
                    .memory
                    .read_byte(address.wrapping_add(i as u32) & 0x00FF_FFFF)
            })
            .collect();

        ToolResult::Success(serde_json::json!({
            "address": address,
            "length": length,
            "data": bytes,
        }))
    }

    fn handle_poke(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0x00FF_FFFF => a as u32,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'address' (0-16777215, 24-bit)".to_string(),
                };
            }
        };

        let value = match params.get("value").and_then(|v| v.as_u64()) {
            Some(v) if v <= 0xFF => v as u8,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'value' (0-255)".to_string(),
                };
            }
        };

        amiga.memory.write_byte(addr, value);
        ToolResult::Success(serde_json::json!({"address": addr, "value": value}))
    }

    fn handle_set_breakpoint(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0x00FF_FFFF => a as u32,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'address' (0-16777215, 24-bit)".to_string(),
                };
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

        ToolResult::Success(serde_json::json!({
            "hit": hit,
            "pc": format!("${:08X}", if hit { addr } else { amiga.cpu.regs.pc }),
            "frames_run": frames_run,
        }))
    }

    fn handle_insert_disk(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        // Auto-detect format by magic bytes.
        if format_ipf::IpfImage::is_ipf(&data) {
            match format_ipf::IpfImage::from_bytes(&data) {
                Ok(ipf) => {
                    amiga.insert_disk_image(Box::new(ipf));
                    ToolResult::Success(serde_json::json!({"status": "ok", "format": "ipf"}))
                }
                Err(e) => ToolResult::Error {
                    code: -32000,
                    message: format!("IPF load failed: {e}"),
                },
            }
        } else {
            match Adf::from_bytes(data) {
                Ok(adf) => {
                    amiga.insert_disk(adf);
                    ToolResult::Success(serde_json::json!({"status": "ok", "format": "adf"}))
                }
                Err(e) => ToolResult::Error {
                    code: -32000,
                    message: format!("ADF load failed: {e}"),
                },
            }
        }
    }

    fn handle_press_key(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let key_name = match params.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing 'key' parameter".to_string(),
                };
            }
        };

        match parse_key_name(key_name) {
            Some(keycode) => {
                amiga.key_event(keycode, true);
                ToolResult::Success(serde_json::json!({"key": key_name, "pressed": true}))
            }
            None => ToolResult::Error {
                code: -32602,
                message: format!("Unknown key: {key_name}"),
            },
        }
    }

    fn handle_release_key(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let key_name = match params.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing 'key' parameter".to_string(),
                };
            }
        };

        match parse_key_name(key_name) {
            Some(keycode) => {
                amiga.key_event(keycode, false);
                ToolResult::Success(serde_json::json!({"key": key_name, "pressed": false}))
            }
            None => ToolResult::Error {
                code: -32602,
                message: format!("Unknown key: {key_name}"),
            },
        }
    }

    fn handle_record_video(&mut self, params: &JsonValue) -> ToolResult {
        let amiga = match self.require_amiga() {
            Ok(a) => a,
            Err(e) => return e,
        };

        let frames = match params.get("frames").and_then(|v| v.as_u64()) {
            Some(f) if f > 0 => f,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'frames' (positive integer)".to_string(),
                };
            }
        };

        let Some(save_path) = params.get("save_path").and_then(|v| v.as_str()) else {
            return ToolResult::Error {
                code: -32602,
                message: "Missing 'save_path' parameter".to_string(),
            };
        };

        let pal = matches!(amiga.region, AmigaRegion::Pal);
        let fps = if pal { 50 } else { 60 };

        // Get initial viewport dimensions for recorder setup.
        let viewport = amiga.denise.as_inner().extract_viewport(
            crate::commodore_denise_ocs::ViewportPreset::Standard,
            pal,
            true,
        );

        let display = parse_display_size(params, viewport.width, viewport.height);
        let mut rec = match emu_core::video::VideoRecorder::new(
            viewport.width,
            viewport.height,
            fps,
            2, // stereo
            crate::AUDIO_SAMPLE_RATE,
            std::path::Path::new(save_path),
            display,
        ) {
            Ok(r) => r,
            Err(e) => {
                return ToolResult::Error {
                    code: -32000,
                    message: format!("Video recorder init: {e}"),
                };
            }
        };

        for _ in 0..frames {
            amiga.run_frame();
            let viewport = amiga.denise.as_inner().extract_viewport(
                crate::commodore_denise_ocs::ViewportPreset::Standard,
                pal,
                true,
            );
            let audio = amiga.take_audio_buffer();
            if let Err(e) = rec.add_frame(&viewport.pixels, &audio) {
                return ToolResult::Error {
                    code: -32000,
                    message: format!("Video recording failed: {e}"),
                };
            }
        }

        match rec.finish() {
            Ok(info) => mcp::video_result(save_path, info.frames, info.fps),
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("Video finish failed: {e}"),
            },
        }
    }
}

fn parse_model_arg(value: &str) -> Result<AmigaModel, String> {
    match value.to_ascii_lowercase().as_str() {
        "a1000" => Ok(AmigaModel::A1000),
        "a500" => Ok(AmigaModel::A500),
        "a500+" | "a500plus" => Ok(AmigaModel::A500Plus),
        "a600" => Ok(AmigaModel::A600),
        "a1200" => Ok(AmigaModel::A1200),
        "a2000" => Ok(AmigaModel::A2000),
        "a3000" => Ok(AmigaModel::A3000),
        "a4000" => Ok(AmigaModel::A4000),
        other => Err(format!(
            "Unknown model: {other}. Use a1000, a500, a500plus, a600, a1200, a2000, a3000, or a4000."
        )),
    }
}

const fn chipset_for_model(model: AmigaModel) -> AmigaChipset {
    match model {
        AmigaModel::A1000 | AmigaModel::A500 | AmigaModel::A2000 => AmigaChipset::Ocs,
        AmigaModel::A500Plus | AmigaModel::A600 | AmigaModel::A3000 => AmigaChipset::Ecs,
        AmigaModel::A1200 | AmigaModel::A4000 => AmigaChipset::Aga,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read `correct_aspect` (default true) and compute display size.
fn parse_display_size(params: &JsonValue, w: u32, h: u32) -> Option<(u32, u32)> {
    let correct = params
        .get("correct_aspect")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if correct {
        Some(mcp::display_size_4_3(w, h))
    } else {
        None
    }
}

/// Load binary data from a `data` (base64) or `path` parameter.
fn load_binary_param(params: &JsonValue) -> Result<Vec<u8>, ToolResult> {
    if let Some(b64) = params.get("data").and_then(|v| v.as_str()) {
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| ToolResult::Error {
                code: -32602,
                message: format!("Invalid base64: {e}"),
            })
    } else if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
        std::fs::read(path).map_err(|e| ToolResult::Error {
            code: -32602,
            message: format!("Cannot read file: {e}"),
        })
    } else {
        Err(ToolResult::Error {
            code: -32602,
            message: "Provide 'data' (base64) or 'path'".to_string(),
        })
    }
}

/// Map a key name string to an Amiga raw keycode.
fn parse_key_name(name: &str) -> Option<u8> {
    match name.to_lowercase().as_str() {
        // Letters
        "a" => Some(0x20),
        "b" => Some(0x35),
        "c" => Some(0x33),
        "d" => Some(0x22),
        "e" => Some(0x12),
        "f" => Some(0x23),
        "g" => Some(0x24),
        "h" => Some(0x25),
        "i" => Some(0x17),
        "j" => Some(0x26),
        "k" => Some(0x27),
        "l" => Some(0x28),
        "m" => Some(0x37),
        "n" => Some(0x36),
        "o" => Some(0x18),
        "p" => Some(0x19),
        "q" => Some(0x10),
        "r" => Some(0x13),
        "s" => Some(0x21),
        "t" => Some(0x14),
        "u" => Some(0x16),
        "v" => Some(0x34),
        "w" => Some(0x11),
        "x" => Some(0x32),
        "y" => Some(0x15),
        "z" => Some(0x31),
        // Number row
        "1" => Some(0x01),
        "2" => Some(0x02),
        "3" => Some(0x03),
        "4" => Some(0x04),
        "5" => Some(0x05),
        "6" => Some(0x06),
        "7" => Some(0x07),
        "8" => Some(0x08),
        "9" => Some(0x09),
        "0" => Some(0x0A),
        // Special keys
        "space" => Some(0x40),
        "return" | "enter" => Some(0x44),
        "backspace" => Some(0x41),
        "tab" => Some(0x42),
        "escape" | "esc" => Some(0x45),
        "delete" | "del" => Some(0x46),
        // Cursor keys
        "up" | "cursor_up" => Some(0x4C),
        "down" | "cursor_down" => Some(0x4D),
        "right" | "cursor_right" => Some(0x4E),
        "left" | "cursor_left" => Some(0x4F),
        // Modifiers
        "lshift" | "left_shift" => Some(0x60),
        "rshift" | "right_shift" => Some(0x61),
        "capslock" | "caps_lock" => Some(0x62),
        "ctrl" | "control" => Some(0x63),
        "lalt" | "left_alt" => Some(0x64),
        "ralt" | "right_alt" => Some(0x65),
        "lamiga" | "left_amiga" => Some(0x66),
        "ramiga" | "right_amiga" => Some(0x67),
        // Function keys
        "f1" => Some(0x50),
        "f2" => Some(0x51),
        "f3" => Some(0x52),
        "f4" => Some(0x53),
        "f5" => Some(0x54),
        "f6" => Some(0x55),
        "f7" => Some(0x56),
        "f8" => Some(0x57),
        "f9" => Some(0x58),
        "f10" => Some(0x59),
        // Punctuation
        "minus" | "-" => Some(0x0B),
        "equals" | "=" => Some(0x0C),
        "backslash" | "\\" => Some(0x0D),
        "semicolon" | ";" => Some(0x29),
        "quote" | "'" => Some(0x2A),
        "comma" | "," => Some(0x38),
        "period" | "." => Some(0x39),
        "slash" | "/" => Some(0x3A),
        "leftbracket" | "[" => Some(0x1A),
        "rightbracket" | "]" => Some(0x1B),
        "backquote" | "`" => Some(0x00),
        _ => None,
    }
}

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

/// Encode stereo audio samples as WAV bytes.
fn encode_wav_stereo(samples: &[f32]) -> Result<Vec<u8>, String> {
    let mut wav_buf = Vec::new();
    let cursor = std::io::Cursor::new(&mut wav_buf);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: crate::AUDIO_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::new(cursor, spec).map_err(|e| format!("WAV encode error: {e}"))?;
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let scaled = (clamped * f32::from(i16::MAX)) as i16;
        writer
            .write_sample(scaled)
            .map_err(|e| format!("WAV write error: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("WAV finalize error: {e}"))?;
    Ok(wav_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_tool_returns_error() {
        let mut mcp = AmigaMcp::new();
        let result = mcp.dispatch_tool("nonexistent", &JsonValue::Null);
        assert!(matches!(result, ToolResult::Error { code: -32601, .. }));
    }

    #[test]
    fn run_frames_without_boot_returns_error() {
        let mut mcp = AmigaMcp::new();
        let result = mcp.dispatch_tool("run_frames", &serde_json::json!({"count": 1}));
        assert!(matches!(result, ToolResult::Error { .. }));
    }

    #[test]
    fn query_without_boot_returns_error() {
        let mut mcp = AmigaMcp::new();
        let result = mcp.dispatch_tool("query", &serde_json::json!({"path": "cpu.pc"}));
        assert!(matches!(result, ToolResult::Error { .. }));
    }

    #[test]
    fn query_paths_without_boot_returns_error() {
        let mut mcp = AmigaMcp::new();
        let result = mcp.dispatch_tool("query_paths", &serde_json::json!({}));
        assert!(matches!(result, ToolResult::Error { .. }));
    }

    #[test]
    fn query_paths_can_filter_to_agnus_and_denise_surfaces() {
        let mut mcp = AmigaMcp {
            amiga: Some(Amiga::new(vec![0; 256 * 1024])),
        };

        let agnus_result = mcp.dispatch_tool(
            "query_paths",
            &serde_json::json!({
                "prefix": "agnus.mode."
            }),
        );
        match agnus_result {
            ToolResult::Success(value) => {
                let paths = value
                    .get("paths")
                    .and_then(|v| v.as_array())
                    .expect("paths array");
                assert!(
                    paths
                        .iter()
                        .any(|v| v.as_str() == Some("agnus.mode.varbeamen"))
                );
                assert!(
                    paths
                        .iter()
                        .any(|v| v.as_str() == Some("agnus.mode.harddis"))
                );
                assert!(
                    !paths
                        .iter()
                        .any(|v| v.as_str() == Some("denise.mode.shres"))
                );
            }
            ToolResult::Error { message, .. } => panic!("unexpected error: {message}"),
        }

        let denise_result = mcp.dispatch_tool(
            "query_paths",
            &serde_json::json!({
                "prefix": "denise.mode."
            }),
        );
        match denise_result {
            ToolResult::Success(value) => {
                let paths = value
                    .get("paths")
                    .and_then(|v| v.as_array())
                    .expect("paths array");
                assert!(
                    paths
                        .iter()
                        .any(|v| v.as_str() == Some("denise.mode.shres"))
                );
                assert!(
                    paths
                        .iter()
                        .any(|v| v.as_str() == Some("denise.mode.killehb"))
                );
                assert!(
                    !paths
                        .iter()
                        .any(|v| v.as_str() == Some("agnus.mode.varbeamen"))
                );
            }
            ToolResult::Error { message, .. } => panic!("unexpected error: {message}"),
        }
    }

    #[test]
    fn query_returns_new_agnus_ecs_observable_fields() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: vec![0; 256 * 1024],
            slow_ram_size: 0,
        });
        amiga.write_custom_reg(0x1DC, 0xAB3E);

        let mut mcp = AmigaMcp { amiga: Some(amiga) };
        let result = mcp.dispatch_tool(
            "query",
            &serde_json::json!({
                "path": "agnus.beamcon0"
            }),
        );

        match result {
            ToolResult::Success(value) => {
                assert_eq!(
                    value.get("path").and_then(|v| v.as_str()),
                    Some("agnus.beamcon0")
                );
                assert_eq!(value.get("value"), Some(&serde_json::json!(0xAB3E)));
            }
            ToolResult::Error { message, .. } => panic!("unexpected error: {message}"),
        }
    }

    #[test]
    fn query_paths_can_filter_to_cpu_surface() {
        let amiga = Amiga::new(vec![0; 256 * 1024]);
        let mut mcp = AmigaMcp { amiga: Some(amiga) };
        let result = mcp.dispatch_tool(
            "query_paths",
            &serde_json::json!({
                "prefix": "cpu."
            }),
        );

        match result {
            ToolResult::Success(value) => {
                let paths = value
                    .get("paths")
                    .and_then(|v| v.as_array())
                    .expect("paths array");
                assert!(paths.iter().any(|v| v.as_str() == Some("cpu.pc")));
                assert!(paths.iter().any(|v| v.as_str() == Some("cpu.flags.z")));
                assert!(
                    !paths
                        .iter()
                        .any(|v| v.as_str() == Some("cpu.<68000_paths>"))
                );
                assert!(!paths.iter().any(|v| v.as_str() == Some("agnus.vpos")));
            }
            ToolResult::Error { message, .. } => panic!("unexpected error: {message}"),
        }
    }

    #[test]
    fn parse_model_arg_accepts_supported_models_and_aliases() {
        assert_eq!(parse_model_arg("a1000"), Ok(AmigaModel::A1000));
        assert_eq!(parse_model_arg("a500"), Ok(AmigaModel::A500));
        assert_eq!(parse_model_arg("a500+"), Ok(AmigaModel::A500Plus));
        assert_eq!(parse_model_arg("a500plus"), Ok(AmigaModel::A500Plus));
        assert_eq!(parse_model_arg("a600"), Ok(AmigaModel::A600));
        assert_eq!(parse_model_arg("a1200"), Ok(AmigaModel::A1200));
        assert_eq!(parse_model_arg("a2000"), Ok(AmigaModel::A2000));
        assert_eq!(parse_model_arg("a3000"), Ok(AmigaModel::A3000));
        assert_eq!(parse_model_arg("a4000"), Ok(AmigaModel::A4000));
        assert!(parse_model_arg("cd32").is_err());
    }

    #[test]
    fn chipset_for_model_matches_machine_presets() {
        assert_eq!(chipset_for_model(AmigaModel::A1000), AmigaChipset::Ocs);
        assert_eq!(chipset_for_model(AmigaModel::A500), AmigaChipset::Ocs);
        assert_eq!(chipset_for_model(AmigaModel::A2000), AmigaChipset::Ocs);
        assert_eq!(chipset_for_model(AmigaModel::A500Plus), AmigaChipset::Ecs);
        assert_eq!(chipset_for_model(AmigaModel::A600), AmigaChipset::Ecs);
        assert_eq!(chipset_for_model(AmigaModel::A3000), AmigaChipset::Ecs);
        assert_eq!(chipset_for_model(AmigaModel::A1200), AmigaChipset::Aga);
        assert_eq!(chipset_for_model(AmigaModel::A4000), AmigaChipset::Aga);
    }

    #[test]
    fn boot_rejects_explicit_chipset_override() {
        let mut mcp = AmigaMcp::new();
        let result = mcp.dispatch_tool(
            "boot",
            &serde_json::json!({
                "model": "a1200",
                "chipset": "aga"
            }),
        );

        assert!(matches!(
            result,
            ToolResult::Error { code: -32602, message }
                if message.contains("chipset is derived from model")
        ));
    }

    #[test]
    fn parse_key_names() {
        assert_eq!(parse_key_name("a"), Some(0x20));
        assert_eq!(parse_key_name("A"), Some(0x20));
        assert_eq!(parse_key_name("return"), Some(0x44));
        assert_eq!(parse_key_name("enter"), Some(0x44));
        assert_eq!(parse_key_name("space"), Some(0x40));
        assert_eq!(parse_key_name("f1"), Some(0x50));
        assert_eq!(parse_key_name("lshift"), Some(0x60));
        assert_eq!(parse_key_name("lamiga"), Some(0x66));
        assert_eq!(parse_key_name("unknown"), None);
    }
}
