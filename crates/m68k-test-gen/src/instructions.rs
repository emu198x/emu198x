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

    // TODO: Add remaining instructions that need special setup:
    // Bcc, BSR, DBcc, JMP, JSR, RTS, RTE, RTR
    // LINK, UNLINK, TRAP, TRAPV, STOP
    // MOVEM, MOVEP, PEA, LEA
    // ILLEGAL, LINEA, LINEF
    // MOVEfromUSP, MOVEtoUSP
    // RESET

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
fn cpu_type_order(cpu_type: u32) -> u32 {
    match cpu_type {
        musashi::M68K_CPU_TYPE_68000 => 0,
        musashi::M68K_CPU_TYPE_68010 => 1,
        musashi::M68K_CPU_TYPE_68EC020 => 2,
        musashi::M68K_CPU_TYPE_68020 => 3,
        _ => 99,
    }
}

/// Find an instruction definition by name.
pub fn find(cpu_type: u32, name: &str) -> Option<InstructionDef> {
    catalogue(cpu_type)
        .into_iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))
}
