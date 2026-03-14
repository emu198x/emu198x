//! Cycle-accurate Atari 2600 emulator.
//!
//! The master clock ticks at the TIA colour clock frequency:
//! - NTSC: 3,579,545 Hz
//! - PAL: 3,546,894 Hz
//!
//! The CPU (6507, a pin-limited 6502) runs at crystal/3 = 1 CPU cycle
//! per 3 colour clocks. The RIOT timer also ticks once per CPU cycle.
//!
//! One scanline = 228 colour clocks = 76 CPU cycles.
//! One frame ≈ 262 lines (NTSC) / 312 lines (PAL), software-controlled.

mod bus;
#[cfg(feature = "native")]
pub mod capture;
mod cartridge;
mod config;
#[cfg(feature = "native")]
pub mod controller_map;

pub use atari_tia as tia;
pub use bus::Atari2600Bus;
pub use config::{Atari2600Config, Atari2600Region};

use atari_tia::{Tia, TiaRegion};
use emu_core::{AudioFrame, Cpu, Machine, Observable, Tickable, Value};
use mos_6502::Mos6502;

use crate::bus::Atari2600BusInner;
use crate::cartridge::Cartridge;

/// Atari 2600 system.
pub struct Atari2600 {
    cpu: Mos6502,
    bus: Atari2600BusInner,
    /// Master clock: counts colour clocks.
    master_clock: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Video region.
    region: Atari2600Region,
    /// Colour clocks per frame.
    clocks_per_frame: u64,
}

impl Atari2600 {
    /// Create a new Atari 2600 from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM data is invalid.
    pub fn new(config: &Atari2600Config) -> Result<Self, String> {
        let cart = Cartridge::from_rom(&config.rom_data)?;
        let tia_region = match config.region {
            Atari2600Region::Ntsc => TiaRegion::Ntsc,
            Atari2600Region::Pal => TiaRegion::Pal,
        };

        let tia = Tia::new(tia_region);
        let riot = mos_riot_6532::Riot6532::new();

        let mut bus = Atari2600BusInner {
            tia,
            riot,
            cart,
        };

        let mut cpu = Mos6502::new();

        // Read reset vector from $FFFC-$FFFD (mapped through bus).
        let reset_lo = emu_core::Bus::read(&mut Atari2600Bus(&mut bus), 0xFFFC).data;
        let reset_hi = emu_core::Bus::read(&mut Atari2600Bus(&mut bus), 0xFFFD).data;
        cpu.regs.pc = u16::from(reset_lo) | (u16::from(reset_hi) << 8);

        let lines = u64::from(config.region.lines_per_frame());
        let clocks_per_frame = lines * u64::from(atari_tia::CLOCKS_PER_LINE);

        Ok(Self {
            cpu,
            bus,
            master_clock: 0,
            frame_count: 0,
            region: config.region,
            clocks_per_frame,
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

    /// Reference to the framebuffer (ARGB32, 160 × lines).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.tia.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.bus.tia.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.bus.tia.framebuffer_height()
    }

    /// Video region.
    #[must_use]
    pub fn region(&self) -> Atari2600Region {
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

    /// Reference to the TIA.
    #[must_use]
    pub fn tia(&self) -> &Tia {
        &self.bus.tia
    }

    /// Reference to the RIOT.
    #[must_use]
    pub fn riot(&self) -> &mos_riot_6532::Riot6532 {
        &self.bus.riot
    }

    /// Set RIOT port A input (joystick directions).
    pub fn set_joystick_input(&mut self, value: u8) {
        self.bus.riot.set_port_a_input(value);
    }

    /// Set RIOT port B input (console switches).
    pub fn set_switch_input(&mut self, value: u8) {
        self.bus.riot.set_port_b_input(value);
    }

    /// Set player 0 fire button state.
    pub fn set_fire_button_p0(&mut self, pressed: bool) {
        self.bus.tia.set_inpt4(pressed);
    }

    /// Set player 1 fire button state.
    pub fn set_fire_button_p1(&mut self, pressed: bool) {
        self.bus.tia.set_inpt5(pressed);
    }
}

impl Tickable for Atari2600 {
    fn tick(&mut self) {
        self.master_clock += 1;

        // TIA ticks every colour clock (1:1 with master clock).
        self.bus.tia.tick();

        // CPU and RIOT tick every 3rd colour clock.
        if self.master_clock.is_multiple_of(3) {
            // Skip CPU when WSYNC is active.
            if !self.bus.tia.wsync_halt {
                self.cpu.tick(&mut Atari2600Bus(&mut self.bus));
            }

            // RIOT timer ticks once per CPU cycle.
            self.bus.riot.tick();
        }
    }
}

impl Observable for Atari2600 {
    fn query(&self, path: &str) -> Option<Value> {
        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("tia.") {
            match rest {
                "hpos" => Some(self.bus.tia.hpos().into()),
                "vpos" => Some(self.bus.tia.vpos().into()),
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
            "tia.hpos",
            "tia.vpos",
            "master_clock",
            "frame_count",
        ]
    }
}

impl Machine for Atari2600 {
    fn run_frame(&mut self) {
        let _ = self.run_frame();
    }

    fn framebuffer(&self) -> &[u32] {
        self.framebuffer()
    }

    fn framebuffer_width(&self) -> u32 {
        self.framebuffer_width()
    }

    fn framebuffer_height(&self) -> u32 {
        self.framebuffer_height()
    }

    fn take_audio_buffer(&mut self) -> Vec<AudioFrame> {
        Vec::new()
    }

    fn frame_count(&self) -> u64 {
        self.frame_count()
    }

    fn reset(&mut self) {
        self.cpu_mut().reset();
    }
}
