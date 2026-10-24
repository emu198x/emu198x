//! Action 53 (Mapper 28): multicart mapper with switchable 16/32 KiB
//! PRG modes, 8 KiB CHR RAM banking, and mapper-controlled mirroring.

use serde::{Deserialize, Serialize};

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// Action 53 (Mapper 28): multicart mapper with switchable 16/32 KiB
/// PRG modes, 8 KiB CHR RAM banking, and mapper-controlled mirroring.
#[derive(Clone, Serialize, Deserialize)]
pub struct Action53 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    register_select: u8,
    chr_bank: u8,
    inner_bank: u8,
    mode: u8,
    outer_bank: u8,
}

impl Action53 {
    /// Construct mapper 28 from parsed payloads.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr = if chr_data.is_empty() {
            vec![0u8; 32768]
        } else {
            chr_data
        };
        let last_16k_bank = (prg_rom.len() / 16384).saturating_sub(1);
        Self {
            prg_rom,
            chr,
            register_select: 0,
            chr_bank: 0,
            inner_bank: 0,
            mode: 0x0C,
            outer_bank: (last_16k_bank / 2) as u8,
        }
    }

    fn prg_16k_count(&self) -> usize {
        (self.prg_rom.len() / 16384).max(1)
    }

    fn replace_outer_low_bits(outer: u8, inner: u8, bits: u8) -> usize {
        if bits == 0 {
            usize::from(outer)
        } else {
            let mask = (1u8 << bits) - 1;
            usize::from((outer & !mask) | (inner & mask))
        }
    }

    fn prg_bank_for_window(&self, high_window: bool) -> usize {
        let prg_mode = (self.mode >> 2) & 0x03;
        let outer_size = (self.mode >> 4) & 0x03;
        let outer = self.outer_bank;
        let inner = self.inner_bank & 0x0F;

        let bank = match prg_mode {
            0 | 1 => {
                (Self::replace_outer_low_bits(outer, inner, outer_size) << 1)
                    | usize::from(high_window)
            }
            2 if high_window => Self::replace_outer_low_bits(outer, inner, outer_size + 1),
            2 => usize::from(outer) << 1,
            3 if high_window => (usize::from(outer) << 1) | 1,
            3 => Self::replace_outer_low_bits(outer, inner, outer_size + 1),
            _ => unreachable!(),
        };
        bank % self.prg_16k_count()
    }

    fn chr_bank_count(&self) -> usize {
        (self.chr.len() / 8192).max(1)
    }

    fn update_onescreen_bit(&mut self, value: u8) {
        if self.mode & 0x03 <= 1 {
            self.mode = (self.mode & !1) | ((value >> 4) & 1);
        }
    }
}

impl Mapper for Action53 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank_for_window(false);
                let offset = usize::from(addr - 0x8000);
                self.prg_rom[bank * 16384 + offset]
            }
            0xC000..=0xFFFF => {
                let bank = self.prg_bank_for_window(true);
                let offset = usize::from(addr - 0xC000);
                self.prg_rom[bank * 16384 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x5000..=0x5FFF => self.register_select = value & 0x81,
            0x8000..=0xFFFF => match self.register_select {
                0x00 => {
                    self.chr_bank = value & 0x03;
                    self.update_onescreen_bit(value);
                }
                0x01 => {
                    self.inner_bank = value & 0x0F;
                    self.update_onescreen_bit(value);
                }
                0x80 => self.mode = value & 0x3F,
                0x81 => self.outer_bank = value,
                _ => {}
            },
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let bank = usize::from(self.chr_bank) % self.chr_bank_count();
        let offset = usize::from(addr) & 0x1FFF;
        self.chr[bank * 8192 + offset]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        let bank = usize::from(self.chr_bank) % self.chr_bank_count();
        let offset = usize::from(addr) & 0x1FFF;
        self.chr[bank * 8192 + offset] = value;
    }

    fn mirroring(&self) -> Mirroring {
        match self.mode & 0x03 {
            0 => Mirroring::SingleScreenLower,
            1 => Mirroring::SingleScreenUpper,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => unreachable!(),
        }
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Action53(self.clone())
    }
}
