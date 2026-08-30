//! Deterministic cycle-driven 8N1 adapter for VIC-20 user-port serial.

use std::collections::VecDeque;

/// A byte-stream boundary backed by physical 8N1 levels.
///
/// The computer transmits on CB2 and receives on PB0. Timing is expressed in
/// emulated CPU cycles so tests and recordings remain independent of host
/// scheduling.
#[derive(Debug)]
pub struct BitBangSerial {
    cycles_per_bit: u32,
    previous_tx: bool,
    receive_countdown: u32,
    receive_bit: u8,
    receive_byte: u8,
    received: VecDeque<u8>,
    transmit: VecDeque<u8>,
    transmit_byte: Option<u8>,
    transmit_bit: u8,
    transmit_countdown: u32,
}

impl BitBangSerial {
    #[must_use]
    pub fn new(cycles_per_bit: u32) -> Self {
        assert!(
            cycles_per_bit >= 2,
            "serial bit period must be at least two cycles"
        );
        Self {
            cycles_per_bit,
            previous_tx: true,
            receive_countdown: 0,
            receive_bit: 0,
            receive_byte: 0,
            received: VecDeque::new(),
            transmit: VecDeque::new(),
            transmit_byte: None,
            transmit_bit: 0,
            transmit_countdown: 0,
        }
    }

    pub fn queue_input(&mut self, bytes: &[u8]) {
        self.transmit.extend(bytes.iter().copied());
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        self.received.drain(..).collect()
    }

    /// Advance one emulated CPU cycle and return the PB0 level for that cycle.
    pub fn tick(&mut self, cb2: bool) -> bool {
        self.receive_tick(cb2);
        self.previous_tx = cb2;
        self.transmit_tick()
    }

    fn receive_tick(&mut self, level: bool) {
        if self.receive_countdown == 0 {
            if self.receive_bit == 0 {
                if self.previous_tx && !level {
                    // Sample data bit zero in the middle of its cell: one full
                    // start bit plus half a data bit from the falling edge.
                    self.receive_bit = 1;
                    self.receive_byte = 0;
                    self.receive_countdown = self.cycles_per_bit + self.cycles_per_bit / 2 - 1;
                }
                return;
            }

            if self.receive_bit <= 8 {
                if level {
                    self.receive_byte |= 1 << (self.receive_bit - 1);
                }
                self.receive_bit += 1;
                self.receive_countdown = self.cycles_per_bit - 1;
            } else {
                if level {
                    self.received.push_back(self.receive_byte);
                }
                self.receive_bit = 0;
            }
        } else {
            self.receive_countdown -= 1;
        }
    }

    fn transmit_tick(&mut self) -> bool {
        if self.transmit_byte.is_none() {
            self.transmit_byte = self.transmit.pop_front();
            self.transmit_bit = 0;
            self.transmit_countdown = self.cycles_per_bit;
        }
        let Some(byte) = self.transmit_byte else {
            return true;
        };

        let level = match self.transmit_bit {
            0 => false,
            1..=8 => byte & (1 << (self.transmit_bit - 1)) != 0,
            _ => true,
        };
        self.transmit_countdown -= 1;
        if self.transmit_countdown == 0 {
            self.transmit_bit += 1;
            self.transmit_countdown = self.cycles_per_bit;
            if self.transmit_bit > 9 {
                self.transmit_byte = None;
            }
        }
        level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive_byte(serial: &mut BitBangSerial, byte: u8, period: u32) {
        for bit in 0..10 {
            let level = match bit {
                0 => false,
                1..=8 => byte & (1 << (bit - 1)) != 0,
                _ => true,
            };
            for _ in 0..period {
                serial.tick(level);
            }
        }
        serial.tick(true);
    }

    #[test]
    fn decodes_8n1_bytes_from_cycle_levels() {
        let mut serial = BitBangSerial::new(12);
        drive_byte(&mut serial, 0xA5, 12);
        assert_eq!(serial.take_output(), [0xA5]);
    }

    #[test]
    fn emits_start_data_and_stop_bits_for_queued_byte() {
        let mut serial = BitBangSerial::new(4);
        serial.queue_input(&[0x03]);
        let levels: Vec<bool> = (0..40).map(|_| serial.tick(true)).collect();
        for bit in 0..10 {
            let expected = match bit {
                0 => false,
                1..=8 => 0x03 & (1 << (bit - 1)) != 0,
                _ => true,
            };
            assert!(levels[bit * 4..bit * 4 + 4].iter().all(|&v| v == expected));
        }
    }
}
