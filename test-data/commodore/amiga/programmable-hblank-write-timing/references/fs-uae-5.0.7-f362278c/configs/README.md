# Captured configurations

## Purpose

This directory preserves the exact generated UAE configuration used by each
registered run.

## Scope

The configurations select the registered PAL A500 Plus ECS or A1200 AGA
profile, exact case ADF, external Kickstart image, and capture settings.
Absolute paths identify isolated capture directories and the external
firmware location on the capture host. They are provenance, not portable
templates.

## Relationship to neighbouring sections

The capture adapter generated these files from its reusable template. The
neighbouring manifests bind every configuration by SHA-256 to its inputs and
run, while the logs show the configuration FS-UAE actually loaded.

## Expected contents

Ten files named `<profile>--<case>.uae` are expected, one for every packaged
capture and record. Hand-written alternatives, firmware files, ADFs, and
portable example configurations do not belong here. The reusable template is
in
[`tools/fs-uae-hblank-write-timing-capture/config.uae.in`](../../../../../../../tools/fs-uae-hblank-write-timing-capture/config.uae.in).

## Related files

- [Package overview](../README.md)
- [Capture adapter](../../../../../../../tools/fs-uae-hblank-write-timing-capture/README.md)
- [Run manifests](../manifests/README.md)
- [Producer logs](../logs/README.md)
