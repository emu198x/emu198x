# Captures

This directory contains one three-frame APNG for each captured machine profile
and case.

Frames retain the exact 716 by 570 RGBA8 output written by Copperline's raw
frame-dump path. APNG frame order is capture order. No frame is aligned,
cropped, scaled, filtered, colour-managed, or otherwise normalised before
packaging.

The corresponding JSON record in [`../records/`](../records/) binds the APNG
file hash, the SHA-256 of the concatenated decoded RGBA8 frames, field
identities, geometry, and semantic observations.

## Related files

- [`../README.md`](../README.md) defines the producer and capture limits.
- [`../records/README.md`](../records/README.md) defines the evidence records.
- [`../package-v1.json`](../package-v1.json) binds all package files.
