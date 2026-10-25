//! NEC µPD765 December 1978, printed pp11–12 and 17–18: command C is
//! compared with the recorded ID, not physical position. ND/WC report a
//! mismatching cylinder; BC additionally reports a mismatching recorded FF.
//! These sector-image tests do not establish failed CHRN or search timing.
mod common;
use emu198x_nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn sector(c: u8, id: u8, deleted: bool) -> DiskSector {
    DiskSector {
        c,
        h: 0,
        id,
        n: 0,
        st1: 0,
        st2: if deleted { 0x40 } else { 0 },
        data: vec![c.wrapping_add(id); 128],
    }
}

fn controller(sectors: Vec<DiskSector>) -> Upd765a {
    let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
    fdc.insert_disk(
        0,
        DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack { sectors }]],
        },
    );
    fdc
}

fn read(fdc: &mut Upd765a, deleted: bool, c: u8, r: u8, eot: u8) -> (Vec<u8>, [u8; 7]) {
    assert_eq!(fdc.read_status(), 0x80);
    for byte in [
        if deleted { 0x4c } else { 0x46 },
        0,
        c,
        0,
        r,
        0,
        eot,
        0x2a,
        128,
    ] {
        fdc.write_data(byte);
    }
    let mut data = Vec::new();
    while fdc.read_status() & 0x20 != 0 {
        common::await_read(fdc);
        assert!(data.len() < 1024, "bounded execution-byte drain");
        data.push(fdc.read_data());
    }
    assert_eq!(
        fdc.read_status(),
        0xd0,
        "result, without delivering error-sector data"
    );
    let mut result = [0; 7];
    for byte in &mut result {
        assert_eq!(fdc.read_status(), 0xd0, "complete seven-byte result");
        *byte = fdc.read_data();
    }
    assert_eq!(fdc.read_status(), 0x80, "result drains to idle");
    (data, result)
}

fn assert_wrong_cylinder(deleted: bool, recorded: u8) {
    let mut fdc = controller(vec![sector(recorded, 1, deleted)]);
    let (data, result) = read(&mut fdc, deleted, 0, 1, 1);
    assert!(
        data.is_empty(),
        "wrong-cylinder payload must not reach the host"
    );
    assert_eq!(
        &result[..3],
        &[0x40, 0x04, if recorded == 0xff { 0x12 } else { 0x10 }]
    );
    let (data, result) = read(&mut fdc, deleted, recorded, 1, 1);
    assert_eq!(data, vec![recorded.wrapping_add(1); 128]);
    assert_eq!(
        &result[..3],
        &[0x40, 0x80, 0],
        "subsequent matching read recovers and reaches EOT without TC"
    );
}

#[test]
fn read_data_rejects_wrong_cylinder() {
    assert_wrong_cylinder(false, 7);
}

#[test]
fn read_data_rejects_bad_cylinder() {
    assert_wrong_cylinder(false, 0xff);
}

#[test]
fn read_deleted_data_rejects_wrong_cylinder() {
    assert_wrong_cylinder(true, 7);
}

#[test]
fn read_deleted_data_rejects_bad_cylinder() {
    assert_wrong_cylinder(true, 0xff);
}

#[test]
fn matching_nonphysical_and_ff_headers_are_readable() {
    for deleted in [false, true] {
        for recorded in [7, 0xff] {
            let mut fdc = controller(vec![sector(recorded, 1, deleted)]);
            let (data, result) = read(&mut fdc, deleted, recorded, 1, 1);
            assert_eq!(data, vec![recorded.wrapping_add(1); 128]);
            assert_eq!(&result[..3], &[0x40, 0x80, 0]);
            assert_eq!(
                result[3], recorded,
                "result C is not physical cylinder zero"
            );
        }
    }
}

#[test]
fn later_matching_duplicate_sector_id_is_not_hidden_by_wrong_cylinder() {
    for deleted in [false, true] {
        let mut fdc = controller(vec![sector(7, 1, deleted), sector(0, 1, deleted)]);
        let (data, result) = read(&mut fdc, deleted, 0, 1, 1);
        assert_eq!(data, vec![1; 128]);
        assert_eq!(&result[..3], &[0x40, 0x80, 0]);
    }
}

#[test]
fn absent_requested_r_still_reports_encountered_wrong_cylinder() {
    for deleted in [false, true] {
        let mut fdc = controller(vec![sector(7, 3, deleted)]);
        let (data, result) = read(&mut fdc, deleted, 0, 1, 1);
        assert!(data.is_empty());
        assert_eq!(&result[..3], &[0x40, 4, 0x10]);
    }
}

#[test]
fn later_wrong_cylinder_preserves_only_preceding_sector_data() {
    for deleted in [false, true] {
        let mut fdc = controller(vec![sector(0, 1, deleted), sector(7, 2, deleted)]);
        let (data, result) = read(&mut fdc, deleted, 0, 1, 2);
        assert_eq!(data, vec![1; 128]);
        assert_eq!(&result[..3], &[0x40, 4, 0x10]);
        assert_eq!(result[5], 2, "R identifies the read that failed");
    }
}

#[test]
fn absent_requested_r_on_matching_cylinder_has_no_wrong_cylinder_status() {
    for deleted in [false, true] {
        let mut fdc = controller(vec![sector(7, 3, deleted)]);
        let (data, result) = read(&mut fdc, deleted, 7, 1, 1);
        assert!(data.is_empty());
        assert_eq!(&result[..3], &[0x40, 4, 0]);
    }
}
