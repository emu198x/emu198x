# Lisa vertical blanking at the first visible line

All ten retained AGA blanking guests match their counter-qualified reference
rasters after the correction: thirty fields, 25,967,760 RGB samples, zero
differences. Eighteen pre-fix fields each differ by 1,304 samples; twelve
pre-fix control fields are exact. `replay.py` requires both results and
verifies the archived artifact hashes before comparison.

```sh
python3.13 test-data/commodore/amiga/ecs-output-phase/top-field-blanking/replay.py
```

Native origin (16,2), reference origin (4,0), and extent 1508×574 are the
existing counter-qualified contract. Reference fields and guest ADFs are
reused from `../ecs-colour-blanking/aga/`; no pixels, origins, or crop masks
were changed. Native captures are unfiltered 1536×576 PNGs. Reproduce a live
capture with the commands/actions in each case's `baseline.json`, using the
current binary and a fresh output path. The readiness record must contain
SPHX, the expected guest identity and at least nine completed guest fields.

The missing state was Lisa's vertical-blank latch. The board delivered the
STRVBL→STRHOR transition through its normal RGA stage, but only the horizontal
counter consumed it. On the first visible line the reference keeps vertical
blank asserted until HBSTOP; native output previously exposed COLOR00 at
x=[16,668), y=[2,4). Horizontal blanking already masked x=[668,924).

`reference-trace.patch` only adds observation logging to the previously
instrumented FS-UAE 5.0.7 producer. Its three fresh raw buffers are byte-for-byte
identical to the retained programmed-central fields; `reference-validation.json`
records their hashes. The before/after drawing sources and compressed log are
retained. Trace `v` denotes UAE's linear display/queue label, not Agnus VPOS.
`native-trace.log` records Agnus VPOS and the existing pending strobe stages.

Current vendored WinUAE retains separate pending fixed/programmed vertical
events. Lisa now does the same, consuming only the selected horizontal
comparator's matching start/stop event. The normal strobe descriptor remains
owned by the existing board counter stage; no additional clock or bus path
is introduced. ECS and OCS behaviour is unchanged.

Snapshot 55 preserves the preceding strobe identity, both pending events and
both vertical-blank levels; version 54 is rejected. Tests cover all eight
fine stop phases, repeated strobes, selector changes, equal/wrapping edges,
and 24 half-CCK restore boundaries. `red.log` is the board regression before
the fix; `green.log` is the same assertion after it.

These are UAE-family software observations, not physical silicon calibration.
The independent Minimig path does not supply the same programmable comparator
evidence. OCS far-edge and counter-reset discrepancies remain separate work.
