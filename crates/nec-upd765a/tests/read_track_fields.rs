//! Bounded field observations for #1870, not an oracle for disputed results.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::{DiskImage, DiskSector, DiskTrack, Upd765a};

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
    let present = lines.next().expect("media presence") == "1";
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
        tracks: vec![vec![DiskTrack { sectors }]],
    };
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    if present {
        fdc.insert_disk(0, image.clone());
    }
    for byte in &command {
        assert_eq!(fdc.read_status() & 0xc0, 0x80);
        fdc.write_data(*byte);
    }
    let mut data = Vec::new();
    let mut result = Vec::new();
    let mut ticks = 0;
    while result.len() < 7 {
        assert!(ticks < 2_000_000, "emulated deadline");
        let msr = fdc.read_status();
        if msr & 0xc0 == 0xc0 {
            let byte = fdc.read_data();
            if msr & 0x20 != 0 {
                assert!(data.len() < 65536);
                data.push(byte);
            } else {
                result.push(byte);
            }
        }
        fdc.tick(0);
        ticks += 1;
    }
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
        "{{\"data\":{data:?},\"result\":{result:?},\"idle_msr\":128,\"recovery\":{recovery},\"recovery_msr\":128,\"ticks\":{ticks},\"media_unchanged\":true}}"
    );
}
