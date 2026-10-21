# Read command READY admission

Lane: Spectrum launch-hardening. Base: released 1e1fb7957e48c5d196e16b908acbf2af8caddd4b.

Demonstrate whether explicit physical READY-low is ignored with mounted media.
Use existing interfaces only; no new dependency, API, state field or native version.

1. Add `crates/nec-upd765a/tests/read_ready.rs`: bounded command/result helpers, mounted READY-low and aliased selectors, full result drain, unchanged image and READY-high recovery. Read Deleted Data and Read Track share startup, so exercise both explicitly.
2. Run the reproducer before production edits and retain its failing output.
3. Correct only shared `start_read` admission, reusing `drive_ready` and result formatting. Do not change mid-transfer READY events, IRQ timing, single-sided drive wiring or mechanical timing.
4. Run controller tests, format and strict all-target controller Clippy. Save commands/results and red witness, push a narrow PR, retain for root review. Do not merge or release during v0.42.0 publication.

Primary: NEC uPD765A/B datasheet, printed 5-12, ST0.NR: not-ready Read/Write sets NR. Local source: `reference/by-topic/fdc-765/nec-upd765ab-datasheet.txt`, lines 509–515; original PDF SHA in source sidecar. Reference: vendored MAME `src/devices/machine/upd765.cpp` read_data_start lines 1838–1855 and read_track_start lines 2257–2273 sample READY and return NR|FAIL before data. Source inspection, not a fresh reference execution.

## Reproducer

The unchanged release failed all three command tests: expected result MSR `0xd0`, observed execution-preamble `0x70`. `red.log` records Cargo exit 101, three failures and zero passes. Fail-first used Rust 1.98.0; subsequent verification explicitly selects the repository-pinned Rust 1.99.0. The same test source is retained for the corrected run. Owning bug: https://github.com/emu198x/emu198x/issues/1929.

Root cause: shared `start_read` never sampled the existing physical READY input before `prepare_read_sector` searched mounted bytes. Correction: after initializing command status/progress, sample the aliased physical drive; deasserted READY sets abnormal+NR and drains via existing `read_result`. This applies to Read Data, Read Deleted Data and Read Track because all three enter the same startup. No newly claimed mid-transfer, IRQ or physical timing behavior.

The first full pinned-toolchain run passed the three new tests, then exposed four existing size/search tests that implicitly used an empty (default not-ready) drive while expecting missing-ID status. Their helpers now explicitly assert READY-high, preserving every size/search expectation and exercising that independent input. The initial failure log is retained as `tests-initial.log`; this is a fixture correction, not a change to the READY oracle.

## Local verification

Explicit Rust 1.99.0: controller suite 177 passed, 0 failed, 0 ignored; strict all-target controller Clippy and workspace format passed. Commands,300 second host limits and elapsed times are in `2026-10-21-read-data-ready/verification.json`, with logs, source hashes and compiler identity alongside it. Broader owning +3/common/runtime checks were subsequently run after the separate Read ID proof was sealed and its cache explicitly handed off. The frozen source worktree and receipts were not altered.

## Broader owning verification

With explicit Rust 1.99.0, the controller/+3 common/runtime suites passed 563 tests, 0 failed, 62 ignored across 60 result blocks. The ignores remain uncredited firmware prerequisites, not passes. Strict all-target Clippy for all three owning crates and workspace format passed. No runtime/native fixture changes were needed. `owning-verification.json` and `owning-{tests,clippy,fmt}.log` retain exact commands,600 second host limits and outputs. Review scope remains admission only; the broader Read Data qualification ticket is still open.
