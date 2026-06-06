# Nintendo Game Boy

## Status: Native DMG verifier; CPU oracle green

A primary system: native `wgpu` window (`raw`/`lcd`/`crt`), headless cartridge
runner, snapshots, `.sav` battery-RAM sidecars. Sharp LR35902 (SM83) core.

## What works

- **SM83 CPU** — 49,600 Adam Tennant single-step tests pass + 92 lib unit tests.
- **DMG-family** — native window with keyboard/gamepad joypad, screenshots, live
  audio + capture, scripts, snapshots, `.sav` battery RAM.

## Not implemented / accuracy gaps

- **LCD preset not calibrated** — the `lcd` filter is wired but not tuned against
  side-by-side hardware photos (Game Boy is the obvious case to take seriously).
- **Real-game smoke breadth** — boot-through coverage of known-good titles is
  thin; lock screenshots to catch regressions.
- (PPU/APU/MBC accuracy beyond the CPU oracle isn't separately ledgered here — a
  verification target.)

## Known unknowns / disproven hypotheses

- **Open: PPU/APU/timer accuracy** — the SM83 core is oracle-validated; the rest
  is validated by "games run", not by Blargg/mooneye-style test ROMs yet.
- **Verification targets** — run the mooneye-gb + Blargg Game Boy test suites and
  calibrate `lcd` against `emulators/gameboy/` (SameBoy) references.

## Validated against

- Adam Tennant SM83 corpus (49,600 tests, `~/Projects/Emu198x-Unclean/GameboyCPUTests/v2/`).
- Reference: SameBoy (`emulators/gameboy/`).

## Timing & cycle-accuracy

- **Master clock & dividers** — 4.194304 MHz (DMG). CPU machine-cycle = ÷4
  (1.048576 MHz); PPU + timers run off the master.
- **Timing model realised** — the SM83 is cycle-stepped and the PPU is driven from
  it, but PPU/APU sub-cycle accuracy isn't ledgered beyond "games run" (no
  mooneye/Blargg GB timing suite wired yet).
- **CPU timing** — SM83 cycle-accurate (§62); 49,600 single-step tests prove the
  *instruction set*, not PPU/timer interaction.
- **Distance to full cycle-accuracy** — run mooneye-gb + Blargg GB timing ROMs;
  PPU mode-timing (OAM scan / drawing / hblank) edge cases.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp`.
- **Native window** — yes (primary tier): shared `wgpu` `raw`/`lcd`/`crt`,
  keyboard/gamepad joypad.
- **Disassembler** — pending the Asm198x shared SM83 disassembler.

## Peripherals & connectivity

- **Emulated now** — cartridge (MBC), `.sav` battery RAM, joypad.
- **Period peripherals (emulatable)** — link cable, Game Boy Printer, Game Boy
  Camera, MBC5 rumble.
- **Internet-capable** — **Yes**: the **Mobile Adapter GB** was a real Game Boy
  online service (Japan, 2001) — a documented, emulatable modem peripheral (BGB
  supports it). Link cable covers local multiplayer.

## Crates

| Crate | Role |
|-------|------|
| `sharp-lr35902` | SM83 CPU |
| `machine-nintendo-game-boy` / `runtime-…` / `emu198x-game-boy` | wiring + runner |

## ROMs

No BIOS required (optional DMG boot ROM). Carts (`.gb`) + `.sav` sidecars.

## Launch

```sh
cargo run --release -p emu198x-game-boy -- game.gb
```
