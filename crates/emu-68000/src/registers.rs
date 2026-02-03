//! Motorola 68000 CPU registers.
//!
//! The 68000 has a rich register set:
//! - D0-D7: 8 data registers (32-bit)
//! - A0-A7: 8 address registers (32-bit, A7 is the active stack pointer)
//! - USP: User stack pointer (A7 when in user mode)
//! - SSP: Supervisor stack pointer (A7 when in supervisor mode)
//! - PC: Program counter (32-bit, but only 24 bits used for addressing)
//! - SR: Status register (16-bit)

/// 68000 CPU register set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registers {
    /// Data registers D0-D7.
    pub d: [u32; 8],
    /// Address registers A0-A6 (A7 is the active stack pointer).
    /// Note: A7 in this array is only used during context switches.
    pub a: [u32; 7],
    /// User stack pointer (active A7 when in user mode).
    pub usp: u32,
    /// Supervisor stack pointer (active A7 when in supervisor mode).
    pub ssp: u32,
    /// Program counter (only lower 24 bits are used).
    pub pc: u32,
    /// Status register.
    pub sr: u16,
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

impl Registers {
    /// Create registers in reset state.
    ///
    /// After reset on the 68000:
    /// - SSP is loaded from address $000000-$000003
    /// - PC is loaded from address $000004-$000007
    /// - Supervisor mode is entered (S bit set)
    /// - Interrupt mask set to level 7
    /// - All other registers are undefined (we use 0)
    #[must_use]
    pub const fn new() -> Self {
        Self {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            pc: 0,
            // Supervisor mode, interrupt level 7
            sr: 0x2700,
        }
    }

    /// Get address register by index (0-7).
    /// A7 returns the active stack pointer based on supervisor mode.
    #[must_use]
    pub fn a(&self, n: usize) -> u32 {
        debug_assert!(n < 8);
        if n < 7 {
            self.a[n]
        } else {
            self.active_sp()
        }
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

    /// Enter supervisor mode, swapping stack pointers if needed.
    pub fn enter_supervisor(&mut self) {
        if !self.is_supervisor() {
            // Swap to supervisor stack
            // USP keeps its value, we just switch to SSP
            self.sr |= 0x2000;
        }
    }

    /// Enter user mode, swapping stack pointers if needed.
    pub fn enter_user(&mut self) {
        if self.is_supervisor() {
            // Swap to user stack
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

    /// Pop a word from the active stack, returning the address to read.
    pub fn pop_word(&mut self) -> u32 {
        let sp = self.active_sp();
        self.set_active_sp(sp.wrapping_add(2));
        sp
    }

    /// Pop a long from the active stack, returning the address to read.
    pub fn pop_long(&mut self) -> u32 {
        let sp = self.active_sp();
        self.set_active_sp(sp.wrapping_add(4));
        sp
    }
}
