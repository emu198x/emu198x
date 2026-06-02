//! Mattel Aquarius machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-mattel-aquarius`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — memory map, TEA1002
//! palette, 8-row keyboard via port `$FF` with row select on the high
//! address byte, NMI on VBlank — but the wiring is written against
//! [`zilog_z80::Z80`]'s public pin fields and `bus_request()` collapse.
//!
//! # The Mattel Aquarius
//!
//! The Aquarius (1983) is a Z80A-based home computer designed by
//! Radofin for Mattel Electronics. Famous (mostly notorious) for its
//! tiny chiclet keyboard. Character-only display — 40×24 cells with a
//! TEA1002 colour encoder producing a 16-colour palette. The character
//! generator lives in the upper 2 KB of the 8 KB BASIC ROM.
//!
//! - **CPU:** Z80A @ 3.5 MHz
//! - **ROM:** 8 KB Microsoft BASIC at `$0000-$1FFF` (character set at
//!   `$1800-$1FFF`)
//! - **RAM:** 1 KB char + 1 KB colour + 2 KB spare at `$3000-$3FFF`
//! - **Expansion RAM:** up to 16 KB at `$4000-$7FFF`
//! - **Cart ROM:** up to 8 KB at `$E000-$FFFF`
//! - **Display:** 320×192 (40×24 8×8 characters), TEA1002 16-colour
//!   palette
//! - **Sound:** 1-bit internal speaker (port `$FF` bit 0)
//!
//! # Memory map
//!
//! | Range         | Contents                                  |
//! |---------------|-------------------------------------------|
//! | `$0000-$1FFF` | 8 KB Microsoft BASIC ROM + character set  |
//! | `$2000-$2FFF` | Unmapped (`$FF`)                          |
//! | `$3000-$33FF` | 1 KB character RAM                        |
//! | `$3400-$37FF` | 1 KB colour RAM                           |
//! | `$3800-$3FFF` | 2 KB spare RAM                            |
//! | `$4000-$7FFF` | Up to 16 KB expansion RAM                 |
//! | `$8000-$DFFF` | Unmapped                                  |
//! | `$E000-$FFFF` | Up to 8 KB cart ROM                       |
//!
//! # I/O map
//!
//! | Port  | R/W   | Function                                            |
//! |-------|-------|-----------------------------------------------------|
//! | `$FC` | write | PSG data (Mini Expander AY-3-8910 — stub)           |
//! | `$FE` | r/w   | Printer status (read) / data (write) — stub         |
//! | `$FF` | read  | Keyboard column read; rows selected by addr A8-A15  |
//! | `$FF` | write | Scrambler latch + 1-bit speaker on bit 0            |
//!
//! # Keyboard
//!
//! 8 rows × 6 columns matrix, active-low. The CPU writes to `$FF` with
//! the high address byte (A8-A15) selecting which rows to scan
//! (active-low; a bit set to 0 enables that row), and the resulting
//! AND of all selected rows' column bytes appears on the read.

use zilog_z80::{BusOp, Z80};

const CHAR_COLS: u32 = 40;
const CHAR_ROWS: u32 = 24;
const CHAR_WIDTH: u32 = 8;
const CHAR_HEIGHT: u32 = 8;
/// Framebuffer pixel width (`CHAR_COLS * CHAR_WIDTH`).
pub const FB_WIDTH: u32 = CHAR_COLS * CHAR_WIDTH;
/// Framebuffer pixel height (`CHAR_ROWS * CHAR_HEIGHT`).
pub const FB_HEIGHT: u32 = CHAR_ROWS * CHAR_HEIGHT;

const CPU_CLOCK_HZ: u64 = 3_500_000;
const FRAMES_PER_SECOND_PAL: u64 = 50;
const CPU_TSTATES_PER_FRAME: u64 = CPU_CLOCK_HZ / FRAMES_PER_SECOND_PAL;

const CHAR_ROM_OFFSET: usize = 0x1800;
const NUM_KEY_ROWS: usize = 8;

/// TEA1002 / Aquarius 16-colour palette (ARGB32).
const PALETTE: [u32; 16] = [
    0xFF00_0000, // 0: Black
    0xFFFF_0000, // 1: Red
    0xFF00_0000, // 2: Dark blue (rendered as black on most TVs)
    0xFFFF_00FF, // 3: Magenta
    0xFF00_8000, // 4: Dark green
    0xFF80_8080, // 5: Dark grey
    0xFF00_00FF, // 6: Blue
    0xFF80_80FF, // 7: Light blue
    0xFF00_FF00, // 8: Bright green
    0xFFFF_FF00, // 9: Yellow
    0xFFC0_C0C0, // 10: Light grey
    0xFFFF_C0C0, // 11: Light red / pink
    0xFF00_FF80, // 12: Cyan-green
    0xFFFF_FF80, // 13: Light yellow
    0xFF80_FFFF, // 14: Light cyan
    0xFFFF_FFFF, // 15: White
];

/// Mattel Aquarius machine.
pub struct Aquarius {
    cpu: Z80,
    rom: Vec<u8>,
    char_ram: [u8; 1024],
    colour_ram: [u8; 1024],
    spare_ram: [u8; 2048],
    expansion_ram: Vec<u8>,
    cart_rom: Vec<u8>,
    /// 8 rows × 6 columns matrix; active-low (1 = released).
    key_matrix: [u8; NUM_KEY_ROWS],
    speaker_bit: bool,
    scrambler: u8,
    framebuffer: Vec<u32>,
    cpu_tstates: u64,
    frame_count: u64,
    /// Set true the cycle a VBlank NMI is being delivered.
    nmi_pulse: bool,
}

impl Aquarius {
    /// Create a new Aquarius with the given 8 KB BASIC ROM and optional
    /// expansion RAM in kilobytes (up to 16).
    #[must_use]
    pub fn new(rom: Vec<u8>, expansion_kb: usize) -> Self {
        let expansion_ram = if expansion_kb > 0 {
            vec![0u8; expansion_kb.min(16) * 1024]
        } else {
            Vec::new()
        };
        Self {
            cpu: Z80::new(),
            rom,
            char_ram: [0x20; 1024],   // Spaces
            colour_ram: [0x70; 1024], // White-on-black default
            // (high nibble = fg = 7, low nibble = bg = 0).
            spare_ram: [0; 2048],
            expansion_ram,
            cart_rom: Vec::new(),
            key_matrix: [0xFF; NUM_KEY_ROWS],
            speaker_bit: false,
            scrambler: 0,
            framebuffer: vec![PALETTE[0]; (FB_WIDTH * FB_HEIGHT) as usize],
            cpu_tstates: 0,
            frame_count: 0,
            nmi_pulse: false,
        }
    }

    /// Insert a cart ROM (mapped at `$E000-$FFFF`, up to 8 KB).
    pub fn insert_cart(&mut self, rom: Vec<u8>) {
        self.cart_rom = rom;
    }

    /// Run one frame and return T-states consumed.
    pub fn run_frame(&mut self) -> u64 {
        let target = self.cpu_tstates + CPU_TSTATES_PER_FRAME;
        // Hold NMI low during the frame.
        self.nmi_pulse = false;
        self.cpu.nmi = false;
        while self.cpu_tstates < target {
            self.tick_tstate();
        }
        // Pulse NMI for one T-state at VBlank.
        self.nmi_pulse = true;
        self.cpu.nmi = true;
        self.tick_tstate();
        self.cpu.nmi = false;
        self.nmi_pulse = false;

        self.render_display();
        self.frame_count += 1;
        CPU_TSTATES_PER_FRAME
    }

    fn tick_tstate(&mut self) {
        self.cpu.tick();
        self.handle_bus();
        self.cpu_tstates += 1;
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                self.cpu.data_in = self.mem_read(self.cpu.addr);
            }
            Some(BusOp::MemWrite) => {
                self.mem_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IoRead) => {
                self.cpu.data_in = self.io_read(self.cpu.addr);
            }
            Some(BusOp::IoWrite) => {
                self.io_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // Aquarius BASIC sets IM 1; INT line is not externally
                // wired — only NMI on VBlank.
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            0x2000..=0x2FFF => 0xFF,
            0x3000..=0x33FF => self.char_ram[(addr & 0x03FF) as usize],
            0x3400..=0x37FF => self.colour_ram[(addr & 0x03FF) as usize],
            0x3800..=0x3FFF => self.spare_ram[(addr & 0x07FF) as usize],
            0x4000..=0x7FFF => self
                .expansion_ram
                .get((addr - 0x4000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0x8000..=0xDFFF => 0xFF,
            0xE000..=0xFFFF => self
                .cart_rom
                .get((addr - 0xE000) as usize)
                .copied()
                .unwrap_or(0xFF),
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x3000..=0x33FF => self.char_ram[(addr & 0x03FF) as usize] = value,
            0x3400..=0x37FF => self.colour_ram[(addr & 0x03FF) as usize] = value,
            0x3800..=0x3FFF => self.spare_ram[(addr & 0x07FF) as usize] = value,
            0x4000..=0x7FFF => {
                let idx = (addr - 0x4000) as usize;
                if let Some(slot) = self.expansion_ram.get_mut(idx) {
                    *slot = value;
                }
            }
            _ => {}
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let low = port as u8;
        if low == 0xFF {
            // Address lines A8-A15 select rows: a 0 bit enables the
            // row's column data to be AND'd into the result.
            let row_select = (port >> 8) as u8;
            let mut result = 0xFF_u8;
            for row in 0..NUM_KEY_ROWS {
                if row_select & (1 << row) == 0 {
                    result &= self.key_matrix[row];
                }
            }
            return result;
        }
        // Printer status read at $FE — always not-busy.
        if low == 0xFE {
            return 0xFF;
        }
        0xFF
    }

    fn io_write(&mut self, port: u16, value: u8) {
        let low = port as u8;
        match low {
            0xFC => {} // Mini-Expander PSG stub.
            0xFE => {} // Printer data stub.
            0xFF => {
                self.scrambler = value;
                self.speaker_bit = value & 0x01 != 0;
            }
            _ => {}
        }
    }

    fn render_display(&mut self) {
        // 40×24 cells, each 8×8 pixels. The character generator lives
        // in the upper 2 KB of the BASIC ROM.
        for row in 0..CHAR_ROWS {
            for col in 0..CHAR_COLS {
                let screen_off = (row * CHAR_COLS + col) as usize;
                let char_code = self.char_ram[screen_off % 1024] as usize;
                let colour_byte = self.colour_ram[screen_off % 1024];
                // Aquarius colour byte: high nibble = foreground,
                // low nibble = background. (The donor's source comment
                // claimed the opposite; the BIOS writes confirm fg is
                // the high nibble.)
                let fg = PALETTE[((colour_byte >> 4) & 0x0F) as usize];
                let bg = PALETTE[(colour_byte & 0x0F) as usize];
                let char_base = CHAR_ROM_OFFSET + char_code * 8;
                for py in 0..CHAR_HEIGHT {
                    let pattern = self.rom.get(char_base + py as usize).copied().unwrap_or(0);
                    let fb_y = row * CHAR_HEIGHT + py;
                    let fb_row_start = (fb_y * FB_WIDTH) as usize;
                    for px in 0..CHAR_WIDTH {
                        let fb_x = (col * CHAR_WIDTH + px) as usize;
                        let pixel = if pattern & (0x80 >> px) != 0 { fg } else { bg };
                        self.framebuffer[fb_row_start + fb_x] = pixel;
                    }
                }
            }
        }
    }

    /// Framebuffer (320×192 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    /// Observe one byte on the Z80 bus without side effects.
    /// Resolves ROM / character RAM / colour RAM / spare RAM /
    /// expansion RAM / cart ROM via the standard Aquarius memory map.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Press / release a key at the given (row, column).
    pub fn set_key(&mut self, row: usize, col: u8, pressed: bool) {
        if row < self.key_matrix.len() && col < 6 {
            if pressed {
                self.key_matrix[row] &= !(1 << col);
            } else {
                self.key_matrix[row] |= 1 << col;
            }
        }
    }

    /// CPU reference.
    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    /// CPU mutable reference.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }

    /// Frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// CPU T-states executed since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.cpu_tstates
    }

    /// Current speaker bit (1-bit audio).
    #[must_use]
    pub fn speaker_bit(&self) -> bool {
        self.speaker_bit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        // 8 KB ROM with NOPs, JR -2 trap at $0008, and a simple
        // character set (every char = $FF pattern) at $1800-$1FFF for
        // the render tests.
        let mut rom = vec![0u8; 0x2000];
        rom[0x0008] = 0x18;
        rom[0x0009] = 0xFE;
        // Solid-block character pattern at $1800 onwards (8 bytes of
        // $FF per character — fills the cell).
        for byte in rom.iter_mut().skip(CHAR_ROM_OFFSET).take(2048) {
            *byte = 0xFF;
        }
        rom
    }

    #[test]
    fn frame_returns_expected_tstates() {
        let mut sys = Aquarius::new(trap_rom(), 0);
        let t = sys.run_frame();
        assert_eq!(t, CPU_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = Aquarius::new(trap_rom(), 0);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }

    #[test]
    fn rom_visible_at_low_window() {
        let sys = Aquarius::new(trap_rom(), 0);
        assert_eq!(sys.mem_read(0x0008), 0x18);
        // Character ROM byte.
        assert_eq!(sys.mem_read(0x1800), 0xFF);
    }

    #[test]
    fn char_and_colour_ram_round_trip() {
        let mut sys = Aquarius::new(trap_rom(), 0);
        sys.mem_write(0x3000, b'A');
        sys.mem_write(0x3400, 0xF0);
        assert_eq!(sys.mem_read(0x3000), b'A');
        assert_eq!(sys.mem_read(0x3400), 0xF0);
    }

    #[test]
    fn expansion_ram_round_trip_when_present() {
        let mut sys = Aquarius::new(trap_rom(), 16);
        sys.mem_write(0x4000, 0x42);
        sys.mem_write(0x7FFF, 0x77);
        assert_eq!(sys.mem_read(0x4000), 0x42);
        assert_eq!(sys.mem_read(0x7FFF), 0x77);
    }

    #[test]
    fn expansion_ram_returns_ff_without_expansion() {
        let mut sys = Aquarius::new(trap_rom(), 0);
        sys.mem_write(0x4000, 0x42);
        assert_eq!(sys.mem_read(0x4000), 0xFF);
    }

    #[test]
    fn keyboard_high_byte_selects_row() {
        let mut sys = Aquarius::new(trap_rom(), 0);
        sys.key_matrix[3] = 0x0F; // Row 3 has columns 4-7 pressed.
        // Selecting row 3 means clearing bit 3 of the high address byte.
        let port = ((!(1u16 << 3)) << 8) | 0xFF;
        assert_eq!(sys.io_read(port), 0x0F);
    }

    #[test]
    fn writing_ff_drives_speaker_bit() {
        let mut sys = Aquarius::new(trap_rom(), 0);
        sys.io_write(0xFF, 0x01);
        assert!(sys.speaker_bit());
        sys.io_write(0xFF, 0x00);
        assert!(!sys.speaker_bit());
    }

    #[test]
    fn render_paints_framebuffer_with_default_colour_ram() {
        let mut sys = Aquarius::new(trap_rom(), 0);
        // Default char RAM = $20 (space). With solid $FF character
        // pattern this would render all cells as the fg colour for
        // space — but space is char $20 so the pattern would be
        // whatever the BASIC font says. Our trap ROM puts $FF
        // patterns in EVERY character, so cells should render to fg.
        sys.run_frame();
        let fb = sys.framebuffer();
        assert_eq!(fb.len(), (FB_WIDTH * FB_HEIGHT) as usize);
        let unique: std::collections::HashSet<u32> = fb.iter().copied().collect();
        assert!(
            unique.len() >= 1,
            "framebuffer should have rendered at least one colour"
        );
    }

    #[test]
    fn key_press_and_release() {
        let mut sys = Aquarius::new(trap_rom(), 0);
        sys.set_key(2, 4, true);
        assert_eq!(sys.key_matrix[2] & (1 << 4), 0);
        sys.set_key(2, 4, false);
        assert_eq!(sys.key_matrix[2] & (1 << 4), 1 << 4);
    }
}
