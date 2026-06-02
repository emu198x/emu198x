//! Sord M5 machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-sord-m5` used
//! the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — memory map, I/O
//! port routing, keyboard matrix, CTC stub — but the wiring is
//! written against [`zilog_z80::Z80`]'s public pin fields and
//! `bus_request()` collapse.
//!
//! # The Sord M5
//!
//! The M5 (1982) is a Japanese home computer from Sord Computer
//! Corporation, sold in Europe through Takara. Same TMS9918A + SN76489
//! pair as ColecoVision / SG-1000 / MSX, but with a built-in keyboard,
//! an 8 KB monitor / BASIC-I ROM, and a unique cart layout that places
//! the cart at `$2000-$6FFF` rather than the Coleco / SG-1000 ceiling
//! at `$8000-$FFFF` / `$0000-$BFFF`.
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **VDP:** TMS9918A (16 KB VRAM)
//! - **PSG:** SN76489A
//! - **CTC:** Z80 CTC (channel 3 wired to VDP INT on real hardware;
//!   modelled as a write-stub on this initial port — most early M5
//!   software doesn't use it heavily)
//! - **RAM:** 4 KB at `$7000-$7FFF`; optional cart RAM at
//!   `$8000-$BFFF` (up to 16 KB, FALC etc.)
//! - **Monitor ROM:** 8 KB at `$0000-$1FFF`
//! - **Cartridge:** up to 20 KB at `$2000-$6FFF`
//!
//! # Memory map
//!
//! | Range         | Contents                              |
//! |---------------|---------------------------------------|
//! | `$0000-$1FFF` | Monitor / BASIC-I ROM (8 KB)          |
//! | `$2000-$6FFF` | Cartridge ROM (up to 20 KB)           |
//! | `$7000-$7FFF` | 4 KB system RAM                       |
//! | `$8000-$BFFF` | Optional cart RAM (up to 16 KB)       |
//! | `$C000-$FFFF` | Unmapped (returns `$FF`)              |
//!
//! # I/O map
//!
//! Decoded by `port & 0xF8` — each block fills 8 mirrored ports.
//!
//! | Port block | R/W   | Function                                |
//! |------------|-------|-----------------------------------------|
//! | `$00-$07`  | r/w   | TMS9918A (even=data, odd=control)       |
//! | `$10-$17`  | write | SN76489 PSG                             |
//! | `$20-$27`  | read  | Keyboard column for the selected row    |
//! | `$30-$37`  | write | Keyboard row strobe (bits 0-3 select)   |
//! | `$50`      | write | Z80 CTC stub                            |
//!
//! # Keyboard
//!
//! 10 rows × 8 columns matrix, active-low. The CPU writes the row
//! index to port `$30-$37` then reads the column data from
//! `$20-$27`. Standard M5 layout (function keys + alpha rows +
//! shift/ctrl).
//!
//! # Clock model
//!
//! Adopts SG-1000's correct 3:2 VDP-dot-to-CPU-T-state phase counter
//! (CPU 3.579545 MHz, VDP dot 5.369 MHz). One iteration corresponds
//! to one Z80 T-state; per iteration the phase counter advances by 3
//! and yields one VDP dot whenever it reaches 2.

use ti_sn76489::Sn76489;
use ti_tms9918::{Tms9918, VdpRegion};
use zilog_z80::{BusOp, Z80};

const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
const VDP_DOT_PHASE_DENOMINATOR: u32 = 2;
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const NTSC_PSG_CLOCK_HZ: u32 = 3_579_545;
const PAL_PSG_CLOCK_HZ: u32 = 3_546_893;

/// Number of keyboard matrix rows on the M5.
pub const NUM_KEY_ROWS: usize = 10;

/// Sord M5 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum M5Region {
    Ntsc,
    Pal,
}

/// Sord M5 machine.
pub struct SordM5 {
    cpu: Z80,
    vdp: Tms9918,
    psg: Sn76489,
    rom: Vec<u8>,
    cart_rom: Vec<u8>,
    cart_ram: Vec<u8>,
    ram: [u8; 4096],
    /// 10×8 keyboard matrix, active-low (1 = released).
    key_matrix: [u8; NUM_KEY_ROWS],
    /// Last value written to `$30-$37`; bits 0-3 select the matrix
    /// row that the next `$20-$27` read returns.
    key_row: u8,
    /// CTC stub register (initial port — channel 3 not wired).
    ctc_reg: u8,
    region: M5Region,
    cpu_tstates: u64,
    tstates_per_frame: u64,
    vdp_phase: u32,
    frame_count: u64,
}

impl SordM5 {
    /// Create a new Sord M5 with the given Monitor / BASIC-I ROM and
    /// optional cart ROM. Cart RAM starts empty; call
    /// [`SordM5::set_cart_ram_size`] to allocate it.
    #[must_use]
    pub fn new(rom: Vec<u8>, cart_rom: Vec<u8>, region: M5Region) -> Self {
        let vdp_region = match region {
            M5Region::Ntsc => VdpRegion::Ntsc,
            M5Region::Pal => VdpRegion::Pal,
        };
        let psg_clock_hz = match region {
            M5Region::Ntsc => NTSC_PSG_CLOCK_HZ,
            M5Region::Pal => PAL_PSG_CLOCK_HZ,
        };
        let tstates_per_frame = match region {
            M5Region::Ntsc => NTSC_TSTATES_PER_FRAME,
            M5Region::Pal => PAL_TSTATES_PER_FRAME,
        };
        Self {
            cpu: Z80::new(),
            vdp: Tms9918::new(vdp_region),
            psg: Sn76489::new(psg_clock_hz),
            rom,
            cart_rom,
            cart_ram: Vec::new(),
            ram: [0; 4096],
            key_matrix: [0xFF; NUM_KEY_ROWS],
            key_row: 0,
            ctc_reg: 0,
            region,
            cpu_tstates: 0,
            tstates_per_frame,
            vdp_phase: 0,
            frame_count: 0,
        }
    }

    /// Allocate (or resize) cart RAM at `$8000-$BFFF`. Some carts
    /// (FALC and friends) need extra RAM beyond the built-in 4 KB.
    pub fn set_cart_ram_size(&mut self, bytes: usize) {
        let capped = bytes.min(0x4000);
        self.cart_ram.resize(capped, 0);
    }

    /// Run one frame and return T-states consumed.
    pub fn run_frame(&mut self) -> u64 {
        let target = self.cpu_tstates + self.tstates_per_frame;
        while self.cpu_tstates < target {
            self.tick_tstate();
        }
        self.frame_count += 1;
        self.tstates_per_frame
    }

    fn tick_tstate(&mut self) {
        self.cpu.tick();
        self.handle_bus();

        self.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
        while self.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
            self.vdp.tick();
            self.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
        }

        self.psg.tick();

        // VDP INT → Z80 IRQ (on real hardware via Z80 CTC channel 3,
        // but the visible effect at this initial-port level is the
        // same level-driven IRQ pin).
        self.cpu.irq = self.vdp.interrupt;

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
                // **Known incomplete.** The Monitor ROM sets IM 2 with
                // `I = $70` and expects the Z80 CTC channel that
                // receives VDP /INT to deliver its programmed vector
                // byte. The CTC's vector base and channel-VDP wiring
                // are configured by the BIOS during early init.
                //
                // The IM 2 vector table at `$7000-$7007` (copied by
                // the BIOS from ROM `$0165`) holds:
                //   `$7000 -> $186C` (no-op `EI; RETI`)
                //   `$7002 -> $1861` (VBlank: dec jiffy counter)
                //   `$7004 -> $186C`
                //   `$7006 -> $01DF` (cassette / keyboard handler)
                //
                // Without a proper Z80 CTC chip emulation (counters,
                // control-register decode, channel-specific vector
                // generation off VDP /INT clock pulses), this initial
                // port returns `$FF` and the BIOS init loop never
                // advances past VDP setup — Dig Dug and other carts
                // stay on a black screen.
                //
                // Tracked in docs/status/outstanding-work.md
                // § Sord M5 as a `zilog-z80-ctc` chip-crate prereq.
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            0x2000..=0x6FFF => self
                .cart_rom
                .get((addr - 0x2000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0x7000..=0x7FFF => self.ram[(addr & 0x0FFF) as usize],
            0x8000..=0xBFFF => self
                .cart_ram
                .get((addr - 0x8000) as usize)
                .copied()
                .unwrap_or(0xFF),
            _ => 0xFF,
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x7000..=0x7FFF => self.ram[(addr & 0x0FFF) as usize] = value,
            0x8000..=0xBFFF => {
                let idx = (addr - 0x8000) as usize;
                if let Some(slot) = self.cart_ram.get_mut(idx) {
                    *slot = value;
                }
            }
            _ => {}
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let p = port as u8;
        match p & 0xF8 {
            0x00 => {
                if p & 1 == 0 {
                    self.vdp.read_data()
                } else {
                    self.vdp.read_status()
                }
            }
            0x20 => {
                let row = (self.key_row & 0x0F) as usize;
                if row < NUM_KEY_ROWS {
                    self.key_matrix[row]
                } else {
                    0xFF
                }
            }
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, value: u8) {
        let p = port as u8;
        match p & 0xF8 {
            0x00 => {
                if p & 1 == 0 {
                    self.vdp.write_data(value);
                } else {
                    self.vdp.write_control(value);
                }
            }
            0x10 => self.psg.write(value),
            0x30 => self.key_row = value & 0x0F,
            0x50 => self.ctc_reg = value,
            _ => {}
        }
    }

    /// Framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vdp.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vdp.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vdp.framebuffer_height()
    }

    /// Observe one byte on the Z80 bus without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Take the accumulated PSG audio buffer.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.psg.take_buffer()
    }

    /// Press a key at the given matrix (row, bit) cell.
    pub fn press_key(&mut self, row: usize, bit: u8) {
        if row < self.key_matrix.len() && bit < 8 {
            self.key_matrix[row] &= !(1 << bit);
        }
    }

    /// Release a key at the given matrix (row, bit) cell.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        if row < self.key_matrix.len() && bit < 8 {
            self.key_matrix[row] |= 1 << bit;
        }
    }

    /// Mutable keyboard matrix (active-low; 0 = pressed).
    pub fn key_matrix_mut(&mut self) -> &mut [u8; NUM_KEY_ROWS] {
        &mut self.key_matrix
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

    /// VDP reference.
    #[must_use]
    pub fn vdp(&self) -> &Tms9918 {
        &self.vdp
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> M5Region {
        self.region
    }

    /// CPU T-states executed since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.cpu_tstates
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        // 8 KB Monitor-ROM stub: NOPs with a JR -2 trap at $0008.
        let mut rom = vec![0u8; 8192];
        rom[0x0008] = 0x18;
        rom[0x0009] = 0xFE;
        rom
    }

    #[test]
    fn ntsc_frame_returns_expected_tstates() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        let t = sys.run_frame();
        assert_eq!(t, NTSC_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_frame_returns_expected_tstates() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Pal);
        let t = sys.run_frame();
        assert_eq!(t, PAL_TSTATES_PER_FRAME);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }

    #[test]
    fn memory_map_routes_pages() {
        let mut sys = SordM5::new(trap_rom(), vec![0xCAu8; 0x4000], M5Region::Ntsc);
        // BIOS trap byte.
        assert_eq!(sys.mem_read(0x0008), 0x18);
        // Cart ROM mirror.
        assert_eq!(sys.mem_read(0x2000), 0xCA);
        // RAM round-trip.
        sys.mem_write(0x7100, 0x42);
        assert_eq!(sys.mem_read(0x7100), 0x42);
        // Cart RAM round-trip after allocating.
        sys.set_cart_ram_size(0x4000);
        sys.mem_write(0x8000, 0x77);
        assert_eq!(sys.mem_read(0x8000), 0x77);
        // Unmapped region.
        assert_eq!(sys.mem_read(0xC000), 0xFF);
    }

    #[test]
    fn keyboard_row_strobe_and_column_read() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        sys.key_matrix[3] = 0x77;
        // Strobe row 3 then read.
        sys.io_write(0x30, 3);
        assert_eq!(sys.io_read(0x20), 0x77);
        // Different row, different value.
        sys.key_matrix[7] = 0xAA;
        sys.io_write(0x30, 7);
        assert_eq!(sys.io_read(0x20), 0xAA);
    }

    #[test]
    fn vdp_io_routes_at_0x00_block() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        sys.io_write(0x01, 0x80); // VDP control low byte
        sys.io_write(0x01, 0xC0 | 0x00); // VDP control high (write to VRAM addr $0000)
        sys.io_write(0x00, 0x42); // VDP data write
        // Reading status doesn't panic.
        let _ = sys.io_read(0x01);
    }

    #[test]
    fn vdp_dot_ratio_is_three_per_two_tstates() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        let start = sys.vdp.scanline();
        for _ in 0..4 {
            sys.tick_tstate();
        }
        assert_eq!(sys.vdp.scanline(), start);
        assert_eq!(sys.vdp_phase, 0);
    }

    #[test]
    fn key_press_and_release_round_trip() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        sys.press_key(2, 5);
        assert_eq!(sys.key_matrix[2] & 0b0010_0000, 0);
        sys.release_key(2, 5);
        assert_eq!(sys.key_matrix[2] & 0b0010_0000, 0b0010_0000);
    }
}
