//! Sector-model input boundary, not a claim about undocumented hardware N.
mod common;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn oversized_n(opcode: u8) {
    let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
    // Size/search guards are independent of drive readiness, including
    // the intentionally empty-media cases below.
    fdc.set_ready(0, true);
    for byte in [opcode, 0, 0, 0, 1, 255, 1, 0x2a, 0xff] {
        fdc.write_data(byte);
    }
    assert_eq!(fdc.read_status(), 0xd0);
    let result: Vec<_> = (0..7).map(|_| fdc.read_data()).collect();
    assert_eq!(result, [0x40, 4, 0, 0, 0, 1, 255]);
    assert_eq!(fdc.read_status(), 0x80);
}
#[test]
fn read_data_oversized_n_returns_a_result_instead_of_panicking() {
    oversized_n(0x46);
}
#[test]
fn read_deleted_oversized_n_returns_a_result_instead_of_panicking() {
    oversized_n(0x4c);
}

fn command(fdc: &mut Upd765a, opcode: u8, n: u8) {
    assert_eq!(fdc.read_status(), 0x80);
    for byte in [opcode, 0, 0, 0, 1, n, 1, 0x2a, 0xff] {
        fdc.write_data(byte);
    }
}
fn result(fdc: &mut Upd765a) -> [u8; 7] {
    let mut bytes = [0; 7];
    for byte in &mut bytes {
        assert_eq!(fdc.read_status(), 0xd0);
        *byte = fdc.read_data();
    }
    assert_eq!(fdc.read_status(), 0x80);
    bytes
}
fn controller(data: Option<Vec<u8>>, deleted: bool) -> Upd765a {
    let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
    // Size/search guards are independent of drive readiness, including
    // the intentionally empty-media cases below.
    fdc.set_ready(0, true);
    if let Some(data) = data {
        fdc.insert_disk(
            0,
            DiskImage {
                sides: 1,
                tracks_per_side: 1,
                tracks: vec![vec![DiskTrack {
                    sectors: vec![DiskSector {
                        c: 0,
                        h: 0,
                        id: 1,
                        n: 7,
                        st1: 0,
                        st2: if deleted { 0x40 } else { 0 },
                        data,
                    }],
                }]],
            },
        );
    }
    fdc
}
#[test]
fn all_out_of_model_sizes_are_rejected_before_payload_or_media_lookup() {
    for (opcode, deleted) in [(0x46, false), (0x4c, true)] {
        for data in [
            None,
            Some(vec![]),
            Some(vec![0x21; 3]),
            Some(vec![0x21; 16384]),
        ] {
            let mut fdc = controller(data, deleted);
            for n in 8..=255 {
                command(&mut fdc, opcode, n);
                assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 1, n]);
            }
            // A following status command must still parse and drain.
            fdc.write_data(4);
            fdc.write_data(0);
            assert_eq!(fdc.read_status(), 0xd0);
            fdc.read_data();
            assert_eq!(fdc.read_status(), 0x80);
        }
    }
}
#[test]
fn largest_in_model_size_preserves_payload_and_existing_short_field_fill() {
    for (opcode, deleted) in [(0x46, false), (0x4c, true)] {
        for data in [
            vec![],
            vec![0x12, 0x34, 0x56],
            (0..16384).map(|i| (i ^ (i >> 8)) as u8).collect(),
        ] {
            let mut fdc = controller(Some(data.clone()), deleted);
            command(&mut fdc, opcode, 255);
            assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 1, 255]);
            command(&mut fdc, opcode, 7);
            for i in 0..16384 {
                common::await_read(&mut fdc);
                assert_eq!(fdc.read_status(), 0xf0);
                let expected = data
                    .get(i)
                    .copied()
                    .unwrap_or_else(|| data.last().copied().unwrap_or(0));
                assert_eq!(fdc.read_data(), expected);
            }
            assert_eq!(result(&mut fdc), [0x40, 0x80, 0, 0, 0, 1, 7]);
            let live = fdc.disk_for_export(0).expect("read-only source image");
            assert_eq!(live.tracks[0][0].sectors[0].data, data);
        }
        let mut fdc = controller(None, deleted);
        command(&mut fdc, opcode, 7);
        assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 1, 7]);
    }
}

#[test]
fn empty_tracks_reject_oversized_n_and_keep_the_existing_missing_id_result() {
    for (opcode, deleted) in [(0x46, false), (0x4c, true)] {
        let mut fdc = controller(Some(vec![]), deleted);
        let mut image = fdc.disk_for_export(0).expect("image").clone();
        image.tracks[0][0].sectors.clear();
        fdc.insert_disk(0, image);
        for n in 8..=255 {
            command(&mut fdc, opcode, n);
            assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 1, n]);
        }
        command(&mut fdc, opcode, 7);
        assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 1, 7]);
    }
}
