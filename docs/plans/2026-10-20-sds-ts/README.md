# Correct +3 Two Side status

The +3 now supplies the controller's independent TS input from its existing
write-protect level. Disk presence and geometry no longer determine that bit
on the +3. Generic consumers without an adapter retain their legacy fallback.

NEC March1979 application note printed p15 figure11 establishes separate WP
and TS sampling. The +3 Service Manual circuit p19 supplies WR PROT through
an inverter in both modes; zxsp FdcPlus3.h independently uses is_wprot for TS.
See umbrella docs/audits/2026-10-20-sds-two-side-input.md for source identities,
precedent disagreements and remaining physical questions.

Run from this repository:

```sh
cargo test --locked -p nec-upd765a -p common-sinclair-zx-spectrum-amstrad-class -p runtime-sinclair-zx-spectrum
cargo clippy --locked -p nec-upd765a -p common-sinclair-zx-spectrum-amstrad-class -p runtime-sinclair-zx-spectrum --all-targets -- -D warnings
```

Verification: 544 tests passed, 62 ignored (no qualification credit). Seven new
tests cover independent inputs, selected head/unit and +3 aliases, media changes,
reset, native-state fixed points, latched SDS results and atomic v7 rejection.
The baseline one-sided writable-media reproducer ran once and failed as intended.
Formatting and scoped all-target Clippy pass. Logs and hashes are adjacent.

Native Spectrum snapshots are v8, with Steve's approval. v7 and earlier are
explicitly rejected; no migration is claimed. The intermediate layout-regression
log retains a failing synthetic-state test before its appended TS field was added.

This is source-build regression evidence, not fresh released whole-machine or
physical qualification. No ROMs or private native states are included. The
qualified operation count remains one of fifteen (SIS). #1876 remains open for
released guest evidence, drive-sensor boundaries and timing/revision scope.
