# FS-UAE 5.0.7 Mid-Line HBLANK Write-Timing Capture Package

## Purpose

This package answers what FS-UAE 5.0.7 at revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2` produced for programmable-HBLANK
write-timing suite 1.0.0.

FS-UAE identifies the underlying chipset core as derived from WinUAE 6.0.1.
These captures therefore belong to the UAE implementation family. They are
software-derived evidence, not an independent vote from WinUAE, physical
hardware evidence, specification authority, or Emu198x expected output.

## Scope

The package contains ten cold-booted runs: each of the five suite cases on
one ECS and one AGA profile. Each case resets its baseline on beam line 127,
marks the mutation line with a `COLOR00` write, changes one tested register
after a relevant horizontal position, and retains the following control line.

The cases cover moving `HBSTRT` behind the beam, moving `HBSTOP` ahead after
the original stop, and enabling `ECSENA`, `EXTBLKEN`, or `BLANKEN` after the
programmed start. They do not cover disabling selectors, writes coincident
with comparator edges, AGA half-CCK write propagation, other programmable
beam controls, or analogue output.

## Producer boundary

The producer was built from <https://github.com/FrodeSolheim/fs-uae> with the
shared capture-only patch retained at
[`tools/fs-uae-hblank-capture/`](../../../../../../tools/fs-uae-hblank-capture/README.md).
The write-timing adapter invokes that same patched producer with the new
suite and has separate hash-bound runner, manifest, and configuration tools.
The exact macOS arm64 binary has SHA-256
`81fdcc09bf36b6a275a9d39b27407e3484815b5713b411e16dbfe6024cf2899b`.
The binary and commercial firmware are not redistributed.

The hook copies a completed UAE chipset framebuffer before FS-UAE advances
the source pointer for its 752 by 572 compatibility view and before frontend
scaling, filtering, shaders, overlays, or GPU presentation. It is gated by
capture-only environment variables and does not write guest memory or change
chipset state.

Each run used an exact read-only ADF. The hook observed the case's ready
record, waited until guest field counter 9, and captured counters 9, 10, and
11. The three fields are byte-identical in every run. Exact configurations,
complete logs, run manifests, raw-frame hashes, producer patch identity,
binary identity, and packaging-tool identities are retained.

## Capture geometry

The package preserves the producer's 756 by 576 BGRA8888 host-HIRES raw
buffer, converted only by channel order to RGBA8 APNG. It does not crop or
scale the buffer.

The source-derived mapping and visible guard transitions identify three
doubled output lines:

- raw rows 202 and 203 are the pre-mutation baseline;
- raw rows 204 and 205 are the marked mutation output;
- raw rows 206 and 207 are the post-mutation control.

The mutation row was discovered from its guard and marker colours rather than
assumed from the guest beam-line number. Raw `x=0` begins at HB coarse
coordinate 46. For the main captured interval, an HB register word `r` maps
to `4 * (r & 0xff) + floor(((r >> 8) & 7) / 2) - 184`.

FS-UAE reserves raw samples `[0, 2)` and the final four rows as zero-filled
compatibility storage. They remain in the APNG. The two left samples are
excluded from semantic colour and blank-run classification.

## Observed output

The table records black-output intervals on the three relevant lines.
Intervals are raw host-HIRES samples, start-inclusive and stop-exclusive.
`none` means no black interval after excluding the storage pad.

| Profile | Case | Baseline | Mutation output | Following control |
| --- | --- | --- | --- | --- |
| ECS | `midline-hbstrt-past` | `[520, 584)` | none | `[264, 584)` |
| ECS | `midline-hbstop-future` | `[200, 264)` | `[200, 264)` | `[200, 520)` |
| ECS | `midline-ecsena-enable` | none | `[390, 520)` | `[264, 520)` |
| ECS | `midline-extblken-enable` | none | `[390, 520)` | `[264, 520)` |
| ECS | `midline-blanken-enable` | none | none | `[264, 520)` |
| AGA | `midline-hbstrt-past` | `[520, 584)` | none | `[264, 584)` |
| AGA | `midline-hbstop-future` | `[200, 264)` | `[200, 264)` | `[200, 520)` |
| AGA | `midline-ecsena-enable` | none | none | `[264, 520)` |
| AGA | `midline-extblken-enable` | none | none | `[264, 520)` |
| AGA | `midline-blanken-enable` | `[264, 520)` | `[264, 520)` | `[264, 520)` |

The two comparator cases behave alike on the registered ECS and AGA
profiles. Moving `HBSTRT` behind the current beam does not manufacture a
start event. Moving `HBSTOP` ahead after the old stop does not reassert
blanking; the new stop participates on the following line.

The selector cases expose a profile distinction. On ECS, enabling `ECSENA`
or `EXTBLKEN` exposes the already-latched state after the producer's output
pipeline delay, beginning at sample 390. On AGA, those writes do not
manufacture the missed start event. Enabling `BLANKEN` does not expose
blanking on the ECS mutation line, while the AGA result is unchanged because
the registered UAE AGA programmable route does not use `BLANKEN`.

These observations are consistent with the audited UAE event model. The
source audit and the capture are evidence about the same implementation
family, not two independent implementations.

## Evidence limits

The scheduled magenta `COLOR00` move proves Copper progress and precedes the
tested register move. The framebuffer does not expose the tested register's
exact bus-write sample, so every record leaves that sample unknown. On the
AGA `midline-blanken-enable` mutation line, existing blanking hides the
marker until sample 520; that image cannot localise the write within the
blank interval.

The host-HIRES output records producer-visible results, including its
pipelines. It does not by itself identify internal latch timing. The matrix
contains one UAE-family producer, one PAL configuration per chipset profile,
and no physical-hardware capture. The corpus's expected output therefore
remains unresolved.

## Relationship to neighbouring sections

This directory is one registered producer beneath the write-timing corpus
`references/` section. The sibling source audit explains the UAE code paths
which produced these observations, while the comparator-capability document
records why the currently audited Copperline and vAmiga revisions cannot
produce admissible observations for this suite. The separate
`programmable-hblank` corpus covers steady-state geometry and selector
behaviour rather than mid-line writes.

Within this package, captures are the retained output, records are the
normalised semantic evidence, and configs, logs, and manifests preserve the
run provenance needed to audit those records.

## Expected contents

- [`captures/README.md`](captures/README.md) describes the ten three-field
  APNGs.
- [`records/README.md`](records/README.md) describes the ten schema-valid
  evidence records.
- [`logs/README.md`](logs/README.md) describes the complete producer logs.
- [`configs/README.md`](configs/README.md) describes the exact generated UAE
  configurations.
- [`manifests/README.md`](manifests/README.md) describes the capture-time run
  manifests.
- [`package.py`](package.py) validates the registered raw runs and writes the
  package.
- [`package-v1.json`](package-v1.json) binds suite inputs, runs, outputs,
  producer, capture adapter, and packaging toolchain.
- [`producer-build-v1.json`](producer-build-v1.json) records the exact source,
  patch, build, binary, dependency, and capture-change identities.

## Related files

- [Corpus overview](../../README.md)
- [Capture schema](../../schema/capture-v1.schema.json)
- [Comparator capabilities](../comparator-capabilities.md)
- [UAE event-model source audit](../uae-event-model-source-audit.md)
- [Steady-state FS-UAE package](../../../programmable-hblank/references/fs-uae-5.0.7-f362278c/README.md)
- [Conformance process](../../../../../../knowledge/processes/amiga-programmable-hblank-write-timing.md)
