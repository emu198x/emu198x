# Paula CPU-fed playback boundary probe

The native probe reproduces 3,180 output/IRQ mismatches in 4,312 observations
against compiled WinUAE methods. Production is unchanged. It exits nonzero
until these faults are corrected; it is a research executable, not an ignored
regression. Existing DMA regressions remain enabled.

## Schedule and observations

Each producer executes 440 scenarios: four channels, periods 1/2/8/124/65,536,
eleven input schedules and actions before/after the period event. Every clock
delivers due IRQs first. A pre-event action precedes the period transition;
a post-event action follows it. No CPU/custom-bus timing is implied.

| Scenario | Action |
| --- | --- |
| 0 | No acknowledgement |
| 1 | Clear at clock 1 |
| 2–5 | Clear at word boundary minus 2, minus 1, at boundary, or plus 1 |
| 6 | Clear at 1, set at word boundary |
| 7 | Clear at 1, set one clock before word boundary |
| 8 | Clear at 1; write `0x3344` halfway through high byte |
| 9 | Clear at 1; write `0x3344` halfway through low byte |
| 10 | INTREQ already set at initial DAT write |

When times coincide, set/data-write takes precedence over clear. Clock-zero
startup occurs before the timed schedule, so actions at zero are not applied.
This matters for periods 1 and 2. The complete initial word is `0x1122`.

CSV columns: channel, effective period, scenario, post-event flag, clock,
state (0 idle, 2 high, 3 low), visible INTREQ bit, signed DAC sample. Observation
clocks are the sorted unique set `{0,1,p-1,p,p+1,2p-2,2p-1,2p,2p+1,2p+2,3p,3p+1}`.
The inventory is exactly 4,312 rows per producer.

## Source execution and limits

`reference.py` extracts the unmodified WinUAE state transitions, period loaders,
register-event handler, buffer loads, sample output, zero-state and IRQ methods.
The adapter replaces external scheduling with explicit CCK steps. `CYCLE_UNIT`
is normalised to one; period zero is supplied as its effective 65,536. Manual
DAT processing has no additional event delay in the pinned reference. DMA-only
callbacks abort if reached. Host hacks and PWM are disabled. Unused reference
parameters/locals are permitted; other compiler warnings are errors.

The vAmiga producer reuses the audited extraction in `interrupt-probe/` and
runs the same input schedule. Both producers' source files and revisions are
hashed. Their state/IRQ mismatch count is 420 rows across 108 scenarios.
vAmiga's experimental sampler suppresses repeated edges; its DAC column is
retained but excluded from that cross-reference comparison. Native functional
comparisons use WinUAE's sample-output and IRQ values. Native state-name
mismatches are separate: the current diagnostic intentionally labels CPU-fed
playback Idle, so its 3,072 differences are not independent functional evidence.

WinUAE's 4.9.0 beta 34 change log explicitly credits a test set for sampling
INTREQ at counter 1. See the primary observations and companion plan for the
three implementation commits and remaining hardware-provenance limits. These
are component adapters, not full-machine or physical-hardware traces.

## Reproduce

```sh
python3.13 test-data/commodore/amiga/paula-audio/manual-probe/reference.py \
  --winuae ../../emulators/amiga/WinUAE --vamiga ../../emulators/amiga/vAmiga \
  --output /tmp/paula-manual-reference
cargo run --locked --release -p emu198x-commodore-paula-8364 \
  --example manual_boundary_probe
```

The second command currently fails with functional mismatch counts
`[216, 312, 304, 284, 304, 216, 308, 260, 304, 376, 296]`.

Extracted WinUAE methods retain their upstream copyright and licensing; vAmiga methods
remain GPL-3.0-only, copyright Dirk W. Hoffmann. Each is compiled as a separate
diagnostic executable, never linked into the emulator. Original adapter/input
schedule and observation data follow the corpus CC0 dedication.
