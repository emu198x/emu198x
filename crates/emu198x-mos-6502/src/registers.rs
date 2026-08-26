use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registers {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub p: u8,
}

pub const FLAG_C: u8 = 0x01;
pub const FLAG_Z: u8 = 0x02;
pub const FLAG_I: u8 = 0x04;
pub const FLAG_D: u8 = 0x08;
pub const FLAG_B: u8 = 0x10;
pub const FLAG_U: u8 = 0x20;
pub const FLAG_V: u8 = 0x40;
pub const FLAG_N: u8 = 0x80;

impl Registers {
    /// Construct a freshly-powered Registers in the cold-boot state.
    ///
    /// On real silicon the stack pointer is undefined at power-on
    /// (the bits start at whatever the SRAM cells decay to). It
    /// becomes `$FD` *after* the 7-cycle reset sequence runs and
    /// decrements SP by 3 from its starting value. We model that
    /// undefined state as `$00` here so that
    /// [`M6502::reset`](crate::M6502::reset)'s `wrapping_sub(3)`
    /// produces the canonical post-reset `$FD` — the value blargg's
    /// `cpu_reset/registers` CRC expects to see at power.
    #[must_use]
    pub fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0x00,
            pc: 0,
            p: FLAG_U | FLAG_I,
        }
    }

    #[must_use]
    pub fn flag(&self, mask: u8) -> bool {
        self.p & mask != 0
    }

    pub fn set_flag(&mut self, mask: u8, value: bool) {
        if value {
            self.p |= mask;
        } else {
            self.p &= !mask;
        }
    }

    #[must_use]
    pub fn carry(&self) -> bool {
        self.flag(FLAG_C)
    }

    #[must_use]
    pub fn zero(&self) -> bool {
        self.flag(FLAG_Z)
    }

    #[must_use]
    pub fn interrupt_disable(&self) -> bool {
        self.flag(FLAG_I)
    }

    #[must_use]
    pub fn decimal(&self) -> bool {
        self.flag(FLAG_D)
    }

    #[must_use]
    pub fn overflow(&self) -> bool {
        self.flag(FLAG_V)
    }

    #[must_use]
    pub fn negative(&self) -> bool {
        self.flag(FLAG_N)
    }

    pub fn set_nz(&mut self, value: u8) {
        self.set_flag(FLAG_N, value & 0x80 != 0);
        self.set_flag(FLAG_Z, value == 0);
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
    fn power_on_state() {
        let registers = Registers::new();
        assert_eq!(registers.a, 0);
        assert_eq!(registers.x, 0);
        assert_eq!(registers.y, 0);
        // Cold-boot SP is modelled as $00; the 7-cycle reset
        // sequence then decrements by 3 to the canonical $FD.
        assert_eq!(registers.sp, 0x00);
        assert!(registers.flag(FLAG_U));
        assert!(registers.interrupt_disable());
    }

    #[test]
    fn flag_operations() {
        let mut registers = Registers::new();
        registers.set_flag(FLAG_C, true);
        assert!(registers.carry());
        registers.set_flag(FLAG_C, false);
        assert!(!registers.carry());
    }

    #[test]
    fn nz_flags_follow_input() {
        let mut registers = Registers::new();
        registers.set_nz(0);
        assert!(registers.zero());
        assert!(!registers.negative());

        registers.set_nz(0x80);
        assert!(!registers.zero());
        assert!(registers.negative());

        registers.set_nz(0x42);
        assert!(!registers.zero());
        assert!(!registers.negative());
    }
}
