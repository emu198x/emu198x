//! Format's recorded IDs and payload lengths have independent sources.
//! NEC December 1978, printed p14, Format A Track. Component-only evidence.
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn old_track() -> DiskTrack {
    DiskTrack {
        sectors: (1..=12)
            .map(|id| DiskSector {
                c: 0,
                h: 0,
                id,
                n: 2,
                st1: 0x20,
                st2: 0x60,
                data: vec![0xa5; 512],
            })
            .collect(),
    }
}

fn wait(fdc: &mut Upd765a) {
    for _ in 0..1_000_000 {
        if fdc.read_status() & 0x80 != 0 {
            return;
        }
        fdc.tick();
    }
    panic!("bounded Format request/result wait exceeded");
}

#[test]
fn format_success_preserves_authored_ids_and_command_sized_filler() {
    let mut cases = 0;
    for nd in [false, true] {
        // These are component cases, not additional +3 configurations.
        for (opcode, n, standard) in (0..=5)
            .map(|n| (0x4d, n, false))
            .chain((0..=4).map(|n| (0x0d, n, false)))
            .chain([(0x4d, 2, true)])
        {
            let ids: Vec<[u8; 4]> = if standard {
                [9, 1, 8, 2, 7, 3, 6, 4, 5].map(|r| [0, 0, r, 2]).to_vec()
            } else {
                vec![[7, 8, 255, 3]]
            };
            let count = if standard { 9 } else { 1 };
            let filler = if standard { 0xe5 } else { 0x69 };
            let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
            fdc.insert_disk_with_writable(
                0,
                DiskImage {
                    sides: 1,
                    tracks_per_side: 2,
                    tracks: vec![vec![old_track(), old_track()]],
                },
                true,
            );
            for byte in [3, 0xdf, 2 | u8::from(nd)] {
                fdc.write_data(byte);
            }
            for byte in [opcode, 0, n, count, if standard { 82 } else { 42 }, filler] {
                fdc.write_data(byte);
            }
            assert_eq!(fdc.read_status(), 0x10 | if nd { 0x20 } else { 0 });
            assert!(!fdc.interrupt);
            assert!(fdc.disk_for_export(0).is_err(), "active export refused");
            for &byte in ids.iter().flatten() {
                wait(&mut fdc);
                assert_eq!(fdc.read_status(), 0x90 | if nd { 0x20 } else { 0 });
                assert_eq!(fdc.interrupt, nd, "byte request IRQ follows ND");
                assert_eq!(fdc.read_data(), 0xff, "read cannot consume a write request");
                fdc.write_data(byte);
                assert_eq!(fdc.read_status() & 0x80, 0, "each byte needs a new request");
                assert!(!fdc.interrupt);
            }
            wait(&mut fdc);
            assert_eq!(fdc.read_status(), 0xd0);
            assert!(fdc.interrupt);
            let last = ids.last().expect("nonempty authored ID list");
            let expected = [0, 0, 0, last[0], last[1], last[2].wrapping_add(1), last[3]];
            for byte in expected {
                assert_eq!(fdc.read_status(), 0xd0);
                assert_eq!(fdc.read_data(), byte, "exact Format result");
                assert!(!fdc.interrupt, "first result byte acknowledges IRQ");
            }
            assert_eq!(fdc.read_status(), 0x80);
            let image = fdc.disk_for_export(0).expect("completed Format exportable");
            let sectors = &image.tracks[0][0].sectors;
            assert_eq!(sectors.len(), ids.len(), "old trailing sectors removed");
            for (sector, id) in sectors.iter().zip(&ids) {
                assert_eq!([sector.c, sector.h, sector.id, sector.n], *id);
                assert_eq!(
                    (sector.st1, sector.st2),
                    (0, 0),
                    "old CRC/deleted flags cleared"
                );
                assert_eq!(sector.data, vec![filler; 128_usize << n]);
            }
            let untouched = &image.tracks[0][1].sectors;
            assert_eq!(untouched.len(), 12);
            for (sector, id) in untouched.iter().zip(1..=12) {
                assert_eq!([sector.c, sector.h, sector.id, sector.n], [0, 0, id, 2]);
                assert_eq!((sector.st1, sector.st2), (0x20, 0x60));
                assert_eq!(sector.data, vec![0xa5; 512]);
            }
            // A new, unrelated command must be accepted after every result drains.
            fdc.write_data(0xff);
            assert_eq!(fdc.read_data(), 0x80);
            assert_eq!(fdc.read_status(), 0x80);
            cases += 1;
        }
    }
    assert_eq!(cases, 24, "finite component case count");
    eprintln!("FORMAT_SUCCESS_CASES={cases}");
}
