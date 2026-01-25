//! Commodore 64 emulator.

use crate::input;
use crate::memory::Memory;
use crate::sid::Sid;
use crate::vic::{self, DISPLAY_HEIGHT, DISPLAY_WIDTH, Vic};
use cpu_6502::Mos6502;
use emu_core::{AudioConfig, Cpu, JoystickState, KeyCode, Machine, VideoConfig};

/// Cycles per frame (PAL: 985248 Hz / 50 Hz = 19656 cycles)
pub const CYCLES_PER_FRAME: u32 = 19656;

/// Audio sample rate
pub const SAMPLE_RATE: u32 = 44100;

/// Samples per frame at 50Hz
pub const SAMPLES_PER_FRAME: usize = 882;

/// CPU clock speed (PAL)
const CPU_CLOCK: u32 = 985248;

/// Commodore 64 emulator.
pub struct C64 {
    cpu: Mos6502,
    memory: Memory,
    vic: Vic,
    sid: Sid,
    frame_cycles: u32,
}

impl C64 {
    pub fn new() -> Self {
        let mut c64 = Self {
            cpu: Mos6502::new(),
            memory: Memory::new(),
            vic: Vic::new(),
            sid: Sid::new(),
            frame_cycles: 0,
        };
        c64.memory.reset();
        c64
    }

    /// Load BASIC ROM.
    pub fn load_basic(&mut self, data: &[u8]) {
        self.memory.load_basic(data);
    }

    /// Load KERNAL ROM.
    pub fn load_kernal(&mut self, data: &[u8]) {
        self.memory.load_kernal(data);
    }

    /// Load Character ROM.
    pub fn load_chargen(&mut self, data: &[u8]) {
        self.memory.load_chargen(data);
    }

    /// Load a PRG file.
    pub fn load_prg(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() < 2 {
            return Err("PRG file too short".to_string());
        }

        // First two bytes are load address (little-endian)
        let load_addr = u16::from_le_bytes([data[0], data[1]]);
        let program = &data[2..];

        // Load into RAM
        for (i, &byte) in program.iter().enumerate() {
            let addr = load_addr.wrapping_add(i as u16);
            self.memory.ram[addr as usize] = byte;
        }

        // Update BASIC pointers if loaded at $0801 (standard BASIC start)
        if load_addr == 0x0801 {
            let end_addr = load_addr.wrapping_add(program.len() as u16);
            // Set end of BASIC (VARTAB)
            self.memory.ram[0x2D] = (end_addr & 0xFF) as u8;
            self.memory.ram[0x2E] = (end_addr >> 8) as u8;
            // Set end of variables (ARYTAB)
            self.memory.ram[0x2F] = (end_addr & 0xFF) as u8;
            self.memory.ram[0x30] = (end_addr >> 8) as u8;
            // Set end of arrays (STREND)
            self.memory.ram[0x31] = (end_addr & 0xFF) as u8;
            self.memory.ram[0x32] = (end_addr >> 8) as u8;
        }

        Ok(())
    }

    /// Run one frame of emulation.
    fn run_frame_internal(&mut self) {
        self.frame_cycles = 0;
        self.memory.cycles = 0;
        self.vic.reset_frame();

        while self.frame_cycles < CYCLES_PER_FRAME {
            // Tick VIC first - it may steal cycles via badlines
            let ba_low = self.vic.tick(&self.memory.vic_registers);

            if !ba_low {
                // CPU only runs when bus is available (BA high)
                let prev_cycles = self.memory.cycles;
                self.cpu.step(&mut self.memory);
                let cpu_cycles = self.memory.cycles - prev_cycles;

                // Process pending SID register writes
                for (reg, value) in self.memory.sid_writes.drain(..) {
                    self.sid.write(reg, value);
                }

                // Tick SID oscillators and envelopes
                self.sid.tick(cpu_cycles);

                // Update readable SID registers
                self.memory.sid_registers[0x1B] = self.sid.read(0x1B);
                self.memory.sid_registers[0x1C] = self.sid.read(0x1C);

                // Tick CIA1 timers and check for IRQ
                if self.memory.tick_cia1(cpu_cycles) {
                    self.cpu.interrupt(&mut self.memory);
                }

                // Tick CIA2 timers and check for NMI
                if self.memory.tick_cia2(cpu_cycles) {
                    self.cpu.nmi(&mut self.memory);
                }

                // Advance frame_cycles by CPU cycles (VIC already ticked once)
                // We need to sync frame_cycles with actual elapsed time
                if cpu_cycles > 1 {
                    // Catch up VIC ticks for multi-cycle instructions
                    for _ in 1..cpu_cycles {
                        self.vic.tick(&self.memory.vic_registers);
                    }
                }
            }

            self.frame_cycles = self.vic.frame_cycle;

            // Check for VIC-II raster interrupt at start of each line
            if self.vic.frame_cycle % 63 == 0 && self.vic.check_irq(&self.memory) {
                self.memory.vic_registers[0x19] |= 0x01; // Set raster IRQ flag
                self.cpu.interrupt(&mut self.memory);
            }
        }
    }

    /// Press a key in the keyboard matrix.
    fn press_key(&mut self, col: usize, row: usize) {
        if col < 8 && row < 8 {
            self.memory.keyboard_matrix[col] &= !(1 << row);
        }
    }

    /// Release a key in the keyboard matrix.
    fn release_key(&mut self, col: usize, row: usize) {
        if col < 8 && row < 8 {
            self.memory.keyboard_matrix[col] |= 1 << row;
        }
    }
}

impl Default for C64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for C64 {
    fn video_config(&self) -> VideoConfig {
        VideoConfig {
            width: DISPLAY_WIDTH,
            height: DISPLAY_HEIGHT,
            fps: 50.0,
        }
    }

    fn audio_config(&self) -> AudioConfig {
        AudioConfig {
            sample_rate: SAMPLE_RATE,
            samples_per_frame: SAMPLES_PER_FRAME,
        }
    }

    fn run_frame(&mut self) {
        self.run_frame_internal();
    }

    fn render(&self, buffer: &mut [u8]) {
        vic::render(&self.memory, buffer);
    }

    fn generate_audio(&mut self, buffer: &mut [f32]) {
        self.sid.generate_samples(buffer, CPU_CLOCK, SAMPLE_RATE);
    }

    fn key_down(&mut self, key: KeyCode) {
        // Handle keys that need shift on C64
        if input::needs_shift(key) {
            self.press_key(1, 7); // Left shift
        }

        if let Some((col, row)) = input::map_key(key) {
            self.press_key(col, row);
        }
    }

    fn key_up(&mut self, key: KeyCode) {
        // Release shift for keys that needed it
        if input::needs_shift(key) {
            self.release_key(1, 7);
        }

        if let Some((col, row)) = input::map_key(key) {
            self.release_key(col, row);
        }
    }

    fn set_joystick(&mut self, port: u8, state: JoystickState) {
        // Joystick 1 is on CIA1 port A, Joystick 2 on CIA1 port B
        // Active low: 0 = pressed
        let mut value = 0xFF;

        if state.up {
            value &= !0x01;
        }
        if state.down {
            value &= !0x02;
        }
        if state.left {
            value &= !0x04;
        }
        if state.right {
            value &= !0x08;
        }
        if state.fire {
            value &= !0x10;
        }

        match port {
            0 => {
                // Port 2 (primary) uses CIA1 port A
                self.memory.cia1.pra = (self.memory.cia1.pra & 0xE0) | (value & 0x1F);
            }
            1 => {
                // Port 1 uses CIA1 port B
                self.memory.cia1.prb = (self.memory.cia1.prb & 0xE0) | (value & 0x1F);
            }
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.memory.reset();
        self.vic.reset();
        self.sid.reset();
        self.cpu.reset(&mut self.memory);
        self.frame_cycles = 0;
    }

    fn load_file(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        let lower = path.to_lowercase();

        if lower.ends_with(".prg") {
            self.load_prg(data)
        } else if lower.ends_with("basic.bin") || lower.ends_with("basic.rom") {
            self.load_basic(data);
            Ok(())
        } else if lower.ends_with("kernal.bin") || lower.ends_with("kernal.rom") {
            self.load_kernal(data);
            Ok(())
        } else if lower.ends_with("chargen.bin") || lower.ends_with("chargen.rom") {
            self.load_chargen(data);
            Ok(())
        } else {
            Err(format!("Unknown file type: {}", path))
        }
    }
}
