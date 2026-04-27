# Dragon 32/64

## Status: Bring-up started

The Dragon 32/64 is the next expansion target after the initial Spectrum, C64,
NES, Amiga, and Game Boy set. The current repository now has the first reusable
`motorola-6809` CPU foundation crate, a first `machine-dragon-32` substrate,
an initial `runtime-dragon` shell bridge, and a minimal native Dragon 32 window.

## What works

- **CPU:** Motorola 6809 — foundation crate started and now executes far enough
  through the Dragon 32 BIOS to reach ROM polling/delay loops.
- **PIA:** MC6821 — reusable `motorola-pia-6821` crate ported from the VICE
  core shape; DDR/data selection, input/output mixing, control registers, IRQ
  flags, and CA2/CB2 output state are implemented.
- **Harness:** `emu198x-script-dragon` can load a 16KB Dragon ROM, run it against
  the MC6809 core with 32KB RAM, 16KB mirrored ROM mapping, and two MC6821 PIAs.
  PIA/SAM activity and readonly ROM writes are recorded for bring-up analysis.
  Plain `.bin` ROMs and single-ROM `.zip` archives are accepted; the Dragon 32
  BIOS now runs past early PIA/SAM setup into ROM polling/delay loops.
- **SAM:** MC6883 — reusable `motorola-sam-6883` crate tracks the write-only
  set/reset latches needed for bring-up, including VDG mode bits and the F0-F6
  display offset. The Dragon 32 ROM selects text base `$0400` by setting F1.
- **VDG text:** MC6847 — reusable `motorola-vdg-6847` crate captures a 32x16
  alphanumeric diagnostic text snapshot and renders it through the real 6-bit
  MC6847 character font into a 256x192 active-area ARGB framebuffer, or a
  372x243 visible framebuffer with the current coarse MC6847 border. The
  diagnostic palette uses XRoar's default ideal VDG voltage palette for Dragon
  alpha text: green on dark green with a near-black border. This is a stable
  reference palette for diagnostics, not yet an attempt to reproduce the warmer
  appearance of a real Dragon through a consumer display. The Dragon harness
  `--dump-text` path now shows the Dragon 32 BASIC banner and `OK` prompt from
  real ROM execution, while `--dump-text-png PATH` writes the border-inclusive
  text framebuffer as a PNG for visual comparison.
- **Harness keyboard:** PIA0 is wired to the confirmed Dragon 32 keyboard matrix:
  PB0-PB7 drive columns via `$FF02`, and PA0-PA6 read rows via `$FF00`. The
  default state is no key pressed (`$FF` on the input side). `--press KEY`
  holds semantic Dragon keys closed, and `--press-matrix R,C` remains available
  for raw ROM-level probing.
- **Machine crate:** `machine-dragon-32` now owns the reusable board-level
  substrate that was proven in the harness: CPU, RAM/ROM map, PIAs, SAM,
  keyboard matrix, bounded run reporting, and VDG text capture/rendering.
- **Runtime crate:** `runtime-dragon` builds from profile-declared Dragon 32
  BASIC firmware, implements the shared `MachineCore` boundary, emits the
  current MC6847 text-mode framebuffer as RGBA8888, and exposes early Dragon
  state, text-screen, and boot-detection queries. It also has real-ROM headless
  tests that wait for the BASIC `OK` prompt, capture a PNG, and verify BASIC
  keyboard echo.
- **Native shell:** `emu198x-dragon` opens a WGPU window from a Dragon 32 BASIC
  ROM, presents the runtime text framebuffer, and maps host keyboard/gamepad
  controls into Dragon key events. Printable host keys now use logical
  character input rather than physical key positions, and shifted printable
  symbols synthesize Dragon `SHIFT` plus the matching matrix key.
- **CAS format:** `format-dragon-cas` parses Dragon CAS cassette images as
  framed byte-level blocks, exposes checksum validity, and decodes the standard
  15-byte namefile header. Cassette timing and ROM-level playback are still the
  next layer.

## Remaining

### Bring-up sequence

1. Continue `motorola-6809` instruction execution validation against real Dragon ROM paths.
2. Expand `motorola-vdg-6847` beyond alphanumeric text rendering into
   semigraphics and graphics modes.
3. Capture and verify an external reference golden for the Dragon BASIC screen.
4. Build a machine cassette peripheral on top of `format-dragon-cas` so ROM
   `CLOAD`/`CLOADM` sees real cassette input.
5. Add `.BIN` loading after cassette playback is stable.

### Archived Target State

The previous codebase aimed at a fuller Dragon/CoCo implementation. These
features remain useful targets, but are not yet present in the current fresh
workspace:

- **Video:** SG4 semigraphics, all 8 graphics modes, CSS colour-set switching,
  and per-display-mode border behaviour.
- **Cassette:** CAS pulse playback, motor control, and ROM-level
  `CLOAD`/`CLOADM`.
- **Audio:** PIA-driven DAC/cassette/cartridge audio routing and host output.
- **Joystick:** Analogue joystick comparator/DAC behaviour and host mapping.
- **Dragon 64:** SAM all-RAM mode and 64K memory map.
- **Shell:** Save states, rewind, screenshots, auto-CLOAD/CLOADM, and `.BIN`
  auto-EXEC.

### Nice to have
- **Floppy controller** (WD2797) — for DragonDOS disk images
- **SG6/SG8/SG12/SG24** — higher semigraphics modes (require external A/S signal control not wired on Dragon hardware — only SG4 is accessible)
- **Cartridge port** — auto-start ROMs
- **Per-cycle VDG rendering** — current renderer is per-scanline
- **Sound MUX** — PIA0 CA2/CB2 select between DAC, cassette, and cartridge audio sources

## Test coverage

| Component | Tests |
|-----------|-------|
| Machine | 13 (ROM mapping, device access reporting, keyboard, SAM text base, text framebuffer) |
| PIA | 5 (DDR, control, IRQ, input pins, mixed I/O) |
| SAM | 4 (defaults, set/clear, video offset, all-RAM) |
| VDG | 5 (text decode and text rendering) |
| Harness | 8 (CLI, ROM loading, keyboard labels, text dumps) |
| Runtime | 9 (profile metadata, firmware construction, text framebuffer emission, queries, boot status, real-ROM headless screenshot and keyboard echo smoke) |
| Native | 2 (CLI and host key mapping) |
| CAS format | 7 (block framing, header decode, real archive prefix, EOF, checksum visibility, truncation errors) |

## ROMs

Place in `roms/dragon/`:

| File | Size | Description |
|------|------|-------------|
| `dragon32.rom` | 16KB | Dragon 32 BASIC ROM (required) |
