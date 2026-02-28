//! Top-level NES system.
//!
//! The master clock ticks at 21,477,272 Hz (NTSC crystal). Components
//! derive their timing from this:
//! - PPU: crystal / 4 = 5,369,318 Hz (tick when `master_clock` % 4 == 0)
//! - CPU: crystal / 12 = 1,789,773 Hz (tick when `master_clock` % 12 == 0)
//!
//! One frame = 341 PPU dots × 262 scanlines = 89,342 PPU cycles.
//! In crystal ticks: 89,342 × 4 = 357,368.

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, Cpu, Observable, Tickable, Value};
use mos_6502::Mos6502;

use crate::bus::NesBus;
use crate::cartridge::{self, Mapper};
use crate::config::{NesConfig, NesRegion};
use crate::controller::Controller;
use crate::input::{InputQueue, NesButton};
use crate::ppu;

/// Crystal divisors (same for both NTSC and PAL — only the crystal changes).
const PPU_DIVISOR: u64 = 4;
const CPU_DIVISOR: u64 = 12;

/// NES system.
pub struct Nes {
    cpu: Mos6502,
    bus: NesBus,
    /// Master clock: counts crystal ticks.
    master_clock: u64,
    /// Crystal ticks per frame (region-dependent).
    ticks_per_frame: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Timed input event queue.
    input_queue: InputQueue,
    /// OAM DMA state.
    dma_cycles_remaining: u16,
    dma_addr: u16,
    dma_read_data: u8,
    dma_odd_cycle: bool,
    /// DMC DMA steal counter: counts down from 4 to 0.
    dmc_dma_cycles: u8,
    /// Video region.
    region: NesRegion,
}

impl Nes {
    /// Create a new NES from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM data is invalid.
    pub fn new(config: &NesConfig) -> Result<Self, String> {
        let mapper = cartridge::parse_ines(&config.rom_data)?;
        Ok(Self::from_mapper(mapper, config.region))
    }

    /// Create a new NES from a pre-parsed mapper.
    fn from_mapper(mapper: Box<dyn Mapper>, region: NesRegion) -> Self {
        let mut bus = NesBus::new_with_region(mapper, region);

        let mut cpu = Mos6502::new();

        // Read reset vector from $FFFC-$FFFD
        let reset_lo = bus.read(0xFFFC).data;
        let reset_hi = bus.read(0xFFFD).data;
        cpu.regs.pc = u16::from(reset_lo) | (u16::from(reset_hi) << 8);

        let scanlines = u64::from(region.scanlines_per_frame());
        let ticks_per_frame = 341 * scanlines * PPU_DIVISOR;

        Self {
            cpu,
            bus,
            master_clock: 0,
            ticks_per_frame,
            frame_count: 0,
            input_queue: InputQueue::new(),
            dma_cycles_remaining: 0,
            dma_addr: 0,
            dma_read_data: 0,
            dma_odd_cycle: false,
            dmc_dma_cycles: 0,
            region,
        }
    }

    /// Run one complete frame.
    ///
    /// Processes any pending input queue events at the start of the frame,
    /// then ticks the master clock for one frame's worth of crystal ticks.
    ///
    /// Returns the number of crystal ticks executed.
    pub fn run_frame(&mut self) -> u64 {
        self.input_queue
            .process(self.frame_count, &mut self.bus.controller1);
        self.frame_count += 1;

        let start_clock = self.master_clock;
        let target = start_clock + self.ticks_per_frame;

        while self.master_clock < target {
            self.tick();
        }

        self.master_clock - start_clock
    }

    /// Reference to the framebuffer (ARGB32, 256x240).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.ppu.framebuffer()
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        ppu::FB_WIDTH
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        ppu::FB_HEIGHT
    }

    /// Video region (NTSC or PAL).
    #[must_use]
    pub fn region(&self) -> NesRegion {
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

    /// Reference to the bus.
    #[must_use]
    pub fn bus(&self) -> &NesBus {
        &self.bus
    }

    /// Mutable reference to the bus.
    pub fn bus_mut(&mut self) -> &mut NesBus {
        &mut self.bus
    }

    /// Master clock tick count (crystal ticks).
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    /// Completed frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Mutable reference to the timed input queue.
    pub fn input_queue(&mut self) -> &mut InputQueue {
        &mut self.input_queue
    }

    /// Press a button on controller 1 immediately.
    pub fn press_button(&mut self, button: NesButton) {
        self.bus.controller1.set_button(button.bit(), true);
    }

    /// Release a button on controller 1.
    pub fn release_button(&mut self, button: NesButton) {
        self.bus.controller1.set_button(button.bit(), false);
    }

    /// Release all buttons.
    pub fn release_all_buttons(&mut self) {
        for bit in 0..8 {
            self.bus.controller1.set_button(bit, false);
        }
    }

    /// Get controller 1 reference.
    #[must_use]
    pub fn controller1(&self) -> &Controller {
        &self.bus.controller1
    }

    /// Take the APU audio output buffer (drains it).
    ///
    /// Returns mono f32 samples in the range -1.0 to 1.0, at 48 kHz.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.bus.apu.take_buffer()
    }

    /// Number of audio samples pending in the APU buffer.
    #[must_use]
    pub fn audio_buffer_len(&self) -> usize {
        self.bus.apu.buffer_len()
    }

    /// Handle OAM DMA within the tick loop.
    fn tick_dma(&mut self) {
        if self.dma_cycles_remaining == 0 {
            return;
        }

        self.dma_cycles_remaining -= 1;

        if self.dma_odd_cycle {
            // Odd cycle: write to OAM
            self.bus.ppu.write_oam(
                self.bus
                    .ppu
                    .oam_addr()
                    .wrapping_add((self.dma_addr & 0xFF) as u8),
                self.dma_read_data,
            );
            self.dma_addr = self.dma_addr.wrapping_add(1);
        } else {
            // Even cycle: read from CPU memory
            self.dma_read_data = self.bus.read(u32::from(self.dma_addr)).data;
        }

        self.dma_odd_cycle = !self.dma_odd_cycle;
    }
}

impl Tickable for Nes {
    fn tick(&mut self) {
        self.master_clock += 1;

        // PPU: every 4 crystal ticks
        if self.master_clock.is_multiple_of(PPU_DIVISOR) {
            self.bus.ppu.tick(self.bus.cartridge.as_mut());

            // VBlank NMI → CPU
            if self.bus.ppu.take_nmi() {
                self.cpu.nmi();
            }
        }

        // CPU: every 12 crystal ticks
        if self.master_clock.is_multiple_of(CPU_DIVISOR) {
            // Check for OAM DMA trigger
            if let Some(page) = self.bus.oam_dma_page.take() {
                self.dma_addr = u16::from(page) << 8;
                // 513 cycles + 1 if on odd CPU cycle
                self.dma_cycles_remaining = 513;
                self.dma_odd_cycle = false;
                if self.master_clock / CPU_DIVISOR % 2 == 1 {
                    self.dma_cycles_remaining += 1;
                }
            }

            // DMC DMA: steal cycles one byte at a time
            if self.dmc_dma_cycles == 0
                && self.bus.apu.dmc.dma_pending
                && self.dma_cycles_remaining == 0
            {
                self.dmc_dma_cycles = 4;
            }

            if self.dmc_dma_cycles > 0 {
                self.dmc_dma_cycles -= 1;
                if self.dmc_dma_cycles == 0 {
                    // Final cycle: perform the bus read and deliver the byte
                    let addr = self.bus.apu.dmc.current_address;
                    let byte = self.bus.read(u32::from(addr)).data;
                    self.bus.apu.dmc.receive_dma_byte(byte);
                }
            } else if self.dma_cycles_remaining > 0 {
                self.tick_dma();
            } else {
                self.cpu.tick(&mut self.bus);
            }

            // APU ticks at CPU rate
            self.bus.apu.tick();

            // APU / mapper IRQ → CPU (level-sensitive)
            if self.bus.apu.irq_pending() || self.bus.cartridge.irq_pending() {
                self.cpu.interrupt();
            }
        }
    }
}

impl Observable for Nes {
    fn query(&self, path: &str) -> Option<Value> {
        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("ppu.") {
            match rest {
                "scanline" => Some(self.bus.ppu.scanline().into()),
                "dot" => Some(self.bus.ppu.dot().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("apu.") {
            self.bus.apu.query(rest)
        } else if let Some(rest) = path.strip_prefix("memory.") {
            let addr =
                if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
                    u16::from_str_radix(hex, 16).ok()
                } else if let Some(hex) = rest.strip_prefix('$') {
                    u16::from_str_radix(hex, 16).ok()
                } else {
                    rest.parse().ok()
                };
            addr.map(|a| Value::U8(self.bus.peek_ram(a)))
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
            "ppu.scanline",
            "ppu.dot",
            "apu.pulse1.period",
            "apu.pulse1.length",
            "apu.pulse1.envelope",
            "apu.pulse2.period",
            "apu.triangle.period",
            "apu.noise.period",
            "apu.frame_counter.mode",
            "memory.<address>",
            "master_clock",
            "frame_count",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::{Mirroring, Nrom};

    fn make_nes() -> Nes {
        // 32K PRG filled with NOPs, reset vector at $8000
        let mut prg = vec![0xEA; 32768]; // NOP sled
        // Reset vector at $FFFC-$FFFD (offset $7FFC-$7FFD in PRG)
        prg[0x7FFC] = 0x00; // Low byte → $8000
        prg[0x7FFD] = 0x80; // High byte
        let chr = vec![0; 8192];
        let mapper = Box::new(Nrom::new(prg, chr, Mirroring::Horizontal));
        Nes::from_mapper(mapper, NesRegion::Ntsc)
    }

    #[test]
    fn master_clock_advances() {
        let mut nes = make_nes();
        assert_eq!(nes.master_clock(), 0);
        nes.tick();
        assert_eq!(nes.master_clock(), 1);
    }

    #[test]
    fn run_frame_returns_tick_count() {
        let mut nes = make_nes();
        let ticks = nes.run_frame();
        // NTSC: 341 dots × 262 scanlines × 4 = 357,368
        assert_eq!(ticks, 341 * 262 * 4);
    }

    #[test]
    fn framebuffer_correct_size() {
        let nes = make_nes();
        assert_eq!(nes.framebuffer_width(), ppu::FB_WIDTH);
        assert_eq!(nes.framebuffer_height(), ppu::FB_HEIGHT);
        assert_eq!(
            nes.framebuffer().len(),
            ppu::FB_WIDTH as usize * ppu::FB_HEIGHT as usize
        );
    }

    #[test]
    fn observable_cpu_pc() {
        let nes = make_nes();
        let pc = nes.query("cpu.pc");
        assert_eq!(pc, Some(Value::U16(0x8000)));
    }

    #[test]
    fn observable_memory() {
        let mut nes = make_nes();
        nes.bus_mut().ram[0] = 0xAB;
        assert_eq!(nes.query("memory.0x0000"), Some(Value::U8(0xAB)));
    }
}
