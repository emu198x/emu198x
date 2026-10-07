# Connected bus-ownership regressions

This pass corrects fourteen tests that still conflated comparator, admission,
service or software-observation boundaries. Production chip timing and runtime
snapshot version 51 are unchanged.

The OCS library tests run real Copper words, held display transfers, disk FIFO
completion and area-blitter admissions through the motherboard driver. They
check CPU exclusion during actual service, both half CCKs and restore where
applicable. Matching nasty/non-nasty and active/parked Copper controls distinguish
priority from an idle bus. The D-only setup runs startup and the locked first
D through the driver; admission cannot satisfy its memory-write assertions.

The additional corrections distinguish:

- BFD becoming idle on an even CCK from Copper's returning odd/free WAIT1
  comparison, with final D still withheld;
- vertical DIWHIGH gating from a separate cross-wrap refresh pointer collision;
- Alice's DDF comparator from its later reservation, memory service and Lisa
  data retirement at the line boundary;
- CLXDAT's fixed bit 15 from bit 0 relatching when CLXCON enables no comparisons.
  Real OCS/ECS/AGA CPU programs test both that relatch and a deliberately
  mismatching BP1 comparison, with two destructive reads.

`build-reference.py` reuses the registered DDF extractor, verifying the pinned
FS-UAE `custom.cpp` hash and unchanged functions. The small AGA fixture supplies
FMODE=1, eight lores planes, HARDDIS, DDF $12/$D0, an open vertical gate and a
227-CCK line. It emits BPL8's reservation at $14, followed by the other seven
planes. The native AGA test checks its addressed transfer at $15, memory service
and line reset at $16, normal RGA data retirement at $17, and the resulting
held tail using both blank and set-bit controls. The final serial-word check
uses an explicit inner-chip parallel copy; this is not a full-raster guest.

```sh
python3.13 build-reference.py /path/to/registered/custom.cpp /tmp/bus-ownership
```

Generated CSV and verification JSON must match this folder byte for byte.
Reversing request/comparator order must differ. This harness compiles source
fragments, not a full reference machine or physical hardware. Copper, DMAL,
area-blitter and combined-RGA provenance also relies on the existing shared
stage corpora and live timing observations; this pass adds no new whole-raster
conformance claim.

The [primary observations](../../../../../../reference/by-system/commodore-amiga/2026-copper-blitter-wake-observations.md)
record the measured failures and interpretation. `validation.json` records the
release sweep, corrected failure names, limits and log hashes.
