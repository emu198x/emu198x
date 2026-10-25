use super::*;

const K: u32 = 1024;

/// Tick to the start of `line`, from wherever the chip is.
fn run_to(glue: &mut GlueMmu, line: u16, cycle: u16) {
    while glue.position() != (line, cycle) {
        glue.tick();
    }
}

fn frame_cycles(glue: &mut GlueMmu) -> u64 {
    let start = glue.frames();
    run_to(glue, 0, 0);
    let mut cycles = 0;
    while glue.frames() == start + 1 {
        glue.tick();
        cycles += 1;
        if glue.position() == (0, 0) {
            break;
        }
    }
    cycles
}

#[test]
fn a_50_hz_frame_is_313_lines_of_512_cycles() {
    let mut glue = GlueMmu::new(512 * K, 0);
    glue.tick();
    assert_eq!(frame_cycles(&mut glue), 313 * 512);
}

#[test]
fn a_60_hz_frame_is_263_lines_of_508_cycles() {
    let mut glue = GlueMmu::new(512 * K, 0);
    glue.write(0x20A, 0x00);
    glue.tick();
    assert_eq!(frame_cycles(&mut glue), 263 * 508);
}

#[test]
fn high_resolution_runs_the_71_hz_monitor() {
    let mut glue = GlueMmu::new(512 * K, 0);
    glue.set_resolution(2);
    glue.tick();
    assert_eq!(frame_cycles(&mut glue), 501 * 224);
}

#[test]
fn a_displayed_line_fetches_eighty_words_one_every_four_cycles() {
    let mut glue = GlueMmu::new(512 * K, 0);
    glue.write(0x201, 0x07);
    glue.write(0x203, 0x80);
    // Run a whole frame so the counter has reloaded from the base.
    for _ in 0..313 * 512 {
        glue.tick();
    }
    run_to(&mut glue, 63, 0);
    let mut fetches = Vec::new();
    for cycle in 0..512u16 {
        glue.tick();
        if let Some(address) = glue.take_fetch() {
            fetches.push((cycle, address));
        }
        assert_eq!(glue.display_enable(), (56..376).contains(&cycle));
    }
    assert_eq!(fetches.len(), 80);
    assert_eq!(fetches[0], (56, 0x07_8000));
    assert_eq!(fetches[1], (60, 0x07_8002));
    assert_eq!(fetches[79].1, 0x07_8000 + 158);
}

#[test]
fn no_fetches_outside_the_display_lines() {
    let mut glue = GlueMmu::new(512 * K, 0);
    run_to(&mut glue, 62, 0);
    for _ in 0..512 {
        glue.tick();
        assert!(glue.take_fetch().is_none());
    }
    run_to(&mut glue, 263, 0);
    for _ in 0..512 {
        glue.tick();
        assert!(glue.take_fetch().is_none());
    }
}

#[test]
fn the_counter_reloads_three_lines_before_the_frame_ends() {
    let mut glue = GlueMmu::new(512 * K, 0);
    glue.write(0x201, 0x01);
    run_to(&mut glue, 310, 56);
    assert_eq!(glue.read(0x205), Some(0x00));
    glue.tick();
    assert_eq!(glue.read(0x205), Some(0x01));
    assert_eq!(glue.read(0x207), Some(0x00));
    assert_eq!(glue.read(0x209), Some(0x00));
}

#[test]
fn hbl_comes_every_line_and_vbl_once_a_frame_at_cycle_64() {
    let mut glue = GlueMmu::new(512 * K, 0);
    let (mut hbls, mut vbls) = (0, 0);
    let mut vbl_at = None;
    for _ in 0..313 * 512 {
        glue.tick();
        hbls += u32::from(glue.take_hbl());
        if glue.take_vbl() {
            vbls += 1;
            vbl_at = Some(glue.position());
        }
    }
    assert_eq!(hbls, 313);
    assert_eq!(vbls, 1);
    // Position is read after the tick, so the VBL cycle is one behind.
    assert_eq!(vbl_at, Some((0, 65)));
}

#[test]
fn timer_b_input_lags_display_enable_by_24_cycles() {
    let mut glue = GlueMmu::new(512 * K, 0);
    run_to(&mut glue, 63, 375);
    glue.tick(); // cycle 375: last DE cycle
    assert!(glue.display_enable());
    for _ in 0..24 {
        glue.tick();
        assert!(glue.timer_b_input());
    }
    glue.tick();
    assert!(!glue.timer_b_input());
}

#[test]
fn a_frequency_change_takes_effect_at_the_next_line() {
    let mut glue = GlueMmu::new(512 * K, 0);
    run_to(&mut glue, 100, 10);
    glue.write(0x20A, 0x00);
    assert_eq!(glue.frequency(), Frequency::Hz50);
    run_to(&mut glue, 101, 0);
    glue.tick();
    assert_eq!(glue.frequency(), Frequency::Hz60);
}

#[test]
fn registers_read_back_with_the_stf_masks() {
    let mut glue = GlueMmu::new(512 * K, 0);
    assert!(glue.write(0x001, 0xFA));
    assert_eq!(glue.read(0x001), Some(0x0A));
    glue.write(0x201, 0xFF);
    assert_eq!(glue.read(0x201), Some(0x3F));
    glue.write(0x20A, 0xFF);
    assert_eq!(glue.read(0x20A), Some(0xFF));
    glue.write(0x205, 0x12);
    assert_eq!(glue.read(0x205), Some(0x00), "the counter is read-only");
    glue.write(0x609, 0x01);
    glue.write(0x60B, 0x23);
    glue.write(0x60D, 0x45);
    assert_eq!(glue.dma_address(), 0x01_2344, "DMA addresses are even");
    glue.advance_dma(512);
    assert_eq!(glue.read(0x60B), Some(0x25));
    assert_eq!(glue.read(0x203), Some(0));
    assert!(!glue.write(0x100, 0));
    assert_eq!(glue.read(0x100), None);
}

#[test]
fn a_matching_configuration_maps_ram_straight_through() {
    let mut glue = GlueMmu::new(512 * K, 512 * K);
    glue.set_config(glue.matching_config());
    assert_eq!(glue.config(), 0x05);
    assert_eq!(glue.translate(0x01_2345), Some(0x01_2345));
    assert_eq!(glue.translate(0x08_0010), Some(0x08_0010), "bank 1");
    assert_eq!(glue.translate(0x10_0000), None, "past both banks");
}

#[test]
fn a_2_mb_configuration_over_512_kb_chips_drops_the_top_row_and_column_bits() {
    let mut glue = GlueMmu::new(512 * K, 0);
    glue.set_config(0x0A);
    for address in [0x0000_0400u32, 0x0001_2345, 0x0007_FFFE, 0x000F_F800] {
        let expected = ((address & 0xF_F800) >> 1) | (address & 0x3FF);
        assert_eq!(
            glue.translate(address),
            Some(expected as usize),
            "{address:#x}"
        );
    }
}

#[test]
fn a_512_kb_configuration_over_2_mb_chips_drives_a10_and_a19_on_the_spare_lines() {
    let mut glue = GlueMmu::new(2048 * K, 0);
    glue.set_config(0x04);
    for address in [0x0000_0400u32, 0x0001_2345, 0x0007_FFFE] {
        let expected = ((address & 0xF_FC00) << 1) | (address & 0x7FF);
        assert_eq!(
            glue.translate(address),
            Some(expected as usize),
            "{address:#x}"
        );
    }
}

#[test]
fn a_128_kb_configuration_over_2_mb_chips() {
    let mut glue = GlueMmu::new(2048 * K, 0);
    glue.set_config(0x00);
    let address = 0x0001_5A5Au32;
    let expected = ((address & 0x7_FE00) << 2) | (address & 0x7FF);
    assert_eq!(glue.translate(address), Some(expected as usize));
}

#[test]
fn an_empty_bank_answers_nothing() {
    let mut glue = GlueMmu::new(512 * K, 0);
    glue.set_config(0x05);
    assert_eq!(glue.translate(0x08_0000), None);
    assert_eq!(glue.physical_ram(), 512 * 1024);
    assert_eq!(glue.matching_config(), 0x04);
}

#[test]
fn state_round_trips_through_serde() {
    let mut glue = GlueMmu::new(1024 * K, 0);
    for _ in 0..1000 {
        glue.tick();
    }
    let json = serde_json::to_string(&glue).expect("serde round trip");
    let back: GlueMmu = serde_json::from_str(&json).expect("serde round trip");
    assert_eq!(back.position(), glue.position());
}
