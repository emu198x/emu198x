# Closed: NES stage 2 — `sprdma_and_dmc_dma`

✅ **Fixed at `1aa4eb67`.** Both ROMs report Passed, all 32 alignment values
match the Mesen2 oracle, and the gated sweep declares them at 137 pass / 3 fail
/ 15 visual.

This was a resume note for work in progress. The evidence and the durable
hardware model now live in
[`nes-accuracy-closure-campaign.md`](../decisions/nes-accuracy-closure-campaign.md)
— see *Stage 2 outcome*. Read that, not this.

## What the defect was

A `$4015` write re-arming an idle DMC requested its first sample fetch on the
write itself. Hardware has no buffer-consumption event to hang that request on,
so it synthesises one **2 or 3 cycles later, chosen by CPU get/put parity**.

Requesting on the write made every fetch ride the write's cadence (433 cycles in
the ROM's sweep) instead of the timer's (432). The two are meant to beat against
each other as a vernier, walking the re-arm-to-fetch latency down a cycle per
iteration; collapsing them produced a flat alignment table where hardware
alternates.

## The method that worked, after two failures

Two earlier attempts made the delay the fetch *trigger*. That deletes the
timer-driven path along with the defect — latency collapsed to 4–5 cycles and
the ROM stopped settling. Both were rolled back.

What worked was **removing behaviour first and measuring before adding any**:
strip the request from `$4015` entirely, change nothing else, and check the
prediction. The latency walked 14, 13, 12, 11, 10, 9 on the first run, and the
whole 16-alignment table snapped into Mesen's shape at a uniform −3 — an
alignment-independent constant, which is what a missing fixed delay looks like.
Only then did the delay go in, and it supplied exactly those 3 cycles.

Had the delay gone in first, its uniform +3 would have been invisible beneath
the still-flat table, and the attempt would have looked like the other two.

## Still open (stage 5, not stage 2)

`dmc_tests/latency.nes` remains ungateable, and the Spectrum ROM-font decoder
added at `da0f91e7` does not help: those ROMs declare **0 KB of CHR**, so the
font is uploaded to CHR RAM at runtime, and their PRG holds no ASCII at all.
Recovering their text is OCR against an unknown font, not decoding a known one.
Framebuffer comparison against Mesen2 is the cheaper route. Details in the
campaign record under stage 5.

## Tooling

All committed and reusable — see
[`tools/mesen-nes-cross-check/`](../../tools/mesen-nes-cross-check/) and its
README for the Mesen2 oracle build, plus the `probe_*` diagnostics in
`crates/machine-nintendo-nes/tests/sprdma_dmc_probe.rs`.
