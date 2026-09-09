//! Compare trusted same-build Asm198x listings with measured instruction costs.
//! Static label totals are deliberately not used: loops need execution weighting.

use crate::{
    MachineError,
    cycle_profile::CycleProfile,
    debug_info::SourceLine,
    routine_profile::{RoutineDefinition, RoutinePlan},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum listing records accepted in one capture request.
pub const MAX_STATIC_LINES: usize = 16_384;

/// Runtime-declared conversion from machine ticks to the static CPU cycle unit.
#[derive(Clone, Copy, Debug)]
pub struct CycleTiming {
    /// ISA identifier used by the assembler.
    pub cpu: &'static str,
    /// Machine ticks per CPU cycle (Z80 T-state). Stalls remain measured time.
    pub ticks_per_cycle: u32,
}

/// Asm198x listing plus an explicit CPU declaration: the listing lacks that field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticCycles {
    /// Must match the runtime's declared CPU.
    pub cpu: String,
    /// Raw Asm198x JSON listing. Labels, coverage and areas are not cost inputs.
    pub listing: StaticListing,
}

/// Only line records participate; unrelated assembler metadata is ignored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticListing {
    /// Emitted source spans, including data spans without cycle estimates.
    pub lines: Vec<StaticLine>,
}

/// One source span from the current flat listing contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticLine {
    /// Absolute CPU address; unresolved section addresses remain uncomparable.
    pub address: Option<u32>,
    /// Section marker, if emitted by a native/linked listing.
    #[serde(default)]
    pub section: Option<u32>,
    /// Source path exactly as recorded in the matching Debug198x sidecar.
    pub file: String,
    /// One-based source line.
    pub line: u32,
    /// Emitted byte length.
    pub bytes: u32,
    /// Absent means no static timing, never zero cost.
    #[serde(default)]
    pub cycles: Option<CycleRange>,
}

/// Inclusive minimum/maximum of static CPU cycles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleRange {
    /// Minimum CPU cycles.
    pub min: u64,
    /// Maximum CPU cycles.
    pub max: u64,
}

/// Measured relationship to an execution-weighted static interval. An excess
/// does not by itself prove contention: build/spec/core disagreement is possible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleRelation {
    BelowRange,
    WithinRange,
    AboveRange,
}

/// Compared instruction start, after an exact address and source join.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticAddressCost {
    /// Flat instruction address.
    pub address: u32,
    /// Matching source location.
    pub source: SourceLine,
    /// Completed executions, including repeated block iterations.
    pub executions: u64,
    /// Machine ticks measured for those executions.
    pub measured_ticks: u64,
    /// Static interval multiplied by completed execution count.
    pub expected_cycles: CycleRange,
    /// Comparison in exact integer ticks, with no rounding.
    pub relation: CycleRelation,
}

/// Exclusive comparison over the comparable subset of one declared routine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticRoutineCost {
    /// Requested routine name.
    pub name: String,
    /// Measured ticks included in the static comparison.
    pub compared_ticks: u64,
    /// Exclusive instruction ticks for which no safe comparison was possible.
    pub uncomparable_ticks: u64,
    /// Execution-weighted interval for the comparable subset only.
    pub expected_cycles: CycleRange,
}

/// Reasons measured code could not safely join the flat listing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UncomparableTicks {
    /// Physical pages cannot be established from flat listing addresses.
    pub banked: u64,
    /// No unique flat row, including overlaps, repeated source lines and sections.
    pub ambiguous_or_missing_row: u64,
    /// Listing explicitly has no cycle estimate for the span.
    pub missing_cycles: u64,
    /// Loaded sidecar disagrees with the listing's exact file/line location.
    pub source_mismatch: u64,
}

/// Static/measured comparison; excludes IRQ entry, HALT waiting and partial time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticComparison {
    /// Explicit CPU identifier validated against the runtime.
    pub cpu: String,
    /// Divide measured ticks by this number for exact CPU-cycle equivalents;
    /// retain any remainder rather than rounding or calling it active CPU time.
    pub ticks_per_cpu_cycle: u32,
    /// Total measured ticks with comparable static records.
    pub compared_ticks: u64,
    /// Execution-weighted CPU-cycle interval for that same subset.
    pub expected_cycles: CycleRange,
    /// Measured instruction time excluded from comparison, grouped by reason.
    pub uncomparable_ticks: UncomparableTicks,
    /// Comparable instruction addresses.
    pub addresses: Vec<StaticAddressCost>,
    /// Exclusive comparisons in requested routine order, never inclusive costs.
    pub routines: Vec<StaticRoutineCost>,
}

pub(crate) struct ComparisonPlan {
    timing: CycleTiming,
    rows: BTreeMap<u32, StaticLine>,
    routines: RoutinePlan,
    names: Vec<String>,
}

impl ComparisonPlan {
    pub(crate) fn new(
        request: &StaticCycles,
        timing: Option<CycleTiming>,
        routines: &[RoutineDefinition],
    ) -> Result<Self, MachineError> {
        let invalid = |reason: &str| MachineError::InvalidRequest {
            reason: reason.into(),
        };
        let timing = timing
            .filter(|t| t.ticks_per_cycle > 0 && t.ticks_per_cycle <= 1024 && t.cpu == request.cpu)
            .ok_or_else(|| {
                invalid("static cycle CPU must match a runtime with a fixed cycle conversion")
            })?;
        if request.listing.lines.len() > MAX_STATIC_LINES {
            return Err(invalid("static cycle listing accepts at most 16384 rows"));
        }
        let mut source_counts = BTreeMap::new();
        let mut address_counts = BTreeMap::new();
        let mut spans = Vec::new();
        for row in &request.listing.lines {
            if row.file.trim().is_empty()
                || row.file.len() > 1024
                || row.line == 0
                || row.bytes == 0
                || row
                    .address
                    .is_some_and(|a| u64::from(a) + u64::from(row.bytes) > u64::from(u32::MAX) + 1)
                || row
                    .cycles
                    .is_some_and(|c| c.min == 0 || c.min > c.max || c.max > 1_000_000)
            {
                return Err(invalid(
                    "invalid static cycle row, source span or cycle range",
                ));
            }
            *source_counts.entry((&row.file, row.line)).or_insert(0usize) += 1;
            if let Some(address) = row.address {
                *address_counts.entry(address).or_insert(0usize) += 1;
                spans.push((address, u64::from(address) + u64::from(row.bytes)));
            }
        }
        // Mark all overlapping spans, including a wide span enclosing many rows.
        spans.sort_unstable();
        let mut ambiguous = BTreeSet::new();
        let mut furthest: Option<(u32, u64)> = None;
        for (start, end) in spans {
            if let Some((prior, prior_end)) = furthest
                && u64::from(start) < prior_end
            {
                ambiguous.insert(prior);
                ambiguous.insert(start);
            }
            if furthest.is_none_or(|(_, prior_end)| end > prior_end) {
                furthest = Some((start, end));
            }
        }
        let rows = request
            .listing
            .lines
            .iter()
            .filter(|row| {
                row.section.is_none()
                    && source_counts[&(&row.file, row.line)] == 1
                    && row
                        .address
                        .is_some_and(|a| address_counts[&a] == 1 && !ambiguous.contains(&a))
            })
            .map(|row| (row.address.expect("filtered resolved row"), row.clone()))
            .collect();
        Ok(Self {
            timing,
            rows,
            routines: RoutinePlan::new(routines)?,
            names: routines.iter().map(|r| r.name.clone()).collect(),
        })
    }

    pub(crate) fn apply(self, profile: &mut CycleProfile) {
        let mut report = StaticComparison {
            cpu: self.timing.cpu.into(),
            ticks_per_cpu_cycle: self.timing.ticks_per_cycle,
            compared_ticks: 0,
            expected_cycles: CycleRange::default(),
            uncomparable_ticks: UncomparableTicks::default(),
            addresses: Vec::new(),
            routines: self
                .names
                .into_iter()
                .map(|name| StaticRoutineCost {
                    name,
                    compared_ticks: 0,
                    uncomparable_ticks: 0,
                    expected_cycles: CycleRange::default(),
                })
                .collect(),
        };
        let starts: BTreeSet<_> = profile
            .addresses
            .iter()
            .filter(|entry| entry.mapping.is_none())
            .map(|entry| entry.address)
            .collect();
        for entry in &profile.addresses {
            let owner = self.routines.owner(entry.address, entry.mapping);
            let row = self.rows.get(&entry.address).filter(|row| {
                // Multiple executed starts inside one source span cannot safely
                // share a static total or one execution multiplier.
                use std::ops::Bound::{Excluded, Unbounded};
                starts
                    .range((Excluded(entry.address), Unbounded))
                    .next()
                    .is_none_or(|next| {
                        u64::from(*next) >= u64::from(entry.address) + u64::from(row.bytes)
                    })
            });
            let missing = if entry.mapping.is_some() {
                Some(&mut report.uncomparable_ticks.banked)
            } else if row.is_none() {
                Some(&mut report.uncomparable_ticks.ambiguous_or_missing_row)
            } else if row.is_some_and(|row| row.cycles.is_none()) {
                Some(&mut report.uncomparable_ticks.missing_cycles)
            } else if !row.is_some_and(|row| {
                entry
                    .source
                    .as_ref()
                    .is_some_and(|s| s.file == row.file && s.line == row.line)
            }) {
                Some(&mut report.uncomparable_ticks.source_mismatch)
            } else {
                None
            };
            if let Some(bucket) = missing {
                *bucket += entry.cost.ticks;
                if let Some(owner) = owner {
                    report.routines[owner].uncomparable_ticks += entry.cost.ticks;
                }
                continue;
            }
            let row = row.expect("comparable row");
            let cycles = row.cycles.expect("comparable cycles");
            let expected = CycleRange {
                min: cycles.min * entry.cost.executions,
                max: cycles.max * entry.cost.executions,
            };
            let divisor = u64::from(self.timing.ticks_per_cycle);
            let relation = if entry.cost.ticks < expected.min * divisor {
                CycleRelation::BelowRange
            } else if entry.cost.ticks > expected.max * divisor {
                CycleRelation::AboveRange
            } else {
                CycleRelation::WithinRange
            };
            report.compared_ticks += entry.cost.ticks;
            report.expected_cycles.min += expected.min;
            report.expected_cycles.max += expected.max;
            if let Some(owner) = owner {
                let routine = &mut report.routines[owner];
                routine.compared_ticks += entry.cost.ticks;
                routine.expected_cycles.min += expected.min;
                routine.expected_cycles.max += expected.max;
            }
            report.addresses.push(StaticAddressCost {
                address: entry.address,
                source: SourceLine {
                    file: row.file.clone(),
                    line: row.line,
                },
                executions: entry.cost.executions,
                measured_ticks: entry.cost.ticks,
                expected_cycles: expected,
                relation,
            });
        }
        profile.static_comparison = Some(Box::new(report));
    }
}

pub(crate) fn schema() -> serde_json::Value {
    serde_json::json!({"type":"object","required":["cpu","listing"],"additionalProperties":false,"properties":{
        "cpu":{"type":"string","enum":["z80"]},
        "listing":{"type":"object","required":["lines"],"properties":{"lines":{"type":"array","maxItems":MAX_STATIC_LINES,
            "items":{"type":"object","required":["address","file","line","bytes"],"properties":{
                "address":{"type":["integer","null"],"minimum":0,"maximum":u32::MAX},"section":{"type":["integer","null"],"minimum":0,"maximum":u32::MAX},
                "file":{"type":"string","minLength":1,"maxLength":1024},"line":{"type":"integer","minimum":1,"maximum":u32::MAX},"bytes":{"type":"integer","minimum":1,"maximum":u32::MAX},
                "cycles":{"type":"object","required":["min","max"],"properties":{"min":{"type":"integer","minimum":1,"maximum":1_000_000},"max":{"type":"integer","minimum":1,"maximum":1_000_000}}}
            }}}}}
    }})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ClockDesc, ClockRate,
        cycle_profile::{CycleCounts, ExecutionCost, ProfileMapping, ProfileMemory},
    };
    fn request(lines: serde_json::Value) -> StaticCycles {
        serde_json::from_value(serde_json::json!({"cpu":"z80","listing":{"lines":lines}}))
            .expect("listing")
    }
    fn row(address: u32, line: u32) -> serde_json::Value {
        serde_json::json!({"address":address,"file":"code.asm","line":line,"bytes":1,"cycles":{"min":4,"max":4}})
    }
    fn timing() -> Option<CycleTiming> {
        Some(CycleTiming {
            cpu: "z80",
            ticks_per_cycle: 5,
        })
    }
    fn profile(ticks: u64) -> CycleProfile {
        let mut profile = CycleCounts {
            ticks,
            addresses: [(
                0,
                ExecutionCost {
                    executions: 2,
                    ticks,
                },
            )]
            .into(),
            ..CycleCounts::default()
        }
        .with_symbols(
            ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            None,
        );
        profile.addresses[0].source = Some(SourceLine {
            file: "code.asm".into(),
            line: 1,
        });
        profile
    }
    #[test]
    fn exact_tick_conversion_preserves_remainders_and_range_direction() {
        for (ticks, relation) in [
            (39, CycleRelation::BelowRange),
            (40, CycleRelation::WithinRange),
            (41, CycleRelation::AboveRange),
        ] {
            let mut profile = profile(ticks);
            ComparisonPlan::new(&request(serde_json::json!([row(0, 1)])), timing(), &[])
                .expect("plan")
                .apply(&mut profile);
            let comparison = profile.static_comparison.expect("report");
            assert_eq!(comparison.ticks_per_cpu_cycle, 5);
            assert_eq!(comparison.addresses[0].measured_ticks, ticks);
            assert_eq!(
                comparison.addresses[0].expected_cycles,
                CycleRange { min: 8, max: 8 }
            );
            assert_eq!(comparison.addresses[0].relation, relation);
        }
    }
    #[test]
    fn missing_estimates_source_mismatches_and_banks_are_not_zero_cost_matches() {
        for reason in 0..3 {
            let mut profile = profile(40);
            let mut line = row(0, 1);
            if reason == 0 {
                line.as_object_mut().expect("object").remove("cycles");
            }
            if reason == 1 {
                line["file"] = "other.asm".into();
            }
            if reason == 2 {
                profile.addresses[0].mapping = Some(ProfileMapping {
                    memory: ProfileMemory::Ram,
                    page: 0,
                    slot: 0,
                    base: 0,
                });
            }
            ComparisonPlan::new(&request(serde_json::json!([line])), timing(), &[])
                .expect("plan")
                .apply(&mut profile);
            let report = profile.static_comparison.expect("report");
            assert_eq!(report.compared_ticks, 0);
            let reasons = report.uncomparable_ticks;
            assert_eq!(
                reasons.missing_cycles + reasons.source_mismatch + reasons.banked,
                40
            );
        }
    }
    #[test]
    fn repeated_source_lines_overlapping_spans_and_sections_are_ambiguous() {
        for lines in [
            serde_json::json!([row(0, 1), row(5, 1)]),
            serde_json::json!([row(0, 1), row(0, 2)]),
            serde_json::json!([{"address":0,"file":"code.asm","line":1,"bytes":10,"cycles":{"min":4,"max":4}},row(5,2),row(7,3)]),
            serde_json::json!([{"address":0,"section":0,"file":"code.asm","line":1,"bytes":1,"cycles":{"min":4,"max":4}}]),
        ] {
            let mut profile = profile(40);
            ComparisonPlan::new(&request(lines), timing(), &[])
                .expect("plan")
                .apply(&mut profile);
            assert_eq!(
                profile
                    .static_comparison
                    .expect("report")
                    .uncomparable_ticks
                    .ambiguous_or_missing_row,
                40
            );
        }
    }
    #[test]
    fn multiple_executed_starts_inside_a_single_row_cannot_share_its_total() {
        let mut profile = profile(40);
        let mut second = profile.addresses[0].clone();
        second.address = 1;
        profile.addresses.push(second);
        let mut line = row(0, 1);
        line["bytes"] = 2.into();
        ComparisonPlan::new(&request(serde_json::json!([line])), timing(), &[])
            .expect("plan")
            .apply(&mut profile);
        let report = profile.static_comparison.expect("report");
        assert_eq!(report.compared_ticks, 0);
        assert_eq!(report.uncomparable_ticks.ambiguous_or_missing_row, 80);
    }
    #[test]
    fn invalid_bounds_and_conversion_are_rejected() {
        let good = request(serde_json::json!([row(0, 1)]));
        assert!(ComparisonPlan::new(&good, None, &[]).is_err());
        assert!(
            ComparisonPlan::new(
                &good,
                Some(CycleTiming {
                    cpu: "6502",
                    ticks_per_cycle: 5
                }),
                &[]
            )
            .is_err()
        );
        for line in [
            serde_json::json!({"address":0,"file":"code.asm","line":1,"bytes":0}),
            serde_json::json!({"address":4294967295u64,"file":"code.asm","line":1,"bytes":2}),
            serde_json::json!({"address":0,"file":"code.asm","line":1,"bytes":1,"cycles":{"min":8,"max":4}}),
        ] {
            assert!(
                ComparisonPlan::new(&request(serde_json::json!([line])), timing(), &[]).is_err()
            );
        }
        let huge = StaticCycles {
            cpu: "z80".into(),
            listing: StaticListing {
                lines: vec![good.listing.lines[0].clone(); MAX_STATIC_LINES + 1],
            },
        };
        assert!(ComparisonPlan::new(&huge, timing(), &[]).is_err());
    }
}
