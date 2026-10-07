//! Completion wake timing from the registered FS-UAE request/state trace.
use common_commodore_amiga::{copper::Copper, memory::Memory};

fn parked_wait() -> (Copper, Memory) {
    let mut memory = Memory::new(vec![0; 256 * 1024]);
    memory.set_overlay(false);
    for (address, value) in [
        (0x1000, 1),
        (0x1002, 0x7ffe),
        (0x1004, 0x180),
        (0x1006, 0xf00),
    ] {
        memory.write_word(address, value);
    }
    let mut copper = Copper::new();
    copper.cop1lc = 0x1000;
    copper.jump1();
    for h in 0..8 {
        assert_eq!(copper.tick_cck(&memory, 0, h + 2, h % 2 == 0, true), None);
    }
    assert!(copper.waiting);
    (copper, memory)
}

#[test]
fn completion_wake_compares_in_a_free_cell_before_fetching() {
    let mut rows = 0;
    for (first_idle, blocked_cell, expected_move) in [
        (10, None, 16),
        (11, None, 16),
        (12, None, 18),
        (13, None, 18),
        (10, Some(12), 18),
        (11, Some(12), 18),
    ] {
        let (mut copper, memory) = parked_wait();
        let mut moves = Vec::new();
        for h in 8..=expected_move {
            copper.bus_used_this_cck = false;
            let granted = h % 2 == 0 && blocked_cell != Some(h);
            if let Some(value) = copper.tick_cck(&memory, 0, h + 2, granted, h < first_idle) {
                moves.push((h, value));
            }
            if h < expected_move - 2 {
                assert!(
                    !copper.bus_used_this_cck,
                    "wake/comparison must yield the bus at {h}"
                );
            }
        }
        assert_eq!(
            moves,
            [(expected_move, (0x180, 0xf00))],
            "idle={first_idle}, blocked={blocked_cell:?}"
        );
        rows += 1;
    }
    assert_eq!(rows, 6);
}

#[test]
fn completion_wake_rechecks_live_busy_and_jump_cancels_it() {
    for second_jump in [false, true] {
        let (mut copper, memory) = parked_wait();
        assert!(copper.wait_blitter_blocked);
        assert_eq!(copper.tick_cck(&memory, 0, 12, true, false), None);
        assert!(copper.pending_wait_delay);
        // A restart before comparison must not let the stashed wake fetch.
        copper.cop1lc = 0x1004;
        copper.cop2lc = 0x1004;
        if second_jump {
            copper.jump2();
        } else {
            copper.jump1();
        }
        assert!(!copper.wait_blitter_blocked);
        assert!(!copper.pending_wait_delay);
        assert_eq!(copper.tick_cck(&memory, 0, 14, true, true), None);
        assert_eq!(
            copper.tick_cck(&memory, 0, 16, true, true),
            Some((0x180, 0xf00))
        );
    }
    let (mut copper, memory) = parked_wait();
    assert_eq!(copper.tick_cck(&memory, 0, 12, true, false), None);
    assert_eq!(copper.tick_cck(&memory, 0, 14, true, true), None);
    assert!(copper.waiting && copper.wait_blitter_blocked);
    let mut moves = Vec::new();
    for h in 14..=20 {
        if let Some(value) = copper.tick_cck(&memory, 0, h + 2, h % 2 == 0, false) {
            moves.push((h, value));
        }
    }
    assert_eq!(moves, [(20, (0x180, 0xf00))]);
}

#[test]
fn beam_only_bfd0_wait_does_not_acquire_a_completion_wake() {
    let mut memory = Memory::new(vec![0; 256 * 1024]);
    memory.set_overlay(false);
    for (address, value) in [
        (0x1000, 0x0101),
        (0x1002, 0x7ffe),
        (0x1004, 0x180),
        (0x1006, 0xf00),
    ] {
        memory.write_word(address, value);
    }
    let mut copper = Copper::new();
    copper.cop1lc = 0x1000;
    copper.jump1();
    for h in 0..10 {
        assert_eq!(copper.tick_cck(&memory, 0, h + 2, h % 2 == 0, true), None);
    }
    assert!(copper.waiting && !copper.wait_blitter_blocked);
    let mut moves = Vec::new();
    for h in 10..=14 {
        if let Some(value) = copper.tick_cck(&memory, 1, h + 2, h % 2 == 0, false) {
            moves.push((h, value));
        }
    }
    assert_eq!(moves, [(14, (0x180, 0xf00))]);
}
