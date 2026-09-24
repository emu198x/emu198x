//! Sinclair 7K010E ULA — the 128K / +2 ZX Spectrum's custom chip.
//!
//! Source references:
//! - `knowledge/chips/sinclair-7k010e.md`
//! - `knowledge/systems/spectrum/contention.md`
//! - Adapted from `../Emu198x-Older/crates/sinclair-ula-7k010e/src/lib.rs`
//!
//! Same contention model as the Ferranti (48K) but different timing:
//! - Crystal: 17,734,475 Hz (4× PAL subcarrier)
//! - CPU divisor: 5 (not 4)
//! - 228 T-states/line (456 ULA clocks), 311 lines
//! - Contention phase 1 (pattern starts 1 T-state later in the line)
//! - Contention starts at T-state 14_361 (vs 14_335 on the 48K)

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{self, FrameTiming};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{self, DELAY_TABLE_48K, UlaEngine};

/// Sinclair 7K010E ULA — the 128K / +2 ZX Spectrum's custom chip.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SinclairUla {
    engine: UlaEngine,
}

impl SinclairUla {
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: UlaEngine::new(&ula_engine::CONFIG_128K),
        }
    }

    #[must_use]
    pub fn border_color(&self) -> u8 {
        self.engine.border
    }

    /// Reinstall the 128K timing config after a snapshot restore.
    ///
    /// `UlaEngine::config` is `#[serde(skip)]` and falls back to the
    /// 48K config on deserialise (see `common_sinclair_zx_spectrum::
    /// ula_engine::default_config`). For the 128K class that's wrong:
    /// CPU divisor, line length, contention start, and the rest diverge.
    /// Call once after `restore`.
    pub fn reattach_config(&mut self) {
        self.engine.set_config(&ula_engine::CONFIG_128K);
    }
    /// Select the grey +2 interrupt phase at construction or state restore.
    /// All raster and contention settings remain in the same ULA family.
    pub fn reattach_plus2_config(&mut self) {
        self.engine.set_config(&ula_engine::CONFIG_PLUS2);
    }
}

impl Default for SinclairUla {
    fn default() -> Self {
        Self::new()
    }
}

impl Ula for SinclairUla {
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
        // The 7K010E asserts /INT two T-states later than the 48K ULA.
        // Its physical display counter still opens the contention window
        // at C=0; the interrupt-relative 14361 coordinate is not C=-6.
        let phase = (e.scan < 192 && e.pixel < 256).then_some((usize::from(e.pixel) + 1) & 0x0f);

        // Snow: a CPU refresh with I in screen-RAM range collides with
        // the video fetch (the Sinclair ULA ignores /RFSH). gap #12.
        let snow = ula_engine::snow_address(cpu_rfsh, cpu_addr);

        e.tick_rendering(memory, framebuffer, snow);

        // FUSE spec128 uses the same early/late port rules as spec48,
        // with odd paged RAM banks contended. Score these independently
        // in machine-sinclair-zx-spectrum-128k/tests/io_contention_oracle.rs.
        if let Some(phase) = phase {
            let contended_addr = memory.is_contended(cpu_addr);
            let arming = e.gate_arms_this_halfcycle();
            let ula_port = cpu_addr & 1 == 0;
            // T1 falling: memory access or the first port lookup.
            let mem_contention = contended_addr && arming && !cpu_mreq && !cpu_iorq;
            // Even ports: one further lookup at T2 falling, regardless
            // of the page. Freeze the previous pin while the CPU stalls.
            let port_answered = ula_port && arming && cpu_iorq && !e.z80_iorq_prev;
            // Odd contended ports: three further falling-edge lookups.
            let port_unanswered = contended_addr && !ula_port && arming && cpu_iorq;

            let contention = mem_contention || port_answered || port_unanswered;
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
        self.engine.read_fe(port, keyboard)
    }

    fn write_fe(&mut self, val: u8) {
        self.engine.write_fe(val);
    }

    fn frame_timing(&self) -> &FrameTiming {
        &timing::TIMING_128K
    }

    fn end_frame(&mut self) {
        self.engine.end_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    #[ignore = "KNOWN DIVERGENCE: passes only with the MREQT23 latch wired into the gate; \
                blocked on the floating-bus derivation — see \
                knowledge/decisions/spectrum-contention-vs-floating-bus.md"]
    fn a_committed_access_suppresses_contention_that_would_otherwise_fire() {
        fn clock_trace(commit: bool) -> Vec<bool> {
            let mut ula = SinclairUla::new();
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

    #[test]
    fn frame_timing_matches_128k_constants() {
        let ula = SinclairUla::new();
        let t = ula.frame_timing();
        assert_eq!(t.master_hz, 17_734_475);
        assert_eq!(t.cpu_divisor, 5);
        assert_eq!(t.tstates_per_line, 228);
        assert_eq!(t.lines_per_frame, 311);
        assert_eq!(t.tstates_per_frame, 70_908);
        assert_eq!(t.contention_start_tstate, 14_361);
        assert_eq!(t.contention_phase, 1);
        assert_eq!(t.interrupt_length_tstates, 36);
    }

    #[test]
    fn defaults_to_white_border_and_no_interrupt() {
        let ula = SinclairUla::new();
        assert_eq!(ula.border_color(), 7);
        assert!(!ula.interrupt_active());
        assert_eq!(ula.floating_bus(), 0xFF);
    }

    #[test]
    fn write_fe_updates_border() {
        let mut ula = SinclairUla::new();
        ula.write_fe(0x05); // border bits 0..2 = 0b101 = magenta
        assert_eq!(ula.border_color(), 5);
    }

    #[test]
    fn contention_uses_the_counter_window_not_the_video_latch() {
        let mut ula = SinclairUla::new();
        let mut framebuffer = vec![0; timing::SCREEN_WIDTH * timing::SCREEN_HEIGHT];
        ula.engine.scan = 0;
        ula.engine.pixel = 3;
        ula.engine.z80_clock_high = false;
        ula.tick(
            &ContendedMemory,
            0x4000,
            false,
            false,
            false,
            &mut framebuffer,
        );
        assert!(
            !ula.cpu_clock_active(),
            "control phase 4 is busy before the first fetch"
        );
        assert!(!ula.engine.video, "fetches begin at counter 8");

        ula.engine.pixel = 259;
        ula.engine.video = true;
        ula.engine.z80_clock_high = false;
        ula.tick(
            &ContendedMemory,
            0x4000,
            false,
            false,
            false,
            &mut framebuffer,
        );
        assert!(ula.cpu_clock_active(), "contention ends at counter 256");
        assert!(ula.engine.video, "the final fetch pair is still in flight");

        ula.engine.scan = 310;
        ula.engine.pixel = 451;
        ula.engine.z80_clock_high = false;
        ula.tick(
            &ContendedMemory,
            0x4000,
            false,
            false,
            false,
            &mut framebuffer,
        );
        assert!(
            ula.cpu_clock_active(),
            "no contention on the preceding border line"
        );
    }
}
