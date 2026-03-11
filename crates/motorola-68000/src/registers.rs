//! Motorola 68000 CPU registers.
//!
//! - D0-D7: 8 data registers (32-bit)
//! - A0-A7: 8 address registers (32-bit, A7 is the active stack pointer)
//! - USP: User stack pointer (A7 when in user mode)
//! - SSP: Supervisor stack pointer (A7 when in supervisor mode)
//! - PC: Program counter (32-bit, 24-bit on 68000)
//! - SR: Status register (16-bit)

/// FPU register value — wraps f64 with bit-exact Eq for emulation.
#[derive(Debug, Clone, Copy)]
pub struct FpReg(pub f64);

impl FpReg {
    pub const ZERO: Self = Self(0.0);
}

impl PartialEq for FpReg {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}
impl Eq for FpReg {}

/// 68000 CPU register set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registers {
    /// Data registers D0-D7.
    pub d: [u32; 8],
    /// Address registers A0-A6 (A7 is handled via USP/SSP).
    pub a: [u32; 7],
    /// User stack pointer (active A7 when in user mode).
    pub usp: u32,
    /// Supervisor stack pointer (active A7 when in supervisor mode).
    /// On 68020+, this serves as the Interrupt Stack Pointer (ISP).
    pub ssp: u32,
    /// Master Stack Pointer (68020+, selected when SR M-flag is set).
    pub msp: u32,
    /// Cache Address Register (68020+).
    pub caar: u32,
    /// Program counter.
    pub pc: u32,
    /// Status register.
    pub sr: u16,
    /// Vector Base Register (68010+).
    pub vbr: u32,
    /// Source Function Code register (68010+, 3 bits).
    pub sfc: u8,
    /// Destination Function Code register (68010+, 3 bits).
    pub dfc: u8,
    /// Cache Control Register (68020+).
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
    /// Supervisor Root Pointer (68030: 64-bit, stored low 32; 68040+: 32-bit).
    pub srp: u32,
    /// User Root Pointer (68030: via CRP, stored low 32; 68040+: 32-bit).
    pub urp: u32,
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
            urp: 0,
            mmusr: 0,
            buscr: 0,
            pcr: 0,
            fp: [FpReg::ZERO; 8],
            fpcr: 0,
            fpsr: 0,
            fpiar: 0,
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

    /// Get the active stack pointer (USP or SSP based on supervisor mode).
    #[must_use]
    pub const fn active_sp(&self) -> u32 {
        if self.is_supervisor() {
            self.ssp
        } else {
            self.usp
        }
    }

    /// Set the active stack pointer.
    pub fn set_active_sp(&mut self, value: u32) {
        if self.is_supervisor() {
            self.ssp = value;
        } else {
            self.usp = value;
        }
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
