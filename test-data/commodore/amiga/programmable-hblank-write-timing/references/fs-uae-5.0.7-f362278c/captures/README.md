# Packaged captures

## Purpose

This directory preserves the video output from the ten registered FS-UAE
write-timing runs.

## Scope

There is one APNG for every ECS-or-AGA profile and case pair. Each APNG has
three byte-identical adjacent fields. Frames are 756 by 576 RGBA8 images
produced by changing the captured BGRA byte order only. The packager performs
no crop, scale, filter, shader, colour management, or image alignment.

The first two samples and final four rows are producer storage padding and
remain present. The mutation evidence occupies the doubled baseline,
mutation, and control rows documented by the matching record; it is not
isolated into a cropped image.

## Relationship to neighbouring sections

These APNGs are the portable image form of raw framebuffers identified by the
run manifests. The neighbouring `records/` directory supplies their semantic
line and interval interpretation. `package-v1.json` binds each APNG and its
decoded pixels by SHA-256.

## Expected contents

Ten files named `<profile>--<case>.apng` are expected: five `ecs` captures
and five `aga` captures for the suite's five cases. No raw BGRA files,
screenshots of the frontend, firmware, or case ADFs belong here.

## Related files

- [Package overview](../README.md)
- [Capture records](../records/README.md)
- [Run manifests](../manifests/README.md)
