//! NEC A/B printed 5-7 MT definition: side 0 EOT continues at sector 1
//! on side 1. Source precedent: MAME upd765.cpp SECTOR_READ.
mod common;
use emu198x_nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn image(deleted: bool) -> DiskImage {
    DiskImage {
        sides: 2,
        tracks_per_side: 1,
        tracks: (0..2)
            .map(|h| {
                vec![DiskTrack {
                    sectors: (1..=2)
                        .map(|r| DiskSector {
                            c: 7,
                            h,
                            id: r,
                            n: 1,
                            st1: 0,
                            st2: if deleted { 0x40 } else { 0 },
                            data: (0..256)
                                .map(|i| (i as u8).wrapping_add(h * 16 + r))
                                .collect(),
                        })
                        .collect(),
                }]
            })
            .collect(),
    }
}
fn controller(image: DiskImage) -> Upd765a {
    let mut f = Upd765a::new(SPECTRUM_128_CPU_HZ);
    f.insert_disk(0, image);
    f
}
fn command(f: &mut Upd765a, op: u8, head: u8, r: u8) {
    assert_eq!(f.read_status(), 0x80);
    for b in [op, head << 2, 7, head, r, 1, 2, 0x2a, 0xff] {
        f.write_data(b);
    }
}
fn data(f: &mut Upd765a, bytes: &[u8]) {
    for b in bytes {
        common::await_read(f);
        assert_eq!(f.read_status(), 0xf0, "payload cannot become result early");
        assert_eq!(f.read_data(), *b);
    }
}
fn result(f: &mut Upd765a) -> [u8; 7] {
    let mut bytes = [0; 7];
    for b in &mut bytes {
        assert_eq!(f.read_status(), 0xd0);
        *b = f.read_data();
    }
    assert_eq!(f.read_status(), 0x80);
    bytes
}
#[test]
fn mt_reads_side_zero_then_sector_one_on_side_one() {
    for (op, deleted) in [(0xc6, false), (0xcc, true)] {
        let image = image(deleted);
        let mut f = controller(image.clone());
        for _ in 0..2 {
            command(&mut f, op, 0, 2);
            data(&mut f, &image.tracks[0][0].sectors[1].data);
            for s in &image.tracks[1][0].sectors {
                data(&mut f, &s.data);
            }
            // Keep the established final-field no-TC result convention.
            assert_eq!(result(&mut f), [0x44, 0x80, 0, 7, 1, 2, 1]);
        }
        assert_eq!(
            format!("{:?}", f.disk_for_export(0).expect("unchanged image")),
            format!("{image:?}")
        );
    }
}
#[test]
fn mt_off_and_starting_side_one_do_not_wrap_back_to_side_zero() {
    for (op, deleted) in [(0x46, false), (0x4c, true)] {
        let image = image(deleted);
        let mut f = controller(image.clone());
        for (command_op, h) in [(op, 0), (op | 0x80, 1)] {
            command(&mut f, command_op, h, 2);
            data(&mut f, &image.tracks[usize::from(h)][0].sectors[1].data);
            assert_eq!(result(&mut f), [0x40 | h << 2, 0x80, 0, 7, h, 2, 1]);
        }
    }
}
#[test]
fn absent_recorded_side_fails_after_side_zero_payload_and_recovers() {
    for (op, deleted) in [(0xc6, false), (0xcc, true)] {
        let mut image = image(deleted);
        image.tracks[1][0].sectors.clear();
        let mut f = controller(image.clone());
        command(&mut f, op, 0, 2);
        data(&mut f, &image.tracks[0][0].sectors[1].data);
        assert_eq!(result(&mut f), [0x44, 4, 0, 7, 1, 1, 1]);
        command(&mut f, op & !0x80, 0, 2);
        data(&mut f, &image.tracks[0][0].sectors[1].data);
        assert_eq!(result(&mut f), [0x40, 0x80, 0, 7, 0, 2, 1]);
    }
}
#[test]
fn crc_or_control_mark_stops_before_head_switch() {
    for (op, deleted) in [(0xc6, false), (0xcc, true)] {
        for crc in [false, true] {
            let mut image = image(deleted);
            let s = &mut image.tracks[0][0].sectors[1];
            if crc {
                s.st1 = 0x20;
                s.st2 |= 0x20;
            } else {
                s.st2 ^= 0x40;
            }
            let bytes = s.data.clone();
            let mut f = controller(image);
            command(&mut f, op, 0, 2);
            data(&mut f, &bytes);
            assert_eq!(
                result(&mut f),
                [
                    0x40,
                    if crc { 0x20 } else { 0 },
                    if crc { 0x20 } else { 0x40 },
                    7,
                    0,
                    2,
                    1
                ]
            );
        }
    }
}

#[test]
fn skipped_side_zero_eot_continues_on_side_one() {
    for (op, deleted) in [(0xe6, false), (0xec, true)] {
        let mut image = image(deleted);
        image.tracks[0][0].sectors[1].st2 ^= 0x40;
        let mut f = controller(image.clone());
        command(&mut f, op, 0, 2);
        for s in &image.tracks[1][0].sectors {
            data(&mut f, &s.data);
        }
        assert_eq!(result(&mut f), [0x44, 0x80, 0, 7, 1, 2, 1]);
        assert_eq!(
            format!("{:?}", f.disk_for_export(0).expect("unchanged image")),
            format!("{image:?}")
        );
    }
}
