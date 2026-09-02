//! Atari 800XL cartridge handling.
//!
//! Flat 8 KB and 16 KB cartridges, and the three bank-switched families the
//! bulk of the later library uses. A banked cartridge decodes the cartridge
//! control select line itself: the machine exposes `$D500-$D5FF` as an access
//! strobe and the cartridge decides what an address or a written byte means.
//!
//! Images may carry the 16-byte `CART` header (magic, big-endian type id,
//! big-endian checksum, four unused bytes) that atari800 introduced and
//! TOSEC ships on some dumps; the type id names the scheme. A headerless
//! image is classified by size alone: up to 8 KB flat at `$A000`, 16 KB flat
//! at `$8000`, and 32 KB to 1 MB as an XEGS cartridge, Atari's own scheme for
//! its large releases and the common headerless dump. An OSS or MegaCart
//! image needs its header to be told apart from those.
//!
//! Scheme details follow atari800's `DOC/cart.txt` and `cartridge.c`, which
//! in turn cite the jindroush cartridge pages and the OSS chip notes.

use serde::{Deserialize, Serialize};

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
}

/// What the OSS `$A000-$AFFF` window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum OssWindow {
    /// One 4 KB bank of the image.
    Bank(u8),
    /// Two chips selected at once. The bus sees the AND of both; atari800
    /// settles for `$FF`, and so does this.
    Blank,
    /// ROM disabled: RAM shows through at `$A000-$BFFF`.
    Off,
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
    /// is present.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        if data.len() >= HEADER_LEN && &data[..4] == HEADER_MAGIC {
            let type_id = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
            let kind = kind_for_car_type(type_id)
                .ok_or_else(|| format!("Unsupported CART header type {type_id}"))?;
            return Self::with_kind(kind, &data[HEADER_LEN..]);
        }
        let kind = match data.len() {
            1..=16384 => CartridgeKind::Standard,
            n if (32768..=1_048_576).contains(&n) && n.is_power_of_two() => CartridgeKind::Xegs,
            other => return Err(format!("Unsupported cartridge size: {other} bytes")),
        };
        Self::with_kind(kind, data)
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
            // Both OSS parts come up showing bank 0 (atari800's reset
            // state); the fixed window is bank 0 on the one-chip part and
            // bank 3 on the two-chip part.
            oss: OssWindow::Bank(0),
            bank: 0,
        })
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
            | CartridgeKind::OssTwoChipLegacy => 0xA000,
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
            | CartridgeKind::OssTwoChipLegacy => {
                let fixed = if self.kind == CartridgeKind::OssOneChip {
                    0
                } else {
                    3
                };
                match addr {
                    0xB000..=0xBFFF if self.oss != OssWindow::Off => {
                        Some(Offset::Rom(fixed * 0x1000 + (a - 0xB000)))
                    }
                    0xA000..=0xAFFF => match self.oss {
                        OssWindow::Bank(bank) => {
                            Some(Offset::Rom(usize::from(bank) * 0x1000 + (a - 0xA000)))
                        }
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
            Some(Offset::Rom(offset)) => self.rom.get(offset).copied().unwrap_or(0xFF),
            Some(Offset::Open) | None => 0xFF,
        }
    }

    #[must_use]
    pub fn covers(&self, addr: u16) -> bool {
        self.offset(addr).is_some()
    }

    /// A CPU read or write anywhere in `$D500-$D5FF`. The OSS parts decode
    /// the address; the value-driven schemes ignore reads.
    pub fn cctl_access(&mut self, addr: u16) {
        let a = addr & 0x0F;
        self.oss = match self.kind {
            CartridgeKind::OssOneChip => match a & 0x09 {
                0x00 => OssWindow::Bank(1),
                0x01 => OssWindow::Bank(3),
                0x08 => OssWindow::Off,
                _ => OssWindow::Bank(2),
            },
            CartridgeKind::OssTwoChip | CartridgeKind::OssTwoChipLegacy => {
                if a & 0x08 != 0 {
                    OssWindow::Off
                } else {
                    // The two image orders differ only in which chip half
                    // the $D5x3/$D5x7 and $D5x4 selects name.
                    let (upper, lower) = if self.kind == CartridgeKind::OssTwoChip {
                        (2, 1)
                    } else {
                        (1, 2)
                    };
                    match a & 0x07 {
                        0x00 => OssWindow::Bank(0),
                        0x03 | 0x07 => OssWindow::Bank(upper),
                        0x04 => OssWindow::Bank(lower),
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
    /// The cartridge is selected but drives nothing useful: `$FF`.
    Open,
}

/// The `CART` header type ids this machine understands.
fn kind_for_car_type(type_id: u32) -> Option<CartridgeKind> {
    Some(match type_id {
        1 | 2 => CartridgeKind::Standard,
        3 => CartridgeKind::OssTwoChipLegacy,
        15 => CartridgeKind::OssOneChip,
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
        assert_eq!(cart.read(0xA000), 0);
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
        for addr in [0xD501, 0xD502, 0xD505, 0xD506] {
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
}
