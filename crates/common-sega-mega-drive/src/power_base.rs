//! The Power Base Converter: a Master System cartridge on a Mega Drive.
//!
//! The converter grounds the cartridge port's /M3 line, and the console
//! wakes in Master System mode: the 68000 is held off, the Z80 runs alone
//! at its usual 3.58 MHz with the Master System's map, the VDP is driven
//! through its 8-bit ports in Mode 4 (see `emu198x-sega-315-5313`), and the
//! I/O chip presents the pads as a Master System's ports `$DC` and `$DD`.
//! The YM2612 is unreachable.
//!
//! # The Z80's map
//!
//! | Range | Contents |
//! |---|---|
//! | `$0000-$03FF` | Cartridge bank 0, fixed (Sega mapper) |
//! | `$0000-$3FFF` | Slot 0, paged by `$FFFD` |
//! | `$4000-$7FFF` | Slot 1, paged by `$FFFE` |
//! | `$8000-$BFFF` | Slot 2, paged by `$FFFF`, or cartridge RAM |
//! | `$C000-$FFFF` | 8 KB of RAM, mirrored; `$FFFC-$FFFF` also write the mapper |
//!
//! Codemasters cartridges page each slot by a write to its first address,
//! and bank all of slot 0.
//!
//! # The Z80's ports
//!
//! Decoded by A7, A6 and A0, as on a Master System: `$3F` the I/O control
//! (TH direction and level), `$7E`/`$7F` the counters and the PSG, `$BE`/
//! `$BF` the VDP, `$DC`/`$DD` the pads. The memory control port `$3E` has
//! nothing to switch: there is no BIOS.
//!
//! # Pause
//!
//! A Master System's pause button drives the Z80's NMI. The converter
//! carries that button itself; this model wires it to the Start button of
//! the pad in port 1, which is how emulators commonly offer it.
//!
//! The mapper follows `machine-sega-master-system`'s, which keeps its
//! banking inside its own machine struct; sharing it would need it moved
//! to a crate both machines can use (see the crate docs).

use serde::{Deserialize, Serialize};

use crate::io::Buttons;

/// Where a Master System cartridge's `TMR SEGA` header may sit.
const HEADER_OFFSETS: [usize; 3] = [0x1FF0, 0x3FF0, 0x7FF0];
/// Where a Codemasters cartridge's own header sits.
const CODEMASTERS_HEADER: usize = 0x7FE0;
/// The cartridge RAM the Sega mapper can page in: two 16 KB banks.
const CART_RAM: usize = 0x8000;

/// Whether `rom` carries a Master System cartridge header.
#[must_use]
pub fn is_master_system_image(rom: &[u8]) -> bool {
    HEADER_OFFSETS
        .iter()
        .any(|&offset| rom.get(offset..offset + 8) == Some(b"TMR SEGA".as_slice()))
}

/// The cartridge's bank-switching scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mapper {
    /// Sega's: page registers at `$FFFD-$FFFF`, the first 1 KB fixed.
    Sega,
    /// Codemasters': a page register at the start of each slot.
    Codemasters,
}

impl Mapper {
    /// The scheme a cartridge carries. A Codemasters image has its own
    /// header at `$7FE0`: a page count matching the image, a checksum and
    /// its complement summing to `$10000`, then six zero bytes.
    #[must_use]
    pub fn detect(rom: &[u8]) -> Self {
        let Some(header) = rom.get(CODEMASTERS_HEADER..CODEMASTERS_HEADER + 0x10) else {
            return Self::Sega;
        };
        let checksum = u16::from_le_bytes([header[6], header[7]]);
        let complement = u16::from_le_bytes([header[8], header[9]]);
        let codemasters = checksum != 0
            && checksum.wrapping_add(complement) == 0
            && header[10..].iter().all(|&b| b == 0)
            && usize::from(header[0]) * 0x4000 == rom.len();
        if codemasters {
            Self::Codemasters
        } else {
            Self::Sega
        }
    }
}

/// The converter's state: the cartridge's mapper, its RAM, and the I/O
/// control register.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerBase {
    pub mapper: Mapper,
    /// Mapper control and the pages in slots 0-2.
    pub pages: [u8; 4],
    cart_ram: Vec<u8>,
    /// Port `$3F`: bits 1 and 3 make port A's and B's TH inputs; bits 5
    /// and 7 are their output levels.
    pub io_control: u8,
}

impl PowerBase {
    #[must_use]
    pub fn new(rom: &[u8]) -> Self {
        Self {
            mapper: Mapper::detect(rom),
            pages: [0, 0, 1, 2],
            cart_ram: vec![0; CART_RAM],
            io_control: 0xFF,
        }
    }

    fn rom_byte(rom: &[u8], page: u8, offset: u16) -> u8 {
        let pages = (rom.len() / 0x4000).max(1);
        let mask = pages.next_power_of_two() - 1;
        let address = (usize::from(page) & mask) * 0x4000 + usize::from(offset & 0x3FFF);
        rom.get(address).copied().unwrap_or(0xFF)
    }

    fn cart_ram_index(&self, address: u16) -> usize {
        let bank = usize::from(self.pages[0] & 0x04 != 0);
        bank * 0x4000 + usize::from(address & 0x3FFF)
    }

    /// The byte at `address` in the Z80's map; `ram` is the 8 KB of work
    /// RAM.
    #[must_use]
    pub fn read(&self, rom: &[u8], ram: &[u8], address: u16) -> u8 {
        match address {
            0x0000..=0x03FF if self.mapper == Mapper::Sega => Self::rom_byte(rom, 0, address),
            0x0000..=0x3FFF => Self::rom_byte(rom, self.pages[1], address),
            0x4000..=0x7FFF => Self::rom_byte(rom, self.pages[2], address),
            0x8000..=0xBFFF if self.pages[0] & 0x08 != 0 => {
                self.cart_ram[self.cart_ram_index(address)]
            }
            0x8000..=0xBFFF => Self::rom_byte(rom, self.pages[3], address),
            _ => ram[usize::from(address) & 0x1FFF],
        }
    }

    /// A Z80 write.
    pub fn write(&mut self, ram: &mut [u8], address: u16, value: u8) {
        if self.mapper == Mapper::Codemasters {
            match address {
                0x0000 => self.pages[1] = value,
                0x4000 => self.pages[2] = value,
                0x8000 => self.pages[3] = value,
                _ => {}
            }
        }
        match address {
            0x8000..=0xBFFF if self.pages[0] & 0x08 != 0 => {
                let index = self.cart_ram_index(address);
                self.cart_ram[index] = value;
            }
            0xC000..=0xFFFF => {
                ram[usize::from(address) & 0x1FFF] = value;
                if self.mapper == Mapper::Sega && address >= 0xFFFC {
                    self.pages[usize::from(address - 0xFFFC)] = value;
                }
            }
            _ => {}
        }
    }

    /// Port `$DC`: port A's six lines, and port B's up and down.
    #[must_use]
    pub fn port_dc(a: &Buttons, b: &Buttons) -> u8 {
        pad_lines(a) | (pad_lines(b) & 0x03) << 6
    }

    /// Port `$DD`: port B's other four lines, the reset button (not
    /// fitted, so high), and the two TH pins. A TH pin made an output
    /// reads back its level on export hardware and low on Japanese.
    #[must_use]
    pub fn port_dd(&self, b: &Buttons, japan: bool) -> u8 {
        let mut value = pad_lines(b) >> 2 | 0x30 | 0xC0;
        for (input, output, bit) in [(0x02, 0x20, 0x40), (0x08, 0x80, 0x80)] {
            if self.io_control & input == 0 {
                value &= !bit;
                if !japan && self.io_control & output != 0 {
                    value |= bit;
                }
            }
        }
        value
    }
}

/// A pad's six lines with TH high, active low: up, down, left, right, then
/// button 1 (the pad's B) and button 2 (its C), as a Master System pad
/// presents them.
fn pad_lines(b: &Buttons) -> u8 {
    let bit = |pressed: bool, n: u8| if pressed { 0 } else { 1 << n };
    bit(b.up, 0) | bit(b.down, 1) | bit(b.left, 2) | bit(b.right, 3) | bit(b.b, 4) | bit(b.c, 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paged_rom(pages: usize) -> Vec<u8> {
        (0..pages)
            .flat_map(|page| vec![page as u8; 0x4000])
            .collect()
    }

    #[test]
    fn the_sega_mapper_pages_three_slots_and_fixes_the_first_kilobyte() {
        let rom = paged_rom(8);
        let mut ram = vec![0; 0x2000];
        let mut pb = PowerBase::new(&rom);
        assert_eq!(pb.mapper, Mapper::Sega);
        assert_eq!(pb.read(&rom, &ram, 0x4000), 1);
        pb.write(&mut ram, 0xFFFD, 5);
        pb.write(&mut ram, 0xFFFF, 0x83);
        assert_eq!(pb.read(&rom, &ram, 0x0000), 0, "first 1 KB fixed");
        assert_eq!(pb.read(&rom, &ram, 0x0400), 5);
        assert_eq!(pb.read(&rom, &ram, 0x8000), 3, "masked to the ROM");
        assert_eq!(pb.read(&rom, &ram, 0xFFFF), 0x83, "the RAM shadows it");
        assert_eq!(pb.read(&rom, &ram, 0xDFFF), 0x83, "mirrored");
    }

    #[test]
    fn cartridge_ram_replaces_slot_2_when_enabled() {
        let rom = paged_rom(4);
        let mut ram = vec![0; 0x2000];
        let mut pb = PowerBase::new(&rom);
        pb.write(&mut ram, 0x8000, 0x55);
        assert_eq!(
            pb.read(&rom, &ram, 0x8000),
            2,
            "ROM: the write went nowhere"
        );
        pb.write(&mut ram, 0xFFFC, 0x08);
        pb.write(&mut ram, 0x8000, 0x55);
        assert_eq!(pb.read(&rom, &ram, 0x8000), 0x55);
        pb.write(&mut ram, 0xFFFC, 0x0C);
        assert_eq!(pb.read(&rom, &ram, 0x8000), 0, "the other bank");
    }

    #[test]
    fn a_codemasters_image_pages_by_slot_strobes() {
        let mut rom = paged_rom(4);
        let header = &mut rom[CODEMASTERS_HEADER..CODEMASTERS_HEADER + 0x10];
        header[0] = 4;
        header[6..8].copy_from_slice(&0x1234u16.to_le_bytes());
        header[8..10].copy_from_slice(&0x1234u16.wrapping_neg().to_le_bytes());
        header[10..].fill(0);
        let mut ram = vec![0; 0x2000];
        let mut pb = PowerBase::new(&rom);
        assert_eq!(pb.mapper, Mapper::Codemasters);
        pb.write(&mut ram, 0x0000, 3);
        assert_eq!(pb.read(&rom, &ram, 0x0000), 3, "all of slot 0 banks");
        pb.write(&mut ram, 0xFFFF, 1);
        assert_eq!(pb.read(&rom, &ram, 0x8000), 2, "$FFFF is plain RAM");
    }

    #[test]
    fn the_pads_read_as_master_system_ports() {
        let a = Buttons {
            up: true,
            b: true,
            ..Buttons::default()
        };
        let b = Buttons {
            down: true,
            right: true,
            c: true,
            ..Buttons::default()
        };
        let pb = PowerBase::new(&[]);
        assert_eq!(PowerBase::port_dc(&a, &b), 0b0110_1110);
        assert_eq!(pb.port_dd(&b, false), 0b1111_0101);
    }

    #[test]
    fn th_outputs_read_back_on_export_consoles_only() {
        let mut pb = PowerBase::new(&[]);
        let pads = Buttons::default();
        pb.io_control = 0xF5; // both TH outputs, driven high
        assert_eq!(pb.port_dd(&pads, false) & 0xC0, 0xC0);
        assert_eq!(pb.port_dd(&pads, true) & 0xC0, 0x00);
        pb.io_control = 0x55; // driven low
        assert_eq!(pb.port_dd(&pads, false) & 0xC0, 0x00);
    }

    #[test]
    fn a_header_identifies_a_master_system_image() {
        let mut rom = vec![0; 0x8000];
        assert!(!is_master_system_image(&rom));
        rom[0x7FF0..0x7FF8].copy_from_slice(b"TMR SEGA");
        assert!(is_master_system_image(&rom));
    }
}
