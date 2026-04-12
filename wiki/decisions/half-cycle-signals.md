# Decision: Half-cycle Signal Granularity

**Date**: April 2026 (fresh start)

## The decision

The Z80 advances one half-cycle per `tick()` call. States follow the pattern `M1_T1_Rise` → `M1_T1_Fall` → `M1_T2_Rise` → etc. The ULA ticks every half-cycle. The CPU ticks only on the appropriate edge.

## Why

Real Z80 signals change on clock edges. Some signals assert on the rising edge, others on the falling edge. Half-cycle granularity lets the emulator match this exactly. The ULA needs to see signals at half-cycle resolution to gate the CPU clock correctly — contention decisions happen between CPU clock edges.

The alternative (full T-state granularity) would require the ULA to predict what the CPU will do, rather than reacting to what it's actually doing.

## Implications

- The master oscillator counter `hc` increments every half-cycle
- ULA renders pixels at half-cycle resolution (2 pixels per T-state on Spectrum)
- CPU state machine has twice as many states as a T-state-level implementation
- More states, but each state does less — simpler per-state logic

## Drift triggers

If I catch myself proposing any of these, stop and re-read the "Why" section above.

**Code patterns to reject:**

- `fn tick(&mut self)` on the Z80 that advances a full T-state (should be half a cycle)
- State machine without `_Rise`/`_Fall` variants: `M1_T1`, `M1_T2`, `M2_T1`, ...
- Incrementing `hc` by 2 per Z80 tick (defeats the half-cycle purpose)
- ULA sampling CPU signals at T-state boundaries rather than half-cycle boundaries
- Any `match` on Z80 states that only covers rising-edge variants

**Phrases that signal drift:**

- "Half-cycle is overkill for this"
- "Let's aggregate the rising and falling edges into one tick"
- "Full-cycle granularity is easier to reason about"
- "We can simplify the Z80 state machine by merging half-states"
- "Per-T-state is fine, the ULA can just use the post-state signals"
- "Twice as many states means twice as slow"

**What to do when triggered:** the alternative — full-T-state granularity — would require the ULA to *predict* what the CPU will do, not react to it. That breaks the [ULA-drives model](ula-drives-model.md). Simplifying here means rewriting contention, the ULA clock-gating logic, and every timing-sensitive test. More states but simpler per-state logic is the deliberate trade; don't undo it.

## Related

- [ULA-drives model](ula-drives-model.md) — the loop that ticks at half-cycle
- [Z80](../chips/zilog-z80.md) — the half-cycle state machine
