//! Agnus Copper coprocessor — minimal M10 implementation.
//!
//! The Copper reads instruction pairs from chip RAM and writes
//! chipset registers at specific beam positions. Three instructions:
//!
//!   MOVE  reg, val  — word1 = reg<<1 (bit 0 = 0); word2 = val.
//!                     Writes `val` to `$DFF000 + reg`.
//!   WAIT  vp, hp    — word1 = (vp<<8) | hp | 1; word2 = mask | 0.
//!                     Pauses copper until beam (vpos,hpos) >= (vp,hp).
//!                     End-of-list = WAIT $FF, $FE.
//!   SKIP  vp, hp    — same shape; word2 bit 0 = 1. Skips next
//!                     instruction if beam already past target.
//!
//! M10 simplifies: no DMA-slot scheduling. The copper executes one
//! instruction every 4 CCKs when DMACON.COPEN + DMAEN are set and
//! the copper isn't in the WAIT-paused state.

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
    /// `true` when the copper is paused at a WAIT instruction. The
    /// `wait_target` fields hold the beam position the WAIT is
    /// blocked on.
    pub waiting: bool,
    pub wait_vp: u8,
    pub wait_hp: u8,
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
        // If waiting, only resume when beam crosses target.
        if self.waiting {
            // Compare as 16-bit words: target_v in high byte, target_h in low.
            let target = (u16::from(self.wait_vp) << 8) | u16::from(self.wait_hp);
            let current = (beam_vp.min(0xFF) << 8) | beam_hp.min(0xFF);
            if current >= target {
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
        // accesses are always to chip RAM via Agnus; we use the
        // OVL-aware Memory::read_chip_ram_byte.
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
        let target_vp = (word1 >> 8) as u8;
        let target_hp = (word1 & 0xFE) as u8;

        if word2 & 1 == 0 {
            // WAIT: pause until beam reaches (target_vp, target_hp).
            // End-of-list shortcut: WAIT $FF, $FE = $FFFF FFFE — vp 255
            // is unreachable on PAL (max 311 fits but vp field is 8-bit
            // so 255 with hp 254 effectively means "wait forever").
            let target = (u16::from(target_vp) << 8) | u16::from(target_hp);
            let current = (beam_vp.min(0xFF) << 8) | beam_hp.min(0xFF);
            if current >= target {
                // Already past — fall through (instruction completed).
            } else {
                self.waiting = true;
                self.wait_vp = target_vp;
                self.wait_hp = target_hp;
            }
        } else {
            // SKIP: if beam already past target, skip next instruction.
            let target = (u16::from(target_vp) << 8) | u16::from(target_hp);
            let current = (beam_vp.min(0xFF) << 8) | beam_hp.min(0xFF);
            if current >= target {
                self.pc = self.pc.wrapping_add(4);
            }
        }
        false
    }
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
        let mem = build_test_memory_with_list(
            &[
                (0x0501, 0xFFFE),   // WAIT v=5, h=0
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
}
