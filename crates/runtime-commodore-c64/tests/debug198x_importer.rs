//! Debug198x importer, end to end on a C64 build (#741).
//!
//! The acceptance criterion for the importer is a whole workflow, not a unit:
//! load a lesson build (image + sidecar), see symbolised disassembly, set a
//! breakpoint on a source line, hit it, and see the line that was reached.
//! This runs that workflow against a real Asm198x build.
//!
//! # Why this needs no ROMs
//!
//! `C64Runtime::new` validates ROM *sizes*, not contents, so the test supplies
//! its own: an 8 KiB "KERNAL" that is zero everywhere except the reset vector,
//! which points at `$C000` — where the fixture loads. The first step performs
//! the 6502 reset sequence, which reads that vector and lands on the program
//! under test, with no Commodore ROM involved and nothing to stage. (The same
//! trick is already used for the 1541 stub in `common::stub_drive_rom_bytes`.)
//!
//! That matters beyond convenience: an accuracy test that skips whenever a
//! corpus is absent is green because it did not run. This one always runs.
//!
//! # The fixture
//!
//! `test-data/commodore/c64/debug198x/border-walk.{s,prg,debug198x}` — built by
//! a real `asm198x --dialect acme --prg --debug` run, not hand-authored, so the
//! reader is tested against what the writer actually emits.

use emu198x_shell::debug_info::DebugSymbols;
use emu198x_shell::{HeadlessSession, MachineCore, ScriptObservation, ScriptStep};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model};

const KERNAL_ROM_SIZE: usize = 0x2000;
const BASIC_ROM_SIZE: usize = 0x2000;
const CHARACTER_ROM_SIZE: usize = 0x1000;

/// PAL C64 cycles per frame — only used as the session's frame quantum; this
/// test steps instructions rather than running frames.
const PAL_CYCLES_PER_FRAME: u64 = 19_656;

/// Where the fixture is assembled to live, and where its reset vector points.
const PROGRAM_BASE: u32 = 0xc000;

const FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../test-data/commodore/c64/debug198x"
);

/// A C64 that comes up executing the fixture, with no Commodore ROMs.
fn c64_running_the_fixture() -> HeadlessSession<C64Runtime, C64SessionQueryProvider> {
    let mut kernal = vec![0u8; KERNAL_ROM_SIZE];
    // $FFFC/$FFFD is the 6502 reset vector; the KERNAL ROM covers $E000-$FFFF,
    // so it sits at offset $1FFC.
    kernal[0x1ffc] = (PROGRAM_BASE & 0xff) as u8;
    kernal[0x1ffd] = (PROGRAM_BASE >> 8) as u8;

    let runtime = C64Runtime::new(
        Model::C64PalBreadbin,
        kernal,
        vec![0u8; BASIC_ROM_SIZE],
        vec![0u8; CHARACTER_ROM_SIZE],
        None,
    )
    .expect("synthetic ROMs of the right size construct a runtime");

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        PAL_CYCLES_PER_FRAME,
        C64SessionQueryProvider,
    );

    let prg = std::fs::read(format!("{FIXTURE_DIR}/border-walk.prg")).expect("fixture .prg reads");
    let load_addr = session
        .machine_mut()
        .load_prg_bytes(&prg)
        .expect("fixture loads");
    assert_eq!(
        u32::from(load_addr),
        PROGRAM_BASE,
        "the fixture must load where the reset vector points"
    );

    session
}

/// Attaches the sidecar, as `load_debug_info` does.
fn load_sidecar(session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>) {
    ScriptStep::LoadDebugInfo {
        path: format!("{FIXTURE_DIR}/border-walk.debug198x").into(),
        section_bases: std::collections::BTreeMap::new(),
    }
    .execute(session)
    .expect("sidecar loads");
}

#[test]
fn the_first_step_takes_the_reset_vector_into_the_loaded_program() {
    let mut session = c64_running_the_fixture();

    // PC reads as 0 before anything runs: the 6502 reset sequence — which is
    // what fetches $FFFC/$FFFD — has not been executed yet. Asserted rather
    // than skipped over, because "PC is 0" looks like a broken setup, and the
    // next line is what shows it is not.
    assert_eq!(pc_of(&mut session), 0);

    ScriptStep::Step {
        instructions: Some(1),
    }
    .execute(&mut session)
    .expect("one step runs");

    assert_eq!(
        pc_of(&mut session),
        PROGRAM_BASE,
        "the synthetic reset vector should land the CPU on the program's entry"
    );
}

#[test]
fn loading_the_sidecar_reports_what_it_describes() {
    let mut session = c64_running_the_fixture();
    let observed = ScriptStep::LoadDebugInfo {
        path: format!("{FIXTURE_DIR}/border-walk.debug198x").into(),
        section_bases: std::collections::BTreeMap::new(),
    }
    .execute_collect(&mut session)
    .expect("sidecar loads")
    .expect("step reports");

    match observed {
        ScriptObservation::DebugInfoLoaded {
            cpu,
            dialect,
            sections,
            symbols,
            lines,
            ..
        } => {
            assert_eq!(cpu, "6502");
            assert_eq!(dialect, "acme");
            assert_eq!((sections, symbols, lines), (1, 5, 9));
        }
        other => panic!("expected DebugInfoLoaded, got {other:?}"),
    }
}

#[test]
fn disassembly_is_symbolised_once_the_sidecar_is_loaded() {
    let mut session = c64_running_the_fixture();

    // Before: addresses and mnemonics, and nothing else. This is the half of
    // the claim that is easy to lose — symbolisation must be *added* by the
    // sidecar, not faked by something the disassembler already knew.
    let bare = disasm(&mut session, PROGRAM_BASE, 8);
    assert!(
        bare.iter()
            .all(|i| i.symbol.is_none() && i.source.is_none()),
        "no sidecar loaded, so nothing should be annotated: {bare:?}"
    );

    load_sidecar(&mut session);
    let listing = disasm(&mut session, PROGRAM_BASE, 8);

    // Address operands now read as the labels they refer to. This is the
    // headline of #741: `BNE $C005` becomes `BNE loop`.
    let branch = listing
        .iter()
        .find(|i| i.addr == 0xc010)
        .expect("the branch is in range");
    let bare_branch = bare
        .iter()
        .find(|i| i.addr == 0xc010)
        .expect("the branch is in range");
    assert_eq!(
        bare_branch.mnemonic, "BNE $C005",
        "without a sidecar it is a bare address"
    );
    assert_eq!(branch.mnemonic, "BNE loop");

    // …and a store to a labelled location likewise.
    let store = listing
        .iter()
        .find(|i| i.addr == 0xc002)
        .expect("the store is in range");
    assert_eq!(store.mnemonic, "STA counter");

    // `border` is a *constant*, not a location, so the write to $D020 keeps
    // its address: there is no label defined there to name it.
    let border_write = listing
        .iter()
        .find(|i| i.addr == 0xc00b)
        .expect("the border write is in range");
    assert_eq!(border_write.mnemonic, "STA $D020");

    // Labels land on the instructions that carry them, and nowhere else.
    assert_eq!(listing[0].addr, PROGRAM_BASE);
    assert_eq!(listing[0].symbol.as_deref(), Some("start"));
    let at_loop = listing
        .iter()
        .find(|i| i.addr == 0xc005)
        .expect("the loop head is in range");
    assert_eq!(at_loop.symbol.as_deref(), Some("loop"));
    // Eight instructions reach $C012 (`rts`), so exactly three of the
    // fixture's five symbols are in range: `start`, `loop`, `done`. The other
    // two are `counter` (data, past the end) and `border` (a constant, which
    // is not a location at all).
    assert_eq!(
        listing
            .iter()
            .filter_map(|i| i.symbol.as_deref())
            .collect::<Vec<_>>(),
        ["start", "loop", "done"]
    );

    // …and every instruction knows the line it came from.
    let first = listing[0].source.as_ref().expect("first line is mapped");
    assert_eq!(first.line, 13);
    assert!(first.file.ends_with("border-walk.s"));
    assert!(
        listing.iter().all(|i| i.source.is_some()),
        "every instruction in an assembled program has a source line"
    );
}

#[test]
fn a_source_line_breakpoint_is_reached_and_reports_the_line() {
    let mut session = c64_running_the_fixture();
    load_sidecar(&mut session);

    // Line 18 is `sta border` — inside the loop, three instructions in.
    let observed = ScriptStep::RunUntilLine {
        file: "border-walk.s".into(),
        line: 18,
        max_steps: Some(10_000),
    }
    .execute_collect(&mut session)
    .expect("run_until_line executes")
    .expect("step reports");

    match observed {
        ScriptObservation::RunUntilLine {
            addr,
            reached,
            pc,
            stopped_at,
            steps,
            ..
        } => {
            assert_eq!(addr, Some(0xc00b), "line 18 assembled to $C00B");
            assert!(reached, "the loop body runs on the first pass");
            assert_eq!(pc, 0xc00b, "execution stopped on that instruction");
            let stopped_at = stopped_at.expect("the stopping point maps back to a line");
            assert_eq!(
                stopped_at.line, 18,
                "the line a debugger highlights is the one asked for"
            );
            assert!(steps > 0, "the machine actually ran to get there");
        }
        other => panic!("expected RunUntilLine, got {other:?}"),
    }

    // The session is left stopped there, not merely reported as having been:
    // a debugger resumes from this state, so the machine must still be on the
    // breakpoint after the step returns.
    assert_eq!(pc_of(&mut session), 0xc00b);
}

#[test]
fn a_line_with_no_code_reports_that_rather_than_running_to_the_budget() {
    let mut session = c64_running_the_fixture();
    load_sidecar(&mut session);

    // Line 15 is the bare label `loop:` — it emitted no bytes.
    let observed = ScriptStep::RunUntilLine {
        file: "border-walk.s".into(),
        line: 15,
        max_steps: Some(10_000),
    }
    .execute_collect(&mut session)
    .expect("step executes")
    .expect("step reports");

    match observed {
        ScriptObservation::RunUntilLine {
            addr,
            reached,
            steps,
            ..
        } => {
            assert_eq!(addr, None, "a codeless line has no breakpoint address");
            assert!(!reached);
            assert_eq!(steps, 0, "the machine should not have been run at all");
        }
        other => panic!("expected RunUntilLine, got {other:?}"),
    }
}

#[test]
fn a_symbol_resolves_to_the_address_a_breakpoint_can_use() {
    let mut session = c64_running_the_fixture();
    load_sidecar(&mut session);

    let observed = ScriptStep::DebugSymbol {
        name: "loop".into(),
    }
    .execute_collect(&mut session)
    .expect("step executes")
    .expect("step reports");

    let ScriptObservation::DebugSymbol { addr, .. } = observed else {
        panic!("expected DebugSymbol, got {observed:?}");
    };
    let addr = addr.expect("`loop` is a label in the fixture");
    assert_eq!(addr, 0xc005);

    // And it is usable as a breakpoint: run there by address.
    let observed = ScriptStep::RunUntilPc {
        addr,
        max_steps: Some(10_000),
    }
    .execute_collect(&mut session)
    .expect("step executes")
    .expect("step reports");
    let ScriptObservation::RunUntilPc { reached, pc, .. } = observed else {
        panic!("expected RunUntilPc, got {observed:?}");
    };
    assert!(reached);
    assert_eq!(pc, 0xc005);
}

#[test]
fn steps_needing_symbols_refuse_to_run_without_a_sidecar() {
    let mut session = c64_running_the_fixture();

    // No `load_debug_info` — this must say so rather than silently never
    // firing, which is the failure mode that costs an afternoon.
    let err = ScriptStep::RunUntilLine {
        file: "border-walk.s".into(),
        line: 18,
        max_steps: Some(10),
    }
    .execute(&mut session)
    .expect_err("must refuse");
    assert!(
        err.to_string().contains("load_debug_info"),
        "the error should name the fix, got: {err}"
    );
}

#[test]
fn the_sidecar_and_the_image_describe_the_same_bytes() {
    // The importer is only trustworthy if the sidecar it reads actually
    // describes the image that was loaded. Cross-check them: every line span
    // must cover bytes that are really in memory at that address.
    let mut session = c64_running_the_fixture();
    let symbols =
        DebugSymbols::load(format!("{FIXTURE_DIR}/border-walk.debug198x")).expect("sidecar loads");

    let prg = std::fs::read(format!("{FIXTURE_DIR}/border-walk.prg")).expect("fixture .prg reads");
    let image = &prg[2..]; // skip the 2-byte load address

    let target = session
        .machine_mut()
        .debug_target_mut()
        .expect("C64 has a debug target");
    for (offset, expected) in image.iter().enumerate() {
        let addr = PROGRAM_BASE + offset as u32;
        assert_eq!(
            target.peek(addr),
            *expected,
            "image byte at ${addr:04X} should be in RAM"
        );
    }

    // `start` is the first byte of the image, and `counter` the last.
    assert_eq!(symbols.addr_of("start"), Some(PROGRAM_BASE));
    assert_eq!(
        symbols.addr_of("counter"),
        Some(PROGRAM_BASE + image.len() as u32 - 1)
    );
}

/// Runs `disasm` and returns the decoded instructions.
fn disasm(
    session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    addr: u32,
    instructions: u32,
) -> Vec<emu198x_shell::DisasmInstruction> {
    let observed = ScriptStep::Disasm {
        addr,
        instructions: Some(instructions),
    }
    .execute_collect(session)
    .expect("disasm executes")
    .expect("step reports");
    match observed {
        ScriptObservation::Disasm { instructions, .. } => instructions,
        other => panic!("expected Disasm, got {other:?}"),
    }
}

/// Current program counter.
fn pc_of(session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>) -> u32 {
    session
        .machine_mut()
        .debug_target_mut()
        .expect("C64 has a debug target")
        .pc()
}
