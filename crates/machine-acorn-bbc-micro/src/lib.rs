//! BBC Micro Model B machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at
//! `Emu198x-Oldest/crates/machine-acorn-bbc-micro` used the
//! deprecated `emu_core::Bus` callback and could not port directly;
//! this file uses it as a system spec — SHEILA I/O page at
//! `$FE00-$FEFF` with 6845 CRTC, Video ULA, ROM bank register,
//! System VIA, User VIA; sideways ROM slot at `$8000-$BFFF`;
//! addressable latch IC32 driven via System VIA port B; SN76489
//! PSG fed via the System VIA + latch — but the wiring is written
//! against `mos-6502`'s public pin fields.
//!
//! # The BBC Micro Model B
//!
//! The BBC Micro (1981) by Acorn Computers is one of the most
//! influential educational computers ever made. Designed in
//! response to the BBC's Computer Literacy Project, it became the
//! UK education-and-home-computing standard for the 1980s.
//!
//! - **CPU:** 6502A @ 2 MHz
//! - **CRTC:** Motorola 6845
//! - **Video ULA:** Acorn custom (256-colour-pool→16-entry palette,
//!   bpp + fast-clock selection)
//! - **PSG:** SN76489 @ 4 MHz, fed via System VIA + addressable
//!   latch IC32
//! - **VIAs:** Two MOS 6522s — System VIA at `$FE40` (sound,
//!   keyboard, IC32) and User VIA at `$FE60` (Centronics, user port)
//! - **RAM:** 32 KB at `$0000-$7FFF`
//! - **MOS ROM:** 16 KB at `$C000-$FFFF`
//! - **Sideways ROMs:** 16 banks × 16 KB at `$8000-$BFFF`, banked
//!   by `$FE30`
//!
//! # Memory map
//!
//! | Range         | Contents                                       |
//! |---------------|------------------------------------------------|
//! | `$0000-$7FFF` | 32 KB RAM                                      |
//! | `$8000-$BFFF` | Sideways ROM slot (banked via `$FE30`)         |
//! | `$C000-$FBFF` | MOS ROM                                        |
//! | `$FC00-$FCFF` | FRED — 1 MHz expansion                         |
//! | `$FD00-$FDFF` | JIM — 1 MHz expansion                          |
//! | `$FE00-$FEFF` | SHEILA — internal I/O (see below)              |
//! | `$FF00-$FFFF` | MOS ROM (reset / IRQ / NMI vectors)            |
//!
//! ## SHEILA register map
//!
//! | Range         | Device                                           |
//! |---------------|--------------------------------------------------|
//! | `$FE00`/`02`  | 6845 CRTC address register                       |
//! | `$FE01`/`03`  | 6845 CRTC data register                          |
//! | `$FE20`       | Video ULA control                                |
//! | `$FE21`       | Video ULA palette write                          |
//! | `$FE30`       | Sideways ROM bank select                         |
//! | `$FE40-$FE4F` | System VIA                                       |
//! | `$FE60-$FE6F` | User VIA                                         |
//!
//! # PSG path
//!
//! The SN76489 is not directly memory-mapped. The CPU writes the
//! PSG byte into System VIA port A (ORA register `$01`/`$0F`), then
//! flips IC32 latch bit 0 (the SN76489 `/WE`) via a System VIA
//! port B write. When bit 0 of the latch transitions low, the
//! current ORA value is latched into the PSG.

use mos_6502::M6502;
use mos_via_6522::Via6522;
use motorola_6845::Crtc6845;
use ti_sn76489::Sn76489;

/// Framebuffer width (640 pixels — MODE 0 native).
pub const FB_WIDTH: u32 = 640;
/// Framebuffer height (256 pixels visible per PAL frame).
pub const FB_HEIGHT: u32 = 256;

const CPU_CLOCK_HZ: u32 = 2_000_000;
const CYCLES_PER_FRAME: u64 = 39_936; // 312 lines × 64 µs × 2 MHz
const SCANLINES_PER_FRAME: u16 = 312;
const CYCLES_PER_LINE: u64 = 128;

const SN76489_CLOCK_HZ: u32 = 4_000_000;

/// Video ULA — palette + control register.
struct VideoUla {
    control: u8,
    palette: [u8; 16],
}

impl VideoUla {
    fn new() -> Self {
        // Default palette: identity with inverted physical bits.
        let mut palette = [0u8; 16];
        for (i, slot) in palette.iter_mut().enumerate() {
            *slot = (i as u8) ^ 0x07;
        }
        Self {
            control: 0,
            palette,
        }
    }

    fn write_control(&mut self, value: u8) {
        self.control = value;
    }

    fn write_palette(&mut self, value: u8) {
        let logical = (value >> 4) as usize;
        let physical = value & 0x0F;
        self.palette[logical] = physical;
    }

    fn bpp(&self) -> u8 {
        match (self.control >> 2) & 0x03 {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => 1,
        }
    }

    fn teletext(&self) -> bool {
        self.control & 0x02 != 0
    }

    fn fast_clock(&self) -> bool {
        self.control & 0x10 != 0
    }

    fn palette_to_argb(&self, index: u8) -> u32 {
        let entry = self.palette[index as usize & 0x0F];
        // Physical colour: bits 0-2 = ~R, ~G, ~B (active-low).
        let r = if entry & 0x01 == 0 { 255 } else { 0 };
        let g = if entry & 0x02 == 0 { 255 } else { 0 };
        let b = if entry & 0x04 == 0 { 255 } else { 0 };
        0xFF00_0000 | (r << 16) | (g << 8) | b
    }
}

/// IC32 addressable latch — System VIA port B writes encode
/// `address = value & 0x07` and `data = (value >> 3) & 1`.
struct AddressableLatch {
    bits: [bool; 8],
}

impl AddressableLatch {
    fn new() -> Self {
        Self { bits: [false; 8] }
    }

    fn write(&mut self, address: u8, data: bool) -> Option<u8> {
        let idx = (address & 0x07) as usize;
        let prev = self.bits[idx];
        self.bits[idx] = data;
        if idx == 0 && prev && !data {
            // Bit 0 falling edge = SN76489 /WE asserted (write PSG).
            Some(0)
        } else {
            None
        }
    }
}

/// BBC Micro Model B machine.
pub struct BbcMicro {
    cpu: M6502,
    crtc: Crtc6845,
    video_ula: VideoUla,
    system_via: Via6522,
    user_via: Via6522,
    psg: Sn76489,
    ram: [u8; 32768],
    mos_rom: Vec<u8>,
    sideways_roms: Vec<Vec<u8>>,
    rom_bank: u8,
    latch: AddressableLatch,
    /// Keyboard matrix (10 columns × 8 rows), active-high.
    keyboard: [[bool; 8]; 10],
    framebuffer: Vec<u32>,
    cpu_cycles: u64,
    frame_count: u64,
}

impl BbcMicro {
    /// Create a new BBC Micro with the 16 KB MOS ROM. Sideways ROMs
    /// start empty; use [`Self::insert_rom`] to install BASIC, DFS,
    /// etc. into specific bank slots.
    #[must_use]
    pub fn new(mos_rom: Vec<u8>) -> Self {
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
            crtc: Crtc6845::new(),
            video_ula: VideoUla::new(),
            system_via: Via6522::new(),
            user_via: Via6522::new(),
            psg: Sn76489::new(SN76489_CLOCK_HZ),
            ram: [0; 32768],
            mos_rom,
            sideways_roms: Vec::new(),
            rom_bank: 0,
            latch: AddressableLatch::new(),
            keyboard: [[false; 8]; 10],
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            cpu_cycles: 0,
            frame_count: 0,
        }
    }

    /// Install a sideways ROM into the given bank slot (0-15).
    pub fn insert_rom(&mut self, bank: usize, rom: Vec<u8>) {
        while self.sideways_roms.len() <= bank {
            self.sideways_roms.push(Vec::new());
        }
        self.sideways_roms[bank] = rom;
    }

    /// Run one PAL frame.
    pub fn run_frame(&mut self) -> u64 {
        for line in 0..SCANLINES_PER_FRAME {
            // Run CPU + chips for one scanline.
            for _ in 0..CYCLES_PER_LINE {
                self.tick_cpu_cycle();
            }
            // Render visible scanlines.
            if line < FB_HEIGHT as u16 {
                self.render_scanline(line as usize);
            }
            // VSYNC pulse during a sensible window — System VIA CA1
            // is wired to the CRTC's VSYNC. Drive the level so the
            // VIA's edge detector latches the interrupt.
            self.system_via.set_ca1_level(!self.crtc.vsync);
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
        self.crtc.tick();
        self.system_via.tick();
        self.user_via.tick();
        self.psg.tick();
        self.cpu.irq = self.system_via.irq || self.user_via.irq;
        self.cpu_cycles += 1;
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0xBFFF => self
                .sideways_roms
                .get(self.rom_bank as usize)
                .and_then(|rom| rom.get((addr - 0x8000) as usize).copied())
                .unwrap_or(0xFF),
            0xFE00..=0xFE07 if addr & 1 == 1 => self.crtc.read_data(),
            0xFE40..=0xFE4F => self.system_via.read((addr & 0x0F) as u8),
            0xFE60..=0xFE6F => self.user_via.read((addr & 0x0F) as u8),
            0xFC00..=0xFEFF => 0xFF,
            0xC000..=0xFFFF => self
                .mos_rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize] = value,
            0xFE00..=0xFE07 if addr & 1 == 0 => self.crtc.write_address(value),
            0xFE00..=0xFE07 if addr & 1 == 1 => self.crtc.write_data(value),
            0xFE20 => self.video_ula.write_control(value),
            0xFE21 => self.video_ula.write_palette(value),
            0xFE30 => self.rom_bank = value & 0x0F,
            0xFE40..=0xFE4F => {
                let reg = (addr & 0x0F) as u8;
                self.system_via.write(reg, value);
                // System VIA port B carries the IC32 addressable
                // latch encoding: low 3 bits = address, bit 3 = data.
                if reg == 0x00 {
                    let latch_addr = value & 0x07;
                    let latch_data = value & 0x08 != 0;
                    if let Some(()) = self.latch.write(latch_addr, latch_data).map(|_| ()) {
                        // SN76489 /WE asserted — latch the byte on
                        // ORA into the PSG.
                        self.psg.write(self.system_via.ora());
                    }
                }
            }
            0xFE60..=0xFE6F => self.user_via.write((addr & 0x0F) as u8, value),
            _ => {}
        }
    }

    fn render_scanline(&mut self, line: usize) {
        let offset = line * FB_WIDTH as usize;
        if self.video_ula.teletext() {
            // MODE 7 / teletext placeholder — backdrop fill. SAA5050
            // is a separate chip and isn't modelled here yet.
            self.framebuffer[offset..offset + FB_WIDTH as usize].fill(0xFF00_0000);
            return;
        }
        let bpp = self.video_ula.bpp() as usize;
        let pixels_per_byte = 8 / bpp;
        let chars_per_line = if self.video_ula.fast_clock() { 80 } else { 40 };
        let pixel_width = FB_WIDTH as usize / (chars_per_line * pixels_per_byte);
        let crtc_start = self.crtc.start_address() as usize;
        let ra = line % 8;
        let char_row = line / 8;
        for col in 0..chars_per_line {
            let ma = crtc_start + char_row * chars_per_line + col;
            let ram_addr = ((ma & 0x3FFF) << 3) | ra;
            let byte = if ram_addr < 0x8000 { self.ram[ram_addr] } else { 0 };
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
                let argb = self.video_ula.palette_to_argb(colour_idx);
                let fb_x = (col * pixels_per_byte + px) * pixel_width;
                for w in 0..pixel_width {
                    if fb_x + w < FB_WIDTH as usize {
                        self.framebuffer[offset + fb_x + w] = argb;
                    }
                }
            }
        }
    }

    /// Framebuffer (640×256 ARGB32).
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

    /// Take the PSG audio buffer.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.psg.take_buffer()
    }

    /// Press a key at the given (column, row).
    pub fn press_key(&mut self, col: usize, row: usize) {
        if col < 10 && row < 8 {
            self.keyboard[col][row] = true;
        }
    }

    /// Release a key at the given (column, row).
    pub fn release_key(&mut self, col: usize, row: usize) {
        if col < 10 && row < 8 {
            self.keyboard[col][row] = false;
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

    /// CRTC reference.
    #[must_use]
    pub fn crtc(&self) -> &Crtc6845 {
        &self.crtc
    }

    /// Current ROM bank (0-15).
    #[must_use]
    pub fn rom_bank(&self) -> u8 {
        self.rom_bank
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// CPU cycles since power-on.
    #[must_use]
    pub fn cpu_cycles(&self) -> u64 {
        self.cpu_cycles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        // 16 KB MOS ROM with JMP self at $C000 + reset / IRQ / NMI
        // vectors pointing there.
        let mut rom = vec![0xEA_u8; 0x4000];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0xC0;
        rom[0x3FFA] = 0x00;
        rom[0x3FFB] = 0xC0;
        rom[0x3FFC] = 0x00;
        rom[0x3FFD] = 0xC0;
        rom[0x3FFE] = 0x00;
        rom[0x3FFF] = 0xC0;
        rom
    }

    #[test]
    fn frame_runs_expected_cycles() {
        let mut sys = BbcMicro::new(trap_rom());
        let t = sys.run_frame();
        assert_eq!(t, CYCLES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = BbcMicro::new(trap_rom());
        for _ in 0..10 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 10);
    }

    #[test]
    fn memory_map_routes_pages() {
        let mut rom = trap_rom();
        rom[0x0100] = 0x99;
        let mut sys = BbcMicro::new(rom);
        sys.insert_rom(0, vec![0x77; 0x4000]);
        // MOS at $C000.
        assert_eq!(sys.mem_read(0xC100), 0x99);
        // Sideways ROM at $8000.
        assert_eq!(sys.mem_read(0x8000), 0x77);
        // RAM round-trip.
        sys.mem_write(0x4000, 0x42);
        assert_eq!(sys.mem_read(0x4000), 0x42);
        // ROM writes ignored.
        sys.mem_write(0xC100, 0x00);
        assert_eq!(sys.mem_read(0xC100), 0x99);
    }

    #[test]
    fn rom_bank_register_at_fe30() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.insert_rom(0, vec![0xAA; 0x4000]);
        sys.insert_rom(7, vec![0xBB; 0x4000]);
        sys.mem_write(0xFE30, 7);
        assert_eq!(sys.rom_bank(), 7);
        assert_eq!(sys.mem_read(0x8000), 0xBB);
        sys.mem_write(0xFE30, 0);
        assert_eq!(sys.mem_read(0x8000), 0xAA);
    }

    #[test]
    fn video_ula_palette_write_decodes_logical_and_physical() {
        let mut sys = BbcMicro::new(trap_rom());
        // Set logical entry 5 to physical 3 ($53 → logical=5, phys=3).
        sys.mem_write(0xFE21, 0x53);
        assert_eq!(sys.video_ula.palette[5], 3);
    }

    #[test]
    fn video_ula_control_sets_bpp_and_fast_clock() {
        let mut sys = BbcMicro::new(trap_rom());
        // bits 3-2 = 10 (4 bpp), bit 4 = 1 (fast 80-col clock).
        sys.mem_write(0xFE20, 0b0001_1000);
        assert_eq!(sys.video_ula.bpp(), 4);
        assert!(sys.video_ula.fast_clock());
    }

    #[test]
    fn system_via_writes_round_trip() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.mem_write(0xFE43, 0xFF); // DDRA
        sys.mem_write(0xFE41, 0x77); // ORA
        assert_eq!(sys.system_via.ora(), 0x77);
    }

    #[test]
    fn ic32_falling_edge_on_bit_0_writes_psg() {
        let mut sys = BbcMicro::new(trap_rom());
        // Set ORA = $80 (PSG tone latch byte for ch0).
        sys.mem_write(0xFE43, 0xFF);
        sys.mem_write(0xFE41, 0x80);
        // Raise latch bit 0 (write port B with addr=0, data=1).
        sys.mem_write(0xFE40, 0b0000_1000);
        // Drop latch bit 0 — should latch ORA into PSG.
        sys.mem_write(0xFE40, 0b0000_0000);
        // PSG sweep / mute behaviour is verified inside ti-sn76489;
        // here we just confirm the write path didn't panic.
    }
}
