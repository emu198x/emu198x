# Play-Test Results — 2026-03-23

## Spectrum (Z80 snapshots)

| Game | Boots | Display | Notes |
|------|-------|---------|-------|
| Manic Miner | YES | Correct | Title screen perfect, colours right |
| Jet Set Willy | YES | Correct | Shows copy protection code entry |
| Knight Lore | YES | Correct | Loading screen with all Spectrum colours |

**Status: Spectrum Z80 snapshot loading works well.**

## NES

| Game | Boots | Display | Notes |
|------|-------|---------|-------|
| Super Mario Bros | YES | Correct | Title screen perfect, ground/hill sprites right |
| Super Mario Bros 3 | YES | Correct | Intro curtain scene with Mario+Luigi (MMC3 mapper) |
| Zelda II | YES | Correct | In-game overworld rendering |

**Status: NES rendering looks accurate for the games tested.**

## C64

| Game | Boots | Display | Notes |
|------|-------|---------|-------|
| International Karate (D64) | BASIC only | N/A | `?DEVICE NOT PRESENT ERROR` — 1541 drive not connecting via IEC bus in headless mode |
| Turrican III intro (PRG) | BASIC only | N/A | PRG loads into memory but doesn't auto-run |

**Issues found:**
1. **D64 disk loading fails in headless mode** — `LOAD "*",8,1` returns DEVICE NOT PRESENT. The drive ROM is loaded but the IEC serial bus may not be initialised in headless mode. Works in the compat harness (which uses the factory function), so this may be a runner-specific issue.
2. **PRG auto-run missing** — PRGs load into memory but the user must type RUN. The headless `--type` flag works but the first-character timing is finicky (type-at 100 loses the first char, type-at 200 works).
3. **No auto-LOAD from disk** — unlike the compat harness which calls load_media(), the runner doesn't auto-load. Users need to type LOAD commands manually.

## Amiga

Not yet tested (needs ROM setup + game files).

## Key Issues to Fix

1. **C64 headless D64 loading** — the 1541 drive needs to work in headless mode
2. **C64 PRG auto-run** — inject `RUN\n` after PRG load in headless mode
3. **C64 typing timing** — first character eaten at type-at 100
4. **All systems: auto-load** — when a file is provided on the command line, it should load AND run, not just insert
