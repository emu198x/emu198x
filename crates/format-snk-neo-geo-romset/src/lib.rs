//! ZIP adapter for Neo Geo named-file cartridge sets.

use std::io::{Cursor, Read};

pub use format198x_snk_neo_geo_romset::RomSet;
use format198x_snk_neo_geo_romset::from_files;
use thiserror::Error;

/// Why a Neo Geo cartridge archive could not be loaded.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RomSetError {
    /// The ZIP archive or one of its entries could not be read.
    #[error("not a readable zip archive: {0}")]
    Zip(String),
    /// The named files do not form a valid cartridge ROM set.
    #[error(transparent)]
    Layout(#[from] format198x_snk_neo_geo_romset::RomSetError),
}

/// Build a ROM set from a ZIP archive's bytes, as MAME keeps one game.
///
/// # Errors
///
/// Returns an error if the archive cannot be read or its named files do
/// not form a valid ROM set.
pub fn from_zip(bytes: &[u8]) -> Result<RomSet, RomSetError> {
    let zip_error = |err: zip::result::ZipError| RomSetError::Zip(err.to_string());
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        if entry.is_dir() {
            continue;
        }
        // The format crate alone decides which names represent ROMs.
        let name = entry.name().map_err(zip_error)?.into_owned();
        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|err| RomSetError::Zip(err.to_string()))?;
        files.push((name, contents));
    }
    Ok(from_files(files)?)
}

/// Whether `bytes` start like a ZIP archive.
#[must_use]
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use format198x_snk_neo_geo_romset::RomSetError as LayoutError;

    use super::{Cursor, RomSetError, from_zip, is_zip};

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for &(name, bytes) in files {
            writer.start_file(name, options).expect("entry");
            writer.write_all(bytes).expect("contents");
        }
        writer.add_directory("empty/", options).expect("directory");
        writer.finish().expect("ZIP").into_inner()
    }

    #[test]
    fn a_zip_archive_reads_like_its_files() {
        let bytes = archive(&[
            ("game/1-p1.p1", &[1, 2]),
            ("game/1-c1.c1", &[3]),
            ("game/1-c2.c2", &[4]),
            ("readme.txt", b"hello"),
        ]);
        assert!(is_zip(&bytes));
        let set = from_zip(&bytes).expect("ROM set");
        assert_eq!(set.p, [2, 1]);
        assert_eq!(set.c, [3, 4]);
        assert!(!is_zip(b"NEO"));
    }

    #[test]
    fn a_zip_archive_keeps_samurai_shodowns_second_program_rom() {
        const BANK: usize = 1024 * 1024;
        let mut p1 = vec![0; BANK];
        p1[0x100..0x108].copy_from_slice(b"NEO-GEO\0");
        for word in p1.as_chunks_mut::<2>().0 {
            word.swap(0, 1);
        }
        let p2 = [0x12, 0x34].repeat(BANK / 2);

        let bytes = archive(&[("045-pg2.sp2", &p2), ("045-p1.p1", &p1)]);
        let set = from_zip(&bytes).expect("ROM set");
        assert_eq!(set.p.len(), 2 * BANK);
        assert_eq!(&set.p[0x100..0x108], b"NEO-GEO\0");
        assert!(
            set.p[BANK..]
                .as_chunks::<2>()
                .0
                .iter()
                .all(|word| *word == [0x34, 0x12])
        );
    }

    #[test]
    fn archive_and_rom_layout_errors_are_distinct() {
        assert!(matches!(
            from_zip(b"PK\x03\x04junk"),
            Err(RomSetError::Zip(_))
        ));
        assert_eq!(
            from_zip(&archive(&[("readme.txt", b"no ROMs")])),
            Err(RomSetError::Layout(LayoutError::NoProgramRom))
        );
    }
}
