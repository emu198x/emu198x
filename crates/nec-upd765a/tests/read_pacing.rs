//! Public-register read handshake regressions. No host-time sleeps.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn controller_pending(op: u8) -> Upd765a {
    let mut f = Upd765a::new();
    f.enabled = true;
    f.insert_disk(
        0,
        DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack {
                sectors: vec![DiskSector {
                    c: 0,
                    h: 0,
                    id: 1,
                    n: 2,
                    st1: 0,
                    st2: if op & 0x1f == 0x0c { 0x40 } else { 0 },
                    data: (0..512).map(|i| i as u8).collect(),
                }],
            }]],
        },
    );
    for b in [op, 0, 0, 0, 1, 2, 1, 0x2a, 0xff] {
        f.write_data(b);
    }
    f
}
fn controller(op: u8) -> Upd765a {
    let mut f = controller_pending(op);
    for _ in 0..10_000 {
        if f.read_status() & 0x80 != 0 {
            return f;
        }
        f.tick(0);
    }
    panic!("initial read request deadline");
}
#[test]
fn first_read_request_waits_for_pre_data_interval() {
    for (op, ticks_before_data) in [
        (0x46, 39 * 114),
        (0x4c, 39 * 114),
        (0x42, 39 * 114),
        (6, 19 * 228),
    ] {
        let mut f = controller_pending(op);
        assert_eq!(f.read_status(), 0x70);
        assert_eq!(f.read_data(), 0xff);
        ticks(&mut f, ticks_before_data - 1);
        assert_eq!(f.read_status(), 0x70);
        ticks(&mut f, 1);
        assert_eq!(f.read_status(), 0xf0);
        assert_eq!(f.read_data(), 0);
    }
}
fn ticks(f: &mut Upd765a, n: u32) {
    for _ in 0..n {
        f.tick(0);
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
fn read_consumes_only_requested_bytes_and_rejects_wrong_direction() {
    for op in [0x46, 0x4c, 0x42] {
        let mut f = controller(op);
        assert_eq!(f.read_status(), 0xf0);
        f.write_data(0x99);
        assert_eq!(f.read_data(), 0);
        assert_eq!(f.read_status(), 0x70, "RQM clears after a read");
        assert_eq!(f.read_data(), 0xff, "early read cannot consume next byte");
        ticks(&mut f, 114);
        assert_eq!(f.read_data(), 1);
    }
}
#[test]
fn fm_and_mfm_requests_follow_fixed_byte_cadence() {
    for (op, period) in [
        (0x46, 114),
        (0x4c, 114),
        (0x42, 114),
        (6, 228),
        (12, 228),
        (2, 228),
    ] {
        let mut f = controller(op);
        // A late read must not shift the following byte's arrival.
        ticks(&mut f, period - 1);
        assert_eq!(f.read_data(), 0);
        ticks(&mut f, 1);
        assert_eq!(f.read_status(), 0xf0);
        assert_eq!(f.read_data(), 1);
        ticks(&mut f, period - 1);
        assert_eq!(f.read_status(), 0x70);
        ticks(&mut f, 1);
        assert_eq!(f.read_status(), 0xf0);
    }
}
#[test]
fn missing_first_or_later_read_deadline_reports_overrun_not_eot() {
    for op in [0x46, 0x4c, 0x42] {
        for consumed in [0, 1, 511] {
            let mut f = controller(op);
            for i in 0..consumed {
                assert_eq!(f.read_data(), i as u8);
                ticks(&mut f, 114);
            }
            ticks(&mut f, 113);
            assert_eq!(f.read_status(), 0xf0);
            ticks(&mut f, 1);
            let r = result(&mut f);
            assert_eq!(&r[..3], &[0x40, 0x10, 0], "OR replaces predicted EOT");
            f.write_data(0x1b);
            assert_eq!(f.read_data(), 0x80, "command recovery");
            assert_eq!(f.read_status(), 0x80);
        }
    }
}
#[test]
fn paced_complete_payload_preserves_eot_and_drains_exactly_seven_results() {
    for op in [0x46, 0x4c, 0x42] {
        let mut f = controller(op);
        for i in 0..512 {
            assert_eq!(f.read_data(), i as u8);
            if i != 511 {
                ticks(&mut f, 114);
            }
        }
        assert_eq!(result(&mut f), [0x40, 0x80, 0, 0, 0, 1, 2]);
        assert_eq!(f.read_data(), 0xff);
    }
}

#[test]
fn overrun_retains_encountered_control_mark_but_not_a_future_one() {
    for op in [0x46, 0x4c] {
        for sectors_before_mark in [0, 1] {
            let mut f = Upd765a::new();
            f.enabled = true;
            let wanted_deleted = op == 0x4c;
            let sectors = (0..=sectors_before_mark)
                .map(|i| DiskSector {
                    c: 0,
                    h: 0,
                    id: i + 1,
                    n: 2,
                    st1: 0,
                    st2: if wanted_deleted ^ (i == sectors_before_mark) {
                        0x40
                    } else {
                        0
                    },
                    data: vec![i; 512],
                })
                .collect();
            f.insert_disk(
                0,
                DiskImage {
                    sides: 1,
                    tracks_per_side: 1,
                    tracks: vec![vec![DiskTrack { sectors }]],
                },
            );
            for b in [op, 0, 0, 0, 1, 2, sectors_before_mark + 1, 0x2a, 0xff] {
                f.write_data(b);
            }
            for _ in 0..10_000 {
                if f.read_status() & 0x80 != 0 {
                    break;
                }
                f.tick(0);
            }
            assert_eq!(f.read_status(), 0xf0);
            ticks(&mut f, 114);
            let r = result(&mut f);
            assert_eq!(
                &r[..3],
                &[0x40, 0x10, if sectors_before_mark == 0 { 0x40 } else { 0 }]
            );
        }
    }
}
