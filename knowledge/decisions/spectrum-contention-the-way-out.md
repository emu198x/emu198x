# Plan: how to actually finish Spectrum contention

**Date:** 2026-08-11
**Status:** IN PROGRESS — Phase 1 done; the I/O sample instant derived and
the floating bus closed on both machines; the contention window's phase and
edge derived, taking the survey to 13 of 70; Phase 3's gate diagnosis
disconfirmed twice and rewritten. See “What happened” below before acting on
the plan, and treat the plan's own Phase 3 as superseded by it.
**Supersedes the approach in:**
[`spectrum-contention-vs-floating-bus.md`](spectrum-contention-vs-floating-bus.md)

## The mistake underneath all of it

Five separate attempts to fix contention see-sawed, and the reason was not
any of the bugs found along the way. It was the oracle.

**FUSE's contention is a per-M-cycle table lookup.** At each M-cycle start it
does `tstates += delay[tstates]; tstates += length`. It is an approximation
calibrated to make real software work, and it is a very good one — but it is
not a claim about signals.

**Our engine is signal-level by mandate.** RULES.md #5: the Z80 is a
half-cycle signal-level state machine, no instruction-level abstraction.
Contention emerges from pins and a clock gate, exactly as the silicon does it.

Scoring a signal-level engine against an M-cycle approximation and treating
every divergence as our defect is a category error. It is why the frame-wide
differential and the gate-level differential kept contradicting each other,
and why "fixing" the gate to match the silicon made the FUSE score worse.

The evidence was in the record from the first day and went unread: **wiring
`MREQT23` took the ZXSpectrum4.net timing survey from 34/70 to 37/70.** A test
program written to detect timing errors on real hardware said the
silicon-matching gate was *more* correct, while FUSE's numbers said it was
worse. We believed FUSE.

## The way out: SpecIde

`198x/emulators/zx-spectrum/SpecIde/source/src/ULA.cc` is the only other
**signal-level** Spectrum ULA we hold — the same abstraction this engine
works at. Our own emulator index already calls it "most faithful to hardware;
bus arbitration emerges from the model". Its gate:

```cpp
bool memContention    = contendedBank && z80Clock;
bool memContentionOff = !(z80_c & SIGNAL_MREQ_);      // live /MREQ, not MREQT23
bool ioUlaPort        = !(z80_a & 0x0001);
bool iorqLow          = !(z80_c & SIGNAL_IORQ_);      // T2 TW T3
bool iorqLow_d        = !(z80_c_2 & SIGNAL_IORQ_);    // TW T3 —, a free-running delay
bool ioContention     = ioUlaPort && iorqLow && z80Clock;
bool ioContentionOff  = ioUlaPort && iorqLow_d;

bool contention = (memContention && !ioContention && !memContentionOff)
               || (ioContention && !ioContentionOff);
cpuClock = !(contention && delayTable[pixel & 0x0F]);
```

This is the model that reconciles everything the session could not:

- It is **signal-level**, so it is a legitimate target for our engine.
- It uses the **live `/MREQ`**, deliberately — not the `MREQT23` latch. So it
  agrees with FUSE on memory contention.
- It **has the `IOREQTW3` cancellation**, as a two-stage delay line rather
  than a gated latch — the same signal Smith draws, reached differently.
- It makes memory and I/O contention **mutually exclusive**, so a contended
  *port* address is charged once as I/O rather than twice. We have never had
  this term, and it is the most likely cause of `$40FE` and `$40FF` costing
  the same in our engine when FUSE separates them by 5.10 T-states.

We hold only the first term of the three.

**A first port was attempted and reverted.** Substituting our
`z80_iorq_prev2` for SpecIde's `z80_c_2` produced behaviour byte-identical to
today's, because ours freezes during a stall (`track_z80_clock` only runs when
the CPU is clocked) while SpecIde's is a free-running shift register clocked
every ULA cycle. **That difference is the first thing to fix**, and it is a
small one.

## The plan

**Phase 1 — fix the Z80's pins.** Five defects, measured against Zilog in
`zilog-z80/tests/bus_pin_waveform.rs`: `M1` opcode and refresh strobes each a
half-cycle short, memory read `/MREQ` released a full T-state early, memory
write `/WR` half early, `/IORQ` released a T-state early. A signal-level ULA
fed wrong pins can never be right, and none of this is contentious — it is
Zilog against us. Add golden waveforms per M-cycle type so they cannot drift
back. **Prerequisite for everything else.**

### Phase 2 baseline, recorded 2026-08-11 at `d9d5e58b`

```
classification: TYPE1 (Early) timings detected.
cases recorded: 70  failing: 36
```

**34/70.** The infrastructure Phase 2 needs already exists: the survey writes
`target/accuracy/spectrum-timing-survey/<commit>/report.json`, so per-commit
tracking is a matter of using it rather than building it.

Two things make this a better gate than the frame-wide differential, beyond
being real software:

- **Every failure names an instruction class.** `test 35 IN A,(n); OUT (n),A;
  IN r,(C)` is the I/O contention work; `test 30 LDI; LDIR` is block transfer;
  `test 3 NOP; LD r,r; INC r; DEC r` is the simplest possible case and is
  currently 407 against an expected 405. That is a diagnostic surface a
  frame-total can never give.
- **It classifies the machine.** "TYPE1 (Early)" is the survey's own reading of
  which real-hardware timing variant we behave like — a fact about us worth
  watching across changes, and one no model differential reports.

Of the 36 failures, 33 are contended cases and 3 uncontended, so most of the
gap is contention rather than base instruction timing.

**Phase 2 — change the acceptance criterion.** Promote real test programs to
the primary contention gate:

- the ZXSpectrum4.net timing survey (70 cases, currently 34) — the primary;
- floatspy, Float48K;
- the catalogue, once both axes are settled.

Demote the frame-wide FUSE differential to a *diagnostic*. It stays valuable —
it is precise, fast and phase-resolved — but a disagreement with it becomes a
question rather than a verdict. Acquire more timing suites; there are others
for the Spectrum and they are cheap to add.

**Phase 3 — port SpecIde's gate properly.** Give the engine a free-running
`IORQ` delay line clocked every ULA cycle, then implement the three-term
expression above. Score on the survey first, the differentials second.

**Phase 4 — consolidate.** One contention implementation with variant decodes
injected, not three hand-written copies in `ferranti-ula-6c001e`,
`sinclair-ula-7k010e` and `timex-scld` that have already drifted apart.

## The architectural suspects

The four phases above assume the design is sound and only the logic is wrong.
That assumption deserves testing, because several of this session's dead ends
were not logic errors — they were the design making a half-cycle question
unanswerable. Each item below is listed with the concrete confusion it caused,
not as a tidy-up.

**A. RETRACTED — the tick order is correct.** Recorded because it was
asserted here and acted on, and because the reasoning is worth not repeating.

The claim was that ticking the ULA before the CPU inserts a half-cycle into a
feedback loop that settles combinationally in silicon. There is no such loop.
The Z80 drives pins that **persist until its next tick**, so a ULA sampling
"the previous tick's pins" is sampling the pins the Z80 is *currently
driving* — which is exactly what the hardware sees. `driver.rs` is right.

What is really there is narrower and is not architectural: the Z80's phase
handlers set pins that become visible to the ULA in the *following*
half-cycle, and that convention is written down nowhere. It is what made two
synthetic-pin harnesses wrong in opposite directions. The fix is Phase 1 plus
a stated convention and the `bus_pin_waveform.rs` golden test, not a
restructure.

The original claim follows, struck through in substance:

~~The ULA/CPU tick order puts a half-cycle of delay in a feedback loop that
has none in hardware.~~ `driver.rs` calls `tick_ula()` and then
`tick_cpu_and_bus()`, so the gate decides half-cycle *N* from pins the CPU
presented at *N−1*. In silicon the loop — gate to `CPUClk` to Z80 pins back to
gate — settles combinationally inside one clock period. Ours cannot, so every
comparison against a reference needs a half-cycle correction applied
somewhere, by someone who has correctly guessed the direction.

*Evidence:* two separate synthetic-pin harnesses got that direction wrong,
in opposite directions, and both produced confident wrong verdicts on the
ULA-port classes. The `ula_gate_vs_hdl` recorder had to be built *inside*
`FerrantiUla::tick` precisely because that is the only place the skew is not
ambiguous.

*Possible rework:* split the CPU tick into **present pins** and **advance
state**, and order the half-cycle as present → gate → advance. The pins the
gate sees would then be the ones the CPU is actually driving, and the
correction disappears rather than moving.

**B. Clock domains for ULA-internal signals were never decided.**
`z80_iorq_prev` / `z80_iorq_prev2` live inside `track_z80_clock`, which only
runs when the CPU is clocked — so they **freeze during a stall**. SpecIde's
equivalent `z80_c_2` is a free-running shift clocked every ULA cycle. The HDL's
`ioreqtw3` / `mreqt23` are `posedge CPUClk`. Those are three different domains
and ours was not chosen, it was inherited from where the code happened to sit.

*Evidence:* the SpecIde gate port was byte-identical to doing nothing, purely
because of this.

**C. DONE, and it was right.** `DELAY_TABLE_48K` was a hand-written 16-entry
table where the hardware has two counter bits. The gate is `C2 | C3`.
Encoding that as a literal invited exactly the phase error it had.

The rework this entry proposed — *derive the window from the pixel counter's
bits, so the phase relationship is stated once and cannot drift* — is what
landed, and the error it was carrying was larger than the entry guessed. Not
one pixel: **three**, and a second four-pixel error in the window's edge
alongside it. Together they were a whole T-state of phase and two T-states
at every line boundary, worth 88,871 of 371,054 samples against FUSE and
sixteen cases of the timing survey.

The entry was also right about *why* it survived: "invisible at T-state
resolution". Every probe pointed at the table measured the gate against
itself. It took a differential resolved by arrival T-state and anchored to
the interrupt to see it. See “The memory differential, and what it found”.

**D. Three hand-written copies of the gate.** `ferranti-ula-6c001e`,
`sinclair-ula-7k010e` and `timex-scld` each carry their own boolean, and they
have already diverged — the 128K one contends from `/Border` while the 48K
contends from the video-fetch window, a difference recorded in a comment and
never reconciled. RULES.md #9 asks for one ULA implementation per variant; it
does not ask for the same contention expression written out three times. What
differs between variants is the *decode* — which ports, which pages — not the
topology.

## Sequencing, if the rework is taken

With A retracted, there is **no large rework in front of Phase 3**. B and C
are small and go with Phase 1; D is a cleanup and goes last, once there is one
correct expression worth having a single copy of.

The standing rule stays regardless: any change here takes the survey baseline
(34/70, recorded above) as its gate, and one that does not raise it gets
reverted.

## Why this is not another see-saw

Every previous attempt changed the engine and asked an oracle whether it had
improved, where the oracle was a different kind of model. This changes the
*oracle* first, to real software, and takes the target from an implementation
at our own abstraction level that is independently regarded as accurate.

If Phase 3 raises the survey and holds the floating-bus tests, it is right. If
it raises the survey and lowers the FUSE differential, **the survey wins** —
that is what Phase 2 decides, in advance, so the result cannot be argued
either way after the fact.

## What would still settle it beyond doubt

Real hardware. A 48K plus a logic analyser, or someone in the community
running a timing suite. Not currently available, and worth revisiting given
how much time this has cost — an accuracy-first emulator with no access to the
machine it emulates is working at a permanent disadvantage.

## What happened

Recorded 2026-08-11, after Phase 1 and the first half of Phase 3.

### Phase 1 landed, and found more than five defects

`bus_pin_waveform.rs` is an asserting golden covering `M1`, memory read,
memory write, I/O read, I/O write, an internal cycle and the not-taken
displacement cycle. Every strobe now matches Zilog UM0080, cross-checked
against SpecIde's half-cycle states, which agree exactly on the memory
read and write and disagree in two places where Zilog governs.

The five defects the plan listed were real. Four more were not on the
list:

- **`/RFSH` ended a T-state early.** A ULA input — the Sinclair ULAs
  derive snow from it.
- **The next M-cycle's address was presented half a T-state early**, on
  the previous cycle's last half-cycle, because `try_advance_walker`
  called `setup_signals` as it handed over. This was a live corruption
  once `/WR` correctly ran to the end of `T3`: a host driving the bus from
  raw pins — Pentagon, Scorpion and Timex all do — wrote the outgoing byte
  to the incoming address. `IM 2` jumped to `$5601` instead of `$5678`.
- **`MStep::ContendPc` had no `/RD` and a short `/MREQ`.** FUSE scores the
  not-taken `JR cc` / `DJNZ` operand cycle as `contend_read( PC, 3 )`.
- **Internal cycles drove `IR`.** Nothing drives the address bus during an
  internal cycle; it holds the last address driven. FUSE gets both cases
  from that one rule — `IR` after `M1`, `DE` after `LDIR`'s write.

### The gate armed on the wrong clock phase, and that is now measured

The plan expected Phase 1 to move the survey and said to record which way.
It moved it **down**: 36 failing to 40. That was correct behaviour from a
correct pin. With `/MREQ` running `T1b`–`T3b` it covers five of a memory
read's six half-cycles, leaving exactly **one** arming half-cycle per
M-cycle — the one where the CPU is about to drop it. The gate's other
term selected the opposite parity, so the engine could not contend a
memory access at all.

`machine-sinclair-zx-spectrum-48k/tests/contention_arming.rs` measures
this on the real machine, from inside `FerrantiUla::tick`. Before the fix:
four arming opportunities in the window, all on `M1(T1Fall)`, all with
`z80_clock_high` false, zero stalls.

The fix is `UlaEngine::gate_arms_this_halfcycle`, and it is derived: the
edge the ULA withholds is the one that drops `/MREQ`, a `Fall` phase, and
`z80_clock_high` is measured true on `Rise` phases. Survey **36 failing →
29 of 70**, the best this engine has recorded — better than the 34
baseline and better than the 37 that wiring `MREQT23` reached.

**This is the likely origin of `MREQT23`.** The latch was invented to
cancel an over-contention that existed only because the strobe was two and
a half T-states short. With the pin right, `!cpu_mreq` inhibits re-arming
on its own. No latch is wired in.

### Phase 3's SpecIde gate is disconfirmed as written

Ported whole, not adjusted: the free-running delay line, the `IOREQTW3`
cancellation, and mutual exclusion between the memory and I/O terms. The
prerequisite the plan names — clocking the delay line on the ULA rather
than the CPU — was fixed in the same change, so this was not the earlier
no-op.

It moved nothing toward the target and the I/O differential away from it:
survey unchanged at 29, I/O differential 75,081 → 84,099 against a 30,741
ratchet, floating bus unchanged, floatspy still red. **Reverted.** The
ratchet's own rule is that a rise means the change made I/O contention
worse, and there is no argument here that FUSE is wrong.

So the plan's diagnosis — that we hold one of three terms and the other
two are the answer — is not supported. The I/O path needs its own
derivation, not this port.

### The floating-bus sample instant is derived, and both leads are gone

`io_read` fires on the `/IORQ` rising edge; the CPU latches the data bus at
the end of the M-cycle. In the phase machine that is `IoRead(T2Fall)` to
`IoRead(T4Fall)` — four half-cycles, **two T-states** — and it is fixed
M-cycle geometry, the same for every variant, port and host. SpecIde reads
at `ST_IORD_T3L_DATARD`, its own last half-cycle; FUSE calls
`readport_internal` after `contend_port_early` (+1) and `_late` (+2), so
cycle start + 3 against an edge at + 1. All three agree.

Both `SAMPLE_LEAD` constants are gone, replaced by one
`zilog_z80::IO_READ_DATA_LATCH_LEAD_TSTATES` that `bus_pin_waveform`
re-derives from the recorded waveform rather than restating. Each machine's
origin is now libspectrum's `top_left_pixel` for its own ULA — 14336 for
`timings_frame_ferranti_5c_6c`, **14362** for `timings_frame_ferranti_7c`,
where the 128K core had an undocumented 14363.

Float128K reads 14364. Float48K is unchanged at 14337, which is the check
that this is a rule: the derivation had to reproduce a value it could not
be tuned to.

**#851's numbers were stale before the work started** — it records 14363
and the engine read 14362 once Phase 1 landed. The lesson the +2A taught
applies here too: a measurement taken before a pin change is not evidence
about the engine after it.

One T-state remains and is now isolated rather than absorbed. Float48K
reads 14337 against Woody's hardware-measured 14338, and
`io_contention_oracle`'s `/INT`-pinned origin puts our T-state 0 one
earlier than `top_left_pixel` does. Dropping both origins by one would put
the 48K on Woody's figure and take the 128K off 14364, so the open question
is between the two anchors, not between the two machines.

### The memory differential, and what it found

The gap named below — no asserting memory-side engine-versus-FUSE
differential — is closed. `contention_oracle.rs` now carries
`memory_contention_matches_fuse_at_every_arrival_tstate`: seven instruction
shapes from one to six M-cycles, executing out of contended RAM and reading
contended RAM, every arrival T-state in the frame, scored against FUSE's
per-M-cycle walk at the `/INT`-pinned origin, asserting.

It opened at **88,871 of 371,054** and reproduced the I/O sweep's shape
independently and far more sharply — `+14334` scored 2,328, against 88,871
at the pin. One global shift, so the error was in the contention window's
phase rather than in any instruction shape, exactly as the I/O sweep had
argued.

**Two defects, both phase errors, neither fitted.**

*The window's phase against the fetch.* `DELAY_TABLE_48K` was sixteen
hand-written booleans, free at half-cycles 15, 0, 1 and 2 — straddling a
T-state boundary at both ends, so its effective phase depended on which
parity the CPU arrived on. Smith gives the shape as `CLKWAIT = C3 + C2` and
Chapter 18 declines to pin the counter's absolute phase, but the **HDL pins
it**, because one counter drives both its fetch and its `CLKWAIT`:
`zx_ula`'s `fpga_version/rtl/ula.v` presents display addresses to VRAM at
`hc[3:0]` 8, 9, 12, 13 and attributes at 10, 11, 14, 15, and gates
contention on `hc[2] | hc[3]`. Our fetch group is pixels 4–11 — four pixels
earlier — so `C3 + C2` read on that origin puts the free run at pixels
12–15. `HDL_HC_LEAD_PIXELS` states those four pixels once.

*The window's edge.* Contention was gated on `UlaEngine::video`, which opens
at `fetch_start`. The HDL gates it on `Border_n` = `!hc[8]`: 256 pixels,
sixteen whole fetch cycles, beginning at a cycle *boundary* eight pixels
ahead of that cycle's access. Ours begins at pixel 0, four pixels ahead of
the access at pixel 4. Two T-states, mischarged at both ends of all 192
display lines, and the entire residual left after the phase fix.

Three corroborations that the phase is derived rather than fitted, none of
them the number it was scored on:

- `contention_tables` reproduces `[6,5,4,3,2,1,0,0]` at alignment **0**
  rather than 3 — the CPU's T-state grid and the ULA's pixel counter share
  an origin, where the literal put them one and a half T-states apart.
- `effective_delay_table`'s two clock parities now agree across the whole
  ramp. The literal made them differ inside the free window, because a
  four-pixel free run starting on an odd pixel gives the two parities
  different T-states.
- The pinned origin is now the offset sweep's **minimum**, where it used to
  sit beside a sharp one a T-state away.

### Standing state

| gate | baseline | now | target |
|---|---|---|---|
| ZXSpectrum4.net survey | 36 failing | **13** | < 36 |
| — of which wrong loop count | — | **6** | 0 |
| memory differential | (none) | **18** of 370,024 | 0 |
| I/O differential | 30,741 | **21,510** | < 30,741 |
| Float48K | 14337 | 14337 | 14337 |
| Float128K | 14363 | **14364** | 14364 |
| floatspy | red, 18px | red, 72px | byte-exact |
| +2A max delay | 1 | 1 | 7 |

The survey is the best this engine has recorded: 36 at the start of the
contention work, 34 at the Phase 2 baseline, 37 with `MREQT23` wired, 29
after the arming parity, **13** now.

Worth stating plainly, because the record has been read the other way
before: the window's phase fix *alone* took the survey from 29 to **33**,
and the edge fix took it from 33 to 13. Landing the first without the
second would have looked like the see-saw this record was written about. It
was one defect seen from two ends, and the differential is what said so —
the four cases the phase fix broke were all multi-M-cycle, which is where
the phase fix's whole 9,858 residual also lived.

### What is left on the floating bus

One byte. `floatspy`'s menu prints its `IN()` result as **64** where its
golden and Spectron's `floatspy_48.png` print **0**. That is the whole
72-pixel diff in `tape_smoke` and one character cell of the self-test's
945-pixel gap against Spectron; it was 18 pixels before this work and is
not repaired by it.

The floating-bus *timing* did not move — Float48K reads 14337 and
Float128K 14364, both unchanged through every step. So this is a byte
sampled at an instant contention has moved, coming back `$40` instead of
`$00`. It is the third floating-bus error this record and
`spectrum-contention-vs-floating-bus.md` have both been pointing at,
reduced from a whole-screen diff to a single wrong byte. Neither golden was
re-blessed.

### The four port classes, measured

`contention_arming` now asserts FUSE's four-way table as a count of charge
points — `ula_contend_port_early` and `_late` transcribed, not a constant
lifted out of them.

| port | gate | FUSE | withheld runs begin on |
|---|---|---|---|
| `$40FE` | 2 | 2 | `T1Fall`, `T3Fall` |
| `$00FE` | 1 | 1 | `T3Rise` |
| `$40FF` | **2** | **4** | `T1Fall`, `T3Fall` |
| `$00FF` | 0 | 0 | — |

Three of four agree, which narrows the standing claim that the engine
cannot separate the port classes: it separates `$00FE` from `$40FE`
correctly. The single defect is that `$40FF` gets two runs where FUSE
charges four, and the mechanism is now visible. `ula_io` is false for an
odd port, so the I/O term never fires on `$40FF`; both of its runs are the
*memory* gate, whose `contended_addr && !cpu_mreq` holds for every
half-cycle of an I/O M-cycle because `/MREQ` is never asserted in one.
`$40FE` reaching the right count is the memory gate standing in for
`contend_port_early`, and that may well be right — FUSE's early charge is
conditioned on the same port page.

#### The obvious fix is a no-op, and the next one is worse

Adding `contend_port_late`'s odd-port branch — `(ula_io ||
contended_addr)` — changes **nothing**. Byte-identical: 75,081 of 294,153,
`$40FF` still at two runs, every class unmoved.

The reason is a term interaction the charge-count table cannot show. The
memory gate arms on `gate_arms_this_halfcycle`, a `Fall` phase; the I/O
term arms on `z80_clock_high`, a `Rise`. `track_z80_clock` does not run
while the CPU is stalled, so `z80_clock_high` **freezes** during a stall.
Once the memory gate withholds a half-cycle inside a contended-page I/O
M-cycle, the clock parity stops advancing and the I/O term never sees an
arming half-cycle at all. It is shadowed, not absent.

Making the two mutually exclusive — SpecIde's third term, suppressing the
memory gate while `/IORQ` is live — does not rescue it either: still two
runs, and the differential rises to 78,339. `/IORQ` is not asserted until
`T2Fall`, so at the M-cycle's start the memory gate is the *only* term that
can fire, and suppressing it deletes exactly the charge that was correctly
standing in for `contend_port_early`. **Third disconfirmation of mutual
exclusion**, and the first one with a mechanism rather than a score.

So the I/O shape is not one missing term. Either the gate needs an arming
parity that survives a stall, or the harness's "maximal run of withheld
half-cycles" is the wrong counterpart to FUSE's charge points for a
level-sensitive gate — a run merges two adjacent charges, which its own
doc already flags as a lower bound. Settling which is the next instrument
question, not the next gate change.

### The +2A stale pin is disconfirmed

`fuse_differential`'s drive asserted `/MREQ` for four half-cycles of six
where Phase 1 gave a memory read five, and this gate contends on `/MREQ`
*asserted*, so every number in #856 was taken against a strobe the CPU had
stopped producing. Corrected, it measures 23,060 M-cycles, 1,728 contended,
engine max 1 — identical to the last digit. The stalls never fall on the
half-cycle the correction touches.

Two of #856's other experiments re-run against the correct pin: the
Sinclair polarity with the shipped 3-run mask measures no contention at all
and the self-check catches it; the same polarity with a 14-half-cycle run
reproduces the 6 against 7. That result survives the pin fix.

The residual T-state is a half-cycle lost in `mcycle_cost`'s division, and
moving the run's start does not recover it: `z80_clock_high` freezes during
a stall, so the mask's phase against the arming parity is not fixed and
cannot be settled from a frame *maximum*. Choosing a phase because it makes
the number reach 7 would be a fit. The +2A needs an arrival-resolved
differential of the kind the 48K has before its mask moves; nothing was
landed.

### An instrument lesson, paid for twice

`contention_arming`'s first port-class measurement reported that the
engine collapses FUSE's four classes into two. It was wrong: the harness
set `BC` before settling the machine, ROM code overwrote it, and all four
runs measured `$FFFE`. Moving the assignment after the settle was still
not enough, because the settle stops mid-instruction and the in-flight ROM
instruction retired over the top of it.

The harness self-checked twice, in the shape the +2A differential taught —
"harness fault, not a finding" — and both checks passed, because they
verified that the *recording aligned with the CPU* and alignment was never
the problem. **An instrument must also check that it is driving the thing
it claims to drive.** Corrected, the four classes separate and track
FUSE's ordering; what survives is the narrower divergence already on
record, `$40FE` and `$40FF` costing the same.

A third check has since been added for the same reason: the run must have
met an open delay table. Without it a class reading zero is reporting the
border rather than the gate, and `$00FF` legitimately reads zero — so the
one class that is *supposed* to be silent was indistinguishable from a
harness that had wandered out of the display window.

## The commit sequence

Small, each independently defensible, each with its own gate. The rule
throughout: **the survey baseline is 34/70 and a commit that lowers it gets
reverted.** Nothing here batches a behaviour change with a refactor.

### Phase 1 — pins and conventions

1. **`test(z80)`: golden pin waveforms.** Promote `bus_pin_waveform.rs` from a
   printing diagnostic to an asserting test, locking today's *actual*
   waveforms. No behaviour change. This is the safety net for everything after
   it, and it must land first so that steps 2–6 each show up as a deliberate,
   reviewable diff in a golden file.
2. **`fix(z80)`: `M1` opcode strobe spans `T1b`–`T2b`.** One strobe, one
   commit, golden updated in the same commit with the Zilog citation.
3. **`fix(z80)`: `M1` refresh strobe spans `T3b`–`T4a`.**
4. **`fix(z80)`: memory read holds `/MREQ` and `/RD` to the end of `T3`.** The
   big one — this is the over-contention `MREQT23` was invented to cancel.
   Expect the survey to move; record which way.
5. **`fix(z80)`: memory write holds `/MREQ` and `/WR` to the end of `T3`.**
6. **`fix(z80)`: I/O holds `/IORQ` and `/RD` to the end of the cycle.**
7. **`refactor(ula)`: derive the contention window from counter bits.**
   Replace `DELAY_TABLE_48K` with `C2 | C3`. Behaviour-neutral by
   construction if the phase is preserved; the frame-wide differential is the
   check, and any change in it means the phase was *not* preserved and the
   commit is wrong.

Steps 2–6 will each move the survey, possibly downward in isolation, because
they are five parts of one correction. **They land on a branch and merge as a
unit once the survey is at least 34/70 again**, with the individual commits
preserved. That is the one place batching is right: the Zilog citation is
per-strobe, but the acceptance is collective.

### Phase 2 — the oracle

8. **`test(spectrum)`: make the timing survey an asserting gate at 34/70.**
   A floor, not a target. It fails if the count drops. Cheap, and it is what
   makes every later step safe.
9. **`docs`: demote the frame-wide FUSE differential to a diagnostic** in its
   own module docs, so a future reader does not treat a regression there as a
   defect.

### Phase 3 — the gate

10. **`fix(ula)`: clock the `IORQ` delay line on the ULA, not the CPU.** It
    currently freezes during a stall. Behaviour change, small, and a
    prerequisite — without it the next commit is a no-op, which is exactly
    what happened when it was tried.
11. **`fix(ula)`: cancel I/O contention with the delayed `IORQ`.** The
    `IOREQTW3` term.
12. **`fix(ula)`: make memory and I/O contention mutually exclusive.** The
    term we have never had, and the likeliest cause of `$40FE` and `$40FF`
    costing the same.

Each of 10–12 is scored on the survey and on `ula_gate_vs_hdl`. If 12 does
not separate the contended-page port classes, stop — the model is wrong
again, and the next move is measurement, not another term.

### Phase 4 — consolidation

13. **`refactor(ula)`: one contention expression, variant decodes injected.**
    Pure refactor, no behaviour change, all three variants' tests green
    before and after.
14. **`fix(ula)`: reconcile the 128K window with the 48K.** The divergence
    recorded in `sinclair-ula-7k010e`'s comment — `/Border` against the
    video-fetch window — is a real bug and should be fixed on its own, with
    its own evidence, not smuggled into the refactor.

### What stays unlanded

The `MREQT23` gate remains computed and unconsulted until Phase 3 is done and
the survey has spoken. It is the single most re-litigated item in this
project's history and it does not move again without a number attached.
