//! Exclusive costs for explicitly declared routine extents. Definitions are
//! validated before execution; labels alone cannot supply reliable boundaries.

use crate::{
    MachineError,
    cycle_profile::{CycleProfile, ProfileMapping, ProfileMemory},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum routines in a capture request.
pub const MAX_ROUTINES: usize = 128;
/// Maximum ranges across all routines in a capture request.
pub const MAX_ROUTINE_RANGES: usize = 512;

/// Coordinate system for a routine's byte range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoutineSpace {
    /// CPU addresses in a flat capture; never matches a banked address.
    #[default]
    Flat,
    /// Offsets in a physical page, independent of its current CPU slot.
    Page {
        /// RAM, base ROM, overlay ROM or unbacked window namespace.
        memory: ProfileMemory,
        /// Page identity from the captured mapping.
        page: u16,
    },
}

/// Half-open byte interval containing instruction starts owned by a routine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutineRange {
    /// Inclusive first byte, in the selected coordinate system.
    pub start: u32,
    /// Exclusive end; 2^32 permits a range through the final u32 address.
    pub end: u64,
    /// Defaults to flat CPU addresses. Banked captures require an explicit page.
    #[serde(default)]
    pub space: RoutineSpace,
}

/// Caller-supplied routine boundaries, including optional discontiguous pieces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutineDefinition {
    /// Unique nonblank name, at most 128 characters.
    pub name: String,
    /// Nonempty, nonoverlapping intervals owned by this routine.
    pub ranges: Vec<RoutineRange>,
}

/// Measured exclusive instruction costs within the declared extents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineCost {
    /// Name from the requested definition.
    pub name: String,
    /// Exact ranges used for attribution.
    pub ranges: Vec<RoutineRange>,
    /// Completed instructions, including loop iterations; not invocation count.
    pub instructions: u64,
    /// Authoritative ticks in this routine's own instructions, including stalls.
    /// Callees outside its ranges and non-instruction buckets are excluded.
    pub exclusive_ticks: u64,
    /// Streaming call costs, when supported by the capture runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_cost: Option<crate::call_profile::RoutineCallCost>,
}

#[derive(Clone, Copy)]
struct OwnedRange {
    start: u32,
    end: u64,
    owner: usize,
}

/// Validated attribution index, prepared before any emulated state advances.
pub(crate) struct RoutinePlan {
    costs: Vec<RoutineCost>,
    ranges: BTreeMap<RoutineSpace, Vec<OwnedRange>>,
}

impl RoutinePlan {
    pub(crate) fn new(definitions: &[RoutineDefinition]) -> Result<Self, MachineError> {
        let invalid = |reason: &str| MachineError::InvalidRequest {
            reason: reason.into(),
        };
        if definitions.len() > MAX_ROUTINES {
            return Err(invalid("profile_cycles accepts at most 128 routines"));
        }
        if definitions
            .iter()
            .map(|definition| definition.ranges.len())
            .sum::<usize>()
            > MAX_ROUTINE_RANGES
        {
            return Err(invalid(
                "profile_cycles accepts at most 512 routine ranges in total",
            ));
        }
        let mut names = BTreeSet::new();
        let mut ranges = BTreeMap::<RoutineSpace, Vec<OwnedRange>>::new();
        for (owner, definition) in definitions.iter().enumerate() {
            if definition.name.trim().is_empty()
                || definition.name.chars().count() > 128
                || !names.insert(&definition.name)
            {
                return Err(invalid(
                    "routine names must be unique, nonblank and at most 128 characters",
                ));
            }
            if definition.ranges.is_empty() {
                return Err(invalid("each routine must declare at least one range"));
            }
            for range in &definition.ranges {
                if range.end <= u64::from(range.start) || range.end > u64::from(u32::MAX) + 1 {
                    return Err(invalid(
                        "routine ranges must have start < end <= 4294967296",
                    ));
                }
                ranges.entry(range.space).or_default().push(OwnedRange {
                    start: range.start,
                    end: range.end,
                    owner,
                });
            }
        }
        for entries in ranges.values_mut() {
            entries.sort_unstable_by_key(|range| range.start);
            if entries
                .windows(2)
                .any(|pair| u64::from(pair[1].start) < pair[0].end)
            {
                return Err(invalid(
                    "routine ranges in the same address space must not overlap",
                ));
            }
        }
        Ok(Self {
            costs: definitions
                .iter()
                .cloned()
                .map(|routine| RoutineCost {
                    name: routine.name,
                    ranges: routine.ranges,
                    instructions: 0,
                    exclusive_ticks: 0,
                    call_cost: None,
                })
                .collect(),
            ranges,
        })
    }

    pub(crate) fn owner(&self, address: u32, mapping: Option<ProfileMapping>) -> Option<usize> {
        let coordinate = match mapping {
            None => Some((RoutineSpace::Flat, address)),
            Some(mapping) => address.checked_sub(mapping.base).map(|offset| {
                (
                    RoutineSpace::Page {
                        memory: mapping.memory,
                        page: mapping.page,
                    },
                    offset,
                )
            }),
        };
        coordinate.and_then(|(space, offset)| {
            let ranges = self.ranges.get(&space)?;
            let index = ranges.partition_point(|range| range.start <= offset);
            let range = ranges.get(index.checked_sub(1)?)?;
            (u64::from(offset) < range.end).then_some(range.owner)
        })
    }

    pub(crate) fn apply(mut self, profile: &mut CycleProfile) {
        if self.costs.is_empty() {
            return;
        }
        let mut unassigned = 0;
        for entry in &profile.addresses {
            let owner = self.owner(entry.address, entry.mapping);
            if let Some(owner) = owner {
                self.costs[owner].instructions += entry.cost.executions;
                self.costs[owner].exclusive_ticks += entry.cost.ticks;
            } else {
                unassigned += entry.cost.ticks;
            }
        }
        profile.routines = self.costs;
        profile.unassigned_routine_ticks = Some(unassigned);
    }
}

/// Schema shared by the MCP registration and script deserializer's contract.
pub(crate) fn schema() -> serde_json::Value {
    serde_json::json!({
        "type":"array", "maxItems":MAX_ROUTINES,
        "items": {"type":"object", "required":["name","ranges"], "additionalProperties":false,
            "properties": {
                "name":{"type":"string","minLength":1,"maxLength":128},
                "ranges":{"type":"array","minItems":1,"maxItems":MAX_ROUTINE_RANGES,
                    "items":{"type":"object","required":["start","end"],"additionalProperties":false,
                        "properties":{
                            "start":{"type":"integer","minimum":0,"maximum":u32::MAX},
                            "end":{"type":"integer","minimum":1,"maximum":u64::from(u32::MAX)+1},
                            "space":{"oneOf":[
                                {"type":"object","required":["kind"],"additionalProperties":false,
                                 "properties":{"kind":{"const":"flat"}}},
                                {"type":"object","required":["kind","memory","page"],"additionalProperties":false,
                                 "properties":{"kind":{"const":"page"},"memory":{"enum":["ram","rom","rom_overlay","unmapped"]},"page":{"type":"integer","minimum":0,"maximum":u16::MAX}}}
                            ]}
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ClockDesc, ClockRate,
        cycle_profile::{CycleCounts, ExecutionCost, MappedExecutionCost, ProfileMapping},
    };

    fn definitions(value: serde_json::Value) -> Vec<RoutineDefinition> {
        serde_json::from_value(value).expect("routine definitions")
    }
    fn clock() -> ClockDesc {
        ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000))
    }

    #[test]
    fn physical_page_ranges_combine_aliases_without_mixing_banks_or_flat_addresses() {
        let counts = CycleCounts {
            ticks: 168,
            halt_ticks: 8,
            addresses: [
                (
                    0,
                    ExecutionCost {
                        executions: 4,
                        ticks: 64,
                    },
                ),
                (
                    0x4000,
                    ExecutionCost {
                        executions: 1,
                        ticks: 16,
                    },
                ),
                (
                    0xc000,
                    ExecutionCost {
                        executions: 5,
                        ticks: 80,
                    },
                ),
            ]
            .into(),
            mapped_addresses: [
                (0, ProfileMemory::Rom, 0, 0, 0, 4, 64),
                (0x4000, ProfileMemory::Ram, 5, 1, 0x4000, 1, 16),
                (0xc000, ProfileMemory::Ram, 5, 3, 0xc000, 2, 32),
                (0xc000, ProfileMemory::Ram, 3, 3, 0xc000, 3, 48),
            ]
            .into_iter()
            .map(
                |(address, memory, page, slot, base, executions, ticks)| MappedExecutionCost {
                    address,
                    mapping: ProfileMapping {
                        memory,
                        page,
                        slot,
                        base,
                    },
                    cost: ExecutionCost { executions, ticks },
                },
            )
            .collect(),
            ..CycleCounts::default()
        };
        let mut profile = counts.with_symbols(clock(), None);
        let definitions = definitions(serde_json::json!([
            {"name":"aliases","ranges":[{"start":0,"end":1,"space":{"kind":"page","memory":"ram","page":5}}]},
            {"name":"other_bank","ranges":[{"start":0,"end":1,"space":{"kind":"page","memory":"ram","page":3}}]},
            {"name":"flat_is_separate","ranges":[{"start":49152,"end":49153}]}
        ]));
        RoutinePlan::new(&definitions)
            .expect("disjoint definitions")
            .apply(&mut profile);
        assert_eq!(
            profile
                .routines
                .iter()
                .map(|cost| (cost.instructions, cost.exclusive_ticks))
                .collect::<Vec<_>>(),
            vec![(3, 48), (3, 48), (0, 0)]
        );
        assert_eq!(profile.unassigned_routine_ticks, Some(64));
        assert_eq!(profile.counts.halt_ticks, 8);
        assert_eq!(
            profile
                .routines
                .iter()
                .map(|cost| cost.exclusive_ticks)
                .sum::<u64>()
                + profile
                    .unassigned_routine_ticks
                    .expect("requested routines"),
            160
        );
    }

    #[test]
    fn half_open_ranges_can_include_the_final_address() {
        let mut profile = CycleCounts {
            ticks: 18,
            addresses: [
                (
                    u32::MAX,
                    ExecutionCost {
                        executions: 1,
                        ticks: 7,
                    },
                ),
                (
                    0,
                    ExecutionCost {
                        executions: 1,
                        ticks: 11,
                    },
                ),
            ]
            .into(),
            ..CycleCounts::default()
        }
        .with_symbols(clock(), None);
        let definitions = definitions(
            serde_json::json!([{"name":"last_byte","ranges":[{"start":4294967295u64,"end":4294967296u64}]}]),
        );
        RoutinePlan::new(&definitions)
            .expect("final address range")
            .apply(&mut profile);
        assert_eq!(profile.routines[0].exclusive_ticks, 7);
        assert_eq!(profile.unassigned_routine_ticks, Some(11));
    }

    #[test]
    fn physical_ranges_must_not_overlap_even_within_one_routine() {
        let definitions = definitions(serde_json::json!([{"name":"bad","ranges":[
            {"start":0,"end":10,"space":{"kind":"page","memory":"ram","page":5}},
            {"start":9,"end":12,"space":{"kind":"page","memory":"ram","page":5}}
        ]}]));
        assert!(RoutinePlan::new(&definitions).is_err());
    }

    #[test]
    fn request_size_limits_bound_the_attribution_index() {
        let one = RoutineDefinition {
            name: "one".into(),
            ranges: vec![RoutineRange {
                start: 0,
                end: 1,
                space: RoutineSpace::Flat,
            }],
        };
        assert!(RoutinePlan::new(&vec![one.clone(); MAX_ROUTINES + 1]).is_err());
        let too_many_ranges = RoutineDefinition {
            name: "many".into(),
            ranges: vec![one.ranges[0].clone(); MAX_ROUTINE_RANGES + 1],
        };
        assert!(RoutinePlan::new(&[too_many_ranges]).is_err());
    }
}
