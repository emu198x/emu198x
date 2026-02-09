//! Top-level C64 system.
//!
//! The master clock ticks at CPU cycle rate (985,248 Hz PAL). All components
//! tick every master clock tick. One frame = 312 lines × 63 cycles = 19,656
//! CPU cycles.
//!
//! # Tick loop
//!
//! Each tick:
//! 1. VIC-II: advance beam, render 8 pixels, detect badline
//! 2. Check VIC-II raster IRQ → CPU IRQ
//! 3. CPU: tick if not stalled by VIC-II badline
//! 4. CIA1: tick timer, check IRQ → CPU IRQ
//! 5. CIA2: tick timer (NMI — stubbed for v1)

#![allow(clippy::cast_possible_truncation)]

use emu_6502::Mos6502;
use emu_core::{Bus, Cpu, Observable, Tickable, Value};

use crate::bus::C64Bus;
use crate::config::{C64Config, C64Model};
use crate::input::{C64Key, InputQueue};
use crate::memory::C64Memory;

/// Cycles per frame (PAL): 312 lines × 63 cycles.
#[cfg(test)]
const CYCLES_PER_FRAME: u64 = 312 * 63;

/// C64 system.
pub struct C64 {
    cpu: Mos6502,
    bus: C64Bus,
    /// Master clock: counts CPU cycles.
    master_clock: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Timed input event queue.
    input_queue: InputQueue,
}

impl C64 {
    /// Create a new C64 from the given configuration.
    ///
    /// # Panics
    ///
    /// Panics if the model is not PAL (only PAL supported in v1).
    #[must_use]
    pub fn new(config: &C64Config) -> Self {
        assert!(
            config.model == C64Model::C64Pal,
            "Only PAL model is supported in v1"
        );

        let memory = C64Memory::new(&config.kernal_rom, &config.basic_rom, &config.char_rom);
        let mut bus = C64Bus::new(memory);

        // Set up CIA1 for keyboard scanning: port A = output, port B = input
        bus.cia1.write(0x02, 0xFF); // DDR A: all output
        bus.cia1.write(0x03, 0x00); // DDR B: all input
        bus.cia1.write(0x00, 0xFF); // Port A: all columns deselected

        // Set up CIA2 for default VIC bank (bank 0)
        bus.cia2.write(0x02, 0x03); // DDR A: bits 0-1 output
        bus.cia2.write(0x00, 0x03); // Port A: %11 → bank 0 (inverted: !%11 & 3 = 0)
        bus.update_vic_bank();

        // Create the CPU
        let mut cpu = Mos6502::new();

        // Read reset vector from Kernal ROM at $FFFC-$FFFD
        let reset_lo = bus.read(0xFFFC).data;
        let reset_hi = bus.read(0xFFFD).data;
        cpu.regs.pc = u16::from(reset_lo) | (u16::from(reset_hi) << 8);

        Self {
            cpu,
            bus,
            master_clock: 0,
            frame_count: 0,
            input_queue: InputQueue::new(),
        }
    }

    /// Run one complete frame (until VIC-II signals frame complete).
    ///
    /// Processes any pending input queue events at the start of the frame,
    /// then ticks the master clock until VIC-II signals frame complete.
    ///
    /// Returns the number of CPU cycles executed during the frame.
    pub fn run_frame(&mut self) -> u64 {
        self.input_queue
            .process(self.frame_count, &mut self.bus.keyboard);
        self.frame_count += 1;

        let start_clock = self.master_clock;

        loop {
            self.tick();
            if self.bus.vic.take_frame_complete() {
                break;
            }
        }

        self.master_clock - start_clock
    }

    /// Reference to the framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.vic.framebuffer()
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.bus.vic.framebuffer_width()
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.bus.vic.framebuffer_height()
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
    pub fn bus(&self) -> &C64Bus {
        &self.bus
    }

    /// Mutable reference to the bus.
    pub fn bus_mut(&mut self) -> &mut C64Bus {
        &mut self.bus
    }

    /// Master clock tick count (CPU cycles).
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

    /// Press a key immediately.
    pub fn press_key(&mut self, key: C64Key) {
        let (col, row) = key.matrix();
        self.bus.keyboard.set_key(col, row, true);
    }

    /// Release a key.
    pub fn release_key(&mut self, key: C64Key) {
        let (col, row) = key.matrix();
        self.bus.keyboard.set_key(col, row, false);
    }

    /// Release all keys.
    pub fn release_all_keys(&mut self) {
        self.bus.keyboard.release_all();
    }

    /// Load a PRG file into memory.
    pub fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        crate::prg::load_prg(&mut self.bus.memory, data)
    }
}

impl Tickable for C64 {
    fn tick(&mut self) {
        self.master_clock += 1;

        // 1. VIC-II: advance beam, render 8 pixels, detect badline
        let cpu_stalled = self.bus.vic.tick(&self.bus.memory);

        // 2. Check VIC-II raster IRQ → CPU IRQ
        if self.bus.vic.irq_active() {
            self.cpu.interrupt();
        }

        // 3. CPU: tick if not stalled by VIC-II badline
        if !cpu_stalled {
            self.cpu.tick(&mut self.bus);
        }

        // 4. CIA1: tick timer, check IRQ → CPU IRQ
        self.bus.cia1.tick();
        if self.bus.cia1.irq_active() {
            self.cpu.interrupt();
        }

        // 5. CIA2: tick timer (NMI stubbed for v1)
        self.bus.cia2.tick();
    }
}

impl Observable for C64 {
    fn query(&self, path: &str) -> Option<Value> {
        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("vic.") {
            match rest {
                "line" => Some(self.bus.vic.raster_line().into()),
                "cycle" => Some(self.bus.vic.raster_cycle().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("cia1.") {
            match rest {
                "timer_a" => Some(self.bus.cia1.timer_a().into()),
                "timer_b" => Some(self.bus.cia1.timer_b().into()),
                "icr_status" => Some(self.bus.cia1.icr_status().into()),
                "icr_mask" => Some(self.bus.cia1.icr_mask().into()),
                "cra" => Some(self.bus.cia1.cra().into()),
                "crb" => Some(self.bus.cia1.crb().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("cia2.") {
            match rest {
                "timer_a" => Some(self.bus.cia2.timer_a().into()),
                "timer_b" => Some(self.bus.cia2.timer_b().into()),
                "icr_status" => Some(self.bus.cia2.icr_status().into()),
                "icr_mask" => Some(self.bus.cia2.icr_mask().into()),
                "cra" => Some(self.bus.cia2.cra().into()),
                "crb" => Some(self.bus.cia2.crb().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("memory.") {
            let addr = if let Some(hex) =
                rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X"))
            {
                u16::from_str_radix(hex, 16).ok()
            } else if let Some(hex) = rest.strip_prefix('$') {
                u16::from_str_radix(hex, 16).ok()
            } else {
                rest.parse().ok()
            };
            addr.map(|a| Value::U8(self.bus.memory.peek(a)))
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
            "vic.line",
            "vic.cycle",
            "cia1.timer_a",
            "cia1.timer_b",
            "cia1.icr_status",
            "cia1.icr_mask",
            "cia1.cra",
            "cia1.crb",
            "cia2.timer_a",
            "cia2.timer_b",
            "cia2.icr_status",
            "cia2.icr_mask",
            "cia2.cra",
            "cia2.crb",
            "memory.<address>",
            "master_clock",
            "frame_count",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vic;

    fn make_c64() -> C64 {
        // Minimal ROMs: Kernal with a reset vector pointing to a HALT-like loop
        let mut kernal = vec![0xEA; 8192]; // NOP sled
        // Reset vector at $FFFC-$FFFD (offset $1FFC-$1FFD in Kernal ROM)
        // Point to $E000 (start of Kernal)
        kernal[0x1FFC] = 0x00; // Low byte
        kernal[0x1FFD] = 0xE0; // High byte

        let basic = vec![0; 8192];
        let chargen = vec![0; 4096];

        C64::new(&C64Config {
            model: C64Model::C64Pal,
            kernal_rom: kernal,
            basic_rom: basic,
            char_rom: chargen,
        })
    }

    #[test]
    fn master_clock_advances() {
        let mut c64 = make_c64();
        assert_eq!(c64.master_clock(), 0);
        c64.tick();
        assert_eq!(c64.master_clock(), 1);
    }

    #[test]
    fn run_frame_returns_cycle_count() {
        let mut c64 = make_c64();
        let cycles = c64.run_frame();
        // Should be close to CYCLES_PER_FRAME (may vary slightly due to
        // instruction boundaries and badlines)
        assert!(
            cycles >= CYCLES_PER_FRAME - 100 && cycles <= CYCLES_PER_FRAME + 100,
            "Expected ~{CYCLES_PER_FRAME} cycles, got {cycles}"
        );
    }

    #[test]
    fn framebuffer_correct_size() {
        let c64 = make_c64();
        assert_eq!(c64.framebuffer_width(), vic::FB_WIDTH);
        assert_eq!(c64.framebuffer_height(), vic::FB_HEIGHT);
        assert_eq!(
            c64.framebuffer().len(),
            vic::FB_WIDTH as usize * vic::FB_HEIGHT as usize
        );
    }

    #[test]
    fn observable_cpu_pc() {
        let c64 = make_c64();
        let pc = c64.query("cpu.pc");
        assert_eq!(pc, Some(Value::U16(0xE000)));
    }

    #[test]
    fn observable_memory() {
        let mut c64 = make_c64();
        c64.bus_mut().memory.ram_write(0x8000, 0xAB);
        assert_eq!(c64.query("memory.0x8000"), Some(Value::U8(0xAB)));
    }
}
