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
