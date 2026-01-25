//! NES machine emulation.

use crate::memory::NesMemory;
use crate::ppu::Ppu;
use cpu_6502::Mos6502;
use emu_core::Cpu;

/// NTSC timing constants.
pub mod ntsc {
    /// CPU cycles per scanline.
    pub const CYCLES_PER_LINE: u32 = 114; // ~113.67, rounded
    /// Visible scanlines.
    pub const VISIBLE_LINES: u32 = 240;
    /// Total scanlines including vblank.
    pub const LINES_PER_FRAME: u32 = 262;
    /// CPU cycles per frame.
    pub const CYCLES_PER_FRAME: u32 = 29781; // 262 * 341 / 3
}

/// PAL timing constants.
pub mod pal {
    /// CPU cycles per scanline.
    pub const CYCLES_PER_LINE: u32 = 107;
    /// Visible scanlines.
    pub const VISIBLE_LINES: u32 = 240;
    /// Total scanlines including vblank.
    pub const LINES_PER_FRAME: u32 = 312;
    /// CPU cycles per frame.
    pub const CYCLES_PER_FRAME: u32 = 33248;
}

/// NES timing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingMode {
    /// NTSC (60 Hz, 262 scanlines).
    Ntsc,
    /// PAL (50 Hz, 312 scanlines).
    Pal,
}

/// The Nintendo Entertainment System.
pub struct Nes {
    /// 2A03 CPU (6502 without BCD).
    cpu: Mos6502,
    /// PPU (picture processing unit).
    pub ppu: Ppu,
    /// Memory/bus subsystem.
    pub memory: NesMemory,
    /// Timing mode.
    timing: TimingMode,
    /// Frame cycle counter.
    frame_cycles: u32,
    /// Total cycles executed.
    total_cycles: u64,
    /// Video output buffer (256x240, indexed color).
    pub video_buffer: Vec<u8>,
}

impl Nes {
    /// Create a new NES with NTSC timing.
    pub fn new() -> Self {
        Self::with_timing(TimingMode::Ntsc)
    }

    /// Create a new NES with specified timing mode.
    pub fn with_timing(timing: TimingMode) -> Self {
        Self {
            cpu: Mos6502::new_2a03(), // 2A03: 6502 without decimal mode
            ppu: Ppu::new(),
            memory: NesMemory::new(),
            timing,
            frame_cycles: 0,
            total_cycles: 0,
            video_buffer: vec![0; 256 * 240],
        }
    }

    /// Get timing mode.
    pub fn timing(&self) -> TimingMode {
        self.timing
    }

    /// Reset the NES.
    pub fn reset(&mut self) {
        self.cpu.reset(&mut self.memory);
        self.ppu.reset();
        self.frame_cycles = 0;
    }

    /// Run for one frame.
    pub fn run_frame(&mut self) {
        let cycles_per_frame = match self.timing {
            TimingMode::Ntsc => ntsc::CYCLES_PER_FRAME,
            TimingMode::Pal => pal::CYCLES_PER_FRAME,
        };

        self.frame_cycles = 0;

        while self.frame_cycles < cycles_per_frame {
            self.step();
        }
    }

    /// Run a single CPU step.
    pub fn step(&mut self) -> u32 {
        let cycles = self.cpu.step(&mut self.memory);

        // PPU runs 3x faster than CPU
        for _ in 0..(cycles * 3) {
            let (nmi, _pixel) = self.ppu.tick(&mut self.memory);
            if nmi {
                self.cpu.nmi(&mut self.memory);
            }
        }

        self.frame_cycles += cycles;
        self.total_cycles += cycles as u64;

        cycles
    }

    /// Load a cartridge.
    pub fn load_cartridge(&mut self, cartridge: crate::Cartridge) {
        self.memory.load_cartridge(cartridge);
        self.reset();
    }

    /// Set controller state.
    pub fn set_controller(&mut self, controller: u8) {
        self.memory.controller_state = controller;
    }

    /// Get total cycles executed.
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }
}

impl Default for Nes {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_nes() {
        let nes = Nes::new();
        assert_eq!(nes.timing(), TimingMode::Ntsc);
    }
}
