//! The intelligent keyboard (IKBD).
//!
//! The ST's keyboard is a computer of its own: a Hitachi HD6301 running a
//! mask-ROM program, talking to the ST over a 7812.5-baud serial link through
//! the keyboard ACIA. It scans the keys, reads the mouse and joysticks, keeps
//! a time-of-day clock, and answers a small command language.
//!
//! This module simulates that program at the protocol level rather than
//! emulating the 6301 and its ROM. The ROM is Atari's and unavailable to the
//! project, and no clean-room 6301 program exists. What software sees is the
//! byte stream, and the protocol is documented: the *Intelligent Keyboard
//! (ikbd) Protocol* (Atari, 1985) and the ST's technical documentation. Facts
//! were cross-checked against Hatari's `ikbd.c` (GPL-2, not used as a code
//! source).
//!
//! # What is modelled
//!
//! - Reset (`$80 $01`), answered with `$F1` about 62.75 ms later, and the
//!   default state it restores: relative mouse, joystick event reporting,
//!   output enabled.
//! - Key make and break codes, the scancode with bit 7 set on release.
//! - Relative mouse packets (`$F8`–`$FB` with the buttons, then signed X and
//!   Y), absolute mouse mode with its scale and limits and the position
//!   inquiry, mouse off, and the Y-axis direction.
//! - Joystick event packets (`$FE`/`$FF` and the stick byte), interrogation
//!   mode and its inquiry, and joysticks off.
//! - Pause and resume output (`$13`/`$11`).
//! - The time-of-day clock: set (`$1B`) and read (`$1C`, answered with `$FC`
//!   and six BCD bytes), advancing once a second.
//! - Status inquiries (`$87`–`$9A`), answered with `$F6` and seven bytes.
//!
//! Commands that load or run code in the 6301's RAM (`$20`–`$22`) are
//! consumed with their parameters and otherwise ignored; there is no 6301 to
//! run them.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// CPU cycles (8 MHz) from a reset command to the `$F1` reply.
const RESET_CYCLES: u32 = 502_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum MouseMode {
    Relative,
    Absolute,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum JoystickMode {
    Event,
    Interrogate,
    Off,
}

/// The joystick port's direction and fire bits, as the IKBD reports them:
/// up, down, left, right in bits 0–3 and fire in bit 7.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stick(pub u8);

impl Stick {
    pub const UP: u8 = 0x01;
    pub const DOWN: u8 = 0x02;
    pub const LEFT: u8 = 0x04;
    pub const RIGHT: u8 = 0x08;
    pub const FIRE: u8 = 0x80;
}

/// The IKBD.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ikbd {
    command: Vec<u8>,
    out: VecDeque<u8>,
    reset_countdown: Option<u32>,
    paused: bool,
    mouse_mode: MouseMode,
    joystick_mode: JoystickMode,
    y_down: bool,
    /// Mouse buttons: bit 0 right, bit 1 left.
    buttons: u8,
    reported_buttons: u8,
    dx: i32,
    dy: i32,
    abs_x: i32,
    abs_y: i32,
    abs_max: (i32, i32),
    abs_scale: (i32, i32),
    sticks: [Stick; 2],
    reported_sticks: [Stick; 2],
    /// Year, month, day, hours, minutes, seconds, in BCD.
    clock: [u8; 6],
    clock_cycles: u32,
    cpu_hz: u32,
}

impl Ikbd {
    /// An IKBD for a machine whose CPU runs at `cpu_hz`.
    #[must_use]
    pub fn new(cpu_hz: u32) -> Self {
        Self {
            command: Vec::new(),
            out: VecDeque::new(),
            reset_countdown: None,
            paused: false,
            mouse_mode: MouseMode::Relative,
            joystick_mode: JoystickMode::Event,
            y_down: true,
            buttons: 0,
            reported_buttons: 0,
            dx: 0,
            dy: 0,
            abs_x: 0,
            abs_y: 0,
            abs_max: (0, 0),
            abs_scale: (1, 1),
            sticks: [Stick::default(); 2],
            reported_sticks: [Stick::default(); 2],
            clock: [0x86, 0x01, 0x01, 0x00, 0x00, 0x00],
            clock_cycles: 0,
            cpu_hz,
        }
    }

    /// The next byte the IKBD sends to the ST, if output is not paused.
    pub fn next_output(&mut self) -> Option<u8> {
        if self.paused {
            return None;
        }
        self.out.pop_front()
    }

    /// A byte from the ST.
    pub fn receive(&mut self, byte: u8) {
        self.command.push(byte);
        let first = self.command[0];
        let needed = match first {
            0x80 | 0x07 | 0x17 => 2,
            0x09 => 5,
            0x0A..=0x0C => 3,
            0x0E => 6,
            0x19 | 0x1B => 7,
            0x21 | 0x22 => 3,
            0x20 => {
                // Load memory: address, count, then count bytes.
                if self.command.len() < 4 {
                    4
                } else {
                    4 + usize::from(self.command[3])
                }
            }
            _ => 1,
        };
        if self.command.len() < needed {
            return;
        }
        let command = std::mem::take(&mut self.command);
        self.execute(&command);
    }

    fn execute(&mut self, c: &[u8]) {
        match c[0] {
            0x80 => {
                if c[1] == 0x01 {
                    self.reset_countdown = Some(RESET_CYCLES);
                }
            }
            0x08 => self.mouse_mode = MouseMode::Relative,
            0x09 => {
                self.mouse_mode = MouseMode::Absolute;
                self.abs_max = (be16(c, 1), be16(c, 3));
            }
            0x0C => self.abs_scale = (i32::from(c[1].max(1)), i32::from(c[2].max(1))),
            0x0D => {
                let mut report = vec![0xF7, 0];
                let changes = self.buttons ^ self.reported_buttons;
                // Bits: 0 right down, 1 right up, 2 left down, 3 left up.
                if changes & 1 != 0 {
                    report[1] |= if self.buttons & 1 != 0 { 1 } else { 2 };
                }
                if changes & 2 != 0 {
                    report[1] |= if self.buttons & 2 != 0 { 4 } else { 8 };
                }
                self.reported_buttons = self.buttons;
                report.extend((self.abs_x as u16).to_be_bytes());
                report.extend((self.abs_y as u16).to_be_bytes());
                self.out.extend(report);
            }
            0x0E => {
                self.abs_x = be16(c, 2);
                self.abs_y = be16(c, 4);
            }
            0x0F => self.y_down = false,
            0x10 => self.y_down = true,
            0x11 => self.paused = false,
            0x12 => self.mouse_mode = MouseMode::Off,
            0x13 => self.paused = true,
            0x14 => self.joystick_mode = JoystickMode::Event,
            0x15 => self.joystick_mode = JoystickMode::Interrogate,
            0x16 => {
                let [a, b] = self.sticks;
                self.out.extend([0xFD, a.0, b.0]);
            }
            0x1A => self.joystick_mode = JoystickMode::Off,
            0x1B => {
                // Each BCD digit pair; $FF (or any invalid pair) leaves the
                // field unchanged.
                for (field, &value) in self.clock.iter_mut().zip(&c[1..7]) {
                    if value >> 4 <= 9 && value & 0x0F <= 9 {
                        *field = value;
                    }
                }
            }
            0x1C => {
                self.out.push_back(0xFC);
                self.out.extend(self.clock);
            }
            0x87..=0x9A => {
                let mode = match c[0] {
                    0x88 => match self.mouse_mode {
                        MouseMode::Relative => 0x08,
                        MouseMode::Absolute => 0x09,
                        MouseMode::Off => 0x12,
                    },
                    0x94 => match self.joystick_mode {
                        JoystickMode::Event => 0x14,
                        JoystickMode::Interrogate => 0x15,
                        JoystickMode::Off => 0x1A,
                    },
                    0x8F | 0x90 => {
                        if self.y_down {
                            0x10
                        } else {
                            0x0F
                        }
                    }
                    _ => 0,
                };
                self.out.extend([0xF6, mode, 0, 0, 0, 0, 0, 0]);
            }
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.out.clear();
        self.paused = false;
        self.mouse_mode = MouseMode::Relative;
        self.joystick_mode = JoystickMode::Event;
        self.y_down = true;
        self.dx = 0;
        self.dy = 0;
        self.reported_buttons = self.buttons;
        self.reported_sticks = self.sticks;
        self.out.push_back(0xF1);
    }

    /// One CPU cycle: the reset delay and the clock.
    pub fn tick(&mut self) {
        if let Some(cycles) = &mut self.reset_countdown {
            *cycles -= 1;
            if *cycles == 0 {
                self.reset_countdown = None;
                self.reset();
            }
        }
        self.clock_cycles += 1;
        if self.clock_cycles >= self.cpu_hz {
            self.clock_cycles = 0;
            self.advance_clock();
        }
    }

    /// Queue whatever the mouse and joysticks have to report, as the 6301
    /// does between scans. The host calls this at the scan rate.
    pub fn scan(&mut self) {
        if !self.out.is_empty() {
            return;
        }
        if self.mouse_mode == MouseMode::Relative
            && (self.dx != 0 || self.dy != 0 || self.buttons != self.reported_buttons)
        {
            let dx = self.dx.clamp(-128, 127);
            let dy = self.dy.clamp(-128, 127);
            self.dx -= dx;
            self.dy -= dy;
            let dy = if self.y_down { dy } else { -dy };
            // Header bit 0 is the right button, bit 1 the left.
            let header = 0xF8 | (self.buttons & 1) | (self.buttons & 2);
            self.out.extend([header, dx as u8, dy as u8]);
            self.reported_buttons = self.buttons;
        }
        if self.joystick_mode == JoystickMode::Event {
            for port in 0..2 {
                if self.sticks[port] != self.reported_sticks[port] {
                    self.out.extend([0xFE + port as u8, self.sticks[port].0]);
                    self.reported_sticks[port] = self.sticks[port];
                }
            }
        }
    }

    fn advance_clock(&mut self) {
        let limits = [0x99, 0x12, 0x31, 0x23, 0x59, 0x59];
        for field in (0..6).rev() {
            let value = self.clock[field];
            if value >= limits[field] {
                self.clock[field] = if field == 1 || field == 2 { 1 } else { 0 };
            } else {
                self.clock[field] = if value & 0x0F == 9 {
                    (value & 0xF0) + 0x10
                } else {
                    value + 1
                };
                return;
            }
        }
    }

    // ---- The host's side: the keys, mouse and sticks ----

    /// A key went down (`pressed`) or up. `scancode` is the ST's.
    pub fn key(&mut self, scancode: u8, pressed: bool) {
        let code = scancode & 0x7F;
        self.out.push_back(if pressed { code } else { code | 0x80 });
    }

    /// The mouse moved.
    pub fn mouse_move(&mut self, dx: i32, dy: i32) {
        match self.mouse_mode {
            MouseMode::Relative => {
                self.dx += dx;
                self.dy += dy;
            }
            MouseMode::Absolute => {
                self.abs_x = (self.abs_x + dx / self.abs_scale.0).clamp(0, self.abs_max.0);
                self.abs_y = (self.abs_y + dy / self.abs_scale.1).clamp(0, self.abs_max.1);
            }
            MouseMode::Off => {}
        }
    }

    /// The mouse buttons.
    pub fn mouse_buttons(&mut self, left: bool, right: bool) {
        self.buttons = u8::from(right) | u8::from(left) << 1;
    }

    /// One mouse button went down or up, leaving the other as it was.
    pub fn mouse_button(&mut self, left: bool, pressed: bool) {
        let bit = if left { 0x02 } else { 0x01 };
        if pressed {
            self.buttons |= bit;
        } else {
            self.buttons &= !bit;
        }
    }

    /// Joystick `port`'s state as last set.
    #[must_use]
    pub fn stick(&self, port: usize) -> Stick {
        self.sticks.get(port).copied().unwrap_or_default()
    }

    /// A joystick's state. Port 1 is the joystick port; port 0 shares the
    /// mouse port.
    pub fn joystick(&mut self, port: usize, stick: Stick) {
        if let Some(slot) = self.sticks.get_mut(port) {
            *slot = stick;
        }
    }
}

fn be16(c: &[u8], offset: usize) -> i32 {
    i32::from(u16::from_be_bytes([c[offset], c[offset + 1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(ikbd: &mut Ikbd) -> Vec<u8> {
        std::iter::from_fn(|| ikbd.next_output()).collect()
    }

    #[test]
    fn reset_answers_f1_after_the_6301_has_restarted() {
        let mut ikbd = Ikbd::new(8_000_000);
        ikbd.receive(0x80);
        ikbd.receive(0x01);
        for _ in 0..RESET_CYCLES - 1 {
            ikbd.tick();
        }
        assert!(drain(&mut ikbd).is_empty());
        ikbd.tick();
        assert_eq!(drain(&mut ikbd), vec![0xF1]);
    }

    #[test]
    fn one_button_changes_alone_and_sticks_read_back() {
        let mut ikbd = Ikbd::new(8_000_000);
        ikbd.mouse_button(false, true);
        ikbd.mouse_button(true, true);
        ikbd.mouse_button(false, false);
        ikbd.scan();
        assert_eq!(drain(&mut ikbd), vec![0xFA, 0, 0], "left only");
        ikbd.joystick(1, Stick(Stick::UP | Stick::FIRE));
        assert_eq!(ikbd.stick(1), Stick(0x81));
        assert_eq!(ikbd.stick(7), Stick::default());
    }

    #[test]
    fn keys_send_make_and_break_codes() {
        let mut ikbd = Ikbd::new(8_000_000);
        ikbd.key(0x1E, true);
        ikbd.key(0x1E, false);
        assert_eq!(drain(&mut ikbd), vec![0x1E, 0x9E]);
    }

    #[test]
    fn relative_mouse_packets_carry_buttons_and_split_large_moves() {
        let mut ikbd = Ikbd::new(8_000_000);
        ikbd.mouse_move(200, -5);
        ikbd.mouse_buttons(true, false);
        ikbd.scan();
        assert_eq!(drain(&mut ikbd), vec![0xFA, 127, (-5i8) as u8]);
        ikbd.scan();
        assert_eq!(drain(&mut ikbd), vec![0xFA, 73, 0]);
        ikbd.scan();
        assert!(drain(&mut ikbd).is_empty(), "nothing new to report");
        ikbd.receive(0x0F); // Y axis up
        ikbd.mouse_move(0, 3);
        ikbd.scan();
        assert_eq!(drain(&mut ikbd), vec![0xFA, 0, (-3i8) as u8]);
    }

    #[test]
    fn joystick_events_and_interrogation() {
        let mut ikbd = Ikbd::new(8_000_000);
        ikbd.joystick(1, Stick(Stick::UP | Stick::FIRE));
        ikbd.scan();
        assert_eq!(drain(&mut ikbd), vec![0xFF, 0x81]);
        ikbd.receive(0x15);
        ikbd.joystick(1, Stick(Stick::LEFT));
        ikbd.scan();
        assert!(drain(&mut ikbd).is_empty(), "interrogation mode is silent");
        ikbd.receive(0x16);
        assert_eq!(drain(&mut ikbd), vec![0xFD, 0x00, 0x04]);
        ikbd.receive(0x94);
        assert_eq!(drain(&mut ikbd)[..2], [0xF6, 0x15]);
    }

    #[test]
    fn absolute_mouse_mode_reports_its_position() {
        let mut ikbd = Ikbd::new(8_000_000);
        for byte in [0x09, 0x02, 0x7F, 0x01, 0x8F] {
            ikbd.receive(byte);
        }
        for byte in [0x0E, 0x00, 0x00, 0x10, 0x00, 0x20] {
            ikbd.receive(byte);
        }
        ikbd.mouse_move(5, 1000);
        ikbd.mouse_buttons(true, false);
        ikbd.receive(0x0D);
        assert_eq!(drain(&mut ikbd), vec![0xF7, 0x04, 0x00, 0x15, 0x01, 0x8F]);
        ikbd.receive(0x88);
        assert_eq!(drain(&mut ikbd)[..2], [0xF6, 0x09]);
    }

    #[test]
    fn the_clock_sets_reads_and_rolls_over() {
        let mut ikbd = Ikbd::new(100);
        for byte in [0x1B, 0x99, 0x12, 0x31, 0x23, 0x59, 0x58] {
            ikbd.receive(byte);
        }
        for _ in 0..200 {
            ikbd.tick();
        }
        ikbd.receive(0x1C);
        assert_eq!(
            drain(&mut ikbd),
            vec![0xFC, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00]
        );
        // $FF leaves a field alone.
        for byte in [0x1B, 0x26, 0xFF, 0xFF, 0x12, 0xFF, 0xFF] {
            ikbd.receive(byte);
        }
        ikbd.receive(0x1C);
        assert_eq!(drain(&mut ikbd)[1..5], [0x26, 0x01, 0x01, 0x12]);
    }

    #[test]
    fn pause_holds_output_and_load_memory_is_consumed() {
        let mut ikbd = Ikbd::new(8_000_000);
        ikbd.receive(0x13);
        ikbd.key(0x10, true);
        assert!(drain(&mut ikbd).is_empty());
        ikbd.receive(0x11);
        assert_eq!(drain(&mut ikbd), vec![0x10]);
        for byte in [0x20, 0x00, 0x80, 0x02, 0xAA, 0xBB] {
            ikbd.receive(byte);
        }
        ikbd.receive(0x12); // mouse off, parsed as its own command
        ikbd.mouse_move(1, 1);
        ikbd.scan();
        assert!(drain(&mut ikbd).is_empty());
        ikbd.receive(0x1A);
        ikbd.joystick(0, Stick(Stick::DOWN));
        ikbd.scan();
        assert!(drain(&mut ikbd).is_empty());
    }
}
