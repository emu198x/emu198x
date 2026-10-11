//! iNES / NES 2.0 cartridge header parser.
//!
//! Both iNES 1.0 and NES 2.0 files map onto [`CartridgeHeader`]; the
//! NES 2.0 upper nibbles are folded into the 12-bit `mapper_number`
//! and the extended PRG/CHR sizes are reported via the byte counts the
//! parser passes to each mapper constructor.

use crate::mapper::{Mapper, Mirroring};
use crate::mappers::{
    action53::Action53, axrom::AxRom, bxrom::BxRom, camerica::Camerica, cnrom::CnRom,
    colordreams::ColorDreams, mmc1::Mmc1, mmc3::Mmc3, mmc5::Mmc5, nina001::Nina001, nrom::Nrom,
    sunsoft4::Sunsoft4, uxrom::UxRom, vrc2a::Vrc2a,
};

/// Parsed iNES header fields. Both iNES 1.0 and NES 2.0 files map
/// onto this struct; the NES 2.0 upper nibbles are folded into the
/// 12-bit `mapper_number` and the extended PRG/CHR sizes are
/// reported via the byte counts the parser passes to [`Nrom::new`].
#[derive(Debug, Clone, Copy)]
pub struct CartridgeHeader {
    /// Raw PRG size byte (header byte 4). This is a 16 KiB bank count
    /// for iNES; NES 2.0 can extend it or use exponent/multiplier encoding.
    pub prg_rom_banks: u8,
    /// Raw CHR size byte (header byte 5). This is an 8 KiB bank count
    /// for iNES; NES 2.0 can extend it or use exponent/multiplier encoding.
    pub chr_rom_banks: u8,
    /// 12-bit mapper number (iNES 1.0 uses only the low 8 bits).
    pub mapper_number: u16,
    /// Mirroring mode derived from header flag bits.
    pub mirroring: Mirroring,
    /// Whether the cartridge reports battery-backed PRG RAM.
    pub has_battery: bool,
}

/// Result of a successful [`parse_ines`] call.
///
/// The `mapper` field is boxed because different cartridges use
/// different mapper types; the machine layer only ever interacts
/// with the trait.
pub struct ParsedCartridge {
    /// The constructed mapper, ready to be plugged into the machine
    /// layer's `cpu_read`/`cpu_write`/`chr_read`/`chr_write`
    /// routers.
    pub mapper: Box<dyn Mapper>,
    /// Mirror of [`CartridgeHeader::has_battery`] — hoisted here
    /// because the machine layer needs it to decide whether to
    /// look for a `.sav` file.
    pub has_battery: bool,
    /// Full header, hoisted so callers can introspect the mapper
    /// number, CHR bank count, etc. without having to re-parse.
    pub header: CartridgeHeader,
}

/// Parse an iNES / NES 2.0 file and return a [`ParsedCartridge`]
/// ready to drive the NES machine layer.
///
/// # Errors
///
/// Returns a human-readable error string if:
/// - The file is shorter than 16 bytes (no header).
/// - The magic bytes are not `NES\x1A`.
/// - The declared PRG/CHR size exceeds the file's actual length.
/// - A NES 2.0 size or total ROM extent exceeds the host address space.
/// - A NES 2.0 payload cannot fill the selected mapper's bank windows.
/// - The mapper number is not supported by this port — see the
///   crate-level doc comment for the current scope.
pub fn parse_ines(data: &[u8]) -> Result<ParsedCartridge, String> {
    if data.len() < 16 {
        return Err("iNES file too short (< 16 bytes)".to_string());
    }
    if &data[0..4] != b"NES\x1a" {
        return Err("Invalid iNES magic (expected NES\\x1A)".to_string());
    }

    let prg_banks = data[4];
    let chr_banks = data[5];
    let flags6 = data[6];
    let flags7 = data[7];

    let mapper_lo = (flags6 >> 4) & 0x0F;
    let mapper_hi = flags7 & 0xF0;

    // NES 2.0 detection: bits 3-2 of flags7 == 0b10.
    let is_nes_2_0 = (flags7 & 0x0C) == 0x08;

    let (mapper_number, prg_size, chr_size) = if is_nes_2_0 {
        // NES 2.0: 12-bit mapper number. Each byte-9 nibble selects
        // either extended bank counts or exponent/multiplier byte sizes.
        let mapper8 = data[8];
        let mapper_number =
            u16::from(mapper_lo) | u16::from(mapper_hi) | (u16::from(mapper8 & 0x0F) << 8);

        let prg_size = nes2_rom_size(prg_banks, data[9] & 0x0F, 16384)
            .ok_or_else(|| "NES 2.0 PRG ROM size exceeds the address space".to_string())?;
        let chr_size = nes2_rom_size(chr_banks, data[9] >> 4, 8192)
            .ok_or_else(|| "NES 2.0 CHR ROM size exceeds the address space".to_string())?;

        (mapper_number, prg_size, chr_size)
    } else {
        // iNES 1.0: 8-bit mapper number.
        let mapper_number = u16::from(mapper_hi | mapper_lo);
        let prg_size = usize::from(prg_banks) * 16384;
        let chr_size = usize::from(chr_banks) * 8192;
        (mapper_number, prg_size, chr_size)
    };

    let mirroring = if flags6 & 0x08 != 0 {
        Mirroring::FourScreen
    } else if flags6 & 0x01 != 0 {
        Mirroring::Vertical
    } else {
        Mirroring::Horizontal
    };

    let has_battery = flags6 & 0x02 != 0;
    let has_trainer = flags6 & 0x04 != 0;

    let header = CartridgeHeader {
        prg_rom_banks: prg_banks,
        chr_rom_banks: chr_banks,
        mapper_number,
        mirroring,
        has_battery,
    };

    // iNES layout: [16-byte header][optional 512-byte trainer][PRG ROM][CHR ROM].
    let prg_start: usize = if has_trainer { 16 + 512 } else { 16 };
    let chr_start = prg_start
        .checked_add(prg_size)
        .ok_or_else(|| "iNES PRG ROM extent exceeds the address space".to_string())?;
    let rom_end = chr_start
        .checked_add(chr_size)
        .ok_or_else(|| "iNES CHR ROM extent exceeds the address space".to_string())?;

    if data.len() < rom_end {
        return Err(format!(
            "iNES file too short: expected {rom_end} bytes, got {}",
            data.len()
        ));
    }

    if is_nes_2_0 {
        validate_nes2_mapper_extents(mapper_number, prg_size, chr_size)?;
    }

    let prg_rom = data[prg_start..chr_start].to_vec();
    let chr_data = if chr_size > 0 {
        data[chr_start..rom_end].to_vec()
    } else {
        Vec::new() // CHR RAM cartridge
    };

    let mapper: Box<dyn Mapper> = match header.mapper_number {
        0 => Box::new(Nrom::new(prg_rom, chr_data, mirroring)),
        1 => Box::new(Mmc1::new(prg_rom, chr_data)),
        2 => Box::new(UxRom::new(prg_rom, chr_data, mirroring)),
        3 => Box::new(CnRom::new(prg_rom, chr_data, mirroring)),
        4 => Box::new(Mmc3::new(prg_rom, chr_data)),
        5 => Box::new(Mmc5::new(prg_rom, chr_data)),
        7 => Box::new(AxRom::new(prg_rom)),
        11 => Box::new(ColorDreams::new(prg_rom, chr_data, mirroring)),
        22 => Box::new(Vrc2a::new(prg_rom, chr_data)),
        28 => Box::new(Action53::new(prg_rom, chr_data)),
        34 if chr_data.is_empty() => Box::new(BxRom::new(prg_rom, mirroring)),
        34 => Box::new(Nina001::new(prg_rom, chr_data, mirroring)),
        68 => Box::new(Sunsoft4::new(prg_rom, chr_data, mirroring)),
        71 => Box::new(Camerica::new(prg_rom, mirroring)),
        n => {
            return Err(format!(
                "Unsupported mapper: {n} — this port currently carries Mapper 0 \
                 (NROM), Mapper 1 (MMC1), Mapper 2 (UxROM), and Mapper 3 \
                 (CNROM), Mapper 4 (MMC3), Mapper 5 (MMC5), Mapper 7 \
                 (AxROM), Mapper 11 (Color Dreams), Mapper 22 (VRC2a), \
                 Mapper 28 (Action 53), Mapper 34 (BxROM/BNROM and NINA-001), Mapper 68 \
                 (Sunsoft-4), and Mapper 71 (Camerica). Additional mappers \
                 will land as compatibility expands."
            ));
        }
    };

    Ok(ParsedCartridge {
        mapper,
        has_battery,
        header,
    })
}

/// NESdev NES 2.0, PRG-ROM/CHR-ROM size: an MSB nibble of $F makes
/// the low byte EEEEEE MM, denoting 2^E * (2*M + 1) bytes. The bank
/// unit applies only to linear encoding. See the primary reference
/// `reference/by-system/nintendo-nes/2026-nes2-rom-size-observations.md`.
fn nes2_rom_size(low: u8, high: u8, bank_size: usize) -> Option<usize> {
    if high == 0x0f {
        let power = 1usize.checked_shl(u32::from(low >> 2))?;
        power.checked_mul(usize::from((low & 3) * 2 + 1))
    } else {
        ((usize::from(high) << 8) | usize::from(low)).checked_mul(bank_size)
    }
}

/// Exponent encoding permits sub-bank payloads that the linear encoding
/// could not express. Do not construct a mapper whose existing bank-window
/// indexing can overrun them or omit a partial bank. These are implementation
/// limits, not restrictions on the NES 2.0 container format.
fn validate_nes2_mapper_extents(mapper: u16, prg: usize, chr: usize) -> Result<(), String> {
    let (prg_unit, chr_unit) = match mapper {
        0 => (1, 8192),
        1 => (16384, 1),
        2 => (16384, 8192),
        3 | 7 => (1, 1),
        4 | 22 => (8192, 1),
        5 => (8192, 8192),
        11 => (32768, 8192),
        28 => (16384, 8192),
        34 if chr == 0 => (32768, 1),
        34 => (32768, 4096),
        68 => (16384, 2048),
        71 => (16384, 1),
        _ => return Ok(()), // The mapper dispatch reports unsupported IDs.
    };
    if prg == 0 || !prg.is_multiple_of(prg_unit) || !chr.is_multiple_of(chr_unit) {
        return Err(format!(
            "Unsupported NES 2.0 ROM layout for mapper {mapper}: PRG {prg} bytes, \
             CHR {chr} bytes; this mapper requires nonzero PRG in {prg_unit}-byte \
             units and CHR in {chr_unit}-byte units (or no CHR ROM)"
        ));
    }
    Ok(())
}
