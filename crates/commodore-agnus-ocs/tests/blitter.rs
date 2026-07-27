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
//!   BLTCON1 bits 4-2    area-fill controls, or line octant controls
//!   BLTCON1 bit   1     descending in area mode, ONEDOT in line mode
//!   BLTCON1 bit   0     line mode
//!
//!   BLTSIZE: height (bits 15-6) = rows, width (bits 5-0) = words/row.
//!
//! The minterm LUT covers all 256 combinations of (A,B,C) → D. Test
//! #135 exercises every index by setting specific bits in the LUT.

use std::cell::RefCell;
use std::collections::HashMap;

use commodore_agnus_ocs::{Agnus, BlitterCckOutcome, BlitterDmaOp, BlitterProgress};

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

/// Drive only the request/execution engine from `start_blit` to its last
/// operation. This is an algorithm oracle; it deliberately does not model
/// physical completion observers. Returns the number of operations executed.
fn run_blit(agnus: &mut Agnus, ram: &TestRam) -> u32 {
    let mut ops = 0u32;
    while agnus.next_blitter_dma_request().is_some() {
        let op = match agnus.tick_blitter_scheduler_op(true) {
            BlitterProgress::Startup => continue,
            BlitterProgress::Operation(op) => op,
            BlitterProgress::NoProgress => break,
        };
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
    agnus.bltcon0 = 0x0B00 | 0xCA; // USEA+C+D, minterm $CA: standard line LF
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

fn program_x_major_line(agnus: &mut Agnus, steps: u16, one_dot: bool, texture: u16) {
    // Octant 0 is X-major, +X/+Y in the current line decode. A negative,
    // unchanged error keeps every step on the same horizontal row.
    agnus.bltcon0 = 0x0B00 | 0xCA; // USEA+C+D, standard line minterm
    agnus.bltcon1 = 0x0019 | if one_dot { 0x0002 } else { 0 };
    agnus.blt_apt = 0x0000_FFFF; // signed error = -1
    agnus.blt_bdat = texture;
    agnus.blt_cpt = 0x2000;
    agnus.blt_dpt = 0x2000;
    agnus.blt_amod = 0;
    agnus.blt_bmod = 0;
    agnus.blt_cmod = -2;
    agnus.bltsize = (steps << 6) | 2;
}

#[test]
fn line_mode_onedot_writes_only_the_first_pixel_in_a_horizontal_row() {
    let mut normal = Agnus::new();
    let normal_ram = TestRam::new();
    program_x_major_line(&mut normal, 4, false, 0xFFFF);
    normal.start_blit();
    run_blit(&mut normal, &normal_ram);
    assert_eq!(
        normal_ram.peek(0x2000) & 0xF000,
        0xF000,
        "the non-ONEDOT control must write every generated pixel",
    );

    let mut one_dot = Agnus::new();
    let one_dot_ram = TestRam::new();
    program_x_major_line(&mut one_dot, 4, true, 0xFFFF);
    one_dot.start_blit();
    run_blit(&mut one_dot, &one_dot_ram);
    assert_eq!(
        one_dot_ram.peek(0x2000),
        0x8000,
        "ONEDOT must suppress the complete later D transfers in the row",
    );
}

#[test]
fn line_mode_uses_preloaded_b_texture_without_b_dma() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    // Standard line setup leaves SRCB disabled. BSH=0 selects texture bit
    // 0 first, then wraps to bit 15 and continues downward.
    program_x_major_line(&mut agnus, 4, false, 0x8001);
    agnus.start_blit();

    run_blit(&mut agnus, &ram);

    assert_eq!(
        ram.peek(0x2000) & 0xF000,
        0xC000,
        "texture bits 0,15,14,13 must gate the four generated pixels",
    );
    assert_eq!(
        agnus.blt_bdat, 0x8001,
        "the line texture rotates through internal phase, not BLTBDAT",
    );
    assert_eq!(
        agnus.bltcon1 >> 12,
        12,
        "four pixels must decrement the visible B shift from 0 to 12",
    );
}

#[test]
fn line_mode_onedot_rearms_after_each_vertical_step() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    agnus.bltcon0 = 0x0B00 | 0xCA; // USEA+C+D, standard line minterm
    agnus.bltcon1 = 0x0007; // LINE | ONEDOT, Y-major +X/+Y octant
    agnus.blt_apt = 0x0000_FFFF;
    agnus.blt_bdat = 0xFFFF;
    agnus.blt_cpt = 0x2000;
    agnus.blt_dpt = 0x2000;
    agnus.blt_amod = 0;
    agnus.blt_bmod = 0;
    agnus.blt_cmod = -2; // each Y step advances one word in test RAM
    agnus.bltsize = (3 << 6) | 2;
    agnus.start_blit();

    run_blit(&mut agnus, &ram);

    assert_eq!(ram.peek(0x2000), 0x8000);
    assert_eq!(ram.peek(0x2002), 0x8000);
    assert_eq!(ram.peek(0x2004), 0x8000);
}

#[test]
fn suppressed_onedot_d_updates_bzero_finishes_and_leaves_the_bus_free() {
    use commodore_agnus_ocs::SlotOwner;
    use commodore_agnus_ocs::bits::{DMACON_BLTEN, DMACON_BLTPRI, DMACON_DMAEN};

    for agnus_id in [0x1000, 0x2300] {
        let mut agnus = Agnus::new();
        let ram = TestRam::new();
        // BSH=0 selects bit 0 first; the line shifter then wraps to bit
        // 15, so this pattern produces zero followed by a non-zero pixel.
        program_x_major_line(&mut agnus, 2, true, 0x8000);
        agnus.agnus_id = agnus_id;
        agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN | DMACON_BLTPRI;
        agnus.hpos = 0x35; // a CPU/free cell
        agnus.start_blit();
        let mut bus = RamBus(&ram);

        // Two startup CCKs, first C/D, then the second C read. The first
        // texture bit is clear, so the permitted D transfer writes zero.
        assert_eq!(
            agnus.tick_blitter_cck(true, &mut bus),
            BlitterCckOutcome::default(),
        );
        assert_eq!(
            agnus.tick_blitter_cck(true, &mut bus),
            BlitterCckOutcome::default(),
        );
        assert_eq!(
            agnus.tick_blitter_cck(true, &mut bus),
            BlitterCckOutcome {
                interrupt: false,
                bus_used: true,
            },
        );
        assert_eq!(
            agnus.tick_blitter_cck(true, &mut bus),
            BlitterCckOutcome {
                interrupt: false,
                bus_used: true,
            },
        );
        assert_eq!(
            agnus.tick_blitter_cck(true, &mut bus),
            BlitterCckOutcome {
                interrupt: false,
                bus_used: true,
            },
        );
        assert!(agnus.blitter_dzero);

        // The rotated texture makes the final generated result non-zero,
        // but ONEDOT suppresses its complete D transfer. Arbitration can
        // see that before the logical WriteD retires.
        let plan = agnus.cck_bus_plan();
        assert_eq!(plan.slot_owner, SlotOwner::Cpu);
        assert!(plan.blitter_dma_progress_granted);
        assert!(!plan.blitter_chip_bus_granted);
        assert!(plan.cpu_chip_bus_granted);
        assert!(!agnus.blitter_nasty_active());

        let outcome = agnus.tick_blitter_cck(true, &mut bus);
        assert_eq!(
            outcome,
            BlitterCckOutcome {
                interrupt: true,
                bus_used: false,
            },
            "revision ${agnus_id:04X} must finish on the bus-free would-be D CCK",
        );
        assert_eq!(ram.peek(0x2000), 0);
        assert!(
            !agnus.blitter_dzero,
            "the suppressed non-zero result must still clear BZERO",
        );
        assert!(!agnus.blitter_busy);
        assert!(agnus.blitter_busy_visible());
        assert!(agnus.blitter_busy_copper());

        agnus.tick_cck();
        assert!(!agnus.blitter_busy_visible());
        assert!(agnus.blitter_busy_copper());
        agnus.tick_cck();
        assert!(!agnus.blitter_busy_copper());
    }
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
        assert_eq!(
            agnus.tick_blitter_scheduler_op(false),
            BlitterProgress::NoProgress,
        );
    }
    assert_eq!(agnus.blitter_ccks_remaining, before);
    // The first two accepted CCKs drain shared startup without consuming
    // either D operation.
    assert_eq!(
        agnus.tick_blitter_scheduler_op(true),
        BlitterProgress::Startup,
    );
    assert_eq!(
        agnus.tick_blitter_scheduler_op(true),
        BlitterProgress::Startup,
    );
    assert_eq!(agnus.blitter_ccks_remaining, before);

    // The third accepted CCK services the first real operation.
    assert_eq!(
        agnus.tick_blitter_scheduler_op(true),
        BlitterProgress::Operation(BlitterDmaOp::WriteD),
    );
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

// ────────────────────────────────────────────────────────────────
// Incremental drain (#31) — byte-for-byte parity with the transaction-level
// synchronous path. `tick_blitter_cck` must produce the same chip RAM as
// `run_blit_to_completion`.
// ────────────────────────────────────────────────────────────────

/// `BlitterBus` view over the test RAM (interior-mutable, so `&TestRam`
/// is enough). Lets us drive the public `tick_blitter_cck` exactly as
/// the machine tick loop does.
struct RamBus<'a>(&'a TestRam);
impl commodore_agnus_ocs::BlitterBus for RamBus<'_> {
    fn read_word(&mut self, addr: u32) -> u16 {
        self.0.peek(addr)
    }
    fn write_word(&mut self, addr: u32, val: u16) {
        self.0.poke(addr, val);
    }
}

/// Drive a started blit one CCK per call via the live path.
///
/// Returns the channel-operation count; asserts that startup plus any
/// final-D tail consume the exact extra CCKs, the finish source fires once,
/// and internal activity drains.
fn run_blit_incremental(agnus: &mut Agnus, ram: &TestRam) -> u32 {
    let mut bus = RamBus(ram);
    let operation_count = agnus.blitter_ccks_remaining;
    let has_area_final_d = agnus.bltcon1 & 1 == 0 && agnus.bltcon0 & 0x0100 != 0;
    let mut accepted_ccks = 0u32;
    let mut interrupts = 0u32;
    while agnus.blitter_busy {
        let outcome = agnus.tick_blitter_cck(true, &mut bus);
        accepted_ccks += 1;
        interrupts += u32::from(outcome.interrupt);
        if accepted_ccks > 10_000 {
            panic!("incremental blit runaway");
        }
    }
    assert_eq!(
        accepted_ccks,
        operation_count + 2 + if has_area_final_d { 2 } else { 0 },
        "incremental drain must include startup and the bounded final-D tail",
    );
    assert_eq!(interrupts, 1, "finish source must fire exactly once");
    assert!(
        !agnus.blitter_busy,
        "blitter must clear when the incremental blit completes"
    );
    operation_count
}

fn tick_live(agnus: &mut Agnus, ram: &TestRam, progress_granted: bool) -> BlitterCckOutcome {
    agnus.tick_cck();
    let mut bus = RamBus(ram);
    agnus.tick_blitter_cck(progress_granted, &mut bus)
}

/// Run `setup` through both the synchronous and the incremental paths
/// into independent RAMs, then assert every word in the working range
/// agrees and the op counts match.
fn assert_paths_agree(setup: impl Fn(&mut Agnus, &TestRam), label: &str) {
    let mut a_sync = Agnus::new();
    let ram_sync = TestRam::new();
    setup(&mut a_sync, &ram_sync);
    a_sync.start_blit();
    let sync_ops = a_sync.blitter_ccks_remaining;
    let mut sync_bus = RamBus(&ram_sync);
    assert!(
        a_sync.run_blit_to_completion(&mut sync_bus),
        "{label}: synchronous drain missed the finish source",
    );

    let mut a_inc = Agnus::new();
    let ram_inc = TestRam::new();
    setup(&mut a_inc, &ram_inc);
    a_inc.start_blit();
    let inc_ops = run_blit_incremental(&mut a_inc, &ram_inc);

    assert_eq!(sync_ops, inc_ops, "{label}: op count differs");
    for addr in (0x0000u32..0x6000).step_by(2) {
        assert_eq!(
            ram_sync.peek(addr),
            ram_inc.peek(addr),
            "{label}: word ${addr:05X} differs (sync vs incremental)"
        );
    }
}

#[test]
fn large_blit_size_beyond_legacy_fields_does_not_wrap() {
    // Regression for #36. The ECS large-blit path drives the engine via
    // `start_blit_with_size` with the full 15-bit height / 11-bit width.
    // A width past the legacy 6-bit field (>63 words) or a height past
    // the legacy 10-bit field (>1023 lines) must NOT wrap.

    // 100-word-wide D-only fill (minterm $FF → D := all ones).
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    agnus.blt_dpt = 0x2000;
    agnus.bltcon0 = 0x0100 | 0xFF; // USED + minterm $FF
    agnus.bltcon1 = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.blt_dmod = 0;
    agnus.start_blit_with_size(1, 100); // 1 row × 100 words (> 63)
    run_blit(&mut agnus, &ram);
    assert_eq!(ram.peek(0x2000 + 63 * 2), 0xFFFF, "word 63 written");
    assert_eq!(
        ram.peek(0x2000 + 64 * 2),
        0xFFFF,
        "word 64 written — the legacy 6-bit width would have wrapped this away"
    );
    assert_eq!(ram.peek(0x2000 + 99 * 2), 0xFFFF, "word 99 written");
    assert_eq!(ram.peek(0x2000 + 100 * 2), 0x0000, "word 100 not written");

    // 1100-line-tall D-only fill (height > the legacy 1023-line field).
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    agnus.blt_dpt = 0x4000;
    agnus.bltcon0 = 0x0100 | 0xFF;
    agnus.bltcon1 = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.blt_dmod = 0;
    agnus.start_blit_with_size(1100, 1); // 1100 rows × 1 word (> 1023)
    run_blit(&mut agnus, &ram);
    // One word per row, dmod 0 → contiguous; row 1099 lands at +1099 words.
    assert_eq!(ram.peek(0x4000 + 1023 * 2), 0xFFFF, "row 1023 written");
    assert_eq!(
        ram.peek(0x4000 + 1099 * 2),
        0xFFFF,
        "row 1099 written — the legacy 10-bit height would have wrapped this away"
    );
    assert_eq!(ram.peek(0x4000 + 1100 * 2), 0x0000, "row 1100 not written");
}

#[test]
fn bzero_tracks_whether_all_d_words_were_zero() {
    // Non-zero D result → BZERO clear.
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    ram.poke(0x1000, 0xABCD);
    agnus.blt_apt = 0x1000;
    agnus.blt_dpt = 0x2000;
    program_single_word_blit(&mut agnus, 0xF0, true, false, false, true); // D = A
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    assert!(
        !agnus.blitter_dzero,
        "a non-zero D word must clear BZERO (DMACONR bit 13)"
    );
    assert_eq!(agnus.dmaconr() & 0x2000, 0, "DMACONR BZERO clear");

    // All-zero D result → BZERO set.
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    ram.poke(0x1000, 0x0000);
    agnus.blt_apt = 0x1000;
    agnus.blt_dpt = 0x2000;
    program_single_word_blit(&mut agnus, 0xF0, true, false, false, true);
    agnus.start_blit();
    run_blit(&mut agnus, &ram);
    assert!(
        agnus.blitter_dzero,
        "an all-zero D result must set BZERO (the collision-test signal)"
    );
    assert_ne!(agnus.dmaconr() & 0x2000, 0, "DMACONR BZERO set");
}

#[test]
fn pre_aga_area_final_d_orders_finish_result_and_write() {
    use commodore_agnus_ocs::bits::{DMACON_BLTEN, DMACON_BLTPRI, DMACON_DMAEN};

    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN | DMACON_BLTPRI;
    agnus.blt_dpt = 0x2000;
    program_single_word_blit(&mut agnus, 0xFF, false, false, false, true);
    agnus.start_blit();

    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default()
    );
    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default()
    );

    let finish = tick_live(&mut agnus, &ram, true);
    assert_eq!(
        finish,
        BlitterCckOutcome {
            interrupt: true,
            bus_used: false,
        }
    );
    assert!(agnus.blitter_busy, "the final-D pipeline remains active");
    assert!(
        !agnus.blitter_nasty_active(),
        "pre-AGA nasty ownership ends at main finish",
    );
    assert!(agnus.blitter_busy_visible(), "DMACONR holds busy through F");
    assert!(agnus.blitter_busy_copper(), "Copper holds busy through F");
    assert_eq!(agnus.blitter_completion_phase(), "final-result");
    assert_eq!(agnus.blitter_completion_ccks_remaining(), 2);
    assert!(agnus.blitter_final_d_pending());
    assert!(
        agnus.blitter_dzero,
        "the final result is not generated at F"
    );
    assert_eq!(ram.peek(0x2000), 0);

    let result = tick_live(&mut agnus, &ram, false);
    assert_eq!(result, BlitterCckOutcome::default());
    assert!(agnus.blitter_busy);
    assert!(!agnus.blitter_nasty_active());
    assert!(!agnus.blitter_busy_visible(), "DMACONR releases at F+1");
    assert!(agnus.blitter_busy_copper(), "Copper remains busy at F+1");
    assert_eq!(agnus.blitter_completion_phase(), "final-write");
    assert_eq!(agnus.blitter_completion_ccks_remaining(), 1);
    assert!(!agnus.blitter_dzero, "BZERO settles with the result at F+1");
    assert_eq!(ram.peek(0x2000), 0, "final D has not reached memory");

    let blocked_write = tick_live(&mut agnus, &ram, false);
    assert_eq!(blocked_write, BlitterCckOutcome::default());
    assert!(
        agnus.blitter_busy,
        "the final D waits for an admitted bus slot"
    );
    assert!(!agnus.blitter_nasty_active());
    assert!(!agnus.blitter_busy_visible());
    assert!(
        !agnus.blitter_busy_copper(),
        "the observer hold expires independently of final-D contention"
    );
    assert_eq!(agnus.blitter_completion_phase(), "final-write");
    assert_eq!(agnus.blitter_completion_ccks_remaining(), 1);
    assert_eq!(ram.peek(0x2000), 0, "a denied slot cannot write final D");

    let write = tick_live(&mut agnus, &ram, true);
    assert_eq!(
        write,
        BlitterCckOutcome {
            interrupt: false,
            bus_used: true,
        }
    );
    assert!(!agnus.blitter_busy);
    assert!(!agnus.blitter_busy_visible());
    assert!(!agnus.blitter_busy_copper());
    assert_eq!(agnus.blitter_completion_phase(), "idle");
    assert_eq!(agnus.blitter_completion_ccks_remaining(), 0);
    assert!(!agnus.blitter_final_d_pending());
    assert_eq!(ram.peek(0x2000), 0xFFFF);

    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default(),
        "finish and final D must remain one-shot",
    );
}

#[test]
fn alice_area_completion_waits_for_final_d() {
    use commodore_agnus_ocs::bits::{DMACON_BLTEN, DMACON_BLTPRI, DMACON_DMAEN};

    let mut agnus = Agnus::new();
    agnus.agnus_id = 0x2300; // PAL Alice identity
    agnus.dmacon = DMACON_DMAEN | DMACON_BLTEN | DMACON_BLTPRI;
    let ram = TestRam::new();
    agnus.blt_dpt = 0x2000;
    program_single_word_blit(&mut agnus, 0xFF, false, false, false, true);
    agnus.start_blit();

    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default()
    );
    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default()
    );

    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default(),
        "Alice must not emit completion at the pre-AGA F boundary",
    );
    assert!(
        !agnus.blitter_nasty_active(),
        "Alice's internal completion tail must not own the chip bus",
    );
    assert_eq!(agnus.blitter_completion_ccks_remaining(), 2);
    assert!(agnus.blitter_busy_visible());
    assert!(agnus.blitter_busy_copper());

    assert_eq!(
        tick_live(&mut agnus, &ram, false),
        BlitterCckOutcome::default(),
    );
    assert_eq!(agnus.blitter_completion_ccks_remaining(), 1);
    assert!(!agnus.blitter_dzero);
    assert_eq!(ram.peek(0x2000), 0);
    assert!(agnus.blitter_busy_visible());
    assert!(agnus.blitter_busy_copper());

    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome {
            interrupt: true,
            bus_used: true,
        },
    );
    assert!(!agnus.blitter_busy);
    assert_eq!(ram.peek(0x2000), 0xFFFF);
    assert!(
        agnus.blitter_busy_visible(),
        "Alice DMACONR holds busy through the final-D finish CCK",
    );
    assert!(agnus.blitter_busy_copper());

    assert_eq!(
        tick_live(&mut agnus, &ram, false),
        BlitterCckOutcome::default(),
    );
    assert!(!agnus.blitter_busy_visible());
    assert!(agnus.blitter_busy_copper());
    assert_eq!(
        tick_live(&mut agnus, &ram, false),
        BlitterCckOutcome::default(),
    );
    assert!(!agnus.blitter_busy_copper());
}

#[test]
fn area_without_d_updates_bzero_and_finishes_on_final_op() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    program_single_word_blit(&mut agnus, 0xFF, false, false, false, false);
    agnus.start_blit();

    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default()
    );
    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default()
    );
    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome {
            interrupt: true,
            bus_used: false,
        },
    );
    assert!(!agnus.blitter_busy);
    assert!(
        !agnus.blitter_dzero,
        "generated non-zero D must clear BZERO"
    );
    assert!(!agnus.blitter_final_d_pending());
    assert!(agnus.blitter_busy_visible());
    assert!(agnus.blitter_busy_copper());
}

#[test]
fn line_mode_finishes_with_its_final_d_write() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();
    agnus.bltcon0 = 0x0B00 | 0xCA; // USEA+C+D, standard line minterm
    agnus.bltcon1 = 0x0001; // LINE
    agnus.blt_apt = 0;
    agnus.blt_bdat = 0xFFFF;
    agnus.blt_cpt = 0x2000;
    agnus.blt_dpt = 0x2000;
    agnus.blt_amod = 0;
    agnus.blt_bmod = 4;
    agnus.blt_cmod = 0;
    agnus.bltsize = (1 << 6) | 2;
    agnus.start_blit();

    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default()
    );
    assert_eq!(
        tick_live(&mut agnus, &ram, true),
        BlitterCckOutcome::default()
    );
    assert!(tick_live(&mut agnus, &ram, true).bus_used); // ReadC
    let finish = tick_live(&mut agnus, &ram, true);
    assert_eq!(
        finish,
        BlitterCckOutcome {
            interrupt: true,
            bus_used: true,
        },
    );
    assert!(!agnus.blitter_busy);
    assert!(!agnus.blitter_final_d_pending());
    assert_ne!(ram.peek(0x2000), 0, "line result lands with completion");
}

#[test]
fn incremental_drain_matches_synchronous_blit() {
    // Area copy, A -> D, minterm $F0.
    assert_paths_agree(
        |agnus, ram| {
            ram.poke(0x1000, 0xABCD);
            agnus.blt_apt = 0x1000;
            agnus.blt_dpt = 0x2000;
            program_single_word_blit(agnus, 0xF0, true, false, false, true);
        },
        "area copy A->D",
    );

    // Multi-row 3x2 area copy (exercises row-end modulo + word advance).
    assert_paths_agree(
        |agnus, ram| {
            for i in 0..6u32 {
                ram.poke(0x1000 + i * 2, 0x1000 + i as u16);
            }
            agnus.blt_apt = 0x1000;
            agnus.blt_dpt = 0x2000;
            agnus.blt_amod = 0;
            agnus.blt_dmod = 0;
            agnus.bltcon0 = 0x0900 | 0xF0; // USEA + USED + minterm A
            agnus.blt_afwm = 0xFFFF;
            agnus.blt_alwm = 0xFFFF;
            agnus.bltsize = (2 << 6) | 3;
        },
        "multi-row 3x2 copy",
    );

    // Full A/B/C minterm (a mux) into D — exercises all three reads.
    assert_paths_agree(
        |agnus, ram| {
            ram.poke(0x1000, 0xF0F0);
            ram.poke(0x2000, 0xCCCC);
            ram.poke(0x3000, 0xAAAA);
            agnus.blt_apt = 0x1000;
            agnus.blt_bpt = 0x2000;
            agnus.blt_cpt = 0x3000;
            agnus.blt_dpt = 0x4000;
            agnus.bltcon0 = 0x0F00 | 0xCA; // USEA+B+C+D, minterm $CA (mux)
            agnus.blt_afwm = 0xFFFF;
            agnus.blt_alwm = 0xFFFF;
            agnus.bltsize = (1 << 6) | 1;
        },
        "abc minterm mux",
    );

    // Inclusive-fill (channel-C area blit through the fill unit).
    assert_paths_agree(
        |agnus, ram| {
            ram.poke(0x3000, 0x8001);
            agnus.blt_cpt = 0x3000;
            agnus.blt_dpt = 0x4000;
            agnus.bltcon0 = 0x0300 | 0xAA;
            agnus.bltcon1 = 0x0010; // EFE
            agnus.blt_afwm = 0xFFFF;
            agnus.blt_alwm = 0xFFFF;
            agnus.bltsize = (1 << 6) | 1;
        },
        "exclusive-fill",
    );

    // Line mode (the ReadC -> WriteD per-step path).
    assert_paths_agree(
        |agnus, _ram| {
            agnus.bltcon0 = 0x0B00 | 0xCA; // USEA+C+D, standard line LF
            agnus.bltcon1 = 0x0001; // LINE mode
            agnus.blt_apt = 0;
            agnus.blt_bdat = 0xFFFF;
            agnus.blt_cpt = 0x2000;
            agnus.blt_dpt = 0x2000;
            agnus.blt_amod = 0;
            agnus.blt_bmod = 4;
            agnus.blt_cmod = 0;
            agnus.bltsize = (4 << 6) | 2;
        },
        "line mode horizontal",
    );
}
