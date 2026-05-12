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

use common_sinclair_zx_spectrum::timing::TIMING_48K;
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
    let mut last_dump: Option<Vec<u8>> = None;
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
    return;

    let pc1 = session.machine().machine().z80().regs.pc;
    let port_fe = session.machine().machine().z80().regs.de; // proxy

    // Sample the bottom border (last row of framebuffer should be border).
    // 48K framebuffer is 352x296 indexed; bottom-row pixels are border.
    let fb = session.machine().machine().framebuffer();
    let mut bottom_palette_counts: std::collections::BTreeMap<u8, u32> =
        std::collections::BTreeMap::new();
    let width = 352usize;
    let height = 296usize;
    // Sample the bottom border (rows 280..295).
    for y in 280..height {
        for x in 0..width {
            let pixel = fb[y * width + x];
            *bottom_palette_counts.entry(pixel).or_insert(0) += 1;
        }
    }
    eprintln!("\n=== Border colour distribution at frame 1350 (PC=${pc1:04x}) ===");
    for (colour, count) in &bottom_palette_counts {
        eprintln!("  palette {colour:#04x} : {count} pixels");
    }
    let _ = port_fe;

    // Frame 1700 — supposedly just before the wipe.
    session.run_frames(350).expect("run_frames");
    let pc2 = session.machine().machine().z80().regs.pc;
    let fb = session.machine().machine().framebuffer();
    let mut counts2: std::collections::BTreeMap<u8, u32> = std::collections::BTreeMap::new();
    for y in 280..height {
        for x in 0..width {
            *counts2.entry(fb[y * width + x]).or_insert(0) += 1;
        }
    }
    eprintln!("\n=== Border colour distribution at frame 1700 (PC=${pc2:04x}) ===");
    for (colour, count) in &counts2 {
        eprintln!("  palette {colour:#04x} : {count} pixels");
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
