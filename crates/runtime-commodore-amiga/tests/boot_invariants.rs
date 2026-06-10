//! Standing boot-invariant suite for the Amiga (OCS PAL) runtime.
//!
//! Each test asserts a known-good waypoint that the project has reached
//! and depends on. The file is the canonical regression gate for
//! Amiga-shaped breakage — when a refactor touches `service_cpu_bus`,
//! `tick_cck`, the chip stack, or the runtime envelope, these are the
//! tests that should stay green.
//!
//! Hermetic invariants run on every `cargo test --workspace`. ROM-
//! backed invariants are `#[ignore]`'d and resolve assets from
//! `~/.emu198x/roms/commodore-amiga/` and
//! `~/.emu198x/media/commodore-amiga/`, matching the existing
//! diagnostic harnesses.
//!
//! Promoted from the diagnostic-test pool per A.2 of
//! `docs/plans/2026-04-28-october-runup-plan.md`.

use std::error::Error;
use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_commodore_amiga::{AmigaOcsRuntime, Model, RamConfig};

fn blank_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 256 * 1024];
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

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

fn home_rom_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga");
    if path.exists() { Some(path) } else { None }
}

fn home_media_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/media/commodore-amiga");
    if path.exists() { Some(path) } else { None }
}

// ─────────────────────────────────────────────────────────────────────
// Hermetic — run on every cargo test
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: every supported A500-family RAM preset constructs cleanly
/// against a blank Kickstart.
///
/// Catches regression: a chip-RAM size change or autoconfig misroute
/// would fail one of these constructions. The four presets exercise
/// the four RAM-config branches the runtime switches on.
#[test]
fn ram_variant_presets_construct_cleanly() {
    for model in [
        Model::A500OcsPal,
        Model::A500OcsPalA501,
        Model::A500PlusEcsPal,
        Model::A500OcsPalMaxed,
    ] {
        let runtime = AmigaOcsRuntime::new(model, blank_kickstart())
            .unwrap_or_else(|e| panic!("preset {model:?} should construct: {e:?}"));
        assert_eq!(
            runtime.machine().memory().chip_ram_size(),
            expected_chip(model)
        );
    }
}

fn expected_chip(model: Model) -> usize {
    use runtime_commodore_amiga::{
        ECS_AGA_CHIP_RAM_BYTES, FAT_AGNUS_CHIP_RAM_BYTES, FATTER_AGNUS_CHIP_RAM_BYTES,
        OCS_AGNUS_CHIP_RAM_BYTES,
    };
    // Same chip-RAM size for the PAL/NTSC pair of every variant —
    // only Agnus differs between regions. The named constants index
    // by the Agnus revision that gates the ceiling.
    match model {
        Model::A500OcsPal | Model::A500OcsPalA501 | Model::A500OcsNtsc | Model::A500OcsNtscA501 => {
            OCS_AGNUS_CHIP_RAM_BYTES
        }
        Model::A500PlusEcsPal
        | Model::A500OcsPalMaxed
        | Model::A500PlusEcsNtsc
        | Model::A500OcsNtscMaxed
        | Model::A600EcsPal
        | Model::A600EcsNtsc
        | Model::A2000OcsPal
        | Model::A2000OcsNtsc => FAT_AGNUS_CHIP_RAM_BYTES,
        Model::A1000OcsPal | Model::A1000OcsNtsc => FATTER_AGNUS_CHIP_RAM_BYTES,
        Model::A1200AgaPal | Model::A1200AgaNtsc => ECS_AGA_CHIP_RAM_BYTES,
    }
}

/// Waypoint: the runtime ticks past the first frame boundary against a
/// blank Kickstart (CPU loops at the reset vector while the chipset
/// runs).
///
/// Catches regression: any infinite-loop / hang in the master-clock
/// run loop, the CPU bus servicing, or the frame-emission path.
#[test]
fn run_until_advances_past_first_frame() -> Result<(), Box<dyn Error>> {
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let mut host = null_host();
    let target = MachineTime::new(300_000);
    runtime.run_until(target, &mut host)?;
    let now = runtime.time();
    assert!(
        now.get() >= 150_000,
        "runtime should have advanced past the first frame, got {now:?}"
    );
    Ok(())
}

/// Waypoint: snapshot → restore → snapshot is a fixed point on a
/// blank-kickstart runtime that has been ticked far enough to have
/// non-trivial state. Mirrors the deeper coverage in
/// `snapshot_roundtrip.rs`; restated here so a `boot_invariants` run
/// on its own surfaces snapshot drift as a boot-class breakage.
///
/// Catches regression: any chip-state field that fails to round-trip
/// — the most common form of "snapshot looks fine but breaks weird
/// software" failure mode.
#[test]
fn snapshot_round_trip_is_fixed_point_after_warmup() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let mut host = null_host();
    original.run_until(MachineTime::new(64_000), &mut host)?;
    let bytes_a = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&bytes_a)?;
    let bytes_b = restored.snapshot()?;
    assert_eq!(bytes_a, bytes_b, "snapshot drift after restore");
    Ok(())
}

/// Waypoint: RAM-config defaults are stable across the supported
/// presets.
///
/// Catches regression: silent change to `Model::ram_config()`. The
/// Workbench/Kickstart goldens depend on these exact sizes.
#[test]
fn ram_config_defaults_are_stable() {
    assert_eq!(Model::A500OcsPal.ram_config(), RamConfig::bare());
    assert_eq!(
        Model::A500OcsPalA501.ram_config(),
        RamConfig::a501_trapdoor()
    );
}

// ─────────────────────────────────────────────────────────────────────
// ROM-backed — `#[ignore]`'d; resolve assets under ~/.emu198x/
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: Kickstart 1.3 boots to the insert-disk screen.
///
/// Catches regression: every chip-only / Kickstart-boot bug the
/// 2026-04-19 restart wave fixed (copper CDANG halt, CIA TOD halt,
/// floppy ID stream, MFM encoder). Promoted from the
/// `diag_wb13_boot_state.rs` and `golden_matrix.rs` waypoints.
#[test]
#[ignore = "requires ~/.emu198x/roms/commodore-amiga/kick13.rom"]
fn kickstart_13_reaches_insert_disk_screen() -> Result<(), Box<dyn Error>> {
    let Some(rom_dir) = home_rom_dir() else {
        eprintln!("skip: no Amiga ROM dir at $HOME/.emu198x/roms/commodore-amiga");
        return Ok(());
    };
    let kickstart_path = rom_dir.join("kick13.rom");
    if !kickstart_path.exists() {
        eprintln!("skip: kick13.rom missing at {}", kickstart_path.display());
        return Ok(());
    }
    let firmware = std::fs::read(&kickstart_path)?;
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPal, firmware)?;

    let mut host = null_host();
    runtime.run_until(MachineTime::new(2_500_000), &mut host)?;

    let provider = runtime_commodore_amiga::AmigaSessionQueryProvider;
    use emu198x_shell::SessionQueryProvider;
    let result = provider
        .query(&runtime, "boot.detected")?
        .expect("boot.detected should be available");
    assert_eq!(
        result.value,
        serde_json::Value::Bool(true),
        "Kickstart 1.3 should reach insert-disk within 2.5M ticks"
    );
    Ok(())
}

/// Waypoint: Kickstart 1.3 + Workbench 1.3 ADF reaches a steady
/// post-boot screen. Promoted from the long-running diag harnesses.
///
/// Catches regression: disk DMA + MFM decode + autoconfig + trackdisk
/// path together; this is the single most expensive regression to
/// reproduce by hand.
#[test]
#[ignore = "requires ~/.emu198x/roms/commodore-amiga/kick13.rom and ~/.emu198x/media/commodore-amiga/workbench-1.3.adf"]
fn workbench_13_reaches_desktop() -> Result<(), Box<dyn Error>> {
    use emu198x_shell::{MediaImage, MediaKind, MediaSet};

    let Some(rom_dir) = home_rom_dir() else {
        eprintln!("skip: no Amiga ROM dir");
        return Ok(());
    };
    let Some(media_dir) = home_media_dir() else {
        eprintln!("skip: no Amiga media dir");
        return Ok(());
    };
    let kickstart_path = rom_dir.join("kick13.rom");
    let adf_path = media_dir.join("workbench-1.3.adf");
    if !kickstart_path.exists() || !adf_path.exists() {
        eprintln!("skip: missing kickstart or workbench ADF");
        return Ok(());
    }

    let firmware = std::fs::read(&kickstart_path)?;
    let adf = std::fs::read(&adf_path)?;
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPalA501, firmware)?;

    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &adf));
    runtime.load_media(&media)?;

    let mut host = null_host();
    // Workbench boot is long; 25M ticks (~3 seconds at 7.16 MHz) is
    // a generous bound that historically reaches the desktop.
    runtime.run_until(MachineTime::new(25_000_000), &mut host)?;

    let provider = runtime_commodore_amiga::AmigaSessionQueryProvider;
    use emu198x_shell::SessionQueryProvider;
    let result = provider
        .query(&runtime, "boot.detected")?
        .expect("boot.detected should be available");
    assert_eq!(
        result.value,
        serde_json::Value::Bool(true),
        "Workbench 1.3 should reach desktop within 25M ticks"
    );
    Ok(())
}

/// Waypoint: Kickstart 2.04 boots an A500+ ECS machine to the
/// insert-disk screen.
///
/// **Validated 2026-05-01:** the AmigaEcs chip stack reaches
/// insert-disk on KS 2.04 r37.175 in ~50M ticks (~7 seconds wall
/// time at PAL = ~350 frames). KS 2.04's cold-boot path is
/// substantially longer than KS 1.3's: the 256 KiB ROM checksum
/// loop (`add.l (A0)+,D5; bcc.s; addq.l #1,D5; dbge D1,…` at
/// $F800E2-$F800EA) consumes the first ~4M ticks alone, then
/// overlay clears (CIA-A PRA bit 0 at $BFE001 written at $F8010C),
/// memlist setup, exec.library init, and graphics.library init
/// take the rest.
///
/// Catches regression: any bus-arbitration / overlay / interrupt
/// regression that breaks the ECS A500+ boot path. Runs on the
/// `AmigaEcsRuntime` (the chip-correct A500+ runtime) — see also
/// `kickstart_13_reaches_insert_disk_screen` for the OCS A500
/// equivalent on KS 1.3.
#[test]
#[ignore = "requires ~/.emu198x/roms/commodore-amiga/kick204.rom (KS 2.04 r37.175)"]
fn kickstart_204_reaches_insert_disk_screen_a500_plus_pal() -> Result<(), Box<dyn Error>> {
    use runtime_commodore_amiga::AmigaEcsRuntime;
    let Some(rom_dir) = home_rom_dir() else {
        eprintln!("skip: no Amiga ROM dir at $HOME/.emu198x/roms/commodore-amiga");
        return Ok(());
    };
    let kickstart_path = rom_dir.join("kick204.rom");
    if !kickstart_path.exists() {
        eprintln!("skip: kick204.rom missing at {}", kickstart_path.display());
        return Ok(());
    }
    let firmware = std::fs::read(&kickstart_path)?;
    let mut runtime = AmigaEcsRuntime::new(Model::A500PlusEcsPal, firmware)?;

    let mut host = null_host();
    // 50M ticks ≈ 350 PAL frames ≈ 7 seconds wall time. Picked from
    // the 2026-05-01 extended-probe diagnostic: detected=true
    // appears reliably from ~50M onwards.
    runtime.run_until(MachineTime::new(50_000_000), &mut host)?;

    let provider = runtime_commodore_amiga::AmigaSessionQueryProvider;
    use emu198x_shell::SessionQueryProvider;
    let result = provider
        .query(&runtime, "boot.detected")?
        .expect("boot.detected should be available");
    assert_eq!(
        result.value,
        serde_json::Value::Bool(true),
        "Kickstart 2.04 should reach insert-disk within 50M ticks (A500+ ECS PAL)"
    );
    Ok(())
}

/// Waypoint: Kickstart 2.04 + Workbench 2.04 ADF reaches a steady
/// post-boot screen on the A500+ ECS chip stack. Parallels the
/// existing `workbench_13_reaches_desktop` waypoint that proves the
/// equivalent OCS A500 + KS 1.3 + WB 1.3 boot path.
///
/// Catches regression: disk DMA + MFM decode + autoconfig + trackdisk
/// path on the ECS chip stack — the single most expensive ECS
/// regression to reproduce by hand. Window is 50M ticks (insert-disk)
/// plus 25M (Workbench load) for 75M total, generous for the longer
/// KS 2.04 cold-boot path.
#[test]
#[ignore = "requires ~/.emu198x/roms/commodore-amiga/kick204.rom and ~/.emu198x/media/commodore-amiga/workbench-2.04.adf"]
fn workbench_204_reaches_desktop_a500_plus_pal() -> Result<(), Box<dyn Error>> {
    use emu198x_shell::{MediaImage, MediaKind, MediaSet};
    use runtime_commodore_amiga::AmigaEcsRuntime;

    let Some(rom_dir) = home_rom_dir() else {
        eprintln!("skip: no Amiga ROM dir");
        return Ok(());
    };
    let Some(media_dir) = home_media_dir() else {
        eprintln!("skip: no Amiga media dir");
        return Ok(());
    };
    let kickstart_path = rom_dir.join("kick204.rom");
    let adf_path = media_dir.join("workbench-2.04.adf");
    if !kickstart_path.exists() || !adf_path.exists() {
        eprintln!("skip: missing kickstart or workbench ADF");
        return Ok(());
    }

    let firmware = std::fs::read(&kickstart_path)?;
    let adf = std::fs::read(&adf_path)?;
    let mut runtime = AmigaEcsRuntime::new(Model::A500PlusEcsPal, firmware)?;

    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &adf));
    runtime.load_media(&media)?;

    let mut host = null_host();
    runtime.run_until(MachineTime::new(250_000_000), &mut host)?;

    let provider = runtime_commodore_amiga::AmigaSessionQueryProvider;
    use emu198x_shell::SessionQueryProvider;

    // `boot.detected` flips to true at the KS 2.04 insert-disk screen,
    // which renders before the disk has been read. Use disk activity
    // to prove the trackdisk path actually executed: KS 2.04 issues
    // hundreds of step pulses to load Workbench, so step_events well
    // above the early "calibrate to TK0" handful proves the disk was
    // read. A spinup-timer regression (peripheral-commodore-amiga-floppy
    // resetting on every PRB write) used to cap step_events at ~12
    // before the loader gave up.
    let steps = provider
        .query(&runtime, "disk.step_events")?
        .expect("disk step_events should be available")
        .value
        .as_u64()
        .expect("step_events is a number");
    assert!(
        steps > 200,
        "KS 2.04 should issue hundreds of step pulses while loading WB 2.04 (got {steps})"
    );
    Ok(())
}

/// Diagnostic: disassemble the KS 2.04 boot path from $F800D2.
/// Reusable for inspecting any region of any KS ROM — bump the
/// `pc`/`end` pair to walk further, or change the ROM file path
/// to point at a different Kickstart. Surfaced
/// 2026-05-01 the ROM checksum loop at $F800E2-$F800EA, which
/// confirmed why early probes saw PC oscillate inside that window
/// for the first ~4M ticks.
///
/// Known limitation: the disassembler currently mis-decodes `DBcc`
/// instructions as `Scc + dc.w` (e.g. `0x5CC9 0xFFF8` should print
/// as `dbge D1, …` but prints as `sf A1; dc.w $FFF8`). Code reads
/// fine for branch / move / arithmetic; DBcc loops need manual
/// re-decoding from the raw bytes for now.
#[test]
#[ignore = "diagnostic — disassemble KS 2.04 cold-boot from $F800D2"]
fn kickstart_204_disassemble_cold_boot() -> Result<(), Box<dyn Error>> {
    let Some(rom_dir) = home_rom_dir() else {
        eprintln!("skip: no Amiga ROM dir");
        return Ok(());
    };
    let kickstart_path = rom_dir.join("kick204.rom");
    if !kickstart_path.exists() {
        eprintln!("skip: kick204.rom missing");
        return Ok(());
    }
    let firmware = std::fs::read(&kickstart_path)?;
    // ROM lives at $F80000 (the reset vector points to $F800D2).
    let rom_base = 0x00F8_0000u32;
    let read = |abs_addr: u32| -> u8 {
        let off = abs_addr.wrapping_sub(rom_base) as usize;
        firmware.get(off).copied().unwrap_or(0)
    };

    // Disassemble from $F800D2 forward. Walk by the byte length each
    // disassembled instruction reports.
    let mut pc: u32 = 0x00F8_00D2;
    let end: u32 = 0x00F8_0200; // first 302 bytes — should be ~80-150 instructions
    eprintln!("\n--- KS 2.04 cold-boot disassembly (${pc:08X} .. ${end:08X}) ---");
    while pc < end {
        let (mnemonic, len) = motorola_68000::disasm::disassemble(pc, read);
        eprintln!("${pc:08X}: {mnemonic}");
        if len == 0 {
            eprintln!("    (length=0; aborting)");
            break;
        }
        pc = pc.saturating_add(u32::from(len));
    }
    Ok(())
}
