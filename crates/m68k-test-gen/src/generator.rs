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

/// Metadata returned by encode_instruction for memory EA modes.
/// Used after register randomisation to seed test data at the computed EA.
struct MemoryEAInfo {
    mode: u8,
    /// An register number (modes 2-6) or mode-7 sub-type (0=abs.W, 1=abs.L, 2=d16PC, 3=idxPC).
    reg: u8,
    size: u8,
    ext_word: Option<u16>,
    /// For abs.L mode: the full 32-bit address.
    abs_long_addr: Option<u32>,
    /// PC value at the extension word (for PC-relative EA computation).
    ext_word_pc: u32,
    /// For MOVEM: the register mask word.
    movem_mask: Option<u16>,
    cpu_type: u32,
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
    let ea_info = encode_instruction(def, CODE_BASE, cpu_type, rng);

    // Fill area after instruction with NOPs so Musashi doesn't crash
    // if the instruction reads extension words or the execute loop
    // tries to read ahead.
    let after_instr = CODE_BASE + 2 + u32::from(def.ext_words) * 2;
    for offset in 0..8 {
        memory::poke_word(after_instr + offset * 2, 0x4E71); // NOP
    }

    // Set up exception vectors and handler NOP sled.
    // Exception handler target at 0x002000 (NOP sled).
    let exc_handler: u32 = 0x002000;
    for offset in 0..16 {
        memory::poke_word(exc_handler + offset * 2, 0x4E71); // NOP sled
    }
    for v in 0..64 {
        memory::poke_long(v * 4, exc_handler);
    }
    // Vector 0 = initial SSP, vector 1 = initial PC (for pulse_reset)
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

    // For word/long memory EA, ensure the effective address is even.
    // Musashi doesn't emulate address errors, so odd EAs cause 100%
    // mismatches (our CPU takes an exception, Musashi doesn't).
    if let Some(ref info) = ea_info
        && info.size >= 2 {
            ensure_even_ea(info, &mut d, &mut a, STACK_TOP);
        }

    // Seed test data at the memory EA so address-computation bugs are
    // detectable (wrong address reads zero instead of the seeded value).
    if let Some(ref info) = ea_info {
        seed_ea_data(info, &d, &a, STACK_TOP, rng);
    }

    // Set CPU type and load registers into Musashi
    musashi::set_cpu_type(cpu_type);

    for (i, &val) in d.iter().enumerate() {
        musashi::set_reg(musashi::M68K_REG_D0 + i as u32, val);
    }
    for (i, &val) in a.iter().enumerate() {
        musashi::set_reg(musashi::M68K_REG_A0 + i as u32, val);
    }
    musashi::set_reg(musashi::M68K_REG_USP, usp);
    musashi::set_reg(musashi::M68K_REG_ISP, STACK_TOP);
    musashi::set_reg(musashi::M68K_REG_SR, u32::from(sr));
    musashi::set_reg(musashi::M68K_REG_PC, CODE_BASE);

    if cpu_type >= musashi::M68K_CPU_TYPE_68EC020 {
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
///
/// Returns `Some(MemoryEAInfo)` for memory-EA setups so that `generate_one`
/// can seed test data at the computed effective address after registers are
/// randomised.
fn encode_instruction(
    def: &InstructionDef,
    pc: u32,
    cpu_type: u32,
    rng: &mut impl Rng,
) -> Option<MemoryEAInfo> {
    memory::poke_word(pc, def.opcode);
    match def.setup {
        InstructionSetup::Fixed => None,
        InstructionSetup::RandExt1 => {
            let ext: u16 = rng.random();
            memory::poke_word(pc.wrapping_add(2), ext);
            None
        }
        InstructionSetup::NeedsStack => {
            // Place a valid even return address on the stack so RTS/RTD
            // can pop it without taking an address error. The return
            // address points to a NOP sled at 0x002000.
            let ret_addr: u32 = 0x002000;
            // NOP sled at return target
            for offset in 0..8 {
                memory::poke_word(ret_addr + offset * 2, 0x4E71); // NOP
            }
            // The 68000 stack: SP points at the top item. Pop reads from
            // SP and increments. Write the return address at STACK_TOP
            // (where SP points) and set ISP to STACK_TOP. But Musashi
            // was told ISP = STACK_TOP by the register setup, so write
            // the return address starting at STACK_TOP.
            let sp = STACK_TOP;
            memory::poke_word(sp, (ret_addr >> 16) as u16);       // hi word at SP
            memory::poke_word(sp.wrapping_add(2), (ret_addr & 0xFFFF) as u16); // lo word at SP+2
            // Write extension words for the instruction
            for ext_idx in 0..def.ext_words {
                let ext: u16 = rng.random();
                memory::poke_word(pc.wrapping_add(2 + u32::from(ext_idx) * 2), ext);
            }
            None
        }
        InstructionSetup::Custom => None,
        InstructionSetup::MemoryEA { size } => {
            let ea_mode = ((def.opcode >> 3) & 7) as u8;
            let ea_reg = (def.opcode & 7) as u8;
            let (ext_word, abs_long_addr) = encode_memory_ea(pc, ea_mode, ea_reg, 2, cpu_type, rng);
            Some(MemoryEAInfo {
                mode: ea_mode, reg: ea_reg, size, ext_word, abs_long_addr,
                ext_word_pc: pc + 2, movem_mask: None, cpu_type,
            })
        }
        InstructionSetup::MemoryEADst { size } => {
            let ea_mode = ((def.opcode >> 6) & 7) as u8;
            let ea_reg = ((def.opcode >> 9) & 7) as u8;
            let (ext_word, abs_long_addr) = encode_memory_ea(pc, ea_mode, ea_reg, 2, cpu_type, rng);
            Some(MemoryEAInfo {
                mode: ea_mode, reg: ea_reg, size, ext_word, abs_long_addr,
                ext_word_pc: pc + 2, movem_mask: None, cpu_type,
            })
        }
        InstructionSetup::ImmMemoryEA { imm_words, size } => {
            // Write random immediate word(s) after opcode
            for i in 0..imm_words {
                let imm: u16 = rng.random();
                memory::poke_word(pc + 2 + u32::from(i) * 2, imm);
            }
            // EA extension word follows the immediate
            let ea_mode = ((def.opcode >> 3) & 7) as u8;
            let ea_reg = (def.opcode & 7) as u8;
            let ext_offset = 2 + u32::from(imm_words) * 2;
            let (ext_word, abs_long_addr) = encode_memory_ea(pc, ea_mode, ea_reg, ext_offset, cpu_type, rng);
            Some(MemoryEAInfo {
                mode: ea_mode, reg: ea_reg, size, ext_word, abs_long_addr,
                ext_word_pc: pc + ext_offset, movem_mask: None, cpu_type,
            })
        }
        InstructionSetup::Movem { size } => {
            // Register mask at pc+2
            let mask: u16 = rng.random();
            memory::poke_word(pc + 2, mask);
            // EA from bits 5-0; extension words start at pc+4
            let ea_mode = ((def.opcode >> 3) & 7) as u8;
            let ea_reg = (def.opcode & 7) as u8;
            let (ext_word, abs_long_addr) = encode_memory_ea(pc, ea_mode, ea_reg, 4, cpu_type, rng);
            Some(MemoryEAInfo {
                mode: ea_mode, reg: ea_reg, size, ext_word, abs_long_addr,
                ext_word_pc: pc + 4, movem_mask: Some(mask), cpu_type,
            })
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

    for (i, reg) in d.iter_mut().enumerate() {
        *reg = musashi::get_reg(musashi::M68K_REG_D0 + i as u32);
    }
    for (i, reg) in a.iter_mut().enumerate() {
        *reg = musashi::get_reg(musashi::M68K_REG_A0 + i as u32);
    }

    let sr = musashi::get_reg(musashi::M68K_REG_SR) as u16;
    let pc = musashi::get_reg(musashi::M68K_REG_PC);
    let ir = musashi::get_reg(musashi::M68K_REG_IR) as u16;
    let pref_data = musashi::get_reg(musashi::M68K_REG_PREF_DATA) as u16;

    let (msp, vbr, cacr, caar) = if cpu_type >= musashi::M68K_CPU_TYPE_68EC020 {
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

// --- Memory EA helpers ---

/// Generate extension word(s) for a memory EA mode and write at `pc + ext_offset`.
///
/// Returns `(ext_word, abs_long_addr)`:
/// - `ext_word`: the 16-bit extension word (d16, brief, abs.W address, etc.)
/// - `abs_long_addr`: for abs.L mode, the full 32-bit address
fn encode_memory_ea(
    pc: u32,
    ea_mode: u8,
    ea_reg: u8,
    ext_offset: u32,
    cpu_type: u32,
    rng: &mut impl Rng,
) -> (Option<u16>, Option<u32>) {
    let ext_addr = pc + ext_offset;
    match ea_mode {
        0b010..=0b100 => (None, None), // (An), (An)+, -(An)
        0b101 => {
            // d16(An): random 16-bit signed displacement
            let d16: u16 = rng.random();
            memory::poke_word(ext_addr, d16);
            (Some(d16), None)
        }
        0b110 => {
            // d8(An,Xn): brief extension word
            let brief = generate_brief_ext_word(cpu_type, rng);
            memory::poke_word(ext_addr, brief);
            (Some(brief), None)
        }
        0b111 => match ea_reg {
            0 => {
                // abs.W: even address in $3000-$7FFE (avoids vectors/code/stack)
                let addr = rng.random_range(0x3000u32..0x7FFFu32) & !1;
                memory::poke_word(ext_addr, addr as u16);
                (Some(addr as u16), None)
            }
            1 => {
                // abs.L: full 32-bit address in data region
                let addr = random_data_addr(rng);
                memory::poke_word(ext_addr, (addr >> 16) as u16);
                memory::poke_word(ext_addr + 2, (addr & 0xFFFF) as u16);
                (None, Some(addr))
            }
            2 => {
                // d16(PC): compute displacement to land in $3000-$7FFE
                let target = rng.random_range(0x3000u32..0x7FFFu32) & !1;
                let d16 = target.wrapping_sub(ext_addr) as u16;
                memory::poke_word(ext_addr, d16);
                (Some(d16), None)
            }
            3 => {
                // d8(PC,Xn): brief extension word (index register dominates EA)
                let brief = generate_brief_ext_word(cpu_type, rng);
                memory::poke_word(ext_addr, brief);
                (Some(brief), None)
            }
            _ => (None, None),
        },
        _ => (None, None),
    }
}

/// Generate a brief extension word for d8(An,Xn) addressing.
///
/// Format: D/A | Reg(3) | W/L | Scale(2) | 0 | d8(8)
/// - D/A: 0=data register, 1=address register
/// - Reg: index register number (0-7)
/// - W/L: 0=word (sign-extended), 1=long
/// - Scale: 0-3 on 68020+ (×1/×2/×4/×8), always 0 on 68000
/// - d8: signed 8-bit displacement
fn generate_brief_ext_word(cpu_type: u32, rng: &mut impl Rng) -> u16 {
    let da: u16 = rng.random_range(0..=1);
    let reg: u16 = rng.random_range(0..=7);
    let wl: u16 = rng.random_range(0..=1);
    let scale: u16 = if cpu_type >= musashi::M68K_CPU_TYPE_68EC020 {
        rng.random_range(0..=3)
    } else {
        0
    };
    // Even d8 prevents odd EAs when index register equals base register.
    // For byte-sized ops this is harmless; for word/long it avoids address
    // errors that Musashi doesn't emulate.
    let d8: u16 = u16::from(rng.random::<u8>()) & 0xFE;

    (da << 15) | (reg << 12) | (wl << 11) | (scale << 9) | d8
}

/// Read An (or SSP for A7 in supervisor mode).
fn reg_an(reg: u8, a: &[u32; 7], ssp: u32) -> u32 {
    if (reg as usize) < 7 { a[reg as usize] } else { ssp }
}

/// Compute the effective address from MemoryEAInfo and register values.
fn compute_ea(
    info: &MemoryEAInfo,
    d: &[u32; 8],
    a: &[u32; 7],
    ssp: u32,
) -> Option<u32> {
    match info.mode {
        0b010 | 0b011 => Some(reg_an(info.reg, a, ssp)),         // (An), (An)+
        0b100 => {
            // -(An): pre-decrement
            let an = reg_an(info.reg, a, ssp);
            Some(an.wrapping_sub(u32::from(info.size)))
        }
        0b101 => {
            // d16(An)
            let an = reg_an(info.reg, a, ssp);
            let d16 = info.ext_word? as i16;
            Some(an.wrapping_add(d16 as i32 as u32))
        }
        0b110 => {
            // d8(An,Xn)
            let an = reg_an(info.reg, a, ssp);
            let brief = info.ext_word?;
            Some(compute_indexed_ea(brief, an, d, a, ssp, info.cpu_type))
        }
        0b111 => match info.reg {
            0 => {
                // abs.W: sign-extend 16-bit address
                let addr = info.ext_word? as i16 as i32 as u32;
                Some(addr)
            }
            1 => {
                // abs.L: full 32-bit address
                Some(info.abs_long_addr?)
            }
            2 => {
                // d16(PC): PC at ext word + sign-extend(d16)
                let d16 = info.ext_word? as i16;
                Some(info.ext_word_pc.wrapping_add(d16 as i32 as u32))
            }
            3 => {
                // d8(PC,Xn): indexed from PC at ext word
                let brief = info.ext_word?;
                Some(compute_indexed_ea(brief, info.ext_word_pc, d, a, ssp, info.cpu_type))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Ensure the effective address is even for word/long operations.
///
/// Musashi doesn't emulate address errors, so when our CPU and Musashi both
/// compute an odd EA, our CPU takes an exception while Musashi proceeds
/// normally. This causes ~50% mismatches for random displacements.
///
/// Fix: toggle the base register's bit 0 (for An-based modes) or the index
/// register's bit 0 (for PC-relative indexed) to flip EA parity.
fn ensure_even_ea(
    info: &MemoryEAInfo,
    d: &mut [u32; 8],
    a: &mut [u32; 7],
    ssp: u32,
) {
    let Some(ea) = compute_ea(info, d, a, ssp) else { return };
    if ea & 1 == 0 { return; }

    match info.mode {
        // An-based modes: toggle An's bit 0
        0b010..=0b110 => {
            if (info.reg as usize) < 7 {
                a[info.reg as usize] ^= 1;
            }
        }
        // Mode 7: only d8(PC,Xn) can produce odd EA from our generators
        0b111 if info.reg == 3 => {
            // Toggle the index register's bit 0
            if let Some(brief) = info.ext_word {
                let da = (brief >> 15) & 1;
                let xn_reg = ((brief >> 12) & 7) as usize;
                if da == 0 {
                    d[xn_reg] ^= 1;
                } else if xn_reg < 7 {
                    a[xn_reg] ^= 1;
                }
            }
        }
        _ => {}
    }
}

/// Seed random test data at the effective address computed from register values.
///
/// Called after register randomisation so the EA computation uses actual
/// register values. Seeding makes address-computation bugs detectable:
/// if the CPU computes a different EA, it reads zero instead of the
/// seeded value, causing a mismatch.
fn seed_ea_data(
    info: &MemoryEAInfo,
    d: &[u32; 8],
    a: &[u32; 7],
    ssp: u32,
    rng: &mut impl Rng,
) {
    let Some(ea) = compute_ea(info, d, a, ssp) else { return };
    let ea = ea & 0x00FF_FFFF;

    // MOVEM: seed enough data for all 16 registers
    if info.movem_mask.is_some() {
        let total = 16 * u32::from(info.size);
        let base = if info.mode == 0b100 {
            // -(An): data extends downward from An
            let an = reg_an(info.reg, a, ssp) & 0x00FF_FFFF;
            an.wrapping_sub(total)
        } else {
            ea
        };
        for offset in (0..total).step_by(2) {
            let addr = base.wrapping_add(offset) & 0x00FF_FFFF;
            memory::poke_word(addr, rng.random());
        }
        return;
    }

    match info.size {
        1 => memory::poke(ea, rng.random()),
        2 => memory::poke_word(ea, rng.random()),
        4 => memory::poke_long(ea, rng.random()),
        _ => {} // size 0 = address-only (LEA, PEA, JMP, JSR)
    }
}

/// Compute effective address for d8(An,Xn) brief extension word.
fn compute_indexed_ea(
    brief: u16,
    an: u32,
    d: &[u32; 8],
    a: &[u32; 7],
    ssp: u32,
    cpu_type: u32,
) -> u32 {
    let da = (brief >> 15) & 1;
    let xn_reg = ((brief >> 12) & 7) as usize;
    let wl = (brief >> 11) & 1;
    let scale = if cpu_type >= musashi::M68K_CPU_TYPE_68EC020 {
        ((brief >> 9) & 3) as u32
    } else {
        0 // 68000 ignores scale bits
    };
    let d8 = (brief & 0xFF) as u8 as i8;

    let xn_value = if da == 0 {
        d[xn_reg]
    } else if xn_reg < 7 {
        a[xn_reg]
    } else {
        ssp
    };

    let index = if wl == 1 {
        xn_value // long: full 32-bit value
    } else {
        (xn_value as u16 as i16 as i32) as u32 // word: sign-extend low 16 bits
    };

    let scaled_index = index.wrapping_mul(1u32 << scale);
    an.wrapping_add(d8 as i32 as u32)
        .wrapping_add(scaled_index)
}
