# Copper WAIT after blitter start

Build the project-authored AGA PAL guests with:

```sh
python3 tools/build.py /private/tmp/amiga-copper-blitter-wait
```

The builder reuses the validated sprite-phase corpus's bootblock, READY
protocol and assembler helpers. Each paired guest has 48 rows: eight blit
lengths, three horizontal start positions, and BLTPRI off/on. `bfd-ignore`
ignores blitter completion; `bfd-wait` waits for completion. DMA enables
Copper and blitter, with COPCON.CDANG set so the Copper can program blitter
registers. Sprites and bitplanes are disabled; COLOR00 marks instruction timing.

After native and reference capture, compare the bounded control with:

```sh
python3 tools/compare.py /private/tmp/amiga-copper-blitter-wait \
  --output /private/tmp/amiga-copper-wait-control.json
```

The native screenshot defaults to `after.png`; `after.log` must contain the
script `memory_read` observation for 128 bytes at `$2FF00`, including READY
identity and counter. Explicit `--case bfd-wait` checks the completion-dependent
case. Either case exits 1 if a full-raster field differs.

The comparison uses the full common PAL raster at documented producer origins:
native `(16,2)` through `(1528,576)`, reference `(0,0)` through `(1512,574)`.
READY identity, a completed guest-field counter, ADF/source hashes and three
adjacent reference fields are required. A blank or boot-screen capture cannot
establish agreement. The existing wide-sprite comparator supplies these fixed
raster rules; the exploratory capture also positively checks native READY data
and red-marker content.

Registered FS-UAE revision `f362278ccd4c60991caac3b4d240d4a3f751bea2` shows
both WAIT idle stages. The BFD=1 control matches all three full rasters after
the version-48 correction. Version 49 corrects the area DMA/holding schedule. Its BFD=0 guest improves
from 22,512 to 1,344 mismatched pixels per field: 42 of 48 programmed marker
starts remain two CCKs early, while six match. The independent event trace
found all 48 completion-dependent MOVE intervals two CCKs early; six marker
edges were hidden by blanking in the image comparison.

Version 50 restores the free live comparison after a blitter-blocked WAIT.
All 96 paired MOVE intervals and six full-raster fields now match the registered
reference. This does not close DMA timing: four BFD=0 busy inputs remain one
CCK early, and the actual destination transfers retain phase differences.
Check the independent event trace alongside the raster; matching pixels alone
cannot validate those boundaries. Native output is never promoted to a
reference image.

Primary observations and source limits live in
`reference/by-system/commodore-amiga/2026-copper-wait-idle-observations.md` in
the umbrella tree, with the follow-up event trace in
`reference/by-system/commodore-amiga/2026-copper-blitter-wake-observations.md`.
The active plan records logs and outstanding work.
