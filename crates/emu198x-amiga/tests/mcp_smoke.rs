//! Integration test for the Amiga `--mcp` mode.
//!
//! Exercises the MCP server in-process: builds the same
//! `AmigaSession` the binary uses, drives a handful of
//! JSON-RPC requests through `Server::handle`, asserts the
//! responses are well-formed and that running advances the
//! machine. This is a smoke test, not a wedge investigation —
//! its job is to keep the Q1–Q3 plumbing from rotting silently.
//!
//! ROM lookup mirrors `ks31_boot.rs`:
//!   1. `$EMU198X_KS31_A1200_ROM`
//!   2. `~/.emu198x/roms/commodore-amiga/kick31a1200.rom`
//! Missing ROM → skip loudly with `eprintln!`, don't fail.

use std::path::PathBuf;

use emu198x_shell::mcp::{JsonRpcId, JsonRpcRequest, Server, ServerInfo};
use serde_json::{Value, json};

#[path = "../src/mcp/session.rs"]
mod session;
#[path = "../src/mcp/tools.rs"]
mod tools;

use runtime_commodore_amiga::Model;
use session::AmigaSession;

fn load_rom() -> Option<(Vec<u8>, PathBuf)> {
    let path = match std::env::var("EMU198X_KS31_A1200_ROM") {
        Ok(p) => PathBuf::from(p),
        Err(_) => {
            let home = std::env::var("HOME").expect("HOME is set");
            PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick31a1200.rom")
        }
    };
    if !path.exists() {
        eprintln!(
            "skipping: KS 3.1 A1200 ROM missing at {} (set $EMU198X_KS31_A1200_ROM)",
            path.display()
        );
        return None;
    }
    let bytes = std::fs::read(&path).expect("read KS 3.1 ROM");
    Some((bytes, path))
}

/// Build a request, dispatch it, return the result Value (or panic
/// loudly with the JSON-RPC error).
fn call(
    server: &mut Server<AmigaSession>,
    session: &mut AmigaSession,
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

/// `tools/call` wraps its result in `{ content: [{ text: "<json>" }] }`.
/// Pull the inner JSON out so tests can assert on it.
fn unwrap_tool_text(result: &Value) -> Value {
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|v| v.get("text"))
        .and_then(Value::as_str)
        .expect("tool result has content[0].text");
    serde_json::from_str(text).expect("tool text is JSON")
}

#[test]
fn mcp_server_boots_and_lists_tools() {
    let Some((rom_bytes, rom_path)) = load_rom() else {
        return;
    };
    let mut session = AmigaSession::new(Model::A1200AgaPal, rom_bytes, rom_path)
        .expect("session constructor accepts Kickstart-sized ROM");
    let mut server: Server<AmigaSession> =
        Server::new(ServerInfo::new("emu198x-amiga", "test"));
    tools::register_all(server.registry_mut());

    let init = call(&mut server, &mut session, 1, "initialize", json!({}));
    assert_eq!(
        init.get("protocolVersion").and_then(Value::as_str),
        Some("2025-06-18")
    );

    let list = call(&mut server, &mut session, 2, "tools/list", json!({}));
    let tools_arr = list
        .get("tools")
        .and_then(Value::as_array)
        .expect("tools/list returns tools array");
    let names: Vec<&str> = tools_arr
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str))
        .collect();
    for expected in &[
        "run_frames",
        "run_ticks",
        "run_until_pc",
        "run_until_any_pc",
        "run_until_mem_change",
        "step",
        "reset",
        "query_cpu",
        "query_chipset",
        "query_paula",
        "query_cia",
        "query_agnus",
        "query_blitter",
        "query_copper_list",
        "query_stack",
        "memory_read",
        "memory_read_long",
        "disasm",
        "insert_media",
        "eject_media",
        "query_disk",
        "query_aga",
        "bplcon0_log",
        "dump_framebuffer",
        "start_video_recording",
        "stop_video_recording",
        "palette_log",
        "restart",
    ] {
        assert!(
            names.contains(expected),
            "tools/list is missing `{expected}` (got {names:?})"
        );
    }
}

#[test]
fn mcp_tools_drive_a_real_boot() {
    let Some((rom_bytes, rom_path)) = load_rom() else {
        return;
    };
    let mut session = AmigaSession::new(Model::A1200AgaPal, rom_bytes, rom_path)
        .expect("session constructor accepts Kickstart-sized ROM");
    let mut server: Server<AmigaSession> =
        Server::new(ServerInfo::new("emu198x-amiga", "test"));
    tools::register_all(server.registry_mut());

    // Post-reset CPU state: PC in ROM window, SR with supervisor + IRQ mask.
    let cpu0 = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        10,
        "tools/call",
        json!({ "name": "query_cpu", "arguments": {} }),
    ));
    let pc0 = cpu0.get("pc").and_then(Value::as_str).unwrap();
    assert!(
        pc0.starts_with("$00F"),
        "fresh-boot PC should be in ROM window, got {pc0}"
    );
    assert_eq!(cpu0.get("supervisor").and_then(Value::as_bool), Some(true));

    // Step a few instructions; instruction counter must advance.
    let step = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        11,
        "tools/call",
        json!({ "name": "step", "arguments": { "count": 4 } }),
    ));
    assert_eq!(step.get("completed").and_then(Value::as_u64), Some(4));
    assert_eq!(
        step.get("trace").and_then(Value::as_array).map(Vec::len),
        Some(4)
    );

    // Run enough frames that the copper starts pointing at a list,
    // then dump it: at least one MOVE-to-BPLCON0 must appear.
    let _ = call(
        &mut server,
        &mut session,
        12,
        "tools/call",
        json!({ "name": "run_frames", "arguments": { "frames": 300 } }),
    );
    let chipset = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        13,
        "tools/call",
        json!({ "name": "query_chipset", "arguments": {} }),
    ));
    let cop1lc = chipset.get("cop1lc").and_then(Value::as_str).unwrap();
    assert_ne!(cop1lc, "$00000000", "expected COP1LC to be programmed");

    let copper = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        14,
        "tools/call",
        json!({ "name": "query_copper_list", "arguments": { "count": 16 } }),
    ));
    let entries = copper
        .get("entries")
        .and_then(Value::as_array)
        .expect("copper entries array");
    let bplcon0_move = entries.iter().any(|e| {
        e.get("op").and_then(Value::as_str) == Some("MOVE")
            && e.get("reg").and_then(Value::as_str) == Some("$0100")
    });
    assert!(
        bplcon0_move,
        "expected the copper list to contain a MOVE to BPLCON0 ($0100); got: {entries:?}"
    );

    // Eject (nothing inserted) and query: should report has_disk:false.
    let _ = call(
        &mut server,
        &mut session,
        15,
        "tools/call",
        json!({ "name": "eject_media", "arguments": {} }),
    );
    let disk = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        16,
        "tools/call",
        json!({ "name": "query_disk", "arguments": {} }),
    ));
    assert_eq!(disk.get("has_disk").and_then(Value::as_bool), Some(false));
}
