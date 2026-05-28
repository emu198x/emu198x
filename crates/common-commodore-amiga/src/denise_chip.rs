//! `DeniseChip` trait — the surface the board-level `Denise` wrapper
//! requires from any concrete Denise variant.
//!
//! Currently implemented by `commodore_denise_ocs::DeniseOcs` and
//! `commodore_denise_ecs::DeniseEcs`. Future variants (AGA's Lisa,
//! CD32's AKIKO-aware variant) impl the same trait and the wrapper
//! works against them unchanged.
//!
//! See Seam 1 of
//! `knowledge/decisions/amiga-full-family-architecture-review.md`.

use commodore_denise_ecs::DeniseEcs;
use commodore_denise_ocs::{DeniseOcs, DeniseOutputPixelDebug};

/// Methods + field accessors the board-level `Denise` wrapper calls
/// on the concrete chip. Both OCS Denise and ECS Super Denise impl
/// this; the wrapper is generic over the impl.
///
/// All `set_*` mutators correspond to fields the wrapper assigns to
/// from `Agnus`-side state each line (BPLCON0, LOF, interlace).
pub trait DeniseChip:
    Clone + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static
{
    fn new() -> Self;

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
    fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16;

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

    // ── Field mutators used by the wrapper ──
    fn set_bplcon0(&mut self, v: u16);
    fn set_interlace_active(&mut self, v: bool);
    fn set_lof(&mut self, v: bool);
}

impl DeniseChip for DeniseOcs {
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
}

// DeniseEcs wraps DeniseOcs via Deref<Target = DeniseOcs>; field
// accesses and method calls dispatch through to the inner OCS core.
impl DeniseChip for DeniseEcs {
    fn new() -> Self {
        DeniseEcs::new()
    }
    fn write_word(&mut self, offset: u16, val: u16) {
        // Route through the ECS-aware write so $106 (BPLCON3) lands
        // on the ECS storage; other offsets fall through to OCS.
        DeniseEcs::write_word(self, offset, val);
    }
    fn load_bitplane(&mut self, idx: usize, val: u16) {
        self.as_inner_mut().load_bitplane(idx, val);
    }
    fn push_bpl_fifo(&mut self, idx: usize, val: u16) {
        self.as_inner_mut().push_bpl_fifo(idx, val);
    }
    fn queue_shift_load_from_bpl1dat(&mut self) {
        self.as_inner_mut().queue_shift_load_from_bpl1dat();
    }
    fn write_sprite_pos(&mut self, sprite: usize, val: u16) {
        self.as_inner_mut().write_sprite_pos(sprite, val);
    }
    fn write_sprite_ctl(&mut self, sprite: usize, val: u16) {
        self.as_inner_mut().write_sprite_ctl(sprite, val);
    }
    fn write_sprite_data(&mut self, sprite: usize, val: u16) {
        self.as_inner_mut().write_sprite_data(sprite, val);
    }
    fn write_sprite_datb(&mut self, sprite: usize, val: u16) {
        self.as_inner_mut().write_sprite_datb(sprite, val);
    }
    fn begin_beam_line(&mut self) {
        self.as_inner_mut().begin_beam_line();
    }
    fn output_pixel_with_beam_and_playfield_gate(
        &mut self,
        x: u32,
        y: u32,
        beam_x: u32,
        beam_y: u32,
        playfield_visible_gate: bool,
    ) -> DeniseOutputPixelDebug {
        self.as_inner_mut()
            .output_pixel_with_beam_and_playfield_gate(x, y, beam_x, beam_y, playfield_visible_gate)
    }
    fn resolve_color_rgb12(&mut self, color_idx: u8) -> u16 {
        self.as_inner_mut().resolve_color_rgb12(color_idx)
    }
    fn palette(&self) -> &[u16; 32] {
        &self.as_inner().palette
    }
    fn interlace_active(&self) -> bool {
        self.as_inner().interlace_active
    }
    fn lof(&self) -> bool {
        self.as_inner().lof
    }
    fn bplcon0(&self) -> u16 {
        self.as_inner().bplcon0
    }
    fn set_bplcon0(&mut self, v: u16) {
        self.as_inner_mut().bplcon0 = v;
    }
    fn set_interlace_active(&mut self, v: bool) {
        self.as_inner_mut().interlace_active = v;
    }
    fn set_lof(&mut self, v: bool) {
        self.as_inner_mut().lof = v;
    }
    fn deniseid(&self) -> u16 {
        DeniseEcs::deniseid(self)
    }
}
