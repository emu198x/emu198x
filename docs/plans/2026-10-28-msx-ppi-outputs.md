# Live MSX PPI outputs (#242)

Lane: engineering-frontier. The user approved reusing the existing mixer with
nominal gain; no new dependencies or snapshot layout.

1. Recheck `RULES.md` and the machine-crate split decision. Do not introduce a
   model/region branch or widen the shared board catalogue. Primary wiring is
   MSX Technical Data Book §1.7.11, printed 43/PDF 46. The CASON correction is
   merged at umbrella c216a2cb; openMSX `MSXPPI::writeC1` and `KeyClick` agree.
2. Add failing audio and CPU-guest query/host-audio controls before production.
   Retain the actual red outputs. Zero-volume PSG plus a low click is the
   silence control; a guest completion sentinel proves the port writes ran.
3. In `machine-msx/src/lib.rs`, derive driven C4/C5/C6/C7 controls from the
   existing 8255 direction and latch state. Do not treat an input latch as an
   actively driven output. Sample click after each CPU half-cycle, including
   BSR/mode changes, on the existing master clock.
4. In `machine-msx/src/sound.rs`, add half-cycle click charge into the existing
   expansion accumulator without incrementing its T-state denominator twice.
   Reuse its averaging, sample cadence, DC blocker and saved partial history.
   Gain follows the already-documented nominal one-PSG-channel convention;
   this does not qualify physical per-board analog loudness.
5. In `runtime-msx/src/queries.rs`, expose the four live controls as grouped
   PPI fields and matching leaves. Preserve keyboard-row/slot/PSG behaviour.
   Cassette transport remains #240; no cassette media capability is claimed.
6. Verify half-cycle composition, retained generated samples, direction/BSR/
   mode changes, mid-sample snapshot replay and a real CPU-to-host audio path.
   Recheck the unchanged snapshot version/layout and absence of manifest edits.
   Run affected suites, strict Clippy and formatting; push and merge after CI.
7. Verify #242 closes, retain evidence and remove temporary worktree/cache.

The archive paths named by rule 25 are absent. This is a correction to the
current PPI consumer and existing mixer, not an archive port or a board split.

## Verification

The audio test failed before production with “PPI SOUND must reach the mixer”.
The CPU guest reached its completion sentinel before failing on the missing
“live output leaf”. Both pass after the change. The affected full suites pass:
90 passed, 0 failed, 9 fixture-ignored. Strict all-target Clippy passes.

The half-cycle control verifies a line high for one half contributes half the
charge of two high halves. Mid-audio-sample replay is verified at the existing
supported whole-T-state snapshot boundary. An exploratory half-T-state restore
failed because `cadence_hc` is already skipped by the saved-state contract; that
experiment does not justify changing the approved snapshot layout. The final
test honours the existing contract and compares audio plus serialized state.

Snapshot version 10 and all serialized fields remain unchanged. No manifest or
lockfile edits. CAPS and motor queries describe actively driven controls, not
floating input-pin voltage. Cassette transport remains #240. Physical gain
calibration is not claimed. Primary correction: stevehill1981/198x#285.
