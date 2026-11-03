# Integrate the Spectrum board split with the snow fix

Engineering-frontier continuation of the approved physical-owner split. On
2026-11-03 main advanced to ffe704759 while #2275 was qualified. Its published
ULA 0.3 adds causal snow state and changes the non-Timex envelope to v19.
The old split head aeb291a08 is retained in its remote branch and a verified
recovery bundle. Never overwrite that reviewed history.

The user approved the integration on 2026-11-03: eighteen explicit 48K-class
profiles write v20, rejecting v19 and earlier before payload decoding. Timex
retains v18; the other seven profiles retain main's v19. This supersedes the
initial plan's v18/v17 compatibility policy for the physical owners.

1. Replay the four reviewed split commits onto ffe704759 on a fresh branch.
   Preserve the published ULA 0.3 dependencies, snow changes and tests. Resolve
   retired machine manifest conflicts by retaining their intentional removal.
2. Update `runtime-sinclair-zx-spectrum/src/snapshot.rs` and version assertions:
   physical owners v20, Timex v18, other profiles v19. Expand atomic predecode
   rejection tests to cover v19 as well as the retained original v17 saves.
3. Keep `pre_split_77440f443.json` and its specimens immutable. Add the separate
   192-case baseline captured directly on ffe704759 (verified private archive
   `2026-11-03-spectrum-snow-baseline.tar.gz`, SHA-256
   24beaae20e69f0afbb672881552c0f5c3891e0024d73126506a2be1483ed4b2c).
   All 192 complete native outputs match the originals; 96 initial/final save
   pairs change. Compare complete current save bodies against that snow-fixed
   baseline, normalizing only the approved envelope and independent board tag.
4. Run the affected Rust suites, original-ROM boot/BASIC, CPU sweeps, atomic
   identity/rejection matrix and native continuation. Prove version rejection
   precedes decode with version-only and complete original v19 payloads.
   Run format/Clippy, browser target checks, workflow and registry/status gates.
5. Push the new reviewed branch and replacement PR; link and retire #2275 only
   once the replacement preserves its full work. Read normal CI results and
   merge with rebase after verification. Update #2144 with the exact limits.
6. Retain final heads and evidence in verified recovery archives; inspect and
   remove task worktree/cache/processes. Preserve unrelated workspaces.

Task resources: `/private/tmp/198x-spectrum-board-v20`,
`/private/tmp/198x-spectrum-board-v20-target`,
`/private/tmp/198x-spectrum-board-v20-evidence`.

## Browser selector correction

The JavaScript player catalogue still selected the retired `spectrum_48k` ID,
so the new runtime profiles lost the default's bundled-firmware marker. Updating
the guard's stand-in profiles to the explicit Issue 3 IDs reproduced the failure:
actual bundled variants `[]`, expected `sinclair-zx-spectrum/spectrum_issue3_48k`.
Update `web-player/catalogue.mjs`, `check-worker.mjs` and the existing firmware
guard together. The guard also asserts the default resolves to a real profile.
This preserves the prior single-default bundling policy and firmware identity
checks while making the approved selector retirement work through the browser.

## Verified integration

All 648 affected tests pass across 101 targets, including the nine owner
compile-fail doctests. The 352 baseline comparisons preserve the original native
outputs and exact snow-fixed saved bodies; the direct continuation matrix covers
296 inputs. The identity suite passes all 306 foreign board/RAM rejections,
90 complete v17 and 90 complete v19 rejections, and version-only predecode checks
for every current profile. All eighteen identified-ROM boot/BASIC cases pass;
eighteen unique observations and native 352×296 PNGs are verified.

Clippy with warnings denied, native consumers, both WebAssembly targets,
workflow lint, registry/status, dependency/evidence ownership, fixture/ignore/
doc-link hygiene and affected-package planning pass. The actual all-family fleet
producer passed through `makeCatalogue` yields 28 unique Spectrum profiles, the
explicit Issue 3 / 48 KiB default and its bundled firmware marker. The updated
firmware guard passes after reproducing the old-selector failure. The original
pre-snow manifest and all 103 catalogue expectations are unchanged.
