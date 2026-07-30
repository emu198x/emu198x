# Run manifests

## Purpose

This directory preserves the structured capture-time provenance for every
registered FS-UAE write-timing run.

## Scope

Each manifest binds the producer source, capture patch, binary, capture
tools, configuration, external firmware hash, suite artifact, immutable
inputs, ready observation, adjacent field labels, raw pixel hashes, field
metadata, and complete log. It records capture-time facts rather than the
semantic meaning assigned to output rows.

Raw BGRA files are not redistributed. Their hashes and metadata remain in
these manifests, and `package.py` verified their content before writing the
APNG and evidence record.

## Relationship to neighbouring sections

The manifests connect the requested configurations and producer logs to the
packaged captures. The neighbouring records add normalisation, the
baseline/mutation/control row mapping, and observed colour intervals.
`package-v1.json` hashes every retained manifest.

## Expected contents

Ten files named `<profile>--<case>.json` are expected, one per run. A manifest
must describe the raw capture and its provenance; normalised expected output
or implementation assertions do not belong here.

## Related files

- [Package overview](../README.md)
- [Producer logs](../logs/README.md)
- [Captured configurations](../configs/README.md)
- [Capture records](../records/README.md)
- [Packaged captures](../captures/README.md)
