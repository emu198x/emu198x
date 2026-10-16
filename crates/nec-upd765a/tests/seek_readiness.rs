//! Register-level Seek checks. These use the controller's existing image-presence
//! readiness model, not a claim about physical +3 READY wiring or step timing.

use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn image() -> DiskImage {
    DiskImage {
        sides: 1,
        tracks_per_side: 40,
        tracks: vec![
            (0..40)
                .map(|c| DiskTrack {
                    sectors: vec![DiskSector {
                        c,
                        h: 0,
                        id: 1,
                        n: 2,
                        st1: 0,
                        st2: 0,
                        data: vec![0xe5; 512],
                    }],
                })
                .collect(),
        ],
    }
}

fn ready(fdc: &mut Upd765a, select: u8) -> bool {
    fdc.write_data(0x04);
    fdc.write_data(select);
    fdc.read_data() & 0x20 != 0
}

fn seek(fdc: &mut Upd765a, select: u8, target: u8) {
    for byte in [0x0f, select, target] {
        fdc.write_data(byte);
    }
}

fn complete(fdc: &mut Upd765a) {
    for hc in 0..1024 {
        fdc.tick(hc);
    }
    assert_eq!(fdc.read_status(), 0x80, "all staged seeks completed");
    assert!(fdc.interrupt, "completion must raise an interrupt");
}

fn sense_interrupt(fdc: &mut Upd765a) -> [u8; 2] {
    fdc.write_data(0x08);
    [fdc.read_data(), fdc.read_data()]
}

#[test]
fn initially_not_ready_seek_reports_error_for_each_drive_and_same_cylinder() {
    for drive in 0..4 {
        for target in [0, 20] {
            let mut fdc = Upd765a::new();
            fdc.enabled = true;
            assert!(!ready(&mut fdc, drive));
            seek(&mut fdc, drive, target);
            assert_eq!(fdc.read_status(), 0x80 | (1 << drive));
            assert!(!fdc.interrupt, "completion is still staged");
            complete(&mut fdc);
            assert_eq!(sense_interrupt(&mut fdc), [0x68 | drive, 0]);
            assert!(!fdc.interrupt, "the only completion was drained");
            assert!(!ready(&mut fdc, drive));
        }
    }
}

#[test]
fn initially_not_ready_seek_preserves_last_ready_position() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.insert_disk(1, image());
    assert!(ready(&mut fdc, 1));
    seek(&mut fdc, 1, 7);
    complete(&mut fdc);
    assert_eq!(sense_interrupt(&mut fdc), [0x21, 7]);

    fdc.eject_disk(1);
    assert!(!ready(&mut fdc, 1));
    seek(&mut fdc, 1, 20);
    complete(&mut fdc);
    assert_eq!(sense_interrupt(&mut fdc), [0x69, 7]);
}

#[test]
fn seek_uses_masked_drive_readiness_and_echoes_requested_unit() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.set_drive_select_mask(1);
    fdc.insert_disk(0, image());
    assert!(ready(&mut fdc, 2), "unit 2 aliases ready drive 0");
    seek(&mut fdc, 2, 7);
    assert_eq!(fdc.read_status(), 0x81, "physical drive 0 is busy");
    complete(&mut fdc);
    assert_eq!(sense_interrupt(&mut fdc), [0x22, 7]);

    assert!(!ready(&mut fdc, 3), "unit 3 aliases not-ready drive 1");
    seek(&mut fdc, 3, 20);
    assert_eq!(fdc.read_status(), 0x82, "physical drive 1 is busy");
    complete(&mut fdc);
    assert_eq!(sense_interrupt(&mut fdc), [0x6b, 0]);
}

#[test]
fn ready_and_not_ready_seek_completions_remain_queued_separately() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.insert_disk(0, image());
    seek(&mut fdc, 0, 12);
    seek(&mut fdc, 3, 20);
    assert_eq!(fdc.read_status(), 0x89);
    complete(&mut fdc);
    assert_eq!(sense_interrupt(&mut fdc), [0x20, 12]);
    assert!(fdc.interrupt, "drive 3 completion is still pending");
    assert_eq!(sense_interrupt(&mut fdc), [0x6b, 0]);
    assert!(!fdc.interrupt);
}
