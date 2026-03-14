//! Atari POKEY (Potentiometer and Keyboard) chip emulator.
//!
//! The POKEY provides four audio channels with programmable frequency
//! dividers, polynomial counter noise generators, timer interrupts, an
//! analog potentiometer scanner, serial I/O, and a random number
//! generator. It appears in the Atari 5200, 400/800, XL/XE, and
//! various Atari arcade boards.
//!
//! # Write Registers ($00-$0F)
//!
//! | Addr | Name   | Description                                   |
//! |------|--------|-----------------------------------------------|
//! | $00  | AUDF1  | Audio frequency channel 1                     |
//! | $01  | AUDC1  | Audio control channel 1                       |
//! | $02  | AUDF2  | Audio frequency channel 2                     |
//! | $03  | AUDC2  | Audio control channel 2                       |
//! | $04  | AUDF3  | Audio frequency channel 3                     |
//! | $05  | AUDC3  | Audio control channel 3                       |
//! | $06  | AUDF4  | Audio frequency channel 4                     |
//! | $07  | AUDC4  | Audio control channel 4                       |
//! | $08  | AUDCTL | Audio control (clocks, filters, poly size)     |
//! | $09  | STIMER | Start timers (resets all channel counters)     |
//! | $0A  | SKRES  | Serial port status reset                      |
//! | $0B  | POTGO  | Start pot scan                                |
//! | $0D  | SEROUT | Serial output data                            |
//! | $0E  | IRQEN  | IRQ enable mask                               |
//! | $0F  | SKCTL  | Serial port control                           |
//!
//! # Read Registers ($00-$0F)
//!
//! | Addr | Name   | Description                                   |
//! |------|--------|-----------------------------------------------|
//! | $00  | POT0   | Potentiometer 0 value (0-228)                 |
//! | $01  | POT1   | Potentiometer 1 value (0-228)                 |
//! | $02  | POT2   | Potentiometer 2 value (0-228)                 |
//! | $03  | POT3   | Potentiometer 3 value (0-228)                 |
//! | $04  | POT4   | Potentiometer 4 value (0-228)                 |
//! | $05  | POT5   | Potentiometer 5 value (0-228)                 |
//! | $06  | POT6   | Potentiometer 6 value (0-228)                 |
//! | $07  | POT7   | Potentiometer 7 value (0-228)                 |
//! | $08  | ALLPOT | Pot scan status (0 = done per bit)            |
//! | $09  | KBCODE | Keyboard code                                 |
//! | $0A  | RANDOM | Random number from polynomial counter         |
//! | $0D  | SERIN  | Serial input data                             |
//! | $0E  | IRQST  | IRQ status (active low: 0 = pending)          |
//! | $0F  | SKSTAT | Serial port status                            |

#![allow(clippy::cast_precision_loss)]

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Output sample rate (Hz).
const SAMPLE_RATE: u32 = 48_000;

/// Maximum pot counter value.
const POT_MAX: u8 = 228;

/// Number of potentiometer inputs.
const NUM_POTS: usize = 8;

/// CPU cycles per 64 kHz base clock tick (CPU / 28).
const DIVIDER_64KHZ: u16 = 28;

/// CPU cycles per 15 kHz base clock tick (CPU / 114, one scan line).
const DIVIDER_15KHZ: u16 = 114;

// Polynomial counter periods.
const POLY4_PERIOD: u32 = 15;
const POLY5_PERIOD: u32 = 31;
const POLY9_PERIOD: u32 = 511;
const POLY17_PERIOD: u32 = 131_071;

// IRQ bit masks (active-low in IRQST, active-high in IRQEN).
const IRQ_TIMER1: u8 = 0x01;
const IRQ_TIMER2: u8 = 0x02;
const IRQ_TIMER4: u8 = 0x04;
// Serial and other IRQ bits — not yet used by the audio/timer path,
// but defined here for completeness and future serial I/O support.
#[allow(dead_code)]
const IRQ_SERIN: u8 = 0x08;
#[allow(dead_code)]
const IRQ_SEROUT: u8 = 0x10;
#[allow(dead_code)]
const IRQ_SERFIN: u8 = 0x20;
#[allow(dead_code)]
const IRQ_OTHER: u8 = 0x40;
#[allow(dead_code)]
const IRQ_BREAK: u8 = 0x80;

// AUDCTL bit masks.
const AUDCTL_POLY9: u8 = 0x01;
const AUDCTL_CH1_179MHZ: u8 = 0x02;
const AUDCTL_CH3_179MHZ: u8 = 0x04;
const AUDCTL_16BIT_CH12: u8 = 0x08;
const AUDCTL_16BIT_CH34: u8 = 0x10;
const AUDCTL_HPF_CH1: u8 = 0x20;
const AUDCTL_HPF_CH2: u8 = 0x40;
const AUDCTL_15KHZ: u8 = 0x80;

// ---------------------------------------------------------------------------
// Polynomial counter tables (precomputed)
// ---------------------------------------------------------------------------

/// Build a polynomial counter lookup table.
///
/// The LFSR uses `feedback = bit(tap_high) XOR bit(tap_low)`, shifting
/// right with feedback entering the MSB.
fn build_poly_table(bits: u32, tap_high: u32, tap_low: u32) -> Vec<u8> {
    let period = (1u32 << bits) - 1;
    let mut table = Vec::with_capacity(period as usize);
    let mut lfsr: u32 = (1 << bits) - 1; // seed with all ones
    for _ in 0..period {
        table.push((lfsr & 1) as u8);
        let feedback = ((lfsr >> tap_high) ^ (lfsr >> tap_low)) & 1;
        lfsr = (lfsr >> 1) | (feedback << (bits - 1));
    }
    table
}

// ---------------------------------------------------------------------------
// Audio channel
// ---------------------------------------------------------------------------

/// One of four POKEY audio channels.
struct Channel {
    /// Frequency divider register (AUDF).
    audf: u8,
    /// Audio control register (AUDC).
    audc: u8,
    /// Current frequency counter (counts down).
    counter: u16,
    /// Channel output toggle (flips when counter underflows).
    output: bool,
    /// High-pass filter flip-flop (toggled by the paired channel).
    hp_flipflop: bool,
}

impl Channel {
    fn new() -> Self {
        Self {
            audf: 0,
            audc: 0,
            counter: 0,
            output: false,
            hp_flipflop: false,
        }
    }

    /// Volume from AUDC bits 3-0.
    fn volume(&self) -> u8 {
        self.audc & 0x0F
    }

    /// Volume-only mode: AUDC bit 4.
    fn volume_only(&self) -> bool {
        self.audc & 0x10 != 0
    }

    /// Distortion field: AUDC bits 7-5.
    fn distortion(&self) -> u8 {
        (self.audc >> 5) & 0x07
    }

    /// Reload the counter from AUDF.
    fn reload(&mut self) {
        self.counter = u16::from(self.audf);
    }

    /// Reload the counter for 16-bit paired mode (high byte from partner).
    #[allow(dead_code)]
    fn reload_16bit(&mut self, high_byte: u8) {
        self.counter = u16::from(self.audf) | (u16::from(high_byte) << 8);
    }
}

// ---------------------------------------------------------------------------
// POKEY
// ---------------------------------------------------------------------------

/// Atari POKEY chip.
pub struct Pokey {
    /// CPU clock frequency (Hz), e.g. `1_789_772` for NTSC.
    /// Stored for diagnostics and future use (e.g. serial baud rate calculation).
    #[allow(dead_code)]
    cpu_freq: u32,

    /// Four audio channels.
    channels: [Channel; 4],

    /// AUDCTL register.
    audctl: u8,

    /// IRQEN — interrupt enable mask.
    irqen: u8,

    /// IRQST — interrupt status (active low: 0 = pending).
    /// Initialised to $FF (no interrupts pending).
    irqst: u8,

    /// SKCTL — serial port control.
    skctl: u8,

    /// SKSTAT — serial port status (active low).
    skstat: u8,

    /// SERIN — serial input data.
    serin: u8,

    /// SEROUT — serial output data.
    serout: u8,

    /// KBCODE — keyboard scan code.
    kbcode: u8,

    // -- Potentiometers --
    /// Target pot values set externally (0-228).
    pot_target: [u8; NUM_POTS],

    /// Latched pot values (readable at POT0-POT7).
    pot_value: [u8; NUM_POTS],

    /// Pot scan counters (0-228).
    pot_counter: [u8; NUM_POTS],

    /// Pot scan active (started by POTGO write).
    pot_scanning: bool,

    /// CPU cycle counter for pot scan timing (one increment per scan line).
    pot_line_counter: u16,

    // -- Polynomial counters --
    poly4_table: Vec<u8>,
    poly5_table: Vec<u8>,
    poly9_table: Vec<u8>,
    poly17_table: Vec<u8>,

    /// Global polynomial counter index (counts every CPU cycle).
    poly_counter: u32,

    // -- Base clock dividers --
    /// Divider for the 64 kHz / 15 kHz base clock.
    base_divider: u16,

    // -- Audio output --
    /// Accumulator for downsampling.
    accumulator: f32,

    /// Number of CPU ticks accumulated.
    sample_count: u32,

    /// CPU ticks per output sample (fractional).
    ticks_per_sample: f32,

    /// Output sample buffer at 48 kHz.
    buffer: Vec<f32>,

    /// DC-blocking high-pass filter state.
    hp_prev_in: f32,
    hp_prev_out: f32,
}

impl Pokey {
    /// Create a new POKEY clocked at the given CPU frequency.
    ///
    /// For NTSC Atari systems, pass `1_789_772`. For PAL, pass `1_773_447`.
    #[must_use]
    pub fn new(cpu_freq: u32) -> Self {
        Self {
            cpu_freq,
            channels: [Channel::new(), Channel::new(), Channel::new(), Channel::new()],
            audctl: 0,
            irqen: 0,
            irqst: 0xFF,
            skctl: 0,
            skstat: 0xFF,
            serin: 0,
            serout: 0,
            kbcode: 0,
            pot_target: [0; NUM_POTS],
            pot_value: [0; NUM_POTS],
            pot_counter: [0; NUM_POTS],
            pot_scanning: false,
            pot_line_counter: 0,
            poly4_table: build_poly_table(4, 3, 2),
            poly5_table: build_poly_table(5, 4, 2),
            poly9_table: build_poly_table(9, 8, 4),
            poly17_table: build_poly_table(17, 16, 4),
            poly_counter: 0,
            base_divider: 0,
            accumulator: 0.0,
            sample_count: 0,
            ticks_per_sample: cpu_freq as f32 / SAMPLE_RATE as f32,
            buffer: Vec::with_capacity(SAMPLE_RATE as usize / 50 + 1),
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
        }
    }

    // -- Public interface -----------------------------------------------------

    /// Tick the POKEY for one CPU cycle.
    pub fn tick(&mut self) {
        // Advance polynomial counters (run at CPU clock rate).
        self.poly_counter = self.poly_counter.wrapping_add(1);

        // Pot scan: one increment per scan line (114 CPU cycles).
        if self.pot_scanning {
            self.pot_line_counter += 1;
            if self.pot_line_counter >= DIVIDER_15KHZ {
                self.pot_line_counter = 0;
                self.tick_pot_scan();
            }
        }

        // Base clock divider for 64 kHz / 15 kHz channels.
        self.base_divider += 1;
        let base_period = if self.audctl & AUDCTL_15KHZ != 0 {
            DIVIDER_15KHZ
        } else {
            DIVIDER_64KHZ
        };

        let base_tick = self.base_divider >= base_period;
        if base_tick {
            self.base_divider = 0;
        }

        // Tick channels.
        self.tick_channels(base_tick);

        // Downsample to 48 kHz.
        let sample = self.mix();
        self.accumulator += sample;
        self.sample_count += 1;

        if self.sample_count as f32 >= self.ticks_per_sample {
            let avg = self.accumulator / self.sample_count as f32;

            // DC-blocking high-pass filter.
            // y[n] = alpha * (y[n-1] + x[n] - x[n-1]), alpha ~= 0.9952 (~37 Hz at 48 kHz)
            const ALPHA: f32 = 0.9952;
            let filtered = ALPHA * (self.hp_prev_out + avg - self.hp_prev_in);
            self.hp_prev_in = avg;
            self.hp_prev_out = filtered;

            self.buffer.push(filtered);
            self.accumulator = 0.0;
            self.sample_count = 0;
        }
    }

    /// Read a POKEY register (addr $00-$0F).
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        match addr & 0x0F {
            // POT0-POT7: latched pot values.
            0x00..=0x07 => self.pot_value[(addr & 0x07) as usize],

            // ALLPOT: pot scan status (0 = scan complete for that pot).
            0x08 => {
                if !self.pot_scanning {
                    return 0x00; // All done.
                }
                let mut status = 0u8;
                for i in 0..NUM_POTS {
                    if self.pot_counter[i] < self.pot_target[i] {
                        status |= 1 << i;
                    }
                }
                status
            }

            // KBCODE: keyboard scan code.
            0x09 => self.kbcode,

            // RANDOM: read from polynomial counter.
            0x0A => {
                if self.audctl & AUDCTL_POLY9 != 0 {
                    let idx = (self.poly_counter as usize) % (POLY9_PERIOD as usize);
                    // Read 8 consecutive bits from the 9-bit poly counter.
                    Self::read_poly_byte(&self.poly9_table, idx, POLY9_PERIOD)
                } else {
                    let idx = (self.poly_counter as usize) % (POLY17_PERIOD as usize);
                    Self::read_poly_byte(&self.poly17_table, idx, POLY17_PERIOD)
                }
            }

            // $0B, $0C: unused read addresses, return $FF.
            0x0B | 0x0C => 0xFF,

            // SERIN: serial input data.
            0x0D => self.serin,

            // IRQST: interrupt status (active low).
            0x0E => self.irqst,

            // SKSTAT: serial port status.
            0x0F => self.skstat,

            _ => 0xFF,
        }
    }

    /// Write a POKEY register (addr $00-$0F).
    pub fn write(&mut self, addr: u8, value: u8) {
        match addr & 0x0F {
            // AUDF1-AUDF4: frequency registers.
            0x00 => self.channels[0].audf = value,
            0x02 => self.channels[1].audf = value,
            0x04 => self.channels[2].audf = value,
            0x06 => self.channels[3].audf = value,

            // AUDC1-AUDC4: audio control registers.
            0x01 => self.channels[0].audc = value,
            0x03 => self.channels[1].audc = value,
            0x05 => self.channels[2].audc = value,
            0x07 => self.channels[3].audc = value,

            // AUDCTL: audio control.
            0x08 => self.audctl = value,

            // STIMER: writing any value resets all channel counters.
            0x09 => {
                for ch in &mut self.channels {
                    ch.reload();
                }
            }

            // SKRES: reset serial port status bits.
            0x0A => {
                self.skstat = 0xFF;
            }

            // POTGO: start pot scan.
            0x0B => {
                self.pot_scanning = true;
                self.pot_line_counter = 0;
                for i in 0..NUM_POTS {
                    self.pot_counter[i] = 0;
                    self.pot_value[i] = POT_MAX;
                }
            }

            // $0C: unused write address.
            0x0C => {}

            // SEROUT: serial output data.
            0x0D => {
                self.serout = value;
            }

            // IRQEN: interrupt enable mask.
            // Writing also clears corresponding bits in IRQST for disabled IRQs.
            0x0E => {
                self.irqen = value;
                // Disabled interrupts are immediately cleared in IRQST (set to 1 = not pending).
                self.irqst |= !value;
            }

            // SKCTL: serial port control.
            0x0F => {
                self.skctl = value;
            }

            _ => {}
        }
    }

    /// Drain the audio output buffer. Returns mono f32 samples at 48 kHz,
    /// in the range -1.0 to 1.0.
    pub fn take_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    /// Number of samples currently in the audio buffer.
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Set a potentiometer target value (index 0-7, value 0-228).
    ///
    /// The pot scanner will latch this value during the next POTGO scan.
    /// For the Atari 5200: index 0/1 = controller 1 X/Y,
    /// index 2/3 = controller 2 X/Y.
    pub fn set_pot(&mut self, index: u8, value: u8) {
        if (index as usize) < NUM_POTS {
            self.pot_target[index as usize] = value.min(POT_MAX);
        }
    }

    /// Returns true if any enabled interrupt is pending.
    #[must_use]
    pub fn irq_pending(&self) -> bool {
        // IRQST is active-low (0 = pending). IRQEN selects which are enabled.
        (self.irqst & self.irqen) != self.irqen
    }

    /// Set the keyboard code register (written by external keyboard controller).
    pub fn set_kbcode(&mut self, code: u8) {
        self.kbcode = code;
    }

    /// Set the serial input data register.
    pub fn set_serin(&mut self, data: u8) {
        self.serin = data;
    }

    /// Read the serial output data register.
    #[must_use]
    pub fn serout(&self) -> u8 {
        self.serout
    }

    /// Get the IRQST register value (for diagnostics).
    #[must_use]
    pub fn irqst(&self) -> u8 {
        self.irqst
    }

    /// Get the IRQEN register value (for diagnostics).
    #[must_use]
    pub fn irqen(&self) -> u8 {
        self.irqen
    }

    /// Get the AUDCTL register value (for diagnostics).
    #[must_use]
    pub fn audctl(&self) -> u8 {
        self.audctl
    }

    // -- Internal helpers -----------------------------------------------------

    /// Tick the four audio channels. `base_tick` is true when the 64/15 kHz
    /// divider has fired.
    fn tick_channels(&mut self, base_tick: bool) {
        // Determine which channels tick this cycle.
        // Channels 1 and 3 can optionally run at 1.79 MHz (every CPU cycle).
        // Otherwise they tick at the base clock rate.
        let ch1_tick = if self.audctl & AUDCTL_CH1_179MHZ != 0 {
            true
        } else {
            base_tick
        };
        let ch3_tick = if self.audctl & AUDCTL_CH3_179MHZ != 0 {
            true
        } else {
            base_tick
        };

        // Channels 2 and 4 always use the base clock.
        let ch2_tick = base_tick;
        let ch4_tick = base_tick;

        // 16-bit mode: ch1+ch2 paired, ch3+ch4 paired.
        // In 16-bit mode, the low channel (1 or 3) clocks the high channel
        // (2 or 4) on underflow instead of using the base clock.
        let pair_12 = self.audctl & AUDCTL_16BIT_CH12 != 0;
        let pair_34 = self.audctl & AUDCTL_16BIT_CH34 != 0;

        // Tick channel 1.
        let ch1_underflow = if ch1_tick {
            self.tick_single_channel(0)
        } else {
            false
        };

        // Tick channel 2.
        let ch2_underflow = if pair_12 {
            // In 16-bit mode, ch2 is clocked by ch1 underflow.
            if ch1_underflow {
                self.tick_single_channel(1)
            } else {
                false
            }
        } else if ch2_tick {
            self.tick_single_channel(1)
        } else {
            false
        };

        // Tick channel 3.
        let ch3_underflow = if ch3_tick {
            self.tick_single_channel(2)
        } else {
            false
        };

        // Tick channel 4.
        let ch4_underflow = if pair_34 {
            if ch3_underflow {
                self.tick_single_channel(3)
            } else {
                false
            }
        } else if ch4_tick {
            self.tick_single_channel(3)
        } else {
            false
        };

        // High-pass filter: channel 1 is filtered by channel 3.
        if self.audctl & AUDCTL_HPF_CH1 != 0 && ch3_underflow {
            self.channels[0].hp_flipflop = self.channels[0].output;
        }

        // High-pass filter: channel 2 is filtered by channel 4.
        if self.audctl & AUDCTL_HPF_CH2 != 0 && ch4_underflow {
            self.channels[1].hp_flipflop = self.channels[1].output;
        }

        // Timer interrupts on underflow.
        if ch1_underflow {
            self.trigger_timer_irq(IRQ_TIMER1);
        }
        if ch2_underflow {
            self.trigger_timer_irq(IRQ_TIMER2);
        }
        // Timer 3 has no dedicated IRQ bit.
        if ch4_underflow {
            self.trigger_timer_irq(IRQ_TIMER4);
        }
    }

    /// Tick a single channel, returning true if it underflowed.
    fn tick_single_channel(&mut self, idx: usize) -> bool {
        let ch = &mut self.channels[idx];
        if ch.counter == 0 {
            ch.reload();
            ch.output = !ch.output;
            true
        } else {
            ch.counter -= 1;
            false
        }
    }

    /// Trigger a timer IRQ if the corresponding IRQEN bit is set.
    fn trigger_timer_irq(&mut self, mask: u8) {
        if self.irqen & mask != 0 {
            // IRQST is active-low: clear the bit to indicate pending.
            self.irqst &= !mask;
        }
    }

    /// Read 8 consecutive bits from a polynomial counter table.
    fn read_poly_byte(table: &[u8], start: usize, period: u32) -> u8 {
        let mut byte = 0u8;
        for bit in 0..8 {
            let idx = (start + bit) % (period as usize);
            byte |= table[idx] << bit;
        }
        byte
    }

    /// Mix all four channels into a single sample value.
    fn mix(&self) -> f32 {
        let mut total: u8 = 0;
        for (i, ch) in self.channels.iter().enumerate() {
            let output = if ch.volume_only() {
                // Volume-only mode: output = volume value directly.
                ch.volume()
            } else {
                // Normal mode: apply distortion/poly gating.
                let poly_gate = self.poly_gate(ch.distortion());
                let channel_active = ch.output && poly_gate;

                // Apply high-pass filter if enabled.
                let hp_active = match i {
                    0 => {
                        if self.audctl & AUDCTL_HPF_CH1 != 0 {
                            ch.output != ch.hp_flipflop
                        } else {
                            channel_active
                        }
                    }
                    1 => {
                        if self.audctl & AUDCTL_HPF_CH2 != 0 {
                            ch.output != ch.hp_flipflop
                        } else {
                            channel_active
                        }
                    }
                    _ => channel_active,
                };

                if hp_active { ch.volume() } else { 0 }
            };
            total = total.saturating_add(output);
        }

        // Max possible = 60 (4 channels x 15). Normalise to 0.0..1.0.
        // The DC-blocking filter will centre around zero.
        f32::from(total) / 60.0
    }

    /// Determine whether the polynomial counter gate is active for the
    /// given distortion field (AUDC bits 7-5).
    fn poly_gate(&self, distortion: u8) -> bool {
        let p5 = self.poly5_bit();
        let p4 = self.poly4_bit();
        let p17_or_9 = self.poly17_or_9_bit();

        match distortion {
            // $00 (000): 5-bit poly AND 17/9-bit poly
            0b000 => p5 && p17_or_9,
            // $20 (001): 5-bit poly AND 4-bit poly
            0b001 => p5 && p4,
            // $40 (010): 5-bit poly only
            // $60 (011): 5-bit poly only (duplicate)
            0b010 | 0b011 => p5,
            // $80 (100): 17/9-bit poly only
            0b100 => p17_or_9,
            // $A0, $C0, $E0 (101, 110, 111): Pure tone (no poly gating)
            _ => true,
        }
    }

    /// Current bit from the 5-bit polynomial counter.
    fn poly5_bit(&self) -> bool {
        let idx = (self.poly_counter as usize) % (POLY5_PERIOD as usize);
        self.poly5_table[idx] != 0
    }

    /// Current bit from the 4-bit polynomial counter.
    fn poly4_bit(&self) -> bool {
        let idx = (self.poly_counter as usize) % (POLY4_PERIOD as usize);
        self.poly4_table[idx] != 0
    }

    /// Current bit from the 17-bit or 9-bit polynomial counter
    /// (selected by AUDCTL bit 0).
    fn poly17_or_9_bit(&self) -> bool {
        if self.audctl & AUDCTL_POLY9 != 0 {
            let idx = (self.poly_counter as usize) % (POLY9_PERIOD as usize);
            self.poly9_table[idx] != 0
        } else {
            let idx = (self.poly_counter as usize) % (POLY17_PERIOD as usize);
            self.poly17_table[idx] != 0
        }
    }

    /// Advance the pot scanner by one scan line.
    fn tick_pot_scan(&mut self) {
        let mut all_done = true;
        for i in 0..NUM_POTS {
            if self.pot_counter[i] < self.pot_target[i] {
                self.pot_counter[i] += 1;
                if self.pot_counter[i] < self.pot_target[i] {
                    all_done = false;
                } else {
                    // This pot has reached its target — latch the value.
                    self.pot_value[i] = self.pot_counter[i];
                }
            }
        }
        // If all pots reached their targets (or exceeded POT_MAX), stop scanning.
        if all_done {
            self.pot_scanning = false;
        }
    }
}

impl Default for Pokey {
    fn default() -> Self {
        // Default to NTSC frequency.
        Self::new(1_789_772)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a POKEY at NTSC frequency.
    fn ntsc_pokey() -> Pokey {
        Pokey::new(1_789_772)
    }

    #[test]
    fn frequency_counter_countdown() {
        let mut pokey = ntsc_pokey();
        // Set channel 1 to 1.79 MHz mode so it ticks every CPU cycle.
        pokey.audctl = AUDCTL_CH1_179MHZ;
        // Set frequency divider to 5.
        pokey.write(0x00, 5); // AUDF1 = 5
        pokey.write(0x01, 0xA0); // AUDC1: pure tone, volume 0 (just testing counter)
        pokey.write(0x09, 0); // STIMER: reset counters

        // Counter should be loaded with 5.
        // Tick 5 times: counter goes 5->4->3->2->1->0.
        for _ in 0..5 {
            pokey.tick();
        }
        // After 5 ticks the counter should have reached 0 but not yet underflowed.
        // The 6th tick causes underflow and reload.
        let output_before = pokey.channels[0].output;
        pokey.tick(); // underflow: counter reloads, output toggles
        assert_ne!(
            pokey.channels[0].output, output_before,
            "Output should toggle on counter underflow"
        );
    }

    #[test]
    fn timer_interrupt_generation() {
        let mut pokey = ntsc_pokey();
        // Enable timer 1 IRQ.
        pokey.write(0x0E, IRQ_TIMER1); // IRQEN
        // Set channel 1 to 1.79 MHz, short period.
        pokey.audctl = AUDCTL_CH1_179MHZ;
        pokey.write(0x00, 2); // AUDF1 = 2
        pokey.write(0x01, 0xA0); // AUDC1: pure tone
        pokey.write(0x09, 0); // STIMER

        // Verify no IRQ pending yet.
        assert!(!pokey.irq_pending(), "No IRQ should be pending initially");

        // Tick until underflow: counter 2->1->0->underflow = 3 ticks.
        for _ in 0..3 {
            pokey.tick();
        }

        assert!(
            pokey.irq_pending(),
            "Timer 1 IRQ should be pending after underflow"
        );
        assert_eq!(
            pokey.irqst & IRQ_TIMER1,
            0,
            "IRQST bit 0 should be 0 (active low) when timer 1 fires"
        );
    }

    #[test]
    fn irqen_irqst_read_write() {
        let mut pokey = ntsc_pokey();

        // Initially IRQST = $FF (no interrupts pending).
        assert_eq!(pokey.read(0x0E), 0xFF);

        // Enable timer 1 and timer 2.
        pokey.write(0x0E, IRQ_TIMER1 | IRQ_TIMER2);
        assert_eq!(pokey.irqen, IRQ_TIMER1 | IRQ_TIMER2);

        // Disabling an IRQ clears it in IRQST.
        // First, force an IRQ pending state.
        pokey.irqst &= !IRQ_TIMER1; // Simulate timer 1 firing.
        assert!(pokey.irq_pending());

        // Now disable timer 1 — its IRQST bit should be set back to 1.
        pokey.write(0x0E, IRQ_TIMER2); // Only timer 2 enabled.
        assert_eq!(
            pokey.irqst | IRQ_TIMER1,
            pokey.irqst,
            "Disabled IRQ bits should be cleared (set to 1) in IRQST"
        );
    }

    #[test]
    fn pot_value_read() {
        let mut pokey = ntsc_pokey();

        // Set pot 0 target to 100.
        pokey.set_pot(0, 100);
        // Start pot scan.
        pokey.write(0x0B, 0); // POTGO

        // ALLPOT should indicate pot 0 is still scanning.
        assert_ne!(pokey.read(0x08) & 0x01, 0, "Pot 0 should still be scanning");

        // Tick enough for the scan to complete (100 scan lines * 114 cycles each).
        for _ in 0..(100 * u32::from(DIVIDER_15KHZ)) {
            pokey.tick();
        }

        // Pot 0 should now be latched at 100.
        assert_eq!(pokey.read(0x00), 100, "POT0 should read 100");
    }

    #[test]
    fn random_register_produces_nonzero() {
        let mut pokey = ntsc_pokey();

        // Tick a few hundred cycles to advance the poly counter.
        for _ in 0..500 {
            pokey.tick();
        }

        let random = pokey.read(0x0A);
        // The poly counter is seeded with all-ones and produces a deterministic
        // LFSR sequence. After 500 ticks it should not be all-zero.
        // (Testing exact value is fragile, but testing non-zero is safe since
        // the LFSR never reaches the all-zero state.)
        assert_ne!(random, 0, "RANDOM should produce non-zero values");
    }

    #[test]
    fn audctl_base_clock_selection() {
        let mut pokey = ntsc_pokey();

        // Default: 64 kHz base clock. Divider period = 28.
        assert_eq!(pokey.audctl & AUDCTL_15KHZ, 0);

        // Set 15 kHz mode.
        pokey.write(0x08, AUDCTL_15KHZ);
        assert_ne!(pokey.audctl & AUDCTL_15KHZ, 0);

        // Set channel 1 with short period and pure tone.
        pokey.write(0x00, 1); // AUDF1 = 1
        pokey.write(0x01, 0xAF); // AUDC1: pure tone, volume 15
        pokey.write(0x09, 0); // STIMER

        let output_before = pokey.channels[0].output;

        // In 15 kHz mode, the base clock ticks every 114 CPU cycles.
        // With AUDF=1, the channel counter counts 1->0->underflow = 2 base ticks.
        // So output should toggle after 2 * 114 = 228 CPU cycles.
        for _ in 0..227 {
            pokey.tick();
        }
        // Should NOT have toggled yet.
        assert_eq!(
            pokey.channels[0].output, output_before,
            "Channel should not toggle before 228 ticks in 15 kHz mode"
        );

        pokey.tick(); // 228th tick: second base clock -> underflow -> toggle.
        assert_ne!(
            pokey.channels[0].output, output_before,
            "Channel should toggle at 228 ticks in 15 kHz mode"
        );
    }

    #[test]
    fn volume_only_mode_output() {
        let mut pokey = ntsc_pokey();

        // Set channel 1 to volume-only mode with volume = 10.
        // Bit 4 = volume-only, bits 3-0 = volume.
        pokey.write(0x01, 0x1A); // AUDC1: volume-only, vol=10

        // In volume-only mode, the channel outputs the volume value directly,
        // independent of frequency counter or poly counters.
        // Mix should produce a non-zero sample.
        let sample = pokey.mix();
        assert!(
            sample > 0.0,
            "Volume-only mode should produce non-zero output, got {sample}"
        );

        // Set volume to 0 — output should be 0.
        pokey.write(0x01, 0x10); // Volume-only, vol=0
        let sample = pokey.mix();
        assert!(
            (sample - 0.0).abs() < f32::EPSILON,
            "Volume-only with vol=0 should produce zero output"
        );
    }

    #[test]
    fn audio_buffer_fills_on_tick() {
        let mut pokey = ntsc_pokey();

        // Set a channel to produce sound.
        pokey.write(0x00, 10); // AUDF1
        pokey.write(0x01, 0xAF); // AUDC1: pure tone, volume 15
        pokey.write(0x09, 0); // STIMER

        // Tick enough cycles to produce at least one 48 kHz sample.
        // At 1.789 MHz, one sample at 48 kHz is ~37.3 ticks.
        for _ in 0..100 {
            pokey.tick();
        }

        let len = pokey.buffer_len();
        assert!(len > 0, "Buffer should have samples after ticking");

        let buf = pokey.take_buffer();
        assert_eq!(buf.len(), len);
        assert_eq!(pokey.buffer_len(), 0, "Buffer should be empty after take");
    }

    #[test]
    fn poly_tables_have_correct_periods() {
        let pokey = ntsc_pokey();
        assert_eq!(pokey.poly4_table.len(), POLY4_PERIOD as usize);
        assert_eq!(pokey.poly5_table.len(), POLY5_PERIOD as usize);
        assert_eq!(pokey.poly9_table.len(), POLY9_PERIOD as usize);
        assert_eq!(pokey.poly17_table.len(), POLY17_PERIOD as usize);
    }

    #[test]
    fn sixteen_bit_mode_pairs_channels() {
        let mut pokey = ntsc_pokey();

        // Enable 16-bit mode for channels 1+2 and 1.79 MHz for channel 1.
        pokey.audctl = AUDCTL_16BIT_CH12 | AUDCTL_CH1_179MHZ;

        // Set AUDF1 = 3 (low byte), AUDF2 = 0 (high byte).
        // Effective 16-bit period = 3 for channel 1.
        pokey.write(0x00, 3); // AUDF1
        pokey.write(0x02, 0); // AUDF2
        pokey.write(0x03, 0xAF); // AUDC2: pure tone, volume 15
        pokey.write(0x09, 0); // STIMER

        // Channel 2 should only tick when channel 1 underflows.
        // Channel 1 underflows every 4 CPU cycles (AUDF=3: 3->2->1->0->underflow).
        let output_before = pokey.channels[1].output;
        for _ in 0..3 {
            pokey.tick();
        }
        assert_eq!(
            pokey.channels[1].output, output_before,
            "Channel 2 should not tick until channel 1 underflows"
        );

        pokey.tick(); // Channel 1 underflows, clocks channel 2.
        // Channel 2 counter was loaded from AUDF2 (0), so it underflows immediately.
        // (counter 0 -> underflow on first clock -> toggle)
        // The exact toggle depends on counter state, but channel 2 should have been clocked.
    }

    #[test]
    fn stimer_resets_all_counters() {
        let mut pokey = ntsc_pokey();

        pokey.write(0x00, 100); // AUDF1
        pokey.write(0x02, 200); // AUDF2
        pokey.write(0x04, 50); // AUDF3
        pokey.write(0x06, 75); // AUDF4

        pokey.write(0x09, 0); // STIMER: reset all counters

        assert_eq!(pokey.channels[0].counter, 100);
        assert_eq!(pokey.channels[1].counter, 200);
        assert_eq!(pokey.channels[2].counter, 50);
        assert_eq!(pokey.channels[3].counter, 75);
    }

    #[test]
    fn skres_resets_serial_status() {
        let mut pokey = ntsc_pokey();
        pokey.skstat = 0x00; // Simulate some status bits being set.
        pokey.write(0x0A, 0); // SKRES
        assert_eq!(pokey.skstat, 0xFF, "SKRES should reset SKSTAT to $FF");
    }

    #[test]
    fn default_creates_ntsc_pokey() {
        let pokey = Pokey::default();
        assert_eq!(pokey.cpu_freq, 1_789_772);
    }
}
