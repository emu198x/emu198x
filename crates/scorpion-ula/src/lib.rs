//! Scorpion ZS-256 ULA — Russian extended Spectrum.
//!
//! Source references:
//! - `wiki/chips/scorpion-ula.md`
//! - Adapted from `/Users/stevehill/Projects/198x/Emu198x-Older/crates/scorpion-ula/src/lib.rs`
//!
//! Same crystal and frame geometry as the 48K (14 MHz, 224 T/line, 312
//! lines, 69,888 T/frame) but no contention — the Scorpion ULA never
//! gates the CPU clock.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{self, FrameTiming};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{self, UlaEngine};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ScorpionUla {
    engine: UlaEngine,
}

impl ScorpionUla {
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: UlaEngine::new(&ula_engine::CONFIG_48K),
        }
    }

    #[must_use]
    pub fn border_color(&self) -> u8 {
        self.engine.border
    }
}

impl Default for ScorpionUla {
    fn default() -> Self {
        Self::new()
    }
}

impl Ula for ScorpionUla {
    fn tick(
        &mut self,
        memory: &dyn MemoryBus,
        _cpu_addr: u16,
        _cpu_mreq: bool,
        cpu_iorq: bool,
        framebuffer: &mut [u8],
    ) {
        self.engine.tick_rendering(memory, framebuffer);
        self.engine.cpu_clock = true; // No contention.
        self.engine.track_z80_clock(cpu_iorq, false);
    }

    fn cpu_clock_active(&self) -> bool {
        true
    }

    fn interrupt_active(&self) -> bool {
        self.engine.int_active
    }

    fn floating_bus(&self) -> u8 {
        0xFF
    }

    fn read_fe(&self, port: u16, keyboard: &[u8; 8]) -> u8 {
        self.engine.read_fe(port, keyboard)
    }

    fn write_fe(&mut self, val: u8) {
        self.engine.write_fe(val);
    }

    fn frame_timing(&self) -> &FrameTiming {
        &timing::TIMING_SCORPION
    }

    fn end_frame(&mut self) {
        self.engine.end_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_timing_matches_scorpion_constants() {
        let u = ScorpionUla::new();
        let t = u.frame_timing();
        assert_eq!(t.master_hz, 14_000_000);
        assert_eq!(t.tstates_per_frame, 69_888);
        assert_eq!(t.contention_pattern, [0; 8]);
    }

    #[test]
    fn cpu_clock_is_always_active() {
        let u = ScorpionUla::new();
        assert!(u.cpu_clock_active());
    }
}
