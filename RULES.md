# Rules

Hard constraints. Non-negotiable. If you find yourself breaking one, stop and rethink.

## Session start

Before writing any code, state in the conversation which October artefact this session serves.

**Public October launch (Crash! Live):** Spectrum only. **Spectrum SOLID** is the October-public goal — full criteria locked at [`wiki/decisions/october-catalogue.md`](wiki/decisions/october-catalogue.md#october-bar-definition). Headline: 8 in-scope variants (16K, 48K, Spectrum+, 128K, +2, +2A, +2B, +3); 10 catalogue entries per variant; single `emu198x-spectrum` binary with `--ui`/`--script`/`--mcp` modes; MCP server functional; pipeline tied to every Code198x curriculum unit's screenshot/video.

**Engineering quality bar:** All four systems (Spectrum, C64, NES, Amiga) have catalogue manifests. Non-Spectrum systems progress as engineering bar in priority order (C64 → NES → Amiga), with no October deadline. The amended public-vs-bar split is documented at the top of [`october-catalogue.md`](wiki/decisions/october-catalogue.md).

Anchor every session against one of:

- **Spectrum SOLID work** — Spectrum catalogue entry, runtime/chip/format gate, variant stability, real-hardware validation, Code198x pipeline reliability. October-public path.
- **Spectrum-supporting infrastructure** — capture pipeline, CRT filter, serialisation, native UI for Spectrum demo. Spectrum SOLID depends on it.
- **Non-Spectrum catalogue progress** (C64, NES, Amiga in that priority order) — engineering bar, no October deadline. Only after Spectrum SOLID is closer to done.

If the requested work doesn't fit one of these — Game Boy, Dragon 32, accuracy work past what manifests assert, jumping to non-Spectrum catalogue work before Spectrum SOLID is closer to done — **name it as deferred**, and ask whether to proceed. Don't silently expand the October-public pile or jump ahead of Spectrum sequencing. Once the user confirms, proceed; the rule is to flag, not refuse.

The October-public system is Spectrum. Everything else is engineering bar (C64/NES/Amiga) or post-launch (Game Boy, Dragon 32, Wave 2+) per [`wiki/decisions/product-roadmap.md`](wiki/decisions/product-roadmap.md).

## Clock

1. The master oscillator drives the loop. Not the CPU. Not the ULA. The crystal.
2. The ULA ticks every half-cycle. The CPU ticks only when the ULA allows it.
3. Contention = the CPU's clock slot is skipped. No extra ticks. No catch-up logic.
4. One clock, everything derives. `hc` is the only time counter.

## CPU

5. The Z80 is a half-cycle signal-level state machine. No instruction-level abstraction. *Other CPUs we add* (6502, 68000, 6809, …) tick at *their* native granularity (cycle-level for the 6502, half-cycle for the 68000) — but always cycle-accurate, never instruction-accurate.
6. **No Bus trait, no bus callback, no method-call-style memory access. Ever. For any CPU.** Every CPU we emulate — past, present, and future, every variant we ever add — exposes its bus state as public pin fields (`addr`, `data`, `data_in`, `rw`/`mreq`/`iorq`/etc.) and the machine inspects those pins between ticks to perform bus transactions. This is non-negotiable: multi-chip bus accuracy requires the pins to be continuously visible to other chips on the same master clock (Spectrum ULA contention, C64 VIC-II sprite DMA, Amiga Agnus bus arbitration, NES PPU/CPU interleave — every system has this, and the answer is the same for every system). See [`wiki/decisions/cpu-bus-interface.md`](wiki/decisions/cpu-bus-interface.md) for the full rationale.
7. Each MStep sequence is a static array. Execute is 0 half-cycles.
8. Conditional instructions: Execute BEFORE the conditional steps (RET cc, CALL cc, DJNZ).

## ULA

9. ULA as trait — one implementation per variant. No parameterisation across families.
10. UlaEngine holds shared rendering. Contention is variant-specific, lives in the wrapper.
11. The ULA renders to a palette-indexed `u8` framebuffer. RGBA conversion is a separate stage.

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
28. **Archives are temporary.** The lifecycle is *port → evaluate → clean up*. After each wave of porting, do a batched cleanup commit that removes the archive crates we ported (now duplicated in the workspace) and the ones we evaluated and judged not-portable. Cleanups are commits, not silent deletes — list what was removed, what was ported, what was rejected. When the archives contain only reference material we still consult, retire them entirely. See [`wiki/decisions/archives-as-source.md`](wiki/decisions/archives-as-source.md) for the full lifecycle.
29. **Archives are "dead" reference material.** Once a wave of cleanup begins, the archives are not expected to compile. It is acceptable for an archive's internal Cargo dependency graph to be broken by deleting a leaf crate even if other archive crates depended on it. This unblocks tight-scope deletions that would otherwise grow into multi-crate audits.

## Wiki

This project maintains an LLM-curated wiki at `wiki/`.
Read `wiki/SCHEMA.md` for structure and conventions.
Read `wiki/index.md` before starting work — it's your knowledge map.
Update wiki pages when you learn something that future sessions need.
For cross-project context, read `/Users/stevehill/Projects/wiki/index.md`.
