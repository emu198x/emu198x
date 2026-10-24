//! Intel 8255 Programmable Peripheral Interface (PPI).
//!
//! Three 8-bit ports, A, B and C, and a control register that sets each
//! port's direction or sets and resets single port C bits. Port C splits
//! into two nibbles with their own directions.
//!
//! The chip knows nothing about the machine around it. The host writes
//! the levels other devices drive onto input lines into
//! [`Ppi8255::input_a`], [`Ppi8255::input_b`] and [`Ppi8255::input_c`],
//! and reads what the chip drives with [`Ppi8255::port_a`],
//! [`Ppi8255::port_b`] and [`Ppi8255::port_c`]. What an undriven input
//! line reads (pull-ups, pull-downs, a floating bus) is a fact about the
//! board, so the host decides it.
//!
//! Mode 0, simple input and output, is modelled. Every machine here
//! uses it. The handshake lines of Modes 1 and 2 (STB, IBF, OBF, ACK,
//! INTR, INTE) are not: in those modes the ports keep their programmed
//! directions, but port C carries no handshake or status bits.
//!
//! Source: Intel 8255A datasheet, summarised in the umbrella's
//! `reference/by-topic/ppi-8255/ppi-8255-reference.md`.

use serde::{Deserialize, Serialize};

/// The mode word RESET leaves: Mode 0, every port an input.
const RESET_MODE: u8 = 0x9B;

/// Intel 8255 PPI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ppi8255 {
    /// Port A output latch.
    output_a: u8,
    /// Port B output latch.
    output_b: u8,
    /// Port C output latch.
    output_c: u8,
    /// The last mode word.
    mode: u8,
    /// Levels other devices drive onto port A's lines. Only the bits the
    /// mode makes inputs are read.
    pub input_a: u8,
    /// Levels other devices drive onto port B's lines.
    pub input_b: u8,
    /// Levels other devices drive onto port C's lines.
    pub input_c: u8,
}

impl Ppi8255 {
    /// A PPI just out of RESET: every port an input and every output
    /// latch clear. The input lines start high, as if pulled up; a host
    /// whose board does otherwise sets them.
    #[must_use]
    pub fn new() -> Self {
        Self {
            output_a: 0,
            output_b: 0,
            output_c: 0,
            mode: RESET_MODE,
            input_a: 0xFF,
            input_b: 0xFF,
            input_c: 0xFF,
        }
    }

    /// Apply RESET. The input levels belong to the board and stay.
    pub fn reset(&mut self) {
        self.output_a = 0;
        self.output_b = 0;
        self.output_c = 0;
        self.mode = RESET_MODE;
    }

    /// The bits of port A that are inputs.
    fn input_mask_a(&self) -> u8 {
        if self.mode & 0x10 != 0 { 0xFF } else { 0x00 }
    }

    /// The bits of port B that are inputs.
    fn input_mask_b(&self) -> u8 {
        if self.mode & 0x02 != 0 { 0xFF } else { 0x00 }
    }

    /// The bits of port C that are inputs, nibble by nibble.
    fn input_mask_c(&self) -> u8 {
        let upper = if self.mode & 0x08 != 0 { 0xF0 } else { 0x00 };
        let lower = if self.mode & 0x01 != 0 { 0x0F } else { 0x00 };
        upper | lower
    }

    fn lines(output: u8, input: u8, input_mask: u8) -> u8 {
        (output & !input_mask) | (input & input_mask)
    }

    /// Port A's lines: the latch on output bits, the driven level on
    /// input bits.
    #[must_use]
    pub fn port_a(&self) -> u8 {
        Self::lines(self.output_a, self.input_a, self.input_mask_a())
    }

    /// Port B's lines.
    #[must_use]
    pub fn port_b(&self) -> u8 {
        Self::lines(self.output_b, self.input_b, self.input_mask_b())
    }

    /// Port C's lines.
    #[must_use]
    pub fn port_c(&self) -> u8 {
        Self::lines(self.output_c, self.input_c, self.input_mask_c())
    }

    /// Read a register; `register` bits 1-0 are A1-A0.
    ///
    /// Ports read their lines: an output bit returns its latch, an input
    /// bit the level driven onto it now, since Mode 0 inputs are not
    /// latched. The control register cannot be read; the datasheet
    /// leaves the bus undriven, which reads here as `$FF`.
    #[must_use]
    pub fn read(&self, register: u8) -> u8 {
        match register & 0x03 {
            0 => self.port_a(),
            1 => self.port_b(),
            2 => self.port_c(),
            _ => 0xFF,
        }
    }

    /// Write a register; `register` bits 1-0 are A1-A0.
    ///
    /// A port write loads its output latch, which drives the lines only
    /// where the mode makes them outputs. A control write with D7 set is
    /// a mode word, which clears every output latch; with D7 clear it
    /// sets (D0 = 1) or resets bit D3-D1 of port C's latch.
    pub fn write(&mut self, register: u8, value: u8) {
        match register & 0x03 {
            0 => self.output_a = value,
            1 => self.output_b = value,
            2 => self.output_c = value,
            _ => {
                if value & 0x80 != 0 {
                    self.mode = value;
                    self.output_a = 0;
                    self.output_b = 0;
                    self.output_c = 0;
                } else {
                    let bit = 1 << ((value >> 1) & 0x07);
                    if value & 0x01 != 0 {
                        self.output_c |= bit;
                    } else {
                        self.output_c &= !bit;
                    }
                }
            }
        }
    }

    /// The last mode word written, or `$9B` after RESET. The chip cannot
    /// return it to the CPU; this is for debuggers and tests.
    #[must_use]
    pub fn mode(&self) -> u8 {
        self.mode
    }
}

impl Default for Ppi8255 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_leaves_every_port_an_input() {
        let mut ppi = Ppi8255::new();
        assert_eq!(ppi.mode(), RESET_MODE);
        ppi.input_a = 0x12;
        ppi.input_b = 0x34;
        ppi.input_c = 0x56;
        ppi.write(0, 0xAA);
        ppi.write(1, 0xBB);
        ppi.write(2, 0xCC);
        assert_eq!([ppi.read(0), ppi.read(1), ppi.read(2)], [0x12, 0x34, 0x56]);
    }

    #[test]
    fn an_output_port_reads_back_its_latch() {
        let mut ppi = Ppi8255::new();
        ppi.write(3, 0x80); // Mode 0, every port an output
        ppi.input_a = 0x00;
        ppi.write(0, 0xAB);
        ppi.write(1, 0xCD);
        ppi.write(2, 0xEF);
        assert_eq!([ppi.read(0), ppi.read(1), ppi.read(2)], [0xAB, 0xCD, 0xEF]);
        assert_eq!(ppi.port_a(), 0xAB);
    }

    #[test]
    fn an_input_port_reads_its_lines_live() {
        let mut ppi = Ppi8255::new();
        ppi.write(3, 0x82); // A out, B in, C out
        ppi.input_b = 0x5A;
        assert_eq!(ppi.read(1), 0x5A);
        ppi.input_b = 0xA5;
        assert_eq!(ppi.read(1), 0xA5, "Mode 0 inputs are not latched");
    }

    #[test]
    fn writing_an_input_port_drives_nothing_until_it_becomes_an_output() {
        let mut ppi = Ppi8255::new();
        ppi.write(3, 0x82); // B is an input
        ppi.input_b = 0xFF;
        ppi.write(1, 0x3C);
        assert_eq!(ppi.port_b(), 0xFF);
    }

    #[test]
    fn port_c_nibbles_take_their_own_directions() {
        let mut ppi = Ppi8255::new();
        ppi.write(3, 0x88); // C upper in, C lower out
        ppi.input_c = 0xA5;
        ppi.write(2, 0x3C);
        assert_eq!(ppi.read(2), 0xAC);
        ppi.write(3, 0x81); // C upper out, C lower in
        ppi.write(2, 0x3C);
        assert_eq!(ppi.read(2), 0x35);
    }

    #[test]
    fn a_mode_word_clears_every_output_latch() {
        let mut ppi = Ppi8255::new();
        ppi.write(3, 0x80);
        ppi.write(0, 0xFF);
        ppi.write(1, 0xFF);
        ppi.write(2, 0xFF);
        ppi.write(3, 0x80);
        assert_eq!([ppi.read(0), ppi.read(1), ppi.read(2)], [0, 0, 0]);
    }

    #[test]
    fn bit_set_reset_changes_one_port_c_bit() {
        let mut ppi = Ppi8255::new();
        ppi.write(3, 0x80);
        ppi.write(3, 0x07); // set PC3
        assert_eq!(ppi.read(2), 0x08);
        ppi.write(3, 0x0F); // set PC7
        assert_eq!(ppi.read(2), 0x88);
        ppi.write(3, 0x06); // reset PC3
        assert_eq!(ppi.read(2), 0x80);
    }

    #[test]
    fn the_control_register_cannot_be_read() {
        let mut ppi = Ppi8255::new();
        ppi.write(3, 0x82);
        assert_eq!(ppi.read(3), 0xFF);
        assert_eq!(ppi.mode(), 0x82);
    }

    #[test]
    fn reset_keeps_the_boards_input_levels() {
        let mut ppi = Ppi8255::new();
        ppi.input_a = 0x00;
        ppi.write(3, 0x80);
        ppi.write(0, 0x77);
        ppi.reset();
        assert_eq!(ppi.mode(), RESET_MODE);
        assert_eq!(ppi.read(0), 0x00);
    }
}
