//! Phase 1 characterisation — drive mechanical + status behaviour.
//!
//! Covers task #167. Exercises the archive's `AmigaFloppyDrive`
//! through its public API, using only behaviours the live machine
//! will rely on post-port:
//!   - head stepping + clamping
//!   - motor spin-up timing
//!   - index pulse once per revolution while selected + spinning
//!   - DSKCHANGE latching + clear-on-step
//!   - head-side select
//!   - drive-ID shift register on DSKSEL falling edge with motor off
//!
//! Paired with `mfm_adf.rs` (#168) which covers encode/decode and
//! write-capture round-trip. Both are lifted into the live crate
//! verbatim when Phase 2 / Phase 3 land.

use format_commodore_amiga_adf::{Adf, ADF_SIZE_DD};
use peripheral_commodore_amiga_floppy::AmigaFloppyDrive;

/// Matches the archive's private `MOTOR_SPINUP_TICKS` constant.
const MOTOR_SPINUP_TICKS: u32 = 350_000;
/// Matches the archive's private `INDEX_PULSE_TICKS` constant.
const INDEX_PULSE_TICKS: u32 = 141_876;

/// Select + motor-on, deasserted step, direction outward, side lower.
fn select_motor_on(drive: &mut AmigaFloppyDrive) {
    drive.update_control(false, false, false, true, true);
}

fn with_blank_disk() -> AmigaFloppyDrive {
    let mut drive = AmigaFloppyDrive::new();
    let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid blank DD ADF");
    drive.insert_disk(adf);
    drive.acknowledge_disk_change();
    drive
}

#[test]
fn head_steps_inward_up_to_cylinder_79() {
    let mut drive = AmigaFloppyDrive::new();
    for _ in 0..80 {
        // Step falling edge: prev deasserted (false/high), now
        // asserted (true/low). update_control takes (step, dir_in,
        // side, sel, motor); dir_in=true steps inward.
        drive.update_control(false, true, false, true, true);
        drive.update_control(true, true, false, true, true);
    }
    assert_eq!(drive.cylinder(), 79, "clamp at outer 80-cylinder limit");
}

#[test]
fn head_does_not_step_below_zero() {
    let mut drive = AmigaFloppyDrive::new();
    // dir=outward step pulse: falling edge should NOT decrement
    // below 0.
    drive.update_control(false, false, false, true, true);
    drive.update_control(true, false, false, true, true);
    assert_eq!(drive.cylinder(), 0);
    assert!(drive.status().track0, "TK0 stays asserted at cylinder 0");
}

#[test]
fn head_side_follows_side_select_signal() {
    let mut drive = AmigaFloppyDrive::new();
    // side_upper=true -> head 1 (upper).
    drive.update_control(false, false, true, true, true);
    assert_eq!(drive.head(), 1);
    drive.update_control(false, false, false, true, true);
    assert_eq!(drive.head(), 0);
}

#[test]
fn motor_spin_up_takes_full_spin_up_interval() {
    let mut drive = with_blank_disk();
    select_motor_on(&mut drive);
    assert!(!drive.status().ready, "not spinning at motor-on");
    for _ in 0..MOTOR_SPINUP_TICKS - 1 {
        drive.tick();
        assert!(!drive.status().ready, "still spinning up");
    }
    drive.tick();
    assert!(drive.status().ready, "spun up after MOTOR_SPINUP_TICKS");
}

#[test]
fn spinning_selected_drive_emits_one_index_pulse_per_revolution() {
    let mut drive = with_blank_disk();
    select_motor_on(&mut drive);
    for _ in 0..MOTOR_SPINUP_TICKS {
        let _ = drive.tick();
    }
    // After spin-up, exactly one index pulse per INDEX_PULSE_TICKS.
    for _ in 0..INDEX_PULSE_TICKS - 1 {
        assert!(!drive.tick());
    }
    assert!(drive.tick(), "index pulse at end of revolution");
}

#[test]
fn deselecting_drive_suppresses_index_pulses() {
    let mut drive = with_blank_disk();
    select_motor_on(&mut drive);
    for _ in 0..MOTOR_SPINUP_TICKS {
        let _ = drive.tick();
    }
    // Deselect (sel=false) with motor still on.
    drive.update_control(false, false, false, false, true);
    for _ in 0..INDEX_PULSE_TICKS {
        assert!(!drive.tick(), "no index pulses while deselected");
    }
}

#[test]
fn dskchange_latched_on_insert_and_cleared_by_step() {
    let mut drive = AmigaFloppyDrive::new();
    let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid ADF");
    drive.insert_disk(adf);
    // Straight after insert, CHNG is active (latched low).
    assert!(drive.status().disk_change);

    // First step pulse (requires select + falling edge) clears CHNG.
    drive.update_control(false, true, false, true, true);
    drive.update_control(true, true, false, true, true);
    assert!(!drive.status().disk_change);
}

#[test]
fn eject_reasserts_dskchange_even_after_acknowledgement() {
    let mut drive = with_blank_disk();
    assert!(!drive.status().disk_change, "acknowledged after insert");
    drive.eject_disk();
    assert!(drive.status().disk_change, "eject reasserts CHNG");
}

#[test]
fn drive_id_stream_shifts_on_dsksel_falling_edge_with_motor_off() {
    // HRM §Device I.D.: 3.5" drive ID = $FFFFFFFF. Each DSKSEL falling
    // edge shifts one MSB-first bit out on /DSKRDY while motor is OFF.
    // All-ones stream means /DSKRDY stays deasserted for every shift.
    let mut drive = AmigaFloppyDrive::new();
    for _ in 0..32 {
        // Falling edge: deselect → select, motor=false.
        drive.update_control(false, false, false, true, false);
        assert!(
            !drive.status().ready,
            "all-ones ID bit leaves /DSKRDY deasserted"
        );
        drive.update_control(false, false, false, false, false);
    }
}

#[test]
fn selecting_with_motor_on_bypasses_id_stream() {
    let mut drive = with_blank_disk();
    select_motor_on(&mut drive);
    assert!(!drive.status().ready, "motor not yet at speed");
    for _ in 0..MOTOR_SPINUP_TICKS {
        drive.tick();
    }
    assert!(drive.status().ready, "ready reports spindle speed, not ID stream");
}
