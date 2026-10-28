# Commercial Game Boy screen checks

Two fixture-gated tests compare Tetris Rev 1 (EU-US) and Super Mario Land Rev 1
with independently captured SameBoy 1.0.3 output. The strict `expected.json`
identifies every complete game image and the original DMG boot ROM by SHA-256
and length. No game, firmware, snapshot or screenshot bytes are distributed.

Run with your own identified images:

```sh
EMU198X_GB_COMMERCIAL_ROOT=/path/to/games \
EMU198X_GB_BOOT_ROM_DIR=/path/to/firmware \
cargo test -p runtime-nintendo-game-boy --test commercial_screens --locked -- --ignored
```

The directories must contain `tetris.gb`, `mario.gb` and `dmg_boot.bin` with the
identities in the manifest. These tests are ignored by default because the
fixtures cannot be distributed. An explicit run fails for missing paths,
missing files or different bytes; it cannot silently skip a configured case.

Both engines boot as DMG, receive no input, and anchor at the first real LCD
frame after BOOT unmaps. The anchor is field zero. We count 240, 480 and 720
completed native LCD fields, excluding LCD-off safety stops and SameBoy's
artificial callbacks. Each expected hash covers all 23,040 shade bytes, in
raster order, 0=white through 3=black. SameBoy uses DMG_B and its grayscale
palette. The tests also save field 480, replay 240 native fields and compare
the final screen and entire saved state. They check source images remain intact.

The [retained probes and evidence](../../../docs/plans/2026-10-28-game-boy-commercial-screens/)
include source-file identities for the read-only SameBoy snapshot, capture
harnesses, all nine comparisons, a second independent capture run and negative
controls. Expected hashes came from SameBoy, not from accepting Emu198x output.
This is reference-emulator agreement, not a physical-hardware capture claim.

Kirby's Dream Land Rev 0 was also measured, but differs by 201/135/177 pixels
at those fields. Its hashes were not accepted as goldens. Coordinates and both
engines' shade values are retained separately for investigation; the cause is
not yet established.
