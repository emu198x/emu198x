//! Read ID signal-level results, independent of rotational and head-load timing.
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

fn image() -> DiskImage {
    DiskImage {
        sides: 1,
        tracks_per_side: 1,
        tracks: vec![vec![DiskTrack {
            sectors: [7, 9]
                .into_iter()
                .map(|id| DiskSector {
                    c: 4,
                    h: 3,
                    id,
                    n: 1,
                    st1: 0,
                    st2: 0,
                    data: vec![0xa5; 256],
                })
                .collect(),
        }]],
    }
}

fn command(fdc: &mut Upd765a, select: u8) {
    assert_eq!(fdc.read_status(), 0x80);
    fdc.write_data(0x4a);
    assert!(!fdc.interrupt, "opcode alone has not completed Read ID");
    fdc.write_data(select);
    assert_eq!(fdc.read_status(), 0xd0);
}

fn drain(fdc: &mut Upd765a) -> [u8; 7] {
    let result = std::array::from_fn(|_| fdc.read_data());
    assert_eq!(fdc.read_status(), 0x80);
    result
}

#[test]
fn ready_low_rejects_a_mounted_id_without_advancing_the_cursor() {
    for select in 0..8 {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.set_drive_select_mask(1);
        let physical = usize::from(select & 1);
        fdc.insert_disk(physical, image());
        fdc.set_ready(physical, false);
        command(&mut fdc, select);
        let result = drain(&mut fdc);
        assert_eq!(&result[..3], &[0x48 | select, 0, 0], "select={select}");
        fdc.set_ready(physical, true);
        command(&mut fdc, select & 3);
        assert_eq!(drain(&mut fdc), [select & 3, 0, 0, 4, 3, 7, 1]);
    }
}

#[test]
fn ready_high_without_media_is_missing_id_not_not_ready() {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.set_ready(1, true);
    command(&mut fdc, 5);
    assert_eq!(&drain(&mut fdc)[..3], &[0x45, 5, 0]);
}

#[test]
fn result_irq_asserts_for_success_and_errors_and_first_byte_acknowledges() {
    for (mounted, ready) in [(true, true), (true, false), (false, true)] {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        if mounted {
            fdc.insert_disk(0, image());
        }
        fdc.set_ready(0, ready);
        command(&mut fdc, 0);
        assert!(
            fdc.interrupt,
            "result entry raises IRQ mounted={mounted} ready={ready}"
        );
        assert_eq!(fdc.read_status(), 0xd0);
        assert!(fdc.interrupt, "MSR reads do not acknowledge result IRQ");
        let _ = fdc.read_data();
        assert!(!fdc.interrupt, "first result byte acknowledges IRQ");
        for _ in 1..7 {
            assert_eq!(fdc.read_status(), 0xd0);
            let _ = fdc.read_data();
            assert!(
                !fdc.interrupt,
                "unread result bytes do not raise another IRQ"
            );
        }
        assert_eq!(fdc.read_status(), 0x80);
        fdc.write_data(8);
        assert_eq!(
            fdc.read_data(),
            0x80,
            "Read ID completion is not a pending SIS record"
        );
    }
}
