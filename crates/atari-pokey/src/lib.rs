//! Atari POKEY (Potentiometer and Keyboard) chip emulator.
//!
//! Adapted from `Emu198x-Oldest/crates/atari-pokey` (port 2026-06-01) for
//! the Atari 5200 / 7800 / 800XL / 130XE family. Self-contained, no
//! external chip dependencies.
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

use serde::{Deserialize, Serialize};

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
// Serial IRQ bits (POKEY IRQST/IRQEN layout):
//   bit 3 = serial output transmission finished (shift register empty)
//   bit 4 = serial output data needed (holding register empty)
//   bit 5 = serial input data ready
// Both output IRQs are edge events: each asserts (bit → 0) once when its
// stage completes, and the CPU clears it by toggling IRQEN (which sets the
// IRQST bit back to 1 — see the IRQEN write below). They must NOT be held
// asserted, or the OS IRQ dispatcher — which services bit 4 at a higher
// priority than bit 3 — would loop on bit 4 forever and never finish a frame.
const IRQ_SEROUT_DONE: u8 = 0x08;
const IRQ_SEROUT_NEEDED: u8 = 0x10;
const IRQ_SERIN_READY: u8 = 0x20;
// bit 6 = keyboard key pressed.
const IRQ_KEY: u8 = 0x40;
/// bit 7 = the Break key. Nothing asserts it: Break is wired to POKEY's own
/// pin on real hardware, and the machines drive the keyboard through KBCODE.
#[allow(dead_code)]
const IRQ_BREAK: u8 = 0x80;

/// SKSTAT bit 2 — "last key still pressed" (active low: 0 = a key is held).
const SKSTAT_KEY_DOWN: u8 = 0x04;

/// SKSTAT bit 3 — Shift key down (active low: 0 = Shift is held).
const SKSTAT_SHIFT: u8 = 0x08;

/// SKSTAT bit 1 — the serial input shift register is active (active low). It
/// goes low when the start bit is sampled and high again on the stop bit, so
/// on back-to-back bytes it is low nine bit cells in ten rather than
/// continuously.
const SKSTAT_SERIN_BUSY: u8 = 0x02;

/// SKSTAT bit 5 — a byte arrived while the previous one was still unread
/// (active low). Set when the serial port tries to raise the input IRQ and
/// finds it already raised.
const SKSTAT_SERIN_OVERRUN: u8 = 0x20;

/// KBCODE bit 6 — Shift was down when the key was pressed. Bit 7, the one
/// above it, is Control.
const KBCODE_SHIFT: u8 = 0x40;

/// A serial frame is ten bit cells: a start bit, eight data bits, and a stop
/// bit.
const SERIAL_FRAME_BITS: u8 = 10;

/// Which timer channel clocks the serial *output* shift register, from
/// SKCTL bits 6-4. Modes 010, 011, 100 and 101 take channel 4; 110 and 111
/// take channel 2; 000 and 001 take an external clock, which nothing on the
/// SIO bus drives here. (*Altirra Hardware Reference Manual*, Table 10.)
fn serial_output_clock(skctl: u8) -> Option<usize> {
    match (skctl >> 4) & 0x07 {
        0b010..=0b101 => Some(3),
        0b110 | 0b111 => Some(1),
        _ => None,
    }
}

/// Which timer channel clocks the serial *input* shift register. Only channel
/// 4 can reach it — either alone or as the high half of the linked 3+4 pair
/// the asynchronous modes use — so every mode but the two external-clock ones
/// lands on the same counter. (*Altirra Hardware Reference Manual*, Table 10.)
fn serial_input_clock(skctl: u8) -> Option<usize> {
    match (skctl >> 4) & 0x07 {
        0b000 | 0b100 => None,
        _ => Some(3),
    }
}

// AUDCTL bit masks. The register reads `PLY CH1 CH3 L12 L34 HP1 HP2 15K` from
// bit 7 down, so the polynomial select is the *high* bit and the clock select
// the low one — the opposite of the order they are listed in.
const AUDCTL_POLY9: u8 = 0x80;
const AUDCTL_CH1_179MHZ: u8 = 0x40;
const AUDCTL_CH3_179MHZ: u8 = 0x20;
const AUDCTL_16BIT_CH12: u8 = 0x10;
const AUDCTL_16BIT_CH34: u8 = 0x08;
const AUDCTL_HPF_CH1: u8 = 0x04;
const AUDCTL_HPF_CH2: u8 = 0x02;
const AUDCTL_15KHZ: u8 = 0x01;

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
#[derive(Serialize, Deserialize)]
struct Channel {
    /// Frequency divider register (AUDF).
    audf: u8,
    /// Audio control register (AUDC).
    audc: u8,
    /// Current frequency counter (counts down).
    counter: u32,
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
        self.counter = u32::from(self.audf);
    }

    /// Reload the counter for 16-bit paired mode (high byte from partner).
    fn reload_16bit(&mut self, high_byte: u8) {
        self.counter = u32::from(self.audf) | (u32::from(high_byte) << 8);
    }
}

// ---------------------------------------------------------------------------
// POKEY
// ---------------------------------------------------------------------------

/// Atari POKEY chip.
#[derive(Serialize, Deserialize)]
pub struct Pokey {
    /// CPU clock frequency (Hz), e.g. `1_789_772` for NTSC. Used to
    /// downsample the audio; the serial port takes its rate from the timers
    /// rather than from here.
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

    /// Whether SEROUT holds a byte waiting to be loaded into the output shift
    /// register. Writing SEROUT queues; the shift register takes it on the
    /// next serial clock edge on which it is idle, and only then does the
    /// "output ready" IRQ (bit 4) assert.
    serout_pending: bool,

    /// Bit cells left in the frame the output shift register is sending, or
    /// zero when it is idle. Idle is what the "output complete" IRQ (bit 3)
    /// reports — see [`Pokey::serial_output_complete`].
    serial_out_bits: u8,

    /// The divide-by-two flip-flop between the clocking timer and the serial
    /// port. The shift register acts on the rising edge, so one bit cell is
    /// two timer underflows.
    serial_out_clock_phase: bool,

    /// The byte the output shift register is sending, and the byte it has
    /// finished sending and not yet handed to the bus.
    serial_out_shifting: u8,
    serial_out_sent: Option<u8>,

    /// The byte the input shift register is receiving, the bit cells left in
    /// its frame, and its own divide-by-two flip-flop. Zero bits left means
    /// the register is idle and can take another byte.
    serial_in_shifting: u8,
    serial_in_bits: u8,
    serial_in_clock_phase: bool,

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

    // -- Audio output (host-only sample drain — not chip state) --
    /// Accumulator for downsampling (mixed output).
    #[serde(skip)]
    accumulator: f32,

    /// Per-channel accumulators for downsampling.
    #[serde(skip)]
    channel_accumulators: [f32; 4],

    /// Number of CPU ticks accumulated.
    #[serde(skip)]
    sample_count: u32,

    /// Fractional output-sample clock, in SAMPLE_RATE units.  Carrying the
    /// remainder alternates 37- and 38-tick windows as required instead of
    /// rounding every window up to 38 ticks (~47.1 kHz on NTSC machines).
    #[serde(skip)]
    sample_phase: u32,

    /// Output sample buffer at 48 kHz.
    #[serde(skip)]
    buffer: Vec<f32>,

    /// Per-channel output buffers at 48 kHz.
    #[serde(skip)]
    channel_buffers: [Vec<f32>; 4],

    /// DC-blocking high-pass filter state.
    #[serde(skip)]
    hp_prev_in: f32,
    #[serde(skip)]
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
            channels: [
                Channel::new(),
                Channel::new(),
                Channel::new(),
                Channel::new(),
            ],
            audctl: 0,
            irqen: 0,
            irqst: 0xFF,
            skctl: 0,
            skstat: 0xFF,
            serin: 0,
            serout: 0,
            serout_pending: false,
            serial_out_bits: 0,
            serial_out_clock_phase: false,
            serial_out_shifting: 0,
            serial_out_sent: None,
            serial_in_shifting: 0,
            serial_in_bits: 0,
            serial_in_clock_phase: false,
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
            channel_accumulators: [0.0; 4],
            sample_count: 0,
            sample_phase: 0,
            buffer: Vec::with_capacity(SAMPLE_RATE as usize / 50 + 1),
            channel_buffers: [
                Vec::with_capacity(SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(SAMPLE_RATE as usize / 50 + 1),
            ],
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
        let (sample, per_channel) = self.mix_with_channels();
        self.accumulator += sample;
        for (i, sample) in per_channel.iter().enumerate() {
            self.channel_accumulators[i] += *sample;
        }
        self.sample_count += 1;

        self.sample_phase += SAMPLE_RATE;
        if self.sample_phase >= self.cpu_freq {
            self.sample_phase -= self.cpu_freq;
            let count = self.sample_count as f32;
            let avg = self.accumulator / count;

            // DC-blocking high-pass filter.
            // y[n] = alpha * (y[n-1] + x[n] - x[n-1]), alpha ~= 0.9952 (~37 Hz at 48 kHz)
            const ALPHA: f32 = 0.9952;
            let filtered = ALPHA * (self.hp_prev_out + avg - self.hp_prev_in);
            self.hp_prev_in = avg;
            self.hp_prev_out = filtered;

            self.buffer.push(filtered);

            // Emit per-channel downsampled averages (no DC filter — channels
            // are already centred around their own mean).
            for i in 0..4 {
                self.channel_buffers[i].push(self.channel_accumulators[i] / count);
                self.channel_accumulators[i] = 0.0;
            }

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
            0x0E => self.irqst_live(),

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
                if self.audctl & AUDCTL_16BIT_CH12 != 0 {
                    let high = self.channels[1].audf;
                    self.channels[0].reload_16bit(high);
                    if self.audctl & AUDCTL_CH1_179MHZ != 0 {
                        self.channels[0].counter += 6;
                    }
                }
                if self.audctl & AUDCTL_16BIT_CH34 != 0 {
                    let high = self.channels[3].audf;
                    self.channels[2].reload_16bit(high);
                    if self.audctl & AUDCTL_CH3_179MHZ != 0 {
                        self.channels[2].counter += 6;
                    }
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

            // SEROUT: serial output data. Writing queues the byte; the shift
            // register loads it on its next idle clock edge, and only that
            // load asserts "output ready". Writing again before the load
            // replaces the queued byte, and the first one is never sent.
            0x0D => {
                self.serout = value;
                self.serout_pending = true;
            }

            // IRQEN: interrupt enable mask.
            // Writing also clears corresponding bits in IRQST for disabled IRQs.
            0x0E => {
                self.irqen = value;
                // Disabled interrupts are immediately cleared in IRQST (set to 1 = not pending).
                self.irqst |= !value;
            }

            // SKCTL: serial port control. Clearing bits 0-1 selects
            // initialisation mode, which interrupts whatever the shift
            // registers are doing and flushes the byte queued in SEROUT.
            // Selecting an external clock for both directions (bits 6-4 =
            // %000) resets the clock flip-flops.
            0x0F => {
                self.skctl = value;
                if value & 0x03 == 0 {
                    self.serial_out_bits = 0;
                    self.serout_pending = false;
                }
                if (value >> 4) & 0x07 == 0 {
                    self.serial_out_clock_phase = false;
                }
            }

            _ => {}
        }
    }

    /// Drain the audio output buffer. Returns mono f32 samples at 48 kHz,
    /// in the range -1.0 to 1.0.
    pub fn take_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    /// Drain the per-channel audio buffers. Returns four `Vec<f32>` at
    /// 48 kHz, one per POKEY channel (0-3), in the range 0.0 to 1.0.
    /// Each buffer has the same length as the corresponding `take_buffer()`
    /// output (they are filled in lockstep).
    pub fn take_channel_buffers(&mut self) -> [Vec<f32>; 4] {
        [
            std::mem::take(&mut self.channel_buffers[0]),
            std::mem::take(&mut self.channel_buffers[1]),
            std::mem::take(&mut self.channel_buffers[2]),
            std::mem::take(&mut self.channel_buffers[3]),
        ]
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

    /// Read the current pot target value for observation.
    #[must_use]
    pub fn pot(&self, index: u8) -> u8 {
        if (index as usize) < NUM_POTS {
            self.pot_target[index as usize]
        } else {
            0
        }
    }

    /// Returns true if any enabled interrupt is pending.
    #[must_use]
    pub fn irq_pending(&self) -> bool {
        // IRQST is active-low (0 = pending). IRQEN selects which are enabled.
        let irqst = self.irqst_live();
        (irqst & self.irqen) != self.irqen
    }

    /// Set the keyboard code register (written by external keyboard controller).
    pub fn set_kbcode(&mut self, code: u8) {
        self.kbcode = code;
    }

    /// Press a key. `code` is the POKEY keyboard scan code in bits 0-5, with
    /// bit 6 = Shift and bit 7 = Ctrl. Latches KBCODE, raises the keyboard
    /// interrupt (IRQST bit 6) — which the OS handler reads and converts to
    /// ATASCII — and marks "key down" in SKSTAT (bit 2 low). The interrupt is
    /// edge-triggered: it asserts once here and the CPU clears it by toggling
    /// IRQEN, exactly like the serial-output IRQs.
    ///
    /// SKSTAT bit 3 follows the code's Shift bit. On hardware that bit tracks
    /// the Shift key itself and moves even with no other key pressed; a host
    /// that sends whole characters has no separate Shift key to track, so the
    /// state is taken from the character being typed and released with it.
    pub fn press_key(&mut self, code: u8) {
        self.kbcode = code;
        self.irqst &= !IRQ_KEY; // keyboard IRQ pending
        self.skstat &= !SKSTAT_KEY_DOWN; // a key is held
        if code & KBCODE_SHIFT == 0 {
            self.skstat |= SKSTAT_SHIFT;
        } else {
            self.skstat &= !SKSTAT_SHIFT;
        }
    }

    /// Release the currently held key — clears the "last key still pressed"
    /// status (SKSTAT bit 2 high). KBCODE retains its last value, as on
    /// hardware.
    pub fn release_key(&mut self) {
        self.skstat |= SKSTAT_KEY_DOWN | SKSTAT_SHIFT;
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
        self.irqst_live()
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

    /// Per-channel AUDF (frequency) registers, channels 0-3 (diagnostics).
    #[must_use]
    pub fn audf(&self) -> [u8; 4] {
        [
            self.channels[0].audf,
            self.channels[1].audf,
            self.channels[2].audf,
            self.channels[3].audf,
        ]
    }

    /// Per-channel AUDC (control) registers, channels 0-3 (diagnostics).
    #[must_use]
    pub fn audc(&self) -> [u8; 4] {
        [
            self.channels[0].audc,
            self.channels[1].audc,
            self.channels[2].audc,
            self.channels[3].audc,
        ]
    }

    /// Get the SKCTL register value (serial / keyboard control; diagnostics).
    #[must_use]
    pub fn skctl(&self) -> u8 {
        self.skctl
    }

    /// Get the SKSTAT register value (serial / keyboard status; diagnostics).
    #[must_use]
    pub fn skstat(&self) -> u8 {
        self.skstat
    }

    /// Get the KBCODE register value (last keyboard scan code; diagnostics).
    #[must_use]
    pub fn kbcode(&self) -> u8 {
        self.kbcode
    }

    /// Serialize POKEY register state for save states.
    ///
    /// Captures channel registers, AUDCTL, IRQ, serial, and pot state.
    /// Does not include poly tables (deterministic) or audio buffers (transient).
    #[must_use]
    pub fn save_state(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(48);
        // Channel registers
        for ch in &self.channels {
            data.push(ch.audf);
            data.push(ch.audc);
            data.extend_from_slice(&ch.counter.to_le_bytes());
            data.push(u8::from(ch.output));
            data.push(u8::from(ch.hp_flipflop));
        }
        data.push(self.audctl);
        data.push(self.irqen);
        data.push(self.irqst);
        data.push(self.skctl);
        data.push(self.skstat);
        data.push(self.serin);
        data.push(self.serout);
        data.push(self.kbcode);
        // Poly counter
        data.extend_from_slice(&self.poly_counter.to_le_bytes());
        // Base divider
        data.extend_from_slice(&self.base_divider.to_le_bytes());
        // Pot state
        data.extend_from_slice(&self.pot_target);
        data.extend_from_slice(&self.pot_value);
        data.push(u8::from(self.pot_scanning));
        data
    }

    /// Restore POKEY state from a save state.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is too short.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        if data.len() < 32 + 14 + 17 {
            return Err("POKEY state truncated".into());
        }
        let mut p = 0;
        for ch in &mut self.channels {
            ch.audf = data[p];
            p += 1;
            ch.audc = data[p];
            p += 1;
            ch.counter = u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]);
            p += 4;
            ch.output = data[p] != 0;
            p += 1;
            ch.hp_flipflop = data[p] != 0;
            p += 1;
        }
        self.audctl = data[p];
        p += 1;
        self.irqen = data[p];
        p += 1;
        self.irqst = data[p];
        p += 1;
        self.skctl = data[p];
        p += 1;
        self.skstat = data[p];
        p += 1;
        self.serin = data[p];
        p += 1;
        self.serout = data[p];
        p += 1;
        self.kbcode = data[p];
        p += 1;
        self.poly_counter = u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]);
        p += 4;
        self.base_divider = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.pot_target.copy_from_slice(&data[p..p + 8]);
        p += 8;
        self.pot_value.copy_from_slice(&data[p..p + 8]);
        p += 8;
        self.pot_scanning = data[p] != 0;
        p += 1;
        Ok(p)
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

        // 16-bit mode: ch1+ch2 paired, ch3+ch4 paired. The low and high AUDF
        // bytes form one divider; modelling them as two reloading 8-bit
        // counters would multiply their periods instead.
        let pair_12 = self.audctl & AUDCTL_16BIT_CH12 != 0;
        let pair_34 = self.audctl & AUDCTL_16BIT_CH34 != 0;

        // Tick channel 1.
        let (ch1_underflow, ch2_underflow) = if pair_12 && ch1_tick {
            self.tick_linked_pair(0, 1)
        } else if ch1_tick {
            (self.tick_single_channel(0), false)
        } else {
            (false, false)
        };

        // Tick channel 2 independently only when it is not the high byte of
        // a linked pair.
        let ch2_underflow = if pair_12 {
            ch2_underflow
        } else if ch2_tick {
            self.tick_single_channel(1)
        } else {
            false
        };

        // Tick channel 3, or the combined 3+4 divider.
        let (ch3_underflow, ch4_underflow) = if pair_34 && ch3_tick {
            self.tick_linked_pair(2, 3)
        } else if ch3_tick {
            (self.tick_single_channel(2), false)
        } else {
            (false, false)
        };

        // Tick channel 4 independently only when it is not linked.
        let ch4_underflow = if pair_34 {
            ch4_underflow
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

        // The serial port is clocked by a divide-by-two flip-flop hanging off
        // the selected timer, so the shift register sees one edge for every
        // two underflows.
        let clocked = match serial_output_clock(self.skctl) {
            Some(1) => ch2_underflow,
            Some(3) => ch4_underflow,
            _ => false,
        };
        if clocked {
            self.serial_out_clock_phase = !self.serial_out_clock_phase;
            if self.serial_out_clock_phase {
                self.tick_serial_output();
            }
        }

        if serial_input_clock(self.skctl) == Some(3) && ch4_underflow {
            self.serial_in_clock_phase = !self.serial_in_clock_phase;
            if self.serial_in_clock_phase {
                self.tick_serial_input();
            }
        }
    }

    /// One bit cell of the serial input shift register.
    ///
    /// A frame is ten bit cells; on the last one the byte lands in SERIN and
    /// the input-ready IRQ is raised. Raising it when it is already raised
    /// means the CPU never read the previous byte, which is the overrun
    /// SKSTAT bit 5 reports — and the new byte replaces the old one either
    /// way.
    fn tick_serial_input(&mut self) {
        if self.serial_in_bits == 0 {
            return;
        }
        self.serial_in_bits -= 1;
        if self.serial_in_bits == 0 {
            if self.irqst & IRQ_SERIN_READY == 0 {
                self.skstat &= !SKSTAT_SERIN_OVERRUN;
            }
            self.serin = self.serial_in_shifting;
            self.irqst &= !IRQ_SERIN_READY;
            self.skstat |= SKSTAT_SERIN_BUSY;
        }
    }

    /// One bit cell of the serial output shift register, on the rising edge of
    /// the serial clock.
    ///
    /// A frame is ten bit cells. When the register finishes one and a byte is
    /// waiting in SEROUT, it loads immediately and asserts the "output ready"
    /// IRQ (bit 4) to say SEROUT is free again — which is why the first byte
    /// of a transmission has to be written without waiting for that IRQ, and
    /// why there is always a bit cell of delay between writing SEROUT and
    /// anything being observable.
    fn tick_serial_output(&mut self) {
        if self.serial_out_bits > 0 {
            self.serial_out_bits -= 1;
            if self.serial_out_bits == 0 {
                // The stop bit has gone out, so the byte is on the wire.
                self.serial_out_sent = Some(self.serial_out_shifting);
            }
        }
        if self.serial_out_bits == 0 && self.serout_pending {
            self.serout_pending = false;
            self.serial_out_shifting = self.serout;
            self.serial_out_bits = SERIAL_FRAME_BITS;
            self.irqst &= !IRQ_SEROUT_NEEDED;
        }
    }

    /// Take the byte the output shift register has finished sending, if any.
    ///
    /// This is POKEY's end of the SIO bus: the machine polls it and hands
    /// whatever comes out to the devices listening on DATA OUT. A byte is
    /// offered once.
    pub fn take_serial_output(&mut self) -> Option<u8> {
        self.serial_out_sent.take()
    }

    /// Whether the input shift register can take another byte.
    #[must_use]
    pub fn serial_input_idle(&self) -> bool {
        self.serial_in_bits == 0
    }

    /// Start shifting a byte in from DATA IN. It arrives in SERIN ten bit
    /// cells later, at the input clock's rate, and raises the input-ready IRQ
    /// then — not now.
    ///
    /// Offering a byte while the register is still busy does nothing, which is
    /// what happens on the bus: a device transmitting over another device's
    /// byte garbles it rather than queuing behind it.
    pub fn begin_serial_input(&mut self, byte: u8) {
        if self.serial_in_bits != 0 {
            return;
        }
        self.serial_in_shifting = byte;
        self.serial_in_bits = SERIAL_FRAME_BITS;
        // The asynchronous receive modes resync their bit clock to the start
        // bit rather than free-running with the transmitter, so the frame is
        // ten whole bit cells from here. The timer underneath keeps its own
        // phase, which is why the first cell can be short.
        self.serial_in_clock_phase = false;
        self.skstat &= !SKSTAT_SERIN_BUSY;
    }

    /// Whether the output shift register is idle, which is what the "output
    /// complete" IRQ (bit 3) reports.
    ///
    /// Unlike every other POKEY interrupt this one is a level, not a latch: it
    /// stays asserted while the register is idle even if IRQEN has it masked
    /// off, and deasserts on its own when shifting starts. Latching it is what
    /// made the OS's send loop depend on where the CPU happened to be when the
    /// edge went by.
    fn serial_output_complete(&self) -> bool {
        self.serial_out_bits == 0
    }

    /// IRQST as software sees it: the latched bits, with the level-driven
    /// "output complete" bit folded in.
    fn irqst_live(&self) -> u8 {
        if self.serial_output_complete() {
            self.irqst & !IRQ_SEROUT_DONE
        } else {
            self.irqst | IRQ_SEROUT_DONE
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

    /// Tick a linked 16-bit pair, returning `(low_borrow, full_underflow)`.
    ///
    /// POKEY concatenates the two AUDF bytes into one little-endian divider.
    /// The low channel still produces a borrow when its byte wraps; the high
    /// channel toggles only when the complete 16-bit value underflows.
    fn tick_linked_pair(&mut self, low_idx: usize, high_idx: usize) -> (bool, bool) {
        let high_audf = self.channels[high_idx].audf;
        let counter = self.channels[low_idx].counter;
        let low_borrow = counter & 0x00FF == 0;

        if counter == 0 {
            self.channels[low_idx].reload_16bit(high_audf);
            // A linked pair clocked directly at 1.79 MHz has six additional
            // propagation cycles.  Atari800's POKEY reference expresses the
            // resulting period as `AUDF2*256 + AUDF1 + 7`; `reload_16bit`
            // and the zero-inclusive countdown already account for the +1.
            // Base-clocked linked pairs use the ordinary +1 period.
            let high_speed = (low_idx == 0 && self.audctl & AUDCTL_CH1_179MHZ != 0)
                || (low_idx == 2 && self.audctl & AUDCTL_CH3_179MHZ != 0);
            if high_speed {
                self.channels[low_idx].counter += 6;
            }
            self.channels[low_idx].output = !self.channels[low_idx].output;
            self.channels[high_idx].output = !self.channels[high_idx].output;
            (true, true)
        } else {
            self.channels[low_idx].counter -= 1;
            if low_borrow {
                self.channels[low_idx].output = !self.channels[low_idx].output;
            }
            (low_borrow, false)
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

    /// Compute per-channel output levels and return (mixed_total, [ch0, ch1, ch2, ch3]).
    ///
    /// The mixed total is the same value that `mix()` produces. Per-channel
    /// values are normalised to 0.0..1.0 (each channel's max is 15/15 = 1.0).
    fn mix_with_channels(&self) -> (f32, [f32; 4]) {
        let mut total: u8 = 0;
        let mut per_channel = [0.0f32; 4];
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
            per_channel[i] = f32::from(output) / 15.0;
            total = total.saturating_add(output);
        }

        // Max possible = 60 (4 channels x 15). Normalise to 0.0..1.0.
        // The DC-blocking filter will centre around zero.
        (f32::from(total) / 60.0, per_channel)
    }

    /// Mix all four channels into a single sample value.
    #[allow(dead_code)]
    fn mix(&self) -> f32 {
        self.mix_with_channels().0
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
            // $20 (001): 5-bit poly only
            0b001 => p5,
            // $40 (010): 5-bit poly AND 4-bit poly
            0b010 => p5 && p4,
            // $60 (011): 5-bit poly only (duplicate of $20)
            0b011 => p5,
            // $80 (100): 17/9-bit poly only
            0b100 => p17_or_9,
            // $A0 (101): Pure tone (no poly gating)
            0b101 => true,
            // $C0 (110): 4-bit poly only
            0b110 => p4,
            // $E0 (111): Pure tone (no poly gating)
            0b111 => true,
            _ => unreachable!("distortion is a three-bit field"),
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

    /// Set POKEY up the way the 800XL OS sets it up to talk to a disk drive:
    /// channels 3+4 linked on the 1.79 MHz clock with divisor $0028, and the
    /// serial port clocked from channel 4.
    fn sio_configured_pokey() -> Pokey {
        let mut pokey = ntsc_pokey();
        pokey.write(0x08, 0x28); // AUDCTL: channel 3 fast clock + 3&4 link
        pokey.write(0x04, 0x28); // AUDF3 — divisor low
        pokey.write(0x06, 0x00); // AUDF4 — divisor high
        pokey.write(0x0F, 0x23); // SKCTL: transmit clocked from channel 4
        pokey
    }

    /// How many CPU cycles the shift register takes over a whole frame, which
    /// is ten bit cells.
    fn cycles_for_one_serial_frame(pokey: &mut Pokey) -> u32 {
        pokey.write(0x0D, 0x55); // SEROUT
        let mut cycles = 0;
        // The load itself takes a bit cell; count from there to idle again.
        while pokey.serial_out_bits == 0 {
            pokey.tick();
            cycles += 1;
            assert!(cycles < 100_000, "the shift register never loaded");
        }
        let mut frame = 0;
        while pokey.serial_out_bits > 0 {
            pokey.tick();
            frame += 1;
            assert!(frame < 100_000, "the frame never finished");
        }
        frame
    }

    /// A linked 1.79 MHz timer's period is its divisor plus seven, and the
    /// serial clock is half the timer, so divisor $0028 gives a 94-cycle bit
    /// cell — 19040 baud, the rate the manual states the SIO bus actually
    /// runs at. Ten bit cells make the frame.
    #[test]
    fn serial_output_runs_at_the_documented_sio_baud_rate() {
        let mut pokey = sio_configured_pokey();
        let frame = cycles_for_one_serial_frame(&mut pokey);

        assert_eq!(frame, 940, "ten 94-cycle bit cells");
        let baud = 1_789_772 / (frame / u32::from(SERIAL_FRAME_BITS));
        assert_eq!(baud, 19_040);
    }

    /// "Output complete" is a level, not a latch: it reports that the shift
    /// register is idle, stays asserted while it is even with IRQEN masking
    /// it off, and deasserts on its own when shifting starts. Latching it made
    /// the OS's send loop depend on where the CPU was when the edge went past.
    #[test]
    fn output_complete_reports_an_idle_shift_register() {
        let mut pokey = sio_configured_pokey();
        assert_eq!(pokey.read(0x0E) & IRQ_SEROUT_DONE, 0, "idle at reset");

        // Masking it off in IRQEN does not clear it.
        pokey.write(0x0E, 0x00);
        assert_eq!(pokey.read(0x0E) & IRQ_SEROUT_DONE, 0);

        pokey.write(0x0D, 0x55);
        while pokey.serial_out_bits == 0 {
            pokey.tick();
        }
        assert_eq!(
            pokey.read(0x0E) & IRQ_SEROUT_DONE,
            IRQ_SEROUT_DONE,
            "deasserts once shifting starts"
        );

        while pokey.serial_out_bits > 0 {
            pokey.tick();
        }
        assert_eq!(pokey.read(0x0E) & IRQ_SEROUT_DONE, 0, "idle again");
    }

    /// "Output ready" asserts when the shift register loads from SEROUT, not
    /// when SEROUT is written — which is why the first byte of a transmission
    /// has to be sent without waiting for it.
    #[test]
    fn output_ready_asserts_on_the_load_not_on_the_write() {
        let mut pokey = sio_configured_pokey();
        pokey.write(0x0D, 0x55);
        assert_eq!(
            pokey.read(0x0E) & IRQ_SEROUT_NEEDED,
            IRQ_SEROUT_NEEDED,
            "nothing has loaded yet"
        );

        while pokey.serial_out_bits == 0 {
            pokey.tick();
        }
        assert_eq!(pokey.read(0x0E) & IRQ_SEROUT_NEEDED, 0, "SEROUT is free");
    }

    /// Only one byte can be queued. A second write before the shift register
    /// takes the first replaces it, and the first is never sent.
    #[test]
    fn a_second_serout_write_replaces_the_byte_waiting_to_load() {
        let mut pokey = sio_configured_pokey();
        pokey.write(0x0D, 0x11);
        pokey.write(0x0D, 0x22);
        assert_eq!(pokey.serout, 0x22);

        while pokey.serial_out_bits == 0 {
            pokey.tick();
        }
        assert!(!pokey.serout_pending, "one load consumed both writes");
    }

    /// Clearing SKCTL bits 0-1 selects initialisation mode, which interrupts
    /// whatever is shifting and flushes the byte queued in SEROUT.
    #[test]
    fn initialisation_mode_flushes_the_output_path() {
        let mut pokey = sio_configured_pokey();
        pokey.write(0x0D, 0x55);
        while pokey.serial_out_bits == 0 {
            pokey.tick();
        }
        pokey.write(0x0D, 0xAA);

        pokey.write(0x0F, 0x00);
        assert_eq!(pokey.serial_out_bits, 0);
        assert!(!pokey.serout_pending);
    }

    /// With an external clock selected for both directions there is nothing on
    /// our SIO bus driving it, so the shift register never advances.
    #[test]
    fn an_external_serial_clock_never_shifts() {
        let mut pokey = sio_configured_pokey();
        pokey.write(0x0F, 0x03); // SKCTL bits 6-4 = %000 — external clock
        pokey.write(0x0D, 0x55);
        for _ in 0..10_000 {
            pokey.tick();
        }
        assert_eq!(pokey.serial_out_bits, 0);
        assert!(pokey.serout_pending, "the byte is still waiting");
    }

    /// The input shift register takes the same ten bit cells as the output
    /// one, and the byte does not appear in SERIN until the last of them.
    #[test]
    fn a_received_byte_arrives_after_a_whole_frame() {
        let mut pokey = sio_configured_pokey();
        assert!(pokey.serial_input_idle());
        assert_eq!(pokey.read(0x0E) & IRQ_SERIN_READY, IRQ_SERIN_READY);

        pokey.begin_serial_input(0x41);
        assert!(!pokey.serial_input_idle());
        assert_eq!(
            pokey.read(0x0F) & SKSTAT_SERIN_BUSY,
            0,
            "the shift register reports itself active"
        );

        let mut cycles = 0;
        while !pokey.serial_input_idle() {
            pokey.tick();
            cycles += 1;
            assert!(cycles < 100_000, "the byte never arrived");
            if !pokey.serial_input_idle() {
                assert_eq!(
                    pokey.read(0x0E) & IRQ_SERIN_READY,
                    IRQ_SERIN_READY,
                    "nothing is ready until the frame ends"
                );
            }
        }

        // Ten 94-cycle bit cells from the start bit, less however far into a
        // timer period the byte happened to arrive.
        assert!(
            (940 - 94..=940).contains(&cycles),
            "expected about ten bit cells, took {cycles}"
        );
        assert_eq!(pokey.read(0x0D), 0x41, "SERIN holds the byte");
        assert_eq!(pokey.read(0x0E) & IRQ_SERIN_READY, 0, "and raises its IRQ");
        assert_eq!(pokey.read(0x0F) & SKSTAT_SERIN_BUSY, SKSTAT_SERIN_BUSY);
    }

    /// Reading SERIN has no side effects — it does not acknowledge the byte,
    /// and it can be read as often as you like.
    #[test]
    fn reading_serin_does_not_acknowledge_it() {
        let mut pokey = sio_configured_pokey();
        pokey.begin_serial_input(0x43);
        while !pokey.serial_input_idle() {
            pokey.tick();
        }
        assert_eq!(pokey.read(0x0D), 0x43);
        assert_eq!(pokey.read(0x0D), 0x43);
        assert_eq!(pokey.read(0x0E) & IRQ_SERIN_READY, 0, "still pending");
    }

    /// A byte arriving while the last one is unread is an overrun: the new
    /// byte replaces the old, and SKSTAT bit 5 says so.
    #[test]
    fn a_byte_arriving_on_an_unread_one_is_an_overrun() {
        let mut pokey = sio_configured_pokey();
        let receive = |pokey: &mut Pokey, byte| {
            pokey.begin_serial_input(byte);
            while !pokey.serial_input_idle() {
                pokey.tick();
            }
        };

        receive(&mut pokey, 0x11);
        assert_eq!(
            pokey.read(0x0F) & SKSTAT_SERIN_OVERRUN,
            SKSTAT_SERIN_OVERRUN,
            "no overrun on the first byte"
        );

        receive(&mut pokey, 0x22);
        assert_eq!(pokey.read(0x0D), 0x22, "the new byte replaces the old");
        assert_eq!(
            pokey.read(0x0F) & SKSTAT_SERIN_OVERRUN,
            0,
            "and the overrun is flagged"
        );
    }

    /// Only one byte at a time is on the wire. Offering a second while the
    /// first is still shifting does nothing — the bus has no queue.
    #[test]
    fn a_byte_offered_mid_frame_is_dropped() {
        let mut pokey = sio_configured_pokey();
        pokey.begin_serial_input(0x11);
        for _ in 0..400 {
            pokey.tick();
        }
        pokey.begin_serial_input(0x22);
        while !pokey.serial_input_idle() {
            pokey.tick();
        }
        assert_eq!(pokey.read(0x0D), 0x11, "the first byte still arrives");
    }

    /// The byte the output shift register finishes goes onto the wire once,
    /// and it is the byte SEROUT held when the register loaded.
    #[test]
    fn a_sent_byte_reaches_the_bus_once() {
        let mut pokey = sio_configured_pokey();
        assert_eq!(pokey.take_serial_output(), None);

        pokey.write(0x0D, 0x53);
        while pokey.serial_out_bits == 0 {
            pokey.tick();
        }
        // Loaded, but not yet shifted out.
        assert_eq!(pokey.take_serial_output(), None);
        // Overwriting SEROUT now cannot change what is already shifting.
        pokey.write(0x0D, 0x99);

        let mut sent = None;
        while sent.is_none() {
            pokey.tick();
            sent = pokey.take_serial_output();
        }
        assert_eq!(sent, Some(0x53));
        assert_eq!(pokey.take_serial_output(), None, "offered once");
    }

    /// With an external clock selected there is nothing to shift the input
    /// register either.
    #[test]
    fn an_external_serial_clock_never_receives() {
        let mut pokey = sio_configured_pokey();
        pokey.write(0x0F, 0x03); // SKCTL bits 6-4 = %000 — external clock
        pokey.begin_serial_input(0x41);
        for _ in 0..10_000 {
            pokey.tick();
        }
        assert!(!pokey.serial_input_idle(), "still waiting for a clock");
        assert_eq!(pokey.read(0x0E) & IRQ_SERIN_READY, IRQ_SERIN_READY);
    }

    /// AUDCTL reads `PLY CH1 CH3 L12 L34 HP1 HP2 15K` from bit 7 down, so the
    /// polynomial select is the high bit and the clock select the low one.
    /// Six of these were transposed, and only the two link bits in the middle
    /// were right — which is why the audio tests, which exercise the link,
    /// never noticed. Asserted against the bit positions rather than against
    /// the constants, so the constants cannot define themselves as correct.
    #[test]
    fn audctl_bits_sit_where_the_register_layout_puts_them() {
        assert_eq!(AUDCTL_POLY9, 0x80);
        assert_eq!(AUDCTL_CH1_179MHZ, 0x40);
        assert_eq!(AUDCTL_CH3_179MHZ, 0x20);
        assert_eq!(AUDCTL_16BIT_CH12, 0x10);
        assert_eq!(AUDCTL_16BIT_CH34, 0x08);
        assert_eq!(AUDCTL_HPF_CH1, 0x04);
        assert_eq!(AUDCTL_HPF_CH2, 0x02);
        assert_eq!(AUDCTL_15KHZ, 0x01);
    }

    /// The 800XL OS sets up SIO with AUDCTL $28: channel 3's fast clock and
    /// the channel 3+4 link, which with AUDF3 $28 gives the documented 19040
    /// baud. Read with the bits transposed it selects the link and channel
    /// 1's high-pass filter, leaves channel 3 on the 64 kHz clock, and runs
    /// the serial port roughly twenty-five times too slow.
    #[test]
    fn the_sio_audctl_value_selects_the_fast_clock_and_the_link() {
        let mut pokey = ntsc_pokey();
        pokey.write(0x08, 0x28);
        assert_eq!(pokey.audctl & AUDCTL_CH3_179MHZ, AUDCTL_CH3_179MHZ);
        assert_eq!(pokey.audctl & AUDCTL_16BIT_CH34, AUDCTL_16BIT_CH34);
        assert_eq!(pokey.audctl & AUDCTL_HPF_CH1, 0);
        assert_eq!(pokey.audctl & AUDCTL_15KHZ, 0);
    }

    #[test]
    fn irqen_irqst_read_write() {
        let mut pokey = ntsc_pokey();

        // Initially nothing is pending but "serial output complete", which is
        // a level rather than a latch and reads asserted while the output
        // shift register is idle — which it is at power-on.
        assert_eq!(pokey.read(0x0E), !IRQ_SEROUT_DONE);

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
    fn output_clock_produces_exactly_48khz_over_one_second() {
        let mut pokey = ntsc_pokey();
        for _ in 0..1_789_772 {
            pokey.tick();
        }
        assert_eq!(pokey.buffer_len(), SAMPLE_RATE as usize);
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
    fn every_distortion_field_uses_the_documented_polynomial_gates() {
        let expected = |distortion, p5, p4, p17_or_9| match distortion {
            0b000 => p5 && p17_or_9,
            0b001 => p5,
            0b010 => p5 && p4,
            0b011 => p5,
            0b100 => p17_or_9,
            0b101 => true,
            0b110 => p4,
            0b111 => true,
            _ => unreachable!(),
        };

        let mut pokey = ntsc_pokey();
        for distortion in 0..=0b111 {
            for bits in 0..=0b111 {
                let p5 = bits & 0b001 != 0;
                let p4 = bits & 0b010 != 0;
                let p17_or_9 = bits & 0b100 != 0;
                pokey.poly5_table[0] = u8::from(p5);
                pokey.poly4_table[0] = u8::from(p4);
                pokey.poly17_table[0] = u8::from(p17_or_9);
                pokey.poly_counter = 0;

                assert_eq!(
                    pokey.poly_gate(distortion),
                    expected(distortion, p5, p4, p17_or_9),
                    "distortion {distortion:03b}, p5={p5}, p4={p4}, p17={p17_or_9}"
                );
            }
        }
    }

    #[test]
    fn sixteen_bit_mode_uses_one_little_endian_divider() {
        let mut pokey = ntsc_pokey();

        // Enable 16-bit mode for channels 1+2 and 1.79 MHz for channel 1.
        pokey.audctl = AUDCTL_16BIT_CH12 | AUDCTL_CH1_179MHZ;

        // In 1.79 MHz mode, $010A + 7 = 273 source ticks. Treating the bytes as cascaded
        // reloading counters would incorrectly produce (10+1)*(1+1) = 22.
        pokey.write(0x00, 10); // AUDF1, low byte
        pokey.write(0x02, 1); // AUDF2, high byte
        pokey.write(0x03, 0xAF); // AUDC2: pure tone, volume 15
        pokey.write(0x09, 0); // STIMER

        assert_eq!(pokey.channels[0].counter, 0x0110);
        let output_before = pokey.channels[1].output;
        for _ in 0..272 {
            pokey.tick();
        }
        assert_eq!(
            pokey.channels[1].output, output_before,
            "the high channel must not toggle before the full divider expires"
        );

        pokey.tick();
        assert_ne!(pokey.channels[1].output, output_before);
    }

    #[test]
    fn maximum_linked_divider_includes_high_speed_latency_without_overflow() {
        let mut pokey = ntsc_pokey();
        pokey.write(0x00, 0xFF);
        pokey.write(0x02, 0xFF);
        pokey.write(0x08, AUDCTL_16BIT_CH12 | AUDCTL_CH1_179MHZ);
        pokey.write(0x09, 0);

        assert_eq!(pokey.channels[0].counter, 0x1_0005);
    }

    #[test]
    fn audctl_join_bits_select_the_documented_channel_pairs() {
        let mut pokey = ntsc_pokey();
        pokey.write(0x00, 0x12);
        pokey.write(0x02, 0x34);
        pokey.write(0x04, 0x56);
        pokey.write(0x06, 0x78);

        pokey.write(0x08, 0x10); // bit 4: join channels 1+2
        pokey.write(0x09, 0);
        assert_eq!(pokey.channels[0].counter, 0x3412);
        assert_eq!(pokey.channels[2].counter, 0x0056);

        pokey.write(0x08, 0x08); // bit 3: join channels 3+4
        pokey.write(0x09, 0);
        assert_eq!(pokey.channels[0].counter, 0x0012);
        assert_eq!(pokey.channels[2].counter, 0x7856);
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

    #[test]
    fn press_key_latches_code_and_raises_interrupt() {
        let mut pokey = ntsc_pokey();
        // Idle: no key pending (IRQST bit 6 high), no key down (SKSTAT bit 2 high).
        assert_eq!(pokey.read(0x0E) & IRQ_KEY, IRQ_KEY);
        assert_eq!(pokey.read(0x0F) & SKSTAT_KEY_DOWN, SKSTAT_KEY_DOWN);

        pokey.press_key(0x2F); // 'q'
        assert_eq!(pokey.read(0x09), 0x2F, "KBCODE latches the scan code");
        assert_eq!(pokey.read(0x0E) & IRQ_KEY, 0, "keyboard IRQ pending");
        assert_eq!(pokey.read(0x0F) & SKSTAT_KEY_DOWN, 0, "key is down");

        // The CPU acks the keyboard IRQ by toggling IRQEN (writing it with the
        // bit clear), exactly as the OS dispatcher does.
        pokey.write(0x0E, !IRQ_KEY);
        assert_eq!(pokey.read(0x0E) & IRQ_KEY, IRQ_KEY, "ack clears the IRQ");

        pokey.release_key();
        assert_eq!(
            pokey.read(0x0F) & SKSTAT_KEY_DOWN,
            SKSTAT_KEY_DOWN,
            "release clears key-down"
        );
        assert_eq!(
            pokey.read(0x09),
            0x2F,
            "KBCODE retains its value after release"
        );
    }

    #[test]
    fn shift_bit_in_the_scan_code_drives_skstat() {
        let mut pokey = ntsc_pokey();
        assert_eq!(pokey.read(0x0F) & SKSTAT_SHIFT, SKSTAT_SHIFT);

        // '(' on the Atari keyboard: Shift + the '9' key. Shift is KBCODE
        // bit 6; bit 7 would be Control.
        pokey.press_key(0x30 | KBCODE_SHIFT);
        assert_eq!(pokey.read(0x0F) & SKSTAT_SHIFT, 0, "Shift reads as held");

        pokey.release_key();
        assert_eq!(
            pokey.read(0x0F) & SKSTAT_SHIFT,
            SKSTAT_SHIFT,
            "Shift reads as released"
        );

        pokey.press_key(0x30);
        assert_eq!(
            pokey.read(0x0F) & SKSTAT_SHIFT,
            SKSTAT_SHIFT,
            "an unshifted key leaves Shift released"
        );
    }
}
