//! Motorola MC6809 CPU foundation.
//!
//! This crate starts with the external bus-facing state needed by Dragon/CoCo
//! machine wiring. Instruction execution will grow behind this boundary; the
//! public pin/register shape is deliberately small and serializable so machine
//! snapshots can use it directly.

pub mod registers;

use registers::{FLAG_C, FLAG_F, FLAG_I, FLAG_N, FLAG_V, FLAG_Z, Registers};
use serde::{Deserialize, Serialize};

const MAX_STACK_BYTES: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum CpuState {
    Fetch,
    Prefix10,
    NopInternal,
    ClrAInternal,
    ClrBInternal,
    ReadCcImm(CcOp),
    ReadImm8(Imm8Op),
    ReadImm16Hi(Imm16Op),
    ReadImm16Lo { op: Imm16Op, hi: u8 },
    ReadRel8(Rel8Op),
    ReadIndexedPostbyte(IndexedOp),
    ReadIndexedOffset8 { op: IndexedOp, post: u8 },
    ReadIndexedOffset16Hi { op: IndexedOp, post: u8 },
    ReadIndexedOffset16Lo { op: IndexedOp, post: u8, hi: u8 },
    ReadStackPostbyte(StackOp),
    ReadDirectOperand(Mem8Op),
    ReadDirectValue(Mem8Op),
    ReadExtendedHi(ExtOp),
    ReadExtendedLo { op: ExtOp, hi: u8 },
    ReadExtendedValue(Mem8Op),
    WriteDirectOperand(Store8Op),
    WriteValue,
    StackRead,
    StackWrite,
    PushWordHi { hi: u8, after: AfterPush },
    PushDone(AfterPush),
    PullWordHi(Pull16Op),
    PullWordLo { op: Pull16Op, hi: u8 },
    IllegalOpcode(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum CcOp {
    And,
    Or,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Imm8Op {
    Lda,
    Ldb,
    Cmpa,
    Cmpb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Imm16Op {
    Ldx,
    Ldu,
    Lds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Rel8Op {
    Branch(BranchCondition),
    Bsr,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Store8Op {
    Sta,
    Stb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum ExtOp {
    Load(Mem8Op),
    Store(Store8Op),
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
enum IndexedOp {
    Lea(Reg16),
    Load(Mem8Op),
    Store(Store8Op),
    Jmp,
    Jsr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum AfterPush {
    SetPc(u16),
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
                let opcode = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.sync = false;
                match opcode {
                    0x10 => self.read_next(CpuState::Prefix10),
                    0x12 => {
                        // NOP is a two-cycle instruction: opcode fetch plus
                        // one internal cycle.
                        self.state = CpuState::NopInternal;
                        self.addr = self.regs.pc;
                        self.rw = true;
                    }
                    0x1A => self.read_next(CpuState::ReadCcImm(CcOp::Or)),
                    0x1C => self.read_next(CpuState::ReadCcImm(CcOp::And)),
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
                    0x39 => self.prepare_pull_word(Pull16Op::Pc),
                    0x4F => {
                        self.clear_a();
                        self.read_next(CpuState::ClrAInternal);
                    }
                    0x5F => {
                        self.clear_b();
                        self.read_next(CpuState::ClrBInternal);
                    }
                    0x6E => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Jmp)),
                    0x7E => self.read_next(CpuState::ReadExtendedHi(ExtOp::Jmp)),
                    0x81 => self.read_next(CpuState::ReadImm8(Imm8Op::Cmpa)),
                    0x86 => self.read_next(CpuState::ReadImm8(Imm8Op::Lda)),
                    0x8D => self.read_next(CpuState::ReadRel8(Rel8Op::Bsr)),
                    0x8E => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Ldx)),
                    0x91 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Cmpa)),
                    0x96 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Lda)),
                    0x97 => self.read_next(CpuState::WriteDirectOperand(Store8Op::Sta)),
                    0xA1 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                            Mem8Op::Cmpa,
                        )));
                    }
                    0xA6 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(Mem8Op::Lda)));
                    }
                    0xA7 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Store(
                        Store8Op::Sta,
                    ))),
                    0xAD => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Jsr)),
                    0xB1 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Cmpa))),
                    0xB6 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Lda))),
                    0xB7 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store(Store8Op::Sta))),
                    0xBD => self.read_next(CpuState::ReadExtendedHi(ExtOp::Jsr)),
                    0xC1 => self.read_next(CpuState::ReadImm8(Imm8Op::Cmpb)),
                    0xC6 => self.read_next(CpuState::ReadImm8(Imm8Op::Ldb)),
                    0xCE => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Ldu)),
                    0xD1 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Cmpb)),
                    0xD6 => self.read_next(CpuState::ReadDirectOperand(Mem8Op::Ldb)),
                    0xD7 => self.read_next(CpuState::WriteDirectOperand(Store8Op::Stb)),
                    0xE1 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(
                            Mem8Op::Cmpb,
                        )));
                    }
                    0xE6 => {
                        self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Load(Mem8Op::Ldb)));
                    }
                    0xE7 => self.read_next(CpuState::ReadIndexedPostbyte(IndexedOp::Store(
                        Store8Op::Stb,
                    ))),
                    0xF1 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Cmpb))),
                    0xF6 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Load(Mem8Op::Ldb))),
                    0xF7 => self.read_next(CpuState::ReadExtendedHi(ExtOp::Store(Store8Op::Stb))),
                    _ => self.trap_illegal(opcode),
                }
            }
            CpuState::Prefix10 => {
                let opcode = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                match opcode {
                    0xCE => self.read_next(CpuState::ReadImm16Hi(Imm16Op::Lds)),
                    _ => self.trap_illegal(opcode),
                }
            }
            CpuState::NopInternal => {
                self.next_fetch();
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
                        self.next_fetch();
                    }
                    Rel8Op::Bsr => self.prepare_push_word(self.regs.pc, AfterPush::SetPc(target)),
                }
            }
            CpuState::ReadIndexedPostbyte(op) => {
                let post = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.resolve_indexed_postbyte(op, post);
            }
            CpuState::ReadIndexedOffset8 { op, post } => {
                let offset = self.data_in as i8;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let base = if Self::indexed_low5(post) == 0x0C {
                    self.regs.pc
                } else {
                    self.index_base(post)
                };
                self.apply_indexed_effective_address(
                    op,
                    base.wrapping_add_signed(i16::from(offset)),
                );
            }
            CpuState::ReadIndexedOffset16Hi { op, post } => {
                let hi = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.read_next(CpuState::ReadIndexedOffset16Lo { op, post, hi });
            }
            CpuState::ReadIndexedOffset16Lo { op, post, hi } => {
                let offset = u16::from_be_bytes([hi, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let base = if Self::indexed_low5(post) == 0x0D {
                    self.regs.pc
                } else {
                    self.index_base(post)
                };
                self.apply_indexed_effective_address(op, base.wrapping_add(offset));
            }
            CpuState::ReadStackPostbyte(op) => {
                let mask = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.start_stack_op(op, mask);
            }
            CpuState::ReadDirectOperand(op) => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.state = CpuState::ReadDirectValue(op);
                self.addr = addr;
                self.rw = true;
                self.sync = false;
            }
            CpuState::ReadDirectValue(op) => {
                self.load_mem8(op, self.data_in);
                self.next_fetch();
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
                    ExtOp::Store(op) => self.prepare_store(op, addr),
                    ExtOp::Jmp => {
                        self.regs.pc = addr;
                        self.next_fetch();
                    }
                    ExtOp::Jsr => self.prepare_push_word(self.regs.pc, AfterPush::SetPc(addr)),
                }
            }
            CpuState::ReadExtendedValue(op) => {
                self.load_mem8(op, self.data_in);
                self.next_fetch();
            }
            CpuState::WriteDirectOperand(op) => {
                let addr = u16::from_be_bytes([self.regs.dp, self.data_in]);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.prepare_store(op, addr);
            }
            CpuState::WriteValue => {
                self.next_fetch();
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
                    Pull16Op::Pc => self.regs.pc = value,
                }
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
        }
    }

    fn load_imm16(&mut self, op: Imm16Op, value: u16) {
        match op {
            Imm16Op::Ldx => self.regs.x = value,
            Imm16Op::Ldu => self.regs.u = value,
            Imm16Op::Lds => self.regs.s = value,
        }
        self.set_nz16(value);
        self.regs.set_flag(FLAG_V, false);
    }

    fn resolve_indexed_postbyte(&mut self, op: IndexedOp, post: u8) {
        if post & 0x80 == 0 {
            let offset = if post & 0x10 != 0 {
                i16::from((post | 0xE0) as i8)
            } else {
                i16::from((post & 0x1F) as i8)
            };
            let addr = self.index_base(post).wrapping_add_signed(offset);
            self.apply_indexed_effective_address(op, addr);
            return;
        }

        match Self::indexed_low5(post) {
            0x00 => {
                let addr = self.index_base(post);
                self.set_index_base(post, addr.wrapping_add(1));
                self.apply_indexed_effective_address(op, addr);
            }
            0x01 => {
                let addr = self.index_base(post);
                self.set_index_base(post, addr.wrapping_add(2));
                self.apply_indexed_effective_address(op, addr);
            }
            0x02 => {
                let addr = self.index_base(post).wrapping_sub(1);
                self.set_index_base(post, addr);
                self.apply_indexed_effective_address(op, addr);
            }
            0x03 => {
                let addr = self.index_base(post).wrapping_sub(2);
                self.set_index_base(post, addr);
                self.apply_indexed_effective_address(op, addr);
            }
            0x04 => self.apply_indexed_effective_address(op, self.index_base(post)),
            0x05 => {
                let offset = i16::from(self.regs.b as i8);
                self.apply_indexed_effective_address(
                    op,
                    self.index_base(post).wrapping_add_signed(offset),
                );
            }
            0x06 => {
                let offset = i16::from(self.regs.a as i8);
                self.apply_indexed_effective_address(
                    op,
                    self.index_base(post).wrapping_add_signed(offset),
                );
            }
            0x08 | 0x0C => self.read_next(CpuState::ReadIndexedOffset8 { op, post }),
            0x09 | 0x0D => self.read_next(CpuState::ReadIndexedOffset16Hi { op, post }),
            0x0B => self.apply_indexed_effective_address(
                op,
                self.index_base(post).wrapping_add(self.regs.d()),
            ),
            _ => self.trap_illegal(post),
        }
    }

    fn apply_indexed_effective_address(&mut self, op: IndexedOp, addr: u16) {
        match op {
            IndexedOp::Lea(reg) => {
                self.set_reg16(reg, addr);
                if matches!(reg, Reg16::X | Reg16::Y) {
                    self.regs.set_flag(FLAG_Z, addr == 0);
                }
                self.next_fetch();
            }
            IndexedOp::Load(op) => {
                self.state = CpuState::ReadExtendedValue(op);
                self.addr = addr;
                self.rw = true;
                self.sync = false;
            }
            IndexedOp::Store(op) => self.prepare_store(op, addr),
            IndexedOp::Jmp => {
                self.regs.pc = addr;
                self.next_fetch();
            }
            IndexedOp::Jsr => self.prepare_push_word(self.regs.pc, AfterPush::SetPc(addr)),
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

    fn prepare_store(&mut self, op: Store8Op, addr: u16) {
        let value = match op {
            Store8Op::Sta => self.regs.a,
            Store8Op::Stb => self.regs.b,
        };
        self.set_load_flags8(value);
        self.state = CpuState::WriteValue;
        self.addr = addr;
        self.data = value;
        self.rw = false;
        self.sync = false;
    }

    fn start_stack_op(&mut self, op: StackOp, mask: u8) {
        match op {
            StackOp::PushS => self.start_stack_push(StackPointer::S, mask),
            StackOp::PullS => self.start_stack_pull(StackPointer::S, mask),
            StackOp::PushU => self.start_stack_push(StackPointer::U, mask),
            StackOp::PullU => self.start_stack_pull(StackPointer::U, mask),
        }
    }

    fn start_stack_push(&mut self, ptr: StackPointer, mask: u8) {
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
            } => {
                if index >= len {
                    self.next_fetch();
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
        } = self.stack_work
        {
            self.stack_work = StackWork::Push {
                ptr,
                bytes,
                len,
                index: index + 1,
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
        }
        self.next_fetch();
    }

    fn set_load_flags8(&mut self, value: u8) {
        self.set_nz8(value);
        self.regs.set_flag(FLAG_V, false);
    }

    fn compare8(&mut self, lhs: u8, rhs: u8) {
        let result = lhs.wrapping_sub(rhs);
        self.set_nz8(result);
        self.regs
            .set_flag(FLAG_V, ((lhs ^ rhs) & (lhs ^ result) & 0x80) != 0);
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

    fn cpu_at(pc: u16) -> Mc6809 {
        let mut cpu = Mc6809::new();
        cpu.regs.pc = pc;
        cpu.addr = pc;
        cpu.sync = true;
        cpu
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
        cpu.data_in = 0xFF;

        cpu.tick();

        assert!(cpu.halt);
        assert_eq!(cpu.regs.pc, 0x4001);
        assert!(!cpu.instruction_boundary());
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
    fn direct_loads_use_direct_page_register() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x96; // LDA <$34
        memory[0x4001] = 0x34;
        memory[0x1234] = 0x7F;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.dp = 0x12;

        run_cycles(&mut cpu, &mut memory, 3);

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

        run_cycles(&mut cpu, &mut memory, 4);
        assert_eq!(cpu.regs.a, 0xA5);
        assert!(cpu.regs.flag(FLAG_N));

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(memory[0x1220], 0x5A);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 4);
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

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.pc, 0x4004);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.pc, 0x4567);
        assert_eq!(cpu.addr, 0x4567);
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

            run_cycles(&mut cpu, &mut memory, 2);

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
    fn bsr_pushes_return_pc_and_branches_relative() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x8D; // BSR +2
        memory[0x4001] = 0x02;
        let mut cpu = cpu_at(0x4000);
        cpu.regs.s = 0x8000;

        run_cycles(&mut cpu, &mut memory, 4);

        assert_eq!(cpu.regs.pc, 0x4004);
        assert_eq!(cpu.regs.s, 0x7FFE);
        assert_eq!(memory[0x7FFE], 0x40);
        assert_eq!(memory[0x7FFF], 0x02);
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

        run_cycles(&mut cpu, &mut memory, 5);
        assert_eq!(cpu.regs.pc, 0x4500);
        assert_eq!(cpu.regs.s, 0x7FFE);
        assert_eq!(memory[0x7FFE], 0x40);
        assert_eq!(memory[0x7FFF], 0x03);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.pc, 0x4003);
        assert_eq!(cpu.regs.s, 0x8000);
        assert!(cpu.instruction_boundary());
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

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.x, 0x1005);
        assert!(!cpu.regs.flag(FLAG_Z));

        run_cycles(&mut cpu, &mut memory, 2);
        assert_eq!(cpu.regs.y, 0x0000);
        assert!(cpu.regs.flag(FLAG_Z));

        run_cycles(&mut cpu, &mut memory, 4);
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

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(cpu.regs.a, 0x7E);
        assert_eq!(cpu.regs.x, 0x2001);
        assert!(cpu.instruction_boundary());

        run_cycles(&mut cpu, &mut memory, 3);
        assert_eq!(memory[0x2006], 0x05);
        assert!(cpu.instruction_boundary());
    }

    #[test]
    fn indexed_pc_relative_jmp_uses_pc_after_offset_operand() {
        let mut memory = [0; 0x10000];
        memory[0x4000] = 0x6E; // JMP 3,PC
        memory[0x4001] = 0x8C;
        memory[0x4002] = 0x03;
        let mut cpu = cpu_at(0x4000);

        run_cycles(&mut cpu, &mut memory, 3);

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

        run_cycles(&mut cpu, &mut memory, 3);
        assert!(cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 3);
        assert!(cpu.regs.flag(FLAG_N));
        assert!(cpu.regs.flag(FLAG_C));

        run_cycles(&mut cpu, &mut memory, 4);
        assert!(!cpu.regs.flag(FLAG_Z));
        assert!(!cpu.regs.flag(FLAG_C));
        assert_eq!(cpu.regs.a, 0x42);
        assert_eq!(cpu.regs.b, 0x40);
    }
}
