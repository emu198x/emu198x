//! Motorola MC6850 Asynchronous Communications Interface Adapter.
//!
//! The 6850 is a serial port with one register pair on each side of its
//! register-select line: control (write) and status (read) at RS=0, transmit
//! data (write) and receive data (read) at RS=1. It frames bytes with a start
//! bit, seven or eight data bits, optional parity and one or two stop bits,
//! clocked by an external clock divided by 1, 16 or 64.
//!
//! The chip is clocked by its host. One [`Acia6850::tick`] is one edge of the
//! transmit/receive clock input; on the Atari ST both ACIAs take a 500 kHz
//! clock, the keyboard one dividing by 64 (7812.5 baud) and the MIDI one by 16
//! (31250 baud). Bytes from the other end of the line arrive through
//! [`Acia6850::receive`] and reach the receive data register once their frame
//! has been clocked in; bytes the chip sends come out of
//! [`Acia6850::take_transmitted`] once their frame has been clocked out.
//!
//! # Registers
//!
//! **Status** (RS=0, read):
//!
//! | Bit | Name | Meaning |
//! |---|---|---|
//! | 0 | RDRF | Receive data register full |
//! | 1 | TDRE | Transmit data register empty |
//! | 2 | DCD | Data carrier detect input (high = lost) |
//! | 3 | CTS | Clear-to-send input (high = not clear) |
//! | 4 | FE | Framing error |
//! | 5 | OVRN | Receiver overrun |
//! | 6 | PE | Parity error |
//! | 7 | IRQ | The interrupt request output, active |
//!
//! **Control** (RS=0, write): bits 1–0 select the clock divider (÷1, ÷16, ÷64,
//! or master reset); bits 4–2 the word format; bits 6–5 the transmit control
//! (RTS and the transmit interrupt enable); bit 7 the receive interrupt
//! enable.
//!
//! The interrupt request is asserted while the receive interrupt is enabled
//! and the receive register is full or has overrun, or while the transmit
//! interrupt is enabled and the transmit register is empty.
//!
//! # Not modelled
//!
//! Parity and framing are never in error: bytes arrive as whole values from a
//! host, not as a sampled bit stream. Break transmission is accepted as a
//! control value and sends nothing.
//!
//! # Reference
//!
//! The MC6850 data sheet; facts cross-checked against Hatari's `acia.c`
//! (GPL-2, not used as a code source).

#![forbid(unsafe_code)]

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

pub const STATUS_RDRF: u8 = 0x01;
pub const STATUS_TDRE: u8 = 0x02;
pub const STATUS_DCD: u8 = 0x04;
pub const STATUS_CTS: u8 = 0x08;
pub const STATUS_OVRN: u8 = 0x20;
pub const STATUS_IRQ: u8 = 0x80;

/// A frame in flight: the byte and the bit-clock periods left.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Frame {
    byte: u8,
    remaining: u32,
}

/// The MC6850.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Acia6850 {
    control: u8,
    /// True from power-on or a master-reset write until the next control
    /// write selects a divider.
    in_reset: bool,
    rdr: u8,
    rdrf: bool,
    overrun: bool,
    tdr: Option<u8>,
    tx_shift: Option<Frame>,
    rx_shift: Option<Frame>,
    rx_queue: VecDeque<u8>,
    transmitted: Vec<u8>,
    clock_count: u32,
    dcd: bool,
    cts: bool,
}

impl Default for Acia6850 {
    fn default() -> Self {
        Self::new()
    }
}

impl Acia6850 {
    /// A chip at power-on, held in master reset until software writes the
    /// control register.
    #[must_use]
    pub fn new() -> Self {
        Self {
            control: 0x03,
            in_reset: true,
            rdr: 0,
            rdrf: false,
            overrun: false,
            tdr: None,
            tx_shift: None,
            rx_shift: None,
            rx_queue: VecDeque::new(),
            transmitted: Vec::new(),
            clock_count: 0,
            dcd: false,
            cts: false,
        }
    }

    /// Read the status (`rs` false) or receive data (`rs` true) register.
    /// Reading the receive data register empties it and clears overrun.
    pub fn read(&mut self, rs: bool) -> u8 {
        if rs {
            self.rdrf = false;
            self.overrun = false;
            self.rdr
        } else {
            self.status()
        }
    }

    /// The status register, without side effects.
    #[must_use]
    pub fn status(&self) -> u8 {
        let mut status = 0;
        if self.rdrf {
            status |= STATUS_RDRF;
        }
        if self.tdr.is_none() && !self.in_reset {
            status |= STATUS_TDRE;
        }
        if self.dcd {
            status |= STATUS_DCD;
        }
        if self.cts {
            status |= STATUS_CTS;
        }
        if self.overrun {
            status |= STATUS_OVRN;
        }
        if self.irq() {
            status |= STATUS_IRQ;
        }
        status
    }

    /// Write the control (`rs` false) or transmit data (`rs` true) register.
    pub fn write(&mut self, rs: bool, value: u8) {
        if rs {
            if !self.in_reset {
                self.tdr = Some(value);
            }
            return;
        }
        self.control = value;
        if value & 0x03 == 0x03 {
            self.master_reset();
        } else {
            self.in_reset = false;
        }
    }

    fn master_reset(&mut self) {
        self.in_reset = true;
        self.rdrf = false;
        self.overrun = false;
        self.tdr = None;
        self.tx_shift = None;
        self.rx_shift = None;
        self.clock_count = 0;
    }

    /// The interrupt request output, active when true.
    #[must_use]
    pub fn irq(&self) -> bool {
        if self.in_reset {
            return false;
        }
        let rie = self.control & 0x80 != 0;
        let tie = self.control & 0x60 == 0x20;
        (rie && (self.rdrf || self.overrun)) || (tie && self.tdr.is_none())
    }

    /// The RTS output, active (low on the pin) when true.
    #[must_use]
    pub fn rts(&self) -> bool {
        self.control & 0x60 != 0x40
    }

    /// Drive the DCD input; true means carrier lost.
    pub fn set_dcd(&mut self, lost: bool) {
        self.dcd = lost;
    }

    /// Drive the CTS input; true means not clear to send.
    pub fn set_cts(&mut self, not_clear: bool) {
        self.cts = not_clear;
    }

    /// A byte arrives on the receive line.
    pub fn receive(&mut self, byte: u8) {
        self.rx_queue.push_back(byte);
    }

    /// True while a byte is still arriving or queued to arrive.
    #[must_use]
    pub fn receiving(&self) -> bool {
        self.rx_shift.is_some() || !self.rx_queue.is_empty()
    }

    /// Bytes sent since the last call.
    pub fn take_transmitted(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.transmitted)
    }

    fn divider(&self) -> u32 {
        match self.control & 0x03 {
            0 => 1,
            1 => 16,
            _ => 64,
        }
    }

    fn frame_bits(&self) -> u32 {
        let format = self.control >> 2 & 7;
        let data = if format & 4 != 0 { 8 } else { 7 };
        let (parity, stop) = match format {
            0 | 1 => (1, 2),
            2 | 3 => (1, 1),
            4 => (0, 2),
            5 => (0, 1),
            _ => (1, 1),
        };
        1 + data + parity + stop
    }

    fn word_mask(&self) -> u8 {
        if self.control & 0x10 != 0 { 0xFF } else { 0x7F }
    }

    /// One edge of the transmit/receive clock input.
    pub fn tick(&mut self) {
        if self.in_reset {
            return;
        }
        self.clock_count += 1;
        if self.clock_count < self.divider() {
            return;
        }
        self.clock_count = 0;

        if self.tx_shift.is_none()
            && let Some(byte) = self.tdr.take()
        {
            self.tx_shift = Some(Frame {
                byte,
                remaining: self.frame_bits(),
            });
        }
        let mask = self.word_mask();
        if let Some(frame) = &mut self.tx_shift {
            frame.remaining -= 1;
            if frame.remaining == 0 {
                self.transmitted.push(frame.byte & mask);
                self.tx_shift = None;
            }
        }

        if self.rx_shift.is_none()
            && let Some(byte) = self.rx_queue.pop_front()
        {
            self.rx_shift = Some(Frame {
                byte,
                remaining: self.frame_bits(),
            });
        }
        if let Some(frame) = &mut self.rx_shift {
            frame.remaining -= 1;
            if frame.remaining == 0 {
                let byte = frame.byte & mask;
                self.rx_shift = None;
                if self.rdrf {
                    self.overrun = true;
                } else {
                    self.rdr = byte;
                    self.rdrf = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 8N1, ÷16, receive interrupts on.
    fn ready() -> Acia6850 {
        let mut acia = Acia6850::new();
        acia.write(false, 0x80 | 0x14 | 0x01);
        acia
    }

    #[test]
    fn power_on_holds_master_reset_until_a_control_write() {
        let mut acia = Acia6850::new();
        assert_eq!(acia.status() & STATUS_TDRE, 0);
        acia.write(true, 0x55);
        acia.write(false, 0x15);
        assert_ne!(
            acia.status() & STATUS_TDRE,
            0,
            "the write in reset was dropped"
        );
    }

    #[test]
    fn a_received_byte_lands_after_its_frame() {
        let mut acia = ready();
        acia.receive(0xF0);
        assert!(acia.receiving());
        // Ten bits at ÷16.
        for _ in 0..159 {
            acia.tick();
        }
        assert_eq!(acia.status() & STATUS_RDRF, 0);
        acia.tick();
        assert_ne!(acia.status() & STATUS_RDRF, 0);
        assert!(acia.irq());
        assert_ne!(acia.status() & STATUS_IRQ, 0);
        assert_eq!(acia.read(true), 0xF0);
        assert!(!acia.irq());
    }

    #[test]
    fn a_second_byte_unread_overruns() {
        let mut acia = ready();
        acia.receive(1);
        acia.receive(2);
        for _ in 0..320 {
            acia.tick();
        }
        assert_ne!(acia.status() & STATUS_OVRN, 0);
        assert!(acia.irq());
        assert_eq!(acia.read(true), 1, "the first byte survives");
        assert_eq!(acia.status() & STATUS_OVRN, 0);
    }

    #[test]
    fn transmit_frames_and_raises_tdre_with_tie() {
        let mut acia = Acia6850::new();
        acia.write(false, 0x20 | 0x14 | 0x01); // TIE, 8N1, ÷16
        assert!(acia.irq(), "empty with TIE");
        acia.write(true, 0x80);
        assert!(!acia.irq());
        acia.tick();
        acia.tick(); // the 16th edge has not come yet
        for _ in 0..14 {
            acia.tick();
        }
        assert!(acia.irq(), "TDR moved into the shift register");
        for _ in 0..16 * 9 {
            acia.tick();
        }
        assert_eq!(acia.take_transmitted(), vec![0x80]);
    }

    #[test]
    fn seven_bit_formats_mask_and_add_parity_bits() {
        let mut acia = Acia6850::new();
        acia.write(false, 0x08); // 7E1, ÷1: 10 bits
        acia.write(true, 0xFF);
        for _ in 0..10 {
            acia.tick();
        }
        assert_eq!(acia.take_transmitted(), vec![0x7F]);
        acia.write(false, 0x10); // 8N2, ÷1: 11 bits
        acia.write(true, 0xAA);
        for _ in 0..10 {
            acia.tick();
        }
        assert!(acia.take_transmitted().is_empty());
        acia.tick();
        assert_eq!(acia.take_transmitted(), vec![0xAA]);
    }

    #[test]
    fn divide_by_64_is_the_st_keyboard_rate() {
        let mut acia = Acia6850::new();
        acia.write(false, 0x96); // RIE, 8N1, ÷64
        acia.receive(0x42);
        for _ in 0..64 * 10 - 1 {
            acia.tick();
        }
        assert!(!acia.irq());
        acia.tick();
        assert!(acia.irq());
    }

    #[test]
    fn master_reset_drops_everything_and_inputs_show_in_status() {
        let mut acia = ready();
        acia.receive(9);
        for _ in 0..160 {
            acia.tick();
        }
        acia.write(false, 0x03);
        assert_eq!(acia.status(), 0);
        acia.set_dcd(true);
        acia.set_cts(true);
        assert_eq!(acia.status(), STATUS_DCD | STATUS_CTS);
        acia.write(false, 0x40 | 0x15);
        assert!(!acia.rts(), "transmit control 10 raises RTS");
        assert_eq!(acia.read(false) & STATUS_TDRE, STATUS_TDRE);
    }

    #[test]
    fn state_round_trips_through_serde() {
        let mut acia = ready();
        acia.receive(7);
        acia.tick();
        let json = serde_json::to_string(&acia).expect("serde round trip");
        let mut back: Acia6850 = serde_json::from_str(&json).expect("serde round trip");
        for _ in 0..160 {
            back.tick();
        }
        assert_eq!(back.read(true), 7);
    }
}
