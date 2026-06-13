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
    /// Minimal model: a busy blitter in nasty mode (BLTPRI) takes CPU/free
    /// slots when blitter DMA is enabled. The blitter operation itself is still
    /// executed synchronously elsewhere, so this only models bus arbitration.
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

/// Maps ddfseq position (0-7) within an 8-CCK group to bitplane index.
/// From Minimig Verilog: plane = {~ddfseq[0], ~ddfseq[1], ~ddfseq[2]}.
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
/// - `fetchunit` is the DDF stop-rounding granularity (color clocks):
///   the fetch window always completes the unit containing DDFSTOP
///   plus one more unit.
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
    texture: u16,
    lf: u8,
    sing: bool,
    texture_enabled: bool,
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

    // Blitter Registers
    pub bltcon0: u16,
    pub bltcon1: u16,
    pub bltsize: u16,
    pub bltsizv_ecs: u16,
    pub bltsizh_ecs: u16,
    pub blitter_busy: bool,
    pub blitter_exec_pending: bool,
    /// Running NOR of every D word the current blit has written: stays
    /// `true` while all results are zero, cleared on the first non-zero
    /// D word. Read out as DMACONR BZERO (bit 13). Reset at `start_blit`.
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
    ///   * OCS NTSC 8361 / 8370 = `$0000`
    ///   * OCS PAL  8367 / 8371 = `$1000`
    ///   * ECS NTSC 8375        = `$3000`
    ///   * ECS PAL  8375        = `$2000`
    ///   * AGA Alice NTSC       = `$3300`
    ///   * AGA Alice PAL        = `$2300`
    ///
    /// Each wrapper (`AgnusEcs`, `AgnusAga`) overrides this in its
    /// constructor so the inner OCS struct still serialises cleanly
    /// while the bus-read returns the wrapper's true chip identity.
    pub agnus_id: u16,

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
            bltcon0: 0,
            bltcon1: 0,
            bltsize: 0,
            bltsizv_ecs: 0,
            bltsizh_ecs: 0,
            blitter_busy: false,
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
            bpl1mod: 0,
            bpl2mod: 0,
            spr_pt: [0; 8],
            spr_pt_hi_latch: [0; 8],
            spr_pt_hi_pending: [false; 8],
            spr_vstart: [0; 8],
            spr_vstop: [0; 8],
            spr_dma_on: [false; 8],
            dsk_pt: 0,
            fmode: 0,
            lof: true,
            lines_per_frame: PAL_LINES_PER_FRAME,
            agnus_id: 0x1000,
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

    /// Create a new Agnus configured for the named video region.
    /// PAL is the existing default — every line is 227 CCKs, frame
    /// is 312 lines, `agnus_id = $1000` (8367 PAL OCS Agnus). NTSC
    /// alternates short/long lines (227/228) per HRM p. 785, frame
    /// is 262 lines, `agnus_id = $0000` (8361 NTSC OCS Agnus). The
    /// first NTSC line is short; the alternation is strict (every
    /// other line) until ECS adds the LOLDIS bit on BPLCON3. `agnus_id`
    /// is stored pre-shifted into VPOSR bits 14-8.
    #[must_use]
    pub fn new_with_region(region: AgnusRegion) -> Self {
        let mut agnus = Self::new();
        match region {
            AgnusRegion::Pal => {
                agnus.region = AgnusRegion::Pal;
                agnus.lines_per_frame = PAL_LINES_PER_FRAME;
                agnus.agnus_id = 0x1000;
                agnus.lol = false;
                agnus.lol_toggle = false;
            }
            AgnusRegion::Ntsc => {
                agnus.region = AgnusRegion::Ntsc;
                agnus.lines_per_frame = NTSC_LINES_PER_FRAME;
                agnus.agnus_id = 0x0000;
                agnus.lol = false;
                agnus.lol_toggle = true;
            }
        }
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

    /// `true` when a busy blitter is in nasty mode and may steal CPU/free slots.
    #[must_use]
    pub fn blitter_nasty_active(&self) -> bool {
        const DMACON_BLTEN: u16 = 0x0040;
        const DMACON_BLTPRI: u16 = 0x0400;

        self.blitter_busy && self.dma_enabled(DMACON_BLTEN) && (self.dmacon & DMACON_BLTPRI) != 0
    }

    /// Start a coarse per-CCK blitter completion timer.
    ///
    /// This preserves `blitter_busy` across CCKs so bus arbitration can react
    /// to the blitter before the existing synchronous blit implementation runs.
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
        self.blitter_exec_pending = true;
        self.blitter_dzero = true; // BZERO accumulates from "all zero"
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

    /// Grant a blitter DMA operation, marking the channel as serviced.
    ///
    /// Called by the machine tick loop when a blitter slot is available.
    pub fn grant_blitter_dma_op(&mut self, op: BlitterDmaOp) {
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

    /// Consume one blitter DMA timing op if progress is granted.
    ///
    /// Queries the word state machine for the next requested op and grants it.
    /// The caller is responsible for executing the op against the incremental
    /// runtime and calling `advance_blitter_word()` when the word completes.
    pub fn tick_blitter_scheduler_op(&mut self, progress_this_cck: bool) -> Option<BlitterDmaOp> {
        if !progress_this_cck {
            return None;
        }
        let op = self.next_blitter_dma_request()?;
        self.grant_blitter_dma_op(op);
        Some(op)
    }

    /// Advance the blitter scheduler by one CCK and report completion.
    ///
    /// Full request→grant→advance cycle wrapper around
    /// `tick_blitter_scheduler_op`. Returns `true` when the blit
    /// completes. Useful to tests + any integrator that wants a
    /// one-call progress step rather than driving the op-by-op
    /// protocol manually.
    pub fn tick_blitter_scheduler(&mut self, progress_this_cck: bool) -> bool {
        if self.tick_blitter_scheduler_op(progress_this_cck).is_none() {
            return false;
        }
        // Check if the word is complete after granting an op.
        if self.blitter_word_complete() {
            if self.blitter_ccks_remaining == 0 {
                self.blitter_word_state = None;
                self.blitter_exec_pending = false;
                self.blitter_line_runtime = None;
                self.blitter_area_runtime = None;
                return true;
            }
            self.advance_blitter_word();
        }
        false
    }

    /// Clear the blitter scheduler state after the blit core executes.
    pub fn clear_blitter_scheduler(&mut self) {
        self.blitter_word_state = None;
        self.blitter_exec_pending = false;
        self.blitter_ccks_remaining = 0;
        self.blitter_line_runtime = None;
        self.blitter_area_runtime = None;
    }

    #[must_use]
    pub fn blitter_exec_ready(&self) -> bool {
        self.blitter_busy
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
                    let b_val = if line.texture_enabled {
                        if line.texture & 0x8000 != 0 {
                            0xFFFF
                        } else {
                            0x0000
                        }
                    } else {
                        0xFFFF
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
                    if line.sing {
                        result = (result & pixel_mask) | (c_val & !pixel_mask);
                    }
                    if result != 0 {
                        self.blitter_dzero = false; // BZERO: a non-zero D word
                    }
                    write_word(line.dpt, result);

                    if line.texture_enabled {
                        line.texture = line.texture.rotate_left(1);
                    }

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

                    if line.error >= 0 {
                        if line.major_is_y {
                            step_y(&mut line);
                            step_x(&mut line);
                        } else {
                            step_x(&mut line);
                            step_y(&mut line);
                        }
                        line.error = line.error.wrapping_add(line.error_sub);
                    } else {
                        if line.major_is_y {
                            step_y(&mut line);
                        } else {
                            step_x(&mut line);
                        }
                        line.error = line.error.wrapping_add(line.error_add);
                    }

                    line.have_c_word = false;
                    line.steps_remaining = line.steps_remaining.saturating_sub(1);
                    if line.steps_remaining == 0 {
                        self.blt_apt = line.error as u16 as u32;
                        self.blt_cpt = line.cpt;
                        self.blt_dpt = line.dpt;
                        self.blt_bdat = line.texture;
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

        if area.use_d {
            if result != 0 {
                self.blitter_dzero = false; // BZERO: a non-zero D word
            }
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
        let texture_enabled = (self.bltcon0 & 0x0400) != 0;
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
            lf,
            sing,
            texture_enabled,
            major_is_y,
            x_neg,
            y_neg,
            last_c_word: 0,
            have_c_word: false,
        });
    }

    /// Tick one CCK (8 crystal ticks).
    pub fn tick_cck(&mut self) {
        self.hpos += 1;
        if self.hpos >= self.current_line_ccks() {
            self.hpos = 0;
            // End-of-line: toggle the long-line flipflop on regions
            // that alternate (NTSC default). PAL has lol_toggle=false
            // so the flipflop stays at 0 (every line is 227).
            if self.lol_toggle {
                self.lol = !self.lol;
            }
            self.vpos += 1;
            // Interlace: long frame has one extra line (313 PAL, 263 NTSC).
            let interlace = (self.bplcon0 & 0x0004) != 0;
            let frame_lines = if interlace && self.lof {
                self.lines_per_frame + 1
            } else {
                self.lines_per_frame
            };
            if self.vpos >= frame_lines {
                self.vpos = 0;
                self.vbl_count += 1;
                if interlace {
                    self.lof = !self.lof;
                }
            }
            // New display line: run the per-line sprite-DMA update
            // (VSTART activation, VSTOP deactivation, top-of-frame
            // control-refetch priming). gap #162.
            self.update_sprite_dma();
        }
    }

    /// Per-line sprite-DMA update — run once as the beam enters each new
    /// display line. Activates a sprite when the beam reaches its VSTART
    /// and deactivates at VSTOP; on the vertical-blank end line it forces
    /// every sprite's VSTOP to that line so the next slots refetch the
    /// control words, re-priming VSTART/VSTOP from the (copper-reloaded)
    /// pointers for the new frame. Mirrors vAmiga `updateSpriteDMA`
    /// (Agnus.cpp:658-680).
    fn update_sprite_dma(&mut self) {
        if !self.dma_enabled(0x0020) {
            return;
        }
        let v = self.vpos;
        if v == VBL_END_LINE {
            for s in 0..8 {
                self.spr_vstop[s] = VBL_END_LINE;
            }
            return;
        }
        if v + 1 >= self.lines_per_frame {
            for s in 0..8 {
                self.spr_dma_on[s] = false;
            }
            return;
        }
        if v < VBL_END_LINE {
            return;
        }
        for s in 0..8 {
            if v == self.spr_vstart[s] {
                self.spr_dma_on[s] = true;
            }
            if v == self.spr_vstop[s] {
                self.spr_dma_on[s] = false;
            }
        }
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
        mut read: impl FnMut(u32) -> u16,
    ) -> Option<(bool, u64)> {
        if channel >= 8 {
            return None;
        }
        // Sprite DMA is suppressed during vertical blank; fetching starts
        // at the reset line (the first control fetch of the frame), so a
        // sprite whose VSTOP still sits at its power-on 0 does not fetch
        // spuriously on line 0. Mirrors vAmiga's `!inVBlankArea` gate.
        if self.vpos < VBL_END_LINE {
            return None;
        }
        if self.vpos == self.spr_vstop[channel] {
            // Control fetch (SPRxPOS / SPRxCTL): always a single word —
            // FMODE widens the data fetch, not the control words.
            self.spr_dma_on[channel] = false;
            let word = read(self.spr_pt[channel]);
            self.spr_pt[channel] = self.spr_pt[channel].wrapping_add(2);
            if second_word {
                self.latch_sprite_ctl(channel, word);
            } else {
                self.latch_sprite_pos(channel, word);
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
        if channel < 8 {
            self.latch_sprite_pos(channel, val);
        }
    }

    /// Apply a direct (CPU/copper) write to `SPRxCTL` — update the
    /// VSTART[8]/VSTOP comparators. See [`Self::poke_sprite_pos`].
    /// Mirrors vAmiga `Agnus::setSPRxCTL` (AgnusRegs.cpp:501).
    pub fn poke_sprite_ctl(&mut self, channel: usize, val: u16) {
        if channel < 8 {
            self.latch_sprite_ctl(channel, val);
        }
    }

    /// Latch VSTART low 8 bits from a fetched SPRxPOS word (bits 15-8).
    fn latch_sprite_pos(&mut self, channel: usize, pos: u16) {
        self.spr_vstart[channel] = (self.spr_vstart[channel] & 0x0100) | (pos >> 8);
    }

    /// Latch VSTOP (CTL bits 15-8) plus VSTART[8] (CTL bit 2) and
    /// VSTOP[8] (CTL bit 1) from a fetched SPRxCTL word. OCS is 9-bit;
    /// the ECS VSTART[9]/VSTOP[9] bits (CTL 6/5) are not modelled here.
    fn latch_sprite_ctl(&mut self, channel: usize, ctl: u16) {
        self.spr_vstart[channel] = (self.spr_vstart[channel] & 0x00FF) | ((ctl & 0x0004) << 6);
        self.spr_vstop[channel] = (ctl >> 8) | ((ctl & 0x0002) << 7);
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

    /// Determine who owns the current CCK slot.
    pub fn current_slot(&self) -> SlotOwner {
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
        if let Some(plane) = self.bitplane_slot_at() {
            return SlotOwner::Bitplane(plane);
        }
        // Sprites 0–7 at 0x15..0x33 (odd cells), SPREN.
        if self.dma_enabled(0x0020) && (0x15..=0x33).contains(&hpos) && !hpos.is_multiple_of(2) {
            return SlotOwner::Sprite(((hpos - 0x15) / 4) as u8);
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
        // DDFSTRT/DDFSTOP align to the fetch boundary: the hardware
        // ignores the low DDF bits. OCS masks $FC (4-CCK lores
        // granularity); ECS/AGA mask $FE (2-CCK, the finer step SHRES
        // needs). agnus_id >= $2000 selects ECS and later. #30 Phase 4.
        let ddf_mask: u16 = if self.agnus_id >= 0x2000 {
            0x00FE
        } else {
            0x00FC
        };
        let ddfstrt = self.ddfstrt & ddf_mask;
        let ddfstop = self.ddfstop & ddf_mask;
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
                let fetchunit: u32 = 8;
                let ddf_span = u32::from(ddfstop.saturating_sub(ddfstrt));
                let blocks = ddf_span.div_ceil(fetchunit) + 1;
                let fetch_window_end = u32::from(ddfstrt) + blocks * fetchunit - 1;
                if u32::from(self.hpos) <= fetch_window_end {
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
                    None
                }
            } else {
                // AGA wide fetch (FMODE > 0). For every width > 1,
                // fetchunit == fetchstart, so each fetchunit block holds
                // exactly one plane access per active plane, each access
                // transferring `fetch_width` words. The DDFSTOP-rounding
                // "complete the block plus one more" rule (div_ceil + 1)
                // is the same as the 16-bit path.
                let (fetchunit, fetchstart) = fetch_cadence(fetch_width, hires, shres);
                let ddf_span = u32::from(ddfstop.saturating_sub(ddfstrt));
                let blocks = ddf_span.div_ceil(fetchunit) + 1;
                let fetch_window_end = u32::from(ddfstrt) + blocks * fetchunit - 1;
                if u32::from(self.hpos) <= fetch_window_end {
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
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    /// Compute the machine-facing Agnus bus-arbitration plan for this CCK.
    pub fn cck_bus_plan(&self) -> CckBusPlan {
        let slot_owner = self.current_slot();
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
        if val & bits::DMACON_SETCLR != 0 {
            self.dmacon |= val & bits::DMACON_MASK;
        } else {
            self.dmacon &= !(val & bits::DMACON_MASK);
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
    }
    pub fn write_diwstop(&mut self, val: u16) {
        self.diwstop = val;
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

    /// Drive a blit to completion synchronously. Used by the simple
    /// "run the blit on BLTSIZE write" integration model — later work
    /// (task #147, true per-slot pacing) replaces this with
    /// incremental tick-driven progress.
    ///
    /// Takes a single bus trait implementation — matches on op type
    /// so only one direction of the bus is borrowed at a time.
    pub fn run_blit_to_completion(&mut self, bus: &mut dyn BlitterBus) {
        let mut guard = 0u32;
        while let Some(op) = self.next_blitter_dma_request() {
            self.grant_blitter_dma_op(op);
            let done = match op {
                BlitterDmaOp::WriteD => self.execute_incremental_blitter_op(
                    op,
                    |_| 0,
                    |addr, val| bus.write_word(addr, val),
                ),
                BlitterDmaOp::Internal => self.execute_incremental_blitter_op(op, |_| 0, |_, _| {}),
                _ => self.execute_incremental_blitter_op(op, |addr| bus.read_word(addr), |_, _| {}),
            };
            if self.blitter_word_complete() && !done {
                self.advance_blitter_word();
            }
            guard += 1;
            if guard > 1_000_000 {
                break;
            }
        }
        self.blitter_busy = false;
    }

    /// Service exactly one granted blitter DMA op against the chip bus —
    /// the per-CCK counterpart to [`Agnus::run_blit_to_completion`]. The
    /// machine calls this once on each CCK the bus plan grants the
    /// blitter (`CckBusPlan::blitter_dma_progress_granted`), so a blit
    /// consumes real chip cycles and contends for the bus instead of
    /// finishing instantly on the BLTSIZE write.
    ///
    /// Returns `true` on the CCK that drains the last op — the caller
    /// raises INT_BLIT. The body mirrors `run_blit_to_completion`'s loop
    /// exactly; the only difference is one op per call instead of a
    /// drain-to-completion loop. An equivalence test
    /// (`incremental_drain_matches_synchronous_blit`) pins that the two
    /// paths produce byte-identical chip-RAM output.
    pub fn tick_blitter_dma(&mut self, bus: &mut dyn BlitterBus) -> bool {
        let Some(op) = self.next_blitter_dma_request() else {
            // Caller only ticks us while busy; no pending op means the
            // blit is already drained — report completion once.
            self.blitter_busy = false;
            return true;
        };
        self.grant_blitter_dma_op(op);
        let done = match op {
            BlitterDmaOp::WriteD => self.execute_incremental_blitter_op(
                op,
                |_| 0,
                |addr, val| bus.write_word(addr, val),
            ),
            BlitterDmaOp::Internal => self.execute_incremental_blitter_op(op, |_| 0, |_, _| {}),
            _ => self.execute_incremental_blitter_op(op, |addr| bus.read_word(addr), |_, _| {}),
        };
        if self.blitter_word_complete() && !done {
            self.advance_blitter_word();
        }
        if self.next_blitter_dma_request().is_none() {
            self.blitter_busy = false;
            return true;
        }
        false
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
    /// plus live blitter status. BBUSY (bit 14) is asserted while a
    /// blit is in flight so a `WaitBlit` poll (read DMACONR until BBUSY
    /// clears) is honoured now that blits take real chip cycles rather
    /// than completing instantly on the BLTSIZE write (#31/#32). BZERO
    /// (bit 13) reports whether the (last) blit produced an all-zero D
    /// result — the signal collision/comparison blits read after
    /// WaitBlit.
    #[must_use]
    pub fn dmaconr(&self) -> u16 {
        let mut v = self.dmacon & bits::DMACON_MASK;
        if self.blitter_busy {
            v |= 0x4000; // BBUSY
        }
        if self.blitter_dzero {
            v |= 0x2000; // BZERO
        }
        v
    }

    /// Level-sensitive /VERTB signal — high while the beam sits inside
    /// the vertical blanking interval. Paula edge-latches this to set
    /// INTREQ.VERTB.
    #[must_use]
    pub fn vertb_level(&self) -> bool {
        self.vpos < VBL_END_LINE
    }
}

/// Last line of the vertical blanking interval (inclusive of lines
/// 0..24 — HRM standard PAL). Exposed so machine callers don't
/// hard-code the boundary.
pub const VBL_END_LINE: u16 = 25;

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

    /// Count bitplane DMA grants for one plane across a whole scan line.
    fn bitplane_grants(agnus: &mut Agnus, plane: u8) -> usize {
        let mut count = 0;
        for h in 0u16..=0xE2 {
            agnus.hpos = h;
            if agnus.current_slot() == SlotOwner::Bitplane(plane) {
                count += 1;
            }
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

    /// The whole-line bitplane plane map (which plane, if any, each
    /// hpos fetches) — the fetch grid `bitplane_slot_at` produces.
    fn plane_map(agnus: &mut Agnus) -> Vec<Option<u8>> {
        (0u16..=0xE2)
            .map(|h| {
                agnus.hpos = h;
                match agnus.current_slot() {
                    SlotOwner::Bitplane(p) => Some(p),
                    _ => None,
                }
            })
            .collect()
    }

    #[test]
    fn ddf_strt_aligns_to_fetch_boundary_per_variant() {
        // OCS ignores DDFSTRT's low 2 bits ($FC, 4-CCK lores boundary):
        // an unaligned $3A produces exactly the same fetch grid as $38.
        let mut aligned = Agnus::new(); // OCS, agnus_id = $1000
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
        agnus.hpos = 0x23; // ddfstrt + 7 => BPL1 slot in lowres fetch group
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN | DMACON_COPEN;
        agnus.bplcon0 = 1 << 12; // 1 bitplane enabled
        agnus.ddfstrt = 0x1C;
        agnus.ddfstop = 0x1C;

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

    #[test]
    fn cck_bus_plan_reports_hires_bitplane_grant_at_group_end() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x43; // ddfstrt + 3 => BPL1 slot in hires fetch group
        agnus.dmacon = DMACON_DMAEN | DMACON_BPLEN | DMACON_COPEN;
        agnus.bplcon0 = 0x8000 | (1 << 12); // HIRES + 1 bitplane
        agnus.ddfstrt = 0x40;
        agnus.ddfstop = 0x40;

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
    fn blitter_scheduler_counts_down_and_requires_progress() {
        let mut agnus = Agnus::new();
        agnus.bltcon0 = 0x0100; // D write only => 1 DMA op/word
        agnus.bltsize = (1 << 6) | 2; // height=1, width=2 => budget=2
        agnus.start_blit();

        assert!(agnus.blitter_busy);
        assert!(agnus.blitter_exec_pending);
        assert_eq!(agnus.blitter_ccks_remaining, 2);

        assert!(
            !agnus.tick_blitter_scheduler(false),
            "no progress when bus grant is withheld"
        );
        assert_eq!(agnus.blitter_ccks_remaining, 2);

        assert!(!agnus.tick_blitter_scheduler(true));
        assert_eq!(agnus.blitter_ccks_remaining, 1);

        assert!(agnus.tick_blitter_scheduler(true));
        assert!(!agnus.blitter_exec_pending);
        assert_eq!(agnus.blitter_ccks_remaining, 0);
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
        agnus.grant_blitter_dma_op(BlitterDmaOp::ReadA);
        assert_eq!(agnus.next_blitter_dma_request(), Some(BlitterDmaOp::ReadC));
        agnus.grant_blitter_dma_op(BlitterDmaOp::ReadC);
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
        agnus.grant_blitter_dma_op(BlitterDmaOp::ReadC);
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
        agnus.dmacon = DMACON_DMAEN | 0x0020; // SPREN

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

    // ---------- region + line-length alternation ----------

    /// VPOSR upper byte is what Kickstart reads to identify the
    /// Agnus revision. Locks the bit-positions so future storage
    /// refactors can't silently regress to the pre-Stage AE-j state
    /// where every chipset reported `$0000` in the ID bits.
    #[test]
    fn vposr_reports_agnus_id_in_upper_byte() {
        let pal = Agnus::new_with_region(AgnusRegion::Pal);
        // PAL 8367: bits 14-8 = `0010000` → upper byte $10 → u16 $1000.
        // LOF starts set + vpos bit 8 zero at reset → bit 15 = 1, bit 0 = 0.
        assert_eq!(pal.vposr() & 0x7F00, 0x1000);

        let ntsc = Agnus::new_with_region(AgnusRegion::Ntsc);
        // NTSC 8361: upper byte $00 → u16 $0000.
        assert_eq!(ntsc.vposr() & 0x7F00, 0x0000);
    }

    #[test]
    fn pal_default_keeps_lol_zero_and_lines_at_227() {
        let agnus = Agnus::new_with_region(AgnusRegion::Pal);
        assert_eq!(agnus.region, AgnusRegion::Pal);
        assert_eq!(agnus.lines_per_frame, PAL_LINES_PER_FRAME);
        // 8367 PAL OCS Agnus stores its VPOSR ID pre-shifted into
        // bits 14-8 — see the `agnus_id` field doc.
        assert_eq!(agnus.agnus_id, 0x1000);
        assert!(!agnus.lol);
        assert!(!agnus.lol_toggle);
        assert_eq!(agnus.current_line_ccks(), PAL_CCKS_PER_LINE);
    }

    #[test]
    fn ntsc_starts_short_and_alternates_per_line() {
        let mut agnus = Agnus::new_with_region(AgnusRegion::Ntsc);
        assert_eq!(agnus.region, AgnusRegion::Ntsc);
        assert_eq!(agnus.lines_per_frame, NTSC_LINES_PER_FRAME);
        // 8361 NTSC OCS Agnus reports $0000 in VPOSR bits 14-8.
        assert_eq!(agnus.agnus_id, 0x0000);
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
        agnus.update_sprite_dma();
        assert!(!agnus.sprite_dma_on(0));
        agnus.vpos = 40;
        agnus.update_sprite_dma();
        assert!(agnus.sprite_dma_on(0), "activates at VSTART");
        agnus.vpos = 50;
        agnus.update_sprite_dma();
        assert!(agnus.sprite_dma_on(0), "stays on between");
        agnus.vpos = 60;
        agnus.update_sprite_dma();
        assert!(!agnus.sprite_dma_on(0), "deactivates at VSTOP");
    }

    #[test]
    fn update_sprite_dma_reset_line_forces_control_refetch() {
        let mut agnus = sprite_dma_agnus();
        for s in 0..8 {
            agnus.spr_vstop[s] = 999;
        }
        agnus.vpos = VBL_END_LINE;
        agnus.update_sprite_dma();
        for s in 0..8 {
            assert_eq!(
                agnus.sprite_vstop(s),
                VBL_END_LINE,
                "reset line forces a VSTOP refetch"
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
    fn sprite_update_is_noop_without_spren() {
        let mut agnus = Agnus::new();
        agnus.dmacon = 0x0200; // DMAEN but no SPREN
        agnus.spr_vstart[0] = 40;
        agnus.vpos = 40;
        agnus.update_sprite_dma();
        assert!(!agnus.sprite_dma_on(0), "no sprite DMA without SPREN");
    }
}
