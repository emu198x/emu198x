use common_sinclair_zx_spectrum::memory::MemoryBus;
use emu198x_shell::{HeadlessSession, ScriptObservation, ScriptStep, debug_info::DebugSymbols};
use runtime_sinclair_zx_spectrum::Spectrum48kRuntime;

fn session() -> HeadlessSession<Spectrum48kRuntime> {
    let mut runtime = Spectrum48kRuntime::new_48k([0; 16384]);
    for (offset, &byte) in
        include_bytes!("../../../test-data/sinclair/zx-spectrum/routine-profile/calls.bin")
            .iter()
            .enumerate()
    {
        runtime
            .machine_mut()
            .write(0xc000 + u16::try_from(offset).expect("small fixture"), byte);
    }
    runtime.machine_mut().z80_mut().regs.pc = 0xc000;
    runtime.machine_mut().z80_mut().regs.sp = 0xff00;
    let mut session = HeadlessSession::new(runtime, 279_552);
    session.set_debug_symbols(Some(
        DebugSymbols::from_ndjson(
            include_str!("../../../test-data/sinclair/zx-spectrum/routine-profile/calls.debug198x"),
            "calls.debug198x",
        )
        .expect("assembler sidecar"),
    ));
    session
}

#[test]
fn declared_routines_exclude_callee_costs_and_match_mcp() {
    let arguments = serde_json::json!({"ticks":552,"routines":[
        {"name":"main","ranges":[{"start":49152,"end":49155},{"start":49155,"end":49159}]},
        {"name":"work","ranges":[{"start":49168,"end":49174}]},
        {"name":"not_run","ranges":[{"start":49408,"end":49409}]}
    ]});
    let mut command = arguments.clone();
    command["action"] = "profile_cycles".into();
    let step: ScriptStep = serde_json::from_value(command).expect("routine request");
    let mut scripted = session();
    let observation = step
        .execute_collect(&mut scripted)
        .expect("capture")
        .expect("profile report");
    let ScriptObservation::ProfileCycles { profile } = &observation else {
        panic!("profile expected")
    };
    assert_eq!(
        profile
            .routines
            .iter()
            .map(|row| (row.instructions, row.exclusive_ticks))
            .collect::<Vec<_>>(),
        vec![(3, 152), (12, 368), (0, 0)]
    );
    assert_eq!(profile.unassigned_routine_ticks, Some(0));
    assert_eq!(profile.counts.halt_ticks, 32);
    assert_eq!(profile.routines[1].name, "work");
    let encoded = serde_json::to_value(profile).expect("serialize profile");
    let decoded: emu198x_shell::cycle_profile::CycleProfile =
        serde_json::from_value(encoded).expect("deserialize profile");
    assert_eq!(decoded.routines, profile.routines);
    assert_eq!(profile.unmapped_ticks, 0);
    assert_eq!(scripted.time().get(), 552);
    let mut automated = session();
    let mut registry = emu198x_shell::mcp::ToolRegistry::new();
    emu198x_shell::mcp_tools::register_tools_for_profiles(
        &mut registry,
        &automated,
        &runtime_sinclair_zx_spectrum::profiles(),
    );
    let response = registry
        .get("profile_cycles")
        .expect("profile tool")
        .call(arguments, &mut automated)
        .expect("MCP capture");
    let value = serde_json::to_value(response).expect("serialize response");
    let report: serde_json::Value =
        serde_json::from_str(value["content"][0]["text"].as_str().expect("MCP text"))
            .expect("report JSON");
    assert_eq!(
        report,
        serde_json::to_value(observation).expect("expected report")
    );
}

#[test]
fn invalid_routines_refuse_before_machine_advances() {
    for routines in [
        serde_json::json!([{"name":"bad","ranges":[]}]),
        serde_json::json!([{"name":"bad","ranges":[{"start":10,"end":10}]}]),
        serde_json::json!([{"name":"bad","ranges":[{"start":0,"end":4294967297u64}]}]),
        serde_json::json!([{"name":"same","ranges":[{"start":0,"end":1}]},{"name":"same","ranges":[{"start":1,"end":2}]}]),
        serde_json::json!([{"name":"one","ranges":[{"start":0,"end":10}]},{"name":"two","ranges":[{"start":9,"end":20}]}]),
        serde_json::json!([{"name":"   ","ranges":[{"start":0,"end":1}]}]),
    ] {
        let mut session = session();
        let before = serde_json::to_value(session.machine().machine()).expect("initial machine");
        let step: ScriptStep = serde_json::from_value(
            serde_json::json!({"action":"profile_cycles","ticks":552,"routines":routines}),
        )
        .expect("structurally valid request");
        assert!(step.execute_collect(&mut session).is_err());
        assert_eq!(session.time().get(), 0);
        assert_eq!(
            serde_json::to_value(session.machine().machine()).expect("unchanged machine"),
            before
        );
    }
}

#[test]
fn misspelled_page_fields_cannot_silently_become_flat_ranges() {
    let result = serde_json::from_value::<ScriptStep>(
        serde_json::json!({"action":"profile_cycles","ticks":16,"routines":[
            {"name":"bank","ranges":[{"start":0,"end":16,"page":5}]}
        ]}),
    );
    assert!(result.is_err());
}
