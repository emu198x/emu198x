//! OCS Denise facade — type alias over the generic board-level
//! wrapper in `common_commodore_amiga::denise`. Concrete chip is
//! [`commodore_denise_ocs::DeniseOcs`].
//!
//! All shared helpers (`ddf_window`, `diw_vertical_window`)
//! re-exported from the substrate for callers that import via
//! `crate::denise::*`. The per-CCK DMA slot arbitration lives in Agnus
//! (`current_slot` / `cck_bus_plan`, #30) and is consumed from the
//! shared `AmigaDriver` body, not here.

use commodore_denise_ocs::DeniseOcs;
pub use common_commodore_amiga::denise::{FB_HEIGHT, FB_WIDTH};

pub type Denise = common_commodore_amiga::denise::Denise<DeniseOcs>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory;

    const DMACON_BPL: u16 = 0x0300;

    #[test]
    fn hires_two_plane_line_fetches_forty_words_per_plane() {
        let mut denise = Denise::new();
        let mut agnus = commodore_agnus_ocs::Agnus::new();
        let memory = Memory::new(vec![0; 256 * 1024]);

        agnus.vpos = 0x002C;
        agnus.dmacon = DMACON_BPL;
        agnus.bplcon0 = 0xA200; // HIRES + BPU=2 + COLOR
        agnus.ddfstrt = 0x003C;
        agnus.ddfstop = 0x00D0;
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.bpl1mod = 0;
        agnus.bpl2mod = 0;
        agnus.bpl_pt[0] = 0x0000_0100;
        agnus.bpl_pt[1] = 0x0000_0200;

        for hpos in 0..=0x00E2 {
            agnus.hpos = hpos;
            denise.tick(0, agnus.vpos, hpos, agnus.dmacon, &mut agnus, &memory);
        }

        assert_eq!(
            agnus.bpl_pt[0],
            0x0000_0100 + 80,
            "hires BPL1 should fetch 40 words across the line",
        );
        assert_eq!(
            agnus.bpl_pt[1],
            0x0000_0200 + 80,
            "hires BPL2 should fetch 40 words across the line",
        );
    }

    #[test]
    fn hires_line_drains_shift_register_before_next_line() {
        let mut denise = Denise::new();
        let mut agnus = commodore_agnus_ocs::Agnus::new();
        let memory = Memory::new(vec![0; 256 * 1024]);

        agnus.vpos = 0x002C;
        agnus.dmacon = DMACON_BPL;
        agnus.bplcon0 = 0x9200; // HIRES + BPU=1 + COLOR
        agnus.ddfstrt = 0x003C;
        agnus.ddfstop = 0x00D0;
        agnus.diwstrt = 0x2C81;
        agnus.diwstop = 0x2CC1;
        agnus.bpl1mod = 0;
        agnus.bpl2mod = 0;
        agnus.bpl_pt[0] = 0x0000_0100;

        for hpos in 0..=0x00E2 {
            agnus.hpos = hpos;
            denise.tick(0, agnus.vpos, hpos, agnus.dmacon, &mut agnus, &memory);
            denise.tick(1, agnus.vpos, hpos, agnus.dmacon, &mut agnus, &memory);
        }

        assert_eq!(
            denise.ocs.shift_count, 0,
            "the wrapper must keep advancing Denise after DDF closes so late-line pixels do not leak into the next line",
        );
    }
}
