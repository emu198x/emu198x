//! Copper coprocessor.
//!
//! The Copper synchronises register writes to beam positions. It reads
//! instruction pairs from chip RAM via DMA.
//!
//! Instructions:
//! - MOVE (IR1 bit 0 = 0): write IR2 to register (IR1 & $01FE)
//! - WAIT (IR1 bit 0 = 1, IR2 bit 0 = 0): block until beam >= position
//! - SKIP (IR1 bit 0 = 1, IR2 bit 0 = 1): skip next if beam >= position

#![allow(clippy::cast_possible_truncation)]

/// Copper state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    FetchIR1,
    FetchIR2,
    WaitBeam,
}

/// Copper coprocessor.
pub struct Copper {
    state: State,
    pc: u32,
    ir1: u16,
    ir2: u16,
    /// Copper list 1 start address.
    pub cop1lc: u32,
    /// Copper list 2 start address.
    pub cop2lc: u32,
    /// COPCON danger bit: allow writes to registers below $080.
    pub danger: bool,
}

impl Copper {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            pc: 0,
            ir1: 0,
            ir2: 0,
            cop1lc: 0,
            cop2lc: 0,
            danger: false,
        }
    }

    /// Restart copper from COP1LC.
    pub fn restart_cop1(&mut self) {
        self.pc = self.cop1lc;
        self.state = State::FetchIR1;
    }

    /// Restart copper from COP2LC.
    pub fn restart_cop2(&mut self) {
        self.pc = self.cop2lc;
        self.state = State::FetchIR1;
    }

    /// Returns true if the copper needs the DMA bus this cycle.
    #[must_use]
    pub fn needs_bus(&self) -> bool {
        matches!(self.state, State::FetchIR1 | State::FetchIR2)
    }

    /// Returns true if the copper is idle.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.state == State::Idle
    }

    /// Tick the copper with DMA bus access.
    ///
    /// Returns `Some((reg, value))` if a MOVE instruction completed.
    pub fn tick_with_bus(
        &mut self,
        read_word: impl Fn(u32) -> u16,
        vpos: u16,
        hpos: u16,
    ) -> Option<(u16, u16)> {
        match self.state {
            State::FetchIR1 => {
                self.ir1 = read_word(self.pc);
                self.pc = self.pc.wrapping_add(2);
                self.state = State::FetchIR2;
                None
            }
            State::FetchIR2 => {
                self.ir2 = read_word(self.pc);
                self.pc = self.pc.wrapping_add(2);
                self.execute(vpos, hpos)
            }
            State::WaitBeam => {
                self.tick_wait(vpos, hpos);
                None
            }
            State::Idle => None,
        }
    }

    /// Tick the copper without bus access (only WAIT checking).
    pub fn tick_no_bus(&mut self, vpos: u16, hpos: u16) {
        if self.state == State::WaitBeam {
            self.tick_wait(vpos, hpos);
        }
    }

    fn execute(&mut self, vpos: u16, hpos: u16) -> Option<(u16, u16)> {
        if self.ir1 & 1 == 0 {
            // MOVE
            let reg = self.ir1 & 0x01FE;
            let value = self.ir2;
            if reg < 0x080 && !self.danger {
                self.state = State::FetchIR1;
                return None;
            }
            self.state = State::FetchIR1;
            Some((reg, value))
        } else {
            if self.ir1 == 0xFFFF && self.ir2 == 0xFFFE {
                self.state = State::Idle;
                return None;
            }
            self.state = State::WaitBeam;
            self.tick_wait(vpos, hpos);
            None
        }
    }

    #[allow(clippy::similar_names)]
    fn tick_wait(&mut self, vpos: u16, hpos: u16) {
        let cmp_vpos = (self.ir1 >> 8) & 0xFF;
        let cmp_hpos = (self.ir1 >> 1) & 0x7F;
        let mask_v = ((self.ir2 >> 8) & 0x7F) | 0x80;
        let mask_h = (self.ir2 >> 1) & 0x7F;

        let beam_v = vpos & mask_v;
        let beam_h = (hpos >> 1) & mask_h;
        let wait_v = cmp_vpos & mask_v;
        let wait_h = cmp_hpos & mask_h;

        let matched = beam_v > wait_v || (beam_v == wait_v && beam_h >= wait_h);

        if matched {
            if self.ir2 & 1 != 0 {
                // SKIP
                self.pc = self.pc.wrapping_add(4);
            }
            self.state = State::FetchIR1;
        }
    }

    /// Current copper PC (for debugging).
    #[must_use]
    pub fn pc(&self) -> u32 {
        self.pc
    }
}

impl Default for Copper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_after_new() {
        let copper = Copper::new();
        assert!(copper.is_idle());
        assert!(!copper.needs_bus());
    }

    #[test]
    fn restart_begins_fetch() {
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.restart_cop1();
        assert!(!copper.is_idle());
        assert!(copper.needs_bus());
    }

    #[test]
    fn move_instruction() {
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.danger = true;
        copper.restart_cop1();

        let result = copper.tick_with_bus(
            |addr| match addr { 0x1000 => 0x0180, 0x1002 => 0x0F00, _ => 0 },
            0, 0,
        );
        assert_eq!(result, None);

        let result = copper.tick_with_bus(
            |addr| match addr { 0x1000 => 0x0180, 0x1002 => 0x0F00, _ => 0 },
            0, 0,
        );
        assert_eq!(result, Some((0x0180, 0x0F00)));
    }

    #[test]
    fn end_of_list_goes_idle() {
        let mut copper = Copper::new();
        copper.cop1lc = 0x2000;
        copper.restart_cop1();

        let _ = copper.tick_with_bus(|_| 0xFFFF, 0, 0);
        let result = copper.tick_with_bus(|_| 0xFFFE, 0, 0);
        assert_eq!(result, None);
        assert!(copper.is_idle());
    }

    #[test]
    fn danger_bit_blocks_low_registers() {
        let mut copper = Copper::new();
        copper.cop1lc = 0x1000;
        copper.danger = false;
        copper.restart_cop1();

        let _ = copper.tick_with_bus(
            |addr| if addr == 0x1000 { 0x0040 } else { 0x1234 },
            0, 0,
        );
        let result = copper.tick_with_bus(
            |addr| if addr == 0x1000 { 0x0040 } else { 0x1234 },
            0, 0,
        );
        assert_eq!(result, None);
    }
}
