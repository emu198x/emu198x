# ColecoVision

## Status: Boots BIOS to its title screen

The first donor extraction. The 1982 BIOS reaches "COLECOVISION™ / TURN GAME OFF
/ © 1982 COLECO". Headless extended system. Z80 + TMS9918A + SN76489AN.

## What works

- **Z80 + TMS9918A (VDP) + SN76489AN (PSG)** — pin-driven bus wiring.
- **BIOS boot to title** — smoke `tests/bios_boot.rs` runs 200 frames and asserts
  a non-trivial framebuffer.
- **Correct 3:2 VDP-dot/CPU phase clock** (2026-06-07) — the VDP advances 3 dots
  per 2 Z80 T-states (master 10.738635 MHz, CPU ÷3, VDP dot ÷2), so wall-clock
  speed, audio pitch, and the *moment* a mid-scanline register write takes effect
  are all correct. Replaced the donor's flat 3:1 model (ran 1.5× too fast);
  matches the SG-1000. Regression tests `vdp_runs_at_three_dots_per_two_tstates`
  + `one_frame_of_tstates_is_exactly_one_vdp_frame`.
- **Per-dot VDP render** — each pixel drawn at the dot it is scanned out, so
  mid-scanline register writes land on the correct pixels.

## Not implemented / accuracy gaps

- **IM 1 IntAck** returns `$FF` (matches BIOS `RST 38h`); cart-driven IntAck
  unverified.
- **HCOUNTER** static (not tracked per-dot) — affects only software that reads it.
- **Snapshot** — deferred. **No native window.**

## Known unknowns / disproven hypotheses

- **Verification targets** — IM 1 cart-bus behaviour against ColecoVision
  diagnostics / reference emulators.

## Validated against

- 1982 ColecoVision BIOS → title; `tests/bios_boot.rs`.
- `ti-tms9918` + `ti-sn76489` chip crates (shared, tested).

## Timing & cycle-accuracy

- **Master clock & dividers** — 10.738635 MHz. CPU = ÷3 ≈ 3.579545 MHz; VDP dot =
  ÷2 ≈ 5.369 MHz (real ratio 1.5 dots/CPU cycle).
- **Timing model realised** — the **correct 3:2** VDP-dot-to-T-state phase
  counter (3 dots per 2 T-states, NTSC/PAL frame budgets in Z80 T-states) **and**
  the shared **per-dot** VDP render (`ti-tms9918::tick`). The same model as the
  SG-1000; replaced the donor's 3:1 approximation. Line/frame interrupts and
  mid-scanline writes land at the right phase.
- **CPU timing** — Z80 cycle-accurate (§62); no Z80 bus-timing oracle.
- **Distance to full cycle-accuracy** — small: per-dot HCOUNTER tracking; verify
  IM 1 cart-bus IntAck.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp` (operational-parity rollout).
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared Z80 disassembler.

## Peripherals & connectivity

- **Emulated now** — cartridge, controllers.
- **Period peripherals (emulatable)** — Expansion Module #1 (Atari 2600 carts),
  #2 (driving), Super Action controllers, roller controller; the ADAM expansion
  (#3) turns it into a computer with tape/disk/printer.
- **Internet-capable** — **Marginal**: only via the ADAM expansion's ADAMlink
  modem (period). Bare ColecoVision has no net path.

## Crates

| Crate | Role |
|-------|------|
| `zilog-z80` | CPU |
| `ti-tms9918` / `ti-sn76489` | VDP · PSG |
| `machine-coleco-colecovision` / `runtime-…` / `emu198x-colecovision` | wiring + runner |

## ROMs

BIOS at `~/.emu198x/roms/coleco-colecovision/`; carts separately.

## Launch

```sh
cargo run --release -p emu198x-colecovision -- --frames 200 --screenshot cv.png
```
