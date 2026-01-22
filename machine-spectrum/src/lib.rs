//! ZX Spectrum 48K emulator.

use cpu_z80::Z80;
use emu_core::{Bus, Cpu, IoBus};

struct Memory {
    data: [u8; 65536],
}

impl Memory {
    fn new() -> Self {
        Self { data: [0; 65536] }
    }
}

impl Bus for Memory {
    fn read(&self, address: u32) -> u8 {
        self.data[(address & 0xFFFF) as usize]
    }

    fn write(&mut self, address: u32, value: u8) {
        self.data[(address & 0xFFFF) as usize] = value;
    }
}

impl IoBus for Memory {
    fn read_io(&self, _port: u16) -> u8 {
        0xFF // nothing connected
    }

    fn write_io(&mut self, _port: u16, _value: u8) {
        // ignore for now
    }
}

pub struct Spectrum48K {
    cpu: Z80,
    memory: Memory,
}

impl Spectrum48K {
    pub fn new() -> Self {
        Self {
            cpu: Z80::new(),
            memory: Memory::new(),
        }
    }

    /// Run the CPU for approximately one frame's worth of cycles.
    pub fn run_frame(&mut self) {
        let mut cycles = 0;
        while cycles < 69888 {
            cycles += self.cpu.step(&mut self.memory);
        }
    }

    /// Get a reference to screen memory (for rendering).
    pub fn screen(&self) -> &[u8] {
        &self.memory.data[0x4000..0x5B00]
    }
}
