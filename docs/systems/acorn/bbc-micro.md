# BBC Micro

## Status: Early boot (headless) — OS bank-scan reaches the BASIC slot

A donor-extracted extended system with Capture + Script + MCP parity but no
native window yet. The OS bank-scan reaches the BASIC ROM slot; full boot needs
the SAA5050 Teletext generator and a BASIC II image. MOS 6502A + 6845 CRTC +
SAA5050 + SN76489 + two 6522 VIAs.

## What works

- **6502 core** — shared `mos-6502` (validated elsewhere: Tom Harte 100%).
- **OS bank-scan** — reaches the BASIC slot (sideways ROM paging works far
  enough to find BASIC).
- **Operational parity** — Capture (screenshot/WAV) + Script + MCP, per the
  2026-06-02 donor rollout.

## Not implemented / accuracy gaps

- **SAA5050 Teletext** — MODE 7 character generator not implemented; blocks full
  boot/display.
- **BASIC II ROM** — needs the correct image to reach the `>` prompt.
- **6845 CRTC custom modes** — BBC's video ULA logic around the 6845 (MODE 0–6
  timing) not validated.
- **SN76489 audio, 6522 VIA timers, DFS (.SSD/.DSD)** — present-or-planned, not
  validated.
- **No native window** — shared remaining surface for the whole extended tier.

## Known unknowns / disproven hypotheses

- **Open: how far is the boot, really?** "Reaches the BASIC slot" is from the
  bank-scan; the gap to a typed `>` prompt (SAA5050 + BASIC II) is scoped but
  unquantified.
- **Verification targets** — clock model (16 MHz ÷8/÷4 video-access), CRTC mode
  timings, and the video ULA behaviour are from secondary knowledge; confirm
  against the BBC Advanced User Guide / primary Acorn docs.

## Validated against

- 6502 core — Tom Harte (shared crate).
- (No BBC-specific reference cross-check recorded yet — a verification target.)

## Crates

| Crate | Role |
|-------|------|
| `mos-6502` | CPU (shared) |
| `machine-acorn-bbc-micro` | machine wiring |
| `runtime-acorn-bbc-micro` | shared-shell runtime |
| `emu198x-acorn-bbc-micro` | headless runner |

(Remaining chip crates — 6845 CRTC, SAA5050, SN76489, 6522 — per `crates/`.)

## ROMs

| File | Size | Description |
|------|------|-------------|
| OS 1.2 | 16 KB | MOS ROM |
| BASIC II | 16 KB | language ROM (needed for `>` prompt) |
| DFS | 16 KB | disk filing system (optional) |

## Launch

```sh
cargo run --release -p emu198x-acorn-bbc-micro -- --frames 300 --screenshot bbc.png
```
