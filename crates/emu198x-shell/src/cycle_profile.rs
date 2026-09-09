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

/// Machine-collected costs before source mapping.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleCounts {
    /// Full capture duration, including every bucket below.
    pub ticks: u64,
    /// Costs keyed by the first opcode/prefix address, never the post-step PC.
    pub addresses: BTreeMap<u32, ExecutionCost>,
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
    /// Join a flat-address capture to the loaded build's source map. The caller
    /// must not use a final paging state to annotate a capture that changed banks.
    #[must_use]
    pub fn with_symbols(self, clock: ClockDesc, symbols: Option<&DebugSymbols>) -> CycleProfile {
        let mut lines = BTreeMap::<(String, u32), ExecutionCost>::new();
        let mut unmapped_ticks = 0;
        let addresses = self
            .addresses
            .iter()
            .map(|(&address, cost)| {
                let source = symbols.and_then(|symbols| symbols.line_at(address));
                if let Some(source) = &source {
                    let total = lines.entry((source.file.clone(), source.line)).or_default();
                    total.ticks += cost.ticks;
                    total.executions += cost.executions;
                } else {
                    unmapped_ticks += cost.ticks;
                }
                ProfileAddress {
                    address,
                    cost: cost.clone(),
                    symbol: symbols
                        .and_then(|symbols| symbols.symbol_at(address))
                        .map(str::to_owned),
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
