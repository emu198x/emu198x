# Brainstorm: unify the debug verbs across MCP and `--script`

**Date:** 2026-06-11
**Status:** Design proposal — react before implementation (RULES.md #31).
**Context:** Follows the #456 debug-tier work and RULES.md #30 ("promote
cross-machine functionality up"). Sits beside
[`../../knowledge/decisions/debug-surface-tiers.md`](../../knowledge/decisions/debug-surface-tiers.md).

## The problem

The debug verbs (`query_cpu`, `memory_read`, `disasm`, `step`, `poke_*`,
`run_until_pc`) have **two implementations** in the shell, and they can drift:

| | MCP | `--script` |
|---|---|---|
| Mechanism | `DebugTool` (`mcp_tools.rs::register_debug_tools`) | `ScriptStep` → `execute_collect` |
| Output | ad-hoc `serde_json::Value` (u32, string PCs, `ticks`) | typed `ScriptObservation` (u16 PCs, `halfcycles`, `halt`) |
| Completeness | full + generic via `DebugTarget` | **stubs** — every arm but `MemoryRead` returns `SystemSpecificStep` |

The non-debug verbs (run_frames, media, snapshot, query…) are *already*
unified: `register_common_tools` registers them as `ScriptStepTool`-wrapped
`ScriptStep`s, so MCP and script share one `execute_collect` body. The debug
verbs were never moved onto that mechanism — instead the **Spectrum**
intercepts the stub arms (`dispatch_live_step`) and implements them itself.
So "the script debug path" is really "the Spectrum's path"; no other
machine's script can `disasm`/`step` today.

**Goal:** one implementation per verb, serving MCP *and* `--script`, for
every machine — the way the common verbs already work.

## The target architecture

1. Implement the six stub arms in `ScriptStep::execute_collect` **generically
   via `DebugTarget`** (port the existing `DebugTool` `run_*` bodies, which are
   already generic).
2. Register the debug verbs as `ScriptStepTool` in `register_debug_tools`
   (exactly like `register_common_tools` does for the common verbs).
3. **Delete the `DebugTool` path** (struct + `run_*` fns).
4. Unwind the Spectrum's interception of the now-shell-handled verbs, keeping
   only genuinely Z80/AY-specific ones (`port_read/write`, `watch_ay_*`).

The blocker is step 1: the typed `ScriptObservation` debug shapes are
Spectrum/Z80/16-bit and don't fit a 6502 or a 68000. So the redesign below
must land first.

## Decision points

### D1 — Unify on the typed `ScriptObservation` (recommended)

MCP JSON becomes the *serialised* `ScriptObservation` (how the common tools
already behave). One shape feeds both the MCP response and the script report.
The alternative — keep ad-hoc JSON and have the script path emit it — throws
away the typed-observation model the report is built on. **Recommend: typed.**

### D2 — PC width: widen to `u32`

`ScriptObservation::Step.pc` / `RunUntilPc.pc` are `u16` — can't hold the
Amiga's 32-bit PC. Widen to `u32`, serialised as a hex string. Precedent
exists: `DisasmInstruction.addr` is already `u32`, and `WatchMemoryLog` was
"widened to u32 … covers both 16-bit (Z80, 6502) and 32-bit (68000)".
**Recommend: `pc: u32`.**

### D3 — `halfcycles` → `ticks: u64`

`halfcycles: u32` is the Spectrum's time unit. The generic quantity is "ticks"
— whatever `DebugTarget::step_instruction` returns (master half-cycles on the
Spectrum, master ticks on NES/Amiga), summed over the step. Widen to `u64`
(Amiga instruction ticks can exceed `u32` over long runs). Document that the
unit is **machine-native**. **Recommend: rename to `ticks: u64`.**

### D4 — Drop `halt` from the generic `Step`

`halt: bool` is a Z80 concept (the 6502 jams via `KIL`; the 68000 has `STOP`).
The Spectrum's `cpu_state` already reports `halt`
(`runtime-sinclair-zx-spectrum/src/debug.rs:84`), so it's reachable via
`query_cpu` — no information lost. CPU-specific status flags belong in
`cpu_state`, not in the shared `Step`. **Recommend: drop `halt`; it lives in
`query_cpu`.**

### D5 — `query_cpu` observation carries a generic `Value`

`ScriptObservation::QueryCpu` is today a fully Z80-specific struct (named
index registers, flag bits, …). The generic `DebugTarget::cpu_state` returns a
machine-shaped `serde_json::Value` (Z80 / 6502 / 68000 each emit their own
fields). The observation must carry that `Value` (e.g. `QueryCpu { registers:
Value }`) rather than a fixed register list. **Recommend: `QueryCpu {
registers: Value }`** — the per-CPU shape stays in `cpu_state`.

### D6 — `disasm` is already generic — keep it

`Disasm { addr, count, instructions: Vec<DisasmInstruction> }` with
`DisasmInstruction { addr: u32, bytes: u8 /*len*/, raw: Vec<u8>, mnemonic }`
generalises cleanly. The unified MCP `disasm` JSON becomes this (note: it
*differs* from the current `DebugTool` `{lines:[{addr,text,bytes,len}]}` —
`mnemonic`/`raw` vs `text`/`bytes`). One shape, fleet-wide. **Recommend: keep
`DisasmInstruction`; the Part A `DebugTool` enrichment is superseded by it.**

### D7 — New verbs `run_until_any_pc` / `run_until_mem_change`

Add as `ScriptStep`s + generic observations (single watched address;
`{addr, changed, old, new, ticks, steps, cpu_pc}`). The Amiga's *richer*
multi-address, tracer-based `run_until_mem_change` is a genuinely different
mechanism (chipset write-watch, not step+peek) — it **stays** as an Amiga
override (converge up, don't flatten richness). **Recommend: generic
single-address shared; Amiga keeps its multi-address tracer version.**

### D8 — `io_trace` stays special (for now)

`io_trace` is port-mapped and gated on `supports_io_trace()` (Z80/6502 I/O
only); it has no `ScriptStep` and no meaning on the 68000. Leave it as the one
debug tool *not* on the `ScriptStep` path until there's a reason to move it.
**Recommend: defer.**

## Migration / contract impact

- **Script report format changes** (`RunnerReport.observations`): `Step` /
  `RunUntilPc` gain `u32` PCs + `ticks`, lose `halfcycles` / `halt`; `QueryCpu`
  becomes `{ registers: <value> }`. **Code198x report parsers may read these
  — audit consumers before landing.** This is the highest-risk item.
- **MCP debug-tool JSON changes fleet-wide**: ad-hoc → serialised observation
  (e.g. `disasm` → `mnemonic`/`raw`; `step` → typed). No in-repo dependents
  found, but it's a published agent-facing surface.
- **Spectrum**: its `disasm`/`step`/`run_until_pc` interception becomes
  redundant once the shell handles them generically (it produced the same
  `ScriptObservation::Disasm`); `port_*` / `watch_ay_*` stay. `step` loses
  `halt` from its body — already in `cpu_state`.
- **Part A**: the `DebugTool` enrichment (disasm `bytes`/`len`, step
  `pc_trace`) is superseded — its behaviours move to the typed observations
  (`pc_trace` added to `Step`; `raw`/`bytes` already in `DisasmInstruction`).
  Revert it as part of the `DebugTool` deletion.

## Proposed commit sequence (once aligned)

1. **Generic observation redesign** — widen `Step`/`RunUntilPc` to `u32` +
   `ticks`, drop `halt`; `QueryCpu { registers: Value }`; add `pc_trace` to
   `Step`; add `RunUntilAnyPc` / `RunUntilMemChange` steps + observations.
   Implement the six arms in `execute_collect` via `DebugTarget`. Shell tests
   via the `DummyMachine` `DebugPrimitives` impl. *(Additive: arms that erred
   now work; nothing breaks.)*
2. **Register-as-`ScriptStepTool` + delete `DebugTool`** — `register_debug_tools`
   wraps the debug `ScriptStep`s; delete the `DebugTool` struct + `run_*` +
   Part A. Rewrite the C64 `shared_debug_tools` test for the unified shapes.
   *(Fleet-wide MCP JSON change.)*
3. **Per-system reconcile** — drop the Spectrum's redundant interception (keep
   Z80/AY-specific); drop the Amiga `step` / `run_until_any_pc` overrides (keep
   the tracer-based `run_until_mem_change` + chip/exec tools).

## Open questions for Steve

1. **Code198x impact (D-migration)** — do any Code198x report parsers consume
   `Step.halfcycles` / `Step.halt` / the Z80-shaped `QueryCpu` observation? If
   yes, we version the report or keep a compat shim; if no, we change freely.
2. **`ticks` naming/unit** — `ticks` as a machine-native unit OK, or do you
   want a normalised unit (e.g. always master-clock) across machines?
3. **D7** — happy for the Amiga to keep a *richer* multi-address
   `run_until_mem_change` override, or force everything onto the single-address
   shared shape for strict uniformity?
4. **Scope** — land all three commits in one push, or stop after #1 (generic
   arms, additive, no contract break) to de-risk and validate before the
   fleet-wide MCP change in #2?
