//! Motorola 68000 CPU core with per-cycle execution.
//!
//! This module implements the 68000 variant of the M68k family. It uses the
//! `M68kBus` trait for word-level bus access with function codes and wait states.
//!
//! Every instruction is decoded into a recipe — a sequence of `RecipeOp`s that
//! expand into cycle-accurate `MicroOp`s. There is no legacy execution path.

mod decode;
mod ea;
mod exceptions;
mod execute_shift;
mod microcode;
mod observable;
mod recipe;
mod timing;

use emu_core::Ticks;

use crate::bus::{FunctionCode, M68kBus};
use crate::common::flags::{Status, C, N, S, V, X, Z};
use crate::common::registers::Registers;

pub use crate::common::addressing::AddrMode;
pub use crate::common::alu::Size;
pub use microcode::{MicroOp, MicroOpQueue};
pub use recipe::{EaSide, RecipeAlu, RecipeOp, RecipeUnary};
use recipe::RECIPE_MAX_OPS;

// Re-export flags for child modules.
pub(crate) use crate::common::flags;

/// 68000 CPU state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    /// Fetching instruction opcode.
    FetchOpcode,
    /// Executing instruction micro-ops.
    Execute,
    /// Processing exception.
    Exception,
    /// Halted (double bus fault).
    Halted,
    /// Stopped (STOP instruction, waiting for interrupt).
    Stopped,
}

/// Motorola 68000 CPU.
///
/// The CPU does not own the bus. Instead, the bus is passed to `tick()` on
/// each clock cycle. This allows the bus to be shared with other components
/// (e.g., custom chips) that may also need bus access.
pub struct Cpu68000 {
    // === Registers ===
    /// CPU registers.
    pub regs: Registers,

    // === Execution state ===
    /// Current CPU state.
    pub(crate) state: State,
    /// Queue of micro-operations for current instruction.
    pub(crate) micro_ops: MicroOpQueue,
    /// Cycle counter within current micro-op.
    pub(crate) cycle: u8,
    /// Total internal cycles for current Internal micro-op.
    pub(crate) internal_cycles: u8,

    // === Instruction decode state ===
    /// Current opcode word.
    pub(crate) opcode: u16,
    /// Extension words.
    pub(crate) ext_words: [u16; 4],
    /// Number of extension words read.
    pub(crate) ext_count: u8,
    /// Index into `ext_words` for current processing.
    pub(crate) ext_idx: u8,
    /// Source addressing mode.
    pub(crate) src_mode: Option<AddrMode>,
    /// Destination addressing mode.
    pub(crate) dst_mode: Option<AddrMode>,

    // === Temporary storage ===
    /// Effective address calculated.
    pub(crate) addr: u32,
    /// Data being transferred.
    pub(crate) data: u32,
    /// Operation size.
    pub(crate) size: Size,
    /// Second address (for some instructions).
    pub(crate) addr2: u32,
    /// Second data value (for some instructions).
    pub(crate) data2: u32,

    // === MOVEM state ===
    /// True if MOVEM is using predecrement mode.
    pub(crate) movem_predec: bool,
    /// True if MOVEM is using postincrement mode.
    pub(crate) movem_postinc: bool,
    /// Long transfer phase / generic micro-phase tracker.
    pub(crate) movem_long_phase: u8,

    // === Recipe state ===
    pub(crate) recipe_ops: [RecipeOp; RECIPE_MAX_OPS],
    pub(crate) recipe_len: u8,
    pub(crate) recipe_idx: u8,
    pub(crate) recipe_src_addr: u32,
    pub(crate) recipe_dst_addr: u32,
    pub(crate) recipe_src_is_reg: bool,
    pub(crate) recipe_dst_is_reg: bool,
    pub(crate) recipe_src_pc_at_ext: u32,
    pub(crate) recipe_dst_pc_at_ext: u32,
    pub(crate) recipe_imm: u32,

    // === Exception state ===
    /// PC at instruction start (after opcode fetch, before extension words consumed).
    pub(crate) instr_start_pc: u32,
    /// Pending exception vector number.
    pub(crate) pending_exception: Option<u8>,
    /// Current exception being processed.
    pub(crate) current_exception: Option<u8>,
    /// Fault address for address/bus error exceptions.
    pub(crate) fault_addr: u32,
    /// True if fault was during read, false for write.
    pub(crate) fault_read: bool,
    /// True if fault was during instruction fetch, false for data access.
    pub(crate) fault_in_instruction: bool,
    /// Function code at time of fault.
    pub(crate) fault_fc: u8,
    /// Access info word for group 0 exception frame.
    pub(crate) group0_access_info: u16,
    /// True if ExtendMemOp has performed its pre-decrements.
    pub(crate) extend_predec_done: bool,
    /// Deferred post-increment: (register, amount).
    pub(crate) deferred_postinc: Option<(u8, u32)>,
    /// Override PC value for exception frame.
    pub(crate) exception_pc_override: Option<u32>,
    /// True when the current EA was computed from a PC-relative mode.
    pub(crate) program_space_access: bool,

    // === Interrupt state ===
    /// Pending interrupt level (1-7), 0 = none.
    int_pending: u8,

    // === Timing ===
    /// Total clock cycles elapsed.
    pub(crate) total_cycles: Ticks,
    /// Wait cycles accumulated from bus contention during current micro-op.
    wait_cycles: u8,
}

impl Cpu68000 {
    /// Create a new 68000 CPU.
    #[must_use]
    pub fn new() -> Self {
        let mut cpu = Self {
            regs: Registers::new(),
            state: State::FetchOpcode,
            micro_ops: MicroOpQueue::new(),
            cycle: 0,
            internal_cycles: 0,
            opcode: 0,
            ext_words: [0; 4],
            ext_count: 0,
            ext_idx: 0,
            src_mode: None,
            dst_mode: None,
            addr: 0,
            data: 0,
            size: Size::Word,
            addr2: 0,
            data2: 0,
            movem_predec: false,
            movem_postinc: false,
            movem_long_phase: 0,
            recipe_ops: [RecipeOp::LoadImm; RECIPE_MAX_OPS],
            recipe_len: 0,
            recipe_idx: 0,
            recipe_src_addr: 0,
            recipe_dst_addr: 0,
            recipe_src_is_reg: false,
            recipe_dst_is_reg: false,
            recipe_src_pc_at_ext: 0,
            recipe_dst_pc_at_ext: 0,
            recipe_imm: 0,
            instr_start_pc: 0,
            pending_exception: None,
            current_exception: None,
            fault_addr: 0,
            fault_read: true,
            fault_in_instruction: false,
            fault_fc: 0,
            group0_access_info: 0,
            extend_predec_done: false,
            deferred_postinc: None,
            exception_pc_override: None,
            program_space_access: false,
            int_pending: 0,
            total_cycles: Ticks::ZERO,
            wait_cycles: 0,
        };
        cpu.micro_ops.push(MicroOp::FetchOpcode);
        cpu
    }

    /// Total clock cycles elapsed since creation.
    #[must_use]
    pub const fn total_cycles(&self) -> Ticks {
        self.total_cycles
    }

    /// Set the interrupt priority level (IPL) lines.
    ///
    /// Level 0 means no interrupt. Levels 1-6 are maskable.
    /// Level 7 is non-maskable (NMI).
    pub fn set_ipl(&mut self, level: u8) {
        self.int_pending = level & 0x07;
    }

    /// Set up CPU with a pre-fetched opcode for single-step testing.
    ///
    /// Initialises the CPU as if the opcode has already been fetched,
    /// ready to execute. The first element of `ext_words_in` is IRC,
    /// followed by words from memory at PC, PC+2, etc.
    pub fn setup_prefetch(&mut self, opcode: u16, ext_words_in: &[u16]) {
        self.opcode = opcode;
        let count = ext_words_in.len().min(4);
        for i in 0..count {
            self.ext_words[i] = ext_words_in[i];
        }
        self.ext_count = count as u8;
        self.ext_idx = 0;
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::Execute);
        self.state = State::Execute;
        self.cycle = 0;
        self.instr_start_pc = self.regs.pc;
    }

    // === Bus access helpers ===

    /// Compute the function code for data access.
    fn data_fc(&self) -> FunctionCode {
        FunctionCode::from_flags(self.regs.is_supervisor(), false)
    }

    /// Compute the function code for program access.
    fn program_fc(&self) -> FunctionCode {
        FunctionCode::from_flags(self.regs.is_supervisor(), true)
    }

    /// Read byte from memory via M68kBus.
    fn read_byte<B: M68kBus>(&mut self, bus: &mut B, addr: u32) -> u8 {
        let fc = self.data_fc();
        let result = bus.read_byte(addr & 0x00FF_FFFF, fc);
        self.wait_cycles = self.wait_cycles.saturating_add(result.wait_cycles);
        result.data as u8
    }

    /// Read word from memory via M68kBus.
    fn read_word<B: M68kBus>(&mut self, bus: &mut B, addr: u32) -> u16 {
        let fc = self.data_fc();
        let result = bus.read_word(addr & 0x00FF_FFFE, fc);
        self.wait_cycles = self.wait_cycles.saturating_add(result.wait_cycles);
        result.data
    }

    /// Read long from memory via M68kBus (two word reads).
    fn read_long<B: M68kBus>(&mut self, bus: &mut B, addr: u32) -> u32 {
        let addr24 = addr & 0x00FF_FFFE;
        let hi = self.read_word(bus, addr24);
        let lo = self.read_word(bus, addr24 + 2);
        u32::from(hi) << 16 | u32::from(lo)
    }

    /// Write byte to memory via M68kBus.
    fn write_byte<B: M68kBus>(&mut self, bus: &mut B, addr: u32, value: u8) {
        let fc = self.data_fc();
        let result = bus.write_byte(addr & 0x00FF_FFFF, value, fc);
        self.wait_cycles = self.wait_cycles.saturating_add(result.wait_cycles);
    }

    /// Write word to memory via M68kBus.
    fn write_word<B: M68kBus>(&mut self, bus: &mut B, addr: u32, value: u16) {
        #[cfg(debug_assertions)]
        if (addr & 0x00FF_FFFE) < 8 {
            eprintln!("  CPU WRITE_WORD ${:06X} = ${value:04X} (PC=${:08X})", addr & 0x00FF_FFFE, self.instr_start_pc.wrapping_sub(2));
        }
        let fc = self.data_fc();
        let result = bus.write_word(addr & 0x00FF_FFFE, value, fc);
        self.wait_cycles = self.wait_cycles.saturating_add(result.wait_cycles);
    }

    // === Queue helpers ===

    /// Queue micro-ops for the next instruction fetch.
    fn queue_fetch(&mut self) {
        self.micro_ops.clear();
        self.state = State::FetchOpcode;
        self.ext_count = 0;
        self.ext_idx = 0;
        self.src_mode = None;
        self.dst_mode = None;
        self.recipe_reset();
        self.micro_ops.push(MicroOp::FetchOpcode);
    }

    /// Queue internal cycles.
    pub(crate) fn queue_internal(&mut self, cycles: u8) {
        self.internal_cycles = cycles;
        self.micro_ops.push(MicroOp::Internal);
    }

    /// Queue internal cycles (for jumps/branches that set PC directly).
    pub(crate) fn queue_internal_no_pc(&mut self, cycles: u8) {
        self.internal_cycles = cycles;
        self.micro_ops.push(MicroOp::Internal);
    }

    /// Set PC for a jump target.
    pub(crate) fn set_jump_pc(&mut self, target: u32) {
        self.regs.pc = target;
    }

    /// Queue memory read micro-ops for the given size.
    pub(crate) fn queue_read_ops(&mut self, size: Size) {
        match size {
            Size::Byte => self.micro_ops.push(MicroOp::ReadByte),
            Size::Word => self.micro_ops.push(MicroOp::ReadWord),
            Size::Long => {
                self.micro_ops.push(MicroOp::ReadLongHi);
                self.micro_ops.push(MicroOp::ReadLongLo);
            }
        }
    }

    /// Queue memory write micro-ops for the given size.
    pub(crate) fn queue_write_ops(&mut self, size: Size) {
        match size {
            Size::Byte => self.micro_ops.push(MicroOp::WriteByte),
            Size::Word => self.micro_ops.push(MicroOp::WriteWord),
            Size::Long => {
                self.micro_ops.push(MicroOp::WriteLongHi);
                self.micro_ops.push(MicroOp::WriteLongLo);
            }
        }
    }

    // === Tick engine ===

    /// Execute one clock cycle of CPU operation.
    ///
    /// Instant micro-ops (0 cycles) are processed in a tight loop.
    /// Timed micro-ops consume one cycle and return.
    #[allow(clippy::too_many_lines)]
    fn tick_internal<B: M68kBus>(&mut self, bus: &mut B) {
        loop {
            let Some(op) = self.micro_ops.current() else {
                self.queue_fetch();
                // Don't return — start the next opcode fetch in this same tick.
                // The 68000 never idles between instructions.
                continue;
            };

            match op {
                // --- Instant micro-ops (continue loop) ---
                MicroOp::CalcEA => {
                    // Placeholder — EA calc is done inside recipes.
                    self.micro_ops.advance();
                    continue;
                }
                MicroOp::Execute => {
                    self.decode_and_execute();
                    if self.pending_exception.is_none() {
                        self.micro_ops.advance();
                    }
                    continue;
                }
                MicroOp::ResetBus => {
                    bus.reset();
                    self.micro_ops.advance();
                    continue;
                }
                MicroOp::BeginException => {
                    self.begin_exception();
                    self.micro_ops.advance();
                    continue;
                }
                MicroOp::SetDataFromData2 => {
                    self.data = self.data2;
                    self.micro_ops.advance();
                    continue;
                }
                MicroOp::ApplyPostInc => {
                    self.apply_deferred_postinc();
                    self.micro_ops.advance();
                    continue;
                }
                MicroOp::RecipeStep => {
                    self.tick_recipe_step();
                    self.micro_ops.advance();
                    continue;
                }

                // --- Timed micro-ops (return after processing) ---
                MicroOp::FetchOpcode => self.tick_fetch_opcode(bus),
                MicroOp::FetchExtWord => self.tick_fetch_ext_word(bus),
                MicroOp::ReadByte => self.tick_read_byte(bus),
                MicroOp::ReadWord => self.tick_read_word(bus),
                MicroOp::ReadLongHi => self.tick_read_long_hi(bus),
                MicroOp::ReadLongLo => self.tick_read_long_lo(bus),
                MicroOp::WriteByte => self.tick_write_byte(bus),
                MicroOp::WriteWord => self.tick_write_word(bus),
                MicroOp::WriteLongHi => self.tick_write_long_hi(bus),
                MicroOp::WriteLongLo => self.tick_write_long_lo(bus),
                MicroOp::Internal => self.tick_internal_cycles(),
                MicroOp::PushWord => self.tick_push_word(bus),
                MicroOp::PushLongHi => self.tick_push_long_hi(bus),
                MicroOp::PushLongLo => self.tick_push_long_lo(bus),
                MicroOp::PopWord => self.tick_pop_word(bus),
                MicroOp::PopLongHi => self.tick_pop_long_hi(bus),
                MicroOp::PopLongLo => self.tick_pop_long_lo(bus),
                MicroOp::ReadVector => self.tick_read_vector(bus),
                MicroOp::MovemWrite => self.tick_movem_write(bus),
                MicroOp::MovemRead => self.tick_movem_read(bus),
                MicroOp::CmpmExecute => self.tick_cmpm_execute(bus),
                MicroOp::TasExecute => self.tick_tas_execute(bus),
                MicroOp::ShiftMemExecute => self.tick_shift_mem_execute(bus),
                MicroOp::AluMemRmw => self.tick_alu_mem_rmw(bus),
                MicroOp::AluMemSrc => self.tick_alu_mem_src(bus),
                MicroOp::BitMemOp => self.tick_bit_mem_op(bus),
                MicroOp::ExtendMemOp => self.tick_extend_mem_op(bus),
                MicroOp::PushGroup0IR => self.tick_push_group0_ir(bus),
                MicroOp::PushGroup0FaultAddr => self.tick_push_group0_fault_addr(bus),
                MicroOp::PushGroup0AccessInfo => self.tick_push_group0_access_info(bus),
            }
            return;
        }
    }

    // === Timed MicroOp handlers ===

    /// Tick for opcode fetch (4 cycles).
    fn tick_fetch_opcode<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.regs.pc & 1 != 0 {
                    self.address_error(self.regs.pc, true, true);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                self.opcode = self.read_word(bus, self.regs.pc);
                self.regs.pc = self.regs.pc.wrapping_add(2);
                self.instr_start_pc = self.regs.pc;
                self.cycle = 0;
                self.ext_count = 0;
                self.ext_idx = 0;
                self.micro_ops.advance();
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for extension word fetch (4 cycles).
    fn tick_fetch_ext_word<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.regs.pc & 1 != 0 {
                    self.address_error(self.regs.pc, true, true);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                let word = self.read_word(bus, self.regs.pc);
                self.regs.pc = self.regs.pc.wrapping_add(2);
                if (self.ext_count as usize) < self.ext_words.len() {
                    self.ext_words[self.ext_count as usize] = word;
                    self.ext_count += 1;
                }
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for byte read (4 cycles).
    fn tick_read_byte<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.apply_deferred_postinc();
            }
            1 | 2 => {}
            3 => {
                self.data = u32::from(self.read_byte(bus, self.addr));
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for word read (4 cycles).
    fn tick_read_word<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.apply_deferred_postinc();
                if self.addr & 1 != 0 {
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                self.data = u32::from(self.read_word(bus, self.addr));
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for long read high word (4 cycles).
    fn tick_read_long_hi<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.addr & 1 != 0 {
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                self.data = u32::from(self.read_word(bus, self.addr)) << 16;
                self.addr = self.addr.wrapping_add(2);
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for long read low word (4 cycles).
    fn tick_read_long_lo<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                self.data |= u32::from(self.read_word(bus, self.addr));
                self.apply_deferred_postinc();
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for byte write (4 cycles).
    fn tick_write_byte<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                self.write_byte(bus, self.addr, self.data as u8);
                self.apply_deferred_postinc();
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for word write (4 cycles).
    fn tick_write_word<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.addr & 1 != 0 {
                    self.address_error(self.addr, false, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                self.write_word(bus, self.addr, self.data as u16);
                self.apply_deferred_postinc();
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for long write high word (4 cycles).
    fn tick_write_long_hi<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.addr & 1 != 0 {
                    self.address_error(self.addr, false, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                self.write_word(bus, self.addr, (self.data >> 16) as u16);
                self.addr = self.addr.wrapping_add(2);
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for long write low word (4 cycles).
    fn tick_write_long_lo<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                self.write_word(bus, self.addr, self.data as u16);
                self.apply_deferred_postinc();
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for internal processing cycles.
    fn tick_internal_cycles(&mut self) {
        self.cycle += 1;
        if self.cycle >= self.internal_cycles {
            // BSR odd target flag — trigger address error after push.
            if self.data2 == 0x8000_0001 {
                self.data2 = 0;
                self.exception_pc_override = Some(self.addr2);
                self.fault_fc = if self.regs.sr & S != 0 { 6 } else { 2 };
                self.fault_addr = self.addr2;
                self.fault_read = true;
                self.fault_in_instruction = false;
                self.exception(3);
                self.cycle = 0;
                return;
            }
            self.cycle = 0;
            self.micro_ops.advance();
        }
    }

    /// Tick for push word (4 cycles).
    fn tick_push_word<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.push_word();
                if self.addr & 1 != 0 {
                    self.address_error(self.addr, false, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                self.write_word(bus, self.addr, self.data as u16);
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for push long high word (4 cycles).
    fn tick_push_long_hi<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.push_long();
                if self.addr & 1 != 0 {
                    self.address_error(self.addr, false, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                self.write_word(bus, self.addr, (self.data >> 16) as u16);
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for push long low word (4 cycles).
    fn tick_push_long_lo<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                self.write_word(bus, self.addr.wrapping_add(2), self.data as u16);
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for pop word (4 cycles).
    fn tick_pop_word<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.pop_word();
                if (self.addr.wrapping_sub(2)) & 1 != 0 {
                    self.address_error(self.addr.wrapping_sub(2), true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                self.data = u32::from(self.read_word(bus, self.addr.wrapping_sub(2)));
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for pop long high word (4 cycles).
    fn tick_pop_long_hi<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.pop_long();
                if (self.addr.wrapping_sub(4)) & 1 != 0 {
                    self.address_error(self.addr.wrapping_sub(4), true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                self.data = u32::from(self.read_word(bus, self.addr.wrapping_sub(4))) << 16;
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for pop long low word (4 cycles).
    fn tick_pop_long_lo<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                self.data |= u32::from(self.read_word(bus, self.addr.wrapping_sub(2)));
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for reading exception vector (two-phase long read).
    fn tick_read_vector<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                if self.movem_long_phase == 0 {
                    if let Some(vec) = self.current_exception {
                        let vector_addr = u32::from(vec) * 4;
                        self.data = u32::from(self.read_word(bus, vector_addr)) << 16;
                        self.addr = vector_addr;
                    }
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }
                // Phase 1: low word
                self.data |= u32::from(self.read_word(bus, self.addr.wrapping_add(2)));
                self.regs.pc = self.data;
                self.current_exception = None;
                self.movem_long_phase = 0;
                self.cycle = 0;
                self.micro_ops.advance();
                // Start fetching from new PC.
                self.micro_ops.clear();
                self.state = State::FetchOpcode;
                self.ext_count = 0;
                self.ext_idx = 0;
                self.src_mode = None;
                self.dst_mode = None;
                self.micro_ops.push(MicroOp::FetchOpcode);
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for MOVEM write (4 cycles per word).
    #[allow(clippy::too_many_lines)]
    fn tick_movem_write<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.movem_long_phase == 0 && self.addr & 1 != 0 {
                    let fault_addr = if self.movem_predec {
                        let ea_reg = (self.addr2 & 7) as usize;
                        self.regs.a(ea_reg).wrapping_sub(2)
                    } else {
                        self.addr
                    };
                    self.address_error(fault_addr, false, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                let mask = self.ext_words[0];
                let is_predec = self.movem_predec;
                let bit_idx = self.data2 as usize;

                let value = if is_predec {
                    if bit_idx < 8 {
                        self.regs.a(7 - bit_idx)
                    } else {
                        self.regs.d[15 - bit_idx]
                    }
                } else if bit_idx < 8 {
                    self.regs.d[bit_idx]
                } else {
                    self.regs.a(bit_idx - 8)
                };

                if self.size == Size::Long && self.movem_long_phase == 0 {
                    self.write_word(bus, self.addr, (value >> 16) as u16);
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }

                if self.size == Size::Long {
                    self.write_word(bus, self.addr, value as u16);
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 0;
                } else {
                    self.write_word(bus, self.addr, value as u16);
                    self.addr = self.addr.wrapping_add(2);
                }

                let next_bit = if is_predec {
                    self.find_next_movem_bit_down(mask, bit_idx)
                } else {
                    self.find_next_movem_bit_up(mask, bit_idx)
                };

                if let Some(next) = next_bit {
                    self.data2 = next as u32;
                    self.cycle = 0;
                    return;
                }

                if is_predec {
                    let ea_reg = (self.addr2 & 7) as usize;
                    let start_addr = self.regs.a(ea_reg);
                    let count = mask.count_ones();
                    let dec = if self.size == Size::Long { 4 } else { 2 };
                    self.regs
                        .set_a(ea_reg, start_addr.wrapping_sub(count * dec));
                }
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for MOVEM read (4 cycles per word).
    #[allow(clippy::too_many_lines)]
    fn tick_movem_read<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.movem_long_phase == 0 && self.addr & 1 != 0 {
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                let mask = self.ext_words[0];
                let bit_idx = self.data2 as usize;

                if self.size == Size::Long && self.movem_long_phase == 0 {
                    self.data = u32::from(self.read_word(bus, self.addr)) << 16;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }

                if self.size == Size::Long {
                    self.data |= u32::from(self.read_word(bus, self.addr));
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 0;
                } else {
                    let word = self.read_word(bus, self.addr);
                    self.data = word as i16 as i32 as u32;
                    self.addr = self.addr.wrapping_add(2);
                }

                if bit_idx < 8 {
                    self.regs.d[bit_idx] = self.data;
                } else {
                    self.regs.set_a(bit_idx - 8, self.data);
                }

                let next_bit = self.find_next_movem_bit_up(mask, bit_idx);

                if let Some(next) = next_bit {
                    self.data2 = next as u32;
                    self.cycle = 0;
                    return;
                }

                if self.movem_postinc {
                    let ea_reg = (self.addr2 & 7) as usize;
                    self.regs.set_a(ea_reg, self.addr);
                }
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    // === MOVEM bit-finding helpers ===

    pub(crate) fn find_next_movem_bit_up(&self, mask: u16, current: usize) -> Option<usize> {
        ((current + 1)..16).find(|&i| mask & (1 << i) != 0)
    }

    pub(crate) fn find_next_movem_bit_down(&self, mask: u16, current: usize) -> Option<usize> {
        (0..current).rev().find(|&i| mask & (1 << i) != 0)
    }

    pub(crate) fn find_first_movem_bit_up(&self, mask: u16) -> Option<usize> {
        (0..16).find(|&i| mask & (1 << i) != 0)
    }

    pub(crate) fn find_first_movem_bit_down(&self, mask: u16) -> Option<usize> {
        (0..16).rev().find(|&i| mask & (1 << i) != 0)
    }

    /// Tick for CMPM: Compare memory (Ay)+,(Ax)+.
    #[allow(clippy::too_many_lines)]
    fn tick_cmpm_execute<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.size != Size::Byte {
                    if self.movem_long_phase == 0 && self.addr & 1 != 0 {
                        let ay = self.data as usize;
                        self.regs.set_a(ay, self.addr.wrapping_add(2));
                        self.address_error(self.addr, true, false);
                        return;
                    } else if self.movem_long_phase == 1 && self.addr2 & 1 != 0 {
                        self.address_error(self.addr2, true, false);
                        return;
                    }
                }
            }
            1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        let src_val = match self.size {
                            Size::Byte => u32::from(self.read_byte(bus, self.addr)),
                            Size::Word => u32::from(self.read_word(bus, self.addr)),
                            Size::Long => self.read_long(bus, self.addr),
                        };
                        self.ext_words[0] = src_val as u16;
                        self.ext_words[1] = (src_val >> 16) as u16;

                        let ay = self.data as usize;
                        let ax = self.data2 as usize;
                        let inc = match self.size {
                            Size::Byte => if ay == 7 { 2 } else { 1 },
                            Size::Word => 2,
                            Size::Long => 4,
                        };
                        self.regs.set_a(ay, self.addr.wrapping_add(inc));

                        if ay == ax {
                            self.addr2 = self.regs.a(ax);
                        }

                        self.movem_long_phase = 1;
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        let dst_val = match self.size {
                            Size::Byte => u32::from(self.read_byte(bus, self.addr2)),
                            Size::Word => u32::from(self.read_word(bus, self.addr2)),
                            Size::Long => self.read_long(bus, self.addr2),
                        };

                        let ax = self.data2 as usize;
                        let inc = match self.size {
                            Size::Byte => if ax == 7 { 2 } else { 1 },
                            Size::Word => 2,
                            Size::Long => 4,
                        };
                        self.regs.set_a(ax, self.addr2.wrapping_add(inc));

                        let src_val =
                            u32::from(self.ext_words[0]) | (u32::from(self.ext_words[1]) << 16);
                        let result = dst_val.wrapping_sub(src_val);
                        self.set_flags_cmp(src_val, dst_val, result, self.size);

                        self.movem_long_phase = 0;
                        self.cycle = 0;
                        self.micro_ops.advance();
                        return;
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for TAS: Test And Set byte.
    fn tick_tas_execute<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        let value = self.read_byte(bus, self.addr);
                        self.data = u32::from(value);
                        self.set_flags_move(u32::from(value), Size::Byte);
                        self.movem_long_phase = 1;
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        let value = (self.data as u8) | 0x80;
                        self.write_byte(bus, self.addr, value);
                        self.apply_deferred_postinc();
                        self.movem_long_phase = 0;
                        self.cycle = 0;
                        self.micro_ops.advance();
                        return;
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for memory shift/rotate operations.
    fn tick_shift_mem_execute<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.movem_long_phase == 0 {
                    self.apply_deferred_postinc();
                }
                if self.movem_long_phase == 0 && self.addr & 1 != 0 {
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        let value = self.read_word(bus, self.addr);
                        self.ext_words[0] = value;
                        self.movem_long_phase = 1;
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        let value = u32::from(self.ext_words[0]);
                        let kind = self.data as u8;
                        let direction = self.data2 != 0;

                        let (result, carry) = self.shift_word_by_one(value, kind, direction);

                        self.write_word(bus, self.addr, result as u16);

                        self.set_flags_move(result, Size::Word);
                        self.regs.sr = Status::set_if(self.regs.sr, C, carry);
                        if kind != 3 {
                            self.regs.sr = Status::set_if(self.regs.sr, X, carry);
                        }
                        if kind == 0 && direction {
                            let v = (value ^ result) & 0x8000 != 0;
                            self.regs.sr = Status::set_if(self.regs.sr, V, v);
                        } else {
                            self.regs.sr &= !V;
                        }

                        self.movem_long_phase = 0;
                        self.cycle = 0;
                        self.micro_ops.advance();
                        return;
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Perform a word shift/rotate by 1 bit. Returns (result, carry_out).
    pub(crate) fn shift_word_by_one(&self, value: u32, kind: u8, left: bool) -> (u32, bool) {
        let mask = 0xFFFF_u32;
        let msb = 0x8000_u32;

        match (kind, left) {
            (0, true) => {
                let carry = (value & msb) != 0;
                ((value << 1) & mask, carry)
            }
            (0, false) => {
                let carry = (value & 1) != 0;
                let sign = value & msb;
                (((value >> 1) | sign) & mask, carry)
            }
            (1, true) => {
                let carry = (value & msb) != 0;
                ((value << 1) & mask, carry)
            }
            (1, false) => {
                let carry = (value & 1) != 0;
                ((value >> 1) & mask, carry)
            }
            (2, true) => {
                let x_in = u32::from(self.regs.sr & X != 0);
                let carry = (value & msb) != 0;
                (((value << 1) | x_in) & mask, carry)
            }
            (2, false) => {
                let x_in = if self.regs.sr & X != 0 { msb } else { 0 };
                let carry = (value & 1) != 0;
                (((value >> 1) | x_in) & mask, carry)
            }
            (3, true) => {
                let carry = (value & msb) != 0;
                (((value << 1) | (value >> 15)) & mask, carry)
            }
            (3, false) => {
                let carry = (value & 1) != 0;
                (((value >> 1) | (value << 15)) & mask, carry)
            }
            _ => (value, false),
        }
    }

    /// Tick for ALU memory read-modify-write operations.
    #[allow(clippy::too_many_lines)]
    fn tick_alu_mem_rmw<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.movem_long_phase == 0 && self.size != Size::Long {
                    self.apply_deferred_postinc();
                }
                if self.movem_long_phase == 0 && self.size != Size::Byte && self.addr & 1 != 0 {
                    if self.size == Size::Long && self.uses_predec_mode() {
                        self.exception_pc_override =
                            Some(self.regs.pc.wrapping_sub(2));
                    }
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        let val = match self.size {
                            Size::Byte => u32::from(self.read_byte(bus, self.addr)),
                            Size::Word => u32::from(self.read_word(bus, self.addr)),
                            Size::Long => u32::from(self.read_word(bus, self.addr)),
                        };
                        if self.size == Size::Long {
                            self.ext_words[1] = val as u16;
                            self.movem_long_phase = 1;
                        } else {
                            self.ext_words[0] = val as u16;
                            self.ext_words[1] = 0;
                            self.movem_long_phase = 2;
                        }
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        let lo = self.read_word(bus, self.addr.wrapping_add(2));
                        self.ext_words[0] = lo;
                        self.apply_deferred_postinc();
                        self.movem_long_phase = 2;
                        self.cycle = 0;
                        return;
                    }
                    2 => {
                        let mem_val =
                            u32::from(self.ext_words[0]) | (u32::from(self.ext_words[1]) << 16);
                        let src = self.data;
                        let op = self.data2;

                        let result = match op {
                            0 => {
                                let res = mem_val.wrapping_add(src);
                                self.set_flags_add(src, mem_val, res, self.size);
                                res
                            }
                            1 => {
                                let res = mem_val.wrapping_sub(src);
                                self.set_flags_sub(src, mem_val, res, self.size);
                                res
                            }
                            2 => {
                                let res = mem_val & src;
                                self.set_flags_move(res, self.size);
                                res
                            }
                            3 => {
                                let res = mem_val | src;
                                self.set_flags_move(res, self.size);
                                res
                            }
                            4 => {
                                let res = mem_val ^ src;
                                self.set_flags_move(res, self.size);
                                res
                            }
                            5 => {
                                let res = 0u32.wrapping_sub(mem_val);
                                self.set_flags_sub(mem_val, 0, res, self.size);
                                res
                            }
                            6 => {
                                let res = !mem_val;
                                self.set_flags_move(res, self.size);
                                res
                            }
                            7 => {
                                let x = u32::from(self.regs.sr & X != 0);
                                let res = 0u32.wrapping_sub(mem_val).wrapping_sub(x);
                                let (src_masked, result_masked, msb) = match self.size {
                                    Size::Byte => (mem_val & 0xFF, res & 0xFF, 0x80u32),
                                    Size::Word => (mem_val & 0xFFFF, res & 0xFFFF, 0x8000),
                                    Size::Long => (mem_val, res, 0x8000_0000),
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
                            8 => {
                                let src_byte = mem_val as u8;
                                let x = u8::from(self.regs.sr & X != 0);
                                let (result, borrow, overflow) = self.nbcd(src_byte, x);
                                let mut sr = self.regs.sr;
                                if result != 0 { sr &= !Z; }
                                sr = Status::set_if(sr, C, borrow);
                                sr = Status::set_if(sr, X, borrow);
                                sr = Status::set_if(sr, N, result & 0x80 != 0);
                                sr = Status::set_if(sr, V, overflow);
                                self.regs.sr = sr;
                                u32::from(result)
                            }
                            9 => {
                                // CLR
                                self.regs.sr = Status::clear_vc(self.regs.sr);
                                self.regs.sr = Status::update_nz_byte(self.regs.sr, 0);
                                0
                            }
                            10 => {
                                // MOVEfromSR
                                src
                            }
                            _ => mem_val,
                        };

                        match self.size {
                            Size::Byte => {
                                self.write_byte(bus, self.addr, result as u8);
                                self.apply_deferred_postinc();
                                self.movem_long_phase = 0;
                                self.cycle = 0;
                                self.micro_ops.advance();
                                return;
                            }
                            Size::Word => {
                                self.write_word(bus, self.addr, result as u16);
                                self.apply_deferred_postinc();
                                self.movem_long_phase = 0;
                                self.cycle = 0;
                                self.micro_ops.advance();
                                return;
                            }
                            Size::Long => {
                                self.write_word(bus, self.addr, (result >> 16) as u16);
                                self.ext_words[2] = result as u16;
                                self.movem_long_phase = 3;
                                self.cycle = 0;
                                return;
                            }
                        }
                    }
                    3 => {
                        self.write_word(bus, self.addr.wrapping_add(2), self.ext_words[2]);
                        self.apply_deferred_postinc();
                        self.movem_long_phase = 0;
                        self.cycle = 0;
                        self.micro_ops.advance();
                        return;
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for ALU memory source operations.
    #[allow(clippy::too_many_lines)]
    fn tick_alu_mem_src<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.size != Size::Long {
                    self.apply_deferred_postinc();
                }
                if self.size != Size::Byte && self.addr & 1 != 0 {
                    if self.size == Size::Long && self.uses_predec_mode() {
                        self.exception_pc_override =
                            Some(self.regs.pc.wrapping_sub(2));
                    }
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                if self.size == Size::Long && self.movem_long_phase == 0 {
                    let hi = self.read_word(bus, self.addr);
                    self.ext_words[2] = hi;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }

                let src = if self.size == Size::Long {
                    let lo = self.read_word(bus, self.addr);
                    self.movem_long_phase = 0;
                    self.apply_deferred_postinc();
                    (u32::from(self.ext_words[2]) << 16) | u32::from(lo)
                } else {
                    match self.size {
                        Size::Byte => u32::from(self.read_byte(bus, self.addr)),
                        Size::Word => u32::from(self.read_word(bus, self.addr)),
                        Size::Long => unreachable!(),
                    }
                };

                let reg = self.data as u8;
                let op = self.data2;

                match op {
                    0 => {
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst.wrapping_add(src);
                        self.write_data_reg(reg, result, self.size);
                        self.set_flags_add(src, dst, result, self.size);
                    }
                    1 => {
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst.wrapping_sub(src);
                        self.write_data_reg(reg, result, self.size);
                        self.set_flags_sub(src, dst, result, self.size);
                    }
                    2 => {
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst & src;
                        self.write_data_reg(reg, result, self.size);
                        self.set_flags_move(result, self.size);
                    }
                    3 => {
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst | src;
                        self.write_data_reg(reg, result, self.size);
                        self.set_flags_move(result, self.size);
                    }
                    4 => {
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst.wrapping_sub(src);
                        self.set_flags_cmp(src, dst, result, self.size);
                    }
                    5 => {
                        let src_extended = if self.size == Size::Word {
                            src as i16 as i32 as u32
                        } else {
                            src
                        };
                        let dst = self.regs.a(reg as usize);
                        self.regs.set_a(reg as usize, dst.wrapping_add(src_extended));
                    }
                    6 => {
                        let src_extended = if self.size == Size::Word {
                            src as i16 as i32 as u32
                        } else {
                            src
                        };
                        let dst = self.regs.a(reg as usize);
                        self.regs.set_a(reg as usize, dst.wrapping_sub(src_extended));
                    }
                    7 => {
                        let src_extended = if self.size == Size::Word {
                            src as i16 as i32 as u32
                        } else {
                            src
                        };
                        let dst = self.regs.a(reg as usize);
                        let result = dst.wrapping_sub(src_extended);
                        self.set_flags_cmp(src_extended, dst, result, Size::Long);
                    }
                    8 => {
                        self.set_flags_move(src, self.size);
                    }
                    9 => {
                        let dn = self.regs.d[reg as usize] as i16;
                        let upper_bound = src as i16;
                        if dn < 0 {
                            self.regs.sr &= !(N | Z | V | C);
                            self.regs.sr |= N;
                            self.exception(6);
                            return;
                        } else if dn > upper_bound {
                            self.regs.sr &= !(N | Z | V | C);
                            self.exception(6);
                            return;
                        }
                        self.regs.sr &= !(N | Z | V | C);
                    }
                    10 => {
                        let src_word = src & 0xFFFF;
                        let dst_word = self.regs.d[reg as usize] & 0xFFFF;
                        let result = src_word * dst_word;
                        self.regs.d[reg as usize] = result;
                        self.regs.sr = Status::clear_vc(self.regs.sr);
                        self.regs.sr = Status::update_nz_long(self.regs.sr, result);
                        let ones = (src_word as u16).count_ones() as u8;
                        let timing = (38 + 2 * ones).saturating_sub(4);
                        self.queue_internal(timing);
                    }
                    11 => {
                        let src_signed = (src as i16) as i32;
                        let dst_signed = (self.regs.d[reg as usize] as i16) as i32;
                        let result = (src_signed * dst_signed) as u32;
                        self.regs.d[reg as usize] = result;
                        self.regs.sr = Status::clear_vc(self.regs.sr);
                        self.regs.sr = Status::update_nz_long(self.regs.sr, result);
                        let src16 = src as u16;
                        let pattern = src16 ^ (src16 << 1);
                        let ones = pattern.count_ones() as u8;
                        let timing = (38 + 2 * ones).saturating_sub(4);
                        self.queue_internal(timing);
                    }
                    12 => {
                        let divisor = src & 0xFFFF;
                        let dividend = self.regs.d[reg as usize];
                        if divisor == 0 {
                            self.exception(5);
                            return;
                        }
                        let quotient = dividend / divisor;
                        let remainder = dividend % divisor;
                        let timing = Self::divu_cycles(dividend, divisor as u16)
                            .saturating_sub(4);
                        if quotient > 0xFFFF {
                            self.regs.sr |= V;
                            self.regs.sr &= !C;
                            self.regs.sr |= N;
                            self.regs.sr &= !Z;
                        } else {
                            self.regs.d[reg as usize] = (remainder << 16) | quotient;
                            self.regs.sr &= !(V | C);
                            self.regs.sr = Status::set_if(self.regs.sr, Z, quotient == 0);
                            self.regs.sr =
                                Status::set_if(self.regs.sr, N, quotient & 0x8000 != 0);
                        }
                        self.queue_internal(timing);
                    }
                    13 => {
                        let divisor = (src as i16) as i32;
                        let dividend = self.regs.d[reg as usize] as i32;
                        if divisor == 0 {
                            self.exception(5);
                            return;
                        }
                        let quotient = dividend / divisor;
                        let remainder = dividend % divisor;
                        let timing = Self::divs_cycles(dividend, divisor as i16)
                            .saturating_sub(4);
                        if (-32768..=32767).contains(&quotient) {
                            let q = quotient as i16 as u16 as u32;
                            let r = remainder as i16 as u16 as u32;
                            self.regs.d[reg as usize] = (r << 16) | q;
                            self.regs.sr &= !(V | C);
                            self.regs.sr = Status::set_if(self.regs.sr, Z, quotient == 0);
                            self.regs.sr = Status::set_if(self.regs.sr, N, quotient < 0);
                        } else {
                            self.regs.sr |= V;
                            self.regs.sr &= !C;
                            self.regs.sr |= N;
                            self.regs.sr &= !Z;
                        }
                        self.queue_internal(timing);
                    }
                    14 => {
                        let dst = src;
                        let imm = self.data;
                        let result = dst.wrapping_sub(imm);
                        self.set_flags_cmp(imm, dst, result, self.size);
                    }
                    _ => {}
                }

                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for bit operations on memory byte.
    fn tick_bit_mem_op<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        let value = self.read_byte(bus, self.addr);
                        let bit = (self.data & 7) as u8;
                        let mask = 1u8 << bit;
                        let was_zero = (value & mask) == 0;
                        self.regs.sr = Status::set_if(self.regs.sr, Z, was_zero);

                        let op = self.data2;
                        if op == 0 {
                            self.apply_deferred_postinc();
                            self.cycle = 0;
                            self.micro_ops.advance();
                            return;
                        }

                        let new_value = match op {
                            1 => value ^ mask,
                            2 => value & !mask,
                            3 => value | mask,
                            _ => value,
                        };

                        self.ext_words[0] = u16::from(new_value);
                        self.movem_long_phase = 1;
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        let value = self.ext_words[0] as u8;
                        self.write_byte(bus, self.addr, value);
                        self.apply_deferred_postinc();
                        self.movem_long_phase = 0;
                        self.cycle = 0;
                        self.micro_ops.advance();
                        return;
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for multi-precision/BCD memory-to-memory: -(Ax),-(Ay).
    #[allow(clippy::too_many_lines)]
    fn tick_extend_mem_op<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Phase 0: Pre-decrement source register.
                if !self.extend_predec_done && self.movem_long_phase == 0 {
                    let rx = (self.data & 0xFF) as usize;
                    let ry = ((self.data >> 8) & 0xFF) as usize;
                    let an = self.regs.a(rx);

                    if self.size == Size::Long {
                        let first_word_addr = an.wrapping_sub(2);
                        if first_word_addr & 1 != 0 {
                            self.address_error(first_word_addr, true, false);
                            return;
                        }
                    }

                    let decr = match self.size {
                        Size::Byte => if rx == 7 { 2 } else { 1 },
                        Size::Word => 2,
                        Size::Long => 4,
                    };

                    let src_addr = an.wrapping_sub(decr);
                    self.regs.set_a(rx, src_addr);
                    self.addr = src_addr;

                    if self.size == Size::Word && src_addr & 1 != 0 {
                        self.address_error(src_addr, true, false);
                        return;
                    }

                    self.data2 = (self.data2 & 0xFF) | ((ry as u32) << 8);
                    self.data2 = (self.data2 & 0xFFFF) | ((rx as u32) << 16);
                    self.extend_predec_done = true;
                }

                // Destination pre-decrement.
                let dest_predec_phase = match self.size {
                    Size::Byte | Size::Word => 1,
                    Size::Long => 2,
                };
                let dest_decremented = self.data2 & 0x0100_0000 != 0;
                if !dest_decremented && self.movem_long_phase == dest_predec_phase {
                    let ry = ((self.data2 >> 8) & 0xFF) as usize;
                    let an = self.regs.a(ry);

                    if self.size == Size::Long {
                        let first_word_addr = an.wrapping_sub(2);
                        if first_word_addr & 1 != 0 {
                            self.address_error(first_word_addr, true, false);
                            return;
                        }
                    }

                    let decr = match self.size {
                        Size::Byte => if ry == 7 { 2 } else { 1 },
                        Size::Word => 2,
                        Size::Long => 4,
                    };

                    let dst_addr = an.wrapping_sub(decr);
                    self.regs.set_a(ry, dst_addr);
                    self.addr2 = dst_addr;

                    if self.size == Size::Word && dst_addr & 1 != 0 {
                        self.address_error(dst_addr, true, false);
                        return;
                    }

                    self.data2 |= 0x0100_0000;
                }
            }
            1 | 2 => {}
            3 => {
                match self.size {
                    Size::Byte => {
                        match self.movem_long_phase {
                            0 => {
                                self.data = u32::from(self.read_byte(bus, self.addr));
                                self.movem_long_phase = 1;
                                self.cycle = 0;
                                return;
                            }
                            1 => {
                                let src = self.data as u8;
                                let dst = self.read_byte(bus, self.addr2);
                                let x = u8::from(self.regs.sr & X != 0);
                                let (result, carry, bcd_overflow) = match self.data2 & 0xFF {
                                    0 => self.bcd_add(src, dst, x),
                                    1 => self.bcd_sub(dst, src, x),
                                    2 => {
                                        let r = u16::from(dst) + u16::from(src) + u16::from(x);
                                        (r as u8, r > 0xFF, false)
                                    }
                                    3 => {
                                        let r = u16::from(dst).wrapping_sub(u16::from(src)).wrapping_sub(u16::from(x));
                                        (r as u8, u16::from(dst) < u16::from(src) + u16::from(x), false)
                                    }
                                    _ => unreachable!(),
                                };
                                self.data = u32::from(result);
                                self.set_extend_flags(u32::from(src), u32::from(dst), u32::from(result), carry, bcd_overflow, Size::Byte);
                                self.movem_long_phase = 2;
                                self.cycle = 0;
                                return;
                            }
                            2 => {
                                self.write_byte(bus, self.addr2, self.data as u8);
                                self.cycle = 0;
                                self.micro_ops.advance();
                                return;
                            }
                            _ => unreachable!(),
                        }
                    }
                    Size::Word => {
                        match self.movem_long_phase {
                            0 => {
                                self.data = u32::from(self.read_word(bus, self.addr));
                                self.movem_long_phase = 1;
                                self.cycle = 0;
                                return;
                            }
                            1 => {
                                let src = self.data as u16;
                                let dst = self.read_word(bus, self.addr2);
                                let x = u32::from(self.regs.sr & X != 0);
                                let (result, carry) = if (self.data2 & 0xFF) == 2 {
                                    let r = u32::from(dst) + u32::from(src) + x;
                                    (r as u16, r > 0xFFFF)
                                } else {
                                    let r = u32::from(dst).wrapping_sub(u32::from(src)).wrapping_sub(x);
                                    (r as u16, u32::from(dst) < u32::from(src) + x)
                                };
                                self.data = u32::from(result);
                                self.set_extend_flags(src.into(), dst.into(), result.into(), carry, false, Size::Word);
                                self.movem_long_phase = 2;
                                self.cycle = 0;
                                return;
                            }
                            2 => {
                                self.write_word(bus, self.addr2, self.data as u16);
                                self.cycle = 0;
                                self.micro_ops.advance();
                                return;
                            }
                            _ => unreachable!(),
                        }
                    }
                    Size::Long => {
                        match self.movem_long_phase {
                            0 => {
                                self.ext_words[3] = self.read_word(bus, self.addr);
                                self.movem_long_phase = 1;
                                self.cycle = 0;
                                return;
                            }
                            1 => {
                                self.data = (u32::from(self.ext_words[3]) << 16) | u32::from(self.read_word(bus, self.addr.wrapping_add(2)));
                                self.movem_long_phase = 2;
                                self.cycle = 0;
                                return;
                            }
                            2 => {
                                self.ext_words[3] = self.read_word(bus, self.addr2);
                                self.movem_long_phase = 3;
                                self.cycle = 0;
                                return;
                            }
                            3 => {
                                let src = self.data;
                                let dst = (u32::from(self.ext_words[3]) << 16) | u32::from(self.read_word(bus, self.addr2.wrapping_add(2)));
                                let x = u32::from(self.regs.sr & X != 0);
                                let (result, carry) = if (self.data2 & 0xFF) == 2 {
                                    let r = (dst as u64) + (src as u64) + (x as u64);
                                    (r as u32, r > 0xFFFF_FFFF)
                                } else {
                                    let r = (dst as u64).wrapping_sub(src as u64).wrapping_sub(x as u64);
                                    (r as u32, (dst as u64) < (src as u64) + (x as u64))
                                };
                                self.data = result;
                                self.set_extend_flags(src, dst, result, carry, false, Size::Long);
                                self.movem_long_phase = 4;
                                self.cycle = 0;
                                return;
                            }
                            4 => {
                                self.write_word(bus, self.addr2, (self.data >> 16) as u16);
                                self.movem_long_phase = 5;
                                self.cycle = 0;
                                return;
                            }
                            5 => {
                                self.write_word(bus, self.addr2.wrapping_add(2), self.data as u16);
                                self.cycle = 0;
                                self.micro_ops.advance();
                                return;
                            }
                            _ => unreachable!(),
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Set flags for ADDX/SUBX/ABCD/SBCD operations.
    pub(crate) fn set_extend_flags(
        &mut self,
        src: u32,
        dst: u32,
        result: u32,
        carry: bool,
        bcd_overflow: bool,
        size: Size,
    ) {
        let (result_masked, msb) = match size {
            Size::Byte => (result & 0xFF, 0x80u32),
            Size::Word => (result & 0xFFFF, 0x8000),
            Size::Long => (result, 0x8000_0000),
        };

        if result_masked != 0 {
            self.regs.sr &= !Z;
        }
        self.regs.sr = Status::set_if(self.regs.sr, C, carry);
        self.regs.sr = Status::set_if(self.regs.sr, X, carry);

        let op_type = self.data2 & 0xFF;
        if op_type >= 2 {
            self.regs.sr = Status::set_if(self.regs.sr, N, result_masked & msb != 0);
            let src_masked = match size {
                Size::Byte => src & 0xFF,
                Size::Word => src & 0xFFFF,
                Size::Long => src,
            };
            let dst_masked = match size {
                Size::Byte => dst & 0xFF,
                Size::Word => dst & 0xFFFF,
                Size::Long => dst,
            };
            let overflow = if op_type == 2 {
                (!(src_masked ^ dst_masked) & (src_masked ^ result_masked) & msb) != 0
            } else {
                ((dst_masked ^ src_masked) & (dst_masked ^ result_masked) & msb) != 0
            };
            self.regs.sr = Status::set_if(self.regs.sr, V, overflow);
        } else {
            self.regs.sr = Status::set_if(self.regs.sr, N, result_masked & msb != 0);
            self.regs.sr = Status::set_if(self.regs.sr, V, bcd_overflow);
        }
    }

    /// Tick for pushing IR during group 0 exception (4 cycles).
    fn tick_push_group0_ir<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.push_word();
            }
            1 | 2 => {}
            3 => {
                self.write_word(bus, self.addr, self.addr2 as u16);
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for pushing fault address during group 0 exception (8 cycles).
    fn tick_push_group0_fault_addr<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                if self.movem_long_phase == 0 {
                    self.addr = self.regs.push_long();
                }
            }
            1 | 2 => {}
            3 => {
                if self.movem_long_phase == 0 {
                    self.write_word(bus, self.addr, (self.fault_addr >> 16) as u16);
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }
                self.write_word(bus, self.addr.wrapping_add(2), self.fault_addr as u16);
                self.movem_long_phase = 0;
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for pushing access info word during group 0 exception (4 cycles).
    fn tick_push_group0_access_info<B: M68kBus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.push_word();
            }
            1 | 2 => {}
            3 => {
                self.write_word(bus, self.addr, self.group0_access_info);
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    // === Flag helpers ===

    /// Set flags for MOVE-style operations (clears V and C, sets N and Z).
    pub(crate) fn set_flags_move(&mut self, value: u32, size: Size) {
        self.regs.sr = Status::clear_vc(self.regs.sr);
        self.regs.sr = match size {
            Size::Byte => Status::update_nz_byte(self.regs.sr, value as u8),
            Size::Word => Status::update_nz_word(self.regs.sr, value as u16),
            Size::Long => Status::update_nz_long(self.regs.sr, value),
        };
    }

    /// Set MOVE flags with only N and Z updated (V and C preserved).
    pub(crate) fn set_flags_move_nz_only(&mut self, value: u32, size: Size) {
        self.regs.sr = match size {
            Size::Byte => Status::update_nz_byte(self.regs.sr, value as u8),
            Size::Word => Status::update_nz_word(self.regs.sr, value as u16),
            Size::Long => Status::update_nz_long(self.regs.sr, value),
        };
    }

    /// Set flags for ADD operation.
    pub(crate) fn set_flags_add(&mut self, src: u32, dst: u32, result: u32, size: Size) {
        let (src, dst, result, msb) = match size {
            Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80),
            Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
            Size::Long => (src, dst, result, 0x8000_0000),
        };
        let mut sr = self.regs.sr;
        sr = Status::set_if(sr, Z, result == 0);
        sr = Status::set_if(sr, N, result & msb != 0);
        let carry = match size {
            Size::Byte => (u16::from(src as u8) + u16::from(dst as u8)) > 0xFF,
            Size::Word => (u32::from(src as u16) + u32::from(dst as u16)) > 0xFFFF,
            Size::Long => src.checked_add(dst).is_none(),
        };
        sr = Status::set_if(sr, C, carry);
        sr = Status::set_if(sr, X, carry);
        let overflow = (!(src ^ dst) & (src ^ result) & msb) != 0;
        sr = Status::set_if(sr, V, overflow);
        self.regs.sr = sr;
    }

    /// Set flags for SUB operation.
    pub(crate) fn set_flags_sub(&mut self, src: u32, dst: u32, result: u32, size: Size) {
        let (src, dst, result, msb) = match size {
            Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80),
            Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
            Size::Long => (src, dst, result, 0x8000_0000),
        };
        let mut sr = self.regs.sr;
        sr = Status::set_if(sr, Z, result == 0);
        sr = Status::set_if(sr, N, result & msb != 0);
        let carry = src > dst;
        sr = Status::set_if(sr, C, carry);
        sr = Status::set_if(sr, X, carry);
        let overflow = ((src ^ dst) & (dst ^ result) & msb) != 0;
        sr = Status::set_if(sr, V, overflow);
        self.regs.sr = sr;
    }

    /// Set flags for CMP operation (like SUB but doesn't set X).
    pub(crate) fn set_flags_cmp(&mut self, src: u32, dst: u32, result: u32, size: Size) {
        let (src, dst, result, msb) = match size {
            Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80),
            Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
            Size::Long => (src, dst, result, 0x8000_0000),
        };
        let mut sr = self.regs.sr;
        sr = Status::set_if(sr, Z, result == 0);
        sr = Status::set_if(sr, N, result & msb != 0);
        sr = Status::set_if(sr, C, src > dst);
        let overflow = ((src ^ dst) & (dst ^ result) & msb) != 0;
        sr = Status::set_if(sr, V, overflow);
        self.regs.sr = sr;
    }

    /// EA calculation internal cycles by mode.
    pub(crate) fn ea_calc_cycles(&self, mode: AddrMode) -> u8 {
        match mode {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) => 0,
            AddrMode::AddrInd(_) => 0,
            AddrMode::AddrIndPostInc(_) | AddrMode::AddrIndPreDec(_) => 2,
            AddrMode::AddrIndDisp(_) | AddrMode::PcDisp => 0,
            AddrMode::AddrIndIndex(_) | AddrMode::PcIndex => 2,
            AddrMode::AbsShort | AddrMode::AbsLong => 0,
            AddrMode::Immediate => 0,
        }
    }
}

impl Default for Cpu68000 {
    fn default() -> Self {
        Self::new()
    }
}

// === Public API ===

impl Cpu68000 {
    /// Advance the CPU by one clock cycle.
    pub fn tick<B: M68kBus>(&mut self, bus: &mut B) {
        self.total_cycles += Ticks::new(1);

        // Burn wait cycles from bus contention before processing micro-ops.
        if self.wait_cycles > 0 {
            self.wait_cycles -= 1;
            return;
        }

        match self.state {
            State::Halted => return,
            State::Stopped => {
                if self.int_pending > self.regs.interrupt_mask() {
                    self.state = State::Execute;
                    let level = self.int_pending;
                    self.int_pending = 0;
                    self.regs.set_interrupt_mask(level);
                    self.exception(24 + level);
                } else {
                    return;
                }
            }
            _ => {}
        }

        self.tick_internal(bus);

        if self.micro_ops.is_empty() {
            if self.int_pending > self.regs.interrupt_mask() {
                let level = self.int_pending;
                self.int_pending = 0;
                self.regs.set_interrupt_mask(level);
                self.exception(24 + level);
            } else if self.pending_exception.is_some() {
                self.micro_ops.push(MicroOp::BeginException);
            } else {
                self.queue_fetch();
            }
        }
    }

    /// Get the program counter.
    #[must_use]
    pub fn pc(&self) -> u32 {
        self.regs.pc
    }

    /// Get a snapshot of registers.
    #[must_use]
    pub fn registers(&self) -> &Registers {
        &self.regs
    }

    /// Get a mutable reference to registers.
    pub fn registers_mut(&mut self) -> &mut Registers {
        &mut self.regs
    }

    /// Check if the CPU is halted or stopped.
    #[must_use]
    pub fn is_halted(&self) -> bool {
        matches!(self.state, State::Halted | State::Stopped)
    }

    /// Check if the CPU is in STOP state (waiting for interrupt).
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        matches!(self.state, State::Stopped)
    }

    /// Reset the CPU.
    pub fn reset_cpu(&mut self) {
        self.regs = Registers::new();
        self.state = State::FetchOpcode;
        self.micro_ops.clear();
        self.cycle = 0;
        self.internal_cycles = 0;
        self.opcode = 0;
        self.ext_words = [0; 4];
        self.ext_count = 0;
        self.ext_idx = 0;
        self.src_mode = None;
        self.dst_mode = None;
        self.addr = 0;
        self.data = 0;
        self.size = Size::Word;
        self.addr2 = 0;
        self.data2 = 0;
        self.pending_exception = None;
        self.current_exception = None;
        self.fault_addr = 0;
        self.fault_read = true;
        self.fault_in_instruction = false;
        self.fault_fc = 0;
        self.group0_access_info = 0;
        self.extend_predec_done = false;
        self.int_pending = 0;
        self.micro_ops.push(MicroOp::FetchOpcode);
    }
}

// === Test utilities ===

#[cfg(feature = "test-utils")]
impl Cpu68000 {
    /// Execute one complete instruction. Returns cycles consumed.
    pub fn step<B: M68kBus>(&mut self, bus: &mut B) -> u32 {
        let mut cycles = 0u32;
        let max_cycles = 200;

        self.tick(bus);
        cycles += 1;

        while !(self.micro_ops.is_empty()
            || (self.cycle == 0 && matches!(self.micro_ops.current(), Some(MicroOp::FetchOpcode))))
        {
            self.tick(bus);
            cycles += 1;
            if cycles >= max_cycles {
                break;
            }
        }

        cycles
    }

    /// Set the program counter.
    pub fn set_pc(&mut self, value: u32) {
        self.regs.pc = value & 0x00FF_FFFF;
    }

    /// Set the stack pointer.
    pub fn set_sp(&mut self, value: u32) {
        self.regs.set_active_sp(value);
    }

    /// Get the full 32-bit program counter.
    pub fn pc32(&self) -> u32 {
        self.regs.pc
    }
}
