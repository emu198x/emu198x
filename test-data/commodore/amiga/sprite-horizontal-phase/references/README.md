# Evidence References

This directory is reserved for reviewed capture packages and concise records
of the documentary sources used to construct the probe.

The register program and sprite-data layout were checked against the Amiga
custom-chip register map and sprite and bitplane programming descriptions.
Those descriptions identify inputs and data structures; they do not settle
the output-sample phase asked by this corpus.

A capture package must identify the exact corpus artifacts, producer build,
machine profile, firmware, raw capture, retained decoded pixels, capture
configuration, measurement method, and implementation family. The producer
adapter or documented procedure must reproduce the retained decoded bytes from
the raw capture before registration. OCS, ECS, and AGA observations remain
distinct.
Software and FPGA results are useful comparative evidence but are not
physical-hardware evidence.

No third-party manual, emulator source, emulator binary, firmware, or user
interface capture is redistributed here. A registered image may contain only
the corpus-authored display output and retained blanking.

This directory is intentionally empty of observations in source version 1.
The correct horizontal interval remains unresolved until admissible captures
are added and reviewed.

## Related files

- [`../cases/README.md`](../cases/README.md) contains the unresolved question.
- [`../schema/README.md`](../schema/README.md) defines capture records.
- [`../README.md`](../README.md) defines the capture contract.
