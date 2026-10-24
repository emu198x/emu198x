//! The `.fds` layout against synthetic sides built here, so no commercial
//! disk image is needed.

use format_nintendo_famicom_fds::{
    BlockKind, DISK_SIGNATURE, FdsError, HEADER_LEN, SIDE_LEN, parse, write,
};

/// A side with a disk info block, a file amount of 1, one file header and
/// its data.
fn side(side_number: u8, data: &[u8]) -> Vec<u8> {
    let mut side = vec![0x01];
    side.extend_from_slice(&DISK_SIGNATURE);
    side.push(0xA4); // manufacturer
    side.extend_from_slice(b"TST");
    side.extend_from_slice(&[b' ', 0, side_number, 0, 0, 0, 0x0F]);
    side.resize(56, 0xFF);
    side.extend_from_slice(&[0x02, 1]);
    side.extend_from_slice(&[0x03, 0, 0x0F]);
    side.extend_from_slice(b"BOOTFILE");
    side.extend_from_slice(&0x6000u16.to_le_bytes());
    side.extend_from_slice(&u16::try_from(data.len()).expect("small file").to_le_bytes());
    side.push(0);
    side.push(0x04);
    side.extend_from_slice(data);
    side.resize(SIDE_LEN, 0);
    side
}

#[test]
fn a_headerless_file_is_whole_sides() {
    let mut file = side(0, b"abc");
    file.extend(side(1, b"de"));
    file.extend_from_slice(&[0; 100]); // a partial side is ignored
    let image = parse(&file).expect("parse");
    assert!(!image.has_header);
    assert_eq!(image.sides.len(), 2);
    assert_eq!(image.sides[1].bytes.len(), SIDE_LEN);
}

#[test]
fn a_header_declares_the_side_count() {
    let sides = [side(0, b"abc"), side(1, b"de")];
    let file = write(&sides, true);
    assert_eq!(file.len(), HEADER_LEN + 2 * SIDE_LEN);
    assert_eq!(&file[..5], b"FDS\x1A\x02");
    let image = parse(&file).expect("parse");
    assert!(image.has_header);
    assert_eq!(image.sides.len(), 2);
    assert_eq!(image.sides[0].bytes, sides[0].as_slice());
    assert_eq!(write(&[image.sides[0].bytes], false), sides[0]);
}

#[test]
fn bad_files_are_rejected() {
    assert_eq!(parse(&[0; 10]), Err(FdsError::TooShort { actual: 10 }));
    let mut file = write(&[side(0, b"")], true);
    file[4] = 2;
    assert_eq!(
        parse(&file),
        Err(FdsError::Truncated {
            declared: 2,
            actual: 1
        })
    );
    file[4] = 0;
    assert_eq!(parse(&file), Err(FdsError::NoSides));
}

#[test]
fn scanning_a_side_finds_each_block() {
    let bytes = side(1, b"hello");
    let image = parse(&bytes).expect("parse");
    let scan = image.sides[0].scan();
    let kinds: Vec<BlockKind> = scan.blocks.iter().map(|b| b.kind).collect();
    assert_eq!(
        kinds,
        [
            BlockKind::DiskInfo,
            BlockKind::FileAmount,
            BlockKind::FileHeader,
            BlockKind::FileData
        ]
    );
    let info = scan.blocks[0].disk_info().expect("disk info");
    assert!(info.signature_ok);
    assert_eq!(info.manufacturer, 0xA4);
    assert_eq!(&info.game_name, b"TST");
    assert_eq!(info.side_number, 1);
    assert_eq!(info.boot_file, 0x0F);
    assert_eq!(scan.blocks[1].file_amount(), Some(1));
    let header = scan.blocks[2].file_header().expect("file header");
    assert_eq!(&header.name, b"BOOTFILE");
    assert_eq!(header.address, 0x6000);
    assert_eq!(header.size, 5);
    assert_eq!(header.id, 0x0F);
    assert_eq!(scan.blocks[3].file_data(), Some(&b"hello"[..]));
    assert_eq!(scan.blocks[3].disk_info(), None);
    assert!(scan.rest.iter().all(|&b| b == 0));
    assert_eq!(scan.rest.len(), SIDE_LEN - (56 + 2 + 16 + 6));
}

#[test]
fn a_scan_stops_at_data_it_cannot_describe() {
    let mut bytes = side(0, b"x");
    // File data with no header before it.
    bytes[56 + 2 + 16 + 2] = 0x04;
    let image = parse(&bytes).expect("parse");
    let scan = image.sides[0].scan();
    assert_eq!(scan.blocks.len(), 4);
    assert_eq!(scan.rest[0], 0x04);
    // A file header whose data would run past the side.
    let mut bytes = side(0, b"x");
    bytes[56 + 2 + 13..56 + 2 + 15].copy_from_slice(&0xFFFFu16.to_le_bytes());
    let image = parse(&bytes).expect("parse");
    assert_eq!(image.sides[0].scan().blocks.len(), 3);
}
