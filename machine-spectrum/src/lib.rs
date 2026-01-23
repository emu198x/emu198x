//! ZX Spectrum 48K emulator.

use cpu_z80::Z80;
use emu_core::{Bus, Cpu, IoBus};

struct Memory {
    pub data: [u8; 65536],
    pub border: u8,
    pub keyboard: [u8; 8],
}

impl Memory {
    fn new() -> Self {
        Self {
            data: [0; 65536],
            border: 7,           // white default
            keyboard: [0xFF; 8], // all keys released
        }
    }
}

impl Bus for Memory {
    fn read(&self, address: u32) -> u8 {
        self.data[(address & 0xFFFF) as usize]
    }

    fn write(&mut self, address: u32, value: u8) {
        let addr = (address & 0xFFFF) as usize;
        if addr >= 0x4000 {
            // Only write to RAM, not ROM
            self.data[addr] = value;
        }
    }
}

impl IoBus for Memory {
    fn read_io(&self, port: u16) -> u8 {
        if port & 0x01 == 0 {
            // ULA port - keyboard
            let high = (port >> 8) as u8;
            let mut result = 0x1F; // bits 0-4, active low
            for row in 0..8 {
                if high & (1 << row) == 0 {
                    result &= self.keyboard[row];
                }
            }
            result
        } else {
            0xFF
        }
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
    tape_data: Vec<u8>,
    tape_pos: usize,
}

impl Spectrum48K {
    pub fn new() -> Self {
        Self {
            cpu: Z80::new(),
            memory: Memory::new(),
            tape_data: Vec::new(),
            tape_pos: 0,
        }
    }

    /// Run the CPU for approximately one frame's worth of cycles.
    pub fn run_frame(&mut self) {
        self.cpu.interrupt(&mut self.memory);

        let mut cycles = 0;
        while cycles < 69888 {
            // Check for tape trap
            if self.cpu.pc() == 0x0556 {
                self.handle_tape_load();
            }

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

    pub fn load_tape(&mut self, data: Vec<u8>) {
        self.tape_data = data;
        self.tape_pos = 0;
    }

    pub fn load_rom(&mut self, rom: &[u8]) {
        self.memory.data[..rom.len()].copy_from_slice(rom);
    }

    pub fn key_down(&mut self, row: usize, bit: u8) {
        self.memory.keyboard[row] &= !(1 << bit);
    }

    pub fn key_up(&mut self, row: usize, bit: u8) {
        self.memory.keyboard[row] |= 1 << bit;
    }

    pub fn reset_keyboard(&mut self) {
        for row in 0..8 {
            self.memory.keyboard[row] = 0xFF;
        }
    }

    fn handle_tape_load(&mut self) {
        let Some(block) = self.next_tape_block() else {
            self.cpu.set_carry(false);
            self.cpu.force_ret(&mut self.memory);
            return;
        };

        let flag = block[0];
        let expected_flag = self.cpu.a();

        if flag != expected_flag {
            self.cpu.set_carry(false);
            self.cpu.force_ret(&mut self.memory);
            return;
        }

        let ix = self.cpu.ix();
        let de = self.cpu.de();
        let data = &block[1..block.len() - 1];
        let len = (de as usize).min(data.len());

        for i in 0..len {
            self.memory.data[ix.wrapping_add(i as u16) as usize] = data[i];
        }

        self.cpu.set_carry(true);
        self.cpu.force_ret(&mut self.memory);
    }

    fn next_tape_block(&mut self) -> Option<Vec<u8>> {
        if self.tape_pos + 2 > self.tape_data.len() {
            return None;
        }

        let len = self.tape_data[self.tape_pos] as usize
            | (self.tape_data[self.tape_pos + 1] as usize) << 8;

        self.tape_pos += 2;

        if self.tape_pos + len > self.tape_data.len() {
            return None;
        }

        let block = self.tape_data[self.tape_pos..self.tape_pos + len].to_vec();
        self.tape_pos += len;
        Some(block)
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
