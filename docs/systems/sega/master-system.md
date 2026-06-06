# Sega Master System

## Status: Boots carts to title (Mode 4)

Boots Alex Kidd in Miracle World straight to its title screen. Headless extended
system. Z80 + **Sega VDP** (315-5124/5246, a TMS9918A derivative with Mode 4) +
SN76489.

## What works

- **Sega VDP Mode 4** — 4bpp tiles, dual 16-colour palettes from a 64-colour
  pool, 8 sprites/line, scroll registers, line-interrupt counter, H/V readback.
- **Sega mapper** — `$FFFC-$FFFF` bank registers + cart-RAM control.
- **Cart boot to title** — Alex Kidd (1986, 128K) full Mode 4 title; smoke
  `tests/cart_boot.rs` (first `.sms`). No BIOS required.
- **Game Gear scaffolding** — `SmsVariant::GameGear`, stereo PSG via `$06`.

## Not implemented / accuracy gaps

- **VDP is `tick_scanline()`-only** — 228 T-states batched per line, no per-dot
  tick (more relaxed than `ti-tms9918`). Per-dot model is the next step.
- **Cart RAM (`$8000-$BFFF`)** reads `$FF` — full SRAM persistence needed for
  Phantasy Star, Wonder Boy III, Golvellius.
- **Mapper bank masking** — uses `next_power_of_two()-1`; non-power-of-two cart
  edge behaviour unmodelled.
- **Line-interrupt programmer-side behaviour** (R10 reload + status bit) needs
  validation against split-screen scrollers.
- **YM2413 FM-PAC** — out of scope. **Snapshot** deferred. **No native window.**

## Known unknowns / disproven hypotheses

- **Open: line-interrupt correctness** — wired through `vdp.interrupt`; the
  programmer-visible reload/status path is unverified against real scrolling code.
- **Verification targets** — Mode 4 per-dot timing + mapper masking vs the SMS
  Power / reference cores (`emulators/`).

## Validated against

- Alex Kidd in Miracle World (1986, US) → title; `tests/cart_boot.rs`.

## Timing & cycle-accuracy

- **Master clock & dividers** — 10.738635 MHz NTSC (CPU ÷3 ≈ 3.58) / 10.640 MHz
  PAL (≈3.55). VDP dot ÷2.
- **Timing model realised** — loosest of the TMS9918 lineage: `sega-vdp` exposes
  only **`tick_scanline()`** (228 T-states batched per line, no per-dot tick),
  more relaxed than `ti-tms9918`. Line-interrupt counter wired but
  programmer-side behaviour unvalidated.
- **CPU timing** — Z80 cycle-accurate (§62); no Z80 bus-timing oracle.
- **Distance to full cycle-accuracy** — per-dot VDP model; line-IRQ reload/status
  validation against split-screen scrollers.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp` (operational-parity rollout).
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared Z80 disassembler.

## Peripherals & connectivity

- **Emulated now** — cartridge, controllers; Game Gear variant scaffolding.
- **Period peripherals (emulatable)** — light phaser, 3-D glasses, the card slot,
  the FM sound unit (YM2413), Game Gear gear-to-gear link.
- **Internet-capable** — **Marginal**: the Sega Modem / "Tele-Genesis"-style
  services were Mega Drive era; the SMS had a Japanese modem peripheral but no
  documented device we'd prioritise emulating. A modern flash-cart WiFi path is
  conceivable.

## Crates

| Crate | Role |
|-------|------|
| `zilog-z80` | CPU |
| `sega-vdp` | Mode 4 VDP |
| `ti-sn76489` | PSG |
| `machine-sega-master-system` / `runtime-…` / `emu198x-sega-master-system` | wiring + runner |

## ROMs

No BIOS. Carts (`.sms`) at `~/.emu198x/media/sega-master-system/`.

## Launch

```sh
cargo run --release -p emu198x-sega-master-system -- --cart alexkidd.sms --frames 300 --screenshot sms.png
```
