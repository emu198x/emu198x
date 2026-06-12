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

use commodore_agnus_ocs::Agnus;
use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_a1200::{AmigaA1200, Copper as A1200Copper};
use machine_commodore_amiga_ecs::AmigaEcs;
use machine_commodore_amiga_ocs::{AmigaFloppyDrive, AmigaKeyboard, AmigaOcs, Cia, Paula8364};
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
    /// 256-entry 24-bit palette (8 banks × 32), stored `0x00RRGGBB`.
    pub palette_24: [u32; 256],
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
    fn cia_a(&self) -> &Cia;
    fn cia_b(&self) -> &Cia;
    fn paula(&self) -> &Paula8364;
    fn drive(&self) -> &AmigaFloppyDrive;
    fn keyboard(&self) -> &AmigaKeyboard;

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

    /// AGA Copper struct reference, for the `query_copper_list` tool.
    /// Returns `None` on OCS / ECS — those chipsets carry a different
    /// Copper type that hasn't been lifted to a shared base yet.
    fn aga_copper(&self) -> Option<&A1200Copper>;

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

    fn cia_a(&self) -> &Cia {
        AmigaOcs::cia_a(self)
    }

    fn cia_b(&self) -> &Cia {
        AmigaOcs::cia_b(self)
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

    fn aga_copper(&self) -> Option<&A1200Copper> {
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

    fn cia_a(&self) -> &Cia {
        AmigaEcs::cia_a(self)
    }

    fn cia_b(&self) -> &Cia {
        AmigaEcs::cia_b(self)
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

    fn aga_copper(&self) -> Option<&A1200Copper> {
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

    fn cia_a(&self) -> &Cia {
        AmigaA1200::cia_a(self)
    }

    fn cia_b(&self) -> &Cia {
        AmigaA1200::cia_b(self)
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

    fn aga_copper(&self) -> Option<&A1200Copper> {
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

    fn aga_copper(&self) -> Option<&A1200Copper> {
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
