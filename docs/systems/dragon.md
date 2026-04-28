# Dragon 32/64

## Status: Early Dragon 32 Usability

Dragon 32 is now a usable early system in the fresh Rust workspace. It boots a
real Dragon 32 BASIC ROM, accepts keyboard input, mounts Dragon CAS cassette
images through the shared runtime media path, can load and start representative
BASIC and machine-code tapes, opens a native `wgpu` verifier window, and has
native CAS autoload plus patched-XRoar screenshot comparison coverage for
cassette smoke runs, and now routes native gamepad input through the Dragon's
analogue joystick comparator path.

Dragon 64, CoCo variants, cartridges, and DragonDOS disk support are still
future work.

## What Works

- **CPU:** `motorola-6809` executes real Dragon 32 ROM and cassette loader paths
  far enough to boot BASIC, load Textstar with `CLOAD`/`RUN`, and start
  machine-code CAS titles with `CLOADM`/`EXEC`.
- **PIA:** `motorola-pia-6821` models DDR/data selection, mixed external pin
  levels, control registers, interrupt flags, and CA2/CB2 output state. Dragon
  PIA0 is wired to the keyboard matrix; PIA1 is wired to cassette input and VDG
  control signals.
- **SAM:** `motorola-sam-6883` tracks the write-only SAM latches used by the
  Dragon ROM and software: VDG mode bits, display offset F0-F6, page mode, MPU
  rate, and all-RAM state.
- **Keyboard:** PIA0 uses the confirmed Dragon 32 keyboard matrix: PB0-PB7
  select columns via `$FF02`, and PA0-PA6 read rows via `$FF00`. The native
  shell maps printable host text semantically, including shifted symbols by
  synthesizing Dragon `SHIFT` plus the matching matrix key. `Backspace` maps to
  `CLEAR`; `F1` maps to `BREAK`.
- **Joystick:** Dragon analogue joystick hardware is wired through PIA0/PIA1:
  PIA0 CB2 selects the port, PIA0 CA2 selects X/Y, PIA1 PA2-PA7 supplies the
  DAC threshold, and the comparator result drives PIA0 PA7. The two fire lines
  pull PIA0 PA0/PA1 low. Native gamepad D-pad/left-stick events currently map
  to joystick 1 axis extremes and South/East maps to fire; true host analogue
  axis values are still pending in the shared input layer. The script runner can
  inject post-start smoke actions with `--smoke-joystick PORT,CONTROL,FRAMES`.
- **VDG:** `motorola-vdg-6847` renders text, inverse text, SG4/SG6
  semigraphics, and standard MC6847 graphics modes. It now exposes full-frame,
  scanline, and byte-position renderers, and `machine-dragon-32` maintains a
  persistent beam-updated framebuffer that samples display memory and PIA1
  VDG-control pins as emulated time advances.
- **Audio:** `machine-dragon-32` now derives 48 kHz mono audio from the Dragon
  PIA sound wiring: PIA1 PA2-PA7 DAC level, PIA0 CA2/CB2 mux source, PIA1 CB2
  mux enable, PIA1 PB1 single-bit sound, and cassette input when the mux selects
  tape. DAC, tape, and single-bit levels are pinned to XRoar's measured-voltage
  gain/offset model; cartridge/AY sources are silent until those expansions
  exist.
- **Runtime:** `runtime-dragon` implements the shared `MachineCore` boundary,
  builds from profile-declared Dragon 32 BASIC firmware, emits RGBA8888 frames
  and mono audio packets, exposes boot/video/PIA/SAM/tape queries, and mounts
  CAS media in slot `tape-1`.
- **Native shell:** `emu198x-dragon` opens a native window, presents the Dragon
  framebuffer through the shared `wgpu` presenter with `raw`/`lcd`/`crt`
  filters, emits live host audio, accepts `--rom`, accepts `--tape`, supports
  `--autoload`, maps keyboard input into Dragon key events, and maps gamepad
  input into Dragon joystick 1.
- **CAS format/media/playback:** `format-dragon-cas` parses framed Dragon CAS
  blocks, exposes checksum validity, and decodes the standard 15-byte namefile
  header. Runtime playback converts CAS blocks into motor-gated cassette input
  pulses consumed by the real ROM loader path.
- **Smoke harness:** `emu198x-script-dragon --smoke-root` classifies real CAS
  loads as load errors, BASIC errors, visible text changes, machine-code
  auto-runs, video-control changes, blank graphics screens, or graphics that
  continue drawing after the post-start settle window. It can write local
  screenshots and patched-XRoar references, then record pixel-difference
  summaries.
- **XRoar comparison:** the current 12-title application smoke batch is 11/12
  exact against patched XRoar. The remaining non-exact case, Dragon Composer,
  differs by capture/timing phase rather than by a static VDG decode error.

## Launch Commands

Native window:

```sh
cargo run --release -p emu198x-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --tape game.cas \
  --autoload \
  --video crt
```

Headless smoke over one cassette tree:

```sh
cargo run --release -q -p emu198x-script-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --smoke-root '/path/to/Dragon/Applications/[CAS]' \
  --smoke-run-limit 12 \
  --smoke-report target/dragon-smoke.json \
  --smoke-screenshot-dir target/dragon-smoke-screens \
  --smoke-screenshot-format xroar-zoomed \
  --smoke-audio-dir target/dragon-smoke-audio \
  --smoke-joystick 2,fire,300
```

Patched-XRoar comparison, when the local patched XRoar binary is available:

```sh
cargo run --release -q -p emu198x-script-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --smoke-root '/path/to/Dragon/Applications/[CAS]' \
  --smoke-run-limit 12 \
  --smoke-report target/dragon-xroar-smoke.json \
  --smoke-screenshot-dir target/dragon-xroar-smoke \
  --smoke-screenshot-format xroar-zoomed \
  --xroar-bin ../Emu198x-Unclean/xroar/src/xroar \
  --xroar-reference-dir target/dragon-xroar-reference
```

## Current Gaps

1. Audio now follows the Dragon PIA DAC/mux/single-bit/cassette signal path and
   uses XRoar's measured level model, but it does not yet model analogue
   filtering, cartridge audio, or AY expansion audio.
2. Joystick hardware now follows the Dragon comparator/DAC behavior, but the
   host input surface still only exposes thresholded gamepad directions rather
   than continuous analogue axis values.
3. The beam framebuffer is in place, but the display model is still calibrated
   to the current 372x243 diagnostic visible area and XRoar zoomed comparison
   bridge. A fuller PAL timing/overscan model can come later.
4. Dragon 64 memory mode, cartridge ROMs, `.BIN` convenience loading, and
   DragonDOS/WD2797 disk support are not implemented.

## Near-Term Plan

1. Validate Dragon audio filtering and audible software behavior against XRoar
   or hardware captures once we have sound-producing CAS fixtures.
2. Extend the shared input layer with true analogue axis events so Dragon
   joysticks can receive continuous gamepad stick positions instead of only
   digital extremes.
3. Revisit PAL geometry and external video reference captures after the current
   practical usability loop is smoother.

## Test Coverage

| Component | Tests |
|-----------|-------|
| Machine | ROM mapping, device access reporting, keyboard, cassette input, analogue joystick comparator/fire wiring, SAM text base, text framebuffer, graphics rendering, XRoar-pinned PIA DAC/tape/single-bit audio |
| PIA | 5: DDR, control, IRQ, input pins, mixed I/O |
| SAM | 4: defaults, set/clear, video offset, all-RAM |
| VDG | 13: text decode/rendering, inverse text, SG4, RG6, CG6, scanline rendering, byte-position rendering |
| Harness | 16: CLI, ROM loading, keyboard labels, text dumps, smoke options, XRoar-compatible screenshots, XRoar reference comparison, smoke classification |
| Runtime | Profile metadata, firmware construction, framebuffer/audio emission, queries, boot status, CAS mounting/playback, joystick button-to-hardware mapping, real-ROM screenshot, Textstar CLOAD/RUN, machine-code CAS smoke, keyboard echo |
| Native | CLI, CAS tape argument, CAS autoload command selection, real Textstar autoload smoke, host key mapping, gamepad-to-joystick mapping |
| CAS format | 7: block framing, header decode, real archive prefix, EOF, checksum visibility, truncation errors |

## ROMs

Place the Dragon 32 BASIC ROM at:

| File | Size | Description |
|------|------|-------------|
| `~/.emu198x/roms/dragon/dragon32.rom` | 16KB | Dragon 32 BASIC ROM |

The native and script runners also accept a zip containing one suitable
ROM/bin candidate.
