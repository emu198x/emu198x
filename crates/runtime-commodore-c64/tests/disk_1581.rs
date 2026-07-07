//! Real 1581 integration for the C64 runtime.
//!
//! `#[ignore]`'d — needs local C64 ROMs, the 1581 DOS ROM at
//! `~/.emu198x/roms/commodore-c64/1581.rom`, and a D81 game archive under the
//! catalogue media root. The 1581 sits on IEC device 9 alongside the 1541 on
//! device 8, so these exercise two drives coexisting on one bus.
//!
//! Coverage today: the 1581 attaches, coexists with a 1541, mounts a D81, and
//! boots the real DOS ROM to its serial idle loop over the interleaved bus.
//! The C64→1581 serial LOAD handshake (the ATN acknowledge) works — it was
//! fixed by the 1581's `data | cpu_bus` DATA fold in `common-commodore-iec`
//! (`write_drive_port_b_1581`, distinct from the 1541's `~data ^ cpu_bus`) —
//! and is asserted end to end by the `empire-1581-load` catalogue entry, which
//! drives `LOAD"*",9,1` to LOADING over the bus. Background:
//! `docs/plans/2026-07-03-1581-drive-build-spec.md`.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{HeadlessSession, MediaImage, MediaKind, MediaSet, read_media_asset};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model};

use common::{local_batman_d81_zip, local_rom_firmware_with_both_drives};

#[test]
#[ignore = "requires local C64 ROMs, 1581 DOS ROM, and a D81 game archive"]
fn real_1581_coexists_with_1541_and_boots_to_idle() {
    let firmware = local_rom_firmware_with_both_drives();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs (incl. 1541 + 1581) should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );

    let disk = read_media_asset(&local_batman_d81_zip(), MediaKind::Disk)
        .expect("local Batman D81 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new("drive-9", MediaKind::Disk, &disk.bytes));
    session
        .load_media(&media)
        .expect("Batman D81 should mount into drive-9");

    // Both drives are on the bus: the 1541 on device 8, the 1581 on device 9.
    assert!(
        session.machine().drive8().is_some(),
        "1541 should be present"
    );
    let drive = session
        .machine()
        .drive_1581()
        .expect("1581 should be present");
    assert_eq!(drive.device_number(), 9, "1581 should jumper to device 9");
    assert!(drive.disk_inserted(), "D81 should be mounted");

    // Boot the machine; the 1581 boots in parallel over the interleaved bus.
    // Confirm it is actually executing its DOS ROM (running in the $8000-$FFFF
    // ROM window), not hung at reset.
    session
        .run_frames(150)
        .expect("advance the machine and drives");
    let drive = session
        .machine()
        .drive_1581()
        .expect("1581 stays present after boot");
    assert!(
        drive.cpu().regs.pc >= 0x8000,
        "the 1581 CPU should be executing its DOS ROM, got ${:04X}",
        drive.cpu().regs.pc
    );
}
