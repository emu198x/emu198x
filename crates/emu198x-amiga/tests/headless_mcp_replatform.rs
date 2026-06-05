//! Phase 3 foundation — prove the Amiga MCP can run on the shared
//! `HeadlessSession` + `register_common_tools` + `register_debug_tools`, before
//! the production cutover from the bespoke `AmigaSession`.
//!
//! This builds the *same* session the Amiga script path already uses (here on a
//! ROM-free blank machine), registers only the shared tool sets, and invokes a
//! representative slice — the generic verbs, the input verb (the gap that
//! motivated the replatform), and the debug verbs that Phase 2's `DebugTarget`
//! lit up. If this holds, the remaining work is porting the ~50 bespoke
//! copper/blitter/exec tools onto the same session (Phase 4), with this as the
//! safety net.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::{register_common_tools, register_debug_tools};
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaRuntimeKind, AmigaSessionQueryProvider, Model,
};
use serde_json::json;

type AmigaHeadless = HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>;

fn boot() -> AmigaHeadless {
    // Mirrors `emu198x-amiga/src/script.rs`, minus firmware: the shared session
    // over the Amiga family enum + its query provider.
    HeadlessSession::new_with_query_provider(
        AmigaRuntimeKind::blank(Model::A500OcsPal),
        A500_PAL_FRAME_TICKS,
        AmigaSessionQueryProvider,
    )
}

#[test]
fn shared_tool_sets_register_on_the_amiga_session() {
    let mut registry: ToolRegistry<AmigaHeadless> = ToolRegistry::new();
    register_common_tools(&mut registry);
    register_debug_tools(&mut registry);

    // The uniform surface every other machine exposes is present on the Amiga —
    // including `input` (keyboard/mouse over MCP, previously absent) and the
    // recording verbs.
    // NB: `reset` / `restart` are NOT in `register_common_tools` today — the
    // flagships register them bespoke. The cutover will need to add them (a
    // small shell addition, like the recording tools were), so they are
    // deliberately absent from this list.
    for name in [
        "run_frames",
        "run_ticks",
        "input",
        "load_media",
        "save_screenshot",
        "save_audio_capture",
        "start_video_recording",
        "query",
        "query_paths",
        "memory_read",
        "poke_byte",
        "disasm",
        "step",
        "query_cpu",
        "run_until_pc",
    ] {
        assert!(
            registry.get(name).is_some(),
            "shared tool `{name}` is registered on the Amiga session"
        );
    }
}

#[test]
fn shared_tools_drive_the_amiga_session() {
    let mut registry: ToolRegistry<AmigaHeadless> = ToolRegistry::new();
    register_common_tools(&mut registry);
    register_debug_tools(&mut registry);
    let mut session = boot();

    let call = |registry: &ToolRegistry<AmigaHeadless>,
                session: &mut AmigaHeadless,
                name: &str,
                args: serde_json::Value| {
        registry
            .get(name)
            .unwrap_or_else(|| panic!("tool `{name}` registered"))
            .call(args, session)
            .unwrap_or_else(|err| panic!("tool `{name}` ran: {err:?}"))
    };

    // Generic control: advance the machine.
    call(
        &registry,
        &mut session,
        "run_frames",
        json!({ "frames": 2 }),
    );

    // Input over MCP — keyboard event queued without error (the gap this whole
    // replatform closes for the Amiga).
    call(
        &registry,
        &mut session,
        "input",
        json!({ "events": [{ "Key": { "name": "a", "pressed": true } }] }),
    );

    // Debug verbs reach the Amiga's Phase-2 `DebugTarget`:
    // query_cpu returns the 68k register shape.
    let cpu = call(&registry, &mut session, "query_cpu", json!({}));
    let cpu_text = serde_json::to_string(&cpu).expect("serialise");
    assert!(
        cpu_text.contains("d0") && cpu_text.contains("pc") && cpu_text.contains("ssp"),
        "query_cpu returns the 68k register file, got {cpu_text}"
    );

    // memory_read folds bytes through the 24-bit bus; disasm decodes a 68k
    // instruction; both exercise the widened u32 surface.
    call(
        &registry,
        &mut session,
        "memory_read",
        json!({ "addr": "$F80000", "len": 4 }),
    );
    call(
        &registry,
        &mut session,
        "disasm",
        json!({ "addr": "$F80000", "count": 1 }),
    );
}
