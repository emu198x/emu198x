# Keep result ID H separate from physical head selection

[Issue #1845](https://github.com/emu198x/emu198x/issues/1845) corrects Read Data
and Read Deleted Data results in the represented-image interface. The full ID
search accepts an explicitly requested H1 record on physical side 0, but
`read_result` replaced result H with physical HD. It now returns command H for
those two operations. ST0.HD, physical side selection, result C/R/N, Read Track,
MT, DTL, search-status conventions, timing and the snapshot layout are unchanged.

This is bounded reference consistency, not unusual-hardware qualification. The
held NEC December 1978 sheet, printed p9, defines H from the ID field and HD as
head selection, but explicitly says H=HD in command words. The mismatched
command fixtures do not establish a valid physical configuration. Table 2 on
p13 leaves non-MT H unchanged for processor/TC termination; it does not prove
missing-ID or overrun behavior. Fresh stock Fuse and the inspected MAME result
path provide the separate observations for those represented cases. Ordinary
H=HD commands retain their previous behavior.

## What ran

Three public-register tests fail against parent source
`9d71a1b9344f47266028ff693bca2bc2fbea60a7`: complete, interrupted and later
missing-ID results substitute H0 for requested H1. The red tests stop at their
first failure, so that run does not claim every later branch was reached.
The exact test source, red executable identity and exit 101 are retained. An
initial test-host compile error assumed DiskImage implemented PartialEq; the
host now compares metadata and payload signatures. That log is retained too.

On corrected source `4f03ce6a141079869e3dce04b1cab2525db6ffb7`, all 130 controller
tests pass, including the three new tests' 96 bounded read commands. They cover
both data marks; physical sides 0/1 and requested H0/1; clean/CRC completion;
missed requests after 0/1/511 bytes; initial and later missing IDs; recovery;
all seven result bytes; ST0.HD; and unchanged source metadata/payload. Existing
Read Track assertions remain intact. Controller Clippy and rustfmt pass.

The focused native-state regression passes at ten checkpoints: pending and
interior reads, RQM low/high, and before/within result drain. Original and
restored runs return H1 while ST0.HD remains zero, reach identical serialized
states and preserve cached source and live exported media. The existing command
buffer already stores H, so no state field or version change is needed.

Fresh unmodified Fuse source-controller execution ran 36 bounded cases, eighteen
identical repeat pairs. Requested/recorded H1 on physical side 0 remains H1 in
clean completion, overrun, initial/later missing ID and CRC-error results.
Both data marks are exercised; the raw ID/data CRCs and payload are audited
independently. This reuses the earlier raw-track host adapter and compiles
unmodified controller, drive and CRC source with ASan/UBSan. All stderr logs
are empty. Each process has a 15-second host timeout and two-second emulated
bound. It is not the full Fuse application, ROM loading or physical timing.

Reference differences remain visible: Fuse missing-ID ST1 is 05 (MA|ND), while
the existing Emu path is 04 (ND). Fuse uses its coarse timeout, not Emu's byte
clock. A request after 511 consumed bytes is the final byte of a sector; the
reported A/B final-byte exception remains unqualified even with two sectors.
Those cases pin source-model observations, not silicon timing. Interior
interruptions provide independent result-H observations without relying on
that boundary. Read Track's result-H convention remains separate work.

The parent validated clean source 4f03ce6a: 338 runtime/+3 tests pass and 63 are
ignored without credit. With the controller suite that is 468 passing tests;
the focused snapshot test is included, not added again. The normal locked
headless release build passes. Two fresh cold boots select the normal +3 ROM
Loader, load the captured diagnostic DSK through the FDC, and execute the
program. Both record 519 native checks, eight firmware checks, three loading
checks and seven execution checks. Completion changes from 0000 to 5A5B;
CPU writes at 8030/8034 and the expected screen RAM show execution. Sampled PC
is 8036. The wrong expected signature 5A5C fails execution while loading passes:
wrapper exits are 0/0/1 although every runtime process exits 0.

This normal-ROM regression names PAL +3/40077/128 KiB, English v4.0 firmware,
and a standard 180 KiB drive-A disk. It protects ordinary H=HD loading; it is
not physical proof for the unusual ID fixture. Runtime execution is fresh;
assembly/mastering use captured fixture replay. All records retain
qualified=false. These are source builds with suite manifest 0.38.0, not a
claim about a distributed 0.38.0 binary. Exact source, executable, tool,
firmware, fixture, recipe, command and relevant environment identities are
retained. The safe archives were compared byte-for-byte against originals;
no proprietary firmware, host executable or native machine state is committed.

## Reproduce

```sh
cargo test --locked -p nec-upd765a --test read_result_head
cargo test --locked -p runtime-sinclair-zx-spectrum --lib requested_id_head_survives
python3.14 docs/plans/2026-10-19-read-result-head/verify.py
python3.14 -m unittest discover -s docs/plans/2026-10-19-read-result-head -v
```

The final two commands inspect captured evidence; they do not execute emulators.
Twelve auditor tests include rejection of zero observations/assertions, wrong
result H, patched reference identity, wrong source, erased red failure, false
qualification/execution pass and missing execution writes. `reference/run.py`
can capture fresh stock-Fuse observations with the existing local source path
and Clang; its `reference/runs` directory must not exist. No firmware is needed
for that source-controller probe. The parent ROM wrapper and exact command
paths are retained in the runtime archive; it requires legitimate local ROMs.
