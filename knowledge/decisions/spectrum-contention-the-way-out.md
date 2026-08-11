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
