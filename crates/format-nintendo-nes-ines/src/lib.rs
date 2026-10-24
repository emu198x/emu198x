//! The iNES / NES 2.0 file layout.
//!
//! An iNES file is a 16-byte header, an optional 512-byte trainer, then PRG
//! ROM and CHR ROM back to back. The header names the mapper board, the
//! nametable mirroring, whether the board has a battery, and how much ROM
//! follows. NES 2.0 is the same layout with a signature in byte 7 that
//! widens the mapper number to 12 bits and the ROM sizes to larger counts or
//! an exponent form.
//!
//! This crate reads the layout and nothing more. Which board a mapper
//! number means, and how that board behaves, is cartridge hardware; it lives
//! in `nintendo-nes-cartridge`, which builds a board from a parsed
//! [`InesImage`].
//!
//! Format-author specification: <https://fms.komkon.org/EMUL8/NES.html>.
//! NES 2.0 sizes: `reference/by-system/nintendo-nes/2026-nes2-rom-size-observations.md`.

use std::fmt;

const HEADER_LEN: usize = 16;
const TRAINER_LEN: usize = 512;
const PRG_BANK: usize = 16 * 1024;
const CHR_BANK: usize = 8 * 1024;

/// How the header says the nametables are mirrored. A board with its own
/// mirroring control may override it once running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderMirroring {
    /// Flags 6 bit 0 clear: the two horizontal strips share a nametable.
    Horizontal,
    /// Flags 6 bit 0 set: the two vertical strips share a nametable.
    Vertical,
    /// Flags 6 bit 3 set: the board supplies four nametables of its own.
    FourScreen,
}

/// The header's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InesHeader {
    /// Raw PRG size byte (header byte 4). A 16 KiB bank count for iNES;
    /// NES 2.0 can extend it or use the exponent form.
    pub prg_rom_banks: u8,
    /// Raw CHR size byte (header byte 5). An 8 KiB bank count for iNES;
    /// NES 2.0 can extend it or use the exponent form.
    pub chr_rom_banks: u8,
    /// Mapper number: 8 bits for iNES, 12 for NES 2.0.
    pub mapper_number: u16,
    /// The mirroring the header declares.
    pub mirroring: HeaderMirroring,
    /// Whether the board keeps its PRG RAM alive with a battery.
    pub has_battery: bool,
    /// Whether the file carries the NES 2.0 signature.
    pub is_nes_2_0: bool,
}

/// A parsed file, borrowing its ROM from the bytes it was parsed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InesImage<'a> {
    /// The header's fields.
    pub header: InesHeader,
    /// The 512 bytes loaded at `$7000` before the CPU starts, if present.
    pub trainer: Option<&'a [u8]>,
    /// The PRG ROM.
    pub prg_rom: &'a [u8],
    /// The CHR ROM. Empty when the board uses CHR RAM instead.
    pub chr_rom: &'a [u8],
}

/// Why a file could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InesError {
    /// Shorter than the 16-byte header.
    TooShort,
    /// The first four bytes are not `NES` and `$1A`.
    BadMagic,
    /// A NES 2.0 PRG size too large to address.
    PrgSizeOverflow,
    /// A NES 2.0 CHR size too large to address.
    ChrSizeOverflow,
    /// The PRG ROM would end past the address space.
    PrgExtentOverflow,
    /// The CHR ROM would end past the address space.
    ChrExtentOverflow,
    /// The header asks for more bytes than the file holds.
    Truncated {
        /// Bytes the header describes.
        expected: usize,
        /// Bytes the file holds.
        actual: usize,
    },
}

impl fmt::Display for InesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => f.write_str("iNES file too short (< 16 bytes)"),
            Self::BadMagic => f.write_str("Invalid iNES magic (expected NES\\x1A)"),
            Self::PrgSizeOverflow => f.write_str("NES 2.0 PRG ROM size exceeds the address space"),
            Self::ChrSizeOverflow => f.write_str("NES 2.0 CHR ROM size exceeds the address space"),
            Self::PrgExtentOverflow => f.write_str("iNES PRG ROM extent exceeds the address space"),
            Self::ChrExtentOverflow => f.write_str("iNES CHR ROM extent exceeds the address space"),
            Self::Truncated { expected, actual } => write!(
                f,
                "iNES file too short: expected {expected} bytes, got {actual}"
            ),
        }
    }
}

impl std::error::Error for InesError {}

/// Parse an iNES or NES 2.0 file.
///
/// # Errors
///
/// [`InesError`] when the file has no header, lacks the magic, or describes
/// more ROM than it holds or than the address space can.
pub fn parse(data: &[u8]) -> Result<InesImage<'_>, InesError> {
    if data.len() < HEADER_LEN {
        return Err(InesError::TooShort);
    }
    if &data[0..4] != b"NES\x1a" {
        return Err(InesError::BadMagic);
    }

    let prg_banks = data[4];
    let chr_banks = data[5];
    let flags6 = data[6];
    let flags7 = data[7];
    let mapper_lo = (flags6 >> 4) & 0x0F;
    let mapper_hi = flags7 & 0xF0;

    // NES 2.0 is signalled by bits 3-2 of flags 7 reading 0b10.
    let is_nes_2_0 = (flags7 & 0x0C) == 0x08;

    let (mapper_number, prg_size, chr_size) = if is_nes_2_0 {
        let mapper_number =
            u16::from(mapper_lo) | u16::from(mapper_hi) | (u16::from(data[8] & 0x0F) << 8);
        let prg_size =
            nes2_rom_size(prg_banks, data[9] & 0x0F, PRG_BANK).ok_or(InesError::PrgSizeOverflow)?;
        let chr_size =
            nes2_rom_size(chr_banks, data[9] >> 4, CHR_BANK).ok_or(InesError::ChrSizeOverflow)?;
        (mapper_number, prg_size, chr_size)
    } else {
        (
            u16::from(mapper_hi | mapper_lo),
            usize::from(prg_banks) * PRG_BANK,
            usize::from(chr_banks) * CHR_BANK,
        )
    };

    let mirroring = if flags6 & 0x08 != 0 {
        HeaderMirroring::FourScreen
    } else if flags6 & 0x01 != 0 {
        HeaderMirroring::Vertical
    } else {
        HeaderMirroring::Horizontal
    };
    let has_trainer = flags6 & 0x04 != 0;

    let prg_start = if has_trainer {
        HEADER_LEN + TRAINER_LEN
    } else {
        HEADER_LEN
    };
    let chr_start = prg_start
        .checked_add(prg_size)
        .ok_or(InesError::PrgExtentOverflow)?;
    let rom_end = chr_start
        .checked_add(chr_size)
        .ok_or(InesError::ChrExtentOverflow)?;
    if data.len() < rom_end {
        return Err(InesError::Truncated {
            expected: rom_end,
            actual: data.len(),
        });
    }

    Ok(InesImage {
        header: InesHeader {
            prg_rom_banks: prg_banks,
            chr_rom_banks: chr_banks,
            mapper_number,
            mirroring,
            has_battery: flags6 & 0x02 != 0,
            is_nes_2_0,
        },
        trainer: has_trainer.then(|| &data[HEADER_LEN..prg_start]),
        prg_rom: &data[prg_start..chr_start],
        chr_rom: &data[chr_start..rom_end],
    })
}

/// A NES 2.0 ROM size. An MSB nibble of `$F` makes the low byte `EEEEEEMM`,
/// meaning 2^E × (2M + 1) bytes; otherwise the nibble and byte together
/// count banks.
fn nes2_rom_size(low: u8, high: u8, bank_size: usize) -> Option<usize> {
    if high == 0x0F {
        let power = 1usize.checked_shl(u32::from(low >> 2))?;
        power.checked_mul(usize::from((low & 3) * 2 + 1))
    } else {
        ((usize::from(high) << 8) | usize::from(low)).checked_mul(bank_size)
    }
}
