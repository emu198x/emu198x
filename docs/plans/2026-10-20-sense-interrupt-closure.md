# Close Sense Interrupt Status for an explicitly named controller configuration

Lane: Spectrum launch-hardening. User approved pausing Read Track expansion to
finish one operation. Current baseline e8f5728a5 (Emu0.39.0), not the historical
v0.36.0 source-inspection checklist. Root source-of-truth canon and Emu rules
reconsulted. No claim is earned by changing the meaning of partial.

## Fixed finish line

1. Verify command/result framing, ST0/PCN, one pending event consumed once,
   all supported units, ordering, active/completing seeks and interleaved commands.
2. Implement and verify READY-change event reporting and reset lifecycle against
   identified primary/reference evidence. Separate actual drive input from media
   presence; settle the input contract before adding public APIs or saved fields.
   Exact analogue/clock-revision timing stays a separate explicit claim, not a
   hidden requirement imposed after this digital contract passes.
3. Verify native-state continuations before/after event creation, partial result,
   consumption and reset, preserving source media. Old supported state behaviour
   must pass or an explicit compatibility decision precedes a schema change.
4. Run redistributable ROM-loaded SIS guest diagnostic twice with released tools,
   independent runtime/reference observations and wrong result/missing firmware/
   corrupt media controls. Audit every result byte and phase transition; no memory
   injection. Record fresh versus replay, exact identities and unresolved evidence.
5. Publish an operation-level verdict with frozen criteria and reproducible packet;
   retain every failed/not-applicable/blocked gate. Update the stale checklist by
   preserving its historical source binding, adding a current operation verdict
   rather than silently relabelling the whole controller or family.

## Sequence and owning files

- Emu `crates/nec-upd765a/tests/sense_interrupt.rs`, existing controller/reference
  harnesses and `runtime-sinclair-zx-spectrum` snapshot tests: inspect/reuse and
  extend meaningful state/event tests. Existing extra-byte correction1822/1823
  is already fixed and is not new work.
- Emu `crates/nec-upd765a/src/lib.rs` and +3 adapter: only reproduced gaps, with
  primary/reference rationale and red/green evidence. API/state design approval
  is required before writing new contract/serialized fields.
- code-samples authored diagnostic and umbrella existing build/disk/Run evidence
  mechanisms: reuse standard BASIC/CODE +3DOS mastering and normal ROM loader.
- Owning PRs, release if runtime changes, source and released tests, current visual
  operation report and issue links. Clean task worktrees/builds after preserving
  unique files/evidence and verifying remote refs.

## Initial observations, not final conclusions

- Current source has correct one-byte no-pending result and two-byte seek results.
- READY-change polling/input is absent; current pending array only receives seek
  completions. NEC Dec1978 p15 explicitly lists READY changes as SIS reasons.
- MAME SIS comments document uncertainty about IRQ with multiple pending events;
  Fuse behaviour differs. Do not pick an IRQ policy merely to match one emulator.
- Reset discards all pending state; assess reset/READY interaction separately from
  later-PC-controller four-reset-status conventions.
- Separate bug session investigates READY/IRQ contract before production edits.

## Approved input/state design and reference precedence

User approved explicit READY input and Spectrum native v6→v7 rejection on
2026-10-20. The A/B-specific NEC transcription establishes scanning after reset,
not the older application note's Specify-only start. Preserve READY input through
reset, reset sampled levels and rational poll phase; at4MHz each logical drive is
sampled every2.048ms between commands. This supplies nominal digital cadence,
not a physical +3 timing/revision trace. Source provenance is retained at umbrella
reference/by-topic/fdc-765/nec-upd765ab-datasheet-transcription.source.json.

set_ready(unit,bool) is independent of image bytes. Legacy callers without an
external input retain their existing image-presence default; +3 supplies motor
bit3 AND media levels for each logical alias. The motor output remains writable
when memory paging is locked. Full digital tests explicitly supply levels.

NEC Dec1978p16 and the A/B Invalid section require SIS after a seek interrupt.
Implement that documented command-order contract with a saved unacknowledged-seek
latch. Rejection preserves pending records and PCNs. An acknowledged IRQ with
other records pending is distinct from a new unacknowledged seek interrupt.
Fuse deliberately relaxes the rule for The New Zealand Story and MAME also
accepts other commands. Those reference disagreements remain explicit; no
upstream defect report or silicon-equivalence claim follows. Normal-ROM regression
is required before shipping. Independently review any exposed software conflict.
