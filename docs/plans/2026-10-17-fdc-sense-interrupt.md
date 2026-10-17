# Return only ST0 when Sense Interrupt has no pending completion

Lane: Spectrum launch-hardening.

## Evidence and boundary

The controller currently classifies Sense Interrupt Status with no pending seek
completion as Invalid Command (`ST0=80`) but returns an extra zero PCN. NEC's
December 1978 µPD765 sheet, p. 9, distinguishes normal Sense Interrupt's ST0/PCN
from Invalid's single ST0; p. 16 describes the Invalid result. The later NEC
µPD765A/B sheet has the same one-byte Invalid result:
https://www.tiffe.de/Robotron/Bausteinuebersicht/FDC/necfdc.htm#Invalid

Neither inspected NEC sheet explicitly describes the no-pending Sense Interrupt
case. The existing classification is independently consistent with vendored
Fuse 1.7.0 `peripherals/disk/upd_fdc.c` (lines 1284–1298), lib765
`765fdc.c` (Sense Interrupt handler), and MAME `upd765.cpp` (Sense Interrupt
handler). Fuse's comment cites Intel 82078 for that classification, not direct
µPD765A measurements. This fixes the result length of the existing Invalid
classification; it does not claim hardware measurement, polling/READY coverage,
or revision equivalence with later controllers.

## Plan

1. Add public-register tests in `crates/nec-upd765a/tests/sense_interrupt.rs`:
   no pending, an active seek, and drained completed seeks. Read MSR between
   bytes and prove the following command is accepted. Run before the fix.
2. Remove only the synthetic PCN from the no-pending branch in
   `crates/nec-upd765a/src/lib.rs`; update private tests to check command readiness.
   Keep pending completions, saved fields and serialization unchanged.
3. Run all controller tests, Spectrum runtime snapshot tests, Clippy and format.
   Retain before/after logs beside this plan, file the smallest owning issue and
   open a separate PR. Do not merge during the parent release.

## Verification

Owning issue: https://github.com/emu198x/emu198x/issues/1822 .
Base: `4e5411084` (main, before release 0.36.0).
The three new register tests fail before the fix specifically because MSR stays
in result phase after ST0 (`208` versus `128`, or `209` versus `129`). After the
fix, all 107 controller tests pass. All 16 Spectrum runtime snapshot tests pass,
including the new one-byte result restore test and the retained released
v0.33.0 controller-state continuation. All-target Clippy for both crates and
workspace format checks pass. The 97 unrelated runtime tests filtered out by
the snapshot selector were not executed in this focused check.

Commands (Rust 1.99.0, source build; no firmware needed):

```sh
CARGO_TARGET_DIR=target-sense cargo test --locked -p nec-upd765a
CARGO_TARGET_DIR=target-sense cargo test --locked -p runtime-sinclair-zx-spectrum --lib snapshot::tests
CARGO_TARGET_DIR=target-sense cargo clippy --locked -p nec-upd765a -p runtime-sinclair-zx-spectrum --all-targets -- -D warnings
cargo fmt --all --check
```

No dependencies, public APIs, stored fields, enum tags or snapshot versions
changed. A previously saved two-byte result keeps its captured buffer on restore;
new no-pending commands produce the corrected one-byte result.

Reference identities (SHA-256):

- NEC December 1978 PDF: `e8b07db283aad56358cdace1bf4ac179f961484d10d6c1700b74d9734eea1117`.

- NEC A/B HTML transcription (retrieved 2026-10-17): `0ff23ca8365e602f56c7c3624c66d66fe0b92aa6fbf5e4f322e455542a517e2e`.

- Fuse 1.7.0: `845b960eb68cba9b21cba00cdc3c03cf0d4dc491ffd96efecb1d53336ce7f72d`.

- lib765: `ed46a860538ced2db543fce0ffa4cca887e439a53bfca6d70c53bcd3dd419b00`.

- MAME source snapshot: `71f284cdbbbded75012919220442a2e45cb808b376f4dfffa0d7722c182bde15`.
