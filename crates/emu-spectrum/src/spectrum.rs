//! Top-level Spectrum system.
//!
//! The master crystal runs at 14 MHz. All component timing derives from this:
//! - ULA (video): ticks every 2 crystal ticks (7 MHz pixel clock)
//! - CPU (Z80): ticks every 4 crystal ticks (3.5 MHz, normal speed)
//!
//! The CPU divider is runtime-configurable for turbo modes (7/14/28 MHz on
//! Pentagon, Scorpion, and Next), but v1 always uses 4 (3.5 MHz).
//!
//! # Frame loop
//!
//! `run_frame()` ticks the master clock until the ULA signals frame complete.
//! One frame = 69,888 T-states = 279,552 crystal ticks (48K PAL).

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Cpu, Observable, Tickable, Value};
use zilog_z80::Z80;

use crate::beeper::BeeperState;
use crate::bus::SpectrumBus;
use crate::config::{SpectrumConfig, SpectrumModel};
use crate::input::{InputQueue, SpectrumKey};
use crate::memory::Memory48K;
use crate::tap::TapFile;
use crate::tape::TapeDeck;
use crate::ula::Ula;

/// CPU clock divider (crystal ticks per CPU T-state).
/// 4 = 3.5 MHz (normal speed for all Sinclair models).
const CPU_DIVIDER: u64 = 4;

/// Video clock divider (crystal ticks per pixel clock tick).
/// 2 = 7 MHz (ULA pixel clock).
const VIDEO_DIVIDER: u64 = 2;

/// Default audio output sample rate.
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// CPU frequency in Hz (3.5 MHz).
const CPU_FREQUENCY: u32 = 3_500_000;

/// ROM address of the LD-BYTES routine (tape loading entry point).
const LD_BYTES_ADDR: u16 = 0x0556;

/// ZX Spectrum system.
pub struct Spectrum {
    cpu: Z80,
    bus: SpectrumBus,
    /// Master crystal tick counter.
    master_clock: u64,
    /// CPU clock divider (crystal ticks per CPU T-state).
    cpu_divider: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Timed input event queue for scripted key sequences.
    input_queue: InputQueue,
    /// Virtual tape deck for TAP loading.
    tape: TapeDeck,
}

impl Spectrum {
    /// Create a new Spectrum from the given configuration.
    ///
    /// # Panics
    ///
    /// Panics if the model is not yet supported (only 48K in v1).
    #[must_use]
    pub fn new(config: &SpectrumConfig) -> Self {
        assert!(
            config.model == SpectrumModel::Spectrum48K,
            "Only 48K model is supported in v1"
        );

        let memory = Box::new(Memory48K::new(&config.rom));
        let video = Box::new(Ula::new());
        let beeper = BeeperState::new(CPU_FREQUENCY, AUDIO_SAMPLE_RATE);
        let bus = SpectrumBus::new(memory, video, beeper);

        Self {
            cpu: Z80::new(),
            bus,
            master_clock: 0,
            cpu_divider: CPU_DIVIDER,
            frame_count: 0,
            input_queue: InputQueue::new(),
            tape: TapeDeck::new(),
        }
    }

    /// Run one complete frame (until the ULA signals frame complete).
    ///
    /// Processes any pending input queue events at the start of the frame,
    /// then ticks the master clock until the ULA signals frame complete.
    ///
    /// Returns the number of CPU T-states executed during the frame.
    pub fn run_frame(&mut self) -> u64 {
        self.input_queue
            .process(self.frame_count, &mut self.bus.keyboard);
        self.frame_count += 1;

        let start_ticks = self.cpu.total_ticks();

        loop {
            self.tick();
            if self.bus.video.take_frame_complete() {
                break;
            }
        }

        (self.cpu.total_ticks() - start_ticks).get()
    }

    /// Reference to the framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.video.framebuffer()
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.bus.video.framebuffer_width()
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.bus.video.framebuffer_height()
    }

    /// Take the audio buffer from the beeper (drains it).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.bus.beeper.take_buffer()
    }

    /// Reference to the CPU.
    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    /// Mutable reference to the CPU.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }

    /// Reference to the bus.
    #[must_use]
    pub fn bus(&self) -> &SpectrumBus {
        &self.bus
    }

    /// Mutable reference to the bus.
    pub fn bus_mut(&mut self) -> &mut SpectrumBus {
        &mut self.bus
    }

    /// Master clock tick count.
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

    /// Press a key immediately (stays pressed until released).
    pub fn press_key(&mut self, key: SpectrumKey) {
        let (row, bit) = key.matrix();
        self.bus.keyboard.set_key(row, bit, true);
    }

    /// Release a key.
    pub fn release_key(&mut self, key: SpectrumKey) {
        let (row, bit) = key.matrix();
        self.bus.keyboard.set_key(row, bit, false);
    }

    /// Release all keys.
    pub fn release_all_keys(&mut self) {
        self.bus.keyboard.release_all();
    }

    /// Insert a TAP file into the tape deck.
    pub fn insert_tap(&mut self, tap: TapFile) {
        self.tape.insert(tap);
    }

    /// Eject the tape.
    pub fn eject_tape(&mut self) {
        self.tape.eject();
    }

    /// Rewind the tape to the start.
    pub fn rewind_tape(&mut self) {
        self.tape.rewind();
    }

    /// Reference to the tape deck.
    #[must_use]
    pub fn tape(&self) -> &TapeDeck {
        &self.tape
    }

    /// Check for and handle the ROM tape-loading trap.
    ///
    /// The Spectrum ROM's `LD-BYTES` routine at $0556 is the standard entry
    /// point for loading data from tape. Instead of emulating tape signal
    /// timing, we intercept this address and copy data directly from the
    /// TAP file into memory.
    ///
    /// Register conventions on entry to LD-BYTES:
    ///   A  = expected flag byte ($00 for header, $FF for data)
    ///   DE = number of bytes expected
    ///   IX = destination address in memory
    ///   Carry flag = set for LOAD, clear for VERIFY
    ///
    /// On success, we set Carry flag and return to the caller by popping
    /// the return address from the stack.
    fn check_tape_trap(&mut self) {
        if self.cpu.regs.pc != LD_BYTES_ADDR || !self.tape.is_loaded() {
            return;
        }

        let expected_flag = self.cpu.regs.a;
        let byte_count = self.cpu.regs.de() as usize;
        let dest_addr = self.cpu.regs.ix;
        let is_load = self.cpu.regs.f & 0x01 != 0; // Carry flag

        // Get the next block from the tape
        let Some(block) = self.tape.next_block() else {
            // No more blocks — let the ROM routine run (it will time out)
            return;
        };

        // Check flag byte matches
        if block.flag != expected_flag {
            // Flag mismatch — ROM would report "Tape loading error"
            // Clear carry to indicate failure, pop return address, jump back
            self.cpu.regs.f &= !0x01; // Clear carry
            self.pop_ret();
            return;
        }

        if is_load {
            // Copy data from TAP block into memory at IX
            let copy_len = byte_count.min(block.data.len());
            for i in 0..copy_len {
                let addr = dest_addr.wrapping_add(i as u16);
                self.bus.memory.write(addr, block.data[i]);
            }
        }
        // VERIFY mode: we skip the actual comparison (always succeed)

        // Set carry flag to indicate success
        self.cpu.regs.f |= 0x01;

        // Pop return address from stack and redirect PC
        self.pop_ret();
    }

    /// Pop the return address from the stack and redirect the CPU to it.
    fn pop_ret(&mut self) {
        let sp = self.cpu.regs.sp;
        let lo = self.bus.memory.read(sp);
        let hi = self.bus.memory.read(sp.wrapping_add(1));
        let ret_addr = u16::from(lo) | (u16::from(hi) << 8);
        self.cpu.regs.sp = sp.wrapping_add(2);
        self.cpu.force_pc(ret_addr);
    }
}

impl Tickable for Spectrum {
    fn tick(&mut self) {
        self.master_clock += 1;

        // Video ticks at 7 MHz (every 2 crystal ticks)
        if self.master_clock.is_multiple_of(VIDEO_DIVIDER) {
            self.bus.video.tick(&*self.bus.memory);
        }

        // CPU ticks at 3.5 MHz (every 4 crystal ticks)
        if self.master_clock.is_multiple_of(self.cpu_divider) {
            // Check INT from video chip
            if self.bus.video.int_active() {
                self.cpu.interrupt();
            }
            self.cpu.tick(&mut self.bus);
            // ROM trap: intercept tape loading at LD-BYTES ($0556)
            self.check_tape_trap();
            // Sample audio at CPU rate
            self.bus.beeper.sample();
        }
    }
}

impl Observable for Spectrum {
    fn query(&self, path: &str) -> Option<Value> {
        // Route queries to sub-components
        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("ula.") {
            match rest {
                "line" => Some(self.bus.video.line().into()),
                "tstate" => Some(self.bus.video.line_tstate().into()),
                "border" => Some(self.bus.video.border_colour().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("memory.") {
            let addr = if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
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
                "cpu_divider" => Some(self.cpu_divider.into()),
                _ => self.cpu.query(path),
            }
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        &[
            "cpu.<z80_paths>",
            "ula.line",
            "ula.tstate",
            "ula.border",
            "memory.<address>",
            "master_clock",
            "cpu_divider",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SpectrumConfig, SpectrumModel};

    fn make_spectrum() -> Spectrum {
        // Use a minimal ROM that just halts: DI; HALT
        let mut rom = vec![0u8; 0x4000];
        rom[0] = 0xF3; // DI
        rom[1] = 0x76; // HALT
        Spectrum::new(&SpectrumConfig {
            model: SpectrumModel::Spectrum48K,
            rom,
        })
    }

    #[test]
    fn master_clock_advances() {
        let mut spec = make_spectrum();
        assert_eq!(spec.master_clock(), 0);
        spec.tick();
        assert_eq!(spec.master_clock(), 1);
    }

    #[test]
    fn run_frame_returns_tstate_count() {
        let mut spec = make_spectrum();
        let tstates = spec.run_frame();
        // Should be close to 69888 (may vary by a few due to instruction
        // boundaries not aligning exactly with frame boundaries)
        assert!(
            tstates >= 69_888 && tstates <= 69_900,
            "Expected ~69888 T-states, got {tstates}"
        );
    }

    #[test]
    fn framebuffer_correct_size() {
        let spec = make_spectrum();
        assert_eq!(spec.framebuffer_width(), 320);
        assert_eq!(spec.framebuffer_height(), 288);
        assert_eq!(spec.framebuffer().len(), 320 * 288);
    }

    #[test]
    fn observable_cpu_pc() {
        let spec = make_spectrum();
        let pc = spec.query("cpu.pc");
        assert_eq!(pc, Some(Value::U16(0)));
    }

    #[test]
    fn observable_ula() {
        let spec = make_spectrum();
        assert!(spec.query("ula.line").is_some());
        assert!(spec.query("ula.tstate").is_some());
        assert!(spec.query("ula.border").is_some());
    }

    #[test]
    fn observable_memory() {
        let mut spec = make_spectrum();
        assert_eq!(spec.query("memory.0x0000"), Some(Value::U8(0xF3)));

        spec.bus.memory.write(0x8000, 0xAB);
        assert_eq!(spec.query("memory.0x8000"), Some(Value::U8(0xAB)));
    }
}
