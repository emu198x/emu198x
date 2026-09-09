use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use emu198x_shell::{
    MachineCore,
    cycle_profile::{CycleCounts, ProfileMemory},
    debug_info::DebugSymbols,
};
use runtime_sinclair_zx_spectrum::{Spectrum128kRuntime, SpectrumPlus2Runtime};

fn symbols() -> DebugSymbols {
    DebugSymbols::from_ndjson(concat!(
        "{\"t\":\"header\",\"format\":\"debug198x\",\"format_version\":\"0.1\",\"tool\":\"test\",\"tool_version\":\"0\",\"cpu\":\"z80\",\"dialect\":\"sjasmplus\",\"sources\":[\"banks.s\"]}\n",
        "{\"t\":\"section\",\"id\":1,\"name\":\"one\",\"space\":{\"slot\":3,\"page\":0}}\n",
        "{\"t\":\"section\",\"id\":3,\"name\":\"three\",\"space\":{\"slot\":3,\"page\":4}}\n",
        "{\"t\":\"section\",\"id\":5,\"name\":\"five\",\"space\":{\"slot\":3,\"page\":5}}\n",
        "{\"t\":\"line\",\"file\":\"banks.s\",\"line\":1,\"section\":1,\"offset\":0,\"length\":4}\n",
        "{\"t\":\"line\",\"file\":\"banks.s\",\"line\":3,\"section\":3,\"offset\":0,\"length\":7}\n",
        "{\"t\":\"line\",\"file\":\"banks.s\",\"line\":5,\"section\":5,\"offset\":0,\"length\":3}\n",
    ), "banks.debug198x").expect("valid banked sidecar")
}

fn conserved(counts: &CycleCounts) {
    let code: u64 = counts.addresses.values().map(|cost| cost.ticks).sum();
    assert_eq!(
        code,
        counts
            .mapped_addresses
            .iter()
            .map(|entry| entry.cost.ticks)
            .sum::<u64>()
    );
    assert_eq!(
        counts.ticks,
        code + counts.interrupt_ticks
            + counts.halt_ticks
            + counts.leading_partial_ticks
            + counts.trailing_partial_ticks
    );
}

#[test]
fn paging_instruction_keeps_its_original_bank_and_same_pc_stays_distinct() {
    let mut runtime = Spectrum128kRuntime::blank();
    let machine = runtime.machine_mut();
    machine.memory.ram_bank_mut(0)[..4].copy_from_slice(&[0x3e, 4, 0xed, 0x79]); // LD A,4; OUT (C),A
    machine.memory.ram_bank_mut(4)[0] = 0x76; // HALT at the same PC in bank 4
    machine.memory.ram_bank_mut(4)[4..7].copy_from_slice(&[0xc3, 0, 0xc0]);
    machine.memory.write_7ffd(0);
    machine.z80.regs.pc = 0xc000;
    machine.z80.regs.bc = 0x7ffd;
    let snapshot = runtime.snapshot().expect("save paging fixture");
    let counts = runtime
        .profile_cycles(2000)
        .expect("capture paging program");
    conserved(&counts);
    assert_eq!(runtime.machine().memory.current_bank(), 4);
    assert_eq!(counts.addresses[&0xc000].executions, 2);
    let out = counts
        .mapped_addresses
        .iter()
        .find(|entry| entry.address == 0xc002)
        .expect("OUT captured");
    assert_eq!(out.mapping.page, 0);
    assert!(out.cost.ticks >= 60); // The paging port itself can be contended.
    let bank_zero_ticks = 35 + out.cost.ticks;
    let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&symbols()));
    assert_eq!(profile.clock.rate.numerator_hz, 17_734_475);
    assert_eq!(
        profile
            .lines
            .iter()
            .map(|line| (line.source.line, line.cost.ticks))
            .collect::<Vec<_>>(),
        vec![(1, bank_zero_ticks), (3, 70)]
    );
    assert_eq!(profile.unmapped_ticks, 0);
    // The same historical join is exposed by the shared MCP/script path.
    let mut restored = Spectrum128kRuntime::blank();
    restored.restore(&snapshot).expect("restore paging fixture");
    let mut session = emu198x_shell::HeadlessSession::new(restored, 354_540);
    session.set_debug_symbols(Some(symbols()));
    let mut registry = emu198x_shell::mcp::ToolRegistry::new();
    emu198x_shell::mcp_tools::register_tools_for_profiles(
        &mut registry,
        &session,
        &runtime_sinclair_zx_spectrum::profiles(),
    );
    let response = registry
        .get("profile_cycles")
        .expect("banked profile tool")
        .call(serde_json::json!({"ticks":2000}), &mut session)
        .expect("MCP banked capture");
    let value = serde_json::to_value(response).expect("serialize MCP response");
    let report: serde_json::Value =
        serde_json::from_str(value["content"][0]["text"].as_str().expect("text response"))
            .expect("JSON profile");
    assert_eq!(
        report,
        serde_json::to_value(emu198x_shell::ScriptObservation::ProfileCycles { profile })
            .expect("serialize expected profile")
    );
}

#[test]
fn aliases_keep_slot_identity_but_share_source_totals() {
    let mut runtime = Spectrum128kRuntime::blank();
    let machine = runtime.machine_mut();
    machine.memory.ram_bank_mut(5)[..3].copy_from_slice(&[0xc3, 0, 0xc0]);
    machine.memory.write_7ffd(5);
    machine.z80.regs.pc = 0x4000;
    let counts = runtime.profile_cycles(5000).expect("capture alias jump");
    conserved(&counts);
    assert_eq!(counts.mapped_addresses.len(), 2);
    assert_eq!(counts.mapped_addresses[0].mapping.slot, 1);
    assert_eq!(counts.mapped_addresses[1].mapping.slot, 3);
    let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&symbols()));
    assert_eq!(profile.lines.len(), 1);
    assert_eq!(
        profile.lines[0].cost.ticks,
        profile
            .counts
            .addresses
            .values()
            .map(|cost| cost.ticks)
            .sum::<u64>()
    );
}

#[test]
fn odd_divider_preserves_nop_costs_at_every_capture_phase() {
    for offset in 0..40 {
        let mut runtime = Spectrum128kRuntime::blank();
        runtime.machine_mut().z80.regs.pc = 0x8000;
        runtime.machine_mut().advance_halfcycles(offset);
        let counts = runtime.profile_cycles(100).expect("capture NOPs");
        conserved(&counts);
        for cost in counts.addresses.values() {
            assert_eq!(cost.ticks, 20, "offset {offset}");
        }
    }
}

#[test]
fn profiling_preserves_contended_machine_state_across_frames() {
    let make = || {
        let mut runtime = Spectrum128kRuntime::blank();
        runtime.machine_mut().memory.ram_bank_mut(1)[..3].copy_from_slice(&[0xc3, 0, 0xc0]);
        runtime.machine_mut().memory.write_7ffd(1);
        runtime.machine_mut().z80.regs.pc = 0xc000;
        runtime
    };
    let mut profiled = make();
    let mut ordinary = make();
    let ticks = profiled.machine().frame_timing().halfcycles_per_frame * 2 + 7;
    let counts = profiled.profile_cycles(ticks).expect("contended capture");
    ordinary.machine_mut().advance_halfcycles(ticks);
    assert_eq!(
        serde_json::to_value(profiled.machine()).expect("serialize profiled"),
        serde_json::to_value(ordinary.machine()).expect("serialize ordinary")
    );
    conserved(&counts);
    assert!(counts.addresses[&0xc000].ticks > counts.addresses[&0xc000].executions * 50);
}

#[test]
fn grey_plus2_uses_the_same_capture_and_rom_is_not_ram() {
    let mut runtime = SpectrumPlus2Runtime::blank();
    let counts = runtime.profile_cycles(20).expect("+2 ROM capture");
    conserved(&counts);
    assert_eq!(
        counts.mapped_addresses[0].mapping.memory,
        ProfileMemory::Rom
    );
    assert_eq!(counts.mapped_addresses[0].mapping.page, 0);
    let profile = counts.with_symbols(runtime.profile().clock.clone(), Some(&symbols()));
    assert_eq!(profile.unmapped_ticks, 20);
    assert!(profile.lines.is_empty());
}

#[test]
fn historical_page_lookup_ignores_live_paging_and_flat_sections() {
    let mut symbols = DebugSymbols::from_ndjson(
        include_str!(
            "../../../test-data/sinclair/zx-spectrum-128/debug198x/spectrum128-banked.debug198x"
        ),
        "spectrum128-banked.debug198x",
    )
    .expect("existing banked fixture");
    symbols.set_paging_from_slots([(3, 3, 0xc000, 0x4000)]);
    let (label, line) = symbols.annotation_in_page(1, 16);
    assert_eq!(label, Some("draw"));
    assert_eq!(line.expect("bank 1 source").line, 5);
    assert_eq!(
        symbols.symbol_at(0xc010),
        Some("music"),
        "historical lookup leaves live paging unchanged"
    );
    let flat = DebugSymbols::from_ndjson(
        include_str!("../../../test-data/sinclair/zx-spectrum/cycle-profile/loop.debug198x"),
        "loop.debug198x",
    )
    .expect("flat fixture");
    assert_eq!(flat.annotation_in_page(0, 0xc000), (None, None));
}
