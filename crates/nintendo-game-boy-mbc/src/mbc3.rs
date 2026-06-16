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
//! The live registers advance over real wall-clock time, lazily: a
//! [`SystemTime`] anchor records when the live values were last current,
//! and any clock-observing operation (latch, register write) first folds the
//! elapsed seconds in. This mirrors the Amiga `Msm6242Rtc` host-clock pattern.
//! The anchor is `#[serde(skip)]` and re-anchors to "now" on snapshot restore;
//! cross-session time is carried by the `.sav` RTC footer instead.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ROM_BANK_SIZE: usize = 0x4000;
const RAM_BANK_SIZE: usize = 0x2000;

/// Length of the `.sav` RTC footer: five live registers + five latched, each a
/// little-endian `u32`, then an 8-byte little-endian last-save Unix timestamp.
/// This is the de-facto BGB/VBA layout, so saves stay portable.
pub const RTC_FOOTER_LEN: usize = 5 * 4 + 5 * 4 + 8;

/// `#[serde(skip)]` default for the host-clock anchor — see [`Mbc3`].
fn default_host_reference() -> SystemTime {
    SystemTime::now()
}

/// Current wall-clock time as whole Unix seconds (0 before the epoch).
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

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

// No `PartialEq`/`Eq`: the `host_reference` wall-clock anchor makes structural
// equality meaningless (two equal clocks compare unequal). Mirrors the Amiga
// `Msm6242Rtc`.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    /// Host-clock anchor: the instant the live registers were last brought
    /// current. Elapsed real seconds since this point are folded into the
    /// live registers by [`sync_live`](Self::sync_live). Re-anchored to "now"
    /// on snapshot restore — cross-session time rides the `.sav` footer.
    #[serde(skip, default = "default_host_reference")]
    host_reference: SystemTime,
}

impl Mbc3 {
    #[must_use]
    pub fn new(has_rtc: bool) -> Self {
        Self {
            ram_enabled: false,
            rom_bank: 1,
            ram_bank: 0,
            rtc: RtcRegisters::default(),
            rtc_latched: RtcRegisters::default(),
            latch_prev: 0xFF,
            has_rtc,
            host_reference: SystemTime::now(),
        }
    }

    /// Fold the real seconds elapsed since the host-clock anchor into the live
    /// registers, then move the anchor forward by exactly that many whole
    /// seconds (the sub-second remainder carries to the next sync). While the
    /// clock is halted the elapsed time is discarded — the counter is stopped —
    /// but the anchor still advances so unhalting does not jump.
    fn sync_live(&mut self) {
        if !self.has_rtc {
            return;
        }
        let elapsed = SystemTime::now()
            .duration_since(self.host_reference)
            .unwrap_or(Duration::ZERO);
        let whole = elapsed.as_secs();
        if whole == 0 {
            return;
        }
        self.advance_seconds(whole); // no-op while halted
        self.host_reference += Duration::from_secs(whole);
    }

    pub(crate) fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        let bank_count = rom.len() / ROM_BANK_SIZE;
        if bank_count == 0 {
            return 0xFF;
        }

        let bank = if addr < 0x4000 {
            0
        } else {
            usize::from(self.rom_bank.max(1))
        } % bank_count;
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
                    self.sync_live(); // fold in elapsed time before snapshotting
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
                let Some(offset) = self.ram_offset(ram, addr) else {
                    return 0xFF;
                };
                ram[offset]
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
        // Setting any RTC register (including the halt bit in $0C) first folds
        // in the time elapsed so far, so the write lands on a current clock.
        if self.has_rtc && (0x08..=0x0C).contains(&self.ram_bank) {
            self.sync_live();
        }
        match self.ram_bank {
            0x00..=0x03 => {
                if let Some(offset) = self.ram_offset(ram, addr) {
                    ram[offset] = value;
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

    /// Advance the live RTC by `elapsed` real seconds, carrying through
    /// minutes / hours / days and honouring the halt bit (`$0C` bit 6). The
    /// day counter is 9-bit (`day_low` + `$0C` bit 0); overflowing day 511
    /// latches the day-carry bit (`$0C` bit 7), which stays set until software
    /// clears it. Latched values are untouched — the game latches to read.
    ///
    /// Sub-counters are normalised mod 60 / 60 / 24; the chip's wrap-at-bit-
    /// width behaviour for software-loaded out-of-range values (seconds 60-63
    /// etc., as RTC3test exercises) is not modelled — every real game keeps
    /// the fields in range.
    pub fn advance_seconds(&mut self, elapsed: u64) {
        if !self.has_rtc || self.rtc.day_high_ctrl & 0x40 != 0 {
            return; // no RTC, or the clock is halted
        }
        let mut secs = u64::from(self.rtc.seconds & 0x3F) + elapsed;
        let mut mins = u64::from(self.rtc.minutes & 0x3F) + secs / 60;
        secs %= 60;
        let mut hours = u64::from(self.rtc.hours & 0x1F) + mins / 60;
        mins %= 60;
        let mut days =
            u64::from(self.rtc.day_low) | (u64::from(self.rtc.day_high_ctrl & 0x01) << 8);
        days += hours / 24;
        hours %= 24;
        if days > 0x1FF {
            self.rtc.day_high_ctrl |= 0x80; // day-counter carry, sticky
            days %= 0x200;
        }
        self.rtc.seconds = secs as u8;
        self.rtc.minutes = mins as u8;
        self.rtc.hours = hours as u8;
        self.rtc.day_low = (days & 0xFF) as u8;
        self.rtc.day_high_ctrl = (self.rtc.day_high_ctrl & !0x01) | ((days >> 8) & 0x01) as u8;
    }

    /// Encode the RTC into the `.sav` footer: live registers, latched
    /// registers, and the current wall-clock timestamp. The live registers are
    /// brought current first so the timestamp matches them.
    pub fn save_footer(&mut self) -> [u8; RTC_FOOTER_LEN] {
        self.sync_live();
        let mut out = [0u8; RTC_FOOTER_LEN];
        let regs = |r: &RtcRegisters| [r.seconds, r.minutes, r.hours, r.day_low, r.day_high_ctrl];
        let mut off = 0;
        for byte in regs(&self.rtc).into_iter().chain(regs(&self.rtc_latched)) {
            out[off..off + 4].copy_from_slice(&u32::from(byte).to_le_bytes());
            off += 4;
        }
        out[off..off + 8].copy_from_slice(&now_unix().to_le_bytes());
        self.host_reference = SystemTime::now();
        out
    }

    /// Restore the RTC from a `.sav` footer, then advance the live registers by
    /// the real time elapsed since the footer was written (so the clock keeps
    /// running while the emulator is closed). Honours the halt bit. A
    /// wrong-length or non-RTC footer is ignored.
    pub fn load_footer(&mut self, footer: &[u8]) {
        if !self.has_rtc || footer.len() != RTC_FOOTER_LEN {
            return;
        }
        let reg = |i: usize| footer[i * 4]; // low byte of each LE u32
        self.rtc = RtcRegisters {
            seconds: reg(0),
            minutes: reg(1),
            hours: reg(2),
            day_low: reg(3),
            day_high_ctrl: reg(4),
        };
        self.rtc_latched = RtcRegisters {
            seconds: reg(5),
            minutes: reg(6),
            hours: reg(7),
            day_low: reg(8),
            day_high_ctrl: reg(9),
        };
        let saved_ts = u64::from_le_bytes(footer[40..48].try_into().unwrap_or([0; 8]));
        self.host_reference = SystemTime::now();
        // Off-time: advance by the real seconds since the save was written.
        self.advance_seconds(now_unix().saturating_sub(saved_ts));
    }

    fn ram_offset(&self, ram: &[u8], addr: u16) -> Option<usize> {
        let bank_count = ram.len() / RAM_BANK_SIZE;
        if bank_count == 0 {
            return None;
        }

        let bank = usize::from(self.ram_bank & 0x03) % bank_count;
        let local = usize::from(addr.wrapping_sub(0xA000) & 0x1FFF);
        Some(bank * RAM_BANK_SIZE + local)
    }
}
