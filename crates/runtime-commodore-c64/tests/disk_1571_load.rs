//! Real SAVE→LOAD→RUN round-trip through a 1571 on IEC device 8.
//!
//! `#[ignore]`'d — requires local C64 ROMs plus the 1541, 1571, and 1581 DOS
//! ROMs at `~/.emu198x/roms/commodore-c64/`.
//!
//! The 1571 in C64 mode is a 1541-compatible serial drive. This puts a 1571 on
//! device 8 (the per-port selector), clears the default 1581 off device 9, and
//! drives the same self-contained round-trip the 1541 passes: SAVE a program,
//! `NEW` to wipe memory, LOAD it back, and RUN it. Because the LOAD reads GCR
//! the drive itself just wrote through the real 1571 ROM, a green run proves the
//! 1571's serial read *and* write paths end to end — the last gap between the
//! wired 1571 core and a working drive.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{HeadlessSession, MediaImage, MediaKind, MediaSet};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, DriveKind, Model, type_string};

use common::{
    local_rom_firmware_with_all_drives, screen_text_lines, wait_for_screen_line_contains,
};

/// A freshly `c1541`-formatted blank disk (real BAM, every block free). The
/// 1571 mounts it in 1541-compatible mode (single-sided, side 0).
fn blank_formatted_d64() -> Vec<u8> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blank-formatted.d64");
    std::fs::read(&path).expect("blank formatted D64 fixture should be present")
}

#[test]
#[ignore = "FIXTURE: requires local C64 ROMs plus 1541, 1571, and 1581 DOS ROMs"]
fn save_then_load_and_run_on_a_1571_device_8() {
    let mut runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_all_drives())
            .expect("local ROMs (incl. all three drives) should construct a runtime");
    // Put a 1571 on device 8 and clear the default 1581 off device 9, so the
    // 1571 is the sole drive on the bus — the exact shape of the 1541 test.
    runtime
        .set_port_drive(8, Some(DriveKind::C1571))
        .expect("put a 1571 on device 8");
    runtime
        .set_port_drive(9, None)
        .expect("clear the default 1581");

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );

    wait_for_screen_line_contains(&mut session, 5, "READY.", 600);

    // Mount a freshly-formatted blank disk, opted-in writable.
    let disk = blank_formatted_d64();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk).writable(true));
    session
        .load_media(&media)
        .expect("blank disk should mount writable into the 1571");

    // Enter and SAVE a program whose RUN output ("HI") is distinct from its own
    // listing line, so a bare `HI` can only be the program executing.
    type_string(&mut session, "10 PRINT \"HI\"\n", 3, 10).expect("typing the program");
    type_string(&mut session, "SAVE\"GREETING\",8\n", 3, 10).expect("typing the SAVE command");
    session
        .wait_for_query_text_contains("screen.text.lines", "SAVING GREETING", 2000)
        .expect("SAVE should reach the SAVING banner on the 1571");
    session
        .run_frames(600)
        .expect("running frames to complete the SAVE");

    // Wipe the program so a successful RUN can only come from disk.
    type_string(&mut session, "NEW\n", 3, 10).expect("typing NEW");

    // LOAD it back from the disk the 1571 just wrote.
    type_string(&mut session, "LOAD\"GREETING\",8\n", 3, 10).expect("typing the LOAD command");
    session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 2000)
        .expect("LOAD should find GREETING on the 1571 and reach LOADING");
    session
        .run_frames(600)
        .expect("running frames to complete the LOAD");

    // RUN the restored program and confirm its output on its own line.
    type_string(&mut session, "RUN\n", 3, 10).expect("typing RUN");
    session
        .run_frames(120)
        .expect("running frames to execute the program");

    let lines = screen_text_lines(&session);
    assert!(
        lines.iter().any(|line| line.trim_end() == "HI"),
        "RUN of the program loaded from the 1571 should print HI; screen:\n{}",
        lines.join("\n")
    );
}
