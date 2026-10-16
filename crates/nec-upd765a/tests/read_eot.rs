//! No-TC EOT is abnormal completion with EN, retaining the final read CHRN.
//! NEC December 1978 p13 Table 2 covers processor/TC termination instead.
//! Fuse start_read_data and MAME SECTOR_READ distinguish these cases.
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn sector(c: u8, r: u8, deleted: bool) -> DiskSector {
    DiskSector {
        c,
        h: 0,
        id: r,
        n: 2,
        st1: 0,
        st2: if deleted { 0x40 } else { 0 },
        data: (0..512).map(|i| (i as u8).wrapping_add(r)).collect(),
    }
}

fn controller(sectors: Vec<DiskSector>) -> Upd765a {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
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

fn command(fdc: &mut Upd765a, opcode: u8, c: u8, r: u8, eot: u8) {
    assert_eq!(fdc.read_status(), 0x80, "idle before command");
    for byte in [opcode, 0, c, 0, r, 2, eot, 0x2a, 0xff] {
        fdc.write_data(byte);
    }
}

fn result(fdc: &mut Upd765a) -> [u8; 7] {
    let mut result = [0; 7];
    for byte in &mut result {
        assert_eq!(fdc.read_status(), 0xd0, "all seven result bytes");
        *byte = fdc.read_data();
    }
    assert_eq!(fdc.read_status(), 0x80, "result drains to idle");
    result
}

fn data(fdc: &mut Upd765a, expected: &[u8]) {
    for byte in expected {
        assert_eq!(fdc.read_status(), 0xf0, "execution until final byte");
        assert_eq!(fdc.read_data(), *byte);
    }
    assert_eq!(
        fdc.read_status(),
        0xd0,
        "result immediately after final byte"
    );
}

fn clean_eot(deleted: bool) {
    let opcode = if deleted { 0x4c } else { 0x46 };
    // Physical track remains zero even for nonstandard recorded C.
    for c in [0, 7, 0xff] {
        let sec = sector(c, 1, deleted);
        let expected = sec.data.clone();
        let mut fdc = controller(vec![sec]);
        for _ in 0..2 {
            command(&mut fdc, opcode, c, 1, 1);
            data(&mut fdc, &expected);
            assert_eq!(result(&mut fdc), [0x40, 0x80, 0, c, 0, 1, 2]);
        }
    }
}

#[test]
fn read_data_reaches_eot_without_terminal_count() {
    clean_eot(false);
}

#[test]
fn read_deleted_data_reaches_eot_without_terminal_count() {
    clean_eot(true);
}

#[test]
fn multisector_read_retains_final_sector_at_eot() {
    for deleted in [false, true] {
        let sectors = vec![sector(0, 1, deleted), sector(0, 2, deleted)];
        let expected: Vec<u8> = sectors
            .iter()
            .flat_map(|s| s.data.iter().copied())
            .collect();
        let mut fdc = controller(sectors);
        command(&mut fdc, if deleted { 0x4c } else { 0x46 }, 0, 1, 2);
        data(&mut fdc, &expected);
        assert_eq!(result(&mut fdc), [0x40, 0x80, 0, 0, 0, 2, 2]);
    }
}

#[test]
fn earlier_missing_or_wrong_cylinder_has_no_eot_status() {
    for deleted in [false, true] {
        for wrong in [false, true] {
            let mut sectors = vec![sector(0, 1, deleted)];
            let expected = sectors[0].data.clone();
            if wrong {
                sectors.push(sector(7, 2, deleted));
            }
            let mut fdc = controller(sectors);
            command(&mut fdc, if deleted { 0x4c } else { 0x46 }, 0, 1, 2);
            data(&mut fdc, &expected);
            assert_eq!(
                result(&mut fdc),
                [0x40, 4, if wrong { 0x10 } else { 0 }, 0, 0, 2, 2]
            );
        }
    }
}

#[test]
fn crc_and_control_mark_at_eot_do_not_gain_en() {
    for deleted in [false, true] {
        for crc in [false, true] {
            let mut sec = sector(0, 1, if crc { deleted } else { !deleted });
            if crc {
                sec.st1 = 0x20;
                sec.st2 |= 0x20;
            }
            let expected = sec.data.clone();
            let mut fdc = controller(vec![sec]);
            command(&mut fdc, if deleted { 0x4c } else { 0x46 }, 0, 1, 1);
            data(&mut fdc, &expected);
            assert_eq!(
                result(&mut fdc),
                [
                    0x40,
                    if crc { 0x20 } else { 0 },
                    if crc { 0x20 } else { 0x40 },
                    0,
                    0,
                    1,
                    2
                ]
            );
        }
    }
}

#[test]
fn all_skipped_sectors_do_not_become_clean_eot() {
    for deleted in [false, true] {
        let mut fdc = controller(vec![sector(0, 1, !deleted)]);
        command(&mut fdc, if deleted { 0x6c } else { 0x66 }, 0, 1, 1);
        assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 1, 2]);
    }
}
