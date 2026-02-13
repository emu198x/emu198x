//! Recipe-based instruction execution for the 68000.
//!
//! Each instruction is described as a sequence of RecipeOps. The recipe engine
//! processes these operations, expanding them into MicroOps for bus-cycle
//! accurate execution.

#![allow(clippy::match_same_arms)]

use super::Cpu68000;
use super::microcode::MicroOp;
use crate::common::addressing::AddrMode;
use crate::common::alu::Size;
use crate::common::flags::{self, Status, C, N, S, V, X, Z};

/// Side selector for recipe EA operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EaSide {
    /// Source operand.
    Src,
    /// Destination operand.
    Dst,
}

/// ALU ops supported by the recipe path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeAlu {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Bitwise AND.
    And,
    /// Bitwise OR.
    Or,
    /// Bitwise exclusive OR.
    Eor,
}

/// Unary ops supported by the recipe path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeUnary {
    /// Negate (0 - value).
    Neg,
    /// Complement (~value).
    Not,
    /// Negate with extend (0 - value - X).
    Negx,
}

impl RecipeUnary {
    /// Convert to AluMemRmw operation code.
    pub(super) fn to_alu_mem_rmw(self) -> u8 {
        match self {
            Self::Neg => 5,
            Self::Not => 6,
            Self::Negx => 7,
        }
    }
}

/// Recipe operations for instruction execution.
///
/// Each instruction decode builds a sequence of these operations.
/// The recipe engine expands them into MicroOps for timed execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeOp {
    /// Fetch N extension words.
    FetchExtWords(u8),
    /// Calculate EA for the given side.
    CalcEa(EaSide),
    /// Read EA value into `data` (Src) or `data2` (Dst).
    ReadEa(EaSide),
    /// Write EA value from `data` (Dst).
    WriteEa(EaSide),
    /// Load immediate into `data` from `recipe_imm`.
    LoadImm,
    /// Load computed EA address into `data`.
    LoadEaAddr(EaSide),
    /// Set N/Z and clear V/C based on `data` and `size`.
    SetFlagsMove,
    /// Internal cycles.
    Internal(u8),
    /// Advance extension word cursor by N.
    SkipExt(u8),
    /// Setup MOVEM registers-to-memory transfer.
    MovemToMem,
    /// Setup MOVEM memory-to-registers transfer.
    MovemFromMem,
    /// ALU operation writing to a data register.
    AluReg { op: RecipeAlu, reg: u8 },
    /// ALU operation writing back to EA.
    AluMem { op: RecipeAlu },
    /// Compare `data` against a register, set flags only.
    CmpReg { reg: u8, addr: bool },
    /// Add/sub source `data` to address register (no flags).
    AddrArith { reg: u8, add: bool },
    /// Read immediate bit number into `data`.
    ReadBitImm { reg: bool },
    /// Read bit number from Dn into `data`.
    ReadBitReg { reg: u8, mem: bool },
    /// Bit operation on data register.
    BitReg { reg: u8, op: u8 },
    /// Bit operation on memory byte.
    BitMem { op: u8 },
    /// Compare `data` (src) against `data2` (dst), set flags only.
    CmpEa,
    /// Set byte based on condition code (Scc).
    Scc { condition: u8 },
    /// Decrement and branch on condition false (DBcc).
    Dbcc { condition: u8, reg: u8 },
    /// Branch/BSR using opcode displacement.
    Branch { condition: u8, disp8: i8 },
    /// Multiply word source by Dn.
    Mul { signed: bool, reg: u8 },
    /// Divide Dn by word source.
    Div { signed: bool, reg: u8 },
    /// Exchange registers.
    Exg { kind: u8, rx: u8, ry: u8 },
    /// ABCD register-to-register.
    AbcdReg { src: u8, dst: u8 },
    /// SBCD register-to-register.
    SbcdReg { src: u8, dst: u8 },
    /// CMPM (Ay)+,(Ax)+.
    Cmpm { ax: u8, ay: u8 },
    /// Jump to subroutine using computed EA.
    Jsr,
    /// Jump to computed EA.
    Jmp,
    /// RTS: pop return address.
    RtsPop,
    /// RTS: finish and set PC.
    RtsFinish,
    /// RTE: pop SR.
    RtePopSr,
    /// RTE: pop PC.
    RtePopPc,
    /// RTE: finish and set PC/SR.
    RteFinish,
    /// RTR: pop CCR.
    RtrPopCcr,
    /// RTR: pop PC.
    RtrPopPc,
    /// RTR: finish and set PC/CCR.
    RtrFinish,
    /// LINK: push An and capture displacement.
    LinkStart { reg: u8 },
    /// LINK: finish by updating An/SP.
    LinkFinish,
    /// UNLK: restore SP and pop An.
    UnlkStart { reg: u8 },
    /// UNLK: finish by writing An.
    UnlkFinish,
    /// MOVE USP (requires supervisor).
    MoveUsp { reg: u8, to_usp: bool },
    /// TRAP vector.
    Trap { vector: u8 },
    /// TRAPV.
    Trapv,
    /// RESET (requires supervisor).
    Reset,
    /// STOP (requires supervisor).
    Stop,
    /// Write condition code register from `data`.
    WriteCcr,
    /// Write status register from `data`.
    WriteSr,
    /// Logical op on CCR using `data`.
    LogicCcr { op: RecipeAlu },
    /// Logical op on SR using `data`.
    LogicSr { op: RecipeAlu },
    /// ADDX register to register.
    AddxReg { src: u8, dst: u8 },
    /// SUBX register to register.
    SubxReg { src: u8, dst: u8 },
    /// Unary operation on a data register.
    UnaryReg { op: RecipeUnary, reg: u8 },
    /// Unary operation on a memory EA.
    UnaryMem { op: RecipeUnary },
    /// Shift/rotate on a data register.
    ShiftReg {
        kind: u8,
        direction: bool,
        count_or_reg: u8,
        reg: u8,
        immediate: bool,
    },
    /// Shift/rotate on a memory EA.
    ShiftMem { kind: u8, direction: bool },
    /// Multi-precision memory-to-memory predecrement.
    ExtendMem { op: u8, src: u8, dst: u8 },
    /// Swap high/low words of a data register.
    SwapReg { reg: u8 },
    /// Sign-extend a data register.
    Ext { size: Size, reg: u8 },
    /// Push `data` as a long onto the stack.
    PushLong,
}

/// Maximum recipe operations per instruction.
pub(super) const RECIPE_MAX_OPS: usize = 16;

impl Cpu68000 {
    /// Reset recipe state.
    pub(super) fn recipe_reset(&mut self) {
        self.recipe_len = 0;
        self.recipe_idx = 0;
    }

    /// Begin building a new recipe.
    pub(super) fn recipe_begin(&mut self) {
        self.recipe_len = 0;
        self.recipe_idx = 0;
        self.recipe_src_is_reg = false;
        self.recipe_dst_is_reg = false;
    }

    /// Push a recipe operation.
    pub(super) fn recipe_push(&mut self, op: RecipeOp) -> bool {
        if (self.recipe_len as usize) >= self.recipe_ops.len() {
            return false;
        }
        self.recipe_ops[self.recipe_len as usize] = op;
        self.recipe_len += 1;
        true
    }

    /// Commit the recipe — queue RecipeStep to start processing.
    pub(super) fn recipe_commit(&mut self) -> bool {
        if self.recipe_len == 0 {
            return false;
        }
        self.ext_count = 0;
        self.ext_idx = 0;
        self.micro_ops.push(MicroOp::RecipeStep);
        true
    }

    /// Process the next recipe step.
    ///
    /// Instant operations are processed in a loop. When a timed operation
    /// is encountered (memory access, internal cycles), the appropriate
    /// MicroOps are queued and control returns to the tick loop.
    #[allow(clippy::too_many_lines)]
    pub(super) fn tick_recipe_step(&mut self) {
        while (self.recipe_idx as usize) < self.recipe_len as usize {
            let op = self.recipe_ops[self.recipe_idx as usize];
            self.recipe_idx += 1;

            match op {
                RecipeOp::FetchExtWords(count) => {
                    if count > 0 {
                        for _ in 0..count {
                            self.micro_ops.push(MicroOp::FetchExtWord);
                        }
                        self.micro_ops.push(MicroOp::RecipeStep);
                        return;
                    }
                }
                RecipeOp::CalcEa(side) => {
                    let (mode, pc_at_ext) = match side {
                        EaSide::Src => (self.src_mode, self.recipe_src_pc_at_ext),
                        EaSide::Dst => (self.dst_mode, self.recipe_dst_pc_at_ext),
                    };
                    let Some(mode) = mode else {
                        self.recipe_reset();
                        return;
                    };
                    let (addr, is_reg) = self.calc_ea(mode, pc_at_ext);
                    match side {
                        EaSide::Src => {
                            self.recipe_src_addr = addr;
                            self.recipe_src_is_reg = is_reg;
                        }
                        EaSide::Dst => {
                            self.recipe_dst_addr = addr;
                            self.recipe_dst_is_reg = is_reg;
                        }
                    }
                }
                RecipeOp::ReadEa(side) => {
                    let mode = match side {
                        EaSide::Src => self.src_mode,
                        EaSide::Dst => self.dst_mode,
                    };
                    let Some(mode) = mode else {
                        self.recipe_reset();
                        return;
                    };
                    if matches!(mode, AddrMode::Immediate) {
                        let value = self.read_immediate();
                        match side {
                            EaSide::Src => self.data = value,
                            EaSide::Dst => self.data2 = value,
                        }
                        continue;
                    }

                    let (is_reg, addr) = match side {
                        EaSide::Src => (self.recipe_src_is_reg, self.recipe_src_addr),
                        EaSide::Dst => (self.recipe_dst_is_reg, self.recipe_dst_addr),
                    };

                    if is_reg {
                        let value = match mode {
                            AddrMode::DataReg(r) => self.read_data_reg(r, self.size),
                            AddrMode::AddrReg(r) => {
                                let val = self.regs.a(r as usize);
                                match self.size {
                                    Size::Byte => val & 0xFF,
                                    Size::Word => val & 0xFFFF,
                                    Size::Long => val,
                                }
                            }
                            _ => 0,
                        };
                        match side {
                            EaSide::Src => self.data = value,
                            EaSide::Dst => self.data2 = value,
                        }
                        continue;
                    }

                    self.addr = addr;
                    self.queue_read_ops(self.size);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::WriteEa(side) => {
                    let mode = match side {
                        EaSide::Src => self.src_mode,
                        EaSide::Dst => self.dst_mode,
                    };
                    let Some(mode) = mode else {
                        self.recipe_reset();
                        return;
                    };
                    let (is_reg, addr) = match side {
                        EaSide::Src => (self.recipe_src_is_reg, self.recipe_src_addr),
                        EaSide::Dst => (self.recipe_dst_is_reg, self.recipe_dst_addr),
                    };
                    if matches!(mode, AddrMode::DataReg(_) | AddrMode::AddrReg(_)) || is_reg {
                        match mode {
                            AddrMode::DataReg(r) => {
                                self.write_data_reg(r, self.data, self.size);
                            }
                            AddrMode::AddrReg(r) => {
                                let value = match self.size {
                                    Size::Word => self.data as i16 as i32 as u32,
                                    Size::Long => self.data,
                                    Size::Byte => self.data & 0xFF,
                                };
                                self.regs.set_a(r as usize, value);
                            }
                            _ => {}
                        }
                        continue;
                    }

                    self.addr = addr;
                    self.queue_write_ops(self.size);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::LoadImm => {
                    self.data = self.recipe_imm;
                }
                RecipeOp::LoadEaAddr(side) => {
                    self.data = match side {
                        EaSide::Src => self.recipe_src_addr,
                        EaSide::Dst => self.recipe_dst_addr,
                    };
                }
                RecipeOp::SetFlagsMove => {
                    self.set_flags_move(self.data, self.size);
                }
                RecipeOp::Internal(cycles) => {
                    if cycles == 0 {
                        // Zero internal cycles — instant, continue to next recipe op.
                        continue;
                    }
                    self.internal_cycles = cycles;
                    self.micro_ops.push(MicroOp::Internal);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::PushLong => {
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::SkipExt(count) => {
                    let max = self.ext_count;
                    let next = self.ext_idx.saturating_add(count);
                    self.ext_idx = if next > max { max } else { next };
                }
                RecipeOp::MovemToMem => {
                    if self.recipe_setup_movem_to_mem() {
                        self.micro_ops.push(MicroOp::RecipeStep);
                        return;
                    }
                    self.recipe_reset();
                    return;
                }
                RecipeOp::MovemFromMem => {
                    if self.recipe_setup_movem_from_mem() {
                        self.micro_ops.push(MicroOp::RecipeStep);
                        return;
                    }
                    self.recipe_reset();
                    return;
                }
                RecipeOp::AluReg { op, reg } => {
                    let src = self.data;
                    let dst = self.read_data_reg(reg, self.size);
                    let mask = self.size.mask();
                    let result = match op {
                        RecipeAlu::Add => dst.wrapping_add(src) & mask,
                        RecipeAlu::Sub => dst.wrapping_sub(src) & mask,
                        RecipeAlu::And => (dst & src) & mask,
                        RecipeAlu::Or => (dst | src) & mask,
                        RecipeAlu::Eor => (dst ^ src) & mask,
                    };

                    match op {
                        RecipeAlu::Add => self.set_flags_add(src, dst, result, self.size),
                        RecipeAlu::Sub => self.set_flags_sub(src, dst, result, self.size),
                        RecipeAlu::And | RecipeAlu::Or | RecipeAlu::Eor => {
                            self.set_flags_move(result, self.size);
                        }
                    }

                    self.write_data_reg(reg, result, self.size);
                }
                RecipeOp::AluMem { op } => {
                    let src = self.data;
                    let dst = self.data2;
                    let mask = self.size.mask();
                    let result = match op {
                        RecipeAlu::Add => dst.wrapping_add(src) & mask,
                        RecipeAlu::Sub => dst.wrapping_sub(src) & mask,
                        RecipeAlu::And => (dst & src) & mask,
                        RecipeAlu::Or => (dst | src) & mask,
                        RecipeAlu::Eor => (dst ^ src) & mask,
                    };

                    match op {
                        RecipeAlu::Add => self.set_flags_add(src, dst, result, self.size),
                        RecipeAlu::Sub => self.set_flags_sub(src, dst, result, self.size),
                        RecipeAlu::And | RecipeAlu::Or | RecipeAlu::Eor => {
                            self.set_flags_move(result, self.size);
                        }
                    }

                    self.data = result;
                }
                RecipeOp::CmpReg { reg, addr } => {
                    let src = self.data;
                    let (src_ext, dst, size) = if addr {
                        let src_ext = if self.size == Size::Word {
                            src as i16 as i32 as u32
                        } else {
                            src
                        };
                        (src_ext, self.regs.a(reg as usize), Size::Long)
                    } else {
                        (src, self.read_data_reg(reg, self.size), self.size)
                    };
                    let result = dst.wrapping_sub(src_ext);
                    self.set_flags_cmp(src_ext, dst, result, size);
                }
                RecipeOp::AddrArith { reg, add } => {
                    let src = if self.size == Size::Word {
                        self.data as i16 as i32 as u32
                    } else {
                        self.data
                    };
                    let dst = self.regs.a(reg as usize);
                    let result = if add {
                        dst.wrapping_add(src)
                    } else {
                        dst.wrapping_sub(src)
                    };
                    self.regs.set_a(reg as usize, result);
                }
                RecipeOp::ReadBitImm { reg } => {
                    let word = self.next_ext_word();
                    let mask = if reg { 31 } else { 7 };
                    self.data = u32::from(word & mask);
                }
                RecipeOp::ReadBitReg { reg, mem } => {
                    let mask = if mem { 7 } else { 31 };
                    self.data = self.regs.d[reg as usize] & mask;
                }
                RecipeOp::BitReg { reg, op } => {
                    let bit = (self.data & 31) as u8;
                    let mask = 1u32 << bit;
                    let value = self.regs.d[reg as usize];
                    let was_zero = value & mask == 0;
                    self.regs.sr = Status::set_if(self.regs.sr, Z, was_zero);

                    match op {
                        1 => self.regs.d[reg as usize] = value ^ mask,
                        2 => self.regs.d[reg as usize] = value & !mask,
                        3 => self.regs.d[reg as usize] = value | mask,
                        _ => {}
                    }
                }
                RecipeOp::BitMem { op } => {
                    self.addr = self.recipe_src_addr;
                    self.data2 = u32::from(op);
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::BitMemOp);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::CmpEa => {
                    let src = self.data;
                    let dst = self.data2;
                    let result = dst.wrapping_sub(src);
                    self.set_flags_cmp(src, dst, result, self.size);
                }
                RecipeOp::Scc { condition } => {
                    let value = if Status::condition(self.regs.sr, condition) {
                        0xFFu32
                    } else {
                        0x00u32
                    };
                    match self.dst_mode {
                        Some(AddrMode::DataReg(r)) => {
                            self.regs.d[r as usize] =
                                (self.regs.d[r as usize] & 0xFFFF_FF00) | value;
                            self.internal_cycles = if value != 0 { 6 } else { 4 };
                            self.micro_ops.push(MicroOp::Internal);
                            return;
                        }
                        Some(AddrMode::AddrReg(_)) | Some(AddrMode::Immediate) | None => {
                            self.recipe_reset();
                            return;
                        }
                        _ => {
                            self.data = value;
                            self.addr = self.recipe_dst_addr;
                            self.queue_write_ops(Size::Byte);
                            self.micro_ops.push(MicroOp::RecipeStep);
                            return;
                        }
                    }
                }
                RecipeOp::Dbcc { condition, reg } => {
                    let disp = self.next_ext_word() as i16 as i32;

                    if Status::condition(self.regs.sr, condition) {
                        self.queue_internal_no_pc(12);
                        return;
                    }

                    let reg = reg as usize;
                    let val = (self.regs.d[reg] & 0xFFFF) as i16;
                    let new_val = val.wrapping_sub(1);

                    if new_val == -1 {
                        self.regs.d[reg] =
                            (self.regs.d[reg] & 0xFFFF_0000) | (new_val as u16 as u32);
                        self.queue_internal_no_pc(14);
                        return;
                    }

                    let target = ((self.regs.pc as i32) - 2 + disp) as u32;
                    if target & 1 != 0 {
                        self.exception_pc_override = Some(self.regs.pc);
                        self.fault_fc = if self.regs.sr & S != 0 { 6 } else { 2 };
                        self.fault_addr = target;
                        self.fault_read = true;
                        self.fault_in_instruction = false;
                        self.exception(3);
                        return;
                    }

                    self.regs.d[reg] =
                        (self.regs.d[reg] & 0xFFFF_0000) | (new_val as u16 as u32);
                    self.regs.pc = target;
                    self.queue_internal_no_pc(10);
                    return;
                }
                RecipeOp::Mul { signed, reg } => {
                    let src = self.data & 0xFFFF;
                    let dst = self.regs.d[reg as usize] & 0xFFFF;
                    let result = if signed {
                        let src_signed = (src as i16) as i32;
                        let dst_signed = (dst as i16) as i32;
                        (src_signed * dst_signed) as u32
                    } else {
                        src.wrapping_mul(dst)
                    };

                    self.regs.d[reg as usize] = result;
                    self.regs.sr = Status::clear_vc(self.regs.sr);
                    self.regs.sr = Status::update_nz_long(self.regs.sr, result);

                    let timing = if signed {
                        let src16 = src as u16;
                        let pattern = src16 ^ (src16 << 1);
                        let ones = pattern.count_ones() as u8;
                        38 + 2 * ones
                    } else {
                        let ones = (src as u16).count_ones() as u8;
                        38 + 2 * ones
                    };

                    let mem_src = !matches!(
                        self.src_mode,
                        Some(AddrMode::DataReg(_)) | Some(AddrMode::Immediate)
                    );
                    let timing = if mem_src {
                        timing.saturating_sub(4)
                    } else {
                        timing
                    };
                    self.queue_internal(timing);
                    return;
                }
                RecipeOp::Div { signed, reg } => {
                    let divisor = self.data & 0xFFFF;
                    if divisor == 0 {
                        self.exception(5);
                        return;
                    }

                    let timing = if signed {
                        let divisor_signed = (divisor as i16) as i32;
                        let dividend = self.regs.d[reg as usize] as i32;
                        let quotient = dividend / divisor_signed;
                        let remainder = dividend % divisor_signed;
                        let timing = Self::divs_cycles(dividend, divisor as i16);

                        if !(-32768..=32767).contains(&quotient) {
                            self.regs.sr |= V;
                            self.regs.sr &= !C;
                            self.regs.sr |= N;
                            self.regs.sr &= !Z;
                        } else {
                            let q = quotient as i16 as u16 as u32;
                            let r = remainder as i16 as u16 as u32;
                            self.regs.d[reg as usize] = (r << 16) | q;
                            self.regs.sr &= !(V | C);
                            self.regs.sr = Status::set_if(self.regs.sr, Z, quotient == 0);
                            self.regs.sr = Status::set_if(self.regs.sr, N, quotient < 0);
                        }

                        timing
                    } else {
                        let dividend = self.regs.d[reg as usize];
                        let quotient = dividend / divisor;
                        let remainder = dividend % divisor;
                        let timing = Self::divu_cycles(dividend, divisor as u16);

                        if quotient > 0xFFFF {
                            self.regs.sr |= V;
                            self.regs.sr &= !C;
                            self.regs.sr |= N;
                            self.regs.sr &= !Z;
                        } else {
                            self.regs.d[reg as usize] = (remainder << 16) | quotient;
                            self.regs.sr &= !(V | C);
                            self.regs.sr =
                                Status::set_if(self.regs.sr, Z, quotient == 0);
                            self.regs.sr =
                                Status::set_if(self.regs.sr, N, quotient & 0x8000 != 0);
                        }

                        timing
                    };

                    let mem_src = !matches!(
                        self.src_mode,
                        Some(AddrMode::DataReg(_)) | Some(AddrMode::Immediate)
                    );
                    let timing = if mem_src {
                        timing.saturating_sub(4)
                    } else {
                        timing
                    };
                    self.queue_internal(timing);
                    return;
                }
                RecipeOp::Exg { kind, rx, ry } => {
                    let rx = rx as usize;
                    let ry = ry as usize;
                    match kind {
                        0x08 => {
                            let tmp = self.regs.d[rx];
                            self.regs.d[rx] = self.regs.d[ry];
                            self.regs.d[ry] = tmp;
                        }
                        0x09 => {
                            let tmp = self.regs.a(rx);
                            self.regs.set_a(rx, self.regs.a(ry));
                            self.regs.set_a(ry, tmp);
                        }
                        0x11 => {
                            let tmp = self.regs.d[rx];
                            self.regs.d[rx] = self.regs.a(ry);
                            self.regs.set_a(ry, tmp);
                        }
                        _ => {
                            self.recipe_reset();
                            return;
                        }
                    }
                    self.queue_internal(6);
                    return;
                }
                RecipeOp::AbcdReg { src, dst } => {
                    let src = self.regs.d[src as usize] as u8;
                    let dst_reg = dst as usize;
                    let dst_val = self.regs.d[dst_reg] as u8;
                    let x = u8::from(self.regs.sr & X != 0);

                    let (result, carry, overflow) = self.bcd_add(src, dst_val, x);

                    self.regs.d[dst_reg] =
                        (self.regs.d[dst_reg] & 0xFFFF_FF00) | u32::from(result);

                    let mut sr = self.regs.sr;
                    if result != 0 { sr &= !Z; }
                    sr = Status::set_if(sr, C, carry);
                    sr = Status::set_if(sr, X, carry);
                    sr = Status::set_if(sr, N, result & 0x80 != 0);
                    sr = Status::set_if(sr, V, overflow);
                    self.regs.sr = sr;

                    self.queue_internal(6);
                    return;
                }
                RecipeOp::SbcdReg { src, dst } => {
                    let src = self.regs.d[src as usize] as u8;
                    let dst_reg = dst as usize;
                    let dst_val = self.regs.d[dst_reg] as u8;
                    let x = u8::from(self.regs.sr & X != 0);

                    let (result, borrow, overflow) = self.bcd_sub(dst_val, src, x);

                    self.regs.d[dst_reg] =
                        (self.regs.d[dst_reg] & 0xFFFF_FF00) | u32::from(result);

                    let mut sr = self.regs.sr;
                    if result != 0 { sr &= !Z; }
                    sr = Status::set_if(sr, C, borrow);
                    sr = Status::set_if(sr, X, borrow);
                    sr = Status::set_if(sr, N, result & 0x80 != 0);
                    sr = Status::set_if(sr, V, overflow);
                    self.regs.sr = sr;

                    self.queue_internal(6);
                    return;
                }
                RecipeOp::Cmpm { ax, ay } => {
                    self.addr = self.regs.a(ay as usize);
                    self.addr2 = self.regs.a(ax as usize);
                    self.data = u32::from(ay);
                    self.data2 = u32::from(ax);
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::CmpmExecute);
                    self.queue_internal(4);
                    return;
                }
                RecipeOp::Branch { condition, disp8 } => {
                    let is_word = disp8 == 0;
                    let disp = if is_word {
                        self.next_ext_word() as i16 as i32
                    } else {
                        i32::from(disp8)
                    };
                    let base_pc = if is_word {
                        self.regs.pc.wrapping_sub(2)
                    } else {
                        self.regs.pc
                    };
                    let target = (base_pc as i32).wrapping_add(disp) as u32;

                    match condition {
                        0 => {
                            // BRA
                            if target & 1 != 0 {
                                self.trigger_branch_address_error(target);
                                return;
                            }
                            self.set_jump_pc(target);
                            self.internal_cycles = if is_word { 10 } else { 4 };
                            self.micro_ops.push(MicroOp::Internal);
                            return;
                        }
                        1 => {
                            // BSR
                            self.data = self.regs.pc;
                            if target & 1 != 0 {
                                if is_word {
                                    let sp = self.regs.active_sp().wrapping_sub(4);
                                    self.regs.set_active_sp(sp);
                                    self.trigger_branch_address_error(target);
                                } else {
                                    self.micro_ops.push(MicroOp::PushLongHi);
                                    self.micro_ops.push(MicroOp::PushLongLo);
                                    self.addr2 = target;
                                    self.data2 = 0x8000_0001;
                                    self.internal_cycles = 0;
                                    self.micro_ops.push(MicroOp::Internal);
                                }
                                return;
                            }

                            self.micro_ops.push(MicroOp::PushLongHi);
                            self.micro_ops.push(MicroOp::PushLongLo);
                            self.set_jump_pc(target);
                            self.internal_cycles = if is_word { 4 } else { 10 };
                            self.micro_ops.push(MicroOp::Internal);
                            return;
                        }
                        _ => {
                            // Bcc
                            if !Status::condition(self.regs.sr, condition) {
                                self.internal_cycles = 8;
                                self.micro_ops.push(MicroOp::Internal);
                                return;
                            }
                            if target & 1 != 0 {
                                self.trigger_branch_address_error(target);
                                return;
                            }
                            self.set_jump_pc(target);
                            self.internal_cycles = 10;
                            self.micro_ops.push(MicroOp::Internal);
                            return;
                        }
                    }
                }
                RecipeOp::Jsr => {
                    let Some(mode) = self.src_mode else {
                        self.recipe_reset();
                        return;
                    };
                    let target = self.recipe_src_addr;
                    let cycles = match mode {
                        AddrMode::AddrInd(_) => 8,
                        AddrMode::AddrIndDisp(_) => 10,
                        AddrMode::AddrIndIndex(_) => 14,
                        AddrMode::AbsShort => 10,
                        AddrMode::AbsLong => 12,
                        AddrMode::PcDisp => 10,
                        AddrMode::PcIndex => 14,
                        _ => {
                            self.recipe_reset();
                            return;
                        }
                    };

                    if target & 1 != 0 {
                        self.trigger_jsr_address_error(target);
                        return;
                    }

                    self.data = self.regs.pc;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.set_jump_pc(target);
                    self.internal_cycles = cycles;
                    self.micro_ops.push(MicroOp::Internal);
                    return;
                }
                RecipeOp::Jmp => {
                    let Some(mode) = self.src_mode else {
                        self.recipe_reset();
                        return;
                    };
                    let target = self.recipe_src_addr;
                    let cycles = match mode {
                        AddrMode::AddrInd(_) => 8,
                        AddrMode::AddrIndDisp(_) => 10,
                        AddrMode::AddrIndIndex(_) => 14,
                        AddrMode::AbsShort => 10,
                        AddrMode::AbsLong => 12,
                        AddrMode::PcDisp => 10,
                        AddrMode::PcIndex => 14,
                        _ => {
                            self.recipe_reset();
                            return;
                        }
                    };

                    if target & 1 != 0 {
                        self.trigger_jmp_address_error(target);
                        return;
                    }

                    self.set_jump_pc(target);
                    self.internal_cycles = cycles;
                    self.micro_ops.push(MicroOp::Internal);
                    return;
                }
                RecipeOp::RtsPop => {
                    self.micro_ops.push(MicroOp::PopLongHi);
                    self.micro_ops.push(MicroOp::PopLongLo);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::RtsFinish => {
                    if self.data & 1 != 0 {
                        self.trigger_rts_address_error(self.data);
                        return;
                    }
                    self.regs.pc = self.data;
                    self.queue_internal_no_pc(8);
                    return;
                }
                RecipeOp::RtePopSr => {
                    if !self.regs.is_supervisor() {
                        self.exception(8);
                        return;
                    }
                    self.micro_ops.push(MicroOp::PopWord);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::RtePopPc => {
                    self.data2 = self.data;
                    self.micro_ops.push(MicroOp::PopLongHi);
                    self.micro_ops.push(MicroOp::PopLongLo);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::RteFinish => {
                    self.regs.sr = (self.data2 as u16) & flags::SR_MASK;
                    if self.data & 1 != 0 {
                        self.trigger_rte_address_error(self.data);
                        return;
                    }
                    self.regs.pc = self.data;
                    self.queue_internal_no_pc(8);
                    return;
                }
                RecipeOp::RtrPopCcr => {
                    self.micro_ops.push(MicroOp::PopWord);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::RtrPopPc => {
                    self.data2 = self.data;
                    self.micro_ops.push(MicroOp::PopLongHi);
                    self.micro_ops.push(MicroOp::PopLongLo);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::RtrFinish => {
                    let ccr = (self.data2 & 0x1F) as u8;
                    self.regs.set_ccr(ccr);
                    if self.data & 1 != 0 {
                        self.trigger_rtr_address_error(self.data);
                        return;
                    }
                    self.regs.pc = self.data;
                    self.queue_internal_no_pc(8);
                    return;
                }
                RecipeOp::LinkStart { reg } => {
                    let disp = self.next_ext_word() as i16 as i32;
                    let reg = reg as usize;
                    let an_value = self.regs.a(reg);
                    self.data = an_value;
                    self.micro_ops.push(MicroOp::PushLongHi);
                    self.micro_ops.push(MicroOp::PushLongLo);
                    self.addr2 = reg as u32;
                    self.data2 = disp as u32;
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::LinkFinish => {
                    let reg = self.addr2 as usize;
                    let disp = self.data2 as i32;
                    let sp = self.regs.active_sp();
                    self.regs.set_a(reg, sp);
                    let new_sp = (sp as i32).wrapping_add(disp) as u32;
                    self.regs.set_active_sp(new_sp);
                    self.queue_internal_no_pc(8);
                    return;
                }
                RecipeOp::UnlkStart { reg } => {
                    let reg = reg as usize;
                    let an_value = self.regs.a(reg);
                    if an_value & 1 != 0 {
                        self.fault_addr = an_value;
                        self.fault_fc = if self.regs.is_supervisor() { 5 } else { 1 };
                        self.fault_read = true;
                        self.fault_in_instruction = false;
                        self.exception_pc_override = Some(self.regs.pc);
                        self.exception(3);
                        return;
                    }
                    self.regs.set_active_sp(an_value);
                    self.micro_ops.push(MicroOp::PopLongHi);
                    self.micro_ops.push(MicroOp::PopLongLo);
                    self.addr2 = reg as u32;
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::UnlkFinish => {
                    let reg = self.addr2 as usize;
                    self.regs.set_a(reg, self.data);
                    self.queue_internal_no_pc(4);
                    return;
                }
                RecipeOp::MoveUsp { reg, to_usp } => {
                    if !self.regs.is_supervisor() {
                        self.exception(8);
                        return;
                    }
                    let reg = reg as usize;
                    if to_usp {
                        self.regs.usp = self.regs.a(reg);
                    } else {
                        let usp = self.regs.usp;
                        self.regs.set_a(reg, usp);
                    }
                    self.queue_internal(4);
                    return;
                }
                RecipeOp::Trap { vector } => {
                    self.exception(vector);
                    return;
                }
                RecipeOp::Trapv => {
                    if self.regs.sr & V != 0 {
                        self.exception(7);
                    } else {
                        self.queue_internal(4);
                    }
                    return;
                }
                RecipeOp::Reset => {
                    if !self.regs.is_supervisor() {
                        self.exception(8);
                        return;
                    }
                    self.micro_ops.push(MicroOp::ResetBus);
                    self.queue_internal(132);
                    return;
                }
                RecipeOp::Stop => {
                    if !self.regs.is_supervisor() {
                        self.exception(8);
                        return;
                    }
                    let imm = self.next_ext_word();
                    self.regs.sr = imm & flags::SR_MASK;
                    self.state = super::State::Stopped;
                    return;
                }
                RecipeOp::WriteCcr => {
                    let ccr = (self.data as u16) & 0x1F;
                    self.regs.sr = (self.regs.sr & 0xFF00) | ccr;
                }
                RecipeOp::WriteSr => {
                    self.regs.sr = (self.data as u16) & flags::SR_MASK;
                }
                RecipeOp::LogicCcr { op } => {
                    let imm = (self.data as u8) & 0x1F;
                    let ccr = self.regs.ccr() & 0x1F;
                    let result = match op {
                        RecipeAlu::And => ccr & imm,
                        RecipeAlu::Or => ccr | imm,
                        RecipeAlu::Eor => ccr ^ imm,
                        _ => ccr,
                    };
                    self.regs.set_ccr(result);
                    self.queue_internal(20);
                    return;
                }
                RecipeOp::LogicSr { op } => {
                    if !self.regs.is_supervisor() {
                        self.exception(8);
                        return;
                    }
                    let imm = self.data as u16;
                    let sr = self.regs.sr;
                    let result = match op {
                        RecipeAlu::And => sr & imm,
                        RecipeAlu::Or => sr | imm,
                        RecipeAlu::Eor => sr ^ imm,
                        _ => sr,
                    };
                    self.regs.sr = result & flags::SR_MASK;
                    self.queue_internal(20);
                    return;
                }
                RecipeOp::AddxReg { src, dst } => {
                    let src = self.read_data_reg(src, self.size);
                    let dst_val = self.read_data_reg(dst, self.size);
                    let x = u32::from(self.regs.sr & X != 0);

                    let result = dst_val.wrapping_add(src).wrapping_add(x);
                    self.write_data_reg(dst, result, self.size);

                    let (src_masked, dst_masked, result_masked, msb) = match self.size {
                        Size::Byte => (src & 0xFF, dst_val & 0xFF, result & 0xFF, 0x80u32),
                        Size::Word => (src & 0xFFFF, dst_val & 0xFFFF, result & 0xFFFF, 0x8000),
                        Size::Long => (src, dst_val, result, 0x8000_0000),
                    };

                    let mut sr = self.regs.sr;
                    sr = Status::set_if(sr, N, result_masked & msb != 0);
                    if result_masked != 0 { sr &= !Z; }
                    let overflow =
                        (!(src_masked ^ dst_masked) & (src_masked ^ result_masked) & msb) != 0;
                    sr = Status::set_if(sr, V, overflow);
                    let carry = match self.size {
                        Size::Byte => {
                            (u16::from(src as u8) + u16::from(dst_val as u8) + u16::from(x as u8))
                                > 0xFF
                        }
                        Size::Word => {
                            (u32::from(src as u16) + u32::from(dst_val as u16) + x) > 0xFFFF
                        }
                        Size::Long => src
                            .checked_add(dst_val)
                            .and_then(|v| v.checked_add(x))
                            .is_none(),
                    };
                    sr = Status::set_if(sr, C, carry);
                    sr = Status::set_if(sr, X, carry);
                    self.regs.sr = sr;
                }
                RecipeOp::SubxReg { src, dst } => {
                    let src = self.read_data_reg(src, self.size);
                    let dst_val = self.read_data_reg(dst, self.size);
                    let x = u32::from(self.regs.sr & X != 0);

                    let result = dst_val.wrapping_sub(src).wrapping_sub(x);
                    self.write_data_reg(dst, result, self.size);

                    let (src_masked, dst_masked, result_masked, msb) = match self.size {
                        Size::Byte => (src & 0xFF, dst_val & 0xFF, result & 0xFF, 0x80u32),
                        Size::Word => (src & 0xFFFF, dst_val & 0xFFFF, result & 0xFFFF, 0x8000),
                        Size::Long => (src, dst_val, result, 0x8000_0000),
                    };

                    let mut sr = self.regs.sr;
                    sr = Status::set_if(sr, N, result_masked & msb != 0);
                    if result_masked != 0 { sr &= !Z; }
                    let overflow =
                        ((dst_masked ^ src_masked) & (dst_masked ^ result_masked) & msb) != 0;
                    sr = Status::set_if(sr, V, overflow);
                    let carry = src_masked.wrapping_add(x) > dst_masked
                        || (src_masked == dst_masked && x != 0);
                    sr = Status::set_if(sr, C, carry);
                    sr = Status::set_if(sr, X, carry);
                    self.regs.sr = sr;
                }
                RecipeOp::UnaryReg { op, reg } => {
                    let value = self.read_data_reg(reg, self.size);
                    let result = match op {
                        RecipeUnary::Neg => {
                            let res = 0u32.wrapping_sub(value);
                            self.set_flags_sub(value, 0, res, self.size);
                            res
                        }
                        RecipeUnary::Not => {
                            let res = !value;
                            self.set_flags_move(res, self.size);
                            res
                        }
                        RecipeUnary::Negx => {
                            let x = u32::from(self.regs.sr & X != 0);
                            let res = 0u32.wrapping_sub(value).wrapping_sub(x);

                            let (src_masked, result_masked, msb) = match self.size {
                                Size::Byte => (value & 0xFF, res & 0xFF, 0x80u32),
                                Size::Word => (value & 0xFFFF, res & 0xFFFF, 0x8000),
                                Size::Long => (value, res, 0x8000_0000),
                            };

                            let mut sr = self.regs.sr;
                            sr = Status::set_if(sr, N, result_masked & msb != 0);
                            if result_masked != 0 { sr &= !Z; }
                            let overflow =
                                (src_masked & msb) != 0 && (result_masked & msb) != 0;
                            sr = Status::set_if(sr, V, overflow);
                            let carry = src_masked != 0 || x != 0;
                            sr = Status::set_if(sr, C, carry);
                            sr = Status::set_if(sr, X, carry);
                            self.regs.sr = sr;

                            res
                        }
                    };
                    self.write_data_reg(reg, result, self.size);
                }
                RecipeOp::SwapReg { reg } => {
                    let value = self.regs.d[reg as usize];
                    let swapped = (value >> 16) | (value << 16);
                    self.regs.d[reg as usize] = swapped;
                    self.set_flags_move(swapped, Size::Long);
                }
                RecipeOp::Ext { size, reg } => {
                    let value = match size {
                        Size::Word => {
                            let byte = self.regs.d[reg as usize] as i8 as i16 as u16;
                            self.regs.d[reg as usize] =
                                (self.regs.d[reg as usize] & 0xFFFF_0000) | u32::from(byte);
                            u32::from(byte)
                        }
                        Size::Long => {
                            let word = self.regs.d[reg as usize] as i16 as i32 as u32;
                            self.regs.d[reg as usize] = word;
                            word
                        }
                        Size::Byte => 0,
                    };
                    self.set_flags_move(value, size);
                }
                RecipeOp::UnaryMem { op } => {
                    self.addr = self.recipe_dst_addr;
                    self.data2 = u32::from(op.to_alu_mem_rmw());
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::AluMemRmw);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::ShiftReg {
                    kind,
                    direction,
                    count_or_reg,
                    reg,
                    immediate,
                } => {
                    self.exec_shift_reg(kind, direction, count_or_reg, reg, Some(self.size), immediate);
                }
                RecipeOp::ShiftMem { kind, direction } => {
                    self.addr = self.recipe_dst_addr;
                    self.data = u32::from(kind);
                    self.data2 = if direction { 1 } else { 0 };
                    self.movem_long_phase = 0;
                    self.micro_ops.push(MicroOp::ShiftMemExecute);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
                RecipeOp::ExtendMem { op, src, dst } => {
                    self.data = u32::from(src) | (u32::from(dst) << 8);
                    self.data2 = u32::from(op);
                    self.movem_long_phase = 0;
                    self.extend_predec_done = false;
                    self.src_mode = Some(AddrMode::AddrIndPreDec(src));
                    self.dst_mode = Some(AddrMode::AddrIndPreDec(dst));
                    self.micro_ops.push(MicroOp::ExtendMemOp);
                    self.micro_ops.push(MicroOp::RecipeStep);
                    return;
                }
            }
        }
        self.recipe_reset();
    }

    /// Setup MOVEM registers-to-memory transfer.
    pub(super) fn recipe_setup_movem_to_mem(&mut self) -> bool {
        let mask = self.ext_words[0];
        if mask == 0 {
            self.queue_internal(8);
            return true;
        }

        let Some(addr_mode) = self.src_mode else {
            return false;
        };

        self.program_space_access = matches!(addr_mode, AddrMode::PcDisp | AddrMode::PcIndex);

        let is_predec = matches!(addr_mode, AddrMode::AddrIndPreDec(_));
        self.movem_predec = is_predec;
        self.movem_postinc = false;
        self.movem_long_phase = 0;

        let (start_addr, ea_reg) = match addr_mode {
            AddrMode::AddrIndPreDec(r) => {
                let dec_per_reg = if self.size == Size::Long { 4 } else { 2 };
                let count = mask.count_ones();
                let start = self.regs.a(r as usize).wrapping_sub(count * dec_per_reg);
                (start, r)
            }
            AddrMode::AddrInd(r) => (self.regs.a(r as usize), r),
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                let base = self.regs.a(r as usize) as i32;
                (base.wrapping_add(disp) as u32, r)
            }
            AddrMode::AbsShort => {
                let addr = self.next_ext_word() as i16 as i32 as u32;
                (addr, 0)
            }
            AddrMode::AbsLong => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                ((u32::from(hi) << 16) | u32::from(lo), 0)
            }
            AddrMode::AddrIndIndex(r) => {
                let base = self.regs.a(r as usize);
                (self.calc_index_ea(base), r)
            }
            _ => return false,
        };

        self.addr = start_addr;
        self.addr2 = u32::from(ea_reg);

        let first_bit = if is_predec {
            self.find_first_movem_bit_down(mask)
        } else {
            self.find_first_movem_bit_up(mask)
        };

        if let Some(bit) = first_bit {
            self.data2 = bit as u32;
            self.micro_ops.push(MicroOp::MovemWrite);
        }

        true
    }

    /// Setup MOVEM memory-to-registers transfer.
    pub(super) fn recipe_setup_movem_from_mem(&mut self) -> bool {
        let mask = self.ext_words[0];
        if mask == 0 {
            self.queue_internal(12);
            return true;
        }

        let Some(addr_mode) = self.dst_mode else {
            return false;
        };

        self.program_space_access = matches!(addr_mode, AddrMode::PcDisp | AddrMode::PcIndex);

        let is_postinc = matches!(addr_mode, AddrMode::AddrIndPostInc(_));
        self.movem_predec = false;
        self.movem_postinc = is_postinc;
        self.movem_long_phase = 0;

        let (start_addr, ea_reg) = match addr_mode {
            AddrMode::AddrIndPostInc(r) => (self.regs.a(r as usize), r),
            AddrMode::AddrInd(r) => (self.regs.a(r as usize), r),
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                let base = self.regs.a(r as usize) as i32;
                (base.wrapping_add(disp) as u32, r)
            }
            AddrMode::AbsShort => {
                let addr = self.next_ext_word() as i16 as i32 as u32;
                (addr, 0)
            }
            AddrMode::AbsLong => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                ((u32::from(hi) << 16) | u32::from(lo), 0)
            }
            AddrMode::AddrIndIndex(r) => {
                let base = self.regs.a(r as usize);
                (self.calc_index_ea(base), r)
            }
            AddrMode::PcDisp => {
                let base_pc = self.recipe_dst_pc_at_ext;
                let disp = self.next_ext_word() as i16 as i32;
                ((base_pc as i32).wrapping_add(disp) as u32, 0)
            }
            AddrMode::PcIndex => {
                let base_pc = self.recipe_dst_pc_at_ext;
                (self.calc_index_ea(base_pc), 0)
            }
            _ => return false,
        };

        self.addr = start_addr;
        self.addr2 = u32::from(ea_reg);

        let first_bit = self.find_first_movem_bit_up(mask);
        if let Some(bit) = first_bit {
            self.data2 = bit as u32;
            self.micro_ops.push(MicroOp::MovemRead);
        }

        true
    }
}
