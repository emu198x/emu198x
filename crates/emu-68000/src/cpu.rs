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
    /// Pending exception vector number.
    pending_exception: Option<u8>,
    /// Current exception being processed.
    current_exception: Option<u8>,

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
            pending_exception: None,
            current_exception: None,
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

    /// Read byte from memory.
    fn read_byte<B: Bus>(&mut self, bus: &mut B, addr: u32) -> u8 {
        // 68000 uses 24-bit addresses
        let addr24 = addr & 0x00FF_FFFF;
        // Byte reads use even address for high byte, odd for low byte
        // For simplicity, we read as if addresses are byte-aligned
        bus.read(addr24 as u16).data
    }

    /// Read word from memory (big-endian).
    fn read_word<B: Bus>(&mut self, bus: &mut B, addr: u32) -> u16 {
        let addr24 = addr & 0x00FF_FFFE; // Word-aligned
        let hi = bus.read(addr24 as u16).data;
        let lo = bus.read((addr24 + 1) as u16).data;
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
        bus.write(addr24 as u16, value);
    }

    /// Write word to memory (big-endian).
    fn write_word<B: Bus>(&mut self, bus: &mut B, addr: u32, value: u16) {
        let addr24 = addr & 0x00FF_FFFE;
        bus.write(addr24 as u16, (value >> 8) as u8);
        bus.write((addr24 + 1) as u16, value as u8);
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

    /// Queue internal cycles.
    fn queue_internal(&mut self, cycles: u8) {
        self.internal_cycles = cycles;
        self.micro_ops.push(MicroOp::Internal);
    }

    /// Execute one clock cycle of CPU operation.
    fn tick_internal<B: Bus>(&mut self, bus: &mut B) {
        let Some(op) = self.micro_ops.current() else {
            // Queue empty - start next instruction
            self.queue_fetch();
            return;
        };

        match op {
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
            MicroOp::CalcEA => {
                self.calc_effective_address();
                self.micro_ops.advance();
            }
            MicroOp::Execute => {
                self.decode_and_execute();
                self.micro_ops.advance();
            }
            MicroOp::Internal => self.tick_internal_cycles(),
            MicroOp::PushWord => self.tick_push_word(bus),
            MicroOp::PushLongHi => self.tick_push_long_hi(bus),
            MicroOp::PushLongLo => self.tick_push_long_lo(bus),
            MicroOp::PopWord => self.tick_pop_word(bus),
            MicroOp::PopLongHi => self.tick_pop_long_hi(bus),
            MicroOp::PopLongLo => self.tick_pop_long_lo(bus),
            MicroOp::BeginException => {
                self.begin_exception();
                self.micro_ops.advance();
            }
            MicroOp::ReadVector => self.tick_read_vector(bus),
            MicroOp::MovemWrite => self.tick_movem_write(bus),
            MicroOp::MovemRead => self.tick_movem_read(bus),
        }
    }

    /// Tick for opcode fetch (4 cycles).
    fn tick_fetch_opcode<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {
                // Bus cycles 1-3: Address setup and data read
            }
            3 => {
                // Cycle 4: Read complete
                self.opcode = self.read_word(bus, self.regs.pc);
                self.regs.pc = self.regs.pc.wrapping_add(2);
                self.cycle = 0;
                self.micro_ops.advance();
                // Queue decode and execute
                self.micro_ops.push(MicroOp::Execute);
                return;
            }
            _ => unreachable!(),
        }
        self.cycle += 1;
    }

    /// Tick for extension word fetch (4 cycles).
    fn tick_fetch_ext_word<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
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
            0 | 1 | 2 => {}
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
            0 | 1 | 2 => {}
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

    /// Tick for long write high word (4 cycles).
    fn tick_write_long_hi<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 | 1 | 2 => {}
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
    fn tick_internal_cycles(&mut self) {
        self.cycle += 1;
        if self.cycle >= self.internal_cycles {
            self.cycle = 0;
            self.micro_ops.advance();
        }
    }

    /// Tick for push word (4 cycles).
    fn tick_push_word<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            0 => {
                self.addr = self.regs.push_word();
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
        match self.cycle {
            0 | 1 | 2 => {}
            3 => {
                // Read vector address (vector * 4)
                if let Some(vec) = self.current_exception {
                    let vector_addr = u32::from(vec) * 4;
                    self.data = self.read_long(bus, vector_addr);
                    self.regs.pc = self.data;
                }
                self.current_exception = None;
                self.cycle = 0;
                self.micro_ops.advance();
                self.queue_fetch();
                return;
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
            0 | 1 | 2 => {}
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
            0 | 1 | 2 => {}
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
                    // Word read - sign extend to long for address registers
                    let word = self.read_word(bus, self.addr);
                    self.data = if bit_idx >= 8 {
                        word as i16 as i32 as u32
                    } else {
                        u32::from(word)
                    };
                    self.addr = self.addr.wrapping_add(2);
                }

                // Store value in register
                if bit_idx < 8 {
                    if self.size == Size::Long {
                        self.regs.d[bit_idx] = self.data;
                    } else {
                        self.regs.d[bit_idx] =
                            (self.regs.d[bit_idx] & 0xFFFF_0000) | (self.data & 0xFFFF);
                    }
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

    /// Begin exception processing.
    fn begin_exception(&mut self) {
        if let Some(vec) = self.pending_exception.take() {
            self.current_exception = Some(vec);

            // Save SR and enter supervisor mode
            let old_sr = self.regs.sr;
            self.regs.enter_supervisor();
            self.regs.sr &= !flags::T; // Clear trace

            // Queue push of PC and SR
            self.data = self.regs.pc;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);

            self.data = u32::from(old_sr);
            self.micro_ops.push(MicroOp::PushWord);

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

    /// Get next extension word and advance index.
    fn next_ext_word(&mut self) -> u16 {
        let idx = self.ext_idx as usize;
        if idx < self.ext_count as usize {
            self.ext_idx += 1;
            self.ext_words[idx]
        } else {
            0
        }
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
                let disp = self.next_ext_word() as i16 as i32;
                let addr = (pc_at_ext as i32).wrapping_add(disp) as u32;
                (addr, false)
            }
            AddrMode::PcIndex => {
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
                let addr = (pc_at_ext as i32)
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

    /// Trigger an exception.
    fn exception(&mut self, vector: u8) {
        self.pending_exception = Some(vector);
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::BeginException);
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
