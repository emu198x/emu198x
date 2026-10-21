# A genuine all-skipped model result, with a failed control

**zxsp suppresses host payload for the all-skipped command, but also suppresses
it incorrectly in the SK-off control. It cannot settle our hardware oracle.**
[Emu #1949](https://github.com/emu198x/emu198x/issues/1949) remains open. The
retained Emu native expected status is unchanged and Read Data remains
unqualified. This packet is fresh component research, not a Spectrum boot or
ROM-loader qualification.

## What actually ran

36 fresh processes executed byte-identical `Fdc765.cpp` and `Fdc765.h` from
[zxsp commit 27d2c30d](https://github.com/Megatokio/zxsp/tree/27d2c30d86d3f4877e24dd3dd97d7ebd51bab7c6).
Apple clang 21.0.0 built the component on macOS ARM64 with AddressSanitizer,
UndefinedBehaviorSanitizer and non-recovering sanitizer errors. Every process
returned all seven read-command result bytes, returned MSR to `80`, then
completed Sense Drive Status (`30`) and returned to idle. No sanitizer output
was emitted; source and live authored media were unchanged in every case.

| Case, both Read Data and Read Deleted | Payload bytes | ST0 ST1 ST2 C H R N | Conclusion |
|---|---:|---|---|
| Mismatched mark, SK=1 | 0 | `00 80 00 01 00 01 02` | A genuine all-skipped execution in this model |
| Mismatched mark, SK=0 | 0 | `40 00 40 00 00 01 02` | **Failed control:** payload should be transferred before the control-mark termination |
| Matching mark, SK=0 or 1 | 512, exact nonuniform R1 pattern | `00 80 00 01 00 01 02` | Positive host-transfer control |
| Mismatched mark, SK=1, bad data CRC | 0 | `40 20 20 00 00 01 02` | The model consumes/checks the skipped field CRC |

The main matrix is 32 processes: both opcodes × both SK settings × matching or
mismatching R1 mark × single R1 or R1/R2 tracks × two repeats. Four additional
fresh processes corrupt the first skipped field's data CRC (both opcodes,
two repeats). Eight main-matrix SK-off payload controls fail. Identical cases
repeat the same bytes and statuses; there is no zero-assertion qualification.

Exact command: `66 00 00 00 01 02 01 2A FF` for SK-on Read Data; `6C` for Read
Deleted, and `46`/`4C` for SK-off. R1 has 512 bytes, valid ID CRC and the selected
mark; optional R2 has the opposite mark beyond EOT=1. Specify is `03 DF 03`,
non-DMA with no terminal count, physical/logical track/head 0/0, logical drive 0.
The previous study's raw-track builder is reused byte-for-byte; its Fuse
container header and clock maps are omitted for zxsp's decoded-byte interface.
The explicit bad-CRC variant flips only the first data CRC's low byte.

## What was replaced, and why that matters

`host/` substitutes the item/base-class wiring, logger and drive interface. The
authored drive provides a rotating 6250-byte track, 32 microseconds per decoded
byte, fixed READY/track-zero signals, read/write counters and live byte storage.
It contains no command decoding, SK logic, CRC calculation or result-status
policy. The unchanged controller implements those. The harness supplies bytes
only through public register methods and classifies reads using the returned
MSR execution bit. Timing is advanced in one-microsecond host increments.

This is not the zxsp GUI, its real drive/mechanics model, a distributed zxsp
release, a CPU/ROM execution or a hardware measurement. No firmware is required.
The chip marking/revision represented by this implementation is not established.
Each command is bounded to two emulated seconds, the whole process to eight
emulated seconds and 15 host seconds; compilation has a 60-second timeout.

The source explains the SK-off result: `Fdc765.cpp:846–848` sets transfer size
to zero for a mismatched mark before checking `skip_bit` at `:877`. The latter
check affects termination but cannot restore the discarded transfer. EOT
handling at `:531–542` changes C/R and sets ST1; it includes an explicit +3DOS
compatibility comment about ST0. That is model policy, not independent evidence
for silicon status. No upstream submission or patch is made from this study.

[NEC's December 1978 datasheet](https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf),
printed pp12–13, distinguishes transferring a mismatched field with SK clear
from skipping it with SK set. It does not explicitly establish this all-skipped
no-TC final status; nor does this model's failed control resolve it.

## Inspect and reproduce

Read the retained observations without a compiler or reference checkout:

```sh
python3 -B docs/plans/2026-10-21-read-all-skipped-zxsp/audit.py
```

Audit exit zero means the captured research packet is internally consistent.
Its output still says `qualified: false`, `status: blocked_dependency`, and
`failed_skip_off_controls: 8`. It reparses the raw register observations,
checks exact payload/results/framing/recovery, reconstructs media from the
prior recipe, verifies all retained hashes and rejects eight altered-evidence
controls after refreshing artifact hashes. `verification.json` retains the
result. `missing-prerequisite.json` records an absent-source run rejected with
exit 2 before creating an output directory.

Run a new bounded capture with a clean local checkout of the pinned source:

```sh
python3 -B docs/plans/2026-10-21-read-all-skipped-zxsp/capture.py \
  --zxsp /path/to/zxsp \
  --output /tmp/zxsp-all-skipped-new
```

The output path must not exist. The capture refuses a different source commit
or modified controller source. `runs.tar.gz` contains actual raw stdout,
stderr, authored input/live media, compiler logs and a manifest identifying
commands, relevant environment, host, source hashes, recipe hashes, compiler
and executable identities. No reference executable, third-party source or ROM
is bundled. Temporary compiler products are removed after capture.

## What this changes

We now have an independent model that actually performs a zero-payload skip,
plus a reproducible reason not to use its final status as the qualification
answer. Fuse and MAME's earlier receipts never reached that zero-payload state;
this adds a distinct observation rather than another vote for their result.

The DTL=0 question remains separate in
[Emu #1947](https://github.com/emu198x/emu198x/issues/1947). The recovered
first-hand hardware report and its different physical-field conditions are
recorded in [umbrella #253](https://github.com/stevehill1981/198x/pull/253).
The additional published-source search did not recover a directly applicable
raw capture in this task. This is a bounded search result, not a claim that
such a capture does not exist.

Next useful evidence is an attributable no-TC capture with genuine suppression,
matching and SK-off controls, exact status and part identity, or another
independent implementation which passes those controls. More agreement from
models that fail the controls cannot settle it.
