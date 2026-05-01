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
    // Same chip-RAM size for the PAL/NTSC pair of every variant —
    // only Agnus differs between regions.
    match model {
        Model::A500OcsPal | Model::A500OcsPalA501 | Model::A500OcsNtsc | Model::A500OcsNtscA501 => {
            512 * 1024
        }
        Model::A500PlusEcsPal
        | Model::A500OcsPalMaxed
        | Model::A500PlusEcsNtsc
        | Model::A500OcsNtscMaxed => 1024 * 1024,
        Model::A1000OcsPal | Model::A1000OcsNtsc => 256 * 1024,
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
/// **Structural check, not a passing baseline.** This session ships
/// the AmigaEcs machine wrapper + AmigaEcsRuntime + A500+ Model
/// reclassification, but doesn't validate that Kickstart 2.04
/// actually reaches insert-disk yet. KS 2.04 may exercise BEAMCON0
/// or BPLCON3 paths that the current ECS chip wrappers stub out.
/// The first real run of this test from a freshly-extracted KS 2.04
/// ROM is expected to surface those gaps; treat regressions found
/// here as the input to the next ECS session.
#[test]
#[ignore = "requires ~/.emu198x/roms/commodore-amiga/kick204.rom (KS 2.04 r37.175)"]
fn kickstart_204_reaches_insert_disk_screen_a500_plus_pal()
-> Result<(), Box<dyn Error>> {
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

    let provider = runtime_commodore_amiga::AmigaSessionQueryProvider;
    use emu198x_shell::SessionQueryProvider;

    // Tick in 100k-tick increments and snapshot diagnostic state at
    // each window so we can see exactly where the boot path stalls.
    // Each window is roughly 35 KS frames at PAL.
    let probes = [100_000u64, 250_000, 500_000, 1_000_000, 2_500_000];
    let mut prior = 0u64;
    for &target in &probes {
        let delta = target - prior;
        let mut host = null_host();
        runtime.run_until(MachineTime::new(target), &mut host)?;
        prior = target;

        let pc = provider
            .query(&runtime, "amiga.cpu.pc")?
            .expect("amiga.cpu.pc")
            .value;
        let ipl = provider
            .query(&runtime, "amiga.cpu.ipl")?
            .expect("amiga.cpu.ipl")
            .value;
        let vpos = provider
            .query(&runtime, "amiga.agnus.vpos")?
            .expect("amiga.agnus.vpos")
            .value;
        let dmacon = provider
            .query(&runtime, "amiga.agnus.dmacon")?
            .expect("amiga.agnus.dmacon")
            .value;
        let intena = provider
            .query(&runtime, "amiga.paula.intena")?
            .expect("amiga.paula.intena")
            .value;
        let intreq = provider
            .query(&runtime, "amiga.paula.intreq")?
            .expect("amiga.paula.intreq")
            .value;
        let bplcon0 = provider
            .query(&runtime, "amiga.agnus.bplcon0")?
            .expect("amiga.agnus.bplcon0")
            .value;
        let overlay = provider
            .query(&runtime, "amiga.memory.overlay")?
            .expect("amiga.memory.overlay")
            .value;
        let detected = provider
            .query(&runtime, "boot.detected")?
            .expect("boot.detected")
            .value;
        let reason = provider
            .query(&runtime, "boot.reason")?
            .expect("boot.reason")
            .value;
        eprintln!(
            "[{:>10} ticks (+{:>8})] PC={pc} IPL={ipl} vpos={vpos} \
             dmacon={dmacon} intena={intena} intreq={intreq} \
             bplcon0={bplcon0} overlay={overlay} \
             boot.detected={detected} boot.reason={reason}",
            target, delta
        );
    }
    Ok(())
}

/// Diagnostic: same KS 2.04 ROM, but constructed against
/// AmigaOcsRuntime instead of AmigaEcsRuntime. Lets us tell whether
/// the early stall is ECS-specific or KS-2.04-specific.
#[test]
#[ignore = "diagnostic — KS 2.04 against OCS chip stack"]
fn kickstart_204_diagnostic_on_ocs_runtime() -> Result<(), Box<dyn Error>> {
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
    // Construct against OCS for diagnostic purposes — A500+ is now
    // ECS in profile metadata, but AmigaOcsRuntime accepts the model
    // and routes to OCS chips for this sort of comparison.
    let mut runtime = AmigaOcsRuntime::new(Model::A500PlusEcsPal, firmware)?;

    let provider = runtime_commodore_amiga::AmigaSessionQueryProvider;
    use emu198x_shell::SessionQueryProvider;

    let probes = [100_000u64, 250_000, 500_000, 1_000_000, 2_500_000];
    for &target in &probes {
        let mut host = null_host();
        runtime.run_until(MachineTime::new(target), &mut host)?;
        let pc = provider.query(&runtime, "amiga.cpu.pc")?.expect("pc").value;
        let dmacon = provider.query(&runtime, "amiga.agnus.dmacon")?.expect("dmacon").value;
        let detected = provider.query(&runtime, "boot.detected")?.expect("detected").value;
        eprintln!(
            "[OCS @ {:>10}] PC={pc} dmacon={dmacon} boot.detected={detected}",
            target
        );
    }
    Ok(())
}
