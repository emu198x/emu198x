//! VIC-II draw-stage sprite sequencer — a faithful port of VICE's
//! `vicii-draw-cycle.c` shift-register sprite pipeline.
//!
//! The engine's existing `overlay_sprites` draws each sprite directly from its
//! fetched data at its X position. That is correct for ordinary sprites
//! (`spritedma` scores 99.78 % against VICE) but cannot reproduce the pixel-
//! timing edges — `spritecrunch`, `spritefetchbug`, `sb_sprite_fetch`, and the
//! sprite-in-border stripes — because those turn on the real per-pixel shift
//! register and its expansion / multicolour flip-flops.
//!
//! This module models that hardware. Each VIC cycle draws 8 pixels; per pixel a
//! sprite is *triggered* when the beam reaches its X position (loading the 24
//! data bits into a shift register), then *drawn* by shifting one bit (hires)
//! or two (multicolour) out of the top of the register, with the X-expansion
//! flip-flop halving the shift rate. Ported from VICE `draw_sprites`,
//! `trigger_sprites`, `draw_sprites8` (`vicii-draw-cycle.c:304-533`).
//!
//! It is **not yet wired into `Vic`** — the first increment of the sequencer
//! port lands it isolated and unit-tested against the pixel output the current
//! renderer produces, before anything switches over. See the plan
//! `docs/plans/2026-06-30-c64-vic-ii-vc-vcbase-rc-rewrite.md`
//! (Increment 5 § sprite sequencer).

/// One sprite pixel the sequencer emitted: which sprite, and its multicolour
/// selector (1 → `$D025`, 2 → the sprite's own `$D027+i`, 3 → `$D026`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpritePixel {
    pub sprite: u8,
    /// Multicolour pixel selector, 1..=3 (0 is transparent and never emitted).
    pub selector: u8,
}

/// Per-sprite draw-stage state. All eight sprites share the bit-mask registers;
/// the shift register and pixel latch are per sprite.
#[derive(Clone, Debug)]
pub(crate) struct SpriteSequencer {
    /// 24-bit data shift register (`sbuf_reg`), MSB first.
    shift_reg: [u32; 8],
    /// Current 2-bit output latch (`sbuf_pixel_reg`), 0..=3.
    pixel_reg: [u8; 8],
    /// Pipelined sprite X positions (`sprite_x_pipe`), in framebuffer pixels.
    x_pipe: [i32; 8],
    /// Sprites currently shifting out (`sprite_active_bits`).
    active: u8,
    /// Sprites whose display bit is set, waiting to trigger (`sprite_pending_bits`).
    pending: u8,
    /// Sprites stalled by their own DMA cycle (`sprite_halt_bits`).
    halt: u8,
    /// X-expansion flip-flops (`sbuf_expx_flops`): when clear, the shift is
    /// skipped this pixel (stretching the sprite horizontally).
    expx_flops: u8,
    /// Multicolour flip-flops (`sbuf_mc_flops`): gate the 2-bit fetch.
    mc_flops: u8,
    /// Latched multicolour-enable bits (`sprite_mc_bits`, `$D01C`).
    mc_bits: u8,
}

impl Default for SpriteSequencer {
    fn default() -> Self {
        Self::new()
    }
}

impl SpriteSequencer {
    pub(crate) fn new() -> Self {
        Self {
            shift_reg: [0; 8],
            pixel_reg: [0; 8],
            x_pipe: [0; 8],
            active: 0,
            pending: 0,
            halt: 0,
            expx_flops: 0,
            mc_flops: 0,
            mc_bits: 0,
        }
    }

    /// Reset the sequencer for a new display line and load every sprite's data.
    ///
    /// This is the **per-line** driving model used while the sequencer is
    /// proven against the geometry renderer (S2): the engine already fetches a
    /// whole line of sprite data before the visible region, so at the first
    /// visible cycle we load all eight shift registers, mark the active sprites
    /// pending, and shift across the line. A later increment (S4/S5) replaces
    /// this with VICE's continuous cross-line pipeline (per-cycle `load_data` at
    /// the s-access) needed for the sprite-fetch edge cases.
    ///
    /// `data[i]` is the three fetched bytes; `active_mask` bit `i` is the
    /// display bit; `x_fb[i]` is the sprite's framebuffer X; `mc_bits` is `$D01C`.
    pub(crate) fn begin_line(
        &mut self,
        data: &[[u8; 3]; 8],
        active_mask: u8,
        x_fb: [i32; 8],
        mc_bits: u8,
    ) {
        self.active = 0;
        self.pending = active_mask;
        self.halt = 0;
        self.expx_flops = 0;
        self.mc_flops = 0;
        self.pixel_reg = [0; 8];
        self.mc_bits = mc_bits;
        self.x_pipe = x_fb;
        for (reg, bytes) in self.shift_reg.iter_mut().zip(data.iter()) {
            *reg = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        }
    }

    /// Load a sprite's 24 data bits into its shift register (VICE
    /// `update_sprite_data`, fired at the sprite's DMA1/DMA2 cycle). `data` is
    /// the three fetched bytes packed MSB-first: `byte0 << 16 | byte1 << 8 | byte2`.
    pub(crate) fn load_data(&mut self, sprite: usize, data: u32) {
        self.shift_reg[sprite] = data & 0x00FF_FFFF;
    }

    /// Latch the display bits into the pending set (VICE sets
    /// `sprite_pending_bits = vicii.sprite_display_bits` at the display-check
    /// cycle). A pending sprite triggers when the beam reaches its X.
    pub(crate) fn set_pending(&mut self, display_bits: u8) {
        self.pending = display_bits;
    }

    /// Publish this cycle's pipelined X positions (VICE `update_sprite_xpos`,
    /// end of `draw_sprites8`). Positions are in framebuffer-pixel space.
    pub(crate) fn set_x_positions(&mut self, x: [i32; 8]) {
        self.x_pipe = x;
    }

    /// Latch `$D01C` multicolour-enable (VICE `update_sprite_mc_bits_6569`,
    /// pixel 7 of the cycle for the 6569). Clearing a bit resets that sprite's
    /// MC flip-flop.
    pub(crate) fn set_mc_bits(&mut self, mc_bits: u8) {
        let toggled = mc_bits ^ self.mc_bits;
        self.mc_flops &= !toggled;
        self.mc_bits = mc_bits;
    }

    /// Mark a sprite's DMA-halt (VICE `sprite_halt_bits |= dma_cycle_0` at
    /// pixel 3): while halted the shift register does not advance.
    pub(crate) fn set_halt(&mut self, sprite: usize) {
        self.halt |= 1 << sprite;
    }

    /// Release a sprite's DMA-halt (VICE `sprite_halt_bits &= ~dma_cycle_2`).
    pub(crate) fn clear_halt(&mut self, sprite: usize) {
        self.halt &= !(1 << sprite);
    }

    /// Deactivate a sprite mid-cycle (VICE `sprite_active_bits &= ~dma_cycle_2`
    /// at pixel 2): its own DMA cycle interrupts an in-progress shift-out.
    pub(crate) fn clear_active(&mut self, sprite: usize) {
        self.active &= !(1 << sprite);
    }

    /// Trigger any pending sprite whose X matches this beam pixel (VICE
    /// `trigger_sprites`): arm its expansion + multicolour flip-flops and mark
    /// it active so `draw_pixel` starts shifting it out.
    fn trigger(&mut self, xpos: i32) {
        if self.pending == 0 {
            return;
        }
        for s in 0..8 {
            let m = 1u8 << s;
            if self.pending & m != 0
                && self.active & m == 0
                && self.halt & m == 0
                && xpos == self.x_pipe[s]
            {
                self.expx_flops |= m;
                self.mc_flops |= m;
                self.active |= m;
            }
        }
    }

    /// Draw one beam pixel: trigger at `xpos`, then shift/emit every active
    /// sprite and return the highest-priority (lowest-numbered) sprite pixel,
    /// or `None` if the beam is transparent here. `expx_bits` is `$D01D`.
    ///
    /// Faithful to VICE `draw_sprites` (the per-sprite `for s = 7..0` loop) —
    /// pixel extraction, MC/expansion flip-flop handling, and shift order match.
    pub(crate) fn draw_pixel(&mut self, xpos: i32, expx_bits: u8) -> Option<SpritePixel> {
        self.trigger(xpos);
        if self.active == 0 {
            return None;
        }

        let mut winner: Option<SpritePixel> = None;
        for s in (0..8).rev() {
            let m = 1u8 << s;
            if self.active & m == 0 {
                continue;
            }

            if self.shift_reg[s] == 0 && self.pixel_reg[s] == 0 {
                // Nothing left to shift — the sprite finished this line.
                self.active &= !m;
                continue;
            }

            if self.halt & m == 0 {
                if self.expx_flops & m != 0 {
                    if self.mc_bits & m != 0 {
                        if self.mc_flops & m != 0 {
                            // Multicolour: latch two bits.
                            self.pixel_reg[s] = ((self.shift_reg[s] >> 22) & 0x03) as u8;
                        }
                        self.mc_flops ^= m;
                    } else {
                        // Hires: latch one bit, mapped to 0 or 2.
                        self.pixel_reg[s] = (((self.shift_reg[s] >> 23) & 0x01) << 1) as u8;
                    }
                }

                if self.expx_flops & m != 0 {
                    self.shift_reg[s] <<= 1;
                }
                if expx_bits & m != 0 {
                    self.expx_flops ^= m;
                } else {
                    self.expx_flops |= m;
                }
            }

            if self.pixel_reg[s] != 0 {
                // Lower sprite numbers have priority; the reverse loop means the
                // last writer (sprite 0) wins.
                winner = Some(SpritePixel {
                    sprite: s as u8,
                    selector: self.pixel_reg[s],
                });
            }
        }
        winner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single hires sprite shifts its 24 data bits out, one per pixel,
    /// starting at its X position — reproducing the current renderer's span.
    #[test]
    fn hires_sprite_shifts_24_pixels_from_x() {
        let mut seq = SpriteSequencer::new();
        // Sprite 0 at framebuffer X = 100, data = 0xC00000 (top two bits set:
        // pixels 0 and 1 are foreground, the rest background).
        let x0 = 100;
        seq.load_data(0, 0xC0_0000);
        seq.set_x_positions([x0, 0, 0, 0, 0, 0, 0, 0]);
        seq.set_pending(0x01);
        seq.set_mc_bits(0x00);

        // Before X: transparent.
        assert_eq!(seq.draw_pixel(x0 - 1, 0x00), None);

        // Pixels 0 and 1 are set (selector 2 = sprite's own colour in hires).
        let p0 = seq.draw_pixel(x0, 0x00);
        assert_eq!(
            p0,
            Some(SpritePixel {
                sprite: 0,
                selector: 2
            })
        );
        let p1 = seq.draw_pixel(x0 + 1, 0x00);
        assert_eq!(
            p1,
            Some(SpritePixel {
                sprite: 0,
                selector: 2
            })
        );

        // Remaining 22 pixels are background (data bits 0) → transparent.
        for dx in 2..24 {
            assert_eq!(
                seq.draw_pixel(x0 + dx, 0x00),
                None,
                "pixel {dx} should be blank"
            );
        }
        // After 24 pixels the register is empty; the sprite deactivates.
        assert_eq!(seq.draw_pixel(x0 + 24, 0x00), None);
    }

    /// A multicolour sprite consumes two data bits per pixel pair, so the same
    /// 24 bits cover 24 half-rate pixel positions with selectors 1..=3.
    #[test]
    fn multicolour_sprite_latches_bit_pairs() {
        let mut seq = SpriteSequencer::new();
        let x0 = 50;
        // Top pair = 0b01 (selector 1), next pair = 0b10 (selector 2).
        seq.load_data(0, 0b01_10 << 20);
        seq.set_x_positions([x0, 0, 0, 0, 0, 0, 0, 0]);
        seq.set_pending(0x01);
        seq.set_mc_bits(0x01); // sprite 0 multicolour

        // First pair (two pixels) carry selector 1.
        assert_eq!(seq.draw_pixel(x0, 0x00).map(|p| p.selector), Some(1));
        assert_eq!(seq.draw_pixel(x0 + 1, 0x00).map(|p| p.selector), Some(1));
        // Second pair carries selector 2.
        assert_eq!(seq.draw_pixel(x0 + 2, 0x00).map(|p| p.selector), Some(2));
        assert_eq!(seq.draw_pixel(x0 + 3, 0x00).map(|p| p.selector), Some(2));
    }

    /// Lower-numbered sprites win when two overlap on the same pixel.
    #[test]
    fn lower_sprite_number_takes_priority() {
        let mut seq = SpriteSequencer::new();
        let x0 = 80;
        seq.load_data(0, 0x80_0000); // sprite 0: top pixel set
        seq.load_data(1, 0x80_0000); // sprite 1: top pixel set
        seq.set_x_positions([x0, x0, 0, 0, 0, 0, 0, 0]);
        seq.set_pending(0x03);
        seq.set_mc_bits(0x00);

        // Both cover x0; sprite 0 wins.
        assert_eq!(seq.draw_pixel(x0, 0x00).map(|p| p.sprite), Some(0));
    }
}
