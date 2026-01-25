//! C128 machine emulation.
//!
//! The C128 integrates two CPUs (8502 and Z80), dual video output
//! (VIC-II and VDC), and extended memory management.

use crate::memory::C128Memory;
use cpu_6502::Mos6502;
use cpu_z80::Z80;
use emu_core::Cpu;
use machine_c64::{SidRevision, TimingMode, VdcRevision};

/// PAL timing constants (VIC-II 6569).
pub mod pal {
    pub const CYCLES_PER_LINE: u32 = 63;
    pub const LINES_PER_FRAME: u32 = 312;
    pub const CYCLES_PER_FRAME: u32 = CYCLES_PER_LINE * LINES_PER_FRAME; // 19656
}

/// NTSC timing constants (VIC-II 6567).
pub mod ntsc {
    pub const CYCLES_PER_LINE: u32 = 65;
    pub const LINES_PER_FRAME: u32 = 263;
    pub const CYCLES_PER_FRAME: u32 = CYCLES_PER_LINE * LINES_PER_FRAME; // 17095
}

/// C128 machine configuration.
#[derive(Clone)]
pub struct C128Config {
    /// Timing mode (PAL/NTSC).
    pub timing: TimingMode,
    /// VDC revision (8563 16K or 8568 64K).
    pub vdc_revision: VdcRevision,
    /// SID revision (6581 or 8580).
    pub sid_revision: SidRevision,
    /// Enable second SID (C128D/DCR).
    pub dual_sid: bool,
}

impl Default for C128Config {
    fn default() -> Self {
        Self {
            timing: TimingMode::Pal,
            vdc_revision: VdcRevision::Vdc8568,
            sid_revision: SidRevision::Mos8580,
            dual_sid: false,
        }
    }
}

/// The Commodore 128 computer.
pub struct C128 {
    /// 8502 CPU (6510-compatible).
    cpu_8502: Mos6502,
    /// Z80 CPU (for CP/M).
    cpu_z80: Z80,
    /// Memory subsystem.
    pub memory: C128Memory,
    /// Machine configuration.
    config: C128Config,
    /// Current frame cycle count.
    frame_cycles: u32,
    /// Total cycles executed.
    total_cycles: u64,
    /// 40-column video buffer (VIC-II, 320x200 or 320x400).
    pub vic_buffer: Vec<u8>,
    /// 80-column video buffer (VDC, 640x200 or 640x400).
    pub vdc_buffer: Vec<u8>,
    /// Cycles per frame (varies by timing mode).
    cycles_per_frame: u32,
}

impl C128 {
    /// Create a new C128 with default configuration.
    pub fn new() -> Self {
        Self::with_config(C128Config::default())
    }

    /// Create a new C128 with specified configuration.
    pub fn with_config(config: C128Config) -> Self {
        let cycles_per_frame = match config.timing {
            TimingMode::Pal | TimingMode::PalN => pal::CYCLES_PER_FRAME,
            TimingMode::Ntsc => ntsc::CYCLES_PER_FRAME,
        };

        Self {
            cpu_8502: Mos6502::new(),
            cpu_z80: Z80::new(),
            memory: C128Memory::new(),
            config,
            frame_cycles: 0,
            total_cycles: 0,
            vic_buffer: vec![0; 320 * 200],
            vdc_buffer: vec![0; 640 * 200],
            cycles_per_frame,
        }
    }

    /// Get the machine configuration.
    pub fn config(&self) -> &C128Config {
        &self.config
    }

    /// Get timing mode.
    pub fn timing(&self) -> TimingMode {
        self.config.timing
    }

    /// Check if in C64 mode.
    pub fn is_c64_mode(&self) -> bool {
        self.memory.is_c64_mode()
    }

    /// Check if Z80 CPU is active.
    pub fn is_z80_mode(&self) -> bool {
        self.memory.is_z80_mode()
    }

    /// Reset the C128.
    pub fn reset(&mut self) {
        self.cpu_8502.reset(&mut self.memory);
        // Z80 reset - starts at address 0
        self.cpu_z80.set_pc(0);
        self.memory.mmu.reset();
        self.frame_cycles = 0;
    }

    /// Run for one frame.
    pub fn run_frame(&mut self) {
        self.frame_cycles = 0;
        self.memory.cycles = 0;

        while self.frame_cycles < self.cycles_per_frame {
            self.step();
        }
    }

    /// Run for a single CPU step.
    pub fn step(&mut self) -> u32 {
        let start_cycles = self.memory.cycles;

        // Execute on the active CPU
        let cycles = if self.memory.is_z80_mode() {
            // Z80 mode: run Z80 CPU
            // Note: Z80 runs at 4MHz, so it executes ~4x as many cycles
            // For now we'll run it at 1:1 cycle ratio for simplicity
            self.cpu_z80.step(&mut self.memory)
        } else {
            // 8502 mode: run 8502 CPU
            self.cpu_8502.step(&mut self.memory)
        };

        // Update timing
        let elapsed = self.memory.cycles.saturating_sub(start_cycles);
        self.frame_cycles += elapsed.max(cycles);
        self.total_cycles += elapsed.max(cycles) as u64;

        // Tick timers
        let irq = self.memory.tick_cia1(elapsed);
        let nmi = self.memory.tick_cia2(elapsed);

        // Handle interrupts
        if irq {
            if self.memory.is_z80_mode() {
                self.cpu_z80.interrupt(&mut self.memory);
            } else {
                self.cpu_8502.interrupt(&mut self.memory);
            }
        }

        if nmi {
            if self.memory.is_z80_mode() {
                self.cpu_z80.nmi(&mut self.memory);
            } else {
                self.cpu_8502.nmi(&mut self.memory);
            }
        }

        cycles
    }

    /// Load C128 BASIC ROM (low part).
    pub fn load_basic_lo(&mut self, data: &[u8]) {
        self.memory.load_basic_lo(data);
    }

    /// Load C128 BASIC ROM (high part).
    pub fn load_basic_hi(&mut self, data: &[u8]) {
        self.memory.load_basic_hi(data);
    }

    /// Load C128 KERNAL ROM.
    pub fn load_kernal(&mut self, data: &[u8]) {
        self.memory.load_kernal(data);
    }

    /// Load Screen Editor ROM.
    pub fn load_editor(&mut self, data: &[u8]) {
        self.memory.load_editor(data);
    }

    /// Load Character ROM.
    pub fn load_chargen(&mut self, data: &[u8]) {
        self.memory.load_chargen(data);
    }

    /// Load C64 BASIC ROM (for C64 mode).
    pub fn load_c64_basic(&mut self, data: &[u8]) {
        self.memory.load_c64_basic(data);
    }

    /// Load C64 KERNAL ROM (for C64 mode).
    pub fn load_c64_kernal(&mut self, data: &[u8]) {
        self.memory.load_c64_kernal(data);
    }

    /// Enter C64 mode.
    pub fn enter_c64_mode(&mut self) {
        self.memory.enter_c64_mode();
        self.cpu_8502.reset(&mut self.memory);
    }

    /// Exit C64 mode (return to C128 mode).
    pub fn exit_c64_mode(&mut self) {
        self.memory.exit_c64_mode();
        self.cpu_8502.reset(&mut self.memory);
    }

    /// Set a key in the keyboard matrix.
    pub fn set_key(&mut self, row: u8, col: u8, pressed: bool) {
        if row < 11 && col < 8 {
            if pressed {
                self.memory.keyboard_matrix[row as usize] &= !(1 << col);
            } else {
                self.memory.keyboard_matrix[row as usize] |= 1 << col;
            }
        }
    }

    /// Release all keys.
    pub fn release_all_keys(&mut self) {
        for row in &mut self.memory.keyboard_matrix {
            *row = 0xFF;
        }
    }

    /// Get total cycles executed.
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Get current frame cycle count.
    pub fn frame_cycles(&self) -> u32 {
        self.frame_cycles
    }

    /// Get pending SID writes.
    pub fn take_sid_writes(&mut self) -> Vec<(u8, u8)> {
        self.memory.take_sid_writes()
    }

    /// Get the current raster line.
    pub fn raster_line(&self) -> u16 {
        self.memory.current_raster_line
    }

    /// Set the current raster line (called by video rendering).
    pub fn set_raster_line(&mut self, line: u16) {
        self.memory.current_raster_line = line;
    }

    /// Get reference to the VDC.
    pub fn vdc(&self) -> &machine_c64::Vdc {
        &self.memory.vdc
    }

    /// Get mutable reference to the VDC.
    pub fn vdc_mut(&mut self) -> &mut machine_c64::Vdc {
        &mut self.memory.vdc
    }

    /// Get reference to the MMU.
    pub fn mmu(&self) -> &machine_c64::Mmu {
        &self.memory.mmu
    }

    /// Get mutable reference to the MMU.
    pub fn mmu_mut(&mut self) -> &mut machine_c64::Mmu {
        &mut self.memory.mmu
    }

    /// Get the 8502 CPU state.
    pub fn cpu_8502(&self) -> &Mos6502 {
        &self.cpu_8502
    }

    /// Get the Z80 CPU state.
    pub fn cpu_z80(&self) -> &Z80 {
        &self.cpu_z80
    }

    /// Get the VIC bank base address.
    pub fn vic_bank(&self) -> u16 {
        self.memory.vic_bank()
    }

    /// Read from VIC-II perspective.
    pub fn vic_read(&self, addr: u16) -> u8 {
        self.memory.vic_read(addr)
    }
}

impl Default for C128 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_c128() {
        let c128 = C128::new();
        assert!(!c128.is_c64_mode());
        assert!(!c128.is_z80_mode());
    }

    #[test]
    fn test_c64_mode() {
        let mut c128 = C128::new();
        c128.enter_c64_mode();
        assert!(c128.is_c64_mode());
        c128.exit_c64_mode();
        assert!(!c128.is_c64_mode());
    }

    #[test]
    fn test_reset() {
        let mut c128 = C128::new();
        c128.reset();
        // After reset, CPU should be at reset vector
        // (We haven't loaded ROMs so this is just checking it runs)
    }
}
