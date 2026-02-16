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

const FOLLOWUP_MOVE_STORE: u8 = 1;

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

        // println!("CPU Tick: PC=${:08X} IR=${:04X} IRC=${:04X} State={:?} Queue={:?}", self.regs.pc, self.ir, self.irc, match self.state {
        //     State::Idle => "Idle",
        //     State::Internal { .. } => "Internal",
        //     State::BusCycle { .. } => "BusCycle",
        //     State::Halted => "Halted",
        //     State::Stopped => "Stopped",
        // }, self.micro_ops.debug_contents());

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

        // MOVE.W (An), Dm (0011 ddd 000 010 sss)
        if (opcode & 0xF1F8) == 0x3010 {
            let src_reg = (opcode & 0x0007) as u8;
            let dst_reg = ((opcode >> 9) & 0x0007) as u8;
            
            self.addr = self.regs.a(src_reg as usize);
            self.size = Size::Word;
            self.dst_mode = Some(AddrMode::DataReg(dst_reg));
            
            self.in_followup = true;
            self.followup_tag = FOLLOWUP_MOVE_STORE;
            self.micro_ops.push(MicroOp::ReadWord);
            self.micro_ops.push(MicroOp::Execute);
            return;
        }

        // MOVE.W Dn, Dm (0011 ddd 000 sss)
        if (opcode & 0xF1C0) == 0x3000 {
            let src_reg = (opcode & 0x0007) as usize;
            let dst_reg = ((opcode >> 9) & 0x0007) as usize;
            let val = self.regs.d[src_reg] as u16;
            self.regs.d[dst_reg] = (self.regs.d[dst_reg] & 0xFFFF_0000) | u32::from(val);
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
            FOLLOWUP_MOVE_STORE => {
                if let Some(AddrMode::DataReg(reg)) = self.dst_mode {
                    let val = self.data;
                    let size = self.size;
                    let reg_val = &mut self.regs.d[reg as usize];
                    *reg_val = match size {
                        Size::Byte => (*reg_val & 0xFFFF_FF00) | (val & 0xFF),
                        Size::Word => (*reg_val & 0xFFFF_0000) | (val & 0xFFFF),
                        Size::Long => val,
                    };
                }
                self.in_followup = false;
            }
            _ => {}
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
