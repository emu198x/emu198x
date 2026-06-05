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
//! Capture overhead when disarmed is one `bool` check per tick; when
//! armed it's a `cpu_instruction_starts` compare + optional PC filter +
//! `Vec` push at each instruction boundary the tick crosses.

use crate::live_access::AmigaLiveAccess;
use crate::runtime::AmigaRuntime;
use crate::variants::AmigaMachine;

/// One captured instruction-boundary CPU snapshot:
/// `(tick_count, instr_start_pc, sr, opcode_word)`. `opcode_word` is the
/// 16-bit word at `instr_start_pc`, read through the live-access surface
/// at capture time.
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
    /// Last observed `cpu_instruction_starts`; capture fires when this
    /// changes between `tick_traced` calls.
    last_seen_instr_starts: u64,
}

impl Default for CpuTrace {
    fn default() -> Self {
        Self {
            armed: false,
            pc_filter: None,
            max_entries: DEFAULT_MAX_ENTRIES,
            entries: Vec::new(),
            last_seen_instr_starts: 0,
        }
    }
}

impl CpuTrace {
    /// Drop captured entries and re-baseline the instruction counter.
    /// Called from `MachineCore::reset` so pre-reset entries don't bleed
    /// into post-reset analysis. Arm-state and filter are kept — re-arming
    /// after every reset would be tedious for a debugging session.
    pub(crate) fn clear_on_reset(&mut self) {
        self.entries.clear();
        self.last_seen_instr_starts = 0;
    }
}

// The trace control + capture surface needs to read CPU state and memory
// off the wrapped machine, so it's bounded on `AmigaLiveAccess` (every
// concrete Amiga machine implements it). The bound stays off the bulk
// `impl<M: AmigaMachine>` blocks in `runtime.rs` to avoid cascading it
// onto the query / snapshot siblings.
impl<M: AmigaMachine + AmigaLiveAccess> AmigaRuntime<M> {
    /// Advance one master/4 tick and, if the trace is armed, capture a
    /// snapshot at every instruction boundary the tick crosses. The
    /// single tick funnel both the `run_until` loop and the per-tick
    /// stepping tools route through.
    pub(crate) fn tick_traced(&mut self) {
        let prev = self.cpu_trace.last_seen_instr_starts;
        // Disambiguate: `AmigaMachine::tick` and `AmigaLiveAccess::tick`
        // are both in scope under this bound. We want the machine's own
        // clock tick.
        AmigaMachine::tick(&mut self.machine);
        if !self.cpu_trace.armed {
            return;
        }
        let now = AmigaLiveAccess::cpu_instruction_starts(&self.machine);
        if now == prev {
            return;
        }
        self.cpu_trace.last_seen_instr_starts = now;
        let snap = AmigaLiveAccess::cpu_snapshot(&self.machine);
        let pc = snap.instr_start_pc;
        if let Some((lo, hi)) = self.cpu_trace.pc_filter
            && (pc < lo || pc > hi)
        {
            return;
        }
        if self.cpu_trace.entries.len() >= self.cpu_trace.max_entries {
            return;
        }
        let opcode = AmigaLiveAccess::read_word(&self.machine, pc);
        let tick_count = AmigaLiveAccess::tick_count(&self.machine);
        self.cpu_trace
            .entries
            .push((tick_count, pc, snap.regs.sr, opcode));
    }

    /// Start recording. Clears any prior trace and replaces the filter +
    /// cap. Re-baselines the instruction counter so the first captured
    /// boundary is the next one crossed, not a stale delta.
    pub fn cpu_trace_arm(&mut self, pc_filter: Option<(u32, u32)>, max_entries: usize) {
        self.cpu_trace.armed = true;
        self.cpu_trace.pc_filter = pc_filter;
        self.cpu_trace.max_entries = max_entries;
        self.cpu_trace.entries.clear();
        self.cpu_trace.last_seen_instr_starts =
            AmigaLiveAccess::cpu_instruction_starts(&self.machine);
    }

    /// Stop recording; keep the captured entries. Returns the entry
    /// count at the moment of disarm.
    pub fn cpu_trace_disarm(&mut self) -> usize {
        self.cpu_trace.armed = false;
        self.cpu_trace.entries.len()
    }

    /// Discard captured entries without disarming. Re-baselines the
    /// counter so the next window starts fresh. Returns how many were
    /// dropped.
    pub fn cpu_trace_clear(&mut self) -> usize {
        let dropped = self.cpu_trace.entries.len();
        self.cpu_trace.entries.clear();
        self.cpu_trace.last_seen_instr_starts =
            AmigaLiveAccess::cpu_instruction_starts(&self.machine);
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
    use crate::live_access::AmigaLiveAccess;
    use crate::variants::AmigaRuntimeKind;
    use crate::{Model, profiles::A500_PAL_FRAME_TICKS};
    use emu198x_shell::{
        HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
    };

    /// Disarmed: per-tick stepping captures nothing, however many
    /// instruction boundaries it crosses.
    #[test]
    fn disarmed_trace_captures_nothing() {
        let mut kind = AmigaRuntimeKind::blank(Model::A500OcsPal);
        assert!(!kind.cpu_trace_armed());
        for _ in 0..500 {
            AmigaLiveAccess::tick(&mut kind);
        }
        assert!(kind.cpu_trace_entries().is_empty());
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
