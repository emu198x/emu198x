# Memotech MTX500 / MTX512

## Status: Boots to MTX BASIC `Ready`

UK Z80A machine (1983). Boots to the BASIC `Ready` prompt with the OS+BASIC+ASSEM
ROM image. Headless extended system. Z80 + TMS9918A + SN76489 + Z80 CTC.

## What works

- **Boot to BASIC** (2026-06-03) — programs the VDP + CTC and renders `Ready`.
  Smoke `tests/boot_trace.rs` (`boots_to_basic_ready`).
- **VDP interrupt via Z80 CTC** (2026-06-04) — CTC at `$08-$0B`, VDP `/INT` feeds
  **CTC channel 0**, the CTC's INT drives the Z80 IRQ (IM 2 + RETI daisy release).
  The test asserts the OS programs `$08-$0B` and ch0 runs interrupt-enabled.
- **Paging** — OS fixed at `$0000`, 16K RAM blocks page the upper windows
  (`RELCPMH` CP/M mode), per MEMU `mem.c`.
- **Joystick** (2026-06-05, `7d4b09c1`) — directions/fire merged into the
  keyboard sense matrix (ANDed in, per MAME `mtx_key_lo_r`).
- **Keyboard matrix** (2026-06-17, #465) — the physical key→(column, sense-bit)
  grid now matches the hardware (rebuilt from MAME `mtx.cpp`; the prior table was
  a donor placeholder that typed `ABCDE` as `@uf11`). Letters, digits,
  punctuation, modifiers, the four cursor keys (incl. the previously-missing
  Down), the editing keypad, and F1-F8 all resolve to their real cells. Verified
  end-to-end: typing `abcde 12345 print` renders `ABCDE 12345 PRINT`.

## Not implemented / accuracy gaps

- **Cassette / Centronics** unwired. **Snapshot** + `.mtx`/`.run` load — deferred.
- **No native window.**

## Known unknowns / disproven hypotheses

- **DISPROVEN (donor): "port `$00` bit 0 = page 0→RAM."** It swapped the executing
  OS ROM out the instant the power-on RAM-sizing loop wrote 1, derailing into
  zeroed RAM. Rewrote `resolve` after MEMU `mem.c`.
- **DISPROVEN (donor I/O map): SN76489 at `$03`; single keyboard port.** Real
  (MEMU): PSG `$06`; keyboard reads **both** `$05` (sense low) and `$06` (sense
  high + country).
- **DISPROVEN: "OS+BASIC ROM is enough."** A stock board carries OS+BASIC+**ASSEM**;
  the cold-start `RST $28 #$50` runs from the ASSEM ROM (paged subpage 1). With
  OS+BASIC only it hit `$FF` and reset-looped. Needs the 24K image.
- **DISPROVEN (donor): the key→cell grid.** The placeholder laid keys out in a
  tidy sequential 8×8 that bore no relation to the wiring — odd digits in
  column 0, letters alphabetical — so the OS scan decoded the wrong character
  for nearly every key. Rebuilt from MAME `mtx.cpp` (#465).

## Validated against

- MEMU (`github.com/Memotech-Bill/MEMU`) — `mem.c` paging, `memu.c` I/O, `kbd2.c`
  keyboard model. Full map: `knowledge/systems/memotech-mtx.md`.
- `boots_to_basic_ready` (asserts CTC ch0 live).

## Timing & cycle-accuracy

- **Master clock & dividers** — CPU at 4 MHz; VDP at 5.37 MHz via a Bresenham
  counter against the CPU clock; PSG at 4 MHz with internal ÷16.
- **Timing model realised** — TMS9918 now renders **per-dot** (each pixel drawn at
  its dot; `ti-tms9918::tick`); the VDP `/INT` feeds **CTC channel 0** (the CTC
  drives the Z80 IRQ via IM 2). The CTC-as-timebase path is wired and asserted
  live. The VDP-dot-to-CPU phase uses a Bresenham counter (relaxed).
- **CPU timing** — Z80 cycle-accurate (§62); no Z80 bus-timing oracle.
- **Distance to full cycle-accuracy** — exact CTC/VDP-dot phase.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp` (operational-parity rollout).
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared Z80 disassembler.

## Peripherals & connectivity

- **Emulated now** — OS+BASIC+ASSEM ROM, VDP, PSG, CTC, joystick.
- **Period peripherals (emulatable)** — cassette, Centronics printer, the
  Memotech **FDX / HDX** disk systems, and the **node ring network** (Memotech
  shipped a real local network for the MTX).
- **Internet-capable** — **Marginal-to-Yes**: the MTX had a genuine period LAN
  (node network) and RS-232; a modern serial-bridge net path is realistic. One of
  the few 8-bits with native networking heritage.

## Crates

| Crate | Role |
|-------|------|
| `zilog-z80` | CPU |
| `ti-tms9918` / `ti-sn76489` / `zilog-z80-ctc` | VDP · PSG · CTC |
| `machine-memotech-mtx` / `runtime-…` / `emu198x-memotech-mtx` | wiring + runner |

## ROMs

OS + paged-ROM image (OS+BASIC+ASSEM, 24K) at `~/.emu198x/roms/memotech-mtx/`.

## Launch

```sh
cargo run --release -p emu198x-memotech-mtx -- --frames 300 --screenshot mtx.png
```
