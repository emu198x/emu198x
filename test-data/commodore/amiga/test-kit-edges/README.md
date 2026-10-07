# Test Kit right-edge corrections

The registered OCS gradients, AGA gradients and AGA crosshatch gates become
exact after restoring the final odd Copper request cell and retaining
Denise's horizontal display-window latch across counter resets. Framebuffer
geometry and reference contracts remain unchanged.

The [primary observations](../../../../../../reference/by-system/commodore-amiga/2026-test-kit-display-edge-observations.md)
record the guest source, pinned reference sources, measured failures and
signal interpretation. Native trace excerpts here preserve the failing
boundaries. They are diagnostic observations from Emu198x, not independent
reference data. Their provenance file hashes the complete scratch traces.

Counter CSVs record state after each existing machine tick. The counter is
therefore the position of the next output tick. Copper JSON entries contain
`[CCK, vpos, hpos, register, value]` at actual MOVE service. Crosshatch's two
pairs of window writes also retain the guest's next-field setup activity;
the measured visible-line window is `$1B51/$37D1`.

Executable regressions live in:

- `machine-commodore-amiga-ocs/tests/copper_line_end.rs`: actual MOVE timing
  at three WAIT positions on PAL and NTSC;
- `common-commodore-amiga/src/denise.rs`: comparator phase, missed stop at
  counter reset and register rewrites without a match;
- `runtime-commodore-amiga/tests/snapshot_roundtrip.rs`: both latch states
  through every half CCK of strobe reset on all three chipset tiers, plus
  rejection of the old version-51 payload;
- the unchanged explicit Test Kit video gates: exact image comparisons for
  all six patterns per profile, including both alternating phases.

Run `python3.13 check-latch-negative-control.py /tmp/latch-check` to compile
the exact production comparator and its two regression functions in isolation.
It also compiles an interval-predicate mutant, which must fail at the counter
reset. This checks test sensitivity; it is not independent hardware evidence.

`validation.json` records the before/after failures, release and raster
checks, producer/source identities and evidence limits. Full private-media
checkpoints and execution logs remain in `/private/tmp/emu198x-test-kit-edges/`.
