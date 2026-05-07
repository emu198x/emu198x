# Spectrum SOLID Status

**As of 2026-05-06.** Live tracker, audited against the [October catalogue Spectrum SOLID criteria](../../decisions/october-catalogue.md#october-bar-definition). Update as criteria flip.

## Headline

| Score | Count | Criteria |
|---|---|---|
| DONE | 4 | Variants, CRT filter, code quality, regressions |
| PARTIAL | 5 | Catalogue, formats, native UI, save state, code coverage (unmeasured) |
| NOT STARTED | 2 | Single binary, MCP |

## Per-criterion

### 1. Catalogue — PARTIAL

9 of ~70 manifest entries authored. Schema and harness in `crates/emu198x-catalogue/`. Manifest at `manifest/spectrum.toml` (284 lines). Coverage so far: 48K (Manic Miner, Knight Lore, Jet Set Willy, Atic Atac, Skool Daze, Chuckie Egg, Saboteur), 128K (Chase H.Q.), +3 / DSK (Chase H.Q. +3). Missing entirely: 16K, +2, +2A, +2B. Harness at `src/lib.rs` runs boot waypoint, scripted-input state-change, and audio-window assertions — but does not yet drive save-state round-trips.

### 2. Variants in scope — DONE

All eight in-scope variants present in code, each in its own crate:
| Variant | Crate | Underlying type |
|---|---|---|
| 16K | `machine-sinclair-zx-spectrum-16k` | `SpectrumMachineCore<Spectrum16kMemory>` |
| 48K | `machine-sinclair-zx-spectrum-48k` | `SpectrumMachineCore<Spectrum48kMemory>` |
| Spectrum+ | `machine-sinclair-zx-spectrum-plus` | `SpectrumMachineCore<Spectrum48kMemory>` (same type as 48K; catalogue identity via `Model::SpectrumPlus`) |
| 128K | `machine-sinclair-zx-spectrum-128k` | `Spectrum128kClassCore<Sinclair128KMarker>` |
| Grey +2 | `machine-sinclair-zx-spectrum-plus2` | `Spectrum128kClassCore<AmstradPlus2Marker>` |
| +2A | `machine-sinclair-zx-spectrum-plus2a` | `SpectrumAmstradClassCore<Plus2AMarker>` |
| +2B | `machine-sinclair-zx-spectrum-plus2b` | `SpectrumAmstradClassCore<Plus2BMarker>` |
| +3 | `machine-sinclair-zx-spectrum-plus3` | `SpectrumAmstradClassCore<Plus3Marker>` |

Three layer crates host the shared compositions:
- `common-sinclair-zx-spectrum-48k-class` — Ferranti ULA + Z80 + 48K-or-16K memory, used by 16K / 48K / Spectrum+.
- `common-sinclair-zx-spectrum-128k-class` — Sinclair 7K010E ULA + Z80 + Memory128K + AY, used by 128K / grey +2.
- `common-sinclair-zx-spectrum-amstrad-class` — Amstrad 40077 + Z80 + MemoryPlus + AY + FDC, used by +2A / +2B / +3 (FDC enabled only on +3 via the marker's `HAS_FDC` const).

Phantom variant markers keep variants type-distinct where the hardware genuinely differs (snapshot type-binding, disk-slot dispatch). The 48K and Spectrum+ are the only pair that share a Rust type — they really are the same hardware, so catalogue identity comes from the `Model` enum alone.

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
- **D8. Extract `SpectrumMachineCore<M: MemoryBus>` to a new layer crate.** Done 2026-05-06. The 48K-class machines (16K, 48K, Spectrum+) are electrically identical except for memory size. The shared composition lives in a new layer crate `common-sinclair-zx-spectrum-48k-class` (depends on `common` + `ferranti-ula-6c001e` + `zilog-z80`) as `SpectrumMachineCore<M: MemoryBus>` with `FerrantiUla` baked in. **Why a layer crate, not `common-sinclair-zx-spectrum`:** putting the core in common would close a build cycle (common ← ferranti ← common) since FerrantiUla is needed in the composition. A second generic `<U: Ula>` would dodge the cycle but adds speculative complexity (every 48K-class variant uses Ferranti). The layer crate keeps `common-{family}` strictly trait/helper-only per `within-family-layering.md` and sets the pattern for future class crates (a `-128k-class` would slot in the same way if 128K and +2 prove worth sharing later). The 48K crate is now `pub type Spectrum48k = SpectrumMachineCore<Spectrum48kMemory>` plus an `ApplyInputEvent` extension trait that handles host-boundary `InputEvent` mapping (kept out of common because common is hardware-only). 16K and Spectrum+ crates are now unblocked as similar thin wrappers. **D8 does not cover +2 / +2A / +2B / +3** (different ULAs and memory paging).

The result is 8 machine crates, one per SOLID variant: 16K, 48K, Spectrum+ (`-plus`), 128K, +2, +2A, +2B, +3. Catalogue manifest grows to ~80 entries (10 per variant via overlap). Spectrum+'s entries duplicate 48K's title set against the `-plus` crate so any future drift between the two surfaces immediately.

## Phase 1 scope

Phase 1 splits into three tracks. Foundations and pipeline can run in parallel; native-menu work depends on foundations (variant scaffolding for the Machine menu; format work for the Open dialogs).

**Track 1A — Foundations (scaffolding, no behaviour change for existing variants):**

1. ~~Split SNA into its own crate.~~ **Done 2026-05-06.** `format-sinclair-zx-spectrum-sna` lives.
2. ~~Extract shared snapshot types into `format-sinclair-zx-spectrum-snapshot`.~~ **Done 2026-05-06** (D7).
3. ~~Add `Spectrum16kMemory` to `common-sinclair-zx-spectrum`.~~ **Done 2026-05-06.** 12 tests, full coverage of read/write/contention/ROM-loader paths.
4. ~~Extract `SpectrumMachineCore<M: MemoryBus>` (D8).~~ **Done 2026-05-06.** New layer crate `common-sinclair-zx-spectrum-48k-class` owns the shared 48K-class composition; the 48K crate is now `pub type Spectrum48k = SpectrumMachineCore<Spectrum48kMemory>` plus the `ApplyInputEvent` extension trait. 48K crate dropped from 976 to 280 lines (mostly tests). 16K and Spectrum+ wrappers unblocked.
5. ~~Scaffold `machine-sinclair-zx-spectrum-16k` as a thin wrapper over `SpectrumMachineCore<Spectrum16kMemory>` (post-D8). Boot tests + ROM-region behaviour tests.~~ **Done 2026-05-06.** New crate is ~120 lines (lib.rs + machine.rs + 9 tests). Wired into the runtime as `Spectrum16kRuntime` with `Model::Spectrum16KPal` profile entry, `SpectrumMachine` impl, frame-emit + snapshot round-trip integration tests.
6. ~~Scaffold `machine-sinclair-zx-spectrum-plus` for the actual Spectrum+ as a thin wrapper over `SpectrumMachineCore<Spectrum48kMemory>` (post-D8 and post-D6 +2A/+2B/+3 extraction).~~ **Done 2026-05-07.** Type alias is identical to `Spectrum48k`'s — same Rust type. `Model::SpectrumPlus` profile entry distinguishes catalogue identity. `SpectrumPlusRuntime` reintroduced atop the freed `-plus` crate name.
7. ~~Scaffold `machine-sinclair-zx-spectrum-plus2`. Mirrors 128K's shape — own crate, reuses `sinclair-ula-7k010e`, adds +2-specific cassette and paging quirks. **Not covered by D8** (128K-family does not share `SpectrumMachineCore`).~~ **Done 2026-05-06.** Lifted a `Spectrum128kClassCore<V: Class128kVariant>` into a new layer crate `common-sinclair-zx-spectrum-128k-class` (parallel to the 48K-class extraction). The 128K crate is now `pub type Spectrum128K = Spectrum128kClassCore<Sinclair128KMarker>`; the new `-plus2` crate is `pub type SpectrumPlus2 = Spectrum128kClassCore<AmstradPlus2Marker>`. Phantom variant marker keeps the two as distinct types so snapshots can't cross variants. The "+2-specific cassette and paging quirks" claim turned out to be inaccurate — the grey +2 is electrically identical to the 128K (different ROM bundle and copyright banner only); built-in joystick ports map to Sinclair Interface 2-style key emulation, not Kempston. Joystick handling still TBD — see follow-up below.
8. ~~D6 extraction: `-plus` (the existing crate handling +2A/+2B/+3 via Model enum) splits into `-plus2a`, `-plus2b`, `-plus3`.~~ **Done 2026-05-07 with the layer-crate pattern (not as originally documented).** New `common-sinclair-zx-spectrum-amstrad-class` layer crate hosts `SpectrumAmstradClassCore<V>` shared across all three variants; each wrapper crate is a one-line type alias plus tests. Mirrors D8 (48K-class) and the 2026-05-06 128K-class extraction.
9. Runtime aliases for new variants in `runtime-sinclair-zx-spectrum/src/variants.rs`.
10. Boot tests for every new variant.

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
| 2026-05-06 | **Track 1A — `Spectrum16kMemory` landed.** New type in `common-sinclair-zx-spectrum/src/memory.rs` with 12 tests covering read/write across the three address regions (ROM / RAM / disconnected $8000-$FFFF returning $FF), contention map, ROM loader, and accessors. 51 tests pass in common (was 39). Foundation for D8 + 16K wrapper. |
| 2026-05-06 | **D8 locked.** Extract `SpectrumMachineCore<M: MemoryBus>` from `machine-sinclair-zx-spectrum-48k` into `common-sinclair-zx-spectrum`. Next session work — 4–6 hour refactor. Unblocks 16K and Spectrum+ as thin wrappers; does not affect 128K family or +2-onward variants (they use different ULAs). |
| 2026-05-06 | **D8 landed (layer crate).** New crate `common-sinclair-zx-spectrum-48k-class` owns `SpectrumMachineCore<M: MemoryBus>` with `FerrantiUla` baked in. Routed via a layer crate (rather than common) to avoid the common ← ferranti dependency cycle. The 48K wrapper is now ~280 lines (was 976): a type alias plus the `ApplyInputEvent` extension trait. `TapeInput` moved to the layer crate; 48K crate re-exports it. All tests pass: 7 in layer, 13 in 48K wrapper, 18 in `runtime_48k` integration tests (the snapshot round-trip exercises the new extension trait). 16K and Spectrum+ wrappers unblocked. |
| 2026-05-06 | **16K wrapper landed.** New `machine-sinclair-zx-spectrum-16k` crate is `pub type Spectrum16K = SpectrumMachineCore<Spectrum16kMemory>` with 9 tests covering ROM-region reads, RAM read/write within the lower 16 KiB, electrically-disconnected upper 32 KiB ($FF reads, dropped writes), contention map matching 48K, and Issue 2/3 EAR feedback. Wired into runtime as `Spectrum16kRuntime` with the `Model::Spectrum16KPal` profile entry, full `SpectrumMachine` impl, frame-emit and snapshot round-trip integration tests. Runtime variant test count: 44 → 46. SOLID criterion 2 (variants in scope) advances: 5 → 6 of 8 variants in code (16K joins 48K, 128K, +2A/+2B/+3). |
| 2026-05-07 | **Kempston lifted to a Peripheral.** New `peripheral-kempston-joystick` crate implements the family's `Peripheral` trait with `KempstonJoystick { attached: bool, state: u8 }`. Migrated 7 machines: 48K-class (16K, 48K, Spectrum+ all gain a kempston field for the first time, since the pre-extraction 48K crate didn't have one), 128K-class, Pentagon, Scorpion, TC2048, TS2068. Removed kempston entirely from Amstrad-class (+2A/+2B/+3) — Amstrad broke the rear connector pinout in 1987 so classic Kempston interfaces don't physically fit. Default is `attached: false` everywhere, so unplugged interfaces correctly fall through to floating bus instead of returning zero. Updated the `Peripheral` trait docstring to remove the "Kempston isn't a peripheral" carveout (the original reasoning ignored optionality and wrong-machine modelling). Sinclair Interface 2 keyboard mapping for the built-in joystick ports on +2/+2A/+2B/+3 remains deferred — runtime-input-layer concern, no machine-side change required when it lands. Tests: peripheral crate 10/10, all 7 migrated variant crates green, runtime variants 51/51. |
| 2026-05-07 | **D6 + step 6 + step 7 landed in one wave.** New layer crate `common-sinclair-zx-spectrum-amstrad-class` hosts `SpectrumAmstradClassCore<V: AmstradVariant>` with phantom markers `Plus2AMarker`, `Plus2BMarker`, `Plus3Marker`. The Plus3 marker's `HAS_FDC=true` and `HAS_DISK_SLOT=true` consts gate the µPD765A and disk-slot dispatch at the type level. Old `machine-sinclair-zx-spectrum-plus` (which handled +2A/+2B/+3 via a runtime Model enum) deleted; three thin wrapper crates `-plus2a`/`-plus2b`/`-plus3` replace it. The freed `-plus` name now hosts the actual Spectrum+ wrapper (`pub type SpectrumPlus = SpectrumMachineCore<Spectrum48kMemory>`). Runtime updated: single blanket `impl<V: AmstradVariant> SpectrumMachine for SpectrumAmstradClassCore<V>` replaces the old `impl SpectrumMachine for SpectrumPlus` + `matches!(self.model, ...)` runtime branching. Three new runtime aliases (`SpectrumPlus2ARuntime`, `SpectrumPlus2BRuntime`, `SpectrumPlus3Runtime`) plus `SpectrumPlusRuntime` for the actual Spectrum+; old `SpectrumPlusRuntime` (which aliased the +2A/+2B/+3 enum machine) retired. `Model::SpectrumPlus` profile added (1984, 48K-equivalent). 30+ test sites in `tests/variants.rs` migrated; HasAy impl became a single blanket impl. SOLID criterion 2 (variants in scope) **flips to DONE: 8 of 8 variants in code.** Runtime variant test count: 49 → 51. SOLID headline: 4 done / 5 partial / 2 not started. |
| 2026-05-06 | **128K-class layer crate + +2 wrapper landed.** New `common-sinclair-zx-spectrum-128k-class` hosts `Spectrum128kClassCore<V: Class128kVariant>` with the Sinclair 7K010E ULA, AY-3-8912, Memory128K, and TIMING_128K baked in. Phantom variant marker (`Sinclair128KMarker`, `AmstradPlus2Marker`) gives distinct types per variant. The 128K crate dropped from 342+279 lines to ~50 (lib.rs alone, no memory.rs); the new `-plus2` crate is ~100 lines with 7 tests (model id, defaults, frame cadence, ROM loader, paging, contention, dimensions). Wired into runtime as `SpectrumPlus2Runtime` with full `SpectrumMachine` impl, dedicated `SPECTRUM_PLUS2_BANNERS` ("Amstrad Consumer Electronics plc"), and three integration tests including a snapshot type-bound test (a 128K snapshot can't restore into a +2 runtime). Runtime variant test count: 46 → 49. SOLID criterion 2 advances: 6 → 7 of 8 (only Spectrum+ left, blocked on D6). |
