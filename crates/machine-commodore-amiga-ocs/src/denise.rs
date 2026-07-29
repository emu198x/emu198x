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
pub use common_commodore_amiga::denise::{BitplaneDmaFetch, FB_HEIGHT, FB_WIDTH};

pub type Denise = common_commodore_amiga::denise::Denise<DeniseOcs>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory;

    const DMACON_BPL: u16 = 0x0300;

    fn observe_ddf_start(agnus: &mut commodore_agnus_ocs::Agnus) {
        if agnus.agnus_id < 0x2000 && !agnus.vertical_diw_active() {
            let diwstrt = agnus.diwstrt;
            let diwstop = agnus.diwstop;
            assert!(agnus.vpos <= 0x00FF);
            agnus.write_diwstop(diwstop);
            agnus.write_diwstrt((agnus.vpos << 8) | (diwstrt & 0x00FF));
            agnus.write_diwstrt(diwstrt);
            for _ in 0..8 {
                agnus.tick_cck();
            }
        }
        let mask = if agnus.agnus_id >= 0x2000 {
            0x00FE
        } else {
            0x00FC
        };
        let start = agnus.ddfstrt & mask;
        assert!(start > 0, "test helper requires a non-zero DDFSTRT");
        agnus.hpos = start - 1;
        agnus.tick_cck();
        assert_eq!(agnus.ddf_start_match(), Some(start));
    }

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
        observe_ddf_start(&mut agnus);

        loop {
            let plan = agnus.cck_bus_plan();
            let width = agnus.bpl_fetch_width();
            let vertical_diw_active = agnus.vertical_diw_active();
            let line_ccks = agnus.current_line_ccks();
            denise.tick(
                0,
                plan.bitplane_dma_fetch_plane.map(|plane| BitplaneDmaFetch {
                    plane,
                    width_words: width,
                }),
                vertical_diw_active,
                &mut agnus,
                &memory,
                line_ccks,
            );
            if agnus.hpos == 0x00E2 {
                break;
            }
            agnus.tick_cck();
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
        observe_ddf_start(&mut agnus);

        loop {
            let plan = agnus.cck_bus_plan();
            let width = agnus.bpl_fetch_width();
            let vertical_diw_active = agnus.vertical_diw_active();
            let line_ccks = agnus.current_line_ccks();
            denise.tick(
                0,
                plan.bitplane_dma_fetch_plane.map(|plane| BitplaneDmaFetch {
                    plane,
                    width_words: width,
                }),
                vertical_diw_active,
                &mut agnus,
                &memory,
                line_ccks,
            );
            denise.tick(1, None, vertical_diw_active, &mut agnus, &memory, line_ccks);
            if agnus.hpos == 0x00E2 {
                break;
            }
            agnus.tick_cck();
        }

        assert_eq!(
            denise.ocs.shift_count, 0,
            "the wrapper must keep advancing Denise after DDF closes so late-line pixels do not leak into the next line",
        );
    }

    #[test]
    fn start_after_stop_does_not_reconstruct_an_early_vertical_window() {
        let mut agnus = commodore_agnus_ocs::Agnus::new();

        agnus.vpos = 0x0030;
        agnus.dmacon = DMACON_BPL;
        agnus.bplcon0 = 0x1000;
        agnus.ddfstrt = 0x0038;
        agnus.ddfstop = 0x00D0;
        agnus.diwstrt = 0xF081;
        agnus.diwstop = 0xE0C1;
        assert!(
            !agnus.vertical_diw_active(),
            "position alone cannot manufacture an open comparator latch",
        );

        agnus.hpos = 0x0037;
        agnus.tick_cck();
        assert_eq!(
            agnus.ddf_start_match(),
            None,
            "the early line cannot admit bitplane DMA before the later VSTART event",
        );
    }
}
