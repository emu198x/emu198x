# Running the NES blargg reference survey

This process answers how the wired NES test ROM cases are measured at one
identifiable Emu198x revision.

It is a diagnostic measurement, not a general NES pass rate and not a
physical-hardware-conformance claim. It says what the cases *currently wired
into the test suite* report — nothing about the ~200 staged ROMs that no test
references yet.

## Inputs

The harness resolves the ROM root in this order:

1. `NES_BLARGG_ROOT`, a directory containing the blargg suite subdirectories;
2. `assets/test-suites/nes-test-roms/`.

If neither resolves, each per-ROM test becomes a no-op rather than a failure.

The tracked
[`assets-v1.json`](../../test-data/nintendo/nes/blargg-survey/assets-v1.json)
manifest identifies **57 ROMs** by logical path, byte count and SHA-256, across
the suite families the tests reference. Unlike the C64 survey it does not gate
the run — it records which bytes produced a given result, so a later
disagreement can be attributed to the emulator rather than to a changed corpus.

## Running it

    cargo test -p machine-nintendo-nes --no-fail-fast -- --ignored

⚠⚠ **`--no-fail-fast` is required, not a convenience.** Cargo runs the lib
target before the integration targets, so any lib-test failure stops the run and
the blargg cases never execute. Without it a single unrelated lib failure
reports as `0 passed; 1 failed; 19 filtered out`, which reads like a broken
suite and is actually a suite that never ran.

⚠ **A test that needs an absent input must SKIP, never panic.** `diagnostic_nes_suite`
panicked on a missing `EMU198X_NES_SUITE` and, being in the lib target, took the
whole package down with it — so the NES ignored suite was unrunnable on any
machine that had not set that variable. Fixed 2026-08-09. Any new ROM-dependent
test follows `blargg_root()`: return `Option`, no-op when absent.

## Baseline — 2026-08-09

At `9db24f8d`, on the corpus pinned in `assets-v1.json`:

| | |
|---|---|
| cases run | **51** |
| passed | **50** |
| failed | 1 — `diagnostic_nes_suite`, the entry-point defect above, fixed in the same commit |

Passing families: `sprite_hit` 01–11, `ppu_vbl_nmi` 01–10, `sprite_overflow`
1–5, `mmc3_test` 1–5, `blargg_ppu_tests_2005.09.15b` (palette RAM, power-up
palette, sprite RAM, vbl clear time, VRAM access), `oam_read`, `oam_stress`,
`cpu_dummy_reads`, `ppu_read_buffer`, blargg `cpu_test5`, plus behavioural
probes (`vblank_flag_during_stall`, `what_the_cpu_actually_reads_from_2002`,
`long_run_pc_distribution`).

`sprite_hit` and `sprite_overflow` passing in full is the load-bearing result:
those are the classic discriminators emulators habitually fail.

## ⚠ What this baseline does NOT say

**Coverage is the gap, not correctness.** 263 ROMs are staged; **57 are
referenced** by tests. Everything wired passes, so the finding is that the core
is under-*measured*, not that it is inaccurate — the opposite of the assumption
that started this work.

Unreferenced families cluster in mapper and IRQ behaviour, which is where NES
emulators typically diverge and where a campaign should therefore look first:

    mmc3_irq_tests  mmc3_test_2  mmc5test  mmc5test_v2  MMC1_A12
    exram  nrom368  m22chrbankingtest  blargg_litewall  240pee  nes15-1.0.0

## Related

- [`c64-vicii-vice-survey.md`](c64-vicii-vice-survey.md) — the survey this
  follows, and the stricter manifest model (that one refuses a changed corpus).
- [`golden-image-capture.md`](golden-image-capture.md) — frame-level comparison,
  the natural follow-on if NES units gain executable curriculum proofs.
