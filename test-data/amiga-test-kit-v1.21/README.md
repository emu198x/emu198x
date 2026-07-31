# Amiga Test Kit v1.21 video references

This directory answers what belongs in the registered Amiga Test Kit v1.21
video-reference collection.

## Purpose

The collection holds independently produced digital frames used by the
explicit Test Kit video-conformance gates. Each reference family fixes the
machine configuration, producer, viewport normalisation, guest navigation,
settling rule, and image identities required for a reproducible comparison.

## Scope

This directory contains committed reference manifests and images. It does not
contain the Test Kit ADF, proprietary Kickstart firmware, Emu198x diagnostic
output, analogue-video captures, or ordinary boot-path regression goldens.

An Emu198x-produced frame does not qualify as an independent reference and
must not be added here as an expected image.

## Relationship to neighbouring sections

[`../amiga-test-kit-v1.21.md`](../amiga-test-kit-v1.21.md) registers the
shared external ADF and profile-specific firmware identities. The
[video-conformance process](../../knowledge/processes/amiga-test-kit-video-conformance.md)
defines how the explicit gates consume these references and how a result may
be interpreted.

Sibling directories below this collection represent separately registered
machine and reference configurations. A result for one directory does not
apply to another model, chipset, region, memory configuration, producer, or
capture geometry.

## Expected contents

- one subdirectory per registered machine and reference configuration;
- a profile-specific `README.md`;
- one strict provenance manifest;
- the exact PNGs named and hashed by that manifest.

The current collection contains:

- the [A500+A501 OCS PAL vAmiga reference](a500-a501-ocs-pal/README.md); and
- the [A1200 AGA PAL FS-UAE reference](a1200-aga-pal/README.md).

## Related documents

- [Amiga Test Kit v1.21 fixture identity](../amiga-test-kit-v1.21.md)
- [Amiga Test Kit v1.21 video conformance](../../knowledge/processes/amiga-test-kit-video-conformance.md)
- [Amiga Test Kit v1.12 fixture identity](../amiga-test-kit-v1.12.md)
- [Accuracy corpora](../accuracy-corpora.md)
