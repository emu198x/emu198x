//! Atari 800XL cartridge handling.
//!
//! Flat 8 KB and 16 KB cartridges, and the three bank-switched families the
//! bulk of the later library uses. A banked cartridge decodes the cartridge
//! control select line itself: the machine exposes `$D500-$D5FF` as an access
//! strobe and the cartridge decides what an address or a written byte means.
//!
//! The scheme is decided in this order, first answer wins:
//!
//! 1. An explicit [`CartridgeKind`] from the caller (`--cart-type`, the
//!    runtime's `insert_cartridge_as`, a script's `cart_type`). It
//!    overrides the header as well as the guess, for a header that lies.
//! 2. The 16-byte `CART` header (magic, big-endian type id, big-endian
//!    checksum, four unused bytes) that atari800 introduced and TOSEC ships
//!    on some dumps; the type id names the scheme.
//! 3. The CRC32 of a headerless image, looked up in `oss_carts.rs`, the OSS
//!    titles from MAME's CC0 software list. OSS boards share their sizes
//!    with flat cartridges, so nothing in the bytes tells them apart.
//! 4. Size alone: up to 8 KB flat at `$A000`, 16 KB flat at `$8000`, and
//!    32 KB to 1 MB as an XEGS cartridge, Atari's own scheme for its large
//!    releases and the common headerless dump.
//!
//! Step 4 picks the plain layout for an unidentified 8 or 16 KB image, per
//! `knowledge/decisions/cart-layout-needs-positive-evidence.md`. An
//! unidentified OSS or MegaCart dump therefore needs step 1.
//!
//! XEGS and MegaCart details follow atari800's `DOC/cart.txt` and
//! `cartridge.c`. The OSS boards follow Altirra (`src/Altirra/source/
//! cartridge.cpp` and `src/ATIO/source/cartridgeimage.cpp`): the Altirra
//! Hardware Reference Manual's chapter 8 does not cover OSS, so its source
//! is the reference. Altirra and atari800 agree on which address selects
//! which bank; Altirra also models the double-chip selects as the AND of
//! both banks, where atari800 leaves a TODO and drives `$FF`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::oss_carts::OSS_CRC32;

/// The 16-byte `CART` header that may precede the ROM image.
const HEADER_LEN: usize = 16;
const HEADER_MAGIC: &[u8; 4] = b"CART";

/// Which banking hardware the cartridge carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CartridgeKind {
    /// Flat ROM, no banking. Up to 8 KB sits at `$A000`; 16 KB at `$8000`.
    Standard,
    /// OSS one-chip 16 KB (`M091`): 4 KB banked at `$A000`, bank 0 fixed at
    /// `$B000`. Bank chosen by address bits 0 and 3 of a `$D5xx` access.
    OssOneChip,
    /// OSS two-chip 16 KB (`043M`): 4 KB banked at `$A000`, bank 3 fixed at
    /// `$B000`. Bank chosen by address bits 0-3 of a `$D5xx` access.
    OssTwoChip,
    /// The obsolete `034M` image order for the two-chip cartridge, kept for
    /// `CART` type 3 files: same hardware, banks 1 and 2 swapped.
    OssTwoChipLegacy,
    /// XEGS: 8 KB banks; the byte written to `$D5xx` picks the bank at
    /// `$8000`, and the last bank is fixed at `$A000`.
    Xegs,
    /// MegaCart: 16 KB banks at `$8000-$BFFF` picked by the low bits of the
    /// byte written to `$D5xx`; bit 7 disables the cartridge.
    Mega,
    /// OSS 8 KB: the one-chip board with an 8 KB chip (The Writer's Tool).
    /// 4 KB banked at `$A000`, bank 0 fixed at `$B000`; `$D5xx` address
    /// bits 3 and 0 pick bank 1 (`0x`), off (`10`) or bank 0 (`11`).
    OssEightK,
}

impl CartridgeKind {
    /// Every kind, in the order the CLI lists them.
    pub const ALL: [Self; 7] = [
        Self::Standard,
        Self::OssOneChip,
        Self::OssTwoChip,
        Self::OssTwoChipLegacy,
        Self::OssEightK,
        Self::Xegs,
        Self::Mega,
    ];

    /// The names the CLI, scripts and MCP accept, in [`Self::ALL`] order.
    pub const NAMES: [&'static str; 7] = [
        "standard", "oss-m091", "oss-043m", "oss-034m", "oss-8k", "xegs", "mega",
    ];

    /// The stable name for this kind, as `--cart-type` takes it.
    #[must_use]
    pub fn name(self) -> &'static str {
        let index = Self::ALL
            .iter()
            .position(|kind| *kind == self)
            .unwrap_or_default();
        Self::NAMES[index]
    }
}

impl fmt::Display for CartridgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for CartridgeKind {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::NAMES
            .iter()
            .position(|known| *known == name)
            .map(|index| Self::ALL[index])
            .ok_or_else(|| {
                format!(
                    "unknown cartridge type `{name}`; expected {}",
                    Self::NAMES.join(" | ")
                )
            })
    }
}

/// What the OSS `$A000-$AFFF` window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum OssWindow {
    /// One 4 KB bank of the image.
    Bank(u8),
    /// Neither chip selected while the board stays enabled: `$FF`.
    Blank,
    /// ROM disabled: RAM shows through at `$A000-$BFFF`.
    Off,
    /// Two chips selected at once (two-chip board, `$D5x1` / `$D5x5`). Both
    /// drive the bus and the open-collector result is the AND of the two
    /// banks, as Altirra builds it.
    And(u8, u8),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cartridge {
    rom: Vec<u8>,
    kind: CartridgeKind,
    /// Flat only: where the image sits.
    base: u16,
    oss: OssWindow,
    /// XEGS: bank at `$8000`. MegaCart: the last byte written, bit 7 = off.
    bank: u8,
}

impl Cartridge {
    /// Build a cartridge from an image, honouring a `CART` header when one
    /// is present and identifying known headerless OSS dumps by CRC32.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        Self::from_rom_as(data, None)
    }

    /// Build a cartridge from an image, as `kind` when the caller names one.
    ///
    /// An explicit kind overrides both the `CART` header (which is stripped
    /// either way) and the CRC32 and size identification. Without one, the
    /// order in the module documentation applies.
    pub fn from_rom_as(data: &[u8], kind: Option<CartridgeKind>) -> Result<Self, String> {
        let header = (data.len() >= HEADER_LEN && &data[..4] == HEADER_MAGIC)
            .then(|| u32::from_be_bytes([data[4], data[5], data[6], data[7]]));
        let body = if header.is_some() {
            &data[HEADER_LEN..]
        } else {
            data
        };
        if let Some(kind) = kind {
            return Self::with_kind(kind, body);
        }
        if let Some(type_id) = header {
            let kind = kind_for_car_type(type_id)
                .ok_or_else(|| format!("Unsupported CART header type {type_id}"))?;
            return Self::with_kind(kind, body);
        }
        if let Some(kind) = identify(body) {
            return Self::with_kind(kind, body);
        }
        let kind = match body.len() {
            1..=16384 => CartridgeKind::Standard,
            n if (32768..=1_048_576).contains(&n) && n.is_power_of_two() => CartridgeKind::Xegs,
            other => {
                return Err(format!(
                    "Unsupported cartridge size: {other} bytes; name the scheme with a \
                     cartridge type ({})",
                    CartridgeKind::NAMES.join(" | ")
                ));
            }
        };
        Self::with_kind(kind, body)
    }

    /// Build a cartridge of a known scheme from a bare image.
    pub fn with_kind(kind: CartridgeKind, rom: &[u8]) -> Result<Self, String> {
        let size_ok = match kind {
            CartridgeKind::Standard => (1..=16384).contains(&rom.len()),
            CartridgeKind::OssOneChip
            | CartridgeKind::OssTwoChip
            | CartridgeKind::OssTwoChipLegacy => rom.len() == 16384,
            CartridgeKind::Xegs => {
                (32768..=1_048_576).contains(&rom.len()) && rom.len().is_power_of_two()
            }
            CartridgeKind::Mega => {
                (16384..=1_048_576).contains(&rom.len()) && rom.len().is_power_of_two()
            }
            CartridgeKind::OssEightK => rom.len() == 8192,
        };
        if !size_ok {
            return Err(format!(
                "Unsupported cartridge size for {kind:?}: {} bytes",
                rom.len()
            ));
        }
        let base = if rom.len() > 8192 { 0x8000 } else { 0xA000 };
        Ok(Self {
            rom: rom.to_vec(),
            kind,
            base,
            oss: cold_oss_window(kind),
            bank: 0,
        })
    }

    /// Reset mapper latches while preserving the parsed cartridge type.
    #[must_use]
    pub fn cold_boot(&self) -> Self {
        Self {
            oss: cold_oss_window(self.kind),
            bank: 0,
            ..self.clone()
        }
    }

    #[must_use]
    pub fn kind(&self) -> CartridgeKind {
        self.kind
    }

    /// Lowest address the cartridge answers at power-on, where an OS-less
    /// boot starts executing.
    #[must_use]
    pub fn base(&self) -> u16 {
        match self.kind {
            CartridgeKind::Standard => self.base,
            CartridgeKind::OssOneChip
            | CartridgeKind::OssTwoChip
            | CartridgeKind::OssTwoChipLegacy
            | CartridgeKind::OssEightK => 0xA000,
            CartridgeKind::Xegs | CartridgeKind::Mega => 0x8000,
        }
    }

    /// The image offset `addr` maps to under the current bank selection, or
    /// `None` where the cartridge leaves the bus alone.
    fn offset(&self, addr: u16) -> Option<Offset> {
        let a = usize::from(addr);
        match self.kind {
            CartridgeKind::Standard => {
                let offset = a.checked_sub(usize::from(self.base))?;
                (offset < self.rom.len()).then_some(Offset::Rom(offset))
            }
            CartridgeKind::OssOneChip
            | CartridgeKind::OssTwoChip
            | CartridgeKind::OssTwoChipLegacy
            | CartridgeKind::OssEightK => {
                let fixed = match self.kind {
                    CartridgeKind::OssTwoChip | CartridgeKind::OssTwoChipLegacy => 3,
                    _ => 0,
                };
                match addr {
                    0xB000..=0xBFFF if self.oss != OssWindow::Off => {
                        Some(Offset::Rom(fixed * 0x1000 + (a - 0xB000)))
                    }
                    0xA000..=0xAFFF => match self.oss {
                        OssWindow::Bank(bank) => {
                            Some(Offset::Rom(usize::from(bank) * 0x1000 + (a - 0xA000)))
                        }
                        OssWindow::And(first, second) => Some(Offset::And(
                            usize::from(first) * 0x1000 + (a - 0xA000),
                            usize::from(second) * 0x1000 + (a - 0xA000),
                        )),
                        OssWindow::Blank => Some(Offset::Open),
                        OssWindow::Off => None,
                    },
                    _ => None,
                }
            }
            CartridgeKind::Xegs => {
                let banks = self.rom.len() / 0x2000;
                match addr {
                    0x8000..=0x9FFF => {
                        let bank = usize::from(self.bank) & (banks - 1);
                        Some(Offset::Rom(bank * 0x2000 + (a - 0x8000)))
                    }
                    0xA000..=0xBFFF => Some(Offset::Rom((banks - 1) * 0x2000 + (a - 0xA000))),
                    _ => None,
                }
            }
            CartridgeKind::Mega => {
                if self.bank & 0x80 != 0 || !(0x8000..=0xBFFF).contains(&addr) {
                    return None;
                }
                let banks = self.rom.len() / 0x4000;
                let bank = usize::from(self.bank & 0x7F) & (banks - 1);
                Some(Offset::Rom(bank * 0x4000 + (a - 0x8000)))
            }
        }
    }

    #[must_use]
    pub fn read(&self, addr: u16) -> u8 {
        match self.offset(addr) {
            Some(Offset::Rom(offset)) => self.byte(offset),
            Some(Offset::And(first, second)) => self.byte(first) & self.byte(second),
            Some(Offset::Open) | None => 0xFF,
        }
    }

    fn byte(&self, offset: usize) -> u8 {
        self.rom.get(offset).copied().unwrap_or(0xFF)
    }

    #[must_use]
    pub fn covers(&self, addr: u16) -> bool {
        self.offset(addr).is_some()
    }

    /// A CPU read or write anywhere in `$D500-$D5FF`. The OSS parts decode
    /// the address, whichever way the access goes; the value-driven schemes
    /// ignore reads. The tables are Altirra's `kBankLookup` for each board.
    pub fn cctl_access(&mut self, addr: u16) {
        let a = addr & 0x0F;
        self.oss = match self.kind {
            CartridgeKind::OssOneChip => match a & 0x09 {
                0x00 => OssWindow::Bank(1),
                0x01 => OssWindow::Bank(3),
                0x08 => OssWindow::Off,
                _ => OssWindow::Bank(2),
            },
            CartridgeKind::OssEightK => match a & 0x09 {
                0x00 | 0x01 => OssWindow::Bank(1),
                0x08 => OssWindow::Off,
                _ => OssWindow::Bank(0),
            },
            CartridgeKind::OssTwoChip | CartridgeKind::OssTwoChipLegacy => {
                if a & 0x08 != 0 {
                    OssWindow::Off
                } else {
                    // The two image orders differ only in which image bank
                    // the $D5x3/$D5x7 and $D5x4 selects name.
                    let (upper, lower) = if self.kind == CartridgeKind::OssTwoChip {
                        (2, 1)
                    } else {
                        (1, 2)
                    };
                    match a & 0x07 {
                        0x00 => OssWindow::Bank(0),
                        0x01 => OssWindow::And(0, upper),
                        0x03 | 0x07 => OssWindow::Bank(upper),
                        0x04 => OssWindow::Bank(lower),
                        0x05 => OssWindow::And(lower, upper),
                        _ => OssWindow::Blank,
                    }
                }
            }
            _ => return,
        };
    }

    /// A CPU write anywhere in `$D500-$D5FF`.
    pub fn cctl_write(&mut self, addr: u16, value: u8) {
        match self.kind {
            CartridgeKind::Xegs => self.bank = value,
            CartridgeKind::Mega => self.bank = value,
            _ => self.cctl_access(addr),
        }
    }
}

enum Offset {
    Rom(usize),
    /// Two chips driving the bus at once: the AND of both image offsets.
    And(usize, usize),
    /// The cartridge is selected but drives nothing useful: `$FF`.
    Open,
}

/// Where an OSS board's banked window sits after a cold start: Altirra's
/// `InitBank` for the mode. Only the OSS boards use the window at all.
fn cold_oss_window(kind: CartridgeKind) -> OssWindow {
    match kind {
        CartridgeKind::OssTwoChip | CartridgeKind::OssTwoChipLegacy => OssWindow::Bank(2),
        CartridgeKind::OssOneChip => OssWindow::Bank(3),
        CartridgeKind::OssEightK => OssWindow::Bank(1),
        _ => OssWindow::Bank(0),
    }
}

/// The board a known headerless dump was built on, by CRC32.
fn identify(image: &[u8]) -> Option<CartridgeKind> {
    let crc = crc32(image);
    OSS_CRC32
        .binary_search_by_key(&crc, |(known, _)| *known)
        .ok()
        .map(|index| OSS_CRC32[index].1)
        .filter(|kind| match kind {
            CartridgeKind::OssEightK => image.len() == 8192,
            _ => image.len() == 16384,
        })
}

/// CRC-32 (IEEE 802.3, reflected), the checksum MAME's software lists use.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// The `CART` header type ids this machine understands.
fn kind_for_car_type(type_id: u32) -> Option<CartridgeKind> {
    Some(match type_id {
        1 | 2 => CartridgeKind::Standard,
        3 => CartridgeKind::OssTwoChipLegacy,
        15 => CartridgeKind::OssOneChip,
        44 => CartridgeKind::OssEightK,
        45 => CartridgeKind::OssTwoChip,
        12 | 13 | 14 | 23 | 24 | 25 => CartridgeKind::Xegs,
        26..=32 => CartridgeKind::Mega,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An image whose every bank carries its own number at every byte.
    fn banked_image(banks: usize, bank_len: usize) -> Vec<u8> {
        (0..banks)
            .flat_map(|b| std::iter::repeat_n(b as u8, bank_len))
            .collect()
    }

    fn with_header(type_id: u32, rom: &[u8]) -> Vec<u8> {
        let mut image = b"CART".to_vec();
        image.extend_from_slice(&type_id.to_be_bytes());
        image.extend_from_slice(&[0; 8]);
        image.extend_from_slice(rom);
        image
    }

    #[test]
    fn detect_8k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 8192]).expect("8K");
        assert_eq!(cart.base(), 0xA000);
    }

    #[test]
    fn detect_16k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 16384]).expect("16K");
        assert_eq!(cart.base(), 0x8000);
    }

    #[test]
    fn headerless_32k_and_up_is_xegs() {
        for banks in [4, 8, 16, 32, 64, 128] {
            let cart = Cartridge::from_rom(&banked_image(banks, 0x2000)).expect("XEGS");
            assert_eq!(cart.kind(), CartridgeKind::Xegs, "{banks} banks");
        }
    }

    #[test]
    fn reject_odd_sizes() {
        assert!(Cartridge::from_rom(&vec![0u8; 32769]).is_err());
        assert!(Cartridge::from_rom(&vec![0u8; 24576]).is_err());
        assert!(Cartridge::from_rom(&vec![0u8; 2 * 1_048_576]).is_err());
    }

    #[test]
    fn read_within_range() {
        let mut rom = vec![0u8; 8192];
        rom[0] = 0x42;
        rom[0x1FFF] = 0x99;
        let cart = Cartridge::from_rom(&rom).expect("8K");
        assert_eq!(cart.read(0xA000), 0x42);
        assert_eq!(cart.read(0xBFFF), 0x99);
    }

    #[test]
    fn covers_reports_correctly() {
        let cart = Cartridge::from_rom(&vec![0u8; 8192]).expect("8K");
        assert!(cart.covers(0xA000));
        assert!(cart.covers(0xBFFF));
        assert!(!cart.covers(0x9FFF));
        assert!(!cart.covers(0xC000));
    }

    #[test]
    fn cart_header_names_the_scheme_and_is_stripped() {
        let mut rom = vec![0u8; 8192];
        rom[0] = 0x42;
        let cart = Cartridge::from_rom(&with_header(1, &rom)).expect("8K");
        assert_eq!(cart.kind(), CartridgeKind::Standard);
        assert_eq!(cart.read(0xA000), 0x42);

        let cart = Cartridge::from_rom(&with_header(28, &banked_image(4, 0x4000))).expect("Mega");
        assert_eq!(cart.kind(), CartridgeKind::Mega);
        let cart = Cartridge::from_rom(&with_header(15, &banked_image(4, 0x1000))).expect("OSS");
        assert_eq!(cart.kind(), CartridgeKind::OssOneChip);

        assert!(Cartridge::from_rom(&with_header(4, &vec![0; 32768])).is_err());
    }

    #[test]
    fn flat_carts_ignore_the_control_line() {
        let mut cart = Cartridge::from_rom(&vec![0x42; 16384]).expect("16K");
        cart.cctl_write(0xD500, 0x81);
        cart.cctl_access(0xD508);
        assert!(cart.covers(0x8000));
        assert_eq!(cart.read(0xBFFF), 0x42);
    }

    #[test]
    fn xegs_banks_the_lower_window_and_fixes_the_last_bank() {
        let mut cart =
            Cartridge::with_kind(CartridgeKind::Xegs, &banked_image(8, 0x2000)).expect("XEGS 64K");
        assert_eq!(cart.read(0x8000), 0);
        assert_eq!(cart.read(0xA000), 7);
        for bank in 0..8u8 {
            cart.cctl_write(0xD5FF, bank);
            assert_eq!(cart.read(0x9FFF), bank);
            assert_eq!(cart.read(0xBFFF), 7);
        }
        // Only as many bits as there are banks take part.
        cart.cctl_write(0xD500, 0x0A);
        assert_eq!(cart.read(0x8000), 2);
        // Reads of the control line change nothing.
        cart.cctl_access(0xD500);
        assert_eq!(cart.read(0x8000), 2);
        assert!(!cart.covers(0x7FFF));
        assert!(!cart.covers(0xC000));
    }

    #[test]
    fn megacart_banks_the_whole_window_and_bit_7_switches_it_off() {
        let mut cart =
            Cartridge::with_kind(CartridgeKind::Mega, &banked_image(4, 0x4000)).expect("Mega 64K");
        assert_eq!(cart.read(0x8000), 0);
        assert_eq!(cart.read(0xBFFF), 0);
        cart.cctl_write(0xD500, 3);
        assert_eq!(cart.read(0x8000), 3);
        assert_eq!(cart.read(0xBFFF), 3);
        cart.cctl_write(0xD500, 0x83);
        assert!(!cart.covers(0x8000));
        assert!(!cart.covers(0xBFFF));
        cart.cctl_write(0xD500, 0x01);
        assert!(cart.covers(0x8000));
        assert_eq!(cart.read(0xA000), 1);
    }

    #[test]
    fn oss_one_chip_selects_by_address_bits_0_and_3() {
        let mut cart = Cartridge::with_kind(CartridgeKind::OssOneChip, &banked_image(4, 0x1000))
            .expect("OSS M091");
        // Altirra's cold-reset bank for the board.
        assert_eq!(cart.read(0xA000), 3);
        assert_eq!(cart.read(0xB000), 0);
        for (addr, bank) in [(0xD500, 1), (0xD501, 3), (0xD509, 2), (0xD5F0, 1)] {
            cart.cctl_access(addr);
            assert_eq!(cart.read(0xAFFF), bank, "{addr:#06x}");
            assert_eq!(cart.read(0xB000), 0, "{addr:#06x}");
        }
        cart.cctl_access(0xD508);
        assert!(!cart.covers(0xA000));
        assert!(!cart.covers(0xBFFF));
        // A write is an access like any other.
        cart.cctl_write(0xD509, 0xFF);
        assert_eq!(cart.read(0xA000), 2);
    }

    #[test]
    fn oss_two_chip_selects_by_the_low_address_nibble() {
        let mut cart = Cartridge::with_kind(CartridgeKind::OssTwoChip, &banked_image(4, 0x1000))
            .expect("OSS 043M");
        assert_eq!(cart.read(0xB000), 3);
        for (addr, bank) in [(0xD500, 0), (0xD503, 2), (0xD507, 2), (0xD504, 1)] {
            cart.cctl_access(addr);
            assert_eq!(cart.read(0xA000), bank, "{addr:#06x}");
            assert_eq!(cart.read(0xBFFF), 3, "{addr:#06x}");
        }
        for addr in [0xD502, 0xD506] {
            cart.cctl_access(addr);
            assert!(cart.covers(0xA000));
            assert_eq!(cart.read(0xA000), 0xFF, "{addr:#06x}");
        }
        cart.cctl_access(0xD508);
        assert!(!cart.covers(0xB000));

        let mut legacy =
            Cartridge::with_kind(CartridgeKind::OssTwoChipLegacy, &banked_image(4, 0x1000))
                .expect("OSS 034M");
        legacy.cctl_access(0xD503);
        assert_eq!(legacy.read(0xA000), 1);
        legacy.cctl_access(0xD504);
        assert_eq!(legacy.read(0xA000), 2);
    }

    #[test]
    fn oss_8k_selects_by_address_bits_0_and_3() {
        let mut cart = Cartridge::with_kind(CartridgeKind::OssEightK, &banked_image(2, 0x1000))
            .expect("OSS 8K");
        // Altirra's cold-reset bank for the board.
        assert_eq!(cart.read(0xA000), 1);
        assert_eq!(cart.read(0xB000), 0);
        for (addr, bank) in [(0xD509, 0), (0xD500, 1), (0xD5F9, 0), (0xD501, 1)] {
            cart.cctl_access(addr);
            assert_eq!(cart.read(0xAFFF), bank, "{addr:#06x}");
            assert_eq!(cart.read(0xBFFF), 0, "{addr:#06x}");
        }
        cart.cctl_write(0xD508, 0);
        assert!(!cart.covers(0xA000));
        assert!(!cart.covers(0xB000));
        assert_eq!(cart.base(), 0xA000);
        assert!(Cartridge::with_kind(CartridgeKind::OssEightK, &[0; 16384]).is_err());
    }

    #[test]
    fn cart_type_44_is_the_oss_8k_board() {
        let cart = Cartridge::from_rom(&with_header(44, &banked_image(2, 0x1000))).expect("44");
        assert_eq!(cart.kind(), CartridgeKind::OssEightK);
    }

    #[test]
    fn kind_names_round_trip_and_unknown_names_list_the_choices() {
        for kind in CartridgeKind::ALL {
            assert_eq!(kind.name().parse::<CartridgeKind>(), Ok(kind));
        }
        assert_eq!("oss-043m".parse(), Ok(CartridgeKind::OssTwoChip));
        let err = "megarom".parse::<CartridgeKind>().expect_err("unknown");
        assert!(err.contains("`megarom`"), "{err}");
        assert!(
            err.contains("standard | oss-m091 | oss-043m | oss-034m | oss-8k | xegs | mega"),
            "{err}"
        );
    }

    #[test]
    fn an_explicit_kind_overrides_the_size_guess_and_the_header() {
        let image = banked_image(4, 0x1000);
        assert_eq!(
            Cartridge::from_rom(&image).expect("flat").kind(),
            CartridgeKind::Standard
        );
        let cart =
            Cartridge::from_rom_as(&image, Some(CartridgeKind::OssOneChip)).expect("override");
        assert_eq!(cart.kind(), CartridgeKind::OssOneChip);
        assert_eq!(cart.read(0xB000), 0);

        // The header is stripped and its type ignored.
        let headed = with_header(1, &image);
        let cart = Cartridge::from_rom_as(&headed, Some(CartridgeKind::OssTwoChip))
            .expect("override beats header");
        assert_eq!(cart.kind(), CartridgeKind::OssTwoChip);
        assert_eq!(cart.read(0xB000), 3);

        // A kind the image cannot be is refused, not coerced.
        let err = Cartridge::from_rom_as(&image, Some(CartridgeKind::Xegs)).expect_err("size");
        assert!(err.contains("Xegs"), "{err}");
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    /// The table is what `binary_search_by_key` needs: sorted, no duplicates,
    /// and only OSS boards in it.
    #[test]
    fn oss_table_is_sorted_and_holds_only_oss_boards() {
        assert!(OSS_CRC32.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(OSS_CRC32.iter().all(|(_, kind)| matches!(
            kind,
            CartridgeKind::OssOneChip
                | CartridgeKind::OssTwoChip
                | CartridgeKind::OssTwoChipLegacy
                | CartridgeKind::OssEightK
        )));
    }

    /// An image whose CRC32 is not in the table keeps the plain layout for
    /// its size (the decision record's default); identification never
    /// fires on a size the board cannot have.
    #[test]
    fn unidentified_images_keep_the_plain_layout() {
        assert_eq!(identify(&vec![0xEA; 16384]), None);
        assert_eq!(identify(&vec![0xEA; 8192]), None);
        assert_eq!(
            Cartridge::from_rom(&vec![0xEA; 16384]).expect("16K").kind(),
            CartridgeKind::Standard
        );
    }

    /// Four 4 KB banks whose bytes are distinct single bits, so the AND of
    /// any two banks is zero and the AND of a bank with itself is the bank.
    fn bit_banks() -> Vec<u8> {
        [0x11u8, 0x22, 0x44, 0x88]
            .into_iter()
            .flat_map(|byte| std::iter::repeat_n(byte, 0x1000))
            .collect()
    }

    /// The two-chip board selects both chips at once on `$D5x1` and `$D5x5`,
    /// and the bus sees the AND of the two banks (Altirra `cartridgeimage.cpp`
    /// builds those as banks 5 and 6; `cartridge.cpp`'s `kBankLookup` picks
    /// them). `$D5x2` and `$D5x6` select neither chip: `$FF`.
    #[test]
    fn oss_two_chip_double_selects_read_the_and_of_both_banks() {
        let mut image = bit_banks();
        // Overlapping bits so the AND is not trivially zero.
        image[0x0010] = 0xF0; // bank 0
        image[0x1010] = 0x3C; // bank 1
        image[0x2010] = 0x5A; // bank 2

        let mut cart = Cartridge::with_kind(CartridgeKind::OssTwoChip, &image).expect("043M");
        cart.cctl_access(0xD501);
        assert_eq!(cart.read(0xA010), 0xF0 & 0x5A, "043M $D5x1: banks 0 and 2");
        assert_eq!(cart.read(0xA000), 0x11 & 0x44);
        cart.cctl_access(0xD505);
        assert_eq!(cart.read(0xA010), 0x3C & 0x5A, "043M $D5x5: banks 1 and 2");
        for addr in [0xD502, 0xD506] {
            cart.cctl_access(addr);
            assert_eq!(cart.read(0xA010), 0xFF, "{addr:#06x}");
        }
        // The fixed window is untouched by every select.
        assert_eq!(cart.read(0xB000), 0x88);

        let mut legacy =
            Cartridge::with_kind(CartridgeKind::OssTwoChipLegacy, &image).expect("034M");
        legacy.cctl_access(0xD501);
        assert_eq!(
            legacy.read(0xA010),
            0xF0 & 0x3C,
            "034M $D5x1: banks 0 and 1"
        );
        legacy.cctl_access(0xD5F5);
        assert_eq!(
            legacy.read(0xA010),
            0x3C & 0x5A,
            "034M $D5x5: banks 1 and 2"
        );
    }

    /// Altirra's cold-reset banks (`cartridge.cpp`, the `InitBank` column of
    /// the mode table): the two-chip boards start on bank 2 and the one-chip
    /// board on bank 3, each with its fixed window enabled. A real board has
    /// no power-on reset and comes up anywhere; the software copes because
    /// the OS only reads the fixed window before the cartridge banks itself.
    #[test]
    fn oss_boards_cold_boot_on_altirras_banks() {
        for (kind, bank, fixed) in [
            (CartridgeKind::OssTwoChip, 0x44, 0x88),
            (CartridgeKind::OssTwoChipLegacy, 0x44, 0x88),
            (CartridgeKind::OssOneChip, 0x88, 0x11),
        ] {
            let cart = Cartridge::with_kind(kind, &bit_banks()).expect("OSS");
            for cart in [cart.clone(), cart.cold_boot()] {
                assert_eq!(cart.read(0xA000), bank, "{kind:?} banked window");
                assert_eq!(cart.read(0xB000), fixed, "{kind:?} fixed window");
            }
        }
    }
}
