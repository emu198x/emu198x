# Machine binaries share a launcher

**Status:** Adopted 2026-09-07. The launcher is in the tree
(`emu198x-shell/src/launch.rs`, `emu198x-ui/src/launch.rs`) and the Sord M5
and Jupiter Ace binaries run on it. The other binaries are ported one at a
time; a binary is "on the launcher" when its `main.rs` is the three-line
dispatch below and it has no `script.rs` or `mcp.rs` of its own.

## The problem

Every `emu198x-<machine>` binary has the same three modes and the same
common flags, and until now every one of them carried its own copy of the
glue: mode detection in `main.rs`, a CLI parser in `ui.rs` and a second one
in `script.rs`, the `~/.emu198x/roms/<machine>/` lookup (three copies per
binary: UI, script, MCP), the headless run-and-report loop, and the MCP
server setup. Measured before the change, on the two smallest binaries:

| File | Lines | Identical between Sord M5 and Jupiter Ace after renaming |
|---|---|---|
| ui.rs | 362 | 184 |
| script.rs | 252 | 162 |
| mcp.rs | 59 | 25 |

Thirty binaries, thirty hand-maintained copies, and no two `main.rs` files
alike once names were normalised. That is the shape
[`z80-machines-should-share-a-cadence-driver.md`](z80-machines-should-share-a-cadence-driver.md)
diagnoses for the CPU loop, applied to the host glue, and it is what
[RULES.md rule 30](../../RULES.md) says to promote upward.

## The decision

The shell owns the headless half and the UI crate owns the windowed half.
A machine binary supplies one type implementing two traits:

- `emu198x_shell::launch::MachineApp` — the machine's own flags
  (`parse_flag`), how to build its runtime (`build_runtime`, and
  `build_mcp_runtime` when MCP starts blank), its frame budget and query
  provider, any startup media, the machine-specific fields of the headless
  report, and which MCP tools it registers (default: base + keyboard).
- `emu198x_ui::launch::UiApp` — the `UiSystem` that drives its window.

`main.rs` becomes:

```rust
mod app;
#[cfg(feature = "ui")]
mod ui;

fn main() {
    #[cfg(feature = "ui")]
    emu198x_ui::launch::main::<app::Machine>();
    #[cfg(not(feature = "ui"))]
    emu198x_shell::launch::main_headless::<app::Machine>();
}
```

The launcher owns: mode detection (`--script`/`--frames`/`--screenshot`/
`--audio-capture`/`--headless` → script, `--mcp`/`--mcp-stdio` → MCP, else
window), the common flags, `--help` text assembled from the machine's option
and control lines, the conventional ROM lookup (`$ENV`, then
`~/.emu198x/roms/<relative>`), the script loop (prepare → script → frames →
captures → report), and the MCP server (build → `startup_media::load_into`
→ register tools → serve).

## What changes for a user of the binaries

Deliberately little. The report JSON keeps each machine's keys. The MCP tool
set is unchanged. Two things are now uniform where they varied:

- Usage errors (unknown flag, bad value, capture with nothing to run) exit
  2 with a pointer at `--help`; runtime failures exit 1. Some binaries used
  1 for both and printed the whole usage text on every error.
- `--scale` and `--video` are accepted in every mode rather than rejected
  as unknown by the script parser. They only matter to the window.

## What the port measured

| Binary | Lines before | Lines after |
|---|---|---|
| emu198x-sord-m5 | 770 | 366 |
| emu198x-jupiter-ace | 734 | 309 |

The launcher itself is ~700 lines, paid once. What remains in a ported
binary is the machine: its flags, its runtime construction, its report
fields, its `UiSystem`, and its key map.

## Porting the rest

Port one binary per commit. Keep the machine's report keys and its MCP tool
registrations exactly; the launcher has hooks for both. A binary whose
script mode does more than the shared loop (the Spectrum, C64, Amiga, and
Dragon carry bespoke script runners and tool sets) keeps that code and
still gains the shared parsing, ROM lookup, and dispatch; if a hook is
missing, add it to the launcher rather than keeping a private copy of the
loop.

## Drift triggers

Stop and re-read this record if you find yourself:

- adding a `script.rs` or `mcp.rs` to a machine binary;
- writing `fn next_arg`, `fn die`, or `fn default_rom_path` in a binary;
- parsing `--frames`, `--screenshot`, `--audio-capture`, `--script`,
  `--scale`, or `--video` anywhere but `emu198x-shell/src/launch.rs`;
- giving a new machine a `main.rs` longer than the dispatch above.
