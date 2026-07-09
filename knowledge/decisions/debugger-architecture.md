# Decision: Debugger architecture — MCP-client UI, multi-window, learner-first

**Date:** 2026-05-23
**Status:** Locked. Brainstorm captured in
[`docs/plans/2026-05-23-post-october-roadmap.md`](../../docs/plans/2026-05-23-post-october-roadmap.md)
§ Phase C; this record promotes the design to a binding decision
before code starts. Implementation phased across four waves below.

## What this is

The architectural spine for the Emu198x debugger UI. Names the
binding choices on audience, process model, transport, tech stack,
disassembler approach, pause semantics, and MVP scope. Once any code
lands against this, the spine is load-bearing; tightening within it
is fine, replacing it is a new decision record.

This sits at the same architectural level as
[`spectrum-driver.md`](spectrum-driver.md),
[`runtime-internal-shape.md`](runtime-internal-shape.md), and
[`native-ui-strategy.md`](native-ui-strategy.md) — a long-lived
shape decision the rest of the codebase organises around.

## The decisions

1. **Separate `emu198x-debugger` binary**, not a `--debug` mode of
   each per-system binary and not an embedded panel in `--ui` mode.
2. **MCP client architecture** — the debugger is an MCP client of
   the per-system MCP servers, riding on the existing protocol
   surface plus a small set of new debug verbs.
3. **New transport: `--mcp-listen <addr>` mode** added to each
   per-system binary, alongside the existing stdio `--mcp` mode.
   Listen mode accepts multiple concurrent MCP clients against the
   same emulator instance.
4. **Multi-client model:** debugger UI + Claude + pop-out inspector
   windows can all connect to the same emulator simultaneously. All
   clients share the emulator state; each holds its own
   subscription / view state.
5. **Audience: learner-first.** Teaching-oriented chrome
   (tooltips, instruction explanations, scaffolded workflows) is
   the default; developer mode is a layout/scaffold toggle added
   later, not a separate product.
6. **Tech stack: egui** for the native UI (immediate-mode, integrates
   with the existing wgpu / winit stack).
7. **Disassemblers: hand-written `disasm-<cpu>` crates** —
   `disasm-zilog-z80`, `disasm-mos-6502`, `disasm-motorola-6809`,
   `disasm-motorola-68000`. Not third-party crates. Each owns the
   hover-tooltip data tied to the knowledge layer.
8. **Pause architecture: pause-anywhere** is supported (every chip
   checkpoints at master-clock granularity per the snapshot
   round-trip discipline). **UX exposes instruction-boundary step**
   in MVP; finer grains (step cycle, step half-cycle, step
   scanline) added when concrete use cases push for them.
9. **MVP V1 scope: read-only inspector + pause/resume + step
   instruction.** No breakpoints, no memory editing, no conditional
   pause. Those are Phase 2.
10. **Trace ring buffer in MVP** — last N executed instructions /
    bus ops, scrollable in a pop-out window.

## Why

### Why a separate binary and not an embedded panel

Three options were on the table:

- **(a) Embedded panel in `--ui` mode** — toggle the debugger as a
  sidebar of the existing per-system shell. Simplest to ship.
- **(b) `--debug` mode** of each per-system binary — same process,
  replaces `--ui` when set.
- **(c) Separate `emu198x-debugger` binary**, MCP client of the
  per-system shells.

(c) wins because it unlocks three first-class properties that (a)
and (b) cannot:

- **Multi-window / multi-client naturally.** A second debugger
  instance is just another MCP client. Pop-out memory inspector
  and pop-out trace viewer fall out for free.
- **Remote debugging is free** once the transport is over a socket.
  Debugger on your laptop, emulator on a Pi running headlessly —
  no extra plumbing.
- **Agentic debugging is free** — Claude (or any MCP-aware agent)
  becomes a debugger client by being itself. "The emulator is at
  this save state, find the bug" is a real workflow, not a future
  fantasy.

The cost is real: a second transport (`--mcp-listen`) and
multi-client safety in the MCP dispatch layer. Both are scoped,
small, and worth paying because they enrich the existing MCP server
that's already the project's distinctive feature.

### Why MCP and not a custom debug protocol

The MCP server already exposes ~80% of what a debugger needs to
read (CPU state via query, chip state via query, memory via query).
Adding 5–10 debug-control verbs (`pause`, `resume`,
`step_instruction`, eventually `set_breakpoint`, `write_memory`) is
enrichment of an existing surface, not invention of a parallel one.

The bonus: every debug verb that lands becomes a tool Claude can
call. Building the debugger and building agent-driven debugging are
the same work.

### Why native (egui) and not web/WASM

Steve's call, captured 2026-05-23 brainstorm: **emulators can't
embed ROMs**. The Code198x curriculum can't ship a Spectrum
playable as a web embed because most Spectrum titles are
commercial and not redistributable. That killed the
WASM-debugger-embedded-in-curriculum-pages angle that was the main
argument for a web frontend.

Without that angle, native wins on every axis: lower latency,
matches the existing wgpu/winit stack, better debug-tooling-shaped
widgets out of the box (`egui_dock`, `egui_extras` table view, hex
editor crates), no HTTP server in the emulator process.

If a web frontend is ever wanted later, the MCP transport already
supports it — any client speaking JSON-RPC over the listen socket
works.

### Why `--mcp-listen` as an additional mode rather than replacing stdio

Stdio mode stays because Claude Code launches the emulator as a
subprocess and talks via the spawned process's pipes — that pattern
shouldn't change. Listen mode is for everything else: debugger UI,
multiple concurrent clients, remote sessions. Same dispatch path,
different transport plumbing.

Implementation: ~200–300 LOC in `emu198x-shell/src/mcp.rs` adding
a `serve_listen(addr)` sibling to `serve_stdio`. The existing
`serve()` function is already transport-agnostic over `Read +
Write`, which is most of the work.

### Why hand-written disassemblers

The Rust ecosystem for retro CPU disassembly is patchy:

- Z80: a few crates exist, varying maintenance and coverage
- 6502: similar — multiple options, none authoritative
- 6809: scarce
- 68000: a couple of options, often tied to specific emulator
  projects (e.g., Musashi)

Writing our own across four CPUs is ~3500–5000 LOC total. We
already have the per-CPU opcode tables in the `mos-6502`,
`zilog-z80`, `motorola-6809`, `motorola-68000` execution code — the
disassembler reuses those tables.

The strategic payoff: **we own the hover-tooltip data**. Every
opcode in `disasm-zilog-z80` carries the description the learner
debugger surfaces on hover. That's the learner-mode differentiator,
and it's only possible if we own the disassembler.

Crate naming follows the existing convention
([crate-naming.md](crate-naming.md)): `disasm-<manufacturer>-<chipname>`.

### Why learner-first audience

The Emu198x project's adjacent audience is Code198x — retro
game-dev curriculum for learners. The debuggers that already exist
in this space (openMSX, Mesen2, VICE-mon, Fuse-debugger) are all
developer-oriented and dense. There's no teaching debugger for
retro 8-bit / 16-bit systems.

A learner-first debugger differentiates the project. A developer
debugger built first is harder to retrofit explanations into; a
learner-first debugger gets developer-mode by removing chrome (the
explanations toggle off, the layout densifies, the tooltips
disappear).

### Why pause-anywhere is architecturally free

Every chip in the workspace ticks at master-clock granularity (the
`tick_one_halfcycle` discipline established by
[spectrum-driver.md](spectrum-driver.md) and equivalent per-system
drivers). Every chip serialises losslessly per the
[save-state-format.md](save-state-format.md) discipline.

Therefore pause-at-any-tick costs nothing structural — the loop is
already ticking at the smallest unit, and any tick boundary can
serialize state cleanly.

The expensive part of "cycle-accurate pause" in other emulators is
usually that their tick loop isn't actually at master-clock
granularity, so pausing mid-CPU-instruction requires backtracking
or special-casing. We don't have that problem.

This means the UX can expose whatever granularity makes sense per
use case, in whatever order they become useful, without re-doing
the architecture.

### Why instruction-step in MVP, finer grains later

Step-instruction is the bread-and-butter UX for the learner audience
and the dominant case for developer debugging too. Adding it to MVP
is small.

Finer grains (step cycle, step half-cycle, step scanline) are real
needs for specific accuracy work (Float48K-class bugs, C64 BA/RDY,
NES PPU dot timing). They land when those needs concretely surface,
not speculatively. The architecture supports them; the UI surfaces
them when used.

Step-frame is trivial (it's what `run_until_frame` already does
inside the runtime). Step-scanline is small and worth landing the
moment the first raster-dependent investigation needs it. Step-cycle
and step-half-cycle are larger and probably never get used by 95% of
users.

### Why trace ring buffer in MVP

Read-only inspector without trace is "what's happening RIGHT NOW."
Read-only inspector WITH trace is "what just happened, scroll back
and see it." For accuracy work — which is half the debugger's
acceleration claim — trace is disproportionately valuable per LOC.

Cheap to implement (~200 LOC ring buffer in `emu198x-shell` next to
the existing query surface). Pop-out window in the debugger UI
scrolls through the last N entries.

## What we are NOT doing

- **No embedded debugger in the per-system `--ui` mode.** Toggling
  a sidebar feels easy but blocks the multi-window /
  multi-client / remote / agentic properties that motivate the
  whole design.
- **No web frontend in MVP.** ROM-legality kills WASM embedding;
  no other compelling reason for HTML/CSS over native.
- **No source-level mapping** *(in MVP; partly reopened 2026-07-04)*.
  We don't have source for third-party ROMs, so for those this
  isn't gdb — that stands. But for programs *we* assemble
  (Code198x lessons, Forge198x, anything built with Asm198x), we
  now do: Asm198x emits `dbg198x`, a debug-info sidecar (line map +
  symbols + sections + address spaces). Source-level debugging for
  our own programs is therefore back on the table — see § Forward
  vision. Third-party-ROM debugging stays symbol-optional.
- **No breakpoints, memory editing, or conditional pause in MVP.**
  Real Phase 2 work; require synchronous pause-at-condition
  semantics in every chip. Worth doing right rather than fast.
- **No netplay / multi-machine debugging.** Out of scope; flag-only.
- **No third-party disassembler crates** as primary dependencies.
  The hover-tooltip story requires we own the opcode descriptions.

## Phased delivery

### Wave 1 — transport (1–2 weeks)

- Add `--mcp-listen <addr>` mode to `emu198x-shell::mcp::serve_*`.
  TCP or Unix socket; both behind a small enum.
- Wire `--mcp-listen` into each per-system binary's CLI alongside
  the existing `--mcp` flag.
- Multi-client dispatch: the existing `Server` is already mostly
  multi-client-safe (it's a stateless router); audit and lock
  down the shared-state path.
- Add `pause` + `resume` MCP verbs to the protocol surface.
  Implementation: a runtime-level pause flag the tick loop honours.

### Wave 2 — disassemblers (2–3 weeks)

- `disasm-zilog-z80` first (Spectrum is the lead audience).
- `disasm-mos-6502` second (C64 / NES audience).
- `disasm-motorola-6809` and `disasm-motorola-68000` follow.
- Each crate exposes: `disassemble_at(memory: &[u8], pc: u16) ->
  DisassembledInstruction` with mnemonic, operands, byte length,
  and `description: &'static str` for hover tooltips.

### Wave 3 — debugger MVP (2–3 weeks)

- New crate `emu198x-debugger` (binary, egui-based).
- Connect by `--connect <mcp-url>`; auto-discover system via
  `system.identify` verb.
- Layout: CPU panel + disassembly + memory hex + system-specific
  chip-state panel + trace ring buffer pop-out.
- Pause / resume / step-instruction controls.
- Per-system chip-state widgets behind a trait
  (`SystemDebugWidgets`), one impl per anchor system.

### Wave 4 — ongoing enrichment (Phase 2+)

- Step-over, step-out, step-scanline as accuracy work demands.
- Breakpoints (PC, memory read/write, register equality).
- Memory editor with hex / decimal / binary views.
- Conditional pause / watch expressions.
- Learner-mode tooltips wired to `knowledge/chips/` content.
- Developer-mode layout toggle (denser, no tooltips, more panels).
- Pop-out memory inspector / pop-out trace / pop-out chip state as
  separate `emu198x-debugger` instances (free once Wave 1 lands).

## Open questions deferred to Wave 4+

- **Step-over implementation for the four CPUs.** Z80 and 6502 are
  straightforward (CALL / JSR have known opcodes); 6809 has BSR /
  JSR / LBSR; 68000 has BSR / JSR / TRAP. Each is small per CPU
  but the cross-cutting question of "what counts as a return" needs
  a per-CPU answer.
- **Breakpoint mechanism — software or hardware.** Software (replace
  the byte at PC with a `RST 38` / `BRK` / `SWI` / `TRAP` and trap
  it) is simpler but mutates memory and requires snapshot of the
  original byte. Hardware (PC comparison in the tick loop) is
  cleaner but adds a tick-time check. Probably hardware-style; flag
  for Wave 4 design.
- **Memory editing safety.** Editing the live tick loop's memory
  has to be sequenced safely. Probably: pause first, edit, resume.
  Atomic-while-running is out of scope.
- **Watch expression DSL.** "Pause when `$D012 == $80`" — what
  syntax? Either a small expression parser or per-type structured
  predicates. Defer until the first concrete use case demands it.
- **Code198x learner-mode integration shape.** Tooltips can be
  static text per opcode, or pull from per-system reference pages
  in `knowledge/chips/` or `knowledge/systems/`. Decide when
  learner-mode lands, not now.

## Forward vision (added 2026-07-04)

Three connected capabilities that the existing architecture makes unusually
cheap — captured now as direction, not committed scope. They share a spine: the
emulator is deterministic (no RNG, no wall-clock in the sim path), it already
plans a trace ring buffer, it exposes a scriptable MCP surface, and — new — it
can consume `dbg198x` to know what the running bytes *mean*.

- **Consume `dbg198x` for symbols and source** (the enabler for the other two).
  When Wave 4's breakpoints and stepping land, build them to read `dbg198x`
  (`symbol_at` / `addr_of` / `line_at`) rather than inventing a symbol format:
  address→label disassembly, breakpoint-by-label, and source-anchored stepping
  for any program we assembled. It is `serde`-only and authored in Asm198x —
  see the umbrella
  [`asm198x-and-shared-isa-spec.md`](../../../../decisions/asm198x-and-shared-isa-spec.md)
  § The debug-info layer. Wiring it into the shared `DebugTarget` tier lights it
  up across all machines at once, and it directly serves Forge198x's dev loop.

- **Execution-flow visualization — "a flow chart of how a game progresses."**
  Two layers that combine: a *static* control-flow graph from the structured
  disassembly (`isa-disasm`) annotated with `dbg198x` symbols (basic blocks,
  branches, calls between *named* routines), and a *dynamic* overlay from the
  trace ring buffer showing which paths *this* playthrough actually took and how
  it moved between routines/states over time. The dynamic-over-static overlay is
  the interesting artifact — "here is how the game flowed through its own code,
  by name" — and it doubles as docs-site material (annotated inner-workings) and
  an agent-native surface (an agent reasons over named flow, not hex). Feasible
  here specifically because determinism makes the trace reproducible, `dbg198x`
  supplies the names, and the MCP surface makes the whole thing scriptable.

- **Flow recovery for games we *didn't* write** (the third-party mirror of the
  above — no `dbg198x`, no source). Recovering structure from a stranger's ROM
  is hard *statically* (you can't tell code from data, resolve `JMP ($xxxx)` /
  jump tables / RTS-dispatch, or follow bank switches by staring at bytes). But
  we don't have to do it statically: a cycle-accurate, deterministic,
  instrumented engine turns it into **dynamic trace-driven recovery**, which
  sidesteps all of those — every executed byte is provably code, every taken
  edge is observed not inferred, and the live bank config is already tracked
  (the trace records `(bank, addr)`, the same address-space model `dbg198x`
  defines). Proven prior art: Mesen's Code/Data Logger does the code-vs-data
  half by watching execution. Two limits, and the two things that turn them into
  our advantage: (1) *dynamic tracing is sound but incomplete* — it only sees
  paths that ran — so use the **agent fleet as the coverage engine** (the W4
  compat pipeline pointed at coverage: agents exploring, deterministic traces
  merged); (2) *it recovers structure without names* (`sub_C123`, not "sprite
  multiplexer") — so annotate flow by **which hardware each block touches** (a
  block writing `$D400–$D418` is SID → music; one reached only from the IRQ
  vector is the raster routine). Chip-awareness gives semantics a bare
  disassembler can't. The honest line: *flow chart of a playthrough* is
  tractable and uniquely cheap here; *complete named reverse-engineering* stays
  hard and human-in-the-loop.

- **Source-anchored rewind.** Rewind is already cheap by construction —
  determinism means a snapshot ring buffer + deterministic replay reconstructs
  any earlier state exactly (the RZX-style replay generalised; see the rewind
  planning note). `dbg198x` upgrades it from "rewind N frames" to "rewind to the
  last time we were in `draw_sprite` / at `main.asm:214`" — time travel anchored
  to source and symbols, not raw frame counts. Rewind + the flow chart are the
  same feature from two angles: the chart shows where you've been and can go;
  rewind takes you there.

None of this is scheduled — it sits behind the Wave 4 breakpoint/stepping work
and the umbrella best-in-class programme's near-term floor. Captured so the
`dbg198x` seam and the determinism dividend are designed *for* rather than
rediscovered later.

## Acceleration claim — concrete

This decision pulls debugger work forward from the Phase C window
(Oct–Feb post-SOLID per the roadmap doc) into roughly **late June
/ early July** because four ongoing investigations would compress
materially with the debugger in hand:

1. **A1200 / Kickstart 3.1 boot** — currently print-statement
   debugging in `ks31_boot.rs` across 5,000+ frames. Live D7 / SSP
   / PC / chip-state view collapses the hypothesise-rerun loop.
2. **C64 Seam 1 BA/RDY accuracy** — Ghostbusters / Thinker /
   Thing-on-a-Spring loader stalls are diagnostic-loop-bound.
3. **NES Blargg PPU test ROM debugging** (once those land in CI).
4. **Compounding leverage on every future accuracy bug** —
   shape-of-work change.

If the acceleration doesn't materialise after Wave 1+2 land, this
decision gets re-examined. Worst case the debugger MVP is delayed
without losing the foundational `--mcp-listen` transport work,
which is independently valuable.

## Drift triggers

If I'm about to suggest any of these, stop and re-read this record
before continuing.

- **"Just embed the debugger in the existing --ui as a panel"** —
  loses multi-window, multi-client, remote, and agentic properties
  that motivated the whole design. The convenience is real; the
  cost is permanent shape constraint.
- **"Let's add a web frontend so Code198x can embed playable
  emulators"** — re-read the ROM-legality argument. WASM embedding
  was killed by that, not by tech preference.
- **"We can depend on the {z80-asm, 6502-asm, ...} crate instead
  of writing one"** — loses the hover-tooltip ownership that makes
  learner mode work. Third-party crates also have unpredictable
  maintenance.
- **"Pause-anywhere is too expensive — let's commit to
  instruction-boundary forever"** — wrong; the architecture
  supports pause-anywhere for free. Don't bake instruction-only
  into the protocol or runtime.
- **"Add breakpoints to MVP"** — out of scope. Breakpoints need
  synchronous pause-at-condition support across every chip; doing
  them properly is Wave 4 work and doing them quickly is worse than
  not doing them.
- **"Make this a `--debug` mode of each per-system binary instead
  of a separate binary"** — collapses two distinct deliverables
  (the per-system shell and the debugger) into one tightly-coupled
  build, blocks remote, blocks multi-client. The separation is
  load-bearing.
- **"Cycle-accurate step UX in MVP"** — defer to actual need. The
  arch supports it; surfacing it without a concrete bug needing it
  is premature complexity.

## Log

### 2026-07-04 — Forward vision added (dbg198x, flow chart, rewind)

Asm198x grew a `dbg198x` debug-info crate (line map + symbols + sections +
address spaces, NDJSON, serde-only) that names Emu198x as its consumer. Added
§ Forward vision capturing three connected directions the existing architecture
makes cheap: consume `dbg198x` for symbol/source-level debugging (rather than
inventing a format); execution-flow visualization ("a flow chart of how a game
progresses" — static CFG from `isa-disasm` + `dbg198x` names, overlaid with the
dynamic trace); and source-anchored rewind (determinism makes rewind cheap;
`dbg198x` anchors it to routines/lines instead of frame counts). The two
early product thoughts (flow chart, rewind) are Steve's. Also amended the
"No source-level mapping" NOT-doing item — it reopens for programs we assemble.
Umbrella capture of the `dbg198x` layer is in
[`asm198x-and-shared-isa-spec.md`](../../../../decisions/asm198x-and-shared-isa-spec.md).

### 2026-05-23 — Decision locked

Brainstorm captured in
[`docs/plans/2026-05-23-post-october-roadmap.md`](../../docs/plans/2026-05-23-post-october-roadmap.md)
§ Phase C and the in-session brainstorm thread. Audience direction
(learner-first), architecture (MCP-client separate binary),
transport (`--mcp-listen`), tech stack (egui), disassemblers
(hand-written), pause semantics (pause-anywhere arch /
instruction-boundary UX), MVP scope (read-only + trace + pause +
step-instruction) all confirmed by Steve.

Phased delivery agreed: Wave 1 transport → Wave 2 disasm → Wave 3
MVP → Wave 4 enrichment. Acceleration claim grounded in the four
concrete in-flight investigations the debugger should compress.

No code landed yet. First implementation work is Wave 1 in
`emu198x-shell/src/mcp.rs` and the per-binary CLI surfaces.
