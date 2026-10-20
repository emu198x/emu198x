mod common;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn controller(mask: u8) -> Upd765a {
    let mut f = Upd765a::new();
    f.enabled = true;
    f.set_drive_select_mask(mask);
    f.insert_disk(
        0,
        DiskImage {
            sides: 1,
            tracks_per_side: 16,
            tracks: vec![
                (0..16)
                    .map(|c| DiskTrack {
                        sectors: vec![DiskSector {
                            c,
                            h: 0,
                            id: 1,
                            n: 0,
                            st1: 0,
                            st2: 0,
                            data: vec![c; 128],
                        }],
                    })
                    .collect(),
            ],
        },
    );
    f
}
fn seek(f: &mut Upd765a, select: u8, target: u8) {
    for b in [0x0f, select, target] {
        f.write_data(b);
    }
}
fn complete(fdc: &mut Upd765a) {
    // Busy now includes unacknowledged completions, so it is not a motion timer.
    for hc in 0..4096 {
        fdc.tick(hc);
    }
    assert!(fdc.interrupt, "bounded seek completion raises IRQ");
}
fn sis(f: &mut Upd765a) -> [u8; 2] {
    f.write_data(8);
    let result = [f.read_data(), f.read_data()];
    assert_eq!(f.read_status() & 0xf0, 0x80);
    result
}
#[test]
fn alias_pcns_and_pending_statuses_are_independent() {
    let mut f = controller(1);
    seek(&mut f, 0, 5);
    seek(&mut f, 2, 3);
    assert_eq!(
        f.read_status() & 15,
        5,
        "logical units0 and2 own distinct busy bits"
    );
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x20, 5]);
    assert_eq!(sis(&mut f), [0x22, 3]);
    f.write_data(8);
    assert_eq!(f.read_data(), 0x80);
}
#[test]
fn alias_seek_steps_from_its_own_pcn_to_physical_track_eight() {
    let mut f = controller(1);
    seek(&mut f, 0, 5);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x20, 5]);
    seek(&mut f, 2, 3);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x22, 3]);
    for b in [0x46, 0, 8, 0, 1, 0, 1, 42, 128] {
        f.write_data(b);
    }
    for _ in 0..128 {
        common::await_read(&mut f);
        assert_eq!(f.read_data(), 8);
    }
    let mut result = [0; 7];
    for b in &mut result {
        *b = f.read_data();
    }
    assert_eq!(result, [0x40, 0x80, 0, 8, 0, 1, 0]);
    assert_eq!(f.read_status(), 0x80);
    seek(&mut f, 2, 1);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x22, 1]);
    for b in [0x4a, 0] {
        f.write_data(b);
    }
    let mut id = [0; 7];
    for b in &mut id {
        *b = f.read_data();
    }
    assert_eq!(id, [0, 0, 0, 6, 0, 1, 0], "outward delta reaches physical6");
}
#[test]
fn alias_failed_seek_preserves_pcn_and_echoes_unit_and_head() {
    let mut f = controller(1);
    seek(&mut f, 6, 3);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x26, 3]);
    f.eject_disk(0);
    seek(&mut f, 6, 9);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x6e, 3]);
}
#[test]
fn recalibrate_clears_only_selected_logical_pcn() {
    let mut f = controller(1);
    seek(&mut f, 0, 5);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x20, 5]);
    seek(&mut f, 2, 3);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x22, 3]);
    for b in [7, 2] {
        f.write_data(b);
    }
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x22, 0]);
    // Unit0 remains at PCN5: requesting5 emits no step after physical recal.
    seek(&mut f, 0, 5);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x20, 5]);
    f.write_data(0x04);
    f.write_data(0);
    assert_ne!(f.read_data() & 0x10, 0);
}
#[test]
fn identity_mask_keeps_independent_physical_drives() {
    let mut f = controller(3);
    f.insert_disk(2, DiskImage::default());
    seek(&mut f, 0, 5);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x20, 5]);
    seek(&mut f, 2, 3);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x22, 3]);
    seek(&mut f, 0, 5);
    complete(&mut f);
    assert_eq!(sis(&mut f), [0x20, 5]);
}
