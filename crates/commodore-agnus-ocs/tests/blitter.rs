//! Phase 1 characterization tests — Blitter.
//!
//! Covers tasks #134–#138. The Blitter lives in the Agnus die; these
//! tests drive it via the public API (pub fields + start_blit +
//! execute_incremental_blitter_op) using a small in-test memory map.
//!
//! Per HRM chapter 7 (Blitter Hardware):
//!
//!   BLTCON0 bits 15-12  ash     A shift (0-15 bits left)
//!   BLTCON0 bit  11     USEA
//!   BLTCON0 bit  10     USEB
//!   BLTCON0 bit   9     USEC
//!   BLTCON0 bit   8     USED
//!   BLTCON0 bits  7-0   lf      minterm LUT (the 256 logic functions)
//!
//!   BLTCON1 bits 15-12  bsh     B shift
//!   BLTCON1 bit   4     efe     exclusive fill enable
//!   BLTCON1 bit   3     ife     inclusive fill enable
//!   BLTCON1 bit   2     fc      fill carry initial
//!   BLTCON1 bit   1     desc    descending (reverse)
//!   BLTCON1 bit   0     line    line mode
//!
//!   BLTSIZE: height (bits 15-6) = rows, width (bits 5-0) = words/row.
//!
//! The minterm LUT covers all 256 combinations of (A,B,C) → D. Test
//! #135 exercises every index by setting specific bits in the LUT.

use std::cell::RefCell;
use std::collections::HashMap;

use commodore_agnus_ocs::Agnus;

// ────────────────────────────────────────────────────────────────
// Test scaffolding
// ────────────────────────────────────────────────────────────────

struct TestRam {
    cells: RefCell<HashMap<u32, u16>>,
}

impl TestRam {
    fn new() -> Self {
        Self {
            cells: RefCell::new(HashMap::new()),
        }
    }
    fn poke(&self, addr: u32, val: u16) {
        self.cells.borrow_mut().insert(addr & !1, val);
    }
    fn peek(&self, addr: u32) -> u16 {
        *self.cells.borrow().get(&(addr & !1)).unwrap_or(&0)
    }
}

/// Drive the blitter synchronously from `start_blit` to completion.
/// Returns the total number of DMA ops that ran.
fn run_blit(agnus: &mut Agnus, ram: &TestRam) -> u32 {
    let mut ops = 0u32;
    while let Some(op) = agnus.next_blitter_dma_request() {
        agnus.grant_blitter_dma_op(op);
        let read = |addr: u32| ram.peek(addr);
        let write = |addr: u32, val: u16| ram.poke(addr, val);
        let done = agnus.execute_incremental_blitter_op(op, read, write);
        if agnus.blitter_word_complete() && !done {
            agnus.advance_blitter_word();
        }
        ops += 1;
        if ops > 10_000 {
            panic!("blit runaway");
        }
    }
    ops
}

/// Program a simple one-word area blit with a given minterm LUT.
fn program_single_word_blit(
    agnus: &mut Agnus,
    lf: u8,
    use_a: bool,
    use_b: bool,
    use_c: bool,
    use_d: bool,
) {
    let useflags: u16 = (u16::from(use_a) << 11)
        | (u16::from(use_b) << 10)
        | (u16::from(use_c) << 9)
        | (u16::from(use_d) << 8);
    agnus.bltcon0 = useflags | u16::from(lf);
    agnus.bltcon1 = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltsize = (1 << 6) | 1; // 1 row × 1 word
}

// ────────────────────────────────────────────────────────────────
// #134 — Copy-mode fundamentals
// ────────────────────────────────────────────────────────────────

#[test]
fn copy_mode_d_equals_a_minterm_f0_writes_source_through_to_dest() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    ram.poke(0x1000, 0xABCD);
    agnus.blt_apt = 0x1000;
    agnus.blt_dpt = 0x2000;
    // Minterm $F0 = A (all rows where A=1 yield 1).
    program_single_word_blit(&mut agnus, 0xF0, true, false, false, true);
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    assert_eq!(ram.peek(0x2000), 0xABCD);
}

#[test]
fn copy_mode_d_equals_not_a_minterm_0f_inverts_source() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    ram.poke(0x1000, 0xABCD);
    agnus.blt_apt = 0x1000;
    agnus.blt_dpt = 0x2000;
    program_single_word_blit(&mut agnus, 0x0F, true, false, false, true);
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    assert_eq!(ram.peek(0x2000), !0xABCD);
}

#[test]
fn multi_row_copy_walks_apt_by_amod_on_each_row_end() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    // 3 words per row, 2 rows. Source $1000..: row 0 three words,
    // AMOD = 0, so row 1 starts at $1006 (= $1000 + 3*2).
    for i in 0..6u32 {
        ram.poke(0x1000 + i * 2, 0x1000 + i as u16);
    }
    agnus.blt_apt = 0x1000;
    agnus.blt_dpt = 0x2000;
    agnus.blt_amod = 0;
    agnus.blt_dmod = 0;
    agnus.bltcon0 = 0x0900 | 0xF0; // USEA + USED + minterm A
    agnus.bltcon1 = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltsize = (2 << 6) | 3;
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    for i in 0..6u32 {
        assert_eq!(ram.peek(0x2000 + i * 2), 0x1000 + i as u16);
    }
}

#[test]
fn first_word_mask_afwm_is_anded_with_the_first_a_word() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    ram.poke(0x1000, 0xFFFF);
    ram.poke(0x1002, 0xFFFF);
    agnus.blt_apt = 0x1000;
    agnus.blt_dpt = 0x2000;
    agnus.blt_afwm = 0xFF00; // mask the low byte out of the first word
    agnus.blt_alwm = 0xFFFF;
    agnus.bltcon0 = 0x0900 | 0xF0;
    agnus.bltcon1 = 0;
    agnus.bltsize = (1 << 6) | 2; // 2 words
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    assert_eq!(ram.peek(0x2000), 0xFF00, "AFWM masks the first word");
    assert_eq!(
        ram.peek(0x2002),
        0xFFFF,
        "AFWM does NOT mask subsequent words"
    );
}

#[test]
fn last_word_mask_alwm_is_anded_with_the_last_a_word() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    ram.poke(0x1000, 0xFFFF);
    ram.poke(0x1002, 0xFFFF);
    agnus.blt_apt = 0x1000;
    agnus.blt_dpt = 0x2000;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0x00FF;
    agnus.bltcon0 = 0x0900 | 0xF0;
    agnus.bltcon1 = 0;
    agnus.bltsize = (1 << 6) | 2;
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    assert_eq!(ram.peek(0x2000), 0xFFFF);
    assert_eq!(ram.peek(0x2002), 0x00FF, "ALWM masks only the last word");
}

// ────────────────────────────────────────────────────────────────
// #135 — Minterm LUT (all 256)
// ────────────────────────────────────────────────────────────────

fn run_minterm(lf: u8, a: u16, b: u16, c: u16) -> u16 {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    ram.poke(0x1000, a);
    ram.poke(0x2000, b);
    ram.poke(0x3000, c);
    agnus.blt_apt = 0x1000;
    agnus.blt_bpt = 0x2000;
    agnus.blt_cpt = 0x3000;
    agnus.blt_dpt = 0x4000;
    agnus.bltcon0 = 0x0F00 | u16::from(lf); // USEA+B+C+D
    agnus.bltcon1 = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltsize = (1 << 6) | 1;
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    ram.peek(0x4000)
}

#[test]
fn minterm_truth_table_covers_all_256_functions() {
    // For each 8-bit LUT, verify that every (a,b,c) input bit
    // produces the expected output bit. We pack a/b/c with 8 distinct
    // patterns in the low 8 bits (one per (a,b,c) row of the truth
    // table) so a single blit evaluates all 8 entries.
    //
    //   bit  a b c  index
    //     0  0 0 0    0
    //     1  0 0 1    1
    //     2  0 1 0    2
    //     3  0 1 1    3
    //     4  1 0 0    4
    //     5  1 0 1    5
    //     6  1 1 0    6
    //     7  1 1 1    7
    let a_mask = 0b11110000u16; // bits 4..=7 have a=1
    let b_mask = 0b11001100u16; // bits 2,3,6,7 have b=1
    let c_mask = 0b10101010u16; // bits 1,3,5,7 have c=1

    for lf in 0u16..=255 {
        let d = run_minterm(lf as u8, a_mask, b_mask, c_mask);
        for bit in 0..8 {
            let a = (a_mask >> bit) & 1;
            let b = (b_mask >> bit) & 1;
            let c = (c_mask >> bit) & 1;
            let index = (a << 2) | (b << 1) | c;
            let expected = (lf >> index) & 1;
            let got = (d >> bit) & 1;
            assert_eq!(
                got, expected,
                "lf={lf:02X} bit={bit} abc={a}{b}{c} index={index}: \
                 expected {expected}, got {got}"
            );
        }
    }
}

// ────────────────────────────────────────────────────────────────
// #136 — Shift + fill
// ────────────────────────────────────────────────────────────────

#[test]
fn a_shift_rotates_source_right_across_word_boundaries() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    // Two-word source $FFFF 0000 → with ASH=4, D should see ...FFFFF0
    // across the window boundary.
    ram.poke(0x1000, 0xFFFF);
    ram.poke(0x1002, 0x0000);
    agnus.blt_apt = 0x1000;
    agnus.blt_dpt = 0x2000;
    agnus.bltcon0 = (4 << 12) | 0x0900 | 0xF0; // ASH=4, USEA+D, minterm A
    agnus.bltcon1 = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltsize = (1 << 6) | 2;
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    // First word = $0FFF (top 4 bits are the pre-shift "previous" = 0).
    assert_eq!(ram.peek(0x2000), 0x0FFF);
    // Second word = $F000 (bottom 4 bits are first word's shifted-out).
    assert_eq!(ram.peek(0x2002), 0xF000);
}

#[test]
fn inclusive_fill_fills_from_first_one_through_just_before_next_one() {
    // IFE scans bit 0 upward, toggling a running carry on each 1-bit
    // then sampling AFTER the toggle. For input $8001 (bits 0 + 15):
    //   bit 0  : d=1, carry 0→1, out = 1
    //   bit 1..14: d=0, carry = 1, out = 1
    //   bit 15 : d=1, carry 1→0, out = 0
    // Output = $7FFF — the closing 1 is "consumed" by the toggle.
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    ram.poke(0x3000, 0x8001);
    agnus.blt_cpt = 0x3000;
    agnus.blt_dpt = 0x4000;
    agnus.bltcon0 = 0x0300 | 0xAA; // USEC+USED, minterm $AA (D = C)
    agnus.bltcon1 = 0x0008; // IFE
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltsize = (1 << 6) | 1;
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    assert_eq!(ram.peek(0x4000), 0x7FFF);
}

#[test]
fn exclusive_fill_inverts_inclusive_result_plus_one() {
    // EFE samples carry-XOR-d (the state BEFORE the toggle). For $8001:
    //   bit 0 : d=1, carry 0→1, out = 1^1 = 0
    //   bit 1..14: d=0, carry = 1, out = 1^0 = 1
    //   bit 15 : d=1, carry 1→0, out = 0^1 = 1
    // Output = $FFFE.
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    ram.poke(0x3000, 0x8001);
    agnus.blt_cpt = 0x3000;
    agnus.blt_dpt = 0x4000;
    agnus.bltcon0 = 0x0300 | 0xAA;
    agnus.bltcon1 = 0x0010; // EFE
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltsize = (1 << 6) | 1;
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    assert_eq!(ram.peek(0x4000), 0xFFFE);
}

// ────────────────────────────────────────────────────────────────
// #137 — Line mode
// ────────────────────────────────────────────────────────────────

#[test]
fn line_mode_draws_a_horizontal_line_as_pixels_into_d() {
    // A horizontal line of length 4, starting at pixel 0 of word at
    // $2000, should set 4 bits.
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    // BLTCON0 ash = starting pixel in word (bits 15-12).
    agnus.bltcon0 = 0x0B00 | 0xCA; // USEB+C+D, minterm $CA: standard line LF
    agnus.bltcon1 = 0x0001; // LINE mode
    agnus.blt_apt = 0; // Bresenham error
    agnus.blt_bdat = 0xFFFF; // texture: solid
    agnus.blt_cpt = 0x2000;
    agnus.blt_dpt = 0x2000;
    agnus.blt_amod = 0;
    agnus.blt_bmod = 4;
    agnus.blt_cmod = 0; // row modulo 0 for horizontal
    agnus.bltsize = (4 << 6) | 2; // 4 line steps
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    // First four pixels of $2000 set.
    let out = ram.peek(0x2000);
    assert_eq!(
        out & 0xF000,
        0xF000,
        "4 leftmost pixels plotted; got ${out:04X}"
    );
}

// ────────────────────────────────────────────────────────────────
// #138 — DMA slot scheduler
// ────────────────────────────────────────────────────────────────

#[test]
fn scheduler_total_ops_matches_enabled_channel_count() {
    let cases = [
        (0x0000, 0), // no channels → still internal
        (0x0100, 1), // D only
        (0x0900, 2), // A + D
        (0x0D00, 3), // A + B + D
        (0x0F00, 4), // A + B + C + D
    ];
    for (useflags, expected_ops) in cases {
        let mut agnus = Agnus::new();
        agnus.bltcon0 = useflags;
        agnus.bltsize = (1 << 6) | 1;
        agnus.start_blit();
        let ops = agnus.blitter_ccks_remaining;
        // Internal-only (useflags == 0) → 1 internal cycle per word.
        let expected = if expected_ops == 0 { 1 } else { expected_ops };
        assert_eq!(
            ops, expected,
            "useflags ${useflags:04X}: expected {expected} ops, got {ops}"
        );
    }
}

#[test]
fn scheduler_halts_when_bus_grant_is_withheld() {
    let mut agnus = Agnus::new();
    agnus.bltcon0 = 0x0100; // D only
    agnus.bltsize = (1 << 6) | 2;
    agnus.start_blit();
    let before = agnus.blitter_ccks_remaining;
    // Tick with progress disabled — should not decrement.
    for _ in 0..10 {
        assert!(!agnus.tick_blitter_scheduler(false));
    }
    assert_eq!(agnus.blitter_ccks_remaining, before);
    // Tick with progress — counts down.
    agnus.tick_blitter_scheduler(true);
    assert_eq!(agnus.blitter_ccks_remaining, before - 1);
}

#[test]
fn blitter_nasty_mode_requires_busy_blten_bltpri_all_set() {
    use commodore_agnus_ocs::bits::{DMACON_BLTEN, DMACON_BLTPRI, DMACON_DMAEN};
    let mut agnus = Agnus::new();
    agnus.blitter_busy = true;
    agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN | DMACON_BLTPRI;
    assert!(agnus.blitter_nasty_active());

    agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN; // missing BLTPRI
    assert!(!agnus.blitter_nasty_active());

    agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN | DMACON_BLTPRI;
    agnus.blitter_busy = false;
    assert!(!agnus.blitter_nasty_active());
}
