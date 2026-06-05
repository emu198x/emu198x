# Plan: Unified driver surface — fold Amiga onto the shared session + 68000 debug

**Date:** 2026-06-05
**Type:** Refactor (flagship). Owner-approved override of
[`knowledge/decisions/debug-surface-tiers.md`](../../knowledge/decisions/debug-surface-tiers.md)
(see its 2026-06-05 Log entry).
**Goal:** every machine is driven the *same way* through either door — `--script`
and `--mcp` expose the same verb vocabulary, built on the shared
`HeadlessSession` + `register_common_tools` + `register_debug_tools`. Amiga is the
last machine on a parallel path; this folds it in.

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

### Phase 1 — Widen `DebugTarget` to `u32` addresses (shared, foundational)

The 68000 has a 24-bit bus / 32-bit PC; the trait is `u16`. Widen it so one trait
serves every CPU.

- `emu198x-shell/src/debug.rs`: `pc() -> u32`, `peek(addr: u32)`, `poke(addr:
  u32, …)`, `disassemble(addr: u32)`, `IoEvent.pc: u32`.
- The three `impl_{6502,z80,6809}_debug_target!` macros: widen signatures;
  8/16-bit cores read the low 16 bits (`addr as u16` at the machine boundary).
- `mcp_tools.rs` `register_debug_tools`: widen address arg parsing/formatting to
  `u32` (hex `$XXXXXX`).
- **Gate:** all 24 shared-tier machines compile + a smoke debug call works.
  Pure widening, no behaviour change for existing machines.

### Phase 2 — Build `impl_68000_debug_target!`

New macro member in `debug.rs`, mirroring the others but for the 68k.

- `cpu_state` from `motorola-68k-common::Registers` (`d[0..8]`, `a[0..7]`, `usp`,
  `ssp`, `pc`, `sr`).
- `disassemble` via `motorola_68000::disasm::disassemble(pc, read)`.
- `step_instruction` via the runtime's existing single-instruction advance.
- Spans 68000 (OCS/ECS) and 68020 (AGA) through the `AmigaMachine` CPU accessor.
- Applies to the non-generic `AmigaRuntimeKind` enum (the macros emit non-generic
  impls; `AmigaRuntimeKind` already `impl MachineCore`).
- **Gate:** macro compiles against `AmigaRuntimeKind`; `cpu_state`/`disasm`/`step`
  return correct values vs the current bespoke Amiga tools (cross-check).

### Phase 3 — Stand up Amiga MCP on `HeadlessSession`

Replace the `AmigaSession`-typed registry with the shared session.

- `emu198x-amiga/src/mcp/mod.rs`: build `HeadlessSession<AmigaRuntimeKind,
  AmigaSessionQueryProvider>` (mirror `script.rs`), call `register_common_tools`
  + `register_debug_tools`, then the Amiga-specific registrations (Phase 4).
- `AmigaRuntimeKind`: add `debug_target_hooks!` + the Phase-2 macro invocation.
- **Gate:** input (keyboard + mouse), capture (screenshot/audio/video +
  recording), media, run/reset/snapshot, and the shared debug verbs all work
  over MCP; Workbench boots; mouse moves the pointer.

### Phase 4 — Port bespoke Amiga tools onto the shared session

Move the ~40 Amiga-specific tools (copper list, blitter, chipset, CIA, Agnus,
Paula, Denise/AGA, exec tasks/ports, library resolution, LVO, `address_to_library`,
cpu-trace, msgport dump, stack read, video recording state) from
`Tool<AmigaSession>`/`InlineTool` onto `Tool<HeadlessSession<…>>`.

- Most are reads → re-express as `SessionQueryProvider` paths where natural, or
  keep as thin tools reading through the session's `machine()`.
- The `cpu_trace` + `recorder` state that lived on `AmigaSession` moves to a
  session-side extension or query provider field (decide during the phase; keep
  it minimal).
- Port in clusters (chipset, exec, library, trace) — one cluster per commit,
  each verified against the pre-migration tool output.
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
