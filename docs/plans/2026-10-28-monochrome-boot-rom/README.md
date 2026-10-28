# Monochrome boot and snapshot evidence

The version-3 fixtures were generated at source
49956980d7841a53420aaa49ff16211161e38edb using its locked dependencies and the
original `loop_rom` cartridge. Copy the retained generators to that revision's
`crates/runtime-nintendo-game-boy/examples/` directory and run them with the
fixture directory as their sole argument. `baseline_generator.rs` creates the
starting states; `baseline_continuation.rs` restores those states using unchanged
version-3 code and advances by a further model frame. Source-relative includes
resolve from that examples directory.

`run_until(2048)` returns after a complete frame. The migration tests decode the
old prefix and compare the actual stored time, rather than assuming the target
was exact. DMG/CGB continuations advance from 16,417 to 33,973; no continuation
check can pass by executing zero cycles. `test-data/game-boy/snapshot-v3/manifest.json`
records identities for all nine files. The files contain no commercial firmware.

The original-firmware test lives in `machine-nintendo-game-boy/src/mono_boot_tests.rs`.
It reads local DMG0/DMG/MGB firmware only when explicitly enabled with
`EMU198X_GB_BOOT_ROM_DIR` and `--ignored`. The project demo's logo field is blank
for skipped boot, so the valid-input header is completed in memory from the
known DMG ROM's reference logo table. Its disk bytes remain untouched.
Deliberate logo and checksum corruption exercise the boot ROM's own refusal
through raw cartridge hardware; the runtime loader independently rejects bad
header checksums before execution.

All three original ROMs render their logo, reject both corrupt controls, and
hand off at $0100/SP=$FFFE with CPU registers matching whichboot's hardware
reference values. Measured handoffs from this model's reset state:

| Boot ROM | M-cycles | Divider counter |
|---|---:|---:|
| DMG0 | 6,140,810 | $CE28 |
| DMG | 5,860,081 | $ABC4 |
| MGB | 5,860,081 | $ABC4 |

These are execution observations, not a claim of universal cold-power phase
calibration. No post-boot register or timer patch is applied at unmapping.

The public runtime regression initially failed with `UnknownFirmware` for the
monochrome id. It now covers optional loading, overlay boundaries, even/odd
BOOT writes, irreversible unmapping, mid-boot replay, firmware-only states,
post-unmap reset and wrong-size refusal. The default skipped-boot path and CGB
coverage remain in the affected package suite. V4 corruption/cache/hardware
mismatches are rejected atomically; V3 migration uses genuine old bytes.

The nine-ROM mooneye phase-2 gate passes with every required file verified present
before invocation. Original-firmware controls and full affected suites pass;
strict all-target Clippy and formatting pass. Firmware acquisition identities
are retained in the local asset provenance and the umbrella reference note.
