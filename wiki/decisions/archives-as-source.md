# Decision: Archives are a first-class source

**Date:** 2026-04-09

## The decision

Several sibling project directories outside the current `Emu198x` workspace
contain either pre-rewrite implementations or external reference emulators and
libraries. The Rust workspaces are **first-class sources to port from** where
that makes sense. The external emulator and FPGA directories are **first-class
verification and cross-reference sources** even when we would never port code
from them directly.

Throwing the work away because we rewrote the cycle-accuracy core is
unreasonable — the cycle-accuracy boundary is narrower than it sounds.

## Local source categories

There are two different kinds of sibling sources and they should not be
confused:

- **Porting sources**: older in-house Rust workspaces whose structure, format
  parsers, capture code, scripting, and some non-timing-sensitive subsystems
  can be lifted into the new workspace.
- **Verification sources**: third-party emulators, FPGA cores, and one-off
  experiments that are valuable for comparison, behavior tracing, and source
  archaeology, but are not candidates for direct adoption.

| Path | Snapshot date | Crate count | Architectural style | Best for |
|---|---|---|---|---|
| `~/Projects/Emu198x-archive` | 2026-03-23 | 148 | Old cycle-accuracy approach. Single big workspace covering many systems. Each `emu-*` crate has its own `mcp.rs`, `capture.rs`, etc. The "broad and deep" archive. | Format crates (TAP, TZX, SNA, Z80, BAS, D64, GCR, PRG, INES, ADF, IPF, …), tokenisers (Spectrum BASIC, C64 BASIC), MCP tool handlers, the JSON-script dispatcher pattern (`emu-core::McpServer::run_script`), CLI argument layouts. |
| `~/Projects/Emu198x-archive-april2026` | 2026-04-02 | 22 | Mid-refactor toward cleaner separation. `bins/` directory, modular `emu-capture` (separate `gif.rs`, `screenshot.rs`, `video.rs`, `wav.rs`, `audio_recording.rs`), `emu-mcp` split into `catalogue/request/response/tool`, dedicated `cpu-*` crates. The "tight and structured" archive. | Architectural patterns and module organisation. `emu-rewind` (Phase 5.5 prior art). `emu-config` patterns. |
| `~/Projects/Emu198x-backup` | 2026-01-30 | n/a (different layout — flat `core/`, `systems/<machine>/src/`) | Pre-Cargo-workspace organisation. The earliest iteration. Simpler and less complete than the later archives, but the only other source we have for several subsystems. | **Second reference for chip-level code.** `systems/c64/src/{cia,sid,vic_ii,bus,cartridge,c64}.rs` and `systems/vic20/src/{vic,vic20}.rs` are functional, less complete than the March archive's equivalents but valuable to diff against when the March versions' behaviour is unclear. |

### Local-path note

The path names above reflect the decision's original wording, but the local
machine may expose equivalent sources under different names.

In the current environment:

- `~/Projects/Emu198x-Older` is the available older in-house Rust workspace
  and serves the same role as the older archive references above.
- `~/Projects/Emu198x-Unclean` is a verification/reference corpus containing
  third-party emulators, MiSTer cores, and libraries such as `ares`, `fceux`,
  `capsimg`, `fs-uae`, and multiple FPGA implementations. Treat it as a source
  of comparative behavior and implementation ideas, not as code to port
  wholesale.
- `~/Projects/Emu198x-Zig` contains a cycle-accurate Game Boy emulator in Zig.
  Treat it as **abandoned as an implementation direction** for Emu198x, but
  keep it available as a Game Boy verification and design reference until a
  future Rust version exists.

## What ports cleanly

These categories are *almost always* lift-and-shift, possibly with edition bumps or workspace-dependency rewiring:

- **Format crates** (`format-*`): pure parsers and serializers, no machine state, no clock model. Includes BAS tokenisers.
- **MCP tool handlers**: the *what each tool does* logic, not the rmcp framing. Boot, load_sna, run_frames, screenshot, audio_capture, query, poke, press_key, etc. all wrap calls into the runtime.
- **Capture pipeline pieces**: PNG encoding, WAV streaming, ffmpeg-pipe video. Already partially ported in `emu198x-shell::capture` (Phase 0.13); the april2026 archive has a more refined module split worth studying.
- **JSON-script dispatcher**: the `serde_json::Value` → method-name → handler pattern. ~50 lines, lift-and-shift the algorithm.
- **CLI argument layouts**: the *names and shape* of flags (`--headless`, `--script`, `--frames`, `--screenshot`, …) are reusable as a contract Code198x already understands.
- **Tools and helpers**: `parse_hex`, `char_to_spectrum_key`, key name resolution, etc.

## What does not port

The **April 2026 fresh-start rewrite** changed the cycle-accuracy approach. The new constraints are non-negotiable (see [`RULES.md`](../../RULES.md) items 1–14):

- The master oscillator drives the loop. Not the CPU, not the ULA.
- The ULA ticks every half-cycle; the CPU ticks only when the ULA allows it.
- Contention skips the CPU's clock slot — no extra ticks, no catch-up logic.
- The Z80 is a half-cycle signal-level state machine. No instruction-level abstraction.
- No `Bus` trait. The machine inspects Z80 signals and performs bus transactions directly.
- ULA as a per-variant trait, not parameterised across families.

This rules out direct ports of:

- **`cpu-*` crates** (cpu-z80, cpu-6502, cpu-6809, cpu-m68k): the new Z80 is a different shape entirely. Read for ISA reference, rewrite the implementation.
- **`machine-*` crates**: the bus loop, the per-T-state behaviour, and the contention model all have to be rewritten. The *peripheral* side (keyboard matrix, kempston, tape player API) often ports if you carefully separate state-only fields from bus-touching methods.
- **`*-ula` crates**: same reason. The interface is closer (a per-variant trait), but the timing model is different.
- **WASM frontends using the old machine API** (`emu-*-wasm`): the *entry-point pattern* (wasm-bindgen exposing methods like `load_basic`, `set_key`, `step_frame`) is informative, but the Machine calls underneath need to be rewritten for the new runtime.

When in doubt: read the archive code, take the *idea*, write the implementation against the new rules.

## Process when porting

1. **Search the archives first.** Before authoring something new, check whether the same problem was already solved. `find ~/Projects/Emu198x-archive* -name "*.rs" | xargs grep -l <thing>` is the easiest first pass.
2. **Read both archives**, not just one. The Mar 23 archive has more code; the April 2026 archive has cleaner organisation. The two together usually beat either alone.
3. **Note the source.** When porting non-trivially, leave a comment naming the archive path the code came from. Example:
   ```rust
   // Ported from Emu198x-archive/crates/format-sinclair-zx-spectrum-bas/src/lib.rs
   // Tokeniser is unchanged — the Spectrum BASIC keyword table and FP encoder
   // are independent of the cycle-accuracy core.
   ```
4. **Update edition and dependencies** to match the new workspace (`edition.workspace = true`, deps from workspace).
5. **Run any tests that came with the original** before adapting. If they pass unchanged, you have high confidence the port is sound.
6. **Adapt only what you must** to fit the new architecture. Resist refactoring opportunistically — that's how lift-and-shift turns into rewrite.

## Lifecycle: archives are temporary

The archives are *not forever*. They exist as a porting source, not a permanent attic. The lifecycle is:

1. **Port phase (now).** Search the archives, port what's useful, leave provenance comments naming the source path.
2. **Evaluation phase (during porting).** For each archive crate touched, decide explicitly: *ported*, *will-port-later*, or *not-portable* (chip/CPU/cycle-accuracy code that needs full rewrite).
3. **Cleanup phase (after porting waves complete).** Once a class of work is done — for example, "all Spectrum format crates have been evaluated" — delete the archive crates that fall into the *not-portable* bucket and the *ported* bucket from whichever archive(s) they came from. This keeps the archives focused on what's still actively useful and avoids ambiguity about whether something has been migrated.

### Archives are "dead" — internal builds may break

A clarification added 2026-04-09 after the first cleanup attempt: **the archives are treated as *dead* reference material from this point on, not as buildable workspaces.** It is acceptable for an archive's internal Cargo dependency graph to be broken by a deletion. If you delete crate `foo` from an archive and three other crates in the same archive depended on `foo`, those three will fail `cargo build` — and that is fine. Nothing in any archive is expected to compile.

This unblocks a class of cleanups that would otherwise be impossible: deleting a ported leaf crate without also deleting (or evaluating) every consumer. Without this clarification, every cleanup grew into a multi-crate audit; with it, we can delete in tight scope and let the broken refs accumulate as evidence of "this archive is no longer self-consistent."

### Cleanup discipline

- **Audit before deleting non-trivial chunks.** "Chip/CPU/ULA code" is reference material we might want to consult later — read before deleting anything in `cpu-*`, `machine-*`, `*-ula`. For *fully-ported* leaf crates (where we have a verified replacement in the new workspace) the audit is just "is the new version equivalent or better."
- **Prefer batched cleanups** to incremental ones. After a wave of porting (e.g. "Spectrum content pipeline + BASIC injection"), do one cleanup pass that removes everything that wave evaluated and decided against. Avoid spread-out single-file deletions that lose audit-trail coherence.
- **Cleanups are commits, not silent deletes.** A cleanup commit message lists which crates were removed, which were ported, and which were explicitly judged not-portable. Future readers can reconstruct *why* a thing is gone.
- **`target/` deletions don't need a commit** — they're build cache, gitignored, regenerable. They're free disk reclaim and should happen routinely.
- **When all three archives are empty (or contain only chip/CPU/cycle-accuracy code we judged as reference-only), retire the archives entirely.** Delete the directories. Note the retirement in this decision record's version history.

### What to keep until the very end

- The **chip/CPU/ULA crates** that we read for ISA reference but don't port. These have value as documentation even if they can't be lifted directly. Keep them in `Emu198x-archive` until the equivalent new crate exists with comparable test coverage.
- **System implementations we haven't touched yet.** If a wave of porting only covers Spectrum, leave the C64/NES/Amiga material alone for the next wave.

## Per-subsystem source map

Per-chip decisions about which archive to port from. Consult this before starting a chip port so the next session doesn't have to re-derive it.

### C64

| Subsystem | Primary source | Cross-reference | Status |
|---|---|---|---|
| `mos-6502` CPU | `Emu198x-archive-april2026/crates/cpu-6502/` *(deleted, see archive commit `bd942d9`)* | — | **Ported.** Emu198x commit `2d42f8b` (pipelined pin bus). Foundation in commit `25cd870`. 16 tests. |
| `mos-cia-6526` | `Emu198x-archive/crates/mos-cia-6526/` *(deleted, see archive commit `6bdc617d3a`)* | `Emu198x-backup/systems/c64/src/cia.rs` | **Ported.** Emu198x commit `cf7d0e7`. 23 tests. The archive's `external_a`/`external_b` pin separation made the pin-port straightforward. |
| `mos-sid-6581` | `Emu198x-archive/crates/mos-sid-6581/` *(deleted, see archive commit `6bdc617d3a`)* | `Emu198x-backup/systems/c64/src/sid.rs` | **Ported.** Emu198x commit `49128bf`. 9 tests. Kept the archive's four-file split and reSID lookup tables. |
| `mos-vic-ii` | `Emu198x-archive/crates/mos-vic-ii/` *(deleted, see archive commit `6bdc617d3a`)* | `Emu198x-backup/systems/c64/src/vic_ii.rs` | **Ported.** Emu198x commit `7ac5a65`. 23 tests. First chip with cross-chip bus visibility (BA pin + IRQ pin); VRAM access is a `VicMemory` trait, mirroring the Spectrum ULA's precedent. |
| `format-commodore-c64-bas` | `Emu198x-archive/crates/format-commodore-c64-bas/` *(deleted, see archive commit `6bdc617d3a`)* | — | **Ported.** Emu198x commit `25cd870` (C64 phase 1). |
| `format-commodore-c64-prg` | `Emu198x-archive/crates/format-commodore-c64-prg/` *(deleted, see archive commit `6bdc617d3a`)* | — | **Ported.** Emu198x commit `25cd870` (C64 phase 1). |
| `format-commodore-c64-d64/-gcr/-tap` | `Emu198x-archive/crates/format-commodore-c64-{d64,gcr,tap}/` | — | Not yet ported. Same lift-and-shift shape as the other format crates. |
| `machine-commodore-c64` | `Emu198x-archive-april2026/crates/machine-commodore-c64/` (cleaner module split) plus `Emu198x-archive/crates/machine-commodore-c64/` (more complete logic) | `Emu198x-backup/systems/c64/src/{c64,bus}.rs` | **Ported.** Emu198x commit `a398d4c`. Bus routing rewritten for pin-level interface; CIA keyboard scan, VIC-II bank select, IRQ/NMI routing all working. Boots KERNAL to READY. prompt (frame 108). 12 tests. |

### NES

| Subsystem | Primary source | Cross-reference | Status |
|---|---|---|---|
| `mos-6502` (2A03 variant) | `Emu198x-archive-april2026/crates/cpu-6502/` *(deleted)* | — | **Ported.** 2A03 variant (`new_2a03()`, `decimal_disabled: true`) added in Emu198x commit `dd76911`. 2×2.47M Tom Harte validated (stock 6502 + NES). |
| `ricoh-ppu-2c02` | `Emu198x-archive/crates/ricoh-ppu-2c02/` *(to be deleted)* | `Emu198x-backup/systems/nes/src/ppu.rs` | **Ported.** Emu198x commit `2f3b287`. 20 tests. Interface rewritten (closures → `&mut dyn Mapper`, `nmi` as public field, A12 direct notification). Rendering logic lifts intact. |
| `format-nintendo-nes-ines` (NROM/MMC1/UxROM/CNROM/MMC3/AxROM/BxROM) | `Emu198x-archive/crates/format-nintendo-nes-ines/` | — | **Partially ported.** Started in Emu198x commit `dd76911`; mapper coverage now includes NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), AxROM (7), and BxROM/BNROM (34). 56 tests. Remaining mappers stay in archive. |
| `ricoh-apu-2a03` | `Emu198x-archive/crates/ricoh-apu-2a03/` *(deleted, archive commit `3d8c51d60b`)* | `Emu198x-backup/systems/nes/src/apu/` | **Ported (clean lift).** Emu198x commit `df54f22`. 21 tests pass unchanged. |
| `machine-nintendo-nes` | `Emu198x-archive/crates/machine-nintendo-nes/` *(to be deleted)* | — | **Ported (rewrite).** Emu198x commit `75d8c2e`. Written from scratch against nes-clock-topology.md — the old version used CPU-driven batching. 12 tests + nestest 8991/8991. |
| `emu-nintendo-nes` | `Emu198x-archive/crates/emu-nintendo-nes/` | — | Not ported. Frontend/windowing patterns, reference for runtime. |
| `emu-nintendo-nes-wasm` | `Emu198x-archive/crates/emu-nintendo-nes-wasm/` | — | Not ported. WASM bindings, no equivalent. |

**Reading the deleted paths**: after a port, the primary source paths above point to files that no longer exist as live files on disk — the archive cleanup commits (`6bdc617d3a` for the March archive, `bd942d9` for the April archive) removed them. To read the original source, check out the relevant commit in the archive's git history: `cd ~/Projects/Emu198x-archive && git show HEAD~1:crates/<crate>/src/lib.rs` (or similar for the April archive). The cleanup commit messages in each archive list exactly what went where.

### Note on the backup's usefulness

The initial decision entry (below) described `Emu198x-backup` as *"probably nothing useful for the current rewrite."* That was wrong. The backup has functional `cia.rs` / `sid.rs` / `vic_ii.rs` / `c64.rs` implementations in `systems/c64/src/` that are worth cross-referencing during chip ports — especially when the March archive's behaviour is unclear and a second implementation helps sanity-check intent. The "Best for" column at the top of this page has been updated; leaving this note here as an audit trail of the correction. The backup was **consulted during every C64 chip port in this phase** (CIA, SID, VIC-II) even though the archive was the primary source — which is exactly the role it should play until it too retires.

## Cleanup history

| Date | What was removed | What was ported | Disk freed |
|---|---|---|---|
| 2026-04-09 | `Emu198x-archive/crates/format-sinclair-zx-spectrum-bas/` (the entire crate); `Emu198x-archive/target/` (build cache); `Emu198x-backup/target/` (build cache, was 99.8% of the entire backup directory). | `format-sinclair-zx-spectrum-bas` to the new workspace as Phase 1.10 (Emu198x commit `7d05f96`), with the corpus test and provenance comment added. | ~47 GB |
| 2026-04-09 | `Emu198x-archive/crates/{mos-cia-6526, mos-sid-6581, mos-vic-ii, format-commodore-c64-bas, format-commodore-c64-prg}/` (archive commit `6bdc617d3a`). `Emu198x-archive-april2026/crates/cpu-6502/` (archive commit `bd942d9`). | All six crates have passing test suites in the new workspace: `mos-6502` (Emu198x `2d42f8b`, 16 tests), `mos-cia-6526` (`cf7d0e7`, 23 tests), `mos-sid-6581` (`49128bf`, 9 tests), `mos-vic-ii` (`7ac5a65`, 23 tests), and the two `format-commodore-c64-*` crates in C64 phase 1 (`25cd870`). | ~350 KB (code only; archive `target/` already cleaned up in the previous pass) |
| 2026-04-10 | `Emu198x-archive/crates/{ricoh-ppu-2c02, machine-nintendo-nes}/` (archive commit `355a702e51`). | `ricoh-ppu-2c02` ported (Emu198x `2f3b287`, 20 tests, interface rewritten for pin-level). `machine-nintendo-nes` rewritten from scratch (Emu198x `75d8c2e`, 12 tests + nestest 8991/8991). Kept: `ricoh-apu-2a03` (APU not ported), `format-nintendo-nes-ines` (47 mappers not ported), `emu-nintendo-nes{,-wasm}` (frontend/WASM). | ~3.7 KB |
| 2026-04-10 | `Emu198x-archive/crates/ricoh-apu-2a03/` (archive commit `3d8c51d60b`). | `ricoh-apu-2a03` clean lift (Emu198x `df54f22`, 21 tests pass unchanged). Remaining NES crates in archive: `format-nintendo-nes-ines` (47 mappers), `emu-nintendo-nes{,-wasm}` (frontend/WASM). | ~1.8 KB |

## Examples

**Port that worked cleanly (Phase 1.10, BASIC tokeniser):**
- Source: `Emu198x-archive/crates/format-sinclair-zx-spectrum-bas/`
- Target: `crates/format-sinclair-zx-spectrum-bas/`
- Adaptation: edition bump, workspace deps, drop the `bas2tap` binary (Phase 1 framing rules out tape exporters).
- Result: 13 unit tests passed unchanged on first build. ~5 minutes of work.

**Port that needs adaptation (Phase 1.10, in-RAM BASIC injection):**
- Source: `Emu198x-archive/crates/machine-sinclair-zx-spectrum/src/spectrum.rs::load_basic`
- Target: `crates/runtime-sinclair-zx-spectrum/src/lib.rs::Machine::load_basic`
- Adaptation: the old code lived on a single `Spectrum` struct; the new code has a 7-variant `MachineInner` enum. The PROG sysvar address is the same on every variant, so the algorithm ports unchanged but the memory accessor is the new runtime's API. ~20 minutes.

## Drift triggers

**Phrases that signal you should re-read this entry:**

- "Let's write a tokeniser from scratch" — check `format-*` in the archives first.
- "We need an MCP handler for X" — check `emu-*/src/mcp.rs` in the Mar 23 archive.
- "How do we capture frames to a video?" — check `emu198x-shell::capture` and `emu-capture` in april2026.
- "I'll just rewrite this small thing instead of finding it" — small things add up. Check first.

**Phrases that signal you should NOT port:**

- "This Z80 implementation is great, let's grab it" — it's the wrong cycle model.
- "The old machine struct already does X" — the bus loop is rewritten; verify what X actually depends on before porting.
- "This ULA implementation already handles contention" — it does, against the old timing model. Read for reference, rewrite for the new rules.

## Related

- [`RULES.md`](../../RULES.md) items 25–27 — the binding form of this rule.
- [Fresh start rationale](fresh-start-rationale.md) — why we rewrote and what carried forward.
