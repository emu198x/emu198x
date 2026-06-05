//! Acorn Electron machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-acorn-electron`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — ULA register map at
//! `$FE00-$FE0F`, eight display modes with mode-dependent bpp and
//! byte-per-line layouts, BBC-Micro-compatible 8-colour palette,
//! 14×4 keyboard matrix scanned via the address bus, VBlank + RTC
//! IRQ sources — but the wiring is written against [`mos_6502::M6502`]'s
//! public pin fields and its own internal reset-vector fetch.
//!
//! # The Acorn Electron
//!
//! The Electron (1983) is a cost-reduced BBC Micro. A single custom ULA
//! replaces the BBC's discrete video, sound, keyboard, and interrupt
//! chips. Sold heavily into UK schools and homes. **Famous for its
//! ULA's bus-contention behaviour**: the CPU and ULA share the RAM
//! bus, and the CPU effectively halves to 1 MHz during RAM access
//! windows (this initial port runs CPU at a flat 2 MHz; contention
//! is in the accuracy backlog).
//!
//! - **CPU:** 6502A @ 2 MHz (1 MHz under ULA contention on real
//!   hardware — flat 2 MHz here)
//! - **ULA:** custom — video / sound / keyboard / cassette /
//!   interrupts. Eight display modes matching the BBC Micro except
//!   MODE 7 teletext (which lives in the SAA5050 on the BBC and is
//!   absent from the Electron).
//! - **RAM:** 32 KB at `$0000-$7FFF`
//! - **OS ROM:** 16 KB at `$C000-$FFFF` (with ULA registers visible
//!   at `$FE00-$FE0F`)
//! - **BASIC ROM:** 16 KB at `$8000-$BFFF` (the sideways ROM slot)
//!
//! # Memory map
//!
//! | Range         | Contents                                       |
//! |---------------|------------------------------------------------|
//! | `$0000-$7FFF` | 32 KB RAM (shared with ULA video fetch)        |
//! | `$8000-$BFFF` | Sideways ROM slot (BASIC by default)           |
//! | `$C000-$FDFF` | OS ROM                                         |
//! | `$FE00-$FE0F` | ULA registers (override the OS ROM here)       |
//! | `$FE10-$FEFF` | I/O space (returns `$FF`)                      |
//! | `$FF00-$FFFF` | OS ROM (includes reset / IRQ / NMI vectors)    |
//!
//! # ULA registers
//!
//! - `$FE00` r: interrupt status (bit 7 high, bit 0 master IRQ) /
//!   w: interrupt control (bits 2-6 enable each source). The keyboard
//!   is read through paged ROM slots 8/9 ($8000-$BFFF), not here.
//! - `$FE02-$FE03` w: screen start address high/low (display fetch)
//! - `$FE04` w: cassette data shift register (stub)
//! - `$FE05` w: ROM page select + interrupt clears + NMI enable
//! - `$FE06` w: counter / sound period low byte
//! - `$FE07` w: misc control — display mode (bits 3-5), cassette
//!   motor, caps lock LED
//! - `$FE08-$FE0F` w: palette mapping — each register sets two
//!   logical-to-physical colour entries

use mos_6502::M6502;

/// Framebuffer width (640 pixels; modes 1/4/6 render 320 doubled, modes
/// 2/5 render 160 quadrupled).
pub const FB_WIDTH: u32 = 640;
/// Framebuffer height (256 visible scanlines per PAL Electron frame).
pub const FB_HEIGHT: u32 = 256;

const CYCLES_PER_SCANLINE: u64 = 128;
const SCANLINES_PER_FRAME: u16 = 312;
const CYCLES_PER_FRAME: u64 = CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME as u64;

const CPU_CLOCK_HZ: u32 = 2_000_000;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Electron 8-colour physical palette (3-bit RGB, ARGB32). Identical to
/// the BBC Micro standard colours.
const PALETTE: [u32; 8] = [
    0xFF00_0000, // 0: black
    0xFFFF_0000, // 1: red
    0xFF00_FF00, // 2: green
    0xFFFF_FF00, // 3: yellow
    0xFF00_00FF, // 4: blue
    0xFFFF_00FF, // 5: magenta
    0xFF00_FFFF, // 6: cyan
    0xFFFF_FFFF, // 7: white
];

/// Decode the eight palette registers into the 16 logical colours.
///
/// The Electron ULA scrambles the logical→physical mapping: each of the four
/// register *pairs* `(0,1) (2,3) (4,5) (6,7)` feeds four logical colours, and
/// every colour's red/green/blue components are drawn from individual,
/// non-contiguous bits of the pair. `pal_regs` already holds the inverted
/// bytes the ULA stores (`written ^ 0xFF`), so a set bit means the component
/// is on. Algorithm and bit positions match MAME's `electron_ula` device.
fn decode_palette(pal_regs: &[u8; 8]) -> [u32; 16] {
    // Pen base for each register pair — the pairs do not map to contiguous
    // logical-colour ranges.
    const PEN_BASE: [usize; 4] = [0, 4, 5, 1];
    let bit = |x: u8, n: u8| u32::from((x >> n) & 1);
    let argb =
        |r: u32, g: u32, b: u32| 0xFF00_0000 | ((r * 0xFF) << 16) | ((g * 0xFF) << 8) | (b * 0xFF);
    let mut out = [0xFF00_0000u32; 16];
    for pair in 0..4 {
        let p0 = pal_regs[pair * 2];
        let p1 = pal_regs[pair * 2 + 1];
        let pb = PEN_BASE[pair];
        out[pb] = argb(bit(p1, 0), bit(p1, 4), bit(p0, 4));
        out[pb + 2] = argb(bit(p1, 1), bit(p1, 5), bit(p0, 5));
        out[pb + 8] = argb(bit(p1, 2), bit(p0, 2), bit(p0, 6));
        out[pb + 10] = argb(bit(p1, 3), bit(p0, 3), bit(p0, 7));
    }
    out
}

struct ElectronUla {
    /// `$FE00` interrupt status: bit 0 master IRQ, 1 power-on reset,
    /// 2 display end (VBlank), 3 RTC, 4-6 cassette.
    interrupt_status: u8,
    /// Active-high enable bits 1-6 for the matching status sources.
    interrupt_enable: u8,
    screen_start: u16,
    /// `$FE07` bits: 3-5 display mode, 6 cassette motor, 7 caps lock.
    misc_control: u8,
    /// The eight palette registers (`$FE08-$FE0F`) as stored by the ULA. The
    /// hardware inverts on write, so each holds `written ^ 0xFF`. Decoded in
    /// pairs into `logical_argb`.
    pal_regs: [u8; 8],
    /// The 16 logical colours as ARGB32, recomputed from `pal_regs` on each
    /// palette write. The Electron's logical→physical mapping is scrambled and
    /// per-component (see `decode_palette`).
    logical_argb: [u32; 16],
    /// `$FE06` counter / sound period low byte.
    counter: u8,
    rom_page: u8,
    sound_period: u16,
    sound_toggle: bool,
    sound_counter: u16,
    sound_enabled: bool,
    /// Bresenham accumulator for 2 MHz → 48 kHz downsample.
    audio_accum: u64,
    audio_buffer: Vec<f32>,
    /// 14 columns × 4 rows, indexed `[column][row]`. `true` = pressed;
    /// `read_keyboard` returns the rows active-high, as the hardware does.
    keyboard: [[bool; 4]; 14],
}

impl ElectronUla {
    fn new() -> Self {
        let pal_regs = [0xFFu8; 8];
        let logical_argb = decode_palette(&pal_regs);
        Self {
            interrupt_status: 0x02, // Power-on reset bit set
            interrupt_enable: 0,
            screen_start: 0x3000, // MODE 6 default
            misc_control: 0,
            pal_regs,
            logical_argb,
            counter: 0,
            rom_page: 0,
            sound_period: 0,
            sound_toggle: false,
            sound_counter: 0,
            sound_enabled: false,
            audio_accum: 0,
            audio_buffer: Vec::with_capacity(960),
            keyboard: [[false; 4]; 14],
        }
    }

    fn mode(&self) -> u8 {
        (self.misc_control >> 3) & 0x07
    }

    fn bpp(&self) -> u8 {
        match self.mode() {
            0 | 3 | 4 | 6 => 1,
            1 | 5 => 2,
            2 => 4,
            _ => 1,
        }
    }

    fn bytes_per_line(&self) -> usize {
        match self.mode() {
            0..=3 => 80,
            4..=6 => 40,
            _ => 40,
        }
    }

    fn pixel_width(&self) -> usize {
        match self.mode() {
            0 | 3 => 1,
            1 | 4 | 6 => 2,
            2 | 5 => 4,
            _ => 2,
        }
    }

    /// Scanlines per character row. Text modes (3, 6, 7) space rows 10 lines
    /// apart — eight glyph lines plus a two-line gap — giving a 250-line
    /// display; graphics modes pack rows eight lines tall. (MAME `mode_max_ra`.)
    fn char_pitch(&self) -> usize {
        const T: [usize; 8] = [8, 8, 8, 10, 8, 8, 10, 10];
        T[self.mode() as usize & 7]
    }

    /// Number of displayed scanlines for the current mode (MAME `mode_dispend`).
    fn display_lines(&self) -> usize {
        const T: [usize; 8] = [256, 256, 256, 250, 256, 256, 250, 250];
        T[self.mode() as usize & 7]
    }

    /// Screen-buffer size for the current mode. Video addresses that run past
    /// the top of RAM (`$8000`) wrap by subtracting this (MAME `mode_size`).
    fn screen_size(&self) -> usize {
        const T: [usize; 8] = [
            0x5000, 0x5000, 0x5000, 0x4000, 0x2800, 0x2800, 0x2000, 0x2000,
        ];
        T[self.mode() as usize & 7]
    }

    fn colour(&self, logical_index: u8) -> u32 {
        self.logical_argb[logical_index as usize & 0x0F]
    }

    fn read_keyboard(&self, addr: u16) -> u8 {
        // A0-A13 select columns: a column contributes when its address
        // bit is LOW. The four rows return on D0-D3, active high — a
        // pressed key sets its bit. (MAME `electron_state::keyboard_r`.)
        let mut data = 0u8;
        for col in 0..14 {
            if (addr >> col) & 1 == 0 {
                for row in 0..4 {
                    if self.keyboard[col][row] {
                        data |= 1 << row;
                    }
                }
            }
        }
        data & 0x0F
    }

    fn tick_sound(&mut self) {
        if self.sound_enabled && self.sound_period > 0 {
            if self.sound_counter == 0 {
                self.sound_counter = self.sound_period;
                self.sound_toggle = !self.sound_toggle;
            } else {
                self.sound_counter -= 1;
            }
        }
        self.audio_accum += u64::from(AUDIO_SAMPLE_RATE);
        if self.audio_accum >= u64::from(CPU_CLOCK_HZ) {
            self.audio_accum -= u64::from(CPU_CLOCK_HZ);
            let sample = if self.sound_enabled && self.sound_toggle {
                0.3_f32
            } else {
                0.0
            };
            self.audio_buffer.push(sample);
        }
    }

    fn irq_active(&self) -> bool {
        // MAME masks with ~0x83: bit 0 (master) and bit 1 (power-on
        // reset) never raise the line; sources are bits 2-6.
        (self.interrupt_status & self.interrupt_enable & 0x7C) != 0
    }

    fn refresh_master_irq(&mut self) {
        if self.irq_active() {
            self.interrupt_status |= 0x01;
        } else {
            self.interrupt_status &= !0x01;
        }
    }

    fn signal_vblank(&mut self) {
        self.interrupt_status |= 0x04;
        self.refresh_master_irq();
    }

    fn signal_rtc(&mut self) {
        self.interrupt_status |= 0x08;
        self.refresh_master_irq();
    }
}

/// Acorn Electron machine.
pub struct AcornElectron {
    cpu: M6502,
    ula: ElectronUla,
    ram: [u8; 32768],
    os_rom: Vec<u8>,
    basic_rom: Vec<u8>,
    framebuffer: Vec<u32>,
    cpu_cycles: u64,
    frame_count: u64,
    /// Scanline currently being rendered.
    scanline: u16,
}

impl AcornElectron {
    /// Create a new Electron with the given OS and BASIC ROMs (each
    /// 16 KB). The 6502 fetches the reset vector through the bus on
    /// its own first cycles, so no manual PC initialisation needed.
    #[must_use]
    pub fn new(os_rom: Vec<u8>, basic_rom: Vec<u8>) -> Self {
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
            ula: ElectronUla::new(),
            ram: [0; 32768],
            os_rom,
            basic_rom,
            framebuffer: vec![PALETTE[0]; (FB_WIDTH * FB_HEIGHT) as usize],
            cpu_cycles: 0,
            frame_count: 0,
            scanline: 0,
        }
    }

    /// Run one PAL frame (312 scanlines × 128 CPU cycles each).
    pub fn run_frame(&mut self) -> u64 {
        self.scanline = 0;
        for line in 0..SCANLINES_PER_FRAME {
            for _ in 0..CYCLES_PER_SCANLINE {
                self.tick_cpu_cycle();
            }
            if line < FB_HEIGHT as u16 {
                self.render_scanline();
            }
            if line == FB_HEIGHT as u16 {
                self.ula.signal_vblank();
            }
            if line == 0 {
                self.ula.signal_rtc();
            }
            self.cpu.irq = self.ula.irq_active();
        }
        self.frame_count += 1;
        CYCLES_PER_FRAME
    }

    fn tick_cpu_cycle(&mut self) {
        self.cpu.tick();
        if self.cpu.rw {
            self.cpu.data_in = self.mem_read(self.cpu.addr);
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
        self.ula.tick_sound();
        self.cpu_cycles += 1;
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0xBFFF => {
                // Sideways-ROM slot. Slots 8 and 9 are the keyboard: reading
                // the paged area with those selected returns the matrix
                // (A0-A13 select columns, D0-D3 the rows). Other slots hold
                // BASIC / expansion ROMs.
                match self.ula.rom_page {
                    8 | 9 => self.ula.read_keyboard(addr),
                    _ => self
                        .basic_rom
                        .get((addr - 0x8000) as usize)
                        .copied()
                        .unwrap_or(0xFF),
                }
            }
            0xFE00..=0xFE0F => self.ula_read(addr),
            0xFC00..=0xFDFF | 0xFE10..=0xFEFF => 0xFF,
            0xC000..=0xFFFF => self
                .os_rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize] = value,
            0xFE00..=0xFE0F => self.ula_write(addr, value),
            _ => {}
        }
    }

    fn ula_read(&mut self, addr: u16) -> u8 {
        match addr & 0x0F {
            0x00 => {
                // Interrupt status. Bit 7 always reads high; bit 0 is the
                // master IRQ; bits 1-6 the individual sources. The keyboard
                // is NOT read here — it pages into $8000-$BFFF via ROM
                // slots 8/9. (MAME `electron_ula` SHEILA $00 read.)
                0x80 | self.ula.interrupt_status
            }
            _ => 0xFF,
        }
    }

    fn ula_write(&mut self, addr: u16, value: u8) {
        match addr & 0x0F {
            // $FE00: interrupt control. Bits 2-6 enable the matching
            // status sources; the master IRQ asserts when an enabled
            // source is pending. Interrupt *clears* are on $FE05, not
            // here. (MAME `electron_ula` SHEILA $00 write.)
            0x00 => {
                self.ula.interrupt_enable = value;
                self.ula.refresh_master_irq();
            }
            // Screen start address. The ULA packs address bits A6-A14 across
            // the two registers (the low six bits are always zero, so the
            // start is on a 64-byte boundary): $FE02 carries A8-A6 in its top
            // three bits, $FE03 carries A14-A9 in its low six. (MAME decode.)
            0x02 => {
                self.ula.screen_start =
                    (self.ula.screen_start & 0x7E00) | ((u16::from(value) & 0xE0) << 1);
            }
            0x03 => {
                self.ula.screen_start =
                    (self.ula.screen_start & 0x01FF) | ((u16::from(value) & 0x3F) << 9);
            }
            0x04 => {} // Cassette data shift (stub)
            0x05 => {
                self.ula.rom_page = value & 0x0F;
                if value & 0x10 != 0 {
                    self.ula.interrupt_status &= !0x04;
                }
                if value & 0x20 != 0 {
                    self.ula.interrupt_status &= !0x08;
                }
                if value & 0x40 != 0 {
                    self.ula.interrupt_status &= !0x40;
                }
                self.ula.refresh_master_irq();
            }
            0x06 => {
                self.ula.counter = value;
                self.ula.sound_period = (self.ula.sound_period & 0xFF00) | u16::from(value);
                self.ula.sound_counter = self.ula.sound_period;
                self.ula.sound_enabled = self.ula.sound_period > 0;
            }
            0x07 => {
                self.ula.misc_control = value;
            }
            0x08..=0x0F => {
                // The ULA inverts the written byte, then decodes the register
                // pair this write belongs to into four logical colours.
                let reg = (addr & 0x07) as usize;
                self.ula.pal_regs[reg] = value ^ 0xFF;
                self.ula.logical_argb = decode_palette(&self.ula.pal_regs);
            }
            _ => {}
        }
    }

    fn render_scanline(&mut self) {
        if self.scanline >= FB_HEIGHT as u16 {
            return;
        }
        let line = self.scanline as usize;
        let offset = line * FB_WIDTH as usize;
        let background = self.ula.colour(0);

        // Lines past the displayed area, and the two-line gap between text
        // rows, show the background colour rather than glyph data.
        let pitch = self.ula.char_pitch();
        let char_line = line % pitch;
        if line >= self.ula.display_lines() || char_line >= 8 {
            for x in 0..FB_WIDTH as usize {
                self.framebuffer[offset + x] = background;
            }
            self.scanline += 1;
            return;
        }

        let bpp = self.ula.bpp() as usize;
        let cols = self.ula.bytes_per_line();
        let pixel_width = self.ula.pixel_width();
        let pixels_per_byte = 8 / bpp;
        let char_row = line / pitch;
        let screen_base = self.ula.screen_start as usize;
        let screen_size = self.ula.screen_size();
        for col in 0..cols {
            // The Electron stores each 8×8 cell as eight consecutive bytes
            // (one per glyph line), so columns step by 8 and the scanline
            // within the cell is the low offset. Addresses that overflow the
            // top of RAM wrap by the mode's screen-buffer size.
            let mut addr = screen_base + char_row * cols * 8 + col * 8 + char_line;
            if addr & 0x8000 != 0 {
                addr -= screen_size;
            }
            let byte = self.ram[addr & 0x7FFF];
            for px in 0..pixels_per_byte {
                let colour_idx = match bpp {
                    1 => (byte >> (7 - px)) & 0x01,
                    2 => {
                        let bit_h = (byte >> (7 - px)) & 0x01;
                        let bit_l = (byte >> (3 - px)) & 0x01;
                        (bit_h << 1) | bit_l
                    }
                    4 => {
                        let pi = (px & 1) as u8;
                        let b7 = (byte >> (7 - pi)) & 0x01;
                        let b5 = (byte >> (5 - pi)) & 0x01;
                        let b3 = (byte >> (3 - pi)) & 0x01;
                        let b1 = (byte >> (1 - pi)) & 0x01;
                        (b7 << 3) | (b5 << 2) | (b3 << 1) | b1
                    }
                    _ => 0,
                };
                let argb = self.ula.colour(colour_idx);
                let fb_x = (col * pixels_per_byte + px) * pixel_width;
                for w in 0..pixel_width {
                    if fb_x + w < FB_WIDTH as usize {
                        self.framebuffer[offset + fb_x + w] = argb;
                    }
                }
            }
        }
        self.scanline += 1;
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

    /// Framebuffer (640×256 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Take audio samples (mono f32 at 48 kHz).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.ula.audio_buffer)
    }

    /// Press a key at the given (column, row) — Electron keyboard
    /// matrix is 14 columns × 4 rows.
    pub fn press_key(&mut self, col: usize, row: usize) {
        if col < 14 && row < 4 {
            self.ula.keyboard[col][row] = true;
        }
    }

    /// Release a key at the given (column, row).
    pub fn release_key(&mut self, col: usize, row: usize) {
        if col < 14 && row < 4 {
            self.ula.keyboard[col][row] = false;
        }
    }

    /// CPU reference.
    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    /// CPU mutable reference.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    /// Total CPU cycles since power-on.
    #[must_use]
    pub fn cpu_cycles(&self) -> u64 {
        self.cpu_cycles
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Current ULA display mode (0-7).
    #[must_use]
    pub fn display_mode(&self) -> u8 {
        self.ula.mode()
    }

    /// `true` if the ULA's master IRQ line is asserted.
    #[must_use]
    pub fn irq_asserted(&self) -> bool {
        self.ula.irq_active()
    }
}

impl AcornElectron {
    /// Read one byte with no side effects (RAM / BASIC / OS ROM;
    /// `$FF` for the ULA / I/O page).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0xBFFF => self
                .basic_rom
                .get((addr - 0x8000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xFC00..=0xFEFF => 0xFF,
            0xC000..=0xFFFF => self
                .os_rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
        }
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
            self.tick_cpu_cycle();
            ticks += 1;
        }
        while !self.cpu.instruction_complete() && ticks < 4096 {
            self.tick_cpu_cycle();
            ticks += 1;
        }
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_roms() -> (Vec<u8>, Vec<u8>) {
        // OS ROM: 16 KB. Reset vector at $FFFC = $C000 (top of OS).
        // Put a JR -2 trap (6502 is a JMP self at $C000).
        let mut os = vec![0xEA_u8; 0x4000]; // NOP fill
        // $C000 = JMP $C000 → 4C 00 C0
        os[0x0000] = 0x4C;
        os[0x0001] = 0x00;
        os[0x0002] = 0xC0;
        // Reset vector at offset $3FFC-$3FFD points to $C000.
        os[0x3FFC] = 0x00;
        os[0x3FFD] = 0xC0;
        // IRQ + BRK vectors at $FFFE-$FFFF also point to $C000.
        os[0x3FFE] = 0x00;
        os[0x3FFF] = 0xC0;
        // NMI vector at $FFFA-$FFFB points to $C000.
        os[0x3FFA] = 0x00;
        os[0x3FFB] = 0xC0;
        let basic = vec![0xFF; 0x4000];
        (os, basic)
    }

    #[test]
    fn frame_runs_expected_cycles() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        let t = sys.run_frame();
        assert_eq!(t, CYCLES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        for _ in 0..30 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 30);
    }

    #[test]
    fn memory_map_routes_pages() {
        let (mut os, mut basic) = trap_roms();
        os[0x0100] = 0xAA;
        basic[0x0100] = 0xBB;
        let mut sys = AcornElectron::new(os, basic);
        assert_eq!(sys.mem_read(0xC100), 0xAA);
        assert_eq!(sys.mem_read(0x8100), 0xBB);
        sys.mem_write(0x4000, 0x42);
        assert_eq!(sys.mem_read(0x4000), 0x42);
        // ROM writes ignored.
        sys.mem_write(0xC100, 0x99);
        assert_eq!(sys.mem_read(0xC100), 0xAA);
    }

    #[test]
    fn ula_screen_start_decodes_mode6_base() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        // The OS programs MODE 6's $6000 screen base as $FE03=$30, $FE02=$00.
        // The ULA packs address bits A14-A9 into $FE03 and A8-A6 into $FE02;
        // a naive high/low-byte reading would land at $0030 instead.
        sys.ula_write(0xFE03, 0x30);
        sys.ula_write(0xFE02, 0x00);
        assert_eq!(sys.ula.screen_start, 0x6000);
    }

    #[test]
    fn ula_misc_control_sets_display_mode() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        // Mode 4 = bits 3-5 = 100, so $20.
        sys.ula_write(0xFE07, 0b0010_0000);
        assert_eq!(sys.display_mode(), 4);
    }

    #[test]
    fn vblank_signal_sets_status_and_master_irq() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        // Enable display-end interrupt.
        sys.ula.interrupt_enable = 0x04;
        sys.ula.signal_vblank();
        assert!(sys.ula.interrupt_status & 0x04 != 0);
        assert!(sys.irq_asserted());
    }

    #[test]
    fn keyboard_press_and_release_round_trip() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        sys.press_key(3, 1);
        assert!(sys.ula.keyboard[3][1]);
        sys.release_key(3, 1);
        assert!(!sys.ula.keyboard[3][1]);
    }

    #[test]
    fn palette_decodes_mode6_black_on_white() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        // The OS's MODE 6 (2-colour) steady state: register pair 0 = $11,
        // pair 3 = $00. This must resolve to logical 0 = black, logical 1 =
        // white — the canonical boot screen. (Captured from the real ROM.)
        sys.ula_write(0xFE08, 0x11);
        sys.ula_write(0xFE09, 0x11);
        sys.ula_write(0xFE0E, 0x00);
        sys.ula_write(0xFE0F, 0x00);
        assert_eq!(sys.ula.colour(0), 0xFF00_0000, "logical 0 should be black");
        assert_eq!(sys.ula.colour(1), 0xFFFF_FFFF, "logical 1 should be white");
    }

    #[test]
    fn keyboard_reads_active_high_through_paged_rom() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        // Page the keyboard in — ROM slot 8 maps the matrix into
        // $8000-$BFFF.
        sys.ula_write(0xFE05, 0x08);
        // Press H = column 7, row 2.
        sys.press_key(7, 2);
        // Selecting column 7 means driving address bit 7 low; the row
        // returns active-high on D2.
        let sel7 = 0x8000 | (0x3FFF & !(1u16 << 7));
        assert_eq!(sys.mem_read(sel7) & 0x0F, 0x04, "row 2 should read high");
        // Leaving bit 7 high does not select column 7.
        let not7 = 0x8000 | (1u16 << 7);
        assert_eq!(sys.mem_read(not7) & 0x0F, 0x00, "column 7 not selected");
        sys.release_key(7, 2);
        assert_eq!(sys.mem_read(sel7) & 0x0F, 0x00, "released key reads low");
    }

    #[test]
    fn interrupt_control_write_gates_master_irq() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        // A pending RTC with no source enabled must not raise the line.
        sys.ula.signal_rtc();
        assert!(!sys.irq_asserted());
        // Writing $FE00 enables the RTC source, raising the master IRQ.
        sys.ula_write(0xFE00, 0x08);
        assert!(sys.irq_asserted());
        // The status read sets bit 7 high (MAME `0x80 | m_int_status`).
        assert_eq!(sys.mem_read(0xFE00) & 0x80, 0x80);
    }

    #[test]
    fn palette_register_write_inverts_and_stores() {
        let (os, basic) = trap_roms();
        let mut sys = AcornElectron::new(os, basic);
        sys.ula_write(0xFE08, 0x11);
        // The ULA inverts on write.
        assert_eq!(sys.ula.pal_regs[0], 0xEE);
    }
}
