# Preserve TAP parity through playback and recording (#2315)

Lane: Spectrum launch-hardening. This continues the approved tape-integrity fix
from the Build198x handoff; no CPU, ULA or pulse timing changes are needed.

Cause: the 0.1 TAP decoder discards stored parity and the runtime reconstructs
it, so the ROM receives a repaired stream. Format198x 0.2 retains parity.

1. Add a valid/damaged tape regression to
   `crates/emu198x-spectrum-web/tests/tape_transport.rs`. Run against the
   current runtime and verify that the damaged tape fails the assertion.
2. Update the existing TAP dependency to 0.2 in the runtime, browser and 128K
   machine manifests and Cargo.lock. Use `tape_bytes()` at the runtime bridge
   and the four existing integration-test bridges. Preserve recorded parity
   in `flush_tape_image`, and adapt existing literals/tests.
3. Verify exact damaged bytes at the bridge and run the ROM regression in
   release mode, including the valid control. Run affected crate tests,
   formatting and Clippy. Add the ROM regression to the existing nightly
   z80test job (which already provisions the ROM), and compare SAVE exports
   against captured bytes in `tests/tape_save_roundtrip.rs`. Do not refresh
   unrelated catalogue fingerprints.
4. Commit and push a single fix PR. Verify the remote retains the commit,
   then remove this task's worktree and build cache. Leave prior Claude
   bisect worktrees and locked investigations intact.

Reference result: issue #2315 records MAME 0.289 rejecting the damaged CODE
block with ERR_NR 0x1A while Emu198x loads it with ERR_NR 0xFF. The regression
checks the ROM's displayed tape error, not a host-side checksum rejection.

## Verification evidence

- Before the fix, the paired ROM regression's valid control passed, and the
  damaged tape failed: it printed `43` and `0 OK, 30:1` instead of the tape
  error. This reproduces the corruption being silently repaired.
- With the 0.2 codec source preview and consumer fix, both cases passed in
  7.70 seconds. Final validation must use the published registry package.
- TAP 0.2.0 was initially unavailable because Format198x's release stopped on
  MDR's first publish; the coordinator is recovering that release separately.

- Final validation used the published TAP 0.2.0 package (registry checksum
  `a5a9465b3aea0ab68acbdad54efd6da3b7da567ff51fc1c279330d684f20d538`),
  without a path patch. The three affected crate suites passed: 464 tests,
  zero failures, 129 ignored fixture/slow tests. The two selected real-ROM
  tests then ran explicitly with `--ignored`: both passed.
- `cargo clippy --offline --locked --release -p runtime-sinclair-zx-spectrum
  -p machine-sinclair-zx-spectrum-128k -p emu198x-spectrum-web --all-targets
  -- -D warnings`, `cargo fmt --all -- --check`, `actionlint`, and
  `git diff --check` passed.
- No unrelated dependency updates: Cargo reports 106 existing dependencies
  behind latest; this change only updates the TAP codec.
