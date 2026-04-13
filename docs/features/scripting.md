# Scripting

> **Current fresh-workspace status.** Shared headless scripting exists today in
> `emu198x-shell`, and the current fresh-workspace runners exposing that path
> are `emu198x-script-spectrum` and `emu198x-script-c64`. References in older
> docs to NES, Amiga, or JSON-RPC/MCP scripting flows should be treated as
> historical until those families and host surfaces land again in this Rust
> workspace.

## Overview

Headless scripting is the current automation path for repeatable boot, media,
capture, snapshot, and observability workflows. The shared shell layer owns the
generic session model:

- load declared media into slots
- control media transport
- queue host input events
- run exact native-frame counts
- save or restore snapshots
- save PNG screenshots
- save WAV audio captures
- query current shared session state
- discover supported shared query paths

The current implementation is intentionally headless and deterministic. It is
for CI, content generation, verification harnesses, and future MCP-style host
surfaces, not for embedding UI policy into machine runtimes.

## CLI Usage

The current runners are `emu198x-script-spectrum` and `emu198x-script-c64`.
Spectrum currently exposes the richer family-specific workflow surface:

```bash
cargo run -p emu198x-script-spectrum -- \
  --rom 48.rom \
  --script capture.json
```

You can combine `--script` with normal runner flags such as:

- `--firmware ID=PATH`
- `--rom PATH`
- `--media SLOT:KIND=PATH`
- `--tape PATH`
- `--start-slot SLOT`
- `--stop-slot SLOT`
- `--play-tape`
- `--autoload-tape`
- `--load-snapshot PATH`
- `--save-snapshot PATH`
- `--screenshot PATH`
- `--audio-capture PATH`
- `--wait-for-tape-stop N`
- `--frames N`

Those flags prepare the machine before or after the shared script steps. The
script itself handles the reusable in-session workflow.

`--autoload-tape` is currently a Spectrum-specific convenience layered above
the shared session surface. It waits for the 48K ROM boot banner, types the
standard `LOAD ""` command through the real ROM editor, and starts tape
transport on `tape-1`. It is not an instant-loader shortcut.

`--wait-for-tape-stop N` is the current Spectrum runner alias for waiting until
`spectrum.tape.playing` becomes `false`. It runs after any autoload and script
steps, so it can block on a real tape load finishing before extra frame
execution or capture.

C64 currently exposes both a narrower host-side software-import surface and the
first real datasette media path:

```bash
cargo run -p emu198x-script-c64 -- \
  --rom-dir ~/.emu198x/roms/commodore-c64 \
  --load demo.bas \
  --save-snapshot demo.c64.pst
```

```bash
cargo run -p emu198x-script-c64 -- \
  --rom-dir ~/.emu198x/roms/commodore-c64 \
  --tape game.tap \
  --autoload-tape \
  --wait-for-tape-stop 12000
```

You can combine `--script` with normal C64 runner flags such as:

- `--rom-dir DIR`
- `--kernal PATH`
- `--basic PATH`
- `--chargen PATH`
- `--model pal|ntsc`
- `--load PATH`
- `--tape PATH`
- `--autoload-tape`
- `--start-tape`
- `--load-snapshot PATH`
- `--save-snapshot PATH`
- `--wait-for-boot N`
- `--wait-for-tape-stop N`
- `--screenshot PATH`
- `--frames N`

`--load PATH` is currently a C64-specific host convenience above the runtime
boundary:

- `.prg` files are imported directly into RAM using their declared load address
- `.bas` files are treated as UTF-8 plain-text BASIC source, tokenised, then
  imported as a BASIC program
- `.t64` files are treated as host-side containers, with the first loadable
  entry extracted and imported as a PRG

This is not emulated tape or disk loading. It is a fast host-side software
injection path for development, scripting, and future editor workflows. The
same concept will likely exist for other families later, but tokenisation and
editor semantics remain family-specific.

`--tape PATH` is different: it inserts a real Commodore TAP image into the
datasette slot. `--autoload-tape` waits for `READY.`, presses the real
`SHIFT+RUN/STOP` KERNAL shortcut, waits for `PRESS PLAY ON TAPE`, and then
starts `tape-1` through the shared media-control boundary. `--start-tape` and
`--wait-for-tape-stop` operate on that same live transport path.

`T64` support is intentionally separate. It currently lives under `--load PATH`
as a host-side container import path, not as a claim of pulse-accurate
datasette media.

## Script Format

A script file is a JSON array of shared steps. Each step uses an `action` field
plus any direct parameters needed by that action.

Example:

```json
[
  {"action":"run_frames","frames":200},
  {"action":"query_paths","prefix":"session.profile."},
  {"action":"query","path":"session.time"},
  {"action":"save_screenshot","path":"boot.png"}
]
```

Supported shared actions today:

| Action | Parameters | Result |
| --- | --- | --- |
| `load_media` | `slot`, `kind`, `path` | none |
| `media_transport` | `slot`, `transport` | none |
| `input` | `events` | none |
| `run_frames` | `frames` | `run_frames` observation |
| `wait_for_boot` | `max_frames` | `wait_for_boot` observation |
| `wait_for_query_bool` | `path`, `value`, `max_frames` | `wait_for_query_bool` observation |
| `query` | `path` | `query` observation |
| `query_paths` | `prefix` (optional) | `query_paths` observation |
| `load_snapshot` | `path` | none |
| `save_snapshot` | `path` | none |
| `save_screenshot` | `path` | none |
| `save_audio_capture` | `path`, `reset_after` (optional, default `true`) | none |

`input` events use the current `InputEvent` JSON form from the shared shell.
For example, one key tap looks like:

```json
{"action":"input","events":[{"Key":{"name":"enter","pressed":true}}]}
```

Media kinds currently accepted by the shared script layer:

- `tape`
- `disk`
- `cartridge`
- `optical`
- `snapshot`

Media transport actions currently accepted:

- `start`
- `stop`

## Query Surface

The shared query layer currently exposes stable, generic session paths rather
than family-specific chip state. The implemented paths today are:

- `capture.has_audio`
- `capture.has_frame`
- `run.last.reached`
- `run.last.stop_reason`
- `session.native_frame_ticks`
- `session.profile.capabilities`
- `session.profile.clock.rate.denominator_hz`
- `session.profile.clock.rate.numerator_hz`
- `session.profile.clock.unit`
- `session.profile.display_name`
- `session.profile.family`
- `session.profile.firmware.ids`
- `session.profile.machine_id`
- `session.profile.media_slots.ids`
- `session.profile.profile_id`
- `session.profile.region`
- `session.profile.release_year`
- `session.profile.summary`
- `session.profile.support_tier`
- `session.time`

For automation, prefer `query_paths` before hard-coding a query path. That
keeps scripts resilient as the shared shell surface expands.

Runners can also add machine-owned query paths on top of the shared shell layer.
The current Spectrum runner adds these generic automation paths:

- `boot.detected`
- `boot.reason`
- `boot.row`
- `screen.text.cols`
- `screen.text.lines`
- `screen.text.rows`

For the 48K Spectrum today, `screen.text.lines` is derived by matching the
bitmap screen against the resident ROM font. That is precise for ROM text
screens such as the boot banner, but it is not a general OCR path for arbitrary
graphics screens. The ROM copyright glyph is normalized to Unicode `©` so the
decoded line stays one cell wide and script-friendly.

It also adds these family-specific paths:

- `spectrum.keyboard.rows`
- `spectrum.machine.half_cycle_in_frame`
- `spectrum.machine.tstate_in_frame`
- `spectrum.machine.issue`
- `spectrum.tape.loaded`
- `spectrum.tape.playing`

The current C64 runner adds these family-specific paths:

- `boot.detected`
- `boot.offset`
- `boot.reason`
- `boot.row`
- `screen.text.lines`
- `c64.machine.cycle_in_line`
- `c64.machine.raster_line`
- `c64.tape.loaded`
- `c64.tape.playing`
- `c64.cia1.irq`
- `c64.cia2.irq`
- `c64.vic.ba_low`
- `c64.vic.irq`

For the current C64 runtime, `screen.text.lines` is decoded directly from
screen RAM and is useful for KERNAL text states such as `READY.`, `SEARCHING`,
`FOUND ...`, and `LOADING`.

## Output

When `--script PATH` is used, the current runners write one JSON report to
stdout after the run finishes.

Current Spectrum shape:

```json
{
  "observations": [
    {
      "kind": "run_frames",
      "frames": 200,
      "reached": 13977600,
      "stop_reason": "reached_target"
    },
    {
      "kind": "wait_for_boot",
      "frames": 200,
      "reached": 55910400,
      "reason": "found copyright banner on row 23",
      "row": 23
    },
    {
      "kind": "wait_for_query_bool",
      "path": "spectrum.tape.playing",
      "value": false,
      "frames": 10672,
      "reached": 3026356224
    },
    {
      "kind": "query",
      "result": {
        "path": "spectrum.machine.issue",
        "value": "issue3"
      }
    }
  ],
  "time": 13977600,
  "tape_loaded": true,
  "tape_playing": false
}
```

`observations` only includes steps that return structured data today:

- `run_frames`
- `wait_for_boot`
- `wait_for_query_bool`
- `query`
- `query_paths`

Errors are still reported on stderr, and the process exits non-zero on failure.

## Example

Boot the 48K ROM, run for 200 frames, discover profile paths, query current
time, and save a screenshot:

```json
[
  {"action":"wait_for_boot","max_frames":250},
  {"action":"query_paths","prefix":"session.profile."},
  {"action":"query","path":"boot.detected"},
  {"action":"query","path":"screen.text.lines"},
  {"action":"query","path":"session.time"},
  {"action":"query","path":"spectrum.machine.issue"},
  {"action":"save_screenshot","path":"spectrum_boot.png"}
]
```

Run it with:

```bash
cargo run -p emu198x-script-spectrum -- \
  --rom 48.rom \
  --script capture.json
```

For the current Spectrum runner, the equivalent launch-time convenience is:

```bash
cargo run -p emu198x-script-spectrum -- \
  --rom 48.rom \
  --tape manic_miner.tzx \
  --autoload-tape \
  --wait-for-tape-stop 12000
```
