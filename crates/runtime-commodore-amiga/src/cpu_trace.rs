//! Instruction-boundary CPU trace, owned by the runtime.
//!
//! Moved here from the `emu198x-amiga` MCP session (`AmigaSession::
//! tick_with_trace` + `CpuTraceState`) so the trace is captured during
//! the runtime's *own* tick, regardless of which entry point drove it:
//!
//!   - the shared `MachineCore::run_until` loop (the `run_frames` /
//!     `run_ticks` shell tools route through here), and
//!   - the fine-grained `AmigaLiveAccess::tick` the `step` /
//!     `run_until_*` tools pump one master/4 tick at a time.
//!
//! Both funnel through [`AmigaRuntime::tick_traced`], so an armed trace
//! captures the same boundaries no matter who is ticking. This is what
//! lets the Amiga MCP run on the shared `HeadlessSession` without a
//! bespoke `tick_with_trace` wrapper on the session.
//!
//! Every tick drains the machine's instruction-boundary queue, including
//! while tracing is disarmed. When armed, each drained boundary passes
//! through the optional PC filter and into the bounded trace buffer. This
//! preserves multiple boundaries crossed by faster CPUs during one Amiga
//! system tick instead of collapsing them to the last observed CPU state.

use crate::AmigaLiveAccess;
use crate::runtime::AmigaRuntime;
use crate::variants::AmigaMachine;

/// One captured instruction-boundary CPU snapshot:
/// `(system_tick, instr_start_pc, sr, opcode_word)`. The machine records
/// the opcode and register state at the boundary itself, before a later CPU
/// edge can replace them. `system_tick` is the zero-based tick being executed
/// when the boundary is crossed; after that tick completes, the machine's
/// completed-tick count is one greater. Faster CPUs can emit several entries
/// with the same timestamp.
pub type CpuTraceEntry = (u64, u32, u16, u16);

/// Default cap on captured entries (~1.6 MB at 16 bytes each). Past the
/// cap, further pushes are dropped (silent truncation); the tool layer
/// reports `at_limit` so the truncation is visible.
const DEFAULT_MAX_ENTRIES: usize = 100_000;

/// Armed instruction-boundary trace state. Lives on [`AmigaRuntime`] so
/// it clears naturally on `reset()` and is captured by the runtime's own
/// tick loop.
pub struct CpuTrace {
    /// `true` while recording. Toggled by `cpu_trace_arm` / `_disarm`.
    armed: bool,
    /// Optional inclusive PC range `(min, max)`; entries whose
    /// `instr_start_pc` falls outside are dropped before push.
    pc_filter: Option<(u32, u32)>,
    /// Hard cap on captured entries.
    max_entries: usize,
    /// Captured entries, oldest first.
    entries: Vec<CpuTraceEntry>,
}

impl Default for CpuTrace {
    fn default() -> Self {
        Self {
            armed: false,
            pc_filter: None,
            max_entries: DEFAULT_MAX_ENTRIES,
            entries: Vec::new(),
        }
    }
}

impl CpuTrace {
    /// Drop captured entries.
    /// Called from `MachineCore::reset` so pre-reset entries don't bleed
    /// into post-reset analysis. Arm-state and filter are kept — re-arming
    /// after every reset would be tedious for a debugging session.
    pub(crate) fn clear_on_reset(&mut self) {
        self.entries.clear();
    }

    fn capture_boundaries(&mut self, boundaries: impl IntoIterator<Item = CpuTraceEntry>) {
        for boundary in boundaries {
            if self.entries.len() >= self.max_entries {
                break;
            }
            let pc = boundary.1;
            if let Some((lo, hi)) = self.pc_filter
                && (pc < lo || pc > hi)
            {
                continue;
            }
            self.entries.push(boundary);
        }
    }
}

impl<M: AmigaMachine + AmigaLiveAccess> AmigaRuntime<M> {
    /// Advance one master/4 tick and, if the trace is armed, capture a
    /// snapshot at every instruction boundary the tick crosses. The
    /// single tick funnel both the `run_until` loop and the per-tick
    /// stepping tools route through.
    pub(crate) fn tick_traced(&mut self) {
        AmigaMachine::tick(&mut self.machine);
        let boundaries = AmigaMachine::drain_cpu_boundaries(&mut self.machine);
        if !self.cpu_trace.armed {
            return;
        }
        self.cpu_trace.capture_boundaries(boundaries);
    }

    /// Advance the active CPU to one instruction boundary without allowing a
    /// faster processor to run through later boundaries in the same system
    /// tick. Returns whether a boundary was crossed.
    pub(crate) fn advance_to_cpu_boundary_traced(&mut self) -> bool {
        let start_tick = AmigaLiveAccess::tick_count(&self.machine);
        let crossed_boundary = AmigaMachine::advance_to_cpu_boundary(&mut self.machine);
        let completed_ticks = AmigaLiveAccess::tick_count(&self.machine).wrapping_sub(start_tick);
        debug_assert!(
            completed_ticks <= 1,
            "one boundary advance can complete at most one system tick"
        );
        {
            let boundaries = AmigaMachine::drain_cpu_boundaries(&mut self.machine);
            if self.cpu_trace.armed {
                self.cpu_trace.capture_boundaries(boundaries);
            }
        }
        self.account_debug_progress(completed_ticks);
        crossed_boundary
    }

    /// Start recording. Clears any prior trace and replaces the filter +
    /// cap. Drains any boundaries retained outside the runtime tick funnel
    /// so the first captured entry is crossed after arming.
    pub fn cpu_trace_arm(&mut self, pc_filter: Option<(u32, u32)>, max_entries: usize) {
        self.cpu_trace.armed = true;
        self.cpu_trace.pc_filter = pc_filter;
        self.cpu_trace.max_entries = max_entries;
        self.cpu_trace.entries.clear();
        let _ = AmigaMachine::drain_cpu_boundaries(&mut self.machine);
    }

    /// Stop recording; keep the captured entries. Returns the entry
    /// count at the moment of disarm.
    pub fn cpu_trace_disarm(&mut self) -> usize {
        self.cpu_trace.armed = false;
        self.cpu_trace.entries.len()
    }

    /// Discard captured and pending entries without disarming. Returns how
    /// many captured entries were dropped.
    pub fn cpu_trace_clear(&mut self) -> usize {
        let dropped = self.cpu_trace.entries.len();
        self.cpu_trace.entries.clear();
        let _ = AmigaMachine::drain_cpu_boundaries(&mut self.machine);
        dropped
    }

    /// Whether the trace is currently recording.
    #[must_use]
    pub fn cpu_trace_armed(&self) -> bool {
        self.cpu_trace.armed
    }

    /// Current hard cap on captured entries.
    #[must_use]
    pub fn cpu_trace_max_entries(&self) -> usize {
        self.cpu_trace.max_entries
    }

    /// Captured entries, oldest first.
    #[must_use]
    pub fn cpu_trace_entries(&self) -> &[CpuTraceEntry] {
        &self.cpu_trace.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_access::AmigaLiveAccess;
    use crate::variants::AmigaRuntimeKind;
    use crate::{Model, profiles::A500_PAL_FRAME_TICKS};
    use emu198x_shell::{
        HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
    };

    /// The trace buffer retains distinct entries even when multiple
    /// instruction boundaries share one Amiga system tick.
    #[test]
    fn capture_preserves_multiple_boundaries_from_one_system_tick() {
        let boundaries = [
            (17, 0x1000, 0x2700, 0x4E71),
            (17, 0x1002, 0x2700, 0x7000),
            (18, 0x1004, 0x2700, 0x4E75),
        ];
        let mut trace = CpuTrace {
            armed: true,
            max_entries: boundaries.len(),
            ..CpuTrace::default()
        };

        trace.capture_boundaries(boundaries);

        assert_eq!(trace.entries, boundaries);
    }

    #[test]
    fn trace_timestamp_names_the_zero_based_tick_containing_the_boundary() {
        let mut kind = AmigaRuntimeKind::blank(Model::A500OcsPal);
        kind.cpu_trace_arm(None, 1);

        for _ in 0..2_000 {
            AmigaLiveAccess::tick(&mut kind);
            if let Some(entry) = kind.cpu_trace_entries().first() {
                assert_eq!(
                    entry.0.wrapping_add(1),
                    AmigaLiveAccess::tick_count(&kind),
                    "a boundary captured during a completed tick names that tick, not the following one"
                );
                return;
            }
        }
        panic!("blank A500 firmware did not cross an instruction boundary");
    }

    /// Disarmed per-tick stepping captures nothing and still drains the
    /// machine queue, so arming later cannot report stale boundaries.
    #[test]
    fn disarmed_trace_captures_nothing_and_drains_machine_queue() {
        let mut kind = AmigaRuntimeKind::blank(Model::A1200AgaPal);
        assert!(!kind.cpu_trace_armed());
        for _ in 0..500 {
            AmigaLiveAccess::tick(&mut kind);
        }
        assert!(kind.cpu_trace_entries().is_empty());
        let AmigaRuntimeKind::Aga(runtime) = &mut kind else {
            panic!("A1200 must use the AGA runtime");
        };
        assert!(
            runtime
                .machine_mut()
                .drain_cpu_boundaries()
                .next()
                .is_none()
        );
    }

    /// Every instruction-start counter increment is represented by one
    /// captured boundary, including on the A1200's two-edge CPU clock.
    #[test]
    fn a1200_trace_matches_instruction_start_delta() {
        let mut kind = AmigaRuntimeKind::blank(Model::A1200AgaPal);
        kind.cpu_trace_arm(None, 10_000);
        let starts_before = kind.cpu_instruction_starts();

        for _ in 0..2_000 {
            AmigaLiveAccess::tick(&mut kind);
        }

        let starts_after = kind.cpu_instruction_starts();
        let crossed = starts_after.wrapping_sub(starts_before);
        assert!(crossed > 0, "blank A1200 firmware should execute");
        assert_eq!(kind.cpu_trace_entries().len() as u64, crossed);
    }

    /// Armed: per-tick stepping (the `step` / `run_until_*` path)
    /// captures instruction boundaries up to the cap, then stops.
    #[test]
    fn armed_per_tick_stepping_captures_up_to_cap() {
        let mut kind = AmigaRuntimeKind::blank(Model::A500OcsPal);
        kind.cpu_trace_arm(None, 4);
        assert!(kind.cpu_trace_armed());
        for _ in 0..2000 {
            AmigaLiveAccess::tick(&mut kind);
        }
        assert!(
            kind.cpu_trace_entries().len() <= 4,
            "respects the max-entries cap"
        );
    }

    /// The shared `run_until` loop (what the `run_frames` shell tool
    /// drives) feeds the same armed trace — the whole point of moving
    /// the trace into the runtime.
    #[test]
    fn run_until_loop_feeds_the_armed_trace() {
        let mut kind = AmigaRuntimeKind::blank(Model::A500OcsPal);
        kind.cpu_trace_arm(None, 1_000);
        let mut frame_sink = NullFrameSink;
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        kind.run_until(MachineTime::new(A500_PAL_FRAME_TICKS), &mut host)
            .expect("one frame runs");
        // A blank ROM's reset vector still executes *some* instructions
        // before it settles, so the run captured at least one boundary.
        assert!(
            !kind.cpu_trace_entries().is_empty(),
            "run_until fed the trace"
        );
        // Disarm returns the held count; clear empties it.
        let held = kind.cpu_trace_disarm();
        assert_eq!(held, kind.cpu_trace_entries().len());
        let dropped = kind.cpu_trace_clear();
        assert_eq!(held, dropped);
        assert!(kind.cpu_trace_entries().is_empty());
    }
}
