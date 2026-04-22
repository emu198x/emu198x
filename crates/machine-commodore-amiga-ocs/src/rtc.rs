//! Battery-backed RTC (MSM6242B-style old-address clock).
//!
//! AmigaOS 1.3-era utilities like `SetClock load` probe the "old
//! address" RTC directly at `$DC0000` when running on A500/A2000-class
//! machines with an expansion clock. The hardware exposes sixteen
//! 4-bit registers, each on a 32-bit boundary. To match the broad
//! access patterns Amiga code uses, the machine routes any byte/word
//! access within a 4-byte slot to the same nibble register.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const RTC_BASE: u32 = 0x00DC_0000;

const REG_SECOND_1: usize = 0x0;
const REG_SECOND_10: usize = 0x1;
const REG_MINUTE_1: usize = 0x2;
const REG_MINUTE_10: usize = 0x3;
const REG_HOUR_1: usize = 0x4;
const REG_HOUR_10: usize = 0x5;
const REG_DAY_1: usize = 0x6;
const REG_DAY_10: usize = 0x7;
const REG_MONTH_1: usize = 0x8;
const REG_MONTH_10: usize = 0x9;
const REG_YEAR_1: usize = 0xA;
const REG_YEAR_10: usize = 0xB;
const REG_WEEKDAY: usize = 0xC;
const REG_CD: usize = 0xD;
const REG_CE: usize = 0xE;
const REG_CF: usize = 0xF;

const CD_IRQ_FLAG: u8 = 1 << 2;
const CD_BUSY: u8 = 1 << 1;
const CD_HOLD: u8 = 1 << 0;

const CF_24H: u8 = 1 << 2;
const CF_STOP: u8 = 1 << 1;
const CF_RESET: u8 = 1 << 0;

const HOUR10_PM: u8 = 1 << 2;
const HOUR10_MASK: u8 = 0b0011;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CalendarTime {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    weekday: u8, // Sunday = 0
}

#[derive(Debug, Clone)]
pub struct Msm6242Rtc {
    unix_seconds: i64,
    host_reference: SystemTime,
    control_d: u8,
    control_e: u8,
    control_f: u8,
}

impl Default for Msm6242Rtc {
    fn default() -> Self {
        Self::new()
    }
}

impl Msm6242Rtc {
    #[must_use]
    pub fn new() -> Self {
        let now = SystemTime::now();
        let unix_seconds = system_time_to_unix_seconds(now);
        Self {
            unix_seconds,
            host_reference: now,
            control_d: CD_IRQ_FLAG,
            control_e: 0,
            control_f: CF_24H,
        }
    }

    #[cfg(test)]
    fn with_unix_seconds_for_test(unix_seconds: i64) -> Self {
        Self {
            unix_seconds,
            host_reference: SystemTime::now(),
            // Freeze the clock so tests observe the fixed timestamp
            // instead of accumulating host elapsed time.
            control_d: CD_IRQ_FLAG | CD_HOLD,
            control_e: 0,
            control_f: CF_24H,
        }
    }

    #[must_use]
    pub fn read_byte(&self, addr24: u32) -> u8 {
        let reg = reg_index(addr24);
        self.read_nibble(reg)
    }

    #[must_use]
    pub fn read_word(&self, addr24: u32) -> u16 {
        let byte = u16::from(self.read_byte(addr24));
        (byte << 8) | byte
    }

    pub fn write_byte(&mut self, addr24: u32, value: u8) {
        let reg = reg_index(addr24);
        self.write_nibble(reg, value & 0x0F);
    }

    pub fn write_word(&mut self, addr24: u32, value: u16) {
        let hi = ((value >> 8) & 0x0F) as u8;
        let lo = (value & 0x0F) as u8;
        let nibble = if lo != 0 || hi == 0 { lo } else { hi };
        self.write_byte(addr24, nibble);
    }

    fn running(&self) -> bool {
        (self.control_d & CD_HOLD) == 0 && (self.control_f & CF_STOP) == 0
    }

    fn effective_unix_seconds(&self) -> i64 {
        if !self.running() {
            return self.unix_seconds;
        }
        let elapsed = match SystemTime::now().duration_since(self.host_reference) {
            Ok(duration) => duration,
            Err(_) => Duration::ZERO,
        };
        self.unix_seconds
            .saturating_add(i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
    }

    fn sync_to_now(&mut self) {
        self.unix_seconds = self.effective_unix_seconds();
        self.host_reference = SystemTime::now();
    }

    fn read_nibble(&self, reg: usize) -> u8 {
        let calendar = CalendarTime::from_unix_seconds(self.effective_unix_seconds());
        match reg {
            REG_SECOND_1 => calendar.second % 10,
            REG_SECOND_10 => calendar.second / 10,
            REG_MINUTE_1 => calendar.minute % 10,
            REG_MINUTE_10 => calendar.minute / 10,
            REG_HOUR_1 => self.hour_ones(calendar),
            REG_HOUR_10 => self.hour_tens(calendar),
            REG_DAY_1 => calendar.day % 10,
            REG_DAY_10 => calendar.day / 10,
            REG_MONTH_1 => calendar.month % 10,
            REG_MONTH_10 => calendar.month / 10,
            REG_YEAR_1 => (calendar.year.rem_euclid(100) as u8) % 10,
            REG_YEAR_10 => (calendar.year.rem_euclid(100) as u8) / 10,
            REG_WEEKDAY => calendar.weekday & 0x0F,
            REG_CD => (self.control_d & !CD_BUSY) | CD_IRQ_FLAG,
            REG_CE => self.control_e & 0x0F,
            REG_CF => self.control_f & 0x0F,
            _ => 0,
        }
    }

    fn write_nibble(&mut self, reg: usize, nibble: u8) {
        if reg == REG_CD {
            let was_running = self.running();
            if was_running {
                self.sync_to_now();
            }
            self.control_d = (nibble & !CD_BUSY) | CD_IRQ_FLAG;
            if !was_running && self.running() {
                self.host_reference = SystemTime::now();
            }
            return;
        }
        if reg == REG_CE {
            self.control_e = nibble & 0x0F;
            return;
        }
        if reg == REG_CF {
            let was_running = self.running();
            if was_running {
                self.sync_to_now();
            }
            self.control_f = nibble & 0x0F;
            if (self.control_f & CF_RESET) != 0 {
                self.unix_seconds = CalendarTime {
                    year: 1978,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    weekday: 0,
                }
                .to_unix_seconds();
                self.control_f &= !CF_RESET;
            }
            if !was_running && self.running() {
                self.host_reference = SystemTime::now();
            }
            return;
        }

        self.sync_to_now();
        let mut calendar = CalendarTime::from_unix_seconds(self.unix_seconds);
        match reg {
            REG_SECOND_1 => calendar.second = (calendar.second / 10) * 10 + nibble.min(9),
            REG_SECOND_10 => calendar.second = (nibble.min(5) * 10) + (calendar.second % 10),
            REG_MINUTE_1 => calendar.minute = (calendar.minute / 10) * 10 + nibble.min(9),
            REG_MINUTE_10 => calendar.minute = (nibble.min(5) * 10) + (calendar.minute % 10),
            REG_HOUR_1 => {
                let tens = self.hour_tens(calendar);
                calendar.hour = self.decode_hour(tens, nibble.min(9), calendar.hour)
            }
            REG_HOUR_10 => {
                let ones = self.hour_ones(calendar);
                calendar.hour = self.decode_hour(nibble & 0x0F, ones, calendar.hour)
            }
            REG_DAY_1 => calendar.day = ((calendar.day / 10) * 10 + nibble.min(9)).clamp(1, 31),
            REG_DAY_10 => calendar.day = ((nibble.min(3) * 10) + (calendar.day % 10)).clamp(1, 31),
            REG_MONTH_1 => {
                calendar.month = ((calendar.month / 10) * 10 + nibble.min(9)).clamp(1, 12)
            }
            REG_MONTH_10 => {
                calendar.month = ((nibble.min(1) * 10) + (calendar.month % 10)).clamp(1, 12)
            }
            REG_YEAR_1 => {
                let year = calendar.year.rem_euclid(100) as u8;
                calendar.year = expand_two_digit_year((year / 10) * 10 + nibble.min(9))
            }
            REG_YEAR_10 => {
                let year = calendar.year.rem_euclid(100) as u8;
                calendar.year = expand_two_digit_year((nibble.min(9) * 10) + (year % 10))
            }
            REG_WEEKDAY => calendar.weekday = nibble % 7,
            _ => {}
        }
        self.unix_seconds = calendar.to_unix_seconds();
        self.host_reference = SystemTime::now();
    }

    fn hour_ones(&self, calendar: CalendarTime) -> u8 {
        if (self.control_f & CF_24H) != 0 {
            calendar.hour % 10
        } else {
            let hour12 = hour_to_12h(calendar.hour);
            hour12 % 10
        }
    }

    fn hour_tens(&self, calendar: CalendarTime) -> u8 {
        if (self.control_f & CF_24H) != 0 {
            calendar.hour / 10
        } else {
            let hour12 = hour_to_12h(calendar.hour);
            let mut tens = hour12 / 10;
            if calendar.hour >= 12 {
                tens |= HOUR10_PM;
            }
            tens
        }
    }

    fn decode_hour(&self, tens: u8, ones: u8, fallback: u8) -> u8 {
        if (self.control_f & CF_24H) != 0 {
            let value = ((tens & HOUR10_MASK) * 10) + ones.min(9);
            return value.min(23);
        }
        let hour12 = ((tens & HOUR10_MASK) * 10) + ones.min(9);
        if hour12 == 0 || hour12 > 12 {
            return fallback;
        }
        let pm = (tens & HOUR10_PM) != 0;
        match (pm, hour12) {
            (false, 12) => 0,
            (true, 12) => 12,
            (false, h) => h,
            (true, h) => h + 12,
        }
    }
}

fn reg_index(addr24: u32) -> usize {
    ((addr24 - RTC_BASE) >> 2) as usize & 0x0F
}

fn expand_two_digit_year(year: u8) -> i32 {
    if year <= 69 {
        2000 + i32::from(year)
    } else {
        1900 + i32::from(year)
    }
}

fn system_time_to_unix_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(error) => {
            let duration = error.duration();
            -(i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        }
    }
}

fn hour_to_12h(hour24: u8) -> u8 {
    match hour24 {
        0 => 12,
        1..=12 => hour24,
        _ => hour24 - 12,
    }
}

impl CalendarTime {
    fn from_unix_seconds(unix_seconds: i64) -> Self {
        let days = unix_seconds.div_euclid(86_400);
        let seconds_of_day = unix_seconds.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let hour = u8::try_from(seconds_of_day / 3_600).unwrap_or(0);
        let minute = u8::try_from((seconds_of_day % 3_600) / 60).unwrap_or(0);
        let second = u8::try_from(seconds_of_day % 60).unwrap_or(0);
        let weekday = u8::try_from((days + 4).rem_euclid(7)).unwrap_or(0);
        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            weekday,
        }
    }

    fn to_unix_seconds(self) -> i64 {
        let days = days_from_civil(self.year, self.month, self.day);
        days * 86_400
            + i64::from(self.hour) * 3_600
            + i64::from(self.minute) * 60
            + i64::from(self.second)
    }
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u8, u8) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = i32::try_from(yoe + era * 400).unwrap_or(1970);
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u8::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month_i64 = mp + if mp < 10 { 3 } else { -9 };
    let month = u8::try_from(month_i64).unwrap_or(1);
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let year = i64::from(year) - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::{
        CalendarTime, Msm6242Rtc, REG_DAY_1, REG_DAY_10, REG_HOUR_1, REG_HOUR_10, REG_MINUTE_1,
        REG_MINUTE_10, REG_MONTH_1, REG_MONTH_10, REG_SECOND_1, REG_SECOND_10, REG_WEEKDAY,
        REG_YEAR_1, REG_YEAR_10, RTC_BASE,
    };

    #[test]
    fn calendar_round_trip_preserves_date_time() {
        let calendar = CalendarTime {
            year: 2026,
            month: 4,
            day: 22,
            hour: 19,
            minute: 51,
            second: 57,
            weekday: 3,
        };
        let round_trip = CalendarTime::from_unix_seconds(calendar.to_unix_seconds());
        assert_eq!(round_trip.year, 2026);
        assert_eq!(round_trip.month, 4);
        assert_eq!(round_trip.day, 22);
        assert_eq!(round_trip.hour, 19);
        assert_eq!(round_trip.minute, 51);
        assert_eq!(round_trip.second, 57);
    }

    #[test]
    fn reads_expected_nibbles_for_fixed_time() {
        let unix_seconds = CalendarTime {
            year: 2026,
            month: 4,
            day: 22,
            hour: 19,
            minute: 51,
            second: 57,
            weekday: 3,
        }
        .to_unix_seconds();
        let rtc = Msm6242Rtc::with_unix_seconds_for_test(unix_seconds);
        let regs = [
            (REG_SECOND_1, 7),
            (REG_SECOND_10, 5),
            (REG_MINUTE_1, 1),
            (REG_MINUTE_10, 5),
            (REG_HOUR_1, 9),
            (REG_HOUR_10, 1),
            (REG_DAY_1, 2),
            (REG_DAY_10, 2),
            (REG_MONTH_1, 4),
            (REG_MONTH_10, 0),
            (REG_YEAR_1, 6),
            (REG_YEAR_10, 2),
            (REG_WEEKDAY, 3),
        ];
        for (reg, expected) in regs {
            let addr = RTC_BASE + (reg as u32) * 4;
            assert_eq!(rtc.read_byte(addr), expected);
            assert_eq!(rtc.read_word(addr), u16::from(expected) * 0x0101);
        }
    }
}
