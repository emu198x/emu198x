//! MCP (Model Context Protocol) server for the C64 emulator.
//!
//! Implements `McpEmulator` to expose the C64 as tool calls over the
//! shared MCP protocol layer. Run with `--mcp` for MCP mode or
//! `--script` for batch mode.

#![allow(clippy::cast_possible_truncation)]

use base64::Engine;
use serde_json::Value as JsonValue;

use emu_core::mcp::{self, McpEmulator, ToolDefinition, ToolResult};
use emu_core::{Cpu, Observable, Tickable};

use crate::C64;
use crate::config::{C64Config, C64Model};
use crate::input::C64Key;

// ---------------------------------------------------------------------------
// Public re-export: the MCP server type for main.rs
// ---------------------------------------------------------------------------

pub type McpServer = mcp::McpServer<C64Mcp>;

// ---------------------------------------------------------------------------
// C64 MCP implementation
// ---------------------------------------------------------------------------

pub struct C64Mcp {
    c64: Option<C64>,
}

impl C64Mcp {
    #[must_use]
    pub fn new() -> Self {
        Self { c64: None }
    }

    fn require_c64(&mut self) -> Result<&mut C64, ToolResult> {
        if let Some(ref mut c64) = self.c64 {
            Ok(c64)
        } else {
            Err(ToolResult::Error {
                code: -32000,
                message: "No C64 instance. Call 'boot' first.".to_string(),
            })
        }
    }
}

impl Default for C64Mcp {
    fn default() -> Self {
        Self::new()
    }
}

impl McpEmulator for C64Mcp {
    fn server_name(&self) -> &'static str {
        "emu-c64"
    }

    fn server_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "boot",
                description: "Boot the Commodore 64 with PAL ROMs",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "reset",
                description: "Reset the CPU",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "load_prg",
                description: "Load a PRG file into C64 memory",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to .prg file" },
                        "data": { "type": "string", "description": "Base64-encoded PRG data" }
                    }
                }),
            },
            ToolDefinition {
                name: "load_bas",
                description: "Tokenise a BASIC V2 source file, convert to PRG, and load it",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to .bas file" },
                        "source": { "type": "string", "description": "BASIC source text (alternative to path)" },
                        "autostart": {
                            "type": "boolean",
                            "description": "Type RUN and execute the program automatically (default: false)"
                        }
                    }
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
                name: "screenshot",
                description: "Capture the current screen as PNG",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "save_path": { "type": "string", "description": "If set, save PNG to this path and return metadata only" },
                        "correct_aspect": { "type": "boolean", "description": "Scale to 4:3 display aspect ratio (default: true)" }
                    }
                }),
            },
            ToolDefinition {
                name: "step_instruction",
                description: "Execute one CPU instruction",
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
                name: "audio_capture",
                description: "Run N frames and capture audio as WAV",
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
                description: "Query an observable value (e.g. cpu.pc, vic.raster)",
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
                        "prefix": { "type": "string", "description": "Optional path prefix filter, e.g. vic. or sid." }
                    }
                }),
            },
            ToolDefinition {
                name: "poke",
                description: "Write a byte to RAM",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": { "type": "integer", "description": "0-65535" },
                        "value": { "type": "integer", "description": "0-255" }
                    },
                    "required": ["address", "value"]
                }),
            },
            ToolDefinition {
                name: "press_key",
                description: "Press a key on the C64 keyboard",
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
                description: "Release a key on the C64 keyboard",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "key": { "type": "string" }
                    },
                    "required": ["key"]
                }),
            },
            ToolDefinition {
                name: "type_text",
                description: "Queue text to be typed into the C64",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "at_frame": { "type": "integer", "description": "Frame at which to start typing" }
                    },
                    "required": ["text"]
                }),
            },
            ToolDefinition {
                name: "set_breakpoint",
                description: "Run until PC reaches an address (or max frames elapsed)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": { "type": "integer" },
                        "max_frames": { "type": "integer", "default": 10000 }
                    },
                    "required": ["address"]
                }),
            },
            ToolDefinition {
                name: "get_screen_text",
                description: "Read the 25x40 text screen as strings",
                input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            },
            ToolDefinition {
                name: "boot_detected",
                description: "Check if the BASIC boot screen is visible",
                input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            },
            ToolDefinition {
                name: "boot_status",
                description: "Get boot detection status plus screen text",
                input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            },
            ToolDefinition {
                name: "query_memory",
                description: "Read a range of bytes from RAM",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": { "type": "integer" },
                        "length": { "type": "integer", "description": "1-65536" }
                    },
                    "required": ["address", "length"]
                }),
            },
            ToolDefinition {
                name: "load_d64",
                description: "Insert a D64 disk image",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "data": { "type": "string", "description": "Base64-encoded D64" }
                    }
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
            "boot" => self.handle_boot(),
            "reset" => self.handle_reset(),
            "load_prg" => self.handle_load_prg(arguments),
            "load_bas" => self.handle_load_bas(arguments),
            "run_frames" => self.handle_run_frames(arguments),
            "step_instruction" => self.handle_step_instruction(),
            "step_ticks" => self.handle_step_ticks(arguments),
            "screenshot" => self.handle_screenshot(arguments),
            "audio_capture" => self.handle_audio_capture(arguments),
            "query" => self.handle_query(arguments),
            "query_paths" => self.handle_query_paths(arguments),
            "boot_detected" => self.handle_boot_detected(),
            "boot_status" => self.handle_boot_status(),
            "poke" => self.handle_poke(arguments),
            "press_key" => self.handle_press_key(arguments),
            "release_key" => self.handle_release_key(arguments),
            "type_text" => self.handle_type_text(arguments),
            "set_breakpoint" => self.handle_set_breakpoint(arguments),
            "get_screen_text" => self.handle_get_screen_text(),
            "query_memory" => self.handle_query_memory(arguments),
            "load_d64" => self.handle_load_d64(arguments),
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

impl C64Mcp {
    fn handle_boot(&mut self) -> ToolResult {
        let config = match load_c64_config() {
            Ok(c) => c,
            Err(e) => {
                return ToolResult::Error {
                    code: -32000,
                    message: e,
                };
            }
        };
        self.c64 = Some(C64::new(&config));
        ToolResult::Success(serde_json::json!({"status": "ok"}))
    }

    fn handle_reset(&mut self) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };
        c64.cpu_mut().reset();
        ToolResult::Success(serde_json::json!({"status": "ok"}))
    }

    fn handle_load_prg(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        match c64.load_prg(&data) {
            Ok(addr) => ToolResult::Success(
                serde_json::json!({"status": "ok", "load_address": format!("${addr:04X}")}),
            ),
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("PRG load failed: {e}"),
            },
        }
    }

    fn handle_load_bas(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        // Read BASIC source from `source` param or `path` file
        let source = if let Some(text) = params.get("source").and_then(|v| v.as_str()) {
            text.to_string()
        } else if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
            match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    return ToolResult::Error {
                        code: -32602,
                        message: format!("Cannot read file: {e}"),
                    };
                }
            }
        } else {
            return ToolResult::Error {
                code: -32602,
                message: "Provide 'source' (BASIC text) or 'path' (file path)".to_string(),
            };
        };

        // Tokenise to PRG format
        let program = match format_c64_bas::tokenise(&source) {
            Ok(p) => p,
            Err(e) => {
                return ToolResult::Error {
                    code: -32000,
                    message: format!("Tokenise failed: {e}"),
                };
            }
        };

        // Load the PRG into memory
        let addr = match c64.load_prg(&program.bytes) {
            Ok(a) => a,
            Err(e) => {
                return ToolResult::Error {
                    code: -32000,
                    message: format!("PRG load failed: {e}"),
                };
            }
        };

        let autostart = params
            .get("autostart")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if autostart {
            // Type RUN + Return and run frames for it to execute
            let start = c64.frame_count();
            let end = c64.input_queue().enqueue_text("RUN\n", start);
            while c64.frame_count() < end + 30 {
                c64.run_frame();
            }
            ToolResult::Success(serde_json::json!({
                "status": "ok",
                "format": "bas",
                "load_address": format!("${addr:04X}"),
                "autostart": true,
                "frames": c64.frame_count(),
            }))
        } else {
            ToolResult::Success(serde_json::json!({
                "status": "ok",
                "format": "bas",
                "load_address": format!("${addr:04X}"),
            }))
        }
    }

    fn handle_run_frames(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let count = params
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .or_else(|| params.get("frames").and_then(serde_json::Value::as_u64))
            .unwrap_or(1);

        let mut total_cycles = 0u64;
        for _ in 0..count {
            total_cycles += c64.run_frame();
        }

        ToolResult::Success(serde_json::json!({
            "frames": count,
            "cycles": total_cycles,
            "frame_count": c64.frame_count(),
        }))
    }

    fn handle_step_instruction(&mut self) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
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

        ToolResult::Success(serde_json::json!({
            "pc": format!("${:04X}", c64.cpu().regs.pc),
            "cycles": cycles,
        }))
    }

    fn handle_step_ticks(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let count = params
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        for _ in 0..count {
            c64.tick();
        }

        ToolResult::Success(serde_json::json!({
            "pc": format!("${:04X}", c64.cpu().regs.pc),
        }))
    }

    fn handle_screenshot(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let save_path = params.get("save_path").and_then(|v| v.as_str());
        let display = parse_display_size(params, c64.framebuffer_width(), c64.framebuffer_height());
        mcp::screenshot_result(
            c64.framebuffer_width(),
            c64.framebuffer_height(),
            c64.framebuffer(),
            save_path,
            display,
        )
    }

    fn handle_audio_capture(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let frames = params
            .get("frames")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(50);

        let mut all_audio: Vec<f32> = Vec::new();
        for _ in 0..frames {
            c64.run_frame();
            all_audio.extend_from_slice(&c64.take_audio_buffer());
        }

        if let Some(save_path) = params.get("save_path").and_then(|v| v.as_str())
            && let Err(e) = crate::capture::save_audio(&all_audio, std::path::Path::new(save_path))
        {
            return ToolResult::Error {
                code: -32000,
                message: format!("Failed to save audio: {e}"),
            };
        }

        let b64 = if all_audio.is_empty() {
            String::new()
        } else {
            let mut wav_buf = Vec::new();
            {
                let cursor = std::io::Cursor::new(&mut wav_buf);
                let spec = hound::WavSpec {
                    channels: 1,
                    sample_rate: 48_000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let mut writer = hound::WavWriter::new(cursor, spec).expect("WAV writer");
                for &s in &all_audio {
                    let clamped = s.clamp(-1.0, 1.0);
                    let scaled = (clamped * f32::from(i16::MAX)) as i16;
                    writer.write_sample(scaled).expect("WAV sample");
                }
                writer.finalize().expect("WAV finalize");
            }
            base64::engine::general_purpose::STANDARD.encode(&wav_buf)
        };

        ToolResult::Success(serde_json::json!({
            "format": "wav",
            "samples": all_audio.len(),
            "frames": frames,
            "data": b64,
        }))
    }

    fn handle_query(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let Some(path) = params.get("path").and_then(|v| v.as_str()) else {
            return ToolResult::Error {
                code: -32602,
                message: "Missing 'path' parameter".to_string(),
            };
        };

        match c64.query(path) {
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
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let prefix = params.get("prefix").and_then(|v| v.as_str());
        let paths: Vec<&str> = c64
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

    fn handle_poke(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(serde_json::Value::as_u64) {
            Some(a) if a <= 0xFFFF => a as u16,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'address' (0-65535)".to_string(),
                };
            }
        };

        let value = match params.get("value").and_then(serde_json::Value::as_u64) {
            Some(v) if v <= 0xFF => v as u8,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'value' (0-255)".to_string(),
                };
            }
        };

        c64.bus_mut().memory.ram_write(addr, value);
        ToolResult::Success(serde_json::json!({"address": addr, "value": value}))
    }

    fn handle_press_key(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let Some(key_name) = params.get("key").and_then(|v| v.as_str()) else {
            return ToolResult::Error {
                code: -32602,
                message: "Missing 'key' parameter".to_string(),
            };
        };

        match parse_key_name(key_name) {
            Some(key) => {
                c64.press_key(key);
                ToolResult::Success(serde_json::json!({"key": key_name, "pressed": true}))
            }
            None => ToolResult::Error {
                code: -32602,
                message: format!("Unknown key: {key_name}"),
            },
        }
    }

    fn handle_release_key(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let Some(key_name) = params.get("key").and_then(|v| v.as_str()) else {
            return ToolResult::Error {
                code: -32602,
                message: "Missing 'key' parameter".to_string(),
            };
        };

        match parse_key_name(key_name) {
            Some(key) => {
                c64.release_key(key);
                ToolResult::Success(serde_json::json!({"key": key_name, "pressed": false}))
            }
            None => ToolResult::Error {
                code: -32602,
                message: format!("Unknown key: {key_name}"),
            },
        }
    }

    fn handle_type_text(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let text = match params.get("text").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing 'text' parameter".to_string(),
                };
            }
        };

        let at_frame = params
            .get("at_frame")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_else(|| c64.frame_count());

        let end_frame = c64.input_queue().enqueue_text(&text, at_frame);
        ToolResult::Success(serde_json::json!({
            "text": text,
            "start_frame": at_frame,
            "end_frame": end_frame,
        }))
    }

    fn handle_set_breakpoint(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(serde_json::Value::as_u64) {
            Some(a) if a <= 0xFFFF => a as u16,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'address' (0-65535)".to_string(),
                };
            }
        };

        let max_frames = params
            .get("max_frames")
            .and_then(serde_json::Value::as_u64)
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

        ToolResult::Success(serde_json::json!({
            "hit": hit,
            "pc": format!("${:04X}", if hit { addr } else { c64.cpu().regs.pc }),
            "frames_run": frames_run,
        }))
    }

    fn handle_get_screen_text(&mut self) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let lines = read_screen_lines(c64);
        ToolResult::Success(serde_json::json!({
            "rows": 25,
            "cols": 40,
            "lines": lines,
        }))
    }

    fn handle_boot_detected(&mut self) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let lines = read_screen_lines(c64);
        let (detected, reason) = detect_boot(&lines);
        ToolResult::Success(serde_json::json!({
            "boot_detected": detected,
            "reason": reason,
        }))
    }

    fn handle_boot_status(&mut self) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let lines = read_screen_lines(c64);
        let (detected, reason) = detect_boot(&lines);
        ToolResult::Success(serde_json::json!({
            "boot_detected": detected,
            "reason": reason,
            "frame_count": c64.frame_count(),
            "rows": 25,
            "cols": 40,
            "lines": lines,
        }))
    }

    fn handle_query_memory(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let address = match params.get("address").and_then(serde_json::Value::as_u64) {
            Some(a) if a <= 0xFFFF => a as u16,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'address' (0-65535)".to_string(),
                };
            }
        };

        let length = match params.get("length").and_then(serde_json::Value::as_u64) {
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
            .map(|i| c64.bus().memory.peek(address.wrapping_add(i as u16)))
            .collect();

        ToolResult::Success(serde_json::json!({
            "address": address,
            "length": length,
            "data": bytes,
        }))
    }

    fn handle_load_d64(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        match c64.load_d64(&data) {
            Ok(()) => ToolResult::Success(serde_json::json!({"status": "ok", "size": data.len()})),
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("D64 load failed: {e}"),
            },
        }
    }

    fn handle_record_video(&mut self, params: &JsonValue) -> ToolResult {
        let c64 = match self.require_c64() {
            Ok(c) => c,
            Err(e) => return e,
        };

        let frames = match params.get("frames").and_then(serde_json::Value::as_u64) {
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

        let display = parse_display_size(params, c64.framebuffer_width(), c64.framebuffer_height());
        let mut rec = match emu_core::video::VideoRecorder::new(
            c64.framebuffer_width(),
            c64.framebuffer_height(),
            50, // PAL
            1,  // mono
            48_000,
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
            c64.run_frame();
            let audio = c64.take_audio_buffer();
            if let Err(e) = rec.add_frame(c64.framebuffer(), &audio) {
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
        0x80 => '@',
        0x81..=0x9A => (b'A' + code - 0x81) as char,
        0xA0 => ' ',
        0xA1..=0xBF => (b'!' + code - 0xA1) as char,
        _ => ' ',
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
    use crate::config::SidModel;

    fn make_c64() -> C64 {
        let mut kernal = vec![0xEA; 8192];
        kernal[0x1FFC] = 0x00;
        kernal[0x1FFD] = 0xE0;

        C64::new(&C64Config {
            model: C64Model::C64Pal,
            sid_model: SidModel::Sid6581,
            kernal_rom: kernal,
            basic_rom: vec![0; 8192],
            char_rom: vec![0; 4096],
            drive_rom: None,
            reu_size: None,
        })
    }

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
    fn unknown_tool_returns_error() {
        let mut mcp = C64Mcp::new();
        let result = mcp.dispatch_tool("nonexistent", &JsonValue::Null);
        assert!(matches!(result, ToolResult::Error { code: -32601, .. }));
    }

    #[test]
    fn run_frames_without_boot_returns_error() {
        let mut mcp = C64Mcp::new();
        let result = mcp.dispatch_tool("run_frames", &serde_json::json!({"count": 1}));
        assert!(matches!(result, ToolResult::Error { .. }));
    }

    #[test]
    fn query_paths_without_boot_returns_error() {
        let mut mcp = C64Mcp::new();
        let result = mcp.dispatch_tool("query_paths", &serde_json::json!({}));
        assert!(matches!(result, ToolResult::Error { .. }));
    }

    #[test]
    fn query_paths_can_filter_to_vic_and_sid_surfaces() {
        let mut mcp = C64Mcp {
            c64: Some(make_c64()),
        };

        let vic_result = mcp.dispatch_tool(
            "query_paths",
            &serde_json::json!({
                "prefix": "vic."
            }),
        );
        match vic_result {
            ToolResult::Success(value) => {
                let paths = value
                    .get("paths")
                    .and_then(|v| v.as_array())
                    .expect("paths array");
                assert!(paths.iter().any(|v| v.as_str() == Some("vic.line")));
                assert!(paths.iter().any(|v| v.as_str() == Some("vic.cycle")));
                assert!(!paths.iter().any(|v| v.as_str() == Some("sid.volume")));
            }
            ToolResult::Error { message, .. } => panic!("unexpected error: {message}"),
        }

        let sid_result = mcp.dispatch_tool(
            "query_paths",
            &serde_json::json!({
                "prefix": "sid."
            }),
        );
        match sid_result {
            ToolResult::Success(value) => {
                let paths = value
                    .get("paths")
                    .and_then(|v| v.as_array())
                    .expect("paths array");
                assert!(paths.iter().any(|v| v.as_str() == Some("sid.volume")));
                assert!(paths.iter().any(|v| v.as_str() == Some("sid.filter.mode")));
                assert!(!paths.iter().any(|v| v.as_str() == Some("vic.line")));
            }
            ToolResult::Error { message, .. } => panic!("unexpected error: {message}"),
        }
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
