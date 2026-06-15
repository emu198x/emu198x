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

    // --- 68881/2 FPU registers (default to 0 / absent for non-FP tests) ---
    /// FP data registers FP0-FP7 as 80-bit extended `(high16, low64)` pairs.
    #[serde(default = "default_fp", skip_serializing_if = "is_zero_fp")]
    pub fp: [(u16, u64); 8],
    /// FP Control Register (rounding mode/precision + exception enables).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub fpcr: u32,
    /// FP Status Register (condition codes + exception/accrued bits).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub fpsr: u32,
    /// FP Instruction Address Register.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub fpiar: u32,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

fn default_fp() -> [(u16, u64); 8] {
    [(0, 0); 8]
}

fn is_zero_fp(v: &[(u16, u64); 8]) -> bool {
    v.iter().all(|&(high, low)| high == 0 && low == 0)
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

#[cfg(test)]
mod tests {
    use super::{CpuState, TestCase, TestFile, default_fp};

    #[test]
    fn test_file_round_trips_through_messagepack() {
        let file = TestFile {
            cpu: String::from("68020"),
            instruction: String::from("MOVE.l"),
            tests: vec![TestCase {
                name: String::from("MOVE.l #0"),
                initial: CpuState {
                    d: [1, 2, 3, 4, 5, 6, 7, 8],
                    a: [9, 10, 11, 12, 13, 14, 15],
                    usp: 0x1000,
                    ssp: 0x2000,
                    sr: 0x2700,
                    pc: 0x1234,
                    prefetch: [0x4E71, 0x4E75],
                    ram: vec![(0x1000, 0x12), (0x1001, 0x34)],
                    msp: 0x3000,
                    vbr: 0x4000,
                    cacr: 0x5000,
                    caar: 0x6000,
                    fp: [(0x4000, 0xC000_0000_0000_0000); 8],
                    fpcr: 0x0030,
                    fpsr: 0x0800_0000,
                    fpiar: 0x1234,
                },
                final_state: CpuState {
                    d: [8, 7, 6, 5, 4, 3, 2, 1],
                    a: [15, 14, 13, 12, 11, 10, 9],
                    usp: 0x1004,
                    ssp: 0x2004,
                    sr: 0x2004,
                    pc: 0x1238,
                    prefetch: [0x4E75, 0x4E71],
                    ram: vec![(0x1000, 0x56), (0x1001, 0x78)],
                    msp: 0x3004,
                    vbr: 0x4004,
                    cacr: 0x5004,
                    caar: 0x6004,
                    fp: [(0x3FFF, 0x8000_0000_0000_0000); 8],
                    fpcr: 0x0030,
                    fpsr: 0x0400_0000,
                    fpiar: 0x1238,
                },
                cycles: 12,
            }],
        };

        let encoded = rmp_serde::to_vec(&file).expect("serialisation should work");
        let decoded: TestFile =
            rmp_serde::from_slice(&encoded).expect("deserialisation should work");

        assert_eq!(decoded.cpu, file.cpu);
        assert_eq!(decoded.instruction, file.instruction);
        assert_eq!(decoded.tests.len(), 1);
        assert_eq!(decoded.tests[0].name, file.tests[0].name);
        assert_eq!(
            decoded.tests[0].initial.prefetch,
            file.tests[0].initial.prefetch
        );
        assert_eq!(decoded.tests[0].initial.ram, file.tests[0].initial.ram);
        assert_eq!(
            decoded.tests[0].final_state.prefetch,
            file.tests[0].final_state.prefetch
        );
        assert_eq!(
            decoded.tests[0].final_state.ram,
            file.tests[0].final_state.ram
        );
        assert_eq!(
            decoded.tests[0].initial.fp, file.tests[0].initial.fp,
            "FP registers should round-trip"
        );
        assert_eq!(decoded.tests[0].initial.fpcr, 0x0030);
        assert_eq!(decoded.tests[0].final_state.fpsr, 0x0400_0000);
        assert_eq!(decoded.tests[0].final_state.fpiar, 0x1238);
        assert_eq!(decoded.tests[0].cycles, file.tests[0].cycles);
    }

    #[test]
    fn zero_extended_registers_round_trip_as_zero() {
        let state = CpuState {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            sr: 0x2000,
            pc: 0x1000,
            prefetch: [0x4E71, 0x4E71],
            ram: vec![(0x1000, 0x4E), (0x1001, 0x71)],
            msp: 0,
            vbr: 0,
            cacr: 0,
            caar: 0,
            fp: default_fp(),
            fpcr: 0,
            fpsr: 0,
            fpiar: 0,
        };

        let encoded = rmp_serde::to_vec(&state).expect("serialisation should work");
        let decoded: CpuState =
            rmp_serde::from_slice(&encoded).expect("deserialisation should work");

        assert_eq!(decoded.msp, 0);
        assert_eq!(decoded.vbr, 0);
        assert_eq!(decoded.cacr, 0);
        assert_eq!(decoded.caar, 0);
        assert_eq!(decoded.fp, [(0, 0); 8]);
        assert_eq!(decoded.fpcr, 0);
        assert_eq!(decoded.fpsr, 0);
        assert_eq!(decoded.fpiar, 0);
        assert_eq!(decoded.prefetch, state.prefetch);
        assert_eq!(decoded.ram, state.ram);
    }

    /// Regression guard for the interior-hole bug: an FP test commonly has
    /// `fpcr == 0` (skipped) while `fpsr != 0` (condition codes set), and
    /// `msp == 0` (skipped) while `fp != 0`. With a positional array
    /// encoding the skipped interior fields shift every later field and
    /// corrupt the load. The production path serialises named maps
    /// (`to_vec_named`), which keys by field name, so any combination of
    /// omitted fields round-trips. This test pins that contract.
    #[test]
    fn named_map_encoding_survives_interior_skipped_fields() {
        let state = CpuState {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            sr: 0x2000,
            pc: 0x1000,
            prefetch: [0xF200, 0x4E71],
            ram: vec![(0x1000, 0xF2), (0x1001, 0x00)],
            msp: 0,                                   // skipped
            vbr: 0,                                   // skipped
            cacr: 0,                                  // skipped
            caar: 0,                                  // skipped
            fp: [(0x4000, 0xC000_0000_0000_0000); 8], // present
            fpcr: 0,                                  // skipped (interior)
            fpsr: 0x0800_0000,                        // present — N bit set
            fpiar: 0x0000_1004,                       // present
        };

        let encoded = rmp_serde::to_vec_named(&state).expect("serialisation should work");
        let decoded: CpuState =
            rmp_serde::from_slice(&encoded).expect("deserialisation should work");

        assert_eq!(decoded.fp, state.fp, "fp must survive the interior hole");
        assert_eq!(decoded.fpcr, 0);
        assert_eq!(
            decoded.fpsr, 0x0800_0000,
            "fpsr must not absorb fpcr's slot"
        );
        assert_eq!(decoded.fpiar, 0x0000_1004);
        assert_eq!(decoded.msp, 0);
        assert_eq!(decoded.sr, 0x2000);
        assert_eq!(decoded.ram, state.ram);
    }
}
