//! MBC3 — used by Pokémon Red/Blue/Yellow/Gold/Silver/Crystal and a
//! lot of other late-DMG titles. Adds an optional real-time clock.
//!
//! | Range            | Effect                                           |
//! |------------------|--------------------------------------------------|
//! | `$0000..=$1FFF`  | RAM + RTC enable: low nibble == `$A` enables     |
//! | `$2000..=$3FFF`  | ROM bank (7 bits; writing 0 reads as 1)          |
//! | `$4000..=$5FFF`  | RAM bank `$00..=$03` OR RTC register `$08..=$0C` |
//! | `$6000..=$7FFF`  | Latch RTC: write `0` then `1` snapshots          |
//!
//! The RTC has five registers: seconds (`$08`), minutes (`$09`),
//! hours (`$0A`), day low byte (`$0B`), day high + control (`$0C`).
//! We model the registers but don't advance them — the machine
//! layer can drive a wall-clock-driven advance once it exists.

use serde::{Deserialize, Serialize};

const ROM_BANK_SIZE: usize = 0x4000;
const RAM_BANK_SIZE: usize = 0x2000;

/// RTC register snapshot — five live values plus their latched
/// counterparts. Reads from `$A000..=$BFFF` go through the latched
/// values whenever an RTC register is selected; live values keep
/// updating in the background.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RtcRegisters {
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub day_low: u8,
    /// Bits: 0 = day high bit 8, 6 = halt, 7 = day-counter carry.
    pub day_high_ctrl: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Mbc3 {
    pub ram_enabled: bool,
    /// 7-bit ROM bank. `0` reads as `1`.
    pub rom_bank: u8,
    /// Currently selected RAM bank (`0..=3`) or RTC register
    /// (`0x08..=0x0C`).
    pub ram_bank: u8,
    /// Live RTC values.
    pub rtc: RtcRegisters,
    /// Latched RTC values returned by reads while the latch is
    /// active.
    pub rtc_latched: RtcRegisters,
    /// Latch-sequence state: previous write to `$6000..=$7FFF`. The
    /// 0→1 transition latches RTC.
    pub latch_prev: u8,
    pub has_rtc: bool,
}

impl Mbc3 {
    #[must_use]
    pub const fn new(has_rtc: bool) -> Self {
        Self {
            ram_enabled: false,
            rom_bank: 1,
            ram_bank: 0,
            rtc: RtcRegisters {
                seconds: 0,
                minutes: 0,
                hours: 0,
                day_low: 0,
                day_high_ctrl: 0,
            },
            rtc_latched: RtcRegisters {
                seconds: 0,
                minutes: 0,
                hours: 0,
                day_low: 0,
                day_high_ctrl: 0,
            },
            latch_prev: 0xFF,
            has_rtc,
        }
    }

    pub(crate) fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        let bank = if addr < 0x4000 {
            0
        } else {
            usize::from(self.rom_bank.max(1))
        };
        let offset = bank * ROM_BANK_SIZE + usize::from(addr & 0x3FFF);
        rom.get(offset).copied().unwrap_or(0xFF)
    }

    pub(crate) fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = (value & 0x0F) == 0x0A,
            0x2000..=0x3FFF => {
                let bank = value & 0x7F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => self.ram_bank = value,
            0x6000..=0x7FFF => {
                if self.latch_prev == 0 && value == 1 && self.has_rtc {
                    self.rtc_latched = self.rtc;
                }
                self.latch_prev = value;
            }
            _ => {}
        }
    }

    pub(crate) fn read_ram(&self, ram: &[u8], addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }
        match self.ram_bank {
            0x00..=0x03 => {
                let offset = usize::from(self.ram_bank) * RAM_BANK_SIZE
                    + usize::from(addr.wrapping_sub(0xA000) & 0x1FFF);
                ram.get(offset).copied().unwrap_or(0xFF)
            }
            0x08 if self.has_rtc => self.rtc_latched.seconds,
            0x09 if self.has_rtc => self.rtc_latched.minutes,
            0x0A if self.has_rtc => self.rtc_latched.hours,
            0x0B if self.has_rtc => self.rtc_latched.day_low,
            0x0C if self.has_rtc => self.rtc_latched.day_high_ctrl,
            _ => 0xFF,
        }
    }

    pub(crate) fn write_ram(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        if !self.ram_enabled {
            return;
        }
        match self.ram_bank {
            0x00..=0x03 => {
                let offset = usize::from(self.ram_bank) * RAM_BANK_SIZE
                    + usize::from(addr.wrapping_sub(0xA000) & 0x1FFF);
                if let Some(slot) = ram.get_mut(offset) {
                    *slot = value;
                }
            }
            0x08 if self.has_rtc => self.rtc.seconds = value & 0x3F,
            0x09 if self.has_rtc => self.rtc.minutes = value & 0x3F,
            0x0A if self.has_rtc => self.rtc.hours = value & 0x1F,
            0x0B if self.has_rtc => self.rtc.day_low = value,
            0x0C if self.has_rtc => self.rtc.day_high_ctrl = value & 0xC1,
            _ => {}
        }
    }
}
