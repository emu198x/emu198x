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
}

const FOLLOWUP_MOVE_READ_SRC_DATA: u8 = 1;
const FOLLOWUP_MOVE_CALC_DST_EA: u8 = 2;
const FOLLOWUP_MOVE_WRITE_DATA: u8 = 3;

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
        }
    }

    pub fn setup_prefetch(&mut self, opcode: u16, irc: u16) {
        self.ir = opcode;
        self.irc = irc;
        self.irc_addr = self.regs.pc.wrapping_sub(2);
        self.instr_start_pc = self.regs.pc.wrapping_sub(4);
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::Execute);
        self.state = State::Idle;
        self.in_followup = false;
        self.followup_tag = 0;
    }

    pub fn reset_to(&mut self, ssp: u32, pc: u32) {
        self.regs.ssp = ssp;
        self.regs.pc = pc;
        self.regs.sr = 0x2700; // Supervisor, interrupts masked
        self.state = State::Idle;
        self.in_followup = false;
        self.followup_tag = 0;
        self.micro_ops.clear();
        
        // At RESET, the 68000 does:
        // 1. Read initial SSP (handled by caller)
        // 2. Read initial PC (handled by caller)
        // 3. Fetch first opcode into IRC
        // 4. Promote IRC to IR, fetch next word into IRC, Execute IR
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::PromoteIRC);
    }

    /// Consume IRC as an extension word and queue FetchIRC to refill.
    pub fn consume_irc(&mut self) -> u16 {
        let val = self.irc;
        self.micro_ops.push(MicroOp::FetchIRC);
        val
    }

    pub fn tick<B: M68kBus>(&mut self, bus: &mut B, crystal_clock: u64) {
        if crystal_clock % 4 != 0 {
            return;
        }

        // 1. If idle, try to start something
        if matches!(self.state, State::Idle) {
            // Process leading instant ops
            self.process_instant_ops(bus);

            // If still idle and queue empty, start next instruction
            if matches!(self.state, State::Idle) && self.micro_ops.is_empty() {
                self.start_next_instruction();
                self.process_instant_ops(bus);
            }

            // If still idle, pick up the next timed/bus op
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

        // 2. Process current state
        match &mut self.state {
            State::Idle => {}
            State::Internal { cycles } => {
                if *cycles > 1 {
                    *cycles -= 1;
                } else {
                    self.state = State::Idle;
                    // Trailing instant ops after internal delay
                    self.process_instant_ops(bus);
                }
            }
            State::BusCycle { op, addr, fc, is_read, is_word, data, cycle_count } => {
                *cycle_count += 1;
                
                // Poll for /DTACK at cycle 4+
                if *cycle_count >= 4 {
                    match bus.poll_cycle(*addr, *fc, *is_read, *is_word, *data) {
                        BusStatus::Ready(read_data) => {
                            let completed_op = *op;
                            self.finish_bus_cycle(completed_op, read_data);
                            self.state = State::Idle;
                            // 3. Process trailing instant ops (e.g. Execute after FetchIRC)
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
        // Promote IRC to IR
        self.ir = self.irc;
        self.instr_start_pc = self.irc_addr;
        self.in_followup = false;
        self.followup_tag = 0;
        
        // Refill IRC and then Execute the promoted instruction
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::Execute);
    }

    fn decode_and_execute(&mut self) {
        if self.in_followup {
            self.continue_instruction();
            return;
        }

        let opcode = self.ir;
        // println!("  DECODE: opcode=${:04X} at instr_start=${:08X}", opcode, self.instr_start_pc);

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
            
            // Step 1: Calculate Source EA
            self.in_followup = true;
            self.followup_tag = FOLLOWUP_MOVE_READ_SRC_DATA;
            self.calc_ea(self.src_mode.unwrap(), self.size);
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        match opcode {
            0x4E71 => { // NOP
                // println!("  EXEC: NOP");
            }
            0x4E70 => { // RESET
                if self.regs.is_supervisor() {
                    // println!("  EXEC: RESET");
                    self.micro_ops.push(MicroOp::AssertReset);
                    self.micro_ops.push(MicroOp::Internal(124));
                } else {
                    // println!("  HALT: RESET in user mode");
                    self.state = State::Halted;
                }
            }
            _ => {
                // println!("  HALT: Unknown opcode ${:04X}", opcode);
                self.state = State::Halted;
            }
        }
    }

    fn continue_instruction(&mut self) {
        match self.followup_tag {
            FOLLOWUP_MOVE_READ_SRC_DATA => {
                let src_mode = self.src_mode.unwrap();
                match src_mode {
                    AddrMode::DataReg(reg) => {
                        let val = self.regs.d[reg as usize];
                        self.data = val;
                        self.followup_tag = FOLLOWUP_MOVE_CALC_DST_EA;
                        self.continue_instruction();
                    }
                    AddrMode::AddrReg(reg) => {
                        let val = self.regs.a[reg as usize];
                        self.data = val;
                        self.followup_tag = FOLLOWUP_MOVE_CALC_DST_EA;
                        self.continue_instruction();
                    }
                    AddrMode::Immediate => {
                        let val = self.consume_irc();
                        if self.size == Size::Long {
                            self.data = (u32::from(val) << 16) | u32::from(self.consume_irc());
                        } else {
                            self.data = u32::from(val);
                        }
                        self.followup_tag = FOLLOWUP_MOVE_CALC_DST_EA;
                        self.continue_instruction();
                    }
                    _ => {
                        self.followup_tag = FOLLOWUP_MOVE_CALC_DST_EA;
                        self.queue_read_ops(self.size);
                        self.micro_ops.push(MicroOp::Execute);
                    }
                }
            }
            FOLLOWUP_MOVE_CALC_DST_EA => {
                self.followup_tag = FOLLOWUP_MOVE_WRITE_DATA;
                self.calc_ea(self.dst_mode.unwrap(), self.size);
                self.micro_ops.push(MicroOp::Execute);
            }
            FOLLOWUP_MOVE_WRITE_DATA => {
                let dst_mode = self.dst_mode.unwrap();
                match dst_mode {
                    AddrMode::DataReg(reg) => {
                        let val = self.data;
                        let size = self.size;
                        let reg_val = &mut self.regs.d[reg as usize];
                        *reg_val = match size {
                            Size::Byte => (*reg_val & 0xFFFF_FF00) | (val & 0xFF),
                            Size::Word => (*reg_val & 0xFFFF_0000) | (val & 0xFFFF),
                            Size::Long => val,
                        };
                        self.in_followup = false;
                    }
                    AddrMode::AddrReg(reg) => {
                        let val = self.data;
                        // MOVEA always sign-extends word to long and doesn't set flags.
                        // (Wait, MOVE to AddrReg is MOVEA).
                        self.regs.a[reg as usize] = if self.size == Size::Word {
                            (val as i16 as i32) as u32
                        } else {
                            val
                        };
                        self.in_followup = false;
                    }
                    _ => {
                        self.queue_write_ops(self.size);
                        self.in_followup = false;
                    }
                }
            }
            _ => {
                self.in_followup = false;
            }
        }
    }

    fn calc_ea(&mut self, mode: AddrMode, _size: Size) {
        match mode {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate => {
                // Instant
            }
            AddrMode::AddrInd(reg) => {
                self.addr = self.regs.a(reg as usize);
            }
            _ => {
                // To be implemented as needed
                println!("HALT: AddrMode {:?} not yet implemented in calc_ea", mode);
                self.state = State::Halted;
            }
        }
    }

    fn queue_read_ops(&mut self, size: Size) {
        match size {
            Size::Byte | Size::Word => {
                self.micro_ops.push(MicroOp::ReadWord); // Amiga is word-bus, but CPU handles byte
            }
            Size::Long => {
                self.micro_ops.push(MicroOp::ReadLongHi);
                self.micro_ops.push(MicroOp::ReadLongLo);
            }
        }
    }

    fn queue_write_ops(&mut self, size: Size) {
        match size {
            Size::Byte | Size::Word => {
                self.micro_ops.push(MicroOp::WriteWord);
            }
            Size::Long => {
                self.micro_ops.push(MicroOp::WriteLongHi);
                self.micro_ops.push(MicroOp::WriteLongLo);
            }
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
            _ => panic!("Non-bus op in initiate_bus_cycle: {:?}", op),
        };

        State::BusCycle {
            op,
            addr,
            fc,
            is_read,
            is_word,
            data,
            cycle_count: 0,
        }
    }

    fn finish_bus_cycle(&mut self, op: MicroOp, read_data: u16) {
        match op {
            MicroOp::FetchIRC => {
                self.irc = read_data;
                // IRC was fetched from current PC, which is then incremented
                self.irc_addr = self.regs.pc;
                self.regs.pc = self.regs.pc.wrapping_add(2);
            }
            MicroOp::ReadWord | MicroOp::ReadByte => {
                self.data = u32::from(read_data);
            }
            _ => {}
        }
    }
}
