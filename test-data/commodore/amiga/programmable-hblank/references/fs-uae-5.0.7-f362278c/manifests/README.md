# Run manifests

This directory contains the capture-time manifest produced for every FS-UAE
run.

Each manifest binds the producer source, capture patch, binary, tools,
configuration, external firmware hash, suite artifact, immutable inputs,
ready observation, adjacent field labels, raw pixel hashes, field metadata,
and complete log.

Raw BGRA files are not redistributed. Their hashes and metadata are retained,
and `package.py` verified them before writing the APNG and schema record.

## Related files

- [Package overview](../README.md)
- [Producer logs](../logs/README.md)
- [Capture records](../records/README.md)
