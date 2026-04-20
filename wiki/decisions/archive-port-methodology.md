# Archive-port methodology — bringing the -archive crates into the rewrite

**Date:** 2026-04-20
**Status:** Approved; applies to every `-archive` crate in `crates/`.

## Context

The 2026-04-19 restart kept the original Amiga implementation only as
`-archive` crates alongside a ground-up rewrite. Today's session
(chip-only KS 1.3 boot) surfaced three hardware-accuracy bugs that the
archive may not have had: the 8520 one-shot `TxHI` auto-start, the
CIA-A `/DSKCHANGE` default, and the copper CDANG halt. The user's
assessment is now:

> The archived code is probably not faulty. We should port it back —
> but carefully, with comprehensive tests.

This document codifies how every port works. **No `-archive` code
enters the live build without passing through all three phases
described here.**

## Drift triggers

If you find yourself:

- copying archive code wholesale and "just adding a test after",
- skipping characterization because "the archive is obviously right",
- porting a sub-feature without its dedicated tests,
- integrating a ported module without running the existing boot suite,

**stop and re-read this doc.** The whole reason we have this
methodology is that the rewrite introduced subtle bugs that took
days of bisection to find. We will not trade one class of bugs for
another by cut-pasting.

## The three phases

### Phase 1 — read-and-characterize

**Goal:** understand the hardware behaviour the archive encodes, and
write it down as tests, before touching any code.

**Inputs:**

- The archive source: `crates/<name>-archive/src/`
- The Amiga Hardware Reference Manual text at
  `~/Projects/Emu198x-Reference/_organised/by-system/commodore-amiga/amiga-hardware-reference-manual-3rd-edition.txt`
- WinUAE: `~/Projects/Emu198x-Unclean/WinUAE/*.cpp`
- vAmiga: `~/Projects/Emu198x-Unclean/vAmiga/Core/**/*.cpp`
- AROS if relevant: `~/Projects/Emu198x-Unclean/aros-timer/` (more to add)

**Deliverables:**

1. **Gap-list document** at `wiki/amiga/<crate>-porting-gap-list.md`.
   Tabulates every register, every timer mode, every state machine
   transition the archive implements. For each entry, mark:
   - `covered-in-current-impl` — the rewrite already has this
   - `missing-from-current-impl` — we lack it entirely
   - `behaviour-matches-HRM` — archive matches the manual
   - `deviates-from-HRM` — archive does something the manual doesn't
     describe (often a real hardware quirk the manual omits; note
     which other emulator confirms it)
2. **Characterization tests** written against the ARCHIVE crate.
   These tests assert hardware behaviour as known from HRM + UAE +
   vAmiga — they are the ground truth the port must reproduce. If a
   characterization test FAILS on the archive, that's a bug we found
   in the archive; investigate before porting.

**What characterization tests must cover:**

- Every public register: a test that writes a value, then either
  reads it back (for readable registers) or observes the effect
  through state (for write-only registers).
- Every timer / state machine mode: boundary transitions (0-count,
  max-count, mode-switches).
- Every interrupt source: bit latches on the right condition; read
  clears (where applicable); `/IRQ` output level is correct.
- Every DMA participant: its slot is claimed / yielded correctly per
  the HRM slot table.
- Every cross-chip signal: CIA /IRQ into Paula, beam counter into
  Agnus VBL, etc.
- Reset state: every field starts at the documented reset value.
- Known 8520-/OCS-specific quirks that the HRM doesn't spell out —
  the CDANG halt, the `TxHI` auto-start, the DSKLEN double-write
  disarm-then-arm. Cross-reference against UAE / vAmiga for these.

**No code changes in Phase 1 beyond adding tests to the archive.**

### Phase 2 — port with tests

**Goal:** move a single concern from the archive into the live tree,
with per-concern tests.

**Rules:**

1. Port concerns in roughly the order of the gap list's risk — small
   self-contained ones first, complex cross-cutting ones last.
2. One concern = one commit. A "concern" is defined by the task list
   (e.g. "Timer A/B all modes", "DMACON + slot arbitration",
   "blitter minterm evaluator"). Large concerns split into sub-tasks.
3. Each ported concern ships with:
   - **Unit tests at register level** — every public register the
     concern touches has its Phase 1 tests running now against the
     live module.
   - **Timing tests** — cycle / DMA-slot accuracy verified.
   - **Edge-case tests** — fencepost values, reset state, mode
     transitions, overflow.
   - **Integration hook** — an import/wire-up point in
     `machine-commodore-amiga-ocs` so the next concern can build on
     it. Doesn't need to be used yet; just typecheck clean.
4. If the ported code differs from the archive (e.g. because we found
   an archive bug during characterization), document the difference
   inline in code + in the commit message.
5. Every commit leaves the full workspace test suite green. Phase 2
   is incremental by design; half-finished states must compile and
   pass existing tests.

### Phase 3 — integrate

**Goal:** make the ported module the primary implementation, retire
the in-tree stub / archive crate.

**Steps:**

1. Replace the in-tree stub (e.g. `src/cia.rs`) with the ported
   module. Keep the old file for one commit so the diff is small and
   reviewable.
2. Run:
   - Every `cargo test --workspace` test
   - Every Kickstart boot test (both `Amiga::new()` and
     `Amiga::with_slow_ram`)
   - Every golden-frame test
3. Add at least one **new** integration test that exercises a
   feature the ported module unlocks but the stub couldn't — e.g.
   "audio DMA produces non-zero sample output during boot",
   "blitter-based clear visible in framebuffer at frame N". This
   proves we got new capability, not just a re-arrangement.
4. Remove the `-archive` crate from the workspace `Cargo.toml` and
   delete its directory. Do this in a SEPARATE commit so it's easy
   to revert if something surfaces later.

## Test coverage targets (the bar)

Before any ported module is called "done":

- **Every public register** has at least one unit test (write
  updates state; read returns expected).
- **Every DMA-slot-participating path** has a cycle-accurate test
  that exercises contention with a higher-priority claim.
- **Every timing-sensitive path** has a test with known inputs →
  known outputs cross-checked against WinUAE or vAmiga trace output.
- **Every module** has at least one integration test that runs actual
  Kickstart ROM code touching it. (Cross-module-integration tests
  live in `machine-commodore-amiga-ocs/tests/` and are named
  `integration_*.rs`.)

"At least one" is the floor, not the target. Prefer one test per
distinct behaviour.

## Per-crate porting order (from the task list)

1. `mos-cia-8520-archive` (773 lines) — shakedown run for this
   methodology; lowest blast radius; we already have a known-good
   in-tree CIA to cross-check against.
2. `commodore-paula-8364-archive` (1431 lines) — interrupts, audio,
   serial, disk registers.
3. `commodore-agnus-ocs-archive` (1727 lines) — beam, DMA
   arbitration, copper, blitter. Biggest module by conceptual
   complexity.
4. `commodore-denise-ocs-archive` (2340 lines) — bitplane pipeline,
   sprites, HAM/EHB/DPF, collisions.
5. `peripheral-commodore-amiga-floppy-archive` (1051 lines) +
   `format-commodore-amiga-adf-archive` — MFM encode, drive state,
   ADF loader.
6. `peripheral-commodore-amiga-keyboard-archive` (357 lines) —
   scancode stream + SP handshake.
7. `commodore-gary-archive` (687 lines) — address decode, autoconfig.
8. `runtime-commodore-amiga-archive` (1162 lines) — boot loop,
   frame pacing, save states.
9. `emu198x-script-amiga-archive` — scripting.

## After all ports complete

- Remove `Amiga::new_with_slow_ram(_, 512*1024)` workaround (chip-only
  now boots correctly via CDANG — see `amiga-chip-only-boot-failure.md`).
- Write an ADR closing the porting phase — signpost the completed
  rebuild for future readers.

## Related documents

- `amiga-restart-plan.md` — the overall rebuild-from-scratch plan
- `amiga-chip-only-boot-failure.md` — the session that motivated
  this methodology
- `amiga-architecture-review.md` — whole-system architecture
