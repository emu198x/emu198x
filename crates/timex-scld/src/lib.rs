use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{self, FrameTiming};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{self, DELAY_TABLE_48K, UlaEngine};

/// Timex SCLD — Semi-Custom Logic Device.
///
/// Used in the TC2048, TC2068, and TS2068. Same contention model as the
/// Ferranti 6C001E (48K pattern) but adds 8 video modes and full I/O decoding.
///
/// Port $FF (SCLD control register):
///   Bits 0-2: Video mode (0-7)
///   Bit 3:    Hi-res ink colour bit 0
///   Bit 4:    Hi-res ink colour bit 1
///   Bit 5:    Hi-res ink colour bit 2
///   Bit 6:    Interrupt disable (1 = disable)
///
/// Video modes:
///   0: Standard Spectrum display (256×192, 8×8 attributes)
///   1: Dual-screen (alternates screen 0 and screen 1)
///   2: Hi-colour (8×1 attribute cells instead of 8×8)
///   3: Hi-colour + dual-screen
///   4: Hi-res monochrome (512×192)
///   5: Hi-res + dual-screen
///   6: Hi-res + hi-colour
///   7: Hi-res + hi-colour + dual-screen
///
/// Currently only Mode 0 is rendered. The mode register is stored for
/// future implementation of the extended video modes.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TimexScld {
    engine: UlaEngine,
    /// Port $FF value (video mode + hi-res colour + interrupt control).
    scld_reg: u8,
}

impl TimexScld {
    pub fn new() -> Self {
        Self {
            engine: UlaEngine::new_hires(&ula_engine::CONFIG_48K),
            scld_reg: 0,
        }
    }

    /// Create with a specific ULA timing config (for TS2068 NTSC).
    pub fn with_config(config: &'static ula_engine::UlaConfig) -> Self {
        Self {
            engine: UlaEngine::new_hires(config),
            scld_reg: 0,
        }
    }

    /// Current video mode (bits 0-2 of port $FF).
    pub fn video_mode(&self) -> u8 {
        self.scld_reg & 0x07
    }

    /// Port $FF write (SCLD control register).
    pub fn write_ff(&mut self, val: u8) {
        self.scld_reg = val;
        self.engine.scld_mode = val & 0x07;
        self.engine.scld_hires_ink = (val >> 3) & 0x07;
    }

    /// Port $FF read.
    pub fn read_ff(&self) -> u8 {
        self.scld_reg
    }

    /// Reinstall the timing config after a snapshot restore.
    ///
    /// `UlaEngine::config` is `#[serde(skip)]` and deserialises to the
    /// 48K fallback. The SCLD serves both the PAL TC2048/TC2068
    /// (`CONFIG_48K`) and the NTSC TS2068 (`CONFIG_TS2068`), which have
    /// different frame geometry, so the caller — which knows the model —
    /// supplies the config. The hi-res framebuffer width is a serialised
    /// field and survives restore; only the config ref needs reattaching.
    pub fn reattach_config(&mut self, config: &'static ula_engine::UlaConfig) {
        self.engine.set_config(config);
    }
}

impl Default for TimexScld {
    fn default() -> Self {
        Self::new()
    }
}

impl Ula for TimexScld {
    fn tick(
        &mut self,
        memory: &dyn MemoryBus,
        cpu_addr: u16,
        cpu_mreq: bool,
        cpu_iorq: bool,
        // The Timex SCLD shares the Ferranti DRAM design and likely
        // snows in standard mode, but its interaction with the SCLD
        // hi-res/hi-colour fetch paths is unverified, so snow is not
        // modelled here yet.
        _cpu_rfsh: bool,
        framebuffer: &mut [u8],
    ) {
        let e = &mut self.engine;
        let phase = (e.pixel as usize) & 0x0F;

        // The contention window, read before `tick_rendering` advances
        // the counter — sixteen whole fetch cycles from the boundary, not
        // the fetch window `e.video` opens four pixels in. See
        // `ula_engine::CONTENDED_PIXELS_PER_LINE`; the SCLD shares the
        // Ferranti's fetch phase, so it shares the relationship.
        let contend_window =
            e.scan < ula_engine::CONTENDED_LINES && e.pixel < ula_engine::CONTENDED_PIXELS_PER_LINE;

        e.tick_rendering(memory, framebuffer, None);

        // Same contention as 48K Ferranti (memory + I/O)
        if contend_window {
            let contended_addr = memory.is_contended(cpu_addr);
            // `/MREQT23` — see `UlaEngine::mreq_t23`. Keying off
            // `!cpu_mreq` alone lets the gate re-arm in `T3`, while the
            // contended address is still on the bus, and charges a
            // second full rotation to every M-cycle past `M1`.
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
            let mem_contention = contended_addr && e.gate_arms_this_halfcycle() && !cpu_mreq;

            let io_even_port = (cpu_addr & 1) == 0;
            // As in `sinclair-ula-7k010e`: the 48K's gate now counts FUSE's
            // contention lookups instead of holding a level, and this one
            // deliberately does not, because the Timex has boot and golden
            // tests and no contention oracle at all. See
            // `knowledge/decisions/io-contention-is-a-count-not-a-level.md`,
            // "48K only, deliberately".
            let io_contention = (cpu_iorq || e.z80_iorq_prev) && io_even_port && e.z80_clock_high;

            let contention = mem_contention || io_contention;
            e.cpu_clock = !(contention && DELAY_TABLE_48K[phase]);
        } else {
            e.cpu_clock = true;
        }

        e.track_z80_clock(cpu_iorq, cpu_mreq, cpu_iorq && (cpu_addr & 1) == 0);
    }

    fn cpu_clock_active(&self) -> bool {
        self.engine.cpu_clock
    }

    fn interrupt_active(&self) -> bool {
        // Bit 6 of SCLD register can disable interrupts
        if self.scld_reg & 0x40 != 0 {
            false
        } else {
            self.engine.int_active
        }
    }

    fn floating_bus(&self) -> u8 {
        if self.engine.idle {
            0xFF
        } else {
            self.engine.bus_data
        }
    }

    fn read_fe(&self, port: u16, keyboard: &[u8; 8]) -> u8 {
        self.engine.read_fe(port, keyboard)
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
            let mut ula = TimexScld::new();
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
}
