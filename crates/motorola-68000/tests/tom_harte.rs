//! SingleStepTests 680x0 single-step test harness.
//!
//! Runs the registered SingleStepTests/680x0 fixture suite from a local root
//! such as:
//! - `assets/test-suites/processor-tests/680x0/68000/v1/*.json.gz`
//! - `~/Projects/Emu198x-Unclean/680x0/68000/v1/*.json.gz`
//! - `~/Projects/Emu198x-archive/test-data/680x0/68000/v1/*.json.gz`
//!
//! against the pin-level CPU core.
//!
//! Each fixture file contains 8,065 single-instruction tests for
//! one opcode group. Each test gives CPU + memory state
//! before and after execution. The harness sets up the CPU from
//! the `initial` block, ticks it until the next instruction starts,
//! and compares against the `final` block.
//!
//! The comparison covers D0-D7, A0-A6, USP, SSP, SR, the next
//! instruction address, and listed final RAM bytes. It does not compare
//! final prefetch words, cycle length, or ordered bus transactions.
//!
//! Skipped under normal `cargo test`. Run explicitly with:
//!
//! ```sh
//! cargo test --release -p motorola-68000 --test tom_harte harte_full_sweep -- --include-ignored --nocapture
//! ```
//!
//! The default test (`harte_smoke`) runs a small curated subset of
//! opcodes that are load-bearing for the Amiga graphics.library
//! init code path we're investigating. The full sweep
//! (`harte_full_sweep`) runs the registered 124-file corpus. That
//! corpus contains 1,000,060 rows. Two named invalid rows are excluded.
//! Of the remaining 1,000,058 rows, 968,687 agree exactly. Another 3,401
//! PC-relative address-error rows and 27,970 instruction-fetch address-error
//! rows are accepted only as narrowly classified software-oracle divergences.
//! Retained transaction and exception-frame structure must identify the
//! relevant address error, and the disputed access-information bits must be
//! the sole final-state difference.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde_json::Value;

use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode, interrupt_acknowledge_level};
use motorola_68000::cpu::State;

const EXPECTED_FULL_SWEEP_FIXTURES: usize = 124;
const EXPECTED_FULL_SWEEP_ROWS: usize = 1_000_060;
const EXPECTED_INVALID_ROWS: usize = 2;
const EXPECTED_COMPARED_ROWS: usize = 1_000_058;
const EXPECTED_EXACT_AGREEMENT_ROWS: usize = 968_687;
const EXPECTED_PC_RELATIVE_AE_DIVERGENCES: usize = 3_401;
const EXPECTED_PC_DISPLACEMENT_AE_DIVERGENCES: usize = 1_691;
const EXPECTED_PC_INDEXED_AE_DIVERGENCES: usize = 1_710;
const EXPECTED_INSTRUCTION_FETCH_AE_IN_DIVERGENCES: usize = 27_970;
const EXPECTED_INSTRUCTION_FETCH_AE_IN_BY_KIND: [usize; 8] =
    [2_200, 3_980, 1_964, 3_806, 3_882, 4_057, 4_054, 4_027];
const MASK_24: u32 = 0x00FF_FFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstructionFetchAddressErrorKind {
    Bcc,
    Bsr,
    Dbcc,
    Jmp,
    Jsr,
    Rts,
    Rte,
    Rtr,
}

impl InstructionFetchAddressErrorKind {
    const fn index(self) -> usize {
        match self {
            Self::Bcc => 0,
            Self::Bsr => 1,
            Self::Dbcc => 2,
            Self::Jmp => 3,
            Self::Jsr => 4,
            Self::Rts => 5,
            Self::Rte => 6,
            Self::Rtr => 7,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompatibilityFingerprint(u64);

impl Default for CompatibilityFingerprint {
    fn default() -> Self {
        Self(0xCBF2_9CE4_8422_2325)
    }
}

impl CompatibilityFingerprint {
    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_case(&mut self, case_name: &str, class: u8) {
        self.write_u64(case_name.len() as u64);
        self.write_bytes(case_name.as_bytes());
        self.write_u8(class);
    }
}

const EXPECTED_COMPATIBILITY_FINGERPRINT: CompatibilityFingerprint =
    CompatibilityFingerprint(0x52FB_9713_C00A_B6AE);

// ─── Sparse memory ────────────────────────────────────────────────

/// Sparse byte memory for the 24-bit address space the 68000 sees.
///
/// SingleStepTests fixtures only touch a handful of bytes per test, so
/// `HashMap<u32,u8>` is the obvious representation. Unset locations
/// read as zero; the fixture provides initial values for anything
/// the instruction reads.
struct SparseMem {
    bytes: HashMap<u32, u8>,
}

impl SparseMem {
    fn new() -> Self {
        Self {
            bytes: HashMap::new(),
        }
    }

    fn load_ram(&mut self, ram: &[Value]) {
        for entry in ram {
            let pair = entry.as_array().expect("ram entry is array");
            let addr = pair[0].as_u64().expect("ram addr") as u32;
            let value = pair[1].as_u64().expect("ram value") as u8;
            self.bytes.insert(addr & 0xFF_FFFF, value);
        }
    }

    fn read_byte(&self, addr: u32) -> u8 {
        *self.bytes.get(&(addr & 0xFF_FFFF)).unwrap_or(&0)
    }

    fn read_word(&self, addr: u32) -> u16 {
        let a = addr & 0xFF_FFFE;
        (u16::from(self.read_byte(a)) << 8) | u16::from(self.read_byte(a + 1))
    }

    fn write_byte(&mut self, addr: u32, val: u8) {
        self.bytes.insert(addr & 0xFF_FFFF, val);
    }

    fn write_word(&mut self, addr: u32, val: u16) {
        let a = addr & 0xFF_FFFE;
        self.write_byte(a, (val >> 8) as u8);
        self.write_byte(a + 1, val as u8);
    }
}

// ─── Fixture loading ──────────────────────────────────────────────

fn candidate_fixture_roots() -> Vec<PathBuf> {
    let home = std::env::var("HOME").expect("HOME set");
    let home = PathBuf::from(home);

    let mut roots = Vec::new();
    if let Ok(path) = std::env::var("EMU198X_68000_TOM_HARTE_ROOT") {
        roots.push(PathBuf::from(path));
    }
    roots.push(home.join("Projects/198x/assets/test-suites/processor-tests/680x0/68000/v1"));
    roots.push(home.join("Projects/Emu198x-Unclean/680x0/68000/v1"));
    roots.push(home.join("Projects/Emu198x-archive/test-data/680x0/68000/v1"));
    roots
}

fn fixture_root() -> PathBuf {
    if let Ok(path) = std::env::var("EMU198X_68000_TOM_HARTE_ROOT") {
        return PathBuf::from(path);
    }

    candidate_fixture_roots()
        .into_iter()
        .find(|path| path.is_dir())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").expect("HOME set");
            PathBuf::from(home)
                .join("Projects/198x/assets/test-suites/processor-tests/680x0/68000/v1")
        })
}

fn load_fixture(path: &Path) -> Result<Vec<Value>, String> {
    let file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let gz = GzDecoder::new(file);
    let parsed: Value = serde_json::from_reader(gz)
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))?;
    parsed
        .as_array()
        .cloned()
        .ok_or_else(|| format!("fixture root is not an array: {}", path.display()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PcRelativeAddressErrorKind {
    Displacement,
    Indexed,
}

fn idle_transaction(value: &Value, cycles: u64) -> bool {
    let Some(fields) = value.as_array() else {
        return false;
    };
    fields.len() == 2 && fields[0].as_str() == Some("n") && fields[1].as_u64() == Some(cycles)
}

fn word_bus_transaction(value: &Value, kind: &str, fc: u64) -> Option<(u32, u16)> {
    let fields = value.as_array()?;
    if fields.len() != 6
        || fields[0].as_str()? != kind
        || fields[1].as_u64()? != 4
        || fields[2].as_u64()? != fc
        || fields[4].as_str()? != ".w"
    {
        return None;
    }
    Some((
        (fields[3].as_u64()? as u32) & MASK_24,
        fields[5].as_u64()? as u16,
    ))
}

fn fixture_ram_byte(state: &Value, address: u32) -> Option<u8> {
    let wanted = address & MASK_24;
    let mut found = None;
    for item in state.get("ram")?.as_array()? {
        let pair = item.as_array()?;
        if pair.len() != 2 {
            return None;
        }
        if (pair[0].as_u64()? as u32 & MASK_24) == wanted {
            if found.is_some() {
                return None;
            }
            found = Some(pair[1].as_u64()? as u8);
        }
    }
    found
}

fn fixture_ram_word(state: &Value, address: u32) -> Option<u16> {
    Some(
        (u16::from(fixture_ram_byte(state, address)?) << 8)
            | u16::from(fixture_ram_byte(state, address.wrapping_add(1))?),
    )
}

fn supervisor_program_read_at(value: &Value, address: u32) -> bool {
    word_bus_transaction(value, "r", 6).is_some_and(|(actual, _)| actual == (address & MASK_24))
}

fn pc_relative_prefix_matches(prefix: &[Value], pc: u32) -> bool {
    match prefix {
        [a] => supervisor_program_read_at(a, pc.wrapping_add(4)),
        [a, b] if idle_transaction(a, 2) => supervisor_program_read_at(b, pc.wrapping_add(4)),
        [a, b] => {
            supervisor_program_read_at(a, pc.wrapping_add(4))
                && supervisor_program_read_at(b, pc.wrapping_add(6))
        }
        [a, b, c] => {
            supervisor_program_read_at(a, pc.wrapping_add(4))
                && idle_transaction(b, 2)
                && supervisor_program_read_at(c, pc.wrapping_add(6))
        }
        _ => false,
    }
}

/// Identify the older corpus's PC-relative address-error software-oracle
/// boundary from retained transaction and frame structure rather than names.
/// Any malformed or incomplete evidence returns `None` and remains subject to
/// ordinary exact comparison.
fn classify_pc_relative_address_error(test: &Value) -> Option<PcRelativeAddressErrorKind> {
    let test = test.as_object()?;
    let initial = test.get("initial")?.as_object()?;
    let final_state = test.get("final")?;
    let prefetch = initial.get("prefetch")?.as_array()?;
    let opcode = prefetch.first()?.as_u64()? as u16;
    let kind = match opcode & 0x003F {
        0x003A => PcRelativeAddressErrorKind::Displacement,
        0x003B => PcRelativeAddressErrorKind::Indexed,
        _ => return None,
    };

    let initial_sr = initial.get("sr")?.as_u64()? as u16;
    if initial_sr & 0x2000 == 0 {
        return None;
    }
    let initial_ssp = initial.get("ssp")?.as_u64()? as u32;
    let final_ssp = final_state.get("ssp")?.as_u64()? as u32;
    if final_ssp != initial_ssp.wrapping_sub(14) {
        return None;
    }

    let transactions = test.get("transactions")?.as_array()?;
    if !(14..=16).contains(&transactions.len()) {
        return None;
    }
    let split = transactions.len() - 13;
    let (prefix, tail) = transactions.split_at(split);
    let pc = initial.get("pc")?.as_u64()? as u32;
    if !pc_relative_prefix_matches(prefix, pc) || !idle_transaction(&tail[0], 4) {
        return None;
    }

    let frame_offsets = [12u32, 8, 10, 6, 4, 0, 2];
    for (cycle, offset) in tail[1..8].iter().zip(frame_offsets) {
        let (address, data) = word_bus_transaction(cycle, "w", 5)?;
        let expected_address = final_ssp.wrapping_add(offset) & MASK_24;
        if address != expected_address || fixture_ram_word(final_state, address)? != data {
            return None;
        }
    }

    let (vector_high_address, vector_high) = word_bus_transaction(&tail[8], "r", 5)?;
    let (vector_low_address, vector_low) = word_bus_transaction(&tail[9], "r", 5)?;
    if vector_high_address != 12 || vector_low_address != 14 {
        return None;
    }
    let vector = (u32::from(vector_high) << 16) | u32::from(vector_low);
    let (handler_first_address, handler_first) = word_bus_transaction(&tail[10], "r", 6)?;
    if handler_first_address != (vector & MASK_24) || !idle_transaction(&tail[11], 2) {
        return None;
    }
    let (handler_second_address, handler_second) = word_bus_transaction(&tail[12], "r", 6)?;
    if handler_second_address != (vector.wrapping_add(2) & MASK_24) {
        return None;
    }

    if final_state.get("pc")?.as_u64()? as u32 != vector {
        return None;
    }
    let final_prefetch = final_state.get("prefetch")?.as_array()?;
    if final_prefetch.len() != 2
        || final_prefetch[0].as_u64()? as u16 != handler_first
        || final_prefetch[1].as_u64()? as u16 != handler_second
    {
        return None;
    }

    let special_status_word = fixture_ram_word(final_state, final_ssp)?;
    if special_status_word & 0x001F != 0x0015 {
        return None;
    }
    Some(kind)
}

/// Identify an address-error frame caused by an odd program-space read whose
/// retained access-information word marks the processor as not processing an
/// instruction. Motorola defines the opposite I/N value for normal instruction
/// processing, so this class remains a software-oracle compatibility boundary.
fn classify_instruction_fetch_address_error(
    test: &Value,
) -> Option<InstructionFetchAddressErrorKind> {
    let test = test.as_object()?;
    let prefetch = test.get("initial")?.get("prefetch")?.as_array()?;
    let opcode = prefetch.first()?.as_u64()? as u16;
    let kind = if opcode >> 12 == 6 {
        if (opcode >> 8) & 0x0F == 1 {
            InstructionFetchAddressErrorKind::Bsr
        } else {
            InstructionFetchAddressErrorKind::Bcc
        }
    } else if opcode & 0xF0F8 == 0x50C8 {
        InstructionFetchAddressErrorKind::Dbcc
    } else if opcode & 0xFFC0 == 0x4EC0 {
        InstructionFetchAddressErrorKind::Jmp
    } else if opcode & 0xFFC0 == 0x4E80 {
        InstructionFetchAddressErrorKind::Jsr
    } else {
        match opcode {
            0x4E75 => InstructionFetchAddressErrorKind::Rts,
            0x4E73 => InstructionFetchAddressErrorKind::Rte,
            0x4E77 => InstructionFetchAddressErrorKind::Rtr,
            _ => return None,
        }
    };
    let final_state = test.get("final")?;
    let final_ssp = final_state.get("ssp")?.as_u64()? as u32;
    let transactions = test.get("transactions")?.as_array()?;
    if transactions.len() < 13 {
        return None;
    }
    let tail = &transactions[transactions.len() - 13..];
    if !idle_transaction(&tail[0], 4) {
        return None;
    }

    let frame_offsets = [12u32, 8, 10, 6, 4, 0, 2];
    for (cycle, offset) in tail[1..8].iter().zip(frame_offsets) {
        let (address, data) = word_bus_transaction(cycle, "w", 5)?;
        let expected_address = final_ssp.wrapping_add(offset) & MASK_24;
        if address != expected_address || fixture_ram_word(final_state, address)? != data {
            return None;
        }
    }

    let (vector_high_address, vector_high) = word_bus_transaction(&tail[8], "r", 5)?;
    let (vector_low_address, vector_low) = word_bus_transaction(&tail[9], "r", 5)?;
    if vector_high_address != 12 || vector_low_address != 14 {
        return None;
    }
    let vector = (u32::from(vector_high) << 16) | u32::from(vector_low);
    let (handler_first_address, handler_first) = word_bus_transaction(&tail[10], "r", 6)?;
    if handler_first_address != (vector & MASK_24) || !idle_transaction(&tail[11], 2) {
        return None;
    }
    let (handler_second_address, handler_second) = word_bus_transaction(&tail[12], "r", 6)?;
    if handler_second_address != (vector.wrapping_add(2) & MASK_24) {
        return None;
    }

    if final_state.get("pc")?.as_u64()? as u32 != vector {
        return None;
    }
    let final_prefetch = final_state.get("prefetch")?.as_array()?;
    if final_prefetch.len() != 2
        || final_prefetch[0].as_u64()? as u16 != handler_first
        || final_prefetch[1].as_u64()? as u16 != handler_second
    {
        return None;
    }

    let special_status_word = fixture_ram_word(final_state, final_ssp)?;
    let function_code = special_status_word & 0x0007;
    if special_status_word & 0x0018 != 0x0018 || !matches!(function_code, 2 | 6) {
        return None;
    }
    let instruction_register = fixture_ram_word(final_state, final_ssp.wrapping_add(6))?;
    if special_status_word & 0xFFE0 != instruction_register & 0xFFE0 {
        return None;
    }
    let fault_address = (u32::from(fixture_ram_word(final_state, final_ssp.wrapping_add(2))?)
        << 16)
        | u32::from(fixture_ram_word(final_state, final_ssp.wrapping_add(4))?);
    if fault_address & 1 == 0 {
        return None;
    }

    Some(kind)
}

// ─── CPU state setup from fixture ─────────────────────────────────

fn apply_initial(cpu: &mut Cpu68000, mem: &mut SparseMem, initial: &Value) {
    let o = initial.as_object().expect("initial is object");

    for i in 0..8 {
        let k = format!("d{i}");
        cpu.regs.d[i] = o[&k].as_u64().expect("dN") as u32;
    }
    for i in 0..7 {
        let k = format!("a{i}");
        cpu.regs.a[i] = o[&k].as_u64().expect("aN") as u32;
    }
    cpu.regs.usp = o["usp"].as_u64().expect("usp") as u32;
    cpu.regs.ssp = o["ssp"].as_u64().expect("ssp") as u32;
    let pc = o["pc"].as_u64().expect("pc") as u32;
    cpu.regs.sr = o["sr"].as_u64().expect("sr") as u16;

    let pq = o["prefetch"].as_array().expect("prefetch array");
    let pf0 = pq[0].as_u64().expect("prefetch0") as u16;
    let pf1 = pq[1].as_u64().expect("prefetch1") as u16;

    // Put both prefetch words into memory so FetchIRC can read them.
    mem.write_word(pc, pf0);
    mem.write_word(pc.wrapping_add(2), pf1);

    // Use the CPU's own setup_prefetch(), which correctly initialises
    // all pipeline registers (IR, IRC, irc_addr, next_fetch_addr,
    // instr_start_pc) and directly queues Execute — bypassing the
    // PromoteIRC step and getting the prefetch state exactly right.
    //
    // setup_prefetch expects regs.pc to already point past both the
    // opcode word and the IRC word: pc = fixture_pc + 4.
    cpu.regs.pc = pc.wrapping_add(4);
    cpu.setup_prefetch(pf0, pf1);
}

// ─── Bus service (same pattern as pin_level.rs) ────────────────────

fn service_bus(cpu: &mut Cpu68000, mem: &mut SparseMem) {
    if let State::BusCycle {
        addr,
        fc,
        is_read,
        is_word,
        data,
        cycle_count,
        ..
    } = &cpu.state
    {
        if *cycle_count >= 3 {
            if *fc == FunctionCode::InterruptAck {
                let acknowledged_level = interrupt_acknowledge_level(*addr);
                cpu.bus_status = BusStatus::Ready(24 + u16::from(acknowledged_level));
            } else if *is_read {
                let val = if *is_word {
                    mem.read_word(*addr)
                } else {
                    u16::from(mem.read_byte(*addr))
                };
                cpu.bus_status = BusStatus::Ready(val);
            } else {
                let val = data.unwrap_or(0);
                if *is_word {
                    mem.write_word(*addr, val);
                } else {
                    mem.write_byte(*addr, val as u8);
                }
                cpu.bus_status = BusStatus::Ready(0);
            }
        } else {
            cpu.bus_status = BusStatus::Wait;
        }
    } else {
        cpu.bus_status = BusStatus::Wait;
    }
}

// ─── Run exactly one instruction ───────────────────────────────────

/// Run the CPU until the fixture instruction has completed.
///
/// Setup calls `setup_prefetch`, which puts `prefetch[0]` in IR,
/// `prefetch[1]` in IRC, queues `Execute`, and sets
/// `instruction_starts = 1`. The fixture instruction then executes.
/// Completion queues and promotes the next instruction, incrementing
/// `instruction_starts`. The harness stops when that counter increases.
/// A wide tick bound allows even long instructions such as TRAP and
/// MOVEM to finish.
fn run_one_instruction(cpu: &mut Cpu68000, mem: &mut SparseMem, _fixture_pc: u32) -> bool {
    let start_count = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(cpu, mem);
        cpu.tick();
        if cpu.instruction_starts > start_count {
            return true;
        }
    }
    false
}

// ─── State comparison ─────────────────────────────────────────────

#[derive(Default, Debug)]
struct Mismatch {
    field: String,
    expected: String,
    actual: String,
}

fn compare_final(cpu: &Cpu68000, mem: &SparseMem, final_state: &Value) -> Vec<Mismatch> {
    let mut v = Vec::new();
    let o = final_state.as_object().expect("final is object");

    for i in 0..8 {
        let k = format!("d{i}");
        let expected = o[&k].as_u64().unwrap() as u32;
        if cpu.regs.d[i] != expected {
            v.push(Mismatch {
                field: k,
                expected: format!("${expected:08X}"),
                actual: format!("${:08X}", cpu.regs.d[i]),
            });
        }
    }
    for i in 0..7 {
        let k = format!("a{i}");
        let expected = o[&k].as_u64().unwrap() as u32;
        if cpu.regs.a[i] != expected {
            v.push(Mismatch {
                field: k,
                expected: format!("${expected:08X}"),
                actual: format!("${:08X}", cpu.regs.a[i]),
            });
        }
    }

    let expected_usp = o["usp"].as_u64().unwrap() as u32;
    if cpu.regs.usp != expected_usp {
        v.push(Mismatch {
            field: "usp".into(),
            expected: format!("${expected_usp:08X}"),
            actual: format!("${:08X}", cpu.regs.usp),
        });
    }
    let expected_ssp = o["ssp"].as_u64().unwrap() as u32;
    if cpu.regs.ssp != expected_ssp {
        v.push(Mismatch {
            field: "ssp".into(),
            expected: format!("${expected_ssp:08X}"),
            actual: format!("${:08X}", cpu.regs.ssp),
        });
    }
    // The fixture's "final pc" is the address of the NEXT instruction
    // (= where the prefetched opcode came from). In this CPU's prefetch
    // model that's `instr_start_pc` after the current instruction's
    // completion, NOT `regs.pc` (which is instr_start_pc + 2).
    let expected_pc = o["pc"].as_u64().unwrap() as u32;
    if cpu.instr_start_pc != expected_pc {
        v.push(Mismatch {
            field: "pc".into(),
            expected: format!("${expected_pc:08X}"),
            actual: format!("${:08X}", cpu.instr_start_pc),
        });
    }
    let expected_sr = o["sr"].as_u64().unwrap() as u16;
    if cpu.regs.sr != expected_sr {
        v.push(Mismatch {
            field: "sr".into(),
            expected: format!("${expected_sr:04X}"),
            actual: format!("${:04X}", cpu.regs.sr),
        });
    }

    // Memory.
    for entry in o["ram"].as_array().unwrap() {
        let pair = entry.as_array().unwrap();
        let addr = pair[0].as_u64().unwrap() as u32;
        let expected = pair[1].as_u64().unwrap() as u8;
        let actual = mem.read_byte(addr);
        if actual != expected {
            v.push(Mismatch {
                field: format!("mem[${addr:06X}]"),
                expected: format!("${expected:02X}"),
                actual: format!("${actual:02X}"),
            });
        }
    }

    v
}

// ─── Per-fixture runner ────────────────────────────────────────────

struct FixtureResult {
    name: String,
    total: usize,
    passed: usize,
    accepted_pc_displacement_ae: usize,
    accepted_pc_indexed_ae: usize,
    accepted_instruction_fetch_ae_in: usize,
    accepted_instruction_fetch_ae_in_by_kind: [usize; 8],
    compatibility_fingerprint: CompatibilityFingerprint,
    skipped: usize,
    first_fail: Option<(String, Vec<Mismatch>)>,
}

impl FixtureResult {
    fn accepted_pc_relative_ae(&self) -> usize {
        self.accepted_pc_displacement_ae + self.accepted_pc_indexed_ae
    }

    fn successful(&self) -> usize {
        self.passed + self.accepted_pc_relative_ae() + self.accepted_instruction_fetch_ae_in
    }
}

fn known_invalid_case(case_name: &str) -> Option<&'static str> {
    match case_name {
        // Both of these vectors are for the same byte-sized ASL opcode (E502),
        // but the expected final D2 mutates the upper 24 bits. A 68000 byte
        // operation on Dn can only replace the low byte of that register, and
        // the rest of the ASL.b corpus obeys that rule. Treat these as bad
        // fixture rows rather than bending the core around impossible state.
        "e502 [ASL.b Q, D2] 1583" | "e502 [ASL.b Q, D2] 1761" => Some("invalid ASL.b fixture row"),
        _ => None,
    }
}

fn is_exact_pc_relative_ae_fc_difference(
    mismatches: &[Mismatch],
    memory: &SparseMem,
    final_state: &Value,
) -> bool {
    let Some(stack_pointer) = final_state.get("ssp").and_then(Value::as_u64) else {
        return false;
    };
    let address = (stack_pointer as u32).wrapping_add(1) & MASK_24;
    let Some(expected) = fixture_ram_byte(final_state, address) else {
        return false;
    };
    let actual = memory.read_byte(address);

    mismatches.len() == 1
        && mismatches[0].field == format!("mem[${address:06X}]")
        && expected & 0x1F == 0x15
        && actual & 0x1F == 0x16
        && expected & 0xE0 == actual & 0xE0
}

fn is_exact_instruction_fetch_ae_in_difference(
    mismatches: &[Mismatch],
    memory: &SparseMem,
    final_state: &Value,
) -> bool {
    let Some(stack_pointer) = final_state.get("ssp").and_then(Value::as_u64) else {
        return false;
    };
    let address = (stack_pointer as u32).wrapping_add(1) & MASK_24;
    let Some(expected) = fixture_ram_byte(final_state, address) else {
        return false;
    };
    let actual = memory.read_byte(address);
    let expected_low = expected & 0x1F;

    mismatches.len() == 1
        && mismatches[0].field == format!("mem[${address:06X}]")
        && matches!(expected_low, 0x1A | 0x1E)
        && actual & 0x1F == expected_low & !0x08
        && expected & 0xE0 == actual & 0xE0
}

fn run_fixture(path: &Path) -> Result<FixtureResult, String> {
    let tests = load_fixture(path)?;
    let name = path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .trim_end_matches(".json.gz")
        .to_string();

    let mut passed = 0;
    let mut accepted_pc_displacement_ae = 0;
    let mut accepted_pc_indexed_ae = 0;
    let mut accepted_instruction_fetch_ae_in = 0;
    let mut accepted_instruction_fetch_ae_in_by_kind = [0; 8];
    let mut compatibility_fingerprint = CompatibilityFingerprint::default();
    let mut skipped = 0;
    let mut first_fail: Option<(String, Vec<Mismatch>)> = None;

    for test in &tests {
        let t = test.as_object().unwrap();
        let case_name = t["name"].as_str().unwrap_or("?").to_string();
        if known_invalid_case(&case_name).is_some() {
            skipped += 1;
            continue;
        }
        let initial = &t["initial"];
        let final_state = &t["final"];
        let pc_relative_address_error = classify_pc_relative_address_error(test);
        let instruction_fetch_address_error = classify_instruction_fetch_address_error(test);

        let mut cpu = Cpu68000::new();
        let mut mem = SparseMem::new();
        mem.load_ram(initial["ram"].as_array().unwrap());
        apply_initial(&mut cpu, &mut mem, initial);
        let fixture_pc = initial["pc"].as_u64().unwrap() as u32;

        let ran = run_one_instruction(&mut cpu, &mut mem, fixture_pc);
        if !ran {
            if first_fail.is_none() {
                first_fail = Some((
                    case_name,
                    vec![Mismatch {
                        field: "run".into(),
                        expected: "1 instruction".into(),
                        actual: "timeout".into(),
                    }],
                ));
            }
            continue;
        }

        let mismatches = compare_final(&cpu, &mem, final_state);
        match pc_relative_address_error {
            Some(kind) if is_exact_pc_relative_ae_fc_difference(&mismatches, &mem, final_state) => {
                match kind {
                    PcRelativeAddressErrorKind::Displacement => {
                        accepted_pc_displacement_ae += 1;
                        compatibility_fingerprint.write_case(&case_name, 0);
                    }
                    PcRelativeAddressErrorKind::Indexed => {
                        accepted_pc_indexed_ae += 1;
                        compatibility_fingerprint.write_case(&case_name, 1);
                    }
                }
            }
            Some(_) if mismatches.is_empty() && first_fail.is_none() => {
                first_fail = Some((
                    case_name,
                    vec![Mismatch {
                        field: "PC-relative address-error SSW".into(),
                        expected: "sole FC5-to-FC6 divergence".into(),
                        actual: "no divergence".into(),
                    }],
                ));
            }
            Some(_) if mismatches.is_empty() => {}
            _ if instruction_fetch_address_error.is_some()
                && is_exact_instruction_fetch_ae_in_difference(&mismatches, &mem, final_state) =>
            {
                accepted_instruction_fetch_ae_in += 1;
                let kind = instruction_fetch_address_error
                    .expect("guard requires an instruction-fetch classification");
                accepted_instruction_fetch_ae_in_by_kind[kind.index()] += 1;
                compatibility_fingerprint.write_case(&case_name, 2 + kind.index() as u8);
            }
            _ if instruction_fetch_address_error.is_some()
                && mismatches.is_empty()
                && first_fail.is_none() =>
            {
                first_fail = Some((
                    case_name,
                    vec![Mismatch {
                        field: "instruction-fetch address-error I/N".into(),
                        expected: "sole I/N=1-to-I/N=0 divergence".into(),
                        actual: "no divergence".into(),
                    }],
                ));
            }
            _ if instruction_fetch_address_error.is_some() && mismatches.is_empty() => {}
            _ if mismatches.is_empty() => passed += 1,
            _ if first_fail.is_none() => first_fail = Some((case_name, mismatches)),
            _ => {}
        }
    }

    Ok(FixtureResult {
        name,
        total: tests.len().saturating_sub(skipped),
        passed,
        accepted_pc_displacement_ae,
        accepted_pc_indexed_ae,
        accepted_instruction_fetch_ae_in,
        accepted_instruction_fetch_ae_in_by_kind,
        compatibility_fingerprint,
        skipped,
        first_fail,
    })
}

fn print_result(r: &FixtureResult) {
    let successful = r.successful();
    let rate = if r.total > 0 {
        (successful as f64 / r.total as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "  {:<24} {:>5}/{:<5} ({:>5.1}%)",
        r.name, successful, r.total, rate
    );
    if r.accepted_pc_relative_ae() > 0 {
        println!(
            "    exact agreements: {}; accepted PC-relative AE FC divergences: {} (d16,PC {}, d8,PC,Xn {})",
            r.passed,
            r.accepted_pc_relative_ae(),
            r.accepted_pc_displacement_ae,
            r.accepted_pc_indexed_ae
        );
    }
    if r.accepted_instruction_fetch_ae_in > 0 {
        println!(
            "    accepted instruction-fetch AE I/N divergences: {}",
            r.accepted_instruction_fetch_ae_in
        );
    }
    if r.skipped > 0 {
        println!(
            "    skipped: {} invalid fixture row{}",
            r.skipped,
            if r.skipped == 1 { "" } else { "s" }
        );
    }
    if let Some((case, mismatches)) = &r.first_fail {
        println!("    first fail: {case}");
        for m in mismatches.iter().take(6) {
            println!(
                "      {:<16} expected={:<12} actual={}",
                m.field, m.expected, m.actual
            );
        }
        if mismatches.len() > 6 {
            println!("      ... and {} more", mismatches.len() - 6);
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

/// Run a small curated set of opcodes that the Amiga graphics.library
/// init code path depends on heavily. This is the first filter —
/// if any of these fail the full sweep is redundant until fixed.
#[test]
#[ignore]
fn harte_smoke() {
    let root = fixture_root();
    assert!(
        root.is_dir(),
        "fixture directory not found at {}; set EMU198X_68000_TOM_HARTE_ROOT",
        root.display()
    );

    // Load-bearing opcodes for the graphics.library init path:
    //   MOVE.L/.W/.b (all addressing modes)
    //   LEA, PEA
    //   ADDQ, SUBQ
    //   AND, OR, ANDI-to-CCR/SR, ORI
    //   BTST, BCLR, BSET
    //   Bcc, DBcc, BSR, BRA, JMP, JSR, RTS
    //   CLR, CMPI, MOVEQ
    //   TST
    //   MOVEM
    let smoke = [
        "MOVE.b", "MOVE.w", "MOVE.l", "MOVE.q", "MOVEA.w", "MOVEA.l", "MOVEM.w", "MOVEM.l", "LEA",
        "PEA", "ADD.w", "ADD.l", "SUB.w", "SUB.l", "AND.w", "AND.l", "OR.w", "OR.l", "CMP.w",
        "CMP.l", "BTST", "BCLR", "BSET", "BCHG", "Bcc", "BSR", "DBcc", "JMP", "JSR", "RTS", "RTE",
        "CLR.w", "CLR.l", "TST.w", "TST.l", "EXT.w", "EXT.l", "SWAP", "LSL.w", "LSR.w", "ASL.w",
        "ASR.w", "NOP",
    ];

    println!();
    println!(
        "SingleStepTests 680x0 smoke test ({} opcodes):",
        smoke.len()
    );
    let mut total_passed = 0;
    let mut total_accepted = 0;
    let mut total_accepted_instruction_fetch_ae_in = 0;
    let mut total_tests = 0;
    let mut fail_count = 0;

    for name in smoke {
        let path = root.join(format!("{name}.json.gz"));
        let r = run_fixture(&path)
            .unwrap_or_else(|error| panic!("failed to run fixture {name}: {error}"));
        print_result(&r);
        total_passed += r.passed;
        total_accepted += r.accepted_pc_relative_ae();
        total_accepted_instruction_fetch_ae_in += r.accepted_instruction_fetch_ae_in;
        total_tests += r.total;
        if r.successful() != r.total {
            fail_count += 1;
        }
    }

    println!();
    let successful = total_passed + total_accepted + total_accepted_instruction_fetch_ae_in;
    let rate = (successful as f64 / total_tests as f64) * 100.0;
    println!(
        "SMOKE TOTAL: {}/{} ({:.1}%) — {} opcodes with failures",
        successful, total_tests, rate, fail_count
    );
    println!(
        "  exact agreements: {}; accepted PC-relative AE FC divergences: {}",
        total_passed, total_accepted
    );
    println!(
        "  accepted instruction-fetch AE I/N divergences: {}",
        total_accepted_instruction_fetch_ae_in
    );
    assert_eq!(
        successful, total_tests,
        "SingleStepTests smoke comparison failures were reported above"
    );
    assert_eq!(fail_count, 0, "one or more smoke fixtures did not pass");
}

/// Run the registered 124-file fixture sweep. Slow — takes several
/// minutes. Report per-file pass rates and fail on any incomplete or
/// mismatching comparison.
#[test]
#[ignore]
fn harte_full_sweep() {
    let root = fixture_root();
    assert!(
        root.is_dir(),
        "fixture directory not found at {}; set EMU198X_68000_TOM_HARTE_ROOT",
        root.display()
    );

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("read_dir")
        .map(|entry| entry.expect("read fixture directory entry").path())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().ends_with(".json.gz"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();
    assert_eq!(
        entries.len(),
        EXPECTED_FULL_SWEEP_FIXTURES,
        "unexpected fixture count in {}",
        root.display()
    );

    println!();
    println!("Fixture root: {}", root.display());
    println!(
        "SingleStepTests 680x0 full sweep ({} fixtures):",
        entries.len()
    );

    let mut total_passed = 0usize;
    let mut total_accepted_pc_displacement_ae = 0usize;
    let mut total_accepted_pc_indexed_ae = 0usize;
    let mut total_accepted_instruction_fetch_ae_in = 0usize;
    let mut total_accepted_instruction_fetch_ae_in_by_kind = [0usize; 8];
    let mut compatibility_fingerprint = CompatibilityFingerprint::default();
    let mut total_tests = 0usize;
    let mut fully_passing = 0usize;
    let mut fully_failing = 0usize;
    let mut total_skipped = 0usize;
    let mut total_rows = 0usize;

    for path in &entries {
        let r = run_fixture(path)
            .unwrap_or_else(|error| panic!("failed to run fixture {}: {error}", path.display()));
        print_result(&r);
        total_passed += r.passed;
        total_accepted_pc_displacement_ae += r.accepted_pc_displacement_ae;
        total_accepted_pc_indexed_ae += r.accepted_pc_indexed_ae;
        total_accepted_instruction_fetch_ae_in += r.accepted_instruction_fetch_ae_in;
        for (total, fixture) in total_accepted_instruction_fetch_ae_in_by_kind
            .iter_mut()
            .zip(r.accepted_instruction_fetch_ae_in_by_kind)
        {
            *total += fixture;
        }
        compatibility_fingerprint.write_u64(r.compatibility_fingerprint.0);
        total_tests += r.total;
        total_skipped += r.skipped;
        total_rows += r.total + r.skipped;
        if r.successful() == r.total {
            fully_passing += 1;
        } else if r.successful() == 0 {
            fully_failing += 1;
        }
    }

    println!();
    let total_accepted = total_accepted_pc_displacement_ae + total_accepted_pc_indexed_ae;
    let successful = total_passed + total_accepted + total_accepted_instruction_fetch_ae_in;
    let rate = (successful as f64 / total_tests as f64) * 100.0;
    println!("FULL TOTAL: {}/{} ({:.2}%)", successful, total_tests, rate);
    println!("  exact agreements: {}", total_passed);
    println!(
        "  accepted PC-relative AE FC divergences: {} (d16,PC {}, d8,PC,Xn {})",
        total_accepted, total_accepted_pc_displacement_ae, total_accepted_pc_indexed_ae
    );
    println!(
        "  accepted instruction-fetch AE I/N divergences: {}",
        total_accepted_instruction_fetch_ae_in
    );
    println!(
        "    by family: Bcc={} BSR={} DBcc={} JMP={} JSR={} RTS={} RTE={} RTR={}",
        total_accepted_instruction_fetch_ae_in_by_kind[0],
        total_accepted_instruction_fetch_ae_in_by_kind[1],
        total_accepted_instruction_fetch_ae_in_by_kind[2],
        total_accepted_instruction_fetch_ae_in_by_kind[3],
        total_accepted_instruction_fetch_ae_in_by_kind[4],
        total_accepted_instruction_fetch_ae_in_by_kind[5],
        total_accepted_instruction_fetch_ae_in_by_kind[6],
        total_accepted_instruction_fetch_ae_in_by_kind[7],
    );
    println!(
        "  row-stable compatibility fingerprint: {:016x}",
        compatibility_fingerprint.0
    );
    if total_skipped > 0 {
        println!("  skipped invalid fixture rows: {}", total_skipped);
    }
    println!("  fully passing: {} / {}", fully_passing, entries.len());
    println!("  fully failing: {} / {}", fully_failing, entries.len());

    assert_eq!(
        total_rows, EXPECTED_FULL_SWEEP_ROWS,
        "registered corpus row count changed"
    );
    assert_eq!(
        total_skipped, EXPECTED_INVALID_ROWS,
        "invalid fixture exclusion set changed"
    );
    assert_eq!(
        total_tests, EXPECTED_COMPARED_ROWS,
        "comparison denominator changed"
    );
    assert_eq!(
        total_passed, EXPECTED_EXACT_AGREEMENT_ROWS,
        "exact-agreement count changed"
    );
    assert_eq!(
        total_accepted, EXPECTED_PC_RELATIVE_AE_DIVERGENCES,
        "accepted PC-relative address-error divergence count changed"
    );
    assert_eq!(
        total_accepted_pc_displacement_ae, EXPECTED_PC_DISPLACEMENT_AE_DIVERGENCES,
        "accepted d16,PC address-error divergence count changed"
    );
    assert_eq!(
        total_accepted_pc_indexed_ae, EXPECTED_PC_INDEXED_AE_DIVERGENCES,
        "accepted d8,PC,Xn address-error divergence count changed"
    );
    assert_eq!(
        total_accepted_instruction_fetch_ae_in, EXPECTED_INSTRUCTION_FETCH_AE_IN_DIVERGENCES,
        "accepted instruction-fetch address-error I/N divergence count changed"
    );
    assert_eq!(
        total_accepted_instruction_fetch_ae_in_by_kind, EXPECTED_INSTRUCTION_FETCH_AE_IN_BY_KIND,
        "instruction-fetch address-error family partition changed"
    );
    assert_eq!(
        compatibility_fingerprint, EXPECTED_COMPATIBILITY_FINGERPRINT,
        "row-level address-error compatibility boundary changed"
    );
    assert_eq!(
        successful, total_tests,
        "unexpected SingleStepTests comparison failures were reported above"
    );
    assert_eq!(
        fully_passing,
        entries.len(),
        "one or more fixture files did not pass completely"
    );
}

/// Run only the remaining non-green opcode groups so iteration does not
/// require another full multi-minute sweep.
#[test]
#[ignore]
fn harte_focus_remaining() {
    let root = fixture_root();
    assert!(
        root.is_dir(),
        "fixture directory not found at {}; set EMU198X_68000_TOM_HARTE_ROOT",
        root.display()
    );

    let focus = ["ASL.b", "DIVS", "DIVU"];

    println!();
    println!(
        "SingleStepTests 680x0 focused sweep ({} opcodes):",
        focus.len()
    );
    let mut total_passed = 0usize;
    let mut total_accepted = 0usize;
    let mut total_accepted_instruction_fetch_ae_in = 0usize;
    let mut total_tests = 0usize;
    let mut fail_count = 0usize;
    let mut total_skipped = 0usize;

    for name in focus {
        let path = root.join(format!("{name}.json.gz"));
        let r = run_fixture(&path)
            .unwrap_or_else(|error| panic!("failed to run fixture {name}: {error}"));
        print_result(&r);
        total_passed += r.passed;
        total_accepted += r.accepted_pc_relative_ae();
        total_accepted_instruction_fetch_ae_in += r.accepted_instruction_fetch_ae_in;
        total_tests += r.total;
        total_skipped += r.skipped;
        if r.successful() != r.total {
            fail_count += 1;
        }
    }

    println!();
    let successful = total_passed + total_accepted + total_accepted_instruction_fetch_ae_in;
    let rate = (successful as f64 / total_tests as f64) * 100.0;
    println!(
        "FOCUS TOTAL: {}/{} ({:.2}%) — {} opcodes with failures",
        successful, total_tests, rate, fail_count
    );
    println!(
        "  exact agreements: {}; accepted PC-relative AE FC divergences: {}",
        total_passed, total_accepted
    );
    println!(
        "  accepted instruction-fetch AE I/N divergences: {}",
        total_accepted_instruction_fetch_ae_in
    );
    if total_skipped > 0 {
        println!("  skipped invalid fixture rows: {}", total_skipped);
    }
    assert_eq!(
        successful, total_tests,
        "SingleStepTests focused comparison failures were reported above"
    );
    assert_eq!(fail_count, 0, "one or more focused fixtures did not pass");
}
