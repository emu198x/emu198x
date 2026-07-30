# Programmable HBLANK Write-Timing Corpus

This corpus asks what happens when Amiga programmable-horizontal-blanking
registers change after a relevant comparator position has passed on the
current line.

The corpus is emulator-neutral. It contains bootable ADF sources,
deterministic build tooling, case metadata, and interchange schemas. It does
not contain emulator assertions or a claim about physical hardware.

Every case restores a baseline on beam line 127. On beam line 128, the Copper
changes `COLOR00` to a visible marker and then writes one tested register.
The next output line confirms that the write reached the selected register.
This schedule distinguishes event-latched comparator behaviour from a
renderer that recomputes a geometric interval from the current register
values for every pixel.

## Building

The canonical build requires GNU Binutils for `m68k-elf` and Python's
standard library:

```sh
python3 tools/build.py
```

Generated ADFs, payloads, and `suite-v1.json` are written to `dist/`. The
manifest records source, case, artifact, and toolchain identities.

## Capture contract

Wait until the ready record at `0x0002ff00` contains `HBLK`, the expected case
number, and a field counter of at least eight. Capture three adjacent fields
without cropping, scaling, filtering, shaders, or automatic blank removal.

A capture must preserve the output row before the visible marker, the marked
mutation row, and the following control row. The capture record must declare
how those rows map to the producer's framebuffer. It must not infer that the
stimulus beam line and the producer's output row have the same number.

## Scope

Version 1.0.0 covers:

- moving `HBSTRT` behind the current beam;
- moving `HBSTOP` ahead after the original stop event;
- enabling `ECSENA` after `HBSTRT`;
- enabling `EXTBLKEN` after `HBSTRT`;
- enabling `BLANKEN` after `HBSTRT`.

It does not test gate disable writes, writes coincident with comparator
edges, AGA half-CCK write propagation, programmable vertical blanking,
programmable sync, variable totals, or analogue output.

## Related files

- [`cases/README.md`](cases/README.md) defines the five questions.
- [`src/README.md`](src/README.md) describes the on-machine schedule.
- [`schema/README.md`](schema/README.md) defines evidence interchange.
- [`references/README.md`](references/README.md) defines admissible evidence.
- [`../programmable-hblank/README.md`](../programmable-hblank/README.md)
  defines the separate steady-state corpus.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) defines extension rules.

All original material in this directory is dedicated to the public domain
under CC0 1.0. See [`LICENSE`](LICENSE).
