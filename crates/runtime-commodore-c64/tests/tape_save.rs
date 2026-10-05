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
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    DEFAULT_TAPE_AUTOLOAD_SLOT, DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES, Model, autoload_basic_tape,
    type_string,
};

use common::{local_rom_firmware, screen_text_lines, wait_for_screen_line_contains};

#[test]
#[ignore = "FIXTURE: requires local C64 ROMs at ~/.emu198x/roms/commodore-c64/"]
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

/// Runs until a `READY.` prompt appears on a row below the one holding
/// `banner`, i.e. until the command that printed `banner` has finished.
fn wait_for_ready_after(
    session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    banner: &str,
    max_frames: u32,
) {
    for _ in 0..max_frames {
        let lines = screen_text_lines(session);
        if let Some(row) = lines.iter().position(|line| line.contains(banner))
            && lines[row + 1..].iter().any(|line| line.contains("READY."))
        {
            return;
        }
        session.run_frames(1).expect("running a frame");
    }
    panic!(
        "no READY. below {banner:?} within {max_frames} frames: {:?}",
        screen_text_lines(session)
    );
}

fn pal_session() -> HeadlessSession<C64Runtime, C64SessionQueryProvider> {
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware())
        .expect("local ROMs should construct a C64 runtime");
    HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    )
}

/// The KERNAL must be able to load back what it saved. Each KERNAL pulse is a
/// full wave that starts on a rising edge of the write line (`$FBB1` toggles
/// port bit 3 and the IRQ handler only moves on once the toggle leaves it low),
/// so the recorder has to measure rising edge to rising edge. Measuring falling
/// to falling shifts every pulse by half a wave and the loader never finds the
/// header (emu198x#1565).
#[test]
#[ignore = "FIXTURE: requires local C64 ROMs at ~/.emu198x/roms/commodore-c64/"]
fn kernal_loads_back_a_tape_it_saved() {
    let mut session = pal_session();
    wait_for_screen_line_contains(&mut session, 5, "READY.", 600);
    let mut media = MediaSet::new();
    media.push(MediaImage::new(DEFAULT_TAPE_AUTOLOAD_SLOT, MediaKind::Tape, &[]).writable(true));
    session
        .load_media(&media)
        .expect("blank tape should mount writable");

    // The program prints a word its own listing does not contain, so finding
    // it on screen proves the loaded program ran.
    type_string(&mut session, "10 PRINT \"ROUND\"+\"TRIP\"\n", 3, 10).expect("typing the program");
    type_string(&mut session, "SAVE\"RT\"\n", 3, 10).expect("typing the SAVE command");
    session
        .wait_for_query_text_contains("screen.text.lines", "PRESS RECORD", 600)
        .expect("SAVE should prompt for RECORD & PLAY");
    session
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            MediaTransportAction::Start,
        )))
        .expect("pressing PLAY should start the datasette");
    session
        .wait_for_query_text_contains("screen.text.lines", "SAVING RT", 4000)
        .expect("SAVE should reach the SAVING banner");
    wait_for_ready_after(&mut session, "SAVING RT", 6000);

    let tap_bytes = session
        .machine()
        .flush_tape_image()
        .expect("writable tape should flush a .tap image");

    // A fresh machine, the recorded tape, and the stock KERNAL's LOAD + RUN.
    let mut session = pal_session();
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        MediaKind::Tape,
        &tap_bytes,
    ));
    session
        .load_media(&media)
        .expect("recorded tape should mount");
    autoload_basic_tape(
        &mut session,
        DEFAULT_TAPE_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("LOAD should reach PRESS PLAY ON TAPE and start the transport");
    session
        .wait_for_query_text_contains("screen.text.lines", "FOUND RT", 3000)
        .expect("the KERNAL should find the header it saved");
    session
        .wait_for_query_text_contains("screen.text.lines", "ROUNDTRIP", 8000)
        .expect("the loaded program should run and print ROUNDTRIP");
}
