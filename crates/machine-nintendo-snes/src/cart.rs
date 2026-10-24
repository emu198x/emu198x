//! The cartridge boards: how each decodes the address bus onto its ROM and
//! RAM.
//!
//! | Board | ROM | RAM |
//! |---|---|---|
//! | LoROM | `00-7D,80-FF:8000-FFFF` in 32 KB banks, mirrored into `40-6F,C0-EF:0000-7FFF` | `70-7D,F0-FF:0000-7FFF` |
//! | HiROM | `00-3F,80-BF:8000-FFFF` and `40-7D,C0-FF:0000-FFFF` in 64 KB banks | `20-3F,A0-BF:6000-7FFF`, 8 KB per bank |
//! | ExHiROM | as HiROM, with `00-7D` reaching the second 4 MB and `80-FF` the first | as HiROM |
//!
//! A ROM whose size is not a power of two is mirrored the way the board's
//! decoder does it: the part past the largest power of two repeats to fill
//! the next one. The maps follow ares's board definitions (ISC licence) and
//! `198x/reference/by-system/nintendo-snes/`.
//!
//! Coprocessor boards (DSP, SuperFX, SA-1 and the rest) are refused: none is
//! modelled yet.

use format_nintendo_snes_sfc::{MapMode, SfcError, VideoStandard};
use serde::{Deserialize, Serialize};
use std::fmt;

/// How the board maps the ROM.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Map {
    LoRom,
    HiRom,
    ExHiRom,
}

impl From<MapMode> for Map {
    fn from(mode: MapMode) -> Self {
        match mode {
            MapMode::LoRom => Self::LoRom,
            MapMode::HiRom => Self::HiRom,
            MapMode::ExHiRom => Self::ExHiRom,
        }
    }
}

/// Why a cartridge could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CartridgeError {
    /// The image has no usable header.
    Image(SfcError),
    /// The board carries a coprocessor this machine does not model.
    UnsupportedCoprocessor(&'static str),
}

impl fmt::Display for CartridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Image(error) => error.fmt(f),
            Self::UnsupportedCoprocessor(name) => {
                write!(f, "cartridges with a {name} are not supported yet")
            }
        }
    }
}

impl std::error::Error for CartridgeError {}

/// Where an address lands on the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Rom(usize),
    Ram(usize),
    /// Nothing drives the bus: open bus.
    None,
}

/// A cartridge in the slot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cartridge {
    pub map: Map,
    rom: Vec<u8>,
    ram: Vec<u8>,
    /// Whether the RAM is battery-backed, so worth saving.
    pub battery: bool,
    /// The header's title, for display.
    pub title: String,
    /// The video standard the header's destination code implies.
    pub pal: bool,
}

impl Cartridge {
    /// Build the board an image's header describes.
    ///
    /// # Errors
    ///
    /// [`CartridgeError`] when the image has no header, or its board has a
    /// coprocessor.
    pub fn from_image(file: &[u8]) -> Result<Self, CartridgeError> {
        let image = format_nintendo_snes_sfc::parse(file).map_err(CartridgeError::Image)?;
        let header = &image.header;
        if let Some(name) = header.chipset.coprocessor_name() {
            return Err(CartridgeError::UnsupportedCoprocessor(name));
        }
        let ram = if header.chipset.has_ram() {
            header.ram_size
        } else {
            0
        };
        let mut cart = Self::from_rom(image.rom.to_vec(), header.map.into(), ram);
        cart.battery = header.chipset.has_battery();
        cart.title.clone_from(&header.title);
        cart.pal = header.video_standard() == VideoStandard::Pal;
        Ok(cart)
    }

    /// A board of `map` with this ROM and `ram_size` bytes of RAM, no
    /// header needed. For tests and tools.
    #[must_use]
    pub fn from_rom(rom: Vec<u8>, map: Map, ram_size: usize) -> Self {
        Self {
            map,
            rom,
            ram: vec![0; ram_size],
            battery: false,
            title: String::new(),
            pal: false,
        }
    }

    #[must_use]
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Replace the RAM's contents, as a saved file restores them. Extra
    /// bytes are dropped and a short file leaves the rest as it was.
    pub fn load_ram(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.ram.len());
        self.ram[..n].copy_from_slice(&bytes[..n]);
    }

    /// Where `addr` lands, for an address the console has not claimed.
    #[must_use]
    pub fn decode(&self, addr: u32) -> Target {
        let bank = (addr >> 16) as usize & 0xFF;
        let offset = addr as usize & 0xFFFF;
        let low = bank & 0x7F;
        let ram = !self.ram.is_empty();
        match self.map {
            Map::LoRom => {
                if ram && (0x70..=0x7D).contains(&low) && offset < 0x8000 {
                    return Target::Ram(((low & 0x0F) << 15 | offset) % self.ram.len());
                }
                if offset >= 0x8000 || low >= 0x40 {
                    let linear = low << 15 | (offset & 0x7FFF);
                    return self.rom_target(linear);
                }
                Target::None
            }
            Map::HiRom | Map::ExHiRom => {
                if ram && (0x20..=0x3F).contains(&low) && (0x6000..0x8000).contains(&offset) {
                    return Target::Ram(((low & 0x1F) << 13 | (offset - 0x6000)) % self.ram.len());
                }
                if offset >= 0x8000 || low >= 0x40 {
                    let mut linear = (bank & 0x3F) << 16 | offset;
                    if self.map == Map::ExHiRom && bank & 0x80 == 0 {
                        linear |= 0x40_0000;
                    }
                    return self.rom_target(linear);
                }
                Target::None
            }
        }
    }

    fn rom_target(&self, linear: usize) -> Target {
        if self.rom.is_empty() {
            Target::None
        } else {
            Target::Rom(mirror(linear, self.rom.len()))
        }
    }

    /// Read what the board drives at `addr`, or `None` for open bus.
    #[must_use]
    pub fn read(&self, addr: u32) -> Option<u8> {
        match self.decode(addr) {
            Target::Rom(i) => Some(self.rom[i]),
            Target::Ram(i) => Some(self.ram[i]),
            Target::None => None,
        }
    }

    /// Write `value` at `addr`. ROM ignores it.
    pub fn write(&mut self, addr: u32, value: u8) {
        if let Target::Ram(i) = self.decode(addr) {
            self.ram[i] = value;
        }
    }
}

/// Fold `addr` into `size` the way a cartridge decoder mirrors a ROM whose
/// size is not a power of two (ares's `Bus::mirror`).
#[must_use]
pub fn mirror(mut addr: usize, mut size: usize) -> usize {
    if size == 0 {
        return 0;
    }
    let mut base = 0;
    let mut mask = 1usize << 23;
    while addr >= size {
        while addr & mask == 0 {
            mask >>= 1;
        }
        addr -= mask;
        if size > mask {
            size -= mask;
            base += mask;
        }
        mask >>= 1;
    }
    base + addr
}
