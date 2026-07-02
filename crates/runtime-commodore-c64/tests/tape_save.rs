//! Real SAVE round-trip for the C64 runtime via a writable datasette tape.
//!
//! `#[ignore]`'d — requires local C64 ROMs at `~/.emu198x/roms/commodore-c64/`.
//! Proves the tape write path end to end: the KERNAL's SAVE routine toggles the
//! cassette write line, the datasette records the pulse train, and the flush
//! encodes it into a valid `.tap` that parses back. Rides the same write-back
//! model as the disk SAVE. See `knowledge/decisions/disk-save-write-back.md`.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{
    ControlCommand, HeadlessSession, MediaImage, MediaKind, MediaSet, MediaTransportAction,
    MediaTransportCommand,
};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model, type_string};

use common::{local_rom_firmware, wait_for_screen_line_contains};

#[test]
#[ignore = "requires local C64 ROMs at ~/.emu198x/roms/commodore-c64/"]
fn save_records_a_readable_tap_on_a_writable_tape() {
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware())
        .expect("local ROMs should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );

    // Boot to READY., then mount a blank writable tape (the SAVE work image).
    wait_for_screen_line_contains(&mut session, 5, "READY.", 600);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &[]).writable(true));
    session
        .load_media(&media)
        .expect("blank tape should mount writable");

    // Enter a one-line program and SAVE it to tape (device 1, the default).
    type_string(&mut session, "10 PRINT \"HI\"\n", 3, 10).expect("typing the program");
    type_string(&mut session, "SAVE\"HI\"\n", 3, 10).expect("typing the SAVE command");

    // The KERNAL asks for RECORD & PLAY; press PLAY to satisfy the sense line.
    session
        .wait_for_query_text_contains("screen.text.lines", "PRESS RECORD", 600)
        .expect("SAVE should prompt for RECORD & PLAY");
    session
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Start,
        )))
        .expect("pressing PLAY should start the datasette");

    // The KERNAL writes the leader then the program. Give it time to lay down a
    // substantial pulse train, then flush the work image.
    session
        .wait_for_query_text_contains("screen.text.lines", "SAVING HI", 4000)
        .expect("SAVE should reach the SAVING banner");
    session
        .run_frames(6000)
        .expect("running frames to record the tape");

    let tap_bytes = session
        .machine()
        .flush_tape_image()
        .expect("writable tape should flush a .tap image");
    // A valid TAP header, and a payload big enough that the KERNAL clearly laid
    // down its leader + program (each pulse is at least one payload byte).
    assert_eq!(&tap_bytes[..12], b"C64-TAPE-RAW");
    let payload_len =
        u32::from_le_bytes([tap_bytes[16], tap_bytes[17], tap_bytes[18], tap_bytes[19]]) as usize;
    assert!(
        payload_len > 1000,
        "the KERNAL SAVE should record a substantial pulse train, got {payload_len} bytes"
    );
    assert_eq!(tap_bytes.len(), 20 + payload_len);
}
