//! Tape autoload on a machine that has already been used (#1569).
//!
//! The autoload waits for the copyright banner before typing `LOAD ""`. A
//! learner who has started the machine and typed a line has cleared that
//! banner, so the wait used to time out with "copyright banner not
//! visible" until the machine was restarted by hand. Autoload starts from
//! power-on, so it resets a machine that has already run.
//!
//! ```text
//! cargo test -p runtime-sinclair-zx-spectrum --test autoload_running_machine -- --ignored
//! ```

use std::path::PathBuf;

use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore, MediaImage, MediaKind,
    MediaSet,
};
use runtime_sinclair_zx_spectrum::{
    Model, SpectrumRuntimeKind, SpectrumSessionQueryProvider, tap_key,
};

/// Frames allowed for the tape to load and its program to run. A two-block
/// tape this small loads in well under 1,000 frames at normal speed.
const LOAD_FRAMES: u32 = 2_000;

fn rom_path() -> PathBuf {
    std::env::var_os("EMU198X_SPECTRUM_48K_ROM").map_or_else(
        || {
            PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
        },
        PathBuf::from,
    )
}

fn tap_block(flag: u8, payload: &[u8]) -> Vec<u8> {
    let length = u16::try_from(payload.len() + 2).expect("small block");
    let mut block = length.to_le_bytes().to_vec();
    block.push(flag);
    block.extend_from_slice(payload);
    block.push(payload.iter().fold(flag, |sum, byte| sum ^ byte));
    block
}

/// A self-starting tape holding `10 PRINT 7`.
fn print_seven_tape() -> Vec<u8> {
    // Line 10, length 9: PRINT, "7", the hidden five-byte number, ENTER.
    let program = [
        0x00, 0x0A, 0x09, 0x00, 0xF5, b'7', 0x0E, 0x00, 0x00, 0x07, 0x00, 0x00, 0x0D,
    ];
    let length = u16::try_from(program.len()).expect("small program");
    let mut header = vec![0x00];
    header.extend_from_slice(b"probe     ");
    header.extend_from_slice(&length.to_le_bytes());
    header.extend_from_slice(&10u16.to_le_bytes());
    header.extend_from_slice(&length.to_le_bytes());
    let mut tape = tap_block(0x00, &header);
    tape.extend(tap_block(0xFF, &program));
    tape
}

#[test]
#[ignore = "FIXTURE: needs the 48K Spectrum ROM — run with --ignored"]
fn autoload_loads_a_tape_on_a_machine_that_has_already_been_used() {
    let rom = std::fs::read(rom_path()).expect("needs the 48K ROM");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("sinclair-zx-spectrum-48k-rom", &rom));
    let runtime = SpectrumRuntimeKind::from_firmware(Model::Spectrum48KPal, &firmware)
        .expect("the 48K builds from its ROM");
    let frame_ticks = u64::from(runtime.frame_halfcycles());
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        frame_ticks,
        SpectrumSessionQueryProvider,
    );

    // Use the machine the way the issue's learner did: boot, open the
    // editor, and run `PRINT 1`, which replaces the copyright banner.
    session.wait_for_boot(250).expect("the 48K boots");
    tap_key(&mut session, "enter").expect("open the editor");
    session.run_frames(20).expect("run");
    for key in ["p", "1", "enter"] {
        tap_key(&mut session, key).expect("type");
    }
    session
        .wait_for_query_text_contains("screen.text.lines", "0 OK, 0:1", 100)
        .expect("PRINT 1 ran");

    let tape = print_seven_tape();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &tape));
    session.load_media(&media).expect("the tape inserts");

    <SpectrumRuntimeKind as MachineCore>::autoload_tape(&mut session, "tape-1", 0)
        .expect("autoload works on a machine that has already been used");

    session
        .wait_for_query_text_contains("screen.text.lines", "0 OK, 10:1", LOAD_FRAMES)
        .expect("the tape's program loaded and ran");
}
