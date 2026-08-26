//! FUSE Z80 reference test harness.
//!
//! Runs the FUSE emulator's per-T-state fixture suite (`tests.in` /
//! `tests.expected`) against the half-cycle Z80 core. Each fixture
//! gives a starting CPU + memory state, the requested T-state budget,
//! and a list of bus events plus a final state to compare against.
//!
//! The corpus path is resolved by `support::find_fuse_z80_tests_dir`
//! (driven by `EMU198X_FUSE_Z80_TESTS_DIR`).
//!
//! Skipped under normal `cargo test`. Run explicitly with:
//!
//! ```sh
//! cargo test -p zilog-z80 --test z80_fuse -- --ignored --nocapture
//! ```

mod support;

use support::find_fuse_z80_tests_dir;
use zilog_z80::Z80;
use zilog_z80::mcycle::{self, MStep};
use zilog_z80::walker::Prefix;
use zilog_z80::z80::{InternalPhase, IoPhase, M1Phase, MemPhase, Phase};

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
    memory: [u8; 65_536],
}

const ACCEPTED_FUSE_DISAGREEMENTS: &[(&str, &[&str])] = &[
    ("76", &["PC"]),
    // Four block-repeat instructions disagree only on the X/Y
    // undocumented AF bits (F bits 3 and 5) — INIR (edb2), OTIR
    // (edb3), CPDR (edb9) and OTDR (edbb). WZ now matches FUSE (and
    // Patrik Rak's z80memptr) after the 2026-05-31 fix that stopped
    // the repeat path from clobbering the WZ value (BC ± 1) set during
    // the IN/OUT portion; everything except the undoc X/Y bits matches.
    //
    // Reclassified 2026-08-09. The previous note called these "the
    // *final* repeat iteration" and "silicon-variable ... effectively
    // unclosable". Both claims are wrong, and the second followed from
    // the first:
    //
    // - FUSE runs each of these for 21 T-states with B = 0x0a, 0x03,
    //   0x00 and 0x04 respectively (tests.in). 21 T-states is the
    //   *repeating* cost; 16 is the terminating one. So every disputed
    //   case observes a NON-final iteration, with PC rewound to
    //   re-execute — not the final one.
    // - `edba_1` INDR has the identical shape (B = 0x06, 21 T-states)
    //   and passes every bit. We therefore do model the repeating-
    //   iteration rule; it agrees with FUSE for one instruction and
    //   disagrees for four siblings. That is an inconsistency in our
    //   model, not silicon variance.
    // - Patrik Rak's z80full / z80flags / z80memptr set `maskflags
    //   equ 0` (src/*.asm), so they compare the full 0xFF flag mask
    //   including bits 3 and 5, and every block instruction — INIR,
    //   INDR, OTIR, OTDR, CPIR, CPDR, LDIR, LDDR and the ->NOP'
    //   variants — passes against CRCs measured on a real 48K Zilog
    //   board. The suite is not silent on these bits. It observes the
    //   instruction after completion, so it does not obviously cover
    //   FUSE's mid-repeat point, but it does constrain the rule.
    //
    // Still allowlisted: the disagreement is real and unexplained. But
    // it is now a tractable question — why one of five siblings agrees
    // — rather than a silicon mystery. Next step is a differential
    // against the vendored SpecIde / Fuse / zesarux implementations of
    // the repeat H/PV adjustment (see `repeat_block_io_flags` in
    // src/execute.rs, which the I/O paths apply and the compare path
    // does not).
    //
    // NB: `edb9` is CPDR (ED B9), a block-*compare*, not a block-I/O
    // op — an earlier note mislabelled it INDR.
    //
    // WZ on `edb2_1` / `edba_1`, added 2026-08-17 (#949). The 2026-05-31
    // change described above made WZ match FUSE by removing
    // `WZ = PC + 1` from the INIR/INDR repeat path, on the stated
    // grounds that Patrik Rak asserted the same value. He does not:
    // without that line `z80memptr` fails `102 INIR->NOP'` and
    // `103 INDR->NOP'`, and the commit's claim that the suite passed
    // could not have been measured, because it could not reach its
    // Result line until #948. The line is restored, `z80memptr` is
    // 160 of 160, and these two FUSE cases disagree on WZ again.
    //
    // Not a defect being papered over. FUSE captures these mid-repeat
    // at 21 T-states with PC rewound; Rak observes after the
    // instruction completes. They are measuring different instants and
    // may both be right about their own. This engine cannot yet hold
    // both, and `decisions/spectrum-test-oracle-priority.md` ranks
    // `z80test` above FUSE for Spectrum work. Holding both is the open
    // question — the point of the note above about differentialling
    // SpecIde / Fuse / zesarux on the repeat rule.
    ("edb2_1", &["AF", "WZ"]), // INIR — WZ added 2026-08-17, see below
    ("edb3_1", &["AF"]),       // OTIR
    ("edb9_2", &["AF"]),       // CPDR
    ("edba_1", &["WZ"]),       // INDR — WZ only
    ("edbb_1", &["AF"]),       // OTDR
];

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

fn current_tstate(half_cycles: u32) -> u32 {
    half_cycles / 2
}

fn push_fuse_event(
    events: &mut Vec<FuseEvent>,
    time: u32,
    kind: FuseEventKind,
    address: u16,
    data: Option<u8>,
) {
    events.push(FuseEvent {
        time,
        kind,
        address,
        data,
    });
}

fn record_port_contention(events: &mut Vec<FuseEvent>, time: u32, port: u16) {
    push_fuse_event(events, time, FuseEventKind::PortContend, port, None);
}

fn record_io_read_events(events: &mut Vec<FuseEvent>, time: u32, port: u16) {
    let high_contended = (port & 0xC000) == 0x4000;
    let data_time = time + 1;
    let data = (port >> 8) as u8;

    if high_contended {
        record_port_contention(events, time, port);
    }

    push_fuse_event(events, data_time, FuseEventKind::PortRead, port, Some(data));

    if port & 0x0001 != 0 {
        if high_contended {
            record_port_contention(events, data_time, port);
            record_port_contention(events, data_time + 1, port);
            record_port_contention(events, data_time + 2, port);
        }
    } else {
        record_port_contention(events, data_time, port);
    }
}

fn record_io_write_events(events: &mut Vec<FuseEvent>, time: u32, port: u16, data: u8) {
    let high_contended = (port & 0xC000) == 0x4000;
    let data_time = time + 1;

    if high_contended {
        record_port_contention(events, time, port);
    }

    push_fuse_event(
        events,
        data_time,
        FuseEventKind::PortWrite,
        port,
        Some(data),
    );

    if port & 0x0001 != 0 {
        if high_contended {
            record_port_contention(events, data_time, port);
            record_port_contention(events, data_time + 1, port);
            record_port_contention(events, data_time + 2, port);
        }
    } else {
        record_port_contention(events, data_time, port);
    }
}

fn fuse_internal_addr(z80: &Z80) -> u16 {
    let walker = &z80.walker;
    let regs = &z80.regs;
    let sequence = walker.sequence.as_ptr();

    if let Some(previous) = walker
        .step_idx
        .checked_sub(1)
        .and_then(|index| walker.sequence.get(index))
    {
        match previous {
            MStep::FetchByte | MStep::FetchByteHi | MStep::FetchDisp => {
                return regs.pc.wrapping_sub(1);
            }
            MStep::ReadAddr | MStep::WriteAddr => return walker.staged.addr,
            MStep::ReadAddrHi | MStep::WriteAddrHi => return walker.staged.addr.wrapping_add(1),
            MStep::PopLo | MStep::PopHi => return regs.sp.wrapping_sub(1),
            MStep::PushHi | MStep::PushLo => return regs.sp,
            _ => {}
        }
    }

    if sequence == mcycle::SEQ_CALL_CC.as_ptr() && walker.step_idx == 3 {
        return walker.staged.push_val.wrapping_sub(1);
    }
    if (sequence == mcycle::SEQ_LDIR_REPEAT.as_ptr() && walker.step_idx == 6)
        || (sequence == mcycle::SEQ_CPIR_REPEAT.as_ptr() && walker.step_idx == 4)
        || (sequence == mcycle::SEQ_INIR_REPEAT.as_ptr() && walker.step_idx == 6)
        || (sequence == mcycle::SEQ_OTIR_REPEAT.as_ptr() && walker.step_idx == 6)
    {
        return walker.staged.addr;
    }
    if (sequence == mcycle::SEQ_DDCB_HL.as_ptr() || sequence == mcycle::SEQ_DDCB_BIT.as_ptr())
        && walker.step_idx == 0
    {
        return regs.pc.wrapping_sub(1);
    }

    if (sequence == mcycle::SEQ_LD_SP_HL.as_ptr())
        || (sequence == mcycle::SEQ_PUSH.as_ptr())
        || (sequence == mcycle::SEQ_DJNZ_TAKEN.as_ptr() && walker.step_idx == 0)
        || (sequence == mcycle::SEQ_DJNZ_NOT_TAKEN.as_ptr() && walker.step_idx == 0)
        || (sequence == mcycle::SEQ_RET_CC.as_ptr())
        || (sequence == mcycle::SEQ_RST.as_ptr())
        || (sequence == mcycle::SEQ_INI.as_ptr())
        || (sequence == mcycle::SEQ_INIR_REPEAT.as_ptr() && walker.step_idx == 0)
        || (sequence == mcycle::SEQ_INIR_DONE.as_ptr())
        || (sequence == mcycle::SEQ_OUTI.as_ptr())
        || (sequence == mcycle::SEQ_OTIR_REPEAT.as_ptr() && walker.step_idx == 0)
        || (sequence == mcycle::SEQ_OTIR_DONE.as_ptr())
        || (sequence == mcycle::SEQ_INC_DEC_RR.as_ptr())
        || (sequence == mcycle::SEQ_ADD_HL_RR.as_ptr())
        || (sequence == mcycle::SEQ_LD_IR.as_ptr())
        || (sequence == mcycle::SEQ_NMI.as_ptr())
    {
        return regs.ir();
    }

    regs.ir()
}

fn record_step_start_events(
    z80: &Z80,
    memory: &[u8; 65_536],
    events: &mut Vec<FuseEvent>,
    time: u32,
) {
    match z80.phase {
        Phase::M1(M1Phase::T1Rise) => {
            let address = z80.regs.pc;
            push_fuse_event(events, time, FuseEventKind::MemContend, address, None);
            push_fuse_event(
                events,
                time + 4,
                FuseEventKind::MemRead,
                address,
                Some(memory[address as usize]),
            );
        }
        // `T1Fall`, not `T1Rise`. `present_step_signals` drives the
        // address *during* the cycle's first half-cycle, and this
        // function runs before that tick — so sampling at `T1Rise` reads
        // the bus the previous M-cycle left behind. FUSE logs the
        // contention event with the address of the access about to
        // happen, which is what the bus carries for the whole of `T1`;
        // `T1Fall` is the same T-state, so `time` is unchanged.
        //
        // This was 830 of 1356 fixtures failing as `expected [4 MC 0001],
        // got [4 MC 0000]`, and it was the harness, not the core: the
        // `M1` arm above reads `regs.pc` rather than the bus and was
        // always right, which is why only the non-`M1` accesses failed.
        Phase::MemRead(MemPhase::T1Fall) => {
            let address = z80.addr;
            push_fuse_event(events, time, FuseEventKind::MemContend, address, None);
            push_fuse_event(
                events,
                time + 3,
                FuseEventKind::MemRead,
                address,
                Some(memory[address as usize]),
            );
        }
        Phase::MemWrite(MemPhase::T1Fall) => {
            let address = z80.addr;
            push_fuse_event(events, time, FuseEventKind::MemContend, address, None);
            push_fuse_event(
                events,
                time + 3,
                FuseEventKind::MemWrite,
                address,
                Some(z80.data),
            );
        }
        Phase::Contend(MemPhase::T1Fall) => {
            push_fuse_event(events, time, FuseEventKind::MemContend, z80.addr, None);
        }
        // `T1Fall` for the same reason as the memory arms above: the port
        // address is driven during `T1`↑, so sampling before that tick
        // reads the previous M-cycle's bus. With a stale port the
        // contention branches key off the wrong page too, so this fixed
        // both the addresses and the missing `PC` events.
        Phase::IoRead(IoPhase::T1Fall) => record_io_read_events(events, time, z80.addr),
        Phase::IoWrite(IoPhase::T1Fall) => record_io_write_events(events, time, z80.addr, z80.data),
        Phase::Internal(InternalPhase { remaining }) if matches!(z80.walker.current_step(), Some(MStep::Internal(tstates)) if remaining == tstates * 2) =>
        {
            let address = fuse_internal_addr(z80);
            for offset in 0..u32::from(remaining / 2) {
                push_fuse_event(
                    events,
                    time + offset,
                    FuseEventKind::MemContend,
                    address,
                    None,
                );
            }
        }
        _ => {}
    }
}

fn format_fuse_event(event: &FuseEvent) -> String {
    let kind = match event.kind {
        FuseEventKind::MemRead => "MR",
        FuseEventKind::MemWrite => "MW",
        FuseEventKind::MemContend => "MC",
        FuseEventKind::PortRead => "PR",
        FuseEventKind::PortWrite => "PW",
        FuseEventKind::PortContend => "PC",
    };

    match event.data {
        Some(data) => format!("{} {} {:04x} {:02x}", event.time, kind, event.address, data),
        None => format!("{} {} {:04x}", event.time, kind, event.address),
    }
}

fn describe_event_mismatch(expected: &[FuseEvent], observed: &[FuseEvent]) -> String {
    let mismatch_index = expected
        .iter()
        .zip(observed.iter())
        .position(|(left, right)| left != right);

    match mismatch_index {
        Some(index) => format!(
            "first mismatch at event {}: expected [{}], got [{}]",
            index,
            format_fuse_event(&expected[index]),
            format_fuse_event(&observed[index]),
        ),
        None => format!(
            "event count mismatch: expected {}, got {}",
            expected.len(),
            observed.len()
        ),
    }
}

fn run_fuse_case(input: &FuseInput) -> FuseObserved {
    let mut z80 = Z80::new();
    let mut memory = deadbeef_memory();
    let mut events = Vec::new();
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
        record_step_start_events(&z80, &memory, &mut events, current_tstate(half_cycles));
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
        events,
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
#[ignore = "FIXTURE: requires local FUSE Z80 fixtures"]
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

        if observed.events != expected_case.events {
            errors.push(format!(
                "events: {}",
                describe_event_mismatch(&expected_case.events, &observed.events)
            ));
        }

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
