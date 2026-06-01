# Current System Usability Matrix

Status as of 2026-06-01. This page is deliberately practical: it records how a
developer can launch each current system today, what a user can reasonably do
with it, and the shortest path to making it comfortable to use. October launch
target is **ZX Spectrum**; the rest are engineering-bar systems with their own
honest status below.

For the cross-system rollup of remaining work see
[`outstanding-work.md`](outstanding-work.md).

## Summary

| System | Current launch path | Current usable state | Next usability step |
|--------|---------------------|----------------------|---------------------|
| ZX Spectrum | `emu198x-spectrum` | **October launch target.** 11 variants boot (16K, 48K, 48K+, 128K, +2, +2A, +2B, +3, Pentagon 128, Timex TC2048, Timex TS2068); Scorpion ZS-256 reaches CPU-liveness but not screen output. Shared `wgpu` native video with `raw`/`lcd`/`crt` modes, keyboard, audio, tape loading/autoload, snapshots through the shared runtime. CPU oracles green: Tom Harte 100%, ZEXDOC/ALL pass, FUSE 1,351/1,356, Patrik Rak `z80test` 6/6 zero-allowlist. 262/262 runtime tests pass; all 8 boot goldens green. 6 ULA/contention TAPs wired as smokes across 48K and 128K. | Strict PNG comparison for the 5 newly-wired ULA-test smokes against Spectron's references; Scorpion screen rendering (research recorded, fix scoped). |
| Commodore 64 | `emu198x-c64` | Interactive verifier shell with shared `wgpu` native video with `raw`/`lcd`/`crt` modes, keyboard, audio, PRG/BAS/T64 import, TAP autoload, optional 1541/`D64` path, physical gamepad input, and host-key joystick mode for port 2. Headless boot to `READY.` verified 2026-06-01; disk autoload (`LOAD"*",8,1` → `SEARCHING FOR *` → `LOADING`) walks an Impossible Mission D64 end-to-end through the IEC bus and 1541 drive. CPU oracles: Tom Harte 100% (2.56M), Dormann functional pass, Lorenz 250/265 (15 hardware-dependent skips). 71/71 active runtime tests pass; 13 software-autoload tests sit `ignored` pending external D64/TAP archive paths. | Wire the gated D64/TAP autoload tests once archive paths land; an optional `--autoload-run` that follows `LOAD"*",8,1` with `RUN` would smooth one-line game launches. |
| Nintendo NES | `emu198x-nes` | Native verifier window with shared `wgpu` video and `raw`/`lcd`/`crt` modes plus headless cartridge runner with screenshots, live audio/audio capture, keyboard/gamepad controller input, scripts, snapshots, local smoke-matrix reporting, Blargg-style `$6000` test ROM assertions, and NROM/MMC1/UxROM/CNROM/MMC3/MMC5/AxROM/Color Dreams/VRC2a/Action 53/BxROM/NINA-001/Sunsoft-4/Camerica mapper support. **155-ROM test sweep: 135 PASS / 5 FAIL / 0 TIMEOUT / 15 VISUAL**; nestest 8991/8991; Super Mario Bros. renders. APU length-counter timing + LXA / ATX magic constant both closed 2026-06-01 (NES oracle-priority decision landed); `test_ppu_read_buffer.nes` reclassified VISUAL after confirming our CPU+PPU drive it correctly and the test reports via screen + audio, not `$6000`. Remaining hard items: `blargg_nes_cpu_test5` test 01-implied (CRC probe foundation at 2/20), OAMDMA + DMC DMA cycle interleave, `cpu_timing_test6` protocol. | Drive the CRC probe from 2/20 toward isolation of the 01-implied culprit; model OAMDMA odd-cycle penalty + DMC sample-DMA interleave. |
| Commodore Amiga | `emu198x-amiga` | Native verifier window with shared `wgpu` video, keyboard/mouse input, port-1 joystick/gamepad input, and live Paula audio, plus headless Kickstart/Workbench runner with the full A1000 / A500-family / A600 / A1200 (AGA) / A2000 model matrix reachable from `--model` in script mode (commit `bc23bc8`, 2026-06-01). A1200 + Kickstart 3.1 boots to the Insert-Workbench prompt and Workbench 3.1 mounts to a clean desktop with no palette or geometry artefacts. AGA fixes landed last week: 64-bit FMODE bitplane wide-fetch (`d31e46a`), 68020 full-format EA decode for the WB palette path (`369d50b`), DENISEID `$00F8` for AGA Lisa (`bc0e8ec`). DF0 `ADF`, screenshots, audio capture, scripted input all working. CPU oracles green: 68000 100% Tom Harte (1M tests); 68010/68020 100% against Musashi (via `m68k-test-gen`). | Broaden game/application software validation across OCS/ECS/AGA; flesh out Gayle for A600/A1200 IDE/PCMCIA paths; promote Workbench 3.1 boot to an automated screenshot smoke. |
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
cargo run --release -p emu198x-amiga --no-default-features -- --model a1200 --kickstart ~/.emu198x/roms/commodore-amiga/kick31a1200.rom --disk workbench31.adf --frames 1800 --screenshot aga_wb.png
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
4. Broaden Amiga software validation across OCS / ECS / AGA now that the
   `--model` matrix is reachable from script mode and AGA + Workbench 3.1
   render cleanly.
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
