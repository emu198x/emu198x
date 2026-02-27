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
use sinclair_ula::Ula;
use zilog_z80::Z80;

use crate::beeper::BeeperState;
use crate::bus::SpectrumBus;
use crate::config::{SpectrumConfig, SpectrumModel};
use crate::input::{InputQueue, SpectrumKey};
use crate::memory::{Memory128K, Memory48K, SpectrumMemory};
use crate::tap::TapFile;
use crate::tape::TapeDeck;
use crate::tzx::TzxFile;
use crate::tzx_signal::TzxSignal;

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
    /// Virtual tape deck for TAP loading (ROM trap / instant load).
    tape: TapeDeck,
    /// AY clock toggle (ticks every other CPU T-state).
    ay_toggle: bool,
    /// Spectrum model (stored for TZX 48K detection).
    model: SpectrumModel,
    /// TZX signal generator for real-time tape loading.
    tzx_signal: Option<TzxSignal>,
}

impl Spectrum {
    /// Create a new Spectrum from the given configuration.
    ///
    /// # Panics
    ///
    /// Panics if the model is not yet supported or the ROM size is wrong.
    #[must_use]
    pub fn new(config: &SpectrumConfig) -> Self {
        let memory: Box<dyn SpectrumMemory> = match config.model {
            SpectrumModel::Spectrum48K => Box::new(Memory48K::new(&config.rom)),
            SpectrumModel::Spectrum128K | SpectrumModel::SpectrumPlus2 => {
                Box::new(Memory128K::new(&config.rom))
            }
            other => panic!("Model {other:?} is not yet supported"),
        };

        let has_ay = matches!(
            config.model,
            SpectrumModel::Spectrum128K | SpectrumModel::SpectrumPlus2
        );

        let ula = Ula::new();
        let beeper = BeeperState::new(CPU_FREQUENCY, AUDIO_SAMPLE_RATE);
        let mut bus = SpectrumBus::new(memory, ula, beeper);
        if has_ay {
            bus.enable_ay(CPU_FREQUENCY, AUDIO_SAMPLE_RATE);
            if let Some(ay) = &mut bus.ay {
                ay.set_stereo(gi_ay_3_8910::StereoMode::Acb);
            }
        }

        Self {
            cpu: Z80::new(),
            bus,
            master_clock: 0,
            cpu_divider: CPU_DIVIDER,
            frame_count: 0,
            input_queue: InputQueue::new(),
            tape: TapeDeck::new(),
            ay_toggle: false,
            model: config.model,
            tzx_signal: None,
        }
    }

    /// Run one complete frame (until the ULA signals frame complete).
    ///
    /// Processes any pending input queue events at the start of the frame,
    /// then ticks the master clock until the ULA signals frame complete.
    ///
    /// Returns the number of CPU T-states executed during the frame.
    pub fn run_frame(&mut self) -> u64 {
        self.input_queue.process(
            self.frame_count,
            &mut self.bus.keyboard,
            &mut self.bus.kempston,
        );
        self.frame_count += 1;

        let start_ticks = self.cpu.total_ticks();

        loop {
            self.tick();
            if self.bus.ula.take_frame_complete() {
                break;
            }
        }

        (self.cpu.total_ticks() - start_ticks).get()
    }

    /// Reference to the framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.ula.framebuffer()
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.bus.ula.framebuffer_width()
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.bus.ula.framebuffer_height()
    }

    /// Take the mixed audio buffer (beeper + AY if present). Drains both.
    ///
    /// Returns stereo samples as `[left, right]` pairs. The beeper is mono
    /// (duplicated to both channels); the AY provides stereo via ACB panning.
    pub fn take_audio_buffer(&mut self) -> Vec<[f32; 2]> {
        let beeper = self.bus.beeper.take_buffer();
        if let Some(ay) = &mut self.bus.ay {
            let ay_buf = ay.take_buffer();
            let len = beeper.len().min(ay_buf.len());
            let mut out = Vec::with_capacity(len);
            for i in 0..len {
                let b = beeper[i];
                out.push([
                    (b + ay_buf[i][0]) * 0.5,
                    (b + ay_buf[i][1]) * 0.5,
                ]);
            }
            out
        } else {
            // No AY — beeper only, duplicate mono to stereo.
            beeper.into_iter().map(|s| [s, s]).collect()
        }
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
        if let Some(bit) = key.kempston_bit() {
            self.bus.kempston |= 1 << bit;
        } else {
            let (row, bit) = key.matrix();
            self.bus.keyboard.set_key(row, bit, true);
        }
    }

    /// Release a key.
    pub fn release_key(&mut self, key: SpectrumKey) {
        if let Some(bit) = key.kempston_bit() {
            self.bus.kempston &= !(1 << bit);
        } else {
            let (row, bit) = key.matrix();
            self.bus.keyboard.set_key(row, bit, false);
        }
    }

    /// Release all keys.
    pub fn release_all_keys(&mut self) {
        self.bus.keyboard.release_all();
        self.bus.kempston = 0;
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

    /// Insert a TZX file and start playback.
    pub fn insert_tzx(&mut self, tzx: TzxFile) {
        let is_48k = self.model == SpectrumModel::Spectrum48K;
        let mut signal = TzxSignal::new(tzx.blocks, is_48k, CPU_FREQUENCY);
        signal.play();
        self.tzx_signal = Some(signal);
    }

    /// Eject the TZX tape and restore MIC loopback.
    pub fn eject_tzx(&mut self) {
        self.tzx_signal = None;
        self.bus.tape_ear = None;
    }

    /// Whether a TZX signal is currently playing.
    #[must_use]
    pub fn is_tzx_playing(&self) -> bool {
        self.tzx_signal
            .as_ref()
            .is_some_and(|s| s.is_playing())
    }

    /// The Spectrum model.
    #[must_use]
    pub fn model(&self) -> SpectrumModel {
        self.model
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
            // No more blocks -- let the ROM routine run (it will time out)
            return;
        };

        // Check flag byte matches
        if block.flag != expected_flag {
            // Flag mismatch -- ROM would report "Tape loading error"
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
            let mem = &*self.bus.memory;
            self.bus.ula.tick(|addr| mem.vram_peek(addr));
        }

        // CPU ticks at 3.5 MHz (every 4 crystal ticks)
        if self.master_clock.is_multiple_of(self.cpu_divider) {
            // Advance TZX signal (one T-state) before CPU tick
            if let Some(ref mut signal) = self.tzx_signal {
                let level = signal.tick();
                self.bus.tape_ear = Some(level);
                if signal.is_finished() {
                    self.bus.tape_ear = None;
                }
            }

            // Check INT from ULA
            if self.bus.ula.int_active() {
                self.cpu.interrupt();
            }
            self.cpu.tick(&mut self.bus);
            // ROM trap: only when no TZX signal is driving the EAR bit.
            // TZX loading uses the ROM's own LD-BYTES via real signal timing,
            // so the trap must not short-circuit it.
            if self.bus.tape_ear.is_none() {
                self.check_tape_trap();
            }
            // Sample audio at CPU rate
            self.bus.beeper.sample();

            // AY clocks at half CPU rate (1.7734 MHz)
            self.ay_toggle = !self.ay_toggle;
            if self.ay_toggle && let Some(ay) = &mut self.bus.ay {
                ay.tick();
            }
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
                "line" => Some(self.bus.ula.line().into()),
                "tstate" => Some(self.bus.ula.line_tstate().into()),
                "border" => Some(self.bus.ula.border_colour().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("memory.") {
            let addr =
                if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
                    u16::from_str_radix(hex, 16).ok()
                } else if let Some(hex) = rest.strip_prefix('$') {
                    u16::from_str_radix(hex, 16).ok()
                } else {
                    rest.parse().ok()
                };
            addr.map(|a| Value::U8(self.bus.memory.peek(a)))
        } else if let Some(rest) = path.strip_prefix("ay.") {
            let ay = self.bus.ay.as_ref()?;
            match rest {
                "buffer_len" => Some(Value::U64(ay.buffer_len() as u64)),
                _ => None,
            }
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
