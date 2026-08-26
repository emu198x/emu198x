//! Diagnostic: watch the $74A4-$74A5 word in Rainbow Islands' Speedlock 7
//! loader. The SkoolKit byte-diff documented in
//! `knowledge/decisions/speedlock-silent-music.md` (log entry 2026-05-17)
//! showed that across 49,152 bytes of post-load RAM, exactly two bytes
//! differ between our emulator and SkoolKit's known-good simulator —
//! the 16-bit word at $74A4 (ours $1EE7, SkoolKit $1F97), which sits
//! inside a CRC-style accumulator's output region. A single R-register-
//! timing divergence anywhere in the loader's decryption pass would
//! integrate into exactly this signature (per Muckypaws' Speedlock '87
//! analysis: "no feedback to the decoding routine" means one bad seed
//! compounds into one wrong accumulated word).
//!
//! This test identifies the instruction that writes the diverging bytes
//! and the PC tail leading up to it, so we can step back through the
//! calculation chain to find the upstream Z80 instruction whose
//! side-effects differ from real hardware.
//!
//! Two-phase shape mirrors `speedlock7_tape_ram_dump::find_feb3_write_in_green_beret`:
//!
//!   1. Coarse scan in 100-frame chunks to locate the frame range
//!      where the watched word first becomes non-zero (and the final
//!      stable value settles).
//!   2. Reset, fast-forward to the start of the narrow window, then
//!      single-T-state step. On every change to the watched bytes,
//!      log the PC at the moment of the change plus the last 64
//!      distinct PCs leading up to it.
//!
//! Skipped if the TZX file or ROMs aren't installed.
//!
//! Run with:
//!
//!     cargo test -p runtime-sinclair-zx-spectrum \
//!         --test rainbow_islands_speedlock_watchpoint \
//!         -- --ignored --nocapture

use std::collections::VecDeque;
use std::env;
use std::path::PathBuf;

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::TIMING_128K;
use emu198x_shell::{
    ControlCommand, FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MediaImage, MediaKind,
    MediaSet, MediaTransportAction, MediaTransportCommand, read_firmware_asset, read_media_asset,
};
use runtime_sinclair_zx_spectrum::{Spectrum128kRuntime, SpectrumSessionQueryProvider};

const WATCH_LO: u16 = 0x74A4;
const WATCH_HI: u16 = 0x74A5;

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn build_session() -> Option<(
    HeadlessSession<Spectrum128kRuntime, SpectrumSessionQueryProvider>,
    Vec<u8>,
)> {
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-128k");
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join("Rainbow Islands - The Story of Bubble Bobble 2 (1990)(Ocean)(48K-128K).zip");
    if !firmware_root.exists() || !tzx_path.exists() {
        return None;
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

    let tape = read_media_asset(&tzx_path, MediaKind::Tape).expect("tzx");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "tape-1".to_owned(),
        MediaKind::Tape,
        &tape.bytes,
    ));
    session.prepare(&media, &[]).expect("prepare");

    // 128K boot menu navigation: wait for boot, press ENTER for Tape
    // Loader, settle, start tape. Mirrors
    // `emu198x_catalogue::lib::autoload_128k_tape_loader`.
    session.wait_for_boot(400).expect("boot wait");
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: true,
    });
    session.run_frames(2).expect("enter press");
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: false,
    });
    session.run_frames(10).expect("enter settle");
    session
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1".to_owned(),
            MediaTransportAction::Start,
        )))
        .expect("tape start");

    Some((session, tape.bytes.to_vec()))
}

fn read_pair(
    session: &HeadlessSession<Spectrum128kRuntime, SpectrumSessionQueryProvider>,
) -> (u8, u8) {
    let m = session.machine().machine();
    (m.memory.read(WATCH_LO), m.memory.read(WATCH_HI))
}

#[test]
#[ignore = "DIAGNOSTIC: diagnostic — pin the instruction that writes the diverging $74A4-$74A5 word in Rainbow Islands"]
fn find_74a4_writes_in_rainbow_islands() {
    let Some((mut session, tape_bytes)) = build_session() else {
        emu198x_test_skip::skip!("skipped: 128K ROMs or Rainbow Islands TZX not installed");
    };

    // Phase 1 — coarse scan in 100-frame chunks, continuing until
    // the tape transport reports stopped (with a safety cap). Log
    // every change; track the most-recent change window so phase 2
    // can zoom in on the FINAL write that locks in the
    // SkoolKit-divergent value.
    let mut prev = read_pair(&session);
    eprintln!("coarse scan ({:02X}{:02X} = initial):", prev.0, prev.1);
    let mut last_change_window: Option<u32> = None;
    let mut current_frame = 0u32;
    let mut tape_stopped_at: Option<u32> = None;
    let max_frame_cap = 25000u32;
    for target_frame in (100u32..max_frame_cap).step_by(100) {
        session
            .run_frames(target_frame - current_frame)
            .expect("run_frames");
        current_frame = target_frame;
        let now = read_pair(&session);
        if now != prev {
            eprintln!(
                "  frame {target_frame}: $74A4={:02X}{:02X} -> {:02X}{:02X}",
                prev.0, prev.1, now.0, now.1,
            );
            last_change_window = Some(target_frame.saturating_sub(100));
            prev = now;
        }
        let playing = session
            .query("tape.playing")
            .ok()
            .and_then(|v| v.value.as_bool())
            .unwrap_or(true);
        if !playing && tape_stopped_at.is_none() {
            tape_stopped_at = Some(target_frame);
            eprintln!("  frame {target_frame}: tape transport stopped");
            // Run another 1000 frames past tape-stop to catch any
            // post-stop writes (the SkoolKit divergence was observed
            // at the moment our wait_for_tape_stop returned, but the
            // game's loader may still be running code that writes the
            // watched word).
        }
        if let Some(stop) = tape_stopped_at
            && target_frame > stop + 1000
        {
            eprintln!("  stopping coarse scan 1000 frames after tape-stop");
            break;
        }
    }
    let final_value = read_pair(&session);
    eprintln!(
        "coarse scan complete at frame {current_frame}; final $74A4={:02X}{:02X}",
        final_value.0, final_value.1,
    );

    let Some(window_start) = last_change_window else {
        eprintln!("watched word never changed in coarse scan; aborting");
        return;
    };

    // Phase 2 — re-prepare from scratch, run fast to the start of the
    // narrow window, then single-T-state step recording every change
    // to the watched word along with the last 64 distinct PCs.
    eprintln!("\nnarrow window starts at frame {window_start}; restarting to single-T-state step");
    let Some((mut session, _)) = build_session() else {
        unreachable!("first build_session() succeeded");
    };
    let _ = tape_bytes; // capture path used the bytes once; re-prepare did its own read

    session
        .run_frames(window_start)
        .expect("fast-forward to narrow window");

    let mut prev = read_pair(&session);
    eprintln!(
        "re-prepared at frame {window_start}, $74A4={:02X}{:02X}",
        prev.0, prev.1,
    );

    let mut pc_tail: VecDeque<u16> = VecDeque::with_capacity(80);
    let mut prev_pc: u16 = u16::MAX;
    let max_tstates = 1000u32 * TIMING_128K.tstates_per_frame;
    let mut change_count = 0u32;
    for t in 0..max_tstates {
        session.machine_mut().machine_mut().advance_tstates(1);
        let machine = session.machine().machine();
        let pc = machine.z80.regs.pc;
        if pc != prev_pc {
            if pc_tail.len() == 64 {
                pc_tail.pop_front();
            }
            pc_tail.push_back(pc);
            prev_pc = pc;
        }
        let now = (machine.memory.read(WATCH_LO), machine.memory.read(WATCH_HI));
        if now != prev {
            change_count += 1;
            eprintln!(
                "\n$74A4 change #{change_count} at +{t}T (frame ~{}): {:02X}{:02X} -> {:02X}{:02X} | PC=${pc:04X}",
                t / TIMING_128K.tstates_per_frame,
                prev.0,
                prev.1,
                now.0,
                now.1,
            );
            let tail: Vec<String> = pc_tail.iter().map(|p| format!("${p:04X}")).collect();
            eprintln!("  last 64 PCs: {}", tail.join(" "));
            prev = now;
        }
    }

    let final_val = read_pair(&session);
    eprintln!(
        "\nfinal $74A4={:02X}{:02X} after {change_count} changes",
        final_val.0, final_val.1,
    );
}
