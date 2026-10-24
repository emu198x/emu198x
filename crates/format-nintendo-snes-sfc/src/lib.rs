//! The Super NES / Super Famicom cartridge image layout.
//!
//! A `.sfc` or `.smc` file is the cartridge's ROM in the order the board
//! wires it, sometimes behind a 512-byte header added by a copier. Inside
//! the ROM, a 64-byte internal header sits just below the 65C816's vectors,
//! where the board maps it into bank `$00` at `$FFC0`. Which file offset that
//! is depends on how the board maps the ROM:
//!
//! | Map | Header at file offset |
//! |---|---|
//! | LoROM | `$7FC0` |
//! | HiROM | `$FFC0` |
//! | ExHiROM | `$40FFC0` |
//!
//! Nothing in the file says which, so [`parse`] scores each candidate: a
//! checksum and complement that add to `$FFFF`, a map-mode byte that names
//! the map the offset implies, a reset vector into ROM, and a title of
//! printable characters. The best-scoring candidate wins.
//!
//! This crate reads the layout and nothing more. How a board decodes the
//! address bus, and what its RAM and coprocessors do, is cartridge hardware
//! and lives with the machine.
//!
//! The field layout is the one documented in
//! `198x/reference/by-system/nintendo-snes/`.

use std::fmt;

/// The copier header some dumps carry in front of the ROM.
pub const COPIER_HEADER_LEN: usize = 512;

/// The smallest image that can hold a LoROM header.
const MIN_LEN: usize = 0x8000;

/// How the board maps the ROM into the 65C816's address space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapMode {
    /// 32 KB banks at `$8000-$FFFF`.
    LoRom,
    /// 64 KB banks.
    HiRom,
    /// HiROM extended past 4 MB.
    ExHiRom,
}

impl MapMode {
    /// The file offset of this map's internal header.
    #[must_use]
    pub const fn header_offset(self) -> usize {
        match self {
            Self::LoRom => 0x7FC0,
            Self::HiRom => 0xFFC0,
            Self::ExHiRom => 0x40_FFC0,
        }
    }
}

/// The destination code's video standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoStandard {
    Ntsc,
    Pal,
}

/// What the cartridge-type byte at `$FFD6` says is on the board besides ROM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chipset {
    /// ROM only.
    Rom,
    /// ROM and RAM.
    RomRam,
    /// ROM and battery-backed RAM.
    RomRamBattery,
    /// A coprocessor. The high nibble names its family (`0` DSP, `1` GSU,
    /// `2` OBC1, `3` SA-1, `4` S-DD1, `5` S-RTC, `E` other, `F` custom); the
    /// low nibble says whether RAM and a battery come with it.
    Coprocessor { family: u8, layout: u8 },
}

impl Chipset {
    fn decode(byte: u8) -> Self {
        match byte {
            0x00 => Self::Rom,
            0x01 => Self::RomRam,
            0x02 => Self::RomRamBattery,
            _ => Self::Coprocessor {
                family: byte >> 4,
                layout: byte & 0x0F,
            },
        }
    }

    /// The coprocessor family's usual name, if the board has one.
    #[must_use]
    pub fn coprocessor_name(self) -> Option<&'static str> {
        match self {
            Self::Coprocessor { family, .. } => Some(match family {
                0x0 => "DSP",
                0x1 => "SuperFX (GSU)",
                0x2 => "OBC1",
                0x3 => "SA-1",
                0x4 => "S-DD1",
                0x5 => "S-RTC",
                0xE => "other coprocessor",
                0xF => "custom chip",
                _ => "unknown coprocessor",
            }),
            _ => None,
        }
    }

    /// Whether the board has cartridge RAM.
    #[must_use]
    pub fn has_ram(self) -> bool {
        match self {
            Self::Rom => false,
            Self::RomRam | Self::RomRamBattery => true,
            Self::Coprocessor { layout, .. } => matches!(layout, 0x4 | 0x5 | 0x6 | 0x9 | 0xA),
        }
    }

    /// Whether the cartridge RAM is battery-backed.
    #[must_use]
    pub fn has_battery(self) -> bool {
        match self {
            Self::RomRamBattery => true,
            Self::Coprocessor { layout, .. } => matches!(layout, 0x5 | 0x6 | 0x9 | 0xA),
            _ => false,
        }
    }
}

/// The 65C816's vectors, as the header region holds them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vectors {
    pub native_cop: u16,
    pub native_brk: u16,
    pub native_abort: u16,
    pub native_nmi: u16,
    pub native_irq: u16,
    pub emulation_cop: u16,
    pub emulation_abort: u16,
    pub emulation_nmi: u16,
    pub reset: u16,
    pub emulation_irq: u16,
}

/// The internal header's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SfcHeader {
    /// Where the header was found, which is also how the board maps the ROM.
    pub map: MapMode,
    /// The 21-byte title, trailing spaces trimmed. Bytes outside printable
    /// ASCII become `?`.
    pub title: String,
    /// The raw map-mode byte at `$FFD5`.
    pub map_mode_byte: u8,
    /// Bit 4 of the map-mode byte: the board's ROM is fast enough for
    /// FastROM (MEMSEL).
    pub fast_rom: bool,
    /// The raw cartridge-type byte at `$FFD6`.
    pub chipset_byte: u8,
    pub chipset: Chipset,
    /// The ROM size the header declares, in bytes (`1 KB << byte`).
    pub declared_rom_size: usize,
    /// The cartridge RAM size, in bytes (`1 KB << byte`, or 0).
    pub ram_size: usize,
    /// The destination code at `$FFD9`.
    pub destination: u8,
    /// The developer code at `$FFDA`.
    pub developer: u8,
    /// The version at `$FFDB`.
    pub version: u8,
    pub checksum_complement: u16,
    pub checksum: u16,
    pub vectors: Vectors,
}

impl SfcHeader {
    /// The video standard the destination code implies.
    #[must_use]
    pub fn video_standard(&self) -> VideoStandard {
        match self.destination {
            0x02..=0x0C | 0x11 => VideoStandard::Pal,
            _ => VideoStandard::Ntsc,
        }
    }

    /// Whether the checksum and its complement are consistent.
    #[must_use]
    pub fn checksum_pair_valid(&self) -> bool {
        self.checksum ^ self.checksum_complement == 0xFFFF
    }
}

/// A parsed image: the header and the ROM it describes, borrowed from the
/// file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SfcImage<'a> {
    pub header: SfcHeader,
    /// Whether a 512-byte copier header was found and skipped.
    pub had_copier_header: bool,
    /// The ROM, copier header removed.
    pub rom: &'a [u8],
}

/// Why an image could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SfcError {
    /// Too short to hold even a LoROM header.
    TooShort(usize),
    /// No candidate header location looked like a header.
    NoHeader,
}

impl fmt::Display for SfcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort(len) => write!(
                f,
                "SNES image is {len} bytes, too short to hold a cartridge header"
            ),
            Self::NoHeader => {
                f.write_str("no SNES cartridge header found at $7FC0, $FFC0 or $40FFC0")
            }
        }
    }
}

impl std::error::Error for SfcError {}

/// Parse a cartridge image, skipping a copier header if there is one and
/// finding the internal header by score.
///
/// # Errors
///
/// [`SfcError`] when the image is too short, or no candidate location holds
/// anything resembling a header.
pub fn parse(file: &[u8]) -> Result<SfcImage<'_>, SfcError> {
    let had_copier_header = file.len() % 1024 == COPIER_HEADER_LEN;
    let rom = if had_copier_header {
        &file[COPIER_HEADER_LEN..]
    } else {
        file
    };
    if rom.len() < MIN_LEN {
        return Err(SfcError::TooShort(rom.len()));
    }
    let (_, map) = [MapMode::LoRom, MapMode::HiRom, MapMode::ExHiRom]
        .into_iter()
        .filter_map(|map| score(rom, map).map(|s| (s, map)))
        // Ties go to the earlier offset, so a small LoROM image that also has
        // bytes at $FFC0 stays LoROM.
        .max_by_key(|&(s, map)| (s, std::cmp::Reverse(map.header_offset())))
        .filter(|&(s, _)| s > 0)
        .ok_or(SfcError::NoHeader)?;
    Ok(SfcImage {
        header: read_header(rom, map),
        had_copier_header,
        rom,
    })
}

/// Read the header at `map`'s offset. The caller has checked the ROM is
/// long enough.
fn read_header(rom: &[u8], map: MapMode) -> SfcHeader {
    let h = &rom[map.header_offset()..map.header_offset() + 0x40];
    let word = |i: usize| u16::from_le_bytes([h[i], h[i + 1]]);
    let title = h[..21]
        .iter()
        .map(|&b| {
            if (0x20..0x7F).contains(&b) {
                char::from(b)
            } else {
                '?'
            }
        })
        .collect::<String>()
        .trim_end()
        .to_owned();
    let size = |byte: u8| {
        if byte == 0 || byte > 13 {
            0
        } else {
            1024usize << byte
        }
    };
    SfcHeader {
        map,
        title,
        map_mode_byte: h[0x15],
        fast_rom: h[0x15] & 0x10 != 0,
        chipset_byte: h[0x16],
        chipset: Chipset::decode(h[0x16]),
        declared_rom_size: size(h[0x17]),
        ram_size: size(h[0x18]),
        destination: h[0x19],
        developer: h[0x1A],
        version: h[0x1B],
        checksum_complement: word(0x1C),
        checksum: word(0x1E),
        vectors: Vectors {
            native_cop: word(0x24),
            native_brk: word(0x26),
            native_abort: word(0x28),
            native_nmi: word(0x2A),
            native_irq: word(0x2E),
            emulation_cop: word(0x34),
            emulation_abort: word(0x38),
            emulation_nmi: word(0x3A),
            reset: word(0x3C),
            emulation_irq: word(0x3E),
        },
    }
}

/// How much the bytes at `map`'s header offset look like a header, or
/// `None` if the ROM is too short to have one there.
fn score(rom: &[u8], map: MapMode) -> Option<i32> {
    let base = map.header_offset();
    if rom.len() < base + 0x40 {
        return None;
    }
    let header = read_header(rom, map);
    let mut s = 0;
    if header.checksum_pair_valid() {
        s += 8;
        if header.checksum != 0 && header.checksum != 0xFFFF {
            s += 2;
        }
    }
    let expected = match map {
        MapMode::LoRom => 0x20,
        MapMode::HiRom => 0x21,
        MapMode::ExHiRom => 0x25,
    };
    let mode = header.map_mode_byte & 0xEF;
    if mode == expected {
        s += 6;
    } else if (0x20..=0x3A).contains(&mode) {
        s += 1;
    }
    if header.vectors.reset >= 0x8000 {
        s += 4;
    } else {
        s -= 4;
    }
    if header.declared_rom_size != 0 && header.declared_rom_size >= rom.len() / 2 {
        s += 1;
    }
    let printable = rom[base..base + 21]
        .iter()
        .filter(|&&b| (0x20..0x7F).contains(&b))
        .count();
    if printable == 21 {
        s += 2;
    }
    if map == MapMode::ExHiRom && rom.len() <= 0x40_0000 {
        s -= 8;
    }
    Some(s)
}

/// The checksum the header's `checksum` field should hold: the sum of every
/// ROM byte, with a ROM whose size is not a power of two counted the way the
/// cartridge mirrors it (the remainder repeated up to the next power of two).
#[must_use]
pub fn compute_checksum(rom: &[u8]) -> u16 {
    if rom.is_empty() {
        return 0;
    }
    let base = 1usize << (usize::BITS - 1 - rom.len().leading_zeros());
    let sum = |bytes: &[u8]| {
        bytes
            .iter()
            .fold(0u32, |acc, &b| acc.wrapping_add(u32::from(b)))
    };
    let mut total = sum(&rom[..base]);
    if base < rom.len() {
        let rest = &rom[base..];
        let repeats = (base / rest.len()) as u32;
        total = total.wrapping_add(sum(rest).wrapping_mul(repeats));
    }
    total as u16
}
