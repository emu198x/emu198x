//! [`DeniseChip`] for ECS Super Denise.

use emu198x_commodore_denise_ocs::{DeniseChip, DeniseOutputPixelDebug};

use crate::DeniseEcs;

// DeniseEcs wraps DeniseOcs via Deref<Target = DeniseOcs>; field
// accesses and method calls dispatch through to the inner OCS core.
impl DeniseChip for DeniseEcs {
    const SUPPORTS_DIWHIGH: bool = true;
    const RESETS_COUNTER_ON_EQUALISATION: bool = true;
    fn new() -> Self {
        DeniseEcs::new()
    }
    fn write_word(&mut self, offset: u16, val: u16) {
        // Route through the ECS-aware write so $106 (BPLCON3) lands
        // on the ECS storage; other offsets fall through to OCS.
        DeniseEcs::write_word(self, offset, val);
    }
    fn write_color_before_output_tick(&mut self, offset: u16, value: u16) -> bool {
        // ECS applies COLOR on the early RGA phase. A further board tick
        // made each independently traced Copper colour edge one lores late.
        DeniseEcs::write_word(self, offset, value);
        true
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

    fn enable_sprites_from_bpl1dat(&mut self) {
        self.as_inner_mut().enable_sprites_from_bpl1dat();
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
        self.as_inner_mut().output_pixel_with_beam_sprite_coords(
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
        DeniseEcs::resolve_color_rgb12(self, color_idx)
    }
    fn advance_register_output_pipeline(&mut self) {
        self.advance_output_selector_pipeline();
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
    fn read_clxdat(&mut self) -> u16 {
        self.as_inner_mut().read_clxdat()
    }
    fn peek_clxdat(&self) -> u16 {
        self.as_inner().peek_clxdat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_commodore_denise_ocs::{DeniseOcs, HorizontalDiwComparatorPhase};

    #[test]
    fn ocs_and_ecs_use_current_tick_horizontal_diw_matches() {
        let ocs = DeniseOcs::new();
        let ecs = DeniseEcs::new();

        assert_eq!(
            ocs.horizontal_diw_comparator_phase(),
            HorizontalDiwComparatorPhase::BeforeOutput,
        );
        assert_eq!(
            ecs.horizontal_diw_comparator_phase(),
            HorizontalDiwComparatorPhase::BeforeOutput,
        );
    }

    #[test]
    fn ecs_live_output_adapter_honors_killehb() {
        let mut denise = DeniseEcs::new();
        denise.as_inner_mut().set_palette(5, 0x0ACE);
        denise.as_inner_mut().bplcon0 = 0x6000; // 6 planes, EHB
        DeniseChip::write_word(&mut denise, 0x0104, 0x0200); // BPLCON2 KILLEHB

        assert_eq!(
            DeniseChip::resolve_output_color_argb(&mut denise, 0x25, 0x25, false),
            0xFFAA_CCEE,
        );
    }
}
