# Decision: October Catalogue (40-title curated bench)

**Date:** 2026-05-04

## The decision

The October 2026 launch bar for each of the four systems (Spectrum, C64, NES, Amiga) is a **curated 10-title catalogue per system, 40 titles total**. The catalogue is implemented as a shared crate `emu198x-catalogue`, driven by per-system TOML manifests, with frame and audio assertions hashed via **xxhash64**. One harness, four manifests, one cross-system green/red grid as the test output.

## October bar definition

"October-ready" for any of the four systems means: **all 10 catalogue entries pass — boot waypoint, scripted-input state-change, and audio-window assertion.**

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

Rejected: `wiki/catalogue/*.toml` (reads as documentation, but the harness would need explicit path configuration; manifests would also drift away from the runner's actual schema).

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

**What to do when triggered:** the October bar is the curated 40-title catalogue. Variations are user decisions, not mine. Raise scope or shape concerns explicitly; do not silently expand or contract.

## Related

- [Product roadmap](product-roadmap.md) — names the four October systems and the must-haves
- [Save state format](save-state-format.md) — adjacent test-infrastructure decision; postcard snapshots already have round-trip proofs
- [Runtime internal shape](runtime-internal-shape.md) — the per-runtime four-module shape the catalogue harness consumes
- [Phase 1 inventory + Phase 2 plan](../log.md) — the gap analysis the catalogue addresses
