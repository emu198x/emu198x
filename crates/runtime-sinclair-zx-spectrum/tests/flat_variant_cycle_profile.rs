use common_sinclair_zx_spectrum::{driver::SpectrumDriver, memory::MemoryBus};
use emu198x_shell::{
    HeadlessSession, MachineCore, ScriptObservation, cycle_profile::CycleCounts,
    debug_info::DebugSymbols,
};
use runtime_sinclair_zx_spectrum::{Spectrum16kRuntime, Spectrum48kRuntime, SpectrumPlusRuntime};

fn conserved(counts: &CycleCounts) {
    assert!(counts.mapped_addresses.is_empty());
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
fn absent_upper_ram_executes_open_bus_on_16k_but_real_ram_on_plus() {
    for address in [0x8000, 0xffff] {
        let mut rom = [0; 16384];
        rom[0x38] = 0x76; // Stop after the $FF (RST $38) read from absent RAM.
        let mut small = Spectrum16kRuntime::new_16k(rom);
        let mut plus = SpectrumPlusRuntime::new_plus(rom);
        small.machine_mut().write(address, 0x76);
        plus.machine_mut().write(address, 0x76);
        assert_eq!(small.machine().read(address), 0xff);
        assert_eq!(plus.machine().read(address), 0x76);
        small.machine_mut().z80_mut().regs.pc = address;
        plus.machine_mut().z80_mut().regs.pc = address;
        small.machine_mut().z80_mut().regs.sp = 0x7000;
        let small_cost = small.profile_cycles(5000).expect("profile absent RAM");
        let plus_cost = plus.profile_cycles(5000).expect("profile real RAM");
        conserved(&small_cost);
        conserved(&plus_cost);
        assert_eq!(small_cost.addresses.len(), 2);
        assert!(small_cost.addresses[&u32::from(address)].ticks >= 44);
        assert_eq!(small_cost.addresses[&0x38].executions, 1);
        assert_eq!(plus_cost.addresses.len(), 1);
        assert_eq!(plus_cost.addresses[&u32::from(address)].ticks, 16);
    }
}

#[test]
fn last_16k_ram_byte_is_executable_and_capture_preserves_state() {
    let make = || {
        let mut runtime = Spectrum16kRuntime::blank();
        runtime.machine_mut().write(0x7fff, 0x76);
        runtime.machine_mut().z80_mut().regs.pc = 0x7fff;
        runtime
    };
    let mut profiled = make();
    let mut ordinary = make();
    let ticks = profiled.machine().frame_timing().halfcycles_per_frame * 2 + 5;
    let counts = profiled
        .profile_cycles(ticks)
        .expect("capture at RAM boundary");
    ordinary.machine_mut().advance_halfcycles(ticks);
    conserved(&counts);
    assert_eq!(counts.addresses.len(), 1);
    assert_eq!(counts.addresses[&0x7fff].executions, 1);
    assert!(counts.halt_ticks > 0);
    assert_eq!(
        serde_json::to_value(profiled.machine()).expect("serialize profiled"),
        serde_json::to_value(ordinary.machine()).expect("serialize ordinary")
    );
    let snapshot = profiled.snapshot().expect("capture snapshot");
    let mut restored = Spectrum16kRuntime::blank();
    restored.restore(&snapshot).expect("restore snapshot");
    assert_eq!(
        profiled.profile_cycles(1000).expect("continued capture"),
        restored.profile_cycles(1000).expect("restored capture")
    );
}

#[test]
fn plus_and_48k_measure_the_same_upper_ram_program() {
    let bytes = include_bytes!("../../../test-data/sinclair/zx-spectrum/cycle-profile/loop.bin");
    let mut baseline = Spectrum48kRuntime::new_48k([0; 16384]);
    let mut plus = SpectrumPlusRuntime::blank();
    for (offset, &byte) in bytes.iter().enumerate() {
        let address = 0xc000 + u16::try_from(offset).expect("small fixture");
        baseline.machine_mut().write(address, byte);
        plus.machine_mut().write(address, byte);
    }
    baseline.machine_mut().z80_mut().regs.pc = 0xc000;
    plus.machine_mut().z80_mut().regs.pc = 0xc000;
    let expected = baseline.profile_cycles(260).expect("48K fixture capture");
    let actual = plus.profile_cycles(260).expect("Spectrum+ fixture capture");
    assert_eq!(actual, expected);
    conserved(&actual);
}

#[test]
fn sixteen_k_mcp_joins_the_relocated_assembler_fixture() {
    let mut runtime = Spectrum16kRuntime::blank();
    for (offset, &byte) in
        include_bytes!("../../../test-data/sinclair/zx-spectrum/cycle-profile/loop.bin")
            .iter()
            .enumerate()
    {
        runtime
            .machine_mut()
            .write(0x4000 + u16::try_from(offset).expect("small fixture"), byte);
    }
    runtime.machine_mut().z80_mut().regs.pc = 0x4000;
    let mut symbols = DebugSymbols::from_ndjson(
        include_str!("../../../test-data/sinclair/zx-spectrum/cycle-profile/loop.debug198x"),
        "loop.debug198x",
    )
    .expect("assembler sidecar");
    symbols.set_section_base(0, 0x4000);
    let snapshot = runtime.snapshot().expect("fixture snapshot");
    let expected = runtime
        .profile_cycles(5000)
        .expect("direct fixture capture")
        .with_symbols(runtime.profile().clock.clone(), Some(&symbols));
    assert_eq!(expected.lines.len(), 4);
    assert_eq!(expected.lines[1].cost.executions, 3);
    assert_eq!(expected.lines[2].cost.executions, 3);
    assert_eq!(expected.unmapped_ticks, 0);
    runtime.restore(&snapshot).expect("restore fixture");
    let mut session = HeadlessSession::new(runtime, 279_552);
    session.set_debug_symbols(Some(symbols));
    let mut registry = emu198x_shell::mcp::ToolRegistry::new();
    emu198x_shell::mcp_tools::register_tools_for_profiles(
        &mut registry,
        &session,
        &runtime_sinclair_zx_spectrum::profiles(),
    );
    let response = registry
        .get("profile_cycles")
        .expect("shared profile tool")
        .call(serde_json::json!({"ticks":5000}), &mut session)
        .expect("MCP capture");
    let value = serde_json::to_value(response).expect("serialize MCP response");
    let report: serde_json::Value =
        serde_json::from_str(value["content"][0]["text"].as_str().expect("MCP text"))
            .expect("JSON report");
    assert_eq!(
        report,
        serde_json::to_value(ScriptObservation::ProfileCycles { profile: expected })
            .expect("serialize expected report")
    );
}
