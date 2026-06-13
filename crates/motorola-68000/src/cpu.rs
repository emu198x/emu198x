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
use crate::bus::{BusStatus, FunctionCode};
use crate::microcode::{MicroOp, MicroOpQueue};
use crate::registers::Registers;
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
/// RTE: 68020+ Format $A short bus-fault — pop the remaining 20
/// bytes (= 10 words) above the F/V word. Each step reads one word
/// and advances SSP; step 10 finishes the RTE.
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
/// step 11 hands off to `TAG_AE_FETCH_VECTOR`.
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

/// Bit manipulation operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BitOp {
    Btst,
    Bset,
    Bclr,
    Bchg,
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
    /// CPU. Tests use this to confirm IRQ delivery without poking
    /// `exc_vector` (which `initiate_interrupt_exception` intentionally
    /// leaves unset to distinguish interrupts from group-1/2
    /// exceptions in the shared follow-up tag chain).
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

    /// **Input:** Interrupt priority level (IPL0-IPL2), written by
    /// the machine layer from Paula's interrupt priority encoder.
    /// Checked at instruction boundaries and in the Stopped state.
    pub ipl: u8,

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
    /// PRM defines V as undefined for these. Real 68000 hardware
    /// (the upstream Tom Harte SingleStepTests corpus) and Musashi
    /// pick different concrete values for V; both are valid
    /// interpretations of "undefined" but they disagree
    /// instruction-by-instruction. Our reference oracles split:
    /// the m68k-test-gen 68010 / 68020 corpora are Musashi-driven
    /// (so they expect Musashi V), while the upstream 68000 corpus
    /// is real-hardware-derived (and expects real-hw V).
    ///
    /// `false` (default) → real-hw V via `bcd_add_realhw` etc.
    /// `true` → Musashi V via `bcd_add_musashi` etc.
    /// The 68010 / 68020 wrappers set it `true` in `new()`.
    #[serde(skip)]
    pub variant_musashi_bcd_v: bool,

    /// Musashi-style overflow flag handling on 16-bit `DIVU.W` /
    /// `DIVS.W` (and 32-bit `DIVL` already follows this path).
    ///
    /// PRM § 6.2.7: "on overflow N undefined, Z undefined, C
    /// cleared, V set". Real 68000 hardware does roughly that (V
    /// set, C cleared, N/Z preserved). Musashi preserves *all*
    /// flags except V (which is set). The same Musashi-vs-real-hw
    /// split as the BCD V flag applies.
    ///
    /// `false` (default) → real-hw: clear C, set V, preserve N/Z/X.
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

    /// 68020+ Format `$A` group-0 exception frame.
    ///
    /// The 68000 / 68010 push a 14-byte frame for bus error (vec 2)
    /// and address error (vec 3): access info, fault address, IR,
    /// SR, PC. The 68020 promotes group-0 to a 28-byte short
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

    /// On-chip instruction cache (68020+). `None` on the 68000/68010
    /// (no cache); the 68020+ wrapper installs `Some(ICache::new())` in
    /// `install_variant_hooks`. A program-space prefetch ([`FetchIRC`])
    /// that hits self-serves the word with no external bus cycle, so
    /// cached code does not contend with the chip-RAM DMA grid — the
    /// real Amiga benefit, not merely a clock saving. Gated at runtime
    /// on CACR.E (enable) / CACR.F (freeze). `#[serde(skip)]`: rebuilt
    /// empty on deserialize, which is transparent (a cold cache always
    /// misses to the bus). See `icache.rs` and the 68k cycle-timing
    /// plan (#41/#110/#111).
    ///
    /// [`FetchIRC`]: crate::microcode::MicroOp::FetchIRC
    #[serde(skip)]
    pub variant_icache: Option<crate::icache::ICache>,

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

    /// Step counter for the 11-step Format `$A` push sequence.
    /// Consulted by `TAG_AE_FMT_A_STEP`.
    #[serde(skip)]
    pub(crate) ae_fmt_a_step: u8,

    /// Step counter for the 10-step Format `$A` RTE pop. Each step
    /// reads one word and advances SSP; step 10 finishes the RTE.
    #[serde(skip)]
    pub(crate) rte_fmta_step: u8,

    /// Frame PC saved at group-0 entry for use by Format `$A`
    /// pushes. The 68000 path stores it in `self.data` and pushes
    /// immediately; Format `$A` needs the value preserved across
    /// many intermediate pushes.
    #[serde(skip)]
    pub(crate) ae_frame_pc: u32,

    /// Pending PC value during the 68010+ exception frame push.
    ///
    /// When `variant_six_word_frame` is set,
    /// `begin_group1_exception` pushes the Format/Vector word first,
    /// which needs `self.data` to hold the format word during that
    /// push. The PC value gets stashed here and restored to
    /// `self.data` once the format push completes.
    #[serde(skip)]
    pub(crate) exc_pending_pc: u32,

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
}

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
            ae_from_fetch_irc: false,
            dbcc_dn_undo: None,
            ae_frame_ir: 0,
            pre_move_sr: None,
            pre_move_vc: None,
            program_space_access: false,
            ae_undo_reg: None,
            sp_undo: None,
            group0_vector: 3,
            bus_status: BusStatus::Wait,
            ipl: 0,
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
            variant_format_a_group0: false,
            variant_min_bus_clocks: 4,
            variant_constant_shift_timing: false,
            variant_icache: None,
            variant_um_ea_calc_timing: false,
            variant_ext_word: 0,
            ae_fmt_a_step: 0,
            ae_frame_pc: 0,
            rte_fmta_step: 0,
            exc_pending_pc: 0,
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
        self.state = State::Idle;
        self.instruction_starts = 1;
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
        // --- Idle: drain instant ops, check interrupts, start bus cycles ---
        if matches!(self.state, State::Idle) {
            self.process_instant_ops();

            // Check for pending interrupts when no work remains
            if matches!(self.state, State::Idle) && self.micro_ops.is_empty() {
                let ipl = self.ipl;
                if ipl > self.regs.interrupt_mask() || ipl == 7 {
                    self.initiate_interrupt_exception(ipl);
                    self.process_instant_ops();
                }
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
                    let result = self.bus_status;
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
                // priority higher than the current mask. The machine
                // writes self.ipl before each tick.
                let ipl = self.ipl;
                if ipl > self.regs.interrupt_mask() || ipl == 7 {
                    self.state = State::Idle;
                    self.initiate_interrupt_exception(ipl);
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
                    // The 68000 samples interrupts at instruction boundaries.
                    let ipl = self.ipl;
                    if ipl > self.regs.interrupt_mask() || ipl == 7 {
                        self.initiate_interrupt_exception(ipl);
                    } else {
                        self.promote_pipeline();
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
        self.interrupts_taken = self.interrupts_taken.wrapping_add(1);
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

        if self.variant_six_word_frame {
            // 68010+: 8-byte Format-$0 interrupt frame. Push the F/V
            // word first using the autovector number (24 + level).
            // All retro 68010+ targets we support (Amiga A1200/A3000/
            // A4000/CD32, Atari Falcon) use autovectored interrupts,
            // so the subsequent IACK returns the same autovector
            // value the CPU pre-pushed in the F/V word — the frame
            // is internally consistent. Genuinely-vectored 68010+
            // systems (Mac via VIA/SCC) would need an IACK-first
            // refactor before the F/V push. M68000PRM § 8.6.
            //
            // RTE on 68010+ pops 8 bytes for Format $0 (Stage J
            // fix); pushing 6 bytes here was leaking 2 bytes per
            // interrupt, accumulating into SSP overflow past the top
            // of chip RAM. Surfaced during A1200 Stage L.
            let vector = 24u8.saturating_add(level);
            self.exc_pending_pc = pc_to_push;
            self.data = u32::from(u16::from(vector) * 4);
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
    ///
    /// One op does not enter `BusCycle`: a [`MicroOp::FetchIRC`] that
    /// hits the 68020+ instruction cache self-serves the word and
    /// returns a 1-clock `State::Internal` instead, so cached code
    /// neither stalls for the bus nor contends with chip-RAM DMA. The
    /// hit path is gated on `variant_icache` being present (68020+ only)
    /// and CACR.E (enable); everything else is unchanged.
    fn initiate_bus_cycle(&mut self, op: MicroOp) -> State {
        let is_sup = self.regs.is_supervisor();

        // 68020+ instruction-cache hit: self-serve the prefetch word
        // with no external bus cycle. `lookup` borrows `variant_icache`
        // only for the call, so the borrow ends before we update fetch
        // state. The served value is byte-identical to a bus fetch —
        // only the cycle is elided — so architectural state is unchanged.
        if matches!(op, MicroOp::FetchIRC) && self.regs.cacr & 0x01 != 0 {
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
                if enabled && !frozen {
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

        // Double address error: halt the CPU
        if self.ae_in_progress {
            self.state = State::Halted;
            return true;
        }

        // Determine function code for the group-0 frame.
        // The Harte/MAME fixtures expect only instruction-fetch faults to
        // report program-space FC bits; data operand faults, including
        // PC-relative operands, report data-space FC bits.
        let is_sup = self.regs.is_supervisor();
        let is_program = matches!(op, MicroOp::FetchIRC);
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
        if let Some((reg, amount, is_postinc, _is_dst)) = self.ae_undo_reg.take() {
            let undo = if is_postinc {
                if !is_read {
                    // Write AE: always undo postincrement (write never committed).
                    true
                } else {
                    // Standard postincrement source reads stick, even for long
                    // transfers; the odd/partial access faults after the
                    // address register update has committed.
                    false
                }
            } else {
                // Predecrement undo rules:
                // - ADDX/SUBX -(Ay),-(Ax): byte/word source predecrement
                //   sticks on AE. Long only commits the first -2 step.
                // - Standard -(An) EA: only undo on write AE for Long size.
                //   The real 68000 keeps the decremented value for byte/word
                //   write AE, but undoes it for long (verified by DL tests).
                // ADDX/SUBX -(Ay),-(Ax) long: the 68000 decrements by 2
                // (word-sized step) before checking alignment. AE fires after
                // the first -2, so only half the predecrement gets undone.
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

        // MOVEM mem→reg with postincrement advances the address register a
        // word at a time. If the first read faults, the +2 step has already
        // committed even for long transfers.
        if is_read && (self.ir & 0xFF80) == 0x4C80 && self.movem_an_reg != 0xFF {
            let r = self.movem_an_reg as usize;
            self.regs.set_a(r, self.addr.wrapping_add(2));
        }

        // DBcc with an odd taken target keeps the Dn.w decrement. The fetch
        // that faults is for the next instruction, not part of the decrement.
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

        self.ae_frame_ir = self.opcode_at_start;

        self.ae_access_info = (self.ae_frame_ir & 0xFFE0)
            | (if is_read { 0x10 } else { 0 })
            | (if self.ae_from_fetch_irc { 0x08 } else { 0 })
            | u16::from(fc.bits() & 0x07);

        // Enter supervisor mode and clear trace
        self.regs.set_supervisor(true);
        self.regs.sr &= !0x8000; // Clear trace

        // Abandon current instruction
        self.micro_ops.clear();
        self.in_followup = true;

        // Frame PC: complex formula that depends on instruction type,
        // addressing modes, access size, and read/write direction.
        let frame_pc = self.compute_ae_frame_pc(is_read);

        if self.variant_format_a_group0 {
            // 68020+ 28-byte Format $A frame. The push happens
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
    /// Called when the bus returns `BusStatus::Error` (e.g. Fat Gary timeout).
    /// The 68000 pushes the same 14-byte group-0 frame as address error,
    /// using vector 2 instead of vector 3.
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

        // 68000: 14-byte frame as address error, but vector 2.
        self.group0_vector = 2;
        self.ae_fault_addr = fault_addr;
        self.ae_frame_ir = self.ir;
        self.ae_access_info =
            (self.ir & 0xFFE0) | (if is_read { 0x10 } else { 0 }) | u16::from(fc.bits() & 0x07);

        if self.variant_format_a_group0 {
            // 68020+ 28-byte Format $A frame.
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
        //
        // When a branch/jump instruction resolves to an odd address, the
        // CPU has already updated regs.pc to the target before issuing
        // the FetchIRC that triggers the AE. The real 68000 saves
        // `regs.pc - 4` as the frame PC, reflecting the prefetch
        // pipeline state at the moment of the fault.
        //
        // Verified against Tom Harte 680x0 fixtures for JMP, JSR, BSR,
        // Bcc, DBcc, RTS, RTE, RTR.
        if self.ae_from_fetch_irc {
            return self.regs.pc.wrapping_sub(4);
        }

        let ea_mode = ((self.ir >> 3) & 7) as u8;
        let ea_reg = (self.ir & 7) as u8;

        // UNLK: frame PC = ISP + 4 (past opcode and prefetched IRC word).
        if self.ir & 0xFFF8 == 0x4E58 {
            return self.instr_start_pc.wrapping_add(4);
        }

        // ADDX/SUBX -(An),-(An): address errors report the instruction start
        // PC, not ISP + 4. The long predecrement path only commits the first
        // word-sized decrement before the alignment fault trips.
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
