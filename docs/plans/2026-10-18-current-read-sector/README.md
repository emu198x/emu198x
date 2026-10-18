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
`qualified=false`. Exact
service windows remain separate under [#1831](https://github.com/emu198x/emu198x/issues/1831).
No Fuse upstream report has been submitted.

## Implemented and executed

Source `19395eb81c122b8b0afbbdbabd06b3eed7ee07c6`, Rust 1.99.0, source binary
version 0.37.0, executable SHA-256
`b805bf01955e99abb57b92bd0fe9ebd24b01067ed045d732323ce44eb38dc13b`:
117 controller, six +3 machine and 118 runtime tests pass. The three new snapshot
tests exercise 108 checkpoints. Controller/runtime Clippy and formatting pass;
the normal locked headless release build passes. The snapshot log retains the
initial legacy-layout test failure and subsequent approved rejection correction.
No existing assertion is disabled to admit the new format.

Two fresh cold-ROM DSK loads pass 519 native, eight firmware, three loading and
seven execution assertions each. The diagnostic reaches PC 32820 and writes new
`5A 5B` bytes after initially clearing them, with the expected 6912-byte screen.
An incorrect expected signature exits 1 with
`signature mismatch: expected [90, 92], observed [90, 91]`.

Named configuration:
`spectrum-plus3-pal-40077-en40-rom-841d5454-6d6ccab2-c5590212-8ec5cb9f-drive-a-dsk180-v1`:
PAL +3 / 40077 / 128 KiB / four English v4.0 ROM pages / internal drive A /
standard 180 KiB DSK, protected mount. Exact firmware identities, source and
recipe hashes, commands, environment and artifacts are in each regression record.
These ROM runs use no injection or native-state restore. Assembly/mastering are
captured fixture replay; runtime execution is fresh from a source build, not a
new distributed-release or full-stack qualification. Other models, drives,
firmware, geometries, H/N/MT/DTL behavior and raw/index timing remain unqualified.

`evidence/runs.tar.gz` retains 39 safe regular artifacts, including all three ROM
records, screens, command outputs and build/test logs. `hashes.json` binds every
member. No firmware, native states or executables are included. Inspect offline:

```sh
python3.14 docs/plans/2026-10-18-current-read-sector/verify.py
python3.14 -m unittest discover -s docs/plans/2026-10-18-current-read-sector -v
```

Run the active controller regressions and current-sector native-state cases:

```sh
cargo test --locked -p nec-upd765a --test read_progress
cargo test --locked -p runtime-sinclair-zx-spectrum --lib current_read_snapshot
```
