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

### D7 — New verbs `run_until_any_pc` / `run_until_mem_change` (RESOLVED: converge up)

Add as `ScriptStep`s + generic observations. **The shared
`run_until_mem_change` watches a *list* of addresses** — the Amiga's richer
multi-address design is *elevated to the shared tier* (step+peek each watched
address per instruction, report which changed) so every machine gets it.
Observation: `{ addrs, changed, changed_addr, old, new, ticks, steps, pc }`.
The Amiga's tracer-based impl (chipset write-watch, catches intra-instruction
writes step+peek can miss) is evaluated in commit 3 — kept as a more-accurate
override only if that extra accuracy matters, otherwise dropped for the shared
one. `run_until_any_pc`: `{ targets }` → `{ reached, pc, ticks, steps }`.

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

## Resolved with Steve (2026-06-11)

1. **Code198x impact — CLEAR TO CHANGE FREELY.** Audited
   `~/Projects/198x/Code198x`: scripts use only `run_frames` / `input` /
   `poke_byte` / `type_string` / media / capture actions — *no* `disasm` /
   `step` / `query_cpu` / `run_until*` / `memory_read`. `code-samples/_capture/
   capture.py` reads `observations` but filters only on `kind ∈
   {stop_audio_recording, stop_video_recording}` — it never touches a debug
   observation. No compat shim needed; redesign the debug observation shapes
   freely. `poke_byte` *input* stays unchanged, so the 10 scripts using it are
   safe.
2. **`ticks` unit — machine-native.** `ticks: u64`, documented as the
   machine's own unit (Spectrum master half-cycles, NES/Amiga master ticks).
   No cross-machine normalisation.
3. **D7 — converge UP: the shared `run_until_mem_change` is multi-address.**
   Elevate the Amiga's richer multi-address watch to the shared tier (every
   machine gets it), don't keep it as an Amiga-only override. Shared shape
   watches a list of addresses via step+peek; commit 3 decides whether the
   Amiga's tracer-based impl is kept for intra-instruction-write accuracy or
   dropped for the shared one.
4. **Scope — all three commits in one go.**
