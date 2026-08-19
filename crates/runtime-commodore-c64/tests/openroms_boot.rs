//! The C64 booting real firmware — one nobody needs permission for.
//!
//! `boot_invariants.rs` has the same waypoint against Commodore's own
//! BASIC/KERNAL/CHARGEN, which cannot be distributed, so it only ever runs
//! on a machine that already has them. This runs the same machine against
//! [Open ROMs](https://github.com/MEGA65/open-roms): a clean-room BASIC and
//! KERNAL written against the documented `$FF81` jump table and published
//! under the GPL, so that emulators can ship legal firmware.
//!
//! It is not Commodore's KERNAL. Open ROMs is explicit that it is
//! incomplete, and a title reaching past the documented interface may
//! behave differently. For "does this machine start", that does not matter.
//!
//! Provisioned from the corpora store; `EMU198X_ROMS_ROOT` is the firmware
//! root and this joins `commodore-c64/` onto it. Upstream filenames are
//! kept so the images cannot be confused with Commodore's.

use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_commodore_c64::{C64Runtime, Model};

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

fn rom_dir() -> Option<PathBuf> {
    let root = std::env::var_os("EMU198X_ROMS_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms")))?;
    Some(root.join("commodore-c64"))
}

/// Screen RAM as text. Codes 1..=26 are the letters; 32..=63 coincide with
/// ASCII, which covers the space, the digits and the full stop `READY.`
/// ends on.
fn screen_text(machine: &machine_commodore_c64::C64) -> String {
    let mut out = String::with_capacity(25 * 41);
    for row in 0..25u16 {
        for col in 0..40u16 {
            let code = machine.memory().ram_read(0x0400 + row * 40 + col);
            out.push(match code {
                0 => '@',
                1..=26 => (b'A' + code - 1) as char,
                32..=63 => code as char,
                _ => '\u{fffd}',
            });
        }
        out.push('\n');
    }
    out
}

#[test]
#[ignore = "needs Open ROMs at <EMU198X_ROMS_ROOT>/commodore-c64/{kernal_generic,basic_generic,chargen_openroms}.rom"]
fn open_roms_cold_starts_to_a_basic_prompt() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("neither EMU198X_ROMS_ROOT nor HOME is set");
    };
    let kernal_path = dir.join("kernal_generic.rom");
    if !kernal_path.exists() {
        emu198x_test_skip::skip!("Open ROMs not staged at {}", dir.display());
    }

    let kernal = std::fs::read(&kernal_path).expect("KERNAL should read");
    let basic = std::fs::read(dir.join("basic_generic.rom")).expect("BASIC should read");
    let chargen =
        std::fs::read(dir.join("chargen_openroms.rom")).expect("character ROM should read");

    let mut runtime = C64Runtime::new(Model::C64PalBreadbin, kernal, basic, chargen, None)
        .expect("Open ROMs images should construct a C64 runtime");
    let mut host = null_host();

    // 300 PAL frames is ~6 seconds. Open ROMs prints its banner and the
    // free-memory count before the prompt, and takes longer than
    // Commodore's KERNAL to get there.
    let pal_frame_ticks: u64 = 985_248 / 50;
    runtime
        .run_until(MachineTime::new(300 * pal_frame_ticks), &mut host)
        .expect("the C64 should run 300 frames");

    let screen = screen_text(runtime.machine());

    // Three assertions, because any one alone is weak. The banner proves
    // the KERNAL ran its own initialisation rather than the machine
    // landing somewhere by accident; the byte count proves BASIC sized the
    // RAM it was given; `READY.` proves the interpreter reached its prompt.
    // A machine that hung shows a blank screen and fails all three.
    assert!(
        screen.contains("OPEN ROMS"),
        "Open ROMs should print its banner; screen was:\n{screen}"
    );
    assert!(
        screen.contains("BASIC BYTES FREE"),
        "BASIC should report the free RAM it sized; screen was:\n{screen}"
    );
    assert!(
        screen.contains("READY."),
        "BASIC should reach its READY. prompt; screen was:\n{screen}"
    );
}
