//! Inherited-subset cross-check against the 68000 SingleStepTests corpus.
//!
//! Runs the upstream Tom Harte SingleStepTests/680x0 corpus —
//! implementation-generated according to its README — through the
//! `Cpu68020` wrapper. Most of the 68020 ISA is inherited from the
//! 68000; this harness verifies the variant wrapper hasn't
//! accidentally broken compatibility with that suite.
//!
//! See [`knowledge/decisions/m68k-test-oracle-strategy.md`] for the
//! cross-validation rationale: the m68k-test-gen 68020 corpus is
//! Musashi-driven, so it can't expose Musashi-vs-SingleStepTests
//! divergences. This harness performs that suite comparison.
//!
//! Adapted from `motorola-68000/tests/tom_harte.rs` — same fixture
//! format, sparse-memory harness, and bus-cycle service routine,
//! just pointed at `Cpu68020` instead of `Cpu68000`.
//!
//! Default corpus root:
//!   `~/Projects/198x/assets/test-suites/processor-tests/680x0/68000/v1/`
//!
//! Override with `EMU198X_68000_TOM_HARTE_ROOT`.
//!
//! The selected 68000 source subset contains 790,364 rows. This harness
//! excludes 124,225 structurally identified 68000 address-error rows because
//! the 68020 permits misaligned data operands and uses different bus-fault
//! frames. It also excludes 73 rows whose `$FF` branch displacement selects a
//! long displacement on the 68020 but an eight-bit displacement on the 68000.
//! Every remaining row must agree exactly. Counts, class partitions, and a
//! source-row fingerprint pin the exclusion boundary.
//!
//! # Expected divergence
//!
//! The following fixtures are skipped because the wrapper's selected
//! behaviour differs from the SingleStepTests 68000 expectations. The
//! causes include documented variant behaviour and explicit
//! software-oracle compatibility policies; the skips are not evidence
//! of physical-hardware behaviour:
//!
//! - **`ABCD` / `SBCD` / `NBCD`** — Musashi-style "undefined V"
//!   flag, enabled by `variant_musashi_bcd_v` on the 68020 wrapper.
//! - **`DIVU.W` / `DIVS.W`** overflow cases — Musashi preserves C
//!   on overflow; SingleStepTests clears it, as the PRM also
//!   specifies (`variant_musashi_div_overflow`).
//! - **`MOVE from SR`** — non-privileged on the 68000, privileged
//!   on the 68020+. Any test in user mode raises a different trap
//!   on the 68020, with a different frame format.
//! - **Exception-bearing tests** that take a group-1/2 trap during
//!   the fixture: the 68020 pushes either an 8-byte Format `$0`
//!   frame (for TRAP / BKPT) or a 12-byte Format `$2` frame (for
//!   CHK / divide-by-zero / TRAPV — see
//!   `variant_format2_vectors`), whereas the 68000 pushes a 6-byte
//!   frame. Affects `CHK`, `TRAP`, `TRAPV`, `BKPT`, and the
//!   zero-divisor cases of `DIVU.W` / `DIVS.W`.
//! - **`RTE`** — the 68000 pops a 6-byte frame; the 68020 pops an
//!   8-byte frame (Format `$0`) or 12-byte (`$2`) frame.
//! - **`MOVEtoSR` / `ORItoSR` / `EORItoSR` / `ANDItoSR`** — the
//!   68020 widens the SR write mask to include the M-flag (bit
//!   12). When the source value or initial SR has the M-bit set,
//!   the 68000 corpus expects it to be masked out; the 68020
//!   preserves it. Documented as `variant_extended_sr_writes`.
//! - **Per-case indexed-addressing skips** (name contains "(d8,"):
//!   the brief extension word's scale field (bits 10-9) is "don't
//!   care" on the 68000 but `*1/*2/*4/*8` on the 68020+. The 68000
//!   corpus's random extension words contain non-zero scale bits
//!   that 68020 EA computation honours, producing a different
//!   effective address from the corpus's unscaled 68000 expectation.
//!   This is `variant_scaled_index` working as intended.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde_json::Value;

use motorola_68000::bus::{BusStatus, FunctionCode, interrupt_acknowledge_level};
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const EXPECTED_FIXTURE_FILES: usize = 124;
const EXPECTED_INCLUDED_FIXTURES: usize = 109;
const EXPECTED_SOURCE_ROWS: usize = 790_364;
const EXPECTED_COMPARED_ROWS: usize = 666_066;
const EXPECTED_EXACT_ROWS: usize = 666_066;
const EXPECTED_EXCLUDED_ADDRESS_ERROR_ROWS: usize = 124_225;
const EXPECTED_EXCLUDED_READ_ADDRESS_ERROR_ROWS: usize = 118_664;
const EXPECTED_EXCLUDED_WRITE_ADDRESS_ERROR_ROWS: usize = 5_561;
const EXPECTED_EXCLUDED_LONG_BRANCH_ROWS: usize = 73;
const EXPECTED_EXCLUDED_LONG_BCC_ROWS: usize = 38;
const EXPECTED_EXCLUDED_LONG_BSR_ROWS: usize = 35;
const EXPECTED_EXCLUSION_FINGERPRINT: ExclusionFingerprint =
    ExclusionFingerprint(0x660F_34E4_27A1_7DCA);
const MASK_24: u32 = 0x00FF_FFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddressErrorKind {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LongBranchKind {
    Bcc,
    Bsr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExclusionFingerprint(u64);

impl Default for ExclusionFingerprint {
    fn default() -> Self {
        Self(0xCBF2_9CE4_8422_2325)
    }
}

impl ExclusionFingerprint {
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

    fn write_address_error_case(&mut self, case_name: &str, kind: AddressErrorKind) {
        self.write_u64(case_name.len() as u64);
        self.write_bytes(case_name.as_bytes());
        self.write_u8(match kind {
            AddressErrorKind::Read => 0,
            AddressErrorKind::Write => 1,
        });
    }

    fn write_long_branch_case(&mut self, case_name: &str, kind: LongBranchKind) {
        self.write_u64(case_name.len() as u64);
        self.write_bytes(case_name.as_bytes());
        self.write_u8(match kind {
            LongBranchKind::Bcc => 2,
            LongBranchKind::Bsr => 3,
        });
    }
}

// ─── Sparse memory ────────────────────────────────────────────────

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

fn fixture_root() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME set");
    let home = PathBuf::from(home);
    if let Ok(path) = std::env::var("EMU198X_68000_TOM_HARTE_ROOT") {
        let p = PathBuf::from(path);
        if p.is_dir() {
            return p;
        }
    }
    home.join("Projects/198x/assets/test-suites/processor-tests/680x0/68000/v1")
}

fn load_fixture(path: &Path) -> Option<Vec<Value>> {
    let file = File::open(path).ok()?;
    let gz = GzDecoder::new(file);
    let parsed: Value = serde_json::from_reader(gz).ok()?;
    Some(parsed.as_array()?.clone())
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

fn classify_address_error(test: &Value) -> Option<AddressErrorKind> {
    let test = test.as_object()?;
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
        if address != (final_ssp.wrapping_add(offset) & MASK_24)
            || fixture_ram_word(final_state, address)? != data
        {
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
    if handler_second_address != (vector.wrapping_add(2) & MASK_24)
        || final_state.get("pc")?.as_u64()? as u32 != vector
    {
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
    if !matches!(function_code, 1 | 2 | 5 | 6) {
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

    Some(if special_status_word & 0x0010 != 0 {
        AddressErrorKind::Read
    } else {
        AddressErrorKind::Write
    })
}

fn classify_long_branch_variant(test: &Value) -> Option<LongBranchKind> {
    let opcode = test
        .get("initial")?
        .get("prefetch")?
        .as_array()?
        .first()?
        .as_u64()? as u16;
    if opcode >> 12 != 6 || opcode & 0x00FF != 0x00FF {
        return None;
    }
    Some(if (opcode >> 8) & 0x0F == 1 {
        LongBranchKind::Bsr
    } else {
        LongBranchKind::Bcc
    })
}

// ─── CPU state setup ──────────────────────────────────────────────

fn apply_initial(cpu: &mut Cpu68020, mem: &mut SparseMem, initial: &Value) {
    let o = initial.as_object().expect("initial is object");
    for i in 0..8 {
        cpu.regs.d[i] = o[&format!("d{i}")].as_u64().expect("dN") as u32;
    }
    for i in 0..7 {
        cpu.regs.a[i] = o[&format!("a{i}")].as_u64().expect("aN") as u32;
    }
    cpu.regs.usp = o["usp"].as_u64().expect("usp") as u32;
    cpu.regs.ssp = o["ssp"].as_u64().expect("ssp") as u32;
    cpu.regs.sr = o["sr"].as_u64().expect("sr") as u16;
    let pc = o["pc"].as_u64().expect("pc") as u32;

    let pq = o["prefetch"].as_array().expect("prefetch array");
    let pf0 = pq[0].as_u64().expect("prefetch0") as u16;
    let pf1 = pq[1].as_u64().expect("prefetch1") as u16;
    mem.write_word(pc, pf0);
    mem.write_word(pc.wrapping_add(2), pf1);

    cpu.regs.pc = pc.wrapping_add(4);
    cpu.setup_prefetch(pf0, pf1);
}

fn service_bus(cpu: &mut Cpu68020, mem: &mut SparseMem) {
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

fn run_one_instruction(cpu: &mut Cpu68020, mem: &mut SparseMem) -> bool {
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

fn matches_final(cpu: &Cpu68020, mem: &SparseMem, final_state: &Value) -> bool {
    let o = final_state.as_object().expect("final is object");
    for i in 0..8 {
        if cpu.regs.d[i] != o[&format!("d{i}")].as_u64().unwrap() as u32 {
            return false;
        }
    }
    for i in 0..7 {
        if cpu.regs.a[i] != o[&format!("a{i}")].as_u64().unwrap() as u32 {
            return false;
        }
    }
    if cpu.regs.usp != o["usp"].as_u64().unwrap() as u32 {
        return false;
    }
    if cpu.regs.ssp != o["ssp"].as_u64().unwrap() as u32 {
        return false;
    }
    if cpu.instr_start_pc != o["pc"].as_u64().unwrap() as u32 {
        return false;
    }
    if cpu.regs.sr != o["sr"].as_u64().unwrap() as u16 {
        return false;
    }
    for entry in o["ram"].as_array().unwrap() {
        let pair = entry.as_array().unwrap();
        let addr = pair[0].as_u64().unwrap() as u32;
        let expected = pair[1].as_u64().unwrap() as u8;
        if mem.read_byte(addr) != expected {
            return false;
        }
    }
    true
}

// ─── Expected-divergence fixture list ─────────────────────────────

/// Known-invalid fixture rows in the upstream corpus. The 68000
/// harness skips the same names — these are corpus bugs, not CPU
/// bugs (the "expected final D2" mutates bits a byte operation
/// can't touch).
fn is_known_invalid_case(case_name: &str) -> bool {
    matches!(
        case_name,
        "e502 [ASL.b Q, D2] 1583" | "e502 [ASL.b Q, D2] 1761"
    )
}

/// Per-case skip: 68020 honours the brief-extension-word scale
/// field (bits 10-9) that the 68000 treats as unused, so any
/// test whose disassembled name uses indexed addressing has a
/// 68000-corpus / 68020 EA divergence by design. The corpus name
/// includes the disassembly snippet — `"(d8, A0, Xn)"` or
/// `"(d8, PC, Xn)"` — so substring-matching on `"(d8,"` is enough.
fn is_indexed_addressing_case(case_name: &str) -> bool {
    case_name.contains("(d8,")
}

/// Fixtures whose expectations legitimately differ between the
/// 68000 corpus and the 68020. Skipped entirely from the pass-rate
/// total because they're not regressions.
fn is_expected_divergent_fixture(name: &str) -> bool {
    matches!(
        name,
        // BCD V flag (variant_musashi_bcd_v).
        "ABCD" | "SBCD" | "NBCD"
            // DIV: overflow path differs (variant_musashi_div_overflow);
            // zero-divisor path differs in frame format.
            | "DIVU" | "DIVS"
            // Frame-format differences (6-byte vs 8-byte) when the
            // fixture triggers a group-1/2 exception.
            | "CHK" | "TRAP" | "TRAPV" | "BKPT" | "RTE" | "RTR"
            // MOVE from SR — non-privileged on 68000, privileged on 68020+.
            | "MOVEfromSR"
            // SR-write instructions where the 68020 preserves the
            // M-flag (bit 12) but the 68000 corpus expects it
            // masked out. variant_extended_sr_writes.
            | "MOVEtoSR" | "ORItoSR" | "ANDItoSR" | "EORItoSR" // 68020+ exception frame contains a Format/Vector word
                                                               // that the 68000 corpus's MOVEM (write to stack via TRAP)
                                                               // and similar context-saves don't match.
                                                               //
                                                               // Note: MOVEM itself isn't divergent at the ISA level;
                                                               // any failure here would be a regression. Not skipped.
    )
}

// ─── Test ─────────────────────────────────────────────────────────

// `real_hw` in this legacy test name does not describe corpus provenance.
#[test]
#[ignore]
fn inherited_subset_passes_real_hw_68000_corpus() {
    let root = fixture_root();
    if !root.exists() {
        eprintln!("Skipping: fixture dir not found at {}", root.display());
        return;
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("read_dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().ends_with(".json.gz"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();
    assert_eq!(entries.len(), EXPECTED_FIXTURE_FILES);

    println!();
    println!(
        "68020 inherited-subset cross-check against SingleStepTests 68000 corpus ({} fixtures total):",
        entries.len()
    );

    let mut total_source_rows = 0usize;
    let mut total_compared_rows = 0usize;
    let mut total_exact = 0usize;
    let mut total_excluded_read_address_errors = 0usize;
    let mut total_excluded_write_address_errors = 0usize;
    let mut total_excluded_long_bcc = 0usize;
    let mut total_excluded_long_bsr = 0usize;
    let mut exclusion_fingerprint = ExclusionFingerprint::default();
    let mut skipped_fixtures = 0usize;
    let mut regressions: Vec<(String, usize, usize, String)> = Vec::new();

    for path in &entries {
        let fixture_name = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .trim_end_matches(".json.gz")
            .to_string();
        let base_name = fixture_name
            .split('_')
            .next()
            .unwrap_or(&fixture_name)
            .split('.')
            .next()
            .unwrap_or(&fixture_name);

        if is_expected_divergent_fixture(base_name) {
            skipped_fixtures += 1;
            continue;
        }

        let Some(tests) = load_fixture(path) else {
            continue;
        };
        let mut exact = 0usize;
        let mut excluded_read_address_errors = 0usize;
        let mut excluded_write_address_errors = 0usize;
        let mut excluded_long_bcc = 0usize;
        let mut excluded_long_bsr = 0usize;
        let mut skipped_cases = 0usize;
        let mut first_fail: Option<String> = None;
        for test in &tests {
            let t = test.as_object().unwrap();
            let case_name = t["name"].as_str().unwrap_or("?").to_string();
            if is_known_invalid_case(&case_name) || is_indexed_addressing_case(&case_name) {
                skipped_cases += 1;
                continue;
            }
            if let Some(kind) = classify_long_branch_variant(test) {
                match kind {
                    LongBranchKind::Bcc => excluded_long_bcc += 1,
                    LongBranchKind::Bsr => excluded_long_bsr += 1,
                }
                exclusion_fingerprint.write_long_branch_case(&case_name, kind);
                continue;
            }
            if let Some(kind) = classify_address_error(test) {
                match kind {
                    AddressErrorKind::Read => excluded_read_address_errors += 1,
                    AddressErrorKind::Write => excluded_write_address_errors += 1,
                }
                exclusion_fingerprint.write_address_error_case(&case_name, kind);
                continue;
            }

            let initial = &t["initial"];
            let final_state = &t["final"];
            let mut cpu = Cpu68020::new();
            let mut mem = SparseMem::new();
            mem.load_ram(initial["ram"].as_array().unwrap());
            apply_initial(&mut cpu, &mut mem, initial);

            if !run_one_instruction(&mut cpu, &mut mem) {
                if first_fail.is_none() {
                    first_fail = Some(format!("{case_name} (timeout)"));
                }
                continue;
            }
            if matches_final(&cpu, &mem, final_state) {
                exact += 1;
            } else if first_fail.is_none() {
                first_fail = Some(case_name);
            }
        }
        let source_rows = tests.len() - skipped_cases;
        let excluded = excluded_read_address_errors
            + excluded_write_address_errors
            + excluded_long_bcc
            + excluded_long_bsr;
        let compared = source_rows - excluded;
        total_source_rows += source_rows;
        total_compared_rows += compared;
        total_exact += exact;
        total_excluded_read_address_errors += excluded_read_address_errors;
        total_excluded_write_address_errors += excluded_write_address_errors;
        total_excluded_long_bcc += excluded_long_bcc;
        total_excluded_long_bsr += excluded_long_bsr;

        if exact != compared {
            regressions.push((
                fixture_name,
                exact,
                compared,
                first_fail.unwrap_or_else(|| "?".into()),
            ));
        }
    }

    let total_excluded_address_errors =
        total_excluded_read_address_errors + total_excluded_write_address_errors;
    let total_excluded_long_branches = total_excluded_long_bcc + total_excluded_long_bsr;
    let rate = if total_compared_rows > 0 {
        (total_exact as f64 / total_compared_rows as f64) * 100.0
    } else {
        0.0
    };
    println!();
    println!(
        "INHERITED SUBSET: {}/{} ({:.4}%) across {} fixtures",
        total_exact,
        total_compared_rows,
        rate,
        entries.len() - skipped_fixtures,
    );
    println!("  exact agreements: {total_exact}");
    println!("  selected 68000 source rows: {total_source_rows}");
    println!(
        "  excluded 68000 address-error rows: {} (read={} write={})",
        total_excluded_address_errors,
        total_excluded_read_address_errors,
        total_excluded_write_address_errors
    );
    println!(
        "  excluded 68000/68020 long-branch encoding rows: {} (Bcc={} BSR={})",
        total_excluded_long_branches, total_excluded_long_bcc, total_excluded_long_bsr
    );
    println!(
        "  row-stable exclusion fingerprint: {:016x}",
        exclusion_fingerprint.0
    );
    println!("  expected-divergent fixtures skipped: {skipped_fixtures}");
    if !regressions.is_empty() {
        println!();
        println!("Unexpected regressions ({}):", regressions.len());
        for (name, passed, total, first_fail) in &regressions {
            let pct = (*passed as f64 / *total as f64) * 100.0;
            println!(
                "  {:<24} {:>5}/{:<5} ({:>5.1}%)  first fail: {first_fail}",
                name, passed, total, pct
            );
        }
    }

    assert_eq!(
        entries.len() - skipped_fixtures,
        EXPECTED_INCLUDED_FIXTURES,
        "included fixture count changed"
    );
    assert_eq!(
        total_source_rows, EXPECTED_SOURCE_ROWS,
        "selected source-row count changed"
    );
    assert_eq!(
        total_compared_rows, EXPECTED_COMPARED_ROWS,
        "comparison denominator changed"
    );
    assert_eq!(
        total_exact, EXPECTED_EXACT_ROWS,
        "exact-agreement count changed"
    );
    assert_eq!(
        total_excluded_address_errors, EXPECTED_EXCLUDED_ADDRESS_ERROR_ROWS,
        "address-error exclusion boundary changed"
    );
    assert_eq!(
        total_excluded_read_address_errors, EXPECTED_EXCLUDED_READ_ADDRESS_ERROR_ROWS,
        "read-address-error exclusion boundary changed"
    );
    assert_eq!(
        total_excluded_write_address_errors, EXPECTED_EXCLUDED_WRITE_ADDRESS_ERROR_ROWS,
        "write-address-error exclusion boundary changed"
    );
    assert_eq!(
        total_excluded_long_branches, EXPECTED_EXCLUDED_LONG_BRANCH_ROWS,
        "long-branch encoding exclusion boundary changed"
    );
    assert_eq!(
        total_excluded_long_bcc, EXPECTED_EXCLUDED_LONG_BCC_ROWS,
        "Bcc long-branch exclusion boundary changed"
    );
    assert_eq!(
        total_excluded_long_bsr, EXPECTED_EXCLUDED_LONG_BSR_ROWS,
        "BSR long-branch exclusion boundary changed"
    );
    assert_eq!(
        exclusion_fingerprint, EXPECTED_EXCLUSION_FINGERPRINT,
        "row-level exclusion boundary changed"
    );
    assert_eq!(
        total_source_rows,
        total_compared_rows + total_excluded_address_errors + total_excluded_long_branches,
        "source rows are not fully partitioned into compared and excluded rows"
    );
    assert_eq!(
        total_exact, total_compared_rows,
        "unexpected inherited-subset comparison failures were reported above"
    );
    assert!(
        regressions.is_empty(),
        "The 68020 wrapper regressed on {} inherited-subset fixture(s) — see report above. \
         These are 68000 instructions that the 68020 inherits verbatim; failures here are real \
         regressions, not Musashi/SingleStepTests divergence (those fixtures are pre-skipped).",
        regressions.len()
    );
}
