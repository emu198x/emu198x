//! Master System cartridge image normalization, standard header parsing and
//! mapper detection.

use serde::{Deserialize, Serialize};

const SMD_HEADER_LEN: usize = 0x200;
const HEADER_LEN: usize = 0x10;
const SIGNATURE: &[u8; 8] = b"TMR SEGA";
const HEADER_OFFSETS: [usize; 3] = [0x1FF0, 0x3FF0, 0x7FF0];

/// Hardware territory encoded in the standard Sega cartridge header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CartridgeTerritory {
    /// Japanese Master System / Mark III.
    SmsJapan,
    /// Export Master System. The header does not distinguish NTSC from PAL.
    SmsExport,
    /// Japanese Game Gear.
    GameGearJapan,
    /// Export Game Gear.
    GameGearExport,
    /// International Game Gear.
    GameGearInternational,
    /// A header value outside Sega's documented assignments.
    Unknown(u8),
}

impl CartridgeTerritory {
    fn from_code(code: u8) -> Self {
        match code {
            3 => Self::SmsJapan,
            4 => Self::SmsExport,
            5 => Self::GameGearJapan,
            6 => Self::GameGearExport,
            7 => Self::GameGearInternational,
            other => Self::Unknown(other),
        }
    }
}

/// Parsed fields from a standard `TMR SEGA` cartridge header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CartridgeHeader {
    /// Header offset in the normalized ROM image.
    pub offset: usize,
    /// Checksum stored at header bytes `$0A-$0B` (little-endian).
    pub stored_checksum: u16,
    /// Sum of ROM bytes outside the 16-byte header, available to a BIOS gate.
    pub computed_checksum: u16,
    /// Hardware territory encoded by the high nibble of the final byte.
    pub territory: CartridgeTerritory,
    /// ROM length represented by the low nibble, when assigned by Sega.
    pub declared_size: Option<usize>,
}

/// The bank-switching scheme a cartridge carries. The mapper is on the
/// cartridge, not the console, so the same machine runs either.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CartridgeMapper {
    /// Sega's own: page registers at `$FFFD-$FFFF`, first 1 KB fixed.
    #[default]
    Sega,
    /// Codemasters': a page register at the bottom of each slot, written
    /// at `$0000`, `$4000` and `$8000`, with no fixed first 1 KB
    /// (`reference/by-system/sega-master-system/sms-reference.md`,
    /// "Codemasters Mapper").
    Codemasters,
}

/// Where Codemasters cartridges keep their own header, below Sega's.
const CODEMASTERS_HEADER: usize = 0x7FE0;

/// Picks the mapper from the image. Codemasters cartridges carry their own
/// header at `$7FE0`, beside the Sega one: the number of 16 KB pages, a BCD
/// date and time, then a 16-bit checksum and its complement, which sum to
/// `$10000`, followed by six zero bytes. Every Codemasters image checked
/// (Fantastic Dizzy on both consoles, The Excellent Dizzy Collection)
/// matches all of that; a Sega-mapped image matching it by chance is
/// vanishingly unlikely.
#[must_use]
pub fn detect_mapper(rom: &[u8]) -> CartridgeMapper {
    let Some(header) = rom.get(CODEMASTERS_HEADER..CODEMASTERS_HEADER + 0x10) else {
        return CartridgeMapper::Sega;
    };
    let checksum = u16::from_le_bytes([header[6], header[7]]);
    let complement = u16::from_le_bytes([header[8], header[9]]);
    let pages = usize::from(header[0]);
    let codemasters = checksum != 0
        && checksum.wrapping_add(complement) == 0
        && header[10..].iter().all(|&byte| byte == 0)
        && pages * 0x4000 == rom.len();
    if codemasters {
        CartridgeMapper::Codemasters
    } else {
        CartridgeMapper::Sega
    }
}

/// Normalize a cartridge dump and parse its standard Sega header, if present.
///
/// A 512-byte copier header is removed when the file length has the SMD
/// remainder described by the format. Header signatures are checked at the
/// three locations used by 8, 16, and 32+ KiB images.
#[must_use]
pub fn normalize_cartridge(mut rom: Vec<u8>) -> (Vec<u8>, Option<CartridgeHeader>) {
    if rom.len() % 0x4000 == SMD_HEADER_LEN {
        rom.drain(..SMD_HEADER_LEN);
    }

    let offset = HEADER_OFFSETS
        .into_iter()
        .find(|offset| rom.get(*offset..*offset + SIGNATURE.len()) == Some(SIGNATURE.as_slice()));
    let header = offset.and_then(|offset| parse_header(&rom, offset));
    (rom, header)
}

fn parse_header(rom: &[u8], offset: usize) -> Option<CartridgeHeader> {
    let bytes = rom.get(offset..offset + HEADER_LEN)?;
    let stored_checksum = u16::from_le_bytes([bytes[0x0A], bytes[0x0B]]);
    let region_size = bytes[0x0F];
    let computed_checksum = rom
        .iter()
        .enumerate()
        .filter(|(index, _)| *index < offset || *index >= offset + HEADER_LEN)
        .fold(0_u16, |sum, (_, byte)| sum.wrapping_add(u16::from(*byte)));

    Some(CartridgeHeader {
        offset,
        stored_checksum,
        computed_checksum,
        territory: CartridgeTerritory::from_code(region_size >> 4),
        declared_size: declared_size(region_size & 0x0F),
    })
}

fn declared_size(code: u8) -> Option<usize> {
    match code {
        0xA => Some(8 * 1024),
        0xB => Some(16 * 1024),
        0xC => Some(32 * 1024),
        0xD => Some(48 * 1024),
        0xE => Some(64 * 1024),
        0xF => Some(128 * 1024),
        0x0 => Some(256 * 1024),
        0x1 => Some(512 * 1024),
        0x2 => Some(1024 * 1024),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom_with_header(size: usize, territory: u8, size_code: u8) -> Vec<u8> {
        let mut rom = vec![1; size];
        let offset = if size <= 0x2000 {
            0x1FF0
        } else if size <= 0x4000 {
            0x3FF0
        } else {
            0x7FF0
        };
        rom[offset..offset + 8].copy_from_slice(SIGNATURE);
        rom[offset + 0x0F] = territory << 4 | size_code;
        let checksum = rom
            .iter()
            .enumerate()
            .filter(|(index, _)| *index < offset || *index >= offset + HEADER_LEN)
            .fold(0_u16, |sum, (_, byte)| sum.wrapping_add(u16::from(*byte)));
        rom[offset + 0x0A..offset + 0x0C].copy_from_slice(&checksum.to_le_bytes());
        rom
    }

    #[test]
    fn strips_smd_header_before_parsing() {
        let payload = rom_with_header(0x8000, 4, 0xC);
        let mut smd = vec![0xCC; SMD_HEADER_LEN];
        smd.extend_from_slice(&payload);
        let (normalized, header) = normalize_cartridge(smd);
        assert_eq!(normalized, payload);
        assert_eq!(header.expect("standard header").offset, 0x7FF0);
    }

    #[test]
    fn decodes_territory_size_and_checksum() {
        let rom = rom_with_header(0x8000, 3, 0xC);
        let (_, header) = normalize_cartridge(rom);
        let header = header.expect("standard header");
        assert_eq!(header.territory, CartridgeTerritory::SmsJapan);
        assert_eq!(header.declared_size, Some(32 * 1024));
        assert_eq!(header.computed_checksum, header.stored_checksum);
    }

    #[test]
    fn an_export_code_does_not_invent_a_tv_standard() {
        let rom = rom_with_header(0x8000, 4, 0xC);
        let (_, header) = normalize_cartridge(rom);
        assert_eq!(
            header.expect("standard header").territory,
            CartridgeTerritory::SmsExport
        );
    }

    /// The Codemasters header from the Game Gear Fantastic Dizzy (256 KB):
    /// 16 pages, 23 August 1993 at 19:24, checksum `$B64C`, complement
    /// `$49B4`.
    const FANTASTIC_DIZZY_GG: [u8; 16] = [
        0x10, 0x23, 0x08, 0x93, 0x19, 0x24, 0x4C, 0xB6, 0xB4, 0x49, 0, 0, 0, 0, 0, 0,
    ];

    fn rom_with_codemasters_header(size: usize, header: [u8; 16]) -> Vec<u8> {
        let mut rom = rom_with_header(size, 4, 0xC);
        rom[CODEMASTERS_HEADER..CODEMASTERS_HEADER + 0x10].copy_from_slice(&header);
        rom
    }

    #[test]
    fn a_codemasters_header_selects_the_codemasters_mapper() {
        let rom = rom_with_codemasters_header(0x40000, FANTASTIC_DIZZY_GG);
        assert_eq!(detect_mapper(&rom), CartridgeMapper::Codemasters);
    }

    #[test]
    fn anything_short_of_the_whole_header_stays_sega() {
        let mut bad_complement = FANTASTIC_DIZZY_GG;
        bad_complement[8] ^= 1;
        let mut trailing_byte = FANTASTIC_DIZZY_GG;
        trailing_byte[15] = 0x40;
        let mut no_checksum = FANTASTIC_DIZZY_GG;
        no_checksum[6..10].fill(0);
        for header in [bad_complement, trailing_byte, no_checksum] {
            let rom = rom_with_codemasters_header(0x40000, header);
            assert_eq!(detect_mapper(&rom), CartridgeMapper::Sega);
        }
        // The page count must match the image.
        let rom = rom_with_codemasters_header(0x20000, FANTASTIC_DIZZY_GG);
        assert_eq!(detect_mapper(&rom), CartridgeMapper::Sega);
        // Too small to hold the header at all.
        assert_eq!(
            detect_mapper(&rom_with_header(0x4000, 4, 0xC)),
            CartridgeMapper::Sega
        );
        assert_eq!(
            detect_mapper(&rom_with_header(0x8000, 4, 0xC)),
            CartridgeMapper::Sega
        );
    }
}
