# DDF boundary schedules

Seven cases compile the unchanged registered FS-UAE register handlers and
display sequencer using the adjacent DDF-register-write extractor. The source
hash is checked before extraction. All 1,556 requests are retained in the CSV;
reversing request/comparator order changes every case and fails the control.

| Case | External inputs |
|---|---|
| 0 | OCS, one lores plane, DDF $18/$D8; write DDFSTOP=$10 at first-line $D8 |
| 1 | Same OCS configuration without the rewrite |
| 2 | OCS, four hires planes, DDF $18/$D0; disable DMA and write start=$10 at first-line $D7, leave one line idle, re-enable at third-line h=0 |
| 3 | ECS, four hires planes, DDF $18/$E0, fixed limits |
| 4 | Same ECS configuration with HARDDIS |
| 5 | ECS, four hires planes, equal DDF $38/$38, fixed limits |
| 6 | Same equal-boundary configuration with HARDDIS |

All cases use 227-CCK lines, an open vertical display gate and 16-bit fetches.
Case 2 runs three lines; the others run two. The harness supplies external
signals at the stated positions. It does not run the Copper, CPU, RGA bus,
memory or pixel output, and makes no physical-hardware certification claim.

```sh
python3.13 build-reference.py /path/to/registered/custom.cpp /tmp/ddf-boundaries
```

Compare the generated CSV and verification JSON byte for byte with this folder.
The existing register builder also rechecks its own controls during extraction.

The connected OCS machine tests in `tests/ddf_hard_stop.rs` check the entire
first-line reservation schedules for cases 4–6, real Copper delivery for case 0,
terminal memory services and pointer effects. The early-OCS equality control
also agrees with case 5's first-line schedule. Runtime `snapshot_roundtrip.rs`
checks case 2's early requests and actual services while comparing restored and
original state after every half CCK. These use the normal motherboard loop.

HARDDIS adds a reservation at $E2; its memory service occurs at next-line h=1.
At h=0 the completed byte counts therefore equal the fixed-limit control.
Following refresh collisions can replace a bitplane pointer, so subtracting
start/end pointers over an already-running cross-wrap sequence cannot measure
display bytes. Combined-service expectations are supported separately by the
registered `rga-conflicts` corpus.

Primary provenance and interpretation are in the
[shared observations](../../../../../../reference/by-system/commodore-amiga/2026-copper-blitter-wake-observations.md).
