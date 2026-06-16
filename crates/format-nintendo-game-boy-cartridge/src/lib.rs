//! Game Boy cartridge header parser.
//!
//! The DMG / CGB cartridge header lives at ROM offsets `$0100..=$014F`.
//! This crate decodes it into a [`CartridgeHeader`], and pairs that
//! with the ROM bytes to construct a fully-loaded
//! [`Cartridge`](nintendo_game_boy_mbc::Cartridge) from the
//! [`nintendo-game-boy-mbc`] crate.
//!
//! Header layout (Pan Docs §11):
//!
//! | Range            | Field                                            |
//! |------------------|--------------------------------------------------|
//! | `$0100..=$0103`  | entry point (typically `NOP; JP $0150`)          |
//! | `$0104..=$0133`  | Nintendo logo (48 bytes — boot ROM checks this)  |
//! | `$0134..=$0143`  | title (16 bytes; later carts use 11 + CGB flag)  |
//! | `$0143`          | CGB flag (`$80` = CGB-supported, `$C0` = CGB-only)|
//! | `$0144..=$0145`  | new licensee code (2 ASCII bytes)                |
//! | `$0146`          | SGB flag (`$03` = SGB-aware)                     |
//! | `$0147`          | cartridge type byte → [`CartType`]               |
//! | `$0148`          | ROM size code (`32 KiB << code`)                 |
//! | `$0149`          | RAM size code → external-RAM bytes               |
//! | `$014A`          | destination code (`$00` = Japan, `$01` = world)  |
//! | `$014B`          | old licensee code                                |
//! | `$014C`          | mask ROM version                                 |
//! | `$014D`          | header checksum (sum of `$0134..=$014C` bytes)   |
//! | `$014E..=$014F`  | global checksum (16-bit big-endian)              |

use nintendo_game_boy_mbc::{CartType, Cartridge};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Smallest legal Game Boy ROM (header + a single 32 KiB bank).
const MIN_ROM_SIZE: usize = 0x8000;

/// Header offsets relative to the start of the ROM image.
mod offset {
    pub const TITLE: usize = 0x0134;
    pub const TITLE_END: usize = 0x0144;
    pub const CGB_FLAG: usize = 0x0143;
    pub const NEW_LICENSEE: usize = 0x0144;
    pub const SGB_FLAG: usize = 0x0146;
    pub const CART_TYPE: usize = 0x0147;
    pub const ROM_SIZE: usize = 0x0148;
    pub const RAM_SIZE: usize = 0x0149;
    pub const DESTINATION: usize = 0x014A;
    pub const OLD_LICENSEE: usize = 0x014B;
    pub const MASK_VERSION: usize = 0x014C;
    pub const HEADER_CHECKSUM: usize = 0x014D;
    pub const GLOBAL_CHECKSUM: usize = 0x014E;
    pub const HEADER_CHECKSUM_RANGE_START: usize = 0x0134;
    pub const HEADER_CHECKSUM_RANGE_END: usize = 0x014C;
}

/// Decoded cartridge header.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CartridgeHeader {
    /// Title bytes as read from the ROM, trimmed of trailing zeros.
    /// On post-CGB carts the field shrinks (the last 5 bytes become
    /// licensee + CGB flag); we still expose the trimmed title.
    pub title: String,
    /// CGB-compatibility flag. `$80` = CGB-aware (still runs on
    /// DMG); `$C0` = CGB-only.
    pub cgb_flag: u8,
    /// New licensee code (two ASCII bytes). Only meaningful when the
    /// old licensee code is `$33`.
    pub new_licensee: [u8; 2],
    /// SGB flag. `$03` = SGB-aware.
    pub sgb_flag: u8,
    /// Decoded cartridge type.
    pub cart_type: CartType,
    /// ROM size in bytes (`32 KiB << code` for codes 0..=8).
    pub rom_size: usize,
    /// External RAM size in bytes (0 / 2 KiB / 8 KiB / 32 KiB / 64 KiB / 128 KiB).
    pub ram_size: usize,
    /// Destination code: `$00` = Japan, `$01` = overseas.
    pub destination: u8,
    /// Old licensee code; `$33` indicates the new code at $0144 is in
    /// use.
    pub old_licensee: u8,
    /// Mask ROM version number (usually `$00`).
    pub mask_version: u8,
    /// Stored header checksum byte at `$014D`.
    pub header_checksum: u8,
    /// Stored global checksum (`$014E..=$014F`, big-endian).
    pub global_checksum: u16,
}

impl CartridgeHeader {
    /// Parse the header from a complete ROM image.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderError`] when the ROM is too small, the cart
    /// type byte denotes a mapper this crate doesn't yet support,
    /// the ROM-size code is invalid, the RAM-size code is unknown,
    /// the actual ROM length doesn't match the declared size, or the
    /// header checksum is wrong.
    pub fn parse(rom: &[u8]) -> Result<Self, HeaderError> {
        if rom.len() < MIN_ROM_SIZE {
            return Err(HeaderError::TooShort { len: rom.len() });
        }

        let cart_type = decode_cart_type(rom[offset::CART_TYPE])?;
        let rom_size = decode_rom_size(rom[offset::ROM_SIZE])?;
        let ram_size = decode_ram_size(rom[offset::RAM_SIZE], cart_type)?;

        if rom.len() != rom_size {
            return Err(HeaderError::RomLengthMismatch {
                declared: rom_size,
                actual: rom.len(),
            });
        }

        let header_checksum = rom[offset::HEADER_CHECKSUM];
        let computed = compute_header_checksum(rom);
        if header_checksum != computed {
            return Err(HeaderError::HeaderChecksumMismatch {
                stored: header_checksum,
                computed,
            });
        }

        let cgb_flag = rom[offset::CGB_FLAG];
        let title = decode_title(&rom[offset::TITLE..offset::TITLE_END], cgb_flag);
        let new_licensee = [rom[offset::NEW_LICENSEE], rom[offset::NEW_LICENSEE + 1]];
        let global_checksum = u16::from_be_bytes([
            rom[offset::GLOBAL_CHECKSUM],
            rom[offset::GLOBAL_CHECKSUM + 1],
        ]);

        Ok(Self {
            title,
            cgb_flag,
            new_licensee,
            sgb_flag: rom[offset::SGB_FLAG],
            cart_type,
            rom_size,
            ram_size,
            destination: rom[offset::DESTINATION],
            old_licensee: rom[offset::OLD_LICENSEE],
            mask_version: rom[offset::MASK_VERSION],
            header_checksum,
            global_checksum,
        })
    }
}

/// Parse the header and build the loaded [`Cartridge`].
///
/// # Errors
///
/// As [`CartridgeHeader::parse`].
pub fn load(rom: Vec<u8>) -> Result<(CartridgeHeader, Cartridge), HeaderError> {
    let header = CartridgeHeader::parse(&rom)?;
    let cart = Cartridge::new(rom, header.cart_type, header.ram_size);
    Ok((header, cart))
}

/// Header-decoding errors.
#[derive(Debug, Eq, PartialEq, Error)]
pub enum HeaderError {
    /// ROM is shorter than the minimum 32 KiB header + bank-0 image.
    #[error("ROM too short — got {len} bytes, need at least {MIN_ROM_SIZE}")]
    TooShort { len: usize },
    /// Cart-type byte denotes a mapper this crate doesn't (yet)
    /// implement.
    #[error("unsupported cartridge type ${byte:02X} ({name})")]
    UnsupportedCartType { byte: u8, name: &'static str },
    /// Cart-type byte isn't in any documented Pan Docs slot.
    #[error("unknown cartridge type byte ${byte:02X}")]
    UnknownCartType { byte: u8 },
    /// ROM-size code is outside the documented 0..=8 range.
    #[error("invalid ROM size code ${code:02X}")]
    InvalidRomSize { code: u8 },
    /// RAM-size code isn't one of the known values.
    #[error("invalid RAM size code ${code:02X}")]
    InvalidRamSize { code: u8 },
    /// File length doesn't match the declared ROM-size code.
    #[error("ROM length mismatch — header says {declared} bytes, file has {actual}")]
    RomLengthMismatch { declared: usize, actual: usize },
    /// Header checksum byte at $014D is wrong. The boot ROM will
    /// refuse to start a cart that fails this — we surface the
    /// failure rather than silently masking it.
    #[error("header checksum mismatch — stored ${stored:02X}, computed ${computed:02X}")]
    HeaderChecksumMismatch { stored: u8, computed: u8 },
}

fn decode_cart_type(byte: u8) -> Result<CartType, HeaderError> {
    match byte {
        0x00 => Ok(CartType::RomOnly { battery: false }),
        0x01 => Ok(CartType::Mbc1 {
            ram: false,
            battery: false,
        }),
        0x02 => Ok(CartType::Mbc1 {
            ram: true,
            battery: false,
        }),
        0x03 => Ok(CartType::Mbc1 {
            ram: true,
            battery: true,
        }),
        0x05 => Ok(CartType::Mbc2 { battery: false }),
        0x06 => Ok(CartType::Mbc2 { battery: true }),
        // ROM+RAM (rare, 32 KiB ROMs). No MBC — the cartridge wires RAM
        // directly and the ram-size byte drives the allocation. $09 adds a
        // battery, so its RAM persists to the `.sav` sidecar; $08 does not.
        0x08 => Ok(CartType::RomOnly { battery: false }),
        0x09 => Ok(CartType::RomOnly { battery: true }),
        0x0F => Ok(CartType::Mbc3 {
            ram: false,
            battery: true,
            rtc: true,
        }),
        0x10 => Ok(CartType::Mbc3 {
            ram: true,
            battery: true,
            rtc: true,
        }),
        0x11 => Ok(CartType::Mbc3 {
            ram: false,
            battery: false,
            rtc: false,
        }),
        0x12 => Ok(CartType::Mbc3 {
            ram: true,
            battery: false,
            rtc: false,
        }),
        0x13 => Ok(CartType::Mbc3 {
            ram: true,
            battery: true,
            rtc: false,
        }),
        0x19 => Ok(CartType::Mbc5 {
            ram: false,
            battery: false,
            rumble: false,
        }),
        0x1A => Ok(CartType::Mbc5 {
            ram: true,
            battery: false,
            rumble: false,
        }),
        0x1B => Ok(CartType::Mbc5 {
            ram: true,
            battery: true,
            rumble: false,
        }),
        0x1C => Ok(CartType::Mbc5 {
            ram: false,
            battery: false,
            rumble: true,
        }),
        0x1D => Ok(CartType::Mbc5 {
            ram: true,
            battery: false,
            rumble: true,
        }),
        0x1E => Ok(CartType::Mbc5 {
            ram: true,
            battery: true,
            rumble: true,
        }),
        // Documented but not (yet) supported.
        0x0B..=0x0D => Err(HeaderError::UnsupportedCartType {
            byte,
            name: "MMM01",
        }),
        0x20 => Err(HeaderError::UnsupportedCartType { byte, name: "MBC6" }),
        0x22 => Err(HeaderError::UnsupportedCartType {
            byte,
            name: "MBC7+SENSOR+RUMBLE+RAM+BATTERY",
        }),
        0xFC => Err(HeaderError::UnsupportedCartType {
            byte,
            name: "POCKET CAMERA",
        }),
        0xFD => Err(HeaderError::UnsupportedCartType {
            byte,
            name: "BANDAI TAMA5",
        }),
        0xFE => Err(HeaderError::UnsupportedCartType { byte, name: "HuC3" }),
        0xFF => Err(HeaderError::UnsupportedCartType {
            byte,
            name: "HuC1+RAM+BATTERY",
        }),
        _ => Err(HeaderError::UnknownCartType { byte }),
    }
}

fn decode_rom_size(code: u8) -> Result<usize, HeaderError> {
    if code > 8 {
        return Err(HeaderError::InvalidRomSize { code });
    }
    Ok(0x8000usize << code)
}

fn decode_ram_size(code: u8, cart_type: CartType) -> Result<usize, HeaderError> {
    // MBC2's "RAM" lives on the mapper chip (512×4 bits = 512 bytes).
    // The header byte is conventionally `$00`.
    if matches!(cart_type, CartType::Mbc2 { .. }) {
        return Ok(0x200);
    }
    Ok(match code {
        0 => 0,
        1 => 0x800,   // 2 KiB (rare unofficial use)
        2 => 0x2000,  // 8 KiB
        3 => 0x8000,  // 32 KiB (4 banks of 8 KiB)
        4 => 0x20000, // 128 KiB (16 banks)
        5 => 0x10000, // 64 KiB (8 banks)
        _ => return Err(HeaderError::InvalidRamSize { code }),
    })
}

fn compute_header_checksum(rom: &[u8]) -> u8 {
    let mut checksum: u8 = 0;
    for &byte in &rom[offset::HEADER_CHECKSUM_RANGE_START..=offset::HEADER_CHECKSUM_RANGE_END] {
        checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
    }
    checksum
}

fn decode_title(bytes: &[u8], cgb_flag: u8) -> String {
    // CGB-aware carts moved the last 4-5 bytes for the manufacturer
    // code + CGB flag. If the CGB flag is set, only the first 11
    // bytes are title.
    let effective_len = if cgb_flag == 0x80 || cgb_flag == 0xC0 {
        11
    } else {
        bytes.len()
    };
    let trimmed: Vec<u8> = bytes[..effective_len.min(bytes.len())]
        .iter()
        .copied()
        .take_while(|&b| b != 0)
        .filter(|&b| (0x20..=0x7E).contains(&b))
        .collect();
    String::from_utf8(trimmed).unwrap_or_default()
}

#[cfg(test)]
mod tests;
