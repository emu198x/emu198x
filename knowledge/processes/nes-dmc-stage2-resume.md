# Resume: NES stage 2 — `sprdma_and_dmc_dma`

Where to pick up the DMC fix, what is already proven, and which experiment to
run first. Companion to
[`nes-accuracy-closure-campaign.md`](../decisions/nes-accuracy-closure-campaign.md),
which holds the full evidence trail. Baseline commit: `6489717f`.

## The goal

`sprdma_and_dmc_dma.nes` and `sprdma_and_dmc_dma_512.nes` fail with `#01`.
Expected values are recorded in
[`test-data/nintendo/nes/blargg-survey/sprdma-dmc-dma-expected.tsv`](../../test-data/nintendo/nes/blargg-survey/sprdma-dmc-dma-expected.tsv).
Emu198x is `+1` cycle at exactly the even alignments, never `-1`, never more.

## The model (measured, not assumed)

The DMC reaches a sample fetch by **two independent paths**. Emu198x collapses
them into one, and that is the defect.

**Path A — steady state, timer-driven.** The output unit consumes the sample
buffer when `bits_remaining` hits 0, and requests the next byte *there*. Fetches
therefore inherit the timer's cadence: 428 period + 4 DMA = **432 cycles**.

**Path B — cold start, `$4015`-driven.** With the channel idle there is no
buffer consumption to hang the request on, so hardware synthesises one after
**2 or 3 cycles chosen by CPU get/put parity** — Mesen2's `_transferStartDelay`.

⚠ **The ROM's alignment sweep rides path A, not path B.** Its `$4015` cadence is
433 cycles against the timer's 432, so each iteration the write lands one cycle
earlier relative to consumption and the re-arm-to-fetch latency walks
**12, 11, 10, 9**. Two cadences used as a vernier. Confirmed by state trace: every
432-cadence fetch occurs at an identical timer phase (`timer=49, bits=8`) with
`transferStartDelay == 0`.

Path B was observed firing exactly once in that window, at `c=1237621`:
`tsd` counted 1 → 0 and the fetch followed ~6 cycles after the write, which is
the 2–3 delay plus 3–4 of DMA.

## The bug

**Emu198x's `$4015` requests a fetch immediately**, pre-empting path A. The fetch
then rides the *write's* cadence instead of the timer's, so there is no drift, no
vernier, and a flat table where hardware alternates.

A second, independent defect in `Dmc::clock_output`, confirmed against Mesen: the
request sits *after* the `if/else`, so it also fires on the silence path where
the buffer was already empty and hardware requests nothing. It belongs inside the
buffer-consumed branch, guarded by `transfer_start_delay == 0`.

## Run this first

One minimal change, then measure — do not write the delay half yet:

> `$4015` enable sets `current_address` and `bytes_remaining` and **nothing
> else** — no `dma_pending`, no delay. The request comes only from
> `clock_output`'s buffer-consumed branch.

Then run `probe_dmc_rearm_vs_fetch`. **Success looks like the latency walking
12, 11, 10, 9.** Anything flat, or in the 4–6 range, falsifies the model in one
run and should stop the attempt.

Only once the drift reproduces, add `transfer_start_delay` (2 or 3 by parity) for
the idle-enable case, with the `== 0` guard in `clock_output` as the interlock.
The APU has no get/put phase of its own; the machine owns it as `cpu_cycle_count`
and must publish it.

⚠ **Two attempts have already failed the same way**, both rolled back: each made
`transfer_start_delay` the trigger, which deletes path A. Latency collapsed to
4–5 and the ROM stopped settling. If a third attempt starts by reaching for the
delay, it is repeating them.

## Open question

At `c=1228891` the channel is idle (`bytesRemaining == 0`), so Mesen's
`else if(_bytesRemaining == 0)` branch should arm the delay — yet the traced
`transferStartDelay` stays 0. Either the poll misses a 2–3 cycle transient
between reads, or that branch carries a guard not yet read. Resolve before
writing the delay half. It does not block the minimal change above.

## Tooling

All committed and reusable.

| | |
|---|---|
| [`tools/mesen-nes-cross-check/`](../../tools/mesen-nes-cross-check/) | Mesen2 oracle harness; see its README for build steps (`make core -j8`, SDL2, no .NET) |
| `dma-trace.lua` | per-cycle DMA bus-op trace, episode-split |
| `dmc-fetch-cycles.lua` | CPU cycle of every DMC sample fetch (address is a constant `$E3C0`) |
| `dmc-transfer-delay.lua` | `transferStartDelay` / timer / bits state around the sweep window |
| `probe_dmc_rearm_vs_fetch` | **the gate for the experiment above** |
| `probe_dma_episodes_around_transfers` | every DMA episode with extent and kind |
| `probe_sprdma_and_dmc_dma` | the ROM's own clock table |
| `probe_dmc_tests_text` | records why `dmc_tests` cannot gate (below) |

Lua memory callbacks receive `(address, value)` only — no operation type. State
keys are flat and dotted: `emu.getState()["apu.dmc.bytesRemaining"]`. The script
log is a 500-row ring buffer, so pack output or log only changes.

## Dead ends — do not retry

| Hypothesis | Result |
|---|---|
| OAM DMA length wrong | 513/514 by alignment, correct |
| DMC DMA length wrong | 3/4 by alignment, correct |
| Get/put phase inverted vs Mesen | ROM never settles |
| The OAM transfer itself | all 16 match Mesen address-for-address |
| Inter-transfer intervals are the cause | an effect — exactly 1.00 frames, a vblank sync miss |
| DMC-only DMA length | both emulators take 4 there (432 spacing) |
| Sample fetch as a level condition per NESdev | no change: same 30 fetches, same gaps, same table |
| `transfer_start_delay` as the fetch trigger | twice; latency 4–5, ROM hangs |

`dmc_tests/latency.nes` looks like the natural gate but **cannot be one**: those
four ROMs carry no `$6000` protocol (Mesen confirms) and hold no ASCII in
nametable RAM — they draw tile indices against a CHR font. Gating them needs
tile-index decoding or framebuffer comparison, which is stage 5 work.

## Working practice

⚠ **Another session works in this tree concurrently** (Spectrum, `nec-upd765a`,
`runtime-sinclair-zx-spectrum`). Never `git add -A`; stage explicit paths, and
`git commit -- <paths>`. A push here can also carry that session's unpushed
commits — check `git log origin/main..HEAD` before pushing.

Commits are conventional (`feat:`/`fix:` gate release-plz). Signing needs 1Password
unlocked; if it fails, ask rather than bypass. `cargo fmt` is a commit gate.
