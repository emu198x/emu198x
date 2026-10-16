# Report end of cylinder after reads without terminal count

Lane: Spectrum launch-hardening. Owning issue: #1808.

1. Reproduce both 46/4C public-register reads on a physical track-zero
   sector with matching C, N=2 and R=EOT=1. Assert all 512 bytes, each phase,
   all seven result bytes, and recovery. Retain the failing output in
   `docs/plans/2026-10-16-fdc-read-eot/before.txt`.
2. Correct only clean no-TC completion in `crates/nec-upd765a/src/lib.rs`.
   Retain EOT R and stage IC01/EN; leave missing/wrong-cylinder, CRC, CM,
   all-skipped and partial-drain timeout paths unchanged. No new state,
   API, dependencies, MT, DTL or pacing. Add public integration tests in
   `crates/nec-upd765a/tests/read_eot.rs`, updating conflicting clean-read
   result expectations only where this contract requires it.
3. Run the full controller and +3 machine tests, targeted fmt and Clippy.
   Parent session runs independent MAME and normal ROM disk load checks.
   Retain green output, commit, push and open a reviewable PR. Do not merge
   before parent review.

Evidence: NEC uPD765 December 1978 printed p12 explains TC termination;
Table 2 on p13 is explicitly *processor-terminated* result CHRN, not no-TC
EOT. P17 defines EN on access beyond the final sector. Fuse 1.7.0
`start_read_data` sets IC01/EN on clean EOT without TC and preserves R;
MAME `SECTOR_READ` does the same. Parent's two independent MAME runs observe
[40,80,00,C,00,01,02]. Thus the table's C+1/R=1 is not applied to this
no-TC path. No hardware measurement is claimed.

Primary: https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf
Reference source: https://github.com/fuse-emulator/fuse/blob/1.7.0/peripherals/disk/upd_fdc.c
Reference source: https://github.com/mamedev/mame/blob/mame0289/src/devices/machine/upd765.cpp

Archive check: old archive locations are absent; current frozen
Emu198x-Older/Oldest controller sources were inspected for EOT. They use the
older chip model and are not ported under rules 25/26. Existing live buffer
and result staging suffice.

Verification:

- Before: the three EOT tests fail with normal status and EOT+1; the three
  error/skip preservation controls pass. See `before.txt`.
- After: 77 controller tests and six machine tests pass; the local-ROM boot
  test remains ignored in this run (not credited). `after.txt` records this.
- Clippy with `--all-targets -- -D warnings` passes for both crates.
- Existing timeout-rearm test now asserts execution before every read and
  all bytes drained before the complete expected no-TC EOT result. The old
  normal-status expectation could not distinguish this contract correctly.
- Terminal skipped-sector behavior after earlier delivered data is unchanged
  and is not qualified by this bounded correction. All-skipped remains ND.
- Runtime/ROM-loader validation is separate parent-session evidence.
