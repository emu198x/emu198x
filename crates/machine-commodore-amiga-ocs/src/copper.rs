//! Agnus Copper coprocessor.
//!
//! The Copper reads instruction pairs from chip RAM and writes
//! chipset registers at specific beam positions. Three instructions:
//!
//!   MOVE  reg, val  — word1 = reg<<1 (bit 0 = 0); word2 = val.
//!                     Writes `val` to `$DFF000 + reg`.
//!   WAIT  vp, hp    — word1 = (vp<<8) | (hp<<1) | 1; word2 has
//!                     bit 0 = 0 plus a compare-enable mask:
//!                       bit 15 = BFD (blitter-finished-disable).
//!                       bits 14-8 = VE (vertical-position mask).
//!                       bits 7-1  = HE (horizontal-position mask).
//!                     Pauses copper until the masked beam position
//!                     ≥ the masked target. VP bit 7 is ALWAYS
//!                     compared (HRM: "you can not mask the most
//!                     significant bit"). HP bit 0 is always ignored
//!                     (HRM: "the least significant bit is not used
//!                     in the comparison"). End-of-list sentinel:
//!                     WAIT \$FFFF \$FFFE — VP=$FF HP=$7F with full
//!                     mask, which can never be satisfied since
//!                     hpos maxes at $E2.
//!   SKIP  vp, hp    — same shape; word2 bit 0 = 1. Skips the
//!                     next instruction if the masked beam already
//!                     ≥ the masked target.
//!
//! Scheduling: this module does not yet model Agnus DMA-slot
//! arbitration for the copper (odd-CCK copper slots competing with
//! bitplane / sprite / blitter). The throttle is a simple "at most
//! one instruction per 4 CCKs" until a later milestone brings in
//! the full slot schedule.

use crate::chipset::Chipset;
use crate::memory::Memory;

/// Copper internal state.
#[derive(Default)]
pub struct Copper {
    /// COP1LC ($DFF080/$082) — first copper-list pointer.
    pub cop1lc: u32,
    /// COP2LC ($DFF084/$086) — second copper-list pointer.
    pub cop2lc: u32,
    /// Current copper PC (chip-RAM address of next instruction).
    pub pc: u32,
    /// `true` when the copper is paused at a WAIT instruction.
    pub waiting: bool,
    /// WAIT target position packed as `(vp << 8) | (hp_bit0_cleared)`.
    /// Only meaningful while `waiting` is true.
    pub wait_target: u16,
    /// WAIT mask (bit 15 forced to 1 since VP bit 7 is always
    /// compared; bit 0 forced to 0 since hpos LSB is ignored).
    pub wait_mask: u16,
    /// Blitter-finished-disable bit from the WAIT instruction. When
    /// false (`BFD=0`), the WAIT must also observe blitter-finished
    /// before proceeding. We don't yet model the blitter, so BFD=0
    /// currently behaves as "blitter always finished" — this is the
    /// same simplification UAE uses when the blitter is idle.
    pub wait_bfd: bool,
    /// CCKs accumulated since last copper instruction step. The
    /// copper does one MOVE / WAIT / SKIP every 4 CCKs (two 16-bit
    /// chip-RAM reads + an internal cycle).
    pub cck_phase: u8,
}

impl Copper {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// COPJMP1 strobe — load PC from COP1LC and clear waiting flag.
    pub fn jump1(&mut self) {
        self.pc = self.cop1lc;
        self.waiting = false;
        self.cck_phase = 0;
    }

    /// COPJMP2 strobe — load PC from COP2LC and clear waiting flag.
    pub fn jump2(&mut self) {
        self.pc = self.cop2lc;
        self.waiting = false;
        self.cck_phase = 0;
    }

    /// Tick the copper one CCK. Returns true if a copper-driven
    /// chipset write happened this CCK (caller used it to know
    /// whether to re-evaluate state, e.g. update IPL after an
    /// INTREQ write).
    pub fn tick_cck(
        &mut self,
        memory: &Memory,
        chipset: &mut Chipset,
        beam_vp: u16,
        beam_hp: u16,
    ) -> bool {
        // If waiting, only resume when the masked beam position
        // reaches the masked target AND (for BFD=0) the blitter is
        // finished. We don't yet model the blitter; treat BFD=0 as
        // "blitter always finished" — matches UAE's idle-blitter
        // behaviour.
        if self.waiting {
            if beam_match(self.wait_target, self.wait_mask, beam_vp, beam_hp) {
                self.waiting = false;
                self.cck_phase = 0;
            }
            return false;
        }

        // Throttle to one instruction per 4 CCKs.
        self.cck_phase = self.cck_phase.wrapping_add(1);
        if self.cck_phase < 4 {
            return false;
        }
        self.cck_phase = 0;

        // Fetch instruction pair (4 bytes) from chip RAM. Copper
        // accesses are always to chip RAM via Agnus.
        let word1 = (u16::from(memory.read_chip_ram_byte(self.pc)) << 8)
            | u16::from(memory.read_chip_ram_byte(self.pc.wrapping_add(1)));
        let word2 = (u16::from(memory.read_chip_ram_byte(self.pc.wrapping_add(2))) << 8)
            | u16::from(memory.read_chip_ram_byte(self.pc.wrapping_add(3)));
        self.pc = self.pc.wrapping_add(4);

        if word1 & 1 == 0 {
            // MOVE: reg = word1 & $1FE; val = word2.
            let reg = word1 & 0x1FE;
            chipset.write_word(reg, word2);
            return true;
        }

        // WAIT or SKIP — distinguished by word2 bit 0.
        let target = word1 & 0xFFFE;
        // Enable mask: word2 bits 14-1 come from the instruction.
        // Bit 15 (BFD) is NOT part of the mask (separate semantics).
        // Bit 0 (WAIT/SKIP flag) is not a position bit.
        // VP bit 7 is ALWAYS compared per HRM — force mask bit 15 = 1.
        let mask = (word2 & 0x7FFE) | 0x8000;
        let bfd = (word2 & 0x8000) != 0;

        if word2 & 1 == 0 {
            // WAIT.
            if beam_match(target, mask, beam_vp, beam_hp) {
                // Already past — instruction completes, copper
                // continues with the next pair on the next tick.
            } else {
                self.waiting = true;
                self.wait_target = target;
                self.wait_mask = mask;
                self.wait_bfd = bfd;
            }
        } else {
            // SKIP: if beam already satisfies the mask/target, skip
            // the next instruction word-pair.
            if beam_match(target, mask, beam_vp, beam_hp) {
                self.pc = self.pc.wrapping_add(4);
            }
        }
        false
    }
}

/// Compare the current beam position against a WAIT/SKIP target
/// using the enable mask defined by IR2. Returns true when the
/// masked current position is `>=` the masked target — the condition
/// WAIT releases on and SKIP skips on.
///
/// Semantics per Amiga Hardware Reference Manual 3rd ed., "Coprocessor
/// Hardware":
/// - HP bit 0 is NOT used in the comparison (step of 2 CCKs).
/// - VP bit 7 is always compared (the mask's bit 15 is forced on).
/// - Only the low 8 bits of vpos are used — vpos > 255 wraps to 0
///   for comparison purposes.
#[must_use]
pub fn beam_match(target: u16, mask: u16, beam_vp: u16, beam_hp: u16) -> bool {
    let current = ((beam_vp & 0x00FF) << 8) | (beam_hp & 0x00FE);
    (current & mask) >= (target & mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_memory_with_list(list: &[(u16, u16)], at: u32) -> Memory {
        let mut mem = Memory::new(vec![0u8; 256 * 1024]);
        // Drop overlay so chip RAM writes via this mem helper land
        // in chip RAM (Memory's overlay default is true).
        mem.set_overlay(false);
        for (i, (w1, w2)) in list.iter().enumerate() {
            let off = at + (i as u32) * 4;
            mem.write_byte(off, (*w1 >> 8) as u8);
            mem.write_byte(off + 1, *w1 as u8);
            mem.write_byte(off + 2, (*w2 >> 8) as u8);
            mem.write_byte(off + 3, *w2 as u8);
        }
        mem
    }

    #[test]
    fn move_writes_chipset_register() {
        let mem = build_test_memory_with_list(
            &[(0x0180, 0x0F0F), (0xFFFF, 0xFFFE)],
            0x1000,
        );
        let mut chipset = Chipset::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.jump1();

        // Copper instruction = 4 CCKs, so tick 4 times.
        for _ in 0..4 {
            copper.tick_cck(&mem, &mut chipset, 0, 0);
        }
        assert_eq!(chipset.color[0], 0x0F0F);
    }

    #[test]
    fn wait_pauses_until_beam_target() {
        // WAIT line 5, full mask, then MOVE COLOR00=$0FFF.
        let mem = build_test_memory_with_list(
            &[
                (0x0501, 0xFFFE),   // WAIT v=5, h=0, full mask
                (0x0180, 0x0FFF),   // MOVE COLOR00 = $0FFF (after wait)
                (0xFFFF, 0xFFFE),
            ],
            0x1000,
        );
        let mut chipset = Chipset::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.jump1();

        // Tick the WAIT instruction (4 CCKs) — copper goes to waiting.
        for _ in 0..4 {
            copper.tick_cck(&mem, &mut chipset, 0, 0);
        }
        assert!(copper.waiting);

        // Tick more with beam still below target — MOVE doesn't run.
        for _ in 0..50 {
            copper.tick_cck(&mem, &mut chipset, 4, 200);
        }
        assert_eq!(chipset.color[0], 0);

        // Tick with beam at target — copper resumes.
        for _ in 0..8 {
            copper.tick_cck(&mem, &mut chipset, 5, 0);
        }
        assert!(!copper.waiting);
        assert_eq!(chipset.color[0], 0x0FFF);
    }

    /// Direct beam_match coverage of the mask semantics in the HRM.
    #[test]
    fn beam_match_masks_per_hrm() {
        // Target $9600 (line 150, hp 0), VE = $7F (all vpos bits),
        // HE = $00 (hpos ignored) → mask after force-bit-15 = $FF00.
        let target = 0x9600;
        let mask = (0xFF00u16 & 0x7FFE) | 0x8000;

        // vpos=149: below, shouldn't match.
        assert!(!beam_match(target, mask, 149, 0));
        assert!(!beam_match(target, mask, 149, 0xE2));
        // vpos=150: matches regardless of hpos (hpos masked out).
        assert!(beam_match(target, mask, 150, 0));
        assert!(beam_match(target, mask, 150, 0xE2));
        // vpos=200: past, matches.
        assert!(beam_match(target, mask, 200, 0));
    }

    #[test]
    fn beam_match_vp_bit7_is_always_compared() {
        // Even with VE=$00 (target tries to mask all vpos bits) the
        // HRM says VP bit 7 is ALWAYS compared. So if target has
        // VP bit 7 = 1 and current has VP bit 7 = 0, WAIT never
        // releases.
        let target = 0x8000; // VP=128, HP=0
        let mask = (0x0000u16 & 0x7FFE) | 0x8000; // all low bits masked; bit 15 forced

        assert!(!beam_match(target, mask, 127, 0));
        assert!(!beam_match(target, mask, 10, 0xE2));
        assert!(beam_match(target, mask, 128, 0));
        assert!(beam_match(target, mask, 200, 0));
    }

    #[test]
    fn beam_match_hp_bit0_is_ignored() {
        // Target IR1 = $0005: WAIT flag (bit 0) + HP bit 1 (bit 1) +
        // HP bit 2 (bit 2) → HP field value 2, which in HRM's
        // "horizontal position" terms is beam_hp = 4.
        //   target (after clearing flag) = $0004
        //   mask bits 7-1 set → mask = $80FE
        let target = 0x0004;
        let mask = (0x00FEu16 & 0x7FFE) | 0x8000;

        // beam_hp = 4: current = 4, 4 >= 4 → true.
        assert!(beam_match(target, mask, 0, 4));
        // beam_hp = 5: LSB ignored → current still = 4, 4 >= 4 → true.
        assert!(beam_match(target, mask, 0, 5));
        // beam_hp = 3: current = 2 (LSB cleared), 2 >= 4 → false.
        assert!(!beam_match(target, mask, 0, 3));
        // beam_hp = 6: current = 6, 6 >= 4 → true.
        assert!(beam_match(target, mask, 0, 6));
    }

    #[test]
    fn wait_with_horizontal_mask_ignores_hpos() {
        // WAIT line 10 with HE=0 (horizontal ignored). MOVE runs as
        // soon as vpos == 10 regardless of hpos.
        // IR1 = $0A01 (VP=10, HP=0, flag=1)
        // IR2 = $FF00 (BFD=1, VE=$7F, HE=$00, flag=0)
        let mem = build_test_memory_with_list(
            &[
                (0x0A01, 0xFF00),
                (0x0180, 0x0ABC),
                (0xFFFF, 0xFFFE),
            ],
            0x2000,
        );
        let mut chipset = Chipset::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x2000;
        copper.jump1();

        // Execute WAIT at vpos=0 hpos=200 — should enter waiting.
        for _ in 0..4 {
            copper.tick_cck(&mem, &mut chipset, 0, 200);
        }
        assert!(copper.waiting);
        assert_eq!(copper.wait_target, 0x0A00);
        // Mask: (0xFF00 & 0x7FFE) | 0x8000 = 0xFF00.
        assert_eq!(copper.wait_mask, 0xFF00);

        // Advance beam to vpos=10 at any hpos — WAIT releases and
        // MOVE runs on the next instruction window.
        for _ in 0..8 {
            copper.tick_cck(&mem, &mut chipset, 10, 50);
        }
        assert!(!copper.waiting);
        assert_eq!(chipset.color[0], 0x0ABC);
    }

    #[test]
    fn skip_consumes_next_instruction_when_condition_met() {
        // SKIP if vpos >= 5 (full mask), followed by two MOVEs.
        // IR1 = $0501 (WAIT/SKIP flag), IR2 = $FFFF (SKIP, full mask).
        let mem = build_test_memory_with_list(
            &[
                (0x0501, 0xFFFF),           // SKIP if beam >= (5, 0)
                (0x0180, 0x0F00),           // COLOR00 = $F00
                (0x0182, 0x00F0),           // COLOR01 = $0F0
                (0xFFFF, 0xFFFE),
            ],
            0x3000,
        );
        let mut chipset = Chipset::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x3000;
        copper.jump1();

        // Beam past target → SKIP consumes COLOR00 MOVE, only COLOR01
        // runs.
        for _ in 0..16 {
            copper.tick_cck(&mem, &mut chipset, 100, 0);
        }
        assert_eq!(chipset.color[0], 0x0000);
        assert_eq!(chipset.color[1], 0x00F0);
    }

    #[test]
    fn skip_does_not_consume_when_beam_before_target() {
        let mem = build_test_memory_with_list(
            &[
                (0x6401, 0xFFFF),           // SKIP if beam >= (100, 0)
                (0x0180, 0x0F00),           // COLOR00 = $F00 (should run)
                (0xFFFF, 0xFFFE),
            ],
            0x4000,
        );
        let mut chipset = Chipset::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x4000;
        copper.jump1();

        for _ in 0..12 {
            copper.tick_cck(&mem, &mut chipset, 10, 0);
        }
        assert_eq!(chipset.color[0], 0x0F00);
    }
}
