# DTL zero: model result known, silicon encoding unresolved

**Both executed references return 128 bytes for Read Data / Read Deleted Data N=0, DTL=0. This does not establish the encoding on identified NEC hardware.** Research issue [#1947](https://github.com/emu198x/emu198x/issues/1947) remains open, linked to umbrella [#231](https://github.com/stevehill1981/198x/issues/231) and [#1831](https://github.com/emu198x/emu198x/issues/1831). No production code changed.

| DTL | Unmodified Fuse: host bytes | Installed MAME: host bytes |
|---:|---:|---:|
| 0 | 128 | 128 |
| 1 | 1 | 128 |
| 127 | 127 | 128 |
| 128 | 128 | 128 |
| 255 | 128 | 128 |

Each cell was observed for both opcodes, twice. MAME's nonzero controls demonstrate that it ignores DTL generally; its zero agreement is not an independent zero-specific interpretation. Fuse sets its requested length to zero, then increments the offset before testing equality, so its implementation reaches the full 128-byte field boundary. That is an explanation of the executed model, not a claim about silicon's counter encoding.

## What actually ran

There were **48 fresh processes: 24 Fuse and 24 MAME**. The 12 cases per engine, each repeated twice, cover both Data/Deleted opcodes at DTL 0/1/127/128/255 and DTL0 data-CRC errors. Every payload matched the exact authored fixture prefix, every seven-byte result was drained, and a following Sense Drive Status command returned to idle. Source media hashes and Fuse's live raw-track comparison stayed unchanged.

Clean result, hexadecimal: `40 80 00 00 00 01 00`. Corrupted data CRC at DTL0 still transferred 128 bytes, then returned `40 20 20 00 00 01 00`. Eight fresh CRC-error observations demonstrate that those executed engines read/check the full field at DTL0. These are deliberate fault cases, not qualification passes.

- Fuse upstream commit: `0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2`, unmodified controller/drive/CRC files, authored standalone event/public-register host reused from earlier studies. Source-built executable SHA256: `3e8bc96f48b7d9a23b49780c4cad13e3133449b92b1779b811aabae1c03ddb71`.
- Compiler: Apple clang 21.0.0, address/undefined-behavior sanitizers enabled. Exact arguments, compiler binary identity and source hashes are retained.
- MAME 0.289 installed executable SHA256: `6d40df87963c58cdd3857d203e186af854fea49ae03b2ab555e30ef1029636d9`. Its source commit is **unknown**. Inspected vendored MAME source is recorded separately and is not a producer attestation.
- MAME uses legitimate local English +3 ROMs, identified by hashes only. Its existing direct-port/idle-loop research probe uses an authored EDSK and host PC/register control. **This is not ROM-loader qualification.** No firmware or native states are retained.
- Fuse request/transfer phases have two-second emulated bounds and a 15-second host timeout. MAME has a ten-second total emulated bound and a 45-second host timeout. The compiler is bounded to 60 seconds. Exact commands and relevant environment are in the archive.

## What the primary sources establish

The original [NEC December 1978 datasheet](https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf), PDF page 12, explains that N=0 uses DTL to limit host delivery while the controller internally reads/checks the complete field. It does not explicitly identify zero's encoding or legality. The [NEC A/B datasheet](https://hxc2001.com/download/datasheet/floppy/thirdparty/FDC/NEC/UPD765_Datasheet_OCRed.pdf), PDF page 11, defines the parameter without resolving zero. The recorded application note's parameter definition, PDF page 32, adds no zero-specific rule.

The December datasheet and application-note PDFs were recovered from the existing library's original URLs and required to match their stored SHA256 identities exactly. Their text extracts were restored in the primary `reference/by-topic/fdc-765/` library. `primary-identities.json` records identities; the manufacturer PDFs/full text are not in this Git packet.

Part and density matter. An old NEC text reproduced in the APC reference excludes MFM 128-byte/N00 operations. This is not transferred wholesale to the +3's A/B part or every clone/revision. No identified-hardware experiment was available here. Zero could not responsibly be declared zero bytes, 128, 256 or reserved merely from the inspected prose and emulator agreement.

The smallest next evidence is an applicable manufacturer erratum or a bounded host-transfer observation on an identified controller part/revision, with valid N0 media/density and DTL0 plus nonzero controls. Keep the required silicon item unresolved until that evidence arrives; do not silently remove it from the contract.

## Reproduce and audit

Read-only replay of the retained receipts, no ROMs/tools needed:

```sh
python3 docs/plans/2026-10-21-read-data-dtl-zero/audit.py
```

Fresh reference execution, requiring the existing reference checkout, clang, MAME and legitimate local +3 ROMs:

```sh
python3 docs/plans/2026-10-21-read-data-dtl-zero/capture.py
```

Capture refuses an existing `runs/` output directory. It creates only local research outputs and a temporary private firmware directory, which is removed on exit. The readonly auditor enforces unique/exact cases, vectors/results, bounded receipts, source/artifact hashes and honest identity/qualification classes. Ten semantic negative controls reject false qualification, missing/duplicate cases, zero/256 replacement payloads, CRC failures presented clean, invented MAME source identity, changed DTL command, stale executable description and zero processes.

`verification.json` is a pass for **auditing these captured reference-model observations** with `qualified=false` and `silicon_semantics=unresolved`. It is not a machine, operation, release or hardware qualification verdict. Existing Read Data DTL fixes and the required native/guest contract remain separate work.
