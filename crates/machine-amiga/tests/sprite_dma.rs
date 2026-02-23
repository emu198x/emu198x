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
