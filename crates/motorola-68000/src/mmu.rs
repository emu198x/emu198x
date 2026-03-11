//! MMU support for 68030 and 68040 address translation.
//!
//! 68030: variable-depth table walk, configurable page size (256B–32KB),
//! 22-entry fully associative ATC, two TT registers (TT0/TT1).
//!
//! 68040: fixed 3-level table walk, 4KB or 8KB pages, dual 64-entry
//! 4-way set-associative ATCs (instruction + data), four TT registers
//! (ITT0/ITT1 for instruction, DTT0/DTT1 for data).

use crate::model::{CpuModel, TimingClass};

// ---------------------------------------------------------------------------
// MMU mode
// ---------------------------------------------------------------------------

/// MMU operating mode, determined by CPU model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmuMode {
    /// No MMU — EC variants or 68000/68010.
    Disabled,
    /// 68030-style variable-depth table walk.
    M68030,
    /// 68040-style fixed 3-level table walk.
    M68040,
}

impl MmuMode {
    /// Determine MMU mode from CPU model.
    #[must_use]
    pub fn from_model(model: CpuModel) -> Self {
        if !model.capabilities().mmu {
            return Self::Disabled;
        }
        match model.timing_class() {
            TimingClass::M68000 => Self::Disabled,
            TimingClass::M68020 => Self::M68030,
            TimingClass::M68040 | TimingClass::M68060 => Self::M68040,
        }
    }
}

// ---------------------------------------------------------------------------
// TC (Translation Control) register parsing
// ---------------------------------------------------------------------------

/// Parsed 68030 TC register.
///
/// Layout (32 bits):
/// - Bit 31: Enable
/// - Bit 25: SRE (Supervisor Root Enable)
/// - Bit 24: FCL (Function Code Lookup)
/// - Bits 23–20: PS (page size = 2^PS bytes)
/// - Bits 19–16: IS (initial shift)
/// - Bits 15–12: TIA (table index A width)
/// - Bits 11–8: TIB
/// - Bits 7–4: TIC
/// - Bits 3–0: TID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tc030 {
    pub enabled: bool,
    pub sre: bool,
    pub fcl: bool,
    pub page_shift: u8,
    pub initial_shift: u8,
    pub tia: u8,
    pub tib: u8,
    pub tic: u8,
    pub tid: u8,
}

impl Tc030 {
    /// Parse a raw 68030 TC register value.
    #[must_use]
    pub const fn parse(tc: u32) -> Self {
        Self {
            enabled: tc & (1 << 31) != 0,
            sre: tc & (1 << 25) != 0,
            fcl: tc & (1 << 24) != 0,
            page_shift: ((tc >> 20) & 0xF) as u8,
            initial_shift: ((tc >> 16) & 0xF) as u8,
            tia: ((tc >> 12) & 0xF) as u8,
            tib: ((tc >> 8) & 0xF) as u8,
            tic: ((tc >> 4) & 0xF) as u8,
            tid: (tc & 0xF) as u8,
        }
    }

    /// Index field widths as an array \[TIA, TIB, TIC, TID\].
    #[must_use]
    pub const fn index_fields(&self) -> [u8; 4] {
        [self.tia, self.tib, self.tic, self.tid]
    }

    /// Number of non-zero table levels.
    #[must_use]
    pub fn num_levels(&self) -> u8 {
        self.index_fields().iter().filter(|&&f| f > 0).count() as u8
    }

    /// Page size in bytes.
    #[must_use]
    pub const fn page_size(&self) -> u32 {
        1u32 << self.page_shift
    }

    /// Bit-mask covering the page offset (bits below page boundary).
    #[must_use]
    pub const fn page_mask(&self) -> u32 {
        self.page_size().wrapping_sub(1)
    }
}

/// Parsed 68040 TC register.
///
/// Layout (16-bit effective):
/// - Bit 15: Enable
/// - Bit 14: Page size (0 = 4KB, 1 = 8KB)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tc040 {
    pub enabled: bool,
    pub page_8k: bool,
}

impl Tc040 {
    /// Parse a raw 68040 TC register value.
    #[must_use]
    pub const fn parse(tc: u32) -> Self {
        Self {
            enabled: tc & (1 << 15) != 0,
            page_8k: tc & (1 << 14) != 0,
        }
    }

    /// Page size shift (12 for 4KB, 13 for 8KB).
    #[must_use]
    pub const fn page_shift(&self) -> u8 {
        if self.page_8k { 13 } else { 12 }
    }

    /// Page size in bytes.
    #[must_use]
    pub const fn page_size(&self) -> u32 {
        1u32 << self.page_shift()
    }

    /// Bit-mask covering the page offset.
    #[must_use]
    pub const fn page_mask(&self) -> u32 {
        self.page_size().wrapping_sub(1)
    }
}

// ---------------------------------------------------------------------------
// TT (Transparent Translation) register matching
// ---------------------------------------------------------------------------

/// Result of a successful transparent translation match.
#[derive(Debug, Clone, Copy)]
pub struct TtMatchResult {
    /// Page is cache-inhibited.
    pub cache_inhibit: bool,
    /// Page is write-protected (68040 only; always false for 68030).
    pub write_protect: bool,
}

/// Check if a 68030 TT register matches the given access.
///
/// 68030 TT register layout:
/// - Bits 31–24: Logical Address Base
/// - Bits 23–16: Logical Address Mask (1 = don't compare)
/// - Bit 15: Enable
/// - Bit 14: Cache Inhibit output
/// - Bit 13: R/W (0 = read, 1 = write)
/// - Bit 12: R/W Mask (1 = match both reads and writes)
/// - Bits 6–4: Function Code Base
/// - Bits 2–0: Function Code Mask (1 = don't compare)
#[must_use]
pub fn tt_match_030(tt: u32, addr: u32, fc: u8, is_write: bool) -> Option<TtMatchResult> {
    // Enable check
    if tt & (1 << 15) == 0 {
        return None;
    }

    // FC match: (fc ^ fc_base) & ~fc_mask == 0
    let fc_base = ((tt >> 4) & 7) as u8;
    let fc_mask = (tt & 7) as u8;
    if (fc ^ fc_base) & !fc_mask & 7 != 0 {
        return None;
    }

    // R/W match
    let rwm = tt & (1 << 12) != 0;
    if !rwm {
        let rw_bit = tt & (1 << 13) != 0;
        if is_write != rw_bit {
            return None;
        }
    }

    // Address match: compare A31–A24
    let addr_base = (tt >> 24) as u8;
    let addr_mask = ((tt >> 16) & 0xFF) as u8;
    let addr_upper = (addr >> 24) as u8;
    if (addr_upper ^ addr_base) & !addr_mask != 0 {
        return None;
    }

    Some(TtMatchResult {
        cache_inhibit: tt & (1 << 14) != 0,
        write_protect: false,
    })
}

/// Check if a 68040 TT register matches the given access.
///
/// 68040 TT register layout:
/// - Bits 31–24: Logical Address Base
/// - Bits 23–16: Logical Address Mask (1 = don't compare)
/// - Bit 15: Enable
/// - Bits 14–13: S-field (00 = user, 01 = supervisor, 1x = both)
/// - Bits 7–6: CM (cache mode; ≥ 2 means noncacheable)
/// - Bit 2: W (write protect)
///
/// The caller selects ITT vs DTT based on function code.
#[must_use]
pub fn tt_match_040(tt: u32, addr: u32, is_supervisor: bool) -> Option<TtMatchResult> {
    if tt & (1 << 15) == 0 {
        return None;
    }

    // S-field match
    let s_field = (tt >> 13) & 3;
    match s_field {
        0b00 if is_supervisor => return None,
        0b01 if !is_supervisor => return None,
        _ => {}
    }

    // Address match: compare A31–A24
    let addr_base = (tt >> 24) as u8;
    let addr_mask = ((tt >> 16) & 0xFF) as u8;
    let addr_upper = (addr >> 24) as u8;
    if (addr_upper ^ addr_base) & !addr_mask != 0 {
        return None;
    }

    let cm = (tt >> 6) & 3;
    Some(TtMatchResult {
        cache_inhibit: cm >= 2,
        write_protect: tt & (1 << 2) != 0,
    })
}

/// Check 68030 transparent translation (TT0 then TT1).
#[must_use]
pub fn check_tt_030(
    tt0: u32,
    tt1: u32,
    addr: u32,
    fc: u8,
    is_write: bool,
) -> Option<TtMatchResult> {
    tt_match_030(tt0, addr, fc, is_write).or_else(|| tt_match_030(tt1, addr, fc, is_write))
}

/// Check 68040 transparent translation.
///
/// Instruction fetches (FC = 2 or 6) check ITT0/ITT1.
/// Data accesses (FC = 1 or 5) check DTT0/DTT1.
#[must_use]
pub fn check_tt_040(
    itt0: u32,
    itt1: u32,
    dtt0: u32,
    dtt1: u32,
    addr: u32,
    fc: u8,
    is_supervisor: bool,
) -> Option<TtMatchResult> {
    match fc {
        2 | 6 => {
            tt_match_040(itt0, addr, is_supervisor)
                .or_else(|| tt_match_040(itt1, addr, is_supervisor))
        }
        _ => {
            tt_match_040(dtt0, addr, is_supervisor)
                .or_else(|| tt_match_040(dtt1, addr, is_supervisor))
        }
    }
}

// ---------------------------------------------------------------------------
// ATC (Address Translation Cache)
// ---------------------------------------------------------------------------

const ATC_030_SIZE: usize = 22;
const ATC_040_SETS: usize = 16;
const ATC_040_WAYS: usize = 4;

/// A single ATC entry.
#[derive(Debug, Clone, Copy)]
pub struct AtcEntry {
    /// Logical page address (page-aligned, offset bits zeroed).
    pub logical_page: u32,
    /// Physical page address (page-aligned).
    pub physical_page: u32,
    /// Function code of the access that created this entry.
    pub fc: u8,
    /// Entry is valid.
    pub valid: bool,
    /// Page is write-protected.
    pub write_protect: bool,
    /// Page is cache-inhibited.
    pub cache_inhibit: bool,
    /// Page has been modified (written to).
    pub modified: bool,
    /// Global page (68040 only — preserved by PFLUSHN).
    pub global: bool,
}

impl AtcEntry {
    const EMPTY: Self = Self {
        logical_page: 0,
        physical_page: 0,
        fc: 0,
        valid: false,
        write_protect: false,
        cache_inhibit: false,
        modified: false,
        global: false,
    };
}

/// 68030 ATC: 22-entry fully associative with FIFO replacement.
#[derive(Clone)]
pub struct Atc030 {
    entries: [AtcEntry; ATC_030_SIZE],
    next_slot: usize,
}

impl Default for Atc030 {
    fn default() -> Self {
        Self::new()
    }
}

impl Atc030 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: [AtcEntry::EMPTY; ATC_030_SIZE],
            next_slot: 0,
        }
    }

    /// Look up a logical page + FC.
    #[must_use]
    pub fn lookup(&self, logical_page: u32, fc: u8) -> Option<&AtcEntry> {
        self.entries
            .iter()
            .find(|e| e.valid && e.logical_page == logical_page && e.fc == fc)
    }

    /// Insert an entry. Replaces an existing match, fills an empty slot,
    /// or evicts the oldest via FIFO.
    pub fn insert(&mut self, entry: AtcEntry) {
        // Replace existing entry for same page + FC
        for e in &mut self.entries {
            if e.valid && e.logical_page == entry.logical_page && e.fc == entry.fc {
                *e = entry;
                return;
            }
        }
        // First empty slot
        for e in &mut self.entries {
            if !e.valid {
                *e = entry;
                return;
            }
        }
        // FIFO eviction
        self.entries[self.next_slot] = entry;
        self.next_slot = (self.next_slot + 1) % ATC_030_SIZE;
    }

    /// Invalidate all entries.
    pub fn flush_all(&mut self) {
        for e in &mut self.entries {
            e.valid = false;
        }
        self.next_slot = 0;
    }

    /// Invalidate entries matching a specific function code.
    pub fn flush_by_fc(&mut self, fc: u8) {
        for e in &mut self.entries {
            if e.valid && e.fc == fc {
                e.valid = false;
            }
        }
    }

    /// Invalidate the entry for a specific page + FC.
    pub fn flush_by_addr(&mut self, logical_page: u32, fc: u8) {
        for e in &mut self.entries {
            if e.valid && e.logical_page == logical_page && e.fc == fc {
                e.valid = false;
            }
        }
    }
}

/// One bank of the 68040 ATC (16 sets × 4 ways = 64 entries).
#[derive(Clone)]
pub struct Atc040Bank {
    entries: [[AtcEntry; ATC_040_WAYS]; ATC_040_SETS],
    next_way: [u8; ATC_040_SETS],
}

impl Default for Atc040Bank {
    fn default() -> Self {
        Self::new()
    }
}

impl Atc040Bank {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: [[AtcEntry::EMPTY; ATC_040_WAYS]; ATC_040_SETS],
            next_way: [0; ATC_040_SETS],
        }
    }

    /// Compute set index from a page-aligned logical address.
    fn set_index(logical_page: u32, page_shift: u8) -> usize {
        ((logical_page >> page_shift) as usize) & (ATC_040_SETS - 1)
    }

    /// Look up a logical page in this bank.
    #[must_use]
    pub fn lookup(&self, logical_page: u32, fc: u8, page_shift: u8) -> Option<&AtcEntry> {
        let set = Self::set_index(logical_page, page_shift);
        self.entries[set]
            .iter()
            .find(|e| e.valid && e.logical_page == logical_page && e.fc == fc)
    }

    /// Insert an entry, evicting by FIFO within the set if full.
    pub fn insert(&mut self, entry: AtcEntry, page_shift: u8) {
        let set = Self::set_index(entry.logical_page, page_shift);

        // Replace existing match
        for e in &mut self.entries[set] {
            if e.valid && e.logical_page == entry.logical_page && e.fc == entry.fc {
                *e = entry;
                return;
            }
        }
        // First empty way
        for e in &mut self.entries[set] {
            if !e.valid {
                *e = entry;
                return;
            }
        }
        // FIFO eviction within the set
        let way = self.next_way[set] as usize;
        self.entries[set][way] = entry;
        self.next_way[set] = ((way + 1) % ATC_040_WAYS) as u8;
    }

    /// Invalidate all entries.
    pub fn flush_all(&mut self) {
        for set in &mut self.entries {
            for e in set {
                e.valid = false;
            }
        }
        for w in &mut self.next_way {
            *w = 0;
        }
    }

    /// Invalidate entries for a page. When `include_global` is false,
    /// global entries are preserved (PFLUSHN behaviour).
    pub fn flush_page(&mut self, logical_page: u32, page_shift: u8, include_global: bool) {
        let set = Self::set_index(logical_page, page_shift);
        for e in &mut self.entries[set] {
            if e.valid && e.logical_page == logical_page && (include_global || !e.global) {
                e.valid = false;
            }
        }
    }
}

/// 68040 ATC: dual banks (instruction + data).
#[derive(Clone)]
pub struct Atc040 {
    pub instruction: Atc040Bank,
    pub data: Atc040Bank,
}

impl Default for Atc040 {
    fn default() -> Self {
        Self::new()
    }
}

impl Atc040 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            instruction: Atc040Bank::new(),
            data: Atc040Bank::new(),
        }
    }

    /// Select the bank for a function code (FC 2/6 → instruction, else data).
    #[must_use]
    pub fn bank_for_fc(&self, fc: u8) -> &Atc040Bank {
        if fc == 2 || fc == 6 {
            &self.instruction
        } else {
            &self.data
        }
    }

    /// Select the bank (mutable) for a function code.
    pub fn bank_for_fc_mut(&mut self, fc: u8) -> &mut Atc040Bank {
        if fc == 2 || fc == 6 {
            &mut self.instruction
        } else {
            &mut self.data
        }
    }

    /// Invalidate all entries in both banks.
    pub fn flush_all(&mut self) {
        self.instruction.flush_all();
        self.data.flush_all();
    }
}

// ---------------------------------------------------------------------------
// Top-level MMU
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum AtcStorage {
    None,
    M030(Box<Atc030>),
    M040(Box<Atc040>),
}

/// Top-level MMU state.
#[derive(Clone)]
pub struct Mmu {
    mode: MmuMode,
    atc: AtcStorage,
}

impl Mmu {
    /// Create an MMU configured for the given CPU model.
    #[must_use]
    pub fn new(model: CpuModel) -> Self {
        let mode = MmuMode::from_model(model);
        let atc = match mode {
            MmuMode::Disabled => AtcStorage::None,
            MmuMode::M68030 => AtcStorage::M030(Box::default()),
            MmuMode::M68040 => AtcStorage::M040(Box::default()),
        };
        Self { mode, atc }
    }

    /// Current MMU mode.
    #[must_use]
    pub fn mode(&self) -> MmuMode {
        self.mode
    }

    // --- 68030 ATC ---

    /// ATC lookup (68030).
    #[must_use]
    pub fn atc_lookup_030(&self, logical_page: u32, fc: u8) -> Option<&AtcEntry> {
        match &self.atc {
            AtcStorage::M030(atc) => atc.lookup(logical_page, fc),
            _ => None,
        }
    }

    /// ATC insert (68030).
    pub fn atc_insert_030(&mut self, entry: AtcEntry) {
        if let AtcStorage::M030(atc) = &mut self.atc {
            atc.insert(entry);
        }
    }

    // --- 68040 ATC ---

    /// ATC lookup (68040). Routes to instruction or data bank by FC.
    #[must_use]
    pub fn atc_lookup_040(&self, logical_page: u32, fc: u8, page_shift: u8) -> Option<&AtcEntry> {
        match &self.atc {
            AtcStorage::M040(atc) => atc.bank_for_fc(fc).lookup(logical_page, fc, page_shift),
            _ => None,
        }
    }

    /// ATC insert (68040). Routes to instruction or data bank by FC.
    pub fn atc_insert_040(&mut self, entry: AtcEntry, fc: u8, page_shift: u8) {
        if let AtcStorage::M040(atc) = &mut self.atc {
            atc.bank_for_fc_mut(fc).insert(entry, page_shift);
        }
    }

    // --- Flush operations ---

    /// Flush all ATC entries.
    pub fn flush_all(&mut self) {
        match &mut self.atc {
            AtcStorage::None => {}
            AtcStorage::M030(atc) => atc.flush_all(),
            AtcStorage::M040(atc) => atc.flush_all(),
        }
    }

    /// Flush entries by function code (68030).
    pub fn flush_by_fc(&mut self, fc: u8) {
        if let AtcStorage::M030(atc) = &mut self.atc {
            atc.flush_by_fc(fc);
        }
    }

    /// Flush entry matching a specific address + FC (68030).
    pub fn flush_by_addr_030(&mut self, logical_page: u32, fc: u8) {
        if let AtcStorage::M030(atc) = &mut self.atc {
            atc.flush_by_addr(logical_page, fc);
        }
    }

    /// Flush entries matching an address (any ATC backend).
    /// For 68030: flushes by logical page + fc_base.
    /// For 68040: flushes the page (including global entries).
    pub fn flush_by_addr(&mut self, addr: u32, fc_base: u8, _fc_mask: u8) {
        match &mut self.atc {
            AtcStorage::None => {}
            AtcStorage::M030(atc) => atc.flush_by_addr(addr, fc_base),
            AtcStorage::M040(atc) => {
                // 68040 PFLUSH flushes by address in both banks.
                // Use 4KB page shift as default (page size from TC isn't
                // stored on Mmu yet — Phase 5 will parse TC on the fly).
                atc.bank_for_fc_mut(fc_base).flush_page(addr, 12, true);
            }
        }
    }

    /// Flush page entry (68040). When `include_global` is false, global
    /// entries are preserved (PFLUSHN).
    pub fn flush_page_040(
        &mut self,
        logical_page: u32,
        fc: u8,
        page_shift: u8,
        include_global: bool,
    ) {
        if let AtcStorage::M040(atc) = &mut self.atc {
            atc.bank_for_fc_mut(fc)
                .flush_page(logical_page, page_shift, include_global);
        }
    }
}

// ---------------------------------------------------------------------------
// Bus integration — translation fast path and table walk context
// ---------------------------------------------------------------------------

use crate::bus::FunctionCode;
use crate::microcode::MicroOp;
use crate::registers::Registers;

/// Result of the fast address translation path.
///
/// `translate_fast` checks TT registers and the ATC without doing any bus
/// reads. If the address misses the ATC, the caller must enter
/// `State::TableWalk` to read page table descriptors from memory.
#[derive(Debug)]
pub enum TranslateResult {
    /// MMU disabled — address passes through unchanged.
    Passthrough(u32),
    /// Translation succeeded (TT match or ATC hit). Contains physical address.
    Physical(u32),
    /// Write to write-protected page — caller should raise bus error.
    Fault,
    /// ATC miss — table walk required. Contains pre-computed walk context.
    NeedWalk(TableWalkContext),
}

/// The original bus operation suspended while a table walk is in progress.
#[derive(Debug, Clone)]
pub struct PendingBusCycle {
    /// Original micro-op that triggered the bus cycle.
    pub op: MicroOp,
    /// Logical address being translated.
    pub logical_addr: u32,
    /// Function code of the original access.
    pub fc: FunctionCode,
    /// True if the original access was a read.
    pub is_read: bool,
    /// True if the original access was word-sized.
    pub is_word: bool,
    /// Write data for the original access (None for reads).
    pub data: Option<u16>,
}

/// Progress state for a page table walk in flight.
///
/// Stored inside `State::TableWalk` while the CPU reads descriptors from
/// memory. Each descriptor read is a real bus cycle, so the walk may span
/// many ticks.
#[derive(Debug, Clone)]
pub struct TableWalkContext {
    /// The original bus operation suspended during this walk.
    pub pending: PendingBusCycle,

    // --- Walk state (common) ---
    /// Current table level (0-based).
    pub level: u8,
    /// Physical address of the next descriptor to read.
    pub next_descriptor_addr: u32,

    // --- 68030-specific walk state ---
    /// Page offset mask (lower bits that pass through unchanged).
    pub page_mask: u32,
    /// Page offset extracted from logical address.
    pub page_offset: u32,
    /// Index field widths [TIA, TIB, TIC, TID].
    pub index_fields: [u8; 4],
    /// Number of remaining shift bits for index extraction.
    pub remaining_shift: u8,

    // --- 68040-specific walk state ---
    /// True if this is a 68040 walk (fixed 3-level structure).
    pub is_040: bool,
    /// 68040: page shift (12 for 4KB, 13 for 8KB).
    pub page_shift_040: u8,

    // --- Accumulated protection bits ---
    pub write_protect: bool,
    pub cache_inhibit: bool,
}

/// Result of processing a single descriptor during a table walk.
#[derive(Debug)]
pub enum WalkStep {
    /// Need to read the next level descriptor at this physical address.
    NextLevel(u32),
    /// Walk completed successfully. Physical address ready.
    Complete {
        physical_addr: u32,
        write_protect: bool,
        cache_inhibit: bool,
    },
    /// Walk hit an invalid descriptor or protection violation.
    Fault,
}

impl Mmu {
    /// Fast-path address translation: checks TT registers and ATC.
    ///
    /// Does NOT perform any bus reads. Returns `NeedWalk` when the ATC misses
    /// and a full table walk is required.
    pub fn translate_fast(
        &mut self,
        regs: &Registers,
        logical_addr: u32,
        fc: FunctionCode,
        is_write: bool,
        pending: PendingBusCycle,
    ) -> TranslateResult {
        let fc_bits = fc.bits();

        match self.mode {
            MmuMode::Disabled => TranslateResult::Passthrough(logical_addr),

            MmuMode::M68030 => {
                let tc = Tc030::parse(regs.tc);
                if !tc.enabled {
                    return TranslateResult::Passthrough(logical_addr);
                }

                // Check TT0/TT1 for transparent translation.
                if let Some(tt) = tt_match_030(regs.itt0, logical_addr, fc_bits, is_write) {
                    let _ = tt; // TT match → address passes through
                    return TranslateResult::Physical(logical_addr);
                }
                if let Some(tt) = tt_match_030(regs.itt1, logical_addr, fc_bits, is_write) {
                    let _ = tt;
                    return TranslateResult::Physical(logical_addr);
                }

                // Check ATC.
                let page_mask = tc.page_mask();
                let page_key = logical_addr & !page_mask;
                if let AtcStorage::M030(atc) = &self.atc
                    && let Some(entry) = atc.lookup(page_key, fc_bits)
                {
                    if is_write && entry.write_protect {
                        return TranslateResult::Fault;
                    }
                    let physical = entry.physical_page | (logical_addr & page_mask);
                    return TranslateResult::Physical(physical);
                }

                // ATC miss — prepare walk context.
                let page_offset = logical_addr & page_mask;
                let fields = tc.index_fields();
                let remaining_shift = 32u8.saturating_sub(tc.initial_shift);

                // Select root pointer and compute first descriptor address.
                let root = select_root_pointer_030(
                    &tc,
                    fc_bits,
                    regs.srp,
                    regs.srp_upper,
                    regs.urp,
                    regs.crp_upper,
                );
                let root_dt = DescriptorType030::from_bits(root as u32);
                let table_base = (root as u32) & !0x3;

                // If root is invalid, fault immediately.
                if matches!(root_dt, DescriptorType030::Invalid) {
                    return TranslateResult::Fault;
                }
                // If root is a page descriptor, no walk needed — compute physical directly.
                if matches!(root_dt, DescriptorType030::Page) {
                    let frame_mask = !page_mask & 0xFFFF_FFF0;
                    let physical = (table_base & frame_mask) | page_offset;
                    return TranslateResult::Physical(physical);
                }

                // Compute first index into the root table.
                let first_field = fields[0];
                if first_field == 0 {
                    // No index fields → root IS the translation (shouldn't happen with valid TC).
                    return TranslateResult::Physical(table_base | page_offset);
                }
                let first_shift = remaining_shift.saturating_sub(first_field);
                let first_index = (logical_addr >> first_shift) & ((1u32 << first_field) - 1);
                let first_desc_addr = table_base.wrapping_add(first_index * 4);

                TranslateResult::NeedWalk(TableWalkContext {
                    pending,
                    level: 0,
                    next_descriptor_addr: first_desc_addr,
                    page_mask,
                    page_offset,
                    index_fields: fields,
                    remaining_shift: first_shift,
                    is_040: false,
                    page_shift_040: 0,
                    write_protect: false,
                    cache_inhibit: false,
                })
            }

            MmuMode::M68040 => {
                let tc = Tc040::parse(regs.tc);
                if !tc.enabled {
                    return TranslateResult::Passthrough(logical_addr);
                }

                // Check ITT/DTT transparent translation.
                let is_supervisor = matches!(
                    fc,
                    FunctionCode::SupervisorData | FunctionCode::SupervisorProgram
                );
                if let Some(_tt) = check_tt_040(
                    regs.itt0, regs.itt1, regs.dtt0, regs.dtt1,
                    logical_addr, fc_bits, is_supervisor,
                ) {
                    return TranslateResult::Physical(logical_addr);
                }

                // Check ATC.
                if let AtcStorage::M040(atc) = &self.atc {
                    let bank = atc.bank_for_fc(fc_bits);
                    if let Some(entry) = bank.lookup(logical_addr, fc_bits, tc.page_shift()) {
                        if is_write && entry.write_protect {
                            return TranslateResult::Fault;
                        }
                        let page_mask = (1u32 << tc.page_shift()) - 1;
                        let physical = entry.physical_page | (logical_addr & page_mask);
                        return TranslateResult::Physical(physical);
                    }
                }

                // ATC miss — prepare 68040 walk context.
                let page_shift = tc.page_shift();
                let page_mask = (1u32 << page_shift) - 1;
                let page_offset = logical_addr & page_mask;

                // Select root pointer.
                let is_supervisor = matches!(
                    fc,
                    FunctionCode::SupervisorData | FunctionCode::SupervisorProgram
                );
                let root_ptr = if is_supervisor { regs.srp } else { regs.urp };

                // First level: root index from bits 31-25 (always 7 bits).
                let root_index = (logical_addr >> 25) & 0x7F;
                let first_desc_addr = root_ptr.wrapping_add(root_index * 4);

                TranslateResult::NeedWalk(TableWalkContext {
                    pending,
                    level: 0,
                    next_descriptor_addr: first_desc_addr,
                    page_mask,
                    page_offset,
                    index_fields: [0; 4], // Not used for 68040
                    remaining_shift: 0,   // Not used for 68040
                    is_040: true,
                    page_shift_040: page_shift,
                    write_protect: false,
                    cache_inhibit: false,
                })
            }
        }
    }

    /// Process a 32-bit descriptor read during a table walk.
    ///
    /// Returns the next step: read another level, complete with a physical
    /// address, or fault.
    pub fn process_walk_descriptor(
        &mut self,
        descriptor: u32,
        ctx: &mut TableWalkContext,
        regs: &Registers,
    ) -> WalkStep {
        if ctx.is_040 {
            self.process_walk_descriptor_040(descriptor, ctx)
        } else {
            self.process_walk_descriptor_030(descriptor, ctx, regs)
        }
    }

    fn process_walk_descriptor_030(
        &mut self,
        descriptor: u32,
        ctx: &mut TableWalkContext,
        _regs: &Registers,
    ) -> WalkStep {
        let dt = DescriptorType030::from_bits(descriptor);

        match dt {
            DescriptorType030::Invalid => WalkStep::Fault,

            DescriptorType030::Page => {
                // Accumulate protection from page descriptor.
                if descriptor & (1 << 2) != 0 {
                    ctx.write_protect = true;
                }
                if descriptor & (1 << 6) != 0 {
                    ctx.cache_inhibit = true;
                }

                let frame_mask = !ctx.page_mask & 0xFFFF_FFF0;
                let physical = (descriptor & frame_mask) | ctx.page_offset;

                // Insert into ATC.
                let fc_bits = ctx.pending.fc.bits();
                let page_key = ctx.pending.logical_addr & !ctx.page_mask;
                if let AtcStorage::M030(atc) = &mut self.atc {
                    atc.insert(AtcEntry {
                        logical_page: page_key,
                        physical_page: descriptor & frame_mask,
                        fc: fc_bits,
                        valid: true,
                        write_protect: ctx.write_protect,
                        cache_inhibit: ctx.cache_inhibit,
                        modified: false,
                        global: false,
                    });
                }

                WalkStep::Complete {
                    physical_addr: physical,
                    write_protect: ctx.write_protect,
                    cache_inhibit: ctx.cache_inhibit,
                }
            }

            DescriptorType030::Pointer4 => {
                // 4-byte pointer — advance to next table level.
                let table_base = descriptor & !0x3;
                ctx.level += 1;

                // Compute next index.
                let next_field_idx = (ctx.level) as usize;
                if next_field_idx >= 4 || ctx.index_fields[next_field_idx] == 0 {
                    // No more index fields — this pointer IS the page.
                    // "Early termination page" — treat current table_base as page frame.
                    let frame_mask = !ctx.page_mask & 0xFFFF_FFFC;
                    let physical = (table_base & frame_mask) | ctx.page_offset;
                    return WalkStep::Complete {
                        physical_addr: physical,
                        write_protect: ctx.write_protect,
                        cache_inhibit: ctx.cache_inhibit,
                    };
                }

                let field_width = ctx.index_fields[next_field_idx];
                ctx.remaining_shift = ctx.remaining_shift.saturating_sub(field_width);
                let index = (ctx.pending.logical_addr >> ctx.remaining_shift)
                    & ((1u32 << field_width) - 1);
                ctx.next_descriptor_addr = table_base.wrapping_add(index * 4);
                WalkStep::NextLevel(ctx.next_descriptor_addr)
            }

            DescriptorType030::Pointer8 => {
                // 8-byte pointer — first long is table address, second long has
                // protection bits. We only have the first long here. We need to
                // read the second long separately.
                //
                // For simplicity in the state machine, treat DT=3 pointers the
                // same as DT=2 (the second long's WP/CI are accumulated later
                // if the walker reads it). TODO: add a sub-state for 8-byte
                // descriptor second-long reads.
                let table_base = descriptor & !0x3;
                ctx.level += 1;

                let next_field_idx = (ctx.level) as usize;
                if next_field_idx >= 4 || ctx.index_fields[next_field_idx] == 0 {
                    let frame_mask = !ctx.page_mask & 0xFFFF_FFFC;
                    let physical = (table_base & frame_mask) | ctx.page_offset;
                    return WalkStep::Complete {
                        physical_addr: physical,
                        write_protect: ctx.write_protect,
                        cache_inhibit: ctx.cache_inhibit,
                    };
                }

                let field_width = ctx.index_fields[next_field_idx];
                ctx.remaining_shift = ctx.remaining_shift.saturating_sub(field_width);
                let index = (ctx.pending.logical_addr >> ctx.remaining_shift)
                    & ((1u32 << field_width) - 1);
                ctx.next_descriptor_addr = table_base.wrapping_add(index * 4);
                WalkStep::NextLevel(ctx.next_descriptor_addr)
            }
        }
    }

    fn process_walk_descriptor_040(
        &mut self,
        descriptor: u32,
        ctx: &mut TableWalkContext,
    ) -> WalkStep {
        let dt = descriptor & 3;

        // DT=0: invalid
        if dt == 0 {
            return WalkStep::Fault;
        }

        match ctx.level {
            // Root and pointer levels (0, 1)
            0 | 1 => {
                if dt == 1 {
                    // Resident page descriptor at non-leaf level → treat as page.
                    return self.finish_040_walk(descriptor, ctx);
                }
                // DT=2 (resident pointer) or DT=3 (indirect pointer)
                let table_base = if dt == 3 {
                    // Indirect: descriptor points to the actual descriptor.
                    // For simplicity, treat as direct pointer (descriptor & ~3).
                    // Full indirect support would need another bus read.
                    descriptor & !0x3
                } else {
                    descriptor & !0x3
                };

                ctx.level += 1;

                // Compute next level index.
                let index = match ctx.level {
                    1 => {
                        // Pointer level: bits 24-18 (7 bits)
                        (ctx.pending.logical_addr >> 18) & 0x7F
                    }
                    2 => {
                        // Page level: bits 17-12 (6 bits for 4KB) or 17-13 (5 bits for 8KB)
                        let page_bits = if ctx.page_shift_040 == 12 { 6 } else { 5 };
                        let shift = ctx.page_shift_040;
                        (ctx.pending.logical_addr >> shift) & ((1u32 << page_bits) - 1)
                    }
                    _ => return WalkStep::Fault,
                };

                ctx.next_descriptor_addr = table_base.wrapping_add(index * 4);
                WalkStep::NextLevel(ctx.next_descriptor_addr)
            }

            // Leaf level (2) — page descriptor
            2 => self.finish_040_walk(descriptor, ctx),

            _ => WalkStep::Fault,
        }
    }

    fn finish_040_walk(&mut self, descriptor: u32, ctx: &mut TableWalkContext) -> WalkStep {
        let page_mask = ctx.page_mask;
        let page_offset = ctx.page_offset;

        // Extract protection bits.
        let write_protect = descriptor & (1 << 2) != 0;
        let cache_inhibit = (descriptor >> 5) & 3 == 2; // CM=10 = cache-inhibit
        let global = descriptor & (1 << 10) != 0;
        let _modified = descriptor & (1 << 4) != 0;

        let frame_mask = !page_mask & 0xFFFF_FFF0;
        let physical = (descriptor & frame_mask) | page_offset;

        // Insert into ATC.
        let fc_bits = ctx.pending.fc.bits();
        if let AtcStorage::M040(atc) = &mut self.atc {
            let bank = atc.bank_for_fc_mut(fc_bits);
            bank.insert(
                AtcEntry {
                    logical_page: ctx.pending.logical_addr & !page_mask,
                    physical_page: descriptor & frame_mask,
                    fc: fc_bits,
                    valid: true,
                    write_protect,
                    cache_inhibit,
                    modified: false,
                    global,
                },
                ctx.page_shift_040,
            );
        }

        WalkStep::Complete {
            physical_addr: physical,
            write_protect: write_protect || ctx.write_protect,
            cache_inhibit: cache_inhibit || ctx.cache_inhibit,
        }
    }
}

// ---------------------------------------------------------------------------
// 68030 table walker
// ---------------------------------------------------------------------------

/// Descriptor type field (DT, bits 1–0) in 68030 table/page descriptors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorType030 {
    /// DT = 0: Invalid.
    Invalid,
    /// DT = 1: Page descriptor (valid only at leaf level).
    Page,
    /// DT = 2: Valid 4-byte table pointer.
    Pointer4,
    /// DT = 3: Valid 8-byte table pointer (second long has additional info).
    Pointer8,
}

impl DescriptorType030 {
    fn from_bits(dt: u32) -> Self {
        match dt & 3 {
            0 => Self::Invalid,
            1 => Self::Page,
            2 => Self::Pointer4,
            3 => Self::Pointer8,
            _ => unreachable!(),
        }
    }
}

/// Result of a 68030 table walk.
#[derive(Debug, Clone, Copy)]
pub enum WalkResult030 {
    /// Walk succeeded — physical address + protection bits.
    Ok {
        physical_addr: u32,
        write_protect: bool,
        cache_inhibit: bool,
        modified: bool,
    },
    /// Walk hit an invalid descriptor.
    Fault {
        /// MMUSR-format status bits.
        status: u16,
        /// Table level where the fault occurred (0-based).
        level: u8,
    },
}

/// Walk a 68030 page table to translate a logical address.
///
/// `tc` — parsed TC register.
/// `root_descriptor` — 64-bit root pointer (SRP or CRP). The upper 32 bits
///     hold the limit/descriptor type; the lower 32 bits hold the table base.
/// `logical_addr` — address to translate.
/// `fc` — function code of the access.
/// `is_write` — true if this is a write access (for WP checks).
/// `read_long` — closure that reads a 32-bit value from a physical address.
///     Called once per table level (or twice for 8-byte descriptors).
pub fn walk_030(
    tc: &Tc030,
    root_descriptor: u64,
    logical_addr: u32,
    _fc: u8,
    _is_write: bool,
    mut read_long: impl FnMut(u32) -> u32,
) -> WalkResult030 {
    let page_mask = tc.page_mask();
    let page_offset = logical_addr & page_mask;
    let fields = tc.index_fields();

    // Accumulated protection bits across all levels.
    let mut write_protect = false;
    let mut cache_inhibit = false;
    let mut modified = false;

    // Parse the root pointer descriptor.
    // Upper 32 bits: bits 1-0 = DT of root.
    // Lower 32 bits: table base address (descriptor address bits 31-2, bits 1-0 = DT).
    // For root pointers, the lower word's bits 1-0 are the DT.
    let root_dt = DescriptorType030::from_bits(root_descriptor as u32);
    let mut table_base = (root_descriptor as u32) & !0x3; // Mask off DT bits

    match root_dt {
        DescriptorType030::Invalid => {
            return WalkResult030::Fault {
                status: 0,
                level: 0,
            };
        }
        DescriptorType030::Page => {
            // Root is a page descriptor — entire address space maps to one page.
            // Extract physical base, apply offset.
            let physical_base = table_base & !page_mask;
            return WalkResult030::Ok {
                physical_addr: physical_base | page_offset,
                write_protect,
                cache_inhibit,
                modified,
            };
        }
        DescriptorType030::Pointer4 | DescriptorType030::Pointer8 => {
            // Root points to a table — continue walking.
        }
    }

    // Walk each index level (TIA, TIB, TIC, TID).
    // The address is shifted by IS first, then each index field extracts bits
    // from the shifted address, most significant first.
    let mut remaining_shift = 32u8.saturating_sub(tc.initial_shift);

    for (level_idx, &field_width) in fields.iter().enumerate() {
        if field_width == 0 {
            break;
        }

        // Extract index from logical address.
        remaining_shift = remaining_shift.saturating_sub(field_width);
        let index = (logical_addr >> remaining_shift) & ((1u32 << field_width) - 1);

        // Read descriptor at table_base + index * 4.
        let descriptor_addr = table_base.wrapping_add(index * 4);
        let descriptor = read_long(descriptor_addr);

        let dt = DescriptorType030::from_bits(descriptor);

        // Protection bits (WP, CI, M) are NOT present in short pointer
        // descriptors (DT=2). They only exist in:
        // - Page descriptors (DT=1): WP=bit 2, CI=bit 6, M=bit 4
        // - 8-byte pointer second long (DT=3): WP=bit 2, CI=bit 6

        match dt {
            DescriptorType030::Invalid => {
                // Build MMUSR: B (bus error) = 0, L (level) in bits 2-0,
                // I (invalid) = bit 10.
                let status = (1u16 << 10) | (level_idx as u16 & 7);
                return WalkResult030::Fault {
                    status,
                    level: level_idx as u8,
                };
            }
            DescriptorType030::Page => {
                // Leaf: accumulate protection bits from this page descriptor.
                if descriptor & (1 << 2) != 0 {
                    write_protect = true;
                }
                if descriptor & (1 << 6) != 0 {
                    cache_inhibit = true;
                }
                if descriptor & (1 << 4) != 0 {
                    modified = true;
                }

                // Extract physical base: bits above page offset, clear control
                // bits (low 4 bits are DT/WP/M).
                let frame_mask = !page_mask & 0xFFFF_FFF0;
                let physical_base = descriptor & frame_mask;
                return WalkResult030::Ok {
                    physical_addr: physical_base | page_offset,
                    write_protect,
                    cache_inhibit,
                    modified,
                };
            }
            DescriptorType030::Pointer4 => {
                // 4-byte pointer: bits 31-2 = next table base. No protection bits.
                table_base = descriptor & !0x3;
            }
            DescriptorType030::Pointer8 => {
                // 8-byte pointer: first long bits 31-2 = next table base.
                table_base = descriptor & !0x3;
                // Second long carries protection/control bits.
                let descriptor_lo = read_long(descriptor_addr.wrapping_add(4));
                // WP = bit 2, CI = bit 6 of second long.
                if descriptor_lo & (1 << 2) != 0 {
                    write_protect = true;
                }
                if descriptor_lo & (1 << 6) != 0 {
                    cache_inhibit = true;
                }
            }
        }
    }

    // If we exhausted all index fields without finding a page descriptor,
    // the last pointer IS the page descriptor (early termination page).
    // The current table_base is the physical page frame.
    let frame_mask = !page_mask & 0xFFFF_FFFC;
    let physical_base = table_base & frame_mask;
    WalkResult030::Ok {
        physical_addr: physical_base | page_offset,
        write_protect,
        cache_inhibit,
        modified,
    }
}

/// Select the root pointer for a 68030 table walk.
///
/// When SRE is set in TC, supervisor accesses use SRP, user accesses use CRP.
/// When SRE is clear, CRP is always used.
///
/// Returns the 64-bit root pointer descriptor (upper, lower).
#[must_use]
pub fn select_root_pointer_030(
    tc: &Tc030,
    fc: u8,
    srp: u32,
    srp_upper: u32,
    urp: u32,
    crp_upper: u32,
) -> u64 {
    let is_supervisor = fc == 5 || fc == 6;
    if tc.sre && is_supervisor {
        (u64::from(srp_upper) << 32) | u64::from(srp)
    } else {
        (u64::from(crp_upper) << 32) | u64::from(urp)
    }
}

// ---------------------------------------------------------------------------
// 68040 table walker
// ---------------------------------------------------------------------------

/// Result of a 68040 table walk.
#[derive(Debug, Clone, Copy)]
pub enum WalkResult040 {
    /// Walk succeeded.
    Ok {
        physical_addr: u32,
        write_protect: bool,
        cache_mode: u8,
        modified: bool,
        global: bool,
    },
    /// Walk hit an invalid descriptor or a supervisor-only page in user mode.
    Fault {
        /// SSW-format bits for the 68040 access error frame.
        status: u16,
    },
}

/// Walk a 68040 page table to translate a logical address.
///
/// The 68040 uses a fixed 3-level structure:
/// - 4KB pages: 7-7-6 index split (root 31–25, pointer 24–18, page 17–12)
/// - 8KB pages: 7-7-5 index split (root 31–25, pointer 24–18, page 17–13)
///
/// `tc` — parsed TC register.
/// `root_ptr` — 32-bit root pointer (SRP or URP).
/// `logical_addr` — address to translate.
/// `fc` — function code.
/// `is_write` — true for write access.
/// `read_long` — closure reading a 32-bit value from a physical address.
pub fn walk_040(
    tc: &Tc040,
    root_ptr: u32,
    logical_addr: u32,
    _fc: u8,
    is_write: bool,
    mut read_long: impl FnMut(u32) -> u32,
) -> WalkResult040 {
    let page_shift = tc.page_shift();
    let page_mask = tc.page_mask();
    let page_offset = logical_addr & page_mask;

    // Accumulated protection.
    let mut write_protect = false;

    // Level 1 (root): index from bits 31–25 (always 7 bits).
    let root_index = (logical_addr >> 25) & 0x7F;
    let root_desc = read_long(root_ptr.wrapping_add(root_index * 4));

    let root_udt = root_desc & 3;
    if root_udt < 2 {
        // UDT 0 or 1: invalid.
        return WalkResult040::Fault { status: 0 };
    }
    // UDT 2 or 3: resident pointer descriptor.
    // Bit 2: Write protect.
    if root_desc & (1 << 2) != 0 {
        write_protect = true;
    }
    let pointer_table_base = root_desc & 0xFFFF_FF00; // Bits 31–8 = table address (aligned).

    // Level 2 (pointer): index from bits 24–18 (always 7 bits).
    let pointer_index = (logical_addr >> 18) & 0x7F;
    let pointer_desc = read_long(pointer_table_base.wrapping_add(pointer_index * 4));

    let pointer_udt = pointer_desc & 3;
    if pointer_udt < 2 {
        return WalkResult040::Fault { status: 0 };
    }
    if pointer_desc & (1 << 2) != 0 {
        write_protect = true;
    }
    let page_table_base = pointer_desc & 0xFFFF_FF00;

    // Level 3 (page): index depends on page size.
    // 4KB: bits 17–12 (6 bits), 8KB: bits 17–13 (5 bits).
    let page_index_bits = 18 - page_shift; // 6 for 4KB, 5 for 8KB
    let page_index = (logical_addr >> page_shift) & ((1u32 << page_index_bits) - 1);
    let page_desc = read_long(page_table_base.wrapping_add(page_index * 4));

    let page_pdt = page_desc & 3;
    match page_pdt {
        0 => {
            // Invalid.
            return WalkResult040::Fault { status: 0 };
        }
        1 => {
            // Resident page descriptor.
        }
        2 => {
            // Invalid for page descriptors.
            return WalkResult040::Fault { status: 0 };
        }
        3 => {
            // Indirect: descriptor holds pointer to the real page descriptor.
            // Read the indirect descriptor.
            let indirect_addr = page_desc & 0xFFFF_FFFC;
            let real_desc = read_long(indirect_addr);
            let real_pdt = real_desc & 3;
            if real_pdt != 1 {
                return WalkResult040::Fault { status: 0 };
            }
            // Use the indirect descriptor as the page descriptor.
            return finish_040_page(
                real_desc,
                page_offset,
                page_mask,
                write_protect,
                is_write,
            );
        }
        _ => unreachable!(),
    }

    finish_040_page(page_desc, page_offset, page_mask, write_protect, is_write)
}

/// Extract fields from a 68040 resident page descriptor and produce the walk result.
fn finish_040_page(
    desc: u32,
    page_offset: u32,
    page_mask: u32,
    mut write_protect: bool,
    is_write: bool,
) -> WalkResult040 {
    // Page descriptor fields:
    // Bit 2: Write protect (W).
    // Bit 3: Used (U) — set on access.
    // Bit 4: Modified (M) — set on write.
    // Bit 5: Cache mode bit 0 (CM0).
    // Bit 6: Cache mode bit 1 (CM1).
    // Bit 7: Supervisor only (S).
    // Bit 10: Global (G).
    if desc & (1 << 2) != 0 {
        write_protect = true;
    }

    let modified = desc & (1 << 4) != 0;
    let cm = ((desc >> 5) & 3) as u8;
    let global = desc & (1 << 10) != 0;

    // Check write-protect violation.
    if is_write && write_protect {
        return WalkResult040::Fault { status: 0 };
    }

    let physical_base = desc & !page_mask & 0xFFFF_FF00;
    WalkResult040::Ok {
        physical_addr: physical_base | page_offset,
        write_protect,
        cache_mode: cm,
        modified,
        global,
    }
}

/// Select the root pointer for a 68040 table walk.
///
/// Supervisor accesses (FC 5/6) use SRP; user accesses (FC 1/2) use URP.
#[must_use]
pub fn select_root_pointer_040(fc: u8, srp: u32, urp: u32) -> u32 {
    if fc == 5 || fc == 6 { srp } else { urp }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- MmuMode selection ---

    #[test]
    fn mode_disabled_for_68000_and_68010() {
        assert_eq!(MmuMode::from_model(CpuModel::M68000), MmuMode::Disabled);
        assert_eq!(MmuMode::from_model(CpuModel::M68010), MmuMode::Disabled);
    }

    #[test]
    fn mode_disabled_for_ec_variants() {
        assert_eq!(MmuMode::from_model(CpuModel::M68EC020), MmuMode::Disabled);
        assert_eq!(MmuMode::from_model(CpuModel::M68EC030), MmuMode::Disabled);
        assert_eq!(MmuMode::from_model(CpuModel::M68EC040), MmuMode::Disabled);
        assert_eq!(MmuMode::from_model(CpuModel::M68EC060), MmuMode::Disabled);
    }

    #[test]
    fn mode_030_for_mmu_capable_020_030() {
        assert_eq!(MmuMode::from_model(CpuModel::M68020), MmuMode::M68030);
        assert_eq!(MmuMode::from_model(CpuModel::M68LC030), MmuMode::M68030);
        assert_eq!(MmuMode::from_model(CpuModel::M68030), MmuMode::M68030);
    }

    #[test]
    fn mode_040_for_mmu_capable_040_060() {
        assert_eq!(MmuMode::from_model(CpuModel::M68LC040), MmuMode::M68040);
        assert_eq!(MmuMode::from_model(CpuModel::M68040), MmuMode::M68040);
        assert_eq!(MmuMode::from_model(CpuModel::M68LC060), MmuMode::M68040);
        assert_eq!(MmuMode::from_model(CpuModel::M68060), MmuMode::M68040);
    }

    // --- TC parsing (68030) ---

    #[test]
    fn tc030_disabled() {
        let tc = Tc030::parse(0);
        assert!(!tc.enabled);
    }

    #[test]
    fn tc030_typical_amiga_3level() {
        // Enable=1, PS=12 (4KB), IS=0, TIA=7, TIB=7, TIC=6, TID=0
        let raw = (1u32 << 31) | (12 << 20) | (7 << 12) | (7 << 8) | (6 << 4);
        let tc = Tc030::parse(raw);
        assert!(tc.enabled);
        assert!(!tc.sre);
        assert!(!tc.fcl);
        assert_eq!(tc.page_shift, 12);
        assert_eq!(tc.initial_shift, 0);
        assert_eq!(tc.tia, 7);
        assert_eq!(tc.tib, 7);
        assert_eq!(tc.tic, 6);
        assert_eq!(tc.tid, 0);
        assert_eq!(tc.page_size(), 4096);
        assert_eq!(tc.page_mask(), 0xFFF);
        assert_eq!(tc.num_levels(), 3);
    }

    #[test]
    fn tc030_with_sre_and_initial_shift() {
        let raw = (1u32 << 31) | (1 << 25) | (10 << 20) | (4 << 16) | (5 << 12) | (5 << 8);
        let tc = Tc030::parse(raw);
        assert!(tc.enabled);
        assert!(tc.sre);
        assert_eq!(tc.page_shift, 10);
        assert_eq!(tc.initial_shift, 4);
        assert_eq!(tc.tia, 5);
        assert_eq!(tc.tib, 5);
        assert_eq!(tc.num_levels(), 2);
        assert_eq!(tc.page_size(), 1024);
    }

    // --- TC parsing (68040) ---

    #[test]
    fn tc040_4kb() {
        let tc = Tc040::parse(1 << 15);
        assert!(tc.enabled);
        assert!(!tc.page_8k);
        assert_eq!(tc.page_shift(), 12);
        assert_eq!(tc.page_size(), 4096);
    }

    #[test]
    fn tc040_8kb() {
        let tc = Tc040::parse((1 << 15) | (1 << 14));
        assert!(tc.enabled);
        assert!(tc.page_8k);
        assert_eq!(tc.page_shift(), 13);
        assert_eq!(tc.page_size(), 8192);
    }

    #[test]
    fn tc040_disabled() {
        let tc = Tc040::parse(0);
        assert!(!tc.enabled);
    }

    // --- TT matching (68030) ---

    /// Build a 68030 TT register value from fields.
    fn make_tt030(
        addr_base: u8,
        addr_mask: u8,
        enable: bool,
        ci: bool,
        rw: bool,
        rwm: bool,
        fc_base: u8,
        fc_mask: u8,
    ) -> u32 {
        (u32::from(addr_base) << 24)
            | (u32::from(addr_mask) << 16)
            | if enable { 1 << 15 } else { 0 }
            | if ci { 1 << 14 } else { 0 }
            | if rw { 1 << 13 } else { 0 }
            | if rwm { 1 << 12 } else { 0 }
            | (u32::from(fc_base & 7) << 4)
            | u32::from(fc_mask & 7)
    }

    #[test]
    fn tt030_disabled_returns_none() {
        let tt = make_tt030(0, 0xFF, false, false, false, true, 5, 0);
        assert!(tt_match_030(tt, 0x0040_0000, 5, false).is_none());
    }

    #[test]
    fn tt030_supervisor_data_all_addrs() {
        let tt = make_tt030(0x00, 0xFF, true, false, false, true, 5, 0);
        // FC=5 matches
        assert!(tt_match_030(tt, 0x1234_5678, 5, false).is_some());
        assert!(tt_match_030(tt, 0x1234_5678, 5, true).is_some());
        // FC=1 does not match (fc_mask=0 → exact)
        assert!(tt_match_030(tt, 0x1234_5678, 1, false).is_none());
    }

    #[test]
    fn tt030_address_base_mask() {
        // Match only addresses where A31-A24 = 0x04
        let tt = make_tt030(0x04, 0x00, true, false, false, true, 5, 7);
        assert!(tt_match_030(tt, 0x0400_0000, 5, false).is_some());
        assert!(tt_match_030(tt, 0x04FF_FFFF, 5, false).is_some());
        assert!(tt_match_030(tt, 0x0500_0000, 5, false).is_none());
    }

    #[test]
    fn tt030_rw_filtering() {
        // R/W=0 (read), RWM=0 (check R/W field)
        let tt = make_tt030(0x00, 0xFF, true, false, false, false, 5, 0);
        assert!(tt_match_030(tt, 0, 5, false).is_some());
        assert!(tt_match_030(tt, 0, 5, true).is_none());

        // R/W=1 (write), RWM=0
        let tt = make_tt030(0x00, 0xFF, true, false, true, false, 5, 0);
        assert!(tt_match_030(tt, 0, 5, true).is_some());
        assert!(tt_match_030(tt, 0, 5, false).is_none());
    }

    #[test]
    fn tt030_cache_inhibit() {
        let tt = make_tt030(0x00, 0xFF, true, true, false, true, 5, 7);
        let r = tt_match_030(tt, 0, 5, false).unwrap();
        assert!(r.cache_inhibit);
        assert!(!r.write_protect);
    }

    #[test]
    fn tt030_fc_mask_partial() {
        // FC_base=4 (0b100), FC_mask=3 (0b011) → matches FCs 4,5,6,7
        let tt = make_tt030(0x00, 0xFF, true, false, false, true, 4, 3);
        assert!(tt_match_030(tt, 0, 4, false).is_some());
        assert!(tt_match_030(tt, 0, 5, false).is_some());
        assert!(tt_match_030(tt, 0, 6, false).is_some());
        assert!(tt_match_030(tt, 0, 7, false).is_some());
        assert!(tt_match_030(tt, 0, 0, false).is_none());
        assert!(tt_match_030(tt, 0, 1, false).is_none());
    }

    // --- TT matching (68040) ---

    /// Build a 68040 TT register value from fields.
    fn make_tt040(
        addr_base: u8,
        addr_mask: u8,
        enable: bool,
        s_field: u8,
        cm: u8,
        w: bool,
    ) -> u32 {
        (u32::from(addr_base) << 24)
            | (u32::from(addr_mask) << 16)
            | if enable { 1 << 15 } else { 0 }
            | (u32::from(s_field & 3) << 13)
            | (u32::from(cm & 3) << 6)
            | if w { 1 << 2 } else { 0 }
    }

    #[test]
    fn tt040_disabled() {
        let tt = make_tt040(0, 0xFF, false, 0b10, 0, false);
        assert!(tt_match_040(tt, 0, true).is_none());
    }

    #[test]
    fn tt040_supervisor_only() {
        let tt = make_tt040(0, 0xFF, true, 0b01, 0, false);
        assert!(tt_match_040(tt, 0, true).is_some());
        assert!(tt_match_040(tt, 0, false).is_none());
    }

    #[test]
    fn tt040_user_only() {
        let tt = make_tt040(0, 0xFF, true, 0b00, 0, false);
        assert!(tt_match_040(tt, 0, false).is_some());
        assert!(tt_match_040(tt, 0, true).is_none());
    }

    #[test]
    fn tt040_both_modes() {
        let tt = make_tt040(0, 0xFF, true, 0b10, 0, false);
        assert!(tt_match_040(tt, 0, true).is_some());
        assert!(tt_match_040(tt, 0, false).is_some());
        // s_field=0b11 also matches both
        let tt = make_tt040(0, 0xFF, true, 0b11, 0, false);
        assert!(tt_match_040(tt, 0, true).is_some());
        assert!(tt_match_040(tt, 0, false).is_some());
    }

    #[test]
    fn tt040_write_protect() {
        let tt = make_tt040(0, 0xFF, true, 0b10, 0, true);
        let r = tt_match_040(tt, 0, true).unwrap();
        assert!(r.write_protect);
    }

    #[test]
    fn tt040_cache_modes() {
        for cm in 0..4u8 {
            let tt = make_tt040(0, 0xFF, true, 0b10, cm, false);
            let r = tt_match_040(tt, 0, true).unwrap();
            assert_eq!(r.cache_inhibit, cm >= 2, "CM={cm}");
        }
    }

    #[test]
    fn tt040_address_filtering() {
        let tt = make_tt040(0x04, 0x00, true, 0b10, 0, false);
        assert!(tt_match_040(tt, 0x0400_0000, true).is_some());
        assert!(tt_match_040(tt, 0x0500_0000, true).is_none());
    }

    // --- check_tt_040 instruction vs data ---

    #[test]
    fn check_tt_040_routes_by_fc() {
        let itt0 = make_tt040(0x00, 0xFF, true, 0b10, 0, false);
        let dtt0 = make_tt040(0x00, 0xFF, true, 0b10, 2, false); // CI
        let disabled = 0u32;

        // Instruction fetch (FC=6) → ITT0 → not CI
        let r = check_tt_040(itt0, disabled, dtt0, disabled, 0, 6, true).unwrap();
        assert!(!r.cache_inhibit);

        // Data access (FC=5) → DTT0 → CI
        let r = check_tt_040(itt0, disabled, dtt0, disabled, 0, 5, true).unwrap();
        assert!(r.cache_inhibit);
    }

    // --- ATC 030 ---

    #[test]
    fn atc030_insert_and_lookup() {
        let mut atc = Atc030::new();
        atc.insert(AtcEntry {
            logical_page: 0x0004_0000,
            physical_page: 0x0010_0000,
            fc: 5,
            valid: true,
            ..AtcEntry::EMPTY
        });

        let found = atc.lookup(0x0004_0000, 5);
        assert!(found.is_some());
        assert_eq!(found.unwrap().physical_page, 0x0010_0000);

        // Wrong FC → miss
        assert!(atc.lookup(0x0004_0000, 1).is_none());
        // Wrong page → miss
        assert!(atc.lookup(0x0005_0000, 5).is_none());
    }

    #[test]
    fn atc030_replace_existing() {
        let mut atc = Atc030::new();
        atc.insert(AtcEntry {
            logical_page: 0x1000,
            physical_page: 0x2000,
            fc: 5,
            valid: true,
            ..AtcEntry::EMPTY
        });
        atc.insert(AtcEntry {
            logical_page: 0x1000,
            physical_page: 0x9000,
            fc: 5,
            valid: true,
            ..AtcEntry::EMPTY
        });
        assert_eq!(atc.lookup(0x1000, 5).unwrap().physical_page, 0x9000);
    }

    #[test]
    fn atc030_flush_all() {
        let mut atc = Atc030::new();
        for i in 0..5u32 {
            atc.insert(AtcEntry {
                logical_page: i * 0x1000,
                physical_page: i * 0x1000 + 0x10_0000,
                fc: 5,
                valid: true,
                ..AtcEntry::EMPTY
            });
        }
        assert!(atc.lookup(0x2000, 5).is_some());
        atc.flush_all();
        assert!(atc.lookup(0x2000, 5).is_none());
    }

    #[test]
    fn atc030_flush_by_fc() {
        let mut atc = Atc030::new();
        atc.insert(AtcEntry {
            logical_page: 0x1000,
            physical_page: 0x2000,
            fc: 5,
            valid: true,
            ..AtcEntry::EMPTY
        });
        atc.insert(AtcEntry {
            logical_page: 0x1000,
            physical_page: 0x3000,
            fc: 1,
            valid: true,
            ..AtcEntry::EMPTY
        });
        atc.flush_by_fc(5);
        assert!(atc.lookup(0x1000, 5).is_none());
        assert!(atc.lookup(0x1000, 1).is_some());
    }

    #[test]
    fn atc030_flush_by_addr() {
        let mut atc = Atc030::new();
        atc.insert(AtcEntry {
            logical_page: 0x1000,
            physical_page: 0x2000,
            fc: 5,
            valid: true,
            ..AtcEntry::EMPTY
        });
        atc.insert(AtcEntry {
            logical_page: 0x2000,
            physical_page: 0x3000,
            fc: 5,
            valid: true,
            ..AtcEntry::EMPTY
        });
        atc.flush_by_addr(0x1000, 5);
        assert!(atc.lookup(0x1000, 5).is_none());
        assert!(atc.lookup(0x2000, 5).is_some());
    }

    #[test]
    fn atc030_fifo_eviction() {
        let mut atc = Atc030::new();
        for i in 0..ATC_030_SIZE as u32 {
            atc.insert(AtcEntry {
                logical_page: i * 0x1000,
                physical_page: i * 0x1000 + 0x100_0000,
                fc: 5,
                valid: true,
                ..AtcEntry::EMPTY
            });
        }
        // All present
        for i in 0..ATC_030_SIZE as u32 {
            assert!(atc.lookup(i * 0x1000, 5).is_some(), "slot {i} missing");
        }
        // 23rd evicts slot 0
        atc.insert(AtcEntry {
            logical_page: ATC_030_SIZE as u32 * 0x1000,
            physical_page: 0xFFF_0000,
            fc: 5,
            valid: true,
            ..AtcEntry::EMPTY
        });
        assert!(atc.lookup(0, 5).is_none());
        assert!(atc.lookup(ATC_030_SIZE as u32 * 0x1000, 5).is_some());
    }

    // --- ATC 040 ---

    #[test]
    fn atc040_insert_and_lookup() {
        let mut bank = Atc040Bank::new();
        let ps = 12;
        bank.insert(
            AtcEntry {
                logical_page: 0x0004_0000,
                physical_page: 0x0010_0000,
                fc: 5,
                valid: true,
                ..AtcEntry::EMPTY
            },
            ps,
        );
        assert!(bank.lookup(0x0004_0000, 5, ps).is_some());
        assert!(bank.lookup(0x0004_0000, 1, ps).is_none());
        assert!(bank.lookup(0x0005_0000, 5, ps).is_none());
    }

    #[test]
    fn atc040_set_associative_eviction() {
        let mut bank = Atc040Bank::new();
        let ps = 12;
        // Pages 0x0, 0x1_0000, 0x2_0000, 0x3_0000 all hash to set 0
        // (page_number & 0xF = 0x0, 0x10&F=0, 0x20&F=0, 0x30&F=0)
        for i in 0..4u32 {
            bank.insert(
                AtcEntry {
                    logical_page: i * 0x1_0000,
                    physical_page: i * 0x1_0000 + 0x100_0000,
                    fc: 5,
                    valid: true,
                    ..AtcEntry::EMPTY
                },
                ps,
            );
        }
        for i in 0..4u32 {
            assert!(bank.lookup(i * 0x1_0000, 5, ps).is_some());
        }
        // 5th in same set evicts way 0
        bank.insert(
            AtcEntry {
                logical_page: 0x4_0000,
                physical_page: 0x104_0000,
                fc: 5,
                valid: true,
                ..AtcEntry::EMPTY
            },
            ps,
        );
        assert!(bank.lookup(0, 5, ps).is_none());
        assert!(bank.lookup(0x4_0000, 5, ps).is_some());
    }

    #[test]
    fn atc040_global_preserved_by_pflushn() {
        let mut bank = Atc040Bank::new();
        let ps = 12;
        bank.insert(
            AtcEntry {
                logical_page: 0x1000,
                physical_page: 0x2000,
                fc: 5,
                valid: true,
                global: true,
                ..AtcEntry::EMPTY
            },
            ps,
        );
        // PFLUSHN (include_global=false) preserves global
        bank.flush_page(0x1000, ps, false);
        assert!(bank.lookup(0x1000, 5, ps).is_some());
        // PFLUSH (include_global=true) flushes everything
        bank.flush_page(0x1000, ps, true);
        assert!(bank.lookup(0x1000, 5, ps).is_none());
    }

    #[test]
    fn atc040_flush_all() {
        let mut atc = Atc040::new();
        let ps = 12;
        atc.data.insert(
            AtcEntry {
                logical_page: 0x1000,
                physical_page: 0x2000,
                fc: 5,
                valid: true,
                ..AtcEntry::EMPTY
            },
            ps,
        );
        atc.instruction.insert(
            AtcEntry {
                logical_page: 0x1000,
                physical_page: 0x3000,
                fc: 6,
                valid: true,
                ..AtcEntry::EMPTY
            },
            ps,
        );
        atc.flush_all();
        assert!(atc.data.lookup(0x1000, 5, ps).is_none());
        assert!(atc.instruction.lookup(0x1000, 6, ps).is_none());
    }

    #[test]
    fn atc040_bank_routing() {
        let atc = Atc040::new();
        // FC=1 (user data) → data bank
        assert!(std::ptr::eq(atc.bank_for_fc(1), &atc.data));
        // FC=5 (supervisor data) → data bank
        assert!(std::ptr::eq(atc.bank_for_fc(5), &atc.data));
        // FC=2 (user program) → instruction bank
        assert!(std::ptr::eq(atc.bank_for_fc(2), &atc.instruction));
        // FC=6 (supervisor program) → instruction bank
        assert!(std::ptr::eq(atc.bank_for_fc(6), &atc.instruction));
    }

    // --- Mmu construction ---

    #[test]
    fn mmu_construction_matches_model() {
        assert_eq!(Mmu::new(CpuModel::M68000).mode(), MmuMode::Disabled);
        assert_eq!(Mmu::new(CpuModel::M68030).mode(), MmuMode::M68030);
        assert_eq!(Mmu::new(CpuModel::M68040).mode(), MmuMode::M68040);
    }

    #[test]
    fn mmu_030_atc_round_trip() {
        let mut mmu = Mmu::new(CpuModel::M68030);
        mmu.atc_insert_030(AtcEntry {
            logical_page: 0x4000,
            physical_page: 0x8000,
            fc: 5,
            valid: true,
            ..AtcEntry::EMPTY
        });
        assert!(mmu.atc_lookup_030(0x4000, 5).is_some());
        mmu.flush_all();
        assert!(mmu.atc_lookup_030(0x4000, 5).is_none());
    }

    #[test]
    fn mmu_040_atc_round_trip() {
        let mut mmu = Mmu::new(CpuModel::M68040);
        mmu.atc_insert_040(
            AtcEntry {
                logical_page: 0x4000,
                physical_page: 0x8000,
                fc: 5,
                valid: true,
                ..AtcEntry::EMPTY
            },
            5,
            12,
        );
        assert!(mmu.atc_lookup_040(0x4000, 5, 12).is_some());
        // Instruction bank untouched
        assert!(mmu.atc_lookup_040(0x4000, 6, 12).is_none());
        mmu.flush_all();
        assert!(mmu.atc_lookup_040(0x4000, 5, 12).is_none());
    }

    // --- 68030 table walker ---

    /// Simple in-memory page table for 68030 walker tests.
    /// Uses a flat Vec as physical memory, read via closure.
    struct TestMemory {
        data: Vec<u8>,
    }

    impl TestMemory {
        fn new(size: usize) -> Self {
            Self {
                data: vec![0; size],
            }
        }

        fn write_long(&mut self, addr: u32, value: u32) {
            let a = addr as usize;
            self.data[a] = (value >> 24) as u8;
            self.data[a + 1] = (value >> 16) as u8;
            self.data[a + 2] = (value >> 8) as u8;
            self.data[a + 3] = value as u8;
        }

        fn read_long(&self, addr: u32) -> u32 {
            let a = addr as usize;
            (u32::from(self.data[a]) << 24)
                | (u32::from(self.data[a + 1]) << 16)
                | (u32::from(self.data[a + 2]) << 8)
                | u32::from(self.data[a + 3])
        }
    }

    #[test]
    fn walk030_single_level_identity() {
        // TC: enable, PS=12 (4KB), IS=12, TIA=8 → 12+8+12=32
        // Single-level table with 256 entries mapping identity.
        let tc = Tc030::parse((1u32 << 31) | (12 << 20) | (12 << 16) | (8 << 12));
        assert!(tc.enabled);
        assert_eq!(tc.tia, 8);
        assert_eq!(tc.num_levels(), 1);

        let mut mem = TestMemory::new(0x2_0000);
        let table_base = 0x1_0000u32;

        // Fill 256 page descriptors: identity mapping (DT=1 page).
        // Each descriptor: physical_page | 0x01 (DT=1).
        for i in 0..256u32 {
            let phys_page = i << 12;
            mem.write_long(table_base + i * 4, phys_page | 0x01);
        }

        // Root pointer: DT=2 (valid 4-byte pointer) to table_base.
        let root = u64::from(table_base | 0x02);

        // Walk address 0x0004_2100:
        // IS=12 skips top 12 bits, TIA index=0x42, offset=0x100
        // Entry 0x42 maps to physical 0x42000, so result = 0x42100.
        let result = walk_030(&tc, root, 0x0004_2100, 5, false, |a| mem.read_long(a));
        match result {
            WalkResult030::Ok {
                physical_addr,
                write_protect,
                ..
            } => {
                assert_eq!(physical_addr, 0x0004_2100);
                assert!(!write_protect);
            }
            WalkResult030::Fault { .. } => panic!("unexpected fault"),
        }
    }

    #[test]
    fn walk030_two_level_remapped() {
        // TC: enable, PS=12 (4KB), IS=8, TIA=4, TIB=8 → 8+4+8+12=32
        // Two-level: 16-entry root table, 256-entry second-level tables.
        let tc = Tc030::parse((1u32 << 31) | (12 << 20) | (8 << 16) | (4 << 12) | (8 << 8));
        assert_eq!(tc.tia, 4);
        assert_eq!(tc.tib, 8);
        assert_eq!(tc.num_levels(), 2);

        let mut mem = TestMemory::new(0x10_0000);

        let root_table = 0x0001_0000u32;
        let sub_table = 0x0002_0000u32;

        // Root table: 16 entries. Only entry 0 is valid, pointing to sub_table.
        mem.write_long(root_table, sub_table | 0x02); // DT=2

        // Sub table: 256 entries. Map page 5 → physical 0x8_5000.
        // Entry 5: page descriptor (DT=1) with physical base 0x8_5000.
        mem.write_long(sub_table + 5 * 4, 0x0008_5001); // phys=0x85000, DT=1

        let root = u64::from(root_table | 0x02);

        // Logical address 0x0000_5ABC → TIA index=0, TIB index=5, offset=0xABC.
        // Should map to physical 0x0008_5ABC.
        let result = walk_030(&tc, root, 0x0000_5ABC, 5, false, |a| mem.read_long(a));
        match result {
            WalkResult030::Ok { physical_addr, .. } => {
                assert_eq!(physical_addr, 0x0008_5ABC);
            }
            WalkResult030::Fault { .. } => panic!("unexpected fault"),
        }
    }

    #[test]
    fn walk030_invalid_descriptor_faults() {
        // TC: enable, PS=12, TIA=8
        let tc = Tc030::parse((1u32 << 31) | (12 << 20) | (8 << 12));

        let mut mem = TestMemory::new(0x2_0000);
        let table_base = 0x1_0000u32;
        // All entries are 0 (DT=0 = invalid).

        let root = u64::from(table_base | 0x02);
        let result = walk_030(&tc, root, 0x0000_1000, 5, false, |a| mem.read_long(a));
        match result {
            WalkResult030::Fault { level, .. } => {
                assert_eq!(level, 0); // First level descriptor is invalid
            }
            WalkResult030::Ok { .. } => panic!("expected fault"),
        }
    }

    #[test]
    fn walk030_write_protect_accumulates() {
        // TC: PS=12, IS=8, TIA=4, TIB=8 → 8+4+8+12=32
        let tc = Tc030::parse((1u32 << 31) | (12 << 20) | (8 << 16) | (4 << 12) | (8 << 8));

        let mut mem = TestMemory::new(0x10_0000);
        let root_table = 0x1_0000u32;
        let sub_table = 0x2_0000u32;

        // Root entry: DT=3 (8-byte pointer), table base = sub_table.
        // First long: address | DT=3.
        mem.write_long(root_table, sub_table | 0x03);
        // Second long: WP bit set (bit 2).
        mem.write_long(root_table + 4, 1 << 2);

        // Sub table entry 0: page descriptor (DT=1), no WP.
        mem.write_long(sub_table, 0x0005_0001);

        let root = u64::from(root_table | 0x02);
        let result = walk_030(&tc, root, 0x0000_0000, 5, false, |a| mem.read_long(a));
        match result {
            WalkResult030::Ok { write_protect, .. } => {
                assert!(write_protect, "WP should accumulate from 8-byte pointer level");
            }
            WalkResult030::Fault { .. } => panic!("unexpected fault"),
        }
    }

    #[test]
    fn walk030_root_pointer_selection() {
        let tc = Tc030 {
            enabled: true,
            sre: true,
            fcl: false,
            page_shift: 12,
            initial_shift: 0,
            tia: 8,
            tib: 0,
            tic: 0,
            tid: 0,
        };

        // SRP (supervisor)
        let srp = 0x1000_0000u32;
        let srp_upper = 0;
        let urp = 0x2000_0000u32;
        let crp_upper = 0;

        // FC=5 (supervisor data) → SRP
        let root = select_root_pointer_030(&tc, 5, srp, srp_upper, urp, crp_upper);
        assert_eq!(root as u32, srp);

        // FC=1 (user data) → CRP
        let root = select_root_pointer_030(&tc, 1, srp, srp_upper, urp, crp_upper);
        assert_eq!(root as u32, urp);

        // SRE=false → always CRP
        let tc_no_sre = Tc030 { sre: false, ..tc };
        let root = select_root_pointer_030(&tc_no_sre, 5, srp, srp_upper, urp, crp_upper);
        assert_eq!(root as u32, urp);
    }

    // --- 68040 table walker ---

    #[test]
    fn walk040_identity_4kb() {
        let tc = Tc040::parse(1 << 15); // Enable, 4KB

        let mut mem = TestMemory::new(0x40_0000);
        let root_table = 0x10_0000u32;
        let pointer_table = 0x20_0000u32;
        let page_table = 0x30_0000u32;

        // Logical address 0x0000_1234:
        // Root index: bits 31-25 = 0
        // Pointer index: bits 24-18 = 0
        // Page index: bits 17-12 = 1 (page 0x1000)
        // Offset: bits 11-0 = 0x234

        // Root entry 0: pointer to pointer_table (UDT=2).
        mem.write_long(root_table, pointer_table | 0x02);
        // Pointer entry 0: pointer to page_table (UDT=2).
        mem.write_long(pointer_table, page_table | 0x02);
        // Page entry 1 (for page 0x1000): resident page (PDT=1), phys = 0x0000_1000.
        mem.write_long(page_table + 1 * 4, 0x0000_1001);

        let result = walk_040(&tc, root_table, 0x0000_1234, 5, false, |a| {
            mem.read_long(a)
        });
        match result {
            WalkResult040::Ok { physical_addr, .. } => {
                assert_eq!(physical_addr, 0x0000_1234);
            }
            WalkResult040::Fault { .. } => panic!("unexpected fault"),
        }
    }

    #[test]
    fn walk040_remapped() {
        let tc = Tc040::parse(1 << 15); // Enable, 4KB

        let mut mem = TestMemory::new(0x40_0000);
        let root_table = 0x10_0000u32;
        let pointer_table = 0x20_0000u32;
        let page_table = 0x30_0000u32;

        // Map logical 0x0000_1000 → physical 0x0008_0000.
        mem.write_long(root_table, pointer_table | 0x02);
        mem.write_long(pointer_table, page_table | 0x02);
        // Page entry 1: physical 0x0008_0000, PDT=1.
        mem.write_long(page_table + 1 * 4, 0x0008_0001);

        let result = walk_040(&tc, root_table, 0x0000_1ABC, 5, false, |a| {
            mem.read_long(a)
        });
        match result {
            WalkResult040::Ok { physical_addr, .. } => {
                assert_eq!(physical_addr, 0x0008_0ABC);
            }
            WalkResult040::Fault { .. } => panic!("unexpected fault"),
        }
    }

    #[test]
    fn walk040_invalid_root_faults() {
        let tc = Tc040::parse(1 << 15);
        let mem = TestMemory::new(0x10_0000);
        let root_table = 0x0u32;
        // Root entry 0 is all zeros (UDT=0 invalid).
        let result = walk_040(&tc, root_table, 0, 5, false, |a| mem.read_long(a));
        assert!(matches!(result, WalkResult040::Fault { .. }));
    }

    #[test]
    fn walk040_write_protect() {
        let tc = Tc040::parse(1 << 15); // 4KB
        let mut mem = TestMemory::new(0x40_0000);
        let root_table = 0x10_0000u32;
        let pointer_table = 0x20_0000u32;
        let page_table = 0x30_0000u32;

        mem.write_long(root_table, pointer_table | 0x02);
        mem.write_long(pointer_table, page_table | 0x02);
        // Page entry 0: phys 0x0000_0000, PDT=1, W=1 (bit 2).
        mem.write_long(page_table, 0x0000_0001 | (1 << 2));

        // Read should succeed.
        let result = walk_040(&tc, root_table, 0x0000_0000, 5, false, |a| {
            mem.read_long(a)
        });
        assert!(matches!(result, WalkResult040::Ok { write_protect: true, .. }));

        // Write should fault.
        let result = walk_040(&tc, root_table, 0x0000_0000, 5, true, |a| {
            mem.read_long(a)
        });
        assert!(matches!(result, WalkResult040::Fault { .. }));
    }

    #[test]
    fn walk040_global_bit() {
        let tc = Tc040::parse(1 << 15);
        let mut mem = TestMemory::new(0x40_0000);
        let root_table = 0x10_0000u32;
        let pointer_table = 0x20_0000u32;
        let page_table = 0x30_0000u32;

        mem.write_long(root_table, pointer_table | 0x02);
        mem.write_long(pointer_table, page_table | 0x02);
        // Page entry 0: phys 0, PDT=1, G=1 (bit 10).
        mem.write_long(page_table, 0x0000_0001 | (1 << 10));

        let result = walk_040(&tc, root_table, 0, 5, false, |a| mem.read_long(a));
        match result {
            WalkResult040::Ok { global, .. } => assert!(global),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn walk040_root_pointer_selection() {
        let srp = 0x1000_0000u32;
        let urp = 0x2000_0000u32;

        assert_eq!(select_root_pointer_040(5, srp, urp), srp); // supervisor data
        assert_eq!(select_root_pointer_040(6, srp, urp), srp); // supervisor program
        assert_eq!(select_root_pointer_040(1, srp, urp), urp); // user data
        assert_eq!(select_root_pointer_040(2, srp, urp), urp); // user program
    }
}
