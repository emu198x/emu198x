# Capture records

This directory contains one `capture-v1` JSON record for each APNG in
[`../captures/`](../captures/).

Each record identifies the exact suite artifact, Copperline revision, machine
configuration, external firmware hash, ready observation, adjacent fields,
source geometry, coordinate mapping, file hashes, observed blank edges, and
evidence classification.

Records describe Copperline output. They do not assign corpus expectations.

## Related files

- [`../README.md`](../README.md) defines the producer and interpretation
  limits.
- [`../logs/README.md`](../logs/README.md) describes the retained raw logs.
- [`../manifests/README.md`](../manifests/README.md) describes capture-time
  input identity.
- [`../../schema/capture-v1.schema.json`](../../../schema/capture-v1.schema.json)
  defines the record format.
