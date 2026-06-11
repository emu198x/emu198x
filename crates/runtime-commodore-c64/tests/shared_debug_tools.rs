//! End-to-end check of the SHARED debug-tool tier (`register_debug_tools`)
//! on a no-shadow 6502 machine. The C64 takes the shared tools as-is, so it's
//! the cleanest place to prove the cross-fleet enrichments land for every
//! machine: `disasm` carries raw `bytes` + `len`, `step` carries a `pc_trace`,
//! and `run_until_any_pc` / `run_until_mem_change` exist as shared verbs.
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
fn shared_debug_tools_are_enriched_and_complete() {
    let mut session = HeadlessSession::new_with_query_provider(
        C64Runtime::blank(Model::C64PalBreadbin),
        1,
        C64SessionQueryProvider,
    );
    let mut registry: ToolRegistry<C64Session> = ToolRegistry::new();
    register_debug_tools(&mut registry);

    // The two new shared run-until verbs are registered for every machine.
    assert!(
        registry.get("run_until_any_pc").is_some(),
        "run_until_any_pc is now a shared verb"
    );
    assert!(
        registry.get("run_until_mem_change").is_some(),
        "run_until_mem_change is now a shared verb"
    );

    // Plant LDA #$42 (A9 42) in RAM via the shared poke, then disassemble:
    // the line now carries the raw instruction bytes and length, not just text.
    call(
        &registry,
        &mut session,
        "poke_byte",
        json!({ "addr": "$1000", "value": "$A9" }),
    );
    call(
        &registry,
        &mut session,
        "poke_byte",
        json!({ "addr": "$1001", "value": "$42" }),
    );
    let dis = call(
        &registry,
        &mut session,
        "disasm",
        json!({ "addr": "$1000", "count": 1 }),
    );
    let line = &dis["lines"][0];
    assert_eq!(line["text"], "LDA #$42");
    assert_eq!(
        line["bytes"], "A9 42",
        "disasm now includes raw instruction bytes: {line}"
    );
    assert_eq!(line["len"], 2, "disasm now includes instruction length");

    // step now reports a per-instruction PC trace.
    let st = call(&registry, &mut session, "step", json!({ "count": 3 }));
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
        json!({ "targets": ["$FFFF", "$1234"], "max_steps": 50 }),
    );
    assert!(any.get("reached").is_some() && any.get("cpu_pc").is_some());

    let memchg = call(
        &registry,
        &mut session,
        "run_until_mem_change",
        json!({ "addr": "$0200", "max_steps": 50 }),
    );
    assert!(
        memchg.get("changed").is_some()
            && memchg.get("old").is_some()
            && memchg.get("new").is_some(),
        "run_until_mem_change reports changed/old/new: {memchg}"
    );
}
