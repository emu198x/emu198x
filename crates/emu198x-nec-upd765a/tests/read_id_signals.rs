//! Read ID signal-level results, independent of rotational and head-load timing.
use emu198x_nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

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
        let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
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
    let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
    fdc.set_ready(1, true);
    command(&mut fdc, 5);
    assert_eq!(&drain(&mut fdc)[..3], &[0x45, 5, 0]);
}

#[test]
fn result_irq_asserts_for_success_and_errors_and_first_byte_acknowledges() {
    for (mounted, ready) in [(true, true), (true, false), (false, true)] {
        let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
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

#[test]
fn recorded_order_duplicate_ids_and_physical_drive_cursors_remain_distinct() {
    let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
    let mut source = image();
    let mut duplicate = source.tracks[0][0].sectors[0].clone();
    duplicate.c = 8;
    duplicate.h = 2;
    duplicate.n = 0;
    duplicate.data = vec![0x71; 128];
    source.tracks[0][0].sectors.insert(1, duplicate);
    source.tracks[0][0].sectors[2].id = 3;
    source.sides = 2;
    let mut back = source.tracks[0].clone();
    for sector in &mut back[0].sectors {
        sector.c = 19;
        sector.h = 1;
    }
    source.tracks.push(back);
    for drive in 0..4 {
        fdc.insert_disk(drive, source.clone());
    }
    let inspect = |fdc: &mut Upd765a, select, expected| {
        command(fdc, select);
        assert_eq!(drain(fdc), expected);
    };
    // Duplicate R=7 IDs with different CHN must retain recorded order.
    inspect(&mut fdc, 0, [0, 0, 0, 4, 3, 7, 1]);
    inspect(&mut fdc, 0, [0, 0, 0, 8, 2, 7, 0]);
    // Other physical drives have not consumed either preceding ID.
    for drive in 1..4 {
        inspect(&mut fdc, drive, [drive, 0, 0, 4, 3, 7, 1]);
    }
    inspect(&mut fdc, 0, [0, 0, 0, 4, 3, 3, 1]);
    inspect(&mut fdc, 0, [0, 0, 0, 4, 3, 7, 1]);
    // On a fresh controller each selected physical head reads its own IDs.
    // This does not claim head-independent rotational phases or timed rotation.
    for drive in 0..4 {
        let mut head = Upd765a::new(SPECTRUM_128_CPU_HZ);
        head.insert_disk(usize::from(drive), source.clone());
        inspect(&mut head, drive | 4, [drive | 4, 0, 0, 19, 1, 7, 1]);
    }
    for drive in 0..4 {
        let live = fdc.disk_for_export(drive).expect("mounted image");
        assert_eq!(live.sides, source.sides);
        assert_eq!(live.tracks_per_side, source.tracks_per_side);
        for (source_side, live_side) in source.tracks.iter().zip(&live.tracks) {
            assert_eq!(source_side.len(), live_side.len());
            for (source_track, live_track) in source_side.iter().zip(live_side) {
                assert_eq!(source_track.sectors.len(), live_track.sectors.len());
                for (before, after) in source_track.sectors.iter().zip(&live_track.sectors) {
                    assert_eq!(
                        (
                            before.c, before.h, before.id, before.n, before.st1, before.st2
                        ),
                        (after.c, after.h, after.id, after.n, after.st1, after.st2)
                    );
                    assert_eq!(before.data, after.data, "Read ID cannot change payloads");
                }
            }
        }
    }
}
