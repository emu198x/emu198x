# Report Read ID errors when no sector ID exists

Lane: Spectrum launch-hardening.

Scope: correct the existing sector-image Read ID result, preserving command timing,
serialization and public APIs. The selected track must contain an ID before the
command can report normal completion. No new dependencies.

Evidence: NEC µPD765 December 1978 data sheet, printed p14 READ ID and pp17–18
ST0/ST1; https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf.
Fuse `peripherals/disk/upd_fdc.c`, `read_id` and `start_read_id`, corroborates
abnormal termination with ST1.MA/ND when no ID is found. The Intel 8272A is not
silently substituted for NEC silicon. No original-hardware observation is claimed.

1. Add public-register regressions in `crates/nec-upd765a/tests/read_id_errors.rs`
   for a mounted empty track, absent track/head and no mounted image. Confirm the
   assertions fail before modifying `src/lib.rs`; retain the command/output.
2. Change only the Read ID no-ID branch in `crates/nec-upd765a/src/lib.rs` to return
   abnormal IC with MA/ND. Keep existing failed-result CHRN compatibility, treating
   it as unspecified rather than a sector identity. Do not infer drive-ready pins
   from an absent image: those pins are not exposed by this model.
3. Check seven-byte result draining/idle recovery, successful nonstandard CHRN and
   rotation, stale-error clearing, and +3 US1 aliasing through public registers.
4. Run affected crate tests, Spectrum integration tests/build and Clippy; run normal
   CI on a reviewable PR. Record tests and limitations in the PR. No manual version
   bump; release maintenance owns the next patch.

Remaining gaps: physical two-index-pulse search timing, readiness changes and
A-revision distinctions, ID CRC representation versus data CRC, density mismatch,
and real drive geometry remain separate work. This fix does not qualify any
additional machine configuration or media format.

## Verification

Baseline: `42e28e284d2b3120e850aea66bfcf097b6119afa` (v0.35.2 source).
Host: macOS arm64, rustc 1.99.0 (`b940084d7`), Cargo 1.99.0.
This is fresh source-build execution, not a distributed-binary qualification or
reference-emulator replay. Fuse was inspected, not executed.

`before.txt` and `after.txt` capture fresh sequential register traces from:

```sh
EMU198X_FDC_TRACE=1 cargo test -p nec-upd765a --test read_id_errors -- --nocapture --test-threads=1
```

With the same five tests and baseline library source, exit 101: four missing-ID
cases fail (`ST0.IC` 0 instead of 0x40), valid CHRN/rotation/alias control passes.
With the fix, exit 0: all five pass. Empty/absent track and absent image result
changes from `00 00 00 00 00 01 02` to `40 05 00 00 00 01 02`; absent head from
`04 00 00 00 01 01 02` to `44 05 00 00 01 01 02`. The last four bytes on an
error are filler retained for compatibility, not a successfully observed ID.

Other fresh checks:

- `cargo test -p nec-upd765a -p common-sinclair-zx-spectrum-amstrad-class -p machine-sinclair-zx-spectrum-plus3`: 100 passed; one explicitly ignored firmware test.
- `cargo test -p machine-sinclair-zx-spectrum-plus3 --test boot_test -- --ignored --exact boot_to_menu_renders_screen_content`: the ignored test was then explicitly run against legitimately available local firmware, one passed.
- `cargo clippy -p nec-upd765a -p common-sinclair-zx-spectrum-amstrad-class -p machine-sinclair-zx-spectrum-plus3 --all-targets -- -D warnings`: passed.
- `cargo build -p machine-sinclair-zx-spectrum-plus3`: passed.
- `cargo build -p emu198x-spectrum --no-default-features`: passed (supported headless CLI/MCP path).
- `cargo fmt --all --check` and `git diff --check`: passed.

No ROMs or runtime snapshots are included in these logs. This is a controller
regression fix; no additional source-to-media-to-ROM-load configuration is claimed.
