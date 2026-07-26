//! ALU execution and result writeback for the 68000.
//!
//! After operands are fetched, the execute stage runs the ALU operation
//! and the writeback stage stores the result. These are separate follow-up
//! tags because some instructions (like CMP) execute but don't write back.

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::cpu::{AluOp, BitOp, Cpu68000};
use crate::microcode::MicroOp;

impl Cpu68000 {
    /// Execute the ALU operation for the current instruction.
    ///
    /// Called at TAG_EXECUTE after both source and destination operands
    /// are available in `src_val` and `dst_val`.
    ///
    /// Sets `self.data` to the result for writeback. Updates flags in SR.
    pub fn perform_execute(&mut self) {
        let opcode = self.ir;

        // MOVE: copy source to data, set flags (unless MOVEA)
        if (opcode & 0xC000) == 0 && (opcode & 0x3000) != 0 {
            self.data = self.src_val;
            let dst = self
                .dst_mode
                .expect("destination mode should be resolved before execute/writeback");
            if !matches!(dst, AddrMode::AddrReg(_)) {
                self.set_flags_move(self.data, self.size);
            }
            return;
        }

        // LEA: result is the computed address
        if (opcode & 0xF1C0) == 0x41C0 {
            self.src_val = self.addr;
            return;
        }

        // PEA: result is the computed address (for push)
        if (opcode & 0xFFC0) == 0x4840 {
            self.src_val = self.addr;
            return;
        }

        // NBCD: negate BCD (0 - val - X)
        if (opcode & 0xFFC0) == 0x4800 {
            let val = self.dst_val as u8;
            let x = self.x_flag();
            match self.nbcd_op(val, x) {
                Some((result, carry, overflow)) => {
                    self.set_bcd_flags(result, carry, overflow);
                    self.data = u32::from(result);
                }
                None => {
                    // Musashi `pre == 0x9a` corner: destination
                    // unchanged. The bus writeback path will write
                    // `self.data`; set it to the original value so
                    // the write is a no-op.
                    self.data = u32::from(val);
                }
            }
            return;
        }

        // CLR: result is zero, set flags
        if (opcode & 0xFF00) == 0x4200 {
            self.data = 0;
            self.set_flags_move(0, self.size);
            return;
        }

        // TAS: test byte, set flags, then set bit 7 (must be before TST)
        if (opcode & 0xFFC0) == 0x4AC0 {
            let val = self.dst_val & 0xFF;
            self.set_flags_logic(val, Size::Byte);
            self.data = val | 0x80;
            return;
        }

        // TST: already read into dst_val, just set flags
        if (opcode & 0xFF00) == 0x4A00 {
            self.set_flags_move(self.dst_val, self.size);
            return;
        }

        // Scc: set byte to 0xFF if condition true, 0x00 if false
        if (opcode & 0xF0C0) == 0x50C0 && (opcode & 0x0038) != 0x0008 {
            let cond = ((opcode >> 8) & 0x0F) as u8;
            self.data = if self.check_condition(cond) {
                0xFF
            } else {
                0x00
            };
            return;
        }

        // NOT: ~dst
        if (opcode & 0xFF00) == 0x4600 && ((opcode >> 6) & 3) != 3 {
            let mask = self.size.mask();
            self.data = !self.dst_val & mask;
            self.set_flags_logic(self.data, self.size);
            return;
        }

        // NEG: 0 - dst
        if (opcode & 0xFF00) == 0x4400 && ((opcode >> 6) & 3) != 3 {
            let (result, new_sr) = crate::alu::neg(self.dst_val, self.size, self.regs.sr);
            self.regs.sr = new_sr;
            self.data = result;
            return;
        }

        // NEGX: 0 - dst - X
        if (opcode & 0xFF00) == 0x4000 && ((opcode >> 6) & 3) != 3 {
            let (result, new_sr) = crate::alu::negx(self.dst_val, self.size, self.regs.sr);
            self.regs.sr = new_sr;
            self.data = result;
            return;
        }

        // MOVE from SR to memory (RMW pattern: read then write)
        if (opcode & 0xFFC0) == 0x40C0 {
            // dst_val has the dummy read; we write SR
            self.data = u32::from(self.regs.sr);
            return;
        }

        // MOVE to CCR / MOVE to SR
        // The 68000 spends 8 internal clocks processing the SR/CCR write.
        // Don't set in_followup=false here — let the normal TAG_WRITEBACK
        // path complete the instruction so the instruction-end FetchIRC runs.
        if (opcode & 0xFFC0) == 0x44C0 || (opcode & 0xFFC0) == 0x46C0 {
            let val = self.src_val;
            if (opcode & 0xFFC0) == 0x44C0 {
                // MOVE to CCR: only bits 0-4
                self.regs.sr = (self.regs.sr & 0xFF00) | (val as u16 & 0x001F);
            } else {
                // MOVE to SR
                self.regs.sr = val as u16 & self.sr_write_mask();
            }
            let d: u8 = 8;
            if d > 0 {
                self.micro_ops.push(MicroOp::Internal(d));
            }
            return;
        }

        // Bit operations (memory destination)
        if (opcode & 0xF000) == 0x0000 {
            let is_dynamic = (opcode & 0x0100) != 0;
            let is_static = (opcode & 0xFF00) == 0x0800;
            if is_dynamic || is_static {
                let bit = self.src_val & 7; // byte: mod 8
                let val = self.dst_val & 0xFF;
                self.regs.sr = if val & (1 << bit) == 0 {
                    self.regs.sr | crate::flags::Z
                } else {
                    self.regs.sr & !crate::flags::Z
                };
                if matches!(self.bit_op, BitOp::Btst) {
                    // BTST: no writeback needed (Z flag already set above).
                    // Don't set in_followup=false here — let TAG_WRITEBACK
                    // handle the clean exit so the instruction-end FetchIRC runs.
                    return;
                }
                self.data = match self.bit_op {
                    BitOp::Btst => unreachable!(),
                    BitOp::Bchg => val ^ (1 << bit),
                    BitOp::Bclr => val & !(1 << bit),
                    BitOp::Bset => val | (1 << bit),
                };
                return;
            }
        }

        // Shift/rotate (memory destination): shift by 1
        if (opcode & 0xF000) == 0xE000 && ((opcode >> 6) & 3) == 3 {
            let shift_type = self.src_val as u8 & 0x0F;
            let direction = ((self.src_val >> 4) & 1) as u8;
            self.perform_shift_memory(direction, shift_type);
            return;
        }

        // ADDA/SUBA/CMPA: address register operations don't set flags
        // (except CMPA which sets flags on the subtraction).
        // The source is sign-extended from word to long before the operation,
        // and the full 32-bit result is written to the address register.
        if matches!(self.dst_mode, Some(AddrMode::AddrReg(_))) {
            let src = if self.size == Size::Word {
                self.src_val as i16 as i32 as u32
            } else {
                self.src_val
            };
            match self.alu_op {
                AluOp::Add => self.data = self.dst_val.wrapping_add(src),
                AluOp::Sub => self.data = self.dst_val.wrapping_sub(src),
                AluOp::Cmp => {
                    // CMPA sets flags but doesn't write back
                    self.data = self.exec_alu(AluOp::Cmp, src, self.dst_val, Size::Long);
                }
                _ => {
                    self.data = self.exec_alu(self.alu_op, self.src_val, self.dst_val, self.size);
                }
            }
            // Force Long size so writeback stores the full 32-bit result
            self.size = Size::Long;
            return;
        }

        // ALU operations: ADD, SUB, CMP, AND, OR, EOR, ADDI, SUBI, etc.
        // ADDQ/SUBQ also land here (src_val already set to quick value)
        self.data = self.exec_alu(self.alu_op, self.src_val, self.dst_val, self.size);
    }

    /// Write the execution result back to the destination.
    ///
    /// Called at TAG_WRITEBACK after `perform_execute`. Routes the result
    /// to a register or queues memory write bus cycles.
    pub fn perform_writeback(&mut self) {
        let opcode = self.ir;

        // LEA: store address in destination A-register
        if (opcode & 0xF1C0) == 0x41C0 {
            if let Some(AddrMode::AddrReg(r)) = self.dst_mode {
                self.regs.set_a(r as usize, self.src_val);
            }
            return;
        }

        // PEA: push the computed address onto the stack
        if (opcode & 0xFFC0) == 0x4840 {
            self.data = self.src_val;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            return;
        }

        // CMP/CMPI/CMPA: no writeback (flags only)
        if matches!(self.alu_op, AluOp::Cmp) {
            let is_cmp = (opcode & 0xF000) == 0xB000 || (opcode & 0xFF00) == 0x0C00;
            if is_cmp {
                return;
            }
        }

        // TST: no writeback (flags only). Exclude TAS (size=3 → 0x4AC0).
        if (opcode & 0xFF00) == 0x4A00 && ((opcode >> 6) & 3) != 3 {
            return;
        }

        // MOVE to CCR/SR: no writeback (already applied in execute)
        if (opcode & 0xFFC0) == 0x44C0 || (opcode & 0xFFC0) == 0x46C0 {
            return;
        }

        // BTST memory: no writeback (only modifies Z flag)
        let is_btst_static = (opcode & 0xFFC0) == 0x0800;
        let is_btst_dynamic = (opcode & 0xF1C0) == 0x0100;
        if is_btst_static || is_btst_dynamic {
            return;
        }

        // Everything else: write data to destination
        // Covers MOVE, ALU general (ADD/SUB/AND/OR/EOR), ALU immediate
        // (ORI/ANDI/SUBI/ADDI/EORI), ADDQ/SUBQ, CLR, Scc, NOT, NEG, NEGX,
        // MOVE from SR (memory), BCHG/BCLR/BSET (memory), shifts (memory)
        if let Some(dst) = self.dst_mode {
            match dst {
                AddrMode::DataReg(r) => {
                    let reg = &mut self.regs.d[r as usize];
                    *reg = match self.size {
                        Size::Byte => (*reg & 0xFFFF_FF00) | (self.data & 0xFF),
                        Size::Word => (*reg & 0xFFFF_0000) | (self.data & 0xFFFF),
                        Size::Long => self.data,
                    };
                }
                AddrMode::AddrReg(r) => {
                    // Address register destinations are always sign-extended
                    let val = if self.size == Size::Word {
                        (self.data as i16 as i32) as u32
                    } else {
                        self.data
                    };
                    self.regs.set_a(r as usize, val);
                }
                _ => {
                    // Memory destination: queue write bus cycles
                    self.queue_write_ops(self.size);
                }
            }
        }
    }

    /// Dispatch an ALU operation and update flags.
    ///
    /// Public so variant crates can reuse the exact, heavily-tested flag
    /// semantics — notably `AluOp::Cmp`, which 68020 CAS needs (subtract
    /// flags with X preserved).
    pub fn exec_alu(&mut self, op: AluOp, src: u32, dst: u32, size: Size) -> u32 {
        let mask = size.mask();
        let s = src & mask;
        let d = dst & mask;
        match op {
            AluOp::Add => {
                let r = s.wrapping_add(d) & mask;
                self.set_flags_add(s, d, r, size);
                r
            }
            AluOp::Sub => {
                let r = d.wrapping_sub(s) & mask;
                self.set_flags_sub(s, d, r, size);
                r
            }
            AluOp::Cmp => {
                // CMP sets N, Z, V, C but does NOT affect X
                let saved_x = self.regs.sr & crate::flags::X;
                let r = d.wrapping_sub(s) & mask;
                self.set_flags_sub(s, d, r, size);
                self.regs.sr = (self.regs.sr & !crate::flags::X) | saved_x;
                r
            }
            AluOp::And => {
                let r = s & d;
                self.set_flags_logic(r, size);
                r
            }
            AluOp::Or => {
                let r = s | d;
                self.set_flags_logic(r, size);
                r
            }
            AluOp::Eor => {
                let r = s ^ d;
                self.set_flags_logic(r, size);
                r
            }
        }
    }

    // --- Flag computation ---

    pub(crate) fn set_flags_add(&mut self, s: u32, d: u32, r: u32, size: Size) {
        use crate::flags::{C, N, V, X, Z};
        let msb = size.msb_mask();
        self.regs.sr &= !(N | Z | V | C | X);
        if r == 0 {
            self.regs.sr |= Z;
        }
        if r & msb != 0 {
            self.regs.sr |= N;
        }
        let sm = s & msb != 0;
        let dm = d & msb != 0;
        let rm = r & msb != 0;
        if (sm && dm) || (!rm && (sm || dm)) {
            self.regs.sr |= C | X;
        }
        if (sm && dm && !rm) || (!sm && !dm && rm) {
            self.regs.sr |= V;
        }
    }

    pub(crate) fn set_flags_sub(&mut self, s: u32, d: u32, r: u32, size: Size) {
        use crate::flags::{C, N, V, X, Z};
        let msb = size.msb_mask();
        self.regs.sr &= !(N | Z | V | C | X);
        if r == 0 {
            self.regs.sr |= Z;
        }
        if r & msb != 0 {
            self.regs.sr |= N;
        }
        let sm = s & msb != 0;
        let dm = d & msb != 0;
        let rm = r & msb != 0;
        if (sm && !dm) || (rm && (sm || !dm)) {
            self.regs.sr |= C | X;
        }
        if (!sm && dm && !rm) || (sm && !dm && rm) {
            self.regs.sr |= V;
        }
    }

    pub(crate) fn set_flags_logic(&mut self, r: u32, size: Size) {
        use crate::flags::{C, N, V, Z};
        let msb = size.msb_mask();
        self.regs.sr &= !(N | Z | V | C);
        if r == 0 {
            self.regs.sr |= Z;
        }
        if r & msb != 0 {
            self.regs.sr |= N;
        }
    }

    pub(crate) fn set_flags_move(&mut self, val: u32, size: Size) {
        use crate::flags::{C, N, V, Z};
        let mask = size.mask();
        let msb = size.msb_mask();
        let v = val & mask;
        self.regs.sr &= !(N | Z | V | C);
        if v == 0 {
            self.regs.sr |= Z;
        }
        if v & msb != 0 {
            self.regs.sr |= N;
        }
    }

    /// Perform a register shift/rotate operation.
    ///
    /// `shift_type`: 0=AS, 1=LS, 2=ROX, 3=RO
    /// `direction`: 0=right, 1=left
    pub(crate) fn perform_shift(
        &mut self,
        reg: u8,
        count: u32,
        direction: u8,
        shift_type: u8,
        size: Size,
    ) {
        let mask = size.mask();
        let msb = size.msb_mask();
        let mut val = self.regs.d[reg as usize] & mask;
        let x_in = self.regs.sr & crate::flags::X != 0;

        use crate::flags::{C, N, V, X, Z};
        let mut sr = self.regs.sr;

        if count == 0 {
            // No shift: clear C (and V for ASL), set N/Z
            sr &= !(C | V);
            if shift_type == 2 {
                // ROXL/ROXR with count 0: C = X
                if x_in {
                    sr |= C;
                } else {
                    sr &= !C;
                }
            }
            sr &= !(N | Z);
            if val == 0 {
                sr |= Z;
            }
            if val & msb != 0 {
                sr |= N;
            }
            self.regs.sr = sr;
            return;
        }

        // SingleStepTests expects C and X to be cleared when the register
        // shift count exceeds the operand width (8/16/32). For count ==
        // width, C/X reflect the last bit shifted out normally. Emu198x
        // follows that software-oracle expectation for corpus compatibility.
        let data_bits: u32 = match size {
            Size::Byte => 8,
            Size::Word => 16,
            Size::Long => 32,
        };

        match (shift_type, direction) {
            (0, 1) => {
                // ASL (arithmetic shift left)
                sr &= !(C | X | V | N | Z);
                let mut v_changed = false;
                for _ in 0..count {
                    let out = val & msb != 0;
                    val = (val << 1) & mask;
                    let new_msb = val & msb != 0;
                    if out != new_msb {
                        v_changed = true;
                    }
                    if out {
                        sr |= C | X;
                    } else {
                        sr &= !(C | X);
                    }
                }
                if count > data_bits {
                    sr &= !(C | X);
                }
                if v_changed {
                    sr |= V;
                }
                if val == 0 {
                    sr |= Z;
                }
                if val & msb != 0 {
                    sr |= N;
                }
            }
            (0, 0) => {
                // ASR (arithmetic shift right)
                sr &= !(C | X | V | N | Z);
                for _ in 0..count {
                    let out = val & 1 != 0;
                    let sign = val & msb;
                    val = (val >> 1) | sign;
                    val &= mask;
                    if out {
                        sr |= C | X;
                    } else {
                        sr &= !(C | X);
                    }
                }
                if count > data_bits {
                    sr &= !(C | X);
                }
                if val == 0 {
                    sr |= Z;
                }
                if val & msb != 0 {
                    sr |= N;
                }
            }
            (1, 1) => {
                // LSL (logical shift left)
                sr &= !(C | X | V | N | Z);
                for _ in 0..count {
                    let out = val & msb != 0;
                    val = (val << 1) & mask;
                    if out {
                        sr |= C | X;
                    } else {
                        sr &= !(C | X);
                    }
                }
                if count > data_bits {
                    sr &= !(C | X);
                }
                if val == 0 {
                    sr |= Z;
                }
                if val & msb != 0 {
                    sr |= N;
                }
            }
            (1, 0) => {
                // LSR (logical shift right)
                sr &= !(C | X | V | N | Z);
                for _ in 0..count {
                    let out = val & 1 != 0;
                    val >>= 1;
                    if out {
                        sr |= C | X;
                    } else {
                        sr &= !(C | X);
                    }
                }
                if count > data_bits {
                    sr &= !(C | X);
                }
                if val == 0 {
                    sr |= Z;
                }
                if val & msb != 0 {
                    sr |= N;
                }
            }
            (2, 1) => {
                // ROXL (rotate left through extend)
                sr &= !(C | V | N | Z);
                let mut x = x_in;
                for _ in 0..count {
                    let out = val & msb != 0;
                    val = ((val << 1) & mask) | u32::from(x);
                    x = out;
                }
                if x {
                    sr |= C | X;
                } else {
                    sr &= !X;
                }
                if val == 0 {
                    sr |= Z;
                }
                if val & msb != 0 {
                    sr |= N;
                }
            }
            (2, 0) => {
                // ROXR (rotate right through extend)
                sr &= !(C | V | N | Z);
                let mut x = x_in;
                for _ in 0..count {
                    let out = val & 1 != 0;
                    val = (val >> 1) | if x { msb } else { 0 };
                    x = out;
                }
                if x {
                    sr |= C | X;
                } else {
                    sr &= !X;
                }
                if val == 0 {
                    sr |= Z;
                }
                if val & msb != 0 {
                    sr |= N;
                }
            }
            (3, 1) => {
                // ROL (rotate left)
                sr &= !(C | V | N | Z);
                for _ in 0..count {
                    let out = val & msb != 0;
                    val = ((val << 1) & mask) | u32::from(out);
                    if out {
                        sr |= C;
                    } else {
                        sr &= !C;
                    }
                }
                // ROL does not affect X
                if val == 0 {
                    sr |= Z;
                }
                if val & msb != 0 {
                    sr |= N;
                }
            }
            (3, 0) => {
                // ROR (rotate right)
                sr &= !(C | V | N | Z);
                for _ in 0..count {
                    let out = val & 1 != 0;
                    val = (val >> 1) | if out { msb } else { 0 };
                    if out {
                        sr |= C;
                    } else {
                        sr &= !C;
                    }
                }
                // ROR does not affect X
                if val == 0 {
                    sr |= Z;
                }
                if val & msb != 0 {
                    sr |= N;
                }
            }
            _ => {}
        }

        self.regs.sr = sr;
        // Write back to register
        let reg_val = &mut self.regs.d[reg as usize];
        *reg_val = match size {
            Size::Byte => (*reg_val & 0xFFFF_FF00) | (val & 0xFF),
            Size::Word => (*reg_val & 0xFFFF_0000) | (val & 0xFFFF),
            Size::Long => val,
        };
    }

    /// Perform a memory shift/rotate by 1.
    ///
    /// `direction`: 0=right, 1=left
    /// `shift_type`: 0=AS, 1=LS, 2=ROX, 3=RO
    pub(crate) fn perform_shift_memory(&mut self, direction: u8, shift_type: u8) {
        let val = self.dst_val & 0xFFFF;
        let msb: u32 = 0x8000;
        let mask: u32 = 0xFFFF;
        let x_in = self.regs.sr & crate::flags::X != 0;

        use crate::flags::{C, N, V, X, Z};
        let mut sr = self.regs.sr;
        let result;

        match (shift_type, direction) {
            (0, 1) => {
                // ASL by 1
                let out = val & msb != 0;
                result = (val << 1) & mask;
                sr &= !(C | X | V | N | Z);
                if out {
                    sr |= C | X;
                }
                if out != (result & msb != 0) {
                    sr |= V;
                }
                if result == 0 {
                    sr |= Z;
                }
                if result & msb != 0 {
                    sr |= N;
                }
            }
            (0, 0) => {
                // ASR by 1
                let out = val & 1 != 0;
                let sign = val & msb;
                result = ((val >> 1) | sign) & mask;
                sr &= !(C | X | V | N | Z);
                if out {
                    sr |= C | X;
                }
                if result == 0 {
                    sr |= Z;
                }
                if result & msb != 0 {
                    sr |= N;
                }
            }
            (1, 1) => {
                // LSL by 1
                let out = val & msb != 0;
                result = (val << 1) & mask;
                sr &= !(C | X | V | N | Z);
                if out {
                    sr |= C | X;
                }
                if result == 0 {
                    sr |= Z;
                }
                if result & msb != 0 {
                    sr |= N;
                }
            }
            (1, 0) => {
                // LSR by 1
                let out = val & 1 != 0;
                result = val >> 1;
                sr &= !(C | X | V | N | Z);
                if out {
                    sr |= C | X;
                }
                if result == 0 {
                    sr |= Z;
                }
                if result & msb != 0 {
                    sr |= N;
                }
            }
            (2, 1) => {
                // ROXL by 1
                let out = val & msb != 0;
                result = ((val << 1) & mask) | u32::from(x_in);
                sr &= !(C | V | N | Z);
                if out {
                    sr |= C | X;
                } else {
                    sr &= !X;
                }
                if result == 0 {
                    sr |= Z;
                }
                if result & msb != 0 {
                    sr |= N;
                }
            }
            (2, 0) => {
                // ROXR by 1
                let out = val & 1 != 0;
                result = (val >> 1) | if x_in { msb } else { 0 };
                sr &= !(C | V | N | Z);
                if out {
                    sr |= C | X;
                } else {
                    sr &= !X;
                }
                if result == 0 {
                    sr |= Z;
                }
                if result & msb != 0 {
                    sr |= N;
                }
            }
            (3, 1) => {
                // ROL by 1
                let out = val & msb != 0;
                result = ((val << 1) & mask) | u32::from(out);
                sr &= !(C | V | N | Z);
                if out {
                    sr |= C;
                }
                if result == 0 {
                    sr |= Z;
                }
                if result & msb != 0 {
                    sr |= N;
                }
            }
            (3, 0) => {
                // ROR by 1
                let out = val & 1 != 0;
                result = (val >> 1) | if out { msb } else { 0 };
                sr &= !(C | V | N | Z);
                if out {
                    sr |= C;
                }
                if result == 0 {
                    sr |= Z;
                }
                if result & msb != 0 {
                    sr |= N;
                }
            }
            _ => {
                result = val;
            }
        }

        self.regs.sr = sr;
        self.data = result;
    }

    /// Compute exact DIVU cycle timing using Jorge Cwik's restoring
    /// division algorithm. Returns total CPU clock cycles.
    pub(crate) fn divu_cycles(dividend: u32, divisor: u16) -> u8 {
        // Overflow case
        if (dividend >> 16) >= u32::from(divisor) {
            return 10;
        }

        let mut mcycles: u32 = 38;
        let hdivisor = u32::from(divisor) << 16;
        let mut dvd = dividend;

        for _ in 0..15 {
            let temp = dvd;
            dvd <<= 1;

            if temp & 0x8000_0000 != 0 {
                dvd = dvd.wrapping_sub(hdivisor);
            } else {
                mcycles += 2;
                if dvd >= hdivisor {
                    dvd = dvd.wrapping_sub(hdivisor);
                    mcycles -= 1;
                }
            }
        }
        (mcycles * 2) as u8
    }

    /// Compute exact DIVS cycle timing using Jorge Cwik's algorithm.
    /// Returns total CPU clock cycles.
    pub(crate) fn divs_cycles(dividend: i32, divisor: i16) -> u8 {
        let mut mcycles: u32 = 6;
        if dividend < 0 {
            mcycles += 1;
        }

        let abs_dividend = (dividend as i64).unsigned_abs() as u32;
        let abs_divisor = (divisor as i32).unsigned_abs() as u16;

        if (abs_dividend >> 16) >= u32::from(abs_divisor) {
            return ((mcycles + 2) * 2) as u8;
        }

        let mut aquot = abs_dividend / u32::from(abs_divisor);

        mcycles += 55;

        if divisor >= 0 {
            if dividend >= 0 {
                mcycles -= 1;
            } else {
                mcycles += 1;
            }
        }

        // Each 0-bit in the top 15 bits of the absolute quotient adds 1 mcycle
        for _ in 0..15 {
            if (aquot as i16) >= 0 {
                mcycles += 1;
            }
            aquot <<= 1;
        }
        (mcycles * 2) as u8
    }

    // --- BCD arithmetic ---

    /// Get the current X flag as 0 or 1.
    pub(crate) fn x_flag(&self) -> u8 {
        u8::from(self.regs.sr & crate::flags::X != 0)
    }

    /// BCD addition: `src + dst + extend`. Returns
    /// `(result, carry, overflow)`. The `result` and `carry` agree
    /// between SingleStepTests and Musashi; only `V` (the
    /// "undefined" flag in PRM) differs. Dispatch on
    /// [`Self::variant_musashi_bcd_v`] picks which V we report.
    pub(crate) fn bcd_add(&self, src: u8, dst: u8, extend: u8) -> (u8, bool, bool) {
        if self.variant_musashi_bcd_v {
            bcd_add_musashi(src, dst, extend)
        } else {
            bcd_add_realhw(src, dst, extend)
        }
    }

    /// BCD subtraction: `dst - src - extend`. Same variant-dispatch
    /// pattern as [`Self::bcd_add`].
    pub(crate) fn bcd_sub(&self, dst: u8, src: u8, extend: u8) -> (u8, bool, bool) {
        if self.variant_musashi_bcd_v {
            bcd_sub_musashi(dst, src, extend)
        } else {
            bcd_sub_realhw(dst, src, extend)
        }
    }

    /// NBCD: the 68k "10's-complement negate". SingleStepTests
    /// expects `bcd_sub(0, dst, X)`; Musashi computes
    /// `(0x9a - dst - X) & 0xff` directly with a special case for
    /// `pre == 0x9a` (the destination is unchanged but N is set
    /// from bit 7 of `0x9a`).
    ///
    /// Returns `Some((result, carry, overflow))` for the normal
    /// path — caller writes `result` back to the destination and
    /// calls `set_bcd_flags`. Returns `None` for the Musashi
    /// special case: this method has already adjusted N / V / C /
    /// X directly (preserving Z's sticky semantics), and the
    /// destination must stay untouched.
    pub(crate) fn nbcd_op(&mut self, src: u8, extend: u8) -> Option<(u8, bool, bool)> {
        if self.variant_musashi_bcd_v {
            nbcd_op_musashi(self, src, extend)
        } else {
            // SingleStepTests NBCD: equivalent to `bcd_sub(0, dst, X)`.
            Some(bcd_sub_realhw(0, src, extend))
        }
    }

    /// Set flags for ABCD/SBCD/NBCD operations.
    /// Z is "sticky": only cleared, never set (supports multi-byte BCD).
    pub(crate) fn set_bcd_flags(&mut self, result: u8, carry: bool, overflow: bool) {
        use crate::flags::{C, V, X, Z};
        self.regs.sr = if carry {
            self.regs.sr | X | C
        } else {
            self.regs.sr & !(X | C)
        };
        if result != 0 {
            self.regs.sr &= !Z;
        }
        if result & 0x80 != 0 {
            self.regs.sr |= 0x0008; // N
        } else {
            self.regs.sr &= !0x0008;
        }
        // SingleStepTests-compatible 68000 policy: set V from the supplied
        // overflow result. Variant wrappers may select a different
        // software-oracle policy for this architecturally undefined flag.
        if overflow {
            self.regs.sr |= V;
        } else {
            self.regs.sr &= !V;
        }
    }

    /// Evaluate a condition code (0-15) against the current SR flags.
    /// Public so variant crates (e.g. the 68020's TRAPcc / Bcc.L /
    /// coprocessor cc instructions) can reuse the same predicate.
    pub fn check_condition(&self, cond: u8) -> bool {
        use crate::flags::{C, N, V, Z};
        let sr = self.regs.sr;
        let n = sr & N != 0;
        let z = sr & Z != 0;
        let v = sr & V != 0;
        let c = sr & C != 0;
        match cond {
            0 => true,            // T
            1 => false,           // F
            2 => !c && !z,        // HI
            3 => c || z,          // LS
            4 => !c,              // CC
            5 => c,               // CS
            6 => !z,              // NE
            7 => z,               // EQ
            8 => !v,              // VC
            9 => v,               // VS
            10 => !n,             // PL
            11 => n,              // MI
            12 => n == v,         // GE
            13 => n != v,         // LT
            14 => !z && (n == v), // GT
            15 => z || (n != v),  // LE
            _ => unreachable!(),
        }
    }
}

// ─── BCD ALU helpers — SingleStepTests and Musashi variants ───────
//
// The result and carry agree between the two; only the "undefined"
// V flag is computed differently. The legacy `realhw` names match
// the implementation-generated SingleStepTests/680x0 corpus;
// Musashi values match the m68k-test-gen 68010 / 68020 corpora.

/// SingleStepTests-compatible BCD add.
/// V = bit 7 of `(~uncorrected) & corrected_result`.
fn bcd_add_realhw(src: u8, dst: u8, extend: u8) -> (u8, bool, bool) {
    let low_sum = (dst & 0x0F) + (src & 0x0F) + extend;
    let corf: u16 = if low_sum > 9 { 6 } else { 0 };
    let uncorrected = u16::from(dst) + u16::from(src) + u16::from(extend);
    let low_corrected = low_sum + if low_sum > 9 { 6 } else { 0 };
    let low_carry = low_corrected >> 4;
    let high_sum = (dst >> 4) + (src >> 4) + low_carry;
    let carry = high_sum > 9;
    let result = if carry {
        uncorrected + corf + 0x60
    } else {
        uncorrected + corf
    };
    let overflow = (!uncorrected & result & 0x80) != 0;
    (result as u8, carry, overflow)
}

/// SingleStepTests-compatible BCD subtract.
/// V = bit 7 of `uncorrected & ~result`.
fn bcd_sub_realhw(dst: u8, src: u8, extend: u8) -> (u8, bool, bool) {
    let uncorrected = dst.wrapping_sub(src).wrapping_sub(extend);
    let mut result = uncorrected;
    let low_borrowed = (dst & 0x0F) < (src & 0x0F).saturating_add(extend);
    if low_borrowed {
        result = result.wrapping_sub(6);
    }
    let high_borrowed = (dst >> 4) < (src >> 4) + u8::from(low_borrowed);
    if high_borrowed {
        result = result.wrapping_sub(0x60);
    }
    let low_correction_wraps = low_borrowed && uncorrected < 6;
    let borrow = high_borrowed || low_correction_wraps;
    let overflow = (uncorrected & !result & 0x80) != 0;
    (result, borrow, overflow)
}

/// Musashi BCD add. V = bit 7 of `(~low_nibble_intermediate) & final_result`.
fn bcd_add_musashi(src: u8, dst: u8, extend: u8) -> (u8, bool, bool) {
    let mut res: u32 = u32::from(src & 0x0f) + u32::from(dst & 0x0f) + u32::from(extend);
    let v_first = !res;
    if res > 9 {
        res += 6;
    }
    res += u32::from(src & 0xf0) + u32::from(dst & 0xf0);
    let carry = res > 0x99;
    if carry {
        res = res.wrapping_sub(0xa0);
    }
    let result = (res & 0xff) as u8;
    let overflow = (v_first & res & 0x80) != 0;
    (result, carry, overflow)
}

/// Musashi BCD subtract. Same V pattern as [`bcd_add_musashi`].
fn bcd_sub_musashi(dst: u8, src: u8, extend: u8) -> (u8, bool, bool) {
    let mut res: u32 = u32::from(dst & 0x0f)
        .wrapping_sub(u32::from(src & 0x0f))
        .wrapping_sub(u32::from(extend));
    let v_first = !res;
    if res > 9 {
        res = res.wrapping_sub(6);
    }
    res = res
        .wrapping_add(u32::from(dst & 0xf0))
        .wrapping_sub(u32::from(src & 0xf0));
    let borrow = res > 0x99;
    if borrow {
        res = res.wrapping_add(0xa0);
    }
    let result = (res & 0xff) as u8;
    let overflow = (v_first & res & 0x80) != 0;
    (result, borrow, overflow)
}

/// Musashi NBCD. Computes `(0x9a - dst - X) & 0xff` and bumps the
/// low nibble past `0xa` when the BCD correction kicks in.
///
/// Returns `Some((result, carry, overflow))` for the normal path.
/// Returns `None` for the `pre == 0x9a` corner (i.e.
/// `dst == 0 && X == 0`, or `dst == 0xff && X == 1`): Musashi
/// leaves the destination unchanged but sets N from bit 7 of the
/// intermediate `0x9a` (= 1) and clears V / C / X. We apply those
/// flag changes here and signal "no writeback" via `None`.
fn nbcd_op_musashi(
    cpu: &mut crate::cpu::Cpu68000,
    src: u8,
    extend: u8,
) -> Option<(u8, bool, bool)> {
    let pre: u32 = (0x9a_u32
        .wrapping_sub(u32::from(src))
        .wrapping_sub(u32::from(extend)))
        & 0xff;
    if pre == 0x9a {
        // Special case — set flags directly, no writeback.
        use crate::flags::{C, N, V, X};
        cpu.regs.sr = (cpu.regs.sr & !(N | V | C | X)) | N;
        return None;
    }
    let v_first = !pre;
    let mut res = pre;
    if (res & 0x0f) == 0x0a {
        res = (res & 0xf0).wrapping_add(0x10);
    }
    res &= 0xff;
    let overflow = (v_first & res & 0x80) != 0;
    Some((res as u8, true, overflow))
}
