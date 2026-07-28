# Amiga Test Kit v1.12 fixture identity

This record answers which exact external files constitute the registered
Amiga Test Kit v1.12 verification fixture.

## Registered inputs

| Normalised name | Size | SHA-256 | Redistribution |
|---|---:|---|---|
| `amiga-test-kit-v1.12.adf` | 901,120 bytes | `c53d706eadc4eddbc64b3cd2c5b3dd6af75bcfbe5bfb043b69b18ddf2516ddf6` | Public domain |
| `kickstart-1.3-r34.5.rom` | 262,144 bytes | `ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53` | Proprietary; do not redistribute |

The machine-readable form is
[`amiga-test-kit-v1.12.sha256`](amiga-test-kit-v1.12.sha256). The normalised
names describe the files after any delivery archive has been unpacked. An
outer ZIP filename or checksum does not replace the payload checksum.

The registered ADF has also been observed in the following external delivery
archive:

- name: `Amiga Test Kit v1.12 (2020-08-21)(Keirf).zip`;
- size: 43,004 bytes;
- SHA-256:
  `021a74038ddd6e4224bc5b3570e71834c3497b692dd945e4a39ff347d78bdc09`;
- members: one ADF, matching the registered ADF checksum above.

This archive identity is recorded for diagnosis. It is not the canonical
fixture identity because another archive may contain the same ADF bytes.

## Source provenance

Amiga Test Kit was written by Keir Fraser. The registered image identifies
itself as version 1.12. Its corresponding upstream source is tag
[`testkit-v1.12`](https://github.com/keirf/amiga-stuff/tree/testkit-v1.12/testkit)
at commit `2e88cf200fe3fbf069371877da0357f6ef840c9f`.

The upstream source declares the work free and unencumbered software released
into the public domain under the Unlicense. The ADF is therefore a Tier 1
fixture under the test-ROM policy. It remains externally supplied because the
project defaults to referenced fixtures and because the complete gate also
requires a proprietary Kickstart image.

Kickstart 1.3 revision 34.005 is Commodore firmware. Its checksum is registered
only to make the verification input reproducible. This repository does not
contain or redistribute the ROM.

## Delivery contract

The verification lane receives the ADF, or a ZIP containing it, through
`EMU198X_AMIGA_TEST_KIT_ADF`. It receives Kickstart through
`EMU198X_AMIGA_KICKSTART_13_ROM`; the wrapper may resolve that file from the
existing `EMU198X_AMIGA_ROM_DIR` convention before setting the direct path.

An explicitly invoked gate treats both files as required. A missing file, an
archive with an ambiguous ADF member, a malformed image, or a checksum mismatch
is a failure rather than a skipped test.

## Related documents

- [Amiga Test Kit verification](../knowledge/processes/amiga-test-kit-verification.md)
- [Accuracy corpora](accuracy-corpora.md)
- [Test ROM bundling policy](../knowledge/decisions/test-rom-policy.md)
