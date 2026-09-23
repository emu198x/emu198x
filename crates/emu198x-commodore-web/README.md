# C64 and Amiga browser prototype

PAL C64 breadbin and PAL A500 + A501 (512 KiB chip RAM, 512 KiB trapdoor RAM and RTC), using the existing
runtimes through `emu198x-web`. This is a local engineering prototype, not a
published package or a broad browser compatibility claim.

## Run

From this directory, with the Rust WASM target and wasm-pack installed:

```sh
rustup target add wasm32-unknown-unknown
bash scripts/build.sh
python3 scripts/fetch-open-roms.py
python3 -m http.server 8765 --bind 127.0.0.1
```

Open <http://127.0.0.1:8765/example/>. Select a machine and click **Start / restart**.
The C64 defaults to Open ROMs. For the Amiga, select an A500-compatible Kickstart
file (tested with 1.3), or select **AROS m68k ROM pair** and supply both matching
512 KiB images. Local ROM/media file selection reads into browser memory;
there is no upload, persistence or server API.

If wasm-pack cannot install its CLI, provide a matching wasm-bindgen executable:
`WASM_BINDGEN=/path/to/wasm-bindgen bash scripts/build.sh`. Its version must match
Cargo.lock (0.2.127 at introduction). Builds omit wasm-opt so the measured build
can be reproduced without another optimiser installation.

No original Commodore ROM, Kickstart or drive firmware is embedded in the WASM
module or copied by the build. Generated packages, Open ROMs downloads and
corresponding source are ignored by Git. No deployment or npm publishing occurs.

## Controls and media

- Click the screen to type. Keys follow physical positions; on C64, Shift+2
  produces a quotation mark. Escape maps to RUN/STOP, Alt to Commodore.
- Select **Arrows + Space as joystick** for control port 2. On Amiga, click the
  screen or **Capture Amiga mouse** to capture relative mouse input. Only the
  Amiga pointer is visible while captured; **Escape** releases it. The first
  click only captures, so it cannot activate an unrelated guest control.
  Uncaptured hovering does not move the guest. Pause, restart and focus loss
  release capture and held buttons. A desktop browser with Pointer Lock support
  is required for mouse control; capture failure is reported beside the button.
- **Pause**, **One frame**, and **Start / restart** permit inspection and a fresh
  boot. Restart discards media and session changes. Losing window focus or
  hiding the tab pauses playback and releases held input; resume explicitly.
- C64 PRG: load after READY, then type RUN or SYS with the program's entry point.
  This uses the existing direct RAM import; it is not an emulated disk load.
- C64 D64/G64: supply a 1541 ROM at startup, insert the disk, then use the ROM's
  LOAD command. Open ROMs does not provide a 1541 ROM. No tape/cartridge UI yet.
- Amiga: insert a bootable ADF. The base configuration includes the A501 RAM expansion and its battery-backed
  clock, so Workbench can use the existing RTC implementation.
- Enable **Sound** for stereo AudioWorklet playback. The worklet has a bounded
  ring and rebuffers after starvation. It needs a secure context (localhost
  qualifies). No persistent save state, disk export or network modem is exposed.

The worker permits one frame request in flight. The shared pacer derives the
machine frequency from the runtime profile, caps catch-up, and handles invalid
clock deltas. The page transfers RGBA/audio buffers; no shared memory or
cross-origin isolation is required. `Measure 120 frames` advances the machine
while paused and measures emulation time, excluding presentation/transfer costs.
The running percentage includes wall time, so it is the better end-to-end check.

## Open ROMs

The optional downloader uses the matched **generic** KERNAL and BASIC plus
`chargen_openroms.rom` from MEGA65 Open ROMs revision
`ad178dbe4d48cd6a317737a8e0e7e662f7e33d32`. ROM and notice hashes are in
`scripts/open-roms.json`. The upstream prebuilt images identify themselves as
`DEV.210823.FC.1`; pinning the repository does not imply they were rebuilt then.

Upstream declares LGPL-3.0-or-later with per-file exceptions, including MIT
material. The downloader retains its notice, GPL/LGPL text, status document,
and the full corresponding source archive beside separate, replaceable ROM
files. Do not mix BASIC and KERNAL versions or use hybrid ROMs as substitutes.

Upstream's status document lists incomplete BASIC and other functionality.
Booting, PRINT and a specific program succeeding do not establish general game
or curriculum compatibility. Original firmware remains selectable. This is a
C64 option; it does not replace Amiga Kickstart.

Sources:

- [Project and licence](https://github.com/MEGA65/open-roms/tree/ad178dbe4d48cd6a317737a8e0e7e662f7e33d32)
- [Firmware pairing rules](https://github.com/MEGA65/open-roms/blob/ad178dbe4d48cd6a317737a8e0e7e662f7e33d32/bin/README.md)
- [Implementation status](https://github.com/MEGA65/open-roms/blob/ad178dbe4d48cd6a317737a8e0e7e662f7e33d32/STATUS.md)

## AROS on the Amiga

Select **AROS m68k ROM pair (experimental)**, then supply
`aros-amiga-m68k-rom.bin` and `aros-amiga-m68k-ext.bin` from the same build.
Both must be 512 KiB. The binding reuses the machine core's existing AROS
configuration: main ROM at `$F80000`, extension at `$E00000`, and the same
A500 + A501 base as the Kickstart option. Start/restart recreates both windows.
Allow about 30 seconds of emulated startup time. The tested local pair reaches
the AROS eyes/logo boot screen in Chromium (15 colours across 97 content rows);
this is firmware boot evidence, not a desktop or application compatibility test.

AROS is an open-source AmigaOS replacement under the AROS Public License;
see the [upstream project](https://www.aros.org/) and
[licence](https://www.aros.org/license.html). The prototype accepts local images;
it does not yet download or bundle them. A redistributable default package
would need a pinned build with its notices and corresponding source. AROS is
an alternative operating system implementation, so reaching its boot screen
does not establish compatibility with Workbench 1.3 or arbitrary Amiga software.

The bare A500 profile remains accurate in the core. The browser default selects
the existing A501 profile rather than adding an RTC to every bare A500.

## Verification

Generate native checkpoints from the workspace root, using the same firmware
that will be supplied to WASM:

```sh
cargo run --release -p emu198x-commodore-web --example parity -- \
  c64 PATH_TO_KERNAL PATH_TO_BASIC PATH_TO_CHARGEN > /tmp/c64-native.json
cargo run --release -p emu198x-commodore-web --example parity -- \
  amiga PATH_TO_KICKSTART > /tmp/amiga-native.json
```

Then from this crate:

```sh
node scripts/check-parity.mjs /tmp/c64-native.json PATH_TO_KERNAL PATH_TO_BASIC PATH_TO_CHARGEN
node scripts/check-parity.mjs /tmp/amiga-native.json PATH_TO_KICKSTART
```

Append `--browser` for real Chromium checks with the local server running.
Set `PLAYWRIGHT_PACKAGE` to an installed Playwright module's absolute path if
it is not locally resolvable. The checker runs twice from fresh machines and
compares native framebuffer hashes, dimensions, sample counts and audio energy
at frames 1/100/300/310/360, including input press/release. Audio energy permits
small floating-point differences; no claim of sample-bit identity is made.

Additional real-browser checks:

```sh
node scripts/browser-smoke.cjs PATH_TO_KICKSTART /tmp/commodore-screens
node scripts/check-mouse.cjs PATH_TO_KICKSTART
node scripts/check-media.cjs PATH_TO_KICKSTART ../../test-data/commodore/amiga/paula-audio/dist/channel-0-full.adf
```

The media check loads its own tiny C64 SID/border PRG through Open ROMs and the
repository-owned Paula ADF, then requires non-silent output. Build that ADF via
the existing corpus tools if absent. The UI check exercises boot, keyboard,
AudioWorklet setup, pause/step/restart, and malformed media handling. Screenshots
still need inspection; the UI check is not an OCR assertion of the BASIC result.

The mouse check opens a focused Chromium window (required for native pointer
lock on macOS) and exercises real browser capture, first-click suppression, Escape
release, held button/key cleanup, pause/restart and C64 isolation. It also checks
that resizing the canvas does not change relative mouse counts. The browser
does not try to align an absolute host cursor with a guest-controlled cursor:
Amiga software owns mouse acceleration, clipping and pointer warps.

For the A501 default and optional AROS path:

```sh
node scripts/check-amiga-firmware.cjs PATH_TO_KICKSTART PATH_TO_WB13_ADF PATH_TO_AROS_MAIN PATH_TO_AROS_EXT /tmp/amiga-firmware
```

This uses the actual file pickers, captures Workbench after startup and AROS
after 35 seconds, and checks the AROS screen is not a flat field. Inspect the
Workbench capture for `Battery backed up clock not found`; the unit regression
also verifies that the default decodes `$DC0000` as the RTC and fits A501 RAM.

## Initial measurements

On the development Mac in headless Chromium, release WASM without wasm-opt:

| Workload | Mean emulation/frame | p95 | Frame budget |
|---|---:|---:|---:|
| C64 Open ROMs READY/PRINT | 3.2 ms | 3.4 ms | 19.95 ms |
| C64 Open ROMs SID/border PRG | 3.3 ms | 3.5 ms | 19.95 ms |
| A500 Kickstart 1.3 disk prompt | 17.1 ms | 17.3 ms | 19.97 ms |
| A500 Paula channel-0-full ADF | 15.1 ms | 15.8 ms | 19.97 ms |

Both maintained approximately 100% real time at the boot screens. These are
120-frame samples on one host/browser, not minimum-spec claims. The initial
A500 measurements used the bare 512 KiB profile, before selecting A501 as the
browser default; they should not be treated as current expanded-profile benchmarks. C64 has useful
headroom; the A500's smaller margin makes busy graphics/disk workloads and
slower hosts the next performance gates. Safari, Firefox and mobile devices
have not been validated. The original combined firmware-free module was about 736 KiB; size varies with the enabled bindings.

The runtime portability change is RTC host-time access: on browser WASM,
`common-commodore-amiga` uses `web-time::SystemTime` instead of the unsupported
`std::time::SystemTime::now`. Native time access and deterministic emulated
clock advancement remain unchanged. Existing RTC tests and native/WASM boot
comparisons cover that boundary.
