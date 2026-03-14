//! Cycle-accurate Atari 7800 emulator.
//!
//! The master clock ticks at the colour clock frequency:
//! - NTSC: 3,579,545 Hz
//! - PAL: 3,546,894 Hz
//!
//! The CPU (6502C "SALLY") runs at crystal/2 = 1 CPU cycle per 2
//! colour clocks. MARIA renders one scanline at a time and steals
//! CPU cycles for DMA. RIOT ticks once per CPU cycle (every 2nd
//! colour clock).
//!
//! One scan line = 228 colour clocks = 114 CPU cycles.
//! One frame = 263 lines (NTSC) / 313 lines (PAL).

mod bus;
#[cfg(feature = "native")]
pub mod capture;
mod cartridge;
mod config;
#[cfg(feature = "native")]
pub mod controller_map;
mod tia_audio;

pub use atari_maria as maria;
pub use bus::Atari7800Bus;
pub use config::{Atari7800Config, Atari7800Region};

use atari_maria::{Maria, MariaRegion};
use emu_core::{Cpu, Observable, Tickable, Value};
use mos_6502::Mos6502;
use mos_riot_6532::Riot6532;

use crate::bus::Atari7800BusInner;
use crate::cartridge::Cartridge;
use crate::tia_audio::TiaAudio;

/// Colour clocks per scanline (same as 2600/5200).
const COLOUR_CLOCKS_PER_LINE: u16 = 228;

/// Atari 7800 system.
pub struct Atari7800 {
    /// 6502C "SALLY" CPU.
    cpu: Mos6502,
    /// Bus (owns RAM, chips, cartridge).
    bus: Atari7800BusInner,
    /// Master clock: counts colour clocks.
    master_clock: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Video region.
    region: Atari7800Region,
    /// Colour clocks per frame.
    clocks_per_frame: u64,
    /// DMA cycles stolen by MARIA for the current scan line.
    dma_budget: u8,
    /// CPU cycle counter within the current scan line (0-113).
    line_cycle: u16,
    /// Fire button state (active low on RIOT port A bit 7).
    fire_pressed: bool,
}

impl Atari7800 {
    /// Create a new Atari 7800 from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM data is invalid.
    pub fn new(config: &Atari7800Config) -> Result<Self, String> {
        let cart = Cartridge::from_rom(&config.rom_data)?;
        let maria_region = match config.region {
            Atari7800Region::Ntsc => MariaRegion::Ntsc,
            Atari7800Region::Pal => MariaRegion::Pal,
        };

        let maria = Maria::new(maria_region);
        let tia_audio = TiaAudio::new();
        let riot = Riot6532::new();

        let mut bus = Atari7800BusInner {
            maria,
            tia_audio,
            riot,
            cart,
            ram_zp: [0; 192],
            ram_stack: [0; 192],
            ram_main: [0; 4096],
        };

        let mut cpu = Mos6502::new();

        // RIOT ports: all inputs active-high by default.
        // Port A bits 4-7 = P0 joystick (active low), bits 0-3 = P1.
        bus.riot.input_a = 0xFF;
        // Port B = console switches (active low).
        bus.riot.input_b = 0xFF;

        // Read reset vector from cartridge.
        let reset_lo = emu_core::Bus::read(&mut Atari7800Bus(&mut bus), 0xFFFC).data;
        let reset_hi = emu_core::Bus::read(&mut Atari7800Bus(&mut bus), 0xFFFD).data;
        cpu.regs.pc = u16::from(reset_lo) | (u16::from(reset_hi) << 8);

        let lines = u64::from(config.region.lines_per_frame());
        let clocks_per_frame = lines * u64::from(COLOUR_CLOCKS_PER_LINE);

        Ok(Self {
            cpu,
            bus,
            master_clock: 0,
            frame_count: 0,
            region: config.region,
            clocks_per_frame,
            dma_budget: 0,
            line_cycle: 0,
            fire_pressed: false,
        })
    }

    /// Run one complete frame.
    ///
    /// Returns the number of colour clocks executed.
    pub fn run_frame(&mut self) -> u64 {
        self.frame_count += 1;
        let start = self.master_clock;
        let target = start + self.clocks_per_frame;

        while self.master_clock < target {
            self.tick();
        }

        self.master_clock - start
    }

    /// Reference to the framebuffer (ARGB32, 320 x 240).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.maria.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.bus.maria.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.bus.maria.framebuffer_height()
    }

    /// Video region.
    #[must_use]
    pub fn region(&self) -> Atari7800Region {
        self.region
    }

    /// Reference to the CPU.
    #[must_use]
    pub fn cpu(&self) -> &Mos6502 {
        &self.cpu
    }

    /// Mutable reference to the CPU.
    pub fn cpu_mut(&mut self) -> &mut Mos6502 {
        &mut self.cpu
    }

    /// Master clock tick count (colour clocks).
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    /// Completed frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Reference to the MARIA display processor.
    #[must_use]
    pub fn maria(&self) -> &Maria {
        &self.bus.maria
    }

    /// Set joystick direction (player 0). Active-low on RIOT port A bits 4-7.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool) {
        let mut val = self.bus.riot.input_a | 0xF0;
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
        self.bus.riot.input_a = val;
    }

    /// Set fire button state (active low).
    pub fn set_fire(&mut self, pressed: bool) {
        self.fire_pressed = pressed;
    }

    /// Process the start of a new scan line: render MARIA, handle DLI/NMI.
    fn process_scan_line(&mut self) {
        // Split the bus so MARIA can read from RAM/ROM without a borrow conflict.
        let maria = &mut self.bus.maria;
        let cart = &self.bus.cart;
        let ram_zp = &self.bus.ram_zp;
        let ram_stack = &self.bus.ram_stack;
        let ram_main = &self.bus.ram_main;

        let dma_cycles = maria.render_line(&mut |addr| match addr {
            0x0040..=0x00FF => ram_zp[(addr - 0x40) as usize],
            0x0140..=0x01FF => ram_stack[(addr - 0x140) as usize],
            0x1800..=0x3FFF => ram_main[((addr - 0x1800) & 0x0FFF) as usize],
            0x4000..=0xFFFF => cart.read_pure(addr),
            _ => 0,
        });

        self.dma_budget = dma_cycles;
        self.line_cycle = 0;

        // Clear WSYNC at line boundary.
        self.bus.maria.clear_wsync();

        // DLI fires NMI.
        if self.bus.maria.take_dli() {
            self.cpu.nmi();
        }
    }
}

impl Tickable for Atari7800 {
    fn tick(&mut self) {
        self.master_clock += 1;

        // At the start of each scan line (every 228 colour clocks).
        if self.master_clock.is_multiple_of(u64::from(COLOUR_CLOCKS_PER_LINE)) {
            self.process_scan_line();
        }

        // CPU + RIOT tick every 2nd colour clock.
        if self.master_clock.is_multiple_of(2) {
            self.line_cycle += 1;

            // CPU: skip if MARIA is stealing cycles (DMA) or WSYNC is active.
            if self.line_cycle > u16::from(self.dma_budget)
                && !self.bus.maria.wsync_halt()
            {
                self.cpu.tick(&mut Atari7800Bus(&mut self.bus));
            }

            // RIOT timer always ticks.
            self.bus.riot.tick();
        }
    }
}

impl Observable for Atari7800 {
    fn query(&self, path: &str) -> Option<Value> {
        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("maria.") {
            match rest {
                "scan_line" => Some(self.bus.maria.scan_line().into()),
                "vblank" => Some(Value::Bool(self.bus.maria.vblank())),
                _ => None,
            }
        } else {
            match path {
                "master_clock" => Some(self.master_clock.into()),
                "frame_count" => Some(self.frame_count.into()),
                _ => self.cpu.query(path),
            }
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        &[
            "cpu.<6502_paths>",
            "maria.scan_line",
            "maria.vblank",
            "master_clock",
            "frame_count",
        ]
    }
}
