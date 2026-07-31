# Paula Audio Conformance Corpus

This corpus asks what steady-state waveform an Amiga-compatible implementation
produces from a small set of controlled Paula audio configurations.

The corpus is emulator-neutral. It contains bootable ADF sources,
deterministic build tooling, case metadata, and an evidence interchange
schema. It does not contain firmware, emulator runners, expected waveforms, or
implementation-specific assertions.

Each case disables unrelated DMA and interrupts, disables the switchable LED
filter, programs one audio channel with a repeating signed square wave,
publishes a machine-readable ready record in chip RAM, and then leaves the
configuration unchanged. A producer records stereo audio only after the ready
record has settled.

The comparison boundary is semantic. Captures are reduced to fundamental
frequency, left/right RMS levels, channel dominance, and paired amplitude
ratios. Raw sample equality is not required because producers may expose
different resamplers and declared analogue filter models.

## Building

The canonical build uses GNU Binutils for `m68k-elf` and Python's standard
library:

```sh
python3 tools/build.py
```

Generated ADFs, raw payloads, and `suite-v1.json` are written to `dist/`. The
manifest records every input and output SHA-256 digest and the exact toolchain
versions.

## Capture contract

Wait until the record at address `0x0002ff00` contains the `PAUD` magic, the
expected numeric case identifier, and a field counter of at least eight.
Record at least three adjacent PAL fields of stereo audio without automatic
gain control, channel remapping, time stretching, or noise suppression.

The producer must declare whether the capture is taken from its digital mixer,
its modelled analogue output, or an external hardware line output. Filtering
and resampling are retained and described rather than silently normalised.

## Scope

Version 1 covers:

- channel 0 routing at full volume;
- channel 1 routing at full volume; and
- channel 0 at half volume, paired with the full-volume channel 0 case.

All cases use the same `0x7f81` sample word and a period of 512 colour clocks.
This produces a robust low-kilohertz square wave that survives ordinary Amiga
output filtering and 44.1 or 48 kHz capture.

The corpus does not establish an exact analogue transfer function, noise
floor, distortion profile, interpolation mode, or minimum-period behaviour.
Those require narrower evidence.

## Related files

- [`cases/README.md`](cases/README.md) explains the questions represented by
  the cases.
- [`src/README.md`](src/README.md) describes the on-machine probe.
- [`tools/README.md`](tools/README.md) defines the reproducible build.
- [`schema/README.md`](schema/README.md) defines evidence interchange.
- [`references/README.md`](references/README.md) defines admissible reference
  producers.

All original material in this directory is dedicated to the public domain
under CC0 1.0. See [`LICENSE`](LICENSE).
