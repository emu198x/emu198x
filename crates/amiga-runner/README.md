# `amiga-runner`

Minimal frontend for `machine-amiga` (currently focused on A500/OCS).

## What It Does Today

- Windowed video output (`winit` + `pixels`)
- Host keyboard mapping to Amiga keyboard codes
- Basic Paula audio playback (with `--mute` option)
- Headless screenshot/audio capture
- Headless KS1.3 insert-screen benchmark mode

## Basic Usage

```sh
# Windowed boot (ROM via env var)
AMIGA_KS13_ROM=roms/kick13.rom cargo run -p amiga-runner

# Windowed boot (explicit ROM + optional ADF)
cargo run -p amiga-runner -- --rom roms/kick13.rom --adf path/to/disk.adf
```

## Headless Capture

```sh
# Save a screenshot after N frames
cargo run -p amiga-runner -- \
  --rom roms/kick13.rom \
  --headless \
  --frames 300 \
  --screenshot test_output/boot.png

# Save screenshot + WAV
cargo run -p amiga-runner -- \
  --rom roms/kick13.rom \
  --headless \
  --frames 300 \
  --screenshot test_output/boot.png \
  --audio test_output/boot.wav
```

## KS1.3 Boot Benchmark (Insert Screen)

This mode runs headless and stops when the framebuffer matches the KS1.3
insert-disk screen regression heuristic used by the machine tests.

```sh
cargo run --release -p amiga-runner -- \
  --rom roms/kick13.rom \
  --headless \
  --frames 300 \
  --bench-insert-screen \
  --mute
```

Example output:

```text
KS1.3 insert-screen detected.
  Frames run: 206
  Emulated time: 4.113s
  Wall time: 1.272s
  Realtime ratio: 3.235x
```

## Notes

- The benchmark and KS1.3 screenshot regression require a Kickstart ROM you
  provide locally.
- The long `boot_kickstart` screenshot test remains separate from this runner
  and is intentionally not part of normal CI due ROM licensing.
