# `amiga-runner`

Runnable frontend for `machine-amiga`. This crate is transitional and is
intended to become `emu-amiga`.

## What It Does Today

- Windowed video output (`winit` + `pixels`)
- Host keyboard mapping to Amiga keyboard codes
- Basic Paula audio playback (with `--mute` option)
- Headless screenshot/audio capture
- Headless scripting via `--script`
- Headless MCP server mode via `--mcp`

## Basic Usage

```sh
# Windowed boot (ROM via env var)
AMIGA_KS13_ROM=roms/kick13.rom cargo run -p amiga-runner

# Windowed boot (explicit ROM + optional disk image)
cargo run -p amiga-runner -- --rom roms/kick13.rom --disk path/to/disk.adf

# Select a different model; chipset derives from the model preset
cargo run -p amiga-runner -- --rom roms/kick31.rom --model a1200
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

`--disk` auto-detects ADF vs IPF. `--adf` remains available when you want to
force ADF loading explicitly.

## Automation

```sh
# Run a scripted headless session
cargo run -p amiga-runner -- --script scripts/amiga-boot.json

# Run as MCP JSON-RPC server over stdin/stdout
cargo run -p amiga-runner -- --mcp
```

In normal use, the machine preset chooses the chipset:

| Model       | Chipset |
| ----------- | ------- |
| `a1000`     | OCS     |
| `a500`      | OCS     |
| `a2000`     | OCS     |
| `a500plus`  | ECS     |
| `a600`      | ECS     |
| `a3000`     | ECS     |
| `a1200`     | AGA     |
| `a4000`     | AGA     |

## Notes

- You must provide a Kickstart ROM locally, either with `--rom` or
  `AMIGA_KS13_ROM` for simple local runs.
- The long `boot_kickstart` screenshot test remains separate from this runner
  and is intentionally not part of normal CI due ROM licensing.
