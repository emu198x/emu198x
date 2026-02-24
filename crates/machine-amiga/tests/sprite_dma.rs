use machine_amiga::memory::ROM_BASE;
use machine_amiga::Amiga;
use machine_amiga::TICKS_PER_CCK;

const REG_DMACON: u16 = 0x096;
const REG_SPR0PTH: u16 = 0x120;
const REG_SPR0PTL: u16 = 0x122;

const DMACON_SPREN: u16 = 0x0020;
const DMACON_DMAEN: u16 = 0x0200;

fn make_test_amiga() -> Amiga {
    let mut rom = vec![0u8; 256 * 1024];

    let ssp = 0x0007FFF0u32;
    rom[0] = (ssp >> 24) as u8;
    rom[1] = (ssp >> 16) as u8;
    rom[2] = (ssp >> 8) as u8;
    rom[3] = ssp as u8;

    let pc = ROM_BASE + 8;
    rom[4] = (pc >> 24) as u8;
    rom[5] = (pc >> 16) as u8;
    rom[6] = (pc >> 8) as u8;
    rom[7] = pc as u8;

    // BRA.S *
    rom[8] = 0x60;
    rom[9] = 0xFE;

    Amiga::new(rom)
}

fn tick_ccks(amiga: &mut Amiga, ccks: u32) {
    for _ in 0..ccks {
        for _ in 0..TICKS_PER_CCK {
            amiga.tick();
        }
    }
}

fn write_sprite_words(amiga: &mut Amiga, base: u32, words: &[u16]) {
    for (i, &word) in words.iter().enumerate() {
        let addr = base + (i as u32) * 2;
        amiga.memory.write_byte(addr, (word >> 8) as u8);
        amiga.memory.write_byte(addr + 1, word as u8);
    }
}

fn wait_for_sprite_slot(amiga: &mut Amiga, sprite: u8) {
    while amiga.agnus.cck_bus_plan().sprite_dma_service_channel != Some(sprite) {
        tick_ccks(amiga, 1);
    }
}

fn wait_for_beam(amiga: &mut Amiga, vpos: u16, hpos: u16) {
    while amiga.agnus.vpos != vpos || amiga.agnus.hpos != hpos {
        tick_ccks(amiga, 1);
    }
}

#[test]
fn sprite_dma_slots_fetch_sprxpos_ctl_then_data_datb() {
    let mut amiga = make_test_amiga();
    let spr0_addr = 0x0000_3000u32;

    // POS=$0000, CTL=$0200 => vstart=0, vstop=2, x=0
    amiga.memory.write_byte(spr0_addr, 0x00);
    amiga.memory.write_byte(spr0_addr + 1, 0x00);
    amiga.memory.write_byte(spr0_addr + 2, 0x02);
    amiga.memory.write_byte(spr0_addr + 3, 0x00);
    amiga.memory.write_byte(spr0_addr + 4, 0x9A);
    amiga.memory.write_byte(spr0_addr + 5, 0xBC);
    amiga.memory.write_byte(spr0_addr + 6, 0xDE);
    amiga.memory.write_byte(spr0_addr + 7, 0xF0);

    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr >> 16) as u16);
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr & 0xFFFF) as u16);
    amiga.denise.spr_pos[0] = 0x1111;
    amiga.denise.spr_ctl[0] = 0x2222;
    amiga.denise.spr_data[0] = 0xAAAA;
    amiga.denise.spr_datb[0] = 0xBBBB;

    // Position just before the first sprite-0 DMA slot pair (0x0B, 0x0C).
    amiga.agnus.vpos = 0;
    amiga.agnus.hpos = 0x0A;
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_SPREN);

    assert_eq!(amiga.agnus.cck_bus_plan().sprite_dma_service_channel, None);
    tick_ccks(&mut amiga, 1); // services hpos=0x0A, advances to 0x0B

    assert_eq!(
        amiga.agnus.spr_pt[0], spr0_addr,
        "sprite pointer must not advance before the first sprite slot"
    );
    assert_eq!(amiga.denise.spr_pos[0], 0x1111);
    assert_eq!(amiga.denise.spr_ctl[0], 0x2222);
    assert_eq!(amiga.denise.spr_data[0], 0xAAAA);
    assert_eq!(amiga.denise.spr_datb[0], 0xBBBB);

    assert_eq!(amiga.agnus.hpos, 0x0B);
    assert_eq!(
        amiga.agnus.cck_bus_plan().sprite_dma_service_channel,
        Some(0)
    );
    tick_ccks(&mut amiga, 1); // sprite-0 first slot => POS

    assert_eq!(
        amiga.denise.spr_pos[0], 0x0000,
        "first sprite slot should fetch into SPR0POS"
    );
    assert_eq!(
        amiga.denise.spr_ctl[0], 0x2222,
        "first sprite slot should not overwrite SPR0CTL"
    );
    assert_eq!(amiga.denise.spr_data[0], 0xAAAA);
    assert_eq!(amiga.denise.spr_datb[0], 0xBBBB);
    assert_eq!(
        amiga.agnus.spr_pt[0],
        spr0_addr + 2,
        "sprite pointer should advance by one word per sprite slot"
    );

    assert_eq!(amiga.agnus.hpos, 0x0C);
    assert_eq!(
        amiga.agnus.cck_bus_plan().sprite_dma_service_channel,
        Some(0)
    );
    tick_ccks(&mut amiga, 1); // sprite-0 second slot => CTL

    assert_eq!(amiga.denise.spr_pos[0], 0x0000);
    assert_eq!(
        amiga.denise.spr_ctl[0], 0x0200,
        "second sprite slot should fetch into SPR0CTL"
    );
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr + 4);

    // Wait for the next sprite-0 slot pair (following scanline) to fetch DATA/DATB.
    while amiga.agnus.cck_bus_plan().sprite_dma_service_channel != Some(0) {
        tick_ccks(&mut amiga, 1);
    }
    tick_ccks(&mut amiga, 1); // sprite-0 next first slot => DATA
    assert_eq!(amiga.denise.spr_data[0], 0x9ABC);
    assert_eq!(amiga.denise.spr_datb[0], 0xBBBB);
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr + 6);

    while amiga.agnus.cck_bus_plan().sprite_dma_service_channel != Some(0) {
        tick_ccks(&mut amiga, 1);
    }
    tick_ccks(&mut amiga, 1); // sprite-0 next second slot => DATB
    assert_eq!(amiga.denise.spr_data[0], 0x9ABC);
    assert_eq!(amiga.denise.spr_datb[0], 0xDEF0);
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr + 8);
}

#[test]
fn sprite_dma_control_words_wait_for_vstart_without_consuming_data_slots() {
    let mut amiga = make_test_amiga();
    let spr0_addr = 0x0000_3200u32;

    // POS=$0200, CTL=$0400 => vstart=2, vstop=4, x=0
    write_sprite_words(&mut amiga, spr0_addr, &[0x0200, 0x0400, 0x9ABC, 0xDEF0]);

    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr >> 16) as u16);
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr & 0xFFFF) as u16);
    amiga.denise.spr_data[0] = 0xAAAA;
    amiga.denise.spr_datb[0] = 0xBBBB;

    amiga.agnus.vpos = 0;
    amiga.agnus.hpos = 0x0A;
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_SPREN);

    tick_ccks(&mut amiga, 1); // -> hpos 0x0B
    tick_ccks(&mut amiga, 1); // fetch POS
    tick_ccks(&mut amiga, 1); // fetch CTL

    assert_eq!(amiga.denise.spr_pos[0], 0x0200);
    assert_eq!(amiga.denise.spr_ctl[0], 0x0400);
    assert_eq!(
        amiga.agnus.spr_pt[0],
        spr0_addr + 4,
        "pointer should advance past control words"
    );

    // Next scanline sprite slots should not consume data yet (vstart=2).
    wait_for_sprite_slot(&mut amiga, 0);
    tick_ccks(&mut amiga, 1);
    assert_eq!(
        amiga.agnus.spr_pt[0],
        spr0_addr + 4,
        "inactive-window first slot must not advance pointer"
    );
    assert_eq!(amiga.denise.spr_data[0], 0xAAAA);
    assert_eq!(amiga.denise.spr_datb[0], 0xBBBB);

    wait_for_sprite_slot(&mut amiga, 0);
    tick_ccks(&mut amiga, 1);
    assert_eq!(
        amiga.agnus.spr_pt[0],
        spr0_addr + 4,
        "inactive-window second slot must not advance pointer"
    );
    assert_eq!(amiga.denise.spr_data[0], 0xAAAA);
    assert_eq!(amiga.denise.spr_datb[0], 0xBBBB);

    // At vstart=2, sprite DMA should begin consuming DATA/DATB.
    while amiga.agnus.vpos != 2 {
        tick_ccks(&mut amiga, 1);
    }
    wait_for_sprite_slot(&mut amiga, 0);
    tick_ccks(&mut amiga, 1);
    assert_eq!(amiga.denise.spr_data[0], 0x9ABC);
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr + 6);

    wait_for_sprite_slot(&mut amiga, 0);
    tick_ccks(&mut amiga, 1);
    assert_eq!(amiga.denise.spr_datb[0], 0xDEF0);
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr + 8);
}

#[test]
fn sprite_dma_disable_stops_slot_fetches_immediately() {
    let mut amiga = make_test_amiga();
    let spr0_addr = 0x0000_3400u32;
    write_sprite_words(&mut amiga, spr0_addr, &[0x0000, 0x0200, 0x9ABC, 0xDEF0]);

    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr >> 16) as u16);
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr & 0xFFFF) as u16);
    amiga.denise.spr_data[0] = 0xAAAA;
    amiga.denise.spr_datb[0] = 0xBBBB;

    amiga.agnus.vpos = 0;
    amiga.agnus.hpos = 0x0A;
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_SPREN);

    tick_ccks(&mut amiga, 1); // -> 0x0B
    tick_ccks(&mut amiga, 1); // POS
    tick_ccks(&mut amiga, 1); // CTL
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr + 4);

    // Disable sprite DMA before the next sprite-0 slot pair.
    amiga.write_custom_reg(REG_DMACON, DMACON_SPREN);

    wait_for_beam(&mut amiga, 1, 0x0B);
    assert_eq!(
        amiga.agnus.cck_bus_plan().sprite_dma_service_channel,
        None,
        "with SPREN clear, sprite0 slots should not be granted"
    );
    let ptr_before = amiga.agnus.spr_pt[0];
    let data_before = amiga.denise.spr_data[0];
    let datb_before = amiga.denise.spr_datb[0];

    tick_ccks(&mut amiga, 1); // sprite0 first slot time (disabled)
    assert_eq!(amiga.agnus.vpos, 1);
    assert_eq!(amiga.agnus.hpos, 0x0C);
    assert_eq!(amiga.agnus.cck_bus_plan().sprite_dma_service_channel, None);

    tick_ccks(&mut amiga, 1); // sprite0 second slot time (disabled)
    assert_eq!(amiga.agnus.vpos, 1);
    assert_eq!(amiga.agnus.hpos, 0x0D);

    assert_eq!(amiga.agnus.spr_pt[0], ptr_before);
    assert_eq!(amiga.denise.spr_data[0], data_before);
    assert_eq!(amiga.denise.spr_datb[0], datb_before);
}

#[test]
fn sprite_pointer_phase_reset_happens_on_low_write_not_high_write() {
    let mut amiga = make_test_amiga();
    let spr0_addr_a = 0x0000_3600u32;
    let spr0_addr_b = 0x0000_3640u32; // same high word; low write should be the commit point

    write_sprite_words(&mut amiga, spr0_addr_a, &[0x0000, 0x0200, 0x9ABC, 0xDEF0]);
    write_sprite_words(&mut amiga, spr0_addr_b, &[0x1357, 0x2468, 0xAAAA, 0xBBBB]);

    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr_a >> 16) as u16);
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr_a & 0xFFFF) as u16);
    amiga.denise.spr_pos[0] = 0x1111;
    amiga.denise.spr_ctl[0] = 0x2222;
    amiga.denise.spr_data[0] = 0x3333;
    amiga.denise.spr_datb[0] = 0x4444;

    amiga.agnus.vpos = 0;
    amiga.agnus.hpos = 0x0A;
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_SPREN);

    tick_ccks(&mut amiga, 1); // -> hpos 0x0B
    tick_ccks(&mut amiga, 1); // POS
    tick_ccks(&mut amiga, 1); // CTL

    assert_eq!(amiga.denise.spr_pos[0], 0x0000);
    assert_eq!(amiga.denise.spr_ctl[0], 0x0200);
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr_a + 4);

    // Pointer high write alone should not restart the sprite phase machine.
    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr_b >> 16) as u16);
    wait_for_sprite_slot(&mut amiga, 0);
    tick_ccks(&mut amiga, 1); // next DATA slot from stream A

    assert_eq!(
        amiga.denise.spr_pos[0], 0x0000,
        "PTH write alone must not restart fetch at SPR0POS"
    );
    assert_eq!(
        amiga.denise.spr_data[0], 0x9ABC,
        "PTH write alone must preserve DATA phase"
    );
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr_a + 6);

    // Pointer low write commits the new pointer and re-arms control-word fetch.
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr_b & 0xFFFF) as u16);
    wait_for_sprite_slot(&mut amiga, 0);
    tick_ccks(&mut amiga, 1); // should fetch SPR0POS from stream B

    assert_eq!(
        amiga.denise.spr_pos[0], 0x1357,
        "PTL write should restart sprite DMA from new control words"
    );
    assert_eq!(
        amiga.denise.spr_datb[0], 0x4444,
        "PTL-triggered restart should not consume DATB from old stream"
    );
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr_b + 2);
}

#[test]
fn sprite_pointer_high_word_value_is_staged_until_low_commit() {
    let mut amiga = make_test_amiga();
    let spr0_addr_a = 0x0000_3800u32;
    let spr0_addr_b = 0x0001_3840u32; // different high word to prove address latch timing

    write_sprite_words(&mut amiga, spr0_addr_a, &[0x0000, 0x0200, 0x9ABC, 0xDEF0]);
    write_sprite_words(&mut amiga, spr0_addr_b, &[0x1357, 0x2468, 0xAAAA, 0xBBBB]);

    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr_a >> 16) as u16);
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr_a & 0xFFFF) as u16);
    amiga.denise.spr_data[0] = 0x3333;
    amiga.denise.spr_datb[0] = 0x4444;

    amiga.agnus.vpos = 0;
    amiga.agnus.hpos = 0x0A;
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_SPREN);

    tick_ccks(&mut amiga, 1); // -> hpos 0x0B
    tick_ccks(&mut amiga, 1); // POS
    tick_ccks(&mut amiga, 1); // CTL
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr_a + 4);

    // Stage only the high word for stream B; effective DMA pointer must stay on stream A.
    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr_b >> 16) as u16);
    assert_eq!(
        amiga.agnus.spr_pt[0],
        spr0_addr_a + 4,
        "SPRxPTH should stage the new high word without changing the active DMA pointer"
    );

    wait_for_sprite_slot(&mut amiga, 0);
    tick_ccks(&mut amiga, 1); // DATA from stream A
    assert_eq!(amiga.denise.spr_data[0], 0x9ABC);
    assert_eq!(
        amiga.agnus.spr_pt[0],
        spr0_addr_a + 6,
        "staged high word must not redirect the next sprite DMA fetch"
    );

    // Commit stream B pointer on PTL; next slot restarts at B control words.
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr_b & 0xFFFF) as u16);
    assert_eq!(
        amiga.agnus.spr_pt[0], spr0_addr_b,
        "SPRxPTL should commit the staged high word and low word into the active pointer"
    );
    wait_for_sprite_slot(&mut amiga, 0);
    tick_ccks(&mut amiga, 1); // POS from stream B
    assert_eq!(amiga.denise.spr_pos[0], 0x1357);
    assert_eq!(amiga.agnus.spr_pt[0], spr0_addr_b + 2);
}
