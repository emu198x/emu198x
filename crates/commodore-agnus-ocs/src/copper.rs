//! Copper - Coprocessor for synchronized register updates.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Fetch1, // Fetch first word
    Fetch2, // Fetch second word
    Wait,   // Waiting for beam position
}

pub struct Copper {
    pub state: State,
    pub cop1lc: u32,
    pub cop2lc: u32,
    pub pc: u32,
    pub ir1: u16,
    pub ir2: u16,
    pub waiting: bool,
    pub danger: bool, // COPCON bit 1
}

impl Copper {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            cop1lc: 0,
            cop2lc: 0,
            pc: 0,
            ir1: 0,
            ir2: 0,
            waiting: false,
            danger: false,
        }
    }

    pub fn restart_cop1(&mut self) {
        self.pc = self.cop1lc;
        self.state = State::Fetch1;
        self.waiting = false;
    }

    pub fn restart_cop2(&mut self) {
        self.pc = self.cop2lc;
        self.state = State::Fetch1;
        self.waiting = false;
    }

    /// Perform one Copper cycle.
    /// returns Some((reg_offset, value)) if a MOVE instruction completed.
    pub fn tick(
        &mut self,
        vpos: u16,
        hpos: u16,
        read_mem: impl Fn(u32) -> u16,
    ) -> Option<(u16, u16)> {
        match self.state {
            State::Idle => None,
            State::Fetch1 => {
                self.ir1 = read_mem(self.pc);
                self.pc = self.pc.wrapping_add(2);
                self.state = State::Fetch2;
                None
            }
            State::Fetch2 => {
                self.ir2 = read_mem(self.pc);
                self.pc = self.pc.wrapping_add(2);
                self.execute(vpos, hpos)
            }
            State::Wait => {
                if self.check_wait(vpos, hpos) {
                    self.waiting = false;
                    self.state = State::Fetch1;
                }
                None
            }
        }
    }

    fn execute(&mut self, vpos: u16, hpos: u16) -> Option<(u16, u16)> {
        if (self.ir1 & 1) == 0 {
            // MOVE
            let reg = self.ir1 & 0x01FE;
            let val = self.ir2;
            self.state = State::Fetch1;
            Some((reg, val))
        } else {
            // WAIT or SKIP
            let is_skip = (self.ir2 & 1) != 0;
            if is_skip {
                // SKIP: if beam position reached, skip next instruction
                if self.check_wait(vpos, hpos) {
                    self.pc = self.pc.wrapping_add(4);
                }
                self.state = State::Fetch1;
                None
            } else {
                // WAIT
                self.waiting = true;
                if self.check_wait(vpos, hpos) {
                    self.waiting = false;
                    self.state = State::Fetch1;
                } else {
                    self.state = State::Wait;
                }
                None
            }
        }
    }

    fn check_wait(&self, vpos: u16, hpos: u16) -> bool {
        // End-of-list marker ($FFFF,$FFFE): never resolves.
        if self.ir1 == 0xFFFF && self.ir2 == 0xFFFE {
            return false;
        }

        let wait_v = (self.ir1 >> 8) & 0xFF;
        let wait_h = (self.ir1 >> 1) & 0x7F;
        let mask_v = (self.ir2 >> 8) & 0x7F;
        let mask_h = (self.ir2 >> 1) & 0x7F;

        let cur_v = vpos & 0xFF;
        let cur_h = (hpos >> 1) & 0x7F;

        let cmp_cur = ((cur_v & mask_v) << 7) | (cur_h & mask_h);
        let cmp_wait = ((wait_v & mask_v) << 7) | (wait_h & mask_h);
        let result = cmp_cur >= cmp_wait;

        // V7 partial fix: on real hardware V7 (bit 7 of the vertical beam
        // counter) is always compared, even though it has no mask bit.
        // Without this, WAIT VP=$F4 falsely triggers at line $74 because
        // the masked comparison ignores V7. Full V7 emulation requires
        // fixing copper list overrun issues first; for now we only block
        // the false-early case: VP has V7=1 but current vpos has V7=0.
        if result && (wait_v & 0x80 != 0) && (cur_v & 0x80 == 0) {
            return false;
        }

        result
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
    fn skip_advances_pc_when_condition_met() {
        let mut cop = Copper::new();
        // Place a SKIP instruction: wait for vpos >= 0, hpos >= 0 (always true)
        // ir1: VP=0 HP=0 with bit 0 set (second word marker)
        // ir2: mask all, bit 0 = 1 (SKIP)
        //
        // Memory layout at address 0:
        //   $0000: ir1 = $0001 (WAIT/SKIP marker)
        //   $0002: ir2 = $8001 (V mask=$80, H mask=$00, SKIP bit set)
        //   $0004: (next instruction — should be skipped)
        //   $0008: (instruction after skip)
        let mem = |addr: u32| -> u16 {
            match addr {
                0 => 0x0001,  // ir1: vp=0, hp=0, bit0=1
                2 => 0x8001,  // ir2: mask_v=$80, mask_h=$00, skip=1
                _ => 0x0000,
            }
        };

        cop.pc = 0;
        cop.state = State::Fetch1;

        // Fetch1: reads ir1 from addr 0, advances PC to 2
        cop.tick(100, 100, mem);
        assert_eq!(cop.state, State::Fetch2);

        // Fetch2 + execute: reads ir2 from addr 2, advances PC to 4,
        // then SKIP condition is met (vpos=100 >= 0), so PC advances +4 to 8
        cop.tick(100, 100, mem);
        assert_eq!(cop.state, State::Fetch1);
        assert_eq!(cop.pc, 8); // Skipped one instruction (4 bytes)
    }

    #[test]
    fn skip_does_not_advance_when_condition_not_met() {
        let mut cop = Copper::new();
        // SKIP waiting for vpos >= 200 — current beam at line 50, so not met
        let mem = |addr: u32| -> u16 {
            match addr {
                0 => 0xC801, // ir1: vp=$C8 (200), hp=0, bit0=1
                2 => 0xFF01, // ir2: full mask, skip=1
                _ => 0x0000,
            }
        };

        cop.pc = 0;
        cop.state = State::Fetch1;

        cop.tick(50, 0, mem); // Fetch1
        cop.tick(50, 0, mem); // Fetch2 + execute

        assert_eq!(cop.state, State::Fetch1);
        assert_eq!(cop.pc, 4); // No skip — proceeds to next instruction normally
    }
}
