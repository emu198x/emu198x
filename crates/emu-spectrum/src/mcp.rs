//! MCP (Model Context Protocol) server for the ZX Spectrum emulator.
//!
//! Implements `McpEmulator` to expose the Spectrum as tool calls over the
//! shared MCP protocol layer. Run with `--mcp` for MCP mode or
//! `--script` for batch mode.

#![allow(clippy::cast_possible_truncation)]

use base64::Engine;
use serde_json::Value as JsonValue;

use emu_core::mcp::{self, McpEmulator, ToolDefinition, ToolResult};
use emu_core::{Cpu, Observable, Tickable};

use crate::Spectrum;
use crate::config::{SpectrumConfig, SpectrumModel};
use crate::input::SpectrumKey;
use crate::sna::load_sna;
use crate::tap::TapFile;
use crate::tzx::TzxFile;
use crate::z80::load_z80;

/// Embedded 48K ROM.
const ROM_48K: &[u8] = include_bytes!("../../../roms/48.rom");

// ---------------------------------------------------------------------------
// Public re-export: the MCP server type for main.rs
// ---------------------------------------------------------------------------

pub type McpServer = mcp::McpServer<SpectrumMcp>;

// ---------------------------------------------------------------------------
// Spectrum MCP implementation
// ---------------------------------------------------------------------------

pub struct SpectrumMcp {
    spectrum: Option<Spectrum>,
}

impl SpectrumMcp {
    #[must_use]
    pub fn new() -> Self {
        Self { spectrum: None }
    }

    fn require_spectrum(&mut self) -> Result<&mut Spectrum, ToolResult> {
        if let Some(ref mut spectrum) = self.spectrum {
            Ok(spectrum)
        } else {
            Err(ToolResult::Error {
                code: -32000,
                message: "No Spectrum instance. Call 'boot' first.".to_string(),
            })
        }
    }
}

impl Default for SpectrumMcp {
    fn default() -> Self {
        Self::new()
    }
}

impl McpEmulator for SpectrumMcp {
    fn server_name(&self) -> &'static str {
        "emu-spectrum"
    }

    fn server_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "boot",
                description: "Boot the ZX Spectrum with the specified model",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "model": { "type": "string", "description": "Spectrum model: 48k, 128k, plus2, plus2a, plus3 (default: 48k)" },
                        "rom": { "type": "string", "description": "Base64-encoded ROM data (required for non-48K models)" },
                        "rom_path": { "type": "string", "description": "Path to ROM file (required for non-48K models)" }
                    }
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
                name: "load_sna",
                description: "Load a SNA snapshot file",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to .sna file" },
                        "data": { "type": "string", "description": "Base64-encoded SNA data" }
                    }
                }),
            },
            ToolDefinition {
                name: "load_z80",
                description: "Load a .Z80 snapshot file (v1/v2/v3)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to .z80 file" },
                        "data": { "type": "string", "description": "Base64-encoded Z80 data" }
                    }
                }),
            },
            ToolDefinition {
                name: "load_tap",
                description: "Insert a TAP file into the tape deck",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to .tap file" },
                        "data": { "type": "string", "description": "Base64-encoded TAP data" }
                    }
                }),
            },
            ToolDefinition {
                name: "load_tzx",
                description: "Insert a TZX file (real-time tape signal)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to .tzx file" },
                        "data": { "type": "string", "description": "Base64-encoded TZX data" }
                    }
                }),
            },
            ToolDefinition {
                name: "load_dsk",
                description: "Insert a DSK disk image (+3 only)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to .dsk file" },
                        "data": { "type": "string", "description": "Base64-encoded DSK data" }
                    }
                }),
            },
            ToolDefinition {
                name: "tape_status",
                description: "Query the current tape deck status",
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
                description: "Advance by N T-states (each = 4 master clock ticks)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "default": 1 }
                    }
                }),
            },
            ToolDefinition {
                name: "audio_capture",
                description: "Run N frames and capture audio as WAV (stereo)",
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
                description: "Query an observable value (e.g. cpu.pc, ula.border_colour)",
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
                        "prefix": { "type": "string", "description": "Optional path prefix filter, e.g. ula. or cpu." }
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
                description: "Press a key on the Spectrum keyboard",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "key": { "type": "string", "description": "Key name (a-z, 0-9, enter, space, caps_shift, sym_shift, kempston_*, etc.)" }
                    },
                    "required": ["key"]
                }),
            },
            ToolDefinition {
                name: "release_key",
                description: "Release a key on the Spectrum keyboard",
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
                description: "Queue text to be typed into the Spectrum",
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
                description: "Read the 24x32 text screen by matching against the ROM character set",
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
            "load_sna" => self.handle_load_sna(arguments),
            "load_z80" => self.handle_load_z80(arguments),
            "load_tap" => self.handle_load_tap(arguments),
            "load_tzx" => self.handle_load_tzx(arguments),
            "load_dsk" => self.handle_load_dsk(arguments),
            "tape_status" => self.handle_tape_status(),
            "run_frames" => self.handle_run_frames(arguments),
            "step_instruction" => self.handle_step_instruction(),
            "step_ticks" => self.handle_step_ticks(arguments),
            "screenshot" => self.handle_screenshot(arguments),
            "audio_capture" => self.handle_audio_capture(arguments),
            "query" => self.handle_query(arguments),
            "query_paths" => self.handle_query_paths(arguments),
            "poke" => self.handle_poke(arguments),
            "press_key" => self.handle_press_key(arguments),
            "release_key" => self.handle_release_key(arguments),
            "type_text" => self.handle_type_text(arguments),
            "set_breakpoint" => self.handle_set_breakpoint(arguments),
            "get_screen_text" => self.handle_get_screen_text(),
            "query_memory" => self.handle_query_memory(arguments),
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

impl SpectrumMcp {
    fn handle_boot(&mut self, params: &JsonValue) -> ToolResult {
        let model_str = params
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("48k");

        let (model, model_label) = match model_str.to_lowercase().as_str() {
            "48k" | "48" => (SpectrumModel::Spectrum48K, "48k"),
            "128k" | "128" => (SpectrumModel::Spectrum128K, "128k"),
            "plus2" | "+2" => (SpectrumModel::SpectrumPlus2, "plus2"),
            "plus2a" | "+2a" => (SpectrumModel::SpectrumPlus2A, "plus2a"),
            "plus3" | "+3" => (SpectrumModel::SpectrumPlus3, "plus3"),
            other => {
                return ToolResult::Error {
                    code: -32602,
                    message: format!(
                        "Unknown model: {other}. Use 48k, 128k, plus2, plus2a, or plus3."
                    ),
                };
            }
        };

        let rom = if model == SpectrumModel::Spectrum48K {
            ROM_48K.to_vec()
        } else if let Some(b64) = params.get("rom").and_then(|v| v.as_str()) {
            match base64::engine::general_purpose::STANDARD.decode(b64) {
                Ok(d) => d,
                Err(e) => {
                    return ToolResult::Error {
                        code: -32602,
                        message: format!("Invalid base64 ROM: {e}"),
                    };
                }
            }
        } else if let Some(path) = params.get("rom_path").and_then(|v| v.as_str()) {
            match std::fs::read(path) {
                Ok(d) => d,
                Err(e) => {
                    return ToolResult::Error {
                        code: -32602,
                        message: format!("Cannot read ROM file: {e}"),
                    };
                }
            }
        } else {
            return ToolResult::Error {
                code: -32602,
                message: format!(
                    "{model_label} model requires 'rom' (base64) or 'rom_path' parameter"
                ),
            };
        };

        let config = SpectrumConfig { model, rom };
        self.spectrum = Some(Spectrum::new(&config));
        ToolResult::Success(serde_json::json!({"status": "ok", "model": model_label}))
    }

    fn handle_reset(&mut self) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };
        spec.cpu_mut().reset();
        ToolResult::Success(serde_json::json!({"status": "ok"}))
    }

    fn handle_load_sna(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        match load_sna(spec, &data) {
            Ok(()) => ToolResult::Success(serde_json::json!({"status": "ok"})),
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("SNA load failed: {e}"),
            },
        }
    }

    fn handle_load_z80(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        match load_z80(spec, &data) {
            Ok(()) => ToolResult::Success(serde_json::json!({"status": "ok"})),
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("Z80 load failed: {e}"),
            },
        }
    }

    fn handle_load_tap(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        match TapFile::parse(&data) {
            Ok(tap) => {
                let blocks = tap.blocks.len();
                spec.insert_tap(tap);
                ToolResult::Success(serde_json::json!({"status": "ok", "blocks": blocks}))
            }
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("TAP parse failed: {e}"),
            },
        }
    }

    fn handle_load_tzx(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        match TzxFile::parse(&data) {
            Ok(tzx) => {
                let blocks = tzx.blocks.len();
                spec.insert_tzx(tzx);
                ToolResult::Success(
                    serde_json::json!({"status": "ok", "blocks": blocks, "format": "tzx"}),
                )
            }
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("TZX parse failed: {e}"),
            },
        }
    }

    fn handle_load_dsk(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        match spec.load_dsk(&data) {
            Ok(()) => ToolResult::Success(serde_json::json!({"status": "ok", "format": "dsk"})),
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("DSK load failed: {e}"),
            },
        }
    }

    fn handle_tape_status(&mut self) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let tap_loaded = spec.tape().is_loaded();
        let tap_block = spec.tape().block_index();
        let tap_blocks = spec.tape().block_count();
        let tzx_playing = spec.is_tzx_playing();

        ToolResult::Success(serde_json::json!({
            "tap": {
                "loaded": tap_loaded,
                "block_index": tap_block,
                "block_count": tap_blocks,
            },
            "tzx": {
                "playing": tzx_playing,
            },
        }))
    }

    fn handle_run_frames(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);

        let mut total_tstates = 0u64;
        for _ in 0..count {
            total_tstates += spec.run_frame();
        }

        ToolResult::Success(serde_json::json!({
            "frames": count,
            "tstates": total_tstates,
            "frame_count": spec.frame_count(),
        }))
    }

    fn handle_step_instruction(&mut self) -> ToolResult {
        let spec = match self.require_spectrum() {
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

        ToolResult::Success(serde_json::json!({
            "pc": format!("${:04X}", spec.cpu().regs.pc),
            "tstates": tstates,
        }))
    }

    fn handle_step_ticks(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let count = params
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);

        // Each CPU T-state = 4 master clock ticks
        for _ in 0..(count * 4) {
            spec.tick();
        }

        ToolResult::Success(serde_json::json!({
            "pc": format!("${:04X}", spec.cpu().regs.pc),
        }))
    }

    fn handle_screenshot(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let save_path = params.get("save_path").and_then(|v| v.as_str());
        let display =
            parse_display_size(params, spec.framebuffer_width(), spec.framebuffer_height());
        mcp::screenshot_result(
            spec.framebuffer_width(),
            spec.framebuffer_height(),
            spec.framebuffer(),
            save_path,
            display,
        )
    }

    fn handle_audio_capture(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let frames = params
            .get("frames")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(50);

        let mut all_audio: Vec<[f32; 2]> = Vec::new();
        for _ in 0..frames {
            spec.run_frame();
            all_audio.extend_from_slice(&spec.take_audio_buffer());
        }

        if let Some(save_path) = params.get("save_path").and_then(|v| v.as_str())
            && let Err(e) = crate::capture::save_audio(&all_audio, std::path::Path::new(save_path))
        {
            return ToolResult::Error {
                code: -32000,
                message: format!("Failed to save audio: {e}"),
            };
        }

        // Encode as WAV in memory (stereo)
        let b64 = if all_audio.is_empty() {
            String::new()
        } else {
            let mut wav_buf = Vec::new();
            {
                let cursor = std::io::Cursor::new(&mut wav_buf);
                let spec_wav = hound::WavSpec {
                    channels: 2,
                    sample_rate: 48_000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let mut writer = hound::WavWriter::new(cursor, spec_wav).expect("WAV writer");
                for &[left, right] in &all_audio {
                    let l = (left.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                    let r = (right.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                    writer.write_sample(l).expect("WAV sample");
                    writer.write_sample(r).expect("WAV sample");
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
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let Some(path) = params.get("path").and_then(|v| v.as_str()) else {
            return ToolResult::Error {
                code: -32602,
                message: "Missing 'path' parameter".to_string(),
            };
        };

        match spec.query(path) {
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
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let prefix = params.get("prefix").and_then(|v| v.as_str());
        let paths: Vec<&str> = spec
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
        let spec = match self.require_spectrum() {
            Ok(s) => s,
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

        spec.bus_mut().memory.write(addr, value);
        ToolResult::Success(serde_json::json!({"address": addr, "value": value}))
    }

    fn handle_press_key(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
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
                spec.press_key(key);
                ToolResult::Success(serde_json::json!({"key": key_name, "pressed": true}))
            }
            None => ToolResult::Error {
                code: -32602,
                message: format!("Unknown key: {key_name}"),
            },
        }
    }

    fn handle_release_key(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
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
                spec.release_key(key);
                ToolResult::Success(serde_json::json!({"key": key_name, "pressed": false}))
            }
            None => ToolResult::Error {
                code: -32602,
                message: format!("Unknown key: {key_name}"),
            },
        }
    }

    fn handle_type_text(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
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
            .unwrap_or_else(|| spec.frame_count());

        let end_frame = spec.input_queue().enqueue_text(&text, at_frame);
        ToolResult::Success(serde_json::json!({
            "text": text,
            "start_frame": at_frame,
            "end_frame": end_frame,
        }))
    }

    fn handle_set_breakpoint(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
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

        ToolResult::Success(serde_json::json!({
            "hit": hit,
            "pc": format!("${:04X}", if hit { addr } else { spec.cpu().regs.pc }),
            "frames_run": frames_run,
        }))
    }

    fn handle_get_screen_text(&mut self) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let mut lines = Vec::new();
        for row in 0..24u8 {
            let mut line = String::with_capacity(32);
            for col in 0..32u8 {
                let ch = read_screen_char(spec, row, col);
                line.push(ch);
            }
            lines.push(line);
        }

        ToolResult::Success(serde_json::json!({
            "rows": 24,
            "cols": 32,
            "lines": lines,
        }))
    }

    fn handle_query_memory(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
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
            .map(|i| spec.bus().memory.peek(address.wrapping_add(i as u16)))
            .collect();

        ToolResult::Success(serde_json::json!({
            "address": address,
            "length": length,
            "data": bytes,
        }))
    }

    fn handle_record_video(&mut self, params: &JsonValue) -> ToolResult {
        let spec = match self.require_spectrum() {
            Ok(s) => s,
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

        let display =
            parse_display_size(params, spec.framebuffer_width(), spec.framebuffer_height());
        let mut rec = match emu_core::video::VideoRecorder::new(
            spec.framebuffer_width(),
            spec.framebuffer_height(),
            50, // PAL
            2,  // stereo
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
            spec.run_frame();
            // Spectrum returns stereo as Vec<[f32; 2]> — flatten to interleaved.
            let stereo = spec.take_audio_buffer();
            let interleaved: Vec<f32> = stereo.iter().flat_map(|s| [s[0], s[1]]).collect();
            if let Err(e) = rec.add_frame(spec.framebuffer(), &interleaved) {
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
        // Kempston joystick
        "kempston_right" | "joy_right" => Some(SpectrumKey::KempstonRight),
        "kempston_left" | "joy_left" => Some(SpectrumKey::KempstonLeft),
        "kempston_down" | "joy_down" => Some(SpectrumKey::KempstonDown),
        "kempston_up" | "joy_up" => Some(SpectrumKey::KempstonUp),
        "kempston_fire" | "joy_fire" => Some(SpectrumKey::KempstonFire),
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

        for (py, &cell_byte) in cell.iter().enumerate() {
            let rom_byte = mem.peek(rom_addr + py as u16);
            if rom_byte == cell_byte {
                matching_bits += 8;
            } else {
                all_match = false;
                // Count matching bits
                let diff = rom_byte ^ cell_byte;
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_spectrum() -> Spectrum {
        let mut rom = vec![0u8; 0x4000];
        rom[0] = 0xF3;
        rom[1] = 0x76;
        Spectrum::new(&SpectrumConfig {
            model: SpectrumModel::Spectrum48K,
            rom,
        })
    }

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
        let mut mcp = SpectrumMcp::new();
        assert!(mcp.spectrum.is_none());

        let result = mcp.dispatch_tool("boot", &JsonValue::Null);
        assert!(matches!(result, ToolResult::Success(_)));
        assert!(mcp.spectrum.is_some());
    }

    #[test]
    fn run_frames_without_boot_returns_error() {
        let mut mcp = SpectrumMcp::new();
        let result = mcp.dispatch_tool("run_frames", &serde_json::json!({"count": 1}));
        assert!(matches!(result, ToolResult::Error { .. }));
    }

    #[test]
    fn query_paths_without_boot_returns_error() {
        let mut mcp = SpectrumMcp::new();
        let result = mcp.dispatch_tool("query_paths", &serde_json::json!({}));
        assert!(matches!(result, ToolResult::Error { .. }));
    }

    #[test]
    fn query_paths_can_filter_to_ula_and_cpu_surfaces() {
        let mut mcp = SpectrumMcp {
            spectrum: Some(make_spectrum()),
        };

        let ula_result = mcp.dispatch_tool(
            "query_paths",
            &serde_json::json!({
                "prefix": "ula."
            }),
        );
        match ula_result {
            ToolResult::Success(value) => {
                let paths = value
                    .get("paths")
                    .and_then(|v| v.as_array())
                    .expect("paths array");
                assert!(paths.iter().any(|v| v.as_str() == Some("ula.line")));
                assert!(paths.iter().any(|v| v.as_str() == Some("ula.tstate")));
                assert!(!paths.iter().any(|v| v.as_str() == Some("cpu.<z80_paths>")));
            }
            ToolResult::Error { message, .. } => panic!("unexpected error: {message}"),
        }

        let cpu_result = mcp.dispatch_tool(
            "query_paths",
            &serde_json::json!({
                "prefix": "cpu."
            }),
        );
        match cpu_result {
            ToolResult::Success(value) => {
                let paths = value
                    .get("paths")
                    .and_then(|v| v.as_array())
                    .expect("paths array");
                assert!(paths.iter().any(|v| v.as_str() == Some("cpu.<z80_paths>")));
                assert!(!paths.iter().any(|v| v.as_str() == Some("ula.line")));
            }
            ToolResult::Error { message, .. } => panic!("unexpected error: {message}"),
        }
    }

    #[test]
    fn boot_and_run_frames() {
        let mut mcp = SpectrumMcp::new();
        mcp.dispatch_tool("boot", &JsonValue::Null);

        let result = mcp.dispatch_tool("run_frames", &serde_json::json!({"count": 10}));
        match result {
            ToolResult::Success(val) => {
                assert_eq!(val["frames"], 10);
            }
            ToolResult::Error { message, .. } => panic!("Expected success, got error: {message}"),
        }
    }

    #[test]
    fn screenshot_returns_base64_png() {
        let mut mcp = SpectrumMcp::new();
        mcp.dispatch_tool("boot", &JsonValue::Null);
        mcp.dispatch_tool("run_frames", &serde_json::json!({"count": 5}));

        let result = mcp.dispatch_tool("screenshot", &JsonValue::Null);
        match result {
            ToolResult::Success(val) => {
                assert_eq!(val["format"], "png");
                // 320×288 native → 768×576 with 2× pre-scale + 4:3 correction
                assert_eq!(val["width"], 768);
                assert_eq!(val["height"], 576);
                assert!(val["data"].as_str().unwrap().len() > 100);
            }
            ToolResult::Error { message, .. } => panic!("Expected success, got error: {message}"),
        }
    }

    #[test]
    fn query_cpu_pc() {
        let mut mcp = SpectrumMcp::new();
        mcp.dispatch_tool("boot", &JsonValue::Null);

        let result = mcp.dispatch_tool("query", &serde_json::json!({"path": "cpu.pc"}));
        match result {
            ToolResult::Success(val) => {
                assert_eq!(val["path"], "cpu.pc");
            }
            ToolResult::Error { message, .. } => panic!("Expected success, got error: {message}"),
        }
    }

    #[test]
    fn poke_memory() {
        let mut mcp = SpectrumMcp::new();
        mcp.dispatch_tool("boot", &JsonValue::Null);

        let result = mcp.dispatch_tool(
            "poke",
            &serde_json::json!({"address": 0x8000, "value": 0xAB}),
        );
        assert!(matches!(result, ToolResult::Success(_)));

        // Verify with query
        let result = mcp.dispatch_tool("query", &serde_json::json!({"path": "memory.0x8000"}));
        match result {
            ToolResult::Success(val) => {
                assert_eq!(val["value"], 0xAB);
            }
            ToolResult::Error { message, .. } => panic!("Expected success, got error: {message}"),
        }
    }

    #[test]
    fn press_and_release_key() {
        let mut mcp = SpectrumMcp::new();
        mcp.dispatch_tool("boot", &JsonValue::Null);

        let result = mcp.dispatch_tool("press_key", &serde_json::json!({"key": "a"}));
        assert!(matches!(result, ToolResult::Success(_)));

        let result = mcp.dispatch_tool("release_key", &serde_json::json!({"key": "a"}));
        assert!(matches!(result, ToolResult::Success(_)));
    }

    #[test]
    fn unknown_tool_returns_error() {
        let mut mcp = SpectrumMcp::new();
        let result = mcp.dispatch_tool("nonexistent", &JsonValue::Null);
        assert!(matches!(result, ToolResult::Error { code: -32601, .. }));
    }

    #[test]
    fn get_screen_text_after_boot() {
        let mut mcp = SpectrumMcp::new();
        mcp.dispatch_tool("boot", &JsonValue::Null);
        mcp.dispatch_tool("run_frames", &serde_json::json!({"count": 200}));

        let result = mcp.dispatch_tool("get_screen_text", &JsonValue::Null);
        match result {
            ToolResult::Success(val) => {
                assert_eq!(val["rows"], 24);
                assert_eq!(val["cols"], 32);
                let lines: Vec<String> = val["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                let all_text = lines.join("\n");
                assert!(
                    all_text.contains("1982"),
                    "Screen should show copyright year: {all_text}"
                );
            }
            ToolResult::Error { message, .. } => panic!("Expected success, got error: {message}"),
        }
    }

    #[test]
    fn boot_128k_with_rom() {
        let mut mcp = SpectrumMcp::new();
        // Create a minimal 32K ROM (all zeros is fine for construction).
        let rom = vec![0u8; 0x8000];
        let rom_b64 = base64::engine::general_purpose::STANDARD.encode(&rom);
        let params = serde_json::json!({"model": "128k", "rom": rom_b64});
        let result = mcp.dispatch_tool("boot", &params);
        assert!(
            matches!(result, ToolResult::Success(_)),
            "Expected success for 128K boot with ROM"
        );
        assert!(mcp.spectrum.is_some());
    }

    #[test]
    fn boot_128k_missing_rom_errors() {
        let mut mcp = SpectrumMcp::new();
        let params = serde_json::json!({"model": "128k"});
        let result = mcp.dispatch_tool("boot", &params);
        assert!(
            matches!(result, ToolResult::Error { .. }),
            "128K boot without ROM should fail"
        );
        assert!(mcp.spectrum.is_none());
    }

    #[test]
    fn boot_48k_default_no_params() {
        let mut mcp = SpectrumMcp::new();
        let result = mcp.dispatch_tool("boot", &JsonValue::Null);
        match result {
            ToolResult::Success(val) => {
                assert_eq!(val["model"], "48k");
            }
            ToolResult::Error { message, .. } => panic!("Expected success, got error: {message}"),
        }
    }
}
