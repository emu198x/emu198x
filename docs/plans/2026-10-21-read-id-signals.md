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
