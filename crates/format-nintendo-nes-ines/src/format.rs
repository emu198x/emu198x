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
    /// Number of 16 KiB PRG ROM banks as reported by byte 4 of the
    /// header. For NES 2.0 the full size is in bytes.
    pub prg_rom_banks: u8,
    /// Number of 8 KiB CHR ROM banks. Zero means the cartridge has
    /// CHR RAM instead.
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
        // NES 2.0: 12-bit mapper number + extended 12-bit bank
        // counts (byte 9 holds the high nibbles of each).
        let mapper8 = data[8];
        let mapper_number =
            u16::from(mapper_lo) | u16::from(mapper_hi) | (u16::from(mapper8 & 0x0F) << 8);

        let prg_hi = usize::from(data[9] & 0x0F);
        let prg_size = ((prg_hi << 8) | usize::from(prg_banks)) * 16384;

        let chr_hi = usize::from((data[9] >> 4) & 0x0F);
        let chr_size = ((chr_hi << 8) | usize::from(chr_banks)) * 8192;

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
    let prg_start = if has_trainer { 16 + 512 } else { 16 };
    let chr_start = prg_start + prg_size;

    if data.len() < chr_start + chr_size {
        return Err(format!(
            "iNES file too short: expected {} bytes, got {}",
            chr_start + chr_size,
            data.len()
        ));
    }

    let prg_rom = data[prg_start..prg_start + prg_size].to_vec();
    let chr_data = if chr_size > 0 {
        data[chr_start..chr_start + chr_size].to_vec()
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
