//! Ferranti 6C001E ULA wrapper.
//!
//! Source references:
//! - `knowledge/chips/ferranti-6c001e.md`
//! - `knowledge/systems/spectrum/contention.md`
//! - Adapted from `../Emu198x-Older/crates/ferranti-ula-6c001e/src/lib.rs`

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{self, FrameTiming};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{self, DELAY_TABLE_48K, UlaEngine};

/// Ferranti 6C001E ULA — the 48K ZX Spectrum's custom chip.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct FerrantiUla {
    engine: UlaEngine,
    revision: UlaRevision,
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
            let mem_contention = contended_addr && e.z80_clock_high && !cpu_mreq;

            let io_even_port = (cpu_addr & 1) == 0;
            let io_contention = (cpu_iorq || e.z80_iorq_prev) && io_even_port && e.z80_clock_high;

            let contention = mem_contention || io_contention;
            e.cpu_clock = !(contention && DELAY_TABLE_48K[phase]);
        } else {
            e.cpu_clock = true;
        }

        // Track Z80 clock phase
        e.track_z80_clock(cpu_iorq, cpu_mreq);
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

    /// Measure the gate's *effective* delay table and print it beside the
    /// canonical one.
    ///
    /// The residual on multi-M-cycle instructions in the per-instruction
    /// oracle is unexplained, and one candidate is that the engine's
    /// table sits at a different phase origin from the published
    /// `[6,5,4,3,2,1,0,0]`: `DELAY_TABLE_48K` is free at half-cycles
    /// 15, 0, 1 and 2, whereas the canonical pattern's zero-delay slots
    /// are T-phases 6 and 7. Rather than argue from the table's contents,
    /// this ticks the ULA to each arrival half-cycle in turn and counts
    /// how long the clock is actually withheld.
    ///
    /// The answer is that the table is **not** misaligned. At odd
    /// half-cycles the measured stalls are 6, 5, 4, 3, 2, 1, 0, 0
    /// T-states — canonical exactly — with the phase origin at
    /// half-cycle 3, which independently confirms the one-T-state
    /// sampling offset the oracle calibrates.
    ///
    /// The even half-cycles are the interesting column: they cost an
    /// extra *half* T-state (5.5, 4.5, 3.5, …), because contention only
    /// fires while `z80_clock_high`. Arriving on the opposite parity
    /// costs half a cycle that can only be spent as a whole one. That is
    /// the shape of the residual the per-instruction oracle reports on
    /// multi-M-cycle instructions and not on single-M-cycle ones, and it
    /// is something the canonical whole-T-state model cannot represent.
    ///
    /// Measured at both clock parities, the rule is that **the wait ends
    /// only on a clock-high C0 that is not asserted**. The two parities
    /// agree everywhere except inside the free window: a low-half arrival
    /// there must first reach the high half, and at pixel 2 that lands
    /// past the window, costing the full 13 C0 rather than 0.
    ///
    /// Worth recording what this does *not* explain. Applying that rule
    /// to the arrivals the single-M-cycle anchors actually see leaves
    /// their wait unchanged, so the parity term — now pinned by
    /// measurement rather than fitted — is not the cause of the one
    /// T-state those cases are out by in the oracle.
    #[test]
    #[ignore = "diagnostic probe"]
    fn effective_delay_table() {
        use common_sinclair_zx_spectrum::timing;
        const CANONICAL: [u32; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

        println!("\n half-cycle  T-phase  stall@clkhi  stall@clklo  delta  canonical[T-phase]");
        for h in 0..16u16 {
            let mut stalls = [0u32; 2];
            for (slot, clk_hi) in [true, false].into_iter().enumerate() {
                let mut ula = FerrantiUla::new(UlaRevision::Ferranti6C);
                let mut fb = vec![0; timing::SCREEN_WIDTH * timing::SCREEN_HEIGHT];
                let _ = clk_hi;
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
                ula.engine.z80_clock_high = clk_hi;

                // Contended address, MREQ inactive. Count C0 cycles until
                // the Z80 can actually advance past `T1`.
                //
                // The stop condition matters. The gate can only hold the
                // clock *high*, so a cycle arriving on the low half is never
                // withheld — stopping at "first cycle the clock is active"
                // therefore reports zero for every low-half arrival, which is
                // an artefact of the question, not a property of the gate.
                // What the CPU is waiting for is a high-half cycle that is
                // not withheld, so that is what is counted.
                let mut stall = 0u32;
                for _ in 0..64 {
                    let was_high = ula.engine.z80_clock_high;
                    Ula::tick(
                        &mut ula,
                        &ContendedMemory,
                        0x4000,
                        false,
                        false,
                        false,
                        &mut fb,
                    );
                    if was_high && ula.cpu_clock_active() {
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
