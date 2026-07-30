//! Agnus - Beam counter and DMA slot allocation.

use serde::{Deserialize, Serialize};

/// Named bit masks for the DMACON register (HRM Appendix A.3 +
/// Chapter 6). These describe DMA-channel enables; Paula reads the
/// same bits for its own slot gating.
pub mod bits {
    /// DMACON write flag: 1 = SET bits in val[14..0], 0 = CLEAR.
    pub const DMACON_SETCLR: u16 = 0x8000;
    pub const DMACON_BLTPRI: u16 = 0x0400; // blitter bus priority
    pub const DMACON_DMAEN: u16 = 0x0200; // master enable
    pub const DMACON_BPLEN: u16 = 0x0100; // bitplane DMA
    pub const DMACON_COPEN: u16 = 0x0080; // copper DMA
    pub const DMACON_BLTEN: u16 = 0x0040; // blitter DMA
    pub const DMACON_SPREN: u16 = 0x0020; // sprite DMA
    pub const DMACON_DSKEN: u16 = 0x0010; // disk DMA
    pub const DMACON_AUD3EN: u16 = 0x0008;
    pub const DMACON_AUD2EN: u16 = 0x0004;
    pub const DMACON_AUD1EN: u16 = 0x0002;
    pub const DMACON_AUD0EN: u16 = 0x0001;
    /// Per-channel audio DMA enable masks, indexed 0..=3.
    pub const DMACON_AUD: [u16; 4] = [DMACON_AUD0EN, DMACON_AUD1EN, DMACON_AUD2EN, DMACON_AUD3EN];
    /// DMACON bits Agnus stores (excludes SETCLR write flag).
    pub const DMACON_MASK: u16 = 0x07FF;

    /// BPLCON0 bits Agnus cares about (rest are Denise-owned).
    pub const BPLCON0_HIRES: u16 = 0x8000;
    pub const BPLCON0_UHRES: u16 = 0x0080;
    pub const BPLCON0_SHRES: u16 = 0x0040;
    pub const BPLCON0_LACE: u16 = 0x0004;
    /// BPU (number of bitplanes) field — 3 high bits at 14..12.
    pub const BPLCON0_BPU_MASK: u16 = 0x7000;
    pub const BPLCON0_BPU_SHIFT: u32 = 12;
}

pub const PAL_CCKS_PER_LINE: u16 = 227;
pub const PAL_LINES_PER_FRAME: u16 = 312;

/// NTSC short-line length (227 CCKs). On NTSC, lines alternate
/// between short (227) and long (228) every line — see
/// `Agnus::lol` / `Agnus::lol_toggle`. The alternation provides the
/// half-line phase shift the colour subcarrier needs and gives
/// NTSC its average 227.5 CCK/line. PAL has a fixed 227 every line
/// (no alternation).
pub const NTSC_CCKS_PER_LINE_SHORT: u16 = 227;
/// NTSC long-line length (228 CCKs). See `NTSC_CCKS_PER_LINE_SHORT`.
pub const NTSC_CCKS_PER_LINE_LONG: u16 = 228;
/// NTSC uses 262 lines per frame (vs PAL's 312).
pub const NTSC_LINES_PER_FRAME: u16 = 262;

/// Total CCKs per non-interlace NTSC frame: 131 short × 227 + 131
/// long × 228 = 59,605. Equivalent to 262 × 227.5.
pub const NTSC_CCKS_PER_FRAME: u32 =
    131 * NTSC_CCKS_PER_LINE_SHORT as u32 + 131 * NTSC_CCKS_PER_LINE_LONG as u32;

/// Total CCKs per non-interlace PAL frame: 312 × 227 = 70,824.
pub const PAL_CCKS_PER_FRAME: u32 = PAL_LINES_PER_FRAME as u32 * PAL_CCKS_PER_LINE as u32;

/// Selected video region. Drives the Agnus revision-ID byte, frame
/// line count, and the per-line short/long alternation flipflop.
/// PAL is the default. Future ECS/AGA Agnus variants override the
/// agnus_id but reuse this enum for region selection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgnusRegion {
    /// PAL: 50 Hz, 312 lines/frame, 227 CCKs every line.
    #[default]
    Pal,
    /// NTSC: 60 Hz, 262 lines/frame, line length alternates 227/228.
    Ntsc,
}

/// Installed original-Agnus silicon revision.
///
/// The VPOSR identity bits distinguish PAL from NTSC but do not distinguish
/// the A1000's 8361/8367 Agnus from the later 8370/8371 original Agnus.
/// Their hard vertical-blank close occurs on different physical lines, and
/// only the A1000 revision delays externally visible blitter busy until its
/// first accepted startup CCK. Machine construction and snapshots must
/// therefore carry that identity separately.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OriginalAgnusRevision {
    /// 8361/8367 as installed in the A1000: hard close on line zero and
    /// delayed visible blitter busy.
    A1000,
    /// 8370/8371 as installed in later original-chipset machines: hard close
    /// on the final physical line of the field.
    #[default]
    Later,
}

/// Derived vertical timing used by the shared sprite-DMA state machine.
///
/// OCS supplies the fixed regional blank-stop line. ECS and AGA supply
/// their latched programmed blank level and one-line `VBSTOP` event while
/// `BEAMCON0.VARVBEN` is active. This value is deliberately transient:
/// ECS/AGA serialize the underlying programmable event-generator state.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteDmaVerticalTiming {
    programmed_blank_active: Option<bool>,
    reset_event: bool,
    fixed_blank_stop: u16,
    sprite_vertical_mask: u16,
}

impl SpriteDmaVerticalTiming {
    /// Fixed OCS/PAL-or-NTSC vertical blank, which occupies lines before
    /// `blank_stop` in the current fixed-timing model.
    #[must_use]
    pub const fn fixed(blank_stop: u16) -> Self {
        Self {
            programmed_blank_active: None,
            reset_event: false,
            fixed_blank_stop: blank_stop,
            sprite_vertical_mask: 0x01FF,
        }
    }

    /// ECS/AGA programmable vertical blank selected from the event-driven
    /// latch and line-held `VBSTOP` pulse.
    #[must_use]
    pub const fn programmed(blank_active: bool, reset_event: bool) -> Self {
        Self {
            programmed_blank_active: Some(blank_active),
            reset_event,
            fixed_blank_stop: 0,
            sprite_vertical_mask: 0x01FF,
        }
    }

    /// Select the undocumented ECS/AGA tenth vertical comparator bit
    /// carried in `SPRxCTL` bits 6 and 5.
    #[must_use]
    pub const fn with_ten_bit_sprite_comparators(self) -> Self {
        Self {
            sprite_vertical_mask: 0x03FF,
            ..self
        }
    }

    const fn sprite_vertical_high_mask(self) -> u16 {
        self.sprite_vertical_mask & 0x0300
    }

    /// Whether the ordinary sprite comparators and DMA requests are
    /// suppressed at `vpos`.
    #[must_use]
    pub const fn blank_active(self, vpos: u16) -> bool {
        match self.programmed_blank_active {
            Some(active) => active,
            None => vpos < self.fixed_blank_stop,
        }
    }

    /// `VBSTOP` (or the fixed regional equivalent) is a one-line sprite
    /// reset and control-refetch event, distinct from the blank level.
    #[must_use]
    pub const fn reset_event(self, vpos: u16) -> bool {
        match self.programmed_blank_active {
            Some(_) => self.reset_event,
            None => vpos == self.fixed_blank_stop,
        }
    }

    /// Fixed timing retains the existing end-of-field safety clear.
    /// Programmable timing derives reset solely from its explicit stop event,
    /// allowing deliberately wrapped active intervals.
    const fn clears_on_field_end(self) -> bool {
        self.programmed_blank_active.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotOwner {
    Cpu,
    Refresh,
    Disk,
    Audio(u8),
    Sprite(u8),
    Bitplane(u8),
    Copper,
}

/// How Paula audio DMA return-latency timing should behave for this CCK slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaulaReturnProgressPolicy {
    /// Return latency advances normally this CCK.
    Advance,
    /// Return latency is stalled by an Agnus-reserved DMA slot.
    Stall,
    /// Return latency advances unless copper actually performs a chip fetch.
    ///
    /// Agnus grants the slot to copper, but the machine must observe whether
    /// copper is in a fetch state or waiting.
    CopperFetchConditional,
}

/// Agnus-owned summary of one CCK bus decision.
///
/// This is the machine-facing API for consumers that need to react to Agnus DMA
/// arbitration (e.g. Paula DMA service/return progress) without duplicating the
/// slot decoding rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CckBusPlan {
    /// Raw slot owner for debugging/inspection. Prefer the explicit grant fields
    /// below for machine behavior.
    pub slot_owner: SlotOwner,
    /// Disk DMA slot service grant for this CCK.
    pub disk_dma_slot_granted: bool,
    /// Sprite DMA slot service grant for this CCK.
    pub sprite_dma_service_channel: Option<u8>,
    /// Paula audio DMA slot service grant for this CCK.
    pub audio_dma_service_channel: Option<u8>,
    /// Bitplane DMA fetch grant for this CCK.
    pub bitplane_dma_fetch_plane: Option<u8>,
    /// Copper is granted this slot (it may still be in WAIT and not fetch).
    pub copper_dma_slot_granted: bool,
    /// CPU chip-bus grant for this CCK in the current arbitration model.
    ///
    /// This is true on CPU/free slots unless another modeled chip-bus client
    /// (currently blitter nasty mode) takes the grant.
    pub cpu_chip_bus_granted: bool,
    /// Blitter chip-bus grant for this CCK.
    ///
    /// The active main blitter in nasty mode (BLTPRI) requests CPU/free slots
    /// while blitter DMA is enabled. The machine executes the admitted
    /// incremental operation in the same CCK and separately latches whether it
    /// actually drove the chip bus.
    pub blitter_chip_bus_granted: bool,
    /// Blitter work-progress grant for this CCK.
    ///
    /// This is the coarse scheduler's "blitter may make progress now" signal.
    /// In the current model, progress is granted on Agnus CPU/free slots while
    /// blitter DMA is enabled and the blitter is busy.
    pub blitter_dma_progress_granted: bool,
    /// Paula audio DMA return-latency policy for this slot.
    pub paula_return_progress_policy: PaulaReturnProgressPolicy,
}

impl CckBusPlan {
    /// Resolve Paula return-latency progress for this CCK.
    ///
    /// `copper_used_chip_bus` is only relevant when
    /// [`PaulaReturnProgressPolicy::CopperFetchConditional`] is selected.
    #[must_use]
    pub fn paula_return_progress(self, copper_used_chip_bus: bool) -> bool {
        match self.paula_return_progress_policy {
            PaulaReturnProgressPolicy::Advance => true,
            PaulaReturnProgressPolicy::Stall => false,
            PaulaReturnProgressPolicy::CopperFetchConditional => !copper_used_chip_bus,
        }
    }
}

/// Source that is authoritative for the blitter's ownership of the current
/// CPU/free CCK.
///
/// A live plan is sufficient before the machine has serviced the current
/// CCK. Once service has run, the recorded outcome wins because the blitter
/// can advance to a different request before the CPU observes the same cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlitterBusDiagnosticAuthority {
    /// No service outcome has been recorded; use the current bus plan.
    CurrentPlanFallback,
    /// The per-CCK actual-use and nasty-ownership latches are authoritative.
    RecordedCckState,
}

/// Side-effect-free view of Agnus arbitration for the current CCK.
///
/// [`Self::plan`] is the live arbitration decision. The recorded fields retain
/// what actually happened earlier in the same CCK, where completing a sprite
/// fetch or advancing the blitter can make a newly computed plan differ from
/// the decision already consumed by the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusBusDiagnosticSnapshot {
    /// Current vertical beam position.
    pub vpos: u16,
    /// Current horizontal beam position in CCKs.
    pub hpos: u16,
    /// Complete current Agnus arbitration plan.
    pub plan: CckBusPlan,
    /// Whether sprite DMA actually drove the chip bus in this CCK.
    pub sprite_bus_used_this_cck: bool,
    /// Effective sprite ownership after combining the live plan with recorded
    /// same-CCK use.
    pub sprite_holds_bus: bool,
    /// Whether a blitter transfer actually drove the chip bus in this CCK.
    pub blitter_bus_used_this_cck: bool,
    /// Whether nasty mode owned this CPU/free cell even if the operation was
    /// internally bus-free.
    pub blitter_nasty_owned_this_cck: bool,
    /// Whether the machine has recorded the current CCK's blitter outcome.
    pub blitter_cck_bus_state_recorded: bool,
    /// State source used to decide [`Self::blitter_holds_bus`].
    pub blitter_authority: BlitterBusDiagnosticAuthority,
    /// Effective blitter ownership after applying the authoritative source.
    pub blitter_holds_bus: bool,
}

/// Installed Agnus identity and construction-time capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusIdentityDiagnosticSnapshot {
    /// VPOSR identity bits for the installed Agnus or Alice revision.
    pub agnus_id: u16,
    /// Original-Agnus revision identity not represented by VPOSR.
    pub original_revision: OriginalAgnusRevision,
    /// Selected fixed-sync video region.
    pub region: AgnusRegion,
    /// Maximum bitplane count admitted by the installed chipset.
    pub max_bitplanes: u8,
}

/// Side-effect-free view of the fixed beam counters and field state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusBeamDiagnosticSnapshot {
    /// Current vertical beam position.
    pub vpos: u16,
    /// Current horizontal beam position in CCKs.
    pub hpos: u16,
    /// Long-frame field flag.
    pub lof: bool,
    /// Fixed-sync lines per non-interlaced frame.
    pub lines_per_frame: u16,
    /// Long-line flip-flop.
    pub lol: bool,
    /// Whether line wrap toggles [`Self::lol`].
    pub lol_toggle: bool,
    /// Number of completed fields since construction.
    pub vbl_count: u64,
    /// Physical length of the current fixed-sync line in CCKs.
    pub current_line_ccks: u16,
    /// Horizontal position currently observed by the Copper comparator.
    pub copper_comparator_hpos: u16,
}

/// Original-Agnus vertical display-window and hard-blank latches.
///
/// Enhanced-chipset wrappers use their own DIWHIGH-aware display-window
/// latch. [`Self::vertical_diw_active`] remains the effective value returned
/// by the shared core, while the two `ocs_*` fields expose the serialized
/// original-Agnus latches even when an outer wrapper does not consume them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusOcsLatchDiagnosticSnapshot {
    /// Effective vertical display-window state returned by the shared core.
    pub vertical_diw_active: bool,
    /// Hidden original-Agnus comparator-driven display-window latch.
    pub ocs_vertical_diw_active: bool,
    /// Line-held original-Agnus hard vertical-blank force-off state.
    pub ocs_hard_vertical_blank_active: bool,
}

/// Fixed-sync event levels and one-position strobes derived from the beam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusEventDiagnosticSnapshot {
    /// Whether the beam is inside the fixed vertical-blank interval.
    pub vertb_level: bool,
    /// Fixed-sync automatic Copper restart position.
    pub fixed_sync_copper_restart_event: bool,
    /// Fixed-sync CIA-A TOD event position.
    pub fixed_sync_cia_a_tod_event: bool,
    /// Fixed-sync CIA-B TOD event position.
    pub fixed_sync_cia_b_tod_event: bool,
}

/// Complete side-effect-free view of the shared sprite-DMA state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusSpriteDmaDiagnosticSnapshot {
    /// Effective chip-RAM pointers used by each sprite channel.
    pub spr_pt: [u32; 8],
    /// Staged high pointer words awaiting matching low-word writes.
    pub spr_pt_hi_latch: [u16; 8],
    /// Whether each staged high pointer word is pending.
    pub spr_pt_hi_pending: [bool; 8],
    /// Latched vertical-start comparators.
    pub spr_vstart: [u16; 8],
    /// Latched vertical-stop comparators.
    pub spr_vstop: [u16; 8],
    /// Per-channel sprite-DMA active latches.
    pub spr_dma_on: [bool; 8],
}

/// Complete non-blitter Agnus diagnostic state not already covered by the
/// arbitration and DDF snapshots.
///
/// This is a read-only projection: observing it does not advance the beam,
/// consume an event, commit a staged pointer or change sprite-DMA state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusDiagnosticSnapshot {
    /// Installed revision, region and chipset capability.
    pub identity: AgnusIdentityDiagnosticSnapshot,
    /// Beam counters, field identity and fixed-sync line geometry.
    pub beam: AgnusBeamDiagnosticSnapshot,
    /// Effective and raw original-Agnus vertical latches.
    pub ocs_latches: AgnusOcsLatchDiagnosticSnapshot,
    /// Live fixed-sync levels and event positions.
    pub events: AgnusEventDiagnosticSnapshot,
    /// Sprite pointer staging, comparators and DMA-active latches.
    pub sprite_dma: AgnusSpriteDmaDiagnosticSnapshot,
}

/// Side-effect-free view of the implemented data-fetch comparator sequencer.
///
/// The raw registers alone cannot reconstruct a run after the beam has
/// observed their comparators. This snapshot therefore includes the frozen
/// match and terminal latches as well as original-Agnus abort/start authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusDdfDiagnosticSnapshot {
    /// Current vertical beam position.
    pub vpos: u16,
    /// Current horizontal beam position in CCKs.
    pub hpos: u16,
    /// Installed Agnus identity bits used to select comparator behavior.
    pub agnus_id: u16,
    /// Raw DDFSTRT register.
    pub ddfstrt: u16,
    /// Raw DDFSTOP register.
    pub ddfstop: u16,
    /// Comparator mask selected by the installed Agnus generation.
    pub comparator_mask: u16,
    /// Masked DDFSTRT comparator value.
    pub effective_ddfstrt: u16,
    /// Masked DDFSTOP comparator value.
    pub effective_ddfstop: u16,
    /// Current line's observed start comparator and frozen fetch origin.
    pub start_match: Option<u16>,
    /// Current line's observed ordinary stop comparator.
    pub stop_match: Option<u16>,
    /// Inclusive terminal CCK frozen for the active fetch run.
    pub fetch_end: Option<u16>,
    /// Whether original Agnus aborted the current-line run after losing an
    /// effective bitplane-DMA eligibility gate.
    pub ocs_run_aborted: bool,
    /// Whether original Agnus currently permits a DDFSTRT comparator to open
    /// a run.
    pub ocs_hard_start_open: bool,
}

/// Maps ddfseq position (0-7) within an 8-CCK group to bitplane index.
/// From Minimig Verilog: `plane = {~ddfseq[0], ~ddfseq[1], ~ddfseq[2]}`.
/// None = free slot (available for copper/CPU).
pub const LOWRES_DDF_TO_PLANE: [Option<u8>; 8] = [
    None,    // 0: free
    Some(3), // 1: BPL4
    Some(5), // 2: BPL6
    Some(1), // 3: BPL2
    None,    // 4: free
    Some(2), // 5: BPL3
    Some(4), // 6: BPL5
    Some(0), // 7: BPL1 (triggers shift register load)
];

/// AGA (Alice) lowres fetch order. Identical to [`LOWRES_DDF_TO_PLANE`]
/// except the two slots OCS/ECS leave idle (positions 0 and 4) carry
/// BPL7 and BPL8 — the extra two planes that let AGA show 256 colours in
/// lowres. Because every shared position matches the OCS table and the
/// fetch loop filters slots to `p < num_bitplanes`, selecting this table
/// for any AGA screen is byte-identical to the OCS table at ≤6 planes and
/// only adds BPL7/BPL8 when they are actually enabled. Lives here (not in
/// `commodore-agnus-aga`) so the shared fetch loop can reach it; the AGA
/// crate re-exports it.
pub const LOWRES_DDF_TO_PLANE_AGA: [Option<u8>; 8] = [
    Some(6), // 0: BPL7 (idle in OCS/ECS)
    Some(3), // 1: BPL4
    Some(5), // 2: BPL6
    Some(1), // 3: BPL2
    Some(7), // 4: BPL8 (idle in OCS/ECS)
    Some(2), // 5: BPL3
    Some(4), // 6: BPL5
    Some(0), // 7: BPL1 (triggers shift register load)
];

/// Hires bitplane fetch order within a 4-CCK group.
///
/// Plane 0 (BPL1) remains last so Denise can trigger a shift-load on the
/// final fetch of the group. Slots for planes >= current depth are free.
pub const HIRES_DDF_TO_PLANE: [Option<u8>; 4] = [
    Some(3), // BPL4
    Some(1), // BPL2
    Some(2), // BPL3
    Some(0), // BPL1 (triggers shift register load)
];

/// Superhires bitplane fetch order within a 2-CCK group (ECS/AGA SHRES).
///
/// SuperHires (BPLCON0 bit 6, 35ns pixels) fetches twice as often as hires,
/// so each fetch group is 2 CCKs. At the 16-bit fetch width (FMODE 0, the
/// classic ECS SuperHires) this caps at 2 planes / 4 colours; BPL1 stays
/// last so Denise triggers its shift-load on the final fetch. The same
/// 2-plane group is reused by the wide-fetch path's smallest SHRES groups.
pub const SHRES_DDF_TO_PLANE: [Option<u8>; 2] = [
    Some(1), // BPL2
    Some(0), // BPL1 (triggers shift register load)
];

/// AGA wide-fetch (FMODE > 0) plane order within a fetchstart group.
///
/// WinUAE's `bpl_sequence_8` (custom.cpp), here 0-based. Every FMODE > 0
/// mode uses the 8-plane sequence (`fm_maxplane == 8`); the fetchstart
/// group is wider than 8 CCK for some widths, so positions 8+ are idle
/// and resolved as free slots by the caller. Index is
/// `(hpos - ddfstrt) % fetchstart`. BPL1 (plane 0) is last among the
/// active planes so Denise can trigger its shift-load on the final fetch.
pub const WIDE_FETCH_PLANE_ORDER: [Option<u8>; 8] = [
    Some(7), // BPL8
    Some(3), // BPL4
    Some(5), // BPL6
    Some(1), // BPL2
    Some(6), // BPL7
    Some(2), // BPL3
    Some(4), // BPL5
    Some(0), // BPL1 (triggers shift register load)
];

/// Bitplane fetch cadence for a given fetch width and resolution,
/// mirroring WinUAE's `fetchunits[]` / `fetchstarts[]` tables
/// (custom.cpp), indexed `[fetchmode * 4 + res]`.
///
/// - `fetchunit` is the ordinary DDFSTOP rounding granularity (color
///   clocks): an ordinary stop completes the unit containing DDFSTOP
///   plus one more unit. Fixed hardware limits can use different
///   termination rules.
/// - `fetchstart` is the plane-fetch group length: one DMA access per
///   active plane per group, each access transferring `fetch_width`
///   words (16-bit = 1, 32-bit = 2, 64-bit = 4).
///
/// `fetch_width == 1` (FMODE = 0, the only case on OCS / ECS) returns
/// the historical 16-bit cadence — so this is a no-op for every
/// non-AGA machine. `shres` (BPLCON0 bit 6) selects the superhires
/// column; it takes precedence over `hires` because real hardware
/// treats the two bits that way.
#[must_use]
pub const fn fetch_cadence(fetch_width: u8, hires: bool, shres: bool) -> (u32, u32) {
    const FETCHUNITS: [u32; 12] = [8, 8, 8, 0, 16, 8, 8, 0, 32, 16, 8, 0];
    const FETCHSTART_SHIFT: [u32; 12] = [3, 2, 1, 0, 4, 3, 2, 0, 5, 4, 3, 0];
    let fetchmode = match fetch_width {
        1 => 0,
        2 => 1,
        _ => 2,
    };
    let res = if shres {
        2
    } else if hires {
        1
    } else {
        0
    };
    let idx = fetchmode * 4 + res;
    (FETCHUNITS[idx], 1u32 << FETCHSTART_SHIFT[idx])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlitterDmaOp {
    ReadA,
    ReadB,
    ReadC,
    WriteD,
    Internal,
}

/// Result of offering one CCK to the blitter scheduler.
///
/// `Startup` is distinct from `NoProgress`: it consumes one of the two
/// accepted/free startup CCKs shared by every supported Agnus/Alice revision,
/// but deliberately does not service the pending channel operation.
#[must_use = "the scheduler outcome distinguishes a withheld CCK, startup progress, and a serviced operation"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlitterProgress {
    NoProgress,
    Startup,
    Operation(BlitterDmaOp),
}

/// Observable work performed by one blitter CCK.
///
/// Completion is not synonymous with pipeline drain on pre-AGA Agnus:
/// an area blit with D enabled emits its finish source while the final
/// result and D write are still pending. The machine therefore consumes
/// the interrupt edge separately from [`Agnus::blitter_busy`].
#[must_use = "the machine must preserve the interrupt edge and bus-use observation"]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BlitterCckOutcome {
    /// The Agnus blitter-finished source fired during this CCK.
    pub interrupt: bool,
    /// A blitter A/B/C read or D write drove the chip bus during this CCK.
    pub bus_used: bool,
}

/// Final two-stage drain of a normal area blit with D enabled.
///
/// The last main cycle is followed by one internal result/BZERO stage and
/// then the final D bus write. Original and ECS Agnus emit completion before
/// these stages; Alice delays completion until the write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum BlitterCompletionPhase {
    FinalResult,
    FinalWrite { addr: u32, value: u16 },
}

/// Per-word blitter state machine: tracks which channel accesses still need
/// servicing for the current word. Replaces the pre-built VecDeque queue so
/// that individual channel accesses can be granted in any order (with the
/// constraint that WriteD must wait until all reads are done).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct BlitterWordState {
    need_a: bool,
    need_b: bool,
    need_c: bool,
    need_d: bool,
    reads_done: bool,
    /// True when no channels are enabled (internal-only timing).
    internal_only: bool,
    /// For internal-only words, tracks whether the single internal op is done.
    internal_done: bool,
}

impl BlitterWordState {
    fn new_area(use_a: bool, use_b: bool, use_c: bool, use_d: bool) -> Self {
        let internal_only = !use_a && !use_b && !use_c && !use_d;
        let reads_done = !use_a && !use_b && !use_c;
        Self {
            need_a: use_a,
            need_b: use_b,
            need_c: use_c,
            need_d: use_d,
            reads_done,
            internal_only,
            internal_done: false,
        }
    }

    fn new_line() -> Self {
        Self {
            need_a: false,
            need_b: false,
            need_c: true,
            need_d: true,
            reads_done: false,
            internal_only: false,
            internal_done: false,
        }
    }

    fn is_complete(&self) -> bool {
        if self.internal_only {
            return self.internal_done;
        }
        !self.need_a && !self.need_b && !self.need_c && !self.need_d
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct BlitterLineRuntime {
    steps_remaining: u32,
    error: i16,
    error_add: i16,
    error_sub: i16,
    cpt: u32,
    dpt: u32,
    pixel_bit: u16,
    row_mod: i16,
    /// Preloaded BLTBDAT line texture. Standard line setup leaves B DMA
    /// disabled; the internal shifter consumes this register value directly.
    texture: u16,
    /// Current BLTCON1 BSH selector. Zero selects texture bit 0, then each
    /// generated pixel decrements the selector with 15-to-0 wrap.
    texture_bit: u8,
    lf: u8,
    sing: bool,
    /// Whether the current horizontal row has already emitted its ONEDOT
    /// D transfer. A vertical step starts a fresh row and clears the latch.
    one_dot_drawn: bool,
    major_is_y: bool,
    x_neg: bool,
    y_neg: bool,
    last_c_word: u16,
    have_c_word: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct BlitterAreaRuntime {
    rows_remaining: u32,
    width_words: u32,
    words_remaining_in_row: u32,
    use_a: bool,
    use_b: bool,
    use_c: bool,
    use_d: bool,
    lf: u8,
    a_shift: u16,
    b_shift: u16,
    desc: bool,
    ptr_step: i32,
    mod_dir: i32,
    fill_enabled: bool,
    ife: bool,
    efe: bool,
    fill_carry_init: u16,
    fill_carry: u16,
    apt: u32,
    bpt: u32,
    cpt: u32,
    dpt: u32,
    amod: i16,
    bmod: i16,
    cmod: i16,
    dmod: i16,
    a_prev: u16,
    b_prev: u16,
    a_raw: u16,
    b_raw: u16,
    c_val: u16,
}

/// Side-effect-free view of every implemented blitter register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusBlitterRegistersDiagnosticSnapshot {
    /// Raw BLTCON0 control register.
    pub bltcon0: u16,
    /// Raw BLTCON1 control register.
    pub bltcon1: u16,
    /// Raw legacy BLTSIZE register.
    pub bltsize: u16,
    /// Raw shared ECS BLTSIZV shadow used by wrapper integrations.
    pub bltsizv_ecs: u16,
    /// Raw shared ECS BLTSIZH shadow used by wrapper integrations.
    pub bltsizh_ecs: u16,
    /// Current A-channel pointer.
    pub blt_apt: u32,
    /// Current B-channel pointer.
    pub blt_bpt: u32,
    /// Current C-channel pointer.
    pub blt_cpt: u32,
    /// Current D-channel pointer.
    pub blt_dpt: u32,
    /// Signed A-channel modulo.
    pub blt_amod: i16,
    /// Signed B-channel modulo.
    pub blt_bmod: i16,
    /// Signed C-channel modulo.
    pub blt_cmod: i16,
    /// Signed D-channel modulo.
    pub blt_dmod: i16,
    /// Current A-channel data register.
    pub blt_adat: u16,
    /// Current B-channel data register.
    pub blt_bdat: u16,
    /// Current C-channel data register.
    pub blt_cdat: u16,
    /// First-word A mask.
    pub blt_afwm: u16,
    /// Last-word A mask.
    pub blt_alwm: u16,
}

/// Public diagnostic projection of the blitter's final-result/final-D drain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgnusBlitterCompletionDiagnosticPhase {
    /// No final-result or final-D pipeline stage is pending.
    None,
    /// The final destination result and BZERO update are pending.
    FinalResult,
    /// The final destination word is buffered and waiting for a bus grant.
    FinalWrite {
        /// Chip-RAM destination address.
        address: u32,
        /// Buffered destination value.
        value: u16,
    },
}

/// Side-effect-free view of blitter scheduling and completion state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusBlitterExecutionDiagnosticSnapshot {
    /// Installed Agnus/Alice identity bits that select completion behavior.
    pub agnus_id: u16,
    /// Installed original-Agnus revision that selects visible startup timing.
    pub original_revision: OriginalAgnusRevision,
    /// Raw DMACON state that gates blitter progress and priority.
    pub dmacon: u16,
    /// Whether master DMA and blitter DMA are both enabled.
    pub dma_enabled: bool,
    /// Whether the BLTPRI priority bit is set.
    pub priority_enabled: bool,
    /// Internal blitter activity, including startup and completion drain.
    pub busy: bool,
    /// DMACONR-visible busy state.
    pub busy_visible: bool,
    /// Busy state observed by Copper BFD.
    pub busy_copper: bool,
    /// Whether the current main-engine request can steal a CPU/free CCK.
    pub nasty_active: bool,
    /// Accepted/free startup CCKs remaining before the first channel op.
    pub startup_ccks_remaining: u8,
    /// Current final-result/final-D completion phase.
    pub completion_phase: AgnusBlitterCompletionDiagnosticPhase,
    /// CCK stages remaining in the final-result/final-D drain.
    pub completion_ccks_remaining: u8,
    /// Whether a final destination result or write remains pending.
    pub final_d_pending: bool,
    /// Whether the one-shot blitter-finished source has fired.
    pub finish_emitted: bool,
    /// DMACONR busy-retention CCKs remaining.
    pub dmacon_busy_hold_ccks: u8,
    /// Copper busy-retention CCKs remaining.
    pub copper_busy_hold_ccks: u8,
    /// Whether incremental execution still has a pending word operation.
    pub exec_pending: bool,
    /// Whether register-programmed work is ready to be admitted.
    pub exec_ready: bool,
    /// Running BZERO state; `true` means every generated result remains zero.
    pub zero: bool,
    /// Effective height or line length captured when the blit started.
    pub height: u32,
    /// Effective width in words captured when the blit started.
    pub width_words: u32,
    /// Remaining channel/internal scheduler operations.
    pub ccks_remaining: u32,
    /// Next channel/internal operation requested from arbitration.
    pub next_dma_request: Option<BlitterDmaOp>,
    /// Whether arbitration must reserve a chip-bus cell for the next admitted
    /// progress stage.
    pub next_progress_uses_bus: bool,
    /// Whether all operations for the current word or line step are complete.
    pub word_complete: bool,
    /// Whether an area or line incremental runtime is installed.
    pub incremental_runtime_present: bool,
}

/// Side-effect-free view of the current word's pending channel operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusBlitterWordDiagnosticSnapshot {
    /// Whether an A-channel read is pending.
    pub need_a: bool,
    /// Whether a B-channel read is pending.
    pub need_b: bool,
    /// Whether a C-channel read is pending.
    pub need_c: bool,
    /// Whether a D-channel write is pending.
    pub need_d: bool,
    /// Whether every enabled source read has completed.
    pub reads_done: bool,
    /// Whether the word has no external channel operations.
    pub internal_only: bool,
    /// Whether the internal-only timing operation has completed.
    pub internal_done: bool,
}

/// Side-effect-free view of the implemented line-mode runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusBlitterLineDiagnosticSnapshot {
    /// Remaining pixel steps.
    pub steps_remaining: u32,
    /// Current Bresenham error accumulator.
    pub error: i16,
    /// Error term added on a major-axis-only step.
    pub error_add: i16,
    /// Error term added when both axes move.
    pub error_sub: i16,
    /// Internal C-channel pointer.
    pub cpt: u32,
    /// Internal D-channel pointer.
    pub dpt: u32,
    /// Current pixel bit within the destination word.
    pub pixel_bit: u16,
    /// Signed row modulo.
    pub row_mod: i16,
    /// Latched line texture.
    pub texture: u16,
    /// Current line-texture bit selector.
    pub texture_bit: u8,
    /// Latched minterm lookup function.
    pub lf: u8,
    /// Whether ONEDOT line suppression is enabled.
    pub sing: bool,
    /// Whether the current horizontal row has already emitted its D transfer.
    pub one_dot_drawn: bool,
    /// Whether Y is the major axis.
    pub major_is_y: bool,
    /// Whether X movement is negative.
    pub x_negative: bool,
    /// Whether Y movement is negative.
    pub y_negative: bool,
    /// Most recently fetched C word.
    pub last_c_word: u16,
    /// Whether [`Self::last_c_word`] belongs to the current step.
    pub have_c_word: bool,
}

/// Side-effect-free view of the implemented area-mode runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusBlitterAreaDiagnosticSnapshot {
    /// Rows not yet completed.
    pub rows_remaining: u32,
    /// Captured width in words.
    pub width_words: u32,
    /// Words remaining in the current row.
    pub words_remaining_in_row: u32,
    /// Whether A DMA is enabled for this blit.
    pub use_a: bool,
    /// Whether B DMA is enabled for this blit.
    pub use_b: bool,
    /// Whether C DMA is enabled for this blit.
    pub use_c: bool,
    /// Whether D DMA is enabled for this blit.
    pub use_d: bool,
    /// Latched minterm lookup function.
    pub lf: u8,
    /// Captured A shift.
    pub a_shift: u16,
    /// Captured B shift.
    pub b_shift: u16,
    /// Whether descending mode is active.
    pub descending: bool,
    /// Signed per-word pointer step.
    pub pointer_step: i32,
    /// Sign applied to row modulos.
    pub modulo_direction: i32,
    /// Whether either fill mode is enabled.
    pub fill_enabled: bool,
    /// Whether inclusive fill is enabled.
    pub inclusive_fill_enabled: bool,
    /// Whether exclusive fill is enabled.
    pub exclusive_fill_enabled: bool,
    /// Fill carry loaded at the start of each row.
    pub fill_carry_initial: u16,
    /// Current fill carry.
    pub fill_carry: u16,
    /// Internal A-channel pointer.
    pub apt: u32,
    /// Internal B-channel pointer.
    pub bpt: u32,
    /// Internal C-channel pointer.
    pub cpt: u32,
    /// Internal D-channel pointer.
    pub dpt: u32,
    /// Captured A modulo.
    pub amod: i16,
    /// Captured B modulo.
    pub bmod: i16,
    /// Captured C modulo.
    pub cmod: i16,
    /// Captured D modulo.
    pub dmod: i16,
    /// Previous masked A word used by the shift pipeline.
    pub a_previous: u16,
    /// Previous B word used by the shift pipeline.
    pub b_previous: u16,
    /// Current raw A word.
    pub a_raw: u16,
    /// Current raw B word.
    pub b_raw: u16,
    /// Current C value.
    pub c_value: u16,
}

/// Complete side-effect-free view of the implemented Agnus blitter.
///
/// The register projection is kept separate from execution and per-mode
/// runtime state so diagnostic consumers can compare stable register
/// programming independently from a partially completed word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgnusBlitterDiagnosticSnapshot {
    /// Raw programmed and live blitter registers.
    pub registers: AgnusBlitterRegistersDiagnosticSnapshot,
    /// Scheduler, busy-observer, BZERO and completion state.
    pub execution: AgnusBlitterExecutionDiagnosticSnapshot,
    /// Pending operations for the current word or line step.
    pub word: Option<AgnusBlitterWordDiagnosticSnapshot>,
    /// Line-mode runtime, present only for an active line blit.
    pub line: Option<AgnusBlitterLineDiagnosticSnapshot>,
    /// Area-mode runtime, present only for an active area blit.
    pub area: Option<AgnusBlitterAreaDiagnosticSnapshot>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Agnus {
    pub vpos: u16,
    pub hpos: u16, // in CCKs

    // DMA Registers
    pub dmacon: u16,
    pub bplcon0: u16,
    /// Maximum bitplane count: 6 for OCS/ECS, 8 for AGA.
    ///
    /// Controls whether BPLCON0 bit 4 extends the BPU field to 4 bits (8 planes).
    /// Set at construction time by the outermost chipset wrapper.
    pub max_bitplanes: u8,
    pub bpl_pt: [u32; 8],
    pub ddfstrt: u16,
    pub ddfstop: u16,
    /// Current line's observed DDFSTRT comparator and frozen fetch-phase
    /// origin. `None` means the comparator has not matched this line.
    ddf_start_match: Option<u16>,
    /// Current line's ordinary DDFSTOP event that requested termination
    /// of an active fetch run. Later register writes cannot revoke it.
    ddf_stop_match: Option<u16>,
    /// Inclusive terminal CCK for the current fetch region, frozen when
    /// either ordinary DDFSTOP or a fixed hardware boundary requests
    /// termination. `None` means no terminal endpoint has been latched
    /// for an active fetch region this line.
    ddf_fetch_end: Option<u16>,
    /// Original-Agnus current-line run-abort latch. Losing effective
    /// bitplane eligibility through DMA disable or a vertical display-window
    /// stop before a terminal request ends the active sequencer without
    /// erasing the observed DDFSTRT phase used by Denise. Restoring the
    /// eligibility gate alone therefore cannot resume that stale run.
    ///
    /// Enhanced-chipset wrappers serialize this shared field but do not
    /// consume it; their soft-enable and multi-region behavior needs a
    /// separate sequencer model.
    ocs_ddf_run_aborted: bool,
    /// Effective original-Agnus horizontal start permission. `$18` opens
    /// it and completion of a terminal fetch unit closes it. A one-CCK
    /// logical tail beyond a short physical line is projected at wrap into
    /// its proven next-line start-inhibition result; this field does not
    /// claim the tail's exact bus position. Unlike the current-line
    /// comparator fields, this state survives line wrap.
    ///
    /// Enhanced-chipset wrappers carry the field because they serialize
    /// this shared inner core, but their timing does not consume it.
    ocs_ddf_hard_start_open: bool,

    // Blitter Registers
    pub bltcon0: u16,
    pub bltcon1: u16,
    pub bltsize: u16,
    pub bltsizv_ecs: u16,
    pub bltsizh_ecs: u16,
    /// Internal blitter activity. This becomes true as soon as a blit starts
    /// so arbitration, nasty mode and completion draining can react
    /// immediately, even on the A1000 revision whose external BBUSY signal
    /// has a one-progress-CCK startup delay.
    pub blitter_busy: bool,
    /// Accepted/free startup CCKs remaining before the first channel op.
    ///
    /// All supported revisions share the two-CCK startup pipeline. The A1000
    /// additionally keeps external BBUSY clear while this is `2`, then asserts
    /// it when the first accepted startup CCK decrements the value to `1`.
    blitter_startup_ccks_remaining: u8,
    /// Pending final-result/final-D drain for a normal D-enabled area blit.
    blitter_completion_phase: Option<BlitterCompletionPhase>,
    /// One-shot main-finish source for the current blit.
    ///
    /// This becomes true before the final D drain on pre-AGA chips and at
    /// final D on Alice. It is distinct from `blitter_busy`, which remains
    /// true until every internal pipeline stage has drained.
    blitter_finish_emitted: bool,
    /// DMACONR retains BBUSY through the CCK that emits the finish source.
    blitter_dmacon_busy_hold_ccks: u8,
    /// Copper BFD observes the blitter-finished condition one CCK later than
    /// DMACONR in the pinned cycle-exact completion model.
    blitter_copper_busy_hold_ccks: u8,
    pub blitter_exec_pending: bool,
    /// Running NOR of every D result the current blit has generated: stays
    /// `true` while all results are zero, cleared on the first non-zero
    /// result even when D DMA is disabled. Read out as DMACONR BZERO
    /// (bit 13). The previous result is preserved at `start_blit`, then reset
    /// on the first accepted startup CCK.
    pub blitter_dzero: bool,
    /// Effective blit height (rows for area mode, length for line mode):
    /// the legacy BLTSIZE `>>6` field (0→1024) or, for an ECS large
    /// blit, the full 15-bit BLTSIZV (0→32768). Set at blit start so the
    /// op count + runtime don't re-derive a wrapped value from the
    /// 10-bit legacy field (#36).
    pub blt_height: u32,
    /// Effective blit width in words: the legacy BLTSIZE low-6 field
    /// (0→64) or, for an ECS large blit, the full 11-bit BLTSIZH
    /// (0→2048). Set at blit start (#36).
    pub blt_width_words: u32,
    pub blitter_ccks_remaining: u32,
    blitter_word_state: Option<BlitterWordState>,
    blitter_line_runtime: Option<BlitterLineRuntime>,
    blitter_area_runtime: Option<BlitterAreaRuntime>,
    pub blt_apt: u32,
    pub blt_bpt: u32,
    pub blt_cpt: u32,
    pub blt_dpt: u32,
    pub blt_amod: i16,
    pub blt_bmod: i16,
    pub blt_cmod: i16,
    pub blt_dmod: i16,
    pub blt_adat: u16,
    pub blt_bdat: u16,
    pub blt_cdat: u16,
    pub blt_afwm: u16,
    pub blt_alwm: u16,

    // Display window
    pub diwstrt: u16,
    pub diwstop: u16,
    /// Hidden original-Agnus vertical display-window flip-flop. Decoded
    /// VSTART/VSTOP comparator events and the revision-specific hard
    /// vertical-blank close change this state; live register geometry does
    /// not reconstruct it.
    ///
    /// Enhanced-chipset wrappers serialize this shared field but use their
    /// own DIWHIGH-aware vertical latch.
    ocs_vertical_diw_active: bool,
    /// Line-held original-Agnus hard vertical-blank force-off state.
    ///
    /// This is selected on line entry from the installed revision and the
    /// then-current field length. It remains stable across same-line register
    /// writes, including changes to interlace state.
    ocs_hard_vertical_blank_active: bool,
    pub bpl1mod: i16,
    pub bpl2mod: i16,

    // Sprite pointers
    pub spr_pt: [u32; 8],
    spr_pt_hi_latch: [u16; 8],
    spr_pt_hi_pending: [bool; 8],

    // Sprite-DMA state machine (gap #162). Agnus latches each sprite's
    // VSTART/VSTOP from the control words it fetches and tracks the
    // per-sprite DMA-on flag; the control-vs-data decision lives in
    // `service_sprite_dma_cyc`, the activation/deactivation in
    // `update_sprite_dma`. Grounded in vAmiga Agnus.cpp:559-680.
    spr_vstart: [u16; 8],
    spr_vstop: [u16; 8],
    spr_dma_on: [bool; 8],
    /// Record that sprite DMA drove the chip bus during the current CCK.
    /// A second control-word fetch can change VSTOP and make a newly
    /// computed plan look idle; CPU arbitration must still honour the
    /// original use for both master/4 phases of that CCK. Snapshots
    /// serialize this alongside the machine's half-CCK phase.
    sprite_bus_used_this_cck: bool,
    /// Record that the current CCK's pre-service plan gave the chip bus to a
    /// blitter transfer. Completion can change the live plan before the CPU
    /// polls later in the same CCK, so the actual use must remain latched
    /// across both master/4 phases.
    blitter_bus_used_this_cck: bool,
    /// Record that the pre-service plan assigned the current CPU/free cell
    /// to a nasty blitter operation. The line engine can advance from a
    /// bus-free ONEDOT would-be write to its next C read before the CPU polls;
    /// preserving the original decision prevents that next request from
    /// retroactively taking the already-free cell.
    blitter_nasty_owned_this_cck: bool,
    /// Whether the current CCK has a pre-service/actual-use blitter
    /// arbitration observation. Direct bus-service callers that have not run
    /// the phase-0 DMA body must fall back to the live plan.
    blitter_cck_bus_state_recorded: bool,

    // Disk pointer
    pub dsk_pt: u32,

    /// AGA FMODE register — controls bitplane/sprite DMA fetch width.
    pub fmode: u16,

    /// Long frame flag (LOF). Toggled at frame start when BPLCON0 LACE is set.
    /// LOF=true means long/odd frame (313 lines PAL, 263 NTSC).
    /// LOF=false means short/even frame (312 lines PAL, 262 NTSC).
    /// Starts true (first frame is long).
    pub lof: bool,

    /// Lines per frame for this region (312 PAL, 262 NTSC). Set at
    /// construction time and used by `tick_cck()` for frame wrapping.
    pub lines_per_frame: u16,

    /// Agnus revision ID, stored *pre-shifted* into bits 14-8 of a
    /// `u16` — `vposr()` returns `agnus_id & 0x7F00`, so the field
    /// is the literal value KS reads from VPOSR (minus LOF and the
    /// vpos high bit). Real-chip values:
    ///   * OCS NTSC 8361 / 8370 = `$1000`
    ///   * OCS PAL  8367 / 8371 = `$0000`
    ///   * ECS NTSC 8375        = `$3000`
    ///   * ECS PAL  8375        = `$2000`
    ///   * AGA Alice NTSC       = `$3300`
    ///   * AGA Alice PAL        = `$2300`
    ///
    /// Each wrapper (`AgnusEcs`, `AgnusAga`) overrides this in its
    /// constructor so the inner OCS struct still serialises cleanly
    /// while the bus-read returns the wrapper's true chip identity.
    pub agnus_id: u16,

    /// Original-Agnus revision identity not represented by VPOSR.
    ///
    /// Enhanced-chipset wrappers serialize this nested field but do not
    /// consume it because their `agnus_id` selects their own vertical latch.
    original_revision: OriginalAgnusRevision,

    /// Selected video region. PAL or NTSC. Drives `lines_per_frame`,
    /// `agnus_id`, and `lol_toggle`.
    pub region: AgnusRegion,

    /// Long-line flipflop (LOL). When true, the current line is 228
    /// CCKs instead of 227. Toggles at end-of-line if `lol_toggle`
    /// is set (NTSC default). Starts false on every region; the
    /// first NTSC line is short (227), the second is long (228), and
    /// so on — see HRM p. 785 and vAmiga's `Beam::eol`.
    pub lol: bool,

    /// Whether `lol` toggles at end-of-line. False on PAL (every
    /// line is the same length). True on NTSC by default; can be
    /// disabled via the BPLCON3 LOLDIS bit on ECS+ machines (not
    /// modelled in OCS — ECS BPLCON3 is a separate add).
    pub lol_toggle: bool,

    /// Total VBLs since construction. Diagnostic — incremented every
    /// time `tick_cck` wraps vpos.
    pub vbl_count: u64,
}

impl Agnus {
    pub fn new() -> Self {
        Self {
            vpos: 0,
            hpos: 0,
            dmacon: 0,
            bplcon0: 0,
            max_bitplanes: 6,
            bpl_pt: [0; 8],
            ddfstrt: 0,
            ddfstop: 0,
            ddf_start_match: None,
            ddf_stop_match: None,
            ddf_fetch_end: None,
            ocs_ddf_run_aborted: false,
            // Preserve the established first-line behavior. The bounded
            // hardware contract begins once `$18` or a terminal completion
            // has driven the latch.
            ocs_ddf_hard_start_open: true,
            bltcon0: 0,
            bltcon1: 0,
            bltsize: 0,
            bltsizv_ecs: 0,
            bltsizh_ecs: 0,
            blitter_busy: false,
            blitter_startup_ccks_remaining: 0,
            blitter_completion_phase: None,
            blitter_finish_emitted: false,
            blitter_dmacon_busy_hold_ccks: 0,
            blitter_copper_busy_hold_ccks: 0,
            blitter_exec_pending: false,
            blitter_dzero: true,
            blt_height: 0,
            blt_width_words: 0,
            blitter_ccks_remaining: 0,
            blitter_word_state: None,
            blitter_line_runtime: None,
            blitter_area_runtime: None,
            blt_apt: 0,
            blt_bpt: 0,
            blt_cpt: 0,
            blt_dpt: 0,
            blt_amod: 0,
            blt_bmod: 0,
            blt_cmod: 0,
            blt_dmod: 0,
            blt_adat: 0,
            blt_bdat: 0,
            blt_cdat: 0,
            blt_afwm: 0xFFFF,
            blt_alwm: 0xFFFF,
            diwstrt: 0,
            diwstop: 0,
            ocs_vertical_diw_active: false,
            ocs_hard_vertical_blank_active: false,
            bpl1mod: 0,
            bpl2mod: 0,
            spr_pt: [0; 8],
            spr_pt_hi_latch: [0; 8],
            spr_pt_hi_pending: [false; 8],
            spr_vstart: [0; 8],
            spr_vstop: [0; 8],
            spr_dma_on: [false; 8],
            sprite_bus_used_this_cck: false,
            blitter_bus_used_this_cck: false,
            blitter_nasty_owned_this_cck: false,
            blitter_cck_bus_state_recorded: false,
            dsk_pt: 0,
            fmode: 0,
            lof: true,
            lines_per_frame: PAL_LINES_PER_FRAME,
            agnus_id: 0x0000,
            original_revision: OriginalAgnusRevision::Later,
            region: AgnusRegion::Pal,
            lol: false,
            lol_toggle: false,
            vbl_count: 0,
        }
    }

    /// Create a new Agnus with the specified lines-per-frame count.
    /// Kept for backward compatibility with existing call sites that
    /// only need to override the line count; new callers should use
    /// `new_with_region` for explicit PAL/NTSC selection.
    #[must_use]
    pub fn new_with_region_lines(lines_per_frame: u16) -> Self {
        let mut agnus = Self::new();
        agnus.lines_per_frame = lines_per_frame;
        agnus
    }

    /// Create a later original Agnus configured for the named video region.
    ///
    /// PAL is the existing default: every line is 227 CCKs, each short field
    /// is 312 lines, and the 8371 uses `agnus_id = $0000`, shared with the
    /// A1000's 8367. NTSC alternates short and long lines (227/228) per HRM
    /// p. 785, uses 262-line short fields, and the 8370 uses
    /// `agnus_id = $1000`, shared with the A1000's 8361. The first NTSC line
    /// is short; the alternation is strict until ECS adds the LOLDIS bit on
    /// BPLCON3. `agnus_id` is stored pre-shifted into VPOSR bits 14-8.
    #[must_use]
    pub fn new_with_region(region: AgnusRegion) -> Self {
        let mut agnus = Self::new();
        match region {
            AgnusRegion::Pal => {
                agnus.region = AgnusRegion::Pal;
                agnus.lines_per_frame = PAL_LINES_PER_FRAME;
                agnus.agnus_id = 0x0000;
                agnus.lol = false;
                agnus.lol_toggle = false;
            }
            AgnusRegion::Ntsc => {
                agnus.region = AgnusRegion::Ntsc;
                agnus.lines_per_frame = NTSC_LINES_PER_FRAME;
                agnus.agnus_id = 0x1000;
                agnus.lol = false;
                agnus.lol_toggle = true;
            }
        }
        agnus
    }

    /// Create the 8361/8367 original Agnus installed in an A1000.
    ///
    /// PAL/NTSC VPOSR identity and beam totals remain region-selected. The
    /// separate revision identity selects the A1000 line-zero hard
    /// vertical-blank close and delayed visible blitter-busy signal.
    #[must_use]
    pub fn new_a1000_with_region(region: AgnusRegion) -> Self {
        let mut agnus = Self::new_with_region(region);
        agnus.original_revision = OriginalAgnusRevision::A1000;
        // Construction starts at line zero, the A1000 hard-blank line.
        agnus.ocs_hard_vertical_blank_active = true;
        agnus
    }

    /// Installed original-Agnus revision.
    #[must_use]
    pub const fn original_revision(&self) -> OriginalAgnusRevision {
        self.original_revision
    }

    /// Create the 1 MiB Fat Agnus 8372A used with OCS Denise in later
    /// A500 and A2000 revisions. Its chip-stack shape remains OCS.
    /// Revision behaviour shared by this core—including ten-bit
    /// sprite vertical comparators—is selected by the `$2000`/`$3000`
    /// VPOSR identity. The rest of the 8372A's ECS register surface is
    /// supplied by the ECS extension layer, not this shared core.
    #[must_use]
    pub fn new_fat_agnus_with_region(region: AgnusRegion) -> Self {
        let mut agnus = Self::new_with_region(region);
        agnus.agnus_id = match region {
            AgnusRegion::Pal => 0x2000,
            AgnusRegion::Ntsc => 0x3000,
        };
        agnus
    }

    /// Length of the current scanline in CCKs. PAL is always 227;
    /// NTSC returns 227 (short) or 228 (long) depending on the LOL
    /// flipflop. Used by Denise's end-of-line border-fill logic to
    /// know where the last paintable CCK lives.
    #[must_use]
    pub fn current_line_ccks(&self) -> u16 {
        if self.lol {
            NTSC_CCKS_PER_LINE_LONG
        } else {
            NTSC_CCKS_PER_LINE_SHORT
        }
    }

    /// Horizontal position currently visible to the Copper `WAIT`/`SKIP`
    /// comparator.
    ///
    /// The comparator observes the beam two CCKs ahead. Its effective period
    /// is the largest even number of CCKs in the current line: PAL and NTSC
    /// short lines therefore wrap at physical `$E0`, while an NTSC long line
    /// wraps at physical `$E2`.
    #[must_use]
    pub fn copper_comparator_hpos(&self) -> u16 {
        let period = self.current_line_ccks() & !1;
        if period == 0 {
            0
        } else {
            (self.hpos + 2) % period
        }
    }

    pub fn num_bitplanes(&self) -> u8 {
        if self.max_bitplanes > 6 {
            // AGA: 4-bit BPU from bits 14-12 (3 bits) + bit 4 (extra high bit).
            let bpu_hi3 = ((self.bplcon0 >> 12) & 0x07) as u8;
            let bpu_bit3 = ((self.bplcon0 >> 4) & 0x01) as u8;
            let bpu = (bpu_bit3 << 3) | bpu_hi3;
            bpu.min(self.max_bitplanes)
        } else {
            let bpl_bits = (self.bplcon0 >> 12) & 0x07;
            if bpl_bits > 6 { 6 } else { bpl_bits as u8 }
        }
    }

    /// Bitplane DMA fetch width in 16-bit words, from FMODE bits 1..0:
    /// 1 (16-bit), 2 (32-bit), or 4 (64-bit). `fmode` stays 0 on OCS /
    /// ECS (Kickstart never writes $1FC there), so this returns 1 — the
    /// historical behaviour. The bitplane fetch scheduler reads this
    /// directly, so the AGA machine must propagate FMODE writes onto
    /// this inner Agnus (not only the AGA wrapper).
    #[must_use]
    pub fn bpl_fetch_width(&self) -> u8 {
        match self.fmode & 0x0003 {
            0 => 1,
            1 | 2 => 2,
            _ => 4,
        }
    }

    /// Sprite DMA fetch width in 16-bit words, from FMODE bits 3..2.
    /// Returns 1 on OCS / ECS (`fmode` always 0).
    #[must_use]
    pub fn spr_fetch_width(&self) -> u8 {
        match (self.fmode >> 2) & 0x0003 {
            0 => 1,
            1 | 2 => 2,
            _ => 4,
        }
    }

    pub fn dma_enabled(&self, bit: u16) -> bool {
        (self.dmacon & 0x0200) != 0 && (self.dmacon & bit) != 0
    }

    /// Sprite pointer register write semantics: `SPRxPTH` stages the high word,
    /// `SPRxPTL` commits the effective pointer used by DMA.
    pub fn write_sprite_pointer_reg(&mut self, sprite: usize, high_word: bool, val: u16) {
        if sprite >= 8 {
            return;
        }

        if high_word {
            self.spr_pt_hi_latch[sprite] = val;
            self.spr_pt_hi_pending[sprite] = true;
            return;
        }

        let hi = if self.spr_pt_hi_pending[sprite] {
            self.spr_pt_hi_latch[sprite]
        } else {
            (self.spr_pt[sprite] >> 16) as u16
        };
        self.spr_pt[sprite] = (u32::from(hi) << 16) | u32::from(val & 0xFFFE);
        self.spr_pt_hi_latch[sprite] = hi;
        self.spr_pt_hi_pending[sprite] = false;
    }

    /// `true` while the blitter's main engine is in nasty mode and may steal
    /// CPU/free slots.
    ///
    /// The internal result/final-D tail does not itself own a chip-bus cell,
    /// including Alice's two CCKs before its delayed finish source. A later
    /// final D still blocks same-CCK CPU chip access through
    /// [`Agnus::blitter_bus_used_this_cck`].
    #[must_use]
    pub fn blitter_nasty_active(&self) -> bool {
        const DMACON_BLTEN: u16 = 0x0040;
        const DMACON_BLTPRI: u16 = 0x0400;

        self.blitter_busy
            && !self.blitter_finish_emitted
            && self.blitter_completion_phase.is_none()
            && self.dma_enabled(DMACON_BLTEN)
            && (self.dmacon & DMACON_BLTPRI) != 0
            && self.next_blitter_progress_uses_bus()
    }

    /// Whether DMACONR-visible blitter busy is asserted.
    ///
    /// Internal activity begins immediately on every Agnus revision. The
    /// 8361/8367 installed in the A1000 delays only its externally visible
    /// BBUSY signal until the first accepted/free blitter progress CCK.
    ///
    /// Completion has a second observer boundary. Original and ECS Agnus
    /// emit their main-finish source before the final D pipeline drains, and
    /// DMACONR retains BBUSY through that finish CCK. Copper has its own later
    /// observation exposed by [`Agnus::blitter_busy_copper`]. Diagnostics and
    /// completion admission use internal [`Agnus::blitter_busy`]; nasty
    /// ownership ends at main finish and later bus transfers are latched from
    /// their actual use.
    #[must_use]
    pub fn blitter_busy_visible(&self) -> bool {
        self.blitter_observer_busy() || self.blitter_dmacon_busy_hold_ccks != 0
    }

    /// Whether the Copper's BFD blitter-finished input remains busy.
    ///
    /// The A1000 startup exception is shared with DMACONR, but completion is
    /// not: the Copper retains the busy condition for one additional CCK.
    #[must_use]
    pub fn blitter_busy_copper(&self) -> bool {
        self.blitter_observer_busy() || self.blitter_copper_busy_hold_ccks != 0
    }

    /// Stable diagnostic name for the current final-D completion stage.
    #[must_use]
    pub fn blitter_completion_phase(&self) -> &'static str {
        match self.blitter_completion_phase {
            Some(BlitterCompletionPhase::FinalResult) => "final-result",
            Some(BlitterCompletionPhase::FinalWrite { .. }) => "final-write",
            None if self.blitter_busy => "running",
            None => "idle",
        }
    }

    /// CCK stages remaining in the bounded final-D completion pipeline.
    #[must_use]
    pub const fn blitter_completion_ccks_remaining(&self) -> u8 {
        match self.blitter_completion_phase {
            Some(BlitterCompletionPhase::FinalResult) => 2,
            Some(BlitterCompletionPhase::FinalWrite { .. }) => 1,
            None => 0,
        }
    }

    /// Whether a final area-mode D result remains to be written.
    #[must_use]
    pub const fn blitter_final_d_pending(&self) -> bool {
        self.blitter_completion_phase.is_some()
    }

    /// Whether the one-shot blitter-finished source has fired for this blit.
    #[must_use]
    pub const fn blitter_finish_emitted(&self) -> bool {
        self.blitter_finish_emitted
    }

    #[must_use]
    fn blitter_observer_busy(&self) -> bool {
        self.blitter_busy
            && !self.blitter_finish_emitted
            && !(self.delays_visible_blitter_busy() && self.blitter_startup_ccks_remaining == 2)
    }

    /// Accepted startup CCKs remaining before the first channel operation.
    ///
    /// This is diagnostic state. A value of `2` is the just-started state,
    /// `1` means the first startup CCK has been accepted, and `0` admits
    /// normal A/B/C/D/internal operations.
    #[must_use]
    pub const fn blitter_startup_ccks_remaining(&self) -> u8 {
        self.blitter_startup_ccks_remaining
    }

    #[must_use]
    fn delays_visible_blitter_busy(&self) -> bool {
        self.agnus_id < 0x2000 && self.original_revision == OriginalAgnusRevision::A1000
    }

    #[must_use]
    fn is_alice(&self) -> bool {
        self.agnus_id & 0x0F00 == 0x0300
    }

    /// Start a legacy-BLTSIZE blit and arm its incremental CCK scheduler.
    pub fn start_blit(&mut self) {
        // Legacy BLTSIZE encoding: V in bits 15-6 (0 → 1024 lines), H in
        // bits 5-0 (0 → 64 words).
        let height = match u32::from((self.bltsize >> 6) & 0x03FF) {
            0 => 1024,
            h => h,
        };
        let width_words = match u32::from(self.bltsize & 0x003F) {
            0 => 64,
            w => w,
        };
        self.start_blit_with_size(height, width_words);
    }

    /// Start a blit with an explicit height/width, bypassing the legacy
    /// 10+6-bit BLTSIZE decode. The ECS large-blit path (BLTSIZV at
    /// $05C, BLTSIZH at $05E) calls this to drive the engine from the
    /// full 15-bit height / 11-bit width without wrapping them back into
    /// the legacy field widths (#36). `height` is rows (area) / length
    /// (line); `width_words` is words per row.
    pub fn start_blit_with_size(&mut self, height: u32, width_words: u32) {
        self.blt_height = height;
        self.blt_width_words = width_words;
        self.blitter_busy = true;
        self.blitter_startup_ccks_remaining = 2;
        self.blitter_completion_phase = None;
        self.blitter_finish_emitted = false;
        self.blitter_dmacon_busy_hold_ccks = 0;
        self.blitter_copper_busy_hold_ccks = 0;
        self.blitter_exec_pending = true;
        self.init_incremental_blitter_runtime();
        self.init_blitter_word_state();
        self.blitter_ccks_remaining = self.count_total_blitter_ops();
    }

    /// Return the next DMA operation the blitter wants, without consuming it.
    ///
    /// The bus plan queries this to decide whether to grant the blitter a slot.
    /// Reads are offered first (A, B, C in priority order), then WriteD once
    /// all reads are done. For line mode, the strict ReadC→WriteD sequence is
    /// preserved.
    #[must_use]
    pub fn next_blitter_dma_request(&self) -> Option<BlitterDmaOp> {
        if !self.blitter_exec_pending || !self.blitter_busy {
            return None;
        }
        let ws = self.blitter_word_state.as_ref()?;
        if ws.internal_only {
            return if ws.internal_done {
                None
            } else {
                Some(BlitterDmaOp::Internal)
            };
        }
        if ws.need_a {
            return Some(BlitterDmaOp::ReadA);
        }
        if ws.need_b {
            return Some(BlitterDmaOp::ReadB);
        }
        if ws.need_c {
            return Some(BlitterDmaOp::ReadC);
        }
        if ws.reads_done && ws.need_d {
            return Some(BlitterDmaOp::WriteD);
        }
        None
    }

    /// Service one channel operation after the shared startup phase.
    ///
    /// Startup consumption belongs to [`Agnus::tick_blitter_scheduler_op`],
    /// keeping this low-level mutation operation-only.
    fn consume_blitter_dma_op(&mut self, op: BlitterDmaOp) {
        if let Some(ws) = &mut self.blitter_word_state {
            match op {
                BlitterDmaOp::ReadA => ws.need_a = false,
                BlitterDmaOp::ReadB => ws.need_b = false,
                BlitterDmaOp::ReadC => ws.need_c = false,
                BlitterDmaOp::WriteD => ws.need_d = false,
                BlitterDmaOp::Internal => ws.internal_done = true,
            }
            ws.reads_done = !ws.need_a && !ws.need_b && !ws.need_c;
        }
        self.blitter_ccks_remaining = self.blitter_ccks_remaining.saturating_sub(1);
    }

    /// Check if the current word's channel ops are all serviced.
    #[must_use]
    pub fn blitter_word_complete(&self) -> bool {
        self.blitter_word_state
            .as_ref()
            .is_some_and(|ws| ws.is_complete())
    }

    /// Return a side-effect-free diagnostic snapshot of every implemented
    /// blitter register, scheduler latch and incremental runtime field.
    #[must_use]
    pub fn blitter_diagnostic_snapshot(&self) -> AgnusBlitterDiagnosticSnapshot {
        let completion_phase = match self.blitter_completion_phase {
            None => AgnusBlitterCompletionDiagnosticPhase::None,
            Some(BlitterCompletionPhase::FinalResult) => {
                AgnusBlitterCompletionDiagnosticPhase::FinalResult
            }
            Some(BlitterCompletionPhase::FinalWrite { addr, value }) => {
                AgnusBlitterCompletionDiagnosticPhase::FinalWrite {
                    address: addr,
                    value,
                }
            }
        };
        let next_dma_request = self.next_blitter_dma_request();
        let next_progress_uses_bus = match completion_phase {
            AgnusBlitterCompletionDiagnosticPhase::FinalResult => false,
            AgnusBlitterCompletionDiagnosticPhase::FinalWrite { .. } => true,
            AgnusBlitterCompletionDiagnosticPhase::None => {
                next_dma_request.is_some()
                    && self.blitter_busy
                    && self.next_blitter_progress_uses_bus()
            }
        };

        AgnusBlitterDiagnosticSnapshot {
            registers: AgnusBlitterRegistersDiagnosticSnapshot {
                bltcon0: self.bltcon0,
                bltcon1: self.bltcon1,
                bltsize: self.bltsize,
                bltsizv_ecs: self.bltsizv_ecs,
                bltsizh_ecs: self.bltsizh_ecs,
                blt_apt: self.blt_apt,
                blt_bpt: self.blt_bpt,
                blt_cpt: self.blt_cpt,
                blt_dpt: self.blt_dpt,
                blt_amod: self.blt_amod,
                blt_bmod: self.blt_bmod,
                blt_cmod: self.blt_cmod,
                blt_dmod: self.blt_dmod,
                blt_adat: self.blt_adat,
                blt_bdat: self.blt_bdat,
                blt_cdat: self.blt_cdat,
                blt_afwm: self.blt_afwm,
                blt_alwm: self.blt_alwm,
            },
            execution: AgnusBlitterExecutionDiagnosticSnapshot {
                agnus_id: self.agnus_id,
                original_revision: self.original_revision,
                dmacon: self.dmacon,
                dma_enabled: self.dma_enabled(bits::DMACON_BLTEN),
                priority_enabled: self.dmacon & bits::DMACON_BLTPRI != 0,
                busy: self.blitter_busy,
                busy_visible: self.blitter_busy_visible(),
                busy_copper: self.blitter_busy_copper(),
                nasty_active: self.blitter_nasty_active(),
                startup_ccks_remaining: self.blitter_startup_ccks_remaining,
                completion_phase,
                completion_ccks_remaining: self.blitter_completion_ccks_remaining(),
                final_d_pending: self.blitter_final_d_pending(),
                finish_emitted: self.blitter_finish_emitted,
                dmacon_busy_hold_ccks: self.blitter_dmacon_busy_hold_ccks,
                copper_busy_hold_ccks: self.blitter_copper_busy_hold_ccks,
                exec_pending: self.blitter_exec_pending,
                exec_ready: self.blitter_exec_ready(),
                zero: self.blitter_dzero,
                height: self.blt_height,
                width_words: self.blt_width_words,
                ccks_remaining: self.blitter_ccks_remaining,
                next_dma_request,
                next_progress_uses_bus,
                word_complete: self.blitter_word_complete(),
                incremental_runtime_present: self.has_incremental_blitter_runtime(),
            },
            word: self
                .blitter_word_state
                .map(|word| AgnusBlitterWordDiagnosticSnapshot {
                    need_a: word.need_a,
                    need_b: word.need_b,
                    need_c: word.need_c,
                    need_d: word.need_d,
                    reads_done: word.reads_done,
                    internal_only: word.internal_only,
                    internal_done: word.internal_done,
                }),
            line: self
                .blitter_line_runtime
                .map(|line| AgnusBlitterLineDiagnosticSnapshot {
                    steps_remaining: line.steps_remaining,
                    error: line.error,
                    error_add: line.error_add,
                    error_sub: line.error_sub,
                    cpt: line.cpt,
                    dpt: line.dpt,
                    pixel_bit: line.pixel_bit,
                    row_mod: line.row_mod,
                    texture: line.texture,
                    texture_bit: line.texture_bit,
                    lf: line.lf,
                    sing: line.sing,
                    one_dot_drawn: line.one_dot_drawn,
                    major_is_y: line.major_is_y,
                    x_negative: line.x_neg,
                    y_negative: line.y_neg,
                    last_c_word: line.last_c_word,
                    have_c_word: line.have_c_word,
                }),
            area: self
                .blitter_area_runtime
                .map(|area| AgnusBlitterAreaDiagnosticSnapshot {
                    rows_remaining: area.rows_remaining,
                    width_words: area.width_words,
                    words_remaining_in_row: area.words_remaining_in_row,
                    use_a: area.use_a,
                    use_b: area.use_b,
                    use_c: area.use_c,
                    use_d: area.use_d,
                    lf: area.lf,
                    a_shift: area.a_shift,
                    b_shift: area.b_shift,
                    descending: area.desc,
                    pointer_step: area.ptr_step,
                    modulo_direction: area.mod_dir,
                    fill_enabled: area.fill_enabled,
                    inclusive_fill_enabled: area.ife,
                    exclusive_fill_enabled: area.efe,
                    fill_carry_initial: area.fill_carry_init,
                    fill_carry: area.fill_carry,
                    apt: area.apt,
                    bpt: area.bpt,
                    cpt: area.cpt,
                    dpt: area.dpt,
                    amod: area.amod,
                    bmod: area.bmod,
                    cmod: area.cmod,
                    dmod: area.dmod,
                    a_previous: area.a_prev,
                    b_previous: area.b_prev,
                    a_raw: area.a_raw,
                    b_raw: area.b_raw,
                    c_value: area.c_val,
                }),
        }
    }

    /// Advance to the next word after the current word completed.
    ///
    /// For area mode: decrements `words_remaining_in_row`, handles row
    /// transitions. For line mode: decrements `steps_remaining`.
    /// Returns `true` when the entire blit is finished.
    pub fn advance_blitter_word(&mut self) -> bool {
        if self.blitter_line_runtime.is_some() {
            // Line mode: word state is re-initialized per step in
            // execute_incremental_blitter_op when WriteD completes and
            // steps_remaining > 0. If line_runtime is None after that call,
            // the blit is done.
            return self.blitter_line_runtime.is_none();
        }

        if let Some(area) = &self.blitter_area_runtime {
            if area.words_remaining_in_row == 0 && area.rows_remaining == 0 {
                // Blit finished — will be cleaned up by caller.
                self.blitter_word_state = None;
                self.blitter_exec_pending = false;
                return true;
            }
            // Set up next word state from area runtime channel enables.
            self.blitter_word_state = Some(BlitterWordState::new_area(
                area.use_a, area.use_b, area.use_c, area.use_d,
            ));
            return false;
        }

        // No runtime — blit must have completed.
        self.blitter_word_state = None;
        self.blitter_exec_pending = false;
        true
    }

    /// Consume one accepted/free blitter progress CCK.
    ///
    /// The first two accepted CCKs report [`BlitterProgress::Startup`] and do
    /// not service the pending operation. The first of those CCKs reloads
    /// BZERO. Once startup is drained, the caller is responsible for executing
    /// each returned operation against the incremental runtime and calling
    /// [`Agnus::advance_blitter_word`] when the word completes.
    pub fn tick_blitter_scheduler_op(&mut self, progress_this_cck: bool) -> BlitterProgress {
        if !progress_this_cck {
            return BlitterProgress::NoProgress;
        }
        let Some(op) = self.next_blitter_dma_request() else {
            return BlitterProgress::NoProgress;
        };
        if self.blitter_startup_ccks_remaining != 0 {
            self.blitter_startup_ccks_remaining -= 1;
            if self.blitter_startup_ccks_remaining == 1 {
                self.blitter_dzero = true;
            }
            return BlitterProgress::Startup;
        }
        self.consume_blitter_dma_op(op);
        BlitterProgress::Operation(op)
    }

    /// Clear all scheduler and internal activity state.
    pub fn clear_blitter_scheduler(&mut self) {
        self.blitter_word_state = None;
        self.blitter_busy = false;
        self.blitter_startup_ccks_remaining = 0;
        self.blitter_completion_phase = None;
        self.blitter_finish_emitted = false;
        self.blitter_dmacon_busy_hold_ccks = 0;
        self.blitter_copper_busy_hold_ccks = 0;
        self.blitter_exec_pending = false;
        self.blitter_ccks_remaining = 0;
        self.blitter_line_runtime = None;
        self.blitter_area_runtime = None;
        self.blitter_bus_used_this_cck = false;
        self.blitter_nasty_owned_this_cck = false;
        self.blitter_cck_bus_state_recorded = false;
    }

    #[must_use]
    pub fn blitter_exec_ready(&self) -> bool {
        self.blitter_busy
            && self.blitter_completion_phase.is_none()
            && !self.blitter_exec_pending
            && self.blitter_line_runtime.is_none()
            && self.blitter_area_runtime.is_none()
    }

    #[must_use]
    pub fn has_incremental_blitter_runtime(&self) -> bool {
        self.blitter_line_runtime.is_some() || self.blitter_area_runtime.is_some()
    }

    /// Execute one queued blitter DMA timing op against the incremental runtime.
    ///
    /// Returns `true` when the incremental blit completed on this op.
    pub fn execute_incremental_blitter_op<FRead, FWrite>(
        &mut self,
        op: BlitterDmaOp,
        read_word: FRead,
        write_word: FWrite,
    ) -> bool
    where
        FRead: FnOnce(u32) -> u16,
        FWrite: FnOnce(u32, u16),
    {
        if let Some(mut line) = self.blitter_line_runtime {
            return match op {
                BlitterDmaOp::ReadC => {
                    let c_val = read_word(line.cpt);
                    self.blt_cdat = c_val;
                    line.last_c_word = c_val;
                    line.have_c_word = true;
                    self.blitter_line_runtime = Some(line);
                    false
                }
                BlitterDmaOp::WriteD => {
                    let c_val = if line.have_c_word {
                        line.last_c_word
                    } else {
                        // Defensive fallback; queue should always present ReadC first.
                        let c_val = read_word(line.cpt);
                        self.blt_cdat = c_val;
                        c_val
                    };

                    let pixel_mask: u16 = 0x8000 >> line.pixel_bit;
                    let a_val = pixel_mask;
                    let b_val = if line.texture & (1 << line.texture_bit) != 0 {
                        0xFFFF
                    } else {
                        0x0000
                    };

                    let mut result: u16 = 0;
                    for bit in 0..16u16 {
                        let a_bit = (a_val >> bit) & 1;
                        let b_bit = (b_val >> bit) & 1;
                        let c_bit = (c_val >> bit) & 1;
                        let index = (a_bit << 2) | (b_bit << 1) | c_bit;
                        if (line.lf >> index) & 1 != 0 {
                            result |= 1 << bit;
                        }
                    }
                    if result != 0 {
                        self.blitter_dzero = false; // BZERO: a non-zero D word
                    }
                    let write_d = !line.sing || !line.one_dot_drawn;
                    if write_d {
                        write_word(line.dpt, result);
                    }

                    line.texture_bit = line.texture_bit.wrapping_sub(1) & 0x0F;

                    let step_x = |line: &mut BlitterLineRuntime| {
                        if line.x_neg {
                            line.pixel_bit = line.pixel_bit.wrapping_sub(1) & 0xF;
                            if line.pixel_bit == 15 {
                                line.cpt = line.cpt.wrapping_sub(2);
                                line.dpt = line.dpt.wrapping_sub(2);
                            }
                        } else {
                            line.pixel_bit = (line.pixel_bit + 1) & 0xF;
                            if line.pixel_bit == 0 {
                                line.cpt = line.cpt.wrapping_add(2);
                                line.dpt = line.dpt.wrapping_add(2);
                            }
                        }
                    };
                    let step_y = |line: &mut BlitterLineRuntime| {
                        if line.y_neg {
                            line.cpt = (line.cpt as i32 + line.row_mod as i32) as u32;
                            line.dpt = (line.dpt as i32 + line.row_mod as i32) as u32;
                        } else {
                            line.cpt = (line.cpt as i32 - line.row_mod as i32) as u32;
                            line.dpt = (line.dpt as i32 - line.row_mod as i32) as u32;
                        }
                    };

                    let moved_y = if line.error >= 0 {
                        if line.major_is_y {
                            step_y(&mut line);
                            step_x(&mut line);
                        } else {
                            step_x(&mut line);
                            step_y(&mut line);
                        }
                        line.error = line.error.wrapping_add(line.error_sub);
                        true
                    } else {
                        if line.major_is_y {
                            step_y(&mut line);
                        } else {
                            step_x(&mut line);
                        }
                        line.error = line.error.wrapping_add(line.error_add);
                        line.major_is_y
                    };

                    if line.sing {
                        // ONEDOT permits the first D transfer in each
                        // horizontal row. A Y transition during this step
                        // arms the next row; otherwise this row remains
                        // suppressed.
                        line.one_dot_drawn = !moved_y;
                    }

                    line.have_c_word = false;
                    line.steps_remaining = line.steps_remaining.saturating_sub(1);
                    if line.steps_remaining == 0 {
                        self.blt_apt = line.error as u16 as u32;
                        self.blt_cpt = line.cpt;
                        self.blt_dpt = line.dpt;
                        self.blt_bdat = line.texture;
                        self.bltcon1 =
                            (self.bltcon1 & 0x0FFF) | (u16::from(line.texture_bit) << 12);
                        self.blitter_line_runtime = None;
                        self.blitter_word_state = None;
                        self.blitter_exec_pending = false;
                        true
                    } else {
                        self.blitter_line_runtime = Some(line);
                        // Reset word state for next line step.
                        self.blitter_word_state = Some(BlitterWordState::new_line());
                        false
                    }
                }
                BlitterDmaOp::ReadA | BlitterDmaOp::ReadB | BlitterDmaOp::Internal => {
                    self.blitter_line_runtime = Some(line);
                    false
                }
            };
        }

        let Some(mut area) = self.blitter_area_runtime else {
            return false;
        };

        match op {
            BlitterDmaOp::ReadA => {
                let w = read_word(area.apt);
                area.apt = (area.apt as i32 + area.ptr_step) as u32;
                self.blt_adat = w;
                area.a_raw = w;
            }
            BlitterDmaOp::ReadB => {
                let w = read_word(area.bpt);
                area.bpt = (area.bpt as i32 + area.ptr_step) as u32;
                self.blt_bdat = w;
                area.b_raw = w;
            }
            BlitterDmaOp::ReadC => {
                let w = read_word(area.cpt);
                area.cpt = (area.cpt as i32 + area.ptr_step) as u32;
                self.blt_cdat = w;
                area.c_val = w;
            }
            BlitterDmaOp::WriteD | BlitterDmaOp::Internal => {}
        }

        // Word processing happens when all channel ops are complete.
        if !self.blitter_word_complete() {
            self.blitter_area_runtime = Some(area);
            return false;
        }

        let current_col = area.width_words - area.words_remaining_in_row;
        let mut a_masked = area.a_raw;
        if current_col == 0 {
            a_masked &= self.blt_afwm;
        }
        if area.words_remaining_in_row == 1 {
            a_masked &= self.blt_alwm;
        }

        let a_combined = if area.desc {
            (u32::from(a_masked) << 16) | u32::from(area.a_prev)
        } else {
            (u32::from(area.a_prev) << 16) | u32::from(a_masked)
        };
        let a_shifted = if area.desc {
            (a_combined >> (16 - area.a_shift)) as u16
        } else {
            (a_combined >> area.a_shift) as u16
        };

        let b_combined = if area.desc {
            (u32::from(area.b_raw) << 16) | u32::from(area.b_prev)
        } else {
            (u32::from(area.b_prev) << 16) | u32::from(area.b_raw)
        };
        let b_shifted = if area.desc {
            (b_combined >> (16 - area.b_shift)) as u16
        } else {
            (b_combined >> area.b_shift) as u16
        };

        area.a_prev = a_masked;
        area.b_prev = area.b_raw;

        let mut result: u16 = 0;
        for bit in 0..16u16 {
            let a_bit = (a_shifted >> bit) & 1;
            let b_bit = (b_shifted >> bit) & 1;
            let c_bit = (area.c_val >> bit) & 1;
            let index = (a_bit << 2) | (b_bit << 1) | c_bit;
            if (area.lf >> index) & 1 != 0 {
                result |= 1 << bit;
            }
        }

        if area.fill_enabled {
            let mut filled: u16 = 0;
            for bit in 0..16u16 {
                let d_bit = (result >> bit) & 1;
                area.fill_carry ^= d_bit;
                let out = if area.efe {
                    area.fill_carry ^ d_bit
                } else if area.ife {
                    area.fill_carry
                } else {
                    d_bit
                };
                filled |= out << bit;
            }
            result = filled;
        }

        // BZERO observes every generated destination result, whether or not
        // D DMA is enabled. USED controls the memory transfer, not the
        // minterm/zero-detection path.
        if result != 0 {
            self.blitter_dzero = false;
        }
        if area.use_d {
            write_word(area.dpt, result);
            area.dpt = (area.dpt as i32 + area.ptr_step) as u32;
        }

        area.words_remaining_in_row = area.words_remaining_in_row.saturating_sub(1);
        if area.words_remaining_in_row == 0 {
            if area.use_a {
                area.apt = (area.apt as i32 + i32::from(area.amod) * area.mod_dir) as u32;
            }
            if area.use_b {
                area.bpt = (area.bpt as i32 + i32::from(area.bmod) * area.mod_dir) as u32;
            }
            if area.use_c {
                area.cpt = (area.cpt as i32 + i32::from(area.cmod) * area.mod_dir) as u32;
            }
            if area.use_d {
                area.dpt = (area.dpt as i32 + i32::from(area.dmod) * area.mod_dir) as u32;
            }

            area.rows_remaining = area.rows_remaining.saturating_sub(1);
            if area.rows_remaining == 0 {
                self.blt_apt = area.apt;
                self.blt_bpt = area.bpt;
                self.blt_cpt = area.cpt;
                self.blt_dpt = area.dpt;
                self.blitter_area_runtime = None;
                self.blitter_word_state = None;
                self.blitter_exec_pending = false;
                return true;
            }

            area.words_remaining_in_row = area.width_words;
            area.fill_carry = area.fill_carry_init;
        }

        // Reset word state for the next word.
        self.blitter_word_state = Some(BlitterWordState::new_area(
            area.use_a, area.use_b, area.use_c, area.use_d,
        ));
        self.blitter_area_runtime = Some(area);
        false
    }

    /// Initialise the per-word state machine for the first word of the blit.
    fn init_blitter_word_state(&mut self) {
        if (self.bltcon1 & 0x0001) != 0 {
            // LINE mode: strict ReadC → WriteD per pixel step.
            self.blitter_word_state = Some(BlitterWordState::new_line());
            return;
        }

        let use_a = (self.bltcon0 & 0x0800) != 0;
        let use_b = (self.bltcon0 & 0x0400) != 0;
        let use_c = (self.bltcon0 & 0x0200) != 0;
        let use_d = (self.bltcon0 & 0x0100) != 0;
        self.blitter_word_state = Some(BlitterWordState::new_area(use_a, use_b, use_c, use_d));
    }

    /// Count the total blitter DMA ops for the entire blit (for BLTBUSY timing).
    fn count_total_blitter_ops(&self) -> u32 {
        // Effective size set by start_blit / start_blit_with_size — full
        // width for ECS large blits, legacy-decoded otherwise (#36).
        let height = self.blt_height;
        let width_words = self.blt_width_words;

        if (self.bltcon1 & 0x0001) != 0 {
            // LINE mode: 2 ops per step (ReadC + WriteD).
            return height * 2;
        }

        let use_a = (self.bltcon0 & 0x0800) != 0;
        let use_b = (self.bltcon0 & 0x0400) != 0;
        let use_c = (self.bltcon0 & 0x0200) != 0;
        let use_d = (self.bltcon0 & 0x0100) != 0;
        let ops_per_word =
            u32::from(use_a) + u32::from(use_b) + u32::from(use_c) + u32::from(use_d);

        // When no external channels are enabled, each word takes one Internal op.
        let ops_per_word = ops_per_word.max(1);
        height * width_words * ops_per_word
    }

    fn init_incremental_blitter_runtime(&mut self) {
        self.blitter_line_runtime = None;
        self.blitter_area_runtime = None;
        if (self.bltcon1 & 0x0001) == 0 {
            // Effective size from start_blit_with_size — full width for
            // ECS large blits, legacy-decoded otherwise (#36).
            let height = self.blt_height;
            let width_words = self.blt_width_words;
            let use_a = (self.bltcon0 & 0x0800) != 0;
            let use_b = (self.bltcon0 & 0x0400) != 0;
            let use_c = (self.bltcon0 & 0x0200) != 0;
            let use_d = (self.bltcon0 & 0x0100) != 0;
            let desc = (self.bltcon1 & 0x0002) != 0;
            let fci = (self.bltcon1 & 0x0004) != 0;
            let ife = (self.bltcon1 & 0x0008) != 0;
            let efe = (self.bltcon1 & 0x0010) != 0;
            let fill_enabled = ife || efe;
            self.blitter_area_runtime = Some(BlitterAreaRuntime {
                rows_remaining: height,
                width_words,
                words_remaining_in_row: width_words,
                use_a,
                use_b,
                use_c,
                use_d,
                lf: self.bltcon0 as u8,
                a_shift: (self.bltcon0 >> 12) & 0xF,
                b_shift: (self.bltcon1 >> 12) & 0xF,
                desc,
                ptr_step: if desc { -2 } else { 2 },
                mod_dir: if desc { -1 } else { 1 },
                fill_enabled,
                ife,
                efe,
                fill_carry_init: if fci { 1 } else { 0 },
                fill_carry: if fci { 1 } else { 0 },
                apt: self.blt_apt,
                bpt: self.blt_bpt,
                cpt: self.blt_cpt,
                dpt: self.blt_dpt,
                amod: self.blt_amod,
                bmod: self.blt_bmod,
                cmod: self.blt_cmod,
                dmod: self.blt_dmod,
                a_prev: 0,
                b_prev: 0,
                a_raw: self.blt_adat,
                b_raw: self.blt_bdat,
                c_val: self.blt_cdat,
            });
            return;
        }

        // Line mode: length is the effective height field (legacy
        // BLTSIZE only — ECS BLTSIZV/H drive area blits).
        let length = self.blt_height;
        let ash = (self.bltcon0 >> 12) & 0xF;
        let lf = self.bltcon0 as u8;
        let texture_bit = (self.bltcon1 >> 12) as u8;
        let sud = self.bltcon1 & 0x0010 != 0;
        let sul = self.bltcon1 & 0x0008 != 0;
        let aul = self.bltcon1 & 0x0004 != 0;
        let sing = self.bltcon1 & 0x0002 != 0;
        let oct_code = ((sud as u8) << 2) | ((sul as u8) << 1) | (aul as u8);
        let octant = match oct_code {
            0b000 => 6,
            0b001 => 1,
            0b010 => 5,
            0b011 => 2,
            0b100 => 7,
            0b101 => 4,
            0b110 => 0,
            0b111 => 3,
            _ => unreachable!(),
        };
        let (major_is_y, x_neg, y_neg) = match octant {
            0 => (false, false, false),
            1 => (true, false, false),
            2 => (true, true, false),
            3 => (false, true, false),
            4 => (false, true, true),
            5 => (true, true, true),
            6 => (true, false, true),
            7 => (false, false, true),
            _ => unreachable!(),
        };

        self.blitter_line_runtime = Some(BlitterLineRuntime {
            steps_remaining: length,
            error: self.blt_apt as i16,
            error_add: self.blt_bmod,
            error_sub: self.blt_amod,
            cpt: self.blt_cpt,
            dpt: self.blt_dpt,
            pixel_bit: ash,
            row_mod: self.blt_cmod,
            texture: self.blt_bdat,
            texture_bit,
            lf,
            sing,
            one_dot_drawn: false,
            major_is_y,
            x_neg,
            y_neg,
            last_c_word: 0,
            have_c_word: false,
        });
    }

    /// Tick one CCK (8 crystal ticks).
    pub fn tick_cck(&mut self) {
        self.tick_cck_with_timing(self.current_line_ccks(), self.lines_per_frame);
    }

    /// Tick one CCK using the selected line and short-field totals.
    ///
    /// `line_ccks` is the number of horizontal counter positions in one
    /// line. `short_field_lines` is the non-interlaced or interlaced
    /// short-field length; a long interlaced field adds one line. Keeping
    /// the boundary lifecycle here lets ECS/AGA programmable totals select
    /// different counter limits without bypassing LOF/LOL, VBL accounting,
    /// or per-line sprite DMA state.
    ///
    /// This method is public only so chipset wrapper crates can reuse the
    /// OCS boundary lifecycle. Callers must pass totals in `1..u16::MAX`.
    #[doc(hidden)]
    pub fn tick_cck_with_timing(&mut self, line_ccks: u16, short_field_lines: u16) {
        let sprite_timing = self.fixed_sprite_dma_vertical_timing();
        self.tick_cck_with_timing_and_sprite_vertical_timing(
            line_ccks,
            short_field_lines,
            sprite_timing,
        );
    }

    /// Return the raster line that the next CCK enters, or `None` when
    /// the next CCK remains on the current line.
    ///
    /// Chipset wrappers use this to advance edge-driven extension state
    /// before the shared boundary lifecycle evaluates the new line.
    #[doc(hidden)]
    #[must_use]
    pub fn next_cck_line_entry(&self, line_ccks: u16, short_field_lines: u16) -> Option<u16> {
        debug_assert!(
            line_ccks > 0 && line_ccks < u16::MAX,
            "line total must fit the beam counter"
        );
        debug_assert!(
            short_field_lines > 0 && short_field_lines < u16::MAX,
            "field total must leave room for the interlace extension"
        );
        if self.hpos + 1 < line_ccks {
            return None;
        }

        let interlace = (self.bplcon0 & 0x0004) != 0;
        let frame_lines = if interlace && self.lof {
            short_field_lines + 1
        } else {
            short_field_lines
        };
        let next_vpos = self.vpos + 1;
        Some(if next_vpos >= frame_lines {
            0
        } else {
            next_vpos
        })
    }

    /// Tick one CCK with variant-selected beam totals and sprite vertical
    /// timing. ECS/AGA use this seam so programmable blanking reaches the
    /// same lifecycle that advances the shared OCS beam counters.
    #[doc(hidden)]
    pub fn tick_cck_with_timing_and_sprite_vertical_timing(
        &mut self,
        line_ccks: u16,
        short_field_lines: u16,
        sprite_timing: SpriteDmaVerticalTiming,
    ) {
        self.tick_cck_with_variant_timing(
            line_ccks,
            short_field_lines,
            sprite_timing,
            self.agnus_id < 0x2000,
        );
    }

    /// Tick one CCK with wrapper-selected timing and fixed-DDF policy.
    ///
    /// Enhanced-chipset wrappers use this seam to disable the fixed
    /// right-hand DDF boundary without storing a derived policy in the
    /// serializable OCS core.
    #[doc(hidden)]
    pub fn tick_cck_with_variant_timing(
        &mut self,
        line_ccks: u16,
        short_field_lines: u16,
        sprite_timing: SpriteDmaVerticalTiming,
        fixed_ddf_right_stop_enabled: bool,
    ) {
        debug_assert!(
            line_ccks > 0 && line_ccks < u16::MAX,
            "line total must fit the beam counter"
        );
        debug_assert!(
            short_field_lines > 0 && short_field_lines < u16::MAX,
            "field total must leave room for the interlace extension"
        );
        self.blitter_dmacon_busy_hold_ccks = self.blitter_dmacon_busy_hold_ccks.saturating_sub(1);
        self.blitter_copper_busy_hold_ccks = self.blitter_copper_busy_hold_ccks.saturating_sub(1);
        self.hpos += 1;
        if self.hpos >= line_ccks {
            // A phase-shifted OCS fetch unit can have its logical terminal
            // endpoint at $E3 even when a short physical line ends at $E2.
            // WinUAE and vAmiga agree that the old run prevents a fresh $00
            // start and that the hard-start permission is closed before the
            // next legal $04 comparator. They do not establish one shared
            // externally visible bus cell for the tail. Consume only that
            // proven start-admission result before discarding line-local DDF
            // state. No bus slot or pointer service is synthesized for the
            // old terminal tail.
            if self.agnus_id < 0x2000 && self.ddf_fetch_end == Some(line_ccks) {
                self.ocs_ddf_hard_start_open = false;
            }
            self.hpos = 0;
            self.ddf_start_match = None;
            self.ddf_stop_match = None;
            self.ddf_fetch_end = None;
            self.ocs_ddf_run_aborted = false;
            // End-of-line: toggle the long-line flipflop on regions
            // that alternate (NTSC default). PAL has lol_toggle=false
            // so the flipflop stays at 0 (every line is 227).
            if self.lol_toggle {
                self.lol = !self.lol;
            } else {
                self.lol = false;
            }
            self.vpos += 1;
            // Interlace: long frame has one extra line (313 PAL, 263 NTSC).
            let interlace = (self.bplcon0 & 0x0004) != 0;
            let frame_lines = if interlace && self.lof {
                short_field_lines + 1
            } else {
                short_field_lines
            };
            let field_wrapped = self.vpos >= frame_lines;
            if field_wrapped {
                self.vpos = 0;
                self.vbl_count += 1;
                if interlace {
                    self.lof = !self.lof;
                }
            }
            // Original Agnus carries a hidden vertical display-window
            // flip-flop. The installed revision's hard vertical-blank close
            // is a force-off level for its physical line and takes
            // precedence over a coincident VSTART. Evaluate the new line
            // before its first horizontal DDF comparator so VSTART admits,
            // and either VSTOP or hard blank rejects, a same-line fetch
            // start.
            self.ocs_hard_vertical_blank_active =
                self.original_hard_vertical_blank_active(self.vpos, frame_lines);
            self.evaluate_ocs_vertical_diw_comparators(self.vpos);
            // New display line: run the per-line sprite-DMA update
            // (VSTART activation, VSTOP deactivation, top-of-frame
            // control-refetch priming). gap #162.
            self.update_sprite_dma_with_vertical_timing(frame_lines, sprite_timing);
        }
        self.evaluate_ddf_comparators(fixed_ddf_right_stop_enabled);
    }

    /// Per-line sprite-DMA update — run once as the beam enters each new
    /// display line. Outside vertical blank it activates a sprite when
    /// the beam reaches VSTART and deactivates it at VSTOP. Comparator
    /// state evolves independently of SPREN; that bit gates the resulting
    /// bus request in slot arbitration.
    #[cfg(test)]
    fn update_sprite_dma(&mut self, frame_lines: u16) {
        let sprite_timing = self.fixed_sprite_dma_vertical_timing();
        self.update_sprite_dma_with_vertical_timing(frame_lines, sprite_timing);
    }

    fn update_sprite_dma_with_vertical_timing(
        &mut self,
        frame_lines: u16,
        sprite_timing: SpriteDmaVerticalTiming,
    ) {
        let v = self.vpos;
        if sprite_timing.clears_on_field_end() && v + 1 >= frame_lines {
            for s in 0..8 {
                self.spr_dma_on[s] = false;
            }
            return;
        }
        for s in 0..8 {
            self.update_sprite_dma_comparator_with_vertical_timing(s, sprite_timing);
        }
    }

    /// Fixed vertical-blank end and sprite control-refetch boundary:
    /// line 25 on PAL and line 20 on NTSC. Ordinary sprite data activity
    /// starts on the following line.
    #[must_use]
    pub const fn fixed_vblank_end_line(&self) -> u16 {
        match self.region {
            AgnusRegion::Pal => PAL_VBL_END_LINE,
            AgnusRegion::Ntsc => NTSC_VBL_END_LINE,
        }
    }

    const fn fixed_sprite_dma_vertical_timing(&self) -> SpriteDmaVerticalTiming {
        let timing = SpriteDmaVerticalTiming::fixed(self.fixed_vblank_end_line());
        if self.agnus_id >= 0x2000 {
            timing.with_ten_bit_sprite_comparators()
        } else {
            timing
        }
    }

    fn update_sprite_dma_comparator_with_vertical_timing(
        &mut self,
        channel: usize,
        sprite_timing: SpriteDmaVerticalTiming,
    ) {
        if channel >= self.spr_dma_on.len() || sprite_timing.blank_active(self.vpos) {
            return;
        }
        if sprite_timing.reset_event(self.vpos) || self.vpos == self.spr_vstop[channel] {
            self.spr_dma_on[channel] = false;
        } else if self.vpos == self.spr_vstart[channel] {
            self.spr_dma_on[channel] = true;
        }
    }

    fn sprite_control_fetch_due_with_vertical_timing(
        &self,
        channel: usize,
        sprite_timing: SpriteDmaVerticalTiming,
    ) -> bool {
        channel < self.spr_vstop.len()
            && (sprite_timing.reset_event(self.vpos) || self.vpos == self.spr_vstop[channel])
    }

    /// Whether `channel` has a control- or data-fetch request on the
    /// current line. SPREN is deliberately handled by slot arbitration;
    /// this predicate describes the per-channel request state shared by
    /// planning and service.
    fn sprite_dma_cycle_requested_with_vertical_timing(
        &self,
        channel: usize,
        sprite_timing: SpriteDmaVerticalTiming,
    ) -> bool {
        channel < self.spr_dma_on.len()
            && !sprite_timing.blank_active(self.vpos)
            && (self.sprite_control_fetch_due_with_vertical_timing(channel, sprite_timing)
                || self.spr_dma_on[channel])
    }

    /// Start a new CCK with no recorded sprite bus use.
    pub fn reset_sprite_bus_usage(&mut self) {
        self.sprite_bus_used_this_cck = false;
    }

    /// Whether a sprite fetch has driven the chip bus during this CCK.
    #[must_use]
    pub fn sprite_bus_used_this_cck(&self) -> bool {
        self.sprite_bus_used_this_cck
    }

    /// Record the blitter's pre-service nasty ownership and actual bus use
    /// for this CCK.
    ///
    /// The driver records both after the operation is known. Completion or a
    /// bus-free ONEDOT would-be write can otherwise change the live plan
    /// before a later same-CCK CPU arbitration query.
    pub fn record_blitter_cck_bus_state(&mut self, nasty_owned: bool, bus_used: bool) {
        self.blitter_nasty_owned_this_cck = nasty_owned;
        self.blitter_bus_used_this_cck = bus_used;
        self.blitter_cck_bus_state_recorded = true;
    }

    /// Start a CCK without a recorded blitter arbitration outcome.
    ///
    /// The driver replaces this empty state after the phase-0 blitter step.
    /// Until then, a direct CPU bus-service call must use the live plan.
    pub fn reset_blitter_cck_bus_state(&mut self) {
        self.blitter_bus_used_this_cck = false;
        self.blitter_nasty_owned_this_cck = false;
        self.blitter_cck_bus_state_recorded = false;
    }

    /// Whether a blitter transfer drove the chip bus during this CCK.
    #[must_use]
    pub const fn blitter_bus_used_this_cck(&self) -> bool {
        self.blitter_bus_used_this_cck
    }

    /// Whether the current CCK's pre-service plan assigned its CPU/free cell
    /// to the nasty blitter.
    #[must_use]
    pub const fn blitter_nasty_owned_this_cck(&self) -> bool {
        self.blitter_nasty_owned_this_cck
    }

    /// Whether the phase-0 DMA body has recorded blitter ownership for this
    /// CCK.
    #[must_use]
    pub const fn blitter_cck_bus_state_recorded(&self) -> bool {
        self.blitter_cck_bus_state_recorded
    }

    /// Service one sprite-DMA bus cycle for `channel` (gap #162). Called
    /// by the machine when Agnus has granted this CCK's slot to a sprite;
    /// `second_word` selects the second of the channel's two per-line
    /// fetches. Returns `(is_control, word)` when a fetch occurs — the
    /// machine routes the word to Denise's SPRxPOS/CTL (control) or
    /// SPRxDATA/DATB (data) — or `None` when the sprite is idle.
    ///
    /// The control-vs-data decision — VSTOP wins over the DMA-on flag —
    /// and the VSTART/VSTOP latch follow vAmiga's `executeFirst/Second
    /// SpriteCycle` (Agnus.cpp:559-641). The pointer advances one word
    /// per fetched word; there is no automatic reload (the copper/CPU
    /// rewrite SPRxPT each frame).
    pub fn service_sprite_dma_cyc(
        &mut self,
        channel: usize,
        second_word: bool,
        width: u8,
        read: impl FnMut(u32) -> u16,
    ) -> Option<(bool, u64)> {
        let sprite_timing = self.fixed_sprite_dma_vertical_timing();
        self.service_sprite_dma_cyc_with_vertical_timing(
            channel,
            second_word,
            width,
            sprite_timing,
            read,
        )
    }

    /// Variant-aware form of [`Self::service_sprite_dma_cyc`].
    #[doc(hidden)]
    pub fn service_sprite_dma_cyc_with_vertical_timing(
        &mut self,
        channel: usize,
        second_word: bool,
        width: u8,
        sprite_timing: SpriteDmaVerticalTiming,
        mut read: impl FnMut(u32) -> u16,
    ) -> Option<(bool, u64)> {
        // Sprite DMA is suppressed during vertical blank, and an idle
        // channel does not consume its scheduled bus opportunity.
        if !self.sprite_dma_cycle_requested_with_vertical_timing(channel, sprite_timing) {
            return None;
        }
        self.sprite_bus_used_this_cck = true;
        if self.sprite_control_fetch_due_with_vertical_timing(channel, sprite_timing) {
            // Control fetch (SPRxPOS / SPRxCTL): always a single word —
            // FMODE widens the data fetch, not the control words.
            self.spr_dma_on[channel] = false;
            let word = read(self.spr_pt[channel]);
            self.spr_pt[channel] = self.spr_pt[channel].wrapping_add(2);
            if second_word {
                self.latch_sprite_ctl_with_vertical_timing(channel, word, sprite_timing);
            } else {
                self.latch_sprite_pos_with_vertical_timing(channel, word, sprite_timing);
            }
            Some((true, u64::from(word)))
        } else if self.spr_dma_on[channel] {
            // Data fetch (SPRxDATA / SPRxDATB): FMODE makes the access
            // fetch `width` (1/2/4) consecutive words, assembled MSB-first
            // into the 64-bit serial-shifter payload — the first word
            // holds the leftmost pixels (#99). width 1 keeps OCS/ECS at
            // the historical single-word, low-16-bit behaviour.
            let mut data = 0u64;
            for _ in 0..width.max(1) {
                let word = read(self.spr_pt[channel]);
                self.spr_pt[channel] = self.spr_pt[channel].wrapping_add(2);
                data = (data << 16) | u64::from(word);
            }
            Some((false, data))
        } else {
            None
        }
    }

    /// Apply a direct (CPU/copper) write to `SPRxPOS` — update the
    /// vertical-start comparator just as a DMA control-word fetch does.
    /// On real Agnus the SPRxPOS/CTL registers ARE the comparators;
    /// every write (DMA or register) updates them. Without this, a
    /// program that positions a DMA sprite by writing the registers
    /// directly (e.g. Blitz `ShowSprite`, which leaves the chip-RAM
    /// control words zero) leaves VSTART/VSTOP at 0 and the sprite never
    /// displays. Mirrors vAmiga `Agnus::setSPRxPOS` (AgnusRegs.cpp:462).
    pub fn poke_sprite_pos(&mut self, channel: usize, val: u16) {
        let sprite_timing = self.fixed_sprite_dma_vertical_timing();
        self.poke_sprite_pos_with_vertical_timing(channel, val, sprite_timing);
    }

    /// Variant-aware form of [`Self::poke_sprite_pos`].
    #[doc(hidden)]
    pub fn poke_sprite_pos_with_vertical_timing(
        &mut self,
        channel: usize,
        val: u16,
        sprite_timing: SpriteDmaVerticalTiming,
    ) {
        if channel < 8 {
            self.latch_sprite_pos_with_vertical_timing(channel, val, sprite_timing);
        }
    }

    /// Apply a direct (CPU/copper) write to `SPRxCTL` — update the
    /// `VSTART[8]`/VSTOP comparators. See [`Self::poke_sprite_pos`].
    /// Mirrors vAmiga `Agnus::setSPRxCTL` (AgnusRegs.cpp:501).
    pub fn poke_sprite_ctl(&mut self, channel: usize, val: u16) {
        let sprite_timing = self.fixed_sprite_dma_vertical_timing();
        self.poke_sprite_ctl_with_vertical_timing(channel, val, sprite_timing);
    }

    /// Variant-aware form of [`Self::poke_sprite_ctl`].
    #[doc(hidden)]
    pub fn poke_sprite_ctl_with_vertical_timing(
        &mut self,
        channel: usize,
        val: u16,
        sprite_timing: SpriteDmaVerticalTiming,
    ) {
        if channel < 8 {
            self.latch_sprite_ctl_with_vertical_timing(channel, val, sprite_timing);
        }
    }

    /// Latch VSTART low 8 bits from a fetched SPRxPOS word (bits 15-8).
    fn latch_sprite_pos_with_vertical_timing(
        &mut self,
        channel: usize,
        pos: u16,
        sprite_timing: SpriteDmaVerticalTiming,
    ) {
        self.spr_vstart[channel] =
            (self.spr_vstart[channel] & sprite_timing.sprite_vertical_high_mask()) | (pos >> 8);
        self.update_sprite_dma_comparator_with_vertical_timing(channel, sprite_timing);
    }

    /// Latch VSTOP (CTL bits 15-8) plus VSTART[8] (CTL bit 2) and
    /// VSTOP[8] (CTL bit 1) from a fetched SPRxCTL word. Enhanced Agnus
    /// also maps CTL bits 6/5 to VSTART[9]/VSTOP[9].
    fn latch_sprite_ctl_with_vertical_timing(
        &mut self,
        channel: usize,
        ctl: u16,
        sprite_timing: SpriteDmaVerticalTiming,
    ) {
        let enhanced_high_bit = sprite_timing.sprite_vertical_high_mask() & 0x0200;
        self.spr_vstart[channel] = (self.spr_vstart[channel] & 0x00FF)
            | ((ctl & 0x0004) << 6)
            | (((ctl & 0x0040) << 3) & enhanced_high_bit);
        self.spr_vstop[channel] =
            (ctl >> 8) | ((ctl & 0x0002) << 7) | (((ctl & 0x0020) << 4) & enhanced_high_bit);
        self.update_sprite_dma_comparator_with_vertical_timing(channel, sprite_timing);
    }

    /// Test/diagnostic accessors for the sprite-DMA state machine.
    #[must_use]
    pub fn sprite_vstart(&self, channel: usize) -> u16 {
        self.spr_vstart[channel]
    }
    #[must_use]
    pub fn sprite_vstop(&self, channel: usize) -> u16 {
        self.spr_vstop[channel]
    }
    #[must_use]
    pub fn sprite_dma_on(&self, channel: usize) -> bool {
        self.spr_dma_on[channel]
    }

    fn ocs_vertical_diw_bounds(&self) -> (u16, u16) {
        let vstart = (self.diwstrt >> 8) & 0x00FF;
        let stop_low = (self.diwstop >> 8) & 0x00FF;
        let stop_v8 = ((!((stop_low >> 7) & 0x1)) & 0x1) << 8;
        (vstart, stop_v8 | stop_low)
    }

    fn original_hard_vertical_blank_active(&self, vpos: u16, field_lines: u16) -> bool {
        if self.agnus_id >= 0x2000 {
            return false;
        }

        match self.original_revision {
            OriginalAgnusRevision::A1000 => vpos == 0,
            OriginalAgnusRevision::Later => vpos == field_lines - 1,
        }
    }

    fn evaluate_ocs_vertical_diw_comparators(&mut self, vpos: u16) {
        if self.agnus_id >= 0x2000 {
            return;
        }

        let (vstart, vstop) = self.ocs_vertical_diw_bounds();
        let start = vpos == vstart && !self.ocs_hard_vertical_blank_active;
        let stop = vpos == vstop || self.ocs_hard_vertical_blank_active;
        let was_active = self.ocs_vertical_diw_active;

        // Stop has precedence. Equal decoded comparators and a VSTART
        // coincident with the hard vertical-blank line therefore leave the
        // window closed.
        if start && !stop {
            self.ocs_vertical_diw_active = true;
        } else if stop {
            self.ocs_vertical_diw_active = false;
        }

        if was_active
            && !self.ocs_vertical_diw_active
            && self.ddf_start_match.is_some()
            && self.ddf_fetch_end.is_none()
        {
            self.ocs_ddf_run_aborted = true;
        }
    }

    fn evaluate_ocs_vertical_diw_write(&mut self) {
        self.evaluate_ocs_vertical_diw_comparators(self.vpos);
    }

    /// Whether the vertical display window is active at the current beam
    /// line. Original Agnus returns its hidden comparator-driven latch. Raw
    /// enhanced-chipset test instances retain the legacy geometric fallback;
    /// production ECS/AGA wrappers supply their own DIWHIGH-aware latch.
    #[must_use]
    pub fn vertical_diw_active(&self) -> bool {
        if self.agnus_id < 0x2000 {
            return self.ocs_vertical_diw_active;
        }

        let (vstart, vstop) = self.ocs_vertical_diw_bounds();
        if vstart == vstop {
            return false;
        }
        if vstart < vstop {
            self.vpos >= vstart && self.vpos < vstop
        } else {
            self.vpos >= vstart || self.vpos < vstop
        }
    }

    const fn ddf_mask(&self) -> u16 {
        if self.agnus_id >= 0x2000 {
            0x00FE
        } else {
            0x00FC
        }
    }

    fn evaluate_ddf_comparators(&mut self, fixed_ddf_right_stop_enabled: bool) {
        let ddf_mask = self.ddf_mask();
        let enhanced = self.agnus_id >= 0x2000;

        if !enhanced {
            // A terminal fetch represented on this physical line closes the
            // original-Agnus left gate only when its final CCK completes.
            // An idle line therefore carries the open state across EOL,
            // while an in-line completion carries the closed state. `$18`
            // reopens the gate before a coincident DDFSTRT comparator.
            if self.ddf_fetch_end == Some(self.hpos) {
                self.ocs_ddf_hard_start_open = false;
            }
            if self.hpos == 0x0018 {
                self.ocs_ddf_hard_start_open = true;
            }
        }

        // Stop signals sample the sequencer state that existed on beam
        // entry. In particular, a DDFSTRT comparator coincident with the
        // OCS $D8 hard edge may start an idle sequencer, but that new run
        // is not stopped by the same edge.
        if let Some(matched_start) = self.ddf_start_match
            && self.ddf_fetch_end.is_none()
            && (enhanced || !self.ocs_ddf_run_aborted)
        {
            let ddfstop = self.ddfstop & ddf_mask;
            // The ordinary stop branch applies only when this run started
            // before the stop comparator. A register-equal pair is not an
            // empty window: the stop phase samples the idle sequencer before
            // the start phase opens a run, so equality deliberately reaches
            // the fixed right edge instead. Equality with a pre-existing run,
            // stop-before-start, phase-shifted OCS terminal completion across
            // horizontal wrap and enhanced multiple regions need further
            // sequencer state.
            if matched_start < ddfstop && self.hpos == ddfstop {
                self.ddf_stop_match = Some(ddfstop);
                self.ddf_fetch_end = Some(self.ddf_terminal_fetch_end(matched_start, ddfstop));
            }

            // Original Agnus and enhanced chips with horizontal hard limits
            // enabled have a fixed right-hand fetch boundary at $D8. It
            // requests termination even when programmed DDFSTOP is later or
            // its comparator has already passed. Preserve the selected full
            // fetch-unit terminal policy and the matched start phase. An
            // ordinary comparator at the same position is recorded first;
            // both request the same terminal sequence.
            if fixed_ddf_right_stop_enabled && self.hpos == 0x00D8 && self.ddf_fetch_end.is_none() {
                self.ddf_fetch_end = Some(self.ddf_terminal_fetch_end(matched_start, 0x00D8));
            }
        }

        let ddfstrt = self.ddfstrt & ddf_mask;
        let start_comparator_can_open_run = self.ddf_start_match.is_none()
            || (!enhanced && self.ocs_ddf_run_aborted && self.ddf_fetch_end.is_none());
        if start_comparator_can_open_run
            && self.hpos == ddfstrt
            && (enhanced || self.ocs_ddf_hard_start_open)
        {
            // Early OCS starts the sequencer only when display DMA is enabled
            // inside the vertical display window. Fat Agnus, ECS and Alice
            // retain the comparator match independently and apply those gates
            // when arbitration consumes it. An OCS abort preserves its old
            // display-phase origin until a genuinely later DDFSTRT comparator
            // replaces it; restoring DMA alone cannot reach this branch.
            if enhanced || (self.dma_enabled(0x0100) && self.vertical_diw_active()) {
                self.ddf_start_match = Some(ddfstrt);
                if !enhanced {
                    self.ocs_ddf_run_aborted = false;
                }
            }
        }
    }

    fn ddf_terminal_fetch_end(&self, matched_start: u16, stop: u16) -> u16 {
        let hires = (self.bplcon0 & 0x8000) != 0;
        let shres = (self.bplcon0 & 0x0040) != 0;
        let fetch_width = self.bpl_fetch_width();
        let fetchunit = if fetch_width <= 1 {
            8
        } else {
            fetch_cadence(fetch_width, hires, shres).0
        };
        let span = u32::from(stop - matched_start);
        let blocks = span.div_ceil(fetchunit) + 1;
        let end = u32::from(matched_start) + blocks * fetchunit - 1;

        end.min(u32::from(u16::MAX)) as u16
    }

    /// Current line's observed DDFSTRT comparator and frozen fetch-phase
    /// origin.
    #[must_use]
    pub const fn ddf_start_match(&self) -> Option<u16> {
        self.ddf_start_match
    }

    /// Current line's ordinary DDFSTOP event that requested termination
    /// of the active fetch run.
    #[must_use]
    pub const fn ddf_stop_match(&self) -> Option<u16> {
        self.ddf_stop_match
    }

    /// Inclusive terminal CCK frozen when the current line observed an
    /// ordinary or fixed-hardware stop.
    #[must_use]
    pub const fn ddf_fetch_end(&self) -> Option<u16> {
        self.ddf_fetch_end
    }

    /// Whether the effective original-Agnus horizontal start permission
    /// currently admits a DDFSTRT comparator. This includes the proven
    /// admission result of a one-CCK terminal tail across a short-line wrap,
    /// not a claim about that tail's exact bus timing. Enhanced variants do
    /// not consume this shared serialized field.
    #[must_use]
    pub const fn ocs_ddf_hard_start_open(&self) -> bool {
        self.ocs_ddf_hard_start_open
    }

    /// Whether loss of effective bitplane eligibility terminated the current
    /// original-Agnus fetch run before a terminal unit was requested. The
    /// observed DDFSTRT phase remains available for the display pipeline, but
    /// cannot own new bus slots. Enhanced variants do not consume this shared
    /// field.
    #[must_use]
    pub const fn ocs_ddf_run_aborted(&self) -> bool {
        self.ocs_ddf_run_aborted
    }

    /// Return a side-effect-free snapshot of Agnus identity, beam, vertical
    /// latches, fixed-sync events and sprite-DMA state.
    #[must_use]
    pub fn diagnostic_snapshot(&self) -> AgnusDiagnosticSnapshot {
        AgnusDiagnosticSnapshot {
            identity: AgnusIdentityDiagnosticSnapshot {
                agnus_id: self.agnus_id,
                original_revision: self.original_revision,
                region: self.region,
                max_bitplanes: self.max_bitplanes,
            },
            beam: AgnusBeamDiagnosticSnapshot {
                vpos: self.vpos,
                hpos: self.hpos,
                lof: self.lof,
                lines_per_frame: self.lines_per_frame,
                lol: self.lol,
                lol_toggle: self.lol_toggle,
                vbl_count: self.vbl_count,
                current_line_ccks: self.current_line_ccks(),
                copper_comparator_hpos: self.copper_comparator_hpos(),
            },
            ocs_latches: AgnusOcsLatchDiagnosticSnapshot {
                vertical_diw_active: self.vertical_diw_active(),
                ocs_vertical_diw_active: self.ocs_vertical_diw_active,
                ocs_hard_vertical_blank_active: self.ocs_hard_vertical_blank_active,
            },
            events: AgnusEventDiagnosticSnapshot {
                vertb_level: self.vertb_level(),
                fixed_sync_copper_restart_event: self.fixed_sync_copper_restart_event(),
                fixed_sync_cia_a_tod_event: self.fixed_sync_cia_a_tod_event(),
                fixed_sync_cia_b_tod_event: self.fixed_sync_cia_b_tod_event(),
            },
            sprite_dma: AgnusSpriteDmaDiagnosticSnapshot {
                spr_pt: self.spr_pt,
                spr_pt_hi_latch: self.spr_pt_hi_latch,
                spr_pt_hi_pending: self.spr_pt_hi_pending,
                spr_vstart: self.spr_vstart,
                spr_vstop: self.spr_vstop,
                spr_dma_on: self.spr_dma_on,
            },
        }
    }

    /// Return a side-effect-free diagnostic snapshot of the implemented DDF
    /// comparator and fetch-run state.
    #[must_use]
    pub fn ddf_diagnostic_snapshot(&self) -> AgnusDdfDiagnosticSnapshot {
        let comparator_mask = self.ddf_mask();
        AgnusDdfDiagnosticSnapshot {
            vpos: self.vpos,
            hpos: self.hpos,
            agnus_id: self.agnus_id,
            ddfstrt: self.ddfstrt,
            ddfstop: self.ddfstop,
            comparator_mask,
            effective_ddfstrt: self.ddfstrt & comparator_mask,
            effective_ddfstop: self.ddfstop & comparator_mask,
            start_match: self.ddf_start_match,
            stop_match: self.ddf_stop_match,
            fetch_end: self.ddf_fetch_end,
            ocs_run_aborted: self.ocs_ddf_run_aborted,
            ocs_hard_start_open: self.ocs_ddf_hard_start_open,
        }
    }

    /// Determine who owns the current CCK slot.
    pub fn current_slot(&self) -> SlotOwner {
        self.current_slot_with_vertical_timing(
            self.vertical_diw_active(),
            self.fixed_sprite_dma_vertical_timing(),
        )
    }

    fn current_slot_with_vertical_timing(
        &self,
        bitplane_vertical_active: bool,
        sprite_timing: SpriteDmaVerticalTiming,
    ) -> SlotOwner {
        // Hardware-correct OCS PAL DMA time-slot allocation (vAmiga
        // `SequencerDas.cpp` + Minimig `agnus.v` priority chain). Every
        // fixed chipset slot sits on an ODD hpos; the CPU gets the even
        // gaps plus any odd slot no channel claimed; the copper takes the
        // even FREE cells. Priority on a contended cell:
        //   disk > refresh > audio > bitplane > sprite > copper > cpu
        // (blitter contention is layered onto the Cpu slots in
        // `cck_bus_plan`). #30; see
        // `docs/plans/2026-06-12-amiga-single-bus-rewrite-30.md`.
        let hpos = self.hpos;
        let eol_refresh = if self.lol { 0xE3 } else { 0xE2 };

        // Disk D0/D1/D2 (DSKEN) — highest priority.
        if self.dma_enabled(0x0010) && matches!(hpos, 0x07 | 0x09 | 0x0B) {
            return SlotOwner::Disk;
        }
        // Memory refresh — 0x01/0x03/0x05 + end-of-line. Unconditional.
        if matches!(hpos, 0x01 | 0x03 | 0x05) || hpos == eol_refresh {
            return SlotOwner::Refresh;
        }
        // Audio A0–A3 (per-channel DMACON enable).
        match hpos {
            0x0D if self.dma_enabled(0x0001) => return SlotOwner::Audio(0),
            0x0F if self.dma_enabled(0x0002) => return SlotOwner::Audio(1),
            0x11 if self.dma_enabled(0x0004) => return SlotOwner::Audio(2),
            0x13 if self.dma_enabled(0x0008) => return SlotOwner::Audio(3),
            _ => {}
        }
        // Bitplane (DDF-gated) — priority above sprite on a DDF∩sprite
        // overlap (only with a very low DDFSTRT). Fetch grid unchanged.
        if bitplane_vertical_active && let Some(plane) = self.bitplane_slot_at() {
            return SlotOwner::Bitplane(plane);
        }
        // Sprites 0–7 at 0x15..0x33 (odd cells). SPREN makes the
        // opportunities available, but a channel claims its pair only
        // while requesting a control or data fetch.
        if self.dma_enabled(0x0020) && (0x15..=0x33).contains(&hpos) && !hpos.is_multiple_of(2) {
            let channel = ((hpos - 0x15) / 4) as usize;
            if self.sprite_dma_cycle_requested_with_vertical_timing(channel, sprite_timing) {
                return SlotOwner::Sprite(channel as u8);
            }
        }
        // Copper — even FREE cells, COPEN, excluding the E0 blocked cell.
        if self.dma_enabled(0x0080) && hpos.is_multiple_of(2) && hpos != 0xE0 {
            return SlotOwner::Copper;
        }
        // CPU gets every remaining cell.
        SlotOwner::Cpu
    }

    /// Bitplane DMA owner for the current `hpos`, or `None` if no plane
    /// is fetched this cell. DDF-gated; the fetch-unit -> plane tables are
    /// the validated, vAmiga-identical grids (unchanged by #30).
    fn bitplane_slot_at(&self) -> Option<u8> {
        let num_bpl = self.num_bitplanes();
        let hires = (self.bplcon0 & 0x8000) != 0;
        let shres = (self.bplcon0 & 0x0040) != 0;
        let fetch_width = self.bpl_fetch_width();
        let ddfstrt = self.ddf_start_match?;
        if self.agnus_id < 0x2000 && self.ocs_ddf_run_aborted {
            return None;
        }
        if self.ddf_fetch_end.is_some_and(|end| self.hpos > end) {
            return None;
        }
        if self.dma_enabled(0x0100) && num_bpl > 0 && self.hpos >= ddfstrt {
            if fetch_width <= 1 {
                // OCS / ECS / AGA 16-bit cadence. SuperHires (ECS+)
                // halves the group to 2 CCKs; lores/hires unchanged.
                let group_len = if shres {
                    2
                } else if hires {
                    4
                } else {
                    8
                };
                let pos_in_group = ((self.hpos - ddfstrt) % group_len) as usize;
                let plane_slot = if shres {
                    // SuperHires at 16-bit fetch — 2 planes / 4
                    // colours (#469). BPL3+ are not fetched (the
                    // 2-slot group caps it, matching real SHRES).
                    SHRES_DDF_TO_PLANE[pos_in_group]
                } else if hires {
                    HIRES_DDF_TO_PLANE[pos_in_group]
                } else if self.max_bitplanes > 6 {
                    // AGA lowres fills the two idle slots with
                    // BPL7/BPL8 (#99). Identical to the OCS table
                    // for planes 0-5; the `< num_bpl` filter below
                    // drops BPL7/BPL8 when fewer planes are active.
                    LOWRES_DDF_TO_PLANE_AGA[pos_in_group]
                } else {
                    LOWRES_DDF_TO_PLANE[pos_in_group]
                };
                plane_slot.filter(|&p| p < num_bpl)
            } else {
                // AGA wide fetch (FMODE > 0). Each fetchstart group
                // contains one access per active plane, with each access
                // transferring `fetch_width` words.
                let (_, fetchstart) = fetch_cadence(fetch_width, hires, shres);
                let pos = ((u32::from(self.hpos) - u32::from(ddfstrt)) % fetchstart) as usize;
                // The 8-entry WIDE order covers planes 0..7 only
                // when the group is >= 8 CCKs (FMODE>0 lores/hires,
                // and SHRES@FMODE2). SHRES@FMODE1 shrinks the group
                // to 4 CCKs / 4 planes (#469) — the wide order does
                // not nest, so reuse the 4-slot hires order, whose
                // plane set is exactly planes 0..3 with BPL1 last.
                let plane_slot = match fetchstart {
                    4 => HIRES_DDF_TO_PLANE.get(pos).copied().flatten(),
                    _ => WIDE_FETCH_PLANE_ORDER.get(pos).copied().flatten(),
                };
                plane_slot.filter(|&p| p < num_bpl)
            }
        } else {
            None
        }
    }

    /// Compute the machine-facing Agnus bus-arbitration plan for this CCK.
    pub fn cck_bus_plan(&self) -> CckBusPlan {
        self.cck_bus_plan_with_vertical_timing(
            self.vertical_diw_active(),
            self.fixed_sprite_dma_vertical_timing(),
        )
    }

    /// Return a side-effect-free diagnostic snapshot of the current OCS bus
    /// plan and the recorded per-CCK use latches.
    ///
    /// ECS and AGA wrappers with programmable vertical timing should compute
    /// their wrapper-specific plan and call
    /// [`Self::bus_diagnostic_snapshot_for_plan`] instead.
    #[must_use]
    pub fn bus_diagnostic_snapshot(&self) -> AgnusBusDiagnosticSnapshot {
        self.bus_diagnostic_snapshot_for_plan(self.cck_bus_plan())
    }

    /// Return a side-effect-free bus diagnostic snapshot using an
    /// integration-selected current plan.
    ///
    /// This seam lets enhanced-chipset wrappers supply the same
    /// vertical-timing-aware plan they use for arbitration while Agnus remains
    /// the owner of the actual-use latches and diagnostic representation.
    #[must_use]
    pub fn bus_diagnostic_snapshot_for_plan(&self, plan: CckBusPlan) -> AgnusBusDiagnosticSnapshot {
        let (blitter_authority, blitter_holds_bus) = if self.blitter_cck_bus_state_recorded {
            (
                BlitterBusDiagnosticAuthority::RecordedCckState,
                self.blitter_bus_used_this_cck || self.blitter_nasty_owned_this_cck,
            )
        } else {
            (
                BlitterBusDiagnosticAuthority::CurrentPlanFallback,
                matches!(plan.slot_owner, SlotOwner::Cpu) && !plan.cpu_chip_bus_granted,
            )
        };

        AgnusBusDiagnosticSnapshot {
            vpos: self.vpos,
            hpos: self.hpos,
            plan,
            sprite_bus_used_this_cck: self.sprite_bus_used_this_cck,
            sprite_holds_bus: self.sprite_bus_used_this_cck
                || matches!(plan.slot_owner, SlotOwner::Sprite(_)),
            blitter_bus_used_this_cck: self.blitter_bus_used_this_cck,
            blitter_nasty_owned_this_cck: self.blitter_nasty_owned_this_cck,
            blitter_cck_bus_state_recorded: self.blitter_cck_bus_state_recorded,
            blitter_authority,
            blitter_holds_bus,
        }
    }

    /// Build a complete plan using a chipset-variant vertical bitplane
    /// eligibility decision.
    ///
    /// ECS and AGA wrappers use this seam so their extended DIW decode
    /// participates in the normal priority chain instead of mutating an
    /// OCS plan after bitplane arbitration.
    pub fn cck_bus_plan_with_bitplane_vertical_active(
        &self,
        bitplane_vertical_active: bool,
    ) -> CckBusPlan {
        self.cck_bus_plan_with_vertical_timing(
            bitplane_vertical_active,
            self.fixed_sprite_dma_vertical_timing(),
        )
    }

    /// Build a complete plan using chipset-variant bitplane and sprite
    /// vertical timing.
    #[doc(hidden)]
    pub fn cck_bus_plan_with_vertical_timing(
        &self,
        bitplane_vertical_active: bool,
        sprite_timing: SpriteDmaVerticalTiming,
    ) -> CckBusPlan {
        let slot_owner =
            self.current_slot_with_vertical_timing(bitplane_vertical_active, sprite_timing);
        let disk_dma_slot_granted = matches!(slot_owner, SlotOwner::Disk);
        let sprite_dma_service_channel = match slot_owner {
            SlotOwner::Sprite(channel) => Some(channel),
            _ => None,
        };
        let audio_dma_service_channel = match slot_owner {
            SlotOwner::Audio(channel) => Some(channel),
            _ => None,
        };
        let bitplane_dma_fetch_plane = match slot_owner {
            SlotOwner::Bitplane(plane) => Some(plane),
            _ => None,
        };
        let copper_dma_slot_granted = matches!(slot_owner, SlotOwner::Copper);
        let blitter_dma_progress_granted =
            matches!(slot_owner, SlotOwner::Cpu) && self.blitter_busy && self.dma_enabled(0x0040);
        let blitter_nasty_active = self.blitter_nasty_active();
        let blitter_chip_bus_granted = blitter_dma_progress_granted && blitter_nasty_active;
        let cpu_chip_bus_granted =
            matches!(slot_owner, SlotOwner::Cpu) && !blitter_chip_bus_granted;
        let paula_return_progress_policy = match slot_owner {
            SlotOwner::Refresh
            | SlotOwner::Disk
            | SlotOwner::Sprite(_)
            | SlotOwner::Bitplane(_) => PaulaReturnProgressPolicy::Stall,
            SlotOwner::Copper => PaulaReturnProgressPolicy::CopperFetchConditional,
            SlotOwner::Cpu | SlotOwner::Audio(_) => PaulaReturnProgressPolicy::Advance,
        };
        CckBusPlan {
            slot_owner,
            disk_dma_slot_granted,
            sprite_dma_service_channel,
            audio_dma_service_channel,
            bitplane_dma_fetch_plane,
            copper_dma_slot_granted,
            cpu_chip_bus_granted,
            blitter_chip_bus_granted,
            blitter_dma_progress_granted,
            paula_return_progress_policy,
        }
    }
}

/// Machine-facing register-write API. The machine's custom-register
/// bus dispatches here rather than reaching into Agnus's pub fields,
/// so that set/clear semantics and write-masking live in one place.
impl Agnus {
    /// Write to DMACON ($DFF096) with HRM set/clear semantics:
    /// bit 15 = SET, bit 15 clear = CLEAR.
    pub fn write_dmacon(&mut self, val: u16) {
        let bitplane_dma_was_enabled = self.dma_enabled(bits::DMACON_BPLEN);
        if val & bits::DMACON_SETCLR != 0 {
            self.dmacon |= val & bits::DMACON_MASK;
        } else {
            self.dmacon &= !(val & bits::DMACON_MASK);
        }
        if self.agnus_id < 0x2000
            && bitplane_dma_was_enabled
            && !self.dma_enabled(bits::DMACON_BPLEN)
            && self.ddf_start_match.is_some()
            && self.ddf_fetch_end.is_none()
        {
            self.ocs_ddf_run_aborted = true;
        }
    }

    /// Write BPLCON0 ($DFF100). Straight store; Denise sees the same
    /// value for mode bits.
    pub fn write_bplcon0(&mut self, val: u16) {
        self.bplcon0 = val;
    }

    pub fn write_ddfstrt(&mut self, val: u16) {
        self.ddfstrt = val;
    }
    pub fn write_ddfstop(&mut self, val: u16) {
        self.ddfstop = val;
    }
    pub fn write_diwstrt(&mut self, val: u16) {
        self.diwstrt = val;
        self.evaluate_ocs_vertical_diw_write();
    }
    pub fn write_diwstop(&mut self, val: u16) {
        self.diwstop = val;
        self.evaluate_ocs_vertical_diw_write();
    }
    pub fn write_bpl1mod(&mut self, val: u16) {
        self.bpl1mod = val as i16;
    }
    pub fn write_bpl2mod(&mut self, val: u16) {
        self.bpl2mod = val as i16;
    }

    /// Write one half of a bitplane pointer. `high_word = true` writes
    /// BPLxPTH (the upper 16 bits); `false` writes BPLxPTL (lower 16,
    /// bit 0 forced to 0 for chip-RAM word alignment).
    pub fn write_bpl_pointer(&mut self, plane: usize, high_word: bool, val: u16) {
        if plane >= self.bpl_pt.len() {
            return;
        }
        let cur = self.bpl_pt[plane];
        self.bpl_pt[plane] = if high_word {
            (cur & 0x0000_FFFF) | (u32::from(val) << 16)
        } else {
            (cur & 0xFFFF_0000) | u32::from(val & 0xFFFE)
        };
    }

    /// Write one half of DSKPT ($020 / $022).
    pub fn write_dsk_pointer(&mut self, high_word: bool, val: u16) {
        self.dsk_pt = if high_word {
            (self.dsk_pt & 0x0000_FFFF) | (u32::from(val) << 16)
        } else {
            (self.dsk_pt & 0xFFFF_0000) | u32::from(val & 0xFFFE)
        };
    }

    /// Dispatch a CPU write to any blitter register ($040..=$074).
    /// Returns `true` if the offset is a blitter register (handled or
    /// silently dropped on the unused slots), `false` otherwise.
    /// Writing BLTSIZE ($058) triggers `start_blit()` — the caller is
    /// responsible for running the blit to completion afterward.
    pub fn write_blitter_register(&mut self, offset: u16, val: u16) -> bool {
        match offset {
            0x040 => self.bltcon0 = val,
            0x042 => self.bltcon1 = val,
            0x044 => self.blt_afwm = val,
            0x046 => self.blt_alwm = val,
            0x048 => self.blt_cpt = (self.blt_cpt & 0x0000_FFFF) | (u32::from(val) << 16),
            0x04A => self.blt_cpt = (self.blt_cpt & 0xFFFF_0000) | u32::from(val & 0xFFFE),
            0x04C => self.blt_bpt = (self.blt_bpt & 0x0000_FFFF) | (u32::from(val) << 16),
            0x04E => self.blt_bpt = (self.blt_bpt & 0xFFFF_0000) | u32::from(val & 0xFFFE),
            0x050 => self.blt_apt = (self.blt_apt & 0x0000_FFFF) | (u32::from(val) << 16),
            0x052 => self.blt_apt = (self.blt_apt & 0xFFFF_0000) | u32::from(val & 0xFFFE),
            0x054 => self.blt_dpt = (self.blt_dpt & 0x0000_FFFF) | (u32::from(val) << 16),
            0x056 => self.blt_dpt = (self.blt_dpt & 0xFFFF_0000) | u32::from(val & 0xFFFE),
            0x058 => {
                self.bltsize = val;
                self.start_blit();
            }
            0x060 => self.blt_cmod = val as i16,
            0x062 => self.blt_bmod = val as i16,
            0x064 => self.blt_amod = val as i16,
            0x066 => self.blt_dmod = val as i16,
            0x070 => self.blt_cdat = val,
            0x072 => self.blt_bdat = val,
            0x074 => self.blt_adat = val,
            // Unused / ECS-only slots in the blitter range.
            0x05A..=0x05E | 0x068..=0x06E | 0x076..=0x07A => {}
            _ => return false,
        }
        true
    }

    /// Emit the one-shot main-finish source and arm observer-specific busy
    /// holds. Returns `true` only for the first emission of the current blit.
    fn emit_blitter_finish(&mut self) -> bool {
        if self.blitter_finish_emitted {
            return false;
        }
        self.blitter_finish_emitted = true;
        // The source fires after the Copper has sampled this CCK but before a
        // same-CCK CPU custom-register read. DMACONR therefore needs one
        // retained CCK; Copper needs the finish CCK plus one more.
        self.blitter_dmacon_busy_hold_ccks = 1;
        self.blitter_copper_busy_hold_ccks = 2;
        true
    }

    /// Whether the next scheduler request is the final D of a normal area
    /// blit. That request is split into main-finish, result and write stages.
    #[must_use]
    fn final_area_d_requested(&self) -> bool {
        self.blitter_startup_ccks_remaining == 0
            && matches!(
                (
                    self.blitter_line_runtime,
                    self.blitter_area_runtime,
                    self.next_blitter_dma_request(),
                ),
                (
                    None,
                    Some(BlitterAreaRuntime {
                        rows_remaining: 1,
                        words_remaining_in_row: 1,
                        use_d: true,
                        ..
                    }),
                    Some(BlitterDmaOp::WriteD),
                )
            )
    }

    /// Clear internal activity after every pipeline stage has drained.
    fn finish_blitter_pipeline(&mut self) {
        self.blitter_word_state = None;
        self.blitter_busy = false;
        self.blitter_startup_ccks_remaining = 0;
        self.blitter_completion_phase = None;
        self.blitter_exec_pending = false;
        self.blitter_ccks_remaining = 0;
        self.blitter_line_runtime = None;
        self.blitter_area_runtime = None;
    }

    /// Execute one already-consumed scheduler operation against the bus.
    fn execute_blitter_bus_op(&mut self, op: BlitterDmaOp, bus: &mut dyn BlitterBus) -> bool {
        match op {
            BlitterDmaOp::WriteD => self.execute_incremental_blitter_op(
                op,
                |_| 0,
                |addr, val| bus.write_word(addr, val),
            ),
            BlitterDmaOp::Internal => self.execute_incremental_blitter_op(op, |_| 0, |_, _| {}),
            _ => self.execute_incremental_blitter_op(op, |addr| bus.read_word(addr), |_, _| {}),
        }
    }

    /// Whether the current logical operation drives the chip bus.
    ///
    /// A suppressed line-mode ONEDOT D operation still computes the minterm,
    /// updates BZERO, advances the line state and may emit completion, but it
    /// does not perform a D transfer.
    fn blitter_operation_uses_bus(&self, op: BlitterDmaOp) -> bool {
        match op {
            BlitterDmaOp::Internal => false,
            BlitterDmaOp::WriteD => !self
                .blitter_line_runtime
                .is_some_and(|line| line.sing && line.one_dot_drawn),
            BlitterDmaOp::ReadA | BlitterDmaOp::ReadB | BlitterDmaOp::ReadC => true,
        }
    }

    /// Whether the next admitted scheduler CCK needs the chip bus.
    ///
    /// Startup retains the existing accepted/free-cell policy. Once startup
    /// has drained, the pending operation determines whether nasty mode owns
    /// the cell; notably, a suppressed ONEDOT WriteD leaves it free.
    fn next_blitter_progress_uses_bus(&self) -> bool {
        if self.blitter_startup_ccks_remaining != 0 {
            return true;
        }
        self.next_blitter_dma_request()
            .is_none_or(|op| self.blitter_operation_uses_bus(op))
    }

    /// Advance the blitter by one CCK.
    ///
    /// `progress_granted` admits startup, channel operations and the final D
    /// bus write. Once the last main cycle has been admitted, the internal
    /// final-result stage advances on the following CCK without another bus
    /// grant. This keeps the two-stage output pipeline distinct from bus
    /// arbitration.
    pub fn tick_blitter_cck(
        &mut self,
        progress_granted: bool,
        bus: &mut dyn BlitterBus,
    ) -> BlitterCckOutcome {
        if !self.blitter_busy {
            return BlitterCckOutcome::default();
        }

        if let Some(phase) = self.blitter_completion_phase {
            return match phase {
                BlitterCompletionPhase::FinalResult => {
                    let op = self.tick_blitter_scheduler_op(true);
                    debug_assert_eq!(op, BlitterProgress::Operation(BlitterDmaOp::WriteD));

                    let mut pending_write = None;
                    let done = self.execute_incremental_blitter_op(
                        BlitterDmaOp::WriteD,
                        |_| 0,
                        |addr, value| pending_write = Some((addr, value)),
                    );
                    debug_assert!(done, "the buffered D operation must be the final area word");
                    let Some((addr, value)) = pending_write else {
                        self.finish_blitter_pipeline();
                        return BlitterCckOutcome::default();
                    };
                    self.blitter_completion_phase =
                        Some(BlitterCompletionPhase::FinalWrite { addr, value });
                    BlitterCckOutcome::default()
                }
                BlitterCompletionPhase::FinalWrite { addr, value } => {
                    if !progress_granted {
                        return BlitterCckOutcome::default();
                    }
                    bus.write_word(addr, value);
                    self.finish_blitter_pipeline();
                    BlitterCckOutcome {
                        interrupt: self.emit_blitter_finish(),
                        bus_used: true,
                    }
                }
            };
        }

        if !progress_granted {
            return BlitterCckOutcome::default();
        }

        // The existing scheduler presents final D as one operation. Hardware
        // first retires the last main cycle, then computes BZERO/result, then
        // writes D. Split only the final area word here; earlier pipelined D
        // operations retain the established execution model.
        if self.final_area_d_requested() {
            self.blitter_completion_phase = Some(BlitterCompletionPhase::FinalResult);
            return BlitterCckOutcome {
                interrupt: if self.is_alice() {
                    false
                } else {
                    self.emit_blitter_finish()
                },
                bus_used: false,
            };
        }

        let op = match self.tick_blitter_scheduler_op(true) {
            BlitterProgress::Startup | BlitterProgress::NoProgress => {
                return BlitterCckOutcome::default();
            }
            BlitterProgress::Operation(op) => op,
        };
        let bus_used = self.blitter_operation_uses_bus(op);
        let done = self.execute_blitter_bus_op(op, bus);
        if self.blitter_word_complete() && !done {
            self.advance_blitter_word();
        }
        if self.next_blitter_dma_request().is_some() {
            return BlitterCckOutcome {
                interrupt: false,
                bus_used,
            };
        }

        self.finish_blitter_pipeline();
        BlitterCckOutcome {
            interrupt: self.emit_blitter_finish(),
            bus_used,
        }
    }

    /// Drive a blit to completion synchronously.
    ///
    /// This is the transaction-level CPU-write serialization fallback. It
    /// consumes the same startup, completion and final-D stages as the live
    /// CCK path, but exposes no intermediate observer phase to software.
    ///
    /// Takes a single bus trait implementation — matches on op type
    /// so only one direction of the bus is borrowed at a time.
    pub fn run_blit_to_completion(&mut self, bus: &mut dyn BlitterBus) -> bool {
        if !self.blitter_busy {
            return false;
        }

        let maximum_steps = u64::from(self.blitter_startup_ccks_remaining)
            + u64::from(self.blitter_ccks_remaining)
            + 2;
        let mut steps = 0u64;
        let mut interrupt = false;
        while self.blitter_busy {
            steps += 1;
            interrupt |= self.tick_blitter_cck(true, bus).interrupt;
            assert!(
                steps <= maximum_steps,
                "serialized blitter drain exceeded its finite operation budget",
            );
        }
        // No external observer ran during the synchronous drain.
        self.blitter_dmacon_busy_hold_ccks = 0;
        self.blitter_copper_busy_hold_ccks = 0;
        interrupt
    }

    /// Compatibility wrapper that advances one always-granted CCK.
    ///
    /// Returns `true` only when this CCK drains the complete internal pipeline.
    /// It deliberately does not expose the earlier pre-AGA finish-source edge;
    /// integrations that need interrupt timing or bus use must call
    /// [`Agnus::tick_blitter_cck`].
    pub fn tick_blitter_dma(&mut self, bus: &mut dyn BlitterBus) -> bool {
        let was_busy = self.blitter_busy;
        let _ = self.tick_blitter_cck(true, bus);
        was_busy && !self.blitter_busy
    }
}

/// Chip-bus interface for the blitter synchronous-completion helper.
pub trait BlitterBus {
    fn read_word(&mut self, addr: u32) -> u16;
    fn write_word(&mut self, addr: u32, val: u16);
}

impl Agnus {
    /// Read VPOSR ($DFF004): bit 15 = LOF, bits 14-8 = agnus_id, bit 0
    /// = vpos high bit (vpos bit 8). Bits 7-1 are unused.
    #[must_use]
    pub fn vposr(&self) -> u16 {
        let mut v = self.agnus_id & 0x7F00;
        if self.lof {
            v |= 0x8000;
        }
        v | ((self.vpos >> 8) & 0x0001)
    }

    /// Read VHPOSR ($DFF006): vpos low 8 bits + hpos low 8 bits.
    #[must_use]
    pub fn vhposr(&self) -> u16 {
        ((self.vpos & 0xFF) << 8) | (self.hpos & 0xFF)
    }

    /// Read DMACONR ($DFF002): the stored DMACON enable/control bits
    /// plus live blitter status. BBUSY (bit 14) follows externally visible
    /// busy: later revisions assert it at start, while A1000 Agnus keeps it
    /// clear until the first accepted startup CCK. At pre-AGA completion it
    /// can release before the internal final-D tail drains. BZERO (bit 13)
    /// reports whether the current or last blit generated only zero D
    /// results; a new blit reloads it on its first accepted startup CCK.
    #[must_use]
    pub fn dmaconr(&self) -> u16 {
        let mut v = self.dmacon & bits::DMACON_MASK;
        if self.blitter_busy_visible() {
            v |= 0x4000; // BBUSY
        }
        if self.blitter_dzero {
            v |= 0x2000; // BZERO
        }
        v
    }

    /// Whether the beam is inside the fixed vertical-blank interval.
    ///
    /// The machine detects the transition into this interval to
    /// generate the once-per-frame `VERTB` request. This predicate is
    /// not itself a level-sensitive interrupt input.
    #[must_use]
    pub fn vertb_level(&self) -> bool {
        self.vpos < self.fixed_vblank_end_line()
    }

    /// Whether the current fixed-sync beam position issues the
    /// automatic Copper `COP1LC` restart strobe.
    ///
    /// On fixed-sync Amiga hardware this occurs at the frame boundary,
    /// when the beam enters line zero. It is a separate Agnus event from
    /// Paula's `VERTB` request even though both share the same raster
    /// boundary.
    ///
    /// This predicate identifies the functional strobe position. The
    /// subsequent internal Copper pointer-load pipeline is not yet
    /// represented separately.
    #[must_use]
    pub fn fixed_sync_copper_restart_event(&self) -> bool {
        self.vpos == 0 && self.hpos == 0
    }

    /// Whether the current fixed-sync beam position is the
    /// counter-visible CIA-A TOD event.
    ///
    /// The A500 feeds active-low `/VSYNC` directly to CIA-A `TICK`,
    /// whose counter advances after the input rises at sync end. The
    /// current CIA model has no pin synchroniser, so this folds the
    /// measured delay into one visible event at horizontal position
    /// 84. PAL sync ends on line 5; NTSC sync ends on line 6.
    ///
    /// This is a fixed-sync approximation. Field-dependent sync phase,
    /// the CIA's internal delay, and ECS/AGA programmable sync remain
    /// separate future work.
    #[must_use]
    pub fn fixed_sync_cia_a_tod_event(&self) -> bool {
        const COUNTER_VISIBLE_HPOS: u16 = 84;
        let line = match self.region {
            AgnusRegion::Pal => 5,
            AgnusRegion::Ntsc => 6,
        };
        self.vpos == line && self.hpos == COUNTER_VISIBLE_HPOS
    }

    /// Whether the current fixed-sync beam position is the
    /// counter-visible CIA-B TOD event.
    ///
    /// The A500 feeds active-low `/HSYNC` directly to CIA-B `TICK`.
    /// The raw sync pulse ends earlier in the line, but the current
    /// CIA model has no input synchroniser. Horizontal position
    /// `$66` folds the measured CIA delay into one visible update.
    ///
    /// As with [`Self::fixed_sync_cia_a_tod_event`], this is an
    /// approximation until the sync waveform and CIA input delay are
    /// represented separately.
    #[must_use]
    pub fn fixed_sync_cia_b_tod_event(&self) -> bool {
        const COUNTER_VISIBLE_HPOS: u16 = 0x66;
        self.hpos == COUNTER_VISIBLE_HPOS
    }
}

/// Fixed PAL vertical-blank end and sprite control-refetch boundary.
pub const PAL_VBL_END_LINE: u16 = 25;
/// Fixed NTSC vertical-blank end and sprite control-refetch boundary.
pub const NTSC_VBL_END_LINE: u16 = 20;
/// Backward-compatible PAL boundary alias.
pub const VBL_END_LINE: u16 = PAL_VBL_END_LINE;

impl Default for Agnus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DMACON_DMAEN: u16 = 0x0200;
    const DMACON_AUD0EN: u16 = 0x0001;
    const DMACON_BLTEN: u16 = 0x0040;
    const DMACON_COPEN: u16 = 0x0080;
    const DMACON_BPLEN: u16 = 0x0100;
    const DMACON_BLTPRI: u16 = 0x0400;

    #[test]
    fn fetch_cadence_matches_winuae_tables() {
        // 16-bit (FMODE=0): historical OCS/ECS cadence — fetchunit 8,
        // fetchstart 4 (hires) / 8 (lores). Must be unchanged.
        assert_eq!(fetch_cadence(1, true, false), (8, 4));
        assert_eq!(fetch_cadence(1, false, false), (8, 8));
        // 32-bit (FMODE bits=01/10): fetchunit 8, fetchstart 8 (hires).
        assert_eq!(fetch_cadence(2, true, false), (8, 8));
        assert_eq!(fetch_cadence(2, false, false), (16, 16));
        // 64-bit (FMODE bits=11): hires fetchunit 16, fetchstart 16 —
        // one plane access per 16 CCK, four words each. This is the
        // Workbench 3.1 AGA case.
        assert_eq!(fetch_cadence(4, true, false), (16, 16));
        assert_eq!(fetch_cadence(4, false, false), (32, 32));
        // SuperHires column (#469) — fetchstart halves vs hires at each
        // width: FMODE0 shres = (8, 2), FMODE1 = (8, 4), FMODE2 = (8, 8).
        // `shres` overrides `hires`.
        assert_eq!(fetch_cadence(1, false, true), (8, 2));
        assert_eq!(fetch_cadence(2, false, true), (8, 4));
        assert_eq!(fetch_cadence(4, true, true), (8, 8));
    }

    fn observe_ddf_start_for_test(agnus: &mut Agnus) {
        if agnus.agnus_id < 0x2000 {
            agnus.vpos = 0x0030;
            agnus.diwstrt = 0x2C81;
            agnus.diwstop = 0x2CC1;
            agnus.ocs_vertical_diw_active = true;
            // This helper jumps directly to the comparator. Establish the
            // post-$18 precondition explicitly instead of making unrelated
            // cadence tests traverse the horizontal latch lifecycle.
            agnus.ocs_ddf_hard_start_open = true;
        }
        let start = agnus.ddfstrt & agnus.ddf_mask();
        assert!(start > 0, "test helper requires a non-zero DDFSTRT");
        agnus.hpos = start - 1;
        agnus.tick_cck();
        assert_eq!(agnus.ddf_start_match(), Some(start));
    }

    /// Count bitplane DMA grants for one plane across a whole scan line.
    fn bitplane_grants(agnus: &mut Agnus, plane: u8) -> usize {
        if agnus.ddf_start_match().is_some() {
            agnus.hpos = agnus.current_line_ccks() - 1;
            agnus.tick_cck();
        }
        observe_ddf_start_for_test(agnus);
        let mut count = 0;
        loop {
            if agnus.current_slot() == SlotOwner::Bitplane(plane) {
                count += 1;
            }
            if agnus.hpos == 0xE2 {
                break;
            }
            agnus.tick_cck();
        }
        count
    }

    #[test]
    fn wide_fetch_workbench31_accesses_match_modulo_oracle() {
        // Workbench 3.1 AGA desktop, captured live from the copper list:
        //   DDFSTRT=$38 DDFSTOP=$D8, hires, 2 planes, FMODE=$0003 (64-bit).
        // Interleaved bitmap, line-start BPL1PT=$3E328 BPL2PT=$3E378
        // (row stride 80 bytes), BPL1MOD=BPL2MOD=72. Per-plane fetched
        // bytes = 2*80 - 72 = 88 = 44 words. At 64-bit (4 words/access)
        // that is 11 plane accesses per line. The wide-fetch loop turns
        // each access into 4 words, giving the 44 the modulo demands.
        let mut agnus = Agnus::new();
        agnus.agnus_id = 0x2300;
        agnus.max_bitplanes = 8; // AGA
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 0xA302; // HIRES + 2 planes
        agnus.ddfstrt = 0x38;
        agnus.ddfstop = 0xD8;
        agnus.fmode = 0x0003; // 64-bit bitplane fetch

        assert_eq!(agnus.bpl_fetch_width(), 4);
        assert_eq!(agnus.num_bitplanes(), 2);
        assert_eq!(bitplane_grants(&mut agnus, 0), 11, "BPL1 accesses/line");
        assert_eq!(bitplane_grants(&mut agnus, 1), 11, "BPL2 accesses/line");
    }

    #[test]
    fn narrow_fetch_ocs_hires_accesses_unchanged() {
        // OCS / ECS regression guard: FMODE=0 must keep the historical
        // 16-bit cadence byte-for-byte. DDFSTRT=$40 DDFSTOP=$D0 hires =
        // 19 fetchunit blocks ($40..$D7) → 38 accesses per active plane.
        let mut agnus = Agnus::new(); // max_bitplanes = 6, fmode = 0
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 0xA000; // HIRES + 2 planes
        agnus.ddfstrt = 0x40;
        agnus.ddfstop = 0xD0;

        assert_eq!(agnus.bpl_fetch_width(), 1);
        assert_eq!(agnus.num_bitplanes(), 2);
        assert_eq!(bitplane_grants(&mut agnus, 0), 38, "BPL1 accesses/line");
        assert_eq!(bitplane_grants(&mut agnus, 1), 38, "BPL2 accesses/line");
    }

    #[test]
    fn documented_normal_ddf_windows_keep_their_word_counts() {
        // Amiga Hardware Reference Manual formulas:
        //   lores $38..$D0 = 20 words per plane
        //   hires $3C..$D4 = 40 words per plane
        let mut lores = Agnus::new();
        lores.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        lores.bplcon0 = 0x1000; // lores, one plane
        lores.ddfstrt = 0x0038;
        lores.ddfstop = 0x00D0;
        assert_eq!(bitplane_grants(&mut lores, 0), 20);
        assert_eq!(lores.ddf_stop_match(), Some(0x00D0));
        assert_eq!(lores.ddf_fetch_end(), Some(0x00D7));

        let mut hires = Agnus::new();
        hires.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        hires.bplcon0 = 0x9000; // hires, one plane
        hires.ddfstrt = 0x003C;
        hires.ddfstop = 0x00D4;
        assert_eq!(bitplane_grants(&mut hires, 0), 40);
        assert_eq!(hires.ddf_stop_match(), Some(0x00D4));
        assert_eq!(hires.ddf_fetch_end(), Some(0x00DB));
    }

    #[test]
    fn equal_ddf_boundaries_start_an_idle_ocs_run_until_the_hard_stop() {
        let mut agnus = Agnus::new();
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 0x1000; // lores, one plane
        agnus.ddfstrt = 0x0038;
        agnus.ddfstop = 0x0038;

        assert_eq!(
            bitplane_grants(&mut agnus, 0),
            21,
            "an equal pair runs from $38 through the $D8 terminal unit",
        );
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));
        assert_eq!(
            agnus.ddf_stop_match(),
            None,
            "the stop phase sampled the idle sequencer before the start",
        );
        assert_eq!(agnus.ddf_fetch_end(), Some(0x00DF));
    }

    #[test]
    fn ocs_hard_ddfstop_terminates_fetches_beyond_the_fixed_boundary() {
        // The original chipset's fixed right boundary requests a stop
        // at $D8 even when the programmed DDFSTOP comparator is later
        // or has already passed. The selected terminal policy completes
        // the current eight-CCK fetch unit; the HRM's conflicting 49-word
        // hires statement remains an explicit verification question.
        for ddfstop in [0x00D8, 0x00E0, 0x0010] {
            let mut agnus = Agnus::new();
            agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
            agnus.bplcon0 = 0xC000; // hires, four planes
            agnus.ddfstrt = 0x0018;
            agnus.ddfstop = ddfstop;

            let map = plane_map(&mut agnus);
            assert_eq!(agnus.ddf_fetch_end(), Some(0x00DF));
            assert!(
                map[0x00E0..=0x00E2].iter().all(Option::is_none),
                "the OCS hard stop must release every post-unit bus slot"
            );
            assert_eq!(
                agnus.ddf_stop_match(),
                (ddfstop == 0x00D8).then_some(0x00D8),
                "the programmed comparator is recorded only when it coincides with the hard edge"
            );
        }
    }

    #[test]
    fn ocs_hard_ddfstop_samples_the_preexisting_run_before_a_coincident_start() {
        let mut agnus = Agnus::new();
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 0x9000; // hires, one plane
        agnus.ddfstrt = 0x00D8;
        agnus.ddfstop = 0x00E0;
        agnus.vpos = 0x0030;
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.ocs_vertical_diw_active = true;
        agnus.hpos = 0x00D7;

        agnus.tick_cck();

        assert_eq!(agnus.ddf_start_match(), Some(0x00D8));
        assert_eq!(
            agnus.ddf_fetch_end(),
            None,
            "a run created at $D8 must not consume the same hard-stop edge"
        );
    }

    #[test]
    fn ocs_hard_ddfstop_completion_retains_the_active_fetch_phase() {
        let mut agnus = Agnus::new();
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 0xC000; // hires, four planes
        agnus.ddfstrt = 0x001C;
        agnus.ddfstop = 0x00E8;

        let map = plane_map(&mut agnus);

        assert_eq!(agnus.ddf_fetch_end(), Some(0x00E3));
        assert!(
            map[0x00E0..=0x00E2].iter().any(Option::is_some),
            "the $D8 request must complete the phase-shifted terminal unit"
        );
    }

    #[test]
    fn ocs_hard_ddfstop_cannot_be_cancelled_by_a_late_register_write() {
        let mut agnus = Agnus::new();
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 0xC000; // hires, four planes
        agnus.ddfstrt = 0x0018;
        agnus.ddfstop = 0x00E0;

        observe_ddf_start_for_test(&mut agnus);
        tick_to_hpos(&mut agnus, 0x00D8);
        assert_eq!(agnus.ddf_stop_match(), None);
        assert_eq!(agnus.ddf_fetch_end(), Some(0x00DF));

        agnus.write_ddfstop(0x00E4);
        tick_to_hpos(&mut agnus, 0x00E0);
        assert_eq!(agnus.ddf_stop_match(), None);
        assert_eq!(agnus.ddf_fetch_end(), Some(0x00DF));
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);
    }

    /// The whole-line bitplane plane map (which plane, if any, each
    /// hpos fetches) — the fetch grid `bitplane_slot_at` produces.
    fn plane_map(agnus: &mut Agnus) -> Vec<Option<u8>> {
        observe_ddf_start_for_test(agnus);
        let mut map = vec![None; usize::from(agnus.hpos)];
        loop {
            map.push(match agnus.current_slot() {
                SlotOwner::Bitplane(p) => Some(p),
                _ => None,
            });
            if agnus.hpos == 0xE2 {
                break;
            }
            agnus.tick_cck();
        }
        map
    }

    #[test]
    fn ddf_strt_aligns_to_fetch_boundary_per_variant() {
        // OCS ignores DDFSTRT's low 2 bits ($FC, 4-CCK lores boundary):
        // an unaligned $3A produces exactly the same fetch grid as $38.
        let mut aligned = Agnus::new(); // OCS, agnus_id = $0000
        aligned.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        aligned.bplcon0 = 0x6000; // lores, 6 planes
        aligned.ddfstrt = 0x38;
        aligned.ddfstop = 0xD0;

        let mut unaligned = Agnus::new();
        unaligned.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        unaligned.bplcon0 = 0x6000;
        unaligned.ddfstrt = 0x3A; // masks to $38 on OCS
        unaligned.ddfstop = 0xD0;
        assert_eq!(
            plane_map(&mut aligned),
            plane_map(&mut unaligned),
            "OCS masks DDFSTRT to $FC — $3A behaves as $38",
        );

        // ECS/AGA mask only the low bit ($FE, 2-CCK), the granularity
        // SuperHires needs: $3A stays $3A, shifting the grid vs OCS's
        // aligned-down $38.
        let mut ecs = Agnus::new();
        ecs.agnus_id = 0x2000; // ECS discriminator
        ecs.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        ecs.bplcon0 = 0x6000;
        ecs.ddfstrt = 0x3A;
        ecs.ddfstop = 0xD0;
        assert_ne!(
            plane_map(&mut ecs),
            plane_map(&mut aligned),
            "ECS masks $FE — $3A is not aligned down to $38",
        );
    }

    fn configured_hires_ddf(start: u16) -> Agnus {
        let mut agnus = Agnus::new();
        agnus.vpos = 0x0030;
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 0xC000; // hires, four bitplanes
        agnus.write_ddfstrt(start);
        agnus.write_ddfstop(0x00D0);
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.ocs_vertical_diw_active = true;
        agnus
    }

    fn tick_to_hpos(agnus: &mut Agnus, target: u16) {
        while agnus.hpos != target {
            agnus.tick_cck();
        }
    }

    fn enter_ocs_vertical_line(agnus: &mut Agnus, target: u16) {
        let interlace = (agnus.bplcon0 & 0x0004) != 0;
        let frame_lines = agnus.lines_per_frame + u16::from(interlace && agnus.lof);
        assert!(target < frame_lines);
        agnus.vpos = if target == 0 {
            frame_lines - 1
        } else {
            target - 1
        };
        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        assert_eq!(agnus.vpos, target);
        assert_eq!(agnus.hpos, 0);
    }

    fn settle_ocs_diw_write(agnus: &mut Agnus) {
        for _ in 0..8 {
            agnus.tick_cck();
        }
    }

    #[test]
    fn ocs_vertical_diw_latch_starts_inactive_and_follows_line_events() {
        let mut agnus = Agnus::new();
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0xF0C1;
        assert!(!agnus.vertical_diw_active());

        enter_ocs_vertical_line(&mut agnus, 0x002C);
        assert!(agnus.vertical_diw_active());

        enter_ocs_vertical_line(&mut agnus, 0x00EF);
        assert!(
            agnus.vertical_diw_active(),
            "ordinary lines must preserve the comparator history",
        );

        enter_ocs_vertical_line(&mut agnus, 0x00F0);
        assert!(!agnus.vertical_diw_active());
    }

    #[test]
    fn ocs_equal_vertical_boundaries_leave_the_latch_closed() {
        let mut agnus = Agnus::new();
        agnus.diwstrt = 0xE081;
        agnus.diwstop = 0xE0C1;
        agnus.ocs_vertical_diw_active = true;

        enter_ocs_vertical_line(&mut agnus, 0x00E0);

        assert!(
            !agnus.vertical_diw_active(),
            "VSTOP must take precedence over a coincident VSTART",
        );
    }

    #[test]
    fn ocs_implicit_vstop_high_bit_closes_at_12c_not_2c() {
        let mut agnus = Agnus::new();
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;

        enter_ocs_vertical_line(&mut agnus, 0x002C);
        assert!(
            agnus.vertical_diw_active(),
            "VSTOP must decode to $12C when its stored V7 is clear",
        );
        enter_ocs_vertical_line(&mut agnus, 0x012C);
        assert!(!agnus.vertical_diw_active());
    }

    #[test]
    fn ocs_start_after_stop_is_event_history_not_a_wrapping_range() {
        let mut agnus = Agnus::new();
        agnus.diwstrt = 0xF081;
        agnus.diwstop = 0xE0C1;

        enter_ocs_vertical_line(&mut agnus, 0x0030);
        assert!(
            !agnus.vertical_diw_active(),
            "an early-frame position cannot reconstruct a circular range",
        );
        enter_ocs_vertical_line(&mut agnus, 0x00E0);
        assert!(!agnus.vertical_diw_active());
        enter_ocs_vertical_line(&mut agnus, 0x00F0);
        assert!(agnus.vertical_diw_active());

        let final_line = agnus.lines_per_frame - 1;
        enter_ocs_vertical_line(&mut agnus, final_line);
        assert!(
            !agnus.vertical_diw_active(),
            "later original Agnus must close on the final physical field line",
        );
        enter_ocs_vertical_line(&mut agnus, 0);
        assert!(
            !agnus.vertical_diw_active(),
            "the late VSTART cannot survive into the next field",
        );
    }

    #[test]
    fn original_agnus_hard_vertical_blank_uses_revision_specific_lines() {
        for region in [AgnusRegion::Pal, AgnusRegion::Ntsc] {
            for (interlace, lof) in [(false, true), (true, false), (true, true)] {
                let mut a1000 = Agnus::new_a1000_with_region(region);
                a1000.bplcon0 = if interlace { bits::BPLCON0_LACE } else { 0 };
                a1000.lof = lof;
                a1000.diwstrt = 0x0081;
                a1000.diwstop = 0xE0C1;
                a1000.ocs_vertical_diw_active = true;
                let final_line = a1000.lines_per_frame + u16::from(interlace && a1000.lof) - 1;

                enter_ocs_vertical_line(&mut a1000, final_line);
                assert!(
                    a1000.vertical_diw_active(),
                    "A1000 {region:?} interlace={interlace} lof={lof} must not hard-close on the final field line",
                );
                enter_ocs_vertical_line(&mut a1000, 0);
                assert!(
                    !a1000.vertical_diw_active(),
                    "A1000 {region:?} interlace={interlace} lof={lof} line-zero hard close must beat VSTART",
                );

                let mut later = Agnus::new_with_region(region);
                later.bplcon0 = if interlace { bits::BPLCON0_LACE } else { 0 };
                later.lof = lof;
                later.diwstrt = 0x0081;
                later.diwstop = 0xE0C1;
                later.ocs_vertical_diw_active = true;
                enter_ocs_vertical_line(&mut later, final_line);
                assert!(
                    !later.vertical_diw_active(),
                    "later original Agnus {region:?} interlace={interlace} lof={lof} must hard-close on the final field line",
                );
                enter_ocs_vertical_line(&mut later, 0);
                assert!(
                    later.vertical_diw_active(),
                    "later original Agnus {region:?} interlace={interlace} lof={lof} must allow line-zero VSTART to reopen",
                );
            }
        }
    }

    #[test]
    fn a1000_line_zero_diw_write_cannot_override_hard_blank() {
        let mut agnus = Agnus::new_a1000_with_region(AgnusRegion::Pal);
        agnus.diwstop = 0xE0C1;
        agnus.write_diwstrt(0x0081);
        assert!(
            !agnus.vertical_diw_active(),
            "a newly constructed A1000 must begin with line-zero force-off held",
        );

        agnus.diwstrt = 0xF081;
        agnus.ocs_vertical_diw_active = true;

        enter_ocs_vertical_line(&mut agnus, 0);
        assert!(!agnus.vertical_diw_active());

        agnus.write_diwstrt(0x0081);
        assert!(
            !agnus.vertical_diw_active(),
            "a matching current-line VSTART write must not reopen A1000 line zero",
        );
    }

    #[test]
    fn a1000_hard_blank_force_off_releases_on_line_one() {
        let mut agnus = Agnus::new_a1000_with_region(AgnusRegion::Pal);
        agnus.diwstop = 0xE0C1;
        agnus.write_diwstrt(0x0181);
        assert!(!agnus.vertical_diw_active());

        enter_ocs_vertical_line(&mut agnus, 1);

        assert!(
            agnus.vertical_diw_active(),
            "A1000 force-off must release after line zero so line-one VSTART can open",
        );
    }

    #[test]
    fn later_hard_blank_selection_is_held_across_same_line_lace_writes() {
        let mut agnus = Agnus::new_with_region(AgnusRegion::Pal);
        agnus.bplcon0 = bits::BPLCON0_LACE;
        agnus.diwstrt = 0x3781;
        agnus.diwstop = 0xE0C1;
        agnus.ocs_vertical_diw_active = true;

        // LOF starts true, so interlaced PAL has 313 lines. Line 311 is
        // therefore not the final physical line selected for hard blank.
        enter_ocs_vertical_line(&mut agnus, PAL_LINES_PER_FRAME - 1);
        assert!(!agnus.ocs_hard_vertical_blank_active);
        assert!(agnus.vertical_diw_active());

        // Clearing LACE makes the live register geometry look like a
        // 312-line field. It must not retroactively turn the current line
        // into hard blank when a DIW write re-evaluates its comparator.
        agnus.write_bplcon0(0);
        agnus.write_diwstrt(0x3781);
        assert!(!agnus.ocs_hard_vertical_blank_active);
        assert!(
            agnus.vertical_diw_active(),
            "same-line LACE writes must not replace the held hard-blank event",
        );
    }

    #[test]
    fn a1000_line_zero_hard_close_precedes_ddf_start_admission() {
        let mut agnus = Agnus::new_a1000_with_region(AgnusRegion::Pal);
        agnus.diwstrt = 0x0081;
        agnus.diwstop = 0xE0C1;
        agnus.ocs_vertical_diw_active = true;
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 0xC000;
        agnus.write_ddfstrt(0x0018);
        agnus.write_ddfstop(0x00D0);

        enter_ocs_vertical_line(&mut agnus, 0);
        tick_to_hpos(&mut agnus, 0x0018);

        assert!(!agnus.vertical_diw_active());
        assert_eq!(
            agnus.ddf_start_match(),
            None,
            "line-zero VSTART must not admit a DDF run through A1000 hard blank",
        );
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);
    }

    #[test]
    fn a1000_hard_blank_force_off_aborts_an_unstopped_ddf_run() {
        let mut agnus = Agnus::new_a1000_with_region(AgnusRegion::Pal);
        agnus.vpos = 0;
        agnus.diwstrt = 0x0081;
        agnus.diwstop = 0xE0C1;
        agnus.ocs_vertical_diw_active = true;
        agnus.ddf_start_match = Some(0x0038);
        agnus.ddf_fetch_end = None;
        agnus.ocs_ddf_run_aborted = false;

        agnus.evaluate_ocs_vertical_diw_write();

        assert!(!agnus.vertical_diw_active());
        assert!(
            agnus.ocs_ddf_run_aborted(),
            "hard force-off must terminate an active run without an endpoint",
        );
        assert_eq!(
            agnus.ddf_start_match(),
            Some(0x0038),
            "termination preserves the observed display-phase origin",
        );
    }

    #[test]
    fn ocs_current_line_diw_writes_only_apply_matching_events() {
        let mut agnus = Agnus::new();
        agnus.vpos = 0x00B0;
        agnus.write_diwstrt(0x2C81);
        agnus.write_diwstop(0xF0C1);
        settle_ocs_diw_write(&mut agnus);
        assert!(!agnus.vertical_diw_active());

        agnus.write_diwstrt(0xB081);
        settle_ocs_diw_write(&mut agnus);
        assert!(agnus.vertical_diw_active());
        agnus.write_diwstrt(0x2C81);
        agnus.write_diwstop(0xE0C1);
        settle_ocs_diw_write(&mut agnus);
        assert!(
            agnus.vertical_diw_active(),
            "moving non-matching comparators must preserve an open latch",
        );

        agnus.write_diwstop(0xB0C1);
        settle_ocs_diw_write(&mut agnus);
        assert!(!agnus.vertical_diw_active());
        agnus.write_diwstop(0xF0C1);
        settle_ocs_diw_write(&mut agnus);
        assert!(
            !agnus.vertical_diw_active(),
            "moving a stop comparator cannot reconstruct a closed latch",
        );

        agnus.write_diwstrt(0xB081);
        agnus.write_diwstop(0xB0C1);
        settle_ocs_diw_write(&mut agnus);
        assert!(
            !agnus.vertical_diw_active(),
            "a rewritten equal pair must resolve to the stop event",
        );

        agnus.write_diwstop(0xF0C1);
        settle_ocs_diw_write(&mut agnus);
        assert!(
            agnus.vertical_diw_active(),
            "moving VSTOP away must expose the unchanged current-line VSTART",
        );
    }

    #[test]
    fn ddfstrt_write_at_or_behind_beam_cannot_create_a_line_match() {
        for replacement in [0x0038, 0x0040] {
            let mut agnus = configured_hires_ddf(0x0080);
            agnus.hpos = 0x003F;
            agnus.tick_cck();
            assert_eq!(agnus.hpos, 0x0040);

            agnus.write_ddfstrt(replacement);
            tick_to_hpos(&mut agnus, 0x0043);

            assert_eq!(agnus.ddf_start_match(), None);
            assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);
        }
    }

    #[test]
    fn future_ddfstrt_write_matches_at_the_new_comparator() {
        let mut agnus = configured_hires_ddf(0x0080);
        agnus.hpos = 0x003F;
        agnus.tick_cck();
        agnus.write_ddfstrt(0x0048);

        tick_to_hpos(&mut agnus, 0x0047);
        assert_eq!(agnus.ddf_start_match(), None);
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);

        agnus.tick_cck();
        assert_eq!(agnus.hpos, 0x0048);
        assert_eq!(agnus.ddf_start_match(), Some(0x0048));
        assert_eq!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            Some(3),
            "the first hires slot fetches BPL4"
        );
    }

    #[test]
    fn matched_ddfstrt_freezes_the_current_line_fetch_phase() {
        let mut agnus = configured_hires_ddf(0x0038);
        agnus.hpos = 0x0037;
        agnus.tick_cck();
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));

        agnus.write_ddfstrt(0x0080);
        tick_to_hpos(&mut agnus, 0x003F);

        assert_eq!(agnus.ddf_start_match(), Some(0x0038));
        assert_eq!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            Some(0),
            "a later register write cannot rephase an active line"
        );
    }

    #[test]
    fn ddf_start_match_resets_and_observes_the_rewritten_value_next_line() {
        let mut agnus = configured_hires_ddf(0x0038);
        agnus.hpos = 0x0037;
        agnus.tick_cck();
        agnus.write_ddfstrt(0x0040);
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));

        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.ddf_start_match(), None);

        agnus.hpos = 0x003F;
        agnus.tick_cck();
        assert_eq!(agnus.ddf_start_match(), Some(0x0040));
    }

    #[test]
    fn ocs_pre_18_start_depends_on_previous_line_fetch_completion() {
        let mut completed = configured_hires_ddf(0x0018);
        tick_to_hpos(&mut completed, 0x00D7);
        assert_eq!(completed.ddf_fetch_end(), Some(0x00D7));
        assert!(!completed.ocs_ddf_hard_start_open());
        completed.write_ddfstrt(0x0010);
        tick_to_hpos(&mut completed, 0);
        assert!(!completed.ocs_ddf_hard_start_open());
        tick_to_hpos(&mut completed, 0x0010);
        assert_eq!(
            completed.ddf_start_match(),
            None,
            "the prior terminal fetch closed the carried OCS hard-start gate",
        );
        assert_eq!(
            completed.cck_bus_plan().bitplane_dma_fetch_plane,
            None,
            "a pre-$18 comparator missed while the gate was closed is not replayed",
        );
        tick_to_hpos(&mut completed, 0x0018);
        assert!(completed.ocs_ddf_hard_start_open());
        assert_eq!(
            completed.ddf_start_match(),
            None,
            "opening the gate at $18 cannot replay the missed $10 comparator",
        );

        let mut idle = configured_hires_ddf(0x0018);
        tick_to_hpos(&mut idle, 0x00D7);
        assert!(!idle.ocs_ddf_hard_start_open());
        idle.dmacon = 0;
        idle.write_ddfstrt(0x0038);
        tick_to_hpos(&mut idle, 0);
        assert!(!idle.ocs_ddf_hard_start_open());
        tick_to_hpos(&mut idle, 0x0038);
        assert_eq!(idle.ddf_start_match(), None);
        assert!(idle.ocs_ddf_hard_start_open());
        idle.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        idle.write_ddfstrt(0x0010);
        tick_to_hpos(&mut idle, 0);
        assert!(idle.ocs_ddf_hard_start_open());
        tick_to_hpos(&mut idle, 0x0010);
        assert_eq!(
            idle.ddf_start_match(),
            Some(0x0010),
            "an idle line carries the open OCS hard-start gate across EOL",
        );
        assert_eq!(
            idle.cck_bus_plan().bitplane_dma_fetch_plane,
            Some(3),
            "the carried gate permits the first hires four-plane request",
        );
    }

    #[test]
    fn ocs_phase_shifted_terminal_wrap_blocks_next_line_pre_18_start() {
        for next_start in [0x0000, 0x0004, 0x0008, 0x000C, 0x0010, 0x0014] {
            let mut agnus = configured_hires_ddf(0x001C);
            agnus.write_ddfstop(0x00E0);

            tick_to_hpos(&mut agnus, 0x00D8);
            assert_eq!(
                agnus.ddf_fetch_end(),
                Some(0x00E3),
                "the $D8 hard stop freezes the phase-shifted logical endpoint",
            );
            assert!(
                agnus.ocs_ddf_hard_start_open(),
                "the terminal unit has not completed on the current physical line",
            );

            agnus.write_ddfstrt(next_start);
            tick_to_hpos(&mut agnus, 0);
            tick_to_hpos(&mut agnus, next_start);

            assert!(
                !agnus.ocs_ddf_hard_start_open(),
                "the projected terminal result must inhibit the next-line start",
            );
            assert_eq!(
                agnus.ddf_start_match(),
                None,
                "the next-line pre-$18 comparator must not establish a fresh fetch origin",
            );

            tick_to_hpos(&mut agnus, 0x0018);
            assert!(agnus.ocs_ddf_hard_start_open());
            assert_eq!(
                agnus.ddf_start_match(),
                None,
                "opening at $18 cannot replay the missed comparator",
            );
        }
    }

    #[test]
    fn ocs_logical_e3_terminal_is_in_line_on_ntsc_long_line() {
        let mut agnus = Agnus::new_with_region(AgnusRegion::Ntsc);
        agnus.lol = true;
        agnus.vpos = 0x0030;
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 0xC000; // hires, four bitplanes
        agnus.write_ddfstrt(0x001C);
        agnus.write_ddfstop(0x00E0);
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.ocs_vertical_diw_active = true;
        assert_eq!(agnus.current_line_ccks(), 0x00E4);

        tick_to_hpos(&mut agnus, 0x00D8);
        assert_eq!(agnus.ddf_fetch_end(), Some(0x00E3));
        assert!(agnus.ocs_ddf_hard_start_open());

        tick_to_hpos(&mut agnus, 0x00E3);

        assert_eq!(agnus.hpos, 0x00E3);
        assert!(
            !agnus.ocs_ddf_hard_start_open(),
            "the represented in-line $E3 endpoint closes the effective gate before wrap",
        );
    }

    #[test]
    fn ocs_hard_start_and_ddfstrt_18_coincident_event_starts() {
        let mut agnus = configured_hires_ddf(0x0018);
        tick_to_hpos(&mut agnus, 0x00D7);
        assert!(!agnus.ocs_ddf_hard_start_open());
        tick_to_hpos(&mut agnus, 0);

        tick_to_hpos(&mut agnus, 0x0017);
        assert_eq!(agnus.ddf_start_match(), None);
        assert!(!agnus.ocs_ddf_hard_start_open());
        agnus.tick_cck();

        assert!(agnus.ocs_ddf_hard_start_open());
        assert_eq!(agnus.ddf_start_match(), Some(0x0018));
        assert_eq!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            Some(3),
            "the $18 hard-open event precedes the coincident start comparator",
        );
    }

    #[test]
    fn fresh_ocs_preserves_the_first_line_pre_18_start_policy() {
        let mut agnus = configured_hires_ddf(0x0010);
        assert!(
            agnus.ocs_ddf_hard_start_open(),
            "the deterministic constructor policy starts with the gate open",
        );

        tick_to_hpos(&mut agnus, 0x0010);

        assert_eq!(agnus.ddf_start_match(), Some(0x0010));
        assert_eq!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            Some(3),
            "the compatibility default permits a first-line pre-$18 start",
        );
    }

    #[test]
    fn early_ocs_requires_dma_at_match_but_enhanced_agnus_retains_it() {
        let mut ocs = configured_hires_ddf(0x0038);
        ocs.dmacon = 0;
        ocs.hpos = 0x0037;
        ocs.tick_cck();
        assert_eq!(ocs.ddf_start_match(), None);
        ocs.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        ocs.hpos = 0x003F;
        assert_eq!(ocs.cck_bus_plan().bitplane_dma_fetch_plane, None);

        let mut ecs = configured_hires_ddf(0x0038);
        ecs.agnus_id = 0x2000;
        ecs.ocs_ddf_hard_start_open = false;
        ecs.dmacon = 0;
        ecs.hpos = 0x0037;
        ecs.tick_cck();
        assert_eq!(ecs.ddf_start_match(), Some(0x0038));
        ecs.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        ecs.hpos = 0x003F;
        assert_eq!(ecs.cck_bus_plan().bitplane_dma_fetch_plane, Some(0));
    }

    #[test]
    fn ocs_bpl_dma_disable_ends_unstopped_run_before_reenable() {
        for disabled_bit in [DMACON_BPLEN, DMACON_DMAEN] {
            let mut agnus = configured_hires_ddf(0x0038);
            tick_to_hpos(&mut agnus, 0x0040);
            assert_eq!(agnus.ddf_start_match(), Some(0x0038));
            assert!(
                agnus.cck_bus_plan().bitplane_dma_fetch_plane.is_some(),
                "the test must begin with an active bitplane run",
            );

            agnus.write_dmacon(disabled_bit);
            for _ in 0..8 {
                agnus.tick_cck();
            }
            assert!(!agnus.dma_enabled(DMACON_BPLEN));

            agnus.write_dmacon(0x8000 | disabled_bit);
            for _ in 0..8 {
                agnus.tick_cck();
            }
            assert!(agnus.dma_enabled(DMACON_BPLEN));

            assert!(agnus.ocs_ddf_run_aborted());
            assert_eq!(
                agnus.ddf_start_match(),
                Some(0x0038),
                "the observed DDFSTRT remains the frozen display-phase origin",
            );
            assert_eq!(
                agnus.cck_bus_plan().bitplane_dma_fetch_plane,
                None,
                "re-enabling DMA must not resume the stale DDF fetch origin",
            );

            tick_to_hpos(&mut agnus, 0x00D0);
            assert_eq!(agnus.ddf_stop_match(), None);
            assert_eq!(
                agnus.ddf_fetch_end(),
                None,
                "the aborted unstopped run must not manufacture a terminal unit",
            );
            tick_to_hpos(&mut agnus, 0x00D8);
            assert_eq!(agnus.ddf_fetch_end(), None);
            assert!(
                agnus.ocs_ddf_hard_start_open(),
                "DMA disable does not close the original-Agnus start permission",
            );
        }

        let mut before_start = configured_hires_ddf(0x0080);
        before_start.write_dmacon(DMACON_BPLEN);
        assert!(
            !before_start.ocs_ddf_run_aborted(),
            "disabling DMA before DDFSTRT cannot abort a run that does not exist",
        );
    }

    #[test]
    fn ocs_aborted_run_accepts_a_rewritten_future_ddf_start() {
        let mut agnus = configured_hires_ddf(0x0038);
        tick_to_hpos(&mut agnus, 0x0040);
        agnus.write_dmacon(DMACON_BPLEN);
        for _ in 0..8 {
            agnus.tick_cck();
        }
        agnus.write_dmacon(0x8000 | DMACON_BPLEN);
        for _ in 0..8 {
            agnus.tick_cck();
        }

        assert!(agnus.ocs_ddf_run_aborted());
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);

        agnus.write_ddfstrt(0x0060);
        tick_to_hpos(&mut agnus, 0x005F);
        assert_eq!(
            agnus.ddf_start_match(),
            Some(0x0038),
            "the old display phase remains frozen before the new comparator",
        );
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);

        agnus.tick_cck();
        assert_eq!(agnus.hpos, 0x0060);
        assert_eq!(
            agnus.ddf_start_match(),
            Some(0x0060),
            "the future comparator must replace the aborted fetch origin",
        );
        assert!(!agnus.ocs_ddf_run_aborted());

        tick_to_hpos(&mut agnus, 0x00D0);
        assert_eq!(agnus.ddf_stop_match(), Some(0x00D0));
        assert_eq!(agnus.ddf_fetch_end(), Some(0x00D7));
    }

    #[test]
    fn ocs_aborted_run_does_not_replay_a_missed_ddf_start() {
        for replacement in [0x0048, 0x0050] {
            let mut agnus = configured_hires_ddf(0x0038);
            tick_to_hpos(&mut agnus, 0x0040);
            agnus.write_dmacon(DMACON_BPLEN);
            for _ in 0..8 {
                agnus.tick_cck();
            }
            agnus.write_dmacon(0x8000 | DMACON_BPLEN);
            for _ in 0..8 {
                agnus.tick_cck();
            }
            assert_eq!(agnus.hpos, 0x0050);

            agnus.write_ddfstrt(replacement);
            tick_to_hpos(&mut agnus, 0x0058);
            assert!(agnus.ocs_ddf_run_aborted());
            assert_eq!(
                agnus.ddf_start_match(),
                Some(0x0038),
                "a current or behind-beam rewrite cannot replace the old origin",
            );
            assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);
        }

        let mut dma_off_at_match = configured_hires_ddf(0x0038);
        tick_to_hpos(&mut dma_off_at_match, 0x0040);
        dma_off_at_match.write_dmacon(DMACON_BPLEN);
        for _ in 0..8 {
            dma_off_at_match.tick_cck();
        }
        dma_off_at_match.write_ddfstrt(0x0060);
        tick_to_hpos(&mut dma_off_at_match, 0x0068);
        assert!(!dma_off_at_match.dma_enabled(DMACON_BPLEN));
        assert!(dma_off_at_match.ocs_ddf_run_aborted());
        assert_eq!(dma_off_at_match.ddf_start_match(), Some(0x0038));

        dma_off_at_match.write_dmacon(0x8000 | DMACON_BPLEN);
        for _ in 0..8 {
            dma_off_at_match.tick_cck();
        }
        assert!(dma_off_at_match.dma_enabled(DMACON_BPLEN));
        assert!(dma_off_at_match.ocs_ddf_run_aborted());
        assert_eq!(
            dma_off_at_match.ddf_start_match(),
            Some(0x0038),
            "re-enabling DMA cannot replay a comparator crossed while DMA was off",
        );
        assert_eq!(
            dma_off_at_match.cck_bus_plan().bitplane_dma_fetch_plane,
            None,
        );
    }

    #[test]
    fn enhanced_agnus_does_not_consume_ocs_dma_abort_state() {
        let mut agnus = configured_hires_ddf(0x0038);
        agnus.agnus_id = 0x2000;
        tick_to_hpos(&mut agnus, 0x0040);
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));

        agnus.write_dmacon(DMACON_BPLEN);
        for _ in 0..8 {
            agnus.tick_cck();
        }
        agnus.write_dmacon(0x8000 | DMACON_BPLEN);
        for _ in 0..8 {
            agnus.tick_cck();
        }

        assert!(!agnus.ocs_ddf_run_aborted());
        agnus.ocs_ddf_run_aborted = true;
        assert!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane.is_some(),
            "the shared OCS abort state must not alter enhanced behavior",
        );
    }

    #[test]
    fn early_ocs_requires_the_vertical_window_at_ddf_start_match() {
        let mut agnus = configured_hires_ddf(0x0038);
        agnus.vpos = 0x0010;
        agnus.ocs_vertical_diw_active = false;
        assert!(!agnus.vertical_diw_active());

        agnus.hpos = 0x0037;
        agnus.tick_cck();
        assert_eq!(agnus.ddf_start_match(), None);

        agnus.write_diwstrt(0x1081);
        settle_ocs_diw_write(&mut agnus);
        assert!(agnus.vertical_diw_active());
        assert_eq!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            None,
            "entering the display window cannot replay a missed comparator",
        );
    }

    #[test]
    fn ocs_vertical_stop_event_aborts_without_geometric_resume() {
        let mut agnus = configured_hires_ddf(0x0038);
        agnus.vpos = 0x00B0;
        tick_to_hpos(&mut agnus, 0x0040);
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));
        assert!(agnus.vertical_diw_active());

        agnus.write_diwstop(0xB0C1);
        for _ in 0..8 {
            agnus.tick_cck();
        }
        assert!(!agnus.vertical_diw_active());
        assert!(
            agnus.ocs_ddf_run_aborted(),
            "a vertical stop event must terminate the unstopped OCS run",
        );
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));

        agnus.write_diwstop(0xF0C1);
        for _ in 0..8 {
            agnus.tick_cck();
        }
        assert!(
            !agnus.vertical_diw_active(),
            "restoring register geometry without a VSTART event cannot reopen the latch",
        );
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);

        agnus.write_diwstrt(0xB081);
        for _ in 0..8 {
            agnus.tick_cck();
        }
        assert!(agnus.vertical_diw_active());
        assert!(agnus.ocs_ddf_run_aborted());
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));
        assert_eq!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            None,
            "vertical re-opening alone cannot resume the old DDF origin",
        );

        agnus.write_ddfstrt(0x0080);
        tick_to_hpos(&mut agnus, 0x0080);
        assert_eq!(agnus.ddf_start_match(), Some(0x0080));
        assert!(!agnus.ocs_ddf_run_aborted());

        tick_to_hpos(&mut agnus, 0x00D0);
        assert_eq!(agnus.ddf_stop_match(), Some(0x00D0));
        assert_eq!(agnus.ddf_fetch_end(), Some(0x00D7));
    }

    #[test]
    fn ocs_aborted_run_does_not_replay_ddfstart_crossed_while_vertically_closed() {
        let mut agnus = configured_hires_ddf(0x0038);
        agnus.vpos = 0x00B0;
        tick_to_hpos(&mut agnus, 0x0040);

        agnus.write_diwstop(0xB0C1);
        settle_ocs_diw_write(&mut agnus);
        assert!(!agnus.vertical_diw_active());
        assert!(agnus.ocs_ddf_run_aborted());
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));

        agnus.write_ddfstrt(0x0060);
        tick_to_hpos(&mut agnus, 0x0068);
        assert!(
            agnus.ocs_ddf_run_aborted(),
            "an ineligible future comparator must not replace the old origin",
        );
        assert_eq!(agnus.ddf_start_match(), Some(0x0038));

        agnus.write_diwstop(0xF0C1);
        agnus.write_diwstrt(0xB081);
        settle_ocs_diw_write(&mut agnus);
        assert!(agnus.vertical_diw_active());
        assert!(agnus.ocs_ddf_run_aborted());
        assert_eq!(
            agnus.ddf_start_match(),
            Some(0x0038),
            "vertical reopen cannot replay the comparator crossed while closed",
        );
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);
    }

    #[test]
    fn ddfstop_write_at_or_behind_beam_cannot_stop_retroactively() {
        for replacement in [0x0038, 0x0040] {
            let mut agnus = configured_hires_ddf(0x0038);
            agnus.write_ddfstop(0x0080);
            agnus.hpos = 0x0037;
            agnus.tick_cck();
            tick_to_hpos(&mut agnus, 0x0040);

            agnus.write_ddfstop(replacement);
            tick_to_hpos(&mut agnus, 0x004B);

            assert_eq!(
                agnus.cck_bus_plan().bitplane_dma_fetch_plane,
                Some(0),
                "DDFSTOP={replacement:#06x} missed its current-line comparator",
            );
            assert_eq!(agnus.ddf_stop_match(), None);
            assert_eq!(agnus.ddf_fetch_end(), None);
        }
    }

    #[test]
    fn matched_ddfstop_freezes_the_current_line_fetch_end() {
        let mut agnus = configured_hires_ddf(0x0038);
        agnus.write_ddfstop(0x0040);
        agnus.hpos = 0x0037;
        agnus.tick_cck();
        tick_to_hpos(&mut agnus, 0x0040);

        agnus.write_ddfstop(0x0080);
        tick_to_hpos(&mut agnus, 0x004B);

        assert_eq!(agnus.ddf_stop_match(), Some(0x0040));
        assert_eq!(agnus.ddf_fetch_end(), Some(0x0047));
        assert_eq!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            None,
            "a later register write cannot cancel an observed stop",
        );
    }

    #[test]
    fn future_ddfstop_write_stops_after_the_new_comparator() {
        let mut agnus = configured_hires_ddf(0x0038);
        agnus.write_ddfstop(0x0080);
        agnus.hpos = 0x0037;
        agnus.tick_cck();
        tick_to_hpos(&mut agnus, 0x0040);
        agnus.write_ddfstop(0x0048);

        tick_to_hpos(&mut agnus, 0x004F);
        assert_eq!(agnus.ddf_stop_match(), Some(0x0048));
        assert_eq!(agnus.ddf_fetch_end(), Some(0x004F));
        assert_eq!(
            agnus.cck_bus_plan().bitplane_dma_fetch_plane,
            Some(0),
            "the final fetch unit completes after DDFSTOP",
        );

        tick_to_hpos(&mut agnus, 0x0053);
        assert_eq!(agnus.cck_bus_plan().bitplane_dma_fetch_plane, None);
    }

    #[test]
    fn ddf_stop_match_and_fetch_end_reset_at_line_start() {
        let mut agnus = configured_hires_ddf(0x0038);
        agnus.write_ddfstop(0x0040);
        agnus.hpos = 0x0037;
        agnus.tick_cck();
        tick_to_hpos(&mut agnus, 0x0040);
        assert_eq!(agnus.ddf_stop_match(), Some(0x0040));
        assert_eq!(agnus.ddf_fetch_end(), Some(0x0047));

        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.ddf_start_match(), None);
        assert_eq!(agnus.ddf_stop_match(), None);
        assert_eq!(agnus.ddf_fetch_end(), None);
    }

    #[test]
    fn cck_bus_plan_reports_audio_service_grant() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x0D;
        agnus.dmacon = DMACON_DMAEN | DMACON_AUD0EN;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Audio(0));
        assert!(!plan.disk_dma_slot_granted);
        assert_eq!(plan.sprite_dma_service_channel, None);
        assert_eq!(plan.audio_dma_service_channel, Some(0));
        assert_eq!(plan.bitplane_dma_fetch_plane, None);
        assert!(!plan.copper_dma_slot_granted);
        assert!(!plan.cpu_chip_bus_granted);
        assert!(!plan.blitter_chip_bus_granted);
        assert!(!plan.blitter_dma_progress_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Advance
        );
    }

    #[test]
    fn cck_bus_plan_reports_copper_grant_and_conditional_return_policy() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x1C; // even, variable-slot region
        agnus.dmacon = DMACON_DMAEN | DMACON_COPEN;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Copper);
        assert!(!plan.disk_dma_slot_granted);
        assert_eq!(plan.sprite_dma_service_channel, None);
        assert_eq!(plan.audio_dma_service_channel, None);
        assert_eq!(plan.bitplane_dma_fetch_plane, None);
        assert!(plan.copper_dma_slot_granted);
        assert!(!plan.cpu_chip_bus_granted);
        assert!(!plan.blitter_chip_bus_granted);
        assert!(!plan.blitter_dma_progress_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::CopperFetchConditional
        );
    }

    #[test]
    fn cck_bus_plan_reports_bitplane_grant_and_stall_policy() {
        let mut agnus = Agnus::new();
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN | DMACON_COPEN;
        agnus.bplcon0 = 1 << 12; // 1 bitplane enabled
        agnus.ddfstrt = 0x1C;
        agnus.ddfstop = 0x1C;
        observe_ddf_start_for_test(&mut agnus);
        agnus.hpos = 0x23; // ddfstrt + 7 => BPL1 slot in lowres fetch group

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Bitplane(0));
        assert!(!plan.disk_dma_slot_granted);
        assert_eq!(plan.sprite_dma_service_channel, None);
        assert_eq!(plan.audio_dma_service_channel, None);
        assert_eq!(plan.bitplane_dma_fetch_plane, Some(0));
        assert!(!plan.copper_dma_slot_granted);
        assert!(!plan.cpu_chip_bus_granted);
        assert!(!plan.blitter_chip_bus_granted);
        assert!(!plan.blitter_dma_progress_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Stall
        );
    }

    fn configure_early_bitplane_sprite_overlap(agnus: &mut Agnus) {
        agnus.hpos = 0x23; // BPL1 and sprite 3 overlap
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN;
        agnus.bplcon0 = 1 << 12;
        agnus.ddfstrt = 0x1C;
        agnus.ddfstop = 0x1C;
        agnus.diwstrt = 0x3010;
        agnus.diwstop = 0xA020;
    }

    #[test]
    fn bitplane_slot_is_released_outside_ocs_vertical_window() {
        let mut agnus = Agnus::new();
        configure_early_bitplane_sprite_overlap(&mut agnus);

        agnus.vpos = 0x20;
        let outside = agnus.cck_bus_plan();
        assert_eq!(outside.slot_owner, SlotOwner::Cpu);
        assert_eq!(outside.bitplane_dma_fetch_plane, None);
        assert!(outside.cpu_chip_bus_granted);

        agnus.vpos = 0x30;
        agnus.write_diwstrt(0x3010);
        agnus.hpos = 0x1B;
        agnus.tick_cck();
        agnus.hpos = 0x23;
        let inside = agnus.cck_bus_plan();
        assert_eq!(inside.slot_owner, SlotOwner::Bitplane(0));
        assert_eq!(inside.bitplane_dma_fetch_plane, Some(0));
    }

    #[test]
    fn requesting_sprite_inherits_vertically_inactive_bitplane_slot() {
        let mut agnus = Agnus::new();
        configure_early_bitplane_sprite_overlap(&mut agnus);
        agnus.dmacon |= 0x0020; // SPREN
        agnus.spr_vstop[3] = 0x20; // sprite 3 requests its control words

        agnus.vpos = 0x20;
        let outside = agnus.cck_bus_plan();
        assert_eq!(outside.slot_owner, SlotOwner::Sprite(3));
        assert_eq!(outside.sprite_dma_service_channel, Some(3));
        assert_eq!(outside.bitplane_dma_fetch_plane, None);

        agnus.vpos = 0x30;
        agnus.write_diwstrt(0x3010);
        agnus.hpos = 0x1B;
        agnus.tick_cck();
        agnus.hpos = 0x23;
        let inside = agnus.cck_bus_plan();
        assert_eq!(inside.slot_owner, SlotOwner::Bitplane(0));
        assert_eq!(inside.sprite_dma_service_channel, None);
    }

    #[test]
    fn cck_bus_plan_reports_hires_bitplane_grant_at_group_end() {
        let mut agnus = Agnus::new();
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN | DMACON_COPEN;
        agnus.bplcon0 = 0x8000 | (1 << 12); // HIRES + 1 bitplane
        agnus.ddfstrt = 0x40;
        agnus.ddfstop = 0x40;
        observe_ddf_start_for_test(&mut agnus);
        agnus.hpos = 0x43; // ddfstrt + 3 => BPL1 slot in hires fetch group

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Bitplane(0));
        assert_eq!(plan.bitplane_dma_fetch_plane, Some(0));
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Stall
        );
    }

    #[test]
    fn cck_bus_plan_reports_cpu_chip_bus_grant_on_free_slot() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x35; // odd cell after sprites (0x33), before DDF (0x38):
        // no chipset claimant, and copper takes only even
        // cells, so this is a genuine CPU slot.
        agnus.dmacon = DMACON_DMAEN | DMACON_COPEN | DMACON_BPLEN;
        agnus.bplcon0 = 1 << 12;
        agnus.ddfstrt = 0x38;
        agnus.ddfstop = 0xD8;
        agnus.blitter_busy = false;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Cpu);
        assert!(!plan.disk_dma_slot_granted);
        assert_eq!(plan.audio_dma_service_channel, None);
        assert_eq!(plan.bitplane_dma_fetch_plane, None);
        assert!(!plan.copper_dma_slot_granted);
        assert!(plan.cpu_chip_bus_granted);
        assert!(
            !plan.blitter_chip_bus_granted,
            "blitter per-CCK slot grants are not modeled yet"
        );
        assert!(!plan.blitter_dma_progress_granted);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Advance
        );
    }

    #[test]
    fn cck_bus_plan_reports_blitter_nasty_grant_on_cpu_slot() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x00; // free slot
        agnus.blitter_busy = true;
        agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN | DMACON_BLTPRI;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Cpu);
        assert!(
            !plan.cpu_chip_bus_granted,
            "CPU should lose free slot to blitter in nasty mode"
        );
        assert!(
            plan.blitter_chip_bus_granted,
            "blitter should claim free slot in nasty mode"
        );
        assert!(plan.blitter_dma_progress_granted);
    }

    #[test]
    fn cck_bus_plan_blitter_busy_without_nasty_does_not_take_cpu_slot() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x00; // free slot
        agnus.blitter_busy = true;
        agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN; // BLTPRI clear

        let plan = agnus.cck_bus_plan();
        assert!(plan.cpu_chip_bus_granted);
        assert!(!plan.blitter_chip_bus_granted);
        assert!(
            plan.blitter_dma_progress_granted,
            "non-nasty blitter should still progress on free slots"
        );
    }

    #[test]
    fn blitter_scheduler_ops_count_down_and_require_progress() {
        let mut agnus = Agnus::new();
        agnus.bltcon0 = 0x0100; // D write only => 1 DMA op/word
        agnus.bltsize = (1 << 6) | 2; // height=1, width=2 => budget=2
        agnus.start_blit();

        assert!(agnus.blitter_busy);
        assert!(agnus.blitter_exec_pending);
        assert_eq!(agnus.blitter_ccks_remaining, 2);

        assert_eq!(
            agnus.tick_blitter_scheduler_op(false),
            BlitterProgress::NoProgress,
            "no progress when bus grant is withheld",
        );
        assert_eq!(agnus.blitter_ccks_remaining, 2);

        assert_eq!(
            agnus.tick_blitter_scheduler_op(true),
            BlitterProgress::Startup,
        );
        assert_eq!(
            agnus.tick_blitter_scheduler_op(true),
            BlitterProgress::Startup,
        );
        assert_eq!(
            agnus.blitter_ccks_remaining, 2,
            "two accepted startup CCKs must not consume D operations",
        );

        assert_eq!(
            agnus.tick_blitter_scheduler_op(true),
            BlitterProgress::Operation(BlitterDmaOp::WriteD),
        );
        assert_eq!(agnus.blitter_ccks_remaining, 1);

        assert!(
            agnus.blitter_busy,
            "the request scheduler cannot retire completion without execution",
        );
    }

    #[test]
    fn blitter_total_ops_scales_with_enabled_area_channels() {
        let mut agnus = Agnus::new();
        agnus.bltcon0 = 0x0800 | 0x0200 | 0x0100; // A read + C read + D write
        agnus.bltsize = (1 << 6) | 3; // height=1, width=3 words
        agnus.start_blit();

        assert_eq!(
            agnus.blitter_ccks_remaining, 9,
            "3 words * (A+C+D) => 9 DMA-op grants"
        );

        // First word should request ReadA, then ReadC, then WriteD.
        assert_eq!(agnus.next_blitter_dma_request(), Some(BlitterDmaOp::ReadA));
        agnus.consume_blitter_dma_op(BlitterDmaOp::ReadA);
        assert_eq!(agnus.next_blitter_dma_request(), Some(BlitterDmaOp::ReadC));
        agnus.consume_blitter_dma_op(BlitterDmaOp::ReadC);
        assert_eq!(agnus.next_blitter_dma_request(), Some(BlitterDmaOp::WriteD));
    }

    #[test]
    fn blitter_line_mode_requests_c_then_d_per_step() {
        let mut agnus = Agnus::new();
        agnus.bltcon1 = 0x0001; // LINE mode
        agnus.bltsize = (4 << 6) | 2; // length=4, width field ignored in line mode
        agnus.start_blit();

        assert_eq!(
            agnus.blitter_ccks_remaining, 8,
            "4 line steps * (C read + D write) => 8 DMA-op grants"
        );

        // First line step should request ReadC, then WriteD.
        assert_eq!(agnus.next_blitter_dma_request(), Some(BlitterDmaOp::ReadC));
        agnus.consume_blitter_dma_op(BlitterDmaOp::ReadC);
        assert_eq!(agnus.next_blitter_dma_request(), Some(BlitterDmaOp::WriteD));
    }

    #[test]
    fn cck_bus_plan_reports_disk_service_grant() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x07;
        agnus.dmacon = DMACON_DMAEN | 0x0010; // DSKEN

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Disk);
        assert!(plan.disk_dma_slot_granted);
        assert_eq!(plan.sprite_dma_service_channel, None);
        assert_eq!(plan.audio_dma_service_channel, None);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Stall
        );
    }

    #[test]
    fn cck_bus_plan_reports_sprite_service_grant() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x15; // first sprite DMA slot => sprite 0
        agnus.vpos = 30;
        agnus.dmacon = DMACON_DMAEN | 0x0020; // SPREN
        agnus.spr_vstop[0] = 30; // control-word request

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Sprite(0));
        assert!(!plan.disk_dma_slot_granted);
        assert_eq!(plan.sprite_dma_service_channel, Some(0));
        assert_eq!(plan.audio_dma_service_channel, None);
        assert_eq!(
            plan.paula_return_progress_policy,
            PaulaReturnProgressPolicy::Stall
        );
    }

    #[test]
    fn active_sprite_requests_its_scheduled_slot() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x15;
        agnus.vpos = 30;
        agnus.dmacon = DMACON_DMAEN | 0x0020; // SPREN
        agnus.spr_vstop[0] = 50;
        agnus.spr_dma_on[0] = true;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Sprite(0));
        assert_eq!(plan.sprite_dma_service_channel, Some(0));
    }

    #[test]
    fn idle_sprite_opportunity_allows_nasty_blitter_progress() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x15;
        agnus.vpos = 30;
        agnus.dmacon = DMACON_DMAEN | 0x0020 | 0x0040 | 0x0400; // SPREN | BLTEN | BLTPRI
        agnus.spr_vstop[0] = 50;
        agnus.blitter_busy = true;

        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Cpu);
        assert_eq!(plan.sprite_dma_service_channel, None);
        assert!(plan.blitter_dma_progress_granted);
        assert!(plan.blitter_chip_bus_granted);
        assert!(!plan.cpu_chip_bus_granted);
    }

    // ---------- region + line-length alternation ----------

    /// VPOSR upper byte is what Kickstart reads to identify the
    /// Agnus revision. Locks the bit-positions so future storage
    /// refactors can't silently regress to the pre-Stage AE-j state
    /// where every chipset reported `$0000` in the ID bits.
    #[test]
    fn vposr_reports_original_agnus_region_identity() {
        let pal = Agnus::new_with_region(AgnusRegion::Pal);
        // PAL 8371/8367: upper byte $00 → u16 $0000.
        // LOF starts set + vpos bit 8 zero at reset → bit 15 = 1, bit 0 = 0.
        assert_eq!(pal.vposr() & 0x7F00, 0x0000);

        let ntsc = Agnus::new_with_region(AgnusRegion::Ntsc);
        // NTSC 8370/8361: bits 14-8 = `0010000` → upper byte $10.
        assert_eq!(ntsc.vposr() & 0x7F00, 0x1000);
    }

    #[test]
    fn vposr_identity_cannot_distinguish_a1000_from_later_original_agnus() {
        for region in [AgnusRegion::Pal, AgnusRegion::Ntsc] {
            let a1000 = Agnus::new_a1000_with_region(region);
            let later = Agnus::new_with_region(region);

            assert_eq!(a1000.vposr() & 0x7F00, later.vposr() & 0x7F00);
            assert_eq!(a1000.original_revision(), OriginalAgnusRevision::A1000);
            assert_eq!(later.original_revision(), OriginalAgnusRevision::Later);
            assert_ne!(a1000.original_revision(), later.original_revision());
        }
    }

    #[test]
    fn pal_default_keeps_lol_zero_and_lines_at_227() {
        let agnus = Agnus::new_with_region(AgnusRegion::Pal);
        assert_eq!(agnus.region, AgnusRegion::Pal);
        assert_eq!(agnus.lines_per_frame, PAL_LINES_PER_FRAME);
        // Later 8371 PAL original Agnus shares the 8367's VPOSR ID,
        // stored pre-shifted into bits 14-8.
        assert_eq!(agnus.agnus_id, 0x0000);
        assert!(!agnus.lol);
        assert!(!agnus.lol_toggle);
        assert_eq!(agnus.current_line_ccks(), PAL_CCKS_PER_LINE);
    }

    #[test]
    fn copper_horizontal_comparator_follows_current_line_parity() {
        let mut pal = Agnus::new_with_region(AgnusRegion::Pal);
        for (physical, comparator) in [
            (0x00DD, 0x00DF),
            (0x00DE, 0x00E0),
            (0x00DF, 0x00E1),
            (0x00E0, 0x0000),
            (0x00E1, 0x0001),
            (0x00E2, 0x0002),
        ] {
            pal.hpos = physical;
            assert_eq!(pal.copper_comparator_hpos(), comparator);
        }

        let mut ntsc = Agnus::new_with_region(AgnusRegion::Ntsc);
        ntsc.lol = true;
        assert_eq!(ntsc.current_line_ccks(), NTSC_CCKS_PER_LINE_LONG);
        for (physical, comparator) in [
            (0x00E0, 0x00E2),
            (0x00E1, 0x00E3),
            (0x00E2, 0x0000),
            (0x00E3, 0x0001),
        ] {
            ntsc.hpos = physical;
            assert_eq!(ntsc.copper_comparator_hpos(), comparator);
        }
    }

    #[test]
    fn ntsc_starts_short_and_alternates_per_line() {
        let mut agnus = Agnus::new_with_region(AgnusRegion::Ntsc);
        assert_eq!(agnus.region, AgnusRegion::Ntsc);
        assert_eq!(agnus.lines_per_frame, NTSC_LINES_PER_FRAME);
        // Later 8370 NTSC original Agnus shares the 8361's $1000 ID.
        assert_eq!(agnus.agnus_id, 0x1000);
        assert!(!agnus.lol);
        assert!(agnus.lol_toggle);
        // First line = short (227).
        assert_eq!(agnus.current_line_ccks(), NTSC_CCKS_PER_LINE_SHORT);

        // Tick through the first 227 CCKs — still on line 0 (short).
        for _ in 0..NTSC_CCKS_PER_LINE_SHORT {
            agnus.tick_cck();
        }
        // hpos wrapped, vpos advanced, lol flipped to true → second
        // line is long (228).
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.vpos, 1);
        assert!(agnus.lol);
        assert_eq!(agnus.current_line_ccks(), NTSC_CCKS_PER_LINE_LONG);

        // Tick through the long line: 228 CCKs.
        for _ in 0..NTSC_CCKS_PER_LINE_LONG {
            agnus.tick_cck();
        }
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.vpos, 2);
        assert!(!agnus.lol);
        assert_eq!(agnus.current_line_ccks(), NTSC_CCKS_PER_LINE_SHORT);
    }

    #[test]
    fn ntsc_full_frame_totals_match_canonical_count() {
        // 262 lines, alternating short / long starting from short.
        // Even line indices (0, 2, 4, …, 260) are short; odd indices
        // are long. 131 of each → 131*227 + 131*228 = 59,605 CCK.
        let mut agnus = Agnus::new_with_region(AgnusRegion::Ntsc);
        let mut total_ccks: u32 = 0;
        let starting_vbl = agnus.vbl_count;
        // Run until vpos wraps once (one full frame).
        while agnus.vbl_count == starting_vbl {
            agnus.tick_cck();
            total_ccks += 1;
        }
        assert_eq!(total_ccks, NTSC_CCKS_PER_FRAME);
        assert_eq!(total_ccks, 59_605);
    }

    #[test]
    fn pal_full_frame_totals_match_canonical_count() {
        // 312 lines × 227 = 70,824 CCK.
        let mut agnus = Agnus::new_with_region(AgnusRegion::Pal);
        let mut total_ccks: u32 = 0;
        let starting_vbl = agnus.vbl_count;
        while agnus.vbl_count == starting_vbl {
            agnus.tick_cck();
            total_ccks += 1;
        }
        assert_eq!(total_ccks, PAL_CCKS_PER_FRAME);
        assert_eq!(total_ccks, 70_824);
    }

    #[test]
    fn pal_lol_stays_false_for_full_frame() {
        // PAL has lol_toggle = false, so lol must remain false for
        // every line of the frame.
        let mut agnus = Agnus::new_with_region(AgnusRegion::Pal);
        let starting_vbl = agnus.vbl_count;
        while agnus.vbl_count == starting_vbl {
            assert!(!agnus.lol, "PAL must never set lol");
            agnus.tick_cck();
        }
    }

    // ── Sprite DMA state machine (gap #162) ──────────────────────────

    fn sprite_dma_agnus() -> Agnus {
        let mut agnus = Agnus::new();
        agnus.dmacon = 0x0200 | 0x0020; // DMAEN | SPREN
        agnus
    }

    #[test]
    fn sprite_control_fetch_latches_vstart_vstop_and_bumps_pointer() {
        let mut agnus = sprite_dma_agnus();
        agnus.spr_pt[0] = 0x1000;
        agnus.vpos = 30; // past the reset line, so fetches are not VB-suppressed
        agnus.spr_vstop[0] = 30; // vpos == vstop → control fetch
        // word 0 = SPRxPOS: VSTART[7:0] = bits 15-8 = 0x28 (40).
        assert_eq!(
            agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x2800),
            Some((true, 0x2800))
        );
        assert_eq!(agnus.spr_pt[0], 0x1002, "pointer advances one word");
        assert_eq!(agnus.sprite_vstart(0), 40);
        assert!(!agnus.sprite_dma_on(0), "control fetch turns DMA off");
        // word 1 = SPRxCTL: VSTOP[7:0] = bits 15-8 = 0x32 (50); bit8s clear.
        assert_eq!(
            agnus.service_sprite_dma_cyc(0, true, 1, |_| 0x3200),
            Some((true, 0x3200))
        );
        assert_eq!(agnus.spr_pt[0], 0x1004);
        assert_eq!(agnus.sprite_vstop(0), 50);
        assert_eq!(agnus.sprite_vstart(0), 40);
    }

    #[test]
    fn sprite_ctl_high_bits_extend_vstart_vstop_to_9_bits() {
        let mut agnus = sprite_dma_agnus();
        agnus.vpos = 30;
        agnus.spr_vstop[0] = 30;
        let _ = agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x0100); // VSTART low = 1
        // CTL: VSTOP low = 2; bit2 (SV8)=1 → VSTART|=0x100; bit1 (EV8)=1 → VSTOP|=0x100.
        let _ = agnus.service_sprite_dma_cyc(0, true, 1, |_| 0x0200 | 0x04 | 0x02);
        assert_eq!(agnus.sprite_vstart(0), 0x101, "VSTART = 0x100 | 1");
        assert_eq!(agnus.sprite_vstop(0), 0x102, "VSTOP = 0x100 | 2");
    }

    #[test]
    fn sprite_ctl_enhanced_vertical_bits_are_ignored_on_ocs() {
        let mut agnus = sprite_dma_agnus();

        // CTL bit 6 is VSTART[9] only on enhanced Agnus, while bits 2/1
        // remain the OCS VSTART[8]/VSTOP[8] extensions. Write CTL before
        // POS to prove the later POS write preserves exactly the OCS high
        // bit and does not retain the enhanced bit.
        agnus.poke_sprite_ctl(0, 0x0246);
        agnus.poke_sprite_pos(0, 0x0100);

        assert_eq!(agnus.sprite_vstart(0), 0x101);
        assert_eq!(agnus.sprite_vstop(0), 0x102);

        // CTL bit 5 is likewise ignored rather than becoming VSTOP[9].
        agnus.poke_sprite_ctl(0, 0x0226);
        assert_eq!(agnus.sprite_vstart(0), 0x101);
        assert_eq!(agnus.sprite_vstop(0), 0x102);

        // The DMA-fetched control path uses the same early-OCS width.
        let mut dma_agnus = sprite_dma_agnus();
        dma_agnus.spr_pt[0] = 0x1000;
        dma_agnus.vpos = 30;
        dma_agnus.poke_sprite_ctl(0, 30 << 8);
        assert_eq!(
            dma_agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x0100),
            Some((true, 0x0100))
        );
        assert_eq!(
            dma_agnus.service_sprite_dma_cyc(0, true, 1, |_| 0x0266),
            Some((true, 0x0266))
        );
        assert_eq!(dma_agnus.sprite_vstart(0), 0x101);
        assert_eq!(dma_agnus.sprite_vstop(0), 0x102);
    }

    #[test]
    fn fat_agnus_8372a_fetches_and_compares_ten_bit_sprite_vertical_coordinates() {
        let mut agnus = Agnus::new_fat_agnus_with_region(AgnusRegion::Pal);
        agnus.dmacon = 0x0220; // DMAEN | SPREN
        agnus.spr_pt[0] = 0x1000;
        agnus.vpos = 30;
        agnus.poke_sprite_ctl(0, 30 << 8);

        assert_eq!(
            agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x0100),
            Some((true, 0x0100))
        );
        assert_eq!(
            agnus.service_sprite_dma_cyc(0, true, 1, |_| 0x0266),
            Some((true, 0x0266))
        );

        assert_eq!(agnus.agnus_id, 0x2000);
        assert_eq!(agnus.sprite_vstart(0), 0x301);
        assert_eq!(agnus.sprite_vstop(0), 0x302);

        // Give the fixed-timing core a long synthetic field so the
        // enhanced coordinates can drive the normal line-entry state
        // machine without involving the ECS programmable-beam wrapper.
        agnus.lines_per_frame = 0x0400;
        agnus.vpos = 0x0300;
        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        assert_eq!(agnus.vpos, 0x0301);
        assert!(agnus.sprite_dma_on(0));

        agnus.hpos = agnus.current_line_ccks() - 1;
        agnus.tick_cck();
        assert_eq!(agnus.vpos, 0x0302);
        assert!(!agnus.sprite_dma_on(0));
    }

    #[test]
    fn dma_control_latches_reevaluate_the_current_line() {
        let mut agnus = sprite_dma_agnus();
        agnus.vpos = 40;
        agnus.spr_vstop[0] = 40;

        assert_eq!(
            agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x2800),
            Some((true, 0x2800))
        );
        assert!(!agnus.sprite_dma_on(0));
        assert_eq!(
            agnus.service_sprite_dma_cyc(0, true, 1, |_| 0x3C00),
            Some((true, 0x3C00))
        );
        assert_eq!(agnus.spr_vstart[0], 40);
        assert_eq!(agnus.spr_vstop[0], 60);
        assert!(
            agnus.sprite_dma_on(0),
            "new VSTART matching the current line activates after CTL latches"
        );
    }

    #[test]
    fn sprite_data_fetch_when_active() {
        let mut agnus = sprite_dma_agnus();
        agnus.spr_pt[1] = 0x2000;
        agnus.vpos = 45;
        agnus.spr_vstop[1] = 50;
        agnus.spr_dma_on[1] = true;
        assert_eq!(
            agnus.service_sprite_dma_cyc(1, false, 1, |_| 0xAAAA),
            Some((false, 0xAAAA))
        );
        assert_eq!(
            agnus.service_sprite_dma_cyc(1, true, 1, |_| 0x5555),
            Some((false, 0x5555))
        );
        assert_eq!(agnus.spr_pt[1], 0x2004);
    }

    #[test]
    fn sprite_idle_when_off_does_not_fetch_or_advance() {
        let mut agnus = sprite_dma_agnus();
        agnus.spr_pt[2] = 0x3000;
        agnus.vpos = 30;
        agnus.spr_vstop[2] = 50;
        agnus.spr_dma_on[2] = false;
        assert_eq!(agnus.service_sprite_dma_cyc(2, false, 1, |_| 0xFFFF), None);
        assert_eq!(
            agnus.spr_pt[2], 0x3000,
            "idle slot leaves the pointer alone"
        );
    }

    #[test]
    fn sprite_vstop_wins_over_active_flag() {
        let mut agnus = sprite_dma_agnus();
        agnus.spr_pt[3] = 0x4000;
        agnus.vpos = 50;
        agnus.spr_vstop[3] = 50;
        agnus.spr_dma_on[3] = true; // active...
        // ...but vpos == vstop forces a control fetch, not data.
        assert_eq!(
            agnus.service_sprite_dma_cyc(3, false, 1, |_| 0x1234),
            Some((true, 0x1234))
        );
        assert!(!agnus.sprite_dma_on(3));
    }

    #[test]
    fn update_sprite_dma_activates_at_vstart_deactivates_at_vstop() {
        let mut agnus = sprite_dma_agnus();
        agnus.spr_vstart[0] = 40;
        agnus.spr_vstop[0] = 60;
        agnus.vpos = 39;
        agnus.update_sprite_dma(PAL_LINES_PER_FRAME);
        assert!(!agnus.sprite_dma_on(0));
        agnus.vpos = 40;
        agnus.update_sprite_dma(PAL_LINES_PER_FRAME);
        assert!(agnus.sprite_dma_on(0), "activates at VSTART");
        agnus.vpos = 50;
        agnus.update_sprite_dma(PAL_LINES_PER_FRAME);
        assert!(agnus.sprite_dma_on(0), "stays on between");
        agnus.vpos = 60;
        agnus.update_sprite_dma(PAL_LINES_PER_FRAME);
        assert!(!agnus.sprite_dma_on(0), "deactivates at VSTOP");
    }

    #[test]
    fn regional_reset_line_requests_control_without_overwriting_vstop() {
        for (region, reset_line, frame_lines) in [
            (AgnusRegion::Pal, 25, PAL_LINES_PER_FRAME),
            (AgnusRegion::Ntsc, 20, NTSC_LINES_PER_FRAME),
        ] {
            let mut agnus = Agnus::new_with_region(region);
            agnus.dmacon = DMACON_DMAEN | 0x0020; // DMAEN | SPREN
            agnus.hpos = 0x15;
            agnus.vpos = reset_line;
            agnus.spr_pt[0] = 0x1000;
            agnus.spr_vstop[0] = 99;

            agnus.update_sprite_dma(frame_lines);

            assert_eq!(
                agnus.sprite_vstop(0),
                99,
                "the reset event must not manufacture a VSTOP match"
            );
            assert_eq!(agnus.current_slot(), SlotOwner::Sprite(0));
            assert_eq!(
                agnus.service_sprite_dma_cyc(0, false, 1, |_| 0x4000),
                Some((true, 0x4000))
            );
        }
    }

    #[test]
    fn sprite_dma_suppressed_during_vertical_blank() {
        let mut agnus = sprite_dma_agnus();
        agnus.spr_pt[0] = 0x1000;
        agnus.vpos = 0;
        agnus.spr_vstop[0] = 0; // would control-fetch if not VB-suppressed
        assert_eq!(agnus.service_sprite_dma_cyc(0, false, 1, |_| 0xBEEF), None);
        assert_eq!(agnus.spr_pt[0], 0x1000, "no fetch in vertical blank");
    }

    #[test]
    fn regional_reset_line_precedes_active_data_and_blank_suppression() {
        for (region, reset_line) in [(AgnusRegion::Pal, 25), (AgnusRegion::Ntsc, 20)] {
            let mut agnus = Agnus::new_with_region(region);
            agnus.dmacon = DMACON_DMAEN | 0x0020; // DMAEN | SPREN
            agnus.hpos = 0x15;
            agnus.spr_pt[0] = 0x1000;
            agnus.spr_vstop[0] = 50;
            agnus.spr_dma_on[0] = true;

            agnus.vpos = reset_line - 1;
            assert_eq!(agnus.current_slot(), SlotOwner::Cpu);
            assert_eq!(agnus.service_sprite_dma_cyc(0, false, 1, |_| 0xBEEF), None);
            assert_eq!(agnus.spr_pt[0], 0x1000);

            agnus.vpos = reset_line;
            assert_eq!(agnus.current_slot(), SlotOwner::Sprite(0));
            assert_eq!(
                agnus.service_sprite_dma_cyc(0, false, 1, |_| 0xBEEF),
                Some((true, 0xBEEF)),
                "the regional reset event fetches control, not stale data"
            );
            assert_eq!(agnus.spr_pt[0], 0x1002);
        }
    }

    #[test]
    fn wide_sprite_data_fetch_assembles_fmode_words_msb_first() {
        // #99: an FMODE 32-bit sprite fetches 2 consecutive words per
        // SPRxDATA access, assembled MSB-first (first word = leftmost
        // pixels) into the 64-bit serial-shifter payload.
        let mut agnus = sprite_dma_agnus();
        agnus.spr_pt[1] = 0x2000;
        agnus.vpos = 45;
        agnus.spr_vstop[1] = 50;
        agnus.spr_dma_on[1] = true;
        let words = [0xAAAAu16, 0xBBBBu16];
        let mut i = 0;
        let fetched = agnus.service_sprite_dma_cyc(1, false, 2, |_addr| {
            let w = words[i];
            i += 1;
            w
        });
        assert_eq!(
            fetched,
            Some((false, 0xAAAA_BBBB)),
            "two words assemble MSB-first into a 32-bit payload"
        );
        assert_eq!(agnus.spr_pt[1], 0x2004, "pointer advances by 2 words");

        // 64-bit (width 4): four words fill the full u64.
        let mut agnus = sprite_dma_agnus();
        agnus.spr_pt[2] = 0x3000;
        agnus.vpos = 45;
        agnus.spr_vstop[2] = 50;
        agnus.spr_dma_on[2] = true;
        let words = [0x1111u16, 0x2222, 0x3333, 0x4444];
        let mut i = 0;
        let fetched = agnus.service_sprite_dma_cyc(2, false, 4, |_addr| {
            let w = words[i];
            i += 1;
            w
        });
        assert_eq!(fetched, Some((false, 0x1111_2222_3333_4444)));
        assert_eq!(agnus.spr_pt[2], 0x3008, "pointer advances by 4 words");
    }

    #[test]
    fn sprite_comparators_evolve_while_spren_only_gates_bus_requests() {
        let mut agnus = Agnus::new();
        agnus.dmacon = 0x0200; // DMAEN but no SPREN
        agnus.spr_vstart[0] = 40;
        agnus.spr_vstop[0] = 60;
        agnus.hpos = 0x15;
        agnus.vpos = 40;
        agnus.update_sprite_dma(PAL_LINES_PER_FRAME);
        assert!(
            agnus.sprite_dma_on(0),
            "VSTART records latent active state without SPREN"
        );
        assert_eq!(
            agnus.current_slot(),
            SlotOwner::Cpu,
            "SPREN still gates the bus request"
        );

        agnus.dmacon |= 0x0020;
        agnus.vpos = 41;
        assert_eq!(
            agnus.current_slot(),
            SlotOwner::Sprite(0),
            "enabling SPREN between VSTART and VSTOP exposes the data request"
        );

        agnus.dmacon &= !0x0020;
        agnus.vpos = 60;
        agnus.update_sprite_dma(PAL_LINES_PER_FRAME);
        assert!(
            !agnus.sprite_dma_on(0),
            "VSTOP clears latent active state without SPREN"
        );
    }

    #[test]
    fn sprite_active_state_clears_at_field_end_without_spren() {
        for region in [AgnusRegion::Pal, AgnusRegion::Ntsc] {
            let mut agnus = Agnus::new_with_region(region);
            agnus.dmacon = DMACON_DMAEN;
            agnus.spr_dma_on[0] = true;
            agnus.vpos = agnus.lines_per_frame - 2;
            agnus.hpos = agnus.current_line_ccks() - 1;

            agnus.tick_cck();

            assert_eq!(agnus.vpos, agnus.lines_per_frame - 1);
            assert!(!agnus.sprite_dma_on(0));
        }
    }

    #[test]
    fn equal_vstart_vstop_remains_inactive() {
        let mut agnus = sprite_dma_agnus();
        agnus.spr_vstart[0] = 40;
        agnus.spr_vstop[0] = 40;
        agnus.spr_dma_on[0] = true;
        agnus.vpos = 40;

        agnus.update_sprite_dma(PAL_LINES_PER_FRAME);

        assert!(!agnus.sprite_dma_on(0), "VSTOP must take precedence");
    }

    #[test]
    fn direct_sprite_latches_reevaluate_state_outside_but_not_inside_blank() {
        let mut agnus = Agnus::new();
        agnus.spr_vstop[0] = 60;

        agnus.vpos = 10;
        agnus.poke_sprite_pos(0, 10 << 8);
        assert!(
            !agnus.sprite_dma_on(0),
            "a VSTART match inside fixed vertical blank is ignored"
        );

        agnus.vpos = 40;
        agnus.poke_sprite_pos(0, 40 << 8);
        assert!(
            agnus.sprite_dma_on(0),
            "a direct VSTART match outside blank activates latent state"
        );

        agnus.poke_sprite_ctl(0, 40 << 8);
        assert_eq!(agnus.spr_vstart[0], 40);
        assert_eq!(agnus.spr_vstop[0], 40);
        assert!(
            !agnus.sprite_dma_on(0),
            "a matching direct VSTOP takes precedence and deactivates"
        );
    }

    #[test]
    fn fixed_vertical_blank_level_ends_at_the_regional_boundary() {
        for (region, end_line) in [(AgnusRegion::Pal, 25), (AgnusRegion::Ntsc, 20)] {
            let mut agnus = Agnus::new_with_region(region);
            agnus.vpos = end_line - 1;
            assert!(agnus.vertb_level());
            agnus.vpos = end_line;
            assert!(!agnus.vertb_level());
        }
    }
}
