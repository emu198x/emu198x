//! Real SAVE round-trip for the C64 runtime via a writable 1541 disk.
//!
//! `#[ignore]`'d — requires local C64 + 1541 ROMs at
//! `~/.emu198x/roms/commodore-c64/`. Proves the full write path end to end: the
//! KERNAL's SAVE routine lays GCR onto the live 1541 surface, and the flush
//! decodes that surface back into a valid D64 the directory parser can read.
//! See `knowledge/decisions/disk-save-write-back.md`.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{HeadlessSession, MediaImage, MediaKind, MediaSet};
use format_commodore_c64_d64::{extract_first_prg, parse_directory};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model, type_string};

use common::{local_rom_firmware_with_drive, screen_text_lines, wait_for_screen_line_contains};

/// A freshly `c1541`-formatted blank disk (real BAM with every block free) — a
/// learner's empty work disk. `make_d64`'s synthetic BAM marks no free blocks,
/// so the KERNAL can't SAVE onto it; this fixture can be written.
fn blank_formatted_d64() -> Vec<u8> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blank-formatted.d64");
    std::fs::read(&path).expect("blank formatted D64 fixture should be present")
}

#[test]
#[ignore = "requires local C64 + 1541 ROMs at ~/.emu198x/roms/commodore-c64/"]
fn save_writes_a_readable_file_to_a_writable_disk() {
    let runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_drive())
            .expect("local ROMs should construct a C64 runtime with a drive");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );

    // Boot to the READY. prompt.
    wait_for_screen_line_contains(&mut session, 5, "READY.", 600);

    // Mount a freshly-formatted blank disk, opted-in writable.
    let disk = blank_formatted_d64();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk).writable(true));
    session
        .load_media(&media)
        .expect("blank disk should mount writable");

    // Enter a one-line program and SAVE it to the disk.
    type_string(&mut session, "10 PRINT \"HI\"\n", 3, 10).expect("typing the program");
    type_string(&mut session, "SAVE\"GREETING\",8\n", 3, 10).expect("typing the SAVE command");

    // Wait for the KERNAL to report the SAVE, then let it finish.
    session
        .wait_for_query_text_contains("screen.text.lines", "SAVING GREETING", 2000)
        .expect("SAVE should reach the SAVING banner");
    session
        .run_frames(600)
        .expect("running frames to complete the SAVE");

    // Decode the live GCR surface back to a D64 and confirm the file is there.
    // This round-trips green as of 2026-06-08 — two fixes landed it: the 1541
    // write serialiser now latches the GCR byte (the directory sector survives
    // the drive's read-after-write verify), and an empty drive reads the
    // write-protect line as "not protected" so mounting a writable disk after
    // boot is not a phantom disk-change that force-closes the SAVE channel.
    // See `knowledge/decisions/disk-save-write-back.md`.
    let saved = session
        .machine()
        .flush_drive8_image()
        .expect("a writable disk should flush to a D64 image");

    let directory = parse_directory(&saved).expect("the flushed image should parse");
    assert!(
        directory
            .entries
            .iter()
            .any(|entry| entry.name == "GREETING"),
        "SAVE should have written a GREETING directory entry; saw {:?}",
        directory
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>()
    );

    let program = extract_first_prg(&saved).expect("the saved PRG should decode from disk");
    assert_eq!(
        &program.data[..2],
        &[0x01, 0x08],
        "the saved program should start at the BASIC load address $0801"
    );
}

/// The full learner round-trip: SAVE a program, clear memory, LOAD it back from
/// the disk, and RUN it — proving the saved file is not just structurally valid
/// but functionally restorable. This exercises the drive's *read* path against
/// GCR the drive itself just wrote (through the real ROM), the strongest check
/// that the write path is faithful — a lenient flush decode is not enough.
#[test]
#[ignore = "requires local C64 + 1541 ROMs at ~/.emu198x/roms/commodore-c64/"]
fn saved_program_loads_back_and_runs() {
    let runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_drive())
            .expect("local ROMs should construct a C64 runtime with a drive");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );

    wait_for_screen_line_contains(&mut session, 5, "READY.", 600);

    let disk = blank_formatted_d64();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk).writable(true));
    session
        .load_media(&media)
        .expect("blank disk should mount writable");

    // Enter and SAVE a program whose RUN output ("HI") is distinct from its own
    // listing line (`10 PRINT "HI"`), so a bare line of `HI` can only be the
    // program executing — not the source text still on screen.
    type_string(&mut session, "10 PRINT \"HI\"\n", 3, 10).expect("typing the program");
    type_string(&mut session, "SAVE\"GREETING\",8\n", 3, 10).expect("typing the SAVE command");
    session
        .wait_for_query_text_contains("screen.text.lines", "SAVING GREETING", 2000)
        .expect("SAVE should reach the SAVING banner");
    session
        .run_frames(600)
        .expect("running frames to complete the SAVE");

    // Wipe the program from memory so a successful RUN can only come from disk.
    type_string(&mut session, "NEW\n", 3, 10).expect("typing NEW");

    // LOAD it back from the disk we just wrote.
    type_string(&mut session, "LOAD\"GREETING\",8\n", 3, 10).expect("typing the LOAD command");
    session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 2000)
        .expect("LOAD should find GREETING on the disk and reach LOADING");
    session
        .run_frames(600)
        .expect("running frames to complete the LOAD");

    // RUN the restored program and confirm its output appears on its own line.
    type_string(&mut session, "RUN\n", 3, 10).expect("typing RUN");
    session
        .run_frames(120)
        .expect("running frames to execute the program");

    let lines = screen_text_lines(&session);
    assert!(
        lines.iter().any(|line| line.trim_end() == "HI"),
        "RUN of the loaded program should print HI on its own line; screen:\n{}",
        lines.join("\n")
    );
}
