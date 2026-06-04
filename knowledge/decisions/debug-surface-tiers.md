# Decision: Debug-surface tiers — shared `DebugTarget` vs bespoke per-flagship MCP

**Date:** 2026-06-04
**Status:** Descriptive-binding. Records an existing two-tier shape so the
asymmetry reads as deliberate, not as an artifact to "tidy away". Sits beside
[`runtime-internal-shape.md`](runtime-internal-shape.md) and
[`debugger-architecture.md`](debugger-architecture.md) (the latter is the future
debugger *UI*; this is about the *runtime* debug surface the UI will consume).

## What this is

There are **two tiers** of runtime debug surface, and a system sits on one:

1. **Shared tier.** The runtime implements the [`DebugTarget`] trait
   (`emu198x-shell/src/debug.rs`) — `pc`/`peek`/`poke`/`cpu_state`/`disassemble`/
   `step_instruction`/io-trace — almost always via the
   `impl_{6502,z80,6809}_debug_target!` macros. The per-system binary then calls
   `emu198x_shell::mcp_tools::register_common_tools::<M, Q>()`, which registers
   the *common* MCP/script verbs (`disasm`, `step`, `cpu_state`, `memory_read`,
   `run_frames`, …) on top of `DebugTarget`. Every such system gets those verbs
   for free and identically, plus whatever bespoke tools its binary adds.
   **Members:** the 24 donor extractions, plus C64 and Dragon.

2. **Bespoke tier.** The runtime does **not** implement `DebugTarget` (it falls
   through to the default `debug_target() -> None`). The per-system binary's
   `src/mcp/tools.rs` hand-implements its debug verbs, richer and broader than the
   common set. **Members:** Spectrum and Amiga — the two flagship launch systems.

## Why the flagships are bespoke (not an artifact, not laziness)

Part historical, part structural. All four reasons hold; any one is sufficient.

- **They predate the abstraction.** Spectrum and Amiga shipped first, with
  hand-built MCP servers, *before* `DebugTarget` + `register_common_tools`
  existed. Those servers are a **superset** of the common verbs (Spectrum: AY,
  tape, snapshots, keyboard; Amiga: copper, blitter, chipset, exec tasks,
  libraries — dozens of tools). Folding them onto the shared tier would add a
  *lesser parallel* surface, not remove one.
- **The runtimes are generic.** `SpectrumRuntime<M: SpectrumMachine>` and
  `AmigaRuntime<M: AmigaMachine>`. The macros emit a non-generic
  `impl DebugTarget for $ty`; they cannot express
  `impl<M: Bound> DebugTarget for Runtime<M>`. (Fixable with a generic macro arm,
  but only worth it for Spectrum — see below.)
- **Amiga is 68000.** The `DebugTarget` macro family is 6502 / Z80 / 6809 only;
  there is **no** `impl_68000_debug_target!`. Amiga cannot use the family without
  first building a whole 68000 debug path — for a lowest-common-denominator
  surface beside its richer bespoke one. Net negative.
- **The machine trait shape differs.** `SpectrumDriver` is a half-cycle *timing*
  trait (`hc`, `frame_hc`, `halfcycles_per_tstate`), not the
  `cpu()/peek/poke/step_instruction` shape the macros consume. Spectrum would
  need an adapter even with a generic macro arm.

## Verdict (the part that's binding)

- **Amiga stays bespoke.** No 68000 macro, richer surface; folding in is
  downgrade-plus-work. Do not attempt without a separate, deliberate decision.
- **Spectrum *could* fold in** (it is Z80) via a generic macro arm + a
  `SpectrumDriver`→macro adapter, which would let it call
  `register_common_tools` and delete its hand-rolled *common* verbs (keeping the
  bespoke extras). This is real dedup but real work on the most-tested launch
  system, and it fights the variant/orphan-rule friction already seen in the
  Z80-stepper trait (see the stepper history). **Not scheduled.** Pursue only as
  its own task with a feasibility spike first.

## Drift triggers

If about to suggest any of these, stop and re-read this record.

- **"Spectrum/Amiga are inconsistent — let's tidy them onto the debug macros."**
  No. The asymmetry is a two-tier design: shared `DebugTarget` for the
  macro-shaped systems, bespoke MCP for the flagships. Amiga *can't* (68000);
  Spectrum *can* but only as a deliberate, spiked piece of work, not a sweep.
- **"Add a `direct` arm so the macro covers the generic runtimes."** The blocker
  for Spectrum/Amiga is generics + (for Amiga) the missing 68000 path, not
  storage shape. A `direct` arm alone doesn't reach them.
- **"Give Amiga a `DebugTarget` for uniformity."** It needs a 68000 member of
  the macro family first, and even then duplicates its richer bespoke tools.
  Separate decision required.

## Log

### 2026-06-04 — Recorded

Surfaced while reviewing the debug-target macros after they were made
storage-agnostic (Emu198x `2cbd04fc`) and the C64/Dragon/VIC-20/MSX hand-rolled
impls were collapsed onto them. The question "why are Spectrum and Amiga still
special?" turned out to have a real answer worth writing down: they are a
distinct bespoke-MCP tier, blocked from the shared tier by generics, the missing
68000 macro, and the `SpectrumDriver` shape — not leftover cruft. Chose to
document the tier rather than refactor; Amiga is out, Spectrum is a deliberate
future spike.
