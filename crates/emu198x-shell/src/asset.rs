//! Host-side asset loading helpers.
//!
//! Emulated machines should receive normalized raw media bytes. Archive
//! unpacking is therefore a host concern, not a runtime concern.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use thiserror::Error;
use zip::ZipArchive;

use crate::media::MediaKind;

/// One asset loaded from disk, optionally from inside one archive member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedAsset {
    /// The raw image bytes ready for runtime loading.
    pub bytes: Vec<u8>,
    /// The selected archive member path when the source was compressed.
    pub archive_member: Option<String>,
}

/// Error surfaced while loading one host-side asset.
#[derive(Debug, Error)]
pub enum AssetLoadError {
    /// One filesystem operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// One zip archive could not be parsed.
    #[error("archive {path} is not a valid zip file: {reason}")]
    InvalidZip {
        /// Archive path on disk.
        path: PathBuf,
        /// Parser failure note.
        reason: String,
    },

    /// The archive had no non-directory members.
    #[error("archive {path} does not contain any files")]
    EmptyArchive {
        /// Archive path on disk.
        path: PathBuf,
    },

    /// No suitable archive member matched the expected asset kind.
    #[error("archive {path} does not contain a loadable {expected}")]
    NoMatchingArchiveMember {
        /// Archive path on disk.
        path: PathBuf,
        /// Human-readable expected member description.
        expected: &'static str,
    },

    /// More than one suitable archive member was present.
    #[error("archive {path} contains multiple loadable {expected} entries: {members:?}")]
    AmbiguousArchiveMembers {
        /// Archive path on disk.
        path: PathBuf,
        /// Human-readable expected member description.
        expected: &'static str,
        /// Matching member paths inside the archive.
        members: Vec<String>,
    },
}

/// Reads one firmware asset from disk, expanding a zip archive when required.
///
/// # Errors
///
/// Returns an error if file I/O fails, the zip container is invalid, or the
/// archive does not contain exactly one firmware-like file.
pub fn read_firmware_asset(path: &Path) -> Result<LoadedAsset, AssetLoadError> {
    read_asset(path, "firmware image", &["rom", "bin"])
}

/// Reads one host-side source or program asset from disk, expanding a zip
/// archive when required.
///
/// # Errors
///
/// Returns an error if file I/O fails, the zip container is invalid, or the
/// archive does not contain exactly one `.bas` or `.prg` member.
pub fn read_program_asset(path: &Path) -> Result<LoadedAsset, AssetLoadError> {
    read_asset(path, "program file", &["bas", "prg"])
}

/// Reads one media asset from disk, expanding a zip archive when required.
///
/// # Errors
///
/// Returns an error if file I/O fails, the zip container is invalid, or the
/// archive does not contain exactly one image compatible with the media kind.
pub fn read_media_asset(path: &Path, kind: MediaKind) -> Result<LoadedAsset, AssetLoadError> {
    let (expected, extensions) = archive_match_rules(kind);
    read_asset(path, expected, extensions)
}

fn read_asset(
    path: &Path,
    expected: &'static str,
    extensions: &[&str],
) -> Result<LoadedAsset, AssetLoadError> {
    if !is_zip_path(path) {
        return Ok(LoadedAsset {
            bytes: fs::read(path)?,
            archive_member: None,
        });
    }

    read_zip_asset(path, expected, extensions)
}

fn read_zip_asset(
    path: &Path,
    expected: &'static str,
    extensions: &[&str],
) -> Result<LoadedAsset, AssetLoadError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(|reason| AssetLoadError::InvalidZip {
        path: path.to_path_buf(),
        reason: reason.to_string(),
    })?;

    let mut file_count = 0usize;
    let mut matches = Vec::new();

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|reason| AssetLoadError::InvalidZip {
                path: path.to_path_buf(),
                reason: reason.to_string(),
            })?;
        if entry.is_dir() {
            continue;
        }

        file_count += 1;
        let name = entry.name().to_owned();
        if entry_name_matches(&name, extensions) {
            matches.push((index, name));
        }
    }

    if file_count == 0 {
        return Err(AssetLoadError::EmptyArchive {
            path: path.to_path_buf(),
        });
    }

    let (index, member) = match matches.len() {
        0 => {
            return Err(AssetLoadError::NoMatchingArchiveMember {
                path: path.to_path_buf(),
                expected,
            });
        }
        1 => matches.swap_remove(0),
        _ => {
            let members = matches.into_iter().map(|(_, name)| name).collect();
            return Err(AssetLoadError::AmbiguousArchiveMembers {
                path: path.to_path_buf(),
                expected,
                members,
            });
        }
    };

    let mut entry = archive
        .by_index(index)
        .map_err(|reason| AssetLoadError::InvalidZip {
            path: path.to_path_buf(),
            reason: reason.to_string(),
        })?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;

    Ok(LoadedAsset {
        bytes,
        archive_member: Some(member),
    })
}

fn is_zip_path(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
}

fn entry_name_matches(name: &str, extensions: &[&str]) -> bool {
    Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| {
            extensions
                .iter()
                .any(|expected| ext.eq_ignore_ascii_case(expected))
        })
}

fn archive_match_rules(kind: MediaKind) -> (&'static str, &'static [&'static str]) {
    match kind {
        MediaKind::Tape => ("tape image", &["tap", "tzx"]),
        MediaKind::Disk => (
            "disk image",
            &[
                "adf", "d64", "d71", "d81", "dsk", "g64", "ipf", "nib", "woz",
            ],
        ),
        MediaKind::Cartridge => (
            "cartridge image",
            &["bin", "crt", "gb", "gbc", "md", "nes", "rom"],
        ),
        MediaKind::Optical => ("optical image", &["bin", "ccd", "chd", "cue", "img", "iso"]),
        MediaKind::Snapshot => ("snapshot image", &["pst", "sna", "szx", "z80"]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::{Cursor, Write};

    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    fn write_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, bytes) in entries {
                writer
                    .start_file(*name, options)
                    .expect("zip test entry should start");
                writer
                    .write_all(bytes)
                    .expect("zip test entry should write");
            }
            writer.finish().expect("zip test archive should finish");
        }
        cursor.into_inner()
    }

    fn temp_path(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "emu198x-shell-asset-{}-{suffix}",
            std::process::id()
        ))
    }

    #[test]
    fn read_media_asset_extracts_matching_member_from_zip() {
        let path = temp_path("media.zip");
        let zip = write_zip(&[
            ("manual.txt", b"not media"),
            ("Manic Miner.tzx", b"ZXTape!\x1A\x01"),
        ]);
        fs::write(&path, zip).expect("zip test fixture should write");

        let loaded = read_media_asset(&path, MediaKind::Tape)
            .expect("zip tape asset should extract one tzx member");

        assert_eq!(loaded.bytes, b"ZXTape!\x1A\x01");
        assert_eq!(loaded.archive_member.as_deref(), Some("Manic Miner.tzx"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_media_asset_rejects_ambiguous_zip_members() {
        let path = temp_path("ambiguous.zip");
        let zip = write_zip(&[("a.tap", b"a"), ("b.tzx", b"b")]);
        fs::write(&path, zip).expect("zip test fixture should write");

        let error =
            read_media_asset(&path, MediaKind::Tape).expect_err("multiple tape members must fail");

        assert!(matches!(
            error,
            AssetLoadError::AmbiguousArchiveMembers { .. }
        ));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_firmware_asset_reads_plain_file() {
        let path = temp_path("48.rom");
        fs::write(&path, [0x01, 0x02, 0x03]).expect("firmware fixture should write");

        let loaded = read_firmware_asset(&path).expect("plain firmware file should read directly");

        assert_eq!(loaded.bytes, vec![0x01, 0x02, 0x03]);
        assert_eq!(loaded.archive_member, None);

        let _ = fs::remove_file(path);
    }
}
