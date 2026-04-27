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
- **VDG:** MC6847 — reusable `motorola-vdg-6847` crate captures a 32x16
  alphanumeric diagnostic text snapshot and renders text, SG4/SG6
  semigraphics, and the standard MC6847 full-graphics modes into a 256x192
  active-area ARGB framebuffer, or a 372x243 visible framebuffer with the
  current coarse MC6847 border. That 372x243 output is diagnostic only: it is
  not a PAL beam/overscan model. The diagnostic palette, alpha inverse-video
  handling, MC6847 font alignment, and SG4/SG6 bit ordering are matched against
  patched XRoar's zoomed Dragon output for the exercised modes. The Dragon
  harness `--dump-text` path now shows the Dragon 32 BASIC banner and `OK`
  prompt from real ROM execution, while `--dump-text-png PATH` writes the
  diagnostic border-inclusive text framebuffer as a PNG for visual comparison.
- **Harness keyboard:** PIA0 is wired to the confirmed Dragon 32 keyboard matrix:
  PB0-PB7 drive columns via `$FF02`, and PA0-PA6 read rows via `$FF00`. The
  default state is no key pressed (`$FF` on the input side). `--press KEY`
  holds semantic Dragon keys closed, and `--press-matrix R,C` remains available
  for raw ROM-level probing.
- **Machine crate:** `machine-dragon-32` now owns the reusable board-level
  substrate that was proven in the harness: CPU, RAM/ROM map, PIAs, SAM,
  keyboard matrix, bounded run reporting, VDG text capture, and mode-aware VDG
  rendering.
- **Runtime crate:** `runtime-dragon` builds from profile-declared Dragon 32
  BASIC firmware, implements the shared `MachineCore` boundary, emits the
  current MC6847 framebuffer as RGBA8888, and exposes early Dragon state,
  text-screen, video-state, and boot-detection queries. It also has real-ROM
  headless tests that wait for the BASIC `OK` prompt, capture a PNG, and verify
  BASIC keyboard echo.
- **Native shell:** `emu198x-dragon` opens a WGPU window from a Dragon 32 BASIC
  ROM, presents the runtime text framebuffer, and maps host keyboard/gamepad
  controls into Dragon key events. Printable host keys now use logical
  character input rather than physical key positions, and shifted printable
  symbols synthesize Dragon `SHIFT` plus the matching matrix key.
- **CAS format/media/playback:** `format-dragon-cas` parses Dragon CAS cassette
  images as framed byte-level blocks, exposes checksum validity, and decodes the
  standard 15-byte namefile header. `runtime-dragon` declares a `tape-1`
  cassette slot, mounts CAS media via the shared `MediaSet` path, converts CAS
  blocks into a motor-gated PIA1 cassette input stream, and verifies real-ROM
  `CLOAD` plus `RUN` with Textstar.
- **CAS smoke classification:** `emu198x-script-dragon --smoke-root` classifies
  runtime smoke outcomes as load errors, BASIC errors, visible text changes,
  machine-code auto-runs, video-control changes, blank graphics screens, or
  graphics that continue drawing after the post-start settle window. Reports
  include load/start VDG state and optional screenshots. The
  `--smoke-screenshot-format xroar-zoomed` option writes our active 256x192
  snapshot expanded to XRoar's 512x384 zoomed PNG shape without downscaling;
  this is a temporary validation bridge, not a substitute for proper
  scanline/beam rendering. When supplied with a patched XRoar binary via
  `--xroar-bin` plus `--xroar-reference-dir`, the same smoke run also writes
  deterministic headless XRoar `-vo-picture zoomed` reference PNGs and records
  dimension and pixel-difference summaries for runtime-smoked tapes.

## Remaining

### Bring-up sequence

1. Continue `motorola-6809` instruction execution validation against real Dragon ROM paths.
2. Validate MC6847 graphics and semigraphics rendering against patched-XRoar
   headless reference captures.
3. Replace the current snapshot VDG renderer with proper scanline/beam support:
   HS/FS cadence, active-area placement, borders, mid-frame display changes,
   and reference presentation geometry.
4. Capture and verify an external reference golden for the Dragon BASIC screen.
5. Extend cassette coverage beyond BASIC `CLOAD`: alternate CAS timing/leader
   cases, more real tapes, and better per-title start/input scripts.
6. Add `.BIN` loading after cassette machine-code loading is stable.

### Archived Target State

The previous codebase aimed at a fuller Dragon/CoCo implementation. These
features remain useful targets, but are not yet present in the current fresh
workspace:

- **Video:** external-reference validation, warmer PAL colour tuning, and
  per-display-mode border behaviour.
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
| Machine | 17 (ROM mapping, device access reporting, keyboard, SAM text base, text framebuffer, graphics rendering) |
| PIA | 5 (DDR, control, IRQ, input pins, mixed I/O) |
| SAM | 4 (defaults, set/clear, video offset, all-RAM) |
| VDG | 9 (text decode, text rendering, inverse text, SG4, RG6, and CG6 rendering) |
| Harness | 16 (CLI, ROM loading, keyboard labels, text dumps, smoke options, XRoar-compatible screenshots, XRoar reference comparison, XRoar reference options, smoke classification) |
| Runtime | 18 (profile metadata, firmware construction, framebuffer emission, queries, boot status, CAS mounting/playback, real-ROM headless screenshot, real-CAS mount smoke, Textstar CLOAD/RUN smoke, machine-code CAS smoke, and keyboard echo smoke) |
| Native | 3 (CLI, CAS tape argument, and host key mapping) |
| CAS format | 7 (block framing, header decode, real archive prefix, EOF, checksum visibility, truncation errors) |

## ROMs

Place in `roms/dragon/`:

| File | Size | Description |
|------|------|-------------|
| `dragon32.rom` | 16KB | Dragon 32 BASIC ROM (required) |
