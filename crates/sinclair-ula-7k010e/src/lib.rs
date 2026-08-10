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
        let next_scan = if e.scan + 1 == 311 { 0 } else { e.scan + 1 };
        let contention_pixel = if next_scan < 192 && e.pixel >= 450 {
            Some(e.pixel - 450)
        } else if e.scan < 192 && e.pixel < 250 {
            Some(e.pixel + 6)
        } else {
            None
        };
        // HALT2INT128's early-128K hardware profile fixes the delay-table
        // origin one ULA pixel after this logical /Border coordinate. The
        // alternating Z80 clock level then produces the documented
        // T-state contention ramp across the two-pixel CPU clock cells.
        let phase = contention_pixel.map(|pixel| ((pixel as usize) + 1) & 0x0F);

        // Snow: a CPU refresh with I in screen-RAM range collides with
        // the video fetch (the Sinclair ULA ignores /RFSH). gap #12.
        let snow = ula_engine::snow_address(cpu_rfsh, cpu_addr);

        e.tick_rendering(memory, framebuffer, snow);

        // Contention: same delay pattern as the 48K, advanced by one
        // T-state. The delay table is indexed in ULA pixels, hence the
        // two-pixel conversion above. Contention follows /Border rather
        // than the later video-fetch window: on the 128K it begins at
        // T=14361, while the floating bus does not expose the first fetch
        // until T=14364.
        if let Some(phase) = phase {
            let contended_addr = memory.is_contended(cpu_addr);
            // `/MREQT23` — see `UlaEngine::mreq_t23`. Keying off
            // `!cpu_mreq` alone lets the gate re-arm in `T3`, while the
            // contended address is still on the bus, and charges a
            // second full rotation to every M-cycle past `M1`.
            // `/MREQT23` — see `UlaEngine::mreq_t23`. Keying off
            // `!cpu_mreq` alone lets the gate re-arm in `T3`, while the
            // contended address is still on the bus, and charges a
            // second full rotation to every M-cycle past `M1`.
            let mem_contention = contended_addr && e.z80_clock_high && !e.mreq_t23;

            let io_even_port = (cpu_addr & 1) == 0;
            let io_contention = (cpu_iorq || e.z80_iorq_prev) && io_even_port && e.z80_clock_high;

            let contention = mem_contention || io_contention;
            e.cpu_clock = !(contention && DELAY_TABLE_48K[phase]);
        } else {
            e.cpu_clock = true;
        }

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
    fn contention_starts_before_video_fetch_with_phase_one_offset() {
        let mut ula = SinclairUla::new();
        let mut framebuffer = vec![0; timing::SCREEN_WIDTH * timing::SCREEN_HEIGHT];
        let tick = |ula: &mut SinclairUla, framebuffer: &mut [u8]| {
            ula.tick(&ContendedMemory, 0x4000, false, false, false, framebuffer);
        };

        assert!(ula.engine.border_active);
        tick(&mut ula, &mut framebuffer);
        assert!(
            !ula.engine.border_active,
            "the active-display contention window must open at pixel 0"
        );
        assert!(
            !ula.engine.video,
            "video fetch must not start before pixel 4"
        );
        assert!(
            !ula.engine.cpu_clock,
            "the leading edge of the 128K contention window withholds the CPU clock"
        );

        tick(&mut ula, &mut framebuffer);
        assert!(!ula.engine.video);
        assert!(!ula.engine.cpu_clock);

        tick(&mut ula, &mut framebuffer);
        assert!(!ula.engine.video, "video fetch has not started at pixel 2");
        assert!(
            !ula.engine.cpu_clock,
            "phase-one contention must withhold the CPU clock before video fetch"
        );
    }

    #[test]
    fn contention_opens_before_the_active_line_border_latch() {
        let mut ula = SinclairUla::new();
        let mut framebuffer = vec![0; timing::SCREEN_WIDTH * timing::SCREEN_HEIGHT];
        ula.engine.scan = 310;
        ula.engine.pixel = 449;
        ula.engine.z80_clock_high = false;

        ula.tick(
            &ContendedMemory,
            0x4000,
            false,
            false,
            false,
            &mut framebuffer,
        );
        assert_eq!(ula.engine.pixel, 450);
        assert!(ula.engine.cpu_clock);
        assert!(ula.engine.z80_clock_high);
        assert!(
            ula.engine.border_active,
            "rendering border remains active before the next line",
        );

        ula.tick(
            &ContendedMemory,
            0x4000,
            false,
            false,
            false,
            &mut framebuffer,
        );
        assert_eq!(ula.engine.pixel, 451);
        assert!(
            ula.engine.cpu_clock,
            "delay-table phase 1 does not yet withhold the CPU clock",
        );
        assert!(
            ula.engine.border_active,
            "contention phase selection must not depend on the rendering-border latch",
        );

        ula.tick(
            &ContendedMemory,
            0x4000,
            false,
            false,
            false,
            &mut framebuffer,
        );
        assert_eq!(ula.engine.pixel, 452);
        assert!(ula.engine.cpu_clock);

        ula.tick(
            &ContendedMemory,
            0x4000,
            false,
            false,
            false,
            &mut framebuffer,
        );
        assert_eq!(ula.engine.pixel, 453);
        assert!(
            !ula.engine.cpu_clock,
            "the phase-3 delay must withhold the CPU before the next line",
        );
        assert!(
            ula.engine.border_active,
            "contention must not depend on the rendering-border latch",
        );
    }
}
