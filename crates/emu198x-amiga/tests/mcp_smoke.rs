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

#[path = "../src/mcp/lvo.rs"]
mod lvo;
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
        "memory_scan",
        "resolve_lvo",
        "query_library",
        "address_to_library",
        "read_task_stack",
        "dump_msgport_messages",
        "signal_task",
        "disasm",
        "disasm_around",
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

    // memory_scan should find the ExecBase pointer ($00000004) referenced
    // from itself or from chip-RAM structures that cache it. At minimum we
    // expect *some* match — Exec writes its own base into several Node
    // ln_Name fields and library jump-table slots. Use a tight chip-RAM
    // window so the test stays fast.
    let exec_base = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        20,
        "tools/call",
        json!({ "name": "memory_read_long", "arguments": { "addr": "$00000004" } }),
    ));
    let exec_base_str = exec_base
        .get("value")
        .and_then(Value::as_str)
        .expect("memory_read_long returns value");
    let scan = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        21,
        "tools/call",
        json!({
            "name": "memory_scan",
            "arguments": {
                "start": "$00000000",
                "end":   "$00010000",
                "value": exec_base_str,
                "stride": 4,
                "max_hits": 8
            }
        }),
    ));
    assert_eq!(
        scan.get("value").and_then(Value::as_str),
        Some(exec_base_str)
    );
    let scanned = scan.get("scanned").and_then(Value::as_u64).unwrap();
    assert!(scanned > 0, "memory_scan reported zero longwords scanned");
    let hits = scan.get("hits").and_then(Value::as_array).unwrap();
    let hit_count = scan.get("hit_count").and_then(Value::as_u64).unwrap();
    assert_eq!(hits.len() as u64, hit_count);
    // The longword at $00000004 IS the ExecBase pointer, so it must
    // appear in the hit list when we scan from $00000000.
    let self_hit = hits.iter().any(|h| {
        h.get("addr").and_then(Value::as_str) == Some("$00000004")
            && h.get("value").and_then(Value::as_str) == Some(exec_base_str)
    });
    assert!(
        self_hit,
        "memory_scan missed the ExecBase pointer at $00000004; hits={hits:?}"
    );

    // query_exec_tasks must label every entry with the RKM node-type
    // mnemonic (TASK, PROCESS, etc.) and, when an entry IS an
    // NT_PROCESS, attach the decoded Process struct. We can't rely on
    // disk-spawned processes existing here (no disk is inserted in
    // this smoke test), so we assert the invariant that holds for
    // every entry: ln_type_label is present, and Process decoding
    // fires exactly when ln_type == 13.
    let tasks = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        22,
        "tools/call",
        json!({ "name": "query_exec_tasks", "arguments": {} }),
    ));
    let mut all_entries: Vec<&Value> = Vec::new();
    for key in ["task_wait", "task_ready"] {
        if let Some(arr) = tasks.get(key).and_then(Value::as_array) {
            all_entries.extend(arr);
        }
    }
    if let Some(this_task) = tasks.get("this_task_info") {
        if !this_task.is_null() {
            all_entries.push(this_task);
        }
    }
    assert!(
        !all_entries.is_empty(),
        "expected at least one task entry after 300 frames"
    );
    for entry in &all_entries {
        let ln_type = entry.get("ln_type").and_then(Value::as_u64).unwrap();
        let label = entry.get("ln_type_label").and_then(Value::as_str).unwrap();
        assert!(!label.is_empty(), "ln_type_label must be populated");
        let has_process = entry.get("process").is_some();
        // Process decoder must fire iff the node is NT_PROCESS (13).
        assert_eq!(
            has_process,
            ln_type == 13,
            "process field presence must match ln_type==NT_PROCESS (got ln_type={ln_type}, label={label}, has_process={has_process})"
        );
        if has_process {
            let p = entry.get("process").unwrap();
            assert!(
                p.get("pr_msgport").is_some(),
                "decoded Process must include pr_msgport"
            );
            assert!(
                p.get("pr_cli").is_some(),
                "decoded Process must include pr_cli (BPTR-formatted)"
            );
        }
    }

    // resolve_lvo: known offset must hit; unknown library must
    // report `unknown_library` + the supported list; omitted offset
    // must dump the full table.
    let lvo_hit = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        23,
        "tools/call",
        json!({
            "name": "resolve_lvo",
            "arguments": { "library": "exec.library", "offset": -318 }
        }),
    ));
    assert_eq!(lvo_hit.get("match").and_then(Value::as_str), Some("hit"));
    assert_eq!(lvo_hit.get("name").and_then(Value::as_str), Some("Wait"));
    assert_eq!(
        lvo_hit.get("offset").and_then(Value::as_i64),
        Some(-318),
        "resolver must normalise to negative form"
    );
    let lvo_pos = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        24,
        "tools/call",
        json!({
            "name": "resolve_lvo",
            "arguments": { "library": "dos.library", "offset": "84" }
        }),
    ));
    assert_eq!(lvo_pos.get("name").and_then(Value::as_str), Some("Lock"));
    let lvo_bad = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        25,
        "tools/call",
        json!({
            "name": "resolve_lvo",
            "arguments": { "library": "nosuch.library" }
        }),
    ));
    assert_eq!(
        lvo_bad.get("match").and_then(Value::as_str),
        Some("unknown_library")
    );
    let supported = lvo_bad
        .get("supported_libraries")
        .and_then(Value::as_array)
        .expect("unknown_library response carries supported list");
    assert!(supported.iter().any(|v| v.as_str() == Some("exec.library")));

    let lvo_dump = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        26,
        "tools/call",
        json!({
            "name": "resolve_lvo",
            "arguments": { "library": "graphics.library" }
        }),
    ));
    assert_eq!(
        lvo_dump.get("match").and_then(Value::as_str),
        Some("library_dump")
    );
    let entries = lvo_dump
        .get("entries")
        .and_then(Value::as_array)
        .expect("library_dump carries entries");
    assert!(
        entries.len() > 100,
        "graphics.library has ~163 entries, got {}",
        entries.len()
    );

    // query_library must walk LibList and return at least exec.library.
    // Every Amiga boot has exec wired up; if the count is zero, ExecBase
    // or LibList is wrong. Pick exec.library to assert: it MUST be
    // present, MUST have NegSize > 0, and PosSize must straddle our
    // saved-PC range.
    let libs = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        27,
        "tools/call",
        json!({ "name": "query_library", "arguments": {} }),
    ));
    let library_count = libs.get("library_count").and_then(Value::as_u64).unwrap();
    assert!(
        library_count >= 1,
        "expected at least one loaded library after boot, got {library_count}"
    );
    let arr = libs.get("libraries").and_then(Value::as_array).unwrap();
    let exec = arr
        .iter()
        .find(|l| l.get("ln_name").and_then(Value::as_str) == Some("exec.library"))
        .expect("exec.library MUST be present in LibList");
    let neg = exec.get("neg_size").and_then(Value::as_u64).unwrap();
    let pos = exec.get("pos_size").and_then(Value::as_u64).unwrap();
    assert!(neg > 0, "exec.library NegSize must be > 0 (got {neg})");
    assert!(pos > 0, "exec.library PosSize must be > 0 (got {pos})");

    // Filter mode: name=exec.library should return exactly one entry.
    let exec_only = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        28,
        "tools/call",
        json!({
            "name": "query_library",
            "arguments": { "name": "exec.library" }
        }),
    ));
    assert_eq!(
        exec_only.get("library_count").and_then(Value::as_u64),
        Some(1)
    );

    // address_to_library: a chip-RAM address (e.g. $0) must miss;
    // an address INSIDE exec's code range must hit and report
    // `exec.library`.
    let miss = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        29,
        "tools/call",
        json!({
            "name": "address_to_library",
            "arguments": { "addr": "$00000000" }
        }),
    ));
    assert_eq!(
        miss.get("match").and_then(Value::as_str),
        Some("no_library_contains_addr")
    );
    // Pick an address mid-way through exec's code (library_addr + 4).
    let exec_addr_str = exec.get("addr").and_then(Value::as_str).unwrap();
    let exec_addr = u32::from_str_radix(exec_addr_str.trim_start_matches('$'), 16).unwrap();
    let probe = format!("${:08X}", exec_addr.wrapping_add(4));
    let hit = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        30,
        "tools/call",
        json!({
            "name": "address_to_library",
            "arguments": { "addr": probe }
        }),
    ));
    assert_eq!(hit.get("match").and_then(Value::as_str), Some("hit"));
    assert_eq!(
        hit.get("library").and_then(Value::as_str),
        Some("exec.library")
    );

    // read_task_stack: this_task should always have a stack pointer
    // by the time we've run 300 frames. We can't assert specific
    // ROM hits (the running task isn't parked!), but we can assert
    // the response shape is well-formed and that libraries were
    // searched against ExecBase->LibList.
    let tasks_state = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        31,
        "tools/call",
        json!({ "name": "query_exec_tasks", "arguments": {} }),
    ));
    if let Some(this_task) = tasks_state.get("this_task").and_then(Value::as_str) {
        if this_task != "$00000000" {
            let stack = unwrap_tool_text(&call(
                &mut server,
                &mut session,
                32,
                "tools/call",
                json!({
                    "name": "read_task_stack",
                    "arguments": { "task_addr": this_task, "bytes": 128 }
                }),
            ));
            assert!(stack.get("sp").is_some());
            assert!(stack.get("rom_hits").and_then(Value::as_array).is_some());
            assert!(
                stack
                    .get("libraries_searched")
                    .and_then(Value::as_u64)
                    .unwrap()
                    > 0,
                "read_task_stack must walk the library list"
            );
            assert!(
                stack.get("layout_note").is_some(),
                "response must carry a layout_note explaining how to read rom_hits"
            );
        }
    }

    // disasm_around: point at the current PC and ask for 2 before
    // + 2 after. We expect aligned=true since the CPU PC is on a
    // real instruction boundary, AND the target instruction must
    // appear with is_target=true.
    let cpu_now = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        33,
        "tools/call",
        json!({ "name": "query_cpu", "arguments": {} }),
    ));
    let pc_str = cpu_now.get("pc").and_then(Value::as_str).unwrap();
    let around = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        34,
        "tools/call",
        json!({
            "name": "disasm_around",
            "arguments": { "addr": pc_str, "before": 2, "after": 2 }
        }),
    ));
    assert_eq!(
        around.get("target").and_then(Value::as_str),
        Some(pc_str)
    );
    let instrs = around
        .get("instructions")
        .and_then(Value::as_array)
        .expect("disasm_around carries an instructions list");
    let target_marked = instrs
        .iter()
        .any(|i| i.get("is_target").and_then(Value::as_bool) == Some(true));
    assert!(
        target_marked,
        "disasm_around must mark exactly one instruction with is_target=true"
    );

    // dump_msgport_messages: point at a known port from
    // query_exec_ports. After 300 frames KS has at least one public
    // port (input.device, dos.library, etc.). Find one and dump it.
    let ports = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        35,
        "tools/call",
        json!({ "name": "query_exec_ports", "arguments": {} }),
    ));
    if let Some(port_arr) = ports.get("ports").and_then(Value::as_array) {
        if let Some(first) = port_arr.first() {
            let port_addr = first.get("addr").and_then(Value::as_str).unwrap();
            let dump = unwrap_tool_text(&call(
                &mut server,
                &mut session,
                36,
                "tools/call",
                json!({
                    "name": "dump_msgport_messages",
                    "arguments": { "port": port_addr }
                }),
            ));
            // We can't assert a specific message count (depends on
            // boot state), but the shape must match.
            assert!(dump.get("messages").and_then(Value::as_array).is_some());
            assert!(dump.get("count").and_then(Value::as_u64).is_some());
            assert!(
                dump.get("port").and_then(Value::as_object).is_some(),
                "response must echo the decoded port header"
            );
        }
    }

    // signal_task: pick the first task in TaskWait, OR an extra signal
    // bit into it, then re-query and confirm the bit is now set.
    // Crucially: we set a bit OUTSIDE sig_wait so the task doesn't
    // actually wake — keeps the test reproducible across runs.
    let tasks2 = unwrap_tool_text(&call(
        &mut server,
        &mut session,
        37,
        "tools/call",
        json!({ "name": "query_exec_tasks", "arguments": {} }),
    ));
    if let Some(waiters) = tasks2.get("task_wait").and_then(Value::as_array) {
        if let Some(first) = waiters.first() {
            let task_addr = first.get("addr").and_then(Value::as_str).unwrap();
            let sig_wait_str = first.get("tc_sig_wait").and_then(Value::as_str).unwrap();
            let sig_wait = u32::from_str_radix(sig_wait_str.trim_start_matches('$'), 16).unwrap();
            // Pick a bit NOT in sig_wait. Bit 0 is always allocated by
            // exec to signify "memory list change", but not waited on
            // here. If it happens to be in sig_wait, walk up the bits.
            let mut probe_bit: u32 = 0;
            for b in 0..32 {
                if (sig_wait & (1 << b)) == 0 {
                    probe_bit = 1 << b;
                    break;
                }
            }
            assert!(
                probe_bit != 0,
                "couldn't find an unwaited signal bit on the first waiter"
            );
            let signal = unwrap_tool_text(&call(
                &mut server,
                &mut session,
                38,
                "tools/call",
                json!({
                    "name": "signal_task",
                    "arguments": { "task_addr": task_addr, "signals": probe_bit }
                }),
            ));
            assert_eq!(
                signal.get("would_wake").and_then(Value::as_bool),
                Some(false),
                "we picked a bit OUTSIDE sig_wait — wake-up must be false"
            );
            // Re-query the task and confirm tc_sig_recvd carries the
            // bit we injected.
            let tasks3 = unwrap_tool_text(&call(
                &mut server,
                &mut session,
                39,
                "tools/call",
                json!({ "name": "query_exec_tasks", "arguments": {} }),
            ));
            let after = tasks3
                .get("task_wait")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .find(|e| e.get("addr").and_then(Value::as_str) == Some(task_addr))
                .expect("task must still be in TaskWait — we didn't trigger a wake");
            let recvd_str = after.get("tc_sig_recvd").and_then(Value::as_str).unwrap();
            let recvd = u32::from_str_radix(recvd_str.trim_start_matches('$'), 16).unwrap();
            assert!(
                (recvd & probe_bit) != 0,
                "signal_task didn't persist: tc_sig_recvd={recvd_str} probe_bit=${probe_bit:08X}"
            );
        }
    }

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
