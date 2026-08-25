# Decision: a run that paints nothing says so

**Date:** 2026-08-25
**Status:** Active. Governs how the headless runner reports a run that
completed without producing a picture.

## The problem

A headless run that produced a uniform black frame was indistinguishable
from one that produced a game:

```json
{"cart_loaded":true,"frames_run":3000,"observations":[],"time":179208000}
```

Exit 0, cart loaded, frames run, PNG written. Every signal said success.

This is not hypothetical. Two Atari 5200 cartridges were mapped to the
wrong layout on 2026-06-04 and stayed broken for eleven weeks; nothing
failed, and they were found by a human sweeping 88 titles and looking at
the images (#1171). Fixing that turned up three more of the same shape
(#1177), again only because someone swept for them. Eleven cartridge
machines share this harness, so whatever was true for the 5200 was
silently true for the rest.

## The decision

**Report a single-colour final frame as an observation, say when the
picture last changed, and do not fail the run.**

The check is machine-agnostic: decode each emitted frame to RGBA and ask
whether every pixel is the same. It needs no per-machine knowledge and
would have caught every case found so far.

The last frame alone is not enough, and neither is "did it ever paint".
Both were tried and both failed on contact:

- *Last frame only* flagged Stargate, whose attract sequence peaks at 16
  colours and merely happened to be mid-blank at frame 3,000. A
  diagnostic that fires on a working game is one people learn to ignore.
- *Ever painted* came back true for every Atari 5200 cartridge, working
  or not, because the BIOS logo paints before the cartridge gets a turn.
  Any machine with a boot screen defeats it the same way.

So the observation reports `last_painted_frame` against `frames_seen`
and lets the reader draw the line. Last painted at frame 264 of 3,000 is
a machine that died at the BIOS handoff; at 2,982 of 3,000 it is still
going. No threshold is baked in, because the right threshold depends on
what the run was for.

Reporting rather than failing is the load-bearing half. A blank frame is
legitimate — a machine mid-boot, a cart that wants more frames than the
run gave it, a program whose first act is to clear the screen. A runner
that exited non-zero on those would be worked around within a week, with
`|| true` or a lowered frame count, and the signal would be worth less
than nothing because it would then be routinely ignored. An observation
in the report is greppable by a sweep, ignorable by a test that knows
better, and carries no incentive to disable it.

```sh
jq '.observations[] | select(.kind == "blank_frame")'
```

## What this does not claim

It catches an *absent* picture, not a *wrong* one. A cart rendering
garbage, the wrong palette, or a corrupt display list all pass this
check. It is a floor.

It is also fooled by a picture that flickers. Missile Command draws on
roughly one frame in three, so any single-frame sample of it is a coin
toss and `last_painted_frame` reports it as healthy — which is
technically true and practically misleading. A sweep that wants a
human-meaningful answer should take the peak over a window of frames
rather than trusting one.

Stronger per-machine signals exist and are worth adding where the cost is
low — an ANTIC display-list pointer untouched since reset, a VDP name
table that stayed empty, a CPU that never left a 16-byte range. Each
needs its own rule per machine, which is why the machine-agnostic one
lands first rather than waiting for them.

## Consequences

- `CapturedFrame::uniform_colour`, `CapturedFrame::painted` and
  `HeadlessSession::blank_frame_observation` live in `emu198x-shell`;
  the 25 script runners each call the last one once when assembling
  their report.
- `ScriptObservation::BlankFrame` carries the colour, the frame
  dimensions — so a degenerate zero-sized frame is distinguishable from
  a black one — and `frames_seen` / `last_painted_frame`.
- A frame that cannot be decoded reports nothing rather than erroring: a
  diagnostic must never be the thing that fails a run.
