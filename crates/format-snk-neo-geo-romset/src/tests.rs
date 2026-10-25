//! ROM-set assembly tests on synthetic files.

#![allow(clippy::unwrap_used)]

use std::io::Write;

use super::*;

fn file(name: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    (name.to_owned(), bytes.to_vec())
}

#[test]
fn names_classify_by_extension_or_stem() {
    assert_eq!(classify("201-p1.p1"), Some(('p', 1)));
    assert_eq!(classify("game/201-C12.C12"), Some(('c', 12)));
    assert_eq!(classify("201-v21.bin"), Some(('v', 21)));
    assert_eq!(classify("m1.bin"), Some(('m', 1)));
    assert_eq!(classify("x_m1.rom"), Some(('m', 1)));
    assert_eq!(classify("sfix.sfix"), None);
    assert_eq!(classify("000-lo.lo"), None);
    assert_eq!(classify("readme.txt"), None);
    assert_eq!(classify("p0.p0"), None);
    assert_eq!(classify("c123.c123"), None);
}

#[test]
fn program_roms_are_word_swapped_and_joined_in_order() {
    let set = from_files([file("x-p2.p2", &[5, 6]), file("x-p1.p1", &[1, 2, 3, 4])]).unwrap();
    assert_eq!(set.p, [2, 1, 4, 3, 6, 5]);
}

#[test]
fn sprite_roms_interleave_by_pair() {
    let set = from_files([
        file("p1.p1", &[0, 0]),
        file("c1.c1", &[1, 3]),
        file("c2.c2", &[2, 4]),
        file("c3.c3", &[5]),
        file("c4.c4", &[6]),
    ])
    .unwrap();
    assert_eq!(set.c, [1, 2, 3, 4, 5, 6]);
}

#[test]
fn sample_roms_split_by_region_when_two_digits_say_so() {
    let set = from_files([
        file("p1.p1", &[0, 0]),
        file("v2.v2", &[2]),
        file("v1.v1", &[1]),
    ])
    .unwrap();
    assert_eq!(set.v_a, [1, 2]);
    assert!(set.v_b.is_empty());

    let set = from_files([
        file("p1.p1", &[0, 0]),
        file("v11.v11", &[1]),
        file("v21.v21", &[2]),
        file("v22.v22", &[3]),
    ])
    .unwrap();
    assert_eq!(set.v_a, [1]);
    assert_eq!(set.v_b, [2, 3]);
}

#[test]
fn fix_and_sound_roms_are_taken_whole() {
    let set = from_files([
        file("p1.p1", &[0, 0]),
        file("s1.s1", &[7, 8]),
        file("m1.m1", &[9]),
    ])
    .unwrap();
    assert_eq!(set.s, [7, 8]);
    assert_eq!(set.m, [9]);
}

#[test]
fn errors_name_what_is_wrong() {
    assert_eq!(
        from_files([file("s1.s1", &[0])]),
        Err(RomSetError::NoProgramRom)
    );
    assert!(matches!(
        from_files([file("a-p1.p1", &[0, 0]), file("b-p1.bin", &[0, 0])]),
        Err(RomSetError::Duplicate {
            role: 'p',
            index: 1,
            ..
        })
    ));
    assert_eq!(
        from_files([file("p1.p1", &[0, 0]), file("c1.c1", &[1])]),
        Err(RomSetError::UnpairedSpriteRom { index: 1 })
    );
    assert_eq!(
        from_files([
            file("p1.p1", &[0, 0]),
            file("c1.c1", &[1]),
            file("c2.c2", &[1, 2])
        ]),
        Err(RomSetError::UnpairedSpriteRom { index: 1 })
    );
    assert_eq!(
        from_files([
            file("p1.p1", &[0, 0]),
            file("c1.c1", &[1]),
            file("c3.c3", &[1])
        ]),
        Err(RomSetError::UnpairedSpriteRom { index: 1 })
    );
    assert!(RomSetError::NoProgramRom.to_string().contains("P-ROM"));
}

#[test]
fn a_zip_archive_reads_like_its_files() {
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in [
            ("game/1-p1.p1", &[1u8, 2][..]),
            ("game/1-c1.c1", &[3]),
            ("game/1-c2.c2", &[4]),
            ("readme.txt", b"hello"),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.add_directory("empty/", options).unwrap();
        writer.finish().unwrap();
    }
    let bytes = buffer.into_inner();
    assert!(is_zip(&bytes));
    let set = from_zip(&bytes).unwrap();
    assert_eq!(set.p, [2, 1]);
    assert_eq!(set.c, [3, 4]);
    assert!(!is_zip(b"NEO"));
    assert!(matches!(
        from_zip(b"PK\x03\x04junk"),
        Err(RomSetError::Zip(_))
    ));
}
