//! Test case data structures for 680x0 single-step test vectors.
//!
//! Serialised to MessagePack for compact, self-describing storage.
//! The `#[serde(default)]` on 68020+ fields means they're omitted
//! from 68000 test files, keeping them small.

use serde::{Deserialize, Serialize};

/// A single test vector: run one instruction, compare before/after.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// Human-readable description (e.g. "NOP sr=2700").
    pub name: String,
    /// CPU + memory state before execution.
    pub initial: CpuState,
    /// Expected CPU + memory state after execution.
    pub final_state: CpuState,
    /// Musashi cycle count for the instruction.
    pub cycles: u32,
}

/// Snapshot of CPU registers and relevant memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuState {
    /// Data registers D0-D7.
    pub d: [u32; 8],
    /// Address registers A0-A6.
    pub a: [u32; 7],
    /// User Stack Pointer.
    pub usp: u32,
    /// Supervisor Stack Pointer (68000) or ISP (68020+).
    pub ssp: u32,
    /// Status Register.
    pub sr: u16,
    /// Program Counter.
    pub pc: u32,
    /// Prefetch pipeline: [IR, IRC].
    pub prefetch: [u16; 2],
    /// Memory state as (address, byte) pairs.
    pub ram: Vec<(u32, u8)>,

    // --- 68020+ registers (default to 0 / absent for 68000) ---
    /// Master Stack Pointer (68020+).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub msp: u32,
    /// Vector Base Register (68010+).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub vbr: u32,
    /// Cache Control Register (68020+).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub cacr: u32,
    /// Cache Address Register (68020+).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub caar: u32,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// Container for a batch of tests, serialised as the top-level MessagePack object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFile {
    /// CPU model identifier (e.g. "68000", "68020").
    pub cpu: String,
    /// Instruction mnemonic (e.g. "NOP", "MOVE.w").
    pub instruction: String,
    /// Test vectors.
    pub tests: Vec<TestCase>,
}
