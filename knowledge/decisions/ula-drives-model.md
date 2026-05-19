# Decision: ULA-drives model

**Date**: April 2026 (architecture revision)
**Supersedes**: CPU-drives model from original codebase

## The decision

The ULA (or equivalent video/timing chip) ticks every half-cycle and gates the CPU clock. The CPU is a passive signal-level state machine that only advances when the ULA allows it. Contention is implicit — the CPU doesn't tick when the ULA withholds the clock.

## Why

The original codebase ticked at CPU frequency and bolted contention on as an afterthought. Every accuracy improvement was a risky retrofit. [Signal Part 3](../systems/spectrum/signal-part-3.md) exposed all of this — its interrupt handler only works when contention is cycle-perfect.

The ULA-drives model makes contention a natural consequence of the architecture rather than a separate system to maintain. When the ULA owns the clock, contention "just works" — the CPU doesn't tick, so it doesn't advance.

## How it works

```
while hc < frame_hc {
    ula.tick(memory, z80.addr, z80.mreq, z80.iorq, framebuffer);
    if ula.cpu_clock_active() {
        z80.tick();
        handle_bus();
    }
    z80.irq = ula.interrupt_active();
    hc += 1;
}
```

## Drift triggers

The patterns below have caused real regressions on this project. If I (any LLM working on this codebase) catch myself about to write or propose one of them, **stop and re-read this entry before continuing.** These are not hypothetical — they are transcribed from actual prior failures, which is why the rewrite happened.

**Code patterns:**

- `for _ in 0..tstates_per_frame { cpu.step() }` — or any framing where the CPU loop is the *outer* loop
- `for _ in 0..instructions_per_frame { ... }` — per-instruction frame loops
- `cpu.step_n(cycles)` followed by `ula.run_for(cycles)` — run-then-catch-up
- `cpu.tick(base_cycles + contention_cycles)` — adding contention as extra ticks rather than as a withheld clock from the ULA
- `while cpu.tstates < target { cpu.step() }` — the CPU as outer loop
- Calling `ula.tick()` from inside CPU code rather than the CPU being subordinate to the outer ULA-driven loop
- Any per-instruction emulation in the hot path (the Z80 is a half-cycle signal-level state machine — see RULES.md rule 5)

**Phrases that signal drift in conversation:**

- "Let's step the CPU and then tick the ULA"
- "Run the CPU for N cycles, then catch up the rest"
- "Add contention as extra wait states"
- "We can iterate over instructions for speed"
- "Bolt on contention" — this is literally what was wrong before
- "Add accuracy later" / "fix the timing in a second pass"
- "Catch up the ULA / peripherals / framebuffer"
- "The CPU drives the loop" in any framing

**Architectural framings to reject:**

- Treating the CPU as the orchestrator of the emulation loop
- Modeling contention as tick *injection* rather than tick *withholding*
- Any "fast path" that skips the per-half-cycle ULA tick
- Any framing where the ULA "runs for N cycles" called from CPU code
- Per-instruction Z80 abstraction anywhere on the hot path

**If you catch yourself proposing one of these:** stop, name it explicitly to the user (e.g. *"I think this would put us back in the CPU-drives model — is that intentional?"*), and propose updating this entry rather than silently going the other way. Multiple previous sessions burned on exactly these regressions, which is why the rewrite happened. Re-litigating them is the highest-cost failure mode on this project.

## Cross-system applicability

This model generalises. The "ULA" is whatever chip owns timing in each system:
- **Spectrum**: ULA/gate array
- **C64**: VIC-II (BA line gates CPU)
- **Amiga**: Agnus (bus arbitration)
- **NES**: PPU drives timing

The principle holds: the timing chip drives the loop, the CPU is subordinate.

## Related

- [No Bus Trait](no-bus-trait.md) — the CPU exposes signals, not method calls
- [Half-cycle signals](half-cycle-signals.md) — why half-cycle granularity matters
- [Fresh start rationale](fresh-start-rationale.md) — why the old approach was abandoned
