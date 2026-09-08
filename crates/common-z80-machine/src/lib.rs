//! The cadence every Z80 machine shares, and nothing else.
//!
//! `Z80::tick` advances **one half-cycle**. A machine T-state is therefore two
//! ticks, with the interrupt pins fed before each so the CPU samples `/INT`
//! at the instruction boundary inside its own tick rather than half a T-state
//! stale. Twelve machines hand-rolled that loop and nine of them wrote the
//! factor of two wrong, because every one was asked the same question and
//! given no help answering it (`knowledge/decisions/
//! z80-machines-should-share-a-cadence-driver.md`). The Spectrum family
//! never had the bug, because its `SpectrumDriver` owns the cadence and a
//! machine only fills in hooks.
//!
//! [`Z80Machine`] is that driver with the ULA and contention specifics
//! removed. A machine implements the hooks in the order its hardware runs
//! them, and picks up [`Z80Machine::advance_tstates`] and
//! [`Z80Machine::tick_one_halfcycle`]; it keeps its own `run_frame`, because
//! how a frame is paced (a fixed T-state budget, a VDP raster wrap, an NMI
//! display) is the machine's business — see
//! `knowledge/decisions/system-specific-run-loops.md`, which admits this
//! crate as a building block precisely because it owns no frame loop.
//!
//! Chip scheduling is deliberately not here. The machines schedule their
//! chips three different ways (a 3:2 dot accumulator, an exact-Hz
//! accumulator, a ÷2 phase toggle); each keeps its own inside the hooks.

#![forbid(unsafe_code)]

/// The Z80 cadence, implemented by a machine that owns a `Z80`.
///
/// Per machine T-state, [`tick_one_halfcycle`](Self::tick_one_halfcycle) runs
/// the hooks in this order, which is the order the hand-rolled loops ran
/// them, so a port changes no timing:
///
/// 1. [`before_tstate`](Self::before_tstate) — once, at the first CPU edge.
/// 2. At each of the two CPU edges:
///    [`feed_interrupt_pins`](Self::feed_interrupt_pins), then
///    [`tick_cpu_and_bus`](Self::tick_cpu_and_bus) if
///    [`cpu_clock_active`](Self::cpu_clock_active), then
///    [`tick_chips_halfcycle`](Self::tick_chips_halfcycle).
/// 3. [`tick_chips`](Self::tick_chips) — once, after the second edge.
///
/// The master half-cycle counter [`hc`](Self::hc) runs at
/// [`cpu_divisor`](Self::cpu_divisor) ticks per T-state. Every current Z80
/// machine has no clock finer than the CPU's, so the default divisor of 2
/// makes every master tick a CPU edge; a machine with a faster master crystal
/// raises it, as the Spectrum's `FrameTiming` does.
pub trait Z80Machine {
    /// Master half-cycles per CPU T-state. Must be at least 2: the two CPU
    /// edges are phases `0` and `divisor / 2`.
    fn cpu_divisor(&self) -> u32 {
        2
    }

    /// The master half-cycle counter. Only its phase within a T-state is
    /// read, so a machine may keep it out of its snapshot (`serde(skip)`):
    /// a restore lands on a T-state boundary either way.
    fn hc(&self) -> u64;

    /// Mutable access to the counter for the provided methods.
    fn hc_mut(&mut self) -> &mut u64;

    /// Runs once per T-state, before the first CPU edge. For chips a machine
    /// ticks ahead of its CPU (the Jupiter Ace's display), and for T-state
    /// counters.
    fn before_tstate(&mut self) {}

    /// Copy every interrupt source onto the CPU's pins. Called immediately
    /// before each CPU tick, never after: the Z80 samples `/INT` at an
    /// instruction boundary during its own tick.
    fn feed_interrupt_pins(&mut self);

    /// Exactly one `Z80::tick`, then service the bus request it raised.
    /// Called twice per T-state. Wait-state logic that must wrap the tick
    /// (the MSX's M1 stretch) lives in here.
    fn tick_cpu_and_bus(&mut self);

    /// Chips that advance on every CPU edge, after the CPU (a VDP
    /// re-denominated to half-cycles). Default: nothing.
    fn tick_chips_halfcycle(&mut self) {}

    /// Chips that advance once per T-state, after the second CPU edge.
    /// Default: nothing.
    fn tick_chips(&mut self) {}

    /// Whether the CPU receives this edge. Default `true`; a machine whose
    /// video chip gates the CPU clock overrides it. Interrupt pins are fed
    /// regardless, because the Z80 latches them on its own schedule.
    fn cpu_clock_active(&self) -> bool {
        true
    }

    /// Advance one master half-cycle: the cadence above, keyed on the
    /// counter's phase. **Provided — do not override.**
    fn tick_one_halfcycle(&mut self) {
        let divisor = self.cpu_divisor();
        debug_assert!(
            divisor >= 2,
            "CPU divisor must provide two half-cycle phases"
        );
        let second_edge = divisor / 2;
        let phase = (self.hc() % u64::from(divisor)) as u32;
        if phase == 0 {
            self.before_tstate();
        }
        if phase == 0 || phase == second_edge {
            self.feed_interrupt_pins();
            if self.cpu_clock_active() {
                self.tick_cpu_and_bus();
            }
            self.tick_chips_halfcycle();
            if phase == second_edge {
                self.tick_chips();
            }
        }
        *self.hc_mut() += 1;
    }

    /// Advance an exact number of master half-cycles.
    fn advance_halfcycles(&mut self, halfcycles: u64) {
        for _ in 0..halfcycles {
            self.tick_one_halfcycle();
        }
    }

    /// Advance an exact number of CPU T-states. This is the one call a
    /// machine's `run_frame` and `Z80Stepper::step_tick` make; the factor of
    /// two lives here and nowhere else.
    fn advance_tstates(&mut self, tstates: u64) {
        self.advance_halfcycles(tstates * u64::from(self.cpu_divisor()));
    }
}

#[cfg(test)]
mod tests {
    use super::Z80Machine;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Event {
        BeforeTstate,
        Feed,
        Cpu,
        HalfcycleChips,
        Chips,
    }

    struct Recorder {
        hc: u64,
        divisor: u32,
        cpu_active: bool,
        log: Vec<Event>,
    }

    impl Recorder {
        fn new(divisor: u32) -> Self {
            Self {
                hc: 0,
                divisor,
                cpu_active: true,
                log: Vec::new(),
            }
        }
    }

    impl Z80Machine for Recorder {
        fn cpu_divisor(&self) -> u32 {
            self.divisor
        }
        fn hc(&self) -> u64 {
            self.hc
        }
        fn hc_mut(&mut self) -> &mut u64 {
            &mut self.hc
        }
        fn before_tstate(&mut self) {
            self.log.push(Event::BeforeTstate);
        }
        fn feed_interrupt_pins(&mut self) {
            self.log.push(Event::Feed);
        }
        fn tick_cpu_and_bus(&mut self) {
            self.log.push(Event::Cpu);
        }
        fn tick_chips_halfcycle(&mut self) {
            self.log.push(Event::HalfcycleChips);
        }
        fn tick_chips(&mut self) {
            self.log.push(Event::Chips);
        }
        fn cpu_clock_active(&self) -> bool {
            self.cpu_active
        }
    }

    const ONE_TSTATE: [Event; 8] = [
        Event::BeforeTstate,
        Event::Feed,
        Event::Cpu,
        Event::HalfcycleChips,
        Event::Feed,
        Event::Cpu,
        Event::HalfcycleChips,
        Event::Chips,
    ];

    #[test]
    fn one_tstate_is_two_cpu_edges_with_pins_fed_before_each() {
        let mut m = Recorder::new(2);
        m.advance_tstates(1);
        assert_eq!(m.log, ONE_TSTATE);
        assert_eq!(m.hc, 2);
    }

    #[test]
    fn the_factor_of_two_is_the_drivers_not_the_callers() {
        let mut m = Recorder::new(2);
        m.advance_tstates(100);
        assert_eq!(m.log.iter().filter(|e| **e == Event::Cpu).count(), 200);
        assert_eq!(m.log.iter().filter(|e| **e == Event::Chips).count(), 100);
        assert_eq!(m.hc, 200);
    }

    #[test]
    fn a_faster_master_clock_keeps_the_cpu_edges_at_phase_zero_and_half() {
        let mut m = Recorder::new(4);
        m.advance_tstates(1);
        // Four master ticks, CPU edges at phases 0 and 2, idle ticks between.
        assert_eq!(m.log, ONE_TSTATE);
        assert_eq!(m.hc, 4);
    }

    #[test]
    fn a_gated_cpu_still_has_its_pins_fed() {
        let mut m = Recorder::new(2);
        m.cpu_active = false;
        m.advance_tstates(1);
        assert_eq!(
            m.log,
            [
                Event::BeforeTstate,
                Event::Feed,
                Event::HalfcycleChips,
                Event::Feed,
                Event::HalfcycleChips,
                Event::Chips,
            ]
        );
    }

    #[test]
    fn halfcycle_steps_compose_into_whole_tstates() {
        let mut a = Recorder::new(2);
        a.advance_halfcycles(6);
        let mut b = Recorder::new(2);
        b.advance_tstates(3);
        assert_eq!(a.log, b.log);
    }

    #[test]
    #[should_panic(expected = "CPU divisor must provide two half-cycle phases")]
    fn a_divisor_below_two_cannot_produce_two_edges() {
        let mut m = Recorder::new(1);
        m.tick_one_halfcycle();
    }
}
