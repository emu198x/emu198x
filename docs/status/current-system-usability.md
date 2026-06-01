# Current System Usability Matrix

Status as of 2026-06-01. This page is deliberately practical: it records how a
developer can launch each current system today, what a user can reasonably do
with it, and the shortest path to making it comfortable to use. October launch
target is **ZX Spectrum**; the rest are engineering-bar systems with their own
honest status below.

## Summary

| System | Current launch path | Current usable state | Next usability step |
|--------|---------------------|----------------------|---------------------|
| ZX Spectrum | `emu198x-spectrum` | **October launch target.** 11 variants boot (16K, 48K, 48K+, 128K, +2, +2A, +2B, +3, Pentagon 128, Timex TC2048, Timex TS2068); Scorpion ZS-256 reaches CPU-liveness but not screen output. Shared `wgpu` native video with `raw`/`lcd`/`crt` modes, keyboard, audio, tape loading/autoload, snapshots through the shared runtime. CPU oracles green: Tom Harte 100%, ZEXDOC/ALL pass, FUSE 1,351/1,356, Patrik Rak `z80test` 6/6 zero-allowlist. 262/262 runtime tests pass; all 8 boot goldens green. 6 ULA/contention TAPs wired as smokes across 48K and 128K. | Strict PNG comparison for the 5 newly-wired ULA-test smokes against Spectron's references; Scorpion screen rendering (research recorded, fix scoped). |
| Commodore 64 | `emu198x-c64` | Interactive verifier shell with shared `wgpu` native video with `raw`/`lcd`/`crt` modes, keyboard, audio, PRG/BAS/T64 import, TAP autoload, optional 1541/`D64` path, physical gamepad input, and host-key joystick mode for port 2. CPU oracles: Tom Harte 100% (2.56M), Dormann functional pass, Lorenz 250/265 (15 hardware-dependent skips). | Make drive/tape workflows less flag-heavy and broaden software proofs. |
| Nintendo NES | `emu198x-nes` | Native verifier window with shared `wgpu` video and `raw`/`lcd`/`crt` modes plus headless cartridge runner with screenshots, live audio/audio capture, keyboard/gamepad controller input, scripts, snapshots, local smoke-matrix reporting, Blargg-style `$6000` test ROM assertions, and NROM/MMC1/UxROM/CNROM/MMC3/MMC5/AxROM/Color Dreams/VRC2a/Action 53/BxROM/NINA-001/Sunsoft-4/Camerica mapper support. **155-ROM test sweep: 130 PASS / 10 FAIL / 1 TIMEOUT / 14 VISUAL**; nestest 8991/8991; Super Mario Bros. renders. Open hard items: LXA `$AB` magic-constant stalemate, APU length-counter timing, OAMDMA+DMC DMA cycle accounting. | Use the smoke matrix and automated Blargg assertions to choose the next mapper or accuracy target; MMC5 now has memory mapping, expansion audio, and scanline IRQ coverage, but more hardware-test comparison would still be useful. |
| Commodore Amiga | `emu198x-amiga` | Native OCS verifier window with shared `wgpu` video with `raw`/`lcd`/`crt` modes, keyboard/mouse input, port-1 joystick/gamepad input, and live Paula audio, plus headless Kickstart/Workbench runner with A1000 and A500-family profiles, DF0 `ADF`, screenshots, audio capture, and scripted input. CPU oracles green: 68000 100% Tom Harte (1M tests); 68010/68020 100% against Musashi (via `m68k-test-gen`). ECS and A1200 (AGA) machine crates scaffolded; OCS is the live runtime. | Broaden game/application software validation; promote ECS and AGA from scaffold to live runtime. |
| Nintendo Game Boy | `emu198x-game-boy` | Native DMG-family verifier window using the shared `wgpu` video presenter with `raw`/`lcd`/`crt` modes, plus headless cartridge runner with screenshots, live audio/audio capture, keyboard/gamepad joypad input, scripts, snapshots, and `.sav` battery-RAM sidecars. CPU oracle: 49,600 Adam Tennant SM83 single-step tests pass + 92 lib unit tests. | Tune LCD presentation against hardware references and broaden real-game smoke coverage. |
| Dragon 32 | `emu198x-dragon` | Early native verifier window with shared `wgpu` video, live PIA-derived mono audio pinned to XRoar's DAC/tape/cartridge-SND/single-bit level model, real Dragon 32 BASIC ROM boot, semantic keyboard input, gamepad-to-analogue-joystick input, CAS media mounting, direct DragonDOS `.BIN` program loading, ROM/DGN cartridge mounting with GMC banking, initial DragonDOS VDK disk-sector reads through the P2 controller register path, PC-Dragon PAK snapshot mounting and snapshot smoke screenshots, native `--autoload` over ROM-level `CLOAD`/`CLOADM`, beam-updated MC6847 framebuffer, repeatable fetch/write trace watches, deterministic PAK trace-signature smoke, and optional patched-XRoar screenshot comparisons for CAS and PAK snapshot smokes. **Not in October launch scope.** | Run real DragonDOS ROM + VDK software smokes, then fill in exact controller timing/status/write behavior from observed failures. |

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
cargo run --release -p emu198x-nes --no-default-features -- --rom game.nes --frames 300 --screenshot nes.png
```

```sh
cargo run --release -p emu198x-nes --no-default-features -- --rom apu_test.nes --frames 3000 --assert-blargg
```

```sh
cargo run --release -p emu198x-nes -- game.nes
```

```sh
cargo run --release -p emu198x-amiga --no-default-features -- --model a500-a501 --kickstart ~/.emu198x/roms/commodore-amiga/kick13.rom --disk workbench13.adf --frames 2500 --screenshot amiga.png
```

```sh
cargo run --release -p emu198x-amiga -- --model a500-a501 --kickstart ~/.emu198x/roms/commodore-amiga/kick13.rom --disk workbench13.adf
```

```sh
cargo run --release -p emu198x-game-boy --no-default-features -- --rom game.gb --frames 300 --screenshot gameboy.png
```

```sh
cargo run --release -p emu198x-game-boy -- game.gb
```

```sh
cargo run --release -p emu198x-dragon -- --rom ~/.emu198x/roms/dragon/dragon32.rom --tape game.cas --autoload --video crt
```

```sh
cargo run --release -q -p emu198x-dragon --no-default-features -- --rom ~/.emu198x/roms/dragon/dragon32.rom --smoke-root '/path/to/Dragon/Applications/[CAS]' --smoke-run-limit 12 --smoke-report dragon-smoke.json
```

```sh
cargo run --release -q -p emu198x-dragon --no-default-features -- --rom ~/.emu198x/roms/dragon/dragon32.rom --snapshot-smoke-root '/path/to/Dragon/Games/[PAK]' --smoke-run-limit 32 --smoke-report dragon-pak-smoke.json --smoke-screenshot-dir dragon-pak-screens
```

```sh
cargo run --release -q -p emu198x-dragon --no-default-features -- --rom ~/.emu198x/roms/dragon/dragon32.rom --cart dragon-dos.rom --disk game.vdk --cycles 2000000 --dump-text
```

## Immediate Product Track

October's public deliverable is the **ZX Spectrum**; everything else is the
engineering bar. The fastest route to "I can actually use every emulator" is:

1. Keep the current-system verification gate green:

```sh
scripts/verify-current-systems.sh
```

2. Land the remaining Spectrum launch-blockers tracked in
   [`knowledge/tests/spectrum.md`](../../knowledge/tests/spectrum.md)
   § Outstanding launch-blockers — strict PNG comparison for the 5 ULA-test
   smokes; Scorpion screen rendering (research recorded, fix scoped); residual
   block-I/O AF disagreements (4) are correctness debt, not launch-blockers.
3. Keep the Game Boy, NES, and Dragon native verifier windows honest with
   real-ROM smoke runs before expanding their hardware scope; track the NES
   open known-hard items at
   [`knowledge/tests/nes.md`](../../knowledge/tests/nes.md).
4. Broaden Amiga software validation now that the native shell has mouse,
   joystick, and live audio.
5. Tune the first shared `wgpu` filter presets against hardware references:
   LCD for Game Boy, CRT for the TV/monitor systems.

## Verification Rule

Every row above should eventually have:

- one command that boots or loads representative software
- one automated test or ignored local harness that proves the claim
- one screenshot/audio/query artifact when visual or audio output is part of the claim
- explicit notes for required ROMs/media that cannot be checked into the repository

`scripts/verify-current-systems.sh` is the shared entry point for that rule. It
currently runs in-repository unit/integration checks for the current systems,
then conditionally runs local ROM/media smoke checks when the configured assets
exist. Missing local assets are recorded as `skip`, not `fail`, so the same
command is useful on fresh machines and on the full reference workstation.
