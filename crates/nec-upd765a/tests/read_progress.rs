//! Current transfer position and encountered errors, through public registers.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};
mod common;

fn sector(id: u8, deleted: bool, crc: bool) -> DiskSector {
    DiskSector {
        c: 0,
        h: 0,
        id,
        n: 2,
        st1: if crc { 0x20 } else { 0 },
        st2: (if deleted { 0x40 } else { 0 }) | if crc { 0x20 } else { 0 },
        data: vec![id; 512],
    }
}
fn controller(op: u8, sectors: Vec<DiskSector>, eot: u8) -> Upd765a {
    let mut f = Upd765a::new();
    f.enabled = true;
    f.insert_disk(
        0,
        DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack { sectors }]],
        },
    );
    for b in [op, 0, 0, 0, 1, 2, eot, 0x2a, 0xff] {
        f.write_data(b);
    }
    f
}
fn read(f: &mut Upd765a, n: usize) {
    for i in 0..n {
        common::await_read(f);
        assert_eq!(f.read_status(), 0xf0);
        assert_eq!(f.read_data(), (i / 512 + 1) as u8);
    }
}
fn overrun(f: &mut Upd765a) -> [u8; 7] {
    common::await_read(f);
    assert_eq!(f.read_status(), 0xf0);
    for _ in 0..114 {
        f.tick(0);
    }
    result(f)
}
fn result(f: &mut Upd765a) -> [u8; 7] {
    let mut r = [0; 7];
    for b in &mut r {
        assert_eq!(f.read_status(), 0xd0);
        *b = f.read_data();
    }
    assert_eq!(f.read_status(), 0x80);
    r
}
#[test]
fn interrupted_read_reports_current_sector_instead_of_eventual_eot_or_error() {
    for op in [0x46, 0x4c] {
        let deleted = op == 0x4c;
        for future in [
            None,
            Some(sector(2, deleted, false)),
            Some(sector(2, deleted, true)),
            Some(sector(2, !deleted, false)),
        ] {
            for consumed in [0, 1, 511] {
                let mut sectors = vec![sector(1, deleted, false)];
                sectors.extend(future.clone());
                let mut f = controller(op, sectors, 2);
                read(&mut f, consumed);
                assert_eq!(overrun(&mut f), [0x40, 0x10, 0, 0, 0, 1, 2]);
            }
        }
    }
}
#[test]
fn later_sector_interruption_reports_later_r_and_encountered_mark() {
    for op in [0x46, 0x4c] {
        for mismatch in [false, true] {
            let deleted = op == 0x4c;
            let mut f = controller(
                op,
                vec![
                    sector(1, deleted, false),
                    sector(2, deleted ^ mismatch, false),
                    sector(3, deleted, false),
                ],
                3,
            );
            read(&mut f, 513);
            assert_eq!(
                overrun(&mut f),
                [0x40, 0x10, if mismatch { 0x40 } else { 0 }, 0, 0, 2, 2]
            );
        }
    }
}
#[test]
fn track_overrun_keeps_completed_crc_errors_but_not_future_errors() {
    for first_crc in [false, true] {
        let mut f = controller(
            0x42,
            vec![sector(1, false, first_crc), sector(2, false, true)],
            2,
        );
        read(&mut f, 1);
        // CRC of the current incomplete field has not been encountered yet.
        assert_eq!(&overrun(&mut f)[..3], &[0x40, 0x10, 0]);
        let mut f = controller(
            0x42,
            vec![sector(1, false, first_crc), sector(2, false, true)],
            2,
        );
        read(&mut f, 513);
        assert_eq!(
            &overrun(&mut f)[..3],
            &[
                0x40,
                // Sector 2 ID differs from command R1: retain encountered ND.
                if first_crc { 0x34 } else { 0x14 },
                if first_crc { 0x20 } else { 0 }
            ]
        );
    }
}

#[test]
fn abandoned_read_does_not_count_an_unvisited_future_marginal_sector() {
    let mut f = controller(
        0x46,
        vec![sector(1, false, false), sector(2, false, true)],
        2,
    );
    let _ = overrun(&mut f);
    for b in [0x46, 0, 0, 0, 2, 2, 2, 0x2a, 0xff] {
        f.write_data(b);
    }
    for _ in 0..512 {
        common::await_read(&mut f);
        assert_eq!(
            f.read_data(),
            2,
            "first actual read of the marginal sector is verbatim"
        );
    }
    assert_eq!(result(&mut f), [0x40, 0x20, 0x20, 0, 0, 2, 2]);
}

#[test]
fn skipped_eot_after_data_retains_existing_next_r_result() {
    // Preserve this bounded model convention; broader SK termination remains
    // unqualified, independently of current-sector interruption tracking.
    for op in [0x66, 0x6c] {
        let deleted = op == 0x6c;
        let mut f = controller(
            op,
            vec![sector(1, deleted, false), sector(2, !deleted, false)],
            2,
        );
        read(&mut f, 512);
        assert_eq!(result(&mut f), [0, 0, 0, 0, 0, 3, 2]);
    }
}
