# Original NES regional save/output baseline

Captured before extraction from Emu198x main `4cf6a2b022d75bf2abd280cb416b863e55d91527` using
`runtime-nintendo-nes/src/board_baseline_tests.rs`. The capture source is
committed with these files. These are native version-2 saves and complete
continued output, not hardware qualification or commercial ROM dumps.

Each region has pads, Four Score and Zapper cases. The cartridge comes from
our source-built `test-data/synthetic-cartridges/nintendo-nes-logo.nes` with
an unused PRG area holding a pulse-initialisation thunk; the reset vector is
located from the iNES PRG count. After eight native frames, the specimen
holds all four controller states, Zapper aim/trigger, CPU/PRG RAM markers,
battery-backed RAM and a 137-dot partial field. Capture three further fields
with complete pixels, dimensions, elapsed time, absolute timestamps, every
mono audio sample bit, and the final complete native save.

Capture only into a fresh directory with `EMU198X_NES_CAPTURE=/absolute/path`.
The writer uses create-new and cannot overwrite existing fixtures. Normal
runs compare all files, reconstruct the original state, and compare the
first three resumed fields and final save. Both regions and all three input
configurations pass on the original engine. The distinct CPU open-bus loss
in #2213 is exercised by separate mid-instruction regressions: these stable
frame-boundary samples do not establish correctness at every CPU phase.
