# Dragon 32/64

## Status: Early Dragon 32 Usability

Dragon 32 is now a usable early system in the fresh Rust workspace. It boots a
real Dragon 32 BASIC ROM, accepts keyboard input, mounts Dragon CAS cassette
images, ROM/DGN cartridges, and PC-Dragon PAK snapshots through the shared
runtime media path, can load and start representative BASIC and machine-code
tapes, opens a native `wgpu` verifier window, and has native CAS autoload plus
patched-XRoar screenshot comparison coverage for cassette smoke runs, and now
routes native gamepad input through the Dragon's analogue joystick comparator
path.

Dragon 64, CoCo variants, cartridge audio, and DragonDOS disk support are still
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
  rate, and all-RAM state. The Dragon machine keeps those immediate latches
  separate from the VDG-effective display base, which updates on frame-sync
  fall as documented by the SAM timing notes.
- **Keyboard:** PIA0 uses the confirmed Dragon 32 keyboard matrix: PB0-PB7
  select columns via `$FF02`, and PA0-PA6 read rows via `$FF00`. The native
  shell maps printable host text semantically, including shifted symbols by
  synthesizing Dragon `SHIFT` plus the matching matrix key. `Backspace` maps to
  `CLEAR`; `F1` maps to `BREAK`.
- **Joystick:** Dragon analogue joystick hardware is wired through PIA0/PIA1:
  PIA0 CB2 selects the port, PIA0 CA2 selects X/Y, PIA1 PA2-PA7 supplies the
  DAC threshold, and the comparator result drives PIA0 PA7. The two fire lines
  pull PIA0 PA0/PA1 low. Native gamepad D-pad events still map to joystick 1
  axis extremes, while left-stick motion now feeds continuous host analogue
  axis values into the Dragon comparator path. South/East maps to fire. The
  script runner can inject post-start smoke actions with
  `--smoke-joystick PORT,CONTROL,FRAMES` and analogue comparator stimulus with
  `--smoke-joystick-axis PORT,AXIS,VALUE,FRAMES`, where `VALUE` is normalized
  from -1.0 to 1.0. It can also sweep comparator values with
  `--smoke-joystick-axis-sweep PORT,AXIS,START,END,STEPS,FRAMES`, recording
  per-step visible output changes in the smoke report. It can capture
  same-duration idle baselines with `--smoke-idle-after-start`.
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
- **Cartridge/snapshot media:** `format-dragon-pak` normalises Dragon ROM/DGN
  cartridge images using XRoar-compatible header skipping for
  non-256-byte-aligned files, and parses PC-Dragon PAK snapshots as restored
  machine state. The runtime mounts cartridge media in `cartridge-1` and
  snapshot media in `snapshot-1`; plain ROM cartridges overlay `$C000-$FEFF`,
  and images larger than 16 KiB use Games Master Cartridge-style 16 KiB banking
  through the cartridge I/O range.
- **Runtime:** `runtime-dragon` implements the shared `MachineCore` boundary,
  builds from profile-declared Dragon 32 BASIC firmware, emits RGBA8888 frames
  and mono audio packets, exposes boot/video/PIA/SAM/tape queries, and mounts
  CAS media in slot `tape-1`, cartridge media in slot `cartridge-1`, and
  PC-Dragon PAK snapshots in slot `snapshot-1`.
- **Native shell:** `emu198x-dragon` opens a native window, presents the Dragon
  framebuffer through the shared `wgpu` presenter with `raw`/`lcd`/`crt`
  filters, emits live host audio, accepts `--rom`, `--tape`, `--cart`, and
  `--snapshot`, supports `--autoload`, maps keyboard input into Dragon key
  events, and maps gamepad input into Dragon joystick 1.
- **CAS format/media/playback:** `format-dragon-cas` parses framed Dragon CAS
  blocks, exposes checksum validity, and decodes the standard 15-byte namefile
  header. Runtime playback converts CAS blocks into motor-gated cassette input
  pulses consumed by the real ROM loader path.
- **Smoke harness:** `emu198x-script-dragon --smoke-root` classifies real CAS
  loads as load errors, BASIC errors, visible text changes, machine-code
  auto-runs, video-control changes, blank graphics screens, or graphics that
  continue drawing after the post-start settle window. The regular Backgammon
  audio smoke writes a WAV capture and verifies active 48 kHz mono output with
  multiple levels and sustained transitions. `--snapshot-smoke-root` scans
  PC-Dragon PAK snapshots, resumes each selected snapshot, classifies
  running/halting and visible/blank output, writes a deterministic
  `trace_signature` over CPU fetches, VDG samples, VDG mode writes, video
  phase, text, and framebuffer data, and can write diagnostic or XRoar-zoomed
  screenshots. CAS smoke can write patched-XRoar references after ROM tape-load
  traps; PAK smoke can still produce patched-XRoar references, but the regular
  verifier now uses repeated internal trace signatures as the stable PAK
  alignment gate.
- **Trace probes:** `emu198x-script-dragon` can retain bounded opcode-fetch
  and bus-write traces. `--watch-fetch A[-B]` and `--watch-write A[-B]` may be
  repeated, which lets investigations correlate state variables, framebuffer
  writes, and VDG fetch samples in one deterministic run.
- **XRoar comparison:** the current 12-title application smoke batch is 11/12
  exact against patched XRoar. The remaining non-exact case, Dragon Composer,
  differs by capture/timing phase rather than by a static VDG decode error.
  PAK-vs-XRoar snapshot comparisons are advisory: the PC-Dragon-to-XRoar
  snapshot bridge does not restore enough CPU/PIA/SAM/event state to be a hard
  reference after the initial resumed frame. PAK regression coverage now comes
  from deterministic internal trace-signature comparisons.

## Launch Commands

Native window:

```sh
cargo run --release -p emu198x-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --cart cartridge.dgn \
  --snapshot game.pak \
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
  --smoke-audio-dir target/dragon-smoke-audio \
  --smoke-joystick 2,fire,300 \
  --smoke-joystick-axis 1,x,0.5,300 \
  --smoke-joystick-axis-sweep 1,x,-1.0,1.0,5,120 \
  --smoke-idle-after-start 300
```

Known joystick comparator fixture:

```sh
cargo run -p emu198x-script-dragon -- \
  --rom '/Users/stevehill/Projects/Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Firmware/Dragon Data Dragon 32 BIOS (1982)(Dragon Data).zip' \
  --smoke-root '/Users/stevehill/Projects/Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Applications/[CAS]/Joystick Test (198x)(-).zip' \
  --smoke-run-limit 1 \
  --smoke-report target/dragon-joystick-sweep.json \
  --smoke-screenshot-dir target/dragon-joystick-sweep \
  --smoke-joystick-axis-sweep 1,x,-1.0,1.0,5,120 \
  --smoke-joystick-axis-sweep 1,y,-1.0,1.0,5,120 \
  --smoke-idle-after-start 120
```

This archived CAS fixture is also part of
`scripts/verify-current-systems.sh --local-only` when
`EMU198X_DRAGON_JOYSTICK_CAS` or the standard reference archive path is
available. It loads as `JOY TEST`, starts via `RUN`, reaches a stable idle
frame, then reports visible changes for comparator sweep points on both X and Y
axes. The 2026-05-01 run produced valid CAS checksums,
`classification=started-text-drawing`, `idle_visible_change=false`, and
`joystick_visible_change=true`.

The longer joystick-vs-idle game smoke is intentionally opt-in via
`EMU198X_DRAGON_JOYSTICK_GAME_CAS`. The archived Frogger CAS is useful as a
manual regression, but it is a `CLOADM` tape of roughly 368k bits and the smoke
loads it twice, so it is too slow for the routine local gate.

Headless smoke over one PC-Dragon PAK snapshot tree:

```sh
cargo run --release -q -p emu198x-script-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --snapshot-smoke-root '/path/to/Dragon/Games/[PAK]' \
  --smoke-run-limit 32 \
  --cycles 200000 \
  --smoke-report target/dragon-pak-smoke.json \
  --smoke-screenshot-dir target/dragon-pak-smoke-screens \
  --smoke-screenshot-format xroar-zoomed \
  --xroar-bin ../Emu198x-Unclean/xroar/src/xroar \
  --xroar-reference-dir target/dragon-pak-xroar-reference \
  --xroar-settle-seconds 0.2
```

The regular local verifier runs deterministic PAK trace-alignment checks when
`EMU198X_DRAGON_PAK` or the standard reference archive is available. With no
override, it uses Skramble, Doodle Bug, and Hunchback as a compact curated set;
each snapshot is run twice with completed-frame capture and compared by the
reported `trace_signature`. The signature includes retained CPU fetches, VDG
samples, VDG mode writes, video phase, text, and framebuffer data. The curated
entries also assert `running-visible`, minimum colour counts, required VDG mode
writes where expected, and known-good signatures:

| PAK | Signature |
| --- | --- |
| Skramble | `fede4df2995a9500` |
| Doodle Bug | `5779963295a0d25d` |
| Hunchback | `ec699c8d30c606e5` |

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

This comparison is explicit opt-in through `EMU198X_XROAR_BIN` in
`scripts/verify-current-systems.sh`. It is kept as a regression aid only, and
`xroar-zoomed` screenshots are normalized from the current PAL overscan
framebuffer into XRoar's 512x384 active-area reference size before comparison.
Textstar is exact through this path, but XRoar remains an opt-in regression aid
rather than a timing or accuracy authority.

Focused trace probe for a running PAK snapshot:

```sh
cargo run --release -q -p emu198x-script-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --snapshot game.pak \
  --cycles 120000 \
  --watch-write 0x0088-0x0089 \
  --watch-write 0x1fed \
  --watch-fetch 0x1fe0-0x1fff \
  --trace-limit 80
```

## Current Gaps

1. Audio now follows the Dragon PIA DAC/mux/single-bit/cassette signal path and
   uses XRoar's measured level model. The local Backgammon audio gate now checks
   for active mono 48 kHz output with multiple levels and sustained transitions,
   but the emulator does not yet model analogue filtering, cartridge audio, or
   AY expansion audio.
2. Native gamepad left-stick movement and Dragon script smoke runs can now feed
   continuous analogue axis values into the Dragon comparator path.
3. PAK-vs-XRoar screenshots remain advisory because the synthetic XRoar
   snapshot import path is not a hard state reference after resume. The regular
   PAK gate now compares deterministic internal trace signatures instead.
4. The beam framebuffer is in place, but the display model is still calibrated
   to the current 372x243 diagnostic visible area and XRoar zoomed comparison
   bridge. A fuller PAL timing/overscan model can come later.
5. Dragon 64 memory mode, cartridge audio, `.BIN` convenience loading, and
   DragonDOS/WD2797 disk support are not implemented.

For the source-backed accuracy audit and implementation sequence, see
[`dragon-accuracy-audit.md`](dragon-accuracy-audit.md).

## Near-Term Plan

1. Extend the deterministic PAK trace-alignment set only when a new fixture
   proves a distinct behaviour that the current Skramble, Doodle Bug, and
   Hunchback set does not cover.
2. Add source-backed analogue filtering once we have Dragon circuit references
   or hardware captures; keep the current Backgammon activity gate as a runtime
   regression check, not as proof of analogue accuracy.
3. Add a synthetic comparator fixture only if we need deterministic text
   assertions independent of archived `JOY TEST` media availability.
4. Revisit PAL geometry and external video reference captures after the current
   practical usability loop is smoother.

## Completion Assessment

Dragon 32 is at a practical-use baseline: real BASIC ROM boot, keyboard, CAS
autoload, cartridges, PAK snapshots, beam-updated VDG output, cassette input,
PIA/SAM timing, mono audio, native windowing, gamepad input, and smoke tooling
are all in place. The remaining work to call it complete is accuracy and
coverage rather than initial bring-up.

To complete Dragon 32, the main gaps are: full source-backed MC6809 timing and
interrupt edge cases, PAL video geometry calibrated against hardware captures,
analogue audio filtering and expansion-source mixing, Dragon 64 memory mode,
DragonDOS/WD2797 disks, `.BIN` convenience loading, and a trusted fixture suite
for joystick/audio/video behaviours that does not depend on emulator-vs-emulator
pixel matching.

## Test Coverage

| Component | Tests |
|-----------|-------|
| Machine | ROM mapping, cartridge ROM/GMC overlay, device access reporting, keyboard, cassette input, analogue joystick comparator/fire wiring, SAM text base, frame-sync-delayed VDG display base, source-backed VDG byte-fetch timing, text framebuffer, graphics rendering, XRoar-pinned PIA DAC/tape/single-bit audio |
| PIA | 12: DDR, control, IRQ, input pins, mixed I/O, Cx1 edge selection, Cx2 input/output, Cx1-restored Cx2 strobe modes |
| SAM | 4: defaults, set/clear, video offset, all-RAM |
| VDG | 16: source horizontal geometry/crop split, text decode/rendering, inverse text, SG4, RG6, CG6, scanline rendering, byte-position rendering |
| Harness | 23: CLI, ROM loading, keyboard labels, text dumps, direct screenshots, CAS smoke options, PAK snapshot smoke, XRoar-compatible screenshots, XRoar reference comparison, smoke classification |
| Runtime | Profile metadata, firmware construction, framebuffer/audio emission, queries, boot status, CAS mounting/playback, cartridge mounting, PAK snapshot mounting, joystick button-to-hardware mapping, real-ROM screenshot, Textstar CLOAD/RUN, machine-code CAS smoke, keyboard echo |
| Native | CLI, CAS tape argument, cartridge argument, snapshot argument, CAS autoload command selection, real Textstar autoload smoke, host key mapping, gamepad-to-joystick mapping |
| CAS format | 7: block framing, header decode, real archive prefix, EOF, checksum visibility, truncation errors |
| PAK format | 4: aligned ROM images, XRoar-style cartridge header skip, empty image rejection, PC-Dragon snapshot decode |

## ROMs

Place the Dragon 32 BASIC ROM at:

| File | Size | Description |
|------|------|-------------|
| `~/.emu198x/roms/dragon/dragon32.rom` | 16KB | Dragon 32 BASIC ROM |

The native and script runners also accept a zip containing one suitable
ROM/bin candidate.
