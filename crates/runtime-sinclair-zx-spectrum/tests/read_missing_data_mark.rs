//! Authored EDSK missing-DAM metadata must stop Data/Deleted before payload.
//! Direct controller composition: not ROM loading or physical timing evidence.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;

fn edsk(stored_missing_bytes: usize, deleted: bool) -> Vec<u8> {
    let track_bytes = 256 + stored_missing_bytes + 256;
    let mut source = vec![0; 256 + track_bytes];
    let signature = b"EXTENDED CPC DSK File\r\nDisk-Info\r\n";
    source[..signature.len()].copy_from_slice(signature);
    source[0x22..0x30].copy_from_slice(b"198x MD test  ");
    source[0x30] = 1;
    source[0x31] = 1;
    source[0x34] = (track_bytes / 256) as u8;
    source[256..268].copy_from_slice(b"Track-Info\r\n");
    source[256 + 0x13] = 2; // MFM
    source[256 + 0x14] = 1;
    source[256 + 0x15] = 2;
    source[256 + 0x16] = 0x2a;
    source[256 + 0x17] = 0xe5;
    for (index, length) in [stored_missing_bytes, 256].into_iter().enumerate() {
        let at = 256 + 0x18 + index * 8;
        source[at..at + 4].copy_from_slice(&[7, 3, index as u8 + 1, 1]);
        source[at + 4] = u8::from(index == 0); // canonical ST1.MA
        source[at + 5] = if index == 0 {
            1
        } else if deleted {
            0x40
        } else {
            0
        };
        source[at + 6..at + 8].copy_from_slice(&(length as u16).to_le_bytes());
    }
    source[512..512 + stored_missing_bytes].fill(0xa5);
    source[512 + stored_missing_bytes..].fill(0x69);
    source
}

fn read(machine: &mut SpectrumPlus3, opcode: u8, selected: u8, r: u8) -> (Vec<u8>, [u8; 7]) {
    let fdc = &mut machine.fdc;
    assert_eq!(fdc.read_status(), 0x80, "idle before command");
    for byte in [opcode, selected, 7, 3, r, 1, 2, 0x2a, 0xff] {
        fdc.write_data(byte);
    }
    let mut payload = Vec::new();
    while fdc.read_status() & 0x20 != 0 {
        let mut requested = false;
        for _ in 0..10_000 {
            if fdc.read_status() & 0x80 != 0 {
                requested = true;
                break;
            }
            assert_eq!(fdc.read_status() & 0x70, 0x70);
            fdc.tick(0);
        }
        assert!(requested, "bounded read request");
        assert_eq!(fdc.read_status(), 0xf0);
        assert!(payload.len() < 1024, "bounded host payload");
        payload.push(fdc.read_data());
    }
    let result = std::array::from_fn(|_| {
        assert_eq!(fdc.read_status(), 0xd0, "all seven result bytes");
        fdc.read_data()
    });
    assert_eq!(fdc.read_status(), 0x80, "result drains to idle");
    (payload, result)
}

fn missing_mark(deleted: bool) {
    for stored in [0, 256] {
        let source = edsk(stored, deleted);
        let image = format_amstrad_dsk::parse(&source).expect("valid authored EDSK");
        assert_eq!(image.tracks[0][0].sectors[0].st1, 1);
        assert_eq!(image.tracks[0][0].sectors[0].st2, 1);
        assert_eq!(image.tracks[0][0].sectors[0].data.len(), stored);
        let live_before = format_amstrad_dsk::encode_extended(&image).expect("live baseline");
        for selected in [0, 2] {
            for sk in [false, true] {
                let mut machine = SpectrumPlus3::new();
                machine.fdc.insert_disk(0, image.clone());
                machine.fdc.set_ready(0, true);
                let opcode = if deleted { 0x4c } else { 0x46 } | if sk { 0x20 } else { 0 };
                let (payload, result) = read(&mut machine, opcode, selected, 1);
                assert!(
                    payload.is_empty(),
                    "missing DAM must not deliver stored or invented bytes: deleted={deleted} stored={stored} SK={sk} selected={selected} count={}",
                    payload.len()
                );
                assert_eq!(result, [0x40 | selected, 1, 1, 7, 3, 1, 1]);
                let (payload, result) = read(&mut machine, opcode, selected, 2);
                assert_eq!(
                    payload,
                    vec![0x69; 256],
                    "next command recovers exact good payload"
                );
                assert_eq!(result, [0x40 | selected, 0x80, 0, 7, 3, 2, 1]);
                let live = machine
                    .fdc
                    .disk_for_export(0)
                    .expect("read preserves mounted image");
                assert_eq!(
                    format_amstrad_dsk::encode_extended(live).expect("live image"),
                    live_before
                );
            }
        }
    }
}

#[test]
fn read_data_missing_dam_has_no_payload_and_recovers() {
    missing_mark(false);
}

#[test]
fn read_deleted_missing_dam_has_no_payload_and_recovers() {
    missing_mark(true);
}
