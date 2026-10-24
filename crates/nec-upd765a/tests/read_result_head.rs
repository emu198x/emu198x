//! ID H in read results is independent of physical HD in ST0 and side selection.
mod common;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn controller(opcode: u8, h: u8, crc: bool) -> Upd765a {
    let mut f = Upd765a::new(SPECTRUM_128_CPU_HZ);
    f.insert_disk(
        0,
        DiskImage {
            sides: 2,
            tracks_per_side: 1,
            tracks: (0..2)
                .map(|side| {
                    vec![DiskTrack {
                        sectors: vec![DiskSector {
                            c: 0,
                            h,
                            id: 1,
                            n: 2,
                            st1: if crc { 0x20 } else { 0 },
                            st2: (if opcode == 0x4c { 0x40 } else { 0 })
                                | if crc { 0x20 } else { 0 },
                            data: vec![0x31 + side; 512],
                        }],
                    }]
                })
                .collect(),
        },
    );
    f
}
fn image_signature(f: &Upd765a) -> Vec<([u8; 6], Vec<u8>)> {
    f.disk_for_export(0)
        .expect("mounted source")
        .tracks
        .iter()
        .flat_map(|side| side.iter())
        .flat_map(|track| track.sectors.iter())
        .map(|s| ([s.c, s.h, s.id, s.n, s.st1, s.st2], s.data.clone()))
        .collect()
}
fn command(f: &mut Upd765a, opcode: u8, hd: u8, h: u8, eot: u8) {
    assert_eq!(f.read_status(), 0x80);
    for b in [opcode, hd << 2, 0, h, 1, 2, eot, 0x2a, 0xff] {
        f.write_data(b);
    }
}
fn payload(f: &mut Upd765a, bytes: usize, hd: u8) {
    for _ in 0..bytes {
        common::await_read(f);
        assert_eq!(f.read_status(), 0xf0);
        assert_eq!(f.read_data(), 0x31 + hd, "physical side supplies payload");
    }
}
fn result(f: &mut Upd765a, expected: [u8; 7]) {
    let mut observed = [0; 7];
    for b in &mut observed {
        assert_eq!(f.read_status(), 0xd0);
        *b = f.read_data();
    }
    assert_eq!(observed, expected);
    assert_eq!(f.read_status(), 0x80);
    assert_eq!(f.read_data(), 0xff, "no eighth result byte");
    f.write_data(0x1b);
    assert_eq!(f.read_data(), 0x80, "following command parses normally");
    assert_eq!(f.read_status(), 0x80);
}
#[test]
fn completed_read_and_crc_error_return_id_head_while_st0_keeps_physical_head() {
    for opcode in [0x46, 0x4c] {
        for (hd, h) in [(0, 1), (1, 0), (0, 0), (1, 1)] {
            for crc in [false, true] {
                let mut f = controller(opcode, h, crc);
                let source = image_signature(&f);
                command(&mut f, opcode, hd, h, 1);
                payload(&mut f, 512, hd);
                result(
                    &mut f,
                    [
                        0x40 | hd << 2,
                        if crc { 0x20 } else { 0x80 },
                        if crc { 0x20 } else { 0 },
                        0,
                        h,
                        1,
                        2,
                    ],
                );
                assert_eq!(image_signature(&f), source);
            }
        }
    }
}
#[test]
fn interrupted_read_returns_requested_id_head() {
    for opcode in [0x46, 0x4c] {
        for (hd, h) in [(0, 1), (1, 0), (0, 0), (1, 1)] {
            for consumed in [0, 1, 511] {
                let mut f = controller(opcode, h, false);
                let source = image_signature(&f);
                command(&mut f, opcode, hd, h, 1);
                payload(&mut f, consumed, hd);
                common::await_read(&mut f);
                for _ in 0..114 {
                    f.tick();
                }
                result(&mut f, [0x40 | hd << 2, 0x10, 0, 0, h, 1, 2]);
                command(&mut f, opcode, hd, h, 1);
                payload(&mut f, 512, hd);
                result(&mut f, [0x40 | hd << 2, 0x80, 0, 0, h, 1, 2]);
                assert_eq!(image_signature(&f), source);
            }
        }
    }
}
#[test]
fn missing_first_and_later_id_return_requested_head_and_recover() {
    for opcode in [0x46, 0x4c] {
        for (hd, h) in [(0, 1), (1, 0), (0, 0), (1, 1)] {
            for missing_first in [false, true] {
                let recorded_h = h ^ u8::from(missing_first);
                let mut f = controller(opcode, recorded_h, false);
                let source = image_signature(&f);
                command(&mut f, opcode, hd, h, 2);
                if !missing_first {
                    payload(&mut f, 512, hd);
                }
                result(
                    &mut f,
                    [
                        0x40 | hd << 2,
                        4,
                        0,
                        0,
                        h,
                        if missing_first { 1 } else { 2 },
                        2,
                    ],
                );
                command(&mut f, opcode, hd, recorded_h, 1);
                payload(&mut f, 512, hd);
                result(&mut f, [0x40 | hd << 2, 0x80, 0, 0, recorded_h, 1, 2]);
                assert_eq!(image_signature(&f), source);
            }
        }
    }
}
