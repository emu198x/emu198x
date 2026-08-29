//! Commodore VIC-20 cartridge-image parsing.
//!
//! VICE `.crt` files carry a 64-byte `VIC20 CARTRIDGE ` header followed by
//! address-tagged `CHIP` packets. Raw 2/4/8 KiB images have no address metadata;
//! this parser treats those as the ordinary BLK5 game-cartridge window at
//! `$A000`, while multi-block images must use CRT so their wiring is explicit.

use thiserror::Error;

const MAGIC: &[u8; 16] = b"VIC20 CARTRIDGE ";
const CHIP_MAGIC: &[u8; 4] = b"CHIP";
const HEADER_LEN: usize = 0x40;
const CHIP_HEADER_LEN: usize = 0x10;

/// One address-tagged ROM image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CartridgeBlock {
    /// CPU address at which the image is decoded.
    pub load_address: u16,
    /// ROM bytes in address order.
    pub data: Vec<u8>,
}

/// A parsed generic VIC-20 cartridge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cartridge {
    /// VICE hardware type (`0` is a generic cartridge).
    pub hardware_type: u16,
    /// Header name, without trailing NUL or space padding.
    pub name: String,
    /// Address-tagged ROM blocks in file order.
    pub blocks: Vec<CartridgeBlock>,
}

impl Cartridge {
    /// Whether BLK5 carries the VIC-20 KERNAL's `A0` + high-bit `CBM` signature.
    #[must_use]
    pub fn is_autostart(&self) -> bool {
        self.blocks.iter().any(|block| {
            let start = usize::from(0xA004u16.saturating_sub(block.load_address));
            block.load_address <= 0xA004
                && start + 5 <= block.data.len()
                && block.data[start..start + 5] == [0x41, 0x30, 0xC3, 0xC2, 0xCD]
        })
    }
}

/// Cartridge parse failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// A non-CRT image is not one of the unambiguous BLK5 sizes.
    #[error("raw VIC-20 cartridge must be 2, 4, or 8 KiB; got {0} bytes")]
    UnsupportedRawSize(usize),
    /// The CRT fixed header is missing or malformed.
    #[error("VIC-20 CRT header is truncated or malformed")]
    InvalidHeader,
    /// The container is for a different machine.
    #[error("CRT image does not have the VIC20 CARTRIDGE signature")]
    InvalidSignature,
    /// Only generic, statically mapped cartridges are supported.
    #[error("unsupported VIC-20 cartridge hardware type {0}")]
    UnsupportedHardware(u16),
    /// A CHIP packet is malformed.
    #[error("CRT CHIP packet at offset {0} is truncated or malformed")]
    InvalidChip(usize),
    /// A CHIP packet maps outside BLK1/2/3/5.
    #[error("unsupported VIC-20 cartridge mapping ${address:04X} + {size} bytes")]
    UnsupportedMapping { address: u16, size: usize },
    /// The CRT carries no ROM packets.
    #[error("VIC-20 CRT contains no ROM CHIP packets")]
    NoBlocks,
}

fn be_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn supported_mapping(address: u16, size: usize) -> bool {
    let Some(end) = address.checked_add(u16::try_from(size).unwrap_or(u16::MAX)) else {
        return false;
    };
    matches!(address, 0x2000..=0x7FFF | 0xA000..=0xBFFF)
        && matches!(end.saturating_sub(1), 0x2000..=0x7FFF | 0xA000..=0xBFFF)
        && !(address < 0x8000 && end > 0x8000)
}

/// Parse a VICE CRT or an ordinary raw BLK5 ROM.
///
/// # Errors
///
/// Returns an error for malformed CRT data, bank-switching hardware, mappings
/// outside BLK1/2/3/5, or ambiguous raw image sizes.
pub fn parse(bytes: &[u8]) -> Result<Cartridge, ParseError> {
    if !bytes.starts_with(MAGIC) {
        return match bytes.len() {
            0x0800 | 0x1000 | 0x2000 => Ok(Cartridge {
                hardware_type: 0,
                name: "raw BLK5 cartridge".to_owned(),
                blocks: vec![CartridgeBlock {
                    load_address: 0xA000,
                    data: bytes.to_vec(),
                }],
            }),
            size if size >= MAGIC.len() && bytes[..MAGIC.len()].contains(&b' ') => {
                Err(ParseError::InvalidSignature)
            }
            size => Err(ParseError::UnsupportedRawSize(size)),
        };
    }
    if bytes.len() < HEADER_LEN {
        return Err(ParseError::InvalidHeader);
    }
    let header_len = be_u32(&bytes[0x10..0x14]) as usize;
    if header_len < HEADER_LEN || header_len > bytes.len() {
        return Err(ParseError::InvalidHeader);
    }
    let hardware_type = be_u16(&bytes[0x16..0x18]);
    if hardware_type != 0 {
        return Err(ParseError::UnsupportedHardware(hardware_type));
    }
    let name = String::from_utf8_lossy(&bytes[0x20..0x40])
        .trim_end_matches('\0')
        .trim_end()
        .to_owned();

    let mut blocks = Vec::new();
    let mut offset = header_len;
    while offset < bytes.len() {
        if offset + CHIP_HEADER_LEN > bytes.len() || &bytes[offset..offset + 4] != CHIP_MAGIC {
            return Err(ParseError::InvalidChip(offset));
        }
        let packet_len = be_u32(&bytes[offset + 4..offset + 8]) as usize;
        let chip_type = be_u16(&bytes[offset + 8..offset + 10]);
        let load_address = be_u16(&bytes[offset + 12..offset + 14]);
        let image_size = be_u16(&bytes[offset + 14..offset + 16]) as usize;
        let data_start = offset + CHIP_HEADER_LEN;
        let data_end = data_start.saturating_add(image_size);
        if packet_len < CHIP_HEADER_LEN + image_size
            || offset.saturating_add(packet_len) > bytes.len()
            || data_end > bytes.len()
        {
            return Err(ParseError::InvalidChip(offset));
        }
        if chip_type == 0 || chip_type == 2 {
            if !supported_mapping(load_address, image_size) {
                return Err(ParseError::UnsupportedMapping {
                    address: load_address,
                    size: image_size,
                });
            }
            blocks.push(CartridgeBlock {
                load_address,
                data: bytes[data_start..data_end].to_vec(),
            });
        }
        offset += packet_len;
    }
    if blocks.is_empty() {
        return Err(ParseError::NoBlocks);
    }
    Ok(Cartridge {
        hardware_type,
        name,
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crt(chips: &[(u16, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::from(*MAGIC);
        bytes.extend_from_slice(&0x40u32.to_be_bytes());
        bytes.extend_from_slice(&0x0100u16.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&[0; 32]);
        for (address, data) in chips {
            bytes.extend_from_slice(CHIP_MAGIC);
            bytes.extend_from_slice(&((CHIP_HEADER_LEN + data.len()) as u32).to_be_bytes());
            bytes.extend_from_slice(&0u16.to_be_bytes());
            bytes.extend_from_slice(&0u16.to_be_bytes());
            bytes.extend_from_slice(&address.to_be_bytes());
            bytes.extend_from_slice(&(data.len() as u16).to_be_bytes());
            bytes.extend_from_slice(data);
        }
        bytes
    }

    #[test]
    fn parses_raw_blk5_and_detects_autostart() {
        let mut raw = vec![0; 0x2000];
        raw[4..9].copy_from_slice(&[0x41, 0x30, 0xC3, 0xC2, 0xCD]);
        let cart = parse(&raw).expect("raw BLK5 cart");
        assert_eq!(cart.blocks[0].load_address, 0xA000);
        assert!(cart.is_autostart());
    }

    #[test]
    fn parses_addressed_multi_block_crt() {
        let bytes = crt(&[(0x2000, &[1; 0x2000]), (0xA000, &[2; 0x2000])]);
        let cart = parse(&bytes).expect("generic multi-block cart");
        assert_eq!(cart.blocks.len(), 2);
        assert_eq!(cart.blocks[0].load_address, 0x2000);
        assert_eq!(cart.blocks[1].load_address, 0xA000);
    }

    #[test]
    fn rejects_ambiguous_raw_and_out_of_range_crt() {
        assert_eq!(
            parse(&[0; 0x3000]),
            Err(ParseError::UnsupportedRawSize(0x3000))
        );
        assert!(matches!(
            parse(&crt(&[(0x8000, &[0; 0x2000])])),
            Err(ParseError::UnsupportedMapping { .. })
        ));
    }
}
