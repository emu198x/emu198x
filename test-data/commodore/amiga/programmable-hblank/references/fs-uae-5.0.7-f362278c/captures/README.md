# Packaged captures

This directory contains one APNG per profile and case.

Each APNG has three byte-identical adjacent fields. Frames are 756 by 576
RGBA8 images produced by changing the captured BGRA byte order only. The
packager performs no crop, scale, filter, shader, colour management, or image
alignment.

The first two samples and final four rows are producer storage padding and
remain present. Capture records identify the decoded-pixel and APNG file
hashes.

## Related files

- [Package overview](../README.md)
- [Capture records](../records/README.md)
