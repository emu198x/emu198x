# Interrupted read investigation

The v0.37.0 source controller prepares all sectors and the terminal result before
the host consumes the first byte. Four public-register reproductions fail:

| Reproduction | Expected | Observed |
|---|---|---|
| Interrupt first sector | R1 | R2 |
| Interrupt second of three sectors | R2 | R3 |
| Read Track completes a CRC-bad field, then overruns | Completed DD retained | DD erased |
| Abandon before visiting a marginal second sector, then read it | First read verbatim | Reread mutation already applied |

`evidence/red.log` retains the actual failures (exit 101, zero passed, four
failed), and `evidence/result.json` binds the source and artifact identities.
The proposed Read Track test also retains DE, matching MAME and the existing
completed-read model. That bit is a recorded reference disagreement, not an
established original-hardware assertion.

Spectrum snapshot v5→v6 was explicitly approved on 2026-10-18. The controller
now buffers only one selected sector and serializes current field progress;
v5 is rejected before payload decoding. CPC does not embed this controller
and needs no version change. See [plan.md](plan.md).

The active regression corrects one initial expectation: Read Track reaching
sector R2 while comparing against command R1 also retains ND. The original red
source/log remain unchanged as historical evidence; the active test requires
ND as well as completed DE/DD. An additional test preserves the existing
SK-skipped EOT after-data result without claiming its hardware conformance.

The failing test source is retained under `evidence/`, outside Cargo's active
test directory. To reproduce against source `c5586b620b3eeff140e1755d483582a039c9a40d`
in an isolated checkout, copy `evidence/read_progress.rs` to
`crates/nec-upd765a/tests/read_progress.rs`, then run:

```sh
cargo test --locked -p nec-upd765a --test read_progress
```

This is an expected red reproduction, not a passing qualification check. The
test host uses public registers and bounded ticks; no firmware is involved.

## Independent reference

`evidence/stock-fuse.tar.gz` contains 178 safe regular files: authored probe and
host adapter, source/compiler/executable identities, 52 fresh stock-Fuse
observations, logs, raw fixtures, an auditor and its eight negative tests. It
contains no firmware, executable or native state. SHA-256:
`f022091aa677108e7edd34d3023ce9b13efc5e5ce8c583fc907cb5746827e69d`.

Extract into a new directory to inspect offline:

```sh
mkdir /tmp/198x-current-read-reference-inspection
tar -xzf docs/plans/2026-10-18-current-read-sector/evidence/stock-fuse.tar.gz \
  -C /tmp/198x-current-read-reference-inspection
cd /tmp/198x-current-read-reference-inspection
python3.14 verify.py
python3.14 -m unittest -v test_verify.py
```

All 26 repeat pairs agree. Read Data and Read Deleted Data report R1 before
512 bytes are consumed and R2 afterwards. Read Track preserves completed
ST2.DD without exposing future CRC errors. Fuse clears ST1.DE at the next ID
and advances Read Track R; MAME source retains DE/DD and command CHRN. Those
disagreements remain explicit. MAME was inspected, not run in this capture.

NEC December 1978 printed pp12/14/17 describe read errors and continuation but
do not settle clearing DE at the next ID. Primary reference identity and exact
source paths/hashes are in the archive's `provenance.json`.

These are source-controller observations, not ROM loading, physical timing
conformance or full Fuse application qualification. Every record retains
`qualified=false`. The per-sector fix, native state roundtrips and two fresh
ROM-loading regressions with a wrong-result control remain to be done. Exact
service windows remain separate under [#1831](https://github.com/emu198x/emu198x/issues/1831).
No Fuse upstream report has been submitted.
