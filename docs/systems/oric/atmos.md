# Oric-1 / Atmos

## Status: Boots to BASIC `Ready`; keyboard types

A de-facto French home computer (Loriciels, ESAT). Cold-starts cleanly to
`ORIC EXTENDED BASIC V1.1uk / 1983 TANGERINE / 37631 BYTES FREE / Ready`. Headless
extended system. 6502 + VIA-6522 + AY-3-8912 + custom video ULA (inline) — no new
chip crate.

## What works

- **Boot to BASIC** (2026-06-04) with BASIC 1.1 UK (16K, md5 `3202…b629`, MAME
  `oric1`). Boot test asserts the banner in TEXT screen RAM (`$BB80`).
- **Keyboard** — sense on VIA PB3 (column on PB0-2, row mask on port A), 8×8
  matrix wired (2026-06-05); types `HELLO`, RETURN executes.
- **AY-via-VIA wiring** — VIA port A = AY data bus, CA2 = BDIR, CB2 = BC1; software
  drives one of four (BDIR,BC1) modes per AY op.
- **Display** — TEXT + HIRES with serial-attribute rendering, BBC-compatible
  8-colour 3-bit RGB palette.

## Not implemented / accuracy gaps

- **RAM-under-ROM** — 64K allocated, writes reach RAM even at ROM addresses, but
  ROM wins on reads; bank-switching to expose RAM at `$C000-$FFFF` not modelled.
- **TAP cassette loader** — donor has the `.tap` parser; not yet wired into the
  binary.
- **Mid-frame palette across scanlines** — serial attributes work within a line,
  not across scanlines mid-render (end-of-frame render).
- **Snapshot** deferred. **No native window.**

## Known unknowns / disproven hypotheses

- **DISPROVEN (donor): keyboard sense model.** Fixed to sense on VIA PB3 (column
  PB0-2, row mask on port A) and wired the 8×8 matrix (2026-06-05) — the boot
  itself was clean first-try, no code changes.
- **Verification targets** — RAM-under-ROM banking + cross-scanline serial
  attributes against the Oric reference (`emulators/oric/oricutron/`).

## Validated against

- MAME `oric1` BASIC 1.1 ROM → `Ready` (banner asserted in `$BB80`).
- Reference: Oricutron (`emulators/oric/oricutron/`) — IJK joystick + AY-via-VIA.

## Crates

| Crate | Role |
|-------|------|
| `mos-6502` | CPU |
| `mos-via-6522` / `gi-ay-3-8912` | VIA · PSG |
| `machine-oric-atmos` (video ULA inline) / `runtime-…` / `emu198x-oric-atmos` | wiring + runner |

## ROMs

BASIC 1.1 (16K) at `~/.emu198x/roms/oric/oric.rom`.

## Launch

```sh
cargo run --release -p emu198x-oric-atmos -- --frames 200 --screenshot oric.png
```
