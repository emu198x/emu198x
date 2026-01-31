# Scripting

## Overview

Headless automation for content generation, testing, and CI/CD.

## CLI Usage

```bash
emu198x-cli [OPTIONS] [COMMANDS...]
```

### Options

| Option | Short | Description |
|--------|-------|-------------|
| `--system` | `-s` | Target system |
| `--region` | `-r` | PAL or NTSC |
| `--headless` | `-H` | No display output |
| `--firmware` | `-f` | Firmware path(s) |
| `--verbose` | `-v` | Verbose output |
| `--script` | | JSON/YAML script file |

### Commands

Commands can be chained:

```bash
emu198x-cli -s c64 -H \
  load game.d64 \
  type "LOAD \"*\",8,1\n" \
  wait 300 \
  type "RUN\n" \
  wait 60 \
  screenshot title.png
```

### Command Reference

#### Media

```bash
load <file>              # Auto-detect slot
load --slot drive8 <file>
load --slot tape <file>
load --slot cart <file>
eject <slot>
```

#### Execution

```bash
run                      # Run until stopped
run --frames 60          # Run N frames
run --ticks 1000000      # Run N crystal ticks
run --until 0xC000       # Run until PC = address
step                     # One instruction
step --cycles 10         # N CPU cycles
step --ticks 100         # N crystal ticks
pause
reset
reset --hard
```

#### Input

```bash
type "TEXT\n"            # Type with timing
key A                    # Key down then up
keydown A                # Key down only
keyup A                  # Key up only
joy 2 left fire          # Joystick state
joy 2 release            # Release all
wait 60                  # Wait N frames
```

#### State

```bash
peek 0xD020              # Read memory
poke 0xD020 0            # Write memory
regs                     # Show registers
mem 0x0400 256           # Hex dump
disasm 0xC000 20         # Disassemble
inject 0xC000 code.bin   # Load binary
save-state 1             # Save to slot
load-state 1             # Load from slot
```

#### Capture

```bash
screenshot output.png
screenshot --format bmp output.bmp
record start output.mp4
record stop
audio-capture output.wav
```

## Script Format

### JSON

```json
{
  "system": "c64",
  "region": "pal",
  "firmware": {
    "kernal": "roms/kernal.bin",
    "basic": "roms/basic.bin",
    "chargen": "roms/chargen.bin"
  },
  "steps": [
    {"load": "game.d64"},
    {"type": "LOAD \"*\",8,1\n"},
    {"wait": 300},
    {"type": "RUN\n"},
    {"wait": 60},
    {"screenshot": "title.png"},
    {"run": {"frames": 600}},
    {"screenshot": "gameplay.png"}
  ]
}
```

### YAML

```yaml
system: c64
region: pal
firmware:
  kernal: roms/kernal.bin
  basic: roms/basic.bin
  chargen: roms/chargen.bin
steps:
  - load: game.d64
  - type: "LOAD \"*\",8,1\n"
  - wait: 300
  - type: "RUN\n"
  - wait: 60
  - screenshot: title.png
  - run:
      frames: 600
  - screenshot: gameplay.png
```

## Batch Processing

### Directory Processing

```bash
#!/bin/bash
for game in games/*.d64; do
  name=$(basename "$game" .d64)
  emu198x-cli -s c64 -H \
    load "$game" \
    type "LOAD \"*\",8,1\n" \
    wait 300 \
    type "RUN\n" \
    wait 120 \
    screenshot "screenshots/${name}.png"
done
```

### Parallel Processing

```bash
parallel --jobs 4 'emu198x-cli -s c64 -H --script {}' ::: scripts/*.json
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid arguments |
| 3 | File not found |
| 4 | Emulation error |
| 5 | Timeout |
| 6 | Verification failed |

## Verification Mode

For testing lesson code:

```bash
emu198x-cli -s c64 -H \
  --verify expected.png \
  --tolerance 0.01 \
  load student-code.prg \
  run --frames 60 \
  screenshot

# Exit code 0 = matches, 6 = different
```

### Verification Options

```bash
--verify <expected>       # Image to compare
--tolerance <float>       # Pixel difference tolerance (0-1)
--verify-memory <addr>=<value>  # Check memory value
--verify-register <reg>=<value> # Check register
```

## JSON-over-Stdin Mode

For integration with other tools:

```bash
emu198x-cli -s c64 -H --json-stdin
```

Then send commands as JSON:

```json
{"command": "load", "file": "game.d64"}
{"command": "run", "frames": 60}
{"command": "screenshot", "path": "out.png"}
{"command": "query", "path": "cpu.pc"}
{"command": "quit"}
```

Responses:

```json
{"status": "ok"}
{"status": "ok", "frames_run": 60}
{"status": "ok", "path": "out.png", "size": [320, 200]}
{"status": "ok", "value": 49152}
{"status": "ok", "message": "goodbye"}
```
