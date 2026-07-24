//! Integration coverage for Fat Agnus 8372A paired with OCS Denise.
//!
//! The A2000/A500-upgrade configuration is intentionally a mixed chipset:
//! ECS Agnus register behavior must be available without silently upgrading
//! the display chip to Super Denise.

use machine_commodore_amiga_ocs::{AmigaOcs, RamConfig};

const CUSTOM_BASE: u32 = 0x00DF_F000;
const DMACONR: u32 = CUSTOM_BASE + 0x002;
const DENISEID: u32 = CUSTOM_BASE + 0x07C;
const DMACON: u32 = CUSTOM_BASE + 0x096;
const BLTCON0: u32 = CUSTOM_BASE + 0x040;
const BLTCON1: u32 = CUSTOM_BASE + 0x042;
const BLTDPTH: u32 = CUSTOM_BASE + 0x054;
const BLTDPTL: u32 = CUSTOM_BASE + 0x056;
const BLTCON0L: u32 = CUSTOM_BASE + 0x05A;
const BLTSIZV: u32 = CUSTOM_BASE + 0x05C;
const BLTSIZH: u32 = CUSTOM_BASE + 0x05E;
const DIWSTRT: u32 = CUSTOM_BASE + 0x08E;
const DIWSTOP: u32 = CUSTOM_BASE + 0x090;
const DDFSTRT: u32 = CUSTOM_BASE + 0x092;
const DDFSTOP: u32 = CUSTOM_BASE + 0x094;
const BPLCON0: u32 = CUSTOM_BASE + 0x100;
const BPLCON3: u32 = CUSTOM_BASE + 0x106;
const BPL1PTH: u32 = CUSTOM_BASE + 0x0E0;
const BPL1PTL: u32 = CUSTOM_BASE + 0x0E2;
const SPR0PTH: u32 = CUSTOM_BASE + 0x120;
const SPR0PTL: u32 = CUSTOM_BASE + 0x122;
const SPR0POS: u32 = CUSTOM_BASE + 0x140;
const SPR0CTL: u32 = CUSTOM_BASE + 0x142;
const SPR1POS: u32 = CUSTOM_BASE + 0x148;
const SPR1CTL: u32 = CUSTOM_BASE + 0x14A;
const HTOTAL: u32 = CUSTOM_BASE + 0x1C0;
const VTOTAL: u32 = CUSTOM_BASE + 0x1C8;
const VBSTRT: u32 = CUSTOM_BASE + 0x1CC;
const VBSTOP: u32 = CUSTOM_BASE + 0x1CE;
const BEAMCON0: u32 = CUSTOM_BASE + 0x1DC;
const DIWHIGH: u32 = CUSTOM_BASE + 0x1E4;

const BBUSY: u16 = 0x4000;
const DMA_SET_MASTER_AND_BLITTER: u16 = 0x8240;
const BEAMCON0_VARBEAMEN_PAL: u16 = 0x00A0;

fn parked_cpu_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2: branch to self
    rom
}

fn early_ocs_machine() -> AmigaOcs {
    AmigaOcs::with_ram_config(parked_cpu_rom(), RamConfig::bare())
}

fn fat_agnus_machine() -> AmigaOcs {
    AmigaOcs::with_fat_agnus_ram_config(parked_cpu_rom(), RamConfig::bare())
}

fn fat_agnus_ntsc_machine() -> AmigaOcs {
    AmigaOcs::with_fat_agnus_ram_config_ntsc(parked_cpu_rom(), RamConfig::bare())
}

fn chip_word(amiga: &AmigaOcs, addr: u32) -> u16 {
    (u16::from(amiga.read_chip_ram_byte(addr)) << 8) | u16::from(amiga.read_chip_ram_byte(addr + 1))
}

fn fill_chip_words(amiga: &mut AmigaOcs, start: u32, words: u32, value: u16) {
    for word in 0..words {
        amiga.poke_word(start + word * 2, value);
    }
}

fn prepare_d_only_clear(amiga: &mut AmigaOcs, destination: u32) {
    amiga.poke_word(DMACON, DMA_SET_MASTER_AND_BLITTER);
    amiga.poke_word(BLTCON0, 0x0100); // USED, minterm $00: write zero.
    amiga.poke_word(BLTCON1, 0x0000);
    amiga.poke_word(BLTDPTH, (destination >> 16) as u16);
    amiga.poke_word(BLTDPTL, destination as u16);
}

fn run_blit_to_completion(amiga: &mut AmigaOcs) -> u32 {
    for ticks in 0..100_000 {
        if amiga.read_word(DMACONR) & BBUSY == 0 {
            return ticks;
        }
        amiga.tick();
    }
    panic!("extended blit did not clear BBUSY within the tick budget");
}

#[test]
fn fat_agnus_completes_extended_blit_while_early_ocs_ignores_the_start() {
    const DESTINATION: u32 = 0x0002_0000;
    const WIDTH_WORDS: u32 = 65;
    const SENTINEL: u16 = 0xA55A;

    let mut early = early_ocs_machine();
    fill_chip_words(&mut early, DESTINATION, WIDTH_WORDS, SENTINEL);
    prepare_d_only_clear(&mut early, DESTINATION);
    early.poke_word(BLTSIZV, 1);
    early.poke_word(BLTSIZH, WIDTH_WORDS as u16);

    assert_eq!(
        early.read_word(DMACONR) & BBUSY,
        0,
        "an early Agnus must ignore the ECS BLTSIZV/BLTSIZH start path"
    );
    assert_eq!(chip_word(&early, DESTINATION), SENTINEL);
    assert_eq!(
        chip_word(&early, DESTINATION + (WIDTH_WORDS - 1) * 2),
        SENTINEL
    );

    let mut fat = fat_agnus_machine();
    fill_chip_words(&mut fat, DESTINATION, WIDTH_WORDS, SENTINEL);
    prepare_d_only_clear(&mut fat, DESTINATION);
    fat.poke_word(BLTSIZV, 1);
    fat.poke_word(BLTSIZH, WIDTH_WORDS as u16);

    assert_ne!(
        fat.read_word(DMACONR) & BBUSY,
        0,
        "BLTSIZH must start the extended blit on Fat Agnus"
    );
    assert!(
        run_blit_to_completion(&mut fat) > 0,
        "the extended blit must consume chip cycles"
    );
    assert_eq!(chip_word(&fat, DESTINATION), 0);
    assert_eq!(
        chip_word(&fat, DESTINATION + (WIDTH_WORDS - 1) * 2),
        0,
        "word 65 proves the width did not wrap through legacy BLTSIZE"
    );
}

#[test]
fn fat_agnus_decodes_bltcon0l_without_exposing_it_on_early_ocs() {
    let mut early = early_ocs_machine();
    let mut fat = fat_agnus_machine();
    for amiga in [&mut early, &mut fat] {
        amiga.poke_word(BLTCON0, 0xABCD);
        amiga.poke_word(BLTCON0L, 0x1256);
    }

    assert_eq!(
        early.agnus().bltcon0,
        0xABCD,
        "early OCS must ignore the ECS low-byte alias"
    );
    assert_eq!(
        fat.agnus().bltcon0,
        0xAB56,
        "Fat Agnus must update only BLTCON0's low byte"
    );
}

#[test]
fn fat_agnus_programmable_beam_does_not_upgrade_ocs_denise() {
    let mut early = early_ocs_machine();
    let mut fat = fat_agnus_machine();

    for amiga in [&mut early, &mut fat] {
        amiga.poke_word(HTOTAL, 3); // Four CCKs per line.
        amiga.poke_word(VTOTAL, 1); // Two lines per field.
        amiga.poke_word(BEAMCON0, BEAMCON0_VARBEAMEN_PAL);
    }

    for _ in 0..16 {
        early.tick();
        fat.tick();
    }

    assert_eq!(
        (fat.agnus().vpos, fat.agnus().hpos),
        (0, 0),
        "Fat Agnus must wrap at the programmed two-line field"
    );
    assert_eq!(fat.agnus().vbl_count, 1);
    assert_eq!(
        (early.agnus().vpos, early.agnus().hpos),
        (0, 8),
        "the same extension-register writes must not alter early OCS timing"
    );
    assert_eq!(early.agnus().vbl_count, 0);

    assert_eq!(
        fat.read_word(DENISEID),
        0xFFFF,
        "the mixed machine must retain OCS Denise's open-bus ID"
    );
    fat.poke_word(BPLCON3, 0x0201);
    let bplcon3_write = fat
        .debug_palette_log
        .last()
        .copied()
        .expect("BPLCON3 writes are exposed by the public diagnostic log");
    assert_eq!((bplcon3_write.2, bplcon3_write.3), (0x0106, 0x0201));
    assert_eq!(
        bplcon3_write.4, None,
        "OCS Denise must not acquire a live ECS BPLCON3 latch"
    );
    assert_eq!(fat.denise().deniseid(), 0xFFFF);
}

#[test]
fn snapshot_restore_preserves_fat_agnus_sticky_bltsizv() {
    const DESTINATION: u32 = 0x0002_4000;
    const HEIGHT: u32 = 2;
    const WIDTH_WORDS: u32 = 65;
    const WORDS: u32 = HEIGHT * WIDTH_WORDS;

    let mut fat = fat_agnus_machine();
    fill_chip_words(&mut fat, DESTINATION, WORDS, 0xFFFF);
    prepare_d_only_clear(&mut fat, DESTINATION);
    fat.poke_word(BLTSIZV, HEIGHT as u16);

    let snapshot = fat.snapshot_state();
    let mut restored = early_ocs_machine();
    restored.restore_snapshot_state(snapshot);

    assert_eq!(
        restored.read_word(0x00DF_F004) & 0x7F00,
        0x2000,
        "restoring must preserve the Fat Agnus variant"
    );
    restored.poke_word(BLTSIZH, WIDTH_WORDS as u16);
    assert_ne!(restored.read_word(DMACONR) & BBUSY, 0);
    run_blit_to_completion(&mut restored);

    assert_eq!(chip_word(&restored, DESTINATION), 0);
    assert_eq!(
        chip_word(&restored, DESTINATION + (WORDS - 1) * 2),
        0,
        "the restored sticky BLTSIZV must carry the blit into its second row"
    );
}

#[test]
fn fat_agnus_diwhigh_gates_bitplane_dma_in_the_mixed_machine() {
    const BITPLANE: u32 = 0x0002_0000;

    let mut early = early_ocs_machine();
    let mut fat = fat_agnus_machine();
    for amiga in [&mut early, &mut fat] {
        amiga.poke_word(DIWSTRT, 0x1010);
        amiga.poke_word(DIWSTOP, 0xA020);
        amiga.poke_word(DDFSTRT, 0x001C);
        amiga.poke_word(DDFSTOP, 0x001C);
        amiga.poke_word(BPLCON0, 0x1000); // One bitplane.
        amiga.poke_word(BPL1PTH, (BITPLANE >> 16) as u16);
        amiga.poke_word(BPL1PTL, BITPLANE as u16);
        amiga.poke_word(DIWHIGH, 0x0101);
        amiga.poke_word(DMACON, 0x8300); // SETCLR | DMAEN | BPLEN.
    }

    let mut guard = 0;
    while fat.agnus().vpos <= 0x20 && guard < 100_000 {
        early.tick();
        fat.tick();
        guard += 1;
    }
    assert!(guard < 100_000, "beam did not reach the test line");

    assert_eq!(
        fat.agnus().bpl_pt[0],
        BITPLANE,
        "DIWHIGH moves Fat Agnus's vertical DMA window above line $20"
    );
    assert!(
        early.agnus().bpl_pt[0] > BITPLANE,
        "early OCS ignores DIWHIGH and fetches inside its legacy window"
    );

    while fat.agnus().vpos <= 0x110 && guard < 250_000 {
        fat.tick();
        guard += 1;
    }
    assert!(guard < 250_000, "beam did not reach the extended window");
    assert!(
        fat.agnus().bpl_pt[0] > BITPLANE,
        "Fat Agnus must fetch once the beam enters the DIWHIGH window"
    );
}

#[test]
fn fat_agnus_programmed_vbstop_drives_sprite_control_fetch() {
    const SPRITE: u32 = 0x0000_2000;

    let mut fat = fat_agnus_machine();
    fat.poke_word(VBSTRT, 300);
    fat.poke_word(VBSTOP, 40);
    fat.poke_word(BEAMCON0, 0x1020); // VARVBEN | PAL.
    fat.poke_word(SPRITE, 0x4100); // VSTART = 65.
    fat.poke_word(SPRITE + 2, 0x5000); // VSTOP = 80.
    fat.poke_word(SPR0PTH, (SPRITE >> 16) as u16);
    fat.poke_word(SPR0PTL, SPRITE as u16);

    // Cross programmed VBSTRT and wrap before enabling sprite DMA.
    // The blank state is an edge-driven latch, not reconstructed from
    // the current beam position when VARVBEN is written.
    let mut guard = 0;
    while fat.agnus().vpos < 300 && guard < 1_000_000 {
        fat.tick();
        guard += 1;
    }
    fat.poke_word(DMACON, 0x8220); // SETCLR | DMAEN | SPREN.
    let field = fat.agnus().vbl_count;
    while fat.agnus().vbl_count == field && guard < 1_000_000 {
        fat.tick();
        guard += 1;
    }
    while fat.agnus().vpos < 40 && guard < 1_000_000 {
        fat.tick();
        guard += 1;
    }
    assert_eq!(
        fat.agnus().spr_pt[0],
        SPRITE,
        "the fixed PAL control-fetch line must be inactive under VARVBEN"
    );

    while fat.agnus().vpos < 41 && guard < 1_000_000 {
        fat.tick();
        guard += 1;
    }
    assert!(guard < 1_000_000, "beam did not cross programmed VBSTOP");
    assert_eq!(fat.agnus().spr_pt[0], SPRITE + 4);
    assert_eq!(fat.agnus().sprite_vstart(0), 65);
    assert_eq!(fat.agnus().sprite_vstop(0), 80);
}

#[test]
fn direct_sprite_writes_use_fat_agnus_programmed_blanking() {
    let mut fat = fat_agnus_machine();
    fat.poke_word(VBSTRT, 20);
    fat.poke_word(VBSTOP, 40);
    fat.poke_word(BEAMCON0, 0x1020); // VARVBEN | PAL.

    let mut guard = 0;
    while fat.agnus().vpos < 30 && guard < 100_000 {
        fat.tick();
        guard += 1;
    }
    assert!(guard < 100_000, "beam did not enter programmed blank");

    // Line 30 is outside fixed PAL vertical blank but inside the
    // programmed 20..40 interval. Exercise both write orders so an
    // accidental deref to either base POS or base CTL handling is caught.
    fat.poke_word(SPR0CTL, 60 << 8);
    assert!(!fat.agnus().sprite_dma_on(0));
    fat.poke_word(SPR0POS, 30 << 8);
    assert!(
        !fat.agnus().sprite_dma_on(0),
        "SPRxPOS must not activate sprite data inside programmed blank"
    );

    fat.poke_word(SPR1POS, 30 << 8);
    assert!(!fat.agnus().sprite_dma_on(1));
    fat.poke_word(SPR1CTL, 60 << 8);
    assert!(
        !fat.agnus().sprite_dma_on(1),
        "SPRxCTL must not activate sprite data inside programmed blank"
    );
}

#[test]
fn ntsc_fat_agnus_keeps_ocs_denise_and_programmable_long_lines() {
    let mut fat = fat_agnus_ntsc_machine();
    assert_eq!(fat.read_word(0x00DF_F004) & 0x7F00, 0x3000);
    assert_eq!(fat.read_word(DENISEID), 0xFFFF);

    fat.poke_word(HTOTAL, 1); // Two-CCK short line.
    fat.poke_word(VTOTAL, 7);
    fat.poke_word(BEAMCON0, 0x0080); // VARBEAMEN, PAL clear.

    for _ in 0..4 {
        fat.tick();
    }
    assert_eq!((fat.agnus().vpos, fat.agnus().hpos), (1, 0));
    assert!(
        fat.agnus().lol,
        "NTSC Fat Agnus must alternate into a long line"
    );

    for _ in 0..4 {
        fat.tick();
    }
    assert_eq!((fat.agnus().vpos, fat.agnus().hpos), (1, 2));
}
