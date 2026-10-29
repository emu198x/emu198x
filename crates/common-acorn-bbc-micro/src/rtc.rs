//! The Master 128's HD146818 real-time clock and its 50 bytes of CMOS RAM.
//!
//! The MOS keeps its configuration here (`*CONFIGURE`): the language and
//! filing system to start, the screen mode, the boot option and the rest.
//! The chip hangs off the System VIA, the same way b-em's `cmos.c` wires it:
//!
//! - the slow data bus (System VIA port A) carries the data both ways;
//! - System VIA PB7 latches an address from the bus, and PB6 enables the chip;
//! - addressable-latch IC32 bit 1 is R/W (high to read) and bit 2 is the data
//!   strobe. A write lands on the strobe's rising edge with R/W low, and a read
//!   drives the bus while the strobe is high with R/W high.
//!
//! The clock counts emulated time, not the host's, so a run is repeatable. It
//! starts at midnight on Thursday 1 January 1987.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Master ticks (2 MHz) in one second of the clock.
const TICKS_PER_SECOND: u64 = 2_000_000;

/// Register B: binary rather than BCD (DM).
const REG_B_BINARY: u8 = 0x04;
/// Register B: 24-hour clock.
const REG_B_24_HOUR: u8 = 0x02;

/// The configuration the clock powers up with, from beebwiki's "CMOS
/// configuration RAM allocation" (as b-em's `cmos_defaults`), at bytes 14-30.
/// Two choices differ from b-em's: the filing system is the DFS in bank 9
/// rather than ADFS, so a DFS disc works without `*DISC`, and the Tube is off,
/// as none is fitted.
const USER_DEFAULTS: [u8; 17] = [
    0x00, // Econet station number
    0xFE, // file server station
    0x00, // file server network
    0xEB, // printer server station
    0x00, // printer server network
    0xC9, // language ROM 12 (BASIC), filing system ROM 9 (DFS)
    0xFF, // ROMs 0-7 inserted
    0xFF, // ROMs 8-15 inserted
    0x15, // EDIT: descriptive, insert
    0x00, // telecoms software
    0x17, // MODE 7, *TV 0,1
    0x48, // shift-caps lock, floppy drive, no directory
    0x32, // keyboard auto-repeat delay
    0x08, // keyboard auto-repeat rate
    0x0A, // printer ignore character
    0x2C, // no Tube, 9600 baud, parallel printer
    0x82, // no shadow, loud beep, data bits
];

/// An HD146818 on the Master's slow data bus.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Rtc {
    /// Clock registers 0-13, then the 50 bytes of user RAM.
    #[serde(with = "BigArray")]
    ram: [u8; 64],
    /// The register latched by the last PB7 pulse.
    address: u8,
    /// PB6: the chip is selected.
    enabled: bool,
    /// The data strobe as last seen, for its edges.
    strobe: bool,
    /// The byte the chip drives onto the bus during a read.
    output: u8,
    /// Master ticks into the current second.
    ticks: u64,
}

impl Rtc {
    pub(crate) fn new() -> Self {
        let mut ram = [0u8; 64];
        // 00:00:00, Thursday 1 January 1987, in BCD.
        ram[6] = 0x05; // day of week, Sunday = 1
        ram[7] = 0x01;
        ram[8] = 0x01;
        ram[9] = 0x87;
        ram[10] = 0x20; // register A: 32.768 kHz time base
        ram[11] = REG_B_24_HOUR;
        ram[13] = 0x80; // register D: the RAM and time are valid
        ram[14..14 + USER_DEFAULTS.len()].copy_from_slice(&USER_DEFAULTS);
        Self {
            ram,
            address: 0,
            enabled: false,
            strobe: false,
            output: 0xFF,
            ticks: 0,
        }
    }

    /// A write to System VIA port B: PB7 latches an address from the slow
    /// data bus `bus`, and PB6 selects the chip.
    pub(crate) fn write_port_b(&mut self, port_b: u8, bus: u8) {
        if port_b & 0x80 != 0 {
            self.address = bus & 0x3F;
        }
        self.enabled = port_b & 0x40 != 0;
    }

    /// The control lines from IC32 have changed, or the bus has: act on an
    /// edge of the data strobe.
    pub(crate) fn update(&mut self, read: bool, strobe: bool, bus: u8) {
        let edge = strobe != self.strobe;
        self.strobe = strobe;
        if !edge || !self.enabled {
            return;
        }
        if !read && !strobe {
            self.write_register(self.address, bus);
        }
        if read && strobe {
            self.output = self.read_register(self.address);
        }
    }

    /// What the chip drives onto the slow data bus: the byte being read while
    /// it is selected, the strobe is high and R/W reads; otherwise it lets the
    /// bus float high.
    pub(crate) fn bus(&self, read: bool) -> u8 {
        if self.enabled && self.strobe && read {
            self.output
        } else {
            0xFF
        }
    }

    /// Advance the clock by `ticks` master ticks.
    pub(crate) fn tick(&mut self, ticks: u64) {
        self.ticks += ticks;
        while self.ticks >= TICKS_PER_SECOND {
            self.ticks -= TICKS_PER_SECOND;
            self.advance_second();
        }
    }

    fn read_register(&self, register: u8) -> u8 {
        match register {
            // Register C clears on read on the real chip; nothing here sets
            // its flags, so it reads as zero.
            12 => 0,
            _ => self.ram[usize::from(register & 0x3F)],
        }
    }

    fn write_register(&mut self, register: u8, value: u8) {
        match register {
            // Register C and D are read-only.
            12 | 13 => {}
            // Register A's update-in-progress bit is read-only.
            10 => self.ram[10] = (self.ram[10] & 0x80) | (value & 0x7F),
            _ => self.ram[usize::from(register & 0x3F)] = value,
        }
    }

    /// A clock field as a number, from BCD unless register B says binary.
    fn decode_field(&self, value: u8) -> u8 {
        if self.ram[11] & REG_B_BINARY != 0 {
            value
        } else {
            (value >> 4) * 10 + (value & 0x0F)
        }
    }

    /// A number as a clock field, in BCD unless register B says binary.
    fn encode_field(&self, value: u8) -> u8 {
        if self.ram[11] & REG_B_BINARY != 0 {
            value
        } else {
            ((value / 10) << 4) | (value % 10)
        }
    }

    /// Count one second, carrying into minutes, hours, the day, the month and
    /// the year. Twelve-hour mode is not modelled; the MOS uses 24 hours.
    fn advance_second(&mut self) {
        if !self.bump(0, 60, 0) || !self.bump(2, 60, 0) || !self.bump(4, 24, 0) {
            return;
        }
        self.bump(6, 8, 1);
        let year = self.decode_field(self.ram[9]);
        let days = match self.decode_field(self.ram[8]) {
            2 if year.is_multiple_of(4) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        if self.bump(7, days + 1, 1) && self.bump(8, 13, 1) {
            self.bump(9, 100, 0);
        }
    }

    /// Add one to the field at `index`, wrapping to `first` at `limit`, and
    /// say whether it wrapped.
    fn bump(&mut self, index: usize, limit: u8, first: u8) -> bool {
        let next = self.decode_field(self.ram[index]) + 1;
        let wrapped = next >= limit;
        self.ram[index] = self.encode_field(if wrapped { first } else { next });
        wrapped
    }

    /// One byte of the clock's RAM, for inspection.
    #[cfg(test)]
    pub(crate) fn peek(&self, register: u8) -> u8 {
        self.ram[usize::from(register & 0x3F)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read a register the way the MOS does: latch the address with PB7,
    /// select with PB6, raise R/W and the strobe, and take the bus.
    fn read(rtc: &mut Rtc, register: u8) -> u8 {
        rtc.write_port_b(0xC0, register);
        rtc.update(true, false, 0xFF);
        rtc.update(true, true, 0xFF);
        let value = rtc.bus(true);
        rtc.update(true, false, 0xFF);
        value
    }

    fn write(rtc: &mut Rtc, register: u8, value: u8) {
        rtc.write_port_b(0xC0, register);
        rtc.update(false, true, value);
        rtc.update(false, false, value);
    }

    #[test]
    fn the_bus_protocol_reads_and_writes_the_ram() {
        let mut rtc = Rtc::new();
        assert_eq!(read(&mut rtc, 19), 0xC9, "language and filing system");
        write(&mut rtc, 40, 0x5A);
        assert_eq!(read(&mut rtc, 40), 0x5A);
        assert_eq!(rtc.bus(true), 0xFF, "the bus floats once the strobe falls");
    }

    #[test]
    fn a_deselected_chip_ignores_the_strobe() {
        let mut rtc = Rtc::new();
        rtc.write_port_b(0x80, 40);
        rtc.update(false, true, 0x11);
        rtc.update(false, false, 0x11);
        assert_eq!(rtc.peek(40), 0);
    }

    #[test]
    fn the_clock_counts_emulated_seconds_and_carries() {
        let mut rtc = Rtc::new();
        rtc.ram[0] = 0x59;
        rtc.ram[2] = 0x59;
        rtc.ram[4] = 0x23;
        rtc.ram[7] = 0x31;
        rtc.ram[8] = 0x12;
        rtc.tick(TICKS_PER_SECOND - 1);
        assert_eq!(rtc.peek(0), 0x59, "not yet a second");
        rtc.tick(1);
        assert_eq!(
            [
                rtc.peek(0),
                rtc.peek(2),
                rtc.peek(4),
                rtc.peek(7),
                rtc.peek(8),
                rtc.peek(9)
            ],
            [0x00, 0x00, 0x00, 0x01, 0x01, 0x88],
            "New Year 1988"
        );
    }
}
