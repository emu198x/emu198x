# Decision: C64 BASIC capture tools — mirror the Spectrum, locally

**Date:** 2026-06-07

## The decision

**The C64 binary gets `load_basic_program`, `press_key`, and `type_string` MCP tools implemented in the C64 crates, mirroring the Spectrum binary's bespoke tools — not promoted into the shared shell.** This brings the C64 core to Spectrum parity for authoring Code198x BASIC curriculum (load a `.bas`, run it, read the text screen, type input, screenshot) using the one capture pipeline instead of a VICE/`petcat` side-channel.

The split:

- **`runtime-commodore-c64/src/basic_loader.rs`** — `load_basic_program` / `load_basic_source`. Waits for the KERNAL `READY.` prompt, imports the tokenised PRG at `$0801` via `C64Runtime::load_prg_bytes` (which already relinks line pointers and sets `VARTAB`), mirrors `VARTAB` into `ARYTAB`/`STREND`, and optionally types `RUN`. Concrete over `C64Runtime` (one runtime, so no `LiveAccess` trait — unlike the Spectrum, which is generic across variants).
- **`runtime-commodore-c64/src/typing.rs`** — session-level `press_key` / `type_string` over the CIA1 matrix, plus `keys_for_char` in `input.rs`. Default charset is upper case, so letters map to the unshifted keycap.
- **`emu198x-c64/src/mcp_tools.rs`** — the three `Tool` impls, emitting the shared `ScriptObservation` JSON so the curriculum capture output is byte-identical to the Spectrum's.

## Why local, not shared-shell

The shell already harmonised the machine-agnostic tools (`register_common_tools`). `load_basic_program` / `press_key` / `type_string` are `ScriptStep` variants that `execute_collect` rejects as `SystemSpecificStep`, so each binary handles them — the Spectrum does this bespoke. Folding both Spectrum and C64 onto a shared shell capability-trait (`BasicProgramLoader`, `KeyboardController`) is the right long-term factoring for a multi-platform push (C64 → Amiga AMOS → VIC-20 …), but doing it now would mean abstracting from a single prior example *and* touching the working, shipped Spectrum path. Building C64-local first gives a second concrete instance to factor from later, and keeps the blast radius off the launch platform.

## Follow-up

- **Shell-unification** is tracked, not done: once a third BASIC-family consumer appears (AMOS / VIC-20), promote the three tools into `emu198x-shell` behind per-machine capability traits and migrate Spectrum + C64 onto it.
- The C64 binary still omits `register_debug_tools` (no `memory_read`/`poke_byte`/`disasm` over MCP yet); add if curriculum debugging needs it.

## Verification

Loader error paths, key mapping, and tool registration are unit-tested; clippy clean. End-to-end over real MCP stdin: a `.bas` loads, `RUN` runs, `HELLO C64` prints; an `INPUT` program is driven via `type_string`. ROMs resolve from `~/.emu198x/roms/commodore-c64`.
