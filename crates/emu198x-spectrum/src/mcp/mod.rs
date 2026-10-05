//! MCP server mode — the body behind
//! [`MachineApp::run_mcp`](emu198x_shell::launch::MachineApp::run_mcp).
//!
//! Boots the same eager 48K runtime that script mode uses, builds a
//! shell-side `Server` with one tool per `ScriptStep` variant, and
//! drives the JSON-RPC stdio loop. Tool dispatch goes through the
//! same `execute_step` interceptor as `--script`, so the portable
//! snapshot loader behaves identically across both modes; every other
//! step, `set_machine` included, runs in the shell.
//!
//! The shared server is not used because it reads `--rom` as cartridge
//! media; here `--rom ID=PATH` pins one ROM of the 48K boot bundle
//! (#842).
//!
//! See `docs/brainstorms/2026-05-08-mcp-server-brainstorm.md` for the
//! design and the SOLID criterion 5 acceptance bar.

pub(crate) mod tools;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, ToolRegistry, serve_stdio},
    mcp_tools::register_tools_for_profiles,
};
use runtime_sinclair_zx_spectrum::{Model, SpectrumSessionQueryProvider};

use crate::app::{AppError, firmware_overrides};
use crate::script::runner::boot_variant;

/// Register the full MCP surface. Same uniform layering as the Amiga:
/// shared common + debug + watch tools, then the Spectrum-specific
/// surface. The Spectrum (memory + AY) implements `WatchTarget`, so both
/// watch tiers register here. The bespoke tools are registered last,
/// overriding any generic version by name and keeping the rich Z80
/// curriculum output.
pub(crate) fn register_full_surface(
    registry: &mut ToolRegistry<tools::SpectrumSession>,
    session: &tools::SpectrumSession,
) {
    // The family catalogue, not the boot variant: MCP boots a 48K and a
    // client may `set_machine` to a 128K, which needs the AY tier already
    // registered.
    register_tools_for_profiles(registry, session, &runtime_sinclair_zx_spectrum::profiles());
    tools::register_spectrum_tools(registry);
}

/// Runs MCP mode. Boots an eager 48K session wrapped in the
/// family-level [`SpectrumRuntimeKind`] enum, registers every tool,
/// and runs the stdio loop until stdin closes. Clients can switch the
/// active variant at any time via the `set_machine` tool.
///
/// # Errors
///
/// Returns an error if the 48K ROM cannot be loaded or the stdio loop
/// hits an I/O failure.
pub fn run(rom_specs: &[String]) -> Result<(), AppError> {
    // MCP always boots 48K eagerly, so `--rom` resolves against that
    // bundle; a client that then swaps variant via `set_machine` gets the
    // conventional ROMs for the new one.
    let rom_overrides = firmware_overrides(rom_specs, Model::Spectrum48KPal)
        .map_err(|path| AppError::MissingRom { path })?;
    let kind = boot_variant(Model::Spectrum48KPal, &rom_overrides)?;
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
    register_full_surface(server.registry_mut(), &session);

    serve_stdio(&mut server, &mut session).map_err(AppError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::mcp::{JsonRpcId, JsonRpcRequest};
    use runtime_sinclair_zx_spectrum::{Spectrum48kRuntime, SpectrumRuntimeKind};
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

    #[test]
    fn full_surface_publishes_every_curriculum_tool() {
        // Registration reads the family catalogue, not the live machine,
        // so a zero-filled ROM is enough to stand in for the boot 48K.
        let kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::blank());
        let frame_halfcycles = u64::from(kind.frame_halfcycles());
        let session = HeadlessSession::new_with_query_provider(
            kind,
            frame_halfcycles,
            SpectrumSessionQueryProvider,
        );
        let mut server: Server<tools::SpectrumSession> =
            Server::new(ServerInfo::new("emu198x-spectrum", "0.0.0"));
        register_full_surface(server.registry_mut(), &session);
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
        let kind = match boot_variant(
            Model::Spectrum48KPal,
            &emu198x_shell::FirmwareOverrides::none(),
        ) {
            Ok(rt) => rt,
            Err(_) => {
                emu198x_test_skip::skip!(
                    "48K ROM not staged (~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom)"
                );
            }
        };
        let frame_halfcycles = u64::from(kind.frame_halfcycles());
        let mut session = HeadlessSession::new_with_query_provider(
            kind,
            frame_halfcycles,
            SpectrumSessionQueryProvider,
        );
        let mut server: Server<tools::SpectrumSession> =
            Server::new(ServerInfo::new("emu198x-spectrum", "test"));
        register_full_surface(server.registry_mut(), &session);

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
        let kind = match boot_variant(
            Model::Spectrum48KPal,
            &emu198x_shell::FirmwareOverrides::none(),
        ) {
            Ok(rt) => rt,
            Err(_) => {
                emu198x_test_skip::skip!(
                    "48K ROM not staged (~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom)"
                );
            }
        };
        let frame_halfcycles = u64::from(kind.frame_halfcycles());
        let mut session = HeadlessSession::new_with_query_provider(
            kind,
            frame_halfcycles,
            SpectrumSessionQueryProvider,
        );
        let mut server: Server<tools::SpectrumSession> =
            Server::new(ServerInfo::new("emu198x-spectrum", "test"));
        register_full_surface(server.registry_mut(), &session);

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

    /// A 48K `.sna` with interrupts disabled, SP at `$FFFC` holding the
    /// stacked PC, and `$0000` in the word above it — the shape in #1564,
    /// where a stray pop lands in the ROM. RAM holds two programs:
    /// `$8000: CALL $8006 / NOP / JR $8000 / RET` and `$8010: HALT`.
    fn issue_1564_sna(pc: u16) -> Vec<u8> {
        let mut sna = vec![0u8; 49179];
        sna[23..25].copy_from_slice(&0xFFFCu16.to_le_bytes()); // SP
        sna[25] = 0x01; // IM 1; IFF2 (byte 19) clear
        let ram = |addr: usize| 27 + addr - 0x4000;
        sna[ram(0x8000)..ram(0x8007)].copy_from_slice(&[0xCD, 0x06, 0x80, 0x00, 0x18, 0xFA, 0xC9]);
        sna[ram(0x8010)] = 0x76;
        sna[ram(0xFFFC)..ram(0xFFFE)].copy_from_slice(&pc.to_le_bytes());
        sna
    }

    fn step_pc_trace(
        server: &mut Server<tools::SpectrumSession>,
        session: &mut tools::SpectrumSession,
        instructions: u64,
    ) -> Vec<u64> {
        let step = tool_text(&call(
            server,
            session,
            90,
            "tools/call",
            json!({ "name": "step", "arguments": { "instructions": instructions } }),
        ));
        step.get("pc_trace")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("step reports pc_trace: {step}"))
            .iter()
            .filter_map(Value::as_u64)
            .collect()
    }

    /// Regression for #1564: `load_snapshot` of a `.sna` must run from the
    /// snapshot's PC whatever the previous program was doing. Stops the
    /// running program at frame ends (as `run_frames` does in the issue)
    /// many times over: its 43 T-state loop is co-prime with the 69,888
    /// T-state frame, so the stops land inside CALL, RET, NOP and JR in
    /// turn. Then stops it in HALT. Reloads after each stop. Pre-fix an in-flight RET popped `$0000` and the
    /// next instructions ran in the ROM. Skips when the 48K ROM is absent.
    #[test]
    fn load_snapshot_runs_from_the_sna_pc_after_any_previous_state() {
        let kind = match boot_variant(
            Model::Spectrum48KPal,
            &emu198x_shell::FirmwareOverrides::none(),
        ) {
            Ok(rt) => rt,
            Err(_) => {
                emu198x_test_skip::skip!(
                    "48K ROM not staged (~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom)"
                );
            }
        };
        let frame_halfcycles = u64::from(kind.frame_halfcycles());
        let mut session = HeadlessSession::new_with_query_provider(
            kind,
            frame_halfcycles,
            SpectrumSessionQueryProvider,
        );
        let mut server: Server<tools::SpectrumSession> =
            Server::new(ServerInfo::new("emu198x-spectrum", "test"));
        register_full_surface(server.registry_mut(), &session);

        let dir = std::env::temp_dir();
        let running = dir.join(format!("emu198x_issue_1564_run_{}.sna", std::process::id()));
        let halting = dir.join(format!(
            "emu198x_issue_1564_halt_{}.sna",
            std::process::id()
        ));
        std::fs::write(&running, issue_1564_sna(0x8000)).expect("write temp .sna");
        std::fs::write(&halting, issue_1564_sna(0x8010)).expect("write temp .sna");

        let load = |server: &mut Server<tools::SpectrumSession>,
                    session: &mut tools::SpectrumSession,
                    path: &std::path::Path| {
            let result = call(
                server,
                session,
                91,
                "tools/call",
                json!({ "name": "load_snapshot", "arguments": { "path": path.to_str().expect("temp path is valid UTF-8") } }),
            );
            assert_ne!(
                result.get("isError").and_then(Value::as_bool),
                Some(true),
                "load_snapshot failed: {result}"
            );
        };
        // CALL $8006 -> RET -> NOP, as PCs after each instruction.
        let expected = vec![0x8006, 0x8003, 0x8004];

        for attempt in 0..100 {
            load(&mut server, &mut session, &running);
            call(
                &mut server,
                &mut session,
                92,
                "tools/call",
                json!({ "name": "run_frames", "arguments": { "frames": 1 } }),
            );
            load(&mut server, &mut session, &running);
            assert_eq!(
                step_pc_trace(&mut server, &mut session, 3),
                expected,
                "reload {attempt} after a frame of the running program"
            );
        }

        load(&mut server, &mut session, &halting);
        call(
            &mut server,
            &mut session,
            93,
            "tools/call",
            json!({ "name": "run_frames", "arguments": { "frames": 1 } }),
        );
        load(&mut server, &mut session, &running);
        assert_eq!(
            step_pc_trace(&mut server, &mut session, 3),
            expected,
            "reload after the previous program halted"
        );

        let _ = std::fs::remove_file(&running);
        let _ = std::fs::remove_file(&halting);
    }

    /// The save side of #1564. Emu198x writes its own save state, never a
    /// `.sna` (`save_snapshot` refuses the extension), so the round trip
    /// to check is save -> load of that state. Whether the CPU was
    /// mid-instruction or halted, the reloaded machine must run exactly
    /// as the saved one did. Skips when the 48K ROM is absent.
    #[test]
    fn save_snapshot_round_trips_running_and_halted_cpu() {
        let kind = match boot_variant(
            Model::Spectrum48KPal,
            &emu198x_shell::FirmwareOverrides::none(),
        ) {
            Ok(rt) => rt,
            Err(_) => {
                emu198x_test_skip::skip!(
                    "48K ROM not staged (~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom)"
                );
            }
        };
        let frame_halfcycles = u64::from(kind.frame_halfcycles());
        let mut session = HeadlessSession::new_with_query_provider(
            kind,
            frame_halfcycles,
            SpectrumSessionQueryProvider,
        );
        let mut server: Server<tools::SpectrumSession> =
            Server::new(ServerInfo::new("emu198x-spectrum", "test"));
        register_full_surface(server.registry_mut(), &session);

        let dir = std::env::temp_dir();
        let pid = std::process::id();
        for (name, pc) in [("run", 0x8000u16), ("halt", 0x8010)] {
            let sna = dir.join(format!("emu198x_issue_1564_save_{name}_{pid}.sna"));
            let state = dir.join(format!(
                "emu198x_issue_1564_save_{name}_{pid}.emu198x-state"
            ));
            std::fs::write(&sna, issue_1564_sna(pc)).expect("write temp .sna");
            for (id, tool, args) in [
                (
                    94,
                    "load_snapshot",
                    json!({ "path": sna.to_str().expect("utf-8") }),
                ),
                (95, "run_frames", json!({ "frames": 1 })),
                (
                    96,
                    "save_snapshot",
                    json!({ "path": state.to_str().expect("utf-8") }),
                ),
            ] {
                let result = call(
                    &mut server,
                    &mut session,
                    id,
                    "tools/call",
                    json!({ "name": tool, "arguments": args }),
                );
                assert_ne!(
                    result.get("isError").and_then(Value::as_bool),
                    Some(true),
                    "{tool} failed ({name}): {result}"
                );
            }
            let saved_run = step_pc_trace(&mut server, &mut session, 8);
            let result = call(
                &mut server,
                &mut session,
                97,
                "tools/call",
                json!({ "name": "load_snapshot", "arguments": { "path": state.to_str().expect("utf-8") } }),
            );
            assert_ne!(
                result.get("isError").and_then(Value::as_bool),
                Some(true),
                "reloading the saved state failed ({name}): {result}"
            );
            assert_eq!(
                step_pc_trace(&mut server, &mut session, 8),
                saved_run,
                "the reloaded {name} state runs as the saved one did"
            );
            let _ = std::fs::remove_file(&sna);
            let _ = std::fs::remove_file(&state);
        }
    }
}
