/// Z80 register file.
///
/// Registers are stored as 16-bit pairs. The high/low byte accessors
/// handle the AF pair specially (A is high, F is low).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Registers {
    pub af: u16,
    pub bc: u16,
    pub de: u16,
    pub hl: u16,
    pub af_alt: u16,
    pub bc_alt: u16,
    pub de_alt: u16,
    pub hl_alt: u16,
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    pub r: u8,
    pub iff1: bool,
    pub iff2: bool,
    pub im: u8, // 0, 1, or 2
    /// Internal WZ register (MEMPTR). Affects undocumented flag behaviour.
    pub wz: u16,
    /// Q register: tracks the last F value set by a flag-modifying instruction.
    /// Set to F by flag-modifying instructions (via set_f_q), 0 by others.
    pub q: u8,
    /// Previous instruction's Q value. Used by SCF/CCF to determine bits 3/5:
    /// If prev_q == F, bits 3/5 from A only. If prev_q != F, bits 3/5 from A | F.
    pub prev_q: u8,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            af: 0xFFFF,
            bc: 0x0000,
            de: 0x0000,
            hl: 0x0000,
            af_alt: 0x0000,
            bc_alt: 0x0000,
            de_alt: 0x0000,
            hl_alt: 0x0000,
            ix: 0x0000,
            iy: 0x0000,
            sp: 0xFFFF,
            pc: 0x0000,
            i: 0x00,
            r: 0x00,
            iff1: false,
            iff2: false,
            im: 0,
            wz: 0x0000,
            q: 0,
            prev_q: 0,
        }
    }
}

// 8-bit register accessors
impl Registers {
    #[inline]
    pub fn a(&self) -> u8 {
        (self.af >> 8) as u8
    }
    #[inline]
    pub fn set_a(&mut self, v: u8) {
        self.af = (self.af & 0x00FF) | ((v as u16) << 8);
    }
    #[inline]
    pub fn f(&self) -> u8 {
        self.af as u8
    }
    #[inline]
    pub fn set_f(&mut self, v: u8) {
        self.af = (self.af & 0xFF00) | v as u16;
    }

    #[inline]
    pub fn b(&self) -> u8 {
        (self.bc >> 8) as u8
    }
    #[inline]
    pub fn set_b(&mut self, v: u8) {
        self.bc = (self.bc & 0x00FF) | ((v as u16) << 8);
    }
    #[inline]
    pub fn c(&self) -> u8 {
        self.bc as u8
    }
    #[inline]
    pub fn set_c(&mut self, v: u8) {
        self.bc = (self.bc & 0xFF00) | v as u16;
    }

    #[inline]
    pub fn d(&self) -> u8 {
        (self.de >> 8) as u8
    }
    #[inline]
    pub fn set_d(&mut self, v: u8) {
        self.de = (self.de & 0x00FF) | ((v as u16) << 8);
    }
    #[inline]
    pub fn e(&self) -> u8 {
        self.de as u8
    }
    #[inline]
    pub fn set_e(&mut self, v: u8) {
        self.de = (self.de & 0xFF00) | v as u16;
    }

    #[inline]
    pub fn h(&self) -> u8 {
        (self.hl >> 8) as u8
    }
    #[inline]
    pub fn set_h(&mut self, v: u8) {
        self.hl = (self.hl & 0x00FF) | ((v as u16) << 8);
    }
    #[inline]
    pub fn l(&self) -> u8 {
        self.hl as u8
    }
    #[inline]
    pub fn set_l(&mut self, v: u8) {
        self.hl = (self.hl & 0xFF00) | v as u16;
    }

    #[inline]
    pub fn ixh(&self) -> u8 {
        (self.ix >> 8) as u8
    }
    #[inline]
    pub fn set_ixh(&mut self, v: u8) {
        self.ix = (self.ix & 0x00FF) | ((v as u16) << 8);
    }
    #[inline]
    pub fn ixl(&self) -> u8 {
        self.ix as u8
    }
    #[inline]
    pub fn set_ixl(&mut self, v: u8) {
        self.ix = (self.ix & 0xFF00) | v as u16;
    }

    #[inline]
    pub fn iyh(&self) -> u8 {
        (self.iy >> 8) as u8
    }
    #[inline]
    pub fn set_iyh(&mut self, v: u8) {
        self.iy = (self.iy & 0x00FF) | ((v as u16) << 8);
    }
    #[inline]
    pub fn iyl(&self) -> u8 {
        self.iy as u8
    }
    #[inline]
    pub fn set_iyl(&mut self, v: u8) {
        self.iy = (self.iy & 0xFF00) | v as u16;
    }

    #[inline]
    pub fn w(&self) -> u8 {
        (self.wz >> 8) as u8
    }
    #[inline]
    pub fn z(&self) -> u8 {
        self.wz as u8
    }

    /// Compute the IR register pair (I in high byte, R in low byte).
    /// Used for refresh address and contention checks.
    #[inline]
    pub fn ir(&self) -> u16 {
        ((self.i as u16) << 8) | self.r as u16
    }

    /// Increment R. Only the low 7 bits count; bit 7 is preserved.
    #[inline]
    pub fn inc_r(&mut self) {
        self.r = (self.r & 0x80) | ((self.r.wrapping_add(1)) & 0x7F);
    }

    /// Exchange AF and AF'
    #[inline]
    pub fn ex_af(&mut self) {
        std::mem::swap(&mut self.af, &mut self.af_alt);
    }

    /// Exchange BC, DE, HL with BC', DE', HL'
    #[inline]
    pub fn exx(&mut self) {
        std::mem::swap(&mut self.bc, &mut self.bc_alt);
        std::mem::swap(&mut self.de, &mut self.de_alt);
        std::mem::swap(&mut self.hl, &mut self.hl_alt);
    }

    /// Exchange DE and HL
    #[inline]
    pub fn ex_de_hl(&mut self) {
        std::mem::swap(&mut self.de, &mut self.hl);
    }
}

// Flag bit constants
pub const FLAG_C: u8 = 0x01; // Carry
pub const FLAG_N: u8 = 0x02; // Subtract
pub const FLAG_PV: u8 = 0x04; // Parity/Overflow
pub const FLAG_3: u8 = 0x08; // Undocumented bit 3
pub const FLAG_H: u8 = 0x10; // Half-carry
pub const FLAG_5: u8 = 0x20; // Undocumented bit 5
pub const FLAG_Z: u8 = 0x40; // Zero
pub const FLAG_S: u8 = 0x80; // Sign

impl Registers {
    #[inline]
    pub fn flag(&self, mask: u8) -> bool {
        self.f() & mask != 0
    }

    /// Set F and update Q to match (tracks flag modification for SCF/CCF).
    #[inline]
    pub fn set_f_q(&mut self, v: u8) {
        self.set_f(v);
        self.q = v;
    }

    #[inline]
    pub fn set_flag(&mut self, mask: u8, val: bool) {
        if val {
            self.set_f(self.f() | mask);
        } else {
            self.set_f(self.f() & !mask);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_accessors() {
        let mut r = Registers {
            af: 0x1234,
            ..Registers::default()
        };
        assert_eq!(r.a(), 0x12);
        assert_eq!(r.f(), 0x34);

        r.set_a(0xAB);
        assert_eq!(r.a(), 0xAB);
        assert_eq!(r.f(), 0x34);
        assert_eq!(r.af, 0xAB34);

        r.set_f(0xCD);
        assert_eq!(r.af, 0xABCD);
    }

    #[test]
    fn r_increment_preserves_bit7() {
        let mut r = Registers {
            r: 0x80,
            ..Registers::default()
        };
        r.inc_r();
        assert_eq!(r.r, 0x81);

        r.r = 0xFF;
        r.inc_r();
        assert_eq!(r.r, 0x80); // bit 7 preserved, low 7 wrap to 0
    }

    #[test]
    fn exchange_operations() {
        let mut r = Registers {
            af: 0x1111,
            af_alt: 0x2222,
            ..Registers::default()
        };
        r.ex_af();
        assert_eq!(r.af, 0x2222);
        assert_eq!(r.af_alt, 0x1111);

        r.bc = 0xAAAA;
        r.de = 0xBBBB;
        r.hl = 0xCCCC;
        r.bc_alt = 0x1111;
        r.de_alt = 0x2222;
        r.hl_alt = 0x3333;
        r.exx();
        assert_eq!(r.bc, 0x1111);
        assert_eq!(r.de, 0x2222);
        assert_eq!(r.hl, 0x3333);
    }

    #[test]
    fn ir_register() {
        let r = Registers {
            i: 0x3F,
            r: 0x42,
            ..Registers::default()
        };
        assert_eq!(r.ir(), 0x3F42);
    }
}
