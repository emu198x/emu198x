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

    halted: bool,
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
            halted: false,
        }
    }

    pub fn pc(&self) -> u16 {
        self.pc
    }

    pub fn a(&self) -> u8 {
        self.a
    }

    pub fn ix(&self) -> u16 {
        self.ix
    }

    pub fn de(&self) -> u16 {
        (self.d as u16) << 8 | self.e as u16
    }

    pub fn set_carry(&mut self, value: bool) {
        if value {
            self.f |= 0x01;
        } else {
            self.f &= !0x01;
        }
    }

    pub fn force_ret(&mut self, bus: &mut impl emu_core::Bus) {
        let low = bus.read(self.sp as u32);
        self.sp = self.sp.wrapping_add(1);
        let high = bus.read(self.sp as u32);
        self.sp = self.sp.wrapping_add(1);
        self.pc = (high as u16) << 8 | low as u16;
    }

    fn fetch(&mut self, bus: &impl emu_core::Bus) -> u8 {
        let byte = bus.read(self.pc as u32);
        self.pc = self.pc.wrapping_add(1);
        byte
    }
}

impl<B: IoBus> Cpu<B> for Z80 {
    fn step(&mut self, bus: &mut B) -> u32 {
        if self.halted {
            return 4; // NOP cycles while halted
        }

        let opcode = self.fetch(bus);

        match opcode {
            0x00 => 4, // NOP
            0x01 => {
                // LD BC, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                self.set_bc((high as u16) << 8 | low as u16);
                10
            }
            0x02 => {
                // LD (BC), A
                bus.write(self.bc() as u32, self.a);
                7
            }
            0x03 => {
                // INC BC
                self.set_bc(self.bc().wrapping_add(1));
                6
            }
            0x04 => {
                // INC B
                let value = self.b;
                let result = value.wrapping_add(1);
                self.b = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                self.set_flag(flags::FLAG_PV, value == 0x7F);
                self.set_flag(flags::FLAG_N, false);
                4
            }
            0x05 => {
                // DEC B
                let value = self.b;
                let result = value.wrapping_sub(1);
                self.b = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                self.set_flag(flags::FLAG_PV, value == 0x80);
                self.set_flag(flags::FLAG_N, true);
                4
            }
            0x06 => {
                // LD B, n
                self.b = self.fetch(bus);
                7
            }
            0x07 => {
                // RLCA
                let bit7 = self.a >> 7;
                self.a = (self.a << 1) | bit7;
                self.set_flag(flags::FLAG_H, false);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, bit7 != 0);
                4
            }
            0x08 => {
                // EX AF, AF'
                std::mem::swap(&mut self.a, &mut self.a_shadow);
                std::mem::swap(&mut self.f, &mut self.f_shadow);
                4
            }
            0x09 => {
                // ADD HL, BC
                let hl = self.hl();
                let bc = self.bc();
                let result = (hl as u32) + (bc as u32);

                self.set_flag(flags::FLAG_H, (hl & 0x0FFF) + (bc & 0x0FFF) > 0x0FFF);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, result > 0xFFFF);

                self.set_hl(result as u16);
                11
            }
            0x0A => {
                // LD A, (BC)
                self.a = bus.read(self.bc() as u32);
                7
            }
            0x0B => {
                // DEC BC
                self.set_bc(self.bc().wrapping_sub(1));
                6
            }
            0x0C => {
                // INC C
                let value = self.c;
                let result = value.wrapping_add(1);
                self.c = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                self.set_flag(flags::FLAG_PV, value == 0x7F);
                self.set_flag(flags::FLAG_N, false);
                4
            }
            0x0D => {
                // DEC C
                let value = self.c;
                let result = value.wrapping_sub(1);
                self.c = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                self.set_flag(flags::FLAG_PV, value == 0x80);
                self.set_flag(flags::FLAG_N, true);
                4
            }
            0x0E => {
                // LD C, n
                self.c = self.fetch(bus);
                7
            }
            0x0F => {
                // RRCA
                let bit0 = self.a & 0x01;
                self.a = (self.a >> 1) | (bit0 << 7);
                self.set_flag(flags::FLAG_H, false);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, bit0 != 0);
                4
            }
            0x10 => {
                // DJNZ n
                let offset = self.fetch(bus) as i8;
                self.b = self.b.wrapping_sub(1);
                if self.b != 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    13
                } else {
                    8
                }
            }
            0x11 => {
                // LD DE, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                self.set_de((high as u16) << 8 | low as u16);
                10
            }
            0x12 => {
                // LD (DE), A
                bus.write(self.de() as u32, self.a);
                7
            }
            0x13 => {
                // INC DE
                self.set_de(self.de().wrapping_add(1));
                6
            }
            0x14 => {
                // INC D
                let value = self.d;
                let result = value.wrapping_add(1);
                self.d = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                self.set_flag(flags::FLAG_PV, value == 0x7F);
                self.set_flag(flags::FLAG_N, false);
                4
            }
            0x15 => {
                // DEC D
                let value = self.d;
                let result = value.wrapping_sub(1);
                self.d = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                self.set_flag(flags::FLAG_PV, value == 0x80);
                self.set_flag(flags::FLAG_N, true);
                4
            }
            0x16 => {
                // LD D, n
                self.d = self.fetch(bus);
                7
            }
            0x17 => {
                // RLA
                let old_carry = if self.carry() { 1 } else { 0 };
                let bit7 = self.a >> 7;
                self.a = (self.a << 1) | old_carry;
                self.set_flag(flags::FLAG_H, false);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, bit7 != 0);
                4
            }
            0x18 => {
                // JR n
                let offset = self.fetch(bus) as i8;
                self.pc = self.pc.wrapping_add(offset as u16);
                12
            }
            0x19 => {
                // ADD HL, DE
                let hl = self.hl();
                let de = self.de();
                let result = (hl as u32) + (de as u32);

                self.set_flag(flags::FLAG_H, (hl & 0x0FFF) + (de & 0x0FFF) > 0x0FFF);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, result > 0xFFFF);
                // S, Z, P/V unchanged

                self.set_hl(result as u16);
                11
            }
            0x1A => {
                // LD A, (DE)
                self.a = bus.read(self.de() as u32);
                7
            }
            0x1B => {
                // DEC DE
                self.set_de(self.de().wrapping_sub(1));
                6
            }
            0x1C => {
                // INC E
                let value = self.e;
                let result = value.wrapping_add(1);
                self.e = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                self.set_flag(flags::FLAG_PV, value == 0x7F);
                self.set_flag(flags::FLAG_N, false);
                4
            }
            0x1D => {
                // DEC E
                let value = self.e;
                let result = value.wrapping_sub(1);
                self.e = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                self.set_flag(flags::FLAG_PV, value == 0x80);
                self.set_flag(flags::FLAG_N, true);
                4
            }
            0x1E => {
                // LD E, n
                self.e = self.fetch(bus);
                7
            }
            0x1F => {
                // RRA
                let old_carry = if self.carry() { 0x80 } else { 0 };
                let bit0 = self.a & 0x01;
                self.a = (self.a >> 1) | old_carry;
                self.set_flag(flags::FLAG_H, false);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, bit0 != 0);
                4
            }
            0x20 => {
                // JR NZ, n
                let offset = self.fetch(bus) as i8;
                if !self.zero() {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    12
                } else {
                    7
                }
            }
            0x21 => {
                // LD HL, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                self.set_hl((high as u16) << 8 | low as u16);
                10
            }
            0x22 => {
                // LD (nn), HL
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                bus.write(addr as u32, self.l);
                bus.write((addr.wrapping_add(1)) as u32, self.h);
                16
            }
            0x23 => {
                // INC HL
                self.set_hl(self.hl().wrapping_add(1));
                6
            }
            0x24 => {
                // INC H
                let value = self.h;
                let result = value.wrapping_add(1);
                self.h = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                self.set_flag(flags::FLAG_PV, value == 0x7F);
                self.set_flag(flags::FLAG_N, false);
                4
            }
            0x25 => {
                // DEC H
                let value = self.h;
                let result = value.wrapping_sub(1);
                self.h = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                self.set_flag(flags::FLAG_PV, value == 0x80);
                self.set_flag(flags::FLAG_N, true);
                4
            }
            0x26 => {
                // LD H, n
                self.h = self.fetch(bus);
                7
            }
            0x27 => {
                // DAA
                let mut adjust = 0u8;
                let mut carry = self.carry();

                if self.get_flag(flags::FLAG_H) || (self.a & 0x0F) > 9 {
                    adjust |= 0x06;
                }
                if carry || self.a > 0x99 {
                    adjust |= 0x60;
                    carry = true;
                }

                if self.get_flag(flags::FLAG_N) {
                    self.a = self.a.wrapping_sub(adjust);
                } else {
                    self.a = self.a.wrapping_add(adjust);
                }

                self.set_flag(flags::FLAG_S, self.a & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, self.a == 0);
                self.set_flag(flags::FLAG_H, false); // Simplified
                self.set_flag(flags::FLAG_PV, self.a.count_ones() % 2 == 0);
                self.set_flag(flags::FLAG_C, carry);
                4
            }
            0x28 => {
                // JR Z, n
                let offset = self.fetch(bus) as i8;
                if self.zero() {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    12
                } else {
                    7
                }
            }
            0x29 => {
                // ADD HL, HL
                let hl = self.hl();
                let result = (hl as u32) + (hl as u32);

                self.set_flag(flags::FLAG_H, (hl & 0x0FFF) + (hl & 0x0FFF) > 0x0FFF);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, result > 0xFFFF);

                self.set_hl(result as u16);
                11
            }
            0x2A => {
                // LD HL, (nn)
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                let l = bus.read(addr as u32);
                let h = bus.read((addr.wrapping_add(1)) as u32);
                self.set_hl((h as u16) << 8 | l as u16);
                16
            }
            0x2B => {
                // DEC HL
                self.set_hl(self.hl().wrapping_sub(1));
                6
            }
            0x2C => {
                // INC L
                let value = self.l;
                let result = value.wrapping_add(1);
                self.l = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                self.set_flag(flags::FLAG_PV, value == 0x7F);
                self.set_flag(flags::FLAG_N, false);
                4
            }
            0x2D => {
                // DEC L
                let value = self.l;
                let result = value.wrapping_sub(1);
                self.l = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                self.set_flag(flags::FLAG_PV, value == 0x80);
                self.set_flag(flags::FLAG_N, true);
                4
            }
            0x2E => {
                // LD L, n
                self.l = self.fetch(bus);
                7
            }
            0x2F => {
                // CPL
                self.a = !self.a;
                self.set_flag(flags::FLAG_H, true);
                self.set_flag(flags::FLAG_N, true);
                4
            }
            0x30 => {
                // JR NC, n
                let offset = self.fetch(bus) as i8;
                if !self.carry() {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    12
                } else {
                    7
                }
            }
            0x31 => {
                // LD SP, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                self.sp = (high as u16) << 8 | low as u16;
                10
            }
            0x32 => {
                // LD (nn), A
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                bus.write(addr as u32, self.a);
                13
            }
            0x33 => {
                // INC SP
                self.sp = self.sp.wrapping_add(1);
                6
            }
            0x34 => {
                // INC (HL)
                let addr = self.hl() as u32;
                let value = bus.read(addr);
                let result = value.wrapping_add(1);
                bus.write(addr, result);

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                self.set_flag(flags::FLAG_PV, value == 0x7F);
                self.set_flag(flags::FLAG_N, false);
                11
            }
            0x35 => {
                // DEC (HL)
                let addr = self.hl() as u32;
                let value = bus.read(addr);
                let result = value.wrapping_sub(1);
                bus.write(addr, result);

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                self.set_flag(flags::FLAG_PV, value == 0x80);
                self.set_flag(flags::FLAG_N, true);
                11
            }
            0x36 => {
                // LD (HL), n
                let n = self.fetch(bus);
                bus.write(self.hl() as u32, n);
                10
            }
            0x37 => {
                // SCF
                self.set_flag(flags::FLAG_H, false);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, true);
                4
            }
            0x38 => {
                // JR C, n
                let offset = self.fetch(bus) as i8;
                if self.carry() {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    12
                } else {
                    7
                }
            }
            0x39 => {
                // ADD HL, SP
                let hl = self.hl();
                let sp = self.sp;
                let result = (hl as u32) + (sp as u32);

                self.set_flag(flags::FLAG_H, (hl & 0x0FFF) + (sp & 0x0FFF) > 0x0FFF);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, result > 0xFFFF);

                self.set_hl(result as u16);
                11
            }
            0x3A => {
                // LD A, (nn)
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                self.a = bus.read(addr as u32);
                13
            }
            0x3B => {
                // DEC SP
                self.sp = self.sp.wrapping_sub(1);
                6
            }
            0x3C => {
                // INC A
                let value = self.a;
                let result = value.wrapping_add(1);
                self.a = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                self.set_flag(flags::FLAG_PV, value == 0x7F);
                self.set_flag(flags::FLAG_N, false);
                4
            }
            0x3D => {
                // DEC A
                let value = self.a;
                let result = value.wrapping_sub(1);
                self.a = result;

                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                self.set_flag(flags::FLAG_Z, result == 0);
                self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                self.set_flag(flags::FLAG_PV, value == 0x80);
                self.set_flag(flags::FLAG_N, true);
                4
            }
            0x3E => {
                // LD A, n
                self.a = self.fetch(bus);
                7
            }
            0x3F => {
                // CCF
                let old_carry = self.carry();
                self.set_flag(flags::FLAG_H, old_carry);
                self.set_flag(flags::FLAG_N, false);
                self.set_flag(flags::FLAG_C, !old_carry);
                4
            }
            0x76 => {
                // HALT
                self.halted = true;
                4
            }
            0xC0 => {
                // RET NZ
                if !self.zero() {
                    let low = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    let high = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = (high as u16) << 8 | low as u16;
                    11
                } else {
                    5
                }
            }
            0xC1 => {
                // POP BC
                self.c = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                self.b = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                10
            }
            0xC2 => {
                // JP NZ, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if !self.zero() {
                    self.pc = addr;
                }
                10
            }
            0xC3 => {
                // JP nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                self.pc = (high as u16) << 8 | low as u16;
                10
            }
            0xC4 => {
                // CALL NZ, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if !self.zero() {
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, self.pc as u8);
                    self.pc = addr;
                    17
                } else {
                    10
                }
            }
            0xC5 => {
                // PUSH BC
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.b);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.c);
                11
            }
            0xC6 => {
                // ADD A, n
                let n = self.fetch(bus);
                self.add_a(n);
                7
            }
            0xC7 => {
                // RST 00h
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);
                self.pc = 0x0000;
                11
            }
            0xC8 => {
                // RET Z
                if self.zero() {
                    let low = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    let high = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = (high as u16) << 8 | low as u16;
                    11
                } else {
                    5
                }
            }
            0xC9 => {
                // RET
                let low = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                let high = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                self.pc = (high as u16) << 8 | low as u16;
                10
            }
            0xCA => {
                // JP Z, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if self.zero() {
                    self.pc = addr;
                }
                10
            }
            0xCB => {
                let op2 = self.fetch(bus);
                let x = op2 >> 6;
                let bit = (op2 >> 3) & 0x07;
                let reg = op2 & 0x07;

                match x {
                    1 => {
                        // BIT b, r
                        let value = if reg == 6 {
                            bus.read(self.hl() as u32)
                        } else {
                            self.read_register(reg)
                        };
                        self.set_flag(flags::FLAG_Z, value & (1 << bit) == 0);
                        self.set_flag(flags::FLAG_H, true);
                        self.set_flag(flags::FLAG_N, false);
                        if reg == 6 { 12 } else { 8 }
                    }
                    2 => {
                        // RES b, r
                        if reg == 6 {
                            let addr = self.hl() as u32;
                            let value = bus.read(addr);
                            bus.write(addr, value & !(1 << bit));
                            15
                        } else {
                            let value = self.read_register(reg);
                            self.set_register(reg, value & !(1 << bit));
                            8
                        }
                    }
                    3 => {
                        // SET b, r
                        if reg == 6 {
                            let addr = self.hl() as u32;
                            let value = bus.read(addr);
                            bus.write(addr, value | (1 << bit));
                            15
                        } else {
                            let value = self.read_register(reg);
                            self.set_register(reg, value | (1 << bit));
                            8
                        }
                    }
                    0 => {
                        // Rotates and shifts
                        let value = if reg == 6 {
                            bus.read(self.hl() as u32)
                        } else {
                            self.read_register(reg)
                        };

                        let result = match bit {
                            // 'bit' field is actually the operation here
                            0 => {
                                // RLC
                                let bit7 = value >> 7;
                                self.set_flag(flags::FLAG_C, bit7 != 0);
                                (value << 1) | bit7
                            }
                            1 => {
                                // RRC
                                let bit0 = value & 1;
                                self.set_flag(flags::FLAG_C, bit0 != 0);
                                (value >> 1) | (bit0 << 7)
                            }
                            2 => {
                                // RL
                                let bit7 = value >> 7;
                                let old_c = if self.carry() { 1 } else { 0 };
                                self.set_flag(flags::FLAG_C, bit7 != 0);
                                (value << 1) | old_c
                            }
                            3 => {
                                // RR
                                let bit0 = value & 1;
                                let old_c = if self.carry() { 0x80 } else { 0 };
                                self.set_flag(flags::FLAG_C, bit0 != 0);
                                (value >> 1) | old_c
                            }
                            4 => {
                                // SLA
                                self.set_flag(flags::FLAG_C, value & 0x80 != 0);
                                value << 1
                            }
                            5 => {
                                // SRA
                                self.set_flag(flags::FLAG_C, value & 1 != 0);
                                (value >> 1) | (value & 0x80)
                            }
                            6 => {
                                // SLL (undocumented)
                                self.set_flag(flags::FLAG_C, value & 0x80 != 0);
                                (value << 1) | 1
                            }
                            7 => {
                                // SRL
                                self.set_flag(flags::FLAG_C, value & 1 != 0);
                                value >> 1
                            }
                            _ => unreachable!(),
                        };

                        self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, result == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, result.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);

                        if reg == 6 {
                            bus.write(self.hl() as u32, result);
                            15
                        } else {
                            self.set_register(reg, result);
                            8
                        }
                    }
                    _ => unreachable!(),
                }
            }
            0xCC => {
                // CALL Z, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if self.zero() {
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, self.pc as u8);
                    self.pc = addr;
                    17
                } else {
                    10
                }
            }
            0xCD => {
                // CALL nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;

                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);

                self.pc = addr;
                17
            }
            0xCE => {
                // ADC A, n
                let n = self.fetch(bus);
                self.adc_a(n);
                7
            }
            0xCF => {
                // RST 08h
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);
                self.pc = 0x0008;
                11
            }
            0xD0 => {
                // RET NC
                if !self.carry() {
                    let low = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    let high = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = (high as u16) << 8 | low as u16;
                    11
                } else {
                    5
                }
            }
            0xD1 => {
                // POP DE
                self.e = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                self.d = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                10
            }
            0xD2 => {
                // JP NC, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if !self.carry() {
                    self.pc = addr;
                }
                10
            }
            0xD3 => {
                // OUT (n), A
                let port_low = self.fetch(bus);
                let port = (self.a as u16) << 8 | port_low as u16;
                bus.write_io(port, self.a);
                11
            }
            0xD4 => {
                // CALL NC, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if !self.carry() {
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, self.pc as u8);
                    self.pc = addr;
                    17
                } else {
                    10
                }
            }
            0xD5 => {
                // PUSH DE
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.d);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.e);
                11
            }
            0xD6 => {
                // SUB n
                let n = self.fetch(bus);
                self.sub_a(n);
                7
            }
            0xD7 => {
                // RST 10h
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);
                self.pc = 0x0010;
                11
            }
            0xD8 => {
                // RET C
                if self.carry() {
                    let low = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    let high = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = (high as u16) << 8 | low as u16;
                    11
                } else {
                    5
                }
            }
            0xD9 => {
                // EXX
                std::mem::swap(&mut self.b, &mut self.b_shadow);
                std::mem::swap(&mut self.c, &mut self.c_shadow);
                std::mem::swap(&mut self.d, &mut self.d_shadow);
                std::mem::swap(&mut self.e, &mut self.e_shadow);
                std::mem::swap(&mut self.h, &mut self.h_shadow);
                std::mem::swap(&mut self.l, &mut self.l_shadow);
                4
            }
            0xDA => {
                // JP C, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if self.carry() {
                    self.pc = addr;
                }
                10
            }
            0xDB => {
                // IN A, (n)
                let n = self.fetch(bus);
                let port = (self.a as u16) << 8 | n as u16;
                self.a = bus.read_io(port);
                11
            }
            0xDC => {
                // CALL C, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if self.carry() {
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, self.pc as u8);
                    self.pc = addr;
                    17
                } else {
                    10
                }
            }
            0xDD => {
                let op2 = self.fetch(bus);
                match op2 {
                    0x09 => {
                        // ADD IX, BC
                        let ix = self.ix;
                        let bc = self.bc();
                        let result = (ix as u32) + (bc as u32);

                        self.set_flag(flags::FLAG_H, (ix & 0x0FFF) + (bc & 0x0FFF) > 0x0FFF);
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.ix = result as u16;
                        15
                    }
                    0x19 => {
                        // ADD IX, DE
                        let ix = self.ix;
                        let de = self.de();
                        let result = (ix as u32) + (de as u32);

                        self.set_flag(flags::FLAG_H, (ix & 0x0FFF) + (de & 0x0FFF) > 0x0FFF);
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.ix = result as u16;
                        15
                    }
                    0x21 => {
                        // LD IX, nn
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        self.ix = (high as u16) << 8 | low as u16;
                        14
                    }
                    0x22 => {
                        // LD (nn), IX
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        bus.write(addr as u32, self.ix as u8);
                        bus.write((addr.wrapping_add(1)) as u32, (self.ix >> 8) as u8);
                        20
                    }
                    0x23 => {
                        // INC IX
                        self.ix = self.ix.wrapping_add(1);
                        10
                    }
                    0x29 => {
                        // ADD IX, IX
                        let ix = self.ix;
                        let result = (ix as u32) + (ix as u32);

                        self.set_flag(flags::FLAG_H, (ix & 0x0FFF) + (ix & 0x0FFF) > 0x0FFF);
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.ix = result as u16;
                        15
                    }
                    0x2A => {
                        // LD IX, (nn)
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        let ix_low = bus.read(addr as u32);
                        let ix_high = bus.read((addr.wrapping_add(1)) as u32);
                        self.ix = (ix_high as u16) << 8 | ix_low as u16;
                        20
                    }
                    0x2B => {
                        // DEC IX
                        self.ix = self.ix.wrapping_sub(1);
                        10
                    }
                    0x34 => {
                        // INC (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        let result = value.wrapping_add(1);
                        bus.write(addr, result);

                        self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, result == 0);
                        self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                        self.set_flag(flags::FLAG_PV, value == 0x7F);
                        self.set_flag(flags::FLAG_N, false);
                        23
                    }
                    0x35 => {
                        // DEC (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        let result = value.wrapping_sub(1);
                        bus.write(addr, result);

                        self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, result == 0);
                        self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                        self.set_flag(flags::FLAG_PV, value == 0x80);
                        self.set_flag(flags::FLAG_N, true);
                        23
                    }
                    0x36 => {
                        // LD (IX+d), n
                        let d = self.fetch(bus) as i8;
                        let n = self.fetch(bus);
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        bus.write(addr, n);
                        19
                    }
                    0x39 => {
                        // ADD IX, SP
                        let ix = self.ix;
                        let sp = self.sp;
                        let result = (ix as u32) + (sp as u32);

                        self.set_flag(flags::FLAG_H, (ix & 0x0FFF) + (sp & 0x0FFF) > 0x0FFF);
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.ix = result as u16;
                        15
                    }
                    0x46 => {
                        // LD B, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        self.b = bus.read(addr);
                        19
                    }
                    0x4E => {
                        // LD C, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        self.c = bus.read(addr);
                        19
                    }
                    0x56 => {
                        // LD D, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        self.d = bus.read(addr);
                        19
                    }
                    0x5E => {
                        // LD E, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        self.e = bus.read(addr);
                        19
                    }
                    0x6E => {
                        // LD L, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        self.l = bus.read(addr);
                        19
                    }
                    0x66 => {
                        // LD H, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        self.h = bus.read(addr);
                        19
                    }
                    0x70 => {
                        // LD (IX+d), B
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.b);
                        19
                    }
                    0x71 => {
                        // LD (IX+d), C
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.c);
                        19
                    }
                    0x72 => {
                        // LD (IX+d), D
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.d);
                        19
                    }
                    0x73 => {
                        // LD (IX+d), E
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.e);
                        19
                    }
                    0x74 => {
                        // LD (IX+d), H
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.h);
                        19
                    }
                    0x75 => {
                        // LD (IX+d), L
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.l);
                        19
                    }
                    0x77 => {
                        // LD (IX+d), A
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.a);
                        19
                    }
                    0x7E => {
                        // LD A, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        self.a = bus.read(addr);
                        19
                    }
                    0x86 => {
                        // ADD A, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.add_a(value);
                        19
                    }
                    0x8E => {
                        // ADC A, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.adc_a(value);
                        19
                    }
                    0x96 => {
                        // SUB (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.sub_a(value);
                        19
                    }
                    0x9E => {
                        // SBC A, (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.sbc_a(value);
                        19
                    }
                    0xA6 => {
                        // AND (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.and_a(value);
                        19
                    }
                    0xAE => {
                        // XOR (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.xor_a(value);
                        19
                    }
                    0xB6 => {
                        // OR (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.or_a(value);
                        19
                    }
                    0xBE => {
                        // CP (IX+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.cp_a(value);
                        19
                    }
                    0xCB => {
                        let d = self.fetch(bus) as i8;
                        let addr = self.ix.wrapping_add(d as u16) as u32;
                        let op3 = self.fetch(bus);

                        let x = op3 >> 6;
                        let bit = (op3 >> 3) & 0x07;

                        match x {
                            0 => {
                                // Rotate/shift operations on (IX+d)
                                let value = bus.read(addr);
                                let result = match bit {
                                    0 => {
                                        // RLC (IX+d)
                                        let c = value >> 7;
                                        self.set_flag(flags::FLAG_C, c != 0);
                                        (value << 1) | c
                                    }
                                    1 => {
                                        // RRC (IX+d)
                                        let c = value & 1;
                                        self.set_flag(flags::FLAG_C, c != 0);
                                        (value >> 1) | (c << 7)
                                    }
                                    2 => {
                                        // RL (IX+d)
                                        let old_c = if self.carry() { 1 } else { 0 };
                                        self.set_flag(flags::FLAG_C, value & 0x80 != 0);
                                        (value << 1) | old_c
                                    }
                                    3 => {
                                        // RR (IX+d)
                                        let old_c = if self.carry() { 0x80 } else { 0 };
                                        self.set_flag(flags::FLAG_C, value & 1 != 0);
                                        (value >> 1) | old_c
                                    }
                                    4 => {
                                        // SLA (IX+d)
                                        self.set_flag(flags::FLAG_C, value & 0x80 != 0);
                                        value << 1
                                    }
                                    5 => {
                                        // SRA (IX+d)
                                        self.set_flag(flags::FLAG_C, value & 1 != 0);
                                        (value >> 1) | (value & 0x80)
                                    }
                                    6 => {
                                        // SLL (IX+d) - undocumented
                                        self.set_flag(flags::FLAG_C, value & 0x80 != 0);
                                        (value << 1) | 1
                                    }
                                    7 => {
                                        // SRL (IX+d)
                                        self.set_flag(flags::FLAG_C, value & 1 != 0);
                                        value >> 1
                                    }
                                    _ => unreachable!(),
                                };
                                bus.write(addr, result);
                                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                                self.set_flag(flags::FLAG_Z, result == 0);
                                self.set_flag(flags::FLAG_H, false);
                                self.set_flag(flags::FLAG_PV, result.count_ones() % 2 == 0);
                                self.set_flag(flags::FLAG_N, false);
                                23
                            }
                            1 => {
                                // BIT b, (IX+d)
                                let value = bus.read(addr);
                                self.set_flag(flags::FLAG_Z, value & (1 << bit) == 0);
                                self.set_flag(flags::FLAG_H, true);
                                self.set_flag(flags::FLAG_N, false);
                                20
                            }
                            2 => {
                                // RES b, (IX+d)
                                let value = bus.read(addr);
                                bus.write(addr, value & !(1 << bit));
                                23
                            }
                            3 => {
                                // SET b, (IX+d)
                                let value = bus.read(addr);
                                bus.write(addr, value | (1 << bit));
                                23
                            }
                            _ => unreachable!(),
                        }
                    }
                    0xE1 => {
                        // POP IX
                        let low = bus.read(self.sp as u32);
                        self.sp = self.sp.wrapping_add(1);
                        let high = bus.read(self.sp as u32);
                        self.sp = self.sp.wrapping_add(1);
                        self.ix = (high as u16) << 8 | low as u16;
                        14
                    }
                    0xE5 => {
                        // PUSH IX
                        self.sp = self.sp.wrapping_sub(1);
                        bus.write(self.sp as u32, (self.ix >> 8) as u8);
                        self.sp = self.sp.wrapping_sub(1);
                        bus.write(self.sp as u32, self.ix as u8);
                        15
                    }
                    0xE3 => {
                        // EX (SP), IX
                        let low = bus.read(self.sp as u32);
                        let high = bus.read((self.sp.wrapping_add(1)) as u32);
                        bus.write(self.sp as u32, self.ix as u8);
                        bus.write((self.sp.wrapping_add(1)) as u32, (self.ix >> 8) as u8);
                        self.ix = (high as u16) << 8 | low as u16;
                        23
                    }
                    0xE9 => {
                        // JP (IX)
                        self.pc = self.ix;
                        8
                    }
                    0xF9 => {
                        // LD SP, IX
                        self.sp = self.ix;
                        10
                    }
                    _ => todo!("DD opcode {:#04X}", op2),
                }
            }
            0xDE => {
                // SBC A, n
                let n = self.fetch(bus);
                self.sbc_a(n);
                7
            }
            0xDF => {
                // RST 18h
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);
                self.pc = 0x0018;
                11
            }
            0xE0 => {
                // RET PO
                if !self.get_flag(flags::FLAG_PV) {
                    let low = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    let high = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = (high as u16) << 8 | low as u16;
                    11
                } else {
                    5
                }
            }
            0xE1 => {
                // POP HL
                self.l = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                self.h = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                10
            }
            0xE2 => {
                // JP PO, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if !self.get_flag(flags::FLAG_PV) {
                    self.pc = addr;
                }
                10
            }
            0xE3 => {
                // EX (SP), HL
                let low = bus.read(self.sp as u32);
                let high = bus.read((self.sp.wrapping_add(1)) as u32);
                bus.write(self.sp as u32, self.l);
                bus.write((self.sp.wrapping_add(1)) as u32, self.h);
                self.l = low;
                self.h = high;
                19
            }
            0xE4 => {
                // CALL PO, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if !self.get_flag(flags::FLAG_PV) {
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, self.pc as u8);
                    self.pc = addr;
                    17
                } else {
                    10
                }
            }
            0xE5 => {
                // PUSH HL
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.h);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.l);
                11
            }
            0xE6 => {
                // AND n
                let n = self.fetch(bus);
                self.and_a(n);
                7
            }
            0xE7 => {
                // RST 20h
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);
                self.pc = 0x0020;
                11
            }
            0xE8 => {
                // RET PE
                if self.get_flag(flags::FLAG_PV) {
                    let low = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    let high = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = (high as u16) << 8 | low as u16;
                    11
                } else {
                    5
                }
            }
            0xE9 => {
                // JP (HL)
                self.pc = self.hl();
                4
            }
            0xEA => {
                // JP PE, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if self.get_flag(flags::FLAG_PV) {
                    self.pc = addr;
                }
                10
            }
            0xEB => {
                // EX DE, HL
                std::mem::swap(&mut self.d, &mut self.h);
                std::mem::swap(&mut self.e, &mut self.l);
                4
            }
            0xEC => {
                // CALL PE, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if self.get_flag(flags::FLAG_PV) {
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, self.pc as u8);
                    self.pc = addr;
                    17
                } else {
                    10
                }
            }
            0xED => {
                let op2 = self.fetch(bus);
                match op2 {
                    0x40 => {
                        // IN B, (C)
                        self.b = bus.read_io(self.bc());
                        self.set_flag(flags::FLAG_S, self.b & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.b == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.b.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);
                        12
                    }
                    0x41 => {
                        // OUT (C), B
                        bus.write_io(self.bc(), self.b);
                        12
                    }
                    0x42 => {
                        // SBC HL, BC
                        let hl = self.hl();
                        let bc = self.bc();
                        let c = if self.carry() { 1u32 } else { 0 };
                        let result = (hl as u32).wrapping_sub(bc as u32).wrapping_sub(c);

                        self.set_flag(flags::FLAG_S, result & 0x8000 != 0);
                        self.set_flag(flags::FLAG_Z, (result & 0xFFFF) == 0);
                        self.set_flag(flags::FLAG_H, (hl & 0x0FFF) < (bc & 0x0FFF) + c as u16);
                        self.set_flag(
                            flags::FLAG_PV,
                            ((hl ^ bc) & 0x8000 != 0) && ((hl ^ result as u16) & 0x8000 != 0),
                        );
                        self.set_flag(flags::FLAG_N, true);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.set_hl(result as u16);
                        15
                    }
                    0x43 => {
                        // LD (nn), BC
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        bus.write(addr as u32, self.c);
                        bus.write((addr.wrapping_add(1)) as u32, self.b);
                        20
                    }
                    0x44 => {
                        // NEG
                        let a = self.a;
                        self.a = 0u8.wrapping_sub(a);
                        self.set_flag(flags::FLAG_S, self.a & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.a == 0);
                        self.set_flag(flags::FLAG_H, (0 & 0x0F) < (a & 0x0F));
                        self.set_flag(flags::FLAG_PV, a == 0x80);
                        self.set_flag(flags::FLAG_N, true);
                        self.set_flag(flags::FLAG_C, a != 0);
                        8
                    }
                    0x45 => {
                        // RETN
                        let low = bus.read(self.sp as u32);
                        self.sp = self.sp.wrapping_add(1);
                        let high = bus.read(self.sp as u32);
                        self.sp = self.sp.wrapping_add(1);
                        self.pc = (high as u16) << 8 | low as u16;
                        self.iff1 = self.iff2;
                        14
                    }
                    0x46 => {
                        // IM 0
                        self.interrupt_mode = 0;
                        8
                    }
                    0x47 => {
                        // LD I, A
                        self.i = self.a;
                        9
                    }
                    0x48 => {
                        // IN C, (C)
                        self.c = bus.read_io(self.bc());
                        self.set_flag(flags::FLAG_S, self.c & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.c == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.c.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);
                        12
                    }
                    0x49 => {
                        // OUT (C), C
                        bus.write_io(self.bc(), self.c);
                        12
                    }
                    0x4A => {
                        // ADC HL, BC
                        let hl = self.hl();
                        let bc = self.bc();
                        let c = if self.carry() { 1u32 } else { 0 };
                        let result = (hl as u32) + (bc as u32) + c;

                        self.set_flag(flags::FLAG_S, result & 0x8000 != 0);
                        self.set_flag(flags::FLAG_Z, (result & 0xFFFF) == 0);
                        self.set_flag(
                            flags::FLAG_H,
                            (hl & 0x0FFF) + (bc & 0x0FFF) + c as u16 > 0x0FFF,
                        );
                        self.set_flag(
                            flags::FLAG_PV,
                            ((hl ^ bc) & 0x8000 == 0) && ((hl ^ result as u16) & 0x8000 != 0),
                        );
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.set_hl(result as u16);
                        15
                    }
                    0x4B => {
                        // LD BC, (nn)
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        self.c = bus.read(addr as u32);
                        self.b = bus.read((addr.wrapping_add(1)) as u32);
                        20
                    }
                    0x4D => {
                        // RETI
                        let low = bus.read(self.sp as u32);
                        self.sp = self.sp.wrapping_add(1);
                        let high = bus.read(self.sp as u32);
                        self.sp = self.sp.wrapping_add(1);
                        self.pc = (high as u16) << 8 | low as u16;
                        14
                    }
                    0x4F => {
                        // LD R, A
                        self.r = self.a;
                        9
                    }
                    0x50 => {
                        // IN D, (C)
                        self.d = bus.read_io(self.bc());
                        self.set_flag(flags::FLAG_S, self.d & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.d == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.d.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);
                        12
                    }
                    0x51 => {
                        // OUT (C), D
                        bus.write_io(self.bc(), self.d);
                        12
                    }
                    0x52 => {
                        // SBC HL, DE
                        let hl = self.hl();
                        let de = self.de();
                        let c = if self.carry() { 1u32 } else { 0 };
                        let result = (hl as u32).wrapping_sub(de as u32).wrapping_sub(c);

                        self.set_flag(flags::FLAG_S, result & 0x8000 != 0);
                        self.set_flag(flags::FLAG_Z, (result & 0xFFFF) == 0);
                        self.set_flag(flags::FLAG_H, (hl & 0x0FFF) < (de & 0x0FFF) + c as u16);
                        self.set_flag(
                            flags::FLAG_PV,
                            ((hl ^ de) & 0x8000 != 0) && ((hl ^ result as u16) & 0x8000 != 0),
                        );
                        self.set_flag(flags::FLAG_N, true);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.set_hl(result as u16);
                        15
                    }
                    0x53 => {
                        // LD (nn), DE
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        bus.write(addr as u32, self.e);
                        bus.write((addr.wrapping_add(1)) as u32, self.d);
                        20
                    }
                    0x56 => {
                        // IM 1
                        self.interrupt_mode = 1;
                        8
                    }
                    0x57 => {
                        // LD A, I
                        self.a = self.i;
                        self.set_flag(flags::FLAG_S, self.a & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.a == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.iff2);
                        self.set_flag(flags::FLAG_N, false);
                        9
                    }
                    0x58 => {
                        // IN E, (C)
                        self.e = bus.read_io(self.bc());
                        self.set_flag(flags::FLAG_S, self.e & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.e == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.e.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);
                        12
                    }
                    0x59 => {
                        // OUT (C), E
                        bus.write_io(self.bc(), self.e);
                        12
                    }
                    0x5A => {
                        // ADC HL, DE
                        let hl = self.hl();
                        let de = self.de();
                        let c = if self.carry() { 1u32 } else { 0 };
                        let result = (hl as u32) + (de as u32) + c;

                        self.set_flag(flags::FLAG_S, result & 0x8000 != 0);
                        self.set_flag(flags::FLAG_Z, (result & 0xFFFF) == 0);
                        self.set_flag(
                            flags::FLAG_H,
                            (hl & 0x0FFF) + (de & 0x0FFF) + c as u16 > 0x0FFF,
                        );
                        self.set_flag(
                            flags::FLAG_PV,
                            ((hl ^ de) & 0x8000 == 0) && ((hl ^ result as u16) & 0x8000 != 0),
                        );
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.set_hl(result as u16);
                        15
                    }
                    0x5B => {
                        // LD DE, (nn)
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        self.e = bus.read(addr as u32);
                        self.d = bus.read((addr.wrapping_add(1)) as u32);
                        20
                    }
                    0x5E => {
                        // IM 2
                        self.interrupt_mode = 2;
                        8
                    }
                    0x5F => {
                        // LD A, R
                        self.a = self.r;
                        self.set_flag(flags::FLAG_S, self.a & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.a == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.iff2);
                        self.set_flag(flags::FLAG_N, false);
                        9
                    }
                    0x60 => {
                        // IN H, (C)
                        self.h = bus.read_io(self.bc());
                        self.set_flag(flags::FLAG_S, self.h & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.h == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.h.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);
                        12
                    }
                    0x61 => {
                        // OUT (C), H
                        bus.write_io(self.bc(), self.h);
                        12
                    }
                    0x62 => {
                        // SBC HL, HL
                        let hl = self.hl();
                        let c = if self.carry() { 1u32 } else { 0 };
                        let result = (hl as u32).wrapping_sub(hl as u32).wrapping_sub(c);

                        self.set_flag(flags::FLAG_S, result & 0x8000 != 0);
                        self.set_flag(flags::FLAG_Z, (result & 0xFFFF) == 0);
                        self.set_flag(flags::FLAG_H, c != 0); // Only half carry from the carry flag
                        self.set_flag(flags::FLAG_PV, false); // No overflow when subtracting same value
                        self.set_flag(flags::FLAG_N, true);
                        self.set_flag(flags::FLAG_C, c != 0);

                        self.set_hl(result as u16);
                        15
                    }
                    0x63 => {
                        // LD (nn), HL
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        bus.write(addr as u32, self.l);
                        bus.write((addr.wrapping_add(1)) as u32, self.h);
                        20
                    }
                    0x67 => {
                        // RRD
                        let hl = self.hl();
                        let mem = bus.read(hl as u32);
                        let low_a = self.a & 0x0F;
                        self.a = (self.a & 0xF0) | (mem & 0x0F);
                        let new_mem = (low_a << 4) | (mem >> 4);
                        bus.write(hl as u32, new_mem);
                        self.set_flag(flags::FLAG_S, self.a & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.a == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.a.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);
                        18
                    }
                    0x68 => {
                        // IN L, (C)
                        self.l = bus.read_io(self.bc());
                        self.set_flag(flags::FLAG_S, self.l & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.l == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.l.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);
                        12
                    }
                    0x69 => {
                        // OUT (C), L
                        bus.write_io(self.bc(), self.l);
                        12
                    }
                    0x6A => {
                        // ADC HL, HL
                        let hl = self.hl();
                        let c = if self.carry() { 1u32 } else { 0 };
                        let result = (hl as u32) + (hl as u32) + c;

                        self.set_flag(flags::FLAG_S, result & 0x8000 != 0);
                        self.set_flag(flags::FLAG_Z, (result & 0xFFFF) == 0);
                        self.set_flag(
                            flags::FLAG_H,
                            (hl & 0x0FFF) + (hl & 0x0FFF) + c as u16 > 0x0FFF,
                        );
                        self.set_flag(flags::FLAG_PV, (hl ^ result as u16) & 0x8000 != 0);
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.set_hl(result as u16);
                        15
                    }
                    0x6B => {
                        // LD HL, (nn)
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        self.l = bus.read(addr as u32);
                        self.h = bus.read((addr.wrapping_add(1)) as u32);
                        20
                    }
                    0x6F => {
                        // RLD
                        let hl = self.hl();
                        let mem = bus.read(hl as u32);
                        let low_a = self.a & 0x0F;
                        self.a = (self.a & 0xF0) | (mem >> 4);
                        let new_mem = (mem << 4) | low_a;
                        bus.write(hl as u32, new_mem);
                        self.set_flag(flags::FLAG_S, self.a & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.a == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.a.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);
                        18
                    }
                    0x72 => {
                        // SBC HL, SP
                        let hl = self.hl();
                        let sp = self.sp;
                        let c = if self.carry() { 1u32 } else { 0 };
                        let result = (hl as u32).wrapping_sub(sp as u32).wrapping_sub(c);

                        self.set_flag(flags::FLAG_S, result & 0x8000 != 0);
                        self.set_flag(flags::FLAG_Z, (result & 0xFFFF) == 0);
                        self.set_flag(flags::FLAG_H, (hl & 0x0FFF) < (sp & 0x0FFF) + c as u16);
                        self.set_flag(
                            flags::FLAG_PV,
                            ((hl ^ sp) & 0x8000 != 0) && ((hl ^ result as u16) & 0x8000 != 0),
                        );
                        self.set_flag(flags::FLAG_N, true);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.set_hl(result as u16);
                        15
                    }
                    0x73 => {
                        // LD (nn), SP
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        bus.write(addr as u32, self.sp as u8);
                        bus.write((addr.wrapping_add(1)) as u32, (self.sp >> 8) as u8);
                        20
                    }
                    0x78 => {
                        // IN A, (C)
                        self.a = bus.read_io(self.bc());
                        self.set_flag(flags::FLAG_S, self.a & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, self.a == 0);
                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.a.count_ones() % 2 == 0);
                        self.set_flag(flags::FLAG_N, false);
                        12
                    }
                    0x79 => {
                        // OUT (C), A
                        bus.write_io(self.bc(), self.a);
                        12
                    }
                    0x7A => {
                        // ADC HL, SP
                        let hl = self.hl();
                        let sp = self.sp;
                        let c = if self.carry() { 1u32 } else { 0 };
                        let result = (hl as u32) + (sp as u32) + c;

                        self.set_flag(flags::FLAG_S, result & 0x8000 != 0);
                        self.set_flag(flags::FLAG_Z, (result & 0xFFFF) == 0);
                        self.set_flag(
                            flags::FLAG_H,
                            (hl & 0x0FFF) + (sp & 0x0FFF) + c as u16 > 0x0FFF,
                        );
                        self.set_flag(
                            flags::FLAG_PV,
                            ((hl ^ sp) & 0x8000 == 0) && ((hl ^ result as u16) & 0x8000 != 0),
                        );
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.set_hl(result as u16);
                        15
                    }
                    0x7B => {
                        // LD SP, (nn)
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        let sp_low = bus.read(addr as u32);
                        let sp_high = bus.read((addr.wrapping_add(1)) as u32);
                        self.sp = (sp_high as u16) << 8 | sp_low as u16;
                        20
                    }
                    0xA0 => {
                        // LDI
                        let value = bus.read(self.hl() as u32);
                        bus.write(self.de() as u32, value);

                        self.set_hl(self.hl().wrapping_add(1));
                        self.set_de(self.de().wrapping_add(1));
                        self.set_bc(self.bc().wrapping_sub(1));

                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.bc() != 0);
                        self.set_flag(flags::FLAG_N, false);
                        16
                    }
                    0xA1 => {
                        // CPI
                        let value = bus.read(self.hl() as u32);
                        let result = self.a.wrapping_sub(value);

                        self.set_hl(self.hl().wrapping_add(1));
                        self.set_bc(self.bc().wrapping_sub(1));

                        self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, result == 0);
                        self.set_flag(flags::FLAG_H, (self.a & 0x0F) < (value & 0x0F));
                        self.set_flag(flags::FLAG_PV, self.bc() != 0);
                        self.set_flag(flags::FLAG_N, true);
                        16
                    }
                    0xA2 => {
                        // INI
                        let value = bus.read_io(self.bc());
                        bus.write(self.hl() as u32, value);
                        self.set_hl(self.hl().wrapping_add(1));
                        self.b = self.b.wrapping_sub(1);

                        self.set_flag(flags::FLAG_Z, self.b == 0);
                        self.set_flag(flags::FLAG_N, true);
                        16
                    }
                    0xA3 => {
                        // OUTI
                        let value = bus.read(self.hl() as u32);
                        bus.write_io(self.bc(), value);
                        self.set_hl(self.hl().wrapping_add(1));
                        self.b = self.b.wrapping_sub(1);

                        self.set_flag(flags::FLAG_Z, self.b == 0);
                        self.set_flag(flags::FLAG_N, true);
                        16
                    }
                    0xA8 => {
                        // LDD
                        let value = bus.read(self.hl() as u32);
                        bus.write(self.de() as u32, value);

                        self.set_hl(self.hl().wrapping_sub(1));
                        self.set_de(self.de().wrapping_sub(1));
                        self.set_bc(self.bc().wrapping_sub(1));

                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.bc() != 0);
                        self.set_flag(flags::FLAG_N, false);
                        16
                    }
                    0xA9 => {
                        // CPD
                        let value = bus.read(self.hl() as u32);
                        let result = self.a.wrapping_sub(value);

                        self.set_hl(self.hl().wrapping_sub(1));
                        self.set_bc(self.bc().wrapping_sub(1));

                        self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, result == 0);
                        self.set_flag(flags::FLAG_H, (self.a & 0x0F) < (value & 0x0F));
                        self.set_flag(flags::FLAG_PV, self.bc() != 0);
                        self.set_flag(flags::FLAG_N, true);
                        16
                    }
                    0xAA => {
                        // IND
                        let value = bus.read_io(self.bc());
                        bus.write(self.hl() as u32, value);
                        self.set_hl(self.hl().wrapping_sub(1));
                        self.b = self.b.wrapping_sub(1);

                        self.set_flag(flags::FLAG_Z, self.b == 0);
                        self.set_flag(flags::FLAG_N, true);
                        16
                    }
                    0xAB => {
                        // OUTD
                        let value = bus.read(self.hl() as u32);
                        bus.write_io(self.bc(), value);
                        self.set_hl(self.hl().wrapping_sub(1));
                        self.b = self.b.wrapping_sub(1);

                        self.set_flag(flags::FLAG_Z, self.b == 0);
                        self.set_flag(flags::FLAG_N, true);
                        16
                    }
                    0xB0 => {
                        // LDIR
                        let value = bus.read(self.hl() as u32);
                        bus.write(self.de() as u32, value);

                        self.set_hl(self.hl().wrapping_add(1));
                        self.set_de(self.de().wrapping_add(1));
                        self.set_bc(self.bc().wrapping_sub(1));

                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.bc() != 0);
                        self.set_flag(flags::FLAG_N, false);

                        if self.bc() != 0 {
                            self.pc = self.pc.wrapping_sub(2); // repeat
                            21
                        } else {
                            16
                        }
                    }
                    0xB1 => {
                        // CPIR
                        let value = bus.read(self.hl() as u32);
                        let result = self.a.wrapping_sub(value);

                        self.set_hl(self.hl().wrapping_add(1));
                        self.set_bc(self.bc().wrapping_sub(1));

                        self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, result == 0);
                        self.set_flag(flags::FLAG_H, (self.a & 0x0F) < (value & 0x0F));
                        self.set_flag(flags::FLAG_PV, self.bc() != 0);
                        self.set_flag(flags::FLAG_N, true);
                        // C flag not affected

                        if self.bc() != 0 && result != 0 {
                            self.pc = self.pc.wrapping_sub(2); // repeat
                            21
                        } else {
                            16
                        }
                    }
                    0xB2 => {
                        // INIR
                        let value = bus.read_io(self.bc());
                        bus.write(self.hl() as u32, value);
                        self.set_hl(self.hl().wrapping_add(1));
                        self.b = self.b.wrapping_sub(1);

                        self.set_flag(flags::FLAG_Z, true);
                        self.set_flag(flags::FLAG_N, true);

                        if self.b != 0 {
                            self.pc = self.pc.wrapping_sub(2);
                            21
                        } else {
                            16
                        }
                    }
                    0xB3 => {
                        // OTIR
                        let value = bus.read(self.hl() as u32);
                        bus.write_io(self.bc(), value);
                        self.set_hl(self.hl().wrapping_add(1));
                        self.b = self.b.wrapping_sub(1);

                        self.set_flag(flags::FLAG_Z, true);
                        self.set_flag(flags::FLAG_N, true);

                        if self.b != 0 {
                            self.pc = self.pc.wrapping_sub(2);
                            21
                        } else {
                            16
                        }
                    }
                    0xB8 => {
                        // LDDR
                        let value = bus.read(self.hl() as u32);
                        bus.write(self.de() as u32, value);

                        self.set_hl(self.hl().wrapping_sub(1));
                        self.set_de(self.de().wrapping_sub(1));
                        self.set_bc(self.bc().wrapping_sub(1));

                        self.set_flag(flags::FLAG_H, false);
                        self.set_flag(flags::FLAG_PV, self.bc() != 0);
                        self.set_flag(flags::FLAG_N, false);

                        if self.bc() != 0 {
                            self.pc = self.pc.wrapping_sub(2); // repeat
                            21
                        } else {
                            16
                        }
                    }
                    0xB9 => {
                        // CPDR
                        let value = bus.read(self.hl() as u32);
                        let result = self.a.wrapping_sub(value);

                        self.set_hl(self.hl().wrapping_sub(1));
                        self.set_bc(self.bc().wrapping_sub(1));

                        self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, result == 0);
                        self.set_flag(flags::FLAG_H, (self.a & 0x0F) < (value & 0x0F));
                        self.set_flag(flags::FLAG_PV, self.bc() != 0);
                        self.set_flag(flags::FLAG_N, true);

                        if self.bc() != 0 && result != 0 {
                            self.pc = self.pc.wrapping_sub(2);
                            21
                        } else {
                            16
                        }
                    }
                    0xBA => {
                        // INDR
                        let value = bus.read_io(self.bc());
                        bus.write(self.hl() as u32, value);
                        self.set_hl(self.hl().wrapping_sub(1));
                        self.b = self.b.wrapping_sub(1);

                        self.set_flag(flags::FLAG_Z, true);
                        self.set_flag(flags::FLAG_N, true);

                        if self.b != 0 {
                            self.pc = self.pc.wrapping_sub(2);
                            21
                        } else {
                            16
                        }
                    }
                    0xBB => {
                        // OTDR
                        let value = bus.read(self.hl() as u32);
                        bus.write_io(self.bc(), value);
                        self.set_hl(self.hl().wrapping_sub(1));
                        self.b = self.b.wrapping_sub(1);

                        self.set_flag(flags::FLAG_Z, true);
                        self.set_flag(flags::FLAG_N, true);

                        if self.b != 0 {
                            self.pc = self.pc.wrapping_sub(2);
                            21
                        } else {
                            16
                        }
                    }
                    _ => todo!("ED opcode {:#04X}", op2),
                }
            }
            0xEE => {
                // XOR n
                let n = self.fetch(bus);
                self.xor_a(n);
                7
            }
            0xEF => {
                // RST 28h
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);
                self.pc = 0x0028;
                11
            }
            0xF0 => {
                // RET P
                if !self.get_flag(flags::FLAG_S) {
                    let low = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    let high = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = (high as u16) << 8 | low as u16;
                    11
                } else {
                    5
                }
            }
            0xF1 => {
                // POP AF
                self.f = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                self.a = bus.read(self.sp as u32);
                self.sp = self.sp.wrapping_add(1);
                10
            }
            0xF2 => {
                // JP P, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if !self.get_flag(flags::FLAG_S) {
                    self.pc = addr;
                }
                10
            }
            0xF3 => {
                // DI
                self.iff1 = false;
                self.iff2 = false;
                4
            }
            0xF4 => {
                // CALL P, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if !self.get_flag(flags::FLAG_S) {
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, self.pc as u8);
                    self.pc = addr;
                    17
                } else {
                    10
                }
            }
            0xF5 => {
                // PUSH AF
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.a);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.f);
                11
            }
            0xF6 => {
                // OR n
                let n = self.fetch(bus);
                self.or_a(n);
                7
            }
            0xF7 => {
                // RST 30h
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);
                self.pc = 0x0030;
                11
            }
            0xF8 => {
                // RET M
                if self.get_flag(flags::FLAG_S) {
                    let low = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    let high = bus.read(self.sp as u32);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = (high as u16) << 8 | low as u16;
                    11
                } else {
                    5
                }
            }
            0xF9 => {
                // LD SP, HL
                self.sp = self.hl();
                6
            }
            0xFA => {
                // JP M, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if self.get_flag(flags::FLAG_S) {
                    self.pc = addr;
                }
                10
            }
            0xFB => {
                // EI
                self.iff1 = true;
                self.iff2 = true;
                4
            }
            0xFD => {
                let op2 = self.fetch(bus);
                match op2 {
                    0x09 => {
                        // ADD IY, BC
                        let iy = self.iy;
                        let bc = self.bc();
                        let result = (iy as u32) + (bc as u32);

                        self.set_flag(flags::FLAG_H, (iy & 0x0FFF) + (bc & 0x0FFF) > 0x0FFF);
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.iy = result as u16;
                        15
                    }
                    0x19 => {
                        // ADD IY, DE
                        let iy = self.iy;
                        let de = self.de();
                        let result = (iy as u32) + (de as u32);

                        self.set_flag(flags::FLAG_H, (iy & 0x0FFF) + (de & 0x0FFF) > 0x0FFF);
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.iy = result as u16;
                        15
                    }
                    0x21 => {
                        // LD IY, nn
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        self.iy = (high as u16) << 8 | low as u16;
                        14
                    }
                    0x22 => {
                        // LD (nn), IY
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        bus.write(addr as u32, self.iy as u8);
                        bus.write((addr.wrapping_add(1)) as u32, (self.iy >> 8) as u8);
                        20
                    }
                    0x23 => {
                        // INC IY
                        self.iy = self.iy.wrapping_add(1);
                        10
                    }
                    0x29 => {
                        // ADD IY, IY
                        let iy = self.iy;
                        let result = (iy as u32) + (iy as u32);

                        self.set_flag(flags::FLAG_H, (iy & 0x0FFF) + (iy & 0x0FFF) > 0x0FFF);
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.iy = result as u16;
                        15
                    }
                    0x2A => {
                        // LD IY, (nn)
                        let low = self.fetch(bus);
                        let high = self.fetch(bus);
                        let addr = (high as u16) << 8 | low as u16;
                        let iy_low = bus.read(addr as u32);
                        let iy_high = bus.read((addr.wrapping_add(1)) as u32);
                        self.iy = (iy_high as u16) << 8 | iy_low as u16;
                        20
                    }
                    0x2B => {
                        // DEC IY
                        self.iy = self.iy.wrapping_sub(1);
                        10
                    }
                    0x34 => {
                        // INC (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        let result = value.wrapping_add(1);
                        bus.write(addr, result);

                        self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, result == 0);
                        self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                        self.set_flag(flags::FLAG_PV, value == 0x7F);
                        self.set_flag(flags::FLAG_N, false);
                        23
                    }
                    0x35 => {
                        // DEC (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        let result = value.wrapping_sub(1);
                        bus.write(addr, result);

                        self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                        self.set_flag(flags::FLAG_Z, result == 0);
                        self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                        self.set_flag(flags::FLAG_PV, value == 0x80);
                        self.set_flag(flags::FLAG_N, true);
                        23
                    }
                    0x36 => {
                        // LD (IY+d), n
                        let d = self.fetch(bus) as i8;
                        let n = self.fetch(bus);
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        bus.write(addr, n);
                        19
                    }
                    0x39 => {
                        // ADD IY, SP
                        let iy = self.iy;
                        let sp = self.sp;
                        let result = (iy as u32) + (sp as u32);

                        self.set_flag(flags::FLAG_H, (iy & 0x0FFF) + (sp & 0x0FFF) > 0x0FFF);
                        self.set_flag(flags::FLAG_N, false);
                        self.set_flag(flags::FLAG_C, result > 0xFFFF);

                        self.iy = result as u16;
                        15
                    }
                    0x46 => {
                        // LD B, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        self.b = bus.read(addr);
                        19
                    }
                    0x4E => {
                        // LD C, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        self.c = bus.read(addr);
                        19
                    }
                    0x56 => {
                        // LD D, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        self.d = bus.read(addr);
                        19
                    }
                    0x5E => {
                        // LD E, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        self.e = bus.read(addr);
                        19
                    }
                    0x66 => {
                        // LD H, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        self.h = bus.read(addr);
                        19
                    }
                    0x6E => {
                        // LD L, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        self.l = bus.read(addr);
                        19
                    }
                    0x70 => {
                        // LD (IY+d), B
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.b);
                        19
                    }
                    0x71 => {
                        // LD (IY+d), C
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.c);
                        19
                    }
                    0x72 => {
                        // LD (IY+d), D
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.d);
                        19
                    }
                    0x73 => {
                        // LD (IY+d), E
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.e);
                        19
                    }
                    0x74 => {
                        // LD (IY+d), H
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.h);
                        19
                    }
                    0x75 => {
                        // LD (IY+d), L
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.l);
                        19
                    }
                    0x77 => {
                        // LD (IY+d), A
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        bus.write(addr, self.a);
                        19
                    }
                    0x7E => {
                        // LD A, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        self.a = bus.read(addr);
                        19
                    }
                    0x86 => {
                        // ADD A, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.add_a(value);
                        19
                    }
                    0x8E => {
                        // ADC A, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.adc_a(value);
                        19
                    }
                    0x96 => {
                        // SUB (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.sub_a(value);
                        19
                    }
                    0x9E => {
                        // SBC A, (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.sbc_a(value);
                        19
                    }
                    0xA6 => {
                        // AND (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.and_a(value);
                        19
                    }
                    0xAE => {
                        // XOR (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.xor_a(value);
                        19
                    }
                    0xB6 => {
                        // OR (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.or_a(value);
                        19
                    }
                    0xBE => {
                        // CP (IY+d)
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let value = bus.read(addr);
                        self.cp_a(value);
                        19
                    }
                    0xCB => {
                        let d = self.fetch(bus) as i8;
                        let addr = self.iy.wrapping_add(d as u16) as u32;
                        let op3 = self.fetch(bus);

                        let x = op3 >> 6;
                        let bit = (op3 >> 3) & 0x07;

                        match x {
                            0 => {
                                // Rotate/shift operations on (IY+d)
                                let value = bus.read(addr);
                                let result = match bit {
                                    0 => {
                                        // RLC (IY+d)
                                        let c = value >> 7;
                                        self.set_flag(flags::FLAG_C, c != 0);
                                        (value << 1) | c
                                    }
                                    1 => {
                                        // RRC (IY+d)
                                        let c = value & 1;
                                        self.set_flag(flags::FLAG_C, c != 0);
                                        (value >> 1) | (c << 7)
                                    }
                                    2 => {
                                        // RL (IY+d)
                                        let old_c = if self.carry() { 1 } else { 0 };
                                        self.set_flag(flags::FLAG_C, value & 0x80 != 0);
                                        (value << 1) | old_c
                                    }
                                    3 => {
                                        // RR (IY+d)
                                        let old_c = if self.carry() { 0x80 } else { 0 };
                                        self.set_flag(flags::FLAG_C, value & 1 != 0);
                                        (value >> 1) | old_c
                                    }
                                    4 => {
                                        // SLA (IY+d)
                                        self.set_flag(flags::FLAG_C, value & 0x80 != 0);
                                        value << 1
                                    }
                                    5 => {
                                        // SRA (IY+d)
                                        self.set_flag(flags::FLAG_C, value & 1 != 0);
                                        (value >> 1) | (value & 0x80)
                                    }
                                    6 => {
                                        // SLL (IY+d) - undocumented
                                        self.set_flag(flags::FLAG_C, value & 0x80 != 0);
                                        (value << 1) | 1
                                    }
                                    7 => {
                                        // SRL (IY+d)
                                        self.set_flag(flags::FLAG_C, value & 1 != 0);
                                        value >> 1
                                    }
                                    _ => unreachable!(),
                                };
                                bus.write(addr, result);
                                self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                                self.set_flag(flags::FLAG_Z, result == 0);
                                self.set_flag(flags::FLAG_H, false);
                                self.set_flag(flags::FLAG_PV, result.count_ones() % 2 == 0);
                                self.set_flag(flags::FLAG_N, false);
                                23
                            }
                            1 => {
                                // BIT b, (IY+d)
                                let value = bus.read(addr);
                                self.set_flag(flags::FLAG_Z, value & (1 << bit) == 0);
                                self.set_flag(flags::FLAG_H, true);
                                self.set_flag(flags::FLAG_N, false);
                                20
                            }
                            2 => {
                                // RES b, (IY+d)
                                let value = bus.read(addr);
                                bus.write(addr, value & !(1 << bit));
                                23
                            }
                            3 => {
                                // SET b, (IY+d)
                                let value = bus.read(addr);
                                bus.write(addr, value | (1 << bit));
                                23
                            }
                            _ => unreachable!(),
                        }
                    }
                    0xE1 => {
                        // POP IY
                        let low = bus.read(self.sp as u32);
                        self.sp = self.sp.wrapping_add(1);
                        let high = bus.read(self.sp as u32);
                        self.sp = self.sp.wrapping_add(1);
                        self.iy = (high as u16) << 8 | low as u16;
                        14
                    }
                    0xE3 => {
                        // EX (SP), IY
                        let low = bus.read(self.sp as u32);
                        let high = bus.read((self.sp.wrapping_add(1)) as u32);
                        bus.write(self.sp as u32, self.iy as u8);
                        bus.write((self.sp.wrapping_add(1)) as u32, (self.iy >> 8) as u8);
                        self.iy = (high as u16) << 8 | low as u16;
                        23
                    }
                    0xE5 => {
                        // PUSH IY
                        self.sp = self.sp.wrapping_sub(1);
                        bus.write(self.sp as u32, (self.iy >> 8) as u8);
                        self.sp = self.sp.wrapping_sub(1);
                        bus.write(self.sp as u32, self.iy as u8);
                        15
                    }
                    0xE9 => {
                        // JP (IY)
                        self.pc = self.iy;
                        8
                    }
                    0xF9 => {
                        // LD SP, IY
                        self.sp = self.iy;
                        10
                    }
                    _ => todo!("FD opcode {:#04X}", op2),
                }
            }
            0xFC => {
                // CALL M, nn
                let low = self.fetch(bus);
                let high = self.fetch(bus);
                let addr = (high as u16) << 8 | low as u16;
                if self.get_flag(flags::FLAG_S) {
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    bus.write(self.sp as u32, self.pc as u8);
                    self.pc = addr;
                    17
                } else {
                    10
                }
            }
            0xFE => {
                // CP n
                let n = self.fetch(bus);
                self.cp_a(n);
                7
            }
            0xFF => {
                // RST 38h
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);
                self.pc = 0x0038;
                11
            }
            // LD r, r' (including (HL) cases)
            // Note: 0x76 (HALT) is handled by explicit case above
            op if (op & 0b11000000) == 0b01000000 => {
                let dst = (op >> 3) & 0b111;
                let src = op & 0b111;

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
            // XOR r
            op if (op & 0b11111000) == 0b10101000 => {
                let src = op & 0b111;
                let value = if src == 6 {
                    bus.read(self.hl() as u32)
                } else {
                    self.read_register(src)
                };
                self.xor_a(value);
                if src == 6 { 7 } else { 4 }
            }
            // CP r
            op if (op & 0b11111000) == 0b10111000 => {
                let src = op & 0b111;
                let value = if src == 6 {
                    bus.read(self.hl() as u32)
                } else {
                    self.read_register(src)
                };
                self.cp_a(value);
                if src == 6 { 7 } else { 4 }
            }
            // AND r
            op if (op & 0b11111000) == 0b10100000 => {
                let src = op & 0b111;
                let value = if src == 6 {
                    bus.read(self.hl() as u32)
                } else {
                    self.read_register(src)
                };
                self.and_a(value);
                if src == 6 { 7 } else { 4 }
            }
            // INC r
            op if (op & 0b11000111) == 0b00000100 => {
                let reg = (op >> 3) & 0b111;

                if reg == 6 {
                    // INC (HL)
                    let addr = self.hl() as u32;
                    let value = bus.read(addr);
                    let result = value.wrapping_add(1);
                    bus.write(addr, result);

                    self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                    self.set_flag(flags::FLAG_Z, result == 0);
                    self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                    self.set_flag(flags::FLAG_PV, value == 0x7F);
                    self.set_flag(flags::FLAG_N, false);
                    11
                } else {
                    let value = self.read_register(reg);
                    let result = value.wrapping_add(1);
                    self.set_register(reg, result);

                    self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                    self.set_flag(flags::FLAG_Z, result == 0);
                    self.set_flag(flags::FLAG_H, (value & 0x0F) == 0x0F);
                    self.set_flag(flags::FLAG_PV, value == 0x7F);
                    self.set_flag(flags::FLAG_N, false);
                    4
                }
            }
            // SUB r
            op if (op & 0b11111000) == 0b10010000 => {
                let src = op & 0b111;
                let value = if src == 6 {
                    bus.read(self.hl() as u32)
                } else {
                    self.read_register(src)
                };
                self.sub_a(value);
                if src == 6 { 7 } else { 4 }
            }
            // OR r
            op if (op & 0b11111000) == 0b10110000 => {
                let src = op & 0b111;
                let value = if src == 6 {
                    bus.read(self.hl() as u32)
                } else {
                    self.read_register(src)
                };
                self.or_a(value);
                if src == 6 { 7 } else { 4 }
            }
            // DEC r
            op if (op & 0b11000111) == 0b00000101 => {
                let reg = (op >> 3) & 0b111;

                if reg == 6 {
                    // DEC (HL) - already handled at 0x35
                    let addr = self.hl() as u32;
                    let value = bus.read(addr);
                    let result = value.wrapping_sub(1);
                    bus.write(addr, result);

                    self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                    self.set_flag(flags::FLAG_Z, result == 0);
                    self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                    self.set_flag(flags::FLAG_PV, value == 0x80);
                    self.set_flag(flags::FLAG_N, true);
                    11
                } else {
                    let value = self.read_register(reg);
                    let result = value.wrapping_sub(1);
                    self.set_register(reg, result);

                    self.set_flag(flags::FLAG_S, result & 0x80 != 0);
                    self.set_flag(flags::FLAG_Z, result == 0);
                    self.set_flag(flags::FLAG_H, (value & 0x0F) == 0);
                    self.set_flag(flags::FLAG_PV, value == 0x80);
                    self.set_flag(flags::FLAG_N, true);
                    4
                }
            }
            // RST n
            op if (op & 0b11000111) == 0b11000111 => {
                let addr = (op & 0b00111000) as u16;
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                bus.write(self.sp as u32, self.pc as u8);
                self.pc = addr;
                11
            }
            // SBC A, r
            op if (op & 0b11111000) == 0b10011000 => {
                let src = op & 0b111;
                let value = if src == 6 {
                    bus.read(self.hl() as u32)
                } else {
                    self.read_register(src)
                };
                self.sbc_a(value);
                if src == 6 { 7 } else { 4 }
            }
            // ADC A, r
            op if (op & 0b11111000) == 0b10001000 => {
                let src = op & 0b111;
                let value = if src == 6 {
                    bus.read(self.hl() as u32)
                } else {
                    self.read_register(src)
                };
                self.adc_a(value);
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
        if !self.iff1 {
            return;
        }

        self.halted = false;
        self.iff1 = false;
        self.iff2 = false;

        // IM 1: push PC, jump to 0x0038
        if self.interrupt_mode == 1 {
            self.sp = self.sp.wrapping_sub(1);
            _bus.write(self.sp as u32, (self.pc >> 8) as u8);
            self.sp = self.sp.wrapping_sub(1);
            _bus.write(self.sp as u32, self.pc as u8);
            self.pc = 0x0038;
        }
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

    #[test]
    fn inc_hl_increments() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::new();

        cpu.h = 0x40;
        cpu.l = 0xFF;
        bus.memory[0] = 0x23; // INC HL

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.h, 0x41);
        assert_eq!(cpu.l, 0x00);
    }

    #[test]
    fn jp_nn_jumps() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::new();

        bus.memory[0] = 0xC3; // JP nn
        bus.memory[1] = 0x00; // low byte
        bus.memory[2] = 0x40; // high byte

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 10);
        assert_eq!(cpu.pc, 0x4000);
    }

    #[test]
    fn ld_hl_nn_loads_immediate() {
        let mut cpu = Z80::new();
        let mut bus = TestBus::new();

        bus.memory[0] = 0x21; // LD HL, nn
        bus.memory[1] = 0x00; // low byte
        bus.memory[2] = 0x40; // high byte

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 10);
        assert_eq!(cpu.h, 0x40);
        assert_eq!(cpu.l, 0x00);
    }
}
