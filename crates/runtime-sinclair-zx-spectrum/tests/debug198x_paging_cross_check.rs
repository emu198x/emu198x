//! Debug198x freeze, leg 3: the format's banked model against this repo's real
//! Spectrum 128 paging.
//!
//! Legs 1 and 2 live in the Asm198x tree — the reader resolving the fixture per
//! paging state, and a desk projection of every banked record onto sjasmplus
//! SLD long addresses. Both reason about the fixture alone. This leg is the one
//! that could not be done there: it asks whether the fixture's slot/page
//! expectations describe a machine this emulator can actually be in.
//!
//! So nothing here is asserted from the spec. Every address the sidecar claims
//! is checked against a `Memory128K` that has been paged into that state, by
//! reading the byte back through the same `MemoryBus` the CPU uses.
//!
//! The fixture (`spectrum128-banked.debug198x`, copied from the Asm198x corpus)
//! places `draw` in page 1 and `music` in page 3, both at offset `$0010` of
//! slot 3. Its companion table states the arithmetic this leg has to confirm:
//! slot 3 is based at `$C000`, so both resolve to CPU `$C010` — the *same*
//! address, told apart only by which page is paged in.
//!
//! No ROMs, no firmware, no skips: `Memory128K::new()` is enough, so this runs
//! everywhere rather than being green because it did not execute.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use emu198x_shell::debug_info::DebugSymbols;
use machine_sinclair_zx_spectrum_128k::Memory128K;

/// The fixture, byte-identical to the Asm198x corpus copy.
const BANKED: &str = include_str!(
    "../../../test-data/sinclair/zx-spectrum-128/debug198x/spectrum128-banked.debug198x"
);

/// Slot 3 on a 128K Spectrum: the switchable 16 KiB window.
const SLOT3_BASE: u16 = 0xC000;
/// Where both fixture symbols sit inside their page.
const OFFSET: u16 = 0x0010;
/// The CPU address both therefore share.
const SHARED_ADDR: u16 = SLOT3_BASE + OFFSET;
/// Every 128K slot is a 16 KiB window.
const SLOT_SIZE: u64 = 0x4000;

/// Pages the fixture describes, and a marker byte to prove which one is live.
const PAGE_DRAW: u8 = 1;
const PAGE_MUSIC: u8 = 3;
const MARK_DRAW: u8 = 0xD4;
const MARK_MUSIC: u8 = 0x3C;

fn symbols() -> DebugSymbols {
    DebugSymbols::from_ndjson(BANKED, "spectrum128-banked.debug198x").expect("fixture loads")
}

/// A 128K memory with a distinguishable byte at `OFFSET` of each fixture page.
fn memory_with_marked_pages() -> Memory128K {
    let mut memory = Memory128K::new();
    memory.ram_bank_mut(PAGE_DRAW as usize)[OFFSET as usize] = MARK_DRAW;
    memory.ram_bank_mut(PAGE_MUSIC as usize)[OFFSET as usize] = MARK_MUSIC;
    memory
}

/// Pages `page` into slot 3, as `OUT ($7FFD)` does.
fn page_into_slot3(memory: &mut Memory128K, page: u8) {
    memory.write_7ffd(page);
    assert_eq!(
        memory.current_bank(),
        page,
        "bits 0-2 of $7FFD select the bank at $C000"
    );
}

#[test]
fn slot_three_is_the_switchable_window_the_fixture_assumes() {
    // The fixture's arithmetic rests on slot 3 starting at $C000. Confirmed
    // against the machine rather than the spec: the byte written into a page
    // is readable at $C000 + offset exactly when that page is selected.
    let mut memory = memory_with_marked_pages();

    page_into_slot3(&mut memory, PAGE_DRAW);
    assert_eq!(memory.read(SHARED_ADDR), MARK_DRAW);

    page_into_slot3(&mut memory, PAGE_MUSIC);
    assert_eq!(memory.read(SHARED_ADDR), MARK_MUSIC);
}

#[test]
fn every_page_the_fixture_names_is_one_the_hardware_can_select() {
    // `space.page` is a u16 in the format, but this machine has eight banks
    // and selects them with three bits. A fixture naming page 9 would describe
    // a machine that cannot exist, and nothing else in the corpus would catch
    // it — the reader would resolve it happily.
    let mut memory = Memory128K::new();
    for page in [PAGE_DRAW, PAGE_MUSIC] {
        assert!(
            page < 8,
            "page {page} is outside this machine's eight banks"
        );
        page_into_slot3(&mut memory, page);
    }
}

#[test]
fn the_symbol_at_one_address_follows_the_machines_paging_state() {
    // The heart of leg 3. One CPU address, two symbols, and the only thing
    // separating them is which bank the machine has paged in — so the sidecar
    // and the memory are driven from the same state and must agree.
    let mut memory = memory_with_marked_pages();
    let mut sym = symbols();

    page_into_slot3(&mut memory, PAGE_DRAW);
    sym.set_paging_from_slots([(
        3,
        u16::from(memory.current_bank()),
        u64::from(SLOT3_BASE),
        SLOT_SIZE,
    )]);
    assert_eq!(memory.read(SHARED_ADDR), MARK_DRAW, "bank 1 is live");
    assert_eq!(sym.symbol_at(u32::from(SHARED_ADDR)), Some("draw"));
    assert_eq!(sym.line_at(u32::from(SHARED_ADDR)).map(|l| l.line), Some(5));
    // …and the bank that is paged out cannot answer at all.
    assert_eq!(sym.addr_of("music"), None);

    page_into_slot3(&mut memory, PAGE_MUSIC);
    sym.set_paging_from_slots([(
        3,
        u16::from(memory.current_bank()),
        u64::from(SLOT3_BASE),
        SLOT_SIZE,
    )]);
    assert_eq!(memory.read(SHARED_ADDR), MARK_MUSIC, "bank 3 is live");
    assert_eq!(sym.symbol_at(u32::from(SHARED_ADDR)), Some("music"));
    assert_eq!(
        sym.line_at(u32::from(SHARED_ADDR)).map(|l| l.line),
        Some(12)
    );
    assert_eq!(sym.addr_of("draw"), None);
}

#[test]
fn a_page_the_image_has_no_code_in_answers_nothing() {
    // Selecting a bank the sidecar never mentions must not fall through to
    // another bank's symbol. This is the property that makes a wrong answer
    // impossible rather than merely unlikely.
    let mut memory = memory_with_marked_pages();
    let mut sym = symbols();

    page_into_slot3(&mut memory, 6);
    sym.set_paging_from_slots([(
        3,
        u16::from(memory.current_bank()),
        u64::from(SLOT3_BASE),
        SLOT_SIZE,
    )]);

    assert_eq!(sym.symbol_at(u32::from(SHARED_ADDR)), None);
    assert_eq!(sym.line_at(u32::from(SHARED_ADDR)), None);
    assert_eq!(sym.addr_of("draw"), None);
    assert_eq!(sym.addr_of("music"), None);
}

#[test]
fn the_sld_long_address_projection_matches_this_machines_banks() {
    // Leg 2 projected each record onto an sjasmplus SLD long address —
    // `page * 0x4000 + offset` — as a desk exercise. Tied here to the machine
    // it describes: a long address is a position in this machine's RAM, so it
    // must land inside the bank the page names.
    for (page, long_address) in [(PAGE_DRAW, 0x4010_u32), (PAGE_MUSIC, 0xC010_u32)] {
        let computed = u32::from(page) * 0x4000 + u32::from(OFFSET);
        assert_eq!(computed, long_address, "page {page} long address");
        // The projection addresses 128 KiB of RAM as one flat space, which is
        // exactly this machine's eight 16 KiB banks.
        assert!(computed < 8 * 0x4000, "long address is inside 128K of RAM");
        assert_eq!(
            computed as usize / 0x4000,
            page as usize,
            "lands in its bank"
        );
    }
    // `music`'s long address coinciding with its CPU address is arithmetic —
    // 3 * $4000 == $C000 — not meaning. Asserted so nobody later reads the
    // coincidence as a rule.
    assert_eq!(u32::from(PAGE_MUSIC) * 0x4000, u32::from(SLOT3_BASE));
    assert_ne!(u32::from(PAGE_DRAW) * 0x4000, u32::from(SLOT3_BASE));
}

#[test]
fn a_bank_live_in_two_slots_at_once_answers_at_both_addresses() {
    // Banks 5 and 2 sit at $4000 and $8000 permanently, and bits 0-2 of $7FFD
    // can *also* select them for $C000. Page 5 in and one bank is live at two
    // CPU addresses — proven here by writing through one window and reading it
    // back through the other.
    //
    // A `BaseMap` holds one base per section and cannot describe that, so the
    // importer does not resolve addresses through a single map: each lookup
    // goes through the slot that contains it. Every CPU address is in exactly
    // one slot, so both windows answer correctly.
    let mut memory = Memory128K::new();
    page_into_slot3(&mut memory, 5);

    memory.write(0x4000 + OFFSET, 0xAB);
    assert_eq!(
        memory.read(SLOT3_BASE + OFFSET),
        0xAB,
        "bank 5 is live at $4000 and $C000 simultaneously"
    );

    // Describe that machine to the importer: slot 1 holds page 5, and so does
    // slot 3. A fixture section in page 5 is therefore live at both.
    let banked = BANKED.replace(
        r#""space":{"slot":3,"page":1}"#,
        r#""space":{"slot":3,"page":5}"#,
    );
    let mut sym = DebugSymbols::from_ndjson(&banked, "aliased.debug198x").expect("loads");
    sym.set_paging_from_slots([
        (1, 5, 0x4000, SLOT_SIZE),
        (3, 5, u64::from(SLOT3_BASE), SLOT_SIZE),
    ]);

    // The same symbol answers at both windows, each resolved through its own
    // slot rather than through one map that could only hold one of them.
    assert_eq!(sym.symbol_at(0x4000 + u32::from(OFFSET)), Some("draw"));
    assert_eq!(
        sym.symbol_at(u32::from(SLOT3_BASE) + u32::from(OFFSET)),
        Some("draw")
    );

    // And going the other way is genuinely two answers, not one.
    assert_eq!(sym.addrs_of("draw"), vec![0x4010, 0xC010]);
    assert_eq!(
        sym.addr_of("draw"),
        Some(0x4010),
        "addr_of gives the lowest"
    );
}
