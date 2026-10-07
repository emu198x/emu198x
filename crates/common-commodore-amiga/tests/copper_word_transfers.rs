//! Each Copper fetch cell reads one word, as FS-UAE's COP_read1/read2 do.
use common_commodore_amiga::{copper::Copper, memory::Memory};

fn fixture() -> (Copper, Memory) {
    let mut memory = Memory::new(vec![0; 256 * 1024]);
    memory.set_overlay(false);
    memory.write_word(0x1000, 0x0180);
    memory.write_word(0x1002, 0x0f00);
    let mut copper = Copper::new();
    copper.cop1lc = 0x1000;
    copper.jump1();
    (copper, memory)
}

#[test]
fn first_word_is_retained_but_second_word_is_read_in_its_own_cell() {
    for denied_cells in [0, 1, 7] {
        let (mut copper, mut memory) = fixture();
        assert_eq!(copper.tick_cck(&memory, 0, 2, true, false), None);
        assert_eq!(memory.diagnostic_snapshot().floating_bus_word, 0x0180);
        memory.write_word(0x1000, 0x0182);
        memory.write_word(0x1002, 0x00f0);
        for _ in 0..denied_cells {
            assert_eq!(copper.tick_cck(&memory, 0, 3, false, false), None);
        }
        assert_eq!(
            copper.tick_cck(&memory, 0, 4, true, false),
            Some((0x0180, 0x00f0))
        );
        assert_eq!(copper.pc, 0x1004);
        assert_eq!(memory.diagnostic_snapshot().floating_bus_word, 0x00f0);
    }
}

#[test]
fn restore_between_words_retains_the_word_actually_fetched() {
    let (mut copper, mut memory) = fixture();
    assert_eq!(copper.tick_cck(&memory, 0, 2, true, false), None);
    let bytes = postcard::to_allocvec(&copper).expect("serialize Copper");
    let mut restored: Copper = postcard::from_bytes(&bytes).expect("restore Copper");
    memory.write_word(0x1000, 0x0182);
    memory.write_word(0x1002, 0x00f0);
    for chip in [&mut copper, &mut restored] {
        assert_eq!(
            chip.pc, 0x1002,
            "IR1 service advances the address by one word"
        );
        assert_eq!(
            chip.tick_cck(&memory, 0, 4, true, false),
            Some((0x0180, 0x00f0))
        );
    }
}

#[test]
fn jump_cancels_the_partial_instruction() {
    for second_jump in [false, true] {
        let (mut copper, mut memory) = fixture();
        assert_eq!(copper.tick_cck(&memory, 0, 2, true, false), None);
        memory.write_word(0x2000, 0x0184);
        memory.write_word(0x2002, 0x000f);
        copper.cop1lc = 0x2000;
        copper.cop2lc = 0x2000;
        if second_jump {
            copper.jump2();
        } else {
            copper.jump1();
        }
        assert_eq!(copper.tick_cck(&memory, 0, 4, true, false), None);
        assert_eq!(
            copper.tick_cck(&memory, 0, 6, true, false),
            Some((0x0184, 0x000f))
        );
    }
}
