# Counter-qualified AGA palette XOR

BPLCON4's XOR change was one lores period late. The original delay measurement
included four padded reference-buffer samples. The correction reads the
six-sample tap of the existing ten-sample history; snapshot 54 is unchanged.

The native trace accepts the write before counter 260 output. The trace-only
FS-UAE build records `XOR_WRITE counter=260` and
`XOR_VISIBLE counter_shres=1046` (261.5), at stored x=682. Its three fresh
constant-word fields are byte-identical to the original retained captures.
Varying-word and pointer-reset fields retain their earlier counter-traced
captures. All three guests have 600 qualified origin observations.

Replay with:

```sh
python3.13 test-data/commodore/amiga/ecs-output-phase/palette-xor/replay.py
```

The replay verifies archived artifact hashes and guest/reference identities,
checks counter-origin coverage, and compares complete 1508×574 common rasters.
It requires nine corrected fields to match exactly and all nine pre-fix fields
to differ by 128 samples. It uses native origin (16,2) and reference origin
(4,0); no image-content alignment or pixel exceptions are allowed.

`validation.json` binds the native producer, sources, and archived evidence.
`xor-trace.patch` and the before/after compressed reference source describe
observation-only instrumentation; rendering logic is unchanged. `red.log`
records the old chip failing at native offset 6. `native-trace.log` records
its raw write and ten-sample output history. The separately retained regression
report covers the mid-line corpus; its older reference fields share the traced
producer origin, but do not each have their own origin trace.

This establishes agreement with the registered UAE-family software reference.
It is not independent physical-silicon calibration. Sprite-bank timing, HAM
control-bit interactions and arbitrary subpixel CPU writes are not newly
calibrated by these three guests.
