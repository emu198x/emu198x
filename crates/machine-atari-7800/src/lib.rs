//! Atari 7800 ProSystem machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). Donor at `Emu198x-Oldest/crates/machine-atari-7800/src/lib.rs`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; the donor is used here as the system spec — 6502C "Sally"
//! address decode, MARIA's zone-based display-list rendering, RIOT for
//! joystick + console switches + timer, TIA-audio register stub — but the
//! wiring is written against [`mos_6502::M6502`]'s public pin fields.
//!
//! # The Atari 7800 ProSystem
//!
//! Released in 1986 (designed in 1984 but delayed when Warner sold Atari
//! to Tramiel). Backward-compatible with the 2600 via the same TIA + RIOT
//! pair; native 7800 games drive MARIA instead — a zone-based display
//! processor that DMAs sprite and tile data from RAM each scanline,
//! freeing the 6502C "Sally" CPU from the 2600's race-the-beam model.
//!
//! - **CPU:** MOS 6502C "Sally" — stock 6502 with Atari's HALT pin for
//!   MARIA DMA cycle stealing.
//! - **MARIA:** display processor (zone-based DLL/DL, palette,
//!   320 × 240 framebuffer). See [`atari_maria`].
//! - **RIOT:** I/O and timer (P0 / P1 joystick + console switches).
//! - **TIA:** audio only in 7800 mode (six registers, stubbed here).
//! - **RAM:** 4 KB main at `$1800-$27FF` (mirrored to `$3FFF`), 192 B
//!   zero-page (`$0040-$00FF`), 192 B stack (`$0140-$01FF`).
//! - **Cart:** up to 128 KB; 16 KB / 32 KB / 48 KB flat or SuperGame
//!   banking. See [`Cartridge`].
//!
//! # Memory map
//!
//! | Range         | Contents                                         |
//! |---------------|--------------------------------------------------|
//! | `$0000-$001F` | TIA (audio only in 7800 mode)                    |
//! | `$0020-$003F` | MARIA registers                                  |
//! | `$0040-$00FF` | Zero-page RAM (192 B)                            |
//! | `$0100-$011F` | TIA mirror                                       |
//! | `$0120-$013F` | MARIA mirror                                     |
//! | `$0140-$01FF` | Stack RAM (192 B)                                |
//! | `$0280-$02FF` | RIOT I/O + timer                                 |
//! | `$1800-$27FF` | Main RAM (4 KB)                                  |
//! | `$2800-$3FFF` | Main RAM mirror                                  |
//! | `$4000-$FFFF` | Cartridge ROM                                    |
//!
//! # Clock model
//!
//! Master clock = colour clock (3.58 MHz NTSC, 3.55 MHz PAL). CPU + RIOT
//! tick every 2nd colour clock = 1.79 MHz NTSC. One scan line is 228
//! colour clocks (114 CPU cycles); MARIA renders one scanline at every
//! boundary and stalls the CPU for the line's DMA budget. WSYNC writes
//! halt the CPU until the next line. DLI fires NMI.

mod cartridge;
mod tia_audio;

pub use cartridge::Cartridge;
pub use tia_audio::TiaAudio;

use atari_maria::{Maria, MariaRegion};
use mos_6502::M6502;
use mos_riot_6532::Riot6532;

const COLOUR_CLOCKS_PER_LINE: u16 = 228;

/// Atari 7800 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atari7800Region {
    Ntsc,
    Pal,
}

impl Atari7800Region {
    fn maria_region(self) -> MariaRegion {
        match self {
            Self::Ntsc => MariaRegion::Ntsc,
            Self::Pal => MariaRegion::Pal,
        }
    }

    fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 263,
            Self::Pal => 313,
        }
    }
}

/// Atari 7800 machine.
pub struct Atari7800 {
    cpu: M6502,
    maria: Maria,
    riot: Riot6532,
    tia_audio: TiaAudio,
    cart: Cartridge,
    ram_zp: [u8; 192],
    ram_stack: [u8; 192],
    ram_main: [u8; 4096],
    region: Atari7800Region,
    master_clock: u64,
    clocks_per_frame: u64,
    frame_count: u64,
    dma_budget: u8,
    line_cycle: u16,
}

impl Atari7800 {
    pub fn new(rom: Vec<u8>, region: Atari7800Region) -> Result<Self, String> {
        let cart = Cartridge::from_rom(&rom)?;
        let mut cpu = M6502::new();
        cpu.reset();
        let mut riot = Riot6532::new();
        riot.input_a = 0xFF;
        riot.input_b = 0xFF;
        let clocks_per_frame =
            u64::from(region.lines_per_frame()) * u64::from(COLOUR_CLOCKS_PER_LINE);
        Ok(Self {
            cpu,
            maria: Maria::new(region.maria_region()),
            riot,
            tia_audio: TiaAudio::new(),
            cart,
            ram_zp: [0; 192],
            ram_stack: [0; 192],
            ram_main: [0; 4096],
            region,
            master_clock: 0,
            clocks_per_frame,
            frame_count: 0,
            dma_budget: 0,
            line_cycle: 0,
        })
    }

    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        let target = start + self.clocks_per_frame;
        // Paint the canonical TV-visible border (BACKGRND) at frame start.
        self.maria.fill_border();
        while self.master_clock < target {
            self.tick_colour_clock();
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_colour_clock(&mut self) {
        self.master_clock += 1;

        if self
            .master_clock
            .is_multiple_of(u64::from(COLOUR_CLOCKS_PER_LINE))
        {
            self.process_scan_line();
        }

        if self.master_clock.is_multiple_of(2) {
            self.line_cycle += 1;
            if self.line_cycle > u16::from(self.dma_budget) && !self.maria.wsync_halt() {
                self.cpu.tick();
                if self.cpu.rw {
                    self.cpu.data_in = self.mem_read(self.cpu.addr);
                } else {
                    self.mem_write(self.cpu.addr, self.cpu.data);
                }
            }
            self.riot.tick();
        }
    }

    fn process_scan_line(&mut self) {
        let cart = &self.cart;
        let ram_zp = &self.ram_zp;
        let ram_stack = &self.ram_stack;
        let ram_main = &self.ram_main;
        let dma_cycles = self.maria.render_line(&mut |addr| match addr {
            0x0040..=0x00FF => ram_zp[(addr - 0x40) as usize],
            0x0140..=0x01FF => ram_stack[(addr - 0x140) as usize],
            0x1800..=0x3FFF => ram_main[((addr - 0x1800) & 0x0FFF) as usize],
            0x4000..=0xFFFF => cart.read(addr),
            _ => 0,
        });
        self.dma_budget = dma_cycles;
        self.line_cycle = 0;
        self.maria.clear_wsync();
        self.cpu.nmi = self.maria.take_dli();
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x001F => self.tia_audio.read(addr as u8),
            0x0020..=0x003F => self.maria.read(addr as u8 - 0x20),
            0x0040..=0x00FF => self.ram_zp[(addr - 0x40) as usize],
            0x0100..=0x011F => self.tia_audio.read((addr & 0x1F) as u8),
            0x0120..=0x013F => self.maria.read((addr & 0x1F) as u8),
            0x0140..=0x01FF => self.ram_stack[(addr - 0x140) as usize],
            0x0200..=0x027F => {
                if addr & 0x20 != 0 {
                    self.maria.read((addr & 0x1F) as u8)
                } else {
                    self.tia_audio.read((addr & 0x1F) as u8)
                }
            }
            0x0280..=0x02FF => self.riot.read(addr),
            0x0300..=0x03FF => {
                if addr & 0x80 != 0 {
                    self.riot.read(addr)
                } else if addr & 0x20 != 0 {
                    self.maria.read((addr & 0x1F) as u8)
                } else {
                    self.tia_audio.read((addr & 0x1F) as u8)
                }
            }
            0x0400..=0x047F => {
                if addr & 0x20 != 0 {
                    self.maria.read((addr & 0x1F) as u8)
                } else {
                    self.tia_audio.read((addr & 0x1F) as u8)
                }
            }
            0x0480..=0x04FF => self.riot.read(addr),
            0x0500..=0x17FF => 0xFF,
            0x1800..=0x3FFF => self.ram_main[((addr - 0x1800) & 0x0FFF) as usize],
            0x4000..=0xFFFF => self.cart.read(addr),
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x001F => self.tia_audio.write(addr as u8, value),
            0x0020..=0x003F => self.maria.write(addr as u8 - 0x20, value),
            0x0040..=0x00FF => self.ram_zp[(addr - 0x40) as usize] = value,
            0x0100..=0x011F => self.tia_audio.write((addr & 0x1F) as u8, value),
            0x0120..=0x013F => self.maria.write((addr & 0x1F) as u8, value),
            0x0140..=0x01FF => self.ram_stack[(addr - 0x140) as usize] = value,
            0x0200..=0x027F => {
                if addr & 0x20 != 0 {
                    self.maria.write((addr & 0x1F) as u8, value);
                } else {
                    self.tia_audio.write((addr & 0x1F) as u8, value);
                }
            }
            0x0280..=0x02FF => self.riot.write(addr, value),
            0x0300..=0x03FF => {
                if addr & 0x80 != 0 {
                    self.riot.write(addr, value);
                } else if addr & 0x20 != 0 {
                    self.maria.write((addr & 0x1F) as u8, value);
                } else {
                    self.tia_audio.write((addr & 0x1F) as u8, value);
                }
            }
            0x0400..=0x047F => {
                if addr & 0x20 != 0 {
                    self.maria.write((addr & 0x1F) as u8, value);
                } else {
                    self.tia_audio.write((addr & 0x1F) as u8, value);
                }
            }
            0x0480..=0x04FF => self.riot.write(addr, value),
            0x0500..=0x17FF => {}
            0x1800..=0x3FFF => self.ram_main[((addr - 0x1800) & 0x0FFF) as usize] = value,
            0x4000..=0xFFFF => self.cart.write(addr, value),
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.maria.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.maria.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.maria.framebuffer_height()
    }

    /// Set P0 joystick direction. Active-low on RIOT port A bits 4-7.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool) {
        let mut val = self.riot.input_a | 0xF0;
        if up {
            val &= !0x10;
        }
        if down {
            val &= !0x20;
        }
        if left {
            val &= !0x40;
        }
        if right {
            val &= !0x80;
        }
        self.riot.input_a = val;
    }

    /// Set console switch state (active-low on RIOT port B).
    /// Bit 0 = Reset, bit 1 = Select, bit 3 = Pause.
    pub fn set_console(&mut self, reset: bool, select: bool, pause: bool) {
        let mut val = 0xFFu8;
        if reset {
            val &= !0x01;
        }
        if select {
            val &= !0x02;
        }
        if pause {
            val &= !0x08;
        }
        self.riot.input_b = val;
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }
    #[must_use]
    pub fn maria(&self) -> &Maria {
        &self.maria
    }
    #[must_use]
    pub fn region(&self) -> Atari7800Region {
        self.region
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

    fn trap_rom_32k() -> Vec<u8> {
        let mut rom = vec![0xEAu8; 32768];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0x80;
        rom[0x7FFA] = 0x00;
        rom[0x7FFB] = 0x80;
        rom[0x7FFC] = 0x00;
        rom[0x7FFD] = 0x80;
        rom[0x7FFE] = 0x00;
        rom[0x7FFF] = 0x80;
        rom
    }

    #[test]
    fn frame_advances_master_clock_and_count() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        let clocks = sys.run_frame();
        assert!(clocks > 0);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_runs_more_clocks_than_ntsc() {
        let mut ntsc = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        let mut pal = Atari7800::new(trap_rom_32k(), Atari7800Region::Pal).expect("init");
        assert!(pal.run_frame() > ntsc.run_frame());
    }

    #[test]
    fn memory_map_routes_ram_and_cart() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.mem_write(0x0040, 0x55);
        assert_eq!(sys.mem_read(0x0040), 0x55);
        sys.mem_write(0x1800, 0x66);
        assert_eq!(sys.mem_read(0x1800), 0x66);
        assert_eq!(sys.mem_read(0x2800), 0x66);
        assert_eq!(sys.mem_read(0x8000), 0x4C);
    }

    #[test]
    fn maria_register_route() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.mem_write(0x0020, 0x94);
        // BACKGRND is write-only — read returns 0 from MSTAT mirror at this addr.
        // Just verify no panic.
        let _ = sys.mem_read(0x0020);
    }

    #[test]
    fn joystick_drives_riot_port_a() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.set_joystick(true, false, false, false);
        assert_eq!(sys.riot.input_a & 0x10, 0);
        sys.set_joystick(false, false, false, false);
        assert_eq!(sys.riot.input_a & 0xF0, 0xF0);
    }

    #[test]
    fn console_switches_drive_riot_port_b() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.set_console(true, false, false);
        assert_eq!(sys.riot.input_b & 0x01, 0);
        sys.set_console(false, true, false);
        assert_eq!(sys.riot.input_b & 0x02, 0);
        sys.set_console(false, false, true);
        assert_eq!(sys.riot.input_b & 0x08, 0);
    }

    #[test]
    fn rejects_oversized_rom() {
        assert!(Atari7800::new(vec![0u8; 256_000], Atari7800Region::Ntsc).is_err());
    }
}
