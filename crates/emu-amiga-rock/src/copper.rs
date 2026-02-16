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
    pub fn tick(&mut self, vpos: u16, hpos: u16, read_mem: impl Fn(u32) -> u16) -> Option<(u16, u16)> {
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
                // SKIP stub
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
        let wait_v = (self.ir1 >> 8) & 0xFF;
        let wait_h = (self.ir1 >> 1) & 0x7F;

        let cur_v = vpos & 0xFF;
        let cur_h = (hpos >> 1) & 0x7F; // CCKs to "Copper HPOS" (7 bits)

        // 68000 Reference Manual: WAIT finishes when (cur_v, cur_h) >= (wait_v, wait_h)
        if cur_v > wait_v {
            true
        } else if cur_v == wait_v {
            cur_h >= wait_h
        } else {
            false
        }
    }
}
