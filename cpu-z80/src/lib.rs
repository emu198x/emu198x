//! Z80 CPU emulator.

use emu_core::{Cpu, IoBus};

mod flags;
mod registers;

/// The Z80 CPU state.
pub struct Z80 {
    // Main registers
    a: u8,
    f: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    h: u8,
    l: u8,

    // Shadow registers
    a_shadow: u8,
    f_shadow: u8,
    b_shadow: u8,
    c_shadow: u8,
    d_shadow: u8,
    e_shadow: u8,
    h_shadow: u8,
    l_shadow: u8,

    // Index registers
    ix: u16,
    iy: u16,

    // Other registers
    sp: u16,
    pc: u16,
    i: u8,
    r: u8,

    // Interrupt state
    iff1: bool,
    iff2: bool,
    interrupt_mode: u8,
}

impl Z80 {
    pub fn new() -> Self {
        Self {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            a_shadow: 0,
            f_shadow: 0,
            b_shadow: 0,
            c_shadow: 0,
            d_shadow: 0,
            e_shadow: 0,
            h_shadow: 0,
            l_shadow: 0,
            ix: 0,
            iy: 0,
            sp: 0xFFFF,
            pc: 0,
            i: 0,
            r: 0,
            iff1: false,
            iff2: false,
            interrupt_mode: 0,
        }
    }

    fn fetch(&mut self, bus: &impl emu_core::Bus) -> u8 {
        let byte = bus.read(self.pc as u32);
        self.pc = self.pc.wrapping_add(1);
        byte
    }
}

impl<B: IoBus> Cpu<B> for Z80 {
    fn step(&mut self, bus: &mut B) -> u32 {
        let opcode = self.fetch(bus);

        match opcode {
            0x00 => 4, // NOP
            0x3E => {
                // LD A, n
                self.a = self.fetch(bus);
                7
            }
            // LD r, r' (including (HL) cases, excluding HALT)
            op if (op & 0b11000000) == 0b01000000 => {
                let dst = (op >> 3) & 0b111;
                let src = op & 0b111;

                // 0x76 is HALT, not LD (HL), (HL)
                if dst == 6 && src == 6 {
                    todo!("HALT")
                }

                let value = if src == 6 {
                    bus.read(self.hl() as u32)
                } else {
                    self.read_register(src)
                };

                if dst == 6 {
                    bus.write(self.hl() as u32, value);
                } else {
                    self.set_register(dst, value);
                }

                if src == 6 || dst == 6 { 7 } else { 4 }
            }
            // ALU operations: ADD A, r
            op if (op & 0b11111000) == (0b10000000) => {
                let src = op & 0b111;
                let value = if src == 6 {
                    bus.read(self.hl() as u32)
                } else {
                    self.read_register(src)
                };
                self.add_a(value);
                if src == 6 { 7 } else { 4 }
            }
            _ => todo!("opcode {:#04X}", opcode),
        }
    }

    fn reset(&mut self, _bus: &mut B) {
        self.pc = 0;
        self.i = 0;
        self.r = 0;
        self.iff1 = false;
        self.iff2 = false;
        self.interrupt_mode = 0;

        // SP, AF, BC, DE, HL, IX, IY left unchanged (undefined)
    }

    fn interrupt(&mut self, _bus: &mut B) {
        todo!()
    }

    fn nmi(&mut self, _bus: &mut B) {
        todo!()
    }

    fn pc(&self) -> u16 {
        self.pc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal bus for testing.
    struct TestBus {
        memory: [u8; 65536],
    }

    impl TestBus {
        fn new() -> Self {
            Self { memory: [0; 65536] }
        }
    }

    impl emu_core::Bus for TestBus {
        fn read(&self, address: u32) -> u8 {
            self.memory[address as usize & 0xFFFF]
        }

        fn write(&mut self, address: u32, value: u8) {
            self.memory[address as usize & 0xFFFF] = value;
        }
    }

    impl emu_core::IoBus for TestBus {
        fn read_io(&self, _port: u16) -> u8 {
            0xFF
        }

        fn write_io(&mut self, _port: u16, _value: u8) {
            // ignore
        }
    }

    #[test]
    fn reset_sets_correct_state() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::new();

        // Mess up the state first
        cpu.pc = 0x1234;
        cpu.i = 0xFF;
        cpu.r = 0xFF;
        cpu.iff1 = true;
        cpu.iff2 = true;
        cpu.interrupt_mode = 2;

        cpu.reset(&mut bus);

        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.i, 0);
        assert_eq!(cpu.r, 0);
        assert_eq!(cpu.iff1, false);
        assert_eq!(cpu.iff2, false);
        assert_eq!(cpu.interrupt_mode, 0);
    }

    #[test]
    fn nop_takes_4_cycles() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::new();

        bus.memory[0] = 0x00; // NOP

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc, 1);
    }

    #[test]
    fn ld_a_n_loads_immediate() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::new();

        bus.memory[0] = 0x3E; // LD A, n
        bus.memory[1] = 0x42; // n = 0x42

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 7);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.pc, 2);
    }

    #[test]
    fn ld_b_c_copies_register() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::new();

        cpu.c = 0x42;
        bus.memory[0] = 0x41; // LD B, C

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.b, 0x42);
        assert_eq!(cpu.c, 0x42);
    }

    #[test]
    fn ld_a_hl_reads_memory() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::new();

        cpu.h = 0x40;
        cpu.l = 0x00;
        bus.memory[0x4000] = 0x42;
        bus.memory[0] = 0x7E; // LD A, (HL)

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 7);
        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn add_a_b_adds_registers() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::new();

        cpu.a = 0x10;
        cpu.b = 0x05;
        bus.memory[0] = 0x80; // ADD A, B

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.a, 0x15);
    }
}
