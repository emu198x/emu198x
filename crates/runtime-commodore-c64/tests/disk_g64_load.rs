//! Real G64 raw-GCR load through a 1541.
//!
//! `#[ignore]`'d — needs local C64 + 1541 ROMs and a real G64 image from the
//! Time Capsule TOSEC library. Proves the raw-GCR read path end to end: the
//! drive reads a real disk's own GCR (custom sync, gaps, sectors) off the
//! mounted G64, finds the directory over the serial bus, and the KERNAL's
//! `LOAD"*",8,1` reaches its search/loading banner — where the D64 layer
//! cannot represent the disk at all.

mod common;

use std::path::PathBuf;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{HeadlessSession, MediaImage, MediaKind, MediaSet, read_media_asset};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_DISK_AUTOLOAD_SLOT, Model, autoload_basic_disk,
};

use common::local_rom_firmware_with_drive;

fn g64_path() -> PathBuf {
    PathBuf::from(
        "/Volumes/Data/Library/ROMs/TOSEC/Commodore/C64/Games/Arcade/[G64]/\
         Bomb Jack 2 (1986)(Elite).zip",
    )
}

#[test]
#[ignore = "requires local C64 + 1541 ROMs and a real G64 from the Time Capsule TOSEC"]
fn real_g64_reaches_the_load_search_over_the_bus() {
    let runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_drive())
            .expect("local ROMs should construct a C64 runtime with a 1541");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );

    let disk = read_media_asset(&g64_path(), MediaKind::Disk).expect("local G64 archive loads");
    let mut media = MediaSet::new();
    media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk.bytes));
    session
        .load_media(&media)
        .expect("G64 should mount into the 1541 on drive-8");
    assert!(
        session.machine().drive8().is_some(),
        "1541 on device 8 present"
    );

    // Boot, LOAD"*",8,1, and wait for SEARCHING FOR — the drive answered on the
    // serial bus and is reading its directory straight off the raw GCR.
    // `autoload_basic_disk` errors on timeout, so a clean return already means
    // the SEARCHING FOR banner appeared.
    autoload_basic_disk(&mut session, DEFAULT_DISK_AUTOLOAD_SLOT, 600, 4000)
        .expect("LOAD*,8,1 on a G64 should reach the SEARCHING FOR banner");

    // Then let it stream: the loader reaching LOADING proves the drive read the
    // first file's sectors out of the raw GCR, not just the directory track.
    session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 4000)
        .expect("the G64 load should reach LOADING off the raw GCR surface");
}
