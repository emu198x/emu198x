# Honour Read ID input and completion signals

Lane: Spectrum launch-hardening, FDC operation qualification.

Scope: reuse the existing READY input and interrupt field for Read ID. No new
API, serialized fields, rotational clock or search timing.

1. Add register-level regressions in `crates/nec-upd765a/tests/read_id_signals.rs`
   for READY-low with mounted data, READY-high without readable IDs, physical
   aliases, result IRQ assertion and first-result acknowledgement. Run before
   changing code; retain failures.
2. Compare manufacturer Read ID/ST0/ST1/INT descriptions with MAME `read_id_start`,
   `command_end`, `fifo_r`, and Fuse `start_read_id`/`upd_fdc_read_data`.
3. Correct only proven signal defects in `crates/nec-upd765a/src/lib.rs`; update
   obsolete no-READY test claims. Keep CRC MA disagreement explicit.
4. Run all controller tests and Clippy; demonstrate guards fail when removed.
   Do not commit until parent review.

Evidence and boundaries:

- Existing source returned `[0,0,0]` for mounted A with READY low and did not
  assert Read ID completion IRQ. New regressions reproduced both failures.
- NEC A/B manufacturer status table (printed5-12, PDF p10) defines NR for a
  read issued not-ready. MAME `read_id_start` checks READY before searching;
  `command_end(fi,true)` asserts IRQ, and `fifo_r` clears it on data read.
  https://hxc2001.com/download/datasheet/floppy/thirdparty/FDC/NEC/UPD765_Datasheet_OCRed.pdf
- NEC APC System Reference Manual reproduces the manufacturer processor
  interface text: first result data byte acknowledges completion interrupt.
  https://archive.computerhistory.org/resources/access/text/2021/08/102796836-05-01-acc.pdf
  Fuse `start_read_id` also raises result IRQ, but `upd_fdc_read_data` clears
  at the last result byte; that boundary is not adopted over the primary text.
- CRC status is unchanged: NEC's ND paragraph covers Read ID failure to read
  an ID without error, DE covers ID CRC, and MA is specifically failure to
  detect IDAM before two index pulses. Fuse returns DE|ND; MAME adds MA.
  Finding an IDAM with bad CRC does not establish missing IDAM. This is an
  explicit reference disagreement, not a claim that MAME is broken.
- After the fix all controller tests pass; Clippy `--all-targets -D warnings`
  passes. Removing the first-result acknowledge guard fails the intended test
  with Cargo101 and `first result byte acknowledges IRQ`.
- Native state layout is unchanged. Existing interrupt and READY fields are
  reused. Rotation remains call-driven and no physical latency is qualified.

## Native continuation and direct layout checks

The follow-up test-only increment adds
`crates/runtime-sinclair-zx-spectrum/src/snapshot/read_id_tests.rs`, included
from the existing snapshot test module. Its synthetic matrix covers five
conditions (clean ID, ID CRC, data CRC, missing ID, READY low), selectors
0/2 with both heads, and ten boundaries (idle, opcode-only, zero through seven
result bytes read): 200 cases. Each checks byte-identical native round trip,
phase, IRQ, remaining results, subsequent ID/cursor, source/live immutability,
and uninterrupted/restored final machine equality. Error CHRN filler bytes are
asserted only as preservation of current internal state, not a hardware promise.
Native snapshot version remains9. Optional artifact output uses the existing
`READ_TRACK_NATIVE_ARTIFACTS` sink and emits only JSON plus authored synthetic
EDSK/model fixtures. No firmware or native state is emitted.

Run the matrix:

```
cargo test -p runtime-sinclair-zx-spectrum --lib read_id_native_boundaries
```

`READ_ID_NATIVE_CONTROL=idle-checkpoint` substitutes an idle native state for
an opcode-only checkpoint; Cargo exits101 at `Read ID checkpoint MSR`.
`READ_ID_NATIVE_CONTROL=wrong-result` changes expected ST0; Cargo exits101 at
`Read ID expected result`. These are driver failures, never successful-process
negative-test claims. Two fresh positive matrix processes produce identical
200-case JSON. Direct register tests additionally cover duplicate IDs with
different CHN, recorded nonnumeric order, independent physical-drive cursors,
head selection and unchanged metadata/payloads. They make no head-phase timing
claim. Missing recorded-side tests explicitly establish physical TS=true;
this does not qualify a single-sided mechanism's head1/READY interaction.

Primary bytes were downloaded and actually extracted with `pdftotext -layout`:

- A/B datasheet335,952bytes, SHA256
  `e8a37f1242d4e26b400a524519b0b2efea8a68b83b4770e7a081bb82f6edfebc`.
  PDFpage10, printed5-12: the two-column ST1 table places Read ID ID-field error
  under ND; MA describes missing IDAM/two-index or missing data address mark.
- NEC APC manual43,508,212bytes, SHA256
  `be0438fc83ac10c85acc93658871b7a2d42aa252f28e34fde98fb33b4e34e0cf`.
  PDFpage187, printedA3-12: processor interface states first result data byte
  resets the completion interrupt. These temporary downloads are not vendored.
