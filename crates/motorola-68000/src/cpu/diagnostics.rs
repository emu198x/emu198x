//! Side-effect-free, bounded CPU diagnostics.
//!
//! The execution core owns several private sequencer fields that cannot be
//! reconstructed reliably from the architectural register file. This module is
//! a child of `cpu`, so it can copy those fields into typed snapshots without
//! making the mutable implementation surface public.

use crate::addressing::AddrMode;
use crate::alu::Size;
use crate::bus::{BusStatus, FunctionCode, TransferSize};
use crate::flags::{C, N, V, X, Z};
use crate::microcode::MicroOp;
use crate::registers::{FpReg, StackBank};
use serde::Serialize;

use super::{ActiveBusTransfer, AddressErrorObservation, AluOp, BitOp, Cpu68000, State};

/// Decoded status-register fields and active stack-bank selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuStatusDiagnosticSnapshot {
    /// Low five condition-code bits.
    pub ccr: u8,
    /// Extend condition code.
    pub extend: bool,
    /// Negative condition code.
    pub negative: bool,
    /// Zero condition code.
    pub zero: bool,
    /// Overflow condition code.
    pub overflow: bool,
    /// Carry condition code.
    pub carry: bool,
    /// T1 trace bit.
    pub trace_1: bool,
    /// T0 trace bit, implemented on the MC68020 and later.
    pub trace_0: bool,
    /// Whether either trace bit is set.
    pub trace_enabled: bool,
    /// Supervisor-mode bit.
    pub supervisor: bool,
    /// Raw master-stack selection bit.
    pub master_bit: bool,
    /// Whether this CPU implements distinct interrupt and master stacks.
    pub master_stack_capable: bool,
    /// Whether the master stack is the currently selected A7 bank.
    pub master_stack_active: bool,
    /// Interrupt priority mask decoded from SR.
    pub interrupt_mask: u8,
    /// Stack bank selected by the current S/M state.
    pub active_stack_bank: StackBank,
}

/// Architectural control-register slots implemented by the shared core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuControlDiagnosticSnapshot {
    /// Vector Base Register.
    pub vbr: u32,
    /// Source Function Code register.
    pub sfc: u8,
    /// Destination Function Code register.
    pub dfc: u8,
    /// Cache Control Register.
    pub cacr: u32,
    /// Cache Address Register.
    pub caar: u32,
    /// Translation Control register.
    pub tc: u32,
    /// Instruction Transparent Translation register 0.
    pub itt0: u32,
    /// Instruction Transparent Translation register 1.
    pub itt1: u32,
    /// Data Transparent Translation register 0.
    pub dtt0: u32,
    /// Data Transparent Translation register 1.
    pub dtt1: u32,
    /// Supervisor Root Pointer, low word.
    pub srp: u32,
    /// Supervisor Root Pointer, high word on the MC68030.
    pub srp_upper: u32,
    /// CPU Root Pointer or User Root Pointer, low word.
    pub urp: u32,
    /// CPU Root Pointer, high word on the MC68030.
    pub crp_upper: u32,
    /// MMU Status Register.
    pub mmusr: u32,
    /// MC68060 Bus Control Register slot.
    pub buscr: u32,
    /// MC68060 Processor Configuration Register slot.
    pub pcr: u32,
}

/// Floating-point architectural and configured component state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuFpuDiagnosticSnapshot {
    /// Whether an FPU or FPU coprocessor is connected to the execution core.
    pub present: bool,
    /// Whether the configured external coprocessor uses MC68882 frames.
    pub is_68882: bool,
    /// Internal FSAVE state: zero is null/reset and one is idle.
    pub internal_state: u8,
    /// Floating-point data registers FP0-FP7.
    pub registers: [FpReg; 8],
    /// Floating-point Control Register.
    pub fpcr: u32,
    /// Floating-point Status Register.
    pub fpsr: u32,
    /// Floating-point Instruction Address Register.
    pub fpiar: u32,
    /// FPCR rounding-mode field.
    pub rounding_mode: u8,
    /// FPCR rounding-precision field.
    pub rounding_precision: u8,
    /// FPSR condition-code nibble.
    pub condition_code: u8,
    /// FPSR negative condition.
    pub negative: bool,
    /// FPSR zero condition.
    pub zero: bool,
    /// FPSR infinity condition.
    pub infinity: bool,
    /// FPSR not-a-number condition.
    pub nan: bool,
}

/// Instruction-register and instruction-boundary state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuPrefetchDiagnosticSnapshot {
    /// Instruction register currently executing.
    pub ir: u16,
    /// Next prefetched instruction word.
    pub irc: u16,
    /// Address from which IRC was fetched.
    pub irc_addr: u32,
    /// Address of the next instruction-word fetch.
    pub next_fetch_addr: u32,
    /// Program counter captured at the current instruction start.
    pub instr_start_pc: u32,
    /// Opcode captured at the current instruction start.
    pub opcode_at_start: u16,
    /// Monotonic instruction-start count.
    pub instruction_starts: u64,
}

/// Coarse execution-state classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuExecutionStateDiagnosticKind {
    /// Ready to consume another micro-operation.
    Idle,
    /// Waiting for an internal delay.
    Internal,
    /// Waiting for an external bus cycle.
    BusCycle,
    /// Halted after a terminal fault or unsupported path.
    Halted,
    /// Stopped until an accepted interrupt.
    Stopped,
}

/// One active physical bus cycle held by the execution state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuBusCycleDiagnosticSnapshot {
    /// Micro-operation which initiated the cycle.
    pub operation: MicroOp,
    /// Address driven for the physical cycle.
    pub address: u32,
    /// Function-code pins.
    pub function_code: FunctionCode,
    /// Read direction when true.
    pub is_read: bool,
    /// Word cycle when true; byte cycle when false.
    pub is_word: bool,
    /// Optional write word retained by the compatibility state view.
    pub data: Option<u16>,
    /// CPU clocks already spent in the cycle.
    pub cycle_count: u8,
}

/// Current coarse state with bounded details for the active phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuExecutionStateDiagnosticSnapshot {
    /// Coarse state-machine phase.
    pub kind: CpuExecutionStateDiagnosticKind,
    /// Remaining internal cycles when `kind` is `internal`.
    pub internal_cycles: Option<u8>,
    /// Physical bus-cycle detail when `kind` is `bus_cycle`.
    pub bus_cycle: Option<CpuBusCycleDiagnosticSnapshot>,
}

/// Current instruction execution and effective-address state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuExecutionDiagnosticSnapshot {
    /// Coarse tick-engine state.
    pub state: CpuExecutionStateDiagnosticSnapshot,
    /// Number of queued micro-operations.
    pub micro_op_count: usize,
    /// Fixed queue capacity.
    pub micro_op_capacity: usize,
    /// Next queued operation, without exposing the complete mutable queue.
    pub next_micro_op: Option<MicroOp>,
    /// Current effective or bus address scratch register.
    pub address: u32,
    /// Current data and ALU scratch register.
    pub data: u32,
    /// Whether a multi-phase instruction is active.
    pub in_followup: bool,
    /// Current multi-phase continuation tag.
    pub followup_tag: u8,
    /// Current source addressing mode.
    pub source_mode: Option<AddrMode>,
    /// Current destination addressing mode.
    pub destination_mode: Option<AddrMode>,
    /// Current operand size.
    pub size: Size,
    /// Effective-address register number.
    pub ea_register: u8,
    /// PC base captured for PC-relative addressing.
    pub ea_pc: u32,
    /// Current ALU operation.
    pub alu_operation: AluOp,
    /// Current bit operation.
    pub bit_operation: BitOp,
    /// Current source operand scratch value.
    pub source_value: u32,
    /// Current destination operand scratch value.
    pub destination_value: u32,
    /// Whether the current memory operand selects program space.
    pub program_space_access: bool,
    /// Whether verbose execution logging is enabled.
    pub debug_mode: bool,
}

/// Pin-facing bus state and dynamic-transfer progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuBusDiagnosticSnapshot {
    /// Response currently driven by the machine layer.
    pub status: BusStatus,
    /// Logical MC68020/MC68030 transfer in progress.
    pub active_dynamic_transfer: Option<ActiveBusTransfer>,
    /// Current SIZ1/SIZ0 decoded value.
    pub transfer_size: TransferSize,
    /// SIZ1 asserted-high logical level.
    pub siz1: bool,
    /// SIZ0 asserted-high logical level.
    pub siz0: bool,
    /// Physical D31-D0 write-data image.
    pub data_out: u32,
    /// RESET output asserted by a RESET instruction.
    pub reset_out: bool,
}

/// Interrupt input, sampling, and acceptance state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuInterruptDiagnosticSnapshot {
    /// Most recent IPL value sampled by a CPU edge.
    pub sampled_ipl: u8,
    /// Interrupt level selected for an in-progress acknowledge.
    pub target_ipl: u8,
    /// Pending lower-to-level-7 transition.
    pub level7_transition_pending: bool,
    /// Number of hardware interrupts entered since construction.
    pub interrupts_taken: u64,
}

/// Current and most-recent address-error diagnostic state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuAddressErrorDiagnosticSnapshot {
    /// Whether group-0 exception processing is active.
    pub in_progress: bool,
    /// Fault address retained by the exception sequencer.
    pub fault_address: u32,
    /// Access-information word retained for the exception frame.
    pub access_information: u16,
    /// Status register retained for the exception frame.
    pub saved_sr: u16,
    /// First instruction word retained for the exception frame.
    pub frame_ir: u16,
    /// Program counter retained for a Format-$A frame.
    pub frame_pc: u32,
    /// Whether the rejected access was an instruction fetch.
    pub from_fetch_irc: bool,
    /// Current Format-$A frame-construction step.
    pub format_a_step: u8,
    /// Most recent pre-bus odd-address rejection, retained non-destructively.
    pub last_observation: Option<AddressErrorObservation>,
    /// DBcc register correction retained for a rejected branch.
    pub dbcc_register_undo: Option<(u8, u16)>,
    /// MOVE status-register correction retained for a rejected write.
    pub pre_move_sr: Option<u16>,
    /// MOVE V/C correction retained for a rejected write.
    pub pre_move_vc: Option<u16>,
    /// Address-register side effect retained for rollback.
    pub address_register_undo: Option<(u8, u32, bool, bool)>,
    /// Stack-bank side effect retained for rollback.
    pub stack_pointer_undo: Option<(StackBank, u32)>,
}

/// RTE frame-restoration progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuRteDiagnosticSnapshot {
    /// Current Format-$A pop step.
    pub format_a_step: u8,
    /// Status register already restored from the frame.
    pub saved_sr: u16,
    /// Program counter already restored from the frame.
    pub saved_pc: u32,
    /// Stack bank on which the frame began.
    pub stack_bank: StackBank,
}

/// Exception sequencing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuExceptionDiagnosticSnapshot {
    /// Exception vector selected by the current sequence.
    pub vector: Option<u8>,
    /// Group-0 vector, normally bus error 2 or address error 3.
    pub group0_vector: u8,
    /// Whether the original core is processing a group-0 or group-1 exception.
    pub group0_or_group1_processing: bool,
    /// Program counter retained for exception stacking.
    pub pending_pc: u32,
    /// Whether an interrupt must switch from master to interrupt stack.
    pub master_interrupt_pending: bool,
    /// Address-error state and retained observation.
    pub address_error: CpuAddressErrorDiagnosticSnapshot,
    /// RTE restoration state.
    pub rte: CpuRteDiagnosticSnapshot,
    /// Pending FPU exception vector.
    pub fpu_pending_vector: Option<u8>,
}

/// MC68020 full-format effective-address pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuFullFormatEaDiagnosticSnapshot {
    /// Full-format extension word.
    pub extension_word: u16,
    /// Running base address.
    pub base: u32,
    /// Scaled index value.
    pub index: u32,
    /// Outer displacement.
    pub outer_displacement: u32,
    /// Current displacement accumulator.
    pub displacement: u32,
    /// Displacement phase: base or outer.
    pub phase: u8,
    /// Displacement words still to read.
    pub stream_words_remaining: u8,
    /// Whether the pipeline resolves the source operand.
    pub is_source: bool,
}

/// MOVEM transfer progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuMovemDiagnosticSnapshot {
    /// Registers still awaiting transfer.
    pub remaining_mask: u16,
    /// Current register index.
    pub register_index: u8,
    /// Register-to-memory direction when true.
    pub is_write: bool,
    /// Address register used by predecrement or postincrement.
    pub address_register: u8,
}

/// MC68020 bit-field memory pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuBitFieldDiagnosticSnapshot {
    /// Accumulated memory bytes.
    pub buffer: u64,
    /// First memory byte address.
    pub base_address: u32,
    /// Bit-field operation selector.
    pub sub_operation: u8,
    /// Data register selected by the extension word.
    pub data_register: u8,
    /// Normalized field width.
    pub width: u8,
    /// Bit offset inside the first byte.
    pub bit_offset: u8,
    /// Total memory bytes spanned.
    pub bytes_total: u8,
    /// Memory bytes already transferred.
    pub bytes_done: u8,
    /// Source register value retained for BFINS.
    pub source_value: u32,
    /// Effective-address mode.
    pub ea_mode: u8,
    /// Effective-address register.
    pub ea_register: u8,
    /// Signed byte displacement.
    pub byte_displacement: i32,
}

/// Bounded FPU memory, FMOVEM, and FSAVE/FRESTORE pipeline state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuFpuPipelineDiagnosticSnapshot {
    /// High half of the 12-byte operand accumulator.
    pub operand_buffer_high: u64,
    /// Low half of the 12-byte operand accumulator.
    pub operand_buffer_low: u64,
    /// Total operand bytes.
    pub operand_bytes_total: u8,
    /// Operand bytes already transferred.
    pub operand_bytes_done: u8,
    /// Source or destination format selector.
    pub operand_format: u8,
    /// FPU operation mode.
    pub operation_mode: u8,
    /// Destination floating-point register.
    pub destination_register: u8,
    /// Active rounding precision.
    pub precision: i32,
    /// Effective-address resolution is pending.
    pub operand_pending: bool,
    /// Operand direction is FPU-to-memory when true.
    pub operand_store: bool,
    /// Whether an FMOVEM transfer is active.
    pub movem_active: bool,
    /// FMOVEM direction is FPU-to-memory when true.
    pub movem_store: bool,
    /// Remaining FMOVEM register list.
    pub movem_remaining_list: u8,
    /// Current FMOVEM register.
    pub movem_current_register: u8,
    /// Current FMOVEM address.
    pub movem_address: u32,
    /// FMOVEM address-register number.
    pub movem_address_register: u8,
    /// Fixed internal-state frame buffer. Unlike memory or cache contents,
    /// this is a bounded 60-byte execution register which affects an
    /// in-progress FSAVE or FRESTORE.
    #[serde(with = "serde_big_array::BigArray")]
    pub frame_buffer: [u8; 60],
    /// Fixed internal frame-buffer capacity.
    pub frame_buffer_capacity: usize,
    /// Total bytes in the current internal-state frame.
    pub frame_bytes_total: u8,
    /// Internal-state frame bytes already transferred.
    pub frame_bytes_done: u8,
    /// Base address of the frame transfer.
    pub frame_address: u32,
    /// Frame-transfer address register.
    pub frame_address_register: u8,
    /// Whether FRESTORE uses postincrement.
    pub frame_postincrement: bool,
    /// Whether a frame transfer is pending.
    pub frame_pending: bool,
    /// Frame direction is FPU-to-memory when true.
    pub frame_store: bool,
}

/// Variant-owned transient instruction pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuPipelineDiagnosticSnapshot {
    /// MC68020 full-format effective-address state.
    pub full_format_ea: CpuFullFormatEaDiagnosticSnapshot,
    /// MOVEM transfer state.
    pub movem: CpuMovemDiagnosticSnapshot,
    /// MC68020 bit-field state.
    pub bit_field: CpuBitFieldDiagnosticSnapshot,
    /// FPU operand and frame-transfer state.
    pub fpu: CpuFpuPipelineDiagnosticSnapshot,
    /// Variant instruction extension word retained across a bus operation.
    pub variant_extension_word: u16,
    /// Variant displacement retained across a continuation.
    pub variant_pending_displacement: u32,
}

/// Installed implementation flags shared by the variant wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuVariantDiagnosticSnapshot {
    /// Variant decode hook installed.
    pub decode_hook_present: bool,
    /// Variant continuation hook installed.
    pub continue_hook_present: bool,
    /// Scaled brief-index extension words enabled.
    pub scaled_index: bool,
    /// Formatted 68010+ exception frames enabled.
    pub six_word_frame: bool,
    /// Format-$2 instruction exception frames enabled.
    pub format2_vectors: bool,
    /// Musashi-compatible undefined BCD overflow behavior selected.
    pub musashi_bcd_overflow: bool,
    /// Musashi-compatible divide-overflow flags selected.
    pub musashi_divide_overflow: bool,
    /// MC68020 extended status-register writes enabled.
    pub extended_sr_writes: bool,
    /// Unaligned data accesses enabled.
    pub unaligned_data_access: bool,
    /// MC68020/MC68030 dynamic bus sizing enabled.
    pub dynamic_bus_sizing: bool,
    /// Format-$A group-0 exception frames enabled.
    pub format_a_group0: bool,
    /// Minimum physical bus-cycle clocks.
    pub minimum_bus_clocks: u8,
    /// Constant-time barrel-shifter timing enabled.
    pub constant_shift_timing: bool,
    /// Writable CACR mask.
    pub cacr_write_mask: u32,
    /// CACR command bits which read as zero.
    pub cacr_read_zero_mask: u32,
    /// External cache-disable input asserted.
    pub cache_disable_asserted: bool,
    /// MC68020 effective-address timing enabled.
    pub um_ea_calculation_timing: bool,
    /// Long branch displacement decoding enabled.
    pub long_branch: bool,
    /// FPU execution path connected.
    pub fpu_present: bool,
    /// MC68882 frame shape selected.
    pub fpu_is_68882: bool,
    /// Whether a mutable MMU translation/ATC datapath is installed.
    pub mmu_translation_state_present: bool,
    /// Distinct interrupt and master stacks enabled.
    pub master_stack_capable: bool,
}

/// Bounded instruction-cache state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuCacheDiagnosticSnapshot {
    /// Whether the shared 68020-class instruction-cache model is installed.
    pub instruction_state_present: bool,
    /// Whether CACR currently enables instruction-cache hits.
    pub instruction_enabled: bool,
    /// Whether CACR currently freezes instruction-cache fills.
    pub instruction_frozen: bool,
    /// Whether the external cache-disable input is asserted.
    pub cache_disable_asserted: bool,
    /// Whether instruction-cache hits can currently be served.
    pub instruction_hits_enabled: bool,
    /// Whether instruction-cache fills can currently occur.
    pub instruction_fills_enabled: bool,
    /// Valid direct-mapped lines.
    pub valid_line_count: usize,
    /// Total direct-mapped lines.
    pub line_capacity: usize,
    /// Valid independently tracked instruction words.
    pub valid_word_count: usize,
    /// Total independently tracked instruction words.
    pub word_capacity: usize,
    /// Whether a mutable data-cache state model is currently installed.
    pub data_state_present: bool,
}

/// Complete bounded state owned by the shared execution core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuCoreDiagnosticSnapshot {
    /// Data registers D0-D7.
    pub d: [u32; 8],
    /// Address registers A0-A7, with the active stack bank in A7.
    pub a: [u32; 8],
    /// Explicit active A7 value.
    pub a7: u32,
    /// User Stack Pointer.
    pub usp: u32,
    /// Supervisor or Interrupt Stack Pointer.
    pub ssp: u32,
    /// Master Stack Pointer.
    pub msp: u32,
    /// Program Counter.
    pub pc: u32,
    /// Status Register.
    pub sr: u16,
    /// Decoded status-register state.
    pub status: CpuStatusDiagnosticSnapshot,
    /// Architectural control-register slots.
    pub control: CpuControlDiagnosticSnapshot,
    /// Floating-point register and component state.
    pub fpu: CpuFpuDiagnosticSnapshot,
    /// Prefetch and instruction-boundary state.
    pub prefetch: CpuPrefetchDiagnosticSnapshot,
    /// Current execution state.
    pub execution: CpuExecutionDiagnosticSnapshot,
    /// Pin-facing bus state.
    pub bus: CpuBusDiagnosticSnapshot,
    /// Current IPL input, retained at top level for the established `cpu.ipl`
    /// query path.
    pub ipl: u8,
    /// Interrupt sampling and acceptance state.
    pub interrupts: CpuInterruptDiagnosticSnapshot,
    /// Exception sequencing state.
    pub exception: CpuExceptionDiagnosticSnapshot,
    /// Variant-owned transient pipelines.
    pub pipelines: CpuPipelineDiagnosticSnapshot,
    /// Installed implementation flags.
    pub variant: CpuVariantDiagnosticSnapshot,
    /// Bounded cache configuration and validity counts.
    pub cache: CpuCacheDiagnosticSnapshot,
}

impl Cpu68000 {
    /// Copy the complete bounded diagnostic state without advancing execution
    /// or consuming one-shot observations.
    #[must_use]
    pub fn diagnostic_snapshot(&self) -> CpuCoreDiagnosticSnapshot {
        let status = CpuStatusDiagnosticSnapshot {
            ccr: self.regs.ccr(),
            extend: self.regs.sr & X != 0,
            negative: self.regs.sr & N != 0,
            zero: self.regs.sr & Z != 0,
            overflow: self.regs.sr & V != 0,
            carry: self.regs.sr & C != 0,
            trace_1: self.regs.sr & 0x8000 != 0,
            trace_0: self.regs.sr & 0x4000 != 0,
            trace_enabled: self.regs.sr & 0xC000 != 0,
            supervisor: self.regs.is_supervisor(),
            master_bit: self.regs.sr & 0x1000 != 0,
            master_stack_capable: self.regs.master_stack_capable(),
            master_stack_active: self.regs.master_stack_active(),
            interrupt_mask: self.regs.interrupt_mask(),
            active_stack_bank: self.regs.active_stack_bank(),
        };
        let control = CpuControlDiagnosticSnapshot {
            vbr: self.regs.vbr,
            sfc: self.regs.sfc,
            dfc: self.regs.dfc,
            cacr: self.regs.cacr,
            caar: self.regs.caar,
            tc: self.regs.tc,
            itt0: self.regs.itt0,
            itt1: self.regs.itt1,
            dtt0: self.regs.dtt0,
            dtt1: self.regs.dtt1,
            srp: self.regs.srp,
            srp_upper: self.regs.srp_upper,
            urp: self.regs.urp,
            crp_upper: self.regs.crp_upper,
            mmusr: self.regs.mmusr,
            buscr: self.regs.buscr,
            pcr: self.regs.pcr,
        };
        let fp_condition = self.regs.fpsr_condition_code();
        let fpu = CpuFpuDiagnosticSnapshot {
            present: self.variant_fpu_present,
            is_68882: self.variant_fpu_is_68882,
            internal_state: self.variant_fpu_state,
            registers: self.regs.fp,
            fpcr: self.regs.fpcr,
            fpsr: self.regs.fpsr,
            fpiar: self.regs.fpiar,
            rounding_mode: self.regs.fpcr_rounding_mode(),
            rounding_precision: self.regs.fpcr_rounding_precision(),
            condition_code: fp_condition,
            negative: fp_condition & 0x8 != 0,
            zero: fp_condition & 0x4 != 0,
            infinity: fp_condition & 0x2 != 0,
            nan: fp_condition & 0x1 != 0,
        };
        let prefetch = CpuPrefetchDiagnosticSnapshot {
            ir: self.ir,
            irc: self.irc,
            irc_addr: self.irc_addr,
            next_fetch_addr: self.next_fetch_addr,
            instr_start_pc: self.instr_start_pc,
            opcode_at_start: self.opcode_at_start,
            instruction_starts: self.instruction_starts,
        };
        let state = match &self.state {
            State::Idle => CpuExecutionStateDiagnosticSnapshot {
                kind: CpuExecutionStateDiagnosticKind::Idle,
                internal_cycles: None,
                bus_cycle: None,
            },
            State::Internal { cycles } => CpuExecutionStateDiagnosticSnapshot {
                kind: CpuExecutionStateDiagnosticKind::Internal,
                internal_cycles: Some(*cycles),
                bus_cycle: None,
            },
            State::BusCycle {
                op,
                addr,
                fc,
                is_read,
                is_word,
                data,
                cycle_count,
            } => CpuExecutionStateDiagnosticSnapshot {
                kind: CpuExecutionStateDiagnosticKind::BusCycle,
                internal_cycles: None,
                bus_cycle: Some(CpuBusCycleDiagnosticSnapshot {
                    operation: *op,
                    address: *addr,
                    function_code: *fc,
                    is_read: *is_read,
                    is_word: *is_word,
                    data: *data,
                    cycle_count: *cycle_count,
                }),
            },
            State::Halted => CpuExecutionStateDiagnosticSnapshot {
                kind: CpuExecutionStateDiagnosticKind::Halted,
                internal_cycles: None,
                bus_cycle: None,
            },
            State::Stopped => CpuExecutionStateDiagnosticSnapshot {
                kind: CpuExecutionStateDiagnosticKind::Stopped,
                internal_cycles: None,
                bus_cycle: None,
            },
        };
        let execution = CpuExecutionDiagnosticSnapshot {
            state,
            micro_op_count: self.micro_ops.len(),
            micro_op_capacity: self.micro_ops.capacity(),
            next_micro_op: self.micro_ops.front(),
            address: self.addr,
            data: self.data,
            in_followup: self.in_followup,
            followup_tag: self.followup_tag,
            source_mode: self.src_mode,
            destination_mode: self.dst_mode,
            size: self.size,
            ea_register: self.ea_reg,
            ea_pc: self.ea_pc,
            alu_operation: self.alu_op,
            bit_operation: self.bit_op,
            source_value: self.src_val,
            destination_value: self.dst_val,
            program_space_access: self.program_space_access,
            debug_mode: self.debug_mode,
        };
        let (siz1, siz0) = self.bus_transfer_size.siz_pins();
        let bus = CpuBusDiagnosticSnapshot {
            status: self.bus_status,
            active_dynamic_transfer: self.active_bus_transfer,
            transfer_size: self.bus_transfer_size,
            siz1,
            siz0,
            data_out: self.bus_data_out,
            reset_out: self.reset_out,
        };
        let interrupts = CpuInterruptDiagnosticSnapshot {
            sampled_ipl: self.sampled_ipl,
            target_ipl: self.target_ipl,
            level7_transition_pending: self.level7_transition_pending,
            interrupts_taken: self.interrupts_taken,
        };
        let exception = CpuExceptionDiagnosticSnapshot {
            vector: self.exc_vector,
            group0_vector: self.group0_vector,
            group0_or_group1_processing: self.group0_or_group1_processing,
            pending_pc: self.exc_pending_pc,
            master_interrupt_pending: self.exc_master_interrupt_pending,
            address_error: CpuAddressErrorDiagnosticSnapshot {
                in_progress: self.ae_in_progress,
                fault_address: self.ae_fault_addr,
                access_information: self.ae_access_info,
                saved_sr: self.ae_saved_sr,
                frame_ir: self.ae_frame_ir,
                frame_pc: self.ae_frame_pc,
                from_fetch_irc: self.ae_from_fetch_irc,
                format_a_step: self.ae_fmt_a_step,
                last_observation: self.address_error_observation,
                dbcc_register_undo: self.dbcc_dn_undo,
                pre_move_sr: self.pre_move_sr,
                pre_move_vc: self.pre_move_vc,
                address_register_undo: self.ae_undo_reg,
                stack_pointer_undo: self.sp_undo,
            },
            rte: CpuRteDiagnosticSnapshot {
                format_a_step: self.rte_fmta_step,
                saved_sr: self.rte_saved_sr,
                saved_pc: self.rte_saved_pc,
                stack_bank: self.rte_stack_bank,
            },
            fpu_pending_vector: self.fp_exc_pending,
        };
        let pipelines = CpuPipelineDiagnosticSnapshot {
            full_format_ea: CpuFullFormatEaDiagnosticSnapshot {
                extension_word: self.ff_dp,
                base: self.ff_base,
                index: self.ff_regd,
                outer_displacement: self.ff_outer,
                displacement: self.ff_disp,
                phase: self.ff_phase,
                stream_words_remaining: self.ff_stream_left,
                is_source: self.ff_is_src,
            },
            movem: CpuMovemDiagnosticSnapshot {
                remaining_mask: self.movem_mask,
                register_index: self.movem_idx,
                is_write: self.movem_is_write,
                address_register: self.movem_an_reg,
            },
            bit_field: CpuBitFieldDiagnosticSnapshot {
                buffer: self.bf_buf,
                base_address: self.bf_base_addr,
                sub_operation: self.bf_sub_op,
                data_register: self.bf_dr,
                width: self.bf_width,
                bit_offset: self.bf_bit_offset,
                bytes_total: self.bf_bytes_total,
                bytes_done: self.bf_bytes_done,
                source_value: self.bf_source_val,
                ea_mode: self.bf_ea_mode,
                ea_register: self.bf_ea_reg,
                byte_displacement: self.bf_byte_disp,
            },
            fpu: CpuFpuPipelineDiagnosticSnapshot {
                operand_buffer_high: (self.fp_mem_buf >> 64) as u64,
                operand_buffer_low: self.fp_mem_buf as u64,
                operand_bytes_total: self.fp_mem_bytes_total,
                operand_bytes_done: self.fp_mem_bytes_done,
                operand_format: self.fp_mem_format,
                operation_mode: self.fp_mem_opmode,
                destination_register: self.fp_mem_dst,
                precision: self.fp_mem_precision,
                operand_pending: self.fp_mem_pending,
                operand_store: self.fp_mem_store,
                movem_active: self.fp_movem_active,
                movem_store: self.fp_movem_store,
                movem_remaining_list: self.fp_movem_list,
                movem_current_register: self.fp_movem_cur,
                movem_address: self.fp_movem_an,
                movem_address_register: self.fp_movem_areg,
                frame_buffer: self.fp_frame,
                frame_buffer_capacity: self.fp_frame.len(),
                frame_bytes_total: self.fp_frame_total,
                frame_bytes_done: self.fp_frame_done,
                frame_address: self.fp_frame_an,
                frame_address_register: self.fp_frame_areg,
                frame_postincrement: self.fp_frame_postinc,
                frame_pending: self.fp_frame_pending,
                frame_store: self.fp_frame_store,
            },
            variant_extension_word: self.variant_ext_word,
            variant_pending_displacement: self.variant_pending_disp,
        };
        let variant = CpuVariantDiagnosticSnapshot {
            decode_hook_present: self.variant_decode_hook.is_some(),
            continue_hook_present: self.variant_continue_hook.is_some(),
            scaled_index: self.variant_scaled_index,
            six_word_frame: self.variant_six_word_frame,
            format2_vectors: self.variant_format2_vectors,
            musashi_bcd_overflow: self.variant_musashi_bcd_v,
            musashi_divide_overflow: self.variant_musashi_div_overflow,
            extended_sr_writes: self.variant_extended_sr_writes,
            unaligned_data_access: self.variant_unaligned_data_access,
            dynamic_bus_sizing: self.variant_dynamic_bus_sizing,
            format_a_group0: self.variant_format_a_group0,
            minimum_bus_clocks: self.variant_min_bus_clocks,
            constant_shift_timing: self.variant_constant_shift_timing,
            cacr_write_mask: self.variant_cacr_write_mask,
            cacr_read_zero_mask: self.variant_cacr_read_zero_mask,
            cache_disable_asserted: self.variant_cache_disable_asserted,
            um_ea_calculation_timing: self.variant_um_ea_calc_timing,
            long_branch: self.variant_long_branch,
            fpu_present: self.variant_fpu_present,
            fpu_is_68882: self.variant_fpu_is_68882,
            mmu_translation_state_present: false,
            master_stack_capable: self.regs.master_stack_capable(),
        };
        let instruction_enabled = self.regs.cacr & 0x01 != 0;
        let instruction_frozen = self.regs.cacr & 0x02 != 0;
        let instruction_state_present = self.variant_icache.is_some();
        let (valid_line_count, line_capacity, valid_word_count, word_capacity) =
            self.variant_icache.as_ref().map_or((0, 0, 0, 0), |cache| {
                (
                    cache.valid_line_count(),
                    cache.line_capacity(),
                    cache.valid_word_count(),
                    cache.word_capacity(),
                )
            });
        let cache = CpuCacheDiagnosticSnapshot {
            instruction_state_present,
            instruction_enabled,
            instruction_frozen,
            cache_disable_asserted: self.variant_cache_disable_asserted,
            instruction_hits_enabled: instruction_state_present
                && instruction_enabled
                && !self.variant_cache_disable_asserted,
            instruction_fills_enabled: instruction_state_present
                && instruction_enabled
                && !instruction_frozen
                && !self.variant_cache_disable_asserted,
            valid_line_count,
            line_capacity,
            valid_word_count,
            word_capacity,
            data_state_present: false,
        };

        CpuCoreDiagnosticSnapshot {
            d: self.regs.d,
            a: core::array::from_fn(|index| self.regs.a(index)),
            a7: self.regs.active_sp(),
            usp: self.regs.usp,
            ssp: self.regs.ssp,
            msp: self.regs.msp,
            pc: self.regs.pc,
            sr: self.regs.sr,
            status,
            control,
            fpu,
            prefetch,
            execution,
            bus,
            ipl: self.ipl,
            interrupts,
            exception,
            pipelines,
            variant,
            cache,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_copies_private_state_without_consuming_observation() {
        let mut cpu = Cpu68000::new();
        cpu.regs.d[3] = 0x1234_5678;
        cpu.regs.a[2] = 0x00AB_CDEF;
        cpu.sampled_ipl = 5;
        cpu.level7_transition_pending = true;
        cpu.opcode_at_start = 0x4E71;
        cpu.ff_dp = 0x0123;
        cpu.fp_frame_done = 7;

        let snapshot = cpu.diagnostic_snapshot();

        assert_eq!(snapshot.d[3], 0x1234_5678);
        assert_eq!(snapshot.a[2], 0x00AB_CDEF);
        assert_eq!(snapshot.interrupts.sampled_ipl, 5);
        assert!(snapshot.interrupts.level7_transition_pending);
        assert_eq!(snapshot.prefetch.opcode_at_start, 0x4E71);
        assert_eq!(snapshot.pipelines.full_format_ea.extension_word, 0x0123);
        assert_eq!(snapshot.pipelines.fpu.frame_bytes_done, 7);
        assert_eq!(cpu.take_address_error_observation(), None);
    }

    #[test]
    fn cache_snapshot_reports_counts_without_cache_payload() {
        let mut cpu = Cpu68000::new();
        let mut cache = crate::ICache::new();
        cache.fill(0x1000, true, 0x4E71);
        cache.fill(0x1002, true, 0x4E75);
        cpu.variant_icache = Some(cache);
        cpu.regs.cacr = 0x01;

        let cache = cpu.diagnostic_snapshot().cache;

        assert!(cache.instruction_state_present);
        assert!(cache.instruction_hits_enabled);
        assert!(cache.instruction_fills_enabled);
        assert_eq!(cache.valid_line_count, 1);
        assert_eq!(cache.valid_word_count, 2);
        assert_eq!(cache.line_capacity, 64);
        assert_eq!(cache.word_capacity, 128);
        assert!(!cache.data_state_present);
    }

    #[test]
    fn serialized_snapshot_is_bounded_and_keeps_established_cpu_leaves() {
        let cpu = Cpu68000::new();
        let value = serde_json::to_value(cpu.diagnostic_snapshot())
            .expect("CPU diagnostic snapshot should serialize");
        let object = value
            .as_object()
            .expect("CPU diagnostic snapshot should be an object");

        assert_eq!(object["pc"], serde_json::json!(0));
        assert_eq!(object["sr"], serde_json::json!(0x2700));
        assert_eq!(object["ipl"], serde_json::json!(0));
        assert_eq!(object["execution"]["micro_op_count"], serde_json::json!(0));
        assert_eq!(object["cache"]["valid_word_count"], serde_json::json!(0));
        assert_eq!(
            object["pipelines"]["fpu"]["frame_buffer"]
                .as_array()
                .map(Vec::len),
            Some(60),
        );
    }
}
