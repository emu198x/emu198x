# Qualify one Windows Build198x-to-Emu198x handoff

The user approved a manually dispatched native Windows check using maintained
samples, pinned released tools and the existing 48K firmware secret. This is
Spectrum launch-hardening release validation, not emulator/core/timing changes.

1. Add `scripts/windows-developer-journey-inputs.json` with fixed sample revision
   51297ac3196a990286b0d21a4dedd4653c0168f0, exact Meteor Storm unit-35 recipe
   input hashes, released Windows archive identities and the genuine 48K ROM
   hash. Do not accept arbitrary caller-supplied code or download endpoints.
2. Add `scripts/windows-developer-journey.py` to verify inputs, extract the exact
   Windows binaries into task scratch and reproduce the existing Makefile's
   BASIC/SCREEN$/CODE commands without requiring a new make installation.
   Check TAP framing/checksums and its independently assembled CODE payload.
3. Cold-boot the actual composed tape with released Emu198x, normal LOAD path,
   no snapshots/patches. Require loaded entry bytes, title-state RAM, PC in the
   program and the title's actual rendered glyphs. Use the supplied ROM only as
   a private read-only oracle; never retain its bytes. Exercise a real tape
   missing CODE and require the runtime verifier to reject it. Bound commands,
   frames, downloads and job duration.
4. Extend existing `.github/workflows/test-linux-packages.yml` with an opt-in
   boolean dispatch input and Windows job; leave its existing reusable Linux
   path as default. Reuse the existing 48K secret only in a firmware setup step,
   verify its identity, clean it in an always step and upload an explicit safe
   evidence directory. No new workflow registration or release is needed.
5. Add focused tests in `scripts/test_windows_developer_journey.py` for media,
   missing/wrong execution and firmware controls. Verify rejection before
   trusting a passing fixture. Run scoped Python/Ruff/prose/workflow checks,
   push a PR and manually dispatch the existing workflow on that branch.
   Retain actual native-host results and failures without claiming GUI or
   installation-manager coverage.
6. Remove task-owned scratch, downloads, helper files and the clean worktree
   after verifying commits/evidence on the remote. Preserve unrelated worktrees.

Workspace: `/private/tmp/emu198x-windows-developer-journey`.
Local scratch: `.windows-journey-scratch/` there. Hosted scratch stays under
RUNNER_TEMP with distinct work, firmware and evidence directories. No firmware
is committed or attached to an artifact. Only native Windows can produce the
qualified runtime report; local unit controls do not count as Windows execution.
