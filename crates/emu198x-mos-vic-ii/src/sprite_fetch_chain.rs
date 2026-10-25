//! VIC-II sprite fetch chain — the per-sprite MC / MCBASE / expansion-flip-flop
//! vertical counter, a faithful port of VICE's `vicii-cycle.c` sprite logic.
//!
//! The engine's geometry sprite fetch uses a fixed height (`line < 21/42`),
//! which cannot reproduce sprite crunch: clearing `$D017` (Y-expand) at the
//! wrong cycle corrupts the data counter so the sprite's DMA never terminates
//! cleanly, and it keeps displaying *past* its nominal 21 rows. That is exactly
//! the `spritecrunch` divergence the oracle localised (lines 86-125).
//!
//! This module is the addressing/height half of the real sprite hardware. It
//! produces, per line, the display bit set and the MC data offset the draw-stage
//! `SpriteSequencer` consumes. Ported from VICE `check_sprite_dma`,
//! `check_exp`, `check_sprite_display`, `sprite_mcbase_update`
//! (`vicii-cycle.c:62-129`) and the `$D017` crunch (`vicii-mem.c:182`).
//!
//! Landed **isolated and unit-tested first** (S1 did the same for the draw
//! sequencer): the crunch *mechanism* is proven here before it is wired into the
//! fetch, which must reconcile VICE's on-the-line timing with the engine's
//! fetch-ahead p/s-access (the wiring increment, S4b). See the plan
//! `docs/plans/2026-06-30-c64-vic-ii-vc-vcbase-rc-rewrite.md`
//! (Increment 5 § sprite sequencer, S4).

use serde::{Deserialize, Serialize};

/// Per-sprite MC/MCBASE/exp-flop state. The bit masks are shared across the
/// eight sprites; MC and MCBASE are per sprite.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct SpriteFetchChain {
    /// Data counter (0-63): byte offset into the sprite's 63-byte data block.
    mc: [u8; 8],
    /// Data counter base (0-63): latched from MC each line while the expansion
    /// flop is set; reaching 63 ends the sprite's DMA.
    mcbase: [u8; 8],
    /// Expansion flip-flops: for Y-expanded sprites this toggles per line so
    /// MCBASE only advances every second line (double height).
    exp_flop: u8,
    /// Sprite DMA active (`sprite_dma`): set when Y matches, cleared when MCBASE
    /// reaches 63.
    dma: u8,
    /// Display bits (`sprite_display_bits`): what the draw sequencer renders.
    display: u8,
}

impl SpriteFetchChain {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Current display-bit set — the sprites the sequencer should render.
    pub(crate) fn display_bits(&self) -> u8 {
        self.display
    }

    /// A sprite's current MC data offset (the byte index its next s-access reads).
    pub(crate) fn mc(&self, sprite: usize) -> u8 {
        self.mc[sprite]
    }

    /// Whether a sprite's DMA is active this line.
    pub(crate) fn dma_active(&self, sprite: usize) -> bool {
        self.dma & (1 << sprite) != 0
    }

    /// MCBASE update (VICE `sprite_mcbase_update`, cyc 16): latch MCBASE from MC
    /// while the expansion flop is set; a sprite whose MCBASE reaches 63 has
    /// streamed all 21 rows, so its DMA ends.
    pub(crate) fn update_mcbase(&mut self) {
        for i in 0..8 {
            if self.exp_flop & (1 << i) != 0 {
                self.mcbase[i] = self.mc[i];
                if self.mcbase[i] == 63 {
                    self.dma &= !(1 << i);
                }
            }
        }
    }

    /// Check sprite DMA (VICE `check_sprite_dma`, cyc 55 & 56): start DMA when a
    /// sprite is enabled and its Y matches the low byte of the raster line.
    /// `y[i]` is `$D001+2i`. MCBASE clears and the expansion flop arms.
    pub(crate) fn check_dma(&mut self, enable: u8, y: [u8; 8], raster_low: u8) {
        for (i, &yi) in y.iter().enumerate() {
            let m = 1u8 << i;
            if enable & m != 0 && yi == raster_low && self.dma & m == 0 {
                self.dma |= m;
                self.mcbase[i] = 0;
                self.exp_flop |= m;
            }
        }
    }

    /// Check expansion (VICE `check_exp`, cyc 56): toggle the expansion flop for
    /// Y-expanded active sprites. `y_expand` is `$D017`.
    pub(crate) fn check_exp(&mut self, y_expand: u8) {
        self.exp_flop ^= y_expand & self.dma;
    }

    /// Check sprite display (VICE `check_sprite_display`, cyc 58): reset MC from
    /// MCBASE for the coming fetch, set the display bit while DMA + Y match, and
    /// clear it once DMA has ended.
    pub(crate) fn check_display(&mut self, enable: u8, y: [u8; 8], raster_low: u8) {
        for (i, &yi) in y.iter().enumerate() {
            let m = 1u8 << i;
            self.mc[i] = self.mcbase[i];
            if self.dma & m != 0 {
                if enable & m != 0 && yi == raster_low {
                    self.display |= m;
                }
            } else {
                self.display &= !m;
            }
        }
    }

    /// One s-access data byte (VICE `sprite_dma_cycle_0/2`): the caller reads
    /// `(pointer << 6) + mc(sprite)`, then advances MC.
    pub(crate) fn advance_mc(&mut self, sprite: usize) {
        self.mc[sprite] = (self.mc[sprite] + 1) & 0x3F;
    }

    /// Sprite crunch (VICE `vicii-mem.c` d017_store): applied on a `$D017` write.
    /// For each sprite whose Y-expand bit is being *cleared* while its expansion
    /// flop is low, set the flop; and when the write lands on the crunch cycle,
    /// corrupt MC with the documented bit-merge — the effect that makes the
    /// sprite over-run its 21 rows.
    pub(crate) fn write_d017(&mut self, new_value: u8, crunch_cycle: bool) {
        for i in 0..8 {
            let m = 1u8 << i;
            if new_value & m == 0 && self.exp_flop & m == 0 {
                if crunch_cycle {
                    let mc = self.mc[i];
                    let mcbase = self.mcbase[i];
                    self.mc[i] = (0x2a & (mcbase & mc)) | (0x15 & (mcbase | mc));
                }
                self.exp_flop |= m;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the chain through one display line for a single sprite, in VICE's
    /// per-cycle order (mcbase-update, dma-check, exp-check, display-check, then
    /// the three s-access byte advances). Returns whether the sprite displays.
    fn step_line(chain: &mut SpriteFetchChain, enable: u8, y_expand: u8, y: u8, raster_low: u8) {
        let ys = [y; 8];
        chain.update_mcbase();
        chain.check_dma(enable, ys, raster_low);
        chain.check_exp(y_expand);
        chain.check_display(enable, ys, raster_low);
        if chain.dma_active(0) {
            // Three data-byte s-accesses advance MC by 3.
            chain.advance_mc(0);
            chain.advance_mc(0);
            chain.advance_mc(0);
        }
    }

    /// A plain (non-expanded) sprite displays for exactly 21 lines, then its
    /// MCBASE reaches 63 and DMA ends.
    #[test]
    fn plain_sprite_displays_21_lines() {
        let mut chain = SpriteFetchChain::new();
        let y = 50u8;
        let mut display_lines = 0;
        for line in 50..90u8 {
            step_line(&mut chain, 0x01, 0x00, y, line);
            if chain.display_bits() & 0x01 != 0 {
                display_lines += 1;
            }
        }
        assert_eq!(display_lines, 21, "plain sprite should display 21 lines");
    }

    /// A Y-expanded sprite displays for 42 lines: the expansion flop halves the
    /// MCBASE advance, so it takes twice as long to reach 63.
    #[test]
    fn expanded_sprite_displays_42_lines() {
        let mut chain = SpriteFetchChain::new();
        let y = 50u8;
        let mut display_lines = 0;
        for line in 50..120u8 {
            step_line(&mut chain, 0x01, 0x01, y, line);
            if chain.display_bits() & 0x01 != 0 {
                display_lines += 1;
            }
        }
        assert_eq!(display_lines, 42, "expanded sprite should display 42 lines");
    }

    /// Sprite crunch: clearing `$D017` on the crunch cycle after the expansion
    /// flop has been toggled corrupts MC so MCBASE never lands cleanly on 63 at
    /// row 21 — the sprite over-runs its nominal height (keeps displaying past
    /// where a plain sprite would have ended).
    #[test]
    fn crunch_makes_sprite_overrun() {
        let mut chain = SpriteFetchChain::new();
        let y = 50u8;
        // Run as a Y-expanded sprite for a few lines so the flop + MCBASE are
        // mid-stream, then crunch on the crunch cycle at line 60.
        let mut plain = SpriteFetchChain::new();
        let mut crunched_lines = 0;
        let mut plain_lines = 0;
        for line in 50..160u8 {
            step_line(&mut chain, 0x01, 0x01, y, line);
            step_line(&mut plain, 0x01, 0x01, y, line);
            if line == 60 {
                // Clear expand at the crunch cycle on the crunched chain only.
                chain.write_d017(0x00, true);
            }
            if chain.display_bits() & 0x01 != 0 {
                crunched_lines += 1;
            }
            if plain.display_bits() & 0x01 != 0 {
                plain_lines += 1;
            }
        }
        assert_eq!(plain_lines, 42, "reference expanded sprite is 42 lines");
        assert_ne!(
            crunched_lines, plain_lines,
            "crunch should change the displayed height"
        );
    }
}
