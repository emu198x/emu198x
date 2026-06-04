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
//! - **PPI:** Intel INS8255 at `$B000-$B003` (keyboard + cassette)
//! - **RAM:** 2.5 KB base, expandable to 12 KB
//! - **Video RAM:** 1 KB at `$8000-$83FF` (mirrored to `$9FFF`)
//! - **ROM:** 24 KB combined — BASIC (split `$A000` + `$C000`),
//!   FP at `$B004-$BFFF`, OS at `$D000-$FFFF`
//!
//! # I/O — the INS8255 PPI at `$B000-$B003`
//!
//! Per the Atom Technical Manual (Issue 2), the 8255 drives the keyboard
//! through a 4-to-10 line decoder and reads the columns back:
//!
//! - **Port A** (`$B000`): low nibble = the binary keyboard row index
//!   (0-9) into the decoder; high nibble = the MC6847 mode bits.
//! - **Port B** (`$B001`): the six keyboard column lines, active low.
//! - **Port C** (`$B002`): bits 0-3 output (cassette / speaker / colour
//!   set); bits 4-7 input — PC4 = 2.4 kHz cassette tone, PC7 = the VDG
//!   vertical-blanking (field-sync) the MOS times its keyboard scan off.
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
pub use vdg::{FB_HEIGHT, FB_WIDTH, Mc6847};

use intel_8255::Ppi8255;
use mos_6502::M6502;

/// Acorn Atom machine.
pub struct AcornAtom {
    cpu: M6502,
    ram: Vec<u8>,
    ram_size: usize,
    video_ram: [u8; 1024],
    rom: Vec<u8>,
    /// Intel 8255 PPI: port A drives the keyboard column (PA0-3) and the
    /// MC6847 mode bits (PA4-7); port B reads the six keyboard row lines;
    /// port C handles cassette / speaker / 2.4 kHz.
    ppi: Ppi8255,
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
        // Run the 6502 reset sequence so the first fetch comes from the MOS
        // reset vector ($FFFC); without it the CPU powers on at PC=$0000 and
        // never cold-starts, leaving the uninitialised character grid on screen.
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
            ram: vec![0; ram_size],
            ram_size,
            video_ram: [0; 1024],
            rom,
            ppi: Ppi8255::new(),
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

    /// Present the keyboard rows for the column the MOS has driven on 8255
    /// port A (PA0-3, a binary 0-9 column index) on port B, active low.
    fn update_keyboard(&mut self) {
        let column = (self.ppi.port_a & 0x0F) as usize;
        self.ppi.port_b = !self.keyboard.read_row(column);
    }

    fn tick(&mut self) {
        self.master_clock += 1;
        let video_ram = &self.video_ram;
        self.vdg.tick(|addr| video_ram[(addr & 0x03FF) as usize]);
        // The Atom keyboard is polled, not interrupt-driven; the 8255 has no
        // interrupt line in Mode 0 and the donor models no other IRQ source.
        self.cpu.irq = false;
        self.cpu.tick();
        if self.cpu.rw {
            self.cpu.data_in = self.mem_read(self.cpu.addr);
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
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
            0xB000..=0xB003 => {
                self.update_keyboard();
                // Port C inputs: PC4 = 2.4 kHz cassette tone, PC7 = the 6847
                // field-sync (~50 Hz). The field sync is a brief vertical-
                // blanking pulse once per ~20 ms field, not a square wave, so
                // the MOS scans the matrix once per field rather than twice.
                let field_sync = (self.master_clock % 20_000) < 1_000;
                let tone = (self.master_clock % 416) < 208;
                let inputs = (u8::from(field_sync) << 7) | (u8::from(tone) << 4);
                self.ppi.port_c = (self.ppi.port_c & 0x0F) | inputs;
                self.ppi.read((addr - 0xB000) as u8)
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
            0x0000..=0x7FFF if (addr as usize) < self.ram_size => {
                self.ram[addr as usize] = value;
            }
            0x8000..=0x9FFF => {
                self.video_ram[(addr & 0x03FF) as usize] = value;
            }
            0xB000 => {
                // Port A: low nibble selects the keyboard column, high nibble
                // carries the MC6847 mode bits — latch both.
                self.ppi.write(0, value);
                self.vdg.control = value;
            }
            0xB001..=0xB003 => self.ppi.write((addr - 0xB000) as u8, value),
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
            0x0000..=0x7FFF if (addr as usize) < self.ram_size => self.ram[addr as usize],
            0x8000..=0x9FFF => self.video_ram[(addr & 0x03FF) as usize],
            0xB000 => self.vdg.control,
            0xA000..=0xAFFF => self
                .rom
                .get((addr - 0xA000) as usize)
                .copied()
                .unwrap_or(0xFF),
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

impl AcornAtom {
    /// Read one byte with no side effects (alias of `peek_memory`).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.peek_memory(addr)
    }

    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Run exactly one whole 6502 instruction, returning the clocks it
    /// consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let mut ticks = 0u64;
        while self.cpu.instruction_complete() && ticks < 4096 {
            self.tick();
            ticks += 1;
        }
        while !self.cpu.instruction_complete() && ticks < 4096 {
            self.tick();
            ticks += 1;
        }
        ticks
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
