# Z80 IRQ deadline and Spectrum integration validation

## CPU finding

Perfect Z80 revision `9b0d2e5e826c3a5fae3b5c6669bba1cd5d3b4217` samples IRQ
at the final T-state's rising edge, two half-cycles before the next instruction
boundary dispatches the response. A held IRQ arriving later is deferred. A
pulse covering that earlier instant is accepted even if released before the
boundary; a pulse wholly between sampling instants is not retained.

The candidate stores two previous IRQ levels alongside the boundary-pending
bit. It changes which level qualifies acceptance, retaining IFF/EI checks,
NMI priority and response lengths. This is level history, not an edge latch.

The full IM1 sweep agrees in **1,664 comparisons** over sixteen cases and four
pulse widths: ordinary instructions, HALT, all five selected block repeats,
EI;NOP, EI;HALT, EI;DI, EI;EI;NOP, and DI. The twelve-case pre-change baseline
has **129 disagreements in 976 comparisons**. The public 976-case regression
also checks snapshot continuation after both history updates. It fails on the
baseline at NOP arrival 2, width 3: the old core misses an accepted pulse.

## Spectrum integration resolution

The isolated baseline (`78f34c49`) passes Float48K at **14338** and Float128K
at **14364**. The CPU-only candidate (`202f1f13`) regressed them to 14337/14363.
The integrated correction restores both original hardware-derived targets.

Smith's *The ZX Spectrum ULA*, printed pp.124 and 132, gates fetches with C3.
VidEN delays the border gate, not the fetched byte by another character.
SpecIde revision `56bee623f18749d0d261d49c7dbdc2654d850bca` independently uses
fetch counters 8/10/12/14, bus exposure 8..15 and a wait mask at 3..14.
Its IRQ is evaluated before counter advance, and its I/O data before CPU latch.
These observations replace compensating offsets in the integrated machine:

- Sinclair fetches now use physical counters 8/10/12/14 and feed SLoad at 12/20.
- The wait mask uses the upcoming control phase; 128K contention uses physical
  counters 0..255, independently of video fetch and interrupt-relative timestamps.
- Sinclair IRQ generation precedes counter advance; 128K starts at counter 5.
- Floating I/O reads receive the live bus before each CPU tick, including after
  stalls. The trace records the completed read's actual value. No fitted read
  origin or future-data prediction remains in either Sinclair read path.

FUSE's first-data timestamps map to physical C8 (T4), giving test-only pattern
offsets 14334/14360. These are not IRQ origins. The HALT oracle now uses the
measured CPU sampling deadline and exact master ticks, rather than rounding
the IRQ edge to a T-state or treating FUSE's event loop as pin evidence.

Full Floatspy and HALT2INT match hardware-derived screens on both machines.
btime and ptime also match both hardware screen oracles. The 48K btime local
golden changes only 16 top-border pixels, after passing its independent screen
oracle. Contended floating-bus reads improve from 1,537 mismatches to **0 of
57,602**. All IN/OUT contention classes agree; memory contention retains its
existing **18 of 370,030** harness-tail residual and unchanged ceiling.
Whole-frame bus patterns agree on both models. New hermetic regressions cover
physical fetch slots and live I/O/trace values across changing data and stalls;
both fail against the isolated pre-change implementation.

Timex, Pentagon and Amstrad timing configurations retain their previous
fetch/IRQ ordering. The shared Sinclair 128K/+2 model retains its 36-T-state
pulse; SpecIde's different pulse end and distinct +2 start remain outside this
correction. Frame routing version advances to 5 for changed raster timing.

This supersedes the former
[boundary-sampling decision](../knowledge/decisions/zilog-z80-samples-int-at-the-instruction-boundary.md).
The CPC wording "at T4" did not establish arrival after its rising edge;
instruction-level reference event loops do not resolve that distinction.

## Snapshot compatibility and validation

The former boundary boolean retains its binary field position and byte width.
Values 0/1 retain its meaning with empty IRQ history; values 2–7 also encode
history. Legacy JSON booleans remain readable. Missing legacy history cannot be
reconstructed, so the first two ticks after restore assume IRQ was low. New
history tags need the new reader. Unknown states are rejected.

202 ordinary CPU tests, all 1,604,000 Harte vectors, FUSE's 1,350 exact plus six
pinned differences, six Rak 1.2a exercisers, and ordinary CPC machine tests pass.
The integrated Spectrum probes and all four HALT assertions pass.
All-target CPU and affected Spectrum/Timex Clippy and formatting pass.
Spectrum runtime and Timex ordinary tests pass; all six Rak tapes were rerun
on the integrated machine (240.76s). Earlier ZEX results were not rerun.
The unavailable eihalt TAP was not run.

Original adapters and dated measurements are retained privately in
`ops/experiments/z80-irq-cutoff/`; interpretation lives in
`reference/by-topic/cpu-z80/z80-irq-cutoff-evidence.md`. This is selected no-WAIT
NMOS die-derived evidence, not a CMOS, physical-chip, or exhaustive IRQ/NMI-race
claim. Independent timing sweeps for IM0/IM2 remain outside this experiment.
