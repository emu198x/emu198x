//! Read Track is bounded to represented sector payloads, never fabricated gaps/flux.
//! NEC December 1978 p14; MAME read_track_continue counts EOT sectors.
mod common;
use emu198x_nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;
fn controller() -> Upd765a {
    let mut f = Upd765a::new(SPECTRUM_128_CPU_HZ);
    f.insert_disk(
        0,
        DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack {
                sectors: [3, 1, 2]
                    .into_iter()
                    .map(|id| DiskSector {
                        c: 0,
                        h: 0,
                        id,
                        n: 2,
                        st1: 0,
                        st2: 0,
                        data: vec![id; 512],
                    })
                    .collect(),
            }]],
        },
    );
    f
}
fn command(f: &mut Upd765a, opcode: u8, eot: u8) {
    for b in [opcode, 0, 0, 0, 1, 2, eot, 0x2a, 0xff] {
        f.write_data(b);
    }
}
fn drain(f: &mut Upd765a) -> (Vec<u8>, [u8; 7]) {
    let mut data = Vec::new();
    while f.read_status() & 0x20 != 0 {
        common::await_read(f);
        assert!(data.len() < 4096);
        data.push(f.read_data());
    }
    let mut result = [0; 7];
    for b in &mut result {
        assert_eq!(f.read_status(), 0xd0);
        *b = f.read_data();
    }
    assert_eq!(f.read_status(), 0x80);
    (data, result)
}
#[test]
fn track_reads_physical_order_and_eot_count_not_sorted_ids() {
    for (count, ids, status) in [
        (2, vec![3, 1], 0x80),
        (3, vec![3, 1, 2], 0x84),
        (4, vec![3, 1, 2, 3], 0x84),
    ] {
        let mut f = controller();
        command(&mut f, 0x42, count);
        let (data, result) = drain(&mut f);
        assert_eq!(
            data,
            ids.into_iter()
                .flat_map(|id| vec![id; 512])
                .collect::<Vec<_>>()
        );
        assert_eq!(result, [0x40, status, 0, 0, 0, 1, 2]);
    }
}
#[test]
fn track_ignores_mt_and_sk_and_preserves_source() {
    let mut f = controller();
    let source = f.disk_for_export(0).expect("image").clone();
    command(&mut f, 0xe2, 3);
    let (data, _) = drain(&mut f);
    assert_eq!(data.len(), 1536);
    let after = f.disk_for_export(0).expect("image");
    for (a, b) in after.tracks[0][0]
        .sectors
        .iter()
        .zip(&source.tracks[0][0].sectors)
    {
        assert_eq!(a.data, b.data);
        assert_eq!(
            (a.c, a.h, a.id, a.n, a.st1, a.st2),
            (b.c, b.h, b.id, b.n, b.st1, b.st2)
        );
    }
}
#[test]
fn track_errors_continue_to_next_sector() {
    let mut f = controller();
    let mut image = f.disk_for_export(0).expect("image").clone();
    image.tracks[0][0].sectors[0].st1 = 0x20;
    image.tracks[0][0].sectors[0].st2 = 0x20;
    f.insert_disk(0, image);
    command(&mut f, 0x42, 3);
    let (data, result) = drain(&mut f);
    assert_eq!(data.len(), 1536);
    assert_eq!(result, [0x40, 0xa4, 0x20, 0, 0, 1, 2]);
}
#[test]
fn empty_and_missing_track_have_bounded_error_results() {
    let mut f = controller();
    f.insert_disk(0, DiskImage::default());
    command(&mut f, 0x42, 1);
    let (data, r) = drain(&mut f);
    assert!(data.is_empty());
    assert_eq!(r[0] & 0x40, 0x40);
    assert_eq!(r[1] & 1, 1);
    f.eject_disk(0);
    command(&mut f, 0x42, 1);
    let (data, r) = drain(&mut f);
    assert!(data.is_empty());
    assert_eq!(r, [0x48, 0, 0, 0, 0, 1, 2]);
}
