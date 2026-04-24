# Current System Usability Matrix

Status as of 2026-04-24. This page is deliberately practical: it records how a
developer can launch each current system today, what a user can reasonably do
with it, and the shortest path to making it comfortable to use.

## Summary

| System | Current launch path | Current usable state | Next usability step |
|--------|---------------------|----------------------|---------------------|
| ZX Spectrum | `emu198x-spectrum`, `emu198x-script-spectrum` | Best current interactive path. Windowed video, keyboard, audio, tape loading/autoload, snapshots through the shared runtime. | Tighten model/media defaults and keep expanding verification for non-48K variants. |
| Commodore 64 | `emu198x-c64`, `emu198x-script-c64` | Interactive verifier shell with keyboard, video, audio, PRG/BAS/T64 import, TAP autoload, and optional 1541/`D64` path. | Make drive/tape workflows less flag-heavy and broaden software proofs. |
| Nintendo NES | `emu198x-nes`, `emu198x-script-nes` | Native NROM verifier window plus headless cartridge runner with screenshots, audio capture, scripts, and NROM proof via `nestest`/`Super Mario Bros.`. | Add live audio to the native shell and the next mapper needed by real software. |
| Commodore Amiga | `emu198x-amiga`, `emu198x-script-amiga` | Native OCS verifier window plus headless Kickstart/Workbench runner with A1000 and A500-family profiles, DF0 `ADF`, screenshots, Paula-backed audio capture, and scripted input. | Add mouse/joystick input and live native audio. |
| Nintendo Game Boy | `emu198x-game-boy`, `emu198x-script-game-boy` | Native DMG-family verifier window plus headless cartridge runner with screenshots, audio capture, scripts, and snapshots. | Add live audio to the native shell and persistent battery-save writeback. |

## Launch Commands

These commands are intentionally minimal. Use the system pages for deeper
accuracy notes and known gaps.

```sh
cargo run --release -p emu198x-spectrum -- --rom 48.rom --tape game.tzx --autoload-tape
```

```sh
cargo run --release -p emu198x-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --disk game.d64 --autoload-disk
```

```sh
cargo run --release -p emu198x-script-nes -- --rom game.nes --frames 300 --screenshot nes.png
```

```sh
cargo run --release -p emu198x-nes -- game.nes
```

```sh
cargo run --release -p emu198x-script-amiga -- --model a500-a501 --kickstart ~/.emu198x/roms/commodore-amiga/kick13.rom --disk workbench13.adf --frames 2500 --screenshot amiga.png
```

```sh
cargo run --release -p emu198x-amiga -- --model a500-a501 --kickstart ~/.emu198x/roms/commodore-amiga/kick13.rom --disk workbench13.adf
```

```sh
cargo run --release -p emu198x-script-game-boy -- --rom game.gb --frames 300 --screenshot gameboy.png
```

```sh
cargo run --release -p emu198x-game-boy -- game.gb
```

## Immediate Product Track

The fastest route to "I can actually use every emulator" is:

1. Keep Spectrum and C64 as the first interactive shells and remove obvious launch friction.
2. Keep the new Game Boy and NES native verifier windows honest with real-ROM smoke runs before expanding their hardware scope.
3. Add Amiga mouse/joystick input and live native audio now that the native shell exists.
4. Build a small cross-system verification matrix from these exact launch paths so usability work does not regress accuracy.

## Verification Rule

Every row above should eventually have:

- one command that boots or loads representative software
- one automated test or ignored local harness that proves the claim
- one screenshot/audio/query artifact when visual or audio output is part of the claim
- explicit notes for required ROMs/media that cannot be checked into the repository
