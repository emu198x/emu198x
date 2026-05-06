# Spectrum SOLID Status

**As of 2026-05-06.** Live tracker, audited against the [October catalogue Spectrum SOLID criteria](../../decisions/october-catalogue.md#october-bar-definition). Update as criteria flip.

## Headline

| Score | Count | Criteria |
|---|---|---|
| DONE | 3 | CRT filter, code quality, regressions |
| PARTIAL | 6 | Catalogue, variants, formats, native UI, save state, code coverage (unmeasured) |
| NOT STARTED | 2 | Single binary, MCP |

## Per-criterion

### 1. Catalogue — PARTIAL

9 of ~70 manifest entries authored. Schema and harness in `crates/emu198x-catalogue/`. Manifest at `manifest/spectrum.toml` (284 lines). Coverage so far: 48K (Manic Miner, Knight Lore, Jet Set Willy, Atic Atac, Skool Daze, Chuckie Egg, Saboteur), 128K (Chase H.Q.), +3 / DSK (Chase H.Q. +3). Missing entirely: 16K, +2, +2A, +2B. Harness at `src/lib.rs` runs boot waypoint, scripted-input state-change, and audio-window assertions — but does not yet drive save-state round-trips.

### 2. Variants in scope — PARTIAL

Five of seven in-scope variants present in code: 48K (`machine-sinclair-zx-spectrum-48k`), 128K (`-128k`), and +2A / +2B / +3 (handled by `-plus` via `Model` enum at `src/lib.rs:46-58`). **16K and the original gray +2 are missing.** No `machine-sinclair-zx-spectrum-16k` crate; the 48K crate does not branch for 16K's reduced RAM. No `-plus2` handler; the original +2 has 128K's chipset but specific cassette and paging quirks distinct from +2A.

### 3. Formats — PARTIAL

TAP (`format-sinclair-zx-spectrum-tap`), TZX (`format-sinclair-zx-spectrum-tzx`), Z80 (`format-sinclair-zx-spectrum-z80`), and SNA (`format-sinclair-zx-spectrum-sna`) all implemented as separate crates. Shared `Snapshot` and `SnapshotModel` types live in `format-sinclair-zx-spectrum-snapshot`. DSK/EDSK shared with Amstrad CPC via `format-amstrad-dsk`. PARTIAL because the criterion requires every format working across all in-scope variants — the new variants (16K, +2, Spectrum+, +2A, +2B, +3 once extracted) need format support exercised against them, which can't happen until the variant crates exist.

### 4. Pipeline / single binary — NOT STARTED

Two separate binaries today: `emu198x-spectrum` (winit UI) and `emu198x-script-spectrum` (headless). The locked criterion requires one `emu198x-spectrum` binary with `--ui` (default), `--script`, `--mcp` modes. UI binary's CLI is `--rom`, `--tape`, `--play-tape`, `--autoload-tape`, `--scale`, `--video` — no mode flag.

### 5. MCP — NOT STARTED

No MCP crate, no MCP code anywhere in the workspace. `grep -l "mcp"` returns only build artifacts.

### 6. CRT filter — DONE

`emu198x-native-video` defines `VideoFilter::Crt` with a `crt()` preset constructor. Filter applies post-framebuffer-upload. UI exposes via `--video crt`. One acceptable preset, per the criterion.

### 7. Native UI — PARTIAL

winit-based UI exists with run/pause/reset, volume, window sizing, tape transport. Keyboard shortcuts: Esc (quit), F9/F10 (tape), F11 (turbo), F12 (reset), Numpad 1/2/0 (audio). **Missing for SOLID:** variant selection (no machine menu), runtime file picker, snapshot save/load buttons. Files load at startup only, via CLI flags.

### 8. Save state — PARTIAL

Postcard round-trip works per-variant. Tests at `runtime-sinclair-zx-spectrum/tests/variants.rs` cover Spectrum48k, 128k, Plus2A, Plus3, Pentagon, TC2048, TS2068 (9 functions). Encode/decode at `runtime-sinclair-zx-spectrum/src/snapshot.rs`. **Catalogue harness does not yet drive save-state assertions** — every-variant-every-title coverage needs the harness extended.

### 9. Code quality — DONE

No `.unwrap()` in Spectrum-side library code outside test blocks (audited across `zilog-z80`, `ferranti-ula-6c001e`, `sinclair-ula-7k010e`, `amstrad-ula-40077`, `gi-ay-3-8912`, `nec-upd765a`, `format-sinclair-zx-spectrum-*`, `machine-sinclair-zx-spectrum-*`, `runtime-sinclair-zx-spectrum`, `common-sinclair-zx-spectrum`). No `todo!`/`unimplemented!`/STUB markers found.

### 10. Regressions — DONE (with caveat)

ZEXDOC and ZEXALL pass via `zilog-z80/tests/zex_tests.rs` (67 checkpoints, Frank Cringle's exerciser). Tom Harte exists for 68000 only — for the Z80, ZEX is the equivalent regression suite. **Caveat:** no CI gating yet. Tests pass when run; "stay green" needs a CI lock to be enforceable rather than aspirational.

### 11. Code coverage — PARTIAL (unmeasured at scope; two crates measured)

Existing test surface produces decent coverage on the CPU (Tom Harte / ZEXDOC / ZEXALL on `zilog-z80`), the ULA crates (FUSE-style timing tests, screen rendering tests), the format crates (unit tests per the audit), and the runtime (9 snapshot round-trip tests). Likely 70-85% across most existing crates without measuring; the new and just-extracted machine crates (16K, +2, Spectrum+, +2A, +2B, +3) start at 0%.

**Measured 2026-05-06 (post-SNA-split, post-snapshot-extraction):**

- `format-sinclair-zx-spectrum-sna`: 100% line, 100% region, 100% function (5 tests).
- `format-sinclair-zx-spectrum-snapshot`: data-only crate (struct + enum, no executable logic) — exempt from line-coverage gating by definition. Postcard round-trip exercises the type construction via `runtime-sinclair-zx-spectrum/tests/variants.rs`.

**Pending:** baseline measurement across the full Spectrum-specific crate set, CI gate at ≥90% line coverage, TDD discipline for new variant scaffolding.

**Data-only exemption** (locked 2026-05-06): a Spectrum-specific crate that contains only type definitions (no functions, no logic, no executable code) is exempt from the 90% gate. The exemption is by construction — `cargo-llvm-cov` reports 0 functions / 0 lines for such crates and a coverage percentage is undefined. Currently exempt: `format-sinclair-zx-spectrum-snapshot`. Future data-only crates qualify on the same basis. Round-trip tests against postcard serialisation in consumer crates (e.g. `runtime-sinclair-zx-spectrum/tests/variants.rs`) provide the regression coverage that line counting can't.

## Critical observations

1. **16K variant is in the locked criteria but absent from the codebase.** Three resolutions on the table — fold under 48K with a RAM-size flag, scaffold a dedicated crate, or amend SOLID to drop 16K. Real call point.
2. **Original gray +2 is also missing.** It's not just +2A/+2B/+3 — the original +2 has 128K's chipset but specific cassette/paging quirks. Either fold into the 128K crate or drop from SOLID.
3. **SNA discoverability — RESOLVED 2026-05-06.** SNA split into `format-sinclair-zx-spectrum-sna`; shared snapshot types extracted into `format-sinclair-zx-spectrum-snapshot` (D7).
4. **Catalogue is closer than it looked.** 9 entries already authored. The work to reach ~70 is meaningful but it's manifest authoring + hash capture, not new emulator work — the engine that runs them is operational.
5. **CI gating for ZEX is a small but real gap.** Tests exist, pass, and aren't gated. Closing this is a 30-minute task and turns criterion 10 from "passes when checked" to "stays green by construction."

## Decisions resolved 2026-05-06

- **D1. 16K variant — ADD as separate crate.** Scaffold `machine-sinclair-zx-spectrum-16k` (RAM-limited 48K variant, reuses `ferranti-ula-6c001e`).
- **D2. Original +2 — ADD as separate crate.** Scaffold `machine-sinclair-zx-spectrum-plus2` (Sinclair-era, reuses `sinclair-ula-7k010e` from 128K, +2-specific cassette and paging quirks).
- **D3. SNA — SPLIT OUT.** Move from `format-sinclair-zx-spectrum-z80/src/sna.rs` to a new `format-sinclair-zx-spectrum-sna` crate. Matches per-format crate convention.
- **D4. UI framework — winit + `muda` + `rfd`.** Per the [native-ui-strategy decision](../../decisions/native-ui-strategy.md) amended 2026-05-06. winit stays as the windowing layer; `muda` adds native menus (NSMenu / GTK4 menu / Win32 menu); `rfd` adds native file dialogs. Per-platform SwiftUI / GTK4 / WinUI frontends remain post-October.
- **D5. Spectrum+ — separate crate using the freed `-plus` name (post-extraction), included in SOLID.** Once D6's extraction is complete and the existing `-plus` crate is retired, scaffold a new `machine-sinclair-zx-spectrum-plus` for the actual Spectrum+. Electrically identical to the 48K (same ROM, same RAM, same `ferranti-ula-6c001e`, same keyboard matrix); the crate is a thin home for Spectrum+ identity rather than emulation differences. **Promoted into the October SOLID variant list 2026-05-06** — even though no chip-level difference exists, treating Spectrum+ as a first-class variant in the catalogue gives regression coverage for any future drift between `-plus` and `-48k` and avoids the inconsistency of one variant being treated differently in the selector. Sequencing: D6 extraction must complete first so the `-plus` name is free.
- **D6. +2A / +2B / +3 — EXTRACT to separate crates.** Retire the existing `machine-sinclair-zx-spectrum-plus` crate (which currently handles all three via a Model enum). Extract into `machine-sinclair-zx-spectrum-plus2a`, `-plus2b`, `-plus3`. All three reuse `amstrad-ula-40077`; +3 additionally uses `nec-upd765a` for disk. Rationale: variant-specific quirks belong in the variant's crate, not in a shared Model enum match. Symmetry with D1/D2 — every SOLID variant gets its own crate. After extraction, the `-plus` name frees up for D5.
- **D7. Snapshot types — EXTRACT to a shared crate, rename `Z80Snapshot` → `Snapshot`.** Done 2026-05-06. New crate `format-sinclair-zx-spectrum-snapshot` owns the `Snapshot` struct and `SnapshotModel` enum. Both `format-sinclair-zx-spectrum-z80` and `format-sinclair-zx-spectrum-sna` depend on it. `common-sinclair-zx-spectrum`, all four common-importing machine crates (`-128k`, `-plus`, `-pentagon-128`, `-scorpion-zs256`), and both timex machine crates (`-tc2048`, `-ts2068`) updated. Rationale: the previous `Z80Snapshot` name was misleading (it represented Spectrum machine state, not Z80-file-format state) and the SNA crate had a weird dependency on the Z80 crate purely to import the type. The extraction also gives upcoming variant scaffolding a clean target.

The result is 8 machine crates, one per SOLID variant: 16K, 48K, Spectrum+ (`-plus`), 128K, +2, +2A, +2B, +3. Catalogue manifest grows to ~80 entries (10 per variant via overlap). Spectrum+'s entries duplicate 48K's title set against the `-plus` crate so any future drift between the two surfaces immediately.

## Phase 1 scope

Phase 1 splits into three tracks. Foundations and pipeline can run in parallel; native-menu work depends on foundations (variant scaffolding for the Machine menu; format work for the Open dialogs).

**Track 1A — Foundations (scaffolding, no behaviour change for existing variants):**

1. Scaffold `machine-sinclair-zx-spectrum-16k`. Memory map differs only in available RAM; reuse 48K's ULA, contention model, and ROM slot.
2. Scaffold `machine-sinclair-zx-spectrum-plus2`. Mirrors the 16K shape — separate crate, reuses 128K's ULA and chipset, adds +2-specific cassette and paging quirks.
3. Split SNA: new crate `format-sinclair-zx-spectrum-sna`, move existing module out of the Z80 crate, update workspace members.
4. Runtime aliases for 16K and +2 in `runtime-sinclair-zx-spectrum/src/variants.rs`.
5. Boot tests for 16K and +2 (each variant boots to its menu).

**Track 1B — Single-binary consolidation:**

6. Merge `emu198x-script-spectrum` into `emu198x-spectrum` with `--ui` (default), `--script <input>`, `--mcp` mode flags. Cargo features gate UI-only deps for headless deployments.
7. Update Code198x screenshot/video skills to call `emu198x-spectrum --script` (cross-project change in the Code198x repos).
8. Catalogue harness re-verified on Manic Miner through the consolidated binary.

**Track 1C — Native menu shell:**

9. Add `muda` and `rfd` dependencies.
10. Native menu bar with: **File** (Open Snapshot / Tape / Disk via `rfd`), **Machine** (variant selector across all 7), **State** (Save / Load), **View** (window sizing options).
11. Wire menu actions to existing keyboard-shortcut equivalents (avoid duplicate logic paths).

**Track 1D — Quality lock-ins:**

12. CI gate for ZEXDOC, ZEXALL, save-state round-trip tests. Turns criterion 10 from "passes when run" into "stays green by construction."
13. Install `cargo-llvm-cov`. Capture baseline line coverage per Spectrum-specific crate. Identify gaps.
14. CI gate at ≥90% line coverage on Spectrum-specific crates (per the criterion 11 scope list). Coverage report published as a CI artefact for visibility on each PR.
15. As new variant crates are scaffolded in Track 1A (16K, +2, +2A, +2B, +3, Spectrum+ via `-plus`), tests are authored alongside the crate so it lands at ≥90% from the start, not as a follow-up. TDD discipline rather than coverage-chasing later.

**Output:** all 8 in-scope variants compile and boot; SNA discoverable; single binary with three modes; native menus and file dialogs working; ZEX/save-state regressions and 90% line coverage gated in CI. Phase 2 (catalogue authoring) becomes unblocked.

## Log

| Date | Event |
|---|---|
| 2026-05-06 | Tracker created. Audit of current state vs. ten SOLID criteria: 3 done, 5 partial, 2 not started. |
| 2026-05-06 | D1–D4 resolved. 16K and +2 added; SNA split; winit + muda + rfd locked for UI. Phase 1 scoped into four tracks (1A foundations, 1B single binary, 1C menus, 1D CI lock). |
| 2026-05-06 | D5–D6 resolved. +2A/+2B/+3 extracted into separate crates (retiring the old `-plus`); the freed `-plus` name then used for the actual Spectrum+. Spectrum+ promoted into the SOLID variant list (7 → 8 variants). |
| 2026-05-06 | Criterion 11 added: ≥90% line coverage on Spectrum-specific crates, gated in CI. Branch coverage measured but not gated. Track 1D expanded with coverage measurement, gate, and TDD discipline for new variant crates. |
| 2026-05-06 | **Track 1A — SNA crate split landed.** New crate `format-sinclair-zx-spectrum-sna` (was `format-sinclair-zx-spectrum-z80/src/sna.rs`). 5 tests pass, 100% line coverage. Workspace builds clean. Z80 crate description updated to drop `.sna` claim. |
| 2026-05-06 | **Track 1A — Snapshot type extraction landed (D7).** New crate `format-sinclair-zx-spectrum-snapshot` owns `Snapshot` (renamed from `Z80Snapshot`) and `SnapshotModel`. Both format-z80 and format-sna now depend on it; common-sinclair-zx-spectrum re-exports from it; six machine crates updated. Workspace builds clean; 174+ tests pass across the affected set. Data-only exemption locked into criterion 11. |
