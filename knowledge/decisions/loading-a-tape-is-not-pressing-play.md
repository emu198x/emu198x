# Loading a tape is not pressing play

**Status:** adopted
**Context:** #1050, `runtime-sinclair-zx80`, `machine-sinclair-zx80`'s
`tape_load` suite

## Decision

`Zx80Runtime::load_media` puts the cassette in the deck and stops there. The
pulses reach the machine only on `MediaTransportAction::Start`, and come off
again on `Stop`. A script has to type `LOAD` — the `W` key — and *then* press
play.

## Why the deck has to be separate

The ZX80's loader will not decode anything until the line has been quiet for
a `$5712` countdown at `$0207`, and **any** high resets that countdown. Our
`.o` encoder supplies exactly that quiet run as the tape's lead-in.

So the lead-in has to arrive after `LOAD` is typed, not before. A runtime
that threads the tape at load time spends the whole lead-in during boot and
typing; the data pulses that follow then land inside the leader search,
resetting the countdown on every one of them, and the tape plays out without
a byte being decoded. The loader is then waiting for a signal that has
already gone, and it waits forever.

That failure is indistinguishable from a broken decoder from the outside:
the tape is consumed, RAM is untouched. The tell is the program counter —
stuck at `$0222`/`$022C` in the bit loop long after the tape has run out,
rather than back at `$0198` in the main loop.

## What this replaced

`load_media` used to thread the tape immediately, and its doc comment
claimed this was safe because "the ROM will not decode it until the user has
typed `LOAD`". That is true of the *decoding* and false of the *leader*, and
the machine-level `tape_load` tests had already recorded the correct order —
"type `LOAD` first, then start the tape" — in the same repository. The
runtime contradicted a rule its own test suite had proved. Nothing caught it
because no test exercised the runtime's media path; that is the gap the new
`tape_transport` suite closes.

## What it bought

Cross Chase's 8K build now loads and runs through the runtime, and its
title screen is pixel-identical to MAME 0.289's — 20,706 ink pixels, zero
differences, once each emulator's framebuffer origin is discounted. See
[`zx80-horizontal-position-is-a-fixed-offset.md`](zx80-horizontal-position-is-a-fixed-offset.md)
for the raster model underneath it.

## Drift triggers

Re-read this entry when you catch yourself writing:

- "load the tape and run" as one step
- "the tape can start playing at once"
- "the ROM ignores the tape until `LOAD` is typed, so the order is free"
- a new system's `load_media` that calls the machine's tape/disk insert
  directly rather than storing the image for transport
