use super::{M1Phase, Phase, Z80};
use crate::walker::Prefix;
use std::path::{Path, PathBuf};

const FUSE_Z80_TESTS_ENV: &str = "EMU198X_FUSE_Z80_TESTS_DIR";
const FUSE_CASE_ENV: &str = "EMU198X_FUSE_Z80_CASE";
const FUSE_LIMIT_ENV: &str = "EMU198X_FUSE_Z80_LIMIT";

#[derive(Clone, Debug, PartialEq, Eq)]
struct MemoryBlock {
    start: u16,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FuseInput {
    name: String,
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    af_alt: u16,
    bc_alt: u16,
    de_alt: u16,
    hl_alt: u16,
    ix: u16,
    iy: u16,
    sp: u16,
    pc: u16,
    wz: u16,
    i: u8,
    r: u8,
    iff1: bool,
    iff2: bool,
    im: u8,
    halted: bool,
    requested_tstates: u32,
    memory: Vec<MemoryBlock>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FuseEventKind {
    MemRead,
    MemWrite,
    MemContend,
    PortRead,
    PortWrite,
    PortContend,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FuseEvent {
    time: u32,
    kind: FuseEventKind,
    address: u16,
    data: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FuseExpected {
    name: String,
    events: Vec<FuseEvent>,
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    af_alt: u16,
    bc_alt: u16,
    de_alt: u16,
    hl_alt: u16,
    ix: u16,
    iy: u16,
    sp: u16,
    pc: u16,
    wz: u16,
    i: u8,
    r: u8,
    iff1: bool,
    iff2: bool,
    im: u8,
    halted: bool,
    final_tstates: u32,
    memory: Vec<MemoryBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FuseObserved {
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    af_alt: u16,
    bc_alt: u16,
    de_alt: u16,
    hl_alt: u16,
    ix: u16,
    iy: u16,
    sp: u16,
    pc: u16,
    wz: u16,
    i: u8,
    r: u8,
    iff1: bool,
    iff2: bool,
    im: u8,
    halted: bool,
    final_tstates: u32,
    memory: [u8; 65_536],
}

const ACCEPTED_FUSE_DISAGREEMENTS: &[(&str, &[&str])] = &[
    ("76", &["PC"]),
    ("edb2_1", &["AF", "WZ"]),
    ("edb3_1", &["AF", "WZ"]),
    ("edb9_2", &["AF"]),
    ("edba_1", &["WZ"]),
    ("edbb_1", &["AF", "WZ"]),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .to_path_buf()
}

fn home_projects_path(relative: &str) -> PathBuf {
    match dirs::home_dir() {
        Some(home) => home.join("Projects").join(relative),
        None => PathBuf::from("/missing-home").join(relative),
    }
}

fn first_existing_path(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| path.exists())
}

fn find_fuse_z80_tests_dir() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(FUSE_Z80_TESTS_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(repo_root().join("test-data/fuse-z80"));
    candidates.push(home_projects_path(
        "Emu198x-Unclean/fuse-emulator-fuse/z80/tests",
    ));
    candidates.push(home_projects_path("Reference/fuse-emulator-fuse/z80/tests"));

    first_existing_path(candidates).ok_or_else(|| {
        format!(
            "FUSE Z80 test directory not found. Set {FUSE_Z80_TESTS_ENV} or place the data in one of:\n  - {}\n  - {}\n  - {}",
            repo_root().join("test-data/fuse-z80").display(),
            home_projects_path("Emu198x-Unclean/fuse-emulator-fuse/z80/tests").display(),
            home_projects_path("Reference/fuse-emulator-fuse/z80/tests").display(),
        )
    })
}

fn parse_hex_u16(token: &str) -> u16 {
    u16::from_str_radix(token, 16)
        .unwrap_or_else(|error| panic!("failed to parse '{token}' as u16 hex: {error}"))
}

fn parse_hex_u8(token: &str) -> u8 {
    u8::from_str_radix(token, 16)
        .unwrap_or_else(|error| panic!("failed to parse '{token}' as u8 hex: {error}"))
}

fn parse_bool_flag(token: &str) -> bool {
    match token {
        "0" => false,
        "1" => true,
        other => panic!("expected boolean flag 0 or 1, got '{other}'"),
    }
}

fn parse_memory_block(line: &str) -> Option<MemoryBlock> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed == "-1" {
        return None;
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    assert!(
        !tokens.is_empty(),
        "memory block line must contain at least an address"
    );

    let mut bytes = Vec::new();
    for token in &tokens[1..] {
        if *token == "-1" {
            break;
        }
        bytes.push(parse_hex_u8(token));
    }

    Some(MemoryBlock {
        start: parse_hex_u16(tokens[0]),
        bytes,
    })
}

fn parse_tests_in(data: &str) -> Vec<FuseInput> {
    let mut tests = Vec::new();
    let mut lines = data.lines().peekable();

    while let Some(raw_name) = lines.next() {
        let name = raw_name.trim();
        if name.is_empty() {
            continue;
        }

        let register_line = lines
            .next()
            .unwrap_or_else(|| panic!("missing register line for test '{name}'"));
        let register_tokens: Vec<&str> = register_line.split_whitespace().collect();
        assert_eq!(
            register_tokens.len(),
            13,
            "bad register line for test '{name}': {register_line}"
        );

        let state_line = lines
            .next()
            .unwrap_or_else(|| panic!("missing state line for test '{name}'"));
        let state_tokens: Vec<&str> = state_line.split_whitespace().collect();
        assert_eq!(
            state_tokens.len(),
            7,
            "bad state line for test '{name}': {state_line}"
        );

        let mut memory = Vec::new();
        loop {
            let Some(memory_line) = lines.next() else {
                panic!("unterminated memory section for test '{name}'");
            };

            if memory_line.trim() == "-1" {
                break;
            }

            if let Some(block) = parse_memory_block(memory_line) {
                memory.push(block);
            }
        }

        while lines.peek().is_some_and(|line| line.trim().is_empty()) {
            lines.next();
        }

        tests.push(FuseInput {
            name: name.to_string(),
            af: parse_hex_u16(register_tokens[0]),
            bc: parse_hex_u16(register_tokens[1]),
            de: parse_hex_u16(register_tokens[2]),
            hl: parse_hex_u16(register_tokens[3]),
            af_alt: parse_hex_u16(register_tokens[4]),
            bc_alt: parse_hex_u16(register_tokens[5]),
            de_alt: parse_hex_u16(register_tokens[6]),
            hl_alt: parse_hex_u16(register_tokens[7]),
            ix: parse_hex_u16(register_tokens[8]),
            iy: parse_hex_u16(register_tokens[9]),
            sp: parse_hex_u16(register_tokens[10]),
            pc: parse_hex_u16(register_tokens[11]),
            wz: parse_hex_u16(register_tokens[12]),
            i: parse_hex_u8(state_tokens[0]),
            r: parse_hex_u8(state_tokens[1]),
            iff1: parse_bool_flag(state_tokens[2]),
            iff2: parse_bool_flag(state_tokens[3]),
            im: state_tokens[4]
                .parse()
                .unwrap_or_else(|error| panic!("failed to parse IM for test '{name}': {error}")),
            halted: parse_bool_flag(state_tokens[5]),
            requested_tstates: state_tokens[6].parse().unwrap_or_else(|error| {
                panic!("failed to parse requested tstates for test '{name}': {error}")
            }),
            memory,
        });
    }

    tests
}

fn parse_event_kind(token: &str) -> FuseEventKind {
    match token {
        "MR" => FuseEventKind::MemRead,
        "MW" => FuseEventKind::MemWrite,
        "MC" => FuseEventKind::MemContend,
        "PR" => FuseEventKind::PortRead,
        "PW" => FuseEventKind::PortWrite,
        "PC" => FuseEventKind::PortContend,
        other => panic!("unknown FUSE event kind '{other}'"),
    }
}

fn parse_tests_expected(data: &str) -> Vec<FuseExpected> {
    let mut tests = Vec::new();
    let mut lines = data.lines().peekable();

    while let Some(raw_name) = lines.next() {
        let name = raw_name.trim();
        if name.is_empty() {
            continue;
        }

        let mut events = Vec::new();
        while lines.peek().is_some_and(|line| line.starts_with(' ')) {
            let event_line = lines
                .next()
                .unwrap_or_else(|| panic!("missing event line for test '{name}'"));
            let event_tokens: Vec<&str> = event_line.split_whitespace().collect();
            assert!(
                (3..=4).contains(&event_tokens.len()),
                "bad event line for test '{name}': {event_line}"
            );

            events.push(FuseEvent {
                time: event_tokens[0].parse().unwrap_or_else(|error| {
                    panic!("failed to parse event time for test '{name}': {error}")
                }),
                kind: parse_event_kind(event_tokens[1]),
                address: parse_hex_u16(event_tokens[2]),
                data: event_tokens.get(3).map(|token| parse_hex_u8(token)),
            });
        }

        let register_line = lines
            .next()
            .unwrap_or_else(|| panic!("missing register line for expected test '{name}'"));
        let register_tokens: Vec<&str> = register_line.split_whitespace().collect();
        assert_eq!(
            register_tokens.len(),
            13,
            "bad register line for expected test '{name}': {register_line}"
        );

        let state_line = lines
            .next()
            .unwrap_or_else(|| panic!("missing state line for expected test '{name}'"));
        let state_tokens: Vec<&str> = state_line.split_whitespace().collect();
        assert_eq!(
            state_tokens.len(),
            7,
            "bad state line for expected test '{name}': {state_line}"
        );

        let mut memory = Vec::new();
        while let Some(next_line) = lines.peek() {
            let trimmed = next_line.trim();
            if trimmed.is_empty() {
                break;
            }
            if trimmed == "-1" {
                lines.next();
                break;
            }

            let line = lines
                .next()
                .unwrap_or_else(|| panic!("missing memory line for expected test '{name}'"));
            if let Some(block) = parse_memory_block(line) {
                memory.push(block);
            }
        }

        while lines.peek().is_some_and(|line| line.trim().is_empty()) {
            lines.next();
        }

        tests.push(FuseExpected {
            name: name.to_string(),
            events,
            af: parse_hex_u16(register_tokens[0]),
            bc: parse_hex_u16(register_tokens[1]),
            de: parse_hex_u16(register_tokens[2]),
            hl: parse_hex_u16(register_tokens[3]),
            af_alt: parse_hex_u16(register_tokens[4]),
            bc_alt: parse_hex_u16(register_tokens[5]),
            de_alt: parse_hex_u16(register_tokens[6]),
            hl_alt: parse_hex_u16(register_tokens[7]),
            ix: parse_hex_u16(register_tokens[8]),
            iy: parse_hex_u16(register_tokens[9]),
            sp: parse_hex_u16(register_tokens[10]),
            pc: parse_hex_u16(register_tokens[11]),
            wz: parse_hex_u16(register_tokens[12]),
            i: parse_hex_u8(state_tokens[0]),
            r: parse_hex_u8(state_tokens[1]),
            iff1: parse_bool_flag(state_tokens[2]),
            iff2: parse_bool_flag(state_tokens[3]),
            im: state_tokens[4].parse().unwrap_or_else(|error| {
                panic!("failed to parse IM for expected test '{name}': {error}")
            }),
            halted: parse_bool_flag(state_tokens[5]),
            final_tstates: state_tokens[6].parse().unwrap_or_else(|error| {
                panic!("failed to parse final tstates for expected test '{name}': {error}")
            }),
            memory,
        });
    }

    tests
}

fn deadbeef_memory() -> [u8; 65_536] {
    let mut memory = [0u8; 65_536];
    for chunk in memory.chunks_exact_mut(4) {
        chunk[0] = 0xDE;
        chunk[1] = 0xAD;
        chunk[2] = 0xBE;
        chunk[3] = 0xEF;
    }
    memory
}

fn apply_memory_blocks(memory: &mut [u8; 65_536], blocks: &[MemoryBlock]) {
    for block in blocks {
        for (offset, byte) in block.bytes.iter().copied().enumerate() {
            let address = block.start.wrapping_add(offset as u16);
            memory[address as usize] = byte;
        }
    }
}

fn build_expected_memory(input: &FuseInput, expected: &FuseExpected) -> [u8; 65_536] {
    let mut memory = deadbeef_memory();
    apply_memory_blocks(&mut memory, &input.memory);
    apply_memory_blocks(&mut memory, &expected.memory);
    memory
}

fn run_fuse_case(input: &FuseInput) -> FuseObserved {
    let mut z80 = Z80::new();
    let mut memory = deadbeef_memory();
    apply_memory_blocks(&mut memory, &input.memory);

    z80.regs.af = input.af;
    z80.regs.bc = input.bc;
    z80.regs.de = input.de;
    z80.regs.hl = input.hl;
    z80.regs.af_alt = input.af_alt;
    z80.regs.bc_alt = input.bc_alt;
    z80.regs.de_alt = input.de_alt;
    z80.regs.hl_alt = input.hl_alt;
    z80.regs.ix = input.ix;
    z80.regs.iy = input.iy;
    z80.regs.sp = input.sp;
    z80.regs.pc = input.pc;
    z80.regs.wz = input.wz;
    z80.regs.i = input.i;
    z80.regs.r = input.r;
    z80.regs.iff1 = input.iff1;
    z80.regs.iff2 = input.iff2;
    z80.regs.im = input.im;
    z80.halt = input.halted;
    z80.data_in = 0;
    z80.wait = false;
    z80.irq = false;
    z80.nmi = false;

    let mut half_cycles = 0u32;
    let max_half_cycles = input.requested_tstates.saturating_add(64).saturating_mul(2);

    while (half_cycles / 2) < input.requested_tstates || !at_instruction_boundary(&z80) {
        z80.tick();

        if z80.mreq && z80.rd {
            z80.data_in = memory[z80.addr as usize];
        } else if z80.mreq && z80.wr {
            memory[z80.addr as usize] = z80.data;
        } else if z80.iorq && z80.rd && !z80.m1 {
            z80.data_in = (z80.addr >> 8) as u8;
        } else if z80.iorq && z80.m1 {
            z80.data_in = 0xFF;
        }

        half_cycles += 1;

        assert!(
            half_cycles <= max_half_cycles,
            "FUSE case '{}' exceeded safety budget: {} half-cycles for requested {} T-states",
            input.name,
            half_cycles,
            input.requested_tstates
        );
    }

    FuseObserved {
        af: z80.regs.af,
        bc: z80.regs.bc,
        de: z80.regs.de,
        hl: z80.regs.hl,
        af_alt: z80.regs.af_alt,
        bc_alt: z80.regs.bc_alt,
        de_alt: z80.regs.de_alt,
        hl_alt: z80.regs.hl_alt,
        ix: z80.regs.ix,
        iy: z80.regs.iy,
        sp: z80.regs.sp,
        pc: z80.regs.pc,
        wz: z80.regs.wz,
        i: z80.regs.i,
        r: z80.regs.r,
        iff1: z80.regs.iff1,
        iff2: z80.regs.iff2,
        im: z80.regs.im,
        halted: z80.halt,
        final_tstates: half_cycles / 2,
        memory,
    }
}

fn at_instruction_boundary(z80: &Z80) -> bool {
    matches!(z80.phase, Phase::M1(M1Phase::T1Rise))
        && z80.walker.prefix == Prefix::None
        && z80.walker.instruction_complete
        && !z80.walker.ddcb_fetch_phase
}

fn selected_case_name() -> Option<String> {
    match std::env::var(FUSE_CASE_ENV) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

fn selected_case_limit() -> Option<usize> {
    match std::env::var(FUSE_LIMIT_ENV) {
        Ok(value) => Some(value.parse().unwrap_or_else(|error| {
            panic!("failed to parse {FUSE_LIMIT_ENV}='{value}' as usize: {error}")
        })),
        Err(_) => None,
    }
}

fn accepted_disagreement_labels(case_name: &str) -> Option<&'static [&'static str]> {
    ACCEPTED_FUSE_DISAGREEMENTS
        .iter()
        .find_map(|(name, labels)| (*name == case_name).then_some(*labels))
}

fn mismatch_labels(errors: &[String]) -> Vec<&str> {
    errors
        .iter()
        .map(|error| {
            error
                .split(':')
                .next()
                .unwrap_or_else(|| panic!("malformed mismatch line: {error}"))
        })
        .collect()
}

#[test]
fn parse_fuse_inline_sample() {
    let inputs = parse_tests_in(
        "00\n0000 0000 0000 0000 0000 0000 0000 0000 0000 0000 0000 0000 0000\n00 00 0 0 0 0 1\n0000 00 -1\n-1\n",
    );
    let expected = parse_tests_expected(
        "00\n    0 MC 0000\n    4 MR 0000 00\n0000 0000 0000 0000 0000 0000 0000 0000 0000 0000 0000 0001 0000\n00 01 0 0 0 0 4\n\n",
    );

    assert_eq!(inputs.len(), 1);
    assert_eq!(expected.len(), 1);
    assert_eq!(inputs[0].name, "00");
    assert_eq!(expected[0].events.len(), 2);
    assert_eq!(expected[0].final_tstates, 4);
}

#[test]
#[ignore = "requires local FUSE Z80 fixtures"]
fn run_fuse_z80_reference_suite() {
    let fixture_dir = match find_fuse_z80_tests_dir() {
        Ok(path) => path,
        Err(message) => panic!("{message}"),
    };

    let inputs_data =
        std::fs::read_to_string(fixture_dir.join("tests.in")).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {error}",
                fixture_dir.join("tests.in").display()
            )
        });
    let expected_data =
        std::fs::read_to_string(fixture_dir.join("tests.expected")).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {error}",
                fixture_dir.join("tests.expected").display()
            )
        });

    let mut inputs = parse_tests_in(&inputs_data);
    let mut expected = parse_tests_expected(&expected_data);
    assert_eq!(
        inputs.len(),
        expected.len(),
        "input and expected counts differ"
    );

    if let Some(case_name) = selected_case_name() {
        inputs.retain(|case| case.name == case_name);
        expected.retain(|case| case.name == case_name);
        assert!(
            !inputs.is_empty(),
            "no FUSE case named '{case_name}' was found"
        );
    }

    if let Some(limit) = selected_case_limit() {
        inputs.truncate(limit);
        expected.truncate(limit);
    }

    let expected_accepted = inputs
        .iter()
        .filter(|case| accepted_disagreement_labels(&case.name).is_some())
        .count();

    let mut pass = 0usize;
    let mut accepted = 0usize;
    let mut fail = 0usize;
    let mut failures = Vec::new();

    for (input, expected_case) in inputs.iter().zip(expected.iter()) {
        assert_eq!(input.name, expected_case.name, "case name mismatch");
        let observed = run_fuse_case(input);
        let expected_memory = build_expected_memory(input, expected_case);

        let mut errors = Vec::new();
        macro_rules! check {
            ($label:literal, $actual:expr, $expected:expr) => {
                if $actual != $expected {
                    errors.push(format!(
                        "{}: got {:#06x}, expected {:#06x}",
                        $label, $actual, $expected
                    ));
                }
            };
        }

        check!("AF", observed.af, expected_case.af);
        check!("BC", observed.bc, expected_case.bc);
        check!("DE", observed.de, expected_case.de);
        check!("HL", observed.hl, expected_case.hl);
        check!("AF'", observed.af_alt, expected_case.af_alt);
        check!("BC'", observed.bc_alt, expected_case.bc_alt);
        check!("DE'", observed.de_alt, expected_case.de_alt);
        check!("HL'", observed.hl_alt, expected_case.hl_alt);
        check!("IX", observed.ix, expected_case.ix);
        check!("IY", observed.iy, expected_case.iy);
        check!("SP", observed.sp, expected_case.sp);
        check!("PC", observed.pc, expected_case.pc);
        check!("WZ", observed.wz, expected_case.wz);
        check!("I", observed.i, expected_case.i);
        check!("R", observed.r, expected_case.r);
        check!(
            "IFF1",
            u8::from(observed.iff1),
            u8::from(expected_case.iff1)
        );
        check!(
            "IFF2",
            u8::from(observed.iff2),
            u8::from(expected_case.iff2)
        );
        check!("IM", observed.im, expected_case.im);
        check!(
            "HALT",
            u8::from(observed.halted),
            u8::from(expected_case.halted)
        );
        check!(
            "TSTATES",
            observed.final_tstates,
            expected_case.final_tstates
        );

        if observed.memory != expected_memory {
            let mut mismatches = Vec::new();
            for address in 0u16..=u16::MAX {
                let actual = observed.memory[address as usize];
                let expected_byte = expected_memory[address as usize];
                if actual != expected_byte {
                    mismatches.push(format!(
                        "{address:#06x}: got {actual:#04x}, expected {expected_byte:#04x}"
                    ));
                    if mismatches.len() == 8 {
                        break;
                    }
                }
            }
            errors.push(format!("memory: {}", mismatches.join(", ")));
        }

        if errors.is_empty() {
            pass += 1;
        } else if let Some(expected_labels) = accepted_disagreement_labels(&input.name) {
            let actual_labels = mismatch_labels(&errors);
            if actual_labels == expected_labels {
                accepted += 1;
            } else {
                fail += 1;
                if failures.len() < 16 {
                    failures.push(format!(
                        "{}: expected accepted mismatch labels {:?}, got {:?}: {}",
                        input.name,
                        expected_labels,
                        actual_labels,
                        errors.join("; ")
                    ));
                }
            }
        } else {
            fail += 1;
            if failures.len() < 16 {
                failures.push(format!("{}: {}", input.name, errors.join("; ")));
            }
        }
    }

    eprintln!(
        "FUSE final-state compatibility: {pass}/{} exact, {accepted} accepted disagreements, {fail} unexpected",
        inputs.len(),
    );
    for failure in &failures {
        eprintln!("  {failure}");
    }

    assert_eq!(fail, 0, "FUSE reported {fail} failures");
    assert_eq!(
        accepted, expected_accepted,
        "accepted FUSE disagreement count changed"
    );
}
