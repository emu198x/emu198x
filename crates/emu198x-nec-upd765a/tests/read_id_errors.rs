//! Register-level Read ID regressions. NEC µPD765 data sheet (December 1978),
//! printed p14 and pp17–18: missing IDs terminate abnormally with ST1.MA; no
//! readable ID/data also sets ST1.ND. Failed CHRN and physical search timing are
//! deliberately not asserted by this sector-image test.
use emu198x_nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

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
    let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
    if let Some(image) = image {
        fdc.insert_disk(0, image);
    }
    // A missing readable ID is distinct from a low physical READY input.
    fdc.set_ready(0, true);
    // A missing recorded side does not establish a single-sided mechanism.
    // Keep the physical two-side input explicit for the head-1 error probe.
    fdc.set_two_side(0, true);
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
    // READY is explicitly high: this checks missing-ID status, not the
    // timing or electrical sensors of a disconnected physical mechanism.
    assert_missing_id(None, 0);
}

#[test]
fn valid_read_id_preserves_recorded_chrn_rotation_and_plus3_drive_alias() {
    let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
    fdc.set_drive_select_mask(1);
    fdc.insert_disk(0, valid_image());
    assert_eq!(read_id(&mut fdc, 2), [2, 0, 0, 7, 3, 0xc1, 1]);
    assert_eq!(read_id(&mut fdc, 0), [0, 0, 0, 7, 3, 0xc7, 1]);
    assert_eq!(read_id(&mut fdc, 2), [2, 0, 0, 7, 3, 0xc1, 1]);
}

#[test]
fn read_id_reports_id_crc_and_keeps_data_errors_out_of_results() {
    for (head, drive) in [(0, 0), (1, 1)] {
        let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
        let sectors = [(0x20, 0), (0x20, 0x20), (0, 0), (0x25, 0x41)]
            .into_iter()
            .enumerate()
            .map(|(index, (st1, st2))| DiskSector {
                c: 0,
                h: head,
                id: 7 + index as u8,
                n: 2,
                st1,
                st2,
                data: vec![0xa5; 512],
            })
            .collect();
        let track = DiskTrack { sectors };
        let image = DiskImage {
            sides: 2,
            tracks_per_side: 1,
            tracks: vec![vec![track.clone()], vec![track]],
        };
        let original = image.clone();
        fdc.insert_disk(drive as usize, image);
        let unit = (head << 2) | drive;
        for (r, bad_id) in [(7, true), (8, false), (9, false), (10, true), (7, true)] {
            let expected = [
                unit | if bad_id { 0x40 } else { 0 },
                if bad_id { 0x24 } else { 0 },
                0,
                0,
                head,
                r,
                2,
            ];
            assert_eq!(read_id(&mut fdc, unit), expected, "R={r} unit={unit}");
            assert_eq!(fdc.read_status() & 0xf0, 0x80, "result drained to idle");
        }
        let observed = fdc.disk_for_export(drive as usize).expect("mounted disk");
        assert_eq!(observed.tracks[head as usize][0].sectors.len(), 4);
        for (before, after) in original
            .tracks
            .iter()
            .flatten()
            .flat_map(|t| &t.sectors)
            .zip(observed.tracks.iter().flatten().flat_map(|t| &t.sectors))
        {
            assert_eq!(
                (
                    before.c, before.h, before.id, before.n, before.st1, before.st2
                ),
                (after.c, after.h, after.id, after.n, after.st1, after.st2)
            );
            assert_eq!(before.data, after.data, "Read ID never mutates payload");
        }
    }
}
