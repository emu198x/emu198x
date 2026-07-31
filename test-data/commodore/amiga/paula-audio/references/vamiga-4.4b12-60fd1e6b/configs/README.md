# Producer configurations

This directory contains the complete RetroShell configuration exported by
vAmiga for each cold-boot capture.

The three files are byte-identical because the machine and host-audio
configuration is constant across cases. They remain separate so each evidence
record can identify the exact configuration used by its run.

Firmware paths and disk paths are not part of the exported configuration.
Their identities are retained by hash in the capture records.

## Related files

- [Package overview](../README.md)
- [Capture records](../records/README.md)
- [Producer build](../producer-build-v1.json)
