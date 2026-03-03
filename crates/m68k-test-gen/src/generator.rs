//! Test vector generator: sets up random state, single-steps Musashi,
//! captures before/after snapshots.
//!
//! Musashi runs with `EMULATE_PREFETCH ON` (single-word lookahead).
//! After executing one instruction, its state maps to our DL convention:
//!
//!   DL PC   = Musashi PC + 4
//!   DL IR   = Musashi PREF_DATA  (next instruction's opcode)
//!   DL IRC  = word at Musashi PC + 2
//!
//! The test format stores Musashi's raw values:
//! - `pc` = next instruction address (Musashi convention)
//! - `prefetch` = [IR, PREF_DATA] (Musashi's instruction register + lookahead)
//!
//! The test runner applies the +4 offset and derives IRC from memory.

use rand::Rng;
use std::ffi::c_uint;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::instructions::{InstructionDef, InstructionSetup};
use crate::memory;
use crate::musashi;
use crate::testcase::{CpuState, TestCase};

// --- Instruction hook for single-stepping ---
//
// Called by Musashi before each instruction. We always call
// end_timeslice(), which zeroes the remaining cycles. The current
// instruction still completes (Musashi checks the loop condition
// AFTER the instruction runs), but the loop then exits. Result:
// exactly one instruction executes per execute() call.

static HOOK_COUNT: AtomicU32 = AtomicU32::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn testgen_instruction_hook(_pc: c_uint) {
    HOOK_COUNT.fetch_add(1, Ordering::SeqCst);
    musashi::end_timeslice();
}

/// Memory regions used by the test generator.
///
/// We partition 16 MB into zones to avoid code/stack/data overlap:
/// - Code: 0x001000..0x002000 (4 KB)
/// - Stack: 0x010000..0x011000 (4 KB, grows down from 0x011000)
/// - Data: 0x100000..0x200000 (1 MB, for EA operands)
const CODE_BASE: u32 = 0x001000;
const STACK_TOP: u32 = 0x011000;
const DATA_BASE: u32 = 0x100000;
const DATA_END: u32 = 0x200000;

/// Generate `count` test cases for one instruction definition.
pub fn generate(def: &InstructionDef, cpu_type: u32, count: usize) -> Vec<TestCase> {
    let mut rng = rand::rng();
    let mut tests = Vec::with_capacity(count);

    musashi::init();

    for i in 0..count {
        let test = generate_one(def, cpu_type, &mut rng, i);
        tests.push(test);
    }

    tests
}

fn generate_one(
    def: &InstructionDef,
    cpu_type: u32,
    rng: &mut impl Rng,
    index: usize,
) -> TestCase {
    memory::clear();

    // Encode the instruction at CODE_BASE
    encode_instruction(def, CODE_BASE, rng);

    // Fill area after instruction with NOPs so Musashi doesn't crash
    // if the instruction reads extension words or the execute loop
    // tries to read ahead.
    let after_instr = CODE_BASE + 2 + u32::from(def.ext_words) * 2;
    for offset in 0..8 {
        memory::poke_word(after_instr + offset * 2, 0x4E71); // NOP
    }

    // Set up exception vectors so address errors don't crash.
    // Vector 3 (address error) → a NOP sled at 0x002000
    for v in 0..64 {
        memory::poke_long(v * 4, STACK_TOP); // All vectors point to NOP sled
    }
    // Except: SSP at vector 0, PC at vector 1 (but we don't pulse_reset)
    memory::poke_long(0x000000, STACK_TOP);
    memory::poke_long(0x000004, CODE_BASE);

    // Randomise register values
    let sr = random_sr(rng, cpu_type);
    let mut d = [0u32; 8];
    let mut a = [0u32; 7];
    for reg in &mut d {
        *reg = rng.random();
    }
    for reg in &mut a {
        *reg = random_data_addr(rng);
    }
    let usp = random_stack_addr(rng);

    // Set CPU type and load registers into Musashi
    musashi::set_cpu_type(cpu_type);

    for i in 0..8 {
        musashi::set_reg(musashi::M68K_REG_D0 + i as u32, d[i]);
    }
    for i in 0..7 {
        musashi::set_reg(musashi::M68K_REG_A0 + i as u32, a[i]);
    }
    musashi::set_reg(musashi::M68K_REG_USP, usp);
    musashi::set_reg(musashi::M68K_REG_ISP, STACK_TOP);
    musashi::set_reg(musashi::M68K_REG_SR, u32::from(sr));
    musashi::set_reg(musashi::M68K_REG_PC, CODE_BASE);

    if cpu_type >= musashi::M68K_CPU_TYPE_68020 {
        musashi::set_reg(musashi::M68K_REG_MSP, 0);
        musashi::set_reg(musashi::M68K_REG_VBR, 0);
        musashi::set_reg(musashi::M68K_REG_CACR, 0);
        musashi::set_reg(musashi::M68K_REG_CAAR, 0);
    }

    // Capture initial state
    let initial = capture_state(cpu_type);

    // Single-step one instruction
    HOOK_COUNT.store(0, Ordering::SeqCst);
    memory::reset_writes();
    let cycles_used = musashi::execute(10000) as u32;

    // Capture final state
    let final_state = capture_state_with_writes(cpu_type, &initial);

    let name = format!("{} #{index} sr={:04X}", def.name, sr);

    TestCase {
        name,
        initial,
        final_state,
        cycles: cycles_used,
    }
}

/// Encode an instruction at the given PC.
fn encode_instruction(def: &InstructionDef, pc: u32, _rng: &mut impl Rng) {
    match def.setup {
        InstructionSetup::Fixed => {
            memory::poke_word(pc, def.opcode);
        }
        InstructionSetup::EaBits0 | InstructionSetup::EaSrcDst | InstructionSetup::Custom => {
            // TODO (Phase 2): randomise EA fields
            memory::poke_word(pc, def.opcode);
        }
    }
}

/// Generate a random SR value.
fn random_sr(rng: &mut impl Rng, _cpu_type: u32) -> u16 {
    let ccr: u8 = rng.random_range(0..=0x1F);
    let int_mask: u8 = rng.random_range(0..=7);
    // S=1, T=0, M=0 — supervisor mode, no trace
    0x2000 | (u16::from(int_mask) << 8) | u16::from(ccr)
}

/// Capture current Musashi state.
fn capture_state(cpu_type: u32) -> CpuState {
    let mut d = [0u32; 8];
    let mut a = [0u32; 7];

    for i in 0..8 {
        d[i] = musashi::get_reg(musashi::M68K_REG_D0 + i as u32);
    }
    for i in 0..7 {
        a[i] = musashi::get_reg(musashi::M68K_REG_A0 + i as u32);
    }

    let sr = musashi::get_reg(musashi::M68K_REG_SR) as u16;
    let pc = musashi::get_reg(musashi::M68K_REG_PC);
    let ir = musashi::get_reg(musashi::M68K_REG_IR) as u16;
    let pref_data = musashi::get_reg(musashi::M68K_REG_PREF_DATA) as u16;

    let (msp, vbr, cacr, caar) = if cpu_type >= musashi::M68K_CPU_TYPE_68020 {
        (
            musashi::get_reg(musashi::M68K_REG_MSP),
            musashi::get_reg(musashi::M68K_REG_VBR),
            musashi::get_reg(musashi::M68K_REG_CACR),
            musashi::get_reg(musashi::M68K_REG_CAAR),
        )
    } else {
        (0, 0, 0, 0)
    };

    let ram = memory::snapshot_tracked();

    CpuState {
        d,
        a,
        usp: musashi::get_reg(musashi::M68K_REG_USP),
        ssp: musashi::get_reg(musashi::M68K_REG_ISP),
        sr,
        pc,
        // Musashi prefetch state: [IR, PREF_DATA].
        // The test runner maps these to our DL convention:
        //   DL PC  = pc + 4
        //   DL IR  = pref_data (next instruction's opcode)
        //   DL IRC = word at (pc + 2), read from final RAM
        prefetch: [ir, pref_data],
        ram,
        msp,
        vbr,
        cacr,
        caar,
    }
}

/// Capture final state with RAM snapshot that covers initial + written addresses.
fn capture_state_with_writes(cpu_type: u32, initial: &CpuState) -> CpuState {
    let mut state = capture_state(cpu_type);

    let writes = memory::take_writes();
    let mut addrs: Vec<u32> = initial.ram.iter().map(|&(a, _)| a).collect();
    for &(a, _) in &writes {
        if !addrs.contains(&a) {
            addrs.push(a);
        }
    }
    addrs.sort_unstable();
    state.ram = memory::snapshot_addrs(&addrs);

    state
}

/// Generate an even-aligned address in the data region.
fn random_data_addr(rng: &mut impl Rng) -> u32 {
    let addr: u32 = rng.random_range(DATA_BASE..DATA_END);
    addr & !1 // Even-align
}

/// Generate an even-aligned address for a user stack.
fn random_stack_addr(rng: &mut impl Rng) -> u32 {
    let addr: u32 = rng.random_range(0x010800..STACK_TOP);
    addr & !1
}
