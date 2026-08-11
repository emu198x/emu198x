# Spectrum contention and the floating bus are co-tuned

**Status:** Open. Contention fix written, evidenced, and *deliberately not
enabled* pending a floating-bus derivation. I/O contention now derived
against FUSE and ruled out as the floating-bus blocker. 2026-08-10.

## The short version

Three constants in the Spectrum path had been calibrated against each
other while at least one of them was wrong:

1. the ULA contention gate,
2. the floating-bus sample lead (`SAMPLE_LEAD`),
3. the floating-bus byte pattern (`floating_bus_byte` / `FLOAT_START`).

Together they produced a machine that passed its floating-bus oracles.
Fixing (1) on its own — correctly, with the reference behind it — broke
the floating bus, and no value of (2) recovers both oracles. So the fix
is committed but unwired, and this record exists so the next attempt
starts from the evidence rather than from the symptom.

## What is definitely true

**The contention gate has a real defect.** It re-arms after a memory
access has committed, because it keys off `MREQ` being inactive *now*,
which is also true in the trailing T-state while the contended address is
still on the bus. Every M-cycle past `M1` is charged a second full
8-T-state rotation. `M1` escapes only because the cycle following its
access is the refresh, whose address is uncontended — which is exactly
why single-M-cycle instructions measured perfect and everything else
drifted.

Smith Chapter 18 (pp. 192-193, 197) gives the missing term: `MREQT23` is
`/MREQ` delayed until the clock rises at the end of `T1`, high for `T2`
and `T3` and **low for `T1`**, and Figure 18-15 draws the latch inside
the contention handler. It is ULA-internal, so no new pin is needed.

**The fix works on its own terms.** Wiring the latch in takes every case
of the per-instruction oracle to exact and the ZXSpectrum4.net timing
survey from **34/70 to 37/70**.

**The IO cycle and `IN` timing are correct.** Traced: four T-states, IORQ
across `T2` and the automatic wait state, released at `T3`. Scored
against the canonical model with the IO M-cycle correctly uncontended
(port `$FF`, address `$FFFF`, odd port outside the contended page):
5612 expected against 5611 measured — one instruction, the frame-boundary
straddler.

## What breaks, and why the sample lead cannot fix it

Enabling the latch makes floatspy fail: `IN() BYTE` reads 55 where it
should read 0, and the self-test stalls at `BURST_READ 37`. That is the
test program's own verdict on our machine, not a pixel comparison.

`SAMPLE_LEAD` was 3, and most of that was compensation rather than
hardware: a genuine hardware property would not move when an unrelated
bug is fixed. Sweeping it after the fix:

| lead | floatspy self-test | Float48K vs hardware 14338 |
|---|---|---|
| 0 | **pass** | 14340 — 2 late |
| 2 | fail (881px) | **14338 — exact** |
| 3 | fail (948px) | 14337 — 1 early |

No value satisfies both, and the best floating-bus outcome available
(lead 0) is *worse on Float48K than before the contention fix*. Something
else in the path is being masked.

## What the sample point should be, on the evidence

Four independent lines agree on **2**:

- **Z80 cycle structure** — data is latched at the edge ending `TW`;
  our `bus_request` resolves on the IORQ rising edge, two T-states earlier.
- **Float48K** reads 14338 at lead 2, which is Woody's measurement on
  real 48K hardware.
- **FUSE** — `readport` does `contend_early` (+1), `contend_late` (+2),
  then samples, i.e. IO-cycle start + 3, against our start + 1.
- **SpecIde** — samples at `ST_IORD_T3L_DATARD`, the 4th T-state of
  `T1 → T2 → TW → T3`.

Against that, floatspy and Spectron want 0. Note FUSE's own comment: it
deliberately uses 14338 rather than **Ramsoft's 14347**, because real
software (Arkanoid, Sidewize) only works with the former — and floatspy
*is* a Ramsoft program printing Ramsoft's convention. That does not make
floatspy worthless; its burst-read self-test is a genuine check. It does
mean this repository's designation of floatspy as *the* authoritative
floating-bus oracle deserves revisiting.

## Ruled out

- **Pattern phase.** Our data slots sit at group offsets 0-3 where FUSE's
  sit at 2-5, but the origins differ (`frame_tstate - float_start` versus
  a line start with `left_border` adjustments), so the relative structure
  is identical — four data slots in `data, attr, data+1, attr+1` order
  then four idle. Re-phasing it anyway shifted Float48K by one and made
  floatspy worse at *every* lead. Not the cause.
- **`IN` instruction timing.** Exact, as above.
- **Contention parity and the `MREQT23` clearing edge.** Both measured
  and eliminated; see `spectrum-accuracy-closure-campaign.md`.
- **I/O contention.** Derived against FUSE frame-wide and ruled out *for
  the floating bus specifically* — floatspy reads port `$00FF`, which FUSE
  does not contend at all. It is still broken for three of the four port
  classes; see the I/O section below.

## Why the fix is committed but unwired

The step-3 working contract says revert unless a change strictly
improves. Enabling the latch improves the timing survey and regresses a
floating-bus oracle: that is a trade, not an improvement. The latch is
therefore computed and maintained but not consulted by the gate, so
re-landing is a one-line change in each of the three contending ULAs
(`ferranti-ula-6c001e`, `sinclair-ula-7k010e`, `timex-scld`) rather than
a re-excavation. The differential regression tests are `#[ignore]`d with
a pointer here.

That ignore is itself a compromise worth naming, given this repository
already holds `a-gate-nobody-runs-is-a-silent-gate.md`. It is preferred
to deletion only because it keeps the re-landing path trivial, and it is
listed below so it is not forgotten.

## How this was allowed to happen

None of the gates in routine use could see it. The timing survey
improved, the catalogue passed 103/103, every unit test passed, and the
per-instruction oracle went exact. The floating-bus oracles were
`#[ignore]`d and absent from the nightly matrix, so floatspy only ran
because someone went looking for something unrelated. Two further gates
turned out to be unable to fail at all: Float48K's strict assertion was a
substring search over a transcript that prints every swept T-state, and
the catalogue's 103/103 was a re-capture verified against the same faulty
build — consistency, not correctness.

Fixed since: the floating-bus oracles now run nightly (`d7dec6c6`, needs
the `spectrum-system-tests` corpus uploaded) and the Float48K assertion
parses the probe's answer (`26e87d60`).

## Later the same day: two hypotheses closed, one contradiction opened

**`ORIGIN` is correct.** The `ad0e8c53` commit message guessed the stray
T-state lived there. It does not. Our first display fetch is scan 0,
pixel 4 — our-T 2 with two pixels per T-state — which maps to FUSE-T
14338, exactly FUSE's first floating-bus byte. The origin checks out
against our own geometry, so that lead is closed.

Float48K's 14337 against Woody's 14338 is therefore what
`FLOAT48K_EXPECTED_TSTATE` said before any of this: the probe detects the
first non-`$FF` **edge**, while Woody's figure is the **byte-on-bus**
instant. Different measurements, ±1 expected, not an error.

**The pattern phase was not what blocked the contention fix.** With the
pattern corrected against FUSE and the lead at 2, re-wiring the latch
still fails floatspy by the same 948 pixels. The two defects were
independent; fixing one did not unblock the other. Survey with the latch
re-wired re-confirmed at 37/70.

**And that leaves a contradiction worth stating sharply**, because it is
the next thing to explain rather than a vague disagreement:

- Spectron contends with a **T-state-indexed delay table**
  (`ContentionProvider.BuildContentionTable`, pattern `[6,5,4,3,2,1,0,0]`
  applied once per access). That is the canonical per-access model and it
  structurally cannot re-arm, so Spectron does **not** share our bug.
- With the latch **on**, our per-instruction oracle shows every case
  matching that same canonical model exactly.
- Yet floatspy matches us only with the latch **off** — only when we
  over-contend.

Two implementations of the same canonical model should agree. They do
not, which means something in how contention reaches the floating-bus
sample is still not understood. That is the question to answer, and it is
now well posed.

## RETRACTED: "the third error" (see the correction below)

**The 48K contention window opens three T-states late.**

FUSE's `spectrum_contend_delay_65432100` starts contending at frame
T-state **14335**. Our Ferranti gates contention on `e.video`, which
opens at `fetch_start` — scan 0, pixel 4 — i.e. our-T 2, which is
FUSE-T **14338**.

The confirmation is already in this tree. `sinclair-ula-7k010e` carries
the comment: *"Contention follows /Border rather than the later
video-fetch window: on the 128K it begins at T=14361, while the floating
bus does not expose the first fetch until T=14364."* That is the same
three-T-state gap. **The 128K models it; the 48K does not**, because the
Ferranti keys contention and the video fetch off the same flag.

**Why nothing caught it.** The contended window is 128 T-states, exactly
sixteen complete 8-T-state groups, so rotating its phase leaves the total
delay per line unchanged. The per-instruction oracle scores whole-frame
instruction counts and is therefore *structurally blind* to a window
phase error. It reported "exact" with the reference at 14336 and again at
14335 — two different models, same verdict.

The oracle's own reference was also wrong: `delay_at` started at 14336
and disagreed with FUSE at 21,504 of 69,888 T-states in a clean
one-T-state lag. Pinned to FUSE now by
`matches_fuse_contention_across_the_whole_frame`.

**Why this is the strong candidate for the contention / floating-bus
conflict.** A window-phase error changes precisely when contention bites
relative to the beam, which is what a cycle-exact `IN` probe like floatspy
measures, while leaving bulk instruction throughput — the timing survey —
untouched. That is the observed signature: survey improves with the latch
on, floatspy breaks, and no sample lead reconciles them.

**Next action.** ~~Give the 48K a contention window that opens three
T-states before the fetch window.~~ **Done, measured, and wrong — see
below.**

## Correction: the window was already right

The section above is retracted. The 48K contention window is **not** three
T-states late, and the evidence against it was already in hand when the
claim was made.

Re-scoring the engine against the FUSE-pinned canonical, with the
*existing* window and the latch on, produced exact agreement on `NOP`,
`INC BC`, `LD A,(HL)` and `LD BC,(nn)`. That is a direct measurement
saying the engine's window already matches FUSE. The "three T-states
late" claim came instead from mapping pixels to FUSE T-states by hand —
our-T 2 for the first fetch, therefore FUSE-T 14338 — and was acted on
despite the measurement contradicting it.

Implementing it made things worse, which is the appropriate outcome:

| state | survey | floatspy |
|---|---|---|
| latch off, existing window (committed) | 34/70 | pass |
| latch on, existing window | **37/70** | fail |
| latch on, border-based window | **35/70** | fail |

The window change cost two survey cases and shifted `LD BC,(nn)` from
exact to 0.7% out, while not helping floatspy at all. Reverted.

**What survives from that work:** the oracle's canonical reference *was*
wrong — `delay_at` opened at 14336 against FUSE's 14335, disagreeing at
21,504 T-states — and is now pinned frame-wide by
`matches_fuse_contention_across_the_whole_frame`. That is a real
improvement to the measuring instrument, and it is what allowed the
window claim to be tested and killed quickly.

**And the contradiction is now sharper.** With the latch on, our memory
contention matches FUSE's canonical model exactly for every instruction
tested. Spectron implements that same canonical model. Yet floatspy
passes on Spectron and fails on us. Memory contention is therefore
unlikely to be where the disagreement lives.

**The untested half is I/O contention.** `io_contention` still uses
`cpu_iorq || e.z80_iorq_prev` — the same one-half-cycle approximation
that was proved wrong for `MREQ`. Smith's Figure 18-15 draws *two* gated
D-latches, `MREQT23` and `IOREQTW3`; we approximate both. floatspy is an
I/O test hammering `IN A,(0FFh)`. That is the next thing to derive, and
it should be derived against FUSE's `ula_contend_port_early/late` the way
the floating-bus pattern was — not adjusted and re-measured.

## The I/O path, derived — and the lead it closes

The untested half is now tested. `io_contention_oracle.rs` transcribes
FUSE's `ula_contend_port_early`/`_late` whole, and scores the engine's
measured `IN A,(C)` cost against it at every arrival T-state in the frame,
for five port classes sharing one origin offset.

**I/O contention is not what breaks floatspy.** floatspy contains exactly
one I/O instruction. Disassembled from `floatspy.tap` at offset 5580:

```asm
01 FF 00    LD BC,$00FF
78          LD A,B        ; A = $00
DB FF       IN A,($FF)    ; port = $00FF
C9          RET
```

`IN A,(n)` takes the port's high byte from `A`, so the address is `$00FF` —
the ROM page, uncontended, odd. That is FUSE's `N:4` class: **no I/O
contention at all**, at any T-state. The engine already matches it exactly,
0 of 63,744 samples, and still matches exactly with the `MREQT23` latch
wired in. No change to the I/O gate can move floatspy's `IN` by one
T-state, because there is nothing there to change. (The tape's only other
`IN`-shaped bytes, `ED 40` at offset 2089, sit inside the ASCII string
`*** SELF TEST ***` and are never executed.)

So the `IOREQTW3` lead is closed for the floating bus. It was the strongest
remaining candidate and it is wrong, for a reason that could have been
established at any point by reading the test program's port.

**I/O contention is separately, genuinely broken.** At HEAD, against FUSE:

| port class | FUSE shape | wrong | engine mean | FUSE mean |
|---|---|---|---|---|
| `$40FE` contended, ULA | `C:1 C:3` | 8,769 / 52,692 | 34.39 | 22.60 |
| `$40FF` contended, odd | `C:1 C:1 C:1 C:1` | 9,684 / 52,692 | 34.39 | 27.69 |
| `$C0FE` uncontended, ULA | `N:1 C:3` | 12,288 / 57,600 | 22.67 | 21.78 |
| `$C0FF` uncontended, odd | `N:4` | 0 / 63,744 | 15.73 | 15.73 |
| `$00FF` floatspy | `N:4` | 0 / 63,744 | 15.73 | 15.73 |

The load-bearing row is stated in a form the fitted origin cannot reach.
`$40FE` and `$40FF` differ only in the bit the gate tests, and the engine
costs them the same **to within 0.00 T-states**, where FUSE separates them
by **5.10** — the ULA-port case is *cheaper*, because the ULA holds the bus
for three T-states in one go rather than contending four times. No origin
offset can create a distinction a gate does not test for.

**The page dependence the engine does show comes from the wrong place.**
`mem_contention` is `contended_addr && z80_clock_high && !cpu_mreq`. During
an I/O cycle `MREQ` is inactive and `cpu_addr` holds the *port*, so a port
in `$4000..$8000` trips **memory** contention. That is why the two
contended-page classes measure identically: the leak dominates, and the
even/odd term adds nothing on top of a gate already asserted. It is the
same defect family as the `MREQT23` bug — a gate keying on a signal being
inactive *now* rather than on the access it is meant to describe.

## Re-testing the latch, and a trap found while doing it

With `!cpu_mreq` swapped for `!e.mreq_t23`, total disagreement falls from
30,741 to 21,318 and `$C0FE` goes from 12,288 wrong to exact. That reads as
a clear improvement — and it is not safe to read it that way, because the
fitted origin moved from **+14335 to +14334** at the same time. The raster
did not move. A gate that decides one T-state later than the reference is
indistinguishable from an origin one T-state earlier, so the offset had
been quietly absorbing the change.

Held at a fixed origin (+14335, which is `FIRST_DISPLAY`, itself pinned to
FUSE), the latch reads the other way: `$C0FF` and `$00FF` go from **0
wrong to 18,432 wrong, every one of them exactly +1 T-state**. Those
classes have no I/O contention at all, so that T-state is the two contended
`M1` fetches. The latch over-charges them.

That is a mechanism for floatspy's failure that does not involve I/O
contention: every contended `M1` pair costs one T-state more, so floatspy's
`IN` lands at a different raster position and samples a different byte.

The harness therefore takes `EMU198X_IO_ORACLE_OFFSET` to pin the origin.
Any comparison between two gate configurations must use it. This is the
third time on this problem that a free parameter has absorbed a real error
— after `SAMPLE_LEAD` and after the oracle's own `delay_at` origin — and it
is worth naming as the recurring shape rather than three separate
surprises.

The latch remains unwired. The evidence for it is now more mixed than the
survey score suggested, not less.

## The origin, settled

The offset is no longer fitted. FUSE's frame T-state 0 **is** the
interrupt: `spectrum_frame()` subtracts a frame from `tstates` and
`z80_interrupt()` runs immediately after, holding `/INT` while
`tstates < interrupt_length`. The engine raises `int_active` at its own
T-state **55553**, and 69888 − 55553 = **14335**. That is a measurement of
an edge both implementations define and neither derives from contention.

Two further readings agree at the same anchor. The engine holds `/INT` for
exactly **32** T-states, which is `interrupt_length` for
`timings_frame_ferranti_5c_6c` in libspectrum's `timings.c`. And +14335
puts engine T-state 1 at FUSE's `top_left_pixel` of 14336, one T-state
after the contention window opens — where `FIRST_DISPLAY` already had it,
arrived at independently.

That also disposes of the third candidate. The retracted window section
argued +14336 from a hand mapping of pixels to T-states; at two pixels per
T-state that mapping carries a half-T-state ambiguity the interrupt edge
does not have.

`the_frame_origin_is_pinned_by_the_interrupt` gates it, and the
differential scores against `ORIGIN` rather than a fit. The fit is still
computed and printed only when it disagrees — a divergence now means the
gate's phase has moved against its own interrupt, which is a finding.

## The I/O fork, and how the HDL resolved it

Reading Chapter 18's prose on Figure 18-15 — "`/IWAIT` and its source NOR
gate are physically gone", the handler "used `/MWAIT` alone", and "all
contention … is processed through the single Memory Contention NOR-gate
fan-in" — suggested the 6C had *no* I/O contention path, and that a ULA
port outside `$4000..$7FFF` should not contend at all. That contradicts
FUSE, which gives it `N:1 C:3`, and it is not a small disagreement:
keyboard reads are `IN A,($FE)` with the half-row in `A`, so `$FEFE`,
`$BFFE` and friends all land in the disputed class.

Chapter 18 §7 flags its own limit here — the 6C topology "needs Smith's
accompanying HDL implementation at `opencores.org/projects/zx_ula`, not the
OCR text. The text on pp. 205–207 is partial." So the HDL was acquired
rather than the fork adjudicated. It is now at
`198x/emulators/zx-spectrum/zx_ula/` (Miguel Angel Rodriguez Jodar, Univ.
Seville, from Smith's book and the Harlequin), indexed there.

**The prose reading was wrong, and the HDL says so plainly.** Both the CPLD
and FPGA variants carry the same block:

```verilog
wire ioreq_n = a[0] | iorq_n;              // IORQ *and* an even port
wire Nor1 = (~(a[14] | ~ioreq_n))
          | (~(~a[15] | ~ioreq_n))
          | (~(hc[2] | hc[3]))
          | (~Border_n | ~ioreqtw3 | ~CPUClk | ~mreqt23);
wire Nor2 = (~(hc[2] | hc[3])) | ~Border_n | ~CPUClk | ioreq_n | ~ioreqtw3;
wire CLKContention = ~Nor1 | ~Nor2;
always @(posedge CPUClk) begin
  ioreqtw3 <= ioreq_n;
  mreqt23  <= mreq_n;
end
```

Negating `Nor1` gives `(A14 | IORQ) & (/A15 | IORQ) & (C2|C3) & /Border &
ioreqtw3 & CPUClk & mreqt23`. **The `IORQ` term short-circuits the address
decode.** When the ULA answers the port, both address conditions are
satisfied whatever the page — so a ULA port contends regardless of where
its address lands. `Nor2` is a second contention path for the same case.

What `ioreqtw3` does is *cut the contention short*: it latches `ioreq_n` on
the next `CPUClk` edge, after which both paths are disarmed. That is the
"cancellation of contention during legitimate I/O cycles" the prose
describes — not the absence of an I/O path, which is what it had been read
to mean.

All four classes then fall out, and they are FUSE's:

| class | mechanism | shape |
|---|---|---|
| `$40FE` contended, ULA | address decode at `T1`, then `IORQ`, then cancelled | `C:1 C:3` |
| `$40FF` contended, odd | `ioreq_n` never asserts; address decode armed all cycle | `C:1 C:1 C:1 C:1` |
| `$C0FE` uncontended, ULA | no `T1` decode; `IORQ` short-circuits, then cancelled | `N:1 C:3` |
| `$C0FF` uncontended, odd | neither path arms | `N:4` |

So the reference emulator and the gate-level source agree completely, and
the engine is wrong against both. Implement FUSE's table, by the HDL's
mechanism.

The HDL also latches `mreqt23` on `posedge CPUClk` exactly as
`UlaEngine::mreq_t23` does, and requires it high in `Nor1` — which is
evidence *for* the latch, against the +1 T-state regression measured above.
Those two have not been reconciled and that is the open thread.

## Attempting the gate fix, and what it exposed

Two attempts, both reverted. The tree is at HEAD; this records what they
established, because the second result is the useful one.

**Attempt 1 — the HDL expression, latch sampled directly.**
`!ioreq_tw3 && z80_clock_high && (ula_io || (contended_addr && !cpu_mreq))`,
with `ioreq_tw3` latched from `ula_io` on the rising clock edge alongside
`mreq_t23`.

The load-bearing defect moved: the contended-page pair gap went from
**0.00 to −6.39** against FUSE's **−6.53**. The engine separated `$40FE`
from `$40FF` for the first time, in the right direction and nearly the
right size. But `$C0FE` stopped contending altogether — 15.73 where FUSE
wants 18.52, 18,432 samples wrong — because the latch asserted half a
T-state early and cancelled the contention `T2` is supposed to charge.
Total disagreement 39,639 against HEAD's 30,741.

**Attempt 2 — the latch delayed by a half-cycle.** `IOREQTW3` is high for
`TW` and `T3` and low for `T2`; `/IORQ` falls *mid-*`T2`, after the clock
edge the HDL samples on, while our latch runs at the end of the tick when
the pin has already moved. Taking the pre-edge value should restore the
signal its name describes.

It produced numbers **identical to HEAD, to the digit**. That is the
finding.

**Why, and it is structural rather than a polarity slip.** Our gate is
level-held: contention stalls the CPU until the delay table frees it, and
while stalled `track_z80_clock` does not run, so neither latch clocks. A
single contention event therefore spans a whole 6-of-8 window — through
`TW` and `T3` and out the other side. By the time the latch can take,
`IORQ` has gone. There is never anything left for `IOREQTW3` to cancel.

FUSE, by contrast, charges *discrete* delays at four separate points and
re-reads the table at each. `C:1 C:3` versus `C:1 C:1 C:1 C:1` is a
statement about **how many separate times** the table is consulted — and a
model that absorbs one whole window per event cannot express the
difference, whatever the cancellation term does.

So the I/O shape is not reachable by editing the gate expression. It needs
the stall model to charge per-T-state rather than per-window, which is a
change to the same machinery `MREQT23` sits on — and that is very likely
the same reason the latch measures +1 T-state per contended `M1` pair while
the HDL says it should be required. **The two open threads are probably one
thread.**

Stopped there rather than trying a third variation on the gate: two
attempts had already produced one partial fix and one no-op, which is the
shape the cadence rule says to reset on.

## Correction: the stall model was never the problem

The section above concluded that our level-held stall cannot express
`C:1 C:3`, and proposed a fork over charging per-window versus
per-T-state. **That was wrong**, and the HDL says so in four lines:

```verilog
always @(posedge clk7) begin
    if (CPUClk && !CLKContention) CPUClk <= 0;
    else CPUClk <= 1;
end
```

`CPUClk` is *held high* while contention persists. The HDL is level-held
too. Our stall model is the same kind as the silicon's, RULES.md clock
rule 3 is not in question, and no re-architecture is implied.

That reduces the question to something mechanical: given identical pin
sequences, does our gate produce the same `CPUClk` waveform as the HDL's?

## The gate-level harness, and three Z80 defects

`crates/ferranti-ula-6c001e/tests/hdl_gate_reference.rs` transcribes the
HDL's contention block and runs it as an executable model — no ROM, no CPU,
no frame, at the half-cycle resolution the disagreement lives at.

A transcription is only another reading, which is what put two failed
attempts in the tree. What makes this one usable is an **independent
acceptance test**: it must reproduce FUSE's four-way table for every
arrival phase under a *single* rotation shared by all four classes. Until
it does, a mismatch means the transcription is wrong. Four classes with
four different shapes cannot be rotated into agreement by luck.

It failed three times, and **every failure was in the Z80's pins, not in
the ULA**:

1. **The CPU advanced on one clock edge instead of two**, making every
   T-state cost two. Mine, in the harness.
2. **`/IORQ` released a T-state early** — at the end of `TW` rather than
   the end of `T3`. Ours, in `zilog-z80`. With the port address still on
   the bus, `IOREQTW3` releases early and a contended port re-arms the
   address decode for the final T-state, collapsing `C:1 C:3` toward
   `C:1 C:1 C:1 C:1`.
3. **`/IORQ` asserted half a T-state early** — on `T2`'s edge rather than
   half a clock after the address is stable, which is Zilog's own wording.
   `IOREQTW3` then latches *before* the contention it is meant to allow has
   been charged, and a ULA port outside the contended page stops contending
   at all.

Defects 2 and 3 each name `uncontended, ULA` as the failing class — exactly
the class the engine gets wrong. Defect 2 is corroborated three ways: the
acceptance test, `reference/by-topic/cpu-z80/cpu-z80-reference.md` ("like
memory read", wait state inserted between `T2` and `T3`), and the Z80 bus
model paired with the HDL at `zx_ula/fpga_version/ula_test_for_ise_and_isim/
cpu.v`. Defect 3 is the acceptance test plus Zilog; note that `cpu.v`
*disagrees*, driving `/IORQ` across the whole of `T2` — but it is testbench
stimulus, not an authority on Z80 pin timing.

**So the contention gate was probably never the bug.** Two attempts to fix
it failed because the defect was upstream, in the CPU's pins.

## What is still not explained

Applying the fixes and measuring, each one trades one port-class pair
against the other, and a uniform **+1 T-state** survives every
configuration:

| configuration | contended pair gap | uncontended pair gap |
|---|---|---|
| HEAD | 0.00 (want −5.10) | 6.94 (want 6.05) |
| `/IORQ` held through `T3` + HDL gate + `MREQT23` | −8.01 (want −8.24) | 0.00 (want 2.73) |
| …and `/IORQ` asserted mid-`T2` | 0.00 (want −5.83) | **6.94 (want 6.87)** |

Each configuration nails one pair and loses the other, and in every one of
them the `N:4` classes — which have no I/O contention at all and are pure
`M1` memory contention — sit exactly **+1** high whenever `MREQT23` is
wired in. That +1 is the same residual that has blocked the latch from the
start, it is a *memory*-contention effect with no I/O in it, and it is now
the single unexplained quantity. Nothing was landed; the tree is at HEAD.

## The `+1` localised, and the gate's defects named

**The reference and FUSE agree on memory contention.** Eight back-to-back
`NOP`s out of contended RAM — pure `M1`, no I/O anywhere near it — through
the HDL model against FUSE's `contend_read(pc, 4)`: exact agreement at all
eight phases, and at the same rotation `(1,1)` the four-way I/O table
settled on. So the gate-level source and the reference emulator are
consistent across both cases under one alignment. The residual is ours.

**Driving the engine with the model's own pins names it exactly.**
`the_engine_gate_against_the_hdl_model` runs `FerrantiUla::tick` through
the same synthetic pin sequences the model uses, at the same half-cycle
resolution. Because the pins are *given* rather than produced by our Z80,
any divergence is the gate's alone — which is the entanglement that sank
both earlier attempts:

| case | engine against model |
|---|---|
| `NOP` ×2 — pure `M1` | **exact** |
| `$40FF` contended, odd | **exact** |
| `$C0FF` uncontended, odd | **exact** |
| `$40FE` contended, ULA | **uniformly +6 T-states** |
| `$C0FE` uncontended, ULA | **uniformly −1 T-state** |

Memory contention is right. Both non-ULA port classes are right. **Every
defect is in the ULA-port path**, and there are exactly two: a missing
`IOREQTW3` cancellation, worth a whole contention window (+6), and an I/O
contention window one T-state short (−1).

Only every second phase is scored. A Z80 T-state is two ULA ticks, so the
CPU always enters an M-cycle on the same parity; the odd column is a state
the real machine never occupies, and the engine's flat no-contention
reading there is an artefact of driving it into one.

**Two things this does *not* establish.** `NOP` ×2 is exact with the
*current* gate — `!cpu_mreq` live, no `MREQT23` — so for `M1` the latch and
the live pin coincide, and nothing here says the latch is needed. But the
re-arm bug it was written for was diagnosed on `LD A,(HL)` and
`LD BC,(nn)`, and this harness models only `M1` and `IN`; plain memory
read/write M-cycles are not covered yet. The `MREQT23` question is
therefore still open, just no longer in the way.

## Next

1. Extend the harness to memory read/write M-cycles. That is where
   `MREQT23` was diagnosed and the only remaining gap in coverage; until it
   is there, the latch cannot be settled either way.
2. Land the two gate defects with the two Z80 pin fixes, as one change.
   Each is well-evidenced and none can land alone: the pin fixes make the
   measured system worse while the gate miscounts the corrected window, and
   the gate fixes need correct pins to have anything right to count.
3. Only then the floating bus. It remains untouched by all of this —
   floatspy reads `$00FF`, which no I/O change can reach.
2. Reconcile `MREQT23` with the HDL. The HDL requires the latch; our
   measurement says wiring it costs one T-state per contended `M1` pair.
   One of the two is wrong, and the HDL now gives a gate-level model to
   diff the engine against rather than a survey score to trade off.
3. Find the third error. It is in the floating-bus path, it is not the
   sample lead, the pattern phase, or I/O contention, and it is masked by
   the contention bug. FUSE's `spectrum_unattached_port` is the reference
   implementation to derive against — read it as a whole model rather than
   comparing constants.
2. Establish whether floatspy's self-test passes on **real 48K
   hardware**. This is the single observation that would settle whether
   our burst-read failure is a defect or a convention disagreement, and
   it is worth more than further inference. An FPGA machine (Spectrum
   Next) will not answer it — the floating bus is precisely where
   reimplementations diverge.
3. Re-land the contention fix and the derived sample lead together.
4. Re-capture the Spectrum catalogue once, against code that is right on
   both axes. The 39 hashes captured on 2026-08-10 were discarded for
   exactly this reason.
