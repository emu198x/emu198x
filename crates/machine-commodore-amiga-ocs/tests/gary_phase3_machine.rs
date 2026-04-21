//! Phase 3 machine-level integration tests for Gary.
//!
//! Closes task #178 — final Gary port milestone. Verifies the
//! machine's `chip_select(addr)` decodes representative addresses
//! to the right region for the A500 + slow-RAM config it's built
//! with.

use machine_commodore_amiga_ocs::{AmigaOcs, ChipSelect};

fn zero_rom() -> Vec<u8> {
    vec![0; 512 * 1024]
}

#[test]
fn machine_decodes_chip_ram_rom_cia_custom_correctly() {
    let amiga = AmigaOcs::new(zero_rom());

    // Chip RAM: $000000 - $1FFFFF.
    assert_eq!(amiga.chip_select(0x00_0000), ChipSelect::ChipRam);
    assert_eq!(amiga.chip_select(0x1F_FFFF), ChipSelect::ChipRam);

    // CIA-B at $BFDxxx, CIA-A at $BFExxx.
    assert_eq!(amiga.chip_select(0xBF_D000), ChipSelect::CiaB);
    assert_eq!(amiga.chip_select(0xBF_E001), ChipSelect::CiaA);

    // Slow RAM $C00000 - $D7FFFF, extending up to $DFFFFF on A500
    // (minus custom / CIA shadows, handled by higher-priority rules).
    assert_eq!(amiga.chip_select(0xC0_0000), ChipSelect::SlowRam);
    assert_eq!(amiga.chip_select(0xD7_FFFF), ChipSelect::SlowRam);

    // Custom chip registers shadow slow RAM at $DFFxxx.
    assert_eq!(amiga.chip_select(0xDF_F000), ChipSelect::Custom);
    assert_eq!(amiga.chip_select(0xDF_F1FE), ChipSelect::Custom);

    // ROM at $F80000 - $FFFFFF.
    assert_eq!(amiga.chip_select(0xF8_0000), ChipSelect::Rom);
    assert_eq!(amiga.chip_select(0xFF_FFFF), ChipSelect::Rom);
}

#[test]
fn machine_gary_is_configured_for_a500_with_slow_ram() {
    let amiga = AmigaOcs::new(zero_rom());
    let gary = amiga.gary();
    assert!(gary.slow_ram_present(),
        "A500 machine should enable the slow-RAM chip select");
    assert!(!gary.gayle_present(), "no Gayle on A500");
    assert!(!gary.dmac_present(), "no DMAC on A500");
    assert!(!gary.resource_regs_present(),
        "no A3000 resource registers on A500");
}

#[test]
fn machine_poke_word_routes_custom_registers_via_gary() {
    // $DFF180 is COLOR00 — writing should land in Denise's palette.
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F180, 0x0F0F);
    assert_eq!(amiga.color(0), 0x0F0F,
        "COLOR00 write via poke_word should reach Denise through Gary's Custom decode");
}

#[test]
fn machine_poke_word_routes_chip_ram_writes_via_memory() {
    // $00_2000 is plain chip RAM once the OVL overlay is cleared.
    let mut amiga = AmigaOcs::new(zero_rom());
    // Clear OVL: DDRA bit 0 = output, PRA bit 0 = 0.
    amiga.poke_byte(0x00BF_E201, 0x03);
    amiga.poke_byte(0x00BF_E001, 0x00);
    assert!(!amiga.memory().overlay());

    amiga.poke_word(0x0000_2000, 0xBEEF);
    assert_eq!(amiga.read_word(0x0000_2000), 0xBEEF);
}
