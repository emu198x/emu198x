# Paula CPU-fed playback boundary probe

The correction closes all 3,180 output/IRQ mismatches in 4,312 observations
against compiled WinUAE methods. The probe is now an enforced regression.
Existing DMA regressions remain enabled. Snapshot version 59 retains the
manual stop/continue decision and rejects version 58.

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
mismatches are separate: the baseline diagnostic labelled CPU-fed playback Idle, so its 3,072
differences were not independent functional evidence. Corrected state names
also agree with all reference rows.

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

The second command passes. The retained baseline failed with functional
mismatch counts `[216, 312, 304, 284, 304, 216, 308, 260, 304, 376, 296]`.
`cargo test --locked --release -p emu198x-commodore-paula-8364 --test manual_playback`
runs this probe, the earlier 480-row vAmiga startup/holding schedule and the
attachment regression. The earlier adapter now uses begin/action/finish so
acknowledgements follow due IRQ delivery, matching its original producer.
No reference rows were changed.

The additional `attachments.csv` uses the same extracted WinUAE methods. It
checks 576 observations: four channels, four attachment settings (0/1/16/17),
acknowledgement enabled/disabled, and clocks 0 through 17. Period is 8; initial
DAT is `0x0011`, writes at 4 and 12 supply `0x0022` and `0x0033`; enabled
acknowledgements occur at 1 and 9. Columns are channel, unshifted attachment,
acknowledgement flag, clock, state, IRQ, next-channel period, next-channel
volume. Channel 3 records zero target fields. This establishes IRQ edges and
target-register delivery; it does not compare the muted source DAC buffer.

Extracted WinUAE methods retain their upstream copyright and licensing; vAmiga methods
remain GPL-3.0-only, copyright Dirk W. Hoffmann. Each is compiled as a separate
diagnostic executable, never linked into the emulator. Original adapter/input
schedule and observation data follow the corpus CC0 dedication.
