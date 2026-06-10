---
title: MCP query namespace (drop machine prefix), chip-read folding, and write access
date: 2026-06-10
status: accepted — Code198x path-usage check cleared 2026-06-10; safe to implement
scope: mcp / query + write surface
---

# MCP query namespace, chip-read folding, and write access

## Context

Came out of the #456 MCP/script/UI parity work. After landing
`register_base_tools` (one base surface for every machine) and proving the
debug tools are honestly wired and CI-tested, we examined the remaining
machine-specific MCP tools. The duplication is **not** in media/tape/disk
(generic `load_media` + `media_transport` already cover that in the base
set) — it is in small, near-identical **chip-snapshot read tools**
(`query_vdp` copy-pasted across ColecoVision / MSX / SG-1000; the AY-3-8910
queried as `query_ay` on Spectrum but `query_psg` on MSX), and there is real
redundancy (NES exposes PPU state via *both* a `query_ppu` tool *and*
`nes.ppu.*` query paths, which have already drifted apart).

This note records three coordinated decisions about the query/write surface,
to be designed together (they share one namespace) and landed in order.
Related: [debug-surface-tiers.md](debug-surface-tiers.md),
[debugger-architecture.md](debugger-architecture.md).

## Decisions

### 1. Drop the `<machine>` prefix from query paths

`nes.ppu.ctrl` → `ppu.ctrl`, `c64.cpu.a` → `cpu.a`, etc.

The prefix carries **zero routing weight**: the shell tries the reserved
shared prefixes first, then hands the *full, unstripped* path to the
machine's `SessionQueryProvider`, which matches it as a literal string.
Nothing dispatches on `nes.`. One MCP server hosts exactly one machine, so
the prefix is pure redundancy.

Dropping it is a parity + pedagogy win: `query cpu.pc` works on every
machine instead of forcing machine-specific knowledge into every path, and
it makes machine state consistent with the already-unprefixed shared paths
(`session.*`, `capture.*`, `run.*`).

**Reserved top-level prefixes:** `session`, `capture`, `run` belong to the
shell's shared query resolver. Everything else is chip/subsystem state owned
by the machine provider. New chip/subsystem names must not collide with the
reserved three (none do today — chips are `cpu`/`ppu`/`vdp`/`vic`/… ).

**Forward-looking caveat:** if a single MCP server ever drives *multiple*
machines at once (the Rachel cross-platform netplay direction), a qualifier
would be needed again. Accepted: we re-introduce one if/when that lands.

### 2. Fold pure chip-snapshot reads into the generic `query`

The bespoke `query_<chip>` tools are a second mechanism duplicating what
`query` + `SessionQueryProvider` already do for `cpu.*`/`ppu.*`. Fold them
in: each chip's snapshot becomes query paths (`vdp.scanline`, `psg.registers`
…) on the machine's provider, and the bespoke `InlineTool` is deleted.

`QueryResult.value` is `serde_json::Value`, so a path can return the exact
structured blob the old tool did — no new trait, no new crate, no by-chip
registrar (the by-chip-registrar idea was rejected as premature abstraction:
~30 lines of dedup is not worth a new architectural layer).

- **Path shape:** fine-grained leaves `<chip>.<field>` (matching the
  existing `cpu.*`/`ppu.*` convention), plus an optional grouped `<chip>`
  path returning the whole object for one-call ergonomics.
- **Boundary:** this folds the **pure-snapshot** reads only
  (`query_vdp`/`query_psg`/`query_ay`/`query_antic`/`query_gtia`/
  `query_pokey`/`query_pia`/`query_tia`/`query_ula`/`query_ctc`/`query_ppi`/
  `query_mapper`, and NES's `query_cpu`/`query_ppu`/`query_apu`). It does
  **not** fold the parameterized/bulk tools (`memory_read`, `dump_oam`,
  `dump_nametable`, `dump_palette`) — those take arguments / return indexed
  bulk data and are genuinely tool-shaped.
- **Naming:** one chip → one canonical path name fleet-wide. The AY-3-8910
  becomes `ay.*` everywhere (resolving the `query_ay` vs `query_psg` split).

### 3. Add write access as a narrow, explicit companion — not a mirror of reads

Today the mutation surface is `poke` (memory) and `step` only. You can
**read** every CPU register (`cpu_state` / `cpu.*` paths) but **write none** —
"set PC to `$C000` and run" is impossible. Closing that is what write access
is for.

Reads and writes are **not symmetric** and must not be mirrored 1:1:

- The **read** namespace is broad — it includes *observations*
  (`ppu.scanline`, `machine.frame_count`, `rendering_enabled`) that are
  derived state. "Writing" a scanline is meaningless.
- The **write** domain is narrow — only genuine registers/writable state.
  `set` targets an explicitly-declared **writable subset**; a read-only path
  returns a clear "not writable" error.

Two write channels, kept distinct:

1. **Memory → stays address-based** (`poke_byte`/`poke_word`). Correct as-is:
   it goes through the bus, so writing a memory-mapped register triggers the
   chip's real write side-effects. Do not path-ify memory.
2. **Registers → a new path-based `set <path> <value>`** over the writable
   subset, same unprefixed namespace (`set cpu.pc $C000`). Each write routes
   through the *proper* write logic — a CPU-register setter for `cpu.*`, the
   chip's `write_register` for a chip reg — **never** a blind "stuff the
   internal field," which would bypass side effects and misrepresent hardware.

**Priority:** CPU registers first — they have no memory address, so `poke`
can't reach them; that is the real gap. Chip registers are mostly already
writable via `poke`/`port_write` through their mapped address, so path-based
chip writes are a later convenience, not a gap.

## Trait implications (small, clean)

- `SessionQueryProvider` gains `write(machine, path, value) -> Result<…>`;
  read-only paths return a "not writable" error.
- `DebugTarget` gains a CPU-register setter (symmetric with the `cpu_state`
  reader it already has); `set cpu.*` routes to it.
- `query_paths` gains a way to flag which paths are writable.

## Sequencing

Design together (shared namespace); land in order, each on a clean base:

0. **GATE — Code198x path-usage check. ✅ CLEARED 2026-06-10.** Swept the
   whole Code198x sibling (230 `.script.json` session files + lessons +
   docs). The curriculum drives the emulator with action tools only —
   `run_frames` (1465×), `input` (1116×), `save_screenshot`, `load_snapshot`,
   `start/stop_video_recording`, `load_basic_program`, `type_string`,
   `wait_for_boot`, `poke_byte`, `load_media`, `autoload_tape`, `press_key`,
   audio capture. It uses **zero** `query` / `wait_for_query_*` /
   `query_paths` / `query_<chip>` actions: no machine state is read by query
   path anywhere. Every `"path":` key is a *file* path (`.sna`/`.bas`/`.png`),
   unaffected by the namespace change. And `poke_byte` is address-based,
   which decision 3 keeps. So the prefix drop, the chip-read fold, and the
   `set` addition are all clear of curriculum impact — safe to proceed.
1. Settle the namespace — drop `<machine>`, update providers + path lists +
   tests + the reserved-prefix rule.
2. Fold chip-snapshot reads into the clean namespace; delete the bespoke
   `query_<chip>` tools; unify chip names (`ay`, `vdp`, …).
3. Add register writes via `set` over the explicit writable subset.

## Trade-offs accepted

- **Discoverability:** explicit `query_<chip>` tools appear in `tools/list`
  with descriptions; query paths are discovered via `query_paths` (flatter).
  Accepted — query-paths is already the dominant pattern for state, so this
  is consistency, not loss.
- **Breaking change:** ~448 prefixed path literals across 27+ files
  internally, plus the external curriculum surface. Mechanical and
  scriptable; justified by doing it once, pre-launch, before the read-fold
  proliferates more paths.
