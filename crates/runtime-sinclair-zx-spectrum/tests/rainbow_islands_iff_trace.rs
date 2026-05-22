//! Diagnostic: load SkoolKit's known-good Rainbow Islands `.z80`
//! snapshot, then trace `Z80::regs.iff1` and the AY register-7 mixer
//! state frame-by-frame. Discriminates between the two open
//! hypotheses for the silent-music root cause (see
//! `knowledge/decisions/speedlock-silent-music.md` log entry 2026-05-18):
//!
//!   1. **Early-exit conditional in the music driver** — IRQs fire
//!      (IFF1 flips to 1 within a few frames) but the driver checks
//!      some flag byte and skips its update writes every iteration.
//!      Expected trace: IFF1 transitions from 0 to 1 within ~50
//!      frames and stays at 1.
//!
//!   2. **IRQs never fire** — IFF1 stays 0 throughout, so the music
//!      driver hooked into the IRQ chain never runs. Expected trace:
//!      IFF1 = 0 for every sampled frame.
//!
//! Either way the AY's r7 mixer value at the end of the run is
//! diagnostic: $FF means "all channels disabled" (which is what the
//! initial sweep left it at), and any other value means the driver
//! got past its init pass.
//!
//! Requires `~/Projects/Emu198x/[/tmp/]rainbow-sk-128.z80` (the
//! SkoolKit-generated snapshot) — the test prints a skip notice if
//! the file isn't present, so it's safe to run in environments that
//! don't have the snapshot pre-generated.
//!
//! Run with:
//!
//!     cargo test -p runtime-sinclair-zx-spectrum \
//!         --test rainbow_islands_iff_trace -- --ignored --nocapture

use std::env;
use std::path::PathBuf;

use common_sinclair_zx_spectrum::timing::TIMING_128K;
use emu198x_shell::{FirmwareImage, FirmwareSet, HeadlessSession, read_firmware_asset};
use runtime_sinclair_zx_spectrum::{
    Spectrum128kRuntime, SpectrumMachine, SpectrumSessionQueryProvider,
};

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[test]
#[ignore = "diagnostic — trace IFF1 over time from a SkoolKit Rainbow Islands snapshot"]
fn trace_iff_and_mixer_from_skoolkit_snapshot() {
    let snapshot_path = PathBuf::from("/tmp/rainbow-sk-128.z80");
    if !snapshot_path.exists() {
        eprintln!("skipped: {} not found", snapshot_path.display());
        return;
    }
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-128k");
    if !firmware_root.exists() {
        eprintln!("skipped: 128K ROMs not installed");
        return;
    }

    let rom0 = read_firmware_asset(&firmware_root.join("128-0.rom")).expect("128 rom 0");
    let rom1 = read_firmware_asset(&firmware_root.join("128-1.rom")).expect("128 rom 1");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-128k-rom-0".to_owned(),
        &rom0.bytes,
    ));
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-128k-rom-1".to_owned(),
        &rom1.bytes,
    ));
    let runtime = Spectrum128kRuntime::from_firmware(&firmware).expect("128K runtime");

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_128K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    // Parse and apply the SkoolKit snapshot.
    let snap_bytes = std::fs::read(&snapshot_path).expect("read snapshot");
    let snapshot = format_sinclair_zx_spectrum_z80::parse_z80(&snap_bytes).expect("parse snapshot");
    SpectrumMachine::apply_snapshot(session.machine_mut().machine_mut(), &snapshot);

    // Initial state at frame 0.
    let m = session.machine().machine();
    let init_iff1 = m.z80.regs.iff1;
    let init_pc = m.z80.regs.pc;
    let init_mixer = m.ay.registers()[7];
    eprintln!(
        "frame 0 (post-apply): PC={:04X} IFF1={} mixer=r7={:02X}",
        init_pc, init_iff1 as u8, init_mixer,
    );

    // Run forward in 50-frame chunks, logging IFF1 and mixer state
    // at each boundary plus any transitions.
    let mut prev_iff1 = init_iff1;
    let mut prev_mixer = init_mixer;
    let mut iff1_transitions = 0u32;
    let mut iff1_one_frames = 0u32;
    let mut iff1_zero_frames = 0u32;

    let frames_per_chunk = 50u32;
    let total_chunks = 200u32; // 10000 frames = ~200 seconds
    for chunk in 1..=total_chunks {
        session.run_frames(frames_per_chunk).expect("run_frames");
        let m = session.machine().machine();
        let iff1 = m.z80.regs.iff1;
        let pc = m.z80.regs.pc;
        let mixer = m.ay.registers()[7];

        if iff1 {
            iff1_one_frames += 1;
        } else {
            iff1_zero_frames += 1;
        }

        let changed = iff1 != prev_iff1 || mixer != prev_mixer;
        if changed || chunk <= 5 || chunk % 20 == 0 {
            eprintln!(
                "frame {:5}: PC={:04X} IFF1={} mixer=r7={:02X}{}",
                chunk * frames_per_chunk,
                pc,
                iff1 as u8,
                mixer,
                if iff1 != prev_iff1 {
                    " (IFF1 transition!)"
                } else {
                    ""
                },
            );
        }
        if iff1 != prev_iff1 {
            iff1_transitions += 1;
            prev_iff1 = iff1;
        }
        prev_mixer = mixer;
    }

    eprintln!(
        "\n=== Summary across {} chunks of {} frames ({} total frames, ~{:.1}s) ===",
        total_chunks,
        frames_per_chunk,
        total_chunks * frames_per_chunk,
        (total_chunks * frames_per_chunk) as f64 / 50.0,
    );
    eprintln!("IFF1=1 chunks: {iff1_one_frames}");
    eprintln!("IFF1=0 chunks: {iff1_zero_frames}");
    eprintln!("IFF1 transitions: {iff1_transitions}");
    eprintln!("Final mixer (r7): {:02X}", prev_mixer);
}
