# Spectrum contention and the floating bus are co-tuned

**Status:** Open. Contention fix written, evidenced, and *deliberately not
enabled* pending a floating-bus derivation. 2026-08-10.

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

## Next

1. Find the third error. It is in the floating-bus path, it is not the
   sample lead or the pattern phase, and it is masked by the contention
   bug. FUSE's `spectrum_unattached_port` is the reference implementation
   to derive against — read it as a whole model rather than comparing
   constants.
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
