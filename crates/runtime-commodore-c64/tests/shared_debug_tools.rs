//! End-to-end check of the SHARED debug-tool tier (`register_debug_tools`)
//! on a no-shadow 6502 machine. The C64 takes the shared tools as-is, so it's
//! the cleanest place to prove the unified surface: every debug verb is now a
//! `ScriptStepTool` over the machine-agnostic `ScriptStep` arms (so MCP and
//! `--script` run the identical body), and the observations carry the generic
//! shapes — `disasm` lines with text + raw bytes, `step` with a `pc_trace`,
//! plus `run_until_any_pc` / `run_until_mem_change`.
//!
//! Runs ROM-free against `C64Runtime::blank`.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::{ToolContent, ToolRegistry};
use emu198x_shell::mcp_tools::register_debug_tools;
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model};
use serde_json::{Value, json};

type C64Session = HeadlessSession<C64Runtime, C64SessionQueryProvider>;

fn call(
    registry: &ToolRegistry<C64Session>,
    session: &mut C64Session,
    name: &str,
    args: Value,
) -> Value {
    let resp = registry
        .get(name)
        .unwrap_or_else(|| panic!("tool `{name}` must be registered"))
        .call(args, session)
        .unwrap_or_else(|err| panic!("`{name}` call failed: {err}"));
    assert!(!resp.is_error, "`{name}` returned isError: {resp:?}");
    let ToolContent::Text { text } = resp.content.first().expect("a content block");
    serde_json::from_str(text).expect("tool text is JSON")
}

#[test]
fn shared_debug_tools_are_unified_and_complete() {
    let mut session = HeadlessSession::new_with_query_provider(
        C64Runtime::blank(Model::C64PalBreadbin),
        1,
        C64SessionQueryProvider,
    );
    let mut registry: ToolRegistry<C64Session> = ToolRegistry::new();
    register_debug_tools(&mut registry);

    for verb in [
        "query_cpu",
        "memory_read",
        "poke_byte",
        "poke_word",
        "disasm",
        "step",
        "run_until_pc",
        "run_until_any_pc",
        "run_until_mem_change",
    ] {
        assert!(
            registry.get(verb).is_some(),
            "`{verb}` must be a shared debug verb"
        );
    }

    // query_cpu carries the machine's register snapshot under `registers`.
    let cpu = call(&registry, &mut session, "query_cpu", json!({}));
    assert!(
        cpu.get("registers").and_then(|r| r.get("pc")).is_some(),
        "query_cpu exposes registers.pc: {cpu}"
    );

    // poke_byte writes through the debug target; memory_read reads it back.
    // Plant LDA #$42 = A9 42 into RAM at $1000.
    call(
        &registry,
        &mut session,
        "poke_byte",
        json!({ "addr": 0x1000, "value": 0xA9 }),
    );
    call(
        &registry,
        &mut session,
        "poke_byte",
        json!({ "addr": 0x1001, "value": 0x42 }),
    );
    let read = call(
        &registry,
        &mut session,
        "memory_read",
        json!({ "addr": 0x1000, "len": 2 }),
    );
    assert_eq!(
        read["bytes"],
        json!([0xA9, 0x42]),
        "memory_read returns the poked bytes: {read}"
    );

    // disasm decodes the planted instruction, carrying text + raw bytes + len.
    let dis = call(
        &registry,
        &mut session,
        "disasm",
        json!({ "addr": 0x1000, "instructions": 1 }),
    );
    let line = &dis["instructions"][0];
    assert_eq!(line["mnemonic"], "LDA #$42");
    assert_eq!(line["raw"], json!([0xA9, 0x42]), "disasm carries raw bytes");
    assert_eq!(line["bytes"], 2, "disasm carries instruction length");

    // step reports a per-instruction pc_trace.
    let st = call(
        &registry,
        &mut session,
        "step",
        json!({ "instructions": 3 }),
    );
    assert_eq!(
        st["pc_trace"].as_array().map(Vec::len),
        Some(3),
        "step traces the PC at each boundary: {st}"
    );

    // The new run-until verbs respond with the expected fields (a blank
    // machine won't actually reach/change, so assert shape, not outcome).
    let any = call(
        &registry,
        &mut session,
        "run_until_any_pc",
        json!({ "targets": [0xFFFF, 0x1234], "max_steps": 50 }),
    );
    assert!(any.get("reached").is_some() && any.get("pc").is_some());

    let memchg = call(
        &registry,
        &mut session,
        "run_until_mem_change",
        json!({ "addrs": [0x0200, 0x0201], "max_steps": 50 }),
    );
    assert!(
        memchg.get("changed").is_some() && memchg["addrs"] == json!([0x0200, 0x0201]),
        "run_until_mem_change reports changed + the watched addrs: {memchg}"
    );
}
