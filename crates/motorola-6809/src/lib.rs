//! Motorola MC6809 CPU foundation.
//!
//! This crate starts with the external bus-facing state needed by Dragon/CoCo
//! machine wiring. Instruction execution will grow behind this boundary; the
//! public pin/register shape is deliberately small and serializable so machine
//! snapshots can use it directly.

pub mod registers;

use registers::{FLAG_C, FLAG_E, FLAG_F, FLAG_H, FLAG_I, FLAG_N, FLAG_V, FLAG_Z, Registers};
use serde::{Deserialize, Serialize};

const MAX_STACK_BYTES: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum CpuState {
    Fetch,
    Prefix10,
    Prefix11,
    NopInternal,
    Internal {
        remaining: u8,
    },
    ClrAInternal,
    ClrBInternal,
    ReadCcImm(CcOp),
    ReadTransferPostbyte(TransferOp),
    ReadImm8(Imm8Op),
    ReadImm16Hi(Imm16Op),
    ReadImm16Lo {
        op: Imm16Op,
        hi: u8,
    },
    ReadRel8(Rel8Op),
    ReadRel16Hi(Rel16Op),
    ReadRel16Lo {
        op: Rel16Op,
        hi: u8,
    },
    ReadIndexedPostbyte(IndexedOp),
    ReadIndexedOffset8 {
        op: IndexedOp,
        post: u8,
        extra_cycles: u8,
    },
    ReadIndexedOffset16Hi {
        op: IndexedOp,
        post: u8,
        extra_cycles: u8,
    },
    ReadIndexedOffset16Lo {
        op: IndexedOp,
        post: u8,
        hi: u8,
        extra_cycles: u8,
    },
    ReadIndexedIndirectHi {
        op: IndexedOp,
        ptr: u16,
        extra_cycles: u8,
    },
    ReadIndexedIndirectLo {
        op: IndexedOp,
        hi: u8,
        extra_cycles: u8,
    },
    IndexedExtra {
        op: IndexedOp,
        addr: u16,
        remaining: u8,
    },
    ReadStackPostbyte(StackOp),
    ReadDirectRmwOperand(Rmw8Op),
    ReadDirectOperand(Mem8Op),
    ReadDirectOperand16(Mem16Op),
    ReadDirectValue(Mem8Op),
    ReadRmwValue {
        op: Rmw8Op,
        addr: u16,
        post_cycles: u8,
    },
    ReadMem16Hi {
        op: Mem16Op,
        addr: u16,
    },
    ReadMem16Lo {
        op: Mem16Op,
        hi: u8,
    },
    ReadExtendedHi(ExtOp),
    ReadExtendedLo {
        op: ExtOp,
        hi: u8,
    },
    ReadExtendedValue(Mem8Op),
    WriteDirectOperand(Store8Op),
    WriteDirectOperand16(Store16Op),
    WriteValue,
    WriteValueThenInternal {
        cycles: u8,
    },
    Write16Lo {
        lo: u8,
        post_cycles: u8,
    },
    StackRead,
    StackWrite,
    PushWordHi {
        hi: u8,
        after: AfterPush,
    },
    PushDone(AfterPush),
    PullWordHi(Pull16Op),
    PullWordLo {
        op: Pull16Op,
        hi: u8,
    },
    RtiReadCc,
    ReadDirectJmpOperand,
    ReadCwaiMask,
    WaitForInterrupt {
        stacked: bool,
    },
    ReadVectorHi(Vector),
    ReadVectorLo {
        vector: Vector,
        hi: u8,
    },
    IllegalOpcode(u8),
    ReadDirectJsrOperand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum CcOp {
    And,
    Or,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum TransferOp {
    Exg,
    Tfr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Imm8Op {
    Lda,
    Ldb,
    Cmpa,
    Cmpb,
    AluA(Alu8Op),
    AluB(Alu8Op),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Imm16Op {
    Load(WordReg),
    Compare(WordReg),
    AluD(Alu16Op),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Rel8Op {
    Branch(BranchCondition),
    Bsr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Rel16Op {
    Branch(BranchCondition),
    Lbsr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum BranchCondition {
    Always,
    Never,
    Hi,
    Ls,
    Cc,
    Cs,
    Ne,
    Eq,
    Vc,
    Vs,
    Pl,
    Mi,
    Ge,
    Lt,
    Gt,
    Le,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Mem8Op {
    Lda,
    Ldb,
    Cmpa,
    Cmpb,
    AluA(Alu8Op),
    AluB(Alu8Op),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Alu8Op {
    Add,
    AddCarry,
    Sub,
    SubCarry,
    And,
    Bit,
    Eor,
    Or,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Alu16Op {
    Add,
    Sub,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Rmw8Op {
    Neg,
    Com,
    Lsr,
    Ror,
    Asr,
    Asl,
    Rol,
    Dec,
    Inc,
    Tst,
    Clr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Store8Op {
    Sta,
    Stb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Mem16Op {
    Load(WordReg),
    Compare(WordReg),
    AluD(Alu16Op),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Store16Op {
    Store(WordReg),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum ExtOp {
    Load(Mem8Op),
    Store(Store8Op),
    Rmw(Rmw8Op),
    Load16(Mem16Op),
    Store16(Store16Op),
    Jmp,
    Jsr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Reg16 {
    X,
    Y,
    U,
    S,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum WordReg {
    D,
    X,
    Y,
    U,
    S,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum TransferReg {
    D,
    X,
    Y,
    U,
    S,
    Pc,
    A,
    B,
    Cc,
    Dp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum IndexedOp {
    Lea(Reg16),
    Load(Mem8Op),
    Store(Store8Op),
    Rmw(Rmw8Op),
    Load16(Mem16Op),
    Store16(Store16Op),
    Jmp,
    Jsr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum AfterPush {
    SetPc(u16),
    SetPcAfterInternal { pc: u16, cycles: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum AfterStack {
    Fetch,
    ReadVector(Vector),
    WaitForInterrupt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Vector {
    Swi3,
    Swi2,
    Firq,
    Irq,
    Swi,
    Nmi,
}

impl Vector {
    const fn addr(self) -> u16 {
        match self {
            Self::Swi3 => 0xFFF2,
            Self::Swi2 => 0xFFF4,
            Self::Firq => 0xFFF6,
            Self::Irq => 0xFFF8,
            Self::Swi => 0xFFFA,
            Self::Nmi => 0xFFFC,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Pull16Op {
    Pc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum StackOp {
    PushS,
    PullS,
    PushU,
    PullU,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum StackPointer {
    S,
    U,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum StackTarget {
    None,
    Cc,
    A,
    B,
    Dp,
    XHi,
    XLo,
    YHi,
    YLo,
    UHi,
    ULo,
    SHi,
    SLo,
    PcHi,
    PcLo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum StackWork {
    None,
    Push {
        ptr: StackPointer,
        bytes: [u8; MAX_STACK_BYTES],
        len: u8,
        index: u8,
        after: AfterStack,
    },
    Pull {
        ptr: StackPointer,
        targets: [StackTarget; MAX_STACK_BYTES],
        len: u8,
        index: u8,
        hi: u8,
    },
}

/// Motorola MC6809 CPU state exposed to machine crates.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mc6809 {
    pub regs: Registers,
    pub total_cycles: u64,
    pub addr: u16,
    pub data: u8,
    pub data_in: u8,
    pub rw: bool,
    pub sync: bool,
    pub irq: bool,
    pub firq: bool,
    pub nmi: bool,
    pub halt: bool,
    pub reset_phase: u8,
    vector_hi: u8,
    state: CpuState,
    stack_work: StackWork,
}

impl Mc6809 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            regs: Registers::new(),
            total_cycles: 0,
            addr: 0,
            data: 0,
            data_in: 0,
            rw: true,
            sync: false,
            irq: false,
            firq: false,
            nmi: false,
            halt: false,
            reset_phase: 0,
            vector_hi: 0,
            state: CpuState::Fetch,
            stack_work: StackWork::None,
        }
    }

    /// Schedule reset-vector fetches at `$FFFE/$FFFF`.
    pub fn reset(&mut self) {
        self.regs.cc |= FLAG_F | FLAG_I;
        self.total_cycles = 0;
        self.addr = 0xFFFE;
        self.data = 0;
        self.data_in = 0;
        self.rw = true;
        self.sync = false;
        self.irq = false;
        self.firq = false;
        self.nmi = false;
        self.halt = false;
        self.reset_phase = 2;
        self.vector_hi = 0;
        self.state = CpuState::Fetch;
        self.stack_work = StackWork::None;
    }

    /// Advance one bus-visible CPU cycle.
    pub fn tick(&mut self) {
        if self.halt {
            return;
        }

        match self.reset_phase {
            2 => {
                self.vector_hi = self.data_in;
                self.addr = 0xFFFF;
                self.rw = true;
                self.sync = false;
                self.reset_phase = 1;
            }
            1 => {
                self.regs.pc = u16::from_be_bytes([self.vector_hi, self.data_in]);
                self.addr = self.regs.pc;
                self.rw = true;
                self.sync = true;
                self.reset_phase = 0;
            }
            _ => self.tick_instruction(),
        }

        self.total_cycles = self.total_cycles.saturating_add(1);
    }

    #[must_use]
    pub const fn instruction_boundary(&self) -> bool {
        self.reset_phase == 0 && self.sync
    }

    fn tick_instruction(&mut self) {
        match self.state {
            CpuState::Fetch => {
                if let Some(vector) = self.pending_interrupt() {
                    self.start_interrupt(vector);
                    return;
                }
                let opcode = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.sync = false;
                match opcode {
                    0x00 => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Neg)),
                    0x03 => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Com)),
                    0x04 => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Lsr)),
                    0x06 => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Ror)),
                    0x07 => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Asr)),
                    0x08 => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Asl)),
                    0x09 => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Rol)),
                    0x0A => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Dec)),
                    0x0C => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Inc)),
                    0x0D => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Tst)),
                    0x0E => self.read_next(CpuState::ReadDirectJmpOperand),
                    0x0F => self.read_next(CpuState::ReadDirectRmwOperand(Rmw8Op::Clr)),
                    0x10 => self.read_next(CpuState::Prefix10),
                    0x11 => self.read_next(CpuState::Prefix11),
                    0x12 => {
                        // NOP is a two-cycle instruction: opcode fetch plus
                        // one internal cycle.
                        self.state = CpuState::NopInternal;
                        self.addr = self.regs.pc;
                        self.rw = true;
                    }
                    0x13 => {
                        self.state = CpuState::WaitForInterrupt { stacked: false };
                        self.addr = self.regs.pc;
                        self.rw = true;
                        self.sync = false;
                    }
                    0x16 => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(
                            BranchCondition::Always,
                        )));
                    }
                    0x17 => self.read_next(CpuState::ReadRel16Hi(Rel16Op::Lbsr)),
                    0x19 => {
                        self.daa();
                        self.start_internal_cycles(1);
                    }
                    0x1A => self.read_next(CpuState::ReadCcImm(CcOp::Or)),
                    0x1C => self.read_next(CpuState::ReadCcImm(CcOp::And)),
                    0x1D => {
                        self.sex();
                        self.start_internal_cycles(1);
                    }
                    0x1E => self.read_next(CpuState::ReadTransferPostbyte(TransferOp::Exg)),
                    0x1F => self.read_next(CpuState::ReadTransferPostbyte(TransferOp::Tfr)),
                    0x20 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Always)))
                    }
                    0x21 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Never)))
                    }
                    0x22 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Hi)));
                    }
                    0x23 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Ls)));
                    }
                    0x24 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Cc)));
                    }
                    0x25 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Cs)));
                    }
                    0x26 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Ne)));
                    }
                    0x27 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Eq)));
                    }
                    0x28 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Vc)));
                    }
                    0x29 => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Vs)));
                    }
                    0x2A => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Pl)));
                    }
                    0x2B => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Mi)));
                    }
                    0x2C => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Ge)));
                    }
                    0x2D => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Lt)));
                    }
                    0x2E => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Gt)));
                    }
                    0x2F => {
                        self.read_next(CpuState::ReadRel8(Rel8Op::Branch(BranchCondition::Le)));
                    }
                    0x30 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Lea(Reg16::X))),
                    0x31 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Lea(Reg16::Y))),
                    0x32 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Lea(Reg16::S))),
                    0x33 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Lea(Reg16::U))),
                    0x34 => self.read_next(CpuState::ReadStackPostbyte(StackOp::PushS)),
                    0x35 => self.read_next(CpuState::ReadStackPostbyte(StackOp::PullS)),
                    0x36 => self.read_next(CpuState::ReadStackPostbyte(StackOp::PushU)),
                    0x37 => self.read_next(CpuState::ReadStackPostbyte(StackOp::PullU)),
                    0x3B => {
                        self.state = CpuState::RtiReadCc;
                        self.addr = self.regs.s;
                        self.rw = true;
                        self.sync = false;
                    }
                    0x3C => self.read_next(CpuState::ReadCwaiMask),
                    0x39 => self.prepare_pull_word(Pull16Op::Pc),
                    0x3A => {
                        self.abx();
                        self.start_internal_cycles(2);
                    }
                    0x3D => {
                        self.mul();
                        self.start_internal_cycles(10);
                    }
                    0x3F => self.start_software_interrupt(Vector::Swi),
                    0x40 => {
                        self.regs.a = self.rmw8(Rmw8Op::Neg, self.regs.a).unwrap_or(self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x43 => {
                        self.regs.a = self.rmw8(Rmw8Op::Com, self.regs.a).unwrap_or(self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x44 => {
                        self.regs.a = self.rmw8(Rmw8Op::Lsr, self.regs.a).unwrap_or(self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x46 => {
                        self.regs.a = self.rmw8(Rmw8Op::Ror, self.regs.a).unwrap_or(self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x47 => {
                        self.regs.a = self.rmw8(Rmw8Op::Asr, self.regs.a).unwrap_or(self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x48 => {
                        self.regs.a = self.rmw8(Rmw8Op::Asl, self.regs.a).unwrap_or(self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x49 => {
                        self.regs.a = self.rmw8(Rmw8Op::Rol, self.regs.a).unwrap_or(self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x4A => {
                        self.regs.a = self.rmw8(Rmw8Op::Dec, self.regs.a).unwrap_or(self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x4C => {
                        self.regs.a = self.rmw8(Rmw8Op::Inc, self.regs.a).unwrap_or(self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x4D => {
                        self.rmw8(Rmw8Op::Tst, self.regs.a);
                        self.start_internal_cycles(1);
                    }
                    0x4F => {
                        self.clear_a();
                        self.read_next(CpuState::ClrAInternal);
                    }
                    0x50 => {
                        self.regs.b = self.rmw8(Rmw8Op::Neg, self.regs.b).unwrap_or(self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x53 => {
                        self.regs.b = self.rmw8(Rmw8Op::Com, self.regs.b).unwrap_or(self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x54 => {
                        self.regs.b = self.rmw8(Rmw8Op::Lsr, self.regs.b).unwrap_or(self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x56 => {
                        self.regs.b = self.rmw8(Rmw8Op::Ror, self.regs.b).unwrap_or(self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x57 => {
                        self.regs.b = self.rmw8(Rmw8Op::Asr, self.regs.b).unwrap_or(self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x58 => {
                        self.regs.b = self.rmw8(Rmw8Op::Asl, self.regs.b).unwrap_or(self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x59 => {
                        self.regs.b = self.rmw8(Rmw8Op::Rol, self.regs.b).unwrap_or(self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x5A => {
                        self.regs.b = self.rmw8(Rmw8Op::Dec, self.regs.b).unwrap_or(self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x5C => {
                        self.regs.b = self.rmw8(Rmw8Op::Inc, self.regs.b).unwrap_or(self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x5D => {
                        self.rmw8(Rmw8Op::Tst, self.regs.b);
                        self.start_internal_cycles(1);
                    }
                    0x5F => {
                        self.clear_b();
                        self.read_next(CpuState::ClrBInternal);
                    }
                    0x60 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Neg)))
                    }
                    0x63 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Com)))
                    }
                    0x64 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Lsr)))
                    }
                    0x66 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Ror)))
                    }
                    0x67 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Asr)))
                    }
                    0x68 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Asl)))
                    }
                    0x69 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Rol)))
                    }
                    0x6A => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Dec)))
                    }
                    0x6C => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Inc)))
                    }
                    0x6D => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Tst)))
                    }
                    0x6E => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Jmp)),
                    0x6F => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Rmw(Rmw8Op::Clr)))
                    }
                    0x70 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Neg))),
                    0x73 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Com))),
                    0x74 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Lsr))),
                    0x76 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Ror))),
                    0x77 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Asr))),
                    0x78 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Asl))),
                    0x79 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Rol))),
                    0x7A => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Dec))),
                    0x7C => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Inc))),
                    0x7D => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Tst))),
                    0x7E => self.read_next(CpuState::ReadExtendedHi(ExtOp::Jmp)),
                    0x7F => self.read_next(CpuState::ReadExtendedHi(ExtOp::Rmw(Rmw8Op::Clr))),
                    0x80 => self.read_next(CpuState::ReadImm8(Imm8Op::AluA(Alu8Op::Sub))),
                    0x81 => self.read_next(CpuState::ReadImm8(Imm8Op::Cmpa)),
                    0x82 => self.read_next(CpuState::ReadImm8(Imm8Op::AluA(Alu8Op::SubCarry))),
                    0x83 => self.read_next(CpuState::ReadImm16Hi(Imm16Op::AluD(Alu16Op::Sub))),
                    0x84 => self.read_next(CpuState::ReadImm8(Imm8Op::AluA(Alu8Op::And))),
                    0x85 => self.read_next(CpuState::ReadImm8(Imm8Op::AluA(Alu8Op::Bit))),
                    0x86 => self.read_next(CpuState::ReadImm8(Imm8Op::Lda)),
                    0x88 => self.read_next(CpuState::ReadImm8(Imm8Op::AluA(Alu8Op::Eor))),
                    0x89 => self.read_next(CpuState::ReadImm8(Imm8Op::AluA(Alu8Op::AddCarry))),
                    0x8A => self.read_next(CpuState::ReadImm8(Imm8Op::AluA(Alu8Op::Or))),
                    0x8B => self.read_next(CpuState::ReadImm8(Imm8Op::AluA(Alu8Op::Add))),
                    0x8D => self.read_next(CpuState::ReadRel8(Rel8Op::Bsr)),
                    0x8C => {
                        self.read_next(CpuState::ReadImm16Hi(Imm16Op::Compare(WordReg::X)));
                    }
                    0x8E => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Load(WordReg::X))),
                    0x90 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluA(Alu8Op::Sub))),
                    0x91 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Cmpa)),
                    0x92 => {
                        self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluA(Alu8Op::SubCarry)));
                    }
                    0x93 => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::AluD(Alu16Op::Sub)))
                    }
                    0x94 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluA(Alu8Op::And))),
                    0x95 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluA(Alu8Op::Bit))),
                    0x96 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Lda)),
                    0x97 => self.read_next(CpuState::WriteDirectOperand(Store8Op::Sta)),
                    0x98 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluA(Alu8Op::Eor))),
                    0x99 => {
                        self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluA(Alu8Op::AddCarry)));
                    }
                    0x9A => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluA(Alu8Op::Or))),
                    0x9B => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluA(Alu8Op::Add))),
                    0x9C => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Compare(WordReg::X)))
                    }
                    0x9D => self.read_next(CpuState::ReadDirectJsrOperand),
                    0x9E => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Load(WordReg::X)));
                    }
                    0x9F => {
                        self.read_next(CpuState::WriteDirectOperand16(Store16Op::Store(
                            WordReg::X,
                        )));
                    }
                    0xA0 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluA(Alu8Op::Sub),
                    ))),
                    0xA1 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                            Mem8Op::Cmpa,
                        )));
                    }
                    0xA2 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluA(Alu8Op::SubCarry),
                    ))),
                    0xA3 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::AluD(Alu16Op::Sub),
                    ))),
                    0xA4 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluA(Alu8Op::And),
                    ))),
                    0xA5 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluA(Alu8Op::Bit),
                    ))),
                    0xA6 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(Mem8Op::Lda)));
                    }
                    0xA7 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Store(
                        Store8Op::Sta,
                    ))),
                    0xA8 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluA(Alu8Op::Eor),
                    ))),
                    0xA9 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluA(Alu8Op::AddCarry),
                    ))),
                    0xAA => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluA(Alu8Op::Or),
                    ))),
                    0xAB => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluA(Alu8Op::Add),
                    ))),
                    0xAC => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Compare(WordReg::X),
                    ))),
                    0xAD => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Jsr)),
                    0xAE => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Load(WordReg::X),
                    ))),
                    0xAF => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Store16(
                        Store16Op::Store(WordReg::X),
                    ))),
                    0xB0 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluA(
                        Alu8Op::Sub,
                    )))),
                    0xB1 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Cmpa))),
                    0xB2 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluA(
                        Alu8Op::SubCarry,
                    )))),
                    0xB3 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(Mem16Op::AluD(
                        Alu16Op::Sub,
                    )))),
                    0xB4 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluA(
                        Alu8Op::And,
                    )))),
                    0xB5 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluA(
                        Alu8Op::Bit,
                    )))),
                    0xB6 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Lda))),
                    0xB7 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store(Store8Op::Sta))),
                    0xB8 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluA(
                        Alu8Op::Eor,
                    )))),
                    0xB9 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluA(
                        Alu8Op::AddCarry,
                    )))),
                    0xBA => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluA(
                        Alu8Op::Or,
                    )))),
                    0xBB => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluA(
                        Alu8Op::Add,
                    )))),
                    0xBC => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(
                        Mem16Op::Compare(WordReg::X),
                    ))),
                    0xBD => self.read_next(CpuState::ReadExtendedHi(ExtOp::Jsr)),
                    0xBE => {
                        self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(Mem16Op::Load(
                            WordReg::X,
                        ))));
                    }
                    0xBF => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store16(
                        Store16Op::Store(WordReg::X),
                    ))),
                    0xC0 => self.read_next(CpuState::ReadImm8(Imm8Op::AluB(Alu8Op::Sub))),
                    0xC1 => self.read_next(CpuState::ReadImm8(Imm8Op::Cmpb)),
                    0xC2 => self.read_next(CpuState::ReadImm8(Imm8Op::AluB(Alu8Op::SubCarry))),
                    0xC3 => self.read_next(CpuState::ReadImm16Hi(Imm16Op::AluD(Alu16Op::Add))),
                    0xC4 => self.read_next(CpuState::ReadImm8(Imm8Op::AluB(Alu8Op::And))),
                    0xC5 => self.read_next(CpuState::ReadImm8(Imm8Op::AluB(Alu8Op::Bit))),
                    0xC6 => self.read_next(CpuState::ReadImm8(Imm8Op::Ldb)),
                    0xC8 => self.read_next(CpuState::ReadImm8(Imm8Op::AluB(Alu8Op::Eor))),
                    0xC9 => self.read_next(CpuState::ReadImm8(Imm8Op::AluB(Alu8Op::AddCarry))),
                    0xCA => self.read_next(CpuState::ReadImm8(Imm8Op::AluB(Alu8Op::Or))),
                    0xCB => self.read_next(CpuState::ReadImm8(Imm8Op::AluB(Alu8Op::Add))),
                    0xCC => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Load(WordReg::D))),
                    0xCE => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Load(WordReg::U))),
                    0xD0 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluB(Alu8Op::Sub))),
                    0xD1 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Cmpb)),
                    0xD2 => {
                        self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluB(Alu8Op::SubCarry)));
                    }
                    0xD3 => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::AluD(Alu16Op::Add)))
                    }
                    0xD4 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluB(Alu8Op::And))),
                    0xD5 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluB(Alu8Op::Bit))),
                    0xD6 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Ldb)),
                    0xD7 => self.read_next(CpuState::WriteDirectOperand(Store8Op::Stb)),
                    0xD8 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluB(Alu8Op::Eor))),
                    0xD9 => {
                        self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluB(Alu8Op::AddCarry)));
                    }
                    0xDA => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluB(Alu8Op::Or))),
                    0xDB => self.read_next(CpuState::ReadDirectOperand(Mem8Op::AluB(Alu8Op::Add))),
                    0xDC => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Load(WordReg::D)));
                    }
                    0xDD => {
                        self.read_next(CpuState::WriteDirectOperand16(Store16Op::Store(
                            WordReg::D,
                        )));
                    }
                    0xDE => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Load(WordReg::U)));
                    }
                    0xDF => {
                        self.read_next(CpuState::WriteDirectOperand16(Store16Op::Store(
                            WordReg::U,
                        )));
                    }
                    0xE0 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluB(Alu8Op::Sub),
                    ))),
                    0xE1 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                            Mem8Op::Cmpb,
                        )));
                    }
                    0xE2 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluB(Alu8Op::SubCarry),
                    ))),
                    0xE3 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::AluD(Alu16Op::Add),
                    ))),
                    0xE4 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluB(Alu8Op::And),
                    ))),
                    0xE5 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluB(Alu8Op::Bit),
                    ))),
                    0xE6 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(Mem8Op::Ldb)));
                    }
                    0xE7 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Store(
                        Store8Op::Stb,
                    ))),
                    0xE8 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluB(Alu8Op::Eor),
                    ))),
                    0xE9 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluB(Alu8Op::AddCarry),
                    ))),
                    0xEA => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluB(Alu8Op::Or),
                    ))),
                    0xEB => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                        Mem8Op::AluB(Alu8Op::Add),
                    ))),
                    0xEC => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Load(WordReg::D),
                    ))),
                    0xED => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Store16(
                        Store16Op::Store(WordReg::D),
                    ))),
                    0xEE => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Load(WordReg::U),
                    ))),
                    0xEF => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Store16(
                        Store16Op::Store(WordReg::U),
                    ))),
                    0xF0 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluB(
                        Alu8Op::Sub,
                    )))),
                    0xF1 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Cmpb))),
                    0xF2 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluB(
                        Alu8Op::SubCarry,
                    )))),
                    0xF3 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(Mem16Op::AluD(
                        Alu16Op::Add,
                    )))),
                    0xF4 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluB(
                        Alu8Op::And,
                    )))),
                    0xF5 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluB(
                        Alu8Op::Bit,
                    )))),
                    0xF6 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Ldb))),
                    0xF8 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluB(
                        Alu8Op::Eor,
                    )))),
                    0xF9 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluB(
                        Alu8Op::AddCarry,
                    )))),
                    0xFA => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluB(
                        Alu8Op::Or,
                    )))),
                    0xFB => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::AluB(
                        Alu8Op::Add,
                    )))),
                    0xF7 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store(Store8Op::Stb))),
                    0xFC => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(Mem16Op::Load(
                        WordReg::D,
                    )))),
                    0xFD => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store16(
                        Store16Op::Store(WordReg::D),
                    ))),
                    0xFE => {
                        self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(Mem16Op::Load(
                            WordReg::U,
                        ))));
                    }
                    0xFF => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store16(
                        Store16Op::Store(WordReg::U),
                    ))),
                    _ => self.trap_illegal(opcode),
                }
            }
            CpuState::Prefix10 => {
                let opcode = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                match opcode {
                    0x21 => self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(
                        BranchCondition::Never,
                    ))),
                    0x22 => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Hi)))
                    }
                    0x23 => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Ls)))
                    }
                    0x24 => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Cc)))
                    }
                    0x25 => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Cs)))
                    }
                    0x26 => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Ne)))
                    }
                    0x27 => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Eq)))
                    }
                    0x28 => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Vc)))
                    }
                    0x29 => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Vs)))
                    }
                    0x2A => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Pl)))
                    }
                    0x2B => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Mi)))
                    }
                    0x2C => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Ge)))
                    }
                    0x2D => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Lt)))
                    }
                    0x2E => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Gt)))
                    }
                    0x2F => {
                        self.read_next(CpuState::ReadRel16Hi(Rel16Op::Branch(BranchCondition::Le)))
                    }
                    0x3F => self.start_software_interrupt(Vector::Swi2),
                    0x83 => {
                        self.read_next(CpuState::ReadImm16Hi(Imm16Op::Compare(WordReg::D)));
                    }
                    0x8C => {
                        self.read_next(CpuState::ReadImm16Hi(Imm16Op::Compare(WordReg::Y)));
                    }
                    0x8E => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Load(WordReg::Y))),
                    0x93 => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Compare(WordReg::D)))
                    }
                    0x9C => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Compare(WordReg::Y)))
                    }
                    0x9E => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Load(WordReg::Y)));
                    }
                    0x9F => {
                        self.read_next(CpuState::WriteDirectOperand16(Store16Op::Store(
                            WordReg::Y,
                        )));
                    }
                    0xA3 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Compare(WordReg::D),
                    ))),
                    0xAC => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Compare(WordReg::Y),
                    ))),
                    0xAE => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Load(WordReg::Y),
                    ))),
                    0xAF => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Store16(
                        Store16Op::Store(WordReg::Y),
                    ))),
                    0xB3 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(
                        Mem16Op::Compare(WordReg::D),
                    ))),
                    0xBC => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(
                        Mem16Op::Compare(WordReg::Y),
                    ))),
                    0xBE => {
                        self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(Mem16Op::Load(
                            WordReg::Y,
                        ))));
                    }
                    0xBF => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store16(
                        Store16Op::Store(WordReg::Y),
                    ))),
                    0xCE => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Load(WordReg::S))),
                    0xDE => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Load(WordReg::S)));
                    }
                    0xDF => {
                        self.read_next(CpuState::WriteDirectOperand16(Store16Op::Store(
                            WordReg::S,
                        )));
                    }
                    0xEE => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Load(WordReg::S),
                    ))),
                    0xEF => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Store16(
                        Store16Op::Store(WordReg::S),
                    ))),
                    0xFE => {
                        self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(Mem16Op::Load(
                            WordReg::S,
                        ))));
                    }
                    0xFF => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store16(
                        Store16Op::Store(WordReg::S),
                    ))),
                    _ => self.trap_illegal(opcode),
                }
            }
            CpuState::Prefix11 => {
                let opcode = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                match opcode {
                    0x3F => self.start_software_interrupt(Vector::Swi3),
                    0x83 => {
                        self.read_next(CpuState::ReadImm16Hi(Imm16Op::Compare(WordReg::U)));
                    }
                    0x8C => {
                        self.read_next(CpuState::ReadImm16Hi(Imm16Op::Compare(WordReg::S)));
                    }
                    0x93 => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Compare(WordReg::U)))
                    }
                    0x9C => {
                        self.read_next(CpuState::ReadDirectOperand16(Mem16Op::Compare(WordReg::S)))
                    }
                    0xA3 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Compare(WordReg::U),
                    ))),
                    0xAC => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load16(
                        Mem16Op::Compare(WordReg::S),
                    ))),
                    0xB3 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(
                        Mem16Op::Compare(WordReg::U),
                    ))),
                    0xBC => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load16(
                        Mem16Op::Compare(WordReg::S),
                    ))),
                    _ => self.trap_illegal(opcode),
                }
            }
            CpuState::NopInternal => {
                self.next_fetch();
            }
            CpuState::Internal { remaining } => {
                if remaining <= 1 {
                    self.next_fetch();
                } else {
                    self.state = CpuState::Internal {
                        remaining: remaining - 1,
                    };
                    self.addr = self.regs.pc;
                    self.rw = true;
                    self.sync = false;
                }
            }
            CpuState::ClrAInternal | CpuState::ClrBInternal => {
                self.next_fetch();
            }
            CpuState::ReadCcImm(op) => {
                let value = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                match op {
                    CcOp::And => self.regs.cc &= value,
                    CcOp::Or => self.regs.cc |= value,
                }
                self.next_fetch();
            }
            CpuState::ReadTransferPostbyte(op) => {
                let post = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                match op {
                    TransferOp::Exg => {
                        self.exg(post);
                        self.start_internal_cycles(6);
                    }
                    TransferOp::Tfr => {
                        self.tfr(post);
                        self.start_internal_cycles(4);
                    }
                }
            }
            CpuState::ReadImm8(op) => {
                let value = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.load_imm8(op, value);
                self.next_fetch();
            }
            CpuState::ReadImm16Hi(op) => {
                let hi = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.read_next(CpuState::ReadImm16Lo { op, hi });
            }
            CpuState::ReadImm16Lo { op, hi } => {
                let value = u16::from_be_bytes([hi, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.load_imm16(op, value);
                self.next_fetch();
            }
            CpuState::ReadRel8(op) => {
                let offset = self.data_in as i8;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let target = self.regs.pc.wrapping_add_signed(i16::from(offset));
                match op {
                    Rel8Op::Branch(condition) => {
                        if self.branch_condition(condition) {
                            self.regs.pc = target;
                        }
                        self.start_internal_cycles(1);
                    }
                    Rel8Op::Bsr => self.prepare_push_word(
                        self.regs.pc,
                        AfterPush::SetPcAfterInternal {
                            pc: target,
                            cycles: 3,
                        },
                    ),
                }
            }
            CpuState::ReadRel16Hi(op) => {
                let hi = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.read_next(CpuState::ReadRel16Lo { op, hi });
            }
            CpuState::ReadRel16Lo { op, hi } => {
                let offset = i16::from_be_bytes([hi, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let target = self.regs.pc.wrapping_add_signed(offset);
                match op {
                    Rel16Op::Branch(condition) => {
                        let taken = self.branch_condition(condition);
                        if taken {
                            self.regs.pc = target;
                        }
                        self.start_internal_cycles(if taken { 3 } else { 2 });
                    }
                    Rel16Op::Lbsr => self.prepare_push_word(
                        self.regs.pc,
                        AfterPush::SetPcAfterInternal {
                            pc: target,
                            cycles: 4,
                        },
                    ),
                }
            }
            CpuState::ReadIndexedPostbyte(op) => {
                let post = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.resolve_indexed_postbyte(op, post);
            }
            CpuState::ReadIndexedOffset8 {
                op,
                post,
                extra_cycles,
            } => {
                let offset = self.data_in as i8;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let low5 = Self::indexed_low5(post);
                let base = if low5 == 0x0C || low5 == 0x1C {
                    self.regs.pc
                } else {
                    self.index_base(post)
                };
                let addr = base.wrapping_add_signed(i16::from(offset));
                if matches!(low5, 0x18 | 0x1C) {
                    self.start_indexed_indirect(op, addr, extra_cycles);
                } else {
                    self.start_indexed_effective_address(op, addr, extra_cycles);
                }
            }
            CpuState::ReadIndexedOffset16Hi {
                op,
                post,
                extra_cycles,
            } => {
                let hi = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.read_next(CpuState::ReadIndexedOffset16Lo {
                    op,
                    post,
                    hi,
                    extra_cycles,
                });
            }
            CpuState::ReadIndexedOffset16Lo {
                op,
                post,
                hi,
                extra_cycles,
            } => {
                let offset = u16::from_be_bytes([hi, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let low5 = Self::indexed_low5(post);
                let base = if low5 == 0x0D || low5 == 0x1D {
                    self.regs.pc
                } else {
                    self.index_base(post)
                };
                let addr = if low5 == 0x1F {
                    offset
                } else {
                    base.wrapping_add(offset)
                };
                if matches!(low5, 0x19 | 0x1D | 0x1F) {
                    self.start_indexed_indirect(op, addr, extra_cycles);
                } else {
                    self.start_indexed_effective_address(op, addr, extra_cycles);
                }
            }
            CpuState::ReadIndexedIndirectHi {
                op,
                ptr,
                extra_cycles,
            } => {
                let hi = self.data_in;
                self.state = CpuState::ReadIndexedIndirectLo {
                    op,
                    hi,
                    extra_cycles,
                };
                self.addr = ptr.wrapping_add(1);
                self.rw = true;
                self.sync = false;
            }
            CpuState::ReadIndexedIndirectLo {
                op,
                hi,
                extra_cycles,
            } => {
                let addr = u16::from_be_bytes([hi, self.data_in]);
                self.start_indexed_effective_address(op, addr, extra_cycles);
            }
            CpuState::IndexedExtra {
                op,
                addr,
                remaining,
            } => {
                if remaining <= 1 {
                    self.apply_indexed_effective_address(op, addr);
                } else {
                    self.state = CpuState::IndexedExtra {
                        op,
                        addr,
                        remaining: remaining - 1,
                    };
                    self.addr = self.regs.pc;
                    self.rw = true;
                    self.sync = false;
                }
            }
            CpuState::ReadStackPostbyte(op) => {
                let mask = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.start_stack_op(op, mask);
            }
            CpuState::ReadDirectRmwOperand(op) => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.prepare_rmw(op, addr, Self::rmw_post_cycles(op));
            }
            CpuState::ReadDirectJmpOperand => {
                self.regs.pc = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.start_internal_cycles(1);
            }
            CpuState::ReadDirectJsrOperand => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.prepare_push_word(
                    self.regs.pc,
                    AfterPush::SetPcAfterInternal {
                        pc: addr,
                        cycles: 3,
                    },
                );
            }
            CpuState::ReadDirectOperand(op) => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.state = CpuState::ReadDirectValue(op);
                self.addr = addr;
                self.rw = true;
                self.sync = false;
            }
            CpuState::ReadDirectOperand16(op) => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.prepare_read16(op, addr);
            }
            CpuState::ReadDirectValue(op) => {
                self.load_mem8(op, self.data_in);
                self.start_internal_cycles(1);
            }
            CpuState::ReadRmwValue {
                op,
                addr,
                post_cycles,
            } => match self.rmw8(op, self.data_in) {
                Some(value) => {
                    self.state = if post_cycles == 0 {
                        CpuState::WriteValue
                    } else {
                        CpuState::WriteValueThenInternal {
                            cycles: post_cycles,
                        }
                    };
                    self.addr = addr;
                    self.data = value;
                    self.rw = false;
                    self.sync = false;
                }
                None => self.start_internal_cycles(post_cycles),
            },
            CpuState::ReadMem16Hi { op, addr } => {
                let hi = self.data_in;
                self.state = CpuState::ReadMem16Lo { op, hi };
                self.addr = addr.wrapping_add(1);
                self.rw = true;
                self.sync = false;
            }
            CpuState::ReadMem16Lo { op, hi } => {
                let value = u16::from_be_bytes([hi, self.data_in]);
                self.load_mem16(op, value);
                self.start_internal_cycles(1);
            }
            CpuState::ReadExtendedHi(op) => {
                let hi = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.read_next(CpuState::ReadExtendedLo { op, hi });
            }
            CpuState::ReadExtendedLo { op, hi } => {
                let addr = u16::from_be_bytes([hi, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                match op {
                    ExtOp::Load(op) => {
                        self.state = CpuState::ReadExtendedValue(op);
                        self.addr = addr;
                        self.rw = true;
                        self.sync = false;
                    }
                    ExtOp::Store(op) => self.prepare_store(op, addr, 1),
                    ExtOp::Rmw(op) => self.prepare_rmw(op, addr, Self::rmw_post_cycles(op)),
                    ExtOp::Load16(op) => self.prepare_read16(op, addr),
                    ExtOp::Store16(op) => self.prepare_store16(op, addr, 1),
                    ExtOp::Jmp => {
                        self.regs.pc = addr;
                        self.next_fetch();
                    }
                    ExtOp::Jsr => self.prepare_push_word(
                        self.regs.pc,
                        AfterPush::SetPcAfterInternal {
                            pc: addr,
                            cycles: 3,
                        },
                    ),
                }
            }
            CpuState::ReadExtendedValue(op) => {
                self.load_mem8(op, self.data_in);
                self.start_internal_cycles(1);
            }
            CpuState::WriteDirectOperand(op) => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.prepare_store(op, addr, 1);
            }
            CpuState::WriteDirectOperand16(op) => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.prepare_store16(op, addr, 1);
            }
            CpuState::WriteValue => {
                self.next_fetch();
            }
            CpuState::WriteValueThenInternal { cycles } => {
                self.start_internal_cycles(cycles);
            }
            CpuState::Write16Lo { lo, post_cycles } => {
                self.state = if post_cycles == 0 {
                    CpuState::WriteValue
                } else {
                    CpuState::WriteValueThenInternal {
                        cycles: post_cycles,
                    }
                };
                self.addr = self.addr.wrapping_add(1);
                self.data = lo;
                self.rw = false;
                self.sync = false;
            }
            CpuState::StackRead => {
                self.finish_stack_read();
            }
            CpuState::StackWrite => {
                self.finish_stack_write();
            }
            CpuState::PushWordHi { hi, after } => {
                self.regs.s = self.regs.s.wrapping_sub(1);
                self.state = CpuState::PushDone(after);
                self.addr = self.regs.s;
                self.data = hi;
                self.rw = false;
                self.sync = false;
            }
            CpuState::PushDone(after) => {
                self.after_push(after);
            }
            CpuState::PullWordHi(op) => {
                let hi = self.data_in;
                self.regs.s = self.regs.s.wrapping_add(1);
                self.state = CpuState::PullWordLo { op, hi };
                self.addr = self.regs.s;
                self.rw = true;
                self.sync = false;
            }
            CpuState::PullWordLo { op, hi } => {
                let value = u16::from_be_bytes([hi, self.data_in]);
                self.regs.s = self.regs.s.wrapping_add(1);
                match op {
                    Pull16Op::Pc => {
                        self.regs.pc = value;
                        self.start_internal_cycles(2);
                    }
                }
            }
            CpuState::RtiReadCc => {
                self.regs.cc = self.data_in;
                self.regs.s = self.regs.s.wrapping_add(1);
                if self.regs.flag(FLAG_E) {
                    self.start_stack_pull(StackPointer::S, 0xFE);
                } else {
                    self.prepare_pull_word(Pull16Op::Pc);
                }
            }
            CpuState::ReadCwaiMask => {
                self.regs.cc &= self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.regs.set_flag(FLAG_E, true);
                self.start_stack_push(StackPointer::S, 0xFF, AfterStack::WaitForInterrupt);
            }
            CpuState::WaitForInterrupt { stacked } => {
                if let Some(vector) = self.pending_interrupt() {
                    if stacked {
                        self.apply_interrupt_post_stack_flags(vector);
                        self.read_vector(vector);
                    } else {
                        self.start_interrupt(vector);
                    }
                } else if !stacked && self.maskable_interrupt_asserted() {
                    // SYNC wakes on masked IRQ/FIRQ without stacking; CWAI
                    // has already stacked and waits for a serviceable vector.
                    self.next_fetch();
                } else {
                    self.addr = self.regs.pc;
                    self.rw = true;
                    self.sync = false;
                }
            }
            CpuState::ReadVectorHi(vector) => {
                let hi = self.data_in;
                self.state = CpuState::ReadVectorLo { vector, hi };
                self.addr = vector.addr().wrapping_add(1);
                self.rw = true;
                self.sync = false;
            }
            CpuState::ReadVectorLo { hi, .. } => {
                self.regs.pc = u16::from_be_bytes([hi, self.data_in]);
                self.next_fetch();
            }
            CpuState::IllegalOpcode(_) => {
                self.halt = true;
            }
        }
    }

    fn read_next(&mut self, state: CpuState) {
        self.state = state;
        self.addr = self.regs.pc;
        self.rw = true;
        self.sync = false;
    }

    fn next_fetch(&mut self) {
        self.state = CpuState::Fetch;
        self.stack_work = StackWork::None;
        self.addr = self.regs.pc;
        self.rw = true;
        self.sync = true;
    }

    fn start_internal_cycles(&mut self, remaining: u8) {
        if remaining == 0 {
            self.next_fetch();
        } else {
            self.state = CpuState::Internal { remaining };
            self.addr = self.regs.pc;
            self.rw = true;
            self.sync = false;
        }
    }

    fn pending_interrupt(&mut self) -> Option<Vector> {
        if self.nmi {
            self.nmi = false;
            Some(Vector::Nmi)
        } else if self.firq && !self.regs.firq_masked() {
            Some(Vector::Firq)
        } else if self.irq && !self.regs.irq_masked() {
            Some(Vector::Irq)
        } else {
            None
        }
    }

    fn maskable_interrupt_asserted(&self) -> bool {
        self.irq || self.firq
    }

    fn start_interrupt(&mut self, vector: Vector) {
        self.apply_interrupt_pre_stack_flags(vector);
        let mask = match vector {
            Vector::Firq => 0x81,
            Vector::Irq | Vector::Nmi => 0xFF,
            Vector::Swi | Vector::Swi2 | Vector::Swi3 => unreachable!(),
        };
        self.start_stack_push(StackPointer::S, mask, AfterStack::ReadVector(vector));
    }

    fn start_software_interrupt(&mut self, vector: Vector) {
        self.regs.set_flag(FLAG_E, true);
        self.start_stack_push(StackPointer::S, 0xFF, AfterStack::ReadVector(vector));
    }

    fn apply_interrupt_pre_stack_flags(&mut self, vector: Vector) {
        match vector {
            Vector::Firq => {
                self.regs.set_flag(FLAG_E, false);
            }
            Vector::Irq | Vector::Nmi => {
                self.regs.set_flag(FLAG_E, true);
            }
            Vector::Swi | Vector::Swi2 | Vector::Swi3 => {}
        }
    }

    fn apply_interrupt_post_stack_flags(&mut self, vector: Vector) {
        match vector {
            Vector::Firq | Vector::Swi => {
                self.regs.set_flag(FLAG_I, true);
                self.regs.set_flag(FLAG_F, true);
            }
            Vector::Irq | Vector::Nmi => {
                self.regs.set_flag(FLAG_I, true);
            }
            Vector::Swi2 | Vector::Swi3 => {}
        }
    }

    fn read_vector(&mut self, vector: Vector) {
        self.state = CpuState::ReadVectorHi(vector);
        self.addr = vector.addr();
        self.rw = true;
        self.sync = false;
    }

    fn after_stack(&mut self, after: AfterStack) {
        match after {
            AfterStack::Fetch => self.next_fetch(),
            AfterStack::ReadVector(vector) => {
                self.apply_interrupt_post_stack_flags(vector);
                self.read_vector(vector);
            }
            AfterStack::WaitForInterrupt => {
                if let Some(vector) = self.pending_interrupt() {
                    self.apply_interrupt_post_stack_flags(vector);
                    self.read_vector(vector);
                } else {
                    self.state = CpuState::WaitForInterrupt { stacked: true };
                    self.addr = self.regs.pc;
                    self.rw = true;
                    self.sync = false;
                }
            }
        }
    }

    fn trap_illegal(&mut self, opcode: u8) {
        self.state = CpuState::IllegalOpcode(opcode);
        self.halt = true;
        self.addr = self.regs.pc;
        self.rw = true;
        self.sync = false;
    }

    fn clear_a(&mut self) {
        self.regs.a = 0;
        self.set_nz8(0);
        self.regs.set_flag(FLAG_V, false);
        self.regs.set_flag(FLAG_C, false);
    }

    fn clear_b(&mut self) {
        self.regs.b = 0;
        self.set_nz8(0);
        self.regs.set_flag(FLAG_V, false);
        self.regs.set_flag(FLAG_C, false);
    }

    fn load_imm8(&mut self, op: Imm8Op, value: u8) {
        match op {
            Imm8Op::Lda => {
                self.regs.a = value;
                self.set_load_flags8(value);
            }
            Imm8Op::Ldb => {
                self.regs.b = value;
                self.set_load_flags8(value);
            }
            Imm8Op::Cmpa => self.compare8(self.regs.a, value),
            Imm8Op::Cmpb => self.compare8(self.regs.b, value),
            Imm8Op::AluA(op) => self.alu_a(op, value),
            Imm8Op::AluB(op) => self.alu_b(op, value),
        }
    }

    fn load_mem8(&mut self, op: Mem8Op, value: u8) {
        match op {
            Mem8Op::Lda => {
                self.regs.a = value;
                self.set_load_flags8(value);
            }
            Mem8Op::Ldb => {
                self.regs.b = value;
                self.set_load_flags8(value);
            }
            Mem8Op::Cmpa => self.compare8(self.regs.a, value),
            Mem8Op::Cmpb => self.compare8(self.regs.b, value),
            Mem8Op::AluA(op) => self.alu_a(op, value),
            Mem8Op::AluB(op) => self.alu_b(op, value),
        }
    }

    fn load_imm16(&mut self, op: Imm16Op, value: u16) {
        match op {
            Imm16Op::Load(reg) => {
                self.write_word_reg(reg, value);
                self.set_load_flags16(value);
            }
            Imm16Op::Compare(reg) => self.compare16(self.read_word_reg(reg), value),
            Imm16Op::AluD(op) => self.alu_d(op, value),
        }
    }

    fn load_mem16(&mut self, op: Mem16Op, value: u16) {
        match op {
            Mem16Op::Load(reg) => {
                self.write_word_reg(reg, value);
                self.set_load_flags16(value);
            }
            Mem16Op::Compare(reg) => self.compare16(self.read_word_reg(reg), value),
            Mem16Op::AluD(op) => self.alu_d(op, value),
        }
    }

    fn resolve_indexed_postbyte(&mut self, op: IndexedOp, post: u8) {
        if post & 0x80 == 0 {
            let offset = if post & 0x10 != 0 {
                i16::from((post | 0xE0) as i8)
            } else {
                i16::from((post & 0x1F) as i8)
            };
            let addr = self.index_base(post).wrapping_add_signed(offset);
            let extra_cycles = if matches!(op, IndexedOp::Lea(_)) {
                2
            } else {
                1
            };
            self.start_indexed_effective_address(op, addr, extra_cycles);
            return;
        }

        match Self::indexed_low5(post) {
            0x00 => {
                let addr = self.index_base(post);
                self.set_index_base(post, addr.wrapping_add(1));
                self.start_indexed_effective_address(op, addr, 2);
            }
            0x01 => {
                let addr = self.index_base(post);
                self.set_index_base(post, addr.wrapping_add(2));
                self.start_indexed_effective_address(op, addr, 3);
            }
            0x02 => {
                let addr = self.index_base(post).wrapping_sub(1);
                self.set_index_base(post, addr);
                self.start_indexed_effective_address(op, addr, 2);
            }
            0x03 => {
                let addr = self.index_base(post).wrapping_sub(2);
                self.set_index_base(post, addr);
                self.start_indexed_effective_address(op, addr, 3);
            }
            0x04 => self.start_indexed_effective_address(op, self.index_base(post), 0),
            0x05 => {
                let offset = i16::from(self.regs.b as i8);
                self.start_indexed_effective_address(
                    op,
                    self.index_base(post).wrapping_add_signed(offset),
                    1,
                );
            }
            0x06 => {
                let offset = i16::from(self.regs.a as i8);
                self.start_indexed_effective_address(
                    op,
                    self.index_base(post).wrapping_add_signed(offset),
                    1,
                );
            }
            0x08 | 0x0C => self.read_next(CpuState::ReadIndexedOffset8 {
                op,
                post,
                extra_cycles: 0,
            }),
            0x09 => self.read_next(CpuState::ReadIndexedOffset16Hi {
                op,
                post,
                extra_cycles: 2,
            }),
            0x0D => self.read_next(CpuState::ReadIndexedOffset16Hi {
                op,
                post,
                extra_cycles: 3,
            }),
            0x0B => self.start_indexed_effective_address(
                op,
                self.index_base(post).wrapping_add(self.regs.d()),
                4,
            ),
            0x11 => {
                let ptr = self.index_base(post);
                self.set_index_base(post, ptr.wrapping_add(2));
                self.start_indexed_indirect(op, ptr, 4);
            }
            0x13 => {
                let ptr = self.index_base(post).wrapping_sub(2);
                self.set_index_base(post, ptr);
                self.start_indexed_indirect(op, ptr, 4);
            }
            0x14 => self.start_indexed_indirect(op, self.index_base(post), 1),
            0x15 => {
                let offset = i16::from(self.regs.b as i8);
                self.start_indexed_indirect(
                    op,
                    self.index_base(post).wrapping_add_signed(offset),
                    2,
                );
            }
            0x16 => {
                let offset = i16::from(self.regs.a as i8);
                self.start_indexed_indirect(
                    op,
                    self.index_base(post).wrapping_add_signed(offset),
                    2,
                );
            }
            0x18 | 0x1C => self.read_next(CpuState::ReadIndexedOffset8 {
                op,
                post,
                extra_cycles: 1,
            }),
            0x19 => self.read_next(CpuState::ReadIndexedOffset16Hi {
                op,
                post,
                extra_cycles: 3,
            }),
            0x1D => self.read_next(CpuState::ReadIndexedOffset16Hi {
                op,
                post,
                extra_cycles: 4,
            }),
            0x1F => self.read_next(CpuState::ReadIndexedOffset16Hi {
                op,
                post,
                extra_cycles: 1,
            }),
            0x1B => self.start_indexed_indirect(
                op,
                self.index_base(post).wrapping_add(self.regs.d()),
                5,
            ),
            _ => self.trap_illegal(post),
        }
    }

    fn start_indexed_indirect(&mut self, op: IndexedOp, ptr: u16, extra_cycles: u8) {
        self.state = CpuState::ReadIndexedIndirectHi {
            op,
            ptr,
            extra_cycles,
        };
        self.addr = ptr;
        self.rw = true;
        self.sync = false;
    }

    fn start_indexed_effective_address(&mut self, op: IndexedOp, addr: u16, extra_cycles: u8) {
        if extra_cycles == 0 {
            self.apply_indexed_effective_address(op, addr);
        } else {
            self.state = CpuState::IndexedExtra {
                op,
                addr,
                remaining: extra_cycles,
            };
            self.addr = self.regs.pc;
            self.rw = true;
            self.sync = false;
        }
    }

    fn apply_indexed_effective_address(&mut self, op: IndexedOp, addr: u16) {
        match op {
            IndexedOp::Lea(reg) => {
                self.set_reg16(reg, addr);
                if matches!(reg, Reg16::X | Reg16::Y) {
                    self.regs.set_flag(FLAG_Z, addr == 0);
                }
                self.start_internal_cycles(1);
            }
            IndexedOp::Load(op) => {
                self.state = CpuState::ReadExtendedValue(op);
                self.addr = addr;
                self.rw = true;
                self.sync = false;
            }
            IndexedOp::Store(op) => self.prepare_store(op, addr, 1),
            IndexedOp::Rmw(op) => self.prepare_rmw(op, addr, Self::rmw_post_cycles(op)),
            IndexedOp::Load16(op) => self.prepare_read16(op, addr),
            IndexedOp::Store16(op) => self.prepare_store16(op, addr, 1),
            IndexedOp::Jmp => {
                self.regs.pc = addr;
                self.next_fetch();
            }
            IndexedOp::Jsr => self.prepare_push_word(
                self.regs.pc,
                AfterPush::SetPcAfterInternal {
                    pc: addr,
                    cycles: 3,
                },
            ),
        }
    }

    fn indexed_low5(post: u8) -> u8 {
        post & 0x1F
    }

    fn index_base(&self, post: u8) -> u16 {
        match (post >> 5) & 0x03 {
            0 => self.regs.x,
            1 => self.regs.y,
            2 => self.regs.u,
            _ => self.regs.s,
        }
    }

    fn set_index_base(&mut self, post: u8, value: u16) {
        match (post >> 5) & 0x03 {
            0 => self.regs.x = value,
            1 => self.regs.y = value,
            2 => self.regs.u = value,
            _ => self.regs.s = value,
        }
    }

    fn set_reg16(&mut self, reg: Reg16, value: u16) {
        match reg {
            Reg16::X => self.regs.x = value,
            Reg16::Y => self.regs.y = value,
            Reg16::U => self.regs.u = value,
            Reg16::S => self.regs.s = value,
        }
    }

    fn read_word_reg(&self, reg: WordReg) -> u16 {
        match reg {
            WordReg::D => self.regs.d(),
            WordReg::X => self.regs.x,
            WordReg::Y => self.regs.y,
            WordReg::U => self.regs.u,
            WordReg::S => self.regs.s,
        }
    }

    fn write_word_reg(&mut self, reg: WordReg, value: u16) {
        match reg {
            WordReg::D => {
                let [hi, lo] = value.to_be_bytes();
                self.regs.a = hi;
                self.regs.b = lo;
            }
            WordReg::X => self.regs.x = value,
            WordReg::Y => self.regs.y = value,
            WordReg::U => self.regs.u = value,
            WordReg::S => self.regs.s = value,
        }
    }

    fn transfer_reg(code: u8) -> Option<TransferReg> {
        match code {
            0x0 => Some(TransferReg::D),
            0x1 => Some(TransferReg::X),
            0x2 => Some(TransferReg::Y),
            0x3 => Some(TransferReg::U),
            0x4 => Some(TransferReg::S),
            0x5 => Some(TransferReg::Pc),
            0x8 => Some(TransferReg::A),
            0x9 => Some(TransferReg::B),
            0xA => Some(TransferReg::Cc),
            0xB => Some(TransferReg::Dp),
            _ => None,
        }
    }

    fn transfer_reg_is_8_bit(reg: TransferReg) -> bool {
        matches!(
            reg,
            TransferReg::A | TransferReg::B | TransferReg::Cc | TransferReg::Dp
        )
    }

    fn read_transfer_reg(&self, reg: TransferReg) -> u16 {
        match reg {
            TransferReg::D => self.regs.d(),
            TransferReg::X => self.regs.x,
            TransferReg::Y => self.regs.y,
            TransferReg::U => self.regs.u,
            TransferReg::S => self.regs.s,
            TransferReg::Pc => self.regs.pc,
            TransferReg::A => u16::from(self.regs.a),
            TransferReg::B => u16::from(self.regs.b),
            TransferReg::Cc => u16::from(self.regs.cc),
            TransferReg::Dp => u16::from(self.regs.dp),
        }
    }

    fn write_transfer_reg(&mut self, reg: TransferReg, value: u16) {
        match reg {
            TransferReg::D => {
                let [hi, lo] = value.to_be_bytes();
                self.regs.a = hi;
                self.regs.b = lo;
            }
            TransferReg::X => self.regs.x = value,
            TransferReg::Y => self.regs.y = value,
            TransferReg::U => self.regs.u = value,
            TransferReg::S => self.regs.s = value,
            TransferReg::Pc => self.regs.pc = value,
            TransferReg::A => self.regs.a = value as u8,
            TransferReg::B => self.regs.b = value as u8,
            TransferReg::Cc => self.regs.cc = value as u8,
            TransferReg::Dp => self.regs.dp = value as u8,
        }
    }

    fn invalid_transfer_value(reg: TransferReg) -> u16 {
        if Self::transfer_reg_is_8_bit(reg) {
            0x00FF
        } else {
            0xFFFF
        }
    }

    fn tfr(&mut self, post: u8) {
        let source = Self::transfer_reg(post >> 4);
        let target = Self::transfer_reg(post & 0x0F);
        if let (Some(source), Some(target)) = (source, target) {
            let value =
                if Self::transfer_reg_is_8_bit(source) == Self::transfer_reg_is_8_bit(target) {
                    self.read_transfer_reg(source)
                } else {
                    Self::invalid_transfer_value(target)
                };
            self.write_transfer_reg(target, value);
        }
    }

    fn exg(&mut self, post: u8) {
        let left = Self::transfer_reg(post >> 4);
        let right = Self::transfer_reg(post & 0x0F);
        if let (Some(left), Some(right)) = (left, right) {
            let (left_value, right_value) =
                if Self::transfer_reg_is_8_bit(left) == Self::transfer_reg_is_8_bit(right) {
                    (self.read_transfer_reg(left), self.read_transfer_reg(right))
                } else {
                    (
                        Self::invalid_transfer_value(left),
                        Self::invalid_transfer_value(right),
                    )
                };
            self.write_transfer_reg(right, left_value);
            self.write_transfer_reg(left, right_value);
        }
    }

    fn sex(&mut self) {
        self.regs.a = if self.regs.b & 0x80 != 0 { 0xFF } else { 0x00 };
        self.set_nz16(self.regs.d());
    }

    fn abx(&mut self) {
        self.regs.x = self.regs.x.wrapping_add(u16::from(self.regs.b));
    }

    fn mul(&mut self) {
        let result = u16::from(self.regs.a) * u16::from(self.regs.b);
        let [hi, lo] = result.to_be_bytes();
        self.regs.a = hi;
        self.regs.b = lo;
        self.regs.set_flag(FLAG_Z, result == 0);
        self.regs.set_flag(FLAG_C, result & 0x0080 != 0);
    }

    fn daa(&mut self) {
        let value = self.regs.a;
        let msn = value & 0xF0;
        let lsn = value & 0x0F;
        let mut result = u16::from(value);

        if lsn > 0x09 || self.regs.flag(FLAG_H) {
            result = result.wrapping_add(0x06);
        }
        if msn > 0x90 || self.regs.flag(FLAG_C) || (msn > 0x80 && lsn > 0x09) {
            result = result.wrapping_add(0x60);
        }

        self.regs.a = result as u8;
        self.set_nz8(self.regs.a);
        self.regs
            .set_flag(FLAG_C, self.regs.flag(FLAG_C) || result & 0x100 != 0);
    }

    fn branch_condition(&self, condition: BranchCondition) -> bool {
        let c = self.regs.flag(FLAG_C);
        let n = self.regs.flag(FLAG_N);
        let z = self.regs.flag(FLAG_Z);
        let v = self.regs.flag(FLAG_V);

        match condition {
            BranchCondition::Always => true,
            BranchCondition::Never => false,
            BranchCondition::Hi => !c && !z,
            BranchCondition::Ls => c || z,
            BranchCondition::Cc => !c,
            BranchCondition::Cs => c,
            BranchCondition::Ne => !z,
            BranchCondition::Eq => z,
            BranchCondition::Vc => !v,
            BranchCondition::Vs => v,
            BranchCondition::Pl => !n,
            BranchCondition::Mi => n,
            BranchCondition::Ge => n == v,
            BranchCondition::Lt => n != v,
            BranchCondition::Gt => !z && n == v,
            BranchCondition::Le => z || n != v,
        }
    }

    const fn rmw_post_cycles(op: Rmw8Op) -> u8 {
        if matches!(op, Rmw8Op::Tst) { 3 } else { 2 }
    }

    fn prepare_store(&mut self, op: Store8Op, addr: u16, post_cycles: u8) {
        let value = match op {
            Store8Op::Sta => self.regs.a,
            Store8Op::Stb => self.regs.b,
        };
        self.set_load_flags8(value);
        self.state = if post_cycles == 0 {
            CpuState::WriteValue
        } else {
            CpuState::WriteValueThenInternal {
                cycles: post_cycles,
            }
        };
        self.addr = addr;
        self.data = value;
        self.rw = false;
        self.sync = false;
    }

    fn prepare_rmw(&mut self, op: Rmw8Op, addr: u16, post_cycles: u8) {
        self.state = CpuState::ReadRmwValue {
            op,
            addr,
            post_cycles,
        };
        self.addr = addr;
        self.rw = true;
        self.sync = false;
    }

    fn prepare_read16(&mut self, op: Mem16Op, addr: u16) {
        self.state = CpuState::ReadMem16Hi { op, addr };
        self.addr = addr;
        self.rw = true;
        self.sync = false;
    }

    fn prepare_store16(&mut self, op: Store16Op, addr: u16, post_cycles: u8) {
        let Store16Op::Store(reg) = op;
        let value = self.read_word_reg(reg);
        let [hi, lo] = value.to_be_bytes();
        self.set_load_flags16(value);
        self.state = CpuState::Write16Lo { lo, post_cycles };
        self.addr = addr;
        self.data = hi;
        self.rw = false;
        self.sync = false;
    }

    fn start_stack_op(&mut self, op: StackOp, mask: u8) {
        match op {
            StackOp::PushS => self.start_stack_push(StackPointer::S, mask, AfterStack::Fetch),
            StackOp::PullS => self.start_stack_pull(StackPointer::S, mask),
            StackOp::PushU => self.start_stack_push(StackPointer::U, mask, AfterStack::Fetch),
            StackOp::PullU => self.start_stack_pull(StackPointer::U, mask),
        }
    }

    fn start_stack_push(&mut self, ptr: StackPointer, mask: u8, after: AfterStack) {
        let mut bytes = [0; MAX_STACK_BYTES];
        let mut len = 0;

        if mask & 0x80 != 0 {
            len = Self::append_stack_word(&mut bytes, len, self.regs.pc);
        }
        if mask & 0x40 != 0 {
            let value = match ptr {
                StackPointer::S => self.regs.u,
                StackPointer::U => self.regs.s,
            };
            len = Self::append_stack_word(&mut bytes, len, value);
        }
        if mask & 0x20 != 0 {
            len = Self::append_stack_word(&mut bytes, len, self.regs.y);
        }
        if mask & 0x10 != 0 {
            len = Self::append_stack_word(&mut bytes, len, self.regs.x);
        }
        if mask & 0x08 != 0 {
            len = Self::append_stack_byte(&mut bytes, len, self.regs.dp);
        }
        if mask & 0x04 != 0 {
            len = Self::append_stack_byte(&mut bytes, len, self.regs.b);
        }
        if mask & 0x02 != 0 {
            len = Self::append_stack_byte(&mut bytes, len, self.regs.a);
        }
        if mask & 0x01 != 0 {
            len = Self::append_stack_byte(&mut bytes, len, self.regs.cc);
        }

        self.stack_work = StackWork::Push {
            ptr,
            bytes,
            len: len as u8,
            index: 0,
            after,
        };
        self.schedule_stack_work();
    }

    fn start_stack_pull(&mut self, ptr: StackPointer, mask: u8) {
        let mut targets = [StackTarget::None; MAX_STACK_BYTES];
        let mut len = 0;

        if mask & 0x01 != 0 {
            len = Self::append_stack_target(&mut targets, len, StackTarget::Cc);
        }
        if mask & 0x02 != 0 {
            len = Self::append_stack_target(&mut targets, len, StackTarget::A);
        }
        if mask & 0x04 != 0 {
            len = Self::append_stack_target(&mut targets, len, StackTarget::B);
        }
        if mask & 0x08 != 0 {
            len = Self::append_stack_target(&mut targets, len, StackTarget::Dp);
        }
        if mask & 0x10 != 0 {
            len = Self::append_stack_target_pair(
                &mut targets,
                len,
                StackTarget::XHi,
                StackTarget::XLo,
            );
        }
        if mask & 0x20 != 0 {
            len = Self::append_stack_target_pair(
                &mut targets,
                len,
                StackTarget::YHi,
                StackTarget::YLo,
            );
        }
        if mask & 0x40 != 0 {
            let (hi, lo) = match ptr {
                StackPointer::S => (StackTarget::UHi, StackTarget::ULo),
                StackPointer::U => (StackTarget::SHi, StackTarget::SLo),
            };
            len = Self::append_stack_target_pair(&mut targets, len, hi, lo);
        }
        if mask & 0x80 != 0 {
            len = Self::append_stack_target_pair(
                &mut targets,
                len,
                StackTarget::PcHi,
                StackTarget::PcLo,
            );
        }

        self.stack_work = StackWork::Pull {
            ptr,
            targets,
            len: len as u8,
            index: 0,
            hi: 0,
        };
        self.schedule_stack_work();
    }

    fn append_stack_word(bytes: &mut [u8; MAX_STACK_BYTES], len: usize, value: u16) -> usize {
        let [hi, lo] = value.to_be_bytes();
        let next = Self::append_stack_byte(bytes, len, lo);
        Self::append_stack_byte(bytes, next, hi)
    }

    fn append_stack_byte(bytes: &mut [u8; MAX_STACK_BYTES], len: usize, value: u8) -> usize {
        bytes[len] = value;
        len + 1
    }

    fn append_stack_target(
        targets: &mut [StackTarget; MAX_STACK_BYTES],
        len: usize,
        target: StackTarget,
    ) -> usize {
        targets[len] = target;
        len + 1
    }

    fn append_stack_target_pair(
        targets: &mut [StackTarget; MAX_STACK_BYTES],
        len: usize,
        hi: StackTarget,
        lo: StackTarget,
    ) -> usize {
        let next = Self::append_stack_target(targets, len, hi);
        Self::append_stack_target(targets, next, lo)
    }

    fn schedule_stack_work(&mut self) {
        match self.stack_work {
            StackWork::None => self.next_fetch(),
            StackWork::Push {
                ptr,
                bytes,
                len,
                index,
                after,
            } => {
                if index >= len {
                    self.after_stack(after);
                } else {
                    let addr = self.stack_pointer(ptr).wrapping_sub(1);
                    self.set_stack_pointer(ptr, addr);
                    self.addr = addr;
                    self.data = bytes[index as usize];
                    self.rw = false;
                    self.sync = false;
                    self.state = CpuState::StackWrite;
                }
            }
            StackWork::Pull {
                ptr, len, index, ..
            } => {
                if index >= len {
                    self.next_fetch();
                } else {
                    self.addr = self.stack_pointer(ptr);
                    self.rw = true;
                    self.sync = false;
                    self.state = CpuState::StackRead;
                }
            }
        }
    }

    fn finish_stack_write(&mut self) {
        if let StackWork::Push {
            ptr,
            bytes,
            len,
            index,
            after,
        } = self.stack_work
        {
            self.stack_work = StackWork::Push {
                ptr,
                bytes,
                len,
                index: index + 1,
                after,
            };
        }
        self.schedule_stack_work();
    }

    fn finish_stack_read(&mut self) {
        if let StackWork::Pull {
            ptr,
            targets,
            len,
            index,
            hi,
        } = self.stack_work
        {
            let next_hi = self.apply_stack_target(targets[index as usize], hi, self.data_in);
            let next_ptr = self.stack_pointer(ptr).wrapping_add(1);
            self.set_stack_pointer(ptr, next_ptr);
            self.stack_work = StackWork::Pull {
                ptr,
                targets,
                len,
                index: index + 1,
                hi: next_hi,
            };
        }
        self.schedule_stack_work();
    }

    fn stack_pointer(&self, ptr: StackPointer) -> u16 {
        match ptr {
            StackPointer::S => self.regs.s,
            StackPointer::U => self.regs.u,
        }
    }

    fn set_stack_pointer(&mut self, ptr: StackPointer, value: u16) {
        match ptr {
            StackPointer::S => self.regs.s = value,
            StackPointer::U => self.regs.u = value,
        }
    }

    fn apply_stack_target(&mut self, target: StackTarget, hi: u8, value: u8) -> u8 {
        match target {
            StackTarget::None => hi,
            StackTarget::Cc => {
                self.regs.cc = value;
                hi
            }
            StackTarget::A => {
                self.regs.a = value;
                hi
            }
            StackTarget::B => {
                self.regs.b = value;
                hi
            }
            StackTarget::Dp => {
                self.regs.dp = value;
                hi
            }
            StackTarget::XHi
            | StackTarget::YHi
            | StackTarget::UHi
            | StackTarget::SHi
            | StackTarget::PcHi => value,
            StackTarget::XLo => {
                self.regs.x = u16::from_be_bytes([hi, value]);
                hi
            }
            StackTarget::YLo => {
                self.regs.y = u16::from_be_bytes([hi, value]);
                hi
            }
            StackTarget::ULo => {
                self.regs.u = u16::from_be_bytes([hi, value]);
                hi
            }
            StackTarget::SLo => {
                self.regs.s = u16::from_be_bytes([hi, value]);
                hi
            }
            StackTarget::PcLo => {
                self.regs.pc = u16::from_be_bytes([hi, value]);
                hi
            }
        }
    }

    fn prepare_push_word(&mut self, value: u16, after: AfterPush) {
        let [hi, lo] = value.to_be_bytes();
        self.regs.s = self.regs.s.wrapping_sub(1);
        self.state = CpuState::PushWordHi { hi, after };
        self.addr = self.regs.s;
        self.data = lo;
        self.rw = false;
        self.sync = false;
    }

    fn prepare_pull_word(&mut self, op: Pull16Op) {
        self.state = CpuState::PullWordHi(op);
        self.addr = self.regs.s;
        self.rw = true;
        self.sync = false;
    }

    fn after_push(&mut self, after: AfterPush) {
        match after {
            AfterPush::SetPc(pc) => self.regs.pc = pc,
            AfterPush::SetPcAfterInternal { pc, cycles } => {
                self.regs.pc = pc;
                self.start_internal_cycles(cycles);
                return;
            }
        }
        self.next_fetch();
    }

    fn alu_a(&mut self, op: Alu8Op, rhs: u8) {
        if let Some(value) = self.alu8(op, self.regs.a, rhs) {
            self.regs.a = value;
        }
    }

    fn alu_b(&mut self, op: Alu8Op, rhs: u8) {
        if let Some(value) = self.alu8(op, self.regs.b, rhs) {
            self.regs.b = value;
        }
    }

    fn alu_d(&mut self, op: Alu16Op, rhs: u16) {
        let result = match op {
            Alu16Op::Add => self.add16(self.regs.d(), rhs),
            Alu16Op::Sub => self.sub16(self.regs.d(), rhs),
        };
        let [hi, lo] = result.to_be_bytes();
        self.regs.a = hi;
        self.regs.b = lo;
    }

    fn rmw8(&mut self, op: Rmw8Op, value: u8) -> Option<u8> {
        match op {
            Rmw8Op::Neg => {
                let result = 0u8.wrapping_sub(value);
                self.set_nz8(result);
                self.regs.set_flag(FLAG_V, value == 0x80);
                self.regs.set_flag(FLAG_C, result != 0);
                Some(result)
            }
            Rmw8Op::Com => {
                let result = !value;
                self.set_nz8(result);
                self.regs.set_flag(FLAG_V, false);
                self.regs.set_flag(FLAG_C, true);
                Some(result)
            }
            Rmw8Op::Lsr => {
                let result = value >> 1;
                self.regs.set_flag(FLAG_N, false);
                self.regs.set_flag(FLAG_Z, result == 0);
                self.regs.set_flag(FLAG_C, value & 0x01 != 0);
                Some(result)
            }
            Rmw8Op::Ror => {
                let carry_in = if self.regs.flag(FLAG_C) { 0x80 } else { 0x00 };
                let result = (value >> 1) | carry_in;
                self.set_nz8(result);
                self.regs.set_flag(FLAG_C, value & 0x01 != 0);
                Some(result)
            }
            Rmw8Op::Asr => {
                let result = (value >> 1) | (value & 0x80);
                self.set_nz8(result);
                self.regs.set_flag(FLAG_C, value & 0x01 != 0);
                Some(result)
            }
            Rmw8Op::Asl => {
                let result = value << 1;
                self.set_nz8(result);
                self.regs.set_flag(FLAG_V, (value ^ result) & 0x80 != 0);
                self.regs.set_flag(FLAG_C, value & 0x80 != 0);
                Some(result)
            }
            Rmw8Op::Rol => {
                let carry_in = u8::from(self.regs.flag(FLAG_C));
                let result = (value << 1) | carry_in;
                self.set_nz8(result);
                self.regs.set_flag(FLAG_V, (value ^ result) & 0x80 != 0);
                self.regs.set_flag(FLAG_C, value & 0x80 != 0);
                Some(result)
            }
            Rmw8Op::Dec => {
                let result = value.wrapping_sub(1);
                self.set_nz8(result);
                self.regs.set_flag(FLAG_V, value == 0x80);
                Some(result)
            }
            Rmw8Op::Inc => {
                let result = value.wrapping_add(1);
                self.set_nz8(result);
                self.regs.set_flag(FLAG_V, value == 0x7F);
                Some(result)
            }
            Rmw8Op::Tst => {
                self.set_nz8(value);
                self.regs.set_flag(FLAG_V, false);
                None
            }
            Rmw8Op::Clr => {
                self.regs.set_flag(FLAG_N, false);
                self.regs.set_flag(FLAG_Z, true);
                self.regs.set_flag(FLAG_V, false);
                self.regs.set_flag(FLAG_C, false);
                Some(0)
            }
        }
    }

    fn alu8(&mut self, op: Alu8Op, lhs: u8, rhs: u8) -> Option<u8> {
        match op {
            Alu8Op::Add => Some(self.add8(lhs, rhs, 0)),
            Alu8Op::AddCarry => {
                let carry = u8::from(self.regs.flag(FLAG_C));
                Some(self.add8(lhs, rhs, carry))
            }
            Alu8Op::Sub => Some(self.sub8(lhs, rhs, 0)),
            Alu8Op::SubCarry => {
                let carry = u8::from(self.regs.flag(FLAG_C));
                Some(self.sub8(lhs, rhs, carry))
            }
            Alu8Op::And => Some(self.logical8(lhs & rhs)),
            Alu8Op::Bit => {
                self.logical8(lhs & rhs);
                None
            }
            Alu8Op::Eor => Some(self.logical8(lhs ^ rhs)),
            Alu8Op::Or => Some(self.logical8(lhs | rhs)),
        }
    }

    fn add8(&mut self, lhs: u8, rhs: u8, carry: u8) -> u8 {
        let wide = u16::from(lhs) + u16::from(rhs) + u16::from(carry);
        let result = wide as u8;
        self.set_nz8(result);
        self.regs
            .set_flag(FLAG_H, ((lhs & 0x0F) + (rhs & 0x0F) + carry) & 0x10 != 0);
        self.regs
            .set_flag(FLAG_V, (!(lhs ^ rhs) & (lhs ^ result) & 0x80) != 0);
        self.regs.set_flag(FLAG_C, wide > 0xFF);
        result
    }

    fn sub8(&mut self, lhs: u8, rhs: u8, carry: u8) -> u8 {
        let subtrahend = u16::from(rhs) + u16::from(carry);
        let result = lhs.wrapping_sub(rhs).wrapping_sub(carry);
        self.set_nz8(result);
        self.regs
            .set_flag(FLAG_V, ((lhs ^ rhs) & (lhs ^ result) & 0x80) != 0);
        self.regs.set_flag(FLAG_C, u16::from(lhs) < subtrahend);
        result
    }

    fn add16(&mut self, lhs: u16, rhs: u16) -> u16 {
        let wide = u32::from(lhs) + u32::from(rhs);
        let result = wide as u16;
        self.set_nz16(result);
        self.regs
            .set_flag(FLAG_V, (!(lhs ^ rhs) & (lhs ^ result) & 0x8000) != 0);
        self.regs.set_flag(FLAG_C, wide > 0xFFFF);
        result
    }

    fn sub16(&mut self, lhs: u16, rhs: u16) -> u16 {
        let result = lhs.wrapping_sub(rhs);
        self.set_nz16(result);
        self.regs
            .set_flag(FLAG_V, ((lhs ^ rhs) & (lhs ^ result) & 0x8000) != 0);
        self.regs.set_flag(FLAG_C, lhs < rhs);
        result
    }

    fn logical8(&mut self, value: u8) -> u8 {
        self.set_nz8(value);
        self.regs.set_flag(FLAG_V, false);
        value
    }

    fn set_load_flags8(&mut self, value: u8) {
        self.set_nz8(value);
        self.regs.set_flag(FLAG_V, false);
    }

    fn set_load_flags16(&mut self, value: u16) {
        self.set_nz16(value);
        self.regs.set_flag(FLAG_V, false);
    }

    fn compare8(&mut self, lhs: u8, rhs: u8) {
        let result = lhs.wrapping_sub(rhs);
        self.set_nz8(result);
        self.regs
            .set_flag(FLAG_V, ((lhs ^ rhs) & (lhs ^ result) & 0x80) != 0);
        self.regs.set_flag(FLAG_C, lhs < rhs);
    }

    fn compare16(&mut self, lhs: u16, rhs: u16) {
        let result = lhs.wrapping_sub(rhs);
        self.set_nz16(result);
        self.regs
            .set_flag(FLAG_V, ((lhs ^ rhs) & (lhs ^ result) & 0x8000) != 0);
        self.regs.set_flag(FLAG_C, lhs < rhs);
    }

    fn set_nz8(&mut self, value: u8) {
        self.regs.set_flag(FLAG_N, value & 0x80 != 0);
        self.regs.set_flag(FLAG_Z, value == 0);
    }

    fn set_nz16(&mut self, value: u16) {
        self.regs.set_flag(FLAG_N, value & 0x8000 != 0);
        self.regs.set_flag(FLAG_Z, value == 0);
    }
}

impl Default for Mc6809 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use super::*;
    use crate::registers::{FLAG_C, FLAG_F, FLAG_I};

    fn run_cycle(cpu: &mut Mc6809, memory: &mut [u8; 0x10000]) {
        if cpu.rw {
            cpu.data_in = memory[cpu.addr as usize];
        } else {
            memory[cpu.addr as usize] = cpu.data;
        }
        cpu.tick();
    }

    fn run_cycles(cpu: &mut Mc6809, memory: &mut [u8; 0x10000], count: usize) {
        for _ in 0..count {
            run_cycle(cpu, memory);
        }
    }

    fn run_until_boundary(cpu: &mut Mc6809, memory: &mut [u8; 0x10000]) {
        run_cycle(cpu, memory);
        for _ in 0..64 {
            if cpu.instruction_boundary() {
                return;
            }
            run_cycle(cpu, memory);
        }
        panic!("instruction did not reach boundary");
    }

    fn run_instruction_cycles(cpu: &mut Mc6809, memory: &mut [u8; 0x10000]) -> u64 {
        let start = cpu.total_cycles;
        run_until_boundary(cpu, memory);
        cpu.total_cycles - start
    }

    fn run_until_boundary_or_wait_or_halt(cpu: &mut Mc6809, memory: &mut [u8; 0x10000]) {
        run_cycle(cpu, memory);
        for _ in 0..128 {
            if cpu.instruction_boundary()
                || cpu.halt
                || matches!(cpu.state, CpuState::WaitForInterrupt { .. })
            {
                return;
            }
            run_cycle(cpu, memory);
        }
        panic!("instruction did not reach boundary, wait, or halt");
    }

    fn cpu_at(pc: u16) -> Mc6809 {
        let mut cpu = Mc6809::new();
        cpu.regs.pc = pc;
        cpu.addr = pc;
        cpu.sync = true;
        cpu
    }

    fn assert_opcode_dispatches(bytes: &[u8], label: fmt::Arguments<'_>) {
        let mut memory = [0x12; 0x10000];
        memory[0x4000..0x4000 + bytes.len()].copy_from_slice(bytes);
        memory[0x4000 + bytes.len()] = 0x84; // Safe indexed postbyte: ,X.
        memory[0x4001 + bytes.len()] = 0x50; // Safe high byte for 16-bit operands.
        memory[0x4002 + bytes.len()] = 0x00; // Safe low byte for 16-bit operands.
        memory[0xFFFA] = 0x50;
        memory[0xFFFB] = 0x00;
        memory[0xFFFC] = 0x50;
        memory[0xFFFD] = 0x00;
        memory[0x5000] = 0x12;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x50;
        cpu.regs.x = 0x5000;
        cpu.regs.y = 0x5000;
        cpu.regs.u = 0x9000;
        cpu.regs.s = 0x8000;

        run_until_boundary_or_wait_or_halt(&mut cpu, &mut memory);

        assert!(!cpu.halt, "{label} tripped illegal-opcode diagnostic");
    }

    #[test]
    fn reset_fetches_vector_big_endian() {
        let mut cpu = Mc6809::new();
        cpu.reset();
        assert_eq!(cpu.addr, 0xFFFE);
        assert!(cpu.rw);
        assert_eq!(cpu.reset_phase, 2);

        cpu.data_in = 0xC0;
        cpu.tick();
        assert_eq!(cpu.addr, 0xFFFF);
        assert_eq!(cpu.reset_phase, 1);
        assert!(!cpu.sync);

        cpu.data_in = 0x00;
        cpu.tick();
        assert_eq!(cpu.regs.pc, 0xC000);
        assert_eq!(cpu.addr, 0xC000);
        assert_eq!(cpu.reset_phase, 0);
        assert!(cpu.sync);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn reset_sets_interrupt_masks_without_clearing_other_cc_bits() {
        let mut cpu = Mc6809::new();
        cpu.regs.set_flag(FLAG_C, true);
        cpu.regs.set_flag(FLAG_I, false);
        cpu.irq = true;
        cpu.firq = true;
        cpu.nmi = true;

        cpu.reset();

        assert!(cpu.regs.flag(FLAG_C));
        assert!(cpu.regs.irq_masked());
        assert!(cpu.regs.firq_masked());
        assert!(!cpu.irq);
        assert!(!cpu.firq);
        assert!(!cpu.nmi);
    }

    #[test]
    fn idle_tick_presents_pc_for_fetch() {
        let mut cpu = Mc6809::new();
        cpu.regs.pc = 0x1234;
        cpu.sync = true;
        cpu.data_in = 0x01;
        cpu.tick();

        assert!(cpu.halt);
        assert_eq!(cpu.addr, 0x1235);
        assert!(cpu.rw);
        assert!(!cpu.sync);
        assert_eq!(cpu.total_cycles, 1);
    }

    #[test]
    fn nop_fetches_and_returns_to_instruction_boundary() {
        let mut cpu = cpu_at(0x4000);
        cpu.data_in = 0x12;

        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x4001);
        assert_eq!(cpu.addr, 0x4001);
        assert!(!cpu.sync);
        assert!(!cpu.halt);

        cpu.tick();
        assert_eq!(cpu.regs.pc, 0x4001);
        assert_eq!(cpu.addr, 0x4001);
        assert!(cpu.instruction_boundary());
        assert_eq!(cpu.total_cycles, 2);
    }

    #[test]
    fn unknown_opcode_halts_for_bringup_visibility() {
        let mut cpu = cpu_at(0x4000);
        cpu.data_in = 0x01;

        cpu.tick();

        assert!(cpu.halt);
        assert_eq!(cpu.regs.pc, 0x4001);
        assert!(!cpu.instruction_boundary());
    }

    #[test]
    fn official_opcode_map_does_not_trip_illegal_opcode_diagnostic() {
        const PRIMARY: &[u8] = &[
            0x00, 0x03, 0x04, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0C, 0x0D, 0x0E, 0x0F, 0x12, 0x13,
            0x16, 0x17, 0x19, 0x1A, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25,
            0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33,
            0x34, 0x35, 0x36, 0x37, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3F, 0x40, 0x43, 0x44, 0x46,
            0x47, 0x48, 0x49, 0x4A, 0x4C, 0x4D, 0x4F, 0x50, 0x53, 0x54, 0x56, 0x57, 0x58, 0x59,
            0x5A, 0x5C, 0x5D, 0x5F, 0x60, 0x63, 0x64, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6C, 0x6D,
            0x6E, 0x6F, 0x70, 0x73, 0x74, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7C, 0x7D, 0x7E, 0x7F,
            0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E,
            0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0x9B, 0x9C, 0x9D,
            0x9E, 0x9F, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB,
            0xAC, 0xAD, 0xAE, 0xAF, 0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9,
            0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC8,
            0xC9, 0xCA, 0xCB, 0xCC, 0xCE, 0xD0, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8,
            0xD9, 0xDA, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF, 0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6,
            0xE7, 0xE8, 0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xEE, 0xEF, 0xF0, 0xF1, 0xF2, 0xF3, 0xF4,
            0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, 0xFF,
        ];
        const PREFIX_10: &[u8] = &[
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E,
            0x2F, 0x3F, 0x83, 0x8C, 0x8E, 0x93, 0x9C, 0x9E, 0x9F, 0xA3, 0xAC, 0xAE, 0xAF, 0xB3,
            0xBC, 0xBE, 0xBF, 0xCE, 0xDE, 0xDF, 0xEE, 0xEF, 0xFE, 0xFF,
        ];
        const PREFIX_11: &[u8] = &[0x3F, 0x83, 0x8C, 0x93, 0x9C, 0xA3, 0xAC, 0xB3, 0xBC];

        for &opcode in PRIMARY {
            assert_opcode_dispatches(&[opcode], format_args!("primary ${opcode:02X}"));
        }
        for &opcode in PREFIX_10 {
            assert_opcode_dispatches(&[0x10, opcode], format_args!("10 ${opcode:02X}"));
        }
        for &opcode in PREFIX_11 {
            assert_opcode_dispatches(&[0x11, opcode], format_args!("11 ${opcode:02X}"));
        }
    }

    #[test]
    fn lda_and_ldb_immediate_update_nzv_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x86; // LDA #$80
        memory[0x4001] = 0x80;
        memory[0x4002] = 0xC6; // LDB #$00
        memory[0x4003] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.set_flag(FLAG_V, true);

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x80);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0x00);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
    }

    #[test]
    fn sixteen_bit_immediate_loads_x_u_and_s() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x8E; // LDX #$1234
        memory[0x4001] = 0x12;
        memory[0x4002] = 0x34;
        memory[0x4003] = 0xCE; // LDU #$8000
        memory[0x4004] = 0x80;
        memory[0x4005] = 0x00;
        memory[0x4006] = 0x10; // LDS #$0000
        memory[0x4007] = 0xCE;
        memory[0x4008] = 0x00;
        memory[0x4009] = 0x00;
        let mut cpu = cpu_at(0x4000);

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.x, 0x1234);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.u, 0x8000);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));

        run_cycles(&mut cpu, &mut memory, 4);
        assert_eq!(cpu.regs.s, 0x0000);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_Z));
    }

    #[test]
    fn base_page_sixteen_bit_memory_ops_cover_x_and_u() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x9E; // LDX <$30
        memory[0x4001] = 0x30;
        memory[0x1230] = 0xAB;
        memory[0x1231] = 0xCD;
        memory[0x4002] = 0xBF; // STX $2000
        memory[0x4003] = 0x20;
        memory[0x4004] = 0x00;
        memory[0x4005] = 0xEE; // LDU ,X
        memory[0x4006] = 0x84;
        memory[0xABCD] = 0x80;
        memory[0xABCE] = 0x01;
        memory[0x4007] = 0xDF; // STU <$40
        memory[0x4008] = 0x40;
        memory[0x4009] = 0xBC; // CMPX $2000
        memory[0x400A] = 0x20;
        memory[0x400B] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(cpu.regs.x, 0xABCD);
        assert!(cpu.regs.flag(FLAG_N));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(&memory[0x2000..=0x2001], &[0xAB, 0xCD]);
        assert!(cpu.instruction_boundary());

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(cpu.regs.u, 0x8001);

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(&memory[0x1240..=0x1241], &[0x80, 0x01]);

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));
        assert_eq!(cpu.regs.x, 0xABCD);
    }

    #[test]
    fn page_two_sixteen_bit_ops_cover_y_s_and_d_compare() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x10; // LDY #$8000
        memory[0x4001] = 0x8E;
        memory[0x4002] = 0x80;
        memory[0x4003] = 0x00;
        memory[0x4004] = 0x10; // STY <$20
        memory[0x4005] = 0x9F;
        memory[0x4006] = 0x20;
        memory[0x4007] = 0x10; // LDS $3000
        memory[0x4008] = 0xFE;
        memory[0x4009] = 0x30;
        memory[0x400A] = 0x00;
        memory[0x3000] = 0x00;
        memory[0x3001] = 0x00;
        memory[0x400B] = 0x10; // CMPD ,Y
        memory[0x400C] = 0xA3;
        memory[0x400D] = 0xA4;
        memory[0x8000] = 0x12;
        memory[0x8001] = 0x35;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.a = 0x12;
        cpu.regs.b = 0x34;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 4);
        assert_eq!(cpu.regs.y, 0x8000);
        assert!(cpu.regs.flag(FLAG_N));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(&memory[0x1220..=0x1221], &[0x80, 0x00]);

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 7);
        assert_eq!(cpu.regs.s, 0x0000);
        assert!(cpu.regs.flag(FLAG_Z));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_C));
        assert_eq!(cpu.regs.d(), 0x1234);
    }

    #[test]
    fn page_three_compares_u_and_s_across_addressing_modes() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x11; // CMPU #$2000
        memory[0x4001] = 0x83;
        memory[0x4002] = 0x20;
        memory[0x4003] = 0x00;
        memory[0x4004] = 0x11; // CMPS <$40
        memory[0x4005] = 0x9C;
        memory[0x4006] = 0x40;
        memory[0x1240] = 0x80;
        memory[0x1241] = 0x00;
        memory[0x4007] = 0x11; // CMPS ,X
        memory[0x4008] = 0xAC;
        memory[0x4009] = 0x84;
        memory[0x2200] = 0x7F;
        memory[0x2201] = 0xFF;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.u = 0x1000;
        cpu.regs.s = 0x8000;
        cpu.regs.x = 0x2200;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 4);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_C));
        assert_eq!(cpu.regs.u, 0x1000);

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn ldd_and_std_cover_immediate_direct_indexed_and_extended() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0xCC; // LDD #$1234
        memory[0x4001] = 0x12;
        memory[0x4002] = 0x34;
        memory[0x4003] = 0xDD; // STD <$20
        memory[0x4004] = 0x20;
        memory[0x4005] = 0xEC; // LDD ,X
        memory[0x4006] = 0x84;
        memory[0x2200] = 0x80;
        memory[0x2201] = 0x01;
        memory[0x4007] = 0xFD; // STD $3000
        memory[0x4008] = 0x30;
        memory[0x4009] = 0x00;
        memory[0x400A] = 0xFC; // LDD $3000
        memory[0x400B] = 0x30;
        memory[0x400C] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.x = 0x2200;

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.d(), 0x1234);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(&memory[0x1220..=0x1221], &[0x12, 0x34]);

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(cpu.regs.d(), 0x8001);
        assert!(cpu.regs.flag(FLAG_N));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(&memory[0x3000..=0x3001], &[0x80, 0x01]);

        cpu.regs.a = 0;
        cpu.regs.b = 0;
        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(cpu.regs.d(), 0x8001);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn addd_and_subd_update_sixteen_bit_arithmetic_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0xC3; // ADDD #$0001
        memory[0x4001] = 0x00;
        memory[0x4002] = 0x01;
        memory[0x4003] = 0xD3; // ADDD <$20
        memory[0x4004] = 0x20;
        memory[0x1220] = 0x7F;
        memory[0x1221] = 0xFF;
        memory[0x4005] = 0x83; // SUBD #$0002
        memory[0x4006] = 0x00;
        memory[0x4007] = 0x02;
        memory[0x4008] = 0xB3; // SUBD $3000
        memory[0x4009] = 0x30;
        memory[0x400A] = 0x00;
        memory[0x3000] = 0x7F;
        memory[0x3001] = 0xFF;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.a = 0x7F;
        cpu.regs.b = 0xFF;

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.d(), 0x8000);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_V));
        assert!(!cpu.regs.flag(FLAG_C));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(cpu.regs.d(), 0xFFFF);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_V));
        assert!(!cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.d(), 0xFFFD);
        assert!(!cpu.regs.flag(FLAG_C));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(cpu.regs.d(), 0x7FFE);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_V));
        assert!(!cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn addd_indexed_and_subd_indexed_share_sixteen_bit_memory_path() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0xE3; // ADDD ,X
        memory[0x4001] = 0x84;
        memory[0x2200] = 0x00;
        memory[0x2201] = 0x01;
        memory[0x4002] = 0xA3; // SUBD ,X+
        memory[0x4003] = 0x80;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.x = 0x2200;
        cpu.regs.a = 0xFF;
        cpu.regs.b = 0xFF;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(cpu.regs.d(), 0x0000);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_C));

        cpu.regs.a = 0x00;
        cpu.regs.b = 0x02;
        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 7);
        assert_eq!(cpu.regs.d(), 0x0001);
        assert_eq!(cpu.regs.x, 0x2201);
        assert!(!cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn direct_loads_use_direct_page_register() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x96; // LDA <$34
        memory[0x4001] = 0x34;
        memory[0x1234] = 0x7F;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 4);

        assert_eq!(cpu.regs.a, 0x7F);
        assert_eq!(cpu.regs.pc, 0x4002);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn extended_load_store_and_direct_store_use_bus_cycles() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0xB6; // LDA $2345
        memory[0x4001] = 0x23;
        memory[0x4002] = 0x45;
        memory[0x2345] = 0xA5;
        memory[0x4003] = 0xD7; // STB <$20
        memory[0x4004] = 0x20;
        memory[0x4005] = 0xF7; // STB $3456
        memory[0x4006] = 0x34;
        memory[0x4007] = 0x56;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.b = 0x5A;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(cpu.regs.a, 0xA5);
        assert!(cpu.regs.flag(FLAG_N));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 4);
        assert_eq!(memory[0x1220], 0x5A);
        assert!(cpu.instruction_boundary());

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(memory[0x3456], 0x5A);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn bra_and_jmp_change_pc() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x20; // BRA +2
        memory[0x4001] = 0x02;
        memory[0x4004] = 0x7E; // JMP $4567
        memory[0x4005] = 0x45;
        memory[0x4006] = 0x67;
        let mut cpu = cpu_at(0x4000);

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.pc, 0x4004);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.pc, 0x4567);
        assert_eq!(cpu.addr, 0x4567);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn direct_jmp_and_long_branches_change_pc() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x0E; // JMP <$34
        memory[0x4001] = 0x34;
        memory[0x1234] = 0x16; // LBRA +$0010
        memory[0x1235] = 0x00;
        memory[0x1236] = 0x10;
        memory[0x1247] = 0x10; // LBEQ -$0008
        memory[0x1248] = 0x27;
        memory[0x1249] = 0xFF;
        memory[0x124A] = 0xF8;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.pc, 0x1234);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 5);
        assert_eq!(cpu.regs.pc, 0x1247);

        cpu.regs.set_flag(FLAG_Z, true);
        run_cycles(&mut cpu, &mut memory, 6);
        assert_eq!(cpu.regs.pc, 0x1243);
    }

    #[test]
    fn lbsr_pushes_return_pc_and_branches_relative() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x17; // LBSR +$0010
        memory[0x4001] = 0x00;
        memory[0x4002] = 0x10;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;

        run_cycles(&mut cpu, &mut memory, 9);

        assert_eq!(cpu.regs.pc, 0x4013);
        assert_eq!(cpu.regs.s, 0x7FFE);
        assert_eq!(memory[0x7FFE], 0x40);
        assert_eq!(memory[0x7FFF], 0x03);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn short_conditional_branches_follow_condition_codes() {
        fn branch_target(opcode: u8, cc: u8) -> u16 {
            let mut memory = [0; 0x10000];
            memory[0x4000] = opcode;
            memory[0x4001] = 0x05;
            let mut cpu = cpu_at(0x4000);
            cpu.regs.cc = cc;

            run_cycles(&mut cpu, &mut memory, 3);

            cpu.regs.pc
        }

        assert_eq!(branch_target(0x20, 0), 0x4007); // BRA
        assert_eq!(branch_target(0x21, 0), 0x4002); // BRN
        assert_eq!(branch_target(0x22, 0), 0x4007); // BHI
        assert_eq!(branch_target(0x22, FLAG_Z), 0x4002);
        assert_eq!(branch_target(0x23, FLAG_C), 0x4007); // BLS
        assert_eq!(branch_target(0x24, 0), 0x4007); // BCC
        assert_eq!(branch_target(0x25, FLAG_C), 0x4007); // BCS
        assert_eq!(branch_target(0x26, 0), 0x4007); // BNE
        assert_eq!(branch_target(0x27, FLAG_Z), 0x4007); // BEQ
        assert_eq!(branch_target(0x28, 0), 0x4007); // BVC
        assert_eq!(branch_target(0x29, FLAG_V), 0x4007); // BVS
        assert_eq!(branch_target(0x2A, 0), 0x4007); // BPL
        assert_eq!(branch_target(0x2B, FLAG_N), 0x4007); // BMI
        assert_eq!(branch_target(0x2C, FLAG_N | FLAG_V), 0x4007); // BGE
        assert_eq!(branch_target(0x2D, FLAG_N), 0x4007); // BLT
        assert_eq!(branch_target(0x2E, 0), 0x4007); // BGT
        assert_eq!(branch_target(0x2E, FLAG_Z), 0x4002);
        assert_eq!(branch_target(0x2F, FLAG_Z), 0x4007); // BLE
    }

    #[test]
    fn clear_a_and_b_set_expected_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x4F; // CLRA
        memory[0x4001] = 0x5F; // CLRB
        let mut cpu = cpu_at(0x4000);
        cpu.regs.a = 0xFF;
        cpu.regs.b = 0x80;
        cpu.regs.set_flag(FLAG_C, true);
        cpu.regs.set_flag(FLAG_V, true);

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
        assert!(!cpu.regs.flag(FLAG_C));

        cpu.regs.set_flag(FLAG_C, true);
        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn accumulator_rmw_ops_update_values_and_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x40; // NEGA
        memory[0x4001] = 0x43; // COMA
        memory[0x4002] = 0x44; // LSRA
        memory[0x4003] = 0x56; // RORB
        memory[0x4004] = 0x48; // ASLA
        memory[0x4005] = 0x4D; // TSTA
        let mut cpu = cpu_at(0x4000);
        cpu.regs.a = 0x80;
        cpu.regs.b = 0x02;

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x80);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_V));
        assert!(cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x7F);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_V));
        assert!(cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x3F);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0x81);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x7E);
        assert!(!cpu.regs.flag(FLAG_C));
        assert!(!cpu.regs.flag(FLAG_V));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x7E);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
    }

    #[test]
    fn increment_decrement_and_rotate_accumulators_cover_edge_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x4C; // INCA
        memory[0x4001] = 0x5A; // DECB
        memory[0x4002] = 0x49; // ROLA
        memory[0x4003] = 0x57; // ASRB
        let mut cpu = cpu_at(0x4000);
        cpu.regs.a = 0x7F;
        cpu.regs.b = 0x80;

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x80);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_V));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0x7F);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_V));

        cpu.regs.set_flag(FLAG_C, true);
        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x01);
        assert!(cpu.regs.flag(FLAG_C));
        assert!(cpu.regs.flag(FLAG_V));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0x3F);
        assert!(cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn condition_code_immediates_mask_and_set_cc() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x1A; // ORCC #$01
        memory[0x4001] = FLAG_C;
        memory[0x4002] = 0x1C; // ANDCC #!$10
        memory[0x4003] = !FLAG_I;
        let mut cpu = cpu_at(0x4000);

        run_cycles(&mut cpu, &mut memory, 2);
        assert!(cpu.regs.flag(FLAG_C));
        assert!(cpu.regs.flag(FLAG_I));
        assert!(cpu.regs.flag(FLAG_F));

        run_cycles(&mut cpu, &mut memory, 2);
        assert!(cpu.regs.flag(FLAG_C));
        assert!(!cpu.regs.flag(FLAG_I));
        assert!(cpu.regs.flag(FLAG_F));
    }

    #[test]
    fn transfer_and_exchange_register_postbytes_follow_6809_encoding() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x1F; // TFR X,Y
        memory[0x4001] = 0x12;
        memory[0x4002] = 0x1F; // TFR A,DP
        memory[0x4003] = 0x8B;
        memory[0x4004] = 0x1E; // EXG D,X
        memory[0x4005] = 0x01;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.x = 0x1234;
        cpu.regs.a = 0x56;
        cpu.regs.b = 0x78;

        run_cycles(&mut cpu, &mut memory, 6);
        assert_eq!(cpu.regs.y, 0x1234);
        assert_eq!(cpu.regs.x, 0x1234);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 6);
        assert_eq!(cpu.regs.dp, 0x56);

        run_cycles(&mut cpu, &mut memory, 8);
        assert_eq!(cpu.regs.d(), 0x1234);
        assert_eq!(cpu.regs.x, 0x5678);
    }

    #[test]
    fn sex_abx_and_mul_apply_documented_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x1D; // SEX
        memory[0x4001] = 0x3A; // ABX
        memory[0x4002] = 0x3D; // MUL
        let mut cpu = cpu_at(0x4000);
        cpu.regs.b = 0xF0;
        cpu.regs.x = 0x1000;
        cpu.regs.set_flag(FLAG_C, true);

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.d(), 0xFFF0);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_C), "SEX does not affect C");

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.x, 0x10F0);
        assert!(cpu.regs.flag(FLAG_C), "ABX does not affect flags");

        cpu.regs.a = 0x10;
        cpu.regs.b = 0x08;
        cpu.regs.set_flag(FLAG_N, true);
        run_cycles(&mut cpu, &mut memory, 11);
        assert_eq!(cpu.regs.d(), 0x0080);
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_C));
        assert!(cpu.regs.flag(FLAG_N), "MUL only updates Z and C");
    }

    #[test]
    fn daa_adjusts_accumulator_after_bcd_add() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x19; // DAA
        memory[0x4001] = 0x19; // DAA
        let mut cpu = cpu_at(0x4000);
        cpu.regs.a = 0x3C;
        cpu.regs.set_flag(FLAG_H, true);

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x42);
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));

        cpu.regs.a = 0x9A;
        cpu.regs.set_flag(FLAG_H, false);
        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x00);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn bsr_pushes_return_pc_and_branches_relative() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x8D; // BSR +2
        memory[0x4001] = 0x02;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;

        run_cycles(&mut cpu, &mut memory, 7);

        assert_eq!(cpu.regs.pc, 0x4004);
        assert_eq!(cpu.regs.s, 0x7FFE);
        assert_eq!(memory[0x7FFE], 0x40);
        assert_eq!(memory[0x7FFF], 0x02);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn jsr_direct_pushes_return_pc_and_branches_via_direct_page() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x9D; // JSR <$34 with DP=$12 => $1234
        memory[0x4001] = 0x34;
        memory[0x1234] = 0x39; // RTS
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.s = 0x8000;

        run_cycles(&mut cpu, &mut memory, 7);
        assert_eq!(cpu.regs.pc, 0x1234);
        assert_eq!(cpu.regs.s, 0x7FFE);
        assert_eq!(memory[0x7FFE], 0x40);
        assert_eq!(memory[0x7FFF], 0x02);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 5);
        assert_eq!(cpu.regs.pc, 0x4002);
        assert_eq!(cpu.regs.s, 0x8000);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn jsr_extended_and_rts_round_trip_via_s_stack() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0xBD; // JSR $4500
        memory[0x4001] = 0x45;
        memory[0x4002] = 0x00;
        memory[0x4500] = 0x39; // RTS
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;

        run_cycles(&mut cpu, &mut memory, 8);
        assert_eq!(cpu.regs.pc, 0x4500);
        assert_eq!(cpu.regs.s, 0x7FFE);
        assert_eq!(memory[0x7FFE], 0x40);
        assert_eq!(memory[0x7FFF], 0x03);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 5);
        assert_eq!(cpu.regs.pc, 0x4003);
        assert_eq!(cpu.regs.s, 0x8000);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn swi_stacks_full_frame_and_rti_restores_it() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x3F; // SWI
        memory[0x4500] = 0x3B; // RTI
        memory[0xFFFA] = 0x45;
        memory[0xFFFB] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;
        cpu.regs.u = 0xA0B0;
        cpu.regs.y = 0xC0D0;
        cpu.regs.x = 0x1234;
        cpu.regs.dp = 0x56;
        cpu.regs.b = 0x78;
        cpu.regs.a = 0x9A;
        cpu.regs.cc = 0;

        run_until_boundary(&mut cpu, &mut memory);

        assert_eq!(cpu.regs.pc, 0x4500);
        assert_eq!(cpu.regs.s, 0x7FF4);
        assert_eq!(
            &memory[0x7FF4..=0x7FFF],
            &[
                FLAG_E, 0x9A, 0x78, 0x56, 0x12, 0x34, 0xC0, 0xD0, 0xA0, 0xB0, 0x40, 0x01
            ]
        );
        assert!(cpu.regs.flag(FLAG_I));
        assert!(cpu.regs.flag(FLAG_F));

        cpu.regs.u = 0;
        cpu.regs.y = 0;
        cpu.regs.x = 0;
        cpu.regs.dp = 0;
        cpu.regs.b = 0;
        cpu.regs.a = 0;
        run_until_boundary(&mut cpu, &mut memory);

        assert_eq!(cpu.regs.pc, 0x4001);
        assert_eq!(cpu.regs.s, 0x8000);
        assert_eq!(cpu.regs.u, 0xA0B0);
        assert_eq!(cpu.regs.y, 0xC0D0);
        assert_eq!(cpu.regs.x, 0x1234);
        assert_eq!(cpu.regs.dp, 0x56);
        assert_eq!(cpu.regs.b, 0x78);
        assert_eq!(cpu.regs.a, 0x9A);
        assert_eq!(cpu.regs.cc, FLAG_E);
    }

    #[test]
    fn external_irq_and_firq_enter_via_vectors_with_expected_stack_frames() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x12; // would be fetched if IRQ were not taken first
        memory[0xFFF8] = 0x45;
        memory[0xFFF9] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;
        cpu.regs.cc = 0;
        cpu.irq = true;

        run_until_boundary(&mut cpu, &mut memory);

        assert_eq!(cpu.regs.pc, 0x4500);
        assert_eq!(cpu.regs.s, 0x7FF4);
        assert_eq!(memory[0x7FF4], FLAG_E);
        assert_eq!(&memory[0x7FFE..=0x7FFF], &[0x40, 0x00]);
        assert!(cpu.regs.flag(FLAG_I));

        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x12;
        memory[0xFFF6] = 0x46;
        memory[0xFFF7] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;
        cpu.regs.cc = 0;
        cpu.firq = true;

        run_until_boundary(&mut cpu, &mut memory);

        assert_eq!(cpu.regs.pc, 0x4600);
        assert_eq!(cpu.regs.s, 0x7FFD);
        assert_eq!(&memory[0x7FFD..=0x7FFF], &[0x00, 0x40, 0x00]);
        assert!(cpu.regs.flag(FLAG_I));
        assert!(cpu.regs.flag(FLAG_F));
        assert!(!cpu.regs.flag(FLAG_E));
    }

    #[test]
    fn prefixed_software_interrupts_use_separate_vectors_without_masking() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x10; // SWI2
        memory[0x4001] = 0x3F;
        memory[0xFFF4] = 0x48;
        memory[0xFFF5] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;
        cpu.regs.cc = FLAG_C;

        run_until_boundary(&mut cpu, &mut memory);

        assert_eq!(cpu.regs.pc, 0x4800);
        assert_eq!(memory[0x7FF4], FLAG_E | FLAG_C);
        assert_eq!(&memory[0x7FFE..=0x7FFF], &[0x40, 0x02]);
        assert!(!cpu.regs.flag(FLAG_I));
        assert!(!cpu.regs.flag(FLAG_F));
    }

    #[test]
    fn sync_waits_for_interrupt_without_consuming_next_opcode() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x13; // SYNC
        memory[0x4001] = 0x12; // must remain unfetched until after interrupt
        memory[0xFFFC] = 0x49;
        memory[0xFFFD] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;
        cpu.regs.cc = 0;

        run_cycle(&mut cpu, &mut memory);
        assert_eq!(cpu.regs.pc, 0x4001);
        assert!(!cpu.instruction_boundary());

        cpu.nmi = true;
        run_until_boundary(&mut cpu, &mut memory);

        assert_eq!(cpu.regs.pc, 0x4900);
        assert_eq!(cpu.regs.s, 0x7FF4);
        assert_eq!(&memory[0x7FFE..=0x7FFF], &[0x40, 0x01]);
        assert!(cpu.regs.flag(FLAG_I));
    }

    #[test]
    fn sync_continues_after_masked_irq_without_stacking() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x13; // SYNC
        memory[0x4001] = 0x12; // NOP
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;
        cpu.regs.cc = FLAG_I;

        run_cycle(&mut cpu, &mut memory);
        assert_eq!(cpu.regs.pc, 0x4001);
        assert!(!cpu.instruction_boundary());

        cpu.irq = true;
        run_until_boundary(&mut cpu, &mut memory);

        assert_eq!(cpu.regs.pc, 0x4001);
        assert_eq!(cpu.regs.s, 0x8000);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn cwai_stacks_then_vectors_when_interrupt_arrives() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x3C; // CWAI #$EF
        memory[0x4001] = 0xEF;
        memory[0xFFF8] = 0x47;
        memory[0xFFF9] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;
        cpu.regs.cc = FLAG_I | FLAG_C;

        run_cycles(&mut cpu, &mut memory, 14);
        assert_eq!(cpu.regs.s, 0x7FF4);
        assert_eq!(memory[0x7FF4], FLAG_E | FLAG_C);
        assert!(!cpu.instruction_boundary());

        cpu.irq = true;
        run_until_boundary(&mut cpu, &mut memory);

        assert_eq!(cpu.regs.pc, 0x4700);
        assert_eq!(cpu.regs.s, 0x7FF4);
        assert!(cpu.regs.flag(FLAG_I));
    }

    #[test]
    fn pshs_and_puls_round_trip_full_register_set() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x34; // PSHS all
        memory[0x4001] = 0xFF;
        memory[0x4002] = 0x35; // PULS all
        memory[0x4003] = 0xFF;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;
        cpu.regs.u = 0xA0B0;
        cpu.regs.y = 0xC0D0;
        cpu.regs.x = 0x1234;
        cpu.regs.dp = 0x56;
        cpu.regs.b = 0x78;
        cpu.regs.a = 0x9A;
        cpu.regs.cc = 0xA5;

        run_cycles(&mut cpu, &mut memory, 14);

        assert_eq!(cpu.regs.s, 0x7FF4);
        assert_eq!(
            &memory[0x7FF4..=0x7FFF],
            &[
                0xA5, 0x9A, 0x78, 0x56, 0x12, 0x34, 0xC0, 0xD0, 0xA0, 0xB0, 0x40, 0x02
            ]
        );
        assert!(cpu.instruction_boundary());

        cpu.regs.u = 0;
        cpu.regs.y = 0;
        cpu.regs.x = 0;
        cpu.regs.dp = 0;
        cpu.regs.b = 0;
        cpu.regs.a = 0;
        cpu.regs.cc = 0;

        run_cycles(&mut cpu, &mut memory, 14);

        assert_eq!(cpu.regs.s, 0x8000);
        assert_eq!(cpu.regs.u, 0xA0B0);
        assert_eq!(cpu.regs.y, 0xC0D0);
        assert_eq!(cpu.regs.x, 0x1234);
        assert_eq!(cpu.regs.dp, 0x56);
        assert_eq!(cpu.regs.b, 0x78);
        assert_eq!(cpu.regs.a, 0x9A);
        assert_eq!(cpu.regs.cc, 0xA5);
        assert_eq!(cpu.regs.pc, 0x4002);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn pshu_and_pulu_transfer_s_on_u_stack() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x36; // PSHU S
        memory[0x4001] = 0x40;
        memory[0x4002] = 0x37; // PULU S
        memory[0x4003] = 0x40;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x1234;
        cpu.regs.u = 0x9000;

        run_cycles(&mut cpu, &mut memory, 4);

        assert_eq!(cpu.regs.u, 0x8FFE);
        assert_eq!(memory[0x8FFE], 0x12);
        assert_eq!(memory[0x8FFF], 0x34);
        assert!(cpu.instruction_boundary());

        cpu.regs.s = 0;

        run_cycles(&mut cpu, &mut memory, 4);

        assert_eq!(cpu.regs.s, 0x1234);
        assert_eq!(cpu.regs.u, 0x9000);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn lea_indexed_updates_target_registers_and_x_y_zero_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x30; // LEAX 5,X
        memory[0x4001] = 0x05;
        memory[0x4002] = 0x31; // LEAY -16,Y
        memory[0x4003] = 0x30;
        memory[0x4004] = 0x32; // LEAS $0010,S
        memory[0x4005] = 0xE9;
        memory[0x4006] = 0x00;
        memory[0x4007] = 0x10;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.x = 0x1000;
        cpu.regs.y = 0x0010;
        cpu.regs.s = 0x8000;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(cpu.regs.x, 0x1005);
        assert!(!cpu.regs.flag(FLAG_Z));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(cpu.regs.y, 0x0000);
        assert!(cpu.regs.flag(FLAG_Z));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 7);
        assert_eq!(cpu.regs.s, 0x8010);
        assert!(cpu.regs.flag(FLAG_Z), "LEAS does not update Z");
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn indexed_load_store_and_auto_increment_use_effective_address() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0xA6; // LDA ,X+
        memory[0x4001] = 0x80;
        memory[0x2000] = 0x7E;
        memory[0x4002] = 0xE7; // STB B,X
        memory[0x4003] = 0x85;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.x = 0x2000;
        cpu.regs.b = 0x05;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(cpu.regs.a, 0x7E);
        assert_eq!(cpu.regs.x, 0x2001);
        assert!(cpu.instruction_boundary());

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(memory[0x2006], 0x05);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn direct_indexed_and_extended_rmw_ops_use_bus_cycles() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x00; // NEG <$10
        memory[0x4001] = 0x10;
        memory[0x1210] = 0x01;
        memory[0x4002] = 0x6C; // INC ,X
        memory[0x4003] = 0x84;
        memory[0x2200] = 0x7F;
        memory[0x4004] = 0x7F; // CLR $3300
        memory[0x4005] = 0x33;
        memory[0x4006] = 0x00;
        memory[0x3300] = 0x55;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.x = 0x2200;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(memory[0x1210], 0xFF);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_C));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(memory[0x2200], 0x80);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_V));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 7);
        assert_eq!(memory[0x3300], 0x00);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn memory_tst_reads_without_writing_back() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x0D; // TST <$10
        memory[0x4001] = 0x10;
        memory[0x1210] = 0x80;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.set_flag(FLAG_C, true);

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);

        assert_eq!(memory[0x1210], 0x80);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
        assert!(cpu.regs.flag(FLAG_C), "TST does not affect C");
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn memory_shift_and_rotate_ops_cover_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x74; // LSR $3000
        memory[0x4001] = 0x30;
        memory[0x4002] = 0x00;
        memory[0x3000] = 0x01;
        memory[0x4003] = 0x68; // ASL ,X
        memory[0x4004] = 0x84;
        memory[0x2200] = 0x80;
        memory[0x4005] = 0x09; // ROL <$20
        memory[0x4006] = 0x20;
        memory[0x1220] = 0x00;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.x = 0x2200;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 7);
        assert_eq!(memory[0x3000], 0x00);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_C));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(memory[0x2200], 0x00);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_C));
        assert!(cpu.regs.flag(FLAG_V));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 6);
        assert_eq!(memory[0x1220], 0x01);
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn indexed_pc_relative_jmp_uses_pc_after_offset_operand() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x6E; // JMP 3,PC
        memory[0x4001] = 0x8C;
        memory[0x4002] = 0x03;
        let mut cpu = cpu_at(0x4000);

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 3);

        assert_eq!(cpu.regs.pc, 0x4006);
        assert_eq!(cpu.addr, 0x4006);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn compare_immediate_updates_subtraction_flags_without_mutating_registers() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x81; // CMPA #$20
        memory[0x4001] = 0x20;
        memory[0x4002] = 0xC1; // CMPB #$01
        memory[0x4003] = 0x01;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.a = 0x10;
        cpu.regs.b = 0x80;

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x10);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
        assert!(cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0x80);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_V));
        assert!(!cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn compare_direct_indexed_and_extended_share_memory_read_path() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x91; // CMPA <$10
        memory[0x4001] = 0x10;
        memory[0x1210] = 0x42;
        memory[0x4002] = 0xE1; // CMPB ,X
        memory[0x4003] = 0x84;
        memory[0x2200] = 0x42;
        memory[0x4004] = 0xB1; // CMPA $3300
        memory[0x4005] = 0x33;
        memory[0x4006] = 0x00;
        memory[0x3300] = 0x40;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.x = 0x2200;
        cpu.regs.a = 0x42;
        cpu.regs.b = 0x40;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 4);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 4);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_C));

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));
        assert_eq!(cpu.regs.a, 0x42);
        assert_eq!(cpu.regs.b, 0x40);
    }

    #[test]
    fn add_subtract_and_carry_update_arithmetic_flags() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x8B; // ADDA #$01
        memory[0x4001] = 0x01;
        memory[0x4002] = 0x89; // ADCA #$7F
        memory[0x4003] = 0x7F;
        memory[0x4004] = 0xC0; // SUBB #$20
        memory[0x4005] = 0x20;
        memory[0x4006] = 0xC2; // SBCB #$0F
        memory[0x4007] = 0x0F;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.a = 0x7F;
        cpu.regs.b = 0x10;

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x80);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_V));
        assert!(cpu.regs.flag(FLAG_H));
        assert!(!cpu.regs.flag(FLAG_C));

        cpu.regs.set_flag(FLAG_C, true);
        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x00);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0xF0);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0xE0);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
    }

    #[test]
    fn logical_operations_update_nzv_and_preserve_destination_for_bit() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x84; // ANDA #$0F
        memory[0x4001] = 0x0F;
        memory[0x4002] = 0x8A; // ORA #$80
        memory[0x4003] = 0x80;
        memory[0x4004] = 0x88; // EORA #$FF
        memory[0x4005] = 0xFF;
        memory[0x4006] = 0xC5; // BITB #$0F
        memory[0x4007] = 0x0F;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.a = 0xF3;
        cpu.regs.b = 0xF0;
        cpu.regs.set_flag(FLAG_C, true);
        cpu.regs.set_flag(FLAG_V, true);

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x03);
        assert!(!cpu.regs.flag(FLAG_N));
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_V));
        assert!(cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x83);
        assert!(cpu.regs.flag(FLAG_N));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.a, 0x7C);
        assert!(!cpu.regs.flag(FLAG_N));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.b, 0xF0);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn alu_direct_indexed_and_extended_share_memory_read_path() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x9B; // ADDA <$10
        memory[0x4001] = 0x10;
        memory[0x1210] = 0x05;
        memory[0x4002] = 0xE4; // ANDB ,X
        memory[0x4003] = 0x84;
        memory[0x2200] = 0x0F;
        memory[0x4004] = 0xBA; // ORA $3300
        memory[0x4005] = 0x33;
        memory[0x4006] = 0x00;
        memory[0x3300] = 0x80;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;
        cpu.regs.x = 0x2200;
        cpu.regs.a = 0x10;
        cpu.regs.b = 0xF3;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 4);
        assert_eq!(cpu.regs.a, 0x15);

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 4);
        assert_eq!(cpu.regs.b, 0x03);

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 5);
        assert_eq!(cpu.regs.a, 0x95);
        assert!(cpu.regs.flag(FLAG_N));
    }

    #[test]
    fn indirect_indexed_load_reads_pointer_then_effective_address() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0xA6; // LDA [,X++]
        memory[0x4001] = 0x91;
        memory[0x2000] = 0x34;
        memory[0x2001] = 0x56;
        memory[0x3456] = 0xA5;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.x = 0x2000;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 10);

        assert_eq!(cpu.regs.a, 0xA5);
        assert_eq!(cpu.regs.x, 0x2002);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn indirect_pc_relative_store_writes_resolved_effective_address() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0xE7; // STB [2,PC]
        memory[0x4001] = 0x9C;
        memory[0x4002] = 0x02;
        memory[0x4005] = 0x45;
        memory[0x4006] = 0x67;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.b = 0x5A;

        assert_eq!(run_instruction_cycles(&mut cpu, &mut memory), 8);

        assert_eq!(memory[0x4567], 0x5A);
        assert_eq!(cpu.regs.pc, 0x4003);
        assert!(cpu.instruction_boundary());
    }
}
