# October Run-Up Plan

> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.


**Status:** Working plan
**Date:** 2026-04-28
**Supersedes:** nothing — this is a tactical follow-up to [`2026-04-12-emulator-suite-coherent-development-plan.md`](2026-04-12-emulator-suite-coherent-development-plan.md), not a replacement for it.

## Purpose

Sequence the highest-impact remaining work for the October 2026 CRASH! Live launch across the four anchor families (Spectrum, C64, NES, Amiga). The April 12 coherent plan defines *what* the suite is building. This plan defines *what to do next* across the next ~12-14 weeks, in priority order, with each item phrased as **verify first, then either build or document**.

## Working assumption: "documented or it isn't done"

Several items below may already be partially or fully implemented. Where something turns out to be done but undocumented, the work item shifts from *build* to *write it down* — the README, the wiki index, and the per-system overview pages should reflect reality without the reader needing to read source.

Each item begins with a verification step. The phrasing "if absent" or "if incomplete" guards against silently rebuilding what's already there.

This is itself the first lesson: `knowledge/log.md` last has a substantial entry on 2026-04-10, with ~80 commits unrecorded since. The Track 0 governance discipline from the April 12 plan needs reactivation as part of Phase A.

## Source-of-truth hierarchy

When this plan disagrees with anything, use:

1. `RULES.md`
2. `knowledge/decisions/`
3. this plan
4. the April 12 coherent plan (architectural framing)
5. README and wiki overview pages (state claims)

## Out of scope

These are deliberately excluded; push back if any belong on the list.

- **Anything Dragon-shaped.** Codex is working through Dragon items end to end and will not be redirected until Dragon work is complete.
- **Wave 2 systems.** Atari 2600, BBC Micro, MSX, Master System are forbidden before October per [`product-roadmap.md`](../../knowledge/decisions/product-roadmap.md).
- **Game Boy CGB / boot ROM / link cable / long-tail cartridges.** README owns these as later work.
- **C64 1541 write path.** Substantial scope; not blocking read-only curriculum.
- **Spectrum +3 end-to-end disk verification.** Niche; +2A and below are the dominant cases.
- **CPU internals.** No CPU regressions, no abstraction-level changes. Tom Harte gates stay green.

---

## Phase A — Amiga parity and regression net (4-5 weeks)

The Amiga is the documented cut candidate, four of the five "done criteria" in [`amiga-architecture-review.md`](../../knowledge/decisions/amiga-architecture-review.md) are still open, and it's the only anchor missing serde snapshots. Stabilising it is the single most leveraged thing on the board.

### A.0 — Wiki log catch-up

**Verify.** `git log --since="2026-04-10" --pretty=format:"%h %s"` against `knowledge/log.md` entries.
**Build.** Add log entries for the gaps: NES mapper expansion (UxROM through MMC5/VRC2a/Action 53/Camerica/NINA), Game Boy native verifier + battery saves, Amiga native verifier + audio + mouse + joystick, shared `wgpu` presenter migration, native APU channel controls, the Dragon line of work in summary form (Codex owns the detail).
**Done when.** `knowledge/log.md` reads coherently from 2026-04-10 to today; `knowledge/index.md` references match what the workspace actually contains.
**Sequence first.** Cheapest item. Establishes the context that subsequent items rely on.

### A.1 — Amiga snapshots

**Verify.** Search the Amiga chip stack for `#[derive(... Serialize, Deserialize ...)]` and any `Snapshot`-shaped types. Items: `motorola-68000`, `commodore-agnus-ocs`, `commodore-denise-ocs`, `commodore-paula-8364`, `mos-cia-8520`, `commodore-gary`, `commodore-amiga-autoconfig`, `peripheral-commodore-amiga-floppy`, `peripheral-commodore-amiga-keyboard`, `machine-commodore-amiga-ocs`, `runtime-commodore-amiga`. Cross-reference with what C64/NES/Spectrum already do.
**Build (if absent).** Add serde derive across the chip stack. Implement the runtime envelope. Add a postcard round-trip integration test that takes a snapshot mid-Workbench-boot, restores it, and verifies the resulting frame is bit-identical for at least 60 ticks afterwards.
**Document.** Update README's "Notably not claimed yet" list and `systems/commodore-amiga.md`.
**Done when.** `Amiga snapshot support exists` is no longer a README disclaimer; round-trip test runs in `cargo test --workspace`.

### A.2 — Boot-invariant test suites across all four anchors

**Verify.** `find crates -name 'boot_invariants.rs'` and review existing diagnostic examples per anchor.
**Build.** Promote diagnostic examples to standing tests, one waypoint per known-good state:
- Spectrum: 48K → ©1982, 128K → menu, +2A → menu, Pentagon → ROM
- C64: KERNAL → `READY.`, datasette → `Bruce Lee` LOADING, 1541 → `Aztec Challenge` instructions, joystick → `Bomb Jack` fire
- NES: nestest pass, SMB → title screen, MMC1 → `Zelda` title, MMC3 → `SMB3` title
- Amiga: Kickstart 1.3 → insert-disk, Workbench 1.3 → desktop, seven-disk-DMA-arms (catches the CIA double-read regression), bootblock decodes to DOS magic (catches the WORDSYNC fix regression)
**Done when.** Each anchor has a `tests/boot_invariants.rs` in its runtime crate; suite runs on every `cargo test --workspace`.
**Sequence before A.3.** The seam refactors in A.3 require a regression net before they touch `service_cpu_bus`.

### A.3 — Amiga seams 1 and 4 (`service_cpu_bus` + byte-lane conventions)

**Verify.** Read `machine-commodore-amiga-ocs/src/lib.rs` (currently ~3000 lines per the architecture review). Confirm the four byte-lane conventions and the CIA-A even-address landmine still exist as described.
**Build.** Implement `BusTransaction { addr, fc, is_read, is_word, data }` and `BusResponse { Byte(u8), Word(u16), Float }`. Refactor each chip-select arm to produce `BusResponse`. Single dispatcher applies one rule: byte responses go in low byte, `Float` returns `0xFFFF`. CIA-A even-address case must actually read the CIA.
**Done when.** `service_cpu_bus` is under 200 lines; all chip-select arms produce `BusResponse`; A.2 boot invariants stay green.
**Risk.** Largest refactor in this plan. Land A.2 first.

### A.4 — Amiga seam 2 (Paula owns disk read DMA)

**Verify.** Read `peripheral-commodore-amiga-floppy`, `commodore-paula-8364`, and `machine-commodore-amiga-ocs::service_disk_dma_slot`. Confirm WORDSYNC suppression still lives in the machine layer.
**Build.** Add `tick_disk_dma_slot(&mut self, fetch: impl FnOnce() -> Option<u16>) -> Option<DiskDmaWrite>` to Paula. Move WORDSYNC, sync-stripping, DSKBYTR/DSKDATR updates, and DSKBLK interrupt into Paula. Machine layer becomes four lines of glue.
**Done when.** Paula owns disk read end-to-end; `bootblock_decodes_to_dos_magic` from A.2 stays green; Workbench 1.3 still reaches desktop.

### A-phase exit criteria

- All four anchors have boot-invariant test suites in CI.
- Amiga snapshots are first-class; round-trip test passes.
- `service_cpu_bus` is under 200 lines.
- Paula owns disk read DMA; machine layer is glue.
- `knowledge/log.md` is current.
- The architecture-review document moves from `Proposed (draft for review)` to `Implemented`.
- **Coverage:** workspace at 78% line / 65% branch; Cov-1 and Cov-2 (mos-6502 investigations) closed.

---

## Phase B — October must-haves (4-5 weeks)

The April 12 plan and [`product-roadmap.md`](../../knowledge/decisions/product-roadmap.md) name three October-blocking infrastructure items: **capture pipeline**, **CRT filter**, **serialisation traits**. A.1 covered the third. This phase covers the first two plus the MCP control surface that Code198x will consume.

### B.1 — CRT filter audit and parity

**Verify.** `grep -r "raw\|lcd\|crt" crates/emu198x-native-video crates/emu198x-spectrum crates/emu198x-c64 crates/emu198x-nes crates/emu198x-amiga crates/emu198x-game-boy`. README only explicitly claims `raw/lcd/crt` for Game Boy.
**Build (if missing).** Confirm the shared `wgpu` presenter actually exposes a CRT shader (not only `raw`/`lcd`). Wire the CRT mode into Spectrum, C64, NES, and Amiga native windows. Common keybind to cycle modes.
**Document.** Update each system's README section and `systems/<family>/overview.md` to state which presentation modes are available.
**Done when.** All four anchor verifier windows expose `raw`/`crt` (and `lcd` where appropriate) through a documented hotkey.

### B.2 — Video capture pipeline

**Verify.** Inspect `emu198x-shell` and the per-family scripts for any video output beyond per-frame PNG. Audio capture is already known to exist headless.
**Build (if absent).** Add an mp4/webm sink to `emu198x-shell`. Default to invoking ffmpeg as a sidecar (boring choice; no Rust video-encoder dependency rabbit hole). Sync the existing audio capture stream so output is a single playable file. Expose `--video out.mp4` on each headless runner.
**Done when.** `cargo run -p emu198x-c64 --no-default-features -- --tape demo.tap --video out.mp4 --frames 1800` produces a playable file with synchronised audio.

### B.3 — MCP control surface for the four anchors

**Verify.** Search the workspace for any existing MCP scaffolding or JSON-over-stdio control surface. There may be groundwork in archives we haven't ported.
**Build.** Stable JSON-over-stdio protocol exposing per-family operations: `run_frames`, `inject_input`, `screenshot`, `snapshot`, `restore_snapshot`, `query_screen_text`, `query_boot_state`, `media_insert`, `media_eject`. Reuses the existing query surfaces — the work is the protocol shell. One binary per family (`emu198x-script-<family>` already exists; either extend with `--mcp` flag or add a sibling `emu198x-mcp-<family>`).
**Done when.** A documented MCP schema lives in `docs/mcp/`. Each anchor's MCP server passes a smoke conversation script.

### B.4 — Native verifier feature parity

**Verify.** Read each native shell's input/UI handling. Items to confirm: APU channel mute (NES, Game Boy have it; Spectrum, C64 likely do not). Save/load snapshot from the UI (currently CLI-only?). Reset, pause, frame step.
**Build.** Bring Spectrum and C64 up to NES/Game Boy on channel mutes (beeper/AY for Spectrum; SID voices for C64). Add load/save snapshot from UI to all four anchors (depends on A.1 for Amiga). Add a common pause/step shortcut to all four.
**Done when.** Each anchor's native verifier exposes channel mutes, save/load snapshot, pause, and frame-step through documented hotkeys.

### B-phase exit criteria

- CRT filter is uniformly available across the four anchors.
- Video + audio capture produces playable mp4 from headless runners.
- MCP servers exist for each anchor and pass smoke tests.
- Native verifiers have feature parity on the curriculum primitives.
- **Coverage:** workspace at 82% line / 70% branch; Cov-3 (68000 carve-out) and Cov-4 (C64 runtime) closed.

---

## Phase C — Launch-ready polish (3-4 weeks)

Order within this phase is flexible; each item is independent.

### C.1 — Unified `emu198x` launcher

**Verify.** No `emu198x` (no suffix) crate in the workspace today.
**Build.** Smallest viable shape: catalogue UI → spawn the per-family binary. Do not merge cores. Reuse the shared `wgpu` presenter for the catalogue if it makes sense; otherwise SDL/native menus per the [`native-ui-strategy.md`](../../knowledge/decisions/native-ui-strategy.md) decision.
**Done when.** `cargo run -p emu198x` opens a catalogue, picking a system launches the corresponding binary.

### C.2 — NES curriculum mapper expansion

**Verify.** Cross-reference current mapper coverage (NROM, MMC1, UxROM, CNROM, MMC3, MMC5, AxROM, BxROM, NINA-001, Camerica, VRC2a, Action 53, Sunsoft-4, Color Dreams) against the Code198x NES curriculum game list.
**Build.** Pick 2-4 mappers from the gap by *curriculum game value*, not chip prestige. Strong candidates if curriculum needs them: VRC6 (Akumajou Densetsu — expansion audio), VRC7 (Lagrange Point — FM expansion audio), MMC2 (Punch-Out!! — bank-switching trick), FME-7 (Gimmick! — expansion audio).
**Done when.** Each chosen mapper has chip-level tests, an iNES smoke test, and a real-ROM regression in the headless `emu198x-nes` runner.
**Caveat.** Expansion-audio mappers also touch the APU mixer; scope grows accordingly. Pick non-audio mappers first if time is tight.

### C.3 — Amiga seam 3 (chip-owned `read_register_word`)

**Verify.** `byte_merge_latch` in `machine-commodore-amiga-ocs/src/lib.rs`.
**Build.** Each chip with custom registers exposes `fn read_register_word(&self, offset: u16) -> Option<u16>` covering its register range. Machine asks the right chip when it needs the merge value.
**Done when.** `byte_merge_latch` is gone; ~30 lines net reduction; A.2 invariants stay green.
**Lowest priority.** Current code is correct; this is future-proofing.

### C-phase exit criteria

- Unified launcher works.
- NES mapper coverage matches curriculum needs.
- The architecture review's seam list is fully closed.
- **Coverage:** workspace at 85% line / 75% branch on the emulation-relevant subset; Cov-5 (Z80 snapshot format) closed; coverage hygiene tasks (Cov-H1–H3) shipped.

---

## Track: Coverage uplift (parallel to all phases)

`./scripts/coverage.sh` baseline as of 2026-04-28: **72.07% region, 72.27% line, 59.20% branch, 74.62% function** across the workspace. That isn't catastrophically low, but the branch number is the soft spot, and a per-crate breakdown reveals specific places where coverage is asking us a question we should answer rather than papering over.

This track runs in parallel with Phases A–C. Per [`docs/testing-policy.md`](../testing-policy.md), **tests are spec-driven, not coverage-driven**. Treat low coverage as a signal that points at one of three things:

1. **Dead or unwired code** — delete it, or wire it into the test it was written to support.
2. **Spec gaps** — paths the existing tests don't reach because the spec was never written down. Fill the spec, then test it.
3. **Legitimately untestable** — wgpu surface code, host I/O glue, native-window event loops. Carve these out of the workspace target with `--ignore-filename-regex` so the headline number tracks emulation-relevant code.

### Workspace targets

- **By end of Phase A:** 78% line, 65% branch (mostly from A.1 snapshot round-trips and A.2 boot invariants).
- **By end of Phase B:** 82% line, 70% branch (B.3 MCP exercises the runtime surfaces, B.4 exercises native verifier paths).
- **By end of Phase C / October:** 85% line, 75% branch on the emulation-relevant subset.
- **Anti-target:** never set a per-PR coverage gate. Coverage is a periodic audit, not a per-change metric. Spec-driven testing first.

### High-leverage investigations (do these in order)

These are sized for "investigate, decide, then act" — not "write 50 tests".

#### Cov-1 — `mos-6502/src/cycle.rs` (14.8% line, 774 uncovered lines)

**Smell.** Tom Harte hits 2.47M cases through `tick.rs`. `cycle.rs` co-exists at 14.8% covered. Either it is a parallel implementation that was never reached by the harness, or it is a dead artefact from the port.
**Action.** Read it. Decide: delete, or write the test that proves it equivalent to `tick.rs` and wire it into the suite.
**Why high-leverage.** ~770 uncovered lines in a CPU crate that's the foundation for C64, NES, and four Wave 2 systems. Either decision lifts the workspace number meaningfully and reduces cognitive load on the next reader.

#### Cov-2 — `mos-6502/src/tick.rs` (44.1% line, 34.0% branch, 501 uncovered lines)

**Smell.** Tom Harte is 100%. So why 44.1%? Likely: the 9 deliberately-stubbed unstable opcodes; reset/IRQ/NMI sequences (Tom Harte tests the opcodes, not the pin events); RDY-stall paths only exercised by VIC-II badlines; OAMDMA stall (NES-specific).
**Action.** Read the uncovered regions. Add directed unit tests for any *correctness-critical* paths Tom Harte misses (interrupt latency, RDY behaviour, reset vector). Mark genuinely-unreachable paths `unreachable!()` and document the unstable-opcode stubs.
**Why high-leverage.** Same blast radius as Cov-1; this is the live tick path used by every 6502 system.

#### Cov-3 — `motorola-68000` carve-out + decode coverage (decode.rs 46.6%, fpu.rs 62.4%, mmu.rs 73.8%, disasm.rs 38.9%)

**Smell.** Tom Harte is 100% on the 68000 opcode set. The 68020+ FPU/MMU code and the disassembler aren't reachable from A500 paths.
**Action.** Add `--ignore-filename-regex 'motorola-68000/src/(fpu|mmu|disasm)\.rs'` to `scripts/coverage.sh` *with a comment in the file pointing to this decision*. For `decode.rs` and `cpu.rs`, identify which opcodes the 68000 dispatcher includes for 010/020 variants and either carve them out or test the variant-detect short-circuit explicitly.
**Why high-leverage.** Largest absolute uncovered-line cluster in the workspace. Most of it is correct-by-not-being-reached. A careful exclusion list is more honest than a number.

#### Cov-4 — `runtime-commodore-c64/src/runtime.rs` (44.1% line, 1225 uncovered lines)

**Smell.** Single 2200-line file at 44% coverage. The Amiga's runtime is at 82.6%, NES at 76.1% — so the C64 is a clear outlier. Likely autoload helpers, snapshot save/load, host-side import paths (`.bas`, `.t64`, `.d64`), live-1541 attach paths.
**Action.** Read the file structure, identify the 4-6 logical regions that are uncovered, write directed integration tests for each over the headless runner. Use the existing test ROMs/media (`Bruce Lee`, `Aztec Challenge`, etc.) as fixtures.
**Why high-leverage.** Anchor family runtime; biggest single-file gap; will compound with B.3 (MCP) and B.4 (native verifier) which exercise the same surface.

#### Cov-5 — `format-sinclair-zx-spectrum-z80` (lib.rs 48.1% line, sna.rs 62.4%, branch 19.4%)

**Smell.** Z80 is the most common Spectrum snapshot format. Branch coverage of 19.4% says the compression and bank-handling edge cases are largely untested.
**Action.** Add round-trip tests across: v1 uncompressed, v1 compressed, v2 (128K), v3 (Pentagon/+3, multiple memory pages), edge cases (compression with `0xED` in data, bank order on +3). The fixture pool exists at `~/Projects/Emu198x-Unclean/Reference/sinclair/spectrum/`.
**Why high-leverage.** Low risk, high coverage payoff, exercises a format that anchors the Spectrum verification matrix.

### Coverage hygiene tasks

These are cheap and pay off across the whole track:

- **Cov-H1.** Add `--ignore-filename-regex` to `scripts/coverage.sh` for: native verifier `main.rs` files (window/event glue), `emu198x-native-video/src/lib.rs` (wgpu surface code that needs a GPU), `emu198x-shell/src/input.rs` (host gamepad/keyboard glue). Document each exclusion in a top-of-file comment.
- **Cov-H2.** Commit `target/llvm-cov/coverage-summary.txt` to the repo on each release-candidate cut and reference it from `knowledge/log.md`. Provides the "coverage moved from X to Y this cycle" trail.
- **Cov-H3.** Add a `cargo xtask coverage-diff` shape (or a simple shell script) that compares two `coverage-summary.json` files and prints per-crate deltas — so future investigations know which crates regressed without eyeballing 80 lines.

### What this track explicitly does *not* do

- Write tests purely to lift a percentage. If a path is uncovered and you can't articulate the spec, the path may be wrong.
- Treat 100% as a target. Honest exclusions are better than synthetic tests over wgpu code.
- Add coverage gates to PRs. Per the testing policy, "a low coverage percentage should trigger audit work, not status inflation"; gates create the wrong incentive (test-shaped code that doesn't prove anything).

## Risks and dependencies

- **A.1 → B.4.** Amiga snapshot UI in B.4 needs A.1's serde substrate.
- **A.2 → A.3, A.4.** Boot invariants must land before the seam refactors that depend on them.
- **B.1 may surface presenter gaps.** If the shared `wgpu` presenter doesn't actually have a CRT shader (only raw/lcd), B.1 grows from "audit + wire" to "implement the shader". Verification step is non-optional.
- **B.2 ffmpeg dependency.** A sidecar process is the lowest-risk choice but introduces a system dependency. Document this clearly; offer `--no-video` for headless contexts.
- **B.3 archive search.** MCP scaffolding may exist in `~/Projects/Emu198x-archive*` (per RULES.md item 25). Check before building.
- **Phase B and Phase A do not need to be strictly sequential.** A.0 (log catch-up) and A.1 (snapshots) gate B; A.3 and A.4 do not. If a contributor is blocked on Amiga work, B.1 and B.2 can run in parallel.

## Definition of plan-done

- All A-phase items shipped, documented, tested.
- All B-phase items shipped, documented, tested.
- C-phase items either shipped or explicitly deferred with a reason.
- README, `knowledge/index.md`, and the per-system overview pages match the workspace.
- Each anchor family has a green boot-invariant suite, snapshot round-trip, MCP server, and feature-parity native verifier.
- Workspace coverage sits at 85% line / 75% branch on the emulation-relevant subset, with all five Cov-N investigations either closed or documented as "expected exclusion".
- The October launch is unblocked on infrastructure; remaining work is curriculum content and per-system polish.

## Related

- [Coherent development plan (Apr 12)](2026-04-12-emulator-suite-coherent-development-plan.md) — the architectural framing this plan inherits
- [Product roadmap](../../knowledge/decisions/product-roadmap.md) — October scope and drift triggers
- [Amiga architecture review](../../knowledge/decisions/amiga-architecture-review.md) — Phase A.3, A.4, C.3 source
- [Native UI strategy](../../knowledge/decisions/native-ui-strategy.md) — informs C.1
- [Save state format](../../knowledge/decisions/save-state-format.md) — informs A.1
- [Testing policy](../testing-policy.md) — informs every "Done when" criterion
