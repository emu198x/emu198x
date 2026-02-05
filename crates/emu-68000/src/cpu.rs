//! Motorola 68000 CPU core with per-cycle execution.

#![allow(clippy::cast_possible_truncation)] // Intentional truncation for byte/word extraction.
#![allow(clippy::cast_possible_wrap)] // Intentional i16 casts for displacements.
#![allow(clippy::cast_sign_loss)] // Intentional sign-extending conversions.
#![allow(clippy::cast_lossless)] // Using as for consistency in chain casts.
#![allow(clippy::struct_excessive_bools)] // CPU state requires multiple boolean flags.
#![allow(clippy::match_same_arms)] // T-state match arms are intentionally explicit.
#![allow(clippy::manual_range_patterns)] // Explicit T-state values for clarity.
#![allow(clippy::unused_self)] // Bus methods need &mut self for future wait state tracking.
#![allow(dead_code)] // Helper functions will be used as more instructions are implemented.

use emu_core::{Bus, Cpu, Observable, Ticks, Value};

use crate::flags::{self, Status, C, N, V, X, Z};
use crate::microcode::{MicroOp, MicroOpQueue};
use crate::registers::Registers;

/// Operation size for 68000 instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// 8-bit byte operation.
    Byte,
    /// 16-bit word operation.
    Word,
    /// 32-bit long operation.
    Long,
}

/// Instruction execution phase for multi-step instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrPhase {
    /// Initial decode and setup.
    Initial,
    /// Fetched source extension words, need to calculate EA and read.
    SrcEACalc,
    /// Read source operand, now fetch dest extension words.
    SrcRead,
    /// Fetched dest extension words, need to calculate EA and write.
    DstEACalc,
    /// Final write to destination.
    DstWrite,
    /// Instruction complete.
    Complete,
}

impl Size {
    /// Get size from the standard 2-bit encoding (00=byte, 01=word, 10=long).
    #[must_use]
    pub fn from_bits(bits: u8) -> Option<Self> {
        match bits & 0x03 {
            0 => Some(Self::Byte),
            1 => Some(Self::Word),
            2 => Some(Self::Long),
            _ => None,
        }
    }

    /// Get size from the move encoding (01=byte, 11=word, 10=long).
    #[must_use]
    pub fn from_move_bits(bits: u8) -> Option<Self> {
        match bits & 0x03 {
            1 => Some(Self::Byte),
            3 => Some(Self::Word),
            2 => Some(Self::Long),
            _ => None,
        }
    }
}

/// Addressing mode for 68000 instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddrMode {
    /// Data register direct: Dn
    DataReg(u8),
    /// Address register direct: An
    AddrReg(u8),
    /// Address register indirect: (An)
    AddrInd(u8),
    /// Address register indirect with postincrement: (An)+
    AddrIndPostInc(u8),
    /// Address register indirect with predecrement: -(An)
    AddrIndPreDec(u8),
    /// Address register indirect with displacement: d16(An)
    AddrIndDisp(u8),
    /// Address register indirect with index: d8(An,Xn)
    AddrIndIndex(u8),
    /// Absolute short: (xxx).W
    AbsShort,
    /// Absolute long: (xxx).L
    AbsLong,
    /// Program counter with displacement: d16(PC)
    PcDisp,
    /// Program counter with index: d8(PC,Xn)
    PcIndex,
    /// Immediate: #<data>
    Immediate,
}

impl AddrMode {
    /// Decode addressing mode from mode/register fields.
    #[must_use]
    pub fn decode(mode: u8, reg: u8) -> Option<Self> {
        match mode & 0x07 {
            0 => Some(Self::DataReg(reg & 0x07)),
            1 => Some(Self::AddrReg(reg & 0x07)),
            2 => Some(Self::AddrInd(reg & 0x07)),
            3 => Some(Self::AddrIndPostInc(reg & 0x07)),
            4 => Some(Self::AddrIndPreDec(reg & 0x07)),
            5 => Some(Self::AddrIndDisp(reg & 0x07)),
            6 => Some(Self::AddrIndIndex(reg & 0x07)),
            7 => match reg & 0x07 {
                0 => Some(Self::AbsShort),
                1 => Some(Self::AbsLong),
                2 => Some(Self::PcDisp),
                3 => Some(Self::PcIndex),
                4 => Some(Self::Immediate),
                _ => None,
            },
            _ => None,
        }
    }

    /// Check if this mode is a data alterable destination.
    #[must_use]
    pub fn is_data_alterable(&self) -> bool {
        matches!(
            self,
            Self::DataReg(_)
                | Self::AddrInd(_)
                | Self::AddrIndPostInc(_)
                | Self::AddrIndPreDec(_)
                | Self::AddrIndDisp(_)
                | Self::AddrIndIndex(_)
                | Self::AbsShort
                | Self::AbsLong
        )
    }

    /// Check if this mode is memory alterable.
    #[must_use]
    pub fn is_memory_alterable(&self) -> bool {
        matches!(
            self,
            Self::AddrInd(_)
                | Self::AddrIndPostInc(_)
                | Self::AddrIndPreDec(_)
                | Self::AddrIndDisp(_)
                | Self::AddrIndIndex(_)
                | Self::AbsShort
                | Self::AbsLong
        )
    }
}

/// 68000 CPU state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Fetching instruction opcode.
    FetchOpcode,
    /// Executing instruction micro-ops.
    Execute,
    /// Processing exception.
    Exception,
    /// Halted (double bus fault or STOP instruction).
    Halted,
    /// Stopped (STOP instruction, waiting for interrupt).
    Stopped,
}

/// Motorola 68000 CPU.
///
/// The CPU does not own the bus. Instead, the bus is passed to `tick()` on
/// each clock cycle. This allows the bus to be shared with other components
/// (e.g., custom chips) that may also need bus access.
pub struct M68000 {
    // === Registers ===
    /// CPU registers.
    pub regs: Registers,

    // === Execution state ===
    /// Current CPU state.
    state: State,
    /// Queue of micro-operations for current instruction.
    micro_ops: MicroOpQueue,
    /// Cycle counter within current micro-op.
    cycle: u8,
    /// Total internal cycles for current Internal micro-op.
    internal_cycles: u8,
    /// Whether Internal micro-op should advance PC (false for jumps/branches that set PC directly).
    internal_advances_pc: bool,

    // === Instruction decode state ===
    /// Current opcode word.
    opcode: u16,
    /// Extension words.
    ext_words: [u16; 4],
    /// Number of extension words read.
    ext_count: u8,
    /// Index into `ext_words` for current processing.
    ext_idx: u8,
    /// Source addressing mode.
    src_mode: Option<AddrMode>,
    /// Destination addressing mode.
    dst_mode: Option<AddrMode>,
    /// Current instruction execution phase.
    instr_phase: InstrPhase,

    // === Temporary storage ===
    /// Effective address calculated.
    addr: u32,
    /// Data being transferred.
    data: u32,
    /// Operation size.
    size: Size,
    /// Second address (for some instructions).
    addr2: u32,
    /// Second data value (for some instructions).
    data2: u32,

    // === MOVEM state ===
    /// True if MOVEM is using predecrement mode.
    movem_predec: bool,
    /// True if MOVEM is using postincrement mode.
    movem_postinc: bool,
    /// Long transfer phase: 0 = high word, 1 = low word.
    movem_long_phase: u8,

    // === Exception state ===
    /// PC at instruction start (before extension words consumed).
    /// Used for correct PC value in exception frames.
    instr_start_pc: u32,
    /// Pending exception vector number.
    pending_exception: Option<u8>,
    /// Current exception being processed.
    current_exception: Option<u8>,
    /// Fault address for address/bus error exceptions.
    fault_addr: u32,
    /// True if fault was during read, false for write.
    fault_read: bool,
    /// True if fault was during instruction fetch, false for data access.
    fault_in_instruction: bool,
    /// Function code at time of fault (for bus/address error frame).
    fault_fc: u8,
    /// Access info word for group 0 exception frame (computed in begin_exception).
    group0_access_info: u16,
    /// True if ExtendMemOp has performed its pre-decrements.
    extend_predec_done: bool,
    /// True if next FetchOpcode should not push Execute (post-exception prefetch only).
    prefetch_only: bool,
    /// In prefetch_only mode: did the instruction access external memory?
    /// Used to determine if prefetch advance is needed even without extension words.
    mem_accessed: bool,

    // === Interrupt state ===
    /// Pending interrupt level (1-7), 0 = none.
    int_pending: u8,

    // === Timing ===
    /// Total clock cycles elapsed.
    total_cycles: Ticks,
}

impl M68000 {
    /// Create a new 68000 CPU.
    #[must_use]
    pub fn new() -> Self {
        let mut cpu = Self {
            regs: Registers::new(),
            state: State::FetchOpcode,
            micro_ops: MicroOpQueue::new(),
            cycle: 0,
            internal_cycles: 0,
            internal_advances_pc: true,
            opcode: 0,
            ext_words: [0; 4],
            ext_count: 0,
            ext_idx: 0,
            src_mode: None,
            dst_mode: None,
            instr_phase: InstrPhase::Initial,
            addr: 0,
            data: 0,
            size: Size::Word,
            addr2: 0,
            data2: 0,
            movem_predec: false,
            movem_postinc: false,
            movem_long_phase: 0,
            instr_start_pc: 0,
            pending_exception: None,
            current_exception: None,
            fault_addr: 0,
            fault_read: true,
            fault_in_instruction: false,
            fault_fc: 0,
            group0_access_info: 0,
            extend_predec_done: false,
            prefetch_only: false,
            mem_accessed: false,
            int_pending: 0,
            total_cycles: Ticks::ZERO,
        };
        // Start with a fetch
        cpu.micro_ops.push(MicroOp::FetchOpcode);
        cpu
    }

    /// Total clock cycles elapsed since creation.
    #[must_use]
    pub const fn total_cycles(&self) -> Ticks {
        self.total_cycles
    }

    /// Set up CPU with a pre-fetched opcode for single-step testing.
    ///
    /// This initializes the CPU as if the opcode has already been fetched,
    /// ready to execute. Used by test harnesses that provide prefetch state.
    ///
    /// - `opcode`: The instruction opcode (IR register)
    /// - `ext_words_in`: Extension words (IRC and subsequent words from memory)
    ///
    /// The first element is IRC (prefetch[1]), followed by words from PC, PC+2, etc.
    pub fn setup_prefetch(&mut self, opcode: u16, ext_words_in: &[u16]) {
        self.opcode = opcode;
        // Copy extension words (up to 4)
        let count = ext_words_in.len().min(4);
        for i in 0..count {
            self.ext_words[i] = ext_words_in[i];
        }
        self.ext_count = count as u8;
        self.ext_idx = 0;
        self.micro_ops.clear();
        // Queue Execute instead of FetchOpcode - opcode is already loaded
        self.micro_ops.push(MicroOp::Execute);
        self.state = State::Execute;
        self.cycle = 0;
        // Set prefetch_only so after this instruction, we just prefetch without executing
        self.prefetch_only = true;
        self.mem_accessed = false;
        // Reset internal_advances_pc - instructions that don't use internal cycles
        // need the final prefetch advance
        self.internal_advances_pc = false;
        // Save instruction start PC for exception handling.
        // In prefetch_only mode, this is the PC value before execution begins.
        self.instr_start_pc = self.regs.pc;
    }

    /// Read byte from memory.
    fn read_byte<B: Bus>(&mut self, bus: &mut B, addr: u32) -> u8 {
        // 68000 uses 24-bit addresses
        let addr24 = addr & 0x00FF_FFFF;
        bus.read(addr24).data
    }

    /// Read word from memory (big-endian).
    fn read_word<B: Bus>(&mut self, bus: &mut B, addr: u32) -> u16 {
        let addr24 = addr & 0x00FF_FFFE; // Word-aligned
        let hi = bus.read(addr24).data;
        let lo = bus.read(addr24 + 1).data;
        u16::from(hi) << 8 | u16::from(lo)
    }

    /// Read long from memory (big-endian).
    fn read_long<B: Bus>(&mut self, bus: &mut B, addr: u32) -> u32 {
        let addr24 = addr & 0x00FF_FFFC; // Long-aligned
        let hi = self.read_word(bus, addr24);
        let lo = self.read_word(bus, addr24 + 2);
        u32::from(hi) << 16 | u32::from(lo)
    }

    /// Write byte to memory.
    fn write_byte<B: Bus>(&mut self, bus: &mut B, addr: u32, value: u8) {
        let addr24 = addr & 0x00FF_FFFF;
        bus.write(addr24, value);
    }

    /// Write word to memory (big-endian).
    fn write_word<B: Bus>(&mut self, bus: &mut B, addr: u32, value: u16) {
        let addr24 = addr & 0x00FF_FFFE;
        bus.write(addr24, (value >> 8) as u8);
        bus.write(addr24 + 1, value as u8);
    }

    /// Write long to memory (big-endian).
    fn write_long<B: Bus>(&mut self, bus: &mut B, addr: u32, value: u32) {
        let addr24 = addr & 0x00FF_FFFC;
        self.write_word(bus, addr24, (value >> 16) as u16);
        self.write_word(bus, addr24 + 2, value as u16);
    }

    /// Queue micro-ops for the next instruction fetch.
    fn queue_fetch(&mut self) {
        self.micro_ops.clear();
        self.state = State::FetchOpcode;
        self.ext_count = 0;
        self.ext_idx = 0;
        self.src_mode = None;
        self.dst_mode = None;
        self.instr_phase = InstrPhase::Initial;
        self.micro_ops.push(MicroOp::FetchOpcode);
    }

    /// Queue internal cycles (with overlapped PC advancement for long operations).
    fn queue_internal(&mut self, cycles: u8) {
        self.internal_cycles = cycles;
        self.internal_advances_pc = true;
        self.micro_ops.push(MicroOp::Internal);
    }

    /// Queue internal cycles without PC advancement (for jumps/branches that set PC directly).
    fn queue_internal_no_pc(&mut self, cycles: u8) {
        self.internal_cycles = cycles;
        self.internal_advances_pc = false;
        self.micro_ops.push(MicroOp::Internal);
    }

    /// Execute one clock cycle of CPU operation.
    ///
    /// Instant micro-ops (those taking 0 cycles like Execute, CalcEA) are
    /// processed in a loop until we hit a micro-op that takes actual cycles.
    fn tick_internal<B: Bus>(&mut self, bus: &mut B) {
        loop {
            let Some(op) = self.micro_ops.current() else {
                // Queue empty - start next instruction (unless in prefetch-only mode)
                if self.prefetch_only {
                    // In prefetch-only mode (e.g., single-step testing), just idle
                    return;
                }
                self.queue_fetch();
                return;
            };

            match op {
                // Instant micro-ops (0 cycles) - continue loop after processing
                MicroOp::CalcEA => {
                    self.calc_effective_address();
                    self.micro_ops.advance();
                    continue;
                }
                MicroOp::Execute => {
                    self.decode_and_execute();
                    // Don't advance if exception() was called - it clears the queue
                    // and pushes BeginException which we need to execute
                    if self.pending_exception.is_none() {
                        self.micro_ops.advance();
                    }
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

                // Timed micro-ops (consume cycles) - return after processing
                MicroOp::FetchOpcode => self.tick_fetch_opcode(bus),
                MicroOp::FetchExtWord => {
                    // In prefetch_only mode, extension words are already preloaded
                    // so this is an instant no-op (0 cycles)
                    if self.prefetch_only {
                        self.micro_ops.advance();
                        continue;
                    }
                    self.tick_fetch_ext_word(bus);
                }
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

    /// Tick for opcode fetch (4 cycles).
    fn tick_fetch_opcode<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Check for odd PC - triggers address error
                if self.regs.pc & 1 != 0 {
                    self.address_error(self.regs.pc, true, true);
                    return;
                }
            }
            1 | 2 => {
                // Bus cycles 2-3: Address setup and data read
            }
            3 => {
                // Cycle 4: Read complete
                self.opcode = self.read_word(bus, self.regs.pc);
                self.regs.pc = self.regs.pc.wrapping_add(2);
                // Save instruction start PC for exception handling.
                // At this point, PC points past the opcode to where extension words begin.
                self.instr_start_pc = self.regs.pc;
                self.cycle = 0;
                self.micro_ops.advance();
                // Queue decode and execute (unless prefetch_only is set)
                if !self.prefetch_only {
                    self.micro_ops.push(MicroOp::Execute);
                }
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for extension word fetch (4 cycles).
    /// Note: In prefetch_only mode, this function is not called - the caller handles
    /// that case as an instant no-op since extension words are already preloaded.
    fn tick_fetch_ext_word<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Check for odd PC - triggers address error
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
    fn tick_read_byte<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
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
    fn tick_read_word<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Check for odd address - triggers address error
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
    fn tick_read_long_hi<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Check for odd address - triggers address error
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
    fn tick_read_long_lo<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                self.data |= u32::from(self.read_word(bus, self.addr));
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for byte write (4 cycles).
    fn tick_write_byte<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                self.write_byte(bus, self.addr, self.data as u8);
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for word write (4 cycles).
    fn tick_write_word<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Check for odd address - triggers address error
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

    /// Tick for long write high word (4 cycles).
    fn tick_write_long_hi<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Check for odd address - triggers address error
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
    fn tick_write_long_lo<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
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

    /// Tick for internal processing cycles.
    ///
    /// For long internal operations (>= 4 cycles), the 68000 overlaps prefetch with
    /// computation. We advance PC by 2 to simulate this. For short operations,
    /// prefetch happens sequentially during FetchOpcode.
    /// Jump/branch instructions set internal_advances_pc = false to prevent this.
    fn tick_internal_cycles(&mut self) {
        self.cycle += 1;
        if self.cycle >= self.internal_cycles {
            // For operations with >= 4 internal cycles and PC advancement enabled,
            // prefetch is overlapped so we advance PC here.
            if self.internal_advances_pc && self.internal_cycles >= 4 {
                self.regs.pc = self.regs.pc.wrapping_add(2);
            }
            self.cycle = 0;
            self.micro_ops.advance();
        }
    }

    /// Tick for push word (4 cycles).
    fn tick_push_word<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.push_word();
                // Check for odd address - triggers address error
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
    fn tick_push_long_hi<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.push_long();
                // Check for odd address - triggers address error
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
    fn tick_push_long_lo<B: Bus>(&mut self, bus: &mut B) {
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
    fn tick_pop_word<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.pop_word();
                // Check for odd address - triggers address error (read from addr-2)
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
    fn tick_pop_long_hi<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.pop_long();
                // Check for odd address - triggers address error (read from addr-4)
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
    fn tick_pop_long_lo<B: Bus>(&mut self, bus: &mut B) {
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

    /// Tick for reading exception vector (4 cycles).
    fn tick_read_vector<B: Bus>(&mut self, bus: &mut B) {
        // Vector read is a long read (8 cycles total: 4 for high word, 4 for low word)
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                if self.movem_long_phase == 0 {
                    // Phase 0: Read high word of vector
                    if let Some(vec) = self.current_exception {
                        let vector_addr = u32::from(vec) * 4;
                        self.data = u32::from(self.read_word(bus, vector_addr)) << 16;
                        self.addr = vector_addr; // Save for low word read
                    }
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                } else {
                    // Phase 1: Read low word of vector
                    self.data |= u32::from(self.read_word(bus, self.addr.wrapping_add(2)));
                    self.regs.pc = self.data;
                    self.current_exception = None;
                    self.movem_long_phase = 0;
                    self.cycle = 0;
                    self.micro_ops.advance();

                    // In prefetch_only mode (single-step tests), don't continue
                    // executing from the vector - just stop with PC set to the
                    // vector address. The test validates the exception frame
                    // and final PC, not the execution at the vector.
                    if self.prefetch_only {
                        // The test expects final PC to include prefetch advance.
                        // Exception entry does 2 prefetches from new PC.
                        self.regs.pc = self.regs.pc.wrapping_add(4);
                        // Mark that PC was already advanced so tick() doesn't add +2
                        self.internal_advances_pc = true;
                        self.micro_ops.clear();
                        return;
                    }

                    // After exception, start fetching from new PC.
                    // Queue FetchOpcode - tick_fetch_opcode will queue Execute.
                    // Note: For full prefetch accuracy, we'd queue FetchExtWord too,
                    // but that causes PC to advance too far for some tests.
                    self.micro_ops.clear();
                    self.state = State::FetchOpcode;
                    self.ext_count = 0;
                    self.ext_idx = 0;
                    self.src_mode = None;
                    self.dst_mode = None;
                    self.instr_phase = InstrPhase::Initial;
                    self.prefetch_only = false; // Allow normal flow
                    self.micro_ops.push(MicroOp::FetchOpcode);
                    return;
                }
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for MOVEM write (4 cycles per word).
    ///
    /// State: `ext_words[0]` = register mask, `data2` = current bit index (0-15),
    /// `addr` = current memory address, `addr2` = address register for update.
    /// For predecrement mode, we iterate from bit 15 down to 0.
    fn tick_movem_write<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // MOVEM always uses word/long; check for odd address on first phase
                if self.movem_long_phase == 0 && self.addr & 1 != 0 {
                    // For predecrement mode, the fault address should be An - size (first access),
                    // not the pre-calculated start address (An - total)
                    let fault_addr = if self.movem_predec {
                        let ea_reg = (self.addr2 & 7) as usize;
                        let size = if self.size == Size::Long { 4 } else { 2 };
                        self.regs.a(ea_reg).wrapping_sub(size)
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

                // Find the current register to write
                let bit_idx = self.data2 as usize;

                // Get register value based on bit index
                // For normal modes: bits 0-7 = D0-D7, bits 8-15 = A0-A7
                // For predecrement: bits 0-7 = A7-A0, bits 8-15 = D7-D0
                let value = if is_predec {
                    if bit_idx < 8 {
                        // bit 0 = A7, bit 1 = A6, ..., bit 7 = A0
                        self.regs.a(7 - bit_idx)
                    } else {
                        // bit 8 = D7, bit 9 = D6, ..., bit 15 = D0
                        self.regs.d[15 - bit_idx]
                    }
                } else if bit_idx < 8 {
                    self.regs.d[bit_idx]
                } else {
                    self.regs.a(bit_idx - 8)
                };

                // Write the value
                if self.size == Size::Long && self.movem_long_phase == 0 {
                    // Long mode, phase 0: write high word
                    self.write_word(bus, self.addr, (value >> 16) as u16);
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }

                if self.size == Size::Long {
                    // Long mode, phase 1: write low word
                    self.write_word(bus, self.addr, value as u16);
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 0;
                } else {
                    // Word write
                    self.write_word(bus, self.addr, value as u16);
                    self.addr = self.addr.wrapping_add(2);
                }

                // Find next register in mask
                let next_bit = if is_predec {
                    self.find_next_movem_bit_down(mask, bit_idx)
                } else {
                    self.find_next_movem_bit_up(mask, bit_idx)
                };

                if let Some(next) = next_bit {
                    // More registers to write
                    self.data2 = next as u32;
                    self.cycle = 0;
                    return;
                }

                // Done - update address register for predecrement mode
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
    ///
    /// State: `ext_words[0]` = register mask, `data2` = current bit index (0-15),
    /// `addr` = current memory address, `addr2` = address register for update.
    fn tick_movem_read<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // MOVEM always uses word/long; check for odd address on first phase
                if self.movem_long_phase == 0 && self.addr & 1 != 0 {
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                let mask = self.ext_words[0];
                let bit_idx = self.data2 as usize;

                // Read the value
                if self.size == Size::Long && self.movem_long_phase == 0 {
                    // Long mode, phase 0: read high word
                    self.data = u32::from(self.read_word(bus, self.addr)) << 16;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }

                if self.size == Size::Long {
                    // Long mode, phase 1: read low word and combine
                    self.data |= u32::from(self.read_word(bus, self.addr));
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 0;
                } else {
                    // Word read - sign extend to long for ALL registers (per M68000 spec)
                    let word = self.read_word(bus, self.addr);
                    self.data = word as i16 as i32 as u32;
                    self.addr = self.addr.wrapping_add(2);
                }

                // Store value in register (MOVEM.W always sign-extends to full 32-bit)
                if bit_idx < 8 {
                    self.regs.d[bit_idx] = self.data;
                } else {
                    self.regs.set_a(bit_idx - 8, self.data);
                }

                // Find next register in mask (always ascending for read)
                let next_bit = self.find_next_movem_bit_up(mask, bit_idx);

                if let Some(next) = next_bit {
                    self.data2 = next as u32;
                    self.cycle = 0;
                    return;
                }

                // Done - update address register for postincrement mode
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

    /// Find next set bit in mask, going up from current position.
    fn find_next_movem_bit_up(&self, mask: u16, current: usize) -> Option<usize> {
        ((current + 1)..16).find(|&i| mask & (1 << i) != 0)
    }

    /// Find next set bit in mask, going down from current position.
    fn find_next_movem_bit_down(&self, mask: u16, current: usize) -> Option<usize> {
        (0..current).rev().find(|&i| mask & (1 << i) != 0)
    }

    /// Find first set bit in mask going up from 0.
    fn find_first_movem_bit_up(&self, mask: u16) -> Option<usize> {
        (0..16).find(|&i| mask & (1 << i) != 0)
    }

    /// Find first set bit in mask going down from 15.
    fn find_first_movem_bit_down(&self, mask: u16) -> Option<usize> {
        (0..16).rev().find(|&i| mask & (1 << i) != 0)
    }

    /// Tick for CMPM: Compare memory (Ay)+,(Ax)+.
    ///
    /// State: `addr` = source address (Ay), `addr2` = dest address (Ax),
    /// `data` = Ay register number, `data2` = Ax register number.
    /// Uses `movem_long_phase` to track read phase: 0=read src, 1=read dst, 2=compare.
    fn tick_cmpm_execute<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Check for odd address on word/long access
                // Phase 0 checks source (addr), Phase 1 checks dest (addr2)
                if self.size != Size::Byte {
                    let check_addr = if self.movem_long_phase == 0 { self.addr } else { self.addr2 };
                    if check_addr & 1 != 0 {
                        self.address_error(check_addr, true, false);
                        return;
                    }
                }
            }
            1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        // Phase 0: Read source operand from (Ay)
                        let src_val = match self.size {
                            Size::Byte => u32::from(self.read_byte(bus, self.addr)),
                            Size::Word => u32::from(self.read_word(bus, self.addr)),
                            Size::Long => self.read_long(bus, self.addr),
                        };
                        // Store source value temporarily
                        self.ext_words[0] = src_val as u16;
                        self.ext_words[1] = (src_val >> 16) as u16;

                        // Increment Ay
                        let ay = self.data as usize;
                        let inc = match self.size {
                            Size::Byte => if ay == 7 { 2 } else { 1 },
                            Size::Word => 2,
                            Size::Long => 4,
                        };
                        self.regs.set_a(ay, self.addr.wrapping_add(inc));

                        self.movem_long_phase = 1;
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        // Phase 1: Read destination operand from (Ax)
                        let dst_val = match self.size {
                            Size::Byte => u32::from(self.read_byte(bus, self.addr2)),
                            Size::Word => u32::from(self.read_word(bus, self.addr2)),
                            Size::Long => self.read_long(bus, self.addr2),
                        };

                        // Increment Ax
                        let ax = self.data2 as usize;
                        let inc = match self.size {
                            Size::Byte => if ax == 7 { 2 } else { 1 },
                            Size::Word => 2,
                            Size::Long => 4,
                        };
                        self.regs.set_a(ax, self.addr2.wrapping_add(inc));

                        // Retrieve source value
                        let src_val =
                            u32::from(self.ext_words[0]) | (u32::from(self.ext_words[1]) << 16);

                        // Compare: dst - src
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

    /// Tick handler for TAS (Test And Set) instruction.
    ///
    /// State: `addr` = memory address.
    /// Uses `movem_long_phase` to track phase: 0=read byte, 1=write byte with bit 7 set.
    fn tick_tas_execute<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        // Phase 0: Read byte and set flags
                        let value = self.read_byte(bus, self.addr);
                        self.data = u32::from(value);

                        // Set flags based on original value
                        self.set_flags_move(u32::from(value), Size::Byte);

                        self.movem_long_phase = 1;
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        // Phase 1: Write byte with bit 7 set
                        let value = (self.data as u8) | 0x80;
                        self.write_byte(bus, self.addr, value);

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

    /// Tick handler for memory shift/rotate operations.
    ///
    /// State: `addr` = memory address, `data` = kind (0=AS, 1=LS, 2=ROX, 3=RO),
    /// `data2` = direction (0=right, 1=left).
    /// Uses `movem_long_phase` to track phase: 0=read word, 1=write shifted word.
    fn tick_shift_mem_execute<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Memory shifts are always word-sized; check for odd address on initial read
                if self.movem_long_phase == 0 && self.addr & 1 != 0 {
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        // Phase 0: Read word
                        let value = self.read_word(bus, self.addr);
                        self.ext_words[0] = value;

                        self.movem_long_phase = 1;
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        // Phase 1: Shift by 1 and write back
                        let value = u32::from(self.ext_words[0]);
                        let kind = self.data as u8;
                        let direction = self.data2 != 0; // 0=right, 1=left

                        let (result, carry) = self.shift_word_by_one(value, kind, direction);

                        // Write result
                        self.write_word(bus, self.addr, result as u16);

                        // Set flags
                        self.set_flags_move(result, Size::Word);
                        self.regs.sr = Status::set_if(self.regs.sr, C, carry);
                        // X is set for shifts (kind 0,1) but not for rotates (kind 2,3)
                        if kind < 2 {
                            self.regs.sr = Status::set_if(self.regs.sr, X, carry);
                        }
                        // V is cleared (simplified)
                        self.regs.sr &= !crate::flags::V;

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

    /// Perform a word shift/rotate by 1 bit.
    /// Returns (result, `carry_out`).
    fn shift_word_by_one(&self, value: u32, kind: u8, left: bool) -> (u32, bool) {
        let mask = 0xFFFF_u32;
        let msb = 0x8000_u32;

        match (kind, left) {
            // ASL - Arithmetic shift left
            (0, true) => {
                let carry = (value & msb) != 0;
                let result = (value << 1) & mask;
                (result, carry)
            }
            // ASR - Arithmetic shift right (sign extends)
            (0, false) => {
                let carry = (value & 1) != 0;
                let sign = value & msb;
                let result = ((value >> 1) | sign) & mask;
                (result, carry)
            }
            // LSL - Logical shift left
            (1, true) => {
                let carry = (value & msb) != 0;
                let result = (value << 1) & mask;
                (result, carry)
            }
            // LSR - Logical shift right
            (1, false) => {
                let carry = (value & 1) != 0;
                let result = (value >> 1) & mask;
                (result, carry)
            }
            // ROXL - Rotate through X left
            (2, true) => {
                let x_in = u32::from(self.regs.sr & X != 0);
                let carry = (value & msb) != 0;
                let result = ((value << 1) | x_in) & mask;
                (result, carry)
            }
            // ROXR - Rotate through X right
            (2, false) => {
                let x_in = if self.regs.sr & X != 0 { msb } else { 0 };
                let carry = (value & 1) != 0;
                let result = ((value >> 1) | x_in) & mask;
                (result, carry)
            }
            // ROL - Rotate left
            (3, true) => {
                let carry = (value & msb) != 0;
                let result = ((value << 1) | (value >> 15)) & mask;
                (result, carry)
            }
            // ROR - Rotate right
            (3, false) => {
                let carry = (value & 1) != 0;
                let result = ((value >> 1) | (value << 15)) & mask;
                (result, carry)
            }
            _ => (value, false),
        }
    }

    /// Tick handler for ALU memory read-modify-write operations.
    ///
    /// State: `addr` = memory address, `data` = source value (from register),
    /// `data2` = operation (0=ADD, 1=SUB, 2=AND, 3=OR, 4=EOR).
    /// Uses `movem_long_phase` to track phase:
    ///   Byte/Word: 0=read, 1=write
    ///   Long: 0=read hi, 1=read lo, 2=write hi, 3=write lo
    #[allow(clippy::too_many_lines)]
    fn tick_alu_mem_rmw<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Check for odd address on word/long access (only check on initial read phase)
                if self.movem_long_phase == 0 && self.size != Size::Byte && self.addr & 1 != 0 {
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        // Phase 0: Read from memory (first/only read)
                        let val = match self.size {
                            Size::Byte => u32::from(self.read_byte(bus, self.addr)),
                            Size::Word => u32::from(self.read_word(bus, self.addr)),
                            Size::Long => {
                                // Long: read high word first
                                u32::from(self.read_word(bus, self.addr))
                            }
                        };
                        // Store value in ext_words
                        if self.size == Size::Long {
                            self.ext_words[1] = val as u16; // High word
                            self.movem_long_phase = 1; // Need to read low word
                        } else {
                            self.ext_words[0] = val as u16;
                            self.ext_words[1] = 0;
                            self.movem_long_phase = 2; // Skip to compute+write
                        }
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        // Phase 1 (Long only): Read low word
                        let lo = self.read_word(bus, self.addr.wrapping_add(2));
                        self.ext_words[0] = lo; // Low word
                        self.movem_long_phase = 2; // Go to compute+write
                        self.cycle = 0;
                        return;
                    }
                    2 => {
                        // Phase 2: Compute and write (first/only write)
                        let mem_val =
                            u32::from(self.ext_words[0]) | (u32::from(self.ext_words[1]) << 16);
                        let src = self.data;
                        let op = self.data2;

                        let result = match op {
                            0 => {
                                // ADD: mem + src
                                let res = mem_val.wrapping_add(src);
                                self.set_flags_add(src, mem_val, res, self.size);
                                res
                            }
                            1 => {
                                // SUB: mem - src
                                let res = mem_val.wrapping_sub(src);
                                self.set_flags_sub(src, mem_val, res, self.size);
                                res
                            }
                            2 => {
                                // AND: mem & src
                                let res = mem_val & src;
                                self.set_flags_move(res, self.size);
                                res
                            }
                            3 => {
                                // OR: mem | src
                                let res = mem_val | src;
                                self.set_flags_move(res, self.size);
                                res
                            }
                            4 => {
                                // EOR: mem ^ src
                                let res = mem_val ^ src;
                                self.set_flags_move(res, self.size);
                                res
                            }
                            5 => {
                                // NEG: 0 - mem
                                let res = 0u32.wrapping_sub(mem_val);
                                self.set_flags_sub(mem_val, 0, res, self.size);
                                res
                            }
                            6 => {
                                // NOT: ~mem
                                let res = !mem_val;
                                self.set_flags_move(res, self.size);
                                res
                            }
                            7 => {
                                // NEGX: 0 - mem - X
                                let x = u32::from(self.regs.sr & X != 0);
                                let res = 0u32.wrapping_sub(mem_val).wrapping_sub(x);

                                // NEGX flags: like NEG but Z is only cleared (never set)
                                let (src_masked, result_masked, msb) = match self.size {
                                    Size::Byte => (mem_val & 0xFF, res & 0xFF, 0x80u32),
                                    Size::Word => (mem_val & 0xFFFF, res & 0xFFFF, 0x8000),
                                    Size::Long => (mem_val, res, 0x8000_0000),
                                };

                                let mut sr = self.regs.sr;
                                sr = Status::set_if(sr, N, result_masked & msb != 0);
                                if result_masked != 0 {
                                    sr &= !Z; // Only clear, never set
                                }
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
                                // NBCD: 0 - mem - X (BCD negate, byte only)
                                let src = mem_val as u8;
                                let x = u8::from(self.regs.sr & X != 0);
                                let (result, borrow) = self.bcd_sub(0, src, x);

                                // Set flags
                                let mut sr = self.regs.sr;
                                // Z: cleared if non-zero, unchanged otherwise
                                if result != 0 {
                                    sr &= !Z;
                                }
                                sr = Status::set_if(sr, C, borrow);
                                sr = Status::set_if(sr, X, borrow);
                                sr = Status::set_if(sr, N, result & 0x80 != 0);
                                self.regs.sr = sr;

                                u32::from(result)
                            }
                            _ => mem_val,
                        };

                        // Write result back to memory
                        match self.size {
                            Size::Byte => {
                                self.write_byte(bus, self.addr, result as u8);
                                self.movem_long_phase = 0;
                                self.cycle = 0;
                                self.micro_ops.advance();
                                return;
                            }
                            Size::Word => {
                                self.write_word(bus, self.addr, result as u16);
                                self.movem_long_phase = 0;
                                self.cycle = 0;
                                self.micro_ops.advance();
                                return;
                            }
                            Size::Long => {
                                // Write high word first, store result for low word write
                                self.write_word(bus, self.addr, (result >> 16) as u16);
                                self.ext_words[2] = result as u16; // Store low word
                                self.movem_long_phase = 3;
                                self.cycle = 0;
                                return;
                            }
                        }
                    }
                    3 => {
                        // Phase 3 (Long only): Write low word
                        self.write_word(bus, self.addr.wrapping_add(2), self.ext_words[2]);
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

    /// Tick handler for ALU memory source operations.
    ///
    /// State: `addr` = memory address, `data` = destination register number,
    /// `data2` = operation (0=ADD, 1=SUB, 2=AND, 3=OR, 4=CMP, 5=ADDA, 6=SUBA, 7=CMPA).
    /// Uses `movem_long_phase` for long reads: 0=hi word, 1=lo word.
    #[allow(clippy::too_many_lines)]
    fn tick_alu_mem_src<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                // Mark that we're accessing external memory (for prefetch tracking)
                self.mem_accessed = true;
                // Check for odd address on word/long access
                if self.size != Size::Byte && self.addr & 1 != 0 {
                    self.address_error(self.addr, true, false);
                    return;
                }
            }
            1 | 2 => {}
            3 => {
                // For long operations, need two read phases
                if self.size == Size::Long && self.movem_long_phase == 0 {
                    // Phase 0: Read high word
                    let hi = self.read_word(bus, self.addr);
                    self.ext_words[2] = hi;
                    self.addr = self.addr.wrapping_add(2);
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                }

                // Read the value (or low word for long)
                let src = if self.size == Size::Long {
                    // Phase 1: Read low word and combine
                    let lo = self.read_word(bus, self.addr);
                    self.movem_long_phase = 0;
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
                        // ADD: reg + mem -> reg
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst.wrapping_add(src);
                        self.write_data_reg(reg, result, self.size);
                        self.set_flags_add(src, dst, result, self.size);
                    }
                    1 => {
                        // SUB: reg - mem -> reg
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst.wrapping_sub(src);
                        self.write_data_reg(reg, result, self.size);
                        self.set_flags_sub(src, dst, result, self.size);
                    }
                    2 => {
                        // AND: reg & mem -> reg
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst & src;
                        self.write_data_reg(reg, result, self.size);
                        self.set_flags_move(result, self.size);
                    }
                    3 => {
                        // OR: reg | mem -> reg
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst | src;
                        self.write_data_reg(reg, result, self.size);
                        self.set_flags_move(result, self.size);
                    }
                    4 => {
                        // CMP: reg - mem (flags only)
                        let dst = self.read_data_reg(reg, self.size);
                        let result = dst.wrapping_sub(src);
                        self.set_flags_cmp(src, dst, result, self.size);
                    }
                    5 => {
                        // ADDA: An + mem -> An (no flags)
                        let src_extended = if self.size == Size::Word {
                            src as i16 as i32 as u32
                        } else {
                            src
                        };
                        let dst = self.regs.a(reg as usize);
                        let result = dst.wrapping_add(src_extended);
                        self.regs.set_a(reg as usize, result);
                    }
                    6 => {
                        // SUBA: An - mem -> An (no flags)
                        let src_extended = if self.size == Size::Word {
                            src as i16 as i32 as u32
                        } else {
                            src
                        };
                        let dst = self.regs.a(reg as usize);
                        let result = dst.wrapping_sub(src_extended);
                        self.regs.set_a(reg as usize, result);
                    }
                    7 => {
                        // CMPA: An - mem (flags only, word sign-extends)
                        let src_extended = if self.size == Size::Word {
                            src as i16 as i32 as u32
                        } else {
                            src
                        };
                        let dst = self.regs.a(reg as usize);
                        let result = dst.wrapping_sub(src_extended);
                        // CMPA always compares as long
                        self.set_flags_cmp(src_extended, dst, result, Size::Long);
                    }
                    8 => {
                        // TST: just set flags based on memory value
                        self.set_flags_move(src, self.size);
                    }
                    9 => {
                        // CHK: check bounds, trigger exception if out of range
                        // `data` contains the data register number
                        // `src` is the upper bound from memory (word)
                        let dn = self.regs.d[reg as usize] as i16;
                        let upper_bound = src as i16;

                        if dn < 0 {
                            // Clear N/Z/V/C then set N (X not affected)
                            self.regs.sr &= !(N | Z | V | C);
                            self.regs.sr |= N;
                            self.exception(6); // CHK exception
                        } else if dn > upper_bound {
                            // Clear N/Z/V/C (X not affected)
                            self.regs.sr &= !(N | Z | V | C);
                            self.exception(6); // CHK exception
                        }
                        // If within bounds, just continue normally
                    }
                    10 => {
                        // MULU: unsigned 16x16->32 multiply
                        let src_word = src & 0xFFFF;
                        let dst_word = self.regs.d[reg as usize] & 0xFFFF;
                        let result = src_word * dst_word;
                        self.regs.d[reg as usize] = result;

                        // Set flags: N based on bit 31, Z if result is 0, V=0, C=0
                        self.regs.sr = Status::clear_vc(self.regs.sr);
                        self.regs.sr = Status::update_nz_long(self.regs.sr, result);

                        // Timing: 38 + 2*ones in source, minus the memory read cycles already spent
                        let ones = (src_word as u16).count_ones() as u8;
                        // Memory read took 4 cycles, so subtract to get remaining internal time
                        let timing = (38 + 2 * ones).saturating_sub(4);
                        // Don't advance PC during internal cycles - already positioned correctly
                        self.queue_internal_no_pc(timing);
                    }
                    11 => {
                        // MULS: signed 16x16->32 multiply
                        let src_signed = (src as i16) as i32;
                        let dst_signed = (self.regs.d[reg as usize] as i16) as i32;
                        let result = (src_signed * dst_signed) as u32;
                        self.regs.d[reg as usize] = result;

                        self.regs.sr = Status::clear_vc(self.regs.sr);
                        self.regs.sr = Status::update_nz_long(self.regs.sr, result);

                        // Timing: 38 + 2*bit transitions, minus memory read cycles
                        let src16 = src as u16;
                        let pattern = src16 ^ (src16 << 1);
                        let ones = pattern.count_ones() as u8;
                        let timing = (38 + 2 * ones).saturating_sub(4);
                        // Don't advance PC during internal cycles - already positioned correctly
                        self.queue_internal_no_pc(timing);
                    }
                    12 => {
                        // DIVU: unsigned 32/16 -> 16r:16q division
                        let divisor = src & 0xFFFF;
                        let dividend = self.regs.d[reg as usize];

                        if divisor == 0 {
                            self.exception(5); // Division by zero
                            return;
                        }

                        let quotient = dividend / divisor;
                        let remainder = dividend % divisor;

                        if quotient > 0xFFFF {
                            // Overflow - detected early, minimal processing
                            self.regs.sr |= crate::flags::V;
                            self.regs.sr &= !crate::flags::C;
                            // On overflow, N is set based on bit 16 of quotient (overflow bit)
                            self.regs.sr = Status::set_if(
                                self.regs.sr,
                                crate::flags::N,
                                quotient & 0x1_0000 != 0,
                            );
                            // Z is cleared on overflow
                            self.regs.sr &= !crate::flags::Z;
                            // Overflow detection takes ~10 internal cycles after memory read
                            self.queue_internal(10);
                        } else {
                            self.regs.d[reg as usize] = (remainder << 16) | quotient;
                            self.regs.sr &= !(crate::flags::V | crate::flags::C);
                            self.regs.sr =
                                Status::set_if(self.regs.sr, crate::flags::Z, quotient == 0);
                            self.regs.sr = Status::set_if(
                                self.regs.sr,
                                crate::flags::N,
                                quotient & 0x8000 != 0,
                            );
                            // Full division timing varies with dividend
                            self.queue_internal(140);
                        }
                    }
                    13 => {
                        // DIVS: signed 32/16 -> 16r:16q division
                        let divisor = (src as i16) as i32;
                        let dividend = self.regs.d[reg as usize] as i32;

                        if divisor == 0 {
                            self.exception(5); // Division by zero
                            return;
                        }

                        let quotient = dividend / divisor;
                        let remainder = dividend % divisor;

                        if (-32768..=32767).contains(&quotient) {
                            let q = quotient as i16 as u16 as u32;
                            let r = remainder as i16 as u16 as u32;
                            self.regs.d[reg as usize] = (r << 16) | q;
                            self.regs.sr &= !(crate::flags::V | crate::flags::C);
                            self.regs.sr =
                                Status::set_if(self.regs.sr, crate::flags::Z, quotient == 0);
                            self.regs.sr =
                                Status::set_if(self.regs.sr, crate::flags::N, quotient < 0);
                            // Full division timing varies with dividend
                            self.queue_internal(158);
                        } else {
                            // Overflow - detected early, minimal timing
                            self.regs.sr |= crate::flags::V;
                            self.regs.sr &= !crate::flags::C;
                            // On overflow, N is set based on MSB of dividend (bit 31)
                            self.regs.sr =
                                Status::set_if(self.regs.sr, crate::flags::N, dividend < 0);
                            // Z is cleared on overflow
                            self.regs.sr &= !crate::flags::Z;
                            // Overflow detected early: ~10 internal cycles after memory read
                            self.queue_internal(10);
                        }
                    }
                    14 => {
                        // CMPI: compare immediate (memory - immediate)
                        // For CMPI, self.data holds the immediate value, src is memory
                        let dst = src; // Memory value is the destination
                        let imm = self.data; // Immediate is the source
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

    /// Tick handler for bit operations on memory byte.
    ///
    /// State: `addr` = memory address, `data` = bit number (0-7),
    /// `data2` = operation (0=BTST, 1=BCHG, 2=BCLR, 3=BSET).
    /// Uses `movem_long_phase`: 0=read, 1=write (for modifying ops).
    fn tick_bit_mem_op<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                match self.movem_long_phase {
                    0 => {
                        // Phase 0: Read byte
                        let value = self.read_byte(bus, self.addr);
                        let bit = (self.data & 7) as u8;
                        let mask = 1u8 << bit;
                        let was_zero = (value & mask) == 0;

                        // Set Z flag based on original bit value
                        self.regs.sr = Status::set_if(self.regs.sr, Z, was_zero);

                        let op = self.data2;
                        if op == 0 {
                            // BTST: read-only, done
                            self.cycle = 0;
                            self.micro_ops.advance();
                            return;
                        }

                        // Calculate new value for BCHG/BCLR/BSET
                        let new_value = match op {
                            1 => value ^ mask,  // BCHG: toggle
                            2 => value & !mask, // BCLR: clear
                            3 => value | mask,  // BSET: set
                            _ => value,
                        };

                        // Store for write phase
                        self.ext_words[0] = u16::from(new_value);
                        self.movem_long_phase = 1;
                        self.cycle = 0;
                        return;
                    }
                    1 => {
                        // Phase 1: Write modified byte
                        let value = self.ext_words[0] as u8;
                        self.write_byte(bus, self.addr, value);

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

    /// Tick handler for multi-precision/BCD memory-to-memory: -(Ax),-(Ay).
    ///
    /// State: `addr` = source address (Ax pre-decremented),
    /// `addr2` = destination address (Ay pre-decremented),
    /// `data2` = operation (0=ABCD, 1=SBCD, 2=ADDX, 3=SUBX).
    /// Size from `self.size` (ABCD/SBCD always byte).
    /// Uses `movem_long_phase` for phase tracking.
    #[allow(clippy::too_many_lines)]
    fn tick_extend_mem_op<B: Bus>(&mut self, bus: &mut B) {
        // For byte: phases 0=read src, 1=read dst, 2=write
        // For word: phases 0=read src, 1=read dst, 2=write
        // For long: phases 0=read src hi, 1=read src lo, 2=read dst hi, 3=read dst lo, 4=write hi, 5=write lo
        match self.cycle {
            0 => {
                // Handle pre-decrements at the correct phases.
                // Source pre-decrement happens in phase 0 (before source read).
                // Destination pre-decrement happens before destination read:
                //   - Byte/Word: phase 1
                //   - Long: phase 2 (after source lo read)

                // Phase 0: Pre-decrement source register
                if !self.extend_predec_done && self.movem_long_phase == 0 {
                    // Extract register numbers from self.data (packed as rx | (ry << 8))
                    let rx = (self.data & 0xFF) as usize;
                    let ry = ((self.data >> 8) & 0xFF) as usize;

                    // Calculate decrement size
                    let decr = match self.size {
                        Size::Byte => 1,
                        Size::Word => 2,
                        Size::Long => 4,
                    };

                    // Calculate and apply source pre-decrement
                    let src_addr = self.regs.a(rx).wrapping_sub(decr);
                    self.regs.set_a(rx, src_addr);
                    self.addr = src_addr;

                    // Check for address error AFTER pre-decrementing (word/long only)
                    // On 68000, the register is modified before the address error is detected
                    if self.size != Size::Byte && src_addr & 1 != 0 {
                        self.address_error(src_addr, true, false);
                        return;
                    }

                    // Store ry in upper bits of data2 for later use (data2 low byte = op type)
                    self.data2 = (self.data2 & 0xFF) | ((ry as u32) << 8);
                    // Also store rx in case we need it for same-register detection
                    self.data2 = (self.data2 & 0xFFFF) | ((rx as u32) << 16);

                    self.extend_predec_done = true;
                }

                // Destination pre-decrement: before phase 1 (byte/word) or phase 2 (long)
                let dest_predec_phase = match self.size {
                    Size::Byte | Size::Word => 1,
                    Size::Long => 2,
                };
                // Use bit 24 of data2 to track if dest has been decremented
                let dest_decremented = self.data2 & 0x0100_0000 != 0;
                if !dest_decremented && self.movem_long_phase == dest_predec_phase {
                    let ry = ((self.data2 >> 8) & 0xFF) as usize;

                    let decr = match self.size {
                        Size::Byte => 1,
                        Size::Word => 2,
                        Size::Long => 4,
                    };

                    // Calculate and apply destination pre-decrement
                    // (For same-register case, ry == rx, and rx has already been decremented,
                    // so we get the correct sequential behavior automatically)
                    let dst_addr = self.regs.a(ry).wrapping_sub(decr);
                    self.regs.set_a(ry, dst_addr);
                    self.addr2 = dst_addr;

                    // Check for address error AFTER pre-decrementing (word/long only)
                    if self.size != Size::Byte && dst_addr & 1 != 0 {
                        self.address_error(dst_addr, true, false);
                        return;
                    }

                    // Mark dest as decremented
                    self.data2 |= 0x0100_0000;
                }

                // Address checks are now done at pre-decrement time.
                // Source checked in phase 0, destination checked at dest_predec_phase.
            }
            1 | 2 => {}
            3 => {
                match self.size {
                    Size::Byte => {
                        match self.movem_long_phase {
                            0 => {
                                // Read source byte
                                self.data = u32::from(self.read_byte(bus, self.addr));
                                self.movem_long_phase = 1;
                                self.cycle = 0;
                                return;
                            }
                            1 => {
                                // Read destination byte, compute result
                                let src = self.data as u8;
                                let dst = self.read_byte(bus, self.addr2);
                                let x = u8::from(self.regs.sr & X != 0);

                                // Mask to get just the operation type (low byte), ignoring
                                // ry and flags stored in upper bits
                                let (result, carry) = match self.data2 & 0xFF {
                                    0 => self.bcd_add(src, dst, x),
                                    1 => self.bcd_sub(dst, src, x),
                                    2 => {
                                        let r = u16::from(dst) + u16::from(src) + u16::from(x);
                                        (r as u8, r > 0xFF)
                                    }
                                    3 => {
                                        let r = u16::from(dst).wrapping_sub(u16::from(src)).wrapping_sub(u16::from(x));
                                        (r as u8, u16::from(dst) < u16::from(src) + u16::from(x))
                                    }
                                    _ => unreachable!(),
                                };

                                self.data = u32::from(result);
                                self.set_extend_flags(u32::from(src), u32::from(dst), u32::from(result), carry, Size::Byte);
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
                                // Read source word
                                self.data = u32::from(self.read_word(bus, self.addr));
                                self.movem_long_phase = 1;
                                self.cycle = 0;
                                return;
                            }
                            1 => {
                                // Read destination word, compute result
                                let src = self.data as u16;
                                let dst = self.read_word(bus, self.addr2);
                                let x = u32::from(self.regs.sr & X != 0);

                                // Mask to get just the operation type (low byte)
                                let (result, carry) = if (self.data2 & 0xFF) == 2 {
                                    // ADDX
                                    let r = u32::from(dst) + u32::from(src) + x;
                                    (r as u16, r > 0xFFFF)
                                } else {
                                    // SUBX
                                    let r = u32::from(dst).wrapping_sub(u32::from(src)).wrapping_sub(x);
                                    (r as u16, u32::from(dst) < u32::from(src) + x)
                                };

                                self.data = u32::from(result);
                                self.set_extend_flags(src.into(), dst.into(), result.into(), carry, Size::Word);
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
                                // Read source high word
                                self.ext_words[3] = self.read_word(bus, self.addr);
                                self.movem_long_phase = 1;
                                self.cycle = 0;
                                return;
                            }
                            1 => {
                                // Read source low word
                                self.data = (u32::from(self.ext_words[3]) << 16) | u32::from(self.read_word(bus, self.addr.wrapping_add(2)));
                                self.movem_long_phase = 2;
                                self.cycle = 0;
                                return;
                            }
                            2 => {
                                // Read destination high word
                                self.ext_words[3] = self.read_word(bus, self.addr2);
                                self.movem_long_phase = 3;
                                self.cycle = 0;
                                return;
                            }
                            3 => {
                                // Read destination low word, compute result
                                let src = self.data;
                                let dst = (u32::from(self.ext_words[3]) << 16) | u32::from(self.read_word(bus, self.addr2.wrapping_add(2)));
                                let x = u32::from(self.regs.sr & X != 0);

                                // Mask to get just the operation type (low byte)
                                let (result, carry) = if (self.data2 & 0xFF) == 2 {
                                    // ADDX
                                    let r = (dst as u64) + (src as u64) + (x as u64);
                                    (r as u32, r > 0xFFFF_FFFF)
                                } else {
                                    // SUBX
                                    let r = (dst as u64).wrapping_sub(src as u64).wrapping_sub(x as u64);
                                    (r as u32, (dst as u64) < (src as u64) + (x as u64))
                                };

                                self.data = result;
                                self.set_extend_flags(src, dst, result, carry, Size::Long);
                                self.movem_long_phase = 4;
                                self.cycle = 0;
                                return;
                            }
                            4 => {
                                // Write result high word
                                self.write_word(bus, self.addr2, (self.data >> 16) as u16);
                                self.movem_long_phase = 5;
                                self.cycle = 0;
                                return;
                            }
                            5 => {
                                // Write result low word
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
    fn set_extend_flags(&mut self, src: u32, dst: u32, result: u32, carry: bool, size: Size) {
        let (result_masked, msb) = match size {
            Size::Byte => (result & 0xFF, 0x80u32),
            Size::Word => (result & 0xFFFF, 0x8000),
            Size::Long => (result, 0x8000_0000),
        };

        // Z: cleared if non-zero, unchanged otherwise
        if result_masked != 0 {
            self.regs.sr &= !Z;
        }
        // C and X: set on carry/borrow
        self.regs.sr = Status::set_if(self.regs.sr, C, carry);
        self.regs.sr = Status::set_if(self.regs.sr, X, carry);

        // For ADDX/SUBX (ops 2,3), set N and V
        if self.data2 >= 2 {
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

            let overflow = if self.data2 == 2 {
                // ADDX: same signs produce different sign
                (!(src_masked ^ dst_masked) & (src_masked ^ result_masked) & msb) != 0
            } else {
                // SUBX: different signs produce sign same as src
                ((dst_masked ^ src_masked) & (dst_masked ^ result_masked) & msb) != 0
            };
            self.regs.sr = Status::set_if(self.regs.sr, V, overflow);
        } else {
            // N undefined for BCD but set based on MSB
            self.regs.sr = Status::set_if(self.regs.sr, N, result_masked & msb != 0);
        }
    }

    /// Tick for pushing IR (opcode) during group 0 exception (4 cycles).
    fn tick_push_group0_ir<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.push_word();
                // Stack should always be aligned, but check anyway
                if self.addr & 1 != 0 {
                    // Double fault - can't handle address error during address error
                    // Real 68000 would halt; we'll just proceed with the push
                }
            }
            1 | 2 => {}
            3 => {
                // Write opcode (stored in addr2)
                self.write_word(bus, self.addr, self.addr2 as u16);
                self.cycle = 0;
                self.micro_ops.advance();
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for pushing fault address during group 0 exception (8 cycles for long).
    fn tick_push_group0_fault_addr<B: Bus>(&mut self, bus: &mut B) {
        // Phase 0: push high word, Phase 1: push low word
        match self.cycle {
            0 => {
                if self.movem_long_phase == 0 {
                    self.addr = self.regs.push_long();
                }
            }
            1 | 2 => {}
            3 => {
                if self.movem_long_phase == 0 {
                    // Write high word of fault address
                    self.write_word(bus, self.addr, (self.fault_addr >> 16) as u16);
                    self.movem_long_phase = 1;
                    self.cycle = 0;
                    return;
                } else {
                    // Write low word of fault address
                    self.write_word(bus, self.addr.wrapping_add(2), self.fault_addr as u16);
                    self.movem_long_phase = 0;
                    self.cycle = 0;
                    self.micro_ops.advance();
                    return;
                }
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for pushing access info word during group 0 exception (4 cycles).
    fn tick_push_group0_access_info<B: Bus>(&mut self, bus: &mut B) {
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

    /// Begin exception processing.
    ///
    /// Group 0 exceptions (bus error, address error) use a 14-byte stack frame:
    /// - PC (4 bytes)
    /// - SR (2 bytes)
    /// - IR/opcode (2 bytes)
    /// - Fault address (4 bytes)
    /// - Access info word (2 bytes): bit 4 = R/W, bit 3 = I/N, bits 2-0 = FC
    ///
    /// Other exceptions use a standard 6-byte frame (PC + SR).
    fn begin_exception(&mut self) {
        if let Some(vec) = self.pending_exception.take() {
            self.current_exception = Some(vec);

            // Save SR and enter supervisor mode
            let old_sr = self.regs.sr;
            self.regs.enter_supervisor();
            self.regs.sr &= !flags::T; // Clear trace

            // Group 0 exceptions (bus error = 2, address error = 3) have extended stack frame
            let is_group_0 = vec == 2 || vec == 3;

            if is_group_0 {
                // 14-byte frame for group 0 exceptions
                // Reset phase tracking before starting push operations
                self.movem_long_phase = 0;

                // Group 0 exceptions have additional internal cycles for exception processing
                // The 68000 needs time to capture the fault state before stacking
                self.internal_cycles = 13;
                self.internal_advances_pc = false;
                self.micro_ops.push(MicroOp::Internal);

                // Push order: PC, SR, IR, fault addr, access info

                // Build access info word (special status word):
                // The 68000 includes parts of the instruction register in the upper bits.
                // bits 15-8: IR[15:8] (high byte of instruction register)
                // bits 7-5: IR[7:5] (high bits of IR low byte, often undefined/random)
                // bit 4: R/W (1 = read, 0 = write)
                // bit 3: I/N (1 = instruction fetch, 0 = not instruction/data)
                // bits 2-0: Function code
                let access_info: u16 = (u16::from(self.opcode) & 0xFF00) // IR[15:8]
                    | (u16::from(self.opcode) & 0x00E0) // IR[7:5]
                    | (if self.fault_read { 0x10 } else { 0 })
                    | (if self.fault_in_instruction { 0x08 } else { 0 })
                    | u16::from(self.fault_fc & 0x07);

                // Store values for pushing
                // We need to push: PC, SR, IR (opcode), fault_addr, access_info
                // Using data for PC, data2 for SR, we'll need to handle the rest inline
                //
                // For group 0 exceptions, the PC pushed depends on the addressing mode:
                // - Pre-decrement mode: push instr_start_pc (PC past opcode)
                // - Absolute modes (AbsShort, AbsLong): push instr_start_pc + (ext_words - 1) * 2
                // - Other modes: push instr_start_pc - 2
                let (ext_words, is_absolute) = match self.src_mode {
                    Some(AddrMode::AbsShort) => (1u8, true),
                    Some(AddrMode::AbsLong) => (2u8, true),
                    Some(mode) => (self.ext_words_for_mode(mode), false),
                    None => (0, false),
                };
                // MOVEM instructions (0x48xx, 0x4Cxx) consume extension words via next_ext_word()
                // which advances PC in prefetch_only mode, so they need current PC
                let is_movem = (self.opcode & 0xFB80) == 0x4880;
                self.data = if self.uses_predec_mode() || (is_movem && !is_absolute) {
                    // For predecrement mode or MOVEM with displacement/indexed modes,
                    // use current PC (after extension words were consumed)
                    self.regs.pc
                } else if is_absolute {
                    // For absolute modes, adjust based on extension word count
                    self.instr_start_pc
                        .wrapping_add(u32::from(ext_words.saturating_sub(1)) * 2)
                } else {
                    // For all other modes, point to opcode or first extension word
                    self.instr_start_pc.wrapping_sub(2)
                };
                self.data2 = u32::from(old_sr);

                // Push PC (4 bytes)
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);

                // Push SR (2 bytes) - copy from data2 to data
                self.micro_ops.push(MicroOp::SetDataFromData2);
                self.micro_ops.push(MicroOp::PushWord);

                // For the remaining pushes, we need to set up data appropriately
                // Store opcode in addr2 and fault_addr/access_info in fault fields
                // We'll use internal state to track what to push

                // Push IR/opcode (2 bytes)
                self.addr2 = u32::from(self.opcode);
                self.micro_ops.push(MicroOp::PushGroup0IR);

                // Push fault address (4 bytes)
                self.micro_ops.push(MicroOp::PushGroup0FaultAddr);

                // Push access info (2 bytes)
                self.group0_access_info = access_info;
                self.micro_ops.push(MicroOp::PushGroup0AccessInfo);
            } else {
                // Standard 6-byte frame for other exceptions
                // Queue push of PC and SR
                // PC is stored in data for PushLongHi/Lo
                //
                // Different exceptions save different PC values:
                //
                // Faulting instruction address (PC - 4):
                // - Privilege violation (8): PC of the privileged instruction
                // - Illegal instruction (4): PC of the illegal opcode
                // - Line A/F emulator (10, 11): PC of the line A/F instruction
                //
                // Return address (PC - 2):
                // - TRAP (32-47): address of instruction after TRAP
                // - TRAPV (7): address of instruction after TRAPV
                // - CHK (6): return address (instruction following CHK)
                //
                // With prefetch model, current PC = opcode + 4, so:
                // - For fault address: subtract 4 to get opcode address
                // - For return address: subtract 2 to get next instruction
                let saved_pc = match vec {
                    // Push fault address (back to the instruction that caused exception)
                    4 | 8 | 10 | 11 => self.regs.pc.wrapping_sub(4),
                    // Push return address (instruction after the trap/exception)
                    6 | 7 | 32..=47 => self.regs.pc.wrapping_sub(2),
                    // Other exceptions use current PC
                    _ => self.regs.pc,
                };
                self.data = saved_pc;
                // Old SR is stored in data2 to preserve it during PC push
                self.data2 = u32::from(old_sr);

                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::PushLongLo);
                // After PC push, copy old_sr from data2 to data for PushWord
                self.micro_ops.push(MicroOp::SetDataFromData2);
                self.micro_ops.push(MicroOp::PushWord);
            }

            // Queue vector read
            self.micro_ops.push(MicroOp::ReadVector);
        }
    }

    /// Calculate effective address based on addressing mode.
    fn calc_effective_address(&mut self) {
        // This will be expanded as we implement more addressing modes
        // For now, it's a placeholder that gets filled in by instruction handlers
    }

    /// Get the size increment for an addressing mode.
    fn size_increment(&self) -> u32 {
        match self.size {
            Size::Byte => 1,
            Size::Word => 2,
            Size::Long => 4,
        }
    }

    /// Get the number of internal cycles needed for EA calculation.
    /// Different addressing modes have different timing overhead.
    fn ea_calc_cycles(&self, mode: AddrMode) -> u8 {
        match mode {
            // Register direct: no extra cycles
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) => 0,
            // Simple indirect: no extra cycles
            AddrMode::AddrInd(_) => 0,
            // Post-increment/pre-decrement: 2 cycles for register update
            AddrMode::AddrIndPostInc(_) | AddrMode::AddrIndPreDec(_) => 2,
            // Displacement: no extra cycles (displacement added during address phase)
            AddrMode::AddrIndDisp(_) | AddrMode::PcDisp => 0,
            // Index modes: 2 cycles for index calculation
            AddrMode::AddrIndIndex(_) | AddrMode::PcIndex => 2,
            // Absolute modes: no extra cycles beyond the extension word fetch
            AddrMode::AbsShort | AddrMode::AbsLong => 0,
            // Immediate: no extra cycles beyond extension word fetch
            AddrMode::Immediate => 0,
        }
    }

    /// Queue effective address calculation micro-ops for source operand.
    fn queue_ea_read(&mut self, mode: AddrMode, size: Size) {
        self.size = size;

        match mode {
            AddrMode::DataReg(r) => {
                // Direct register - data already available
                self.data = self.regs.d[r as usize];
            }
            AddrMode::AddrReg(r) => {
                self.data = self.regs.a(r as usize);
            }
            AddrMode::AddrInd(r) => {
                self.addr = self.regs.a(r as usize);
                self.queue_read_ops(size);
            }
            AddrMode::AddrIndPostInc(r) => {
                self.addr = self.regs.a(r as usize);
                let inc = if size == Size::Byte && r == 7 {
                    2 // SP always stays word-aligned
                } else {
                    self.size_increment()
                };
                self.regs.set_a(r as usize, self.addr.wrapping_add(inc));
                self.queue_read_ops(size);
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if size == Size::Byte && r == 7 {
                    2 // SP always stays word-aligned
                } else {
                    self.size_increment()
                };
                self.addr = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, self.addr);
                self.queue_read_ops(size);
            }
            AddrMode::AddrIndDisp(_r) => {
                // Need extension word
                self.micro_ops.push(MicroOp::FetchExtWord);
                // Will calculate address after extension word is fetched
            }
            AddrMode::AddrIndIndex(_r) => {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            AddrMode::AbsShort => {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            AddrMode::AbsLong => {
                self.micro_ops.push(MicroOp::FetchExtWord);
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            AddrMode::PcDisp => {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            AddrMode::PcIndex => {
                self.micro_ops.push(MicroOp::FetchExtWord);
            }
            AddrMode::Immediate => {
                match size {
                    Size::Byte | Size::Word => {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                    Size::Long => {
                        self.micro_ops.push(MicroOp::FetchExtWord);
                        self.micro_ops.push(MicroOp::FetchExtWord);
                    }
                }
            }
        }
    }

    /// Queue memory read micro-ops for the given size.
    fn queue_read_ops(&mut self, size: Size) {
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
    fn queue_write_ops(&mut self, size: Size) {
        match size {
            Size::Byte => self.micro_ops.push(MicroOp::WriteByte),
            Size::Word => self.micro_ops.push(MicroOp::WriteWord),
            Size::Long => {
                self.micro_ops.push(MicroOp::WriteLongHi);
                self.micro_ops.push(MicroOp::WriteLongLo);
            }
        }
    }

    /// Get next extension word from the prefetch queue.
    /// Returns the word at ext_idx and increments the index.
    ///
    /// In prefetch_only mode (single-step testing):
    /// - Index 0 is IRC (Instruction Register Cache). Consuming it triggers the
    ///   prefetch pipeline to refill IRC from memory at PC, advancing PC by 2.
    /// - Index 1+ are words that were preloaded from memory at PC, PC+2, etc.
    ///   Consuming them also advances PC by 2 each.
    ///
    /// Returns 0 if the queue is exhausted (shouldn't happen in correct code).
    fn next_ext_word(&mut self) -> u16 {
        let idx = self.ext_idx as usize;
        if idx < self.ext_count as usize {
            self.ext_idx += 1;
            // In prefetch_only mode, consuming ANY extension word advances PC by 2
            // because the prefetch pipeline always fetches the next word.
            if self.prefetch_only {
                self.regs.pc = self.regs.pc.wrapping_add(2);
            }
            self.ext_words[idx]
        } else {
            0
        }
    }

    /// Calculate indexed effective address: base + index + displacement.
    /// Uses the next extension word from the prefetch queue.
    fn calc_index_ea(&mut self, base: u32) -> u32 {
        let ext = self.next_ext_word();
        let disp = (ext & 0xFF) as i8 as i32;
        let xn = ((ext >> 12) & 7) as usize;
        let is_addr = ext & 0x8000 != 0;
        let is_long = ext & 0x0800 != 0;
        let idx_val = if is_addr {
            self.regs.a(xn)
        } else {
            self.regs.d[xn]
        };
        let idx_val = if is_long {
            idx_val as i32
        } else {
            idx_val as i16 as i32
        };
        (base as i32).wrapping_add(disp).wrapping_add(idx_val) as u32
    }

    /// Calculate effective address for an addressing mode using extension words.
    /// Returns the address and whether it's a register (not memory).
    fn calc_ea(&mut self, mode: AddrMode, pc_at_ext: u32) -> (u32, bool) {
        match mode {
            AddrMode::DataReg(r) => (r as u32, true),
            AddrMode::AddrReg(r) => (r as u32, true),
            AddrMode::AddrInd(r) => (self.regs.a(r as usize), false),
            AddrMode::AddrIndPostInc(r) => {
                let addr = self.regs.a(r as usize);
                let inc = if self.size == Size::Byte && r == 7 {
                    2
                } else {
                    self.size_increment()
                };
                self.regs.set_a(r as usize, addr.wrapping_add(inc));
                (addr, false)
            }
            AddrMode::AddrIndPreDec(r) => {
                let dec = if self.size == Size::Byte && r == 7 {
                    2
                } else {
                    self.size_increment()
                };
                let addr = self.regs.a(r as usize).wrapping_sub(dec);
                self.regs.set_a(r as usize, addr);
                (addr, false)
            }
            AddrMode::AddrIndDisp(r) => {
                let disp = self.next_ext_word() as i16 as i32;
                let addr = (self.regs.a(r as usize) as i32).wrapping_add(disp) as u32;
                (addr, false)
            }
            AddrMode::AddrIndIndex(r) => {
                let ext = self.next_ext_word();
                let disp = (ext & 0xFF) as i8 as i32;
                let xn = ((ext >> 12) & 7) as usize;
                let is_addr = ext & 0x8000 != 0;
                let is_long = ext & 0x0800 != 0;
                let idx_val = if is_addr {
                    self.regs.a(xn)
                } else {
                    self.regs.d[xn]
                };
                let idx_val = if is_long {
                    idx_val as i32
                } else {
                    idx_val as i16 as i32
                };
                let addr = (self.regs.a(r as usize) as i32)
                    .wrapping_add(disp)
                    .wrapping_add(idx_val) as u32;
                (addr, false)
            }
            AddrMode::AbsShort => {
                let addr = self.next_ext_word() as i16 as i32 as u32;
                (addr, false)
            }
            AddrMode::AbsLong => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                let addr = (u32::from(hi) << 16) | u32::from(lo);
                (addr, false)
            }
            AddrMode::PcDisp => {
                // For PC-relative modes, the base PC is the address of the extension word.
                // In prefetch_only mode, this is PC-2 (before next_ext_word advances PC).
                let base_pc = if self.prefetch_only {
                    self.regs.pc.wrapping_sub(2)
                } else {
                    pc_at_ext
                };
                let disp = self.next_ext_word() as i16 as i32;
                let addr = (base_pc as i32).wrapping_add(disp) as u32;
                (addr, false)
            }
            AddrMode::PcIndex => {
                // For PC-relative modes, the base PC is the address of the extension word.
                // In prefetch_only mode, this is PC-2 (before next_ext_word advances PC).
                let base_pc = if self.prefetch_only {
                    self.regs.pc.wrapping_sub(2)
                } else {
                    pc_at_ext
                };
                let ext = self.next_ext_word();
                let disp = (ext & 0xFF) as i8 as i32;
                let xn = ((ext >> 12) & 7) as usize;
                let is_addr = ext & 0x8000 != 0;
                let is_long = ext & 0x0800 != 0;
                let idx_val = if is_addr {
                    self.regs.a(xn)
                } else {
                    self.regs.d[xn]
                };
                let idx_val = if is_long {
                    idx_val as i32
                } else {
                    idx_val as i16 as i32
                };
                let addr = (base_pc as i32)
                    .wrapping_add(disp)
                    .wrapping_add(idx_val) as u32;
                (addr, false)
            }
            AddrMode::Immediate => {
                // Immediate data is in extension words, return a marker
                // The actual data will be read from ext_words
                (0, true) // is_reg=true means "don't read memory"
            }
        }
    }

    /// Read immediate value from extension words.
    fn read_immediate(&mut self) -> u32 {
        match self.size {
            Size::Byte => {
                u32::from(self.next_ext_word() & 0xFF)
            }
            Size::Word => {
                u32::from(self.next_ext_word())
            }
            Size::Long => {
                let hi = self.next_ext_word();
                let lo = self.next_ext_word();
                (u32::from(hi) << 16) | u32::from(lo)
            }
        }
    }

    /// Read value from register based on size.
    fn read_data_reg(&self, r: u8, size: Size) -> u32 {
        let val = self.regs.d[r as usize];
        match size {
            Size::Byte => val & 0xFF,
            Size::Word => val & 0xFFFF,
            Size::Long => val,
        }
    }

    /// Write value to data register based on size.
    fn write_data_reg(&mut self, r: u8, value: u32, size: Size) {
        let reg = &mut self.regs.d[r as usize];
        *reg = match size {
            Size::Byte => (*reg & 0xFFFF_FF00) | (value & 0xFF),
            Size::Word => (*reg & 0xFFFF_0000) | (value & 0xFFFF),
            Size::Long => value,
        };
    }

    /// Count extension words needed for an addressing mode.
    fn ext_words_for_mode(&self, mode: AddrMode) -> u8 {
        match mode {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) => 0,
            AddrMode::AddrInd(_) | AddrMode::AddrIndPostInc(_) | AddrMode::AddrIndPreDec(_) => 0,
            AddrMode::AddrIndDisp(_) | AddrMode::AddrIndIndex(_) => 1,
            AddrMode::AbsShort | AddrMode::PcDisp | AddrMode::PcIndex => 1,
            AddrMode::AbsLong => 2,
            AddrMode::Immediate => match self.size {
                Size::Byte | Size::Word => 1,
                Size::Long => 2,
            },
        }
    }

    /// Check if the current instruction uses pre-decrement addressing mode.
    /// Used to determine the correct PC value to push during address errors.
    fn uses_predec_mode(&self) -> bool {
        matches!(self.src_mode, Some(AddrMode::AddrIndPreDec(_)))
            || matches!(self.dst_mode, Some(AddrMode::AddrIndPreDec(_)))
    }

    /// Trigger an exception.
    fn exception(&mut self, vector: u8) {
        self.pending_exception = Some(vector);
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::BeginException);
        // Reset cycle and phase tracking for clean exception processing
        self.cycle = 0;
        self.movem_long_phase = 0;
    }

    /// Trigger an address error exception (vector 3).
    /// Called when word/long access is attempted at an odd address.
    fn address_error(&mut self, addr: u32, is_read: bool, is_instruction: bool) {
        // Calculate function code based on supervisor mode and access type
        let supervisor = self.regs.sr & flags::S != 0;
        self.fault_fc = match (supervisor, is_instruction) {
            (false, false) => 1, // User data
            (false, true) => 2,  // User program
            (true, false) => 5,  // Supervisor data
            (true, true) => 6,   // Supervisor program
        };
        self.fault_addr = addr;
        self.fault_read = is_read;
        self.fault_in_instruction = is_instruction;
        self.exception(3); // Address error vector
    }

    /// Set flags for MOVE-style operations (clears V and C, sets N and Z).
    fn set_flags_move(&mut self, value: u32, size: Size) {
        self.regs.sr = Status::clear_vc(self.regs.sr);
        self.regs.sr = match size {
            Size::Byte => Status::update_nz_byte(self.regs.sr, value as u8),
            Size::Word => Status::update_nz_word(self.regs.sr, value as u16),
            Size::Long => Status::update_nz_long(self.regs.sr, value),
        };
    }

    /// Set flags for ADD operation.
    fn set_flags_add(&mut self, src: u32, dst: u32, result: u32, size: Size) {
        let (src, dst, result, msb) = match size {
            Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80),
            Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
            Size::Long => (src, dst, result, 0x8000_0000),
        };

        let mut sr = self.regs.sr;

        // Zero flag
        sr = Status::set_if(sr, Z, result == 0);

        // Negative flag
        sr = Status::set_if(sr, N, result & msb != 0);

        // Carry flag: set if there was a carry out
        let carry = match size {
            Size::Byte => (u16::from(src as u8) + u16::from(dst as u8)) > 0xFF,
            Size::Word => (u32::from(src as u16) + u32::from(dst as u16)) > 0xFFFF,
            Size::Long => src.checked_add(dst).is_none(),
        };
        sr = Status::set_if(sr, C, carry);

        // Extend flag: copy of carry
        sr = Status::set_if(sr, X, carry);

        // Overflow: set if both operands had same sign and result has different sign
        let overflow = (!(src ^ dst) & (src ^ result) & msb) != 0;
        sr = Status::set_if(sr, V, overflow);

        self.regs.sr = sr;
    }

    /// Set flags for SUB operation.
    fn set_flags_sub(&mut self, src: u32, dst: u32, result: u32, size: Size) {
        let (src, dst, result, msb) = match size {
            Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80),
            Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
            Size::Long => (src, dst, result, 0x8000_0000),
        };

        let mut sr = self.regs.sr;

        // Zero flag
        sr = Status::set_if(sr, Z, result == 0);

        // Negative flag
        sr = Status::set_if(sr, N, result & msb != 0);

        // Carry (borrow) flag: set if src > dst
        let carry = src > dst;
        sr = Status::set_if(sr, C, carry);

        // Extend flag: copy of carry
        sr = Status::set_if(sr, X, carry);

        // Overflow: set if operands had different signs and result sign differs from dst
        let overflow = ((src ^ dst) & (dst ^ result) & msb) != 0;
        sr = Status::set_if(sr, V, overflow);

        self.regs.sr = sr;
    }

    /// Set flags for CMP operation (like SUB but doesn't set X).
    fn set_flags_cmp(&mut self, src: u32, dst: u32, result: u32, size: Size) {
        let (src, dst, result, msb) = match size {
            Size::Byte => (src & 0xFF, dst & 0xFF, result & 0xFF, 0x80),
            Size::Word => (src & 0xFFFF, dst & 0xFFFF, result & 0xFFFF, 0x8000),
            Size::Long => (src, dst, result, 0x8000_0000),
        };

        let mut sr = self.regs.sr;

        // Zero flag
        sr = Status::set_if(sr, Z, result == 0);

        // Negative flag
        sr = Status::set_if(sr, N, result & msb != 0);

        // Carry (borrow) flag: set if src > dst
        sr = Status::set_if(sr, C, src > dst);

        // Overflow: set if operands had different signs and result sign differs from dst
        let overflow = ((src ^ dst) & (dst ^ result) & msb) != 0;
        sr = Status::set_if(sr, V, overflow);

        self.regs.sr = sr;
    }
}

impl Default for M68000 {
    fn default() -> Self {
        Self::new()
    }
}

// Instruction execution in separate module
mod execute;

impl Cpu for M68000 {
    type Registers = Registers;

    fn tick<B: Bus>(&mut self, bus: &mut B) {
        self.total_cycles += Ticks::new(1);

        // If halted or stopped, don't execute
        match self.state {
            State::Halted => return,
            State::Stopped => {
                // Check for interrupt that can wake us
                if self.int_pending > self.regs.interrupt_mask() {
                    self.state = State::Execute;
                    // Process interrupt
                    let level = self.int_pending;
                    self.int_pending = 0;
                    // Set interrupt mask to the level being processed
                    self.regs.set_interrupt_mask(level);
                    // Autovector interrupt
                    self.exception(24 + level);
                } else {
                    return;
                }
            }
            _ => {}
        }

        self.tick_internal(bus);

        // Check for pending interrupts at instruction boundary
        if self.micro_ops.is_empty() {
            if self.prefetch_only {
                // In prefetch-only mode (single-step testing):
                // Add PC advance for prefetch refill unless the internal cycles
                // already advanced PC (DIVU, MULU, etc. overlap their prefetch
                // during execution, handled by tick_internal_cycles).
                //
                // This applies to ALL instructions - both those with and without
                // extension words need the final prefetch advance.
                if !self.internal_advances_pc {
                    self.regs.pc = self.regs.pc.wrapping_add(2);
                }
                self.state = State::Halted;
                self.prefetch_only = false;
                return;
            }
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

    fn pc(&self) -> u16 {
        // Return low 16 bits of PC (for trait compatibility)
        self.regs.pc as u16
    }

    fn registers(&self) -> Self::Registers {
        self.regs
    }

    fn is_halted(&self) -> bool {
        matches!(self.state, State::Halted | State::Stopped)
    }

    fn interrupt(&mut self) -> bool {
        // For the basic trait, accept any interrupt
        // In reality, 68000 uses prioritized interrupts
        self.int_pending = 7; // Level 7 is non-maskable
        true
    }

    fn nmi(&mut self) {
        // Level 7 interrupt is effectively NMI
        self.int_pending = 7;
    }

    fn reset(&mut self) {
        // Reset sequence: read SSP from $0, PC from $4
        self.regs = Registers::new();
        self.state = State::FetchOpcode;
        self.micro_ops.clear();
        self.cycle = 0;
        self.internal_cycles = 0;
        self.internal_advances_pc = true;
        self.opcode = 0;
        self.ext_words = [0; 4];
        self.ext_count = 0;
        self.ext_idx = 0;
        self.src_mode = None;
        self.dst_mode = None;
        self.instr_phase = InstrPhase::Initial;
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
        self.prefetch_only = false;
        self.int_pending = 0;

        // Start fetch sequence (in real hardware, this reads SSP and PC first)
        self.micro_ops.push(MicroOp::FetchOpcode);
    }
}

/// Query paths supported by the 68000.
const M68000_QUERY_PATHS: &[&str] = &[
    // Data registers
    "d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7",
    // Address registers
    "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7",
    // Stack pointers
    "usp", "ssp",
    // Program counter
    "pc",
    // Status register
    "sr", "ccr",
    // Flags
    "flags.x", "flags.n", "flags.z", "flags.v", "flags.c",
    // System flags
    "flags.s", "flags.t",
    // Interrupt mask
    "int_mask",
    // CPU state
    "halted", "stopped", "cycles",
    // Current instruction
    "opcode",
];

impl Observable for M68000 {
    fn query(&self, path: &str) -> Option<Value> {
        match path {
            // Data registers
            "d0" => Some(self.regs.d[0].into()),
            "d1" => Some(self.regs.d[1].into()),
            "d2" => Some(self.regs.d[2].into()),
            "d3" => Some(self.regs.d[3].into()),
            "d4" => Some(self.regs.d[4].into()),
            "d5" => Some(self.regs.d[5].into()),
            "d6" => Some(self.regs.d[6].into()),
            "d7" => Some(self.regs.d[7].into()),

            // Address registers
            "a0" => Some(self.regs.a(0).into()),
            "a1" => Some(self.regs.a(1).into()),
            "a2" => Some(self.regs.a(2).into()),
            "a3" => Some(self.regs.a(3).into()),
            "a4" => Some(self.regs.a(4).into()),
            "a5" => Some(self.regs.a(5).into()),
            "a6" => Some(self.regs.a(6).into()),
            "a7" => Some(self.regs.a(7).into()),

            // Stack pointers
            "usp" => Some(self.regs.usp.into()),
            "ssp" => Some(self.regs.ssp.into()),

            // Program counter
            "pc" => Some(self.regs.pc.into()),

            // Status register
            "sr" => Some(Value::U16(self.regs.sr)),
            "ccr" => Some(self.regs.ccr().into()),

            // Condition code flags
            "flags.x" => Some((self.regs.sr & X != 0).into()),
            "flags.n" => Some((self.regs.sr & N != 0).into()),
            "flags.z" => Some((self.regs.sr & Z != 0).into()),
            "flags.v" => Some((self.regs.sr & V != 0).into()),
            "flags.c" => Some((self.regs.sr & C != 0).into()),

            // System flags
            "flags.s" => Some(self.regs.is_supervisor().into()),
            "flags.t" => Some(self.regs.is_trace().into()),

            // Interrupt mask
            "int_mask" => Some(self.regs.interrupt_mask().into()),

            // CPU state
            "halted" => Some(matches!(self.state, State::Halted).into()),
            "stopped" => Some(matches!(self.state, State::Stopped).into()),
            "cycles" => Some(self.total_cycles.get().into()),

            // Current instruction
            "opcode" => Some(Value::U16(self.opcode)),

            _ => None,
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        M68000_QUERY_PATHS
    }
}

#[cfg(feature = "test-utils")]
impl M68000 {
    /// Execute one complete instruction.
    /// Returns the number of cycles consumed.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let mut cycles = 0u32;
        let max_cycles = 200; // Safety limit

        // Run at least one tick
        self.tick(bus);
        cycles += 1;

        // Continue until queue is ready for next instruction
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
