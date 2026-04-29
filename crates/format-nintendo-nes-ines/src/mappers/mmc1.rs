//! MMC1 (Mapper 1, SxROM): serial-register PRG/CHR banking.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// MMC1 (Mapper 1, SxROM): serial-register PRG/CHR banking.
///
/// CPU writes load a 5-bit shift register one bit at a time. Once
/// complete, the address selects one of four internal registers:
/// control, CHR bank 0, CHR bank 1, or PRG bank. This supports MMC1's
/// 16 KiB and 32 KiB PRG modes, 4 KiB and 8 KiB CHR modes, dynamic
/// nametable mirroring, and the standard 8 KiB PRG-RAM window.
#[derive(Clone, Serialize, Deserialize)]
pub struct Mmc1 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    #[serde(with = "BigArray")]
    prg_ram: [u8; 8192],
    pub(crate) shift_register: u8,
    pub(crate) shift_count: u8,
    pub(crate) control: u8,
    pub(crate) chr_bank_0: u8,
    pub(crate) chr_bank_1: u8,
    pub(crate) prg_bank: u8,
}

impl Mmc1 {
    /// Construct MMC1 from parsed iNES payloads.
    ///
    /// `chr_data` is empty for CHR-RAM cartridges; in that case this
    /// allocates the standard 8 KiB CHR RAM window.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: [0; 8192],
            shift_register: 0,
            shift_count: 0,
            control: 0x0C,
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.prg_rom.len() / 16384).max(1)
    }

    fn read_prg(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_bank_count();
        self.prg_rom[bank * 16384 + offset]
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        if value & 0x80 != 0 {
            self.shift_register = 0;
            self.shift_count = 0;
            self.control |= 0x0C;
            return;
        }

        self.shift_register |= (value & 1) << self.shift_count;
        self.shift_count += 1;

        if self.shift_count == 5 {
            let data = self.shift_register;
            match (addr >> 13) & 0x03 {
                0 => self.control = data,
                1 => self.chr_bank_0 = data,
                2 => self.chr_bank_1 = data,
                3 => self.prg_bank = data,
                _ => unreachable!(),
            }
            self.shift_register = 0;
            self.shift_count = 0;
        }
    }

    fn chr_index(&self, addr: u16) -> usize {
        let addr = usize::from(addr) & 0x1FFF;
        let chr_mode = (self.control >> 4) & 1;
        if chr_mode == 0 {
            let bank_base = (usize::from(self.chr_bank_0) & 0x1E) * 4096;
            (bank_base + addr) % self.chr.len()
        } else {
            let bank = if addr < 0x1000 {
                self.chr_bank_0
            } else {
                self.chr_bank_1
            };
            let offset = addr & 0x0FFF;
            (usize::from(bank) * 4096 + offset) % self.chr.len()
        }
    }
}

impl Mapper for Mmc1 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[usize::from(addr - 0x6000)],
            0x8000..=0xBFFF => {
                let offset = usize::from(addr - 0x8000);
                match (self.control >> 2) & 0x03 {
                    0 | 1 => self.read_prg(usize::from(self.prg_bank & 0x0E), offset),
                    2 => self.read_prg(0, offset),
                    3 => self.read_prg(usize::from(self.prg_bank & 0x0F), offset),
                    _ => unreachable!(),
                }
            }
            0xC000..=0xFFFF => {
                let offset = usize::from(addr - 0xC000);
                match (self.control >> 2) & 0x03 {
                    0 | 1 => self.read_prg(usize::from(self.prg_bank & 0x0E) + 1, offset),
                    2 => self.read_prg(usize::from(self.prg_bank & 0x0F), offset),
                    3 => self.read_prg(self.prg_bank_count() - 1, offset),
                    _ => unreachable!(),
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                self.prg_ram[usize::from(addr - 0x6000)] = value;
            }
            0x8000..=0xFFFF => self.write_register(addr, value),
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[self.chr_index(addr)]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            let index = self.chr_index(addr);
            self.chr[index] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        match self.control & 0x03 {
            0 => Mirroring::SingleScreenLower,
            1 => Mirroring::SingleScreenUpper,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => unreachable!(),
        }
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Mmc1(self.clone())
    }
}

#[cfg(test)]
mod tests {
    //! Inline tests for MMC1 internal-state assertions.
    //!
    //! These tests poke at private fields (`shift_count`,
    //! `shift_register`, `control`, `chr_bank_*`, `prg_bank`) that
    //! are visible to crate-local code via `pub(crate)` but not to
    //! integration tests in `tests/`. Behaviour-only MMC1 tests live
    //! in `tests/parser.rs` alongside the rest.

    use super::*;

    fn make_mmc1(prg_banks: u8, chr_banks: u8) -> Mmc1 {
        let prg_size = usize::from(prg_banks) * 16384;
        let chr_size = usize::from(chr_banks) * 8192;
        let mut prg_rom = vec![0u8; prg_size];
        for bank in 0..usize::from(prg_banks) {
            for byte in &mut prg_rom[bank * 16384..(bank + 1) * 16384] {
                *byte = bank as u8;
            }
        }
        let chr_data = if chr_size > 0 {
            let mut chr = vec![0u8; chr_size];
            for page in 0..chr_size / 4096 {
                for byte in &mut chr[page * 4096..(page + 1) * 4096] {
                    *byte = page as u8;
                }
            }
            chr
        } else {
            Vec::new()
        };
        Mmc1::new(prg_rom, chr_data)
    }

    fn mmc1_write_5(mapper: &mut Mmc1, addr: u16, value: u8) {
        for bit in 0..5 {
            mapper.cpu_write(addr, (value >> bit) & 1);
        }
    }

    #[test]
    fn mmc1_reset_write_clears_shift_register_and_sets_prg_mode_3() {
        let mut mapper = make_mmc1(8, 2);
        mapper.cpu_write(0x8000, 1);
        mapper.cpu_write(0x8000, 0);

        mapper.cpu_write(0x8000, 0x80);

        assert_eq!(mapper.shift_count, 0);
        assert_eq!(mapper.shift_register, 0);
        assert_eq!((mapper.control >> 2) & 0x03, 3);
    }

    #[test]
    fn mmc1_loads_registers_lsb_first() {
        let mut mapper = make_mmc1(8, 2);

        mmc1_write_5(&mut mapper, 0x8000, 0b10101);
        mmc1_write_5(&mut mapper, 0xA000, 3);
        mmc1_write_5(&mut mapper, 0xC000, 5);
        mmc1_write_5(&mut mapper, 0xE000, 2);

        assert_eq!(mapper.control, 0b10101);
        assert_eq!(mapper.chr_bank_0, 3);
        assert_eq!(mapper.chr_bank_1, 5);
        assert_eq!(mapper.prg_bank, 2);
    }
}
