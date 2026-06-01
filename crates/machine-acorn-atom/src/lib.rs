//! Acorn Atom (1980) — 6502 + MC6847 text-mode VDG + 6520 PIA.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-acorn-atom/`
//! used the deprecated `emu_core::Bus` callback; the wiring here goes
//! through [`mos_6502::M6502`]'s public pin fields.
//!
//! # The Acorn Atom
//!
//! Acorn's £120 self-build (1980) — designed by Sophie Wilson and
//! Steve Furber, the team that would design the BBC Micro the
//! following year. Used as the platform for several Acornsoft
//! titles and the first commercial release of Elite (an
//! Atom-targeted demo).
//!
//! - **CPU:** MOS 6502 at 1 MHz
//! - **VDG:** Motorola MC6847 text mode (32 × 16 chars, 8 × 12 cell)
//!   — Atom-specific variant with an embedded 64-glyph character
//!   ROM (see [`vdg`])
//! - **PIA:** MOS 6520 at `$B001-$B003`
//! - **RAM:** 2.5 KB base, expandable to 12 KB
//! - **Video RAM:** 1 KB at `$8000-$83FF` (mirrored to `$9FFF`)
//! - **ROM:** 24 KB combined — BASIC (split `$A000` + `$C000`),
//!   FP at `$B004-$BFFF`, OS at `$D000-$FFFF`
//!
//! # I/O
//!
//! - `$B000`: VDG control register (mode select + display flags)
//! - `$B001-$B003`: PIA 6520 (port A column-select / port B row-data)
//!
//! Clock model: one master tick = one 6502 cycle (1 MHz). VDG ticks
//! at the same rate. One PAL frame ≈ 71,136 ticks (228 × 312).
//!
//! Scope of this port: text mode only. Graphics modes 1-4 (semi-
//! graphics) and mode 5 (256 × 192 dot graphics) are stubbed in the
//! VDG and tracked as follow-ups in `docs/status/outstanding-work.md`.

pub mod input;
mod keyboard;
pub mod vdg;

pub use input::AtomKey;
pub use keyboard::KeyboardState;
pub use vdg::{Mc6847, FB_HEIGHT, FB_WIDTH};

use mos_6502::M6502;
use mos_pia_6520::Pia6520;

/// Acorn Atom machine.
pub struct AcornAtom {
    cpu: M6502,
    ram: Vec<u8>,
    ram_size: usize,
    video_ram: [u8; 1024],
    rom: Vec<u8>,
    pia: Pia6520,
    vdg: Mc6847,
    keyboard: KeyboardState,
    master_clock: u64,
    frame_count: u64,
}

impl AcornAtom {
    /// Create a new Atom. `rom` is the combined 24 KB BASIC + FP + OS
    /// blob (BASIC1 at offset 0, FP at $1000, BASIC2 at $2000, OS at
    /// $3000). `ram_size` is 2560-12288 bytes.
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        Self {
            cpu: M6502::new(),
            ram: vec![0; ram_size],
            ram_size,
            video_ram: [0; 1024],
            rom,
            pia: Pia6520::new(),
            vdg: Mc6847::new(),
            keyboard: KeyboardState::new(),
            master_clock: 0,
            frame_count: 0,
        }
    }

    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        for _ in 0..200_000 {
            self.tick();
            if self.vdg.take_frame_complete() {
                break;
            }
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick(&mut self) {
        self.master_clock += 1;
        let video_ram = &self.video_ram;
        self.vdg.tick(|addr| video_ram[(addr & 0x03FF) as usize]);
        self.cpu.irq = self.pia.irq_pending();
        self.cpu.tick();
        if self.cpu.rw {
            self.cpu.data_in = self.mem_read(self.cpu.addr);
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
    }

    fn update_keyboard(&mut self) {
        let col_select = self.pia.port_a_output();
        let row_data = self.keyboard.read(col_select);
        self.pia.set_port_b_input(row_data);
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                if (addr as usize) < self.ram_size {
                    self.ram[addr as usize]
                } else {
                    0xFF
                }
            }
            0x8000..=0x9FFF => self.video_ram[(addr & 0x03FF) as usize],
            0xA000..=0xAFFF => {
                let offset = (addr - 0xA000) as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xB000 => self.vdg.control,
            0xB001..=0xB003 => {
                self.update_keyboard();
                self.pia.read((addr - 0xB000) as u8)
            }
            0xB004..=0xBFFF => {
                let offset = 0x1000 + (addr - 0xB000) as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xC000..=0xCFFF => {
                let offset = 0x2000 + (addr - 0xC000) as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xD000..=0xFFFF => {
                let offset = 0x3000 + (addr - 0xD000) as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => {
                if (addr as usize) < self.ram_size {
                    self.ram[addr as usize] = value;
                }
            }
            0x8000..=0x9FFF => {
                self.video_ram[(addr & 0x03FF) as usize] = value;
            }
            0xB000 => self.vdg.control = value,
            0xB001..=0xB003 => self.pia.write((addr - 0xB000) as u8, value),
            _ => {}
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vdg.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vdg.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vdg.framebuffer_height()
    }

    pub fn press_key(&mut self, key: AtomKey) {
        let (row, col) = key.matrix();
        self.keyboard.set_key(row, col, true);
    }

    pub fn release_key(&mut self, key: AtomKey) {
        let (row, col) = key.matrix();
        self.keyboard.set_key(row, col, false);
    }

    pub fn release_all_keys(&mut self) {
        self.keyboard.release_all();
    }

    #[must_use]
    pub fn peek_memory(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                if (addr as usize) < self.ram_size {
                    self.ram[addr as usize]
                } else {
                    0xFF
                }
            }
            0x8000..=0x9FFF => self.video_ram[(addr & 0x03FF) as usize],
            0xB000 => self.vdg.control,
            0xA000..=0xAFFF => self.rom.get((addr - 0xA000) as usize).copied().unwrap_or(0xFF),
            0xB004..=0xBFFF => self
                .rom
                .get(0x1000 + (addr - 0xB000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xC000..=0xCFFF => self
                .rom
                .get(0x2000 + (addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xD000..=0xFFFF => self
                .rom
                .get(0x3000 + (addr - 0xD000) as usize)
                .copied()
                .unwrap_or(0xFF),
            _ => 0xFF,
        }
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        // 24 KB combined ROM. OS reset vector at $FFFC → $D000.
        let mut rom = vec![0xEAu8; 0x6000];
        rom[0x3FFC] = 0x00;
        rom[0x3FFD] = 0xD0;
        rom[0x3000] = 0x4C;
        rom[0x3001] = 0x00;
        rom[0x3002] = 0xD0;
        rom
    }

    #[test]
    fn frame_advances_count() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        let _ = sys.run_frame();
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn ram_round_trips() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        sys.mem_write(0x0100, 0x42);
        assert_eq!(sys.mem_read(0x0100), 0x42);
    }

    #[test]
    fn video_ram_round_trips_and_mirrors() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        sys.mem_write(0x8000, 0xAB);
        assert_eq!(sys.mem_read(0x8000), 0xAB);
        assert_eq!(sys.mem_read(0x8400), 0xAB);
    }

    #[test]
    fn vdg_control_register_round_trips() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        sys.mem_write(0xB000, 0x80);
        assert_eq!(sys.mem_read(0xB000), 0x80);
    }

    #[test]
    fn rom_writes_ignored() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        sys.mem_write(0xF000, 0xFF);
        assert_eq!(sys.mem_read(0xF000), 0xEA);
    }
}
