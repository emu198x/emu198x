//! Runtime-level tests for A500-family RAM and accelerator profiles.
//!
//! The tests cover stock, A501-trapdoor, A500+, and maxed A500 RAM
//! layouts, plus the A500 + GVP A530 research profile. They confirm
//! each preset wires the correct `RamConfig` through to the machine
//! and that Kickstart can discover both motherboard-bus and
//! accelerator-local RAM through Autoconfig.
//!
//! The ROM-backed boot tests become active when they find Kickstart
//! 1.3 at `~/.emu198x/roms/commodore-amiga/kick13.rom`, the same path
//! used by the machine-layer boot tests. When the ROM is absent they
//! print a skip marker and return; CI without the ROM still runs the
//! rest of the suite.

use std::error::Error;
use std::path::PathBuf;

use motorola_68000::CpuModel;
use runtime_commodore_amiga::{
    AmigaEcsRuntime, AmigaLiveAccess, AmigaOcsRuntime, Model, RamConfig,
};

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
        emu198x_test_skip::record(&format!(
            "skipping: Kickstart 1.3 ROM missing at {}",
            path.display()
        ));
        return None;
    }
    std::fs::read(path).ok()
}

#[test]
fn stock_a500_preset_has_no_fast_ram_board() -> Result<(), Box<dyn Error>> {
    let rt = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    assert_eq!(rt.ram_config(), RamConfig::bare());
    assert_eq!(rt.machine().memory().chip_ram_size(), 512 * 1024);
    assert_eq!(rt.machine().memory().slow_ram_size(), 0);
    assert!(rt.machine().autoconfig().is_none());
    Ok(())
}

#[test]
fn a501_trapdoor_preset_installs_slow_ram() -> Result<(), Box<dyn Error>> {
    let rt = AmigaOcsRuntime::new(Model::A500OcsPalA501, blank_kickstart())?;
    assert_eq!(rt.ram_config(), RamConfig::a501_trapdoor());
    assert_eq!(rt.machine().memory().chip_ram_size(), 512 * 1024);
    assert_eq!(rt.machine().memory().slow_ram_size(), 512 * 1024);
    assert!(rt.machine().autoconfig().is_none());
    Ok(())
}

#[test]
fn a500_plus_preset_installs_1m_chip() -> Result<(), Box<dyn Error>> {
    let rt = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    assert_eq!(rt.ram_config(), RamConfig::a500_plus());
    assert_eq!(rt.machine().memory().chip_ram_size(), 1024 * 1024);
    assert_eq!(rt.machine().memory().slow_ram_size(), 0);
    assert!(rt.machine().autoconfig().is_none());
    Ok(())
}

#[test]
fn maxed_a500_preset_attaches_8m_fast_ram_board() -> Result<(), Box<dyn Error>> {
    let rt = AmigaOcsRuntime::new(Model::A500OcsPalMaxed, blank_kickstart())?;
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
    let rt = AmigaOcsRuntime::with_ram_config(
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
    let mut rt = AmigaOcsRuntime::new(Model::A500OcsPalMaxed, rom)?;
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

/// End-to-end integration: the shipped A500 + GVP A530 profile boots
/// Kickstart 1.3 on its 40 MHz MC68EC030 and `expansion.library`
/// assigns the accelerator's local-RAM memory function a Zorro-II
/// base address.
///
/// The loop exits as soon as the board is configured. Its cap is a
/// regression bound rather than a fixed boot duration, keeping the
/// test focused on the first Autoconfig pass.
///
/// No-op when the Kickstart 1.3 image is absent at
/// `~/.emu198x/roms/commodore-amiga/kick13.rom`.
#[test]
fn kickstart_13_configures_a530_local_ram_during_boot() -> Result<(), Box<dyn Error>> {
    let Some(rom) = load_kickstart_13() else {
        return Ok(());
    };
    let mut rt = AmigaOcsRuntime::new(Model::A500OcsPalGvpA530, rom)?;
    let board = rt
        .machine()
        .gvp_a530()
        .expect("A530 profile must install its accelerator");
    assert_eq!(rt.machine().active_cpu().model(), CpuModel::M68EC030);
    assert_eq!(rt.config().cpu().clock_hz(), 40_000_000);
    assert_eq!(board.ram_size(), 1024 * 1024);
    assert!(board.configuration_is_coherent());
    assert_eq!(board.mapped_base(), None);

    let instruction_starts_before = rt.machine().cpu_instruction_starts();
    const MAX_BOOT_FRAMES: u64 = 75;
    let max_ticks = MAX_BOOT_FRAMES * runtime_commodore_amiga::A500_PAL_FRAME_TICKS;
    let mut configured_at_tick = None;
    for tick in 1..=max_ticks {
        rt.machine_mut().tick();
        if rt
            .machine()
            .gvp_a530()
            .and_then(|board| board.mapped_base())
            .is_some()
        {
            configured_at_tick = Some(tick);
            break;
        }
    }

    let instruction_starts = rt
        .machine()
        .cpu_instruction_starts()
        .wrapping_sub(instruction_starts_before);
    assert!(
        instruction_starts >= 1_000,
        "A530 CPU made insufficient boot progress: {instruction_starts} instruction starts"
    );
    let configured_at_tick = configured_at_tick.unwrap_or_else(|| {
        panic!(
            "Kickstart 1.3 did not configure A530 local RAM within {MAX_BOOT_FRAMES} PAL frames \
             ({max_ticks} system ticks, {instruction_starts} instruction starts)"
        )
    });
    let board = rt
        .machine()
        .gvp_a530()
        .expect("A530 board must remain installed after Autoconfig");
    let base = board
        .mapped_base()
        .expect("configured A530 local RAM must have a base address");
    let end = base
        .checked_add(board.ram_size())
        .expect("coherent A530 mapping must not overflow");
    assert_eq!(base & 0x0000_FFFF, 0, "Zorro-II base must be 64K-aligned");
    assert!(
        base >= 0x0020_0000 && end <= 0x00A0_0000,
        "A530 Fast RAM ${base:06X}-${end:06X} must occupy the dedicated Zorro-II memory space"
    );
    assert!(board.contains_mapped_address(base));
    assert!(board.contains_mapped_address(end - 1));
    assert!(!board.contains_mapped_address(end));
    assert!(board.configuration_is_coherent());
    assert_eq!(rt.machine().read_word(0x00E8_0000), 0xFFFF);

    let instruction_starts_at_config = rt.machine().cpu_instruction_starts();
    for _ in 0..10_000 {
        rt.machine_mut().tick();
    }
    assert!(
        rt.machine()
            .cpu_instruction_starts()
            .wrapping_sub(instruction_starts_at_config)
            > 0,
        "the A530 CPU must continue executing after its local RAM is mapped"
    );
    assert_eq!(
        rt.machine()
            .gvp_a530()
            .expect("A530 remains installed")
            .mapped_base(),
        Some(base)
    );
    eprintln!(
        "A530 local RAM configured at ${base:06X} after {configured_at_tick} system ticks \
         ({instruction_starts} instruction starts)"
    );
    Ok(())
}
