# Testing Policy

This file defines the verification standard for emulator components in
Emu198x. It is the source of truth for how new crates should be tested and how
existing crates should be audited.

For current support state, see [status.md](status.md). For active work, see
[roadmap.md](roadmap.md). For the current crate inventory, see
[inventory.md](inventory.md).

---

## Purpose

The project should not rely on boot tests alone to establish correctness.
Every reusable component should have isolated, repeatable, source-backed tests
for its own observable behavior, with machine-level tests confirming that the
wiring matches the component contracts.

The practical goals are:

- make component behavior explicit
- catch regressions before they become machine-level failures
- keep timing-sensitive behavior testable in isolation
- tie emulator behavior back to primary hardware references where possible
- make `Complete` and similar status labels defensible

## Core Principles

- Tests are spec-driven, not coverage-driven.
- Every behavior under test should have a clear source, observable outcome, or
  both.
- Component tests come before machine tests when behavior can be isolated.
- Fast deterministic tests should form the default development loop.
- Slow ROM suites and differential checks are confirmation layers, not the
  first line of defense.
- Bug fixes should add or strengthen a regression test in the same change.
- A crate should not be marked `Complete` unless it meets the expectations in
  this policy or documents the remaining gap explicitly.

## Evidence Hierarchy

Use reference material in this order:

1. vendor manuals, hardware reference manuals, and original technical
   documentation
2. measured behavior, logic-analyzer captures, and authoritative test
   programs or ROM suites
3. project-local fixtures, engineering notes, and generated reference data
4. mature external emulators as secondary oracles when the primary sources are
   incomplete or ambiguous

Notes:

- `docs/platforms/` and `docs/Reference/` should be the first places checked
  for in-repo reference material.
- If two sources disagree, record the chosen interpretation in the test or in
  a short linked engineering note.
- External emulator behavior should not be treated as primary truth unless the
  original hardware behavior is otherwise undocumented.

## Verification Ladder

Each reusable component should move through the same test ladder.

### 1. Contract Tests

These prove the basic external contract:

- reset state
- register map
- read and write masks
- address decode windows
- unmapped behavior
- documented side effects

### 2. Functional Tests

These prove the component's main behavior in isolation:

- instruction semantics for CPUs
- pixel generation for video chips
- waveform or mixer behavior for audio chips
- DMA and arbitration decisions
- parser and serializer behavior for format crates

### 3. Timing Tests

These prove cycle, tick, or phase-accurate behavior where timing matters:

- interrupt timing
- DMA slot timing
- bus contention
- raster or beam counters
- audio period behavior
- peripheral handshakes

### 4. Integration Confirmation

These prove that the machine wiring matches the component contract:

- machine-level register visibility
- cross-chip interactions
- boot-path checks for the relevant subsystem
- one or two representative end-to-end scenarios per major feature

### 5. Reference Programs And Differential Checks

These are slower confirmation layers:

- diagnostic ROMs
- conformance programs
- cross-checks against measured fixtures
- differential traces against trusted implementations

They should support the component tests, not replace them.

## Minimum Expectations By Crate Type

- `CPU`: instruction semantics, flags, exceptions, interrupts, and timing
  covered by single-step or equivalent authoritative suites
- `Chip`: reset state, register behavior, masks, side effects, interrupts,
  DMA or arbitration, timing invariants, and output behavior tested in
  isolation
- `Peripheral`: protocol handling, register or port behavior, error paths, and
  handshake timing tested in isolation
- `Format`: parse success cases, round-trip where applicable, corrupt and
  truncated input rejection, and checksum or structural validation where the
  format defines it
- `Machine`: wiring tests, representative boot or smoke tests, cross-component
  timing checks, and per-model configuration checks
- `Runnable emu-*`: CLI or host API behavior, media loading, scripting or MCP
  entry points, and headed versus headless parity where both modes exist
- `Transitional stubs`: narrowly scoped contract tests proving the exact
  behavior relied on today, with missing behavior documented explicitly

## Per-Crate Verification Matrix

Each crate should have an auditable verification matrix. It does not need a
special format yet, but it should capture at least:

- the observable behavior being claimed
- the reference source or fixture backing that claim
- the test file covering it
- whether the coverage is contract, functional, timing, or integration
- any known gaps or intentionally stubbed behavior

This can live in a future audit document, crate-local notes, or issue tracker
work, but the information needs to exist in a form that can be reviewed.

## Status Gates

These labels should mean the following when applied to component crates:

- `Not started`: no meaningful isolated behavior is implemented or verified
- `Stub`: only the minimum behavior needed to unblock another path is
  implemented; tests cover that narrow contract
- `In progress`: major behavior exists, but one or more required test
  categories are still missing
- `Complete`: the crate meets the relevant expectations in this policy and any
  remaining limitations are documented

`Complete` should not mean "boots one thing" or "seems to work in the machine."

## Fixture And Reference Data Rules

- Keep fast tests self-contained where possible.
- Put reusable binary fixtures under crate-local `tests/data/` directories.
- If a fixture is generated or transformed, keep the generation steps or
  provenance documented next to it.
- If redistribution is restricted, keep a checksum, acquisition note, and any
  local conversion script rather than committing unclear blobs.
- When a test depends on a known external program or ROM suite, state that
  dependency in the test module or supporting documentation.

## Execution Tiers

Use at least three practical tiers:

- `Fast`: default local development loop with unit tests, contract tests,
  small timing tests, and parser tests
- `Slow`: pre-merge or scheduled validation with reference ROMs, differential
  traces, and larger fixture sweeps
- `Heavy`: targeted milestone confirmation with full boot matrices, larger
  media suites, and long-running soak tests

Fast tests should be cheap enough to run routinely while changing the crate
they cover.

## Change Policy

- New reusable crates should land with contract tests at minimum.
- New externally visible behavior should land with isolated tests for the new
  behavior.
- A regression fix should include a regression test or a documented reason why
  one is not practical.
- If a test must be ignored because it depends on unavailable ROMs or local
  assets, the reason should be obvious from the test or helper comments.

## Current State

The repository is not yet fully aligned with this policy.

Current strengths:

- CPU verification is already strong.
- Several mature machine and chip crates already have meaningful isolated and
  integration coverage.
- The repository already contains substantial in-tree reference material for
  major systems, especially Amiga.

Current gaps:

- some wrapper-style crates have little or no direct isolated coverage
- some Amiga support-chip behavior is still stubbed or inline in machine code
- some inventory status labels are ahead of the documented verification
  standard

The next step after adopting this policy is a crate-by-crate audit against the
matrix above. The current first-pass audit lives in
[testing-audit.md](testing-audit.md).
