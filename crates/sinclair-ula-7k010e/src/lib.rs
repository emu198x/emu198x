//! Sinclair 7K010E ULA — the 128K / +2 ZX Spectrum's custom chip.
//!
//! Source references:
//! - `wiki/chips/sinclair-7k010e.md`
//! - `wiki/systems/spectrum/contention.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/sinclair-ula-7k010e/src/lib.rs`
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
        framebuffer: &mut [u8],
    ) {
        let e = &mut self.engine;
        let phase = (e.pixel as usize) & 0x0F;

        e.tick_rendering(memory, framebuffer);

        // Contention: same model as 48K (memory + I/O), same delay table.
        // The phase difference (`contention_phase: 1`) is a property of the
        // 5-divisor crystal — the ULA's 16-clock cell aligns differently
        // relative to the CPU than on the 48K's 4-divisor machine.
        if e.video {
            let contended_addr = memory.is_contended(cpu_addr);
            let mem_contention = contended_addr && e.z80_clock_high && !cpu_mreq;

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
}
