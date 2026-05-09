---
date: 2026-05-09
topic: spectrum-save-state-catalogue-harness
---

# Spectrum save-state catalogue harness

## What We're Building

A second pass over each Spectrum catalogue entry that proves
save-state is **lossless and deterministic** by re-running the
existing audio-capture window from a snapshotted-then-restored
runtime and asserting bit-identical output. Lifts SOLID criterion 8
(Save state) from PARTIAL to DONE: not by writing more snapshot
round-trip unit tests, but by binding the snapshot path to every
real-game scenario already gated by criterion 1's catalogue.

New `run_spectrum_entry_with_snapshot_check` function in
`crates/emu198x-catalogue/src/lib.rs`, called by the catalogue test
for every Spectrum entry. Public surface for the binary stays
unchanged.

## Why This Approach

**Why the catalogue, not more `variants.rs` tests** — `variants.rs`
already round-trips empty runtimes (snapshot → restore → snapshot,
byte-identical) for every variant. That covers data-structure
fidelity but not real-game state. Loaded games stress the snapshot
the way unit tests can't: the AY's 14 registers actually have
non-default values, paging is mid-flight, the screen file has real
content, the keyboard rows reflect script state. The catalogue is
the only place we have those scenarios already deterministic.

**Why a wrapper, not modify `run_entry`** — `run_entry` is consumed
by the catalogue binary's paste-into-manifest workflow which
doesn't care about snapshot. Adding new `EntryOutcome` variants and
a `assert_snapshot_fidelity` flag would complicate every caller for
one caller's benefit. A wrapper is the surgical extension; future
systems extend by adding their own wrapper.

**Why both byte-identity AND audio-fidelity** — they catch
different bugs. Byte-identity catches "snapshot lost a field" (the
restored runtime might still *behave* the same if the field defaults
to its real value on decode). Audio-fidelity catches "restored
runtime drifts" (transient state that wasn't snapshotted but matters
for forward-running emulation). Cheap to do both since the snapshot
bytes are already in hand.

## Key Decisions

- **Snapshot waypoint**: right after the original captures the boot
  frame, before the audio-gap advance. Same point the manifest's
  `frame_hash` anchors. The restored path then runs the same gap
  and audio window. Symmetric.
- **Five assertions** in order:
  1. Snapshot encode succeeds.
  2. A fresh-from-firmware runtime decodes the snapshot.
  3. Restored runtime's boot frame hash matches original.
  4. Restored runtime's audio hash matches original (the headline
     fidelity check).
  5. Re-encoding the restored runtime yields bytes byte-identical
     to the original encode (cheap data-integrity belt-and-braces).
- **New result type**: `SnapshotCheckResult` carrying outcome plus
  per-stage metadata (encoded bytes length, restored hashes). The
  wrapper returns `(RunResult, SnapshotCheckResult)`.
- **New outcome variants**: `EncodeFailed`, `RestoreFailed`,
  `FrameHashDrift`, `AudioHashDrift`, `BytesDrift`, plus `Pass`.
  All Spectrum-system-agnostic types so the same shape extends to
  C64/NES/Amiga later.
- **Variant-distinct fresh runtime**: 48K rebuilds via
  `Spectrum48kRuntime::from_firmware`, 128K via
  `Spectrum128kRuntime::from_firmware`, +3 via the +3 builder.
  No cross-variant snapshot trickery; each variant restores into
  its own type.
- **+3 entry included** — its boot waypoint is at the disk menu
  with the disk inserted; menu state is real runtime state worth
  round-tripping. The pinned FDC-load hang is post-menu and out of
  scope here.
- **Test wiring**: extend `crates/emu198x-catalogue/tests/run.rs`
  to call the wrapper for every Spectrum entry and assert
  `SnapshotCheckResult::outcome == Pass`. Keep the existing
  `RunResult` assertion as-is.

## Open / parked items (not in this commit)

- **Cross-variant snapshot rejection**: the type bound prevents it
  at compile time, but no test in the catalogue exercises that a
  128K snapshot decoded into a 48K runtime errors gracefully.
  `variants.rs` already covers this; not duplicating here.
- **Mid-script snapshots**: snapshot during the loading phase, not
  at the boot waypoint. Stronger guarantee but the loading-phase
  state isn't currently deterministic in a useful way (tape
  transport + pulse position). Out of scope.
- **C64 / NES / Amiga snapshot harness**: same shape, separate
  commit per system, only when those systems get their own SOLID
  criteria.
- **Snapshot bytes regression-locked into the manifest**: would
  bind the catalogue to postcard format. Existing hash-based bar
  is sufficient; defer.

## Next Steps

→ Implementation. Phase shape:
  1. Add `SnapshotCheckResult` + `SnapshotOutcome` types and the
     wrapper `run_spectrum_entry_with_snapshot_check` in `lib.rs`.
     Per-variant fresh-runtime construction inline; no need to
     pull `from_firmware` factories out.
  2. Extend `tests/run.rs` to call the wrapper for every Spectrum
     entry. Keep the existing assertion shape.
  3. Run locally against all 9 Spectrum catalogue entries. Expect
     the audio-fidelity check to surface real gaps if the
     snapshot has any drift; likely candidates if any fail —
     AY register state, contention counters, interrupt latch.
  4. Fix whatever the harness surfaces. The criterion-8 bar is
     "save state works on every catalogue entry"; bugs found
     are the criterion's own deliverable, not setbacks.
  5. Flip SOLID criterion 8 from PARTIAL to DONE.
