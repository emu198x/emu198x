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
    let call_costs: Vec<_> = profile
        .routines
        .iter()
        .map(|row| {
            let cost = row.call_cost.as_ref().expect("inclusive cost");
            (
                cost.inclusive_ticks,
                cost.calls,
                cost.completed_calls,
                cost.incomplete_calls,
            )
        })
        .collect();
    assert_eq!(
        call_costs,
        vec![(520, 0, 0, 0), (368, 2, 2, 0), (0, 0, 0, 0)]
    );
    assert_eq!(
        profile
            .call_tracking
            .as_ref()
            .expect("tracking")
            .discontinuities,
        0
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

#[test]
fn recursive_program_counts_invocations_without_multiplying_ticks() {
    let mut session = session();
    for (base, bytes) in [
        (0xc000, &[0x06, 3, 0xcd, 0x10, 0xc0, 0x76][..]),
        (0xc010, &[0x05, 0xc8, 0xcd, 0x10, 0xc0, 0xc9][..]),
    ] {
        for (offset, &byte) in bytes.iter().enumerate() {
            session
                .machine_mut()
                .machine_mut()
                .write(base + u16::try_from(offset).expect("small code"), byte);
        }
    }
    let step: ScriptStep = serde_json::from_value(
        serde_json::json!({"action":"profile_cycles","ticks":1000,"routines":[
            {"name":"main","ranges":[{"start":49152,"end":49158}]},
            {"name":"work","ranges":[{"start":49168,"end":49174}]}
        ]}),
    )
    .expect("request");
    let Some(ScriptObservation::ProfileCycles { profile }) =
        step.execute_collect(&mut session).expect("capture")
    else {
        panic!("profile")
    };
    let main = profile.routines[0].call_cost.as_ref().expect("main");
    let work = profile.routines[1].call_cost.as_ref().expect("work");
    assert_eq!(main.inclusive_ticks, 460);
    assert_eq!(work.inclusive_ticks, 348);
    assert_eq!(work.inclusive_ticks, profile.routines[1].exclusive_ticks);
    assert_eq!(
        (work.calls, work.completed_calls, work.incomplete_calls),
        (3, 3, 0)
    );
    assert_eq!(profile.counts.halt_ticks, 528);
    assert_eq!(profile.counts.trailing_partial_ticks, 12);
    assert_eq!(profile.call_tracking.expect("summary").max_depth, 4);
}

#[test]
fn call_at_capture_end_does_not_invent_a_callee_duration() {
    let mut session = session();
    let step: ScriptStep = serde_json::from_value(
        serde_json::json!({"action":"profile_cycles","ticks":68,"routines":[
            {"name":"main","ranges":[{"start":49152,"end":49159}]},
            {"name":"work","ranges":[{"start":49168,"end":49174}]}
        ]}),
    )
    .expect("request");
    let Some(ScriptObservation::ProfileCycles { profile }) =
        step.execute_collect(&mut session).expect("capture")
    else {
        panic!("profile")
    };
    assert_eq!(
        profile.routines[0]
            .call_cost
            .as_ref()
            .expect("main")
            .inclusive_ticks,
        68
    );
    assert_eq!(
        profile.routines[1].call_cost.as_ref().expect("work").calls,
        0
    );
    assert_eq!(profile.call_tracking.expect("summary").unresolved_calls, 1);
}

#[test]
fn interrupt_inside_a_call_suspends_its_cost_without_changing_execution() {
    use common_sinclair_zx_spectrum::driver::SpectrumDriver;
    use emu198x_shell::{MachineCore, call_profile::CallProfiler};
    fn runtime() -> Spectrum48kRuntime {
        let mut rom = [0; 16384];
        rom[0x38..0x3b].copy_from_slice(&[0xf3, 0xed, 0x4d]); // DI; RETI
        let mut runtime = Spectrum48kRuntime::new_48k(rom);
        let frame = runtime.machine().frame_timing().halfcycles_per_frame;
        for _ in 0..frame {
            runtime.machine_mut().advance_halfcycles(1);
            if runtime.machine().z80().irq && runtime.machine().hc().is_multiple_of(4) {
                break;
            }
        }
        assert!(runtime.machine().z80().irq);
        *runtime.machine_mut().z80_mut() = emu198x_zilog_z80::Z80::new();
        let cpu = runtime.machine_mut().z80_mut();
        cpu.regs.pc = 0xc000;
        cpu.regs.sp = 0xff00;
        cpu.regs.im = 1;
        for (base, bytes) in [
            (0xc000, &[0xcd, 0x10, 0xc0, 0x76][..]),
            (0xc010, &[0xfb, 0, 0, 0xc9][..]),
        ] {
            for (offset, &byte) in bytes.iter().enumerate() {
                runtime
                    .machine_mut()
                    .write(base + u16::try_from(offset).expect("small code"), byte);
            }
        }
        runtime
    }
    let definitions: Vec<emu198x_shell::routine_profile::RoutineDefinition> =
        serde_json::from_value(serde_json::json!([
            {"name":"main","ranges":[{"start":49152,"end":49156}]},
            {"name":"work","ranges":[{"start":49168,"end":49172}]},
            {"name":"irq","ranges":[{"start":56,"end":59}]}
        ]))
        .expect("definitions");
    let mut tracker = CallProfiler::new(&definitions).expect("plan");
    let mut observed = runtime();
    let mut plain = runtime();
    let counts = observed
        .profile_cycles_observed(400, &mut tracker)
        .expect("capture");
    assert_eq!(
        serde_json::to_value(&counts).expect("counts"),
        serde_json::to_value(plain.profile_cycles(400).expect("plain capture"))
            .expect("plain counts")
    );
    assert_eq!(
        serde_json::to_value(observed.machine()).expect("observed state"),
        serde_json::to_value(plain.machine()).expect("plain state")
    );
    assert_eq!(observed.time(), plain.time());
    let mut report = counts.with_symbols(observed.profile().clock.clone(), None);
    tracker.finish(&mut report);
    let costs: Vec<_> = report
        .routines
        .iter()
        .map(|r| r.call_cost.as_ref().expect("cost").inclusive_ticks)
        .collect();
    assert_eq!(costs, vec![172, 88, 72]);
    assert_eq!(report.counts.interrupt_ticks, 52);
    let work = report.routines[1].call_cost.as_ref().expect("work");
    assert_eq!(
        (work.calls, work.completed_calls, work.incomplete_calls),
        (1, 1, 0)
    );
    assert_eq!(report.call_tracking.expect("summary").discontinuities, 0);
}

#[test]
fn asm_listing_compares_execution_weighted_exclusive_costs_in_script_and_mcp() {
    let listing: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test-data/sinclair/zx-spectrum/routine-profile/calls.listing.json"
    ))
    .expect("real Asm198x listing");
    let args = serde_json::json!({"ticks":552,"routines":[
        {"name":"main","ranges":[{"start":49152,"end":49159}]},
        {"name":"work","ranges":[{"start":49168,"end":49174}]}
    ],"static_cycles":{"cpu":"z80","listing":listing}});
    let mut command = args.clone();
    command["action"] = "profile_cycles".into();
    let step: ScriptStep = serde_json::from_value(command).expect("comparison request");
    let mut scripted = session();
    let observation = step
        .execute_collect(&mut scripted)
        .expect("capture")
        .expect("report");
    let ScriptObservation::ProfileCycles { profile } = &observation else {
        panic!("profile")
    };
    let comparison = profile.static_comparison.as_ref().expect("comparison");
    assert_eq!(comparison.ticks_per_cpu_cycle, 4);
    assert_eq!(comparison.compared_ticks, 520);
    assert_eq!(
        (
            comparison.expected_cycles.min,
            comparison.expected_cycles.max
        ),
        (120, 140)
    );
    let work = &comparison.routines[1];
    assert_eq!((work.compared_ticks, work.uncomparable_ticks), (368, 0));
    assert_eq!(
        (work.expected_cycles.min, work.expected_cycles.max),
        (82, 102)
    );
    assert_eq!(comparison.routines[0].expected_cycles.min, 38);
    assert_eq!(profile.counts.halt_ticks, 32);
    assert!(
        comparison
            .addresses
            .iter()
            .all(|row| row.relation == emu198x_shell::static_cycles::CycleRelation::WithinRange)
    );
    let djnz = comparison
        .addresses
        .iter()
        .find(|row| row.address == 0xc013)
        .expect("DJNZ");
    assert_eq!(djnz.executions, 4);
    assert_eq!(
        (djnz.expected_cycles.min, djnz.expected_cycles.max),
        (32, 52)
    );
    assert_eq!(djnz.measured_ticks, 168);
    let mut automated = session();
    let mut registry = emu198x_shell::mcp::ToolRegistry::new();
    emu198x_shell::mcp_tools::register_tools_for_profiles(
        &mut registry,
        &automated,
        &runtime_sinclair_zx_spectrum::profiles(),
    );
    let response = registry
        .get("profile_cycles")
        .expect("tool")
        .call(args, &mut automated)
        .expect("MCP capture");
    let value = serde_json::to_value(response).expect("response");
    let report: serde_json::Value =
        serde_json::from_str(value["content"][0]["text"].as_str().expect("text")).expect("JSON");
    assert_eq!(
        report,
        serde_json::to_value(observation).expect("script report")
    );
}

#[test]
fn static_comparison_refuses_wrong_cpu_and_missing_sidecar_before_execution() {
    for (cpu, remove_symbols, wrong_header) in [
        ("6502", false, false),
        ("z80", true, false),
        ("z80", false, true),
    ] {
        let mut session = session();
        if remove_symbols {
            session.set_debug_symbols(None);
        }
        if wrong_header {
            let sidecar = include_str!(
                "../../../test-data/sinclair/zx-spectrum/routine-profile/calls.debug198x"
            )
            .replace("\"cpu\":\"z80\"", "\"cpu\":\"6502\"");
            session.set_debug_symbols(Some(
                DebugSymbols::from_ndjson(&sidecar, "calls.debug198x").expect("sidecar"),
            ));
        }
        let before = serde_json::to_value(session.machine().machine()).expect("before");
        let step: ScriptStep =
            serde_json::from_value(serde_json::json!({"action":"profile_cycles","ticks":552,
            "static_cycles":{"cpu":cpu,"listing":{"lines":[]}}}))
            .expect("request");
        assert!(step.execute_collect(&mut session).is_err());
        assert_eq!(session.time().get(), 0);
        assert_eq!(
            serde_json::to_value(session.machine().machine()).expect("after"),
            before
        );
    }
}
