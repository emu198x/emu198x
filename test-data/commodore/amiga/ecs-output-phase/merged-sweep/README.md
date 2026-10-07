# Merged Amiga graphics validation

All 128 retained A1200 PAL graphics guests match on merged revision
`277957646cf041c343daf503d8d32d3e24e20eeb`, snapshot 56. The 384 comparisons
cover 332,387,328 RGB samples with zero differences. The three palette-XOR
guests that failed the preceding broad sweep are now exact with the other 125.
No production code changed for this validation.

The same clean source passes both strict Test Kit lanes (six patterns each),
all fourteen static and ten timed programmable-blanking cases, and all 59
snapshot integration tests. `validation.json`, `gates.json`, `logs/` and
`timed-reports/` retain the results and the positive case counts.

## Evidence and scope

The live adapter reuses the preceding 128-case inventory, ready-identity
checks and fixed counter-qualified mapping. Guest ADFs, reference metadata,
raw reference fields and firmware match the previous registered hashes.
The new executable and all inputs are hashed before and after capture;
none changed during the run. `producers.json` retains those identities.

Each static guest runs for 360 fields before one new native screenshot is
captured and its SPHX ready record is checked. That image is compared with
each of the three retained reference guest fields 9, 10 and 11. These are
384 comparisons, not 384 separately captured native fields. The reference
origin is shared source/counter calibration; the broad corpus does not carry
a fresh counter trace for every guest.

The unchanged common rectangle is 1508x574 samples: native origin (16,2),
reference origin (4,0). `historical_mapping_fields` preserves the superseded
padding-based mapping for audit; it is not the acceptance result.

`cases/` retains every native screenshot and ready-record log, guest input,
reference metadata and compressed raw reference. Gzip is lossless with a zero
timestamp. No firmware or native executable is bundled. The capture and gate
drivers are retained as `.py.txt` records of the local invocation; live
recapture requires the recorded firmware and original local directory layout.
Archived comparison replay is independent of that layout.

This establishes UAE-family agreement for the retained software corpus. It
does not establish physical-hardware timing, all display-mode combinations,
NTSC equivalence, audio accuracy or a new real-software catalogue run. The
blanking gate continues to label five cross-producer comparator disagreements;
a passing phase check does not turn those into cross-family consensus.

## Replay

With the project's existing Python 3.13 and Pillow environment:

```sh
python3.13 test-data/commodore/amiga/ecs-output-phase/merged-sweep/replay.py
```

The replay requires all 128 distinct guests and 384 complete adjacent reference
fields, validates identities and hashes, and repeats every RGB comparison.
It also injects one changed RGB sample in memory and requires exactly one
mismatch. `negative-controls.json` records separate rejection of a changed
PNG, the same changed PNG with its hash updated, and an empty inventory.
Original bytes were restored before the successful full replay.

`archive-hashes.json` records the retained run artifacts. `comparison.json`
contains every case result; `replay.json` records the independently repeated
totals. The preceding failing palette-XOR evidence remains in its existing
archive and has not been replaced.
