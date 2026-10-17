# Paced reads within the existing sector model

Read Data, Read Deleted Data and Read Track stage an initial request after
the existing sector-model pre-data interval plus one completed byte (39 MFM /
19 FM byte slots). This avoids a deadline before the ROM reaches its first
status poll. They consume a byte only while RQM
is high. A read clears RQM; the existing CPU-T-state countdown publishes the
next byte at a fixed boundary. Late host service does not move the boundary.
Missing a request ends execution with abnormal IC and OR, rather than the old
long inactivity timeout and EN. Completed reads retain their existing payload
and EOT/error results. A known mark mismatch is retained on overrun; a future
sector's mark is not reported early.

The serialized fields and envelope layout are unchanged. New-state checkpoints
exercise RQM high/low and 0/1/113 elapsed ticks for all three operations. Older
active-read snapshots can retain their old long countdown until the next
boundary: unchanged layout is not an identical timing contract across versions.

The old tests asserting the long inactivity/EN behaviour are replaced by public
register tests for the corrected handshake, cadence, missed first/later/final
request, mark status, complete payload/result drain and command recovery.
Existing test hosts now tick/poll between reads; their payload/status checks
remain. No assertion is disabled to admit the fix.

## Boundary

This is a byte-slot model, not physical timing conformance. It reuses the
existing write/Scan period: rounded 114 +3 CPU T-state ticks for MFM, 228 for FM
(32/64 microseconds rounded up at 3,546,895 Hz). NEC December 1978 printed p12
specifies 13/27 microsecond service requirements at its documented clock; a
full-byte service slot does not establish those sub-byte limits. Initial
sector-search latency, inter-sector gaps, physical index/rotation, DMA/ND and
exact interrupted CHRN/error accumulation remain unqualified. Prepared buffers
still compute the final result before transfer; the overrun path discards
unconsumed outcomes and retains encountered CM only. No full FDC claim follows.

The vendored Fuse path checks RQM and reports OR on its coarse timeout; it does
not establish byte cadence. Installed MAME 0.289 clears RQM after each serviced
byte and clocks successive requests. Two fresh repetitions for each operation
read all 512 expected bytes and produce `40 80 00 00 00 01 02`. Pausing service
for 1 ms produces OR with `40 90 00 00 00 01 02` and only 495 delivered bytes.
Its shared FIFO and continued delivery differ from our one-byte termination;
its observed initial intervals are 30–31 microseconds, not an exact match to our
rounded 32-microsecond model. These differences remain recorded, not patched
away. `checks_passed` means the stated bounded reference observations passed,
not equivalence or qualification. All records retain `qualified=false`.

## Reproduce

From this repository:

```sh
cargo test --locked -p nec-upd765a --test read_pacing
cargo test --locked -p runtime-sinclair-zx-spectrum --lib paced_read_snapshot
python3.14 -m unittest discover -s docs/plans/2026-10-17-read-pacing -v
python3.14 docs/plans/2026-10-17-read-pacing/reference.py \
  --umbrella /path/to/198x \
  --output /new/output/directory
```

The reference command requires installed MAME and four legitimately available
local +3 firmware pages in `~/.emu198x/roms/amstrad-zx-spectrum-plus3/`. It uses
an injected idle loop and host port accesses: it is **not ROM qualification**.
Each request loop is bounded to two emulated seconds, each process to 45 host
seconds and the machine run to ten emulated seconds. Private firmware copies
are removed automatically; no ROMs are committed. The fixture generator is
reused from the prior command investigation. The probe records executable,
source, firmware and media hashes, argv/environment, logs and observations.

Sources: held NEC December 1978 datasheet printed pp12/17 in umbrella
`reference/by-topic/fdc-765/`; vendored Fuse `upd_fdc_read_data`/timeout event;
vendored MAME `fifo_r`/`fifo_push`/`READ_SECTOR_DATA_BYTE`. Those implementations
provide comparison evidence, not a substitute for original hardware.

## Executed result

Source build `eb26c4b59464d46f111a66d0e060a813bbd81b2a`, Rust 1.99.0,
executable SHA-256
`30548d7940f9066a43abb4e3486bfdfb03a60e5ca6fefd80094dda8e63650b3e`:
two fresh normal-ROM disk loads pass 519 native checks, eight firmware checks,
three loading checks and seven execution checks each. The 54-byte diagnostic
reaches PC 32820, writes new `5A 5B` completion bytes and the expected full
6912-byte screen pattern. The wrong-signature control exits 1 with
`signature mismatch: expected [90, 92], observed [90, 91]`.

This names the existing PAL +3 / 40077 / 128 KiB / English v4.0 / internal drive A /
standard 180 KiB DSK configuration. Assembly/mastering are captured fixture
replay, not fresh execution. These are fresh source-runtime ROM regressions,
not a new distributed release or full-stack qualification. No snapshot restore,
injection or host port writes occur in these ROM runs.

The initial candidate failed before the ROM's first data poll: first-byte RQM
was published synchronously with the final command parameter and expired during
the ROM's return from the command routine. Its failed result and bounded trace
are retained. A new regression failed before the pending-start correction and
passed afterwards. The normal-ROM path then passed twice.

`evidence/runs.tar.gz` retains 101 regular safe artifacts, including all twelve
MAME observations, both ROM passes, the wrong-signature failure, the startup
failure and red/green/build logs. `hashes.json` binds every member. No firmware,
native states or executables are included. The archive's identities record exact
source, lockfile, compiler, executable and command details. Inspect it offline:

```sh
python3.14 docs/plans/2026-10-17-read-pacing/verify.py
python3.14 -m unittest discover -s docs/plans/2026-10-17-read-pacing -v
```

The offline auditor is captured-evidence inspection, not fresh execution. Its
negative tests reject zero reference cases, false qualification, missing data,
a falsely passing negative control, missing execution writes, zero assertions
and changed runtime source identity.
