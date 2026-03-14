//! BBC Micro Model B emulator.
//!
//! The BBC Micro (1981) by Acorn Computers, one of the most influential
//! educational computers. Eight display modes from 640×256 down to
//! teletext, driven by a 6845 CRTC and custom Video ULA.
//!
//! - **CPU:** 6502A @ 2 MHz (no contention — CPU and CRTC share RAM on
//!   alternate phases)
//! - **Video:** Motorola 6845 CRTC + Acorn Video ULA + SAA5050 (MODE 7)
//! - **Audio:** SN76489 (4 MHz clock, via System VIA)
//! - **I/O:** Two 6522 VIAs (System + User)
//! - **RAM:** 32 KB ($0000-$7FFF)
//! - **ROM:** 16 KB MOS at $C000-$FFFF + 16 KB sideways ROM at $8000-$BFFF

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, Cpu, ReadResult};
use mos_6502::Mos6502;
use mos_via_6522::Via6522;
use motorola_6845::Crtc6845;
use ti_sn76489::Sn76489;

/// PAL frame: 312 scanlines × 64 µs/line × 2 MHz = 39936 CPU cycles.
const CYCLES_PER_FRAME: u64 = 39936;
/// Framebuffer dimensions (MODE 0: 640×256, but we use a standard size).
pub const FB_WIDTH: u32 = 640;
pub const FB_HEIGHT: u32 = 256;

// ---------------------------------------------------------------------------
// Video ULA
// ---------------------------------------------------------------------------

/// Acorn Video ULA: serialises RAM data into pixels using palette mapping.
struct VideoUla {
    /// Control register ($FE20).
    control: u8,
    /// Palette: 16 entries, each 4 bits (flash + ~B ~G ~R).
    palette: [u8; 16],
}

impl VideoUla {
    fn new() -> Self {
        // Default palette: identity mapping with inverted RGB
        let mut palette = [0u8; 16];
        for i in 0..16 {
            palette[i] = (i as u8) ^ 0x07;
        }
        Self { control: 0, palette }
    }

    fn write_control(&mut self, value: u8) {
        self.control = value;
    }

    fn write_palette(&mut self, value: u8) {
        let logical = (value >> 4) as usize;
        let physical = value & 0x0F;
        self.palette[logical] = physical;
    }

    /// Bits per pixel for the current mode (1, 2, or 4).
    fn bpp(&self) -> u8 {
        match (self.control >> 2) & 0x03 {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => 1, // Unused combo
        }
    }

    /// Whether teletext mode is active.
    fn teletext(&self) -> bool {
        self.control & 0x02 != 0
    }

    /// CRTC clock rate: true = 2 MHz (80-col), false = 1 MHz (40-col).
    fn fast_clock(&self) -> bool {
        self.control & 0x10 != 0
    }

    /// Convert a palette index to ARGB32.
    fn palette_to_argb(&self, index: u8) -> u32 {
        let entry = self.palette[index as usize & 0x0F];
        // Physical colour: bits 2-0 are ~B, ~G, ~R (inverted)
        let r = if entry & 0x01 == 0 { 255 } else { 0 };
        let g = if entry & 0x02 == 0 { 255 } else { 0 };
        let b = if entry & 0x04 == 0 { 255 } else { 0 };
        0xFF00_0000 | (r << 16) | (g << 8) | b
    }
}

// ---------------------------------------------------------------------------
// Addressable latch (IC32)
// ---------------------------------------------------------------------------

struct AddressableLatch {
    bits: [bool; 8],
}

impl AddressableLatch {
    fn new() -> Self {
        Self { bits: [false; 8] }
    }

    fn write(&mut self, address: u8, data: bool) {
        let idx = (address & 0x07) as usize;
        self.bits[idx] = data;
    }

    fn screen_size_bits(&self) -> u8 {
        (if self.bits[5] { 2 } else { 0 }) | (if self.bits[4] { 1 } else { 0 })
    }
}

// ---------------------------------------------------------------------------
// BBC Micro Bus
// ---------------------------------------------------------------------------

/// BBC Micro bus.
pub struct BbcBus {
    /// 32 KB RAM.
    pub ram: [u8; 32768],
    /// MOS ROM (16 KB at $C000-$FFFF).
    pub mos_rom: Vec<u8>,
    /// Sideways ROM banks (up to 16, 16 KB each).
    pub sideways_roms: Vec<Vec<u8>>,
    /// Selected sideways ROM bank.
    pub rom_bank: u8,
    /// 6845 CRTC.
    pub crtc: Crtc6845,
    /// Video ULA.
    video_ula: VideoUla,
    /// System VIA (6522 at $FE40).
    pub system_via: Via6522,
    /// User VIA (6522 at $FE60).
    pub user_via: Via6522,
    /// SN76489 PSG.
    pub psg: Sn76489,
    /// Addressable latch (IC32, driven by System VIA Port B).
    latch: AddressableLatch,
    /// Keyboard matrix (10 columns × 8 rows), active high (1 = pressed).
    pub keyboard: [[bool; 8]; 10],
    /// Framebuffer (640×256 ARGB32).
    pub framebuffer: Vec<u32>,
    /// Scanline counter for rendering.
    scanline: u16,
}

impl BbcBus {
    /// Create a new BBC Micro bus with MOS ROM.
    #[must_use]
    pub fn new(mos_rom: Vec<u8>) -> Self {
        Self {
            ram: [0; 32768],
            mos_rom,
            sideways_roms: Vec::new(),
            rom_bank: 0,
            crtc: Crtc6845::new(),
            video_ula: VideoUla::new(),
            system_via: Via6522::new(),
            user_via: Via6522::new(),
            psg: Sn76489::new(4_000_000),
            latch: AddressableLatch::new(),
            keyboard: [[false; 8]; 10],
            framebuffer: vec![0; (FB_WIDTH * FB_HEIGHT) as usize],
            scanline: 0,
        }
    }

    /// Insert a sideways ROM into the given bank slot.
    pub fn insert_rom(&mut self, bank: usize, rom: Vec<u8>) {
        while self.sideways_roms.len() <= bank {
            self.sideways_roms.push(Vec::new());
        }
        self.sideways_roms[bank] = rom;
    }

    /// Render one scanline from CRTC state + Video ULA.
    fn render_scanline(&mut self) {
        if self.scanline >= FB_HEIGHT as u16 {
            return;
        }

        let line = self.scanline as usize;
        let offset = line * FB_WIDTH as usize;
        let bpp = self.video_ula.bpp();
        let pixels_per_byte: usize = 8 / bpp as usize;
        let chars_per_line: usize = if self.video_ula.fast_clock() { 80 } else { 40 };
        let pixel_width: usize = FB_WIDTH as usize / (chars_per_line * pixels_per_byte);

        if self.video_ula.teletext() {
            // MODE 7: teletext placeholder (filled with backdrop)
            self.framebuffer[offset..offset + FB_WIDTH as usize].fill(0xFF00_0000);
            self.scanline += 1;
            return;
        }

        // Determine screen RAM base from CRTC start address
        let crtc_start = self.crtc.start_address() as usize;
        let ra = (self.scanline as usize % 8) as usize;
        let char_row = self.scanline as usize / 8;

        for col in 0..chars_per_line {
            // BBC address translation: MA bits form upper address, RA forms bits 0-2
            let ma = crtc_start + char_row * chars_per_line + col;
            let ram_addr = ((ma & 0x3FFF) << 3) | ra;
            let byte = if ram_addr < 0x8000 {
                self.ram[ram_addr]
            } else {
                0
            };

            // Deserialise pixels
            for px in 0..pixels_per_byte {
                let color_idx = match bpp {
                    1 => (byte >> (7 - px)) & 0x01,
                    2 => {
                        let bit_h = (byte >> (7 - px)) & 0x01;
                        let bit_l = (byte >> (3 - px)) & 0x01;
                        (bit_h << 1) | bit_l
                    }
                    4 => {
                        let b7 = (byte >> (7 - (px & 1))) & 0x01;
                        let b5 = (byte >> (5 - (px & 1))) & 0x01;
                        let b3 = (byte >> (3 - (px & 1))) & 0x01;
                        let b1 = (byte >> (1 - (px & 1))) & 0x01;
                        (b7 << 3) | (b5 << 2) | (b3 << 1) | b1
                    }
                    _ => 0,
                };

                let argb = self.video_ula.palette_to_argb(color_idx);
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
}

impl Bus for BbcBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;
        let data = match addr {
            // RAM
            0x0000..=0x7FFF => self.ram[addr as usize],
            // Sideways ROM
            0x8000..=0xBFFF => {
                let bank = self.rom_bank as usize;
                let offset = (addr - 0x8000) as usize;
                self.sideways_roms
                    .get(bank)
                    .and_then(|rom| rom.get(offset).copied())
                    .unwrap_or(0xFF)
            }
            // SHEILA
            0xFE00..=0xFE07 if addr & 1 == 1 => self.crtc.read_data(),
            0xFE40..=0xFE4F => self.system_via.read((addr & 0x0F) as u8),
            0xFE60..=0xFE6F => self.user_via.read((addr & 0x0F) as u8),
            0xFC00..=0xFEFF => 0xFF, // Other SHEILA/FRED/JIM
            // MOS ROM
            0xC000..=0xFFFF => {
                let offset = (addr - 0xC000) as usize;
                self.mos_rom.get(offset).copied().unwrap_or(0xFF)
            }
        };
        ReadResult::new(data)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr = addr as u16;
        match addr {
            // RAM
            0x0000..=0x7FFF => self.ram[addr as usize] = value,
            // SHEILA
            0xFE00..=0xFE07 if addr & 1 == 0 => self.crtc.write_address(value),
            0xFE00..=0xFE07 if addr & 1 == 1 => self.crtc.write_data(value),
            0xFE20 => self.video_ula.write_control(value),
            0xFE21 => self.video_ula.write_palette(value),
            0xFE30 => self.rom_bank = value & 0x0F,
            0xFE40..=0xFE4F => {
                self.system_via.write((addr & 0x0F) as u8, value);
                // Handle addressable latch writes via Port B
                if addr & 0x0F == 0 {
                    // Port B write: bits 0-2 = latch address, bit 3 = data
                    let latch_addr = value & 0x07;
                    let latch_data = value & 0x08 != 0;
                    self.latch.write(latch_addr, latch_data);

                    // Latch bit 0 = SN76489 /WE
                    if latch_addr == 0 && !latch_data {
                        // Write Port A value to PSG
                        let psg_data = self.system_via.read(0x0F); // ORA no handshake
                        self.psg.write(psg_data);
                    }
                }
            }
            0xFE60..=0xFE6F => self.user_via.write((addr & 0x0F) as u8, value),
            _ => {}
        }
        0
    }

    fn io_read(&mut self, _port: u32) -> ReadResult {
        ReadResult::new(0xFF) // 6502 has no separate I/O space
    }

    fn io_write(&mut self, _port: u32, _value: u8) -> u8 {
        0
    }
}

// ---------------------------------------------------------------------------
// BBC Micro system
// ---------------------------------------------------------------------------

/// BBC Micro Model B.
pub struct BbcMicro {
    cpu: Mos6502,
    bus: BbcBus,
    master_clock: u64,
    frame_count: u64,
}

impl BbcMicro {
    /// Create a new BBC Micro with MOS ROM.
    #[must_use]
    pub fn new(mos_rom: Vec<u8>) -> Self {
        let bus = BbcBus::new(mos_rom);
        let mut cpu = Mos6502::new();

        // Read reset vector from MOS ROM
        let reset_lo = bus.mos_rom.get(0x3FFC).copied().unwrap_or(0);
        let reset_hi = bus.mos_rom.get(0x3FFD).copied().unwrap_or(0);
        cpu.regs.pc = u16::from(reset_lo) | (u16::from(reset_hi) << 8);

        Self { cpu, bus, master_clock: 0, frame_count: 0 }
    }

    /// Insert a sideways ROM (BASIC, DFS, etc.) into a bank slot.
    pub fn insert_rom(&mut self, bank: usize, rom: Vec<u8>) {
        self.bus.insert_rom(bank, rom);
    }

    /// Run one complete frame (~312 scanlines).
    pub fn run_frame(&mut self) {
        let target = self.master_clock + CYCLES_PER_FRAME;
        self.bus.scanline = 0;

        // Run scanlines
        let crtc_chars_per_line: u64 = if self.bus.video_ula.fast_clock() { 128 } else { 64 };
        let scanlines = 312u16;

        for line in 0..scanlines {
            // Tick CRTC for one scanline
            for _ in 0..crtc_chars_per_line {
                self.bus.crtc.tick();
            }

            // Render if in visible area
            if line < FB_HEIGHT as u16 {
                self.bus.render_scanline();
            }

            // CPU: 128 ticks per line at 2 MHz (64 µs × 2 MHz)
            for _ in 0..128 {
                self.cpu.tick(&mut self.bus);
                self.bus.psg.tick();
            }

            // VSYNC → System VIA CA1 interrupt
            if self.bus.crtc.vsync {
                // System VIA CA1 would trigger here
            }
        }

        self.master_clock = target;
        self.frame_count += 1;
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.bus.framebuffer
    }

    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.bus.psg.take_buffer()
    }

    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    pub fn cpu_mut(&mut self) -> &mut Mos6502 {
        &mut self.cpu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_mos() -> Vec<u8> {
        // Minimal MOS ROM: reset vector points to $C000, code = DI; JR -2
        let mut rom = vec![0xFF_u8; 16384];
        rom[0] = 0x78; // SEI
        rom[1] = 0x4C; // JMP
        rom[2] = 0x00; // $C000 low
        rom[3] = 0xC0; // $C000 high
        // Reset vector at $FFFC ($C000-relative = $3FFC)
        rom[0x3FFC] = 0x00;
        rom[0x3FFD] = 0xC0;
        rom
    }

    #[test]
    fn boots_and_runs_frame() {
        let mut bbc = BbcMicro::new(minimal_mos());
        bbc.run_frame();
        assert_eq!(bbc.frame_count(), 1);
    }

    #[test]
    fn ram_read_write() {
        let mut bus = BbcBus::new(minimal_mos());
        bus.write(0x1000, 0xAB);
        assert_eq!(bus.read(0x1000).data, 0xAB);
    }

    #[test]
    fn mos_rom_accessible() {
        let mos = minimal_mos();
        let mut bus = BbcBus::new(mos);
        assert_eq!(bus.read(0xC000).data, 0x78); // SEI
    }

    #[test]
    fn sideways_rom_banking() {
        let mut bus = BbcBus::new(minimal_mos());
        let mut basic = vec![0u8; 16384];
        basic[0] = 0xBB;
        bus.insert_rom(15, basic);

        bus.write(0xFE30, 15); // Select bank 15
        assert_eq!(bus.read(0x8000).data, 0xBB);

        bus.write(0xFE30, 0); // Select empty bank
        assert_eq!(bus.read(0x8000).data, 0xFF);
    }

    #[test]
    fn crtc_register_access() {
        let mut bus = BbcBus::new(minimal_mos());
        // Write R1 = 80
        bus.write(0xFE00, 1); // Select register 1
        bus.write(0xFE01, 80); // R1 = 80
        assert_eq!(bus.crtc.regs()[1], 80);
    }

    #[test]
    fn video_ula_palette() {
        let mut bus = BbcBus::new(minimal_mos());
        // Set logical colour 0 to physical white ($07 → inverted = black... wait)
        // Physical $00 = ~R=0,~G=0,~B=0 → all on = white
        bus.write(0xFE21, 0x00); // Logical 0 = physical 0 (white)
        assert_eq!(bus.video_ula.palette_to_argb(0), 0xFF_FF_FF_FF);
    }
}
