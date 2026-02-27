# Scripting

## Overview

Headless batch automation for content generation, testing, and CI/CD. Each
emulator binary accepts `--script <file.json>` to run a sequence of commands
without opening a window. Scripts reuse the MCP server's method dispatch, so
every MCP command is available.

## CLI Usage

Each system has its own binary. Pass `--script` with a JSON file:

```bash
# ZX Spectrum (ROM embedded — no extra files needed)
cargo run -p emu-spectrum -- --script capture.json

# Commodore 64 (needs ROMs in roms/ directory)
cargo run -p emu-c64 -- --script capture.json

# NES (ROM passed via boot command or --rom)
cargo run -p emu-nes -- --rom game.nes --script capture.json

# Amiga (Kickstart passed via boot command or AMIGA_KS13_ROM)
cargo run -p amiga-runner -- --script capture.json
```

## Script Format

A script file is a JSON array of simplified RPC requests. No `jsonrpc` or `id`
fields needed — the runner assigns sequential IDs automatically.

Each step has a `method` and optional `params`:

```json
[
  {"method": "boot", "params": {}},
  {"method": "run_frames", "params": {"count": 200}},
  {"method": "screenshot", "params": {"save_path": "boot.png"}},
  {"method": "query", "params": {"path": "cpu.pc"}}
]
```

### save_path Convention

When a response contains base64 `data` (from `screenshot` or `audio_capture`)
and the request params include `save_path`, the runner decodes and writes the
file to disk. The MCP dispatch ignores `save_path` — it is handled by the
script runner after the response comes back.

## Available Methods

Methods match the MCP server for each system. Common methods across all four:

| Method | Params | Description |
|--------|--------|-------------|
| `boot` | system-specific | Create emulator instance |
| `reset` | — | Reset CPU |
| `run_frames` | `count` | Run N frames |
| `step_instruction` | — | Step one instruction |
| `step_ticks` | `count` | Step N master clock ticks |
| `screenshot` | `save_path` (optional) | Capture PNG |
| `audio_capture` | `frames`, `save_path` | Capture WAV |
| `query` | `path` | Query observable state |
| `query_memory` | `address`, `length` | Read memory bytes |
| `poke` | `address`, `value` | Write memory byte |
| `set_breakpoint` | `address`, `max_frames` | Run until PC hits address |

### System-specific methods

**Spectrum:** `load_sna`, `load_tap`, `press_key`, `release_key`, `type_text`,
`get_screen_text`

**C64:** `load_prg`, `press_key`, `release_key`, `type_text`,
`get_screen_text`, `boot_detected`, `boot_status`

**NES:** `load_rom`, `press_button`, `release_button`, `input_sequence`

**Amiga:** `insert_disk`, `press_key`, `release_key`

## Output

The runner writes one JSON-line response per step to stdout. Diagnostic
messages (like "Saved boot.png") go to stderr.

```
{"jsonrpc":"2.0","result":{"status":"ok"},"id":1}
{"jsonrpc":"2.0","result":{"frames":200,"tstates":13977600},"id":2}
{"jsonrpc":"2.0","result":{"format":"png","width":320,"height":288,"data":"iVBOR..."},"id":3}
{"jsonrpc":"2.0","result":{"path":"cpu.pc","value":4572},"id":4}
```

## Examples

### Spectrum: Boot and capture the copyright screen

```json
[
  {"method": "boot"},
  {"method": "run_frames", "params": {"count": 200}},
  {"method": "screenshot", "params": {"save_path": "spectrum_boot.png"}},
  {"method": "get_screen_text"}
]
```

### Spectrum: Load a SNA snapshot and screenshot

```json
[
  {"method": "boot"},
  {"method": "load_sna", "params": {"path": "shadowkeep.sna"}},
  {"method": "run_frames", "params": {"count": 50}},
  {"method": "screenshot", "params": {"save_path": "shadowkeep.png"}}
]
```

### C64: Boot, wait for READY, load a PRG

```json
[
  {"method": "boot"},
  {"method": "run_frames", "params": {"count": 120}},
  {"method": "boot_detected"},
  {"method": "load_prg", "params": {"path": "starfield.prg"}},
  {"method": "run_frames", "params": {"count": 60}},
  {"method": "screenshot", "params": {"save_path": "starfield.png"}}
]
```

### NES: Load a ROM and capture

```json
[
  {"method": "boot", "params": {"path": "dash.nes"}},
  {"method": "run_frames", "params": {"count": 30}},
  {"method": "screenshot", "params": {"save_path": "dash.png"}}
]
```

### Amiga: Boot Kickstart and capture insert-disk screen

```json
[
  {"method": "boot", "params": {"kickstart_path": "roms/kick13.rom"}},
  {"method": "run_frames", "params": {"count": 300}},
  {"method": "screenshot", "params": {"save_path": "amiga_boot.png"}}
]
```

### Amiga: Boot with ADF disk

```json
[
  {"method": "boot", "params": {"kickstart_path": "roms/kick13.rom"}},
  {"method": "insert_disk", "params": {"path": "exodus.adf"}},
  {"method": "run_frames", "params": {"count": 500}},
  {"method": "screenshot", "params": {"save_path": "exodus.png"}}
]
```

## Batch Processing

```bash
#!/bin/bash
for sna in snapshots/*.sna; do
  name=$(basename "$sna" .sna)
  cat > /tmp/script.json <<SCRIPT
[
  {"method": "boot"},
  {"method": "load_sna", "params": {"path": "$sna"}},
  {"method": "run_frames", "params": {"count": 50}},
  {"method": "screenshot", "params": {"save_path": "screenshots/${name}.png"}}
]
SCRIPT
  cargo run -p emu-spectrum -- --script /tmp/script.json
done
```

## MCP Server Mode

For interactive use (AI agents, IDEs, live debugging), use `--mcp` instead.
This reads newline-delimited JSON-RPC 2.0 requests from stdin and writes
responses to stdout, running until stdin closes.

```bash
cargo run -p emu-spectrum -- --mcp
```

## Future Work

Not implemented yet, but planned:

- **Verification mode** — compare screenshots against expected images
- **Chained CLI commands** — `--run 60 --screenshot out.png` without a JSON file
- **YAML script format** — for readability in lesson pipelines
