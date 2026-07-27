//! End-to-end Agnus startup timing through the OCS machine driver.

use machine_commodore_amiga_ocs::{AmigaOcs, RamConfig};

const DMACONR: u32 = 0x00DF_F002;
const INTREQR: u32 = 0x00DF_F01E;
const BLTCON0: u32 = 0x00DF_F040;
const BLTSIZE: u32 = 0x00DF_F058;
const COP1LCH: u32 = 0x00DF_F080;
const COP1LCL: u32 = 0x00DF_F082;
const COPJMP1: u32 = 0x00DF_F088;
const DMACON: u32 = 0x00DF_F096;
const BBUSY: u16 = 0x4000;
const INT_BLIT: u16 = 0x0040;

fn parked_kickstart() -> Vec<u8> {
    let mut rom = vec![0u8; 512 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S *
    rom[9] = 0xFE;
    rom
}

fn parked_a1000_bootstrap() -> Vec<u8> {
    let mut rom = vec![0u8; 64 * 1024];
    rom[0] = 0x11;
    rom[1] = 0x11;
    rom[2] = 0x4E; // JMP $F80008
    rom[3] = 0xF9;
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S *
    rom[9] = 0xFE;
    rom
}

fn start_one_internal_operation(amiga: &mut AmigaOcs) {
    amiga.poke_word(BLTCON0, 0x0000);
    amiga.poke_word(BLTSIZE, (1 << 6) | 1);
}

fn install_matching_bfd0_skip(amiga: &mut AmigaOcs) {
    // At beam position >= 0:
    //   SKIP next instruction if the externally visible blitter is idle
    //   MOVE COLOR00, $0F00
    //   END
    amiga.poke_word(0x0000_1000, 0x0001);
    amiga.poke_word(0x0000_1002, 0x7FFF);
    amiga.poke_word(0x0000_1004, 0x0180);
    amiga.poke_word(0x0000_1006, 0x0F00);
    amiga.poke_word(0x0000_1008, 0xFFFF);
    amiga.poke_word(0x0000_100A, 0xFFFE);
    amiga.poke_word(COP1LCH, 0x0000);
    amiga.poke_word(COP1LCL, 0x1000);
    amiga.poke_word(COPJMP1, 0);
}

#[test]
fn a1000_machine_hides_busy_until_first_grant_without_completing_or_interrupting() {
    let a1000_ram = RamConfig {
        chip_kb: 256,
        slow_kb: 0,
        fast_kb: 0,
    };
    let mut a1000 = AmigaOcs::with_a1000_bootstrap_rom(parked_a1000_bootstrap(), a1000_ram);
    let mut later = AmigaOcs::with_ram_config(parked_kickstart(), RamConfig::bare());

    start_one_internal_operation(&mut a1000);
    start_one_internal_operation(&mut later);

    assert!(a1000.agnus().blitter_busy);
    assert_eq!(a1000.read_word(DMACONR) & BBUSY, 0);
    assert_eq!(a1000.agnus().blitter_startup_ccks_remaining(), 2);
    assert_ne!(
        later.read_word(DMACONR) & BBUSY,
        0,
        "later original Agnus exposes busy immediately even with DMA disabled",
    );
    assert_eq!(later.agnus().blitter_startup_ccks_remaining(), 2);

    for _ in 0..32 {
        a1000.tick();
    }
    assert_eq!(
        a1000.agnus().blitter_startup_ccks_remaining(),
        2,
        "DMA-disabled ticks must not enter startup",
    );
    assert_eq!(a1000.read_word(DMACONR) & BBUSY, 0);
    assert_eq!(a1000.read_word(INTREQR) & INT_BLIT, 0);

    // SETCLR | DMAEN | BLTEN | BLTPRI. Nasty mode makes the free-slot grant
    // unambiguous in the current machine scheduler.
    a1000.poke_word(DMACON, 0x8640);
    let mut guard = 0;
    while a1000.agnus().blitter_startup_ccks_remaining() == 2 {
        a1000.tick();
        guard += 1;
        assert!(guard < 1_000, "A1000 never accepted its first startup CCK");
    }

    assert_eq!(a1000.agnus().blitter_startup_ccks_remaining(), 1);
    assert_ne!(a1000.read_word(DMACONR) & BBUSY, 0);
    assert!(a1000.agnus().blitter_busy);
    assert_eq!(
        a1000.agnus().blitter_ccks_remaining,
        1,
        "first startup CCK must not consume the internal operation",
    );
    assert_eq!(
        a1000.read_word(INTREQR) & INT_BLIT,
        0,
        "startup-only CCK must not raise the completion interrupt",
    );

    while a1000.agnus().blitter_busy {
        a1000.tick();
        guard += 1;
        assert!(guard < 2_000, "A1000 one-operation blit never completed");
    }
    while a1000.read_word(DMACONR) & BBUSY != 0 {
        a1000.tick();
        guard += 1;
        assert!(guard < 2_000, "A1000 DMACONR completion hold never cleared");
    }
    assert_eq!(a1000.read_word(DMACONR) & BBUSY, 0);
    assert_ne!(a1000.read_word(INTREQR) & INT_BLIT, 0);
}

#[test]
fn machine_driver_feeds_revision_visible_busy_to_copper_bfd() {
    let a1000_ram = RamConfig {
        chip_kb: 256,
        slow_kb: 0,
        fast_kb: 0,
    };
    let mut a1000 = AmigaOcs::with_a1000_bootstrap_rom(parked_a1000_bootstrap(), a1000_ram);
    let mut later = AmigaOcs::with_ram_config(parked_kickstart(), RamConfig::bare());

    for amiga in [&mut a1000, &mut later] {
        start_one_internal_operation(amiga);
        install_matching_bfd0_skip(amiga);
        // SETCLR | DMAEN | COPEN. BLTEN deliberately remains clear so neither
        // machine accepts a startup CCK while the Copper evaluates its SKIP.
        amiga.poke_word(DMACON, 0x8280);
        for _ in 0..64 {
            amiga.tick();
        }
        assert_eq!(amiga.agnus().blitter_startup_ccks_remaining(), 2);
    }

    assert_eq!(
        a1000.color(0),
        0,
        "A1000-visible idle must let a matching BFD=0 SKIP skip the MOVE",
    );
    assert_eq!(
        later.color(0) & 0x0FFF,
        0x0F00,
        "later original Agnus exposes busy at BLTSIZE, so BFD=0 must retain the MOVE",
    );
}

#[test]
fn a1000_first_startup_cck_changes_pending_copper_skip_decision() {
    let ram = RamConfig {
        chip_kb: 256,
        slow_kb: 0,
        fast_kb: 0,
    };
    let mut amiga = AmigaOcs::with_a1000_bootstrap_rom(parked_a1000_bootstrap(), ram);
    start_one_internal_operation(&mut amiga);
    install_matching_bfd0_skip(&mut amiga);
    amiga.poke_word(DMACON, 0x8280); // SETCLR | DMAEN | COPEN; BLTEN remains clear

    let mut guard = 0;
    while !amiga.copper().pending_wait_delay {
        amiga.tick();
        guard += 1;
        assert!(guard < 1_000, "Copper never decoded the BFD=0 SKIP");
    }
    assert!(amiga.copper().pending_wait_is_skip);
    assert_eq!(amiga.copper().pc, 0x1004);
    assert_eq!(amiga.agnus().blitter_startup_ccks_remaining(), 2);
    assert!(!amiga.agnus().blitter_busy_visible());

    // Hold Copper DMA after decode while admitting a blitter grant. This
    // isolates the architectural boundary from the current coarse model's
    // same-CCK Copper/blitter ordering.
    amiga.poke_word(DMACON, 0x0080); // clear COPEN
    amiga.poke_word(DMACON, 0x8440);
    while amiga.agnus().blitter_startup_ccks_remaining() == 2 {
        amiga.tick();
        guard += 1;
        assert!(guard < 2_000, "A1000 never accepted its first startup CCK");
    }
    assert!(amiga.agnus().blitter_busy_visible());
    assert!(
        amiga.copper().pending_wait_delay,
        "the SKIP comparison must remain pending across the intervening blitter grant",
    );

    amiga.poke_word(DMACON, 0x8080); // SETCLR | COPEN
    for _ in 0..64 {
        amiga.tick();
    }
    assert_eq!(
        amiga.color(0) & 0x0FFF,
        0x0F00,
        "SKIP decoded while A1000 BBUSY was hidden must retain the MOVE when BBUSY asserts before comparison",
    );
}
