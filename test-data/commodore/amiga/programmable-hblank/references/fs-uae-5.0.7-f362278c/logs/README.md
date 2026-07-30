# Producer logs

This directory retains the complete combined stdout and stderr from every
FS-UAE run.

The logs show the loaded ROM and ADF, effective machine and raster
configuration, CPU execution mode, first observed ready record, all captured
core and guest field labels, and capture completion. Timestamps, process
identifiers, and temporary absolute paths are capture-host details rather than
portable configuration.

Each log includes one `No disk in drive 0.` message during preliminary
default-core initialisation. In every registered run, that message precedes
the exact case ADF insertion, the matching `CODEX_READY` record, and all three
captures. It does not describe the effective capture configuration.

## Related files

- [Package overview](../README.md)
- [Run manifests](../manifests/README.md)
