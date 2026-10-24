//! Fully drained observations, not an oracle for disputed ST0/ST1/CHRN values.
//! Matrix and independent CRC audit: docs/plans/2026-10-19-read-track-drained-crc.
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn wait(fdc: &mut Upd765a, ticks: &mut usize) {
    while fdc.read_status() & 0x80 == 0 {
        *ticks += 1;
        assert!(*ticks < 2_000_000, "emulated controller deadline");
        fdc.tick();
    }
}

#[test]
fn fully_drained_crc_observations_preserve_order_and_recover() {
    for repeat in 1..=2 {
        for ids in [&[1_u8][..], &[1, 2][..], &[2, 1][..]] {
            for (kind, position) in [(0, 0), (1, 0), (2, 0), (1, 1), (2, 1)] {
                if ids.len() == 1 && position == 1 {
                    continue;
                }
                let image = DiskImage {
                    sides: 1,
                    tracks_per_side: 1,
                    tracks: vec![vec![DiskTrack {
                        sectors: ids
                            .iter()
                            .enumerate()
                            .map(|(i, &id)| DiskSector {
                                c: 0,
                                h: 0,
                                id,
                                n: 2,
                                st1: if kind != 0 && i == position { 0x20 } else { 0 },
                                st2: if kind == 2 && i == position { 0x20 } else { 0 },
                                data: vec![0x20 + id; 512],
                            })
                            .collect(),
                    }]],
                };
                let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
                fdc.insert_disk(0, image.clone());
                let mut ticks = 0;
                let command = [0x42, 0, 0, 0, 1, 2, ids.len() as u8, 0x2a, 0xff];
                for byte in command {
                    assert_eq!(
                        fdc.read_status(),
                        0x80 | if byte == 0x42 { 0 } else { 0x10 }
                    );
                    fdc.write_data(byte);
                }
                let mut payload = Vec::new();
                loop {
                    wait(&mut fdc, &mut ticks);
                    if fdc.read_status() & 0x20 == 0 {
                        break;
                    }
                    assert_eq!(fdc.read_status() & 0xe0, 0xe0);
                    assert!(payload.len() < ids.len() * 512, "excess payload");
                    payload.push(fdc.read_data());
                }
                let expected: Vec<_> = ids.iter().flat_map(|id| vec![0x20 + id; 512]).collect();
                assert_eq!(payload, expected, "physical order and complete payload");
                let mut result = [0; 7];
                for byte in &mut result {
                    assert_eq!(fdc.read_status(), 0xd0);
                    *byte = fdc.read_data();
                }
                assert_eq!(fdc.read_status(), 0x80);
                // A real subsequent command, not merely an idle-bit assertion.
                fdc.write_data(4);
                fdc.write_data(0);
                assert_eq!(fdc.read_status(), 0xd0);
                let recovery = fdc.read_data();
                assert_eq!(fdc.read_status(), 0x80);
                let after = fdc.disk_for_export(0).expect("source image");
                for (before, after) in image.tracks[0][0]
                    .sectors
                    .iter()
                    .zip(&after.tracks[0][0].sectors)
                {
                    assert_eq!(before.data, after.data);
                    assert_eq!(
                        (
                            before.c, before.h, before.id, before.n, before.st1, before.st2
                        ),
                        (after.c, after.h, after.id, after.n, after.st1, after.st2)
                    );
                }
                println!(
                    "{{\"repeat\":{repeat},\"ids\":{ids:?},\"kind\":{kind},\"position\":{position},\"command\":{command:?},\"data\":{payload:?},\"result\":{result:?},\"idle_msr\":128,\"recovery\":{recovery},\"recovery_msr\":128,\"ticks\":{ticks},\"media_unchanged\":true}}"
                );
            }
        }
    }
}
