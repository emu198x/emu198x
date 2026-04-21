//! Configurable RAM layout tests.
//!
//! Exercises `AmigaOcs::with_ram_config` through the `RamConfig`
//! presets. Fast RAM is currently a no-op at this layer — the
//! Zorro-II autoconfig board that maps it into memory lands in a
//! follow-up commit; these tests only assert the preset round-trips
//! through chip + slow and that out-of-range configs panic as
//! documented.

use machine_commodore_amiga_ocs::{AmigaOcs, RamConfig};

fn zero_rom() -> Vec<u8> {
    vec![0; 512 * 1024]
}

#[test]
fn bare_ramconfig_installs_512k_chip_no_slow() {
    let amiga = AmigaOcs::with_ram_config(zero_rom(), RamConfig::bare());
    assert_eq!(amiga.memory().chip_ram_size(), 512 * 1024);
    assert_eq!(amiga.memory().slow_ram_size(), 0);
}

#[test]
fn a501_trapdoor_preset_installs_512k_chip_plus_512k_slow() {
    let amiga = AmigaOcs::with_ram_config(zero_rom(), RamConfig::a501_trapdoor());
    assert_eq!(amiga.memory().chip_ram_size(), 512 * 1024);
    assert_eq!(amiga.memory().slow_ram_size(), 512 * 1024);
}

#[test]
fn a500plus_preset_installs_1m_chip_no_slow() {
    let amiga = AmigaOcs::with_ram_config(zero_rom(), RamConfig::a500_plus());
    assert_eq!(amiga.memory().chip_ram_size(), 1024 * 1024);
    assert_eq!(amiga.memory().slow_ram_size(), 0);
}

#[test]
fn a500_maxed_preset_installs_1m_chip_plus_512k_slow_plus_8m_fast() {
    let cfg = RamConfig::a500_maxed();
    assert_eq!(cfg.fast_kb, 8192);
    let amiga = AmigaOcs::with_ram_config(zero_rom(), cfg);
    assert_eq!(amiga.memory().chip_ram_size(), 1024 * 1024);
    assert_eq!(amiga.memory().slow_ram_size(), 512 * 1024);
    // Fast RAM wiring lands in the follow-up autoconfig commit; the
    // Memory layer doesn't surface fast RAM directly.
}

#[test]
fn with_slow_ram_is_thin_wrapper_over_with_ram_config() {
    let a = AmigaOcs::with_slow_ram(zero_rom(), 512 * 1024);
    let b = AmigaOcs::with_ram_config(
        zero_rom(),
        RamConfig { chip_kb: 512, slow_kb: 512, fast_kb: 0 },
    );
    assert_eq!(a.memory().chip_ram_size(), b.memory().chip_ram_size());
    assert_eq!(a.memory().slow_ram_size(), b.memory().slow_ram_size());
}

#[test]
fn new_is_thin_wrapper_over_bare_config() {
    let a = AmigaOcs::new(zero_rom());
    let b = AmigaOcs::with_ram_config(zero_rom(), RamConfig::bare());
    assert_eq!(a.memory().chip_ram_size(), b.memory().chip_ram_size());
    assert_eq!(a.memory().slow_ram_size(), b.memory().slow_ram_size());
}

#[test]
fn ram_config_validates_fast_kb_multiple_of_64() {
    let bad = RamConfig { chip_kb: 512, slow_kb: 0, fast_kb: 100 };
    assert!(!bad.is_valid(), "100 KiB is not a multiple of 64");
    let ok = RamConfig { chip_kb: 512, slow_kb: 0, fast_kb: 128 };
    assert!(ok.is_valid());
}

#[test]
fn ram_config_rejects_fast_kb_above_8m() {
    let bad = RamConfig { chip_kb: 512, slow_kb: 0, fast_kb: 16384 };
    assert!(!bad.is_valid(), "Zorro-II single-board max is 8 MiB");
}

#[test]
#[should_panic(expected = "RamConfig out of range")]
fn with_ram_config_panics_on_invalid_sizes() {
    let bad = RamConfig { chip_kb: 384, slow_kb: 0, fast_kb: 0 };
    let _ = AmigaOcs::with_ram_config(zero_rom(), bad);
}
