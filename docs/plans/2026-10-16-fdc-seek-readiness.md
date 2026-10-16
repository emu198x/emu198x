# Reject initially-not-ready Seek without moving PCN

Lane: Spectrum launch-hardening. Issue: https://github.com/emu198x/emu198x/issues/1801.

1. Add public register tests in `crates/nec-upd765a/tests/seek_readiness.rs`
   for initial RY clear, preserved nonzero PCN, same-cylinder requests,
   drive-select aliases, staged busy/interrupt completion and mixed ready/error
   interrupt queues. Run against the old implementation and record failures.
2. Change only `Command::SeekTrack` in `crates/nec-upd765a/src/lib.rs`:
   under the current image-presence readiness model, absent images stage
   IC=01|NR|SE and preserve PCN. Keep the existing completion countdown. Mount an
   image in the old success test so its ready precondition is explicit.
3. Run all controller tests, affected +3 library tests, controller Clippy for all
   targets/features, formatting and diff checks. Push a focused PR for review.

NEC December 1978 datasheet, printed p15 Seek, specifies NR and IC=01 for
initially-not-ready Seek. MAME `upd765_family_device::seek_start` in snapshot
9391e1eff80eb04abd58d48218a1d3b9643c1ea8 additionally sets SE and preserves PCN.
Fuse differs by returning IC=11; use NEC's IC definition, with MAME as
implementation precedent. No new public API, fields, dependency or schema.
READY changes during Seek, physical +3 wiring and motor/step timing remain
unqualified and outside this bounded fix.

Primary source: https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf
printed p15. Reference implementation:
https://github.com/mamedev/mame/blob/9391e1eff80eb04abd58d48218a1d3b9643c1ea8/src/devices/machine/upd765.cpp#L1716.

## Verification

Base: 6181ebe36 (v0.35.2 plus the invalid-command correction), Rust 1.98.0,
macOS arm64. Before the production change, all four new register tests failed
with exit 101. For the nonzero-PCN case the old response was `[33, 20]`, expected
`[105, 7]`; aliases/queued errors returned `[35, 20]`, expected `[107, 0]`;
the same-cylinder test returned `[32, 0]`, expected `[104, 0]`.

After the fix:
- `cargo test -p nec-upd765a --all-features`: 53 unit tests and 4 integration tests passed, none ignored.
- `cargo test -p machine-sinclair-zx-spectrum-plus3 --lib`: 6 passed, none ignored.
- `cargo clippy -p nec-upd765a --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

These are fresh source-build register and machine regressions. They do not
replace the earlier release-binary qualification evidence or qualify timing,
physical READY signals, or READY transitions during an in-flight Seek.
