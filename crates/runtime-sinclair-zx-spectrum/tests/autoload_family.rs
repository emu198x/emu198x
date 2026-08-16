//! `autoload_basic_tape` across the boot styles the Spectrum family has.
//!
//! A 48K reaches a BASIC editor and is typed into; the 128K family reaches
//! a menu and is navigated instead. Before #50 the helper only knew the
//! first, so every 128K-family autoload failed with "48K BASIC prompt was
//! not ready" — the machine had booted fine, the helper was looking for
//! the wrong thing.
//!
//! The two variants covered here are the extremes of the menu path. The
//! 128K draws its menu about five frames after the boot banner; the +3
//! tries its disk drive first and does not offer tape for some 2,600
//! frames. Anything in between is bracketed by the pair.
//!
//! ```text
//! cargo test -p runtime-sinclair-zx-spectrum --test autoload_family -- --ignored
//! ```

use std::env;
use std::path::PathBuf;

use common_sinclair_zx_spectrum::timing::{TIMING_128K, TIMING_PLUS2A};
use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, MediaImage, MediaKind, MediaSet,
    read_firmware_asset, read_media_asset,
};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Spectrum128kRuntime, SpectrumMachine, SpectrumPlus3Runtime,
    SpectrumRuntime, SpectrumSessionQueryProvider, autoload_basic_tape,
};

/// Frames run after autoload before looking for the loading indicator.
///
/// Comfortably past the +3's disk timeout, which is the slowest thing in
/// the family by an order of magnitude.
const LOAD_FRAMES: u32 = 9_000;

/// The ROM prints this while a tape block is coming in. Machine-observable
/// evidence that the tape is actually being read, rather than that a key
/// was pressed and nothing objected.
const LOADING_MARKER: &str = "Bytes:";

fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").expect("HOME"))
}

fn tape_path() -> PathBuf {
    env::var_os("EMU198X_SPECTRUM_TZX_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/test-data/spectrum-system-tests/tosec"))
        .join("manic-miner-bug-byte.tzx")
}

fn firmware_from(dir: &str, files: &[(&str, &str)]) -> Option<Vec<(String, Vec<u8>)>> {
    let root = home().join(".emu198x/roms").join(dir);
    let mut out = Vec::new();
    for (id, file) in files {
        let path = root.join(file);
        if !path.exists() {
            return None;
        }
        out.push((
            (*id).to_owned(),
            read_firmware_asset(&path).ok()?.bytes.to_vec(),
        ));
    }
    Some(out)
}

/// Autoload, run, and report whether the ROM started reading tape.
fn loads_via_autoload<M: SpectrumMachine>(
    runtime: SpectrumRuntime<M>,
    frame_halfcycles: u32,
) -> bool {
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(frame_halfcycles),
        SpectrumSessionQueryProvider,
    );
    let tape = read_media_asset(&tape_path(), MediaKind::Tape).expect("tape media");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "tape-1".to_owned(),
        MediaKind::Tape,
        &tape.bytes,
    ));
    session.prepare(&media, &[]).expect("prepare");

    autoload_basic_tape(&mut session, "tape-1", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
        .expect("autoload should drive whichever boot destination this variant reaches");

    session.run_frames(LOAD_FRAMES).expect("run");
    let lines = session.query("screen.text.lines").expect("screen text");
    lines
        .value
        .as_array()
        .expect("array of lines")
        .iter()
        .filter_map(|v| v.as_str())
        .any(|line| line.contains(LOADING_MARKER))
}

#[test]
#[ignore = "needs the 128K ROMs and the Manic Miner TZX — run with --ignored"]
fn autoload_drives_the_128k_boot_menu() {
    let Some(images) = firmware_from(
        "sinclair-zx-spectrum-128k",
        &[
            ("sinclair-zx-spectrum-128k-rom-0", "128-0.rom"),
            ("sinclair-zx-spectrum-128k-rom-1", "128-1.rom"),
        ],
    ) else {
        emu198x_test_skip::skip!("128K ROMs not staged");
    };
    if !tape_path().exists() {
        emu198x_test_skip::skip!("Manic Miner TZX not staged");
    }
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &images {
        firmware.push(FirmwareImage::new(id.clone(), bytes));
    }
    let runtime = Spectrum128kRuntime::from_firmware(&firmware).expect("128K runtime");
    assert!(
        loads_via_autoload(runtime, TIMING_128K.halfcycles_per_frame),
        "the 128K menu path did not reach a tape load"
    );
}

#[test]
#[ignore = "needs the +3 ROMs and the Manic Miner TZX — run with --ignored"]
fn autoload_waits_out_the_plus3_disk_timeout() {
    // The +3 is the reason the helper waits for the loader to ask for
    // tape rather than starting the transport on a fixed settle: it
    // offers tape only after its disk drive times out, and a tape rolled
    // before then plays its pilot tone at a ROM that is not listening.
    let Some(images) = firmware_from(
        "amstrad-zx-spectrum-plus3",
        &[
            ("sinclair-zx-spectrum-plus3-rom-0", "plus3-0.rom"),
            ("sinclair-zx-spectrum-plus3-rom-1", "plus3-1.rom"),
            ("sinclair-zx-spectrum-plus3-rom-2", "plus3-2.rom"),
            ("sinclair-zx-spectrum-plus3-rom-3", "plus3-3.rom"),
        ],
    ) else {
        emu198x_test_skip::skip!("+3 ROMs not staged");
    };
    if !tape_path().exists() {
        emu198x_test_skip::skip!("Manic Miner TZX not staged");
    }
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &images {
        firmware.push(FirmwareImage::new(id.clone(), bytes));
    }
    let runtime = SpectrumPlus3Runtime::from_firmware(&firmware).expect("+3 runtime");
    assert!(
        loads_via_autoload(runtime, TIMING_PLUS2A.halfcycles_per_frame),
        "the +3 did not reach a tape load after its disk timeout"
    );
}
