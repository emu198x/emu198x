# Format protection and interruption evidence

**328 native checkpoints pass in each of two fresh source-build runs.**
The packet auditor passes; complete Format Track qualification remains false.
No production behaviour, dependency, API or snapshot schema changed.

| Cases per run | What ran |
| --- | --- |
|72|Successful Format continuation before/after each ID byte and final fill/result boundary|
|192|Write protection, missed request service and reset before/after each partial ID boundary|
|64|Host-protected and independent-WP rejection at every pending/drained result position|

Each case uses authored synthetic EDSK and blank ROMs. FM/MFM and ND0/ND1 are
component modes, not extra qualified +3 configurations or a DMA handshake claim.
Direct continuation and native restore agree. Exact payloads, supplied IDs,
command-sized filler, metadata replacement, committed-sector retention and the
untouched tail are asserted. Cached source bytes remain unchanged. Every
terminal live image exports, reparses and reopens in a fresh blank runtime.
Active export refusal leaves the checkpoint unchanged. Nested checks cover
all eight result positions; they are not added to the 328 top-level count.

The wrong expected success R fails at `Format success result`; replacing an
active checkpoint with an idle one fails at `Format checkpoint fixed point`.
Both exit 101. The restored source then passes both positive runs. The owning
chip/runtime suites report 563 passed, 0 failed, 62 ignored across 64 summaries;
ignored cases and empty doctest summaries are not qualification evidence.
Workspace fmt, explicit included-file rustfmt, all-target Clippy and Python
Ruff checks pass. The auditor's nine tests include eight false-packet controls;
changed observation files are deliberately rehashed before rejection.

## Reproduce

From the Emu source revision containing this test:

```sh
cargo test -p runtime-sinclair-zx-spectrum --lib format_native_protection_interruption -- --nocapture
python3 -B docs/plans/2026-10-21-format-interruptions-evidence/verify.py
python3 -B -m unittest discover -s docs/plans/2026-10-21-format-interruptions-evidence -v
```

The cargo command executes fresh component/native checks. The Python commands
only audit the retained packet. Set the existing `READ_TRACK_NATIVE_ARTIFACTS`
environment variable to a new directory to capture `format-native.json`.
Every controller wait is bounded to one million peripheral ticks. Captured
invocations have 300-second host limits; the two positives took 94.222 and
100.587 seconds. No firmware or full native state is published.

## Identities and limits

`verification.json` retains the base revision, final source/recipe/lock hashes,
actual Rust/Cargo executable identities, test executable hash, commands,
relevant environment, durations, artifact hashes and auditor checks.
Rust/Cargo 1.99.0 ran on macOS ARM64. This is source-build evidence; compiled
dependency-graph attestation remains unknown. Raw results and exact case IDs
are in `run-1/format-native.json` and `run-2/format-native.json`.

NEC December1978 printed14 defines command N, filler D, supplied CHRN and
successful R+1; printed17 defines NW/OR. MAME `format_track_start` checks
initial protection; Fuse `upd_fdc_write_data` commits four-byte ID tuples.
Those references were inspected, not executed in this packet. Existing
digital-model mutation/protection/reset promises are tested; no physical
partial-track, head/index/flux or exact protection-sampling timing is claimed.

Aborted CHRN is observed and compared across restore, without a new silicon
oracle. Active READY sampling/status and applicable Fault remain unresolved.
This packet contains no ROM loading, independent runtime execution or released
binary evidence. The remaining Format operation matrix, ROM diagnostic,
reference comparison and final closure auditor remain under umbrella237.
