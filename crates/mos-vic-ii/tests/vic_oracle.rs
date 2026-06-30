//! VIC-II per-cycle oracle harness (Increment 1 of the VC/VCBASE/RC rewrite).
//!
//! Drives the real `Vic` engine across known PAL raster lines, records its
//! per-cycle BA state and memory accesses through a recording `VicMemory`, and
//! compares them against the canonical schedule in `mos_vic_ii::oracle`.
//!
//! - **Live tests** lock the timing facts the engine already gets right (the
//!   Seam 1 BA audit, re-proved here through the oracle).
//! - **`#[ignore]`d tests** carry the *target* behaviour for the rewrite: they
//!   assert the per-cycle fetch distribution the VC/VCBASE/RC streaming must
//!   produce, and currently fail because the engine batches its fetches. They
//!   flip green at Increment 3.
//!
//! Plan: `docs/plans/2026-06-30-c64-vic-ii-vc-vcbase-rc-rewrite.md`.

use std::cell::RefCell;

use mos_vic_ii::oracle::{self, canonical_for_engine_cycle, expected_ba_low};
use mos_vic_ii::{Vic, VicMemory, VicModel};

const CYCLES_PER_LINE: u32 = 63;
const LINES_PER_FRAME: u32 = 312;

// $D018 = 0x18 → video matrix at $0400-$07FF, char base at $2000.
const REG_D018: u8 = 0x18;
const SCREEN_LO: u16 = 0x0400;
const SCREEN_HI: u16 = 0x0800;

// A badline (line & 7 == YSCROLL=0, inside the display window) that is also
// inside the RSEL=1 character render window ($33..$FB), so both the c-access
// and the g-accesses fire. Line 0x39 (the row after) is a non-badline in the
// same window — g-accesses only, no matrix fetch, no fetch BA.
const BADLINE: u16 = 0x38;
const NON_BADLINE: u16 = 0x39;

/// `VicMemory` that records every access with no side effects (returns 0).
struct RecordingMemory {
    vram: RefCell<Vec<u16>>,
    colour: RefCell<Vec<u16>>,
}

impl RecordingMemory {
    fn new() -> Self {
        Self {
            vram: RefCell::new(Vec::new()),
            colour: RefCell::new(Vec::new()),
        }
    }

    fn clear(&self) {
        self.vram.borrow_mut().clear();
        self.colour.borrow_mut().clear();
    }
}

impl VicMemory for RecordingMemory {
    fn read_vram(&self, addr: u16) -> u8 {
        self.vram.borrow_mut().push(addr);
        0
    }

    fn read_colour(&self, offset: u16) -> u8 {
        self.colour.borrow_mut().push(offset);
        0
    }
}

/// One engine cycle's observed behaviour.
struct CycleObs {
    cycle: u8,
    ba_low: bool,
    vram_reads: Vec<u16>,
    colour_reads: Vec<u16>,
    vc: u16,
    vcbase: u16,
    rc: u8,
}

impl CycleObs {
    /// `read_vram` calls hitting the video-matrix region (c-accesses).
    fn matrix_reads(&self) -> usize {
        self.vram_reads
            .iter()
            .filter(|&&a| (SCREEN_LO..SCREEN_HI).contains(&a))
            .count()
    }
}

/// Run one PAL frame with `setup` applied first, collecting per-cycle
/// observations for `line`. Tick index `k` deterministically maps to engine
/// `(line, cycle)` = `(k / 63, k % 63)`, so no engine cycle accessor is needed.
fn capture_line(setup: impl Fn(&mut Vic), line: u16) -> Vec<CycleObs> {
    let mut vic = Vic::new(VicModel::Pal6569);
    setup(&mut vic);
    let mem = RecordingMemory::new();
    let mut obs = Vec::new();

    for k in 0..(LINES_PER_FRAME * CYCLES_PER_LINE) {
        let this_line = (k / CYCLES_PER_LINE) as u16;
        let this_cycle = (k % CYCLES_PER_LINE) as u8;
        mem.clear();
        vic.tick(&mem);
        if this_line == line {
            obs.push(CycleObs {
                cycle: this_cycle,
                ba_low: vic.ba_low,
                vram_reads: mem.vram.borrow().clone(),
                colour_reads: mem.colour.borrow().clone(),
                vc: vic.vc(),
                vcbase: vic.vcbase(),
                rc: vic.rc(),
            });
        }
    }
    assert_eq!(obs.len(), CYCLES_PER_LINE as usize, "captured a full line");
    obs
}

/// Standard text mode, DEN on, RSEL/CSEL=1, YSCROLL=0, no sprites.
fn text_mode(vic: &mut Vic) {
    vic.write(0x11, 0x18); // DEN=1, RSEL=1, YSCROLL=0
    vic.write(0x16, 0x08); // CSEL=1
    vic.write(REG_D018, 0x18); // matrix $0400, char $2000
    vic.write(0x15, 0x00); // sprites off
}

// ---- Live tests: lock the timing the engine already gets right ----

#[test]
fn badline_ba_low_matches_canonical() {
    let obs = capture_line(text_mode, BADLINE);
    for o in &obs {
        let entry = canonical_for_engine_cycle(o.cycle);
        let expected = expected_ba_low(entry, true, 0);
        assert_eq!(
            o.ba_low, expected,
            "engine cycle {} (canonical {}): BA low should be {expected}",
            o.cycle, entry.cycle
        );
    }
    // Sanity: this really exercised the fetch window (cycles 12-54).
    assert!(obs.iter().filter(|o| o.ba_low).count() == 43);
}

#[test]
fn non_badline_never_pulls_ba_low() {
    let obs = capture_line(text_mode, NON_BADLINE);
    for o in &obs {
        let entry = canonical_for_engine_cycle(o.cycle);
        assert!(
            !expected_ba_low(entry, false, 0),
            "oracle: no BA off-badline"
        );
        assert!(!o.ba_low, "engine cycle {}: BA should stay high", o.cycle);
    }
}

#[test]
fn badline_fetches_the_full_matrix_row() {
    let obs = capture_line(text_mode, BADLINE);
    let colour: usize = obs.iter().map(|o| o.colour_reads.len()).sum();
    let matrix: usize = obs.iter().map(CycleObs::matrix_reads).sum();
    assert_eq!(colour, 40, "40 colour-RAM reads over the badline");
    assert_eq!(matrix, 40, "40 video-matrix reads over the badline");
}

#[test]
fn non_badline_does_no_matrix_fetch() {
    let obs = capture_line(text_mode, NON_BADLINE);
    let colour: usize = obs.iter().map(|o| o.colour_reads.len()).sum();
    let matrix: usize = obs.iter().map(CycleObs::matrix_reads).sum();
    assert_eq!(colour, 0, "no colour-RAM reads off a badline");
    assert_eq!(matrix, 0, "no video-matrix reads off a badline");
}

#[test]
fn sprite0_ba_lead_in_matches_canonical() {
    let setup = |vic: &mut Vic| {
        text_mode(vic);
        vic.write(0x15, 0x01); // enable sprite 0
        vic.write(0x01, 50); // sprite 0 Y = 50 → DMA-active across line 0x39
    };
    let obs = capture_line(setup, NON_BADLINE);
    for o in &obs {
        let entry = canonical_for_engine_cycle(o.cycle);
        let expected = expected_ba_low(entry, false, 0x01);
        assert_eq!(
            o.ba_low, expected,
            "engine cycle {} (canonical {}): sprite-0 BA should be {expected}",
            o.cycle, entry.cycle
        );
    }
    // Sprite-0 lead-in is canonical cycles 55-59.
    assert_eq!(obs.iter().filter(|o| o.ba_low).count(), 5);
}

// ---- Increment 2: shadow VC/VCBASE/RC validated against the geometry path ----
// The video-counter chain runs in parallel with the existing geometry
// addressing but does not yet drive fetches. These prove the shadow counters
// produce exactly the matrix addresses the geometry path uses, so Increment 3
// can swap the addressing over with confidence. `obs[c]` indexes cycle `c`
// directly — `capture_line` pushes all 63 cycles in order.

fn text_row(line: u16) -> u16 {
    (line - 0x30) / 8
}

#[test]
fn shadow_vc_tracks_geometry_matrix_addresses_on_a_badline() {
    let obs = capture_line(text_mode, BADLINE);
    let base = text_row(BADLINE) * 40;
    // On the 40 c-access cycles (canonical 15-54) VC walks the matrix row:
    // base + column. This is exactly screen_base + text_row*40 + col offset
    // the geometry path computes in `fetch_screen_row`.
    for c in 15u8..=54 {
        let col = u16::from(c - 15);
        assert_eq!(
            obs[c as usize].vc,
            base + col,
            "cycle {c}: VC should address matrix col {col} of row {}",
            text_row(BADLINE)
        );
    }
}

#[test]
fn shadow_vcbase_advances_one_row_per_character_block() {
    // VCBASE during the c-access window is the row's matrix base; it must
    // equal the geometry text_row * 40 for every visible character row.
    for line in [0x30u16, 0x38, 0x40, 0x48, 0x80, 0xC0] {
        let obs = capture_line(text_mode, line);
        assert_eq!(
            obs[15].vcbase,
            text_row(line) * 40,
            "line {line:#x}: VCBASE should be the base of row {}",
            text_row(line)
        );
    }
}

#[test]
fn shadow_rc_counts_the_character_sub_row() {
    // RC resets to 0 on a badline (UpdateVc, cycle 14) and steps by one each
    // subsequent raster of the 8-line character block.
    let badline = capture_line(text_mode, BADLINE);
    assert_eq!(badline[20].rc, 0, "RC is 0 on the badline (first) raster");

    let second = capture_line(text_mode, BADLINE + 1);
    assert_eq!(second[20].rc, 1, "RC is 1 on the row's second raster");

    let eighth = capture_line(text_mode, BADLINE + 7);
    assert_eq!(eighth[20].rc, 7, "RC is 7 on the row's last raster");
}

// ---- Increment 3: c-access streaming (now live) ----
// The canonical per-cycle video-matrix fetch distribution. These were the
// rewrite's acceptance criteria; they pass now that the engine streams one
// c-access per badline cycle 15-54 instead of batching 40 at cycle 15.

#[test]
fn c_access_streams_one_per_cycle() {
    let obs = capture_line(text_mode, BADLINE);
    for o in &obs {
        let canonical = oracle::engine_to_canonical(o.cycle);
        let expected = usize::from((15..=54).contains(&canonical));
        assert_eq!(
            o.matrix_reads(),
            expected,
            "engine cycle {} (canonical {canonical}): expected {expected} c-access",
            o.cycle
        );
    }
}

#[test]
fn colour_access_streams_one_per_cycle() {
    let obs = capture_line(text_mode, BADLINE);
    for o in &obs {
        let canonical = oracle::engine_to_canonical(o.cycle);
        let expected = usize::from((15..=54).contains(&canonical));
        assert_eq!(
            o.colour_reads.len(),
            expected,
            "engine cycle {} (canonical {canonical}): expected {expected} colour read",
            o.cycle
        );
    }
}

// ---- Increment 4: sprite p/s-access streaming (now live) ----
// The sprite fetch streams across two cycles: pointer + data byte 0 on the
// p-access cycle, data bytes 1-2 on the next. Was the rewrite's last ignored
// acceptance test; passes now that the batched 4-reads-at-once fetch is split.

#[test]
fn sprite0_data_access_spans_two_cycles() {
    let setup = |vic: &mut Vic| {
        text_mode(vic);
        vic.write(0x15, 0x01);
        vic.write(0x01, 50);
    };
    let obs = capture_line(setup, NON_BADLINE);
    // Canonical sprite 0: cycle 58 = p-access + s-byte 0 (2 reads);
    // cycle 59 = s-bytes 1 and 2 (2 reads).
    let at = |c: u8| {
        obs.iter()
            .find(|o| o.cycle == c)
            .map_or(0, |o| o.vram_reads.len())
    };
    assert_eq!(at(58), 2, "cycle 58: pointer + first sprite byte");
    assert_eq!(at(59), 2, "cycle 59: remaining two sprite bytes");
}
