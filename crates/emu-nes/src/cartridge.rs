//! iNES cartridge parser and mapper implementations.
//!
//! Parses the iNES file format (header + PRG ROM + CHR ROM) and provides
//! a `Mapper` trait for address translation. Only NROM (Mapper 0) is
//! implemented for v1.

#![allow(clippy::cast_possible_truncation)]

/// Nametable mirroring mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    FourScreen,
}

/// Parsed iNES file header.
#[derive(Debug)]
#[allow(dead_code)]
pub struct CartridgeHeader {
    pub prg_rom_banks: u8,
    pub chr_rom_banks: u8,
    pub mapper_number: u8,
    pub mirroring: Mirroring,
    pub has_battery: bool,
}

/// Mapper trait: translates CPU and PPU addresses to cartridge ROM/RAM.
pub trait Mapper {
    fn cpu_read(&self, addr: u16) -> u8;
    fn cpu_write(&mut self, addr: u16, value: u8);
    fn chr_read(&self, addr: u16) -> u8;
    fn chr_write(&mut self, addr: u16, value: u8);
    fn mirroring(&self) -> Mirroring;
}

/// NROM (Mapper 0): no bank switching.
///
/// - PRG: 16K mirrored at $8000-$FFFF, or 32K at $8000-$FFFF
/// - CHR: 8K at PPU $0000-$1FFF (ROM or RAM if `chr_rom_banks` == 0)
pub struct Nrom {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
}

impl Nrom {
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192] // 8K CHR RAM
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            mirroring,
        }
    }
}

impl Mapper for Nrom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                if self.prg_rom.len() == 16384 {
                    // 16K: mirror $8000-$BFFF to $C000-$FFFF
                    self.prg_rom[offset % 16384]
                } else {
                    // 32K: direct mapping
                    self.prg_rom[offset % self.prg_rom.len()]
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, _addr: u16, _value: u8) {
        // NROM has no writable PRG area
    }

    fn chr_read(&self, addr: u16) -> u8 {
        self.chr[(addr as usize) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr[(addr as usize) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Parse an iNES file and return a boxed mapper.
///
/// # Errors
///
/// Returns an error string if the header is invalid or the mapper is unsupported.
pub fn parse_ines(data: &[u8]) -> Result<Box<dyn Mapper>, String> {
    if data.len() < 16 {
        return Err("iNES file too short (< 16 bytes)".to_string());
    }

    // Check magic: "NES\x1A"
    if &data[0..4] != b"NES\x1a" {
        return Err("Invalid iNES magic (expected NES\\x1A)".to_string());
    }

    let prg_banks = data[4];
    let chr_banks = data[5];
    let flags6 = data[6];
    let flags7 = data[7];

    let mapper_lo = (flags6 >> 4) & 0x0F;
    let mapper_hi = flags7 & 0xF0;
    let mapper_number = mapper_hi | mapper_lo;

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

    let prg_size = usize::from(prg_banks) * 16384;
    let chr_size = usize::from(chr_banks) * 8192;

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
        Vec::new() // CHR RAM
    };

    match header.mapper_number {
        0 => Ok(Box::new(Nrom::new(prg_rom, chr_data, mirroring))),
        n => Err(format!("Unsupported mapper: {n}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ines(prg_banks: u8, chr_banks: u8, flags6: u8) -> Vec<u8> {
        let prg_size = usize::from(prg_banks) * 16384;
        let chr_size = usize::from(chr_banks) * 8192;
        let mut data = vec![0u8; 16 + prg_size + chr_size];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = prg_banks;
        data[5] = chr_banks;
        data[6] = flags6;
        // Fill PRG with a recognizable pattern
        for i in 0..prg_size {
            data[16 + i] = (i & 0xFF) as u8;
        }
        // Fill CHR with a different pattern
        for i in 0..chr_size {
            data[16 + prg_size + i] = ((i + 0x80) & 0xFF) as u8;
        }
        data
    }

    #[test]
    fn parse_valid_nrom_16k() {
        let data = make_ines(1, 1, 0x00); // Horizontal mirroring
        let mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        // PRG at $8000 should be byte 0 of PRG ROM
        assert_eq!(mapper.cpu_read(0x8000), 0x00);
        // 16K mirrored: $C000 should mirror $8000
        assert_eq!(mapper.cpu_read(0xC000), 0x00);
    }

    #[test]
    fn parse_valid_nrom_32k() {
        let data = make_ines(2, 1, 0x01); // Vertical mirroring
        let mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
        assert_eq!(mapper.cpu_read(0x8000), 0x00);
        // $C000 maps to bank 1 start
        assert_eq!(mapper.cpu_read(0xC000), 0x00); // offset 0x4000 & 0xFF = 0
    }

    #[test]
    fn chr_read_write_ram() {
        let data = make_ines(1, 0, 0x00); // CHR RAM (0 banks)
        let mut mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.chr_read(0x0000), 0);
        mapper.chr_write(0x0000, 0xAB);
        assert_eq!(mapper.chr_read(0x0000), 0xAB);
    }

    #[test]
    fn chr_rom_not_writable() {
        let data = make_ines(1, 1, 0x00); // CHR ROM (1 bank)
        let mut mapper = parse_ines(&data).expect("parse failed");
        let original = mapper.chr_read(0x0000);
        mapper.chr_write(0x0000, 0xFF);
        assert_eq!(mapper.chr_read(0x0000), original); // Unchanged
    }

    #[test]
    fn invalid_magic() {
        let data = vec![0u8; 32];
        assert!(parse_ines(&data).is_err());
    }

    #[test]
    fn unsupported_mapper() {
        let mut data = make_ines(1, 1, 0x00);
        data[6] = 0x10; // Mapper 1 (low nibble)
        assert!(parse_ines(&data).is_err());
    }
}
