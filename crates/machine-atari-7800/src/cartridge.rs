//! Atari 7800 cartridge handling.
//!
//! A78 images are configured from their header. Headerless dumps retain a
//! size-based fallback for flat ROM and the original SuperGame mapper.

use serde::{Deserialize, Serialize};

const HEADER_LEN: usize = 128;
const MAGIC: &[u8; 9] = b"ATARI7800";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum BankingScheme {
    Flat { base: u16 },
    SuperGame { option: SuperGameOption },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SuperGameOption {
    None,
    Ram16K,
    ExRom,
    ExFix,
}

/// Address at which the A78 header says the cartridge's POKEY is decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PokeyLocation {
    Addr0440,
    Addr0450,
    Addr0800,
    Addr4000,
}

impl PokeyLocation {
    #[must_use]
    pub const fn base(self) -> u16 {
        match self {
            Self::Addr0440 => 0x0440,
            Self::Addr0450 => 0x0450,
            Self::Addr0800 => 0x0800,
            Self::Addr4000 => 0x4000,
        }
    }
}

#[derive(Clone, Copy)]
struct Header {
    rom_size: usize,
    mapper: u8,
    options: u8,
    pokey: Option<PokeyLocation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Cartridge {
    rom: Vec<u8>,
    banking: BankingScheme,
    bank: usize,
    ram: Vec<u8>,
    pokey: Option<PokeyLocation>,
}

impl Cartridge {
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        let (body, header) = match parse_header(data)? {
            Some((header, body)) => (body, Some(header)),
            None => (data, None),
        };
        let (banking, ram_size, pokey) = if let Some(header) = header {
            if header.rom_size != body.len() {
                return Err(format!(
                    "A78 header declares {} ROM bytes, file contains {}",
                    header.rom_size,
                    body.len()
                ));
            }
            let (banking, ram_size) = layout_from_header(header)?;
            (banking, ram_size, header.pokey)
        } else {
            (layout_from_size(body.len())?, 0, None)
        };

        let capacity = match banking {
            BankingScheme::Flat { base } => 0x1_0000 - usize::from(base),
            BankingScheme::SuperGame { .. } => 0x2_0000,
        };
        if body.len() > capacity {
            return Err(format!(
                "Cartridge mapper accepts at most {capacity} ROM bytes, got {}",
                body.len()
            ));
        }
        let mut rom = vec![0xFF; capacity];
        let start = if matches!(banking, BankingScheme::Flat { .. }) {
            capacity - body.len()
        } else {
            0
        };
        rom[start..start + body.len()].copy_from_slice(body);
        Ok(Self {
            rom,
            banking,
            bank: 0,
            ram: vec![0; ram_size],
            pokey,
        })
    }

    #[must_use]
    pub const fn pokey_location(&self) -> Option<PokeyLocation> {
        self.pokey
    }

    pub fn read(&self, addr: u16) -> u8 {
        match self.banking {
            BankingScheme::Flat { base } => {
                if addr < base {
                    0xFF
                } else {
                    self.rom
                        .get((addr - base) as usize)
                        .copied()
                        .unwrap_or(0xFF)
                }
            }
            BankingScheme::SuperGame { option } => {
                if addr >= 0xC000 {
                    self.rom
                        .get(7 * 0x4000 + (addr as usize - 0xC000))
                        .copied()
                        .unwrap_or(0xFF)
                } else if addr >= 0x8000 {
                    self.rom
                        .get(self.bank * 0x4000 + (addr as usize - 0x8000))
                        .copied()
                        .unwrap_or(0xFF)
                } else if addr >= 0x4000 {
                    match option {
                        SuperGameOption::None => 0xFF,
                        SuperGameOption::Ram16K => self.ram[addr as usize - 0x4000],
                        SuperGameOption::ExRom => self
                            .rom
                            .get(addr as usize - 0x4000)
                            .copied()
                            .unwrap_or(0xFF),
                        SuperGameOption::ExFix => self
                            .rom
                            .get(6 * 0x4000 + (addr as usize - 0x4000))
                            .copied()
                            .unwrap_or(0xFF),
                    }
                } else {
                    0xFF
                }
            }
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        if let BankingScheme::SuperGame { option } = self.banking {
            if (0x8000..0xC000).contains(&addr) {
                self.bank = (value as usize) & 7;
            } else if option == SuperGameOption::Ram16K && (0x4000..0x8000).contains(&addr) {
                self.ram[addr as usize - 0x4000] = value;
            }
        }
    }
}

fn parse_header(data: &[u8]) -> Result<Option<(Header, &[u8])>, String> {
    if data.len() < HEADER_LEN || data.get(1..10) != Some(MAGIC.as_slice()) {
        return Ok(None);
    }
    let version = data[0];
    if !(1..=4).contains(&version) {
        return Err(format!("Unsupported A78 header version: {version}"));
    }
    let rom_size = u32::from_be_bytes([data[49], data[50], data[51], data[52]]) as usize;
    let legacy = u16::from_be_bytes([data[53], data[54]]);
    let (mapper, options, pokey) = if version >= 4 {
        let audio = u16::from_be_bytes([data[66], data[67]]);
        let pokey = decode_pokey(audio & 7, true)?;
        if audio & !7 != 0 {
            return Err(format!("Unsupported A78 audio features: ${audio:04X}"));
        }
        (data[64], data[65], pokey)
    } else {
        let mapper = if legacy & 0x0002 != 0 {
            1
        } else if legacy & 0x0100 != 0 {
            2
        } else if legacy & 0x0200 != 0 {
            3
        } else if legacy & 0x1000 != 0 {
            4
        } else {
            0
        };
        let options = legacy_options(legacy)?;
        let selector = match legacy & (0x0001 | 0x0040 | 0x0400 | 0x8000) {
            0 => 0,
            0x0400 => 1,
            0x0040 => 2,
            0x8000 => 4,
            0x0001 => 5,
            _ => 3,
        };
        (mapper, options, decode_pokey(selector, false)?)
    };
    Ok(Some((
        Header {
            rom_size,
            mapper,
            options,
            pokey,
        },
        &data[HEADER_LEN..],
    )))
}

fn decode_pokey(selector: u16, v4: bool) -> Result<Option<PokeyLocation>, String> {
    match selector {
        0 => Ok(None),
        1 => Ok(Some(PokeyLocation::Addr0440)),
        2 => Ok(Some(PokeyLocation::Addr0450)),
        4 => Ok(Some(PokeyLocation::Addr0800)),
        5 => Ok(Some(PokeyLocation::Addr4000)),
        3 if v4 => Err("Dual-POKEY A78 cartridges are not yet supported".into()),
        3 => Err("Multiple POKEY locations in A78 header are not supported".into()),
        value => Err(format!("Unknown A78 POKEY selector: {value}")),
    }
}

fn legacy_options(flags: u16) -> Result<u8, String> {
    if flags & (0x0020 | 0x0080 | 0x2000 | 0x4000) != 0 {
        return Err(format!("Unsupported A78 RAM/bankset flags: ${flags:04X}"));
    }
    match flags & (0x0004 | 0x0008 | 0x0010) {
        0 => Ok(0),
        0x0004 => Ok(1),
        0x0008 => Ok(4),
        0x0010 => Ok(5),
        _ => Err(format!("Conflicting A78 mapper options: ${flags:04X}")),
    }
}

fn layout_from_header(header: Header) -> Result<(BankingScheme, usize), String> {
    match header.mapper {
        0 if header.options == 0 => Ok((layout_from_size(header.rom_size)?, 0)),
        0 => Err(format!(
            "Unsupported A78 linear mapper options: ${:02X}",
            header.options
        )),
        1 => {
            if header.options & 0x80 != 0 {
                return Err("A78 bankset ROM is not yet supported".into());
            }
            let (option, ram_size) = match header.options {
                0 => (SuperGameOption::None, 0),
                1 => (SuperGameOption::Ram16K, 0x4000),
                4 => (SuperGameOption::ExRom, 0),
                5 => (SuperGameOption::ExFix, 0),
                value => return Err(format!("Unsupported A78 SuperGame option: {value}")),
            };
            Ok((BankingScheme::SuperGame { option }, ram_size))
        }
        mapper => Err(format!("Unsupported A78 mapper: {mapper}")),
    }
}

fn layout_from_size(size: usize) -> Result<BankingScheme, String> {
    match size {
        0..=16_384 => Ok(BankingScheme::Flat { base: 0xC000 }),
        16_385..=32_768 => Ok(BankingScheme::Flat { base: 0x8000 }),
        32_769..=49_152 => Ok(BankingScheme::Flat { base: 0x4000 }),
        49_153..=131_072 => Ok(BankingScheme::SuperGame {
            option: SuperGameOption::ExFix,
        }),
        other => Err(format!("Unsupported cartridge size: {other} bytes")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a78(version: u8, rom: &[u8], legacy: u16, mapper: u8, options: u8, audio: u16) -> Vec<u8> {
        let mut image = vec![0; HEADER_LEN];
        image[0] = version;
        image[1..10].copy_from_slice(MAGIC);
        image[49..53].copy_from_slice(&(rom.len() as u32).to_be_bytes());
        image[53..55].copy_from_slice(&legacy.to_be_bytes());
        image[64] = mapper;
        image[65] = options;
        image[66..68].copy_from_slice(&audio.to_be_bytes());
        image.extend_from_slice(rom);
        image
    }

    #[test]
    fn headerless_sizes_select_legacy_layouts() {
        assert_eq!(
            Cartridge::from_rom(&vec![0; 16_384])
                .expect("16K cart")
                .banking,
            BankingScheme::Flat { base: 0xC000 }
        );
        assert_eq!(
            Cartridge::from_rom(&vec![0; 32_768])
                .expect("32K cart")
                .banking,
            BankingScheme::Flat { base: 0x8000 }
        );
        assert!(matches!(
            Cartridge::from_rom(&vec![0; 131_072])
                .expect("128K cart")
                .banking,
            BankingScheme::SuperGame { .. }
        ));
    }

    #[test]
    fn canonical_a78_magic_is_stripped() {
        let mut rom = vec![0; 32_768];
        rom[0] = 0x42;
        let cart = Cartridge::from_rom(&a78(3, &rom, 0, 0, 0, 0)).expect("A78 cart");
        assert_eq!(cart.read(0x8000), 0x42);
    }

    #[test]
    fn v3_supergame_ram_is_selected_and_writable() {
        let image = a78(3, &vec![0; 131_072], 0x0006, 0, 0, 0);
        let mut cart = Cartridge::from_rom(&image).expect("SuperGame RAM cart");
        cart.write(0x4000, 0xA5);
        assert_eq!(cart.read(0x4000), 0xA5);
    }

    #[test]
    fn v4_fields_override_v3_fields() {
        let cart = Cartridge::from_rom(&a78(4, &vec![0; 131_072], 0, 1, 5, 1)).expect("v4 cart");
        assert_eq!(
            cart.banking,
            BankingScheme::SuperGame {
                option: SuperGameOption::ExFix
            }
        );
        assert_eq!(cart.pokey_location(), Some(PokeyLocation::Addr0440));
    }

    #[test]
    fn declared_size_must_match_payload() {
        let mut image = a78(3, &vec![0; 32_768], 0, 0, 0, 0);
        image[51] -= 1;
        assert!(
            Cartridge::from_rom(&image)
                .expect_err("size mismatch should fail")
                .contains("declares")
        );
    }

    #[test]
    fn unsupported_mapper_is_not_guessed_from_size() {
        let image = a78(4, &vec![0; 32_768], 0, 2, 0, 0);
        assert_eq!(
            Cartridge::from_rom(&image).expect_err("unsupported mapper should fail"),
            "Unsupported A78 mapper: 2"
        );
    }

    #[test]
    fn supergame_bank_switching() {
        let mut rom = vec![0; 131_072];
        rom[0] = 0xAA;
        rom[0xC000] = 0xBB;
        rom[7 * 0x4000] = 0xCC;
        let mut cart = Cartridge::from_rom(&rom).expect("SuperGame cart");
        assert_eq!(cart.read(0x8000), 0xAA);
        assert_eq!(cart.read(0xC000), 0xCC);
        cart.write(0x8000, 3);
        assert_eq!(cart.read(0x8000), 0xBB);
    }
}
