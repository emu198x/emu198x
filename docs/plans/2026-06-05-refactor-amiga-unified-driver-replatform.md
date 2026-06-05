# Plan: Unified driver surface — fold Amiga onto the shared session + 68000 debug

**Date:** 2026-06-05
**Type:** Refactor (flagship). Owner-approved override of
[`knowledge/decisions/debug-surface-tiers.md`](../../knowledge/decisions/debug-surface-tiers.md)
(see its 2026-06-05 Log entry).
**Goal:** every machine is driven the *same way* through either door — `--script`
and `--mcp` expose the same verb vocabulary, built on the shared
`HeadlessSession` + `register_common_tools` + `register_debug_tools`. Amiga is the
last machine on a parallel path; this folds it in.

## Progress (live)

| Phase | State |
|---|---|
| 1 — Widen `DebugTarget` to `u32` | ✅ done (`refactor(shell): widen DebugTarget addresses to u32`) |
| 2 — Amiga `DebugTarget` | ✅ done — **via `DebugPrimitives`, not the planned `impl_68000_debug_target!` macro** (see below) |
| 2b — Fleet onto `DebugPrimitives` | ✅ done (`refactor(shell): one debug pattern`) — emergent from the Phase-2 spike |
| 3 — Amiga MCP onto `HeadlessSession` | ✅ done — foundation (`test(amiga): prove the shared HeadlessSession MCP path`) then production cutover (`cut the MCP server over to HeadlessSession`). `mcp/mod.rs::run` builds the shared session and registers `register_common_tools` + `register_debug_tools` + `register_amiga_tools`. |
| 4 — Port bespoke Amiga tools | ✅ done — every tool generic over `AmigaCtx`; `CpuTraceState` relocated into the runtime (`own the CPU instruction trace`); `query_aga` via a new `AmigaLiveAccess::aga_lisa()` accessor with the AGA downcast deleted; `register_amiga_tools<C: AmigaCtx>` (`generic register_amiga_tools`). |
| 5 — Retire `AmigaSession` | ✅ done — deleted with the cutover. `mcp/session.rs`, `register_all`, the five session-local tools + recorder plumbing all gone; recording/run/reset come from the shared shell. Verified by `mcp_smoke.rs` against a real KS 3.1 A1200 ROM. |

**Replatform complete (2026-06-05).** The Amiga is driven through the
identical door as every other machine: `--script` and `--mcp` both run the
shared `HeadlessSession` over `AmigaRuntimeKind`, with `input`
(keyboard/mouse over MCP) now present. The only remaining sibling is the
Spectrum (Phase 6, out of scope here).

**What changed from the original plan:** Phase 2 was written as "build
`impl_68000_debug_target!`". The spike for it found a better shape: a
`DebugPrimitives` adapter trait + a single blanket
`impl<T: DebugPrimitives> DebugTarget for T` in the shell. The Amiga hand-
implements `DebugPrimitives` (delegating to its existing `AmigaLiveAccess`
adapter) and gets `DebugTarget` for free — no 68000 *macro* needed. The blanket
impl turned out to coexist with the legacy per-CPU macro impls (the orphan rule
makes it legal), so the win generalised: the three `impl_*_debug_target!` macros
were converted to emit `DebugPrimitives` and renamed `impl_*_debug_primitives!`,
putting the **whole fleet** on one `DebugTarget`-via-blanket pattern. Spectrum is
now the only bespoke holdout. Net: the debug half of the unification is *done and
better than planned*; what remains (Phases 3–5) is wiring the Amiga's MCP server
onto the shared session so it actually *uses* that `DebugTarget` plus the shared
input/capture/common tools.

## Why

`docs/status/drivability-assessment.md` found the Amiga MCP server is a parallel
implementation (`AmigaSession` + `InlineTool`) of infrastructure the Amiga
**script** path already runs on the shared `HeadlessSession`
(`AmigaRuntimeKind: MachineCore`, `AmigaSessionQueryProvider`). The divergence
costs real capability: **no keyboard or mouse over MCP**, recording/common verbs
that differ from the fleet, and a second debug vocabulary. Folding Amiga onto the
shared session retires the duplicate and lights up uniform input/capture/media +
the shared debug suite.

## End state (definition of done)

- Amiga MCP runs on `HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>`
  — the same session the script path builds.
- `register_common_tools` + `register_debug_tools` provide the common verbs;
  Amiga's bespoke copper/blitter/chipset/exec/library/cpu-trace tools layer on
  top as `Tool<HeadlessSession<…>>`.
- `AmigaSession` + the `InlineTool` scaffold are deleted.
- `impl_68000_debug_target!` exists and is reused-ready for Atari ST / Mega Drive
  / Neo Geo / X68000.
- **No regression:** Workbench 1.3/2.0/3.x still boots; Tom Harte 68k stays
  100%; every existing shared-tier machine's debug verbs still work.

## Verification anchors (run at every phase gate)

- `cargo fmt --all && cargo clippy --workspace` clean.
- `cargo test -p emu198x-shell` (133+).
- A representative shared-tier machine still debugs (e.g. jupiter-ace `step` +
  `memory_read` over MCP) — guards the `u32` widening.
- Amiga boots: existing Amiga boot/script smoke (Workbench framebuffer).
- Tom Harte 68k regression unaffected (CPU cores untouched by this work).

## Phases

### Phase 1 — Widen `DebugTarget` to `u32` addresses (shared, foundational) ✅ DONE

The 68000 has a 24-bit bus / 32-bit PC; the trait was `u16`. Widened so one trait
serves every CPU.

- `emu198x-shell/src/debug.rs`: `pc() -> u32`, `peek(addr: u32)`, `poke(addr:
  u32, …)`, `disassemble(addr: u32)`, `IoEvent.pc: u32`. ✅
- The three per-CPU macros: widened signatures; 8/16-bit cores read the low 16
  bits (`addr as u16` at the machine boundary). ✅
- `mcp_tools.rs` `register_debug_tools`: address arg parsing widened to `u32`
  (`format!("${:04X}", …)` already grows past 4 digits, so 8-bit output is
  unchanged). ✅
- **Gate met:** whole workspace compiles, clippy clean, 133 shell tests + the
  6502/6809 debug-surface integration tests pass.

### Phase 2 — Amiga `DebugTarget` ✅ DONE (via `DebugPrimitives`, not a 68000 macro)

The planned `impl_68000_debug_target!` macro was **superseded** by a better shape
found during the spike — a `DebugPrimitives` adapter trait + a single blanket
`impl<T: DebugPrimitives> DebugTarget for T`:

- `emu198x-shell/src/debug.rs`: added `DebugPrimitives` (the `dbg_*` method set)
  and the blanket impl. ✅
- `runtime-commodore-amiga/src/debug.rs`: `impl DebugPrimitives for
  AmigaRuntimeKind`, delegating to the existing `AmigaLiveAccess` adapter —
  `cpu_state` from `motorola-68k-common::Registers`, `disasm` via
  `motorola_68000::disasm`, single-step by ticking until the instruction-start
  counter advances, big-endian byte fold over `read_word`. Spans 68000 (OCS/ECS)
  and 68020 (AGA) through the enum. `MachineCore::debug_target[_mut]` return
  `Some(self)`. (`motorola-68000` promoted dev-dep → dep for the disassembler.) ✅
- **Gate met:** `debug_surface_works_on_68000` passes; coexisted cleanly with the
  legacy macro impls during the spike.

### Phase 2b — Fleet onto `DebugPrimitives` ✅ DONE (emergent)

Because the blanket impl coexists with the legacy concrete impls (orphan rule),
the three `impl_*_debug_target!` macros were converted to emit `DebugPrimitives`
and renamed `impl_*_debug_primitives!`. All 24 macro machines + the Amiga now
land on one `DebugTarget` (the single blanket impl); Spectrum is the last bespoke
holdout. **Gate met:** workspace + clippy clean; 6502/6809/68000 debug tests pass.

### Phase 3 — Stand up Amiga MCP on `HeadlessSession`

Replace the `AmigaSession`-typed registry with the shared session.

- `emu198x-amiga/src/mcp/mod.rs`: build `HeadlessSession<AmigaRuntimeKind,
  AmigaSessionQueryProvider>` (mirror `script.rs`), call `register_common_tools`
  + `register_debug_tools`, then the Amiga-specific registrations (Phase 4).
- `AmigaRuntimeKind` already implements `DebugPrimitives` and returns
  `Some(self)` from `debug_target[_mut]` (Phase 2), so `register_debug_tools`
  lights up the moment the registry is `HeadlessSession`-typed — no extra wiring.
- **Gate:** input (keyboard + mouse), capture (screenshot/audio/video +
  recording), media, run/reset/snapshot, and the shared debug verbs all work
  over MCP; Workbench boots; mouse moves the pointer.

### Phase 4 — Port bespoke Amiga tools onto the shared session

`mcp/tools.rs` (3639 lines, 48 `tool_*` fns over `&mut AmigaSession`). The
session-state usage was inventoried 2026-06-05:
`s.access()/access_mut()` (68 sites → `session.machine()/machine_mut()`, which is
`&[mut] AmigaRuntimeKind: AmigaLiveAccess`), `s.cpu_trace` (16), `s.recorder` +
`s.last_recorded` (11), `s.tick_with_trace` (6), `s.rom_path` (1).

**Two state-home decisions (made):**

- **`recorder` → drop.** `HeadlessSession` owns video recording, and
  `register_common_tools` now exposes `start/stop_video_recording`. The bespoke
  `recorder`/`last_recorded`/`push_recorder_frame` and the bespoke
  `start/stop_video_recording`/`run_frames`/`run_ticks` tools are redundant.
- **`cpu_trace` → move `CpuTraceState` into `AmigaRuntimeKind`.** Give the runtime
  the trace buffer + arm/disarm/clear/log accessors, and capture into it when
  armed as the runtime ticks (incl. through `DebugTarget::step_instruction`).
  Then the shared `step`/`run_until_pc` feed the trace for free; the bespoke
  `step`/`run_until_pc` drop, and `cpu_trace_*` become thin tools over
  `session.machine_mut()`.

**DROP (covered by `register_common_tools` + `register_debug_tools`):**
`run_frames`, `run_ticks`, `step`, `run_until_pc`, `query_cpu`, `memory_read`,
`poke_word`, `disasm`, `start_video_recording`, `stop_video_recording`.

**PORT (Amiga-specific → `Tool<HeadlessSession<…>>`, via `session.machine()`):**
chip queries — `query_chipset`, `query_copper_list`, `query_blitter`,
`query_agnus`, `query_cia`, `query_paula`, `query_aga`, `query_disk`,
`query_exec_tasks`, `query_exec_ports`, `query_library`, `query_stack`; Exec —
`address_to_library`, `resolve_lvo`, `resolve_libraries`, `read_task_stack`,
`dump_msgport_messages`, `signal_task`, `wake_task`; tracers — `palette_log`,
`bplcon0_log`, `chipset_read_log`, `chipset_write_log`; trace —
`cpu_trace_arm/clear/disarm/log`; run variants — `run_until_any_pc`,
`run_until_mem_change`; misc — `memory_read_long`, `disasm_around`, `memory_scan`,
`dump_framebuffer`, `watch_memory`/`_clear`/`_log`, `insert_media`, `eject_media`,
`reset`, `restart`. (`reset`/`restart` could instead be added to
`register_common_tools` — a small shell change; decide during the port.)

**Approach (keeps the crate compiling):** build a new
`register_amiga_tools(registry: &mut ToolRegistry<HeadlessSession<…>>)` alongside
the existing `register_all`, porting in clusters (chip queries → exec → tracers →
trace → run-variants/misc), each cluster a commit verified against pre-migration
output. The production `mcp/mod.rs` stays on `AmigaSession` until the new module
is complete, then Phase 3 flips it. `headless_mcp_replatform.rs` is the guardrail.
- **Gate:** every previously-registered Amiga tool present and equivalent.

### Phase 5 — Retire `AmigaSession`

Delete `mcp/session.rs` (`AmigaSession`), the local `InlineTool`, and dead glue.

- **Gate:** full Amiga MCP tool list matches or supersets the pre-migration set;
  all verification anchors green; no dead code (clippy).

### Phase 6 (out of scope here) — Spectrum fold-in

Tracked separately per `debug-surface-tiers.md`; needs a generic-macro arm + a
`SpectrumDriver`→macro adapter. Not part of this plan.

## Parallel, non-blocking wins (independent of the Amiga campaign)

These close real fleet gaps from the assessment and can land any time:

1. **Audio recording → incremental-WAV streaming** (agreed option 1) — bound the
   in-RAM audio buffer for long sessions.
2. **Joystick `Button` consumers** on the ~12 capable-but-unwired machines.
3. **Paddle/analogue `Axis` consumers** where the chip seam exists (Atari 2600;
   **C64** via SID POTX/POTY; Amiga via Paula POT once it's on the shared session).
4. **Game Boy** `impl_sm83_debug_target!` — the one genuine shared-tier debug
   gap (a new macro member for a shipped single-CPU machine).

## Risk + rollback

- **Highest risk:** Phase 3/4 on the most-tested launch machine. Mitigate by
  keeping the bespoke tools registered until each cluster is ported+verified, and
  by gating on the Workbench boot + Tom Harte anchors every commit.
- **Phase 1** touches all 24 machines via 3 macros — mechanical, but a wide blast
  radius. Land it alone, verify the fleet compiles + a smoke debug call, before
  anything 68000-specific.
- Roll back per-phase: each phase is independently revertable; the bespoke Amiga
  MCP keeps working until Phase 5 deletes it.
