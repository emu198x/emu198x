//! Observable implementation for the 68000 CPU.

use emu_core::{Observable, Value};

use super::Cpu68000;
use super::State;
use crate::common::flags::{C, N, V, X, Z};

/// Query paths supported by the 68000.
const M68000_QUERY_PATHS: &[&str] = &[
    "d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7",
    "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7",
    "usp", "ssp",
    "pc",
    "sr", "ccr",
    "flags.x", "flags.n", "flags.z", "flags.v", "flags.c",
    "flags.s", "flags.t",
    "int_mask",
    "halted", "stopped", "cycles",
    "opcode",
];

impl Observable for Cpu68000 {
    fn query(&self, path: &str) -> Option<Value> {
        match path {
            "d0" => Some(self.regs.d[0].into()),
            "d1" => Some(self.regs.d[1].into()),
            "d2" => Some(self.regs.d[2].into()),
            "d3" => Some(self.regs.d[3].into()),
            "d4" => Some(self.regs.d[4].into()),
            "d5" => Some(self.regs.d[5].into()),
            "d6" => Some(self.regs.d[6].into()),
            "d7" => Some(self.regs.d[7].into()),
            "a0" => Some(self.regs.a(0).into()),
            "a1" => Some(self.regs.a(1).into()),
            "a2" => Some(self.regs.a(2).into()),
            "a3" => Some(self.regs.a(3).into()),
            "a4" => Some(self.regs.a(4).into()),
            "a5" => Some(self.regs.a(5).into()),
            "a6" => Some(self.regs.a(6).into()),
            "a7" => Some(self.regs.a(7).into()),
            "usp" => Some(self.regs.usp.into()),
            "ssp" => Some(self.regs.ssp.into()),
            "pc" => Some(self.regs.pc.into()),
            "sr" => Some(Value::U16(self.regs.sr)),
            "ccr" => Some(self.regs.ccr().into()),
            "flags.x" => Some((self.regs.sr & X != 0).into()),
            "flags.n" => Some((self.regs.sr & N != 0).into()),
            "flags.z" => Some((self.regs.sr & Z != 0).into()),
            "flags.v" => Some((self.regs.sr & V != 0).into()),
            "flags.c" => Some((self.regs.sr & C != 0).into()),
            "flags.s" => Some(self.regs.is_supervisor().into()),
            "flags.t" => Some(self.regs.is_trace().into()),
            "int_mask" => Some(self.regs.interrupt_mask().into()),
            "halted" => Some(matches!(self.state, State::Halted).into()),
            "stopped" => Some(matches!(self.state, State::Stopped).into()),
            "cycles" => Some(self.total_cycles.get().into()),
            "opcode" => Some(Value::U16(self.opcode)),
            _ => None,
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        M68000_QUERY_PATHS
    }
}
