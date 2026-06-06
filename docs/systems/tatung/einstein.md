# Tatung Einstein TC-01

## Status: Boots to the MOS prompt; keyboard types

Boots the X-TAL MOS v1.2 to its `Ready` prompt + "insert disc" banner — a usable
built-in monitor, no disk needed. Headless extended system. Z80 + TMS9918A +
AY-3-8910 + WD1770 floppy (inline).

## What works

- **Boot to MOS** (2026-06-04) with the MOS v1.2 ROM (8K, SHA-256 `401d…0ae1`).
- **Keyboard** — AY-driven 8×8 matrix (row select on AY R14/port A, column read
  at `$20`), with a 50 Hz IM2 scan interrupt (2026-06-05). Types `HELLO`.
- **WD1770 FDC** at `$18-$1B` — register interface, Type I seek/restore, sector
  reads from an inserted image, record-not-found when none; `insert_disk` API.

## Not implemented / accuracy gaps

- **OS-from-disk** — needs an Einstein disk image (CP/M / Xtal DOS); none on hand.
- **Z80 CTC** — channel 0 stubbed at `$28`; MOS uses IM 1 so boot doesn't need it,
  but disk-loaded software likely will (CTC crate exists, wiring is port work).
- **TMS9918A scanline-batched render** (shared family debt).
- **Cassette / printer** unwired. **Snapshot** deferred. **No native window.**

## Known unknowns / disproven hypotheses

- **DISPROVEN (donor): "ROM pages out once at `$21`."** The ROM toggles in/out via
  *any* access to port `$24`; the MOS copies ROM→RAM toggling between bytes, and
  the missing `$24` handler left it spinning ~32,000× in the copy loop.
- **DISPROVEN (donor I/O map): keyboard on the CTC/wrong ports.** Real map (MAME):
  keyboard on AY port A(row)/port B(col), `$02`=AY addr/data select, `$03`=AY
  data, `$20`=kbd int mask; the keyboard interrupt is a 50 Hz IM2 device, not the
  CTC.
- **Verification target** — CTC timing for disk-loaded software.

## Validated against

- MAME `tatung/einstein.cpp` — `$24` ROM toggle, WD1770 at `$18-$1B`, INDEX pulse,
  AY-port keyboard. MOS v1.2 → `Ready`.

## Timing & cycle-accuracy

- **Master clock & dividers** — Z80A at ~4 MHz. (Exact crystal/divider tree —
  verify against MAME `tatung/einstein.cpp`.)
- **Timing model realised** — relaxed: TMS9918 **scanline-batched** render; the
  keyboard interrupt is a synthesised 50 Hz IM2 device; WD1770 modelled at the
  register level (Type I + sector reads), not raw MFM bit timing.
- **CPU timing** — Z80 cycle-accurate (§62); no Z80 bus-timing oracle.
- **Distance to full cycle-accuracy** — per-dot VDP; raw-MFM/exact FDC timing;
  CTC channel timing for disk-loaded software.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp` (operational-parity rollout).
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared Z80 disassembler.

## Peripherals & connectivity

- **Emulated now** — built-in WD1770 floppy (sector-dump images via
  `insert_disk`), AY sound, keyboard.
- **Period peripherals (emulatable)** — second floppy, the Tatung "Tatung Pipe"
  expansion bus, printer, cassette, CP/M software stack.
- **Internet-capable** — **Marginal**: RS-232 + CP/M-era networking add-ons
  existed; no documented modern device we'd prioritise emulating.

## Crates

| Crate | Role |
|-------|------|
| `zilog-z80` | CPU |
| `ti-tms9918` / `gi-ay-3-8912` | VDP · PSG (+ keyboard) |
| `machine-tatung-einstein` (WD1770 inline) / `runtime-…` / `emu198x-tatung-einstein` | wiring + runner |

## ROMs

MOS v1.2 (8K) at `~/.emu198x/roms/tatung-einstein/`.

## Launch

```sh
cargo run --release -p emu198x-tatung-einstein -- --frames 300 --screenshot ein.png
```
