//! Host media replacement/ejection must not splice images into one read.
//! This follows the existing media-change abort policy, not READY timing.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};
mod common;

fn image(base: u8, deleted: bool, crc: bool) -> DiskImage {
    DiskImage {
        sides: 1,
        tracks_per_side: 1,
        tracks: vec![vec![DiskTrack {
            sectors: (1..=2)
                .map(|id| DiskSector {
                    c: 0,
                    h: 0,
                    id,
                    n: 2,
                    st1: if crc && id == 1 { 0x20 } else { 0 },
                    st2: if deleted { 0x40 } else { 0 } | if crc && id == 1 { 0x20 } else { 0 },
                    data: vec![base + id; 512],
                })
                .collect(),
        }]],
    }
}
fn command(f: &mut Upd765a, op: u8) {
    assert_eq!(f.read_status(), 0x80);
    for b in [op, 0, 0, 0, 1, 2, 2, 0x2a, 0xff] {
        f.write_data(b);
    }
}
fn consume(f: &mut Upd765a, count: usize, base: u8) {
    for i in 0..count {
        common::await_read(f);
        assert_eq!(f.read_status(), 0xf0);
        assert_eq!(f.read_data(), base + (i / 512 + 1) as u8);
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
#[test]
fn changing_selected_disk_aborts_reads_without_delivering_more_cached_bytes() {
    for op in [0x46, 0x4c, 0x42] {
        for (consumed, rqm_high) in [(0, false), (0, true), (1, false), (512, false), (512, true)] {
            for eject in [false, true] {
                let mut f = Upd765a::new();
                f.enabled = true;
                f.insert_disk(0, image(0x10, op == 0x4c, false));
                command(&mut f, op);
                consume(&mut f, consumed, 0x10);
                if rqm_high {
                    common::await_read(&mut f);
                }
                if eject {
                    f.eject_disk(0);
                } else {
                    f.insert_disk(0, image(0xa0, op == 0x4c, false));
                }
                assert_eq!(
                    f.read_status(),
                    0xd0,
                    "op={op:02x} consumed={consumed} high={rqm_high} eject={eject}"
                );
                for _ in 0..228 {
                    f.tick(0);
                }
                assert_eq!(
                    f.read_status(),
                    0xd0,
                    "no new payload requests after media change"
                );
                assert_eq!(
                    result(&mut f),
                    [
                        0x48,
                        if op == 0x42 && consumed == 512 { 4 } else { 0 },
                        0,
                        0,
                        0,
                        if op != 0x42 && consumed == 512 { 2 } else { 1 },
                        2
                    ]
                );
                // The next command must use the replacement, without stale progress.
                if eject {
                    f.insert_disk(0, image(0xa0, op == 0x4c, false));
                }
                command(&mut f, op);
                consume(&mut f, 1024, 0xa0);
                assert_eq!(
                    result(&mut f),
                    [
                        0x40,
                        if op == 0x42 { 0x84 } else { 0x80 },
                        0,
                        0,
                        0,
                        if op == 0x42 { 1 } else { 2 },
                        2
                    ]
                );
            }
        }
    }
}
#[test]
fn changing_unrelated_disk_does_not_abort_or_change_selected_read() {
    for op in [0x46, 0x4c, 0x42] {
        for consumed in [0, 1, 512] {
            for eject in [false, true] {
                let mut f = Upd765a::new();
                f.enabled = true;
                f.insert_disk(0, image(0x10, op == 0x4c, false));
                command(&mut f, op);
                let mut bytes = Vec::new();
                for _ in 0..consumed {
                    common::await_read(&mut f);
                    bytes.push(f.read_data());
                }
                if eject {
                    f.eject_disk(1);
                } else {
                    f.insert_disk(1, image(0xa0, false, false));
                }
                for _ in consumed..1024 {
                    common::await_read(&mut f);
                    bytes.push(f.read_data());
                }
                assert_eq!(&bytes[..512], &[0x11; 512]);
                assert_eq!(&bytes[512..], &[0x12; 512]);
                assert_eq!(
                    result(&mut f),
                    [
                        0x40,
                        if op == 0x42 { 0x84 } else { 0x80 },
                        0,
                        0,
                        0,
                        if op == 0x42 { 1 } else { 2 },
                        2
                    ]
                );
            }
        }
    }
}
#[test]
fn track_media_change_retains_completed_crc_but_not_pending_future_status() {
    let mut f = Upd765a::new();
    f.enabled = true;
    f.insert_disk(0, image(0x10, false, true));
    command(&mut f, 0x42);
    consume(&mut f, 512, 0x10);
    f.eject_disk(0);
    assert_eq!(result(&mut f), [0x48, 0x24, 0x20, 0, 0, 1, 2]);
}
#[test]
fn media_change_keeps_encountered_mark_and_excludes_incomplete_crc() {
    for op in [0x46, 0x4c] {
        let mut f = Upd765a::new();
        f.enabled = true;
        f.insert_disk(0, image(0x10, op != 0x4c, true));
        command(&mut f, op);
        consume(&mut f, 1, 0x10);
        f.insert_disk(0, image(0xa0, op == 0x4c, false));
        assert_eq!(result(&mut f), [0x48, 0, 0x40, 0, 0, 1, 2]);
    }
}
