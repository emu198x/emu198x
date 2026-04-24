//! Runtime-level tests for the RAM-variant presets.
//!
//! The runtime exposes four `Model` presets that map onto A500-family
//! RAM layouts: stock, A501-trapdoor, A500+, and a maxed A500 with
//! Zorro-II fast RAM. These tests confirm each preset wires the
//! correct `RamConfig` through to the machine and that the fast-RAM
//! path stands up an autoconfig board visible to the ROM.
//!
//! The fast-RAM boot test is hermetic until it finds a Kickstart 1.3
//! ROM at `~/.emu198x/roms/commodore-amiga/kick13.rom` — same path
//! the machine-layer boot tests use. When the ROM is absent the test
//! prints a skip marker and returns; CI without the ROM still runs
//! the rest of the suite.

use std::error::Error;
use std::path::PathBuf;

use runtime_commodore_amiga::{AmigaRuntime, Model, RamConfig};

fn blank_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 256 * 1024];
    // Minimal reset vector so `AmigaOcs::new` doesn't blow up on
    // startup. Same shape as the runtime's own dummy_kickstart.
    kickstart[0] = 0x00;
    kickstart[1] = 0x08;
    kickstart[2] = 0x00;
    kickstart[3] = 0x00;
    kickstart[4] = 0x00;
    kickstart[5] = 0xF8;
    kickstart[6] = 0x00;
    kickstart[7] = 0x08;
    kickstart[8] = 0x60;
    kickstart[9] = 0xFE;
    kickstart
}

fn load_kickstart_13() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    std::fs::read(path).ok()
}

#[test]
fn stock_a500_preset_has_no_fast_ram_board() -> Result<(), Box<dyn Error>> {
    let rt = AmigaRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    assert_eq!(rt.ram_config(), RamConfig::bare());
    assert_eq!(rt.machine().memory().chip_ram_size(), 512 * 1024);
    assert_eq!(rt.machine().memory().slow_ram_size(), 0);
    assert!(rt.machine().autoconfig().is_none());
    Ok(())
}

#[test]
fn a501_trapdoor_preset_installs_slow_ram() -> Result<(), Box<dyn Error>> {
    let rt = AmigaRuntime::new(Model::A500OcsPalA501, blank_kickstart())?;
    assert_eq!(rt.ram_config(), RamConfig::a501_trapdoor());
    assert_eq!(rt.machine().memory().chip_ram_size(), 512 * 1024);
    assert_eq!(rt.machine().memory().slow_ram_size(), 512 * 1024);
    assert!(rt.machine().autoconfig().is_none());
    Ok(())
}

#[test]
fn a500_plus_preset_installs_1m_chip() -> Result<(), Box<dyn Error>> {
    let rt = AmigaRuntime::new(Model::A500PlusOcsPal, blank_kickstart())?;
    assert_eq!(rt.ram_config(), RamConfig::a500_plus());
    assert_eq!(rt.machine().memory().chip_ram_size(), 1024 * 1024);
    assert_eq!(rt.machine().memory().slow_ram_size(), 0);
    assert!(rt.machine().autoconfig().is_none());
    Ok(())
}

#[test]
fn maxed_a500_preset_attaches_8m_fast_ram_board() -> Result<(), Box<dyn Error>> {
    let rt = AmigaRuntime::new(Model::A500OcsPalMaxed, blank_kickstart())?;
    assert_eq!(rt.ram_config(), RamConfig::a500_maxed());
    assert_eq!(rt.machine().memory().chip_ram_size(), 1024 * 1024);
    assert_eq!(rt.machine().memory().slow_ram_size(), 512 * 1024);
    let board = rt.machine().autoconfig().expect("fast-RAM board attached");
    assert_eq!(board.ram_size(), 8 * 1024 * 1024);
    assert!(board.visible_in_probe_window());
    assert!(board.base().is_none());
    Ok(())
}

#[test]
fn with_ram_config_accepts_custom_layout() -> Result<(), Box<dyn Error>> {
    // Custom 2M fast-RAM layout outside the Model presets. Profile
    // metadata still tracks A500OcsPal — the model is decoupled from
    // the RAM layout when `with_ram_config` is used.
    let rt = AmigaRuntime::with_ram_config(
        Model::A500OcsPal,
        blank_kickstart(),
        RamConfig {
            chip_kb: 512,
            slow_kb: 0,
            fast_kb: 2048,
        },
    )?;
    assert_eq!(rt.model(), Model::A500OcsPal);
    let board = rt
        .machine()
        .autoconfig()
        .expect("2M fast-RAM board attached");
    assert_eq!(board.ram_size(), 2 * 1024 * 1024);
    Ok(())
}

/// End-to-end integration: the maxed A500 preset boots Kickstart
/// 1.3 and its `expansion.library` discovers the Zorro-II fast-RAM
/// board through the standard autoconfig handshake. After boot the
/// board transitions to `Configured` with a base address inside the
/// Zorro-II address space ($200000..$A00000).
///
/// Before the byte-read fix to the machine's autoconfig bus path,
/// KS 1.3 couldn't read the ER_TYPE nibble via `move.b`, so
/// ConfigChain short-circuited and the board stayed unconfigured.
///
/// No-op when the Kickstart 1.3 image is absent at
/// `~/.emu198x/roms/commodore-amiga/kick13.rom` — same convention
/// the machine-layer boot tests use.
#[test]
fn kickstart_13_configures_fast_ram_board_during_boot() -> Result<(), Box<dyn Error>> {
    let Some(rom) = load_kickstart_13() else {
        return Ok(());
    };
    let mut rt = AmigaRuntime::new(Model::A500OcsPalMaxed, rom)?;
    // 300 frames mirrors the machine-level boot tests — ample time
    // for Kickstart to finish Exec init, run ExpansionInit, and
    // assign the autoconfig base.
    for _ in 0..(300u64 * runtime_commodore_amiga::A500_PAL_FRAME_TICKS) {
        rt.machine_mut().tick();
    }
    let board = rt
        .machine()
        .autoconfig()
        .expect("board should still be present after boot");
    let base = board
        .base()
        .expect("expansion.library should have assigned a base address");
    // Zorro-II boards sit above the chip + slow + ROM region. The
    // exact slot is whatever `expansion.library` picked.
    assert!(
        (0x0020_0000..0x00A0_0000).contains(&base),
        "base $0{base:06X} is outside Zorro-II space"
    );
    // Probe window has gone silent for the configured board —
    // subsequent scans see floating bus.
    assert_eq!(rt.machine().read_word(0x00E8_0000), 0xFFFF);
    Ok(())
}
