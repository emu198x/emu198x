# MCP Integration

## Overview

Each emulator exposes an MCP (Model Context Protocol) server that allows Claude and other AI tools to interact with the running system.

## Architecture

```
┌─────────────────┐     ┌─────────────────┐
│  Claude Code    │────▶│   MCP Server    │
│  or other AI    │◀────│   (per system)  │
└─────────────────┘     └────────┬────────┘
                                 │
                        ┌────────▼────────┐
                        │    Emulator     │
                        │      Core       │
                        └─────────────────┘
```

The MCP server is a thin translation layer. It does not contain emulation logic.

## Tools

### System Control

#### `boot`
Start the emulator.

```json
{
  "system": "c64",
  "region": "pal",
  "firmware": "/path/to/kernal.bin"
}
```

#### `reset`
Reset to initial state.

```json
{
  "type": "soft" | "hard"
}
```

#### `shutdown`
Stop the emulator.

### Media

#### `insert_media`
Insert disk, tape, or cartridge.

```json
{
  "slot": "drive8" | "tape" | "cartridge",
  "path": "/path/to/file.d64"
}
```

#### `eject_media`
Remove media from slot.

```json
{
  "slot": "drive8"
}
```

### Execution

#### `run`
Run continuously.

```json
{
  "until": "vblank" | "breakpoint" | null
}
```

#### `run_frames`
Run for N frames.

```json
{
  "count": 60
}
```

#### `run_ticks`
Run for N crystal ticks.

```json
{
  "count": 1000000
}
```

#### `step`
Step by instruction, cycle, or tick.

```json
{
  "unit": "instruction" | "cycle" | "tick",
  "count": 1
}
```

#### `pause`
Pause execution.

### State Inspection

#### `query_registers`
Get CPU register state.

Response:
```json
{
  "pc": 49152,
  "sp": 255,
  "a": 0,
  "x": 0,
  "y": 0,
  "flags": {
    "n": false,
    "v": false,
    "b": false,
    "d": false,
    "i": true,
    "z": true,
    "c": false
  }
}
```

#### `query_memory`
Read memory range.

```json
{
  "address": 53280,
  "length": 16
}
```

Response:
```json
{
  "address": 53280,
  "data": [14, 6, 1, 2, 3, 4, 5, 6, 7, 0, 0, 0, 0, 0, 0, 0]
}
```

#### `query_video`
Get video chip state.

Response (C64 example):
```json
{
  "raster_line": 156,
  "raster_cycle": 32,
  "border_colour": 14,
  "background_colour": 6,
  "sprites_enabled": 255,
  "badline": false
}
```

#### `query_audio`
Get audio chip state.

Response (C64 example):
```json
{
  "voices": [
    {"frequency": 1000, "waveform": "pulse", "gate": true},
    {"frequency": 500, "waveform": "sawtooth", "gate": false},
    {"frequency": 250, "waveform": "noise", "gate": true}
  ],
  "filter_cutoff": 1024,
  "volume": 15
}
```

#### `disassemble`
Disassemble memory region.

```json
{
  "address": 49152,
  "count": 10
}
```

Response:
```json
{
  "instructions": [
    {"address": 49152, "bytes": [169, 0], "mnemonic": "LDA", "operand": "#$00"},
    {"address": 49154, "bytes": [141, 32, 208], "mnemonic": "STA", "operand": "$D020"}
  ]
}
```

### Memory Modification

#### `poke`
Write to memory.

```json
{
  "address": 53280,
  "value": 0
}
```

#### `inject`
Load binary into memory.

```json
{
  "address": 49152,
  "data": [169, 0, 141, 32, 208, 96]
}
```

### Input

#### `key_down`
Press key.

```json
{
  "key": "A"
}
```

#### `key_up`
Release key.

```json
{
  "key": "A"
}
```

#### `type_text`
Type string (handles key timing).

```json
{
  "text": "LOAD \"*\",8,1\n"
}
```

#### `joystick`
Set joystick state.

```json
{
  "port": 2,
  "up": false,
  "down": false,
  "left": true,
  "right": false,
  "fire": true
}
```

### Breakpoints

#### `add_breakpoint`
Set breakpoint.

```json
{
  "type": "execution" | "read" | "write",
  "address": 49152,
  "condition": "a == 0"
}
```

Response:
```json
{
  "id": 1
}
```

#### `remove_breakpoint`
Clear breakpoint.

```json
{
  "id": 1
}
```

#### `list_breakpoints`
Get all breakpoints.

### Capture

#### `screenshot`
Capture current frame.

Response:
```json
{
  "format": "png",
  "width": 320,
  "height": 200,
  "data": "<base64>"
}
```

#### `start_recording`
Begin video/audio capture.

```json
{
  "video": true,
  "audio": true,
  "path": "/path/to/output.mp4"
}
```

#### `stop_recording`
End capture.

### Save States

#### `save_state`
Save emulator state.

```json
{
  "slot": 1
}
```

#### `load_state`
Restore emulator state.

```json
{
  "slot": 1
}
```

## Event Notifications

The MCP server can emit events:

### `breakpoint_hit`
```json
{
  "breakpoint_id": 1,
  "address": 49152,
  "registers": { ... }
}
```

### `frame_complete`
```json
{
  "frame_number": 1234
}
```

### `error`
```json
{
  "message": "Invalid memory address"
}
```

## Usage Examples

### Run a game and take screenshot

```
boot(system: "c64", region: "pal")
insert_media(slot: "drive8", path: "game.d64")
type_text("LOAD \"*\",8,1\n")
run_frames(count: 300)  // Wait for load
type_text("RUN\n")
run_frames(count: 600)  // Wait for title screen
screenshot()
```

### Debug assembly code

```
boot(system: "spectrum")
inject(address: 32768, data: [...])
add_breakpoint(type: "execution", address: 32768)
run()
// Breakpoint hits
query_registers()
step(unit: "instruction", count: 1)
query_registers()
```

### Verify lesson code

```
boot(system: "nes")
inject(address: 0x8000, data: student_code)
run_frames(count: 60)
screenshot()
// Compare against expected output
```

## Implementation Notes

- MCP server runs in separate thread from emulator core
- Commands are queued and processed at safe points (frame boundaries)
- State queries snapshot current state without affecting emulation
- Breakpoints are checked during tick loop, not after
