//! NEC December 1978 p13: Write Deleted Data differs only in its data mark.
//! Public-register tests, independent of the controller's private command tags.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, ST2_CM, Upd765a};

fn controller(writable: bool) -> Upd765a {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.insert_disk_with_writable(
        0,
        DiskImage {
            sides: 2,
            tracks_per_side: 1,
            tracks: (0..2)
                .map(|h| {
                    vec![DiskTrack {
                        sectors: (1..=2)
                            .map(|id| DiskSector {
                                c: 0,
                                h,
                                id,
                                n: 2,
                                st1: 0,
                                st2: 0,
                                data: vec![id; 512],
                            })
                            .collect(),
                    }]
                })
                .collect(),
        },
        writable,
    );
    fdc
}
fn command(f: &mut Upd765a, opcode: u8, r: u8, eot: u8) {
    assert_eq!(f.read_status(), 0x80);
    for b in [opcode, 0, 0, 0, r, 2, eot, 0x2a, 0xff] {
        f.write_data(b);
    }
}
fn ready(f: &mut Upd765a) {
    for _ in 0..5000 {
        if f.read_status() & 0x80 != 0 {
            return;
        }
        f.tick(0);
    }
    panic!("bounded write request");
}
fn transfer(f: &mut Upd765a, bytes: &[u8]) {
    for &b in bytes {
        ready(f);
        assert_eq!(f.read_status() & 0xf0, 0xb0);
        f.write_data(b);
        for _ in 0..114 {
            f.tick(0);
        }
    }
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
fn read(f: &mut Upd765a) -> Vec<u8> {
    let mut data = Vec::new();
    while f.read_status() == 0xf0 {
        assert!(data.len() < 2048);
        data.push(f.read_data());
    }
    data
}
#[test]
fn deleted_write_read_mark_and_skip_matrix() {
    for (opcode, expected_data, expected_status) in [
        (0x46, vec![0xa5; 512], [0x40, 0, 0x40, 0, 0, 1, 2]),
        (0x4c, vec![0xa5; 512], [0x40, 0x80, 0, 0, 0, 1, 2]),
        (0x66, vec![2; 512], [0x40, 0x80, 0, 0, 0, 2, 2]),
        (0x6c, vec![0xa5; 512], [0x40, 0x80, 0, 0, 0, 1, 2]),
    ] {
        let mut f = controller(true);
        command(&mut f, 0x49, 1, 1);
        transfer(&mut f, &[0xa5; 512]);
        assert_eq!(result(&mut f), [0x40, 0x80, 0, 1, 0, 1, 2]);
        let image = f.disk_for_export(0).expect("completed image");
        assert_eq!(image.tracks[0][0].sectors[0].st2, ST2_CM);
        assert_eq!(image.tracks[0][0].sectors[1].data, [2; 512]);
        command(&mut f, opcode, 1, if opcode == 0x66 { 2 } else { 1 });
        assert_eq!(read(&mut f), expected_data);
        assert_eq!(result(&mut f), expected_status);
    }
}
#[test]
fn normal_write_replaces_deleted_mark() {
    let mut f = controller(true);
    for (opcode, mark, value) in [(0x49, ST2_CM, 0xa5), (0x45, 0, 0x5a)] {
        command(&mut f, opcode, 1, 1);
        transfer(&mut f, &[value; 512]);
        result(&mut f);
        let s = &f.disk_for_export(0).expect("disk").tracks[0][0].sectors[0];
        assert_eq!((s.st1, s.st2), (0, mark));
        assert_eq!(s.data, [value; 512]);
    }
}
#[test]
fn deleted_partial_overrun_preserves_mark_prefix_and_crc_error() {
    let mut f = controller(true);
    command(&mut f, 0x49, 1, 1);
    transfer(&mut f, &[0xa5; 16]);
    ready(&mut f);
    f.write_data(0xa5);
    assert!(f.disk_for_export(0).is_err());
    // RQM low writes and wrong-direction reads must not transfer data.
    f.write_data(0x99);
    assert_eq!(f.read_data(), 0xff);
    for _ in 0..228 {
        f.tick(0);
    }
    assert_eq!(result(&mut f), [0x40, 0x10, 0, 0, 0, 1, 2]);
    let s = &f.disk_for_export(0).expect("interrupted image").tracks[0][0].sectors[0];
    assert_eq!(&s.data[..17], &[0xa5; 17]);
    assert!(s.data[17..].iter().all(|b| *b == 1));
    assert_eq!((s.st1, s.st2), (0x20, 0x20 | ST2_CM));
    f.reset();
    assert_eq!(
        f.disk_for_export(0).expect("retained").tracks[0][0].sectors[0].st2,
        0x60
    );
}
#[test]
fn deleted_write_protection_and_missing_media_do_not_mutate() {
    let mut f = controller(false);
    command(&mut f, 0x49, 1, 1);
    assert_eq!(result(&mut f), [0x40, 2, 0, 0, 0, 1, 2]);
    assert_eq!(
        f.disk_for_export(0).expect("unchanged").tracks[0][0].sectors[0].data,
        [1; 512]
    );
    f.eject_disk(0);
    command(&mut f, 0x49, 1, 1);
    assert_eq!(result(&mut f), [0x48, 0, 0, 0, 0, 1, 2]);
}
#[test]
fn deleted_multitrack_write_preserves_marks_on_both_heads() {
    let mut f = controller(true);
    command(&mut f, 0xc9, 1, 2);
    for value in [0xa1, 0xa2, 0xb1, 0xb2] {
        transfer(&mut f, &[value; 512]);
    }
    assert_eq!(result(&mut f), [0x40, 0x80, 0, 1, 0, 1, 2]);
    let disk = f.disk_for_export(0).expect("complete");
    for (head, values) in [(0, [0xa1, 0xa2]), (1, [0xb1, 0xb2])] {
        for (s, value) in disk.tracks[head][0].sectors.iter().zip(values) {
            assert_eq!(s.data, [value; 512]);
            assert_eq!((s.st1, s.st2), (0, ST2_CM));
        }
    }
}
