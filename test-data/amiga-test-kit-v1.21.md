# Amiga Test Kit v1.21 fixture identity

This record answers which exact external files constitute the registered
Amiga Test Kit v1.21 video-conformance fixture.

## Registered inputs

| Normalised name | Profile | Size | SHA-256 | Redistribution |
|---|---|---:|---|---|
| `amiga-test-kit-v1.21.adf` | both | 901,120 bytes | `abe7426c93619a7bb61ce10e3e66a4747fcaf22acd1d1876310033faa700ad28` | Public domain |
| `kickstart-1.3-r34.5.rom` | A500+A501 OCS PAL | 262,144 bytes | `ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53` | Proprietary; do not redistribute |
| `kickstart-3.1-a1200-r40.68.rom` | A1200 AGA PAL | 524,288 bytes | `6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707` | Proprietary; do not redistribute |

The A500 machine-readable form is
[`amiga-test-kit-v1.21.sha256`](amiga-test-kit-v1.21.sha256). The A1200 form is
[`amiga-test-kit-v1.21-a1200-aga-pal.sha256`](amiga-test-kit-v1.21-a1200-aga-pal.sha256).
Each file contains the shared ADF and only the firmware required by that
profile. The normalised names describe the files after any delivery archive
has been unpacked. An outer ZIP filename or checksum does not replace the
payload checksum.

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

Kickstart 1.3 revision 34.005 and A1200 Kickstart 3.1 revision 40.068 are
Commodore firmware. Their checksums are registered only to make each
verification input reproducible. This repository does not contain or
redistribute either ROM.

## Reference families

The registered video references are:

- the [A500+A501 OCS PAL vAmiga family](amiga-test-kit-v1.21/a500-a501-ocs-pal/README.md);
  and
- the [A1200 AGA PAL FS-UAE family](amiga-test-kit-v1.21/a1200-aga-pal/README.md).

Each manifest records its machine configuration, capture producer, viewport,
navigation, settling rule, and committed PNG identities.

Both families are independent of Emu198x. They cover different machine
configurations and therefore do not constitute cross-implementation
consensus. FS-UAE and WinUAE also belong to one UAE implementation family.
Neither reference is physical-hardware evidence.

## Delivery contract

Both verification gates receive the ADF, or a ZIP containing it, through
`EMU198X_AMIGA_TEST_KIT_V121_ADF`. The A500 gate receives Kickstart through
`EMU198X_AMIGA_KICKSTART_13_ROM`. The A1200 gate receives it through
`EMU198X_AMIGA_KICKSTART_31_A1200_ROM`. Each wrapper may resolve its ROM from
the existing `EMU198X_AMIGA_ROM_DIR` convention before setting the direct
path.

An explicitly invoked gate requires the shared ADF, its profile-specific ROM,
and its registered reference family. It does not require the other profile's
firmware. A missing file, an archive with an ambiguous ADF member, a malformed
image, or a checksum mismatch fails the gate; it does not skip the test.

## Related documents

- [A500+A501 OCS PAL vAmiga reference](amiga-test-kit-v1.21/a500-a501-ocs-pal/README.md)
- [A1200 AGA PAL FS-UAE reference](amiga-test-kit-v1.21/a1200-aga-pal/README.md)
- [FS-UAE A1200 capture adapter](../tools/fs-uae-test-kit-video-capture/README.md)
- [Amiga Test Kit v1.21 video conformance](../knowledge/processes/amiga-test-kit-video-conformance.md)
- [Amiga Test Kit v1.12 fixture identity](amiga-test-kit-v1.12.md)
- [Accuracy corpora](accuracy-corpora.md)
- [Test ROM bundling policy](../knowledge/decisions/test-rom-policy.md)
