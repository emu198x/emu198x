# Decision: Spectrum accuracy closure campaign

**Date:** 2026-08-09
**Status:** ACTIVE
**Assessment revision:** `a19de51c`

## The question

What accuracy work must Emu198x complete before the active ZX Spectrum effort
pivots from broad improvement to failure-driven maintenance?

## Current assessment

The strongest supported slice is the 48K and 128K Sinclair machines running
ordinary tape, disk and snapshot software. CPU coverage is the best in the
fleet: every Spectrum-native oracle passes, and `z80test` passes with no
allowlist at all. The 103-entry catalogue across eight variants is the largest
regression foundation any system in the workspace has.

The gap is not a known defect. It is a **measurement gap**. Every declared
Spectrum ULA gate is binary pass/fail, and at `a19de51c` all of them pass. That
instrumentation cannot distinguish "the ULA is correct" from "the ULA is wrong
in ways no declared gate probes". The Amiga and C64 campaigns both turned on a
graded, revision-keyed differential survey that reported *where* residual
disagreement lived; the Spectrum has no equivalent, so it cannot generate its
own next question.

No numerical family score is assigned, for the reason recorded in the
[C64](c64-accuracy-closure-campaign.md) and [Amiga](amiga-accuracy-closure-campaign.md)
campaigns: the available measurements describe different assertion boundaries —
per-instruction final state, program-level CRCs, cycle traces, single T-state
probes, byte-equal framebuffer goldens and catalogue frame/audio hashes.
Combining them into one percentage would imply a weighting the evidence does
not provide.

## Two goals, staged

The campaign's end state is the pivot gate below. Inside it sits a nearer,
differently-shaped milestone, because CRASH! Live in October 2026 does not need
the campaign finished — it needs something specific and true to say.

**October milestone.** Emu198x can make a specific, reproducible, public
accuracy claim about the 48K and 128K Spectrum, backed by a graded survey
against published real-hardware results and verifiable by anyone who runs the
same suite. Today the strongest honest public statement is "every test we have
passes", which is weak precisely because all eight ULA gates are binary. Steps
1, 2, 3 and 5 serve this.

**Campaign end state.** The pivot gate: Spectrum work becomes failure-driven.
Steps 6, 7 and 8 — the RZX harness, 128K/Amstrad-class contention evidence, and
sourcing `ulatest3` / `progforhackers` — are not October-shaped and are not
scoped against that date.

The distinction matters because the two deadlines answer different questions.
The milestone asks "what can we defensibly say in public?"; the gate asks "when
does broad improvement stop?".

## Evidence supporting the assessment

Every figure below is a fresh run at `a19de51c`, not inherited from
[`tests/spectrum.md`](../tests/spectrum.md), which was last refreshed
2026-05-31 with 47 Spectrum-crate commits landing since.

**The baseline is complete and the foundation is restored.** The CPU layer and
all eight variants are measured, and the catalogue runs 103/103 PASS with
103/103 SNAP-PASS as of `ad686cb6`. Restoring it took a media-path repoint, a
controlled re-capture and one real bug fix — and it caught that bug itself, a
`+3` save-state defect that had been invisible for three months. Step 3 is now
unblocked: a change that absorbs an unexplained regression in a stronger lane
will be seen.

### Processor

| Oracle | Rank | Result | Boundary |
| --- | --- | --- | --- |
| `z80test` (raxoft) | 2 | **6/6 exercisers, zero allowlist entries** | Program-level CRCs over documented + undocumented effects, validated on a real 48K Sinclair board |
| FUSE cycle traces | 3 | 1,351/1,356 exact, 5 accepted, **0 unexpected** | Event sequence, final register state, memory effects, final T-state count |
| Tom Harte | 4 | **1,604,000/1,604,000, 0 failed opcodes** | Per-instruction final state + per-cycle bus probe; CPU-generic. Four `ed b2/b3/ba/bb` entries carry documented WZ-only allowances |
| ZEXDOC | 5 | 67/67 checkpoints | Program-level CRC |
| ZEXALL | 5 | 67/67 checkpoints | Program-level CRC |

Ranks are from [`spectrum-test-oracle-priority.md`](spectrum-test-oracle-priority.md).
Rank 1 — real-hardware RZX replay — **has no harness and has never run**. The
decision has carried it as "harness pending" since 2026-05-18;
`format-sinclair-zx-spectrum-rzx` parses and writes RZX but has no replay
consumer and no tests directory.

The Tom Harte figure required fixing the gate first. It resolved no corpus and
still reported `test result: ok`; see § Gate integrity below.

### ULA, contention and floating bus

Eight system-level gates, all passing, all binary:

| Gate | Machine | Assertion class |
| --- | --- | --- |
| Woody `Float48K` | 48K | Single T-state probe — **14337** |
| Woody `Float128k` | 128K | Single T-state probe |
| Ramsoft `floatspy` | 48K | Byte-equal indexed-PNG golden |
| `halt2int` | 48K | Byte-equal golden |
| `halt2int128` | 128K | Byte-equal golden |
| `btime` | 48K | Byte-equal golden |
| `ptime` | 48K | Byte-equal golden |
| Woodmass `Super HALT Invaders` | 128K | Byte-equal golden |

Two are single-number probes; six are whole-frame identity comparisons. Neither
shape yields a graded residual. A byte-equal golden that passes says nothing
about margin; one that fails says only "something moved".

### Determinism and compatibility

- **The catalogue runs green**: 103 entries across eight variants, 103/103
  PASS and 103/103 SNAP-PASS at `ad686cb6`, each gated on snapshot round-trip
  through a fresh-from-firmware runtime at `audio_routing_version` 3 and
  `frame_routing_version` 4. It had not run since **2026-06-05** — see the
  progress log for the three independent breakages that had to be cleared.
- Routing-version constants fail loud on stale hashes (architecture review
  Seam 4), so a timing change cannot silently relabel captured output as
  expected. **This worked exactly as designed — and nine weeks passed before
  anyone ran the gate that would surface it.** A loud gate nobody runs is a
  silent gate.
- The catalogue is the **only** oracle for audio and for media paths on every
  Spectrum variant. While it was red there was no coverage of either, which is
  why restoring it preceded all other campaign work.
- Catalogue frame and audio hashes are Emu198x regression oracles, not
  independent hardware evidence.
- The runtime suite passes 35/35, and 48K and 128K boot invariants both pass.

## Why a parity claim is not yet defensible

### The graded-survey gap

The C64 survey runs 17 programs across 13 VIC-II categories and reports a
pixel-match fraction per category. That is what took `colorfetchbug` from
69.294% to five exact programs across four commits — each commit could see
which category it moved and which it disturbed.

The Spectrum has no comparable instrument, so it has no equivalent of the C64's
"54 pixels across eight rows, isolated to delayed C-data output sequencing" —
not because no such residual exists, but because nothing measures one.

**A graded, real-hardware-referenced instrument already exists and is already on
disk.** `timingTests48k.sna`, in the `z80test` fixture directory, was catalogued
as "unidentified — plausibly the ZXSpectrum4.net package" in
[the reference test-ROM catalogue](../../../../reference/by-topic/testing-suites/spectrum-test-roms.md).
Structural identification at `a19de51c` confirms it: the RAM image carries the
load title `Timing Tests 48k Spectrum`, the reporting address
`to - richard@ZXSpectrum4.net`, the prompt `choose test 1-35 or leave blank for
all`, and 35 named opcode-class tests (`JR; INC BC; LD BC,(nn)`,
`EXX; EX AF,AF'; EX DE,HL`, `LDI; LDIR; LDD; LDDR`, `INI; INIR; IND; INDR`,
`OUTI; OTIR; OUTD; OTDR`, and so on).

It is better suited than a reference-image survey here:

- **It self-grades.** Each test prints `Pass` or `Fail` plus `Expecting:` with
  the reference value, so no external reference image needs registering and no
  palette-classification method needs defining. That sidesteps the provenance
  problem the C64 survey still carries.
- **It separates contention from base timing**, running each test
  `{Contended}` and `{Uncontended}`.
- **It classifies the machine** as `TYPE1 (Early)` or `TYPE2 (Late)`, bearing
  directly on [`ula-first-fetch-tstate-offset.md`](ula-first-fetch-tstate-offset.md).
- **Its expected values are published real-hardware results** from genuine 48K
  boards, placing it at rank 2 of the adjudication order — above FUSE.

### Gate integrity

The Tom Harte gate resolved no corpus and reported `test result: ok`. Two
causes, both fixed at `bd4e7887`: the fallback path was off by one directory
level (`../assets/…` resolves to `198x/Emu198x/assets`; the shared corpora live
two levels up at `198x/assets/…`), and a missing fixture returned early rather
than failing. `find_zex_binary` carried the identical off-by-one. CI was
unaffected because `nightly-accuracy` always exports the env vars — which is
precisely why it went unnoticed locally.

This matters beyond one gate: **a soft-skip that reports `ok` is
indistinguishable from a pass** in any log or summary, and the same pattern
appears across the repo's fixture-gated tests (`z80test`, `float_bus`,
`tape_smoke` all `eprintln!` and return). Whether that house pattern should
change fleet-wide is a separate decision, deliberately not taken here.

### Other claim boundaries

- **The four residual FUSE block-repeat disagreements are reclassified, not
  closed.** `edb2_1 INIR`, `edb3_1 OTIR`, `edb9_2 CPDR` and `edbb_1 OTDR`
  differ only in the undocumented X/Y bits. They were tracked as
  "silicon-variable at the final iteration ... effectively unclosable without
  silicon evidence". Both halves of that are wrong; see the step 4 progress-log
  entry. The disagreement is real and stays allowlisted, but it is now a
  tractable inconsistency in our own repeating-iteration model rather than a
  silicon mystery.
- **`Float48K` now accepts T-state 14337**, where `tests/spectrum.md` documents
  14338 → `255` and 14339 → `128`. The change is a deliberate re-pin
  (`db49e7cd`), and the architecture review already carries "48K T-state probe
  offset" as a deferred thread — but the reference catalogue puts real 48K
  hardware at ~14338 for float48k and ~14339 for ulatest3, so the probe is now
  two T-states earlier than the published figure. This may be a frame-origin
  convention difference rather than a behaviour error. Nothing currently
  distinguishes the two, which is the campaign's argument in miniature.
- **No 128K, +2A, +2B or +3 timing suite is registered.** Every contention test
  above targets the 48K Ferranti ULA. The Sinclair 7K010E's phase-1 contention
  and the Amstrad 40077's MREQ-only contention have no published equivalent.
- **`ulatest3` and `progforhackers` are unsourced.** Both are named canonical
  oracles in the reference catalogue; neither is on disk.
- **Deferred architecture-review fidelity findings remain open**: palette
  luminance deviating from BT.601, the beeper's four output voltages modelled
  as two, and the 5C-versus-6C breezeway shift.
- Catalogue coverage is 103 entries against a bar of the full Code198x
  curriculum corpus; authoring continues as titles enter the curriculum.

## Ordered closure campaign

Each implementation change is committed separately from evidence
requalification, with a Conventional Commits subject so release-plz can act on
it (`fix:` / `feat:` bump; `test:` / `docs:` do not). The Amiga and C64
campaigns produced 40 consecutive non-conventional commits and therefore no
version bump at all; this campaign does not repeat that.

1. **Restore the catalogue, then preserve it.** This is now the campaign's
   first real work item, not a formality. Re-capture all 103 entries' frame and
   audio hashes at `AUDIO_ROUTING_VERSION` 3 / `FRAME_ROUTING_VERSION` 4, then
   verify ordinary and fresh-runtime replay across all eight variants.

   Re-capture accepts current behaviour as correct, so it must not be
   mechanical. Two intentional changes are being baked in — the beeper
   AC-coupling (`85f3abbc`) and the 128K HALT2INT contention pin (`9d2ef79e`) —
   and any *unintentional* drift in the same nine-week window would be baked in
   alongside them. The 2026-05-19/-20 re-capture wave handled this by holding
   R-Type's 128 audio hash as a canonical *unchanged* invariant and confirming
   it did not move; this wave needs an equivalent control chosen before
   capture, not after. Commit the re-capture separately from any behaviour
   change, as that wave did across nine commits.

   Thereafter no timing change may weaken the catalogue, its snapshot
   round-trip gate, the routing-version constants, or any of the eight passing
   ULA gates.
2. **Register the ZXSpectrum4.net 35-test suite as a graded survey.** Pin the
   image by SHA-256, boot it headless, drive the "all tests" path, and parse the
   per-test `Pass` / `Fail` / `Expecting:` output into a revision-keyed report
   under `target/accuracy/`, shaped like the C64 VIC-II survey report. Record
   contended and uncontended results separately and the detected `TYPE1` /
   `TYPE2` classification. Resolve upstream provenance and licence; the sourcing
   path is `zxspectrum4.net/op_timing.php`, and the published real-hardware
   results table at `spectrum48k_timing_results.htm` is the reference to
   register alongside it.
3. **Close whatever the survey opens**, worst category first, under the standard
   rule: a change must improve the targeted oracle without absorbing an
   unexplained regression in a stronger lane. Treat the test program and its
   published expected values as the black-box contract; consult vendored Fuse,
   SpecIde, zesarux and the MiSTer core as implementation evidence, never as
   specification.
4. ~~**Verify the `z80test` block-instruction coverage question** and
   reclassify the four FUSE residuals.~~ **Done 2026-08-09** — see the progress
   log. Follow-on: differential the repeating-iteration flag rule against the
   vendored SpecIde, Fuse and zesarux implementations to explain why `INDR`
   agrees with FUSE while its four siblings do not.
5. **Reconcile the `Float48K` probe offset** against the published real-hardware
   value, and record whether the two-T-state difference is a frame-origin
   convention or a behaviour question.
6. **Build the RZX replay harness** on `format-sinclair-zx-spectrum-rzx`,
   filling the empty rank-1 slot in the adjudication order and converting RZX
   Archive recordings into deterministic, T-state-bounded per-title tests. Start
   with recordings of catalogue titles so harness and catalogue cross-check each
   other.
7. **Extend contention evidence to 128K and the Amstrad gate array.** If no
   published suite exists, build one in-house and state plainly that it is
   project-authored and emulator-neutral, in the shape of the Amiga
   programmable-HBLANK corpus. `testInt.tap` (see the progress log) is a
   starting oracle for INT pulse length on 128K-class machines.
8. **Source `ulatest3` and `progforhackers`** and wire both.
9. **Re-run every declared gate** at the closing revision and classify every
   remaining disagreement as fixed, explicitly outside the supported claim, or
   blocked on stronger evidence.

## Step 3 working contract

Agreed 2026-08-10 so unattended closure work is auditable against
something written rather than judgement in the moment.

**Metric.** Contended failures in the timing survey, from a baseline of
33/35 at `242e9abe` (uncontended 32 pass / 3 fail).

**A change is rejected unless all four hold:**

1. Survey contended failures strictly decrease, and uncontended does not
   get worse.
2. All eight binary ULA gates still pass.
3. CPU oracles unchanged: `z80test` 6/6 with zero allowlist, FUSE
   1,351/1,356 with 0 unexpected, Tom Harte 1,604,000/1,604,000.
4. Catalogue stays 103/103 PASS and 103/103 SNAP-PASS.

Gates 1–3 run per candidate (minutes); gate 4 runs before any commit
(~90 minutes).

**Prohibited.** Weakening, skipping or allowlisting any gate to make a
change pass. **Re-capturing catalogue hashes** — that would let a bad
contention change launder itself green, and a re-capture needs a control
chosen before capture, which is a decision rather than unattended work.
Routing-version bumps. Anything outside the Spectrum ULA and the three
named instruction-timing failures.

**Method.** Per RULES §32, work from the vendored reference emulators —
SpecIde first as the closest architecturally, then Fuse and zesarux —
never deduce contention from the spec alone.

**Stop and report** on: three consecutive refuted hypotheses in one
category; any guardrail that cannot be satisfied; or a change that
improves the survey while moving a catalogue hash, which is the
CPU-timing trap in
[routing versions do not cover CPU timing](routing-versions-do-not-cover-cpu-timing.md)
and needs a human.

## Non-goals

This campaign does not expand into every Spectrum clone or peripheral.

- **Scorpion ZS-256 screen rendering** stays out. It is a clone outside the
  October target, the three interacting fixes are already researched against
  Fuse's `machines/scorpion.c` and written up in
  [`tests/spectrum.md`](../tests/spectrum.md), and they can land in one focused
  session whenever that work is scheduled.
- **Pentagon** breadth does not keep the campaign open. Note the tension: the
  newly identified `testInt.tap` is Pentagon-aware, so Pentagon evidence may
  arrive as a by-product of step 7. Accepting it as evidence is fine; opening a
  Pentagon workstream is not.
- Deferred architecture-review fidelity findings (BT.601 palette, four-level
  beeper, breezeway shift) enter only if a selected closure case requires one.
- The campaign does not claim physical-hardware conformance from Fuse, SpecIde,
  MiSTer or Emu198x-produced output. Software comparison, published
  real-hardware result tables and direct physical measurement remain distinct
  evidence classes.

## Pivot gate

The broad Spectrum push ends when:

- the 35-test timing survey runs as a revision-keyed report with pinned fixture
  identity, and every category either passes or carries a precise retained
  disagreement signature;
- the four FUSE block-repeat residuals are explained — why `INDR` agrees and
  its four siblings do not — and then fixed, scoped out, or recorded as
  blocked on new evidence;
- the `Float48K` probe offset is reconciled against the published figure;
- an RZX replay harness runs at least one real-hardware recording per SOLID
  variant that has one available;
- 128K and Amstrad-class contention have registered evidence, in-house or
  otherwise, with provenance and boundary stated;
- the 103-entry catalogue passes ordinary and fresh-runtime replay gates at the
  closing revision; and
- a revision-keyed closure report records every gate and its evidence class.

After that gate, Spectrum work becomes failure-driven. New work must begin from
a real-software failure, a comparator disagreement, new primary or hardware
evidence, or an explicit expansion of the supported configuration claim.

## Progress log

| Date | Step | Result |
| --- | --- | --- |
| 2026-08-09 | Campaign baseline | Every declared gate re-run at `a19de51c`. `z80test` 6/6 with zero allowlist; FUSE 1,351/1,356 with 0 unexpected; ZEXDOC and ZEXALL 67/67; Tom Harte 1,604,000/1,604,000 with 0 failed opcodes; 48K and 128K boot; `Float48K` at 14337 and `Float128K` both strict pass; six tape-smoke goldens byte-equal; runtime suite 35/35. No regression against the recorded figures; the ten-week-old status doc is accurate where it makes claims. |
| 2026-08-09 | 2. Survey source identified | `timingTests48k.sna` structurally confirmed as the ZXSpectrum4.net 35-test suite: self-grading, contended/uncontended, early/late classification, published real-hardware expected values. Already on disk; no acquisition needed. Upstream provenance and licence still to resolve. |
| 2026-08-09 | Gate integrity | Commit `bd4e7887`. The Tom Harte gate resolved no corpus and reported `ok`; the fallback path was off by one directory level and a missing fixture returned early instead of failing. `find_zex_binary` had the identical off-by-one. The gate now resolves with no env var set and reports 1,604,000/1,604,000. |
| 2026-08-09 | 7. 128K oracle identified | `testInt.tap`, catalogued as "unidentified — likely a Woody interrupt timing test", is in fact **TEST INT v1.10 by Yuri Kovalenko, "COMPER-Utility", 1995** — a Soviet-scene diagnostic that measures INT signal duration against a мала/Норма/велика band on a 10–120 scale, alongside effective data-bus bits and an IM 2 figure. It targets Pentagon 48/128 and the Sinclair Spectrum and refuses to run below 128K. It is a direct oracle for INT pulse length, which no current gate measures, and it discriminates Pentagon from Sinclair. Turning it into an assertion needs a screen-region decode or a RAM probe, since it reports through a bar graph rather than a printed number. |
| 2026-08-10 | 3. Two hypotheses refuted, both reverted, nothing landed | **(a) The contention condition.** Ours OR'd a memory term with an I/O term and extended I/O with a one-half-cycle-old IORQ; SpecIde's `ULA.cc` instead suppresses memory contention while I/O contends and lifts I/O using a full-T-state-delayed IORQ, documented against the four canonical cases. Adopting SpecIde's expression changed **only test 35** (`IN`/`OUT`) and moved it *away* from expected (`R` 36 → 51, expected 40); every other measurement was byte-identical. That is itself the useful part: for pure memory access the two expressions reduce to the same thing, so **our memory-contention condition was never the difference**. Reverted. **(b) Delay-table phase.** Seam 1 shifted `MEM_TABLE` and `IDLE_TABLE` four entries left (fetches to phases 4/6/8/10) while `DELAY_TABLE_48K` stayed byte-identical to SpecIde's unshifted table, so contention looked 2 T-states out of phase with our own fetch. Rotating it four left made things worse — 38 failing, uncontended 32→30 — while contended pass/fail was unchanged. Reverted. |
| 2026-08-10 | 3. What the refutations established | Contention is live and phase-sensitive: the rotation moved contended measurements substantially (test 2 closer on all three readings, test 1 further on `loop`), just never across a threshold. And the error has a consistent direction — of the 24 contended failures reporting a loop count, **21 measure below expected and 3 above**. A lower loop count means fewer iterations completed per frame, so **we stall the CPU more than real hardware does**. That points at the amount of contention per window rather than only its phase, which is where the next hypothesis should start. Neither the delay table nor the memory-contention condition is the cause; both now have evidence against them. |
| 2026-08-10 | 2. **Survey landed — contention is the finding** | Commit `928ea34d`. All 35 tests, both modes, **70/70 cases recorded, 36 failing**, written to `target/accuracy/spectrum-timing-survey/<revision>/report.json`. The split is the result: **uncontended 32 pass / 3 fail; contended 2 pass / 33 fail.** Base instruction timing is essentially right; contention disagrees with real hardware across nearly every opcode class. The three uncontended failures are independent of contention: `LD A,(ii+n); LD r,(ii+n)` (22), `RST 18` (34), and `IN A,(n); OUT (n),A; IN r,(C); OUT (C),r` (35). |
| 2026-08-10 | 2. Type-mismatch hypothesis **refuted** | A near-uniform contended failure looks more like one systematic difference than 33 independent ones, and the obvious candidate was a machine-type mismatch: the suite classifies us `TYPE1 (Early)`, and the community results table records TYPE 1 and TYPE 2 separately. Refuted on two independent grounds. First, the suite adapts: its detection branches `POKE 40004,0` for TYPE1 and `POKE 40004,1` for TYPE2, and forcing the flag to 1 changes test 1's expectations from the values we match (`R=43 loop=1201 sp=56806`, Pass) to different ones (`R=40 loop=1200 sp=56801`, Fail) — so the flag drives the comparison and we were judged against TYPE1 correctly. Second, uncontended passes 32/35 against those same TYPE1 expectations; a wrong classification would have skewed uncontended too. The classification is right and the contention model is wrong. Worst-first closure can proceed on that basis. |
| 2026-08-10 | 2. Survey validated — **first graded failure found** | The suite runs headlessly from the `.sna` with no harness work: boot, press ENTER at `choose test 1-35 or leave blank for all`, and it self-reports. Emu198x classifies as **`TYPE1 (Early)` timings**. Test 1 `{Uncontended}` (`JR; INC BC; LD BC,(nn); LD (nn),BC`) reports **Pass** — `R=43 loop=1201 sp=56806`. Test 1 `{Contended}` reports **Fail** — got `R=100 loop=987 sp=23296`, expecting `R=74 loop=1014`. So the first contended test in the suite disagrees with the published real-48K reference, while all eight binary ULA gates pass. That is the campaign's thesis demonstrated on one screen: the existing instrumentation could not have found this, and this instrument found it immediately. Output format for the harness: three measured quantities per case (`R`, `loop`, `sp`), an explicit `Expecting:` line on failure, `{Contended}` / `{Uncontended}` per test, and a blocking `Press any key for next test.` between tests — so the runner must drive 35 key presses and scrape between them rather than run to completion. |
| 2026-08-09 | 1. **Step 1 complete — catalogue fully green** | Commit `ad686cb6`. The `+3` snapshot failures traced to snapshot version 2 (Seam 3, `7ea88420`, 2026-05-20), which made a mounted disk survive restore by caching the raw image and replaying it after decode — through `load_disk_image` → `insert_disk`, which invalidates the FDC's re-read key, re-read count and per-drive `ReadID` position. Those fields serialise normally and decoded correctly; the replay ran last and cleared what the decode had just restored. So the change that made the disk survive restore is what introduced the loss, six days after the last green run. Fixed by separating the two events: `Upd765a::reattach_disk` mounts without touching cached state, and `SpectrumMachine::reattach_disk_image` defaults to `load_disk_image` so machines that cache nothing are unaffected. Not merely a hash mismatch — those fields drive the marginal-encoding model, so reloading a `+3` save state reset the re-read counter and a Speedlock-style loader mid-retry could diverge from an unbroken run. **The full catalogue now runs 103/103 PASS and 103/103 SNAP-PASS**, the first clean result since 2026-06-05. A first hypothesis (that the catalogue's own restore path called `insert_disk`) was refuted by an isolated FDC round-trip test before any fix was written — right function, wrong caller. |
| 2026-08-09 | 1. Catalogue restored — and it immediately caught a hidden regression | Media paths repointed (`b060ac16`) and all 103 entries re-captured at routing 3/4 (`b822b5fa`), gated on the 48K-family frame control: 46 of 47 byte-identical, the one exception investigated and explained rather than absorbed. Verification: **103/103 PASS** on frame and audio — the re-capture is correct and the frame/audio regression foundation is real again. **87/103 SNAP-PASS**: all 16 `+3` entries fail the snapshot re-encode, and nothing outside `+3` does. Identical signature on every one — the re-encoded snapshot is exactly **4 bytes shorter**, with tens of thousands of differing bytes starting ~60% in (`Some(1) -> Some(0)`). This is a **regression**, not a known limitation: the SOLID status doc records 2026-05-14 with all 16 `+3` entries SNAP-PASS and "re-encode bytes are byte-identical". It is not caused by the re-capture, which only changed expected hashes — the snapshot check compares snapshot bytes against re-encoded snapshot bytes and never consults the manifest. `+3` is the only variant with an FDC, and `+2A`/`+2B` share the Amstrad class and pass, so the fault is in the disk-image serialisation. It hid for roughly three months because the catalogue could not run. |
| 2026-08-09 | Baseline completed — catalogue is RED | The six previously unmeasured variant crates (16K, Plus, +2, +2A, +2B, +3) all pass, though each carries only lib unit tests and a boot test — no ULA, timing or floating-bus gate among them, so six of eight SOLID variants have no accuracy instrumentation beyond "it boots". The catalogue does not run at all: the manifest declares `audio_routing_version = 1` / `frame_routing_version = 3` against a runtime at 3 / 4, so Seam 4 refuses at the first entry (`manic-miner`) and zero of 103 entries are verified. Red since 2026-06-05 (`85f3abbc`, beeper AC-coupling), moved again 2026-07-26 (`9d2ef79e`), with 20 Spectrum-crate commits landing unverified since the last re-capture. Restoring it is now step 1. Corrected: the manifest holds 103 entries, not the 114 first reported — the earlier count included non-`[[entry]]` tables. |
| 2026-08-09 | Tooling | Commit `abd0360c`. `EMU198X_CATALOGUE_SYSTEMS` scopes a catalogue run to named manifests. A full pass is ~192 entries across four systems; a per-system campaign needs its own system's baseline without tripling wall time or absorbing a sibling system's in-flight work. |
| 2026-08-09 | 4. Block-repeat residuals reclassified | The standing note called these "the *final* repeat iteration" and "silicon-variable … effectively unclosable without silicon evidence". Both are wrong, and the second followed from the first. FUSE runs `edb2_1`/`edb3_1`/`edb9_2`/`edbb_1` for **21 T-states** with B = `0x0a`, `0x03`, `0x00`, `0x04` — 21 is the *repeating* cost, 16 the terminating one — so every disputed case observes a **non-final** iteration with PC rewound. `edba_1 INDR` has the identical shape (B = `0x06`, 21 T-states) and passes every bit, so we do model the repeating-iteration rule; it agrees for one instruction and disagrees for four siblings. Separately, `z80full`/`z80flags`/`z80memptr` set `maskflags equ 0`, comparing the full `0xFF` mask including bits 3 and 5, and every block instruction including the `->NOP'` variants passes against real-48K-Zilog CRCs — the suite is not silent on these bits, though it observes the instruction after completion rather than mid-repeat. Net: the debt is not closed, but it moves from silicon mystery to a tractable inconsistency in our own model. Comment corrected at the allowlist; the gate still reports 1,351/1,356 with 0 unexpected. |
| 2026-08-09 | Tooling | Commit `32d59886`. `emu198x-spectrum --machine ID` boots any of the 13 variants headlessly, so variant-specific test software no longer needs a scratch JSON script. Needed by steps 2 and 7. |

## Related Documents

- [Spectrum architecture review](spectrum-architecture-review.md)
- [Spectrum test oracle priority](spectrum-test-oracle-priority.md)
- [ULA first-fetch T-state offset](ula-first-fetch-tstate-offset.md)
- [ULA drives model](ula-drives-model.md)
- [C64 accuracy closure campaign](c64-accuracy-closure-campaign.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)
- [October catalogue](october-catalogue.md)
- [Spectrum test results](../tests/spectrum.md)
- [Spectrum contention](../systems/spectrum/contention.md)
- [Floating-bus accuracy](../systems/spectrum/floating-bus-accuracy.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [Spectrum catalogue manifest](../../crates/emu198x-catalogue/manifest/spectrum.toml)
- [ZX Spectrum test ROM catalogue](../../../../reference/by-topic/testing-suites/spectrum-test-roms.md)
