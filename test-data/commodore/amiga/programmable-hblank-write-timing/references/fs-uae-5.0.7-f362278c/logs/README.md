# Producer logs

## Purpose

This directory preserves the complete combined stdout and stderr from every
registered FS-UAE write-timing run.

## Scope

The logs show the loaded ROM and ADF, effective machine and raster
configuration, CPU execution mode, first observed ready record, all captured
core and guest field labels, and capture completion. Timestamps, process
identifiers, temporary absolute paths, frontend initialisation, and warnings
are retained capture-host details rather than portable configuration.

Each log includes one `No disk in drive 0.` message during preliminary
default-core initialisation. In every registered run, that message precedes
the exact case ADF insertion, the matching `CODEX_READY` record, and all
three captures. It does not describe the effective capture configuration.

## Relationship to neighbouring sections

Logs provide the human-readable execution trace behind the structured run
manifests. The neighbouring configurations record the requested setup;
manifests and package metadata bind the complete log files by SHA-256.
Semantic video observations belong in `records/`, not in these logs.

## Expected contents

Ten files named `<profile>--<case>.log` are expected, one for each ECS-or-AGA
profile and case pair. Logs are complete producer output and must not be
trimmed to only the capture-hook messages.

## Related files

- [Package overview](../README.md)
- [Run manifests](../manifests/README.md)
- [Captured configurations](../configs/README.md)
- [Capture records](../records/README.md)
