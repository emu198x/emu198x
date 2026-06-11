//! MCP server mode.
//!
//! Boots the same eager 48K runtime that script mode uses, builds a
//! shell-side `Server` with one tool per `ScriptStep` variant, and
//! drives the JSON-RPC stdio loop. Tool dispatch goes through the
//! same `execute_step` interceptor as `--script`, so SetMachine /
//! AutoloadTape / LoadBasicProgram behave identically across both
//! modes.
//!
//! See `docs/brainstorms/2026-05-08-mcp-server-brainstorm.md` for the
//! design and the SOLID criterion 5 acceptance bar.

pub(crate) mod tools;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::{
        register_ay_watch_tools, register_base_tools, register_keyboard_tools,
        register_memory_watch_tools,
    },
};
use runtime_sinclair_zx_spectrum::{SpectrumRuntimeKind, SpectrumSessionQueryProvider};

use crate::AppError;
use crate::script::runner::boot_eager_48k;

/// Runs MCP mode. Boots an eager 48K session wrapped in the
/// family-level [`SpectrumRuntimeKind`] enum, registers every tool,
/// and runs the stdio loop until stdin closes. Clients can switch the
/// active variant at any time via the `set_machine` tool.
///
/// # Errors
///
/// Returns an error if the 48K ROM cannot be loaded or the stdio loop
/// hits an I/O failure.
pub fn run() -> Result<(), AppError> {
    let runtime_48k = boot_eager_48k()?;
    let kind = SpectrumRuntimeKind::Spectrum48K(runtime_48k);
    let frame_halfcycles = u64::from(kind.frame_halfcycles());
    let mut session = HeadlessSession::new_with_query_provider(
        kind,
        frame_halfcycles,
        SpectrumSessionQueryProvider,
    );

    let mut server = Server::new(ServerInfo::new(
        "emu198x-spectrum",
        env!("CARGO_PKG_VERSION"),
    ));
    // Same uniform layering as the Amiga: shared common + debug + watch
    // tools, then the Spectrum-specific surface. The Spectrum (memory + AY)
    // implements `WatchTarget`, so both watch tiers register here. The
    // bespoke tools are registered last, overriding any generic version by
    // name and keeping the rich Z80 curriculum output.
    register_base_tools(server.registry_mut());
    register_memory_watch_tools(server.registry_mut());
    register_ay_watch_tools(server.registry_mut());
    register_keyboard_tools(server.registry_mut());
    tools::register_spectrum_tools(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(AppError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::mcp::{JsonRpcId, JsonRpcRequest};
    use serde_json::{Value, json};

    /// Every MCP tool the launch curriculum pipeline may call by name.
    /// This is the parity contract for the Phase-6 fold onto the shared
    /// `register_common_tools` + `register_debug_tools`: the fold may ADD
    /// tools (e.g. `run_ticks`, `io_trace`) but must not drop or rename
    /// any of these. Keep this list as the regression gate.
    ///
    /// `query_ay` was deliberately removed (#456): its data is now the
    /// grouped `ay` object + decoded `ay.*` query paths on the generic
    /// `query` tool. The curriculum does not call `query_ay` by name.
    const REQUIRED_TOOLS: &[&str] = &[
        "autoload_tape",
        "clear_audio_capture",
        "disasm",
        "input",
        "load_basic_program",
        "load_media",
        "load_snapshot",
        "media_transport",
        "memory_read",
        "poke_byte",
        "poke_word",
        "port_read",
        "port_write",
        "press_key",
        "query",
        "query_cpu",
        "query_paths",
        "reset",
        "run_frames",
        "run_until_pc",
        "save_audio_capture",
        "save_screenshot",
        "save_snapshot",
        "set_machine",
        "start_audio_recording",
        "start_video_recording",
        "step",
        "stop_audio_recording",
        "stop_video_recording",
        "type_string",
        "wait_for_boot",
        "wait_for_query_bool",
        "wait_for_query_contains",
        "watch_ay_clear",
        "watch_ay_log",
        "watch_ay_start",
        "watch_memory_clear",
        "watch_memory_log",
        "watch_memory_start",
    ];

    /// Register the full MCP surface exactly as `run()` does.
    fn register_full_surface(server: &mut Server<tools::SpectrumSession>) {
        register_base_tools(server.registry_mut());
        register_memory_watch_tools(server.registry_mut());
        register_ay_watch_tools(server.registry_mut());
        register_keyboard_tools(server.registry_mut());
        tools::register_spectrum_tools(server.registry_mut());
    }

    #[test]
    fn full_surface_publishes_every_curriculum_tool() {
        let mut server: Server<tools::SpectrumSession> =
            Server::new(ServerInfo::new("emu198x-spectrum", "0.0.0"));
        register_full_surface(&mut server);
        for name in REQUIRED_TOOLS {
            assert!(
                server.registry().get(name).is_some(),
                "the fold dropped the curriculum tool `{name}`"
            );
        }
    }

    fn call(
        server: &mut Server<tools::SpectrumSession>,
        session: &mut tools::SpectrumSession,
        id: i64,
        method: &str,
        params: Value,
    ) -> Value {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonRpcId::Number(id)),
            method: method.to_string(),
            params: Some(params),
        };
        let resp = server
            .handle(req, session)
            .expect("request had id, response must be Some");
        if let Some(err) = resp.error {
            panic!("{method} failed: {} (code {})", err.message, err.code);
        }
        resp.result.expect("success response carries result")
    }

    fn tool_text(result: &Value) -> Value {
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(|v| v.get("text"))
            .and_then(Value::as_str)
            .expect("tool result has content[0].text");
        serde_json::from_str(text).expect("tool text is JSON")
    }

    /// End-to-end parity smoke: boot a real 48K ROM, register the tool
    /// set, and drive a representative slice through JSON-RPC. This pins
    /// the live behaviour the Phase-6 fold must preserve. Skips loudly
    /// when the ROM is absent.
    #[test]
    fn mcp_tools_drive_a_real_boot() {
        let runtime_48k = match boot_eager_48k() {
            Ok(rt) => rt,
            Err(_) => {
                eprintln!("skipping: 48K ROM missing (set up ~/.emu198x/roms/...)");
                return;
            }
        };
        let kind = SpectrumRuntimeKind::Spectrum48K(runtime_48k);
        let frame_halfcycles = u64::from(kind.frame_halfcycles());
        let mut session = HeadlessSession::new_with_query_provider(
            kind,
            frame_halfcycles,
            SpectrumSessionQueryProvider,
        );
        let mut server: Server<tools::SpectrumSession> =
            Server::new(ServerInfo::new("emu198x-spectrum", "test"));
        register_full_surface(&mut server);

        // tools/list exposes every required tool.
        let list = call(&mut server, &mut session, 1, "tools/list", json!({}));
        let names: Vec<&str> = list
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();
        for name in REQUIRED_TOOLS {
            assert!(names.contains(name), "tools/list missing `{name}`");
        }

        // CPU snapshot carries a PC (shape the curriculum reads).
        let cpu = tool_text(&call(
            &mut server,
            &mut session,
            2,
            "tools/call",
            json!({ "name": "query_cpu", "arguments": {} }),
        ));
        assert!(
            cpu.get("registers").and_then(|r| r.get("pc")).is_some(),
            "query_cpu must report registers.pc: {cpu}"
        );

        // Run a few frames, then memory_read / disasm / step and a couple
        // of query paths all respond without error on the live machine.
        // (`ay` resolves only on AY-bearing variants; on this 48K boot it
        // is an unknown path, which `let _` tolerates.)
        call(
            &mut server,
            &mut session,
            3,
            "tools/call",
            json!({ "name": "run_frames", "arguments": { "frames": 4 } }),
        );
        for (id, name, args) in [
            (4, "memory_read", json!({ "addr": 0x4000, "len": 8 })),
            (5, "disasm", json!({ "addr": 0x0000, "instructions": 4 })),
            (6, "step", json!({ "instructions": 2 })),
            (7, "query", json!({ "path": "ay" })),
            (8, "query", json!({ "path": "boot.detected" })),
        ] {
            let _ = call(
                &mut server,
                &mut session,
                id,
                "tools/call",
                json!({ "name": name, "arguments": args }),
            );
        }
    }

    /// Regression for #6: the MCP `load_snapshot` tool must route a
    /// portable `.sna` through the shared snapshot parser, not
    /// postcard-decode it as the runtime's own save state. The bug
    /// surfaced as `Found a bool that wasn't 0 or 1` because a `.sna`
    /// is not postcard. Loads a hand-built 48K `.sna` whose RAM dump
    /// carries a sentinel byte and reads it back to prove the snapshot
    /// applied. Skips when the 48K ROM is absent.
    #[test]
    fn load_snapshot_routes_portable_sna_not_postcard() {
        let runtime_48k = match boot_eager_48k() {
            Ok(rt) => rt,
            Err(_) => {
                eprintln!("skipping: 48K ROM missing (set up ~/.emu198x/roms/...)");
                return;
            }
        };
        let kind = SpectrumRuntimeKind::Spectrum48K(runtime_48k);
        let frame_halfcycles = u64::from(kind.frame_halfcycles());
        let mut session = HeadlessSession::new_with_query_provider(
            kind,
            frame_halfcycles,
            SpectrumSessionQueryProvider,
        );
        let mut server: Server<tools::SpectrumSession> =
            Server::new(ServerInfo::new("emu198x-spectrum", "test"));
        register_full_surface(&mut server);

        // Minimal valid 48K .sna: 27-byte header + 49152 bytes of RAM
        // ($4000-$FFFF). Park SP at $6000 so the PC restore pops from
        // harmless zero RAM; IM = 1. A sentinel at $C000 proves the RAM
        // dump landed — a postcard misdecode would have errored, not
        // written RAM.
        const SENTINEL_ADDR: usize = 0xC000;
        let mut sna = vec![0u8; 49179];
        sna[23] = 0x00; // SP low
        sna[24] = 0x60; // SP high -> $6000
        sna[25] = 0x01; // interrupt mode 1
        sna[27 + (SENTINEL_ADDR - 0x4000)] = 0xA5;

        let path = std::env::temp_dir().join(format!(
            "emu198x_mcp_sna_regression_{}.sna",
            std::process::id()
        ));
        std::fs::write(&path, &sna).expect("write temp .sna");

        // Pre-fix this call postcard-decoded the .sna and errored; the
        // overriding Spectrum tool routes it to the portable parser. A
        // tool-level error surfaces as `isError`, which the sentinel
        // read below would also catch.
        let load = call(
            &mut server,
            &mut session,
            1,
            "tools/call",
            json!({ "name": "load_snapshot", "arguments": { "path": path.to_str().expect("temp path is valid UTF-8") } }),
        );
        assert_ne!(
            load.get("isError").and_then(Value::as_bool),
            Some(true),
            "load_snapshot of a portable .sna must not error: {load}"
        );

        let read = tool_text(&call(
            &mut server,
            &mut session,
            2,
            "tools/call",
            json!({ "name": "memory_read", "arguments": { "addr": SENTINEL_ADDR, "len": 1 } }),
        ));
        let first = read
            .get("bytes")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_u64);
        assert_eq!(
            first,
            Some(0xA5),
            "loaded .sna RAM byte at $C000 should be the sentinel 0xA5: {read}"
        );

        let _ = std::fs::remove_file(&path);
    }
}
