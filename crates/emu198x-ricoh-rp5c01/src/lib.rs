//! Ricoh RP5C01 real-time clock, the MSX2's clock chip.
//!
//! The chip has sixteen four-bit registers on four address lines. Registers
//! 0-12 are a window onto one of four blocks of thirteen nibbles; registers
//! 13-15 are the mode, test and reset registers, the same in every block.
//!
//! | Block | Holds                                                        |
//! |-------|--------------------------------------------------------------|
//! | 0     | the time: seconds, minutes, hours, weekday, day, month, year |
//! | 1     | the alarm, the 12/24-hour select (10) and the leap year (11) |
//! | 2, 3  | 26 nibbles of battery-backed RAM                             |
//!
//! Every field is BCD, one digit a register, low digit first. A register
//! bit the chip does not have is ignored on a write and reads as 0.
//!
//! The mode register (13) selects the block in bits 0-1, enables the alarm
//! with bit 2 and lets time run with bit 3. The test register (14) makes the
//! seconds, minutes, days or years advance at the chip's internal rate
//! instead of once a carry. Writing bit 0 of the reset register (15) clears
//! the alarm; bit 1 clears the fraction of a second.
//!
//! # Time
//!
//! [`Rp5c01::tick`] is one period of the chip's 16,384 Hz divider, and the
//! machine calls it from its own clock. Time therefore passes with emulated
//! time rather than the host's clock: a run is deterministic and a snapshot
//! resumes on the tick it was taken. The clock starts at
//! 1980-01-01 00:00:00, a Tuesday, the first moment the year registers can
//! hold; [`Rp5c01::set_date_time`] sets it.
//!
//! In 12-hour mode (block 1 register 10 = 0, the power-on state) the tens of
//! hours register carries the PM flag in bit 1, so 1 p.m. reads as `21`.
//!
//! The alarm and the 1 Hz and 16 Hz outputs are not modelled: the MSX does
//! not connect them.
//!
//! # Provenance
//!
//! The register masks, the 12/24-hour encoding and the test-mode carries
//! are facts read from openMSX (GPL-2.0, read for facts only, not code), at
//! commit `38af174e2785b490de07bb0ff891e950e64a3bcd`.

use serde::{Deserialize, Serialize};

/// The chip's internal rate: [`Rp5c01::tick`] calls per second.
pub const TICK_HZ: u32 = 16_384;

/// Nibbles in a block.
const BLOCK: usize = 13;

const MODE_REG: u8 = 13;
const TEST_REG: u8 = 14;
const RESET_REG: u8 = 15;

const MODE_BLOCK: u8 = 0x3;
const MODE_TIMER_ENABLE: u8 = 0x8;

const TEST_SECONDS: u8 = 0x1;
const TEST_MINUTES: u8 = 0x2;
const TEST_DAYS: u8 = 0x4;
const TEST_YEARS: u8 = 0x8;

const RESET_ALARM: u8 = 0x1;
const RESET_FRACTION: u8 = 0x2;

/// Block 1's 12/24-hour select; 1 means 24-hour.
const HOUR_MODE: usize = BLOCK + 10;
/// Block 1's leap-year counter; 0 means this year is a leap year.
const LEAP_YEAR: usize = BLOCK + 11;

/// The bits each register of each block has.
const MASKS: [[u8; BLOCK]; 4] = [
    [
        0xF, 0x7, 0xF, 0x7, 0xF, 0x3, 0x7, 0xF, 0x3, 0xF, 0x1, 0xF, 0xF,
    ],
    [
        0x0, 0x0, 0xF, 0x7, 0xF, 0x3, 0x7, 0xF, 0x3, 0x0, 0x1, 0x3, 0x0,
    ],
    [0xF; BLOCK],
    [0xF; BLOCK],
];

const DAYS_IN_MONTH: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// A calendar time as the chip counts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateTime {
    /// 1980-2079: the chip holds two digits from 1980.
    pub year: u16,
    /// 1-12.
    pub month: u8,
    /// 1-31.
    pub day: u8,
    /// 0-6. The chip only counts it; the BIOS decides which day is 0.
    pub weekday: u8,
    /// 0-23.
    pub hour: u8,
    /// 0-59.
    pub minute: u8,
    /// 0-59.
    pub second: u8,
}

/// The RP5C01.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rp5c01 {
    /// Four blocks of thirteen nibbles.
    regs: Vec<u8>,
    mode: u8,
    test: u8,
    /// Ticks into the current second.
    fraction: u32,
    // The counters the time block displays, kept in binary.
    seconds: u32,
    minutes: u32,
    hours: u32,
    weekday: u32,
    /// 0-30.
    days: u32,
    /// 0-11.
    months: u32,
    /// 0-99, from 1980.
    years: u32,
    /// 0-3; 0 is a leap year.
    leap: u32,
}

impl Default for Rp5c01 {
    fn default() -> Self {
        Self::new()
    }
}

impl Rp5c01 {
    /// A clock at 1980-01-01 00:00:00, a Tuesday, running, in 12-hour mode,
    /// with its RAM clear.
    #[must_use]
    pub fn new() -> Self {
        let mut rtc = Self {
            regs: vec![0; 4 * BLOCK],
            mode: MODE_TIMER_ENABLE,
            test: 0,
            fraction: 0,
            seconds: 0,
            minutes: 0,
            hours: 0,
            weekday: 2,
            days: 0,
            months: 0,
            years: 0,
            leap: 0,
        };
        rtc.time_to_regs();
        rtc
    }

    /// Set the time and date. Out-of-range fields are clamped, and the leap
    /// year counter follows the year.
    pub fn set_date_time(&mut self, time: DateTime) {
        let year = u32::from(time.year.clamp(1980, 2079)) - 1980;
        self.years = year;
        self.leap = year % 4;
        self.months = u32::from(time.month.clamp(1, 12)) - 1;
        self.days = u32::from(time.day.clamp(1, 31)) - 1;
        self.weekday = u32::from(time.weekday.min(6));
        self.hours = u32::from(time.hour.min(23));
        self.minutes = u32::from(time.minute.min(59));
        self.seconds = u32::from(time.second.min(59));
        self.fraction = 0;
        self.time_to_regs();
    }

    /// The time and date the chip holds.
    #[must_use]
    pub fn date_time(&self) -> DateTime {
        DateTime {
            year: 1980 + self.years as u16,
            month: self.months as u8 + 1,
            day: self.days as u8 + 1,
            weekday: self.weekday as u8,
            hour: self.hours as u8,
            minute: self.minutes as u8,
            second: self.seconds as u8,
        }
    }

    /// Read register `reg` (0-15) as a nibble. The test and reset registers
    /// are write-only and read as `0xF`.
    #[must_use]
    pub fn read(&self, reg: u8) -> u8 {
        match reg & 0x0F {
            MODE_REG => self.mode,
            TEST_REG | RESET_REG => 0x0F,
            reg => {
                let block = usize::from(self.mode & MODE_BLOCK);
                let reg = usize::from(reg);
                self.regs[block * BLOCK + reg] & MASKS[block][reg]
            }
        }
    }

    /// Write the low nibble of `value` to register `reg` (0-15).
    pub fn write(&mut self, reg: u8, value: u8) {
        let value = value & 0x0F;
        match reg & 0x0F {
            MODE_REG => self.mode = value,
            TEST_REG => self.test = value,
            RESET_REG => {
                if value & RESET_ALARM != 0 {
                    // The alarm's minutes, hours, weekday and day.
                    self.regs[BLOCK + 2..BLOCK + 9].fill(0);
                }
                if value & RESET_FRACTION != 0 {
                    self.fraction = 0;
                }
            }
            reg => {
                let block = usize::from(self.mode & MODE_BLOCK);
                let reg = usize::from(reg);
                self.regs[block * BLOCK + reg] = value & MASKS[block][reg];
                if block < 2 {
                    self.regs_to_time();
                }
            }
        }
    }

    /// The 26 nibbles of battery-backed RAM, blocks 2 and 3.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.regs[2 * BLOCK..]
    }

    /// Restore the battery-backed RAM from `nibbles`, as saved from
    /// [`Rp5c01::ram`]. Extra bytes are ignored and each keeps its low
    /// nibble.
    pub fn load_ram(&mut self, nibbles: &[u8]) {
        for (reg, value) in self.regs[2 * BLOCK..].iter_mut().zip(nibbles) {
            *reg = value & 0x0F;
        }
    }

    /// One period of the 16,384 Hz divider.
    pub fn tick(&mut self) {
        if self.mode & MODE_TIMER_ENABLE != 0 {
            self.fraction += 1;
        }
        let carry_seconds = if self.test & TEST_SECONDS != 0 {
            1
        } else {
            self.fraction / TICK_HZ
        };
        if carry_seconds == 0 && self.test == 0 {
            return;
        }
        self.fraction %= TICK_HZ;
        self.seconds += carry_seconds;
        let carry_minutes = if self.test & TEST_MINUTES != 0 {
            1
        } else {
            self.seconds / 60
        };
        self.minutes += carry_minutes;
        self.hours += self.minutes / 60;
        let carry_days = if self.test & TEST_DAYS != 0 {
            1
        } else {
            self.hours / 24
        };
        if carry_days != 0 {
            // Roll the month over only when a day passes, so a BIOS writing
            // a date one digit at a time can pass through 31 February.
            self.days += carry_days;
            self.weekday += carry_days;
            while self.days >= self.days_in_month() {
                self.days -= self.days_in_month();
                self.months += 1;
            }
        }
        let carry_years = if self.test & TEST_YEARS != 0 {
            1
        } else {
            self.months / 12
        };
        self.years += carry_years;
        self.leap += carry_years;
        self.seconds %= 60;
        self.minutes %= 60;
        self.hours %= 24;
        self.weekday %= 7;
        self.months %= 12;
        self.years %= 100;
        self.leap %= 4;
        self.time_to_regs();
    }

    fn days_in_month(&self) -> u32 {
        let month = (self.months % 12) as usize;
        if month == 1 && self.leap == 0 {
            29
        } else {
            u32::from(DAYS_IN_MONTH[month])
        }
    }

    fn twenty_four_hour(&self) -> bool {
        self.regs[HOUR_MODE] != 0
    }

    /// Read the counters back from the time block after a CPU write.
    fn regs_to_time(&mut self) {
        let digits = |low: usize| u32::from(self.regs[low]) + 10 * u32::from(self.regs[low + 1]);
        self.seconds = digits(0);
        self.minutes = digits(2);
        self.hours = digits(4);
        self.weekday = u32::from(self.regs[6]);
        self.days = digits(7).saturating_sub(1);
        self.months = digits(9).saturating_sub(1);
        self.years = digits(11);
        self.leap = u32::from(self.regs[LEAP_YEAR]);
        if !self.twenty_four_hour() && self.hours >= 20 {
            self.hours = self.hours - 20 + 12;
        }
    }

    /// Show the counters in the time block.
    fn time_to_regs(&mut self) {
        let hours = if !self.twenty_four_hour() && self.hours >= 12 {
            self.hours - 12 + 20
        } else {
            self.hours
        };
        let fields = [
            self.seconds,
            self.minutes,
            hours,
            self.weekday,
            self.days + 1,
            self.months + 1,
            self.years,
        ];
        let mut reg = 0;
        for (index, value) in fields.into_iter().enumerate() {
            if index == 3 {
                self.regs[reg] = value as u8;
                reg += 1;
            } else {
                self.regs[reg] = (value % 10) as u8;
                self.regs[reg + 1] = (value / 10) as u8;
                reg += 2;
            }
        }
        self.regs[LEAP_YEAR] = self.leap as u8;
    }
}

#[cfg(test)]
mod tests;
