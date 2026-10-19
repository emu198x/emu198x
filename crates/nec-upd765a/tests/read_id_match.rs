//! Read Data/Deleted must match recorded CHRN on the physically selected side.
//! Result CHRN and physical search timing are separate conformance work.
mod common;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn sector(h: u8, n: u8, value: u8, deleted: bool) -> DiskSector {
    DiskSector {
        c: 0,
        h,
        id: 1,
        n,
        st1: 0,
        st2: if deleted { 0x40 } else { 0 },
        data: vec![value; 128_usize << n],
    }
}
fn controller(sides: Vec<Vec<DiskSector>>) -> Upd765a {
    let mut f = Upd765a::new();
    f.enabled = true;
    f.insert_disk(
        0,
        DiskImage {
            sides: sides.len() as u8,
            tracks_per_side: 1,
            tracks: sides
                .into_iter()
                .map(|sectors| vec![DiskTrack { sectors }])
                .collect(),
        },
    );
    f
}
fn image_signature(f: &Upd765a) -> Vec<([u8; 6], Vec<u8>)> {
    f.disk_for_export(0)
        .expect("read-only mounted image")
        .tracks
        .iter()
        .flat_map(|side| side.iter())
        .flat_map(|track| track.sectors.iter())
        .map(|s| ([s.c, s.h, s.id, s.n, s.st1, s.st2], s.data.clone()))
        .collect()
}
fn read(f: &mut Upd765a, deleted: bool, physical_head: u8, h: u8, n: u8) -> (Vec<u8>, [u8; 7]) {
    assert_eq!(f.read_status(), 0x80, "command starts idle");
    for b in [
        if deleted { 0x4c } else { 0x46 },
        physical_head << 2,
        0,
        h,
        1,
        n,
        1,
        0x2a,
        0x80,
    ] {
        f.write_data(b);
    }
    let mut bytes = Vec::new();
    while f.read_status() & 0x20 != 0 {
        common::await_read(f);
        assert_eq!(f.read_status(), 0xf0);
        assert!(bytes.len() < 2048, "bounded public-register transfer");
        bytes.push(f.read_data());
    }
    let mut result = [0; 7];
    for b in &mut result {
        assert_eq!(
            f.read_status(),
            0xd0,
            "all seven result bytes are available"
        );
        *b = f.read_data();
    }
    assert_eq!(f.read_status(), 0x80, "result drains to idle");
    (bytes, result)
}
#[test]
fn wrong_recorded_head_is_rejected_and_matching_request_recovers() {
    for deleted in [false, true] {
        for (recorded, requested) in [(1, 0), (0, 1), (7, 0), (0xff, 0)] {
            let mut f = controller(vec![vec![sector(recorded, 2, 0x31, deleted)]]);
            let before = image_signature(&f);
            let (bytes, result) = read(&mut f, deleted, 0, requested, 2);
            assert!(bytes.is_empty(), "wrong H must not deliver its payload");
            assert_eq!(&result[..3], &[0x40, 4, 0], "ND without invented WC/BC");
            let (bytes, result) = read(&mut f, deleted, 0, recorded, 2);
            assert_eq!(bytes, vec![0x31; 512]);
            assert_eq!(&result[..3], &[0x40, 0x80, 0]);
            assert_eq!(image_signature(&f), before);
        }
    }
}
#[test]
fn wrong_recorded_size_is_rejected_and_matching_request_recovers() {
    for deleted in [false, true] {
        for recorded_n in [0, 1, 3] {
            let mut f = controller(vec![vec![sector(0, recorded_n, 0x42, deleted)]]);
            let before = image_signature(&f);
            let (bytes, result) = read(&mut f, deleted, 0, 0, 2);
            assert!(
                bytes.is_empty(),
                "wrong N must not be truncated or padded into a match"
            );
            assert_eq!(&result[..3], &[0x40, 4, 0]);
            let (bytes, result) = read(&mut f, deleted, 0, 0, recorded_n);
            assert_eq!(bytes, vec![0x42; 128_usize << recorded_n]);
            assert_eq!(&result[..3], &[0x40, 0x80, 0]);
            assert_eq!(image_signature(&f), before);
        }
    }
}
#[test]
fn later_exact_duplicate_is_not_hidden_by_wrong_head_or_size() {
    for deleted in [false, true] {
        for (h, n) in [(0, 2), (1, 1), (0, 1)] {
            let mut f = controller(vec![vec![
                sector(h, n, 0x11, deleted),
                sector(1, 2, 0xa5, deleted),
            ]]);
            let before = image_signature(&f);
            let (bytes, result) = read(&mut f, deleted, 0, 1, 2);
            assert_eq!(bytes, vec![0xa5; 512], "select the later exact CHRN record");
            assert_eq!(&result[..3], &[0x40, 0x80, 0]);
            assert_eq!(image_signature(&f), before);
        }
    }
}
#[test]
fn requested_id_head_does_not_select_the_physical_side() {
    for deleted in [false, true] {
        let mut f = controller(vec![
            vec![sector(1, 2, 0x11, deleted)],
            vec![sector(0, 2, 0xa2, deleted)],
        ]);
        let before = image_signature(&f);
        for (physical_head, id_head, value) in [(0, 1, 0x11), (1, 0, 0xa2)] {
            let (bytes, result) = read(&mut f, deleted, physical_head, id_head, 2);
            assert_eq!(bytes, vec![value; 512]);
            assert_eq!(&result[..3], &[0x40 | physical_head << 2, 0x80, 0]);
        }
        // An exact ID on the other physical side cannot satisfy this search.
        let (bytes, result) = read(&mut f, deleted, 0, 0, 2);
        assert!(bytes.is_empty());
        assert_eq!(&result[..3], &[0x40, 4, 0]);
        assert_eq!(image_signature(&f), before);
    }
}
