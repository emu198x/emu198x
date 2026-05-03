# Dragon 32/64

## Status: Early Dragon 32/64 Usability

Dragon 32 is now a usable early system in the fresh Rust workspace. It boots
real Dragon BASIC ROMs, accepts keyboard input, mounts Dragon CAS cassette
images, DragonDOS `.BIN` programs, DragonDOS VDK disk images, ROM/DGN
cartridges, and PC-Dragon PAK snapshots through the shared runtime media path,
can load and start
representative BASIC and machine-code tapes, opens a native `wgpu` verifier
window, and has native CAS autoload plus patched-XRoar screenshot comparison
coverage for cassette smoke runs, and now routes native gamepad input through
the Dragon's analogue joystick comparator path.

Dragon 64 is represented as a distinct PAL runtime profile. It cold-boots in
the real hardware's Dragon 32-compatible mode, adds the Dragon 64 ACIA decode
at `$FF04-$FF07`, keeps the SAM-backed 64K RAM paging model, and now supports
the native `EXEC 48000` transition into usable 64-mode BASIC. PIA1 PB2 selects
between the compatible and 64-mode internal BASIC ROMs at `$8000-$BFFF`, while
SAM map type 1 exposes RAM below the `$FFxx` device/vector page. The runtime
rejects obviously bad Dragon 64 firmware pairs, including duplicated ROMs and
known swapped compatible/mode ROM CRCs, without requiring every alternate valid
dump to match a catalogued CRC. CoCo variants and cartridge expansion hardware
beyond the documented audio input pin remain future work. DragonDOS disk
support is present as an initial VDK sector-read controller path, not yet as a
complete WD2797 timing/write implementation.

## What Works

- **CPU:** `motorola-6809` executes real Dragon ROM and cassette loader paths
  far enough to boot BASIC, load Textstar with `CLOAD`/`RUN`, and start
  machine-code CAS titles with `CLOADM`/`EXEC`.
- **PIA/ACIA:** `motorola-pia-6821` models DDR/data selection, mixed external pin
  levels, control registers, interrupt flags, and CA2/CB2 output state. Dragon
  PIA0 is wired to the keyboard matrix; PIA1 is wired to cassette input and VDG
  control signals. Dragon 64 also decodes a minimal 6551-compatible ACIA stub
  at `$FF04-$FF07`, currently enough for no-serial-device cold boot with the
  transmit-ready status bit.
- **SAM:** `motorola-sam-6883` tracks the write-only SAM latches used by the
  Dragon ROM and software: VDG mode bits, display offset F0-F6, page mode, MPU
  rate, memory size, and map type. The Dragon machine keeps those immediate
  latches separate from the VDG-effective display base, which updates on
  frame-sync fall as documented by the SAM timing notes. SAM page select now
  switches the low 32 KiB RAM page in map type 0, and TY map type 1 maps MPU
  reads/writes through RAM below the `$FFxx` device/vector page.
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
  tape. The documented expansion connector pin 35 `SND` input is exposed as a
  normalized cartridge sound level when the mux selects the cartridge source.
  DAC, tape, cartridge input, and single-bit levels are pinned to XRoar's
  measured-voltage gain/offset model; the fourth mux input remains unused and
  silent.
- **Cartridge/program/snapshot media:** `format-dragon-bin` parses DragonDOS
  `.BIN` machine-code programs using the locally referenced XRoar-compatible
  header shape: `$55`, file type `$02`, big-endian load address, big-endian
  payload length, big-endian exec address, `$AA`, then payload. The machine can
  inject those payload bytes into RAM, set the BASIC `EXEC` vector, and autorun
  at the declared exec address. `format-dragon-pak` normalises Dragon ROM/DGN
  cartridge images using XRoar-compatible header skipping for
  non-256-byte-aligned files, and parses PC-Dragon PAK snapshots as restored
  machine state. The runtime mounts program media in `program-1`, cartridge
  media in `cartridge-1`, and snapshot media in `snapshot-1`; plain ROM
  cartridges overlay `$C000-$FEFF`, and images larger than 16 KiB use Games
  Master Cartridge-style 16 KiB banking through the cartridge I/O range.
- **DragonDOS disks:** `format-dragon-disk` parses VDK disk images with the
  observed Dragon archive geometry of 40 tracks, one side, 18 sectors per
  track, and 256-byte sectors. `machine-dragon-32` exposes the DragonDOS P2
  controller range at `$FF40-$FF5F`, including command/status, track, sector,
  data, and drive-control registers, and can satisfy single-sector read and
  in-memory write transfers from mounted VDK media. Write-protected mounted
  media reports the WD write-protect status without mutating sectors. Real
  DragonDOS ROM directory reads now run through the WD2797-style DRQ/FIRQ and
  INTRQ/NMI paths. Index pulse timing, host writeback, and full WD2797 format
  behavior remain to be implemented.
- **Runtime:** `runtime-dragon` implements the shared `MachineCore` boundary,
  exposes separate Dragon 32 PAL and Dragon 64 PAL profiles, builds from
  profile-declared BASIC firmware, emits RGBA8888 frames and mono audio
  packets, exposes boot/video/PIA/SAM/tape/program queries, and mounts CAS media
  in slot `tape-1`, VDK disk media in `drive-1`, direct DragonDOS `.BIN`
  programs in `program-1`, cartridge media in slot `cartridge-1`, and PC-Dragon
  PAK snapshots in `snapshot-1`.
- **Native shell:** `emu198x-dragon` opens a native window, presents the Dragon
  framebuffer through the shared `wgpu` presenter with `raw`/`lcd`/`crt`
  filters, emits live host audio, accepts `--model dragon32|dragon64`,
  `--rom`, Dragon 64 `--rom64`, `--tape`, `--bin`, `--cart`, and
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
  multiple levels and sustained transitions. Direct script runs can mount VDK
  disk images with `--disk`. `--snapshot-smoke-root` scans
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
  --bin game.bin \
  --snapshot game.pak \
  --tape game.cas \
  --autoload \
  --video crt
```

Native Dragon 64 window:

```sh
cargo run --release -p emu198x-dragon -- \
  --model dragon64 \
  --rom ~/.emu198x/roms/dragon/dragon64-compat.rom \
  --rom64 ~/.emu198x/roms/dragon/dragon64.rom \
  --video crt
```

Direct DragonDOS `.BIN` program smoke:

```sh
cargo run --release -q -p emu198x-script-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --bin '/path/to/Dragon/Games/[BIN]/Cross Chase (2021-05-13)(Caruso, Fabrizio).zip' \
  --cycles 3000000 \
  --screenshot dragon-bin.png
```

Direct `.BIN` autorun boots the Dragon 32 ROM to the BASIC `OK` prompt first,
then injects the program payload and starts at the file's EXEC address. Starting
from reset-time CPU/PIA/SAM state is not equivalent: real programs depend on the
ROM-initialized stack, display base, and device state.

DragonDOS VDK disk smoke:

```sh
cargo run --release -q -p emu198x-script-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --cart dragon-dos.rom \
  --disk game.vdk \
  --cycles 2000000 \
  --type-command DIR
```

VDK mounting exercises the live DragonDOS P2 controller registers at
`$FF40-$FF5F`. The current implementation can run `DIR` through the real
DragonDOS ROM and return directory data from mounted media. Sector writes are
handled in memory for the mounted image, with an explicit in-memory
write-protect state for protected-media tests. Host writeback, formatting, and
index-pulse timing remain future work.

The real-ROM DragonDOS `DIR` path has an opt-in regression test. Point the
environment variables at a Dragon 32 ROM, DragonDOS ROM, and the Disk Doctor
VDK sample before running it:

```sh
EMU198X_DRAGON32_ROM=/path/to/dragon32.rom \
EMU198X_DRAGON_DOS_ROM=/path/to/dragon-dos.rom \
EMU198X_DRAGON_DOS_DIR_VDK=/path/to/disk-doctor.vdk \
cargo test -p emu198x-script-dragon dragon_dos_dir_command_lists_vdk_directory
```

The matching write-path smoke uses the same ROM variables plus a scratch VDK
path. The mounted image is mutated only in memory:

```sh
EMU198X_DRAGON32_ROM=/path/to/dragon32.rom \
EMU198X_DRAGON_DOS_ROM=/path/to/dragon-dos.rom \
EMU198X_DRAGON_DOS_SAVE_VDK=/path/to/scratch.vdk \
cargo test -p emu198x-script-dragon dragon_dos_save_command_returns_ok_on_vdk
```

Headless smoke over a DragonDOS `.BIN` tree:

```sh
cargo run --release -q -p emu198x-script-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --bin-smoke-root '/path/to/Dragon/Games/[BIN]' \
  --smoke-run-limit 8 \
  --cycles 3000000 \
  --smoke-report target/dragon-bin-smoke.json \
  --smoke-screenshot-dir target/dragon-bin-smoke-screens \
  --smoke-screenshot-format xroar-zoomed \
  --screenshot-phase completed-frame
```

The script crate also has a synthetic real-ROM regression for this path:

```sh
cargo test -p emu198x-script-dragon bin_smoke_matrix_runs_synthetic_program_when_dragon_rom_available
```

It builds a tiny DragonDOS `.BIN` fixture at test time, boots BASIC from the
configured Dragon 32 ROM, injects the program, starts it via the same EXEC path
as archived `.BIN` software, and asserts the smoke matrix reports visible
running output. Set `EMU198X_DRAGON32_ROM` when the ROM is not in the default
local archive locations.

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

Dragon 64 CAS smoke uses the same runtime-backed path with the Dragon 64
compatible-mode ROM and the separate 64-mode BASIC ROM:

```sh
cargo run --release -q -p emu198x-script-dragon -- \
  --model dragon64 \
  --rom ~/.emu198x/roms/dragon/dragon64-compat.rom \
  --rom64 ~/.emu198x/roms/dragon/dragon64.rom \
  --smoke-root '/path/to/Dragon/Applications/[CAS]' \
  --smoke-run-limit 8 \
  --smoke-report target/dragon64-smoke.json
```

Dragon 64 also works through runtime-backed `.BIN` and PAK smoke by using the
same `--model dragon64 --rom ... --rom64 ...` firmware arguments with
`--bin-smoke-root` or `--snapshot-smoke-root`. Dragon 32 keeps the older
trace-rich low-level harness for `.BIN` and PAK smoke so existing deterministic
trace signatures remain stable. Dragon 64 XRoar-reference and direct low-level
harness modes still reject `--model dragon64` rather than silently running the
wrong model.

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

1. Audio now follows the Dragon PIA DAC/mux/single-bit/cassette/cartridge-SND
   signal path and uses XRoar's measured level model. The local Backgammon audio
   gate now checks for active mono 48 kHz output with multiple levels and
   sustained transitions, but the emulator does not yet model analogue
   filtering or non-standard expansion audio hardware.
2. Native gamepad left-stick movement and Dragon script smoke runs can now feed
   continuous analogue axis values into the Dragon comparator path.
3. PAK-vs-XRoar screenshots remain advisory because the synthetic XRoar
   snapshot import path is not a hard state reference after resume. The regular
   PAK gate now compares deterministic internal trace signatures instead.
4. The beam framebuffer is in place, but the display model is still calibrated
   to the current 372x243 diagnostic visible area and XRoar zoomed comparison
   bridge. A fuller PAL timing/overscan model can come later.
5. Dragon 64 cold-boot and native `EXEC 48000` 64-mode BASIC entry are in
   place, including PIA1 PB2 ROM selection, `$FF04-$FF07` ACIA decode, and a
   post-transition BASIC command smoke. Full RS-232 behavior, cartridge
   expansion hardware beyond the documented `SND` input pin, and complete
   WD2797 disk timing/write behavior are not implemented.
6. DragonDOS VDK support is intentionally narrow at this stage: sector reads and
   real-ROM `DIR` work through the P2 controller register path, while
   write-sector, format, and index pulse timing still need source-backed
   implementation and real-ROM smoke coverage.

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
4. Run DragonDOS ROM plus VDK software smokes through the new disk-controller
   path and use the failures to prioritize exact WD2797 status/timing behavior.
5. Revisit PAL geometry and external video reference captures after the current
   practical usability loop is smoother.

## Completion Assessment

Dragon 32 is at a practical-use baseline: real BASIC ROM boot, keyboard, CAS
autoload, cartridges, PAK snapshots, beam-updated VDG output, cassette input,
PIA/SAM timing, mono audio, native windowing, gamepad input, and smoke tooling
are all in place. Dragon 64 now cold-boots through a separate runtime profile in
the hardware's Dragon 32-compatible reset mode and enters 64-mode BASIC with
`EXEC 48000`. The remaining work to call the family complete is accuracy,
peripheral coverage, and complete disk-controller behavior rather than initial
bring-up.

To complete Dragon 32, the main gaps are: full source-backed MC6809 timing and
interrupt edge cases, PAL video geometry calibrated against hardware captures,
analogue audio filtering and expansion-device mixing, deeper Dragon 64
post-transition software coverage, DragonDOS/WD2797 index and format behavior,
and a trusted fixture suite for joystick/audio/video behaviours that does not
depend on emulator-vs-emulator pixel matching.

## Test Coverage

| Component | Tests |
|-----------|-------|
| Machine | Dragon 32/64 ROM mapping, Dragon 64 ACIA decode, SAM P/TY RAM paging, direct DragonDOS `.BIN` program RAM/EXEC loading, DragonDOS P2 disk-sector reads, cartridge ROM/GMC overlay, device access reporting, keyboard, cassette input, analogue joystick comparator/fire wiring, SAM text base, frame-sync-delayed VDG display base, source-backed VDG byte-fetch timing, text framebuffer, graphics rendering, XRoar-pinned PIA DAC/tape/cartridge-SND/single-bit audio |
| PIA | 12: DDR, control, IRQ, input pins, mixed I/O, Cx1 edge selection, Cx2 input/output, Cx1-restored Cx2 strobe modes |
| SAM | 4: defaults, set/clear, video offset, all-RAM |
| VDG | 16: source horizontal geometry/crop split, text decode/rendering, inverse text, SG4, RG6, CG6, scanline rendering, byte-position rendering |
| Harness | 25: CLI, Dragon 32/64 ROM loading, VDK disk argument, direct `.BIN` argument, keyboard labels, text dumps, direct screenshots, CAS smoke options, Dragon 64 runtime `.BIN`/PAK smoke, Dragon 32 trace-backed PAK snapshot smoke, XRoar-compatible screenshots, XRoar reference comparison, smoke classification |
| Runtime | Dragon 32 and Dragon 64 profile metadata, firmware construction, framebuffer/audio emission, queries, boot status, CAS mounting/playback, VDK disk mounting, direct `.BIN` mounting, cartridge mounting, PAK snapshot mounting, joystick button-to-hardware mapping, real-ROM screenshot, Dragon 64 `EXEC 48000` plus post-transition BASIC smoke, Textstar CLOAD/RUN, machine-code CAS smoke, keyboard echo |
| Native | CLI, CAS tape argument, VDK disk argument, direct `.BIN` argument, cartridge argument, snapshot argument, CAS autoload command selection, real Textstar autoload smoke, host key mapping, gamepad-to-joystick mapping |
| CAS format | 7: block framing, header decode, real archive prefix, EOF, checksum visibility, truncation errors |
| BIN format | 4: DragonDOS sentinel, machine-code type, header fields, payload length validation |
| VDK disk format | 4: VDK signature/header decode, fixed Dragon sample geometry, one-based sector lookup, truncated payload rejection |
| PAK format | 4: aligned ROM images, XRoar-style cartridge header skip, empty image rejection, PC-Dragon snapshot decode |

## ROMs

Place the Dragon BASIC ROMs at:

| File | Size | Description |
|------|------|-------------|
| `~/.emu198x/roms/dragon/dragon32.rom` | 16KB | Dragon 32 BASIC ROM |
| `~/.emu198x/roms/dragon/dragon64-compat.rom` | 16KB | Dragon 64 compatible-mode BASIC ROM used at cold boot |
| `~/.emu198x/roms/dragon/dragon64.rom` | 16KB | Dragon 64 64-mode BASIC ROM selected by `EXEC 48000` |

Observed Dragon 64 compatible-mode CRC32 values in local references are
`0x60A4634C` and `0x84F68BF9`; the observed 64-mode BASIC CRC32 is
`0x17893A42`. These are used as diagnostics for swapped ROMs, not as a strict
allow-list for all possible valid dumps.

The native and script runners also accept a zip containing one suitable
ROM/bin candidate.
