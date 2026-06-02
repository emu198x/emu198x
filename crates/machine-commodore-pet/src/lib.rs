//! Commodore PET — 6502 + 6845 CRTC + 6520 PIA + 6522 VIA.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-commodore-pet/`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as the system spec but wires the 6502
//! through its public pin fields (`addr`, `data`, `data_in`, `rw`).
//!
//! # The Commodore PET
//!
//! Released by Commodore in 1977. One of the original "1977 trinity"
//! alongside the Apple II and the TRS-80 Model I. The 8032 (modelled
//! here) is the 1980 80-column business variant — same chipset, wider
//! display. Famous chiclet keyboard on the earlier 2001; the 8032
//! moved to a full-travel layout.
//!
//! - **CPU:** MOS 6502 at 1 MHz.
//! - **CRTC:** Motorola 6845 generating character-display timing.
//! - **PIA 6520** at `$E810` — keyboard column-select on port A,
//!   row-data on port B.
//! - **VIA 6522** at `$E840` — cassette + IEEE-488 + piezo speaker
//!   on CB2.
//! - **RAM:** 32 KB at `$0000-$7FFF`.
//! - **Video RAM:** 2 KB at `$8000-$87FF`.
//! - **ROMs:** BASIC (8 KB at `$C000-$DFFF`), Editor (2 KB at
//!   `$E000-$E7FF`), Kernal (4 KB at `$F000-$FFFF`), Character ROM
//!   (4 KB, display-only).
//!
//! Clock model: one master tick per 6502 cycle (1 MHz). CRTC + VIA
//! tick on the same cadence. Per the donor's v1 simplification, the
//! CRTC ticks at CPU rate even in 80-column mode where the real
//! hardware would clock it at 2 MHz; mid-frame timing accuracy is on
//! the accuracy backlog.

pub mod input;
mod keyboard;

pub use input::PetKey;
pub use keyboard::KeyboardState;

use mos_6502::M6502;
use mos_pia_6520::Pia6520;
use mos_via_6522::Via6522;
use motorola_6845::Crtc6845;

pub const ACTIVE_WIDTH_40: u32 = 320;
pub const ACTIVE_WIDTH_80: u32 = 640;
pub const ACTIVE_HEIGHT: u32 = 200;

/// Border thickness around the active text display. The PET's
/// monochrome P1 phosphor display always shows black around the
/// green-on-black active region — no programmable border colour.
pub const BORDER_LEFT: u32 = 32;
pub const BORDER_RIGHT: u32 = 32;
pub const BORDER_TOP: u32 = 24;
pub const BORDER_BOTTOM: u32 = 24;

pub const SCREEN_WIDTH_40: u32 = ACTIVE_WIDTH_40 + BORDER_LEFT + BORDER_RIGHT;
pub const SCREEN_WIDTH_80: u32 = ACTIVE_WIDTH_80 + BORDER_LEFT + BORDER_RIGHT;
pub const SCREEN_HEIGHT: u32 = ACTIVE_HEIGHT + BORDER_TOP + BORDER_BOTTOM;

/// Commodore PET machine.
pub struct Pet {
    cpu: M6502,
    ram: [u8; 0x8000],
    video_ram: [u8; 0x0800],
    basic_rom: Vec<u8>,
    editor_rom: Vec<u8>,
    kernal_rom: Vec<u8>,
    char_rom: Vec<u8>,
    crtc: Crtc6845,
    pia: Pia6520,
    via: Via6522,
    keyboard: KeyboardState,
    framebuffer: Vec<u32>,
    screen_chars: u32,
    screen_width_px: u32,
    frame_complete: bool,
    master_clock: u64,
    frame_count: u64,
}

impl Pet {
    /// Create a new PET. ROM sizes: kernal 4 KB, basic 8 KB, editor 2 KB,
    /// char ROM 4 KB. `screen_chars` is 40 (PET 4032 / 8032) or 80
    /// (PET 8032).
    pub fn new(
        kernal_rom: Vec<u8>,
        basic_rom: Vec<u8>,
        editor_rom: Vec<u8>,
        char_rom: Vec<u8>,
        screen_chars: u32,
    ) -> Self {
        let screen_width_px = if screen_chars >= 80 {
            SCREEN_WIDTH_80
        } else {
            SCREEN_WIDTH_40
        };
        let mut crtc = Crtc6845::new();
        // Standard PET CRTC register setups, taken from donor.
        let regs: [u8; 14] = if screen_chars >= 80 {
            [99, 80, 82, 8, 31, 4, 25, 29, 0, 9, 0, 0, 0x10, 0x00]
        } else {
            [49, 40, 41, 4, 31, 4, 25, 29, 0, 7, 0, 0, 0x10, 0x00]
        };
        for (i, &v) in regs.iter().enumerate() {
            crtc.write_address(i as u8);
            crtc.write_data(v);
        }

        Self {
            cpu: M6502::new(),
            ram: [0; 0x8000],
            video_ram: [0; 0x0800],
            basic_rom,
            editor_rom,
            kernal_rom,
            char_rom,
            crtc,
            pia: Pia6520::new(),
            via: Via6522::new(),
            keyboard: KeyboardState::new(),
            framebuffer: vec![0xFF00_0000; (screen_width_px * SCREEN_HEIGHT) as usize],
            screen_chars,
            screen_width_px,
            frame_complete: false,
            master_clock: 0,
            frame_count: 0,
        }
    }

    /// Run one full frame and return the number of CPU cycles executed.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        // ~20,000 cycles per 50 Hz frame at 1 MHz; cap defensively at 30,000
        // to avoid an infinite loop if the CRTC never raises frame-complete.
        for _ in 0..30_000 {
            self.tick();
            if self.take_frame_complete() {
                break;
            }
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick(&mut self) {
        self.master_clock += 1;
        self.tick_display();
        self.via.tick();
        self.cpu.irq = self.pia.irq_pending() || self.via.irq;
        self.cpu.tick();
        if self.cpu.rw {
            self.cpu.data_in = self.mem_read(self.cpu.addr);
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
    }

    fn tick_display(&mut self) {
        let new_frame = self.crtc.tick();
        if new_frame {
            self.frame_complete = true;
        }
        if !self.crtc.display_enable {
            return;
        }
        let ma = self.crtc.memory_address();
        let ra = self.crtc.raster_address();
        let char_code = self.video_ram[(ma & 0x07FF) as usize];
        let char_rom_addr = (u16::from(char_code) * 16 + u16::from(ra)) as usize;
        let char_data = self.char_rom.get(char_rom_addr).copied().unwrap_or(0);
        let on_cursor = self.crtc.cursor_active;
        let chars_per_row = self.screen_chars;
        let char_col = ma % chars_per_row as u16;
        let char_row = ma / chars_per_row as u16;
        let active_y =
            u32::from(char_row) * (u32::from(self.crtc.max_scanline()) + 1) + u32::from(ra);
        let active_x_base = u32::from(char_col) * 8;
        if active_y >= ACTIVE_HEIGHT {
            return;
        }
        let fb_y = BORDER_TOP + active_y;
        let fb_x_base = BORDER_LEFT + active_x_base;
        for px in 0..8u32 {
            let fb_x = fb_x_base + px;
            if fb_x >= self.screen_width_px {
                break;
            }
            let bit = (char_data >> (7 - px)) & 1;
            let fg = if on_cursor { bit == 0 } else { bit != 0 };
            let colour = if fg { 0xFF00_FF00 } else { 0xFF00_0000 };
            let idx = (fb_y * self.screen_width_px + fb_x) as usize;
            if idx < self.framebuffer.len() {
                self.framebuffer[idx] = colour;
            }
        }
    }

    fn take_frame_complete(&mut self) -> bool {
        let v = self.frame_complete;
        self.frame_complete = false;
        v
    }

    fn update_keyboard(&mut self) {
        let col_select = self.pia.port_a_output();
        let row_data = self.keyboard.read(col_select);
        self.pia.set_port_b_input(row_data);
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0x87FF => self.video_ram[(addr - 0x8000) as usize],
            0x8800..=0x8FFF => 0xFF,
            0x9000..=0xBFFF => 0xFF,
            0xC000..=0xDFFF => {
                let offset = (addr - 0xC000) as usize;
                self.basic_rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xE000..=0xE7FF => {
                let offset = (addr - 0xE000) as usize;
                self.editor_rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xE810..=0xE81F => {
                self.update_keyboard();
                self.pia.read((addr & 0x03) as u8)
            }
            0xE840..=0xE84F => self.via.read((addr & 0x0F) as u8),
            0xE880 => self.crtc.read_data(),
            0xE800..=0xEFFF => 0xFF,
            0xF000..=0xFFFF => {
                let offset = (addr - 0xF000) as usize;
                self.kernal_rom.get(offset).copied().unwrap_or(0xFF)
            }
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize] = value,
            0x8000..=0x87FF => self.video_ram[(addr - 0x8000) as usize] = value,
            0xE810..=0xE81F => self.pia.write((addr & 0x03) as u8, value),
            0xE840..=0xE84F => self.via.write((addr & 0x0F) as u8, value),
            0xE880 => self.crtc.write_address(value),
            0xE881 => self.crtc.write_data(value),
            _ => {}
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.screen_width_px
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        SCREEN_HEIGHT
    }

    pub fn press_key(&mut self, key: PetKey) {
        let (row, col) = key.matrix();
        self.keyboard.set_key(row, col, true);
    }

    pub fn release_key(&mut self, key: PetKey) {
        let (row, col) = key.matrix();
        self.keyboard.set_key(row, col, false);
    }

    pub fn release_all_keys(&mut self) {
        self.keyboard.release_all();
    }

    #[must_use]
    pub fn peek_memory(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0x87FF => self.video_ram[(addr - 0x8000) as usize],
            0xC000..=0xDFFF => self
                .basic_rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xE000..=0xE7FF => self
                .editor_rom
                .get((addr - 0xE000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xF000..=0xFFFF => self
                .kernal_rom
                .get((addr - 0xF000) as usize)
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

    fn make_pet() -> Pet {
        let mut kernal = vec![0u8; 0x1000];
        // Reset vector at $FFFC/$FFFD = $F000 — execute NOPs forever.
        kernal[0x0FFC] = 0x00;
        kernal[0x0FFD] = 0xF0;
        for byte in kernal.iter_mut().take(0x0FFC) {
            *byte = 0xEA;
        }
        Pet::new(
            kernal,
            vec![0u8; 0x2000],
            vec![0u8; 0x0800],
            vec![0u8; 0x1000],
            40,
        )
    }

    #[test]
    fn frame_advances_frame_count() {
        let mut pet = make_pet();
        let _ = pet.run_frame();
        assert_eq!(pet.frame_count(), 1);
    }

    #[test]
    fn framebuffer_correct_size_40_col() {
        let pet = make_pet();
        assert_eq!(pet.framebuffer_width(), SCREEN_WIDTH_40);
        assert_eq!(pet.framebuffer_height(), SCREEN_HEIGHT);
        assert_eq!(
            pet.framebuffer().len(),
            (SCREEN_WIDTH_40 * SCREEN_HEIGHT) as usize
        );
    }

    #[test]
    fn framebuffer_correct_size_80_col() {
        let pet = Pet::new(
            vec![0u8; 0x1000],
            vec![0u8; 0x2000],
            vec![0u8; 0x0800],
            vec![0u8; 0x1000],
            80,
        );
        assert_eq!(pet.framebuffer_width(), SCREEN_WIDTH_80);
    }

    #[test]
    fn ram_round_trips() {
        let mut pet = make_pet();
        pet.mem_write(0x0100, 0x55);
        assert_eq!(pet.mem_read(0x0100), 0x55);
    }

    #[test]
    fn video_ram_round_trips() {
        let mut pet = make_pet();
        pet.mem_write(0x8000, 0xAA);
        assert_eq!(pet.mem_read(0x8000), 0xAA);
    }

    #[test]
    fn rom_writes_ignored() {
        let mut pet = make_pet();
        let before = pet.mem_read(0xF000);
        pet.mem_write(0xF000, 0xFF);
        assert_eq!(pet.mem_read(0xF000), before);
    }
}
