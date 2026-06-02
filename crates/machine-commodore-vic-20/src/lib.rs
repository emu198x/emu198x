//! Commodore VIC-20 (1981) — 6502 + VIC 6560/6561 (inline) + character ROM.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-commodore-vic-20/`
//! used the deprecated `emu_core::Bus` callback; the wiring here goes
//! through [`mos_6502::M6502`]'s public pin fields.
//!
//! # The VIC-20
//!
//! Commodore's 1981 home-computer launch — the first computer to sell
//! over a million units, designed to a $300 price point with Robert
//! Yannes' MOS 6560/6561 VIC handling both video AND audio on a single
//! chip. Marketed as VIC-20 in North America and VC-20 in Germany;
//! sold under various names worldwide.
//!
//! - **CPU:** MOS 6502 at 1.108 MHz (PAL) / 1.023 MHz (NTSC)
//! - **VIC 6560/6561:** 22 × 23 character display (176 × 184),
//!   3-tone + noise audio. Inline as
//!   [`vic::Vic6560`].
//! - **RAM:** 5 KB total (1 KB zero page/stack + 4 KB main at `$1000`),
//!   expandable to 32 KB
//! - **ROMs:** 8 KB Kernal at `$E000`, 8 KB BASIC at `$A000`, 4 KB
//!   character ROM at `$8000`
//!
//! Scope of this initial port — VIC chip lives in the dedicated
//! [`mos_vic_i`] chip crate (text-mode video only; audio is stubbed).
//! The donor also stubbed VIA 6522 ×2 wiring (keyboard + joystick);
//! the same stubs land here.

pub mod input;
mod keyboard;

pub use input::Vic20Key;
pub use keyboard::KeyboardState;
pub use mos_vic_i::{FB_HEIGHT, FB_WIDTH, Vic6560};

use mos_6502::M6502;

/// VIC-20 model selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vic20Model {
    Pal,
    Ntsc,
}

/// VIC-20 machine.
pub struct Vic20 {
    cpu: M6502,
    ram_low: [u8; 0x0400],
    ram_exp_low: [u8; 0x0C00],
    ram_main: [u8; 0x1000],
    ram_exp_high: Vec<u8>,
    has_exp_low: bool,
    exp_high_size: usize,
    colour_ram: [u8; 0x0400],
    char_rom: Vec<u8>,
    basic_rom: Vec<u8>,
    kernal_rom: Vec<u8>,
    vic: Vic6560,
    keyboard: KeyboardState,
    // Latched bus state placeholders — kept on the struct so the
    // future tape / userport / IEC wiring lands as a small additive
    // change rather than a structural one.
    #[allow(dead_code)]
    via1_port_a: u8,
    #[allow(dead_code)]
    via2_port_b: u8,
    model: Vic20Model,
    master_clock: u64,
    frame_count: u64,
}

impl Vic20 {
    /// Create a new VIC-20. ROMs: `kernal` 8 KB, `basic` 8 KB, `char_rom` 4 KB.
    /// `ram_expansion_kb` is 0 (unexpanded), 3 (low expansion = full $0400-$0FFF),
    /// or 3+N where N ≤ 24 (high expansion at $2000 onwards).
    pub fn new(
        kernal_rom: Vec<u8>,
        basic_rom: Vec<u8>,
        char_rom: Vec<u8>,
        model: Vic20Model,
        ram_expansion_kb: usize,
    ) -> Self {
        let pal = model == Vic20Model::Pal;
        let has_exp_low = ram_expansion_kb >= 3;
        let exp_high_size = if ram_expansion_kb > 3 {
            (ram_expansion_kb - 3) * 1024
        } else {
            0
        };
        let exp_high_size = exp_high_size.min(0x6000);
        Self {
            cpu: M6502::new(),
            ram_low: [0; 0x0400],
            ram_exp_low: [0; 0x0C00],
            ram_main: [0; 0x1000],
            ram_exp_high: vec![0; exp_high_size],
            has_exp_low,
            exp_high_size,
            colour_ram: [0; 0x0400],
            char_rom,
            basic_rom,
            kernal_rom,
            vic: Vic6560::new(pal),
            keyboard: KeyboardState::new(),
            via1_port_a: 0xFF,
            via2_port_b: 0xFF,
            model,
            master_clock: 0,
            frame_count: 0,
        }
    }

    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        for _ in 0..200_000 {
            self.tick_cycle();
            if self.vic.take_frame_complete() {
                break;
            }
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_cycle(&mut self) {
        self.master_clock += 1;
        // Tick VIC chip with callbacks for screen RAM, colour RAM, char ROM reads.
        let ram_main = &self.ram_main;
        let colour_ram = &self.colour_ram;
        let char_rom = &self.char_rom;
        self.vic.tick(
            |addr| {
                // Screen RAM lives in main RAM at $1000-$1FFF by default —
                // the donor's VIC reads through this mirror.
                ram_main[(addr & 0x0FFF) as usize]
            },
            |addr| colour_ram[(addr & 0x03FF) as usize],
            |addr| char_rom.get((addr & 0x0FFF) as usize).copied().unwrap_or(0xFF),
        );

        self.cpu.tick();
        if self.cpu.rw {
            self.cpu.data_in = self.mem_read(self.cpu.addr);
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x03FF => self.ram_low[addr as usize],
            0x0400..=0x0FFF => {
                if self.has_exp_low {
                    self.ram_exp_low[(addr - 0x0400) as usize]
                } else {
                    0xFF
                }
            }
            0x1000..=0x1FFF => self.ram_main[(addr - 0x1000) as usize],
            0x2000..=0x7FFF => {
                let offset = (addr - 0x2000) as usize;
                if offset < self.exp_high_size {
                    self.ram_exp_high[offset]
                } else {
                    0xFF
                }
            }
            0x8000..=0x8FFF => self
                .char_rom
                .get((addr - 0x8000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0x9000..=0x93FF => self.vic.read((addr & 0x0F) as u8),
            0x9400..=0x97FF => self.colour_ram[(addr - 0x9400) as usize] & 0x0F,
            0x9800..=0x9FFF => 0xFF,
            0xA000..=0xBFFF => self
                .basic_rom
                .get((addr - 0xA000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xC000..=0xDFFF => self
                .kernal_rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xE000..=0xFFFF => self
                .kernal_rom
                .get((addr - 0xE000) as usize)
                .copied()
                .unwrap_or(0xFF),
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x03FF => self.ram_low[addr as usize] = value,
            0x0400..=0x0FFF
                if self.has_exp_low => {
                    self.ram_exp_low[(addr - 0x0400) as usize] = value;
                }
            0x1000..=0x1FFF => self.ram_main[(addr - 0x1000) as usize] = value,
            0x2000..=0x7FFF => {
                let offset = (addr - 0x2000) as usize;
                if offset < self.exp_high_size {
                    self.ram_exp_high[offset] = value;
                }
            }
            0x9000..=0x93FF => self.vic.write((addr & 0x0F) as u8, value),
            0x9400..=0x97FF => {
                self.colour_ram[(addr - 0x9400) as usize] = value & 0x0F;
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vic.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vic.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vic.framebuffer_height()
    }

    pub fn press_key(&mut self, key: Vic20Key) {
        let (row, col) = key.matrix();
        self.keyboard.set_key(row, col, true);
    }

    pub fn release_key(&mut self, key: Vic20Key) {
        let (row, col) = key.matrix();
        self.keyboard.set_key(row, col, false);
    }

    pub fn release_all_keys(&mut self) {
        self.keyboard.release_all();
    }

    #[must_use]
    pub fn peek_memory(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    #[must_use]
    pub fn model(&self) -> Vic20Model {
        self.model
    }

    #[must_use]
    pub fn vic(&self) -> &Vic6560 {
        &self.vic
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

    fn make_vic20() -> Vic20 {
        let mut kernal = vec![0xEAu8; 0x2000];
        // Reset vector at $FFFC/$FFFD → $E000.
        kernal[0x1FFC] = 0x00;
        kernal[0x1FFD] = 0xE0;
        Vic20::new(
            kernal,
            vec![0u8; 0x2000],
            vec![0u8; 0x1000],
            Vic20Model::Pal,
            0,
        )
    }

    #[test]
    fn ram_round_trips() {
        let mut sys = make_vic20();
        sys.mem_write(0x0000, 0x42);
        assert_eq!(sys.mem_read(0x0000), 0x42);
    }

    #[test]
    fn main_ram_round_trips() {
        let mut sys = make_vic20();
        sys.mem_write(0x1000, 0xAB);
        assert_eq!(sys.mem_read(0x1000), 0xAB);
    }

    #[test]
    fn colour_ram_masks_to_nibble() {
        let mut sys = make_vic20();
        sys.mem_write(0x9400, 0xFF);
        assert_eq!(sys.mem_read(0x9400), 0x0F);
    }

    #[test]
    fn rom_writes_ignored() {
        let mut sys = make_vic20();
        sys.mem_write(0xE000, 0xFF);
        assert_eq!(sys.mem_read(0xE000), 0xEA);
    }

    #[test]
    fn frame_advances_count() {
        let mut sys = make_vic20();
        let _ = sys.run_frame();
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn ntsc_runs() {
        let mut kernal = vec![0xEAu8; 0x2000];
        kernal[0x1FFC] = 0x00;
        kernal[0x1FFD] = 0xE0;
        let mut sys = Vic20::new(
            kernal,
            vec![0u8; 0x2000],
            vec![0u8; 0x1000],
            Vic20Model::Ntsc,
            0,
        );
        let _ = sys.run_frame();
        assert_eq!(sys.frame_count(), 1);
        assert_eq!(sys.model(), Vic20Model::Ntsc);
    }

    #[test]
    fn expansion_ram_low() {
        let mut sys = Vic20::new(
            vec![0xEA; 0x2000],
            vec![0u8; 0x2000],
            vec![0u8; 0x1000],
            Vic20Model::Pal,
            3,
        );
        sys.mem_write(0x0400, 0x55);
        assert_eq!(sys.mem_read(0x0400), 0x55);
    }
}
