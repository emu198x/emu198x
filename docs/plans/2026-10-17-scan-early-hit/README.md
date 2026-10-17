# Scan Equal: stop at a matching sector before EOT

**Emu and unmodified Fuse stop after the first matching 512-byte sector.
Stock MAME transfers the second sector too. The controls expose separate Fuse
continuation bugs, so full Scan conformance remains unqualified.**

This is controller-level evidence. Emu runs a source-built register test; Fuse
runs its unmodified controller, drive and CRC code in a standalone source-built
host; MAME runs an installed binary with direct I/O probes and an injected idle
loop. None of these probes qualifies ROM loading, CPU execution or hardware.
The existing public v0.36.0 ROM guest evidence remains separately scoped.

## Fresh observations

Normal sectors R1 and R2 contain 512 bytes of 21 and 22 respectively. Command:
`51 00 00 00 01 02 02 2A 01` — Scan Equal, C0/H0/R1/N2, EOT2, STP1, MFM.
Two fresh repeats per case produce the same observations.

| Host data | Expected transfers | Emu source test | Unmodified Fuse | Experimental Fuse patch | Installed MAME |
| --- | ---: | ---: | ---: | ---: | ---: |
| 21: first hit | 512 | 512 | 512 | 512 | **1024, fail** |
| 22: later hit | 1024 | 1024 | **512, fail** | 1024 | 1024 |
| 23: no hit | 1024 | 1024 | **512, fail** | 1024 | 1024 |

Counts are byte transfers, not a timing measurement. Matching sector 1 should
terminate after that whole sector rather than continuing to EOT. The primary
basis is NEC's December 1978 µPD765 datasheet, printed p15, Scan Commands/Table4;
its held PDF SHA-256 is
`e8b07db283aad56358cdace1bf4ac179f961484d10d6c1700b74d9734eea1117`.
This does not silently equate every detail with the A revision or other vendors.

The first-hit transfer behavior now has direct agreement with independent
unmodified Fuse controller code. MAME's actual first-hit result is
`40 80 04 00 00 02 02`, after 1,024 bytes; its second-sector mismatch replaces
the earlier hit. We retain that as a failure, not a successful reference verdict.

Exact CHRN remains separate. On a first hit Emu returns R1 while Fuse returns
R2; on the later hit the patched Fuse returns R3 while Emu returns R2. This
experiment does not settle physical result CHRN or final EOT status. A matching
count in another column is not a full status/conformance pass.

## Fuse continuation investigation

The separate bug session reproduced two defects in the unmodified source:
Scan advances R by STP before calling `start_read_data`; the shared helper tests
EOT against that value and advances R again. It therefore stops before R2 in
the two-sector case. The not-satisfied latch also survives a sector transition.

The retained [four-line candidate patch](reference/fuse-continuation.patch)
compensates for the helper's increment on continuation and clears SN only when
a real new-sector transfer starts. The first experiment cleared SN too early;
that lost the final no-hit flag on EOT abort and was rejected. The final patch
preserves the first-hit baseline, transfers 1,024 bytes for both controls and
retains final SN for no hit. It preserves the disputed next-R convention.

The parent review checked the patch against both paths; the fresh parent
campaign rebuilt and repeated it independently of the bug session. This is an
explicitly patched source-controller experiment, not an accepted upstream fix,
a distributed Fuse binary result or a complete independent passing oracle.
No canonical or vendored source was edited, and no upstream message was posted.
The smallest upstream follow-up is to submit this reproducer and bounded patch
for review, keeping result CHRN and other Scan semantics separate.

The [upstream check](reference/upstream.json) pins SourceForge HEAD
`43810279d09cd40f46907d7d1d902c4e5ee4648a`. Its controller bytes match the
vendored source exactly. That is current-source inspection; the full current
Fuse application was not built or run.

## Adapter and failure controls

The [host adapter](reference/host.c) supplies allocation/type aliases, a bounded
queue of absolute-T-state events and inert UI/startup wiring. It implements no
Scan comparison, sector selection, drive read or result phase. The build compiles
upstream `upd_fdc.c`, `fdd.c`, `crc.c` and production headers directly. The
[probe](reference/fuse_probe.c) authors a normal MFM raw track with genuine
address/data marks and CRCs, initializes a ready drive, and uses the register API.
This bypasses media-container decoding and drive spin-up, and claims neither.

A fourth case changes exactly raw-fixture byte 655: the first data CRC byte.
Payload bytes still match, but the real Fuse CRC path now reports ST1/ST2
`20 28` instead of `00 08`. Both stock and patched builds detect it in both
repeats. The offline checker independently recomputes all four field CRCs and
checks both complete payloads. Every run preserves its input media and drains
seven result bytes back to idle MSR80. Address/undefined-behavior sanitizers
report no diagnostic in the final builds/runs.

The initial harness setup omitted the raw buffer's `disk.data` backing pointer;
the sanitizer caught it when the real drive selected its track. Correct backing
storage and its three-byte track prefix were supplied before these final runs.
No upstream code was changed to conceal that setup error.

## Identities and verification

Frozen probe/test source: `b2fbb3a898de887054b5126925938e93a2a3a796`, based on
Emu main `828b13f504fef04392db88ed3820cab2ea5bf963`. Exact source files, tool
versions/executables, commands, environment, host limits and media hashes are
in [identities](evidence/identities.json) and the archived campaign record.

| Runtime | Identity | Delivery |
| --- | --- | --- |
| Emu controller test | executable `aa8dc453218875aa2b757d6723264796dd591c7bc077a94119e2a874c8565efb` | Rust source test build |
| Fuse controller/drive | commit `0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2`; controller `845b960eb68cba9b21cba00cdc3c03cf0d4dc491ffd96efecb1d53336ce7f72d` | Unmodified source plus explicit host adapter |
| Patched Fuse controller | controller `77cea9606d3ebe3854faa39ac25ddbdf1290edff9e8366344f5e4cd80eab06e8` | Separate source experiment |
| MAME 0.289 (unknown) | executable `6d40df87963c58cdd3857d203e186af854fea49ae03b2ab555e30ef1029636d9` | Installed binary; source revision unknown |

Apple Clang 21.0.0, Rust 1.99.0 and Python 3.14.3 ran the probes. MAME's four
legitimate English v4.0 ROM hashes are retained; firmware and full native states
are not. Fuse needs no firmware for this controller probe.

All 108 controller tests and targeted all-target Clippy pass. The new register
test measures all three transfer counts, requires actual result phase, checks
SH/SN and verifies unchanged sector data/flags. Zero doc-tests add no coverage.
Ten auditor tests reject wrong counts, missing results/cases, altered sources or
fixtures, zero executed tests, false qualification and patched-as-stock claims.
No emulator product behavior, public API, dependency or snapshot schema changes.

The [safe archive](evidence/runs.tar.gz) contains 89 regular files;
[hashes](evidence/hashes.json) and [verification](evidence/verification.json)
cover 28 fresh case observations and 88 artifact hashes. Archive bytes were
compared to the originals before cleanup. The older 16-case MAME campaign and
public ROM guest records remain unchanged.

## Reproduce

From an Emu checkout containing this change:

```sh
python3.14 docs/plans/2026-10-17-scan-early-hit/reference/run.py \
  --fuse-root /Users/stevehill/Projects/198x/emulators/zx-spectrum/fuse-emulator-fuse \
  --output /NEW/OUTPUT/DIRECTORY
```

This rebuilds the unmodified and explicitly patched Fuse controller probes,
runs MAME with temporary private firmware copies, then runs the Emu source tests.
It is an evidence capture command, not a qualification verdict. The output must
be new. It needs existing Clang, patch, Cargo and MAME; no dependency is installed.
Host subprocess and emulated event limits are explicit and bounded. Disposable
Fuse binaries and temporary ROM copies are removed automatically; remove the
task-owned `cargo-target` beside the output after inspecting retained evidence.

Inspect the archived evidence without firmware or a runtime:

```sh
python3.14 docs/plans/2026-10-17-scan-early-hit/reference/verify.py
python3.14 -m unittest discover \
  -s docs/plans/2026-10-17-scan-early-hit/reference -p 'test_*.py' -v
```

[Emu #1813](https://github.com/emu198x/emu198x/issues/1813) stays open for the
remaining relations, mixed bytes, STP/MT, deleted marks/SK, N=0 and termination/
error/deadline matrices. Hardware/result-CHRN questions remain unresolved. All
fifteen FDC operations stay partial; no configuration or family-wide pass is added.
