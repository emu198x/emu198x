//! Battery-backed RTC (MSM6242B-style old-address clock).
//!
//! AmigaOS 1.3-era utilities like `SetClock load` probe the "old
//! address" RTC directly at `$DC0000` when running on A500/A2000-class
//! machines with an expansion clock. The hardware exposes sixteen
//! 4-bit registers, each on a 32-bit boundary. To match the broad
//! access patterns Amiga code uses, the machine routes any byte/word
//! access within a 4-byte slot to the same nibble register.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
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

/// Source used to advance the RTC calendar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RtcClockMode {
    /// Advance only from explicit emulated system ticks.
    #[default]
    Emulated,
    /// Advance from elapsed host wall time while the RTC is running.
    HostSynchronized,
}

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

/// Side-effect-free snapshot of the Amiga RTC's emulated clock and controls.
///
/// The host-side time anchor is intentionally excluded. `effective_unix_seconds`
/// and the decoded calendar are sampled once so all returned fields describe
/// the same emulated instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Msm6242RtcDiagnosticSnapshot {
    /// Source currently advancing the RTC calendar.
    pub clock_mode: RtcClockMode,
    /// Persisted whole-second Unix timestamp.
    pub stored_unix_seconds: i64,
    /// Visible Unix timestamp after applying the selected clock source.
    pub effective_unix_seconds: i64,
    /// Decoded calendar year for the effective timestamp.
    pub year: i32,
    /// Decoded calendar month in the range 1..=12.
    pub month: u8,
    /// Decoded day of month in the range 1..=31.
    pub day: u8,
    /// Decoded 24-hour clock hour in the range 0..=23.
    pub hour: u8,
    /// Decoded minute in the range 0..=59.
    pub minute: u8,
    /// Decoded second in the range 0..=59.
    pub second: u8,
    /// Decoded weekday, where Sunday is zero.
    pub weekday: u8,
    /// Raw stored control-D nibble.
    pub control_d: u8,
    /// Raw stored control-E nibble.
    pub control_e: u8,
    /// Raw stored control-F nibble after RESET strobe handling.
    pub control_f: u8,
    /// Whether the RTC is currently advancing.
    pub running: bool,
    /// Whether control-D HOLD is set.
    pub hold: bool,
    /// Whether control-F STOP is set.
    pub stop: bool,
    /// Whether control-F selects 24-hour display.
    pub hour_mode_24: bool,
    /// Whether the control-D IRQ flag is set.
    pub irq_flag: bool,
    /// Whether the control-D BUSY flag is stored.
    pub busy: bool,
    /// Whether the control-F RESET strobe remains set.
    pub reset: bool,
    /// Retained integer system ticks within the current emulated second.
    pub subsecond_system_ticks: u64,
    /// System ticks in one second for the retained emulated phase.
    ///
    /// This is zero until the first emulated tick is supplied, and remains
    /// zero in host-synchronized mode.
    pub system_ticks_per_second: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Msm6242Rtc {
    unix_seconds: i64,
    clock_mode: RtcClockMode,
    subsecond_system_ticks: u64,
    system_ticks_per_second: u64,
    /// Host-side anchor for elapsed-time computation. Emulated mode keeps an
    /// inert epoch value so deserializing deterministic state does not read
    /// wall time.
    #[serde(skip)]
    host_reference: SystemTime,
    control_d: u8,
    control_e: u8,
    control_f: u8,
}

impl<'de> Deserialize<'de> for Msm6242Rtc {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct StoredRtc {
            unix_seconds: i64,
            clock_mode: RtcClockMode,
            subsecond_system_ticks: u64,
            system_ticks_per_second: u64,
            control_d: u8,
            control_e: u8,
            control_f: u8,
        }

        let stored = StoredRtc::deserialize(deserializer)?;
        if stored.system_ticks_per_second == 0 && stored.subsecond_system_ticks != 0 {
            return Err(D::Error::custom("RTC phase has no system-tick frequency"));
        }
        if stored.system_ticks_per_second != 0
            && stored.subsecond_system_ticks >= stored.system_ticks_per_second
        {
            return Err(D::Error::custom(
                "RTC phase exceeds its system-tick frequency",
            ));
        }
        if stored.clock_mode == RtcClockMode::HostSynchronized
            && (stored.subsecond_system_ticks != 0 || stored.system_ticks_per_second != 0)
        {
            return Err(D::Error::custom(
                "host-synchronized RTC retains an emulated phase",
            ));
        }

        let host_reference = match stored.clock_mode {
            RtcClockMode::Emulated => UNIX_EPOCH,
            RtcClockMode::HostSynchronized => SystemTime::now(),
        };
        Ok(Self {
            unix_seconds: stored.unix_seconds,
            clock_mode: stored.clock_mode,
            subsecond_system_ticks: stored.subsecond_system_ticks,
            system_ticks_per_second: stored.system_ticks_per_second,
            host_reference,
            control_d: stored.control_d,
            control_e: stored.control_e,
            control_f: stored.control_f,
        })
    }
}

impl Default for Msm6242Rtc {
    fn default() -> Self {
        Self::new()
    }
}

impl Msm6242Rtc {
    /// Create a deterministic RTC seeded once from host wall time.
    ///
    /// After construction, the calendar advances only when
    /// [`Self::advance_system_ticks`] is called. No emulated-mode operation
    /// reads wall time.
    #[must_use]
    pub fn new() -> Self {
        Self::with_unix_seconds(system_time_to_unix_seconds(SystemTime::now()))
    }

    /// Create a deterministic, running RTC at an exact Unix timestamp.
    ///
    /// The subsecond phase starts at zero. This constructor does not read
    /// host wall time.
    #[must_use]
    pub const fn with_unix_seconds(unix_seconds: i64) -> Self {
        Self {
            unix_seconds,
            clock_mode: RtcClockMode::Emulated,
            subsecond_system_ticks: 0,
            system_ticks_per_second: 0,
            host_reference: UNIX_EPOCH,
            control_d: CD_IRQ_FLAG,
            control_e: 0,
            control_f: CF_24H,
        }
    }

    /// Create an RTC whose running calendar follows elapsed host wall time.
    #[must_use]
    pub fn host_synchronized() -> Self {
        Self::host_synchronized_at(SystemTime::now())
    }

    /// Return the source currently advancing the RTC calendar.
    #[must_use]
    pub const fn clock_mode(&self) -> RtcClockMode {
        self.clock_mode
    }

    /// Change the source advancing the RTC calendar.
    ///
    /// The currently visible whole-second timestamp is retained. Switching
    /// modes starts a new subsecond phase because host wall-time phase is not
    /// part of deterministic machine state.
    pub fn set_clock_mode(&mut self, mode: RtcClockMode) {
        if self.clock_mode == mode {
            return;
        }
        self.set_clock_mode_at(mode, SystemTime::now());
    }

    /// Advance a deterministic RTC by emulated system ticks.
    ///
    /// Ticks are ignored while the RTC is host synchronized, held, or
    /// stopped. A rate change retains fractional progress using integer
    /// rescaling.
    ///
    /// # Panics
    ///
    /// Panics when `ticks_per_second` is zero.
    #[inline]
    pub fn advance_system_ticks(&mut self, ticks: u64, ticks_per_second: u64) {
        assert!(
            ticks_per_second != 0,
            "RTC system-tick frequency must be non-zero"
        );
        if self.clock_mode != RtcClockMode::Emulated || !self.running() || ticks == 0 {
            return;
        }

        self.rescale_subsecond_phase(ticks_per_second);
        let ticks_until_rollover = ticks_per_second - self.subsecond_system_ticks;
        if ticks < ticks_until_rollover {
            self.subsecond_system_ticks += ticks;
            return;
        }

        let ticks_after_rollover = ticks - ticks_until_rollover;
        let elapsed_seconds = 1 + ticks_after_rollover / ticks_per_second;
        self.subsecond_system_ticks = ticks_after_rollover % ticks_per_second;
        self.unix_seconds = saturating_add_unsigned_seconds(self.unix_seconds, elapsed_seconds);
    }

    /// Return serializable RTC state normalized to the currently visible
    /// timestamp.
    ///
    /// Emulated state is copied exactly, including its subsecond phase.
    /// Host-synchronized state captures its visible whole-second time and
    /// establishes a fresh host anchor in the returned state.
    #[must_use]
    pub fn snapshot_state(&self) -> Self {
        match self.clock_mode {
            RtcClockMode::Emulated => self.clone(),
            RtcClockMode::HostSynchronized => self.snapshot_state_at(SystemTime::now()),
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

    /// Return a side-effect-free diagnostic snapshot of the stored and
    /// effective clock value plus every implemented control state.
    #[must_use]
    pub fn diagnostic_snapshot(&self) -> Msm6242RtcDiagnosticSnapshot {
        let effective_unix_seconds = self.effective_unix_seconds();
        let calendar = CalendarTime::from_unix_seconds(effective_unix_seconds);
        Msm6242RtcDiagnosticSnapshot {
            clock_mode: self.clock_mode,
            stored_unix_seconds: self.unix_seconds,
            effective_unix_seconds,
            year: calendar.year,
            month: calendar.month,
            day: calendar.day,
            hour: calendar.hour,
            minute: calendar.minute,
            second: calendar.second,
            weekday: calendar.weekday,
            control_d: self.control_d,
            control_e: self.control_e,
            control_f: self.control_f,
            running: self.running(),
            hold: self.control_d & CD_HOLD != 0,
            stop: self.control_f & CF_STOP != 0,
            hour_mode_24: self.control_f & CF_24H != 0,
            irq_flag: self.control_d & CD_IRQ_FLAG != 0,
            busy: self.control_d & CD_BUSY != 0,
            reset: self.control_f & CF_RESET != 0,
            subsecond_system_ticks: self.subsecond_system_ticks,
            system_ticks_per_second: self.system_ticks_per_second,
        }
    }

    fn running(&self) -> bool {
        (self.control_d & CD_HOLD) == 0 && (self.control_f & CF_STOP) == 0
    }

    fn effective_unix_seconds(&self) -> i64 {
        if self.clock_mode == RtcClockMode::Emulated || !self.running() {
            return self.unix_seconds;
        }
        self.effective_unix_seconds_at(SystemTime::now())
    }

    fn effective_unix_seconds_at(&self, now: SystemTime) -> i64 {
        if self.clock_mode == RtcClockMode::Emulated || !self.running() {
            return self.unix_seconds;
        }
        let elapsed = match now.duration_since(self.host_reference) {
            Ok(duration) => duration,
            Err(_) => Duration::ZERO,
        };
        saturating_add_unsigned_seconds(self.unix_seconds, elapsed.as_secs())
    }

    fn sync_to_now(&mut self) {
        if self.clock_mode == RtcClockMode::HostSynchronized {
            let now = SystemTime::now();
            self.unix_seconds = self.effective_unix_seconds_at(now);
            self.host_reference = now;
        }
    }

    fn reanchor_host_clock(&mut self) {
        if self.clock_mode == RtcClockMode::HostSynchronized {
            self.host_reference = SystemTime::now();
        }
    }

    fn reset_subsecond_phase(&mut self) {
        self.subsecond_system_ticks = 0;
        self.system_ticks_per_second = 0;
    }

    fn rescale_subsecond_phase(&mut self, ticks_per_second: u64) {
        if self.system_ticks_per_second == ticks_per_second {
            return;
        }
        if self.system_ticks_per_second == 0 {
            self.subsecond_system_ticks = 0;
        } else {
            let phase = u128::from(self.subsecond_system_ticks) * u128::from(ticks_per_second)
                / u128::from(self.system_ticks_per_second);
            self.subsecond_system_ticks = phase as u64;
        }
        self.system_ticks_per_second = ticks_per_second;
    }

    fn host_synchronized_at(now: SystemTime) -> Self {
        Self {
            unix_seconds: system_time_to_unix_seconds(now),
            clock_mode: RtcClockMode::HostSynchronized,
            subsecond_system_ticks: 0,
            system_ticks_per_second: 0,
            host_reference: now,
            control_d: CD_IRQ_FLAG,
            control_e: 0,
            control_f: CF_24H,
        }
    }

    fn set_clock_mode_at(&mut self, mode: RtcClockMode, now: SystemTime) {
        if self.clock_mode == mode {
            return;
        }
        if self.clock_mode == RtcClockMode::HostSynchronized {
            self.unix_seconds = self.effective_unix_seconds_at(now);
        }
        self.clock_mode = mode;
        self.reset_subsecond_phase();
        self.host_reference = match mode {
            RtcClockMode::Emulated => UNIX_EPOCH,
            RtcClockMode::HostSynchronized => now,
        };
    }

    fn snapshot_state_at(&self, now: SystemTime) -> Self {
        let mut snapshot = self.clone();
        snapshot.unix_seconds = self.effective_unix_seconds_at(now);
        snapshot.host_reference = now;
        snapshot
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
                self.reanchor_host_clock();
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
                self.reset_subsecond_phase();
                self.control_f &= !CF_RESET;
            }
            if !was_running && self.running() {
                self.reanchor_host_clock();
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
        self.reset_subsecond_phase();
        self.reanchor_host_clock();
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

fn saturating_add_unsigned_seconds(timestamp: i64, elapsed_seconds: u64) -> i64 {
    let sum = i128::from(timestamp) + i128::from(elapsed_seconds);
    sum.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
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
        CD_HOLD, CF_24H, CF_STOP, CalendarTime, Msm6242Rtc, REG_CD, REG_CE, REG_CF, REG_DAY_1,
        REG_DAY_10, REG_HOUR_1, REG_HOUR_10, REG_MINUTE_1, REG_MINUTE_10, REG_MONTH_1,
        REG_MONTH_10, REG_SECOND_1, REG_SECOND_10, REG_WEEKDAY, REG_YEAR_1, REG_YEAR_10, RTC_BASE,
        RtcClockMode,
    };
    use std::time::{Duration, UNIX_EPOCH};

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
        let rtc = Msm6242Rtc::with_unix_seconds(unix_seconds);
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

    #[test]
    fn diagnostic_snapshot_reports_stored_effective_calendar_and_controls() {
        let calendar = CalendarTime {
            year: 2026,
            month: 4,
            day: 22,
            hour: 19,
            minute: 51,
            second: 57,
            weekday: 3,
        };
        let unix_seconds = calendar.to_unix_seconds();
        let mut rtc = Msm6242Rtc::with_unix_seconds(unix_seconds);
        rtc.write_byte(RTC_BASE + (REG_CD as u32) * 4, CD_HOLD);
        rtc.write_byte(RTC_BASE + (REG_CE as u32) * 4, 0x0B);

        let snapshot = rtc.diagnostic_snapshot();
        assert_eq!(snapshot.clock_mode, RtcClockMode::Emulated);
        assert_eq!(snapshot.stored_unix_seconds, unix_seconds);
        assert_eq!(snapshot.effective_unix_seconds, unix_seconds);
        assert_eq!(snapshot.year, 2026);
        assert_eq!(snapshot.month, 4);
        assert_eq!(snapshot.day, 22);
        assert_eq!(snapshot.hour, 19);
        assert_eq!(snapshot.minute, 51);
        assert_eq!(snapshot.second, 57);
        assert_eq!(snapshot.weekday, 3);
        assert_eq!(snapshot.control_d, CD_HOLD | 0x04);
        assert_eq!(snapshot.control_e, 0x0B);
        assert_eq!(snapshot.control_f, CF_24H);
        assert!(!snapshot.running);
        assert!(snapshot.hold);
        assert!(!snapshot.stop);
        assert!(snapshot.hour_mode_24);
        assert!(snapshot.irq_flag);
        assert!(!snapshot.busy);
        assert!(!snapshot.reset);
        assert_eq!(snapshot.subsecond_system_ticks, 0);
        assert_eq!(snapshot.system_ticks_per_second, 0);

        rtc.write_byte(RTC_BASE + (REG_CF as u32) * 4, CF_STOP);
        let stopped = rtc.diagnostic_snapshot();
        assert!(!stopped.running);
        assert!(stopped.hold);
        assert!(stopped.stop);
        assert!(!stopped.hour_mode_24);

        rtc.write_byte(RTC_BASE + (REG_CD as u32) * 4, 0);
        rtc.write_byte(RTC_BASE + (REG_CF as u32) * 4, CF_24H);
        let running = rtc.diagnostic_snapshot();
        assert!(running.running);
        assert!(!running.hold);
        assert!(!running.stop);
        assert!(running.hour_mode_24);
        assert_eq!(running.effective_unix_seconds, running.stored_unix_seconds);
    }

    #[test]
    fn fixed_seed_is_stable_until_emulated_ticks_advance() {
        let rtc = Msm6242Rtc::with_unix_seconds(1_782_844_317);

        let first = rtc.diagnostic_snapshot();
        let second = rtc.diagnostic_snapshot();
        assert_eq!(first, second);
        assert_eq!(first.clock_mode, RtcClockMode::Emulated);
        assert_eq!(first.stored_unix_seconds, 1_782_844_317);
        assert_eq!(first.effective_unix_seconds, 1_782_844_317);
        assert!(first.running);
        assert_eq!(first.subsecond_system_ticks, 0);
        assert_eq!(first.system_ticks_per_second, 0);
    }

    #[test]
    fn emulated_ticks_roll_over_at_the_exact_calendar_boundary() {
        let before_midnight = CalendarTime {
            year: 2024,
            month: 2,
            day: 29,
            hour: 23,
            minute: 59,
            second: 59,
            weekday: 4,
        }
        .to_unix_seconds();
        let mut rtc = Msm6242Rtc::with_unix_seconds(before_midnight);

        rtc.advance_system_ticks(3, 4);
        let before = rtc.diagnostic_snapshot();
        assert_eq!(before.effective_unix_seconds, before_midnight);
        assert_eq!(before.subsecond_system_ticks, 3);
        assert_eq!(before.system_ticks_per_second, 4);

        rtc.advance_system_ticks(1, 4);
        let after = rtc.diagnostic_snapshot();
        assert_eq!(after.effective_unix_seconds, before_midnight + 1);
        assert_eq!((after.year, after.month, after.day), (2024, 3, 1));
        assert_eq!((after.hour, after.minute, after.second), (0, 0, 0));
        assert_eq!(after.subsecond_system_ticks, 0);

        rtc.advance_system_ticks(9, 4);
        let batch = rtc.diagnostic_snapshot();
        assert_eq!(batch.effective_unix_seconds, before_midnight + 3);
        assert_eq!(batch.second, 2);
        assert_eq!(batch.subsecond_system_ticks, 1);
    }

    #[test]
    fn hold_and_stop_pause_both_seconds_and_subsecond_phase() {
        let mut rtc = Msm6242Rtc::with_unix_seconds(10_000);
        rtc.advance_system_ticks(3, 4);

        rtc.write_byte(RTC_BASE + (REG_CD as u32) * 4, CD_HOLD);
        rtc.advance_system_ticks(9, 4);
        let held = rtc.diagnostic_snapshot();
        assert_eq!(held.effective_unix_seconds, 10_000);
        assert_eq!(held.subsecond_system_ticks, 3);

        rtc.write_byte(RTC_BASE + (REG_CD as u32) * 4, 0);
        rtc.advance_system_ticks(1, 4);
        let resumed = rtc.diagnostic_snapshot();
        assert_eq!(resumed.effective_unix_seconds, 10_001);
        assert_eq!(resumed.subsecond_system_ticks, 0);

        rtc.write_byte(RTC_BASE + (REG_CF as u32) * 4, CF_24H | CF_STOP);
        rtc.advance_system_ticks(8, 4);
        let stopped = rtc.diagnostic_snapshot();
        assert_eq!(stopped.effective_unix_seconds, 10_001);
        assert_eq!(stopped.subsecond_system_ticks, 0);

        rtc.write_byte(RTC_BASE + (REG_CF as u32) * 4, CF_24H);
        rtc.advance_system_ticks(4, 4);
        assert_eq!(rtc.diagnostic_snapshot().effective_unix_seconds, 10_002);
    }

    #[test]
    fn emulated_snapshot_round_trip_preserves_exact_phase() {
        let mut rtc = Msm6242Rtc::with_unix_seconds(20_000);
        rtc.advance_system_ticks(13, 5);

        let snapshot = rtc.snapshot_state();
        let encoded = postcard::to_allocvec(&snapshot).expect("serialize deterministic RTC");
        let restored: Msm6242Rtc =
            postcard::from_bytes(&encoded).expect("deserialize deterministic RTC");

        assert_eq!(restored.diagnostic_snapshot(), rtc.diagnostic_snapshot());
        assert_eq!(restored.clock_mode(), RtcClockMode::Emulated);

        let mut original = rtc;
        let mut replay = restored;
        original.advance_system_ticks(2, 5);
        replay.advance_system_ticks(2, 5);
        assert_eq!(replay.diagnostic_snapshot(), original.diagnostic_snapshot());
    }

    #[test]
    fn host_snapshot_normalizes_visible_time_at_capture() {
        let anchor = UNIX_EPOCH + Duration::from_secs(100_000);
        let capture = anchor + Duration::from_secs(17);
        let rtc = Msm6242Rtc::host_synchronized_at(anchor);

        let snapshot = rtc.snapshot_state_at(capture);
        assert_eq!(snapshot.clock_mode, RtcClockMode::HostSynchronized);
        assert_eq!(snapshot.unix_seconds, 100_017);
        assert_eq!(snapshot.host_reference, capture);
        assert_eq!(snapshot.subsecond_system_ticks, 0);
        assert_eq!(snapshot.system_ticks_per_second, 0);

        let encoded = postcard::to_allocvec(&snapshot).expect("serialize host RTC");
        let restored: Msm6242Rtc = postcard::from_bytes(&encoded).expect("deserialize host RTC");
        assert_eq!(restored.clock_mode(), RtcClockMode::HostSynchronized);
        assert_eq!(restored.unix_seconds, 100_017);
    }

    #[test]
    fn mode_switch_retains_visible_seconds_and_resets_phase() {
        let host_anchor = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let mut rtc = Msm6242Rtc::with_unix_seconds(30_000);
        rtc.advance_system_ticks(3, 4);

        rtc.set_clock_mode_at(RtcClockMode::HostSynchronized, host_anchor);
        assert_eq!(rtc.clock_mode(), RtcClockMode::HostSynchronized);
        assert_eq!(rtc.effective_unix_seconds_at(host_anchor), 30_000);
        assert_eq!(rtc.subsecond_system_ticks, 0);
        assert_eq!(rtc.system_ticks_per_second, 0);
        rtc.advance_system_ticks(40, 4);
        assert_eq!(
            rtc.effective_unix_seconds_at(host_anchor + Duration::from_secs(2)),
            30_002
        );

        rtc.set_clock_mode_at(RtcClockMode::Emulated, host_anchor + Duration::from_secs(2));
        assert_eq!(rtc.clock_mode(), RtcClockMode::Emulated);
        assert_eq!(rtc.effective_unix_seconds(), 30_002);
        assert_eq!(rtc.subsecond_system_ticks, 0);
        assert_eq!(rtc.system_ticks_per_second, 0);

        rtc.advance_system_ticks(4, 4);
        assert_eq!(rtc.effective_unix_seconds(), 30_003);
    }

    #[test]
    fn tick_rate_change_rescales_fractional_progress_exactly() {
        let mut rtc = Msm6242Rtc::with_unix_seconds(40_000);
        rtc.advance_system_ticks(1, 4);
        rtc.advance_system_ticks(1, 8);

        let snapshot = rtc.diagnostic_snapshot();
        assert_eq!(snapshot.effective_unix_seconds, 40_000);
        assert_eq!(snapshot.subsecond_system_ticks, 3);
        assert_eq!(snapshot.system_ticks_per_second, 8);
    }

    #[test]
    fn maximum_tick_batch_saturates_seconds_without_overflow() {
        let mut rtc = Msm6242Rtc::with_unix_seconds(0);

        rtc.advance_system_ticks(u64::MAX, 1);

        let snapshot = rtc.diagnostic_snapshot();
        assert_eq!(snapshot.effective_unix_seconds, i64::MAX);
        assert_eq!(snapshot.subsecond_system_ticks, 0);
        assert_eq!(snapshot.system_ticks_per_second, 1);
    }

    #[test]
    fn maximum_tick_batch_from_negative_epoch_uses_the_full_unsigned_range() {
        let mut rtc = Msm6242Rtc::with_unix_seconds(i64::MIN);

        rtc.advance_system_ticks(u64::MAX, 1);

        let snapshot = rtc.diagnostic_snapshot();
        assert_eq!(snapshot.effective_unix_seconds, i64::MAX);
        assert_eq!(snapshot.subsecond_system_ticks, 0);
        assert_eq!(snapshot.system_ticks_per_second, 1);
    }
}
