# C64-family audit remediation plan

> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.


**Created:** 2026-07-06 · **Scope:** GitHub issues #762–#781 (the 2026-07-06 C64-family audit) · **Goal:** close all 20, in a dependency-aware order that keeps every accuracy fix verifiable and avoids double-work.

## Bottom line

Five phases. Ship the two user-visible bugs first, then lay the test/decision scaffolding the accuracy work needs, bank the independent fixes, run the subsystem accuracy passes on that now-tested foundation, and finish with the drive-core refactor plus the format/feature work that rides on it. ~14–18 atomic PRs.

## Two sequencing rules

1. **Test infrastructure precedes the accuracy it proves.** #770 (SID unit tests), #768 (VIC CI render floor), #773 (6502 per-cycle bus assertion) land before the SID/VIC/CPU accuracy fixes that depend on them. Matches the project's "assert output, verify, cycle-accurate" ethos.
2. **The drive-core refactor precedes the drive/format fixes it absorbs.** #764 (extract the shared GCR/rotation engine) lands before the drive-touching items of #781, so those fixes are applied once, not once per drive.

## Cross-cutting consequence: catalogue re-bless

Accuracy fixes change catalogue hashes. This is the designed routing-version mechanism (`manifest/c64.toml` § Seam 4), not a surprise — but it shapes batching:

- Any SID-audio change (#762 is CIA not SID; #763, #769, #777) → bump `AUDIO_ROUTING_VERSION` (`mos-sid-6581`) and re-bless every C64 audio hash.
- Any VIC-frame change (#778, the colour-RAM item of #779) → bump `FRAME_ROUTING_VERSION` (`mos-vic-ii`) and re-bless frame hashes.
- **Batch SID and VIC work** so we re-bless once per subsystem, not per PR.

## Phase 0 — High-severity bugs (ship now)

Two small, standalone, user-visible correctness fixes. No dependencies.

- **#762 CIA TOD ÷5/÷6 prescaler.** Add the tenths prescaler to `mos-cia-6526`; rewrite the `tod_counts_at_pal_50hz` test to assert 10 Hz. (No catalogue impact — TOD doesn't drive frame/audio hashes.)
- **#763 SID sample-rate carry.** Bresenham/float remainder carry in the decimator. **Bumps `AUDIO_ROUTING_VERSION` + re-bless** — so this is also the first audio re-bless; fold later SID work into subsequent bumps.

## Phase 1 — Test & decision scaffolding (unblocks Phase 3)

- **#770 SID module unit tests.** voice/envelope/filter/external-filter — ring-mod, sync, combined waveforms, noise LFSR, TEST bit. Prerequisite for #769/#777.
- **#768 VIC-II CI render floor.** Commit a golden-frame fixture (1–2 programs) so one pixel-parity assertion runs in default CI. Prerequisite for #778.
- **#773 6502 harness per-cycle bus assertion.** Assert `cpu.addr`/`cpu.rw` per cycle against the Tom Harte trace. Catches the bus regressions #780 might introduce; shared with NES.
- **#776 VIC-II ARGB32-vs-indexed decision.** A decision record: exempt the VIC from Rule 11, or align it to an indexed `u8` framebuffer. **Gates #778** (pixel work shape depends on it). Recommend deciding here even if the answer is "record the exemption" (size S).

## Phase 2 — Independent bugs & docs (standalone, low-risk)

All standalone; can run in parallel with Phase 1.

- **#765** snapshot restores cartridge/GeoRAM/REU/1351-mouse.
- **#766** T64 skip-bad-entry instead of failing the container.
- **#767** wire `--turbo-tape` into the throttle (or remove it).
- **#774 + #775** docs drift (one PR): VIC-II knowledge doc → current pipeline; `disk_1581.rs` doc → fixed/asserted, plus the small 1581 OR-fold unit test.
- **#780** 6502 cleanups: `unreachable!` → graceful, JAM-doesn't-vector, ANE per-variant magic. (Verified by #773.)

## Phase 3 — Subsystem accuracy passes

Each on its now-tested foundation. Batch per subsystem for a single re-bless.

- **SID** (verified via #770, one `AUDIO_ROUTING_VERSION` bump): **#769** 8580 combined-waveform tables + noise lock-up, then **#777** residual fidelity (ring-mod polarity, TEST-bit ramp, open-bus reads, bitfade).
- **VIC-II** (per #776 decision, verified via #768, one `FRAME_ROUTING_VERSION` bump): **#778** pixel pipeline (off-window collisions, grey-dot/colour delay, `$DD00` one-cycle skew) + the colour-RAM-open-bus item of #779 (needs the VIC to expose its last-driven bus byte).
- **CIA / machine** (batch on `mos-cia-6526` + machine): **#771** select the 6526A for C64C + coverage; **#779** remainder (CIA2 FLAG, serial-SR input/CNT mode, power-on RAM pattern, the `.expect("REU present")` rule-19 cleanup).

## Phase 4 — Drive-core refactor + format/feature work

Largest and last, so it consolidates already-correct code.

- **#764** extract `common-commodore-drive-gcr` (or `machine-commodore-drive-core`): the GCR codec, `build_track_data*`, rotation/serialiser state machine, weak-bit LFSR — parameterised by side count. 1541/1571 embed it; 1581 unaffected. Reconcile the drifted format-dispatch (typed enum vs byte-sniff) into one path. Guard: the G64 real-load + D64/D71 round-trip + weak-bit tests must stay green.
- **#781** format-writer hardening — the drive-touching items (1581 flush contract, D71 side-1 write-back, G64 `write()` `try_from` guards) applied **on the consolidated core**, plus TAP v0 overflow note and the `save_disk` G64/.d64 mislabel.
- **#772** tape SAVE + 1581 disk SAVE persistence: wire `flush_tape_image`/`flush_drive_1581_image` into an MCP verb and/or UI action, mirroring `save_disk`.

## Risks & notes

- **Re-bless churn** is the main tax; the batching above minimises it. Re-verify each re-blessed hash against a screenshot/audio window (don't blind-bless).
- **#764 is the one large/risky item.** Keep it self-contained; it depends on nothing in Phases 0–3 except that the drive fixes in #781 wait for it.
- **Verify-the-symptom discipline:** after any shared-chip fix (6502, CIA, SID, VIC) re-run the whole C64 consumer chain, not just the unit crate.
- Order within a phase is flexible; the phase boundaries encode the real dependencies.
