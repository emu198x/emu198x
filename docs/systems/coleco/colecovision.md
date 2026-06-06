# ColecoVision

## Status: Boots BIOS to its title screen

The first donor extraction. The 1982 BIOS reaches "COLECOVISION™ / TURN GAME OFF
/ © 1982 COLECO". Headless extended system. Z80 + TMS9918A + SN76489AN.

## What works

- **Z80 + TMS9918A (VDP) + SN76489AN (PSG)** — pin-driven bus wiring.
- **BIOS boot to title** — smoke `tests/bios_boot.rs` runs 200 frames and asserts
  a non-trivial framebuffer.

## Not implemented / accuracy gaps

- **Clock ratios (initial-port)** — VDP runs 3 dots/CPU cycle; the real ratio is
  1.5 (master 10.738635 MHz, CPU ÷3, VDP dot ÷2). Frame structure completes but
  wall-clock speed is off. SG-1000 already has the correct 3:2 counter — port it.
- **TMS9918A VDP-dot/CPU phase** — the VDP now renders **per-dot** (mid-scanline
  register writes land on the correct pixels), but the VDP-dot-to-CPU-cycle phase
  is still the relaxed 3:1 ratio (see clock-ratios above), so the *moment* a write
  takes effect can still be off by a few dots.
- **IM 1 IntAck** returns `$FF` (matches BIOS `RST 38h`); cart-driven IntAck
  unverified.
- **Snapshot** — deferred. **No native window.**

## Known unknowns / disproven hypotheses

- **Open: clock fidelity** — the 3:1 ratio is a known initial-port approximation,
  not validated; the fix (3:2) is mechanical once the model is comfortable.
- **Verification targets** — VDP per-dot timing + IM 1 cart-bus behaviour against
  ColecoVision diagnostics / reference emulators.

## Validated against

- 1982 ColecoVision BIOS → title; `tests/bios_boot.rs`.
- `ti-tms9918` + `ti-sn76489` chip crates (shared, tested).

## Timing & cycle-accuracy

- **Master clock & dividers** — 10.738635 MHz. CPU = ÷3 ≈ 3.579545 MHz; VDP dot =
  ÷2 ≈ 5.369 MHz (real ratio 1.5 dots/CPU cycle).
- **Timing model realised** — VDP render is now **per-dot** (each pixel drawn at
  the dot it is scanned out; see `ti-tms9918::tick`). The remaining debt is the
  VDP-dot-to-CPU-cycle *phase*: the initial port runs the VDP at **3:1** (3× too
  fast) with NTSC/PAL frame budgets in CPU cycles. SG-1000 already carries the
  correct 3:2 counter — porting it is the fix.
- **CPU timing** — Z80 cycle-accurate (§62); no Z80 bus-timing oracle.
- **Distance to full cycle-accuracy** — correct the 3:2 dot ratio (per-dot VDP
  render landed); verify IM 1 cart-bus IntAck.

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
