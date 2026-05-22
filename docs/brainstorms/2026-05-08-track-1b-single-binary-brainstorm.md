---
date: 2026-05-08
topic: track-1b-single-binary
---

# Track 1B — Single binary consolidation

## What We're Building

Merge `emu198x-script-spectrum` (headless) into `emu198x-spectrum` (UI) so
the family ships one binary with three modes — interactive UI, headless
script execution, and MCP server. Closes SOLID criterion 4
("Pipeline / single binary") in `knowledge/systems/spectrum/solid-status.md`
and unblocks Code198x's curriculum DoD (every unit's screenshot/video
captures through this binary).

The consolidation is also a CLI surface cleanup: most existing flags
get replaced by JSON script commands, with a small set of "convenience
aliases" surviving for shell one-liners. The script vocabulary becomes
the canonical machine-state surface — same vocabulary that MCP will
expose as tools when it lands.

## Why This Approach

We considered three flavours of MCP scope (stub mode, hello-world MCP,
full MCP) and picked stub mode for this commit — Track 1B's UI/script
consolidation is enough scope on its own. Folding MCP in muddies the
seam. MCP gets its own commit when criterion 5 (MCP server functional)
is tackled separately.

We considered three CLI shapes (mode flags, subcommand dispatch,
double-duty `--script`) and picked double-duty `--script PATH` because
it preserves Code198x backward compatibility and MCP-as-flag matches
SOLID's locked criterion text.

We considered keeping the existing flag-heavy CLI vs migrating most
verbs to JSON script commands, and picked aggressive migration. CLI
flags can't carry ordering, dependencies, or composition; script
commands can. The vocabulary already covers ~13 of 15 existing flags;
the gaps (firmware loading, autoload-tape, set-machine) close with
small `ScriptStep` extensions.

We considered keeping `--rom` / `--firmware` flags as developer-side
escape hatches and decided against — consistent with "no LoadFirmware
script step", custom-ROM workflows put the file at the conventional
path.

We considered a cargo feature default of "headless first, opt in to
UI" vs "UI first, opt out for headless" and picked the latter — devs
get the GUI by default, Code198x builds with `--no-default-features`
for a fast headless binary.

## Key Decisions

- **MCP scope**: stub mode this commit — `--mcp` flag exists, prints
  "not implemented yet" and exits 1. Implementation is its own
  Track / commit when criterion 5 is tackled.
- **Mode-flag spelling**: `--script PATH` doubles as the headless-mode
  trigger AND as the JSON session file argument. `--mcp` is a separate
  mode flag. `--headless` (or just absence of `--script`/`--mcp`) lets
  the user run `--script` without the GUI.
- **CLI surface, final**:
  ```
  Modes (default UI):    --headless, --mcp
  Source:                --script PATH
  Display config (UI):   --scale N, --video raw|lcd|crt, --turbo-tape
  Convenience aliases:   --tape PATH, --autoload-tape, --play-tape
  Other:                 --help, --version
  ```
- **Dropped flags**: every other flag from both binaries
  (`--rom`, `--firmware`, `--media`, `--start-slot`, `--stop-slot`,
  `--load-snapshot`, `--save-snapshot`, `--screenshot`, `--audio-capture`,
  `--wait-for-boot`, `--wait-for-tape-stop`, `--frames`) becomes a
  JSON script step. **No `--rom`** — consistent with "no `LoadFirmware`
  script step".
- **Default boot**: with no `--script` and no `--mcp`, the binary
  invokes `set_machine: spectrum_48k` itself. User never types ROM
  paths.
- **`--script` works in both UI and headless**. UI mode (default)
  opens the GUI and runs the script visibly; headless skips the
  window. Same vocabulary, different presentation.
- **New `ScriptStep` variants** to add:
  - `SetMachine { kind }` — JSON action `set_machine` — variant string
    (e.g. `"spectrum_48k"`); binary resolves to the conventional ROM
    bundle via `read_variant_firmware`. Replaces every "load firmware"
    pattern with one declarative step. Implicit reset of any prior
    state (loaded media, snapshots, frame counter).
  - `AutoloadTape { slot, max_frames }` — JSON action `autoload_tape`
    — wraps the existing `autoload_basic_tape` helper.
- **`SetMachine` layering**: shell-level `ScriptStep` carries a
  `kind: String`; binary intercepts and dispatches Spectrum-specific
  resolution. Per-system script vocabularies (a typed
  `MachineKind` per system) is the principled long-term refactor;
  defer until vocabulary growth justifies it.
- **Cargo features**: `default = ["ui"]`. UI deps (winit, wgpu, muda,
  rfd, cpal, native-video) only pull when the feature is on. `--ui`
  mode requires the feature; `--headless`, `--script`, `--mcp` always
  work. Code198x builds with `cargo build -p emu198x-spectrum
  --no-default-features` for a small headless binary.
- **Code organization**: subdirectory layout —
  `src/ui/` (App loop, window, menu, live_machine, frame pacing),
  `src/script/` (executor entry, JSON parsing, capture sinks),
  `src/mcp/` (stub today, expandable),
  `src/main.rs` (tiny dispatcher).
- **Migration**: atomic cutover. Delete `emu198x-script-spectrum`
  crate, update Code198x screenshot/video skills in lockstep. Both
  repos under same ownership; coordination is two `git push`es.

## Resolved questions

- **Default boot policy** — eager. When `--script` is given without
  `set_machine`, the binary boots 48K and then runs the script.
  Preserves Code198x backward compat; matches the "no `--script` =
  default 48K UI boot" behaviour.
- **Implicit reset semantics on `SetMachine`** — always resets
  in-progress state (loaded tape, snapshots, frame counter, audio
  buffer). Documented in the `ScriptStep::SetMachine` docstring.
- **Script vocabulary docstring location** — on `ScriptStep` itself
  in `emu198x-shell/src/script.rs`, with the binary's `--help` output
  pointing at it.

## Next Steps

→ `/workflows:plan` for implementation steps (file changes, ordered
tasks, verification points).

Or proceed straight to implementation given the design is concrete and
the work is mechanical.
