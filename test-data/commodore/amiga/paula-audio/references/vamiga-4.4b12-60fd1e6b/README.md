# vAmiga 4.4b12 Paula-audio capture package

This package answers what vAmiga 4.4b12 at revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` produced for Paula-audio suite
1.0.0.

The captures belong to the vAmiga implementation family. They are independent
software-derived evidence for Emu198x. They are not physical-hardware
evidence, specification authority, or an exact sample oracle for a different
filter and resampler.

## Producer boundary

The producer was built from
<https://github.com/dirkwhoffmann/vAmiga> through the adapter retained at
[`tools/vamiga-paula-audio-capture/`](../../../../../../tools/vamiga-paula-audio-capture/README.md).
The source tree was clean at the recorded revision. The compiled producer and
commercial Kickstart firmware are not redistributed.

Each case uses a fresh A500 OCS PAL machine with 512 KiB chip RAM and 512 KiB
slow RAM. The adapter selects vAmiga's A500 filter, linear interpolation to
48 kHz, disabled adaptive sample rate, and hard stereo panning. It waits for
the complete `PAUD` record at field 8 and captures fields 9, 10, and 11.

The source WAVs contain the exact finite binary32 samples returned by vAmiga's
interleaved audio API. No normalisation, gain adjustment, channel remapping,
clipping conversion, or dither was applied.

## Observed output

| Case | Dominant output | AC RMS | Fundamental |
| --- | --- | ---: | ---: |
| `channel-0-full` | right | 0.091604773 | 3463.059 Hz |
| `channel-1-full` | left | 0.091602194 | 3463.059 Hz |
| `channel-0-half` | right | 0.045798066 | 3463.059 Hz |

The inactive output is exactly zero in each source capture. The
channel-0 half/full RMS ratio is 0.499952835. The two full-volume channels
differ by approximately 0.0028 percent.

The result establishes a stable vAmiga-family observation for logical stereo
routing, programmed cadence, equal-channel level, and the paired volume
relationship. Exact RMS magnitude remains producer-specific because vAmiga
and Emu198x use different gain, filtering, and host-sampling paths.

## Contents

- [`captures/README.md`](captures/README.md) describes the source WAVs.
- [`records/README.md`](records/README.md) describes the schema-valid evidence
  records.
- [`configs/README.md`](configs/README.md) describes the exported vAmiga
  configurations.
- [`manifests/README.md`](manifests/README.md) describes the adapter's raw
  field and audio-buffer reports.
- [`logs/README.md`](logs/README.md) describes the producer and build logs.
- [`package.py`](package.py) validates a raw capture run and writes or verifies
  this package.
- `package-v1.json` binds the producer, adapter, capture files, records,
  configurations, and logs.
- `producer-build-v1.json` records the source, build tools, host, adapter
  source, and unredistributed binary identity.

## Interpretation limits

This package does not measure a physical Amiga output. It cannot establish
motherboard filter tolerances, noise, crosstalk, distortion, clipping, DC
offset, connector behaviour, or differences among board revisions.

It also does not establish every Paula timing edge. The suite covers one
steady period, two channels, two volumes, and an inactive switchable LED
filter. Minimum periods, period writes, modulation, interrupt timing, and the
remaining channel combinations require focused cases.

## Related files

- [Corpus overview](../../README.md)
- [Capture schema](../../schema/capture-v1.schema.json)
- [Reference policy](../README.md)
- [Paula-audio conformance process](../../../../../../knowledge/processes/amiga-paula-audio-conformance.md)
- [Paula stereo-routing decision](../../../../../../knowledge/decisions/amiga-paula-stereo-routing.md)
