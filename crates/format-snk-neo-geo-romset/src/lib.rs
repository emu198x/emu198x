//! Neo Geo cartridge ROM sets, as MAME names and lays them out.
//!
//! A Neo Geo cartridge is not one image but a set of chips, each dumped to
//! its own file: P-ROMs for the 68000, an S-ROM of fix-layer tiles, an
//! M-ROM for the Z80, V-ROMs of ADPCM samples and C-ROMs of sprite tiles.
//! This crate takes those files, by name, and builds the one region per bus
//! a machine reads.
//!
//! A file's role and order come from its extension, or failing that the end
//! of its stem: `201-p1.p1` and `201-p1.bin` are both P-ROM 1. The layout
//! follows MAME's `neogeo.cpp` (BSD-3-Clause):
//!
//! - P-ROMs are stored with each 16-bit word byte-swapped; they are swapped
//!   back to the 68000's order and joined, P1 first.
//! - V-ROMs with one digit (`v1`, `v2`) feed ADPCM-A, which ADPCM-B shares.
//!   Two digits (`v11`, `v21`) name the region first: `v1x` for ADPCM-A,
//!   `v2x` for ADPCM-B, as on the first boards that kept them apart.
//! - C-ROMs pair up, C1 with C2, C3 with C4, and each pair interleaves a
//!   byte at a time, the odd ROM in even bytes.
//!
//! Files that fit no role, a BIOS or a readme in the same archive, are left
//! out. Some games load a P-ROM's halves in the other order, and some are
//! encrypted; neither is handled here.

use std::io::{Cursor, Read};

use thiserror::Error;

/// The ROMs of one cartridge, in the layout each bus reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RomSet {
    /// P-ROM, big-endian.
    pub p: Vec<u8>,
    /// S-ROM.
    pub s: Vec<u8>,
    /// M-ROM.
    pub m: Vec<u8>,
    /// V-ROM for ADPCM-A (and ADPCM-B when `v_b` is empty).
    pub v_a: Vec<u8>,
    /// V-ROM for ADPCM-B, when the board keeps it apart.
    pub v_b: Vec<u8>,
    /// C-ROM pairs, each interleaved.
    pub c: Vec<u8>,
}

/// Why a ROM set could not be built.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RomSetError {
    /// No P-ROM: there is nothing for the 68000 to run.
    #[error("no P-ROM (a file named like p1)")]
    NoProgramRom,
    /// Two files claim the same role and number.
    #[error("two files are {role}{index}: {first} and {second}")]
    Duplicate {
        role: char,
        index: u8,
        first: String,
        second: String,
    },
    /// A C-ROM is missing its partner, or the two differ in size.
    #[error("C-ROM {index} has no partner of the same size")]
    UnpairedSpriteRom { index: u8 },
    /// The archive could not be read.
    #[error("not a readable zip archive: {0}")]
    Zip(String),
}

/// What part a file plays: its role letter and its number.
fn classify(name: &str) -> Option<(char, u8)> {
    let base = name.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
    let (stem, extension) = base.rsplit_once('.').unwrap_or((&base, ""));
    tag(extension).or_else(|| {
        let tail = stem.rsplit(['-', '_']).next()?;
        tag(tail)
    })
}

/// `p1`, `c12`, `v21` and the like.
fn tag(text: &str) -> Option<(char, u8)> {
    let mut chars = text.chars();
    let role = chars.next()?;
    if !matches!(role, 'p' | 's' | 'm' | 'v' | 'c') {
        return None;
    }
    let digits = chars.as_str();
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let index: u8 = digits.parse().ok()?;
    (index > 0).then_some((role, index))
}

/// Build a ROM set from `(file name, contents)` pairs.
///
/// # Errors
///
/// Returns an error if there is no P-ROM, two files claim one role, or a
/// C-ROM has no partner of its size.
pub fn from_files(
    files: impl IntoIterator<Item = (String, Vec<u8>)>,
) -> Result<RomSet, RomSetError> {
    let mut roles: Vec<(char, u8, String, Vec<u8>)> = Vec::new();
    for (name, bytes) in files {
        let Some((role, index)) = classify(&name) else {
            continue;
        };
        if let Some((_, _, first, _)) = roles.iter().find(|r| r.0 == role && r.1 == index) {
            return Err(RomSetError::Duplicate {
                role,
                index,
                first: first.clone(),
                second: name,
            });
        }
        roles.push((role, index, name, bytes));
    }
    roles.sort_by_key(|r| (r.0, r.1));
    let of = |role: char| roles.iter().filter(move |r| r.0 == role);

    let mut set = RomSet::default();
    for (_, _, _, bytes) in of('p') {
        set.p
            .extend(bytes.chunks(2).flat_map(|w| w.iter().rev().copied()));
    }
    if set.p.is_empty() {
        return Err(RomSetError::NoProgramRom);
    }
    if let Some((_, _, _, bytes)) = of('s').next() {
        set.s.clone_from(bytes);
    }
    if let Some((_, _, _, bytes)) = of('m').next() {
        set.m.clone_from(bytes);
    }
    for (_, index, _, bytes) in of('v') {
        if *index >= 20 {
            set.v_b.extend_from_slice(bytes);
        } else {
            set.v_a.extend_from_slice(bytes);
        }
    }
    let c: Vec<_> = of('c').collect();
    for pair in c.chunks(2) {
        match pair {
            [(_, odd_index, _, odd), (_, even_index, _, even)]
                if *even_index == odd_index + 1 && odd.len() == even.len() =>
            {
                set.c.reserve(odd.len() * 2);
                for (&a, &b) in odd.iter().zip(even) {
                    set.c.push(a);
                    set.c.push(b);
                }
            }
            [(_, index, _, _), ..] => {
                return Err(RomSetError::UnpairedSpriteRom { index: *index });
            }
            [] => {}
        }
    }
    Ok(set)
}

/// Build a ROM set from a zip archive's bytes, as MAME keeps one game.
///
/// # Errors
///
/// Returns an error if the archive cannot be read, or as [`from_files`].
pub fn from_zip(bytes: &[u8]) -> Result<RomSet, RomSetError> {
    let zip_error = |err: zip::result::ZipError| RomSetError::Zip(err.to_string());
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        if entry.is_dir() || classify(entry.name()).is_none() {
            continue;
        }
        let name = entry.name().to_owned();
        let mut contents = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut contents)
            .map_err(|err| RomSetError::Zip(err.to_string()))?;
        files.push((name, contents));
    }
    from_files(files)
}

/// Whether `bytes` start like a zip archive.
#[must_use]
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

#[cfg(test)]
mod tests;
