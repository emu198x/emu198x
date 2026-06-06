# Mattel Aquarius

## Status: Boots BASIC and plays cartridge games

Renders text correctly and auto-starts cartridge games to their title screens —
Astrosmash, Snafu, Burgertime, Night Stalker, Utopia and Tron all reach play.
Z80A + Microsoft BASIC ROM (8 KB) + separate 2 KB character ROM, 4 KB internal
RAM, optional 16 KB expansion.

## What works

- **Z80A core** — shared `zilog-z80` at 3.579545 MHz.
- **Text rendering** — glyphs from the separate 2 KB character-generator ROM
  (`aq2.u5`), not the BASIC ROM. BASIC boots to "BASIC / Press RETURN".
- **Cartridge auto-start** — the BIOS cart-detect descrambles correctly and
  jumps to `$E010`; six game carts verified to title screens (validated:
  `cart_detect_reaches_cart_entry`, `cart_descrambles_and_renders`).
- **Software-lock scrambler** — port `$FF` sets an 8-bit pattern; all external-bus
  bytes (`$4000-$FFFF`) are XORed with it on read and write. Transparent for RAM,
  descrambling for cart ROM.
- **16 KB cart mapping** — carts map at the top of memory by size (8 KB at
  `$E000`, 16 KB at `$C000`).
- **Controllers** — read through the Mini Expander AY-3-8910 I/O ports.

## Not implemented / accuracy gaps

- **Sound** — the 1-bit speaker (`$FC` / `$FF` bit 0) isn't routed to host audio;
  the runtime emits silent packets. Games bit-bang it (audible boot beep, in-game
  tones) but nothing is heard.
- **Mapper view / port `$FD`** — MAME has a second memory layout (cart at
  `$0000`, BIOS relocated) switched via `$FD`. We implement only the default
  layout; software that switches it is unhandled.
- **Expansion RAM region** — modelled at `$4000-$7FFF`; MAME's external region is
  `$4000-$BFFF`. Not exercised by tested carts.
- **Speaker bit lives on port `$FF`** alongside the scrambler — a likely
  inaccuracy (the cassette/speaker output is port `$FC`); harmless while audio is
  unrouted, worth correcting when audio lands.

## Known unknowns / disproven hypotheses

- **DISPROVEN: "Astrosmash not drawing is a per-game quirk."** It stalled blank
  on *every* 16 KB cart, not just Astrosmash — a system-level bug, the scrambler
  (below). (2026-06-06.)
- **DISPROVEN: "cart-detect fails, so carts fall to BASIC."** Detection actually
  *succeeded*; the corruption was a fictitious per-frame NMI whose `$0066` vector
  sits inside the detect loop. The base Aquarius wires no periodic interrupt (per
  MAME, IRQ/NMI come only from the expansion port). (Fixed `5196c74a`.)
- **Root cause of the blank-cart reboot (resolved):** the missing scrambler XOR.
  The BIOS derives the lock pattern from the cart checksum and `OUT ($FF),A`
  before `JP $E010`; without the XOR applied to reads, the cart entry read as
  garbage (`$39` = `ADD HL,SP; RST $30` → reset) and reboot-looped. (Fixed
  `5196c74a`.)
- **Open: no-cart RUN/playability depth.** BASIC boots to the prompt; typing
  full programs into BASIC isn't exercised here.
- **Open: audio.** Until the speaker is routed, all sound is untested.

## Validated against

- MAME `aquarius.cpp` — `scrambler_w` + the `$4000-$FFFF` bus XOR handlers; the
  expansion-only IRQ/NMI wiring (no VBlank interrupt); the mapper view.
- MAME `aquarius.zip` `aq2.u5` — the standard character generator.
- Six TOSEC cart games (`.../Mattel/Aquarius/Games/[BIN]`) to title screens.

## Timing & cycle-accuracy

- **Master clock & dividers** — Z80A at 3.579545 MHz. **No periodic CPU
  interrupt** — the base machine wires neither IRQ nor NMI (per MAME; only the
  expansion port can assert them). This is load-bearing: a fictitious per-frame
  NMI corrupted cart-detect (fixed this session).
- **Timing model realised** — relaxed: the 40×24 character display renders
  **end-of-frame** (mid-frame char/colour writes show next frame).
- **CPU timing** — Z80 cycle-accurate (§62).
- **Distance to full cycle-accuracy** — per-scanline display; correct external-bus
  (scrambler) timing is already modelled and validated.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp`; `run_until_pc` + the gated cart-boot
  probes used to crack the scrambler this session.
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared Z80 disassembler.

## Peripherals & connectivity

- **Emulated now** — cartridge (with the software-lock scrambler), keyboard,
  Mini-Expander AY controllers.
- **Period peripherals (emulatable)** — the Mini Expander (AY + 2 controller
  ports), 4K/16K RAM cartridges, cassette, the Aquarius printer.
- **Internet-capable** — **No**: a low-end 1983 machine with cassette I/O and no
  period or practical modern net path.

## Crates

| Crate | Role |
|-------|------|
| `zilog-z80` | CPU |
| `general-instrument-ay-3-8910` | Mini Expander PSG + controller ports |
| `machine-mattel-aquarius` | machine wiring (display, scrambler, cart map) |
| `runtime-mattel-aquarius` | shared-shell runtime |
| `emu198x-mattel-aquarius` | headless runner |

## ROMs

- BIOS: `~/.emu198x/roms/mattel-aquarius/aquarius.rom` (8 KB), or
  `$EMU198X_AQUARIUS_BIOS`.
- Character ROM: `~/.emu198x/roms/mattel-aquarius/aquarius-char.rom` (2 KB, from
  MAME `aq2.u5`), or `--char` / `$EMU198X_AQUARIUS_CHAR`.
- Carts: TOSEC `.../Mattel/Aquarius/Games/[BIN]` (zipped).

## Launch

```sh
cargo run --release -p emu198x-mattel-aquarius -- \
  --cart "Astrosmash (1982)(Mattel).bin" --frames 400 --screenshot astro.png
```

Behavioural memory: `project_aquarius_no_periodic_irq.md`.
