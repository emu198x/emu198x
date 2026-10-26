//! The surface a board-level Denise wrapper drives: [`DeniseChip`], and
//! the signal types that cross it.
//!
//! OCS Denise, ECS Super Denise and AGA Lisa each implement
//! [`DeniseChip`]; a board written against the trait works with any of
//! them. The trait lives here, with the OCS chip every later variant
//! builds on, so a variant crate depends only on the chips below it and a
//! board crate implements nothing for a chip it does not own.
//!
//! See Seam 1 of
//! `knowledge/decisions/amiga-full-family-architecture-review.md`.

use crate::{DeniseOcs, DeniseOutputPixelDebug};

/// A timing strobe Agnus delivers to Denise over the RGA bus, as the
/// chip receives it: `STREQU`, `STRVBL` or `STRHOR`.
///
/// Boards translate their own strobe type into this one when they retire
/// a strobe into the chip through [`DeniseChip::retire_timing_strobe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TimingStrobe {
    /// `STREQU`: a line in the vertical equalisation interval.
    Equalisation,
    /// `STRVBL`: a line in vertical blank.
    VerticalBlank,
    /// `STRHOR`: an ordinary line.
    Horizontal,
}

/// External horizontal-blank levels at the four 35 ns positions in one
/// lores output tick. Hires callers supply levels held over each pair.
///
/// The machine layer advances the chipset-specific comparator latches and
/// supplies their resulting levels. The renderer therefore has no register,
/// comparator, or selector policy of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HorizontalBlanking {
    output_samples: [bool; 4],
}

impl HorizontalBlanking {
    /// Disable external programmable horizontal blanking.
    #[must_use]
    pub const fn disabled() -> Self {
        Self::from_level(false)
    }

    /// Apply one horizontal-blank level to both samples in this output tick.
    #[must_use]
    pub const fn from_level(active: bool) -> Self {
        Self {
            output_samples: [active; 4],
        }
    }

    /// Apply independently composed levels to the two output samples in this
    /// tick. Lisa uses this form because one fine comparator can split the
    /// pair.
    #[must_use]
    pub const fn from_output_samples(output_samples: [bool; 2]) -> Self {
        Self {
            output_samples: [
                output_samples[0],
                output_samples[0],
                output_samples[1],
                output_samples[1],
            ],
        }
    }

    /// Independent blank levels at each 35 ns boundary in a lores period.
    #[must_use]
    pub const fn from_superhires_samples(output_samples: [bool; 4]) -> Self {
        Self { output_samples }
    }

    /// Whether output sample `subpixel` of a lores period is blanked, when
    /// the period holds `samples_per_lores` retained samples (2 for OCS and
    /// ECS, 4 at Lisa's 35 ns sample clock).
    #[must_use]
    pub fn contains_output_sample(self, subpixel: u8, samples_per_lores: u32) -> bool {
        self.output_samples[usize::from(subpixel) * 4 / samples_per_lores as usize]
    }
}

/// Timing of a horizontal display-window comparator match relative to the
/// output tick at the matching beam position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalDiwComparatorPhase {
    /// The comparator result controls the output at the matching position.
    BeforeOutput,
    /// The comparator result controls the output after the matching position.
    AfterOutput,
}

/// Methods + field accessors the board-level `Denise` wrapper calls
/// on the concrete chip. Both OCS Denise and ECS Super Denise impl
/// this; the wrapper is generic over the impl.
///
/// All `set_*` mutators correspond to fields the wrapper assigns to
/// from `Agnus`-side state each line (BPLCON0, LOF, interlace).
pub trait DeniseChip:
    Clone + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static
{
    /// Retained output samples per lores pixel, independent of the live mode.
    const OUTPUT_SAMPLES_PER_LORES: u32 = 2;

    /// OCS free-runs through STREQU; enhanced Denise/Lisa reset on all strobes.
    const RESETS_COUNTER_ON_EQUALISATION: bool = false;

    /// Installed display chip decodes the horizontal DIWHIGH extension.
    const SUPPORTS_DIWHIGH: bool = false;

    fn new() -> Self;

    /// Consume a strobe retiring from the board's existing normal RGA stage.
    /// Lisa retains the vertical-blank transition independently of its counter.
    fn retire_timing_strobe(&mut self, _strobe: TimingStrobe) {}

    /// Select when horizontal DIW comparator matches reach pixel output.
    /// OCS Denise and ECS Super Denise use the match for the current tick;
    /// All installed variants compose the level after the equality comparison.
    fn horizontal_diw_comparator_phase(&self) -> HorizontalDiwComparatorPhase {
        HorizontalDiwComparatorPhase::BeforeOutput
    }

    // ── Register / data writes the wrapper forwards ──
    fn write_word(&mut self, offset: u16, val: u16);
    fn load_bitplane(&mut self, idx: usize, val: u16);
    /// Queue an extra word from a wide (FMODE > 0) bitplane DMA fetch.
    /// The first word of each access goes through `load_bitplane`; the
    /// 32-/64-bit fetch's remaining 1 or 3 words queue here and reload
    /// the plane's shift register as it drains. The board wrapper only
    /// calls this when `Agnus::bpl_fetch_width() > 1`, so OCS / ECS
    /// (always width 1) never touch the FIFO.
    fn push_bpl_fifo(&mut self, idx: usize, val: u16);
    fn queue_shift_load_from_bpl1dat(&mut self);
    /// BPL1DAT's early sprite-enable stage, before bitplane parallel copy.
    fn enable_sprites_from_bpl1dat(&mut self);
    fn write_sprite_pos(&mut self, sprite: usize, val: u16);
    fn write_sprite_ctl(&mut self, sprite: usize, val: u16);
    fn write_sprite_data(&mut self, sprite: usize, val: u16);
    fn write_sprite_datb(&mut self, sprite: usize, val: u16);

    // ── Per-line state ──
    fn begin_beam_line(&mut self);

    // ── Pixel emission ──
    fn output_pixel_with_beam_and_playfield_gate(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug;
    /// As `output_pixel_with_beam_and_playfield_gate`, but with the sprite
    /// comparator driven by *absolute* beam coordinates (`spr_beam_x` in
    /// lores units, `spr_beam_y` the raster line) distinct from the
    /// bitplane pipeline's scroll-relative `beam_x`/`beam_y`. The board
    /// uses this so DMA-driven sprites position by their SPRxPOS/CTL
    /// absolute HSTART/VSTART. gap #162.
    #[allow(clippy::too_many_arguments)]
    fn output_pixel_with_beam_sprite_coords(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        spr_beam_x: u32,
        spr_beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug;
    /// Window levels at the four 35 ns positions in the existing output tick.
    /// OCS/ECS compare coarse positions; Lisa overrides for fractional edges.
    #[allow(clippy::too_many_arguments)]
    fn output_pixel_with_sample_gates(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        spr_beam_x: u32,
        spr_beam_y: u32,
        gates: [bool; 4],
    ) -> DeniseOutputPixelDebug {
        assert!(gates.iter().all(|&gate| gate == gates[0]));
        self.output_pixel_with_beam_sprite_coords(
            x, y, beam_x, beam_y, spr_beam_x, spr_beam_y, gates[0],
        )
    }
    fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16;

    /// Resolve a playfield colour index to a final ARGB8888 pixel. The
    /// default is the 12-bit palette path upscaled (OCS / ECS — 4 bits
    /// per channel); AGA Lisa overrides it to resolve normal indexed
    /// modes through its 24-bit palette for true 8-bit-per-channel
    /// colour (#93).
    fn resolve_color_argb(&mut self, color_idx: u8) -> u32 {
        rgb12_to_argb(self.resolve_color_rgb12(color_idx))
    }

    /// Resolve the final composited sample while preserving both the
    /// underlying playfield and the result of sprite priority. HAM and EHB
    /// decode the playfield stream before a winning sprite replaces the
    /// visible colour with a direct palette selection.
    fn resolve_output_color_argb(
        &mut self,
        playfield_color_idx: u8,
        output_color_idx: u8,
        is_sprite: bool,
    ) -> u32 {
        let playfield = self.resolve_color_argb(playfield_color_idx);
        if is_sprite {
            let rgb12 = self.palette()[usize::from(output_color_idx) & 0x1F];
            rgb12_to_argb(rgb12)
        } else {
            playfield
        }
    }

    /// Resolve one retained output sample. `sample` is its position within
    /// the lores period; concrete chips preserve their colour propagation
    /// time when the framebuffer uses a faster sample clock.
    fn resolve_output_sample_argb(
        &mut self,
        playfield_color_idx: u8,
        output_color_idx: u8,
        is_sprite: bool,
        sample: u8,
    ) -> u32 {
        let _ = sample;
        self.resolve_output_color_argb(playfield_color_idx, output_color_idx, is_sprite)
    }

    /// Mask the final output in the border without stopping colour or sprite
    /// advancement. Lisa overrides this for ECSENA-gated BRDRBLNK.
    fn border_blanking_for_output(&self, playfield_visible_gate: bool) -> bool {
        let _ = playfield_visible_gate;
        false
    }

    /// Advance colour-output timing without resolving a visible pixel.
    ///
    /// OCS and ECS have no deferred colour-output state. AGA Lisa overrides
    /// this hook because a `COLORxx` write reaches the output one hires pixel
    /// after the register write, including while the board viewport is not
    /// being recorded.
    fn advance_color_output_samples(&mut self, samples: u8) {
        let _ = samples;
    }

    /// Handle a pre-output COLOR write in the concrete chip. OCS makes it
    /// visible immediately; Lisa retains its hires palette delay. Returns true
    /// when handled, or false to use the retained board write stage.
    fn write_color_before_output_tick(&mut self, offset: u16, value: u16) -> bool {
        let _ = (offset, value);
        false
    }

    /// Retire one early COLOR-write output stage after a master/4 tick.
    fn advance_early_color_output_pipeline(&mut self) {}

    /// Advance display-side custom-register propagation by one master/4 tick.
    ///
    /// OCS has no additional selector or comparator stages. ECS Super Denise
    /// and AGA Lisa override this hook for their normal-stage output controls.
    fn advance_register_output_pipeline(&mut self) {}

    // ── Field accessors used by the wrapper ──
    fn palette(&self) -> &[u16; 32];
    fn interlace_active(&self) -> bool;
    fn lof(&self) -> bool;
    fn bplcon0(&self) -> u16;

    /// DENISEID register value ($DFF07C). OCS Denise 8362 has no
    /// version register and returns open-bus `$FFFF`; ECS Super
    /// Denise 8373 returns `$FFFC`; AGA Lisa returns `$FFF8`. KS 3.x
    /// reads this to discriminate chipset generation during init.
    fn deniseid(&self) -> u16;

    /// Read and clear the raw 15-bit sprite/playfield collision latch.
    /// The board register surface adds CLXDAT's fixed high bit. Collision
    /// state lives in the shared OCS core for every chipset.
    fn read_clxdat(&mut self) -> u16;
    /// Inspect the raw collision latch without clearing it.
    fn peek_clxdat(&self) -> u16;

    // ── Field mutators used by the wrapper ──
    fn set_bplcon0(&mut self, v: u16);
    /// Present the board's raw BPLCON0 mirror to the chip input. Enhanced
    /// output stages can retain a distinct copy currently driving pixels.
    fn sync_bplcon0_input(&mut self, value: u16) {
        self.set_bplcon0(value);
    }
    fn set_interlace_active(&mut self, v: bool);
    fn set_lof(&mut self, v: bool);
}

impl DeniseChip for DeniseOcs {
    fn write_color_before_output_tick(&mut self, offset: u16, value: u16) -> bool {
        self.write_word(offset, value);
        true
    }
    fn new() -> Self {
        DeniseOcs::new()
    }
    fn write_word(&mut self, offset: u16, val: u16) {
        self.write_word(offset, val);
    }
    fn load_bitplane(&mut self, idx: usize, val: u16) {
        self.load_bitplane(idx, val);
    }
    fn push_bpl_fifo(&mut self, idx: usize, val: u16) {
        self.push_bpl_fifo(idx, val);
    }
    fn queue_shift_load_from_bpl1dat(&mut self) {
        self.queue_shift_load_from_bpl1dat();
    }

    fn enable_sprites_from_bpl1dat(&mut self) {
        self.enable_sprites_from_bpl1dat();
    }
    fn write_sprite_pos(&mut self, sprite: usize, val: u16) {
        self.write_sprite_pos(sprite, val);
    }
    fn write_sprite_ctl(&mut self, sprite: usize, val: u16) {
        self.write_sprite_ctl(sprite, val);
    }
    fn write_sprite_data(&mut self, sprite: usize, val: u16) {
        self.write_sprite_data(sprite, val);
    }
    fn write_sprite_datb(&mut self, sprite: usize, val: u16) {
        self.write_sprite_datb(sprite, val);
    }
    fn begin_beam_line(&mut self) {
        self.begin_beam_line();
    }
    fn output_pixel_with_beam_and_playfield_gate(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        self.output_pixel_with_beam_and_playfield_gate(x, y, beam_x, beam_y, playfield_visible_gate)
    }
    fn output_pixel_with_beam_sprite_coords(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        spr_beam_x: u32,
        spr_beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        self.output_pixel_with_beam_sprite_coords(
            x,
            y,
            beam_x,
            beam_y,
            spr_beam_x,
            spr_beam_y,
            playfield_visible_gate,
        )
    }
    fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16 {
        self.resolve_color_rgb12(color_idx)
    }
    fn palette(&self) -> &[u16; 32] {
        &self.palette
    }
    fn interlace_active(&self) -> bool {
        self.interlace_active
    }
    fn lof(&self) -> bool {
        self.lof
    }
    fn bplcon0(&self) -> u16 {
        self.bplcon0
    }
    fn set_bplcon0(&mut self, v: u16) {
        self.bplcon0 = v;
    }
    fn set_interlace_active(&mut self, v: bool) {
        self.interlace_active = v;
    }
    fn set_lof(&mut self, v: bool) {
        self.lof = v;
    }
    fn deniseid(&self) -> u16 {
        // OCS Denise 8362 has no version register — open-bus read.
        0xFFFF
    }
    fn read_clxdat(&mut self) -> u16 {
        self.read_clxdat()
    }
    fn peek_clxdat(&self) -> u16 {
        self.peek_clxdat()
    }
}

fn rgb12_to_argb(c12: u16) -> u32 {
    let r = ((c12 >> 8) & 0xF) as u32;
    let g = ((c12 >> 4) & 0xF) as u32;
    let b = (c12 & 0xF) as u32;
    let r8 = (r << 4) | r;
    let g8 = (g << 4) | g;
    let b8 = (b << 4) | b;
    0xFF00_0000 | (r8 << 16) | (g8 << 8) | b8
}
