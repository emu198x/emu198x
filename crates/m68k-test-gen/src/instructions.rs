//! Instruction catalogue for 680x0 test generation.
//!
//! Each entry describes how to encode one instruction variant. Register
//! fields are baked into the opcode (typically D0 source, D1 destination)
//! and register *values* are randomised by the generator.
//!
//! For instructions needing extension words, the setup type tells the
//! generator how many random words to append.

use crate::musashi;

/// An instruction variant to generate tests for.
#[derive(Debug, Clone)]
pub struct InstructionDef {
    /// Human-readable name (used in filenames and test names).
    pub name: &'static str,
    /// Base opcode word.
    pub opcode: u16,
    /// Number of extension words after the opcode.
    pub ext_words: u8,
    /// How to set up the instruction in memory.
    pub setup: InstructionSetup,
    /// Minimum CPU type required.
    pub min_cpu: u32,
}

/// How the generator should set up memory and registers for this instruction.
#[derive(Debug, Clone, Copy)]
pub enum InstructionSetup {
    /// Fixed opcode, no extension words (NOP, SWAP D0, etc.)
    Fixed,
    /// Opcode + 1 random extension word (byte immediate, word immediate).
    RandExt1,
    /// Opcode + 2 random extension words (long immediate).
    RandExt2,
    /// Instruction needs a valid stack frame (RTS, RTE, RTR, UNLK).
    #[allow(dead_code)]
    NeedsStack,
    /// Custom setup (instruction-specific logic).
    #[allow(dead_code)]
    Custom,
    /// Memory EA in bits 5-0. Generator computes EA, seeds test data.
    MemoryEA { size: u8 }, // 1=byte, 2=word, 4=long
    /// Destination EA in bits 11-6 (MOVE encoding). Generator seeds data at EA.
    MemoryEADst { size: u8 },
    /// Immediate value(s) followed by memory EA in bits 5-0.
    /// imm_words=1 for byte/word, 2 for long. EA ext words follow the immediate.
    ImmMemoryEA { imm_words: u8, size: u8 },
    /// MOVEM: register mask at pc+2, then EA ext words at pc+4.
    Movem { size: u8 },
}

const M68K: u32 = musashi::M68K_CPU_TYPE_68000;

/// Helper to define a fixed instruction (no extension words).
const fn fixed(name: &'static str, opcode: u16) -> InstructionDef {
    InstructionDef {
        name,
        opcode,
        ext_words: 0,
        setup: InstructionSetup::Fixed,
        min_cpu: M68K,
    }
}

/// Helper to define an instruction with 1 random extension word.
const fn rand1(name: &'static str, opcode: u16) -> InstructionDef {
    InstructionDef {
        name,
        opcode,
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: M68K,
    }
}

/// Helper to define an instruction with 2 random extension words.
const fn rand2(name: &'static str, opcode: u16) -> InstructionDef {
    InstructionDef {
        name,
        opcode,
        ext_words: 2,
        setup: InstructionSetup::RandExt2,
        min_cpu: M68K,
    }
}

/// Helper to define a memory-EA instruction (EA in bits 5-0).
const fn mem_ea(name: &'static str, opcode: u16, ext_words: u8, size: u8) -> InstructionDef {
    InstructionDef {
        name,
        opcode,
        ext_words,
        setup: InstructionSetup::MemoryEA { size },
        min_cpu: M68K,
    }
}

/// Helper for memory-EA destination (MOVE encoding, EA in bits 11-6).
const fn mem_ea_dst(name: &'static str, opcode: u16, ext_words: u8, size: u8) -> InstructionDef {
    InstructionDef {
        name,
        opcode,
        ext_words,
        setup: InstructionSetup::MemoryEADst { size },
        min_cpu: M68K,
    }
}

/// Helper for immediate-to-memory-EA instructions (ADDI, CMPI, etc.).
const fn imm_mem_ea(
    name: &'static str,
    opcode: u16,
    ext_words: u8,
    imm_words: u8,
    size: u8,
) -> InstructionDef {
    InstructionDef {
        name,
        opcode,
        ext_words,
        setup: InstructionSetup::ImmMemoryEA { imm_words, size },
        min_cpu: M68K,
    }
}

/// Helper for MOVEM instructions.
const fn movem(name: &'static str, opcode: u16, ext_words: u8, size: u8) -> InstructionDef {
    InstructionDef {
        name,
        opcode,
        ext_words,
        setup: InstructionSetup::Movem { size },
        min_cpu: M68K,
    }
}

const M68010: u32 = musashi::M68K_CPU_TYPE_68010;

/// Helper for a 68010+ fixed instruction.
const fn fixed_010(name: &'static str, opcode: u16) -> InstructionDef {
    InstructionDef {
        name,
        opcode,
        ext_words: 0,
        setup: InstructionSetup::Fixed,
        min_cpu: M68010,
    }
}

/// Helper for a 68010+ instruction needing a valid stack frame.
const fn needs_stack_010(name: &'static str, opcode: u16, ext_words: u8) -> InstructionDef {
    InstructionDef {
        name,
        opcode,
        ext_words,
        setup: InstructionSetup::NeedsStack,
        min_cpu: M68010,
    }
}

/// Return all instruction definitions for the given CPU type.
pub fn catalogue(cpu_type: u32) -> Vec<InstructionDef> {
    let mut defs = Vec::new();

    // ===== Fixed instructions (no operands / operands in opcode) =====
    defs.push(fixed("NOP", 0x4E71));

    // --- MOVE.q #imm,Dn (immediate in opcode bits 7:0, Dn in bits 11:9) ---
    // MOVEQ #0,D1 — the immediate will vary via random SR/data, but the
    // instruction effect is "D1 = sign-extend(imm8), set flags".
    // We use D1 and a zero immediate; the generator randomises D1's value.
    defs.push(fixed("MOVE.q", 0x7200)); // MOVEQ #0,D1

    // --- Register-to-register ALU (Dn op Dn → Dn) ---
    // Format: base | (Dn_dst << 9) | (000 << 3) | Dn_src
    // Using D0 as source, D1 as destination.

    // ADD Dn,Dn
    defs.push(fixed("ADD.b", 0xD200)); // ADD.b D0,D1
    defs.push(fixed("ADD.w", 0xD240)); // ADD.w D0,D1
    defs.push(fixed("ADD.l", 0xD280)); // ADD.l D0,D1

    // SUB Dn,Dn
    defs.push(fixed("SUB.b", 0x9200)); // SUB.b D0,D1
    defs.push(fixed("SUB.w", 0x9240)); // SUB.w D0,D1
    defs.push(fixed("SUB.l", 0x9280)); // SUB.l D0,D1

    // AND Dn,Dn (AND <ea>,Dn direction)
    defs.push(fixed("AND.b", 0xC200)); // AND.b D0,D1
    defs.push(fixed("AND.w", 0xC240)); // AND.w D0,D1
    defs.push(fixed("AND.l", 0xC280)); // AND.l D0,D1

    // OR Dn,Dn (OR <ea>,Dn direction)
    defs.push(fixed("OR.b", 0x8200)); // OR.b D0,D1
    defs.push(fixed("OR.w", 0x8240)); // OR.w D0,D1
    defs.push(fixed("OR.l", 0x8280)); // OR.l D0,D1

    // EOR Dn,<ea> (EOR always goes Dn→EA; use D1→D0)
    defs.push(fixed("EOR.b", 0xB300)); // EOR.b D1,D0
    defs.push(fixed("EOR.w", 0xB340)); // EOR.w D1,D0
    defs.push(fixed("EOR.l", 0xB380)); // EOR.l D1,D0

    // CMP Dn,Dn
    defs.push(fixed("CMP.b", 0xB200)); // CMP.b D0,D1
    defs.push(fixed("CMP.w", 0xB240)); // CMP.w D0,D1
    defs.push(fixed("CMP.l", 0xB280)); // CMP.l D0,D1

    // CMPA An,An (CMPA <ea>,An)
    defs.push(fixed("CMPA.w", 0xB2C8)); // CMPA.w A0,A1 (word)
    defs.push(fixed("CMPA.l", 0xB3C8)); // CMPA.l A0,A1 (long)

    // ADDA / SUBA (EA,An)
    defs.push(fixed("ADDA.w", 0xD2C0)); // ADDA.w D0,A1
    defs.push(fixed("ADDA.l", 0xD3C0)); // ADDA.l D0,A1
    defs.push(fixed("SUBA.w", 0x92C0)); // SUBA.w D0,A1
    defs.push(fixed("SUBA.l", 0x93C0)); // SUBA.l D0,A1

    // --- Unary register ops (operate on Dn) ---
    // Format: base | (000 << 3) | Dn
    // Using D0 as operand.

    defs.push(fixed("CLR.b", 0x4200)); // CLR.b D0
    defs.push(fixed("CLR.w", 0x4240)); // CLR.w D0
    defs.push(fixed("CLR.l", 0x4280)); // CLR.l D0

    defs.push(fixed("NEG.b", 0x4400)); // NEG.b D0
    defs.push(fixed("NEG.w", 0x4440)); // NEG.w D0
    defs.push(fixed("NEG.l", 0x4480)); // NEG.l D0

    defs.push(fixed("NEGX.b", 0x4000)); // NEGX.b D0
    defs.push(fixed("NEGX.w", 0x4040)); // NEGX.w D0
    defs.push(fixed("NEGX.l", 0x4080)); // NEGX.l D0

    defs.push(fixed("NOT.b", 0x4600)); // NOT.b D0
    defs.push(fixed("NOT.w", 0x4640)); // NOT.w D0
    defs.push(fixed("NOT.l", 0x4680)); // NOT.l D0

    defs.push(fixed("TST.b", 0x4A00)); // TST.b D0
    defs.push(fixed("TST.w", 0x4A40)); // TST.w D0
    defs.push(fixed("TST.l", 0x4A80)); // TST.l D0

    defs.push(fixed("EXT.w", 0x4880)); // EXT.w D0
    defs.push(fixed("EXT.l", 0x48C0)); // EXT.l D0

    defs.push(fixed("SWAP", 0x4840)); // SWAP D0

    defs.push(fixed("Scc", 0x57C0)); // SEQ D0 (condition = EQ)

    // --- MOVE register-to-register ---
    // MOVE.b D0,D1: 0001 [D1=001] [000] [000] [D0=000]
    defs.push(fixed("MOVE.b", 0x1200)); // MOVE.b D0,D1
    defs.push(fixed("MOVE.w", 0x3200)); // MOVE.w D0,D1
    defs.push(fixed("MOVE.l", 0x2200)); // MOVE.l D0,D1

    // MOVEA Dn,An
    defs.push(fixed("MOVEA.w", 0x3240)); // MOVEA.w D0,A1
    defs.push(fixed("MOVEA.l", 0x2240)); // MOVEA.l D0,A1

    // --- Shifts and rotates (register count, Dn) ---
    // Format: 1110 [count/Dn] [direction] [size] [i/r] [type] [Dn]
    // Using shift count = 1 (encoded as Dn=D1), register D0.
    // count=1 is bits 11:9 = 001.

    // ASL/ASR: type = 00
    defs.push(fixed("ASL.b", 0xE300)); // ASL.b #1,D0
    defs.push(fixed("ASL.w", 0xE340)); // ASL.w #1,D0
    defs.push(fixed("ASL.l", 0xE380)); // ASL.l #1,D0
    defs.push(fixed("ASR.b", 0xE200)); // ASR.b #1,D0
    defs.push(fixed("ASR.w", 0xE240)); // ASR.w #1,D0
    defs.push(fixed("ASR.l", 0xE280)); // ASR.l #1,D0

    // LSL/LSR: type = 01
    defs.push(fixed("LSL.b", 0xE308)); // LSL.b #1,D0
    defs.push(fixed("LSL.w", 0xE348)); // LSL.w #1,D0
    defs.push(fixed("LSL.l", 0xE388)); // LSL.l #1,D0
    defs.push(fixed("LSR.b", 0xE208)); // LSR.b #1,D0
    defs.push(fixed("LSR.w", 0xE248)); // LSR.w #1,D0
    defs.push(fixed("LSR.l", 0xE288)); // LSR.l #1,D0

    // ROL/ROR: type = 11
    defs.push(fixed("ROL.b", 0xE318)); // ROL.b #1,D0
    defs.push(fixed("ROL.w", 0xE358)); // ROL.w #1,D0
    defs.push(fixed("ROL.l", 0xE398)); // ROL.l #1,D0
    defs.push(fixed("ROR.b", 0xE218)); // ROR.b #1,D0
    defs.push(fixed("ROR.w", 0xE258)); // ROR.w #1,D0
    defs.push(fixed("ROR.l", 0xE298)); // ROR.l #1,D0

    // ROXL/ROXR: type = 10
    defs.push(fixed("ROXL.b", 0xE310)); // ROXL.b #1,D0
    defs.push(fixed("ROXL.w", 0xE350)); // ROXL.w #1,D0
    defs.push(fixed("ROXL.l", 0xE390)); // ROXL.l #1,D0
    defs.push(fixed("ROXR.b", 0xE210)); // ROXR.b #1,D0
    defs.push(fixed("ROXR.w", 0xE250)); // ROXR.w #1,D0
    defs.push(fixed("ROXR.l", 0xE290)); // ROXR.l #1,D0

    // --- BCD register-to-register ---
    defs.push(fixed("ABCD", 0xC300)); // ABCD D0,D1
    defs.push(fixed("SBCD", 0x8300)); // SBCD D0,D1
    defs.push(fixed("NBCD", 0x4800)); // NBCD D0

    // --- Bit operations (register, Dn) ---
    // BTST Dn,Dn / BCHG Dn,Dn / BCLR Dn,Dn / BSET Dn,Dn
    defs.push(fixed("BTST", 0x0300)); // BTST D1,D0
    defs.push(fixed("BCHG", 0x0340)); // BCHG D1,D0
    defs.push(fixed("BCLR", 0x0380)); // BCLR D1,D0
    defs.push(fixed("BSET", 0x03C0)); // BSET D1,D0

    // --- MUL/DIV (Dn,Dn) ---
    defs.push(fixed("MULU", 0xC2C0)); // MULU D0,D1
    defs.push(fixed("MULS", 0xC3C0)); // MULS D0,D1
    // DIVU/DIVS: risk of division by zero when D0=0.
    // The generator randomises D0 so ~1/2^32 chance of zero — acceptable.
    // Division by zero takes an exception vector, which we set up.
    defs.push(fixed("DIVU", 0x82C0)); // DIVU D0,D1
    defs.push(fixed("DIVS", 0x83C0)); // DIVS D0,D1

    // --- EXG ---
    defs.push(fixed("EXG", 0xC141)); // EXG D0,D1

    // --- ADDX/SUBX register-to-register ---
    defs.push(fixed("ADDX.b", 0xD300)); // ADDX.b D0,D1
    defs.push(fixed("ADDX.w", 0xD340)); // ADDX.w D0,D1
    defs.push(fixed("ADDX.l", 0xD380)); // ADDX.l D0,D1
    defs.push(fixed("SUBX.b", 0x9300)); // SUBX.b D0,D1
    defs.push(fixed("SUBX.w", 0x9340)); // SUBX.w D0,D1
    defs.push(fixed("SUBX.l", 0x9380)); // SUBX.l D0,D1

    // --- TAS ---
    defs.push(fixed("TAS", 0x4AC0)); // TAS D0

    // --- MOVE to/from SR/CCR/USP ---
    defs.push(fixed("MOVEfromSR", 0x40C0)); // MOVE SR,D0
    defs.push(fixed("MOVEtoCCR", 0x44C0)); // MOVE D0,CCR
    defs.push(fixed("MOVEtoSR", 0x46C0)); // MOVE D0,SR

    // --- Immediate to CCR/SR (need 1 extension word) ---
    defs.push(rand1("ANDItoCCR", 0x023C));
    defs.push(rand1("ORItoCCR", 0x003C));
    defs.push(rand1("EORItoCCR", 0x0A3C));
    defs.push(rand1("ANDItoSR", 0x027C));
    defs.push(rand1("ORItoSR", 0x007C));
    defs.push(rand1("EORItoSR", 0x0A7C));

    // --- CHK (Dn,Dn) ---
    defs.push(fixed("CHK", 0x4380)); // CHK D0,D1

    // ===== Memory EA instructions =====
    //
    // These test address computation, bus read/write ordering, prefetch
    // consumption, and register side-effects for each EA mode.

    // --- Source EA: ADD <ea>,D1 ---
    // (An) indirect
    defs.push(mem_ea("ADD.b_(A0)_D1", 0xD210, 0, 1));
    defs.push(mem_ea("ADD.w_(A0)_D1", 0xD250, 0, 2));
    defs.push(mem_ea("ADD.l_(A0)_D1", 0xD290, 0, 4));
    // (An)+ post-increment
    defs.push(mem_ea("ADD.b_(A0)+_D1", 0xD218, 0, 1));
    defs.push(mem_ea("ADD.w_(A0)+_D1", 0xD258, 0, 2));
    defs.push(mem_ea("ADD.l_(A0)+_D1", 0xD298, 0, 4));
    // -(An) pre-decrement
    defs.push(mem_ea("ADD.b_-(A0)_D1", 0xD220, 0, 1));
    defs.push(mem_ea("ADD.w_-(A0)_D1", 0xD260, 0, 2));
    defs.push(mem_ea("ADD.l_-(A0)_D1", 0xD2A0, 0, 4));
    // d16(An) displacement
    defs.push(mem_ea("ADD.b_d16(A0)_D1", 0xD228, 1, 1));
    defs.push(mem_ea("ADD.w_d16(A0)_D1", 0xD268, 1, 2));
    defs.push(mem_ea("ADD.l_d16(A0)_D1", 0xD2A8, 1, 4));
    // d8(An,Xn) indexed
    defs.push(mem_ea("ADD.b_idx(A0)_D1", 0xD230, 1, 1));
    defs.push(mem_ea("ADD.w_idx(A0)_D1", 0xD270, 1, 2));
    defs.push(mem_ea("ADD.l_idx(A0)_D1", 0xD2B0, 1, 4));

    // --- Destination EA: CLR <ea> ---
    // (An) indirect
    defs.push(mem_ea("CLR.b_(A0)", 0x4210, 0, 1));
    defs.push(mem_ea("CLR.w_(A0)", 0x4250, 0, 2));
    defs.push(mem_ea("CLR.l_(A0)", 0x4290, 0, 4));
    // (An)+
    defs.push(mem_ea("CLR.b_(A0)+", 0x4218, 0, 1));
    defs.push(mem_ea("CLR.w_(A0)+", 0x4258, 0, 2));
    defs.push(mem_ea("CLR.l_(A0)+", 0x4298, 0, 4));
    // -(An)
    defs.push(mem_ea("CLR.b_-(A0)", 0x4220, 0, 1));
    defs.push(mem_ea("CLR.w_-(A0)", 0x4260, 0, 2));
    defs.push(mem_ea("CLR.l_-(A0)", 0x42A0, 0, 4));
    // d16(An)
    defs.push(mem_ea("CLR.b_d16(A0)", 0x4228, 1, 1));
    defs.push(mem_ea("CLR.w_d16(A0)", 0x4268, 1, 2));
    defs.push(mem_ea("CLR.l_d16(A0)", 0x42A8, 1, 4));
    // d8(An,Xn)
    defs.push(mem_ea("CLR.b_idx(A0)", 0x4230, 1, 1));
    defs.push(mem_ea("CLR.w_idx(A0)", 0x4270, 1, 2));
    defs.push(mem_ea("CLR.l_idx(A0)", 0x42B0, 1, 4));

    // --- Read-modify-write: ADD D0,<ea> (Dn→EA direction) ---
    // Uses A1 to avoid conflict with source-EA entries using A0.
    // (A1)
    defs.push(mem_ea("ADD.b_D0_(A1)", 0xD111, 0, 1));
    defs.push(mem_ea("ADD.w_D0_(A1)", 0xD151, 0, 2));
    defs.push(mem_ea("ADD.l_D0_(A1)", 0xD191, 0, 4));
    // d16(A1)
    defs.push(mem_ea("ADD.b_D0_d16(A1)", 0xD129, 1, 1));
    defs.push(mem_ea("ADD.w_D0_d16(A1)", 0xD169, 1, 2));
    defs.push(mem_ea("ADD.l_D0_d16(A1)", 0xD1A9, 1, 4));
    // d8(A1,Xn)
    defs.push(mem_ea("ADD.b_D0_idx(A1)", 0xD131, 1, 1));
    defs.push(mem_ea("ADD.w_D0_idx(A1)", 0xD171, 1, 2));
    defs.push(mem_ea("ADD.l_D0_idx(A1)", 0xD1B1, 1, 4));

    // --- MOVE with memory source EA ---
    defs.push(mem_ea("MOVE.l_(A0)_D1", 0x2210, 0, 4));
    defs.push(mem_ea("MOVE.l_(A0)+_D1", 0x2218, 0, 4));
    defs.push(mem_ea("MOVE.l_d16(A0)_D1", 0x2228, 1, 4));
    defs.push(mem_ea("MOVE.l_idx(A0)_D1", 0x2230, 1, 4));

    // --- MOVE with memory destination EA ---
    // Destination EA uses bits 11-6 (mode=8:6, reg=11:9).
    defs.push(mem_ea_dst("MOVE.l_D0_(A1)", 0x2280, 0, 4));
    defs.push(mem_ea_dst("MOVE.l_D0_d16(A1)", 0x2340, 1, 4));
    defs.push(mem_ea_dst("MOVE.l_D0_idx(A1)", 0x2380, 1, 4));

    // --- A. Absolute short: ADD <abs.W>,D1 / CLR <abs.W> ---
    defs.push(mem_ea("ADD.b_absW_D1", 0xD238, 1, 1));
    defs.push(mem_ea("ADD.w_absW_D1", 0xD278, 1, 2));
    defs.push(mem_ea("ADD.l_absW_D1", 0xD2B8, 1, 4));
    defs.push(mem_ea("CLR.b_absW", 0x4238, 1, 1));
    defs.push(mem_ea("CLR.w_absW", 0x4278, 1, 2));
    defs.push(mem_ea("CLR.l_absW", 0x42B8, 1, 4));

    // --- B. Absolute long: ADD <abs.L>,D1 / CLR <abs.L> ---
    defs.push(mem_ea("ADD.b_absL_D1", 0xD239, 2, 1));
    defs.push(mem_ea("ADD.w_absL_D1", 0xD279, 2, 2));
    defs.push(mem_ea("ADD.l_absL_D1", 0xD2B9, 2, 4));
    defs.push(mem_ea("CLR.b_absL", 0x4239, 2, 1));
    defs.push(mem_ea("CLR.w_absL", 0x4279, 2, 2));
    defs.push(mem_ea("CLR.l_absL", 0x42B9, 2, 4));

    // --- C. PC-relative: ADD <d16(PC)>,D1 / ADD <d8(PC,Xn)>,D1 ---
    defs.push(mem_ea("ADD.b_d16PC_D1", 0xD23A, 1, 1));
    defs.push(mem_ea("ADD.w_d16PC_D1", 0xD27A, 1, 2));
    defs.push(mem_ea("ADD.l_d16PC_D1", 0xD2BA, 1, 4));
    defs.push(mem_ea("ADD.b_idxPC_D1", 0xD23B, 1, 1));
    defs.push(mem_ea("ADD.w_idxPC_D1", 0xD27B, 1, 2));
    defs.push(mem_ea("ADD.l_idxPC_D1", 0xD2BB, 1, 4));

    // --- D. Immediate-to-memory: ADDI/CMPI ---
    defs.push(imm_mem_ea("ADDI.b_(A0)", 0x0610, 1, 1, 1));
    defs.push(imm_mem_ea("ADDI.w_(A0)", 0x0650, 1, 1, 2));
    defs.push(imm_mem_ea("ADDI.l_(A0)", 0x0690, 2, 2, 4));
    defs.push(imm_mem_ea("ADDI.b_d16(A0)", 0x0628, 2, 1, 1));
    defs.push(imm_mem_ea("ADDI.w_d16(A0)", 0x0668, 2, 1, 2));
    defs.push(imm_mem_ea("ADDI.l_d16(A0)", 0x06A8, 3, 2, 4));
    defs.push(imm_mem_ea("ADDI.b_idx(A0)", 0x0630, 2, 1, 1));
    defs.push(imm_mem_ea("ADDI.w_idx(A0)", 0x0670, 2, 1, 2));
    defs.push(imm_mem_ea("ADDI.l_idx(A0)", 0x06B0, 3, 2, 4));
    defs.push(imm_mem_ea("CMPI.b_(A0)", 0x0C10, 1, 1, 1));
    defs.push(imm_mem_ea("CMPI.w_(A0)", 0x0C50, 1, 1, 2));
    defs.push(imm_mem_ea("CMPI.l_(A0)", 0x0C90, 2, 2, 4));

    // --- E. MOVEM ---
    defs.push(movem("MOVEM.w_to_(A0)", 0x4890, 1, 2));
    defs.push(movem("MOVEM.l_to_(A0)", 0x48D0, 1, 4));
    defs.push(movem("MOVEM.w_to_-(A0)", 0x48A0, 1, 2));
    defs.push(movem("MOVEM.l_to_-(A0)", 0x48E0, 1, 4));
    defs.push(movem("MOVEM.w_from_(A0)+", 0x4C98, 1, 2));
    defs.push(movem("MOVEM.l_from_(A0)+", 0x4CD8, 1, 4));
    defs.push(movem("MOVEM.l_to_d16(A0)", 0x48E8, 2, 4));
    defs.push(movem("MOVEM.l_from_d16(A0)", 0x4CE8, 2, 4));

    // --- F. LEA <ea>,A1 ---
    defs.push(mem_ea("LEA_d16(A0)_A1", 0x43E8, 1, 0));
    defs.push(mem_ea("LEA_idx(A0)_A1", 0x43F0, 1, 0));
    defs.push(mem_ea("LEA_absW_A1", 0x43F8, 1, 0));
    defs.push(mem_ea("LEA_d16PC_A1", 0x43FA, 1, 0));

    // --- G. PEA <ea> ---
    defs.push(mem_ea("PEA_d16(A0)", 0x4868, 1, 0));
    defs.push(mem_ea("PEA_idx(A0)", 0x4870, 1, 0));
    defs.push(mem_ea("PEA_absW", 0x4878, 1, 0));
    defs.push(mem_ea("PEA_d16PC", 0x487A, 1, 0));

    // JMP/JSR omitted: Musashi's prefetch model doesn't refresh the pipeline
    // on control flow changes, so PREF_DATA stays stale after a jump. Our
    // emulator correctly updates IR/IRC at the target, causing 100% mismatches.
    // These instructions need a different test format.

    // --- I. Unary memory ops (A0 indirect) ---
    defs.push(mem_ea("NEG.b_(A0)", 0x4410, 0, 1));
    defs.push(mem_ea("NEG.w_(A0)", 0x4450, 0, 2));
    defs.push(mem_ea("NEG.l_(A0)", 0x4490, 0, 4));
    defs.push(mem_ea("NOT.b_(A0)", 0x4610, 0, 1));
    defs.push(mem_ea("NOT.w_(A0)", 0x4650, 0, 2));
    defs.push(mem_ea("NOT.l_(A0)", 0x4690, 0, 4));
    defs.push(mem_ea("TST.b_(A0)", 0x4A10, 0, 1));
    defs.push(mem_ea("TST.w_(A0)", 0x4A50, 0, 2));
    defs.push(mem_ea("TST.l_(A0)", 0x4A90, 0, 4));
    defs.push(mem_ea("NEGX.b_(A0)", 0x4010, 0, 1));
    defs.push(mem_ea("NEGX.w_(A0)", 0x4050, 0, 2));
    defs.push(mem_ea("NEGX.l_(A0)", 0x4090, 0, 4));

    // --- J. ADDQ/SUBQ #1,<ea> memory ---
    defs.push(mem_ea("ADDQ.b_1_(A0)", 0x5210, 0, 1));
    defs.push(mem_ea("ADDQ.w_1_(A0)", 0x5250, 0, 2));
    defs.push(mem_ea("ADDQ.l_1_(A0)", 0x5290, 0, 4));
    defs.push(mem_ea("SUBQ.b_1_(A0)", 0x5310, 0, 1));
    defs.push(mem_ea("SUBQ.w_1_(A0)", 0x5350, 0, 2));
    defs.push(mem_ea("SUBQ.l_1_(A0)", 0x5390, 0, 4));

    // --- K. Bit ops on memory (byte-sized) ---
    defs.push(mem_ea("BTST_D1_(A0)", 0x0310, 0, 1));
    defs.push(mem_ea("BCHG_D1_(A0)", 0x0350, 0, 1));
    defs.push(mem_ea("BCLR_D1_(A0)", 0x0390, 0, 1));
    defs.push(mem_ea("BSET_D1_(A0)", 0x03D0, 0, 1));

    // --- L. Shifts/rotates on memory (word-sized, single-bit) ---
    defs.push(mem_ea("ASL_(A0)", 0xE1D0, 0, 2));
    defs.push(mem_ea("ASR_(A0)", 0xE0D0, 0, 2));
    defs.push(mem_ea("LSL_(A0)", 0xE3D0, 0, 2));
    defs.push(mem_ea("LSR_(A0)", 0xE2D0, 0, 2));
    defs.push(mem_ea("ROL_(A0)", 0xE7D0, 0, 2));
    defs.push(mem_ea("ROR_(A0)", 0xE6D0, 0, 2));
    defs.push(mem_ea("ROXL_(A0)", 0xE5D0, 0, 2));
    defs.push(mem_ea("ROXR_(A0)", 0xE4D0, 0, 2));

    // TODO: Bcc, BSR, DBcc, RTS, RTE, RTR
    // LINK, UNLINK, TRAP, TRAPV, STOP
    // MOVEP, ILLEGAL, LINEA, LINEF
    // MOVEfromUSP, MOVEtoUSP, RESET

    // ===== 68010+ instructions =====

    // RTD: return and deallocate (pop PC, add d16 to SP)
    defs.push(needs_stack_010("RTD", 0x4E74, 1));

    // MOVE from CCR: read CCR to Dn (not privileged)
    defs.push(fixed_010("MOVEfromCCR", 0x42C0)); // MOVE CCR,D0

    // BKPT: breakpoint (takes illegal instruction exception on 68010)
    defs.push(fixed_010("BKPT", 0x4848)); // BKPT #0

    // MOVEC: already tested via 68000 Musashi tests (gated by model),
    // but add explicit 68010 entry for completeness
    defs.push(InstructionDef {
        name: "MOVEC_010",
        opcode: 0x4E7A,
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: M68010,
    });

    // ===== 68020+ instructions =====

    let m68020 = musashi::M68K_CPU_TYPE_68020;

    // EXTB.L: sign-extend byte to long (register only)
    defs.push(InstructionDef {
        name: "EXTB.l",
        opcode: 0x49C0, // EXTB.L D0
        ext_words: 0,
        setup: InstructionSetup::Fixed,
        min_cpu: m68020,
    });

    // MULL: 32×32 multiply (register mode, D0 × D1)
    // Opcode $4C00 + extension word with register encoding.
    // Extension word: bit 11 = signed, bits 14-12 = Dh (64-bit hi), bits 2-0 = Dl
    // MULU.L D0,D1: ext = 0x0001 (unsigned, Dl=D1)
    defs.push(InstructionDef {
        name: "MULL",
        opcode: 0x4C00, // MULL D0,...
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // DIVL: 32÷32 divide (register mode, D0 / D1)
    // Opcode $4C40 + extension word
    defs.push(InstructionDef {
        name: "DIVL",
        opcode: 0x4C40, // DIVL D0,...
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // Bitfield instructions (register mode, Dn)
    // Opcode format: 1110_1xxx_11_000_rrr (EA mode=000 for Dn, reg in bits 2-0)
    // Extension word encodes offset/width and optional Dn destination.
    // Using D0 as the target register.

    // BFTST D0{...}: 1110_1000_1100_0000 = $E8C0
    defs.push(InstructionDef {
        name: "BFTST",
        opcode: 0xE8C0, // BFTST D0{offset:width}
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // BFCHG D0{...}: 1110_1010_1100_0000 = $EAC0
    defs.push(InstructionDef {
        name: "BFCHG",
        opcode: 0xEAC0,
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // BFCLR D0{...}: 1110_1100_1100_0000 = $ECC0
    defs.push(InstructionDef {
        name: "BFCLR",
        opcode: 0xECC0,
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // BFSET D0{...}: 1110_1110_1100_0000 = $EEC0
    defs.push(InstructionDef {
        name: "BFSET",
        opcode: 0xEEC0,
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // BFEXTU D0{...},D1: 1110_1001_1100_0000 = $E9C0
    // Extension word bits 14-12 = dest register D1 (001)
    // We use RandExt1 — random ext word; the dest reg is random but that's fine.
    defs.push(InstructionDef {
        name: "BFEXTU",
        opcode: 0xE9C0,
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // BFEXTS D0{...},D1: 1110_1011_1100_0000 = $EBC0
    defs.push(InstructionDef {
        name: "BFEXTS",
        opcode: 0xEBC0,
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // BFFFO D0{...},D1: 1110_1101_1100_0000 = $EDC0
    defs.push(InstructionDef {
        name: "BFFFO",
        opcode: 0xEDC0,
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // BFINS D1,D0{...}: 1110_1111_1100_0000 = $EFC0
    defs.push(InstructionDef {
        name: "BFINS",
        opcode: 0xEFC0,
        ext_words: 1,
        setup: InstructionSetup::RandExt1,
        min_cpu: m68020,
    });

    // CAS.l Dc,Du,D0: $0EC0 + extension word
    // For register-mode testing only. CAS compares Dc with (EA).
    // But CAS only works with memory EA — Dn is invalid for CAS.
    // Skip CAS in the register-mode catalogue for now.

    // Filter by CPU type
    let min_order = cpu_type_order(cpu_type);
    defs.retain(|d| cpu_type_order(d.min_cpu) <= min_order);
    defs
}

/// Map CPU type constants to an ordering for filtering.
///
/// The 68020/030/040 families share the same base integer instruction set
/// (MULL, DIVL, bitfields, EXTB, etc.). EC/LC variants only remove
/// FPU and/or MMU, which aren't in our catalogue. Give them all the
/// same order so every variant gets the full instruction set.
fn cpu_type_order(cpu_type: u32) -> u32 {
    match cpu_type {
        musashi::M68K_CPU_TYPE_68000 => 0,
        musashi::M68K_CPU_TYPE_68010 => 1,
        musashi::M68K_CPU_TYPE_68EC020
        | musashi::M68K_CPU_TYPE_68020
        | musashi::M68K_CPU_TYPE_68EC030
        | musashi::M68K_CPU_TYPE_68030
        | musashi::M68K_CPU_TYPE_68EC040
        | musashi::M68K_CPU_TYPE_68LC040
        | musashi::M68K_CPU_TYPE_68040 => 2,
        _ => 99,
    }
}

/// Find an instruction definition by name.
pub fn find(cpu_type: u32, name: &str) -> Option<InstructionDef> {
    catalogue(cpu_type)
        .into_iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))
}
