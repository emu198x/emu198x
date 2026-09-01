# Evidence References

This directory is reserved for reviewed capture packages and concise records
of the documentary or source evidence used to construct the probe.

The register program and bitplane layout were checked against the Amiga
custom-chip register map and Copper and bitplane programming descriptions.
Those descriptions identify inputs and transfer order; they do not state at
which OCS Denise output sample a Copper colour write becomes visible.

A capture package must identify the exact corpus artifacts, producer build,
machine profile, firmware, raw capture, retained decoded pixels, capture
configuration, measurement method, and implementation family. The producer
adapter or documented procedure must reproduce the retained decoded bytes
from the raw capture before registration.

FS-UAE and WinUAE share the UAE implementation family and cannot supply two
independent votes. Software and FPGA results are comparative evidence, not
physical-hardware evidence. The first bounded comparison should capture the
same ADF on PAL A500 OCS profiles in FS-UAE and vAmiga; an additional unrelated
implementation can strengthen the software result without upgrading it to a
hardware result.

No third-party manual, emulator source, emulator binary, firmware, or user
interface capture is redistributed here. A registered image may contain only
the corpus-authored display output and retained blanking.

This directory is intentionally empty of observations in source version 1.
The correct OCS colour phase remains unresolved until admissible captures are
added and reviewed.

## Related files

- [`../cases/README.md`](../cases/README.md) contains the unresolved question.
- [`../schema/README.md`](../schema/README.md) defines capture records.
- [`../README.md`](../README.md) defines the capture contract.
