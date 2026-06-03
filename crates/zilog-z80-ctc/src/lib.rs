//! Zilog Z80 CTC (Z8430) — Counter/Timer Circuit.
//!
//! Four independent counter/timer channels with Z80-mode-2 vectored,
//! daisy-chained interrupts. Used across the Z80 home-computer scene to
//! turn peripheral signals (VDP frame interrupt, baud clocks, cassette
//! timing) into prioritised, vectored CPU interrupts.
//!
//! First consumer in this workspace is the **Sord M5**, whose Monitor ROM
//! runs `IM 2` with `I = $70` and routes the TMS9918A `/INT` line into a
//! CTC channel's `CLK/TRG` input; the CTC counts those edges and supplies
//! the matching IM 2 vector so the BIOS reaches its VBlank handler. The
//! same chip sits in the MSX, Memotech MTX and Tatung Einstein.
//!
//! Authored from the Zilog Z8430 spec, cross-checked against the reference
//! library's *Z80 Microprocessor* (Macmillan, 1988) ch.14 "Programmable
//! Timers and Counters", which reproduces the Zilog Z80 CTC Technical
//! Manual (1982). No prior implementation existed in the archives or the
//! vendored reference emulators — this is a fresh write.
//!
//! # Register interface
//!
//! The chip occupies four consecutive I/O ports; `CS1,CS0` (= host address
//! lines `A1,A0`) select the channel. The host decodes its own port base
//! and passes the channel index 0-3 to [`Ctc::read`] / [`Ctc::write`].
//!
//! A write is interpreted by its low bit:
//!
//! - **D0 = 1** — *channel control word* (see [`Ctc::write`] for the bit
//!   layout). If D2 (time-constant-follows) is set, the **next** write to
//!   the same channel is taken as the 8-bit time constant.
//! - **D0 = 0** — *interrupt vector*, valid only when written to channel 0.
//!   Bits 7-3 are the user vector base; the CTC supplies bits 2-1 (the
//!   channel number) at interrupt-acknowledge time; bit 0 is always 0.
//!
//! A read returns the channel's **live down-counter** value.
//!
//! # Clock model
//!
//! [`Ctc::tick`] advances the chip by one system-clock cycle (the Z80
//! clock on these machines) and must be called once per CPU T-state. In
//! timer mode the prescaler divides that clock by 16 or 256; in counter
//! mode the prescaler is bypassed and the down-counter decrements on each
//! active edge of the channel's `CLK/TRG` input, sampled from
//! [`Ctc::set_trg`].
//!
//! # Interrupts
//!
//! The host wires the chip's [`Ctc::interrupt`] output into the Z80 `irq`
//! pin (level), calls [`Ctc::acknowledge`] during the interrupt-acknowledge
//! cycle to fetch the vector byte, and calls [`Ctc::reti`] when it observes
//! a `RETI` (`ED 4D`) opcode fetch so the daisy chain releases the
//! in-service channel. The Z80 exports no RETI signal, so RETI detection is
//! the host's responsibility.

/// Number of channels in a single CTC.
pub const NUM_CHANNELS: usize = 4;

/// One CTC channel: control state, time constant, down-counter, prescaler,
/// and the two daisy-chain interrupt latches.
#[derive(Clone)]
struct Channel {
    /// D7 — interrupt enabled.
    int_enable: bool,
    /// D6 — counter mode (`true`) vs timer mode (`false`).
    counter_mode: bool,
    /// D5 — timer prescaler: 256 (`true`) vs 16 (`false`).
    prescaler_256: bool,
    /// D4 — active `CLK/TRG` edge: rising (`true`) vs falling (`false`).
    rising_edge: bool,
    /// D3 — timer trigger: external edge (`true`) vs automatic (`false`).
    external_trigger: bool,

    /// Latched time constant (0 represents 256).
    time_constant: u8,
    /// Whether the next write to this channel is the time constant.
    expecting_tc: bool,

    /// Live down-counter (1..=256; 0 only transiently before reload).
    counter: u16,
    /// Prescaler countdown (timer mode).
    prescaler: u16,

    /// Channel is armed and counting.
    running: bool,
    /// External-trigger timer loaded but waiting for its first edge.
    waiting_for_trigger: bool,
    /// Channel sits in the reset state, ignoring control words until a
    /// time constant arrives (hardware reset, or software reset with D2=0).
    reset_state: bool,

    /// Interrupt requested (counter reached zero, awaiting acknowledge).
    int_pending: bool,
    /// Interrupt acknowledged, service routine running (until `RETI`).
    int_in_service: bool,

    /// `ZC/TO` output pulse for this cycle (channels 0-2; channel 3 has no
    /// output pin and this stays `false`).
    zc_to: bool,
    /// Last sampled `CLK/TRG` level, for edge detection.
    prev_trg: bool,
    /// Current `CLK/TRG` input level, set by the host.
    trg: bool,
}

impl Channel {
    fn new() -> Self {
        Self {
            int_enable: false,
            counter_mode: false,
            prescaler_256: false,
            rising_edge: false,
            external_trigger: false,
            time_constant: 0,
            expecting_tc: false,
            counter: 256,
            prescaler: 16,
            running: false,
            waiting_for_trigger: false,
            reset_state: true,
            int_pending: false,
            int_in_service: false,
            zc_to: false,
            prev_trg: false,
            trg: false,
        }
    }

    /// Prescaler divisor for timer mode.
    fn prescaler_divisor(&self) -> u16 {
        if self.prescaler_256 { 256 } else { 16 }
    }

    /// Load the down-counter from the time constant (0 → 256).
    fn reload(&mut self) {
        self.counter = if self.time_constant == 0 {
            256
        } else {
            u16::from(self.time_constant)
        };
    }

    /// Decrement the down-counter by one count, handling zero-crossing:
    /// pulse `ZC/TO`, reload, and raise the interrupt if enabled.
    fn decrement(&mut self) {
        self.counter -= 1;
        if self.counter == 0 {
            self.zc_to = true;
            self.reload();
            if self.int_enable {
                self.int_pending = true;
            }
        }
    }
}

/// A Zilog Z80 CTC.
pub struct Ctc {
    channels: [Channel; NUM_CHANNELS],
    /// Interrupt vector base (bits 7-3); bits 2-1 are filled per channel,
    /// bit 0 is always 0.
    vector_base: u8,
}

impl Ctc {
    /// Create a CTC in its power-on / hardware-reset state: all channels
    /// stopped, interrupts disabled, awaiting a control word + time
    /// constant.
    #[must_use]
    pub fn new() -> Self {
        Self {
            channels: core::array::from_fn(|_| Channel::new()),
            vector_base: 0,
        }
    }

    /// Write to a channel register (`channel` = 0-3, the `CS1,CS0` lines).
    ///
    /// The low bit of `value` selects the interpretation:
    ///
    /// | Bit | Control word (D0=1)                                  |
    /// |-----|-----------------------------------------------------|
    /// | D7  | Interrupt enable                                    |
    /// | D6  | Mode: 1 = counter, 0 = timer                        |
    /// | D5  | Prescaler (timer): 1 = ÷256, 0 = ÷16                |
    /// | D4  | `CLK/TRG` edge: 1 = rising, 0 = falling             |
    /// | D3  | Timer trigger: 1 = external edge, 0 = automatic     |
    /// | D2  | Time constant follows this write                    |
    /// | D1  | Software reset                                       |
    /// | D0  | 1 = control word                                    |
    ///
    /// With D0=0 the byte is an interrupt vector (honoured only on
    /// channel 0): bits 7-3 set [`Self::vector_base`].
    pub fn write(&mut self, channel: u8, value: u8) {
        let ch = &mut self.channels[(channel & 0x03) as usize];

        if ch.expecting_tc {
            // Second half of a "time constant follows" sequence.
            ch.time_constant = value;
            ch.expecting_tc = false;
            ch.reset_state = false;
            ch.reload();
            ch.prescaler = ch.prescaler_divisor();
            if ch.counter_mode {
                // Counter mode counts external edges immediately.
                ch.running = true;
                ch.waiting_for_trigger = false;
            } else if ch.external_trigger {
                // Timer waits for the first active CLK/TRG edge.
                ch.running = true;
                ch.waiting_for_trigger = true;
            } else {
                // Automatic timer starts on the following clock.
                ch.running = true;
                ch.waiting_for_trigger = false;
            }
            return;
        }

        if value & 0x01 == 0 {
            // Interrupt vector — channel 0 only (per the datasheet, the
            // vector is addressed by writing channel 0 with D0=0).
            if channel & 0x03 == 0 {
                self.vector_base = value & 0xF8;
            }
            return;
        }

        // Control word (D0 = 1).
        ch.int_enable = value & 0x80 != 0;
        ch.counter_mode = value & 0x40 != 0;
        ch.prescaler_256 = value & 0x20 != 0;
        ch.rising_edge = value & 0x10 != 0;
        ch.external_trigger = value & 0x08 != 0;
        let tc_follows = value & 0x04 != 0;
        let software_reset = value & 0x02 != 0;

        if software_reset {
            ch.running = false;
            ch.waiting_for_trigger = false;
            ch.zc_to = false;
            // A software reset withdraws any request still pending; an
            // in-service routine is left alone (cleared by RETI).
            ch.int_pending = false;
            if !tc_follows {
                ch.reset_state = true;
            }
        }

        if tc_follows {
            ch.expecting_tc = true;
        }
    }

    /// Read a channel register: the live down-counter value (1..=256
    /// returned as 0..=255, i.e. a full count reads back as 0).
    #[must_use]
    pub fn read(&self, channel: u8) -> u8 {
        // The counter holds 1..=256; the bus is 8 bits, so 256 reads as 0,
        // matching the silicon (a freshly reloaded ÷256 channel reads 0).
        (self.channels[(channel & 0x03) as usize].counter & 0xFF) as u8
    }

    /// Set a channel's `CLK/TRG` input level. The host calls this every
    /// tick (or whenever the line changes); [`Self::tick`] samples it for
    /// edges. Counts external edges in counter mode and starts
    /// external-trigger timers.
    pub fn set_trg(&mut self, channel: u8, level: bool) {
        self.channels[(channel & 0x03) as usize].trg = level;
    }

    /// Advance the chip by one system-clock cycle.
    pub fn tick(&mut self) {
        for ch in &mut self.channels {
            // ZC/TO is a one-cycle pulse.
            ch.zc_to = false;

            // Timer mode: prescaler divides the system clock.
            if ch.running && !ch.counter_mode && !ch.waiting_for_trigger {
                ch.prescaler -= 1;
                if ch.prescaler == 0 {
                    ch.prescaler = ch.prescaler_divisor();
                    ch.decrement();
                }
            }

            // Edge detection on CLK/TRG.
            let edge = if ch.rising_edge {
                ch.trg && !ch.prev_trg
            } else {
                !ch.trg && ch.prev_trg
            };
            ch.prev_trg = ch.trg;

            if edge {
                if ch.counter_mode && ch.running {
                    ch.decrement();
                } else if ch.waiting_for_trigger {
                    // First active edge starts an external-trigger timer.
                    ch.waiting_for_trigger = false;
                    ch.prescaler = ch.prescaler_divisor();
                }
            }
        }
    }

    /// Whether the chip is currently requesting an interrupt (the `INT`
    /// pin, active). True when some channel has a pending request that the
    /// daisy chain has not masked behind a higher-priority channel.
    #[must_use]
    pub fn interrupt(&self) -> bool {
        let mut iei = true;
        for ch in &self.channels {
            if iei && ch.int_pending {
                return true;
            }
            // IEO drops (blocking lower priority) while this channel has a
            // pending request or a service routine in progress.
            iei = iei && !(ch.int_pending || ch.int_in_service);
        }
        false
    }

    /// Interrupt-acknowledge: return the vector for the highest-priority
    /// requesting channel and move it from "pending" to "in service".
    ///
    /// Returns the open-bus value `$FF` if nothing is actually pending
    /// (the host should only call this when [`Self::interrupt`] is true).
    pub fn acknowledge(&mut self) -> u8 {
        let mut iei = true;
        for (index, ch) in self.channels.iter_mut().enumerate() {
            if iei && ch.int_pending {
                ch.int_pending = false;
                ch.int_in_service = true;
                // Vector: base (D7-D3) | channel number in D2-D1 | D0 = 0.
                return (self.vector_base & 0xF8) | ((index as u8) << 1);
            }
            iei = iei && !(ch.int_pending || ch.int_in_service);
        }
        0xFF
    }

    /// `RETI` observed: clear the highest-priority in-service channel,
    /// releasing the daisy chain for lower-priority interrupts.
    pub fn reti(&mut self) {
        for ch in &mut self.channels {
            if ch.int_in_service {
                ch.int_in_service = false;
                return;
            }
        }
    }

    /// The current interrupt vector base (bits 7-3 meaningful).
    #[must_use]
    pub fn vector_base(&self) -> u8 {
        self.vector_base
    }

    /// The live down-counter for a channel (1..=256).
    #[must_use]
    pub fn counter(&self, channel: u8) -> u16 {
        self.channels[(channel & 0x03) as usize].counter
    }

    /// Whether a channel is armed and counting.
    #[must_use]
    pub fn running(&self, channel: u8) -> bool {
        self.channels[(channel & 0x03) as usize].running
    }

    /// Whether a channel has interrupts enabled.
    #[must_use]
    pub fn int_enabled(&self, channel: u8) -> bool {
        self.channels[(channel & 0x03) as usize].int_enable
    }

    /// Whether a channel is in counter mode (`true`) or timer mode.
    #[must_use]
    pub fn counter_mode(&self, channel: u8) -> bool {
        self.channels[(channel & 0x03) as usize].counter_mode
    }

    /// A channel's `ZC/TO` output pulse for the current cycle. Channel 3
    /// has no output pin and always reads `false`.
    #[must_use]
    pub fn zc_to(&self, channel: u8) -> bool {
        let c = channel & 0x03;
        if c == 3 {
            return false;
        }
        self.channels[c as usize].zc_to
    }

    /// Serialize CTC state for save states.
    #[must_use]
    pub fn save_state(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(1 + NUM_CHANNELS * 16);
        data.push(self.vector_base);
        for ch in &self.channels {
            let flags = u8::from(ch.int_enable)
                | u8::from(ch.counter_mode) << 1
                | u8::from(ch.prescaler_256) << 2
                | u8::from(ch.rising_edge) << 3
                | u8::from(ch.external_trigger) << 4
                | u8::from(ch.expecting_tc) << 5
                | u8::from(ch.running) << 6
                | u8::from(ch.waiting_for_trigger) << 7;
            let latches = u8::from(ch.reset_state)
                | u8::from(ch.int_pending) << 1
                | u8::from(ch.int_in_service) << 2
                | u8::from(ch.zc_to) << 3
                | u8::from(ch.prev_trg) << 4
                | u8::from(ch.trg) << 5;
            data.push(flags);
            data.push(latches);
            data.push(ch.time_constant);
            data.extend_from_slice(&ch.counter.to_le_bytes());
            data.extend_from_slice(&ch.prescaler.to_le_bytes());
        }
        data
    }

    /// Restore CTC state from a save state.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is too short.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        let needed = 1 + NUM_CHANNELS * 7;
        if data.len() < needed {
            return Err("CTC state truncated".into());
        }
        let mut p = 0;
        self.vector_base = data[p];
        p += 1;
        for ch in &mut self.channels {
            let flags = data[p];
            p += 1;
            let latches = data[p];
            p += 1;
            ch.int_enable = flags & 0x01 != 0;
            ch.counter_mode = flags & 0x02 != 0;
            ch.prescaler_256 = flags & 0x04 != 0;
            ch.rising_edge = flags & 0x08 != 0;
            ch.external_trigger = flags & 0x10 != 0;
            ch.expecting_tc = flags & 0x20 != 0;
            ch.running = flags & 0x40 != 0;
            ch.waiting_for_trigger = flags & 0x80 != 0;
            ch.reset_state = latches & 0x01 != 0;
            ch.int_pending = latches & 0x02 != 0;
            ch.int_in_service = latches & 0x04 != 0;
            ch.zc_to = latches & 0x08 != 0;
            ch.prev_trg = latches & 0x10 != 0;
            ch.trg = latches & 0x20 != 0;
            ch.time_constant = data[p];
            p += 1;
            ch.counter = u16::from_le_bytes([data[p], data[p + 1]]);
            p += 2;
            ch.prescaler = u16::from_le_bytes([data[p], data[p + 1]]);
            p += 2;
        }
        Ok(p)
    }
}

impl Default for Ctc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Program a channel as a timer: control word then time constant.
    fn program_timer(ctc: &mut Ctc, channel: u8, prescaler_256: bool, tc: u8, int: bool) {
        let mut cw = 0x05; // D2 (TC follows) + D0 (control word)
        if int {
            cw |= 0x80;
        }
        if prescaler_256 {
            cw |= 0x20;
        }
        ctc.write(channel, cw);
        ctc.write(channel, tc);
    }

    #[test]
    fn new_ctc_is_idle() {
        let ctc = Ctc::new();
        assert!(!ctc.interrupt());
        for c in 0..4 {
            assert!(!ctc.running(c));
        }
    }

    #[test]
    fn timer_div16_period_matches_time_constant() {
        let mut ctc = Ctc::new();
        // ÷16 prescaler, time constant 3 → one ZC every 48 system clocks.
        program_timer(&mut ctc, 0, false, 3, false);
        assert!(ctc.running(0));

        let mut pulses = 0;
        for _ in 0..48 {
            ctc.tick();
            if ctc.zc_to(0) {
                pulses += 1;
            }
        }
        assert_eq!(pulses, 1, "exactly one ZC/TO in 16*3 = 48 clocks");
    }

    #[test]
    fn timer_div256_period() {
        let mut ctc = Ctc::new();
        // ÷256, TC 2 → ZC every 512 clocks.
        program_timer(&mut ctc, 1, true, 2, false);
        let mut pulses = 0;
        for _ in 0..512 {
            ctc.tick();
            if ctc.zc_to(1) {
                pulses += 1;
            }
        }
        assert_eq!(pulses, 1);
    }

    #[test]
    fn time_constant_zero_means_256() {
        let mut ctc = Ctc::new();
        // ÷16, TC 0 → 256, so 16*256 = 4096 clocks per ZC.
        program_timer(&mut ctc, 0, false, 0, false);
        let mut pulses = 0;
        for _ in 0..4096 {
            ctc.tick();
            if ctc.zc_to(0) {
                pulses += 1;
            }
        }
        assert_eq!(pulses, 1);
    }

    #[test]
    fn counter_mode_decrements_on_trg_edges() {
        let mut ctc = Ctc::new();
        // Counter mode (D6), interrupt enabled (D7), rising edge (D4),
        // TC follows (D2), control word (D0): 1101_0101.
        ctc.write(2, 0b1101_0101);
        ctc.write(2, 3); // time constant 3
        assert!(ctc.running(2));
        assert!(ctc.counter_mode(2));

        // Three rising edges → one interrupt.
        for _ in 0..3 {
            ctc.set_trg(2, false);
            ctc.tick();
            ctc.set_trg(2, true);
            ctc.tick();
        }
        assert!(ctc.interrupt(), "counter reached zero, interrupt pending");
    }

    #[test]
    fn counter_tc1_interrupts_every_edge() {
        // The Sord M5 arrangement: VDP /INT into a counter-mode channel
        // with time constant 1 → one vectored interrupt per edge.
        let mut ctc = Ctc::new();
        ctc.write(0, 0x00); // vector base = $00 (written to channel 0)
        ctc.write(1, 0b1101_0101); // ch1: int, counter, rising edge, TC follows
        ctc.write(1, 1); // TC = 1

        for frame in 0..3 {
            ctc.set_trg(1, false);
            ctc.tick();
            ctc.set_trg(1, true); // rising edge = one count
            ctc.tick();
            assert!(ctc.interrupt(), "frame {frame}: interrupt pending");
            let vec = ctc.acknowledge();
            assert_eq!(vec, 0x02, "channel 1 vector = base | (1<<1)");
            assert!(!ctc.interrupt(), "cleared after acknowledge");
            ctc.reti();
        }
    }

    #[test]
    fn vector_encodes_channel_number() {
        let mut ctc = Ctc::new();
        ctc.write(0, 0x70); // vector base $70 (written to channel 0, D0=0)
        assert_eq!(ctc.vector_base(), 0x70);

        // Make each channel request and check its vector offset.
        for ch in 0u8..4 {
            ctc.write(ch, 0b1101_0101); // int, counter, rising edge, TC follows
            ctc.write(ch, 1); // TC = 1
            ctc.set_trg(ch, false);
            ctc.tick();
            ctc.set_trg(ch, true);
            ctc.tick();
        }
        // Highest priority first: channel 0 → $70, then 1 → $72, etc.
        for ch in 0u8..4 {
            assert!(ctc.interrupt());
            let vec = ctc.acknowledge();
            assert_eq!(vec, 0x70 | (ch << 1), "channel {ch} vector");
            ctc.reti();
        }
    }

    #[test]
    fn daisy_chain_priority_blocks_lower_channels() {
        let mut ctc = Ctc::new();
        ctc.write(0, 0x40);
        // Arm channels 1 and 3 in counter mode, rising edge, TC 1.
        for ch in [1u8, 3] {
            ctc.write(ch, 0b1101_0101);
            ctc.write(ch, 1);
        }
        // Fire channel 3 first, then channel 1.
        ctc.set_trg(3, true);
        ctc.tick();
        ctc.set_trg(1, true);
        ctc.tick();

        // Both pending; acknowledge must serve channel 1 (higher priority).
        assert!(ctc.interrupt());
        assert_eq!(ctc.acknowledge(), 0x40 | (1 << 1));

        // Channel 3 is now blocked behind channel 1's in-service latch.
        assert!(
            !ctc.interrupt(),
            "lower channel masked while ch1 in service"
        );

        // RETI on channel 1 releases channel 3.
        ctc.reti();
        assert!(ctc.interrupt());
        assert_eq!(ctc.acknowledge(), 0x40 | (3 << 1));
    }

    #[test]
    fn disabled_interrupt_still_pulses_zc_to() {
        let mut ctc = Ctc::new();
        program_timer(&mut ctc, 0, false, 1, false); // int disabled
        let mut pulses = 0;
        for _ in 0..16 {
            ctc.tick();
            if ctc.zc_to(0) {
                pulses += 1;
            }
        }
        assert_eq!(pulses, 1);
        assert!(!ctc.interrupt(), "no interrupt when D7 clear");
    }

    #[test]
    fn channel_three_has_no_zc_to_output() {
        let mut ctc = Ctc::new();
        program_timer(&mut ctc, 3, false, 1, true);
        // Channel 3 still interrupts but never exposes a ZC/TO pulse.
        let mut saw_pulse = false;
        for _ in 0..16 {
            ctc.tick();
            if ctc.zc_to(3) {
                saw_pulse = true;
            }
        }
        assert!(!saw_pulse, "channel 3 has no ZC/TO output pin");
        assert!(ctc.interrupt(), "but its interrupt still fires");
    }

    #[test]
    fn external_trigger_timer_waits_for_edge() {
        let mut ctc = Ctc::new();
        // Timer, external trigger (D3), rising edge (D4), TC follows, ctrl.
        ctc.write(0, 0b0001_1101);
        ctc.write(0, 2); // TC = 2, ÷16

        // No edge yet — counter must not move.
        for _ in 0..100 {
            ctc.tick();
        }
        assert!(!ctc.zc_to(0));

        // Provide the start edge; counting begins on the next clock and
        // runs as a ÷16 timer (TC 2 → 32 clocks per pulse).
        ctc.set_trg(0, true);
        ctc.tick(); // the trigger edge itself starts the timer, no count
        let mut pulses = 0;
        for _ in 0..32 {
            ctc.tick();
            if ctc.zc_to(0) {
                pulses += 1;
            }
        }
        assert_eq!(pulses, 1);
    }

    #[test]
    fn software_reset_stops_channel() {
        let mut ctc = Ctc::new();
        program_timer(&mut ctc, 0, false, 1, true);
        assert!(ctc.running(0));
        // Software reset: D1 set, D0 set, no TC follows.
        ctc.write(0, 0b0000_0011);
        assert!(!ctc.running(0));
        for _ in 0..64 {
            ctc.tick();
        }
        assert!(!ctc.interrupt(), "reset channel produces no interrupt");
    }

    #[test]
    fn read_returns_live_counter() {
        let mut ctc = Ctc::new();
        program_timer(&mut ctc, 0, false, 10, false); // ÷16, TC 10
        assert_eq!(ctc.read(0), 10);
        // After 16 clocks the counter has decremented once.
        for _ in 0..16 {
            ctc.tick();
        }
        assert_eq!(ctc.read(0), 9);
    }

    #[test]
    fn save_load_round_trip() {
        let mut ctc = Ctc::new();
        ctc.write(0, 0x70);
        program_timer(&mut ctc, 1, true, 5, true);
        ctc.write(2, 0b1100_0101);
        ctc.write(2, 1);
        for _ in 0..40 {
            ctc.tick();
        }

        let saved = ctc.save_state();
        let mut restored = Ctc::new();
        let consumed = restored.load_state(&saved).expect("load");
        assert_eq!(consumed, saved.len());
        assert_eq!(restored.vector_base(), 0x70);
        assert_eq!(restored.counter(1), ctc.counter(1));
        assert_eq!(restored.running(2), ctc.running(2));
        assert_eq!(restored.read(1), ctc.read(1));
    }

    #[test]
    fn load_state_rejects_truncated() {
        let mut ctc = Ctc::new();
        assert!(ctc.load_state(&[0u8; 4]).is_err());
    }
}
