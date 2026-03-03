//! Instruction catalogue for 680x0 test generation.
//!
//! Each entry describes how to encode an instruction variant and what
//! memory/register constraints apply. Starting with NOP for Phase 1;
//! subsequent phases add the full 68000 set and 68020 extensions.

use crate::musashi;

/// An instruction variant to generate tests for.
#[derive(Debug, Clone)]
pub struct InstructionDef {
    /// Human-readable name (used in filenames and test names).
    pub name: &'static str,
    /// Base opcode word(s). The generator may randomise EA fields.
    pub opcode: u16,
    /// Number of extension words after the opcode.
    pub ext_words: u8,
    /// How to set up the instruction in memory.
    pub setup: InstructionSetup,
    /// Minimum CPU type required.
    pub min_cpu: u32,
}

/// How the generator should set up memory and registers for this instruction.
#[derive(Debug, Clone, Copy)]
pub enum InstructionSetup {
    /// Fixed opcode, no operands (NOP, RTS, etc.)
    Fixed,
    /// Instruction has EA in bits 5:0 — generator picks valid modes.
    #[allow(dead_code)]
    EaBits0,
    /// Instruction has source EA in bits 5:0 and dest EA in bits 11:6.
    #[allow(dead_code)]
    EaSrcDst,
    /// Custom setup (instruction-specific logic, identified by name).
    #[allow(dead_code)]
    Custom,
}

/// Return all instruction definitions for the given CPU type.
pub fn catalogue(cpu_type: u32) -> Vec<InstructionDef> {
    let mut defs = Vec::new();

    // -- 68000 instructions --
    defs.push(InstructionDef {
        name: "NOP",
        opcode: 0x4E71,
        ext_words: 0,
        setup: InstructionSetup::Fixed,
        min_cpu: musashi::M68K_CPU_TYPE_68000,
    });

    // TODO (Phase 2): Add remaining 68000 instructions
    // TODO (Phase 3): Add 68020-only instructions

    // Filter by CPU type
    let min_order = cpu_type_order(cpu_type);
    defs.retain(|d| cpu_type_order(d.min_cpu) <= min_order);
    defs
}

/// Map CPU type constants to an ordering for filtering.
fn cpu_type_order(cpu_type: u32) -> u32 {
    match cpu_type {
        musashi::M68K_CPU_TYPE_68000 => 0,
        musashi::M68K_CPU_TYPE_68010 => 1,
        musashi::M68K_CPU_TYPE_68EC020 => 2,
        musashi::M68K_CPU_TYPE_68020 => 3,
        _ => 99,
    }
}

/// Find an instruction definition by name.
pub fn find(cpu_type: u32, name: &str) -> Option<InstructionDef> {
    catalogue(cpu_type)
        .into_iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))
}
