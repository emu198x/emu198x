# Raw MAME input resolves the Read Track ID-CRC input gap

All 13 Read Track cases repeat twice through MAME's existing MFI input. Every
payload byte, all seven result bytes, idle status and next-command recovery
agree with the recorded Emu observations from #1851. In particular, ID-only
errors set DE and leave DD clear; data-only errors set both. This satisfies the
bounded input/comparison task [#1860](https://github.com/emu198x/emu198x/issues/1860).
It does **not** close Read Track or qualify a machine: `qualified=false`.

There are **38 fresh installed-MAME observations**: 26 Read Track, six Read ID
and six Read Data. The **52 Emu/Fuse observations are captured reference replay**
from the separately audited earlier packet. Neither engine was rerun here.
The previous DSK-backed MAME observations remain historical and are not credited
as raw ID-error evidence. No production emulator or format interface changes.

## Results

ST0/ST1/ST2 are hexadecimal. Full CHRN, payloads, commands and comparisons are in
[summary.json](summary.json) and [evidence.tar.gz](evidence.tar.gz). Each row has
identical repeats. Physical field IDs are [1], [1,2] and [2,1], C0/H0/N2. Errors
are isolated first/last ID or data CRC mutations. EOT is one or two fields;
command R remains 1, no terminal count is supplied.

| Read Track input | [1] | [1,2] | [2,1] |
| --- | --- | --- | --- |
| Clean | `40 80 00` | `40 84 00` | `40 80 00` |
| ID CRC, first or last where present | `40 A0 00` | `40 A4 00` | `40 A0 00` |
| Data CRC, first or last where present | `40 A0 20` | `40 A4 20` | `40 A0 20` |

All Read Track CHRN results are `00 00 01 02`. Thus MAME agrees with recorded Emu
on DE/DD, ND/EN, abnormal ST0 and all four CHRN bytes in these cases. This is
independent implementation agreement, not proof of revision-specific silicon.
The earlier Fuse differences in DE lifetime, ND, EN/ST0 and result R remain
visible in the summary and are not adjudicated. No upstream report is filed.

The one-sector controls prove that raw ID damage reaches the controller:

| Input | Read ID status / bytes | Read Data status / bytes |
| --- | --- | --- |
| Clean | `00 00 00` / 0 | `40 80 00` / 512 |
| ID-only CRC | `40 25 00` / 0 | `40 20 00` / 0 |
| Data-only CRC | `00 00 00` / 0 | `40 20 20` / 512 |

Read ID exposed a separate Emu bug: it ignored represented ID CRC metadata.
[#1867](https://github.com/emu198x/emu198x/issues/1867), corrected separately in
[PR #1868](https://github.com/emu198x/emu198x/pull/1868), records primary/Fuse support
for DE|ND and abnormal termination. MAME also sets MA; that extra bit remains a
reference disagreement and is not copied into the fix. DSK/EDSK DE+DD cannot
prove additional independent ID damage.

## Input and identities

MAME v0.289 (unknown), installed executable SHA256
`6d40df87963c58cdd3857d203e186af854fea49ae03b2ab555e30ef1029636d9`, runs
`specpls3`, English BIOS, the unchanged internal `3ssdd` drive. `-listmedia`
advertises MFI. HFE is not advertised; no drive replacement was used.
The installed binary's source commit is **unknown**. Separately inspected MAME
source is `9391e1eff80eb04abd58d48218a1d3b9643c1ea8`, specifically
`src/lib/formats/mfi_dsk.{cpp,h}`, `flopimg.h`, `upd765.cpp` and `specpls3.cpp`.
Their hashes are recorded; inspection is not the executed source identity.

The fixture-only [encoder](mfi.py) wraps the already audited 6250-byte raw Fuse
track and missing-clock bitmap into one cylinder/one side of MFI transitions.
MFI uses 200 million angular units per revolution. The fixture has 100,000 MFM
cells; transitions are on the 2,000-unit grid at cell end minus 500. Python
stdlib zlib compresses the delta positions. The independent [decoder](decode.py)
reconstructs all cells, validates normal MFM clock pairs and missing-clock A1
words, and compares all 6250 bytes and marks against the original raw input.
The baseline auditor independently validates every ID/data CRC and exact isolated
mutation. This proves preservation of these authored cells, not physical flux
fidelity, jitter, weak sectors or analogue hardware behaviour.

The campaign records source/adapter hashes, installed executable identity,
firmware hashes, host/Python versions, commands, environment, exit codes, runtime
bounds, media hashes and raw JSON/stdout/stderr. Legitimately available local
English +3 firmware is used, with the same four page hashes as the baseline.
Private copies were removed; no ROMs, snapshots or memory dumps are retained.

Lua installs an idle CPU loop and accesses ports directly. This is a bounded
controller experiment, **not normal ROM loading**. Each command has a two-second
emulated bound, each MAME process ten emulated seconds and a 45-second host
limit. Bounds are experiment controls, not measured controller deadlines.

## Audit and repeat

No firmware or emulators are needed for the offline audit. It requires the
committed earlier `2026-10-19-read-track-drained-crc/` packet and its source-bound
Rust probe, already present alongside this folder in a repository checkout.

```sh
python3.14 docs/plans/2026-10-19-read-track-raw-crc/verify.py
python3.14 -m unittest discover -s docs/plans/2026-10-19-read-track-raw-crc -p test_verify.py -v
```

All 21 auditor checks pass, including rejection of missing/duplicate rows,
missing fixtures/artifacts, zero payload, truncated results, failed recovery or wrong Sense Drive Status result,
timeouts, a wrong expected byte count, false fresh/qualification/source claims,
changed raw input, rehashed off-grid flux, missing-clock marks, trailing zlib streams and oversized
tracks. Semantic controls recompute hashes: a consistent hash list cannot turn
wrong observations into passes. The raw output is compared to the derived row.

Fresh capture requires the installed MAME path and legitimately available local
firmware paths recorded in [run.py](run.py), plus the held baseline packet.
`python3.14 docs/plans/2026-10-19-read-track-raw-crc/run.py` refuses an existing
`runs/` directory. It leaves failure logs rather than counting process exit as a
pass. The archive SHA256 is
`3662226019e03309fd3ef4c5b7cd7cb94e39ac04e93fec3dcd87e52170cad386`.

See [the operation closure path](closure-path.md) for what remains. Basic released
ROM loading, hardware timing, raw gaps/index/rotation, combined CRC damage,
other sizes/densities, controller revisions and other configurations gain no
qualification from this packet.
