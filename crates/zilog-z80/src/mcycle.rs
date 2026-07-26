/// M-step types that compose Z80 instructions.
///
/// Each Z80 instruction is a sequence of MSteps. The tick walker processes
/// one MStep at a time, advancing through its half-cycle phases. When a
/// step completes, the walker moves to the next step in the sequence.
///
/// The Execute step is special: it has 0 half-cycles and is processed
/// immediately when reached. It applies the ALU operation using data
/// staged by previous steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MStep {
    /// Fetch a byte from (PC), store in staged data_lo. PC incremented.
    /// 3 T-states (6 half-cycles): memory read at PC.
    FetchByte,

    /// Fetch a byte from (PC), store in staged data_hi. PC incremented.
    /// 3 T-states (6 half-cycles): memory read at PC.
    FetchByteHi,

    /// Fetch displacement byte from (PC) for indexed addressing (IX+d, IY+d).
    /// Store in staged displacement. PC incremented.
    /// 3 T-states (6 half-cycles): memory read at PC.
    FetchDisp,

    /// Read a byte from staged addr, store in staged data_lo.
    /// 3 T-states (6 half-cycles): memory read at addr.
    ReadAddr,

    /// Read a byte from staged addr+1, store in staged data_hi.
    /// 3 T-states (6 half-cycles): memory read at addr+1.
    ReadAddrHi,

    /// Write staged data_lo to staged addr.
    /// 3 T-states (6 half-cycles): memory write.
    WriteAddr,

    /// Write staged data_hi to staged addr+1.
    /// 3 T-states (6 half-cycles): memory write.
    WriteAddrHi,

    /// Push high byte of a 16-bit value. SP decremented, write to (SP).
    /// 3 T-states (6 half-cycles): memory write.
    PushHi,

    /// Push low byte of a 16-bit value. SP decremented, write to (SP).
    /// 3 T-states (6 half-cycles): memory write.
    PushLo,

    /// Pop low byte. Read from (SP), SP incremented.
    /// 3 T-states (6 half-cycles): memory read.
    PopLo,

    /// Pop high byte. Read from (SP), SP incremented.
    /// 3 T-states (6 half-cycles): memory read.
    PopHi,

    /// Contended memory cycle at PC without a read strobe.
    /// 3 T-states (6 half-cycles): address and MREQ only.
    ContendPc,

    /// Read from I/O port (staged addr = port address).
    /// 4 T-states (8 half-cycles): I/O read.
    IoRead,

    /// Write to I/O port (staged addr = port address).
    /// 4 T-states (8 half-cycles): I/O write.
    IoWrite,

    /// Internal operation — no bus activity, just burns time.
    /// N T-states (2N half-cycles). Address bus shows IR (or context-dependent).
    Internal(u8),

    /// Non-maskable interrupt response cycle.
    /// 5 T-states (10 half-cycles): discarded PC read followed by refresh.
    NmiAck,

    /// Interrupt acknowledge cycle.
    /// 7 T-states (14 half-cycles): special M1-like cycle.
    IntAck,

    /// Apply the instruction's operation using staged data.
    /// 0 half-cycles — processed immediately by the walker.
    Execute,
}

impl MStep {
    /// Number of half-cycles this step takes.
    /// Execute is 0 — it's processed immediately.
    pub fn half_cycles(&self) -> u8 {
        match self {
            MStep::FetchByte
            | MStep::FetchByteHi
            | MStep::FetchDisp
            | MStep::ReadAddr
            | MStep::ReadAddrHi
            | MStep::WriteAddr
            | MStep::WriteAddrHi
            | MStep::PushHi
            | MStep::PushLo
            | MStep::PopLo
            | MStep::PopHi
            | MStep::ContendPc => 6, // 3 T-states

            MStep::IoRead | MStep::IoWrite => 8, // 4 T-states

            MStep::Internal(n) => n * 2, // N T-states

            MStep::NmiAck => 10, // 5 T-states

            MStep::IntAck => 14, // 7 T-states

            MStep::Execute => 0,
        }
    }
}

// ============================================================================
// Instruction sequences
// ============================================================================
//
// Each Z80 instruction is defined as a static slice of MSteps. The first
// step is always implicit — it's the M1 opcode fetch handled by the walker.
// The sequences below define what happens AFTER the opcode is decoded.
//
// For instructions that are just the M1 fetch (NOP, EI, DI, etc.),
// the sequence is just [Execute].

// --- 8-bit Load Group ---

/// NOP — no operation. Just the M1 fetch + execute (which does nothing).
pub static SEQ_NOP: &[MStep] = &[MStep::Execute];

/// LD r, r' — register to register (single byte opcode, no extra M-cycles).
pub static SEQ_LD_R_R: &[MStep] = &[MStep::Execute];

/// LD r, n — load immediate byte into register.
/// M1 fetch (implicit) + fetch byte + execute.
pub static SEQ_LD_R_N: &[MStep] = &[MStep::FetchByte, MStep::Execute];

/// LD r, (HL) — load from memory at HL into register.
/// M1 fetch (implicit) + execute(stage addr=HL) + read at HL + execute(store).
pub static SEQ_LD_R_HL: &[MStep] = &[MStep::Execute, MStep::ReadAddr, MStep::Execute];

/// LD (HL), r — store register to memory at HL.
/// M1 fetch (implicit) + execute (stages data) + write at HL.
pub static SEQ_LD_HL_R: &[MStep] = &[MStep::Execute, MStep::WriteAddr];

/// LD (HL), n — store immediate byte to memory at HL.
/// M1 fetch + fetch byte + execute + write at HL.
pub static SEQ_LD_HL_N: &[MStep] = &[MStep::FetchByte, MStep::Execute, MStep::WriteAddr];

/// LD A, (BC) or LD A, (DE) — load A from memory at BC/DE.
/// First Execute stages the address, ReadAddr reads from it, second Execute stores to A.
pub static SEQ_LD_A_IND: &[MStep] = &[MStep::Execute, MStep::ReadAddr, MStep::Execute];

/// LD (BC), A or LD (DE), A — store A to memory at BC/DE.
/// Execute stages the address and data, WriteAddr writes it.
pub static SEQ_LD_IND_A: &[MStep] = &[MStep::Execute, MStep::WriteAddr];

/// LD A, (nn) — load A from absolute address.
/// M1 + fetch low + fetch high + execute(stage addr) + read at nn + execute(store).
pub static SEQ_LD_A_NN: &[MStep] = &[
    MStep::FetchByte,
    MStep::FetchByteHi,
    MStep::Execute, // stage addr from fetched bytes
    MStep::ReadAddr,
    MStep::Execute, // store data_lo to A
];

/// LD (nn), A — store A to absolute address.
/// M1 + fetch low + fetch high + execute(stage addr+data) + write.
pub static SEQ_LD_NN_A: &[MStep] = &[
    MStep::FetchByte,
    MStep::FetchByteHi,
    MStep::Execute, // stage addr from fetched bytes, data from A
    MStep::WriteAddr,
];

// --- 16-bit Load Group ---

/// LD rr, nn — load 16-bit immediate into register pair.
/// M1 + fetch low + fetch high + execute.
pub static SEQ_LD_RR_NN: &[MStep] = &[MStep::FetchByte, MStep::FetchByteHi, MStep::Execute];

/// LD SP, HL — 2 internal T-states.
/// M1 + internal(2) + execute.
pub static SEQ_LD_SP_HL: &[MStep] = &[MStep::Internal(2), MStep::Execute];

/// PUSH rr — push 16-bit register pair.
/// M1 + internal(1) + push high + push low.
pub static SEQ_PUSH: &[MStep] = &[
    MStep::Internal(1),
    MStep::Execute, // stages the value to push
    MStep::PushHi,
    MStep::PushLo,
];

/// POP rr — pop 16-bit register pair.
/// M1 + pop low + pop high + execute.
pub static SEQ_POP: &[MStep] = &[MStep::PopLo, MStep::PopHi, MStep::Execute];

/// LD (nn), rr — store 16-bit register to memory.
/// M1 + fetch low addr + fetch high addr + execute(stage) + write low + write high.
pub static SEQ_LD_NN_RR: &[MStep] = &[
    MStep::FetchByte,
    MStep::FetchByteHi,
    MStep::Execute, // stage addr and write values
    MStep::WriteAddr,
    MStep::WriteAddrHi,
];

/// LD rr, (nn) — load 16-bit register from memory.
/// M1 + fetch low addr + fetch high addr + execute(stage addr) + read low + read high + execute(store).
pub static SEQ_LD_RR_NN_IND: &[MStep] = &[
    MStep::FetchByte,
    MStep::FetchByteHi,
    MStep::Execute, // stage addr from fetched bytes
    MStep::ReadAddr,
    MStep::ReadAddrHi,
    MStep::Execute, // store to register pair
];

// --- ALU Group (8-bit arithmetic/logic) ---

/// ALU r — operate on register (ADD, ADC, SUB, SBC, AND, OR, XOR, CP).
pub static SEQ_ALU_R: &[MStep] = &[MStep::Execute];

/// ALU n — operate on immediate byte.
pub static SEQ_ALU_N: &[MStep] = &[MStep::FetchByte, MStep::Execute];

/// ALU (HL) — operate on memory at HL.
pub static SEQ_ALU_HL: &[MStep] = &[MStep::Execute, MStep::ReadAddr, MStep::Execute];

/// INC/DEC (HL) — read-modify-write at HL.
/// M1 + execute(stage addr=HL) + read at HL + internal(1) + execute(modify) + write at HL.
pub static SEQ_INC_DEC_HL: &[MStep] = &[
    MStep::Execute, // stage addr = HL
    MStep::ReadAddr,
    MStep::Internal(1),
    MStep::Execute, // INC/DEC the byte, stage write_val
    MStep::WriteAddr,
];

// --- Jump Group ---

/// JP nn — unconditional absolute jump.
/// M1 + fetch low + fetch high + execute.
pub static SEQ_JP_NN: &[MStep] = &[MStep::FetchByte, MStep::FetchByteHi, MStep::Execute];

/// JP cc, nn — conditional jump. Same sequence regardless of taken/not-taken.
/// The Execute step sets PC if condition is met.
pub static SEQ_JP_CC_NN: &[MStep] = &[MStep::FetchByte, MStep::FetchByteHi, MStep::Execute];

/// JR e — relative jump.
/// M1 + fetch displacement + internal(5) + execute.
pub static SEQ_JR_E: &[MStep] = &[MStep::FetchByte, MStep::Internal(5), MStep::Execute];

/// JR cc, e taken — same bus activity as JR e.
pub static SEQ_JR_CC_TAKEN: &[MStep] = &[MStep::FetchByte, MStep::Internal(5), MStep::Execute];

/// JR cc, e not taken — contend on PC for 3T, then advance PC past the displacement.
pub static SEQ_JR_CC_NOT_TAKEN: &[MStep] = &[MStep::ContendPc, MStep::Execute];

/// DJNZ e taken.
pub static SEQ_DJNZ_TAKEN: &[MStep] = &[
    MStep::Internal(1),
    MStep::FetchByte,
    MStep::Internal(5),
    MStep::Execute,
];

/// DJNZ e not taken — contend on PC for 3T instead of reading the displacement.
pub static SEQ_DJNZ_NOT_TAKEN: &[MStep] = &[MStep::Internal(1), MStep::ContendPc, MStep::Execute];

/// JP (HL) — jump to address in HL. Just an execute (sets PC = HL).
pub static SEQ_JP_HL: &[MStep] = &[MStep::Execute];

// --- Call/Return Group ---

/// CALL nn — unconditional call.
/// M1 + fetch low + fetch high + internal(1) + execute + push high PC + push low PC.
pub static SEQ_CALL_NN: &[MStep] = &[
    MStep::FetchByte,
    MStep::FetchByteHi,
    MStep::Internal(1),
    MStep::Execute,
    MStep::PushHi,
    MStep::PushLo,
];

/// CALL cc, nn — full taken sequence. If not taken, Execute sets done=true
/// to skip Internal(1) + Push. The not-taken timing is 10T (M1+Fetch+Fetch+Execute).
/// The taken timing is 17T (M1+Fetch+Fetch+Execute+Internal+Push+Push).
/// Note: Execute is at index 2 for not-taken (no Internal before it).
pub static SEQ_CALL_CC: &[MStep] = &[
    MStep::FetchByte,
    MStep::FetchByteHi,
    MStep::Execute, // check condition; if not taken, done=true
    MStep::Internal(1),
    MStep::PushHi,
    MStep::PushLo,
];

/// RET — return from subroutine.
/// M1 + pop low + pop high + execute.
pub static SEQ_RET: &[MStep] = &[MStep::PopLo, MStep::PopHi, MStep::Execute];

/// RET cc — Execute checks condition first, then pops if taken.
/// Not-taken: M1(4) + Internal(1) + Execute(0) = 5T
/// Taken: M1(4) + Internal(1) + Execute(0) + PopLo(3) + PopHi(3) + Execute(0) = 11T
pub static SEQ_RET_CC: &[MStep] = &[
    MStep::Internal(1),
    MStep::Execute, // check condition; if not taken, done=true
    MStep::PopLo,
    MStep::PopHi,
    MStep::Execute, // set PC from popped address
];

/// RST p — restart (call to page-zero address).
/// M1 + internal(1) + execute + push high + push low.
pub static SEQ_RST: &[MStep] = &[
    MStep::Internal(1),
    MStep::Execute,
    MStep::PushHi,
    MStep::PushLo,
];

// --- Rotate/Shift (HL) ---

/// RLC/RRC/RL/RR/SLA/SRA/SRL/SLL (HL) — CB-prefix rotate/shift on memory.
/// CB prefix M1 + opcode M1 + Execute(addr=HL) + read (HL) + internal(1) + execute + write (HL).
pub static SEQ_CB_HL: &[MStep] = &[
    MStep::Execute, // stage addr = HL
    MStep::ReadAddr,
    MStep::Internal(1),
    MStep::Execute, // rotate/shift/set/res
    MStep::WriteAddr,
];

/// BIT b, (HL) — CB-prefix bit test on memory (read-only).
pub static SEQ_CB_BIT_HL: &[MStep] = &[
    MStep::Execute, // stage addr = HL
    MStep::ReadAddr,
    MStep::Internal(1),
    MStep::Execute, // BIT test
];

/// CB-prefix operations on registers — just execute after the opcode fetch.
pub static SEQ_CB_R: &[MStep] = &[MStep::Execute];

// --- Exchange, I/O, Misc ---

/// EX (SP), HL — exchange top of stack with HL.
/// M1 + read low (SP) + read high (SP+1) + internal(1) + execute + write high + write low + internal(2).
pub static SEQ_EX_SP_HL: &[MStep] = &[
    MStep::PopLo, // read (SP), SP++
    MStep::PopHi, // read (SP+1), SP++
    MStep::Internal(1),
    MStep::Execute,
    MStep::PushHi, // write (SP-1), SP--
    MStep::PushLo, // write (SP-2), SP--
    MStep::Internal(2),
];

/// IN A, (n) — input from port.
/// M1 + fetch port number + execute (sets up port addr) + I/O read + execute (stores result).
pub static SEQ_IN_A_N: &[MStep] = &[
    MStep::FetchByte,
    MStep::Execute, // stage port address = (A << 8) | n
    MStep::IoRead,
    MStep::Execute, // store result in A
];

/// OUT (n), A — output to port.
/// M1 + fetch port number + execute (sets up port addr + data) + I/O write.
pub static SEQ_OUT_N_A: &[MStep] = &[
    MStep::FetchByte,
    MStep::Execute, // stage port address and data
    MStep::IoWrite,
];

/// IN r, (C) — ED-prefix input from port BC.
/// ED prefix M1 + opcode M1 + Execute(stage port=BC) + I/O read + Execute(store+flags).
pub static SEQ_IN_R_C: &[MStep] = &[
    MStep::Execute, // stage port addr = BC
    MStep::IoRead,
    MStep::Execute, // store result in register, set flags
];

/// OUT (C), r — ED-prefix output to port BC.
/// ED prefix M1 + opcode M1 + Execute(stage port+data) + I/O write.
pub static SEQ_OUT_C_R: &[MStep] = &[
    MStep::Execute, // stage port addr = BC, write_val = register
    MStep::IoWrite,
];

// --- Block Transfer ---

/// LDI/LDD — block transfer (single).
/// Execute(addr=HL) + ReadAddr + Execute(addr=DE,write_val=data) + WriteAddr + Internal(2) + Execute(update regs).
pub static SEQ_LDI: &[MStep] = &[
    MStep::Execute,   // stage addr = HL
    MStep::ReadAddr,  // read from (HL) → data_lo
    MStep::Execute,   // stage addr = DE, write_val = data_lo
    MStep::WriteAddr, // write to (DE)
    MStep::Internal(2),
    MStep::Execute, // update HL, DE, BC, flags
];

/// LDIR/LDDR — block transfer (repeat, not done).
pub static SEQ_LDIR_REPEAT: &[MStep] = &[
    MStep::Execute,
    MStep::ReadAddr,
    MStep::Execute,
    MStep::WriteAddr,
    MStep::Internal(2),
    MStep::Execute, // update regs + check BC, set done or PC-=2
    MStep::Internal(5),
];

/// LDIR/LDDR — block transfer (repeat, done — BC reached 0).
pub static SEQ_LDIR_DONE: &[MStep] = &[
    MStep::Execute,
    MStep::ReadAddr,
    MStep::Execute,
    MStep::WriteAddr,
    MStep::Internal(2),
    MStep::Execute,
];

/// CPI/CPD — block compare (single).
/// Execute(addr=HL) + ReadAddr + Internal(5) + Execute(compare+update).
pub static SEQ_CPI: &[MStep] = &[
    MStep::Execute, // stage addr = HL
    MStep::ReadAddr,
    MStep::Internal(5),
    MStep::Execute, // compare + update HL, BC
];

/// CPIR/CPDR — block compare (repeat, not done).
pub static SEQ_CPIR_REPEAT: &[MStep] = &[
    MStep::Execute,
    MStep::ReadAddr,
    MStep::Internal(5),
    MStep::Execute,     // compare + check done
    MStep::Internal(5), // repeat penalty
];

/// CPIR/CPDR — block compare (repeat, done).
pub static SEQ_CPIR_DONE: &[MStep] = &[
    MStep::Execute,
    MStep::ReadAddr,
    MStep::Internal(5),
    MStep::Execute,
];

/// INI/IND — block input (single).
/// Internal(1) + Execute(stage port=BC) + IoRead + Execute(stage addr=HL) + WriteAddr + Execute(flags+update).
pub static SEQ_INI: &[MStep] = &[
    MStep::Internal(1),
    MStep::Execute,   // stage port addr = BC, decrement B
    MStep::IoRead,    // read byte from port → data_lo
    MStep::Execute,   // stage write addr = HL, write_val = data_lo
    MStep::WriteAddr, // write to (HL)
    MStep::Execute,   // update HL, set flags
];

/// INIR/INDR — block input (repeat, not done).
pub static SEQ_INIR_REPEAT: &[MStep] = &[
    MStep::Internal(1),
    MStep::Execute,
    MStep::IoRead,
    MStep::Execute,
    MStep::WriteAddr,
    MStep::Execute,     // update + check B==0
    MStep::Internal(5), // repeat penalty
];

/// INIR/INDR — block input (repeat, done).
pub static SEQ_INIR_DONE: &[MStep] = &[
    MStep::Internal(1),
    MStep::Execute,
    MStep::IoRead,
    MStep::Execute,
    MStep::WriteAddr,
    MStep::Execute,
];

/// OUTI/OUTD — block output (single).
/// Internal(1) + Execute(stage addr=HL) + ReadAddr + Execute(stage port=BC, dec B) + IoWrite + Execute(flags).
pub static SEQ_OUTI: &[MStep] = &[
    MStep::Internal(1),
    MStep::Execute,  // stage read addr = HL
    MStep::ReadAddr, // read byte from (HL) → data_lo
    MStep::Execute,  // decrement B, stage port addr = BC, write_val = data_lo
    MStep::IoWrite,  // write to port
    MStep::Execute,  // update HL, set flags
];

/// OTIR/OTDR — block output (repeat, not done).
pub static SEQ_OTIR_REPEAT: &[MStep] = &[
    MStep::Internal(1),
    MStep::Execute,
    MStep::ReadAddr,
    MStep::Execute,
    MStep::IoWrite,
    MStep::Execute, // update + check B==0
    MStep::Internal(5),
];

/// OTIR/OTDR — block output (repeat, done).
pub static SEQ_OTIR_DONE: &[MStep] = &[
    MStep::Internal(1),
    MStep::Execute,
    MStep::ReadAddr,
    MStep::Execute,
    MStep::IoWrite,
    MStep::Execute,
];

// --- DD/FD Indexed (IX+d / IY+d) variants ---
// These replace (HL) operations when a DD/FD prefix is active.
// The displacement byte is fetched, then 5 internal T-states for address calculation.

/// LD r, (IX+d) or LD r, (IY+d)
/// DD/FD prefix M1 + opcode M1 + FetchDisp + Internal(5) + ReadAddr + Execute
pub static SEQ_LD_R_IXD: &[MStep] = &[
    MStep::FetchDisp,
    MStep::Internal(5),
    MStep::Execute, // stage addr = IX/IY + d
    MStep::ReadAddr,
    MStep::Execute, // store to register
];

/// LD (IX+d), r or LD (IY+d), r
pub static SEQ_LD_IXD_R: &[MStep] = &[
    MStep::FetchDisp,
    MStep::Internal(5),
    MStep::Execute, // stage addr = IX/IY + d, write_val = register
    MStep::WriteAddr,
];

/// LD (IX+d), n
pub static SEQ_LD_IXD_N: &[MStep] = &[
    MStep::FetchDisp,
    MStep::FetchByte, // immediate value (note: fetched AFTER disp on DD 36)
    MStep::Internal(2),
    MStep::Execute, // stage addr + write_val
    MStep::WriteAddr,
];

/// ALU A, (IX+d) — ADD/ADC/SUB/SBC/AND/OR/XOR/CP
pub static SEQ_ALU_IXD: &[MStep] = &[
    MStep::FetchDisp,
    MStep::Internal(5),
    MStep::Execute, // stage addr
    MStep::ReadAddr,
    MStep::Execute, // ALU operation
];

/// INC/DEC (IX+d)
pub static SEQ_INC_DEC_IXD: &[MStep] = &[
    MStep::FetchDisp,
    MStep::Internal(5),
    MStep::Execute, // stage addr
    MStep::ReadAddr,
    MStep::Internal(1),
    MStep::Execute, // INC/DEC
    MStep::WriteAddr,
];

/// CB (IX+d) rotate/shift/set/res — DDCB/FDCB prefix
/// After the DDCB_FETCH phase (FetchDisp + FetchByte), the execution is:
/// Internal(2) + Execute(addr) + ReadAddr + Internal(1) + Execute(op) + WriteAddr
/// The Internal(2) accounts for address computation time.
pub static SEQ_DDCB_HL: &[MStep] = &[
    MStep::Internal(2), // address computation time
    MStep::Execute,     // stage addr = IX/IY + d
    MStep::ReadAddr,
    MStep::Internal(1),
    MStep::Execute, // rotate/shift/set/res operation
    MStep::WriteAddr,
];

/// BIT b, (IX+d) — DDCB/FDCB bit test (read-only, no write back)
/// Internal(2) + Execute(addr) + ReadAddr + Internal(1) + Execute(BIT)
pub static SEQ_DDCB_BIT: &[MStep] = &[
    MStep::Internal(2),
    MStep::Execute, // stage addr
    MStep::ReadAddr,
    MStep::Internal(1),
    MStep::Execute, // BIT test
];

// --- Interrupt sequences ---

/// IM 0 interrupt response: IntAck + execute + push PC. The interrupting device
/// drives an instruction onto the bus during the ack; we model the `RST n`
/// family (the realistic case — an un-driven bus reads 0xFF = `RST 38h`), so the
/// timing matches an interrupt `RST`, identical to IM 1.
pub static SEQ_INT_IM0: &[MStep] = &[
    MStep::IntAck,
    MStep::Execute, // set PC from the RST n vector latched off the bus
    MStep::PushHi,
    MStep::PushLo,
];

/// IM 1 interrupt response: IntAck + execute (PC=0x0038) + push PC.
pub static SEQ_INT_IM1: &[MStep] = &[
    MStep::IntAck,
    MStep::Execute, // set PC = 0x0038
    MStep::PushHi,
    MStep::PushLo,
];

/// IM 2 interrupt response: IntAck + execute (stage vector) + push PC + read vector low + read vector high + execute (jump).
pub static SEQ_INT_IM2: &[MStep] = &[
    MStep::IntAck,
    MStep::Execute, // stage: push PC, compute vector address from I and latched ack byte
    MStep::PushHi,
    MStep::PushLo,
    MStep::ReadAddr,   // read low byte of handler address from vector table
    MStep::ReadAddrHi, // read high byte
    MStep::Execute,    // set PC = handler address
];

/// NMI response: discarded M1 fetch/refresh + execute + push PC.
pub static SEQ_NMI: &[MStep] = &[
    MStep::NmiAck,
    MStep::Execute, // stage: push current PC, set PC = 0x0066
    MStep::PushHi,
    MStep::PushLo,
];

// --- Simple instructions (just Execute after M1) ---

pub static SEQ_HALT: &[MStep] = &[MStep::Execute];
pub static SEQ_EI: &[MStep] = &[MStep::Execute];
pub static SEQ_DI: &[MStep] = &[MStep::Execute];
pub static SEQ_SCF: &[MStep] = &[MStep::Execute];
pub static SEQ_CCF: &[MStep] = &[MStep::Execute];
pub static SEQ_DAA: &[MStep] = &[MStep::Execute];
pub static SEQ_CPL: &[MStep] = &[MStep::Execute];
pub static SEQ_NEG: &[MStep] = &[MStep::Execute]; // ED prefix
pub static SEQ_EX_AF: &[MStep] = &[MStep::Execute];
pub static SEQ_EXX: &[MStep] = &[MStep::Execute];
pub static SEQ_EX_DE_HL: &[MStep] = &[MStep::Execute];
pub static SEQ_RLCA: &[MStep] = &[MStep::Execute];
pub static SEQ_RRCA: &[MStep] = &[MStep::Execute];
pub static SEQ_RLA: &[MStep] = &[MStep::Execute];
pub static SEQ_RRA: &[MStep] = &[MStep::Execute];

/// INC/DEC rr — 16-bit increment/decrement. 2 internal T-states.
pub static SEQ_INC_DEC_RR: &[MStep] = &[MStep::Internal(2), MStep::Execute];

/// ADD HL, rr — 16-bit add. 7 internal T-states.
pub static SEQ_ADD_HL_RR: &[MStep] = &[MStep::Internal(7), MStep::Execute];

/// RLD/RRD — rotate digit left/right.
/// M1 + Execute(addr=HL) + read (HL) + internal(4) + execute + write (HL).
pub static SEQ_RLD_RRD: &[MStep] = &[
    MStep::Execute, // stage addr = HL
    MStep::ReadAddr,
    MStep::Internal(4),
    MStep::Execute, // rotate digits
    MStep::WriteAddr,
];

/// LD I,A / LD R,A / LD A,I / LD A,R — ED-prefix, 1 internal T-state.
pub static SEQ_LD_IR: &[MStep] = &[MStep::Internal(1), MStep::Execute];

/// RETI/RETN — return from interrupt.
/// Same timing as RET but from ED prefix.
pub static SEQ_RETI: &[MStep] = &[MStep::PopLo, MStep::PopHi, MStep::Execute];

/// IM 0/1/2 — set interrupt mode. Just execute.
pub static SEQ_IM: &[MStep] = &[MStep::Execute];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mstep_half_cycles_match_z80_bus_timing() {
        // Memory cycles: 3 T-states = 6 half-cycles.
        assert_eq!(MStep::FetchByte.half_cycles(), 6);
        assert_eq!(MStep::FetchByteHi.half_cycles(), 6);
        assert_eq!(MStep::FetchDisp.half_cycles(), 6);
        assert_eq!(MStep::ReadAddr.half_cycles(), 6);
        assert_eq!(MStep::ReadAddrHi.half_cycles(), 6);
        assert_eq!(MStep::WriteAddr.half_cycles(), 6);
        assert_eq!(MStep::WriteAddrHi.half_cycles(), 6);
        assert_eq!(MStep::PushHi.half_cycles(), 6);
        assert_eq!(MStep::PushLo.half_cycles(), 6);
        assert_eq!(MStep::PopLo.half_cycles(), 6);
        assert_eq!(MStep::PopHi.half_cycles(), 6);
        assert_eq!(MStep::ContendPc.half_cycles(), 6);

        // I/O cycles: 4 T-states = 8 half-cycles.
        assert_eq!(MStep::IoRead.half_cycles(), 8);
        assert_eq!(MStep::IoWrite.half_cycles(), 8);

        // Internal: scales linearly with T-state count.
        assert_eq!(MStep::Internal(1).half_cycles(), 2);
        assert_eq!(MStep::Internal(5).half_cycles(), 10);

        // Interrupt response cycles.
        assert_eq!(MStep::NmiAck.half_cycles(), 10);
        assert_eq!(MStep::IntAck.half_cycles(), 14);

        // Execute is processed without advancing the clock.
        assert_eq!(MStep::Execute.half_cycles(), 0);
    }
}
