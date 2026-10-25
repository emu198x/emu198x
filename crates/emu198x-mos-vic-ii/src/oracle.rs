//! VIC-II per-cycle oracle — the canonical PAL 6569 and NTSC 6567R8 access / BA
//! schedules, encoded as data, plus a comparator for proving the engine's
//! per-cycle behaviour during the VC/VCBASE/RC rewrite and the NTSC port.
//!
//! This is **reference data and a verification aid**, not part of the render
//! path. It exists so the rewrite of the addressing layer (see
//! `docs/plans/2026-06-30-c64-vic-ii-vc-vcbase-rc-rewrite.md`) is provable
//! against a documented schedule rather than vibes.
//!
//! # Sources
//!
//! - **VICE `cycle_tab_pal[]` / `cycle_tab_ntsc[]`** —
//!   `emulators/c64/vice-3.10/src/viciisc/vicii-chip-model.c:111-403`. The
//!   canonical per-phase (Phi1/Phi2) schedules that VirtualC64, Frodo and
//!   Hoxs64 all validate against.
//! - **Repo reference distillation** —
//!   `reference/by-topic/vic-ii/vic-ii-reference.md:436-540` (badline cycle
//!   effect + the cycle-by-cycle PAL table). Agrees with VICE.
//!
//! # Cycle numbering
//!
//! Entries are **1-based** (PAL `cycle` 1..=63, NTSC 1..=65), matching VICE and
//! Bauer. The engine's `raster_cycle` is **0-based** but reuses VICE's literal
//! numbers, treating its cycle 0 as the line's final canonical cycle (63 PAL /
//! 65 NTSC). The c-access/g-access region (cycles 12-55) and its counter
//! events are identical across models; NTSC's two extra cycles fall in the
//! sprite region.

/// One VIC-II memory-access kind on a single bus phase.
///
/// The VIC-II performs at most one access per bus phase (Phi1 or Phi2). On a
/// stock C64 the CPU owns Phi2 except during badline c-accesses and sprite
/// DMA, so [`AccessKind::None`] marks a phase the VIC leaves to the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    /// VIC performs no access this phase; the CPU has the bus.
    None,
    /// Idle bus cycle (VIC drives the bus but fetches nothing useful —
    /// reads `$3FFF`, the idle address).
    Idle,
    /// DRAM refresh (r-access). Five per line, cycles 11-15 (Phi1).
    Refresh,
    /// c-access: video-matrix (screen code + colour) read. 40 per badline,
    /// cycles 15-54 (Phi2).
    FetchC,
    /// g-access: character/bitmap graphics read. 40 per displayed row,
    /// cycles 16-55 (Phi1).
    FetchG,
    /// p-access: sprite data pointer read for the given sprite (0-7).
    SpritePtr(u8),
    /// s-access: sprite data byte `k` (0-2) for the given sprite (0-7).
    SpriteData(u8, u8),
}

// Cycle flags — mirror VICE's `vicii-chip-model.c` flag #defines. Carried so
// later increments can drive the VC/VCBASE/RC chain off the documented update
// cycles rather than re-deriving them.
/// `UpdateMcBase` — sprite MCBASE update (Phi2 cycle 16).
pub const UPDATE_MC_BASE: u16 = 0x001;
/// `ChkSprExp` — sprite Y-expansion flip-flop check (Phi2 cycle 56).
pub const CHK_SPR_EXP: u16 = 0x002;
/// `ChkSprDma` — sprite DMA on/off check (cycles 55 and 56).
pub const CHK_SPR_DMA: u16 = 0x004;
/// `ChkSprDisp` — sprite display on/off check (Phi1 cycle 58).
pub const CHK_SPR_DISP: u16 = 0x008;
/// `ChkSprCrunch` — sprite-crunch check (Phi2 cycle 15).
pub const CHK_SPR_CRUNCH: u16 = 0x010;
/// `ChkBrdL1` — left-border comparison 1 (Phi2 cycle 17).
pub const CHK_BRD_L1: u16 = 0x020;
/// `ChkBrdL0` — left-border comparison 0 (Phi2 cycle 18).
pub const CHK_BRD_L0: u16 = 0x040;
/// `ChkBrdR0` — right-border comparison 0 (Phi2 cycle 56).
pub const CHK_BRD_R0: u16 = 0x080;
/// `ChkBrdR1` — right-border comparison 1 (Phi2 cycle 57).
pub const CHK_BRD_R1: u16 = 0x100;
/// `UpdateVc` — VC ← VCBASE / VMLI ← 0 (Phi2 cycle 14).
pub const UPDATE_VC: u16 = 0x200;
/// `UpdateRc` — RC increment / VCBASE ← VC at row end (Phi2 cycle 58).
pub const UPDATE_RC: u16 = 0x400;

/// One PAL raster cycle's canonical schedule, collapsing the two bus phases.
#[derive(Debug, Clone, Copy)]
pub struct CanonicalCycle {
    /// 1-based PAL cycle number (1..=63).
    pub cycle: u8,
    /// Access on the Phi1 (first) bus phase.
    pub phi1: AccessKind,
    /// Access on the Phi2 (second) bus phase.
    pub phi2: AccessKind,
    /// True when BA is held low this cycle for a badline c-access fetch
    /// (VICE `BaFetch`). Only effective when the line is actually a badline.
    pub ba_fetch: bool,
    /// Bitmask of sprites (bit `n` = sprite `n`) whose DMA holds BA low this
    /// cycle (VICE `BaSpr*`). Effective only for sprites with active DMA.
    pub ba_sprites: u8,
    /// `Update*` / `Chk*` flags firing this cycle (OR of both phases).
    pub flags: u16,
}

const fn cyc(
    cycle: u8,
    phi1: AccessKind,
    phi2: AccessKind,
    ba_fetch: bool,
    ba_sprites: u8,
    flags: u16,
) -> CanonicalCycle {
    CanonicalCycle {
        cycle,
        phi1,
        phi2,
        ba_fetch,
        ba_sprites,
        flags,
    }
}

use AccessKind::{FetchC, FetchG, Idle, None as NoAcc, Refresh, SpriteData as SD, SpritePtr as SP};

/// The canonical PAL (6569) per-cycle schedule, cycles 1..=63.
///
/// Transcribed from VICE `cycle_tab_pal[]`
/// (`vicii-chip-model.c:111-238`); the `ba_sprites` masks are the VICE
/// `BaSpr1/2/3` values, the `flags` are the OR of each cycle's Phi1+Phi2
/// flags.
pub const CANONICAL_PAL: [CanonicalCycle; 63] = [
    // Sprite DMA wrap-around for sprites 3-7 (from the previous line's view).
    cyc(1, SP(3), SD(3, 0), false, 0b0001_1000, 0),
    cyc(2, SD(3, 1), SD(3, 2), false, 0b0011_1000, 0),
    cyc(3, SP(4), SD(4, 0), false, 0b0011_0000, 0),
    cyc(4, SD(4, 1), SD(4, 2), false, 0b0111_0000, 0),
    cyc(5, SP(5), SD(5, 0), false, 0b0110_0000, 0),
    cyc(6, SD(5, 1), SD(5, 2), false, 0b1110_0000, 0),
    cyc(7, SP(6), SD(6, 0), false, 0b1100_0000, 0),
    cyc(8, SD(6, 1), SD(6, 2), false, 0b1100_0000, 0),
    cyc(9, SP(7), SD(7, 0), false, 0b1000_0000, 0),
    cyc(10, SD(7, 1), SD(7, 2), false, 0b1000_0000, 0),
    // DRAM refresh; BA drops at cycle 12 for badlines (3-cycle lead-in).
    cyc(11, Refresh, NoAcc, false, 0, 0),
    cyc(12, Refresh, NoAcc, true, 0, 0),
    cyc(13, Refresh, NoAcc, true, 0, 0),
    cyc(14, Refresh, NoAcc, true, 0, UPDATE_VC),
    cyc(15, Refresh, FetchC, true, 0, CHK_SPR_CRUNCH),
    // 40 c-accesses (Phi2, cycles 15-54) + 40 g-accesses (Phi1, cycles 16-55).
    cyc(16, FetchG, FetchC, true, 0, UPDATE_MC_BASE),
    cyc(17, FetchG, FetchC, true, 0, CHK_BRD_L1),
    cyc(18, FetchG, FetchC, true, 0, CHK_BRD_L0),
    cyc(19, FetchG, FetchC, true, 0, 0),
    cyc(20, FetchG, FetchC, true, 0, 0),
    cyc(21, FetchG, FetchC, true, 0, 0),
    cyc(22, FetchG, FetchC, true, 0, 0),
    cyc(23, FetchG, FetchC, true, 0, 0),
    cyc(24, FetchG, FetchC, true, 0, 0),
    cyc(25, FetchG, FetchC, true, 0, 0),
    cyc(26, FetchG, FetchC, true, 0, 0),
    cyc(27, FetchG, FetchC, true, 0, 0),
    cyc(28, FetchG, FetchC, true, 0, 0),
    cyc(29, FetchG, FetchC, true, 0, 0),
    cyc(30, FetchG, FetchC, true, 0, 0),
    cyc(31, FetchG, FetchC, true, 0, 0),
    cyc(32, FetchG, FetchC, true, 0, 0),
    cyc(33, FetchG, FetchC, true, 0, 0),
    cyc(34, FetchG, FetchC, true, 0, 0),
    cyc(35, FetchG, FetchC, true, 0, 0),
    cyc(36, FetchG, FetchC, true, 0, 0),
    cyc(37, FetchG, FetchC, true, 0, 0),
    cyc(38, FetchG, FetchC, true, 0, 0),
    cyc(39, FetchG, FetchC, true, 0, 0),
    cyc(40, FetchG, FetchC, true, 0, 0),
    cyc(41, FetchG, FetchC, true, 0, 0),
    cyc(42, FetchG, FetchC, true, 0, 0),
    cyc(43, FetchG, FetchC, true, 0, 0),
    cyc(44, FetchG, FetchC, true, 0, 0),
    cyc(45, FetchG, FetchC, true, 0, 0),
    cyc(46, FetchG, FetchC, true, 0, 0),
    cyc(47, FetchG, FetchC, true, 0, 0),
    cyc(48, FetchG, FetchC, true, 0, 0),
    cyc(49, FetchG, FetchC, true, 0, 0),
    cyc(50, FetchG, FetchC, true, 0, 0),
    cyc(51, FetchG, FetchC, true, 0, 0),
    cyc(52, FetchG, FetchC, true, 0, 0),
    cyc(53, FetchG, FetchC, true, 0, 0),
    cyc(54, FetchG, FetchC, true, 0, 0),
    // 40th g-access; BA released from the fetch window, sprite 0 BA lead-in.
    cyc(55, FetchG, NoAcc, false, 0b0000_0001, CHK_SPR_DMA),
    cyc(
        56,
        Idle,
        NoAcc,
        false,
        0b0000_0001,
        CHK_SPR_DMA | CHK_BRD_R0 | CHK_SPR_EXP,
    ),
    cyc(57, Idle, NoAcc, false, 0b0000_0011, CHK_BRD_R1),
    // Sprite DMA for sprites 0-2 (this line, feeding the next).
    cyc(
        58,
        SP(0),
        SD(0, 0),
        false,
        0b0000_0011,
        CHK_SPR_DISP | UPDATE_RC,
    ),
    cyc(59, SD(0, 1), SD(0, 2), false, 0b0000_0111, 0),
    cyc(60, SP(1), SD(1, 0), false, 0b0000_0110, 0),
    cyc(61, SD(1, 1), SD(1, 2), false, 0b0000_1110, 0),
    cyc(62, SP(2), SD(2, 0), false, 0b0000_1100, 0),
    cyc(63, SD(2, 1), SD(2, 2), false, 0b0001_1100, 0),
];

/// The canonical NTSC 6567R8 per-cycle schedule, cycles 1..=65.
///
/// Transcribed from VICE `cycle_tab_ntsc[]` (`vicii-chip-model.c:272-403`).
/// The c-access/g-access region (cycles 12-55) and its counter flags are
/// identical to PAL; NTSC's two extra cycles fall in the sprite region, so
/// sprites 0-3 p-access in the previous line's tail (59/61/63/65) and the
/// DMA/display checks shift one cycle later than PAL.
pub const CANONICAL_NTSC: [CanonicalCycle; 65] = [
    // Sprite DMA wrap-around for sprites 3-7 (from the previous line's view).
    cyc(1, SD(3, 1), SD(3, 2), false, 0b0011_1000, 0),
    cyc(2, SP(4), SD(4, 0), false, 0b0011_0000, 0),
    cyc(3, SD(4, 1), SD(4, 2), false, 0b0111_0000, 0),
    cyc(4, SP(5), SD(5, 0), false, 0b0110_0000, 0),
    cyc(5, SD(5, 1), SD(5, 2), false, 0b1110_0000, 0),
    cyc(6, SP(6), SD(6, 0), false, 0b1100_0000, 0),
    cyc(7, SD(6, 1), SD(6, 2), false, 0b1100_0000, 0),
    cyc(8, SP(7), SD(7, 0), false, 0b1000_0000, 0),
    cyc(9, SD(7, 1), SD(7, 2), false, 0b1000_0000, 0),
    cyc(10, Idle, NoAcc, false, 0, 0),
    // DRAM refresh; BA drops at cycle 12 for badlines (3-cycle lead-in).
    cyc(11, Refresh, NoAcc, false, 0, 0),
    cyc(12, Refresh, NoAcc, true, 0, 0),
    cyc(13, Refresh, NoAcc, true, 0, 0),
    cyc(14, Refresh, NoAcc, true, 0, UPDATE_VC),
    cyc(15, Refresh, FetchC, true, 0, CHK_SPR_CRUNCH),
    // 40 c-accesses (Phi2, cycles 15-54) + 40 g-accesses (Phi1, cycles 16-55).
    cyc(16, FetchG, FetchC, true, 0, UPDATE_MC_BASE),
    cyc(17, FetchG, FetchC, true, 0, CHK_BRD_L1),
    cyc(18, FetchG, FetchC, true, 0, CHK_BRD_L0),
    cyc(19, FetchG, FetchC, true, 0, 0),
    cyc(20, FetchG, FetchC, true, 0, 0),
    cyc(21, FetchG, FetchC, true, 0, 0),
    cyc(22, FetchG, FetchC, true, 0, 0),
    cyc(23, FetchG, FetchC, true, 0, 0),
    cyc(24, FetchG, FetchC, true, 0, 0),
    cyc(25, FetchG, FetchC, true, 0, 0),
    cyc(26, FetchG, FetchC, true, 0, 0),
    cyc(27, FetchG, FetchC, true, 0, 0),
    cyc(28, FetchG, FetchC, true, 0, 0),
    cyc(29, FetchG, FetchC, true, 0, 0),
    cyc(30, FetchG, FetchC, true, 0, 0),
    cyc(31, FetchG, FetchC, true, 0, 0),
    cyc(32, FetchG, FetchC, true, 0, 0),
    cyc(33, FetchG, FetchC, true, 0, 0),
    cyc(34, FetchG, FetchC, true, 0, 0),
    cyc(35, FetchG, FetchC, true, 0, 0),
    cyc(36, FetchG, FetchC, true, 0, 0),
    cyc(37, FetchG, FetchC, true, 0, 0),
    cyc(38, FetchG, FetchC, true, 0, 0),
    cyc(39, FetchG, FetchC, true, 0, 0),
    cyc(40, FetchG, FetchC, true, 0, 0),
    cyc(41, FetchG, FetchC, true, 0, 0),
    cyc(42, FetchG, FetchC, true, 0, 0),
    cyc(43, FetchG, FetchC, true, 0, 0),
    cyc(44, FetchG, FetchC, true, 0, 0),
    cyc(45, FetchG, FetchC, true, 0, 0),
    cyc(46, FetchG, FetchC, true, 0, 0),
    cyc(47, FetchG, FetchC, true, 0, 0),
    cyc(48, FetchG, FetchC, true, 0, 0),
    cyc(49, FetchG, FetchC, true, 0, 0),
    cyc(50, FetchG, FetchC, true, 0, 0),
    cyc(51, FetchG, FetchC, true, 0, 0),
    cyc(52, FetchG, FetchC, true, 0, 0),
    cyc(53, FetchG, FetchC, true, 0, 0),
    cyc(54, FetchG, FetchC, true, 0, 0),
    // 40th g-access; then the sprite region (2 cycles later than PAL).
    cyc(55, FetchG, NoAcc, false, 0, 0),
    cyc(
        56,
        Idle,
        NoAcc,
        false,
        0b0000_0001,
        CHK_SPR_DMA | CHK_BRD_R0 | CHK_SPR_EXP,
    ),
    cyc(
        57,
        Idle,
        NoAcc,
        false,
        0b0000_0001,
        CHK_SPR_DMA | CHK_BRD_R1,
    ),
    cyc(58, Idle, NoAcc, false, 0b0000_0011, UPDATE_RC),
    // Sprite DMA for sprites 0-3 (this line, feeding the next).
    cyc(59, SP(0), SD(0, 0), false, 0b0000_0011, CHK_SPR_DISP),
    cyc(60, SD(0, 1), SD(0, 2), false, 0b0000_0111, 0),
    cyc(61, SP(1), SD(1, 0), false, 0b0000_0110, 0),
    cyc(62, SD(1, 1), SD(1, 2), false, 0b0000_1110, 0),
    cyc(63, SP(2), SD(2, 0), false, 0b0000_1100, 0),
    cyc(64, SD(2, 1), SD(2, 2), false, 0b0001_1100, 0),
    cyc(65, SP(3), SD(3, 0), false, 0b0001_1000, 0),
];

/// The canonical NTSC 6567R56A (early NTSC) per-cycle schedule, cycles 1..=64.
///
/// Transcribed from VICE `cycle_tab_ntsc_old[]` (`vicii-chip-model.c:437-566`).
/// Between PAL and R8: the sprite wrap-around (cycles 1-10) and c/g-access
/// region are identical to PAL, but the tail sprites 0-2 sit one cycle later
/// (59/61/63 vs PAL 58/60/62), the DMA check is at 56/57 (like R8), and the
/// display latch stays at 58 (like PAL).
pub const CANONICAL_NTSC_OLD: [CanonicalCycle; 64] = [
    // Sprite DMA wrap-around for sprites 3-7 (identical to PAL cycles 1-10).
    cyc(1, SP(3), SD(3, 0), false, 0b0001_1000, 0),
    cyc(2, SD(3, 1), SD(3, 2), false, 0b0011_1000, 0),
    cyc(3, SP(4), SD(4, 0), false, 0b0011_0000, 0),
    cyc(4, SD(4, 1), SD(4, 2), false, 0b0111_0000, 0),
    cyc(5, SP(5), SD(5, 0), false, 0b0110_0000, 0),
    cyc(6, SD(5, 1), SD(5, 2), false, 0b1110_0000, 0),
    cyc(7, SP(6), SD(6, 0), false, 0b1100_0000, 0),
    cyc(8, SD(6, 1), SD(6, 2), false, 0b1100_0000, 0),
    cyc(9, SP(7), SD(7, 0), false, 0b1000_0000, 0),
    cyc(10, SD(7, 1), SD(7, 2), false, 0b1000_0000, 0),
    // DRAM refresh; BA drops at cycle 12 for badlines (3-cycle lead-in).
    cyc(11, Refresh, NoAcc, false, 0, 0),
    cyc(12, Refresh, NoAcc, true, 0, 0),
    cyc(13, Refresh, NoAcc, true, 0, 0),
    cyc(14, Refresh, NoAcc, true, 0, UPDATE_VC),
    cyc(15, Refresh, FetchC, true, 0, CHK_SPR_CRUNCH),
    // 40 c-accesses (Phi2, cycles 15-54) + 40 g-accesses (Phi1, cycles 16-55).
    cyc(16, FetchG, FetchC, true, 0, UPDATE_MC_BASE),
    cyc(17, FetchG, FetchC, true, 0, CHK_BRD_L1),
    cyc(18, FetchG, FetchC, true, 0, CHK_BRD_L0),
    cyc(19, FetchG, FetchC, true, 0, 0),
    cyc(20, FetchG, FetchC, true, 0, 0),
    cyc(21, FetchG, FetchC, true, 0, 0),
    cyc(22, FetchG, FetchC, true, 0, 0),
    cyc(23, FetchG, FetchC, true, 0, 0),
    cyc(24, FetchG, FetchC, true, 0, 0),
    cyc(25, FetchG, FetchC, true, 0, 0),
    cyc(26, FetchG, FetchC, true, 0, 0),
    cyc(27, FetchG, FetchC, true, 0, 0),
    cyc(28, FetchG, FetchC, true, 0, 0),
    cyc(29, FetchG, FetchC, true, 0, 0),
    cyc(30, FetchG, FetchC, true, 0, 0),
    cyc(31, FetchG, FetchC, true, 0, 0),
    cyc(32, FetchG, FetchC, true, 0, 0),
    cyc(33, FetchG, FetchC, true, 0, 0),
    cyc(34, FetchG, FetchC, true, 0, 0),
    cyc(35, FetchG, FetchC, true, 0, 0),
    cyc(36, FetchG, FetchC, true, 0, 0),
    cyc(37, FetchG, FetchC, true, 0, 0),
    cyc(38, FetchG, FetchC, true, 0, 0),
    cyc(39, FetchG, FetchC, true, 0, 0),
    cyc(40, FetchG, FetchC, true, 0, 0),
    cyc(41, FetchG, FetchC, true, 0, 0),
    cyc(42, FetchG, FetchC, true, 0, 0),
    cyc(43, FetchG, FetchC, true, 0, 0),
    cyc(44, FetchG, FetchC, true, 0, 0),
    cyc(45, FetchG, FetchC, true, 0, 0),
    cyc(46, FetchG, FetchC, true, 0, 0),
    cyc(47, FetchG, FetchC, true, 0, 0),
    cyc(48, FetchG, FetchC, true, 0, 0),
    cyc(49, FetchG, FetchC, true, 0, 0),
    cyc(50, FetchG, FetchC, true, 0, 0),
    cyc(51, FetchG, FetchC, true, 0, 0),
    cyc(52, FetchG, FetchC, true, 0, 0),
    cyc(53, FetchG, FetchC, true, 0, 0),
    cyc(54, FetchG, FetchC, true, 0, 0),
    // 40th g-access; then the sprite region (one cycle later than PAL).
    cyc(55, FetchG, NoAcc, false, 0, 0),
    cyc(
        56,
        Idle,
        NoAcc,
        false,
        0b0000_0001,
        CHK_SPR_DMA | CHK_BRD_R0 | CHK_SPR_EXP,
    ),
    cyc(
        57,
        Idle,
        NoAcc,
        false,
        0b0000_0001,
        CHK_SPR_DMA | CHK_BRD_R1,
    ),
    cyc(
        58,
        Idle,
        NoAcc,
        false,
        0b0000_0011,
        CHK_SPR_DISP | UPDATE_RC,
    ),
    // Sprite DMA for sprites 0-2 (this line, feeding the next).
    cyc(59, SP(0), SD(0, 0), false, 0b0000_0011, 0),
    cyc(60, SD(0, 1), SD(0, 2), false, 0b0000_0111, 0),
    cyc(61, SP(1), SD(1, 0), false, 0b0000_0110, 0),
    cyc(62, SD(1, 1), SD(1, 2), false, 0b0000_1110, 0),
    cyc(63, SP(2), SD(2, 0), false, 0b0000_1100, 0),
    cyc(64, SD(2, 1), SD(2, 2), false, 0b0001_1100, 0),
];

/// Map the engine's 0-based `raster_cycle` to the canonical 1-based cycle.
///
/// The engine counts 0..=62 and reuses VICE's literal cycle numbers, treating
/// its cycle 0 as the line's final canonical cycle (63) — see
/// `is_sprite_dma_stealing`, which pairs sprite 2's second slot with engine
/// cycle 0. So engine cycle `n` is canonical `n` for 1..=62, and engine cycle
/// 0 is canonical 63.
#[must_use]
pub fn engine_to_canonical(engine_cycle: u8) -> u8 {
    if engine_cycle == 0 { 63 } else { engine_cycle }
}

/// The canonical schedule entry for an engine `raster_cycle`.
#[must_use]
pub fn canonical_for_engine_cycle(engine_cycle: u8) -> &'static CanonicalCycle {
    let canonical = engine_to_canonical(engine_cycle);
    &CANONICAL_PAL[(canonical - 1) as usize]
}

/// Whether BA should be held low this canonical cycle, given the line's
/// badline status and the set of sprites with active DMA.
///
/// BA is low when either the badline fetch window is active (`ba_fetch` on a
/// badline) or any DMA-active sprite's lead-in mask covers this cycle.
#[must_use]
pub fn expected_ba_low(entry: &CanonicalCycle, badline: bool, active_sprites: u8) -> bool {
    (entry.ba_fetch && badline) || (entry.ba_sprites & active_sprites != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The table must transcribe VICE faithfully. These self-checks lock the
    // structural totals so a typo in the 63-entry literal fails loudly.

    fn count_phase(table: &[CanonicalCycle], pred: impl Fn(AccessKind) -> bool) -> usize {
        table
            .iter()
            .flat_map(|c| [c.phi1, c.phi2])
            .filter(|&a| pred(a))
            .count()
    }

    /// Structural self-checks shared by both models: 40 c/g-accesses, 5 refresh,
    /// one p-access + three data bytes per sprite, the badline window, and the
    /// counter-update flag cycles.
    fn assert_table_well_formed(table: &[CanonicalCycle], ba_window: std::ops::RangeInclusive<u8>) {
        assert_eq!(count_phase(table, |a| a == FetchC), 40, "40 c-accesses");
        assert_eq!(count_phase(table, |a| a == FetchG), 40, "40 g-accesses");
        assert_eq!(count_phase(table, |a| a == Refresh), 5, "5 refresh");
        for n in 0u8..8 {
            assert_eq!(
                count_phase(table, |a| a == SP(n)),
                1,
                "sprite {n} should have exactly one p-access"
            );
            for k in 0u8..3 {
                assert_eq!(
                    count_phase(table, |a| a == SD(n, k)),
                    1,
                    "sprite {n} byte {k} should have exactly one s-access"
                );
            }
        }
        for entry in table {
            let expected = ba_window.contains(&entry.cycle);
            assert_eq!(
                entry.ba_fetch, expected,
                "cycle {} ba_fetch should be {expected}",
                entry.cycle
            );
        }
        let flag_cycle = |flag: u16| table.iter().find(|c| c.flags & flag != 0).map(|c| c.cycle);
        assert_eq!(flag_cycle(UPDATE_VC), Some(14));
        assert_eq!(flag_cycle(UPDATE_RC), Some(58));
        assert_eq!(flag_cycle(UPDATE_MC_BASE), Some(16));
    }

    #[test]
    fn pal_table_well_formed() {
        assert_table_well_formed(&CANONICAL_PAL, 12..=54);
    }

    #[test]
    fn ntsc_table_well_formed() {
        assert_table_well_formed(&CANONICAL_NTSC, 12..=54);
    }

    #[test]
    fn ntsc_old_table_well_formed() {
        assert_table_well_formed(&CANONICAL_NTSC_OLD, 12..=54);
    }

    #[test]
    fn ntsc_old_sits_between_pal_and_r8() {
        // R56A: DMA check at 56 (like R8), display latch at 58 (like PAL);
        // sprite 0's p-access at 59 (PAL 58, R8 59) but sprite 3 stays on the
        // current line at cycle 1 (like PAL, unlike R8's wrap to 65).
        let first = |flag: u16| {
            CANONICAL_NTSC_OLD
                .iter()
                .find(|c| c.flags & flag != 0)
                .map(|c| c.cycle)
        };
        assert_eq!(first(CHK_SPR_DMA), Some(56));
        assert_eq!(first(CHK_SPR_DISP), Some(58));
        let ptr_cycle = |n: u8| {
            CANONICAL_NTSC_OLD
                .iter()
                .find(|c| c.phi1 == SP(n))
                .map(|c| c.cycle)
        };
        assert_eq!(ptr_cycle(0), Some(59));
        assert_eq!(ptr_cycle(3), Some(1));
    }

    #[test]
    fn ntsc_sprite_dma_and_display_shift_one_cycle_later() {
        // NTSC's two extra cycles push the sprite checks later than PAL: the
        // display latch is at 59 (PAL 58) and the first DMA check at 56 (PAL 55).
        let first = |flag: u16| {
            CANONICAL_NTSC
                .iter()
                .find(|c| c.flags & flag != 0)
                .map(|c| c.cycle)
        };
        assert_eq!(first(CHK_SPR_DISP), Some(59));
        assert_eq!(first(CHK_SPR_DMA), Some(56));
        // Sprite 0's p-access is at cycle 59 (PAL 58).
        assert_eq!(
            CANONICAL_NTSC
                .iter()
                .find(|c| c.phi1 == SP(0))
                .map(|c| c.cycle),
            Some(59)
        );
    }

    #[test]
    fn engine_cycle_zero_maps_to_canonical_63() {
        assert_eq!(engine_to_canonical(0), 63);
        assert_eq!(engine_to_canonical(15), 15);
        assert_eq!(engine_to_canonical(62), 62);
        assert_eq!(canonical_for_engine_cycle(0).cycle, 63);
    }

    #[test]
    fn badline_fetch_ba_only_when_badline() {
        let entry = &CANONICAL_PAL[14]; // cycle 15, ba_fetch = true
        assert!(expected_ba_low(entry, true, 0));
        assert!(!expected_ba_low(entry, false, 0));
    }

    #[test]
    fn sprite0_ba_lead_in_covers_cycles_55_to_59() {
        for entry in &CANONICAL_PAL {
            let expected = (55..=59).contains(&entry.cycle);
            assert_eq!(
                entry.ba_sprites & 0b0000_0001 != 0,
                expected,
                "cycle {} sprite-0 BA mask should be {expected}",
                entry.cycle
            );
        }
    }
}
