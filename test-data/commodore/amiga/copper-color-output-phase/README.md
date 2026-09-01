# Copper Colour Output-Phase Corpus

This corpus asks where back-to-back OCS Copper writes to `COLOR00` become
visible relative to a DMA-fed bitplane marker in the same raw capture.

The corpus is emulator-neutral. It contains one bootable ADF source,
deterministic build tooling, unresolved case metadata, and evidence
interchange schemas. It does not contain emulator assertions, expected edge
positions, or a claim about physical hardware.

The probe displays one low-resolution bitplane in a fixed display window. A
single set bit produces a one-lores-pixel `COLOR01` marker on every active
line. On eight consecutive lines, the Copper restores the guard colour before
the visible interval, waits at one fixed horizontal position, and issues four
back-to-back MOVEs to `COLOR00` with no intervening WAIT. The middle line is
the declared measurement row. Short bitplane fetches end before the colour
sequence so bitplane DMA cannot insert variable slots between the MOVEs.

The same ADF is intended for PAL Amiga 500 OCS profiles in Emu198x, FS-UAE,
vAmiga, and other producers capable of preserving uncropped output. A capture
records what each producer emits. The source case does not select a correct
phase or treat related UAE products as independent implementations.

## Building

The canonical build uses GNU Binutils for `m68k-elf` and Python's standard
library:

```sh
python3 tools/build.py
```

Generated ADFs, raw payloads, and `suite-v1.json` are written to `dist/`.
The manifest records input and output SHA-256 digests and exact toolchain
versions.

## Capture contract

Wait until the record at `0x0002ff00` contains the `CCPH` magic, case number
1, schema version 1, and a field counter of at least eight. Capture at least
three adjacent PAL fields with blanking and overscan retained. Do not apply
filtering, shaders, scaling, automatic cropping, or an alignment search.

On beam line 132, measure the one-pixel bitplane marker and the four ordered
`COLOR00` transitions. Record their source-capture coordinates, each colour
edge relative to the marker, and the three adjacent-edge spacings. Captures
from different producers retain their native raw origins; the relative
measurements remove crop-origin differences without moving either image.

A candidate record must pass `schema/capture-v1.schema.json` and the
cross-field and bound-file checks in `tools/validate_capture.py` before it can
be registered as evidence.

## Scope

Version 1 contains one PAL OCS case. It covers Copper writes to `COLOR00` in
adjacent uncontended MOVE instructions at low-resolution output cadence.

It does not cover CPU writes, debugger writes, other palette registers,
bitplane-derived palette changes, Copper contention, ECS Denise, AGA Lisa,
super-hires output, analogue settling, or composite encoding. It does not
resolve physical OCS behaviour by software consensus alone.

## Related files

- [`cases/README.md`](cases/README.md) defines the unresolved question.
- [`src/README.md`](src/README.md) describes the on-machine schedule.
- [`tools/README.md`](tools/README.md) defines the reproducible build and
  semantic tests.
- [`schema/README.md`](schema/README.md) defines evidence interchange.
- [`references/README.md`](references/README.md) defines the evidence boundary.
- [`../programmable-hblank-write-timing/`](../programmable-hblank-write-timing/)
  provides the Copper marker conventions reused here.
- [`../sprite-horizontal-phase/`](../sprite-horizontal-phase/) provides the
  DMA-fed bitplane-anchor convention reused here.

All original material in this directory is dedicated to the public domain
under CC0 1.0. See [`LICENSE`](LICENSE).
