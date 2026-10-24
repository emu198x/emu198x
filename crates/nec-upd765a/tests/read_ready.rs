//! Physical READY admits read commands independently of mounted media.
mod common;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn image(deleted: bool) -> DiskImage {
    DiskImage {
        sides: 1,
        tracks_per_side: 1,
        tracks: vec![vec![DiskTrack {
            sectors: vec![DiskSector {
                c: 7,
                h: 3,
                id: 9,
                n: 1,
                st1: 0,
                st2: if deleted { 0x40 } else { 0 },
                data: (0..=255).collect(),
            }],
        }]],
    }
}

fn command(fdc: &mut Upd765a, opcode: u8, select: u8) {
    assert_eq!(fdc.read_status(), 0x80);
    for byte in [opcode, select, 7, 3, 9, 1, 1, 0x2a, 0xff] {
        fdc.write_data(byte);
    }
}

fn result(fdc: &mut Upd765a) -> [u8; 7] {
    let result = std::array::from_fn(|_| {
        assert_eq!(
            fdc.read_status(),
            0xd0,
            "result without any execution bytes"
        );
        fdc.read_data()
    });
    assert_eq!(fdc.read_status(), 0x80, "complete result drains to idle");
    result
}

fn ready_admission(opcode: u8) {
    for select in 0..4 {
        let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
        fdc.set_drive_select_mask(1);
        let physical = usize::from(select & 1);
        let source = image(opcode == 0x4c);
        fdc.insert_disk(physical, source.clone());
        fdc.set_ready(physical ^ 1, true);
        fdc.set_ready(physical, false);
        command(&mut fdc, opcode, select);
        let rejected = result(&mut fdc);
        assert_eq!(
            &rejected[..3],
            &[0x48 | select, 0, 0],
            "opcode={opcode:02x} select={select}"
        );
        // Error CHRN is retained model formatting, not a hardware oracle.
        assert_eq!(
            &rejected[3..],
            &[7, if opcode == 0x42 { 0 } else { 3 }, 9, 1]
        );
        fdc.set_ready(physical, true);
        command(&mut fdc, opcode, select);
        for expected in &source.tracks[0][0].sectors[0].data {
            common::await_read(&mut fdc);
            assert_eq!(fdc.read_status(), 0xf0, "READY-high reaches execution");
            assert_eq!(fdc.read_data(), *expected);
        }
        assert_eq!(
            result(&mut fdc),
            [
                0x40 | select,
                0x80,
                0,
                7,
                if opcode == 0x42 { 0 } else { 3 },
                9,
                1
            ]
        );
        let live = fdc
            .disk_for_export(physical)
            .expect("read preserves mounted image");
        assert_eq!(
            format!("{live:?}"),
            format!("{source:?}"),
            "reads preserve all represented image fields"
        );
    }
}

#[test]
fn read_data_samples_physical_ready_and_recovers() {
    ready_admission(0x46);
}

#[test]
fn read_deleted_data_samples_physical_ready_and_recovers() {
    ready_admission(0x4c);
}

#[test]
fn read_track_samples_physical_ready_and_recovers() {
    ready_admission(0x42);
}
