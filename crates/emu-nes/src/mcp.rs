//! MCP (Model Context Protocol) server for the NES emulator.
//!
//! Implements `McpEmulator` to expose the NES as tool calls over the
//! shared MCP protocol layer. Run with `--mcp` for MCP mode or
//! `--script` for batch mode.

#![allow(
    clippy::cast_possible_truncation,
    clippy::redundant_closure_for_method_calls
)]
#![allow(clippy::too_many_lines, clippy::match_same_arms)]

use std::path::PathBuf;

use base64::Engine;
use serde_json::Value as JsonValue;

use emu_core::mcp::{self, McpEmulator, ToolDefinition, ToolResult};
use emu_core::{Cpu, Observable, Tickable};

use crate::Nes;
use crate::config::{NesConfig, NesRegion};
use crate::input::NesButton;

// ---------------------------------------------------------------------------
// Public re-export: the MCP server type for main.rs
// ---------------------------------------------------------------------------

pub type McpServer = mcp::McpServer<NesMcp>;

// ---------------------------------------------------------------------------
// NES MCP implementation
// ---------------------------------------------------------------------------

pub struct NesMcp {
    nes: Option<Nes>,
    rom_path: Option<PathBuf>,
}

impl NesMcp {
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

    fn require_nes(&mut self) -> Result<&mut Nes, ToolResult> {
        if let Some(ref mut nes) = self.nes {
            Ok(nes)
        } else {
            Err(ToolResult::Error {
                code: -32000,
                message: "No NES instance. Call 'boot' first.".to_string(),
            })
        }
    }
}

impl Default for NesMcp {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_region(params: &JsonValue) -> NesRegion {
    match params.get("region").and_then(|v| v.as_str()) {
        Some("pal") => NesRegion::Pal,
        _ => NesRegion::Ntsc,
    }
}

impl McpEmulator for NesMcp {
    fn server_name(&self) -> &'static str {
        "emu-nes"
    }

    fn server_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "boot",
                description: "Boot the NES with a ROM (from data, path, or CLI --rom)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to .nes ROM file" },
                        "data": { "type": "string", "description": "Base64-encoded iNES ROM data" },
                        "region": { "type": "string", "description": "ntsc or pal (default: ntsc)" }
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
                name: "load_rom",
                description: "Load a new ROM into the NES (replaces current cartridge)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to .nes ROM file" },
                        "data": { "type": "string", "description": "Base64-encoded iNES ROM data" },
                        "region": { "type": "string", "description": "ntsc or pal (default: ntsc)" }
                    }
                }),
            },
            ToolDefinition {
                name: "run_frames",
                description: "Run the emulator for N frames (60fps NTSC / 50fps PAL)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "description": "Number of frames to run", "default": 1 }
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
                name: "query",
                description: "Query an observable value (e.g. cpu.pc, ppu.scanline)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Dot-separated query path" }
                    },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "poke",
                description: "Write a byte to RAM (0-2047)",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": { "type": "integer", "description": "0-2047 (NES RAM)" },
                        "value": { "type": "integer", "description": "0-255" }
                    },
                    "required": ["address", "value"]
                }),
            },
            ToolDefinition {
                name: "press_button",
                description: "Press a button on the NES controller",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "button": { "type": "string", "description": "Button name (a, b, select, start, up, down, left, right)" },
                        "player": { "type": "integer", "description": "Player number (1 or 2, default: 1)" }
                    },
                    "required": ["button"]
                }),
            },
            ToolDefinition {
                name: "release_button",
                description: "Release a button on the NES controller",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "button": { "type": "string", "description": "Button name" },
                        "player": { "type": "integer", "description": "Player number (1 or 2, default: 1)" }
                    },
                    "required": ["button"]
                }),
            },
            ToolDefinition {
                name: "input_sequence",
                description: "Queue a sequence of button presses with hold/gap timing",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "sequence": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Array of button names to press in order"
                        },
                        "hold_frames": { "type": "integer", "description": "Frames to hold each button (default: 3)" },
                        "gap_frames": { "type": "integer", "description": "Frames between presses (default: 3)" },
                        "at_frame": { "type": "integer", "description": "Frame to start at (default: current)" }
                    },
                    "required": ["sequence"]
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
                name: "enable_zapper",
                description: "Enable the Zapper light gun on port 2",
                input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            },
            ToolDefinition {
                name: "zapper_aim",
                description: "Set the Zapper aim coordinates",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "x": { "type": "integer", "description": "X coordinate (default: 128)" },
                        "y": { "type": "integer", "description": "Y coordinate (default: 120)" }
                    }
                }),
            },
            ToolDefinition {
                name: "zapper_trigger",
                description: "Pull or release the Zapper trigger",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pulled": { "type": "boolean", "description": "true to pull, false to release (default: true)" }
                    }
                }),
            },
            ToolDefinition {
                name: "save_battery",
                description: "Read battery-backed PRG RAM as base64",
                input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            },
            ToolDefinition {
                name: "load_battery",
                description: "Restore battery-backed PRG RAM from data or file",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to battery save file" },
                        "data": { "type": "string", "description": "Base64-encoded battery save data" }
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
            "boot" => self.handle_boot(arguments),
            "reset" => self.handle_reset(),
            "load_rom" => self.handle_load_rom(arguments),
            "run_frames" => self.handle_run_frames(arguments),
            "step_instruction" => self.handle_step_instruction(),
            "step_ticks" => self.handle_step_ticks(arguments),
            "screenshot" => self.handle_screenshot(arguments),
            "query" => self.handle_query(arguments),
            "poke" => self.handle_poke(arguments),
            "press_button" => self.handle_press_button(arguments),
            "release_button" => self.handle_release_button(arguments),
            "input_sequence" => self.handle_input_sequence(arguments),
            "set_breakpoint" => self.handle_set_breakpoint(arguments),
            "query_memory" => self.handle_query_memory(arguments),
            "enable_zapper" => self.handle_enable_zapper(),
            "zapper_aim" => self.handle_zapper_aim(arguments),
            "zapper_trigger" => self.handle_zapper_trigger(arguments),
            "save_battery" => self.handle_save_battery(),
            "load_battery" => self.handle_load_battery(arguments),
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

impl NesMcp {
    fn handle_boot(&mut self, params: &JsonValue) -> ToolResult {
        let rom_data = if let Some(b64) = params.get("data").and_then(|v| v.as_str()) {
            match base64::engine::general_purpose::STANDARD.decode(b64) {
                Ok(d) => d,
                Err(e) => {
                    return ToolResult::Error {
                        code: -32602,
                        message: format!("Invalid base64: {e}"),
                    };
                }
            }
        } else if let Some(path) = params
            .get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.rom_path.clone())
        {
            match std::fs::read(&path) {
                Ok(d) => d,
                Err(e) => {
                    return ToolResult::Error {
                        code: -32000,
                        message: format!("Cannot read ROM: {e}"),
                    };
                }
            }
        } else {
            return ToolResult::Error {
                code: -32602,
                message: "Provide 'data' (base64), 'path', or --rom CLI argument".to_string(),
            };
        };

        let config = NesConfig {
            rom_data,
            region: parse_region(params),
        };
        match Nes::new(&config) {
            Ok(nes) => {
                self.nes = Some(nes);
                ToolResult::Success(serde_json::json!({"status": "ok"}))
            }
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("Boot failed: {e}"),
            },
        }
    }

    fn handle_reset(&mut self) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };
        nes.cpu_mut().reset();
        ToolResult::Success(serde_json::json!({"status": "ok"}))
    }

    fn handle_load_rom(&mut self, params: &JsonValue) -> ToolResult {
        let rom_data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        let config = NesConfig {
            rom_data,
            region: parse_region(params),
        };
        match Nes::new(&config) {
            Ok(nes) => {
                self.nes = Some(nes);
                ToolResult::Success(serde_json::json!({"status": "ok"}))
            }
            Err(e) => ToolResult::Error {
                code: -32000,
                message: format!("ROM load failed: {e}"),
            },
        }
    }

    fn handle_run_frames(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
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

        ToolResult::Success(serde_json::json!({
            "frames": count,
            "ticks": total_ticks,
            "frame_count": nes.frame_count(),
        }))
    }

    fn handle_step_instruction(&mut self) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
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

        ToolResult::Success(serde_json::json!({
            "pc": format!("${:04X}", nes.cpu().regs.pc),
            "ticks": ticks,
        }))
    }

    fn handle_step_ticks(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(1);
        for _ in 0..count {
            nes.tick();
        }

        ToolResult::Success(serde_json::json!({
            "pc": format!("${:04X}", nes.cpu().regs.pc),
        }))
    }

    fn handle_screenshot(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let save_path = params.get("save_path").and_then(|v| v.as_str());
        let display = parse_display_size(params, nes.framebuffer_width(), nes.framebuffer_height());
        mcp::screenshot_result(
            nes.framebuffer_width(),
            nes.framebuffer_height(),
            nes.framebuffer(),
            save_path,
            display,
        )
    }

    fn handle_query(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let Some(path) = params.get("path").and_then(|v| v.as_str()) else {
            return ToolResult::Error {
                code: -32602,
                message: "Missing 'path' parameter".to_string(),
            };
        };

        match nes.query(path) {
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

    fn handle_poke(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0x07FF => a as u16,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'address' (0-2047, RAM only)".to_string(),
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

        nes.bus_mut().ram[addr as usize] = value;
        ToolResult::Success(serde_json::json!({"address": addr, "value": value}))
    }

    fn handle_press_button(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let Some(name) = params.get("button").and_then(|v| v.as_str()) else {
            return ToolResult::Error {
                code: -32602,
                message: "Missing 'button' parameter".to_string(),
            };
        };

        let player = params.get("player").and_then(|v| v.as_u64()).unwrap_or(1);

        match parse_button_name(name) {
            Some(button) => {
                if player == 2 {
                    nes.press_button_p2(button);
                } else {
                    nes.press_button(button);
                }
                ToolResult::Success(
                    serde_json::json!({"button": name, "player": player, "pressed": true}),
                )
            }
            None => ToolResult::Error {
                code: -32602,
                message: format!("Unknown button: {name}"),
            },
        }
    }

    fn handle_release_button(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let Some(name) = params.get("button").and_then(|v| v.as_str()) else {
            return ToolResult::Error {
                code: -32602,
                message: "Missing 'button' parameter".to_string(),
            };
        };

        let player = params.get("player").and_then(|v| v.as_u64()).unwrap_or(1);

        match parse_button_name(name) {
            Some(button) => {
                if player == 2 {
                    nes.release_button_p2(button);
                } else {
                    nes.release_button(button);
                }
                ToolResult::Success(
                    serde_json::json!({"button": name, "player": player, "pressed": false}),
                )
            }
            None => ToolResult::Error {
                code: -32602,
                message: format!("Unknown button: {name}"),
            },
        }
    }

    fn handle_input_sequence(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let Some(sequence) = params.get("sequence").and_then(|v| v.as_array()) else {
            return ToolResult::Error {
                code: -32602,
                message: "Missing 'sequence' array parameter".to_string(),
            };
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
            let Some(name) = item.as_str() else {
                continue;
            };
            if let Some(button) = parse_button_name(name) {
                nes.input_queue().enqueue_button(button, frame, hold_frames);
                frame += hold_frames + gap_frames;
                count += 1;
            }
        }

        ToolResult::Success(serde_json::json!({
            "buttons_queued": count,
            "start_frame": start_frame,
            "end_frame": frame,
        }))
    }

    fn handle_set_breakpoint(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let addr = match params.get("address").and_then(|v| v.as_u64()) {
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

        ToolResult::Success(serde_json::json!({
            "hit": hit,
            "pc": format!("${:04X}", if hit { addr } else { nes.cpu().regs.pc }),
            "frames_run": frames_run,
        }))
    }

    fn handle_query_memory(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let address = match params.get("address").and_then(|v| v.as_u64()) {
            Some(a) if a <= 0xFFFF => a as u16,
            _ => {
                return ToolResult::Error {
                    code: -32602,
                    message: "Missing or invalid 'address' (0-65535)".to_string(),
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
            .map(|i| nes.bus().peek_ram(address.wrapping_add(i as u16)))
            .collect();

        ToolResult::Success(serde_json::json!({
            "address": address,
            "length": length,
            "data": bytes,
        }))
    }

    fn handle_enable_zapper(&mut self) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };
        nes.enable_zapper();
        ToolResult::Success(serde_json::json!({"status": "ok"}))
    }

    fn handle_zapper_aim(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };
        let x = params.get("x").and_then(|v| v.as_u64()).unwrap_or(128) as u16;
        let y = params.get("y").and_then(|v| v.as_u64()).unwrap_or(120) as u16;
        nes.set_zapper_aim(x, y);
        ToolResult::Success(serde_json::json!({"x": x, "y": y}))
    }

    fn handle_zapper_trigger(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };
        let pulled = params
            .get("pulled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        nes.set_zapper_trigger(pulled);
        ToolResult::Success(serde_json::json!({"trigger": pulled}))
    }

    fn handle_save_battery(&mut self) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        match nes.save_battery() {
            Some(data) => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(data);
                ToolResult::Success(serde_json::json!({
                    "size": data.len(),
                    "data": b64,
                }))
            }
            None => ToolResult::Error {
                code: -32000,
                message: "No battery save: cartridge has no battery flag or mapper has no PRG RAM"
                    .to_string(),
            },
        }
    }

    fn handle_load_battery(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
            Err(e) => return e,
        };

        let data = match load_binary_param(params) {
            Ok(d) => d,
            Err(e) => return e,
        };

        match nes.load_battery(&data) {
            Ok(()) => {
                ToolResult::Success(serde_json::json!({"status": "ok", "size": data.len()}))
            }
            Err(e) => ToolResult::Error {
                code: -32000,
                message: e,
            },
        }
    }

    fn handle_record_video(&mut self, params: &JsonValue) -> ToolResult {
        let nes = match self.require_nes() {
            Ok(n) => n,
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

        let fps = match nes.region() {
            NesRegion::Ntsc => 60,
            NesRegion::Pal => 50,
        };

        let display = parse_display_size(params, nes.framebuffer_width(), nes.framebuffer_height());
        let mut rec = match emu_core::video::VideoRecorder::new(
            nes.framebuffer_width(),
            nes.framebuffer_height(),
            fps,
            1, // mono
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
            nes.run_frame();
            let audio = nes.take_audio_buffer();
            if let Err(e) = rec.add_frame(nes.framebuffer(), &audio) {
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
    fn unknown_tool_returns_error() {
        let mut mcp = NesMcp::new();
        let result = mcp.dispatch_tool("nonexistent", &JsonValue::Null);
        assert!(matches!(result, ToolResult::Error { code: -32601, .. }));
    }

    #[test]
    fn run_frames_without_boot_returns_error() {
        let mut mcp = NesMcp::new();
        let result = mcp.dispatch_tool("run_frames", &serde_json::json!({"count": 1}));
        assert!(matches!(result, ToolResult::Error { .. }));
    }
}
