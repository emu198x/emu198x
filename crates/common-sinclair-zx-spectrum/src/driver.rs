//! The `SpectrumDriver` trait — shared master-clock run loop for every
//! Spectrum family machine.
//!
//! Every Spectrum variant (48K, 128K, +2, +2A/+2B/+3, Pentagon,
//! Scorpion, Timex TC2048/TC2068/TS2068) has a nearly-identical
//! per-frame cadence:
//!
//! - Tick the ULA twice per CPU T-state (7 MHz on the 48K family,
//!   ~7.09 MHz on the 128K family).
//! - Tick the CPU on the same half-cycle edges, gated on the ULA's
//!   contention line for stock Sinclair hardware or unconditionally
//!   on Pentagon / Scorpion.
//! - Advance the tape once per CPU T-state and
//!   post audio samples when the tape EAR level changes.
//! - Tick any AY-3-8912 PSG every two CPU T-states.
//!
//! The public API retains its historical `halfcycle` names, but `hc`
//! advances at the master-crystal rate. A divide-by-four T-state has
//! CPU half-cycle edges at phases 0 and 2. A divide-by-five T-state
//! uses phases 0 and 2, leaving an alternating 2/3-master-tick gap.
//! Nothing currently observes the idle master ticks, so this chooses
//! the earlier of the two possible odd-divider polarities without
//! claiming that polarity as a hardware finding.
//!
//! Before Phase 0.6 this loop lived duplicated in each machine crate's
//! `pub fn run_frame`. The seven copies differed only in which
//! `TIMING_*` constant they used, whether they gated on
//! `cpu_clock_active()`, and whether they had an AY to tick. Every
//! cadence fix had to land in seven places.
//!
//! This trait lifts the cadence into one place. Each machine
//! implements a handful of short hooks that expose its specific chip
//! set (ULA, Z80, tape, audio, optional AY), and picks up `run_frame`
//! for free.
//!
//! The trait is deliberately scoped to the Spectrum family — it is
//! *not* a cross-system abstraction. See
//! `knowledge/decisions/system-specific-run-loops.md` for why each system
//! family writes its own loop, and why within-family sharing is
//! explicitly allowed.

use crate::timing::{FramePosition, FrameTiming};

/// Shared per-frame run loop implemented by every Spectrum variant.
///
/// To participate, a machine exposes eight short methods describing
/// its chip set and picks up `run_frame` as a provided method. The
/// default `contended` and `cpu_clock_active` cover uncontended
/// variants; the default `tick_peripherals` is a no-op awaiting Phase
/// 0.7's peripheral bus.
pub trait SpectrumDriver {
    /// Timing descriptor for this machine's frame.
    fn frame_timing(&self) -> &FrameTiming;

    /// Typed view of the current frame position. Legacy `hc` hooks remain
    /// during the migration so callers can move incrementally.
    fn frame_position(&self) -> FramePosition {
        FramePosition::new(self.hc(), self.frame_timing())
    }

    /// Current master-clock counter within the frame. Starts at 0 and
    /// counts up to `frame_hc`. `run_frame` reads this on every
    /// iteration of the loop.
    fn hc(&self) -> u32;

    /// Mutable slot for the master-clock counter. `run_frame`
    /// increments it on every iteration and corrects any end-of-frame
    /// overshoot when the loop exits.
    fn hc_mut(&mut self) -> &mut u32;

    /// Total master-clock ticks in one frame for this machine's ULA.
    ///
    /// Differs per variant:
    /// - 48K / Scorpion / TC2048 / TC2068: `224 × 312 × 4 = 279_552`
    /// - TS2068: `224 × 264 × 4 = 236_544` (US NTSC)
    /// - 128K / +2 / +2A / +2B / +3: `228 × 311 × 5 = 354_540`
    /// - Pentagon: `224 × 320 × 4 = 286_720`
    fn frame_hc(&self) -> u32;

    /// Master-clock ticks per CPU T-state — `4` on every variant whose master
    /// crystal is divided down to the 3.5 MHz Z80, and `5` on the 128K
    /// family whose 17.7 MHz crystal divides by 5. Used by
    /// `advance_tstates` to translate caller-friendly T-state counts
    /// into the counter units the loop runs in.
    fn halfcycles_per_tstate(&self) -> u32;

    /// Does this machine have ULA contention?
    ///
    /// Default `true` — stock Sinclair and Amstrad ULAs gate the
    /// CPU's clock on badline / screen-fetch cycles. Pentagon and
    /// Scorpion override to `false`; when false, `cpu_clock_active`
    /// is never consulted and the CPU receives every scheduled edge.
    fn contended(&self) -> bool {
        true
    }

    /// Tick the ULA one half-cycle.
    ///
    /// Typical body: `self.ula.tick(&self.memory, self.z80.addr,
    /// self.z80.mreq, self.z80.iorq, &mut self.framebuffer)`.
    fn tick_ula(&mut self);

    /// Is the CPU clock currently active?
    ///
    /// Only consulted when `contended()` returns true. Default
    /// returns `true`, which lets uncontended machines skip overriding
    /// this method — when contention is off the driver never reads it.
    fn cpu_clock_active(&self) -> bool {
        true
    }

    /// Tick the CPU one half-cycle and handle any resulting bus
    /// transaction.
    ///
    /// Typical body: `self.z80.tick(); self.handle_bus();` where
    /// `handle_bus` is a private machine method that inspects the
    /// Z80's signal pins and performs memory / I/O reads and writes.
    fn tick_cpu_and_bus(&mut self);

    /// Copy the ULA's interrupt line onto the Z80's IRQ pin.
    ///
    /// Runs on every scheduled half-cycle edge regardless of contention, because
    /// the Z80 latches IRQ level at its own schedule — it needs to
    /// see the line change even when its clock is gated off.
    fn feed_irq(&mut self);

    /// T-state boundary hook — called once per CPU T-state.
    ///
    /// The `hc` argument is the master-clock counter at the second
    /// CPU half-cycle edge. Implementors convert it through their
    /// `FrameTiming` to get a T-state number for audio writes. Typical
    /// body:
    ///
    /// ```ignore
    /// self.tape.advance_tstates(1);
    /// let tstate = TIMING.hc_to_tstates(hc);
    /// if tstate & 1 == 0 { self.ay.tick(); } // if this variant has AY
    /// let ear = self.tape.ear_level();
    /// if ear != self.last_ear {
    ///     self.last_ear = ear;
    ///     self.audio.set_level(tstate, self.speaker_level());
    /// }
    /// ```
    fn on_tstate(&mut self, position: FramePosition);

    /// Per-T-state hook for future peripherals (Beta disk polling,
    /// µPD765A rotation, mouse delta accumulation, …). Called from
    /// the same T-state gate as `on_tstate`. Default
    /// no-op — no machine currently ticks a peripheral in
    /// `run_frame`. Phase 0.7's peripheral bus work is the first
    /// consumer; leaving the hook in place now avoids a trait-API
    /// change later.
    fn tick_peripherals(&mut self) {}

    /// End-of-frame ULA housekeeping.
    ///
    /// Typical body: `self.ula.end_frame();`. Called once, after the
    /// main loop exits but before the `hc` overshoot correction.
    fn end_frame_ula(&mut self);

    /// End-of-frame side-effects other than the ULA — typically an
    /// audio buffer flush. Default no-op. Called once per frame after
    /// `end_frame_ula` and before the `hc` overshoot correction.
    ///
    /// Machines that carry a `BeeperAudio` buffer override this with:
    ///
    /// ```ignore
    /// fn on_end_frame(&mut self) {
    ///     self.audio.end_frame(&mut self.audio_frame);
    /// }
    /// ```
    fn on_end_frame(&mut self) {}

    /// Run exactly one frame at native speed.
    ///
    /// **Provided method — do not override.** The cadence is:
    ///
    /// - Loop while `hc < frame_hc`:
    ///     - At CPU half-cycle phases 0 and `divisor / 2`:
    ///         - `tick_ula`
    ///         - If CPU clock is active (or uncontended), `tick_cpu_and_bus`
    ///         - `feed_irq`
    ///         - At the second phase (T-state boundary), `on_tstate(hc)`
    ///         - `tick_peripherals`
    ///     - `hc += 1`
    /// - `end_frame_ula`
    /// - Correct `hc` overshoot (`hc -= frame_hc`).
    fn run_frame(&mut self) {
        let frame_hc = self.frame_timing().halfcycles_per_frame;

        while self.hc() < frame_hc {
            self.tick_one_halfcycle();
        }

        self.end_frame_ula();
        self.on_end_frame();
        *self.hc_mut() -= frame_hc;
    }

    /// Advance one master-clock tick without
    /// frame-wrap handling. Callers that need to run for a specific
    /// number of half-cycles (timing tests, single-step tools) use
    /// this in a loop and handle frame wrap themselves.
    fn tick_one_halfcycle(&mut self) {
        let position = self.frame_position();
        let hc = position.halfcycles();
        let divisor = self.frame_timing().cpu_divisor;
        debug_assert!(
            divisor >= 2,
            "CPU divisor must provide two half-cycle phases"
        );
        let phase = hc % divisor;
        // For an odd divisor, choose the earlier edge. The current
        // machine model observes only these scheduled edges, so the
        // opposite 3/2 polarity is behaviourally equivalent.
        let second_halfcycle_phase = divisor / 2;

        if phase == 0 || phase == second_halfcycle_phase {
            self.tick_ula();

            if !self.contended() || self.cpu_clock_active() {
                self.tick_cpu_and_bus();
            }

            self.feed_irq();

            if phase == second_halfcycle_phase {
                self.on_tstate(position);
                self.tick_peripherals();
            }
        }
        *self.hc_mut() += 1;
    }

    /// Advance the machine by an exact number of master-clock ticks,
    /// wrapping the frame and flushing audio when the
    /// counter crosses `frame_hc`.
    fn advance_halfcycles(&mut self, halfcycles: u32) {
        let frame_hc = self.frame_timing().halfcycles_per_frame;
        for _ in 0..halfcycles {
            self.tick_one_halfcycle();
            if self.hc() >= frame_hc {
                self.end_frame_ula();
                self.on_end_frame();
                *self.hc_mut() -= frame_hc;
            }
        }
    }

    /// Advance the machine by an exact number of CPU T-states.
    fn advance_tstates(&mut self, tstates: u32) {
        self.advance_halfcycles(tstates * self.frame_timing().cpu_divisor);
    }
}

#[cfg(test)]
mod tests {
    use super::SpectrumDriver;
    use crate::timing::{FramePosition, FrameTiming, TIMING_48K, TIMING_128K};

    struct CountingDriver {
        hc: u32,
        divisor: u32,
        ula_ticks: u32,
        cpu_ticks: u32,
        irq_feeds: u32,
        tstate_ticks: Vec<u32>,
        peripheral_ticks: u32,
    }

    impl CountingDriver {
        fn new(divisor: u32) -> Self {
            Self {
                hc: 0,
                divisor,
                ula_ticks: 0,
                cpu_ticks: 0,
                irq_feeds: 0,
                tstate_ticks: Vec::new(),
                peripheral_ticks: 0,
            }
        }
    }

    impl SpectrumDriver for CountingDriver {
        fn frame_timing(&self) -> &FrameTiming {
            if self.divisor == TIMING_128K.cpu_divisor {
                &TIMING_128K
            } else {
                &TIMING_48K
            }
        }

        fn hc(&self) -> u32 {
            self.hc
        }

        fn hc_mut(&mut self) -> &mut u32 {
            &mut self.hc
        }

        fn frame_hc(&self) -> u32 {
            self.divisor * 4
        }

        fn halfcycles_per_tstate(&self) -> u32 {
            self.divisor
        }

        fn tick_ula(&mut self) {
            self.ula_ticks += 1;
        }

        fn tick_cpu_and_bus(&mut self) {
            self.cpu_ticks += 1;
        }

        fn feed_irq(&mut self) {
            self.irq_feeds += 1;
        }

        fn on_tstate(&mut self, position: FramePosition) {
            self.tstate_ticks.push(position.halfcycles());
        }

        fn tick_peripherals(&mut self) {
            self.peripheral_ticks += 1;
        }

        fn end_frame_ula(&mut self) {}
    }

    fn assert_two_tstate_cadence(divisor: u32, second_phase: u32) {
        let mut driver = CountingDriver::new(divisor);
        driver.advance_halfcycles(divisor * 2);

        assert_eq!(driver.hc, divisor * 2);
        assert_eq!(driver.ula_ticks, 4);
        assert_eq!(driver.cpu_ticks, 4);
        assert_eq!(driver.irq_feeds, 4);
        assert_eq!(driver.tstate_ticks, [second_phase, divisor + second_phase]);
        assert_eq!(driver.peripheral_ticks, 2);
    }

    #[test]
    fn divide_by_four_preserves_two_even_halfcycles_per_tstate() {
        assert_two_tstate_cadence(4, 2);
    }

    #[test]
    fn divide_by_five_uses_two_halfcycles_per_tstate() {
        assert_two_tstate_cadence(5, 2);
    }
}
