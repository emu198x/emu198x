# Decision: FUSE governs the contended window

**Date:** 2026-08-11
**Status:** ACTIVE
**Applies to:** every Spectrum-family contention path — `ferranti-ula-6c001e`,
`sinclair-ula-7k010e`, `timex-scld`, and the delay tables in
`common-sinclair-zx-spectrum`

## The question

Two authorities describe when the ULA contends, and they disagree.

- **FUSE** — `spectrum_contend_delay_65432100` — opens the contended window at
  frame T-state **14335**.
- **The gate-level source** — Chris Smith's ULA reconstructed in Verilog at
  `opencores.org/projects/zx_ula`, vendored at
  `198x/emulators/zx-spectrum/zx_ula/` — opens it at **14338**.

Otherwise they are identical: same shape, same duty, same 8-T-state pattern.
Which one does Emu198x implement?

## What was measured

Both sides anchor to the same event, and **neither anchor is fitted**. FUSE's
frame T-state 0 *is* the interrupt: `spectrum_frame()` subtracts a frame from
`tstates` and `z80_interrupt()` runs immediately after. The HDL asserts
`msk_int_n` at `vc == 248, hc == 0`, and `(312 − 248) × 448 = 28672` `clk7`
cycles later — **14336 T-states** — its display begins, which is exactly
libspectrum's `top_left_pixel` for the Ferranti 5C/6C. The two geometries agree
about where the display sits relative to the interrupt, so the mapping between
them is forced.

Scored that way, frame-wide (`ferranti-ula-6c001e/tests/hdl_vs_fuse_anchored.rs`):

| offset applied to the HDL | mismatched T-states of 69,888 |
|---|---|
| 0 | 12,672 |
| **+3** | **0** |

And the transcription is not the weak link: the **actual Verilog** was
simulated under Icarus (`tests/verilog/tb_window.v`) and stalls the CPU clock
first at T-state **14338** after `/INT`, then every 8. The displacement is real
behaviour of the gate-level source, not a misreading of it.

Three is not a new number. It was derived by hand early in
[`spectrum-contention-vs-floating-bus.md`](spectrum-contention-vs-floating-bus.md)
— 14335 against 14338 — and retracted for costing two timing-survey cases.
`sinclair-ula-7k010e` carries the same gap in a comment: the 128K contends from
T=14361 while the first fetch is at T=14364. That work was right about the gap
and wrong about what to do with it.

## The decision

**Implement FUSE's window. Record the three-T-state divergence from the
gate-level source as deliberate.**

Three reasons, in order of weight:

1. **FUSE is validated against real software, and the divergence is exactly the
   kind software notices.** FUSE's own source comments cite Arkanoid and
   Sidewize as programs that only work with its timing. A contended-window
   position is not an abstract preference — it is what makes or breaks
   multicolour effects and cycle-counted loaders. We ship an emulator for
   running period software.
2. **RULES.md #32 nominates the reference emulator as the authority for timing
   work.** It is a prerequisite, not a cross-check. The rule exists because
   reasoning from the spec alone has burned this project repeatedly, and it
   does not carve out an exception for a gate-level source.
3. **The HDL is a reconstruction, not the die.** `zx_ula` is Miguel Angel
   Rodriguez Jodar's Verilog, built from Smith's book and the Harlequin clone.
   It is the best gate-level account available and it is still a
   reimplementation. Smith's Chapter 18 §7 says the 6C001 topology in the book
   is partial; the HDL is how that gap was filled, not an independent witness
   to the silicon.

## What this costs: the HDL gate does not land

The gate change derived in
[`spectrum-contention-vs-floating-bus.md`](spectrum-contention-vs-floating-bus.md)
— `MREQT23` armed, `IOREQTW3` cancelling, the folded `Nor1`/`Nor2` expression —
makes the engine match the HDL exactly, half-cycle for half-cycle on the real
machine. Under this decision it **does not land at all**.

The first draft of this record said it could land paired with a compensating
window shift, on the reasoning that the two models differ only by three
T-states. **That was wrong, and measuring it is what showed it.** With the gate
in, sweeping the window phase across all sixteen positions against the
frame-wide differential:

| window rotation | disagreeing samples of 303,363 |
|---|---|
| 0 | 82,755 |
| **1 or 2 (best)** | **44,355** |
| 3–15 | 63,360 – 86,403 |

against **30,741** for the shipped gate. No phase reconciles them; the best is
worse than doing nothing.

The reason is that the anchored `+3` result compares *windows* — where
contention is permitted — and window agreement is necessary but not
sufficient for equal costs. The HDL gate also differs in **when inside an
M-cycle** it charges: `MREQT23` arms at `T1`, while FUSE charges once at the
M-cycle start with the table sampled there. That is a structural difference in
the contention model, not a phase, and no rotation converts one into the other.

So the shipped gate stays: `contended_addr && z80_clock_high && !cpu_mreq`,
with both latches computed and maintained but not consulted. It is a coarser
model than the silicon's and it reproduces FUSE, which is what this decision
asks for.

## What would reopen this

- **A measurement on real hardware.** This is the only thing that can arbitrate
  between the two, because it is a question about silicon rather than about
  models. Not currently available.
- **A period program whose visible output distinguishes a 14335 window from a
  14338 one.** Cheaper than a logic analyser and nearly as decisive. Likely a
  border-timing or multicolour demo; the floating-bus work is the most likely
  place to surface one.
- **An independent gate-level account** — a second reconstruction, or a die
  shot — agreeing with `zx_ula` against FUSE. That would make the divergence a
  known FUSE bug rather than a choice, and change what we implement.

Until one of those arrives, FUSE governs and the divergence is recorded rather
than resolved.

## Drift triggers

Re-read this entry if you find yourself:

- moving `FIRST_DISPLAY`, `DELAY_TABLE_48K`, or any contention-window constant
  "to match the hardware reference";
- treating a disagreement with `zx_ula` as a bug in our engine;
- landing the `MREQT23` / `IOREQTW3` gate change on the grounds that it matches
  the gate-level source — it does, and it still moves the engine away from
  FUSE, with no window shift able to compensate;
- adding a rotation or offset search to reconcile two timing models. Three
  separate errors here have been hidden by a fitted alignment —
  `SAMPLE_LEAD`, the oracle's `delay_at` origin, and the rotation search that
  concealed this very gap. Anchor to the interrupt instead; both models define
  it.
