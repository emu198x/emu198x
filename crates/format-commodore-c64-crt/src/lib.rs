//! Commodore 64 `CRT` cartridge-image parsing.
//!
//! A `.crt` file is the standard container for C64 cartridge dumps (the format
//! VICE, CCS64 and Hoxs64 use). It has a fixed 64-byte header carrying the
//! hardware type and the initial state of the `EXROM`/`GAME` control lines,
//! followed by one or more `CHIP` packets — each a ROM/RAM image tagged with a
//! bank number and a load address (`$8000` for ROML, `$A000` or `$E000` for
//! ROMH).
//!
//! This crate is a pure parser: it turns the bytes into a [`CrtCartridge`] and
//! leaves the PLA banking (how `EXROM`/`GAME` + the load addresses map into the
//! `$8000-$FFFF` window) to the machine.

use thiserror::Error;

/// Fixed CRT header length in bytes. The header's own length field usually
/// reports this, but some dumps pad it larger; we honour the reported value.
const MIN_HEADER_LEN: usize = 0x40;
const MAGIC: &[u8; 16] = b"C64 CARTRIDGE   ";
const CHIP_MAGIC: &[u8; 4] = b"CHIP";
const CHIP_HEADER_LEN: usize = 0x10;

/// One ROM/RAM image packet from a CRT file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrtChip {
    /// Chip type: 0 = ROM, 1 = RAM (no image), 2 = Flash ROM.
    pub chip_type: u16,
    /// Bank number this image belongs to (0 for unbanked carts).
    pub bank: u16,
    /// Load address: `$8000` (ROML), `$A000` or `$E000` (ROMH).
    pub load_address: u16,
    /// The ROM image bytes (its advertised size).
    pub data: Vec<u8>,
}

/// A parsed C64 cartridge image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrtCartridge {
    /// Cartridge hardware type (0 = normal/generic, 1 = Action Replay,
    /// 32 = EasyFlash, …). Only 0 (normal) is mapped by the base machine today.
    pub hardware_type: u16,
    /// `EXROM` line asserted (pulled low) at reset. Active-low in hardware; the
    /// header stores 0 for "asserted", which we normalise to `true` here.
    pub exrom: bool,
    /// `GAME` line asserted (pulled low) at reset. Same convention as `exrom`.
    pub game: bool,
    /// Cartridge name from the header (trimmed of NUL padding).
    pub name: String,
    /// The ROM/RAM image packets, in file order.
    pub chips: Vec<CrtChip>,
}

impl CrtCartridge {
    /// The banking shape implied by the `EXROM`/`GAME` lines at reset.
    #[must_use]
    pub fn mode(&self) -> CrtMode {
        match (self.exrom, self.game) {
            // EXROM low, GAME high → 8K ROM at $8000.
            (true, false) => CrtMode::Rom8k,
            // Both low → 16K ROM at $8000 + $A000.
            (true, true) => CrtMode::Rom16k,
            // EXROM high, GAME low → Ultimax (ROM at $8000 + $E000, RAM hidden).
            (false, true) => CrtMode::Ultimax,
            // Both high → nothing mapped (RAM-only / bank-switched-from-off).
            (false, false) => CrtMode::None,
        }
    }
}

/// The reset-time banking mode a cartridge selects via `EXROM`/`GAME`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrtMode {
    /// No ROM mapped at reset (both lines high — e.g. a bank-switched cart that
    /// powers up disabled).
    None,
    /// 8K ROM visible at `$8000-$9FFF`.
    Rom8k,
    /// 16K ROM visible at `$8000-$9FFF` + `$A000-$BFFF`.
    Rom16k,
    /// Ultimax: ROM at `$8000-$9FFF` + `$E000-$FFFF`, most RAM/ROM hidden.
    Ultimax,
}

/// Error surfaced while parsing a CRT image.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CrtParseError {
    /// The image is shorter than the fixed header.
    #[error("CRT image is too short: expected at least {expected} bytes, got {actual}")]
    TooShort { expected: usize, actual: usize },
    /// The 16-byte signature is not `C64 CARTRIDGE   `.
    #[error("CRT image has an invalid header signature")]
    InvalidSignature,
    /// The header's length field is smaller than the fixed minimum.
    #[error("CRT header length {0} is below the {MIN_HEADER_LEN}-byte minimum")]
    InvalidHeaderLength(usize),
    /// A CHIP packet is malformed or extends past the end of the image.
    #[error("CRT CHIP packet at offset {offset} is truncated or malformed")]
    TruncatedChip { offset: usize },
    /// No CHIP packets were present.
    #[error("CRT image contains no CHIP packets")]
    NoChips,
}

fn be_u16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}

fn be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Parse a `.crt` cartridge image.
///
/// # Errors
///
/// Returns an error if the signature is wrong, the header is malformed, or a
/// CHIP packet is truncated.
pub fn parse(bytes: &[u8]) -> Result<CrtCartridge, CrtParseError> {
    if bytes.len() < MIN_HEADER_LEN {
        return Err(CrtParseError::TooShort {
            expected: MIN_HEADER_LEN,
            actual: bytes.len(),
        });
    }
    if &bytes[0..16] != MAGIC {
        return Err(CrtParseError::InvalidSignature);
    }

    let header_len = be_u32(&bytes[0x10..0x14]) as usize;
    if header_len < MIN_HEADER_LEN || header_len > bytes.len() {
        return Err(CrtParseError::InvalidHeaderLength(header_len));
    }

    // The line bytes are 0 = asserted (low). Normalise to "asserted = true".
    let exrom = bytes[0x18] == 0;
    let game = bytes[0x19] == 0;
    let hardware_type = be_u16(&bytes[0x16..0x18]);
    let name = String::from_utf8_lossy(&bytes[0x20..0x40])
        .trim_end_matches('\0')
        .trim_end()
        .to_string();

    let mut chips = Vec::new();
    let mut offset = header_len;
    while offset + CHIP_HEADER_LEN <= bytes.len() {
        if &bytes[offset..offset + 4] != CHIP_MAGIC {
            return Err(CrtParseError::TruncatedChip { offset });
        }
        let packet_len = be_u32(&bytes[offset + 4..offset + 8]) as usize;
        let chip_type = be_u16(&bytes[offset + 8..offset + 10]);
        let bank = be_u16(&bytes[offset + 10..offset + 12]);
        let load_address = be_u16(&bytes[offset + 12..offset + 14]);
        let image_size = be_u16(&bytes[offset + 14..offset + 16]) as usize;

        let data_start = offset + CHIP_HEADER_LEN;
        let data_end = data_start + image_size;
        // `packet_len` covers the 0x10 header + the image; sanity-check both it
        // and the advertised image size against the buffer.
        if packet_len < CHIP_HEADER_LEN
            || data_end > bytes.len()
            || offset + packet_len > bytes.len()
        {
            return Err(CrtParseError::TruncatedChip { offset });
        }
        chips.push(CrtChip {
            chip_type,
            bank,
            load_address,
            data: bytes[data_start..data_end].to_vec(),
        });
        offset += packet_len.max(CHIP_HEADER_LEN + image_size);
    }

    if chips.is_empty() {
        return Err(CrtParseError::NoChips);
    }

    Ok(CrtCartridge {
        hardware_type,
        exrom,
        game,
        name,
        chips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal CRT image: header + one CHIP packet.
    fn build_crt(exrom: u8, game: u8, hw: u16, load: u16, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(MAGIC);
        v.extend_from_slice(&0x40u32.to_be_bytes()); // header length
        v.extend_from_slice(&0x0100u16.to_be_bytes()); // version
        v.extend_from_slice(&hw.to_be_bytes());
        v.push(exrom);
        v.push(game);
        v.extend_from_slice(&[0u8; 6]); // reserved
        let mut name = *b"TESTCART\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
        name[0..8].copy_from_slice(b"TESTCART");
        v.extend_from_slice(&name); // 32-byte name
        assert_eq!(v.len(), 0x40);
        // CHIP packet.
        v.extend_from_slice(CHIP_MAGIC);
        v.extend_from_slice(&((CHIP_HEADER_LEN + data.len()) as u32).to_be_bytes());
        v.extend_from_slice(&0u16.to_be_bytes()); // ROM
        v.extend_from_slice(&0u16.to_be_bytes()); // bank 0
        v.extend_from_slice(&load.to_be_bytes());
        v.extend_from_slice(&(data.len() as u16).to_be_bytes());
        v.extend_from_slice(data);
        v
    }

    #[test]
    fn parses_plain_8k_rom() {
        let img = build_crt(0, 1, 0, 0x8000, &[0xAA; 0x2000]);
        let cart = parse(&img).expect("valid 8K CRT");
        assert_eq!(cart.hardware_type, 0);
        assert!(cart.exrom, "EXROM asserted");
        assert!(!cart.game, "GAME not asserted");
        assert_eq!(cart.mode(), CrtMode::Rom8k);
        assert_eq!(cart.name, "TESTCART");
        assert_eq!(cart.chips.len(), 1);
        assert_eq!(cart.chips[0].load_address, 0x8000);
        assert_eq!(cart.chips[0].data.len(), 0x2000);
    }

    #[test]
    fn mode_reflects_exrom_game_lines() {
        assert_eq!(
            parse(&build_crt(0, 0, 0, 0x8000, &[0; 0x4000]))
                .expect("valid 16K CRT")
                .mode(),
            CrtMode::Rom16k
        );
        assert_eq!(
            parse(&build_crt(1, 0, 0, 0x8000, &[0; 0x2000]))
                .expect("valid Ultimax CRT")
                .mode(),
            CrtMode::Ultimax
        );
    }

    #[test]
    fn rejects_bad_signature() {
        let mut img = build_crt(0, 1, 0, 0x8000, &[0; 0x2000]);
        img[0] = b'X';
        assert_eq!(parse(&img), Err(CrtParseError::InvalidSignature));
    }

    #[test]
    fn rejects_truncated_image() {
        assert!(matches!(
            parse(&[0u8; 8]),
            Err(CrtParseError::TooShort { .. })
        ));
    }

    #[test]
    fn rejects_truncated_chip_data() {
        let mut img = build_crt(0, 1, 0, 0x8000, &[0xAA; 0x2000]);
        img.truncate(0x40 + CHIP_HEADER_LEN + 0x100); // cut the ROM image short
        assert!(matches!(
            parse(&img),
            Err(CrtParseError::TruncatedChip { .. })
        ));
    }
}
