//! Cycle-accurate Atari 5200 emulator.
//!
//! The master clock ticks at the colour clock frequency:
//! - NTSC: 3,579,545 Hz
//! - PAL: 3,546,894 Hz
//!
//! The CPU (6502C "SALLY") runs at crystal/2 = 1 CPU cycle per 2
//! colour clocks. ANTIC ticks at the colour clock rate. POKEY ticks
//! once per CPU cycle (every 2nd colour clock).
//!
//! One scan line = 228 colour clocks = 114 CPU cycles.
//! One frame = 262 lines (NTSC) / 312 lines (PAL).

mod bus;
#[cfg(feature = "native")]
pub mod capture;
mod cartridge;
mod config;
#[cfg(feature = "native")]
pub mod controller_map;

pub use atari_antic as antic;
pub use atari_gtia as gtia;
pub use atari_pokey as pokey;
pub use bus::Atari5200Bus;
pub use config::{Atari5200Config, Atari5200Region};

use atari_antic::{Antic, AnticRegion, COLOUR_CLOCKS_PER_LINE};
use atari_gtia::Gtia;
use atari_pokey::Pokey;
use emu_core::{Cpu, Observable, Tickable, Value};
use mos_6502::Mos6502;

use crate::bus::Atari5200BusInner;
use crate::cartridge::Cartridge;

/// Joystick centre value for POKEY pot registers (0-228 range).
pub const POT_CENTER: u8 = 114;

/// Joystick maximum value (fully right or fully down).
pub const POT_MAX: u8 = 228;

/// Atari 5200 system.
pub struct Atari5200 {
    /// 6502C CPU.
    cpu: Mos6502,
    /// Bus (owns RAM, chips, cartridge).
    bus: Atari5200BusInner,
    /// Master clock: counts colour clocks.
    master_clock: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Video region.
    region: Atari5200Region,
    /// Colour clocks per frame.
    clocks_per_frame: u64,
    /// DMA cycles stolen by ANTIC for the current scan line.
    dma_budget: u8,
    /// CPU cycle counter within the current scan line (0-113).
    line_cycle: u16,
}

impl Atari5200 {
    /// Create a new Atari 5200 from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM data is invalid.
    pub fn new(config: &Atari5200Config) -> Result<Self, String> {
        let cart = Cartridge::from_rom(&config.rom_data)?;
        let antic_region = match config.region {
            Atari5200Region::Ntsc => AnticRegion::Ntsc,
            Atari5200Region::Pal => AnticRegion::Pal,
        };

        let antic = Antic::new(antic_region);
        let gtia = Gtia::new();
        let pokey = Pokey::new(config.region.cpu_hz());

        let bios = config.bios_data.clone().unwrap_or_default();

        let mut bus = Atari5200BusInner {
            ram: [0; 16384],
            antic,
            gtia,
            pokey,
            cart,
            bios,
        };

        let mut cpu = Mos6502::new();

        // Set pot inputs to centre position.
        bus.pokey.set_pot(0, POT_CENTER);
        bus.pokey.set_pot(1, POT_CENTER);

        // Read reset vector.
        let reset_lo = emu_core::Bus::read(&mut Atari5200Bus(&mut bus), 0xFFFC).data;
        let reset_hi = emu_core::Bus::read(&mut Atari5200Bus(&mut bus), 0xFFFD).data;
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
        self.bus.gtia.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.bus.gtia.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.bus.gtia.framebuffer_height()
    }

    /// Video region.
    #[must_use]
    pub fn region(&self) -> Atari5200Region {
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

    /// Reference to the ANTIC.
    #[must_use]
    pub fn antic(&self) -> &Antic {
        &self.bus.antic
    }

    /// Reference to the GTIA.
    #[must_use]
    pub fn gtia(&self) -> &Gtia {
        &self.bus.gtia
    }

    /// Reference to the POKEY.
    #[must_use]
    pub fn pokey(&self) -> &Pokey {
        &self.bus.pokey
    }

    /// Set joystick position via POKEY pots (values 0-228).
    /// 0 = left/up, 114 = centre, 228 = right/down.
    pub fn set_joystick(&mut self, x: u8, y: u8) {
        self.bus.pokey.set_pot(0, x.min(POT_MAX));
        self.bus.pokey.set_pot(1, y.min(POT_MAX));
    }

    /// Set fire button state (GTIA TRIG0).
    pub fn set_fire(&mut self, pressed: bool) {
        self.bus.gtia.set_trigger(0, pressed);
    }

    /// Set start button state (GTIA CONSOL bit 0, active low).
    pub fn set_start(&mut self, pressed: bool) {
        if pressed {
            // Clear bit 0 (active low)
            let consol = self.bus.gtia.read(0x1F) & 0xFE;
            self.bus.gtia.write(0x1F, consol);
        } else {
            let consol = self.bus.gtia.read(0x1F) | 0x01;
            self.bus.gtia.write(0x1F, consol);
        }
    }

    /// Process the start of a new scan line: run ANTIC, render via GTIA,
    /// handle NMI, and set up DMA budget for the line.
    fn process_scan_line(&mut self) {
        // Process ANTIC line -- reads display list and screen data from RAM.
        let result = self.bus.antic.process_line(&self.bus.ram);

        // Feed player/missile data to GTIA if PM DMA occurred.
        if result.pm_dma {
            for i in 0..4 {
                self.bus.gtia.write(0x0D + i as u8, result.player_data[i]);
            }
            self.bus.gtia.write(0x11, result.missile_data);
        }

        // Render the line via GTIA. ANTIC scan_line is post-increment,
        // so the line we just processed is scan_line - 1.
        let line = self.bus.antic.scan_line().saturating_sub(1);
        // Offset into visible region (ANTIC visible starts at line 8).
        let visible_line = line.wrapping_sub(8);
        self.bus.gtia.render_line(
            visible_line,
            &result.playfield,
            result.playfield_width,
            result.mode,
        );

        // Set DMA budget for this line.
        self.dma_budget = result.dma_cycles;
        self.line_cycle = 0;

        // Clear WSYNC at line boundary.
        self.bus.antic.clear_wsync();

        // Handle NMIs from ANTIC.
        if self.bus.antic.take_vbi() {
            self.cpu.nmi();
        }
        if self.bus.antic.take_dli() {
            self.cpu.nmi();
        }
    }
}

impl Tickable for Atari5200 {
    fn tick(&mut self) {
        self.master_clock += 1;

        // At the start of each scan line (every 228 colour clocks).
        if self.master_clock.is_multiple_of(u64::from(COLOUR_CLOCKS_PER_LINE)) {
            self.process_scan_line();
        }

        // CPU + POKEY tick every 2nd colour clock.
        if self.master_clock.is_multiple_of(2) {
            self.line_cycle += 1;

            // CPU: skip if ANTIC is stealing cycles (DMA) or WSYNC is active.
            if self.line_cycle > u16::from(self.dma_budget)
                && !self.bus.antic.wsync_halt()
            {
                self.cpu.tick(&mut Atari5200Bus(&mut self.bus));
            }

            // POKEY always ticks.
            self.bus.pokey.tick();

            // POKEY IRQ -> CPU IRQ.
            if self.bus.pokey.irq_pending() {
                self.cpu.interrupt();
            }
        }
    }
}

impl Observable for Atari5200 {
    fn query(&self, path: &str) -> Option<Value> {
        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("antic.") {
            match rest {
                "scan_line" => Some(self.bus.antic.scan_line().into()),
                "vcount" => Some(self.bus.antic.vcount().into()),
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
            "antic.scan_line",
            "antic.vcount",
            "master_clock",
            "frame_count",
        ]
    }
}
