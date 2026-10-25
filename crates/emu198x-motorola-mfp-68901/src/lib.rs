//! Motorola MC68901 Multi-Function Peripheral.
//!
//! The MFP gathers four jobs on one chip:
//!
//! - **Eight general-purpose I/O lines** (GPIP), each an input or an output,
//!   each able to raise an interrupt on the edge the active-edge register
//!   chooses.
//! - **Four 8-bit timers.** A and B run in delay, event-count or pulse-width
//!   mode; C and D in delay mode only. Each counts down from its data register
//!   through a prescaler of 4, 10, 16, 50, 64, 100 or 200 MFP clocks.
//! - **A 16-channel interrupt controller.** It hands the CPU an 8-bit vector
//!   whose top four bits come from the vector register and whose bottom four
//!   name the channel, so the CPU needs no autovector.
//! - **A USART**, clocked from outside. On the Atari ST that clock is timer
//!   D's output.
//!
//! The chip is clocked by its host: one [`Mfp68901::tick`] is one cycle of the
//! MFP's own clock (2.4576 MHz on the ST, a crystal of its own). Register
//! access is [`Mfp68901::read`] and [`Mfp68901::write`] with the register
//! number 0–23; the host maps those onto its bus (odd bytes `$FFFA01`–`$FFFA2F`
//! on the ST). The interrupt request is the [`Mfp68901::irq`] pin and the
//! acknowledge cycle is [`Mfp68901::acknowledge`].
//!
//! # Interrupt channels
//!
//! Highest priority first:
//!
//! | Channel | Source | | Channel | Source |
//! |---|---|---|---|---|
//! | 15 | GPIP 7 | | 7 | GPIP 5 |
//! | 14 | GPIP 6 | | 6 | GPIP 4 |
//! | 13 | Timer A | | 5 | Timer C |
//! | 12 | Receive buffer full | | 4 | Timer D |
//! | 11 | Receive error | | 3 | GPIP 3 |
//! | 10 | Transmit buffer empty | | 2 | GPIP 2 |
//! | 9 | Transmit error | | 1 | GPIP 1 |
//! | 8 | Timer B | | 0 | GPIP 0 |
//!
//! The "A" registers (IERA, IPRA, ISRA, IMRA) hold channels 15–8 and the "B"
//! registers channels 7–0.
//!
//! A channel's event sets its pending bit only if its enable bit is set.
//! A pending, unmasked channel asks for an interrupt. In software
//! end-of-interrupt mode (vector register bit 3) the acknowledge cycle also
//! sets the channel's in-service bit, and only a channel of higher priority
//! than every in-service one may then interrupt, until software clears the
//! in-service bit. In automatic mode no in-service bits are kept.
//!
//! # Timers
//!
//! A timer's main counter counts down once per prescaler period. When it is at
//! 1 the next count is a time-out: the counter reloads from the data register,
//! the timer's output toggles and its channel fires. A data value of 0 counts
//! 256. Writing the data register while the timer is stopped loads the
//! counter too; while it runs only the data register changes, and the counter
//! takes the new value at the next time-out. Reading the data register reads
//! the main counter.
//!
//! In event-count mode the counter counts active edges of the timer's input
//! (TAI or TBI) instead of prescaled clocks. In pulse-width mode it counts
//! prescaled clocks only while the input is at its active level. The active
//! edge or level of TAI is AER bit 4 and of TBI AER bit 3, shared with GPIP 4
//! and GPIP 3.
//!
//! # Not modelled
//!
//! - The synchronisation delay between a control-register write and the
//!   timer's first count, and between a time-out and the interrupt reaching
//!   the IRQ pin. Both are a few MFP clocks on hardware.
//! - The USART's synchronous mode and break detection. Asynchronous framing,
//!   the buffer-full, overrun and buffer-empty flags and their interrupts are
//!   modelled at byte level, timed from the transmit/receive clock input.
//!
//! # Reference
//!
//! The MC68901 data sheet's register descriptions, cross-checked for facts
//! against Hatari's `mfp.c` (GPL-2, not used as a code source).

#![forbid(unsafe_code)]

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Register numbers, as the data sheet orders them.
pub mod reg {
    pub const GPIP: u8 = 0;
    pub const AER: u8 = 1;
    pub const DDR: u8 = 2;
    pub const IERA: u8 = 3;
    pub const IERB: u8 = 4;
    pub const IPRA: u8 = 5;
    pub const IPRB: u8 = 6;
    pub const ISRA: u8 = 7;
    pub const ISRB: u8 = 8;
    pub const IMRA: u8 = 9;
    pub const IMRB: u8 = 10;
    pub const VR: u8 = 11;
    pub const TACR: u8 = 12;
    pub const TBCR: u8 = 13;
    pub const TCDCR: u8 = 14;
    pub const TADR: u8 = 15;
    pub const TBDR: u8 = 16;
    pub const TCDR: u8 = 17;
    pub const TDDR: u8 = 18;
    pub const SCR: u8 = 19;
    pub const UCR: u8 = 20;
    pub const RSR: u8 = 21;
    pub const TSR: u8 = 22;
    pub const UDR: u8 = 23;
}

/// Interrupt channel numbers.
pub mod channel {
    pub const GPIP0: u8 = 0;
    pub const GPIP1: u8 = 1;
    pub const GPIP2: u8 = 2;
    pub const GPIP3: u8 = 3;
    pub const TIMER_D: u8 = 4;
    pub const TIMER_C: u8 = 5;
    pub const GPIP4: u8 = 6;
    pub const GPIP5: u8 = 7;
    pub const TIMER_B: u8 = 8;
    pub const TRANSMIT_ERROR: u8 = 9;
    pub const TRANSMIT_EMPTY: u8 = 10;
    pub const RECEIVE_ERROR: u8 = 11;
    pub const RECEIVE_FULL: u8 = 12;
    pub const TIMER_A: u8 = 13;
    pub const GPIP6: u8 = 14;
    pub const GPIP7: u8 = 15;
}

/// The channel each GPIP line raises.
const GPIP_CHANNEL: [u8; 8] = [0, 1, 2, 3, 6, 7, 14, 15];

/// The prescaler for control values 1–7 (and 9–15 in pulse-width mode).
const PRESCALE: [u16; 8] = [0, 4, 10, 16, 50, 64, 100, 200];

const VR_S: u8 = 0x08;

const RSR_RE: u8 = 0x01;
const RSR_FE: u8 = 0x10;
const RSR_OE: u8 = 0x40;
const RSR_BF: u8 = 0x80;

const TSR_TE: u8 = 0x01;
const TSR_END: u8 = 0x10;
const TSR_BE: u8 = 0x80;

/// The four timers, as indexes into [`Mfp68901`]'s timer array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerId {
    A,
    B,
    C,
    D,
}

impl TimerId {
    fn channel(self) -> u8 {
        match self {
            Self::A => channel::TIMER_A,
            Self::B => channel::TIMER_B,
            Self::C => channel::TIMER_C,
            Self::D => channel::TIMER_D,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Timer {
    /// The four-bit control value: 0 stopped, 1–7 delay, 8 event count,
    /// 9–15 pulse width.
    control: u8,
    data: u8,
    counter: u8,
    prescale_count: u16,
    output: bool,
    /// The timer input's level (TAI or TBI); unused by C and D.
    input: bool,
}

impl Timer {
    fn prescale(&self) -> u16 {
        PRESCALE[usize::from(self.control & 7)]
    }

    /// Count once; true on a time-out.
    fn count(&mut self) -> bool {
        if self.counter == 1 {
            self.counter = self.data;
            self.output = !self.output;
            true
        } else {
            self.counter = self.counter.wrapping_sub(1);
            false
        }
    }
}

/// A byte crossing the USART: the clock edges left until it is done.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Frame {
    byte: u8,
    remaining: u32,
}

/// The MC68901.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mfp68901 {
    gpip: u8,
    aer: u8,
    ddr: u8,
    /// The levels the host drives onto the GPIP pins.
    gpip_in: u8,
    ier: u16,
    ipr: u16,
    isr: u16,
    imr: u16,
    vr: u8,
    timers: [Timer; 4],

    scr: u8,
    ucr: u8,
    rsr: u8,
    tsr: u8,
    /// The last received byte, as UDR reads it.
    rx_data: u8,
    /// The byte waiting in the transmit buffer.
    tx_buffer: Option<u8>,
    tx_shift: Option<Frame>,
    rx_shift: Option<Frame>,
    rx_queue: VecDeque<u8>,
    transmitted: Vec<u8>,
    /// The USART clock divider: 1, or 16 when UCR bit 7 is set.
    usart_clock_count: u32,
}

impl Default for Mfp68901 {
    fn default() -> Self {
        Self::new()
    }
}

impl Mfp68901 {
    /// A chip just out of reset: every register zero, timers stopped, the
    /// transmit buffer empty.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gpip: 0,
            aer: 0,
            ddr: 0,
            gpip_in: 0xFF,
            ier: 0,
            ipr: 0,
            isr: 0,
            imr: 0,
            vr: 0,
            timers: Default::default(),
            scr: 0,
            ucr: 0,
            rsr: 0,
            tsr: TSR_BE,
            rx_data: 0,
            tx_buffer: None,
            tx_shift: None,
            rx_shift: None,
            rx_queue: VecDeque::new(),
            transmitted: Vec::new(),
            usart_clock_count: 0,
        }
    }

    /// The RESET pin: every register but the GPIP data and timer data
    /// registers clears, which stops the timers and the USART.
    pub fn reset(&mut self) {
        let gpip_in = self.gpip_in;
        let data: [u8; 4] = std::array::from_fn(|i| self.timers[i].data);
        *self = Self::new();
        self.gpip_in = gpip_in;
        for (timer, data) in self.timers.iter_mut().zip(data) {
            timer.data = data;
            timer.counter = data;
        }
    }

    // ---- Pins ----

    /// The interrupt request output, active when true.
    #[must_use]
    pub fn irq(&self) -> bool {
        self.highest_requesting().is_some()
    }

    /// The interrupt acknowledge cycle. Returns the vector the MFP puts on
    /// the bus, or `None` if no channel is asking (the CPU then sees no
    /// vector from the MFP).
    pub fn acknowledge(&mut self) -> Option<u8> {
        let channel = self.highest_requesting()?;
        let bit = 1 << channel;
        self.ipr &= !bit;
        if self.vr & VR_S != 0 {
            self.isr |= bit;
        }
        Some(self.vr & 0xF0 | channel)
    }

    /// Drive the GPIP input pins. Each bit set to an input by the data
    /// direction register raises its channel on the edge AER chooses.
    pub fn set_gpip_inputs(&mut self, levels: u8) {
        let before = self.gpip_levels();
        self.gpip_in = levels;
        self.gpip_edges(before);
    }

    /// Drive one GPIP input pin.
    pub fn set_gpip_input(&mut self, line: u8, level: bool) {
        let mask = 1 << (line & 7);
        let levels = if level {
            self.gpip_in | mask
        } else {
            self.gpip_in & !mask
        };
        self.set_gpip_inputs(levels);
    }

    /// Drive timer A's or B's input pin (TAI or TBI).
    pub fn set_timer_input(&mut self, timer: TimerId, level: bool) {
        let aer_bit = match timer {
            TimerId::A => 4,
            TimerId::B => 3,
            _ => return,
        };
        let index = timer as usize;
        let active = self.aer >> aer_bit & 1 != 0;
        let t = &mut self.timers[index];
        let rising_to_active = t.input != active && level == active;
        let leaving_active = t.input == active && level != active;
        t.input = level;
        let control = t.control;
        if control == 8 && rising_to_active && t.count() {
            self.raise(timer.channel());
        }
        // The end of a pulse-width measurement interrupts on the GPIP
        // channel the timer input shares.
        if (9..=15).contains(&control) && leaving_active {
            let gpip_channel = if index == 0 {
                channel::GPIP4
            } else {
                channel::GPIP3
            };
            self.raise(gpip_channel);
        }
    }

    /// A timer's output line (TAO–TDO), which toggles at each time-out.
    #[must_use]
    pub fn timer_output(&self, timer: TimerId) -> bool {
        self.timers[timer as usize].output
    }

    // ---- USART host side ----

    /// A byte arrives on the serial input. It reaches the receive buffer once
    /// its frame has been clocked in.
    pub fn receive(&mut self, byte: u8) {
        self.rx_queue.push_back(byte);
    }

    /// Bytes the USART has finished sending since the last call.
    pub fn take_transmitted(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.transmitted)
    }

    // ---- Clocking ----

    /// One MFP clock: the delay and pulse-width timers count their
    /// prescalers.
    pub fn tick(&mut self) {
        for index in 0..4 {
            let timer = &mut self.timers[index];
            let control = timer.control & 0x0F;
            let counting = match control {
                1..=7 => true,
                9..=15 => {
                    let active_bit = if index == 0 { 4 } else { 3 };
                    timer.input == (self.aer >> active_bit & 1 != 0)
                }
                _ => false,
            };
            if !counting {
                continue;
            }
            timer.prescale_count += 1;
            if timer.prescale_count >= timer.prescale() {
                timer.prescale_count = 0;
                let was_output = timer.output;
                if timer.count() {
                    let id = [TimerId::A, TimerId::B, TimerId::C, TimerId::D][index];
                    self.raise(id.channel());
                    // Timer D's output clocks the USART on the ST; a rising
                    // edge is one clock.
                    if id == TimerId::D && !was_output {
                        self.usart_clock();
                    }
                }
            }
        }
    }

    /// One edge of the USART's transmit/receive clock. On the ST this is
    /// timer D's output, which [`Mfp68901::tick`] feeds in itself; a host
    /// wiring another clock calls this directly.
    pub fn usart_clock(&mut self) {
        self.usart_clock_count += 1;
        let divider = if self.ucr & 0x80 != 0 { 16 } else { 1 };
        if self.usart_clock_count < divider {
            return;
        }
        self.usart_clock_count = 0;
        self.usart_bit();
    }

    fn frame_bits(&self) -> u32 {
        let data = 8 - u32::from(self.ucr >> 5 & 3);
        let parity = u32::from(self.ucr & 0x04 != 0);
        // Stop bits: 1, 1.5 rounded up, or 2. Synchronous mode (0) is not
        // modelled and frames as one stop bit.
        let stop = match self.ucr >> 3 & 3 {
            2 | 3 => 2,
            _ => 1,
        };
        1 + data + parity + stop
    }

    fn usart_bit(&mut self) {
        // Transmit.
        if self.tsr & TSR_TE != 0 {
            if self.tx_shift.is_none()
                && let Some(byte) = self.tx_buffer.take()
            {
                self.tx_shift = Some(Frame {
                    byte,
                    remaining: self.frame_bits(),
                });
                self.tsr |= TSR_BE;
                self.raise(channel::TRANSMIT_EMPTY);
            }
            if let Some(frame) = &mut self.tx_shift {
                frame.remaining -= 1;
                if frame.remaining == 0 {
                    let mask = 0xFFu8 >> (self.ucr >> 5 & 3);
                    self.transmitted.push(frame.byte & mask);
                    self.tx_shift = None;
                }
            }
        }
        // Receive.
        if self.rsr & RSR_RE != 0 {
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
                    let mask = 0xFFu8 >> (self.ucr >> 5 & 3);
                    let byte = frame.byte & mask;
                    self.rx_shift = None;
                    if self.rsr & RSR_BF != 0 {
                        self.rsr |= RSR_OE;
                        self.raise(channel::RECEIVE_ERROR);
                    } else {
                        self.rx_data = byte;
                        self.rsr |= RSR_BF;
                        self.raise(channel::RECEIVE_FULL);
                    }
                }
            }
        }
    }

    // ---- Registers ----

    /// Read register `register` (0–23). Reading the USART data register
    /// empties the receive buffer.
    pub fn read(&mut self, register: u8) -> u8 {
        let value = self.peek(register);
        match register {
            reg::RSR => self.rsr &= !(RSR_OE | RSR_FE),
            reg::UDR => self.rsr &= !RSR_BF,
            _ => {}
        }
        value
    }

    /// Read a register without side effects, for a debugger.
    #[must_use]
    pub fn peek(&self, register: u8) -> u8 {
        let high = |v: u16| (v >> 8) as u8;
        let low = |v: u16| v as u8;
        match register {
            reg::GPIP => self.gpip_levels(),
            reg::AER => self.aer,
            reg::DDR => self.ddr,
            reg::IERA => high(self.ier),
            reg::IERB => low(self.ier),
            reg::IPRA => high(self.ipr),
            reg::IPRB => low(self.ipr),
            reg::ISRA => high(self.isr),
            reg::ISRB => low(self.isr),
            reg::IMRA => high(self.imr),
            reg::IMRB => low(self.imr),
            reg::VR => self.vr,
            reg::TACR => self.timers[0].control,
            reg::TBCR => self.timers[1].control,
            reg::TCDCR => self.timers[2].control << 4 | self.timers[3].control,
            reg::TADR..=reg::TDDR => self.timers[usize::from(register - reg::TADR)].counter,
            reg::SCR => self.scr,
            reg::UCR => self.ucr,
            reg::RSR => self.rsr,
            reg::TSR => self.tsr,
            reg::UDR => self.rx_data,
            _ => 0xFF,
        }
    }

    /// Write register `register` (0–23).
    pub fn write(&mut self, register: u8, value: u8) {
        let set_high = |v: &mut u16, b: u8| *v = (*v & 0x00FF) | u16::from(b) << 8;
        let set_low = |v: &mut u16, b: u8| *v = (*v & 0xFF00) | u16::from(b);
        match register {
            reg::GPIP => {
                let before = self.gpip_levels();
                self.gpip = value;
                self.gpip_edges(before);
            }
            reg::AER => {
                // Changing the active edge can itself complete an edge.
                let before_active = !(self.gpip_levels() ^ self.aer);
                self.aer = value;
                let after_active = !(self.gpip_levels() ^ self.aer);
                self.raise_gpip(!before_active & after_active);
            }
            reg::DDR => {
                let before = self.gpip_levels();
                self.ddr = value;
                self.gpip_edges(before);
            }
            reg::IERA => {
                set_high(&mut self.ier, value);
                self.ipr &= self.ier;
            }
            reg::IERB => {
                set_low(&mut self.ier, value);
                self.ipr &= self.ier;
            }
            // Pending and in-service bits clear where a zero is written.
            reg::IPRA => self.ipr &= u16::from(value) << 8 | 0x00FF,
            reg::IPRB => self.ipr &= 0xFF00 | u16::from(value),
            reg::ISRA => self.isr &= u16::from(value) << 8 | 0x00FF,
            reg::ISRB => self.isr &= 0xFF00 | u16::from(value),
            reg::IMRA => set_high(&mut self.imr, value),
            reg::IMRB => set_low(&mut self.imr, value),
            reg::VR => {
                self.vr = value;
                if value & VR_S == 0 {
                    self.isr = 0;
                }
            }
            reg::TACR => self.set_control(0, value & 0x0F),
            reg::TBCR => self.set_control(1, value & 0x0F),
            reg::TCDCR => {
                self.set_control(2, value >> 4 & 7);
                self.set_control(3, value & 7);
            }
            reg::TADR..=reg::TDDR => {
                let timer = &mut self.timers[usize::from(register - reg::TADR)];
                timer.data = value;
                if timer.control == 0 {
                    timer.counter = value;
                }
            }
            reg::SCR => self.scr = value,
            reg::UCR => self.ucr = value,
            reg::RSR => self.rsr = (self.rsr & !RSR_RE) | (value & RSR_RE),
            reg::TSR => {
                self.tsr = (self.tsr & !(TSR_TE | 0x0E)) | (value & (TSR_TE | 0x0E));
                if value & TSR_TE == 0 {
                    self.tsr |= TSR_END;
                } else {
                    self.tsr &= !TSR_END;
                }
            }
            reg::UDR => {
                self.tx_buffer = Some(value);
                self.tsr &= !TSR_BE;
            }
            _ => {}
        }
    }

    fn set_control(&mut self, index: usize, control: u8) {
        let timer = &mut self.timers[index];
        if timer.control == 0 && control != 0 {
            timer.prescale_count = 0;
        }
        timer.control = control;
    }

    // ---- Interrupt logic ----

    fn gpip_levels(&self) -> u8 {
        (self.gpip & self.ddr) | (self.gpip_in & !self.ddr)
    }

    /// Raise the GPIP channels whose lines became active since `before`.
    fn gpip_edges(&mut self, before: u8) {
        let after = self.gpip_levels();
        let before_active = !(before ^ self.aer);
        let after_active = !(after ^ self.aer);
        self.raise_gpip(!before_active & after_active);
    }

    fn raise_gpip(&mut self, lines: u8) {
        for (line, &channel) in GPIP_CHANNEL.iter().enumerate() {
            if lines >> line & 1 != 0 {
                self.raise(channel);
            }
        }
    }

    /// A channel's event: pending if enabled.
    fn raise(&mut self, channel: u8) {
        let bit = 1 << channel;
        if self.ier & bit != 0 {
            self.ipr |= bit;
        }
    }

    fn highest_requesting(&self) -> Option<u8> {
        let requesting = self.ipr & self.imr;
        if requesting == 0 {
            return None;
        }
        let channel = 15 - requesting.leading_zeros() as u8;
        if self.vr & VR_S != 0 && self.isr != 0 {
            let in_service = 15 - self.isr.leading_zeros() as u8;
            if channel <= in_service {
                return None;
            }
        }
        Some(channel)
    }
}

#[cfg(test)]
mod tests;
