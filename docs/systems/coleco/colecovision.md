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
- **TMS9918A scanline-batched render** — full scanline on dot-wrap, misses
  mid-scanline register writes (shared family debt).
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
