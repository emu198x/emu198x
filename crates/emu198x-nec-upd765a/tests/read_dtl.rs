//! NEC December1978 p12: N=0 DTL limits host bytes, not the physical field
//! or its CRC. Fuse upd_fdc_read_data independently consumes the unseen tail.
mod common;
use emu198x_nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn image(n: u8, deleted: bool, crc: bool) -> DiskImage {
    DiskImage {
        sides: 1,
        tracks_per_side: 1,
        tracks: vec![vec![DiskTrack {
            sectors: (1..=2)
                .map(|r| DiskSector {
                    c: 0,
                    h: 0,
                    id: r,
                    n,
                    st1: if crc { 0x20 } else { 0 },
                    st2: (if deleted { 0x40 } else { 0 }) | if crc { 0x20 } else { 0 },
                    data: (0..128usize << n)
                        .map(|i| (i as u8).wrapping_add(r))
                        .collect(),
                })
                .collect(),
        }]],
    }
}
fn controller(image: DiskImage) -> Upd765a {
    let mut f = Upd765a::new(SPECTRUM_128_CPU_HZ);
    f.insert_disk(0, image);
    f
}
fn command(f: &mut Upd765a, op: u8, n: u8, eot: u8, dtl: u8) {
    assert_eq!(f.read_status(), 0x80);
    for b in [op, 0, 0, 0, 1, n, eot, 0x2a, dtl] {
        f.write_data(b);
    }
}
fn data(f: &mut Upd765a, bytes: &[u8]) {
    for b in bytes {
        common::await_read(f);
        assert_eq!(f.read_status(), 0xf0);
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
fn tail(f: &mut Upd765a) -> u32 {
    for ticks in 0..40_000 {
        if f.read_status() == 0xd0 {
            return ticks;
        }
        assert_eq!(
            f.read_status(),
            0x70,
            "unseen tail must not request host bytes"
        );
        assert_eq!(f.read_data(), 0xff, "polling cannot consume internal bytes");
        f.tick();
    }
    panic!("internal field deadline");
}
#[test]
fn zero_n_sends_dtl_prefix_then_drains_the_complete_field() {
    for (op, deleted) in [(0x06, false), (0x0c, true)] {
        for dtl in [1, 2, 17, 127] {
            let image = image(0, deleted, false);
            let mut f = controller(image.clone());
            for _ in 0..2 {
                command(&mut f, op, 0, 1, dtl);
                data(
                    &mut f,
                    &image.tracks[0][0].sectors[0].data[..usize::from(dtl)],
                );
                assert!(tail(&mut f) > 0, "complete internal field before result");
                assert_eq!(result(&mut f), [0x40, 0x80, 0, 0, 0, 1, 0]);
            }
            assert_eq!(
                format!("{:?}", f.disk_for_export(0).expect("image")),
                format!("{image:?}")
            );
        }
    }
}
#[test]
fn crc_status_waits_for_the_unseen_suffix() {
    for (op, deleted) in [(0x06, false), (0x0c, true)] {
        let image = image(0, deleted, true);
        let mut f = controller(image.clone());
        command(&mut f, op, 0, 1, 2);
        data(&mut f, &image.tracks[0][0].sectors[0].data[..2]);
        tail(&mut f);
        assert_eq!(result(&mut f), [0x40, 0x20, 0x20, 0, 0, 1, 0]);
    }
}
#[test]
fn nonzero_n_and_full_dtl_keep_the_full_payload() {
    for (op, deleted) in [(0x06, false), (0x0c, true)] {
        for (n, dtl) in [(1, 1), (1, 0), (0, 128), (0, 255)] {
            let image = image(n, deleted, false);
            let mut f = controller(image.clone());
            command(&mut f, op, n, 1, dtl);
            data(&mut f, &image.tracks[0][0].sectors[0].data);
            assert_eq!(result(&mut f), [0x40, 0x80, 0, 0, 0, 1, n]);
        }
    }
}
#[test]
fn missed_prefix_request_still_overruns() {
    for (op, deleted) in [(0x06, false), (0x0c, true)] {
        let mut f = controller(image(0, deleted, false));
        command(&mut f, op, 0, 1, 2);
        common::await_read(&mut f);
        assert_eq!(f.read_data(), 1);
        common::await_read(&mut f);
        for _ in 0..228 {
            f.tick();
        }
        assert_eq!(result(&mut f), [0x40, 0x10, 0, 0, 0, 1, 0]);
    }
}
#[test]
fn read_track_keeps_its_existing_full_field_contract() {
    let image = image(0, false, false);
    let mut f = controller(image.clone());
    command(&mut f, 0x02, 0, 1, 2);
    data(&mut f, &image.tracks[0][0].sectors[0].data);
    assert_eq!(result(&mut f), [0x40, 0x80, 0, 0, 0, 1, 0]);
}

#[test]
fn internal_tail_keeps_absolute_byte_clock_despite_host_latency() {
    for (op, deleted, slots, micros) in [
        (0x06, false, 19, 64),
        (0x0c, true, 19, 64),
        (0x46, false, 39, 32),
        (0x4c, true, 39, 32),
    ] {
        for dtl in [1, 17, 127] {
            for latency in [0, 100] {
                let mut f = controller(image(0, deleted, false));
                command(&mut f, op, 0, 1, dtl);
                let mut consumed = 0;
                let mut elapsed = 0;
                let mut delay = latency;
                loop {
                    if f.read_status() == 0xd0 {
                        break;
                    }
                    if f.read_status() == 0xf0 {
                        if delay == 0 {
                            assert!(consumed < dtl, "no request beyond DTL");
                            assert_eq!(f.read_data(), consumed + 1);
                            consumed += 1;
                        } else {
                            delay -= 1;
                        }
                    }
                    assert!(elapsed < 40_000, "field deadline");
                    f.tick();
                    elapsed += 1;
                }
                assert_eq!(consumed, dtl);
                let expected = ((slots + 127) * micros * SPECTRUM_128_CPU_HZ).div_ceil(1_000_000);
                assert_eq!(
                    elapsed, expected,
                    "DTL and host latency cannot shorten/shift physical field"
                );
                assert_eq!(result(&mut f), [0x40, 0x80, 0, 0, 0, 1, 0]);
            }
        }
    }
}

#[test]
fn each_sector_sends_its_own_prefix_before_eot() {
    for (op, deleted) in [(0x06, false), (0x0c, true)] {
        let image = image(0, deleted, false);
        let mut f = controller(image.clone());
        command(&mut f, op, 0, 2, 2);
        data(&mut f, &image.tracks[0][0].sectors[0].data[..2]);
        for ticks in 0..40_000 {
            assert_eq!(
                f.read_status(),
                0x70,
                "no intermediate result or tail request"
            );
            f.tick();
            if f.read_status() == 0xf0 {
                break;
            }
            assert!(ticks < 39_999, "next-sector deadline");
        }
        data(&mut f, &image.tracks[0][0].sectors[1].data[..2]);
        tail(&mut f);
        assert_eq!(result(&mut f), [0x40, 0x80, 0, 0, 0, 2, 0]);
    }
}
