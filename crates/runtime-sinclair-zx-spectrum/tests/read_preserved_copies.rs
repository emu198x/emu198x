//! EDSK Rev5 integral multiple-sized fields contain preserved versions.
//! Deterministic copy rotation follows accepted marginal-encoding-model policy,
//! not a claim about physical flux. No ROM execution is claimed by this test.
use common_sinclair_zx_spectrum_amstrad_class::{Plus3Marker, SpectrumAmstradClassCore};
type Core = SpectrumAmstradClassCore<Plus3Marker>;

fn sample(head: u8, copy: u8) -> Vec<u8> {
    (0..256)
        .map(|i| (i as u8).wrapping_add(0x20 + head * 0x40 + copy * 0x10))
        .collect()
}
fn edsk(deleted: bool, crc: bool) -> Vec<u8> {
    let mut bytes = vec![0; 256 + 2 * 1280];
    bytes[..34].copy_from_slice(b"EXTENDED CPC DSK File\r\nDisk-Info\r\n");
    bytes[0x30] = 1;
    bytes[0x31] = 2;
    bytes[0x34] = 5;
    bytes[0x35] = 5;
    for head in 0..2u8 {
        let base = 256 + usize::from(head) * 1280;
        bytes[base..base + 12].copy_from_slice(b"Track-Info\r\n");
        bytes[base + 0x11] = head;
        bytes[base + 0x14] = 1;
        bytes[base + 0x15] = 2;
        bytes[base + 0x16] = 0x2a;
        bytes[base + 0x17] = 0xe5;
        for (index, r) in [1, 2].into_iter().enumerate() {
            let s = base + 0x18 + 8 * index;
            bytes[s..s + 8].copy_from_slice(&[
                0,
                head,
                r,
                1,
                if crc && r == 1 { 0x20 } else { 0 },
                (if deleted { 0x40 } else { 0 }) | if crc && r == 1 { 0x20 } else { 0 },
                0,
                if r == 1 { 3 } else { 1 },
            ]);
        }
        for copy in 0..3 {
            let start = base + 256 + usize::from(copy) * 256;
            bytes[start..start + 256].copy_from_slice(&sample(head, copy));
        }
        bytes[base + 1024..base + 1280].fill(0x91 + head);
    }
    bytes
}
fn core(raw: &[u8]) -> Core {
    let mut core = Core::new();
    let image = format_amstrad_dsk::parse(raw).expect("valid authored EDSK");
    assert_eq!((image.sides, image.tracks_per_side), (2, 1));
    for h in 0..2 {
        let s = &image.tracks[h][0].sectors;
        assert_eq!((s.len(), s[0].data.len(), s[1].data.len()), (2, 768, 256));
    }
    core.fdc.insert_disk(0, image);
    core
}
fn command(c: &mut Core, op: u8, select: u8, r: u8) {
    assert_eq!(c.fdc.read_status(), 0x80);
    for b in [op, select, 0, (select >> 2) & 1, r, 1, r, 0x2a, 0xff] {
        c.fdc.write_data(b);
    }
}
fn await_read(c: &mut Core) {
    for _ in 0..10_000 {
        if c.fdc.read_status() & 0x80 != 0 {
            return;
        }
        assert_eq!(c.fdc.read_status(), 0x70);
        c.fdc.tick();
    }
    panic!("read deadline");
}
fn data(c: &mut Core, expected: &[u8]) {
    for b in expected {
        await_read(c);
        assert_eq!(c.fdc.read_status(), 0xf0);
        assert_eq!(c.fdc.read_data(), *b, "verbatim preserved copy");
    }
}
fn result(c: &mut Core) -> [u8; 7] {
    let mut result = [0; 7];
    for b in &mut result {
        assert_eq!(c.fdc.read_status(), 0xd0);
        *b = c.fdc.read_data();
    }
    assert_eq!(c.fdc.read_status(), 0x80);
    result
}
fn read(c: &mut Core, op: u8, select: u8, copy: u8, crc: bool) {
    command(c, op, select, 1);
    data(c, &sample((select >> 2) & 1, copy));
    assert_eq!(
        result(c),
        [
            0x40 | select,
            if crc { 0x20 } else { 0x80 },
            if crc { 0x20 } else { 0 },
            0,
            (select >> 2) & 1,
            1,
            1
        ]
    );
}
#[test]
fn parsed_edsk_rotates_all_copies_verbatim_and_never_mutates_media() {
    for (op, deleted) in [(0x46, false), (0x4c, true)] {
        for crc in [false, true] {
            let raw = edsk(deleted, crc);
            let original = raw.clone();
            let mut c = core(&raw);
            for copy in [0, 1, 2, 0, 1, 2] {
                read(&mut c, op, 0, copy, crc);
            }
            let live = c.fdc.disk_for_export(0).expect("live disk");
            assert_eq!(
                format_amstrad_dsk::encode_extended(live).expect("encode"),
                format_amstrad_dsk::encode_extended(
                    &format_amstrad_dsk::parse(&raw).expect("original")
                )
                .expect("encode")
            );
            assert_eq!(raw, original);
        }
    }
}
#[test]
fn switching_physical_key_restarts_the_existing_consecutive_read_sequence() {
    let raw = edsk(false, false);
    let mut c = core(&raw);
    c.fdc
        .insert_disk(1, format_amstrad_dsk::parse(&raw).expect("second drive"));
    read(&mut c, 0x46, 0, 0, false);
    read(&mut c, 0x46, 2, 1, false); // logical alias
    read(&mut c, 0x46, 1, 0, false);
    read(&mut c, 0x46, 1, 1, false);
    read(&mut c, 0x46, 0, 0, false);
    read(&mut c, 0x46, 4, 0, false);
    read(&mut c, 0x46, 4, 1, false);
    read(&mut c, 0x46, 0, 0, false);
    command(&mut c, 0x46, 0, 2);
    data(&mut c, &[0x91; 256]);
    assert_eq!(result(&mut c), [0x40, 0x80, 0, 0, 0, 2, 1]);
    read(&mut c, 0x46, 0, 0, false);
}
#[test]
fn prepared_but_unvisited_field_does_not_consume_a_copy() {
    let raw = edsk(false, false);
    let mut c = core(&raw);
    read(&mut c, 0x46, 0, 0, false);
    let before = serde_json::to_value(&c.fdc).expect("prior counter");
    command(&mut c, 0x46, 0, 1);
    let pending = serde_json::to_value(&c.fdc).expect("prepared counter");
    assert_eq!(
        pending["reread_count"], before["reread_count"],
        "preparing does not visit the field"
    );
    assert_eq!(pending["reread_key"], before["reread_key"]);
    let bytes = postcard::to_allocvec(&c.fdc).expect("prepared checkpoint");
    let mut restored = Core::new();
    restored.fdc = postcard::from_bytes(&bytes).expect("restore prepared");
    restored.fdc.after_restore();
    data(&mut restored, &sample(0, 1));
    assert_eq!(result(&mut restored), [0x40, 0x80, 0, 0, 0, 1, 1]);
    read(&mut restored, 0x46, 0, 2, false);
}
#[test]
fn visited_interrupted_field_consumes_one_copy_without_generating_bytes() {
    let raw = edsk(false, true);
    let mut c = core(&raw);
    for consumed in [0, 17, 255] {
        command(&mut c, 0x46, 0, 1);
        data(&mut c, &sample(0, 0)[..consumed]);
        await_read(&mut c);
        for _ in 0..114 {
            c.fdc.tick();
        }
        assert_eq!(result(&mut c), [0x40, 0x10, 0, 0, 0, 1, 1]);
        read(&mut c, 0x46, 0, 1, true);
        command(&mut c, 0x46, 0, 2);
        data(&mut c, &[0x91; 256]);
        result(&mut c);
    }
}
#[test]
fn component_restore_preserves_selected_copy_and_next_rotation() {
    for (consumed, ready) in [
        (0, false),
        (0, true),
        (1, false),
        (127, false),
        (127, true),
        (255, false),
        (256, false),
    ] {
        let raw = edsk(false, true);
        let mut c = core(&raw);
        read(&mut c, 0x46, 0, 0, true);
        command(&mut c, 0x46, 0, 1);
        data(&mut c, &sample(0, 1)[..consumed]);
        if ready {
            await_read(&mut c);
        }
        let bytes = postcard::to_allocvec(&c.fdc).expect("serialize controller");
        let mut restored = Core::new();
        restored.fdc = postcard::from_bytes(&bytes).expect("restore controller");
        restored.fdc.after_restore();
        assert_eq!(
            postcard::to_allocvec(&restored.fdc).expect("roundtrip"),
            bytes
        );
        for controller in [&mut c, &mut restored] {
            data(controller, &sample(0, 1)[consumed..]);
            assert_eq!(result(controller), [0x40, 0x20, 0x20, 0, 0, 1, 1]);
            read(controller, 0x46, 0, 2, true);
        }
        assert_eq!(
            postcard::to_allocvec(&c.fdc).expect("final"),
            postcard::to_allocvec(&restored.fdc).expect("restored final")
        );
    }
}

#[test]
fn skipped_mark_does_not_visit_preserved_versions() {
    let raw = edsk(false, false);
    let mut c = core(&raw);
    read(&mut c, 0x46, 0, 0, false);
    let before = serde_json::to_value(&c.fdc).expect("visited counter");
    command(&mut c, 0x6c, 0, 1);
    assert_eq!(result(&mut c), [0x40, 4, 0, 0, 0, 1, 1]);
    let after = serde_json::to_value(&c.fdc).expect("skipped counter");
    assert_eq!(after["reread_count"], before["reread_count"]);
    assert_eq!(after["reread_key"], before["reread_key"]);
    read(&mut c, 0x46, 0, 1, false);
}

#[test]
fn read_track_and_nonintegral_overlength_do_not_rotate_as_complete_versions() {
    let raw = edsk(false, false);
    let mut c = core(&raw);
    for _ in 0..2 {
        command(&mut c, 0x42, 0, 1);
        data(&mut c, &sample(0, 0));
        assert_eq!(result(&mut c), [0x40, 0x80, 0, 0, 0, 1, 1]);
    }
    let mut parsed = format_amstrad_dsk::parse(&raw).expect("original");
    parsed.tracks[0][0].sectors[0].data.push(0xaa);
    let nonintegral =
        format_amstrad_dsk::encode_extended(&parsed).expect("valid nonintegral stored length");
    let mut c = Core::new();
    c.fdc.insert_disk(
        0,
        format_amstrad_dsk::parse(&nonintegral).expect("nonintegral image"),
    );
    for _ in 0..3 {
        read(&mut c, 0x46, 0, 0, false);
    }
}
