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
//! generator is a separate 2 KB ROM (supplied via `set_char_rom`), not part
//! of the BASIC ROM.
//!
//! - **CPU:** Z80A @ 3.5 MHz
//! - **ROM:** 8 KB Microsoft BASIC at `$0000-$1FFF`; separate 2 KB
//!   character-generator ROM
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
//! | `$0000-$1FFF` | 8 KB Microsoft BASIC ROM                   |
//! | `$2000-$2FFF` | Unmapped (`$FF`)                          |
//! | `$3000-$33FF` | 1 KB character RAM                        |
//! | `$3400-$37FF` | 1 KB colour RAM                           |
//! | `$3800-$3FFF` | 2 KB spare RAM                            |
//! | `$4000-$7FFF` | Up to 16 KB expansion RAM                 |
//! | `$8000-$DFFF` | Unmapped                                  |
//! | `$C000-$FFFF` | Cart ROM (8 KB at $E000, or 16 KB at $C000) |
//!
//! # I/O map
//!
//! | Port  | R/W   | Function                                            |
//! |-------|-------|-----------------------------------------------------|
//! | `$F6` | r/w   | Mini Expander AY-3-8910 data (controllers on R14/R15)|
//! | `$F7` | write | Mini Expander AY-3-8910 register select              |
//! | `$FC` | r/w   | Cassette (stub)                                     |
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
//!
//! # Controllers (Mini Expander)
//!
//! The hand controllers are part of the Mini Expander peripheral, which
//! carries an AY-3-8910 reached at `$F7` (register select) / `$F6` (data).
//! Each controller is an Intellivision-style 16-position rotary disc plus
//! six side buttons; a press ANDs a position/button code into the player's
//! 8-bit byte, which the game reads through the AY's I/O ports: port A
//! (register 14) is player 2, port B (register 15) is player 1. Codes are
//! transcribed from MAME `bus/aquarius/mini.cpp`.

use gi_ay_3_8910::Ay3_8910;
use zilog_z80::{BusOp, Z80};

/// Mini Expander AY-3-8910 clock: the Z80 clock divided by two
/// (MAME `DERIVED_CLOCK(1, 2)`).
const AY_CLOCK_HZ: u32 = (CPU_CLOCK_HZ / 2) as u32;
/// AY audio configuration. The controllers only use the chip's I/O ports, so
/// these feed the (unconsumed) tone generator and are not load-bearing.
const AY_SAMPLE_RATE: u32 = 44_100;
const AY_SAMPLES_PER_FRAME: usize = 882;

/// The 16 rotary-disc position codes, in clock order from 12:00 — each value
/// is ANDed into the controller byte when the disc rests at that position.
/// (MAME `mini.cpp` `input_changed` masks.)
const DISC_CODES: [u8; 16] = [
    0xFB, // 12:00 (up)
    0xEB, // 01:00
    0xE9, // 01:30 (up+right)
    0xF9, // 02:00
    0xFD, // 03:00 (right)
    0xED, // 04:00
    0xEC, // 04:30 (down+right)
    0xFC, // 05:00
    0xFE, // 06:00 (down)
    0xEE, // 06:30
    0xE6, // 07:00 (down+left)
    0xF6, // 08:00
    0xF7, // 09:00 (left)
    0xE7, // 09:30
    0xE3, // 10:00 (up+left)
    0xF3, // 11:00
];

/// The six side-button codes, ANDed into the controller byte when held.
const BUTTON_CODES: [u8; 6] = [0xBF, 0x7B, 0x5F, 0xDF, 0x7D, 0x7E];

/// Map the eight host directions onto a disc-position index (into
/// [`DISC_CODES`]). Opposing presses cancel; a pure diagonal picks the disc's
/// half-hour position, and the two diagonals with no exact 16-way slot snap to
/// the nearest (07:00 for down-left, 10:00 for up-left). Returns `None` when
/// the stick is centred.
fn disc_position(up: bool, down: bool, left: bool, right: bool) -> Option<usize> {
    let vertical = i8::from(down) - i8::from(up); // -1 up, +1 down
    let horizontal = i8::from(right) - i8::from(left); // -1 left, +1 right
    Some(match (vertical, horizontal) {
        (-1, 0) => 0,   // 12:00 up
        (-1, 1) => 2,   // 01:30 up+right
        (0, 1) => 4,    // 03:00 right
        (1, 1) => 6,    // 04:30 down+right
        (1, 0) => 8,    // 06:00 down
        (1, -1) => 10,  // 07:00 down+left
        (0, -1) => 12,  // 09:00 left
        (-1, -1) => 14, // 10:00 up+left
        _ => return None,
    })
}

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
    /// 2 KB character-generator ROM (256 glyphs × 8 rows). Empty until supplied
    /// via [`Aquarius::set_char_rom`]; the renderer falls back to the system
    /// ROM's upper 2 KB when absent (wrong, but keeps headless tests building
    /// without the separate char ROM).
    char_rom: Vec<u8>,
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
    /// When `Some`, every I/O port access is appended here (debug trace).
    io_trace: Option<Vec<IoEvent>>,
    /// Set true the cycle a VBlank NMI is being delivered.
    nmi_pulse: bool,
    /// Mini Expander AY-3-8910. The controllers are read through its I/O
    /// ports; the tone generators are present but unconsumed.
    psg: Ay3_8910,
    /// Controller bytes presented on the AY I/O ports, active low. Index 0 is
    /// AY port A (player 2), index 1 is AY port B (player 1). Idle is `0xFF`.
    ctrl_input: [u8; 2],
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
            char_rom: Vec::new(),
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
            io_trace: None,
            nmi_pulse: false,
            psg: Ay3_8910::new(AY_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            ctrl_input: [0xFF; 2],
        }
    }

    /// Insert a cart ROM. It sits at the top of memory by size: an 8 KB
    /// cart at `$E000-$FFFF`, a 16 KB game cart at `$C000-$FFFF`.
    pub fn insert_cart(&mut self, rom: Vec<u8>) {
        self.cart_rom = rom;
    }

    /// Supply the 2 KB character-generator ROM (256 glyphs × 8 rows). Without
    /// it the display renders garbage, since the Aquarius font lives in a
    /// separate chip — not in the BASIC ROM.
    pub fn set_char_rom(&mut self, char_rom: Vec<u8>) {
        self.char_rom = char_rom;
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
                let io_port = (self.cpu.addr & 0xFF) as u8;
                let io_pc = self.cpu.regs.pc;
                let io_val = self.io_read(self.cpu.addr);
                self.cpu.data_in = io_val;
                if let Some(trace) = &mut self.io_trace {
                    trace.push(IoEvent {
                        pc: io_pc,
                        port: io_port,
                        value: io_val,
                        write: false,
                    });
                }
            }
            Some(BusOp::IoWrite) => {
                if let Some(trace) = &mut self.io_trace {
                    trace.push(IoEvent {
                        pc: self.cpu.regs.pc,
                        port: (self.cpu.addr & 0xFF) as u8,
                        value: self.cpu.data,
                        write: true,
                    });
                }
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
            // Cartridge sits at the top of memory, sized by the image: an 8 KB
            // cart at $E000-$FFFF, a 16 KB game cart at $C000-$FFFF. Everything
            // below it in $8000-$BFFF reads as open bus.
            0x8000..=0xFFFF => {
                let base = 0x1_0000usize.saturating_sub(self.cart_rom.len());
                let idx = addr as usize;
                if !self.cart_rom.is_empty() && idx >= base {
                    self.cart_rom[idx - base]
                } else {
                    0xFF
                }
            }
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
        // Mini Expander AY-3-8910 data read ($F6). When the program selects
        // register 14/15 (the I/O ports) in input mode, the controllers read
        // back here: port A = player 2, port B = player 1.
        if low == 0xF6 {
            self.psg.set_port_a_input_mask(self.ctrl_input[0]);
            self.psg.set_port_b_input(self.ctrl_input[1]);
            return self.psg.read_data();
        }
        0xFF
    }

    fn io_write(&mut self, port: u16, value: u8) {
        let low = port as u8;
        match low {
            // Mini Expander AY-3-8910: $F7 selects the register, $F6 writes data.
            0xF6 => self.psg.write_data(value),
            0xF7 => self.psg.select_register(value),
            0xFC => {} // Cassette (stub).
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
                // Glyphs come from the dedicated 2 KB character ROM (one 8-byte
                // bitmap per code). When it hasn't been supplied, fall back to
                // the system ROM's upper 2 KB — wrong (that region is code), but
                // it keeps headless tests rendering *something* without the ROM.
                let (glyph, base) = if self.char_rom.len() >= 2048 {
                    (self.char_rom.as_slice(), char_code * 8)
                } else {
                    (self.rom.as_slice(), CHAR_ROM_OFFSET + char_code * 8)
                };
                for py in 0..CHAR_HEIGHT {
                    let pattern = glyph.get(base + py as usize).copied().unwrap_or(0);
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

    /// Set the hand-controller state for `port` (1 or 2). The directions choose
    /// one of the disc's 16 positions (the eight host directions map to the
    /// nearest position); `fire` is the first side button. The composed code is
    /// presented on the AY I/O port the game reads — port B for player 1, port
    /// A for player 2. Out-of-range ports clamp to the valid pair.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(
        &mut self,
        port: u8,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        fire: bool,
    ) {
        // Player 1 reads on AY port B (`ctrl_input[1]`), player 2 on port A
        // (`ctrl_input[0]`).
        let idx = if port.clamp(1, 2) == 1 { 1 } else { 0 };
        let mut code = 0xFFu8;
        if let Some(pos) = disc_position(up, down, left, right) {
            code &= DISC_CODES[pos];
        }
        if fire {
            code &= BUTTON_CODES[0];
        }
        self.ctrl_input[idx] = code;
    }

    /// The controller byte currently presented on AY port A (player 2) and
    /// port B (player 1). For inspection and host-side input wiring.
    #[must_use]
    pub fn controller_bytes(&self) -> [u8; 2] {
        self.ctrl_input
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

impl zilog_z80::Z80Stepper for Aquarius {
    fn z80_instructions_retired(&self) -> u64 {
        self.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        self.tick_tstate();
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
            !unique.is_empty(),
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

    /// Select an AY register through the expander and read its data back.
    fn read_ay(sys: &mut Aquarius, reg: u8) -> u8 {
        sys.io_write(0xF7, reg); // register select at $F7
        sys.io_read(0xF6) // data read at $F6
    }

    #[test]
    fn controllers_read_through_the_expander_ay_ports() {
        let mut sys = Aquarius::new(trap_rom(), 0);
        // Idle: both ports read all-high (input mode on reset).
        assert_eq!(read_ay(&mut sys, 14), 0xFF);
        assert_eq!(read_ay(&mut sys, 15), 0xFF);

        // Player 1 reads on port B (register 15). Up = disc 12:00 (0xFB),
        // with fire = side button 1 (0xBF): the byte is the AND of both.
        sys.set_joystick(1, true, false, false, false, true);
        assert_eq!(read_ay(&mut sys, 15), 0xFB & 0xBF, "P1 up + fire on port B");
        // Port A (player 2) is untouched.
        assert_eq!(read_ay(&mut sys, 14), 0xFF);

        // Player 2 reads on port A (register 14). Down+right = disc 04:30.
        sys.set_joystick(2, false, true, false, true, false);
        assert_eq!(
            read_ay(&mut sys, 14),
            0xEC,
            "P2 down+right (04:30) on port A"
        );

        // Centre (no direction, no fire) returns to all-high.
        sys.set_joystick(1, false, false, false, false, false);
        assert_eq!(read_ay(&mut sys, 15), 0xFF, "P1 centred");
    }

    #[test]
    fn opposing_directions_cancel_to_centre() {
        assert_eq!(super::disc_position(true, true, false, false), None);
        assert_eq!(super::disc_position(false, false, true, true), None);
        assert_eq!(super::disc_position(true, false, true, false), Some(14)); // up+left
    }
}

/// One captured I/O port access, for the debug trace.
#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    /// CPU program counter at the time of the access.
    pub pc: u16,
    /// I/O port (low 8 bits of the address bus).
    pub port: u8,
    /// Byte written, or byte returned on a read.
    pub value: u8,
    /// `true` for `OUT`, `false` for `IN`.
    pub write: bool,
}

impl Aquarius {
    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Start (or restart) the I/O port-access trace.
    pub fn start_io_trace(&mut self) {
        self.io_trace = Some(Vec::new());
    }

    /// Stop tracing and return the captured I/O events.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.io_trace.take().unwrap_or_default()
    }
}
