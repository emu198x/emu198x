//! Bounded streaming call accounting over runtime-supplied instruction events.
//! No stack is guessed from labels, jumps or a post-capture memory disassembly.

use crate::{
    MachineError,
    cycle_profile::{CycleObserver, CycleProfile, ProfileEvent, ProfileFlow},
    routine_profile::{MAX_ROUTINES, RoutineDefinition, RoutinePlan},
};
use serde::{Deserialize, Serialize};

/// Hard bound, including capture roots, anonymous callees and interrupt barriers.
pub const MAX_CALL_DEPTH: usize = 256;

/// Costs sampled in this capture, not whole-program invocation durations.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineCallCost {
    /// Completed instruction ticks while this routine is active, including
    /// callees. Recursion charges each tick once per routine, not once per frame.
    pub inclusive_ticks: u64,
    /// Observed CALL/RST entries whose first completed instruction belongs here.
    /// Capture roots and interrupt entries are not calls.
    pub calls: u64,
    /// Observed calls closed by a matching return PC and restored stack pointer.
    pub completed_calls: u64,
    /// Observed calls still open at capture end or discarded after uncertainty.
    pub incomplete_calls: u64,
}

/// Completeness and resource diagnostics; exclusive counts are independent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallTracking {
    /// Largest retained stack, including roots and interrupt barriers.
    pub max_depth: usize,
    /// Frames still open at capture end, including roots and interrupts.
    pub open_frames: usize,
    /// Frames discarded after a discontinuity or depth exhaustion.
    pub discarded_frames: u64,
    /// Returns, destinations or routine ownership that broke the tracked chain.
    pub discontinuities: u64,
    /// Calls whose destination had no complete instruction before loss/end.
    pub unresolved_calls: u64,
    /// Observed call destinations outside every declared routine.
    pub unassigned_calls: u64,
    /// Tracking stopped at the bound; inclusive totals cover only its prefix.
    pub depth_limit_reached: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Root,
    Call,
    Interrupt,
}

struct Frame {
    kind: FrameKind,
    owner: Option<usize>,
    // Deferred until the callee's first complete instruction supplies its
    // actual physical mapping (including opcode-fetch-triggered overlays).
    awaiting: Option<(u32, u32)>,
    return_to: Option<(u32, u32)>,
}

/// Validates ranges before execution and consumes ordered events during capture.
pub struct CallProfiler {
    plan: RoutinePlan,
    costs: Vec<RoutineCallCost>,
    frames: Vec<Frame>,
    summary: CallTracking,
}

impl CallProfiler {
    /// Prepare a bounded capture with explicitly declared routine extents.
    ///
    /// # Errors
    /// Rejects invalid, overlapping or oversized routine definitions.
    pub fn new(definitions: &[RoutineDefinition]) -> Result<Self, MachineError> {
        Ok(Self {
            plan: RoutinePlan::new(definitions)?,
            costs: vec![RoutineCallCost::default(); definitions.len()],
            frames: Vec::new(),
            summary: CallTracking::default(),
        })
    }

    fn record_incomplete(&mut self, frame: &Frame) {
        if frame.kind == FrameKind::Call {
            if frame.awaiting.is_some() {
                self.summary.unresolved_calls += 1;
            } else if let Some(owner) = frame.owner {
                self.costs[owner].incomplete_calls += 1;
            }
        }
    }

    fn discard(&mut self) {
        self.summary.discarded_frames += self.frames.len() as u64;
        while let Some(frame) = self.frames.pop() {
            self.record_incomplete(&frame);
        }
    }

    fn push(&mut self, frame: Frame) {
        if self.frames.len() == MAX_CALL_DEPTH {
            self.summary.depth_limit_reached = true;
            self.record_incomplete(&frame);
            self.discard();
        } else {
            self.frames.push(frame);
            self.summary.max_depth = self.summary.max_depth.max(self.frames.len());
        }
    }

    fn instruction(&mut self, event: &ProfileEvent, address: u32) {
        let owner = self.plan.owner(address, event.mapping);
        if let Some(top) = self.frames.last_mut() {
            let consistent = if let Some((pc, sp)) = top.awaiting {
                pc == address && sp == event.stack_before
            } else {
                top.owner == owner
            };
            if !consistent {
                self.summary.discontinuities += 1;
                self.discard();
            } else if top.awaiting.take().is_some() {
                top.owner = owner;
                if top.kind == FrameKind::Call {
                    if let Some(owner) = owner {
                        self.costs[owner].calls += 1;
                    } else {
                        self.summary.unassigned_calls += 1;
                    }
                }
            }
        }
        if self.frames.is_empty() {
            self.push(Frame {
                kind: FrameKind::Root,
                owner,
                awaiting: None,
                return_to: None,
            });
        }
        // Handler instructions accrue only to the handler's segment. An ISR
        // can call routines normally, without charging the interrupted caller.
        let mut charged = [false; MAX_ROUTINES];
        for frame in self.frames.iter().rev() {
            if let Some(owner) = frame.owner
                && !charged[owner]
            {
                self.costs[owner].inclusive_ticks += event.ticks;
                charged[owner] = true;
            }
            if frame.kind == FrameKind::Interrupt {
                break;
            }
        }
    }

    /// Attach exclusive costs, inclusive costs and completeness diagnostics.
    /// The runtime's raw totals and non-instruction buckets remain unchanged.
    pub fn finish(mut self, profile: &mut CycleProfile) {
        self.summary.open_frames = self.frames.len();
        while let Some(frame) = self.frames.pop() {
            self.record_incomplete(&frame);
        }
        self.plan.apply(profile);
        for (routine, cost) in profile.routines.iter_mut().zip(self.costs) {
            routine.call_cost = Some(cost);
        }
        profile.call_tracking = Some(self.summary);
    }
}

impl CycleObserver for CallProfiler {
    fn observe(&mut self, event: ProfileEvent) {
        if self.summary.depth_limit_reached {
            return;
        }
        if let Some(address) = event.address {
            self.instruction(&event, address);
        }
        match event.flow {
            Some(ProfileFlow::Call { return_address })
            | Some(ProfileFlow::Interrupt { return_address }) => {
                self.push(Frame {
                    kind: if event.address.is_none() {
                        FrameKind::Interrupt
                    } else {
                        FrameKind::Call
                    },
                    owner: None,
                    awaiting: Some((event.next_pc, event.stack_after)),
                    return_to: Some((return_address, event.stack_before)),
                });
            }
            Some(ProfileFlow::Return) => {
                if self.frames.last().and_then(|frame| frame.return_to)
                    == Some((event.next_pc, event.stack_after))
                {
                    if let Some(frame) = self.frames.pop()
                        && frame.kind == FrameKind::Call
                        && let Some(owner) = frame.owner
                    {
                        self.costs[owner].completed_calls += 1;
                    }
                } else {
                    self.summary.discontinuities += 1;
                    self.discard();
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ClockDesc, ClockRate,
        cycle_profile::{CycleCounts, ProfileMapping, ProfileMemory},
    };

    fn profiler() -> CallProfiler {
        CallProfiler::new(
            &serde_json::from_value::<Vec<RoutineDefinition>>(serde_json::json!([
                {"name":"root","ranges":[{"start":0,"end":16}]},
                {"name":"work","ranges":[{"start":100,"end":120}]},
                {"name":"irq","ranges":[{"start":200,"end":220}]}
            ]))
            .expect("ranges"),
        )
        .expect("valid plan")
    }
    fn event(
        address: u32,
        ticks: u64,
        before: u32,
        after: u32,
        next: u32,
        flow: Option<ProfileFlow>,
    ) -> ProfileEvent {
        ProfileEvent {
            address: Some(address),
            mapping: None,
            ticks,
            stack_before: before,
            stack_after: after,
            next_pc: next,
            flow,
        }
    }
    fn call(return_address: u32) -> Option<ProfileFlow> {
        Some(ProfileFlow::Call { return_address })
    }
    fn finish(profiler: CallProfiler) -> CycleProfile {
        let mut report = CycleCounts::default()
            .with_symbols(ClockDesc::new("tick", ClockRate::from_hz(1)), None);
        profiler.finish(&mut report);
        report
    }
    fn costs(report: &CycleProfile) -> Vec<(u64, u64, u64, u64)> {
        report
            .routines
            .iter()
            .map(|routine| {
                let cost = routine.call_cost.as_ref().expect("call accounting");
                (
                    cost.inclusive_ticks,
                    cost.calls,
                    cost.completed_calls,
                    cost.incomplete_calls,
                )
            })
            .collect()
    }

    #[test]
    fn recursive_frames_count_ticks_once_per_active_routine() {
        let mut profiler = profiler();
        for e in [
            event(0, 4, 1000, 998, 100, call(1)),
            event(100, 5, 998, 996, 100, call(101)),
            event(100, 6, 996, 998, 101, Some(ProfileFlow::Return)),
            event(101, 7, 998, 1000, 1, Some(ProfileFlow::Return)),
            event(1, 3, 1000, 1000, 2, None),
        ] {
            profiler.observe(e);
        }
        let report = finish(profiler);
        assert_eq!(
            costs(&report),
            vec![(25, 0, 0, 0), (18, 2, 2, 0), (0, 0, 0, 0)]
        );
        let summary = report.call_tracking.expect("summary");
        assert_eq!(summary.discontinuities, 0);
        assert_eq!(summary.max_depth, 3);
        assert_eq!(summary.open_frames, 1);
    }

    #[test]
    fn interrupt_barrier_suspends_caller_but_allows_handler_calls() {
        let mut profiler = profiler();
        profiler.observe(event(0, 2, 1000, 1000, 1, None));
        let mut interrupt = event(
            0,
            99,
            1000,
            998,
            200,
            Some(ProfileFlow::Interrupt { return_address: 1 }),
        );
        interrupt.address = None;
        profiler.observe(interrupt);
        for e in [
            event(200, 3, 998, 996, 100, call(201)),
            event(100, 4, 996, 998, 201, Some(ProfileFlow::Return)),
            event(201, 5, 998, 1000, 1, Some(ProfileFlow::Return)),
            event(1, 6, 1000, 1000, 2, None),
        ] {
            profiler.observe(e);
        }
        let report = finish(profiler);
        assert_eq!(
            costs(&report),
            vec![(8, 0, 0, 0), (4, 1, 1, 0), (12, 0, 0, 0)]
        );
        assert_eq!(report.call_tracking.expect("summary").discontinuities, 0);
    }

    #[test]
    fn mismatched_return_discards_ancestry_and_marks_calls_incomplete() {
        let mut profiler = profiler();
        profiler.observe(event(0, 4, 1000, 998, 100, call(1)));
        profiler.observe(event(100, 5, 998, 1234, 1, Some(ProfileFlow::Return)));
        profiler.observe(event(101, 6, 1234, 1234, 102, None));
        let report = finish(profiler);
        assert_eq!(
            costs(&report),
            vec![(9, 0, 0, 0), (11, 1, 0, 1), (0, 0, 0, 0)]
        );
        let summary = report.call_tracking.expect("summary");
        assert_eq!(summary.discontinuities, 1);
        assert_eq!(summary.discarded_frames, 2);
    }

    #[test]
    fn first_callee_instruction_uses_physical_mapping_and_aliases() {
        let definitions = serde_json::from_value::<Vec<RoutineDefinition>>(serde_json::json!([
            {"name":"bank5","ranges":[{"start":0,"end":16,"space":{"kind":"page","memory":"ram","page":5}}]},
            {"name":"bank3","ranges":[{"start":256,"end":272,"space":{"kind":"page","memory":"ram","page":3}}]}
        ])).expect("banked ranges");
        let mut profiler = CallProfiler::new(&definitions).expect("plan");
        let mapped = |mut e: ProfileEvent, page, base| {
            e.mapping = Some(ProfileMapping {
                memory: ProfileMemory::Ram,
                page,
                base,
                slot: (base >> 14) as u8,
            });
            e
        };
        profiler.observe(mapped(
            event(0xc000, 4, 1000, 998, 0x8100, call(0xc003)),
            5,
            0xc000,
        ));
        profiler.observe(mapped(
            event(0x8100, 5, 998, 1000, 0xc003, Some(ProfileFlow::Return)),
            3,
            0x8000,
        ));
        // A different return bank must not inherit the original caller's cost.
        profiler.observe(mapped(
            event(0xc003, 6, 1000, 1000, 0xc004, None),
            7,
            0xc000,
        ));
        let report = finish(profiler);
        assert_eq!(costs(&report), vec![(9, 0, 0, 0), (5, 1, 1, 0)]);
        assert_eq!(report.call_tracking.expect("summary").discontinuities, 1);
    }

    #[test]
    fn capture_end_before_callee_completion_reports_unresolved_call() {
        let mut profiler = profiler();
        profiler.observe(event(0, 4, 1000, 998, 100, call(1)));
        let report = finish(profiler);
        assert_eq!(
            costs(&report),
            vec![(4, 0, 0, 0), (0, 0, 0, 0), (0, 0, 0, 0)]
        );
        let summary = report.call_tracking.expect("summary");
        assert_eq!(summary.unresolved_calls, 1);
        assert_eq!(summary.open_frames, 2);
    }

    #[test]
    fn depth_limit_stops_inclusive_accounting_without_an_unbounded_trace() {
        let mut profiler = profiler();
        for depth in 0..MAX_CALL_DEPTH + 3 {
            let sp = 1000 - u32::try_from(depth).expect("bounded depth") * 2;
            profiler.observe(event(0, 1, sp, sp - 2, 0, call(1)));
        }
        let report = finish(profiler);
        let cost = report.routines[0].call_cost.as_ref().expect("cost");
        assert_eq!(cost.inclusive_ticks, MAX_CALL_DEPTH as u64);
        assert_eq!(cost.calls, MAX_CALL_DEPTH as u64 - 1);
        assert_eq!(cost.incomplete_calls, cost.calls);
        let summary = report.call_tracking.expect("summary");
        assert!(summary.depth_limit_reached);
        assert_eq!(summary.max_depth, MAX_CALL_DEPTH);
        assert_eq!(summary.unresolved_calls, 1);
        assert_eq!(summary.open_frames, 0);
    }
}
