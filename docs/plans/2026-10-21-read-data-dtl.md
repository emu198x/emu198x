# Read Data N=0 transfer length

Lane: Spectrum launch-hardening. Base ce2d755af. This is separate from MT correction #1933 and Read Data IRQ work. No dependencies, public APIs, new serialized fields or native version bump.

1. Add public-register `crates/nec-upd765a/tests/read_dtl.rs` fail-first cases: N=0 DTL prefix, deferred CRC, bounded internal tail, repeated/recovering commands, immutable images, N>0 ignoring DTL, DTL>=128 and Read Track unchanged, overrun before the prefix completes, multi-sector continuation.
2. Correct the existing read engine using physical `exec_buf` length and existing `exec_pos`/absolute slot clock. Suppress host requests after DTL while clocking through the remainder before field completion and CRC status. Keep the marginal-encoding decision unchanged. DTL0 remains explicitly unresolved/current behavior until an independent useful oracle establishes it.
3. Run controller tests, strict all-target Clippy and workspace fmt using explicit Rust1.99.0,300-second host bounds, preserve fail-first and verification outputs. Push a narrow PR for root review without merging/releasing it.

Primary: NEC December1978 datasheet printed p12, N=0 DTL sends only that prefix while reading the complete field/CRC internally. Primary extracts/provenance are in umbrella reference/by-topic/fdc-765. Fuse1.7.0 upd_fdc.c lines1448–1450 chooses prefix and lines1014–1024 consumes unseen suffix/CRC. Fuse's suffix loop is not a physical clock oracle; reuse our existing paced slots. MAME uses full FIFO sector_size and is not an independent DTL oracle. Historical Emu198x-Older controller has no DTL behavior to port. Read Track has a separate unresolved contract (#1870), so its transfer behavior remains unchanged.

## Result

Owning issue: https://github.com/emu198x/emu198x/issues/1936. Unmodified production code failed both prefix/CRC suffix cases: MSR0xF0 requested an extra host byte where0x70 was required; three independent boundary cases passed. `red.log` preserves Cargo101 and actual assertions. Reusing the complete buffer and position corrects only host delivery and internal draining; the original absolute slot phase and pending status persist.

Seven regression tests now pass, including exact full-field completion time with early/late host reads, no host requests or result before the suffix ends, CRC status, two successive sector prefixes, unchanged live image, repeat/recovery, overrun, N>0 ignoring DTL and Read Track unchanged. Synthetic N0 MFM timing cases test the existing nominal digital cadence only; they do not qualify physical density support. DTL0 is retained unchanged and unqualified.

Explicit Rust1.99.0 verification: 184 controller tests passed, zero failed/ignored; strict all-target Clippy and workspace fmt passed. `verification.json` records exact commands/environment,300-second host limits and actual elapsed times. No new serialized state or native format version.
