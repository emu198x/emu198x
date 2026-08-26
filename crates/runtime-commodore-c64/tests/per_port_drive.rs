//! Per-port IEC drive-type selection for the C64 runtime.
//!
//! `#[ignore]`'d — needs the local C64 ROMs plus all three drive DOS ROMs
//! (`1541.rom`, `1571.rom`, `1581.rom`) under `~/.emu198x/roms/commodore-c64/`.
//!
//! The C64 IEC bus carries devices 8–11, and the user chooses the drive model
//! per port. These exercise the default layout, the live `set_port_drive`
//! selector, a chosen drive booting on a non-default port, and a custom layout
//! surviving a snapshot round-trip.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{HeadlessSession, MachineCore, MachineError};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, DriveKind, Model};

use common::{
    blank_firmware_with_drive, local_rom_firmware_with_all_drives, local_rom_firmware_with_drive,
};

fn all_drives_runtime() -> C64Runtime {
    C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_all_drives())
        .expect("local ROMs (incl. all three drives) should construct a C64 runtime")
}

#[test]
#[ignore = "FIXTURE: requires local C64 ROMs plus 1541, 1571, and 1581 DOS ROMs"]
fn default_layout_is_1541_on_8_and_1581_on_9() {
    let runtime = all_drives_runtime();

    assert_eq!(runtime.port_drive_kind(8), Some(DriveKind::C1541));
    assert_eq!(runtime.port_drive_kind(9), Some(DriveKind::C1581));
    assert_eq!(runtime.port_drive_kind(10), None);
    assert_eq!(runtime.port_drive_kind(11), None);

    assert!(runtime.drive8().is_some(), "1541 on device 8");
    assert!(runtime.drive_1581().is_some(), "1581 on device 9");
}

#[test]
#[ignore = "FIXTURE: requires local C64 ROMs plus 1541, 1571, and 1581 DOS ROMs"]
fn set_port_drive_swaps_and_clears_ports() {
    let mut runtime = all_drives_runtime();

    // Clear device 8, then put a 1571 there instead. The 1541-shaped `drive8`
    // accessor sees no 1541 once the port is empty or holds another model.
    runtime.set_port_drive(8, None).expect("clear device 8");
    assert_eq!(runtime.port_drive_kind(8), None);
    assert!(runtime.drive8().is_none(), "device 8 is empty");

    runtime
        .set_port_drive(8, Some(DriveKind::C1571))
        .expect("put a 1571 on device 8");
    assert_eq!(runtime.port_drive_kind(8), Some(DriveKind::C1571));
    assert!(
        runtime.drive8().is_none(),
        "the 1541-shaped accessor ignores a 1571"
    );

    // Fill the two spare ports with the other models.
    runtime
        .set_port_drive(10, Some(DriveKind::C1541))
        .expect("put a 1541 on device 10");
    runtime
        .set_port_drive(11, Some(DriveKind::C1581))
        .expect("put a 1581 on device 11");
    assert_eq!(runtime.port_drive_kind(10), Some(DriveKind::C1541));
    assert_eq!(runtime.port_drive_kind(11), Some(DriveKind::C1581));

    // Out-of-range device numbers are rejected.
    assert!(matches!(
        runtime.set_port_drive(12, Some(DriveKind::C1541)),
        Err(MachineError::InvalidRequest { .. })
    ));
    assert_eq!(runtime.port_drive_kind(12), None);
}

#[test]
#[ignore = "FIXTURE: requires local C64 ROMs plus 1541, 1571, and 1581 DOS ROMs"]
fn selecting_a_model_without_its_rom_errors() {
    // This runtime has only the 1541 DOS ROM (device 8); the 1571/1581 ROMs
    // were never supplied, so selecting those models must fail cleanly.
    let mut runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_drive())
            .expect("1541-only firmware should construct a runtime");

    assert!(matches!(
        runtime.set_port_drive(9, Some(DriveKind::C1571)),
        Err(MachineError::MissingFirmware { ref id }) if id == "commodore-1571-dos-rom"
    ));
    assert!(matches!(
        runtime.set_port_drive(9, Some(DriveKind::C1581)),
        Err(MachineError::MissingFirmware { ref id }) if id == "commodore-1581-dos-rom"
    ));
    // The failed selections left the port empty.
    assert_eq!(runtime.port_drive_kind(9), None);
}

#[test]
#[ignore = "FIXTURE: requires local C64 ROMs plus 1541, 1571, and 1581 DOS ROMs"]
fn a_1581_chosen_on_a_non_default_port_boots_on_the_bus() {
    let runtime = all_drives_runtime();
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );

    // Move the 1581 off its default device 9 onto device 11, leaving the 1541
    // on device 8. `drive_1581` then finds the sole 1581 at its new port.
    session
        .machine_mut()
        .set_port_drive(9, None)
        .expect("clear the default 1581");
    session
        .machine_mut()
        .set_port_drive(11, Some(DriveKind::C1581))
        .expect("put a 1581 on device 11");

    let drive = session.machine().drive_1581().expect("1581 present on 11");
    assert_eq!(drive.device_number(), 11, "1581 jumpers to device 11");

    // Boot the machine; both drives run in parallel over the interleaved bus.
    session
        .run_frames(150)
        .expect("advance the machine and drives");

    let drive = session
        .machine()
        .drive_1581()
        .expect("1581 stays present after boot");
    assert!(
        drive.cpu().regs.pc >= 0x8000,
        "the device-11 1581 should be executing its DOS ROM, got ${:04X}",
        drive.cpu().regs.pc
    );
    // The 1541 on device 8 keeps running too.
    assert!(
        session.machine().drive8().is_some(),
        "the 1541 on device 8 coexists"
    );
}

#[test]
#[ignore = "FIXTURE: requires local C64 ROMs plus 1541, 1571, and 1581 DOS ROMs"]
fn snapshot_preserves_a_custom_port_layout() {
    let mut runtime = all_drives_runtime();
    // Custom layout: device 8 empty, 1581 on 9 (default), 1571 on 10.
    runtime.set_port_drive(8, None).expect("clear device 8");
    runtime
        .set_port_drive(10, Some(DriveKind::C1571))
        .expect("put a 1571 on device 10");

    let bytes = runtime.snapshot().expect("snapshot the custom layout");

    let mut restored = all_drives_runtime();
    restored.restore(&bytes).expect("restore the custom layout");

    assert_eq!(restored.port_drive_kind(8), None);
    assert_eq!(restored.port_drive_kind(9), Some(DriveKind::C1581));
    assert_eq!(restored.port_drive_kind(10), Some(DriveKind::C1571));
    assert_eq!(restored.port_drive_kind(11), None);
}

// Runs in CI without local ROMs: a stub-ROM 1541 firmware yields the default
// port map — a 1541 on device 8 and every other port empty.
#[test]
fn blank_drive_firmware_has_1541_on_8_and_no_other_ports() {
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware_with_drive())
        .expect("blank firmware with a 1541 should construct a runtime");
    assert_eq!(runtime.port_drive_kind(8), Some(DriveKind::C1541));
    assert_eq!(runtime.port_drive_kind(9), None);
    assert_eq!(runtime.port_drive_kind(10), None);
    assert_eq!(runtime.port_drive_kind(11), None);
}
