use crate::M6502;
use crate::cycle::{self, AddrMode, OpCategory, Operation};
use crate::registers::*;

impl M6502 {
    const ANE_MAGIC: u8 = 0xEE;
    const LXA_MAGIC: u8 = 0xEE;

    pub fn tick(&mut self) -> bool {
        // RDY pin: if external hardware (e.g. VIC-II during a bad line)
        // holds RDY low during a read cycle, the NMOS 6502 stalls with
        // address/data lines unchanged and no cycle accounting. Writes
        // proceed normally on NMOS; RDY stalls only reads. The CMOS
        // 65C02 stalls both, but we model NMOS here.
        if !self.rdy && self.rw {
            return false;
        }

        self.total_cycles += 1;
        if self.so_prev && !self.so {
            self.regs.set_flag(FLAG_V, true);
        }
        self.so_prev = self.so;

        if self.reset_phase != 0 {
            return self.tick_reset();
        }

        // TODO(taskId=32): penultimate-cycle IRQ/NMI sampling and the
        // one-instruction CLI/SEI/PLP delay are not yet modelled. At
        // instruction boundary (cs.cycle == 0) we sample the current
        // IRQ/NMI lines directly rather than the latched penultimate
        // state. This is correct for simple flow but misses a few
        // edge-cases that blargg's cpu_interrupts_v2 and some tight
        // NES timing code exercise. Refactor once we have cycle-count
        // visibility in every addressing-mode helper.
        if self.cs.cycle == 0 {
            if self.nmi && !self.nmi_prev {
                self.nmi_prev = true;
                self.cs.opcode = 0x00;
                self.cs.info = Some(cycle::OpcodeInfo {
                    addr_mode: AddrMode::Brk,
                    operation: Operation::Brk,
                });
                self.cs.cycle = 1;
                self.cs.addr = 0;
                self.cs.data = 1;
                self.cs.offset = 0;
                self.cs.page_crossed = false;
                self.schedule_read(self.regs.pc);
                return false;
            }

            self.nmi_prev = self.nmi;

            if self.irq && !self.regs.interrupt_disable() {
                self.cs.opcode = 0x00;
                self.cs.info = Some(cycle::OpcodeInfo {
                    addr_mode: AddrMode::Brk,
                    operation: Operation::Brk,
                });
                self.cs.cycle = 1;
                self.cs.addr = 0;
                self.cs.data = 2;
                self.cs.offset = 0;
                self.cs.page_crossed = false;
                self.schedule_read(self.regs.pc);
                return false;
            }

            if self.halted {
                self.schedule_read(self.regs.pc);
                return true;
            }

            let opcode = self.data_in;
            self.regs.pc = self.regs.pc.wrapping_add(1);
            self.cs.opcode = opcode;
            self.cs.info = Some(cycle::decode(opcode));
            self.cs.cycle = 1;
            self.cs.addr = 0;
            self.cs.data = 0;
            self.cs.offset = 0;
            self.cs.page_crossed = false;
            self.schedule_read(self.regs.pc);
            return false;
        }

        let info = match self.cs.info {
            Some(info) => info,
            None => unreachable!("6502 instruction cycle without decoded opcode"),
        };

        let done = match info.addr_mode {
            AddrMode::Implied | AddrMode::Accumulator => self.tick_implied(info.operation),
            AddrMode::Immediate => self.tick_immediate(info.operation),
            AddrMode::ZeroPage => self.tick_zero_page(info.operation, 0, false),
            AddrMode::ZeroPageX => self.tick_zero_page(info.operation, self.regs.x, true),
            AddrMode::ZeroPageY => self.tick_zero_page(info.operation, self.regs.y, true),
            AddrMode::Absolute => self.tick_absolute(info.operation, 0, false),
            AddrMode::AbsoluteX => self.tick_absolute(info.operation, self.regs.x, true),
            AddrMode::AbsoluteY => self.tick_absolute(info.operation, self.regs.y, true),
            AddrMode::IndirectX => self.tick_indirect_x(info.operation),
            AddrMode::IndirectY => self.tick_indirect_y(info.operation),
            AddrMode::Relative => self.tick_relative(info.operation),
            AddrMode::Brk => self.tick_brk(),
            AddrMode::Jsr => self.tick_jsr(),
            AddrMode::Rts => self.tick_rts(),
            AddrMode::Rti => self.tick_rti(),
            AddrMode::JmpAbs => self.tick_jmp_abs(),
            AddrMode::JmpInd => self.tick_jmp_ind(),
            AddrMode::Push => self.tick_push(info.operation),
            AddrMode::Pull => self.tick_pull(info.operation),
            AddrMode::Jam => {
                self.halted = true;
                self.schedule_read(self.regs.pc);
                true
            }
        };

        if done {
            self.cs.cycle = 0;
            self.schedule_opcode_fetch(self.regs.pc);
        }

        done
    }

    fn tick_reset(&mut self) -> bool {
        // Reset is 7 cycles: 5 internal/phantom-push cycles during
        // which the bus is held in read-only on the stack area, then
        // two reads of the reset vector at $FFFC/$FFFD.
        match self.reset_phase {
            7 | 6 | 5 | 4 => {
                // Phantom stack cycles — bus stays on SP-relative addr.
                self.reset_phase -= 1;
                self.addr = 0x0100u16.wrapping_add(u16::from(self.regs.sp));
                self.rw = true;
                self.sync = false;
                false
            }
            3 => {
                self.reset_phase = 2;
                self.schedule_read(0xFFFC);
                false
            }
            2 => {
                self.cs.addr = self.data_in as u16;
                self.reset_phase = 1;
                self.schedule_read(0xFFFD);
                false
            }
            1 => {
                self.cs.addr |= (self.data_in as u16) << 8;
                self.regs.pc = self.cs.addr;
                self.reset_phase = 0;
                self.cs.addr = 0;
                self.schedule_opcode_fetch(self.regs.pc);
                true
            }
            _ => true,
        }
    }

    #[inline]
    pub(crate) fn schedule_read(&mut self, addr: u16) {
        self.addr = addr;
        self.rw = true;
        self.sync = false;
    }

    #[inline]
    pub(crate) fn schedule_write(&mut self, addr: u16, data: u8) {
        self.addr = addr;
        self.data = data;
        self.rw = false;
        self.sync = false;
    }

    #[inline]
    pub(crate) fn schedule_opcode_fetch(&mut self, pc: u16) {
        self.addr = pc;
        self.rw = true;
        self.sync = true;
    }

    fn tick_implied(&mut self, op: Operation) -> bool {
        self.apply_implied(op);
        true
    }

    fn tick_immediate(&mut self, op: Operation) -> bool {
        let data = self.data_in;
        self.regs.pc = self.regs.pc.wrapping_add(1);
        self.apply_read(op, data);
        true
    }

    fn tick_zero_page(&mut self, op: Operation, index: u8, indexed: bool) -> bool {
        let category = op.category();

        match self.cs.cycle {
            1 => {
                self.cs.addr = self.data_in as u16;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                if indexed {
                    self.cs.cycle = 2;
                    self.schedule_read(self.cs.addr);
                } else {
                    self.cs.cycle = 3;
                    self.schedule_zp_abs_cycle3(category, op);
                }
                false
            }
            2 => {
                self.cs.addr = (self.cs.addr.wrapping_add(index as u16)) & 0x00FF;
                self.cs.cycle = 3;
                self.schedule_zp_abs_cycle3(category, op);
                false
            }
            3 => match category {
                OpCategory::Read | OpCategory::Implied => {
                    self.apply_read(op, self.data_in);
                    true
                }
                OpCategory::Write => true,
                OpCategory::ReadModWrite => {
                    self.cs.data = self.data_in;
                    self.cs.cycle = 4;
                    self.schedule_write(self.cs.addr, self.cs.data);
                    false
                }
                OpCategory::Control => true,
            },
            4 => {
                self.cs.data = self.apply_rmw(op, self.cs.data);
                self.cs.cycle = 5;
                self.schedule_write(self.cs.addr, self.cs.data);
                false
            }
            5 => true,
            _ => true,
        }
    }

    fn tick_absolute(&mut self, op: Operation, index: u8, indexed: bool) -> bool {
        let category = op.category();
        let is_read = matches!(category, OpCategory::Read | OpCategory::Implied);

        match self.cs.cycle {
            1 => {
                self.cs.addr = self.data_in as u16;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cs.cycle = 2;
                self.schedule_read(self.regs.pc);
                false
            }
            2 => {
                let hi = self.data_in as u16;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cs.addr |= hi << 8;

                if indexed {
                    let base = self.cs.addr;
                    self.cs.addr = base.wrapping_add(index as u16);
                    self.cs.page_crossed = (base & 0xFF00) != (self.cs.addr & 0xFF00);
                    self.cs.cycle = 3;
                    if is_read && !self.cs.page_crossed {
                        self.schedule_read(self.cs.addr);
                    } else {
                        let wrong = (self.cs.addr & 0x00FF)
                            | (self.cs.addr.wrapping_sub(index as u16) & 0xFF00);
                        self.schedule_read(wrong);
                    }
                } else {
                    self.cs.cycle = 4;
                    self.schedule_zp_abs_cycle3(category, op);
                }
                false
            }
            3 => {
                if is_read && !self.cs.page_crossed {
                    self.apply_read(op, self.data_in);
                    return true;
                }
                self.cs.cycle = 4;
                self.schedule_zp_abs_cycle3(category, op);
                false
            }
            4 => match category {
                OpCategory::Read | OpCategory::Implied => {
                    self.apply_read(op, self.data_in);
                    true
                }
                OpCategory::Write => true,
                OpCategory::ReadModWrite => {
                    self.cs.data = self.data_in;
                    self.cs.cycle = 5;
                    self.schedule_write(self.cs.addr, self.cs.data);
                    false
                }
                OpCategory::Control => true,
            },
            5 => {
                self.cs.data = self.apply_rmw(op, self.cs.data);
                self.cs.cycle = 6;
                self.schedule_write(self.cs.addr, self.cs.data);
                false
            }
            6 => true,
            _ => true,
        }
    }

    fn schedule_zp_abs_cycle3(&mut self, category: OpCategory, op: Operation) {
        match category {
            OpCategory::Read | OpCategory::Implied | OpCategory::ReadModWrite => {
                self.schedule_read(self.cs.addr)
            }
            OpCategory::Write => {
                let data = self.get_write_data(op);
                let addr = self.get_write_addr(op, data);
                self.schedule_write(addr, data);
            }
            OpCategory::Control => self.schedule_read(self.cs.addr),
        }
    }

    fn tick_indirect_x(&mut self, op: Operation) -> bool {
        let category = op.category();
        match self.cs.cycle {
            1 => {
                self.cs.data = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cs.cycle = 2;
                self.schedule_read(self.cs.data as u16);
                false
            }
            2 => {
                self.cs.data = self.cs.data.wrapping_add(self.regs.x);
                self.cs.cycle = 3;
                self.schedule_read(self.cs.data as u16);
                false
            }
            3 => {
                self.cs.addr = self.data_in as u16;
                self.cs.cycle = 4;
                self.schedule_read(self.cs.data.wrapping_add(1) as u16);
                false
            }
            4 => {
                self.cs.addr |= (self.data_in as u16) << 8;
                self.cs.cycle = 5;
                self.schedule_zp_abs_cycle3(category, op);
                false
            }
            5 => match category {
                OpCategory::Read | OpCategory::Implied => {
                    self.apply_read(op, self.data_in);
                    true
                }
                OpCategory::Write => true,
                OpCategory::ReadModWrite => {
                    self.cs.data = self.data_in;
                    self.cs.cycle = 6;
                    self.schedule_write(self.cs.addr, self.cs.data);
                    false
                }
                OpCategory::Control => true,
            },
            6 => {
                self.cs.data = self.apply_rmw(op, self.cs.data);
                self.cs.cycle = 7;
                self.schedule_write(self.cs.addr, self.cs.data);
                false
            }
            7 => true,
            _ => true,
        }
    }

    fn tick_indirect_y(&mut self, op: Operation) -> bool {
        let category = op.category();
        let is_read = matches!(category, OpCategory::Read | OpCategory::Implied);

        match self.cs.cycle {
            1 => {
                self.cs.data = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cs.cycle = 2;
                self.schedule_read(self.cs.data as u16);
                false
            }
            2 => {
                self.cs.addr = self.data_in as u16;
                self.cs.cycle = 3;
                self.schedule_read(self.cs.data.wrapping_add(1) as u16);
                false
            }
            3 => {
                let hi = self.data_in as u16;
                self.cs.addr |= hi << 8;
                let base = self.cs.addr;
                self.cs.addr = base.wrapping_add(self.regs.y as u16);
                self.cs.page_crossed = (base & 0xFF00) != (self.cs.addr & 0xFF00);
                self.cs.cycle = 4;
                if is_read && !self.cs.page_crossed {
                    self.schedule_read(self.cs.addr);
                } else {
                    let wrong = (self.cs.addr & 0x00FF) | (base & 0xFF00);
                    self.schedule_read(wrong);
                }
                false
            }
            4 => {
                if is_read && !self.cs.page_crossed {
                    self.apply_read(op, self.data_in);
                    return true;
                }
                self.cs.cycle = 5;
                self.schedule_zp_abs_cycle3(category, op);
                false
            }
            5 => match category {
                OpCategory::Read | OpCategory::Implied => {
                    self.apply_read(op, self.data_in);
                    true
                }
                OpCategory::Write => true,
                OpCategory::ReadModWrite => {
                    self.cs.data = self.data_in;
                    self.cs.cycle = 6;
                    self.schedule_write(self.cs.addr, self.cs.data);
                    false
                }
                OpCategory::Control => true,
            },
            6 => {
                self.cs.data = self.apply_rmw(op, self.cs.data);
                self.cs.cycle = 7;
                self.schedule_write(self.cs.addr, self.cs.data);
                false
            }
            7 => true,
            _ => true,
        }
    }

    fn tick_relative(&mut self, op: Operation) -> bool {
        match self.cs.cycle {
            1 => {
                self.cs.offset = self.data_in;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                if !self.branch_taken(op) {
                    return true;
                }
                self.cs.cycle = 2;
                self.schedule_read(self.regs.pc);
                false
            }
            2 => {
                let old_pc = self.regs.pc;
                let offset = self.cs.offset as i8 as i16;
                self.regs.pc = self.regs.pc.wrapping_add(offset as u16);
                if (old_pc & 0xFF00) != (self.regs.pc & 0xFF00) {
                    self.cs.cycle = 3;
                    let wrong = (self.regs.pc & 0x00FF)
                        | (self.regs.pc.wrapping_sub(offset as u16) & 0xFF00);
                    self.schedule_read(wrong);
                    false
                } else {
                    true
                }
            }
            3 => true,
            _ => true,
        }
    }

    fn tick_brk(&mut self) -> bool {
        let is_nmi = self.cs.data == 1;
        let is_irq = self.cs.data == 2;
        let is_software_brk = !is_nmi && !is_irq;

        match self.cs.cycle {
            1 => {
                if is_software_brk {
                    self.regs.pc = self.regs.pc.wrapping_add(1);
                }
                self.cs.cycle = 2;
                let sp = 0x0100 | self.regs.sp as u16;
                self.schedule_write(sp, (self.regs.pc >> 8) as u8);
                false
            }
            2 => {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.cs.cycle = 3;
                let sp = 0x0100 | self.regs.sp as u16;
                self.schedule_write(sp, self.regs.pc as u8);
                false
            }
            3 => {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.cs.cycle = 4;
                let sp = 0x0100 | self.regs.sp as u16;
                let p = if is_nmi || is_irq {
                    (self.regs.p | FLAG_U) & !FLAG_B
                } else {
                    self.regs.p | FLAG_B | FLAG_U
                };
                self.schedule_write(sp, p);
                false
            }
            4 => {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.regs.set_flag(FLAG_I, true);
                self.cs.cycle = 5;
                self.schedule_read(if is_nmi { 0xFFFA } else { 0xFFFE });
                false
            }
            5 => {
                self.cs.addr = self.data_in as u16;
                self.cs.cycle = 6;
                self.schedule_read(if is_nmi { 0xFFFB } else { 0xFFFF });
                false
            }
            6 => {
                self.cs.addr |= (self.data_in as u16) << 8;
                self.regs.pc = self.cs.addr;
                true
            }
            _ => true,
        }
    }

    fn tick_jsr(&mut self) -> bool {
        match self.cs.cycle {
            1 => {
                self.cs.addr = self.data_in as u16;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cs.cycle = 2;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            2 => {
                self.cs.cycle = 3;
                let sp = 0x0100 | self.regs.sp as u16;
                self.schedule_write(sp, (self.regs.pc >> 8) as u8);
                false
            }
            3 => {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.cs.cycle = 4;
                let sp = 0x0100 | self.regs.sp as u16;
                self.schedule_write(sp, self.regs.pc as u8);
                false
            }
            4 => {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                self.cs.cycle = 5;
                self.schedule_read(self.regs.pc);
                false
            }
            5 => {
                self.cs.addr |= (self.data_in as u16) << 8;
                self.regs.pc = self.cs.addr;
                true
            }
            _ => true,
        }
    }

    fn tick_rts(&mut self) -> bool {
        match self.cs.cycle {
            1 => {
                self.cs.cycle = 2;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            2 => {
                self.regs.sp = self.regs.sp.wrapping_add(1);
                self.cs.cycle = 3;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            3 => {
                self.cs.addr = self.data_in as u16;
                self.regs.sp = self.regs.sp.wrapping_add(1);
                self.cs.cycle = 4;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            4 => {
                self.cs.addr |= (self.data_in as u16) << 8;
                self.cs.cycle = 5;
                self.schedule_read(self.cs.addr);
                false
            }
            5 => {
                self.regs.pc = self.cs.addr.wrapping_add(1);
                true
            }
            _ => true,
        }
    }

    fn tick_rti(&mut self) -> bool {
        match self.cs.cycle {
            1 => {
                self.cs.cycle = 2;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            2 => {
                self.regs.sp = self.regs.sp.wrapping_add(1);
                self.cs.cycle = 3;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            3 => {
                self.regs.p = (self.data_in & !FLAG_B) | FLAG_U;
                self.regs.sp = self.regs.sp.wrapping_add(1);
                self.cs.cycle = 4;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            4 => {
                self.cs.addr = self.data_in as u16;
                self.regs.sp = self.regs.sp.wrapping_add(1);
                self.cs.cycle = 5;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            5 => {
                self.cs.addr |= (self.data_in as u16) << 8;
                self.regs.pc = self.cs.addr;
                true
            }
            _ => true,
        }
    }

    fn tick_jmp_abs(&mut self) -> bool {
        match self.cs.cycle {
            1 => {
                self.cs.addr = self.data_in as u16;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cs.cycle = 2;
                self.schedule_read(self.regs.pc);
                false
            }
            2 => {
                self.cs.addr |= (self.data_in as u16) << 8;
                self.regs.pc = self.cs.addr;
                true
            }
            _ => true,
        }
    }

    fn tick_jmp_ind(&mut self) -> bool {
        match self.cs.cycle {
            1 => {
                self.cs.addr = self.data_in as u16;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cs.cycle = 2;
                self.schedule_read(self.regs.pc);
                false
            }
            2 => {
                self.cs.addr |= (self.data_in as u16) << 8;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cs.cycle = 3;
                self.schedule_read(self.cs.addr);
                false
            }
            3 => {
                self.cs.data = self.data_in;
                self.cs.cycle = 4;
                let hi_addr = (self.cs.addr & 0xFF00) | (self.cs.addr.wrapping_add(1) & 0x00FF);
                self.schedule_read(hi_addr);
                false
            }
            4 => {
                self.regs.pc = self.cs.data as u16 | ((self.data_in as u16) << 8);
                true
            }
            _ => true,
        }
    }

    fn tick_push(&mut self, op: Operation) -> bool {
        match self.cs.cycle {
            1 => {
                let sp = 0x0100 | self.regs.sp as u16;
                let data = match op {
                    Operation::Pha => self.regs.a,
                    Operation::Php => self.regs.p | FLAG_B | FLAG_U,
                    _ => 0,
                };
                self.schedule_write(sp, data);
                self.cs.cycle = 2;
                false
            }
            2 => {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                true
            }
            _ => true,
        }
    }

    fn tick_pull(&mut self, op: Operation) -> bool {
        match self.cs.cycle {
            1 => {
                self.cs.cycle = 2;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            2 => {
                self.regs.sp = self.regs.sp.wrapping_add(1);
                self.cs.cycle = 3;
                self.schedule_read(0x0100 | self.regs.sp as u16);
                false
            }
            3 => {
                match op {
                    Operation::Pla => {
                        self.regs.a = self.data_in;
                        self.regs.set_nz(self.data_in);
                    }
                    Operation::Plp => {
                        self.regs.p = (self.data_in & !FLAG_B) | FLAG_U;
                    }
                    _ => {}
                }
                true
            }
            _ => true,
        }
    }

    fn apply_read(&mut self, op: Operation, data: u8) {
        match op {
            Operation::Lda => {
                self.regs.a = data;
                self.regs.set_nz(data);
            }
            Operation::Ldx => {
                self.regs.x = data;
                self.regs.set_nz(data);
            }
            Operation::Ldy => {
                self.regs.y = data;
                self.regs.set_nz(data);
            }
            Operation::Ora => {
                self.regs.a |= data;
                self.regs.set_nz(self.regs.a);
            }
            Operation::And => {
                self.regs.a &= data;
                self.regs.set_nz(self.regs.a);
            }
            Operation::Eor => {
                self.regs.a ^= data;
                self.regs.set_nz(self.regs.a);
            }
            Operation::Adc => self.alu_adc(data),
            Operation::Sbc => self.alu_sbc(data),
            Operation::Cmp => self.alu_cmp(self.regs.a, data),
            Operation::Cpx => self.alu_cmp(self.regs.x, data),
            Operation::Cpy => self.alu_cmp(self.regs.y, data),
            Operation::Bit => {
                let result = self.regs.a & data;
                self.regs.set_flag(FLAG_Z, result == 0);
                self.regs.set_flag(FLAG_N, data & 0x80 != 0);
                self.regs.set_flag(FLAG_V, data & 0x40 != 0);
            }
            Operation::Lax => {
                self.regs.a = data;
                self.regs.x = data;
                self.regs.set_nz(data);
            }
            Operation::Anc => {
                self.regs.a &= data;
                self.regs.set_nz(self.regs.a);
                self.regs.set_flag(FLAG_C, self.regs.a & 0x80 != 0);
            }
            Operation::Alr => {
                self.regs.a &= data;
                self.regs.set_flag(FLAG_C, self.regs.a & 0x01 != 0);
                self.regs.a >>= 1;
                self.regs.set_nz(self.regs.a);
            }
            Operation::Arr => {
                if self.regs.decimal() && !self.decimal_disabled {
                    self.alu_arr_bcd(data);
                } else {
                    self.regs.a &= data;
                    let carry = self.regs.carry() as u8;
                    self.regs.a = (self.regs.a >> 1) | (carry << 7);
                    self.regs.set_nz(self.regs.a);
                    self.regs.set_flag(FLAG_C, self.regs.a & 0x40 != 0);
                    self.regs
                        .set_flag(FLAG_V, ((self.regs.a >> 6) ^ (self.regs.a >> 5)) & 1 != 0);
                }
            }
            Operation::Axs => {
                let ax = self.regs.a & self.regs.x;
                let val = ax.wrapping_sub(data);
                self.regs.x = val;
                self.regs.set_nz(val);
                self.regs.set_flag(FLAG_C, ax >= data);
            }
            Operation::Ane => {
                self.regs.a = (self.regs.a | Self::ANE_MAGIC) & self.regs.x & data;
                self.regs.set_nz(self.regs.a);
            }
            Operation::Lxa => {
                let result = (self.regs.a | Self::LXA_MAGIC) & data;
                self.regs.a = result;
                self.regs.x = result;
                self.regs.set_nz(result);
            }
            Operation::Las => {
                let result = data & self.regs.sp;
                self.regs.sp = result;
                self.regs.a = result;
                self.regs.x = result;
                self.regs.set_nz(result);
            }
            Operation::NopRead | Operation::Nop => {}
            _ => {}
        }
    }

    fn apply_implied(&mut self, op: Operation) {
        match op {
            Operation::Tax => {
                self.regs.x = self.regs.a;
                self.regs.set_nz(self.regs.x);
            }
            Operation::Tay => {
                self.regs.y = self.regs.a;
                self.regs.set_nz(self.regs.y);
            }
            Operation::Txa => {
                self.regs.a = self.regs.x;
                self.regs.set_nz(self.regs.a);
            }
            Operation::Tya => {
                self.regs.a = self.regs.y;
                self.regs.set_nz(self.regs.a);
            }
            Operation::Tsx => {
                self.regs.x = self.regs.sp;
                self.regs.set_nz(self.regs.x);
            }
            Operation::Txs => self.regs.sp = self.regs.x,
            Operation::Inx => {
                self.regs.x = self.regs.x.wrapping_add(1);
                self.regs.set_nz(self.regs.x);
            }
            Operation::Iny => {
                self.regs.y = self.regs.y.wrapping_add(1);
                self.regs.set_nz(self.regs.y);
            }
            Operation::Dex => {
                self.regs.x = self.regs.x.wrapping_sub(1);
                self.regs.set_nz(self.regs.x);
            }
            Operation::Dey => {
                self.regs.y = self.regs.y.wrapping_sub(1);
                self.regs.set_nz(self.regs.y);
            }
            Operation::Clc => self.regs.set_flag(FLAG_C, false),
            Operation::Sec => self.regs.set_flag(FLAG_C, true),
            Operation::Cli => self.regs.set_flag(FLAG_I, false),
            Operation::Sei => self.regs.set_flag(FLAG_I, true),
            Operation::Clv => self.regs.set_flag(FLAG_V, false),
            Operation::Cld => self.regs.set_flag(FLAG_D, false),
            Operation::Sed => self.regs.set_flag(FLAG_D, true),
            Operation::AslA => {
                self.regs.set_flag(FLAG_C, self.regs.a & 0x80 != 0);
                self.regs.a <<= 1;
                self.regs.set_nz(self.regs.a);
            }
            Operation::LsrA => {
                self.regs.set_flag(FLAG_C, self.regs.a & 0x01 != 0);
                self.regs.a >>= 1;
                self.regs.set_nz(self.regs.a);
            }
            Operation::RolA => {
                let carry = self.regs.carry() as u8;
                self.regs.set_flag(FLAG_C, self.regs.a & 0x80 != 0);
                self.regs.a = (self.regs.a << 1) | carry;
                self.regs.set_nz(self.regs.a);
            }
            Operation::RorA => {
                let carry = self.regs.carry() as u8;
                self.regs.set_flag(FLAG_C, self.regs.a & 0x01 != 0);
                self.regs.a = (self.regs.a >> 1) | (carry << 7);
                self.regs.set_nz(self.regs.a);
            }
            Operation::Nop => {}
            _ => {}
        }
    }

    fn apply_rmw(&mut self, op: Operation, data: u8) -> u8 {
        match op {
            Operation::Asl => {
                self.regs.set_flag(FLAG_C, data & 0x80 != 0);
                let result = data << 1;
                self.regs.set_nz(result);
                result
            }
            Operation::Lsr => {
                self.regs.set_flag(FLAG_C, data & 0x01 != 0);
                let result = data >> 1;
                self.regs.set_nz(result);
                result
            }
            Operation::Rol => {
                let carry = self.regs.carry() as u8;
                self.regs.set_flag(FLAG_C, data & 0x80 != 0);
                let result = (data << 1) | carry;
                self.regs.set_nz(result);
                result
            }
            Operation::Ror => {
                let carry = self.regs.carry() as u8;
                self.regs.set_flag(FLAG_C, data & 0x01 != 0);
                let result = (data >> 1) | (carry << 7);
                self.regs.set_nz(result);
                result
            }
            Operation::Inc => {
                let result = data.wrapping_add(1);
                self.regs.set_nz(result);
                result
            }
            Operation::Dec => {
                let result = data.wrapping_sub(1);
                self.regs.set_nz(result);
                result
            }
            Operation::Slo => {
                let result = self.apply_rmw(Operation::Asl, data);
                self.regs.a |= result;
                self.regs.set_nz(self.regs.a);
                result
            }
            Operation::Rla => {
                let result = self.apply_rmw(Operation::Rol, data);
                self.regs.a &= result;
                self.regs.set_nz(self.regs.a);
                result
            }
            Operation::Sre => {
                let result = self.apply_rmw(Operation::Lsr, data);
                self.regs.a ^= result;
                self.regs.set_nz(self.regs.a);
                result
            }
            Operation::Rra => {
                let result = self.apply_rmw(Operation::Ror, data);
                self.alu_adc(result);
                result
            }
            Operation::Dcp => {
                let result = data.wrapping_sub(1);
                self.regs.set_nz(result);
                self.alu_cmp(self.regs.a, result);
                result
            }
            Operation::Isc => {
                let result = data.wrapping_add(1);
                self.regs.set_nz(result);
                self.alu_sbc(result);
                result
            }
            _ => data,
        }
    }

    fn get_write_data(&mut self, op: Operation) -> u8 {
        match op {
            Operation::Sta => self.regs.a,
            Operation::Stx => self.regs.x,
            Operation::Sty => self.regs.y,
            Operation::Sax => self.regs.a & self.regs.x,
            Operation::Sha => self.regs.a & self.regs.x & self.unstable_store_high_mask(),
            Operation::Shx => self.regs.x & self.unstable_store_high_mask(),
            Operation::Shy => self.regs.y & self.unstable_store_high_mask(),
            Operation::Tas => {
                self.regs.sp = self.regs.a & self.regs.x;
                self.regs.sp & self.unstable_store_high_mask()
            }
            _ => 0,
        }
    }

    fn get_write_addr(&self, op: Operation, data: u8) -> u16 {
        match op {
            Operation::Sha | Operation::Shx | Operation::Shy | Operation::Tas
                if self.cs.page_crossed =>
            {
                (self.cs.addr & 0x00FF) | ((data as u16) << 8)
            }
            _ => self.cs.addr,
        }
    }

    fn unstable_store_high_mask(&self) -> u8 {
        let high = (self.cs.addr >> 8) as u8;
        if self.cs.page_crossed {
            high
        } else {
            high.wrapping_add(1)
        }
    }

    fn branch_taken(&self, op: Operation) -> bool {
        match op {
            Operation::Bcc => !self.regs.carry(),
            Operation::Bcs => self.regs.carry(),
            Operation::Bne => !self.regs.zero(),
            Operation::Beq => self.regs.zero(),
            Operation::Bpl => !self.regs.negative(),
            Operation::Bmi => self.regs.negative(),
            Operation::Bvc => !self.regs.overflow(),
            Operation::Bvs => self.regs.overflow(),
            _ => false,
        }
    }

    fn alu_adc(&mut self, data: u8) {
        if self.regs.decimal() && !self.decimal_disabled {
            self.alu_adc_bcd(data);
        } else {
            let a = self.regs.a as u16;
            let d = data as u16;
            let carry = self.regs.carry() as u16;
            let sum = a + d + carry;
            self.regs.set_flag(FLAG_C, sum > 0xFF);
            self.regs
                .set_flag(FLAG_V, (!(a ^ d) & (a ^ sum) & 0x80) != 0);
            self.regs.a = sum as u8;
            self.regs.set_nz(self.regs.a);
        }
    }

    fn alu_adc_bcd(&mut self, data: u8) {
        // NMOS BCD per Oxyron ("Extra Instructions of the 65XX Series CPU"):
        //   N, Z come from the straight binary sum (bin = a + data + c).
        //   V comes from the two's-complement overflow of the binary sum.
        //   C comes from the final decimal adjustment.
        let a = self.regs.a;
        let carry_in = self.regs.carry() as u8;

        // Binary sum — used for N, Z, V (Oxyron semantics).
        let bin = a.wrapping_add(data).wrapping_add(carry_in);
        self.regs.set_flag(FLAG_Z, bin == 0);
        self.regs.set_flag(FLAG_N, bin & 0x80 != 0);
        self.regs
            .set_flag(FLAG_V, ((!(a ^ data) & (a ^ bin)) & 0x80) != 0);

        // Decimal correction produces the result byte and carry-out.
        let mut lo = (a & 0x0F) + (data & 0x0F) + carry_in;
        let mut hi = (a >> 4) + (data >> 4);
        if lo > 9 {
            lo -= 10;
            hi += 1;
        }
        if hi > 9 {
            hi -= 10;
            self.regs.set_flag(FLAG_C, true);
        } else {
            self.regs.set_flag(FLAG_C, false);
        }

        self.regs.a = (hi << 4) | (lo & 0x0F);
    }

    fn alu_arr_bcd(&mut self, data: u8) {
        let tmp = self.regs.a & data;
        let mut shifted = tmp as u16;
        shifted |= (self.regs.carry() as u16) << 8;
        shifted >>= 1;

        self.regs.set_flag(FLAG_N, self.regs.carry());
        self.regs.set_flag(FLAG_Z, (shifted as u8) == 0);
        self.regs
            .set_flag(FLAG_V, ((shifted as u8 ^ tmp) & 0x40) != 0);

        if (u16::from(tmp & 0x0F) + u16::from(tmp & 0x01)) > 0x05 {
            shifted = (shifted & 0xF0) | ((shifted + 0x06) & 0x0F);
        }

        if (u16::from(tmp & 0xF0) + u16::from(tmp & 0x10)) > 0x50 {
            shifted = (shifted & 0x0F) | ((shifted + 0x60) & 0xF0);
            self.regs.set_flag(FLAG_C, true);
        } else {
            self.regs.set_flag(FLAG_C, false);
        }

        self.regs.a = shifted as u8;
    }

    fn alu_sbc(&mut self, data: u8) {
        if self.regs.decimal() && !self.decimal_disabled {
            self.alu_sbc_bcd(data);
        } else {
            self.alu_adc(data ^ 0xFF);
        }
    }

    fn alu_sbc_bcd(&mut self, data: u8) {
        // NMOS BCD subtract per Oxyron: flags come from the binary
        // subtract (ADC-with-inverted-operand semantics), while the
        // result byte uses the decimal correction. This matches the
        // Bruce Clark BCD test suite.
        let a = self.regs.a;
        let carry_in = self.regs.carry() as u8;
        let inv = data ^ 0xFF;

        // Binary path — same math ADC does with ~data, used for N, Z, V.
        let bin_sum = (a as u16) + (inv as u16) + (carry_in as u16);
        let bin = bin_sum as u8;
        self.regs.set_flag(FLAG_Z, bin == 0);
        self.regs.set_flag(FLAG_N, bin & 0x80 != 0);
        self.regs
            .set_flag(FLAG_V, ((!(a ^ inv) & (a ^ bin)) & 0x80) != 0);

        // Decimal correction for the result byte + carry.
        let mut lo = (a & 0x0F).wrapping_sub(data & 0x0F).wrapping_sub(1 - carry_in) as i8;
        let mut hi = ((a >> 4) as i8).wrapping_sub((data >> 4) as i8);
        if lo < 0 {
            lo += 10;
            hi -= 1;
        }
        if hi < 0 {
            hi += 10;
            self.regs.set_flag(FLAG_C, false);
        } else {
            self.regs.set_flag(FLAG_C, true);
        }

        self.regs.a = ((hi as u8) << 4) | (lo as u8 & 0x0F);
    }

    fn alu_cmp(&mut self, reg: u8, data: u8) {
        let result = reg.wrapping_sub(data);
        self.regs.set_flag(FLAG_C, reg >= data);
        self.regs.set_nz(result);
    }
}
