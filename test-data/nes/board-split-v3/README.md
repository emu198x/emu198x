# Corrected pre-split NES v3 baseline

Captured after the #2213 save-fidelity fix and before regional extraction,
using `crates/runtime-nintendo-nes/src/board_baseline_tests.rs`. The earlier
`../board-split/` v2 fixtures remain the original execution control and
legacy rejection inputs. All frame pixels, dimensions, timestamps and exact
audio sample bits agree with those original controls.

The capture recipe remains project-written logo firmware plus the guest
pulse initializer, both regions, pads/Four Score/Zapper, held controls,
battery RAM and a 137-dot partial field. Each state includes the corrected
CPU bus latch, PPU per-bit refresh timestamps and colour/emphasis output mirror. Output files contain three
complete continued fields and the final native save. Separate tests sample
a guest mid-instruction and save during visible raster output, comparing
the first emitted NTSC waveform packet too.

Set `EMU198X_NES_CAPTURE` only to a fresh external directory. The capture
writer uses create-new and refuses to overwrite any existing file. Normal
tests compare complete files and restored continuation. These are emulator
preservation fixtures, not independent hardware qualification.
