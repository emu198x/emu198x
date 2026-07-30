# FS-UAE 5.0.7 current-generation capture package

This package answers what FS-UAE 5.0.7 at revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2` produced for programmable-HBLANK
suite 1.0.1.

FS-UAE identifies the underlying core as derived from WinUAE 6.0.1. These
captures therefore belong to the UAE implementation family. They are
software-derived evidence, not an independent vote from WinUAE, physical
hardware evidence, specification authority, or Emu198x expected output.

## Producer boundary

The producer was built from
<https://github.com/FrodeSolheim/fs-uae> with the capture-only patch retained
at
[`tools/fs-uae-hblank-capture/`](../../../../../../tools/fs-uae-hblank-capture/README.md).
The exact macOS arm64 binary has SHA-256
`81fdcc09bf36b6a275a9d39b27407e3484815b5713b411e16dbfe6024cf2899b`.
The binary and commercial firmware are not redistributed.

The hook copies a completed UAE chipset framebuffer before FS-UAE advances
the source pointer for its 752 by 572 compatibility view and before frontend
scaling, filtering, shaders, overlays, or GPU presentation. It is gated by
capture-only environment variables and does not write guest memory or change
chipset state.

Each case was cold-booted from an exact read-only ADF. The hook first observed
the ready record at guest field counter 1, waited eight further complete
fields, and captured counters 9, 10, and 11. Every three-field set is
byte-identical. The exact configurations, complete logs, run manifests, raw
frame hashes, producer patch, binary identity, and tool hashes are retained.

## Capture geometry

The package preserves the producer's 756 by 576 BGRA8888 host-HIRES raw
buffer, converted only by channel order to RGBA8 APNG. It does not crop or
scale that buffer.

The source audit establishes the coordinate mapping without image alignment:

- raw rows 202 and 203 are the doubled copies of beam line 128;
- raw `x=0` begins at HB coarse coordinate 46;
- for the main captured interval, an HB register word `r` maps to
  `4 * (r & 0xff) + floor(((r >> 8) & 7) / 2) - 184`;
- the Denise counter restarts inside the raw row, so raw samples 728 through
  755 represent coarse coordinates 1 through 7;
- the host-HIRES grid combines AGA fine phases 0/1, 2/3, 4/5, and 6/7.

FS-UAE reserves the first two samples and final four rows as zero-filled
compatibility storage. Those samples are retained in the APNG but excluded
from semantic blank-run classification. The compatibility frontend later
advances past the two left samples and reports 752 by 572; this package
records the earlier raw stage.

## Observed output

The table records semantic black runs on raw row 202 after excluding the
two-sample storage pad. Intervals are start-inclusive and stop-exclusive.

| Profile | Case | Observed black interval |
| --- | --- | --- |
| ECS | `fixed-control` | none |
| ECS | `ecsena-gate` | none |
| ECS | `extblken-gate` | none |
| ECS | `blanken-path` | none |
| ECS | `programmed-central` | `[328, 456)` |
| ECS | `programmed-wrap` | wraps through the row boundary: `[648, 756)` and `[0, 72)` |
| ECS | `programmed-equal` | empty |
| AGA | `fixed-control` | none |
| AGA | `ecsena-gate` | none |
| AGA | `extblken-gate` | none |
| AGA | `blanken-path` | `[328, 456)` |
| AGA | `programmed-central` | `[328, 456)` |
| AGA | `programmed-wrap` | wraps through the row boundary: `[648, 756)` and `[0, 72)` |
| AGA | `programmed-equal` | empty |
| AGA | `aga-fine-lores` | `[328, 459)` |
| AGA | `aga-fine-hires` | `[328, 459)` |
| AGA | `aga-fine-shres` | `[328, 459)` |

## Interpretation limits

The audited UAE path requires `BPLCON0.ECSENA` and `BPLCON3.EXTBLKEN` before
selecting enhanced external blanking. The ECS path uses the CSYNC-derived
blanking state and did not produce the programmed interval with
`BEAMCON0.BLANKEN` clear. The AGA programmable comparator still produced the
interval with `BLANKEN` clear.

This disagrees with the registered Copperline producer on the ECSENA and
EXTBLKEN cases for both profiles, and on the AGA BLANKEN case. The central,
wrapped, and equal CCK-aligned observations agree after each producer's
declared coordinate mapping. The disagreements remain unresolved and must not
be converted into expected output by majority vote.

The three fine-position captures verify UAE's path end to end at host HIRES.
They do not distinguish fine phases 6 and 7. A separate 1512-sample
host-superhires capture would be needed to observe every Lisa comparator
phase represented by this producer.

## Contents

- [`captures/README.md`](captures/README.md) describes the three-field APNGs.
- [`records/README.md`](records/README.md) describes the schema-valid evidence
  records.
- [`logs/README.md`](logs/README.md) describes the complete producer logs.
- [`configs/README.md`](configs/README.md) describes the exact generated UAE
  configurations.
- [`manifests/README.md`](manifests/README.md) describes the run manifests.
- [`package.py`](package.py) validates raw runs and writes the package.
- [`package-v1.json`](package-v1.json) binds inputs, outputs, producer, capture
  adapter, and packaging toolchain.
- [`producer-build-v1.json`](producer-build-v1.json) records the exact source,
  patch, build, binary, dependency, and capture-change identities.

## Related files

- [Corpus overview](../../README.md)
- [Capture schema](../../schema/capture-v1.schema.json)
- [Comparator capabilities](../comparator-capabilities.md)
- [Copperline capture package](../copperline-0.13.0-eec5806/README.md)
- [Conformance process](../../../../../../knowledge/processes/amiga-programmable-hblank-conformance.md)
