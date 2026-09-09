use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use emu198x_shell::{MachineCore, cycle_profile::CycleCounts};
use runtime_sinclair_zx_spectrum::{Spectrum48kRuntime, Spectrum128kRuntime, SpectrumMachine};

fn program(address: u16, bytes: &[u8]) -> Spectrum48kRuntime {
    let mut runtime = Spectrum48kRuntime::new_48k([0; 16 * 1024]);
    for (offset, &byte) in bytes.iter().enumerate() {
        runtime.machine_mut().write_byte(
            address + u16::try_from(offset).expect("fixture fits in the address space"),
            byte,
        );
    }
    runtime.machine_mut().z80_mut().regs.pc = address;
    runtime
}

fn conserved(counts: &CycleCounts) {
    assert_eq!(
        counts.ticks,
        counts
            .addresses
            .values()
            .map(|cost| cost.ticks)
            .sum::<u64>()
            + counts.interrupt_ticks
            + counts.halt_ticks
            + counts.leading_partial_ticks
            + counts.trailing_partial_ticks
    );
}

#[test]
fn measured_loop_has_taken_and_untaken_costs_and_separate_halt_waiting() {
    // LD B,3; loop: NOP; DJNZ loop; HALT.
    // 7 + 3*4 + 2*13 + 8 + 4 = 57 T-states, then 8 T halted.
    let mut runtime = program(0xc000, &[0x06, 3, 0, 0x10, 0xfd, 0x76]);
    let counts = runtime.profile_cycles(65 * 4).expect("valid capture");
    assert_eq!(counts.addresses[&0xc000].ticks, 7 * 4);
    assert_eq!(counts.addresses[&0xc002].executions, 3);
    assert_eq!(counts.addresses[&0xc002].ticks, 12 * 4);
    assert_eq!(counts.addresses[&0xc003].ticks, 34 * 4);
    assert_eq!(counts.addresses[&0xc005].ticks, 4 * 4);
    assert_eq!(counts.halt_ticks, 8 * 4);
    assert_eq!(counts.leading_partial_ticks, 0);
    assert_eq!(counts.trailing_partial_ticks, 0);
    assert_eq!(runtime.time().get(), 65 * 4);
    conserved(&counts);
}

#[test]
fn prefixes_belong_to_the_first_byte_and_capture_can_end_mid_instruction() {
    let mut runtime = program(0xc000, &[0xdd, 0x21, 0, 0, 0xcb, 0x7c, 0x76]);
    let counts = runtime
        .profile_cycles(14 * 4 + 8 * 4 + 3)
        .expect("valid capture");
    assert_eq!(counts.addresses.len(), 2);
    assert_eq!(counts.addresses[&0xc000].ticks, 14 * 4);
    assert_eq!(counts.addresses[&0xc004].ticks, 8 * 4);
    assert_eq!(counts.trailing_partial_ticks, 3);
    conserved(&counts);
}

#[test]
fn partial_start_is_not_charged_as_a_complete_instruction() {
    let mut runtime = program(0xc000, &[0, 0, 0]);
    runtime.machine_mut().advance_halfcycles(5);
    let counts = runtime.profile_cycles(27).expect("valid capture");
    assert_eq!(counts.leading_partial_ticks, 11);
    assert_eq!(counts.addresses.len(), 1);
    assert_eq!(counts.addresses[&0xc001].ticks, 16);
    conserved(&counts);
}

#[test]
fn interrupt_entry_is_not_charged_to_the_interrupted_pc() {
    let mut rom = [0; 16 * 1024];
    rom[0x38] = 0x76; // handler halts after the IM1 response
    let mut runtime = Spectrum48kRuntime::new_48k(rom);
    // Align to the real ULA interrupt pulse rather than assuming its raster
    // origin coincides with the driver's initial frame counter.
    let frame = runtime.machine().frame_timing().halfcycles_per_frame;
    for _ in 0..frame {
        runtime.machine_mut().advance_halfcycles(1);
        if runtime.machine().z80().irq && runtime.machine().hc().is_multiple_of(4) {
            break;
        }
    }
    assert!(runtime.machine().z80().irq);
    let cpu = runtime.machine_mut().z80_mut();
    *cpu = emu198x_zilog_z80::Z80::new();
    cpu.regs.iff1 = true;
    cpu.regs.iff2 = true;
    cpu.regs.im = 1;
    cpu.regs.sp = 0xff00;
    let counts = runtime
        .profile_cycles((4 + 13 + 4 + 8) * 4)
        .expect("valid capture");
    assert_eq!(counts.interrupt_ticks, 13 * 4);
    assert_eq!(counts.addresses.len(), 2);
    assert_eq!(counts.addresses[&0].ticks, 4 * 4);
    assert_eq!(counts.addresses[&0x38].ticks, 4 * 4);
    assert_eq!(counts.halt_ticks, 8 * 4);
    conserved(&counts);
}

#[test]
fn capture_is_observational_across_contention_frame_wrap_and_snapshot_restore() {
    // Continuously execute contended RAM through two video frames.
    let bytes = [0xc3, 0, 0x40]; // JP $4000
    let mut profiled = program(0x4000, &bytes);
    let mut ordinary = program(0x4000, &bytes);
    let ticks = profiled.machine().frame_timing().halfcycles_per_frame * 2 + 5;
    let counts = profiled.profile_cycles(ticks).expect("valid capture");
    ordinary.machine_mut().advance_halfcycles(ticks);
    assert_eq!(
        serde_json::to_value(profiled.machine()).expect("serialize profiled machine"),
        serde_json::to_value(ordinary.machine()).expect("serialize ordinary machine")
    );
    let cost = &counts.addresses[&0x4000];
    assert!(
        cost.ticks > cost.executions * 10 * 4,
        "contention must increase elapsed cost"
    );
    conserved(&counts);
    let snapshot = profiled.snapshot().expect("capture snapshot");
    let mut restored = Spectrum48kRuntime::new_48k([0; 16 * 1024]);
    restored.restore(&snapshot).expect("restore snapshot");
    assert_eq!(
        profiled.profile_cycles(1000).expect("valid capture"),
        restored.profile_cycles(1000).expect("valid capture")
    );
}

#[test]
fn unsupported_models_and_invalid_budgets_do_not_advance() {
    let mut runtime = program(0xc000, &[0]);
    for ticks in [0, 14_000_001] {
        assert!(runtime.profile_cycles(ticks).is_err());
        assert_eq!(runtime.time().get(), 0);
        assert_eq!(runtime.machine().hc(), 0);
    }
    let mut banked = Spectrum128kRuntime::blank();
    assert!(banked.profile_cycles(16).is_err());
    assert_eq!(banked.time().get(), 0);
}

#[test]
fn every_start_phase_preserves_complete_nop_costs() {
    for offset in 0..32 {
        let mut runtime = program(0xc000, &[0; 32]);
        runtime.machine_mut().advance_halfcycles(offset);
        let counts = runtime.profile_cycles(64).expect("valid capture");
        for cost in counts.addresses.values() {
            assert_eq!(cost.ticks, 16, "offset {offset}");
        }
        conserved(&counts);
    }
}

#[test]
fn real_assembler_sidecar_joins_costs_and_mcp_matches_script() {
    use emu198x_shell::debug_info::DebugSymbols;
    use emu198x_shell::mcp::ToolRegistry;
    use emu198x_shell::mcp_tools::register_tools_for_profiles;
    use emu198x_shell::{HeadlessSession, ScriptObservation, ScriptStep};
    const BYTES: &[u8] =
        include_bytes!("../../../test-data/sinclair/zx-spectrum/cycle-profile/loop.bin");
    const SYMBOLS: &str =
        include_str!("../../../test-data/sinclair/zx-spectrum/cycle-profile/loop.debug198x");
    let session = || {
        let mut session = HeadlessSession::new(program(0xc000, BYTES), 279_552);
        session.set_debug_symbols(Some(
            DebugSymbols::from_ndjson(SYMBOLS, "loop.debug198x").expect("parse assembler sidecar"),
        ));
        session
    };
    let mut scripted = session();
    let step: ScriptStep =
        serde_json::from_value(serde_json::json!({"action":"profile_cycles","ticks":260}))
            .expect("deserialize profile command");
    let observation = step
        .execute_collect(&mut scripted)
        .expect("execute scripted capture")
        .expect("capture returns an observation");
    let ScriptObservation::ProfileCycles { profile } = &observation else {
        panic!("wrong observation")
    };
    assert_eq!(profile.clock.rate.numerator_hz, 14_000_000);
    assert_eq!(
        profile
            .lines
            .iter()
            .map(|line| (line.source.line, line.cost.ticks))
            .collect::<Vec<_>>(),
        vec![(5, 28), (6, 48), (7, 136), (8, 16)]
    );
    assert_eq!(profile.unmapped_ticks, 0);
    assert_eq!(profile.addresses[1].symbol.as_deref(), Some("loop"));
    let mut automated = session();
    let mut registry = ToolRegistry::new();
    register_tools_for_profiles(
        &mut registry,
        &automated,
        &runtime_sinclair_zx_spectrum::profiles(),
    );
    let response = registry
        .get("profile_cycles")
        .expect("48K registers the profile tool")
        .call(serde_json::json!({"ticks":260}), &mut automated)
        .expect("execute MCP capture");
    // The tool serializes exactly the shared ScriptObservation.
    let value = serde_json::to_value(response).expect("serialize MCP response");
    let report: serde_json::Value = serde_json::from_str(
        value["content"][0]["text"]
            .as_str()
            .expect("MCP content is text"),
    )
    .expect("MCP text contains JSON");
    assert_eq!(
        report,
        serde_json::to_value(observation).expect("serialize script observation")
    );
    assert_eq!(automated.time(), scripted.time());
    assert_eq!(automated.time().get(), 260);
}

#[test]
fn source_totals_preserve_unmapped_execution() {
    use emu198x_shell::cycle_profile::ExecutionCost;
    use emu198x_shell::debug_info::DebugSymbols;
    let symbols = DebugSymbols::from_ndjson(
        include_str!("../../../test-data/sinclair/zx-spectrum/cycle-profile/loop.debug198x"),
        "loop.debug198x",
    )
    .expect("parse assembler sidecar");
    let counts = CycleCounts {
        ticks: 32,
        addresses: [
            (
                0,
                ExecutionCost {
                    executions: 1,
                    ticks: 16,
                },
            ),
            (
                0xc002,
                ExecutionCost {
                    executions: 1,
                    ticks: 16,
                },
            ),
        ]
        .into(),
        ..CycleCounts::default()
    };
    let profile = counts.with_symbols(
        program(0xc000, &[0]).profile().clock.clone(),
        Some(&symbols),
    );
    assert_eq!(profile.unmapped_ticks, 16);
    assert_eq!(profile.lines.len(), 1);
    assert_eq!(profile.lines[0].cost.ticks, 16);
    assert!(profile.addresses[0].source.is_none());
}

#[test]
fn nmi_entry_is_a_separate_interval_too() {
    let mut rom = [0; 16 * 1024];
    rom[0x66] = 0x76;
    let mut runtime = Spectrum48kRuntime::new_48k(rom);
    runtime.machine_mut().z80_mut().nmi = true;
    runtime.machine_mut().z80_mut().regs.sp = 0xff00;
    let counts = runtime
        .profile_cycles((4 + 11 + 4 + 8) * 4)
        .expect("valid capture");
    assert_eq!(counts.interrupt_ticks, 44);
    assert_eq!(counts.addresses[&0].ticks, 16);
    assert_eq!(counts.addresses[&0x66].ticks, 16);
    assert_eq!(counts.halt_ticks, 32);
    conserved(&counts);
}

#[test]
fn profiling_consumes_queued_keyboard_input_before_running() {
    use emu198x_shell::{HeadlessSession, InputEvent, ScriptStep};
    // Select the A/S/D/F/G keyboard row, then read it.
    let mut session =
        HeadlessSession::new(program(0xc000, &[0x3e, 0xfd, 0xdb, 0xfe, 0x76]), 279_552);
    ScriptStep::Input {
        events: vec![InputEvent::Key {
            name: "A".into(),
            pressed: true,
        }],
    }
    .execute_collect(&mut session)
    .expect("queue keyboard input");
    ScriptStep::ProfileCycles { ticks: 72 }
        .execute_collect(&mut session)
        .expect("capture with queued input");
    assert_eq!(session.machine().machine().z80().regs.a() & 1, 0);
}

#[test]
fn active_recording_is_refused_without_advancing() {
    use emu198x_shell::{HeadlessSession, ScriptStep};
    let mut session = HeadlessSession::new(program(0xc000, &[0]), 279_552);
    let path = std::env::temp_dir().join(format!(
        "cycle-profile-recording-{}.wav",
        std::process::id()
    ));
    session
        .start_audio_recording(path)
        .expect("start lazy audio recording");
    assert!(
        ScriptStep::ProfileCycles { ticks: 16 }
            .execute_collect(&mut session)
            .is_err()
    );
    assert_eq!(session.time().get(), 0);
}

#[test]
fn indexed_bit_instruction_has_one_retirement_at_its_first_prefix() {
    let mut runtime = program(0xc000, &[0xdd, 0xcb, 0, 0x46, 0x76]);
    runtime.machine_mut().z80_mut().regs.ix = 0xd000;
    let counts = runtime.profile_cycles(24 * 4).expect("valid capture");
    assert_eq!(counts.addresses.len(), 2);
    assert_eq!(counts.addresses[&0xc000].executions, 1);
    assert_eq!(counts.addresses[&0xc000].ticks, 20 * 4);
    assert_eq!(counts.addresses[&0xc004].ticks, 4 * 4);
    conserved(&counts);
}

#[test]
fn block_repeat_charges_both_iterations_to_the_opcode() {
    let mut runtime = program(0xc000, &[0xed, 0xb0, 0x76]);
    let cpu = runtime.machine_mut().z80_mut();
    cpu.regs.hl = 0xd000;
    cpu.regs.de = 0xe000;
    cpu.regs.bc = 2;
    let counts = runtime
        .profile_cycles((21 + 16 + 4) * 4)
        .expect("valid capture");
    assert_eq!(counts.addresses.len(), 2);
    assert_eq!(counts.addresses[&0xc000].executions, 2);
    assert_eq!(counts.addresses[&0xc000].ticks, 37 * 4);
    assert_eq!(counts.addresses[&0xc002].ticks, 4 * 4);
    conserved(&counts);
}
