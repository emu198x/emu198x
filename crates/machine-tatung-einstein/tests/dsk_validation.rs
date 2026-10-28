//! Malformed DSK input must fail before replacing a mounted disk.
use machine_tatung_einstein::Einstein;

fn disk(extended: bool) -> Vec<u8> {
    let mut d = vec![0; 0x600];
    let signature: &[u8] = if extended {
        b"EXTENDED CPC DSK File\r\nDisk-Info\r\n"
    } else {
        b"MV - CPCEMU Disk-File\r\nDisk-Info\r\n"
    };
    d[..signature.len()].copy_from_slice(signature);
    d[0x30] = 1;
    d[0x31] = 1;
    d[0x32..0x34].copy_from_slice(&0x500u16.to_le_bytes());
    d[0x34] = 5;
    d[0x100..0x10c].copy_from_slice(b"Track-Info\r\n");
    d[0x114] = 2;
    d[0x115] = 2;
    // Physical order is deliberately the opposite of sector-ID order.
    for (s, id) in [1, 0].into_iter().enumerate() {
        let e = 0x118 + s * 8;
        d[e + 2] = id;
        d[e + 3] = 2;
        d[e + 6..e + 8].copy_from_slice(&512u16.to_le_bytes());
        d[0x200 + s * 512..0x400 + s * 512].fill(0x40 + id);
    }
    d
}

#[test]
fn standard_and_extended_disks_keep_sector_id_order() {
    for extended in [false, true] {
        let mut machine = Einstein::new(vec![0; 8192]);
        machine
            .insert_cpc_dsk(0, &disk(extended))
            .expect("valid disk");
        let mounted = machine.disk(0).expect("mounted");
        assert_eq!(&mounted.data()[..512], &[0x40; 512]);
        assert_eq!(&mounted.data()[512..], &[0x41; 512]);
    }
}

#[test]
fn malformed_counts_sizes_and_missing_payload_are_rejected() {
    let mut cases = Vec::new();
    for (offset, value) in [(0x115, 255), (0x114, 64)] {
        let mut d = disk(true);
        d[offset] = value;
        cases.push(d);
    }
    let mut d = disk(true);
    d.truncate(0x200);
    cases.push(d);
    for d in cases {
        let parsed = std::panic::catch_unwind(|| Einstein::validate_cpc_dsk(&d));
        assert!(
            matches!(parsed, Ok(Err(_))),
            "malformed disk must return an error"
        );
    }
}

#[test]
fn rejected_disk_does_not_replace_live_media() {
    let mut machine = Einstein::new(vec![0; 8192]);
    machine.insert_cpc_dsk(0, &disk(true)).expect("valid disk");
    let before = machine.disk(0).expect("mounted").data().to_vec();
    for extended in [false, true] {
        for (offset, value) in [
            (0x30, 0),
            (0x31, 0),
            (0x31, 3),
            (0x34, 1),
            (0x100, 0),
            (0x110, 1),
            (0x111, 1),
            (0x114, 255),
            (0x115, 0),
            (0x115, 30),
            (0x115, 255),
            (0x11a, 0),
            (0x11a, 3),
            (0x11b, 3),
        ] {
            if !extended && offset == 0x34 {
                continue;
            }
            let mut d = disk(extended);
            d[offset] = value;
            assert!(
                machine.insert_cpc_dsk(0, &d).is_err(),
                "accepted {offset:x}={value}"
            );
            assert_eq!(machine.disk(0).expect("still mounted").data(), before);
        }
        for length in [0, 255, 256, 511, 512, 1023, 1535] {
            let mut d = disk(extended);
            d.truncate(length);
            assert!(
                machine.insert_cpc_dsk(0, &d).is_err(),
                "accepted length {length}"
            );
            assert_eq!(machine.disk(0).expect("still mounted").data(), before);
        }
    }
    for stored in [0u16, 1, 511, 513, 65535] {
        let mut d = disk(true);
        d[0x11e..0x120].copy_from_slice(&stored.to_le_bytes());
        assert!(machine.insert_cpc_dsk(0, &d).is_err());
        assert_eq!(machine.disk(0).expect("still mounted").data(), before);
    }
}

#[test]
fn every_sector_size_code_is_bounded_by_the_supplied_payload() {
    for size_code in 0..=u8::MAX {
        let mut d = disk(true);
        d[0x114] = size_code;
        if size_code != 2 {
            assert!(
                Einstein::validate_cpc_dsk(&d).is_err(),
                "accepted N={size_code}"
            );
        }
    }
}

#[test]
fn sector_status_and_nonphysical_address_marks_are_not_discarded() {
    let mut machine = Einstein::new(vec![0; 8192]);
    machine.insert_cpc_dsk(0, &disk(true)).expect("valid disk");
    let before = machine.disk(0).expect("mounted").data().to_vec();
    for extended in [false, true] {
        for (offset, value) in [(0x118, 1), (0x119, 1), (0x11c, 0x20), (0x11d, 0x40)] {
            let mut image = disk(extended);
            image[offset] = value;
            assert!(
                machine.insert_cpc_dsk(0, &image).is_err(),
                "discarded metadata at {offset:x}"
            );
            assert_eq!(
                machine.disk(0).expect("original remains mounted").data(),
                before
            );
        }
    }
}

#[test]
fn physical_track_headers_can_differ_from_container_order() {
    for extended in [false, true] {
        let mut image = disk(extended);
        let second = image[256..].to_vec();
        image.extend_from_slice(&second);
        image[0x30] = 2;
        image[0x35] = 5;
        for (slot, physical) in [(0, 1u8), (1, 0u8)] {
            let base = 256 + slot * 1280;
            image[base + 0x10] = physical;
            for record in 0..2usize {
                image[base + 0x18 + record * 8] = physical;
            }
            image[base + 256..base + 1280].fill(0xB0 + physical);
        }
        let mut machine = Einstein::new(vec![0; 8192]);
        machine.insert_cpc_dsk(0, &image).expect("reordered tracks");
        let mounted = machine.disk(0).expect("mounted").data();
        assert_eq!(&mounted[..1024], &[0xB0; 1024]);
        assert_eq!(&mounted[1024..], &[0xB1; 1024]);
        // Preserve the existing compatibility convention for a zeroed TIB.
        image[256..512].fill(0);
        machine
            .insert_cpc_dsk(0, &image)
            .expect("zeroed track header");
        let mounted = machine.disk(0).expect("mounted").data();
        assert_eq!(&mounted[..1024], &[0xB0; 1024]);
        assert_eq!(&mounted[1024..], &[0; 1024]);
    }
}
