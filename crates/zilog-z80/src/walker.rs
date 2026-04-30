use crate::mcycle::{self, MStep};
use crate::registers::Registers;
use crate::z80::{IntAckPhase, InternalPhase, IoPhase, MemPhase, Phase};

/// Staged data accumulated across MSteps within an instruction.
/// Execute steps consume this data to apply the operation.
///
/// Public so the FUSE-corpus integration test (`tests/z80_fuse.rs`) can
/// inspect mid-instruction state to align bus events with T-states.
/// Treat as crate-internal — production consumers should use `Z80`'s
/// public pin signals instead.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Staged {
    /// Low byte fetched from memory or immediate.
    pub data_lo: u8,
    /// High byte fetched from memory or immediate.
    pub data_hi: u8,
    /// 16-bit address staged for memory operations.
    pub addr: u16,
    /// Value to write to memory or I/O.
    pub write_val: u8,
    /// High byte value to write (for 16-bit stores).
    pub write_hi: u8,
    /// 16-bit value to push to stack.
    pub push_val: u16,
    /// Displacement byte for indexed addressing.
    pub disp: i8,
}

/// Prefix state for multi-byte opcodes.
///
/// Public for the FUSE-corpus integration test; treat as crate-internal.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[allow(clippy::upper_case_acronyms)]
pub enum Prefix {
    None,
    CB,
    ED,
    DD,   // IX
    FD,   // IY
    DDCB, // IX+d bit ops
    FDCB, // IY+d bit ops
}

/// Walker state — tracks progress through an instruction's MStep sequence.
///
/// Public for the FUSE-corpus integration test; treat as crate-internal.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Walker {
    /// The current MStep sequence being executed.
    ///
    /// `#[serde(skip)]` because it's a `&'static` reference — static
    /// data, not serialisable. On deserialise it defaults to `SEQ_NOP`,
    /// which matches the idle "just finished an instruction" state.
    /// Save states must be taken at instruction boundaries
    /// (`instruction_complete == true`) for this to be correct.
    #[serde(skip, default = "default_sequence")]
    pub sequence: &'static [MStep],
    /// Index into the sequence (0 = first step after M1 decode).
    pub step_idx: usize,
    /// Staged data accumulated across steps.
    pub staged: Staged,
    /// The opcode byte (or sub-opcode for CB/ED prefix instructions).
    pub opcode: u8,
    /// Current prefix state.
    pub prefix: Prefix,
    /// True when a conditional instruction was not taken (truncate sequence).
    pub done: bool,
    /// True when the instruction is fully complete and we should fetch next.
    pub instruction_complete: bool,
    /// True when we're in the DDCB/FDCB fetch phase (FetchDisp + FetchByte).
    pub ddcb_fetch_phase: bool,
}

fn default_sequence() -> &'static [MStep] {
    mcycle::SEQ_NOP
}

impl Default for Walker {
    fn default() -> Self {
        Self {
            sequence: mcycle::SEQ_NOP,
            step_idx: 0,
            staged: Staged::default(),
            opcode: 0,
            prefix: Prefix::None,
            done: false,
            instruction_complete: true,
            ddcb_fetch_phase: false,
        }
    }
}

impl Walker {
    /// Begin processing the current step in the sequence.
    /// Returns the Phase to enter, or None if the step is Execute (0 HC).
    pub fn begin_current_step(&self) -> Option<Phase> {
        if self.step_idx >= self.sequence.len() {
            return None;
        }

        let step = self.sequence[self.step_idx];
        match step {
            MStep::FetchByte
            | MStep::FetchByteHi
            | MStep::FetchDisp
            | MStep::ReadAddr
            | MStep::ReadAddrHi
            | MStep::PopLo
            | MStep::PopHi => Some(Phase::MemRead(MemPhase::T1Rise)),
            MStep::ContendPc => Some(Phase::Contend(MemPhase::T1Rise)),
            MStep::WriteAddr | MStep::WriteAddrHi | MStep::PushHi | MStep::PushLo => {
                Some(Phase::MemWrite(MemPhase::T1Rise))
            }
            MStep::IoRead => Some(Phase::IoRead(IoPhase::T1Rise)),
            MStep::IoWrite => Some(Phase::IoWrite(IoPhase::T1Rise)),
            MStep::Internal(n) => Some(Phase::Internal(InternalPhase { remaining: n * 2 })),
            MStep::IntAck => Some(Phase::IntAck(IntAckPhase::T1Rise)),
            MStep::Execute => None, // 0 half-cycles
        }
    }

    /// Set up Z80 address/data bus signals for the current step.
    /// Takes individual fields to avoid borrow-checker conflicts.
    pub fn setup_signals(&self, addr: &mut u16, data: &mut u8, regs: &mut Registers) {
        if self.step_idx >= self.sequence.len() {
            return;
        }

        let step = self.sequence[self.step_idx];
        match step {
            MStep::FetchByte | MStep::FetchByteHi | MStep::FetchDisp => {
                *addr = regs.pc;
            }
            MStep::ReadAddr => {
                *addr = self.staged.addr;
            }
            MStep::ReadAddrHi => {
                *addr = self.staged.addr.wrapping_add(1);
            }
            MStep::WriteAddr => {
                *addr = self.staged.addr;
                *data = self.staged.write_val;
            }
            MStep::WriteAddrHi => {
                *addr = self.staged.addr.wrapping_add(1);
                *data = self.staged.write_hi;
            }
            MStep::PushHi => {
                regs.sp = regs.sp.wrapping_sub(1);
                *addr = regs.sp;
                *data = (self.staged.push_val >> 8) as u8;
            }
            MStep::PushLo => {
                regs.sp = regs.sp.wrapping_sub(1);
                *addr = regs.sp;
                *data = self.staged.push_val as u8;
            }
            MStep::PopLo | MStep::PopHi => {
                *addr = regs.sp;
            }
            MStep::ContendPc => {
                *addr = regs.pc;
            }
            MStep::IoRead => {
                *addr = self.staged.addr;
            }
            MStep::IoWrite => {
                *addr = self.staged.addr;
                *data = self.staged.write_val;
            }
            MStep::Internal(_) => {
                *addr = regs.ir();
            }
            MStep::IntAck => {
                *addr = regs.pc;
            }
            MStep::Execute => {}
        }
    }

    /// Called when a read step completes. Stores the byte in staged data.
    pub fn latch_read(&mut self, data_in: u8, regs: &mut Registers) {
        if self.step_idx >= self.sequence.len() {
            return;
        }

        let step = self.sequence[self.step_idx];
        match step {
            MStep::FetchByte => {
                self.staged.data_lo = data_in;
                regs.pc = regs.pc.wrapping_add(1);
            }
            MStep::FetchByteHi => {
                self.staged.data_hi = data_in;
                regs.pc = regs.pc.wrapping_add(1);
            }
            MStep::FetchDisp => {
                self.staged.disp = data_in as i8;
                regs.pc = regs.pc.wrapping_add(1);
            }
            MStep::ReadAddr => {
                self.staged.data_lo = data_in;
            }
            MStep::ReadAddrHi => {
                self.staged.data_hi = data_in;
            }
            MStep::PopLo => {
                self.staged.data_lo = data_in;
                regs.sp = regs.sp.wrapping_add(1);
            }
            MStep::PopHi => {
                self.staged.data_hi = data_in;
                regs.sp = regs.sp.wrapping_add(1);
            }
            MStep::IoRead => {
                self.staged.data_lo = data_in;
            }
            _ => {}
        }
    }

    /// Advance to the next step. Returns true if instruction complete.
    pub fn advance(&mut self) -> bool {
        self.step_idx += 1;
        if self.done || self.step_idx >= self.sequence.len() {
            self.instruction_complete = true;
            true
        } else {
            false
        }
    }

    /// Reset for a new instruction.
    pub fn begin_instruction(&mut self) {
        self.step_idx = 0;
        self.staged = Staged::default();
        self.done = false;
        self.instruction_complete = false;
        self.ddcb_fetch_phase = false;
        // Q register: reset to 0 at instruction start.
        // Flag-modifying instructions set Q = F in their execute handler.
        // Non-flag-modifying instructions leave Q = 0.
        // This is stored in registers, not walker, but we need access
        // to registers here... so this is handled in Z80::begin_next_instruction.
    }

    /// Get the current MStep, if any.
    pub fn current_step(&self) -> Option<MStep> {
        self.sequence.get(self.step_idx).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_walker_is_at_idle_boundary() {
        let w = Walker::default();
        assert_eq!(w.prefix, Prefix::None);
        assert!(w.instruction_complete);
        assert!(!w.ddcb_fetch_phase);
        assert_eq!(w.step_idx, 0);
        // The default sequence is SEQ_NOP — one Execute step.
        assert!(matches!(w.current_step(), Some(MStep::Execute)));
    }

    #[test]
    fn begin_instruction_clears_completion_flags_but_keeps_sequence() {
        let mut w = Walker {
            sequence: mcycle::SEQ_LD_R_N,
            step_idx: 7,
            done: true,
            instruction_complete: true,
            ddcb_fetch_phase: true,
            staged: Staged {
                disp: -3,
                ..Staged::default()
            },
            ..Walker::default()
        };

        w.begin_instruction();
        assert_eq!(w.step_idx, 0);
        assert!(!w.done);
        assert!(!w.instruction_complete);
        assert!(!w.ddcb_fetch_phase);
        assert_eq!(w.staged.disp, 0);
        // Sequence is owned by the caller (Z80 sets it after decode);
        // begin_instruction must not touch it.
        assert!(std::ptr::eq(w.sequence.as_ptr(), mcycle::SEQ_LD_R_N.as_ptr()));
    }

    #[test]
    fn advance_terminates_on_done_short_circuit() {
        // Conditional instructions set `done` to skip the remaining steps;
        // advance must report completion immediately even if more steps exist.
        let mut w = Walker {
            sequence: mcycle::SEQ_JP_CC_NN,
            done: true,
            instruction_complete: false,
            ..Walker::default()
        };
        assert!(w.advance());
        assert!(w.instruction_complete);
    }

    #[test]
    fn current_step_returns_none_past_sequence_end() {
        let w = Walker {
            sequence: mcycle::SEQ_NOP,
            step_idx: mcycle::SEQ_NOP.len(),
            ..Walker::default()
        };
        assert!(w.current_step().is_none());
    }
}
