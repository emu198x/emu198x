//! Register-level Read ID regressions. NEC µPD765 data sheet (December 1978),
//! printed p14 and pp17–18: missing IDs terminate abnormally with ST1.MA; no
//! readable ID/data also sets ST1.ND. Failed CHRN and physical search timing are
//! deliberately not asserted by this sector-image test.
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn read_id(fdc: &mut Upd765a, select: u8) -> [u8; 7] {
    assert_eq!(fdc.read_status(), 0x80, "controller accepts a new command");
    fdc.write_data(0x4a); // MFM Read ID
    fdc.write_data(select);
    assert_eq!(fdc.read_status(), 0xd0, "seven-byte result is readable");
    let mut result = [0; 7];
    for byte in &mut result {
        assert_eq!(fdc.read_status(), 0xd0, "result must not end early");
        *byte = fdc.read_data();
    }
    assert_eq!(fdc.read_status(), 0x80, "result drained back to idle");
    result
}

fn assert_missing_id(image: Option<DiskImage>, select: u8) {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    if let Some(image) = image {
        fdc.insert_disk(0, image);
    }
    let result = read_id(&mut fdc, select);
    assert_eq!(
        result[0] & 0xc0,
        0x40,
        "ST0.IC must report abnormal termination"
    );
    assert_eq!(
        result[0] & 0x07,
        select & 0x07,
        "unit/head echo survives errors"
    );
    assert_eq!(result[1], 0x05, "ST1.MA and ST1.ND report no readable ID");
    assert_eq!(result[2], 0, "an absent ID is not a data-field error");

    // A failed probe must not poison the next command or leave unread results.
    fdc.insert_disk(0, valid_image());
    assert_eq!(read_id(&mut fdc, 0), [0, 0, 0, 7, 3, 0xc1, 1]);
}

fn valid_image() -> DiskImage {
    DiskImage {
        sides: 1,
        tracks_per_side: 1,
        tracks: vec![vec![DiskTrack {
            sectors: [0xc1, 0xc7]
                .into_iter()
                .map(|id| DiskSector {
                    c: 7,
                    h: 3,
                    id,
                    n: 1,
                    st1: 0,
                    st2: 0,
                    data: vec![0x55; 256],
                })
                .collect(),
        }]],
    }
}

#[test]
fn read_id_on_empty_track_reports_missing_id() {
    assert_missing_id(
        Some(DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack { sectors: vec![] }]],
        }),
        0,
    );
}

#[test]
fn read_id_on_absent_track_reports_missing_id() {
    assert_missing_id(
        Some(DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![]],
        }),
        0,
    );
}

#[test]
fn read_id_on_absent_head_reports_missing_id() {
    assert_missing_id(Some(valid_image()), 4);
}

#[test]
fn read_id_without_image_cannot_report_success() {
    // The core has no drive-ready input: this checks missing-ID status, not NR
    // fidelity or the timing of a disconnected drive's index pulses.
    assert_missing_id(None, 0);
}

#[test]
fn valid_read_id_preserves_recorded_chrn_rotation_and_plus3_drive_alias() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.set_drive_select_mask(1);
    fdc.insert_disk(0, valid_image());
    assert_eq!(read_id(&mut fdc, 2), [2, 0, 0, 7, 3, 0xc1, 1]);
    assert_eq!(read_id(&mut fdc, 0), [0, 0, 0, 7, 3, 0xc7, 1]);
    assert_eq!(read_id(&mut fdc, 2), [2, 0, 0, 7, 3, 0xc1, 1]);
}
