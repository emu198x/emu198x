# Sprite Horizontal-Phase Conformance Corpus

This corpus asks where a fixed Amiga sprite appears horizontally relative to
two signals visible in the same raw capture: the retained hardwired horizontal
blanking edge and a one-bitplane marker.

The corpus is emulator-neutral. It contains one bootable ADF source,
deterministic build tooling, unresolved case metadata, and evidence
interchange schemas. It does not contain emulator runners, expected images,
or implementation-specific assertions.

The probe displays a static low-resolution bitplane and one solid sprite 0.
The bitplane contains a one-pixel vertical marker. Sprite DMA supplies the
same `SPR0POS`, `SPR0CTL`, `SPR0DATA`, and `SPR0DATB` words in every active
line. The probe restores the bitplane and sprite pointers at each vertical
blank, publishes a machine-readable ready record in chip RAM, and otherwise
leaves the register program unchanged.

The same ADF is intended for PAL OCS, ECS, and AGA profiles. A capture records
what each profile produces; the case definition does not state the correct
sprite edge or assign any chipset an expected offset.

## Building

The canonical build uses GNU Binutils for `m68k-elf` and Python's standard
library:

```sh
python3 tools/build.py
```

Generated ADFs, raw payloads, and `suite-v1.json` are written to `dist/`.
The manifest records input and output SHA-256 digests and the exact toolchain
versions.

## Capture contract

Wait until the record at address `0x0002ff00` contains the `SPHX` magic, case
number 1, schema version 1, and a field counter of at least eight. Capture at
least three adjacent PAL fields with blanking and overscan retained. Do not
apply filtering, shaders, scaling, or automatic cropping.

Measure the hardwired-HBLANK stop, bitplane marker interval, and sprite
interval on beam line 132. Record source-sample coordinates and both sprite
start deltas without shifting captures to make producers agree. The capture
schema requires the exact artifact, machine profile, producer revision, and
pixel transformations.

A candidate record must pass both `schema/capture-v1.schema.json` and the
cross-field and bound-file checks in `tools/validate_capture.py` before it can
be registered as evidence.

## Scope

Version 1 contains one low-resolution, 16-pixel-wide, unattached sprite case.
It controls AGA fetch width and sprite palette selection so the same source
program can be used by all three chipset profiles.

The corpus does not determine which observed edge is correct. It does not
cover attached sprites, sprite pairs, wide AGA sprites, border sprites,
collisions, vertical phase, mid-line register writes, or programmable
blanking. The bitplane marker is a second observable anchor, not an assumed
hardware truth.

## Related files

- [`cases/README.md`](cases/README.md) defines the single unresolved question.
- [`src/README.md`](src/README.md) describes the on-machine program.
- [`tools/README.md`](tools/README.md) defines the reproducible build.
- [`schema/README.md`](schema/README.md) defines evidence interchange.
- [`references/README.md`](references/README.md) defines the evidence boundary.
- [`../programmable-hblank/`](../programmable-hblank/) provides the build and
  capture conventions reused here.

All original material in this directory is dedicated to the public domain
under CC0 1.0. See [`LICENSE`](LICENSE).
