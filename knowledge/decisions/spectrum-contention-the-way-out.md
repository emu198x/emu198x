# Plan: how to actually finish Spectrum contention

**Date:** 2026-08-11
**Status:** PROPOSED — not started
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

**A. The ULA/CPU tick order puts a half-cycle of delay in a feedback loop that
has none in hardware.** `driver.rs` calls `tick_ula()` and then
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

**C. `DELAY_TABLE_48K` is a hand-written 16-entry table where the hardware has
two counter bits.** The gate is `C2 | C3`. Encoding that as a literal invites
exactly the phase error it has: ours is rotated one pixel against the HDL's
window, which is invisible at T-state resolution and so survived undetected.

*Possible rework:* derive the window from the pixel counter's bits, so the
phase relationship is stated once and cannot drift.

**D. Three hand-written copies of the gate.** `ferranti-ula-6c001e`,
`sinclair-ula-7k010e` and `timex-scld` each carry their own boolean, and they
have already diverged — the 128K one contends from `/Border` while the 48K
contends from the video-fetch window, a difference recorded in a comment and
never reconciled. RULES.md #9 asks for one ULA implementation per variant; it
does not ask for the same contention expression written out three times. What
differs between variants is the *decode* — which ports, which pages — not the
topology.

## Sequencing, if the rework is taken

**A is the big one and should be attempted before Phase 3, not after.** If the
tick order is wrong, Phase 3 would be tuning a gate against a skewed clock and
the result would be another compensation rather than a fix. It is also the
riskiest change in the engine — it touches every machine that uses
`SpectrumDriver` — so it wants the survey baseline (34/70, recorded above) as
its gate, and a hard rule that a rework which does not raise it gets reverted.

B and C are small and can go with Phase 1. D is a cleanup and should go last,
once there is one correct expression worth having a single copy of.

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
