//! Bounded execution-cost reports. Costs use the machine's authoritative clock,
//! including contention; they are not static instruction timings or host time.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    ClockDesc,
    debug_info::{DebugSymbols, SourceLine},
};

/// Maximum authoritative ticks allowed in a single profile command.
pub const MAX_PROFILE_TICKS: u32 = 14_000_000;

/// Validate the resource bound before any machine or input state changes.
///
/// # Errors
/// Returns an invalid-request error outside the inclusive supported range.
pub fn validate_ticks(ticks: u32) -> Result<(), crate::MachineError> {
    if ticks == 0 || ticks > MAX_PROFILE_TICKS {
        return Err(crate::MachineError::InvalidRequest {
            reason: format!("profile_cycles ticks must be between 1 and {MAX_PROFILE_TICKS}"),
        });
    }
    Ok(())
}

/// Accumulated cost for fully observed executions at one instruction address.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionCost {
    /// Number of completed executions (block-repeat iterations count separately).
    pub executions: u64,
    /// Elapsed authoritative ticks, including stalled CPU clock slots.
    pub ticks: u64,
}

/// Physical memory supplying an instruction's first byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileMemory {
    /// RAM pages use the Debug198x page namespace.
    Ram,
    /// ROM pages are kept separate and currently have no source join.
    Rom,
    /// A ROM supplied by an active overlay, separate from the base ROM map.
    RomOverlay,
}

/// Historical slot mapping at the first opcode fetch, not capture end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProfileMapping {
    /// Distinguishes RAM and ROM page namespaces.
    pub memory: ProfileMemory,
    /// Hardware slot containing the CPU address.
    pub slot: u8,
    /// Physical page selected in that slot.
    pub page: u16,
    /// CPU address at which the slot starts.
    pub base: u32,
}

/// Per-bank decomposition of an address total.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappedExecutionCost {
    /// CPU address of the first opcode/prefix byte.
    pub address: u32,
    /// Mapping supplying the first opcode/prefix byte.
    pub mapping: ProfileMapping,
    /// Completed executions in this mapping.
    #[serde(flatten)]
    pub cost: ExecutionCost,
}

/// Machine-collected costs before source mapping.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleCounts {
    /// Full capture duration, including every bucket below.
    pub ticks: u64,
    /// Costs keyed by the first opcode/prefix address, never the post-step PC.
    pub addresses: BTreeMap<u32, ExecutionCost>,
    /// Optional complete decomposition of `addresses` by physical mapping.
    /// These ticks are already included in `addresses`, never additional time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mapped_addresses: Vec<MappedExecutionCost>,
    /// Time spent accepting interrupts, excluding handler instructions.
    pub interrupt_ticks: u64,
    /// Time spent in completed HALT refresh intervals, excluding HALT itself.
    pub halt_ticks: u64,
    /// Remaining time of an execution already underway when capture began.
    pub leading_partial_ticks: u64,
    /// Time in an unfinished execution when the exact capture budget expired.
    pub trailing_partial_ticks: u64,
}

/// An address and its source annotation. An absent source is retained explicitly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileAddress {
    /// Instruction address in the captured address space.
    pub address: u32,
    /// Historical mapping, absent for flat captures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mapping: Option<ProfileMapping>,
    /// Cost for this instruction address.
    #[serde(flatten)]
    pub cost: ExecutionCost,
    /// Exact label at the instruction address, if known; not a routine extent.
    pub symbol: Option<String>,
    /// Source line producing the first opcode/prefix byte, if known.
    pub source: Option<SourceLine>,
}

/// Costs aggregated across all instruction addresses belonging to a source line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileLine {
    /// Assembler-recorded source location.
    #[serde(flatten)]
    pub source: SourceLine,
    /// Sum of the mapped address costs.
    #[serde(flatten)]
    pub cost: ExecutionCost,
}

/// Source-level profile of one explicitly bounded execution window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleProfile {
    /// Authoritative tick unit and rate, suitable for conversion to elapsed time.
    pub clock: ClockDesc,
    /// Raw accounting, including unmapped code and non-instruction intervals.
    pub counts: CycleCounts,
    /// Source annotations for every recorded instruction address.
    pub addresses: Vec<ProfileAddress>,
    /// Source totals; excludes unmapped instructions and non-instruction buckets.
    pub lines: Vec<ProfileLine>,
    /// Completed instruction ticks without a matching source line.
    pub unmapped_ticks: u64,
}

impl CycleCounts {
    /// Join captured identities to source records, independently of final paging.
    #[must_use]
    pub fn with_symbols(self, clock: ClockDesc, symbols: Option<&DebugSymbols>) -> CycleProfile {
        let mut lines = BTreeMap::<(String, u32), ExecutionCost>::new();
        let mut unmapped_ticks = 0;
        let entries: Vec<_> = if self.mapped_addresses.is_empty() {
            self.addresses
                .iter()
                .map(|(&address, cost)| (address, None, cost))
                .collect()
        } else {
            self.mapped_addresses
                .iter()
                .map(|entry| (entry.address, Some(entry.mapping), &entry.cost))
                .collect()
        };
        let addresses = entries
            .into_iter()
            .map(|(address, mapping, cost)| {
                let (symbol, source) = match (symbols, mapping) {
                    (Some(symbols), None) => (symbols.symbol_at(address), symbols.line_at(address)),
                    (Some(symbols), Some(mapping)) if mapping.memory == ProfileMemory::Ram => {
                        address
                            .checked_sub(mapping.base)
                            .map(|offset| {
                                symbols.annotation_in_page(mapping.page, u64::from(offset))
                            })
                            .unwrap_or((None, None))
                    }
                    _ => (None, None),
                };
                if let Some(source) = &source {
                    let total = lines.entry((source.file.clone(), source.line)).or_default();
                    total.ticks += cost.ticks;
                    total.executions += cost.executions;
                } else {
                    unmapped_ticks += cost.ticks;
                }
                ProfileAddress {
                    address,
                    mapping,
                    cost: cost.clone(),
                    symbol: symbol.map(str::to_owned),
                    source,
                }
            })
            .collect();
        CycleProfile {
            clock,
            counts: self,
            addresses,
            lines: lines
                .into_iter()
                .map(|((file, line), cost)| ProfileLine {
                    source: SourceLine { file, line },
                    cost,
                })
                .collect(),
            unmapped_ticks,
        }
    }
}
