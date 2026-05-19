# Decision: October Catalogue (40-title curated bench)

**Date:** 2026-05-04

## The decision

**Amended 2026-05-06.** This decision originally framed all four systems as October-public. It now separates public October scope from the engineering quality bar. See Log at the bottom for the rationale.

**Public October launch (Crash! Live):** Spectrum only. Spectrum is the system Code198x ships, the system Crash! Live's audience cares about, and the system whose screenshots and videos Code198x's QR-code visitors will see. **Spectrum SOLID** is the October-public goal — locked criteria in the next section.

**Engineering quality bar:** A curated 10-title catalogue per system across Spectrum, C64, NES, and Amiga — 40 titles total. Implemented as a shared crate `emu198x-catalogue`, driven by per-system TOML manifests, with frame and audio assertions hashed via **xxhash64**. One harness, four manifests, one cross-system green/red grid as the test output. The bar is the same for every system; only the public deadline differs.

**Sequencing:** Spectrum SOLID first. Then C64, NES, Amiga in that priority order, as engineering bar without October hard deadline. The four-system catalogue infrastructure is built once and applied to whichever system is up next; nothing forces all four to be hit by October.

**Status of non-Spectrum systems for October:** engineering progress, no public deadline. C64/NES/Amiga catalogues pass when they pass.

## October bar definition

**Catalogue-ready (per non-Spectrum system)** is the engineering quality bar for C64, NES, and Amiga: all 10 catalogue entries pass — boot waypoint, scripted-input state-change, audio-window assertion. No October deadline.

**Spectrum SOLID** is the October-public deliverable. Locked 2026-05-06. All eleven criteria below are binding:

1. **Catalogue.** 10 entries per in-scope variant. With 8 variants in scope and reasonable title overlap (a 48K-universal title produces an entry on every compatible variant; +3-disk-only titles are unique), this is roughly 80 manifest entries from a smaller set of unique titles. Each entry passes boot waypoint, scripted-input state-change, and audio-window assertion. Frame and audio hashes match manifest.

2. **Variants in scope.** 16K, 48K, **Spectrum+**, 128K, +2, +2A, +2B, +3 — each boots reliably and passes its 10 catalogue entries. The Spectrum+ is electrically identical to the 48K (same ROM/RAM/ULA/keyboard matrix); it's included in SOLID for catalogue regression coverage and variant-selector consistency, not because it differs at the chip level. **Deferred to post-October:** Pentagon, Scorpion, TC2048, TC2068, TS2068.

3. **Formats.** TAP, TZX, SNA, Z80 across all eight in-scope variants; DSK/EDSK on +3. TR-DOS and DOCK formats defer with Pentagon and Timex respectively.

4. **Pipeline.** `emu198x-spectrum` is the single binary for the Spectrum family, with three modes: `--ui` (default, native interactive), `--script` (headless capture), `--mcp` (MCP server). Byte-stable output for the same input. For every Code198x curriculum unit with an associated screenshot or video per Code198x's [Definition of Done](../../../Code198x/knowledge/curriculum/revamp.md#definition-of-done-per-unit), the pipeline succeeds reliably.

5. **MCP.** Spectrum MCP server functional and exercised by at least one Code198x skill.

6. **CRT filter.** Functional, with at least one acceptable preset. Final taste-tuning is post-October.

7. **Native UI.** Machine variant selection across the 8 in-scope variants, load snapshot/tape/disk, run/pause/reset, save/load state, volume, window sizing. Anything beyond that is post-launch.

8. **Save state.** Postcard round-trip on every variant + title combination in the catalogue. Not "one per family" — every cell of the variant × title grid round-trips.

9. **Code quality.** No `.unwrap()` and no stubs in Spectrum-side library code.

10. **Regressions.** Tom Harte 100%, ZEXDOC, ZEXALL stay green.

11. **Code coverage.** All Spectrum-specific crates achieve ≥90% line coverage measured by `cargo-llvm-cov`. CI gate enforces the threshold. Branch coverage measured for visibility but not gated. Scope is Spectrum-specific: `zilog-z80`, `ferranti-ula-6c001e`, `sinclair-ula-7k010e`, `amstrad-ula-40077`, `gi-ay-3-8912`, `nec-upd765a`, `format-amstrad-dsk`, all `format-sinclair-zx-spectrum-*` crates, all `machine-sinclair-zx-spectrum-*` crates, `runtime-sinclair-zx-spectrum`, `common-sinclair-zx-spectrum`, `beta-disk-interface`, and the consolidated `emu198x-spectrum` binary. Shared infrastructure (`emu198x-catalogue`, `emu198x-shell`, `emu198x-native-video`) is measured for visibility but not gated by SOLID. **Data-only exemption (added 2026-05-06):** a Spectrum-specific crate that contains only type definitions (no functions, no logic) is exempt — line coverage on type definitions is undefined. Round-trip tests in consumer crates provide the regression coverage instead. Currently exempt: `format-sinclair-zx-spectrum-snapshot`.

The single-binary pattern (`emu198x-<family>` with `--ui`/`--script`/`--mcp` modes) propagates to C64, NES, and Amiga as those systems mature. No timeline lock for the propagation; it follows the Spectrum-first sequencing.

This sits between two rejected alternatives:

- **Boots-to-first-screen only.** Too thin. Three of the four systems already pass this (the fourth was Amiga before Phase 0 closed). No defensible "this system works" claim from a single screenshot.
- **Unbounded compatibility list.** Too broad. The NES already has a 627/629 ROM smoke matrix; adding C64/Spectrum/Amiga compat lists at that scale would absorb the entire ~75-session budget without producing a curated reviewable artifact.

The 10-title curated bench is the defensible middle: every title is named, every assertion is tracked, and the catalogue itself becomes a reviewable PR artifact.

## Why 10 titles per system, not 5 or 20

- **5** covers the canon but misses every gap surfaced in the [Phase 1 inventory](../log.md): PAL/NTSC parity, model-variant coverage (+3 disk, Pentagon TR-DOS), specific mappers (MMC5), WB 2.04 application proof.
- **20** is more than the ~75-session budget can absorb at depth, given the catalogue is one of several Phase 2 work-streams (MCP wrapper, CRT per-system tuning, snapshot mapper-state policy).
- **10** lets each system cover (a) a canonical first-screen waypoint, (b) input/play state-change, (c) audio validation, plus (d) two or three gap-targeted picks (NTSC, +3 disk, MMC5, WB 2.04 application, etc.). Honest depth without scope sprawl.

## Why a shared crate over per-system

Two harness shapes were on the table:

- **Per-system** — extend each system's existing `tests/` directory with per-title hand-written `#[test]` files, mirroring the C64 tape regressions and Amiga `boot_invariants.rs` patterns.
- **Shared crate with manifest** — `emu198x-catalogue` depends on all four runtime crates, reads TOML manifests, runs every entry through one harness, reports a cross-system grid.

The shared crate wins for three reasons:

1. **The catalogue is the deliverable.** Four TOML files reviewed in PR are easier to audit than 40 hand-written Rust files. Renaming a hash algorithm is one regex over the manifests, not 40 file edits.
2. **The cross-system green/red grid is the question to answer.** "Does each system work?" maps directly to "show me a 40-cell grid." The shared crate produces this as natural test output; the per-system shape needs custom CI tooling layered on top.
3. **The runtime crates already converge.** Every system has `MachineCore`, snapshot, framebuffer, audio buffer, scripted input via the shared headless runner pattern. The shared crate formalises an interface that's mostly already implicit.

The escape hatch: if the runtime interfaces don't actually converge cleanly when the harness lands, fall back to per-system tests without throwing work away — the assertion library and CLI tool stay useful as test fixtures.

## Implementation choices

### xxhash64 for frame and audio hashes

Frame and audio hashes are integrity checks, not crypto. xxhash64 is fast, tiny, and the `twox-hash` crate is mature. The 64-bit hash is short enough to read at a glance in the manifest, long enough that collisions are not a practical concern at 40-entry scale.

Rejected: blake3 (overkill, we don't need crypto guarantees), CRC32 (too short for the manifest's lifetime).

### TOML for the manifest

Every other config file in the workspace is TOML. Editor support is universal. The schema is mostly tabular (one entry per title, fixed fields), which TOML's table-of-tables shape handles naturally.

Rejected: JSON (no comments), RON (unfamiliar to most readers, no advantage at this schema complexity), YAML (whitespace-sensitive and the workspace doesn't use it).

### Manifest co-located in the crate

The manifests live at `crates/emu198x-catalogue/manifest/{spectrum,c64,nes,amiga}.toml`.

The manifest is **tested code, not documentation** — it's executed by the harness at test time. Co-location keeps `cargo test -p emu198x-catalogue` self-contained and lets the harness path-resolve manifests without configuration.

Rejected: `knowledge/catalogue/*.toml` (reads as documentation, but the harness would need explicit path configuration; manifests would also drift away from the runner's actual schema).

## Catalogue contents

The 40 titles, selection criteria, and per-system coverage analysis are in:

- `crates/emu198x-catalogue/manifest/spectrum.toml` — 10 entries covering 48K, 128K, +3 (DSK), Pentagon (TR-DOS), Timex (DOCK)
- `crates/emu198x-catalogue/manifest/c64.toml` — 10 entries covering D64 / TAP / NTSC; cartridges deferred until subsystem lands
- `crates/emu198x-catalogue/manifest/nes.toml` — 10 entries covering NROM / UxROM / MMC1 / MMC3 / MMC5 / AxROM / PAL
- `crates/emu198x-catalogue/manifest/amiga.toml` — 10 entries covering KS 1.3 boot-block + AmigaDOS, KS 2.04 application, NTSC

Per-system coverage notes and provenance live in each manifest's leading comments; this decision doc does not duplicate the title list.

## Staged rollout

| Step | Scope | Estimated sessions |
|---|---|---|
| 1 | This decision doc + index entry + log brief | 0.5 (this session) |
| 2 | Stand up `emu198x-catalogue` crate skeleton + manifest schema + Manic Miner end-to-end (proves the shape) | 1.5 |
| 3 | CLI hash-capture tool — `cargo run -p emu198x-catalogue -- capture --entry <id>` runs entry, prints frame/audio hashes for paste-into-manifest | 0.5 |
| 4 | Fill remaining 9 Spectrum entries against the working harness | ~3 |
| 5 | Validate harness on second system (NES — strongest existing infra) | ~3 |
| 6 | C64, then Amiga | ~6 |

Total: ~14–15 sessions. ~20% of the ~75-session budget. Leaves ~60 sessions for content depth, MCP wrapper, CRT per-system tuning, snapshot mapper-state policy, and slack.

## Drift triggers

Catalogue drift comes dressed as scope creep or "while I'm in here" tidying. If I'm about to suggest any of these, stop and re-anchor on this decision.

**Scope drift to reject:**

- Adding an 11th title to any system's catalogue without removing one
- Adding a 5th system to the catalogue (Game Boy and Dragon are post-October per the [product roadmap](product-roadmap.md))
- Replacing curated titles with a smoke matrix ("but we already have 627 ROMs!" — that's the smoke matrix; this is the curated bench, they answer different questions)
- Cutting catalogue titles to free up time ("we'll add it post-October")
- Moving the catalogue out of the test surface ("it's actually a benchmarking tool" — no, it's a regression bench)

**Implementation drift to reject:**

- Writing per-title hand-rolled Rust tests outside the manifest harness
- Adding frame/audio assertion formats other than xxhash64 ("blake3 is more secure" — we don't need security)
- Embedding expected hashes anywhere other than the TOML manifest
- Manifest in JSON, RON, YAML — TOML only
- Per-system catalogue crates instead of one shared crate
- A `tests/catalogue/` directory inside any per-system runtime crate

**Phrases that signal drift:**

- "Let's just write a quick test for X outside the manifest"
- "We can drop title Y from the catalogue"
- "Maybe a custom format would handle this case better"
- "The catalogue is too restrictive, let's run the smoke matrix instead"
- "While I'm in here, let me add a test for Z"

**Public-vs-bar drift to reject (added 2026-05-06):**

- Promoting C64/NES/Amiga catalogue completion to October-public ("Crash! Live needs C64 too")
- Treating non-Spectrum catalogue progress as October-launch progress when reporting status
- Working on C64/NES/Amiga catalogue passes before Spectrum SOLID is closer to done — the sequencing is Spectrum first, others after
- Inferring an October deadline for non-Spectrum catalogues from the original framing in this doc — read the amended top section and the Log

**Spectrum SOLID drift to reject (locked 2026-05-06):**

- Promoting deferred variants (Pentagon, Scorpion, TC2048, TC2068, TS2068) to in-scope without an explicit decision update
- Adding TR-DOS or DOCK format support to October scope (deferred with their respective variants)
- Reintroducing real-hardware validation as a SOLID requirement (explicitly excluded — frame hashes are the regression bar)
- Splitting `emu198x-spectrum` back into separate UI and headless binaries (the single-binary pattern is binding and propagates to other families)
- Treating "catalogue passes" alone as SOLID — pipeline reliability, MCP, native UI, save-state, and CRT filter are equally binding criteria
- Lowering the per-variant catalogue bar below 10 entries
- Decoupling the Code198x curriculum DoD from Emu198x pipeline reliability
- Lowering the 90% line-coverage threshold for any Spectrum-specific crate without an explicit decision update
- Excluding Spectrum-specific crates from the coverage measurement to inflate the gated number
- Treating coverage as aspiration rather than an enforced CI gate

**What to do when triggered:** the October-public bar is Spectrum SOLID. The engineering quality bar is the 40-title catalogue. Variations are user decisions, not mine. Raise scope or shape concerns explicitly; do not silently expand or contract.

## Log

| Date | Event |
|---|---|
| 2026-05-04 | Decision created. 40-title catalogue across four systems framed as the October launch bar. |
| 2026-05-06 | **Amended.** Codex's evaluation of Code198x and Emu198x surfaced a tension with the Code198x launch spec (locked earlier the same day): Code198x is Spectrum-only at October, while this decision committed Emu198x to four-system catalogue completion by October. Resolved by separating public October scope from the engineering quality bar. The 40-title catalogue stays as a quality bar; only Spectrum is publicly October-bound (Spectrum SOLID). C64/NES/Amiga catalogues progress on engineering merit without October deadline. Sequencing locked: Spectrum SOLID first, then C64, NES, Amiga in priority order. |
| 2026-05-06 | **Spectrum SOLID locked.** Ten binding criteria (catalogue, variants, formats, pipeline, MCP, CRT, UI, save state, code quality, regressions). 7 variants in scope (16K, 48K, 128K, +2, +2A, +2B, +3); Pentagon, Scorpion, TC2048, TC2068, TS2068 deferred. 10 catalogue entries per variant — roughly 70 manifest entries via title overlap. Single `emu198x-spectrum` binary with `--ui`/`--script`/`--mcp` modes (pattern propagates to other families later). Real-hardware validation explicitly dropped (frame hashes carry the regression bar). Code198x curriculum DoD coupled to Emu198x pipeline reliability — every unit with a screenshot/video must capture cleanly. |
| 2026-05-06 | **Spectrum+ added to SOLID.** 7 → 8 in-scope variants. Spectrum+ is electrically identical to the 48K but treated as a first-class variant for catalogue regression coverage and variant-selector consistency. Manifest entry count rises from ~70 to ~80. The catalogue duplicates 48K's title set against the Spectrum+ crate so any future drift between the `-48k` and `-plus` machine crates surfaces immediately. |
| 2026-05-06 | **Criterion 11 added: code coverage.** ≥90% line coverage on all Spectrum-specific crates, measured by `cargo-llvm-cov`, gated in CI. Branch coverage measured but not gated. Shared infrastructure crates (catalogue, shell, native-video) measured for visibility but not gated by SOLID. Phase 1 work expands to include baseline measurement and CI gate setup. |
| 2026-05-06 | **D7 — Snapshot types extracted to dedicated crate.** New `format-sinclair-zx-spectrum-snapshot` crate owns `Snapshot` (renamed from `Z80Snapshot`) and `SnapshotModel`. Format crates (z80, sna) and all consumer machine crates updated. Data-only exemption added to criterion 11 (data-only crates have no executable code to count; round-trip tests in consumers cover the regression need). |

## Related

- [Product roadmap](product-roadmap.md) — Spectrum-public + four-system engineering bar, must-haves, post-October waves
- [Save state format](save-state-format.md) — adjacent test-infrastructure decision; postcard snapshots already have round-trip proofs
- [Runtime internal shape](runtime-internal-shape.md) — the per-runtime four-module shape the catalogue harness consumes
- [Phase 1 inventory + Phase 2 plan](../log.md) — the gap analysis the catalogue addresses
- [Code198x October launch spec](../../../Code198x/knowledge/decisions/october-2026-launch-spec.md) — the cross-project Spectrum-only October scope this decision aligns with
