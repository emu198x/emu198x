//! Motorola 68000 CPU core with Reactive Bus State Machine.

use crate::bus::{M68kBus, BusStatus, FunctionCode};
use crate::registers::Registers;
use crate::addressing::AddrMode;
use crate::alu::Size;

/// Maximum number of micro-ops that can be queued for a single instruction.
const QUEUE_CAPACITY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroOp {
    FetchIRC,
    ReadByte,
    ReadWord,
    ReadLongHi,
    ReadLongLo,
    WriteByte,
    WriteWord,
    WriteLongHi,
    WriteLongLo,
    PushWord,
    PushLongHi,
    PushLongLo,
    PopWord,
    PopLongHi,
    PopLongLo,
    Internal(u8),
    AssertReset,
    Execute,
    PromoteIRC,
}

impl MicroOp {
    pub fn is_instant(self) -> bool {
        matches!(self, Self::AssertReset | Self::Execute | Self::PromoteIRC | Self::Internal(0))
    }

    pub fn is_bus(self) -> bool {
        !self.is_instant() && !matches!(self, Self::Internal(_))
    }
}

#[derive(Clone)]
pub struct MicroOpQueue {
    ops: [MicroOp; QUEUE_CAPACITY],
    head: u8,
    len: u8,
}

impl MicroOpQueue {
    pub fn new() -> Self {
        Self {
            ops: [MicroOp::Internal(0); QUEUE_CAPACITY],
            head: 0,
            len: 0,
        }
    }

    pub fn push(&mut self, op: MicroOp) {
        let idx = (self.head as usize + self.len as usize) % QUEUE_CAPACITY;
        self.ops[idx] = op;
        self.len += 1;
    }

    pub fn push_front(&mut self, op: MicroOp) {
        self.head = if self.head == 0 {
            (QUEUE_CAPACITY - 1) as u8
        } else {
            self.head - 1
        };
        self.ops[self.head as usize] = op;
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<MicroOp> {
        if self.len == 0 {
            None
        } else {
            let op = self.ops[self.head as usize];
            self.head = ((self.head as usize + 1) % QUEUE_CAPACITY) as u8;
            self.len -= 1;
            Some(op)
        }
    }

    pub fn front(&self) -> Option<MicroOp> {
        if self.len == 0 {
            None
        } else {
            Some(self.ops[self.head as usize])
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    pub fn debug_contents(&self) -> String {
        let mut out = String::from("[");
        for i in 0..self.len as usize {
            let idx = (self.head as usize + i) % QUEUE_CAPACITY;
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{:?}", self.ops[idx]));
        }
        out.push(']');
        out
    }
}

pub enum State {
    /// CPU is between instructions or in a purely internal state.
    Idle,
    /// CPU is performing an internal operation (e.g. ALU) for N cycles.
    Internal { cycles: u8 },
    /// CPU is performing a bus cycle and waiting for Ready/Error.
    BusCycle {
        op: MicroOp,
        addr: u32,
        fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
        cycle_count: u8,
    },
    /// CPU is halted (double bus fault).
    Halted,
    /// CPU is stopped (waiting for interrupt).
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AluOp {
    Add,
    Sub,
    Cmp,
    And,
    Or,
    Eor,
}

pub struct Cpu68000 {
    pub regs: Registers,
    pub state: State,
    pub micro_ops: MicroOpQueue,
    
    // Prefetch pipeline
    pub ir: u16,
    pub irc: u16,
    pub irc_addr: u32,

    // Temporary storage for instruction execution
    pub addr: u32,
    pub data: u32,
    pub instr_start_pc: u32,

    // Multi-stage decode state
    pub in_followup: bool,
    pub followup_tag: u8,

    // Temporary storage for staged EA calculation
    pub src_mode: Option<AddrMode>,
    pub dst_mode: Option<AddrMode>,
    pub size: Size,
    pub ea_reg: u8,
    pub ea_pc: u32,
    pub alu_op: AluOp,
}

const FOLLOWUP_MOVE_READ_SRC_DATA: u8 = 1;
const FOLLOWUP_MOVE_CALC_DST_EA: u8 = 2;
const FOLLOWUP_MOVE_WRITE_DATA: u8 = 3;
const FOLLOWUP_MOVE_READ_SRC_EA_LONG: u8 = 4;
const FOLLOWUP_MOVE_CALC_DST_EA_LONG: u8 = 5;
const FOLLOWUP_MOVE_READ_SRC_DATA_LONG: u8 = 6;
const FOLLOWUP_MOVE_READ_SRC_EA_DISP: u8 = 7;
const FOLLOWUP_MOVE_CALC_DST_EA_DISP: u8 = 8;
const FOLLOWUP_MOVE_READ_SRC_EA_PCDISP: u8 = 9;
const FOLLOWUP_MOVE_CALC_DST_EA_PCDISP: u8 = 10;
const FOLLOWUP_ALU_READ_SRC: u8 = 11;
const FOLLOWUP_ALU_CALC_DST: u8 = 12;
const FOLLOWUP_ALU_EXECUTE: u8 = 13;
const FOLLOWUP_BCC_EXECUTE: u8 = 14;

impl Cpu68000 {
    pub fn new() -> Self {
        Self {
            regs: Registers::new(),
            state: State::Idle,
            micro_ops: MicroOpQueue::new(),
            ir: 0,
            irc: 0,
            irc_addr: 0,
            addr: 0,
            data: 0,
            instr_start_pc: 0,
            in_followup: false,
            followup_tag: 0,
            src_mode: None,
            dst_mode: None,
            size: Size::Word,
            ea_reg: 0,
            ea_pc: 0,
            alu_op: AluOp::Add,
        }
    }

    pub fn reset_to(&mut self, ssp: u32, pc: u32) {
        self.regs.ssp = ssp;
        self.regs.pc = pc;
        self.regs.sr = 0x2700; // Supervisor, interrupts masked
        self.state = State::Idle;
        self.in_followup = false;
        self.followup_tag = 0;
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::PromoteIRC);
    }

    pub fn consume_irc(&mut self) -> u16 {
        let val = self.irc;
        self.micro_ops.push_front(MicroOp::FetchIRC);
        val
    }

    pub fn tick<B: M68kBus>(&mut self, bus: &mut B, crystal_clock: u64) {
        if crystal_clock % 4 != 0 {
            return;
        }

        if matches!(self.state, State::Idle) {
            self.process_instant_ops(bus);

            if matches!(self.state, State::Idle) && self.micro_ops.is_empty() {
                self.start_next_instruction();
                self.process_instant_ops(bus);
            }

            if matches!(self.state, State::Idle) {
                if let Some(op) = self.micro_ops.pop() {
                    if op.is_bus() {
                        self.state = self.initiate_bus_cycle(op);
                    } else if let MicroOp::Internal(cycles) = op {
                        self.state = State::Internal { cycles };
                    }
                }
            }
        }

        match &mut self.state {
            State::Idle => {}
            State::Internal { cycles } => {
                if *cycles > 1 {
                    *cycles -= 1;
                } else {
                    self.state = State::Idle;
                    self.process_instant_ops(bus);
                }
            }
            State::BusCycle { op, addr, fc, is_read, is_word, data, cycle_count } => {
                *cycle_count += 1;
                if *cycle_count >= 4 {
                    match bus.poll_cycle(*addr, *fc, *is_read, *is_word, *data) {
                        BusStatus::Ready(read_data) => {
                            let completed_op = *op;
                            self.finish_bus_cycle(completed_op, read_data);
                            self.state = State::Idle;
                            self.process_instant_ops(bus);
                        }
                        BusStatus::Wait => {}
                        BusStatus::Error => {
                            self.state = State::Halted;
                        }
                    }
                }
            }
            State::Halted | State::Stopped => {}
        }
    }

    fn process_instant_ops<B: M68kBus>(&mut self, bus: &mut B) {
        while let Some(op) = self.micro_ops.front() {
            if op.is_instant() {
                let op = self.micro_ops.pop().unwrap();
                match op {
                    MicroOp::Execute => {
                        self.decode_and_execute();
                    }
                    MicroOp::PromoteIRC => {
                        self.start_next_instruction();
                    }
                    MicroOp::AssertReset => {
                        bus.reset();
                    }
                    _ => {}
                }
            } else {
                break;
            }
        }
    }

    fn start_next_instruction(&mut self) {
        self.ir = self.irc;
        self.instr_start_pc = self.irc_addr;
        self.in_followup = false;
        self.followup_tag = 0;
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::Execute);
    }

    fn decode_and_execute(&mut self) {
        if self.in_followup {
            self.continue_instruction();
            return;
        }

        let opcode = self.ir;

        // MOVE.B/W/L (00ss ddd mmm sss)
        if (opcode & 0xC000) == 0x0000 && (opcode & 0x3000) != 0 {
            let size = match (opcode >> 12) & 0x03 {
                1 => Size::Byte,
                2 => Size::Long,
                3 => Size::Word,
                _ => unreachable!(),
            };
            let src_mode_bits = (opcode >> 3) & 0x07;
            let src_reg = opcode & 0x07;
            let dst_reg = (opcode >> 9) & 0x07;
            let dst_mode_bits = (opcode >> 6) & 0x07;

            self.size = size;
            self.src_mode = AddrMode::decode(src_mode_bits as u8, src_reg as u8);
            self.dst_mode = AddrMode::decode(dst_mode_bits as u8, dst_reg as u8);
            
            self.in_followup = true;
            self.followup_tag = FOLLOWUP_MOVE_READ_SRC_DATA;
            self.calc_ea(self.src_mode.unwrap(), self.size);
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // ADD (1101 rrr dss mmm rrr)
        if (opcode & 0xF000) == 0xD000 {
            let reg = ((opcode >> 9) & 0x07) as u8;
            let opmode = (opcode >> 6) & 0x07;
            if opmode != 3 && opmode != 7 {
                let size = match opmode {
                    0 | 4 => Size::Byte,
                    1 | 5 => Size::Word,
                    2 | 6 => Size::Long,
                    _ => unreachable!(),
                };
                let to_reg = opmode < 4;
                let mode_bits = (opcode >> 3) & 0x07;
                let reg_bits = opcode & 0x07;
                let ea_mode = AddrMode::decode(mode_bits as u8, reg_bits as u8).unwrap();
                self.alu_op = AluOp::Add;
                self.size = size;
                self.src_mode = if to_reg { Some(ea_mode) } else { Some(AddrMode::DataReg(reg)) };
                self.dst_mode = if to_reg { Some(AddrMode::DataReg(reg)) } else { Some(ea_mode) };
                self.in_followup = true;
                self.followup_tag = FOLLOWUP_ALU_READ_SRC;
                self.calc_ea(self.src_mode.unwrap(), self.size);
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
        }

        // SUB (1001 rrr dss mmm rrr)
        if (opcode & 0xF000) == 0x9000 {
            let reg = ((opcode >> 9) & 0x07) as u8;
            let opmode = (opcode >> 6) & 0x07;
            if opmode != 3 && opmode != 7 {
                let size = match opmode {
                    0 | 4 => Size::Byte,
                    1 | 5 => Size::Word,
                    2 | 6 => Size::Long,
                    _ => unreachable!(),
                };
                let to_reg = opmode < 4;
                let mode_bits = (opcode >> 3) & 0x07;
                let reg_bits = opcode & 0x07;
                let ea_mode = AddrMode::decode(mode_bits as u8, reg_bits as u8).unwrap();
                self.alu_op = AluOp::Sub;
                self.size = size;
                self.src_mode = if to_reg { Some(ea_mode) } else { Some(AddrMode::DataReg(reg)) };
                self.dst_mode = if to_reg { Some(AddrMode::DataReg(reg)) } else { Some(ea_mode) };
                self.in_followup = true;
                self.followup_tag = FOLLOWUP_ALU_READ_SRC;
                self.calc_ea(self.src_mode.unwrap(), self.size);
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
        }

        // CMP (1011 rrr 0ss mmm rrr)
        if (opcode & 0xF100) == 0xB000 {
            let reg = ((opcode >> 9) & 0x07) as u8;
            let opmode = (opcode >> 6) & 0x07;
            if opmode < 3 {
                let size = match opmode {
                    0 => Size::Byte,
                    1 => Size::Word,
                    2 => Size::Long,
                    _ => unreachable!(),
                };
                let mode_bits = (opcode >> 3) & 0x07;
                let reg_bits = opcode & 0x07;
                let ea_mode = AddrMode::decode(mode_bits as u8, reg_bits as u8).unwrap();
                self.alu_op = AluOp::Cmp;
                self.size = size;
                self.src_mode = Some(ea_mode);
                self.dst_mode = Some(AddrMode::DataReg(reg));
                self.in_followup = true;
                self.followup_tag = FOLLOWUP_ALU_READ_SRC;
                self.calc_ea(self.src_mode.unwrap(), self.size);
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
        }

        // AND (1100 rrr dss mmm rrr)
        if (opcode & 0xF000) == 0xC000 {
            let reg = ((opcode >> 9) & 0x07) as u8;
            let opmode = (opcode >> 6) & 0x07;
            if opmode != 3 && opmode != 7 {
                let size = match opmode {
                    0 | 4 => Size::Byte,
                    1 | 5 => Size::Word,
                    2 | 6 => Size::Long,
                    _ => unreachable!(),
                };
                let to_reg = opmode < 4;
                let mode_bits = (opcode >> 3) & 0x07;
                let reg_bits = opcode & 0x07;
                let ea_mode = AddrMode::decode(mode_bits as u8, reg_bits as u8).unwrap();
                self.alu_op = AluOp::And;
                self.size = size;
                self.src_mode = if to_reg { Some(ea_mode) } else { Some(AddrMode::DataReg(reg)) };
                self.dst_mode = if to_reg { Some(AddrMode::DataReg(reg)) } else { Some(ea_mode) };
                self.in_followup = true;
                self.followup_tag = FOLLOWUP_ALU_READ_SRC;
                self.calc_ea(self.src_mode.unwrap(), self.size);
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
        }

        // OR (1000 rrr dss mmm rrr)
        if (opcode & 0xF000) == 0x8000 {
            let reg = ((opcode >> 9) & 0x07) as u8;
            let opmode = (opcode >> 6) & 0x07;
            if opmode != 3 && opmode != 7 {
                let size = match opmode {
                    0 | 4 => Size::Byte,
                    1 | 5 => Size::Word,
                    2 | 6 => Size::Long,
                    _ => unreachable!(),
                };
                let to_reg = opmode < 4;
                let mode_bits = (opcode >> 3) & 0x07;
                let reg_bits = opcode & 0x07;
                let ea_mode = AddrMode::decode(mode_bits as u8, reg_bits as u8).unwrap();
                self.alu_op = AluOp::Or;
                self.size = size;
                self.src_mode = if to_reg { Some(ea_mode) } else { Some(AddrMode::DataReg(reg)) };
                self.dst_mode = if to_reg { Some(AddrMode::DataReg(reg)) } else { Some(ea_mode) };
                self.in_followup = true;
                self.followup_tag = FOLLOWUP_ALU_READ_SRC;
                self.calc_ea(self.src_mode.unwrap(), self.size);
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
        }

        // BRA/Bcc (0110 cccc dddd dddd)
        if (opcode & 0xF000) == 0x6000 {
            let disp8 = (opcode & 0xFF) as i8;
            self.in_followup = true;
            self.followup_tag = FOLLOWUP_BCC_EXECUTE;
            if disp8 == 0 {
                self.micro_ops.push(MicroOp::FetchIRC);
            } else if disp8 == -1 {
                self.state = State::Halted;
            } else {
                self.data = disp8 as i32 as u32;
            }
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        match opcode {
            0x4E71 => { // NOP
            }
            0x4E70 => { // RESET
                if self.regs.is_supervisor() {
                    self.micro_ops.push(MicroOp::AssertReset);
                    self.micro_ops.push(MicroOp::Internal(124));
                } else {
                    self.state = State::Halted;
                }
            }
            _ => {
                self.state = State::Halted;
            }
        }
    }

    fn continue_instruction(&mut self) {
        match self.followup_tag {
            FOLLOWUP_BCC_EXECUTE => {
                let cond = ((self.ir >> 8) & 0x0F) as u8;
                let disp8 = (self.ir & 0xFF) as i8;
                let disp = if disp8 == 0 { self.consume_irc() as i16 as i32 } else { self.data as i32 };
                if self.check_condition(cond) {
                    let target = self.instr_start_pc.wrapping_add(2).wrapping_add(disp as u32);
                    self.regs.pc = target;
                    self.micro_ops.clear();
                    self.micro_ops.push(MicroOp::FetchIRC);
                    self.micro_ops.push(MicroOp::PromoteIRC);
                }
                self.in_followup = false;
            }
            FOLLOWUP_MOVE_READ_SRC_EA_LONG => {
                let lo = self.consume_irc();
                self.addr |= u32::from(lo);
                self.followup_tag = FOLLOWUP_MOVE_READ_SRC_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_MOVE_CALC_DST_EA_LONG => {
                let lo = self.consume_irc();
                self.addr |= u32::from(lo);
                self.followup_tag = FOLLOWUP_MOVE_WRITE_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_MOVE_READ_SRC_DATA_LONG => {
                let lo = self.consume_irc();
                self.data |= u32::from(lo);
                let is_alu = match self.ir & 0xF000 {
                    0xD000 | 0x9000 | 0xB000 | 0xC000 | 0x8000 => true,
                    _ => false,
                };
                if is_alu { self.followup_tag = FOLLOWUP_ALU_CALC_DST; } else { self.followup_tag = FOLLOWUP_MOVE_CALC_DST_EA; }
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_MOVE_READ_SRC_EA_DISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.regs.a(self.ea_reg as usize).wrapping_add(disp as u32);
                self.followup_tag = FOLLOWUP_MOVE_READ_SRC_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_MOVE_CALC_DST_EA_DISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.regs.a(self.ea_reg as usize).wrapping_add(disp as u32);
                self.followup_tag = FOLLOWUP_MOVE_WRITE_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_MOVE_READ_SRC_EA_PCDISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.ea_pc.wrapping_add(disp as u32);
                self.followup_tag = FOLLOWUP_MOVE_READ_SRC_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_MOVE_CALC_DST_EA_PCDISP => {
                let disp = self.consume_irc() as i16 as i32;
                self.addr = self.ea_pc.wrapping_add(disp as u32);
                self.followup_tag = FOLLOWUP_MOVE_WRITE_DATA;
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_MOVE_READ_SRC_DATA => {
                let src_mode = self.src_mode.unwrap();
                match src_mode {
                    AddrMode::DataReg(reg) => {
                        self.data = self.regs.d[reg as usize];
                        self.followup_tag = FOLLOWUP_MOVE_CALC_DST_EA;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::AddrReg(reg) => {
                        self.data = self.regs.a[reg as usize];
                        self.followup_tag = FOLLOWUP_MOVE_CALC_DST_EA;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::Immediate => {
                        let val = self.consume_irc();
                        if self.size == Size::Long {
                            self.data = u32::from(val) << 16;
                            self.followup_tag = FOLLOWUP_MOVE_READ_SRC_DATA_LONG;
                        } else {
                            self.data = u32::from(val);
                            self.followup_tag = FOLLOWUP_MOVE_CALC_DST_EA;
                        }
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    _ => {
                        self.followup_tag = FOLLOWUP_MOVE_CALC_DST_EA;
                        self.queue_read_ops(self.size);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
            }
            FOLLOWUP_MOVE_CALC_DST_EA => {
                if self.calc_ea(self.dst_mode.unwrap(), self.size) {
                    self.followup_tag = FOLLOWUP_MOVE_WRITE_DATA;
                }
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_MOVE_WRITE_DATA => {
                let dst_mode = self.dst_mode.unwrap();
                match dst_mode {
                    AddrMode::DataReg(reg) => {
                        let val = self.data;
                        self.set_flags_move(val, self.size);
                        let reg_val = &mut self.regs.d[reg as usize];
                        *reg_val = match self.size {
                            Size::Byte => (*reg_val & 0xFFFF_FF00) | (val & 0xFF),
                            Size::Word => (*reg_val & 0xFFFF_0000) | (val & 0xFFFF),
                            Size::Long => val,
                        };
                        self.in_followup = false;
                    }
                    AddrMode::AddrReg(reg) => {
                        let val = self.data;
                        self.regs.a[reg as usize] = if self.size == Size::Word { (val as i16 as i32) as u32 } else { val };
                        self.in_followup = false;
                    }
                    _ => {
                        self.set_flags_move(self.data, self.size);
                        self.queue_write_ops(self.size);
                        self.in_followup = false;
                    }
                }
            }
            FOLLOWUP_ALU_READ_SRC => {
                let src_mode = self.src_mode.unwrap();
                match src_mode {
                    AddrMode::DataReg(reg) => {
                        self.data = self.regs.d[reg as usize];
                        self.followup_tag = FOLLOWUP_ALU_CALC_DST;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::AddrReg(reg) => {
                        self.data = self.regs.a[reg as usize];
                        self.followup_tag = FOLLOWUP_ALU_CALC_DST;
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    AddrMode::Immediate => {
                        let val = self.consume_irc();
                        if self.size == Size::Long {
                            self.data = u32::from(val) << 16;
                            self.followup_tag = FOLLOWUP_MOVE_READ_SRC_DATA_LONG;
                        } else {
                            self.data = u32::from(val);
                            self.followup_tag = FOLLOWUP_ALU_CALC_DST;
                        }
                        self.micro_ops.push(MicroOp::Execute);
                    }
                    _ => {
                        self.followup_tag = FOLLOWUP_ALU_CALC_DST;
                        self.queue_read_ops(self.size);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
            }
            FOLLOWUP_ALU_CALC_DST => {
                self.ea_pc = self.data; // Store src_data
                if self.calc_ea(self.dst_mode.unwrap(), self.size) {
                    self.followup_tag = FOLLOWUP_ALU_EXECUTE;
                }
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_ALU_EXECUTE => {
                let src_val = self.ea_pc;
                let dst_mode = self.dst_mode.unwrap();
                match dst_mode {
                    AddrMode::DataReg(reg) => {
                        let dst_val = self.regs.d[reg as usize];
                        let res = self.exec_alu(self.alu_op, src_val, dst_val, self.size);
                        if self.alu_op != AluOp::Cmp {
                            let reg_val = &mut self.regs.d[reg as usize];
                            *reg_val = match self.size {
                                Size::Byte => (*reg_val & 0xFFFF_FF00) | (res & 0xFF),
                                Size::Word => (*reg_val & 0xFFFF_0000) | (res & 0xFFFF),
                                Size::Long => res,
                            };
                        }
                        self.in_followup = false;
                    }
                    _ => { self.state = State::Halted; }
                }
            }
            _ => { self.in_followup = false; }
        }
    }

    fn calc_ea(&mut self, mode: AddrMode, _size: Size) -> bool {
        match mode {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate => true,
            AddrMode::AddrInd(reg) => {
                self.addr = self.regs.a(reg as usize);
                true
            }
            AddrMode::AddrIndPostInc(reg) => {
                self.addr = self.regs.a(reg as usize);
                let step = if reg == 7 && self.size == Size::Byte { 2 } else { self.size.bytes() as u32 };
                self.regs.set_a(reg as usize, self.addr.wrapping_add(step));
                true
            }
            AddrMode::AddrIndPreDec(reg) => {
                let step = if reg == 7 && self.size == Size::Byte { 2 } else { self.size.bytes() as u32 };
                self.addr = self.regs.a(reg as usize).wrapping_sub(step);
                self.regs.set_a(reg as usize, self.addr);
                true
            }
            AddrMode::AddrIndDisp(reg) => {
                self.ea_reg = reg;
                self.followup_tag = match self.followup_tag {
                    FOLLOWUP_MOVE_READ_SRC_DATA => FOLLOWUP_MOVE_READ_SRC_EA_DISP,
                    FOLLOWUP_MOVE_CALC_DST_EA => FOLLOWUP_MOVE_CALC_DST_EA_DISP,
                    FOLLOWUP_ALU_READ_SRC => FOLLOWUP_MOVE_READ_SRC_EA_DISP,
                    FOLLOWUP_ALU_CALC_DST => FOLLOWUP_MOVE_CALC_DST_EA_DISP,
                    _ => self.followup_tag,
                };
                false
            }
            AddrMode::AbsShort => {
                let val = self.consume_irc();
                self.addr = (val as i16 as i32) as u32;
                true
            }
            AddrMode::AbsLong => {
                let hi = self.consume_irc();
                self.addr = u32::from(hi) << 16;
                self.followup_tag = match self.followup_tag {
                    FOLLOWUP_MOVE_READ_SRC_DATA => FOLLOWUP_MOVE_READ_SRC_EA_LONG,
                    FOLLOWUP_MOVE_CALC_DST_EA => FOLLOWUP_MOVE_CALC_DST_EA_LONG,
                    FOLLOWUP_ALU_READ_SRC => FOLLOWUP_MOVE_READ_SRC_EA_LONG,
                    FOLLOWUP_ALU_CALC_DST => FOLLOWUP_MOVE_CALC_DST_EA_LONG,
                    _ => self.followup_tag,
                };
                false
            }
            AddrMode::PcDisp => {
                self.ea_pc = self.irc_addr;
                self.followup_tag = match self.followup_tag {
                    FOLLOWUP_MOVE_READ_SRC_DATA => FOLLOWUP_MOVE_READ_SRC_EA_PCDISP,
                    FOLLOWUP_MOVE_CALC_DST_EA => FOLLOWUP_MOVE_CALC_DST_EA_PCDISP,
                    FOLLOWUP_ALU_READ_SRC => FOLLOWUP_MOVE_READ_SRC_EA_PCDISP,
                    FOLLOWUP_ALU_CALC_DST => FOLLOWUP_MOVE_CALC_DST_EA_PCDISP,
                    _ => self.followup_tag,
                };
                false
            }
            _ => { self.state = State::Halted; true }
        }
    }

    fn queue_read_ops(&mut self, size: Size) {
        match size {
            Size::Byte | Size::Word => { self.micro_ops.push(MicroOp::ReadWord); }
            Size::Long => { self.micro_ops.push(MicroOp::ReadLongHi); self.micro_ops.push(MicroOp::ReadLongLo); }
        }
    }

    fn queue_write_ops(&mut self, size: Size) {
        match size {
            Size::Byte | Size::Word => { self.micro_ops.push(MicroOp::WriteWord); }
            Size::Long => { self.micro_ops.push(MicroOp::WriteLongHi); self.micro_ops.push(MicroOp::WriteLongLo); }
        }
    }

    fn initiate_bus_cycle(&self, op: MicroOp) -> State {
        let is_supervisor = self.regs.is_supervisor();
        let fc_prog = if is_supervisor { FunctionCode::SupervisorProgram } else { FunctionCode::UserProgram };
        let fc_data = if is_supervisor { FunctionCode::SupervisorData } else { FunctionCode::UserData };
        let (addr, fc, is_read, is_word, data) = match op {
            MicroOp::FetchIRC => (self.regs.pc, fc_prog, true, true, None),
            MicroOp::ReadByte => (self.addr, fc_data, true, false, None),
            MicroOp::ReadWord => (self.addr, fc_data, true, true, None),
            MicroOp::ReadLongHi => (self.addr, fc_data, true, true, None),
            MicroOp::ReadLongLo => (self.addr.wrapping_add(2), fc_data, true, true, None),
            MicroOp::WriteByte => (self.addr, fc_data, false, false, Some(u16::from(self.data as u8))),
            MicroOp::WriteWord => (self.addr, fc_data, false, true, Some(self.data as u16)),
            MicroOp::WriteLongHi => (self.addr, fc_data, false, true, Some((self.data >> 16) as u16)),
            MicroOp::WriteLongLo => (self.addr.wrapping_add(2), fc_data, false, true, Some((self.data & 0xFFFF) as u16)),
            MicroOp::PushWord => (self.regs.active_sp().wrapping_sub(2), fc_data, false, true, Some(self.data as u16)),
            MicroOp::PushLongHi => (self.regs.active_sp().wrapping_sub(4), fc_data, false, true, Some((self.data >> 16) as u16)),
            MicroOp::PushLongLo => (self.regs.active_sp().wrapping_sub(2), fc_data, false, true, Some((self.data & 0xFFFF) as u16)),
            MicroOp::PopWord => (self.regs.active_sp(), fc_data, true, true, None),
            MicroOp::PopLongHi => (self.regs.active_sp(), fc_data, true, true, None),
            MicroOp::PopLongLo => (self.regs.active_sp().wrapping_add(2), fc_data, true, true, None),
            _ => panic!("Non-bus op: {:?}", op),
        };
        State::BusCycle { op, addr, fc, is_read, is_word, data, cycle_count: 0 }
    }

    fn finish_bus_cycle(&mut self, op: MicroOp, read_data: u16) {
        match op {
            MicroOp::FetchIRC => { self.irc = read_data; self.irc_addr = self.regs.pc; self.regs.pc = self.regs.pc.wrapping_add(2); }
            MicroOp::ReadWord | MicroOp::ReadByte => { self.data = u32::from(read_data); }
            _ => {}
        }
    }

    fn check_condition(&self, cond: u8) -> bool {
        use crate::flags::{N, Z, V, C};
        let sr = self.regs.sr;
        let n = sr & N != 0; let z = sr & Z != 0; let v = sr & V != 0; let c = sr & C != 0;
        match cond & 0x0F {
            0 => true, 1 => false, 2 => !c && !z, 3 => c || z, 4 => !c, 5 => c, 6 => !z, 7 => z,
            8 => !v, 9 => v, 10 => !n, 11 => n, 12 => (n && v) || (!n && !v), 13 => (n && !v) || (!n && v),
            14 => (n && v && !z) || (!n && !v && !z), 15 => z || (n && !v) || (!n && v), _ => unreachable!(),
        }
    }

    fn exec_alu(&mut self, op: AluOp, src: u32, dst: u32, size: Size) -> u32 {
        let mask = size.mask();
        let s = src & mask; let d = dst & mask;
        match op {
            AluOp::Add => { let res = s.wrapping_add(d) & mask; self.set_flags_add(s, d, res, size); res }
            AluOp::Sub | AluOp::Cmp => { let res = d.wrapping_sub(s) & mask; self.set_flags_sub(s, d, res, size); res }
            AluOp::And => { let res = s & d; self.set_flags_logic(res, size); res }
            AluOp::Or => { let res = s | d; self.set_flags_logic(res, size); res }
            AluOp::Eor => { let res = s ^ d; self.set_flags_logic(res, size); res }
        }
    }

    fn set_flags_add(&mut self, s: u32, d: u32, r: u32, size: Size) {
        use crate::flags::{N, Z, V, C, X};
        self.regs.sr &= !(N | Z | V | C | X);
        let msb = size.msb_mask();
        if r == 0 { self.regs.sr |= Z; }
        if r & msb != 0 { self.regs.sr |= N; }
        let sm = s & msb != 0; let dm = d & msb != 0; let rm = r & msb != 0;
        if (sm && dm) || (!rm && (sm || dm)) { self.regs.sr |= C | X; }
        if (sm && dm && !rm) || (!sm && !dm && rm) { self.regs.sr |= V; }
    }

    fn set_flags_sub(&mut self, s: u32, d: u32, r: u32, size: Size) {
        use crate::flags::{N, Z, V, C, X};
        self.regs.sr &= !(N | Z | V | C | X);
        let msb = size.msb_mask();
        if r == 0 { self.regs.sr |= Z; }
        if r & msb != 0 { self.regs.sr |= N; }
        let sm = s & msb != 0; let dm = d & msb != 0; let rm = r & msb != 0;
        if (sm && !dm) || (rm && (sm || !dm)) { self.regs.sr |= C | X; }
        if (!sm && dm && !rm) || (sm && !dm && rm) { self.regs.sr |= V; }
    }

    fn set_flags_logic(&mut self, r: u32, size: Size) {
        use crate::flags::{N, Z, V, C};
        self.regs.sr &= !(N | Z | V | C);
        let msb = size.msb_mask();
        if r == 0 { self.regs.sr |= Z; }
        if r & msb != 0 { self.regs.sr |= N; }
    }

    fn set_flags_move(&mut self, val: u32, size: Size) {
        use crate::flags::{N, Z, V, C};
        self.regs.sr &= !(N | Z | V | C);
        let mask = size.mask();
        let msb = size.msb_mask();
        let v = val & mask;
        if v == 0 { self.regs.sr |= Z; }
        if v & msb != 0 { self.regs.sr |= N; }
    }
}
