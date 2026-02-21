//! Z80 register set.

#![allow(clippy::cast_possible_truncation)] // Intentional truncation for low byte extraction.

/// Z80 registers snapshot for observation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Registers {
    // Main registers
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,

    // Alternate registers
    pub a_alt: u8,
    pub f_alt: u8,
    pub b_alt: u8,
    pub c_alt: u8,
    pub d_alt: u8,
    pub e_alt: u8,
    pub h_alt: u8,
    pub l_alt: u8,

    // Index registers
    pub ix: u16,
    pub iy: u16,

    // Other registers
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    pub r: u8,

    // Internal registers
    /// WZ/MEMPTR - internal temporary register.
    /// Affects undocumented X/Y flags in BIT instructions and some jumps.
    pub wz: u16,

    // Interrupt state
    pub iff1: bool,
    pub iff2: bool,
    pub im: u8,

    // Halt state
    pub halted: bool,
}

impl Registers {
    /// Get AF register pair.
    #[must_use]
    pub const fn af(&self) -> u16 {
        (self.a as u16) << 8 | self.f as u16
    }

    /// Get BC register pair.
    #[must_use]
    pub const fn bc(&self) -> u16 {
        (self.b as u16) << 8 | self.c as u16
    }

    /// Get DE register pair.
    #[must_use]
    pub const fn de(&self) -> u16 {
        (self.d as u16) << 8 | self.e as u16
    }

    /// Get HL register pair.
    #[must_use]
    pub const fn hl(&self) -> u16 {
        (self.h as u16) << 8 | self.l as u16
    }

    /// Set AF register pair.
    pub fn set_af(&mut self, value: u16) {
        self.a = (value >> 8) as u8;
        self.f = value as u8;
    }

    /// Set BC register pair.
    pub fn set_bc(&mut self, value: u16) {
        self.b = (value >> 8) as u8;
        self.c = value as u8;
    }

    /// Set DE register pair.
    pub fn set_de(&mut self, value: u16) {
        self.d = (value >> 8) as u8;
        self.e = value as u8;
    }

    /// Set HL register pair.
    pub fn set_hl(&mut self, value: u16) {
        self.h = (value >> 8) as u8;
        self.l = value as u8;
    }
}
