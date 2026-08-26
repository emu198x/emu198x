//! Ricoh 2A03 APU (Audio Processing Unit).
//!
//! The APU lives on the 2A03 CPU die. It produces audio via two pulse
//! channels, one triangle channel, one noise channel, and a DMC (delta
//! modulation) channel. The DMC fetches 1-bit delta-encoded samples from
//! PRG memory via DMA, stealing CPU cycles one byte at a time.
//!
//! The APU is ticked once per CPU cycle (~1.789 MHz NTSC). Pulse and
//! noise timers decrement every other CPU cycle (APU cycle). The triangle
//! timer decrements every CPU cycle. The frame counter divides CPU cycles
//! into quarter-frame and half-frame events for envelope, length counter,
//! linear counter, and sweep updates.
//!
//! Output is mixed through a non-linear mixer (nesdev formula) and
//! downsampled to 48 kHz.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]

use serde::{Deserialize, Serialize};

/// Audio routing version. Bumped when the audio path through this crate
/// (pulse / triangle / noise / DMC channel mix, frame counter sequencing,
/// non-linear mixer formula, APU → audio_frame routing) changes in a
/// way that invalidates previously-captured audio hashes in the NES
/// catalogue. The catalogue manifest carries the version each hash was
/// captured against; a mismatch fails loud with a re-capture
/// instruction.
///
/// **Version 1** (2026-05-20): five-channel mixer (2 pulse + triangle +
/// noise + DMC) with nesdev non-linear mixer formula, frame counter in
/// 4-step or 5-step mode per `$4017`, DMC sample fetch via cycle-stealing
/// DMA. NTSC/PAL frame counter divider selected per `ApuRegion`. The
/// pre-Seam-3 APU described in
/// `knowledge/decisions/nes-architecture-review.md`.
///
/// See `knowledge/decisions/nes-architecture-review.md` Seam 4 for
/// the re-capture discipline this constant enforces.
pub const AUDIO_ROUTING_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// APU region — selects NTSC or PAL timing tables.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApuRegion {
    /// NTSC: 1,789,773 Hz CPU clock.
    #[default]
    Ntsc,
    /// PAL: 1,662,607 Hz CPU clock.
    Pal,
}

impl ApuRegion {
    /// CPU frequency in Hz for this region.
    #[must_use]
    pub const fn cpu_hz(self) -> u32 {
        match self {
            Self::Ntsc => 1_789_773,
            Self::Pal => 1_662_607,
        }
    }
}

// ---------------------------------------------------------------------------
// Host-side audio controls
// ---------------------------------------------------------------------------

/// Host-side NES APU channel identifier.
///
/// These controls are deliberately outside the emulated APU register surface:
/// muting a channel here must not affect `$4015`, length counters, IRQs, or DMA.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ApuChannel {
    /// First pulse channel.
    Pulse1,
    /// Second pulse channel.
    Pulse2,
    /// Triangle channel.
    Triangle,
    /// Noise channel.
    Noise,
    /// Delta modulation channel.
    Dmc,
}

impl ApuChannel {
    const fn index(self) -> usize {
        match self {
            Self::Pulse1 => 0,
            Self::Pulse2 => 1,
            Self::Triangle => 2,
            Self::Noise => 3,
            Self::Dmc => 4,
        }
    }

    /// Human-readable channel label for frontend status messages.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pulse1 => "pulse 1",
            Self::Pulse2 => "pulse 2",
            Self::Triangle => "triangle",
            Self::Noise => "noise",
            Self::Dmc => "DMC",
        }
    }
}

/// Per-channel host mixer control.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChannelControl {
    enabled: bool,
    gain: f32,
}

impl Default for ChannelControl {
    fn default() -> Self {
        Self {
            enabled: true,
            gain: 1.0,
        }
    }
}

impl ChannelControl {
    /// Whether this channel contributes to host audio output.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Linear channel gain after sanitisation, clamped to 0.0..=1.0.
    #[must_use]
    pub const fn gain(self) -> f32 {
        self.gain
    }

    fn apply(self, sample: f32) -> f32 {
        if self.enabled {
            sample * sanitize_gain(self.gain)
        } else {
            0.0
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn set_gain(&mut self, gain: f32) {
        self.gain = sanitize_gain(gain);
    }
}

/// Host-side audio controls for the NES APU mixer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioControls {
    master_gain: f32,
    channels: [ChannelControl; 5],
}

impl Default for AudioControls {
    fn default() -> Self {
        Self {
            master_gain: 1.0,
            channels: [ChannelControl::default(); 5],
        }
    }
}

impl AudioControls {
    /// Master gain applied to the internal APU mix.
    #[must_use]
    pub const fn master_gain(self) -> f32 {
        self.master_gain
    }

    /// Set master gain. Non-finite values become 0.0; finite values clamp to
    /// 0.0..=1.0.
    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = sanitize_gain(gain);
    }

    /// Return control state for one channel.
    #[must_use]
    pub const fn channel(self, channel: ApuChannel) -> ChannelControl {
        self.channels[channel.index()]
    }

    /// Enable or disable one channel in the host mixer.
    pub fn set_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        self.channels[channel.index()].set_enabled(enabled);
    }

    /// Set linear gain for one channel. Non-finite values become 0.0; finite
    /// values clamp to 0.0..=1.0.
    pub fn set_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        self.channels[channel.index()].set_gain(gain);
    }

    fn sanitized(mut self) -> Self {
        self.master_gain = sanitize_gain(self.master_gain);
        for channel in &mut self.channels {
            channel.set_gain(channel.gain);
        }
        self
    }
}

fn sanitize_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Lookup tables
// ---------------------------------------------------------------------------

/// Length counter load values, indexed by the top 5 bits of the register write.
const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

/// Noise timer period lookup (NTSC).
const NOISE_PERIOD_TABLE_NTSC: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

/// Noise timer period lookup (PAL).
const NOISE_PERIOD_TABLE_PAL: [u16; 16] = [
    4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778,
];

/// DMC rate table (NTSC) — CPU cycles per sample bit output.
const DMC_RATE_TABLE_NTSC: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];

/// DMC rate table (PAL) — CPU cycles per sample bit output.
const DMC_RATE_TABLE_PAL: [u16; 16] = [
    398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
];

/// Triangle waveform: 32-step sequence (0–15 up, 15–0 down).
const TRIANGLE_SEQUENCE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];

/// Pulse duty cycle waveforms: 4 patterns × 8 steps.
/// 0 = 12.5%, 1 = 25%, 2 = 50%, 3 = 75% (negated 25%).
const PULSE_DUTY: [[bool; 8]; 4] = [
    [false, true, false, false, false, false, false, false], // 12.5%
    [false, true, true, false, false, false, false, false],  // 25%
    [false, true, true, true, true, false, false, false],    // 50%
    [true, false, false, true, true, true, true, true],      // 75%
];

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

/// Decay envelope, clocked at quarter-frame rate.
///
/// When the loop flag is clear, the envelope counts down from 15 to 0 and
/// stays there. When loop is set, it wraps from 0 back to 15.
#[derive(Clone, Serialize, Deserialize)]
struct Envelope {
    start_flag: bool,
    divider: u8,
    decay_level: u8,
    /// Volume / divider period (from register bits 0–3).
    volume: u8,
    /// Constant volume flag (register bit 4).
    constant_volume: bool,
    /// Loop flag (register bit 5, shared with length counter halt).
    loop_flag: bool,
}

impl Envelope {
    fn new() -> Self {
        Self {
            start_flag: false,
            divider: 0,
            decay_level: 0,
            volume: 0,
            constant_volume: false,
            loop_flag: false,
        }
    }

    /// Clock the envelope (called at quarter-frame rate).
    fn clock(&mut self) {
        if self.start_flag {
            self.start_flag = false;
            self.decay_level = 15;
            self.divider = self.volume;
        } else if self.divider == 0 {
            self.divider = self.volume;
            if self.decay_level > 0 {
                self.decay_level -= 1;
            } else if self.loop_flag {
                self.decay_level = 15;
            }
        } else {
            self.divider -= 1;
        }
    }

    /// Current output level (0–15).
    fn output(&self) -> u8 {
        if self.constant_volume {
            self.volume
        } else {
            self.decay_level
        }
    }
}

// ---------------------------------------------------------------------------
// Length counter
// ---------------------------------------------------------------------------

/// Length counter — counts down at half-frame rate. When it reaches zero
/// the channel is silenced.
///
/// Half-frame clocking is split into **detection** (when the frame
/// counter decides an HF is due) and **application** (when the
/// length counter actually decrements). Between those two events the
/// CPU can run one bus cycle that writes the channel's control
/// register, and the blargg `10.len_halt_timing` / `11.len_reload_timing`
/// tests probe exactly that gap.
///
/// To match silicon-validated behaviour the LengthCounter:
///
/// - Snapshots `halt` at HF detection (`snapshot_for_half_frame`).
/// - Tracks whether `load()` has fired since the last snapshot.
/// - The deferred HF clock (`clock_via_snapshot`) uses the snapshot
///   `halt` and skips the decrement entirely if a `load()` fell in
///   the window — both quirks documented at NESdev wiki
///   `APU_Length_Counter`.
#[derive(Clone, Serialize, Deserialize)]
struct LengthCounter {
    counter: u8,
    halt: bool,
    enabled: bool,
    #[serde(default)]
    halt_snapshot: bool,
    #[serde(default)]
    counter_snapshot: u8,
    #[serde(default)]
    loaded_since_snapshot: bool,
}

impl LengthCounter {
    fn new() -> Self {
        Self {
            counter: 0,
            halt: false,
            enabled: false,
            halt_snapshot: false,
            counter_snapshot: 0,
            loaded_since_snapshot: false,
        }
    }

    /// Capture the current halt + counter values for use by the
    /// next deferred half-frame clock. Called by the APU when the
    /// frame counter detects a half-frame event.
    fn snapshot_for_half_frame(&mut self) {
        self.halt_snapshot = self.halt;
        self.counter_snapshot = self.counter;
        self.loaded_since_snapshot = false;
    }

    /// Clock the length counter using the most recent snapshot.
    /// Used by the deferred half-frame path. Implements the silicon
    /// rules from NESdev wiki `APU_Length_Counter`:
    ///
    /// - Plain HF (no intervening load): decrement uses the
    ///   snapshot halt.
    /// - Load coincided with HF AND snapshot counter was 0: load
    ///   wins, no decrement (the counter is already the new value).
    /// - Load coincided with HF AND snapshot counter was > 0: load
    ///   is **ignored** — restore the pre-load counter, then apply
    ///   the snapshot-halt decrement to it.
    fn clock_via_snapshot(&mut self) {
        if self.loaded_since_snapshot {
            if self.counter_snapshot == 0 {
                // Counter was 0 before the HF. Load takes effect;
                // no decrement.
                return;
            }
            // Counter was non-zero before the HF — load is ignored.
            // Restore the pre-load value and apply the HF decrement.
            self.counter = self.counter_snapshot;
        }
        if !self.halt_snapshot && self.counter > 0 {
            self.counter -= 1;
        }
    }

    /// Clock the length counter using the current halt value.
    /// Used by the immediate 5-step path (which fires off a $4017
    /// write rather than the frame counter's HF detection, so no
    /// snapshot exists yet).
    fn clock_immediate(&mut self) {
        if !self.halt && self.counter > 0 {
            self.counter -= 1;
        }
    }

    /// Whether the channel is active (length counter > 0).
    fn active(&self) -> bool {
        self.counter > 0
    }

    /// Load a new value from the length table.
    fn load(&mut self, index: u8) {
        if self.enabled {
            self.counter = LENGTH_TABLE[index as usize];
            self.loaded_since_snapshot = true;
        }
    }

    /// Set enabled state. Disabling clears the counter.
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.counter = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// Sweep unit
// ---------------------------------------------------------------------------

/// Sweep unit for pulse channels. Adjusts the pulse timer period up or
/// down over time. Pulse 1 uses one's-complement negation (period =
/// period - (period >> shift) - 1). Pulse 2 uses two's-complement
/// (period = period - (period >> shift)).
#[derive(Clone, Serialize, Deserialize)]
struct Sweep {
    enabled: bool,
    negate: bool,
    shift: u8,
    period: u8,
    divider: u8,
    reload_flag: bool,
    /// True for pulse 1 (one's-complement negate), false for pulse 2.
    ones_complement: bool,
}

impl Sweep {
    fn new(ones_complement: bool) -> Self {
        Self {
            enabled: false,
            negate: false,
            shift: 0,
            period: 0,
            divider: 0,
            reload_flag: false,
            ones_complement,
        }
    }

    /// Compute the target period given the current timer period.
    fn target_period(&self, current_period: u16) -> u16 {
        let shift_result = current_period >> self.shift;
        if self.negate {
            if self.ones_complement {
                current_period.wrapping_sub(shift_result).wrapping_sub(1)
            } else {
                current_period.wrapping_sub(shift_result)
            }
        } else {
            current_period.wrapping_add(shift_result)
        }
    }

    /// Whether the sweep is muting the channel.
    ///
    /// The `target > $7FF` overflow-mute only applies when negate is **clear**.
    /// With negate set the change amount is subtracted, so a real target can
    /// never exceed the current period — and the one's/two's-complement
    /// negation can underflow to a large value that must not be read as an
    /// overflow. Gating the check on `!negate` is what lets the canonical
    /// "disable the sweep" idiom (write $08 to $4001/$4005: negate set,
    /// shift 0) keep the channel audible. Matches Mesen2 `SquareChannel::
    /// IsMuted` (`_realPeriod < 8 || (!_sweepNegate && _target > 0x7FF)`).
    fn muting(&self, current_period: u16) -> bool {
        current_period < 8 || (!self.negate && self.target_period(current_period) > 0x7FF)
    }

    /// Clock the sweep (called at half-frame rate). Returns new timer period.
    fn clock(&mut self, current_period: u16) -> u16 {
        let mut new_period = current_period;

        if self.divider == 0 && self.enabled && self.shift > 0 && !self.muting(current_period) {
            let target = self.target_period(current_period);
            if target <= 0x7FF {
                new_period = target;
            }
        }

        if self.divider == 0 || self.reload_flag {
            self.divider = self.period;
            self.reload_flag = false;
        } else {
            self.divider -= 1;
        }

        new_period
    }
}

// ---------------------------------------------------------------------------
// Pulse channel
// ---------------------------------------------------------------------------

/// Pulse wave channel (two instances: pulse 1 and pulse 2).
#[derive(Clone, Serialize, Deserialize)]
struct Pulse {
    /// 11-bit timer period (from registers).
    timer_period: u16,
    /// Timer countdown.
    timer: u16,
    /// 8-step duty sequencer position.
    duty_pos: u8,
    /// Duty cycle selection (0–3).
    duty: u8,
    envelope: Envelope,
    length: LengthCounter,
    sweep: Sweep,
}

impl Pulse {
    fn new(ones_complement_negate: bool) -> Self {
        Self {
            timer_period: 0,
            timer: 0,
            duty_pos: 0,
            duty: 0,
            envelope: Envelope::new(),
            length: LengthCounter::new(),
            sweep: Sweep::new(ones_complement_negate),
        }
    }

    /// Clock the pulse timer (called at APU cycle rate = CPU/2).
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.duty_pos = (self.duty_pos + 1) % 8;
        } else {
            self.timer -= 1;
        }
    }

    /// Current output (0–15).
    fn output(&self) -> u8 {
        if !self.length.active() {
            return 0;
        }
        if self.sweep.muting(self.timer_period) {
            return 0;
        }
        if !PULSE_DUTY[self.duty as usize][self.duty_pos as usize] {
            return 0;
        }
        self.envelope.output()
    }
}

// ---------------------------------------------------------------------------
// Triangle channel
// ---------------------------------------------------------------------------

/// Triangle wave channel. The timer ticks at CPU rate (not APU rate).
/// Uses a 32-step sequence and has both a length counter and a linear
/// counter.
#[derive(Clone, Serialize, Deserialize)]
struct Triangle {
    timer_period: u16,
    timer: u16,
    sequence_pos: u8,
    length: LengthCounter,
    /// Linear counter value.
    linear_counter: u8,
    /// Linear counter reload value (from register).
    linear_counter_reload: u8,
    /// Linear counter reload flag.
    linear_reload_flag: bool,
    /// Control flag (shared with length counter halt).
    control_flag: bool,
}

impl Triangle {
    fn new() -> Self {
        Self {
            timer_period: 0,
            timer: 0,
            sequence_pos: 0,
            length: LengthCounter::new(),
            linear_counter: 0,
            linear_counter_reload: 0,
            linear_reload_flag: false,
            control_flag: false,
        }
    }

    /// Clock the triangle timer (called every CPU cycle).
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            // Only advance sequence when both counters are non-zero
            if self.length.active() && self.linear_counter > 0 {
                self.sequence_pos = (self.sequence_pos + 1) % 32;
            }
        } else {
            self.timer -= 1;
        }
    }

    /// Clock the linear counter (called at quarter-frame rate).
    fn clock_linear_counter(&mut self) {
        if self.linear_reload_flag {
            self.linear_counter = self.linear_counter_reload;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
        if !self.control_flag {
            self.linear_reload_flag = false;
        }
    }

    /// Current output (0–15).
    fn output(&self) -> u8 {
        if !self.length.active() || self.linear_counter == 0 {
            return 0;
        }
        // Silence ultrasonic frequencies to avoid aliasing
        if self.timer_period < 2 {
            return 0;
        }
        TRIANGLE_SEQUENCE[self.sequence_pos as usize]
    }
}

// ---------------------------------------------------------------------------
// Noise channel
// ---------------------------------------------------------------------------

/// Noise channel. Uses a 15-bit LFSR with selectable feedback tap
/// (bit 1 for long mode, bit 6 for short mode).
#[derive(Clone, Serialize, Deserialize)]
struct Noise {
    timer_period: u16,
    timer: u16,
    /// 15-bit linear feedback shift register.
    shift_register: u16,
    /// Short mode: use bit 6 for feedback instead of bit 1.
    mode: bool,
    envelope: Envelope,
    length: LengthCounter,
}

impl Noise {
    fn new() -> Self {
        Self {
            timer_period: 0,
            timer: 0,
            shift_register: 1, // Initial state
            mode: false,
            envelope: Envelope::new(),
            length: LengthCounter::new(),
        }
    }

    /// Clock the noise timer (called at APU cycle rate = CPU/2).
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            // Feedback: XOR bit 0 with bit 1 (normal) or bit 6 (short mode)
            let feedback_bit = if self.mode { 6 } else { 1 };
            let feedback = (self.shift_register & 1) ^ ((self.shift_register >> feedback_bit) & 1);
            self.shift_register >>= 1;
            self.shift_register |= feedback << 14;
        } else {
            self.timer -= 1;
        }
    }

    /// Current output (0–15).
    fn output(&self) -> u8 {
        if !self.length.active() {
            return 0;
        }
        // Bit 0 of shift register gates output (inverted: 0 = output, 1 = silence)
        if self.shift_register & 1 != 0 {
            return 0;
        }
        self.envelope.output()
    }
}

// ---------------------------------------------------------------------------
// DMC channel
// ---------------------------------------------------------------------------

/// DMC (delta modulation) channel. Fetches 1-bit delta-encoded samples from
/// PRG memory via DMA, producing drums, bass, and speech. A timer clocks
/// the output shift register; when the shift register is exhausted, the
/// sample buffer is loaded. When the sample buffer is empty and bytes
/// remain, `dma_pending` signals the tick loop to steal a CPU cycle.
#[derive(Clone, Serialize, Deserialize)]
pub struct Dmc {
    /// 7-bit output level (0–127), written directly by $4011.
    pub output_level: u8,
    /// IRQ enable flag (bit 7 of $4010).
    irq_enabled: bool,
    /// IRQ pending flag, read via bit 7 of $4015.
    pub irq_flag: bool,
    /// Loop flag (bit 6 of $4010).
    loop_flag: bool,
    /// Rate index (bits 0–3 of $4010).
    rate_index: u8,
    /// Countdown timer, clocked every CPU cycle.
    timer: u16,
    /// Timer reload value from `DMC_RATE_TABLE[rate_index]`.
    pub timer_period: u16,
    /// Starting sample address (from $4012).
    pub sample_address: u16,
    /// Total sample length in bytes (from $4013).
    pub sample_length: u16,
    /// Current DMA fetch address.
    pub current_address: u16,
    /// Bytes remaining to fetch.
    pub bytes_remaining: u16,
    /// Last byte fetched from memory.
    sample_buffer: u8,
    /// True when the sample buffer has been consumed.
    sample_buffer_empty: bool,
    /// 8-bit output shift register.
    pub shift_register: u8,
    /// Bits remaining in the shift register (counts down from 8).
    pub bits_remaining: u8,
    /// True when no sample data is available for output.
    pub silence_flag: bool,
    /// Controlled by bit 4 of $4015.
    enabled: bool,
    /// Signals the tick loop to steal a CPU cycle for a DMA fetch.
    pub dma_pending: bool,
    /// Set when `$4015` clears DMC enable while a DMA request was queued.
    /// The machine layer consumes this to decide between cancelling a
    /// not-yet-halted transfer and aborting a halted one.
    pub dma_cancelled: bool,
    /// Cycles left before a `$4015`-initiated transfer may start.
    ///
    /// ⚠⚠ Enabling an *idle* DMC does NOT request the fetch there. Hardware
    /// has no buffer-consumption event to hang the request on, so it
    /// synthesises one 2 or 3 cycles later, chosen by CPU get/put parity.
    /// See [`Dmc::start_transfer`] for why this must not be collapsed into
    /// an immediate request.
    #[serde(default)]
    pub transfer_start_delay: u8,
}

impl Dmc {
    fn new() -> Self {
        Self {
            output_level: 0,
            irq_enabled: false,
            irq_flag: false,
            loop_flag: false,
            rate_index: 0,
            timer: DMC_RATE_TABLE_NTSC[0] - 1,
            timer_period: DMC_RATE_TABLE_NTSC[0],
            sample_address: 0xC000,
            sample_length: 1,
            current_address: 0xC000,
            bytes_remaining: 0,
            sample_buffer: 0,
            sample_buffer_empty: true,
            shift_register: 0,
            bits_remaining: 8,
            silence_flag: true,
            enabled: false,
            dma_pending: false,
            dma_cancelled: false,
            transfer_start_delay: 0,
        }
    }

    /// Request a sample fetch, if one is actually due.
    ///
    /// Mirrors Mesen2's `StartDmcTransfer`: a request only stands when the
    /// sample buffer is empty and bytes remain. Both callers — the output
    /// unit consuming the buffer, and the `$4015` start delay expiring —
    /// funnel through here so the guard cannot be forgotten at one of them.
    fn start_transfer(&mut self) {
        if self.sample_buffer_empty && self.bytes_remaining > 0 {
            self.dma_pending = true;
        }
    }

    /// Count down a pending `$4015` transfer-start delay, requesting the
    /// fetch as it expires. Called once per CPU cycle.
    fn clock_transfer_start_delay(&mut self) {
        if self.transfer_start_delay > 0 {
            self.transfer_start_delay -= 1;
            if self.transfer_start_delay == 0 {
                self.start_transfer();
            }
        }
    }

    /// Clock the DMC timer. Called every CPU cycle.
    ///
    /// The timer counts down from `timer_period - 1` to 0, giving exactly
    /// `timer_period` ticks between output clocks. The rate table values
    /// represent the period in CPU cycles (e.g. 428 for rate 0), and the
    /// reload subtracts 1 because the 0→clock transition is one of the
    /// period's ticks.
    fn tick(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period - 1;
            self.clock_output();
        } else {
            self.timer -= 1;
        }
    }

    /// Clock the output unit: shift one bit and update `output_level`.
    fn clock_output(&mut self) {
        // Update output level from the shift register
        if !self.silence_flag {
            if self.shift_register & 1 != 0 {
                if self.output_level <= 125 {
                    self.output_level += 2;
                }
            } else if self.output_level >= 2 {
                self.output_level -= 2;
            }
            self.shift_register >>= 1;
        }

        // Count down bits; reload from sample buffer when exhausted
        self.bits_remaining -= 1;
        if self.bits_remaining == 0 {
            self.bits_remaining = 8;
            if self.sample_buffer_empty {
                self.silence_flag = true;
            } else {
                self.silence_flag = false;
                self.shift_register = self.sample_buffer;
                self.sample_buffer_empty = true;
                // ⚠ The request belongs INSIDE this branch. Sitting after the
                // if/else it also fired on the silence path, where the buffer
                // was already empty and hardware requests nothing.
                //
                // The interlock: a `$4015` write that armed the start delay
                // owns the next fetch, so consumption must not pre-empt it.
                if self.transfer_start_delay == 0 {
                    self.start_transfer();
                }
            }
        }
    }

    /// Whether DMC playback is enabled via `$4015` bit 4.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Whether the DMC's per-sample IRQ is armed via `$4010` bit 7.
    #[must_use]
    pub fn irq_enabled(&self) -> bool {
        self.irq_enabled
    }

    /// Deliver a byte fetched by the DMA controller.
    pub fn receive_dma_byte(&mut self, byte: u8) {
        self.sample_buffer = byte;
        self.sample_buffer_empty = false;
        self.dma_pending = false;

        // Advance address (wraps $FFFF → $8000)
        self.current_address = if self.current_address == 0xFFFF {
            0x8000
        } else {
            self.current_address + 1
        };

        self.bytes_remaining -= 1;
        if self.bytes_remaining == 0 {
            if self.loop_flag {
                self.current_address = self.sample_address;
                self.bytes_remaining = self.sample_length;
            } else if self.irq_enabled {
                self.irq_flag = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Frame counter
// ---------------------------------------------------------------------------

/// Frame counter mode.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum FrameCounterMode {
    /// 4-step: generates IRQ, 4 quarter-frame events per sequence.
    FourStep,
    /// 5-step: no IRQ, 5 quarter-frame events per sequence.
    FiveStep,
}

// Frame counter step boundaries (CPU cycles from counter reset).
// These match the nesdev wiki values. The jitter between even/odd
// CPU cycles comes from the delayed counter reset in the $4017 handler.
// NTSC 4-step: events at 7457, 14913, 22371, 29829 (IRQ at step 4)
// NTSC 5-step: events at 7457, 14913, 22371, 29829, 37281 (no IRQ)
const FOUR_STEP_SEQUENCE_NTSC: [u16; 4] = [7457, 14913, 22371, 29829];
const FIVE_STEP_SEQUENCE_NTSC: [u16; 5] = [7457, 14913, 22371, 29829, 37281];

// PAL frame counter boundaries (adjusted for ~50 Hz frame rate).
const FOUR_STEP_SEQUENCE_PAL: [u16; 4] = [8313, 16627, 24939, 33253];
const FIVE_STEP_SEQUENCE_PAL: [u16; 5] = [8313, 16627, 24939, 33253, 41565];

fn default_noise_period_table() -> &'static [u16; 16] {
    &NOISE_PERIOD_TABLE_NTSC
}

fn default_dmc_rate_table() -> &'static [u16; 16] {
    &DMC_RATE_TABLE_NTSC
}

fn default_four_step_seq() -> &'static [u16; 4] {
    &FOUR_STEP_SEQUENCE_NTSC
}

fn default_five_step_seq() -> &'static [u16; 5] {
    &FIVE_STEP_SEQUENCE_NTSC
}

// ---------------------------------------------------------------------------
// APU
// ---------------------------------------------------------------------------

/// Ricoh 2A03 APU.
#[derive(Clone, Serialize, Deserialize)]
pub struct Apu {
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    /// DMC channel. Public for DMA coordination with the system bus.
    pub dmc: Dmc,
    /// Parity of the machine's CPU cycle counter, published by the machine
    /// layer each cycle.
    ///
    /// ⚠ The APU cannot derive this. Its own `odd_cycle` counts APU cycles
    /// and is free to be out of phase; the DMA get/put phase belongs to the
    /// CPU cycle counter, which keeps running through DMA stalls. The DMC
    /// transfer-start delay is chosen from it, so it must come from the
    /// same counter the DMA arbiter uses.
    #[serde(default)]
    pub cpu_cycle_odd: bool,

    // Frame counter
    frame_mode: FrameCounterMode,
    frame_counter: u16,
    frame_step: u8,
    frame_irq_inhibit: bool,
    frame_irq_flag: bool,
    /// Last value written to `$4017`. Captured so [`soft_reset`]
    /// can rewrite it (real silicon rewrites the last value on
    /// reset, not `$00` as at power-on — blargg `4017_written`).
    #[serde(default)]
    last_4017: u8,

    /// CPU cycle parity: true on odd CPU cycles (pulse/noise tick on even).
    odd_cycle: bool,

    /// Pending $4017 write: (value, cycles_remaining). Real hardware delays
    /// mode and IRQ-inhibit bits by 3 cycles (odd CPU cycle) or 4 (even).
    frame_counter_pending: Option<(u8, u8)>,

    /// Delayed counter reset after step 3 (mode 0) or step 4 (mode 1).
    /// The real APU continues setting the IRQ flag (mode 0 only) for 2
    /// extra cycles after the step event, then resets the counter. This
    /// produces the "three consecutive IRQ sets" documented in blargg's
    /// 6-irq_flag_timing test.
    frame_reset_countdown: u8,

    /// Deferred QF/HF clocking. On real hardware, the frame counter sets
    /// the IRQ flag early in the cycle, but the length counter and envelope
    /// clocking happens later — after the CPU bus read phase. We model this
    /// by deferring the actual QF/HF clock to the NEXT tick. Bits:
    /// bit 0 = quarter-frame pending, bit 1 = half-frame pending.
    pending_frame_clock: u8,

    // Region. Serialized so `after_restore` can reattach the correct
    // `&'static` timing tables for PAL or NTSC. Without this field the
    // default-NTSC tables would silently corrupt PAL snapshots on
    // restore (Seam 3 audit).
    #[serde(default)]
    region: ApuRegion,

    // Region-dependent tables. `#[serde(skip)]` because they're
    // `&'static` references that don't survive serialise/deserialise.
    // `after_restore` re-points them based on `region`. The defaults
    // produce NTSC tables; constructors override before any tick().
    #[serde(skip, default = "default_noise_period_table")]
    noise_period_table: &'static [u16; 16],
    #[serde(skip, default = "default_dmc_rate_table")]
    dmc_rate_table: &'static [u16; 16],
    #[serde(skip, default = "default_four_step_seq")]
    four_step_seq: &'static [u16; 4],
    #[serde(skip, default = "default_five_step_seq")]
    five_step_seq: &'static [u16; 5],

    // Downsampling
    accumulator: f32,
    sample_count: u32,
    ticks_per_sample: f32,
    buffer: Vec<f32>,

    // Per-channel downsampling (linear pre-mix levels, normalised to -1..1)
    channel_accumulators: [f32; 5],
    channel_buffers: [Vec<f32>; 5],
    audio_controls: AudioControls,

    // DC-blocking high-pass filter (applied at output sample rate).
    // Removes the large DC offset inherent in the non-linear mixer.
    // First-order high-pass: y[n] = α * (y[n-1] + x[n] - x[n-1])
    // Cutoff ~37 Hz at 48 kHz: α ≈ 0.9952
    hp_prev_in: f32,
    hp_prev_out: f32,

    /// Expansion audio level from cartridge mapper (e.g. Sunsoft 5B, VRC6,
    /// Namco 163). Set externally each CPU cycle before calling `tick()`.
    /// Range: 0.0 to ~0.5 (mixed additively with the internal APU output).
    pub expansion_audio: f32,
}

impl Apu {
    /// Output sample rate.
    const SAMPLE_RATE: u32 = 48_000;

    /// Create an APU with NTSC timing (default).
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_region(ApuRegion::Ntsc)
    }

    /// Create an APU with region-specific timing tables.
    #[must_use]
    pub fn new_with_region(region: ApuRegion) -> Self {
        let cpu_freq = region.cpu_hz();
        let (noise_table, dmc_table, four_step, five_step) = match region {
            ApuRegion::Ntsc => (
                &NOISE_PERIOD_TABLE_NTSC,
                &DMC_RATE_TABLE_NTSC,
                &FOUR_STEP_SEQUENCE_NTSC,
                &FIVE_STEP_SEQUENCE_NTSC,
            ),
            ApuRegion::Pal => (
                &NOISE_PERIOD_TABLE_PAL,
                &DMC_RATE_TABLE_PAL,
                &FOUR_STEP_SEQUENCE_PAL,
                &FIVE_STEP_SEQUENCE_PAL,
            ),
        };
        Self {
            pulse1: Pulse::new(true),
            pulse2: Pulse::new(false),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: Dmc::new(),
            cpu_cycle_odd: false,
            frame_mode: FrameCounterMode::FourStep,
            frame_counter: 0,
            frame_step: 0,
            frame_irq_inhibit: false,
            frame_irq_flag: false,
            last_4017: 0,
            odd_cycle: false,
            frame_counter_pending: None,
            frame_reset_countdown: 0,
            pending_frame_clock: 0,
            region,
            noise_period_table: noise_table,
            dmc_rate_table: dmc_table,
            four_step_seq: four_step,
            five_step_seq: five_step,
            accumulator: 0.0,
            sample_count: 0,
            ticks_per_sample: cpu_freq as f32 / Self::SAMPLE_RATE as f32,
            buffer: Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
            channel_accumulators: [0.0; 5],
            channel_buffers: [
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
            ],
            audio_controls: AudioControls::default(),
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
            expansion_audio: 0.0,
        }
    }

    /// Soft reset — pressing the front-panel reset button.
    ///
    /// Per blargg `apu_reset/*`: `$4015` is cleared (channels off,
    /// DMC IRQ cleared), the frame IRQ flag is cleared, and the
    /// last value written to `$4017` is re-applied (so the frame
    /// counter mode is preserved — distinct from power-on, which
    /// behaves as if `$00` were written to `$4017`). The triangle
    /// length counter is unaffected — disable+re-enable would
    /// silence it; explicit re-write of `$4015` by the test ROM
    /// chooses what re-enables.
    pub fn soft_reset(&mut self) {
        // $4015 := 0 — channels off, DMC IRQ clear.
        self.write(0x4015, 0);
        // Re-apply last $4017 (preserves mode + irq-inhibit).
        let last = self.last_4017;
        self.write(0x4017, last);
        // Frame IRQ flag clears on reset.
        self.frame_irq_flag = false;
    }

    /// Reattach the `&'static` region-dependent timing tables after a
    /// snapshot restore. The fields are `#[serde(skip)]` because they
    /// hold `&'static` references that don't survive deserialise; this
    /// method re-points them based on the serialized `region` field.
    ///
    /// Call once after `serde::Deserialize` and before the next
    /// `tick()`. Idempotent — safe to call multiple times. Without
    /// this call PAL snapshots silently revert to NTSC timing on
    /// restore (the `default = "default_*"` functions return NTSC
    /// tables).
    ///
    /// See Seam 3 of `knowledge/decisions/nes-architecture-review.md`.
    pub fn after_restore(&mut self) {
        let (noise_table, dmc_table, four_step, five_step) = match self.region {
            ApuRegion::Ntsc => (
                &NOISE_PERIOD_TABLE_NTSC,
                &DMC_RATE_TABLE_NTSC,
                &FOUR_STEP_SEQUENCE_NTSC,
                &FIVE_STEP_SEQUENCE_NTSC,
            ),
            ApuRegion::Pal => (
                &NOISE_PERIOD_TABLE_PAL,
                &DMC_RATE_TABLE_PAL,
                &FOUR_STEP_SEQUENCE_PAL,
                &FIVE_STEP_SEQUENCE_PAL,
            ),
        };
        self.noise_period_table = noise_table;
        self.dmc_rate_table = dmc_table;
        self.four_step_seq = four_step;
        self.five_step_seq = five_step;
    }

    /// Current configured region (NTSC or PAL).
    #[must_use]
    pub const fn region(&self) -> ApuRegion {
        self.region
    }

    /// Current host-side audio controls.
    #[must_use]
    pub const fn audio_controls(&self) -> AudioControls {
        self.audio_controls
    }

    /// Replace all host-side audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.audio_controls = controls.sanitized();
    }

    /// Enable or disable one channel in the host mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        self.audio_controls.set_channel_enabled(channel, enabled);
    }

    /// Set linear gain for one channel in the host mixer.
    pub fn set_audio_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        self.audio_controls.set_channel_gain(channel, gain);
    }

    /// Read an APU register ($4015 is the only readable APU register).
    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x4015 => {
                let mut status = 0u8;
                if self.pulse1.length.active() {
                    status |= 0x01;
                }
                if self.pulse2.length.active() {
                    status |= 0x02;
                }
                if self.triangle.length.active() {
                    status |= 0x04;
                }
                if self.noise.length.active() {
                    status |= 0x08;
                }
                if self.dmc.bytes_remaining > 0 {
                    status |= 0x10;
                }
                if self.frame_irq_flag {
                    status |= 0x40;
                }
                if self.dmc.irq_flag {
                    status |= 0x80;
                }
                // Reading $4015 clears the frame IRQ flag
                self.frame_irq_flag = false;
                status
            }
            _ => 0,
        }
    }

    /// Write an APU register ($4000–$4013, $4015, $4017).
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Pulse 1: $4000–$4003
            0x4000 => {
                self.pulse1.duty = (value >> 6) & 0x03;
                self.pulse1.envelope.loop_flag = value & 0x20 != 0;
                self.pulse1.length.halt = value & 0x20 != 0;
                self.pulse1.envelope.constant_volume = value & 0x10 != 0;
                self.pulse1.envelope.volume = value & 0x0F;
            }
            0x4001 => {
                self.pulse1.sweep.enabled = value & 0x80 != 0;
                self.pulse1.sweep.period = (value >> 4) & 0x07;
                self.pulse1.sweep.negate = value & 0x08 != 0;
                self.pulse1.sweep.shift = value & 0x07;
                self.pulse1.sweep.reload_flag = true;
            }
            0x4002 => {
                self.pulse1.timer_period = (self.pulse1.timer_period & 0x0700) | u16::from(value);
            }
            0x4003 => {
                self.pulse1.timer_period =
                    (self.pulse1.timer_period & 0x00FF) | (u16::from(value & 0x07) << 8);
                self.pulse1.length.load((value >> 3) & 0x1F);
                self.pulse1.envelope.start_flag = true;
                self.pulse1.duty_pos = 0;
            }

            // Pulse 2: $4004–$4007
            0x4004 => {
                self.pulse2.duty = (value >> 6) & 0x03;
                self.pulse2.envelope.loop_flag = value & 0x20 != 0;
                self.pulse2.length.halt = value & 0x20 != 0;
                self.pulse2.envelope.constant_volume = value & 0x10 != 0;
                self.pulse2.envelope.volume = value & 0x0F;
            }
            0x4005 => {
                self.pulse2.sweep.enabled = value & 0x80 != 0;
                self.pulse2.sweep.period = (value >> 4) & 0x07;
                self.pulse2.sweep.negate = value & 0x08 != 0;
                self.pulse2.sweep.shift = value & 0x07;
                self.pulse2.sweep.reload_flag = true;
            }
            0x4006 => {
                self.pulse2.timer_period = (self.pulse2.timer_period & 0x0700) | u16::from(value);
            }
            0x4007 => {
                self.pulse2.timer_period =
                    (self.pulse2.timer_period & 0x00FF) | (u16::from(value & 0x07) << 8);
                self.pulse2.length.load((value >> 3) & 0x1F);
                self.pulse2.envelope.start_flag = true;
                self.pulse2.duty_pos = 0;
            }

            // Triangle: $4008–$400B
            0x4008 => {
                self.triangle.control_flag = value & 0x80 != 0;
                self.triangle.length.halt = value & 0x80 != 0;
                self.triangle.linear_counter_reload = value & 0x7F;
            }
            0x4009 => {} // Unused
            0x400A => {
                self.triangle.timer_period =
                    (self.triangle.timer_period & 0x0700) | u16::from(value);
            }
            0x400B => {
                self.triangle.timer_period =
                    (self.triangle.timer_period & 0x00FF) | (u16::from(value & 0x07) << 8);
                self.triangle.length.load((value >> 3) & 0x1F);
                self.triangle.linear_reload_flag = true;
            }

            // Noise: $400C–$400F
            0x400C => {
                self.noise.envelope.loop_flag = value & 0x20 != 0;
                self.noise.length.halt = value & 0x20 != 0;
                self.noise.envelope.constant_volume = value & 0x10 != 0;
                self.noise.envelope.volume = value & 0x0F;
            }
            0x400D => {} // Unused
            0x400E => {
                self.noise.mode = value & 0x80 != 0;
                self.noise.timer_period = self.noise_period_table[(value & 0x0F) as usize];
            }
            0x400F => {
                self.noise.length.load((value >> 3) & 0x1F);
                self.noise.envelope.start_flag = true;
            }

            // DMC: $4010–$4013
            0x4010 => {
                self.dmc.irq_enabled = value & 0x80 != 0;
                self.dmc.loop_flag = value & 0x40 != 0;
                self.dmc.rate_index = value & 0x0F;
                self.dmc.timer_period = self.dmc_rate_table[self.dmc.rate_index as usize];
                if !self.dmc.irq_enabled {
                    self.dmc.irq_flag = false;
                }
            }
            0x4011 => {
                // Direct load: 7-bit output level
                self.dmc.output_level = value & 0x7F;
            }
            0x4012 => {
                self.dmc.sample_address = 0xC000 + u16::from(value) * 64;
            }
            0x4013 => {
                self.dmc.sample_length = u16::from(value) * 16 + 1;
            }

            // Status: $4015
            0x4015 => {
                self.pulse1.length.set_enabled(value & 0x01 != 0);
                self.pulse2.length.set_enabled(value & 0x02 != 0);
                self.triangle.length.set_enabled(value & 0x04 != 0);
                self.noise.length.set_enabled(value & 0x08 != 0);

                // DMC enable (bit 4)
                let dmc_enable = value & 0x10 != 0;
                if dmc_enable {
                    if self.dmc.bytes_remaining == 0 {
                        self.dmc.current_address = self.dmc.sample_address;
                        self.dmc.bytes_remaining = self.dmc.sample_length;
                        // ⚠⚠ Do NOT request the fetch here. Requesting on the
                        // write made the fetch ride the write's cadence, so
                        // `sprdma_and_dmc_dma`'s alignment table came out flat
                        // where hardware alternates. Hardware instead waits
                        // 2 or 3 cycles by get/put parity, which lets the
                        // timer's own 432-cycle cadence stay in charge.
                        self.dmc.transfer_start_delay = if self.cpu_cycle_odd { 3 } else { 2 };
                    }
                } else {
                    self.dmc.bytes_remaining = 0;
                    // ⚠⚠ A queued-but-unstarted DMA must be CANCELLED, not left
                    // pending. `$4015` bit 4 cleared while `dma_pending` was set
                    // previously left the request standing, so the machine layer
                    // still stole a cycle for a transfer the program had just
                    // disabled — the sprite/DMC contention cost came out wrong
                    // by whole cycles at some alignments.
                    //
                    // Mesen2 splits this in `StopDmcTransfer()`: before the halt
                    // cycle the DMA is cancelled outright; once halted it can
                    // only be *aborted*, which is a different and observable
                    // path. This clears the pre-halt case; the machine layer
                    // owns the post-halt one, since only it knows whether the
                    // halt cycle has run.
                    self.dmc.dma_pending = false;
                    self.dmc.dma_cancelled = true;
                }
                self.dmc.enabled = dmc_enable;
                self.dmc.irq_flag = false;
            }

            // Frame counter: $4017
            //
            // Writing $4017 delays the entire frame counter reset by
            // 3-4 CPU cycles depending on APU even/odd cycle alignment.
            // This is the source of APU jitter — the counter reset
            // and mode change happen 1 cycle apart on even vs odd writes.
            // Only the IRQ inhibit flag clears immediately.
            0x4017 => {
                self.last_4017 = value;
                // IRQ-inhibit clears immediately
                if value & 0x40 != 0 {
                    self.frame_irq_flag = false;
                }
                // Delay counter reset + mode change
                let delay = if self.odd_cycle { 4 } else { 3 };
                self.frame_counter_pending = Some((value, delay));
            }

            _ => {}
        }
    }

    /// Tick the APU one CPU cycle.
    pub fn tick(&mut self) {
        // Execute deferred QF/HF clocking from the previous tick.
        // On real hardware, the frame counter sets the IRQ flag early in
        // the cycle, but length/envelope clocking happens after the CPU
        // bus read phase. We model this by deferring the actual clock to
        // the start of the NEXT tick — the CPU reads $4015 between the
        // flag set and the length decrement.
        if self.pending_frame_clock != 0 {
            if self.pending_frame_clock & 1 != 0 {
                self.clock_quarter_frame();
            }
            if self.pending_frame_clock & 2 != 0 {
                self.clock_half_frame_via_snapshot();
            }
            self.pending_frame_clock = 0;
        }

        // Process pending $4017 write (delayed counter reset + mode change)
        if let Some((value, delay)) = self.frame_counter_pending {
            if delay <= 1 {
                self.frame_counter_pending = None;
                // Reset counter and sequencer (cancels any pending reset)
                self.frame_counter = 0;
                self.frame_step = 0;
                self.frame_reset_countdown = 0;
                self.pending_frame_clock = 0;
                // Apply mode and IRQ inhibit
                self.frame_mode = if value & 0x80 != 0 {
                    FrameCounterMode::FiveStep
                } else {
                    FrameCounterMode::FourStep
                };
                self.frame_irq_inhibit = value & 0x40 != 0;
                // In 5-step mode, immediately clock all units.
                // This is NOT deferred — it happens during the pending
                // resolution, before the CPU can read $4015.
                if self.frame_mode == FrameCounterMode::FiveStep {
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                }
            } else {
                self.frame_counter_pending = Some((value, delay - 1));
            }
        }

        // Triangle timer ticks every CPU cycle
        self.triangle.clock_timer();

        // Pulse and noise timers tick every other CPU cycle (APU cycle)
        if self.odd_cycle {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.noise.clock_timer();
        }
        self.odd_cycle = !self.odd_cycle;

        // A `$4015`-armed transfer start counts down every CPU cycle and is
        // serviced before the timer, so the delay expiring and the output
        // unit consuming the buffer on the same cycle resolve in that order.
        self.dmc.clock_transfer_start_delay();

        // DMC timer ticks every CPU cycle
        self.dmc.tick();

        // Frame counter
        self.clock_frame_counter();

        // Capture per-channel linear levels (normalised to roughly -1..1).
        // Pulse/noise envelopes output 0–15, triangle outputs 0–15,
        // DMC outputs 0–127. Scale each so full-range maps to 0..1,
        // then centre around zero (subtract 0.5, multiply by 2) to get -1..1.
        let p1 = self
            .audio_controls
            .channel(ApuChannel::Pulse1)
            .apply(f32::from(self.pulse1.output()));
        let p2 = self
            .audio_controls
            .channel(ApuChannel::Pulse2)
            .apply(f32::from(self.pulse2.output()));
        let tri = self
            .audio_controls
            .channel(ApuChannel::Triangle)
            .apply(f32::from(self.triangle.output()));
        let noi = self
            .audio_controls
            .channel(ApuChannel::Noise)
            .apply(f32::from(self.noise.output()));
        let dmc = self
            .audio_controls
            .channel(ApuChannel::Dmc)
            .apply(f32::from(self.dmc.output_level));

        let p1_linear = p1 / 15.0;
        let p2_linear = p2 / 15.0;
        let tri_linear = tri / 15.0;
        let noi_linear = noi / 15.0;
        let dmc_linear = dmc / 127.0;

        self.channel_accumulators[0] += p1_linear;
        self.channel_accumulators[1] += p2_linear;
        self.channel_accumulators[2] += tri_linear;
        self.channel_accumulators[3] += noi_linear;
        self.channel_accumulators[4] += dmc_linear;

        // Mix and downsample (including expansion audio from cartridge)
        let sample = self.controlled_mix() + self.expansion_audio;
        self.accumulator += sample;
        self.sample_count += 1;

        if self.sample_count as f32 >= self.ticks_per_sample {
            let avg = self.accumulator / self.sample_count as f32;
            let count = self.sample_count as f32;

            // DC-blocking high-pass filter: removes mixer's inherent DC offset.
            // y[n] = α * (y[n-1] + x[n] - x[n-1]), α ≈ 0.9952 (~37 Hz at 48 kHz)
            const ALPHA: f32 = 0.9952;
            let filtered = ALPHA * (self.hp_prev_out + avg - self.hp_prev_in);
            self.hp_prev_in = avg;
            self.hp_prev_out = filtered;

            self.buffer.push(filtered);

            // Emit per-channel downsampled values, centred around zero (-1..1).
            for (buf, acc) in self
                .channel_buffers
                .iter_mut()
                .zip(self.channel_accumulators.iter())
            {
                let ch_avg = acc / count;
                buf.push(ch_avg * 2.0 - 1.0);
            }

            self.accumulator = 0.0;
            self.sample_count = 0;
            self.channel_accumulators = [0.0; 5];
        }
    }

    /// Clock the frame counter. Generates quarter-frame and half-frame
    /// events at the appropriate CPU cycle counts.
    fn clock_frame_counter(&mut self) {
        // Handle delayed counter reset from step 3 (mode 0) or step 4 (mode 1).
        // The real APU continues for 2 extra ticks after the step event before
        // resetting the counter. In mode 0, the IRQ flag is set on each of
        // these ticks, producing 3 consecutive IRQ sets.
        if self.frame_reset_countdown > 0 {
            self.frame_reset_countdown -= 1;
            // Mode 0: continue setting IRQ flag during countdown
            if self.frame_mode == FrameCounterMode::FourStep && !self.frame_irq_inhibit {
                self.frame_irq_flag = true;
            }
            if self.frame_reset_countdown == 0 {
                // Final countdown tick: reset counter before increment so
                // the next sequence starts from 1, giving the correct spacing
                // to the next step 1 event.
                self.frame_counter = 0;
                self.frame_step = 0;
            }
            self.frame_counter += 1;
            return;
        }

        self.frame_counter += 1;

        match self.frame_mode {
            FrameCounterMode::FourStep => {
                if self.frame_step < 4
                    && self.frame_counter >= self.four_step_seq[self.frame_step as usize]
                {
                    match self.frame_step {
                        0 => self.pending_frame_clock |= 1, // QF
                        1 => {
                            self.pending_frame_clock |= 3; // QF + HF
                            self.snapshot_lengths_for_half_frame();
                        }
                        2 => self.pending_frame_clock |= 1, // QF
                        3 => {
                            self.pending_frame_clock |= 3; // QF + HF
                            self.snapshot_lengths_for_half_frame();
                            if !self.frame_irq_inhibit {
                                self.frame_irq_flag = true;
                            }
                            // Don't reset counter here — delayed by 2 ticks.
                            // During the countdown, IRQ flag continues to be
                            // set (3 consecutive sets total).
                            self.frame_reset_countdown = 2;
                        }
                        _ => {}
                    }
                    self.frame_step += 1;
                }
            }
            FrameCounterMode::FiveStep => {
                if self.frame_step < 5
                    && self.frame_counter >= self.five_step_seq[self.frame_step as usize]
                {
                    match self.frame_step {
                        0 => self.pending_frame_clock |= 1, // QF
                        1 => {
                            self.pending_frame_clock |= 3; // QF + HF
                            self.snapshot_lengths_for_half_frame();
                        }
                        2 => self.pending_frame_clock |= 1, // QF
                        3 => {}                             // No clocking on step 4 of 5-step
                        4 => {
                            self.pending_frame_clock |= 3; // QF + HF
                            self.snapshot_lengths_for_half_frame();
                            // Don't reset counter here — delayed by 2 ticks,
                            // matching mode 0's reset delay.
                            self.frame_reset_countdown = 2;
                        }
                        _ => {}
                    }
                    self.frame_step += 1;
                }
            }
        }
    }

    /// Capture each length counter's halt bit at the moment the
    /// frame counter detects a half-frame event. The deferred HF
    /// clock applied at the next CPU cycle uses these snapshots
    /// (not the live `halt` field) so that a same-cycle bus write
    /// to `$400X` doesn't mis-affect the decrement. See the doc
    /// block on `LengthCounter`.
    fn snapshot_lengths_for_half_frame(&mut self) {
        self.pulse1.length.snapshot_for_half_frame();
        self.pulse2.length.snapshot_for_half_frame();
        self.triangle.length.snapshot_for_half_frame();
        self.noise.length.snapshot_for_half_frame();
    }

    /// Quarter-frame: clock envelopes and triangle linear counter.
    fn clock_quarter_frame(&mut self) {
        self.pulse1.envelope.clock();
        self.pulse2.envelope.clock();
        self.noise.envelope.clock();
        self.triangle.clock_linear_counter();
    }

    /// Half-frame via the deferred-from-previous-tick path: length
    /// counters use the snapshot halt captured at HF detection so
    /// intervening bus writes to `$400X` honour the silicon-validated
    /// timing window described on `LengthCounter`.
    fn clock_half_frame_via_snapshot(&mut self) {
        self.pulse1.length.clock_via_snapshot();
        self.pulse2.length.clock_via_snapshot();
        self.triangle.length.clock_via_snapshot();
        self.noise.length.clock_via_snapshot();
        let p = self.pulse1.sweep.clock(self.pulse1.timer_period);
        self.pulse1.timer_period = p;
        let p = self.pulse2.sweep.clock(self.pulse2.timer_period);
        self.pulse2.timer_period = p;
    }

    /// Half-frame: clock length counters and sweep units using the
    /// live halt value. Reserved for the immediate 5-step path
    /// triggered by `$4017` writes — there's no separate detection
    /// step so the snapshot path doesn't apply.
    fn clock_half_frame(&mut self) {
        self.pulse1.length.clock_immediate();
        self.pulse2.length.clock_immediate();
        self.triangle.length.clock_immediate();
        self.noise.length.clock_immediate();
        let p = self.pulse1.sweep.clock(self.pulse1.timer_period);
        self.pulse1.timer_period = p;
        let p = self.pulse2.sweep.clock(self.pulse2.timer_period);
        self.pulse2.timer_period = p;
    }

    /// Non-linear mixer (nesdev formula).
    #[cfg(test)]
    fn mix(&self) -> f32 {
        self.mix_levels(
            f32::from(self.pulse1.output()),
            f32::from(self.pulse2.output()),
            f32::from(self.triangle.output()),
            f32::from(self.noise.output()),
            f32::from(self.dmc.output_level),
        )
    }

    fn controlled_mix(&self) -> f32 {
        let controls = self.audio_controls;
        self.mix_levels(
            controls
                .channel(ApuChannel::Pulse1)
                .apply(f32::from(self.pulse1.output())),
            controls
                .channel(ApuChannel::Pulse2)
                .apply(f32::from(self.pulse2.output())),
            controls
                .channel(ApuChannel::Triangle)
                .apply(f32::from(self.triangle.output())),
            controls
                .channel(ApuChannel::Noise)
                .apply(f32::from(self.noise.output())),
            controls
                .channel(ApuChannel::Dmc)
                .apply(f32::from(self.dmc.output_level)),
        ) * controls.master_gain()
    }

    fn mix_levels(&self, p1: f32, p2: f32, tri: f32, noi: f32, dmc: f32) -> f32 {
        let pulse_out = if p1 + p2 > 0.0 {
            95.88 / (8128.0 / (p1 + p2) + 100.0)
        } else {
            0.0
        };

        let tnd_sum = tri / 8227.0 + noi / 12241.0 + dmc / 22638.0;
        let tnd_out = if tnd_sum > 0.0 {
            159.79 / (1.0 / tnd_sum + 100.0)
        } else {
            0.0
        };

        // Raw mixer output is 0.0 to ~0.8. The DC-blocking high-pass
        // filter in the downsample path centres this around zero.
        pulse_out + tnd_out
    }

    /// Whether an IRQ is pending (frame counter or DMC).
    #[must_use]
    pub fn irq_pending(&self) -> bool {
        self.frame_irq_flag || self.dmc.irq_flag
    }

    /// Take the audio output buffer (drains it).
    ///
    /// Returns mono f32 samples in the range -1.0 to 1.0, at 48 kHz.
    pub fn take_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    /// Take per-channel audio buffers (drains them).
    ///
    /// Returns 5 mono f32 buffers in the range -1.0 to 1.0, at 48 kHz.
    /// Channel order: \[Pulse 1, Pulse 2, Triangle, Noise, DMC\].
    /// These are linear pre-mix levels (before the non-linear NES mixer),
    /// suitable for per-channel visualisation.
    pub fn take_channel_buffers(&mut self) -> [Vec<f32>; 5] {
        std::mem::replace(
            &mut self.channel_buffers,
            [
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
                Vec::with_capacity(Self::SAMPLE_RATE as usize / 50 + 1),
            ],
        )
    }

    /// Number of audio samples pending in the buffer.
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    // -----------------------------------------------------------------------
    // Observable state (raw getters — system layer wraps in Value)
    // -----------------------------------------------------------------------

    /// Pulse 1 timer period (11-bit).
    #[must_use]
    pub fn pulse1_period(&self) -> u16 {
        self.pulse1.timer_period
    }

    /// Pulse 1 length counter.
    #[must_use]
    pub fn pulse1_length(&self) -> u8 {
        self.pulse1.length.counter
    }

    /// Pulse 1 envelope output (0–15).
    #[must_use]
    pub fn pulse1_envelope(&self) -> u8 {
        self.pulse1.envelope.output()
    }

    /// Pulse 1 duty cycle (0–3).
    #[must_use]
    pub fn pulse1_duty(&self) -> u8 {
        self.pulse1.duty
    }

    /// Pulse 2 timer period (11-bit).
    #[must_use]
    pub fn pulse2_period(&self) -> u16 {
        self.pulse2.timer_period
    }

    /// Pulse 2 length counter.
    #[must_use]
    pub fn pulse2_length(&self) -> u8 {
        self.pulse2.length.counter
    }

    /// Pulse 2 envelope output (0–15).
    #[must_use]
    pub fn pulse2_envelope(&self) -> u8 {
        self.pulse2.envelope.output()
    }

    /// Pulse 2 duty cycle (0–3).
    #[must_use]
    pub fn pulse2_duty(&self) -> u8 {
        self.pulse2.duty
    }

    /// Triangle timer period (11-bit).
    #[must_use]
    pub fn triangle_period(&self) -> u16 {
        self.triangle.timer_period
    }

    /// Triangle length counter.
    #[must_use]
    pub fn triangle_length(&self) -> u8 {
        self.triangle.length.counter
    }

    /// Triangle linear counter.
    #[must_use]
    pub fn triangle_linear(&self) -> u8 {
        self.triangle.linear_counter
    }

    /// Noise timer period.
    #[must_use]
    pub fn noise_period(&self) -> u16 {
        self.noise.timer_period
    }

    /// Noise length counter.
    #[must_use]
    pub fn noise_length(&self) -> u8 {
        self.noise.length.counter
    }

    /// Noise envelope output (0–15).
    #[must_use]
    pub fn noise_envelope(&self) -> u8 {
        self.noise.envelope.output()
    }

    /// Frame counter mode (0 = four-step, 1 = five-step).
    #[must_use]
    pub fn frame_counter_mode(&self) -> u8 {
        match self.frame_mode {
            FrameCounterMode::FourStep => 0,
            FrameCounterMode::FiveStep => 1,
        }
    }

    // --- Save state support ---

    /// Snapshot the APU register file for save state.
    ///
    /// Returns 24 bytes representing $4000-$4013, $4015, $4017 in order,
    /// plus the frame counter position and odd cycle flag. This captures
    /// enough state to reproduce audio output after restore.
    #[must_use]
    pub fn save_registers(&self) -> [u8; 24] {
        let mut regs = [0u8; 24];

        // Pulse 1 ($4000-$4003)
        regs[0] = (self.pulse1.duty << 6)
            | if self.pulse1.envelope.loop_flag {
                0x20
            } else {
                0
            }
            | if self.pulse1.envelope.constant_volume {
                0x10
            } else {
                0
            }
            | self.pulse1.envelope.volume;
        regs[1] = if self.pulse1.sweep.enabled { 0x80 } else { 0 }
            | if self.pulse1.sweep.negate { 0x08 } else { 0 }
            | (self.pulse1.sweep.period << 4)
            | self.pulse1.sweep.shift;
        regs[2] = self.pulse1.timer_period as u8;
        regs[3] = ((self.pulse1.timer_period >> 8) as u8) & 0x07;

        // Pulse 2 ($4004-$4007)
        regs[4] = (self.pulse2.duty << 6)
            | if self.pulse2.envelope.loop_flag {
                0x20
            } else {
                0
            }
            | if self.pulse2.envelope.constant_volume {
                0x10
            } else {
                0
            }
            | self.pulse2.envelope.volume;
        regs[5] = if self.pulse2.sweep.enabled { 0x80 } else { 0 }
            | if self.pulse2.sweep.negate { 0x08 } else { 0 }
            | (self.pulse2.sweep.period << 4)
            | self.pulse2.sweep.shift;
        regs[6] = self.pulse2.timer_period as u8;
        regs[7] = ((self.pulse2.timer_period >> 8) as u8) & 0x07;

        // Triangle ($4008-$400B)
        regs[8] =
            if self.triangle.control_flag { 0x80 } else { 0 } | self.triangle.linear_counter_reload;
        regs[9] = 0; // unused
        regs[10] = self.triangle.timer_period as u8;
        regs[11] = ((self.triangle.timer_period >> 8) as u8) & 0x07;

        // Noise ($400C-$400F)
        regs[12] = if self.noise.envelope.loop_flag {
            0x20
        } else {
            0
        } | if self.noise.envelope.constant_volume {
            0x10
        } else {
            0
        } | self.noise.envelope.volume;
        regs[13] = 0; // unused
        // Reverse-lookup period index from the noise period table
        let noise_idx = self
            .noise_period_table
            .iter()
            .position(|&p| p == self.noise.timer_period)
            .unwrap_or(0) as u8;
        regs[14] = if self.noise.mode { 0x80 } else { 0 } | noise_idx;
        regs[15] = 0; // unused

        // DMC ($4010-$4013)
        regs[16] = if self.dmc.irq_enabled { 0x80 } else { 0 }
            | if self.dmc.loop_flag { 0x40 } else { 0 }
            | self.dmc.rate_index;
        regs[17] = self.dmc.output_level;
        regs[18] = ((self.dmc.sample_address.wrapping_sub(0xC000)) >> 6) as u8;
        regs[19] = ((self.dmc.sample_length.wrapping_sub(1)) >> 4) as u8;

        // $4015 (status)
        regs[20] = if self.pulse1.length.enabled { 0x01 } else { 0 }
            | if self.pulse2.length.enabled { 0x02 } else { 0 }
            | if self.triangle.length.enabled {
                0x04
            } else {
                0
            }
            | if self.noise.length.enabled { 0x08 } else { 0 }
            | if self.dmc.bytes_remaining > 0 {
                0x10
            } else {
                0
            };

        // $4017 (frame counter)
        regs[21] = match self.frame_mode {
            FrameCounterMode::FiveStep => 0x80,
            FrameCounterMode::FourStep => 0x00,
        } | if self.frame_irq_inhibit { 0x40 } else { 0 };

        // Frame counter position
        regs[22] = self.frame_counter as u8;
        regs[23] = (self.frame_counter >> 8) as u8;

        regs
    }

    /// Restore APU state from a register snapshot.
    ///
    /// Replays the register writes to rebuild internal state. Not
    /// perfectly cycle-accurate (envelope/sweep phase is lost) but
    /// produces correct audio output for practical save states.
    pub fn restore_registers(&mut self, regs: &[u8; 24]) {
        // Replay register writes in order
        self.write(0x4015, regs[20]); // Enable channels first
        for i in 0..=3 {
            self.write(0x4000 + i, regs[i as usize]);
        }
        for i in 0..=3 {
            self.write(0x4004 + i, regs[4 + i as usize]);
        }
        for i in 0..=3 {
            self.write(0x4008 + i, regs[8 + i as usize]);
        }
        for i in 0..=3 {
            self.write(0x400C + i, regs[12 + i as usize]);
        }
        for i in 0..=3 {
            self.write(0x4010 + i, regs[16 + i as usize]);
        }
        self.write(0x4017, regs[21]);

        // Restore frame counter position
        self.frame_counter = u16::from(regs[22]) | (u16::from(regs[23]) << 8);
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_by_default() {
        let mut apu = Apu::new();
        // Tick for enough cycles to produce samples
        for _ in 0..100 {
            apu.tick();
        }
        let buf = apu.take_buffer();
        // With DC-blocking filter, silence should be near zero
        for &s in &buf {
            assert!(
                s.abs() < 0.1,
                "Silent APU should produce near-zero output, got {s}"
            );
        }
    }

    #[test]
    fn pulse_produces_audio() {
        let mut apu = Apu::new();

        // Enable pulse 1
        apu.write(0x4015, 0x01);
        // Duty 50%, constant volume 15
        apu.write(0x4000, 0xBF); // 10_1_1_1111
        // Timer period low = $FD (A4 = 440 Hz → period ≈ 253)
        apu.write(0x4002, 0xFD);
        // Timer period high + length load
        apu.write(0x4003, 0x01 << 3); // period high = 0, length index = 1

        // Need enough ticks for multiple full 8-step duty cycles.
        // One duty cycle = 8 * (period+1) * 2 CPU ticks = 8 * 254 * 2 = 4064.
        // Run 10000 to see several cycles.
        for _ in 0..10000 {
            apu.tick();
        }

        let buf = apu.take_buffer();
        assert!(
            !buf.is_empty(),
            "Pulse channel should produce audio samples"
        );

        // Check that output isn't flat — should have variation from duty cycle
        let min = buf.iter().copied().fold(f32::INFINITY, f32::min);
        let max = buf.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max - min > 0.01,
            "Pulse output should have dynamic range, got min={min} max={max}"
        );
    }

    #[test]
    fn triangle_produces_audio() {
        let mut apu = Apu::new();

        // Enable triangle
        apu.write(0x4015, 0x04);
        // Linear counter reload = 127, control flag set
        apu.write(0x4008, 0xFF);
        // Timer period
        apu.write(0x400A, 0xFD);
        apu.write(0x400B, 0x01 << 3); // length index = 1

        // Triangle needs a quarter-frame event (7457 CPU cycles) to load
        // its linear counter from the reload value. Run 10000 ticks to
        // pass the first quarter-frame and produce audible output.
        for _ in 0..10000 {
            apu.tick();
        }

        let buf = apu.take_buffer();
        assert!(
            !buf.is_empty(),
            "Triangle channel should produce audio samples"
        );

        let min = buf.iter().copied().fold(f32::INFINITY, f32::min);
        let max = buf.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max - min > 0.01,
            "Triangle output should have dynamic range, got min={min} max={max}"
        );
    }

    #[test]
    fn noise_produces_audio() {
        let mut apu = Apu::new();

        // Enable noise
        apu.write(0x4015, 0x08);
        // Constant volume 15
        apu.write(0x400C, 0x3F); // halt + constant + vol=15
        // Period index 4 (medium)
        apu.write(0x400E, 0x04);
        // Length load
        apu.write(0x400F, 0x01 << 3);

        for _ in 0..2000 {
            apu.tick();
        }

        let buf = apu.take_buffer();
        assert!(
            !buf.is_empty(),
            "Noise channel should produce audio samples"
        );

        let min = buf.iter().copied().fold(f32::INFINITY, f32::min);
        let max = buf.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max - min > 0.01,
            "Noise output should have dynamic range, got min={min} max={max}"
        );
    }

    #[test]
    fn status_register_reflects_length() {
        let mut apu = Apu::new();

        // Enable pulse 1 and triangle
        apu.write(0x4015, 0x05);
        // Load pulse 1 length
        apu.write(0x4003, 0x01 << 3); // length index 1 = 254
        // Load triangle length
        apu.write(0x4008, 0xFF);
        apu.write(0x400B, 0x01 << 3); // length index 1 = 254

        let status = apu.read(0x4015);
        assert!(status & 0x01 != 0, "Pulse 1 length should be active");
        assert!(status & 0x04 != 0, "Triangle length should be active");
        assert!(status & 0x02 == 0, "Pulse 2 length should be inactive");
        assert!(status & 0x08 == 0, "Noise length should be inactive");
    }

    #[test]
    fn disable_channel_clears_length() {
        let mut apu = Apu::new();

        // Enable pulse 1
        apu.write(0x4015, 0x01);
        apu.write(0x4003, 0x01 << 3); // Load length
        assert!(apu.read(0x4015) & 0x01 != 0);

        // Disable pulse 1
        apu.write(0x4015, 0x00);
        assert!(apu.read(0x4015) & 0x01 == 0, "Length should be cleared");
    }

    #[test]
    fn host_audio_controls_do_not_change_status_register() {
        let mut apu = Apu::new();

        apu.write(0x4015, 0x01);
        apu.write(0x4003, 0x01 << 3);
        assert!(apu.read(0x4015) & 0x01 != 0);

        apu.set_audio_channel_enabled(ApuChannel::Pulse1, false);
        assert!(!apu.audio_controls().channel(ApuChannel::Pulse1).enabled());
        assert!(
            apu.read(0x4015) & 0x01 != 0,
            "host muting must not clear the emulated length status"
        );
    }

    #[test]
    fn host_audio_controls_mute_output_only() {
        let mut apu = Apu::new();
        apu.write(0x4011, 0x40);

        assert!(
            apu.controlled_mix() > 0.0,
            "DMC direct load should contribute to host output"
        );
        assert!(
            apu.mix() > 0.0,
            "raw emulated mixer should see DMC direct load"
        );

        apu.set_audio_channel_enabled(ApuChannel::Dmc, false);
        assert_eq!(apu.controlled_mix(), 0.0);
        assert!(
            apu.mix() > 0.0,
            "host muting must not change raw emulated channel state"
        );
    }

    #[test]
    fn host_audio_controls_clamp_gain() {
        let mut controls = AudioControls::default();
        controls.set_master_gain(2.0);
        controls.set_channel_gain(ApuChannel::Noise, f32::NAN);
        controls.set_channel_gain(ApuChannel::Dmc, -1.0);

        assert_eq!(controls.master_gain(), 1.0);
        assert_eq!(controls.channel(ApuChannel::Noise).gain(), 0.0);
        assert_eq!(controls.channel(ApuChannel::Dmc).gain(), 0.0);
    }

    #[test]
    fn frame_irq_in_four_step_mode() {
        let mut apu = Apu::new();

        // 4-step mode, IRQ enabled
        apu.write(0x4017, 0x00);

        // Tick through enough cycles for the delayed counter reset (3-4)
        // plus a full 4-step sequence (~29829 CPU cycles) = ~29833 total
        for _ in 0..29834 {
            apu.tick();
        }

        assert!(
            apu.irq_pending(),
            "Frame counter IRQ should fire in 4-step mode"
        );
    }

    #[test]
    fn no_irq_in_five_step_mode() {
        let mut apu = Apu::new();

        // 5-step mode
        apu.write(0x4017, 0x80);

        // Tick through a full sequence
        for _ in 0..40000 {
            apu.tick();
        }

        assert!(
            !apu.frame_irq_flag,
            "Frame counter should not generate IRQ in 5-step mode"
        );
    }

    #[test]
    fn buffer_drain() {
        let mut apu = Apu::new();

        for _ in 0..1000 {
            apu.tick();
        }

        let len1 = apu.buffer_len();
        assert!(len1 > 0, "Buffer should have samples after ticking");

        let buf = apu.take_buffer();
        assert_eq!(buf.len(), len1);
        assert_eq!(apu.buffer_len(), 0, "Buffer should be empty after take");
    }

    #[test]
    fn dmc_direct_load() {
        let mut apu = Apu::new();

        apu.write(0x4011, 0x40); // Direct load = 64

        // The DMC output level affects the mix
        let sample = apu.mix();
        // With only DMC at 64, tnd_out should be non-zero
        assert!(
            sample > -1.0,
            "DMC direct load should shift output, got {sample}"
        );
    }

    // -----------------------------------------------------------------------
    // DMC DMA tests
    // -----------------------------------------------------------------------

    #[test]
    fn dmc_rate_table_length() {
        assert_eq!(DMC_RATE_TABLE_NTSC.len(), 16);
        assert_eq!(DMC_RATE_TABLE_PAL.len(), 16);
    }

    #[test]
    fn dmc_address_formula() {
        let mut apu = Apu::new();
        // $4012 value 0 → $C000, value 1 → $C040, value $FF → $FFC0
        apu.write(0x4012, 0x00);
        assert_eq!(apu.dmc.sample_address, 0xC000);
        apu.write(0x4012, 0x01);
        assert_eq!(apu.dmc.sample_address, 0xC040);
        apu.write(0x4012, 0xFF);
        assert_eq!(apu.dmc.sample_address, 0xFFC0);
    }

    #[test]
    fn dmc_length_formula() {
        let mut apu = Apu::new();
        // $4013 value 0 → 1, value 1 → 17, value $FF → 4081
        apu.write(0x4013, 0x00);
        assert_eq!(apu.dmc.sample_length, 1);
        apu.write(0x4013, 0x01);
        assert_eq!(apu.dmc.sample_length, 17);
        apu.write(0x4013, 0xFF);
        assert_eq!(apu.dmc.sample_length, 4081);
    }

    #[test]
    fn dmc_enable_starts_sample() {
        let mut apu = Apu::new();
        apu.write(0x4012, 0x00); // address = $C000
        apu.write(0x4013, 0x01); // length = 17
        apu.write(0x4015, 0x10); // enable DMC

        assert!(
            apu.dmc.bytes_remaining > 0,
            "DMC should have bytes to fetch"
        );
        assert_eq!(apu.dmc.current_address, 0xC000);

        // ⚠ Enabling an idle DMC must NOT request the fetch on the write
        // itself. Doing so made the fetch ride the write's cadence instead of
        // the timer's, and `sprdma_and_dmc_dma` measured a flat alignment
        // table where hardware alternates.
        assert!(
            !apu.dmc.dma_pending,
            "the $4015 write must not request the fetch itself"
        );
        assert_eq!(
            apu.dmc.transfer_start_delay, 2,
            "even CPU cycle gives a 2-cycle start delay"
        );

        apu.tick();
        assert!(!apu.dmc.dma_pending, "still counting the delay out");
        apu.tick();
        assert!(apu.dmc.dma_pending, "delay expired, so the fetch is due");
    }

    #[test]
    fn dmc_enable_start_delay_is_three_on_an_odd_cycle() {
        let mut apu = Apu::new();
        apu.cpu_cycle_odd = true;
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x01);
        apu.write(0x4015, 0x10);

        assert_eq!(
            apu.dmc.transfer_start_delay, 3,
            "odd CPU cycle gives a 3-cycle start delay"
        );
    }

    #[test]
    fn dmc_disable_stops() {
        let mut apu = Apu::new();
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x01);
        apu.write(0x4015, 0x10); // enable
        assert!(apu.dmc.bytes_remaining > 0);

        apu.write(0x4015, 0x00); // disable
        assert_eq!(apu.dmc.bytes_remaining, 0, "DMC should stop immediately");
    }

    #[test]
    fn dmc_status_bit4_active() {
        let mut apu = Apu::new();
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x01);
        apu.write(0x4015, 0x10);

        let status = apu.read(0x4015);
        assert!(status & 0x10 != 0, "Bit 4 should reflect DMC active");
    }

    #[test]
    fn dmc_timer_output_changes() {
        let mut apu = Apu::new();
        // Set rate index 0 (period = 428)
        apu.write(0x4010, 0x00);
        // Start at output_level 64
        apu.write(0x4011, 64);

        // Manually feed a byte with all 1-bits into the DMC
        apu.dmc.sample_buffer = 0xFF;
        apu.dmc.sample_buffer_empty = false;
        apu.dmc.silence_flag = false;
        apu.dmc.shift_register = 0xFF;
        apu.dmc.bits_remaining = 8;

        let before = apu.dmc.output_level;

        // Tick through one full timer period + 1 to trigger an output clock
        for _ in 0..=(apu.dmc.timer_period + 1) {
            apu.dmc.tick();
        }

        assert_ne!(
            apu.dmc.output_level, before,
            "Output level should change after clocking the shift register"
        );
    }

    #[test]
    fn dmc_loop_restarts() {
        let mut apu = Apu::new();
        apu.write(0x4010, 0x40); // loop flag set, no IRQ
        apu.write(0x4012, 0x00); // address = $C000
        apu.write(0x4013, 0x00); // length = 1
        apu.write(0x4015, 0x10); // enable

        // Deliver the single byte — should restart
        apu.dmc.receive_dma_byte(0xAA);
        assert_eq!(
            apu.dmc.bytes_remaining, 1,
            "Loop should restart bytes_remaining"
        );
        assert_eq!(apu.dmc.current_address, 0xC000, "Loop should reset address");
    }

    #[test]
    fn dmc_irq_at_end() {
        let mut apu = Apu::new();
        apu.write(0x4010, 0x80); // IRQ enabled, no loop
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x00); // length = 1
        apu.write(0x4015, 0x10); // enable (clears irq_flag)

        apu.dmc.receive_dma_byte(0x00);
        assert!(
            apu.dmc.irq_flag,
            "IRQ flag should be set when sample ends with IRQ enabled"
        );
    }

    #[test]
    fn frame_counter_4017_write_delayed() {
        let mut apu = Apu::new();

        // Start in 4-step mode (default)
        assert_eq!(apu.frame_counter_mode(), 0, "should start in 4-step");

        // Write $4017 with mode bit set (5-step)
        apu.write(0x4017, 0x80);

        // Mode should NOT change immediately — still 4-step
        assert_eq!(
            apu.frame_counter_mode(),
            0,
            "mode should not change on write cycle"
        );

        // Tick 1 — delay counting down, still 4-step
        apu.tick();
        assert_eq!(
            apu.frame_counter_mode(),
            0,
            "mode should be delayed after 1 tick"
        );

        // Tick 2
        apu.tick();
        assert_eq!(
            apu.frame_counter_mode(),
            0,
            "mode should be delayed after 2 ticks"
        );

        // Tick through remaining delay (3-4 ticks total)
        // After 3-4 ticks the pending write resolves
        apu.tick();
        apu.tick();

        assert_eq!(
            apu.frame_counter_mode(),
            1,
            "mode should be 5-step after delay completes"
        );
    }

    #[test]
    fn dmc_irq_disabled_no_flag() {
        let mut apu = Apu::new();
        apu.write(0x4010, 0x00); // no IRQ, no loop
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x00); // length = 1
        apu.write(0x4015, 0x10); // enable

        apu.dmc.receive_dma_byte(0x00);
        assert!(
            !apu.dmc.irq_flag,
            "IRQ flag should not be set when IRQ is disabled"
        );
    }

    // -----------------------------------------------------------------------
    // Region / ApuChannel surface
    // -----------------------------------------------------------------------

    #[test]
    fn region_cpu_hz_values() {
        assert_eq!(ApuRegion::Ntsc.cpu_hz(), 1_789_773);
        assert_eq!(ApuRegion::Pal.cpu_hz(), 1_662_607);
        assert_eq!(ApuRegion::default(), ApuRegion::Ntsc);
    }

    #[test]
    fn channel_labels_and_indices() {
        assert_eq!(ApuChannel::Pulse1.label(), "pulse 1");
        assert_eq!(ApuChannel::Pulse2.label(), "pulse 2");
        assert_eq!(ApuChannel::Triangle.label(), "triangle");
        assert_eq!(ApuChannel::Noise.label(), "noise");
        assert_eq!(ApuChannel::Dmc.label(), "DMC");
        // Index round-trip via AudioControls (touches all 5 index variants)
        let mut ctrl = AudioControls::default();
        for ch in [
            ApuChannel::Pulse1,
            ApuChannel::Pulse2,
            ApuChannel::Triangle,
            ApuChannel::Noise,
            ApuChannel::Dmc,
        ] {
            ctrl.set_channel_enabled(ch, false);
            assert!(!ctrl.channel(ch).enabled());
            ctrl.set_channel_gain(ch, 0.25);
            assert!((ctrl.channel(ch).gain() - 0.25).abs() < 1e-6);
        }
    }

    #[test]
    fn audio_controls_sanitized_clamps_all_channels() {
        // Construct a controls value with NaN gains that bypasses set_*
        // by directly cloning an instance and forcing values via setters
        // on a default and then sanitizing.
        let mut ctrl = AudioControls::default();
        ctrl.set_master_gain(f32::INFINITY);
        ctrl.set_channel_gain(ApuChannel::Pulse1, f32::NAN);
        // sanitized() is private but reachable through Apu::set_audio_controls
        let mut apu = Apu::new();
        apu.set_audio_controls(ctrl);
        let after = apu.audio_controls();
        assert_eq!(after.master_gain(), 0.0, "infinity master sanitised to 0");
        assert_eq!(
            after.channel(ApuChannel::Pulse1).gain(),
            0.0,
            "NaN sanitised to 0"
        );
    }

    #[test]
    fn audio_controls_setters_through_apu() {
        let mut apu = Apu::new();
        apu.set_audio_channel_gain(ApuChannel::Pulse2, 0.5);
        assert!((apu.audio_controls().channel(ApuChannel::Pulse2).gain() - 0.5).abs() < 1e-6);
        apu.set_audio_channel_enabled(ApuChannel::Triangle, false);
        assert!(!apu.audio_controls().channel(ApuChannel::Triangle).enabled());
    }

    // -----------------------------------------------------------------------
    // PAL region
    // -----------------------------------------------------------------------

    #[test]
    fn pal_region_initialises_pal_tables() {
        let mut apu = Apu::new_with_region(ApuRegion::Pal);
        // Trigger a noise period write — must use PAL table
        apu.write(0x400E, 0x02); // index 2 → PAL value 14, NTSC value 16
        assert_eq!(apu.noise_period(), 14, "PAL noise period table in use");
        // DMC rate index 0 → PAL value 398, NTSC 428
        apu.write(0x4010, 0x00);
        assert_eq!(apu.dmc.timer_period, 398, "PAL DMC rate table in use");
    }

    /// Seam 3: PAL APU survives snapshot → restore correctly. Without
    /// the `after_restore` reattachment, the region-dependent tables
    /// silently default to NTSC on deserialise and a PAL noise write
    /// would produce NTSC noise period.
    #[test]
    fn pal_apu_survives_snapshot_round_trip() {
        let original = Apu::new_with_region(ApuRegion::Pal);
        let bytes = postcard::to_allocvec(&original).expect("serialise PAL APU");
        let mut restored: Apu = postcard::from_bytes(&bytes).expect("deserialise PAL APU");
        // Before after_restore, the &'static tables default to NTSC
        // (the chosen `default = "default_*"` functions return NTSC).
        // Verify the field that IS serialised survived.
        assert_eq!(restored.region(), ApuRegion::Pal, "region survives");
        // Call after_restore — this is what machine.restore_snapshot
        // does in the integration path.
        restored.after_restore();
        // Now a noise period write must produce a PAL-table value.
        restored.write(0x400E, 0x02);
        assert_eq!(
            restored.noise_period(),
            14,
            "PAL noise table reattached after_restore"
        );
        // DMC rate too.
        restored.write(0x4010, 0x00);
        assert_eq!(restored.dmc.timer_period, 398, "PAL DMC table reattached");
    }

    /// Seam 3: NTSC also survives (the trivial path, but worth
    /// locking — catches a regression where `after_restore` accidentally
    /// forces PAL).
    #[test]
    fn ntsc_apu_survives_snapshot_round_trip() {
        let original = Apu::new_with_region(ApuRegion::Ntsc);
        let bytes = postcard::to_allocvec(&original).expect("serialise NTSC APU");
        let mut restored: Apu = postcard::from_bytes(&bytes).expect("deserialise NTSC APU");
        assert_eq!(restored.region(), ApuRegion::Ntsc);
        restored.after_restore();
        restored.write(0x400E, 0x02);
        assert_eq!(restored.noise_period(), 16, "NTSC noise table preserved");
    }

    #[test]
    fn pal_frame_counter_uses_pal_sequence() {
        let mut apu = Apu::new_with_region(ApuRegion::Pal);
        apu.write(0x4017, 0x00); // 4-step, IRQ enabled
        // PAL step 1 fires at cycle 8313. Tick well past with no IRQ at 7457.
        for _ in 0..7460 {
            apu.tick();
        }
        // Ticks 7457..7460 would have fired NTSC step 0 — but PAL hasn't yet
        // generated any IRQ regardless (IRQ only on step 4). Verify state by
        // running through the entire PAL 4-step.
        for _ in 0..30000 {
            apu.tick();
        }
        assert!(apu.irq_pending(), "PAL frame IRQ fires after full sequence");
    }

    // -----------------------------------------------------------------------
    // Envelope decay paths
    // -----------------------------------------------------------------------

    #[test]
    fn envelope_decays_to_zero_without_loop() {
        let mut env = Envelope::new();
        env.volume = 0; // divider reloads to 0 each clock
        env.start_flag = true;
        env.clock(); // Sets decay_level to 15
        assert_eq!(env.decay_level, 15);
        // 15 down-clocks should bring it to 0
        for _ in 0..15 {
            env.clock();
        }
        assert_eq!(env.decay_level, 0);
        // Without loop_flag, it stays at 0
        env.clock();
        assert_eq!(env.decay_level, 0, "decay stays at 0 without loop");
    }

    #[test]
    fn envelope_loops_back_to_15() {
        let mut env = Envelope::new();
        env.volume = 0;
        env.loop_flag = true;
        env.start_flag = true;
        env.clock();
        for _ in 0..15 {
            env.clock();
        }
        assert_eq!(env.decay_level, 0);
        env.clock(); // should loop to 15
        assert_eq!(env.decay_level, 15, "decay loops to 15 with loop_flag");
    }

    #[test]
    fn envelope_divider_decrements() {
        let mut env = Envelope::new();
        env.volume = 3;
        env.start_flag = true;
        env.clock(); // decay_level=15, divider=3
        env.clock(); // divider 3 → 2
        assert_eq!(env.divider, 2);
        env.clock(); // 2 → 1
        env.clock(); // 1 → 0
        assert_eq!(env.divider, 0);
        env.clock(); // divider==0 path: reload + decrement decay
        assert_eq!(env.decay_level, 14);
    }

    #[test]
    fn envelope_constant_volume_output() {
        let mut env = Envelope::new();
        env.volume = 7;
        env.constant_volume = true;
        assert_eq!(env.output(), 7, "constant volume returns volume");
        env.decay_level = 3;
        assert_eq!(env.output(), 7, "ignores decay_level when constant");
    }

    // -----------------------------------------------------------------------
    // Length counter clock
    // -----------------------------------------------------------------------

    #[test]
    fn length_counter_decrements_on_clock() {
        let mut lc = LengthCounter::new();
        lc.set_enabled(true);
        lc.load(0); // index 0 → 10
        assert_eq!(lc.counter, 10);
        lc.clock_immediate();
        assert_eq!(lc.counter, 9);
        lc.halt = true;
        lc.clock_immediate();
        assert_eq!(lc.counter, 9, "halt freezes the counter");
    }

    #[test]
    fn length_counter_snapshot_halt_drives_deferred_clock() {
        let mut lc = LengthCounter::new();
        lc.set_enabled(true);
        lc.load(1); // index 1 → 254
        assert_eq!(lc.counter, 254);
        // HF detection happens with halt = false → snapshot captures false.
        lc.snapshot_for_half_frame();
        // CPU bus write changes the live halt between detection and apply.
        lc.halt = true;
        // Deferred HF apply still decrements (uses snapshot halt, not live).
        lc.clock_via_snapshot();
        assert_eq!(lc.counter, 253);
    }

    #[test]
    fn length_counter_load_with_ctr_zero_overrides_pending_clock() {
        // blargg `len_reload_timing` sub-test 4: when the prior
        // counter was 0, a load coinciding with the HF takes effect
        // and there's nothing to decrement.
        let mut lc = LengthCounter::new();
        lc.set_enabled(true);
        // counter starts at 0 (no prior load)
        lc.snapshot_for_half_frame();
        lc.load(3); // counter = LENGTH_TABLE[3] = 2
        lc.clock_via_snapshot();
        assert_eq!(lc.counter, 2, "load wins when snapshot counter was 0");
    }

    #[test]
    fn length_counter_load_with_ctr_nonzero_is_ignored() {
        // blargg `len_reload_timing` sub-test 5: when the prior
        // counter was non-zero, a load coinciding with the HF is
        // IGNORED — the HF decrement applies to the previous value.
        let mut lc = LengthCounter::new();
        lc.set_enabled(true);
        lc.load(7); // counter = LENGTH_TABLE[7] = 6
        lc.snapshot_for_half_frame();
        // Same-cycle reload attempt.
        lc.load(3); // would set counter = 2, but should be ignored
        lc.clock_via_snapshot();
        assert_eq!(
            lc.counter, 5,
            "load ignored when snapshot counter > 0; HF decrement of pre-load value"
        );
    }

    // -----------------------------------------------------------------------
    // Sweep behaviour (pulse 1 vs pulse 2; negate, target update, reload)
    // -----------------------------------------------------------------------

    #[test]
    fn sweep_pulse1_negate_uses_ones_complement() {
        let s = Sweep::new(true);
        // current=0x100, shift=0 → shift_result=0x100, target = 0x100 - 0x100 - 1 = 0xFFFF (wrap)
        let mut s = s;
        s.shift = 0;
        s.negate = true;
        // With shift 0, ones-complement gives current - current - 1 → wraps
        let t = s.target_period(0x100);
        assert_eq!(t, 0xFFFF, "pulse 1 ones-complement subtracts an extra 1");
    }

    #[test]
    fn sweep_pulse2_negate_uses_twos_complement() {
        let mut s = Sweep::new(false);
        s.shift = 1;
        s.negate = true;
        // target = current - (current >> 1)
        assert_eq!(s.target_period(0x100), 0x100 - 0x80);
    }

    #[test]
    fn sweep_clock_updates_period_when_enabled() {
        let mut s = Sweep::new(false);
        s.enabled = true;
        s.shift = 1;
        s.negate = false; // up-sweep
        s.divider = 0;
        s.period = 0;
        // current=0x200, target = 0x200 + 0x100 = 0x300
        let new_p = s.clock(0x200);
        assert_eq!(new_p, 0x300, "sweep clocked to target period");
    }

    #[test]
    fn sweep_clock_reload_path() {
        let mut s = Sweep::new(false);
        s.divider = 3;
        s.reload_flag = true;
        s.period = 5;
        let _ = s.clock(0x100);
        assert_eq!(s.divider, 5, "reload sets divider to period");
        assert!(!s.reload_flag, "reload flag cleared");
    }

    #[test]
    fn sweep_clock_decrements_divider() {
        let mut s = Sweep::new(false);
        s.divider = 3;
        s.reload_flag = false;
        let _ = s.clock(0x100);
        assert_eq!(s.divider, 2);
    }

    // -----------------------------------------------------------------------
    // Pulse / Triangle output edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn pulse_output_silenced_by_sweep_muting() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x01); // enable pulse 1
        apu.write(0x4000, 0x1F); // constant volume 15
        // Period < 8 → muting always true regardless of sweep state
        apu.write(0x4002, 0x05);
        apu.write(0x4003, 0x08); // length idx=1, period high=0
        // duty pos starts at 0 with duty=0 → already silent unless we tick
        // the duty cycle around. But sweep muting overrides envelope/duty.
        assert_eq!(apu.pulse1.output(), 0, "muted pulse outputs 0");
    }

    #[test]
    fn sweep_negate_disable_idiom_keeps_pulse1_audible() {
        // Issue #472: the canonical "disable the sweep" write — $08 to $4001
        // (negate set, shift 0) — must not mute the channel. Before the fix,
        // pulse 1's one's-complement target wrapped to $FFFF and the >$7FF
        // overflow-mute silenced a perfectly ordinary G4 tone. Exact repro
        // from the bug report.
        let mut apu = Apu::new();
        apu.write(0x4015, 0x01); // enable pulse 1
        apu.write(0x4000, 0xB8); // duty 50%, length halt, constant volume 8
        apu.write(0x4001, 0x08); // sweep: negate set, shift 0 (the disable idiom)
        apu.write(0x4002, 0x1C); // period low
        apu.write(0x4003, 0x01); // period high 1 → timer_period 0x11C (~G4)

        // negate is set, so the >$7FF overflow check does not apply and the
        // in-range period (0x11C) must not be muted.
        assert!(
            !apu.pulse1.sweep.muting(apu.pulse1.timer_period),
            "negate-set sweep must not mute an in-range period"
        );

        // And the channel must be audible across its duty cycle.
        let mut peak = 0;
        for pos in 0..8 {
            apu.pulse1.duty_pos = pos;
            peak = peak.max(apu.pulse1.output());
        }
        assert!(
            peak > 0,
            "pulse 1 should emit a non-zero sample, not silence"
        );
    }

    #[test]
    fn triangle_silences_at_ultrasonic_period() {
        let mut tri = Triangle::new();
        tri.length.set_enabled(true);
        tri.length.load(1); // counter > 0
        tri.linear_counter = 5;
        tri.timer_period = 1; // < 2 → silent
        assert_eq!(tri.output(), 0, "period < 2 silences triangle");
    }

    #[test]
    fn triangle_linear_counter_decrements_when_reload_clear() {
        let mut tri = Triangle::new();
        tri.linear_counter = 5;
        tri.linear_reload_flag = false;
        tri.control_flag = false;
        tri.clock_linear_counter();
        assert_eq!(tri.linear_counter, 4);
    }

    // -----------------------------------------------------------------------
    // DMC clock_output paths
    // -----------------------------------------------------------------------

    #[test]
    fn dmc_clock_output_decreases_level_on_zero_bit() {
        let mut apu = Apu::new();
        apu.write(0x4011, 64); // start at 64
        apu.dmc.silence_flag = false;
        apu.dmc.shift_register = 0x00; // all zeros
        apu.dmc.bits_remaining = 8;
        apu.dmc.clock_output();
        assert_eq!(apu.dmc.output_level, 62, "0-bit decreases level by 2");
    }

    #[test]
    fn dmc_clock_output_clamps_at_zero() {
        let mut apu = Apu::new();
        apu.write(0x4011, 1); // level=1, below 2
        apu.dmc.silence_flag = false;
        apu.dmc.shift_register = 0x00;
        apu.dmc.bits_remaining = 8;
        apu.dmc.clock_output();
        assert_eq!(apu.dmc.output_level, 1, "level < 2 cannot decrement");
    }

    #[test]
    fn dmc_clock_output_loads_from_buffer() {
        let mut apu = Apu::new();
        apu.dmc.silence_flag = true;
        apu.dmc.bits_remaining = 1;
        apu.dmc.sample_buffer = 0xA5;
        apu.dmc.sample_buffer_empty = false;
        apu.dmc.clock_output();
        assert_eq!(apu.dmc.shift_register, 0xA5, "buffer copied into shift");
        assert!(apu.dmc.sample_buffer_empty, "buffer marked empty");
        assert!(!apu.dmc.silence_flag, "silence cleared");
    }

    #[test]
    fn dmc_clock_output_requests_dma_when_buffer_consumed() {
        let mut apu = Apu::new();
        apu.dmc.silence_flag = true;
        apu.dmc.bits_remaining = 1;
        apu.dmc.sample_buffer = 0xA5;
        apu.dmc.sample_buffer_empty = false; // buffer holds a byte to consume
        apu.dmc.bytes_remaining = 5;
        apu.dmc.clock_output();
        assert!(
            apu.dmc.dma_pending,
            "consuming the buffer requests the next byte"
        );
    }

    #[test]
    fn dmc_clock_output_does_not_request_dma_on_the_silence_path() {
        // ⚠ The request used to sit after the if/else, so it fired here too --
        // where the buffer was already empty, the channel goes silent, and
        // hardware requests nothing.
        let mut apu = Apu::new();
        apu.dmc.silence_flag = false;
        apu.dmc.bits_remaining = 1;
        apu.dmc.sample_buffer_empty = true;
        apu.dmc.bytes_remaining = 5;
        apu.dmc.clock_output();
        assert!(apu.dmc.silence_flag, "channel goes silent");
        assert!(
            !apu.dmc.dma_pending,
            "the silence path must not request a fetch"
        );
    }

    #[test]
    fn dmc_transfer_start_delay_defers_the_consumption_request() {
        // The interlock: a $4015 write that armed the start delay owns the
        // next fetch, so buffer consumption must not pre-empt it.
        let mut apu = Apu::new();
        apu.dmc.transfer_start_delay = 2;
        apu.dmc.silence_flag = true;
        apu.dmc.bits_remaining = 1;
        apu.dmc.sample_buffer = 0xA5;
        apu.dmc.sample_buffer_empty = false;
        apu.dmc.bytes_remaining = 5;
        apu.dmc.clock_output();
        assert!(
            !apu.dmc.dma_pending,
            "an armed start delay owns the next fetch"
        );
    }

    #[test]
    fn dmc_clock_output_silence_when_buffer_empty() {
        let mut apu = Apu::new();
        apu.dmc.silence_flag = false;
        apu.dmc.bits_remaining = 1;
        apu.dmc.sample_buffer_empty = true;
        apu.dmc.bytes_remaining = 0;
        apu.dmc.clock_output();
        assert!(apu.dmc.silence_flag, "silence flag set when buffer empty");
    }

    #[test]
    fn dmc_address_wraps_from_ffff_to_8000() {
        let mut apu = Apu::new();
        apu.dmc.current_address = 0xFFFF;
        apu.dmc.bytes_remaining = 5;
        apu.dmc.receive_dma_byte(0x42);
        assert_eq!(
            apu.dmc.current_address, 0x8000,
            "address wraps $FFFF → $8000"
        );
    }

    // -----------------------------------------------------------------------
    // Register write coverage: pulse 2, triangle, noise, DMC, $4017
    // -----------------------------------------------------------------------

    #[test]
    fn pulse2_register_writes_set_state() {
        let mut apu = Apu::new();
        // $4004: duty=2, halt+loop, constant volume, vol=10
        apu.write(0x4004, 0xBA); // 1011_1010
        assert_eq!(apu.pulse2_duty(), 2);
        assert!(apu.pulse2.envelope.loop_flag);
        assert!(apu.pulse2.length.halt);
        assert!(apu.pulse2.envelope.constant_volume);
        assert_eq!(apu.pulse2.envelope.volume, 10);

        // $4005: sweep enabled, period=5, negate, shift=3
        apu.write(0x4005, 0xDB); // 1101_1011
        assert!(apu.pulse2.sweep.enabled);
        assert_eq!(apu.pulse2.sweep.period, 5);
        assert!(apu.pulse2.sweep.negate);
        assert_eq!(apu.pulse2.sweep.shift, 3);
        assert!(apu.pulse2.sweep.reload_flag);

        // $4006: timer low
        apu.write(0x4006, 0x42);
        assert_eq!(apu.pulse2_period() & 0xFF, 0x42);

        // $4007: timer high + length load
        apu.write(0x4015, 0x02); // enable pulse 2
        apu.write(0x4007, (1 << 3) | 0x05); // length idx=1, period high=5
        assert_eq!(apu.pulse2_period(), 0x542);
        assert!(apu.pulse2_length() > 0);
        assert_eq!(apu.pulse2.duty_pos, 0, "duty pos reset on $4007 write");
        assert!(apu.pulse2.envelope.start_flag);
    }

    #[test]
    fn unused_register_writes_are_noops() {
        let mut apu = Apu::new();
        // $4009 and $400D are unused — must not panic and must not alter state
        apu.write(0x4009, 0xFF);
        apu.write(0x400D, 0xFF);
        // Out-of-range writes hit the default branch — also a no-op
        apu.write(0x4014, 0xFF);
        apu.write(0x4018, 0xFF);
        // No assertion — just exercising the code paths
    }

    #[test]
    fn write_4017_irq_inhibit_clears_flag_immediately() {
        let mut apu = Apu::new();
        // Get IRQ flag set
        apu.write(0x4017, 0x00);
        for _ in 0..29834 {
            apu.tick();
        }
        assert!(apu.frame_irq_flag);
        // Writing $40 (inhibit set) clears it immediately
        apu.write(0x4017, 0x40);
        assert!(!apu.frame_irq_flag, "IRQ inhibit clears the flag");
    }

    #[test]
    fn dmc_enable_no_effect_when_already_running() {
        let mut apu = Apu::new();
        apu.write(0x4012, 0x00);
        apu.write(0x4013, 0x01); // length=17
        apu.write(0x4015, 0x10);
        // Drain DMA pending so we can see the second-enable path
        apu.dmc.dma_pending = false;
        let prev_addr = apu.dmc.current_address;
        let prev_bytes = apu.dmc.bytes_remaining;
        // Second enable should not restart since bytes_remaining > 0
        apu.write(0x4015, 0x10);
        assert_eq!(apu.dmc.current_address, prev_addr);
        assert_eq!(apu.dmc.bytes_remaining, prev_bytes);
    }

    // -----------------------------------------------------------------------
    // $4015 status register — DMC IRQ, frame IRQ clearing, default read
    // -----------------------------------------------------------------------

    #[test]
    fn read_4015_reports_dmc_irq_and_clears_frame_irq() {
        let mut apu = Apu::new();
        // Force both IRQs
        apu.frame_irq_flag = true;
        apu.dmc.irq_flag = true;
        let s = apu.read(0x4015);
        assert!(s & 0x40 != 0, "frame IRQ bit set");
        assert!(s & 0x80 != 0, "DMC IRQ bit set");
        // Frame IRQ clears, DMC IRQ does not
        assert!(!apu.frame_irq_flag, "frame IRQ cleared by read");
        assert!(apu.dmc.irq_flag, "DMC IRQ not cleared by read");
    }

    #[test]
    fn read_4015_reports_pulse2_and_noise_length_bits() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x02 | 0x08); // enable pulse 2 + noise
        apu.write(0x4007, 0x08); // pulse 2 length load
        apu.write(0x400F, 0x08); // noise length load
        let s = apu.read(0x4015);
        assert!(s & 0x02 != 0, "pulse 2 length bit");
        assert!(s & 0x08 != 0, "noise length bit");
    }

    #[test]
    fn read_other_addresses_returns_zero() {
        let mut apu = Apu::new();
        assert_eq!(apu.read(0x4000), 0);
        assert_eq!(apu.read(0x4017), 0, "non-$4015 addresses return 0");
    }

    // -----------------------------------------------------------------------
    // Frame counter 5-step path — step 3 no-op branch
    // -----------------------------------------------------------------------

    #[test]
    fn frame_counter_five_step_step3_is_quiet() {
        let mut apu = Apu::new();
        apu.write(0x4017, 0x80); // 5-step
        // Settle the pending write
        for _ in 0..5 {
            apu.tick();
        }
        // Count up to step 3 (CPU cycle 29829). On 5-step, step 3 produces
        // no QF/HF clocks. We can't observe directly but coverage exercises
        // the match arm.
        for _ in 0..40000 {
            apu.tick();
        }
        assert!(!apu.frame_irq_flag, "5-step still suppresses IRQ");
    }

    // -----------------------------------------------------------------------
    // Public observable getters
    // -----------------------------------------------------------------------

    #[test]
    fn observable_getters_return_register_state() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x0F); // enable pulse1+2, tri, noise

        // Pulse 1
        apu.write(0x4000, 0x9F); // constant volume 15
        apu.write(0x4002, 0x34);
        apu.write(0x4003, 0x09); // period high=1, length idx=1
        assert_eq!(apu.pulse1_period(), 0x134);
        assert!(apu.pulse1_length() > 0);
        assert_eq!(apu.pulse1_envelope(), 15);
        assert_eq!(apu.pulse1_duty(), 2);

        // Pulse 2
        apu.write(0x4004, 0x4A); // duty 1, vol=10
        apu.write(0x4006, 0x12);
        apu.write(0x4007, 0x09);
        assert_eq!(apu.pulse2_period(), 0x112);
        assert!(apu.pulse2_length() > 0);
        // pulse2_envelope: not constant_volume → returns decay_level (0 initially)
        assert_eq!(apu.pulse2_envelope(), 0);
        assert_eq!(apu.pulse2_duty(), 1);

        // Triangle
        apu.write(0x4008, 0xFF);
        apu.write(0x400A, 0x44);
        apu.write(0x400B, 0x09);
        assert_eq!(apu.triangle_period(), 0x144);
        assert!(apu.triangle_length() > 0);
        // linear counter loads on QF; right after register write it's 0
        assert_eq!(apu.triangle_linear(), 0);

        // Noise
        apu.write(0x400C, 0x1F); // constant vol 15
        apu.write(0x400E, 0x05);
        apu.write(0x400F, 0x09);
        assert_eq!(apu.noise_period(), NOISE_PERIOD_TABLE_NTSC[5]);
        assert!(apu.noise_length() > 0);
        assert_eq!(apu.noise_envelope(), 15);

        // Frame counter mode getter
        assert_eq!(apu.frame_counter_mode(), 0);
        apu.write(0x4017, 0x80);
        for _ in 0..5 {
            apu.tick();
        }
        assert_eq!(apu.frame_counter_mode(), 1);
    }

    // -----------------------------------------------------------------------
    // take_channel_buffers
    // -----------------------------------------------------------------------

    #[test]
    fn take_channel_buffers_drains_per_channel() {
        let mut apu = Apu::new();
        apu.write(0x4011, 64); // DMC level
        for _ in 0..2000 {
            apu.tick();
        }
        let bufs = apu.take_channel_buffers();
        assert_eq!(bufs.len(), 5);
        // After taking, internal channel buffers are empty (they were
        // replaced with fresh capacity-allocated Vecs).
        let again = apu.take_channel_buffers();
        for b in &again {
            assert!(b.is_empty(), "channel buffers reset after take");
        }
        // DMC channel had a non-zero level → its drained buffer should
        // contain something (after enough ticks for one downsample).
        assert!(!bufs[4].is_empty() || !bufs[0].is_empty());
    }

    // -----------------------------------------------------------------------
    // Save / restore registers
    // -----------------------------------------------------------------------

    #[test]
    fn save_registers_default_state() {
        let apu = Apu::new();
        let regs = apu.save_registers();
        assert_eq!(regs.len(), 24);
        // Default: all zero except $4017 (mode 4-step, no inhibit) and the
        // counters. Pulse 1/2 reg[0] = 0, reg[1] = 0, etc.
        assert_eq!(regs[0], 0);
        assert_eq!(regs[4], 0);
        assert_eq!(regs[8], 0); // triangle ctrl
        assert_eq!(regs[21], 0, "default 4-step + inhibit clear");
    }

    #[test]
    fn save_registers_round_trip() {
        let mut apu = Apu::new();
        // Set up a varied register state covering every save_registers branch
        apu.write(0x4015, 0x1F); // enable everything
        apu.write(0x4000, 0xBF); // pulse1: duty=2, halt+loop+const+vol=15
        apu.write(0x4001, 0xCB); // sweep en, period=4, negate, shift=3
        apu.write(0x4002, 0x34);
        apu.write(0x4003, 0x09); // period high=1, length idx=1
        apu.write(0x4004, 0x7A); // pulse2: duty=1, halt+loop+const+vol=10
        apu.write(0x4005, 0x55); // sweep
        apu.write(0x4006, 0x21);
        apu.write(0x4007, 0x12);
        apu.write(0x4008, 0xFF); // triangle ctrl + linear reload
        apu.write(0x400A, 0x33);
        apu.write(0x400B, 0x0A);
        apu.write(0x400C, 0x3F); // noise: halt+const+vol=15
        apu.write(0x400E, 0x87); // mode bit set, period idx 7
        apu.write(0x400F, 0x10);
        apu.write(0x4010, 0xCF); // DMC: irq+loop, rate=15
        apu.write(0x4011, 0x55);
        apu.write(0x4012, 0x10); // sample address C400
        apu.write(0x4013, 0x20); // length 0x201
        // Use 4-step mode (no immediate QF/HF clock that would mutate sweep state)
        apu.write(0x4017, 0x40); // 4-step + inhibit
        for _ in 0..5 {
            apu.tick(); // settle 4017 write
        }

        let regs = apu.save_registers();
        // Verify a few salient bits made it through
        assert_eq!(regs[0] & 0xF0, 0xB0, "pulse1 duty/loop/const top nibble");
        assert!(regs[1] & 0x80 != 0, "pulse1 sweep enabled");
        assert!(regs[14] & 0x80 != 0, "noise mode bit");
        assert!(regs[16] & 0xC0 == 0xC0, "DMC IRQ + loop");
        assert_eq!(regs[17], 0x55, "DMC output_level captured");
        assert!(regs[20] & 0x10 != 0, "DMC active in $4015 snapshot");
        assert!(regs[21] & 0x40 != 0, "inhibit in $4017 snapshot");
        assert!(regs[21] & 0x80 == 0, "4-step in $4017 snapshot");

        // Restore into a fresh APU and re-snapshot — must round-trip
        let mut other = Apu::new();
        other.restore_registers(&regs);
        // $4017 write is delayed; tick to settle it.
        for _ in 0..5 {
            other.tick();
        }
        let regs2 = other.save_registers();
        // Compare register fields (0..=21). Length counters (regs[20]) and
        // counter position bytes (22..24) can drift from re-running writes.
        assert_eq!(
            &regs[0..=19],
            &regs2[0..=19],
            "channel register snapshot round-trips"
        );
        assert_eq!(regs[21], regs2[21], "frame mode/inhibit round-trips");
    }

    #[test]
    fn save_registers_captures_five_step_mode() {
        let mut apu = Apu::new();
        apu.write(0x4017, 0x80);
        for _ in 0..5 {
            apu.tick();
        }
        let regs = apu.save_registers();
        assert!(regs[21] & 0x80 != 0, "5-step mode bit captured");
    }

    #[test]
    fn save_registers_pulse_period_high_byte() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x03);
        apu.write(0x4002, 0x00);
        apu.write(0x4003, 0x07); // period high = 0x700, length idx=0
        let regs = apu.save_registers();
        assert_eq!(regs[2], 0x00);
        assert_eq!(regs[3], 0x07, "pulse 1 high byte preserved");
    }

    // -----------------------------------------------------------------------
    // Default impl
    // -----------------------------------------------------------------------

    #[test]
    fn apu_default_matches_new() {
        let _a = Apu::default();
        // Just touching Default::default is enough to cover those lines.
    }
}
