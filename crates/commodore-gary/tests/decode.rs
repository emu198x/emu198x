//! Phase 1 characterisation — Gary address-decode truth tables.
//!
//! Covers task #176. Exercises the archive's `Gary::decode` through
//! the public API for the three Amiga configs the port cares about:
//!   - **A500 + slow RAM** — the live machine's target
//!   - **A1200 (Gayle + slow RAM)** — future AGA port
//!   - **A3000 (DMAC + resource registers)** — future higher-end port
//!
//! Each test locks in the decode priority order so Phase 2's machine
//! wiring can swap inline bit tests for `gary.decode()` without
//! observable change.

use commodore_gary::{ChipSelect, Gary};

fn gary_a500_with_slow_ram() -> Gary {
    let mut g = Gary::new();
    g.set_slow_ram_present(true);
    g
}

fn gary_a1200() -> Gary {
    let mut g = Gary::new();
    g.set_gayle_present(true);
    g.set_slow_ram_present(true);
    g
}

fn gary_a3000() -> Gary {
    let mut g = Gary::new();
    g.set_dmac_present(true);
    g.set_resource_regs_present(true);
    g
}

/// Simple helper to test a full truth table and produce informative
/// diffs on failure.
fn assert_table(label: &str, gary: &Gary, cases: &[(u32, ChipSelect)]) {
    for &(addr, expected) in cases {
        let actual = gary.decode(addr);
        assert_eq!(
            actual, expected,
            "{label}: ${addr:06X} expected {expected:?}, got {actual:?}",
        );
    }
}

#[test]
fn a500_truth_table_matches_hrm_memory_map() {
    let gary = gary_a500_with_slow_ram();
    assert_table(
        "A500",
        &gary,
        &[
            // Chip RAM
            (0x00_0000, ChipSelect::ChipRam),
            (0x08_0000, ChipSelect::ChipRam),
            (0x1F_FFFF, ChipSelect::ChipRam),
            // Expansion gap (unmapped on stock A500)
            (0x20_0000, ChipSelect::Unmapped),
            (0x9F_FFFF, ChipSelect::Unmapped),
            // Ranger gap
            (0xA0_0000, ChipSelect::Unmapped),
            (0xBF_0000, ChipSelect::Unmapped),
            // CIA-B / CIA-A
            (0xBF_D000, ChipSelect::CiaB),
            (0xBF_DF00, ChipSelect::CiaB),
            (0xBF_E001, ChipSelect::CiaA),
            (0xBF_EF01, ChipSelect::CiaA),
            // Slow RAM
            (0xC0_0000, ChipSelect::SlowRam),
            (0xD7_FFFF, ChipSelect::SlowRam),
            (0xD8_0000, ChipSelect::SlowRam),
            // Custom shadows the slow-RAM range
            (0xDF_F000, ChipSelect::Custom),
            (0xDF_F1FE, ChipSelect::Custom),
            // Diagnostics gap
            (0xE0_0000, ChipSelect::Unmapped),
            (0xE7_FFFF, ChipSelect::Unmapped),
            // Autoconfig
            (0xE8_0000, ChipSelect::Autoconfig),
            (0xEF_FFFF, ChipSelect::Autoconfig),
            // Diagnostics gap
            (0xF0_0000, ChipSelect::Unmapped),
            (0xF7_FFFF, ChipSelect::Unmapped),
            // ROM
            (0xF8_0000, ChipSelect::Rom),
            (0xFF_FFFF, ChipSelect::Rom),
        ],
    );
}

#[test]
fn a1200_gayle_truncates_slow_ram_and_is_shadowed_by_custom() {
    let gary = gary_a1200();
    assert_table(
        "A1200",
        &gary,
        &[
            // Slow RAM only up to $D7FFFF; $D80000+ is Gayle.
            (0xC0_0000, ChipSelect::SlowRam),
            (0xD7_FFFF, ChipSelect::SlowRam),
            (0xD8_0000, ChipSelect::Gayle),
            (0xDE_FFFF, ChipSelect::Gayle),
            // Custom still wins inside Gayle's range.
            (0xDF_F000, ChipSelect::Custom),
        ],
    );
}

#[test]
fn a3000_dmac_and_resource_regs_decode_ahead_of_slow_ram() {
    let gary = gary_a3000();
    assert_table(
        "A3000",
        &gary,
        &[
            // No slow RAM by default
            (0xC0_0000, ChipSelect::Unmapped),
            (0xDC_FFFF, ChipSelect::Unmapped),
            // DMAC
            (0xDD_0000, ChipSelect::Dmac),
            (0xDD_FFFF, ChipSelect::Dmac),
            // Resource registers
            (0xDE_0000, ChipSelect::ResourceRegisters),
            (0xDE_FFFF, ChipSelect::ResourceRegisters),
            // Custom
            (0xDF_F000, ChipSelect::Custom),
        ],
    );
}

#[test]
fn address_is_truncated_to_24_bits() {
    // A 68020 system running in 24-bit-gate mode feeds 32-bit
    // addresses into Gary; the top byte must be ignored.
    let gary = Gary::new();
    assert_eq!(
        gary.decode(0x0100_0000),
        ChipSelect::ChipRam,
        "$01000000 should alias to $000000"
    );
    assert_eq!(
        gary.decode(0x01BF_E001),
        ChipSelect::CiaA,
        "$01BFE001 should alias to CIA-A"
    );
}

#[test]
fn cias_shadow_slow_ram_at_bfc_bfd_bfe_ranges() {
    let gary = gary_a500_with_slow_ram();
    // CIA-B + CIA-A sit inside what would otherwise be slow-RAM-
    // adjacent $BFxxxx. Gary must pick the CIA decode.
    assert_eq!(gary.decode(0xBF_D000), ChipSelect::CiaB);
    assert_eq!(gary.decode(0xBF_E001), ChipSelect::CiaA);
}

#[test]
fn rtc_decode_is_bounded_to_sixty_four_bytes() {
    let mut gary = Gary::new();
    gary.set_rtc_present(true);
    assert_eq!(gary.decode(0xDC_0000), ChipSelect::Rtc);
    assert_eq!(gary.decode(0xDC_003F), ChipSelect::Rtc);
    // $DC_0040 falls outside the RTC register window.
    assert_eq!(gary.decode(0xDC_0040), ChipSelect::Unmapped);
}

#[test]
fn default_gary_has_no_optional_peripherals() {
    let gary = Gary::default();
    assert!(!gary.slow_ram_present());
    assert!(!gary.gayle_present());
    assert!(!gary.pcmcia_present());
    assert!(!gary.dmac_present());
    assert!(!gary.resource_regs_present());
    assert!(!gary.rtc_present());
}
