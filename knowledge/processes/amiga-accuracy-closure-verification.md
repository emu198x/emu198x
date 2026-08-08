# Running the Amiga accuracy-closure evidence set

This process answers how the Amiga closure lanes are run at one identifiable
revision and how their machine-readable result may be interpreted.

The runner records evidence for the pivot gate in the
[Amiga accuracy closure campaign](../decisions/amiga-accuracy-closure-campaign.md).
It does not replace the assertion boundaries in the individual conformance
processes and does not turn a passing result into a general Amiga-accuracy or
physical-hardware-conformance claim.

## Revision boundary

[`scripts/verify-amiga-closure.py`](../../scripts/verify-amiga-closure.py)
resolves the complete 40-character Git revision before starting. It refuses a
dirty worktree by default. This keeps generated evidence attributable to
committed source rather than to an unrecorded local patch.

`--allow-dirty` exists for diagnosis. A report produced that way records
`dirty: true` at the report, invocation and lane-attempt levels and is not
closure evidence. The runner also checks the revision and dirty state after
each completed lane. A change during a clean run fails the invocation.

The result is written below:

```text
target/accuracy/amiga-closure/<full-revision>/report.json
```

Lane logs occupy the sibling `logs/` directory. Their paths in the report are
relative to the revision directory.

A kernel-backed lock serialises report mutation for each revision. It is held
from report loading through lane execution, finalisation and optional archive
publication. A second invocation for the same revision fails before it can
reuse an attempt number or log path. The lock file remains below the ignored
`target/accuracy/amiga-closure/` directory; process ownership, rather than file
existence, determines whether the lock is active.

## Required external inputs

The complete run requires these path variables to be set explicitly:

- `EMU198X_AMIGA_TEST_KIT_ADF`, naming the registered Test Kit v1.12 input;
- `EMU198X_AMIGA_TEST_KIT_V121_ADF`, naming the registered Test Kit v1.21
  input shared by the OCS and AGA lanes;
- `EMU198X_AMIGA_A1000_KICKSTART_DISK`, naming the A1000 Kickstart disk used by
  the boot-golden matrix;
- `EMU198X_CATALOGUE_MEDIA_ROOT`, naming the catalogue media root; and
- `EMU198X_CATALOGUE_FIRMWARE_ROOT`, naming the catalogue firmware root.

Firmware needed by the focused wrappers is supplied through the direct ROM
variables documented by those wrappers or through `EMU198X_AMIGA_ROM_DIR`.
The golden matrix also consumes its documented local Workbench and Kickstart
assets. Its closure lane always sets `EMU198X_REQUIRE_GOLDEN_ASSETS=1`, so an
absent matrix input fails rather than producing a passing skip. The runner
removes inherited `EMU198X_UPDATE_GOLDENS`; verification cannot rewrite a
baseline.

Before either the golden or catalogue command starts, its strict wrapper runs
[`scripts/verify-amiga-closure-assets.py`](../../scripts/verify-amiga-closure-assets.py)
for that lane. The tracked
[`closure-assets-v1.json`](../../test-data/commodore/amiga/closure-assets-v1.json)
binds every consumed source container, selected archive member and normalised
payload by byte count and SHA-256. Its machine-readable attestation records
logical IDs and hashes but no local paths, and becomes part of the retained
lane log.

The runner records environment-variable names, not their values. Before a
line reaches the terminal or retained log, known ROM, media, Test Kit, disk,
home and temporary path values are replaced with named redaction markers.
Command arguments are not copied into the JSON; the report records the fixed
command ID instead.

Each wrapper remains responsible for verifying the byte identity of its
external input. Satisfying the path preflight is not evidence that the named
file is the registered artifact.

## Lanes

The lanes execute sequentially in this fixed order:

| Command ID | Evidence recorded |
| --- | --- |
| `amiga-regressions` | Hermetic common-chip, chipset, peripheral, machine and runtime library regressions plus the bounded integration set below. |
| `snapshot-roundtrip` | Snapshot byte fixed points and identical forward execution across the exercised OCS, ECS and AGA profiles. |
| `test-kit-v1.12` | Guest-reported Test Kit v1.12 execution and the selected A500/A530 assertions. |
| `test-kit-v1.21-ocs` | Exact registered A500+A501 OCS PAL video cases. |
| `test-kit-v1.21-aga` | Exact registered A1200 AGA PAL video cases. |
| `paula-audio` | Registered Paula routing, cadence and paired-volume comparison. |
| `programmable-hblank` | Steady-state consensus assertions and retained measurement-only cases. |
| `programmable-hblank-write-timing` | Independently remeasured mid-line write observations against the registered UAE-family package. |
| `golden-matrix` | Identity-verified required-asset boot-path regression images, including A1000 disk swapping. |
| `catalogue-ten` | The identity-verified, reviewed ten-entry OCS, ECS and AGA compatibility and snapshot/replay set. |

The regression lane invokes
[`scripts/verify-amiga-regressions.sh`](../../scripts/verify-amiga-regressions.sh).
That wrapper retains the declared library package set and explicitly names the
integration targets for Agnus arbitration and blitting, Paula disk behaviour,
floppy mechanism and MFM handling, OCS disk DMA and write-back, OCS/ECS/AGA
incremental blitting, runtime mount/query/lifecycle/interrupt behaviour, and
the checked-in Amiga catalogue-manifest contract. Diagnostic and
external-firmware integration tests are not selected implicitly.

The catalogue lane has an additional runner-level contract. Its log must
contain `[PASS]` and `[SNAP-PASS]` markers for exactly the reviewed entry IDs,
in manifest order. Ten different IDs, duplicated IDs, reordered IDs, or a zero
process exit without both exact sequences is a lane failure. The report retains
the expected and observed sequences.

Run the complete set from the repository root after exporting the required
inputs:

```sh
scripts/verify-amiga-closure.py
```

List command IDs without preflighting or running them:

```sh
scripts/verify-amiga-closure.py --list-lanes
```

## Interruption and retry

The report is replaced atomically before a lane begins and after it ends.
Each lane owns an append-only `attempts` list. A completed attempt records:

- the full revision and starting dirty state;
- UTC start and end timestamps;
- fixed command ID;
- process exit code and monotonic duration;
- relative log path and SHA-256; and
- any runner-level validation result.

If the runner is interrupted, completed lanes and the running attempt remain
in the last complete JSON document. The runner closes and hashes the current
log, marks that attempt `interrupted`, updates the report atomically and exits
with status 130 when it receives an ordinary interrupt it can handle. A host
power loss can leave the most recent attempt marked `running`; earlier
completed attempts remain intact.

Every log included in a retained archive must have a valid SHA-256. A
power-loss attempt that ended before the runner hashed its log therefore makes
that disposable report directory ineligible for archive. Remove that
revision's incomplete directory and run the complete set again when retained
evidence is required.

Retry one or more failed lanes with repeated selectors:

```sh
scripts/verify-amiga-closure.py \
  --lane programmable-hblank-write-timing \
  --lane catalogue-ten
```

Selectors do not change execution order. A retry appends an attempt rather
than deleting the previous failure. An invocation can pass while the overall
report remains `incomplete` or `fail`; overall `pass` requires the latest
attempt of every declared lane to pass at that revision.

## Retaining a passing report

`target/` is disposable and ignored by Git. After a complete passing run, use
the explicit archive option when the evidence is to be retained:

```sh
scripts/verify-amiga-closure.py --archive-passing-report
```

The option is rejected unless the overall report status is `pass`, the report
was produced from a clean tree, and every lane's latest passing attempt names
that clean revision and exited with status zero. It verifies the on-disk
report, every referenced log path and every referenced log SHA-256, then
rechecks that the repository is still clean at the same revision immediately
before staging `report.json` and the redacted logs. The complete revision
directory is published atomically below:

```text
test-data/commodore/amiga/closure-reports/<full-revision>/
```

An existing revision archive is never replaced. Publication uses a
revision-specific exclusive lock and fails if another publisher or retained
archive is present. This makes the checked-in directory an immutable record;
correcting evidence requires a new committed revision and a new closure run.

The archive step occurs after the runner's final repository-state check. It is
therefore expected to make an otherwise clean worktree dirty. Review the
archive size, report and logs before committing them. The runner retains full
redacted logs rather than substituting a summary.

## Disagreement registry

Every report embeds one registry whose allowed classifications are `fixed`,
`scoped-out` and `blocked-stronger-evidence`. This prevents a green lane from
silently converting a retained disagreement or assertion boundary into an
accuracy claim.

The runner pins the complete registry ID set and its order. Startup fails if a
row is added, removed or reordered without an explicit contract change, or if
one of the row's canonical document paths does not resolve to a repository
file.

The current registry covers:

| ID | Classification | Boundary |
| --- | --- | --- |
| `paula-stereo-channel-assignment` | `fixed` | Primary documentation adjudicated the vAmiga disagreement and the mixer was corrected. |
| `lisa-color-output-delay` | `fixed` | The one-hires-sample delay corrected the registered A1200 Test Kit disagreement. |
| `a1000-workbench-pointer-golden-baseline` | `fixed` | The stale Workbench 1.2 golden omitted the current pointer; the reviewed rebaseline retains those pointer pixels as an unmasked exact assertion. |
| `a1000-workbench-free-memory-readout` | `scoped-out` | The exact 60 x 18 mask excludes only six allocator-derived digits whose reviewed value moved from 131288 to 131224 bytes, four 16-byte allocation quanta. |
| `disk-read-dma-request-stage` | `blocked-stronger-evidence` | WinUAE and vAmiga select different read stages; direct hardware evidence remains desirable. |
| `programmable-hblank-ecsena-gate` | `blocked-stronger-evidence` | Audited implementation families disagree; the case remains measurement-only. |
| `programmable-hblank-extblken-gate` | `blocked-stronger-evidence` | Audited implementation families disagree; the case remains measurement-only. |
| `programmable-hblank-aga-blanken-path` | `blocked-stronger-evidence` | The AGA observation lacks cross-family agreement. |
| `programmable-hblank-aga-fine-phase` | `scoped-out` | The current comparison grid does not claim the final 35 ns phase bit. |
| `programmable-hblank-midline-write-timing` | `blocked-stronger-evidence` | The registered observation is from one UAE family and other audited comparators cannot answer the cases. |
| `paula-cross-producer-raw-waveform` | `scoped-out` | Raw equality and absolute RMS are excluded across different filter and resampling paths. |

Changing a classification requires changing the evidence and its canonical
campaign or conformance document first. Re-running the closure command alone
cannot promote one.

## Recorded evidence limits

The Test Kit and focused comparator wrappers verify their registered external
inputs by byte identity. The golden and catalogue wrappers additionally verify
all 16 logical ROM and media payloads and their 20 lane-specific source uses
against the tracked closure-asset manifest. The retained attestation contains
the manifest hash, logical payload hashes and counts without exposing the
directories from which those bytes were loaded.

Before the programmable-HBLANK write-timing consumer boots Emu198x, its wrapper
runs the independent retained-package verifier. That verifier decodes all 30
APNG frames, checks the decoded-pixel hashes and adjacent-field stability, and
re-derives the semantic runs without importing the packager's measurement
functions, image alignment or pixel tolerance. This strengthens the
capture-to-record chain but does not change the single-UAE-family assertion
boundary.

The registered producer packages retain capture-time absolute paths, host
details and operator fields in their full configurations, manifests and logs.
The closure runner redacts paths from newly generated closure logs, but does
not rewrite those already registered provenance files. Any public redacted
derivative must retain a verifiable relationship to the original bytes rather
than silently replacing the evidence package.

## Result interpretation

A passing report establishes that every declared command completed and met
its recorded assertion contract at the named clean revision, with no
unclassified disagreement erased by the runner. Each lane remains evidence
only for its declared fixtures, machine profiles, timing domains, comparator
families and assertion geometry.

The report does not establish untested software compatibility, physical
hardware agreement, preservation-grade floppy behaviour, analogue audio,
later-processor caches or MMUs, IDE, PCMCIA, or any other configuration beyond
the underlying lane documents.

## Related Documents

- [Amiga accuracy closure campaign](../decisions/amiga-accuracy-closure-campaign.md)
- [Amiga Test Kit v1.12 verification](amiga-test-kit-verification.md)
- [Amiga Test Kit v1.21 video conformance](amiga-test-kit-video-conformance.md)
- [Amiga Paula-audio conformance](amiga-paula-audio-conformance.md)
- [Amiga programmable-HBLANK conformance](amiga-programmable-hblank-conformance.md)
- [Amiga programmable-HBLANK write timing](amiga-programmable-hblank-write-timing.md)
- [Amiga boot-path golden capture](golden-image-capture.md)
- [Amiga closure asset identities](../../test-data/commodore/amiga/README.md)
- [Retained Amiga closure reports](../../test-data/commodore/amiga/closure-reports/README.md)
- [Catalogue startup navigation](../decisions/catalogue-startup-navigation.md)
