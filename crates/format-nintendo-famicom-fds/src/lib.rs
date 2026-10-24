//! The Famicom Disk System `.fds` file layout.
//!
//! An `.fds` file is the data of each disk side, back to back, 65,500
//! bytes a side, optionally after a 16-byte header written by the fwNES
//! emulator: `FDS` and `$1A`, the side count, then eleven zero bytes.
//!
//! A side holds a sequence of blocks, each starting with its type byte:
//!
//! | Type | Block | Length |
//! |---|---|---|
//! | 1 | Disk info | 56 bytes |
//! | 2 | File amount | 2 bytes |
//! | 3 | File header | 16 bytes |
//! | 4 | File data | 1 + the size in the file header before it |
//!
//! The file keeps only the block contents. What the drive also reads —
//! the gaps, the start mark before each block and the CRC after it — is
//! left out, and putting it back is the drive's business, so it lives with
//! the RAM adapter in `nintendo-nes-cartridge`.
//!
//! This crate reads and writes the layout and nothing more.
//!
//! Facts from the NESdev wiki "FDS file format" and "FDS disk format"
//! pages.

use std::fmt;

/// The bytes one disk side takes in the file.
pub const SIDE_LEN: usize = 65_500;

/// The fwNES header's length.
pub const HEADER_LEN: usize = 16;

/// The header's first four bytes.
pub const HEADER_MAGIC: [u8; 4] = *b"FDS\x1A";

/// The signature the disk info block carries after its type byte.
pub const DISK_SIGNATURE: [u8; 14] = *b"*NINTENDO-HVC*";

const DISK_INFO_LEN: usize = 56;
const FILE_AMOUNT_LEN: usize = 2;
const FILE_HEADER_LEN: usize = 16;

/// A parsed `.fds` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FdsImage<'a> {
    /// Whether the file has the fwNES header.
    pub has_header: bool,
    /// Each side's 65,500 bytes, in file order: disk 1 side A, side B,
    /// disk 2 side A and so on.
    pub sides: Vec<Side<'a>>,
}

/// One disk side's bytes as the file stores them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side<'a> {
    /// The side's 65,500 bytes.
    pub bytes: &'a [u8],
}

/// Why a file could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FdsError {
    /// The file holds less than one side.
    TooShort {
        /// Bytes the file holds.
        actual: usize,
    },
    /// The header declares more sides than the file holds.
    Truncated {
        /// Sides the header declares.
        declared: usize,
        /// Whole sides the file holds.
        actual: usize,
    },
    /// The header declares no sides.
    NoSides,
}

impl fmt::Display for FdsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { actual } => write!(
                f,
                "FDS file too short: {actual} bytes, a disk side is {SIDE_LEN}"
            ),
            Self::Truncated { declared, actual } => write!(
                f,
                "FDS header declares {declared} sides but the file holds {actual}"
            ),
            Self::NoSides => f.write_str("FDS header declares no disk sides"),
        }
    }
}

impl std::error::Error for FdsError {}

/// Parse an `.fds` file, with or without the fwNES header.
///
/// Without a header the side count is the file length over 65,500; bytes
/// past the last whole side are ignored, as other readers do. With one,
/// the declared count must fit in the file.
///
/// # Errors
///
/// [`FdsError`] when the file holds no whole side, or fewer than its
/// header declares.
pub fn parse(data: &[u8]) -> Result<FdsImage<'_>, FdsError> {
    let has_header = data.len() >= HEADER_LEN && data[..4] == HEADER_MAGIC;
    let body = if has_header {
        &data[HEADER_LEN..]
    } else {
        data
    };
    let whole = body.len() / SIDE_LEN;
    let count = if has_header {
        let declared = usize::from(data[4]);
        if declared == 0 {
            return Err(FdsError::NoSides);
        }
        if declared > whole {
            return Err(FdsError::Truncated {
                declared,
                actual: whole,
            });
        }
        declared
    } else {
        if whole == 0 {
            return Err(FdsError::TooShort { actual: data.len() });
        }
        whole
    };
    let sides = body
        .as_chunks::<SIDE_LEN>()
        .0
        .iter()
        .take(count)
        .map(|bytes| Side { bytes })
        .collect();
    Ok(FdsImage { has_header, sides })
}

/// Write sides back out as an `.fds` file. Each side is padded with zeros
/// or cut to 65,500 bytes. `header` adds the fwNES header, whose side
/// count is one byte; more than 255 sides are written but counted as 255.
#[must_use]
pub fn write<S: AsRef<[u8]>>(sides: &[S], header: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + sides.len() * SIDE_LEN);
    if header {
        out.extend_from_slice(&HEADER_MAGIC);
        out.push(u8::try_from(sides.len()).unwrap_or(u8::MAX));
        out.resize(HEADER_LEN, 0);
    }
    for side in sides {
        let bytes = side.as_ref();
        let n = bytes.len().min(SIDE_LEN);
        out.extend_from_slice(&bytes[..n]);
        out.resize(out.len() + SIDE_LEN - n, 0);
    }
    out
}

/// A block's type, from its first byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// Type 1: disk info.
    DiskInfo,
    /// Type 2: file amount.
    FileAmount,
    /// Type 3: file header.
    FileHeader,
    /// Type 4: file data.
    FileData,
}

/// One block on a side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block<'a> {
    /// Which block it is.
    pub kind: BlockKind,
    /// Its bytes, type byte included.
    pub bytes: &'a [u8],
}

impl Block<'_> {
    /// The disk info fields, for a type 1 block.
    #[must_use]
    pub fn disk_info(&self) -> Option<DiskInfo> {
        (self.kind == BlockKind::DiskInfo).then(|| DiskInfo::from_block(self.bytes))
    }

    /// The visible file count, for a type 2 block. Files past it are
    /// hidden from the BIOS's file listing but still on the disk.
    #[must_use]
    pub fn file_amount(&self) -> Option<u8> {
        (self.kind == BlockKind::FileAmount).then(|| self.bytes[1])
    }

    /// The file header fields, for a type 3 block.
    #[must_use]
    pub fn file_header(&self) -> Option<FileHeader> {
        (self.kind == BlockKind::FileHeader).then(|| FileHeader::from_block(self.bytes))
    }

    /// The file's contents, for a type 4 block.
    #[must_use]
    pub fn file_data(&self) -> Option<&[u8]> {
        (self.kind == BlockKind::FileData).then(|| &self.bytes[1..])
    }
}

/// The fields of the disk info block that identify a disk side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskInfo {
    /// Whether bytes 1–14 hold `*NINTENDO-HVC*`; the BIOS refuses a disk
    /// without it.
    pub signature_ok: bool,
    /// The licensee code.
    pub manufacturer: u8,
    /// The three-letter game code.
    pub game_name: [u8; 3],
    /// The game type: a space for a normal disk, `E` for an event disk,
    /// `R` for a reduction disk.
    pub game_type: u8,
    /// The game revision.
    pub revision: u8,
    /// 0 for side A, 1 for side B.
    pub side_number: u8,
    /// The disk number, from 0.
    pub disk_number: u8,
    /// The disk type.
    pub disk_type: u8,
    /// Files with an ID up to this one are loaded at boot.
    pub boot_file: u8,
}

impl DiskInfo {
    fn from_block(bytes: &[u8]) -> Self {
        Self {
            signature_ok: bytes[1..15] == DISK_SIGNATURE,
            manufacturer: bytes[15],
            game_name: [bytes[16], bytes[17], bytes[18]],
            game_type: bytes[19],
            revision: bytes[20],
            side_number: bytes[21],
            disk_number: bytes[22],
            disk_type: bytes[23],
            boot_file: bytes[25],
        }
    }
}

/// What a file header says about the file data block after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileHeader {
    /// The file's position on the side, from 0.
    pub number: u8,
    /// The ID the boot file setting is compared with.
    pub id: u8,
    /// The eight-character file name.
    pub name: [u8; 8],
    /// Where the file loads.
    pub address: u16,
    /// The file data block's size, without its type byte.
    pub size: u16,
    /// 0 for CPU memory, 1 for CHR, 2 for nametables.
    pub kind: u8,
}

impl FileHeader {
    fn from_block(bytes: &[u8]) -> Self {
        let mut name = [0; 8];
        name.copy_from_slice(&bytes[3..11]);
        Self {
            number: bytes[1],
            id: bytes[2],
            name,
            address: u16::from_le_bytes([bytes[11], bytes[12]]),
            size: u16::from_le_bytes([bytes[13], bytes[14]]),
            kind: bytes[15],
        }
    }
}

/// The blocks a side holds, and whatever followed the last of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan<'a> {
    /// The blocks from the start of the side, in order.
    pub blocks: Vec<Block<'a>>,
    /// The bytes after the last whole block: zeros on an ordinary disk,
    /// data in a layout the four block types do not describe otherwise.
    pub rest: &'a [u8],
}

impl<'a> Side<'a> {
    /// Read the blocks from the start of the side. The scan stops at the
    /// first byte that is not a block type, at a block that would run past
    /// the side's end, or at file data with no file header before it.
    #[must_use]
    pub fn scan(&self) -> Scan<'a> {
        let bytes = self.bytes;
        let mut blocks = Vec::new();
        let mut at = 0;
        let mut file_size: Option<usize> = None;
        while at < bytes.len() {
            let (kind, len) = match bytes[at] {
                1 => (BlockKind::DiskInfo, DISK_INFO_LEN),
                2 => (BlockKind::FileAmount, FILE_AMOUNT_LEN),
                3 => (BlockKind::FileHeader, FILE_HEADER_LEN),
                4 => match file_size.take() {
                    Some(size) => (BlockKind::FileData, 1 + size),
                    None => break,
                },
                _ => break,
            };
            let Some(block) = bytes.get(at..at + len) else {
                break;
            };
            if kind == BlockKind::FileHeader {
                file_size = Some(usize::from(u16::from_le_bytes([block[13], block[14]])));
            }
            blocks.push(Block { kind, bytes: block });
            at += len;
        }
        Scan {
            blocks,
            rest: &bytes[at..],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_describe_themselves() {
        assert!(FdsError::TooShort { actual: 3 }.to_string().contains('3'));
        assert!(
            FdsError::Truncated {
                declared: 2,
                actual: 1
            }
            .to_string()
            .contains("2 sides")
        );
        assert!(FdsError::NoSides.to_string().contains("no disk sides"));
    }
}
