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

use crate::denise::DmaClaim;
use crate::memory::Memory;
use serde::{Deserialize, Serialize};

/// Copper internal state.
#[derive(Default, Clone, Serialize, Deserialize)]
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
    /// CCKs accumulated since last copper instruction step. MOVE and
    /// SKIP complete in 2 eligible CCKs (HRM: "two memory cycles and
    /// four memory clocks per instruction"). WAIT needs 3 eligible
    /// CCKs total; see `pending_wait_delay`.
    pub cck_phase: u8,
    /// Post-decode one-CCK delay for WAIT. HRM: "The WAIT instruction
    /// requires three memory cycles and six memory clocks per
    /// instruction" — one more memory cycle than MOVE/SKIP. After
    /// fetching + decoding a WAIT, the copper holds this extra cycle
    /// before entering the waiting state. Fields below stash the
    /// target / mask / bfd that will be committed when the delay
    /// eligible-CCK arrives.
    pub pending_wait_delay: bool,
    pub pending_wait_target: u16,
    pub pending_wait_mask: u16,
    pub pending_wait_bfd: bool,
    /// Copper halted by a dangerous MOVE (register address < $80 and
    /// CDANG = 0). Per HRM + WinUAE: "if the Copper DMA attempts to
    /// write to a register below $80, the Copper DMA is stopped".
    /// Resumes when COPJMP1 / COPJMP2 fires (typically the next VBL
    /// auto-strobe). Without this gate, chip-only KS 1.3 lets the
    /// copper run through ExecBase-as-copper-list, corrupting INTENA
    /// and deadlocking the scheduler. See task #96.
    pub stopped: bool,
    /// COPCON bit 1 (CDANG — "copper danger"). When 1, the copper is
    /// allowed to MOVE to registers below $80 (blitter + Agnus I/O
    /// space). Power-on default is 0, which matches the KS 1.3 boot
    /// path. Set via the $DFF02E write at the custom-register bus.
    pub cdang: bool,
}

impl Copper {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// CPU write to COPCON ($DFF02E). Only bit 1 (CDANG) is meaningful.
    pub fn write_copcon(&mut self, val: u16) {
        self.cdang = val & 0x0002 != 0;
    }

    /// COPJMP1 strobe — load PC from COP1LC and clear waiting flag.
    pub fn jump1(&mut self) {
        self.pc = self.cop1lc;
        self.waiting = false;
        self.cck_phase = 0;
        self.pending_wait_delay = false;
        self.stopped = false;
    }

    /// COPJMP2 strobe — load PC from COP2LC and clear waiting flag.
    pub fn jump2(&mut self) {
        self.pc = self.cop2lc;
        self.waiting = false;
        self.cck_phase = 0;
        self.pending_wait_delay = false;
        self.stopped = false;
    }

    /// Tick the copper one CCK. Returns `Some((reg, val))` when
    /// the copper has just executed a MOVE instruction — the
    /// caller is responsible for routing the write through the
    /// full machine-layer dispatch (same path the CPU uses). This
    /// matters because the copper can legitimately MOVE to any
    /// custom register, not just the Denise-owned ones: bitplane
    /// pointers (Agnus), DMACON/INTENA/INTREQ/ADKCON (Agnus/Paula),
    /// DDF/DIW/modulos (Agnus), and sprite pointers (Agnus) all
    /// see copper writes during normal boot. Routing everything to
    /// `denise.write_word` (as an earlier port did) silently drops
    /// those writes and leaves the copper unable to re-load
    /// bitplane pointers each frame.
    ///
    /// COPJMP1 / COPJMP2 strobes stay internal — the copper reloads
    /// its own PC and returns `None` for those.
    pub fn tick_cck(
        &mut self,
        memory: &Memory,
        beam_vp: u16,
        beam_hp: u16,
        claim: DmaClaim,
    ) -> Option<(u16, u16)> {
        // Halted by a dangerous MOVE? Sit still until the next
        // COPJMP1/COPJMP2 strobe (which the VBL auto-fires).
        if self.stopped {
            return None;
        }

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
            return None;
        }

        // Copper-eligible CCK: odd hpos AND no bitplane claim. Per
        // HRM Chapter 2: "The Copper is a two-cycle processor that
        // requests the bus only during odd-numbered memory cycles."
        // BPL5 / BPL6 at BPU ≥ 5 / 6 claim odd slots within DDF —
        // those are the ones copper must yield to.
        let eligible = (beam_hp & 1) != 0 && claim.is_free();
        if !eligible {
            return None;
        }

        // WAIT takes 3 memory cycles (HRM). The first 2 fetch + decode
        // the instruction pair (handled by the fetch throttle below);
        // the 3rd is the extra cycle before actually pausing. When
        // this flag is set, the current eligible CCK is that 3rd
        // cycle: commit the stashed target/mask and enter waiting.
        if self.pending_wait_delay {
            // This eligible CCK is the WAITSKIP2 cycle (Minimig
            // agnus_copper.v FSM: FETCH1 → FETCH2 → WAITSKIP1 →
            // WAITSKIP2). The beam comparison happens HERE — after the
            // word-pair fetch and the dummy cycle — not at fetch time.
            // Evaluating it here is what makes the $FFDF line-255
            // crossing behave (#458): a WAIT fetched late on line 255
            // has its compare deferred to a couple of CCKs later, by
            // which the beam has wrapped to line 256 (V[7:0] = 0),
            // instead of seeing the still-$FF V[7:0] of line 255.
            self.pending_wait_delay = false;
            if beam_match(
                self.pending_wait_target,
                self.pending_wait_mask,
                beam_vp,
                beam_hp,
            ) {
                // Beam already at/past target: the WAIT completes and
                // the copper proceeds to fetch the next instruction.
                self.cck_phase = 0;
            } else {
                self.waiting = true;
                self.wait_target = self.pending_wait_target;
                self.wait_mask = self.pending_wait_mask;
                self.wait_bfd = self.pending_wait_bfd;
            }
            return None;
        }

        // Each MOVE / SKIP (and the fetch+decode portion of WAIT)
        // requires two memory cycles (= two eligible CCKs). In
        // unconstrained conditions those are two consecutive odd
        // CCKs, for 4 wall CCKs. When BPL5 / BPL6 steal odd slots the
        // copper's effective rate drops proportionally.
        self.cck_phase = self.cck_phase.wrapping_add(1);
        if self.cck_phase < 2 {
            return None;
        }
        self.cck_phase = 0;

        // Fetch instruction pair from chip RAM. Copper accesses are
        // always chip-RAM-only, via Agnus DMA; each fetch drives the
        // chip bus so the floating-bus residue tracks it.
        let word1 = memory.read_chip_ram_word(self.pc);
        let word2 = memory.read_chip_ram_word(self.pc.wrapping_add(2));
        self.pc = self.pc.wrapping_add(4);

        if word1 & 1 == 0 {
            // MOVE: reg = word1 & $1FE; val = word2.
            let reg = word1 & 0x1FE;
            // "Dangerous" MOVE per HRM Appendix A / WinUAE
            // test_copper_dangerous: a MOVE to a register address
            // below $80 halts the copper unless CDANG (COPCON bit 1)
            // has been set. CDANG defaults clear at reset; KS 1.3 boot
            // never sets it.
            //
            // This is the mechanism that rescues real 512K-chip-only
            // A500s when VBL leaves COP2LC = ExecBase: the first
            // ExecBase longword (ln_Succ = 0) decodes as MOVE $000,
            // which halts the copper immediately, preventing it from
            // executing ExecBase struct bytes as instructions.
            if reg < 0x80 && !self.cdang {
                self.stopped = true;
                return None;
            }
            // COPJMP1 / COPJMP2 strobes ($088 / $08A) are copper-
            // internal — writing to them reloads the copper PC from
            // COP1LC / COP2LC. These aren't real chipset registers,
            // so Chipset::write_word doesn't handle them; we must
            // handle them here because a running copper list can do
            // its own jumps (tail-chained from one list to another
            // at the start of each VBL).
            //
            // Every other MOVE bubbles up to the caller, which is
            // responsible for driving it through the full machine
            // dispatch (`dispatch_custom_write`). Bitplane pointers,
            // DMACON, INTENA, sprite pointers, DDF/DIW etc. all land
            // there — writing them directly through Denise would
            // silently drop all non-Denise registers.
            match reg {
                0x088 => {
                    self.jump1();
                    None
                }
                0x08A => {
                    self.jump2();
                    None
                }
                _ => Some((reg, word2)),
            }
        } else {
            // WAIT or SKIP — distinguished by word2 bit 0.
            let target = word1 & 0xFFFE;
            // Enable mask: word2 bits 14-1 come from the instruction.
            // Bit 15 (BFD) is NOT part of the mask (separate
            // semantics). Bit 0 (WAIT/SKIP flag) is not a position
            // bit. VP bit 7 is ALWAYS compared per HRM — force mask
            // bit 15 = 1.
            let mask = (word2 & 0x7FFE) | 0x8000;
            let bfd = (word2 & 0x8000) != 0;

            if word2 & 1 == 0 {
                // WAIT. The beam comparison is NOT made here at fetch
                // time — it is deferred to the WAITSKIP2 cycle (the
                // `pending_wait_delay` handler above), matching the
                // Agnus copper FSM. Arm the delay unconditionally and
                // stash the target/mask/bfd; the next eligible CCK
                // evaluates the compare and either completes the WAIT
                // (already past) or commits the waiting state.
                self.pending_wait_delay = true;
                self.pending_wait_target = target;
                self.pending_wait_mask = mask;
                self.pending_wait_bfd = bfd;
            } else {
                // SKIP: if beam already satisfies the mask/target,
                // skip the next instruction word-pair.
                if beam_match(target, mask, beam_vp, beam_hp) {
                    self.pc = self.pc.wrapping_add(4);
                }
            }
            None
        }
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
    // Test the copper against the concrete OCS Denise — the copper
    // is chipset-agnostic so any DeniseChip impl would do, but
    // DeniseOcs is the canonical reference.
    use crate::denise::Denise;
    use commodore_denise_ocs::DeniseOcs;
    type TestDenise = Denise<DeniseOcs>;

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

    /// Tick the copper for `ccks` wall-CCKs, advancing hpos each
    /// tick and holding vpos fixed. Keeps claim = Free so copper
    /// sees unconstrained odd-CCK availability. MOVEs returned by
    /// the copper are routed through Denise — these tests only
    /// exercise Denise-owned registers, so a local routing closure
    /// is enough. The machine layer wires this through the full
    /// `dispatch_custom_write` in production.
    fn run_ccks(copper: &mut Copper, mem: &Memory, denise: &mut TestDenise, vpos: u16, ccks: u16) {
        for i in 0..ccks {
            if let Some((reg, val)) = copper.tick_cck(mem, vpos, i % 227, DmaClaim::Free) {
                denise.write_word(reg, val);
            }
        }
    }

    #[test]
    fn move_writes_chipset_register() {
        let mem = build_test_memory_with_list(&[(0x0180, 0x0F0F), (0xFFFF, 0xFFFE)], 0x1000);
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.jump1();

        // Copper instruction = 2 odd-CCK memory cycles = 4 wall CCKs
        // when unconstrained. Run 4 wall CCKs with hpos cycling 0..3.
        run_ccks(&mut copper, &mem, &mut denise, 0, 4);
        assert_eq!(denise.color(0), 0x0F0F);
    }

    #[test]
    fn move_does_not_run_when_only_even_ccks_offered() {
        // Pin hpos = 0 (even) for 40 ticks — copper gets zero
        // eligible cycles and should not execute.
        let mem = build_test_memory_with_list(&[(0x0180, 0x0F0F), (0xFFFF, 0xFFFE)], 0x1000);
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.jump1();

        for _ in 0..40 {
            let write = copper.tick_cck(&mem, 0, 0, DmaClaim::Free);
            if let Some((reg, val)) = write {
                denise.write_word(reg, val);
            }
        }
        assert_eq!(denise.color(0), 0x0000, "copper must not run on even CCKs");
    }

    #[test]
    fn move_blocked_by_bitplane_claim_on_odd_cck() {
        // Copper MOVE needs 2 eligible odd CCKs. If every odd CCK is
        // claimed by a bitplane (simulating BPL5/BPL6 contention at
        // BPU ≥ 5), the copper never completes.
        let mem = build_test_memory_with_list(&[(0x0180, 0x0F0F), (0xFFFF, 0xFFFE)], 0x1000);
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.jump1();

        for i in 0..40u16 {
            let hpos = i % 227;
            let claim = if hpos & 1 != 0 {
                DmaClaim::Bitplane(5) // BPL6 blocks copper
            } else {
                DmaClaim::Free
            };
            if let Some((reg, val)) = copper.tick_cck(&mem, 0, hpos, claim) {
                denise.write_word(reg, val);
            }
        }
        assert_eq!(
            denise.color(0),
            0x0000,
            "copper must yield to bitplane DMA on odd CCKs",
        );
    }

    #[test]
    fn wait_pauses_until_beam_target() {
        // WAIT line 5, full mask, then MOVE COLOR00=$0FFF.
        let mem = build_test_memory_with_list(
            &[
                (0x0501, 0xFFFE), // WAIT v=5, h=0, full mask
                (0x0180, 0x0FFF), // MOVE COLOR00 = $0FFF (after wait)
                (0xFFFF, 0xFFFE),
            ],
            0x1000,
        );
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.jump1();

        // Tick the WAIT instruction: 3 eligible CCKs = 6 wall CCKs
        // (HRM: "WAIT requires three memory cycles and six memory
        // clocks per instruction").
        run_ccks(&mut copper, &mem, &mut denise, 0, 6);
        assert!(copper.waiting);

        // Tick more with beam still below target — MOVE doesn't run.
        for i in 0..50 {
            if let Some((reg, val)) = copper.tick_cck(&mem, 4, i % 227, DmaClaim::Free) {
                denise.write_word(reg, val);
            }
        }
        assert_eq!(denise.color(0), 0);

        // Tick with beam at target — WAIT releases (i=0) then copper
        // needs 2 eligible odd CCKs (i=1, i=3) to execute the MOVE.
        // 4 wall CCKs is exactly right; running further would fetch
        // the end-of-list WAIT and re-enter the waiting state.
        run_ccks(&mut copper, &mem, &mut denise, 5, 4);
        assert_eq!(denise.color(0), 0x0FFF, "MOVE after WAIT release");
    }

    #[test]
    fn wait_takes_3_eligible_ccks_before_pausing() {
        // HRM: "The WAIT instruction requires three memory cycles
        // and six memory clocks per instruction." MOVE and SKIP take
        // 2 memory cycles / 4 memory clocks. The difference is one
        // extra eligible CCK of delay between the word-pair fetch
        // (cycles 1 + 2) and actually entering the waiting state
        // (cycle 3).
        let mem = build_test_memory_with_list(
            &[
                (0x0501, 0xFFFE), // WAIT v=5, full mask
                (0xFFFF, 0xFFFE), // end-of-list
            ],
            0x1000,
        );
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.jump1();

        // Tick 2 eligible CCKs (= 4 wall CCKs). The WAIT is fetched
        // and decoded, but the 3rd memory cycle hasn't fired yet:
        // `waiting` should still be false and the delay should be
        // armed instead.
        run_ccks(&mut copper, &mem, &mut denise, 0, 4);
        assert!(
            !copper.waiting,
            "WAIT must not enter waiting yet — only 2 eligible CCKs \
             elapsed, HRM requires 3",
        );
        assert!(
            copper.pending_wait_delay,
            "After fetch+decode of WAIT the 3rd-cycle delay should be armed",
        );

        // One more eligible CCK (2 wall CCKs) fires the 3rd memory
        // cycle and commits the waiting state.
        run_ccks(&mut copper, &mem, &mut denise, 0, 2);
        assert!(
            copper.waiting,
            "3rd eligible CCK enters waiting (HRM's 3-cycle rule)",
        );
        assert!(!copper.pending_wait_delay);
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
        // All low bits masked out; bit 15 forced per HRM.
        let mask = 0x8000u16;

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
            &[(0x0A01, 0xFF00), (0x0180, 0x0ABC), (0xFFFF, 0xFFFE)],
            0x2000,
        );
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x2000;
        copper.jump1();

        // Execute WAIT at vpos=0: 3 eligible CCKs = 6 wall CCKs for
        // the full HRM-accurate WAIT timing.
        run_ccks(&mut copper, &mem, &mut denise, 0, 6);
        assert!(copper.waiting);
        assert_eq!(copper.wait_target, 0x0A00);
        // Mask: (0xFF00 & 0x7FFE) | 0x8000 = 0xFF00.
        assert_eq!(copper.wait_mask, 0xFF00);

        // Advance beam to vpos=10 — WAIT releases (i=0), then copper
        // takes 2 eligible odd CCKs to fetch + execute the MOVE.
        // 4 wall CCKs is enough; don't overrun into end-of-list.
        run_ccks(&mut copper, &mem, &mut denise, 10, 4);
        assert_eq!(denise.color(0), 0x0ABC, "MOVE after horizontal-masked WAIT");
    }

    #[test]
    fn skip_consumes_next_instruction_when_condition_met() {
        // SKIP if vpos >= 5 (full mask), followed by two MOVEs.
        // IR1 = $0501 (WAIT/SKIP flag), IR2 = $FFFF (SKIP, full mask).
        let mem = build_test_memory_with_list(
            &[
                (0x0501, 0xFFFF), // SKIP if beam >= (5, 0)
                (0x0180, 0x0F00), // COLOR00 = $F00
                (0x0182, 0x00F0), // COLOR01 = $0F0
                (0xFFFF, 0xFFFE),
            ],
            0x3000,
        );
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x3000;
        copper.jump1();

        // Beam past target → SKIP consumes COLOR00 MOVE, only COLOR01
        // runs. Each instruction = 4 wall CCKs, we have 3 instrs →
        // need ≥ 12 CCKs of eligible copper cycles.
        run_ccks(&mut copper, &mem, &mut denise, 100, 16);
        assert_eq!(denise.color(0), 0x0000);
        assert_eq!(denise.color(1), 0x00F0);
    }

    #[test]
    fn copjmp2_strobe_inside_list_jumps_to_cop2lc() {
        // A copper list can chain to another list mid-flight by
        // writing to COPJMP1 ($088) or COPJMP2 ($08A). Real Agnus
        // treats these writes as strobes that reload the copper PC.
        // Chipset::write_word doesn't handle them (they're not
        // stored registers), so Copper must handle them directly.
        //
        // List at $1000:
        //   MOVE COLOR00 = $0F00   (sets color marker #1)
        //   MOVE $08A = 0          (COPJMP2 — jumps to COP2LC)
        //   MOVE COLOR01 = $00F0   (should NOT run — jumped past)
        //   ...
        // List at $2000 (COP2LC target):
        //   MOVE COLOR02 = $00FF   (runs after the jump)
        //   ...
        let mem = build_test_memory_with_list(
            &[
                (0x0180, 0x0F00), // MOVE COLOR00
                (0x008A, 0x0000), // MOVE COPJMP2 strobe
                (0x0182, 0x00F0), // MOVE COLOR01 (should be skipped)
                (0xFFFF, 0xFFFE), // end
            ],
            0x1000,
        );
        // Stash the COP2LC target list at $2000. Re-use a fresh
        // memory helper call that appends to the same chip RAM.
        let mut mem = mem;
        let target_list = [(0x0184u16, 0x00FFu16), (0xFFFF, 0xFFFE)];
        for (i, (w1, w2)) in target_list.iter().enumerate() {
            let off = 0x2000 + (i as u32) * 4;
            mem.write_byte(off, (*w1 >> 8) as u8);
            mem.write_byte(off + 1, *w1 as u8);
            mem.write_byte(off + 2, (*w2 >> 8) as u8);
            mem.write_byte(off + 3, *w2 as u8);
        }

        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.cop2lc = 0x2000;
        copper.jump1();

        // Plenty of cycles to run through MOVE + COPJMP2 + MOVE.
        run_ccks(&mut copper, &mem, &mut denise, 10, 20);
        assert_eq!(
            denise.color(0),
            0x0F00,
            "first MOVE before COPJMP2 should run"
        );
        assert_eq!(
            denise.color(1),
            0x0000,
            "MOVE after COPJMP2 in list-1 must NOT run (jumped past)",
        );
        assert_eq!(
            denise.color(2),
            0x00FF,
            "MOVE in list-2 (at COP2LC) should run after the jump",
        );
    }

    #[test]
    fn dangerous_move_stops_copper() {
        // MOVE to BLTDDAT (reg $000) with CDANG=0 must halt the
        // copper. The write is discarded and subsequent instructions
        // do not execute until a COPJMP strobe restarts it.
        let mem = build_test_memory_with_list(
            &[
                (0x0000, 0x1234), // MOVE BLTDDAT (dangerous)
                (0x0180, 0x0F00), // MOVE COLOR00=$F00 (must NOT run)
                (0xFFFF, 0xFFFE),
            ],
            0x5000,
        );
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x5000;
        copper.jump1();

        run_ccks(&mut copper, &mem, &mut denise, 0, 40);
        assert_eq!(
            denise.color(0),
            0x0000,
            "COLOR00 must NOT have been written — dangerous MOVE halts copper"
        );
        assert!(
            copper.stopped,
            "copper should be stopped after dangerous MOVE"
        );

        // Restart via COPJMP1: COP1LC unchanged → copper re-runs the
        // same dangerous MOVE and halts again. But if we reset COP1LC
        // to a safe list first, it resumes.
        let mem2 = build_test_memory_with_list(&[(0x0180, 0x0ABC), (0xFFFF, 0xFFFE)], 0x6000);
        copper.cop1lc = 0x6000;
        copper.jump1();
        assert!(!copper.stopped, "COPJMP1 must clear stopped");
        run_ccks(&mut copper, &mem2, &mut denise, 0, 8);
        assert_eq!(denise.color(0), 0x0ABC, "copper restarts after COPJMP1");
    }

    #[test]
    fn safe_register_threshold_is_exactly_dollar_80() {
        // Reg $7E is still dangerous ($< $80); reg $80 is safe.
        for (reg, should_halt) in [(0x007E, true), (0x0080, false)] {
            let mem = build_test_memory_with_list(&[(reg, 0x1234), (0xFFFF, 0xFFFE)], 0x7000);
            let mut denise = TestDenise::new();
            let mut copper = Copper::new();
            copper.cop1lc = 0x7000;
            copper.jump1();
            run_ccks(&mut copper, &mem, &mut denise, 0, 8);
            assert_eq!(
                copper.stopped,
                should_halt,
                "reg ${reg:03X} should {} the copper",
                if should_halt { "halt" } else { "NOT halt" }
            );
        }
    }

    #[test]
    fn skip_does_not_consume_when_beam_before_target() {
        let mem = build_test_memory_with_list(
            &[
                (0x6401, 0xFFFF), // SKIP if beam >= (100, 0)
                (0x0180, 0x0F00), // COLOR00 = $F00 (should run)
                (0xFFFF, 0xFFFE),
            ],
            0x4000,
        );
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x4000;
        copper.jump1();

        run_ccks(&mut copper, &mem, &mut denise, 10, 12);
        assert_eq!(denise.color(0), 0x0F00);
    }

    /// Drive a real beam across the line-255 → 256 wrap through the
    /// canonical PAL-bottom sequence and report the line on which the
    /// post-crossing MOVE fires. Returns `Some(line)` or `None` if the
    /// copper never reached the MOVE within the frame.
    fn ffdf_crossing_fire_line() -> Option<u16> {
        // WAIT $FFDF (cross line 255), WAIT $1C01 (target line 284),
        // MOVE COLOR00, end. Full PAL line = 227 CCKs (hpos 0..=226).
        let mem = build_test_memory_with_list(
            &[
                (0xFFDF, 0xFFFE),
                (0x1C01, 0xFFFE),
                (0x0180, 0x0F00),
                (0xFFFF, 0xFFFE),
            ],
            0x1000,
        );
        let mut denise = TestDenise::new();
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.jump1();

        for vpos in 250u16..312 {
            for hpos in 0u16..227 {
                if let Some((reg, val)) = copper.tick_cck(&mem, vpos, hpos, DmaClaim::Free) {
                    denise.write_word(reg, val);
                    if reg == 0x0180 {
                        return Some(vpos);
                    }
                }
            }
        }
        None
    }

    #[test]
    fn ffdf_line255_crossing_waits_for_the_real_target_line() {
        // Regression for #458. After `WAIT $FFDF` crosses line 255
        // (V[7:0] = $FF), a following `WAIT $1C01` must wait for the
        // beam to wrap to line 256 (V[7:0] = 0) and count up to $1C —
        // i.e. fire at line 284 — NOT fire immediately at the crossing.
        //
        // The bug fired it at line 256 because the copper evaluated the
        // second WAIT's beam comparison at fetch time (still V[7:0]=$FF,
        // and $FF >= $1C), instead of at the hardware WAITSKIP2 point a
        // couple of CCKs later, by which the beam has wrapped.
        let line = ffdf_crossing_fire_line();
        assert_eq!(
            line,
            Some(284),
            "post-crossing MOVE must fire at line 284, not at the crossing"
        );
    }
}
