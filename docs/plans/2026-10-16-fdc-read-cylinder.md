# Read commands reject wrong cylinder headers

Lane: Spectrum launch-hardening.

The released read arm reads the physical track correctly but ignores command C.
A sector with the requested R and a different recorded C can therefore supply
payload bytes with a successful result. This slice changes only cylinder
selection and its error status for Read Data and Read Deleted Data.

1. Add public-register regressions in `crates/nec-upd765a/tests/read_cylinder.rs`.
   Exercise mismatching C, mismatching FF (WC and BC), matching nonphysical C,
   matching FF, later matching duplicate R, missing R on a wrong-cylinder track,
   a mismatch after a delivered sector, and recovery. Record the pre-fix output.
2. Correct candidate selection in `crates/nec-upd765a/src/lib.rs` without changing
   H/N matching, DTL, MT, pacing, snapshots, APIs or dependencies. Continue searching
   after a mismatching cylinder; fail with ND/WC (and BC for FF) if no match exists.
3. Run the new tests, controller suite, +3 machine tests, format and targeted
   Clippy. Record actual results, file the owning issue, then commit/push a PR.

Hardware source: [NEC µPD765 data sheet, December 1978](https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf),
Read Data printed pp11–12 and status registers pp17–18. WC relates to ND and
compares medium C with IDR C; BC additionally requires medium C=FF. Failed
CHRN contents and rotational search timing are not established by these tests.

Reference source inspection: Fuse 1.7.0 `peripherals/disk/upd_fdc.c`, `seek_id`
and `start_read_data`; MAME `src/devices/machine/upd765.cpp`, `SCAN_ID`.
Both search onward after mismatches. This is implementation precedent inspected
as source, not a claim that either reference runtime executed this diagnostic.

Verification records go in `docs/plans/2026-10-16-fdc-read-cylinder/`.

## Observed results

Owning bug: [emu198x/emu198x#1806](https://github.com/emu198x/emu198x/issues/1806).
Base source: `bb12ccc31b847e02d414135947d997f62a327782` (v0.35.3).

- `cargo test -p nec-upd765a --test read_cylinder` before the fix: exit 101,
  seven failures and one passing matching-C control (`before.txt`). Both
  opcodes returned forbidden payload, including recorded FF. Later matching R
  returned the first wrong-C payload; missing R omitted WC; a multi-sector
  read returned the second wrong-C payload.
- After the fix, `cargo test -p nec-upd765a`: 53 library, nine cylinder,
  five Read ID and four seek tests passed (`after.txt`). The ninth test is
  an additional missing-R/matching-C negative control added after reproduction.
- `cargo test -p machine-sinclair-zx-spectrum-plus3`: six passed; its one
  fixture-dependent boot test was initially ignored (`machine.txt`).
- `cargo test -p machine-sinclair-zx-spectrum-plus3 --test boot_test -- --ignored`:
  the explicit local-firmware boot test passed (`boot.txt`), so the ignored
  default invocation is not counted as boot evidence.
- `cargo clippy -p nec-upd765a --all-targets -- -D warnings` and
  `cargo fmt --all -- --check` passed. Rust toolchain: pinned 1.99.0.

These are fresh source-build controller/machine checks, not released-binary
qualification or a source-to-disk-to-ROM-loading run. The boot assertion checks
menu screen content, not this controller error path. No proprietary firmware
is included in these records.

The sector model still does not reproduce rotational search, H/N matching,
DTL, MT or byte pacing. Mixed-ID tracks with a preceding FF mismatch followed
by a successful match have a sticky-BC disagreement between the inspected
MAME and Fuse sources; this slice does not claim to settle that case. Failed
result CHRN remains unqualified. No archive code was ported: the three archive
locations named in rule 25 are absent locally; the current sector-image read
arm was corrected directly.
