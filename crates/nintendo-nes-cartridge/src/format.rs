//! Loading a board from an iNES / NES 2.0 file.
//!
//! `format-nintendo-nes-ines` reads the file's layout; this module picks the
//! board its mapper number names and fills it with the file's ROM.
//!
//! Both iNES 1.0 and NES 2.0 files map onto [`CartridgeHeader`]; the
//! NES 2.0 upper nibbles are folded into the 12-bit `mapper_number`
//! and the extended PRG/CHR sizes are reported via the byte counts the
//! parser passes to each mapper constructor.

use format_nintendo_nes_ines::HeaderMirroring;

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
    let image = format_nintendo_nes_ines::parse(data).map_err(|error| error.to_string())?;
    let mirroring = match image.header.mirroring {
        HeaderMirroring::Horizontal => Mirroring::Horizontal,
        HeaderMirroring::Vertical => Mirroring::Vertical,
        HeaderMirroring::FourScreen => Mirroring::FourScreen,
    };
    let has_battery = image.header.has_battery;
    let header = CartridgeHeader {
        prg_rom_banks: image.header.prg_rom_banks,
        chr_rom_banks: image.header.chr_rom_banks,
        mapper_number: image.header.mapper_number,
        mirroring,
        has_battery,
    };

    if image.header.is_nes_2_0 {
        validate_nes2_mapper_extents(
            header.mapper_number,
            image.prg_rom.len(),
            image.chr_rom.len(),
        )?;
    }

    let prg_rom = image.prg_rom.to_vec();
    let chr_data = image.chr_rom.to_vec(); // empty for a CHR RAM cartridge

    let mut mapper: Box<dyn Mapper> = match header.mapper_number {
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

    // The iNES trainer initializes $7000-$71FF before the CPU starts.
    // Use the existing RAM backing directly: power-on mapper write protection
    // must not discard loader data (Sunsoft-4 starts with PRG RAM disabled).
    // This supplies existing RAM, not an overlay for RAM-less boards.
    // Format-author specification: https://fms.komkon.org/EMUL8/NES.html
    if let Some(trainer) = image.trainer
        && mapper.save_ram().len() >= 0x2000
    {
        let mut ram = mapper.save_ram().to_vec();
        ram[0x1000..0x1200].copy_from_slice(trainer);
        mapper.restore_save_ram(&ram);
    }

    Ok(ParsedCartridge {
        mapper,
        has_battery,
        header,
    })
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
