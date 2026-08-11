//! Ferranti 6C001E ULA wrapper.
//!
//! Source references:
//! - `knowledge/chips/ferranti-6c001e.md`
//! - `knowledge/systems/spectrum/contention.md`
//! - Adapted from `../Emu198x-Older/crates/ferranti-ula-6c001e/src/lib.rs`

#[doc(hidden)]
pub mod hdl_model;

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{self, FrameTiming};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{self, DELAY_TABLE_48K, UlaEngine};

/// Ferranti 6C001E ULA — the 48K ZX Spectrum's custom chip.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct FerrantiUla {
    engine: UlaEngine,
    revision: UlaRevision,

    /// Half-cycle recorder, off unless explicitly armed.
    ///
    /// Records what `tick` was *given* and what it *decided*, from inside
    /// the ULA. That is the only place the question can be answered
    /// without assuming anything about the driver: two separate attempts
    /// to reproduce the driver's tick order in a test harness both got it
    /// wrong by a half-cycle, in opposite directions, and a half-cycle is
    /// exactly the width of the window `IOREQTW3` opens and closes in.
    #[serde(skip)]
    trace: Option<Vec<UlaTick>>,
}

/// One half-cycle as the ULA saw it. `_before` fields are sampled prior to
/// the contention decision, which is the state that decision was made on.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UlaTick {
    pub pixel: u16,
    pub video: bool,
    pub addr: u16,
    pub mreq: bool,
    pub iorq: bool,
    pub clock_high_before: bool,
    pub ioreq_tw3_before: bool,
    pub mreq_t23_before: bool,
    pub cpu_clock_after: bool,
}

/// Which family of the 48K-class Ferranti ULA this machine carries.
///
/// Per Smith Chapter 24, six ULA revisions exist (5C102E / 5C112E /
/// 6C001E-6 / 6C001E-7 / 6C011E / 7K010E-5), but only one boundary
/// is *software-visible*: the EAR-feedback bit on port $FE differs
/// between the 5C family (Issue 1 + Issue 2 boards) and the 6C
/// family (Issue 3 + Issue 4 boards). 5C/6C subvariants within each
/// family differ only in DRAM-margin handling — invisible to
/// software. The 128K's 7K010E lives on its own ULA crate
/// (`sinclair-ula-7k010e`); this enum only covers the 48K-class.
///
/// **Variant order is load-bearing for serde.** `Ferranti5C` maps
/// to where `Issue2` was; `Ferranti6C` to where `Issue3` was. Both
/// are still tag index 0 / 1 in postcard, so on-the-wire snapshot
/// bytes remain byte-identical across this rename.
///
/// Mapped to historical board issues:
/// - `Ferranti5C` — used in Issue 1 + Issue 2 boards. EAR
///   feedback (bit 6 of `$FE` read) reflects `(MIC | EAR)` from the
///   last write. Games detect the family by writing `$08` to `$FE`
///   and reading bit 6 back as `1`.
/// - `Ferranti6C` — used in Issue 3, Issue 4, and most later 48K
///   boards. EAR feedback reflects only the `EAR` bit; `MIC`
///   alone does not drive bit 6 high.
///
/// See `knowledge/decisions/spectrum-architecture-review.md` for
/// the rationale on tying revision identity to the ULA chip rather
/// than the PCB issue number.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum UlaRevision {
    /// 5C102E (Issue 1) or 5C112E (Issue 2). EAR feedback = MIC | EAR.
    Ferranti5C,
    /// 6C001E (Issue 3+). EAR feedback = EAR only.
    Ferranti6C,
}

impl FerrantiUla {
    pub fn new(revision: UlaRevision) -> Self {
        Self {
            engine: UlaEngine::new(&ula_engine::CONFIG_48K),
            revision,
            trace: None,
        }
    }

    #[must_use]
    pub const fn revision(&self) -> UlaRevision {
        self.revision
    }

    #[must_use]
    pub fn border_color(&self) -> u8 {
        self.engine.border
    }

    /// Debug raster probe: `(scan, pixel, video, idle, bus_data)`. Used by
    /// floating-bus / interrupt-phase investigations to map a frame
    /// T-state onto the ULA's internal beam position.
    #[doc(hidden)]
    #[must_use]
    pub fn debug_raster(&self) -> (u16, u16, bool, bool, u8) {
        (
            self.engine.scan,
            self.engine.pixel,
            self.engine.video,
            self.engine.idle,
            self.engine.bus_data,
        )
    }

    /// Arm the half-cycle recorder, discarding anything already held.
    #[doc(hidden)]
    pub fn debug_trace_start(&mut self) {
        self.trace = Some(Vec::new());
    }

    /// Take the recording and disarm.
    #[doc(hidden)]
    pub fn debug_trace_take(&mut self) -> Vec<UlaTick> {
        self.trace.take().unwrap_or_default()
    }

    /// Reinstall the 48K timing config after a snapshot restore.
    ///
    /// `UlaEngine::config` is `#[serde(skip)]` and falls back to the
    /// 48K config on deserialise. The Ferranti happens to want the 48K
    /// config too, so this method is currently a structural mirror of
    /// the 128K and Amstrad cases — it documents that every variant
    /// must reattach explicitly rather than relying on the fallback,
    /// so the pattern doesn't silently break if the default ever
    /// changes.
    pub fn reattach_config(&mut self) {
        self.engine.set_config(&ula_engine::CONFIG_48K);
    }

    /// Compute the EAR feedback bit (bit 6) for a port-$FE read.
    ///
    /// With no tape signal driving the EAR line, real hardware reflects
    /// the last write to port $FE back onto bit 6 — but the 5C-family
    /// (Issue 1/2) and 6C-family (Issue 3+) boards do this differently:
    ///
    /// - **Ferranti 5C** (Issue 1/2): bit 6 reads as `(MIC | EAR)` from
    ///   the last write. Either bit 3 (MIC) or bit 4 (EAR) being high
    ///   drives bit 6 high.
    /// - **Ferranti 6C** (Issue 3+): bit 6 reads as just `EAR` from the
    ///   last write. Bit 3 (MIC) alone does not set bit 6 high.
    ///
    /// Games that probe the ULA family use exactly this distinction:
    /// write `$08` to `$FE`, read back, and check bit 6.
    fn ear_feedback_bit(&self) -> u8 {
        let beeper_bit = self.engine.beeper;
        let mic_bit = self.engine.mic;
        let high = match self.revision {
            UlaRevision::Ferranti5C => beeper_bit || mic_bit,
            UlaRevision::Ferranti6C => beeper_bit,
        };
        if high { 0x40 } else { 0x00 }
    }
}

impl Ula for FerrantiUla {
    fn tick(
        &mut self,
        memory: &dyn MemoryBus,
        cpu_addr: u16,
        cpu_mreq: bool,
        cpu_iorq: bool,
        cpu_rfsh: bool,
        framebuffer: &mut [u8],
    ) {
        let e = &mut self.engine;
        let phase = (e.pixel as usize) & 0x0F;

        // Snow: a CPU refresh with I in screen-RAM range collides with
        // the video fetch (the Ferranti ULA ignores /RFSH). gap #12.
        let snow = ula_engine::snow_address(cpu_rfsh, cpu_addr);

        // Rendering: video fetch, pixel output, counters, interrupt
        e.tick_rendering(memory, framebuffer, snow);

        // State as it stands before the contention decision, kept for the
        // recorder. Cheap enough to take unconditionally; the push is what
        // is gated.
        let before = (
            // The pixel the delay table is *indexed by*, not the counter's
            // value after `tick_rendering` advanced it. Recording the
            // post-advance value shifts the trace one pixel, which happens
            // to cancel the known one-pixel rotation between
            // `DELAY_TABLE_48K` and the HDL's `hc[2]|hc[3]` — and so
            // reports perfect agreement that is not there.
            phase as u16,
            e.video,
            e.z80_clock_high,
            e.ioreq_tw3,
            e.mreq_t23,
        );

        // The ULA answers even ports (`spec48_port_from_ula`); the HDL
        // folds that decode into the pin, `ioreq_n = a[0] | iorq_n`.
        // Outside the window because the latches clock on every `CPUClk`
        // edge, border or not.
        let ula_io = (cpu_addr & 1) == 0;

        // Contention (48K model): memory + I/O + internal
        if e.video {
            let contended_addr = memory.is_contended(cpu_addr);
            // `/MREQT23` gates the wait so it cannot re-arm inside an
            // M-cycle — Smith Chapter 18, p. 197: the circuit detects `T1`
            // of a contended cycle by waiting for A14 high with A15 and
            // MREQT23 low.
            // `/MREQT23` — see `UlaEngine::mreq_t23`. Keying off
            // `!cpu_mreq` alone lets the gate re-arm in `T3`, while the
            // contended address is still on the bus, and charges a
            // second full rotation to every M-cycle past `M1`.
            // `/MREQT23` is computed and correct (see `UlaEngine::mreq_t23`)
            // but **deliberately not wired in here yet**. Enabling it —
            // swapping `!cpu_mreq` for `!e.mreq_t23` — fixes a real defect:
            // the gate otherwise re-arms after an access has committed and
            // over-charges every M-cycle past `M1` by a full 8-T-state
            // rotation. That change took the ZXSpectrum4.net timing survey
            // from 34/70 to 37/70.
            //
            // It also breaks the floating bus, and the two cannot currently
            // be satisfied together. The old over-contention was being
            // compensated by the floating-bus sample lead, and no value of
            // that lead recovers both oracles once the contention is fixed:
            // floatspy's self-test passes only at lead 0, where Float48K
            // reads 14340 against a hardware-measured 14338, and the lead
            // that makes Float48K exact leaves floatspy red. A third error
            // in the floating-bus path is being masked, and it is not the
            // sample lead or the pattern phase — both were tried.
            //
            // See `knowledge/decisions/spectrum-contention-vs-floating-bus.md`.
            // **Both latches are maintained but not consulted.** Wiring
            // them in makes the gate match the HDL exactly — verified
            // half-cycle for half-cycle on the real machine by
            // `machine-sinclair-zx-spectrum-48k`'s `ula_gate_vs_hdl` — and
            // costs one T-state per contended M-cycle against FUSE. The
            // HDL and FUSE genuinely disagree here; the engine can match
            // one or the other, not both.
            //
            //     let contended_access = contended_addr && !e.mreq_t23;
            //     let contention = !e.ioreq_tw3
            //         && e.z80_clock_high
            //         && (ula_io || contended_access);
            //
            // See `knowledge/decisions/spectrum-contention-vs-floating-bus.md`.
            let mem_contention = contended_addr && e.gate_arms_this_halfcycle() && !cpu_mreq;
            let io_contention = (cpu_iorq || e.z80_iorq_prev) && ula_io && e.z80_clock_high;
            let contention = mem_contention || io_contention;
            e.cpu_clock = !(contention && DELAY_TABLE_48K[phase]);
        } else {
            e.cpu_clock = true;
        }

        // Record before `track_z80_clock` advances the latches, so the
        // captured state is the state the decision above was made on.
        if let Some(trace) = &mut self.trace {
            trace.push(UlaTick {
                pixel: before.0,
                video: before.1,
                addr: cpu_addr,
                mreq: cpu_mreq,
                iorq: cpu_iorq,
                clock_high_before: before.2,
                ioreq_tw3_before: before.3,
                mreq_t23_before: before.4,
                cpu_clock_after: self.engine.cpu_clock,
            });
        }

        let e = &mut self.engine;
        // Track Z80 clock phase
        e.track_z80_clock(cpu_iorq, cpu_mreq, cpu_iorq && ula_io);
    }

    fn cpu_clock_active(&self) -> bool {
        self.engine.cpu_clock
    }

    fn interrupt_active(&self) -> bool {
        self.engine.int_active
    }

    fn floating_bus(&self) -> u8 {
        if self.engine.idle {
            0xFF
        } else {
            self.engine.bus_data
        }
    }

    fn read_fe(&self, port: u16, keyboard: &[u8; 8]) -> u8 {
        // Start with the shared engine's keyboard + high-bit result.
        // Bit 6 (EAR) needs revision-specific handling — the shared
        // engine always returns bit 6 high, which is only correct when
        // the tape input is idle on a Ferranti6C board with no recent
        // writes.
        let mut val = self.engine.read_fe(port, keyboard);
        val &= !0x40; // clear bit 6 so we can set it based on the revision
        val |= self.ear_feedback_bit();
        val
    }

    fn write_fe(&mut self, val: u8) {
        self.engine.write_fe(val);
    }

    fn frame_timing(&self) -> &FrameTiming {
        &timing::TIMING_48K
    }

    fn end_frame(&mut self) {
        self.engine.end_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The half-cycle recorder must capture the state each contention
    /// decision was made *on*, not the state left behind after it.
    ///
    /// This is not hypothetical. The recorder first captured `pixel`
    /// after `tick_rendering` had advanced it, while the phase the gate
    /// consults is computed before — a one-pixel skew that exactly
    /// cancelled a known table rotation and made the engine and the HDL
    /// model appear to agree perfectly when they did not. The check that
    /// caught it lives in `machine-sinclair-zx-spectrum-48k`'s
    /// differential, which is `#[ignore]`d because it needs a real ROM.
    /// Nothing about the recorder needs one, so the guard belongs here
    /// where it runs.
    #[test]
    fn the_recorder_captures_the_state_each_decision_was_made_on() {
        use common_sinclair_zx_spectrum::timing;

        let mut ula = FerrantiUla::new(UlaRevision::Ferranti6C);
        let mut fb = vec![0; timing::SCREEN_WIDTH * timing::SCREEN_HEIGHT];

        ula.engine.scan = 100;
        ula.engine.pixel = 0;
        let first_pixel = ula.engine.pixel;

        ula.debug_trace_start();
        for _ in 0..8 {
            Ula::tick(
                &mut ula,
                &ContendedMemory,
                0x4000,
                true,
                false,
                false,
                &mut fb,
            );
        }
        let trace = ula.debug_trace_take();

        assert_eq!(trace.len(), 8, "one entry per tick, no more and no less");
        for (i, tick) in trace.iter().enumerate() {
            assert_eq!(
                tick.pixel,
                first_pixel + i as u16,
                "entry {i} must hold the pixel the decision was made on; \
                 an off-by-one here is the skew that hid a real disagreement",
            );
            assert_eq!(tick.addr, 0x4000, "entry {i} must record the CPU address");
            assert!(tick.mreq, "entry {i} must record /MREQ as asserted");
            assert!(!tick.iorq, "entry {i} must record /IORQ as idle");
        }

        assert!(
            ula.debug_trace_take().is_empty(),
            "taking the recording must also disarm it",
        );
    }

    /// Measure the gate's *effective* delay table and print it beside the
    /// canonical one.
    ///
    /// Rather than argue from the table's contents, this ticks the ULA to
    /// each arrival half-cycle in turn and counts how long the clock is
    /// actually withheld.
    ///
    /// The measured ramp is `6, 5, 4, 3, 2, 1, 0, 0` T-states with its
    /// phase origin at **half-cycle 0** — the canonical pattern, on the
    /// canonical T-state grid, with no rotation constant between them.
    /// That is what `DELAY_TABLE_48K` being derived from `C3 + C2` on the
    /// fetch group's origin buys: the CPU's T-states and the ULA's pixel
    /// counter start together, and each delay slot is two whole pixels of
    /// one T-state rather than a pair straddling a boundary.
    ///
    /// The literal this replaced was free at half-cycles 15, 0, 1 and 2
    /// and reproduced the same ramp at origin 3. It read as canonical
    /// too, and the difference did not show up here — a single access
    /// pays the same ramp either way. It showed up as a whole-T-state
    /// shift of the window against the frame, which is what
    /// `contention_oracle`'s arrival-resolved differential scores and
    /// this probe cannot see.
    ///
    /// These numbers survived the change of arming polarity byte for
    /// byte, once this probe stopped naming a clock *level* and started
    /// asking the gate which half-cycle arms it. That is the useful
    /// result: the polarity moved which edge is withheld, and moved the
    /// delay ramp not at all.
    ///
    /// The non-arming column is the interesting one: it costs one C0
    /// *less* than the arming one — half a T-state that can only be spent
    /// as a whole one, because the gate withholds only on the arming
    /// half. That is the shape of the residual the per-instruction oracle
    /// still reports on multi-M-cycle instructions and not on
    /// single-M-cycle ones, and it is something the canonical
    /// whole-T-state model cannot represent.
    ///
    /// The two parities now agree on every slot of the ramp, where the
    /// literal made them differ inside the free window. `h = 15` is the
    /// one boundary case left: a non-arming arrival on the last free
    /// half-cycle reaches the arming half only after the window has shut,
    /// and pays the whole next rotation.
    #[test]
    fn effective_delay_table() {
        use common_sinclair_zx_spectrum::timing;
        const CANONICAL: [u32; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

        /// The measured table, in C0 half-cycles, as `(arming, other)` per
        /// arrival half-cycle.
        const MEASURED: [(u32, u32); 16] = [
            (12, 12),
            (11, 11),
            (10, 10),
            (9, 9),
            (8, 8),
            (7, 7),
            (6, 6),
            (5, 5),
            (4, 4),
            (3, 3),
            (2, 2),
            (1, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 13),
        ];

        let mut measured = [(0u32, 0u32); 16];

        println!("\n half-cycle  T-phase  stall@arming  stall@other  delta  canonical[T-phase]");
        for h in 0..16u16 {
            let mut stalls = [0u32; 2];
            // Seed the *arming* parity, not the raw clock level. Which
            // level arms the gate is a property of the gate and has moved
            // once already; naming the level here is what made this probe
            // need repairing when it did.
            for (slot, arrives_arming) in [true, false].into_iter().enumerate() {
                let mut ula = FerrantiUla::new(UlaRevision::Ferranti6C);
                let mut fb = vec![0; timing::SCREEN_WIDTH * timing::SCREEN_HEIGHT];

                // `video` is latched as the line's fetch window opens, so
                // the ULA has to be ticked in from the start of a line rather
                // than parked mid-window — jumping straight to a pixel leaves
                // the latch clear and no contention fires at all.
                ula.engine.scan = 100;
                ula.engine.pixel = 0;
                for _ in 0..(128 + h) {
                    Ula::tick(
                        &mut ula,
                        &ContendedMemory,
                        0x4000,
                        false,
                        false,
                        false,
                        &mut fb,
                    );
                }

                // Clear the MREQT23 latch and the clock phase so the arrival
                // is defined by the pixel counter alone.
                ula.engine.mreq_t23 = false;
                ula.engine.z80_clock_high = !arrives_arming;

                // Contended address, MREQ inactive. Count C0 cycles until
                // the Z80 can advance past `T1`.
                //
                // The stop condition matters, and it has to follow the
                // gate rather than restate it. The gate withholds only on
                // the arming half-cycle, so a cycle arriving on the other
                // one is never withheld — stopping at "first cycle the
                // clock is active" reports zero for every such arrival,
                // which is an artefact of the question. What the CPU is
                // waiting for is an *arming* half-cycle that is not
                // withheld, so that is what is counted. Asking
                // `gate_arms_this_halfcycle` keeps this probe honest if the
                // polarity is ever revisited; spelling the parity out here
                // is what made it need repairing when it was.
                let mut stall = 0u32;
                for _ in 0..64 {
                    let was_arming = ula.engine.gate_arms_this_halfcycle();
                    Ula::tick(
                        &mut ula,
                        &ContendedMemory,
                        0x4000,
                        false,
                        false,
                        false,
                        &mut fb,
                    );
                    if was_arming && ula.cpu_clock_active() {
                        break;
                    }
                    stall += 1;
                }
                stalls[slot] = stall;
            }

            let tphase = h / 2;
            let delta = stalls[0] as i64 - stalls[1] as i64;
            println!(
                "{h:>10} {tphase:>8} {:>12} {:>12} {delta:>+6} {:>19}",
                stalls[0], stalls[1], CANONICAL[tphase as usize]
            );
            measured[h as usize] = (stalls[0], stalls[1]);
        }

        assert_eq!(
            measured, MEASURED,
            "the gate's effective delay table changed; the printed table \
             above shows how, and any change here moves contention for \
             every contended access",
        );

        // The property the table above is evidence *for*, stated directly
        // so a coincidental match cannot pass for the real thing: an
        // arrival on the arming half of T-phase `p` stalls `CANONICAL[p]`
        // T-states, with the phase origin at half-cycle 0.
        //
        // There is no rotation constant in that statement, and there was
        // one before. A `slot` expression is where a phase error hides:
        // it can be chosen to make any alignment read as canonical, which
        // is exactly what it did.
        for h in (0..16).step_by(2) {
            let t_states = measured[h].0 / 2;
            let slot = h / 2;
            assert_eq!(
                t_states, CANONICAL[slot],
                "half-cycle {h} opens T-phase {slot} and should stall {} \
                 T-states, not {t_states}",
                CANONICAL[slot],
            );
        }

        // And within a T-phase the second half-cycle costs one C0 less —
        // half a T-state that can only be spent as a whole one, because
        // the gate withholds the clock only on the arming half. This is
        // the shape of the residual the per-instruction oracle reports on
        // multi-M-cycle instructions.
        //
        // Bounded to the ramp. Inside the free window the relationship
        // inverts: there is nothing left to withhold, so the second
        // half-cycle costs one C0 *more* while it waits for an arming
        // half to arrive.
        for h in (0..12).step_by(2) {
            assert_eq!(
                measured[h].0,
                measured[h + 1].0 + 1,
                "half-cycle {h} should cost one C0 more than {}",
                h + 1,
            );
        }
    }

    use common_sinclair_zx_spectrum::timing;

    struct ContendedMemory;

    impl MemoryBus for ContendedMemory {
        fn read(&self, _addr: u16) -> u8 {
            0
        }

        fn write(&mut self, _addr: u16, _value: u8) {}

        fn is_contended(&self, _addr: u16) -> bool {
            true
        }
    }

    /// Committing an access must suppress contention that would
    /// otherwise fire — the `/MREQT23` term (Smith Chapter 18, pp.
    /// 192-193 and 197).
    ///
    /// Keying the gate off `MREQ` being inactive *right now* also matches
    /// the trailing T-state of a memory cycle, where the contended
    /// address is still on the bus, so every M-cycle past `M1` was
    /// charged a second full 8-T-state rotation. `M1` hid the fault: the
    /// cycle following its access is the refresh, whose address is
    /// uncontended, so single-M-cycle instructions measured exact while
    /// everything else drifted.
    ///
    /// Written as a differential rather than a fixed expectation. Two
    /// runs are driven from the same point with the same pin sequence
    /// except that one asserts `MREQ` to commit an access; the pixel
    /// counter advances whether or not the CPU clock is held, so the runs
    /// stay phase-aligned and are directly comparable. The test demands
    /// a tick where the committed run runs and the uncommitted run
    /// stalls. A gate without `MREQT23` produces identical traces —
    /// once `MREQ` is low again it has no memory of the access — so this
    /// cannot pass vacuously, and it does fail against the old gate.
    #[test]
    #[ignore = "passes only with the MREQT23 latch wired into the gate; \
                blocked on the floating-bus derivation — see \
                knowledge/decisions/spectrum-contention-vs-floating-bus.md"]
    fn a_committed_access_suppresses_contention_that_would_otherwise_fire() {
        fn clock_trace(commit: bool) -> Vec<bool> {
            let mut ula = FerrantiUla::new(UlaRevision::Ferranti6C);
            let mut fb = vec![0; timing::SCREEN_WIDTH * timing::SCREEN_HEIGHT];
            let tick = |ula: &mut _, mreq: bool, fb: &mut [u8]| {
                Ula::tick(ula, &ContendedMemory, 0x4000, mreq, false, false, fb);
            };

            // Into the contended window, then on to the free window that
            // releases the CPU — the delay table ends a stall, not MREQ.
            for _ in 0..256 {
                tick(&mut ula, false, &mut fb);
                if !ula.cpu_clock_active() {
                    break;
                }
            }
            for _ in 0..256 {
                tick(&mut ula, false, &mut fb);
                if ula.cpu_clock_active() {
                    break;
                }
            }

            // Optionally commit an access, then record what the gate does
            // once MREQ is low again in both runs.
            for _ in 0..3 {
                tick(&mut ula, commit, &mut fb);
            }
            (0..6)
                .map(|_| {
                    tick(&mut ula, false, &mut fb);
                    ula.cpu_clock_active()
                })
                .collect()
        }

        let committed = clock_trace(true);
        let uncommitted = clock_trace(false);

        assert!(
            uncommitted.iter().any(|running| !running),
            "the uncommitted run should stall, or the comparison proves nothing"
        );
        assert!(
            committed
                .iter()
                .zip(&uncommitted)
                .any(|(with, without)| *with && !*without),
            "committing an access did not suppress any contention: the \
             /MREQT23 term is missing, and the gate re-arms in the trailing \
             T-state, charging a second rotation to every M-cycle past M1\n\
             committed:   {committed:?}\n\
             uncommitted: {uncommitted:?}"
        );
    }

    fn empty_keyboard() -> [u8; 8] {
        [0xFF; 8]
    }

    #[test]
    fn ferranti6c_ear_reflects_only_bit4() {
        let mut ula = FerrantiUla::new(UlaRevision::Ferranti6C);

        // Write $00: everything clear. Bit 6 should be low.
        ula.write_fe(0x00);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x00);

        // Write $08 (MIC only): on Ferranti6C, bit 6 stays low.
        ula.write_fe(0x08);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x00);

        // Write $10 (EAR only): bit 6 goes high.
        ula.write_fe(0x10);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x40);

        // Write $18 (MIC + EAR): bit 6 stays high.
        ula.write_fe(0x18);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x40);
    }

    #[test]
    fn ferranti5c_ear_reflects_mic_or_ear() {
        let mut ula = FerrantiUla::new(UlaRevision::Ferranti5C);

        // Write $00: everything clear. Bit 6 should be low.
        ula.write_fe(0x00);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x00);

        // Write $08 (MIC only): on Ferranti5C, bit 6 goes high.
        // This is the key distinction from Ferranti6C.
        ula.write_fe(0x08);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x40);

        // Write $10 (EAR only): bit 6 stays high.
        ula.write_fe(0x10);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x40);

        // Write $18 (MIC + EAR): bit 6 stays high.
        ula.write_fe(0x18);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x40);
    }
}
