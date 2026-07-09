# Rules

Hard constraints. Non-negotiable. If you find yourself breaking one, stop and rethink.

## Umbrella context

This project lives at `~/Projects/198x/Emu198x/emu198x/` inside the local `emu198x` org container. The umbrella binds rules that span sibling projects — see [`../../CLAUDE.md`](../../CLAUDE.md) and [`../../decisions/`](../../decisions/).

Hardware reference is **layered**, not single-canon. The primary library at [`../../reference/`](../../reference/) is the source of truth — Docling-extracted datasheets, manuals, magazines with sidecar metadata, organised by-system and by-topic. This project's [`knowledge/`](knowledge/) is a *codebase-tied distillation*: schema-bound (`knowledge/SCHEMA.md`), pressure-tested by working code, capturing what the emulator actually depends on. It cites the primary library; it does not replace it. When chip-level facts (Z80, 6502, 6510, 68000, VIC-II, ULA, Paula, SID, AY-3-8912) need updating, the primary library is the first port of call.

Full layered model, citation direction, and drift triggers at [`../../decisions/shared-hardware-reference-canon.md`](../../decisions/shared-hardware-reference-canon.md).

## Session start

Before writing code, state what lane the session serves:

- **Spectrum launch-hardening** — regression gates, validation, capture reliability, and any residual accuracy/scope debt in [`docs/status/outstanding-work.md`](docs/status/outstanding-work.md).
- **Best-in-class campaign work** — staged reference-class campaigns per [`../../decisions/emu198x-best-in-class.md`](../../decisions/emu198x-best-in-class.md) and [`docs/plans/2026-07-03-best-in-class-programme.md`](docs/plans/2026-07-03-best-in-class-programme.md).
- **Engineering-frontier work** — additional systems, catalogue progress, shared infrastructure, validation corpus work, and cross-machine improvements.

Flag genuinely out-of-roadmap work before expanding scope. Roadmap tiers live at [`knowledge/decisions/product-roadmap.md`](knowledge/decisions/product-roadmap.md).

## Clock

1. The master oscillator drives the loop. Not the CPU. Not the ULA. The crystal.
2. The ULA ticks every half-cycle. The CPU ticks only when the ULA allows it.
3. Contention = the CPU's clock slot is skipped. No extra ticks. No catch-up logic.
4. One clock, everything derives. `hc` is the only time counter.

**Drift triggers.** If you find yourself writing `for _ in 0..tstates_per_frame`, STOP — you're wrong. No extra ticks inside bus calls. No catch-up logic.

## CPU

5. The Z80 is a half-cycle signal-level state machine. No instruction-level abstraction. *Other CPUs we add* (6502, 68000, 6809, …) tick at *their* native granularity (cycle-level for the 6502, half-cycle for the 68000) — but always cycle-accurate, never instruction-accurate.
6. **No Bus trait, no bus callback, no method-call-style memory access. Ever. For any CPU.** Every CPU we emulate — past, present, and future, every variant we ever add — exposes its bus state as public pin fields (`addr`, `data`, `data_in`, `rw`/`mreq`/`iorq`/etc.) and the machine inspects those pins between ticks to perform bus transactions. This is non-negotiable: multi-chip bus accuracy requires the pins to be continuously visible to other chips on the same master clock (Spectrum ULA contention, C64 VIC-II sprite DMA, Amiga Agnus bus arbitration, NES PPU/CPU interleave — every system has this, and the answer is the same for every system). See [`knowledge/decisions/cpu-bus-interface.md`](knowledge/decisions/cpu-bus-interface.md) for the full rationale.
7. Each MStep sequence is a static array. Execute is 0 half-cycles.
8. Conditional instructions: Execute BEFORE the conditional steps (RET cc, CALL cc, DJNZ).

## ULA

9. ULA as trait — one implementation per variant. No parameterisation across families.
10. UlaEngine holds shared rendering. Contention is variant-specific, lives in the wrapper.
11. The ULA renders to a palette-indexed `u8` framebuffer. RGBA conversion is a separate stage. This is a ULA-family choice (and the Game Boy PPU shares it), **not** a fleet-wide pipeline rule — most video chips, including the VIC-II, VIC-I, NES PPU, TMS9918, Sega VDP and the Atari chips, render straight to ARGB32. See [`knowledge/decisions/framebuffer-pixel-format.md`](knowledge/decisions/framebuffer-pixel-format.md) for the convention and why per-chip choice is fine.

## Memory

12. Memory is a separate trait from the ULA. `is_contended(addr)` is a memory concern.
13. ROM writes are silently ignored. No panics, no logs.

## Audio

14. Beeper + tape EAR are mixed into the speaker level. AY is a separate channel.
15. The AY uses a /8 internal prescaler. Bresenham downsampling, not floating-point.

## Testing

16. Tom Harte, ZEXDOC, ZEXALL, and FUSE tests are integrated from day one.
17. Screen rendering tests verify pixel-level accuracy without visual inspection.
18. Boot tests verify each variant reaches its menu with screen content.

## Quality

19. No `.unwrap()` in library code. Runner code may panic on setup failures.
20. No stub implementations. Every chip does what the silicon does.
21. Accuracy is foundational, not retrofitted. If it's wrong, fix it now.

## File handling

22. **Loaded source files are immutable.** The emulator never writes back to a path it loaded from via `load_file` or any equivalent. Snapshots, tapes, and disk images are preservation-grade artifacts; the canonical TOSEC/WoS dump is the version nobody touched.
23. In-memory modifications stay in memory across the session, and are persisted *only* via the save state format (`~/.emu198x/saves/<family>/*.state`). Disk-controller writes (WD1793, µPD765A) mutate the in-RAM `DiskImage`, never the file on disk.
24. Exporting modified state to a foreign format (`.z80`, `.sna`, `.tap`, `.tzx`, `.dsk`, `.edsk`) always writes to a *new* user-chosen path. There is no `Save` vs `Save As` distinction — both are `Save As`, and overwriting the loaded source is not a code path that exists.

## Code reuse

25. **Check the archives before writing new code.** Three sibling archives at `~/Projects/Emu198x-archive`, `~/Projects/Emu198x-archive-april2026`, and `~/Projects/Emu198x-backup` contain hundreds of crates of pre-rewrite work — format parsers, tokenisers, MCP handlers, capture pipelines, scripting infrastructure, tools, and full system implementations. Throwing this away to write something new is unreasonable. Before authoring anything substantive, search the archives.
26. **Chip/CPU/cycle-accuracy code does not port.** The April 2026 fresh-start rewrite changed the cycle-accuracy approach (master oscillator drives the loop, half-cycle ULA, signal-level Z80, no Bus trait). Anything in the archives that lives in `cpu-*`, `machine-*`, `*-ula`, or directly touches the bus protocol is suspect — read it for ideas, but assume it needs rewriting against the new rules. Format crates, tokenisers, MCP tool handlers, capture pipelines, scripts, and tools are usually portable with minor adaptation.
27. **Document why an archive port was needed.** When porting non-trivially from an archive, mention the source path in a code comment. This preserves the audit trail and helps the next reader understand why the code is shaped the way it is.
28. **Archives are temporary.** The lifecycle is *port → evaluate → clean up*. After each wave of porting, do a batched cleanup commit that removes the archive crates we ported (now duplicated in the workspace) and the ones we evaluated and judged not-portable. Cleanups are commits, not silent deletes — list what was removed, what was ported, what was rejected. When the archives contain only reference material we still consult, retire them entirely. See [`knowledge/decisions/archives-as-source.md`](knowledge/decisions/archives-as-source.md) for the full lifecycle.
29. **Archives are "dead" reference material.** Once a wave of cleanup begins, the archives are not expected to compile. It is acceptable for an archive's internal Cargo dependency graph to be broken by deleting a leaf crate even if other archive crates depended on it. This unblocks tight-scope deletions that would otherwise grow into multi-crate audits.
30. **Promote cross-machine functionality to the highest layer that fits.** When you add a capability to one machine, ask whether other machines would want it. If it generalises with no machine-specific knowledge, implement it once in the shared layer — the shell's `DebugTarget` + `register_*_tools` for debug/MCP verbs, a `common-{family}` crate within a family, or a `runtime-{family}` for a within-family concern — not in a per-system binary. Per-system code is reserved for genuinely chip- or architecture-bound behaviour (Amiga copper/Exec/library walks, NES PPU palette/OAM/nametable dumps, Z80 port I/O, AY register watches). **When you find a per-system tool re-implementing a generalisable verb, elevate the richness *up* and let the per-system copy collapse onto it — converge up, never down by deleting the richer behaviour.** The shared tier should be the *richest* version, not the lowest common denominator. The tell that this rule was skipped: the same shape hand-rolled independently in two binaries — the NES and the Amiga each separately grew a per-step PC trace, raw instruction `bytes` in disasm, and run-until-memory-change, none of which needed CPU-specific knowledge. See [`knowledge/decisions/debug-surface-tiers.md`](knowledge/decisions/debug-surface-tiers.md) (shared `DebugTarget` vs bespoke), [`knowledge/decisions/runtime-internal-shape.md`](knowledge/decisions/runtime-internal-shape.md), and the umbrella [`../../decisions/`](../../decisions/) within-family layering.

## Planning

31. **Brainstorm before implementation.** Do not jump straight to code. Use `/workflow:brainstorm` or `AskUserQuestion` to align on approach first. We burned an entire session retrofitting accuracy because we skipped planning — the cost of pausing to think is always lower than the cost of unwinding the wrong design.

## Reference emulators

32. **Never do accuracy or timing work on a system without a reference emulator.** A vendored, authoritative emulator under `198x/emulators/<system>/` is a *prerequisite* for changing chip/CPU/timing behaviour — not an optional cross-check. If none exists for the system in hand, **stop and clone one** (canonical, readable, licence-compatible; e.g. VICE for Commodore, b-em for the BBC, Stella for the 2600, Elkulator for the Electron) and record it in that system's `INDEX.md` before touching code. Reasoning the fix out from the datasheet and first principles is the last resort, for when no reference can be obtained at all. **Drift trigger:** if you're about to deduce a frame length, cycle count, register-bit meaning, or interrupt behaviour from the spec alone — STOP and consult the reference emulator. The pattern of wins is consistent (VICE gave the PET frame outright; b-em's `acia.c` fixed the BBC `>` prompt; Stella caught a bad 2600 WSYNC change).

## Knowledge

Knowledge layer at `knowledge/` (decisions in `knowledge/decisions/`, index at `knowledge/index.md`). The SessionStart hook surfaces this content automatically. See [[project-knowledge-layer]] for capture conventions and [[kb-architecture]] for the three-layer model.
