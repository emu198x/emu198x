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
use crate::bus::{
    BusStatus, DataPortSize, FunctionCode, TransferSize, dynamic_transfer_bytes,
    dynamic_write_data, extract_dynamic_bus_data, interrupt_acknowledge_address,
};
use crate::microcode::{MicroOp, MicroOpQueue};
use crate::registers::{Registers, StackBank};
use serde::{Deserialize, Serialize};

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
/// 68020+ Bcc.L / BSR.L / BRA.L: the high displacement word has been
/// stashed in `src_val`'s upper half and the low word prefetched into
/// `irc`; combine them into the 32-bit displacement and branch.
pub const TAG_LONG_BRANCH_LO: u8 = 118;

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
/// RTE: 68010+ Format/Vector word read. Inspects the Format nibble
/// to decide whether further bytes (Format $2 Instruction Address)
/// also need popping before resuming execution.
pub const TAG_RTE_READ_FORMAT: u8 = 91;
/// RTE: 68020+ Format $2 — high word of Instruction Address read.
pub const TAG_RTE_READ_FMT2_HI: u8 = 92;
/// RTE: 68020+ Format $2 — low word of Instruction Address read.
pub const TAG_RTE_READ_FMT2_LO: u8 = 93;
/// RTE: 68020+ Format $A short bus-fault — pop the remaining 24
/// bytes (= 12 words) above the F/V word. Each step reads one word
/// and advances the stack bank on which the frame began; step 12
/// finishes the RTE.
pub const TAG_RTE_READ_FMTA_STEP: u8 = 95;

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
/// Exception: 68010+ Format/Vector word just pushed; restore the
/// pending PC into `self.data` and continue with the regular PC
/// push. Only used when `variant_six_word_frame` is enabled.
pub const TAG_EXC_STACK_FORMAT: u8 = 43;
/// Exception: 68020+ Format `$2` — high word of the Instruction
/// Address has been pushed; queue the low word. Sits above the
/// Format/Vector word in the frame.
pub const TAG_EXC_STACK_INSTR_ADDR_HI: u8 = 44;
/// Exception: 68020+ Format `$2` — low word of the Instruction
/// Address pushed; restore `self.data` to the Format/Vector value
/// and continue with the format-word push.
pub const TAG_EXC_STACK_INSTR_ADDR_LO: u8 = 45;
/// Exception: 68010+ interrupt acknowledge completed; retain the
/// selected vector and push its Format/Vector offset.
pub const TAG_EXC_IACK_COMPLETE: u8 = 46;

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
/// 68020+ Format `$A` short bus-fault frame push step. Called
/// repeatedly with `ae_fmt_a_step` selecting which field to push;
/// step 12 hands off to `TAG_AE_FETCH_VECTOR`.
pub const TAG_AE_FMT_A_STEP: u8 = 94;

// 68020+ bit-field memory pipeline (Phase 5 / Stage M).
//
// Bit-field instructions on memory operands need a multi-step
// pipeline: resolve the EA (some modes need extension words), read
// up to 5 bytes covering the field span, do the field math in
// `bf_buf`, and — for R-M-W ops — write the modified bytes back.
/// BF memory: EA extension words just landed; finish address
/// calculation, set `bf_base_addr` / `bf_bytes_total`, queue first
/// `ReadByte`. Skipped for modes that resolve instantly (the
/// dispatch in `execute_bf` jumps directly to queueing reads in
/// that case).
pub const TAG_BF_MEM_EA_RESOLVE: u8 = 96;
/// BF memory: one byte just landed in `self.data`'s low byte;
/// shift it into `bf_buf` and either queue the next `ReadByte` (if
/// more bytes remain) or hand off to `TAG_BF_MEM_EXEC`.
pub const TAG_BF_MEM_READ: u8 = 97;
/// BF memory: all bytes are in `bf_buf`; dispatch on `bf_sub_op` to
/// extract / test / modify the field. Read-only ops (BFTST / EXTU /
/// EXTS / FFO) finish here; R-M-W ops (BFCHG / CLR / SET / INS)
/// queue the first writeback byte and hand off to
/// `TAG_BF_MEM_WRITE`.
pub const TAG_BF_MEM_EXEC: u8 = 98;
/// BF memory: one byte just written; if more remain queue the next
/// `WriteByte`, otherwise finish the instruction.
pub const TAG_BF_MEM_WRITE: u8 = 99;
/// BF memory: AbsLong second extension word — the high word has
/// been stashed in `bf_base_addr`; consume the low word and finish
/// the EA computation. Other ext-word modes (`d16(An)`,
/// `(d8,An,Xn)`, AbsShort, PcDisp, PcIndex) all complete in
/// `TAG_BF_MEM_EA_RESOLVE` in one shot.
pub const TAG_BF_MEM_EA_ABSLONG_LO: u8 = 100;

// 68020+ full-format extension word EA pipeline.
//
// The brief extension word (bit 8 = 0) resolves an indexed EA in one
// shot inside `calc_ea_start`. The full format (bit 8 = 1, 68020+
// only) can carry a base displacement (word/long), an outer
// displacement (word/long), and a memory-indirect long read — none
// of which the single prefetched IRC word can supply synchronously.
// These tags drive the follow-up reads, mirroring WinUAE's
// `get_disp_ea_020`. The synchronous sub-case (null base
// displacement, no memory indirect) still resolves in
// `calc_ea_start` and never reaches these tags.
/// Full format: consume one displacement word (base or outer) into
/// `ff_disp`; when the current displacement is complete, apply it and
/// advance (`ff_phase` selects base vs outer).
pub const TAG_EA_FF_STREAM: u8 = 110;
/// Full format: base displacement applied (or null); branch to the
/// outer-displacement / memory-indirect / finalize step. Also the
/// entry point when there is no base displacement but a memory
/// indirection is still required.
pub const TAG_EA_FF_AFTER_BD: u8 = 111;
/// Full format: the memory-indirect long read has landed in
/// `self.data`; apply post-indexing and the outer displacement, then
/// hand off to the source/destination data fetch.
pub const TAG_EA_FF_INDIRECT_DONE: u8 = 112;

/// 68020+ memory-source MUL.L / DIV.L: the long source operand has been
/// read into `self.data` (via the shared `TAG_FETCH_SRC_*` EA pipeline,
/// reclaimed by the variant continue hook). The handler runs the 64-bit
/// multiply / divide using `variant_ext_word` (the stashed spec word).
pub const TAG_V_MULDIV_MEM_EXEC: u8 = 113;

/// 68020+ CHK2 / CMP2: EA (bounds-tuple base) resolved; the lower bound
/// has been read into `self.data`. Stash it, then read the upper bound
/// at EA + size.
pub const TAG_V_CHK2_LOWER: u8 = 114;
/// 68020+ CHK2 / CMP2: both bounds are in (`src_val` = lower,
/// `self.data` = upper). Compare the register against them, set Z/C
/// (leaving N/V/X), and on CHK2 trap vector 6 if out of bounds.
pub const TAG_V_CHK2_UPPER: u8 = 115;

/// 68020+ CAS: the destination operand has been read from `[EA]` into
/// `self.data`. Compare it with Dc (subtract flags, X preserved); on
/// equal, queue the write of Du to `[EA]`; on not-equal, load the read
/// value into Dc.
pub const TAG_V_CAS_COMPARE: u8 = 116;
/// 68020+ CAS: the conditional write of Du to `[EA]` has completed —
/// end the instruction.
pub const TAG_V_CAS_WRITE_DONE: u8 = 117;

/// 68020+ CAS2: both extension words have been gathered into `src_val`;
/// read the first destination at `[Rn1]`.
pub const TAG_V_CAS2_GATHER: u8 = 119;
/// 68020+ CAS2: the first destination is in `dst_val`; read the second
/// destination at `[Rn2]`.
pub const TAG_V_CAS2_READ2: u8 = 120;
/// 68020+ CAS2: both destinations are read (`dst_val` = dest1,
/// `self.data` = dest2). Compare each against Dc1/Dc2; on a double match
/// queue the Du1 write, otherwise load both read values into Dc1/Dc2.
pub const TAG_V_CAS2_COMPUTE: u8 = 121;
/// 68020+ CAS2: Du1 has been written to `[Rn1]`; write Du2 to `[Rn2]`.
pub const TAG_V_CAS2_WRITE2: u8 = 122;
/// 68020+ CAS2: both writes have completed — end the instruction.
pub const TAG_V_CAS2_WRITE_DONE: u8 = 123;

/// 68881/2 FPU memory source operand: a byte of the operand has been
/// read; accumulate it and either queue the next byte or run the op.
pub const TAG_V_FP_MEM_READ: u8 = 124;
/// 68881/2 FPU memory source operand: all bytes are in `fp_mem_buf` —
/// decode the FP format, build the `floatx80`, and apply the opmode.
pub const TAG_V_FP_MEM_EXEC: u8 = 125;
/// 68881/2 FPU memory destination (FMOVE FPn → ea): a byte of the operand
/// has been written; step to the next byte or finish.
pub const TAG_V_FP_MEM_WRITE: u8 = 126;
/// 68881/2 FBcc.L: the low displacement word has been prefetched; combine
/// it with the stashed high word and resolve the branch.
pub const TAG_V_FBCC_L: u8 = 127;
/// 68881/2 FPU immediate source operand: an operand word has been
/// prefetched; accumulate it and either read the next or run the op.
pub const TAG_V_FP_IMM_READ: u8 = 128;
/// 68881/2 FMOVEM: a register's 12 bytes have transferred; process it and
/// either start the next register or finish.
pub const TAG_V_FMOVEM_STEP: u8 = 129;

/// 68881/2 FDBcc: the 16-bit displacement word has been fetched into `irc`;
/// test the condition, decrement the counter, and branch or fall through.
pub const TAG_V_FDBCC: u8 = 130;

/// 68881/2 FSAVE: a byte of the internal-state frame has been written;
/// step to the next byte or finish.
pub const TAG_V_FSAVE_WRITE: u8 = 131;
/// 68881/2 FRESTORE: a byte of the internal-state frame has been read.
/// The first four bytes are the frame id (version + size); once they are
/// in, dispatch on the frame version (null reset / idle / format error)
/// and consume any remaining frame bytes.
pub const TAG_V_FRESTORE_READ: u8 = 132;

/// CPU state machine state.
#[derive(Clone, Serialize, Deserialize)]
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

/// One logical MC68020/MC68030 data transfer that may span several bus cycles.
///
/// The processor keeps the original operand intact while SIZ reports the
/// bytes still outstanding. Each DSACK response can accept a different number
/// of bytes, so this state is serialized independently of the compatibility
/// [`State::BusCycle`] view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveBusTransfer {
    /// Original logical operand size.
    pub logical_size: TransferSize,
    /// Bytes still outstanding, encoded as the current SIZ pin value.
    pub remaining: TransferSize,
    /// Complete write operand, right-justified in big-endian byte order.
    pub write_data: u32,
    /// Sequential read bytes accepted by completed physical phases.
    pub read_data: u32,
}

/// ALU operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AluOp {
    Add,
    Sub,
    Cmp,
    And,
    Or,
    Eor,
}

/// Default for the FSAVE/FRESTORE frame buffer (arrays longer than 32 do
/// not implement `Default`, which `#[serde(skip)]` would otherwise need).
fn default_fp_frame() -> [u8; 60] {
    [0; 60]
}

/// Bit manipulation operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BitOp {
    Btst,
    Bset,
    Bclr,
    Bchg,
}

/// Direction of a word transfer rejected by address-error detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AddressErrorAccess {
    /// The rejected transfer would have read a word from memory.
    Read,
    /// The rejected transfer would have written a word to memory.
    Write,
}

/// Diagnostic observation of a word transfer rejected at an odd address.
///
/// Address errors are detected before the core enters [`State::BusCycle`], so
/// the machine layer never receives an ordinary bus request for the rejected
/// transfer. This record exposes the internal rejection boundary without
/// implying that address strobe was asserted or that an external transfer
/// completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AddressErrorObservation {
    /// Address produced by the core's abstract word-transfer micro-operation.
    pub requested_address: u32,
    /// Address recorded by the exception sequencer after instruction-specific
    /// word-step adjustment.
    pub frame_fault_address: u32,
    /// Read or write direction of the rejected word transfer.
    pub access: AddressErrorAccess,
    /// Function-code value selected for the attempted transfer.
    pub function_code: FunctionCode,
    /// Access-information word prepared for the group-0 frame.
    pub access_information: u16,
    /// Status register prepared for the group-0 frame.
    pub saved_sr: u16,
    /// Instruction-register word prepared for the group-0 frame.
    pub frame_ir: u16,
    /// Program-counter value prepared for the group-0 frame.
    pub frame_pc: u32,
}

/// Motorola 68000 CPU with reactive bus state machine.
///
/// Call [`tick`](Cpu68000::tick) every crystal clock cycle. The CPU only
/// acts on 4-clock boundaries (matching the 68000's minimum bus cycle).
#[derive(Clone, Serialize, Deserialize)]
pub struct Cpu68000 {
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

    // 68020+ full-format extension word EA, in-progress state.
    // Populated by `calc_ea_start` when it decodes a bit-8-set
    // extension word that needs follow-up reads; consumed by the
    // `TAG_EA_FF_*` handlers. See `cpu::TAG_EA_FF_STREAM`.
    /// The full-format extension word itself (BS/IS/BD/IS-field bits).
    pub(crate) ff_dp: u16,
    /// Running base: An (or PC), base-suppressed to 0 if BS set, plus
    /// the base displacement once read.
    pub(crate) ff_base: u32,
    /// Scaled, sign-extended index register value (0 if IS set).
    pub(crate) ff_regd: u32,
    /// Outer displacement (memory-indirect modes only).
    pub(crate) ff_outer: u32,
    /// Displacement-word accumulator (big-endian: hi word then lo).
    pub(crate) ff_disp: u32,
    /// Which displacement is being read: 0 = base, 1 = outer.
    pub(crate) ff_phase: u8,
    /// Displacement words still to read for the current phase.
    pub(crate) ff_stream_left: u8,
    /// Whether this EA feeds the source (true) or destination (false).
    pub(crate) ff_is_src: bool,

    /// ALU operation for the current instruction.
    pub alu_op: AluOp,
    /// Bit operation for the current instruction.
    pub bit_op: BitOp,
    /// Interrupt priority level being processed.
    pub target_ipl: u8,
    /// Count of hardware interrupts entered (PromoteIRC / tick-idle
    /// IPL-acceptance paths). Diagnostic only — never reset by the
    /// CPU. Tests use this instead of sampling `exc_vector`, which is
    /// transient continuation state and is cleared before handler
    /// execution.
    #[serde(skip)]
    pub interrupts_taken: u64,
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
    /// Exception vector already known to the current exception sequence.
    ///
    /// Group 1/2 exceptions set this before stacking. MC68010+ hardware
    /// interrupts set it after interrupt acknowledge so the selected vector
    /// supplies both the Format/Vector word and the handler fetch.
    /// When set, `TAG_EXC_STACK_SR` skips interrupt acknowledge.
    pub exc_vector: Option<u8>,
    /// Source operand value.
    pub src_val: u32,
    /// Destination operand value.
    pub dst_val: u32,

    // --- Address error state ---
    /// Fault address that triggered the address error.
    pub(crate) ae_fault_addr: u32,
    /// Access info word (IR bits [15:5] | R/W | I/N | function code).
    pub(crate) ae_access_info: u16,
    /// Saved SR at time of address error (before supervisor mode).
    pub(crate) ae_saved_sr: u16,
    /// True while processing a group-0 exception, including reset and the
    /// first handler-instruction fetch. A further bus or address fault halts.
    pub(crate) ae_in_progress: bool,
    /// True while the original MC68000 is processing a group-0 or group-1
    /// exception. This is the source of the address-error frame's I/N bit;
    /// group-2 exceptions remain part of instruction processing.
    #[serde(default)]
    pub(crate) group0_or_group1_processing: bool,
    /// True when the AE was caused by a FetchIRC (branch/jump to odd target).
    pub(crate) ae_from_fetch_irc: bool,
    /// DBcc: original Dn.w value before decrement, for undo on branch AE.
    pub(crate) dbcc_dn_undo: Option<(u8, u16)>,
    /// First instruction word prepared for the address-error frame.
    /// Kept separately because frame construction occurs after the rejected
    /// transfer has abandoned the instruction pipeline.
    pub(crate) ae_frame_ir: u16,
    /// Saved SR for the current MOVE write-AE compatibility policy:
    /// - `pre_move_sr`: full restore (for register src to (An)/(An)+, or
    ///   memory src to (An)/(An)+/abs.l with lo-word synthetic flags)
    /// - `pre_move_vc`: partial restore, V/C only (for register src to d16/d8+idx)
    ///
    /// These snapshots reproduce classified software-oracle outcomes; they do
    /// not establish the original processor's internal flag timing.
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
    /// UNLK: original stack pointer retained by the current compatibility path.
    /// UNLK sets A7 ← An before reading from the new (potentially odd) A7.
    /// Tuple: (selected stack bank, original pointer).
    pub(crate) sp_undo: Option<(StackBank, u32)>,
    /// Most recent internally rejected odd-address transfer.
    ///
    /// Diagnostic only. Consumers take the observation explicitly; snapshots
    /// omit it because it does not affect processor execution.
    #[serde(skip)]
    address_error_observation: Option<AddressErrorObservation>,

    // --- Bus error / group-0 exception state ---
    /// Vector number for the current group-0 exception (2=bus error, 3=address error).
    /// Used by TAG_AE_FETCH_VECTOR to read the correct vector.
    pub(crate) group0_vector: u8,

    // ── Pin-level bus interface ──────────────────────────────────
    //
    // The machine layer inspects these output pins between ticks to
    // determine what bus operation the CPU wants, performs it, and
    // writes the result to the input pins before the next tick.
    //
    // This replaces the archive's `M68kBus` trait. See
    // knowledge/decisions/cpu-bus-interface.md and amiga-port-plan.md.
    /// **Input:** Bus cycle result, written by the machine layer
    /// after performing the memory operation indicated by the
    /// output pins. Read by the tick function when in `BusCycle`
    /// state and `cycle_count >= min_bus`.
    pub bus_status: BusStatus,

    /// Serialized logical transfer state for MC68020/MC68030 dynamic sizing.
    ///
    /// `None` on the MC68000/MC68010 and for program-space prefetches.
    pub active_bus_transfer: Option<ActiveBusTransfer>,

    /// **Output:** current SIZ1/SIZ0 value decoded as bytes remaining.
    ///
    /// Meaningful while [`Self::active_bus_transfer`] is present.
    pub bus_transfer_size: TransferSize,

    /// **Output:** physical D31-D0 write-data image for the current phase.
    ///
    /// The MC68020/MC68030 duplicate operand bytes before knowing which
    /// responder width will terminate the cycle. Reads drive zero here.
    pub bus_data_out: u32,

    /// **Input:** Interrupt priority level (IPL0-IPL2), written by
    /// the machine layer from Paula's interrupt priority encoder.
    /// Sampled on every tick. Ordinary requests are checked at instruction
    /// boundaries and in the Stopped state; lower-to-level-7 transitions are
    /// retained in `level7_transition_pending` until one of those boundaries.
    pub ipl: u8,
    /// Most recent interrupt level sampled by [`Self::tick`].
    ///
    /// This is architectural history for detecting a lower-to-level-7
    /// transition, so it must survive save and restore.
    pub(crate) sampled_ipl: u8,
    /// A sampled lower-to-level-7 transition awaiting an instruction boundary.
    ///
    /// A boolean models the processor's pending condition rather than a queue:
    /// repeated transitions before service coalesce into one pending request.
    pub(crate) level7_transition_pending: bool,

    /// **Output:** True when the CPU wants to assert the RESET
    /// line on the bus (from a RESET instruction). The machine
    /// layer checks and clears this after each tick.
    pub reset_out: bool,

    /// Monotonic count of instruction starts observed by the prefetch
    /// pipeline. Useful for single-step harnesses, including branch-to-self
    /// cases where `instr_start_pc` does not change across an instruction.
    pub instruction_starts: u64,
    /// Opcode word captured when the current instruction started.
    pub(crate) opcode_at_start: u16,

    /// Variant-decode hook: gives a wrapping `Cpu68010` / `Cpu68020`
    /// / … a chance to handle opcodes the M68000 takes ILLEGAL on.
    ///
    /// Called by [`Self::try_variant_decode`] from each of the
    /// 68010+/68020+ ILLEGAL-trap arms in
    /// [`Self::decode_and_execute`]. Returning `true` means the hook
    /// fully handled the opcode (advanced PC, set flags, queued any
    /// follow-up micro-ops); the 68000 then skips its ILLEGAL trap.
    /// Returning `false` (or leaving the hook `None`) preserves the
    /// pure-68000 behaviour exactly. This is the only extension
    /// point that the 68000 exposes to its variants — the family
    /// crates (`motorola-68010`, `motorola-68020`, …) install hooks
    /// in their wrapper's `new()`. Pinned to `fn` rather than
    /// `Box<dyn Fn>` so it stays trivially `Copy` / `Clone`, and
    /// `#[serde(skip)]` because function pointers don't serialise
    /// — variant wrappers re-install the hook on deserialise.
    #[serde(skip)]
    pub variant_decode_hook: Option<fn(&mut Cpu68000, u16) -> bool>,

    /// 68020+ brief-extension-word scaled index (×1 / ×2 / ×4 / ×8).
    ///
    /// On the 68000 / 68010 bits 10-9 of the brief extension word
    /// are "don't care" — the EA path always uses scale = 1.
    /// On the 68020+ they encode `1 << bits` so the index can be
    /// `Xn.SIZE * 1 / 2 / 4 / 8`. The flag is consulted by
    /// `calc_ea_start` for `AddrIndIndex` and `PcIndex` modes; the
    /// 68020 wrapper flips it to `true` in `new()`.
    ///
    /// `#[serde(skip)]` with a `default = "false"` deserialiser:
    /// snapshots restore the inner core and the variant wrapper
    /// re-applies the flag on the next construction. Variant
    /// behaviour bits live on the inner core (rather than the
    /// wrappers) so the shared EA / SR / exception code can consult
    /// them without going through a generic trait.
    #[serde(skip)]
    pub variant_scaled_index: bool,

    /// 68010+ six-word exception frame.
    ///
    /// The 68000 pushes `[PC, SR]` (six bytes) on a group-1/2
    /// exception. The 68010+ pushes `[PC, SR, Format/Vector]` (eight
    /// bytes) — the format word records the exception type and
    /// vector offset so `RTE` can pop the right frame size. The
    /// 68010 / 68020 wrappers set this flag in `new()`; the
    /// exception path (`begin_group1_exception` + the
    /// `TAG_EXC_STACK_FORMAT` continuation arm) consults it.
    #[serde(skip)]
    pub variant_six_word_frame: bool,

    /// 68020+ Format `$2` exception frame for instruction-error
    /// traps.
    ///
    /// On the 68010 every group-1/2 exception uses the short
    /// Format `$0` 8-byte frame. The 68020+ promotes
    /// CHK / CHK2 / divide-by-zero / TRAPV / TRAPcc / Trace
    /// (vectors 5, 6, 7, 9) to a 12-byte Format `$2` frame that
    /// adds an "Instruction Address" long at the top — the address
    /// of the instruction that took the trap (= `instr_start_pc`).
    ///
    /// Consulted by `begin_group1_exception`; the 68020 wrapper
    /// enables it. The 68010 leaves it false (Format `$0` for
    /// everything). PRM § 8.6.3.
    #[serde(skip)]
    pub variant_format2_vectors: bool,

    /// Musashi-style "undefined V" for ABCD / SBCD / NBCD.
    ///
    /// PRM defines V as undefined for these. SingleStepTests and
    /// Musashi pick different concrete values for V; both fit
    /// "undefined" but disagree instruction-by-instruction. Our
    /// reference oracles split:
    /// the m68k-test-gen 68010 / 68020 corpora are Musashi-driven
    /// (so they expect Musashi V), while the upstream 68000 corpus
    /// is implementation-generated and expects SingleStepTests V.
    ///
    /// `false` (default) → SingleStepTests-compatible V via the
    /// legacy `bcd_add_realhw` helpers.
    /// `true` → Musashi V via `bcd_add_musashi` etc.
    /// The 68010 / 68020 wrappers set it `true` in `new()`.
    #[serde(skip)]
    pub variant_musashi_bcd_v: bool,

    /// Musashi-style overflow flag handling on 16-bit `DIVU.W` /
    /// `DIVS.W` (and 32-bit `DIVL` already follows this path).
    ///
    /// PRM § 6.2.7: "on overflow N undefined, Z undefined, C
    /// cleared, V set". SingleStepTests clears C, sets V and
    /// preserves N/Z; Musashi preserves *all* flags except V
    /// (which is set). The same suite-vs-Musashi split as the BCD
    /// V flag applies.
    ///
    /// `false` (default) → SingleStepTests: clear C, set V,
    /// preserve N/Z/X.
    /// `true` → Musashi: set V, preserve everything else.
    /// The 68010 / 68020 wrappers set it `true` in `new()`.
    #[serde(skip)]
    pub variant_musashi_div_overflow: bool,

    /// 68020+ extended SR write mask (allows the M-flag, bit 12).
    ///
    /// The 68000 / 68010 SR mask is `$A71F` (T1, S, IPL[2:0], CCR).
    /// The 68020 widens it to `$F71F`, adding the master/interrupt
    /// stack bit. Consulted by every code path that writes to SR
    /// from a 16-bit value (`MOVE-to-SR`, `ORI/ANDI/EORI-to-SR`,
    /// `RTE`'s SR pop). The 68020 wrapper enables it.
    #[serde(skip)]
    pub variant_extended_sr_writes: bool,

    /// 68020+ unaligned data access support.
    ///
    /// The 68000 / 68010 reject odd-address word and long-word transfers.
    /// The 68020+ may read or write data at any byte address; only an
    /// instruction prefetch from an odd address raises an address error.
    /// The 68020 wrapper enables this non-serialized capability and the
    /// shared address-error gate keeps applying the original rule otherwise.
    #[serde(skip)]
    pub variant_unaligned_data_access: bool,

    /// MC68020/MC68030 SIZ and DSACK dynamic bus sizing.
    ///
    /// The capability is reinstalled by the variant wrapper after
    /// deserialization. The MC68040 has a different bus protocol and disables
    /// this inherited MC68020/MC68030 interface.
    #[serde(skip)]
    pub variant_dynamic_bus_sizing: bool,

    /// 68020+ Format `$A` group-0 exception frame.
    ///
    /// The 68000 / 68010 push a 14-byte frame for bus error (vec 2)
    /// and address error (vec 3): access info, fault address, IR,
    /// SR, PC. The 68020 promotes group-0 to a 32-byte short
    /// bus-fault frame (Format `$A`) with a different layout:
    /// SR, PC, F/V word, then internal pipeline state. KS 3.1's
    /// vec-2/3 handler at `$F80B0E` reads the frame at offsets
    /// consistent with Format `$A`; with our 14-byte frame, the
    /// handler reads the wrong fields and routes through code
    /// paths that don't work on 68020. PRM § 8.6.4.
    #[serde(skip)]
    pub variant_format_a_group0: bool,

    /// Minimum bus-cycle length in CPU clocks. The 68000/68010 use a
    /// 4-clock minimum (S0–S7); the 68020/68030 use 3. Chip RAM still
    /// stretches via `BusStatus::Wait` regardless, so this only sets
    /// the fast-memory access floor. Wrappers set it from their
    /// `TimingClass`; see the 68k cycle-timing plan (#41/#110/#111).
    pub variant_min_bus_clocks: u8,

    /// When set, the barrel-shifter instructions (LSL/LSR/ASL/ASR/
    /// ROL/ROR/ROXL/ROXR) cost a constant internal delay regardless of
    /// shift count — the 68020+ behaviour — instead of the 68000's
    /// `2 + 2·count` clocks.
    pub variant_constant_shift_timing: bool,

    /// Variant-specific writable-bit mask for the cache control register.
    ///
    /// The MC68020 wrapper installs `$0000_000F`; the MC68030 widens the
    /// implemented register to `$0000_3F1F`. This is a variant binding,
    /// not architectural state, so wrappers reinstall it after deserialize.
    #[serde(skip)]
    pub variant_cacr_write_mask: u32,

    /// CACR action bits that always read as zero on this CPU variant.
    ///
    /// The current MC68020 compatibility model preserves its four written
    /// bits. The MC68030 marks CI/CEI/CD/CED (`$0000_0C0C`) as momentary
    /// clear commands, leaving only persistent controls in `regs.cacr`.
    #[serde(skip)]
    pub variant_cacr_read_zero_mask: u32,

    /// External cache-disable input, expressed as an asserted logical level.
    ///
    /// The MC68030's active-low CDIS pin suppresses instruction-cache hits
    /// and fills without invalidating entries. This field is combinational
    /// machine input rather than CPU state, so it is deliberately not
    /// serialized; a machine that exposes CDIS must drive it before each CPU
    /// edge after construction or restore.
    #[serde(skip)]
    pub variant_cache_disable_asserted: bool,

    /// Scratch slot for a variant instruction's primary extension word,
    /// stashed across a memory-operand fetch. Used by 68020 memory-source
    /// MUL.L / DIV.L (and future mem-operand instructions): the spec word
    /// is read at decode, then the source operand is fetched through the
    /// shared EA pipeline; the continuation re-reads the spec from here.
    pub variant_ext_word: u16,

    /// When set, indexed and computed effective-address calculations
    /// cost the 68020's clocks instead of the 68000 model's flat 2-clock
    /// approximation. The figures are the M68020UM § 8.2.3 "Calculate
    /// Effective Address" Cache-Case column — the no-overlap case our
    /// sequential engine targets (the manual's Best Case assumes
    /// cross-instruction pipeline overlap, which this model does not
    /// represent): brief `(d8,An,Xn)`/`(d8,PC,Xn)` = 4, full-format
    /// base+index `(An,Xn*scale)` = 6, predecrement `-(An)` = 2.
    /// Default `false` (68000/68010 keep the flat 2); the 68020+ wrapper
    /// sets it `true`. Timing only — the computed address is identical.
    /// See the 68k cycle-timing plan (#41) Phase 4.
    pub variant_um_ea_calc_timing: bool,

    /// When set, the `Bcc`/`BSR`/`BRA` family decodes the 68020+ 32-bit
    /// displacement form (8-bit displacement field == `$FF`). On the
    /// 68000/68010 that encoding is a normal 8-bit branch with
    /// displacement −1, so this must be a core flag, not a variant
    /// decode-hook fallback (the opcode is never illegal). Default
    /// `false`; the 68020+ wrapper sets it `true`.
    #[serde(skip)]
    pub variant_long_branch: bool,

    /// When set, F-line (`$Fxxx`) coprocessor-ID-1 opcodes are decoded as
    /// 68881/68882 FPU instructions instead of taking the vector-11
    /// F-line emulator trap. The FPU is an *attached coprocessor*, not a
    /// CPU feature: a 68EC020 (A1200/CD32) has no coprocessor interface,
    /// and a full 68020 with no 68881 fitted also traps F-line (the
    /// handler can soft-emulate). So this is gated per *machine*, not per
    /// CPU model — default `false` (trap), set `true` by machines with an
    /// FPU. Not `#[serde(skip)]`: it's machine configuration that must
    /// survive save/load (a plain bool, unlike the hook function
    /// pointers the other flags carry).
    pub variant_fpu_present: bool,

    /// FPU coprocessor model: `false` = MC68881, `true` = MC68882. The two
    /// share the same `fpu_version` ($1F) and behave identically for the
    /// arithmetic core; only the FSAVE/FRESTORE internal-state frame size
    /// differs (68881 = 28 bytes, 68882 = 60 bytes). Machine configuration —
    /// not `#[serde(skip)]`. Default `false` (68881).
    pub variant_fpu_is_68882: bool,

    /// FPU internal state, as reported by FSAVE / consumed by FRESTORE:
    /// `0` = null (reset — the power-on / post-`fpu_null` state), `1` = idle
    /// (any 68881/2 FP instruction has executed since reset). Mirrors WinUAE
    /// `regs.fpu_state`. Machine state that must survive save/load, so not
    /// `#[serde(skip)]`. Default `0` (null).
    pub variant_fpu_state: u8,

    /// Step counter for the 13-step Format `$A` push sequence.
    /// Consulted by `TAG_AE_FMT_A_STEP`.
    pub(crate) ae_fmt_a_step: u8,

    /// Step counter for the 12-step Format `$A` RTE tail pop. Each step
    /// reads one word after the common eight-byte prefix; step 12 finishes.
    pub(crate) rte_fmta_step: u8,

    /// Frame PC saved at group-0 entry for use by Format `$A`
    /// pushes. The 68000 path stores it in `self.data` and pushes
    /// immediately; Format `$A` needs the value preserved across
    /// many intermediate pushes.
    pub(crate) ae_frame_pc: u32,

    /// Pending PC value during the 68010+ exception frame push.
    ///
    /// When `variant_six_word_frame` is set, formatted synchronous
    /// exceptions and acknowledged interrupts use `self.data` for the
    /// Format/Vector word before the PC push begins. The PC is stashed
    /// here until `TAG_EXC_STACK_FORMAT` restores it to `self.data`.
    pub(crate) exc_pending_pc: u32,

    /// Whether the normal frame for a master-mode interrupt is still being
    /// written to MSP.
    ///
    /// Once that frame completes, the exception sequencer clears live M and
    /// writes a Format-$1 throwaway frame to ISP. This phase is serialized so
    /// a snapshot taken during either frame resumes on the same stack bank.
    pub(crate) exc_master_interrupt_pending: bool,

    /// Saved SR read from the current RTE frame.
    ///
    /// RTE defers applying it until the complete frame has been consumed so
    /// every frame read remains a supervisor-data access. Format-$1 applies
    /// this intermediate SR before restarting RTE on MSP.
    pub(crate) rte_saved_sr: u16,
    /// Saved PC read from the current RTE frame.
    pub(crate) rte_saved_pc: u32,
    /// Stack-pointer bank from which the current RTE frame is being consumed.
    /// Capturing all three possibilities matters because a Format-$1 restart
    /// may select USP, ISP or MSP. It also prevents an SR restore from
    /// redirecting pointer updates to another bank.
    pub(crate) rte_stack_bank: StackBank,

    // ── 68020+ bit-field memory pipeline scratch ──────────────────
    //
    // Stage M (Phase 5): bit-field instructions (BFTST/EXTU/CHG/EXTS/
    // CLR/FFO/SET/INS) on memory operands need a multi-step pipeline:
    // compute base byte address from EA + signed(offset/8); read up
    // to 5 bytes; do the field math; for R-M-W ops write the modified
    // bytes back. The Dn-mode fast path in the 68020 hook stays
    // synchronous; only memory modes use these scratch fields.
    //
    // All `#[serde(skip)]` because BF execution is mid-instruction
    // transient state — a save state taken outside a BF instruction
    // doesn't need to preserve any of these, and one taken mid-BF
    // would need every other mid-instruction field too.
    /// Packed read/write buffer for the memory operand. Up to 5 bytes
    /// (40 bits) starting at the byte containing the field MSB. Bytes
    /// are packed MSB-first: the first byte read occupies bits 63-56,
    /// the second 55-48, and so on.
    #[serde(skip)]
    pub bf_buf: u64,
    /// Base byte address of the memory operand (= EA + signed(offset/8))
    /// where the first byte of the field resides. Preserved across the
    /// read chain so R-M-W writeback can step forward from the same
    /// starting address.
    #[serde(skip)]
    pub bf_base_addr: u32,
    /// Sub-op (0..=7) matching the BF opcode encoding:
    /// 0=BFTST 1=BFEXTU 2=BFCHG 3=BFEXTS 4=BFCLR 5=BFFFO 6=BFSET 7=BFINS.
    #[serde(skip)]
    pub bf_sub_op: u8,
    /// Source / destination data register for ops that need one
    /// (BFEXTU / BFEXTS / BFFFO write Dr; BFINS reads Dr at start).
    #[serde(skip)]
    pub bf_dr: u8,
    /// Effective field width, 1..=32.
    #[serde(skip)]
    pub bf_width: u8,
    /// Bit offset within the first byte of `bf_buf` where the field
    /// MSB sits (0..=7, MSB-numbered). For a byte-aligned field this
    /// is 0; for a 1-bit-shifted field this is 1; etc.
    #[serde(skip)]
    pub bf_bit_offset: u8,
    /// Total bytes the field spans (1..=5). Determines how many
    /// `ReadByte` ops the read chain queues.
    #[serde(skip)]
    pub bf_bytes_total: u8,
    /// Bytes already read into `bf_buf` (0..=`bf_bytes_total`).
    /// Doubles as the write-chain index during the R-M-W writeback.
    #[serde(skip)]
    pub bf_bytes_done: u8,
    /// For BFINS: Dr's value at instruction start (captured before
    /// the read chain runs, in case the field math would otherwise
    /// see a value mutated by some intermediate step).
    #[serde(skip)]
    pub bf_source_val: u32,
    /// EA mode (0..=7) for the memory operand, stashed across the
    /// extension-word resolve gap when the EA needs further words
    /// (`d16(An)`, AbsShort/Long, `(d8,An,Xn)`, PC-relative). The
    /// instant modes (`(An)` / `(An)+` / `-(An)`) skip the resolve
    /// step and don't consult this field.
    #[serde(skip)]
    pub bf_ea_mode: u8,
    /// EA register number (0..=7) for An-based modes. Meaningless
    /// (and unread) for AbsShort / AbsLong / PC-relative.
    #[serde(skip)]
    pub bf_ea_reg: u8,
    /// Signed byte displacement derived from `offset / 8` in the BF
    /// extension word. Stashed across the EA-resolve gap because the
    /// BF ext word is consumed before the EA can be computed; reusing
    /// `bf_base_addr` would conflict with the resolve handler's final
    /// write of the base byte address.
    #[serde(skip)]
    pub bf_byte_disp: i32,

    // ─── 68881/2 FPU memory-operand pipeline (mid-instruction) ───
    // Like the bit-field memory state above, these are transient
    // working registers for a memory source operand and are not
    // preserved across snapshots.
    /// Operand bytes accumulated big-endian (first byte read in the
    /// most-significant position). Holds up to 12 bytes (extended).
    #[serde(skip)]
    pub fp_mem_buf: u128,
    /// Total bytes the operand spans (1/2/4/8/12, per the FP format).
    #[serde(skip)]
    pub fp_mem_bytes_total: u8,
    /// Bytes already read into `fp_mem_buf`.
    #[serde(skip)]
    pub fp_mem_bytes_done: u8,
    /// FP source-format specifier (0=Long 1=Single 2=Extended 4=Word
    /// 5=Double 6=Byte) from the extension word's bits 12-10.
    #[serde(skip)]
    pub fp_mem_format: u8,
    /// The FP opmode (bits 6-0 of the extension word) to apply once the
    /// operand is loaded.
    #[serde(skip)]
    pub fp_mem_opmode: u8,
    /// Destination Fpn (extension-word bits 9-7).
    #[serde(skip)]
    pub fp_mem_dst: u8,
    /// Rounding precision (80/64/32) to apply once the operand is loaded —
    /// the FSxxx/FDxxx opmode prefix override, or the FPCR precision field.
    #[serde(skip)]
    pub fp_mem_precision: i32,
    /// Set while an FPU memory operand is using the core's EA-resolution
    /// machinery (`calc_ea_start`) for the non-auto-increment addressing
    /// modes. Lets the 68020 continue hook recognise the shared
    /// `TAG_FETCH_SRC_DATA` tag as ours and start the operand read from
    /// the resolved `addr` instead of running the core's data fetch.
    #[serde(skip)]
    pub fp_mem_pending: bool,
    /// When an FPU memory operand is resolved via `calc_ea_start`, selects
    /// the direction: `true` = store (write the operand from `fp_mem_buf`),
    /// `false` = load (read into `fp_mem_buf`).
    #[serde(skip)]
    pub fp_mem_store: bool,

    // ─── 68881/2 FMOVEM register-list transfer (mid-instruction) ───
    /// True while a FMOVEM is in flight (redirects the 12-byte transfer
    /// completion to the FMOVEM controller instead of the FMOVE exec).
    #[serde(skip)]
    pub fp_movem_active: bool,
    /// FMOVEM direction: `true` = registers → memory (predecrement),
    /// `false` = memory → registers (postincrement).
    #[serde(skip)]
    pub fp_movem_store: bool,
    /// Remaining register-list bits still to transfer (cleared as each is
    /// processed, lowest bit first).
    #[serde(skip)]
    pub fp_movem_list: u8,
    /// The register index (0..7) of the in-flight transfer; `0xFF` before
    /// the first register.
    #[serde(skip)]
    pub fp_movem_cur: u8,
    /// Working address pointer, stepped by 12 per register and written
    /// back to the An register at the end.
    #[serde(skip)]
    pub fp_movem_an: u32,
    /// The An register the pointer is written back to.
    #[serde(skip)]
    pub fp_movem_areg: u8,

    // ─── 68881/2 FSAVE / FRESTORE internal-state frame (mid-instruction) ───
    /// The internal-state frame being streamed byte-at-a-time. Large enough
    /// for the 68882's 60-byte idle frame; only the first four bytes (the
    /// frame id) are inspected on FRESTORE.
    #[serde(skip, default = "default_fp_frame")]
    pub fp_frame: [u8; 60],
    /// Total bytes the frame spans (4 for a null frame, 28 for a 68881 idle
    /// frame, 60 for a 68882 idle frame). On FRESTORE this starts at 4 (the
    /// frame id) and is revised once the frame version is known.
    #[serde(skip)]
    pub fp_frame_total: u8,
    /// Bytes already streamed (read or written).
    #[serde(skip)]
    pub fp_frame_done: u8,
    /// Base address the frame stream started from, used to write back the
    /// postincrement pointer on FRESTORE.
    #[serde(skip)]
    pub fp_frame_an: u32,
    /// The An register a FRESTORE postincrement pointer is written back to.
    #[serde(skip)]
    pub fp_frame_areg: u8,
    /// FRESTORE used `(An)+` — write the consumed-byte count back to An at
    /// the end (only on the non-fault path).
    #[serde(skip)]
    pub fp_frame_postinc: bool,

    /// A pending 68881/2 arithmetic-exception trap vector (48-54), armed by
    /// an FP instruction that raised an *enabled* exception. Delivered at
    /// the instruction boundary (the `PromoteIRC` step) so the stacked PC
    /// points at the following instruction — the 68881 post-instruction
    /// model. `None` when no FP trap is pending. Transient mid-dispatch
    /// state, so `#[serde(skip)]`.
    #[serde(skip)]
    pub fp_exc_pending: Option<u8>,
    /// Set while an FSAVE/FRESTORE frame is using the core's EA-resolution
    /// machinery (`calc_ea_start`) for the control addressing modes. Lets the
    /// 68020 continue hook recognise the shared `TAG_FETCH_SRC_DATA` tag as
    /// the frame stream and start it from the resolved `addr`.
    #[serde(skip)]
    pub fp_frame_pending: bool,
    /// Frame-stream direction for the `calc_ea_start` path: `true` = FSAVE
    /// (write the frame from `fp_frame`), `false` = FRESTORE (read into it).
    #[serde(skip)]
    pub fp_frame_store: bool,

    /// Variant continuation hook: gives a wrapping variant a chance
    /// to dispatch follow-up tags that the 68000 doesn't know about.
    ///
    /// Called by [`Self::continue_instruction`] *before* the inner
    /// match. Returning `true` means the hook handled the tag; the
    /// 68000's own dispatch is skipped. Returning `false` (or
    /// leaving the hook `None`) preserves pure-68000 behaviour.
    ///
    /// Variants reserve tag numbers in the 200+ range (the 68000
    /// uses 0..=80ish) to avoid collisions. The 68010 wrapper
    /// installs a hook in `new()`; the 68020 inherits it through
    /// the wrapped `Cpu68010` and only needs to override when it
    /// gains its own continuation-bearing opcodes.
    #[serde(skip)]
    pub variant_continue_hook: Option<fn(&mut Cpu68000) -> bool>,

    /// Generic 32-bit stash for variant continuation state.
    ///
    /// Used by multi-step variant instructions to carry data across
    /// follow-up tag transitions — for example, `RTD` consumes the
    /// `d16` extension word in its first dispatch, stashes the
    /// sign-extended value here, and applies it to SP after the PC
    /// pop completes.
    #[serde(skip)]
    pub variant_pending_disp: u32,

    /// On-chip instruction cache (68020+). `None` on the 68000/68010
    /// (no cache); the 68020+ wrapper installs `Some(ICache::new())` in
    /// `install_variant_hooks`. A program-space prefetch ([`FetchIRC`])
    /// that hits self-serves the word with no external bus cycle, so
    /// cached code does not contend with the chip-RAM DMA grid. Gated at
    /// runtime on CACR.E (enable) / CACR.F (freeze). Cache contents are
    /// architectural timing state and are serialized so a restored warm hit
    /// does not become an external bus cycle. Runtime snapshots version this
    /// binary-layout change at their envelope boundary.
    ///
    /// [`FetchIRC`]: crate::microcode::MicroOp::FetchIRC
    #[serde(default)]
    pub variant_icache: Option<crate::icache::ICache>,
}

mod diagnostics;

pub use diagnostics::{
    CpuAddressErrorDiagnosticSnapshot, CpuBitFieldDiagnosticSnapshot,
    CpuBusCycleDiagnosticSnapshot, CpuBusDiagnosticSnapshot, CpuCacheDiagnosticSnapshot,
    CpuControlDiagnosticSnapshot, CpuCoreDiagnosticSnapshot, CpuExceptionDiagnosticSnapshot,
    CpuExecutionDiagnosticSnapshot, CpuExecutionStateDiagnosticKind,
    CpuExecutionStateDiagnosticSnapshot, CpuFpuDiagnosticSnapshot,
    CpuFpuPipelineDiagnosticSnapshot, CpuFullFormatEaDiagnosticSnapshot,
    CpuInterruptDiagnosticSnapshot, CpuMovemDiagnosticSnapshot, CpuPipelineDiagnosticSnapshot,
    CpuPrefetchDiagnosticSnapshot, CpuRteDiagnosticSnapshot, CpuStatusDiagnosticSnapshot,
    CpuVariantDiagnosticSnapshot,
};

impl Cpu68000 {
    /// Create a new CPU in reset state.
    ///
    /// Supervisor mode, interrupt mask level 7, all registers zero.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
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
            ff_dp: 0,
            ff_base: 0,
            ff_regd: 0,
            ff_outer: 0,
            ff_disp: 0,
            ff_phase: 0,
            ff_stream_left: 0,
            ff_is_src: false,
            alu_op: AluOp::Add,
            bit_op: BitOp::Btst,
            target_ipl: 0,
            interrupts_taken: 0,
            debug_mode: false,
            movem_mask: 0,
            movem_idx: 0,
            movem_is_write: false,
            movem_an_reg: 0xFF,
            exc_vector: None,
            src_val: 0,
            dst_val: 0,
            ae_fault_addr: 0,
            ae_access_info: 0,
            ae_saved_sr: 0,
            ae_in_progress: false,
            group0_or_group1_processing: false,
            ae_from_fetch_irc: false,
            dbcc_dn_undo: None,
            ae_frame_ir: 0,
            pre_move_sr: None,
            pre_move_vc: None,
            program_space_access: false,
            ae_undo_reg: None,
            sp_undo: None,
            address_error_observation: None,
            group0_vector: 3,
            bus_status: BusStatus::Wait,
            active_bus_transfer: None,
            bus_transfer_size: TransferSize::Word,
            bus_data_out: 0,
            ipl: 0,
            sampled_ipl: 0,
            level7_transition_pending: false,
            reset_out: false,
            instruction_starts: 0,
            opcode_at_start: 0,
            variant_decode_hook: None,
            variant_scaled_index: false,
            variant_six_word_frame: false,
            variant_format2_vectors: false,
            variant_musashi_bcd_v: false,
            variant_musashi_div_overflow: false,
            variant_extended_sr_writes: false,
            variant_unaligned_data_access: false,
            variant_dynamic_bus_sizing: false,
            variant_format_a_group0: false,
            variant_min_bus_clocks: 4,
            variant_constant_shift_timing: false,
            variant_icache: None,
            variant_cacr_write_mask: 0,
            variant_cacr_read_zero_mask: 0,
            variant_cache_disable_asserted: false,
            variant_um_ea_calc_timing: false,
            variant_long_branch: false,
            variant_fpu_present: false,
            variant_fpu_is_68882: false,
            variant_fpu_state: 0,
            variant_ext_word: 0,
            ae_fmt_a_step: 0,
            ae_frame_pc: 0,
            rte_fmta_step: 0,
            exc_pending_pc: 0,
            exc_master_interrupt_pending: false,
            rte_saved_sr: 0,
            rte_saved_pc: 0,
            rte_stack_bank: StackBank::Interrupt,
            bf_buf: 0,
            bf_base_addr: 0,
            bf_sub_op: 0,
            bf_dr: 0,
            bf_width: 0,
            bf_bit_offset: 0,
            bf_bytes_total: 0,
            bf_bytes_done: 0,
            bf_source_val: 0,
            bf_ea_mode: 0,
            bf_ea_reg: 0,
            bf_byte_disp: 0,
            fp_mem_buf: 0,
            fp_mem_bytes_total: 0,
            fp_mem_bytes_done: 0,
            fp_mem_format: 0,
            fp_mem_opmode: 0,
            fp_mem_dst: 0,
            fp_mem_precision: 80,
            fp_mem_pending: false,
            fp_mem_store: false,
            fp_movem_active: false,
            fp_movem_store: false,
            fp_movem_list: 0,
            fp_movem_cur: 0,
            fp_movem_an: 0,
            fp_movem_areg: 0,
            fp_frame: [0; 60],
            fp_frame_total: 0,
            fp_frame_done: 0,
            fp_frame_an: 0,
            fp_frame_areg: 0,
            fp_frame_postinc: false,
            fp_frame_pending: false,
            fp_frame_store: false,
            fp_exc_pending: None,
            variant_continue_hook: None,
            variant_pending_disp: 0,
        }
    }

    /// Give the installed variant-decode hook a chance to handle an
    /// opcode that the M68000 core would otherwise route to ILLEGAL.
    ///
    /// Returns `true` if the hook took the opcode. Call sites are the
    /// 68010+/68020+ ILLEGAL-trap arms in
    /// [`Self::decode_and_execute`]; each one does
    /// `if !self.try_variant_decode(opcode) { ... ILLEGAL ... }` so
    /// pure-68000 behaviour is preserved when the hook is absent.
    pub fn try_variant_decode(&mut self, opcode: u16) -> bool {
        if let Some(hook) = self.variant_decode_hook {
            hook(self, opcode)
        } else {
            false
        }
    }

    /// SR write mask for the current variant.
    ///
    /// Returns [`motorola_68k_common::flags::SR_MASK`] (`$A71F`) on
    /// 68000 / 68010, and `SR_MASK_020` (`$F71F`) on 68020+ (adds
    /// the M-flag at bit 12). Consulted by every code path that
    /// writes a 16-bit value into SR.
    #[must_use]
    pub fn sr_write_mask(&self) -> u16 {
        if self.variant_extended_sr_writes {
            crate::flags::SR_MASK_020
        } else {
            crate::flags::SR_MASK
        }
    }

    /// Enter supervisor mode and clear the trace state appropriate to this
    /// processor generation.
    ///
    /// The 68000 and 68010 expose only T1. The 68020 adds T0, and exception
    /// entry clears both trace bits while preserving M for stack selection.
    fn enter_exception_supervisor_mode(&mut self) {
        self.regs.set_supervisor(true);
        if self.variant_extended_sr_writes {
            self.regs.sr &= !0xC000;
        } else {
            self.regs.sr &= !0x8000;
        }
    }

    /// Reset the CPU to begin executing from a given SSP and PC.
    ///
    /// Sets supervisor mode with interrupts masked, clears the micro-op
    /// queue, and begins the prefetch sequence.
    pub fn reset_to(&mut self, ssp: u32, pc: u32) {
        self.clear_address_error_execution_state();
        self.clear_active_bus_transfer();
        self.ae_in_progress = true;
        self.group0_or_group1_processing = true;
        self.regs.ssp = ssp;
        self.regs.pc = pc;
        self.regs.sr = 0x2700;
        self.regs.cacr = 0;
        if let Some(cache) = self.variant_icache.as_mut() {
            cache.clear();
        }
        self.next_fetch_addr = pc;
        self.state = State::Idle;
        self.in_followup = false;
        self.followup_tag = 0;
        self.sampled_ipl = self.ipl;
        self.level7_transition_pending = false;
        self.reset_out = false;
        self.exc_master_interrupt_pending = false;
        self.rte_saved_sr = 0;
        self.rte_saved_pc = 0;
        self.rte_stack_bank = StackBank::Interrupt;
        self.ae_fmt_a_step = 0;
        self.rte_fmta_step = 0;
        self.ae_frame_pc = 0;
        self.address_error_observation = None;
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
        self.clear_address_error_execution_state();
        self.clear_active_bus_transfer();
        self.ir = opcode;
        self.opcode_at_start = opcode;
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
        self.address_error_observation = None;
        self.state = State::Idle;
        self.instruction_starts = 1;
    }

    /// Take the most recent internally rejected odd-address transfer.
    ///
    /// The observation is independent of the later exception-frame sequence.
    /// Taking it prevents a previous fault from being mistaken for a new one.
    pub fn take_address_error_observation(&mut self) -> Option<AddressErrorObservation> {
        self.address_error_observation.take()
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
        self.clear_active_bus_transfer();
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

    /// True when the CPU has completed an instruction and is ready to decode
    /// the next one.
    #[must_use]
    pub fn is_instruction_complete(&self) -> bool {
        matches!(self.state, State::Idle) && self.micro_ops.is_empty()
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
    ///
    /// Call every 4 crystal clocks. The machine layer must:
    /// 1. Before calling: write `bus_status` if the CPU is in a
    ///    `BusCycle` state (inspect the state via the bus output
    ///    fields set by `initiate_bus_cycle`).
    /// 2. Before calling: write `ipl` from Paula's interrupt
    ///    priority encoder.
    /// 3. After calling: check `reset_out` for RESET instruction.
    pub fn tick(&mut self) {
        self.sample_interrupt_level();

        // --- Idle: drain instant ops, check interrupts, start bus cycles ---
        if matches!(self.state, State::Idle) {
            self.process_instant_ops();

            // Check for pending interrupts when no work remains
            if matches!(self.state, State::Idle)
                && self.micro_ops.is_empty()
                && let Some(level) = self.take_interrupt_at_boundary()
            {
                self.initiate_interrupt_exception(level);
                self.process_instant_ops();
            }

            // Start next instruction if nothing queued
            if matches!(self.state, State::Idle) && self.micro_ops.is_empty() {
                self.start_next_instruction();
                self.process_instant_ops();
            }

            // Dispatch next non-instant op
            if matches!(self.state, State::Idle)
                && let Some(op) = self.micro_ops.pop()
            {
                if op.is_bus() {
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
        // 4-clock minimum bus cycle (S0-S7) on the 68000/68010; the
        // 68020+ wrapper lowers this to 3 via its TimingClass.
        let min_bus = self.variant_min_bus_clocks;
        let mut completed_bus_cycle = None;
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
                is_word: _,
                data: _,
                cycle_count,
            } => {
                *cycle_count = cycle_count.saturating_add(1);
                if *cycle_count >= min_bus {
                    // The machine layer writes bus_status before this
                    // tick based on the output pins (addr, fc, rw, etc.)
                    // that were set when this BusCycle state was entered.
                    if !matches!(self.bus_status, BusStatus::Wait) {
                        completed_bus_cycle = Some((*op, *addr, *fc, *is_read, self.bus_status));
                    }
                }
            }
            State::Halted => {}
            State::Stopped => {
                // The STOP instruction waits for an interrupt with a
                // priority higher than the current mask. The machine
                // writes self.ipl before each tick.
                if let Some(level) = self.take_interrupt_at_boundary() {
                    self.state = State::Idle;
                    self.initiate_interrupt_exception(level);
                    self.process_instant_ops();
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

        // Bus completion mutates both the serialized logical-transfer state
        // and the compatibility State::BusCycle view. Process it after the
        // state match so neither mutation overlaps the borrow above.
        if let Some((op, addr, fc, is_read, result)) = completed_bus_cycle {
            match result {
                BusStatus::Ready(read_data) => {
                    if self.active_bus_transfer.is_some() {
                        self.finish_dynamic_bus_phase(
                            op,
                            addr,
                            is_read,
                            u32::from(read_data),
                            None,
                        );
                    } else {
                        self.finish_bus_cycle(op, read_data);
                        self.state = State::Idle;
                    }
                }
                BusStatus::ReadySized { data, port } => {
                    assert!(
                        self.active_bus_transfer.is_some(),
                        "sized bus response requires an active MC68020/MC68030 data transfer"
                    );
                    self.finish_dynamic_bus_phase(op, addr, is_read, data, Some(port));
                }
                BusStatus::Error => {
                    self.clear_active_bus_transfer();
                    self.state = State::Idle;
                    if op == MicroOp::InterruptAck {
                        // BERR terminates an interrupt-acknowledge cycle by
                        // supplying the spurious interrupt vector.
                        self.finish_bus_cycle(op, 24);
                    } else {
                        self.begin_bus_error(addr, is_read, fc);
                    }
                }
                BusStatus::Wait => unreachable!("wait responses are not completion events"),
            }
        }
    }

    /// Process all instant ops at the front of the queue.
    fn process_instant_ops(&mut self) {
        while let Some(op) = self.micro_ops.front() {
            if !op.is_instant() {
                break;
            }
            let op = self
                .micro_ops
                .pop()
                .expect("instant op queue should contain the requested micro-op");
            match op {
                MicroOp::Execute => self.decode_and_execute(),
                MicroOp::PromoteIRC => {
                    // A 68881/2 arithmetic exception raised by the just-retired
                    // FP instruction is delivered here, before the next
                    // instruction (and before interrupt sampling): `irc_addr`
                    // is the following instruction's address — the stacked PC
                    // for this post-instruction trap.
                    if let Some(vector) = self.fp_exc_pending.take() {
                        self.begin_group1_exception(vector, self.irc_addr);
                    } else {
                        // The 68000 samples interrupts at instruction boundaries.
                        if let Some(level) = self.take_interrupt_at_boundary() {
                            self.initiate_interrupt_exception(level);
                        } else {
                            self.promote_pipeline();
                        }
                    }
                }
                MicroOp::AssertReset => {
                    self.reset_out = true;
                }
                _ => {}
            }
        }
    }

    /// Queue PromoteIRC to start the next instruction.
    pub(crate) fn start_next_instruction(&mut self) {
        self.micro_ops.push(MicroOp::PromoteIRC);
    }

    /// Sample the external request level and retain a lower-to-level-7
    /// transition until the next interrupt-recognition boundary.
    fn sample_interrupt_level(&mut self) {
        debug_assert!(self.ipl <= 7, "IPL input must be encoded in three bits");
        if self.sampled_ipl < 7 && self.ipl == 7 {
            self.level7_transition_pending = true;
        }
        self.sampled_ipl = self.ipl;
    }

    /// Select one interrupt at an instruction or STOP boundary.
    ///
    /// A pending level-7 transition is independent of the active mask and
    /// takes priority over the current level comparison. Once consumed, a
    /// continuously held level 7 can be accepted again only if software
    /// lowers the active mask below 7.
    fn take_interrupt_at_boundary(&mut self) -> Option<u8> {
        if self.level7_transition_pending {
            self.level7_transition_pending = false;
            Some(7)
        } else {
            let level = self.ipl;
            (level > self.regs.interrupt_mask()).then_some(level)
        }
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
        self.opcode_at_start = self.ir;
        self.instruction_starts = self.instruction_starts.wrapping_add(1);
        // Standard 68000: PC points past the opcode word
        self.regs.pc = self.instr_start_pc.wrapping_add(2);
        self.in_followup = false;
        self.followup_tag = 0;
        self.src_mode = None;
        self.dst_mode = None;
        self.clear_address_error_execution_state();
        self.micro_ops.push(MicroOp::FetchIRC);
        self.micro_ops.push(MicroOp::Execute);
    }

    /// Clear transient state retained while constructing an address-error frame.
    ///
    /// This does not clear the public observation: callers may inspect that after
    /// the exception handler's prefetch has promoted the pipeline.
    fn clear_address_error_execution_state(&mut self) {
        self.ae_in_progress = false;
        self.group0_or_group1_processing = false;
        self.ae_from_fetch_irc = false;
        self.ae_undo_reg = None;
        self.sp_undo = None;
        self.dbcc_dn_undo = None;
        self.pre_move_sr = None;
        self.pre_move_vc = None;
        self.program_space_access = false;
    }

    /// Clear the externally visible state of a dynamic-sized data transfer.
    fn clear_active_bus_transfer(&mut self) {
        self.active_bus_transfer = None;
        self.bus_transfer_size = TransferSize::Word;
        self.bus_data_out = 0;
        self.bus_status = BusStatus::Wait;
    }

    /// Begin an interrupt exception sequence.
    ///
    /// The processor enters supervisor mode immediately when processing an
    /// exception. The 68000/68010 use SSP. On the 68020+, an interrupt
    /// accepted with M set first writes its normal frame to MSP, then clears M
    /// and writes a Format-$1 throwaway frame to ISP.
    fn initiate_interrupt_exception(&mut self, level: u8) {
        self.clear_address_error_execution_state();
        self.group0_or_group1_processing = true;
        self.target_ipl = level;
        self.interrupts_taken = self.interrupts_taken.wrapping_add(1);
        // Save old SR before changing mode (for pushing in the exception frame).
        self.ae_saved_sr = self.regs.sr;
        self.exc_master_interrupt_pending =
            self.regs.master_stack_capable() && self.regs.sr & 0x1000 != 0;
        // Enter supervisor mode before pushing so A7 selects the appropriate
        // supervisor stack (SSP/ISP, or MSP when M is set on a 68020+).
        self.enter_exception_supervisor_mode();
        // The active processor mask changes to the accepted level as
        // interrupt processing begins. The saved copy above retains the
        // pre-interrupt mask for the exception frame.
        self.regs.sr = (self.regs.sr & !0x0700) | (u16::from(self.target_ipl) << 8);
        self.in_followup = true;
        // The PC to save is the address of the NEXT instruction — the one
        // that would have executed if the interrupt hadn't fired. That's
        // irc_addr (where the current IRC was fetched from), NOT regs.pc
        // (which points 2 bytes past irc_addr due to the prefetch pipeline).
        // RTE will restore this address and begin a fresh prefetch from it.
        let pc_to_push = self.irc_addr;

        if self.variant_six_word_frame {
            // 68010+: the selected interrupt vector must supply both
            // the Format/Vector word and the handler fetch. Acquire it
            // before constructing the 8-byte Format-$0 frame so a
            // device vector or spurious response cannot leave an
            // autovector-derived offset in the frame.
            //
            // RTE on 68010+ pops 8 bytes for Format $0 (Stage J
            // fix); pushing 6 bytes here was leaking 2 bytes per
            // interrupt, accumulating into SSP overflow past the top
            // of chip RAM. Surfaced during A1200 Stage L.
            self.exc_pending_pc = pc_to_push;
            self.exc_vector = None;
            self.followup_tag = TAG_EXC_IACK_COMPLETE;
            self.micro_ops.push(MicroOp::InterruptAck);
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // 68000: push PC directly (6-byte frame: PC + SR).
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
        self.clear_address_error_execution_state();
        self.group0_or_group1_processing = matches!(vector, 4 | 8 | 9 | 10 | 11);
        self.ae_saved_sr = self.regs.sr;
        self.exc_master_interrupt_pending = false;
        self.enter_exception_supervisor_mode();
        self.exc_vector = Some(vector);
        self.in_followup = true;
        self.micro_ops.clear();

        if self.variant_six_word_frame {
            // Stash the PC so `TAG_EXC_STACK_FORMAT` can restore it
            // into `self.data` for the PC push.
            self.exc_pending_pc = pc_to_push;

            if self.variant_format2_vectors && matches!(vector, 5 | 6 | 7 | 9) {
                // 68020+ Format `$2` 12-byte frame: push the
                // Instruction Address long *above* the Format word.
                // M68000PRM § 8.6.3 lists the Format-$2 vectors as
                // CHK / CHK2 (6), divide-by-zero (5), TRAPV / TRAPcc
                // (7), and Trace (9). The Instruction Address is
                // the PC of the faulting instruction —
                // `instr_start_pc` is exactly that.
                self.data = self.instr_start_pc;
                self.followup_tag = TAG_EXC_STACK_INSTR_ADDR_HI;
                self.micro_ops.push(MicroOp::PushLongHi);
                self.micro_ops.push(MicroOp::Execute);
            } else {
                // Short Format `$0` 8-byte frame: push the
                // Format/Vector word first (it sits at the highest
                // address, SP+6 in the final layout).
                // M68000PRM § 8.6.
                self.data = u32::from(u16::from(vector) * 4);
                self.followup_tag = TAG_EXC_STACK_FORMAT;
                self.micro_ops.push(MicroOp::PushWord);
                self.micro_ops.push(MicroOp::Execute);
            }
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
    /// Public so variant crates can fetch a memory operand through the
    /// shared pipeline (e.g. the 68020 memory-source MUL.L / DIV.L).
    pub fn queue_read_ops(&mut self, size: Size) {
        match size {
            Size::Byte => self.micro_ops.push(MicroOp::ReadByte),
            Size::Word => self.micro_ops.push(MicroOp::ReadWord),
            Size::Long => {
                if self.variant_dynamic_bus_sizing {
                    self.micro_ops.push(MicroOp::ReadLong);
                } else {
                    self.micro_ops.push(MicroOp::ReadLongHi);
                    self.micro_ops.push(MicroOp::ReadLongLo);
                }
            }
        }
    }

    /// Queue write micro-ops for the given size at the current EA address.
    ///
    /// Public so variant crates can stage a memory write-back (e.g. 68020
    /// CAS writing the update register on a successful compare).
    pub fn queue_write_ops(&mut self, size: Size) {
        match size {
            Size::Byte => self.micro_ops.push(MicroOp::WriteByte),
            Size::Word => self.micro_ops.push(MicroOp::WriteWord),
            Size::Long => {
                if self.variant_dynamic_bus_sizing {
                    self.micro_ops.push(MicroOp::WriteLong);
                } else {
                    self.micro_ops.push(MicroOp::WriteLongHi);
                    self.micro_ops.push(MicroOp::WriteLongLo);
                }
            }
        }
    }

    /// Map a micro-op to bus cycle parameters and enter BusCycle state.
    ///
    /// Push ops decrement SP before computing the write address.
    /// Pop ops increment SP after the read address is computed.
    ///
    /// One op does not enter `BusCycle`: a [`MicroOp::FetchIRC`] that
    /// hits the 68020+ instruction cache self-serves the word and
    /// returns a 1-clock `State::Internal` instead, so cached code
    /// neither stalls for the bus nor contends with chip-RAM DMA. The
    /// hit path is gated on `variant_icache` being present (68020+ only)
    /// and CACR.E (enable); everything else is unchanged.
    fn initiate_bus_cycle(&mut self, op: MicroOp) -> State {
        assert!(
            self.variant_dynamic_bus_sizing
                || !matches!(
                    op,
                    MicroOp::ReadLong | MicroOp::WriteLong | MicroOp::PushLong | MicroOp::PopLong
                ),
            "logical long micro-op requires MC68020/MC68030 dynamic bus sizing"
        );

        let is_sup = self.regs.is_supervisor();

        // 68020+ instruction-cache hit: self-serve the prefetch word
        // with no external bus cycle. `lookup` borrows `variant_icache`
        // only for the call, so the borrow ends before we update fetch
        // state. The served value is byte-identical to a bus fetch —
        // only the cycle is elided — so architectural state is unchanged.
        if matches!(op, MicroOp::FetchIRC)
            && !self.variant_cache_disable_asserted
            && self.regs.cacr & 0x01 != 0
        {
            let addr = self.next_fetch_addr;
            let hit = self
                .variant_icache
                .as_ref()
                .and_then(|cache| cache.lookup(addr, is_sup));
            if let Some(word) = hit {
                // Mirror finish_bus_cycle's FetchIRC bookkeeping.
                self.irc = word;
                self.irc_addr = addr;
                self.next_fetch_addr = addr.wrapping_add(2);
                self.regs.pc = self.next_fetch_addr;
                // A cache hit costs ~1 internal clock vs the 3-clock
                // (020) external bus cycle.
                return State::Internal { cycles: 1 };
            }
        }

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
            MicroOp::ReadLong => (self.addr, fc_ea, true, true, None),
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
            MicroOp::WriteLong => (
                self.addr,
                fc_data,
                false,
                true,
                Some((self.data >> 16) as u16),
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
            MicroOp::PushLong => {
                // SP -= 4 once for the complete logical transfer.
                let sp = self.regs.active_sp().wrapping_sub(4);
                self.regs.set_active_sp(sp);
                (sp, fc_data, false, true, Some((self.data >> 16) as u16))
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
            MicroOp::PopLong => {
                // Match the existing pop path: expose the updated SP while
                // the memory cycle is active.
                let sp = self.regs.active_sp();
                self.regs.set_active_sp(sp.wrapping_add(4));
                (sp, fc_data, true, true, None)
            }
            MicroOp::InterruptAck => {
                // During interrupt acknowledge the 68000 places the
                // accepted level on A3-A1 and drives every other address
                // line high. The external machine must decode this bus
                // value, not re-sample the live IPL input pins.
                let addr = interrupt_acknowledge_address(self.target_ipl);
                (addr, FunctionCode::InterruptAck, true, true, None)
            }
            _ => panic!("Non-bus op in initiate_bus_cycle: {:?}", op),
        };

        if self.variant_dynamic_bus_sizing
            && let Some((logical_size, write_data)) = self.dynamic_transfer_description(op, is_read)
        {
            let transfer = ActiveBusTransfer {
                logical_size,
                remaining: logical_size,
                write_data,
                read_data: 0,
            };
            let (is_word, data) = Self::compatibility_phase(&transfer, is_read);
            self.active_bus_transfer = Some(transfer);
            self.bus_transfer_size = logical_size;
            self.bus_data_out = if is_read {
                0
            } else {
                dynamic_write_data(write_data, logical_size, addr)
            };
            self.bus_status = BusStatus::Wait;

            return State::BusCycle {
                op,
                addr,
                fc,
                is_read,
                is_word,
                data,
                cycle_count: 0,
            };
        }

        self.clear_active_bus_transfer();

        // M68000 has no MMU: logical and physical addresses are
        // identical. The on-die MMU first appears in the 68030 — the
        // table walk and TT register matching live in
        // `motorola-68030::mmu` and will be wired in when that
        // variant gets its own state machine.
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

    /// Describe an MC68020/MC68030 logical data operand.
    ///
    /// Existing high/low micro-ops remain word-sized logical transfers. This
    /// preserves their continuation boundaries while still applying SIZ and
    /// DSACK rules to odd addresses. The whole-long variants allow ordinary
    /// long loads and stores to complete through one aligned 32-bit phase.
    fn dynamic_transfer_description(
        &self,
        op: MicroOp,
        is_read: bool,
    ) -> Option<(TransferSize, u32)> {
        let size = match op {
            MicroOp::ReadByte | MicroOp::WriteByte => TransferSize::Byte,
            MicroOp::ReadWord
            | MicroOp::ReadWordNoData
            | MicroOp::WriteWord
            | MicroOp::PushWord
            | MicroOp::PopWord
            | MicroOp::ReadLongHi
            | MicroOp::ReadLongLo
            | MicroOp::WriteLongHi
            | MicroOp::WriteLongLo
            | MicroOp::PushLongHi
            | MicroOp::PushLongLo
            | MicroOp::PopLongHi
            | MicroOp::PopLongLo => TransferSize::Word,
            MicroOp::ReadLong | MicroOp::WriteLong | MicroOp::PushLong | MicroOp::PopLong => {
                TransferSize::Long
            }
            MicroOp::FetchIRC | MicroOp::InterruptAck => return None,
            _ => return None,
        };

        let write_data = if is_read {
            0
        } else {
            match op {
                MicroOp::WriteByte => self.data & 0xFF,
                MicroOp::WriteWord | MicroOp::PushWord => self.data & 0xFFFF,
                MicroOp::WriteLongHi | MicroOp::PushLongHi => (self.data >> 16) & 0xFFFF,
                MicroOp::WriteLongLo | MicroOp::PushLongLo => self.data & 0xFFFF,
                MicroOp::WriteLong | MicroOp::PushLong => self.data,
                _ => 0,
            }
        };

        Some((size, write_data))
    }

    /// Produce the legacy byte/word view of the current dynamic phase.
    fn compatibility_phase(transfer: &ActiveBusTransfer, is_read: bool) -> (bool, Option<u16>) {
        let remaining = transfer.remaining.bytes();
        let chunk = remaining.min(2);
        let shift = u32::from(remaining - chunk) * 8;
        let mask = if chunk == 2 { 0xFFFF } else { 0xFF };
        let data = if is_read {
            None
        } else {
            Some(((transfer.write_data >> shift) & mask) as u16)
        };
        (chunk == 2, data)
    }

    /// Consume one completed physical phase of a dynamic-sized transfer.
    fn finish_dynamic_bus_phase(
        &mut self,
        op: MicroOp,
        address: u32,
        is_read: bool,
        bus_data: u32,
        port: Option<DataPortSize>,
    ) {
        let mut transfer = self
            .active_bus_transfer
            .take()
            .expect("dynamic phase requires active transfer state");

        let transferred = match port {
            Some(port) => dynamic_transfer_bytes(transfer.remaining, address, port),
            None => transfer.remaining.bytes().min(2),
        };

        if is_read {
            let phase_data = match port {
                Some(port) => extract_dynamic_bus_data(bus_data, transferred, address, port),
                None => {
                    let mask = if transferred == 2 { 0xFFFF } else { 0xFF };
                    bus_data & mask
                }
            };
            transfer.read_data = ((u64::from(transfer.read_data) << (u32::from(transferred) * 8))
                | u64::from(phase_data)) as u32;
        }

        let remaining = transfer.remaining.bytes() - transferred;
        if remaining == 0 {
            self.clear_active_bus_transfer();
            if is_read {
                match op {
                    MicroOp::ReadLong | MicroOp::PopLong => {
                        self.data = transfer.read_data;
                    }
                    _ => self.finish_bus_cycle(op, transfer.read_data as u16),
                }
            }
            self.state = State::Idle;
            return;
        }

        transfer.remaining = TransferSize::from_bytes(remaining);
        let next_addr = address.wrapping_add(u32::from(transferred));
        let (next_is_word, next_data) = Self::compatibility_phase(&transfer, is_read);

        self.bus_transfer_size = transfer.remaining;
        self.bus_data_out = if is_read {
            0
        } else {
            dynamic_write_data(transfer.write_data, transfer.remaining, next_addr)
        };
        self.active_bus_transfer = Some(transfer);
        self.bus_status = BusStatus::Wait;

        let State::BusCycle {
            op: state_op,
            addr,
            is_read: state_is_read,
            is_word,
            data,
            cycle_count,
            ..
        } = &mut self.state
        else {
            panic!("dynamic transfer phase completed outside a bus cycle");
        };
        debug_assert_eq!(*state_op, op);
        debug_assert_eq!(*state_is_read, is_read);
        *addr = next_addr;
        *is_word = next_is_word;
        *data = next_data;
        *cycle_count = 0;
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
                let fetched_addr = self.next_fetch_addr;
                self.irc = read_data;
                self.irc_addr = fetched_addr;
                self.next_fetch_addr = fetched_addr.wrapping_add(2);
                // PC tracks the fetch address (like real 68000)
                self.regs.pc = self.next_fetch_addr;
                // 68020+ instruction-cache fill on miss: cache the
                // just-fetched program word so a re-fetch (loops — the
                // hot path) self-serves with no bus cycle. Gated on
                // CACR.E (enable) and !CACR.F (freeze suppresses fills
                // but still allows hits). FC2 = the supervisor bit of
                // the program-space function code.
                let enabled = self.regs.cacr & 0x01 != 0;
                let frozen = self.regs.cacr & 0x02 != 0;
                if enabled && !frozen && !self.variant_cache_disable_asserted {
                    let fc2 = self.regs.is_supervisor();
                    if let Some(cache) = self.variant_icache.as_mut() {
                        cache.fill(fetched_addr, fc2, read_data);
                    }
                }
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
            MicroOp::ReadWord | MicroOp::ReadLongHi | MicroOp::ReadLong => (self.addr, true),
            MicroOp::ReadLongLo => (self.addr.wrapping_add(2), true),
            MicroOp::WriteWord | MicroOp::WriteLongHi | MicroOp::WriteLong => (self.addr, false),
            MicroOp::WriteLongLo => (self.addr.wrapping_add(2), false),
            MicroOp::PushWord => (self.regs.active_sp().wrapping_sub(2), false),
            MicroOp::PushLongHi | MicroOp::PushLong => {
                (self.regs.active_sp().wrapping_sub(4), false)
            }
            MicroOp::PushLongLo => (self.regs.active_sp().wrapping_add(2), false),
            MicroOp::PopWord | MicroOp::PopLongHi | MicroOp::PopLong => {
                (self.regs.active_sp(), true)
            }
            MicroOp::PopLongLo => (self.regs.active_sp().wrapping_add(2), true),
            _ => return false,
        };

        // The 68020+ accepts byte, word, and long data operands at any byte
        // boundary. Instruction words remain word-aligned, so FetchIRC must
        // still reject an odd target.
        if self.variant_unaligned_data_access && !matches!(op, MicroOp::FetchIRC) {
            return false;
        }

        // Even address: no error
        if check_addr & 1 == 0 {
            return false;
        }

        // Double address error: halt the CPU
        if self.ae_in_progress {
            self.clear_active_bus_transfer();
            self.state = State::Halted;
            return true;
        }

        // Determine function code for the group-0 frame. Instruction fetches
        // and PC-relative operand reads use program space; other operands use
        // data space.
        let is_sup = self.regs.is_supervisor();
        let is_program = matches!(op, MicroOp::FetchIRC)
            || (self.program_space_access
                && matches!(
                    op,
                    MicroOp::ReadWord
                        | MicroOp::ReadLongHi
                        | MicroOp::ReadLongLo
                        | MicroOp::ReadLong
                ));
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
        self.clear_active_bus_transfer();
        let not_processing_instruction = self.group0_or_group1_processing;
        self.ae_fault_addr = self.adjust_ae_fault_addr(fault_addr, is_read);
        self.ae_in_progress = true;
        self.group0_or_group1_processing = true;
        self.group0_vector = 3;

        // UNLK: undo the A7 ← An modification so the exception frame
        // gets pushed on the original (valid) stack, not the faulting one.
        if let Some((bank, original_sp)) = self.sp_undo.take() {
            self.regs.set_stack_pointer(bank, original_sp);
        }

        // Apply the current software-oracle compatibility policy for EA
        // register side effects. The historical commit boundary is unresolved.
        if let Some((reg, amount, is_postinc, _is_dst)) = self.ae_undo_reg.take() {
            let undo = if is_postinc {
                if !is_read {
                    // Write AE: always undo postincrement (write never committed).
                    true
                } else {
                    // The retained compatibility fixtures keep standard
                    // postincrement source-read updates.
                    false
                }
            } else {
                // Predecrement undo rules:
                // - ADDX/SUBX -(Ay),-(Ax): byte/word source predecrement
                //   sticks on AE. Long only commits the first -2 step.
                // - Standard -(An) EA: only undo on write AE for Long size.
                //   The retained fixtures keep byte/word write decrements but
                //   restore the second half of a long predecrement.
                // - ADDX/SUBX long compatibility retains one word-sized step.
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
                let partial_long_predec = !is_postinc
                    && self.size == Size::Long
                    && ((is_read
                        && matches!(self.ir & 0xF130, 0xD100 | 0x9100)
                        && (self.ir & 0x0008) != 0)
                        || (!is_read && (self.ir >> 12) == 2 && ((self.ir >> 6) & 7) == 4));
                let undo_amount = if partial_long_predec { 2 } else { amount };
                if is_postinc {
                    self.regs.set_a(r, current.wrapping_sub(undo_amount));
                } else {
                    self.regs.set_a(r, current.wrapping_add(undo_amount));
                }
            }
        }

        // Current MOVEM compatibility retains a one-word postincrement when
        // the first read is rejected, including for long transfers.
        if is_read && (self.ir & 0xFF80) == 0x4C80 && self.movem_an_reg != 0xFF {
            let r = self.movem_an_reg as usize;
            self.regs.set_a(r, self.addr.wrapping_add(2));
        }

        // Current Emu198x compatibility retains the DBcc decrement. The
        // MAME-derived corpus instead restores Dn; hardware remains unresolved.
        self.dbcc_dn_undo = None;

        // Apply the retained MOVE write-AE status compatibility policy.
        // pre_move_sr = full restore, pre_move_vc = V,C only.
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
        // Capture the status selected by the compatibility policy above.
        self.ae_saved_sr = self.regs.sr;

        self.ae_frame_ir = self.opcode_at_start;

        self.ae_access_info = (self.ae_frame_ir & 0xFFE0)
            | (if is_read { 0x10 } else { 0 })
            | (if not_processing_instruction { 0x08 } else { 0 })
            | u16::from(fc.bits() & 0x07);

        // Enter supervisor mode and clear the variant's trace bits.
        self.enter_exception_supervisor_mode();

        // Abandon current instruction
        self.micro_ops.clear();
        self.in_followup = true;

        // Frame PC: complex formula that depends on instruction type,
        // addressing modes, access size, and read/write direction.
        let frame_pc = self.compute_ae_frame_pc(is_read);

        self.address_error_observation = Some(AddressErrorObservation {
            requested_address: fault_addr,
            frame_fault_address: self.ae_fault_addr,
            access: if is_read {
                AddressErrorAccess::Read
            } else {
                AddressErrorAccess::Write
            },
            function_code: fc,
            access_information: self.ae_access_info,
            saved_sr: self.ae_saved_sr,
            frame_ir: self.ae_frame_ir,
            frame_pc,
        });

        if self.variant_format_a_group0 {
            // 68020+ 32-byte Format $A frame. The push happens
            // back-to-front (highest field first) so the final SP
            // ends up at the SR slot. Stash the PC for the
            // multi-step push handler.
            self.ae_frame_pc = frame_pc;
            self.ae_fmt_a_step = 0;
            self.data = 0;
            self.followup_tag = TAG_AE_FMT_A_STEP;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // 68000-style 14-byte frame: push PC first, then SR,
            // IR, fault address, access info.
            self.data = frame_pc;
            self.followup_tag = TAG_AE_PUSH_SR;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    /// Start a bus error exception sequence.
    ///
    /// Called when a non-acknowledge bus cycle returns `BusStatus::Error`
    /// (e.g. Fat Gary timeout). The 68000 pushes the same 14-byte group-0
    /// frame as address error, using vector 2 instead of vector 3.
    pub(crate) fn begin_bus_error(&mut self, fault_addr: u32, is_read: bool, fc: FunctionCode) {
        // Double fault during another group-0 exception → halt.
        if self.ae_in_progress {
            self.clear_active_bus_transfer();
            self.state = State::Halted;
            return;
        }

        let not_processing_instruction = self.group0_or_group1_processing;
        self.ae_in_progress = true;
        self.group0_or_group1_processing = true;
        self.ae_saved_sr = self.regs.sr;

        // Enter supervisor mode and clear the variant's trace bits.
        self.enter_exception_supervisor_mode();

        // Abandon current instruction.
        self.clear_active_bus_transfer();
        self.micro_ops.clear();
        self.in_followup = true;

        // 68000: 14-byte frame as address error, but vector 2.
        self.group0_vector = 2;
        self.ae_fault_addr = fault_addr;
        self.ae_frame_ir = self.ir;
        self.ae_access_info = (self.ir & 0xFFE0)
            | (if is_read { 0x10 } else { 0 })
            | (if not_processing_instruction { 0x08 } else { 0 })
            | u16::from(fc.bits() & 0x07);

        if self.variant_format_a_group0 {
            // 68020+ 32-byte Format $A frame.
            self.ae_frame_pc = self.instr_start_pc;
            self.ae_fmt_a_step = 0;
            self.data = 0;
            self.followup_tag = TAG_AE_FMT_A_STEP;
            self.micro_ops.push(MicroOp::Execute);
        } else {
            // 68000-style: reuse the AE tag chain.
            self.data = self.instr_start_pc;
            self.followup_tag = TAG_AE_PUSH_SR;
            self.micro_ops.push(MicroOp::PushLongHi);
            self.micro_ops.push(MicroOp::PushLongLo);
            self.micro_ops.push(MicroOp::Execute);
        }
    }

    /// Compute the frame PC for an address error exception.
    ///
    /// The current compatibility result depends on:
    /// - Instruction type (MOVE vs non-MOVE)
    /// - Access direction (read vs write)
    /// - Addressing modes and their extension words
    /// - Operation size (for predecrement)
    ///
    /// These formulas are classified software-oracle compatibility rules, not
    /// independently measured original-processor behaviour.
    fn compute_ae_frame_pc(&self, is_read: bool) -> u32 {
        // The MC68020 short bus-fault frame is used when an odd instruction
        // target is detected at an instruction boundary. Table 6-5 defines
        // its stacked PC as the next instruction, which is the rejected
        // prefetch address itself. Keep the implementation-generated 68000
        // compatibility formula below isolated to the original frame.
        if self.variant_format_a_group0 && self.ae_from_fetch_irc {
            return self.ae_fault_addr;
        }

        let top = (self.ir >> 12) & 0xF;

        // MOVE instructions have a separate, more complex formula
        if matches!(top, 1..=3) {
            return self.compute_ae_frame_pc_move(is_read);
        }

        // FetchIRC AE: branch/jump to an odd target.
        //
        // When a branch/jump instruction resolves to an odd address, the
        // core has already updated regs.pc to the target before issuing
        // the FetchIRC that triggers the AE. SingleStepTests fixtures for
        // JMP, JSR, BSR, Bcc, DBcc, RTS, RTE and RTR expect `regs.pc - 4`
        // as the frame PC. This is a corpus-compatibility rule, not an
        // independently measured hardware claim.
        if self.ae_from_fetch_irc {
            return self.regs.pc.wrapping_sub(4);
        }

        let ea_mode = ((self.ir >> 3) & 7) as u8;
        let ea_reg = (self.ir & 7) as u8;

        // UNLK: frame PC = ISP + 4 (past opcode and prefetched IRC word).
        if self.ir & 0xFFF8 == 0x4E58 {
            return self.instr_start_pc.wrapping_add(4);
        }

        // ADDX/SUBX -(An),-(An) compatibility uses the instruction-start PC.
        if matches!(top, 0x9 | 0xD) {
            let opmode = (self.ir >> 6) & 7;
            if (4..=6).contains(&opmode) && ea_mode == 1 {
                return self.instr_start_pc;
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
            return self.instr_start_pc.wrapping_add(2 + movem_ea_ext);
        }

        // Extension words consumed before the faulting access advance
        // the internal PC beyond the opcode word. The frame PC reflects
        // how far the pipeline advanced: ISP + (consumed ext words × 2).
        //
        // Predecrement does NOT add to the frame PC — it modifies an
        // address register, not the instruction stream.

        // Absolute addressing extension words.
        let abs_adj: u32 = if ea_mode == 7 {
            match ea_reg {
                0 => 2, // abs.w: 1 ext word
                1 => 4, // abs.l: 2 ext words
                _ => 0,
            }
        } else {
            0
        };

        // d16(An) and d8(An,Xn) extension words.
        let disp_adj: u32 = if ea_mode == 5 || ea_mode == 6 { 2 } else { 0 };

        // d16(PC) and d8(PC,Xn) extension words.
        let pc_rel_adj: u32 = if ea_mode == 7 && matches!(ea_reg, 2 | 3) {
            2
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
            .wrapping_add(abs_adj + disp_adj + pc_rel_adj + imm_adj)
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
            // For MOVE source-read address errors, the frame PC tracks only
            // extension words already consumed for the source EA.
            self.instr_start_pc.wrapping_add(u32::from(src_ext) * 2)
        } else {
            let dst = AddrMode::decode(((self.ir >> 6) & 7) as u8, ((self.ir >> 9) & 7) as u8)
                .unwrap_or(AddrMode::DataReg(0));
            let dst_ext = Self::ext_word_count_for_mode(&dst, size);
            if matches!(dst, AddrMode::AddrInd(_) | AddrMode::AddrIndPostInc(_)) {
                return self.instr_start_pc.wrapping_add(u32::from(src_ext) * 2);
            }

            let src_is_register_like = matches!(
                src,
                AddrMode::DataReg(_) | AddrMode::AddrReg(_) | AddrMode::Immediate
            );
            if src_is_register_like {
                let extra = src_ext.saturating_add(dst_ext.saturating_sub(1));
                return self.instr_start_pc.wrapping_add(2 + u32::from(extra) * 2);
            }

            // MOVE write AEs save a PC that tracks the source-side extension
            // words consumed to obtain the value being written. The common
            // cases also include the opcode-word bump; destination extension
            // words do not further advance the saved PC on the 68000's
            // group-0 frame.
            self.instr_start_pc.wrapping_add(2 + u32::from(src_ext) * 2)
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

    /// Adjust the abstract request to the fault address used by the retained
    /// software-oracle compatibility boundary.
    fn adjust_ae_fault_addr(&self, addr: u32, is_read: bool) -> u32 {
        // ADDX/SUBX long fixtures use a word-stepped fault address while the
        // abstract EA path applies the complete long predecrement at once.
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

        // MOVEM.l predecrement fixtures use a word-stepped fault address while
        // the abstract EA path applies the complete long predecrement at once.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_long_micro_op_requires_dynamic_sizing_before_stack_side_effects() {
        let mut cpu = Cpu68000::new();
        cpu.regs.sr = 0x2000;
        cpu.regs.ssp = 0x8000;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = cpu.initiate_bus_cycle(MicroOp::PushLong);
        }));

        assert!(result.is_err());
        assert_eq!(
            cpu.regs.active_sp(),
            0x8000,
            "a rejected whole-long push must not decrement the stack pointer"
        );
    }

    fn cpu_with_program_read_address_error() -> Cpu68000 {
        let mut cpu = Cpu68000::new();
        cpu.regs.sr = 0x2000;
        cpu.ir = 0x303A;
        cpu.opcode_at_start = 0x303A;
        cpu.instr_start_pc = 0x1000;
        cpu.addr = 0xFF12_3457;
        cpu.program_space_access = true;

        assert!(cpu.check_address_error(MicroOp::ReadWord));
        cpu
    }

    fn cpu_with_data_write_address_error() -> Cpu68000 {
        let mut cpu = Cpu68000::new();
        cpu.regs.sr = 0x2000;
        cpu.ir = 0x3080;
        cpu.opcode_at_start = 0x3080;
        cpu.instr_start_pc = 0x1000;
        cpu.addr = 0x0012_3457;

        assert!(cpu.check_address_error(MicroOp::WriteWord));
        cpu
    }

    fn cpu_with_instruction_fetch_address_error() -> Cpu68000 {
        let mut cpu = Cpu68000::new();
        cpu.regs.sr = 0x2000;
        cpu.ir = 0x4ED2;
        cpu.opcode_at_start = 0x4ED2;
        cpu.instr_start_pc = 0x1000;
        cpu.next_fetch_addr = 0x0012_3457;

        assert!(cpu.check_address_error(MicroOp::FetchIRC));
        cpu
    }

    #[test]
    fn unaligned_data_capability_allows_every_shared_data_transfer() {
        let data_ops = [
            MicroOp::ReadWord,
            MicroOp::ReadLongHi,
            MicroOp::ReadLongLo,
            MicroOp::WriteWord,
            MicroOp::WriteLongHi,
            MicroOp::WriteLongLo,
            MicroOp::PushWord,
            MicroOp::PushLongHi,
            MicroOp::PushLongLo,
            MicroOp::PopWord,
            MicroOp::PopLongHi,
            MicroOp::PopLongLo,
        ];

        for op in data_ops {
            let mut cpu = Cpu68000::new();
            cpu.variant_unaligned_data_access = true;
            cpu.regs.ssp = 0x0012_3457;
            cpu.addr = 0x0012_3457;
            cpu.program_space_access = true;

            assert!(
                !cpu.check_address_error(op),
                "{op:?} must remain a valid 68020 data transfer"
            );
            assert_eq!(cpu.take_address_error_observation(), None);
        }
    }

    #[test]
    fn unaligned_data_capability_still_rejects_odd_instruction_fetches() {
        let mut cpu = Cpu68000::new();
        cpu.variant_unaligned_data_access = true;
        cpu.regs.sr = 0x2000;
        cpu.next_fetch_addr = 0x0012_3457;

        assert!(cpu.check_address_error(MicroOp::FetchIRC));
        assert_eq!(
            cpu.take_address_error_observation()
                .expect("odd instruction fetch must still be observed")
                .requested_address,
            0x0012_3457
        );
    }

    #[test]
    fn address_error_observation_records_program_read_boundary() {
        let mut cpu = cpu_with_program_read_address_error();
        let observation = cpu
            .take_address_error_observation()
            .expect("odd word read should produce an observation");

        assert_eq!(observation.requested_address, 0xFF12_3457);
        assert_eq!(observation.frame_fault_address, 0xFF12_3457);
        assert_eq!(observation.access, AddressErrorAccess::Read);
        assert_eq!(observation.function_code, FunctionCode::SupervisorProgram);
        assert_eq!(observation.access_information, 0x3036);
        assert_eq!(observation.saved_sr, 0x2000);
        assert_eq!(observation.frame_ir, 0x303A);
        assert_eq!(observation.frame_pc, 0x1002);
        assert_eq!(cpu.take_address_error_observation(), None);
    }

    #[test]
    fn address_error_observation_records_data_write_boundary() {
        let mut cpu = cpu_with_data_write_address_error();
        let observation = cpu
            .take_address_error_observation()
            .expect("odd word write should produce an observation");

        assert_eq!(observation.requested_address, 0x0012_3457);
        assert_eq!(observation.frame_fault_address, 0x0012_3457);
        assert_eq!(observation.access, AddressErrorAccess::Write);
        assert_eq!(observation.function_code, FunctionCode::SupervisorData);
        assert_eq!(observation.access_information, 0x3085);
    }

    #[test]
    fn stack_write_address_error_ignores_program_operand_space() {
        let mut cpu = Cpu68000::new();
        cpu.regs.sr = 0x2000;
        cpu.regs.ssp = 0x0012_3457;
        cpu.ir = 0x487A; // PEA (d16,PC)
        cpu.opcode_at_start = 0x487A;
        cpu.instr_start_pc = 0x1000;
        cpu.program_space_access = true;

        assert!(cpu.check_address_error(MicroOp::PushLongHi));
        let observation = cpu
            .take_address_error_observation()
            .expect("odd stack write should produce an observation");

        assert_eq!(observation.access, AddressErrorAccess::Write);
        assert_eq!(observation.function_code, FunctionCode::SupervisorData);
        assert_eq!(observation.access_information & 0x001F, 0x0005);
    }

    #[test]
    fn instruction_fetch_address_error_records_instruction_processing() {
        let mut cpu = cpu_with_instruction_fetch_address_error();
        let observation = cpu
            .take_address_error_observation()
            .expect("odd instruction fetch should produce an observation");

        assert_eq!(observation.access, AddressErrorAccess::Read);
        assert_eq!(observation.function_code, FunctionCode::SupervisorProgram);
        assert_eq!(observation.access_information, 0x4ED6);
        assert_eq!(observation.access_information & 0x0008, 0);
    }

    #[test]
    fn setup_boundaries_clear_address_error_observation() {
        let mut reset_cpu = cpu_with_program_read_address_error();
        reset_cpu.reset_to(0x2000, 0x1000);
        assert_eq!(reset_cpu.take_address_error_observation(), None);
        assert!(reset_cpu.ae_in_progress);
        assert!(reset_cpu.group0_or_group1_processing);
        assert!(!reset_cpu.ae_from_fetch_irc);
        assert!(!reset_cpu.program_space_access);

        let mut prefetch_cpu = cpu_with_program_read_address_error();
        prefetch_cpu.regs.pc = 0x1004;
        prefetch_cpu.setup_prefetch(0x4E71, 0x4E71);
        assert_eq!(prefetch_cpu.take_address_error_observation(), None);
        assert!(!prefetch_cpu.ae_in_progress);
        assert!(!prefetch_cpu.group0_or_group1_processing);
        assert!(!prefetch_cpu.ae_from_fetch_irc);
        assert!(!prefetch_cpu.program_space_access);

        let serialized = serde_json::to_vec(&cpu_with_program_read_address_error())
            .expect("serialize CPU with pending observation");
        let mut restored: Cpu68000 =
            serde_json::from_slice(&serialized).expect("deserialize CPU snapshot");
        assert_eq!(restored.take_address_error_observation(), None);
    }

    #[test]
    fn reset_clears_double_address_error_state() {
        let mut cpu = cpu_with_program_read_address_error();
        cpu.addr = 0x0012_3457;
        assert!(cpu.check_address_error(MicroOp::ReadWord));
        assert!(matches!(cpu.state, State::Halted));

        cpu.reset_to(0x2000, 0x1000);
        cpu.regs.pc = 0x1004;
        cpu.setup_prefetch(0x4E71, 0x4E71);
        cpu.addr = 0x0012_3457;
        assert!(cpu.check_address_error(MicroOp::ReadWord));
        assert!(!matches!(cpu.state, State::Halted));
    }

    #[test]
    fn level7_transition_state_survives_serde() {
        let mut cpu = Cpu68000::new();
        cpu.state = State::Internal { cycles: 3 };
        cpu.ipl = 7;
        cpu.tick();

        assert_eq!(cpu.sampled_ipl, 7);
        assert!(cpu.level7_transition_pending);

        let serialized =
            serde_json::to_vec(&cpu).expect("serialize CPU with pending level-7 transition");
        let restored: Cpu68000 =
            serde_json::from_slice(&serialized).expect("deserialize level-7 transition state");

        assert_eq!(restored.sampled_ipl, 7);
        assert!(restored.level7_transition_pending);
    }

    #[test]
    fn reset_clears_pending_level7_transition_and_synchronizes_input() {
        let mut cpu = Cpu68000::new();
        cpu.state = State::Internal { cycles: 3 };
        cpu.ipl = 7;
        cpu.tick();
        assert!(cpu.level7_transition_pending);

        cpu.ipl = 6;
        cpu.reset_out = true;
        cpu.reset_to(0x2000, 0x1000);

        assert_eq!(cpu.sampled_ipl, 6);
        assert!(!cpu.level7_transition_pending);
        assert!(!cpu.reset_out);
    }

    #[test]
    fn group1_handler_fetch_address_error_records_not_instruction() {
        let mut cpu = Cpu68000::new();
        cpu.regs.sr = 0x2000;
        cpu.ir = 0x4AFC;
        cpu.opcode_at_start = 0x4AFC;
        cpu.begin_group1_exception(4, 0x1000);
        assert!(cpu.group0_or_group1_processing);

        cpu.followup_tag = TAG_EXC_FINISH;
        cpu.data = 0x2001;
        cpu.continue_instruction();
        assert!(cpu.group0_or_group1_processing);
        assert!(!cpu.ae_in_progress);
        assert!(cpu.check_address_error(MicroOp::FetchIRC));

        let observation = cpu
            .take_address_error_observation()
            .expect("odd group-1 handler fetch should produce an observation");
        assert_eq!(observation.access_information & 0x001F, 0x001E);
        assert!(!matches!(cpu.state, State::Halted));
    }

    #[test]
    fn group1_handler_fetch_context_survives_serde() {
        let mut cpu = Cpu68000::new();
        cpu.regs.sr = 0x2000;
        cpu.ir = 0x4AFC;
        cpu.opcode_at_start = 0x4AFC;
        cpu.begin_group1_exception(4, 0x1000);
        cpu.followup_tag = TAG_EXC_FINISH;
        cpu.data = 0x2001;

        let serialized =
            serde_json::to_vec(&cpu).expect("serialize group-1 handler-prefetch state");
        let mut restored: Cpu68000 = serde_json::from_slice(&serialized)
            .expect("deserialize group-1 handler-prefetch state");
        assert!(restored.group0_or_group1_processing);

        restored.continue_instruction();
        assert!(restored.check_address_error(MicroOp::FetchIRC));
        let observation = restored
            .take_address_error_observation()
            .expect("odd restored group-1 handler fetch should produce an observation");
        assert_eq!(observation.access_information & 0x001F, 0x001E);
        assert!(!matches!(restored.state, State::Halted));
    }

    #[test]
    fn group2_handler_fetch_address_error_records_instruction_processing() {
        let mut cpu = Cpu68000::new();
        cpu.regs.sr = 0x2000;
        cpu.ir = 0x80C0;
        cpu.opcode_at_start = 0x80C0;
        cpu.begin_group1_exception(5, 0x1000);
        assert!(!cpu.group0_or_group1_processing);

        cpu.followup_tag = TAG_EXC_FINISH;
        cpu.data = 0x2001;
        cpu.continue_instruction();
        assert!(cpu.check_address_error(MicroOp::FetchIRC));

        let observation = cpu
            .take_address_error_observation()
            .expect("odd group-2 handler fetch should produce an observation");
        assert_eq!(observation.access_information & 0x001F, 0x0016);
    }

    #[test]
    fn group0_odd_handler_fetch_halts() {
        let mut cpu = cpu_with_instruction_fetch_address_error();
        let _ = cpu.take_address_error_observation();
        cpu.followup_tag = TAG_AE_FINISH;
        cpu.data = 0x2001;
        cpu.continue_instruction();

        assert!(cpu.ae_in_progress);
        assert!(cpu.group0_or_group1_processing);
        assert!(cpu.check_address_error(MicroOp::FetchIRC));
        assert!(matches!(cpu.state, State::Halted));
        assert_eq!(cpu.take_address_error_observation(), None);
    }

    #[test]
    fn program_operand_address_error_reads_vector_in_supervisor_data_space() {
        let mut cpu = cpu_with_program_read_address_error();
        let mut vector_cycles = Vec::new();

        for _ in 0..256 {
            let bus_cycle = match &cpu.state {
                State::BusCycle {
                    addr,
                    fc,
                    cycle_count,
                    ..
                } => Some((*addr, *fc, *cycle_count)),
                _ => None,
            };

            if let Some((addr, fc, cycle_count)) = bus_cycle {
                if matches!(addr, 0x0C | 0x0E)
                    && !vector_cycles
                        .iter()
                        .any(|(seen_addr, _)| *seen_addr == addr)
                {
                    vector_cycles.push((addr, fc));
                }
                cpu.bus_status = if cycle_count >= 3 {
                    BusStatus::Ready(0)
                } else {
                    BusStatus::Wait
                };
            } else {
                cpu.bus_status = BusStatus::Wait;
            }

            cpu.tick();
            if vector_cycles.len() == 2 {
                break;
            }
        }

        assert_eq!(
            vector_cycles,
            vec![
                (0x0C, FunctionCode::SupervisorData),
                (0x0E, FunctionCode::SupervisorData),
            ]
        );
    }
}
