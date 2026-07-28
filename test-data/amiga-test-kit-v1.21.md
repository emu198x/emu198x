# Amiga Test Kit v1.21 fixture identity

This record answers which exact external files constitute the registered
Amiga Test Kit v1.21 video-conformance fixture.

## Registered inputs

| Normalised name | Size | SHA-256 | Redistribution |
|---|---:|---|---|
| `amiga-test-kit-v1.21.adf` | 901,120 bytes | `abe7426c93619a7bb61ce10e3e66a4747fcaf22acd1d1876310033faa700ad28` | Public domain |
| `kickstart-1.3-r34.5.rom` | 262,144 bytes | `ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53` | Proprietary; do not redistribute |

The machine-readable form is
[`amiga-test-kit-v1.21.sha256`](amiga-test-kit-v1.21.sha256). The normalised
names describe the files after any delivery archive has been unpacked. An
outer ZIP filename or checksum does not replace the payload checksum.

The registered ADF has also been observed in the upstream delivery archive:

- name: `AmigaTestKit-1.21.zip`;
- size: 198,645 bytes;
- SHA-256:
  `0c609dc991394ba9f1831496e61ce40c74993a581d8a20541ba291d601fcb959`;
- members: one ADF, matching the registered ADF checksum above, together with
  the executable, debug symbols, release notes, README, and icon.

The archive identity is recorded for diagnosis. It is not the canonical
fixture identity because another archive may contain the same ADF bytes.

## Source provenance

Amiga Test Kit was written by Keir Fraser. The registered image identifies
itself as version 1.21. Its corresponding upstream source is tag
[`testkit-v1.21`](https://github.com/keirf/amiga-stuff/tree/testkit-v1.21/testkit)
at commit `9477599d1611da2326f43532dbe563c2848e308b`.

The upstream source declares the work free and unencumbered software released
into the public domain under the Unlicense. The ADF is therefore a Tier 1
fixture under the test-ROM policy. It remains externally supplied because the
project defaults to referenced fixtures and because the complete gate also
requires a proprietary Kickstart image.

Kickstart 1.3 revision 34.005 is Commodore firmware. Its checksum is registered
only to make the verification input reproducible. This repository does not
contain or redistribute the ROM.

## Reference family

The first registered video reference is the
[A500+A501 OCS PAL vAmiga family](amiga-test-kit-v1.21/a500-a501-ocs-pal/README.md).
Its manifest records the machine configuration, capture producer, viewport,
navigation, settling rule, and identities of the committed PNGs.

Those captures are independent of Emu198x, but they come from one vAmiga
implementation family. They are not evidence of agreement between independent
emulator families or with physical hardware.

## Delivery contract

The verification lane receives the ADF, or a ZIP containing it, through
`EMU198X_AMIGA_TEST_KIT_V121_ADF`. It receives Kickstart through
`EMU198X_AMIGA_KICKSTART_13_ROM`; the wrapper may resolve that file from the
existing `EMU198X_AMIGA_ROM_DIR` convention before setting the direct path.

An explicitly invoked gate treats both files and the registered reference
family as required. A missing file, an archive with an ambiguous ADF member, a
malformed image, or a checksum mismatch is a failure rather than a skipped
test.

## Related documents

- [A500+A501 OCS PAL vAmiga reference](amiga-test-kit-v1.21/a500-a501-ocs-pal/README.md)
- [Amiga Test Kit v1.21 video conformance](../knowledge/processes/amiga-test-kit-video-conformance.md)
- [Amiga Test Kit v1.12 fixture identity](amiga-test-kit-v1.12.md)
- [Accuracy corpora](accuracy-corpora.md)
- [Test ROM bundling policy](../knowledge/decisions/test-rom-policy.md)
