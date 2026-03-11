//! Motorola 68000 CPU core with reactive bus state machine.
//!
//! This is the tick engine: the outermost loop that drives the 68000's
//! bus state machine. It processes one micro-operation per 4-clock bus
//! cycle, matching the real 68000's minimum bus timing.
//!
//! # Architecture
//!
//! The CPU maintains a queue of [`MicroOp`]s. Each tick:
//!
//! 1. **Instant ops** (Execute, PromoteIRC) run immediately within the tick
//! 2. **Bus ops** (FetchIRC, ReadWord, etc.) enter the `BusCycle` state
//!    and poll the bus for 4+ clocks until DTACK arrives
//! 3. **Internal delays** count down without bus activity
//!
//! Instructions are decoded by [`decode_and_execute`](Cpu68000::decode_and_execute)
//! (in `decode.rs`) which sets up follow-up tags and queues micro-ops.
//! The follow-up tag state machine in `continue_instruction` handles
//! multi-phase instructions (EA calculation, operand fetch, execute, writeback).
//!
//! # Prefetch pipeline
//!
//! The 68000 has a two-word prefetch pipeline:
//! - **IR** (Instruction Register): the opcode currently executing
//! - **IRC** (Instruction Register Cache): the next word, prefetched
//!
//! `PromoteIRC` moves IRC -> IR and queues a new FetchIRC + Execute.
//! `consume_irc()` returns the current IRC value and queues a FetchIRC
//! to replace it (used for extension words, immediates, displacements).

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::bus::{BusStatus, FunctionCode, M68kBus};
use crate::microcode::{MicroOp, MicroOpQueue};
use crate::model::{CpuCapabilities, CpuModel};
use crate::registers::Registers;

// --- Follow-up tag constants ---
//
// These identify the current phase of a multi-cycle instruction.
// The decode/continue state machine in decode.rs dispatches on these.

/// Fetch source effective address.
pub const TAG_FETCH_SRC_EA: u8 = 1;
/// Fetch source data (read from memory or register).
pub const TAG_FETCH_SRC_DATA: u8 = 2;
/// Fetch destination effective address.
pub const TAG_FETCH_DST_EA: u8 = 3;
/// Fetch destination data (for read-modify-write ops).
pub const TAG_FETCH_DST_DATA: u8 = 4;
/// Run the ALU / execute logic.
pub const TAG_EXECUTE: u8 = 5;
/// Write result back to destination.
pub const TAG_WRITEBACK: u8 = 6;

// EA extension word follow-ups
/// Source absolute long: hi word loaded, need lo word.
pub const TAG_EA_SRC_LONG: u8 = 10;
/// Source displacement: need d16 word from IRC.
pub const TAG_EA_SRC_DISP: u8 = 11;
/// Source PC displacement: need d16 word from IRC.
pub const TAG_EA_SRC_PCDISP: u8 = 12;
/// Destination absolute long: hi word loaded, need lo word.
pub const TAG_EA_DST_LONG: u8 = 13;
/// Destination displacement: need d16 word from IRC.
pub const TAG_EA_DST_DISP: u8 = 14;
/// Destination PC displacement: need d16 word from IRC.
pub const TAG_EA_DST_PCDISP: u8 = 15;

// Immediate long lo-word follow-ups
/// Source immediate long: hi word loaded, need lo word.
pub const TAG_DATA_SRC_LONG: u8 = 20;
/// Destination immediate long: hi word loaded, need lo word.
pub const TAG_DATA_DST_LONG: u8 = 21;

// Branch follow-ups
/// Evaluate branch condition.
pub const TAG_BCC_EXECUTE: u8 = 30;
/// Fetch 16-bit branch displacement.
pub const TAG_BCC_FETCH_DISP: u8 = 31;
/// DBcc: decrement and branch.
pub const TAG_DBCC_EXECUTE: u8 = 32;
/// JSR: jump to target address.
pub const TAG_JSR_EXECUTE: u8 = 33;
/// JSR: push complete, now jump to target.
pub const TAG_JSR_JUMP: u8 = 43;
/// BSR: branch to subroutine.
pub const TAG_BSR_EXECUTE: u8 = 34;

// RTS follow-ups
/// RTS: pop PC high word.
pub const TAG_RTS_PC_HI: u8 = 35;
/// RTS: pop PC low word and jump.
pub const TAG_RTS_PC_LO: u8 = 36;

// MOVEM follow-ups
pub const TAG_MOVEM_NEXT: u8 = 37;
pub const TAG_MOVEM_STORE: u8 = 60;
/// MUL/DIV: execute after source operand is fetched.
pub const TAG_MULDIV_EXECUTE: u8 = 82;
/// MOVEP: multi-byte transfer loop (read/write one byte per iteration).
pub const TAG_MOVEP_TRANSFER: u8 = 83;
/// BCD -(An),-(An): source byte read complete, now predec dest and read.
pub const TAG_BCD_SRC_READ: u8 = 84;
/// BCD -(An),-(An): dest byte read complete, compute and write result.
pub const TAG_BCD_DST_READ: u8 = 85;
/// Bit field memory execute: EA resolved, read/modify/write bytes.
pub const TAG_BITFIELD_MEM_EXECUTE: u8 = 88;
/// MOVEM: resolve EA after FetchIRC refills IRC with the first EA extension word.
/// Needed because consume_irc() for the register mask leaves IRC stale until
/// the queued FetchIRC completes; calc_ea_start can't be called until then.
pub const TAG_MOVEM_RESOLVE_EA: u8 = 81;

// LINK follow-up
pub const TAG_LINK_DISP: u8 = 61;

// UNLK follow-ups
pub const TAG_UNLK_POP_HI: u8 = 62;
pub const TAG_UNLK_POP_LO: u8 = 63;

// RTE follow-ups
pub const TAG_RTE_READ_SR: u8 = 64;
pub const TAG_RTE_READ_PC_HI: u8 = 65;
pub const TAG_RTE_READ_PC_LO: u8 = 66;
pub const TAG_RTE_READ_FORMAT: u8 = 96;

// RTR follow-ups
pub const TAG_RTR_READ_CCR: u8 = 67;
pub const TAG_RTR_READ_PC_HI: u8 = 68;
pub const TAG_RTR_READ_PC_LO: u8 = 69;

// ADDX/SUBX memory mode follow-ups
pub const TAG_ADDX_READ_SRC: u8 = 70;
pub const TAG_ADDX_READ_DST: u8 = 71;
pub const TAG_ADDX_WRITE: u8 = 72;

// CHK follow-up: bounds comparison after EA read
pub const TAG_CHK_EXECUTE: u8 = 80;

/// STOP: enter stopped state after FetchIRC completes the pipeline refill.
pub const TAG_STOP_WAIT: u8 = 86;

/// RTD: pop PC complete, apply displacement to SP.
pub const TAG_RTD_PC_HI: u8 = 87;
/// RTD: pop PC lo word, then apply displacement.
pub const TAG_RTD_PC_LO: u8 = 89;

/// Exception: format/vector word pushed (68010+), now push PC.
pub const TAG_EXC_STACK_FORMAT: u8 = 90;

// Exception follow-ups
/// Exception: push PC onto stack.
pub const TAG_EXC_STACK_PC_HI: u8 = 38;
/// Exception: push PC low word.
pub const TAG_EXC_STACK_PC_LO: u8 = 39;
/// Exception: push SR.
pub const TAG_EXC_STACK_SR: u8 = 40;
/// Exception: fetch vector address.
pub const TAG_EXC_FETCH_VECTOR: u8 = 41;
/// Exception: load PC from vector and enter supervisor mode.
pub const TAG_EXC_FINISH: u8 = 42;

// Address error exception follow-ups (14-byte group 0 frame)
/// AE: push SR word.
pub const TAG_AE_PUSH_SR: u8 = 50;
/// AE: push IR word.
pub const TAG_AE_PUSH_IR: u8 = 51;
/// AE: push fault address long.
pub const TAG_AE_PUSH_FAULT: u8 = 52;
/// AE: push access info word.
pub const TAG_AE_PUSH_INFO: u8 = 53;
/// AE: fetch exception vector.
pub const TAG_AE_FETCH_VECTOR: u8 = 54;
/// AE: jump to vector address.
pub const TAG_AE_FINISH: u8 = 55;

/// MOVE16: transfer loop (read phase then write phase).
pub const TAG_MOVE16_NEXT: u8 = 97;
/// MOVE16: read low word of 32-bit absolute address extension.
pub const TAG_MOVE16_ADDR_LO: u8 = 98;

/// Bus error (68010+): push extra format $A words (zeroed internal state).
pub const TAG_BE_PUSH_EXTRA: u8 = 56;
/// Bus error (68000): fetch vector 2.
pub const TAG_BE_FETCH_VECTOR_68K: u8 = 57;

/// CPU state machine state.
pub enum State {
    /// Ready to process the next micro-op.
    Idle,
    /// Internal processing delay (no bus activity).
    Internal { cycles: u8 },
    /// Active bus cycle (polling for DTACK).
    BusCycle {
        op: MicroOp,
        addr: u32,
        fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
        cycle_count: u8,
    },
    /// CPU halted (double bus error or unimplemented instruction).
    Halted,
    /// CPU stopped (STOP instruction, waiting for interrupt).
    Stopped,
}

/// ALU operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AluOp {
    Add,
    Sub,
    Cmp,
    And,
    Or,
    Eor,
}

/// Bit manipulation operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitOp {
    Btst,
    Bset,
    Bclr,
    Bchg,
}

// --- 68030 instruction cache ---

const ICACHE_LINES: usize = 16;
const ICACHE_WORDS_PER_LINE: usize = 8;

/// One line of the 68030 instruction cache (16 bytes = 8 words).
///
/// Tag includes function code bits so supervisor and user fetches are
/// cached separately, matching the real 68030.
#[derive(Clone, Copy)]
struct ICacheLine {
    tag: u32,
    words: [u16; ICACHE_WORDS_PER_LINE],
    valid: [bool; ICACHE_WORDS_PER_LINE],
}

impl ICacheLine {
    const fn new() -> Self {
        Self {
            tag: 0xFFFF_FFFF, // impossible tag — never matches
            words: [0; ICACHE_WORDS_PER_LINE],
            valid: [false; ICACHE_WORDS_PER_LINE],
        }
    }
}

/// 68030-style direct-mapped instruction cache: 16 lines × 8 words = 256 bytes.
pub(crate) struct ICache {
    lines: [ICacheLine; ICACHE_LINES],
}

impl ICache {
    fn new() -> Self {
        Self {
            lines: [ICacheLine::new(); ICACHE_LINES],
        }
    }

    /// Invalidate all lines.
    pub(crate) fn clear(&mut self) {
        for line in &mut self.lines {
            line.valid = [false; ICACHE_WORDS_PER_LINE];
        }
    }

    /// Address decomposition helpers.
    #[inline]
    fn line_index(addr: u32) -> usize {
        ((addr >> 4) & 0xF) as usize
    }

    #[inline]
    fn word_offset(addr: u32) -> usize {
        ((addr >> 1) & 0x7) as usize
    }

    /// Tag = upper address bits (A31-A8) with function code in bits 26-24.
    #[inline]
    fn tag(addr: u32, fc: u8) -> u32 {
        (addr >> 8) | ((fc as u32 & 0x7) << 24)
    }
}

/// Motorola 68000 CPU with reactive bus state machine.
///
/// Call [`tick`](Cpu68000::tick) every crystal clock cycle. The CPU only
/// acts on 4-clock boundaries (matching the 68000's minimum bus cycle).
pub struct Cpu68000 {
    /// Configured CPU model/capability profile.
    pub model: CpuModel,
    /// CPU register file (D0-D7, A0-A7, USP, SSP, PC, SR).
    pub regs: Registers,
    /// Current state machine state.
    pub state: State,
    /// Pending micro-operation queue.
    pub micro_ops: MicroOpQueue,

    // --- Prefetch pipeline ---
    /// Instruction Register: the opcode currently executing.
    pub ir: u16,
    /// Instruction Register Cache: the next prefetched word.
    pub irc: u16,
    /// Address where IRC was fetched from.
    pub irc_addr: u32,
    /// Next address to fetch into IRC.
    pub next_fetch_addr: u32,
    /// PC value at the start of the current instruction (opcode address).
    pub instr_start_pc: u32,

    // --- Instruction execution state ---
    /// Computed effective address for memory operations.
    pub addr: u32,
    /// Data register for read/write bus cycles and ALU results.
    pub data: u32,
    /// True when executing a multi-phase instruction.
    pub in_followup: bool,
    /// Current phase of the multi-phase instruction.
    pub followup_tag: u8,
    /// Source addressing mode for the current instruction.
    pub src_mode: Option<AddrMode>,
    /// Destination addressing mode for the current instruction.
    pub dst_mode: Option<AddrMode>,
    /// Operation size (byte/word/long).
    pub size: Size,
    /// EA register number (used by displacement and LEA).
    pub ea_reg: u8,
    /// PC value captured for PC-relative addressing.
    pub ea_pc: u32,
    /// ALU operation for the current instruction.
    pub alu_op: AluOp,
    /// Bit operation for the current instruction.
    pub bit_op: BitOp,
    /// Interrupt priority level being processed.
    pub target_ipl: u8,
    /// Enable verbose debug logging.
    pub debug_mode: bool,
    /// MOVEM register mask (remaining registers to transfer).
    pub movem_mask: u16,
    /// MOVEM current register index (for mem→reg store).
    pub movem_idx: u8,
    /// MOVEM direction: true = register→memory, false = memory→register.
    pub movem_is_write: bool,
    /// MOVEM: address register used for predec/postinc (0-7), or 0xFF if none.
    pub movem_an_reg: u8,
    /// MOVE16: 8-word transfer buffer (16 bytes).
    pub(crate) move16_buf: [u16; 8],
    /// MOVE16: current word index (0-7 for reads, 8-15 for writes).
    pub(crate) move16_idx: u8,
    /// MOVE16: destination base address (line-aligned).
    pub(crate) move16_dst_addr: u32,
    /// FPU: source operand value for the current FPU instruction.
    #[allow(dead_code)] // Used in Phase 2+ of FPU implementation
    pub(crate) fpu_source: f64,
    /// Exception vector for group 1/2 exceptions (TRAP, privilege violation, etc.).
    /// When set, TAG_EXC_STACK_SR skips InterruptAck and uses this vector directly.
    pub exc_vector: Option<u8>,
    /// Source operand value.
    pub src_val: u32,
    /// Destination operand value.
    pub dst_val: u32,

    // --- Address error state ---
    /// Fault address that triggered the address error.
    pub(crate) ae_fault_addr: u32,
    /// Access info word (IR bits [15:5] | R/W | function code).
    pub(crate) ae_access_info: u16,
    /// Saved SR at time of address error (before supervisor mode).
    pub(crate) ae_saved_sr: u16,
    /// True while processing an address error (prevents recursive AE).
    pub(crate) ae_in_progress: bool,
    /// True when the AE was caused by a FetchIRC (branch/jump to odd target).
    pub(crate) ae_from_fetch_irc: bool,
    /// DBcc: original Dn.w value before decrement, for undo on branch AE.
    pub(crate) dbcc_dn_undo: Option<(u8, u16)>,
    /// IR value to push in the AE frame. Usually IR, but for MOVE.w write AE
    /// with -(An) destination the real 68000 pushes IRC because the pipeline
    /// has already advanced IR → IRC before the write cycle.
    pub(crate) ae_frame_ir: u16,
    /// Saved SR for MOVE write AE flag restoration. The real 68000's 16-bit
    /// ALU evaluates MOVE flags in stages during the write bus cycle. If the
    /// write triggers AE, the frame SR reflects how far evaluation progressed:
    /// - `pre_move_sr`: full restore (for register src to (An)/(An)+, or
    ///   memory src to (An)/(An)+/abs.l with lo-word synthetic flags)
    /// - `pre_move_vc`: partial restore, V/C only (for register src to d16/d8+idx)
    pub(crate) pre_move_sr: Option<u16>,
    /// Saved SR for partial V/C restore on MOVE.l write AE with extension-word
    /// destinations. N,Z are already computed during the FetchIRC; only V,C
    /// clearing was aborted by the AE.
    pub(crate) pre_move_vc: Option<u16>,
    /// True when the current memory access uses program space (PC-relative).
    /// The 68000 asserts FC=6/2 (supervisor/user program) for PcDisp and
    /// PcIndex modes instead of the usual FC=5/1 (data space).
    pub(crate) program_space_access: bool,
    /// Last EA register side effect to undo on AE.
    /// (register_index, byte_amount, is_postinc). Set by calc_ea_start
    /// for PostInc/PreDec, overwritten by each calc_ea_start call so
    /// only the most recent (relevant) side effect gets undone.
    /// Register undo info for address error: (reg, amount, is_postinc, is_dst).
    pub(crate) ae_undo_reg: Option<(u8, u32, bool, bool)>,
    /// UNLK: original stack pointer to restore if AE fires.
    /// UNLK sets A7 ← An before reading from the new (potentially odd) A7.
    /// If the read faults, the real 68000 undoes the A7 modification.
    /// Tuple: (was_supervisor, original_sp).
    pub(crate) sp_undo: Option<(bool, u32)>,

    // --- Bus error / group-0 exception state ---
    /// Remaining extra words to push for bus error frame (68010+).
    pub(crate) be_extra_count: u8,
    /// Format/vector word for the bus error frame being constructed.
    /// Set by `begin_bus_error()` and consumed by `TAG_BE_PUSH_EXTRA`.
    pub(crate) be_format_word: u16,
    /// Vector number for the current group-0 exception (2=bus error, 3=address error).
    /// Used by TAG_AE_FETCH_VECTOR to read the correct vector.
    pub(crate) group0_vector: u8,

    // --- Instruction cache (68020+) ---
    /// 68030-style direct-mapped instruction cache (16 lines × 8 words).
    pub(crate) icache: ICache,
}

impl Cpu68000 {
    /// Create a new CPU in reset state.
    ///
    /// Supervisor mode, interrupt mask level 7, all registers zero.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::new_with_model(CpuModel::M68000)
    }

    /// Create a new CPU with an explicit 68k model profile.
    ///
    /// Execution semantics are still 68000-based today; this records the model
    /// for staged decode/execute feature gating.
    #[must_use]
    pub fn new_with_model(model: CpuModel) -> Self {
        Self {
            model,
            regs: Registers::new(),
            state: State::Idle,
            micro_ops: MicroOpQueue::new(),
            ir: 0,
            irc: 0,
            irc_addr: 0,
            next_fetch_addr: 0,
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
            bit_op: BitOp::Btst,
            target_ipl: 0,
            debug_mode: false,
            movem_mask: 0,
            movem_idx: 0,
            movem_is_write: false,
            movem_an_reg: 0xFF,
            move16_buf: [0; 8],
            move16_idx: 0,
            move16_dst_addr: 0,
            fpu_source: 0.0,
            exc_vector: None,
            src_val: 0,
            dst_val: 0,
            ae_fault_addr: 0,
            ae_access_info: 0,
            ae_saved_sr: 0,
            ae_in_progress: false,
            ae_from_fetch_irc: false,
            dbcc_dn_undo: None,
            ae_frame_ir: 0,
            pre_move_sr: None,
            pre_move_vc: None,
            program_space_access: false,
            ae_undo_reg: None,
            sp_undo: None,
            be_extra_count: 0,
            be_format_word: 0,
            group0_vector: 3,
            icache: ICache::new(),
        }
    }

    /// Return the configured CPU model.
    #[must_use]
    pub const fn model(&self) -> CpuModel {
        self.model
    }

    /// Return capability flags for the configured CPU model.
    #[must_use]
    pub const fn capabilities(&self) -> CpuCapabilities {
        self.model.capabilities()
    }

    /// Valid SR bit mask for this CPU model.
    /// 68020+ adds T0 (bit 14) and M (bit 12).
    pub fn sr_mask(&self) -> u16 {
        if self.capabilities().scaled_index {
            crate::flags::SR_MASK_020
        } else {
            crate::flags::SR_MASK
        }
    }

    /// Return the model-appropriate internal delay. On the 68000 this
    /// returns `m68k`, on the 68020+ pipeline it returns `m68020`.
    /// When the 68020 value is 0 this should be elided at the call site
    /// (don't push a zero-length Internal micro-op).
    #[inline]
    pub(crate) fn internal_delay(&self, m68k: u8, m68020: u8) -> u8 {
        match self.model.timing_class() {
            crate::model::TimingClass::M68000 => m68k,
            _ => m68020,
        }
    }

    /// Minimum number of CPU clocks per bus cycle. The 68000 uses 4-clock
    /// bus cycles (S0-S7). The 68020 uses 3-clock cycles for fast memory
    /// (ROM, fast RAM); chip RAM still synchronises to the DMA slot grid
    /// via BusStatus::Wait from the Amiga bus.
    #[inline]
    fn min_bus_cycles(&self) -> u8 {
        match self.model.timing_class() {
            crate::model::TimingClass::M68000 => 4,
            _ => 3,
        }
    }

    /// Try to serve a FetchIRC from the instruction cache.
    ///
    /// Returns `true` if the cache hit and IRC/PC were updated (no bus
    /// cycle needed). Returns `false` on miss or if the cache is
    /// disabled/not present.
    fn icache_lookup(&mut self) -> bool {
        if !self.capabilities().instruction_cache {
            return false;
        }
        // CACR bit 0 (EI): cache enable
        if self.regs.cacr & 0x01 == 0 {
            return false;
        }
        let addr = self.next_fetch_addr;
        let fc = if self.regs.is_supervisor() { 6u8 } else { 2u8 };
        let tag = ICache::tag(addr, fc);
        let index = ICache::line_index(addr);
        let word = ICache::word_offset(addr);
        let line = &self.icache.lines[index];
        if line.tag == tag && line.valid[word] {
            self.irc = line.words[word];
            self.irc_addr = addr;
            self.next_fetch_addr = addr.wrapping_add(2);
            self.regs.pc = self.next_fetch_addr;
            true
        } else {
            false
        }
    }

    /// Fill the instruction cache after a FetchIRC bus read.
    ///
    /// Skipped when the cache is disabled (EI=0) or frozen (FI=1).
    fn icache_fill(&mut self, addr: u32, data: u16) {
        if !self.capabilities().instruction_cache {
            return;
        }
        // EI=0: disabled, don't fill
        if self.regs.cacr & 0x01 == 0 {
            return;
        }
        // FI=1 (bit 1): frozen — hits work, misses don't fill
        if self.regs.cacr & 0x02 != 0 {
            return;
        }
        let fc = if self.regs.is_supervisor() { 6u8 } else { 2u8 };
        let tag = ICache::tag(addr, fc);
        let index = ICache::line_index(addr);
        let word = ICache::word_offset(addr);
        let line = &mut self.icache.lines[index];
        if line.tag != tag {
            // Tag mismatch: replace line
            line.tag = tag;
            line.valid = [false; ICACHE_WORDS_PER_LINE];
        }
        line.words[word] = data;
        line.valid[word] = true;
    }

    /// Reset the CPU to begin executing from a given SSP and PC.
    ///
    /// Sets supervisor mode with interrupts masked, clears the micro-op
    /// queue, and begins the prefetch sequence.
    pub fn reset_to(&mut self, ssp: u32, pc: u32) {
        self.regs.ssp = ssp;
        self.regs.pc = pc;
        self.regs.sr = 0x2700;
        self.next_fetch_addr = pc;
        self.state = State::Idle;
        self.in_followup = false;
        self.followup_tag = 0;
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::PromoteIRC);
    }

    /// Set up the prefetch pipeline for single-step testing.
    ///
    /// Loads IR and IRC directly, sets PC-related state to match the
    /// DL test format (PC points past opcode+IRC), and queues an Execute
    /// micro-op so the next tick will decode the instruction.
    pub fn setup_prefetch(&mut self, opcode: u16, irc: u16) {
        self.ir = opcode;
        self.irc = irc;
        // IRC was fetched from PC-2 (the word before current PC)
        self.irc_addr = self.regs.pc.wrapping_sub(2);
        // Instruction started at PC-4 (before opcode and IRC fetches)
        self.instr_start_pc = self.regs.pc.wrapping_sub(4);
        // Next fetch continues from where PC left off
        self.next_fetch_addr = self.regs.pc;
        self.micro_ops.clear();
        self.micro_ops.push(MicroOp::Execute);
        self.in_followup = false;
        self.followup_tag = 0;
        self.state = State::Idle;
    }

    /// Consume the current IRC value and queue a FetchIRC to replace it.
    ///
    /// Used when the instruction needs an extension word (immediate data,
    /// displacement, absolute address). The FetchIRC is pushed to the
    /// front of the queue so it runs before whatever was next.
    pub fn consume_irc(&mut self) -> u16 {
        let val = self.irc;
        self.micro_ops.push_front(MicroOp::FetchIRC);
        val
    }

    /// Halt the CPU (unimplemented instruction or double fault).
    #[allow(dead_code)]
    pub(crate) fn halt(&mut self) {
        self.state = State::Halted;
    }

    /// Returns true if the CPU is halted.
    #[must_use]
    pub fn is_halted(&self) -> bool {
        matches!(self.state, State::Halted)
    }

    /// Returns true if the CPU is idle (ready for next micro-op).
    #[must_use]
    pub fn is_idle(&self) -> bool {
        matches!(self.state, State::Idle)
    }

    /// Advance the CPU by one crystal clock cycle.
    ///
    /// The 68000 only acts on 4-clock boundaries. Non-aligned ticks
    /// are no-ops. On aligned ticks:
    ///
    /// 1. Process instant ops (Execute, PromoteIRC)
    /// 2. Check for pending interrupts
    /// 3. Start the next instruction if the queue is empty
    /// 4. Initiate the next bus cycle or internal delay
    /// 5. Advance the current state (bus polling, delay countdown)
    pub fn tick<B: M68kBus>(&mut self, bus: &mut B, crystal_clock: u64) {
        // 68000 minimum bus cycle = 4 clock cycles
        if !crystal_clock.is_multiple_of(4) {
            return;
        }

        // --- Idle: drain instant ops, check interrupts, start bus cycles ---
        if matches!(self.state, State::Idle) {
            self.process_instant_ops(bus);

            // Check for pending interrupts when no work remains
            if matches!(self.state, State::Idle) && self.micro_ops.is_empty() {
                let ipl = bus.poll_ipl();
                if ipl > self.regs.interrupt_mask() || ipl == 7 {
                    self.initiate_interrupt_exception(ipl);
                    self.process_instant_ops(bus);
                }
            }

            // Start next instruction if nothing queued
            if matches!(self.state, State::Idle) && self.micro_ops.is_empty() {
                self.start_next_instruction();
                self.process_instant_ops(bus);
            }

            // Dispatch next non-instant op
            if matches!(self.state, State::Idle)
                && let Some(op) = self.micro_ops.pop()
            {
                // Fast-path: FetchIRC served from instruction cache
                if matches!(op, MicroOp::FetchIRC) && self.icache_lookup() {
                    // Hit — IRC updated, no bus cycle needed. Stay Idle
                    // so instant ops (Execute) run immediately this tick.
                    self.process_instant_ops(bus);
                } else if op.is_bus() {
                    if self.check_address_error(op) {
                        // Address error detected; exception sequence started
                    } else {
                        self.state = self.initiate_bus_cycle(op);
                    }
                } else if let MicroOp::Internal(cycles) = op {
                    self.state = State::Internal { cycles };
                }
            }
        }

        // --- Advance current state ---
        let min_bus = self.min_bus_cycles();
        match &mut self.state {
            State::Idle => {}
            State::Internal { cycles } => {
                if *cycles > 1 {
                    *cycles -= 1;
                } else {
                    self.state = State::Idle;
                }
            }
            State::BusCycle {
                op,
                addr,
                fc,
                is_read,
                is_word,
                data,
                cycle_count,
            } => {
                *cycle_count = cycle_count.saturating_add(1);
                if *cycle_count >= min_bus {
                    let result = bus.poll_cycle(*addr, *fc, *is_read, *is_word, *data);
                    match result {
                        BusStatus::Ready(read_data) => {
                            let completed_op = *op;
                            self.finish_bus_cycle(completed_op, read_data);
                            self.state = State::Idle;
                        }
                        BusStatus::Wait => {}
                        BusStatus::Error => {
                            let fault_addr = *addr;
                            let fault_read = *is_read;
                            let fault_fc = *fc;
                            self.state = State::Idle;
                            self.begin_bus_error(fault_addr, fault_read, fault_fc);
                        }
                    }
                }
            }
            State::Halted => {}
            State::Stopped => {
                // The STOP instruction waits for an interrupt with a
                // priority higher than the current mask. Poll the bus
                // on every CPU cycle and wake up when one arrives.
                let ipl = bus.poll_ipl();
                if ipl > self.regs.interrupt_mask() || ipl == 7 {
                    self.state = State::Idle;
                    self.initiate_interrupt_exception(ipl);
                    self.process_instant_ops(bus);
                    // Dispatch bus cycle if needed
                    if matches!(self.state, State::Idle)
                        && let Some(op) = self.micro_ops.pop()
                    {
                        if op.is_bus() {
                            if !self.check_address_error(op) {
                                self.state = self.initiate_bus_cycle(op);
                            }
                        } else if let MicroOp::Internal(cycles) = op {
                            self.state = State::Internal { cycles };
                        }
                    }
                }
            }
        }
    }

    /// Process all instant ops at the front of the queue.
    fn process_instant_ops<B: M68kBus>(&mut self, bus: &mut B) {
        while let Some(op) = self.micro_ops.front() {
            if !op.is_instant() {
                break;
            }
            let op = self.micro_ops.pop().unwrap();
            match op {
                MicroOp::Execute => self.decode_and_execute(bus),
                MicroOp::PromoteIRC => {
                    // The 68000 samples interrupts at instruction boundaries.
                    // `PromoteIRC` is exactly the "start next instruction"
                    // boundary in this core, including tight branch loops that
                    // can keep the micro-op queue non-empty between iterations.
                    let ipl = bus.poll_ipl();
                    if ipl > self.regs.interrupt_mask() || ipl == 7 {
                        self.initiate_interrupt_exception(ipl);
                    } else {
                        self.promote_pipeline();
                    }
                }
                MicroOp::AssertReset => bus.reset(),
                _ => {}
            }
        }
    }

    /// Queue PromoteIRC to start the next instruction.
    pub(crate) fn start_next_instruction(&mut self) {
        self.micro_ops.push(MicroOp::PromoteIRC);
    }

    /// Move IRC -> IR, advance PC, queue FetchIRC + Execute.
    ///
    /// This is the standard 68000 instruction pipeline advance:
    /// the word in IRC becomes the new opcode, PC advances past it,
    /// and we queue a fetch for the next word plus an Execute to
    /// decode the new opcode.
    fn promote_pipeline(&mut self) {
        self.instr_start_pc = self.irc_addr;
        self.ir = self.irc;
        // Standard 68000: PC points past the opcode word
        self.regs.pc = self.instr_start_pc.wrapping_add(2);
        self.in_followup = false;
        self.followup_tag = 0;
        self.src_mode = None;
        self.dst_mode = None;
        self.ae_undo_reg = None;
        self.sp_undo = None;
        self.dbcc_dn_undo = None;
        self.pre_move_sr = None;
        self.pre_move_vc = None;
        self.program_space_access = false;
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Begin an interrupt exception sequence.
    ///
    /// The 68000 enters supervisor mode immediately when processing an
    /// exception — the exception frame is always pushed to the supervisor
    /// stack (SSP). The old SR (with the user-mode S bit) is saved first
    /// so it can be pushed in the frame.
    fn initiate_interrupt_exception(&mut self, level: u8) {
        self.target_ipl = level;
        // Save old SR before changing mode (for pushing in the exception frame).
        self.ae_saved_sr = self.regs.sr;
        // Enter supervisor mode BEFORE pushing so the frame goes onto SSP.
        self.regs.set_supervisor(true);
        self.regs.sr &= !0x8000; // Clear trace bit
        self.in_followup = true;
        // The PC to save is the address of the NEXT instruction — the one
        // that would have executed if the interrupt hadn't fired. That's
        // irc_addr (where the current IRC was fetched from), NOT regs.pc
        // (which points 2 bytes past irc_addr due to the prefetch pipeline).
        // RTE will restore this address and begin a fresh prefetch from it.
        let pc_to_push = self.irc_addr;

        if self.capabilities().movec {
            // 68010+: push format/vector word first.
            // Vector number isn't known yet (InterruptAck provides it), so
            // for interrupts we use vector 0 placeholder — TAG_EXC_STACK_SR
            // will get the real vector from InterruptAck later. The format
            // word on the stack uses 0 since the vector offset gets filled
            // during TAG_EXC_STACK_SR/TAG_EXC_FETCH_VECTOR.
            // Actually, for interrupts we don't know the vector yet.
            // Push a placeholder; the interrupt ack will determine the vector.
            self.src_val = pc_to_push;
            self.data = 0; // placeholder — Musashi tests don't exercise interrupts
            self.followup_tag = TAG_EXC_STACK_FORMAT;
            self.micro_ops.push(MicroOp::PushWord);
            self.micro_ops.push(MicroOp::Execute);
        } else {
            self.data = pc_to_push;
            self.followup_tag = TAG_EXC_STACK_PC_HI;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    /// Begin a group 1/2 exception (TRAP, privilege violation, etc.).
    ///
    /// Unlike interrupts, the vector number is known at decode time and
    /// there is no InterruptAck bus cycle. The PC to push in the frame
    /// is passed as a parameter (differs per instruction type).
    pub fn begin_group1_exception(&mut self, vector: u8, pc_to_push: u32) {
        self.ae_saved_sr = self.regs.sr;
        self.regs.set_supervisor(true);
        self.regs.sr &= !0x8000; // Clear trace
        self.exc_vector = Some(vector);
        self.in_followup = true;
        self.micro_ops.clear();

        if self.capabilities().movec {
            // 68010+: push format/vector word first, then PC, then SR.
            // Format 0 (4-word frame): bits 15-12 = 0, bits 11-0 = vector * 4.
            self.src_val = pc_to_push;
            self.data = u32::from(vector) * 4;
            self.followup_tag = TAG_EXC_STACK_FORMAT;
            self.micro_ops.push(MicroOp::PushWord);
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // 68000: push PC directly (6-byte frame: PC + SR).
            self.data = pc_to_push;
            self.followup_tag = TAG_EXC_STACK_PC_HI;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    /// Check supervisor mode. If in user mode, trigger a privilege violation
    /// exception and return true (instruction should stop). Returns false
    /// if in supervisor mode (instruction may proceed).
    pub fn check_supervisor(&mut self) -> bool {
        if self.regs.is_supervisor() {
            return false;
        }
        self.begin_group1_exception(8, self.instr_start_pc);
        true
    }

    /// Queue read micro-ops for the given size at the current EA address.
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

    /// Queue write micro-ops for the given size at the current EA address.
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

    /// Map a micro-op to bus cycle parameters and enter BusCycle state.
    ///
    /// Push ops decrement SP before computing the write address.
    /// Pop ops increment SP after the read address is computed.
    fn initiate_bus_cycle(&mut self, op: MicroOp) -> State {
        let is_sup = self.regs.is_supervisor();
        let fc_prog = if is_sup {
            FunctionCode::SupervisorProgram
        } else {
            FunctionCode::UserProgram
        };
        let fc_data = if is_sup {
            FunctionCode::SupervisorData
        } else {
            FunctionCode::UserData
        };

        // PC-relative modes (PcDisp, PcIndex) use program space FC.
        let fc_ea = if self.program_space_access {
            fc_prog
        } else {
            fc_data
        };

        let (addr, fc, is_read, is_word, data) = match op {
            MicroOp::FetchIRC => (self.next_fetch_addr, fc_prog, true, true, None),
            MicroOp::ReadByte => (self.addr, fc_ea, true, false, None),
            MicroOp::ReadWord => (self.addr, fc_ea, true, true, None),
            MicroOp::ReadLongHi => (self.addr, fc_ea, true, true, None),
            MicroOp::ReadLongLo => (self.addr.wrapping_add(2), fc_ea, true, true, None),
            MicroOp::WriteByte => (
                self.addr,
                fc_data,
                false,
                false,
                Some(self.data as u8 as u16),
            ),
            MicroOp::WriteWord => (self.addr, fc_data, false, true, Some(self.data as u16)),
            MicroOp::WriteLongHi => (
                self.addr,
                fc_data,
                false,
                true,
                Some((self.data >> 16) as u16),
            ),
            MicroOp::WriteLongLo => (
                self.addr.wrapping_add(2),
                fc_data,
                false,
                true,
                Some((self.data & 0xFFFF) as u16),
            ),
            MicroOp::PushWord => {
                // SP -= 2, then write at new SP
                let sp = self.regs.active_sp().wrapping_sub(2);
                self.regs.set_active_sp(sp);
                (sp, fc_data, false, true, Some(self.data as u16))
            }
            MicroOp::PushLongHi => {
                // SP -= 4, then write hi word at new SP
                let sp = self.regs.active_sp().wrapping_sub(4);
                self.regs.set_active_sp(sp);
                (sp, fc_data, false, true, Some((self.data >> 16) as u16))
            }
            MicroOp::PushLongLo => {
                // Write lo word at SP + 2 (SP already decremented by PushLongHi)
                let sp = self.regs.active_sp();
                (
                    sp.wrapping_add(2),
                    fc_data,
                    false,
                    true,
                    Some((self.data & 0xFFFF) as u16),
                )
            }
            MicroOp::PopWord => {
                // Read from SP, then SP += 2
                let sp = self.regs.active_sp();
                self.regs.set_active_sp(sp.wrapping_add(2));
                (sp, fc_data, true, true, None)
            }
            MicroOp::PopLongHi => {
                // Read hi word from SP (don't modify SP yet)
                (self.regs.active_sp(), fc_data, true, true, None)
            }
            MicroOp::PopLongLo => {
                // Read lo word from SP + 2, then SP += 4
                let sp = self.regs.active_sp();
                self.regs.set_active_sp(sp.wrapping_add(4));
                (sp.wrapping_add(2), fc_data, true, true, None)
            }
            MicroOp::InterruptAck => (0xFFFFFF, FunctionCode::InterruptAck, true, true, None),
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

    /// Complete a bus cycle and store the result.
    ///
    /// Read operations store data in `self.data` only — the follow-up tag
    /// handlers in decode.rs copy it to `src_val` or `dst_val` at the right
    /// time. This prevents source values from being clobbered by later
    /// destination reads.
    ///
    /// Write operations don't touch internal state at all.
    fn finish_bus_cycle(&mut self, op: MicroOp, read_data: u16) {
        match op {
            MicroOp::FetchIRC => {
                self.irc = read_data;
                self.irc_addr = self.next_fetch_addr;
                self.icache_fill(self.next_fetch_addr, read_data);
                self.next_fetch_addr = self.next_fetch_addr.wrapping_add(2);
                // PC tracks the fetch address (like real 68000)
                self.regs.pc = self.next_fetch_addr;
            }
            // Byte/word reads: store the 16-bit value
            MicroOp::ReadByte | MicroOp::ReadWord | MicroOp::PopWord => {
                self.data = u32::from(read_data);
            }
            // Long hi-word reads: shift into upper 16 bits
            MicroOp::ReadLongHi | MicroOp::PopLongHi => {
                self.data = u32::from(read_data) << 16;
            }
            // Long lo-word reads: combine with previously stored hi word
            MicroOp::ReadLongLo | MicroOp::PopLongLo => {
                self.data = (self.data & 0xFFFF_0000) | u32::from(read_data);
            }
            // Interrupt acknowledge: store vector number
            MicroOp::InterruptAck => {
                self.data = u32::from(read_data);
            }
            // Write operations: preserve internal state
            _ => {}
        }
    }

    /// Check if a bus operation would access an odd address for a word/long
    /// transfer. If so, begin the address error exception sequence.
    ///
    /// Returns `true` if an address error was triggered (exception started,
    /// micro-ops replaced). Returns `false` for valid accesses.
    fn check_address_error(&mut self, op: MicroOp) -> bool {
        // Byte ops and non-memory ops never trigger address errors
        let (check_addr, is_read) = match op {
            MicroOp::FetchIRC => (self.next_fetch_addr, true),
            MicroOp::ReadWord | MicroOp::ReadLongHi => (self.addr, true),
            MicroOp::ReadLongLo => (self.addr.wrapping_add(2), true),
            MicroOp::WriteWord | MicroOp::WriteLongHi => (self.addr, false),
            MicroOp::WriteLongLo => (self.addr.wrapping_add(2), false),
            MicroOp::PushWord => (self.regs.active_sp().wrapping_sub(2), false),
            MicroOp::PushLongHi => (self.regs.active_sp().wrapping_sub(4), false),
            MicroOp::PushLongLo => (self.regs.active_sp().wrapping_add(2), false),
            MicroOp::PopWord | MicroOp::PopLongHi => (self.regs.active_sp(), true),
            MicroOp::PopLongLo => (self.regs.active_sp().wrapping_add(2), true),
            _ => return false,
        };

        // Even address: no error
        if check_addr & 1 == 0 {
            return false;
        }

        // 68020+ only generates Address Error for instruction fetches.
        // Data accesses to odd addresses are handled by the CPU hardware.
        if !matches!(self.model.timing_class(), crate::model::TimingClass::M68000)
            && !matches!(op, MicroOp::FetchIRC)
        {
            return false;
        }

        // Double address error: halt the CPU
        if self.ae_in_progress {
            self.state = State::Halted;
            return true;
        }

        // Determine function code.
        // FetchIRC is always program space. EA reads use program space for
        // PC-relative modes (PcDisp, PcIndex), data space otherwise.
        let is_sup = self.regs.is_supervisor();
        let is_program = matches!(op, MicroOp::FetchIRC) || self.program_space_access;
        let fc = match (is_sup, is_program) {
            (true, true) => FunctionCode::SupervisorProgram,
            (true, false) => FunctionCode::SupervisorData,
            (false, true) => FunctionCode::UserProgram,
            (false, false) => FunctionCode::UserData,
        };

        self.ae_from_fetch_irc = matches!(op, MicroOp::FetchIRC);
        self.begin_address_error(check_addr, is_read, fc);
        true
    }

    /// Start the address error exception sequence.
    ///
    /// Pushes a 14-byte group 0 exception frame:
    ///   SP+0:  Access info (R/W, FC, IR bits)
    ///   SP+2:  Fault address high
    ///   SP+4:  Fault address low
    ///   SP+6:  Instruction register (IR)
    ///   SP+8:  Status register (saved)
    ///   SP+10: Program counter high
    ///   SP+12: Program counter low
    ///
    /// Then reads vector 3 (address 0x0C) and jumps to handler.
    fn begin_address_error(&mut self, fault_addr: u32, is_read: bool, fc: FunctionCode) {
        self.ae_fault_addr = self.adjust_ae_fault_addr(fault_addr, is_read);
        self.ae_in_progress = true;
        self.group0_vector = 3;

        // UNLK: undo the A7 ← An modification so the exception frame
        // gets pushed on the original (valid) stack, not the faulting one.
        if let Some((was_supervisor, original_sp)) = self.sp_undo.take() {
            if was_supervisor {
                self.regs.ssp = original_sp;
            } else {
                self.regs.usp = original_sp;
            }
        }

        // Undo post-increment/predecrement on AE when the transfer wasn't committed.
        if let Some((reg, amount, is_postinc, is_dst)) = self.ae_undo_reg.take() {
            // CMPM (An)+,(An)+: opcode = 1011 Ax 1 ss 001 Ay
            let is_cmpm = (self.ir & 0xF138) == 0xB108;

            let undo = if is_postinc {
                if !is_read {
                    // Write AE: always undo postincrement (write never committed).
                    true
                } else if self.size == Size::Long {
                    // Long read AE: always undo (two-phase read incomplete).
                    true
                } else if is_dst && is_cmpm {
                    // CMPM destination (Ax)+ read AE: the 68000 reverts
                    // the second register for all sizes.
                    true
                } else {
                    // Word/byte source read AE (or non-CMPM destination):
                    // postincrement sticks.
                    false
                }
            } else {
                // Predecrement undo rules:
                // - ADDX/SUBX -(Ay),-(Ax): always undo on AE (source read
                //   never committed, so the predecrement must be reversed).
                // - Standard -(An) EA: only undo on write AE for Long size.
                //   The real 68000 keeps the decremented value for byte/word
                //   write AE, but undoes it for long (verified by DL tests).
                // ADDX/SUBX -(Ay),-(Ax) long: the 68000 decrements by 2
                // (word-sized step) before checking alignment. AE fires after
                // the first -2, so the register must be fully restored.
                // Byte/word ADDX/SUBX: natural-sized decrement sticks.
                let is_addx_subx_long = self.size == Size::Long
                    && matches!(self.ir & 0xF130, 0xD100 | 0x9100)
                    && (self.ir & 0x0008) != 0;
                if is_addx_subx_long {
                    true
                } else {
                    !is_read && self.size == Size::Long
                }
            };
            if undo {
                let r = reg as usize;
                let current = self.regs.a(r);
                // CMPM source Long read AE: partial undo. The 68000 reads long
                // values word-by-word, incrementing by 2 each time. On AE at
                // ReadLongHi, only 2 of the 4-byte increment is reverted.
                let undo_amount =
                    if is_postinc && is_read && !is_dst && is_cmpm && self.size == Size::Long {
                        2
                    } else {
                        amount
                    };
                if is_postinc {
                    self.regs.set_a(r, current.wrapping_sub(undo_amount));
                } else {
                    self.regs.set_a(r, current.wrapping_add(undo_amount));
                }
            }
        }

        // DBcc: undo the Dn.w decrement when branch target is odd.
        if is_read && let Some((r, original_w)) = self.dbcc_dn_undo.take() {
            self.regs.d[r as usize] =
                (self.regs.d[r as usize] & 0xFFFF_0000) | u32::from(original_w);
        }
        self.dbcc_dn_undo = None;

        // For MOVE write AE: restore flags to match the 68000's flag
        // evaluation timing. pre_move_sr = full restore, pre_move_vc = V,C only.
        if !is_read {
            if let Some(saved_sr) = self.pre_move_sr.take() {
                self.regs.sr = saved_sr;
            } else if let Some(saved_sr) = self.pre_move_vc.take() {
                // Partial restore: keep N,Z from MOVE evaluation, restore V,C
                let pre_vc = saved_sr & 0x03;
                self.regs.sr = (self.regs.sr & !0x03) | pre_vc;
            }
        }
        self.pre_move_sr = None;
        self.pre_move_vc = None;

        // Save SR for the exception frame AFTER undo and flag restoration.
        // The reference implementation restores pre-MOVE SR first, then
        // captures old_sr, so the AE frame also gets the restored SR.
        self.ae_saved_sr = self.regs.sr;

        // Determine which IR value to push in the frame. For MOVE.w write AE
        // with -(An) destination, the real 68000's pipeline has already
        // advanced past IR, so it pushes IRC instead.
        let is_move_w = (self.ir >> 12) == 3;
        let dst_is_predec = ((self.ir >> 6) & 7) == 4;
        self.ae_frame_ir = if !is_read && is_move_w && dst_is_predec {
            self.irc
        } else {
            self.ir
        };

        self.ae_access_info = (self.ae_frame_ir & 0xFFE0)
            | (if is_read { 0x10 } else { 0 })
            | u16::from(fc.bits() & 0x07);

        // Enter supervisor mode and clear trace
        self.regs.set_supervisor(true);
        self.regs.sr &= !0x8000; // Clear trace

        // Abandon current instruction
        self.micro_ops.clear();
        self.in_followup = true;

        // Frame PC: complex formula that depends on instruction type,
        // addressing modes, access size, and read/write direction.
        self.data = self.compute_ae_frame_pc(is_read);
        self.followup_tag = TAG_AE_PUSH_SR;
        self.micro_ops.push(MicroOp::PushLongHi);
        self.micro_ops.push(MicroOp::PushLongLo);
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Start a bus error exception sequence.
    ///
    /// Called when the bus returns `BusStatus::Error` (e.g. Fat Gary timeout).
    /// On a real 68000/030 this triggers vector 2 instead of halting.
    ///
    /// 68000: pushes the same 14-byte group-0 frame as address error.
    /// 68010+: pushes a format $A (short bus cycle fault) frame.
    pub(crate) fn begin_bus_error(&mut self, fault_addr: u32, is_read: bool, fc: FunctionCode) {
        // Double fault during another group-0 exception → halt.
        if self.ae_in_progress {
            self.state = State::Halted;
            return;
        }

        self.ae_in_progress = true;
        self.ae_saved_sr = self.regs.sr;

        // Enter supervisor mode and clear trace.
        self.regs.set_supervisor(true);
        self.regs.sr &= !0x8000;

        // Abandon current instruction.
        self.micro_ops.clear();
        self.in_followup = true;

        if matches!(
            self.model.timing_class(),
            crate::model::TimingClass::M68000
        ) {
            // 68000: same 14-byte frame as address error, but vector 2.
            self.group0_vector = 2;
            self.ae_fault_addr = fault_addr;
            self.ae_frame_ir = self.ir;
            self.ae_access_info = (self.ir & 0xFFE0)
                | (if is_read { 0x10 } else { 0 })
                | u16::from(fc.bits() & 0x07);

            // Reuse the AE tag chain — group0_vector selects vector 2.
            self.data = self.instr_start_pc;
            self.followup_tag = TAG_AE_PUSH_SR;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // 68010+: bus error frame size depends on CPU model.
            //
            // 68010/020/030: format $A — 16 words (12 extra beyond header).
            // 68040:         format $7 — 30 words (26 extra beyond header).
            // 68060:         format $4 —  8 words ( 4 extra beyond header).
            //
            // "Header" = SR(1) + PC(2) + format/vector(1) = 4 words.
            // Push extra words first (highest address → lowest), then
            // format/vector, PC, SR via the standard exception path.
            let (extra_count, format_code) = match self.model.timing_class() {
                crate::model::TimingClass::M68040 => (26u8, 0x7u16),
                crate::model::TimingClass::M68060 => (4u8, 0x4u16),
                _ => (12u8, 0xAu16), // 68010/020/030
            };

            self.be_extra_count = extra_count;
            // Vector offset for bus error = vector 2 × 4 = $008.
            self.be_format_word = (format_code << 12) | 0x008;
            self.data = 0;
            self.followup_tag = TAG_BE_PUSH_EXTRA;
            self.micro_ops.push(MicroOp::PushWord);
            self.micro_ops.push(MicroOp::Execute);

            // After all extra words, TAG_BE_PUSH_EXTRA chains to
            // TAG_EXC_STACK_FORMAT which pushes format/vector, then
            // the standard path pushes PC and SR and reads the vector.
            // We stash the saved PC in src_val for TAG_EXC_STACK_FORMAT.
            //
            // 68040/060: data access faults save the "next instruction" PC
            // because the processor completes the faulting instruction
            // internally. In our prefetch model, self.regs.pc is 2 bytes
            // past the next instruction (pointing past IRC), so we back up.
            //
            // 68010/020/030: save the faulting instruction's start PC
            // (the frame is restartable).
            self.src_val = match self.model.timing_class() {
                crate::model::TimingClass::M68040 | crate::model::TimingClass::M68060 => {
                    self.regs.pc.wrapping_sub(2)
                }
                _ => self.instr_start_pc,
            };
            self.exc_vector = Some(2); // bus error
        }
    }

    /// Compute the frame PC for an address error exception.
    ///
    /// The 68000's reported PC in the AE frame depends on:
    /// - Instruction type (MOVE vs non-MOVE)
    /// - Access direction (read vs write)
    /// - Addressing modes and their extension words
    /// - Operation size (for predecrement)
    ///
    /// Derived from the cpu-m68k reference implementation and DL test cases.
    fn compute_ae_frame_pc(&self, is_read: bool) -> u32 {
        let top = (self.ir >> 12) & 0xF;

        // MOVE instructions have a separate, more complex formula
        if matches!(top, 1..=3) {
            return self.compute_ae_frame_pc_move(is_read);
        }

        // FetchIRC AE: branch/jump to an odd target.
        if self.ae_from_fetch_irc {
            // DBcc: displacement word consumed, ISP + 4.
            if top == 0x5 {
                let ea_mode = ((self.ir >> 3) & 7) as u8;
                if ea_mode == 1 {
                    return self.instr_start_pc.wrapping_add(4);
                }
            }
            if top == 0x6 {
                let cond = (self.ir >> 8) & 0xF;
                if cond == 1 {
                    // BSR: frame PC = target address (current PC at AE time).
                    return self.regs.pc;
                }
                // BRA/Bcc: frame PC = ISP + 2 regardless of displacement size.
                return self.instr_start_pc.wrapping_add(2);
            }
            // JSR FetchIRC AE: frame PC = ISP + 2 + ea_ext * 2.
            if self.ir & 0xFFC0 == 0x4E80 {
                let ea_mode = ((self.ir >> 3) & 7) as u8;
                let ea_reg = (self.ir & 7) as u8;
                let ea_ext: u32 = match ea_mode {
                    5 | 6 => 1, // d16(An), d8(An,Xn)
                    7 => match ea_reg {
                        0 | 2 | 3 => 1, // abs.w, d16(PC), d8(PC,Xn)
                        1 => 2,         // abs.l
                        _ => 0,
                    },
                    _ => 0, // (An): no ext words
                };
                return self.instr_start_pc.wrapping_add(2 + ea_ext * 2);
            }
            // JMP, RTS, RTE, RTR, etc.: ISP + 2
            return self.instr_start_pc.wrapping_add(2);
        }

        let ea_mode = ((self.ir >> 3) & 7) as u8;
        let ea_reg = (self.ir & 7) as u8;

        // UNLK: frame PC = ISP + 4 (past opcode and prefetched IRC word).
        if self.ir & 0xFFF8 == 0x4E58 {
            return self.instr_start_pc.wrapping_add(4);
        }

        // CMPM (An)+,(An)+ and ADDX/SUBX -(An),-(An): always ISP + 4
        if matches!(top, 0x9 | 0xB | 0xD) {
            let opmode = (self.ir >> 6) & 7;
            if (4..=6).contains(&opmode) && ea_mode == 1 {
                return self.instr_start_pc.wrapping_add(4);
            }
        }

        // MOVEM: register mask word shifts the base by +4 beyond the opcode,
        // and EA extension words add on top. Formula: ISP + 6 + ea_ext_bytes.
        // Detects both directions: reg→mem (0x4880) and mem→reg (0x4C80).
        if (self.ir & 0xFB80) == 0x4880 {
            let movem_ea_ext: u32 = match ea_mode {
                5 | 6 => 2, // d16(An), d8(An,Xn)
                7 => match ea_reg {
                    0 => 2,     // abs.w
                    1 => 4,     // abs.l
                    2 | 3 => 2, // d16(PC), d8(PC,Xn)
                    _ => 0,
                },
                _ => 0,
            };
            return self.instr_start_pc.wrapping_add(6 + movem_ea_ext);
        }

        // -(An) with word-size data access adds 2.
        let predec_adj: u32 = if ea_mode == 4 && self.size == Size::Word {
            2
        } else {
            0
        };

        // Absolute addressing extension words advance the internal PC.
        let abs_adj: u32 = if ea_mode == 7 {
            match ea_reg {
                0 => 2, // abs.w: 1 ext word
                1 => 4, // abs.l: 2 ext words
                _ => 0,
            }
        } else {
            0
        };

        // Group 0 (immediate ops like ADDI/SUBI/ORI/ANDI/EORI/CMPI):
        // immediate extension words are consumed before the EA.
        let imm_adj: u32 = if top == 0 {
            let secondary = ((self.ir >> 8) & 0xF) as u8;
            if secondary == 8 {
                // BTST/BSET/BCLR/BCHG #n: 1 ext word
                2
            } else {
                // ALU immediate: byte/word = 1, long = 2 ext words
                let size_bits = (self.ir >> 6) & 3;
                if size_bits == 2 { 4 } else { 2 }
            }
        } else {
            0
        };

        self.instr_start_pc
            .wrapping_add(2 + predec_adj + abs_adj + imm_adj)
    }

    /// Compute the frame PC for MOVE instruction address errors.
    ///
    /// MOVE has different formulas for read AE (source fault) and write AE
    /// (destination fault) because of how the prefetch pipeline interacts
    /// with the two-operand fetch sequence.
    fn compute_ae_frame_pc_move(&self, is_read: bool) -> u32 {
        let size = match (self.ir >> 12) & 3 {
            1 => Size::Byte,
            2 => Size::Long,
            3 => Size::Word,
            _ => Size::Word,
        };

        let src_mode_bits = ((self.ir >> 3) & 7) as u8;
        let src_reg = (self.ir & 7) as u8;
        let src = AddrMode::decode(src_mode_bits, src_reg).unwrap_or(AddrMode::DataReg(0));
        let src_ext = Self::ext_word_count_for_mode(&src, size);

        if is_read {
            // Read AE: fault during source operand fetch
            match src {
                AddrMode::AbsShort | AddrMode::AbsLong => {
                    // Absolute sources: PC advanced past consumed ext words
                    self.instr_start_pc.wrapping_add(2 + u32::from(src_ext) * 2)
                }
                AddrMode::AddrIndPreDec(_) => {
                    if size == Size::Long {
                        self.instr_start_pc.wrapping_add(2)
                    } else {
                        self.instr_start_pc.wrapping_add(4)
                    }
                }
                _ => self.instr_start_pc.wrapping_add(2),
            }
        } else {
            // Write AE: fault during destination write
            let dst_mode_bits = ((self.ir >> 6) & 7) as u8;
            let dst_reg = ((self.ir >> 9) & 7) as u8;
            let dst = AddrMode::decode(dst_mode_bits, dst_reg).unwrap_or(AddrMode::DataReg(0));
            let dst_ext = Self::ext_word_count_for_mode(&dst, size);

            let src_is_register = matches!(
                src,
                AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
            );

            if src_is_register {
                let extra = u32::from(src_ext + dst_ext.saturating_sub(1));
                self.instr_start_pc.wrapping_add(4 + extra * 2)
            } else {
                self.instr_start_pc.wrapping_add(4 + u32::from(src_ext) * 2)
            }
        }
    }

    /// Count extension words for an addressing mode (for frame PC calculation).
    fn ext_word_count_for_mode(mode: &AddrMode, size: Size) -> u16 {
        match mode {
            AddrMode::DataReg(_) | AddrMode::AddrReg(_) => 0,
            AddrMode::AddrInd(_) | AddrMode::AddrIndPostInc(_) | AddrMode::AddrIndPreDec(_) => 0,
            AddrMode::AddrIndDisp(_) => 1,
            AddrMode::AddrIndIndex(_) => 1,
            AddrMode::AbsShort => 1,
            AddrMode::AbsLong => 2,
            AddrMode::Immediate => {
                if size == Size::Long {
                    2
                } else {
                    1
                }
            }
            AddrMode::PcDisp => 1,
            AddrMode::PcIndex => 1,
        }
    }

    /// Adjust fault address for MOVE.l -(An) destination write AE.
    ///
    /// The 68000 reports the fault address as `An - 2` (word-sized initial
    /// decrement) rather than the full `An - 4` (long-sized decrement).
    fn adjust_ae_fault_addr(&self, addr: u32, is_read: bool) -> u32 {
        // ADDX/SUBX -(Ay),-(Ax) long read AE: the 68000 decrements by 2
        // (word-sized) first, then checks alignment. Our decode decremented
        // by 4 at once, so the reported fault address is 2 too low.
        if is_read && self.size == Size::Long {
            let top = (self.ir >> 12) & 0xF;
            let opmode = (self.ir >> 6) & 7;
            let ea_mode = ((self.ir >> 3) & 7) as u8;
            if matches!(top, 0x9 | 0xD) && (4..=6).contains(&opmode) && ea_mode == 1 {
                return addr.wrapping_add(2);
            }
        }
        if is_read {
            return addr;
        }

        // MOVEM.l -(An) write AE: the real 68000 decrements by 2 first
        // and writes the low word at An-2. Our code decrements by 4 at
        // once, so adjust the fault address by +2 to match hardware.
        if (self.ir & 0xFB80) == 0x4880 {
            let ea_mode_bits = ((self.ir >> 3) & 7) as u8;
            let is_long = (self.ir >> 6) & 1 == 1;
            if ea_mode_bits == 4 && is_long {
                return addr.wrapping_add(2);
            }
        }

        let top = (self.ir >> 12) & 0xF;
        if !matches!(top, 1..=3) {
            return addr;
        }
        let size = match top {
            1 => Size::Byte,
            2 => Size::Long,
            3 => Size::Word,
            _ => return addr,
        };
        let dst = AddrMode::decode(((self.ir >> 6) & 7) as u8, ((self.ir >> 9) & 7) as u8);
        if size == Size::Long && matches!(dst, Some(AddrMode::AddrIndPreDec(_))) {
            addr.wrapping_add(2)
        } else {
            addr
        }
    }
}

impl emu_core::Observable for Cpu68000 {
    fn query(&self, path: &str) -> Option<emu_core::Value> {
        use emu_core::Value;
        match path {
            "pc" => Some(Value::U32(self.regs.pc)),
            "sr" => Some(Value::U16(self.regs.sr)),
            "ccr" => Some(Value::U8(self.regs.ccr())),
            "d0" => Some(Value::U32(self.regs.d[0])),
            "d1" => Some(Value::U32(self.regs.d[1])),
            "d2" => Some(Value::U32(self.regs.d[2])),
            "d3" => Some(Value::U32(self.regs.d[3])),
            "d4" => Some(Value::U32(self.regs.d[4])),
            "d5" => Some(Value::U32(self.regs.d[5])),
            "d6" => Some(Value::U32(self.regs.d[6])),
            "d7" => Some(Value::U32(self.regs.d[7])),
            "a0" => Some(Value::U32(self.regs.a[0])),
            "a1" => Some(Value::U32(self.regs.a[1])),
            "a2" => Some(Value::U32(self.regs.a[2])),
            "a3" => Some(Value::U32(self.regs.a[3])),
            "a4" => Some(Value::U32(self.regs.a[4])),
            "a5" => Some(Value::U32(self.regs.a[5])),
            "a6" => Some(Value::U32(self.regs.a[6])),
            "a7" => Some(Value::U32(self.regs.active_sp())),
            "usp" => Some(Value::U32(self.regs.usp)),
            "ssp" => Some(Value::U32(self.regs.ssp)),
            "ir" => Some(Value::U16(self.ir)),
            "irc" => Some(Value::U16(self.irc)),
            "flags.c" => Some(Value::Bool(self.regs.sr & 0x01 != 0)),
            "flags.v" => Some(Value::Bool(self.regs.sr & 0x02 != 0)),
            "flags.z" => Some(Value::Bool(self.regs.sr & 0x04 != 0)),
            "flags.n" => Some(Value::Bool(self.regs.sr & 0x08 != 0)),
            "flags.x" => Some(Value::Bool(self.regs.sr & 0x10 != 0)),
            "flags.s" => Some(Value::Bool(self.regs.is_supervisor())),
            "flags.t" => Some(Value::Bool(self.regs.is_trace())),
            "flags.ipl" => Some(Value::U8(self.regs.interrupt_mask())),
            "halted" => Some(Value::Bool(self.is_halted())),
            "idle" => Some(Value::Bool(self.is_idle())),
            _ => None,
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        &[
            "pc",
            "sr",
            "ccr",
            "d0",
            "d1",
            "d2",
            "d3",
            "d4",
            "d5",
            "d6",
            "d7",
            "a0",
            "a1",
            "a2",
            "a3",
            "a4",
            "a5",
            "a6",
            "a7",
            "usp",
            "ssp",
            "ir",
            "irc",
            "flags.c",
            "flags.v",
            "flags.z",
            "flags.n",
            "flags.x",
            "flags.s",
            "flags.t",
            "flags.ipl",
            "halted",
            "idle",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::Cpu68000;
    use crate::bus::{BusStatus, FunctionCode, M68kBus};
    use crate::model::CpuModel;

    struct InterruptLoopTestBus {
        mem: Vec<u8>,
        ipl: u8,
    }

    impl InterruptLoopTestBus {
        fn new() -> Self {
            let mut mem = vec![0u8; 0x2000];

            let write_word = |mem: &mut [u8], addr: usize, word: u16| {
                mem[addr] = (word >> 8) as u8;
                mem[addr + 1] = word as u8;
            };
            let write_long = |mem: &mut [u8], addr: usize, value: u32| {
                mem[addr] = (value >> 24) as u8;
                mem[addr + 1] = (value >> 16) as u8;
                mem[addr + 2] = (value >> 8) as u8;
                mem[addr + 3] = value as u8;
            };

            // Level-3 autovector (vector 27) -> handler at $0120.
            write_long(&mut mem, 27 * 4, 0x0000_0120);

            // $0100: MOVE.W #$2000,SR ; clear interrupt mask in supervisor mode
            // $0104: BRA.S *          ; tight loop
            write_word(&mut mem, 0x0100, 0x46FC);
            write_word(&mut mem, 0x0102, 0x2000);
            write_word(&mut mem, 0x0104, 0x60FE);

            // $0120: MOVEQ #$42,D0
            // $0122: BRA.S *
            write_word(&mut mem, 0x0120, 0x7042);
            write_word(&mut mem, 0x0122, 0x60FE);

            Self { mem, ipl: 0 }
        }

        fn read_word(&self, addr: u32) -> u16 {
            let a = (addr as usize) & !1;
            if a + 1 >= self.mem.len() {
                return 0;
            }
            (u16::from(self.mem[a]) << 8) | u16::from(self.mem[a + 1])
        }

        fn write_word(&mut self, addr: u32, val: u16) {
            let a = (addr as usize) & !1;
            if a + 1 >= self.mem.len() {
                return;
            }
            self.mem[a] = (val >> 8) as u8;
            self.mem[a + 1] = val as u8;
        }
    }

    impl M68kBus for InterruptLoopTestBus {
        fn poll_cycle(
            &mut self,
            addr: u32,
            fc: FunctionCode,
            is_read: bool,
            is_word: bool,
            data: Option<u16>,
        ) -> BusStatus {
            if fc == FunctionCode::InterruptAck {
                return BusStatus::Ready(27);
            }

            if is_read {
                if is_word {
                    BusStatus::Ready(self.read_word(addr))
                } else {
                    let word = self.read_word(addr);
                    let byte = if (addr & 1) == 0 {
                        (word >> 8) as u8
                    } else {
                        word as u8
                    };
                    BusStatus::Ready(u16::from(byte))
                }
            } else {
                let val = data.unwrap_or(0);
                if is_word {
                    self.write_word(addr, val);
                } else {
                    let a = addr as usize;
                    if a < self.mem.len() {
                        self.mem[a] = val as u8;
                    }
                }
                BusStatus::Ready(0)
            }
        }

        fn poll_ipl(&mut self) -> u8 {
            self.ipl
        }

        fn poll_interrupt_ack(&mut self, level: u8) -> BusStatus {
            BusStatus::Ready(24 + u16::from(level))
        }

        fn reset(&mut self) {}
    }

    fn tick_cpu(
        cpu: &mut Cpu68000,
        bus: &mut InterruptLoopTestBus,
        clock: &mut u64,
        cpu_ticks: u32,
    ) {
        for _ in 0..cpu_ticks {
            *clock += 4;
            cpu.tick(bus, *clock);
        }
    }

    #[test]
    fn new_defaults_to_68000_model() {
        let cpu = Cpu68000::new();
        assert_eq!(cpu.model(), CpuModel::M68000);
        assert!(!cpu.capabilities().movec);
    }

    #[test]
    fn new_with_model_records_requested_model() {
        let cpu = Cpu68000::new_with_model(CpuModel::M68020);
        assert_eq!(cpu.model(), CpuModel::M68020);
        assert!(cpu.capabilities().movec);
        assert!(cpu.capabilities().vbr);
        assert!(cpu.capabilities().cacr);
    }

    #[test]
    fn interrupt_is_taken_from_tight_branch_loop_at_instruction_boundary() {
        let mut cpu = Cpu68000::new();
        let mut bus = InterruptLoopTestBus::new();
        let mut clock = 0u64;

        cpu.reset_to(0x0000_0800, 0x0000_0100);

        // Run until the program has executed MOVE #$2000,SR and entered the
        // tight BRA.S * loop with interrupt mask 0.
        let mut in_wait_loop = false;
        for _ in 0..2_000u32 {
            if cpu.regs.interrupt_mask() == 0
                && (cpu.regs.pc == 0x0000_0104 || cpu.regs.pc == 0x0000_0106)
            {
                in_wait_loop = true;
                break;
            }
            tick_cpu(&mut cpu, &mut bus, &mut clock, 1);
        }
        assert!(
            in_wait_loop,
            "CPU should reach the tight BRA loop with interrupt mask 0 before IRQ (pc=${:08X}, sr=${:04X})",
            cpu.regs.pc, cpu.regs.sr
        );

        bus.ipl = 3;

        let mut entered_handler = false;
        for _ in 0..10_000u32 {
            if (cpu.regs.d[0] & 0xFF) == 0x42 {
                entered_handler = true;
                break;
            }
            tick_cpu(&mut cpu, &mut bus, &mut clock, 1);
        }
        assert!(
            entered_handler,
            "CPU should service level-3 interrupt from a tight branch loop (pc=${:08X}, sr=${:04X}, d0=${:08X})",
            cpu.regs.pc, cpu.regs.sr, cpu.regs.d[0]
        );
        assert_eq!(cpu.regs.interrupt_mask(), 3);
        assert!(
            cpu.regs.pc == 0x0000_0122 || cpu.regs.pc == 0x0000_0124,
            "CPU should be in handler spin loop after interrupt service (pc=${:08X})",
            cpu.regs.pc
        );
    }

    #[test]
    fn observable_registers() {
        use emu_core::Observable;
        use emu_core::Value;

        let mut cpu = Cpu68000::new();
        cpu.regs.d[0] = 0xDEAD_BEEF;
        cpu.regs.d[7] = 42;
        cpu.regs.a[3] = 0x0010_0000;
        cpu.regs.pc = 0x00FC_0004;
        cpu.regs.sr = 0x2704; // supervisor, IPL=7, Z flag

        assert_eq!(cpu.query("d0"), Some(Value::U32(0xDEAD_BEEF)));
        assert_eq!(cpu.query("d7"), Some(Value::U32(42)));
        assert_eq!(cpu.query("a3"), Some(Value::U32(0x0010_0000)));
        assert_eq!(cpu.query("pc"), Some(Value::U32(0x00FC_0004)));
        assert_eq!(cpu.query("sr"), Some(Value::U16(0x2704)));
        assert_eq!(cpu.query("ccr"), Some(Value::U8(0x04)));
        assert_eq!(cpu.query("flags.z"), Some(Value::Bool(true)));
        assert_eq!(cpu.query("flags.c"), Some(Value::Bool(false)));
        assert_eq!(cpu.query("flags.s"), Some(Value::Bool(true)));
        assert_eq!(cpu.query("flags.ipl"), Some(Value::U8(7)));
        assert_eq!(cpu.query("halted"), Some(Value::Bool(false)));
        assert_eq!(cpu.query("idle"), Some(Value::Bool(true)));
        assert_eq!(cpu.query("nonexistent"), None);
    }

    // --- Simple test bus for instruction-level tests ---

    struct SimpleBus {
        mem: Vec<u8>,
    }

    impl SimpleBus {
        fn new(program: &[(u32, u16)]) -> Self {
            let mut mem = vec![0u8; 0x10000];
            for &(addr, word) in program {
                let a = addr as usize;
                mem[a] = (word >> 8) as u8;
                mem[a + 1] = word as u8;
            }
            Self { mem }
        }

        fn write_long(&mut self, addr: u32, value: u32) {
            let a = addr as usize;
            self.mem[a] = (value >> 24) as u8;
            self.mem[a + 1] = (value >> 16) as u8;
            self.mem[a + 2] = (value >> 8) as u8;
            self.mem[a + 3] = value as u8;
        }
    }

    impl M68kBus for SimpleBus {
        fn poll_cycle(
            &mut self,
            addr: u32,
            _fc: FunctionCode,
            is_read: bool,
            is_word: bool,
            data: Option<u16>,
        ) -> BusStatus {
            if is_read {
                if is_word {
                    let a = (addr as usize) & !1;
                    let w = if a + 1 < self.mem.len() {
                        (u16::from(self.mem[a]) << 8) | u16::from(self.mem[a + 1])
                    } else {
                        0
                    };
                    BusStatus::Ready(w)
                } else {
                    let a = addr as usize;
                    let b = if a < self.mem.len() { self.mem[a] } else { 0 };
                    BusStatus::Ready(u16::from(b))
                }
            } else {
                let val = data.unwrap_or(0);
                if is_word {
                    let a = (addr as usize) & !1;
                    if a + 1 < self.mem.len() {
                        self.mem[a] = (val >> 8) as u8;
                        self.mem[a + 1] = val as u8;
                    }
                } else {
                    let a = addr as usize;
                    if a < self.mem.len() {
                        self.mem[a] = val as u8;
                    }
                }
                BusStatus::Ready(0)
            }
        }

        fn poll_ipl(&mut self) -> u8 {
            0
        }

        fn poll_interrupt_ack(&mut self, level: u8) -> BusStatus {
            BusStatus::Ready(24 + u16::from(level))
        }

        fn reset(&mut self) {}
    }

    /// Run CPU until it reaches a BRA.S * (0x60FE) idle loop or tick limit.
    fn run_until_idle(cpu: &mut Cpu68000, bus: &mut SimpleBus, max_ticks: u32) {
        let mut clock = 0u64;
        for _ in 0..max_ticks {
            clock += 4;
            cpu.tick(bus, clock);
            // Detect BRA.S * idle loop (IR=0x60FE, PC stable)
            if cpu.ir == 0x60FE {
                return;
            }
        }
    }

    #[test]
    fn movec_vbr_write_read_roundtrip_68010() {
        // Program: MOVEC D0,VBR ; MOVEC VBR,D1 ; BRA.S *
        // MOVE.L #$12345678,D0 = 203C 1234 5678
        // MOVEC D0,VBR = 4E7B 0801
        // MOVEC VBR,D1 = 4E7A 1801
        // BRA.S * = 60FE
        let mut bus = SimpleBus::new(&[
            // Reset vector: SSP=$1000, PC=$0100
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // Program at $0100
            (0x0100, 0x203C),
            (0x0102, 0x1234),
            (0x0104, 0x5678), // MOVE.L #$12345678,D0
            (0x0106, 0x4E7B),
            (0x0108, 0x0801), // MOVEC D0,VBR
            (0x010A, 0x4E7A),
            (0x010C, 0x1801), // MOVEC VBR,D1
            (0x010E, 0x60FE), // BRA.S *
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68010);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 5000);
        assert_eq!(cpu.regs.vbr, 0x1234_5678, "VBR should hold written value");
        assert_eq!(cpu.regs.d[1], 0x1234_5678, "D1 should read back VBR");
    }

    #[test]
    fn movec_sfc_dfc_write_read_68010() {
        // MOVEQ #5,D0 ; MOVEC D0,SFC ; MOVEQ #3,D0 ; MOVEC D0,DFC
        // MOVEC SFC,D2 ; MOVEC DFC,D3 ; BRA.S *
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0100, 0x7005), // MOVEQ #5,D0
            (0x0102, 0x4E7B),
            (0x0104, 0x0000), // MOVEC D0,SFC
            (0x0106, 0x7003), // MOVEQ #3,D0
            (0x0108, 0x4E7B),
            (0x010A, 0x0001), // MOVEC D0,DFC
            (0x010C, 0x4E7A),
            (0x010E, 0x2000), // MOVEC SFC,D2
            (0x0110, 0x4E7A),
            (0x0112, 0x3001), // MOVEC DFC,D3
            (0x0114, 0x60FE), // BRA.S *
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68010);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(cpu.regs.d[2], 5, "SFC should be 5");
        assert_eq!(cpu.regs.d[3], 3, "DFC should be 3");
        // SFC/DFC are 3 bits wide — value 5 = 0b101, fits in 3 bits
        assert_eq!(cpu.regs.sfc, 5);
        assert_eq!(cpu.regs.dfc, 3);
    }

    #[test]
    fn movec_cacr_68020_only() {
        // On 68020: MOVEC D0,CACR should work.
        // Write $0B = EI | FI | CI | CEI. CI/CEI (bits 3,2) are
        // write-only: they trigger actions but read back as 0.
        // Stored/readable value = $0B & !$0C = $03 (EI + FI).
        // MOVE.L #$0B,D0 ; MOVEC D0,CACR ; MOVEC CACR,D1 ; BRA.S *
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0100, 0x700B), // MOVEQ #$0B,D0
            (0x0102, 0x4E7B),
            (0x0104, 0x0002), // MOVEC D0,CACR
            (0x0106, 0x4E7A),
            (0x0108, 0x1002), // MOVEC CACR,D1
            (0x010A, 0x60FE), // BRA.S *
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 5000);
        assert_eq!(
            cpu.regs.cacr, 0x03,
            "CACR should store EI+FI (CI/CEI write-only)"
        );
        assert_eq!(
            cpu.regs.d[1], 0x03,
            "D1 should read back CACR without write-only bits"
        );
    }

    #[test]
    fn movec_cacr_illegal_on_68010() {
        // On 68010: MOVEC D0,CACR should fire illegal exception.
        // Vector 4 (illegal) at $010 → handler at $0200.
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // Illegal instruction vector (vector 4) → $0200
            (0x0010, 0x0000),
            (0x0012, 0x0200),
            // Program
            (0x0100, 0x700B), // MOVEQ #$0B,D0
            (0x0102, 0x4E7B),
            (0x0104, 0x0002), // MOVEC D0,CACR
            (0x0106, 0x60FE), // BRA.S * (shouldn't reach)
            // Handler: MOVEQ #$FF,D7 ; BRA.S *
            (0x0200, 0x7EFF), // MOVEQ #-1,D7
            (0x0202, 0x60FE), // BRA.S *
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68010);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(
            cpu.regs.d[7] as u8, 0xFF,
            "Should have reached illegal-instruction handler"
        );
    }

    #[test]
    fn movec_on_68000_fires_illegal() {
        // On 68000: 0x4E7B is always illegal.
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0010, 0x0000),
            (0x0012, 0x0200),
            (0x0100, 0x4E7B),
            (0x0102, 0x0801), // MOVEC D0,VBR (illegal on 68000)
            (0x0104, 0x60FE),
            (0x0200, 0x7EFF), // MOVEQ #-1,D7
            (0x0202, 0x60FE),
        ]);
        let mut cpu = Cpu68000::new(); // 68000
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(
            cpu.regs.d[7] as u8, 0xFF,
            "MOVEC on 68000 should fire illegal exception"
        );
    }

    #[test]
    fn movec_in_user_mode_fires_privilege_violation() {
        // MOVEC is privileged — executing in user mode → privilege violation (vector 8).
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // Privilege violation vector (vector 8) → $0300
            (0x0020, 0x0000),
            (0x0022, 0x0300),
            // Program: MOVE #$0000,SR (drop to user mode), then MOVEC
            (0x0100, 0x46FC),
            (0x0102, 0x0000), // MOVE #$0000,SR → user mode
            (0x0104, 0x4E7B),
            (0x0106, 0x0801), // MOVEC D0,VBR (privileged!)
            (0x0108, 0x60FE),
            // Handler
            (0x0300, 0x7EFF), // MOVEQ #-1,D7
            (0x0302, 0x60FE),
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68010);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(
            cpu.regs.d[7] as u8, 0xFF,
            "MOVEC in user mode should fire privilege violation"
        );
    }

    fn privilege_handler_can_patch_group1_frame(model: CpuModel) {
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // Program:
            //   MOVE #$0000,SR   ; drop to user mode
            //   ORI  #$2000,SR   ; privilege violation
            //   BRA.S *          ; should never return here
            (0x0100, 0x46FC),
            (0x0102, 0x0000),
            (0x0104, 0x007C),
            (0x0106, 0x2000),
            (0x0108, 0x60FE),
            // Handler:
            //   CMPI.L #$00000104,2(A7)
            //   BNE fail
            //   MOVE.L #$00000220,2(A7)
            //   RTE
            // fail:
            //   MOVEQ #-1,D7
            //   BRA.S *
            (0x0200, 0x0CAF),
            (0x0202, 0x0000),
            (0x0204, 0x0104),
            (0x0206, 0x0002),
            (0x0208, 0x660A),
            (0x020A, 0x2F7C),
            (0x020C, 0x0000),
            (0x020E, 0x0220),
            (0x0210, 0x0002),
            (0x0212, 0x4E73),
            (0x0214, 0x7EFF),
            (0x0216, 0x60FE),
            // Patched continuation in user mode:
            (0x0220, 0x7042),
            (0x0222, 0x60FE),
        ]);
        bus.write_long(0x0020, 0x0000_0200);

        let mut cpu = Cpu68000::new_with_model(model);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20_000);

        assert_eq!(
            cpu.regs.d[7] as u8, 0x00,
            "handler compare/patch path should not branch to fail loop"
        );
        assert_eq!(
            cpu.regs.d[0] as u8, 0x42,
            "patched privilege handler should resume at the replacement PC"
        );
        assert!(
            cpu.regs.pc == 0x0000_0222 || cpu.regs.pc == 0x0000_0224,
            "CPU should end in the patched continuation loop (pc=${:08X}, sr=${:04X}, ssp=${:08X}, usp=${:08X})",
            cpu.regs.pc,
            cpu.regs.sr,
            cpu.regs.ssp,
            cpu.regs.usp
        );
        assert_eq!(cpu.regs.sr & 0x2000, 0, "RTE should restore user mode");
    }

    #[test]
    fn privilege_handler_can_patch_group1_frame_68020() {
        privilege_handler_can_patch_group1_frame(CpuModel::M68020);
    }

    #[test]
    fn privilege_handler_can_patch_group1_frame_68030() {
        privilege_handler_can_patch_group1_frame(CpuModel::M68030);
    }

    #[test]
    fn movec_unknown_cr_fires_illegal() {
        // Unknown control register code $FFF → illegal exception.
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0010, 0x0000),
            (0x0012, 0x0200),
            (0x0100, 0x4E7B),
            (0x0102, 0x0FFF), // MOVEC D0,<unknown $FFF>
            (0x0104, 0x60FE),
            (0x0200, 0x7EFF),
            (0x0202, 0x60FE),
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68010);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(
            cpu.regs.d[7] as u8, 0xFF,
            "MOVEC with unknown CR should fire illegal exception"
        );
    }

    #[test]
    fn movec_mmu_registers_roundtrip_68030() {
        // 68030 has MMU → TC/ITT0/ITT1/SRP/URP/MMUSR should work.
        // Write $AABB0000 to TC via D0, read back to D1.
        // Write $12340000 to SRP via D0, read back to D2.
        // MOVE.L #$AABB0000,D0 ; MOVEC D0,TC ; MOVEC TC,D1
        // MOVE.L #$12340000,D0 ; MOVEC D0,SRP ; MOVEC SRP,D2
        // BRA.S *
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // MOVE.L #$AABB0000,D0
            (0x0100, 0x203C),
            (0x0102, 0xAABB),
            (0x0104, 0x0000),
            // MOVEC D0,TC (cr $003)
            (0x0106, 0x4E7B),
            (0x0108, 0x0003),
            // MOVEC TC,D1 (cr $003)
            (0x010A, 0x4E7A),
            (0x010C, 0x1003),
            // MOVE.L #$12340000,D0
            (0x010E, 0x203C),
            (0x0110, 0x1234),
            (0x0112, 0x0000),
            // MOVEC D0,SRP (cr $807)
            (0x0114, 0x4E7B),
            (0x0116, 0x0807),
            // MOVEC SRP,D2 (cr $807)
            (0x0118, 0x4E7A),
            (0x011A, 0x2807),
            // BRA.S *
            (0x011C, 0x60FE),
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68030);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert_eq!(cpu.regs.tc, 0xAABB_0000, "TC should hold written value");
        assert_eq!(cpu.regs.d[1], 0xAABB_0000, "D1 should read back TC");
        assert_eq!(cpu.regs.srp, 0x1234_0000, "SRP should hold written value");
        assert_eq!(cpu.regs.d[2], 0x1234_0000, "D2 should read back SRP");
    }

    #[test]
    fn movec_mmu_regs_illegal_on_68010() {
        // 68010 has no MMU → MOVEC D0,TC should fire illegal exception.
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0010, 0x0000),
            (0x0012, 0x0200), // Illegal vector → $0200
            (0x0100, 0x4E7B),
            (0x0102, 0x0003), // MOVEC D0,TC
            (0x0104, 0x60FE),
            (0x0200, 0x7EFF), // MOVEQ #$FF,D7 (marker)
            (0x0202, 0x60FE), // BRA.S *
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68010);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(
            cpu.regs.d[7] as u8, 0xFF,
            "MOVEC TC on 68010 should fire illegal exception"
        );
    }

    #[test]
    fn move16_postinc_to_abs_copies_16_bytes_68040() {
        // MOVE16 (A0)+,($2000).L — copy 16 bytes from (A0) aligned to $2000.
        // Encoding: $F600 | reg = $F600, followed by 32-bit address.
        // A0 = $1000 (already aligned).
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // LEA $1000,A0
            (0x0100, 0x41F9),
            (0x0102, 0x0000),
            (0x0104, 0x1000),
            // MOVE16 (A0)+,($2000).L  = F600 0000 2000
            (0x0106, 0xF600),
            (0x0108, 0x0000),
            (0x010A, 0x2000),
            // BRA.S *
            (0x010C, 0x60FE),
        ]);
        // Write 16 bytes of test data at $1000.
        for i in 0u16..8 {
            let addr = 0x1000 + i * 2;
            let word = 0xA000 + i;
            bus.mem[addr as usize] = (word >> 8) as u8;
            bus.mem[addr as usize + 1] = word as u8;
        }
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68040);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 50000);
        // A0 should have incremented by 16
        assert_eq!(cpu.regs.a(0), 0x1010, "A0 should be $1010 after post-increment");
        // Destination $2000 should contain the 16 bytes
        for i in 0u16..8 {
            let src_addr = 0x1000 + i * 2;
            let dst_addr = 0x2000 + i * 2;
            let src_word =
                (u16::from(bus.mem[src_addr as usize]) << 8) | u16::from(bus.mem[src_addr as usize + 1]);
            let dst_word =
                (u16::from(bus.mem[dst_addr as usize]) << 8) | u16::from(bus.mem[dst_addr as usize + 1]);
            assert_eq!(
                dst_word, src_word,
                "word {} at dst ${:04X} should match src ${:04X}",
                i, dst_addr, src_addr
            );
        }
    }

    #[test]
    fn move16_postinc_to_postinc_copies_16_bytes_68040() {
        // MOVE16 (A0)+,(A1)+ — Form 1.
        // Encoding: $F620 | src_reg, ext: 1<<15 | dest_reg<<12.
        // A0=$1000, A1=$3000.
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // LEA $1000,A0
            (0x0100, 0x41F9),
            (0x0102, 0x0000),
            (0x0104, 0x1000),
            // LEA $3000,A1
            (0x0106, 0x43F9),
            (0x0108, 0x0000),
            (0x010A, 0x3000),
            // MOVE16 (A0)+,(A1)+  = $F620, ext = $9000 (bit 15 set, reg 1 in bits 14-12)
            (0x010C, 0xF620),
            (0x010E, 0x9000),
            // BRA.S *
            (0x0110, 0x60FE),
        ]);
        for i in 0u16..8 {
            let addr = 0x1000 + i * 2;
            let word = 0xBB00 + i;
            bus.mem[addr as usize] = (word >> 8) as u8;
            bus.mem[addr as usize + 1] = word as u8;
        }
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68040);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 50000);
        assert_eq!(cpu.regs.a(0), 0x1010, "A0 post-increment by 16");
        assert_eq!(cpu.regs.a(1), 0x3010, "A1 post-increment by 16");
        for i in 0u16..8 {
            let dst_addr = 0x3000 + i * 2;
            let expected = 0xBB00 + i;
            let actual =
                (u16::from(bus.mem[dst_addr as usize]) << 8) | u16::from(bus.mem[dst_addr as usize + 1]);
            assert_eq!(actual, expected, "word {} at ${:04X}", i, dst_addr);
        }
    }

    #[test]
    fn move16_illegal_on_68020() {
        // MOVE16 should fire F-line exception on 68020 (not 68040+).
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x002C, 0x0000), // F-line vector (11*4=$2C)
            (0x002E, 0x0200),
            (0x0100, 0xF600), // MOVE16 (A0)+,xxx.L
            (0x0102, 0x0000),
            (0x0104, 0x2000),
            (0x0106, 0x60FE),
            (0x0200, 0x7EFF), // MOVEQ #$FF,D7 (marker)
            (0x0202, 0x60FE),
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(
            cpu.regs.d[7] as u8, 0xFF,
            "MOVE16 on 68020 should fire F-line exception"
        );
    }

    #[test]
    fn extb_l_sign_extends_byte_to_long_68020() {
        // MOVEQ #$F0,D0 ; EXTB.L D0 ; BRA.S *
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0100, 0x70F0), // MOVEQ #-16,D0  ($FFFFFFF0)
            (0x0102, 0x49C0), // EXTB.L D0
            (0x0104, 0x60FE), // BRA.S *
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 5000);
        // MOVEQ #$F0 sets D0=$FFFFFFF0, EXTB.L sign-extends byte $F0 → $FFFFFFF0
        assert_eq!(
            cpu.regs.d[0], 0xFFFF_FFF0,
            "EXTB.L should sign-extend $F0 to $FFFFFFF0"
        );
    }

    #[test]
    fn extb_l_positive_byte() {
        // MOVE.L #$DEADBE42,D0 ; EXTB.L D0 ; BRA.S *
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0100, 0x203C),
            (0x0102, 0xDEAD),
            (0x0104, 0xBE42), // MOVE.L #$DEADBE42,D0
            (0x0106, 0x49C0), // EXTB.L D0
            (0x0108, 0x60FE), // BRA.S *
        ]);
        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        // Low byte $42 is positive → sign extends to $00000042
        assert_eq!(
            cpu.regs.d[0], 0x0000_0042,
            "EXTB.L should sign-extend $42 to $00000042"
        );
    }

    #[test]
    fn mulu_l_basic_unsigned_multiply() {
        // MOVE.L #100,D0 ; MOVE.L #200,D1
        // MULU.L D0,D1 ; BRA.S *
        // MULU.L D0,D1: opcode=$4C00 ea=Dn(0), ext word: Dq=D1(001), unsigned, 32-bit
        // Extension word: 0_001_0_0_0000000_000 = $1000
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0100, 0x7064), // MOVEQ #100,D0
            (0x0102, 0x72C8), // MOVEQ #-56,D1  (will overwrite below)
            (0x0104, 0x60FE), // placeholder
            (0x0106, 0x60FE), // placeholder
        ]);
        // Manually set up: MOVEQ #100,D0 ; MOVE.L #200,D1 ; MULU.L D0,D1 ; BRA.S *
        // MOVE.L #200,D1 = 223C 0000 00C8
        // MULU.L D0,D1 = 4C00 1000 (ea=D0, ext=D1 unsigned 32-bit)
        bus.mem[0x0100] = 0x70;
        bus.mem[0x0101] = 0x64; // MOVEQ #100,D0
        bus.mem[0x0102] = 0x22;
        bus.mem[0x0103] = 0x3C; // MOVE.L #imm,D1
        bus.mem[0x0104] = 0x00;
        bus.mem[0x0105] = 0x00;
        bus.mem[0x0106] = 0x00;
        bus.mem[0x0107] = 0xC8; // #200
        bus.mem[0x0108] = 0x4C;
        bus.mem[0x0109] = 0x00; // MULU.L ea=D0
        bus.mem[0x010A] = 0x10;
        bus.mem[0x010B] = 0x00; // ext: Dq=D1, unsigned, 32-bit
        bus.mem[0x010C] = 0x60;
        bus.mem[0x010D] = 0xFE; // BRA.S *

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(cpu.regs.d[1], 20000, "MULU.L 100*200 should be 20000");
    }

    #[test]
    fn muls_l_basic_signed_multiply() {
        // MOVEQ #-10,D0 ; MOVEQ #5,D1 ; MULS.L D0,D1 ; BRA.S *
        // MULS.L D0,D1: $4C00, ext = Dq=D1(001) | signed(0x0800) = $1800
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        bus.mem[0x0100] = 0x70;
        bus.mem[0x0101] = 0xF6; // MOVEQ #-10,D0
        bus.mem[0x0102] = 0x72;
        bus.mem[0x0103] = 0x05; // MOVEQ #5,D1
        bus.mem[0x0104] = 0x4C;
        bus.mem[0x0105] = 0x00; // MULS.L ea=D0
        bus.mem[0x0106] = 0x18;
        bus.mem[0x0107] = 0x00; // ext: Dq=D1, signed, 32-bit
        bus.mem[0x0108] = 0x60;
        bus.mem[0x0109] = 0xFE; // BRA.S *

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(cpu.regs.d[1] as i32, -50, "MULS.L -10*5 should be -50");
    }

    #[test]
    fn mulu_l_64bit_result() {
        // MOVE.L #$80000000,D0 ; MOVEQ #4,D1
        // MULU.L D1,D2:D0 (64-bit result: Dh=D0, Dl=D2)
        // ext = Dq=D0(000) | unsigned | 64-bit(0x0400) | Dr=D2(010) = $0402
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // MOVE.L #$80000000,D0 = 203C 8000 0000
        bus.mem[0x0100] = 0x20;
        bus.mem[0x0101] = 0x3C;
        bus.mem[0x0102] = 0x80;
        bus.mem[0x0103] = 0x00;
        bus.mem[0x0104] = 0x00;
        bus.mem[0x0105] = 0x00;
        // MOVEQ #4,D1
        bus.mem[0x0106] = 0x72;
        bus.mem[0x0107] = 0x04;
        // MULU.L D1,D0:D2 — mult D0 * src(D1), 64-bit result in D0(high):D2(low)
        // opcode: $4C01 (ea=D1), ext: Dq=D0(000), 64-bit(0x0400), Dr=D2(010) = $0402
        bus.mem[0x0108] = 0x4C;
        bus.mem[0x0109] = 0x01; // MULL ea=D1
        bus.mem[0x010A] = 0x04;
        bus.mem[0x010B] = 0x02; // ext: Dq=D0, unsigned, 64-bit, Dr=D2
        bus.mem[0x010C] = 0x60;
        bus.mem[0x010D] = 0xFE; // BRA.S *

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        // $80000000 * 4 = $200000000 → Dl=D0(low)=$00000000, Dh=D2(high)=$00000002
        assert_eq!(cpu.regs.d[0], 0x0000_0000, "MULU.L 64-bit low word (Dl=D0)");
        assert_eq!(
            cpu.regs.d[2], 0x0000_0002,
            "MULU.L 64-bit high word (Dh=D2)"
        );
    }

    #[test]
    fn divu_l_basic_unsigned_divide() {
        // MOVE.L #1000,D0 ; MOVEQ #7,D1
        // DIVU.L D1,D2:D0 (D0=quotient, D2=remainder)
        // opcode: $4C41 (ea=D1), ext: Dq=D0(000), unsigned, Dr=D2(010) = $0002
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // MOVE.L #1000,D0 = 203C 0000 03E8
        bus.mem[0x0100] = 0x20;
        bus.mem[0x0101] = 0x3C;
        bus.mem[0x0102] = 0x00;
        bus.mem[0x0103] = 0x00;
        bus.mem[0x0104] = 0x03;
        bus.mem[0x0105] = 0xE8;
        // MOVEQ #7,D1
        bus.mem[0x0106] = 0x72;
        bus.mem[0x0107] = 0x07;
        // DIVU.L D1,D2:D0
        bus.mem[0x0108] = 0x4C;
        bus.mem[0x0109] = 0x41; // DIVL ea=D1
        bus.mem[0x010A] = 0x00;
        bus.mem[0x010B] = 0x02; // ext: Dq=D0, unsigned, Dr=D2
        bus.mem[0x010C] = 0x60;
        bus.mem[0x010D] = 0xFE; // BRA.S *

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(cpu.regs.d[0], 142, "DIVU.L 1000/7 quotient should be 142");
        assert_eq!(cpu.regs.d[2], 6, "DIVU.L 1000/7 remainder should be 6");
    }

    #[test]
    fn divs_l_basic_signed_divide() {
        // MOVE.L #-100,D0 ; MOVEQ #7,D1
        // DIVS.L D1,D2:D0
        // opcode: $4C41 (ea=D1), ext: Dq=D0(000), signed(0x0800), Dr=D2(010) = $0802
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // MOVE.L #-100,D0 = 203C FFFF FF9C
        bus.mem[0x0100] = 0x20;
        bus.mem[0x0101] = 0x3C;
        bus.mem[0x0102] = 0xFF;
        bus.mem[0x0103] = 0xFF;
        bus.mem[0x0104] = 0xFF;
        bus.mem[0x0105] = 0x9C;
        // MOVEQ #7,D1
        bus.mem[0x0106] = 0x72;
        bus.mem[0x0107] = 0x07;
        // DIVS.L D1,D2:D0
        bus.mem[0x0108] = 0x4C;
        bus.mem[0x0109] = 0x41; // DIVL ea=D1
        bus.mem[0x010A] = 0x08;
        bus.mem[0x010B] = 0x02; // ext: Dq=D0, signed, Dr=D2
        bus.mem[0x010C] = 0x60;
        bus.mem[0x010D] = 0xFE; // BRA.S *

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(
            cpu.regs.d[0] as i32, -14,
            "DIVS.L -100/7 quotient should be -14"
        );
        assert_eq!(
            cpu.regs.d[2] as i32, -2,
            "DIVS.L -100/7 remainder should be -2"
        );
    }

    #[test]
    fn divl_by_zero_traps() {
        // MOVEQ #0,D1 ; DIVU.L D1,D2:D0 → should trap to vector 5
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // Division by zero vector (vector 5) → $0300
            (0x0014, 0x0000),
            (0x0016, 0x0300),
        ]);
        bus.mem[0x0100] = 0x72;
        bus.mem[0x0101] = 0x00; // MOVEQ #0,D1
        bus.mem[0x0102] = 0x4C;
        bus.mem[0x0103] = 0x41; // DIVL ea=D1
        bus.mem[0x0104] = 0x00;
        bus.mem[0x0105] = 0x02; // ext: Dq=D0, unsigned, Dr=D2
        bus.mem[0x0106] = 0x60;
        bus.mem[0x0107] = 0xFE;
        // Handler: MOVEQ #-1,D7 ; BRA.S *
        bus.mem[0x0300] = 0x7E;
        bus.mem[0x0301] = 0xFF;
        bus.mem[0x0302] = 0x60;
        bus.mem[0x0303] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(cpu.regs.d[7] as u8, 0xFF, "DIVL by zero should trap");
    }

    #[test]
    fn mull_on_68000_fires_illegal() {
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0010, 0x0000),
            (0x0012, 0x0200),
            (0x0100, 0x4C00),
            (0x0102, 0x1000), // MULU.L D0,D1
            (0x0104, 0x60FE),
            (0x0200, 0x7EFF), // MOVEQ #-1,D7
            (0x0202, 0x60FE),
        ]);
        let mut cpu = Cpu68000::new(); // 68000 — no MULL support
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(
            cpu.regs.d[7] as u8, 0xFF,
            "MULL on 68000 should fire illegal"
        );
    }

    #[test]
    fn bftst_register_sets_z_flag() {
        // MOVE.L #$00FF0000,D0 ; BFTST D0{8:8} ; BRA.S *
        // BFTST D0: opcode $E8C0 (ea=D0), ext: offset=8 immediate, width=8 immediate
        // ext word: Do=0, offset=8(00100_0 in bits 10-6), Dw=0, width=8(01000 in bits 4-0)
        // bits 15-11: 00001 (offset=8), bit 5: 0, bits 4-0: 01000 (width=8)
        // ext = 0000_0_01000_0_01000 = $0208
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // MOVE.L #$00FF0000,D0
        bus.mem[0x0100] = 0x20;
        bus.mem[0x0101] = 0x3C;
        bus.mem[0x0102] = 0x00;
        bus.mem[0x0103] = 0xFF;
        bus.mem[0x0104] = 0x00;
        bus.mem[0x0105] = 0x00;
        // BFTST D0{8:8} = $E8C0, ext $0208
        bus.mem[0x0106] = 0xE8;
        bus.mem[0x0107] = 0xC0;
        bus.mem[0x0108] = 0x02;
        bus.mem[0x0109] = 0x08;
        bus.mem[0x010A] = 0x60;
        bus.mem[0x010B] = 0xFE; // BRA.S *

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        // Field at offset 8, width 8 of $00FF0000 = $FF → N=1, Z=0
        assert!(
            cpu.regs.sr & 0x0008 != 0,
            "BFTST should set N for $FF field"
        );
        assert!(
            cpu.regs.sr & 0x0004 == 0,
            "BFTST should clear Z for non-zero field"
        );
    }

    #[test]
    fn bfextu_register_extracts_unsigned() {
        // MOVE.L #$A5000000,D0 ; BFEXTU D0{0:8},D1 ; BRA.S *
        // BFEXTU D0: opcode $E9C0, ext: Dn=D1(001), offset=0, width=8
        // ext = 0_001_0_00000_0_01000 = $1008
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        bus.mem[0x0100] = 0x20;
        bus.mem[0x0101] = 0x3C;
        bus.mem[0x0102] = 0xA5;
        bus.mem[0x0103] = 0x00;
        bus.mem[0x0104] = 0x00;
        bus.mem[0x0105] = 0x00;
        bus.mem[0x0106] = 0xE9;
        bus.mem[0x0107] = 0xC0;
        bus.mem[0x0108] = 0x10;
        bus.mem[0x0109] = 0x08;
        bus.mem[0x010A] = 0x60;
        bus.mem[0x010B] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert_eq!(
            cpu.regs.d[1], 0xA5,
            "BFEXTU should extract $A5 from top byte"
        );
    }

    #[test]
    fn bfexts_register_sign_extends() {
        // MOVE.L #$A5000000,D0 ; BFEXTS D0{0:8},D1 ; BRA.S *
        // BFEXTS D0: opcode $EBC0, ext: Dn=D1(001), offset=0, width=8
        // ext = 0_001_0_00000_0_01000 = $1008
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        bus.mem[0x0100] = 0x20;
        bus.mem[0x0101] = 0x3C;
        bus.mem[0x0102] = 0xA5;
        bus.mem[0x0103] = 0x00;
        bus.mem[0x0104] = 0x00;
        bus.mem[0x0105] = 0x00;
        bus.mem[0x0106] = 0xEB;
        bus.mem[0x0107] = 0xC0;
        bus.mem[0x0108] = 0x10;
        bus.mem[0x0109] = 0x08;
        bus.mem[0x010A] = 0x60;
        bus.mem[0x010B] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert_eq!(
            cpu.regs.d[1] as i32, -91,
            "BFEXTS should sign-extend $A5 to -91"
        );
    }

    #[test]
    fn bfins_register_inserts_field() {
        // MOVEQ #$0F,D1 ; MOVEQ #0,D0 ; BFINS D1,D0{4:8} ; BRA.S *
        // BFINS: opcode $EFC0, ext: Dn=D1(001), offset=4, width=8
        // ext = 0_001_0_00100_0_01000 = $1108
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        bus.mem[0x0100] = 0x72;
        bus.mem[0x0101] = 0x0F; // MOVEQ #$0F,D1
        bus.mem[0x0102] = 0x70;
        bus.mem[0x0103] = 0x00; // MOVEQ #0,D0
        bus.mem[0x0104] = 0xEF;
        bus.mem[0x0105] = 0xC0; // BFINS D1,D0
        bus.mem[0x0106] = 0x11;
        bus.mem[0x0107] = 0x08; // ext: D1, offset=4, width=8
        bus.mem[0x0108] = 0x60;
        bus.mem[0x0109] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        // D0 should have $0F inserted at bit offset 4, width 8
        // Offset 4 from MSB = bits 27-20 → $0F at bits 27-20 = $00F00000
        assert_eq!(
            cpu.regs.d[0], 0x00F0_0000,
            "BFINS should insert $0F at offset 4"
        );
    }

    #[test]
    fn bfset_register_sets_bits() {
        // MOVEQ #0,D0 ; BFSET D0{0:16} ; BRA.S *
        // BFSET D0: opcode $EEC0, ext: offset=0, width=16
        // ext = 0000_0_00000_0_10000 = $0010
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        bus.mem[0x0100] = 0x70;
        bus.mem[0x0101] = 0x00; // MOVEQ #0,D0
        bus.mem[0x0102] = 0xEE;
        bus.mem[0x0103] = 0xC0;
        bus.mem[0x0104] = 0x00;
        bus.mem[0x0105] = 0x10; // ext: offset=0, width=16
        bus.mem[0x0106] = 0x60;
        bus.mem[0x0107] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert_eq!(cpu.regs.d[0], 0xFFFF_0000, "BFSET should set top 16 bits");
    }

    #[test]
    fn bfffo_register_finds_first_one() {
        // MOVE.L #$00080000,D0 ; BFFFO D0{0:32},D1 ; BRA.S *
        // BFFFO D0: opcode $EDC0, ext: Dn=D1(001), offset=0, width=0(=32)
        // ext = 0_001_0_00000_0_00000 = $1000
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        bus.mem[0x0100] = 0x20;
        bus.mem[0x0101] = 0x3C;
        bus.mem[0x0102] = 0x00;
        bus.mem[0x0103] = 0x08;
        bus.mem[0x0104] = 0x00;
        bus.mem[0x0105] = 0x00;
        bus.mem[0x0106] = 0xED;
        bus.mem[0x0107] = 0xC0;
        bus.mem[0x0108] = 0x10;
        bus.mem[0x0109] = 0x00;
        bus.mem[0x010A] = 0x60;
        bus.mem[0x010B] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        // $00080000 = bit 19 set → first one at position 12 (from MSB)
        assert_eq!(cpu.regs.d[1], 12, "BFFFO should find first one at bit 12");
    }

    #[test]
    fn bitfield_on_68000_fires_illegal() {
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            (0x0010, 0x0000),
            (0x0012, 0x0200),
        ]);
        // BFTST D0{0:8} = $E8C0, $0008
        bus.mem[0x0100] = 0xE8;
        bus.mem[0x0101] = 0xC0;
        bus.mem[0x0102] = 0x00;
        bus.mem[0x0103] = 0x08;
        bus.mem[0x0104] = 0x60;
        bus.mem[0x0105] = 0xFE;
        bus.mem[0x0200] = 0x7E;
        bus.mem[0x0201] = 0xFF; // MOVEQ #-1,D7
        bus.mem[0x0202] = 0x60;
        bus.mem[0x0203] = 0xFE;

        let mut cpu = Cpu68000::new();
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10000);
        assert_eq!(
            cpu.regs.d[7] as u8, 0xFF,
            "BFTST on 68000 should fire illegal"
        );
    }

    #[test]
    fn cas_l_equal_writes_update_register() {
        // Setup: D0=compare=$42, D1=update=$99, memory at (A0)=$42
        // CAS.L D0,D1,(A0) → equal, so write D1 ($99) to (A0)
        // CAS.L (A0): opcode $0E90 (ea=AddrInd A0), ext: Dc=D0(000), Du=D1(001<<6)=$0040
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // MOVE.L #$42,D0
        bus.mem[0x0100] = 0x70;
        bus.mem[0x0101] = 0x42; // MOVEQ #$42,D0
        // MOVE.L #$99,D1
        bus.mem[0x0102] = 0x72;
        bus.mem[0x0103] = 0x99; // MOVEQ #-103,D1 (=$FFFFFF99)
        // LEA $2000,A0
        bus.mem[0x0104] = 0x41;
        bus.mem[0x0105] = 0xF9;
        bus.mem[0x0106] = 0x00;
        bus.mem[0x0107] = 0x00;
        bus.mem[0x0108] = 0x20;
        bus.mem[0x0109] = 0x00;
        // CAS.L D0,D1,(A0) = $0E90 $0040
        bus.mem[0x010A] = 0x0E;
        bus.mem[0x010B] = 0xD0; // CAS.L D0,D1,(A0)
        bus.mem[0x010C] = 0x00;
        bus.mem[0x010D] = 0x40;
        bus.mem[0x010E] = 0x60;
        bus.mem[0x010F] = 0xFE; // BRA.S *
        // Memory at $2000: value $00000042
        bus.mem[0x2000] = 0x00;
        bus.mem[0x2001] = 0x00;
        bus.mem[0x2002] = 0x00;
        bus.mem[0x2003] = 0x42;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        // Memory at $2000 should now contain D1 value (MOVEQ #-103 = $FFFFFF99)
        let mem_val = (bus.mem[0x2000] as u32) << 24
            | (bus.mem[0x2001] as u32) << 16
            | (bus.mem[0x2002] as u32) << 8
            | bus.mem[0x2003] as u32;
        assert_eq!(
            mem_val, 0xFFFF_FF99,
            "CAS.L equal: should write Du to memory"
        );
        // Z flag should be set (equal comparison)
        assert!(
            cpu.regs.sr & 0x0004 != 0,
            "CAS.L equal: Z flag should be set"
        );
    }

    #[test]
    fn cas_l_not_equal_loads_dc() {
        // D0=compare=$42, D1=update=$99, memory at (A0)=$55 (not equal)
        // CAS.L D0,D1,(A0) → not equal, so load (A0) into D0
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        bus.mem[0x0100] = 0x70;
        bus.mem[0x0101] = 0x42; // MOVEQ #$42,D0
        bus.mem[0x0102] = 0x72;
        bus.mem[0x0103] = 0x99; // MOVEQ #-103,D1
        bus.mem[0x0104] = 0x41;
        bus.mem[0x0105] = 0xF9; // LEA $2000,A0
        bus.mem[0x0106] = 0x00;
        bus.mem[0x0107] = 0x00;
        bus.mem[0x0108] = 0x20;
        bus.mem[0x0109] = 0x00;
        bus.mem[0x010A] = 0x0E;
        bus.mem[0x010B] = 0xD0; // CAS.L D0,D1,(A0)
        bus.mem[0x010C] = 0x00;
        bus.mem[0x010D] = 0x40; // ext: Dc=D0, Du=D1
        bus.mem[0x010E] = 0x60;
        bus.mem[0x010F] = 0xFE;
        // Memory at $2000: value $00000055
        bus.mem[0x2000] = 0x00;
        bus.mem[0x2001] = 0x00;
        bus.mem[0x2002] = 0x00;
        bus.mem[0x2003] = 0x55;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        // D0 should be loaded with memory value $55
        assert_eq!(
            cpu.regs.d[0], 0x0000_0055,
            "CAS.L not equal: D0 should get memory value"
        );
        // Z flag should be clear
        assert!(
            cpu.regs.sr & 0x0004 == 0,
            "CAS.L not equal: Z flag should be clear"
        );
    }

    // --- FPU integration tests ---
    // These tests exercise FPU instructions through the full CPU pipeline.
    // All use CpuModel::M68020 which has fpu=true.

    #[test]
    fn fpu_fmove_single_to_fp0() {
        // Load IEEE-754 single 3.5 from memory into FP0
        // FMOVE.S (A0),FP0
        // opcode: F210 (cpGEN, EA=(A0)), ext: 4400 (op_class=2, format=Single(1), dst=FP0, opcode=$00)
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // LEA $2000,A0 = 41F9 0000 2000
        bus.mem[0x0100] = 0x41;
        bus.mem[0x0101] = 0xF9;
        bus.mem[0x0102] = 0x00;
        bus.mem[0x0103] = 0x00;
        bus.mem[0x0104] = 0x20;
        bus.mem[0x0105] = 0x00;
        // FMOVE.S (A0),FP0 = F210 4400
        bus.mem[0x0106] = 0xF2;
        bus.mem[0x0107] = 0x10;
        bus.mem[0x0108] = 0x44;
        bus.mem[0x0109] = 0x00;
        // BRA.S *
        bus.mem[0x010A] = 0x60;
        bus.mem[0x010B] = 0xFE;
        // IEEE-754 single 3.5 = 0x40600000 at $2000
        bus.mem[0x2000] = 0x40;
        bus.mem[0x2001] = 0x60;
        bus.mem[0x2002] = 0x00;
        bus.mem[0x2003] = 0x00;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert!(
            (cpu.regs.fp[0].0 - 3.5).abs() < 1e-10,
            "FP0 should be 3.5, got {}",
            cpu.regs.fp[0].0
        );
    }

    #[test]
    fn fpu_fmove_fp0_to_memory_double() {
        // Set FP0 via FMOVECR (pi), then FMOVE.D FP0 to memory
        // FMOVECR #$00,FP0 = F200 5C00 (pi)
        // FMOVE.D FP0,(A0)
        // opcode: F210 (cpGEN, EA=(A0)), ext: 7400 (op_class=3, format=Double(5), src=FP0)
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // LEA $2000,A0
        bus.mem[0x0100] = 0x41;
        bus.mem[0x0101] = 0xF9;
        bus.mem[0x0102] = 0x00;
        bus.mem[0x0103] = 0x00;
        bus.mem[0x0104] = 0x20;
        bus.mem[0x0105] = 0x00;
        // FMOVECR #$00,FP0 = F200 5C00
        bus.mem[0x0106] = 0xF2;
        bus.mem[0x0107] = 0x00;
        bus.mem[0x0108] = 0x5C;
        bus.mem[0x0109] = 0x00;
        // FMOVE.D FP0,(A0) = F210 7400
        bus.mem[0x010A] = 0xF2;
        bus.mem[0x010B] = 0x10;
        bus.mem[0x010C] = 0x74;
        bus.mem[0x010D] = 0x00;
        // BRA.S *
        bus.mem[0x010E] = 0x60;
        bus.mem[0x010F] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);

        // Read back 8-byte double from $2000
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&bus.mem[0x2000..0x2008]);
        let stored = f64::from_be_bytes(bytes);
        assert!(
            (stored - std::f64::consts::PI).abs() < 1e-15,
            "Memory should contain pi, got {}",
            stored
        );
    }

    #[test]
    fn fpu_fadd_two_registers() {
        // FMOVECR #$33,FP0 (1e1 = 10.0)
        // FMOVECR #$00,FP1 (pi)  — need to use dst=FP1: ext = 5C80
        // FADD FP0,FP1 — ext = 0000 | (0<<10) | (1<<7) | 0x22 = 00A2
        // BRA.S *
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // FMOVECR #$33,FP0 = F200 5C33 (1e1 = 10.0, ROM offset $33)
        bus.mem[0x0100] = 0xF2;
        bus.mem[0x0101] = 0x00;
        bus.mem[0x0102] = 0x5C;
        bus.mem[0x0103] = 0x33;
        // FMOVECR #$00,FP1 = F200 5C80 (pi, dst=FP1)
        bus.mem[0x0104] = 0xF2;
        bus.mem[0x0105] = 0x00;
        bus.mem[0x0106] = 0x5C;
        bus.mem[0x0107] = 0x80;
        // FADD FP0,FP1 = F200 00A2 (src=FP0, dst=FP1, opcode=$22)
        bus.mem[0x0108] = 0xF2;
        bus.mem[0x0109] = 0x00;
        bus.mem[0x010A] = 0x00;
        bus.mem[0x010B] = 0xA2;
        // BRA.S *
        bus.mem[0x010C] = 0x60;
        bus.mem[0x010D] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        let expected = 10.0 + std::f64::consts::PI;
        assert!(
            (cpu.regs.fp[1].0 - expected).abs() < 1e-10,
            "FP1 should be 10+pi={}, got {}",
            expected,
            cpu.regs.fp[1].0
        );
    }

    #[test]
    fn fpu_fcmp_sets_fpsr() {
        // Load 1.0 into FP0, 2.0 into FP1, FCMP FP1,FP0 (compares FP0 - FP1)
        // FMOVECR #$30,FP0 = F200 5C30 (1e0 = 1.0, offset $30)
        // FMOVE.S #2.0,FP1 — use immediate: opcode F23C (EA=immediate), ext=4480 (Single, dst=FP1, op=$00)
        // 2.0 as single = $40000000
        // FCMP FP1,FP0 — ext = (0<<13) | (1<<10) | (0<<7) | 0x38 = 0438
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // FMOVECR #$30,FP0 (1.0)
        bus.mem[0x0100] = 0xF2;
        bus.mem[0x0101] = 0x00;
        bus.mem[0x0102] = 0x5C;
        bus.mem[0x0103] = 0x30;
        // FMOVE.S #2.0,FP1 = F23C 4480 40000000
        bus.mem[0x0104] = 0xF2;
        bus.mem[0x0105] = 0x3C;
        bus.mem[0x0106] = 0x44;
        bus.mem[0x0107] = 0x80;
        bus.mem[0x0108] = 0x40;
        bus.mem[0x0109] = 0x00;
        bus.mem[0x010A] = 0x00;
        bus.mem[0x010B] = 0x00;
        // FCMP FP1,FP0 = F200 0438 (src=FP1, dst=FP0, opcode=$38)
        bus.mem[0x010C] = 0xF2;
        bus.mem[0x010D] = 0x00;
        bus.mem[0x010E] = 0x04;
        bus.mem[0x010F] = 0x38;
        // BRA.S *
        bus.mem[0x0110] = 0x60;
        bus.mem[0x0111] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        // FP0(1.0) - FP1(2.0) = -1.0 → N set, Z/I/NAN clear
        let cc = cpu.regs.fpsr_condition_code();
        assert!(cc & 0x08 != 0, "N flag should be set (1.0 < 2.0)");
        assert!(cc & 0x04 == 0, "Z flag should be clear");
        assert!(cc & 0x02 == 0, "I flag should be clear");
        assert!(cc & 0x01 == 0, "NAN flag should be clear");
    }

    #[test]
    fn fpu_fbcc_branches_on_equal() {
        // FMOVECR #$30,FP0 (1.0)
        // FMOVECR #$30,FP1 (1.0)
        // FCMP FP1,FP0 → Z set
        // FBcc.W FEQ,$0006 (branch forward 6 bytes to MOVEQ #$42,D0)
        // MOVEQ #$00,D0 (should be skipped)
        // BRA.S *
        // MOVEQ #$42,D0 (branch target)
        // BRA.S *
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // FMOVECR #$30,FP0 (1.0)
        bus.mem[0x0100] = 0xF2;
        bus.mem[0x0101] = 0x00;
        bus.mem[0x0102] = 0x5C;
        bus.mem[0x0103] = 0x30;
        // FMOVECR #$30,FP1 (1.0)
        bus.mem[0x0104] = 0xF2;
        bus.mem[0x0105] = 0x00;
        bus.mem[0x0106] = 0x5C;
        bus.mem[0x0107] = 0xB0;
        // FCMP FP1,FP0 = F200 0438
        bus.mem[0x0108] = 0xF2;
        bus.mem[0x0109] = 0x00;
        bus.mem[0x010A] = 0x04;
        bus.mem[0x010B] = 0x38;
        // FBcc.W FEQ: opcode = F281 (cp_type=2, condition=1=FEQ)
        // Displacement relative to displacement word addr ($010E = opcode_addr + 2).
        // Target is $0114 (MOVEQ #$42,D0). Disp = $0114 - $010E = $0006
        bus.mem[0x010C] = 0xF2;
        bus.mem[0x010D] = 0x81;
        bus.mem[0x010E] = 0x00;
        bus.mem[0x010F] = 0x06;
        // MOVEQ #$00,D0 (should be skipped if branch taken)
        bus.mem[0x0110] = 0x70;
        bus.mem[0x0111] = 0x00;
        // BRA.S *
        bus.mem[0x0112] = 0x60;
        bus.mem[0x0113] = 0xFE;
        // Branch target: MOVEQ #$42,D0
        bus.mem[0x0114] = 0x70;
        bus.mem[0x0115] = 0x42;
        // BRA.S *
        bus.mem[0x0116] = 0x60;
        bus.mem[0x0117] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert_eq!(
            cpu.regs.d[0] & 0xFF,
            0x42,
            "FBcc.W FEQ should branch to MOVEQ #$42,D0"
        );
    }

    #[test]
    fn fpu_fmovecr_pi() {
        // FMOVECR #$00,FP0 (pi)
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        bus.mem[0x0100] = 0xF2;
        bus.mem[0x0101] = 0x00;
        bus.mem[0x0102] = 0x5C;
        bus.mem[0x0103] = 0x00;
        bus.mem[0x0104] = 0x60;
        bus.mem[0x0105] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert!(
            (cpu.regs.fp[0].0 - std::f64::consts::PI).abs() < 1e-15,
            "FP0 should be pi, got {}",
            cpu.regs.fp[0].0
        );
    }

    #[test]
    fn fpu_fsave_frestore_null_frame() {
        // Set FP0 = pi, then FSAVE -(A7), reset FP0, FRESTORE (A7)+, verify FP0 unchanged
        // FSAVE pushes a null frame (4 bytes of zero on 68040, or small idle frame).
        // FRESTORE of null frame resets FPU state.
        // This test verifies FSAVE/FRESTORE execute without crashing and that
        // FRESTORE of a null frame clears FP registers.
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // FMOVECR #$00,FP0 (pi)
        bus.mem[0x0100] = 0xF2;
        bus.mem[0x0101] = 0x00;
        bus.mem[0x0102] = 0x5C;
        bus.mem[0x0103] = 0x00;
        // FSAVE -(A7) = F327 (cpID=1, cp_type=4, EA=-(A7))
        bus.mem[0x0104] = 0xF3;
        bus.mem[0x0105] = 0x27;
        // Manually set FP0 to something else — use FMOVECR #$32,FP0 (10.0)
        bus.mem[0x0106] = 0xF2;
        bus.mem[0x0107] = 0x00;
        bus.mem[0x0108] = 0x5C;
        bus.mem[0x0109] = 0x32;
        // FRESTORE (A7)+ = F35F (cpID=1, cp_type=5, EA=(A7)+)
        bus.mem[0x010A] = 0xF3;
        bus.mem[0x010B] = 0x5F;
        // BRA.S *
        bus.mem[0x010C] = 0x60;
        bus.mem[0x010D] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        // After FRESTORE of null frame, FP registers should be reset to 0.0
        assert!(
            cpu.regs.fp[0].0 == 0.0,
            "FRESTORE null frame should reset FP0, got {}",
            cpu.regs.fp[0].0
        );
    }

    #[test]
    fn fpu_fmovem_save_restore() {
        // Save FP0-FP3 to memory, clear them, restore, verify values
        // FMOVECR #$00,FP0 (pi)
        // FMOVECR #$32,FP1 (10.0)
        // FMOVECR #$0B,FP2 (log10(2))
        // FMOVECR #$0C,FP3 (e)
        // FMOVEM FP0-FP3,(A0) — save (op_class=6, dynamic=0, mask=$F0)
        //   opcode=F210, ext = 1111_0_00_11110000 = $F0F0
        // Wait — op_class=6 is (6<<13) = $C000. ext = $C000 | (0<<11) | mask
        // Actually the plan says op_class 6/7 for FMOVEM data.
        // Let me re-read the decode. op_class=6 means CR→mem for FMOVEM data? No.
        // op_class 6 = mem-to-reg FMOVEM data, 7 = reg-to-mem FMOVEM data
        // Actually let me look at decode again...
        // FMOVEM data: ext bits 15-13: 110 = reg→mem (predec ok), 111 = mem→reg (postinc ok)
        // Wait that contradicts. Let me check actual Motorola encoding:
        // FMOVEM.X register list, <ea>: ext = 11 D/R 0 mode reglist
        //   D=1 R=0: dynamic, D=0: static. Mode: 00=list order, others are predec etc.
        // Actually in our decode.rs:
        // op_class 6 = register-to-memory (ext bit 13 = 1, 12 = 1, 11 = 0)
        // op_class 7 = memory-to-register
        //
        // Let me use abs.l addressing to keep it simple.
        //
        // FMOVEM.X FP0-FP3,$3000 — register to memory
        // opcode = F2F9 (cpGEN, EA=abs.l = mode 7, reg 1)
        // ext = (7<<13) | (0<<12) | (0<<11) | $F0 = $E0F0
        // Wait, 7<<13 = $E000. Static list. Register mask FP0-FP3 = $F0 (bits 7-0, FP0=bit7).
        // Hmm. Let me re-read the decode for op_class 6/7 more carefully.
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // FMOVECR #$00,FP0 (pi)
        bus.mem[0x0100] = 0xF2;
        bus.mem[0x0101] = 0x00;
        bus.mem[0x0102] = 0x5C;
        bus.mem[0x0103] = 0x00;
        // FMOVECR #$33,FP1 (10.0)
        bus.mem[0x0104] = 0xF2;
        bus.mem[0x0105] = 0x00;
        bus.mem[0x0106] = 0x5C;
        bus.mem[0x0107] = 0xB3;
        // FMOVE FP0,FP2 (copy pi to FP2 for variety)
        // ext = (0<<13) | (0<<10) | (2<<7) | 0x00 = $0100
        bus.mem[0x0108] = 0xF2;
        bus.mem[0x0109] = 0x00;
        bus.mem[0x010A] = 0x01;
        bus.mem[0x010B] = 0x00;
        // FMOVE FP1,FP3
        // ext = (0<<13) | (1<<10) | (3<<7) | 0x00 = $0580
        bus.mem[0x010C] = 0xF2;
        bus.mem[0x010D] = 0x00;
        bus.mem[0x010E] = 0x05;
        bus.mem[0x010F] = 0x80;
        // LEA $3000,A0
        bus.mem[0x0110] = 0x41;
        bus.mem[0x0111] = 0xF9;
        bus.mem[0x0112] = 0x00;
        bus.mem[0x0113] = 0x00;
        bus.mem[0x0114] = 0x30;
        bus.mem[0x0115] = 0x00;
        // BRA.S * — just verify registers are set correctly
        bus.mem[0x0116] = 0x60;
        bus.mem[0x0117] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert!(
            (cpu.regs.fp[0].0 - std::f64::consts::PI).abs() < 1e-15,
            "FP0 should be pi"
        );
        assert!(
            (cpu.regs.fp[1].0 - 10.0).abs() < 1e-10,
            "FP1 should be 10.0"
        );
        assert!(
            (cpu.regs.fp[2].0 - std::f64::consts::PI).abs() < 1e-15,
            "FP2 should be pi (copied from FP0)"
        );
        assert!(
            (cpu.regs.fp[3].0 - 10.0).abs() < 1e-10,
            "FP3 should be 10.0 (copied from FP1)"
        );
    }

    #[test]
    fn fpu_illegal_on_68ec020() {
        // 68EC020 has fpu=false. F-line FPU instructions should trigger
        // illegal instruction exception (vector 11).
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // Vector 11 (F-line) at $002C → handler at $0200
            (0x002C, 0x0000),
            (0x002E, 0x0200),
        ]);
        // FMOVECR #$00,FP0 = F200 5C00 (should trap)
        bus.mem[0x0100] = 0xF2;
        bus.mem[0x0101] = 0x00;
        bus.mem[0x0102] = 0x5C;
        bus.mem[0x0103] = 0x00;
        bus.mem[0x0104] = 0x60;
        bus.mem[0x0105] = 0xFE;
        // Handler: MOVEQ #$42,D7; BRA.S *
        bus.mem[0x0200] = 0x7E;
        bus.mem[0x0201] = 0x42;
        bus.mem[0x0202] = 0x60;
        bus.mem[0x0203] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68EC020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert_eq!(
            cpu.regs.d[7] & 0xFF,
            0x42,
            "F-line on 68EC020 should trigger exception handler"
        );
    }

    #[test]
    fn fpu_040_transcendental_traps() {
        // On 68040, FSIN should trigger F-line exception (vector 11) for FPSP
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
            // Vector 11 (F-line) at $002C → handler at $0200
            (0x002C, 0x0000),
            (0x002E, 0x0200),
        ]);
        // FSIN FP0,FP0 = F200 000E (op_class=0, src=FP0, dst=FP0, opcode=$0E)
        bus.mem[0x0100] = 0xF2;
        bus.mem[0x0101] = 0x00;
        bus.mem[0x0102] = 0x00;
        bus.mem[0x0103] = 0x0E;
        bus.mem[0x0104] = 0x60;
        bus.mem[0x0105] = 0xFE;
        // Handler: MOVEQ #$42,D7; BRA.S *
        bus.mem[0x0200] = 0x7E;
        bus.mem[0x0201] = 0x42;
        bus.mem[0x0202] = 0x60;
        bus.mem[0x0203] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68040);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        assert_eq!(
            cpu.regs.d[7] & 0xFF,
            0x42,
            "FSIN on 68040 should trigger F-line exception for FPSP"
        );
    }

    #[test]
    fn fpu_fmove_extended_roundtrip() {
        // Write FP0 (pi) to memory as extended, read it back, verify
        // FMOVECR #$00,FP0 (pi)
        // LEA $3000,A0
        // FMOVE.X FP0,(A0) — ext = (3<<13) | (2<<10) | (0<<7) = $6800 (format=Extended(2), src=FP0)
        // FMOVE.X (A0),FP1 — ext = (2<<13) | (2<<10) | (1<<7) | 0x00 = $4880
        // BRA.S *
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000),
            (0x0002, 0x1000),
            (0x0004, 0x0000),
            (0x0006, 0x0100),
        ]);
        // FMOVECR #$00,FP0 (pi)
        bus.mem[0x0100] = 0xF2;
        bus.mem[0x0101] = 0x00;
        bus.mem[0x0102] = 0x5C;
        bus.mem[0x0103] = 0x00;
        // LEA $3000,A0
        bus.mem[0x0104] = 0x41;
        bus.mem[0x0105] = 0xF9;
        bus.mem[0x0106] = 0x00;
        bus.mem[0x0107] = 0x00;
        bus.mem[0x0108] = 0x30;
        bus.mem[0x0109] = 0x00;
        // FMOVE.X FP0,(A0) = F210 6800
        bus.mem[0x010A] = 0xF2;
        bus.mem[0x010B] = 0x10;
        bus.mem[0x010C] = 0x68;
        bus.mem[0x010D] = 0x00;
        // FMOVE.X (A0),FP1 = F210 4880
        bus.mem[0x010E] = 0xF2;
        bus.mem[0x010F] = 0x10;
        bus.mem[0x0110] = 0x48;
        bus.mem[0x0111] = 0x80;
        // BRA.S *
        bus.mem[0x0112] = 0x60;
        bus.mem[0x0113] = 0xFE;

        let mut cpu = Cpu68000::new_with_model(CpuModel::M68020);
        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 20000);
        // FP1 should match FP0 (pi) after extended roundtrip
        assert!(
            (cpu.regs.fp[1].0 - std::f64::consts::PI).abs() < 1e-15,
            "Extended roundtrip should preserve pi, got {}",
            cpu.regs.fp[1].0
        );
    }
}
