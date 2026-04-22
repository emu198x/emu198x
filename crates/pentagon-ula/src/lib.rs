//! Pentagon ULA — the simplest Spectrum-family ULA.
//!
//! Source references:
//! - `wiki/chips/pentagon-ula.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/pentagon-ula/src/lib.rs`
//!
//! The Pentagon was a Russian Spectrum clone (mass-produced 1991-1995).
//! Its ULA never gates the CPU clock — there is no memory contention,
//! no I/O contention, no internal-op contention. Programs that race the
//! beam on a real Spectrum have to be timed differently for the Pentagon.
//!
//! Crystal: 14.336 MHz (slightly faster than the 48K's 14 MHz).
//! CPU clock: 3.584 MHz. Frame: 224 T-states/line × 320 lines = 71,680 T.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{self, FrameTiming};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{self, UlaEngine};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PentagonUla {
    engine: UlaEngine,
}

impl PentagonUla {
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: UlaEngine::new(&ula_engine::CONFIG_PENTAGON),
        }
    }

    #[must_use]
    pub fn border_color(&self) -> u8 {
        self.engine.border
    }
}

impl Default for PentagonUla {
    fn default() -> Self {
        Self::new()
    }
}

impl Ula for PentagonUla {
    fn tick(
        &mut self,
        memory: &dyn MemoryBus,
        _cpu_addr: u16,
        _cpu_mreq: bool,
        cpu_iorq: bool,
        framebuffer: &mut [u8],
    ) {
        self.engine.tick_rendering(memory, framebuffer);
        // No contention — CPU clock is always live.
        self.engine.cpu_clock = true;
        self.engine.track_z80_clock(cpu_iorq, false);
    }

    fn cpu_clock_active(&self) -> bool {
        true
    }

    fn interrupt_active(&self) -> bool {
        self.engine.int_active
    }

    fn floating_bus(&self) -> u8 {
        // Pentagon revisions vary on what unattached-port reads return.
        // Most emulators settle on $FF.
        0xFF
    }

    fn read_fe(&self, port: u16, keyboard: &[u8; 8]) -> u8 {
        self.engine.read_fe(port, keyboard)
    }

    fn write_fe(&mut self, val: u8) {
        self.engine.write_fe(val);
    }

    fn frame_timing(&self) -> &FrameTiming {
        &timing::TIMING_PENTAGON
    }

    fn end_frame(&mut self) {
        self.engine.end_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_timing_matches_pentagon_constants() {
        let u = PentagonUla::new();
        let t = u.frame_timing();
        assert_eq!(t.master_hz, 14_336_000);
        assert_eq!(t.cpu_divisor, 4);
        assert_eq!(t.tstates_per_line, 224);
        assert_eq!(t.lines_per_frame, 320);
        assert_eq!(t.tstates_per_frame, 71_680);
        assert_eq!(t.contention_pattern, [0; 8]);
    }

    #[test]
    fn cpu_clock_is_always_active() {
        let u = PentagonUla::new();
        assert!(u.cpu_clock_active());
    }
}
