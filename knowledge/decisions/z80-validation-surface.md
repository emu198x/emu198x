# What actually validates the Z80, and what only appears to

**Status:** Open survey, taken 2026-08-13 while looking for a second Z80
machine to prove the interrupt-sampling change in
[`zilog-z80-samples-int-at-the-instruction-boundary.md`](zilog-z80-samples-int-at-the-instruction-boundary.md).

`zilog-z80` is shared by more than forty machine crates. Before leaning
on it elsewhere it is worth writing down which gates are real, which are
silent, and what is missing — because three of the four things that
looked like Z80 validation turned out not to be.

## What genuinely validates it

| gate | status | notes |
|---|---|---|
| ZEXDOC / ZEXALL | **pass** | ~840 s. Corpus at `198x/assets/test-suites/zex`. |
| Tom Harte single-step | **pass** | `198x/assets/test-suites/processor-tests/z80/v1`. |
| `zilog-z80` unit suite | **pass** | 61 tests, pin- and phase-level. |
| ZXSpectrum4.net timing survey | **pass**, 0 of 69 | Machine-level, interrupt-sensitive. |
| `halt2int128` | **pass** | 128K, genuinely runs and asserts its on-screen verdict. |
| FUSE Z80 reference suite | **pass**, 1351/1356 | Was silent, then wrong; see below. Corpus vendored with FUSE itself. |

## What is silent, and why

**The FUSE Z80 reference suite was never running, and when it did it was
lying.** Two separate faults, and neither was in the CPU.

*It could not find its corpus.* `find_fuse_z80_tests_dir` looked only at
`test-data/fuse/z80`, which is not present, so the test panicked on a
missing fixture and was `#[ignore]`d out of sight. There was nothing to
download: the vendored FUSE 1.7.0 source carries the corpus it is the
reference for, at `198x/emulators/zx-spectrum/fuse-1.7.0/z80/tests`, and
it is **byte-identical** to the copy in the frozen `Emu198x-Older`
archive. Wired in now, at both a normal checkout's depth and a
`git worktree`'s.

*Then it reported 830 of 1356 failing, and that was the harness.* Every
failure had one shape —

```text
01: events: first mismatch at event 2: expected [4 MC 0001], got [4 MC 0000]
```

— and the temptation was to read it as FUSE being less precise than a
half-cycle core. It is the opposite. `record_step_start_events` runs
*before* the tick it describes, and read the address from `z80.addr`;
`present_step_signals` drives each address *during* the cycle's `T1`↑, by
documented design. So the harness logged every non-`M1` access with the
address the previous M-cycle left on the bus. The `M1` arm read
`regs.pc` instead of the bus and was always right, which is exactly why
only the non-`M1` accesses failed — the discriminating detail, and it
points at the harness rather than the core.

Measured directly: on `LD BC,nn` the tick at `hc = 8` (T-state 4) takes
the bus from `$0000` to `$0001`, so the address bus carries `$0001` for
the whole of `T1` — which is what FUSE expects and what the silicon does.
Sampling at `T1Fall` instead of `T1Rise` names the same T-state, so no
timestamp moves.

| harness | exact | unexpected |
|---|---|---|
| as found | 525 / 1356 | 830 |
| memory arms sampled at `T1Fall` | 1298 / 1356 | 56 |
| I/O arms too | **1351 / 1356** | **0** |

The last 56 were the same bug in the `IoRead`/`IoWrite` arms; with a
stale port the contention branches also keyed off the wrong page, which
is why whole `PC` events were missing rather than merely mis-addressed.
The five remaining disagreements are exactly the documented allowlist
(`76`, and the four block-repeat undocumented-flag cases).

**So: no, FUSE is not less accurate than we are here.** The engine agreed
with it all along. That question is worth asking each time — we have
already declined Zilog's literal wording in favour of FUSE once, in
[`zilog-z80-samples-int-at-the-instruction-boundary.md`](zilog-z80-samples-int-at-the-instruction-boundary.md)
— but on this evidence the reference was right and our instrument was
wrong, for the sixth time in this campaign.

The suite stays `#[ignore]`d, because the corpus lives in the umbrella
repo and CI checks out `emu198x` alone. That is now an honest reason
rather than a missing-fixture panic, and running it locally is a single
command with no environment variables.

**The Super HALT Invaders Test never runs.** `super_halt_invaders_runs_to_completion`
boots the tape, presses `ENTER` once for the 128K firmware's Tape Loader
menu, and screenshots the result against a golden. But the program loads
to its own title screen asking the user to "press ENTER to start", and
that key is never sent — so the golden is a picture of a menu, and the
5,936-pixel diff it has been failing on is title-screen animation. A
HALT-and-interrupt suite that never reaches a `HALT`.

Sending a second `ENTER` advances it (the title text and a "welcome to
SUPER HALT" line appear) but does not start the test, so it needs real
input driving rather than one more keypress. Left alone rather than
half-fixed.

**`tape_smoke` degrades to a pass when its oracle is absent.** The
Spectron screen comparison is skipped entirely when
`EMU198X_SPECTRON_RESULTS_DIR` is unset — "Unset → that extra check is
skipped" — and the test then reports `ok`. This caused a false "floatspy
now passes" claim during the contention campaign; see
[`spectrum-contention-vs-floating-bus.md`](spectrum-contention-vs-floating-bus.md).

## What is missing for a cross-machine proof

The interrupt-sampling change is adopted on Spectrum evidence alone, and
the Spectrum cannot vary the axis that matters: its `/INT` assertion
instant is fixed by the raster. A second machine whose interrupt comes
from somewhere else would settle it.

The workspace has the machines — MSX, Sega Master System, SG-1000,
ColecoVision, Sord M5, Memotech MTX, Tatung Einstein, SVI-328, and a
`zilog-z80-ctc` crate for the CTC-vectored cases. What it does not have:

- **No Z80 machine-level test suites.** `198x/assets/test-suites` holds
  `zex` and `processor-tests` (both CPU-only, no interrupts) plus 6502,
  C64, Game Boy, NES and m68k material. Nothing for SMS, MSX or CPC.
- **No reference emulator for the Master System.**
  `198x/emulators/sega-master-system/` contains only an `INDEX.md`.
  [`../../RULES.md`](../../RULES.md) rule 32 makes vendoring one a
  *prerequisite* for timing work there, not an optional cross-check.
  `emulators/multi-system/` does carry `ares`, `genesis-plus-gx` and
  `mame`, all of which cover SMS, so the gap is a dedicated readable
  reference rather than any reference at all.

**Suggested order**, cheapest first: SMS or SG-1000, because
`genesis-plus-gx` is already vendored and its VDP raises `/INT` from a
line counter — a genuinely different assertion source from a ULA tied to
the beam. The CTC machines (Einstein, MTX) are the strongest test of IM 2
with real vectors, but need a reference emulator vendored first.

## The cross-machine look found something else first

Going to the Master System to *prove* the interrupt-sampling change
instead found two defects in how the machine drives the shared CPU. Both
are now fixed, and neither had anything to do with interrupts sampling.

**Correction to the section above:** this record claimed there is no
vendored reference emulator for the Master System. That is wrong.
`198x/emulators/sega-master-system/INDEX.md` designates
`multi-system/genesis-plus-gx` as the canonical SMS reference and lists
the relevant files; `ares` and `mame` cover it too. Three independent
references, already present. RULES.md rule 32 is satisfied — the
directory looked empty because the reference lives in `multi-system/`,
which the INDEX explains.

**The Z80 ran at half speed.** `Z80::tick` advances one half-cycle —
`T1Rise`, then `T1Fall`. `Sms::tick_tstate` called it **once** while
incrementing `cpu_tstates` by one, against constants that are the real
figures (228 T-states per scanline, 342 dots, the correct 3:2 dot ratio).
So every instruction cost twice the T-states it should, and the machine
executed half the work per frame its own budget allows. Measured: a `NOP`
cost **8.000** T-states against Zilog's 4.

The SG-1000 carried it identically, down to a comment reading "CPU
half-cycle tick" directly above `cpu_tstates += 1` — the author knew
which it was and counted the other.

Nothing caught it. Both suites passed throughout: boot tests reach their
screens either way, and a *uniform* halving of CPU speed is invisible to
anything that never compares an instruction's cost against a known
figure. `crates/machine-sega-{master-system,sg-1000}/tests/cpu_rate.rs`
now does exactly that, on `NOP` — the least disputed number in the
instruction set, with no memory operand, index prefix or conditional
path. If `NOP` is right the clock division is right; if it is wrong,
nothing else can be.

**And the same `/INT` ordering defect the Spectrum had.** Both machines
fed `cpu.irq = vdp.interrupt` *after* `cpu.tick()`, so the CPU read the
VDP's line one tick stale — the host-contract half of
[`zilog-z80-samples-int-at-the-instruction-boundary.md`](zilog-z80-samples-int-at-the-instruction-boundary.md).
Now fed before the tick, inside the two-half-cycle loop, with the VDP
phase accumulator re-denominated to 4 so its 3:2 dot ratio is unchanged
but interleaved twice as finely.

**The pattern is wider, and is not yet measured.** Every Z80 machine in
the workspace calls `cpu.tick()` once per its tick function, and most
feed `.irq` after it:

| machine | `.irq` fed | verified |
|---|---|---|
| Master System, SG-1000 | after → **fixed** | **measured** (8.000 → 4.000 T/`NOP`) |
| MSX, ColecoVision, Sord M5, Tatung Einstein, SVI-328 | after tick | inspection only |
| Memotech MTX, Jupiter Ace | before tick | inspection only |
| Aquarius, ZX81 | no `.irq` found | inspection only |

Only the two Sega machines are measured. A single `tick()` per tick
function is **not** by itself proof of half speed — the Spectrum calls
`tick_cpu_and_bus` once per scheduled edge and twice per T-state, which
is correct. Each machine needs its own `cpu_rate` probe before any claim
is made about it, and that is the next piece of work.

**Consequence for the interrupt proof.** The cross-machine review this
survey was opened for cannot start on the Sega machines until their
timing is re-validated against `genesis-plus-gx` at the corrected CPU
rate. Fixing the clock is a prerequisite, not the proof: a machine
running its CPU at half speed could not have told us anything about a
half-T-state sampling instant.

## The sweep: nine of eleven Z80 machines run the CPU at half speed

Measured, not inferred. Each machine was driven through its own
`Z80Stepper::step_tick` on a ROM of `NOP`s, counting ticks per retired
instruction. A Zilog `NOP` is four T-states, so a machine whose
`step_tick` is one T-state should read **4.000**.

| machine | step_ticks per `NOP` | verdict |
|---|---|---|
| Jupiter Ace | **4.000** | correct |
| Master System, SG-1000 | 8.000 → **4.000** | fixed, this campaign |
| MSX | 8.000 | half speed |
| ColecoVision | 8.000 | half speed |
| Sord M5 | 8.000 | half speed |
| Spectravideo SVI-328 | 8.000 | half speed |
| Tatung Einstein | 8.000 | half speed |
| Mattel Aquarius | 8.000 | half speed |
| Memotech MTX | 8.000 | half speed |
| ZX81 | 8.000 | half speed |
| ZX80 | 8.000 | half speed |

The Jupiter Ace reading 4.000 is what makes the rest a measurement rather
than a broken probe: one machine in the fleet does it correctly, through
the same harness, on the same instruction.

**And it had already been found.** `machine-jupiter-ace`'s `tick_tstate`
carries this comment, in the tree, today:

> The `zilog-z80` core is a half-cycle state machine: it needs two ticks
> per T-state (the Spectrum drives it the same way — see
> `common-sinclair-zx-spectrum` `tick_one_halfcycle`). Driving it once
> per T-state under-clocked the CPU 2× and meant the IRQ was never
> sampled at an instruction boundary, so the Forth ROM spun forever
> waiting for its 50 Hz frame interrupt.

So this is not a new discovery. It is the *same* defect, diagnosed
correctly once, fixed on the machine where it happened to stop the ROM
booting, and never propagated to the other nine. The Sega pair were found
independently in this campaign and fixed the same way.

Note the second half of that comment: under-clocking and the interrupt
sampling instant are the same failure on these machines, because a CPU
ticked once per T-state never reaches the boundary the sample is taken
at. That is why the Ace's Forth ROM hung rather than merely running slow.

**Why nothing caught it.** A *uniform* halving of CPU speed is invisible
to every gate these machines have: boot tests reach their screens either
way, golden framebuffers were captured under the same halving, and
nothing compared an instruction's cost against a known figure.
`crates/machine-sega-{master-system,sg-1000}/tests/cpu_rate.rs` is the
shape that catches it, and every machine above wants one.

**The durable fix is a layer, not nine patches.** The Spectrum family
does not have this bug because `SpectrumDriver::tick_one_halfcycle` gets
the cadence right once for seven machines. These eleven each hand-roll
their loop, which is exactly the case
[`../../RULES.md`](../../RULES.md) rule 30 covers — promote cross-machine
functionality to the highest layer that fits. The per-machine work is
mechanical (wrap the CPU tick in `for _ in 0..2`, feed `.irq` before the
tick, leave the once-per-T-state chip ticks where they are, as the Ace
does); the question worth answering first is whether these machines
should share a driver trait rather than each rediscovering the cadence.

**Order of work.** Fix the nine with a `cpu_rate` gate each — the Ace is
the template and the Sega pair are worked examples — then revisit the
shared-driver question with eleven known-correct loops to generalise
from. Only then is the cross-machine interrupt proof meaningful: none of
these machines could have said anything useful about a half-T-state
sampling instant while running at half speed.
