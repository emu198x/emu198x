# Arnold all-skipped execution passes payload controls

**The recovered Arnold controller genuinely skips host payload and passes the
SK-off payload control. Its final status is now measured, but is not yet a
part-applicable hardware oracle.** Read Data remains unqualified; no Emu runtime
or native expected-result changes are made by this research.

| Case, both Read Data and Read Deleted | Fresh processes | Host payload | ST0 ST1 ST2 C H R N |
|---|---:|---|---|
| Mismatched mark, SK=1, EOT=1 | 16 | 0 bytes | `40 80 40 00 00 01 02` |
| Mismatched mark, SK=0, EOT=1 | 16 | Exact 512-byte R1 | `00 00 40 00 00 01 02` |
| Matching mark, either SK, EOT=1 | 32 | Exact 512-byte R1 | `40 80 00 00 00 01 02` |
| Skip R1, matching R2, EOT=2 | 8 | Exact 512-byte R2 | `40 80 40 00 00 02 02` |
| Matching R1, CRC-error metadata | 8 | Exact 512-byte R1 | `40 20 20 00 00 01 02` |
| Skipped R1, CRC-error metadata | 8 | 0 bytes | `40 80 40 00 00 01 02` |

All 88 processes return seven result bytes, drain to MSR `80`, complete a
subsequent Sense Drive Status (`30`) and return to idle. Input and live EDSKs
are unchanged. Every case repeats in a new process and is tested at both 1us
and 32us host polling intervals; payload and result bytes agree across all
repeats and intervals. These intervals do not establish silicon timing accuracy.

## Exact execution and identities

The source is Kevin Thacker's original author-distributed Arnold archive,
[Internet Archive capture 2017-06-08](https://web.archive.org/web/20170608014028id_/http://cpctech.cpc-live.com/arnoldsrc.zip),
SHA-256 `bf124873d8e4a71cb0cb9ac92d846e4b0540c531c1581cc64cdb4e63145d58bd`.
The capture date is not asserted to be its publication date. Source commit is
unknown and recorded as null; archive/member hashes identify exactly what ran.

Unmodified members: `src/cpc/fdc.c`, `fdi.c`, `fdd.c`, their three headers,
`device.h` and `diskimage/diskimg.h`. Apple clang 21.0.0 built these on macOS
ARM64 using GNU C99, AddressSanitizer, UndefinedBehaviorSanitizer and
non-recovering sanitizer errors. The actual compiler executable, SDK settings,
compiled executable and recipes are hashed in the retained manifest. Original
compiler warnings remain in `compile.stderr`; no warnings are suppressed and
no original source is patched. Runtime stderr is empty in all 88 processes.

`host/` substitutes platform includes and the clock getter. `probe.c` supplies
only the disk-image API: bounded access to authored EDSK sector descriptors,
status metadata and 512-byte payloads. Arnold's original FDI and FDD decide
selection, rotational sector-ID sequencing, index indication, motor/READY and
track-zero state. Physical track0 is selected as initial drive setup; controller
PCN is established through its normal power/reset methods. Any write, format
or erase callback fails immediately. No SK logic or result-status calculation
lives in the disk adapter.

The original `2026-10-21-read-all-skipped/capture.py` EDSK recipe is reused.
Sector i has the nonuniform seed32+i pattern; R1's selected data mark is the
opposite of R2's. EOT=1 single/two-sector contrasts prove R2 is not accidentally
transferred. EOT=2 contrasts prove the controller can skip R1 then transfer R2.
CRC controls change only EDSK status metadata (`ST1.DE`, `ST2.DD`). **They are not
raw CRC-bit corruptions or an independent CRC calculation by this adapter.**

All commands use physical/logical track/head0, drive0, MFM, N=2, GPL=`2A`,
DTL=`FF`, Specify `03 DF 03`, and no terminal count. The base SK-on read command
is `66 00 00 00 01 02 01 2A FF`; other opcodes are `46`, `4C`, `6C`. Every
command has a two-second emulated limit, the process an eight-second emulated
limit and a 15-second host timeout. Compilation is limited to 60 host seconds.

This is a source-built controller/drive component experiment. It is not the
Arnold GUI, a distributed emulator release, a CPC or +3 CPU/ROM boot, or fresh
hardware execution. Mechanical timing and particular silicon revisions are
not attested. No ROM, third-party executable or original source is redistributed.
The local original ZIP remains a required, hash-pinned input to a fresh build.

## What is clarified, and what is still open

The earlier Fuse/MAME runs transferred R1 when SK was set. zxsp's separate
packet reached zero-payload skipping but failed its SK-off payload controls.
**Arnold now supplies the missing control-passing, zero-payload model
observation**, with both read opcodes, exact status, recovery and immutable
media measured. This addresses the independent-execution part of
[Emu #1949](https://github.com/emu198x/emu198x/issues/1949).

It does not settle the target hardware's complete status. Arnold returns
`40 80 40 00 00 01 02`; zxsp returns `00 80 00 01 00 01 02`; Emu's retained
native expectation is `40 04 00 00 00 01 02`. In particular, IC, EN versus ND,
CM and C advancement must not be treated as interchangeable observations.

Original Arnold `fdc.c:2338–2382` detects mismatched marks and chooses whether
to skip using command SK. Its SK-off path transfers the full field; its SK-on
path calls `FDC_UpdateReadID` and sets End Of Cylinder at EOT (`:2414–2428`).
It sets CM when encountering the mismatched mark before that branch
(`:2367–2372`) and retains CM after a later matching R2. `FDC_SetStatus0`
(`:2977–3014`) treats EN as an abnormal-completion cause. These locations
explain the model observations; they are not proof of the hardware rule.

[NEC's December 1978 datasheet](https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf),
printed pp12–13, supports the skip-versus-transfer payload control. It does
not explicitly provide this all-skipped final status. The Scan-specific CM/STP
paragraph must not be promoted into a no-TC Read Data rule. We therefore retain
#1949's final-status applicability question and keep the qualification row
blocked. The separate N=0/DTL=0 question (#1947) is not exercised by this N=2
matrix and remains open.

## Reproduce

Read-only captured-evidence audit, with no compiler, firmware or archive needed:

```sh
python3 -B docs/plans/2026-10-21-read-all-skipped-arnold/audit.py
```

It verifies raw public-register reads and MSR classification, exact payloads,
result framing/recovery, full case coverage, repeat/poll agreement, source and
recipe identities and unchanged input/live artifacts. It rejects nine altered
records after refreshing artifact hashes. `verification.json` retains that
result; exit zero validates this research packet, while its JSON deliberately
reports `qualified: false` and `status: blocked_dependency`.

Fresh capture, requiring the original ZIP and existing Apple command-line tools:

```sh
python3 -B docs/plans/2026-10-21-read-all-skipped-arnold/capture.py \
  --source-archive /path/to/arnoldsrc.zip \
  --output /tmp/arnold-sk-new
```

The output must not exist. Missing and wrong-source prerequisite runs are
retained in `prerequisite-controls.json`: both reject before output creation.
`runs.tar.gz` retains actual raw stdout/stderr, input/live EDSKs, compiler logs
and exact source, tool, SDK, host, environment, command and artifact identities.
Compiler products are temporary and removed after capture.
