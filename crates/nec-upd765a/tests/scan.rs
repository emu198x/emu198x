//! NEC December 1978 p15, Table4: compare every disk byte against host bytes.
//! Stock MAME 0.289 has documented Scan disagreements, not a passing oracle.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};
fn controller(values: &[u8], deleted: bool) -> Upd765a {
    let mut f = Upd765a::new();
    f.enabled = true;
    f.insert_disk(
        0,
        DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack {
                sectors: values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| DiskSector {
                        c: 0,
                        h: 0,
                        id: i as u8 + 1,
                        n: 2,
                        st1: 0,
                        st2: if deleted { 0x40 } else { 0 },
                        data: vec![*v; 512],
                    })
                    .collect(),
            }]],
        },
    );
    f
}
fn command(f: &mut Upd765a, op: u8, eot: u8, step: u8) {
    assert_eq!(f.read_status(), 0x80);
    for b in [op, 0, 0, 0, 1, 2, eot, 0x2a, step] {
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
    panic!("bounded scan request");
}
fn supply(f: &mut Upd765a, host: &[u8]) {
    for &b in host {
        ready(f);
        assert_eq!(f.read_status(), 0xb0);
        f.write_data(b);
        for _ in 0..114 {
            f.tick(0);
        }
    }
}
fn result(f: &mut Upd765a) -> [u8; 7] {
    ready(f);
    let mut r = [0; 7];
    for b in &mut r {
        assert_eq!(f.read_status(), 0xd0);
        *b = f.read_data();
    }
    assert_eq!(f.read_status(), 0x80);
    r
}
#[test]
fn scan_equal_low_high_truth_table() {
    for op in [0x51, 0x59, 0x5d] {
        for host in [0x20, 0x21, 0x22] {
            let mut f = controller(&[0x21], false);
            command(&mut f, op, 1, 1);
            supply(&mut f, &[host; 512]);
            let satisfied = match op {
                0x51 => host == 0x21,
                0x59 => 0x21 <= host,
                _ => 0x21 >= host,
            };
            let flags = if host == 0x21 {
                8
            } else if satisfied {
                0
            } else {
                4
            };
            let r = result(&mut f);
            assert_eq!(
                r,
                [
                    if satisfied { 0 } else { 0x40 },
                    if satisfied { 0 } else { 0x80 },
                    flags,
                    0,
                    0,
                    1,
                    2
                ],
                "opcode {op:x}, host {host:x}"
            );
            assert_eq!(
                f.disk_for_export(0).expect("read only").tracks[0][0].sectors[0].data,
                [0x21; 512]
            );
        }
    }
}
#[test]
fn scan_latches_an_earlier_violating_byte() {
    for (op, mut host) in [(0x59, vec![0x22; 512]), (0x5d, vec![0x20; 512])] {
        host[0] = if op == 0x59 { 0x20 } else { 0x22 };
        let mut f = controller(&[0x21], false);
        command(&mut f, op, 1, 1);
        supply(&mut f, &host);
        assert_eq!(result(&mut f), [0x40, 0x80, 4, 0, 0, 1, 2]);
    }
}
#[test]
fn scan_step_and_hit_terminate_on_the_matched_sector() {
    let mut f = controller(&[0x21, 0x22, 0x23], false);
    command(&mut f, 0x51, 3, 2);
    supply(&mut f, &[0x23; 1024]);
    assert_eq!(result(&mut f), [0, 0, 8, 0, 0, 3, 2]);
    let mut f = controller(&[0x21, 0x22], false);
    command(&mut f, 0x51, 2, 1);
    supply(&mut f, &[0x21; 512]);
    assert_eq!(result(&mut f), [0, 0, 8, 0, 0, 1, 2]);
}
#[test]
fn scan_overrun_is_non_mutating_and_direction_is_enforced() {
    let mut f = controller(&[0x21], false);
    command(&mut f, 0x51, 1, 1);
    ready(&mut f);
    assert_eq!(f.read_data(), 0xff);
    f.write_data(0x21);
    f.write_data(0x99);
    for _ in 0..228 {
        f.tick(0);
    }
    assert_eq!(result(&mut f)[1], 0x10);
    assert_eq!(
        f.disk_for_export(0).expect("read only").tracks[0][0].sectors[0].data,
        [0x21; 512]
    );
}
#[test]
fn scan_deleted_marks_stop_or_skip_and_remain_observable() {
    let mut f = controller(&[0x21, 0x22], true);
    let mut image = f.disk_for_export(0).expect("disk").clone();
    image.tracks[0][0].sectors[1].st2 = 0;
    f.insert_disk(0, image.clone());
    command(&mut f, 0x71, 2, 1);
    supply(&mut f, &[0x22; 512]);
    assert_eq!(result(&mut f), [0, 0, 0x48, 0, 0, 2, 2]);
    f.insert_disk(0, image);
    command(&mut f, 0x51, 2, 1);
    supply(&mut f, &[0x21; 512]);
    assert_eq!(result(&mut f), [0, 0, 0x48, 0, 0, 1, 2]);
}
#[test]
fn scan_missing_and_crc_errors_terminate_abnormally() {
    let mut f = controller(&[], false);
    command(&mut f, 0x51, 1, 1);
    assert_eq!(result(&mut f)[1] & 4, 4);
    let mut f = controller(&[0x21], false);
    let mut image = f.disk_for_export(0).expect("disk").clone();
    image.tracks[0][0].sectors[0].st1 = 0x20;
    image.tracks[0][0].sectors[0].st2 = 0x20;
    f.insert_disk(0, image);
    command(&mut f, 0x51, 1, 1);
    supply(&mut f, &[0x21; 512]);
    let r = result(&mut f);
    assert_eq!(r[0] & 0x40, 0x40);
    assert_eq!(r[1] & 0x20, 0x20);
    assert_eq!(r[2] & 0x20, 0x20);
}

#[test]
fn scan_multitrack_and_missed_eot_are_distinct() {
    let mut f = controller(&[0x21], false);
    let mut image = f.disk_for_export(0).expect("disk").clone();
    image.sides = 2;
    let mut other = image.tracks[0].clone();
    other[0].sectors[0].h = 1;
    other[0].sectors[0].data.fill(0x22);
    image.tracks.push(other);
    f.insert_disk(0, image);
    command(&mut f, 0xd1, 1, 1);
    supply(&mut f, &[0x22; 1024]);
    assert_eq!(result(&mut f), [4, 0, 8, 0, 1, 1, 2]);
    let mut f = controller(&[0x21, 0x22], false);
    command(&mut f, 0xd1, 2, 2);
    supply(&mut f, &[0x23; 512]);
    assert_eq!(result(&mut f), [0x40, 0x80, 4, 0, 0, 1, 2]);
}
#[test]
fn scan_ff_is_literal_and_n0_uses_sector_length_not_step() {
    let mut f = controller(&[0x21], false);
    let mut image = f.disk_for_export(0).expect("disk").clone();
    image.tracks[0][0].sectors[0].n = 0;
    image.tracks[0][0].sectors[0].data.truncate(128);
    f.insert_disk(0, image);
    for byte in [0x51, 0, 0, 0, 1, 0, 1, 0x2a, 1] {
        f.write_data(byte);
    }
    supply(&mut f, &[0xff; 128]);
    assert_eq!(result(&mut f), [0x40, 0x80, 4, 0, 0, 1, 0]);
}
#[test]
fn scan_ejection_aborts_without_a_fictitious_comparison() {
    let mut f = controller(&[0x21], false);
    command(&mut f, 0x51, 1, 1);
    supply(&mut f, &[0x21; 17]);
    f.eject_disk(0);
    assert_eq!(result(&mut f), [0x48, 0, 0, 0, 0, 1, 2]);
}
