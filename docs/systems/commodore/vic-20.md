# Commodore VIC-20

## Status: Boots to READY; joystick validated; PRG autoload

The VIC-20 boots BASIC to READY, reads the joystick correctly at the register
level, and can auto-load a PRG program from the headless runner. 6502 + VIC
(video + sound) + two 6522 VIAs.

## What works

- **6502 core** — shared `mos-6502`.
- **Boot to READY** — KERNAL/BASIC/char ROMs load and the machine reaches the
  BASIC prompt.
- **Joystick** — up/down/left/fire on VIA #1 port A (`$9111`, bits PA2–PA5),
  right on VIA #2 port B (`$9120`, PB7), all active-low. Layout is bit-exact for
  the standard VIC-20 joystick (validated: `joystick_probe`, commit `76422c1c`).
  Reading "right" requires DDRB bit 7 set to input — the probe pokes `$9122`
  before reading, mirroring how software must.
- **PRG autoload** — `--prg` injects the program at its load address, fixes the
  BASIC text pointers (`$2D`/`$2F`/`$31`), and queues `RUN` in the keyboard
  buffer once BASIC reaches READY. `--prg-sys` queues `SYS<load>` instead, for
  machine-code PRGs (commit `d22538b8`).

## Not implemented / accuracy gaps

- **Cartridge auto-start** — not exercised. Game cart boot is unverified.
- **RAM expansion configs** — autoload assumes the unexpanded layout (TXTTAB
  `$1001`) unless told otherwise; +3K/+8K/+16K block handling isn't
  systematically tested. (The +8K config moves TXTTAB to `$1201`.)
- **VIC video/audio accuracy** — renders and boots, but timing, raster effects,
  and the VIC's audio channels are not validated against hardware or test ROMs.
- **VIA timers / shift register** — present for I/O; full timer/SR accuracy
  unverified.

## Known unknowns / disproven hypotheses

- **DISPROVEN: "the joystick 'right' wiring is broken."** The register probe's
  "right" read failed only because DDRB bit 7 defaults to *output* at READY; with
  the data direction set to input (as real software does) the wiring is correct.
  The model was right; the test setup was incomplete. (Commit `76422c1c`.)
- **Note: machine-code PRGs need SYS, not RUN.** A BASIC `RUN` of a machine-code
  PRG throws `?SYNTAX ERROR` (a real PRG `rachel.prg` did exactly this) — hence
  `--prg-sys`. Recorded so the failure mode isn't re-diagnosed.
- **Open: cartridge software.** No cart has been booted end-to-end; whether the
  cart mapping and any auto-start vector work is unknown.
- **Open: VIC raster/audio fidelity.** No raster-timing or SID-equivalent audio
  validation has been done.

## Validated against

- Standard VIC-20 joystick wiring (up=PA2, down=PA3, left=PA4, fire=PA5;
  right=PB7) — gated `joystick_probe`.
- BASIC READY boot via the KERNAL/BASIC ROMs.

## Timing & cycle-accuracy

- **Master clock & dividers** — the VIC (6560/6561) generates the system clock:
  CPU ≈ 1.108 MHz PAL / 1.0227 MHz NTSC (from the 4.43/14.31 MHz colour crystal).
- **Timing model realised** — relaxed vs the C64: `mos-vic-i` renders the
  character display, but not the cycle-exact, bus-visible per-pixel VIC-II model.
- **CPU timing** — 6502 cycle-accurate (§62).
- **Distance to full cycle-accuracy** — per-cycle VIC rendering + bus visibility;
  VIC audio (3 tone + noise).

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp`; `--prg`/`--prg-sys` autoload; gated
  `joystick_probe` register validation.
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared 6502 disassembler.

## Peripherals & connectivity

- **Emulated now** — joystick (validated), PRG autoload.
- **Period peripherals (emulatable)** — 1540/1541 drive, datasette, printers, RAM
  expansion cartridges, paddles.
- **Internet-capable** — **Yes**: the **VICMODEM** (1982) was the first modem
  under $100 — the VIC-20 put a generation online via the RS-232 user port. Modern
  emulatable kit: WiModem232 and user-port Ethernet. A flagship net story.

## Crates

| Crate | Role |
|-------|------|
| `mos-6502` | CPU |
| `machine-commodore-vic-20` | machine wiring (VIC, VIAs, memory map) |
| `runtime-commodore-vic-20` | shared-shell runtime (+ `autoload_prg`) |
| `emu198x-commodore-vic-20` | headless runner (`--prg` / `--prg-sys`) |

## ROMs

- KERNAL / BASIC / character ROMs at `~/.emu198x/roms/commodore-vic-20/`
  (or `$EMU198X_VIC20_KERNAL` / `_BASIC` / `_CHAR`). Also in the April-2026
  archive at `~/Projects/Emu198x-archive-april2026/roms/c64/`-adjacent paths.

## Launch

```sh
cargo run --release -p emu198x-commodore-vic-20 -- \
  --prg hello.prg --screenshot hello.png            # BASIC PRG, auto-RUN
cargo run --release -p emu198x-commodore-vic-20 -- \
  --prg-sys game.prg --screenshot game.png          # machine-code PRG, auto-SYS
```
