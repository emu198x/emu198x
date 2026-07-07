//! MOS 6569 PAL / 6567 NTSC VIC-II video chip.
//!
//! The VIC-II is the C64's video chip. It drives the dot clock, owns
//! video memory reads, renders text / bitmap / sprites to an ARGB
//! framebuffer, steals CPU cycles during bad lines and sprite DMA,
//! and generates raster / collision / light-pen interrupts.
//!
//! Each [`Vic::tick`] advances one `phi2` cycle and renders 8 pixels.
//! This first fresh-workspace port keeps the archived crate's proven
//! raster, badline, sprite-BA, IRQ, and display-mode behaviour.

#![allow(clippy::cast_possible_truncation)]

pub mod oracle;
pub mod palette;
// Draw-stage sprite sequencer — first increment of the sprite-sequencer port,
// landed isolated and unit-tested before being wired into the renderer. Its API
// is exercised only by its own tests until the wiring increment, so the
// not-yet-used warning is expected and gated here deliberately.
#[allow(dead_code)]
mod sprite_sequencer;
// Sprite fetch chain (MC/MCBASE/exp-flop + crunch) — the addressing/height half
// of the real sprite hardware, landed isolated + unit-tested (S4a) before being
// wired to feed the sequencer (S4b). Exercised only by its own tests until then.
#[allow(dead_code)]
mod sprite_fetch_chain;

use sprite_fetch_chain::SpriteFetchChain;
use sprite_sequencer::{SpritePixel, SpriteSequencer};

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use palette::PALETTE;

/// Frame routing version. Bumped when the rendering path through this
/// chip (pixel pipeline, badline DMA accounting, sprite mux, display
/// mode dispatch, border timing, palette mapping) changes in a way
/// that invalidates previously-captured frame hashes in the C64
/// catalogue. The catalogue manifest carries the version each hash was
/// captured against; a mismatch fails loud with a re-capture
/// instruction.
///
/// **Version 1** (2026-05-20): per-phi2 8-pixel renderer with `tick`
/// advancing one cycle; standard/MCM/ECM text + MCM/hi-res bitmap
/// modes; sprite DMA pre-allocated per active sprite; PAL 312-line
/// frame, NTSC 263-line; palette from `palette::PALETTE`. The
/// pre-Seam-1 VIC-II described in
/// `knowledge/decisions/c64-architecture-review.md`.
///
/// **Version 2** (2026-07-01): sprite rendering switched from the geometry
/// `overlay_sprites` mux to the draw-stage shift-register **sprite sequencer**
/// (VICE `draw_sprites` two-stage model — MC/MCBASE/exp-flop fetch chain +
/// per-pixel DMA halt). Sprite pixels shift by sub-pixel timing and the
/// fetch/crunch/fetch-bug edge cases now match VICE, so any catalogue frame
/// carrying sprites re-hashes. See the sprite-sequencer increment in
/// `docs/plans/2026-06-30-c64-vic-ii-vc-vcbase-rc-rewrite.md`.
pub const FRAME_ROUTING_VERSION: u32 = 2;

const PAL_FIRST_VISIBLE_LINE: u16 = 0;
const PAL_LAST_VISIBLE_LINE: u16 = 312;
// Like PAL, NTSC renders the full frame (every raster line) and leaves cropping
// to the consumer. Calibration against VICE's 6567R8 reference confirmed our
// NTSC content is horizontally pixel-aligned (crop dx=16, same as PAL) and
// matches ~99% on the overlapping rows; VICE's own 247-line visible window
// wraps the frame boundary, which a single first..last range can't express, so
// we render everything rather than pre-crop to an arbitrary sub-window.
const NTSC_FIRST_VISIBLE_LINE: u16 = 0;
const NTSC_LAST_VISIBLE_LINE: u16 = 263;
const FIRST_VISIBLE_CYCLE: u8 = 10;
const LAST_VISIBLE_CYCLE: u8 = 62;
const VISIBLE_CYCLES: u8 = LAST_VISIBLE_CYCLE - FIRST_VISIBLE_CYCLE;
pub const FB_WIDTH: u32 = VISIBLE_CYCLES as u32 * 8;
pub const FB_HEIGHT: u32 = (PAL_LAST_VISIBLE_LINE - PAL_FIRST_VISIBLE_LINE) as u32;
const DISPLAY_START_LINE: u16 = 0x30;
const DISPLAY_END_LINE: u16 = 0xF8;
const DISPLAY_START_CYCLE: u8 = 16;
const DISPLAY_END_CYCLE: u8 = 56;
const SPRITE_X_TO_FB: i16 = 24;

/// Narrow VIC-visible memory bus.
pub trait VicMemory {
    /// Read a byte from VIC-visible memory using the full 16-bit VIC address.
    fn read_vram(&self, addr: u16) -> u8;

    /// Read a colour RAM nibble at the given 0-1023 offset.
    fn read_colour(&self, offset: u16) -> u8;
}

/// VIC-II model variant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VicModel {
    /// PAL 6569: 312 lines, 63 cycles per line.
    #[default]
    Pal6569,
    /// NTSC 6567R8: 263 lines, 65 cycles per line.
    Ntsc6567,
    /// NTSC 6567R56A (early NTSC): 262 lines, 64 cycles per line.
    Ntsc6567R56A,
}

impl VicModel {
    /// Total raster lines per frame.
    #[must_use]
    pub const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Pal6569 => 312,
            Self::Ntsc6567 => 263,
            Self::Ntsc6567R56A => 262,
        }
    }

    /// `phi2` cycles per raster line.
    #[must_use]
    pub const fn cycles_per_line(self) -> u8 {
        match self {
            Self::Pal6569 => 63,
            Self::Ntsc6567 => 65,
            Self::Ntsc6567R56A => 64,
        }
    }

    /// The model's sprite-region cycle schedule. The c-access/g-access region
    /// and its counter events (UpdateVc cyc 14, UpdateMcBase 16, UpdateRc 58)
    /// are identical across models; only the sprite fetch/DMA region differs,
    /// because the NTSC variants' extra cycles are inserted there.
    const fn sprite_timing(self) -> SpriteTiming {
        match self {
            Self::Pal6569 => SPRITE_TIMING_PAL,
            Self::Ntsc6567 => SPRITE_TIMING_NTSC,
            Self::Ntsc6567R56A => SPRITE_TIMING_NTSC_R56A,
        }
    }
}

/// Model-specific sprite cycle schedule, in engine 0-based `raster_cycle`
/// values (VICE's 1-based cycle N maps to engine N, with the line's last cycle
/// relabelled 0). Sourced from VICE `vicii-chip-model.c` `cycle_tab_pal` /
/// `cycle_tab_ntsc`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct SpriteTiming {
    /// Per sprite 0..7: the p-access cycle (pointer + data byte 0) and whether
    /// it fetches for the next raster line (sprites whose p-access sits in the
    /// previous line's tail). The s-access (data bytes 1-2) is the next cycle;
    /// the BA lead-in is the three cycles before the p-access.
    paccess: [(u8, bool); 8],
    /// The two cycles the sprite-DMA check fires (VICE `ChkSprDma`). The first
    /// is where the BA-path `evaluate_sprite_dma` runs.
    chk_dma: [u8; 2],
    /// Y-expansion flip-flop toggle cycle (VICE `ChkSprExp`).
    chk_exp: u8,
    /// Sprite display-bit latch cycle — `MC = MCBASE` (VICE `ChkSprDisp`).
    chk_disp: u8,
}

/// PAL 6569: 63-cycle line. Sprites 0-2 p-access in the previous line's tail
/// (58/60/62), sprites 3-7 on the current line (1/3/5/7/9).
const SPRITE_TIMING_PAL: SpriteTiming = SpriteTiming {
    paccess: [
        (58, true),
        (60, true),
        (62, true),
        (1, false),
        (3, false),
        (5, false),
        (7, false),
        (9, false),
    ],
    chk_dma: [55, 56],
    chk_exp: 56,
    chk_disp: 58,
};

/// NTSC 6567R8: 65-cycle line. The extra two cycles push the sprite region
/// later — sprites 0-3 p-access in the previous line's tail (59/61/63/0-wrap),
/// sprites 4-7 on the current line (2/4/6/8); DMA/display checks shift by one.
const SPRITE_TIMING_NTSC: SpriteTiming = SpriteTiming {
    paccess: [
        (59, true),
        (61, true),
        (63, true),
        (0, true),
        (2, false),
        (4, false),
        (6, false),
        (8, false),
    ],
    chk_dma: [56, 57],
    chk_exp: 56,
    chk_disp: 59,
};

/// NTSC 6567R56A (early NTSC): 64-cycle line. Sits between PAL and R8 — sprites
/// 3-7 p-access on the current line as PAL (1/3/5/7/9), but sprites 0-2 shift
/// one cycle later than PAL to 59/61/63, and the DMA check is at 56/57 (like
/// R8) while the display latch stays at 58 (like PAL). Sourced from VICE
/// `cycle_tab_ntsc_old`.
const SPRITE_TIMING_NTSC_R56A: SpriteTiming = SpriteTiming {
    paccess: [
        (59, true),
        (61, true),
        (63, true),
        (1, false),
        (3, false),
        (5, false),
        (7, false),
        (9, false),
    ],
    chk_dma: [56, 57],
    chk_exp: 56,
    chk_disp: 58,
};

struct CellPixels {
    colour: [u32; 8],
    fg_mask: u8,
}

impl CellPixels {
    fn solid(c: u32) -> Self {
        Self {
            colour: [c; 8],
            fg_mask: 0,
        }
    }
}

/// VIC-II chip state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vic {
    /// IRQ output pin.
    pub irq: bool,
    /// BA pin, represented as `true` when BA is asserted low.
    ///
    /// BA is the VIC-II's request signal to the CPU: "I need the bus
    /// in a few cycles." On the C64, BA is wired to the 6510's RDY
    /// pin, which stalls *reads* but lets *writes* complete. BA goes
    /// low 3 phi2 cycles before the VIC-II actually needs the bus
    /// (cycle 12 of a badline; the badline DMA itself starts at
    /// cycle 15). That 3-cycle warm-up is the canonical NMOS-6510
    /// pattern.
    ///
    /// See [`Vic::cpu_stalled`] for the AEC-equivalent signal that
    /// fires only when the VIC-II is actually on the bus.
    pub ba_low: bool,
    /// AEC-equivalent: `true` when the CPU is actually off the bus
    /// because the VIC-II has taken it (badline DMA cycles 15-54,
    /// or sprite DMA cycles 58/59 for sprite 0 and shifted onwards
    /// for sprites 1-7). Differs from [`Vic::ba_low`] by the 3-cycle
    /// BA-warm-up window where the CPU can still complete writes.
    ///
    /// The machine layer currently drives `cpu.rdy` off `ba_low`
    /// only (NMOS-correct), and this field is informational for
    /// diagnostic tests and future fidelity work (e.g. modelling
    /// CPU writes that race against AEC drop). The asymmetry
    /// between the two fields is asserted in unit tests; see
    /// `c64-architecture-review.md` Seam 1.
    pub cpu_stalled: bool,

    #[serde(with = "BigArray")]
    regs: [u8; 0x40],
    raster_line: u16,
    raster_cycle: u8,
    raster_compare: u16,
    irq_status: u8,
    irq_enable: u8,
    is_badline: bool,
    den_latch: bool,
    frame_complete: bool,
    framebuffer: Vec<u32>,
    #[serde(with = "BigArray")]
    screen_row: [u8; 40],
    #[serde(with = "BigArray")]
    colour_row: [u8; 40],
    vic_bank: u8,
    /// Sprite DMA-active flags (the BA/CPU-stall path), set at cycle 55 by
    /// `evaluate_sprite_dma`. Independent of the draw-stage sequencer.
    sprite_dma_active: [bool; 8],
    /// Draw-stage sprite sequencer (VICE shift-register pixel pipeline). It is
    /// the sole sprite render path; transient render state rebuilt each line,
    /// so it skips serialisation.
    #[serde(skip)]
    sprite_sequencer: SpriteSequencer,
    /// Sprite fetch chain (MC/MCBASE/exp-flop + crunch) and its MC-addressed
    /// data, feeding the sequencer continuously (VICE model): the chain sets
    /// display bits at cyc 58 (→ `set_pending`) and loads data at the s-access
    /// (→ `load_data`). Used only on the sequencer path. Transient render state.
    #[serde(skip)]
    chain: SpriteFetchChain,
    #[serde(skip)]
    chain_data: [[u8; 3]; 8],
    #[serde(skip)]
    chain_fetch_base: [u16; 8],
    /// This cycle's 8 raw sprite pixels (pre-foreground-priority), produced by
    /// the per-cycle draw pass and composited by `render_pixels`.
    #[serde(skip)]
    sprite_cycle_px: [Option<SpritePixel>; 8],
    /// This cycle's 8 sprite coverage masks (which sprites have a pixel), for
    /// collision detection in the composite.
    #[serde(skip)]
    sprite_cycle_cov: [u8; 8],
    /// This cycle's graphics foreground mask (8 bits), latched by
    /// `render_pixels` for the collision pass. Zero off the display window, so
    /// sprite-background collisions only register over real foreground while
    /// sprite-sprite collisions still register everywhere.
    #[serde(skip)]
    gfx_fg_mask: u8,
    sprite_sprite_collision: u8,
    sprite_bg_collision: u8,
    sprite_sprite_irq_latched: bool,
    sprite_bg_irq_latched: bool,
    xscroll_carry_pixels: [u32; 8],
    xscroll_carry_fg: u8,
    xscroll_latch: u8,
    lines_per_frame: u16,
    cycles_per_line: u8,
    /// Model-specific sprite-region cycle schedule (PAL 6569 vs NTSC 6567).
    timing: SpriteTiming,
    first_visible_line: u16,
    last_visible_line: u16,
    lp_triggered: bool,
    last_bus_data: u8,
    /// Vertical border flip-flop. Set at the last raster line of the
    /// display window, cleared at the first (DEN=1 required). Gates
    /// whether the main FF can be cleared — when vert FF is set, the
    /// main FF stays set, producing solid border for the whole line.
    border_vert_ff: bool,
    /// Main (horizontal) border flip-flop. Set at right edge of display,
    /// cleared at left edge (only if vert FF is clear). When set, the
    /// current cycle paints border colour.
    border_main_ff: bool,

    // --- Video-counter chain (VC/VCBASE/RC/VMLI) ---
    //
    // Shadow of the real VIC-II addressing chain, Increment 2 of the
    // VC/VCBASE/RC rewrite (see
    // `docs/plans/2026-06-30-c64-vic-ii-vc-vcbase-rc-rewrite.md`). Advanced
    // per the canonical rules (ported from VICE `vicii-cycle.c:202-563` +
    // `vicii-fetch.c:234-269`) but NOT yet driving fetches — the engine still
    // addresses memory geometrically. These run in parallel so the rewrite's
    // Increment 3 can swap the fetch addressing over once the counters are
    // proven against the geometry path.
    /// Video counter (10-bit) — the live video-matrix offset.
    vc: u16,
    /// Video counter base (10-bit) — latched from VC at each row end (RC==7).
    vcbase: u16,
    /// Row counter (3-bit) — the character sub-row (0-7) being displayed.
    rc: u8,
    /// Video matrix line index (0-39) — index into the matrix line buffer.
    vmli: u8,
    /// Idle state — gates the g-access VC/VMLI advance. Cleared by a badline,
    /// set when RC passes 7. Starts idle (top border before the first row).
    idle_state: bool,
}

impl Vic {
    /// Construct a VIC-II for the given hardware model.
    #[must_use]
    pub fn new(model: VicModel) -> Self {
        let (first_vis, last_vis) = match model {
            VicModel::Pal6569 => (PAL_FIRST_VISIBLE_LINE, PAL_LAST_VISIBLE_LINE),
            // Both NTSC variants share the visible-line window; R56A has one
            // fewer total line but the same displayed region.
            VicModel::Ntsc6567 | VicModel::Ntsc6567R56A => {
                (NTSC_FIRST_VISIBLE_LINE, NTSC_LAST_VISIBLE_LINE)
            }
        };
        let visible_lines = u32::from(last_vis - first_vis);
        let fb_size = FB_WIDTH as usize * visible_lines as usize;

        Self {
            irq: false,
            ba_low: false,
            cpu_stalled: false,
            regs: [0; 0x40],
            raster_line: 0,
            raster_cycle: 0,
            raster_compare: 0,
            irq_status: 0,
            irq_enable: 0,
            is_badline: false,
            den_latch: false,
            frame_complete: false,
            framebuffer: vec![0xFF00_0000; fb_size],
            screen_row: [0; 40],
            colour_row: [0; 40],
            vic_bank: 0,
            sprite_dma_active: [false; 8],
            sprite_sequencer: SpriteSequencer::new(),
            chain: SpriteFetchChain::new(),
            chain_data: [[0; 3]; 8],
            chain_fetch_base: [0; 8],
            sprite_cycle_px: [None; 8],
            sprite_cycle_cov: [0; 8],
            gfx_fg_mask: 0,
            sprite_sprite_collision: 0,
            sprite_bg_collision: 0,
            sprite_sprite_irq_latched: false,
            sprite_bg_irq_latched: false,
            xscroll_carry_pixels: [0; 8],
            xscroll_carry_fg: 0,
            xscroll_latch: 0,
            lines_per_frame: model.lines_per_frame(),
            cycles_per_line: model.cycles_per_line(),
            timing: model.sprite_timing(),
            first_visible_line: first_vis,
            last_visible_line: last_vis,
            lp_triggered: false,
            last_bus_data: 0,
            border_vert_ff: true,
            border_main_ff: true,
            vc: 0,
            vcbase: 0,
            rc: 0,
            vmli: 0,
            idle_state: true,
        }
    }

    /// Tick the VIC-II for one `phi2` cycle.
    pub fn tick(&mut self, memory: &dyn VicMemory) -> bool {
        if self.raster_cycle == self.timing.chk_dma[0] {
            self.evaluate_sprite_dma();
        }

        // Advance the MC/MCBASE/exp-flop chain + its MC-addressed fetch (chain
        // stage), then run the draw stage for this cycle's 8 pixels. The draw
        // runs every cycle (not just visible) so the shift register + DMA-halt
        // housekeeping stay correct through the border.
        self.advance_sprite_chain(memory);
        self.run_sprite_draw_cycle();

        self.update_border_flip_flops();
        self.render_pixels(memory);
        self.accumulate_sprite_collisions();
        self.check_badline();
        self.advance_video_counters();

        let badline_stall = self.is_badline && (15..=54).contains(&self.raster_cycle);
        let sprite_stall = self.is_sprite_dma_stealing();
        self.cpu_stalled = badline_stall || sprite_stall;
        self.ba_low = self.compute_ba_low();

        // Stream the video-matrix c-access: one read per Phi2 cycle of a
        // badline (cycles 15-54), into the matrix line buffer indexed by VMLI
        // and addressed by VC. The per-cycle replacement for the archive's
        // batched 40-read row fetch — same bytes (VC equals the geometry
        // address, proven by the shadow-counter increment), hardware-correct
        // timing. The Phi1 g-access in `render_pixels` already streamed per
        // cycle, so this closes the c-access half of the addressing rewrite.
        if self.is_badline && (15..=54).contains(&self.raster_cycle) {
            self.c_access(memory);
        }

        self.raster_cycle += 1;
        if self.raster_cycle >= self.cycles_per_line {
            self.raster_cycle = 0;
            self.raster_line += 1;

            if self.raster_line >= self.lines_per_frame {
                self.raster_line = 0;
                self.frame_complete = true;
                self.den_latch = false;
                self.lp_triggered = false;
            }
        }

        if self.raster_line == self.raster_compare && self.raster_cycle == 0 {
            self.irq_status |= 0x01;
        }

        self.irq = (self.irq_status & self.irq_enable & 0x0F) != 0;
        self.cpu_stalled
    }

    fn vram_addr(&self, bank_offset: u16) -> u16 {
        u16::from(self.vic_bank) * 0x4000 + (bank_offset & 0x3FFF)
    }

    fn check_badline(&mut self) {
        let den = self.regs[0x11] & 0x10 != 0;
        let yscroll = u16::from(self.regs[0x11] & 0x07);

        if self.raster_line == DISPLAY_START_LINE && den {
            self.den_latch = true;
        }

        self.is_badline = self.den_latch
            && (DISPLAY_START_LINE..DISPLAY_END_LINE).contains(&self.raster_line)
            && (self.raster_line & 7) == yscroll;
    }

    /// Advance the shadow VC/VCBASE/RC/VMLI chain for this cycle.
    ///
    /// Ported from VICE (`vicii-cycle.c` start-of-frame `:202-209`, UpdateVc
    /// `:543-549`, UpdateRc `:553-563`, badline-clears-idle `:51-59`;
    /// `vicii-fetch.c:267-269` for the g-access increment). Runs after
    /// `check_badline` so `is_badline` reflects this line. Cycle numbers are
    /// the engine's 0-based `raster_cycle`, which equals the canonical 1-based
    /// number for 1..=62 (see `oracle::engine_to_canonical`); the relevant
    /// events here (14, 16-55, 58) all fall in that range.
    ///
    /// **Shadow only** — nothing reads these counters yet; the geometry path
    /// still drives fetches. Increment 3 swaps the addressing over.
    fn advance_video_counters(&mut self) {
        let c = self.raster_cycle;

        // Start of frame: VC and VCBASE reset (VICE start_of_frame).
        if self.raster_line == 0 && c == 0 {
            self.vc = 0;
            self.vcbase = 0;
        }

        // A badline takes the chip out of idle (VICE check_badline).
        if self.is_badline {
            self.idle_state = false;
        }

        // UpdateVc — canonical cycle 14: reload VC from VCBASE, clear VMLI,
        // and (on a badline) reset the row counter.
        if c == 14 {
            self.vc = self.vcbase;
            self.vmli = 0;
            if self.is_badline {
                self.rc = 0;
            }
        }

        // g-access — canonical cycles 16-55: advance VC and VMLI once per
        // displayed character, but only while displaying (not idle).
        if (16..=55).contains(&c) && !self.idle_state {
            self.vc = (self.vc + 1) & 0x03FF;
            if self.vmli < 40 {
                self.vmli += 1;
            }
        }

        // UpdateRc — canonical cycle 58: at the end of an 8-row block latch
        // VCBASE and go idle; otherwise step the row counter.
        if c == 58 {
            if self.rc == 7 {
                self.idle_state = true;
                self.vcbase = self.vc;
            }
            if !self.idle_state || self.is_badline {
                self.rc = (self.rc + 1) & 0x07;
                self.idle_state = false;
            }
        }
    }

    /// Shadow video counter (VC). See [`Vic::advance_video_counters`].
    #[must_use]
    pub const fn vc(&self) -> u16 {
        self.vc
    }

    /// Shadow video counter base (VCBASE).
    #[must_use]
    pub const fn vcbase(&self) -> u16 {
        self.vcbase
    }

    /// Shadow row counter (RC), 0-7.
    #[must_use]
    pub const fn rc(&self) -> u8 {
        self.rc
    }

    /// Shadow video matrix line index (VMLI), 0-40.
    #[must_use]
    pub const fn vmli(&self) -> u8 {
        self.vmli
    }

    /// Update border FFs per Bauer's vic-ii.txt rules.
    ///
    /// **Vertical FF** (checked at line boundaries, i.e. cycle 0):
    /// - SET at last display line: 247 (RSEL=0) or 251 (RSEL=1).
    /// - CLEAR at first display line: 55 (RSEL=0) or 51 (RSEL=1), only
    ///   when DEN=1 at that moment.
    ///
    /// **Main FF** (checked every cycle):
    /// - SET at right edge cycle: 56 (CSEL=1) or 55 (CSEL=0).
    /// - CLEAR at left edge cycle: 16 (CSEL=1) or 17 (CSEL=0), only
    ///   when the vertical FF is clear.
    ///
    /// RSEL and CSEL are sampled at the moment each transition fires,
    /// enabling the classic "open the border" trick: software flicks
    /// RSEL/CSEL after the vertical FF would set, suppressing the
    /// transition and leaving the border FF clear.
    fn update_border_flip_flops(&mut self) {
        let rsel = self.regs[0x11] & 0x08 != 0;
        let den = self.regs[0x11] & 0x10 != 0;
        let csel = self.regs[0x16] & 0x08 != 0;

        // Vertical FF transitions fire on a line's first cycle.
        if self.raster_cycle == 0 {
            let last_display = if rsel { 251u16 } else { 247u16 };
            let first_display = if rsel { 51u16 } else { 55u16 };
            if self.raster_line == last_display {
                self.border_vert_ff = true;
            }
            if self.raster_line == first_display && den {
                self.border_vert_ff = false;
            }
        }

        // Main FF transitions fire on per-cycle boundaries.
        let left_edge = if csel { 16u8 } else { 17u8 };
        let right_edge = if csel { 56u8 } else { 55u8 };
        if self.raster_cycle == right_edge {
            self.border_main_ff = true;
        }
        if self.raster_cycle == left_edge && !self.border_vert_ff {
            self.border_main_ff = false;
        }
    }

    /// Single video-matrix c-access for the current cycle: read the screen
    /// code and colour nibble at VC into the matrix line buffer at VMLI.
    ///
    /// Streamed one-per-cycle across a badline's cycles 15-54 (replacing the
    /// archive's batched 40-read row fetch). The screen address is
    /// `screen_base + VC`, which equals the geometry path's
    /// `screen_base + text_row*40 + col` — VMLI tracks the column and VC the
    /// matrix offset, both proven against the geometry path in the
    /// shadow-counter increment.
    fn c_access(&mut self, memory: &dyn VicMemory) {
        let idx = self.vmli as usize;
        if idx >= self.screen_row.len() {
            return;
        }
        let addr = self.vram_addr(self.screen_base() + self.vc);
        let byte = memory.read_vram(addr);
        self.screen_row[idx] = byte;
        self.last_bus_data = byte;
        self.colour_row[idx] = memory.read_colour(self.vc);
    }

    /// The sprite whose p-access (pointer + data byte 0) falls on this cycle,
    /// and whether it targets the next line, per the model's sprite schedule
    /// (`SpriteTiming::paccess`, VICE `cycle_tab_*` SprPtr/SprDma0). The chain
    /// fetch (`chain_paccess`/`chain_saccess`) and the draw stage both key off
    /// this. PAL: sprites 0-2 fetch in the previous line's tail (58/60/62) for
    /// the next line, 3-7 on the current line (1/3/5/7/9). NTSC's two extra
    /// cycles shift this (see `SPRITE_TIMING_NTSC`).
    fn sprite_paccess_cycle(&self, cycle: u8) -> Option<(usize, bool)> {
        self.timing
            .paccess
            .iter()
            .position(|&(c, _)| c == cycle)
            .map(|i| (i, self.timing.paccess[i].1))
    }

    /// The sprite whose s-access (data bytes 1 and 2) falls on this cycle. The
    /// s-access is always the cycle after the sprite's p-access.
    fn sprite_saccess_cycle(&self, cycle: u8) -> Option<usize> {
        let cpl = self.cycles_per_line;
        self.timing
            .paccess
            .iter()
            .position(|&(c, _)| (c + 1) % cpl == cycle)
    }

    fn render_pixels(&mut self, memory: &dyn VicMemory) {
        // Cleared every cycle so the collision pass sees no foreground off the
        // display window (borders, retrace); render_pixels re-latches it below
        // only where graphics data is actually shifted out.
        self.gfx_fg_mask = 0;
        if self.raster_line < self.first_visible_line || self.raster_line >= self.last_visible_line
        {
            return;
        }
        if self.raster_cycle < FIRST_VISIBLE_CYCLE || self.raster_cycle >= LAST_VISIBLE_CYCLE {
            return;
        }

        let fb_y = (self.raster_line - self.first_visible_line) as usize;
        let fb_x = (self.raster_cycle - FIRST_VISIBLE_CYCLE) as usize * 8;
        let fb_offset = fb_y * FB_WIDTH as usize + fb_x;
        let border_colour = PALETTE[(self.regs[0x20] & 0x0F) as usize];
        let rsel = self.regs[0x11] & 0x08 != 0;
        let char_vstart = if rsel { 0x33u16 } else { 0x37u16 };
        let char_vstop = if rsel { 0xFBu16 } else { 0xF7u16 };
        let in_char_area = self.den_latch
            && (char_vstart..char_vstop).contains(&self.raster_line)
            && (DISPLAY_START_CYCLE..DISPLAY_END_CYCLE).contains(&self.raster_cycle);

        let mut fg_mask: u8 = 0;

        if self.raster_cycle == DISPLAY_START_CYCLE && in_char_area {
            self.xscroll_latch = self.regs[0x16] & 0x07;
            let bg = PALETTE[(self.regs[0x21] & 0x0F) as usize];
            self.xscroll_carry_pixels = [bg; 8];
            self.xscroll_carry_fg = 0;
        }

        if in_char_area {
            let display_cycle = self.raster_cycle - DISPLAY_START_CYCLE;
            let col = display_cycle as usize;

            if col < 40 {
                let char_code = self.screen_row[col];
                let colour_nybble = self.colour_row[col];
                let bmm = self.regs[0x11] & 0x20 != 0;
                let ecm = self.regs[0x11] & 0x40 != 0;
                let mcm = self.regs[0x16] & 0x10 != 0;

                let cell = if ecm && (bmm || mcm) {
                    CellPixels::solid(PALETTE[0])
                } else if bmm && mcm {
                    self.render_mcm_bitmap(char_code, colour_nybble, memory)
                } else if bmm {
                    self.render_hires_bitmap(char_code, memory)
                } else if ecm {
                    self.render_ecm_text(char_code, colour_nybble, memory)
                } else if mcm {
                    self.render_mcm_text(char_code, colour_nybble, memory)
                } else {
                    self.render_standard_text(char_code, colour_nybble, memory)
                };

                let xscroll = self.xscroll_latch as usize;

                if xscroll == 0 {
                    for px in 0..8usize {
                        let idx = fb_offset + px;
                        if idx < self.framebuffer.len() {
                            self.framebuffer[idx] = cell.colour[px];
                        }
                    }
                    fg_mask = cell.fg_mask;
                } else {
                    for px in 0..8usize {
                        let idx = fb_offset + px;
                        if idx < self.framebuffer.len() {
                            if px < xscroll {
                                self.framebuffer[idx] = self.xscroll_carry_pixels[px];
                                if (self.xscroll_carry_fg >> px) & 1 != 0 {
                                    fg_mask |= 1 << px;
                                }
                            } else {
                                self.framebuffer[idx] = cell.colour[px - xscroll];
                                if (cell.fg_mask >> (px - xscroll)) & 1 != 0 {
                                    fg_mask |= 1 << px;
                                }
                            }
                        }
                    }
                    for i in 0..xscroll {
                        self.xscroll_carry_pixels[i] = cell.colour[8 - xscroll + i];
                    }
                    self.xscroll_carry_fg =
                        (cell.fg_mask >> (8 - xscroll)) & ((1u8 << xscroll) - 1);
                }
            }
        }

        // Border overlay: paint border colour when the main FF is set.
        // The FFs are updated per-cycle by update_border_flip_flops(),
        // so this faithfully reproduces "open the border" tricks that
        // software uses to keep the main FF clear across a line.
        if self.border_main_ff {
            for px in 0..8usize {
                let idx = fb_offset + px;
                if idx < self.framebuffer.len() {
                    self.framebuffer[idx] = border_colour;
                }
            }
            fg_mask = 0;
        }

        // Latch this cycle's foreground mask for the every-cycle collision
        // pass, then composite the sprite pixels into the framebuffer. The
        // collision accumulation itself lives in `accumulate_sprite_collisions`
        // (called from `tick`) so it runs in the border too.
        self.gfx_fg_mask = fg_mask;
        self.draw_sprites_sequencer(fb_offset, fg_mask);
    }

    /// Accumulate sprite-sprite (`$D01E`) and sprite-background (`$D01F`)
    /// collisions from this cycle's sprite coverage, and raise the collision
    /// IRQs on the first hit. Runs every cycle — including the border and
    /// retrace — because the draw stage shifts sprite pixels out everywhere,
    /// and hardware detects collisions wherever that happens, not only inside
    /// the visible window. `gfx_fg_mask` is zero off the display window, so
    /// only sprite-sprite collisions register there.
    fn accumulate_sprite_collisions(&mut self) {
        let fg_mask = self.gfx_fg_mask;
        for px in 0..8usize {
            let cov = self.sprite_cycle_cov[px];
            if cov.count_ones() >= 2 {
                self.sprite_sprite_collision |= cov;
            }
            if cov != 0 && (fg_mask >> px) & 1 != 0 {
                self.sprite_bg_collision |= cov;
            }
        }

        if self.sprite_sprite_collision != 0 && !self.sprite_sprite_irq_latched {
            self.sprite_sprite_irq_latched = true;
            self.irq_status |= 0x04;
        }
        if self.sprite_bg_collision != 0 && !self.sprite_bg_irq_latched {
            self.sprite_bg_irq_latched = true;
            self.irq_status |= 0x02;
        }
    }

    fn render_standard_text(
        &self,
        char_code: u8,
        colour_nybble: u8,
        memory: &dyn VicMemory,
    ) -> CellPixels {
        let bg_colour = PALETTE[(self.regs[0x21] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x0F) as usize];
        let char_base = self.char_base();
        let bitmap_addr = char_base + u16::from(char_code) * 8 + u16::from(self.rc);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            if bit != 0 {
                cell.fg_mask |= 1 << px;
                cell.colour[px] = fg_colour;
            } else {
                cell.colour[px] = bg_colour;
            }
        }
        cell
    }

    fn render_hires_bitmap(&self, char_code: u8, memory: &dyn VicMemory) -> CellPixels {
        let fg_colour = PALETTE[((char_code >> 4) & 0x0F) as usize];
        let bg_colour = PALETTE[(char_code & 0x0F) as usize];
        let bitmap_base = self.bitmap_base();
        // g-access via the video counter: (VC << 3) | RC + bitmap base, the
        // hardware addressing (VICE `g_fetch_addr`, vicii-fetch.c:169). VC at
        // render equals text_row*40 + col, RC the character sub-row.
        let bitmap_addr = bitmap_base + self.vc * 8 + u16::from(self.rc);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            if bit != 0 {
                cell.fg_mask |= 1 << px;
                cell.colour[px] = fg_colour;
            } else {
                cell.colour[px] = bg_colour;
            }
        }
        cell
    }

    fn render_ecm_text(
        &self,
        char_code: u8,
        colour_nybble: u8,
        memory: &dyn VicMemory,
    ) -> CellPixels {
        let bg_select = (char_code >> 6) & 0x03;
        let bg_colour = PALETTE[(self.regs[0x21 + bg_select as usize] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x0F) as usize];
        let char_base = self.char_base();
        let effective_char = char_code & 0x3F;
        let bitmap_addr = char_base + u16::from(effective_char) * 8 + u16::from(self.rc);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            if bit != 0 {
                cell.fg_mask |= 1 << px;
                cell.colour[px] = fg_colour;
            } else {
                cell.colour[px] = bg_colour;
            }
        }
        cell
    }

    fn render_mcm_text(
        &self,
        char_code: u8,
        colour_nybble: u8,
        memory: &dyn VicMemory,
    ) -> CellPixels {
        if colour_nybble & 0x08 == 0 {
            return self.render_standard_text(char_code, colour_nybble, memory);
        }

        let bg0 = PALETTE[(self.regs[0x21] & 0x0F) as usize];
        let bg1 = PALETTE[(self.regs[0x22] & 0x0F) as usize];
        let bg2 = PALETTE[(self.regs[0x23] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x07) as usize];
        let char_base = self.char_base();
        let bitmap_addr = char_base + u16::from(char_code) * 8 + u16::from(self.rc);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for pair in 0..4usize {
            let bits = (bitmap >> (6 - pair * 2)) & 0x03;
            let colour = match bits {
                0b00 => bg0,
                0b01 => bg1,
                0b10 => bg2,
                _ => fg_colour,
            };
            let is_fg = bits != 0b00;
            let px0 = pair * 2;
            let px1 = px0 + 1;
            if is_fg {
                cell.fg_mask |= (1 << px0) | (1 << px1);
            }
            cell.colour[px0] = colour;
            cell.colour[px1] = colour;
        }
        cell
    }

    fn render_mcm_bitmap(
        &self,
        char_code: u8,
        colour_nybble: u8,
        memory: &dyn VicMemory,
    ) -> CellPixels {
        let bg0 = PALETTE[(self.regs[0x21] & 0x0F) as usize];
        let c01 = PALETTE[((char_code >> 4) & 0x0F) as usize];
        let c10 = PALETTE[(char_code & 0x0F) as usize];
        let c11 = PALETTE[(colour_nybble & 0x0F) as usize];
        let bitmap_base = self.bitmap_base();
        // g-access via the video counter: (VC << 3) | RC + bitmap base, the
        // hardware addressing (VICE `g_fetch_addr`, vicii-fetch.c:169). VC at
        // render equals text_row*40 + col, RC the character sub-row.
        let bitmap_addr = bitmap_base + self.vc * 8 + u16::from(self.rc);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for pair in 0..4usize {
            let bits = (bitmap >> (6 - pair * 2)) & 0x03;
            let colour = match bits {
                0b00 => bg0,
                0b01 => c01,
                0b10 => c10,
                _ => c11,
            };
            let is_fg = bits != 0b00;
            let px0 = pair * 2;
            let px1 = px0 + 1;
            if is_fg {
                cell.fg_mask |= (1 << px0) | (1 << px1);
            }
            cell.colour[px0] = colour;
            cell.colour[px1] = colour;
        }
        cell
    }

    /// Each sprite's leftmost framebuffer X (VIC X + the sprite-to-screen
    /// offset, honouring the `$D010` high bits).
    fn sprite_fb_x_array(&self) -> [i32; 8] {
        let mut x = [0i32; 8];
        for (i, slot) in x.iter_mut().enumerate() {
            let sprite_x = u16::from(self.regs[i * 2])
                | if self.regs[0x10] & (1 << i) != 0 {
                    256
                } else {
                    0
                };
            *slot = i32::from(sprite_x) + i32::from(SPRITE_X_TO_FB);
        }
        x
    }

    /// Each sprite's Y position (`$D001+2i`).
    fn sprite_y_array(&self) -> [u8; 8] {
        let mut y = [0u8; 8];
        for (i, slot) in y.iter_mut().enumerate() {
            *slot = self.regs[1 + i * 2];
        }
        y
    }

    /// Chain stage (VICE `vicii-cycle.c` sprite events): run MCBASE update
    /// (cyc 16), DMA check (55/56), expansion check (56), and display check
    /// (58), then the chain-driven MC-addressed p/s-access filling `chain_data`.
    ///
    /// The Y compare is **VICE-literal** (`Y == raster_line`, no `+1`) so
    /// per-line `$D015`/`$D001` writes are sampled at VICE's cycles, and
    /// **non-wrapping** (activation gated to `raster_line <= 255`) to suppress
    /// the raster-306 re-match VICE also hides (mechanism unpinned). The draw
    /// stage (`run_sprite_draw_cycle`) consumes the display bits + data.
    fn advance_sprite_chain(&mut self, memory: &dyn VicMemory) {
        let c = self.raster_cycle;
        let enable = self.regs[0x15];
        let y = self.sprite_y_array();
        let raster_low = self.raster_line as u8;

        if c == 16 {
            self.chain.update_mcbase();
        }
        if (c == self.timing.chk_dma[0] || c == self.timing.chk_dma[1]) && self.raster_line <= 255 {
            self.chain.check_dma(enable, y, raster_low);
        }
        if c == self.timing.chk_exp {
            self.chain.check_exp(self.regs[0x17]);
        }
        if c == self.timing.chk_disp {
            self.chain.check_display(enable, y, raster_low);
        }

        if let Some((i, _)) = self.sprite_paccess_cycle(c) {
            self.chain_paccess(memory, i);
        }
        if let Some(i) = self.sprite_saccess_cycle(c) {
            self.chain_saccess(memory, i);
        }
    }

    /// Draw stage (VICE `vicii-draw-cycle.c` `draw_sprites8`): produce this
    /// cycle's 8 raw sprite pixels into `sprite_cycle_px`, interleaving the
    /// per-pixel DMA housekeeping — deactivate on the s-access (pixel 2), halt on
    /// the p-access (pixel 3), latch pending + load data on the s-access
    /// (pixel 4), un-halt on the s-access (pixel 7). Halted sprites freeze their
    /// shift register (the sprite-fetch artifacts). Runs every cycle; the
    /// foreground-priority composite happens in `render_pixels`.
    fn run_sprite_draw_cycle(&mut self) {
        let c = self.raster_cycle;
        let expx = self.regs[0x1D];
        let dma0 = self.sprite_paccess_cycle(c).map(|(i, _)| i);
        let dma2 = self.sprite_saccess_cycle(c);
        let xpos_base = (i32::from(c) - i32::from(FIRST_VISIBLE_CYCLE)) * 8;

        self.sprite_sequencer
            .set_x_positions(self.sprite_fb_x_array());
        self.sprite_sequencer.set_mc_bits(self.regs[0x1C]);

        for px in 0..8usize {
            match px {
                2 => {
                    if let Some(i) = dma2 {
                        self.sprite_sequencer.clear_active(i);
                    }
                }
                3 => {
                    if let Some(i) = dma0 {
                        self.sprite_sequencer.set_halt(i);
                    }
                }
                4 => {
                    if c == self.timing.chk_disp {
                        self.sprite_sequencer.set_pending(self.chain.display_bits());
                    }
                    if let Some(i) = dma2 {
                        let d = self.chain_data[i];
                        let data =
                            (u32::from(d[0]) << 16) | (u32::from(d[1]) << 8) | u32::from(d[2]);
                        self.sprite_sequencer.load_data(i, data);
                    }
                }
                7 => {
                    if let Some(i) = dma2 {
                        self.sprite_sequencer.clear_halt(i);
                    }
                }
                _ => {}
            }
            let drawn = self
                .sprite_sequencer
                .draw_pixel(xpos_base + px as i32, expx);
            self.sprite_cycle_px[px] = drawn.winner;
            self.sprite_cycle_cov[px] = drawn.coverage;
        }
    }

    /// Chain p-access: read the sprite pointer and data byte 0 addressed by MC,
    /// latch the pointer base, advance MC. Gated by the chain's DMA state.
    fn chain_paccess(&mut self, memory: &dyn VicMemory, i: usize) {
        if !self.chain.dma_active(i) {
            return;
        }
        let ptr_addr = self.screen_base() + 0x03F8 + i as u16;
        let base = u16::from(memory.read_vram(self.vram_addr(ptr_addr))) << 6;
        self.chain_fetch_base[i] = base;
        let mc = self.chain.mc(i);
        self.chain_data[i][0] = memory.read_vram(self.vram_addr(base + u16::from(mc)));
        // The sprite byte is the VIC's last bus access — drives open-bus reads
        // ($2F-$3F). (Was set by the removed overlay fetch; the chain reads the
        // same steady-state bytes, so this keeps open-bus behaviour.)
        self.last_bus_data = self.chain_data[i][0];
        self.chain.advance_mc(i);
    }

    /// Chain s-access: read data bytes 1 and 2 (advancing MC) into `chain_data`.
    /// The draw stage loads them into the shift register at pixel 4 of this
    /// cycle (VICE `update_sprite_data`).
    fn chain_saccess(&mut self, memory: &dyn VicMemory, i: usize) {
        if !self.chain.dma_active(i) {
            return;
        }
        let base = self.chain_fetch_base[i];
        let mc1 = self.chain.mc(i);
        self.chain_data[i][1] = memory.read_vram(self.vram_addr(base + u16::from(mc1)));
        self.chain.advance_mc(i);
        let mc2 = self.chain.mc(i);
        self.chain_data[i][2] = memory.read_vram(self.vram_addr(base + u16::from(mc2)));
        self.last_bus_data = self.chain_data[i][2];
        self.chain.advance_mc(i);
    }

    /// Composite this cycle's raw sprite pixels (produced by the draw stage,
    /// `run_sprite_draw_cycle`) into the framebuffer, applying sprite-behind-
    /// foreground priority (`$D01B` + `fg_mask`).
    fn draw_sprites_sequencer(&mut self, fb_offset: usize, fg_mask: u8) {
        let priority = self.regs[0x1B];
        for px in 0..8usize {
            // Collision accumulation runs every cycle in
            // `accumulate_sprite_collisions`; this pass only composites the
            // visible pixels, honouring sprite-behind-foreground priority.
            let Some(sp) = self.sprite_cycle_px[px] else {
                continue;
            };
            let i = sp.sprite as usize;
            if priority & (1 << i) != 0 && (fg_mask >> px) & 1 != 0 {
                continue;
            }
            let colour = match sp.selector {
                1 => PALETTE[(self.regs[0x25] & 0x0F) as usize],
                3 => PALETTE[(self.regs[0x26] & 0x0F) as usize],
                _ => PALETTE[(self.regs[0x27 + i] & 0x0F) as usize],
            };
            let idx = fb_offset + px;
            if idx < self.framebuffer.len() {
                self.framebuffer[idx] = colour;
            }
        }
    }

    fn evaluate_sprite_dma(&mut self) {
        let sprite_enable = self.regs[0x15];
        let y_expand = self.regs[0x17];

        for i in 0..8usize {
            if sprite_enable & (1 << i) == 0 {
                self.sprite_dma_active[i] = false;
                continue;
            }

            let sprite_y = u16::from(self.regs[1 + i * 2]);
            let height = if y_expand & (1 << i) != 0 {
                42u16
            } else {
                21u16
            };
            let offset = self.raster_line.wrapping_sub(sprite_y);
            self.sprite_dma_active[i] = offset < height;
        }
    }

    fn is_sprite_dma_stealing(&self) -> bool {
        let c = self.raster_cycle;
        let cpl = self.cycles_per_line;
        // A sprite steals the bus on its p-access and s-access (the next
        // cycle), per the model's schedule.
        self.timing
            .paccess
            .iter()
            .enumerate()
            .any(|(i, &(p, _))| self.sprite_dma_active[i] && (c == p || c == (p + 1) % cpl))
    }

    fn compute_ba_low(&self) -> bool {
        self.badline_ba_low() || self.sprite_ba_low()
    }

    fn badline_ba_low(&self) -> bool {
        self.is_badline && (12..=54).contains(&self.raster_cycle)
    }

    fn sprite_ba_low(&self) -> bool {
        let c = self.raster_cycle;
        let cpl = self.cycles_per_line;
        // BA drops for the three cycles before a sprite's p-access through its
        // s-access (a 5-cycle window), per the model's schedule.
        self.timing.paccess.iter().enumerate().any(|(i, &(p, _))| {
            if !self.sprite_dma_active[i] {
                return false;
            }
            let ba_start = (p + cpl - 3) % cpl;
            let ba_end = (p + 1) % cpl;
            if ba_start <= ba_end {
                c >= ba_start && c <= ba_end
            } else {
                c >= ba_start || c <= ba_end
            }
        })
    }

    fn screen_base(&self) -> u16 {
        u16::from((self.regs[0x18] >> 4) & 0x0F) * 0x0400
    }

    fn char_base(&self) -> u16 {
        u16::from((self.regs[0x18] >> 1) & 0x07) * 0x0800
    }

    fn bitmap_base(&self) -> u16 {
        if self.regs[0x18] & 0x08 != 0 {
            0x2000
        } else {
            0x0000
        }
    }

    /// Read a VIC-II register.
    pub fn read(&mut self, reg: u8) -> u8 {
        // Per reference: $D019 bits 6:4 read as 1, $D01A bits 7:4 read
        // as 1, and unused bits on colour regs ($D020-$D02E) read as 1.
        match reg & 0x3F {
            0x11 => {
                (self.regs[0x11] & 0x7F)
                    | if self.raster_line & 0x100 != 0 {
                        0x80
                    } else {
                        0x00
                    }
            }
            0x12 => (self.raster_line & 0xFF) as u8,
            0x19 => {
                let composite = if (self.irq_status & self.irq_enable & 0x0F) != 0 {
                    0x80
                } else {
                    0x00
                };
                self.irq_status | composite | 0x70
            }
            0x1A => (self.irq_enable & 0x0F) | 0xF0,
            0x1E => {
                let val = self.sprite_sprite_collision;
                self.sprite_sprite_collision = 0;
                self.sprite_sprite_irq_latched = false;
                val
            }
            0x1F => {
                let val = self.sprite_bg_collision;
                self.sprite_bg_collision = 0;
                self.sprite_bg_irq_latched = false;
                val
            }
            r @ 0x20..=0x2E => self.regs[r as usize] | 0xF0,
            // Sprite coordinates ($00-$10), control/pointer registers
            // ($13-$18) and the sprite priority/multicolour/expand-X
            // registers ($1B-$1D) are all readable and return the last
            // written value — exactly what the hardware does, and what
            // read-modify-write movement code (`inc $d000`, `dec $d001`)
            // depends on. Only the special-cased registers above diverge.
            r @ (0x00..=0x10 | 0x13..=0x18 | 0x1B..=0x1D) => self.regs[r as usize],
            // $2F-$3F are unused; modelled as open bus.
            _ => self.last_bus_data,
        }
    }

    /// Read a register without side effects.
    #[must_use]
    pub fn peek(&self, reg: u8) -> u8 {
        match reg & 0x3F {
            0x11 => {
                (self.regs[0x11] & 0x7F)
                    | if self.raster_line & 0x100 != 0 {
                        0x80
                    } else {
                        0x00
                    }
            }
            0x12 => (self.raster_line & 0xFF) as u8,
            // peek() returns the same composite IRR, but we keep the
            // raw peek semantics — callers that want the canonical
            // silicon-observable read mask should use read() instead.
            0x19 => {
                self.irq_status
                    | if (self.irq_status & self.irq_enable & 0x0F) != 0 {
                        0x80
                    } else {
                        0x00
                    }
            }
            0x1A => self.irq_enable & 0x0F,
            0x1E => self.sprite_sprite_collision,
            0x1F => self.sprite_bg_collision,
            r if r <= 0x2E => self.regs[r as usize],
            _ => self.last_bus_data,
        }
    }

    /// Write a VIC-II register.
    pub fn write(&mut self, reg: u8, value: u8) {
        let r = (reg & 0x3F) as usize;
        let old = if r < self.regs.len() { self.regs[r] } else { 0 };
        if r < self.regs.len() {
            self.regs[r] = value;
        }

        match reg & 0x3F {
            0x11 => {
                self.raster_compare =
                    (self.raster_compare & 0x00FF) | (u16::from(value & 0x80) << 1);
            }
            0x12 => {
                self.raster_compare = (self.raster_compare & 0x0100) | u16::from(value);
            }
            0x19 => {
                self.irq_status &= !value & 0x0F;
            }
            0x1A => {
                self.irq_enable = value & 0x0F;
            }
            // Sprite crunch: a `$D017` change feeds the fetch chain's crunch
            // bit-math (gated on the crunch cycle).
            0x17 if value != old => {
                self.chain.write_d017(value, self.raster_cycle == 15);
            }
            _ => {}
        }

        self.irq = (self.irq_status & self.irq_enable & 0x0F) != 0;
    }

    /// Whether the IRQ pin is asserted.
    #[must_use]
    pub const fn irq_active(&self) -> bool {
        self.irq
    }

    /// Whether BA is asserted low.
    #[must_use]
    pub const fn ba_is_low(&self) -> bool {
        self.ba_low
    }

    /// Set the active VIC bank.
    pub fn set_bank(&mut self, bank: u8) {
        self.vic_bank = bank & 0x03;
    }

    /// Current VIC bank.
    #[must_use]
    pub const fn bank(&self) -> u8 {
        self.vic_bank
    }

    /// Trigger the light-pen latch once per frame.
    pub fn trigger_light_pen(&mut self) {
        if self.lp_triggered {
            return;
        }
        self.lp_triggered = true;
        self.regs[0x13] = (u16::from(self.raster_cycle) * 4) as u8;
        self.regs[0x14] = self.raster_line as u8;
    }

    /// Borrow the ARGB32 framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        u32::from(self.last_visible_line - self.first_visible_line)
    }

    /// Check and clear the frame-complete flag.
    pub fn take_frame_complete(&mut self) -> bool {
        let complete = self.frame_complete;
        self.frame_complete = false;
        complete
    }

    /// Current raster line.
    #[must_use]
    pub const fn raster_line(&self) -> u16 {
        self.raster_line
    }

    /// Current cycle within the raster line.
    #[must_use]
    pub const fn raster_cycle(&self) -> u8 {
        self.raster_cycle
    }

    /// Current character row within an 8-line character cell — the VIC-II row
    /// counter (RC), which drives the g-access sub-row addressing.
    #[must_use]
    pub const fn char_row(&self) -> u8 {
        self.rc
    }

    /// Whether the current line is a bad line.
    #[must_use]
    pub const fn is_badline(&self) -> bool {
        self.is_badline
    }

    /// Borrow the raw register file.
    #[must_use]
    pub const fn registers(&self) -> &[u8; 0x40] {
        &self.regs
    }

    /// Restore the raw register file from saved state.
    pub fn set_registers(&mut self, regs: &[u8; 0x40]) {
        self.regs = *regs;
        self.raster_compare = u16::from(self.regs[0x12]) | (u16::from(self.regs[0x11] & 0x80) << 1);
        self.irq_enable = self.regs[0x1A] & 0x0F;
    }

    /// Snapshot of the IRQ status register.
    #[must_use]
    pub const fn irq_status(&self) -> u8 {
        self.irq_status
    }

    /// Restore the IRQ status register.
    pub fn set_irq_status(&mut self, val: u8) {
        self.irq_status = val;
    }
}

impl Default for Vic {
    fn default() -> Self {
        Self::new(VicModel::Pal6569)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES_PER_FRAME: u16 = 312;
    const CYCLES_PER_LINE: u8 = 63;
    const FIRST_VISIBLE_LINE: u16 = PAL_FIRST_VISIBLE_LINE;

    struct TestMemory {
        ram: Box<[u8; 0x10000]>,
        char_rom: Vec<u8>,
        colour_ram: Vec<u8>,
    }

    impl TestMemory {
        fn new(chargen: &[u8]) -> Self {
            Self {
                ram: Box::new([0; 0x10000]),
                char_rom: chargen.to_vec(),
                colour_ram: vec![0; 1024],
            }
        }

        fn with_colour(chargen: &[u8], colour_ram: Vec<u8>) -> Self {
            Self {
                ram: Box::new([0; 0x10000]),
                char_rom: chargen.to_vec(),
                colour_ram,
            }
        }

        fn ram_write(&mut self, addr: u16, value: u8) {
            self.ram[addr as usize] = value;
        }
    }

    impl VicMemory for TestMemory {
        fn read_vram(&self, addr: u16) -> u8 {
            let bank = (addr >> 14) & 0x03;
            let bank_addr = addr & 0x3FFF;
            if (bank == 0 || bank == 2) && (0x1000..0x2000).contains(&bank_addr) {
                self.char_rom[(bank_addr - 0x1000) as usize]
            } else {
                self.ram[addr as usize]
            }
        }

        fn read_colour(&self, offset: u16) -> u8 {
            self.colour_ram
                .get(offset as usize)
                .copied()
                .map(|v| v & 0x0F)
                .unwrap_or(0)
        }
    }

    fn make_vic_and_memory() -> (Vic, TestMemory) {
        make_vic_and_memory_model(VicModel::Pal6569)
    }

    fn make_vic_and_memory_model(model: VicModel) -> (Vic, TestMemory) {
        let chargen = vec![0xFF; 4096];
        let vic = Vic::new(model);
        let memory = TestMemory::new(&chargen);
        (vic, memory)
    }

    /// Tick until the VIC reaches exactly (`line`, `cycle`). Model-agnostic
    /// (unlike `advance_to`, which assumes PAL's cycle count); terminates within
    /// one frame for any valid target.
    fn advance_until(vic: &mut Vic, memory: &TestMemory, line: u16, cycle: u8) {
        while vic.raster_line() != line || vic.raster_cycle() != cycle {
            tick_vic(vic, memory);
        }
    }

    fn tick_vic(vic: &mut Vic, mem: &TestMemory) -> bool {
        vic.tick(mem)
    }

    fn advance_to(vic: &mut Vic, memory: &TestMemory, line: u16, cycle: u8) {
        let target = u32::from(line) * u32::from(CYCLES_PER_LINE) + u32::from(cycle);
        for _ in 0..target {
            tick_vic(vic, memory);
        }
    }

    fn fb_pixel(vic: &Vic, fb_x: usize, fb_y: usize) -> u32 {
        vic.framebuffer()[fb_y * FB_WIDTH as usize + fb_x]
    }

    /// Render one display line (100) carrying three sprites — a hires sprite, a
    /// multicolour sprite, and an X-expanded sprite — through the draw-stage
    /// sequencer, and return the whole framebuffer.
    fn render_three_sprite_scene() -> Vec<u32> {
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // DEN + display on
        vic.write(0x18, 0x14);
        vic.write(0x15, 0x07); // enable sprites 0,1,2
        vic.write(0x1C, 0x02); // sprite 1 multicolour
        vic.write(0x1D, 0x04); // sprite 2 X-expanded
        vic.write(0x25, 0x0A); // MC0
        vic.write(0x26, 0x0D); // MC1
        // Positions (all on line 100) and colours.
        vic.write(0x01, 100);
        vic.write(0x00, 180); // sprite 0 X
        vic.write(0x27, 0x01);
        vic.write(0x03, 100);
        vic.write(0x02, 120); // sprite 1 X
        vic.write(0x28, 0x03);
        vic.write(0x05, 100);
        vic.write(0x04, 60); // sprite 2 X
        vic.write(0x29, 0x05);
        // Sprite pointers → data blocks at $2000/$2040/$2080.
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x07F9, 0x81);
        memory.ram_write(0x07FA, 0x82);
        for k in 0..3u16 {
            memory.ram_write(0x2000 + k, [0xFF, 0x99, 0x3C][k as usize]);
            memory.ram_write(0x2040 + k, [0x1B, 0xE4, 0x5A][k as usize]);
            memory.ram_write(0x2080 + k, [0xC3, 0x66, 0xFF][k as usize]);
        }
        advance_to(&mut vic, &memory, 103, 0);
        vic.framebuffer().to_vec()
    }

    /// The draw-stage sequencer renders three coexisting sprites (hires +
    /// multicolour + X-expanded) on one line. In this *isolated* synthetic
    /// harness the continuous chain feed places the sprites one line later than
    /// their Y register (first-activation vs steady-state timing); real-program
    /// phase is validated by the testbench `sprite_sequencer_spritedma_parity`
    /// (0 px vs VICE). Assert the scene draws sprite pixels on that line.
    #[test]
    fn sprite_sequencer_renders_three_sprite_scene() {
        let fb = render_three_sprite_scene();
        let sq_row = (101 - FIRST_VISIBLE_LINE) as usize * FB_WIDTH as usize;
        let border = fb[0];
        let sprite_px = (0..FB_WIDTH as usize)
            .filter(|&x| fb[sq_row + x] != border)
            .count();
        assert!(
            sprite_px > 0,
            "scene should render sprite pixels on line 101"
        );
    }

    /// A deterministic full PAL frame rendered entirely from `TestMemory` — no
    /// ROMs, no CPU — exercising the render paths the audit flagged: standard
    /// text mode, the border flip-flops, palette mapping (border / background /
    /// foreground / sprite), and the draw-stage sprite sequencer.
    fn render_golden_scene() -> Vec<u32> {
        // Char 1's glyph is $AA (1010_1010) on every row, so each cell
        // alternates foreground and background pixels left to right.
        let mut chargen = vec![0xFFu8; 4096];
        for row in &mut chargen[8..16] {
            *row = 0xAA;
        }
        let mut memory = TestMemory::with_colour(&chargen, vec![0x01; 1024]); // white fg
        let mut vic = Vic::new(VicModel::Pal6569);

        vic.write(0x11, 0x1B); // DEN on, standard text mode, RSEL
        vic.write(0x16, 0x08); // CSEL, XSCROLL 0
        vic.write(0x18, 0x14); // screen matrix $0400, char base $1000 (char ROM)
        vic.write(0x20, 0x0E); // border light blue
        vic.write(0x21, 0x06); // background blue

        // The whole screen matrix is char 1.
        for offset in 0..0x0400u16 {
            memory.ram_write(0x0400 + offset, 0x01);
        }

        // One solid hires sprite so the mux is exercised.
        vic.write(0x15, 0x01); // enable sprite 0
        vic.write(0x00, 100); // X
        vic.write(0x01, 80); // Y
        vic.write(0x27, 0x03); // sprite colour cyan
        memory.ram_write(0x07F8, 0x80); // pointer → $2000
        for k in 0..63u16 {
            memory.ram_write(0x2000 + k, 0xFF);
        }

        advance_to(&mut vic, &memory, 311, 62); // a full PAL frame
        vic.framebuffer().to_vec()
    }

    fn fnv1a_u32(data: &[u32]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for &word in data {
            for byte in word.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }

    /// Render-accuracy regression floor (#768): a hermetic golden frame that
    /// runs in **default** CI, unlike the ROM-gated VICE-parity testbench. It
    /// locks the VIC-II's rendered output for a scene covering text-mode
    /// dispatch, the border flip-flops, palette mapping, and the sprite
    /// sequencer, so a regression to any of them fails loudly instead of
    /// slipping through. If this fails after an *intentional* VIC-II change,
    /// re-bless `GOLDEN_FRAME_HASH` (and bump `FRAME_ROUTING_VERSION` if
    /// catalogue frames move).
    #[test]
    fn render_accuracy_floor_locks_a_golden_frame() {
        const GOLDEN_FRAME_HASH: u64 = 0x0fe7_18f9_1ddb_3dfd;

        let fb = render_golden_scene();
        assert_eq!(fb.len(), (FB_WIDTH * FB_HEIGHT) as usize, "full PAL frame");

        // Named pixel-parity checks aid diagnosis; the hash is the full floor.
        let px = |x: usize, y: usize| fb[y * FB_WIDTH as usize + x];
        assert_eq!(px(0, 0), PALETTE[0x0E], "top-left is border colour");
        // Display starts at cycle 16 → fb_x 48. Char $AA: pixel 0 (bit 7) is
        // foreground, pixel 1 (bit 6) is background.
        assert_eq!(px(48, 80), PALETTE[0x01], "char foreground pixel");
        assert_eq!(px(49, 80), PALETTE[0x06], "char background pixel");

        let hash = fnv1a_u32(&fb);
        assert_eq!(
            hash, GOLDEN_FRAME_HASH,
            "VIC-II render drifted; re-bless if intentional. got {hash:#018x}"
        );
    }

    #[test]
    fn initial_state() {
        let mut vic = Vic::new(VicModel::Pal6569);
        assert_eq!(vic.raster_line(), 0);
        assert_eq!(vic.raster_cycle(), 0);
        assert!(!vic.irq_active());
        assert!(!vic.take_frame_complete());
        assert!(!vic.irq);
        assert!(!vic.ba_low);
    }

    #[test]
    fn raster_advances() {
        let (mut vic, memory) = make_vic_and_memory();
        for _ in 0..63 {
            tick_vic(&mut vic, &memory);
        }
        assert_eq!(vic.raster_line(), 1);
        assert_eq!(vic.raster_cycle(), 0);
    }

    #[test]
    fn frame_complete_after_full_frame() {
        let (mut vic, memory) = make_vic_and_memory();
        let total_cycles = u32::from(LINES_PER_FRAME) * u32::from(CYCLES_PER_LINE);
        for _ in 0..total_cycles {
            tick_vic(&mut vic, &memory);
        }
        assert!(vic.take_frame_complete());
        assert!(!vic.take_frame_complete());
    }

    #[test]
    fn raster_irq_fires_and_acknowledges() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x12, 1);
        vic.write(0x1A, 0x01);

        for _ in 0..63 {
            tick_vic(&mut vic, &memory);
        }
        assert!(vic.irq_active());
        assert!(vic.irq);

        vic.write(0x19, 0x01);
        assert!(!vic.irq_active());
        assert!(!vic.irq);
    }

    /// Seam 1 audit (`c64-architecture-review.md`): the raster-compare
    /// IRQ must fire at the exact phi2 cycle where the raster latch
    /// reaches the compare value — not one cycle early, not one cycle
    /// late. Demos use raster IRQs to drive single-scanline split
    /// effects; a one-cycle drift here breaks every raster effect.
    ///
    /// On the C64, the IRQ asserts at cycle 0 of the matching line
    /// (per Mäkelä §3.12). The `irq` pin is sampled by the CPU on the
    /// following phi2 high. The test asserts exactly that:
    ///   - At the end of the tick processing cycle 0 of line N,
    ///     `vic.irq` is true.
    ///   - At the end of the previous tick (line N-1, cycle 62 in PAL),
    ///     `vic.irq` is still false.
    ///   - At the end of the tick processing cycle 1 of line N,
    ///     `vic.irq` is still true (latched until ack).
    #[test]
    fn raster_irq_asserts_on_exact_phi2_of_compare_match() {
        let (mut vic, memory) = make_vic_and_memory();
        // Compare raster = 5. Enable raster IRQ.
        vic.write(0x12, 5);
        vic.write(0x1A, 0x01);

        // Walk through line 4 cycle 62 (one tick before the IRQ).
        advance_to(&mut vic, &memory, 4, 62);
        // We're now at (line 4, cycle 62) post-advance: the next tick
        // processes line 4 cycle 62. Confirm pre-state.
        assert_eq!(vic.raster_line(), 4);
        assert_eq!(vic.raster_cycle(), 62);
        assert!(!vic.irq, "IRQ should not be asserted before compare line");

        // Process line 4 cycle 62 — last cycle before the IRQ.
        tick_vic(&mut vic, &memory);
        assert_eq!(vic.raster_line(), 5);
        assert_eq!(vic.raster_cycle(), 0);
        // The end-of-tick irq_status |= 0x01 happens *after* the line
        // wrap, so vic.irq should already be high here.
        assert!(
            vic.irq,
            "IRQ must assert at the phi2 boundary entering compare line"
        );

        // Process line 5 cycle 0 — IRQ remains latched.
        tick_vic(&mut vic, &memory);
        assert!(vic.irq, "IRQ remains latched until ack");

        // Ack via $D019 — write 1 to bit 0 to clear.
        vic.write(0x19, 0x01);
        assert!(!vic.irq, "IRQ cleared after ack");
    }

    /// Seam 1 audit: with raster IRQ enabled but the compare value
    /// never matching, `vic.irq` must remain low across a full frame.
    /// Catches a class of bug where the IRQ asserts spuriously
    /// (e.g. comparing against an uninitialised latch).
    #[test]
    fn raster_irq_does_not_fire_when_compare_never_matches() {
        let (mut vic, memory) = make_vic_and_memory();
        // Compare raster = LINES_PER_FRAME (out of range — never
        // matches).
        let unreachable = LINES_PER_FRAME;
        vic.write(0x12, unreachable as u8);
        vic.write(0x11, ((unreachable >> 8) << 7) as u8);
        vic.write(0x1A, 0x01);

        let total_cycles = u32::from(LINES_PER_FRAME) * u32::from(CYCLES_PER_LINE);
        for _ in 0..total_cycles {
            tick_vic(&mut vic, &memory);
            assert!(
                !vic.irq,
                "spurious IRQ at line {} cycle {}",
                vic.raster_line, vic.raster_cycle
            );
        }
    }

    #[test]
    fn framebuffer_size() {
        let vic = Vic::new(VicModel::Pal6569);
        assert_eq!(
            vic.framebuffer().len(),
            FB_WIDTH as usize * FB_HEIGHT as usize
        );
    }

    #[test]
    fn register_read_write() {
        // Colour registers $D020-$D02E: bits 7:4 read as 1 per reference,
        // so writing $06 reads back as $F6 and $01 reads back as $F1.
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x20, 0x06);
        assert_eq!(vic.read(0x20), 0xF6);
        vic.write(0x21, 0x01);
        assert_eq!(vic.read(0x21), 0xF1);
    }

    #[test]
    fn sprite_position_registers_read_back_written_value() {
        // Sprite coordinate registers are readable and must return the
        // last written value — read-modify-write movement code
        // (`inc $d000` / `dec $d001`) relies on it. Regression for the
        // bug where these fell through to open-bus data, so every RMW
        // computed from stale data: the sprite jumped once then froze.
        let mut vic = Vic::new(VicModel::Pal6569);
        for reg in 0x00u8..=0x10 {
            vic.write(reg, 0xAB);
            assert_eq!(vic.read(reg), 0xAB, "register ${reg:02X} should read back");
        }
        // Read-modify-write semantics: write 172, decrement twice, expect 170.
        vic.write(0x01, 172);
        let after_two_dec = vic.read(0x01).wrapping_sub(1).wrapping_sub(1);
        vic.write(0x01, after_two_dec);
        assert_eq!(vic.read(0x01), 170);
    }

    #[test]
    fn bank_selection_masks_to_two_bits() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.set_bank(2);
        assert_eq!(vic.bank(), 2);
        vic.set_bank(5);
        assert_eq!(vic.bank(), 1);
    }

    #[test]
    fn sprite_renders_at_correct_position() {
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x15, 0x01);
        vic.write(0x00, 172);
        vic.write(0x01, 100);
        vic.write(0x27, 0x01);
        vic.write(0x18, 0x14);
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);
        vic.write(0x11, 0x1B);

        // Sprite Y = 100; the draw-stage sequencer draws it one line later in
        // this synthetic first-activation harness (line 101) — the +1 the
        // testbench validates as correct steady-state phase. X = 172 →
        // fb_x = 172 + 24 = 196.
        let target_line = 101u16;
        let target_cycle = 35u8;
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..cycles_to_target {
            tick_vic(&mut vic, &memory);
        }

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let idx = fb_y * FB_WIDTH as usize + 196;
        assert_eq!(vic.framebuffer()[idx], PALETTE[1]);
    }

    #[test]
    fn bitmap_base_selection() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x18, 0x14);
        assert_eq!(vic.bitmap_base(), 0x0000);
        vic.write(0x18, 0x1C);
        assert_eq!(vic.bitmap_base(), 0x2000);
    }

    #[test]
    fn collision_registers_clear_on_read() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.sprite_sprite_collision = 0x05;
        vic.sprite_bg_collision = 0x0A;
        assert_eq!(vic.read(0x1E), 0x05);
        assert_eq!(vic.read(0x1E), 0x00);
        assert_eq!(vic.read(0x1F), 0x0A);
        assert_eq!(vic.read(0x1F), 0x00);
    }

    #[test]
    fn collision_peek_does_not_clear() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.sprite_sprite_collision = 0x03;
        assert_eq!(vic.peek(0x1E), 0x03);
        assert_eq!(vic.peek(0x1E), 0x03);
        assert_eq!(vic.read(0x1E), 0x03);
        assert_eq!(vic.read(0x1E), 0x00);
    }

    /// The draw-stage sequencer detects sprite-sprite collisions: two
    /// fully-overlapping sprites set both collision bits in `$D01E`.
    #[test]
    fn sprite_sequencer_sets_sprite_sprite_collision() {
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x15, 0x03); // enable sprites 0, 1
        vic.write(0x00, 172);
        vic.write(0x01, 100); // sprite 0 at (172, 100)
        vic.write(0x02, 172);
        vic.write(0x03, 100); // sprite 1 at (172, 100) — fully overlapping
        vic.write(0x27, 0x01);
        vic.write(0x28, 0x02);
        vic.write(0x18, 0x14);
        vic.write(0x11, 0x1B);
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x07F9, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);
        // Render past the sprites' display line (sequencer places them one line
        // later than geometry in this synthetic harness).
        advance_to(&mut vic, &memory, 103, 0);
        assert_eq!(
            vic.peek(0x1E) & 0x03,
            0x03,
            "sequencer should flag both sprites collided"
        );
    }

    #[test]
    fn sprite_bg_collision_set_on_foreground_overlap() {
        let chargen = vec![0xFF; 4096];
        let colour_ram = {
            let mut v = vec![0u8; 1024];
            v[0] = 0x01;
            v
        };
        let mut memory = TestMemory::with_colour(&chargen, colour_ram);
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x15, 0x01);
        vic.write(0x11, 0x1B);
        vic.write(0x18, 0x14);
        vic.write(0x00, 24);
        vic.write(0x01, 51);
        vic.write(0x27, 0x01);
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);

        // Sprite Y = 51 draws at line 52 (sequencer first-activation +1); tick
        // past its pixels (X = 24 → fb_x 48 ≈ cycle 16) so the sprite-over-
        // foreground collision registers in `$D01F`.
        let target_line = 52u16;
        let target_cycle = 40u8;
        let total = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=total {
            tick_vic(&mut vic, &memory);
        }

        let collision = vic.read(0x1F);
        assert_ne!(collision & 0x01, 0x00);
    }

    /// Sprites overlapping in the deep horizontal border still collide.
    /// A sprite at X = 392 draws at framebuffer X 416 — cycle 62, which
    /// `render_pixels` treats as off-window (`LAST_VISIBLE_CYCLE = 62`).
    /// The draw stage shifts the pixels out there regardless, so the
    /// sprite-sprite collision must still register in `$D01E`. Hardware
    /// detects collisions wherever sprite data is shifted, not only on-screen.
    #[test]
    fn sprite_sprite_collision_registers_in_the_border() {
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x15, 0x03); // enable sprites 0, 1
        vic.write(0x00, 136); // sprite 0 low X
        vic.write(0x02, 136); // sprite 1 low X
        vic.write(0x10, 0x03); // $D010 high X bit for 0 and 1 → X = 392
        vic.write(0x01, 100);
        vic.write(0x03, 100); // both at Y = 100, fully overlapping
        vic.write(0x27, 0x01);
        vic.write(0x28, 0x02);
        vic.write(0x18, 0x14);
        vic.write(0x11, 0x1B);
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x07F9, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);
        advance_to(&mut vic, &memory, 105, 0);
        assert_eq!(
            vic.peek(0x1E) & 0x03,
            0x03,
            "sprites colliding in the border should still set $D01E",
        );
    }

    #[test]
    fn invalid_mode_renders_black() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x7B);
        vic.write(0x20, 0x06);
        vic.write(0x21, 0x01);

        let target_line = DISPLAY_START_LINE + 3;
        let target_cycle = DISPLAY_START_CYCLE + 5;
        let total = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=total {
            tick_vic(&mut vic, &memory);
        }

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x = (target_cycle - FIRST_VISIBLE_CYCLE) as usize * 8;
        assert_eq!(fb_pixel(&vic, fb_x, fb_y), PALETTE[0]);
    }

    #[test]
    fn ecm_text_selects_background() {
        let chargen = vec![0x00; 4096];
        let mut memory = TestMemory::new(&chargen);
        // Screen codes the streaming c-access reads from the matrix at $0400
        // ($D018=0x14, text_row 0). ECM uses bits 7:6 to pick the background.
        memory.ram_write(0x0400, 0x00);
        memory.ram_write(0x0401, 0x40);
        memory.ram_write(0x0402, 0x80);
        memory.ram_write(0x0403, 0xC0);
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x11, 0x5B);
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00);
        vic.write(0x22, 0x02);
        vic.write(0x23, 0x05);
        vic.write(0x24, 0x06);

        let target_line = DISPLAY_START_LINE + 3;
        let past_fetch = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + 16;
        for _ in 0..past_fetch {
            tick_vic(&mut vic, &memory);
        }

        tick_vic(&mut vic, &memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        assert_eq!(fb_pixel(&vic, fb_x0, fb_y), PALETTE[0]);

        tick_vic(&mut vic, &memory);
        assert_eq!(fb_pixel(&vic, fb_x0 + 8, fb_y), PALETTE[2]);

        tick_vic(&mut vic, &memory);
        assert_eq!(fb_pixel(&vic, fb_x0 + 16, fb_y), PALETTE[5]);

        tick_vic(&mut vic, &memory);
        assert_eq!(fb_pixel(&vic, fb_x0 + 24, fb_y), PALETTE[6]);
    }

    #[test]
    fn badline_ba_low_cycles_12_to_54() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        advance_to(&mut vic, &memory, 0x33, 0);

        for cycle in 0..CYCLES_PER_LINE {
            tick_vic(&mut vic, &memory);
            let expected = (12..=54).contains(&cycle);
            assert_eq!(vic.ba_low, expected);
        }
    }

    #[test]
    fn non_badline_does_not_assert_ba() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        advance_to(&mut vic, &memory, 0x34, 0);

        for _ in 0..CYCLES_PER_LINE {
            tick_vic(&mut vic, &memory);
            assert!(!vic.badline_ba_low());
        }
    }

    /// Seam 1 audit (`c64-architecture-review.md`): asserts the
    /// asymmetry between `ba_low` and `cpu_stalled` on a badline.
    /// `ba_low` covers cycles 12-54 (BA goes low 3 phi2 cycles before
    /// the VIC-II's badline DMA actually starts, so the NMOS 6510 has
    /// time to wind down outstanding bus operations). `cpu_stalled`
    /// covers only cycles 15-54 (the AEC-low window where the CPU is
    /// actually off the bus). The 3-cycle gap (12-14) is the canonical
    /// NMOS warm-up where writes complete and reads still stall via
    /// the RDY pin.
    #[test]
    fn badline_ba_low_leads_cpu_stalled_by_three_cycles() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        advance_to(&mut vic, &memory, 0x33, 0);

        for cycle in 0..CYCLES_PER_LINE {
            tick_vic(&mut vic, &memory);
            let expected_ba_low = (12..=54).contains(&cycle);
            let expected_cpu_stalled = (15..=54).contains(&cycle);
            assert_eq!(
                vic.ba_low, expected_ba_low,
                "cycle {cycle}: ba_low mismatch"
            );
            assert_eq!(
                vic.cpu_stalled, expected_cpu_stalled,
                "cycle {cycle}: cpu_stalled mismatch"
            );
            // Structural invariant: cpu_stalled → ba_low. The
            // converse is intentionally false during cycles 12-14.
            assert!(
                !vic.cpu_stalled || vic.ba_low,
                "cycle {cycle}: cpu_stalled set without ba_low — invariant broken"
            );
        }
    }

    /// Seam 1 audit: same asymmetry holds for sprite DMA. Each active
    /// sprite asserts BA for 5 cycles (55-59 for sprite 0, shifted by
    /// 2 per subsequent sprite) but the CPU is only fully stalled for
    /// the 2-cycle DMA fetch window (58-59 for sprite 0). The 3-cycle
    /// lead-in matches the badline pattern.
    #[test]
    fn sprite_ba_low_leads_cpu_stalled_by_three_cycles() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x15, 0x01); // sprite 0 enabled
        vic.write(0x01, 0); // sprite 0 Y position 0
        advance_to(&mut vic, &memory, 0, 55);

        // Step through cycle 55 onwards to observe BA + cpu_stalled.
        for cycle in 55..CYCLES_PER_LINE {
            tick_vic(&mut vic, &memory);
            let expected_ba_low = (55..=59).contains(&cycle);
            let expected_cpu_stalled = (58..=59).contains(&cycle);
            assert_eq!(
                vic.ba_low, expected_ba_low,
                "cycle {cycle}: sprite ba_low mismatch"
            );
            assert_eq!(
                vic.cpu_stalled, expected_cpu_stalled,
                "cycle {cycle}: sprite cpu_stalled mismatch"
            );
            assert!(
                !vic.cpu_stalled || vic.ba_low,
                "cycle {cycle}: cpu_stalled without ba_low — invariant broken"
            );
        }
    }

    /// Seam 1 audit: lock the full per-sprite cycle allocation table
    /// against Marko Mäkelä's "MOS 6567/6569 video controller" §3.8.
    /// Each of sprites 0-7 has its s-access pair at a deterministic
    /// pair of phi2 cycles, preceded by a 3-cycle BA warm-up:
    ///
    /// Sprite | BA low cycles | DMA cycles (cpu_stalled)
    /// 0      | 55..=59       | 58..=59
    /// 1      | 57..=61       | 60..=61
    /// 2      | 59..=63       | 62, 0   (cpu_stalled set even on
    /// 3      |  0..=4        | 1..=2    wrap; ba_low spans both
    /// 4      |  2..=6        | 3..=4    sides of the cycle-0 wrap)
    /// 5      |  4..=8        | 5..=6
    /// 6      |  6..=10       | 7..=8
    /// 7      |  8..=12       | 9..=10
    ///
    /// Verified by enabling each sprite in isolation (Y=0 so it's
    /// active on raster line 0) and walking from line 0 cycle 0
    /// through into line 1.
    #[test]
    fn each_sprite_steals_canonical_cycles() {
        // (sprite_index, expected (line, cycle) DMA pairs)
        const DMA_CYCLES: [(usize, &[(u16, u8)]); 8] = [
            (0, &[(0, 58), (0, 59)]),
            (1, &[(0, 60), (0, 61)]),
            (2, &[(0, 62), (1, 0)]),
            (3, &[(1, 1), (1, 2)]),
            (4, &[(1, 3), (1, 4)]),
            (5, &[(1, 5), (1, 6)]),
            (6, &[(1, 7), (1, 8)]),
            (7, &[(1, 9), (1, 10)]),
        ];
        for &(i, expected_dma) in &DMA_CYCLES {
            let (mut vic, memory) = make_vic_and_memory();
            vic.write(0x15, 1 << i); // only this sprite enabled
            // Y position 0 for whichever sprite we're testing.
            vic.write(0x01 + (i as u8) * 2, 0);

            // Walk from (0, 0). evaluate_sprite_dma fires at cycle 55
            // of line 0 (sprite_y=0, raster_line=0 → offset 0 < 21 →
            // ACTIVE). The DMA cycles fire at 58-59 of line 0 for
            // sprite 0, shifting through into line 1 for the rest.
            // We need to cover up to line 1 cycle 10 — one full
            // line + 11 cycles = CYCLES_PER_LINE + 11 ticks. Capture
            // the (line, cycle) currently being processed each tick.
            let mut observed_dma = Vec::new();
            for _ in 0..(u32::from(CYCLES_PER_LINE) + 11) {
                // Snapshot the cycle BEFORE the tick — tick increments
                // raster_cycle at the end, so vic.cpu_stalled set
                // during the tick corresponds to the pre-tick cycle.
                let pre_line = vic.raster_line;
                let pre_cycle = vic.raster_cycle;
                tick_vic(&mut vic, &memory);
                if vic.cpu_stalled {
                    observed_dma.push((pre_line, pre_cycle));
                }
            }
            let mut sorted_observed = observed_dma.clone();
            sorted_observed.sort_unstable();
            let mut sorted_expected: Vec<(u16, u8)> = expected_dma.to_vec();
            sorted_expected.sort_unstable();
            assert_eq!(
                sorted_observed, sorted_expected,
                "sprite {i}: cpu_stalled (line, cycle) pairs mismatch; got {observed_dma:?}, want {expected_dma:?}"
            );
        }
    }

    /// Seam 1 audit: when a sprite is disabled, its s-cycle pair must
    /// be free — no BA pull-down and no cpu_stalled assertion. The
    /// real silicon allocates the cycles based on the sprite enable
    /// register at the *end of the previous line*, and disabled
    /// sprites release their slot. This catches a regression where
    /// the schedule pre-reserves cycles regardless of enable.
    #[test]
    fn disabled_sprite_releases_its_cycles() {
        let (mut vic, memory) = make_vic_and_memory();
        // No sprites enabled.
        vic.write(0x15, 0x00);
        advance_to(&mut vic, &memory, LINES_PER_FRAME - 1, 0);
        for _ in 0..(u32::from(CYCLES_PER_LINE) + 13) {
            tick_vic(&mut vic, &memory);
            assert!(
                !vic.cpu_stalled,
                "line {} cycle {}: cpu_stalled fired with no sprites enabled",
                vic.raster_line, vic.raster_cycle
            );
            // Also no sprite BA — but badlines may still assert BA
            // on a separate condition. We're on line 0 / 1 with no
            // DEN set, so badlines aren't triggered.
            assert!(
                !vic.sprite_ba_low(),
                "line {} cycle {}: sprite_ba_low fired with no sprites enabled",
                vic.raster_line,
                vic.raster_cycle
            );
        }
    }

    #[test]
    fn sprite_ba_asserts_with_three_cycle_leadin() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x15, 0x01);
        vic.write(0x01, 0);
        advance_to(&mut vic, &memory, 0, 55);
        tick_vic(&mut vic, &memory);

        for cycle in 56..CYCLES_PER_LINE {
            let expected = (55..=59).contains(&cycle);
            assert_eq!(vic.sprite_ba_low(), expected);
            tick_vic(&mut vic, &memory);
        }
    }

    #[test]
    fn light_pen_latches_beam_position() {
        let (mut vic, memory) = make_vic_and_memory();
        for _ in 0..20 {
            tick_vic(&mut vic, &memory);
        }
        let cycle = vic.raster_cycle();
        let line = vic.raster_line();
        vic.trigger_light_pen();
        assert_eq!(vic.peek(0x14), line as u8);
        assert_eq!(vic.peek(0x13), (cycle as u16 * 4) as u8);
    }

    #[test]
    fn light_pen_latches_once_per_frame() {
        let (mut vic, memory) = make_vic_and_memory();
        while vic.raster_line() < 50 {
            tick_vic(&mut vic, &memory);
        }
        vic.trigger_light_pen();
        let first_lpy = vic.peek(0x14);

        for _ in 0..200 {
            tick_vic(&mut vic, &memory);
        }
        vic.trigger_light_pen();
        assert_eq!(vic.peek(0x14), first_lpy);
    }

    #[test]
    fn unmapped_registers_return_last_bus_data() {
        let (mut vic, memory) = make_vic_and_memory();
        for _ in 0..(CYCLES_PER_LINE as u32 * (DISPLAY_START_LINE as u32 + 2)) {
            tick_vic(&mut vic, &memory);
        }
        assert_eq!(vic.read(0x2F), vic.peek(0x2F));
        assert_eq!(vic.read(0x30), vic.read(0x2F));
        assert_eq!(vic.read(0x3F), vic.read(0x2F));
    }

    #[test]
    fn xscroll_zero_renders_cell_unchanged() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        vic.write(0x16, 0x08);
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00);

        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        vic.colour_row[0] = 0x01;
        tick_vic(&mut vic, &memory);

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        for px in 0..8 {
            assert_eq!(fb_pixel(&vic, fb_x0 + px, fb_y), PALETTE[1]);
        }
    }

    /// Advance by exactly N cycles (one tick per cycle).
    fn step_cycles(vic: &mut Vic, memory: &TestMemory, n: u32) {
        for _ in 0..n {
            tick_vic(vic, memory);
        }
    }

    /// Run until the next tick would execute (line, cycle). advance_to
    /// runs `line * cycles_per_line + cycle` ticks *from construction*,
    /// so call it only on a fresh VIC.
    #[test]
    fn vertical_ff_clears_on_first_display_line_with_den() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // RSEL=1, DEN=1, YSCROLL=3
        // advance_to(51, 0) leaves the next tick pointing at (51,0).
        advance_to(&mut vic, &memory, 51, 0);
        tick_vic(&mut vic, &memory); // executes cycle 0 of line 51
        assert!(!vic.border_vert_ff, "vert FF should clear at line 51");
    }

    #[test]
    fn vertical_ff_sets_at_last_display_line() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // RSEL=1
        advance_to(&mut vic, &memory, 251, 0);
        tick_vic(&mut vic, &memory); // executes cycle 0 of line 251
        assert!(vic.border_vert_ff, "vert FF should set at line 251");
    }

    #[test]
    fn main_ff_clears_at_left_edge_when_vert_ff_off() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // RSEL=1, DEN=1
        vic.write(0x16, 0x08); // CSEL=1
        advance_to(&mut vic, &memory, 100, 16);
        tick_vic(&mut vic, &memory);
        assert!(!vic.border_vert_ff);
        assert!(!vic.border_main_ff, "main FF should clear at left edge");
    }

    #[test]
    fn open_border_trick_rsel_bit_flip_suppresses_vertical_ff() {
        // Open-border sequence: hold RSEL=1 through line 247 (so the
        // RSEL=0 set-rule never fires), then flip to RSEL=0 before
        // line 251 (so the RSEL=1 set-rule doesn't fire either).
        // Result: vert FF stays clear past the normal close point.
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // RSEL=1, DEN=1
        // advance_to(248, 0) from a fresh VIC runs 248*63 = 15624 ticks
        // which covers lines 0-247 in full (including line 51 clear
        // and line 247 which doesn't fire the set-rule with RSEL=1).
        advance_to(&mut vic, &memory, 248, 0);
        assert!(!vic.border_vert_ff);
        // Flip RSEL to 0 before line 251.
        vic.write(0x11, 0x13);
        // Continue to line 252 (another 4 full lines = 4*63 = 252 ticks).
        step_cycles(&mut vic, &memory, 4 * u32::from(CYCLES_PER_LINE));
        assert!(
            !vic.border_vert_ff,
            "open-border trick should keep vert FF clear"
        );
    }

    #[test]
    fn naive_close_sets_vert_ff_at_line_247_rsel_zero() {
        // Control case: with RSEL=0 throughout, line 247 fires the
        // set-rule and the vert FF goes high as expected.
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x13); // RSEL=0, DEN=1
        advance_to(&mut vic, &memory, 248, 0);
        assert!(vic.border_vert_ff, "RSEL=0 + line 247 should set vert FF");
    }

    #[test]
    fn xscroll_four_shifts_cell_right() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        vic.write(0x16, 0x0C);
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00);

        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        vic.colour_row[0] = 0x01;
        tick_vic(&mut vic, &memory);

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        for px in 0..4 {
            assert_eq!(fb_pixel(&vic, fb_x0 + px, fb_y), PALETTE[0]);
        }
        for px in 4..8 {
            assert_eq!(fb_pixel(&vic, fb_x0 + px, fb_y), PALETTE[1]);
        }
    }

    // ----- Cov-5c wave 2: directed coverage tests -----

    #[test]
    fn vic_model_ntsc_timings() {
        assert_eq!(VicModel::Ntsc6567.lines_per_frame(), 263);
        assert_eq!(VicModel::Ntsc6567.cycles_per_line(), 65);
        assert_eq!(VicModel::Pal6569.lines_per_frame(), 312);
        assert_eq!(VicModel::Pal6569.cycles_per_line(), 63);
    }

    #[test]
    fn vic_model_default_is_pal() {
        assert_eq!(VicModel::default(), VicModel::Pal6569);
    }

    #[test]
    fn ntsc_sprite_schedule_matches_6567r8() {
        // VICE cycle_tab_ntsc SprPtr cycles (engine 0-based): sprites 0-3 in
        // the previous line's tail (59/61/63/0-wrap), 4-7 on the current line
        // (2/4/6/8); DMA/display checks shift +1 vs PAL.
        let t = VicModel::Ntsc6567.sprite_timing();
        assert_eq!(
            t.paccess,
            [
                (59, true),
                (61, true),
                (63, true),
                (0, true),
                (2, false),
                (4, false),
                (6, false),
                (8, false),
            ]
        );
        assert_eq!(t.chk_dma, [56, 57]);
        assert_eq!(t.chk_exp, 56);
        assert_eq!(t.chk_disp, 59);
    }

    #[test]
    fn ntsc_sprite_access_cycles_resolve_per_model() {
        let ntsc = Vic::new(VicModel::Ntsc6567);
        // Sprite 0 p-access at 59 (NTSC) not 58 (PAL); s-access the next cycle.
        assert_eq!(ntsc.sprite_paccess_cycle(59), Some((0, true)));
        assert_eq!(ntsc.sprite_saccess_cycle(60), Some(0));
        // Sprite 3 straddles the line boundary (p-access at 0, s-access at 1).
        assert_eq!(ntsc.sprite_paccess_cycle(0), Some((3, true)));
        assert_eq!(ntsc.sprite_saccess_cycle(1), Some(3));
        // Sprite 4 is first on the current line at cycle 2.
        assert_eq!(ntsc.sprite_paccess_cycle(2), Some((4, false)));
        // The PAL sprite-0 cycle (58) is not a p-access on NTSC.
        assert_eq!(ntsc.sprite_paccess_cycle(58), None);

        let pal = Vic::new(VicModel::Pal6569);
        assert_eq!(pal.sprite_paccess_cycle(58), Some((0, true)));
        assert_eq!(pal.sprite_paccess_cycle(59), None);
    }

    #[test]
    fn ntsc_r56a_timings_and_schedule() {
        assert_eq!(VicModel::Ntsc6567R56A.lines_per_frame(), 262);
        assert_eq!(VicModel::Ntsc6567R56A.cycles_per_line(), 64);
        let t = VicModel::Ntsc6567R56A.sprite_timing();
        // Between PAL and R8: sprites 0-2 at 59/61/63 (PAL 58/60/62), but
        // sprite 3 stays on the current line at cycle 1 (R8 wraps it to 0).
        assert_eq!(t.paccess[0], (59, true));
        assert_eq!(t.paccess[2], (63, true));
        assert_eq!(t.paccess[3], (1, false));
        assert_eq!(t.chk_dma, [56, 57]); // like R8
        assert_eq!(t.chk_disp, 58); // like PAL

        let vic = Vic::new(VicModel::Ntsc6567R56A);
        assert_eq!(vic.sprite_paccess_cycle(59), Some((0, true)));
        assert_eq!(vic.sprite_paccess_cycle(1), Some((3, false)));
        // Sprite 2's s-access wraps to engine cycle 0 (VICE cycle 64).
        assert_eq!(vic.sprite_saccess_cycle(0), Some(2));
    }

    #[test]
    fn ntsc_sprite_dma_steals_at_ntsc_cycles() {
        // Enable sprite 0 at a Y that is DMA-active on line 60, then confirm it
        // steals the bus on its NTSC p-access (59) + s-access (60), not the PAL
        // cycle 58.
        let (mut vic, memory) = make_vic_and_memory_model(VicModel::Ntsc6567);
        vic.write(0x15, 0x01); // enable sprite 0
        vic.write(0x01, 60); // sprite 0 Y = 60
        // Wind just past line 60 cycle 56 (NTSC ChkSprDma[0], processed at the
        // end of that cycle's tick) so DMA is evaluated.
        advance_until(&mut vic, &memory, 60, 57);
        assert!(vic.sprite_dma_active[0], "sprite 0 should be DMA-active");
        // Cycle 58 is a steal on PAL but not NTSC (checked before 59, forward).
        advance_until(&mut vic, &memory, 60, 58);
        assert!(
            !vic.is_sprite_dma_stealing(),
            "cyc 58 is not a steal on NTSC (it is on PAL)"
        );
        advance_until(&mut vic, &memory, 60, 59);
        assert!(
            vic.is_sprite_dma_stealing(),
            "steal on NTSC p-access cyc 59"
        );
    }

    #[test]
    fn ntsc_construction_uses_ntsc_visible_lines() {
        let vic = Vic::new(VicModel::Ntsc6567);
        // NTSC renders the full frame (0..263), like PAL — 263 lines.
        assert_eq!(vic.framebuffer_height(), 263);
    }

    #[test]
    fn default_constructs_pal_vic() {
        let vic: Vic = Vic::default();
        assert_eq!(vic.framebuffer_width(), FB_WIDTH);
        // PAL fb height: 312 - 0 = 312.
        assert_eq!(vic.framebuffer_height(), 312);
    }

    #[test]
    fn public_accessor_methods_round_trip() {
        let mut vic = Vic::new(VicModel::Pal6569);
        // Direct accessors initially zero.
        assert_eq!(vic.char_row(), 0);
        assert!(!vic.is_badline());
        assert!(!vic.ba_is_low());
        // registers() returns the raw register file pointer.
        let regs_snapshot = *vic.registers();
        assert_eq!(regs_snapshot.len(), 0x40);
        // irq_status getter/setter round-trip.
        vic.set_irq_status(0x0F);
        assert_eq!(vic.irq_status(), 0x0F);
        // set_registers should restore both raster_compare and irq_enable.
        let mut new_regs = [0u8; 0x40];
        new_regs[0x11] = 0x80; // raster compare bit 8 set
        new_regs[0x12] = 0x10;
        new_regs[0x1A] = 0x01;
        vic.set_registers(&new_regs);
        // peek($D011) reports raster bit 8 in bit 7; raster_line is 0 here.
        assert_eq!(vic.peek(0x11) & 0x80, 0x00);
        // The raw register bits read back via registers().
        assert_eq!(vic.registers()[0x11], 0x80);
        assert_eq!(vic.registers()[0x12], 0x10);
        // raster_compare = ($D011 bit 7) << 8 | $D012 = 0x110 = 272.
        // PAL has 312 lines, so 272 is reachable.
        vic.set_irq_status(0);
        let memory = TestMemory::new(&[0u8; 4096]);
        let mut hit = false;
        for _ in 0..(63u32 * 400) {
            tick_vic(&mut vic, &memory);
            if (vic.irq_status() & 0x01) != 0 {
                hit = true;
                break;
            }
        }
        assert!(hit, "raster IRQ never latched at compare 0x110");
        assert_eq!(vic.raster_line(), 0x110);
    }

    #[test]
    fn read_d011_returns_high_bit_of_raster_line() {
        let (mut vic, memory) = make_vic_and_memory();
        // Advance to a line >= 256. PAL has 312 lines; line 256 is reachable.
        while vic.raster_line() < 256 {
            tick_vic(&mut vic, &memory);
        }
        // Set CR1 lower bits (mode/yscroll) to a known non-zero pattern.
        vic.write(0x11, 0x1B);
        let v = vic.read(0x11);
        // Bit 7 should be set since raster_line >= 256.
        assert_eq!(v & 0x80, 0x80);
        // Lower bits preserved.
        assert_eq!(v & 0x7F, 0x1B);
    }

    #[test]
    fn read_d012_returns_low_byte_of_raster_line() {
        let (mut vic, memory) = make_vic_and_memory();
        // Advance to line 5.
        while vic.raster_line() < 5 {
            tick_vic(&mut vic, &memory);
        }
        assert_eq!(vic.read(0x12), 5);
    }

    #[test]
    fn read_d019_composite_irq_bit_and_unused_high_bits() {
        let mut vic = Vic::new(VicModel::Pal6569);
        // Force raster IRQ pending, with mask enabled.
        vic.set_irq_status(0x01);
        vic.write(0x1A, 0x01);
        let v = vic.read(0x19);
        // Bit 7 = composite, bits 6:4 = 1, bit 0 = pending raster IRQ.
        assert_eq!(v & 0x80, 0x80);
        assert_eq!(v & 0x70, 0x70);
        assert_eq!(v & 0x01, 0x01);
        // With no enable, bit 7 should clear but the latched bit stays.
        vic.write(0x1A, 0x00);
        let v2 = vic.read(0x19);
        assert_eq!(v2 & 0x80, 0x00);
        assert_eq!(v2 & 0x01, 0x01);
    }

    #[test]
    fn read_d01a_returns_irq_enable_with_high_nibble_set() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x1A, 0x05);
        assert_eq!(vic.read(0x1A), 0xF5);
    }

    #[test]
    fn write_d019_acknowledges_only_set_bits() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.set_irq_status(0x0F);
        vic.write(0x19, 0x05); // ack bits 0 and 2 only
        assert_eq!(vic.peek(0x19) & 0x0F, 0x0A);
    }

    #[test]
    fn peek_d011_d012_match_raster() {
        let (mut vic, memory) = make_vic_and_memory();
        while vic.raster_line() < 257 {
            tick_vic(&mut vic, &memory);
        }
        vic.write(0x11, 0x13);
        // peek($D011) should report raster bit 8 in bit 7 of the result.
        assert_eq!(vic.peek(0x11) & 0x80, 0x80);
        assert_eq!(vic.peek(0x12), (vic.raster_line() & 0xFF) as u8);
    }

    #[test]
    fn peek_d019_returns_irq_status_and_composite() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.set_irq_status(0x02);
        vic.write(0x1A, 0x02);
        let v = vic.peek(0x19);
        assert_eq!(v & 0x80, 0x80);
        assert_eq!(v & 0x02, 0x02);
        // Without enable, composite bit clears.
        vic.write(0x1A, 0x00);
        assert_eq!(vic.peek(0x19) & 0x80, 0x00);
    }

    #[test]
    fn peek_d01a_returns_raw_enable() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x1A, 0x09);
        assert_eq!(vic.peek(0x1A), 0x09);
    }

    #[test]
    fn peek_returns_register_for_other_addrs_and_last_bus_above_2e() {
        let (mut vic, memory) = make_vic_and_memory();
        for _ in 0..(CYCLES_PER_LINE as u32 * (DISPLAY_START_LINE as u32 + 2)) {
            tick_vic(&mut vic, &memory);
        }
        vic.write(0x20, 0x05);
        // peek of a colour reg returns the raw register without high-nibble mask.
        assert_eq!(vic.peek(0x20), 0x05);
        // peek($D02F..$D03F) returns last_bus_data.
        let lb = vic.peek(0x2F);
        assert_eq!(vic.peek(0x30), lb);
        assert_eq!(vic.peek(0x3F), lb);
    }

    #[test]
    fn ba_is_low_reflects_pin_state() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // DEN=1, RSEL=1, YSCROLL=3
        advance_to(&mut vic, &memory, 0x33, 0);
        // Run until we are inside the badline-stealing window.
        for _ in 0..20 {
            tick_vic(&mut vic, &memory);
        }
        assert_eq!(vic.ba_is_low(), vic.ba_low);
    }

    #[test]
    fn is_badline_accessor_reports_badline_state() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // DEN=1, YSCROLL=3
        // Walk until we see a badline.
        let mut saw_badline = false;
        for _ in 0..(CYCLES_PER_LINE as u32 * 100) {
            tick_vic(&mut vic, &memory);
            if vic.is_badline() {
                saw_badline = true;
                break;
            }
        }
        assert!(saw_badline);
    }

    #[test]
    fn hires_bitmap_renders_fg_and_bg_from_screen_byte() {
        // BMM=1, MCM=0 → render_hires_bitmap. Bitmap byte 0xF0 paints
        // 4 fg + 4 bg pixels. fg = upper nibble of screen byte, bg = lower.
        let chargen = vec![0u8; 4096];
        let mut memory = TestMemory::new(&chargen);
        let mut vic = Vic::new(VicModel::Pal6569);
        // BMM=1, DEN=1, RSEL=1, YSCROLL=3 → 0x3B
        vic.write(0x11, 0x3B);
        // CB=0 (bitmap at 0x0000), VM=0x14 → screen at 0x1000.
        vic.write(0x18, 0x14);
        // CSEL=1 so the main FF clears at cycle 16 (matches DISPLAY_START_CYCLE).
        vic.write(0x16, 0x08);
        let target_line = DISPLAY_START_LINE + 3;
        // bitmap_addr = bitmap_base + VC*8 + RC. VC=0 (column 0 of row 0),
        // RC=0 (badline row) → addr 0.
        memory.ram_write(0, 0xF0);
        // Screen base = (0x14 >> 4) * 0x400 = 0x400. Column 0 → addr 0x400.
        memory.ram_write(0x400, 0x52); // fg=palette[5], bg=palette[2]
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        tick_vic(&mut vic, &memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        // First 4 px = fg (bit set), last 4 px = bg (bit clear).
        for px in 0..4 {
            assert_eq!(fb_pixel(&vic, fb_x0 + px, fb_y), PALETTE[5]);
        }
        for px in 4..8 {
            assert_eq!(fb_pixel(&vic, fb_x0 + px, fb_y), PALETTE[2]);
        }
    }

    #[test]
    fn mcm_bitmap_renders_four_pixel_pairs() {
        // BMM=1, MCM=1. Pairs in bitmap byte select bg0 / c01 / c10 / c11.
        let chargen = vec![0u8; 4096];
        let colour_ram = {
            let mut v = vec![0u8; 1024];
            v[0] = 0x07; // colour_nybble for col 0 = 7 → c11
            v
        };
        let mut memory = TestMemory::with_colour(&chargen, colour_ram);
        let mut vic = Vic::new(VicModel::Pal6569);
        // BMM=1, MCM=1, DEN=1 → CR1=0x3B, CR2=0x18 (CSEL+MCM).
        vic.write(0x11, 0x3B);
        vic.write(0x16, 0x18);
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x02); // bg0 = 2
        // Bitmap at 0x0000, bytes=0b00_01_10_11 = 0x1B → pair colours (bg0, c01, c10, c11).
        let target_line = DISPLAY_START_LINE + 3;
        // VC=0, RC=0 on the badline row → bitmap addr 0.
        memory.ram_write(0, 0x1B);
        // Screen base = 0x400. Column 0 → addr 0x400.
        memory.ram_write(0x400, 0x46); // c01=palette[4], c10=palette[6]
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        tick_vic(&mut vic, &memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        // Pair 0 = bg0 (palette[2]).
        assert_eq!(fb_pixel(&vic, fb_x0, fb_y), PALETTE[2]);
        assert_eq!(fb_pixel(&vic, fb_x0 + 1, fb_y), PALETTE[2]);
        // Pair 1 = c01 (palette[4]).
        assert_eq!(fb_pixel(&vic, fb_x0 + 2, fb_y), PALETTE[4]);
        assert_eq!(fb_pixel(&vic, fb_x0 + 3, fb_y), PALETTE[4]);
        // Pair 2 = c10 (palette[6]).
        assert_eq!(fb_pixel(&vic, fb_x0 + 4, fb_y), PALETTE[6]);
        assert_eq!(fb_pixel(&vic, fb_x0 + 5, fb_y), PALETTE[6]);
        // Pair 3 = c11 (palette[7]).
        assert_eq!(fb_pixel(&vic, fb_x0 + 6, fb_y), PALETTE[7]);
        assert_eq!(fb_pixel(&vic, fb_x0 + 7, fb_y), PALETTE[7]);
    }

    #[test]
    fn mcm_text_with_high_colour_bit_renders_multicolour() {
        // MCM=1 + colour bit 3 set → render_mcm_text full path.
        let chargen = vec![0xB4u8; 4096]; // 0b10110100 → pairs 10 11 01 00
        let colour_ram = {
            let mut v = vec![0u8; 1024];
            v[0] = 0x0F; // colour bit 3 set → MCM enable, fg = palette[7]
            v
        };
        let memory = TestMemory::with_colour(&chargen, colour_ram);
        let mut vic = Vic::new(VicModel::Pal6569);
        // MCM=1 in CR2 (bit 4), CSEL=1.
        vic.write(0x11, 0x1B);
        vic.write(0x16, 0x18);
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x01); // bg0
        vic.write(0x22, 0x02); // bg1
        vic.write(0x23, 0x03); // bg2
        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        tick_vic(&mut vic, &memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        // Pair 0 = 10 → bg2 (palette[3]).
        assert_eq!(fb_pixel(&vic, fb_x0, fb_y), PALETTE[3]);
        // Pair 1 = 11 → fg (palette[7]).
        assert_eq!(fb_pixel(&vic, fb_x0 + 2, fb_y), PALETTE[7]);
        // Pair 2 = 01 → bg1 (palette[2]).
        assert_eq!(fb_pixel(&vic, fb_x0 + 4, fb_y), PALETTE[2]);
        // Pair 3 = 00 → bg0 (palette[1]).
        assert_eq!(fb_pixel(&vic, fb_x0 + 6, fb_y), PALETTE[1]);
    }

    #[test]
    fn mcm_text_with_low_colour_bit_falls_back_to_standard_text() {
        // colour bit 3 clear → render_mcm_text returns render_standard_text result.
        let chargen = vec![0xFFu8; 4096];
        let colour_ram = {
            let mut v = vec![0u8; 1024];
            v[0] = 0x07; // colour bit 3 clear
            v
        };
        let memory = TestMemory::with_colour(&chargen, colour_ram);
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x11, 0x1B);
        vic.write(0x16, 0x18); // MCM=1 in register
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x02);
        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        tick_vic(&mut vic, &memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        // 0xFF chargen → all foreground = palette[7].
        for px in 0..8 {
            assert_eq!(fb_pixel(&vic, fb_x0 + px, fb_y), PALETTE[7]);
        }
    }

    #[test]
    fn sprite_x_high_bit_places_sprite_past_256() {
        // Set sprite 0 X to 256 + 4 = 260 via $D010 bit 0.
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x15, 0x01);
        vic.write(0x00, 4); // low byte
        vic.write(0x10, 0x01); // high bit for sprite 0
        vic.write(0x01, 100);
        vic.write(0x27, 0x05); // colour palette[5]
        vic.write(0x18, 0x14);
        vic.write(0x11, 0x1B);
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);
        // sprite_fb_x = 260 + 24 = 284. Need a cycle that paints this fb_x.
        // fb_x = (cycle - 10) * 8. Cycle 45 paints fb_x 280..288 → covers 284.
        // Sprite Y = 100 draws at line 101 (sequencer first-activation +1).
        let target_line = 101u16;
        let target_cycle = 45u8;
        let total = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=total {
            tick_vic(&mut vic, &memory);
        }
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let idx = fb_y * FB_WIDTH as usize + 284;
        assert_eq!(vic.framebuffer()[idx], PALETTE[5]);
    }

    #[test]
    fn sprite_multicolor_renders_three_colours() {
        // MCM sprite: pairs select transparent / mc0 / sprite_col / mc1.
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x15, 0x01); // enable sprite 0
        vic.write(0x1C, 0x01); // sprite 0 multicolor
        vic.write(0x00, 172);
        vic.write(0x01, 100);
        vic.write(0x25, 0x04); // mc0
        vic.write(0x26, 0x06); // mc1
        vic.write(0x27, 0x05); // sprite 0 colour
        vic.write(0x18, 0x14);
        vic.write(0x11, 0x1B);
        memory.ram_write(0x07F8, 0x80);
        // Pattern: 0b00_01_10_11 = 0x1B → pairs (transparent, mc0, sprite, mc1).
        memory.ram_write(0x2000, 0x1B);
        memory.ram_write(0x2001, 0x1B);
        memory.ram_write(0x2002, 0x1B);
        // sprite_fb_x = 172 + 24 = 196. Cycle 34 → fb_x 192..200. Cycle 35 → 200..208.
        // Sprite Y = 100 draws at line 101 (sequencer first-activation +1).
        let target_line = 101u16;
        let target_cycle = 34u8;
        let total = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=total {
            tick_vic(&mut vic, &memory);
        }
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let row_off = fb_y * FB_WIDTH as usize;
        // pair 0 (px 196,197): transparent → background (border colour 14 default).
        // pair 1 (px 198,199): mc0 → palette[4].
        assert_eq!(vic.framebuffer()[row_off + 198], PALETTE[4]);
        assert_eq!(vic.framebuffer()[row_off + 199], PALETTE[4]);
        // Tick another cycle to cover 200..208.
        tick_vic(&mut vic, &memory);
        // pair 2 (200,201): sprite_col palette[5].
        assert_eq!(vic.framebuffer()[row_off + 200], PALETTE[5]);
        assert_eq!(vic.framebuffer()[row_off + 201], PALETTE[5]);
        // pair 3 (202,203): mc1 palette[6].
        assert_eq!(vic.framebuffer()[row_off + 202], PALETTE[6]);
        assert_eq!(vic.framebuffer()[row_off + 203], PALETTE[6]);
    }

    #[test]
    fn sprite_priority_below_foreground_is_hidden() {
        // Sprite 0 with priority bit set ($D01B) should be hidden behind FG.
        let chargen = vec![0xFFu8; 4096];
        let colour_ram = {
            let mut v = vec![0u8; 1024];
            v[0] = 0x01; // FG bits in column 0
            v
        };
        let mut memory = TestMemory::with_colour(&chargen, colour_ram);
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x15, 0x01); // enable sprite 0
        vic.write(0x1B, 0x01); // sprite 0 BEHIND fg
        vic.write(0x11, 0x1B);
        vic.write(0x16, 0x08); // CSEL=1 so border off at left edge (cycle 16)
        vic.write(0x18, 0x14);
        vic.write(0x00, 24); // sprite_fb_x = 48 → cycle 16 paints fb_x 48..56.
        vic.write(0x01, 51);
        vic.write(0x27, 0x05);
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);
        let target_line = 51u16;
        let target_cycle = DISPLAY_START_CYCLE; // cycle 16 → fb_x 48..56
        let total = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=total {
            tick_vic(&mut vic, &memory);
        }
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let row_off = fb_y * FB_WIDTH as usize;
        // FG is solid (0xFF chargen): sprite hidden, pixels remain text fg.
        assert_eq!(vic.framebuffer()[row_off + 48], PALETTE[1]);
    }

    #[test]
    fn sprite_dma_y_expand_extends_height() {
        // y_expand sprite (height 42) should still be DMA-active at offset 25.
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x15, 0x01); // enable sprite 0
        vic.write(0x17, 0x01); // y-expand sprite 0
        vic.write(0x01, 0); // sprite Y = 0
        // Walk to line 25, cycle 55: evaluate_sprite_dma runs at cycle 55.
        advance_to(&mut vic, &memory, 25, 55);
        tick_vic(&mut vic, &memory);
        // Without y-expand, height = 21; so offset 25 would NOT be DMA-active.
        // With y-expand, height = 42; so offset 25 IS DMA-active.
        assert!(vic.sprite_dma_active[0]);
    }

    #[test]
    fn sprite_y_expand_fetch_uses_halved_data_line() {
        // Build a sprite where data byte 0 differs from byte at line 1.
        // With y-expand, lines 0 and 1 should both fetch data_line=0.
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x15, 0x01); // enable sprite 0
        vic.write(0x17, 0x01); // y-expand
        vic.write(0x00, 24); // X
        vic.write(0x01, 100); // Y
        vic.write(0x18, 0x14);
        vic.write(0x11, 0x1B);
        vic.write(0x27, 0x05);
        memory.ram_write(0x07F8, 0x80);
        // data_line 0 (target_line - 100 == 1, /2 = 0) → bytes at 0x2000..2002.
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);
        let target_line = 101u16; // line 1 of sprite
        let target_cycle = DISPLAY_START_CYCLE; // fb_x = 48..56, sprite_fb_x = 48
        let total = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=total {
            tick_vic(&mut vic, &memory);
        }
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let row_off = fb_y * FB_WIDTH as usize;
        // Sprite present at fb_x 48 since data byte 0 = 0xFF.
        assert_eq!(vic.framebuffer()[row_off + 48], PALETTE[5]);
    }

    #[test]
    fn sprite_off_screen_below_visible_lines_is_skipped() {
        // The fetch_sprite_if_scheduled path bails when target_line is
        // outside visible lines. PAL: visible 0..312, so wrap test.
        // NTSC: visible 14..258. Use an NTSC vic and advance past line 257.
        let chargen = vec![0u8; 4096];
        let memory = TestMemory::new(&chargen);
        let mut vic = Vic::new(VicModel::Ntsc6567);
        vic.write(0x15, 0x01); // enable sprite 0
        vic.write(0x01, 0); // sprite Y = 0
        // NTSC: 263 lines, last_visible = 258. Walk to a point where target
        // would be 259..262 (sprite p-access fetches at cycle 1/3/5/7/9).
        // PAL has 312 visible lines so use NTSC where last_visible=258 to
        // exercise the early-return branch.
        for _ in 0..(263u32 * 65) {
            tick_vic(&mut vic, &memory);
        }
        // Simply ran one full NTSC frame; if no panic, the off-screen branch
        // was hit.
    }

    #[test]
    fn xscroll_zero_no_carry_load() {
        // Cover the xscroll==0 path explicitly with foreground bits to set
        // fg_mask via cell.fg_mask copy.
        let chargen = vec![0x80u8; 4096];
        let colour_ram = {
            let mut v = vec![0u8; 1024];
            v[0] = 0x07;
            v
        };
        let memory = TestMemory::with_colour(&chargen, colour_ram);
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x11, 0x1B);
        vic.write(0x16, 0x08); // CSEL=1, xscroll=0
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x02);
        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        tick_vic(&mut vic, &memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        // High bit only set → first pixel = fg, rest = bg.
        assert_eq!(fb_pixel(&vic, fb_x0, fb_y), PALETTE[7]);
        assert_eq!(fb_pixel(&vic, fb_x0 + 1, fb_y), PALETTE[2]);
    }

    #[test]
    fn frame_complete_clears_lp_and_den_latches() {
        let (mut vic, memory) = make_vic_and_memory();
        // Trigger light pen.
        for _ in 0..40 {
            tick_vic(&mut vic, &memory);
        }
        vic.trigger_light_pen();
        assert!(vic.lp_triggered);
        // Run a full frame.
        let total = u32::from(LINES_PER_FRAME) * u32::from(CYCLES_PER_LINE);
        for _ in 0..total {
            tick_vic(&mut vic, &memory);
        }
        assert!(!vic.lp_triggered);
        assert!(!vic.den_latch);
    }

    #[test]
    fn raster_irq_compare_works_for_high_bit_lines() {
        // raster_compare > 255 — sets bit 8 via $D011 write.
        let (mut vic, memory) = make_vic_and_memory();
        // line 257 → low byte = 1, high bit = 1.
        vic.write(0x11, 0x80);
        vic.write(0x12, 1);
        vic.write(0x1A, 0x01);
        // Tick until raster matches.
        loop {
            tick_vic(&mut vic, &memory);
            if vic.irq_active() {
                break;
            }
            assert!(vic.raster_line() <= 258, "didn't fire");
        }
        assert_eq!(vic.raster_line(), 257);
    }
}
