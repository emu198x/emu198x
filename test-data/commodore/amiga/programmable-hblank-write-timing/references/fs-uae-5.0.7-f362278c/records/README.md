# Capture records

## Purpose

This directory states the normalised observations extracted from the ten
registered FS-UAE write-timing captures.

## Scope

There is one record for every ECS-or-AGA profile and case pair. Each record
identifies the suite artifact, UAE-family producer, machine and firmware
hash, timed register stimulus, capture procedure, raw geometry,
source-derived beam mapping, and provenance.

The observation contains three doubled output lines: the pre-mutation
baseline, marked mutation output, and post-mutation control. Black, guard,
and marker runs use start-inclusive, stop-exclusive raw host-HIRES sample
intervals after excluding storage samples `[0, 2)`. The records also state
that the exact tested-register bus-write sample is not directly observable
in the framebuffer.

## Relationship to neighbouring sections

Records are the semantic layer over the neighbouring APNGs and run
manifests. They refer to captures, configurations, logs, suite payloads, and
ADFs by file name and hash. The corpus case metadata defines the questions
but contains no expected output; these producer observations do not change
that boundary.

## Expected contents

Ten files named `<profile>--<case>.json` are expected. They conform to
[`../../../schema/capture-v1.schema.json`](../../../schema/capture-v1.schema.json).
Each file must describe one observed producer run. Emu198x assertions,
cross-producer consensus, and claims about physical hardware do not belong
in these records.

## Related files

- [Package overview](../README.md)
- [Packaged captures](../captures/README.md)
- [Run manifests](../manifests/README.md)
- [Suite cases](../../../cases/README.md)
