//! Ferranti 6C001E ULA wrapper.
//!
//! Source references:
//! - `wiki/chips/ferranti-6c001e.md`
//! - `wiki/systems/spectrum/contention.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/ferranti-ula-6c001e/src/lib.rs`

use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{self, FrameTiming};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::{self, DELAY_TABLE_48K, UlaEngine};

/// Ferranti 6C001E ULA — the 48K ZX Spectrum's custom chip.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct FerrantiUla {
    engine: UlaEngine,
    issue: BoardIssue,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BoardIssue {
    Issue2,
    Issue3,
}

impl FerrantiUla {
    pub fn new(issue: BoardIssue) -> Self {
        Self {
            engine: UlaEngine::new(&ula_engine::CONFIG_48K),
            issue,
        }
    }

    #[must_use]
    pub const fn issue(&self) -> BoardIssue {
        self.issue
    }

    #[must_use]
    pub fn border_color(&self) -> u8 {
        self.engine.border
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
    /// the last write to port $FE back onto bit 6 — but Issue 2 and
    /// Issue 3 boards do this differently:
    ///
    /// - **Issue 2**: bit 6 reads as `(MIC | EAR)` from the last write.
    ///   Either bit 3 (MIC) or bit 4 (EAR) being high drives bit 6 high.
    /// - **Issue 3**: bit 6 reads as just `EAR` from the last write. Bit 3
    ///   (MIC) alone does not set bit 6 high.
    ///
    /// Games that probe the board revision use exactly this distinction:
    /// write `$08` to `$FE`, read back, and check bit 6.
    fn ear_feedback_bit(&self) -> u8 {
        let beeper_bit = self.engine.beeper;
        let mic_bit = self.engine.mic;
        let high = match self.issue {
            BoardIssue::Issue2 => beeper_bit || mic_bit,
            BoardIssue::Issue3 => beeper_bit,
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
        framebuffer: &mut [u8],
    ) {
        let e = &mut self.engine;
        let phase = (e.pixel as usize) & 0x0F;

        // Rendering: video fetch, pixel output, counters, interrupt
        e.tick_rendering(memory, framebuffer);

        // Contention (48K model): memory + I/O + internal
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
        // Bit 6 (EAR) needs issue-specific handling — the shared engine
        // always returns bit 6 high, which is only correct when the tape
        // input is idle on an Issue 3 board with no recent writes.
        let mut val = self.engine.read_fe(port, keyboard);
        val &= !0x40; // clear bit 6 so we can set it based on the issue
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

    fn empty_keyboard() -> [u8; 8] {
        [0xFF; 8]
    }

    #[test]
    fn issue3_ear_reflects_only_bit4() {
        let mut ula = FerrantiUla::new(BoardIssue::Issue3);

        // Write $00: everything clear. Bit 6 should be low.
        ula.write_fe(0x00);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x00);

        // Write $08 (MIC only): on Issue 3, bit 6 stays low.
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
    fn issue2_ear_reflects_mic_or_ear() {
        let mut ula = FerrantiUla::new(BoardIssue::Issue2);

        // Write $00: everything clear. Bit 6 should be low.
        ula.write_fe(0x00);
        assert_eq!(ula.read_fe(0xFFFE, &empty_keyboard()) & 0x40, 0x00);

        // Write $08 (MIC only): on Issue 2, bit 6 goes high.
        // This is the key distinction from Issue 3.
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
