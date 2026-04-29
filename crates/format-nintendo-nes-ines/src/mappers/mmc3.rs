//! MMC3 (Mapper 4, TxROM): 8 KiB PRG banking, 1 KiB CHR banking,
//! PRG RAM protection, dynamic mirroring, and scanline IRQs.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// MMC3 (Mapper 4, TxROM): 8 KiB PRG banking, 1 KiB CHR banking,
/// PRG RAM protection, dynamic mirroring, and scanline IRQs.
///
/// MMC3 is used by a large part of the later NES library, including
/// *Super Mario Bros. 3*. The IRQ counter is clocked by debounced PPU
/// A12 rising edges reported through [`Mapper::notify_a12_rendering`].
#[derive(Clone, Serialize, Deserialize)]
pub struct Mmc3 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    #[serde(with = "BigArray")]
    prg_ram: [u8; 8192],
    bank_select: u8,
    registers: [u8; 8],
    mirroring: Mirroring,
    prg_ram_enable: bool,
    prg_ram_write_protect: bool,
    irq_latch: u8,
    irq_counter: u8,
    irq_reload_flag: bool,
    irq_enabled: bool,
    irq_pending: bool,
    last_a12: bool,
    dots_since_last_a12_rise: u16,
}

impl Mmc3 {
    /// Construct MMC3 from parsed iNES payloads.
    ///
    /// Empty CHR data means CHR RAM, allocated as an 8 KiB window.
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
            bank_select: 0,
            registers: [0; 8],
            mirroring: Mirroring::Vertical,
            prg_ram_enable: true,
            prg_ram_write_protect: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_reload_flag: false,
            irq_enabled: false,
            irq_pending: false,
            last_a12: false,
            dots_since_last_a12_rise: 0,
        }
    }

    fn prg_8k_count(&self) -> usize {
        (self.prg_rom.len() / 8192).max(1)
    }

    fn second_last_prg_bank(&self) -> usize {
        self.prg_8k_count().saturating_sub(2)
    }

    fn last_prg_bank(&self) -> usize {
        self.prg_8k_count() - 1
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }

    fn chr_1k_bank(&self, addr: u16) -> usize {
        let slot = (usize::from(addr) & 0x1FFF) >> 10;
        if self.bank_select & 0x80 != 0 {
            match slot {
                0 => usize::from(self.registers[2]),
                1 => usize::from(self.registers[3]),
                2 => usize::from(self.registers[4]),
                3 => usize::from(self.registers[5]),
                4 => usize::from(self.registers[0] & 0xFE),
                5 => usize::from(self.registers[0] | 1),
                6 => usize::from(self.registers[1] & 0xFE),
                7 => usize::from(self.registers[1] | 1),
                _ => unreachable!(),
            }
        } else {
            match slot {
                0 => usize::from(self.registers[0] & 0xFE),
                1 => usize::from(self.registers[0] | 1),
                2 => usize::from(self.registers[1] & 0xFE),
                3 => usize::from(self.registers[1] | 1),
                4 => usize::from(self.registers[2]),
                5 => usize::from(self.registers[3]),
                6 => usize::from(self.registers[4]),
                7 => usize::from(self.registers[5]),
                _ => unreachable!(),
            }
        }
    }

    fn chr_index(&self, addr: u16) -> usize {
        let offset = usize::from(addr) & 0x03FF;
        (self.chr_1k_bank(addr) * 1024 + offset) % self.chr.len()
    }

    fn update_a12(&mut self, a12_high: bool) {
        if a12_high && !self.last_a12 {
            if self.dots_since_last_a12_rise >= 15 {
                self.clock_irq_counter();
            }
            self.dots_since_last_a12_rise = 0;
        } else {
            self.dots_since_last_a12_rise = self.dots_since_last_a12_rise.saturating_add(1);
        }
        self.last_a12 = a12_high;
    }

    fn clock_irq_counter(&mut self) {
        if self.irq_counter == 0 || self.irq_reload_flag {
            self.irq_counter = self.irq_latch;
            self.irq_reload_flag = false;
        } else {
            self.irq_counter -= 1;
        }

        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_pending = true;
        }
    }
}

impl Mapper for Mmc3 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enable {
                    self.prg_ram[usize::from(addr - 0x6000)]
                } else {
                    0
                }
            }
            0x8000..=0x9FFF => {
                let offset = usize::from(addr - 0x8000);
                if self.bank_select & 0x40 == 0 {
                    self.read_prg_8k(usize::from(self.registers[6] & 0x3F), offset)
                } else {
                    self.read_prg_8k(self.second_last_prg_bank(), offset)
                }
            }
            0xA000..=0xBFFF => {
                let offset = usize::from(addr - 0xA000);
                self.read_prg_8k(usize::from(self.registers[7] & 0x3F), offset)
            }
            0xC000..=0xDFFF => {
                let offset = usize::from(addr - 0xC000);
                if self.bank_select & 0x40 == 0 {
                    self.read_prg_8k(self.second_last_prg_bank(), offset)
                } else {
                    self.read_prg_8k(usize::from(self.registers[6] & 0x3F), offset)
                }
            }
            0xE000..=0xFFFF => {
                let offset = usize::from(addr - 0xE000);
                self.read_prg_8k(self.last_prg_bank(), offset)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enable && !self.prg_ram_write_protect {
                    self.prg_ram[usize::from(addr - 0x6000)] = value;
                }
            }
            0x8000..=0x9FFF if addr & 1 == 0 => self.bank_select = value,
            0x8000..=0x9FFF => {
                let register = usize::from(self.bank_select & 0x07);
                self.registers[register] = value;
            }
            0xA000..=0xBFFF if addr & 1 == 0 => {
                self.mirroring = if value & 1 == 0 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
            }
            0xA000..=0xBFFF => {
                self.prg_ram_write_protect = value & 0x40 != 0;
                self.prg_ram_enable = value & 0x80 != 0;
            }
            0xC000..=0xDFFF if addr & 1 == 0 => self.irq_latch = value,
            0xC000..=0xDFFF => self.irq_reload_flag = true,
            0xE000..=0xFFFF if addr & 1 == 0 => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0xE000..=0xFFFF => self.irq_enabled = true,
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
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn notify_a12_rendering(&mut self, a12_high: bool) {
        self.update_a12(a12_high);
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Mmc3(self.clone())
    }
}
