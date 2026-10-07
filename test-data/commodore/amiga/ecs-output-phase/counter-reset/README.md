# Lisa blanking comparisons during counter reset

All twenty reset-boundary guests match their three reference fields after
the correction. Ten retained ordinary AGA blanking controls also remain
exact: ninety fields and 77,903,280 RGB samples. Before correction, eight
reset-boundary guests fail in all three fields; twelve controls are exact.

`HBSTRT/HBSTOP` at CCK 1, fine positions 0..3, compare during the old counter's
last output tick. The pending next counter is already 2, while the current
counter is still 455. Computing current+1 misses those edges. Fine positions
4..7 match after the reset commits and already worked. CCK 0 and 2 provide
additional controls. The correction supplies the existing saved next value
to Lisa without changing counter progression or snapshot version 56.

Run `python3.13 replay.py` with Pillow available. Replay verifies hashes,
positive current=455/next=2 trace observations, sixty baseline comparisons
(twenty-four failing), and ninety exact corrected comparisons. Every case
uses the existing comparator: native origin (16,2), reference origin (4,0),
extent 1508×574. No reference samples, alignment or comparison bounds change.
The ordinary controls reuse reference fields from `../ecs-colour-blanking/aga/`.

`tools/build.py /path/to/fresh/output` rebuilds the twenty guests with the
existing SPHX assembler/ADF builder. All twenty ADF hashes match `cases.json`.
The seed is the previous programmed-central neutral guest: no new guest
framework. `producers.json` identifies native binaries, reference executable,
reference drawing source and ROM. Reference captures used the registered
AGA FS-UAE producer with `FSEMU_CODEX_AGA_PHASE=1`; each reference directory
retains its configuration, complete raw fields, metadata and compressed log.
The recorded native scripts run 360 frames, capture an unfiltered framebuffer
and require the matching SPHX identity and readiness counter. Supply a fresh
output path when rerunning a script.

`red.log` and `regression.patch` retain the board failure at native x1468..1471;
`green.log` is the same test after correction. The preceding four coloured
samples are required too. Counter tests cover all strobe/reset policies and
nine-bit wrap; chip tests cover all eight fine positions for both edges.
Snapshot replay covers 32 live half-CCK boundaries and observes both blank
levels plus the actual pending reset.

All 227 affected library tests and 59 snapshot tests pass. The strict A1200
Test Kit gate matches all six patterns; release build, strict Clippy,
formatting and Ruff pass. This is UAE-family software-reference evidence,
not original-silicon calibration. The wider 128-guest campaign and unrelated
ECS/fixed-blanking cases are not claimed complete here.
