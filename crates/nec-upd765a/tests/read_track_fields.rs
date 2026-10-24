//! Bounded field observations for #1870, not an oracle for disputed results.
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

/// The +3's CPU clock, which these cases were written against: the +3
/// ticks the controller once per T-state.
const SPECTRUM_128_CPU_HZ: u64 = 3_546_895;

fn numbers(line: &str) -> Vec<u8> {
    line.split_whitespace()
        .map(|s| s.parse().expect("fixture byte"))
        .collect()
}

#[test]
fn field_boundary_observations() {
    // Default remains a real assertion-bearing smoke test under ordinary cargo test.
    let input = std::env::var("READ_TRACK_FIELDS")
        .map(|p| std::fs::read_to_string(p).expect("fixture file"))
        .unwrap_or_else(|_| "smoke\n66 0 0 0 1 0 1 42 255\n1\n0 0 1 0 128 33\n".into());
    let mut lines = input.lines();
    let name = lines.next().expect("case name");
    let command = numbers(lines.next().expect("command"));
    assert_eq!(command.len(), 9);
    let mode: u8 = lines
        .next()
        .expect("media mode")
        .parse()
        .expect("media mode byte");
    assert!(
        mode <= 2,
        "0 absent, 1 represented track, 2 missing track entry"
    );
    let present = mode != 0;
    let sectors: Vec<_> = lines
        .map(|line| {
            let n: Vec<usize> = line
                .split_whitespace()
                .map(|s| s.parse().expect("sector number"))
                .collect();
            assert_eq!(n.len(), 6);
            DiskSector {
                c: n[0] as u8,
                h: n[1] as u8,
                id: n[2] as u8,
                n: n[3] as u8,
                st1: 0,
                st2: 0,
                data: vec![n[5] as u8; n[4]],
            }
        })
        .collect();
    let image = DiskImage {
        sides: 1,
        tracks_per_side: 1,
        tracks: vec![if mode == 2 {
            vec![]
        } else {
            vec![DiskTrack { sectors }]
        }],
    };
    let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
    if present {
        fdc.insert_disk(0, image.clone());
    }
    let mut command_bytes_sent = 0;
    for byte in &command {
        if fdc.read_status() & 0xc0 == 0xc0 {
            break;
        }
        assert_eq!(fdc.read_status() & 0xc0, 0x80);
        fdc.write_data(*byte);
        command_bytes_sent += 1;
    }
    let mut data = Vec::new();
    let mut result = Vec::new();
    let mut ticks = 0;
    loop {
        assert!(ticks < 2_000_000, "emulated deadline");
        let msr = fdc.read_status();
        if !result.is_empty() && msr == 0x80 {
            break;
        }
        if msr & 0xc0 == 0xc0 {
            let byte = fdc.read_data();
            if msr & 0x20 != 0 {
                assert!(data.len() < 65536);
                data.push(byte);
            } else {
                assert!(result.len() < 7);
                result.push(byte);
            }
        }
        fdc.tick();
        ticks += 1;
    }
    assert!(
        (command_bytes_sent == 9 && result.len() == 7)
            || (command_bytes_sent == 1 && result == [0x80] && data.is_empty())
    );
    assert_eq!(fdc.read_status(), 0x80);
    fdc.write_data(4);
    fdc.write_data(0);
    assert_eq!(fdc.read_status(), 0xd0);
    let recovery = fdc.read_data();
    assert_eq!(fdc.read_status(), 0x80);
    if present {
        let after = fdc.disk_for_export(0).expect("source disk");
        assert_eq!(
            format!("{image:?}"),
            format!("{after:?}"),
            "immutable source disk"
        );
    } else {
        assert!(fdc.disk_for_export(0).is_err());
    }
    if name == "smoke" {
        assert_eq!(data, vec![33; 128]);
        assert_eq!(result, [64, 128, 0, 0, 0, 1, 0]);
    }
    println!(
        "{{\"data\":{data:?},\"result\":{result:?},\"command_bytes_sent\":{command_bytes_sent},\"idle_msr\":128,\"recovery\":{recovery},\"recovery_msr\":128,\"ticks\":{ticks},\"media_unchanged\":true}}"
    );
}

#[test]
fn missing_track_entry_is_present_media_and_recovers() {
    for opcode in [0x42, 0x62, 0xc2, 0xe2] {
        let mut fdc = Upd765a::new(SPECTRUM_128_CPU_HZ);
        fdc.insert_disk(
            0,
            DiskImage {
                sides: 1,
                tracks_per_side: 1,
                tracks: vec![vec![]],
            },
        );
        for byte in [opcode, 0, 0, 0, 1, 2, 1, 42, 255] {
            fdc.write_data(byte);
        }
        // A present image with no track is MA, not the absent-drive NR result.
        let mut result = Vec::new();
        for _ in 0..7 {
            assert_eq!(fdc.read_status(), 0xd0);
            result.push(fdc.read_data());
        }
        assert_eq!(result, [0x40, 1, 0, 0, 0, 1, 2]);
        assert_eq!(fdc.read_status(), 0x80);
        fdc.write_data(4);
        fdc.write_data(0);
        assert_eq!(fdc.read_status(), 0xd0);
        let _ = fdc.read_data();
        assert_eq!(fdc.read_status(), 0x80);
        assert!(fdc.disk_for_export(0).expect("present media").tracks[0].is_empty());
    }
}
