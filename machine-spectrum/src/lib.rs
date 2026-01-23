//! ZX Spectrum 48K emulator.

use cpu_z80::Z80;
use emu_core::{Bus, Cpu, IoBus};

struct Memory {
    pub data: [u8; 65536],
    pub border: u8,
    pub keyboard: [u8; 8],
    /// Current T-state within the frame (0..69887)
    pub frame_t_state: u32,
}

impl Memory {
    fn new() -> Self {
        Self {
            data: [0; 65536],
            border: 7,           // white default
            keyboard: [0xFF; 8], // all keys released
            frame_t_state: 0,
        }
    }

    /// Check if an address is in contended memory (0x4000-0x7FFF).
    fn is_contended(&self, address: u32) -> bool {
        let addr = address & 0xFFFF;
        addr >= 0x4000 && addr < 0x8000
    }

    /// Calculate contention delay based on current frame position.
    ///
    /// The ULA reads screen memory every 8 T-states during the display period.
    /// If the CPU tries to access contended memory, it's delayed until the
    /// next available slot.
    fn contention_delay(&self) -> u32 {
        // The Spectrum 48K has 64 T-states per scanline, 312 scanlines per frame.
        // The screen is displayed on scanlines 64-255 (192 lines).
        // During each scanline, contention applies for the first 128 T-states
        // (the visible portion).

        let t_state = self.frame_t_state % 69888;

        // Scanline calculation: 64 T-states per line
        let scanline = t_state / 224; // 224 T-states per line (128 screen + 96 border/retrace)
        let line_t_state = t_state % 224;

        // Contention only during display period (lines 64-255) and first 128 T-states of line
        if scanline >= 64 && scanline < 256 && line_t_state < 128 {
            // Contention pattern repeats every 8 T-states
            let pattern_pos = line_t_state % 8;
            match pattern_pos {
                0 => 6,
                1 => 5,
                2 => 4,
                3 => 3,
                4 => 2,
                5 => 1,
                6 | 7 => 0,
                _ => unreachable!(),
            }
        } else {
            0
        }
    }

    /// Apply contention if accessing contended memory, and advance clock.
    fn apply_contention(&mut self, address: u32) {
        if self.is_contended(address) {
            let delay = self.contention_delay();
            self.frame_t_state += delay;
        }
    }
}

impl Bus for Memory {
    fn read(&mut self, address: u32) -> u8 {
        self.apply_contention(address);
        self.frame_t_state += 3; // Memory read takes 3 T-states
        self.data[(address & 0xFFFF) as usize]
    }

    fn write(&mut self, address: u32, value: u8) {
        self.apply_contention(address);
        self.frame_t_state += 3; // Memory write takes 3 T-states
        let addr = (address & 0xFFFF) as usize;
        if addr >= 0x4000 {
            // Only write to RAM, not ROM
            self.data[addr] = value;
        }
    }

    fn tick(&mut self, cycles: u32) {
        self.frame_t_state += cycles;
    }
}

impl IoBus for Memory {
    fn read_io(&mut self, port: u16) -> u8 {
        // I/O contention: ULA ports (bit 0 = 0) are contended
        if port & 0x01 == 0 {
            let delay = self.contention_delay();
            self.frame_t_state += delay;
        }
        self.frame_t_state += 4; // I/O read takes 4 T-states

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
        // I/O contention: ULA ports (bit 0 = 0) are contended
        if port & 0x01 == 0 {
            let delay = self.contention_delay();
            self.frame_t_state += delay;
        }
        self.frame_t_state += 4; // I/O write takes 4 T-states

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
        // Reset frame counter at start of frame
        self.memory.frame_t_state = 0;

        self.cpu.interrupt(&mut self.memory);

        while self.memory.frame_t_state < 69888 {
            // Check for tape trap
            if self.cpu.pc() == 0x0556 {
                self.handle_tape_load();
            }

            self.cpu.step(&mut self.memory);
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

    // Contention model tests

    #[test]
    fn is_contended_identifies_correct_range() {
        let mem = Memory::new();

        // ROM is not contended
        assert!(!mem.is_contended(0x0000));
        assert!(!mem.is_contended(0x3FFF));

        // Screen memory and first 16K of RAM is contended
        assert!(mem.is_contended(0x4000));
        assert!(mem.is_contended(0x5800)); // attributes
        assert!(mem.is_contended(0x7FFF));

        // Upper RAM is not contended
        assert!(!mem.is_contended(0x8000));
        assert!(!mem.is_contended(0xFFFF));
    }

    #[test]
    fn no_contention_in_top_border() {
        let mut mem = Memory::new();

        // Top border is scanlines 0-63 (T-states 0 to 14335)
        // Test at start of frame
        mem.frame_t_state = 0;
        assert_eq!(mem.contention_delay(), 0);

        // Test in middle of top border
        mem.frame_t_state = 7000;
        assert_eq!(mem.contention_delay(), 0);

        // Test at end of top border (line 63, last T-state)
        mem.frame_t_state = 64 * 224 - 1; // 14335
        assert_eq!(mem.contention_delay(), 0);
    }

    #[test]
    fn contention_during_display_period() {
        let mut mem = Memory::new();

        // Display period is scanlines 64-255
        // First display scanline starts at T-state 14336 (64 * 224)
        let display_start = 64 * 224;

        // At position 0 in pattern: 6 T-states delay
        mem.frame_t_state = display_start;
        assert_eq!(mem.contention_delay(), 6);

        // At position 1: 5 T-states delay
        mem.frame_t_state = display_start + 1;
        assert_eq!(mem.contention_delay(), 5);

        // At position 2: 4 T-states delay
        mem.frame_t_state = display_start + 2;
        assert_eq!(mem.contention_delay(), 4);

        // At position 3: 3 T-states delay
        mem.frame_t_state = display_start + 3;
        assert_eq!(mem.contention_delay(), 3);

        // At position 4: 2 T-states delay
        mem.frame_t_state = display_start + 4;
        assert_eq!(mem.contention_delay(), 2);

        // At position 5: 1 T-state delay
        mem.frame_t_state = display_start + 5;
        assert_eq!(mem.contention_delay(), 1);

        // At positions 6 and 7: 0 T-states delay
        mem.frame_t_state = display_start + 6;
        assert_eq!(mem.contention_delay(), 0);
        mem.frame_t_state = display_start + 7;
        assert_eq!(mem.contention_delay(), 0);

        // Pattern repeats at position 8
        mem.frame_t_state = display_start + 8;
        assert_eq!(mem.contention_delay(), 6);
    }

    #[test]
    fn no_contention_in_right_border() {
        let mut mem = Memory::new();

        // During display lines, T-states 128-223 are border/retrace (no contention)
        let display_start = 64 * 224;

        // First T-state after display area on first display line
        mem.frame_t_state = display_start + 128;
        assert_eq!(mem.contention_delay(), 0);

        // Middle of border period
        mem.frame_t_state = display_start + 175;
        assert_eq!(mem.contention_delay(), 0);

        // Last T-state of border period
        mem.frame_t_state = display_start + 223;
        assert_eq!(mem.contention_delay(), 0);
    }

    #[test]
    fn no_contention_in_bottom_border() {
        let mut mem = Memory::new();

        // Bottom border starts at scanline 256
        let bottom_border_start = 256 * 224;

        mem.frame_t_state = bottom_border_start;
        assert_eq!(mem.contention_delay(), 0);

        mem.frame_t_state = bottom_border_start + 50;
        assert_eq!(mem.contention_delay(), 0);

        // End of frame
        mem.frame_t_state = 69887;
        assert_eq!(mem.contention_delay(), 0);
    }

    #[test]
    fn contended_read_adds_delay() {
        let mut mem = Memory::new();

        // Position in display area with maximum contention (pattern position 0)
        let display_start = 64 * 224;
        mem.frame_t_state = display_start;

        // Read from contended memory should add 6 (contention) + 3 (read) = 9 T-states
        mem.read(0x4000);
        assert_eq!(mem.frame_t_state, display_start + 9);
    }

    #[test]
    fn uncontended_read_no_delay() {
        let mut mem = Memory::new();

        // Position in display area
        let display_start = 64 * 224;
        mem.frame_t_state = display_start;

        // Read from ROM (uncontended) should add only 3 T-states
        mem.read(0x0000);
        assert_eq!(mem.frame_t_state, display_start + 3);
    }

    #[test]
    fn contended_write_adds_delay() {
        let mut mem = Memory::new();

        // Position in display area with maximum contention
        let display_start = 64 * 224;
        mem.frame_t_state = display_start;

        // Write to contended memory should add 6 (contention) + 3 (write) = 9 T-states
        mem.write(0x4000, 0xFF);
        assert_eq!(mem.frame_t_state, display_start + 9);
    }

    #[test]
    fn contention_outside_display_period_read() {
        let mut mem = Memory::new();

        // During top border, even contended addresses shouldn't have delay
        mem.frame_t_state = 100;
        mem.read(0x4000);
        assert_eq!(mem.frame_t_state, 103); // Just the 3 T-state read, no contention
    }

    #[test]
    fn io_ula_port_contended_during_display() {
        let mut mem = Memory::new();

        // Position in display area with maximum contention
        let display_start = 64 * 224;
        mem.frame_t_state = display_start;

        // ULA port (bit 0 = 0) should be contended: 6 (contention) + 4 (I/O) = 10 T-states
        mem.read_io(0xFE); // 0xFE has bit 0 = 0, so it's a ULA port
        assert_eq!(mem.frame_t_state, display_start + 10);
    }

    #[test]
    fn io_non_ula_port_not_contended() {
        let mut mem = Memory::new();

        // Position in display area with maximum contention
        let display_start = 64 * 224;
        mem.frame_t_state = display_start;

        // Non-ULA port (bit 0 = 1) should not be contended: just 4 T-states for I/O
        mem.read_io(0xFF); // 0xFF has bit 0 = 1, so not a ULA port
        assert_eq!(mem.frame_t_state, display_start + 4);
    }

    #[test]
    fn frame_t_state_wraps_correctly() {
        let mut mem = Memory::new();

        // Set to end of frame
        mem.frame_t_state = 69880;

        // Read should wrap around
        mem.read(0x0000); // 3 T-states
        assert_eq!(mem.frame_t_state, 69883);

        // The contention_delay calculation should handle values >= 69888
        mem.frame_t_state = 69890; // 2 T-states into "next" frame
        // This is like being at T-state 2, which is in top border
        assert_eq!(mem.contention_delay(), 0);
    }
}
