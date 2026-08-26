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

use emu198x_mos_6502::M6502;
use mos_pia_6520::Pia6520;
use mos_via_6522::Via6522;
use motorola_6845::Crtc6845;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

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
#[derive(Serialize, Deserialize)]
pub struct Pet {
    cpu: M6502,
    #[serde(with = "BigArray")]
    ram: [u8; 0x8000],
    #[serde(with = "BigArray")]
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
        // PET CRTC register setup. This editor ROM does not reprogram the CRTC
        // at boot (it powers up with these values), so they must give the real
        // PET frame timing. The 40-column PAL frame is 64 cycles/line × 313
        // lines = 20,032 cycles → 49.92 Hz at 1 MHz, per VICE (`src/pet/pet.h`
        // PET_PAL_CYCLES_PER_LINE=64, PET_PAL_SCREEN_LINES=313); the CRTC line
        // is R0+1 cycles, and 313 scanlines = (R4+1)×(R9+1) + R5 = 39×8 + 1.
        // The donor values (R0=49, R4=31 → 50×260 = 13,000 cycles ≈ 77 Hz) ran
        // the machine ~1.3× too fast. R6/R1 (25 rows × 40 cols displayed) and R9
        // (8 scanlines/char) are unchanged, so the visible screen is identical;
        // R2/R7 sync positions sit inside the wider totals.
        let regs: [u8; 14] = if screen_chars >= 80 {
            [99, 80, 82, 8, 31, 4, 25, 29, 0, 9, 0, 0, 0x10, 0x00]
        } else {
            [63, 40, 48, 4, 38, 1, 25, 34, 0, 7, 0, 0, 0x10, 0x00]
        };
        for (i, &v) in regs.iter().enumerate() {
            crtc.write_address(i as u8);
            crtc.write_data(v);
        }

        // Run the 6502 reset sequence so the first fetch comes from the KERNAL
        // reset vector ($FFFC); without it the CPU powers on at PC=$0000,
        // executes the BRK there, and never cold-starts (the screen sticks on
        // the uninitialised "@" grid).
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
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
        // PIA #1 CA1 is wired to the CRTC vertical retrace: its edge is the
        // 60 Hz system IRQ that runs the editor's keyboard scan and jiffy
        // clock. Without it the machine boots to READY but never sees a key.
        // PIA #1 CB1 is wired to the CRTC vertical retrace: its edge is the
        // 60 Hz system IRQ that runs the editor's keyboard scan and jiffy
        // clock (the editor enables CB1, not CA1 — CA1 is cassette sense).
        // Without it the machine boots to READY but never sees a key.
        self.pia.set_cb1(self.crtc.in_vertical_retrace());
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
        let ra = self.crtc.raster_address();
        // The CRTC address carries the display start (R12/R13, $1000 here). The
        // screen cell and the video-RAM byte must come from the *same* relative
        // address, so mask both into the 2 KB video RAM — otherwise the cell
        // position lands off-screen while an unwritten cell is fetched.
        let disp_addr = self.crtc.memory_address() & 0x07FF;
        let char_code = self.video_ram[disp_addr as usize];
        // The PET character ROM stores 8 bytes per glyph (one byte per
        // scanline of the 8×8 cell), so the glyph base is `code * 8`, not
        // `* 16`. Using 16 doubled the stride: every glyph read its
        // neighbour's data and "spaces" fetched a non-blank glyph (the
        // screen filled with horizontal-line noise).
        let char_rom_addr = (u16::from(char_code) * 8 + u16::from(ra)) as usize;
        let char_data = self.char_rom.get(char_rom_addr).copied().unwrap_or(0);
        let on_cursor = self.crtc.cursor_active;
        let chars_per_row = self.screen_chars;
        let char_col = disp_addr % chars_per_row as u16;
        let char_row = disp_addr / chars_per_row as u16;
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
        // PIA #1 port A drives the binary row number (0-9); port B reads
        // that row's column lines.
        let row = self.pia.port_a_output() & 0x0F;
        let columns = self.keyboard.read_row(row);
        self.pia.set_port_b_input(columns);
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
            // VIA port B ($E840) carries the CRTC vertical-retrace state on
            // PB5 (0 = off-screen). The editor spin-waits on it before writing
            // the screen to avoid snow, so it must toggle each frame; the other
            // input bits (IEEE handshake) idle high with no device attached.
            0xE840 => {
                // PB5 low signals vertical retrace; every other input bit
                // idles high with no IEEE device attached.
                let pb = if self.crtc.in_vertical_retrace() {
                    !0x20u8
                } else {
                    0xFF
                };
                self.via.read_port_b_with_value(pb)
            }
            0xE841..=0xE84F => self.via.read((addr & 0x0F) as u8),
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

impl Pet {
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

    #[test]
    fn snapshot_round_trips_live_state() {
        let mut pet = make_pet();
        pet.run_frame();
        // PET RAM is $0000-$7FFF; poke a low address and confirm via the read
        // path that it took.
        pet.poke(0x0400, 0x5A);
        assert_eq!(pet.mem_read(0x0400), 0x5A);

        pet.run_frame();
        let first = postcard::to_allocvec(&pet).expect("encode first");
        pet.run_frame();
        let second = postcard::to_allocvec(&pet).expect("encode second");
        assert_ne!(
            first, second,
            "advancing a frame must change the serialised state"
        );

        let restored: Pet = postcard::from_bytes(&first).expect("decode first");
        let reserialised = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            first, reserialised,
            "restoring then re-serialising must be byte-identical"
        );
    }

    #[test]
    fn frame_is_pal_50hz_at_1mhz() {
        // The 40-column PET frame must be 64 cycles/line × 313 lines = 20,032
        // CPU cycles → 49.92 Hz at 1 MHz, per VICE (`src/pet/pet.h`). The donor
        // CRTC values gave 50 × 260 = 13,000 (≈77 Hz), running ~1.3× too fast.
        // The CRTC drives the frame independently of the CPU program, so this
        // holds with the synthetic test ROMs.
        let mut pet = make_pet();
        pet.run_frame(); // sync to a frame boundary
        let frame = pet.run_frame();
        assert_eq!(
            frame, 20_032,
            "PET PAL frame must be 20,032 cycles (64 × 313); got {frame}"
        );
    }
}
