//! The `SpectrumDriver` trait — shared half-cycle run loop for every
//! Spectrum family machine.
//!
//! Every Spectrum variant (48K, 128K, +2, +2A/+2B/+3, Pentagon,
//! Scorpion, Timex TC2048/TC2068/TS2068) has a nearly-identical
//! per-frame cadence:
//!
//! - Tick the ULA on every even half-cycle (7 MHz on 48K-family,
//!   ~8.87 MHz on 128K-family).
//! - Tick the CPU on the same even half-cycle, gated on the ULA's
//!   contention line for stock Sinclair hardware or unconditionally
//!   on Pentagon / Scorpion.
//! - Advance the tape every 4 half-cycles (3.5 MHz T-state rate) and
//!   post audio samples when the tape EAR level changes.
//! - Tick any AY-3-8912 PSG every 8 half-cycles (1.75 MHz).
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

/// Shared per-frame run loop implemented by every Spectrum variant.
///
/// To participate, a machine exposes eight short methods describing
/// its chip set and picks up `run_frame` as a provided method. The
/// default `contended` and `cpu_clock_active` cover uncontended
/// variants; the default `tick_peripherals` is a no-op awaiting Phase
/// 0.7's peripheral bus.
pub trait SpectrumDriver {
    /// Current half-cycle counter within the frame. Starts at 0 and
    /// counts up to `frame_hc`. `run_frame` reads this on every
    /// iteration of the loop.
    fn hc(&self) -> u32;

    /// Mutable slot for the half-cycle counter. `run_frame`
    /// increments it on every iteration and corrects any end-of-frame
    /// overshoot when the loop exits.
    fn hc_mut(&mut self) -> &mut u32;

    /// Total half-cycles in one frame for this machine's ULA.
    ///
    /// Differs per variant:
    /// - 48K / Scorpion / TC2048 / TC2068: `224 × 312 × 4 = 279_552`
    /// - TS2068: `224 × 264 × 4 = 236_544` (US NTSC)
    /// - 128K / +2 / +2A / +2B / +3: `228 × 311 × 5 = 354_540`
    /// - Pentagon: `224 × 320 × 4 = 286_720`
    fn frame_hc(&self) -> u32;

    /// Half-cycles per CPU T-state — `4` on every variant whose master
    /// crystal is divided down to the 3.5 MHz Z80, and `5` on the 128K
    /// family whose 17.7 MHz crystal divides by 5. Used by
    /// `advance_tstates` to translate caller-friendly T-state counts
    /// into the half-cycle units the loop runs in.
    fn halfcycles_per_tstate(&self) -> u32;

    /// Does this machine have ULA contention?
    ///
    /// Default `true` — stock Sinclair and Amstrad ULAs gate the
    /// CPU's clock on badline / screen-fetch cycles. Pentagon and
    /// Scorpion override to `false`; when false, `cpu_clock_active`
    /// is never consulted and the CPU ticks every even half-cycle.
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
    /// Runs every even half-cycle regardless of contention, because
    /// the Z80 latches IRQ level at its own schedule — it needs to
    /// see the line change even when its clock is gated off.
    fn feed_irq(&mut self);

    /// T-state boundary hook — called every 4 half-cycles (3.5 MHz).
    ///
    /// The `hc` argument is the current half-cycle counter (always
    /// even and `% 4 == 2`), which implementors divide by 4 to get a
    /// T-state number for audio writes. Typical body:
    ///
    /// ```ignore
    /// self.tape.advance_tstates(1);
    /// if hc % 8 == 2 { self.ay.tick(); }   // if this variant has AY
    /// let ear = self.tape.ear_level();
    /// if ear != self.last_ear {
    ///     self.last_ear = ear;
    ///     let tstate = hc / 4;
    ///     self.audio.set_level(tstate, self.speaker_level());
    /// }
    /// ```
    fn on_tstate(&mut self, hc: u32);

    /// Per-T-state hook for future peripherals (Beta disk polling,
    /// µPD765A rotation, mouse delta accumulation, …). Called from
    /// the same 3.5 MHz T-state gate as `on_tstate`. Default
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
    ///     - If `hc` is even (ULA tick beat):
    ///         - `tick_ula`
    ///         - If CPU clock is active (or uncontended), `tick_cpu_and_bus`
    ///         - `feed_irq`
    ///         - If `hc % 4 == 2` (T-state boundary), `on_tstate(hc)`
    ///         - `tick_peripherals`
    ///     - `hc += 1`
    /// - `end_frame_ula`
    /// - Correct `hc` overshoot (`hc -= frame_hc`).
    fn run_frame(&mut self) {
        let frame_hc = self.frame_hc();

        while self.hc() < frame_hc {
            self.tick_one_halfcycle();
        }

        self.end_frame_ula();
        self.on_end_frame();
        *self.hc_mut() -= frame_hc;
    }

    /// Advance a single half-cycle of the master oscillator without
    /// frame-wrap handling. Callers that need to run for a specific
    /// number of half-cycles (timing tests, single-step tools) use
    /// this in a loop and handle frame wrap themselves.
    fn tick_one_halfcycle(&mut self) {
        let hc = self.hc();
        if hc & 1 == 0 {
            self.tick_ula();

            if !self.contended() || self.cpu_clock_active() {
                self.tick_cpu_and_bus();
            }

            self.feed_irq();

            if hc % 4 == 2 {
                self.on_tstate(hc);
                self.tick_peripherals();
            }
        }
        *self.hc_mut() += 1;
    }

    /// Advance the machine by an exact number of master-clock
    /// half-cycles, wrapping the frame and flushing audio when the
    /// counter crosses `frame_hc`.
    fn advance_halfcycles(&mut self, halfcycles: u32) {
        let frame_hc = self.frame_hc();
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
        self.advance_halfcycles(tstates * self.halfcycles_per_tstate());
    }
}
