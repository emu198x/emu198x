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

## First native observation

Run 38920027025 verifies the genuine firmware, then fails the initial raw
checkout hash for the Makefile before invoking a released tool. The failed
report and cleanup log are retained. The harness now verifies the pinned Git
revision and exact Git blobs, permits only CRLF checkout conversion as transport,
and materialises the original bytes into task scratch. Real worktree edits and
wrong blobs still fail. A local CRLF-versus-real-edit control covers that case;
the next native run records actual checkout/materialised hashes and conversion.

## Reduced startup reproduction

Run 38921022873 builds and verifies all six TAP blocks, then stock Windows
Emu 0.34.2 exits 0xC00000FD with a main-thread stack overflow. Run 38921616978
reproduces the same failure with only query_cpu: no media or frames are needed.
The PE image reserves 1 MiB for its stack. This is an observed limit, not yet a
proof of which allocation exhausts it. No published binary was patched.

The next comparison pins current stable Emu 0.35.0 from its official archive
digest, retaining the 0.34.2 failures. Asm 0.0.67, Build 0.2.12, firmware, sample
and acceptance checks stay fixed. This checks whether the failure is already
resolved in the current publication; it does not reclassify old results.

Run 38923295182 reproduces the startup-only failure in 0.35.0. Issue #1792
tracks it. Native reports, command output, run metadata and artifact identities
are retained under the adjacent `2026-10-16-windows-developer-journey/` folder.
The packaging stage passes; game boot and the missing-CODE runtime control have
not run. This audit adds no core fix and makes no Windows qualification claim.

## Finish the published developer handoffs

The user approved finishing #112 after #1793 merged. Release #1795 publishes
v0.35.2 from reviewed merge 42e28e284d2b3120e850aea66bfcf097b6119afa.
Use actual public archive digests, not a source candidate. The Windows pin and
new Linux x64 pin select that publication; Asm/Build, sample and firmware stay
fixed. The original failed records and their hashes remain unchanged.

The full journey also needs a source stop. Reuse the exact first-program
source/session and source/disassembly/PC invariants from umbrella audit
5d9c49274b456306ff1ecde350b3260ccbbb64fc. Assemble the fixture afresh on each
native host; require a real missing-sidecar error and rejection of a wrong
source location. The existing package, TAP, title-address and runtime acceptance
functions are unchanged; `preserved-invariants.json` records their AST hashes.
Nine unit controls pass. Native published-release run 38937404950 passes on both
hosts; the adjacent RELEASE-RESULT.md records the outcomes and retained evidence.

The same driver now accepts only the declared native Windows x64 or Linux x64
host and validates PE/ELF architecture. Linux adds fixed archive pins and the
known ALSA/udev runtime prerequisites in its disposable runner. Both dispatches
remain opt-in; ordinary reusable Linux package qualification stays the default.
`verify_developer_journey_release.py` checks actual downloaded public bytes,
source/lock identity and the public receipt against the native producer report,
with four changed-receipt controls per target. Neither source-only qualification
nor an available archive counts as a completed developer handoff.
