//! Oric cassette transport.
//!
//! A TAP file stores decoded bytes, so playback reconstructs the waveform the
//! ROM sees on VIA CB1.  Oricutron's `tape.c` is the implementation precedent:
//! 208-cycle marks, 416-cycle spaces, LSB-first data, odd parity, and three
//! stop bits.  Compact TAP leaders are extended when the motor starts and a
//! short mark gap follows each header so the ROM can change routines.

use serde::{Deserialize, Serialize};

const MARK_CYCLES: u32 = 208;
const SPACE_CYCLES: u32 = 416;
const LEADER_EXTENSION_BYTES: u8 = 80;
const HEADER_GAP_CYCLES: u32 = 1281;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TapeDeck {
    bytes: Vec<u8>,
    offset: usize,
    bit: u8,
    parity: bool,
    level: bool,
    cycles_left: u32,
    pulse_cycles: u32,
    motor_was_on: bool,
    leader_extension: u8,
    header_end: Option<usize>,
    header_gap_left: u32,
}

impl TapeDeck {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self {
            header_end: header_end(&bytes),
            bytes,
            offset: 0,
            bit: 0,
            parity: true,
            level: false,
            cycles_left: 0,
            pulse_cycles: MARK_CYCLES,
            motor_was_on: false,
            leader_extension: 0,
            header_gap_left: 0,
        }
    }

    pub(crate) fn tick(&mut self, motor_on: bool) -> bool {
        if !motor_on {
            if self.motor_was_on && self.bit != 0 {
                self.offset = self.offset.saturating_add(1);
                self.bit = 0;
            }
            self.motor_was_on = false;
            return self.level;
        }
        if !self.motor_was_on {
            self.motor_was_on = true;
            if self.bytes.get(self.offset) == Some(&0x16) {
                self.leader_extension = LEADER_EXTENSION_BYTES;
                self.header_end =
                    header_end(&self.bytes[self.offset..]).map(|end| self.offset + end);
            }
        }
        if self.header_gap_left > 0 {
            self.header_gap_left -= 1;
            return self.level;
        }
        if self.cycles_left > 0 {
            self.cycles_left -= 1;
            return self.level;
        }
        if self.offset >= self.bytes.len() {
            return self.level;
        }

        self.level = !self.level;
        let pulse = if self.level {
            self.pulse_cycles = self.next_pulse();
            self.pulse_cycles
        } else {
            // Both halves of a bit have the same duration.
            self.pulse_cycles
        };
        self.cycles_left = pulse.saturating_sub(1);
        self.level
    }

    fn next_pulse(&mut self) -> u32 {
        let pulse = self.pulse_for_current_bit();
        if self.bit == 1 {
            self.parity = true;
        } else if (2..=9).contains(&self.bit)
            && self.bytes[self.offset] & (1 << (self.bit - 2)) != 0
        {
            self.parity = !self.parity;
        }
        self.bit = (self.bit + 1) % 14;
        if self.bit == 0 {
            if self.leader_extension > 0 {
                self.leader_extension -= 1;
            } else {
                self.offset += 1;
                if self.header_end == Some(self.offset) {
                    self.header_gap_left = HEADER_GAP_CYCLES;
                    self.header_end =
                        header_end(&self.bytes[self.offset..]).map(|end| self.offset + end);
                }
            }
        }
        pulse
    }

    fn pulse_for_current_bit(&self) -> u32 {
        let mark = match self.bit {
            0 | 11..=13 => true,
            1 => false,
            2..=9 => self.bytes[self.offset] & (1 << (self.bit - 2)) != 0,
            10 => self.parity,
            _ => unreachable!("Oric tape framing has fourteen bits"),
        };
        if mark { MARK_CYCLES } else { SPACE_CYCLES }
    }

    pub(crate) fn position(&self) -> usize {
        self.offset
    }
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    let leader = bytes.iter().take_while(|&&byte| byte == 0x16).count();
    if leader < 3 || bytes.get(leader) != Some(&0x24) {
        return None;
    }
    let name_start = leader.checked_add(10)?;
    let name_len = bytes
        .get(name_start..)?
        .iter()
        .position(|&byte| byte == 0)?;
    name_start.checked_add(name_len)?.checked_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_stops_with_the_motor() {
        let mut tape = TapeDeck::new(vec![0x41]);
        for _ in 0..20_000 {
            tape.tick(false);
        }
        assert_eq!(tape.position(), 0);
        for _ in 0..20_000 {
            tape.tick(true);
        }
        assert_eq!(tape.position(), 1);
    }
}
