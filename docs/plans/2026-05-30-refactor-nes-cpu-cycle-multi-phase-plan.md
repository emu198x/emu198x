---
title: NES CPU cycle multi-phase refactor
type: refactor
date: 2026-05-30
---

# NES CPU cycle multi-phase refactor

Implementation plan for [knowledge/decisions/nes-cpu-cycle-multi-phase.md](../../knowledge/decisions/nes-cpu-cycle-multi-phase.md). Target outcome: blargg `ppu_vbl_nmi/05-nmi_timing` + `/06-suppression` + `/07-nmi_on_timing` + `/08-nmi_off_timing` and `cpu_interrupts_v2/2-nmi_and_brk` + `/3-nmi_and_irq` all flip to passing. All other suites stay green.

## Why this is needed

These six failing tests increment their probe by **1 PPU clock per iteration**, but the boundary between "$2002 read suppresses NMI" and "$2002 read is too late" sits at sub-PPU-dot granularity within one CPU cycle. Our current model is locked to 1 master tick = 1 PPU dot resolution; three test iterations that need to land in three different PPU phases within one CPU cycle all collapse into the same master tick. Mesen models this with master clock at 4× PPU dot resolution and `_ppuOffset = 1`. We need the same.

Every coarser-resolution shortcut (move PPU NMI from dot 3 to dot 1, immediate `$2000` writes, sample `cpu.nmi` after bus op, 1-master-tick PPU snapshot lag) has been tried. Each fixes some tests and breaks others, in the self-cancelling pattern that confirms the resolution mismatch.

## Goals

- Phase split per CPU cycle: PPU advances at `master_clock - 1` at both StartCpuCycle and EndCpuCycle, with CPU bus op between.
- Internal master clock resolution at 4× PPU dot (12 per NTSC CPU cycle).
- Six target tests flip to passing.
- Tom Harte single-step ~2.56M, Klaus Dormann functional, nestest 8991/8991, ricoh-ppu-2c02 105/105 unit, ppu_onscreen 22/22, NES sweep ≥60/110 — all preserved.

## Non-goals

- Change to C64 machine, Amiga machine, Spectrum machine — out of scope. C64 already does single-phase per CPU cycle correctly for its shape.
- Shared `Tickable` abstraction across machines — premature per the decision record. Revisit at N≥4.
- PAL NES — NTSC is the focus; PAL ratio (1:3.2) is a follow-up.

## Phase structure

Five phases, each landing in its own commit, each leaving the tree green on the verification gate before moving on.

### Phase 1 — Add `Ppu::run(target)` as additive entry point

**Files:** `crates/ricoh-ppu-2c02/src/lib.rs`

**Changes:**
- Add `ppu_clock: u64` field to `Ppu`. Tracks "internal master clock at 4× PPU dot resolution." Initialized to 0.
- Add associated constant `MASTER_CLOCK_DIVIDER: u64 = 4` (one PPU dot = 4 internal master clocks).
- Add `pub fn run(&mut self, mapper: &mut dyn Mapper, target: u64)` — loop calling existing `self.tick(mapper)` while `self.ppu_clock + MASTER_CLOCK_DIVIDER <= target`, advancing `self.ppu_clock` by `MASTER_CLOCK_DIVIDER` each iteration.
- Keep existing `tick()` exactly as is — it advances one PPU dot and bumps `ppu_clock` by `MASTER_CLOCK_DIVIDER`.

**Verification gate:**
- `cargo test --release -p ricoh-ppu-2c02 --lib` — 105/105 unit tests pass.
- `cargo test --release -p machine-nintendo-nes --test blargg_ppu -- --ignored` — 11/12 (unchanged baseline).
- Tom Harte single-step still green.

**Rollback criteria:**
- Any unit test or downstream blargg test that was green goes red — back out the field and method, no further phases.

### Phase 2 — Scale machine master clock to 4× internal resolution

**Files:** `crates/machine-nintendo-nes/src/lib.rs`

**Changes:**
- Add internal `internal_master_clock: u64` to `Nes`. Initialized to 0.
- Keep the public `master_clock: u64` and `master_clock()` accessor — but as a derived value (`internal_master_clock / 4`) OR continue tracking it separately as "PPU dots since construction" for back-compat.
- Each `Nes::tick()` continues advancing `master_clock` by 1 (one PPU dot) and ALSO advances `internal_master_clock` by 4. PPU is still ticked via `self.ppu.tick()` once per call.
- `cpu_divider` logic unchanged.

**Verification gate:**
- nestest, blargg PPU 11/12, ppu_onscreen 22/22, NES sweep ≥60/110 — all unchanged.
- The contract for `master_clock()` to external callers is preserved (still counts PPU dots).
- `internal_master_clock` is private — not exposed to runtime, MCP, or tests yet.

**Rollback criteria:**
- Any harness sees different absolute master_clock values — back out.

### Phase 3 — Convert PPU to be driven by `run(target)` instead of dot-by-dot

**Files:** `crates/machine-nintendo-nes/src/lib.rs`, `crates/ricoh-ppu-2c02/src/lib.rs`

**Changes:**
- `Nes::tick()` no longer calls `self.ppu.tick(...)` directly. Instead it bumps `internal_master_clock` by 4 and calls `self.ppu.run(self.mapper.as_mut(), self.internal_master_clock)`.
- At this phase, the PPU still advances exactly 1 dot per `Nes::tick()` call (one `run` call advances 4 internal master clocks = 1 PPU dot via the existing loop). External behavior unchanged.
- The `Ppu::tick(...)` method becomes private (or `pub(crate)`); external callers must use `run(target)`. PPU unit tests that called `tick()` directly migrate to either calling `run(ppu.ppu_clock + 4)` for one-dot advance, or staying on the now-private `tick()` since they're in-crate.

**Verification gate:**
- All tests still pass at baseline (same as Phase 2).
- Search the workspace for external callers of `Ppu::tick` — none should remain outside `ricoh-ppu-2c02` itself.

**Rollback criteria:**
- Behavior shift in any test, or compile errors that can't be fixed in-place.

### Phase 4 — Phase split per CPU cycle (THE FIX)

**Files:** `crates/machine-nintendo-nes/src/lib.rs`

**Changes:**
- Restructure the inside of `Nes::tick()` so that when `cpu_divider == 0` (CPU cycle), it splits the master clock advance into two phases:
  1. **Start phase**: advance `internal_master_clock` by some amount (Mesen uses 5 for reads, 7 for writes — start with 5 for reads, 7 for writes mirroring Mesen's `_startClockCount`). Call `ppu.run(internal_master_clock - 1)`.
  2. **Bus op**: perform `cpu_read` or `cpu_write` based on `self.cpu.rw`.
  3. **End phase**: advance `internal_master_clock` to the cycle end (total +12 from start). Call `ppu.run(internal_master_clock - 1)`.
  4. **Sample pins**: `self.cpu.nmi = self.ppu.nmi; self.cpu.irq = ...;`
  5. **CPU tick**: `self.cpu.tick()`.
- On non-CPU master ticks (cpu_divider != 0), the code path stays simpler: advance `internal_master_clock` by 4, call `ppu.run(internal_master_clock - 1)`. Note the `-1` lag — the PPU is always one internal master tick behind the wall clock.

The `_ppuOffset = 1` semantics: PPU is permanently 1 internal master tick behind. At end of each `Nes::tick()`, `internal_master_clock` is at some value M, and `ppu.ppu_clock` is at M - 4 (rounded down to a multiple of 4, since PPU advances in whole dots).

**Verification gate (this is the moment of truth):**
- `cargo test --release -p machine-nintendo-nes --test blargg_ppu -- --ignored` — target 12/12.
- Re-run a focused dump for the four PPU NMI tests + the two cpu_interrupts_v2 NMI tests. All should match expected CRCs.
- Tom Harte still 2.56M green, Dormann green, nestest 8991/8991, ricoh-ppu-2c02 105/105 unit, ppu_onscreen 22/22.

**Rollback criteria:**
- The four target blargg tests don't all flip in this pass — investigate before declaring success. If 1-2 flip but others don't, that's a sign the phase split needs different `_startClockCount`/`_endClockCount` values (Mesen tunes these per region; we may need to experiment).
- If Tom Harte or nestest regresses — back out the phase split (Phase 4 only); Phases 1-3 stay because they're pure-additive structure.

### Phase 5 — Recalibrate test harnesses for the (now-changed) `master_clock()` semantics

**Files:** `crates/machine-nintendo-nes/tests/blargg_ppu.rs`, `tests/nes_sweep.rs`, `tests/ppu_onscreen.rs`, `tests/nestest.rs` if affected, `crates/emu198x-shell/src/script.rs` (run_ticks step), `crates/emu198x-shell/src/mcp_tools.rs` (run_ticks tool).

This only happens if Phase 2 exposes `internal_master_clock` to external callers — which the plan above defers. With the Phase 2 contract preserved, MAX_TICKS constants don't change.

**Open question for this phase:** do we want `master_clock()` external semantic to switch to the new fine resolution, or keep it at PPU-dot resolution as a separate API from a new `internal_clock()` getter? Decide based on what the runtime + MCP layer wants.

## Risks and mitigations

**Risk: PPU's internal state-update timing breaks when ticked via `run(target)` instead of one-dot-per-call.**

Mitigation: Phase 3 only changes the call site (machine → PPU), not the PPU's internal logic. The PPU still advances in whole dots inside `run`. Phase 3's verification gate confirms no behavior shift before Phase 4 starts.

**Risk: Mesen's `_startClockCount = 6` / `_endClockCount = 6` constants don't translate cleanly because of read-vs-write asymmetry — Mesen tunes for `startClockCount - 1` / `endClockCount + 1` on reads.**

Mitigation: implement the read-asymmetry exactly per Mesen — `_masterClock += (start - 1)` for reads, `(start + 1)` for writes; same shape for end. Match Mesen exactly until we have a reason to deviate.

**Risk: Sub-CPU-cycle ordering of `mapper.cpu_tick()` and `apu.tick()` matters and we miss it.**

Mitigation: those happen once per CPU cycle, currently after `cpu.tick()`. Keep them in the same place (after the end-phase + sample + CPU tick). Verify APU-driven tests (`apu_test`, `apu_reset` in the sweep) don't regress.

**Risk: PAL NES timing depends on a different multiplier (5 for PAL PPU divider instead of 4). Future PAL support would need to thread this through.**

Mitigation: make `MASTER_CLOCK_DIVIDER` a const for now; promote to a runtime field if PAL becomes a target.

**Risk: The MCP `run_ticks` step's semantic changes if `master_clock()` shifts resolution.**

Mitigation: Phase 5 explicitly defers this question. If the answer is "external `master_clock()` stays at PPU-dot resolution," no MCP change is needed.

## Definition of done

- Phases 1-4 committed atomically, each with the verification gate met.
- The four target blargg PPU tests + two cpu_interrupts_v2 NMI tests all PASS.
- All previously-green suites still green.
- NES sweep total: target 60+ (ideally 63+ if all six tests flip).
- Task #35 marked completed.
- The decision record stays accurate; if the implementation surfaced new constraints, an addendum to the decision is committed alongside.

## Commit cadence

One commit per phase. Commit message format follows the established session convention:

```
refactor(nes): <phase title>

<why>

<what changed>

<verification>: <suite name> green / counts
```

Phase 4 is the big one — multi-paragraph body explaining the start-end phase split, the `_ppuOffset` analog, and the resulting test deltas with CRC values.

## Open questions

1. **External `master_clock()` semantics**: keep at PPU-dot resolution (back-compat) vs switch to internal 4× resolution (matches Mesen)? Defer to Phase 5; decide based on whether the runtime / MCP layer wants the finer-grained access.
2. **Read vs write start/end clock counts**: start with Mesen's exact constants (5/7 for reads, 7/5 for writes); revisit if a test surfaces an asymmetry we don't expect.
3. **PPU-internal `_needStateUpdate` flag**: Mesen has a deferred state-update mechanism that runs on certain register writes (`$2001` rendering toggle, etc.). Our PPU has `prev_rendering_enabled` for similar purpose. Confirm during Phase 4 that this still works under `run(target)` driving.

## References

- [knowledge/decisions/nes-cpu-cycle-multi-phase.md](../../knowledge/decisions/nes-cpu-cycle-multi-phase.md) — binding decision.
- [knowledge/decisions/nes-clock-topology.md](../../knowledge/decisions/nes-clock-topology.md) — original NES clock topology, which this refactor refines, not replaces.
- `~/Projects/198x/emulators/nes/Mesen2/Core/NES/NesCpu.cpp` — reference implementation. `StartCpuCycle` / `EndCpuCycle` at lines 294 / 317; `_masterClock` and `_ppuOffset` are the relevant state.
- `~/Projects/198x/emulators/nes/Mesen2/Core/NES/NesPpu.h:140` — `Run(runTo)` template, the exact shape `Ppu::run(target)` should mirror.
