# Nintendo Game Boy

## Status: Native DMG verifier; CPU oracle green

A primary system: native `wgpu` window (`raw`/`lcd`/`crt`), headless cartridge
runner, snapshots, `.sav` battery-RAM sidecars. Sharp LR35902 (SM83) core.

## What works

- **SM83 CPU** — 49,600 Adam Tennant single-step tests pass + 92 lib unit tests.
- **PPU rendering — dmg-acid2 pixel-perfect** (2026-06-07). The definitive DMG
  PPU test renders byte-exact against the published reference (0 diffs / 23,040
  px): BG, window, sprites (8×8 / 8×16), sprite-vs-BG priority, X/Y flip,
  palettes, and the 10-sprite-per-line limit are all correct. Locked as a
  golden-hash regression test.
- **PPU / timer / interrupt / OAM-DMA timing — full mooneye DMG acceptance
  suite passes (75/75)**, run under the matching boot profile per ROM
  (dmg0 / dmgABC / mgb / sgb / sgb2). This includes the hard mode-timing edges
  (`intr_2_mode0_timing_sprites`, `hblank_ly_scx_timing`, STAT write timing).
- **DMG-family** — native window with keyboard/gamepad joypad, screenshots, live
  audio + capture, scripts, snapshots, `.sav` battery RAM.

## Not implemented / accuracy gaps

- **LCD preset not calibrated** — the `lcd` filter is wired but not tuned against
  side-by-side hardware photos (Game Boy is the obvious case to take seriously).
- **APU — blargg `dmg_sound` 9/12** (ledgered 2026-06-07). The three failures are
  all the DMG **wave-RAM-access-while-CH3-on** sub-cycle window (`09-wave read`,
  `10-wave trigger`, `12-wave write` while on): the read/write paths already gate
  on the sample-fetch window, but it needs T-cycle-exact alignment of the CPU
  access against the APU fetch. Allow-listed in `blargg_dmg_sound_known_good`.
- **DMG `oam_bug` fails** — the DMG OAM-corruption quirk (sprite-table glitch on
  certain `inc/dec rr` over `$FE00`) isn't modelled. Rare in real software; most
  emulators skip it.
- **Mealybug-tearoom (mid-scanline PPU)** — the hardest LCDC/scroll-mid-line
  tests aren't run yet (framebuffer-vs-reference; the next PPU frontier).

## Known unknowns / disproven hypotheses

- **Closed (2026-06-07): "PPU accuracy isn't ledgered."** It now is — dmg-acid2
  pixel-perfect + mooneye DMG acceptance 75/75. The open frontier narrowed to the
  APU and the mealybug mid-scanline tests.
- **Verification targets** — run same-suite / blargg `dmg_sound` (APU) and
  mealybug-tearoom (mid-scanline PPU); calibrate `lcd` against
  `emulators/gameboy/` (SameBoy).

## Validated against

- Adam Tennant SM83 corpus (49,600 tests, `~/Projects/Emu198x-Unclean/GameboyCPUTests/v2/`).
- **Mooneye Test Suite** `acceptance/` — 75/75 DMG-family
  (`assets/test-suites/gameboy/mooneye-test-suite/`), env-gated test
  `mooneye_dmg_acceptance_suite_passes`.
- **dmg-acid2** (`assets/test-suites/gameboy/dmg-acid2/`) — pixel-perfect vs
  `reference-dmg.png`, golden-hash test `dmg_acid2_renders_reference`.
- **blargg `dmg_sound`** (`assets/test-suites/gameboy/blargg/`) — 9/12, the 3
  wave-while-on quirks allow-listed; test `blargg_dmg_sound_known_good`.
- Reference: SameBoy (`emulators/gameboy/`).

## Timing & cycle-accuracy

- **Master clock & dividers** — 4.194304 MHz (DMG). CPU machine-cycle = ÷4
  (1.048576 MHz); PPU + timers run off the master.
- **Timing model realised** — SM83 cycle-stepped; the PPU is a per-dot 4-state
  fetcher + pixel FIFO with the mode-3 sprite/scroll penalty modelled, and the
  timer/interrupt interaction is **mooneye-verified** (75/75 acceptance). This is
  one of the most thoroughly ledgered cores in the fleet.
- **CPU timing** — SM83 cycle-accurate (§62); 49,600 single-step tests prove the
  *instruction set*; mooneye proves the PPU/timer/interrupt interaction.
- **Distance to full cycle-accuracy** — APU sub-cycle ledger (same-suite /
  blargg `dmg_sound`); mealybug mid-scanline PPU edge cases.

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
