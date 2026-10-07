# OCS fixed blanking at the right edge

OCS colour output now agrees with all three retained reference fields. The
old capture differs in 2,288 RGB samples per field; the corrected capture
differs in zero. The ECS and AGA programmed-central controls also remain
exact: nine fields and 7,790,328 compared RGB samples in total.

Run `python3.13 replay.py` from any working directory with Pillow available.
Replay verifies artifact hashes, 3,000 active-line trace observations, all
three old failure signatures, and all nine corrected comparisons. It reuses
the admitted comparator and reference artifacts in the sibling
`ecs-colour-blanking/` directory. No pixel alignment search or changed crop
is involved.

The instrumented UAE producer records the actual preceding four buffer
writes at each counter. Counter 13 writes colour at x=1504..1507; counter 14
matches next-counter $0F and writes zero at x=1508..1511. Counter 15 observes
the asserted horizontal-blank level and those four zero writes. All five
observations are required on every active line (44..243), in each of guest
fields 9, 10 and 11. Captured reference bytes are unchanged by instrumentation.
`observation.json` records producer/input hashes and field metadata; the
compressed source pair and patch preserve the instrumentation. The original
reference executable and source were restored afterwards.

`red.log` and `regression.patch` retain the board failure before the fix.
`green.log` runs the same regression successfully. Chip tests require both
fixed edges, nine-bit wrap and retention when a counter jump misses an edge.
Runtime snapshots replay seven reset/edge boundaries with both latch levels.
The older publication test assumed the entire tail was coloured; its
retained failure identifies column 760. Its corrected assertions require
the completed coloured carry through column 759 and black through 767.

Verification logs cover 620 distinct chip, shared-board, runtime, output and
snapshot tests, a release build, strict Clippy, formatting, and the strict
A500+A501 Test Kit gate (all six patterns exact). Amiga saves use version 56
and explicitly reject version 55. ECS/AGA behaviour is unchanged by the OCS
board connection. This does not claim physical-hardware calibration, fixed
OCS vertical blanking, or completion of the deferred full raster campaign.
