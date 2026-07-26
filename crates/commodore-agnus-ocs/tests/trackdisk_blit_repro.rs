//! Reproduce the exact blitter setup KS 1.3 trackdisk uses for sector
//! decoding.
//!
//! trackdisk's $FEAE94 routine programs:
//!   BLTCON0 = $05CC   (USEB + USED, minterm $CC = D ← B)
//!   BLTCON1 = shift << 12   (B-channel right-shift; 0 for byte-aligned sync)
//!   BLTBPT  = source pointer
//!   BLTDPT  = destination pointer
//!   BLTBMOD = 0
//!   BLTDMOD = 0
//!   BLTSIZE = ((count + $3F) & $FFC0) | $20   (computed from byte count)
//!
//! For a 1088-byte copy (one sector decode), BLTSIZE = $460 = 17 lines
//! × 32 words. For the 10880-byte copy (sectors 0..9 in one shot),
//! BLTSIZE = $2AA0 = 170 lines × 32 words.
//!
//! If our blitter handles these correctly, the destination should be a
//! byte-for-byte copy of the source.

use std::cell::RefCell;
use std::collections::HashMap;

use commodore_agnus_ocs::{Agnus, BlitterProgress};

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

fn run_blit(agnus: &mut Agnus, ram: &TestRam) {
    let mut ops = 0u32;
    while agnus.next_blitter_dma_request().is_some() {
        let op = match agnus.tick_blitter_scheduler_op(true) {
            BlitterProgress::Startup => continue,
            BlitterProgress::Operation(op) => op,
            BlitterProgress::NoProgress => break,
        };
        let read = |addr: u32| ram.peek(addr);
        let write = |addr: u32, val: u16| ram.poke(addr, val);
        let _done = agnus.execute_incremental_blitter_op(op, read, write);
        if agnus.blitter_word_complete() && !_done {
            agnus.advance_blitter_word();
        }
        ops += 1;
        if ops > 10_000_000 {
            panic!("blit runaway after {ops} ops");
        }
    }
}

#[test]
fn trackdisk_one_sector_b_to_d_copy_succeeds() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();

    // Fill the source with a pattern that's easy to spot if the copy
    // is dropped. 17×32 = 544 words.
    let src_base = 0x1000_0000u32;
    let dst_base = 0x2000_0000u32;
    for i in 0..544u32 {
        ram.poke(src_base + i * 2, 0xC000 | (i as u16));
    }

    agnus.blt_bpt = src_base;
    agnus.blt_dpt = dst_base;
    agnus.blt_bmod = 0;
    agnus.blt_dmod = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltcon0 = 0x05CC;
    agnus.bltcon1 = 0;
    agnus.bltsize = 0x0460; // 17 lines × 32 words = 1088 bytes

    agnus.start_blit();
    run_blit(&mut agnus, &ram);

    let mut mismatches = 0u32;
    let mut first_diff = None;
    for i in 0..544u32 {
        let want = 0xC000 | (i as u16);
        let got = ram.peek(dst_base + i * 2);
        if got != want {
            mismatches += 1;
            if first_diff.is_none() {
                first_diff = Some((i, want, got));
            }
        }
    }
    assert_eq!(
        mismatches, 0,
        "1088-byte B→D copy: {mismatches}/544 mismatches, first {first_diff:?}"
    );
}

/// Reproduce the EXACT overlapping blit from trackdisk at cck 15814613:
/// bpt=$215C, dpt=$2060, size=$2AA0 (10880 bytes). src > dst, forward
/// copy. This is the pattern that decode_buf uses — copy DMA data
/// with -4 byte shift to align gap+sync at slot boundaries.
#[test]
fn trackdisk_overlapping_src_gt_dst_copy_preserves_data() {
    let mut agnus = Agnus::new();
    let ram = TestRam::new();

    // Simulate 64KB of chip RAM. Populate the source range with a
    // known-good MFM-like pattern (unique per word).
    let src_base = 0x0000_215Cu32;
    let dst_base = 0x0000_2060u32;
    // BLTSIZE format: 170 lines × 32 words = 10880 bytes.
    let bltsize: u16 = 0x2AA0;
    let size_words: u32 = (170 * 32) as u32;
    for i in 0..size_words {
        ram.poke(src_base + i * 2, 0xA000 | (i as u16));
    }

    agnus.blt_bpt = src_base;
    agnus.blt_dpt = dst_base;
    agnus.blt_bmod = 0;
    agnus.blt_dmod = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltcon0 = 0x05CC;
    agnus.bltcon1 = 0;
    agnus.bltsize = bltsize;

    agnus.start_blit();
    run_blit(&mut agnus, &ram);

    // Each byte written to dst should match the original source.
    // After the blit, dst[i] == original_src[i].
    let mut mismatches = 0u32;
    let mut first_diff = None;
    for i in 0..size_words {
        let want = 0xA000 | (i as u16);
        let got = ram.peek(dst_base + i * 2);
        if got != want {
            mismatches += 1;
            if first_diff.is_none() {
                first_diff = Some((i, want, got));
            }
        }
    }
    assert_eq!(
        mismatches, 0,
        "overlapping src>dst blit: {mismatches}/{size_words} mismatches, first {first_diff:?}"
    );
}

#[test]
fn trackdisk_ten_sector_b_to_d_copy_succeeds() {
    // The big one: 10880 bytes = 170 lines × 32 words.
    let mut agnus = Agnus::new();
    let ram = TestRam::new();

    let src_base = 0x1000_0000u32;
    let dst_base = 0x2000_0000u32;
    let words = 170u32 * 32; // 5440
    for i in 0..words {
        ram.poke(src_base + i * 2, 0xA000 | (i as u16));
    }

    agnus.blt_bpt = src_base;
    agnus.blt_dpt = dst_base;
    agnus.blt_bmod = 0;
    agnus.blt_dmod = 0;
    agnus.blt_afwm = 0xFFFF;
    agnus.blt_alwm = 0xFFFF;
    agnus.bltcon0 = 0x05CC;
    agnus.bltcon1 = 0;
    agnus.bltsize = 0x2AA0; // 170 lines × 32 words

    agnus.start_blit();
    run_blit(&mut agnus, &ram);

    let mut mismatches = 0u32;
    let mut first_diff = None;
    for i in 0..words {
        let want = 0xA000 | (i as u16);
        let got = ram.peek(dst_base + i * 2);
        if got != want {
            mismatches += 1;
            if first_diff.is_none() {
                first_diff = Some((i, want, got));
            }
        }
    }
    assert_eq!(
        mismatches, 0,
        "10880-byte B→D copy: {mismatches}/{words} mismatches, first {first_diff:?}"
    );
}
