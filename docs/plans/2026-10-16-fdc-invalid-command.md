# Return the documented invalid-command result

Lane: Spectrum launch-hardening. Keep the controller API and native snapshot schema unchanged.

1. Replace the implementation-pinning test in `crates/nec-upd765a/src/lib.rs` with public register assertions for a one-byte 0x80 result, busy/direction transitions, ignored writes during results, repeated invalid commands, stale-result replacement and recovery to a valid command. Run these against the old implementation and retain the failing output.
2. Change only the unknown-command execution arm to set ST0=0x80 and enter the existing result phase. Cite NEC December 1978 datasheet pp.8,16 and Fuse invalid-command precedent. Valid but unimplemented commands remain separate work.
3. Run the complete controller tests, controller Clippy with all targets and features, formatting and an affected Spectrum build/check. Review the diff, commit and push a focused PR. Report results before merge/release.

Primary source: https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf pp.8,16. Reference implementation inspected: vendored Fuse `peripherals/disk/upd_fdc.c`, UPD_CMD_INVALID switch branch.

## Verification

Issue: https://github.com/emu198x/emu198x/issues/1797

Before the fix, `cargo test -p nec-upd765a invalid_command -- --nocapture` failed the new regression: main status was 128 (idle), expected 208 (result ready). The existing Sense Interrupt test passed in that same run.

After the fix:
- `cargo test -p nec-upd765a --all-features`: 53 passed, none ignored.
- `cargo test -p machine-sinclair-zx-spectrum-plus3 --lib`: 6 passed, none ignored.
- `cargo clippy -p nec-upd765a --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

These are source-build register and machine regression checks, not ROM-loader qualification or a timing claim. The earlier released qualification evidence remains pinned to its original executable.
