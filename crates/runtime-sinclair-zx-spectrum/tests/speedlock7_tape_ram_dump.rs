//! Diagnostic: load Op Wolf Speedlock 7 on the 48K, run the BASIC
//! stub + bootstrap (DI / fill attrs / OUT $FE,0 / LDIR 2650 bytes to
//! $F48E / RET to $F48E), then sample RAM at $F48E across multiple
//! frame budgets so the Speedlock loader's self-decryption progress
//! becomes visible.
//!
//! See `wiki/decisions/speedlock-tape-incomplete.md` for the static
//! analysis that established why $F48E is the load target.
//!
//! Run with:
//!
//!     cargo test -p runtime-sinclair-zx-spectrum \
//!         --test speedlock7_tape_ram_dump -- --ignored --nocapture

use std::env;
use std::path::PathBuf;

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, MediaImage, MediaKind, MediaSet,
    read_firmware_asset, read_media_asset,
};

use common_sinclair_zx_spectrum::tape::TapeSpan;
use common_sinclair_zx_spectrum::timing::TIMING_48K;
use format_sinclair_zx_spectrum_tzx::tzx_to_stream;
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Spectrum48kRuntime, SpectrumMachine,
    SpectrumSessionQueryProvider, autoload_basic_tape,
};

const LOADER_BASE: u16 = 0xF48E;
const LOADER_LEN: usize = 0x0A5A;

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn dump_window(session: &HeadlessSession<Spectrum48kRuntime, SpectrumSessionQueryProvider>, label: &str) {
    eprintln!("\n=== RAM @ ${LOADER_BASE:04x}..+0x80 after {label} ===");
    for row in 0..8 {
        let addr = LOADER_BASE.wrapping_add(row * 16);
        let bytes: Vec<u8> = (0..16)
            .map(|i| session.machine().machine().read_byte(addr.wrapping_add(i as u16)))
            .collect();
        let hex = bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii: String = bytes
            .iter()
            .map(|&b| if (32..127).contains(&b) { b as char } else { '.' })
            .collect();
        eprintln!("  ${addr:04x}: {hex}  {ascii}");
    }
}

fn count_in_fe(session: &HeadlessSession<Spectrum48kRuntime, SpectrumSessionQueryProvider>) -> usize {
    let mut count = 0;
    for off in 0..LOADER_LEN - 1 {
        let addr = LOADER_BASE.wrapping_add(off as u16);
        let b0 = session.machine().machine().read_byte(addr);
        let b1 = session.machine().machine().read_byte(addr.wrapping_add(1));
        if b0 == 0xDB && b1 == 0xFE {
            count += 1;
        }
    }
    count
}

#[test]
#[ignore = "diagnostic — needs 48K ROM and Op Wolf SpeedLock 7 TZX"]
fn dump_speedlock7_loader_ram() {
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_file = env::var("SPEEDLOCK7_TZX").unwrap_or_else(|_| {
        "ARCADE COLLECTION 20 - Operation Wolf (1991)(Hit Squad, The)[SpeedLock 7].zip".to_owned()
    });
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(&tzx_file);

    eprintln!("=== Tracing {tzx_file} ===");
    if !firmware_root.exists() {
        eprintln!("[skip] 48K ROM directory missing: {firmware_root:?}");
        return;
    }
    if !tzx_path.exists() {
        eprintln!("[skip] TZX missing: {tzx_path:?}");
        return;
    }

    let rom_path = firmware_root.join("48.rom");
    let rom_bytes = read_firmware_asset(&rom_path).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));

    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime from firmware");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let tape = read_media_asset(&tzx_path, MediaKind::Tape).expect("tzx media");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "tape-1".to_owned(),
        MediaKind::Tape,
        &tape.bytes,
    ));
    session.prepare(&media, &[]).expect("prepare");

    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
        .expect("autoload");

    // Sample RAM at $F48E across a frame ladder. We expect:
    //   - Before bootstrap runs:        all-zero RAM (initial state).
    //   - After BASIC stub auto-runs:   first byte $3E, $47, $ED, $4F (loader-
    //                                   bootstrap signature: LD A,$47 ; LD R,A).
    //   - After self-decryption:        readable byte-decoder code; we look for
    //                                   $DB $FE (`IN A,($FE)`) appearing in the
    //                                   dump as the canonical marker.
    let stages = [
        100u32, 300, 600, 900, 1100, 1200, 1300, 1400, 1500, 1600, 1700, 1800, 1900, 2000,
        2200, 2400, 3000, 4800, 9600,
    ];
    let last_dump: Option<Vec<u8>> = None;
    let mut cumulative = 0u32;
    for &budget in stages.iter() {
        let delta = budget - cumulative;
        session.run_frames(delta).expect("run_frames");
        cumulative = budget;

        let snapshot: Vec<u8> = (0..LOADER_LEN)
            .map(|i| session.machine().machine().read_byte(LOADER_BASE.wrapping_add(i as u16)))
            .collect();
        let in_fe_count = count_in_fe(&session);
        let (changed, first_change_off) = match &last_dump {
            Some(prev) => {
                let diffs: Vec<usize> = snapshot
                    .iter()
                    .zip(prev.iter())
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(i, _)| i)
                    .collect();
                (diffs.len(), diffs.first().copied())
            }
            None => {
                let diffs: Vec<usize> = snapshot
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| **b != 0)
                    .map(|(i, _)| i)
                    .collect();
                (diffs.len(), diffs.first().copied())
            }
        };
        // Read PROG and PC for context.
        let prog_lo = session.machine().machine().read_byte(0x5C53);
        let prog_hi = session.machine().machine().read_byte(0x5C54);
        let prog = u16::from_le_bytes([prog_lo, prog_hi]);
        let tape_playing = session
            .query("spectrum.tape.playing")
            .ok()
            .and_then(|r| r.value.as_bool())
            .unwrap_or(false);
        let z80_pc = session.machine().machine().z80().regs.pc;
        eprintln!(
            "\n>>> Frame {budget:5}: {changed} bytes changed, {in_fe_count} IN($FE) visible, PROG=${prog:04x}, PC=${z80_pc:04x}, tape.playing={tape_playing}",
        );
        if let Some(off) = first_change_off {
            let addr = LOADER_BASE.wrapping_add(off as u16);
            eprintln!("  first change at offset {off:#06x} (${addr:04x})");
            // Dump 16 bytes there + 16 bytes at PC if differ.
            let mem: Vec<u8> = (0..32)
                .map(|i| session.machine().machine().read_byte(addr.wrapping_add(i)))
                .collect();
            let hex = mem.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            eprintln!("    @${addr:04x}: {hex}");
        }
        if !(0x4000..0xFC00).contains(&z80_pc) || z80_pc < LOADER_BASE.wrapping_sub(0x100) {
            let mem: Vec<u8> = (0..32)
                .map(|i| session.machine().machine().read_byte(z80_pc.wrapping_add(i)))
                .collect();
            let hex = mem.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            eprintln!("  PC area @${z80_pc:04x}: {hex}");
        } else {
            let start = z80_pc.wrapping_sub(8);
            let mem: Vec<u8> = (0..32)
                .map(|i| session.machine().machine().read_byte(start.wrapping_add(i)))
                .collect();
            let hex = mem.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            eprintln!("  PC area @${start:04x}: {hex}    (PC=${z80_pc:04x})");
        }
        // Dump 32 bytes at PROG (if PROG is sensible) — shows whether BASIC
        // loaded into the program area at all.
        if prog >= 0x5C00 && prog < 0xFC00 {
            let bytes: Vec<u8> = (0..32)
                .map(|i| session.machine().machine().read_byte(prog.wrapping_add(i)))
                .collect();
            let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            eprintln!("  RAM @ PROG=${prog:04x}: {hex}");
        }
        dump_window(&session, &format!("frame {budget}"));
    }

    // Final dump: save the full $F48E..+0x0A5A region to /tmp for off-line
    // disassembly with a Z80 disassembler.
    let final_snapshot: Vec<u8> = (0..LOADER_LEN)
        .map(|i| session.machine().machine().read_byte(LOADER_BASE.wrapping_add(i as u16)))
        .collect();
    let out_path = PathBuf::from("/tmp/speedlock7-ram-f48e.bin");
    std::fs::write(&out_path, &final_snapshot).expect("write final dump");
    eprintln!(
        "\n=== Wrote final ${LOADER_BASE:04x}..+0x{LOADER_LEN:04x} dump to {} ({} bytes) ===",
        out_path.display(),
        final_snapshot.len(),
    );
}

#[test]
#[ignore = "diagnostic — find what writes $01 to $feb3 in Green Beret"]
fn find_feb3_write_in_green_beret() {
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_file = "ARCADE COLLECTION 02 - Green Beret (1989)(Hit Squad, The)[SpeedLock 7].zip";
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_file);
    if !firmware_root.exists() || !tzx_path.exists() {
        return;
    }
    let rom_bytes = read_firmware_asset(&firmware_root.join("48.rom")).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");

    // First do a coarse scan: run forward in 100-frame chunks and
    // log when $feb3 changes value, to narrow down the write window.
    let mut prev_feb3 = session.machine().machine().read_byte(0xfeb3);
    let mut current_frame: u32 = 0;
    let mut narrow_window: Option<u32> = None;
    eprintln!("coarse scan for $feb3 changes:");
    for frame_step in (100u32..7000).step_by(100) {
        session.run_frames(frame_step - current_frame).expect("run_frames");
        current_frame = frame_step;
        let feb3 = session.machine().machine().read_byte(0xfeb3);
        if feb3 != prev_feb3 {
            eprintln!("  frame {current_frame}: $feb3 ${prev_feb3:02x} -> ${feb3:02x}");
            if narrow_window.is_none() {
                narrow_window = Some(current_frame.saturating_sub(100));
            }
            prev_feb3 = feb3;
        }
    }

    // Restart from scratch to single-step in the narrow window.
    if let Some(window_start) = narrow_window {
        eprintln!("narrow window starts at frame {window_start}; restarting to single-step");
        // Re-prepare to step from scratch.
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom".to_owned(),
            &rom_bytes.bytes,
        ));
        let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
        session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_48K.halfcycles_per_frame),
            SpectrumSessionQueryProvider,
        );
        let mut media = MediaSet::new();
        media.push(MediaImage::new(
            "tape-1".to_owned(),
            MediaKind::Tape,
            &tape.bytes,
        ));
        session.prepare(&media, &[]).expect("prepare");
        autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");
        session.run_frames(window_start).expect("run_frames");
        prev_feb3 = session.machine().machine().read_byte(0xfeb3);
        eprintln!("re-prepared at frame {window_start}, $feb3=${prev_feb3:02x}");
    } else {
        eprintln!("$feb3 never changed in coarse scan; aborting");
        return;
    }

    // Single-T-state step through a wide 1000-frame window so we
    // catch the FINAL settling of $feb3 to $01.
    let max_t = 1000u32 * TIMING_48K.tstates_per_frame;
    let mut pc_tail: std::collections::VecDeque<u16> = std::collections::VecDeque::with_capacity(80);
    let mut prev_pc = u16::MAX;
    for t in 0..max_t {
        session.machine_mut().machine_mut().advance_tstates(1);
        let machine = session.machine().machine();
        let cur_pc = machine.z80().regs.pc;
        if cur_pc != prev_pc {
            if pc_tail.len() == 64 {
                pc_tail.pop_front();
            }
            pc_tail.push_back(cur_pc);
            prev_pc = cur_pc;
        }
        let feb3 = machine.read_byte(0xfeb3);
        if feb3 != prev_feb3 {
            eprintln!(
                "$feb3 changed at +{t}T into single-step (frame ~{}): ${prev_feb3:02x} -> ${feb3:02x}, PC=${cur_pc:04x}",
                t / TIMING_48K.tstates_per_frame,
            );
            let tail: Vec<String> = pc_tail.iter().map(|p| format!("${p:04x}")).collect();
            eprintln!("last 64 PCs: {}", tail.join(" "));
            prev_feb3 = feb3;
            // Keep going to see further changes
        }
    }
    let final_feb3 = session.machine().machine().read_byte(0xfeb3);
    eprintln!("final $feb3: ${final_feb3:02x}");

    // Dump the surrounding code region so we can disassemble the
    // path through $fe92..$feb2 that produced the $01 write.
    for base in [0xfb60u16, 0xfb70, 0xfb80, 0xfe80, 0xfe90, 0xfea0, 0xfeb0] {
        let bytes: Vec<u8> = (0..16)
            .map(|i| session.machine().machine().read_byte(base.wrapping_add(i as u16)))
            .collect();
        let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
        eprintln!("  ${base:04x}: {hex}");
    }
}

#[test]
#[ignore = "diagnostic — what does HL contain at $fe9d for Green Beret vs Op Wolf?"]
fn trace_hl_at_checksum_check() {
    for (label, tzx_relative_path) in [
        (
            "Op Wolf",
            "ARCADE COLLECTION 20 - Operation Wolf (1991)(Hit Squad, The)[SpeedLock 7].zip",
        ),
        (
            "Green Beret",
            "ARCADE COLLECTION 02 - Green Beret (1989)(Hit Squad, The)[SpeedLock 7].zip",
        ),
    ] {
        log_hl_at_fe9d(label, tzx_relative_path);
    }
}

fn log_hl_at_fe9d(label: &str, tzx_relative_path: &str) {
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_relative_path);
    if !firmware_root.exists() || !tzx_path.exists() {
        return;
    }
    let rom_bytes = read_firmware_asset(&firmware_root.join("48.rom")).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");

    // Skip to past the BASIC load (frame ~1500) to make tracing
    // tractable.
    session.run_frames(1500).expect("run_frames");

    // Continuous single-T-state step. ~5000 frames × 70k T = 350M
    // iterations; ~30s in release.
    let max_t = 5000u32 * TIMING_48K.tstates_per_frame;
    let mut prev_pc = u16::MAX;
    let mut fe9d_hits = 0usize;
    let mut feaf_hits = 0usize;
    let mut fec6_hits = 0usize;
    for t in 0..max_t {
        session.machine_mut().machine_mut().advance_tstates(1);
        let z = session.machine().machine().z80();
        let cur_pc = z.regs.pc;
        if cur_pc == prev_pc {
            continue;
        }
        prev_pc = cur_pc;
        match cur_pc {
            0xfe93 => {
                // LD A,(DE) — capture the byte being summed
                let de = z.regs.de;
                let byte = session.machine().machine().read_byte(de);
                eprintln!("[{label}] +{t:9}T  $fe93 LD A,(DE): DE=${de:04x} byte=${byte:02x}");
            }
            0xfe9d => {
                let hl = z.regs.hl;
                let bc = z.regs.bc;
                fe9d_hits += 1;
                eprintln!(
                    "[{label}] +{t:9}T  $fe9d#{fe9d_hits}: HL_sum=${hl:04x} BC=${bc:04x} → {}",
                    if hl < 0x64 { "PASS (HL<$64)" } else { "FAIL (HL≥$64, will write $01)" },
                );
            }
            0xfeaf => {
                feaf_hits += 1;
                eprintln!(
                    "[{label}] +{t:9}T  $feaf#{feaf_hits}: WRITING $01 TO $FEB3 (A=${:02x})",
                    z.regs.a(),
                );
            }
            0xfec6 => {
                let h = z.regs.h();
                fec6_hits += 1;
                eprintln!(
                    "[{label}] +{t:9}T  $fec6#{fec6_hits}: H=${h:02x} → {}",
                    if h >= 1 { "FAIL ($feca CALL $fbcb)" } else { "PASS (RET C)" },
                );
            }
            0xfbd0 => {
                eprintln!("[{label}] +{t:9}T  WIPE SLED ENTERED at $fbd0");
                break;
            }
            _ => {}
        }
    }
    eprintln!(
        "[{label}] totals: $fe9d={fe9d_hits} $feaf={feaf_hits} $fec6={fec6_hits}",
    );
}

#[test]
#[ignore = "diagnostic — log every $fd5f bit-shift hit + wipe trigger for Green Beret"]
fn trace_green_beret_wipe_fire() {
    log_speedlock7_verifier_hits(
        "Green Beret",
        "ARCADE COLLECTION 02 - Green Beret (1989)(Hit Squad, The)[SpeedLock 7].zip",
        20000,
    );
}

#[test]
#[ignore = "diagnostic — log every $fd5f bit-shift hit + wipe trigger for Op Wolf (baseline)"]
fn trace_op_wolf_wipe_fire() {
    log_speedlock7_verifier_hits(
        "Op Wolf",
        "ARCADE COLLECTION 20 - Operation Wolf (1991)(Hit Squad, The)[SpeedLock 7].zip",
        20000,
    );
}

/// Frame-by-frame poll for entries into the bit-shift verifier
/// (PC = $fd5f) and the wipe trigger (PC = $fd6c, $fbd0). Logs L
/// at each hit. Useful for spotting when the loader rejects a
/// verifier output, even though we can't catch every iter at frame
/// resolution.
fn log_speedlock7_verifier_hits(label: &str, tzx_relative_path: &str, max_frames: u32) {
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_relative_path);
    if !firmware_root.exists() || !tzx_path.exists() {
        return;
    }
    let rom_bytes = read_firmware_asset(&firmware_root.join("48.rom")).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");

    // We can't catch PC = $fd5f frame-by-frame because each pass is
    // ~30 T-states; but we CAN catch $fd6c (the wipe trigger) because
    // it survives for several T-states and the loader walks through
    // it just before jumping to $fbcb. Step one T-state at a time
    // through the suspect window only — fall back to coarse run
    // outside it.
    let mut prev_pc = u16::MAX;
    let mut current_frames: u32 = 0;
    let mut wipe_hits: Vec<(u32, u16, u8, u8)> = Vec::new();
    let mut shift_hits: Vec<(u32, u8, u8)> = Vec::new();
    let mut last_l: u8 = 0;
    let mut pc_tail: std::collections::VecDeque<u16> = std::collections::VecDeque::with_capacity(64);
    'outer: while current_frames < max_frames {
        // Coarse step: 1 frame at a time until we're "close" to a
        // hot region (PC in $fd00..$fe00 or $fbc0..$fbe0).
        session.run_frames(1).expect("step");
        current_frames += 1;
        let pc = session.machine().machine().z80().regs.pc;
        let near_decoder = (0xfc00..=0xfdff).contains(&pc) || (0xfbc0..=0xfbe0).contains(&pc);
        if !near_decoder {
            continue;
        }
        // Fine step: 1 T-state at a time for the next 200000 T-states
        // (~3 frames), watching for the trigger PCs.
        for _t in 0..200_000 {
            session.machine_mut().machine_mut().advance_tstates(1);
            let z = session.machine().machine().z80();
            let pc = z.regs.pc;
            if pc == prev_pc {
                continue;
            }
            prev_pc = pc;
            if pc_tail.len() == 60 {
                pc_tail.pop_front();
            }
            pc_tail.push_back(pc);
            if pc == 0xfd5f {
                shift_hits.push((current_frames, z.regs.b(), z.regs.l()));
                last_l = z.regs.l();
            }
            if pc == 0xfd9c {
                // Entry into the SECOND bit-shift verifier (after
                // each pulse measurement). RET NC if CY=0 (timeout).
                // Log when this fires so we can see whether the
                // loader skips it (CY=0 path) or runs it (CY=1 path).
                let cy = (z.regs.f() & 0x01) != 0;
                if !cy {
                    // RET NC fires — no XOR-fold update this iter.
                    // Only log occasionally to avoid noise.
                }
            }
            if pc == 0xfec6 {
                let h = (z.regs.hl >> 8) as u8;
                eprintln!(
                    "[{label}] XOR-fold check at $fec6 at frame ~{current_frames}: H=${h:02x} → {}",
                    if h == 0 { "PASS (no wipe)" } else { "WIPE FIRES" },
                );
            }
            if pc == 0xfd6c {
                // Verifier compare. Log L and the implied outcome,
                // but don't break — we want to keep tracing to find
                // the wipe firing later (if it does).
                let l = z.regs.l();
                wipe_hits.push((current_frames, pc, z.regs.b(), l));
                eprintln!(
                    "[{label}] verifier compare at $fd6c at frame ~{current_frames}: L=${l:02x} → {}",
                    if l == 0x3A { "PASS (no wipe)" } else { "WIPE FIRES" },
                );
            }
            // Match the wipe sled body specifically — PC=$fbd0 is
            // `INC IY` inside the `INC IY ; JR -8` loop. Other PCs
            // in $fbc0..$fbe0 are benign loader code traversed during
            // normal execution.
            if pc == 0xfbd0 {
                eprintln!("[{label}] entered wipe sled at PC=$fbd0 at frame ~{current_frames}");
                let tail: Vec<String> = pc_tail.iter().map(|p| format!("${p:04x}")).collect();
                eprintln!("[{label}] last 60 PCs before wipe: {}", tail.join(" "));
                // Dump bytes around interesting PCs from the tail —
                // anything not in the byte-decoder ($fcd0-$fd00) is
                // likely the originating check site.
                for &region_base in &[0xfd9cu16, 0xfeb0u16, 0xfec0u16, 0xfef0u16, 0xff00u16, 0xfbc0u16] {
                    let bytes: Vec<u8> = (0..32)
                        .map(|i| {
                            session
                                .machine()
                                .machine()
                                .read_byte(region_base.wrapping_add(i as u16))
                        })
                        .collect();
                    let hex = bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    eprintln!("[{label}] bytes at ${region_base:04x}: {hex}");
                }
                break 'outer;
            }
        }
    }

    eprintln!(
        "[{label}] last L at $fd5f before exit: ${last_l:02x}; shift_hits={}, wipe_hits={}",
        shift_hits.len(),
        wipe_hits.len(),
    );
    let last_few: Vec<String> = shift_hits
        .iter()
        .rev()
        .take(20)
        .map(|(f, b, l)| format!("f{f}:B=${b:02x},L=${l:02x}"))
        .collect();
    eprintln!("[{label}] last 20 $fd5f hits: {}", last_few.join(" "));
}

#[test]
#[ignore = "diagnostic — needs 48K ROM and Op Wolf SpeedLock 7 TZX"]
fn trace_speedlock7_byte_decoder_b_values() {
    // Run Op Wolf to frame 1700 (just before pulses start), then drop
    // to single-T-state stepping and capture register state every time
    // PC transitions into the bit-shift loop ($fd5f..$fd6f) or hits the
    // wipe-trigger compare at $fd6b. This gives us the actual per-bit
    // sequence the chip sees, without needing to disassemble FUSE.
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_file = "ARCADE COLLECTION 20 - Operation Wolf (1991)(Hit Squad, The)[SpeedLock 7].zip";
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_file);
    if !firmware_root.exists() || !tzx_path.exists() {
        return;
    }
    let rom_bytes = read_firmware_asset(&firmware_root.join("48.rom")).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");

    // Hard-coded entry point — frame at which our chip first reaches
    // the pulse-decode region. From earlier diagnostic runs this is
    // ~frame 1400; we go a touch earlier so we capture the entry.
    let entry_frame: u32 = std::env::var("ENTRY_FRAME")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1380);
    session.run_frames(entry_frame).expect("run_frames");

    // Snapshot the bit-shift region before the wipe blanks it.
    eprintln!("\n=== Loader bytes $fd00..$fd80 at frame 1700 ===");
    for row in 0..8 {
        let addr: u16 = 0xfd00 + row * 16;
        let bytes: Vec<u8> = (0..16)
            .map(|i| session.machine().machine().read_byte(addr.wrapping_add(i as u16)))
            .collect();
        let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
        eprintln!("  ${addr:04x}: {hex}");
    }
    // Also dump the threshold/seed table region $feb0..$fec0.
    eprintln!("\n=== Loader bytes $feb0..$fec0 ===");
    let bytes: Vec<u8> = (0..16)
        .map(|i| session.machine().machine().read_byte(0xfeb0u16.wrapping_add(i as u16)))
        .collect();
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
    eprintln!("  $feb0: {hex}");

    // Dump the inner-decoder region $fcc0..$fd00.
    eprintln!("\n=== Loader bytes $fcc0..$fd00 (decoder core) ===");
    for row in 0..4 {
        let addr: u16 = 0xfcc0 + row * 16;
        let bytes: Vec<u8> = (0..16)
            .map(|i| session.machine().machine().read_byte(addr.wrapping_add(i as u16)))
            .collect();
        let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
        eprintln!("  ${addr:04x}: {hex}");
    }

    // Single-T-state stepping. Budget: 100 frames × 69 888 T = 6.99M
    // T-states max. In release we expect ≲30 s.
    //
    // We record three event types in time order:
    //  - `D`: PC reached `$FCDB`. The Speedlock-7 inner-delay routine
    //    starts here when called directly (bypassing the default
    //    `LD A,$16` at `$FCD9`). The A register holds the seed.
    //  - `F`: PC reached `$FCD9` (the canonical entry with default
    //    `LD A,$16`).
    //  - `B`: PC reached `$fd5f` (the bit-shift loop). Captures B/L
    //    and the `TapePlayer::current_span` + remaining T-state
    //    countdown at the moment of the compare.
    //
    // The interleaving of D/F events between B hits tells us the
    // delay-seed sequence the loader is using; the captured spans
    // tell us what pulse widths the player is actually delivering.

    #[derive(Debug)]
    enum Event {
        DelaySeed { t: u32, pc: u16, a: u8, span_idx: usize, countdown: u32 },
        FullCall { t: u32, span_idx: usize, countdown: u32, b_entry: u8 },
        PilotAfter { t: u32, b: u8, span_idx: usize },
        BitShift {
            t: u32,
            pc: u16,
            b: u8,
            l: u8,
            ear: bool,
            span: Option<TapeSpan>,
            countdown: u32,
            span_idx: usize,
        },
    }

    let max_tstates: u32 = 600 * TIMING_48K.tstates_per_frame;
    let mut events: Vec<Event> = Vec::new();
    let mut prev_pc = u16::MAX;
    let mut stopped_at: Option<&str> = None;
    for t in 0..max_tstates {
        session.machine_mut().machine_mut().advance_tstates(1);
        let machine = session.machine().machine();
        let pc = machine.z80().regs.pc;
        if pc == prev_pc {
            continue;
        }
        let (span_idx, _) = machine.tape().span_position();
        let countdown = machine.tape().span_countdown();
        match pc {
            0xFCD5 => {
                let b_entry = machine.z80().regs.b();
                events.push(Event::FullCall { t, span_idx, countdown, b_entry });
            }
            0xFCD9 | 0xFCDB => {
                let a = machine.z80().regs.a();
                events.push(Event::DelaySeed { t, pc, a, span_idx, countdown });
            }
            // After CALL $FCD5 returns in pilot detection ($fd12 = JR NC)
            // or in pre-check ($fd37). At these PCs B has the just-measured pulse count.
            0xfd12 | 0xfd37 => {
                let b = machine.z80().regs.b();
                events.push(Event::PilotAfter { t, b, span_idx });
            }
            0xfd5f => {
                let b = machine.z80().regs.b();
                let l = machine.z80().regs.l();
                let ear = machine.tape().ear_level();
                let span = machine.tape().current_span().cloned();
                events.push(Event::BitShift {
                    t, pc, b, l, ear, span, countdown, span_idx,
                });
            }
            _ => {}
        }
        if (0xFBC0..=0xFBE0).contains(&pc) {
            stopped_at = Some("wipe zone");
            break;
        }
        prev_pc = pc;
    }

    eprintln!("\n=== Speedlock-7 byte-decoder event trace ===");
    eprintln!("stopped: {stopped_at:?}");
    eprintln!("events: {}", events.len());
    let mut prev_t: Option<u32> = None;
    let mut prev_span: Option<usize> = None;
    // Aggregate: count consecutive DelaySeed events that form one
    // countdown (A descending from some N down to $01). Print once
    // per call.
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Event::DelaySeed { t, a, span_idx, countdown, .. } => {
                // Walk forward while events are DelaySeeds with monotonically decreasing A.
                let start_t = *t;
                let start_a = *a;
                let mut end_a = *a;
                let mut end_idx = *span_idx;
                let mut end_countdown = *countdown;
                let mut j = i + 1;
                while j < events.len() {
                    if let Event::DelaySeed { a: a2, span_idx: si2, countdown: cd2, .. } = &events[j] {
                        if *a2 < end_a {
                            end_a = *a2;
                            end_idx = *si2;
                            end_countdown = *cd2;
                            j += 1;
                            continue;
                        }
                    }
                    break;
                }
                let pulse_gap = prev_t.map_or(0, |p| start_t.saturating_sub(p));
                let span_delta = prev_span.map_or(0, |p| end_idx.saturating_sub(p));
                eprintln!(
                    "  +{:7}T DELAY A=${:02x}→${:02x} span_idx={} (Δ{}) countdown={} Δprev={}T",
                    start_t, start_a, end_a, end_idx, span_delta, end_countdown, pulse_gap,
                );
                prev_t = Some(start_t);
                prev_span = Some(end_idx);
                i = j;
            }
            Event::FullCall { t, span_idx, countdown, b_entry } => {
                eprintln!(
                    "  +{t:7}T  FCD5  CALL  span_idx={span_idx} countdown={countdown} B_entry=${b_entry:02x}",
                );
                i += 1;
            }
            Event::PilotAfter { t, b, span_idx } => {
                eprintln!(
                    "  +{t:7}T  AFTER B=${b:02x}({}) span_idx={span_idx}",
                    *b as i32 - 0x9c,
                );
                i += 1;
            }
            Event::BitShift { t, b, l, ear, span, countdown, span_idx, .. } => {
                eprintln!(
                    "  +{:7}T  *** BIT  B=${:02x}({:>3})  L=${:02x}  bit={}  ear={}  span_idx={}  countdown={}  span={:?}",
                    t, b, *b as i32 - 0x9e,
                    if *b > 0xBC { 1 } else { 0 }, l,
                    ear, span_idx, countdown, span,
                );
                i += 1;
            }
        }
    }
}

#[test]
#[ignore = "diagnostic — needs 48K ROM and Op Wolf SpeedLock 7 TZX"]
fn sample_border_color_through_loader() {
    // The Speedlock-7 byte-decoder at $FCE3 toggles the border colour
    // (OUT $FE, A) on every successful edge detect. If the loader is
    // actually decoding pulses, the border bottom strip will be
    // flashing through colours. If the border stays static black,
    // the loop is timing out (no edges seen).
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_file = "ARCADE COLLECTION 20 - Operation Wolf (1991)(Hit Squad, The)[SpeedLock 7].zip";
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_file);
    if !firmware_root.exists() || !tzx_path.exists() {
        return;
    }

    let rom_path = firmware_root.join("48.rom");
    let rom_bytes = read_firmware_asset(&rom_path).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");

    // Walk a frame ladder and at each frame sample: tape state, border
    // colour, PC, and whether the EAR level has changed since the
    // previous sample.
    let stages = [
        1700u32, 1710, 1720, 1730, 1740, 1750, 1760, 1770, 1780, 1790, 1800,
    ];
    let mut prior_ear: Option<bool> = None;
    let mut prior_span: Option<usize> = None;
    let mut cumulative = 0u32;
    for &budget in &stages {
        session.run_frames(budget - cumulative).expect("run_frames");
        cumulative = budget;
        let machine = session.machine().machine();
        let pc = machine.z80().regs.pc;
        let ear = machine.tape().ear_level();
        let (span_idx, span_total) = machine.tape().span_position();
        let countdown = machine.tape().span_countdown();
        let span = machine.tape().current_span();
        let b_reg = machine.z80().regs.b();
        let l_reg = machine.z80().regs.l();
        let e_reg = machine.z80().regs.e();
        let hl = machine.z80().regs.hl;
        let ear_changed = prior_ear.map_or("?".to_owned(), |p| if p != ear { "CHANGED".into() } else { "same".into() });
        let span_changed = prior_span.map_or("?".to_owned(), |p| if p != span_idx { format!("Δ={}", span_idx.saturating_sub(p)) } else { "same".into() });
        eprintln!(
            "frame {budget:5}: PC=${pc:04x} EAR={ear} ({ear_changed}) span={span_idx}/{span_total} ({span_changed}) countdown={countdown:6} B=${b_reg:02x} L=${l_reg:02x} E=${e_reg:02x} HL=${hl:04x} span={span:?}",
        );
        prior_ear = Some(ear);
        prior_span = Some(span_idx);
    }
}

/// Helper: load `tzx_relative_path`, run past the protection's wipe-fire
/// window, then sample PC at five well-spaced frames. A live loader walks
/// a wide PC range; a wedged one is pinned to a ≤2-instruction sled.
/// Returns `Ok(())` when alive, `Err(message)` when wedged. Callers decide
/// whether wedged is a hard failure (the title is supposed to work) or an
/// expected outcome (the title is a known separate investigation).
fn check_speedlock_loader_alive(label: &str, tzx_relative_path: &str) -> Result<(), String> {
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_relative_path);
    if !firmware_root.exists() || !tzx_path.exists() {
        eprintln!("[skip {label}] firmware or TZX missing");
        return Ok(());
    }
    let rom_bytes = read_firmware_asset(&firmware_root.join("48.rom")).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");

    let mut current: u32 = 0;
    let mut samples: Vec<u16> = Vec::new();
    for target in [2000u32, 2500, 3000, 3500, 4000] {
        let delta = target - current;
        session.run_frames(delta).expect("run_frames");
        current = target;
        let pc = session.machine().machine().z80().regs.pc;
        samples.push(pc);
        eprintln!("[{label}] frame {target}: PC=${pc:04x}");
    }

    let mut sorted = samples.clone();
    sorted.sort_unstable();
    sorted.dedup();
    let distinct = sorted.len();
    let pc_min = *sorted.first().expect("at least one sample");
    let pc_max = *sorted.last().expect("at least one sample");
    let pc_spread = pc_max - pc_min;

    if distinct >= 3 && pc_spread >= 0x40 {
        Ok(())
    } else {
        Err(format!(
            "[{label}] loader appears wedged: samples={samples:04x?}, distinct={distinct}, spread=${pc_spread:04x}"
        ))
    }
}

#[test]
#[ignore = "diagnostic — needs 48K ROM and Op Wolf SpeedLock 7 TZX"]
fn opwolf_loads_past_speedlock_wipe() {
    check_speedlock_loader_alive(
        "Op Wolf [Speedlock 7]",
        "ARCADE COLLECTION 20 - Operation Wolf (1991)(Hit Squad, The)[SpeedLock 7].zip",
    )
    .expect("Op Wolf should clear the Speedlock-7 wipe");
}

#[test]
#[ignore = "diagnostic — sweep verification for Speedlock-7 Green Beret"]
fn green_beret_loads_past_speedlock_wipe() {
    check_speedlock_loader_alive(
        "Green Beret [Speedlock 7]",
        "ARCADE COLLECTION 02 - Green Beret (1989)(Hit Squad, The)[SpeedLock 7].zip",
    )
    .expect("Green Beret should clear the Speedlock-7 wipe");
}

#[test]
#[ignore = "diagnostic — comparison probe for working title (Op Wolf)"]
fn probe_op_wolf_tape_and_pc_evolution() {
    probe_tape_and_pc(
        "Op Wolf",
        "ARCADE COLLECTION 20 - Operation Wolf (1991)(Hit Squad, The)[SpeedLock 7].zip",
    );
}

#[test]
#[ignore = "diagnostic — Green Beret black-screen investigation"]
fn probe_green_beret_tape_and_pc_evolution() {
    probe_tape_and_pc(
        "Green Beret",
        "ARCADE COLLECTION 02 - Green Beret (1989)(Hit Squad, The)[SpeedLock 7].zip",
    );
}

fn probe_tape_and_pc(label: &str, tzx_relative_path: &str) {
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_relative_path);
    if !firmware_root.exists() || !tzx_path.exists() {
        return;
    }
    let rom_bytes = read_firmware_asset(&firmware_root.join("48.rom")).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");

    let mut current: u32 = 0;
    let mut prev_pc_set: Vec<u16> = Vec::new();
    for target in [
        500u32, 1000, 1500, 2000, 2500, 3000, 4000, 5000, 6000, 8000, 10000,
        12000, 15000, 20000, 25000, 30000,
    ] {
        // Step the gap a frame at a time over the last 50 frames to
        // collect a PC histogram (= how much code is being exercised).
        let mid = target.saturating_sub(50);
        session.run_frames(mid - current).expect("run_frames");
        current = mid;
        let mut pc_seen: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
        for _ in 0..50 {
            session.run_frames(1).expect("step");
            current += 1;
            pc_seen.insert(session.machine().machine().z80().regs.pc);
        }
        let machine = session.machine().machine();
        let pc = machine.z80().regs.pc;
        let (span_idx, span_total) = machine.tape().span_position();
        let playing = machine.tape().is_playing();
        let unique_pcs: Vec<u16> = pc_seen.iter().copied().collect();
        let new_pcs: Vec<u16> = unique_pcs
            .iter()
            .filter(|p| !prev_pc_set.contains(p))
            .copied()
            .collect();
        prev_pc_set = unique_pcs.clone();
        eprintln!(
            "[{label}] frame {target:5}: PC=${pc:04x} tape={playing} span={span_idx}/{span_total} unique_PCs={} (new this window: {})",
            unique_pcs.len(),
            new_pcs.len(),
        );
        // Print first few new PCs to spot transitions
        if !new_pcs.is_empty() && new_pcs.len() <= 8 {
            let pcs_str: Vec<String> = new_pcs.iter().map(|p| format!("${p:04x}")).collect();
            eprintln!("  new PCs: {}", pcs_str.join(" "));
        }
    }
}

#[test]
#[ignore = "diagnostic — needs 48K ROM and Bubble Bobble SpeedLock 5 TZX"]
fn bubble_bobble_loads_past_speedlock_wipe() {
    check_speedlock_loader_alive(
        "Bubble Bobble [Speedlock 5]",
        "ARCADE COLLECTION 30 - Bubble Bobble (1992)(Hit Squad, The)(48K-128K)[SpeedLock 5][re-release].zip",
    )
    .expect("Bubble Bobble should clear the Speedlock-5 wipe");
}

/// Speedlock-2 (Head over Heels) reuses Speedlock-7's byte-decoder
/// loop verbatim (same code, just relocated to `$fd2c..$fd3b`
/// instead of `$fcdb..$fce9`). The TZX is a mix of `0x10` standard
/// blocks, `0x12`/`0x13` pilot+sync sequences, and 11 `0x14` data
/// blocks (all with `bits_last = 8`, so the partial-last-byte fix
/// does not apply).
///
/// Behaviour: the loader reads the whole tape — all 835 729 spans
/// drain by frame 16000 — then sits in the byte-decoder loop
/// polling EAR for more pulses that never come. By frame 30000 the
/// border has turned red, the canonical "tape verify failed"
/// indicator. So the loader is decoding *something* wrong during
/// the data pass — the failure isn't pulse-edge timing or
/// partial-byte parsing.
///
/// This test is diagnostic-only; it prints the live state but
/// never asserts. Promote to a hard assertion once Speedlock-2 is
/// fixed.
#[test]
#[ignore = "diagnostic — Speedlock-2 is a known separate investigation"]
fn head_over_heels_speedlock2_status() {
    match check_speedlock_loader_alive(
        "Head over Heels [Speedlock 2]",
        "ARCADE COLLECTION 12 - Head over Heels (1990)(Hit Squad, The)(48K-128K)[SpeedLock 2].zip",
    ) {
        Ok(()) => eprintln!("Head over Heels: loader varies PC — may be loading correctly!"),
        Err(msg) => eprintln!("Head over Heels: PC pinned in byte-decoder loop ({msg}). Run dump_speedlock2_head_over_heels_tape_state to see whether the tape drained."),
    }
}

#[test]
#[ignore = "diagnostic — needs 48K ROM and Head over Heels SpeedLock 2 TZX"]
fn dump_speedlock2_head_over_heels_tape_state() {
    // Sample tape player state + PC across frames 1000-5000 for
    // Head over Heels. The loader appears wedged in the DEC A
    // delay loop at $fd2e — we want to know what the tape is
    // doing at the same moment (still pilot? mid-data? consumed?).
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_file = "ARCADE COLLECTION 12 - Head over Heels (1990)(Hit Squad, The)(48K-128K)[SpeedLock 2].zip";
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_file);
    if !firmware_root.exists() || !tzx_path.exists() {
        return;
    }
    let rom_bytes = read_firmware_asset(&firmware_root.join("48.rom")).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");

    let mut current: u32 = 0;
    for target in [500u32, 2000, 5000, 8000, 12000, 16000, 20000, 25000, 30000] {
        session.run_frames(target - current).expect("run_frames");
        current = target;
        let machine = session.machine().machine();
        let pc = machine.z80().regs.pc;
        let (span_idx, span_total) = machine.tape().span_position();
        let playing = machine.tape().is_playing();
        eprintln!(
            "frame {target:5}: PC=${pc:04x} tape={playing} span={span_idx}/{span_total}",
        );
    }

    // Capture a screenshot at the final frame so we can see where
    // the loader ends up.
    let machine = session.machine().machine();
    let fb = machine.framebuffer();
    let width = 352usize;
    let height = 296usize;
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        // Convert palette-index framebuffer to RGB using the ULA's
        // standard 16-colour palette.
        let palette: [(u8, u8, u8); 16] = [
            (0x00, 0x00, 0x00), (0x00, 0x00, 0xCD), (0xCD, 0x00, 0x00), (0xCD, 0x00, 0xCD),
            (0x00, 0xCD, 0x00), (0x00, 0xCD, 0xCD), (0xCD, 0xCD, 0x00), (0xCD, 0xCD, 0xCD),
            (0x00, 0x00, 0x00), (0x00, 0x00, 0xFF), (0xFF, 0x00, 0x00), (0xFF, 0x00, 0xFF),
            (0x00, 0xFF, 0x00), (0x00, 0xFF, 0xFF), (0xFF, 0xFF, 0x00), (0xFF, 0xFF, 0xFF),
        ];
        let mut rgb = Vec::with_capacity(width * height * 3);
        for &idx in fb.iter() {
            let (r, g, b) = palette[(idx as usize) & 0x0F];
            rgb.extend_from_slice(&[r, g, b]);
        }
        writer.write_image_data(&rgb).expect("png write");
    }
    std::fs::write("/tmp/speedlock-screenshots/head-over-heels-frame30000.png", &png_bytes).ok();
    eprintln!("Wrote /tmp/speedlock-screenshots/head-over-heels-frame30000.png");
}

#[test]
#[ignore = "diagnostic — needs 48K ROM and Head over Heels SpeedLock 2 TZX"]
fn dump_speedlock2_head_over_heels_loader_bytes() {
    // Speedlock-2 wedges PC at $fd2e..$fd3b — the same range our
    // Speedlock-7 disassembly identified as the 7-iter pre-check
    // loop. This test dumps the loader region $fd00..$fe00 so we
    // can confirm whether Speedlock-2 reuses Speedlock-7's loader
    // code verbatim (suggesting a tape/parser fix similar to the
    // 0x14 case) or has its own different code.
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_file = "ARCADE COLLECTION 12 - Head over Heels (1990)(Hit Squad, The)(48K-128K)[SpeedLock 2].zip";
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_file);
    if !firmware_root.exists() || !tzx_path.exists() {
        return;
    }
    let rom_bytes = read_firmware_asset(&firmware_root.join("48.rom")).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");

    // Run to where PC is in the wedge ($fd2e..$fd3b) so the loader
    // is fully resident in RAM (post-decryption if any).
    session.run_frames(2000).expect("run_frames");

    for base in [0xfc00u16, 0xfd00, 0xfe00] {
        eprintln!("\n=== Head over Heels [Speedlock 2] bytes ${base:04x}..+0x100 ===");
        for row in 0..16 {
            let addr = base.wrapping_add(row * 16);
            let bytes: Vec<u8> = (0..16)
                .map(|i| session.machine().machine().read_byte(addr.wrapping_add(i as u16)))
                .collect();
            let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            eprintln!("  ${addr:04x}: {hex}");
        }
    }

    // Also dump PC, B, C, HL, DE so we know the live state.
    let z = session.machine().machine().z80();
    eprintln!(
        "\nfinal state @ frame 2000: PC=${:04x} B=${:02x} C=${:02x} D=${:02x} E=${:02x} HL=${:04x}",
        z.regs.pc, z.regs.b(), z.regs.c(), z.regs.d(), z.regs.e(), z.regs.hl,
    );
}

#[test]
#[ignore = "diagnostic — needs Op Wolf SpeedLock 7 TZX"]
fn dump_speedlock7_tzx_span_widths_around_57050() {
    // Parse the Op Wolf TZX into a TapeSpan stream and dump the span
    // sequence around span index 57050 — that's where the byte-decoder
    // trace runs and where the bit-shift loop sees its inputs. We need
    // to confirm whether the TZX file actually contains the
    // PILOT-width pulses Speedlock's bit-shift hash expects, or
    // whether all spans in this region are data-width.
    let tzx_file = "ARCADE COLLECTION 20 - Operation Wolf (1991)(Hit Squad, The)[SpeedLock 7].zip";
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_file);
    if !tzx_path.exists() {
        return;
    }
    let tape = read_media_asset(&tzx_path, MediaKind::Tape).expect("tzx");
    let stream = tzx_to_stream(&tape.bytes).expect("parse tzx");

    eprintln!("Total spans: {}", stream.len());

    // First, build a histogram of pulse widths across the whole tape
    // so we know the alphabet of widths the parser produces.
    let mut hist: std::collections::BTreeMap<u32, u32> =
        std::collections::BTreeMap::new();
    for span in &stream {
        if let TapeSpan::Pulse(w) = span {
            *hist.entry(*w).or_insert(0) += 1;
        }
    }
    eprintln!("\n=== Pulse-width histogram ===");
    for (width, count) in &hist {
        eprintln!("  Pulse({width:5}) × {count}");
    }

    // Now dump the spans near index 57050.
    eprintln!("\n=== Spans 57040..57080 ===");
    for idx in 57040..=57080.min(stream.len() - 1) {
        eprintln!("  [{idx}] {:?}", stream[idx]);
    }

    // Find the boundary where Pulse(1428) starts/stops appearing
    // around iter 1 of our trace. That tells us the block structure.
    eprintln!("\n=== Span-width transitions in window 56900..57200 ===");
    let mut last_width: Option<u32> = None;
    for idx in 56900..=57200.min(stream.len() - 1) {
        let w = match &stream[idx] {
            TapeSpan::Pulse(w) => Some(*w),
            _ => None,
        };
        if w != last_width {
            eprintln!("  [{idx}] {:?}", stream[idx]);
            last_width = w;
        }
    }
}

#[test]
#[ignore = "diagnostic — needs 48K ROM and Op Wolf SpeedLock 7 TZX"]
fn dump_speedlock7_decrypted_loader() {
    // Run to frame 1400 (after self-decryption, before anti-tamper wipe)
    // and capture $F48E..+0x0A5A to disk. Companion to
    // `dump_speedlock7_loader_ram` which scans across many frame budgets
    // to find the decryption window; this one captures it.
    let firmware_root = home().join(".emu198x/roms/sinclair-zx-spectrum-48k");
    let tzx_file = "ARCADE COLLECTION 20 - Operation Wolf (1991)(Hit Squad, The)[SpeedLock 7].zip";
    let tzx_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]")
        .join(tzx_file);
    if !firmware_root.exists() || !tzx_path.exists() {
        eprintln!("[skip] firmware or TZX missing");
        return;
    }

    let rom_path = firmware_root.join("48.rom");
    let rom_bytes = read_firmware_asset(&rom_path).expect("48K rom");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "sinclair-zx-spectrum-48k-rom".to_owned(),
        &rom_bytes.bytes,
    ));
    let runtime = Spectrum48kRuntime::from_firmware(&firmware).expect("48K runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
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
    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES).expect("autoload");
    session.run_frames(1400).expect("run_frames");

    let snapshot: Vec<u8> = (0..LOADER_LEN)
        .map(|i| session.machine().machine().read_byte(LOADER_BASE.wrapping_add(i as u16)))
        .collect();
    let out_path = PathBuf::from("/tmp/speedlock7-decrypted-f48e.bin");
    std::fs::write(&out_path, &snapshot).expect("write decrypted dump");
    let in_fe = (0..snapshot.len() - 1)
        .filter(|&i| snapshot[i] == 0xDB && snapshot[i + 1] == 0xFE)
        .count();
    eprintln!(
        "Wrote decrypted loader snapshot ({} bytes, {} IN($FE) visible) to {}",
        snapshot.len(),
        in_fe,
        out_path.display()
    );
}
