# Correct the +3 two-side status input

Lane: Spectrum launch-hardening. Issue #1876.

Approved by Steve on 2026-10-20: independent per-drive TS input and native
Spectrum snapshots v7→v8, rejecting v7. No new dependencies.

1. Reproduce TS set for an unprotected +3 disk with unchanged geometry in
   `crates/common-sinclair-zx-spectrum-amstrad-class/src/core.rs` tests.
2. Append independent optional TS levels and setter to
   `crates/nec-upd765a/src/lib.rs`. Preserve levels across reset. Expose
   the existing effective write-protect level so machine wiring supplies TS.
   Retain the legacy fallback for consumers not supplying an explicit input.
3. Wire +3 TS from write-protect in the existing drive-input sync path.
   Test protected/writable media, READY independence, head/unit echo,
   four generic units, +3 aliases, media changes and reset.
4. Advance `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs` to v8;
   update its version guard tests and `tests/boot_invariants.rs`. Test
   independent TS state and a pending SDS result through native restore.
5. Run controller, Amstrad-class and runtime regression tests, formatting
   and scoped Clippy. Retain red/green logs, review diff, commit and push PR.

Primary evidence: NEC March1979 application note printed p15 figure11
samples WP before TS; +3 service manual circuit page19 connects pin34
through an inverter to the drive's active-low write-protect line, without
mode-dependent multiplexing. zxsp `FdcPlus3.h::is_2sided` independently
maps TS to write-protect. These establish a digital mapping, not mechanical
timing, empty-drive sensor levels, or real-hardware qualification.

Further runtime qualification and release use the existing workflow. SIS's
frozen released evidence is not regenerated or silently upgraded.
