use serde::{Deserialize, Serialize};

pub const FLAG_E: u8 = 0x80;
pub const FLAG_F: u8 = 0x40;
pub const FLAG_H: u8 = 0x20;
pub const FLAG_I: u8 = 0x10;
pub const FLAG_N: u8 = 0x08;
pub const FLAG_Z: u8 = 0x04;
pub const FLAG_V: u8 = 0x02;
pub const FLAG_C: u8 = 0x01;

/// MC6809 programmer-visible register file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Registers {
    pub a: u8,
    pub b: u8,
    pub dp: u8,
    pub cc: u8,
    pub x: u16,
    pub y: u16,
    pub u: u16,
    pub s: u16,
    pub pc: u16,
}

impl Registers {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            a: 0,
            b: 0,
            dp: 0,
            // Interrupt masks are set after reset. The remaining flags are
            // left clear until hardware-specific power-on behaviour is known.
            cc: FLAG_F | FLAG_I,
            x: 0,
            y: 0,
            u: 0,
            s: 0,
            pc: 0,
        }
    }

    #[must_use]
    pub const fn d(&self) -> u16 {
        u16::from_be_bytes([self.a, self.b])
    }

    pub fn set_d(&mut self, value: u16) {
        let [a, b] = value.to_be_bytes();
        self.a = a;
        self.b = b;
    }

    #[must_use]
    pub const fn flag(&self, mask: u8) -> bool {
        self.cc & mask != 0
    }

    pub fn set_flag(&mut self, mask: u8, value: bool) {
        if value {
            self.cc |= mask;
        } else {
            self.cc &= !mask;
        }
    }

    #[must_use]
    pub const fn irq_masked(&self) -> bool {
        self.flag(FLAG_I)
    }

    #[must_use]
    pub const fn firq_masked(&self) -> bool {
        self.flag(FLAG_F)
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d_combines_a_and_b_big_endian() {
        let mut regs = Registers::new();
        regs.a = 0x12;
        regs.b = 0x34;
        assert_eq!(regs.d(), 0x1234);

        regs.set_d(0xABCD);
        assert_eq!(regs.a, 0xAB);
        assert_eq!(regs.b, 0xCD);
    }

    #[test]
    fn condition_code_helpers_mutate_bits() {
        let mut regs = Registers::new();
        assert!(regs.irq_masked());
        assert!(regs.firq_masked());

        regs.set_flag(FLAG_I, false);
        regs.set_flag(FLAG_C, true);

        assert!(!regs.irq_masked());
        assert!(regs.flag(FLAG_C));
    }
}
