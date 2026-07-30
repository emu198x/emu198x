//! `AmigaLiveAccess` — chipset-agnostic chip-level read/write surface.
//!
//! The family MCP tools used to live behind the `AmigaSession::
//! machine_mut()` downcast, which panicked on any kind variant other
//! than `Aga`. That kept every chip-level tool wedded to a single
//! machine type. This trait lifts the cross-cutting surface up so
//! the tools can drive `AmigaOcs` / `AmigaEcs` / `AmigaA1200` with
//! one body — the implementations forward to each machine's existing
//! inherent methods.
//!
//! Per [`knowledge/decisions/amiga-machine-catalogue.md`], this is the
//! Step 3b plumbing for the AGA / ECS / OCS multi-variant MCP surface.
//!
//! Shape choices:
//! - **CPU**: returned as a copy via [`CpuSnapshot`] because Cpu68000
//!   and Cpu68020 are different concrete types. The hot scalar
//!   accessors (`cpu_pc`, `cpu_instruction_starts`) are spelled out
//!   separately so per-tick loops don't pay the copy cost.
//! - **Shared chips** (CIA, Paula, drive, keyboard) are exposed by
//!   reference using the shared base types from the chip crates —
//!   every machine returns the same concrete type from its inherent
//!   accessor.
//! - **Agnus** uses the OCS base type via `Deref` (AgnusEcs and
//!   AgnusAga both `Deref<Target = commodore_agnus_ocs::Agnus>`).
//! - **Denise / Memory** vary per chipset, so the trait exposes the
//!   primitive readback (`framebuffer()`, `overlay()`) rather than
//!   the chip references.
//! - **AGA-only debug logs** return `Option`-wrapped slices so the
//!   palette / bplcon0 / chipset-read inspectors degrade gracefully
//!   on OCS / ECS sessions.
//!
//! [`knowledge/decisions/amiga-machine-catalogue.md`]: ../../../../knowledge/decisions/amiga-machine-catalogue.md

use commodore_agnus_ocs::{Agnus, AgnusBusDiagnosticSnapshot};
use commodore_denise_aga::DeniseAgaDiagnosticSnapshot;
use commodore_denise_ocs::DeniseDiagnosticSnapshot;
use commodore_gary::GaryDiagnosticSnapshot;
use commodore_gayle::GayleDiagnosticSnapshot;
use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_a1200::AmigaA1200;
use machine_commodore_amiga_ecs::{AgnusEcs, AmigaEcs, DeniseEcs};
use machine_commodore_amiga_ocs::{
    AmigaFloppyDrive, AmigaInputDiagnosticSnapshot, AmigaKeyboard, AmigaOcs,
    AmigaSchedulerDiagnosticSnapshot, AmigaTrackStreamDiagnosticSnapshot, Cia, Copper,
    Msm6242RtcDiagnosticSnapshot, Paula8364,
};
use motorola_68k_common::registers::Registers;

use crate::variants::AmigaRuntimeKind;

/// Snapshot of the CPU register file + scheduler bookkeeping. Built
/// by [`AmigaLiveAccess::cpu_snapshot`] so the trait can hand back
/// CPU state without exposing the concrete Cpu68000 / Cpu68020 types.
///
/// The fields mirror the `query_cpu` tool's JSON output one-for-one.
#[derive(Debug, Clone, Copy)]
pub struct CpuSnapshot {
    pub regs: Registers,
    pub instr_start_pc: u32,
    pub ipl: u8,
    pub interrupts_taken: u64,
    pub exc_vector: Option<u8>,
    pub in_followup: bool,
    pub followup_tag: u8,
    pub instruction_starts: u64,
}

/// AGA Lisa (AGA Denise) state snapshot for the `query_aga` tool.
/// Returned by [`AmigaLiveAccess::aga_lisa`] as `Some` only on an AGA
/// session; OCS / ECS return `None`. Copies the AGA-only register state
/// out so the tool body stays chipset-agnostic instead of downcasting to
/// the concrete A1200.
#[derive(Debug, Clone)]
pub struct AgaLisaSnapshot {
    pub deniseid: u16,
    pub bplcon3: u16,
    pub bplcon4: u16,
    pub spr_width: u8,
    pub ham_prev_rgb24: u32,
    pub programmed_hblank_active: bool,
    /// 256-entry 24-bit palette (8 banks × 32), stored `0x00RRGGBB`.
    pub palette_24: [u32; 256],
}

/// Complete read-only view of the ECS Agnus timing layer. AGA Alice inherits
/// the same state, so AGA sessions return this snapshot as well.
#[derive(Debug, Clone, Copy)]
pub struct EcsAgnusTimingSnapshot {
    pub beamcon0: u16,
    pub htotal: u16,
    pub hsstop: u16,
    pub hbstrt: u16,
    pub hbstop: u16,
    pub vtotal: u16,
    pub vsstop: u16,
    pub vbstrt: u16,
    pub vbstop: u16,
    pub hsstrt: u16,
    pub vsstrt: u16,
    pub diwhigh: u16,
    pub diwhigh_written: bool,
    pub bltsizv: u16,
    pub bltsizh: u16,
    pub programmed_vertical_accessed: bool,
    pub programmed_vblank_active: bool,
    pub programmed_vblank_start_event: bool,
    pub programmed_vblank_stop_event: bool,
    pub programmed_hblank_active: bool,
    pub programmed_hblank_routed_active: bool,
    pub vertical_diw_active: bool,
    pub current_line_ccks: u16,
    pub copper_comparator_hpos: u16,
    pub pal_enabled: bool,
    pub dual_enabled: bool,
    pub varbeamen_enabled: bool,
    pub varvben_enabled: bool,
    pub varvsyen_enabled: bool,
    pub varhsyen_enabled: bool,
    pub cscben_enabled: bool,
    pub varcsyen_enabled: bool,
    pub harddis_enabled: bool,
    pub blanken_enabled: bool,
    pub loldis_enabled: bool,
    pub lpendis_enabled: bool,
    pub csytrue_enabled: bool,
    pub vsytrue_enabled: bool,
    pub hsytrue_enabled: bool,
    pub harddis_hblank_window_active: bool,
    pub vblank_window_active: bool,
    pub hsync_window_active: bool,
    pub vsync_window_active: bool,
    pub sync_pin_hsync: bool,
    pub sync_pin_vsync: bool,
    pub sync_pin_csync: bool,
    pub sync_pin_blank: bool,
}

/// Complete read-only view of the ECS Denise layer shared by Super Denise and
/// AGA Lisa, plus the board-composed programmable-HBLANK output level.
#[derive(Debug, Clone, Copy)]
pub struct EnhancedDeniseSnapshot {
    pub deniseid: u16,
    pub bplcon3: u16,
    pub ecsena_enabled: bool,
    pub extblken_enabled: bool,
    pub shres_enabled: bool,
    pub bplhwrm_enabled: bool,
    pub sprhwrm_enabled: bool,
    pub bplcon3_extensions_enabled: bool,
    pub border_blank_enabled: bool,
    pub border_opaque_enabled: bool,
    pub killehb_enabled: bool,
    pub programmed_hblank_active: bool,
}

fn ecs_agnus_timing_snapshot(agnus: &AgnusEcs) -> EcsAgnusTimingSnapshot {
    let pins = agnus.sync_pin_levels(agnus.hpos, agnus.vpos);
    EcsAgnusTimingSnapshot {
        beamcon0: agnus.beamcon0(),
        htotal: agnus.htotal(),
        hsstop: agnus.hsstop(),
        hbstrt: agnus.hbstrt(),
        hbstop: agnus.hbstop(),
        vtotal: agnus.vtotal(),
        vsstop: agnus.vsstop(),
        vbstrt: agnus.vbstrt(),
        vbstop: agnus.vbstop(),
        hsstrt: agnus.hsstrt(),
        vsstrt: agnus.vsstrt(),
        diwhigh: agnus.diwhigh(),
        diwhigh_written: agnus.diwhigh_written(),
        bltsizv: agnus.bltsizv,
        bltsizh: agnus.bltsizh,
        programmed_vertical_accessed: agnus.programmed_vertical_accessed(),
        programmed_vblank_active: agnus.programmed_vblank_active(),
        programmed_vblank_start_event: agnus.programmed_vblank_start_event(),
        programmed_vblank_stop_event: agnus.programmed_vblank_stop_event(),
        programmed_hblank_active: agnus.programmed_hblank_active(),
        programmed_hblank_routed_active: agnus.programmed_hblank_routed_active(),
        vertical_diw_active: agnus.vertical_diw_active(),
        current_line_ccks: agnus.current_line_ccks(),
        copper_comparator_hpos: agnus.copper_comparator_hpos(),
        pal_enabled: agnus.pal_enabled(),
        dual_enabled: agnus.dual_enabled(),
        varbeamen_enabled: agnus.varbeamen_enabled(),
        varvben_enabled: agnus.varvben_enabled(),
        varvsyen_enabled: agnus.varvsyen_enabled(),
        varhsyen_enabled: agnus.varhsyen_enabled(),
        cscben_enabled: agnus.cscben_enabled(),
        varcsyen_enabled: agnus.varcsyen_enabled(),
        harddis_enabled: agnus.harddis_enabled(),
        blanken_enabled: agnus.blanken_enabled(),
        loldis_enabled: agnus.loldis_enabled(),
        lpendis_enabled: agnus.lpendis_enabled(),
        csytrue_enabled: agnus.csytrue_enabled(),
        vsytrue_enabled: agnus.vsytrue_enabled(),
        hsytrue_enabled: agnus.hsytrue_enabled(),
        harddis_hblank_window_active: agnus.harddis_hblank_window_active(agnus.hpos),
        vblank_window_active: agnus.vblank_window_active(agnus.vpos),
        hsync_window_active: agnus.hsync_window_active(agnus.hpos),
        vsync_window_active: agnus.vsync_window_active(agnus.vpos),
        sync_pin_hsync: pins.hsync,
        sync_pin_vsync: pins.vsync,
        sync_pin_csync: pins.csync,
        sync_pin_blank: pins.blank,
    }
}

fn enhanced_denise_snapshot(
    denise: &DeniseEcs,
    deniseid: u16,
    bplcon0: u16,
    programmed_hblank_active: bool,
) -> EnhancedDeniseSnapshot {
    EnhancedDeniseSnapshot {
        deniseid,
        bplcon3: denise.bplcon3,
        ecsena_enabled: (bplcon0 & 0x0001) != 0,
        extblken_enabled: denise.extblken_enabled(),
        shres_enabled: denise.shres_enabled(),
        bplhwrm_enabled: denise.bplhwrm_enabled(),
        sprhwrm_enabled: denise.sprhwrm_enabled(),
        bplcon3_extensions_enabled: denise.bplcon3_extensions_enabled(),
        border_blank_enabled: denise.border_blank_enabled(),
        border_opaque_enabled: denise.border_opaque_enabled(),
        killehb_enabled: denise.killehb_enabled(),
        programmed_hblank_active,
    }
}

/// Watch-log entry shape: `(tick, pc, addr, value, is_word)`.
///
/// Re-aliased here so MCP tool bodies can name the type without
/// dragging in the machine-crate shape. Matches the field tuple on
/// each machine's `debug_watch_writes` field.
pub type WatchLogEntry = (u64, u32, u32, u16, bool);

/// DSK debug-log entry shape: `(tick, pc, register, value)`.
pub type DskLogEntry = (u64, u32, u16, u16);

/// Palette-log entry shape: `(tick, pc, color_index, value,
/// bplcon3_at_write)`.
///
/// The fifth field is `Some(bplcon3)` on chipsets that have a real
/// BPLCON3 register (ECS, AGA) and `None` on OCS where the address
/// `$0106` isn't backed by any register. Tools decode bank / loct
/// from the BPLCON3 word when present.
pub type PaletteLogEntry = (u64, u32, u16, u16, Option<u16>);

/// AGA BPLCON0 log shape: `(tick, pc, value)`.
pub type Bplcon0LogEntry = (u64, u32, u16);

/// AGA chipset-register-read log shape: `(tick, pc, addr, value)`.
pub type RegReadLogEntry = (u64, u32, u16, u16);

/// Chipset-register write log shape:
/// `(tick, pc, addr, offset, raw_val, is_word)`. `addr` is the full
/// 24-bit bus address; `offset` is the chipset-register offset
/// (`addr & 0x1FF`). `is_word` distinguishes word vs byte writes.
pub type CustomWriteEntry = (u64, u32, u32, u16, u16, bool);

/// Copper MOVE diagnostic entry:
/// `(tick, vpos, hpos, custom_register_offset, value)`.
pub type CopperMoveLogEntry = (u64, u16, u16, u16, u16);

/// INTENA / DMACON transition log entry:
/// `(tick, pc, written_value, value_before, value_after)`.
pub type RegisterTransitionLogEntry = (u64, u32, u16, u16, u16);

/// Copper-list pointer update log entry: `(tick, pc, new_pointer)`.
pub type CopperPointerLogEntry = (u64, u32, u32);

/// Blitter-start log entry:
/// `(tick, pc, bltcon0, bltcon1, apt, bpt, cpt, dpt, bltsize)`.
pub type BlitLogEntry = (u64, u32, u16, u16, u32, u32, u32, u32, u16);

/// CIA register-write log entry: `(tick, pc, register, value)`.
pub type CiaRegisterWriteLogEntry = (u64, u32, u8, u8);

/// RTC bus-access log entry:
/// `(tick, pc, address, is_read, is_word, value)`.
pub type RtcAccessLogEntry = (u64, u32, u32, bool, bool, u16);

/// Chipset-agnostic read/write surface used by the family MCP tools.
///
/// Implemented by every concrete machine struct (`AmigaOcs`,
/// `AmigaEcs`, `AmigaA1200`) and by [`AmigaRuntimeKind`] itself so
/// the MCP session can hand a single `&mut dyn AmigaLiveAccess` to
/// every tool body regardless of the active chipset.
pub trait AmigaLiveAccess {
    // ---------- CPU ----------

    /// Full CPU debug snapshot — copies the register file plus the
    /// scheduler bookkeeping the `query_cpu` / `step` tools need.
    fn cpu_snapshot(&self) -> CpuSnapshot;

    /// Fast scalar PC read — used by `run_until_pc` and `step` so the
    /// hot loop doesn't pay the `CpuSnapshot` copy cost.
    fn cpu_pc(&self) -> u32;

    /// Fast scalar instruction-start counter read — used by `step` /
    /// `run_until_any_pc` for instruction-boundary detection.
    fn cpu_instruction_starts(&self) -> u64;

    /// `true` iff the CPU is currently between instructions
    /// (not in a follow-up cycle). Used by single-step logic.
    fn cpu_in_followup(&self) -> bool;

    // ---------- lifecycle ----------

    /// Advance the machine by one master / 4 tick.
    fn tick(&mut self);

    /// Current tick counter.
    fn tick_count(&self) -> u64;

    /// Board scheduler, CPU-domain and pending instruction-boundary state.
    fn scheduler_diagnostic_snapshot(&self) -> AmigaSchedulerDiagnosticSnapshot;

    /// Board-level encoded-track cache and delivery-pacer state.
    fn track_stream_diagnostic_snapshot(&self) -> AmigaTrackStreamDiagnosticSnapshot;

    /// Board-level controller-port counters and host input latches.
    fn input_diagnostic_snapshot(&self) -> AmigaInputDiagnosticSnapshot;

    // ---------- chipset register snapshot ----------

    fn intena(&self) -> u16;
    fn intreq(&self) -> u16;
    fn dmacon(&self) -> u16;
    fn adkcon(&self) -> u16;
    fn bplcon0(&self) -> u16;
    fn color(&self, idx: usize) -> u16;
    fn overlay(&self) -> bool;
    fn copper_pc(&self) -> u32;
    fn copper_cop1lc(&self) -> u32;
    fn copper_cop2lc(&self) -> u32;

    // ---------- chip references (shared base types) ----------

    fn agnus(&self) -> &Agnus;

    /// Current arbitration plan plus recorded same-CCK bus use. Enhanced
    /// variants override this to apply their programmable vertical timing.
    fn agnus_bus_diagnostic_snapshot(&self) -> AgnusBusDiagnosticSnapshot {
        self.agnus().bus_diagnostic_snapshot()
    }

    /// ECS Agnus programmable timing state, inherited by Alice. OCS sessions
    /// return `None`.
    fn ecs_agnus_timing(&self) -> Option<EcsAgnusTimingSnapshot> {
        None
    }

    /// ECS Denise state shared by Super Denise and Lisa, including the
    /// board-composed programmable horizontal-blank output. OCS sessions
    /// return `None`.
    fn enhanced_denise(&self) -> Option<EnhancedDeniseSnapshot> {
        None
    }

    /// Complete common Denise rendering-core state.
    fn denise_diagnostic_snapshot(&self) -> DeniseDiagnosticSnapshot;

    /// Complete mutable state owned by AGA Lisa outside its wrapped ECS/OCS
    /// core. OCS and ECS machines return `None`.
    fn aga_denise_diagnostic_snapshot(&self) -> Option<DeniseAgaDiagnosticSnapshot> {
        None
    }

    fn cia_a(&self) -> &Cia;
    fn cia_b(&self) -> &Cia;
    fn rtc_diagnostic_snapshot(&self) -> Msm6242RtcDiagnosticSnapshot;
    fn paula(&self) -> &Paula8364;
    fn drive(&self) -> &AmigaFloppyDrive;
    fn keyboard(&self) -> &AmigaKeyboard;
    fn copper(&self) -> &Copper;

    /// Complete motherboard address-decoder configuration.
    fn gary_diagnostic_snapshot(&self) -> GaryDiagnosticSnapshot;

    /// Complete Gayle state on A600 and A1200 boards. Other variants do not
    /// contain Gayle.
    fn gayle_diagnostic_snapshot(&self) -> Option<GayleDiagnosticSnapshot> {
        None
    }

    // ---------- video ----------

    /// Borrow the chipset framebuffer as ARGB pixels.
    fn framebuffer(&self) -> &[u32];

    /// `(width, height)` of the chipset framebuffer.
    fn framebuffer_dims(&self) -> (u32, u32);

    // ---------- memory ----------

    fn read_word(&self, addr: u32) -> u16;
    fn read_long(&self, addr: u32) -> u32;
    fn poke_byte(&mut self, addr: u32, value: u8);
    fn poke_word(&mut self, addr: u32, value: u16);

    // ---------- watch / debug logs ----------

    /// Arm or disarm the memory-write watch. `None` clears.
    fn set_watch(&mut self, base_len: Option<(u32, u32)>);

    /// Current armed watch range `(base, len)`, if any.
    fn watch_range(&self) -> Option<(u32, u32)>;

    /// Current memory-write watch log.
    fn watch_log(&self) -> &[WatchLogEntry];

    /// DSK-register write log — present on every chipset (OCS, ECS,
    /// AGA all track disk-register writes for boot diagnostics).
    fn dsk_write_log(&self) -> &[DskLogEntry];

    // ---------- chipset-trace debug logs ----------
    //
    // Mirrored across OCS / ECS / AGA — every chipset now carries
    // the same three tracers. Tools can consume the slices directly
    // without an Option dance.

    /// Palette-log: every write to COLOR ($180..$1BE), BPLCON3
    /// ($0106), BPLCON4 ($010C). The fifth field is `Some(bplcon3)`
    /// on chipsets that have a BPLCON3 register (ECS, AGA) and
    /// `None` on OCS.
    fn palette_log(&self) -> &[PaletteLogEntry];

    /// BPLCON0-log: every write to $0100.
    fn bplcon0_log(&self) -> &[Bplcon0LogEntry];

    /// Chipset-register-read log: every CPU read from a custom-chip
    /// register. Useful for watching how an app or Kickstart probes
    /// the chipset (e.g., reading DENISEID to detect AGA).
    fn reg_read_log(&self) -> &[RegReadLogEntry];

    /// Chipset-register-write log: every CPU write to a custom-chip
    /// register that goes through `dispatch_custom_register`'s write
    /// arm. Includes byte vs word, full bus address, and the offset
    /// inside the custom-register window. Bounded at 1,048,576
    /// entries (~24 MB) on each chip stack — silently truncates past
    /// that. Lets callers answer "when did COP2LC change?", "what
    /// were all the writes to $DFF0xx during the boot?", etc., in
    /// one shot rather than polling `query_chipset`.
    fn custom_write_log(&self) -> &[CustomWriteEntry];

    /// Copper MOVE events routed through the custom-register dispatcher.
    fn copper_move_log(&self) -> &[CopperMoveLogEntry];

    /// Per-register custom-chip read totals.
    fn register_read_counts(&self) -> &std::collections::HashMap<u16, u64>;

    /// Highest INTENA value observed after a CPU write.
    fn peak_intena(&self) -> u16;

    /// Total CPU writes to INTENA, including no-op writes.
    fn intena_write_count(&self) -> u64;

    /// INTENA writes that changed the stored value.
    fn intena_log(&self) -> &[RegisterTransitionLogEntry];

    /// COP1LC pointer updates.
    fn cop1lc_log(&self) -> &[CopperPointerLogEntry];

    /// COP2LC pointer updates.
    fn cop2lc_log(&self) -> &[CopperPointerLogEntry];

    /// DMACON writes that changed the stored value.
    fn dmacon_log(&self) -> &[RegisterTransitionLogEntry];

    /// Total blitter start-register writes.
    fn blitter_start_count(&self) -> u64;

    /// Captured blitter starts and their register context.
    fn blitter_log(&self) -> &[BlitLogEntry];

    /// CIA-A register writes.
    fn cia_a_write_log(&self) -> &[CiaRegisterWriteLogEntry];

    /// CIA-B register writes.
    fn cia_b_write_log(&self) -> &[CiaRegisterWriteLogEntry];

    /// CIA-A register-read totals, when the machine records them.
    fn cia_a_read_counts(&self) -> Option<&std::collections::HashMap<u8, u64>> {
        None
    }

    /// CIA-B register-read totals, when the machine records them.
    fn cia_b_read_counts(&self) -> Option<&std::collections::HashMap<u8, u64>> {
        None
    }

    /// Bounded RTC bus-access log.
    fn rtc_access_log(&self) -> &[RtcAccessLogEntry];

    /// Compatibility projection used by the existing AGA Copper-list tool.
    /// All chipsets now expose the common Copper through [`Self::copper`].
    fn aga_copper(&self) -> Option<&Copper>;

    // ---------- media ----------

    /// Canonical DF0 mount. `writable = false` mounts read-only — an
    /// archive that reports `/DSKPROT` and authentically rejects a SAVE
    /// (#97); see `knowledge/decisions/disk-save-write-back.md`.
    fn insert_floppy0_writable(&mut self, adf: Adf, change_pending: bool, writable: bool);

    /// Insert DF0 writable (the common case) — delegates to
    /// `insert_floppy0_writable`.
    fn insert_floppy0(&mut self, adf: Adf, change_pending: bool) {
        self.insert_floppy0_writable(adf, change_pending, true);
    }
    fn eject_floppy0(&mut self);

    /// Decode DF0's current in-memory image back to ADF bytes so the
    /// host can persist a SAVE. `None` when DF0 is empty. The host
    /// chooses where the bytes land (sidecar by default — see
    /// `knowledge/decisions/disk-save-write-back.md`).
    fn save_floppy0_image(&self) -> Option<Vec<u8>> {
        self.drive().save_adf()
    }

    // ---------- instruction-boundary CPU trace ----------
    //
    // The trace lives on `AmigaRuntime` and is captured by its tick
    // funnel (see `cpu_trace.rs`). Only the `AmigaRuntimeKind` impl
    // overrides these; the bare-machine impls (`AmigaOcs` etc.) use the
    // inert defaults — the trace is a runtime-level concern and tools
    // only ever hold the runtime kind.

    /// Arm the instruction trace with an optional inclusive PC filter
    /// and an entry cap. Clears any prior trace.
    fn cpu_trace_arm(&mut self, _pc_filter: Option<(u32, u32)>, _max_entries: usize) {}

    /// Stop recording, keeping captured entries. Returns the count held.
    fn cpu_trace_disarm(&mut self) -> usize {
        0
    }

    /// Discard captured entries without disarming. Returns the count
    /// dropped.
    fn cpu_trace_clear(&mut self) -> usize {
        0
    }

    /// Whether the trace is recording.
    fn cpu_trace_armed(&self) -> bool {
        false
    }

    /// Current entry cap.
    fn cpu_trace_max_entries(&self) -> usize {
        0
    }

    /// Captured entries, oldest first.
    fn cpu_trace_entries(&self) -> &[crate::CpuTraceEntry] {
        &[]
    }

    // ---------- AGA-only state ----------

    /// AGA Lisa register + palette snapshot, or `None` on OCS / ECS.
    /// Lets the `query_aga` tool reach Lisa state without downcasting to
    /// the concrete A1200.
    fn aga_lisa(&self) -> Option<AgaLisaSnapshot> {
        None
    }
}

// ===================================================================
// AmigaOcs impl
// ===================================================================

impl AmigaLiveAccess for AmigaOcs {
    fn cpu_snapshot(&self) -> CpuSnapshot {
        let cpu = self.cpu();
        CpuSnapshot {
            regs: cpu.regs,
            instr_start_pc: cpu.instr_start_pc,
            ipl: cpu.ipl,
            interrupts_taken: cpu.interrupts_taken,
            exc_vector: cpu.exc_vector,
            in_followup: cpu.in_followup,
            followup_tag: cpu.followup_tag,
            instruction_starts: cpu.instruction_starts,
        }
    }

    fn cpu_pc(&self) -> u32 {
        self.cpu().regs.pc
    }

    fn cpu_instruction_starts(&self) -> u64 {
        self.cpu().instruction_starts
    }

    fn cpu_in_followup(&self) -> bool {
        self.cpu().in_followup
    }

    fn tick(&mut self) {
        AmigaOcs::tick(self);
    }

    fn tick_count(&self) -> u64 {
        AmigaOcs::tick_count(self)
    }

    fn scheduler_diagnostic_snapshot(&self) -> AmigaSchedulerDiagnosticSnapshot {
        AmigaOcs::scheduler_diagnostic_snapshot(self)
    }

    fn track_stream_diagnostic_snapshot(&self) -> AmigaTrackStreamDiagnosticSnapshot {
        AmigaOcs::track_stream_diagnostic_snapshot(self)
    }

    fn input_diagnostic_snapshot(&self) -> AmigaInputDiagnosticSnapshot {
        AmigaOcs::input_diagnostic_snapshot(self)
    }

    fn intena(&self) -> u16 {
        AmigaOcs::intena(self)
    }

    fn intreq(&self) -> u16 {
        AmigaOcs::intreq(self)
    }

    fn dmacon(&self) -> u16 {
        AmigaOcs::dmacon(self)
    }

    fn adkcon(&self) -> u16 {
        AmigaOcs::adkcon(self)
    }

    fn bplcon0(&self) -> u16 {
        AmigaOcs::bplcon0(self)
    }

    fn color(&self, idx: usize) -> u16 {
        AmigaOcs::color(self, idx)
    }

    fn overlay(&self) -> bool {
        self.memory().overlay()
    }

    fn copper_pc(&self) -> u32 {
        self.copper().pc
    }

    fn copper_cop1lc(&self) -> u32 {
        self.copper().cop1lc
    }

    fn copper_cop2lc(&self) -> u32 {
        self.copper().cop2lc
    }

    fn agnus(&self) -> &Agnus {
        AmigaOcs::agnus(self)
    }

    fn agnus_bus_diagnostic_snapshot(&self) -> AgnusBusDiagnosticSnapshot {
        AmigaOcs::agnus(self).bus_diagnostic_snapshot()
    }

    fn denise_diagnostic_snapshot(&self) -> DeniseDiagnosticSnapshot {
        self.denise().ocs.diagnostic_snapshot()
    }

    fn cia_a(&self) -> &Cia {
        AmigaOcs::cia_a(self)
    }

    fn cia_b(&self) -> &Cia {
        AmigaOcs::cia_b(self)
    }

    fn rtc_diagnostic_snapshot(&self) -> Msm6242RtcDiagnosticSnapshot {
        AmigaOcs::rtc_diagnostic_snapshot(self)
    }

    fn paula(&self) -> &Paula8364 {
        AmigaOcs::paula(self)
    }

    fn drive(&self) -> &AmigaFloppyDrive {
        AmigaOcs::drive(self)
    }

    fn keyboard(&self) -> &AmigaKeyboard {
        AmigaOcs::keyboard(self)
    }

    fn copper(&self) -> &Copper {
        AmigaOcs::copper(self)
    }

    fn gary_diagnostic_snapshot(&self) -> GaryDiagnosticSnapshot {
        self.gary().diagnostic_snapshot()
    }

    fn framebuffer(&self) -> &[u32] {
        self.denise().framebuffer()
    }

    fn framebuffer_dims(&self) -> (u32, u32) {
        (
            machine_commodore_amiga_ocs::FB_WIDTH,
            machine_commodore_amiga_ocs::FB_HEIGHT,
        )
    }

    fn read_word(&self, addr: u32) -> u16 {
        AmigaOcs::read_word(self, addr)
    }

    fn read_long(&self, addr: u32) -> u32 {
        AmigaOcs::read_long(self, addr)
    }

    fn poke_byte(&mut self, addr: u32, value: u8) {
        AmigaOcs::poke_byte(self, addr, value);
    }

    fn poke_word(&mut self, addr: u32, value: u16) {
        AmigaOcs::poke_word(self, addr, value);
    }

    fn set_watch(&mut self, base_len: Option<(u32, u32)>) {
        self.debug_watch_addr = base_len;
        self.debug_watch_writes.clear();
    }

    fn watch_range(&self) -> Option<(u32, u32)> {
        self.debug_watch_addr
    }

    fn watch_log(&self) -> &[WatchLogEntry] {
        &self.debug_watch_writes
    }

    fn dsk_write_log(&self) -> &[DskLogEntry] {
        &self.debug_dsk_log
    }

    fn palette_log(&self) -> &[PaletteLogEntry] {
        &self.debug_palette_log
    }

    fn bplcon0_log(&self) -> &[Bplcon0LogEntry] {
        &self.debug_bplcon0_log
    }

    fn reg_read_log(&self) -> &[RegReadLogEntry] {
        &self.debug_reg_read_log
    }

    fn custom_write_log(&self) -> &[CustomWriteEntry] {
        &self.debug_custom_write_log
    }

    fn copper_move_log(&self) -> &[CopperMoveLogEntry] {
        &self.debug_copper_move_log
    }

    fn register_read_counts(&self) -> &std::collections::HashMap<u16, u64> {
        &self.debug_reg_read_counts
    }

    fn peak_intena(&self) -> u16 {
        self.debug_peak_intena
    }

    fn intena_write_count(&self) -> u64 {
        self.debug_intena_writes
    }

    fn intena_log(&self) -> &[RegisterTransitionLogEntry] {
        &self.debug_intena_log
    }

    fn cop1lc_log(&self) -> &[CopperPointerLogEntry] {
        &self.debug_cop1lc_log
    }

    fn cop2lc_log(&self) -> &[CopperPointerLogEntry] {
        &self.debug_cop2lc_log
    }

    fn dmacon_log(&self) -> &[RegisterTransitionLogEntry] {
        &self.debug_dmacon_log
    }

    fn blitter_start_count(&self) -> u64 {
        self.debug_blit_starts
    }

    fn blitter_log(&self) -> &[BlitLogEntry] {
        &self.debug_blit_log
    }

    fn cia_a_write_log(&self) -> &[CiaRegisterWriteLogEntry] {
        &self.debug_cia_a_cr_log
    }

    fn cia_b_write_log(&self) -> &[CiaRegisterWriteLogEntry] {
        &self.debug_cia_b_cr_log
    }

    fn rtc_access_log(&self) -> &[RtcAccessLogEntry] {
        &self.debug_rtc_log
    }

    fn aga_copper(&self) -> Option<&Copper> {
        None
    }

    fn insert_floppy0_writable(&mut self, adf: Adf, change_pending: bool, writable: bool) {
        self.mount_adf(adf, change_pending, writable);
    }

    fn eject_floppy0(&mut self) {
        AmigaOcs::eject_disk(self);
    }
}

// ===================================================================
// AmigaEcs impl
// ===================================================================
//
// Mechanically identical to the OCS impl — the chip-level differences
// (BEAMCON0, BPLCON3) are absorbed by AgnusEcs / DeniseEcs via Deref.

impl AmigaLiveAccess for AmigaEcs {
    fn cpu_snapshot(&self) -> CpuSnapshot {
        let cpu = self.cpu();
        CpuSnapshot {
            regs: cpu.regs,
            instr_start_pc: cpu.instr_start_pc,
            ipl: cpu.ipl,
            interrupts_taken: cpu.interrupts_taken,
            exc_vector: cpu.exc_vector,
            in_followup: cpu.in_followup,
            followup_tag: cpu.followup_tag,
            instruction_starts: cpu.instruction_starts,
        }
    }

    fn cpu_pc(&self) -> u32 {
        self.cpu().regs.pc
    }

    fn cpu_instruction_starts(&self) -> u64 {
        self.cpu().instruction_starts
    }

    fn cpu_in_followup(&self) -> bool {
        self.cpu().in_followup
    }

    fn tick(&mut self) {
        AmigaEcs::tick(self);
    }

    fn tick_count(&self) -> u64 {
        AmigaEcs::tick_count(self)
    }

    fn scheduler_diagnostic_snapshot(&self) -> AmigaSchedulerDiagnosticSnapshot {
        AmigaEcs::scheduler_diagnostic_snapshot(self)
    }

    fn track_stream_diagnostic_snapshot(&self) -> AmigaTrackStreamDiagnosticSnapshot {
        AmigaEcs::track_stream_diagnostic_snapshot(self)
    }

    fn input_diagnostic_snapshot(&self) -> AmigaInputDiagnosticSnapshot {
        AmigaEcs::input_diagnostic_snapshot(self)
    }

    fn intena(&self) -> u16 {
        AmigaEcs::intena(self)
    }

    fn intreq(&self) -> u16 {
        AmigaEcs::intreq(self)
    }

    fn dmacon(&self) -> u16 {
        AmigaEcs::dmacon(self)
    }

    fn adkcon(&self) -> u16 {
        AmigaEcs::adkcon(self)
    }

    fn bplcon0(&self) -> u16 {
        AmigaEcs::bplcon0(self)
    }

    fn color(&self, idx: usize) -> u16 {
        AmigaEcs::color(self, idx)
    }

    fn overlay(&self) -> bool {
        self.memory().overlay()
    }

    fn copper_pc(&self) -> u32 {
        self.copper().pc
    }

    fn copper_cop1lc(&self) -> u32 {
        self.copper().cop1lc
    }

    fn copper_cop2lc(&self) -> u32 {
        self.copper().cop2lc
    }

    fn agnus(&self) -> &Agnus {
        // The inherent accessor already returns &commodore_agnus_ocs::Agnus
        // (the OCS-base type) — AgnusEcs is the wrapper that lives in
        // the struct field, but its public accessor projects the inner
        // OCS Agnus.
        AmigaEcs::agnus(self)
    }

    fn agnus_bus_diagnostic_snapshot(&self) -> AgnusBusDiagnosticSnapshot {
        let agnus = self.agnus_ecs();
        agnus.bus_diagnostic_snapshot_for_plan(agnus.cck_bus_plan())
    }

    fn ecs_agnus_timing(&self) -> Option<EcsAgnusTimingSnapshot> {
        Some(ecs_agnus_timing_snapshot(self.agnus_ecs()))
    }

    fn enhanced_denise(&self) -> Option<EnhancedDeniseSnapshot> {
        let agnus = self.agnus_ecs();
        let denise = self.denise_ecs();
        let output_active = agnus.programmed_hblank_routed_active()
            && agnus.blanken_enabled()
            && (self.bplcon0() & 0x0001) != 0
            && denise.extblken_enabled();
        Some(enhanced_denise_snapshot(
            denise,
            denise.deniseid(),
            self.bplcon0(),
            output_active,
        ))
    }

    fn denise_diagnostic_snapshot(&self) -> DeniseDiagnosticSnapshot {
        self.denise().ocs.diagnostic_snapshot()
    }

    fn cia_a(&self) -> &Cia {
        AmigaEcs::cia_a(self)
    }

    fn cia_b(&self) -> &Cia {
        AmigaEcs::cia_b(self)
    }

    fn rtc_diagnostic_snapshot(&self) -> Msm6242RtcDiagnosticSnapshot {
        AmigaEcs::rtc_diagnostic_snapshot(self)
    }

    fn paula(&self) -> &Paula8364 {
        AmigaEcs::paula(self)
    }

    fn drive(&self) -> &AmigaFloppyDrive {
        AmigaEcs::drive(self)
    }

    fn keyboard(&self) -> &AmigaKeyboard {
        AmigaEcs::keyboard(self)
    }

    fn copper(&self) -> &Copper {
        AmigaEcs::copper(self)
    }

    fn gary_diagnostic_snapshot(&self) -> GaryDiagnosticSnapshot {
        self.gary().diagnostic_snapshot()
    }

    fn gayle_diagnostic_snapshot(&self) -> Option<GayleDiagnosticSnapshot> {
        AmigaEcs::gayle_diagnostic_snapshot(self)
    }

    fn framebuffer(&self) -> &[u32] {
        self.denise().framebuffer()
    }

    fn framebuffer_dims(&self) -> (u32, u32) {
        (
            machine_commodore_amiga_ocs::FB_WIDTH,
            machine_commodore_amiga_ocs::FB_HEIGHT,
        )
    }

    fn read_word(&self, addr: u32) -> u16 {
        AmigaEcs::read_word(self, addr)
    }

    fn read_long(&self, addr: u32) -> u32 {
        AmigaEcs::read_long(self, addr)
    }

    fn poke_byte(&mut self, addr: u32, value: u8) {
        AmigaEcs::poke_byte(self, addr, value);
    }

    fn poke_word(&mut self, addr: u32, value: u16) {
        AmigaEcs::poke_word(self, addr, value);
    }

    fn set_watch(&mut self, base_len: Option<(u32, u32)>) {
        self.debug_watch_addr = base_len;
        self.debug_watch_writes.clear();
    }

    fn watch_range(&self) -> Option<(u32, u32)> {
        self.debug_watch_addr
    }

    fn watch_log(&self) -> &[WatchLogEntry] {
        &self.debug_watch_writes
    }

    fn dsk_write_log(&self) -> &[DskLogEntry] {
        &self.debug_dsk_log
    }

    fn palette_log(&self) -> &[PaletteLogEntry] {
        &self.debug_palette_log
    }

    fn bplcon0_log(&self) -> &[Bplcon0LogEntry] {
        &self.debug_bplcon0_log
    }

    fn reg_read_log(&self) -> &[RegReadLogEntry] {
        &self.debug_reg_read_log
    }

    fn custom_write_log(&self) -> &[CustomWriteEntry] {
        &self.debug_custom_write_log
    }

    fn copper_move_log(&self) -> &[CopperMoveLogEntry] {
        &self.debug_copper_move_log
    }

    fn register_read_counts(&self) -> &std::collections::HashMap<u16, u64> {
        &self.debug_reg_read_counts
    }

    fn peak_intena(&self) -> u16 {
        self.debug_peak_intena
    }

    fn intena_write_count(&self) -> u64 {
        self.debug_intena_writes
    }

    fn intena_log(&self) -> &[RegisterTransitionLogEntry] {
        &self.debug_intena_log
    }

    fn cop1lc_log(&self) -> &[CopperPointerLogEntry] {
        &self.debug_cop1lc_log
    }

    fn cop2lc_log(&self) -> &[CopperPointerLogEntry] {
        &self.debug_cop2lc_log
    }

    fn dmacon_log(&self) -> &[RegisterTransitionLogEntry] {
        &self.debug_dmacon_log
    }

    fn blitter_start_count(&self) -> u64 {
        self.debug_blit_starts
    }

    fn blitter_log(&self) -> &[BlitLogEntry] {
        &self.debug_blit_log
    }

    fn cia_a_write_log(&self) -> &[CiaRegisterWriteLogEntry] {
        &self.debug_cia_a_cr_log
    }

    fn cia_b_write_log(&self) -> &[CiaRegisterWriteLogEntry] {
        &self.debug_cia_b_cr_log
    }

    fn rtc_access_log(&self) -> &[RtcAccessLogEntry] {
        &self.debug_rtc_log
    }

    fn aga_copper(&self) -> Option<&Copper> {
        None
    }

    fn insert_floppy0_writable(&mut self, adf: Adf, change_pending: bool, writable: bool) {
        self.mount_adf(adf, change_pending, writable);
    }

    fn eject_floppy0(&mut self) {
        AmigaEcs::eject_disk(self);
    }
}

// ===================================================================
// AmigaA1200 impl
// ===================================================================

impl AmigaLiveAccess for AmigaA1200 {
    fn cpu_snapshot(&self) -> CpuSnapshot {
        let cpu = self.cpu();
        CpuSnapshot {
            regs: cpu.regs,
            instr_start_pc: cpu.instr_start_pc,
            ipl: cpu.ipl,
            interrupts_taken: cpu.interrupts_taken,
            exc_vector: cpu.exc_vector,
            in_followup: cpu.in_followup,
            followup_tag: cpu.followup_tag,
            instruction_starts: cpu.instruction_starts,
        }
    }

    fn cpu_pc(&self) -> u32 {
        self.cpu().regs.pc
    }

    fn cpu_instruction_starts(&self) -> u64 {
        self.cpu().instruction_starts
    }

    fn cpu_in_followup(&self) -> bool {
        self.cpu().in_followup
    }

    fn tick(&mut self) {
        AmigaA1200::tick(self);
    }

    fn tick_count(&self) -> u64 {
        AmigaA1200::tick_count(self)
    }

    fn scheduler_diagnostic_snapshot(&self) -> AmigaSchedulerDiagnosticSnapshot {
        AmigaA1200::scheduler_diagnostic_snapshot(self)
    }

    fn track_stream_diagnostic_snapshot(&self) -> AmigaTrackStreamDiagnosticSnapshot {
        AmigaA1200::track_stream_diagnostic_snapshot(self)
    }

    fn input_diagnostic_snapshot(&self) -> AmigaInputDiagnosticSnapshot {
        AmigaA1200::input_diagnostic_snapshot(self)
    }

    fn intena(&self) -> u16 {
        AmigaA1200::intena(self)
    }

    fn intreq(&self) -> u16 {
        AmigaA1200::intreq(self)
    }

    fn dmacon(&self) -> u16 {
        AmigaA1200::dmacon(self)
    }

    fn adkcon(&self) -> u16 {
        AmigaA1200::adkcon(self)
    }

    fn bplcon0(&self) -> u16 {
        AmigaA1200::bplcon0(self)
    }

    fn color(&self, idx: usize) -> u16 {
        AmigaA1200::color(self, idx)
    }

    fn overlay(&self) -> bool {
        self.memory().overlay()
    }

    fn copper_pc(&self) -> u32 {
        self.copper().pc
    }

    fn copper_cop1lc(&self) -> u32 {
        self.copper().cop1lc
    }

    fn copper_cop2lc(&self) -> u32 {
        self.copper().cop2lc
    }

    fn agnus(&self) -> &Agnus {
        // Inherent agnus() already returns &commodore_agnus_ocs::Agnus.
        AmigaA1200::agnus(self)
    }

    fn agnus_bus_diagnostic_snapshot(&self) -> AgnusBusDiagnosticSnapshot {
        let agnus = self.agnus_aga();
        agnus.bus_diagnostic_snapshot_for_plan(agnus.cck_bus_plan())
    }

    fn ecs_agnus_timing(&self) -> Option<EcsAgnusTimingSnapshot> {
        Some(ecs_agnus_timing_snapshot(self.agnus_aga().as_inner()))
    }

    fn enhanced_denise(&self) -> Option<EnhancedDeniseSnapshot> {
        let lisa = self.denise_aga();
        let ecs_denise = lisa.as_inner();
        let output_active = lisa.programmed_hblank_active()
            && (self.bplcon0() & 0x0001) != 0
            && ecs_denise.extblken_enabled();
        Some(enhanced_denise_snapshot(
            ecs_denise,
            lisa.deniseid(),
            self.bplcon0(),
            output_active,
        ))
    }

    fn denise_diagnostic_snapshot(&self) -> DeniseDiagnosticSnapshot {
        self.denise_aga()
            .as_inner()
            .as_inner()
            .diagnostic_snapshot()
    }

    fn aga_denise_diagnostic_snapshot(&self) -> Option<DeniseAgaDiagnosticSnapshot> {
        Some(self.denise_aga().diagnostic_snapshot())
    }

    fn cia_a(&self) -> &Cia {
        AmigaA1200::cia_a(self)
    }

    fn cia_b(&self) -> &Cia {
        AmigaA1200::cia_b(self)
    }

    fn rtc_diagnostic_snapshot(&self) -> Msm6242RtcDiagnosticSnapshot {
        AmigaA1200::rtc_diagnostic_snapshot(self)
    }

    fn paula(&self) -> &Paula8364 {
        AmigaA1200::paula(self)
    }

    fn drive(&self) -> &AmigaFloppyDrive {
        AmigaA1200::drive(self)
    }

    fn keyboard(&self) -> &AmigaKeyboard {
        AmigaA1200::keyboard(self)
    }

    fn copper(&self) -> &Copper {
        AmigaA1200::copper(self)
    }

    fn gary_diagnostic_snapshot(&self) -> GaryDiagnosticSnapshot {
        self.gary().diagnostic_snapshot()
    }

    fn gayle_diagnostic_snapshot(&self) -> Option<GayleDiagnosticSnapshot> {
        Some(AmigaA1200::gayle_diagnostic_snapshot(self))
    }

    fn framebuffer(&self) -> &[u32] {
        self.denise().framebuffer()
    }

    fn framebuffer_dims(&self) -> (u32, u32) {
        (
            machine_commodore_amiga_a1200::FB_WIDTH,
            machine_commodore_amiga_a1200::FB_HEIGHT,
        )
    }

    fn read_word(&self, addr: u32) -> u16 {
        AmigaA1200::read_word(self, addr)
    }

    fn read_long(&self, addr: u32) -> u32 {
        AmigaA1200::read_long(self, addr)
    }

    fn poke_byte(&mut self, addr: u32, value: u8) {
        AmigaA1200::poke_byte(self, addr, value);
    }

    fn poke_word(&mut self, addr: u32, value: u16) {
        AmigaA1200::poke_word(self, addr, value);
    }

    fn set_watch(&mut self, base_len: Option<(u32, u32)>) {
        self.debug_watch_addr = base_len;
        self.debug_watch_writes.clear();
    }

    fn watch_range(&self) -> Option<(u32, u32)> {
        self.debug_watch_addr
    }

    fn watch_log(&self) -> &[WatchLogEntry] {
        &self.debug_watch_writes
    }

    fn dsk_write_log(&self) -> &[DskLogEntry] {
        &self.debug_dsk_log
    }

    fn palette_log(&self) -> &[PaletteLogEntry] {
        &self.debug_palette_log
    }

    fn bplcon0_log(&self) -> &[Bplcon0LogEntry] {
        &self.debug_bplcon0_log
    }

    fn reg_read_log(&self) -> &[RegReadLogEntry] {
        &self.debug_reg_read_log
    }

    fn custom_write_log(&self) -> &[CustomWriteEntry] {
        &self.debug_custom_write_log
    }

    fn copper_move_log(&self) -> &[CopperMoveLogEntry] {
        &self.debug_copper_move_log
    }

    fn register_read_counts(&self) -> &std::collections::HashMap<u16, u64> {
        &self.debug_reg_read_counts
    }

    fn peak_intena(&self) -> u16 {
        self.debug_peak_intena
    }

    fn intena_write_count(&self) -> u64 {
        self.debug_intena_writes
    }

    fn intena_log(&self) -> &[RegisterTransitionLogEntry] {
        &self.debug_intena_log
    }

    fn cop1lc_log(&self) -> &[CopperPointerLogEntry] {
        &self.debug_cop1lc_log
    }

    fn cop2lc_log(&self) -> &[CopperPointerLogEntry] {
        &self.debug_cop2lc_log
    }

    fn dmacon_log(&self) -> &[RegisterTransitionLogEntry] {
        &self.debug_dmacon_log
    }

    fn blitter_start_count(&self) -> u64 {
        self.debug_blit_starts
    }

    fn blitter_log(&self) -> &[BlitLogEntry] {
        &self.debug_blit_log
    }

    fn cia_a_write_log(&self) -> &[CiaRegisterWriteLogEntry] {
        &self.debug_cia_a_cr_log
    }

    fn cia_b_write_log(&self) -> &[CiaRegisterWriteLogEntry] {
        &self.debug_cia_b_cr_log
    }

    fn cia_a_read_counts(&self) -> Option<&std::collections::HashMap<u8, u64>> {
        Some(&self.debug_cia_a_read_counts)
    }

    fn cia_b_read_counts(&self) -> Option<&std::collections::HashMap<u8, u64>> {
        Some(&self.debug_cia_b_read_counts)
    }

    fn rtc_access_log(&self) -> &[RtcAccessLogEntry] {
        &self.debug_rtc_log
    }

    fn aga_copper(&self) -> Option<&Copper> {
        Some(AmigaA1200::copper(self))
    }

    fn aga_lisa(&self) -> Option<AgaLisaSnapshot> {
        let aga = self.denise_aga();
        Some(AgaLisaSnapshot {
            deniseid: aga.deniseid(),
            // bplcon3 lives on the inner ECS Denise; reachable via Deref.
            bplcon3: aga.bplcon3,
            bplcon4: aga.bplcon4,
            spr_width: aga.spr_width,
            ham_prev_rgb24: aga.ham_prev_rgb24,
            programmed_hblank_active: aga.programmed_hblank_active(),
            palette_24: aga.palette_24,
        })
    }

    fn insert_floppy0_writable(&mut self, adf: Adf, change_pending: bool, writable: bool) {
        self.mount_adf(adf, change_pending, writable);
    }

    fn eject_floppy0(&mut self) {
        AmigaA1200::eject_disk(self);
    }
}

// ===================================================================
// AmigaRuntimeKind impl — dispatches to the inner machine.
// ===================================================================
//
// Each method calls the inner runtime's `machine()` / `machine_mut()`
// accessor to reach the concrete chip stack, then forwards to the
// trait method on that type. The MCP session can hold `&mut dyn
// AmigaLiveAccess` by reborrowing through `AmigaRuntimeKind`.

impl AmigaLiveAccess for AmigaRuntimeKind {
    fn cpu_snapshot(&self) -> CpuSnapshot {
        match self {
            Self::Ocs(rt) => rt.machine().cpu_snapshot(),
            Self::Ecs(rt) => rt.machine().cpu_snapshot(),
            Self::Aga(rt) => rt.machine().cpu_snapshot(),
        }
    }

    fn cpu_pc(&self) -> u32 {
        match self {
            Self::Ocs(rt) => rt.machine().cpu_pc(),
            Self::Ecs(rt) => rt.machine().cpu_pc(),
            Self::Aga(rt) => rt.machine().cpu_pc(),
        }
    }

    fn cpu_instruction_starts(&self) -> u64 {
        match self {
            Self::Ocs(rt) => rt.machine().cpu_instruction_starts(),
            Self::Ecs(rt) => rt.machine().cpu_instruction_starts(),
            Self::Aga(rt) => rt.machine().cpu_instruction_starts(),
        }
    }

    fn cpu_in_followup(&self) -> bool {
        match self {
            Self::Ocs(rt) => rt.machine().cpu_in_followup(),
            Self::Ecs(rt) => rt.machine().cpu_in_followup(),
            Self::Aga(rt) => rt.machine().cpu_in_followup(),
        }
    }

    fn tick(&mut self) {
        // Route through the runtime's trace funnel (not the bare
        // machine tick) so the per-tick `step` / `run_until_*` tools
        // feed an armed CPU trace, exactly as the run loop does.
        match self {
            Self::Ocs(rt) => rt.tick_traced(),
            Self::Ecs(rt) => rt.tick_traced(),
            Self::Aga(rt) => rt.tick_traced(),
        }
    }

    fn tick_count(&self) -> u64 {
        match self {
            Self::Ocs(rt) => rt.machine().tick_count(),
            Self::Ecs(rt) => rt.machine().tick_count(),
            Self::Aga(rt) => rt.machine().tick_count(),
        }
    }

    fn scheduler_diagnostic_snapshot(&self) -> AmigaSchedulerDiagnosticSnapshot {
        match self {
            Self::Ocs(rt) => rt.machine().scheduler_diagnostic_snapshot(),
            Self::Ecs(rt) => rt.machine().scheduler_diagnostic_snapshot(),
            Self::Aga(rt) => rt.machine().scheduler_diagnostic_snapshot(),
        }
    }

    fn track_stream_diagnostic_snapshot(&self) -> AmigaTrackStreamDiagnosticSnapshot {
        match self {
            Self::Ocs(rt) => rt.machine().track_stream_diagnostic_snapshot(),
            Self::Ecs(rt) => rt.machine().track_stream_diagnostic_snapshot(),
            Self::Aga(rt) => rt.machine().track_stream_diagnostic_snapshot(),
        }
    }

    fn input_diagnostic_snapshot(&self) -> AmigaInputDiagnosticSnapshot {
        match self {
            Self::Ocs(rt) => rt.machine().input_diagnostic_snapshot(),
            Self::Ecs(rt) => rt.machine().input_diagnostic_snapshot(),
            Self::Aga(rt) => rt.machine().input_diagnostic_snapshot(),
        }
    }

    fn intena(&self) -> u16 {
        match self {
            Self::Ocs(rt) => rt.machine().intena(),
            Self::Ecs(rt) => rt.machine().intena(),
            Self::Aga(rt) => rt.machine().intena(),
        }
    }

    fn intreq(&self) -> u16 {
        match self {
            Self::Ocs(rt) => rt.machine().intreq(),
            Self::Ecs(rt) => rt.machine().intreq(),
            Self::Aga(rt) => rt.machine().intreq(),
        }
    }

    fn dmacon(&self) -> u16 {
        match self {
            Self::Ocs(rt) => rt.machine().dmacon(),
            Self::Ecs(rt) => rt.machine().dmacon(),
            Self::Aga(rt) => rt.machine().dmacon(),
        }
    }

    fn adkcon(&self) -> u16 {
        match self {
            Self::Ocs(rt) => rt.machine().adkcon(),
            Self::Ecs(rt) => rt.machine().adkcon(),
            Self::Aga(rt) => rt.machine().adkcon(),
        }
    }

    fn bplcon0(&self) -> u16 {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::bplcon0(rt.machine()),
            Self::Ecs(rt) => AmigaLiveAccess::bplcon0(rt.machine()),
            Self::Aga(rt) => AmigaLiveAccess::bplcon0(rt.machine()),
        }
    }

    fn color(&self, idx: usize) -> u16 {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::color(rt.machine(), idx),
            Self::Ecs(rt) => AmigaLiveAccess::color(rt.machine(), idx),
            Self::Aga(rt) => AmigaLiveAccess::color(rt.machine(), idx),
        }
    }

    fn overlay(&self) -> bool {
        match self {
            Self::Ocs(rt) => rt.machine().overlay(),
            Self::Ecs(rt) => rt.machine().overlay(),
            Self::Aga(rt) => rt.machine().overlay(),
        }
    }

    fn copper_pc(&self) -> u32 {
        match self {
            Self::Ocs(rt) => rt.machine().copper_pc(),
            Self::Ecs(rt) => rt.machine().copper_pc(),
            Self::Aga(rt) => rt.machine().copper_pc(),
        }
    }

    fn copper_cop1lc(&self) -> u32 {
        match self {
            Self::Ocs(rt) => rt.machine().copper_cop1lc(),
            Self::Ecs(rt) => rt.machine().copper_cop1lc(),
            Self::Aga(rt) => rt.machine().copper_cop1lc(),
        }
    }

    fn copper_cop2lc(&self) -> u32 {
        match self {
            Self::Ocs(rt) => rt.machine().copper_cop2lc(),
            Self::Ecs(rt) => rt.machine().copper_cop2lc(),
            Self::Aga(rt) => rt.machine().copper_cop2lc(),
        }
    }

    fn agnus(&self) -> &Agnus {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::agnus(rt.machine()),
            Self::Ecs(rt) => AmigaLiveAccess::agnus(rt.machine()),
            Self::Aga(rt) => AmigaLiveAccess::agnus(rt.machine()),
        }
    }

    fn agnus_bus_diagnostic_snapshot(&self) -> AgnusBusDiagnosticSnapshot {
        match self {
            Self::Ocs(rt) => rt.machine().agnus_bus_diagnostic_snapshot(),
            Self::Ecs(rt) => rt.machine().agnus_bus_diagnostic_snapshot(),
            Self::Aga(rt) => rt.machine().agnus_bus_diagnostic_snapshot(),
        }
    }

    fn ecs_agnus_timing(&self) -> Option<EcsAgnusTimingSnapshot> {
        match self {
            Self::Ocs(_) => None,
            Self::Ecs(rt) => rt.machine().ecs_agnus_timing(),
            Self::Aga(rt) => rt.machine().ecs_agnus_timing(),
        }
    }

    fn enhanced_denise(&self) -> Option<EnhancedDeniseSnapshot> {
        match self {
            Self::Ocs(_) => None,
            Self::Ecs(rt) => rt.machine().enhanced_denise(),
            Self::Aga(rt) => rt.machine().enhanced_denise(),
        }
    }

    fn denise_diagnostic_snapshot(&self) -> DeniseDiagnosticSnapshot {
        match self {
            Self::Ocs(rt) => rt.machine().denise_diagnostic_snapshot(),
            Self::Ecs(rt) => rt.machine().denise_diagnostic_snapshot(),
            Self::Aga(rt) => rt.machine().denise_diagnostic_snapshot(),
        }
    }

    fn aga_denise_diagnostic_snapshot(&self) -> Option<DeniseAgaDiagnosticSnapshot> {
        match self {
            Self::Ocs(_) | Self::Ecs(_) => None,
            Self::Aga(rt) => Some(rt.machine().denise_aga().diagnostic_snapshot()),
        }
    }

    fn cia_a(&self) -> &Cia {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::cia_a(rt.machine()),
            Self::Ecs(rt) => AmigaLiveAccess::cia_a(rt.machine()),
            Self::Aga(rt) => AmigaLiveAccess::cia_a(rt.machine()),
        }
    }

    fn cia_b(&self) -> &Cia {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::cia_b(rt.machine()),
            Self::Ecs(rt) => AmigaLiveAccess::cia_b(rt.machine()),
            Self::Aga(rt) => AmigaLiveAccess::cia_b(rt.machine()),
        }
    }

    fn rtc_diagnostic_snapshot(&self) -> Msm6242RtcDiagnosticSnapshot {
        match self {
            Self::Ocs(rt) => rt.machine().rtc_diagnostic_snapshot(),
            Self::Ecs(rt) => rt.machine().rtc_diagnostic_snapshot(),
            Self::Aga(rt) => rt.machine().rtc_diagnostic_snapshot(),
        }
    }

    fn paula(&self) -> &Paula8364 {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::paula(rt.machine()),
            Self::Ecs(rt) => AmigaLiveAccess::paula(rt.machine()),
            Self::Aga(rt) => AmigaLiveAccess::paula(rt.machine()),
        }
    }

    fn drive(&self) -> &AmigaFloppyDrive {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::drive(rt.machine()),
            Self::Ecs(rt) => AmigaLiveAccess::drive(rt.machine()),
            Self::Aga(rt) => AmigaLiveAccess::drive(rt.machine()),
        }
    }

    fn keyboard(&self) -> &AmigaKeyboard {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::keyboard(rt.machine()),
            Self::Ecs(rt) => AmigaLiveAccess::keyboard(rt.machine()),
            Self::Aga(rt) => AmigaLiveAccess::keyboard(rt.machine()),
        }
    }

    fn copper(&self) -> &Copper {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::copper(rt.machine()),
            Self::Ecs(rt) => AmigaLiveAccess::copper(rt.machine()),
            Self::Aga(rt) => AmigaLiveAccess::copper(rt.machine()),
        }
    }

    fn gary_diagnostic_snapshot(&self) -> GaryDiagnosticSnapshot {
        match self {
            Self::Ocs(rt) => rt.machine().gary().diagnostic_snapshot(),
            Self::Ecs(rt) => rt.machine().gary().diagnostic_snapshot(),
            Self::Aga(rt) => rt.machine().gary().diagnostic_snapshot(),
        }
    }

    fn gayle_diagnostic_snapshot(&self) -> Option<GayleDiagnosticSnapshot> {
        match self {
            Self::Ocs(_) => None,
            Self::Ecs(rt) => rt.machine().gayle_diagnostic_snapshot(),
            Self::Aga(rt) => Some(rt.machine().gayle_diagnostic_snapshot()),
        }
    }

    fn framebuffer(&self) -> &[u32] {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::framebuffer(rt.machine()),
            Self::Ecs(rt) => AmigaLiveAccess::framebuffer(rt.machine()),
            Self::Aga(rt) => AmigaLiveAccess::framebuffer(rt.machine()),
        }
    }

    fn framebuffer_dims(&self) -> (u32, u32) {
        match self {
            Self::Ocs(rt) => rt.machine().framebuffer_dims(),
            Self::Ecs(rt) => rt.machine().framebuffer_dims(),
            Self::Aga(rt) => rt.machine().framebuffer_dims(),
        }
    }

    fn read_word(&self, addr: u32) -> u16 {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::read_word(rt.machine(), addr),
            Self::Ecs(rt) => AmigaLiveAccess::read_word(rt.machine(), addr),
            Self::Aga(rt) => AmigaLiveAccess::read_word(rt.machine(), addr),
        }
    }

    fn read_long(&self, addr: u32) -> u32 {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::read_long(rt.machine(), addr),
            Self::Ecs(rt) => AmigaLiveAccess::read_long(rt.machine(), addr),
            Self::Aga(rt) => AmigaLiveAccess::read_long(rt.machine(), addr),
        }
    }

    fn poke_byte(&mut self, addr: u32, value: u8) {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::poke_byte(rt.machine_mut(), addr, value),
            Self::Ecs(rt) => AmigaLiveAccess::poke_byte(rt.machine_mut(), addr, value),
            Self::Aga(rt) => AmigaLiveAccess::poke_byte(rt.machine_mut(), addr, value),
        }
    }

    fn poke_word(&mut self, addr: u32, value: u16) {
        match self {
            Self::Ocs(rt) => AmigaLiveAccess::poke_word(rt.machine_mut(), addr, value),
            Self::Ecs(rt) => AmigaLiveAccess::poke_word(rt.machine_mut(), addr, value),
            Self::Aga(rt) => AmigaLiveAccess::poke_word(rt.machine_mut(), addr, value),
        }
    }

    fn set_watch(&mut self, base_len: Option<(u32, u32)>) {
        match self {
            Self::Ocs(rt) => rt.machine_mut().set_watch(base_len),
            Self::Ecs(rt) => rt.machine_mut().set_watch(base_len),
            Self::Aga(rt) => rt.machine_mut().set_watch(base_len),
        }
    }

    fn watch_range(&self) -> Option<(u32, u32)> {
        match self {
            Self::Ocs(rt) => rt.machine().watch_range(),
            Self::Ecs(rt) => rt.machine().watch_range(),
            Self::Aga(rt) => rt.machine().watch_range(),
        }
    }

    fn watch_log(&self) -> &[WatchLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().watch_log(),
            Self::Ecs(rt) => rt.machine().watch_log(),
            Self::Aga(rt) => rt.machine().watch_log(),
        }
    }

    fn dsk_write_log(&self) -> &[DskLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().dsk_write_log(),
            Self::Ecs(rt) => rt.machine().dsk_write_log(),
            Self::Aga(rt) => rt.machine().dsk_write_log(),
        }
    }

    fn palette_log(&self) -> &[PaletteLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().palette_log(),
            Self::Ecs(rt) => rt.machine().palette_log(),
            Self::Aga(rt) => rt.machine().palette_log(),
        }
    }

    fn bplcon0_log(&self) -> &[Bplcon0LogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().bplcon0_log(),
            Self::Ecs(rt) => rt.machine().bplcon0_log(),
            Self::Aga(rt) => rt.machine().bplcon0_log(),
        }
    }

    fn reg_read_log(&self) -> &[RegReadLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().reg_read_log(),
            Self::Ecs(rt) => rt.machine().reg_read_log(),
            Self::Aga(rt) => rt.machine().reg_read_log(),
        }
    }

    fn custom_write_log(&self) -> &[CustomWriteEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().custom_write_log(),
            Self::Ecs(rt) => rt.machine().custom_write_log(),
            Self::Aga(rt) => rt.machine().custom_write_log(),
        }
    }

    fn copper_move_log(&self) -> &[CopperMoveLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().copper_move_log(),
            Self::Ecs(rt) => rt.machine().copper_move_log(),
            Self::Aga(rt) => rt.machine().copper_move_log(),
        }
    }

    fn register_read_counts(&self) -> &std::collections::HashMap<u16, u64> {
        match self {
            Self::Ocs(rt) => rt.machine().register_read_counts(),
            Self::Ecs(rt) => rt.machine().register_read_counts(),
            Self::Aga(rt) => rt.machine().register_read_counts(),
        }
    }

    fn peak_intena(&self) -> u16 {
        match self {
            Self::Ocs(rt) => rt.machine().peak_intena(),
            Self::Ecs(rt) => rt.machine().peak_intena(),
            Self::Aga(rt) => rt.machine().peak_intena(),
        }
    }

    fn intena_write_count(&self) -> u64 {
        match self {
            Self::Ocs(rt) => rt.machine().intena_write_count(),
            Self::Ecs(rt) => rt.machine().intena_write_count(),
            Self::Aga(rt) => rt.machine().intena_write_count(),
        }
    }

    fn intena_log(&self) -> &[RegisterTransitionLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().intena_log(),
            Self::Ecs(rt) => rt.machine().intena_log(),
            Self::Aga(rt) => rt.machine().intena_log(),
        }
    }

    fn cop1lc_log(&self) -> &[CopperPointerLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().cop1lc_log(),
            Self::Ecs(rt) => rt.machine().cop1lc_log(),
            Self::Aga(rt) => rt.machine().cop1lc_log(),
        }
    }

    fn cop2lc_log(&self) -> &[CopperPointerLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().cop2lc_log(),
            Self::Ecs(rt) => rt.machine().cop2lc_log(),
            Self::Aga(rt) => rt.machine().cop2lc_log(),
        }
    }

    fn dmacon_log(&self) -> &[RegisterTransitionLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().dmacon_log(),
            Self::Ecs(rt) => rt.machine().dmacon_log(),
            Self::Aga(rt) => rt.machine().dmacon_log(),
        }
    }

    fn blitter_start_count(&self) -> u64 {
        match self {
            Self::Ocs(rt) => rt.machine().blitter_start_count(),
            Self::Ecs(rt) => rt.machine().blitter_start_count(),
            Self::Aga(rt) => rt.machine().blitter_start_count(),
        }
    }

    fn blitter_log(&self) -> &[BlitLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().blitter_log(),
            Self::Ecs(rt) => rt.machine().blitter_log(),
            Self::Aga(rt) => rt.machine().blitter_log(),
        }
    }

    fn cia_a_write_log(&self) -> &[CiaRegisterWriteLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().cia_a_write_log(),
            Self::Ecs(rt) => rt.machine().cia_a_write_log(),
            Self::Aga(rt) => rt.machine().cia_a_write_log(),
        }
    }

    fn cia_b_write_log(&self) -> &[CiaRegisterWriteLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().cia_b_write_log(),
            Self::Ecs(rt) => rt.machine().cia_b_write_log(),
            Self::Aga(rt) => rt.machine().cia_b_write_log(),
        }
    }

    fn cia_a_read_counts(&self) -> Option<&std::collections::HashMap<u8, u64>> {
        match self {
            Self::Ocs(_) | Self::Ecs(_) => None,
            Self::Aga(rt) => rt.machine().cia_a_read_counts(),
        }
    }

    fn cia_b_read_counts(&self) -> Option<&std::collections::HashMap<u8, u64>> {
        match self {
            Self::Ocs(_) | Self::Ecs(_) => None,
            Self::Aga(rt) => rt.machine().cia_b_read_counts(),
        }
    }

    fn rtc_access_log(&self) -> &[RtcAccessLogEntry] {
        match self {
            Self::Ocs(rt) => rt.machine().rtc_access_log(),
            Self::Ecs(rt) => rt.machine().rtc_access_log(),
            Self::Aga(rt) => rt.machine().rtc_access_log(),
        }
    }

    fn aga_copper(&self) -> Option<&Copper> {
        match self {
            Self::Ocs(_) | Self::Ecs(_) => None,
            Self::Aga(rt) => rt.machine().aga_copper(),
        }
    }

    fn aga_lisa(&self) -> Option<AgaLisaSnapshot> {
        match self {
            Self::Ocs(_) | Self::Ecs(_) => None,
            Self::Aga(rt) => rt.machine().aga_lisa(),
        }
    }

    fn insert_floppy0_writable(&mut self, adf: Adf, change_pending: bool, writable: bool) {
        match self {
            Self::Ocs(rt) => {
                rt.machine_mut()
                    .insert_floppy0_writable(adf, change_pending, writable)
            }
            Self::Ecs(rt) => {
                rt.machine_mut()
                    .insert_floppy0_writable(adf, change_pending, writable)
            }
            Self::Aga(rt) => {
                rt.machine_mut()
                    .insert_floppy0_writable(adf, change_pending, writable)
            }
        }
    }

    fn eject_floppy0(&mut self) {
        match self {
            Self::Ocs(rt) => rt.machine_mut().eject_floppy0(),
            Self::Ecs(rt) => rt.machine_mut().eject_floppy0(),
            Self::Aga(rt) => rt.machine_mut().eject_floppy0(),
        }
    }

    // ---------- instruction-boundary CPU trace ----------
    // Delegate to the inner runtime's trace (the only impl that carries
    // real state; the bare-machine impls keep the trait defaults).

    fn cpu_trace_arm(&mut self, pc_filter: Option<(u32, u32)>, max_entries: usize) {
        match self {
            Self::Ocs(rt) => rt.cpu_trace_arm(pc_filter, max_entries),
            Self::Ecs(rt) => rt.cpu_trace_arm(pc_filter, max_entries),
            Self::Aga(rt) => rt.cpu_trace_arm(pc_filter, max_entries),
        }
    }

    fn cpu_trace_disarm(&mut self) -> usize {
        match self {
            Self::Ocs(rt) => rt.cpu_trace_disarm(),
            Self::Ecs(rt) => rt.cpu_trace_disarm(),
            Self::Aga(rt) => rt.cpu_trace_disarm(),
        }
    }

    fn cpu_trace_clear(&mut self) -> usize {
        match self {
            Self::Ocs(rt) => rt.cpu_trace_clear(),
            Self::Ecs(rt) => rt.cpu_trace_clear(),
            Self::Aga(rt) => rt.cpu_trace_clear(),
        }
    }

    fn cpu_trace_armed(&self) -> bool {
        match self {
            Self::Ocs(rt) => rt.cpu_trace_armed(),
            Self::Ecs(rt) => rt.cpu_trace_armed(),
            Self::Aga(rt) => rt.cpu_trace_armed(),
        }
    }

    fn cpu_trace_max_entries(&self) -> usize {
        match self {
            Self::Ocs(rt) => rt.cpu_trace_max_entries(),
            Self::Ecs(rt) => rt.cpu_trace_max_entries(),
            Self::Aga(rt) => rt.cpu_trace_max_entries(),
        }
    }

    fn cpu_trace_entries(&self) -> &[crate::CpuTraceEntry] {
        match self {
            Self::Ocs(rt) => rt.cpu_trace_entries(),
            Self::Ecs(rt) => rt.cpu_trace_entries(),
            Self::Aga(rt) => rt.cpu_trace_entries(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Model;

    /// Smoke test: the OCS variant impls the trait and the kind
    /// dispatcher reaches it without panicking.
    #[test]
    fn ocs_runtime_kind_via_trait() {
        let kind = AmigaRuntimeKind::blank(Model::A500OcsPal);
        // Trait method dispatch must not panic.
        let _intena = AmigaLiveAccess::intena(&kind);
        let _pc = AmigaLiveAccess::cpu_pc(&kind);
        // Chipset-trace logs are present on every variant; initially
        // empty before any boot ticks.
        assert!(kind.palette_log().is_empty());
        assert!(kind.bplcon0_log().is_empty());
        assert!(kind.reg_read_log().is_empty());
        // AGA Copper struct only on AGA.
        assert!(kind.aga_copper().is_none());
    }

    #[test]
    fn ecs_runtime_kind_via_trait() {
        let kind = AmigaRuntimeKind::blank(Model::A500PlusEcsPal);
        let _intena = AmigaLiveAccess::intena(&kind);
        let _pc = AmigaLiveAccess::cpu_pc(&kind);
        assert!(kind.palette_log().is_empty());
        assert!(kind.aga_copper().is_none());
    }

    #[test]
    fn aga_runtime_kind_via_trait() {
        let kind = AmigaRuntimeKind::blank(Model::A1200AgaPal);
        let _intena = AmigaLiveAccess::intena(&kind);
        let _pc = AmigaLiveAccess::cpu_pc(&kind);
        assert!(kind.palette_log().is_empty());
        assert!(kind.bplcon0_log().is_empty());
        assert!(kind.reg_read_log().is_empty());
        // AGA Copper is reachable.
        assert!(kind.aga_copper().is_some());
    }
}
