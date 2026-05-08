---
date: 2026-05-08
topic: spectrum-basic-loader
---

# ZX Spectrum BASIC loader (direct-to-RAM)

## What We're Building

A `ScriptStep::LoadBasicProgram { path, run }` that ingests one
plain-text `.bas` file, tokenises it in pure Rust, pokes the result
into RAM at `PROG`, fixes up the BASIC system variables, and (by
default) starts the program running. Skips the tape entirely.

Pulls the prior-art tokeniser out of `Emu198x-Oldest/crates/
format-sinclair-zx-spectrum-bas` (Steve confirms it was production
code in the previous tree) and rehomes it as a fresh crate in the
new repo. A separate `bas2tap` companion CLI is parked for later —
the primary goal is in-emulator ingestion, not the build pipeline.

Closes Steve's long-standing want of "Code198x writes a `.bas`
file, the emulator runs it" without the tape-load detour. For
curriculum video and screenshot capture this saves ~1–10 seconds
per BASIC unit (no tape autoload, no waiting for tape transport
to stop).

## Why This Approach

**Why pure-Rust tokeniser, not driving the BASIC editor** —
typing every keyword via the keyboard matrix at 50 fps takes
multiple seconds per line of BASIC. The Rust tokeniser is
deterministic, fast, and the keyword table is small (~80 keywords)
and stable. Prior art already exists and was passing 11 tests in
the older project.

**Why direct RAM poke, not a synthetic tape** — the prior art's
output was a `.tap` because the older pipeline used the ROM loader
to install the program. With direct ingestion the script reduces
to `load_basic_program → run_frames`, eliminating the autoload
sequence and the wait-for-tape-stop loop that dominate BASIC unit
capture time.

**Why a fresh `format-sinclair-zx-spectrum-bas` crate** — matches
the project's existing convention (the new tree already has
`format-sinclair-zx-spectrum-tap`, `format-commodore-c64-bas`).
Keeps the tokeniser system-specific but free of runtime deps, so
an eventual `bas2tap` binary or a WASM build can re-use it.

**Why auto-RUN by default** — Code198x's curriculum pipeline is
dominated by "load and play". An opt-out (`run: false`) covers the
rare case (inspecting the program before running). Making the
common case implicit removes a footgun for script authors.

## Key Decisions

- **Tokeniser**: ported verbatim from
  `Emu198x-Oldest/crates/format-sinclair-zx-spectrum-bas/src/
  {lib.rs,tokens.rs}`. 419 + 100 lines, 11 tests; no runtime
  changes needed. Output is `BasicProgram { bytes: Vec<u8> }`
  containing concatenated tokenised lines (4-byte header per line,
  `$0D` terminator).
- **Crate**: new `format-sinclair-zx-spectrum-bas`. Workspace
  member, no system-specific runtime deps. Public API: `tokenise`,
  `BasicProgram`, error type.
- **Runtime helper**: `runtime-sinclair-zx-spectrum::
  load_basic_program(machine, &BasicProgram, run: bool)`. Pokes
  bytes at `PROG` (`0x5CCB` on a vanilla 48K), writes the trailing
  `$80` variables-end marker, writes `$0D $80` after for `E_LINE`,
  and updates system variables `PROG` (`23635`), `VARS`
  (`23627`), `E_LINE` (`23641`). When `run` is true, sets
  `NEWPPC` to the lowest line number so the BASIC interpreter
  enters program execution on its next tick — bypasses the
  keyboard-input route.
- **Step**: `ScriptStep::LoadBasicProgram { path: PathBuf, run: bool }`
  in shell. Default for `run` is `true` (serde default). System-
  specific dispatch: shell executor returns
  `ScriptError::SystemSpecificStep { step: "load_basic_program" }`;
  the Spectrum binary intercepts it before delegation, same shape
  as `AutoloadTape`.
- **Variant scope**: 48K-first. The address constants and system-
  variable layout are identical across 16K/48K/Plus and largely
  identical on 128K-class machines (BASIC area is at the same
  address; bank-switching does not affect it because BASIC lives
  in the always-mapped lower RAM). Initial implementation gates
  on the 48K runtime; 128K-class support gets wired when each
  variant lands a runtime. No machine-specific tokens (the older
  128K BASIC keywords like `PLAY`, `SPECTRUM` are not yet on the
  table — added later if Code198x needs them).
- **Comment / blank lines**: prior art behaviour kept — `#`-prefixed
  lines and blank lines are skipped, matching curriculum-friendly
  conventions.
- **Encoding**: UTF-8 text, but tokenisation operates on bytes;
  any non-ASCII inside string literals or after `REM` is emitted
  as-is (the Spectrum's character set diverges from ASCII for
  graphics codes — those will need escape syntax later, parked).
- **Failure handling**: tokeniser errors surface from the script
  step as `ScriptError::Session(SessionError::Machine(...))`
  carrying the original line and column.

## Open / parked items (not in this commit)

- **`bas2tap` CLI** — a build-pipeline companion that wraps the
  tokeniser into a TAP. Defer until Code198x asks for it.
- **128K BASIC-only keywords** (`PLAY`, `SPECTRUM`, etc.) — not
  in the tokeniser table; add when 128K-class units need them.
- **Spectrum graphics-code escapes** in source text (`\{block-1}`
  or similar) — needed for full curriculum coverage of UDG/block
  graphics; parked behind real demand.
- **Multi-program inspection** — the loader replaces whatever's
  in the BASIC area; it doesn't preserve a previous program.
  Snapshot/restore is the workaround if needed.
- **Cross-system `LoadBasicProgram` for C64** — the same step
  shape would route to a C64 tokeniser; out of scope for the
  October Spectrum-only push.

## Next Steps

→ Implementation. Phase shape:
  1. New `format-sinclair-zx-spectrum-bas` crate. Port tokeniser +
     tokens module + tests verbatim from `Emu198x-Oldest`. Wire
     into the workspace.
  2. Runtime helper `load_basic_program` in
     `runtime-sinclair-zx-spectrum`, with unit tests using a
     freshly-booted 48K machine fixture. Verify `PROG`/`VARS`/
     `E_LINE` end up at the right values and that a peek of
     `(PROG)..` matches the tokenised bytes.
  3. New `ScriptStep::LoadBasicProgram { path, run }` in shell;
     dispatch returns `SystemSpecificStep` when no binary handler.
     JSON round-trip test.
  4. Spectrum binary's script module: intercept the new step,
     read the file, call the runtime helper.
  5. Smoke: write `10 PRINT "HELLO"` `.bas`, capture screenshot,
     verify the screen shows the program output. Time the load:
     should be sub-millisecond plus a handful of frames for the
     interpreter to enter `RUN`.
  6. Verify with one real Code198x BASIC unit from `code-samples/
     sinclair-zx-spectrum/`. If a unit ships a `.bas`, point
     the new step at it directly. If not, transcribe the source
     once as a fixture.
