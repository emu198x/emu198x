# Capture Logs

This directory contains the complete Copperline log for each packaged capture
run.

The logs retain the producer's machine identity, ready-record memory dump,
producer field labels, probe counter updates, render metadata, and frame-dump
completion. Each capture record names its corresponding log and SHA-256. The
top-level package manifest binds the same file to the capture-time manifest,
APNG, and record.

Logs are evidence transcripts. They are not expected output and do not assign
case status.

## Related files

- [`../README.md`](../README.md) defines the capture package.
- [`../manifests/README.md`](../manifests/README.md) describes capture-time
  input identity.
- [`../package-v1.json`](../package-v1.json) binds all package files.
