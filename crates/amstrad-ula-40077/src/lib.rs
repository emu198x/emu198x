//! Amstrad 40077 gate array — the +2A / +2B / +3 ZX Spectrum's custom chip.
//!
//! Source references:
//! - `wiki/chips/amstrad-40077.md`
//! - `wiki/systems/spectrum/contention.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/amstrad-ula-40077/src/lib.rs`
//!
//! Key differences from the Sinclair ULAs:
//! - Different contention pattern: `[1, 0, 7, 6, 5, 4, 3, 2]`
//! - **No I/O contention** — the gate array is MREQ-only
//! - **No internal-op contention** — MREQ is not active during internal cycles
//! - Different contended-bank rules at `$C000` (banks 4–7, not odd banks)
//! - **No floating bus** — reads from unattached ports always return `$FF`
//!
//! Same crystal and line layout as the 128K (17.734475 MHz, 228 T/line, 311 lines).

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{self, FrameTiming};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{self, DELAY_TABLE_PLUS2A, UlaEngine};

/// Amstrad 40077 gate array.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AmstradGateArray {
    engine: UlaEngine,
}

impl AmstradGateArray {
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: UlaEngine::new(&ula_engine::CONFIG_PLUS2A),
        }
    }

    #[must_use]
    pub fn border_color(&self) -> u8 {
        self.engine.border
    }

    /// Reinstall the +2A/+3 timing config after a snapshot restore.
    ///
    /// `UlaEngine::config` is `#[serde(skip)]` and falls back to the
    /// 48K config on deserialise (see `common_sinclair_zx_spectrum::
    /// ula_engine::default_config`). The Amstrad gate-array uses
    /// CONFIG_PLUS2A — different contention pattern, different line
    /// length. Call once after `restore`.
    pub fn reattach_config(&mut self) {
        self.engine.set_config(&ula_engine::CONFIG_PLUS2A);
    }
}

impl Default for AmstradGateArray {
    fn default() -> Self {
        Self::new()
    }
}

impl Ula for AmstradGateArray {
    fn tick(
        &mut self,
        memory: &dyn MemoryBus,
        cpu_addr: u16,
        cpu_mreq: bool,
        cpu_iorq: bool,
        framebuffer: &mut [u8],
    ) {
        let e = &mut self.engine;
        let phase = (e.pixel as usize) & 0x0F;

        e.tick_rendering(memory, framebuffer);

        // Amstrad contention: MREQ-only. No I/O contention, no internal
        // contention. The gate array contends only when the CPU is mid
        // memory access (MREQ asserted) and the address is in a
        // contended range. This is the inverse of the Sinclair model
        // (which contends when MREQ is *not* yet active to gate the
        // upcoming clock).
        if e.video {
            let contended_addr = memory.is_contended(cpu_addr);
            let contention = contended_addr && cpu_mreq && e.z80_clock_high;
            e.cpu_clock = !(contention && DELAY_TABLE_PLUS2A[phase]);
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
        // The Amstrad gate array does not expose the data bus on
        // unattached ports — reads always return $FF.
        0xFF
    }

    fn read_fe(&self, port: u16, keyboard: &[u8; 8]) -> u8 {
        self.engine.read_fe(port, keyboard)
    }

    fn write_fe(&mut self, val: u8) {
        self.engine.write_fe(val);
    }

    fn frame_timing(&self) -> &FrameTiming {
        &timing::TIMING_PLUS2A
    }

    fn end_frame(&mut self) {
        self.engine.end_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_timing_matches_plus2a_constants() {
        let g = AmstradGateArray::new();
        let t = g.frame_timing();
        assert_eq!(t.master_hz, 17_734_475);
        assert_eq!(t.cpu_divisor, 5);
        assert_eq!(t.tstates_per_line, 228);
        assert_eq!(t.lines_per_frame, 311);
        assert_eq!(t.tstates_per_frame, 70_908);
        assert_eq!(t.contention_pattern, [1, 0, 7, 6, 5, 4, 3, 2]);
        assert_eq!(t.contention_phase, 0);
    }

    #[test]
    fn floating_bus_always_returns_high() {
        let g = AmstradGateArray::new();
        assert_eq!(g.floating_bus(), 0xFF);
    }

    #[test]
    fn defaults_to_white_border() {
        let g = AmstradGateArray::new();
        assert_eq!(g.border_color(), 7);
        assert!(!g.interrupt_active());
    }

    #[test]
    fn write_fe_updates_border() {
        let mut g = AmstradGateArray::new();
        g.write_fe(0x02); // border = red (0b010)
        assert_eq!(g.border_color(), 2);
    }
}
