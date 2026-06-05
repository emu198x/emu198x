# Decision: Debug-surface tiers — shared `DebugTarget` vs bespoke per-flagship MCP

**Date:** 2026-06-04
**Status:** **REVISED 2026-06-05.** The original two-tier deferral stands as the
*record of why* Amiga was bespoke, but the verdict is now overridden by an
explicit owner decision (Steve) to fold Amiga onto the shared tier *now* —
building the 68000 debug path early rather than waiting for the first 68000
sibling. See the **2026-06-05 override** in the Log and the migration plan at
[`../../docs/plans/2026-06-05-refactor-amiga-unified-driver-replatform.md`](../../docs/plans/2026-06-05-refactor-amiga-unified-driver-replatform.md).
The drift-trigger guidance below is **suspended for the Amiga 68000 work** (it
remains live for any *other* "tidy the flagships onto the macros" impulse).
Sits beside [`runtime-internal-shape.md`](runtime-internal-shape.md) and
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

- **Amiga stays bespoke *for now* — but the blocker is temporary.** The hard
  reason is the missing 68000 member of the macro family, and **more 68000
  systems are coming** (cpu-family `68000` in the catalogue: **Atari ST**, **Sega
  Mega Drive / Genesis**, **SNK Neo Geo**, **Sharp X68000**). The *first* of those
  to land is the trigger to build `impl_68000_debug_target!` once — and that
  retroactively lets Amiga opt into the shared tier (it would still keep its
  bespoke copper/blitter/chipset tools on top). So this is not "Amiga is special
  forever"; it is "Amiga waits for the 68000 debug path the next 68000 system will
  pay for." Don't fold Amiga in *before* that path exists; don't *re-derive* the
  gap when it does — build the macro member with the first new 68000 system and
  wire Amiga in the same pass.
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
- **"Give Amiga a `DebugTarget` for uniformity."** Not standalone, and not yet:
  it needs a 68000 member of the macro family first. Build that member *with the
  first new 68000 system* (Atari ST / Mega Drive / Neo Geo / X68000) and wire
  Amiga in the same pass — see the Verdict. Doing it before that path exists is
  premature; re-deriving the gap when it does is wasted motion.

## Log

### 2026-06-04 — Recorded

Surfaced while reviewing the debug-target macros after they were made
storage-agnostic (Emu198x `2cbd04fc`) and the C64/Dragon/VIC-20/MSX hand-rolled
impls were collapsed onto them. The question "why are Spectrum and Amiga still
special?" turned out to have a real answer worth writing down: they are a
distinct bespoke-MCP tier, blocked from the shared tier by generics, the missing
68000 macro, and the `SpectrumDriver` shape — not leftover cruft. Documented the
tier instead of refactoring. Amiga waits for the 68000 debug path the incoming
68000 systems (Atari ST, Mega Drive, Neo Geo, X68000) will pay for; Spectrum is a
deliberate future spike.

### 2026-06-05 — Override: fold Amiga onto the shared tier now

Owner decision (Steve), made after a fleet-wide drivability assessment
(`docs/status/drivability-assessment.md`). The assessment found the Amiga MCP is
a **parallel** implementation (`AmigaSession` + `InlineTool`) of infrastructure
the Amiga **script** path already runs on the shared `HeadlessSession`
(`AmigaRuntimeKind: MachineCore`, `AmigaSessionQueryProvider`). Consequences of
the divergence: the Amiga cannot be driven by keyboard or mouse over MCP at all,
and recording/common verbs differ from the rest of the fleet.

**Decision:** prioritise a single common driver surface across all machines over
the cost-deferral this record originally weighed. Build the 68000 debug path
*now* rather than waiting for the first 68000 sibling, and fold Amiga onto the
shared session + `register_common_tools` + `register_debug_tools`.

**Why override the 2026-06-04 deferral:** the deferral optimised for *not paying*
for `impl_68000_debug_target!` ahead of the sibling that would share its cost.
The owner judges fleet-wide uniformity (drive every machine identically through
either door) worth paying early — and the 68000 work is not wasted: the incoming
68000 systems (Atari ST / Mega Drive / Neo Geo / X68000) will reuse the same
macro + the `u32`-widened `DebugTarget` this pass builds.

**Real cost discovered (all four original obstacles bite):**
1. `DebugTarget` is `u16`-addressed; the 68000 needs `u32`. Widen the trait,
   the three `impl_*_debug_target!` macros, the shared tool arg-parsing, and
   `IoEvent.pc` — rippling through all 24 existing shared-tier machines
   (mechanically, via the three macros).
2. No `impl_68000_debug_target!` — build it, wrapping
   `motorola_68000::disasm::disassemble` and `motorola-68k-common::Registers`
   (`d[u32;8]`, `a[u32;7]`, `pc:u32`, `sr:u16`).
3. `AmigaRuntime<M>` is generic; the macros emit non-generic impls. Implement on
   the non-generic `AmigaRuntimeKind` enum (which already impls `MachineCore`).
4. The 68k variant family (68000 for OCS/ECS, 68020 for AGA) must be spanned by
   one impl through the `AmigaMachine` CPU accessor.

Sequenced in `docs/plans/2026-06-05-refactor-amiga-unified-driver-replatform.md`. Spectrum's
fold-in remains a *separate, unscheduled* spike — this override covers Amiga
only.
