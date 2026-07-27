//! SingleStepTests/m68000 binary-corpus comparison harness.
//!
//! This is deliberately separate from `tom_harte.rs`. The registered
//! `SingleStepTests/m68000` corpus is a compact binary corpus generated from
//! MAME's microcoded MC68000 core, according to its README. Its state and
//! program-counter conventions differ from the `SingleStepTests/680x0`
//! corpus.
//!
//! The agreement sweep is a regression gate for the currently agreed subset:
//! 240,090 rows. It parses all 127 files and all 317,500 rows, quarantines the
//! producer-declared-unverified TAS and TRAPV files, and reports the remaining
//! explicit evidence-boundary exclusions before comparing state.
//!
//! The full comparison excludes only TAS and TRAPV. It is expected to expose
//! unresolved differences and therefore fails while any compared row differs.
//! The focused address-error sweep compares the source-designated `re` and
//! `we` events with the core's internal rejected-transfer observation, then
//! reports exception-frame and final-state agreement separately.
//!
//! None of the tests compares ordered normal bus transactions. The focused
//! sweep does not compare the no-AS event's data-bus value or claim external
//! address-strobe timing.
//!
//! Run explicitly with:
//!
//! ```sh
//! EMU198X_68000_MAME_ROOT=/path/to/m68000/v1 \
//! cargo test --release -p motorola-68000 --test mame_singlestep \
//!   mame_agreement_sweep -- --include-ignored --nocapture
//!
//! EMU198X_68000_MAME_ROOT=/path/to/m68000/v1 \
//! cargo test --release -p motorola-68000 --test mame_singlestep \
//!   mame_address_error_event_sweep -- --include-ignored --nocapture
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use motorola_68000::bus::{BusStatus, FunctionCode, interrupt_acknowledge_level};
use motorola_68000::cpu::State;
use motorola_68000::{AddressErrorAccess, AddressErrorObservation, Cpu68000};

const EXPECTED_FIXTURE_FILES: usize = 127;
const EXPECTED_ROWS_PER_FILE: usize = 2_500;
const EXPECTED_CORPUS_ROWS: usize = 317_500;
const EXPECTED_UPSTREAM_QUARANTINE_ROWS: usize = 5_000;
const EXPECTED_ADDRESS_ERROR_ROWS: usize = 55_606;
const EXPECTED_ADDRESS_ERROR_FIXTURE_FILES: usize = 63;
const EXPECTED_READ_ADDRESS_ERROR_ROWS: usize = 53_160;
const EXPECTED_WRITE_ADDRESS_ERROR_ROWS: usize = 2_446;
const EXPECTED_ADDRESS_ERROR_ACCESS_INFORMATION_MATCHES: usize = 55_486;
const EXPECTED_ADDRESS_ERROR_IR_MATCHES: usize = 55_486;
const EXPECTED_ADDRESS_ERROR_SR_MATCHES: usize = 55_354;
const EXPECTED_ADDRESS_ERROR_PC_MATCHES: usize = 17_689;
const EXPECTED_DIVERGENT_GROUP_ROWS: usize = 14_304;
const EXPECTED_STOP_ROWS: usize = 2_500;
const EXPECTED_AGREEMENT_ROWS: usize = 240_090;
const EXPECTED_FULL_COMPARISON_ROWS: usize = 312_500;
const MAX_TICKS_PER_CASE: usize = 800;

const FILE_MAGIC: u32 = 0x1A3F_5D71;
const TEST_MAGIC: u32 = 0xABC1_2367;
const NAME_MAGIC: u32 = 0x89AB_CDEF;
const STATE_MAGIC: u32 = 0x0123_4567;
const TRANSACTIONS_MAGIC: u32 = 0x4567_89AB;

#[derive(Clone, Copy)]
struct RamWord {
    addr: u32,
    value: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceAddressErrorAccess {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug)]
struct AddressErrorTransaction {
    access: SourceAddressErrorAccess,
    cycles: u32,
    function_code: u8,
    address_bus: u32,
    data_bus: u16,
    upper_data_strobe: bool,
    lower_data_strobe: bool,
}

const fn mc68000_word_address_bus(address: u32) -> u32 {
    address & 0x00FF_FFFE
}

struct FixtureState {
    d: [u32; 8],
    a: [u32; 7],
    usp: u32,
    ssp: u32,
    sr: u16,
    pc: u32,
    prefetch: [u16; 2],
    ram: Vec<RamWord>,
}

struct FixtureCase {
    name: String,
    initial: FixtureState,
    final_state: FixtureState,
    address_error_transactions: Vec<AddressErrorTransaction>,
}

struct BinaryReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> BinaryReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn read_bytes(&mut self, count: usize, field: &str) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| format!("{field}: byte offset overflow"))?;
        let value = self.bytes.get(self.offset..end).ok_or_else(|| {
            format!(
                "{field}: need {count} bytes at offset {}, only {} remain",
                self.offset,
                self.remaining()
            )
        })?;
        self.offset = end;
        Ok(value)
    }

    fn read_u8(&mut self, field: &str) -> Result<u8, String> {
        Ok(self.read_bytes(1, field)?[0])
    }

    fn read_u16(&mut self, field: &str) -> Result<u16, String> {
        let bytes: [u8; 2] = self
            .read_bytes(2, field)?
            .try_into()
            .map_err(|_| format!("{field}: invalid u16 encoding"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn read_u32(&mut self, field: &str) -> Result<u32, String> {
        let bytes: [u8; 4] = self
            .read_bytes(4, field)?
            .try_into()
            .map_err(|_| format!("{field}: invalid u32 encoding"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_block(&mut self, expected_magic: u32, label: &str) -> Result<Self, String> {
        let block_start = self.offset;
        let declared_len = usize::try_from(self.read_u32(&format!("{label} length"))?)
            .map_err(|_| format!("{label}: length does not fit usize"))?;
        let actual_magic = self.read_u32(&format!("{label} magic"))?;
        if actual_magic != expected_magic {
            return Err(format!(
                "{label}: magic {actual_magic:#010X}, expected {expected_magic:#010X}"
            ));
        }
        if declared_len < 8 {
            return Err(format!(
                "{label}: declared length {declared_len} is smaller than its header"
            ));
        }
        let body = self
            .read_bytes(declared_len - 8, label)
            .map_err(|error| format!("{label} block beginning at offset {block_start}: {error}"))?;
        Ok(Self::new(body))
    }

    fn require_end(&self, label: &str) -> Result<(), String> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(format!(
                "{label}: {} unconsumed bytes at block offset {}",
                self.remaining(),
                self.offset
            ))
        }
    }
}

fn checked_u16(value: u32, field: &str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{field}: value {value:#010X} exceeds 16 bits"))
}

fn parse_name(reader: &mut BinaryReader<'_>) -> Result<String, String> {
    let mut block = reader.read_block(NAME_MAGIC, "name")?;
    let length = usize::try_from(block.read_u32("name string length")?)
        .map_err(|_| "name string length does not fit usize".to_owned())?;
    let bytes = block.read_bytes(length, "name string")?;
    let name = std::str::from_utf8(bytes)
        .map_err(|error| format!("name is not UTF-8: {error}"))?
        .to_owned();
    block.require_end("name")?;
    Ok(name)
}

fn parse_state(reader: &mut BinaryReader<'_>, label: &str) -> Result<FixtureState, String> {
    let mut block = reader.read_block(STATE_MAGIC, label)?;

    let mut d = [0; 8];
    for (index, value) in d.iter_mut().enumerate() {
        *value = block.read_u32(&format!("{label}.d{index}"))?;
    }

    let mut a = [0; 7];
    for (index, value) in a.iter_mut().enumerate() {
        *value = block.read_u32(&format!("{label}.a{index}"))?;
    }

    let usp = block.read_u32(&format!("{label}.usp"))?;
    let ssp = block.read_u32(&format!("{label}.ssp"))?;
    let sr = checked_u16(
        block.read_u32(&format!("{label}.sr"))?,
        &format!("{label}.sr"),
    )?;
    let pc = block.read_u32(&format!("{label}.pc"))?;
    let prefetch = [
        checked_u16(
            block.read_u32(&format!("{label}.prefetch[0]"))?,
            &format!("{label}.prefetch[0]"),
        )?,
        checked_u16(
            block.read_u32(&format!("{label}.prefetch[1]"))?,
            &format!("{label}.prefetch[1]"),
        )?,
    ];

    let ram_count = usize::try_from(block.read_u32(&format!("{label}.ram count"))?)
        .map_err(|_| format!("{label}.ram count does not fit usize"))?;
    let mut ram = Vec::with_capacity(ram_count);
    for index in 0..ram_count {
        let addr = block.read_u32(&format!("{label}.ram[{index}].address"))?;
        if addr >= 0x0100_0000 {
            return Err(format!(
                "{label}.ram[{index}]: address {addr:#010X} exceeds the 24-bit bus"
            ));
        }
        if addr & 1 != 0 {
            return Err(format!(
                "{label}.ram[{index}]: word address {addr:#010X} is odd"
            ));
        }
        let value = block.read_u16(&format!("{label}.ram[{index}].value"))?;
        ram.push(RamWord { addr, value });
    }
    block.require_end(label)?;

    Ok(FixtureState {
        d,
        a,
        usp,
        ssp,
        sr,
        pc,
        prefetch,
        ram,
    })
}

fn parse_transactions(
    reader: &mut BinaryReader<'_>,
) -> Result<Vec<AddressErrorTransaction>, String> {
    let mut block = reader.read_block(TRANSACTIONS_MAGIC, "transactions")?;
    let _declared_cycles = block.read_u32("transactions cycle length")?;
    let count = usize::try_from(block.read_u32("transaction count")?)
        .map_err(|_| "transaction count does not fit usize".to_owned())?;
    let mut address_errors = Vec::with_capacity(1);

    for index in 0..count {
        let kind = block.read_u8(&format!("transactions[{index}].kind"))?;
        let cycles = block.read_u32(&format!("transactions[{index}].cycles"))?;
        if kind == 0 {
            continue;
        }
        if !(1..=5).contains(&kind) {
            return Err(format!("transactions[{index}]: unsupported kind {kind}"));
        }

        let function_code =
            u8::try_from(block.read_u32(&format!("transactions[{index}].function_code"))?)
                .map_err(|_| format!("transactions[{index}]: function code exceeds eight bits"))?;
        if function_code > 7 {
            return Err(format!(
                "transactions[{index}]: function code {function_code} exceeds three bits"
            ));
        }
        let address = block.read_u32(&format!("transactions[{index}].address_bus"))?;
        if address >= 0x0100_0000 {
            return Err(format!(
                "transactions[{index}]: address {address:#010X} exceeds the 24-bit bus"
            ));
        }
        let data_bus =
            u16::try_from(block.read_u32(&format!("transactions[{index}].data_bus"))?)
                .map_err(|_| format!("transactions[{index}]: data bus value exceeds 16 bits"))?;
        let uds = block.read_u32(&format!("transactions[{index}].uds"))?;
        let lds = block.read_u32(&format!("transactions[{index}].lds"))?;
        if uds > 1 || lds > 1 {
            return Err(format!(
                "transactions[{index}]: invalid lane state UDS={uds}, LDS={lds}"
            ));
        }
        let access = match kind {
            4 => Some(SourceAddressErrorAccess::Read),
            5 => Some(SourceAddressErrorAccess::Write),
            _ => None,
        };
        if let Some(access) = access {
            address_errors.push(AddressErrorTransaction {
                access,
                cycles,
                function_code,
                address_bus: address,
                data_bus,
                upper_data_strobe: uds == 1,
                lower_data_strobe: lds == 1,
            });
        }
    }

    block.require_end("transactions")?;
    Ok(address_errors)
}

fn parse_case(reader: &mut BinaryReader<'_>, index: usize) -> Result<FixtureCase, String> {
    let mut block = reader
        .read_block(TEST_MAGIC, "test")
        .map_err(|error| format!("test {index}: {error}"))?;
    let name = parse_name(&mut block).map_err(|error| format!("test {index}: {error}"))?;
    let initial = parse_state(&mut block, "initial").map_err(|error| format!("{name}: {error}"))?;
    let final_state =
        parse_state(&mut block, "final").map_err(|error| format!("{name}: {error}"))?;
    let address_error_transactions =
        parse_transactions(&mut block).map_err(|error| format!("{name}: {error}"))?;
    block
        .require_end("test")
        .map_err(|error| format!("{name}: {error}"))?;

    Ok(FixtureCase {
        name,
        initial,
        final_state,
        address_error_transactions,
    })
}

fn parse_fixture(bytes: &[u8]) -> Result<Vec<FixtureCase>, String> {
    let mut reader = BinaryReader::new(bytes);
    let magic = reader.read_u32("file magic")?;
    if magic != FILE_MAGIC {
        return Err(format!(
            "file magic {magic:#010X}, expected {FILE_MAGIC:#010X}"
        ));
    }
    let case_count = usize::try_from(reader.read_u32("file case count")?)
        .map_err(|_| "file case count does not fit usize".to_owned())?;
    let mut cases = Vec::with_capacity(case_count);
    for index in 0..case_count {
        cases.push(parse_case(&mut reader, index)?);
    }
    reader.require_end("fixture file")?;
    Ok(cases)
}

struct SparseMem {
    bytes: HashMap<u32, u8>,
}

impl SparseMem {
    fn with_word_capacity(words: usize) -> Self {
        Self {
            bytes: HashMap::with_capacity(words.saturating_mul(2)),
        }
    }

    fn load_words(&mut self, words: &[RamWord]) {
        for word in words {
            self.write_word(word.addr, word.value);
        }
    }

    fn read_byte(&self, addr: u32) -> u8 {
        self.bytes.get(&(addr & 0x00FF_FFFF)).copied().unwrap_or(0)
    }

    fn read_word(&self, addr: u32) -> u16 {
        let addr = addr & 0x00FF_FFFE;
        (u16::from(self.read_byte(addr)) << 8) | u16::from(self.read_byte(addr.wrapping_add(1)))
    }

    fn write_byte(&mut self, addr: u32, value: u8) {
        self.bytes.insert(addr & 0x00FF_FFFF, value);
    }

    fn write_word(&mut self, addr: u32, value: u16) {
        let addr = addr & 0x00FF_FFFE;
        self.write_byte(addr, (value >> 8) as u8);
        self.write_byte(addr.wrapping_add(1), value as u8);
    }
}

fn apply_initial(cpu: &mut Cpu68000, mem: &mut SparseMem, initial: &FixtureState) {
    cpu.regs.d = initial.d;
    cpu.regs.a[..7].copy_from_slice(&initial.a);
    cpu.regs.usp = initial.usp;
    cpu.regs.ssp = initial.ssp;
    cpu.regs.sr = initial.sr;

    mem.load_words(&initial.ram);

    // The fixture PC is MAME m_au: the next prefetch address, already four
    // bytes beyond the instruction start. Preserve fixture RAM independently
    // from the prefetched words so stale-prefetch cases remain representable.
    cpu.regs.pc = initial.pc;
    cpu.setup_prefetch(initial.prefetch[0], initial.prefetch[1]);
}

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
                let value = if *is_word {
                    mem.read_word(*addr)
                } else {
                    u16::from(mem.read_byte(*addr))
                };
                cpu.bus_status = BusStatus::Ready(value);
            } else {
                let value = data.unwrap_or(0);
                if *is_word {
                    mem.write_word(*addr, value);
                } else {
                    mem.write_byte(*addr, value as u8);
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

fn run_one_instruction(cpu: &mut Cpu68000, mem: &mut SparseMem) -> bool {
    let start_count = cpu.instruction_starts;
    for _ in 0..MAX_TICKS_PER_CASE {
        service_bus(cpu, mem);
        cpu.tick();
        if cpu.instruction_starts > start_count
            || matches!(cpu.state, State::Stopped | State::Halted)
        {
            return true;
        }
    }
    false
}

fn complete_final_prefetch(cpu: &mut Cpu68000, mem: &mut SparseMem) -> bool {
    let expected_irc_address = cpu.instr_start_pc.wrapping_add(2);
    for _ in 0..MAX_TICKS_PER_CASE {
        if cpu.irc_addr == expected_irc_address && matches!(cpu.state, State::Idle) {
            return true;
        }
        service_bus(cpu, mem);
        cpu.tick();
    }
    false
}

#[derive(Clone, Debug)]
struct Mismatch {
    field: String,
    expected: String,
    actual: String,
}

fn compare_final(cpu: &Cpu68000, mem: &SparseMem, expected: &FixtureState) -> Vec<Mismatch> {
    let mut mismatches = Vec::new();

    for index in 0..8 {
        if cpu.regs.d[index] != expected.d[index] {
            mismatches.push(Mismatch {
                field: format!("d{index}"),
                expected: format!("${:08X}", expected.d[index]),
                actual: format!("${:08X}", cpu.regs.d[index]),
            });
        }
    }
    for index in 0..7 {
        if cpu.regs.a[index] != expected.a[index] {
            mismatches.push(Mismatch {
                field: format!("a{index}"),
                expected: format!("${:08X}", expected.a[index]),
                actual: format!("${:08X}", cpu.regs.a[index]),
            });
        }
    }

    for (field, actual, expected_value) in [
        ("usp", cpu.regs.usp, expected.usp),
        ("ssp", cpu.regs.ssp, expected.ssp),
    ] {
        if actual != expected_value {
            mismatches.push(Mismatch {
                field: field.to_owned(),
                expected: format!("${expected_value:08X}"),
                actual: format!("${actual:08X}"),
            });
        }
    }

    if cpu.regs.sr != expected.sr {
        mismatches.push(Mismatch {
            field: "sr".to_owned(),
            expected: format!("${:04X}", expected.sr),
            actual: format!("${:04X}", cpu.regs.sr),
        });
    }

    // MAME m_au is four bytes beyond the next instruction start at this
    // comparison boundary. Keep the normalisation explicit rather than
    // changing the source value during parsing.
    let expected_instruction_start = expected.pc.wrapping_sub(4);
    if cpu.instr_start_pc != expected_instruction_start {
        mismatches.push(Mismatch {
            field: "pc (normalised m_au)".to_owned(),
            expected: format!("${expected_instruction_start:08X}"),
            actual: format!("${:08X}", cpu.instr_start_pc),
        });
    }

    for word in &expected.ram {
        let actual = mem.read_word(word.addr);
        if actual != word.value {
            mismatches.push(Mismatch {
                field: format!("mem[${:06X}]", word.addr),
                expected: format!("${:04X}", word.value),
                actual: format!("${actual:04X}"),
            });
        }
    }

    mismatches
}

#[derive(Clone, Copy)]
struct AddressErrorFrame {
    access_information: u16,
    fault_address: u32,
    instruction_register: u16,
    status_register: u16,
    program_counter: u32,
}

fn expected_frame_word(expected: &FixtureState, address: u32, field: &str) -> Result<u16, String> {
    expected
        .ram
        .iter()
        .find(|word| word.addr == address)
        .map(|word| word.value)
        .ok_or_else(|| format!("final state does not list {field} at ${address:06X}"))
}

fn expected_address_error_frame(expected: &FixtureState) -> Result<AddressErrorFrame, String> {
    let base = expected.ssp & 0x00FF_FFFE;
    let word = |offset, field| expected_frame_word(expected, base.wrapping_add(offset), field);
    let access_information = word(0, "access-information word")?;
    let fault_hi = word(2, "fault-address high word")?;
    let fault_lo = word(4, "fault-address low word")?;
    let instruction_register = word(6, "instruction-register word")?;
    let status_register = word(8, "status-register word")?;
    let pc_hi = word(10, "program-counter high word")?;
    let pc_lo = word(12, "program-counter low word")?;

    Ok(AddressErrorFrame {
        access_information,
        fault_address: (u32::from(fault_hi) << 16) | u32::from(fault_lo),
        instruction_register,
        status_register,
        program_counter: (u32::from(pc_hi) << 16) | u32::from(pc_lo),
    })
}

fn compare_frame_memory(
    mem: &SparseMem,
    expected: &FixtureState,
    frame: AddressErrorFrame,
) -> Vec<Mismatch> {
    let base = expected.ssp & 0x00FF_FFFE;
    let expected_words = [
        frame.access_information,
        (frame.fault_address >> 16) as u16,
        frame.fault_address as u16,
        frame.instruction_register,
        frame.status_register,
        (frame.program_counter >> 16) as u16,
        frame.program_counter as u16,
    ];
    let mut mismatches = Vec::new();
    for (index, expected_word) in expected_words.into_iter().enumerate() {
        let address = base.wrapping_add((index as u32) * 2);
        let actual = mem.read_word(address);
        if actual != expected_word {
            mismatches.push(Mismatch {
                field: format!("frame[{}] @ ${address:06X}", index),
                expected: format!("${expected_word:04X}"),
                actual: format!("${actual:04X}"),
            });
        }
    }
    mismatches
}

fn compare_address_error_final(
    cpu: &Cpu68000,
    mem: &SparseMem,
    expected: &FixtureState,
) -> Vec<Mismatch> {
    let mut mismatches = compare_final(cpu, mem, expected);
    for (field, actual, expected_word) in [
        ("prefetch[0]", cpu.ir, expected.prefetch[0]),
        ("prefetch[1]", cpu.irc, expected.prefetch[1]),
    ] {
        if actual != expected_word {
            mismatches.push(Mismatch {
                field: field.to_owned(),
                expected: format!("${expected_word:04X}"),
                actual: format!("${actual:04X}"),
            });
        }
    }
    mismatches
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PreparedFrameDifferences {
    access_information: bool,
    fault_address: bool,
    instruction_register: bool,
    status_register: bool,
    program_counter: bool,
}

impl PreparedFrameDifferences {
    const NONE: Self = Self {
        access_information: false,
        fault_address: false,
        instruction_register: false,
        status_register: false,
        program_counter: false,
    };
    const PROGRAM_COUNTER: Self = Self {
        program_counter: true,
        ..Self::NONE
    };
    const STATUS_REGISTER_AND_PROGRAM_COUNTER: Self = Self {
        status_register: true,
        program_counter: true,
        ..Self::NONE
    };
    const ACCESS_INFORMATION_INSTRUCTION_REGISTER_AND_PROGRAM_COUNTER: Self = Self {
        access_information: true,
        instruction_register: true,
        program_counter: true,
        ..Self::NONE
    };

    fn from_comparison(comparison: &AddressErrorObservationComparison) -> Self {
        Self {
            access_information: !comparison.access_information_matches,
            fault_address: !comparison.frame_fault_address_matches,
            instruction_register: !comparison.frame_ir_matches,
            status_register: !comparison.frame_sr_matches,
            program_counter: !comparison.frame_pc_matches,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SelectedFinalDifferences {
    data_registers: u8,
    address_registers: u8,
    user_stack_pointer: bool,
    supervisor_stack_pointer: bool,
    status_register: bool,
    normalised_program_counter: bool,
    prefetch_words: u8,
    non_frame_memory_words: usize,
    execution_boundary: bool,
}

impl SelectedFinalDifferences {
    const NONE: Self = Self {
        data_registers: 0,
        address_registers: 0,
        user_stack_pointer: false,
        supervisor_stack_pointer: false,
        status_register: false,
        normalised_program_counter: false,
        prefetch_words: 0,
        non_frame_memory_words: 0,
        execution_boundary: false,
    };
    const STATUS_REGISTER_ONLY: Self = Self {
        status_register: true,
        ..Self::NONE
    };

    fn is_one_address_register_only(self) -> bool {
        self.address_registers.count_ones() == 1
            && Self {
                address_registers: 0,
                ..self
            } == Self::NONE
    }

    fn is_one_data_register_only(self) -> bool {
        self.data_registers.count_ones() == 1
            && Self {
                data_registers: 0,
                ..self
            } == Self::NONE
    }
}

fn selected_final_differences(
    cpu: &Cpu68000,
    mem: &SparseMem,
    expected: &FixtureState,
    completed: bool,
) -> SelectedFinalDifferences {
    let mut differences = SelectedFinalDifferences {
        execution_boundary: !completed,
        ..SelectedFinalDifferences::NONE
    };

    for index in 0..8 {
        if cpu.regs.d[index] != expected.d[index] {
            differences.data_registers |= 1 << index;
        }
    }
    for index in 0..7 {
        if cpu.regs.a[index] != expected.a[index] {
            differences.address_registers |= 1 << index;
        }
    }
    differences.user_stack_pointer = cpu.regs.usp != expected.usp;
    differences.supervisor_stack_pointer = cpu.regs.ssp != expected.ssp;
    differences.status_register = cpu.regs.sr != expected.sr;
    differences.normalised_program_counter = cpu.instr_start_pc != expected.pc.wrapping_sub(4);
    differences.prefetch_words =
        u8::from(cpu.ir != expected.prefetch[0]) | (u8::from(cpu.irc != expected.prefetch[1]) << 1);

    let frame_base = expected.ssp & 0x00FF_FFFE;
    for word in &expected.ram {
        let word_address = word.addr & 0x00FF_FFFE;
        let is_frame_word = (0..7).any(|index| word_address == frame_base.wrapping_add(index * 2));
        if !is_frame_word && mem.read_word(word.addr) != word.value {
            differences.non_frame_memory_words += 1;
        }
    }

    differences
}

fn frame_memory_matches_observation(
    mem: &SparseMem,
    expected: &FixtureState,
    observation: AddressErrorObservation,
) -> bool {
    let frame_base = expected.ssp & 0x00FF_FFFE;
    let prepared_words = [
        observation.access_information,
        (observation.frame_fault_address >> 16) as u16,
        observation.frame_fault_address as u16,
        observation.frame_ir,
        observation.saved_sr,
        (observation.frame_pc >> 16) as u16,
        observation.frame_pc as u16,
    ];
    prepared_words
        .into_iter()
        .enumerate()
        .all(|(index, word)| mem.read_word(frame_base.wrapping_add((index as u32) * 2)) == word)
}

struct AddressErrorObservationComparison {
    source_four_cycles: bool,
    source_even_address_bus: bool,
    source_word_lanes: bool,
    source_frame_address_link: bool,
    source_transfer_information_link: bool,
    source_instruction_processing_link: bool,
    observed: bool,
    direction_matches: bool,
    requested_address_bus_matches: bool,
    frame_address_bus_matches: bool,
    function_code_matches: bool,
    event_matches: bool,
    access_information_matches: bool,
    access_information_low_matches: bool,
    frame_fault_address_matches: bool,
    frame_ir_matches: bool,
    frame_sr_matches: bool,
    frame_pc_matches: bool,
    prepared_frame_matches: bool,
    mismatches: Vec<Mismatch>,
}

fn compare_address_error_observation(
    source: AddressErrorTransaction,
    expected_frame: AddressErrorFrame,
    observed: Option<AddressErrorObservation>,
) -> AddressErrorObservationComparison {
    let source_four_cycles = source.cycles == 4;
    let source_even_address_bus = source.address_bus & 1 == 0;
    let source_word_lanes = source.upper_data_strobe && source.lower_data_strobe;
    let source_frame_address_link =
        expected_frame.fault_address & 0x00FF_FFFF == (source.address_bus | 1);
    let expected_transfer_information = match source.access {
        SourceAddressErrorAccess::Read => 0x10,
        SourceAddressErrorAccess::Write => 0,
    } | u16::from(source.function_code);
    let source_transfer_information_link =
        expected_frame.access_information & 0x17 == expected_transfer_information;
    let source_instruction_processing_link = expected_frame.access_information & 0x08 == 0;

    // The source retains a data-bus value for no-AS events, but the core does
    // not expose a corresponding read value or pin-level write value. Keep it
    // parsed without treating it as a comparison field.
    let _source_data_bus = source.data_bus;

    let mut mismatches = Vec::new();
    let Some(observation) = observed else {
        mismatches.push(Mismatch {
            field: "address-error observation".to_owned(),
            expected: format!("{:?}", source.access),
            actual: "none".to_owned(),
        });
        return AddressErrorObservationComparison {
            source_four_cycles,
            source_even_address_bus,
            source_word_lanes,
            source_frame_address_link,
            source_transfer_information_link,
            source_instruction_processing_link,
            observed: false,
            direction_matches: false,
            requested_address_bus_matches: false,
            frame_address_bus_matches: false,
            function_code_matches: false,
            event_matches: false,
            access_information_matches: false,
            access_information_low_matches: false,
            frame_fault_address_matches: false,
            frame_ir_matches: false,
            frame_sr_matches: false,
            frame_pc_matches: false,
            prepared_frame_matches: false,
            mismatches,
        };
    };

    let expected_access = match source.access {
        SourceAddressErrorAccess::Read => AddressErrorAccess::Read,
        SourceAddressErrorAccess::Write => AddressErrorAccess::Write,
    };
    let direction_matches = observation.access == expected_access;
    let requested_address_bus_matches =
        mc68000_word_address_bus(observation.requested_address) == source.address_bus;
    let frame_address_bus_matches =
        mc68000_word_address_bus(observation.frame_fault_address) == source.address_bus;
    let function_code_matches = observation.function_code.bits() == source.function_code;
    let access_information_matches =
        observation.access_information == expected_frame.access_information;
    let access_information_low_matches =
        observation.access_information & 0x1F == expected_frame.access_information & 0x1F;
    let frame_fault_address_matches =
        observation.frame_fault_address == expected_frame.fault_address;
    let frame_ir_matches = observation.frame_ir == expected_frame.instruction_register;
    let frame_sr_matches = observation.saved_sr == expected_frame.status_register;
    let frame_pc_matches = observation.frame_pc == expected_frame.program_counter;

    for (field, matches, expected, actual) in [
        (
            "event direction",
            direction_matches,
            format!("{:?}", expected_access),
            format!("{:?}", observation.access),
        ),
        (
            "event address bus",
            frame_address_bus_matches,
            format!("${:06X}", source.address_bus),
            format!(
                "${:06X}",
                mc68000_word_address_bus(observation.frame_fault_address)
            ),
        ),
        (
            "event function code",
            function_code_matches,
            source.function_code.to_string(),
            observation.function_code.bits().to_string(),
        ),
        (
            "frame access information",
            access_information_matches,
            format!("${:04X}", expected_frame.access_information),
            format!("${:04X}", observation.access_information),
        ),
        (
            "frame fault address",
            frame_fault_address_matches,
            format!("${:08X}", expected_frame.fault_address),
            format!("${:08X}", observation.frame_fault_address),
        ),
        (
            "frame instruction register",
            frame_ir_matches,
            format!("${:04X}", expected_frame.instruction_register),
            format!("${:04X}", observation.frame_ir),
        ),
        (
            "frame status register",
            frame_sr_matches,
            format!("${:04X}", expected_frame.status_register),
            format!("${:04X}", observation.saved_sr),
        ),
        (
            "frame program counter",
            frame_pc_matches,
            format!("${:08X}", expected_frame.program_counter),
            format!("${:08X}", observation.frame_pc),
        ),
    ] {
        if !matches {
            mismatches.push(Mismatch {
                field: field.to_owned(),
                expected,
                actual,
            });
        }
    }

    let event_matches = direction_matches && frame_address_bus_matches && function_code_matches;
    let prepared_frame_matches = access_information_matches
        && frame_fault_address_matches
        && frame_ir_matches
        && frame_sr_matches
        && frame_pc_matches;

    AddressErrorObservationComparison {
        source_four_cycles,
        source_even_address_bus,
        source_word_lanes,
        source_frame_address_link,
        source_transfer_information_link,
        source_instruction_processing_link,
        observed: true,
        direction_matches,
        requested_address_bus_matches,
        frame_address_bus_matches,
        function_code_matches,
        event_matches,
        access_information_matches,
        access_information_low_matches,
        frame_fault_address_matches,
        frame_ir_matches,
        frame_sr_matches,
        frame_pc_matches,
        prepared_frame_matches,
        mismatches,
    }
}

fn fixture_root() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("EMU198X_68000_MAME_ROOT") {
        return Ok(PathBuf::from(explicit));
    }
    let home = std::env::var("HOME").map_err(|error| {
        format!("HOME is unavailable and EMU198X_68000_MAME_ROOT is not set: {error}")
    })?;
    Ok(PathBuf::from(home).join("Projects/198x/assets/test-suites/processor-tests/m68000/v1"))
}

fn fixture_label(path: &Path) -> Result<String, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("fixture path has no UTF-8 file name: {}", path.display()))?;
    file_name
        .strip_suffix(".json.bin")
        .map(str::to_owned)
        .ok_or_else(|| format!("unexpected fixture suffix: {file_name}"))
}

fn is_upstream_quarantine(label: &str) -> bool {
    matches!(label, "TAS" | "TRAPV")
}

fn is_divergent_group(label: &str) -> bool {
    matches!(
        label,
        "ASR.b" | "ASR.w" | "ASR.l" | "CHK" | "DIVS" | "DIVU" | "LINK"
    )
}

#[derive(Clone, Copy)]
enum SweepMode {
    Agreement,
    Full,
}

#[derive(Default)]
struct ExclusionCounts {
    upstream_quarantine: usize,
    address_error: usize,
    divergent_group: usize,
    stop: usize,
}

#[derive(Clone, Copy)]
enum ComparisonClass {
    Agreement,
    AddressError,
    DivergentGroup,
    Stop,
}

#[derive(Default)]
struct ClassCount {
    rows: usize,
    passed: usize,
}

#[derive(Default)]
struct ComparisonCounts {
    agreement: ClassCount,
    address_error: ClassCount,
    divergent_group: ClassCount,
    stop: ClassCount,
}

impl ComparisonCounts {
    fn record(&mut self, class: ComparisonClass, passed: bool) {
        let count = match class {
            ComparisonClass::Agreement => &mut self.agreement,
            ComparisonClass::AddressError => &mut self.address_error,
            ComparisonClass::DivergentGroup => &mut self.divergent_group,
            ComparisonClass::Stop => &mut self.stop,
        };
        count.rows += 1;
        count.passed += usize::from(passed);
    }

    fn add(&mut self, other: &Self) {
        self.agreement.rows += other.agreement.rows;
        self.agreement.passed += other.agreement.passed;
        self.address_error.rows += other.address_error.rows;
        self.address_error.passed += other.address_error.passed;
        self.divergent_group.rows += other.divergent_group.rows;
        self.divergent_group.passed += other.divergent_group.passed;
        self.stop.rows += other.stop.rows;
        self.stop.passed += other.stop.passed;
    }
}

fn comparison_class(label: &str, case: &FixtureCase) -> ComparisonClass {
    if !case.address_error_transactions.is_empty() {
        ComparisonClass::AddressError
    } else if is_divergent_group(label) {
        ComparisonClass::DivergentGroup
    } else if label == "STOP" {
        ComparisonClass::Stop
    } else {
        ComparisonClass::Agreement
    }
}

struct FixtureResult {
    label: String,
    rows: usize,
    compared: usize,
    passed: usize,
    exclusions: ExclusionCounts,
    comparisons: ComparisonCounts,
    first_failure: Option<(String, Vec<Mismatch>)>,
}

fn run_fixture(path: &Path, mode: SweepMode) -> Result<FixtureResult, String> {
    let label = fixture_label(path)?;
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let cases = parse_fixture(&bytes)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    if cases.len() != EXPECTED_ROWS_PER_FILE {
        return Err(format!(
            "{} contains {} rows; expected {EXPECTED_ROWS_PER_FILE}",
            path.display(),
            cases.len()
        ));
    }

    let mut result = FixtureResult {
        label: label.clone(),
        rows: cases.len(),
        compared: 0,
        passed: 0,
        exclusions: ExclusionCounts::default(),
        comparisons: ComparisonCounts::default(),
        first_failure: None,
    };

    for case in cases {
        if is_upstream_quarantine(&label) {
            result.exclusions.upstream_quarantine += 1;
            continue;
        }
        let class = comparison_class(&label, &case);
        if matches!(mode, SweepMode::Agreement) {
            match class {
                ComparisonClass::Agreement => {}
                ComparisonClass::AddressError => {
                    result.exclusions.address_error += 1;
                    continue;
                }
                ComparisonClass::DivergentGroup => {
                    result.exclusions.divergent_group += 1;
                    continue;
                }
                ComparisonClass::Stop => {
                    result.exclusions.stop += 1;
                    continue;
                }
            }
        }

        result.compared += 1;
        let mut cpu = Cpu68000::new();
        let mut memory = SparseMem::with_word_capacity(case.initial.ram.len());
        apply_initial(&mut cpu, &mut memory, &case.initial);

        let mut case_passed = false;
        if !run_one_instruction(&mut cpu, &mut memory) {
            if result.first_failure.is_none() {
                result.first_failure = Some((
                    case.name,
                    vec![Mismatch {
                        field: "execution boundary".to_owned(),
                        expected: "one completed instruction".to_owned(),
                        actual: format!("no boundary within {MAX_TICKS_PER_CASE} ticks"),
                    }],
                ));
            }
        } else {
            let mismatches = compare_final(&cpu, &memory, &case.final_state);
            if mismatches.is_empty() {
                result.passed += 1;
                case_passed = true;
            } else if result.first_failure.is_none() {
                result.first_failure = Some((case.name, mismatches));
            }
        }
        result.comparisons.record(class, case_passed);
    }

    Ok(result)
}

fn print_result(result: &FixtureResult, mode: SweepMode) {
    println!(
        "  {:<20} {:>5}/{:<5} compared; {:>5} rows",
        result.label, result.passed, result.compared, result.rows
    );
    let excluded = result.exclusions.upstream_quarantine
        + result.exclusions.address_error
        + result.exclusions.divergent_group
        + result.exclusions.stop;
    if excluded > 0 {
        println!(
            "    excluded: upstream={} address-error={} divergent-group={} stop={}",
            result.exclusions.upstream_quarantine,
            result.exclusions.address_error,
            result.exclusions.divergent_group,
            result.exclusions.stop
        );
    }
    if matches!(mode, SweepMode::Full)
        && (result.comparisons.address_error.rows > 0
            || result.comparisons.divergent_group.rows > 0
            || result.comparisons.stop.rows > 0)
    {
        println!(
            "    categories: agreement={}/{} address-error={}/{} divergent-group={}/{} stop={}/{}",
            result.comparisons.agreement.passed,
            result.comparisons.agreement.rows,
            result.comparisons.address_error.passed,
            result.comparisons.address_error.rows,
            result.comparisons.divergent_group.passed,
            result.comparisons.divergent_group.rows,
            result.comparisons.stop.passed,
            result.comparisons.stop.rows
        );
    }
    if let Some((case_name, mismatches)) = &result.first_failure {
        println!("    first failure: {case_name}");
        for mismatch in mismatches.iter().take(6) {
            println!(
                "      {:<22} expected={:<24} actual={}",
                mismatch.field, mismatch.expected, mismatch.actual
            );
        }
        if mismatches.len() > 6 {
            println!("      ... and {} more", mismatches.len() - 6);
        }
    }
}

fn run_sweep(mode: SweepMode) {
    let root =
        fixture_root().unwrap_or_else(|error| panic!("fixture root resolution failed: {error}"));
    assert!(
        root.is_dir(),
        "fixture directory not found at {}; set EMU198X_68000_MAME_ROOT",
        root.display()
    );

    let mut fixtures: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("failed to read fixture root {}: {error}", root.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| {
                    panic!("failed to read an entry in {}: {error}", root.display())
                })
                .path()
        })
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".json.bin"))
        })
        .collect();
    fixtures.sort();
    assert_eq!(
        fixtures.len(),
        EXPECTED_FIXTURE_FILES,
        "registered corpus fixture count changed in {}",
        root.display()
    );

    println!();
    println!("Fixture root: {}", root.display());
    println!(
        "SingleStepTests/m68000 {} sweep:",
        match mode {
            SweepMode::Agreement => "agreement",
            SweepMode::Full => "full comparison",
        }
    );

    let mut total_rows = 0;
    let mut total_compared = 0;
    let mut total_passed = 0;
    let mut exclusions = ExclusionCounts::default();
    let mut comparisons = ComparisonCounts::default();
    let mut fixtures_with_failures = 0;

    for path in &fixtures {
        let result = run_fixture(path, mode)
            .unwrap_or_else(|error| panic!("fixture execution failed: {error}"));
        print_result(&result, mode);
        total_rows += result.rows;
        total_compared += result.compared;
        total_passed += result.passed;
        exclusions.upstream_quarantine += result.exclusions.upstream_quarantine;
        exclusions.address_error += result.exclusions.address_error;
        exclusions.divergent_group += result.exclusions.divergent_group;
        exclusions.stop += result.exclusions.stop;
        comparisons.add(&result.comparisons);
        if result.passed != result.compared {
            fixtures_with_failures += 1;
        }
    }

    println!();
    println!("TOTAL ROWS: {total_rows}");
    println!("  compared and passing: {total_passed}/{total_compared}");
    println!(
        "  excluded: upstream={} address-error={} divergent-group={} stop={}",
        exclusions.upstream_quarantine,
        exclusions.address_error,
        exclusions.divergent_group,
        exclusions.stop
    );
    println!("  fixtures with comparison failures: {fixtures_with_failures}");
    if matches!(mode, SweepMode::Full) {
        println!(
            "  comparison categories: agreement={}/{} address-error={}/{} divergent-group={}/{} stop={}/{}",
            comparisons.agreement.passed,
            comparisons.agreement.rows,
            comparisons.address_error.passed,
            comparisons.address_error.rows,
            comparisons.divergent_group.passed,
            comparisons.divergent_group.rows,
            comparisons.stop.passed,
            comparisons.stop.rows
        );
    }

    assert_eq!(total_rows, EXPECTED_CORPUS_ROWS, "corpus row count changed");
    assert_eq!(
        exclusions.upstream_quarantine, EXPECTED_UPSTREAM_QUARANTINE_ROWS,
        "producer-declared quarantine changed"
    );

    match mode {
        SweepMode::Agreement => {
            assert_eq!(
                exclusions.address_error, EXPECTED_ADDRESS_ERROR_ROWS,
                "address-error evidence boundary changed"
            );
            assert_eq!(
                exclusions.divergent_group, EXPECTED_DIVERGENT_GROUP_ROWS,
                "divergent-group evidence boundary changed"
            );
            assert_eq!(
                exclusions.stop, EXPECTED_STOP_ROWS,
                "STOP evidence boundary changed"
            );
            assert_eq!(
                total_compared, EXPECTED_AGREEMENT_ROWS,
                "agreement denominator changed"
            );
        }
        SweepMode::Full => {
            assert_eq!(exclusions.address_error, 0);
            assert_eq!(exclusions.divergent_group, 0);
            assert_eq!(exclusions.stop, 0);
            assert_eq!(
                total_compared, EXPECTED_FULL_COMPARISON_ROWS,
                "full-comparison denominator changed"
            );
            assert_eq!(
                comparisons.agreement.rows, EXPECTED_AGREEMENT_ROWS,
                "agreement-lane partition changed"
            );
            assert_eq!(
                comparisons.address_error.rows, EXPECTED_ADDRESS_ERROR_ROWS,
                "address-error partition changed"
            );
            assert_eq!(
                comparisons.divergent_group.rows, EXPECTED_DIVERGENT_GROUP_ROWS,
                "divergent-group partition changed"
            );
            assert_eq!(
                comparisons.stop.rows, EXPECTED_STOP_ROWS,
                "STOP partition changed"
            );
        }
    }

    assert_eq!(
        total_passed, total_compared,
        "SingleStepTests/m68000 comparison failures were reported above"
    );
    assert_eq!(
        fixtures_with_failures, 0,
        "one or more compared fixture files did not pass completely"
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddressErrorOutcome {
    CompleteAgreement,
    FrameProgramCounterOnly,
    FrameProgramCounterAndAddressRegister,
    FrameProgramCounterAndDataRegister,
    FrameStatusRegisterAndProgramCounter,
    FrameAccessInformationInstructionRegisterAndProgramCounter,
}

impl AddressErrorOutcome {
    const fn fingerprint_code(self) -> u8 {
        match self {
            Self::CompleteAgreement => 0,
            Self::FrameProgramCounterOnly => 1,
            Self::FrameProgramCounterAndAddressRegister => 2,
            Self::FrameProgramCounterAndDataRegister => 3,
            Self::FrameStatusRegisterAndProgramCounter => 4,
            Self::FrameAccessInformationInstructionRegisterAndProgramCounter => 5,
        }
    }
}

fn classify_address_error_outcome(
    comparison: &AddressErrorObservationComparison,
    frame_matches_observation: bool,
    final_differences: SelectedFinalDifferences,
) -> Result<AddressErrorOutcome, String> {
    if !comparison.event_matches
        || !comparison.frame_fault_address_matches
        || !frame_matches_observation
    {
        return Err("address-error common invariant failed".to_owned());
    }

    let prepared = PreparedFrameDifferences::from_comparison(comparison);
    match prepared {
        PreparedFrameDifferences::NONE if final_differences == SelectedFinalDifferences::NONE => {
            Ok(AddressErrorOutcome::CompleteAgreement)
        }
        PreparedFrameDifferences::PROGRAM_COUNTER
            if final_differences == SelectedFinalDifferences::NONE =>
        {
            Ok(AddressErrorOutcome::FrameProgramCounterOnly)
        }
        PreparedFrameDifferences::PROGRAM_COUNTER
            if final_differences.is_one_address_register_only() =>
        {
            Ok(AddressErrorOutcome::FrameProgramCounterAndAddressRegister)
        }
        PreparedFrameDifferences::PROGRAM_COUNTER
            if final_differences.is_one_data_register_only() =>
        {
            Ok(AddressErrorOutcome::FrameProgramCounterAndDataRegister)
        }
        PreparedFrameDifferences::STATUS_REGISTER_AND_PROGRAM_COUNTER
            if final_differences == SelectedFinalDifferences::STATUS_REGISTER_ONLY =>
        {
            Ok(AddressErrorOutcome::FrameStatusRegisterAndProgramCounter)
        }
        PreparedFrameDifferences::ACCESS_INFORMATION_INSTRUCTION_REGISTER_AND_PROGRAM_COUNTER
            if final_differences == SelectedFinalDifferences::NONE =>
        {
            Ok(AddressErrorOutcome::FrameAccessInformationInstructionRegisterAndProgramCounter)
        }
        _ => Err(format!(
            "unclassified address-error outcome: prepared={prepared:?} final={final_differences:?}"
        )),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AddressErrorOutcomeCounts {
    complete_agreement: usize,
    frame_program_counter_only: usize,
    frame_program_counter_and_address_register: usize,
    frame_program_counter_and_data_register: usize,
    frame_status_register_and_program_counter: usize,
    frame_access_information_instruction_register_and_program_counter: usize,
}

impl AddressErrorOutcomeCounts {
    fn record(&mut self, outcome: AddressErrorOutcome) {
        match outcome {
            AddressErrorOutcome::CompleteAgreement => self.complete_agreement += 1,
            AddressErrorOutcome::FrameProgramCounterOnly => {
                self.frame_program_counter_only += 1;
            }
            AddressErrorOutcome::FrameProgramCounterAndAddressRegister => {
                self.frame_program_counter_and_address_register += 1;
            }
            AddressErrorOutcome::FrameProgramCounterAndDataRegister => {
                self.frame_program_counter_and_data_register += 1;
            }
            AddressErrorOutcome::FrameStatusRegisterAndProgramCounter => {
                self.frame_status_register_and_program_counter += 1;
            }
            AddressErrorOutcome::FrameAccessInformationInstructionRegisterAndProgramCounter => {
                self.frame_access_information_instruction_register_and_program_counter += 1;
            }
        }
    }

    fn add(&mut self, other: Self) {
        self.complete_agreement += other.complete_agreement;
        self.frame_program_counter_only += other.frame_program_counter_only;
        self.frame_program_counter_and_address_register +=
            other.frame_program_counter_and_address_register;
        self.frame_program_counter_and_data_register +=
            other.frame_program_counter_and_data_register;
        self.frame_status_register_and_program_counter +=
            other.frame_status_register_and_program_counter;
        self.frame_access_information_instruction_register_and_program_counter +=
            other.frame_access_information_instruction_register_and_program_counter;
    }

    const fn total(self) -> usize {
        self.complete_agreement
            + self.frame_program_counter_only
            + self.frame_program_counter_and_address_register
            + self.frame_program_counter_and_data_register
            + self.frame_status_register_and_program_counter
            + self.frame_access_information_instruction_register_and_program_counter
    }
}

const EXPECTED_ADDRESS_ERROR_OUTCOMES: AddressErrorOutcomeCounts = AddressErrorOutcomeCounts {
    complete_agreement: 17_689,
    frame_program_counter_only: 32_071,
    frame_program_counter_and_address_register: 4_842,
    frame_program_counter_and_data_register: 632,
    frame_status_register_and_program_counter: 252,
    frame_access_information_instruction_register_and_program_counter: 120,
};

const EXPECTED_READ_ADDRESS_ERROR_OUTCOMES: AddressErrorOutcomeCounts = AddressErrorOutcomeCounts {
    complete_agreement: 17_689,
    frame_program_counter_only: 30_105,
    frame_program_counter_and_address_register: 4_734,
    frame_program_counter_and_data_register: 632,
    frame_status_register_and_program_counter: 0,
    frame_access_information_instruction_register_and_program_counter: 0,
};

const EXPECTED_WRITE_ADDRESS_ERROR_OUTCOMES: AddressErrorOutcomeCounts =
    AddressErrorOutcomeCounts {
        complete_agreement: 0,
        frame_program_counter_only: 1_966,
        frame_program_counter_and_address_register: 108,
        frame_program_counter_and_data_register: 0,
        frame_status_register_and_program_counter: 252,
        frame_access_information_instruction_register_and_program_counter: 120,
    };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProgramCounterDelta {
    MinusTwo,
    MinusFour,
    OddTransferTarget,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProgramCounterDeltaCounts {
    minus_two: usize,
    minus_four: usize,
    odd_transfer_target: usize,
}

impl ProgramCounterDeltaCounts {
    fn record(&mut self, delta: ProgramCounterDelta) {
        match delta {
            ProgramCounterDelta::MinusTwo => self.minus_two += 1,
            ProgramCounterDelta::MinusFour => self.minus_four += 1,
            ProgramCounterDelta::OddTransferTarget => self.odd_transfer_target += 1,
        }
    }

    fn add(&mut self, other: Self) {
        self.minus_two += other.minus_two;
        self.minus_four += other.minus_four;
        self.odd_transfer_target += other.odd_transfer_target;
    }

    const fn total(self) -> usize {
        self.minus_two + self.minus_four + self.odd_transfer_target
    }
}

const EXPECTED_ADDRESS_ERROR_PC_DELTAS: ProgramCounterDeltaCounts = ProgramCounterDeltaCounts {
    minus_two: 19_126,
    minus_four: 12_036,
    odd_transfer_target: 6_755,
};
const EXPECTED_READ_ADDRESS_ERROR_PC_DELTAS: ProgramCounterDeltaCounts =
    ProgramCounterDeltaCounts {
        minus_two: 18_296,
        minus_four: 10_420,
        odd_transfer_target: 6_755,
    };
const EXPECTED_WRITE_ADDRESS_ERROR_PC_DELTAS: ProgramCounterDeltaCounts =
    ProgramCounterDeltaCounts {
        minus_two: 830,
        minus_four: 1_616,
        odd_transfer_target: 0,
    };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TaxonomyFingerprint(u64);

impl Default for TaxonomyFingerprint {
    fn default() -> Self {
        Self(0xCBF2_9CE4_8422_2325)
    }
}

impl TaxonomyFingerprint {
    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    fn write_u16(&mut self, value: u16) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    fn write_str(&mut self, value: &str) {
        self.write_u64(value.len() as u64);
        self.write_bytes(value.as_bytes());
    }
}

const EXPECTED_ADDRESS_ERROR_TAXONOMY_FINGERPRINT: TaxonomyFingerprint =
    TaxonomyFingerprint(0xEFBD_F3E9_3D42_81CA);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestAddressRelation {
    SameAsFrame,
    FrameIsRequestPlusTwo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RequestAddressRelationCounts {
    same_as_frame: usize,
    frame_is_request_plus_two: usize,
}

impl RequestAddressRelationCounts {
    fn record(&mut self, relation: RequestAddressRelation) {
        match relation {
            RequestAddressRelation::SameAsFrame => self.same_as_frame += 1,
            RequestAddressRelation::FrameIsRequestPlusTwo => {
                self.frame_is_request_plus_two += 1;
            }
        }
    }

    fn add(&mut self, other: Self) {
        self.same_as_frame += other.same_as_frame;
        self.frame_is_request_plus_two += other.frame_is_request_plus_two;
    }

    const fn total(self) -> usize {
        self.same_as_frame + self.frame_is_request_plus_two
    }
}

const EXPECTED_ADDRESS_ERROR_REQUEST_RELATIONS: RequestAddressRelationCounts =
    RequestAddressRelationCounts {
        same_as_frame: 53_732,
        frame_is_request_plus_two: 1_874,
    };
const EXPECTED_READ_ADDRESS_ERROR_REQUEST_RELATIONS: RequestAddressRelationCounts =
    RequestAddressRelationCounts {
        same_as_frame: 51_504,
        frame_is_request_plus_two: 1_656,
    };
const EXPECTED_WRITE_ADDRESS_ERROR_REQUEST_RELATIONS: RequestAddressRelationCounts =
    RequestAddressRelationCounts {
        same_as_frame: 2_228,
        frame_is_request_plus_two: 218,
    };

#[derive(Default)]
struct AddressErrorOutcomeTaxonomy {
    total: AddressErrorOutcomeCounts,
    read: AddressErrorOutcomeCounts,
    write: AddressErrorOutcomeCounts,
    pc_deltas: ProgramCounterDeltaCounts,
    read_pc_deltas: ProgramCounterDeltaCounts,
    write_pc_deltas: ProgramCounterDeltaCounts,
    request_relations: RequestAddressRelationCounts,
    read_request_relations: RequestAddressRelationCounts,
    write_request_relations: RequestAddressRelationCounts,
    fingerprint: TaxonomyFingerprint,
}

struct AddressErrorOutcomeRecord<'a> {
    case_name: &'a str,
    access: SourceAddressErrorAccess,
    outcome: AddressErrorOutcome,
    observation: AddressErrorObservation,
    expected_frame: AddressErrorFrame,
    final_differences: SelectedFinalDifferences,
    cpu: &'a Cpu68000,
}

impl AddressErrorOutcomeTaxonomy {
    fn record(&mut self, record: AddressErrorOutcomeRecord<'_>) -> Result<(), String> {
        let AddressErrorOutcomeRecord {
            case_name,
            access,
            outcome,
            observation,
            expected_frame,
            final_differences,
            cpu,
        } = record;
        let request_relation = match observation
            .frame_fault_address
            .wrapping_sub(observation.requested_address)
        {
            0 => RequestAddressRelation::SameAsFrame,
            2 => RequestAddressRelation::FrameIsRequestPlusTwo,
            delta => {
                return Err(format!(
                    "unclassified requested-to-frame address delta {delta:#010X}"
                ));
            }
        };
        let pc_delta = if outcome == AddressErrorOutcome::CompleteAgreement {
            None
        } else {
            let delta = observation
                .frame_pc
                .wrapping_sub(expected_frame.program_counter) as i32;
            Some(match delta {
                -2 => ProgramCounterDelta::MinusTwo,
                -4 => ProgramCounterDelta::MinusFour,
                _ if delta & 1 != 0
                    && access == SourceAddressErrorAccess::Read
                    && request_relation == RequestAddressRelation::SameAsFrame
                    && matches!(
                        observation.function_code,
                        FunctionCode::UserProgram | FunctionCode::SupervisorProgram
                    )
                    && observation.requested_address & 1 != 0
                    && observation.frame_pc == observation.requested_address.wrapping_sub(4) =>
                {
                    ProgramCounterDelta::OddTransferTarget
                }
                _ => return Err(format!("unclassified prepared-PC delta {delta}")),
            })
        };

        self.fingerprint.write_str(case_name);
        self.fingerprint.write_u8(match access {
            SourceAddressErrorAccess::Read => 0,
            SourceAddressErrorAccess::Write => 1,
        });
        self.fingerprint.write_u8(outcome.fingerprint_code());
        self.fingerprint.write_u32(observation.requested_address);
        self.fingerprint.write_u32(observation.frame_fault_address);
        self.fingerprint.write_u16(observation.access_information);
        self.fingerprint.write_u16(observation.saved_sr);
        self.fingerprint.write_u16(observation.frame_ir);
        self.fingerprint.write_u32(observation.frame_pc);
        self.fingerprint
            .write_u16(expected_frame.access_information);
        self.fingerprint.write_u32(expected_frame.fault_address);
        self.fingerprint
            .write_u16(expected_frame.instruction_register);
        self.fingerprint.write_u16(expected_frame.status_register);
        self.fingerprint.write_u32(expected_frame.program_counter);
        self.fingerprint.write_u8(final_differences.data_registers);
        self.fingerprint
            .write_u8(final_differences.address_registers);
        self.fingerprint
            .write_bool(final_differences.user_stack_pointer);
        self.fingerprint
            .write_bool(final_differences.supervisor_stack_pointer);
        self.fingerprint
            .write_bool(final_differences.status_register);
        self.fingerprint
            .write_bool(final_differences.normalised_program_counter);
        self.fingerprint.write_u8(final_differences.prefetch_words);
        self.fingerprint
            .write_u64(final_differences.non_frame_memory_words as u64);
        self.fingerprint
            .write_bool(final_differences.execution_boundary);
        for value in cpu.regs.d {
            self.fingerprint.write_u32(value);
        }
        for value in cpu.regs.a {
            self.fingerprint.write_u32(value);
        }
        self.fingerprint.write_u32(cpu.regs.usp);
        self.fingerprint.write_u32(cpu.regs.ssp);
        self.fingerprint.write_u16(cpu.regs.sr);
        self.fingerprint.write_u32(cpu.regs.pc);
        self.fingerprint.write_u32(cpu.instr_start_pc);
        self.fingerprint.write_u16(cpu.ir);
        self.fingerprint.write_u16(cpu.irc);
        self.fingerprint.write_u8(match request_relation {
            RequestAddressRelation::SameAsFrame => 0,
            RequestAddressRelation::FrameIsRequestPlusTwo => 1,
        });
        self.fingerprint.write_u8(match pc_delta {
            None => 0,
            Some(ProgramCounterDelta::MinusTwo) => 1,
            Some(ProgramCounterDelta::MinusFour) => 2,
            Some(ProgramCounterDelta::OddTransferTarget) => 3,
        });

        self.total.record(outcome);
        self.request_relations.record(request_relation);
        if let Some(delta) = pc_delta {
            self.pc_deltas.record(delta);
        }
        match access {
            SourceAddressErrorAccess::Read => {
                self.read.record(outcome);
                self.read_request_relations.record(request_relation);
                if let Some(delta) = pc_delta {
                    self.read_pc_deltas.record(delta);
                }
            }
            SourceAddressErrorAccess::Write => {
                self.write.record(outcome);
                self.write_request_relations.record(request_relation);
                if let Some(delta) = pc_delta {
                    self.write_pc_deltas.record(delta);
                }
            }
        }
        Ok(())
    }

    fn add(&mut self, other: &Self) {
        self.total.add(other.total);
        self.read.add(other.read);
        self.write.add(other.write);
        self.pc_deltas.add(other.pc_deltas);
        self.read_pc_deltas.add(other.read_pc_deltas);
        self.write_pc_deltas.add(other.write_pc_deltas);
        self.request_relations.add(other.request_relations);
        self.read_request_relations
            .add(other.read_request_relations);
        self.write_request_relations
            .add(other.write_request_relations);
        self.fingerprint.write_u64(other.fingerprint.0);
    }
}

#[derive(Default)]
struct AddressErrorKindCounts {
    rows: usize,
    event_matches: usize,
    prepared_frame_matches: usize,
    frame_memory_matches: usize,
    final_state_matches: usize,
    complete_matches: usize,
}

impl AddressErrorKindCounts {
    fn record(
        &mut self,
        event_matches: bool,
        prepared_frame_matches: bool,
        frame_memory_matches: bool,
        final_state_matches: bool,
    ) {
        self.rows += 1;
        self.event_matches += usize::from(event_matches);
        self.prepared_frame_matches += usize::from(prepared_frame_matches);
        self.frame_memory_matches += usize::from(frame_memory_matches);
        self.final_state_matches += usize::from(final_state_matches);
        self.complete_matches += usize::from(
            event_matches && prepared_frame_matches && frame_memory_matches && final_state_matches,
        );
    }

    fn add(&mut self, other: &Self) {
        self.rows += other.rows;
        self.event_matches += other.event_matches;
        self.prepared_frame_matches += other.prepared_frame_matches;
        self.frame_memory_matches += other.frame_memory_matches;
        self.final_state_matches += other.final_state_matches;
        self.complete_matches += other.complete_matches;
    }
}

#[derive(Default)]
struct AddressErrorCounts {
    source_events: usize,
    source_four_cycles: usize,
    source_even_address_bus: usize,
    source_word_lanes: usize,
    source_frame_address_link: usize,
    source_transfer_information_link: usize,
    source_instruction_processing_link: usize,
    observed: usize,
    direction_matches: usize,
    requested_address_bus_matches: usize,
    frame_address_bus_matches: usize,
    function_code_matches: usize,
    event_matches: usize,
    access_information_matches: usize,
    access_information_low_matches: usize,
    frame_fault_address_matches: usize,
    frame_ir_matches: usize,
    frame_sr_matches: usize,
    frame_pc_matches: usize,
    prepared_frame_matches: usize,
    frame_memory_matches: usize,
    final_state_matches: usize,
    complete_matches: usize,
    read: AddressErrorKindCounts,
    write: AddressErrorKindCounts,
}

impl AddressErrorCounts {
    fn record(
        &mut self,
        source: AddressErrorTransaction,
        comparison: &AddressErrorObservationComparison,
        frame_memory_matches: bool,
        final_state_matches: bool,
    ) {
        self.source_events += 1;
        self.source_four_cycles += usize::from(comparison.source_four_cycles);
        self.source_even_address_bus += usize::from(comparison.source_even_address_bus);
        self.source_word_lanes += usize::from(comparison.source_word_lanes);
        self.source_frame_address_link += usize::from(comparison.source_frame_address_link);
        self.source_transfer_information_link +=
            usize::from(comparison.source_transfer_information_link);
        self.source_instruction_processing_link +=
            usize::from(comparison.source_instruction_processing_link);
        self.observed += usize::from(comparison.observed);
        self.direction_matches += usize::from(comparison.direction_matches);
        self.requested_address_bus_matches += usize::from(comparison.requested_address_bus_matches);
        self.frame_address_bus_matches += usize::from(comparison.frame_address_bus_matches);
        self.function_code_matches += usize::from(comparison.function_code_matches);
        self.event_matches += usize::from(comparison.event_matches);
        self.access_information_matches += usize::from(comparison.access_information_matches);
        self.access_information_low_matches +=
            usize::from(comparison.access_information_low_matches);
        self.frame_fault_address_matches += usize::from(comparison.frame_fault_address_matches);
        self.frame_ir_matches += usize::from(comparison.frame_ir_matches);
        self.frame_sr_matches += usize::from(comparison.frame_sr_matches);
        self.frame_pc_matches += usize::from(comparison.frame_pc_matches);
        self.prepared_frame_matches += usize::from(comparison.prepared_frame_matches);
        self.frame_memory_matches += usize::from(frame_memory_matches);
        self.final_state_matches += usize::from(final_state_matches);
        let complete_matches = comparison.event_matches
            && comparison.prepared_frame_matches
            && frame_memory_matches
            && final_state_matches;
        self.complete_matches += usize::from(complete_matches);

        let kind = match source.access {
            SourceAddressErrorAccess::Read => &mut self.read,
            SourceAddressErrorAccess::Write => &mut self.write,
        };
        kind.record(
            comparison.event_matches,
            comparison.prepared_frame_matches,
            frame_memory_matches,
            final_state_matches,
        );
    }

    fn add(&mut self, other: &Self) {
        self.source_events += other.source_events;
        self.source_four_cycles += other.source_four_cycles;
        self.source_even_address_bus += other.source_even_address_bus;
        self.source_word_lanes += other.source_word_lanes;
        self.source_frame_address_link += other.source_frame_address_link;
        self.source_transfer_information_link += other.source_transfer_information_link;
        self.source_instruction_processing_link += other.source_instruction_processing_link;
        self.observed += other.observed;
        self.direction_matches += other.direction_matches;
        self.requested_address_bus_matches += other.requested_address_bus_matches;
        self.frame_address_bus_matches += other.frame_address_bus_matches;
        self.function_code_matches += other.function_code_matches;
        self.event_matches += other.event_matches;
        self.access_information_matches += other.access_information_matches;
        self.access_information_low_matches += other.access_information_low_matches;
        self.frame_fault_address_matches += other.frame_fault_address_matches;
        self.frame_ir_matches += other.frame_ir_matches;
        self.frame_sr_matches += other.frame_sr_matches;
        self.frame_pc_matches += other.frame_pc_matches;
        self.prepared_frame_matches += other.prepared_frame_matches;
        self.frame_memory_matches += other.frame_memory_matches;
        self.final_state_matches += other.final_state_matches;
        self.complete_matches += other.complete_matches;
        self.read.add(&other.read);
        self.write.add(&other.write);
    }
}

struct AddressErrorFixtureResult {
    label: String,
    rows: usize,
    counts: AddressErrorCounts,
    taxonomy: AddressErrorOutcomeTaxonomy,
    first_event_failure: Option<(String, Vec<Mismatch>)>,
    first_prepared_frame_failure: Option<(String, Vec<Mismatch>)>,
    first_final_state_failure: Option<(String, Vec<Mismatch>)>,
}

fn run_address_error_fixture(path: &Path) -> Result<AddressErrorFixtureResult, String> {
    let label = fixture_label(path)?;
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let cases = parse_fixture(&bytes)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    if cases.len() != EXPECTED_ROWS_PER_FILE {
        return Err(format!(
            "{} contains {} rows; expected {EXPECTED_ROWS_PER_FILE}",
            path.display(),
            cases.len()
        ));
    }

    let mut result = AddressErrorFixtureResult {
        label,
        rows: cases.len(),
        counts: AddressErrorCounts::default(),
        taxonomy: AddressErrorOutcomeTaxonomy::default(),
        first_event_failure: None,
        first_prepared_frame_failure: None,
        first_final_state_failure: None,
    };

    for case in cases {
        if case.address_error_transactions.is_empty() {
            continue;
        }
        let [source] = case.address_error_transactions.as_slice() else {
            return Err(format!(
                "{} contains {} address-error transactions; expected exactly one",
                case.name,
                case.address_error_transactions.len()
            ));
        };
        let source = *source;
        let expected_frame = expected_address_error_frame(&case.final_state)
            .map_err(|error| format!("{}: {error}", case.name))?;

        let mut cpu = Cpu68000::new();
        let mut memory = SparseMem::with_word_capacity(case.initial.ram.len());
        apply_initial(&mut cpu, &mut memory, &case.initial);
        let instruction_completed = run_one_instruction(&mut cpu, &mut memory);
        let completed = instruction_completed && complete_final_prefetch(&mut cpu, &mut memory);
        let observation = cpu.take_address_error_observation();
        let comparison = compare_address_error_observation(source, expected_frame, observation);
        let frame_memory_mismatches =
            compare_frame_memory(&memory, &case.final_state, expected_frame);
        let frame_memory_matches = completed && frame_memory_mismatches.is_empty();
        let final_state_mismatches = if completed {
            compare_address_error_final(&cpu, &memory, &case.final_state)
        } else {
            vec![Mismatch {
                field: "execution boundary".to_owned(),
                expected: "one completed instruction with two-word final prefetch".to_owned(),
                actual: format!("boundary not reached within {MAX_TICKS_PER_CASE} ticks per phase"),
            }]
        };
        let final_state_matches = final_state_mismatches.is_empty();
        let observation = observation
            .ok_or_else(|| format!("{} did not expose an address-error observation", case.name))?;
        let final_differences =
            selected_final_differences(&cpu, &memory, &case.final_state, completed);
        let outcome = classify_address_error_outcome(
            &comparison,
            frame_memory_matches_observation(&memory, &case.final_state, observation),
            final_differences,
        )
        .map_err(|error| format!("{}: {error}", case.name))?;
        result
            .taxonomy
            .record(AddressErrorOutcomeRecord {
                case_name: &case.name,
                access: source.access,
                outcome,
                observation,
                expected_frame,
                final_differences,
                cpu: &cpu,
            })
            .map_err(|error| format!("{}: {error}", case.name))?;

        if !comparison.event_matches && result.first_event_failure.is_none() {
            result.first_event_failure = Some((case.name.clone(), comparison.mismatches.clone()));
        }
        if !comparison.prepared_frame_matches && result.first_prepared_frame_failure.is_none() {
            result.first_prepared_frame_failure =
                Some((case.name.clone(), comparison.mismatches.clone()));
        }
        if !final_state_matches && result.first_final_state_failure.is_none() {
            let mut mismatches = frame_memory_mismatches;
            mismatches.extend(final_state_mismatches);
            result.first_final_state_failure = Some((case.name.clone(), mismatches));
        }

        result.counts.record(
            source,
            &comparison,
            frame_memory_matches,
            final_state_matches,
        );
    }

    Ok(result)
}

fn print_address_error_failure(label: &str, failure: &Option<(String, Vec<Mismatch>)>) {
    if let Some((case_name, mismatches)) = failure {
        println!("    first {label}: {case_name}");
        for mismatch in mismatches.iter().take(6) {
            println!(
                "      {:<28} expected={:<24} actual={}",
                mismatch.field, mismatch.expected, mismatch.actual
            );
        }
        if mismatches.len() > 6 {
            println!("      ... and {} more", mismatches.len() - 6);
        }
    }
}

fn run_address_error_sweep() {
    let root =
        fixture_root().unwrap_or_else(|error| panic!("fixture root resolution failed: {error}"));
    assert!(
        root.is_dir(),
        "fixture directory not found at {}; set EMU198X_68000_MAME_ROOT",
        root.display()
    );

    let mut fixtures: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("failed to read fixture root {}: {error}", root.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| {
                    panic!("failed to read an entry in {}: {error}", root.display())
                })
                .path()
        })
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".json.bin"))
        })
        .collect();
    fixtures.sort();
    assert_eq!(
        fixtures.len(),
        EXPECTED_FIXTURE_FILES,
        "registered corpus fixture count changed in {}",
        root.display()
    );

    println!();
    println!("Fixture root: {}", root.display());
    println!("SingleStepTests/m68000 address-error event sweep:");

    let mut total_rows = 0;
    let mut fixtures_with_address_errors = 0;
    let mut counts = AddressErrorCounts::default();
    let mut taxonomy = AddressErrorOutcomeTaxonomy::default();

    for path in &fixtures {
        let result = run_address_error_fixture(path)
            .unwrap_or_else(|error| panic!("fixture execution failed: {error}"));
        total_rows += result.rows;
        if result.counts.source_events == 0 {
            continue;
        }
        fixtures_with_address_errors += 1;
        println!(
            "  {:<20} event={:>4}/{:<4} prepared-frame={:>4}/{:<4} frame-memory={:>4}/{:<4} final-state={:>4}/{:<4} complete={:>4}/{:<4}",
            result.label,
            result.counts.event_matches,
            result.counts.source_events,
            result.counts.prepared_frame_matches,
            result.counts.source_events,
            result.counts.frame_memory_matches,
            result.counts.source_events,
            result.counts.final_state_matches,
            result.counts.source_events,
            result.counts.complete_matches,
            result.counts.source_events,
        );
        print_address_error_failure("event mismatch", &result.first_event_failure);
        print_address_error_failure(
            "prepared-frame mismatch",
            &result.first_prepared_frame_failure,
        );
        print_address_error_failure("final-state mismatch", &result.first_final_state_failure);
        counts.add(&result.counts);
        taxonomy.add(&result.taxonomy);
    }

    println!();
    println!("TOTAL CORPUS ROWS PARSED: {total_rows}");
    println!("  fixture groups with address-error events: {fixtures_with_address_errors}");
    println!(
        "  source events: total={} re={} we={}",
        counts.source_events, counts.read.rows, counts.write.rows
    );
    println!(
        "  source invariants: four-cycle={}/{} even-address-bus={}/{} word-lanes={}/{} event/frame-address={}/{} R/W+FC={}/{} I/N-normal={}/{}",
        counts.source_four_cycles,
        counts.source_events,
        counts.source_even_address_bus,
        counts.source_events,
        counts.source_word_lanes,
        counts.source_events,
        counts.source_frame_address_link,
        counts.source_events,
        counts.source_transfer_information_link,
        counts.source_events,
        counts.source_instruction_processing_link,
        counts.source_events,
    );
    println!(
        "  core event: observed={}/{} direction={}/{} frame-address-bus={}/{} function-code={}/{} complete={}/{}",
        counts.observed,
        counts.source_events,
        counts.direction_matches,
        counts.source_events,
        counts.frame_address_bus_matches,
        counts.source_events,
        counts.function_code_matches,
        counts.source_events,
        counts.event_matches,
        counts.source_events,
    );
    println!(
        "  core abstract requested-address bus: {}/{} matches source event",
        counts.requested_address_bus_matches, counts.source_events
    );
    println!(
        "  prepared frame fields: access-info-low={}/{} access-info-full={}/{} fault-address={}/{} ir={}/{} sr={}/{} pc={}/{} all={}/{}",
        counts.access_information_low_matches,
        counts.source_events,
        counts.access_information_matches,
        counts.source_events,
        counts.frame_fault_address_matches,
        counts.source_events,
        counts.frame_ir_matches,
        counts.source_events,
        counts.frame_sr_matches,
        counts.source_events,
        counts.frame_pc_matches,
        counts.source_events,
        counts.prepared_frame_matches,
        counts.source_events,
    );
    println!(
        "  resulting state: frame-memory={}/{} selected-final-state={}/{} complete-intersection={}/{}",
        counts.frame_memory_matches,
        counts.source_events,
        counts.final_state_matches,
        counts.source_events,
        counts.complete_matches,
        counts.source_events,
    );
    println!(
        "  re: event={}/{} prepared-frame={}/{} frame-memory={}/{} final-state={}/{} complete={}/{}",
        counts.read.event_matches,
        counts.read.rows,
        counts.read.prepared_frame_matches,
        counts.read.rows,
        counts.read.frame_memory_matches,
        counts.read.rows,
        counts.read.final_state_matches,
        counts.read.rows,
        counts.read.complete_matches,
        counts.read.rows,
    );
    println!(
        "  we: event={}/{} prepared-frame={}/{} frame-memory={}/{} final-state={}/{} complete={}/{}",
        counts.write.event_matches,
        counts.write.rows,
        counts.write.prepared_frame_matches,
        counts.write.rows,
        counts.write.frame_memory_matches,
        counts.write.rows,
        counts.write.final_state_matches,
        counts.write.rows,
        counts.write.complete_matches,
        counts.write.rows,
    );
    println!("  mutually exclusive prepared/final outcome taxonomy:");
    println!(
        "    complete={} pc-only={} pc+address-register={} pc+data-register={} sr+pc={} access+ir+pc={}",
        taxonomy.total.complete_agreement,
        taxonomy.total.frame_program_counter_only,
        taxonomy.total.frame_program_counter_and_address_register,
        taxonomy.total.frame_program_counter_and_data_register,
        taxonomy.total.frame_status_register_and_program_counter,
        taxonomy
            .total
            .frame_access_information_instruction_register_and_program_counter,
    );
    println!(
        "    PC deltas among non-complete rows: -2={} -4={} odd-transfer-target={}",
        taxonomy.pc_deltas.minus_two,
        taxonomy.pc_deltas.minus_four,
        taxonomy.pc_deltas.odd_transfer_target,
    );
    println!(
        "    raw request relation: same-as-frame={} frame-is-request+2={}",
        taxonomy.request_relations.same_as_frame,
        taxonomy.request_relations.frame_is_request_plus_two,
    );
    println!(
        "    row-stable taxonomy fingerprint: {:016x}",
        taxonomy.fingerprint.0
    );
    println!("  unmeasured: no-AS data bus, address-strobe timing, ordered normal transactions");

    assert_eq!(total_rows, EXPECTED_CORPUS_ROWS, "corpus row count changed");
    assert_eq!(
        fixtures_with_address_errors, EXPECTED_ADDRESS_ERROR_FIXTURE_FILES,
        "address-error fixture-group count changed"
    );
    assert_eq!(
        counts.source_events, EXPECTED_ADDRESS_ERROR_ROWS,
        "address-error event count changed"
    );
    assert_eq!(
        counts.read.rows, EXPECTED_READ_ADDRESS_ERROR_ROWS,
        "read-address-error event count changed"
    );
    assert_eq!(
        counts.write.rows, EXPECTED_WRITE_ADDRESS_ERROR_ROWS,
        "write-address-error event count changed"
    );
    assert_eq!(
        taxonomy.total, EXPECTED_ADDRESS_ERROR_OUTCOMES,
        "address-error outcome taxonomy changed"
    );
    assert_eq!(
        taxonomy.read, EXPECTED_READ_ADDRESS_ERROR_OUTCOMES,
        "read-address-error outcome taxonomy changed"
    );
    assert_eq!(
        taxonomy.write, EXPECTED_WRITE_ADDRESS_ERROR_OUTCOMES,
        "write-address-error outcome taxonomy changed"
    );
    assert_eq!(taxonomy.total.total(), EXPECTED_ADDRESS_ERROR_ROWS);
    assert_eq!(taxonomy.read.total(), EXPECTED_READ_ADDRESS_ERROR_ROWS);
    assert_eq!(taxonomy.write.total(), EXPECTED_WRITE_ADDRESS_ERROR_ROWS);
    assert_eq!(taxonomy.total.complete_agreement, counts.complete_matches);
    assert_eq!(taxonomy.pc_deltas, EXPECTED_ADDRESS_ERROR_PC_DELTAS);
    assert_eq!(
        taxonomy.read_pc_deltas,
        EXPECTED_READ_ADDRESS_ERROR_PC_DELTAS
    );
    assert_eq!(
        taxonomy.write_pc_deltas,
        EXPECTED_WRITE_ADDRESS_ERROR_PC_DELTAS
    );
    assert_eq!(taxonomy.pc_deltas.total(), 37_917);
    assert_eq!(
        taxonomy.request_relations,
        EXPECTED_ADDRESS_ERROR_REQUEST_RELATIONS
    );
    assert_eq!(
        taxonomy.read_request_relations,
        EXPECTED_READ_ADDRESS_ERROR_REQUEST_RELATIONS
    );
    assert_eq!(
        taxonomy.write_request_relations,
        EXPECTED_WRITE_ADDRESS_ERROR_REQUEST_RELATIONS
    );
    assert_eq!(
        taxonomy.request_relations.total(),
        EXPECTED_ADDRESS_ERROR_ROWS
    );
    assert_eq!(
        taxonomy.fingerprint, EXPECTED_ADDRESS_ERROR_TAXONOMY_FINGERPRINT,
        "address-error row-level taxonomy changed"
    );
    assert_eq!(
        counts.access_information_matches,
        EXPECTED_ADDRESS_ERROR_ACCESS_INFORMATION_MATCHES
    );
    assert_eq!(counts.frame_ir_matches, EXPECTED_ADDRESS_ERROR_IR_MATCHES);
    assert_eq!(counts.frame_sr_matches, EXPECTED_ADDRESS_ERROR_SR_MATCHES);
    assert_eq!(counts.frame_pc_matches, EXPECTED_ADDRESS_ERROR_PC_MATCHES);
    for (field, matched) in [
        (
            "source four-cycle classification",
            counts.source_four_cycles,
        ),
        ("source even address bus", counts.source_even_address_bus),
        ("source word lanes", counts.source_word_lanes),
        (
            "source event/frame address link",
            counts.source_frame_address_link,
        ),
        (
            "source event R/W and function-code link",
            counts.source_transfer_information_link,
        ),
        (
            "source normal-instruction I/N state",
            counts.source_instruction_processing_link,
        ),
        ("core observation presence", counts.observed),
        (
            "core low-five access-information bits",
            counts.access_information_low_matches,
        ),
        ("core event direction", counts.direction_matches),
        (
            "core event frame address bus",
            counts.frame_address_bus_matches,
        ),
        ("core event function code", counts.function_code_matches),
        ("core complete event", counts.event_matches),
    ] {
        assert_eq!(
            matched, EXPECTED_ADDRESS_ERROR_ROWS,
            "{field} agreement changed"
        );
    }
}

/// Parse the complete registered corpus and compare the explicitly bounded
/// 240,090-row agreement subset.
#[test]
#[ignore]
fn mame_agreement_sweep() {
    run_sweep(SweepMode::Agreement);
}

/// Compare every producer-endorsed row. This diagnostic remains red while
/// unresolved software-oracle disagreements exist.
#[test]
#[ignore]
fn mame_full_comparison() {
    run_sweep(SweepMode::Full);
}

/// Compare every source-designated address-error event with the core's
/// rejected-transfer observation. Frame and final-state differences are
/// reported independently and do not make this event-focused gate red.
#[test]
#[ignore]
fn mame_address_error_event_sweep() {
    run_address_error_sweep();
}
