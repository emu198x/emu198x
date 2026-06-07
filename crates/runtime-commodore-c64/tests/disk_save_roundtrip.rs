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

use common::{local_rom_firmware_with_drive, wait_for_screen_line_contains};

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
    //
    // KNOWN FAILURE (2026-06-07): this assertion does not yet pass, and the
    // cause is upstream of the write-back built here. The GCR write-back is
    // correct (write mode engages, track-18 sectors decode, flush round-trips),
    // but a real SAVE never writes the file: instrumentation shows the head
    // writes *only* track 18 (directory/BAM) and never a data track, the
    // program bytes never land in any drive RAM, and the C64 returns to READY
    // almost instantly instead of waiting ~1s for a disk write. So the drive
    // gets the filename/OPEN but the data-channel transfer is lost — the IEC
    // serial C64→drive data phase doesn't deliver while the drive is busy on
    // the OPEN. See `knowledge/decisions/disk-save-write-back.md`.
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
