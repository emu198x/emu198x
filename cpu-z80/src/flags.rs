use crate::Z80;

// Flag bit positions
pub const FLAG_C: u8 = 0; // Carry
pub const FLAG_N: u8 = 1; // Add/Subtract
pub const FLAG_PV: u8 = 2; // Parity/Overflow
pub const FLAG_H: u8 = 4; // Half-carry
pub const FLAG_Z: u8 = 6; // Zero
pub const FLAG_S: u8 = 7; // Sign

impl Z80 {
    pub(crate) fn get_flag(&self, flag: u8) -> bool {
        (self.f & (1 << flag)) != 0
    }

    pub(crate) fn set_flag(&mut self, flag: u8, value: bool) {
        if value {
            self.f |= 1 << flag;
        } else {
            self.f &= !(1 << flag);
        }
    }

    pub(crate) fn carry(&self) -> bool {
        self.get_flag(FLAG_C)
    }

    pub(crate) fn zero(&self) -> bool {
        self.get_flag(FLAG_Z)
    }

    pub(crate) fn add_a(&mut self, value: u8) {
        let a = self.a;
        let result = a.wrapping_add(value);

        self.set_flag(FLAG_S, result & 0x80 != 0);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_H, (a & 0x0F) + (value & 0x0F) > 0x0F);
        self.set_flag(
            FLAG_PV,
            // Overflow: signs of inputs same, sign of result different
            (a ^ value) & 0x80 == 0 && (a ^ result) & 0x80 != 0,
        );
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_C, (a as u16) + (value as u16) > 0xFF);

        self.a = result;
    }

    pub(crate) fn xor_a(&mut self, value: u8) {
        self.a ^= value;

        self.set_flag(FLAG_S, self.a & 0x80 != 0);
        self.set_flag(FLAG_Z, self.a == 0);
        self.set_flag(FLAG_H, false);
        self.set_flag(FLAG_PV, self.a.count_ones() % 2 == 0); // parity
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_C, false);
    }

    pub(crate) fn cp_a(&mut self, value: u8) {
        let a = self.a;
        let result = a.wrapping_sub(value);

        self.set_flag(FLAG_S, result & 0x80 != 0);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_H, (a & 0x0F) < (value & 0x0F));
        self.set_flag(FLAG_PV, (a ^ value) & 0x80 != 0 && (a ^ result) & 0x80 != 0);
        self.set_flag(FLAG_N, true);
        self.set_flag(FLAG_C, a < value);
    }

    pub(crate) fn and_a(&mut self, value: u8) {
        self.a &= value;

        self.set_flag(FLAG_S, self.a & 0x80 != 0);
        self.set_flag(FLAG_Z, self.a == 0);
        self.set_flag(FLAG_H, true); // AND always sets H
        self.set_flag(FLAG_PV, self.a.count_ones() % 2 == 0); // parity
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_C, false);
    }

    pub(crate) fn sub_a(&mut self, value: u8) {
        let a = self.a;
        let result = a.wrapping_sub(value);

        self.set_flag(FLAG_S, result & 0x80 != 0);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_H, (a & 0x0F) < (value & 0x0F));
        self.set_flag(FLAG_PV, (a ^ value) & 0x80 != 0 && (a ^ result) & 0x80 != 0);
        self.set_flag(FLAG_N, true);
        self.set_flag(FLAG_C, a < value);

        self.a = result;
    }

    pub(crate) fn or_a(&mut self, value: u8) {
        self.a |= value;

        self.set_flag(FLAG_S, self.a & 0x80 != 0);
        self.set_flag(FLAG_Z, self.a == 0);
        self.set_flag(FLAG_H, false);
        self.set_flag(FLAG_PV, self.a.count_ones() % 2 == 0);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_C, false);
    }

    pub(crate) fn sbc_a(&mut self, value: u8) {
        let a = self.a;
        let c = if self.carry() { 1 } else { 0 };
        let result = a.wrapping_sub(value).wrapping_sub(c);

        self.set_flag(FLAG_S, result & 0x80 != 0);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_H, (a & 0x0F) < (value & 0x0F) + c);
        self.set_flag(FLAG_PV, (a ^ value) & 0x80 != 0 && (a ^ result) & 0x80 != 0);
        self.set_flag(FLAG_N, true);
        self.set_flag(FLAG_C, (a as u16) < (value as u16) + (c as u16));

        self.a = result;
    }
}
