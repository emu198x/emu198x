//! Motorola 68000 CPU core with Reactive Bus State Machine.

use crate::bus::{M68kBus, BusStatus, FunctionCode};
use crate::registers::Registers;

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
}

impl MicroOp {
    pub fn is_instant(self) -> bool {
        matches!(self, Self::AssertReset | Self::Execute | Self::Internal(0))
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
}

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
        // Standard 68k prefetch after reset
        self.micro_ops.push(MicroOp::FetchIRC); // Fetches first opcode into IRC
        self.micro_ops.push(MicroOp::Execute);  // Will move IRC to IR and fetch next
    }

    pub fn tick<B: M68kBus>(&mut self, bus: &mut B, crystal_clock: u64) {
        if crystal_clock % 4 != 0 {
            return;
        }

        // 1. Process all instant ops at the front of the queue
        self.process_instant_ops(bus);

        // 2. If idle and queue is empty, start next instruction
        if matches!(self.state, State::Idle) && self.micro_ops.is_empty() {
            self.start_next_instruction();
            self.process_instant_ops(bus);
        }

        // 3. Process current state
        match &mut self.state {
            State::Idle => {
                if let Some(op) = self.micro_ops.pop() {
                    if op.is_bus() {
                        self.state = self.initiate_bus_cycle(op);
                    } else if let MicroOp::Internal(cycles) = op {
                        self.state = State::Internal { cycles };
                    }
                }
            }
            State::Internal { cycles } => {
                if *cycles > 1 {
                    *cycles -= 1;
                } else {
                    self.state = State::Idle;
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
                            // Process any instant ops that follow the bus cycle
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
        // IR <- IRC, queue FetchIRC + Execute
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
        // ... rest of the code
    }

    fn continue_instruction(&mut self) {
        // Multi-stage logic will go here
    }


        // MOVE.W Dn, Dm (0011 ddd 000 sss)
        if (opcode & 0xF1C0) == 0x3000 {
            let src_reg = (opcode & 0x0007) as usize;
            let dst_reg = ((opcode >> 9) & 0x0007) as usize;
            let val = self.regs.d[src_reg] as u16;
            self.regs.d[dst_reg] = (self.regs.d[dst_reg] & 0xFFFF_0000) | u32::from(val);
            // Flags update would go here
            return;
        }

        match opcode {
            0x4E71 => { // NOP
                // No action
            }
            0x4E70 => { // RESET
                if self.regs.is_supervisor() {
                    self.micro_ops.push(MicroOp::AssertReset);
                    self.micro_ops.push(MicroOp::Internal(124));
                } else {
                    // Privilege violation stub
                    self.state = State::Halted;
                }
            }
            _ => {
                // Illegal instruction stub
                self.state = State::Halted;
            }
        }
    }

    fn initiate_bus_cycle(&self, op: MicroOp) -> State {
        let is_supervisor = self.regs.is_supervisor();
        
        let (addr, fc, is_read, is_word, data) = match op {
            MicroOp::FetchIRC => (
                self.regs.pc,
                if is_supervisor { FunctionCode::SupervisorProgram } else { FunctionCode::UserProgram },
                true,
                true,
                None
            ),
            MicroOp::ReadWord => (
                self.addr,
                if is_supervisor { FunctionCode::SupervisorData } else { FunctionCode::UserData },
                true,
                true,
                None
            ),
            MicroOp::WriteWord => (
                self.addr,
                if is_supervisor { FunctionCode::SupervisorData } else { FunctionCode::UserData },
                false,
                true,
                Some(self.data as u16)
            ),
            _ => (0, FunctionCode::UserData, true, true, None), // Stub for others
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
