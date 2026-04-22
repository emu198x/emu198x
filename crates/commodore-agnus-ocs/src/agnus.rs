//! Agnus - Beam counter and DMA slot allocation.

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
/// Nominal NTSC short-line length. Per HRM p. 785 the NTSC beam
/// alternates between 227 and 228 CCKs per line; the per-line
/// alternation is not yet modelled (all NTSC lines count 227 in this
/// build). PAL uses a fixed 227 every line.
#[allow(dead_code)]
pub const NTSC_CCKS_PER_LINE_SHORT: u16 = 227;
#[allow(dead_code)]
pub const NTSC_CCKS_PER_LINE_LONG: u16 = 228;
/// Backwards-compatible alias — equal to the short-line length.
#[allow(dead_code)]
pub const NTSC_CCKS_PER_LINE: u16 = NTSC_CCKS_PER_LINE_SHORT;
/// NTSC uses 262 lines per frame (vs PAL's 312).
#[allow(dead_code)]
pub const NTSC_LINES_PER_FRAME: u16 = 262;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Clone)]
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

    /// Agnus revision ID (7-bit value placed in VPOSR bits 14-8).
    /// OCS NTSC (8361/8370) = $00, OCS PAL (8367/8371) = $10,
    /// ECS NTSC (8372A) = $20, ECS PAL (8372A) = $30.
    pub agnus_id: u16,

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
            dsk_pt: 0,
            fmode: 0,
            lof: true,
            lines_per_frame: PAL_LINES_PER_FRAME,
            agnus_id: 0x00,
            vbl_count: 0,
        }
    }

    /// Create a new Agnus with the specified lines-per-frame count.
    #[must_use]
    pub fn new_with_region_lines(lines_per_frame: u16) -> Self {
        let mut agnus = Self::new();
        agnus.lines_per_frame = lines_per_frame;
        agnus
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

    /// Bitplane DMA fetch width: always 1 (16-bit) for OCS/ECS.
    ///
    /// The AGA wrapper crate shadows this to decode FMODE.
    #[must_use]
    pub fn bpl_fetch_width(&self) -> u8 {
        1
    }

    /// Sprite DMA fetch width: always 1 (16-bit) for OCS/ECS.
    ///
    /// The AGA wrapper crate shadows this to decode FMODE bits 3-2.
    #[must_use]
    pub fn spr_fetch_width(&self) -> u8 {
        1
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
        self.blitter_busy = true;
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
        let height = u32::from((self.bltsize >> 6) & 0x03FF);
        let width_words = u32::from(self.bltsize & 0x003F);
        let height = if height == 0 { 1024 } else { height };
        let width_words = if width_words == 0 { 64 } else { width_words };

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
            let height = u32::from((self.bltsize >> 6) & 0x03FF);
            let width_words = u32::from(self.bltsize & 0x003F);
            let height = if height == 0 { 1024 } else { height };
            let width_words = if width_words == 0 { 64 } else { width_words };
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

        let length = u32::from((self.bltsize >> 6) & 0x03FF);
        let length = if length == 0 { 1024 } else { length };
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
        if self.hpos >= PAL_CCKS_PER_LINE {
            self.hpos = 0;
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
        }
    }

    /// Determine who owns the current CCK slot.
    pub fn current_slot(&self) -> SlotOwner {
        match self.hpos {
            // Fixed slots
            0x01..=0x03 | 0x1B => SlotOwner::Refresh,
            0x04..=0x06 => {
                if self.dma_enabled(0x0010) {
                    SlotOwner::Disk
                } else {
                    SlotOwner::Cpu
                }
            }
            0x07 => {
                if self.dma_enabled(0x0001) {
                    SlotOwner::Audio(0)
                } else {
                    SlotOwner::Cpu
                }
            }
            0x08 => {
                if self.dma_enabled(0x0002) {
                    SlotOwner::Audio(1)
                } else {
                    SlotOwner::Cpu
                }
            }
            0x09 => {
                if self.dma_enabled(0x0004) {
                    SlotOwner::Audio(2)
                } else {
                    SlotOwner::Cpu
                }
            }
            0x0A => {
                if self.dma_enabled(0x0008) {
                    SlotOwner::Audio(3)
                } else {
                    SlotOwner::Cpu
                }
            }
            0x0B..=0x1A => {
                if self.dma_enabled(0x0020) {
                    SlotOwner::Sprite(((self.hpos - 0x0B) / 2) as u8)
                } else {
                    SlotOwner::Cpu
                }
            }

            // Variable slots (Bitplane, Copper, CPU)
            0x1C..=0xE2 => {
                // Bitplane DMA: the OCS/ECS fetch sequencer runs in
                // indivisible 8-CCK "fetchunit" blocks (matching WinUAE).
                //
                // WinUAE uses a ddf_stopping state machine (0→1→2):
                //   - DDFSTOP match while fetching → ddf_stopping = 1
                //   - End of current fetchunit block → ddf_stopping = 2
                //   - End of next fetchunit block → DMA stops
                //
                // This means the fetch window always completes the block
                // containing DDFSTOP *plus* one more full block after it.
                // For DDFSTRT=$40, DDFSTOP=$D0 hires: 19 blocks ($40–$D7),
                // fetching 38 BPL1 words (76 bytes).
                let num_bpl = self.num_bitplanes();
                let hires = (self.bplcon0 & 0x8000) != 0;
                let group_len = if hires { 4 } else { 8 };
                let fetchunit: u32 = 8;
                let ddf_span = u32::from(self.ddfstop.saturating_sub(self.ddfstrt));
                let blocks = ddf_span.div_ceil(fetchunit) + 1;
                let fetch_window_end = u32::from(self.ddfstrt) + blocks * fetchunit - 1;
                if self.dma_enabled(0x0100)
                    && num_bpl > 0
                    && self.hpos >= self.ddfstrt
                    && u32::from(self.hpos) <= fetch_window_end
                {
                    let pos_in_group = ((self.hpos - self.ddfstrt) % group_len) as usize;
                    let plane_slot = if hires {
                        HIRES_DDF_TO_PLANE[pos_in_group]
                    } else {
                        LOWRES_DDF_TO_PLANE[pos_in_group]
                    };
                    if let Some(plane) = plane_slot.filter(|&p| p < num_bpl) {
                        return SlotOwner::Bitplane(plane);
                    }
                }

                // Copper
                if self.dma_enabled(0x0080) && self.hpos.is_multiple_of(2) {
                    return SlotOwner::Copper;
                }

                SlotOwner::Cpu
            }

            _ => SlotOwner::Cpu,
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
    fn cck_bus_plan_reports_audio_service_grant() {
        let mut agnus = Agnus::new();
        agnus.hpos = 0x07;
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
        agnus.hpos = 0x00; // free slot outside fixed/variable DMA windows
        agnus.dmacon = DMACON_DMAEN | DMACON_COPEN | DMACON_BPLEN;
        agnus.bplcon0 = 1 << 12;
        agnus.ddfstrt = 0x1C;
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
        agnus.hpos = 0x04;
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
        agnus.hpos = 0x0B; // first sprite DMA slot pair => sprite 0
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
}
