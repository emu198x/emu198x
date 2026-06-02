//! Frame-loop throughput bench for the Spectrum runtime.
//!
//! Measures wall-clock time to call `run_frame()` once on each of the
//! two SOLID workhorse variants (48K + 128K) after a fixed boot
//! warm-up. Divides cleanly into the real-frame wall-clock
//! (~19.97 ms for both PAL variants) to give the "× realtime"
//! multiplier the docs quote in `knowledge/tests/spectrum.md`
//! § Performance.
//!
//! Real ROMs are resolved from `$EMU198X_SPECTRUM_48K_ROM` /
//! `$EMU198X_SPECTRUM_128K_ROM_DIR` (defaulting to
//! `~/.emu198x/roms/sinclair-zx-spectrum-{48k,128k}/...`). When the
//! files are missing the bench falls back to a zeroed 16/32 KiB ROM
//! image so the bench still produces a number (the CPU sits in the
//! reset-vector NOP loop). The fallback workload understates the
//! release runtime by skipping the ULA / contention pressure of the
//! real boot loop — labels reflect the source so a measurement
//! taken on a cold machine isn't mistaken for a real-ROM number.
//!
//! Run with:
//!
//! ```sh
//! cargo bench -p runtime-sinclair-zx-spectrum --bench frame_throughput
//! ```

use std::path::PathBuf;
use std::time::Duration;

use common_sinclair_zx_spectrum::timing::{
    CPU_HZ_48K, CPU_HZ_128K, TSTATES_PER_FRAME_48K, TSTATES_PER_FRAME_128K,
};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;

/// Frames to run before measurement so the machine is past its boot
/// settle and into the BASIC editor's quiescent key-poll loop.
const WARMUP_FRAMES: usize = 200;

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn rom_48k_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EMU198X_SPECTRUM_48K_ROM") {
        return Some(PathBuf::from(path));
    }
    Some(home()?.join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom"))
}

fn rom_128k_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EMU198X_SPECTRUM_128K_ROM_DIR") {
        return Some(PathBuf::from(path));
    }
    Some(home()?.join(".emu198x/roms/sinclair-zx-spectrum-128k"))
}

enum RomSource {
    RealRom,
    Synthetic,
}

fn rom_label(source: &RomSource) -> &'static str {
    match source {
        RomSource::RealRom => "real-rom",
        RomSource::Synthetic => "synthetic-zero-rom",
    }
}

fn warm_spectrum48k() -> (Spectrum48k, RomSource) {
    let mut machine = Spectrum48k::new();
    let source = match rom_48k_path().and_then(|p| std::fs::read(&p).ok()) {
        Some(rom) => {
            machine
                .load_rom_bytes(&rom)
                .expect("48K ROM should fit 16 KiB");
            RomSource::RealRom
        }
        None => RomSource::Synthetic,
    };
    machine.reset();
    for _ in 0..WARMUP_FRAMES {
        machine.run_frame();
    }
    (machine, source)
}

fn warm_spectrum128k() -> (Spectrum128K, RomSource) {
    let mut machine = Spectrum128K::new();
    let source = match rom_128k_dir() {
        Some(dir) => {
            let rom0 = std::fs::read(dir.join("128-0.rom"));
            let rom1 = std::fs::read(dir.join("128-1.rom"));
            if let (Ok(rom0), Ok(rom1)) = (rom0, rom1) {
                machine.memory.load_roms(&rom0, &rom1);
                RomSource::RealRom
            } else {
                RomSource::Synthetic
            }
        }
        None => RomSource::Synthetic,
    };
    for _ in 0..WARMUP_FRAMES {
        machine.run_frame();
    }
    (machine, source)
}

fn realtime_frame_ns_48k() -> u64 {
    // T-states-per-frame × 1e9 / CPU Hz → ns of wall-clock per frame.
    (u64::from(TSTATES_PER_FRAME_48K) * 1_000_000_000) / CPU_HZ_48K
}

fn realtime_frame_ns_128k() -> u64 {
    (u64::from(TSTATES_PER_FRAME_128K) * 1_000_000_000) / CPU_HZ_128K
}

fn bench_48k(c: &mut Criterion) {
    let (mut machine, source) = warm_spectrum48k();
    let realtime_ns = realtime_frame_ns_48k();
    let label = format!("spectrum_48k/run_frame/{}", rom_label(&source));
    let mut group = c.benchmark_group(label);
    // 1 frame ≈ 19.97 ms of emulated time per iteration.
    group.throughput(Throughput::Elements(1));
    group.bench_function("frame", |b| {
        b.iter(|| {
            machine.run_frame();
        });
    });
    group.finish();
    eprintln!(
        "spectrum_48k realtime-frame budget: {:.3} ms ({} ns)",
        realtime_ns as f64 / 1_000_000.0,
        realtime_ns
    );
}

fn bench_128k(c: &mut Criterion) {
    let (mut machine, source) = warm_spectrum128k();
    let realtime_ns = realtime_frame_ns_128k();
    let label = format!("spectrum_128k/run_frame/{}", rom_label(&source));
    let mut group = c.benchmark_group(label);
    group.throughput(Throughput::Elements(1));
    group.bench_function("frame", |b| {
        b.iter(|| {
            machine.run_frame();
        });
    });
    group.finish();
    eprintln!(
        "spectrum_128k realtime-frame budget: {:.3} ms ({} ns)",
        realtime_ns as f64 / 1_000_000.0,
        realtime_ns
    );
}

criterion_group! {
    name = frame_throughput;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));
    targets = bench_48k, bench_128k
}
criterion_main!(frame_throughput);
