# Programmable HBLANK Conformance Corpus

This corpus asks how Amiga-compatible implementations expose programmable
horizontal blanking under a small set of controlled register configurations.

The corpus is emulator-neutral. It contains bootable ADF sources, deterministic
build tooling, case metadata, and interchange schemas. It does not contain
emulator runners, expected images, or implementation-specific assertions.

Every case sets a stable, non-black background, writes one documented
register configuration, publishes a machine-readable ready record in chip RAM,
and then leaves the configuration unchanged. A capture producer is responsible
for retaining blanking and overscan rather than cropping them away.

Observed results are deliberately unresolved in the source case file. A result
becomes evidence only when it is recorded in the capture schema with the exact
artifact, machine configuration, producer revision, and normalization choices.

## Building

The canonical build uses GNU Binutils for `m68k-elf` and Python's standard
library:

```sh
python3 tools/build.py
```

Generated ADFs, raw payloads, and `suite-v1.json` are written to `dist/`.
The manifest records every input and output SHA-256 digest and the exact
toolchain versions.

## Capture contract

Wait until the ready record at address `0x0002ff00` contains the `HBLK` magic,
the expected numeric case identifier, and a field counter of at least eight.
Capture at least three adjacent fields without filtering, shaders, scaling, or
automatic cropping. Record the result with
[`schema/capture-v1.schema.json`](schema/capture-v1.schema.json).

The background color and the NUL-terminated ASCII identity at
`0x0002ff20` distinguish cases. Neither identity is an expected blanking
outcome.

## Scope

This first version covers the fixed path, the three control gates, central,
wrapped, and equal programmable windows, plus AGA fine-position cases in
lores, hires, and super-hires.

It does not specify the correct output. It does not test vertical blanking,
programmable sync, variable line length, genlock, or analogue monitor behavior.

## Related files

- [`cases/README.md`](cases/README.md) explains the questions represented by
  the cases.
- [`src/README.md`](src/README.md) describes the probe's on-machine behavior.
- [`schema/README.md`](schema/README.md) defines evidence interchange.
- [`references/README.md`](references/README.md) identifies the documentary
  basis without redistributing third-party material.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) defines how to add cases and captures.

All original material in this directory is dedicated to the public domain
under CC0 1.0. See [`LICENSE`](LICENSE).
