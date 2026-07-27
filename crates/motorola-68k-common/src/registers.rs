//! Motorola 68000 CPU registers.
//!
//! - D0-D7: 8 data registers (32-bit)
//! - A0-A7: 8 address registers (32-bit, A7 is the active stack pointer)
//! - USP: User stack pointer (A7 when in user mode)
//! - SSP/ISP: Supervisor or interrupt stack pointer
//! - MSP: Master stack pointer on processors with dual supervisor stacks
//! - PC: Program counter (32-bit, 24-bit on 68000)
//! - SR: Status register (16-bit)

use serde::{Deserialize, Serialize};

/// Architectural stack-pointer bank selected for an A7 access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackBank {
    /// User Stack Pointer.
    User,
    /// Supervisor Stack Pointer on the 68000/68010, or Interrupt Stack
    /// Pointer on processors with distinct supervisor stacks.
    Interrupt,
    /// Master Stack Pointer on processors with distinct supervisor stacks.
    Master,
}

/// FPU register value — a true 80-bit extended-precision float, stored
/// as Motorola/Intel `floatx80`: `high` holds the sign (bit 15) and the
/// 15-bit biased exponent (bits 14-0); `low` holds the 64-bit mantissa
/// including the explicit integer bit (bit 63). This matches Musashi's
/// `floatx80` layout exactly so register state is bit-comparable.
///
/// The arithmetic backend that operates on this representation is wired
/// incrementally (#112); the move/abs/neg/test ops need only these bit
/// fields, no float library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FpReg {
    /// Sign (bit 15) + 15-bit biased exponent (bits 14-0).
    pub high: u16,
    /// 64-bit mantissa with explicit integer bit (bit 63).
    pub low: u64,
}

impl FpReg {
    /// Positive zero (`+0.0`): exponent and mantissa all clear.
    pub const ZERO: Self = Self { high: 0, low: 0 };

    /// Construct from the raw 80-bit fields.
    #[must_use]
    pub const fn new(high: u16, low: u64) -> Self {
        Self { high, low }
    }

    /// True when the sign bit is set.
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.high & 0x8000 != 0
    }

    /// True for ±0 (exponent and mantissa fraction both zero), matching
    /// Musashi's `SET_CONDITION_CODES` zero test (`(low << 1) == 0`).
    #[must_use]
    pub const fn is_zero(self) -> bool {
        (self.high & 0x7FFF) == 0 && (self.low << 1) == 0
    }

    /// True for ±infinity (max exponent, zero fraction).
    #[must_use]
    pub const fn is_infinite(self) -> bool {
        (self.high & 0x7FFF) == 0x7FFF && (self.low << 1) == 0
    }

    /// True for NaN (max exponent, non-zero fraction).
    #[must_use]
    pub const fn is_nan(self) -> bool {
        (self.high & 0x7FFF) == 0x7FFF && (self.low << 1) != 0
    }

    /// Absolute value — clear the sign bit (FABS).
    #[must_use]
    pub const fn abs(self) -> Self {
        Self {
            high: self.high & 0x7FFF,
            low: self.low,
        }
    }

    /// Negate — flip the sign bit (FNEG).
    #[must_use]
    pub const fn negate(self) -> Self {
        Self {
            high: self.high ^ 0x8000,
            low: self.low,
        }
    }
}

/// 68000 CPU register set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registers {
    /// Data registers D0-D7.
    pub d: [u32; 8],
    /// Address registers A0-A6 (A7 is handled via USP/SSP).
    pub a: [u32; 7],
    /// User stack pointer (active A7 when in user mode). Used by every variant.
    pub usp: u32,
    /// Supervisor stack pointer (active A7 when in supervisor mode).
    /// On 68020+, also serves as the Interrupt Stack Pointer (ISP). Used by
    /// every variant.
    pub ssp: u32,
    /// Master Stack Pointer. **68020+ only** — selected as A7 when the SR
    /// M-flag (bit 12) is set. Zero / unused on M68000 / M68010, but kept on
    /// the shared register file so the serde envelope is variant-agnostic.
    pub msp: u32,
    /// Cache Address Register. **68020+ only** — points at the cache line a
    /// CINV / CPUSH instruction targets. Zero / unused on M68000 / M68010.
    pub caar: u32,
    /// Program counter.
    pub pc: u32,
    /// Status register.
    pub sr: u16,
    /// Vector Base Register. **68010+ only** — exception vector table base
    /// address. Zero / unused on M68000 (M68000 vector table is hard-fixed
    /// at $00000000).
    pub vbr: u32,
    /// Source Function Code register. **68010+ only** — 3-bit FC for MOVES
    /// source side. Zero / unused on M68000.
    pub sfc: u8,
    /// Destination Function Code register. **68010+ only** — 3-bit FC for
    /// MOVES destination side. Zero / unused on M68000.
    pub dfc: u8,
    /// Cache Control Register. **68020+ only** — enables and freezes the
    /// on-die instruction / data caches. Zero / unused on M68000 / M68010
    /// (no caches).
    pub cacr: u32,
    /// Translation Control register (68030 TC, 68040 TC).
    pub tc: u32,
    /// Transparent Translation register 0 (68030 TT0, 68040 ITT0).
    pub itt0: u32,
    /// Transparent Translation register 1 (68030 TT1, 68040 ITT1).
    pub itt1: u32,
    /// Data Transparent Translation register 0 (68040+).
    pub dtt0: u32,
    /// Data Transparent Translation register 1 (68040+).
    pub dtt1: u32,
    /// Supervisor Root Pointer — low 32 bits (68030: 64-bit descriptor; 68040+: 32-bit).
    pub srp: u32,
    /// Supervisor Root Pointer — high 32 bits (68030 only; 68040+ ignores).
    pub srp_upper: u32,
    /// CPU Root Pointer — low 32 bits (68030 CRP; 68040+ URP).
    pub urp: u32,
    /// CPU Root Pointer — high 32 bits (68030 only; 68040+ ignores).
    pub crp_upper: u32,
    /// MMU Status Register (68030 MMUSR / 68040 MMUSR).
    pub mmusr: u32,
    /// Bus Control Register (68060).
    pub buscr: u32,
    /// Processor Configuration Register (68060).
    pub pcr: u32,

    // --- FPU registers (68881/68882/68040+) ---
    /// Floating-point data registers FP0-FP7.
    pub fp: [FpReg; 8],
    /// FP Control Register (exception enables, rounding mode/precision).
    pub fpcr: u32,
    /// FP Status Register (condition codes, quotient, exception status/accrued).
    pub fpsr: u32,
    /// FP Instruction Address Register (PC of last FPU instruction).
    pub fpiar: u32,
    /// Whether this register file belongs to a processor with distinct
    /// interrupt and master supervisor stacks.
    ///
    /// This is variant configuration rather than architectural state, so
    /// wrappers restore it after deserialization. Keeping it here lets every
    /// shared A7 access select the correct bank without interpreting the
    /// reserved M bit on the 68000 or 68010.
    #[serde(skip)]
    master_stack_capable: bool,
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

impl Registers {
    /// Create registers in reset state.
    ///
    /// After reset: supervisor mode, interrupt mask level 7.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            msp: 0,
            caar: 0,
            pc: 0,
            sr: 0x2700, // Supervisor mode, interrupt level 7
            vbr: 0,
            sfc: 0,
            dfc: 0,
            cacr: 0,
            tc: 0,
            itt0: 0,
            itt1: 0,
            dtt0: 0,
            dtt1: 0,
            srp: 0,
            srp_upper: 0,
            urp: 0,
            crp_upper: 0,
            mmusr: 0,
            buscr: 0,
            pcr: 0,
            fp: [FpReg::ZERO; 8],
            fpcr: 0,
            fpsr: 0,
            fpiar: 0,
            master_stack_capable: false,
        }
    }

    /// FPCR rounding mode (bits 5-4): 0=RN, 1=RZ, 2=RM, 3=RP.
    #[must_use]
    pub const fn fpcr_rounding_mode(&self) -> u8 {
        ((self.fpcr >> 4) & 3) as u8
    }

    /// FPCR rounding precision (bits 7-6): 0=Extended, 1=Single, 2=Double.
    #[must_use]
    pub const fn fpcr_rounding_precision(&self) -> u8 {
        ((self.fpcr >> 6) & 3) as u8
    }

    /// FPSR condition codes (bits 27-24): N, Z, I, NAN.
    #[must_use]
    pub const fn fpsr_condition_code(&self) -> u8 {
        ((self.fpsr >> 24) & 0xF) as u8
    }

    /// Set FPSR condition code bits from individual flags.
    pub fn set_fpsr_cc(&mut self, n: bool, z: bool, i: bool, nan: bool) {
        self.fpsr = (self.fpsr & !0x0F00_0000)
            | if n { 0x0800_0000 } else { 0 }
            | if z { 0x0400_0000 } else { 0 }
            | if i { 0x0200_0000 } else { 0 }
            | if nan { 0x0100_0000 } else { 0 };
    }

    /// Set the FPSR quotient byte (bits 23-16) from FREM / FMOD: bit 23 is the
    /// sign of the quotient, bits 22-16 are its seven least-significant bits.
    pub fn set_fpsr_quotient(&mut self, quotient: u64, sign: bool) {
        let byte = ((u32::from(sign) << 7) | ((quotient as u32) & 0x7F)) << 16;
        self.fpsr = (self.fpsr & !0x00FF_0000) | byte;
    }

    /// Set the BSUN exception: the EXC byte's BSUN bit (FPSR bit 15) plus the
    /// accrued IOP bit (bit 7). Used by FBcc/FScc/FDBcc/FTRAPcc when an
    /// IEEE-nonaware predicate is taken with the NAN condition code set. The
    /// other EXC bits are left untouched (BSUN is OR-ed in, not replaced).
    pub fn set_fpsr_bsun(&mut self) {
        self.fpsr |= 0x0000_8000 | 0x0000_0080;
    }

    /// Apply an operation's exception-status byte to the FPSR. The EXC byte
    /// (bits 15-8: BSUN/SNAN/OPERR/OVFL/UNFL/DZ/INEX2/INEX1) reflects the
    /// most recent operation and is *replaced*; the derived accrued-exception
    /// byte (bits 7-0) *accumulates* (sticky), per the M68881 UM.
    pub fn set_fpsr_exceptions(&mut self, exc: u8) {
        self.fpsr = (self.fpsr & !0x0000_FF00) | (u32::from(exc) << 8);
        self.fpsr |= u32::from(Self::aexc_from_exc(exc));
    }

    /// Derive the accrued-exception byte (AEXC, FPSR bits 7-0) from an
    /// exception-status byte, per M68881 UM Table 6-3. EXC byte bit
    /// positions: BSUN=7 SNAN=6 OPERR=5 OVFL=4 UNFL=3 DZ=2 INEX2=1 INEX1=0.
    #[must_use]
    const fn aexc_from_exc(exc: u8) -> u8 {
        let bsun = exc & 0x80 != 0;
        let snan = exc & 0x40 != 0;
        let operr = exc & 0x20 != 0;
        let ovfl = exc & 0x10 != 0;
        let unfl = exc & 0x08 != 0;
        let dz = exc & 0x04 != 0;
        let inex2 = exc & 0x02 != 0;
        let inex1 = exc & 0x01 != 0;
        let iop = bsun || snan || operr; // AEXC IOP   (bit 7)
        let a_ovfl = ovfl; //                  OVFL  (bit 6)
        let a_unfl = unfl && inex2; //          UNFL  (bit 5)
        let a_dz = dz; //                       DZ    (bit 4)
        let a_inex = inex1 || inex2 || ovfl; // INEX  (bit 3)
        ((iop as u8) << 7)
            | ((a_ovfl as u8) << 6)
            | ((a_unfl as u8) << 5)
            | ((a_dz as u8) << 4)
            | ((a_inex as u8) << 3)
    }

    /// Get address register by index (0-7).
    /// A7 returns the active stack pointer based on supervisor mode.
    #[must_use]
    pub fn a(&self, n: usize) -> u32 {
        debug_assert!(n < 8);
        if n < 7 { self.a[n] } else { self.active_sp() }
    }

    /// Set address register by index (0-7).
    /// A7 sets the active stack pointer based on supervisor mode.
    pub fn set_a(&mut self, n: usize, value: u32) {
        debug_assert!(n < 8);
        if n < 7 {
            self.a[n] = value;
        } else {
            self.set_active_sp(value);
        }
    }

    /// Enable distinct interrupt and master supervisor stack selection.
    ///
    /// MC68020-family wrappers call this after construction and
    /// deserialization. The 68000 and 68010 leave it disabled because SR bit
    /// 12 is reserved on those processors.
    pub fn enable_master_stack(&mut self) {
        self.master_stack_capable = true;
    }

    /// Whether this variant has distinct interrupt and master stacks.
    #[must_use]
    pub const fn master_stack_capable(&self) -> bool {
        self.master_stack_capable
    }

    /// Whether the master stack is the currently selected supervisor stack.
    #[must_use]
    pub const fn master_stack_active(&self) -> bool {
        self.master_stack_capable && self.is_supervisor() && self.sr & 0x1000 != 0
    }

    /// Stack-pointer bank selected by the current S/M state.
    #[must_use]
    pub const fn active_stack_bank(&self) -> StackBank {
        if !self.is_supervisor() {
            StackBank::User
        } else if self.master_stack_active() {
            StackBank::Master
        } else {
            StackBank::Interrupt
        }
    }

    /// Read one stack-pointer bank without changing the active selection.
    #[must_use]
    pub const fn stack_pointer(&self, bank: StackBank) -> u32 {
        match bank {
            StackBank::User => self.usp,
            StackBank::Interrupt => self.ssp,
            StackBank::Master => self.msp,
        }
    }

    /// Write one stack-pointer bank without changing the active selection.
    pub fn set_stack_pointer(&mut self, bank: StackBank, value: u32) {
        match bank {
            StackBank::User => self.usp = value,
            StackBank::Interrupt => self.ssp = value,
            StackBank::Master => self.msp = value,
        }
    }

    /// Get the active stack pointer.
    ///
    /// User mode selects USP. Supervisor mode selects SSP/ISP unless a
    /// dual-supervisor-stack processor has M set, in which case it selects
    /// MSP.
    #[must_use]
    pub const fn active_sp(&self) -> u32 {
        self.stack_pointer(self.active_stack_bank())
    }

    /// Set the active stack pointer.
    pub fn set_active_sp(&mut self, value: u32) {
        self.set_stack_pointer(self.active_stack_bank(), value);
    }

    /// Check if in supervisor mode.
    #[must_use]
    pub const fn is_supervisor(&self) -> bool {
        self.sr & 0x2000 != 0
    }

    pub fn set_supervisor(&mut self, supervisor: bool) {
        if supervisor {
            self.sr |= 0x2000;
        } else {
            self.sr &= !0x2000;
        }
    }

    /// Get the interrupt mask level (0-7).
    #[must_use]
    pub const fn interrupt_mask(&self) -> u8 {
        ((self.sr >> 8) & 0x07) as u8
    }

    /// Set the interrupt mask level (0-7).
    pub fn set_interrupt_mask(&mut self, level: u8) {
        self.sr = (self.sr & !0x0700) | (u16::from(level & 0x07) << 8);
    }

    /// Check if trace mode is enabled.
    #[must_use]
    pub const fn is_trace(&self) -> bool {
        self.sr & 0x8000 != 0
    }

    /// Enter supervisor mode.
    pub fn enter_supervisor(&mut self) {
        if !self.is_supervisor() {
            self.sr |= 0x2000;
        }
    }

    /// Enter user mode.
    pub fn enter_user(&mut self) {
        if self.is_supervisor() {
            self.sr &= !0x2000;
        }
    }

    /// Get the condition code register (low byte of SR).
    #[must_use]
    pub const fn ccr(&self) -> u8 {
        (self.sr & 0xFF) as u8
    }

    /// Set the condition code register (low byte of SR).
    pub fn set_ccr(&mut self, value: u8) {
        self.sr = (self.sr & 0xFF00) | u16::from(value);
    }

    /// Push a word onto the active stack, returning the address written.
    pub fn push_word(&mut self) -> u32 {
        let sp = self.active_sp().wrapping_sub(2);
        self.set_active_sp(sp);
        sp
    }

    /// Push a long onto the active stack, returning the address written.
    pub fn push_long(&mut self) -> u32 {
        let sp = self.active_sp().wrapping_sub(4);
        self.set_active_sp(sp);
        sp
    }

    /// Pop a word from the active stack, returning the NEW SP (after increment).
    pub fn pop_word(&mut self) -> u32 {
        let sp = self.active_sp();
        let new_sp = sp.wrapping_add(2);
        self.set_active_sp(new_sp);
        new_sp
    }

    /// Pop a long from the active stack, returning the NEW SP (after increment).
    pub fn pop_long(&mut self) -> u32 {
        let sp = self.active_sp();
        let new_sp = sp.wrapping_add(4);
        self.set_active_sp(new_sp);
        new_sp
    }
}

#[cfg(test)]
mod tests {
    use super::Registers;

    #[test]
    fn reserved_m_bit_does_not_redirect_a7_without_variant_capability() {
        let mut regs = Registers::new();
        regs.ssp = 0x8000;
        regs.msp = 0x9000;
        regs.sr = 0x3000;

        assert_eq!(regs.active_sp(), 0x8000);
        regs.set_active_sp(0x7FFC);
        assert_eq!(regs.ssp, 0x7FFC);
        assert_eq!(regs.msp, 0x9000);
    }

    #[test]
    fn enabled_master_stack_selects_msp_only_in_supervisor_mode() {
        let mut regs = Registers::new();
        regs.usp = 0x7000;
        regs.ssp = 0x8000;
        regs.msp = 0x9000;
        regs.enable_master_stack();

        regs.sr = 0x1000;
        assert_eq!(regs.active_sp(), 0x7000);

        regs.sr = 0x2000;
        assert_eq!(regs.active_sp(), 0x8000);

        regs.sr = 0x3000;
        assert_eq!(regs.active_sp(), 0x9000);
    }
}
