# Capture-Time Manifests

This directory contains the manifest written before each Copperline process
started.

Each manifest identifies the producer binary and source revision, UTC capture
time, operator, host, profile, case, command, environment, suite, capture
scripts, and SHA-256 of every copied input. The capture procedure verifies the
binary, scripts, environment, and copied inputs again after the producer
exits.

Firmware, ADF, payload, and suite copies remain in the temporary raw run and
are not redistributed here. Their hashes preserve the input identity.

## Related files

- [`../README.md`](../README.md) defines the capture package.
- [`../logs/README.md`](../logs/README.md) describes the retained producer
  logs.
- [`../package-v1.json`](../package-v1.json) binds all package files.
