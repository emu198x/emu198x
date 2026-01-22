//! ZX Spectrum 48K emulator.

use cpu_z80::Z80;
use emu_core::{Bus, Cpu, IoBus};

struct Memory {
    pub data: [u8; 65536],
    pub border: u8,
}

impl Memory {
    fn new() -> Self {
        Self {
            data: [0; 65536],
            border: 7, // white default
        }
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

    fn write_io(&mut self, port: u16, value: u8) {
        if port & 0x01 == 0 {
            // ULA port - low bit clear
            self.border = value & 0x07;
        }
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

    pub fn border(&self) -> u8 {
        self.memory.border
    }

    /// Load bytes into memory at a given address.
    pub fn load(&mut self, address: u16, data: &[u8]) {
        for (i, byte) in data.iter().enumerate() {
            self.memory.data[address as usize + i] = *byte;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_screen_memory() {
        let mut spec = Spectrum48K::new();

        // Program:
        // 0000: LD HL, 0x4000
        // 0003: LD A, 0xFF
        // 0005: LD (HL), A
        // 0006: INC HL
        // 0007: JP 0x0005
        spec.load(
            0x0000,
            &[
                0x21, 0x00, 0x40, // LD HL, 0x4000
                0x3E, 0xFF, // LD A, 0xFF
                0x77, // LD (HL), A
                0x23, // INC HL
                0xC3, 0x05, 0x00, // JP 0x0005
            ],
        );

        // Run for a while
        for _ in 0..10000 {
            spec.cpu.step(&mut spec.memory);
        }

        // Check first few bytes of screen memory
        assert_eq!(spec.memory.data[0x4000], 0xFF);
        assert_eq!(spec.memory.data[0x4001], 0xFF);
        assert_eq!(spec.memory.data[0x4002], 0xFF);
    }
}
