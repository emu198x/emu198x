use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn status(fdc: &mut Upd765a, unit_head: u8) -> u8 {
    assert_eq!(fdc.read_status(), 0x80, "idle before SDS");
    fdc.write_data(4);
    fdc.write_data(unit_head);
    assert_eq!(fdc.read_status(), 0xd0, "one result byte");
    let status = fdc.read_data();
    assert_eq!(fdc.read_status(), 0x80, "result fully drained");
    status
}

fn image() -> DiskImage {
    DiskImage {
        sides: 1,
        tracks_per_side: 1,
        tracks: vec![vec![DiskTrack {
            sectors: vec![DiskSector {
                data: vec![0xa5; 128],
                c: 0,
                h: 0,
                id: 1,
                n: 0,
                st1: 0,
                st2: 0,
            }],
        }]],
    }
}

#[test]
fn two_side_ready_and_write_protect_are_independent_inputs() {
    for unit in 0..4 {
        for mounted in [false, true] {
            for writable in [false, true] {
                for ready in [false, true] {
                    for two_side in [false, true] {
                        let mut fdc = Upd765a::new();
                        fdc.enabled = true;
                        if mounted {
                            fdc.insert_disk_with_writable(unit, image(), writable);
                        }
                        fdc.set_ready(unit, ready);
                        fdc.set_two_side(unit, two_side);
                        for head in 0..2 {
                            let selected = unit as u8 | head << 2;
                            let st3 = status(&mut fdc, selected);
                            assert_eq!(st3 & 7, selected, "unit/head echo");
                            assert_eq!(st3 & 8 != 0, two_side, "TS independent of image");
                            assert_eq!(st3 & 0x20 != 0, ready, "READY independent of TS");
                            assert_eq!(
                                st3 & 0x40 != 0,
                                mounted && !writable,
                                "WP independent of TS"
                            );
                            assert_eq!(st3 & 0x80, 0, "no invented fault");
                        }
                        if mounted {
                            assert_eq!(
                                fdc.disk_for_export(unit).expect("mounted").tracks[0][0].sectors[0]
                                    .data,
                                vec![0xa5; 128],
                                "SDS leaves media unchanged"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn two_side_level_survives_reset_and_media_changes() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.set_two_side(0, true);
    fdc.insert_disk_with_writable(0, image(), true);
    fdc.reset();
    assert_eq!(status(&mut fdc, 0) & 8, 8);
    fdc.eject_disk(0);
    assert_eq!(status(&mut fdc, 0) & 8, 8, "external input is not media");
    fdc.set_two_side(0, false);
    fdc.insert_disk(0, image());
    assert_eq!(
        status(&mut fdc, 0) & 8,
        0,
        "protected media cannot override TS"
    );
    fdc.set_two_side(4, true);
    fdc.set_two_side(usize::MAX, true);
    assert_eq!(status(&mut fdc, 0) & 8, 0, "invalid units ignored");
    assert!(!fdc.is_write_protected(usize::MAX));
}

#[test]
fn external_unit_alias_selects_physical_ts_but_echoes_logical_unit() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.set_drive_select_mask(1);
    fdc.set_two_side(0, true);
    fdc.set_two_side(1, false);
    fdc.set_two_side(2, false);
    fdc.set_two_side(3, true);
    for selected in 0..8 {
        let st3 = status(&mut fdc, selected);
        assert_eq!(st3 & 7, selected);
        assert_eq!(st3 & 8 != 0, selected & 1 == 0);
    }
}
