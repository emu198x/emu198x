# Shared browser player

Player for Code198x system pages and Emu198x system pages. It carries no
firmware, except that a site build may embed the Spectrum 48K ROM (below).
Both sites mount the same inline custom element from their own copy of the
static distribution. The player inherits site typography and theme colours;
there is no iframe or separate player-page interface. Cartridge selection
starts playback; computer firmware stays in a disclosure until needed. The worker
imports only the selected family's WASM module when Start is pressed.

The generated catalogue covers 30 runtime families and 91 model IDs. Models,
firmware requirements and media slots come from `FamilyRuntime`; browser file
extensions and site aliases live in `catalogue.mjs` and `families.json`.
The five established defaults retain their existing loaders: Spectrum 48K PAL,
C64 PAL, A500+A501 (with RTC and optional AROS pair), NES NTSC and DMG Game Boy.
Other models use `emu198x-fleet-web`, compiled separately for each family.
Selecting a model restarts setup and discards the running session.

Code198x also mounts the matching Spectrum model on the Pentagon, Scorpion,
Timex TC2048 and TS2068 pages. Support here is an emulator surface, independent
of curriculum coverage.

## Build

```sh
rustup target add wasm32-unknown-unknown
node scripts/build-browser-player.mjs /path/to/site/public/emulators
```

Run from the repository root with `wasm-pack` installed. Alternatively set
`WASM_BINDGEN` to a wasm-bindgen CLI matching Cargo.lock. No firmware is
fetched, and only generated JS/WASM and player files are copied. `build.json`
records the source revision, whether it was modified, and whether the Spectrum
48K firmware is bundled.

### Bundled Spectrum 48K ROM

Set `EMU198X_SPECTRUM_48K_ROM` to the path of the Sinclair 48K ROM and the
builder compiles `emu198x-spectrum-web`, the module that runs the 48K model,
with its `bundled-rom` feature. Sites supply the image from an encrypted CI
secret; it never enters this repository. Before building, the builder checks
the image is the unmodified ROM (SHA1 `5ea7c2b824672e914525d1d5c419d71b84a426a2`,
16384 bytes) and stops on anything else, including patched 48K images. Unset
or empty, the build is unchanged and the 48K asks for firmware as before.

With the ROM bundled, the catalogue marks the 48K model's firmware as bundled,
the 48K starts (and lesson `src` programs run) without a firmware prompt, and
Controls & session information shows Amstrad's requested acknowledgement.
The 16K, Spectrum+, 128K, +2 and +3 models, and every other family, still
use the visitor's own firmware. The ROM is only ever inside the wasm; the
build fails if any distribution file is a `.rom` or has the ROM's bytes.
`node web-player/check-bundled-firmware.mjs [dist]` checks the guard without
the ROM. See `knowledge/decisions/test-rom-policy.md`
§ Firmware in a published browser build.
The GPL licence and public source/build link travel with the distribution.

The builder runs the executable worker checks, native/WASM Game Boy parity
and fleet parity with repository-owned synthetic fixtures
before replacing the previous output. It needs Node 24 and native Rust as well
as the WASM toolchain. Site workflows build this before Astro, and site prebuild
checks reject missing assets. Land the emulator changes before the site changes;
the clean CI checkout then supplies the corresponding public source revision.

## Validation

```sh
cargo test -p emu198x-game-boy-web -p runtime-nintendo-game-boy
cargo clippy -p emu198x-game-boy-web -p emu198x-nes-web --all-targets --no-deps -- -D warnings
node web-player/check-worker.mjs /path/to/built/emulators
```

The worker check adapts Web Worker transport and local file fetch to Node;
it executes the actual worker and WASM, not a fake emulator. The repository's
synthetic Game Boy and NES cartridges exercise boot, frames, stereo sample
delivery, input dispatch and malformed requests. Native/WASM Game Boy frame
hashes, sample counts and audio energy are compared at 20/40/60 frames with A
released/held/released and repeated from a fresh machine. The native test also
checks the frame duration and wall-clock pacing (M-cycles, not master ticks).

Optional `SPECTRUM_ROM`, `AMIGA_ROM` and `C64_ROM_DIR` inputs extend the worker
check to those three families (a build with the bundled 48K ROM also boots
the Spectrum with no firmware sent); the C64 directory uses the prototype's Open ROMs
filenames. Existing Commodore parity, real-browser mouse and firmware checks
remain documented in `crates/emu198x-commodore-web/README.md`.
This worker check does not establish browser-specific performance or audible
output. In particular, the synthetic logo cartridges do not validate a game's
soundtrack. Browser/device performance and broader software coverage are still to be
measured; since 2026-09-23 they no longer block publication (see the umbrella
`decisions/browser-player-rollout.md`). Publishing the catalogue is not a claim
that every title or peripheral works.

## Behaviour and limits

- Firmware/media come from file inputs and stay local. Open ROMs and AROS are
  accepted as user-selected inputs, with distinct compatibility expectations.
- Computer input uses a physical keyboard. Console families also have touch buttons.
  Tab leaves the screen; Amiga mouse capture releases with Escape.
- Blur, hidden tabs, element removal, pause and restart release input and
  clear queued audio. Restart creates a new worker and discards the session.
- Loading a cartridge requires restart. Computer media can be inserted during
  execution. Select a boot disk before starting the Amiga. Initial C64 PRG
  imports wait through boot; RUN/SYS remains the user's choice.
- Save & resume stores one explicit device save per system/model in IndexedDB,
  or exports a versioned `.emu198x` file. Saves contain firmware, software and
  runtime state. Source files are never overwritten. Device saves are specific
  to the site origin and can be lost when browser data is cleared.
- Remember firmware is opt-in per system/model, with a Forget control. Imported
  saved sessions never silently opt into remembering firmware.
- Fullscreen includes the screen and controls; Escape uses the browser's normal
  exit behaviour. Gamepads and mobile performance remain follow-on work.
- Existing Spectrum curriculum API and muted NES trial defaults are preserved;
  the shared player opts into NES audio explicitly.

The shared audio worklet and pointer-lock implementation are sourced from the
Commodore prototype by the builder so fixes and its regression checks apply to
both surfaces. Emulator chip code remains in the existing runtime/machine crates.

## Inline host checks

`DOM_PACKAGE=/path/to/happy-dom/lib/index.js node web-player/check-inline.mjs
/path/to/built/emulators` checks the DOM host without launching a browser. It
covers deferred startup, cartridge autostart/replacement, isolated instances,
firmware disclosure, pointer-lock retargeting through the shadow root, and
worker/listener/animation cleanup. This uses a simulated worker transport;
`check-worker.mjs` independently executes the real WASM. The element inherits
site theme tokens and keeps its controls inside a shadow root so page styles
cannot accidentally restyle or intercept emulator input.

## Fleet validation

```sh
cargo test -p emu198x-fleet-web --features all-families --lib -p runtime-atari-800xl
cargo clippy -p emu198x-fleet-web --features all-families --all-targets --no-deps -- -D warnings
node web-player/check-fleet.mjs /path/to/built/emulators
PLAYER_TEST_ROM_ROOT=/path/to/local/roms node web-player/check-fleet.mjs /path/to/built/emulators
```

The default fixture set exercises 68 models without proprietary firmware.
Local firmware extends the same gate to all 91 models: 18 frames per model,
three released/pressed/released checkpoints, exact RGBA hashes and stereo sample
counts, finite audio and matching audio energy. This proves the browser boundary
agrees with native execution; it does not prove authentic manufacturer boot,
input response in every application, or real-browser performance. The inline
DOM gate mounts every model and checks variant changes, slot-specific file
filters, BIOS-console setup, Sega Pause, and disposal, alongside the original
five-machine regressions. These DOM checks are separate from the explicit
browser checks below.

Atari 800XL profile timing now describes the colour-clock units the runtime
already returns, with regional rates matching the machine. A regression test
checks that 21 ms advances one frame instead of zero. No CPU/chip timing changed.

Known model limits remain visible: ZX80/ZX81 display generation is simplified;
SGB/SGB2 run the handheld core without SNES host features; Game Boy Color is not
included. The default A500 player accepts an AROS pair, while other Amiga models
currently take their declared Kickstart/bootstrap firmware. C64 extra models
expose PRG, cartridge, tape, 1541 disk and optional 1581 disk slots; selecting
other drive hardware is not exposed. Tape Play/Stop is available where the
runtime provides explicit transport commands. Memotech currently has firmware
and keyboard operation without media slots.

## Save and browser checks

`check-save-file.mjs` checks the binary save envelope, SHA-256 corruption checks,
model isolation and truncation errors. All 91 local-firmware configurations also
pass save/restore frame checks in `check-fleet.mjs`. Spectrum snapshots omit
transient ULA border latches, so that comparison allows their documented first
frame to reseed; some host audio resamplers restart within one stereo sample.
Those are existing runtime snapshot behaviours, not changes to chip timing.

Save import boots and validates a replacement worker before disposing of the
current one. Invalid containers or runtime snapshots leave the current session
available. Restores clear queued input/audio and reset wall-clock pacing.

Disk export is available for C64 D64/D81, Amiga ADF and Dragon VDK working copies.
G64 is read-only and has no export button. Other systems use their runtime's save
state for persistence; the browser does not add unsupported disk-write commands.
Downloads create new files. Remembered firmware and device saves never leave
IndexedDB unless the visitor explicitly exports a save file.

Run explicit browser QA with `PLAYWRIGHT_MODULE` pointing at a consuming site's
Playwright `index.mjs`, `PLAYER_TEST_URL` pointing at the player distribution,
and `node web-player/check-browser.mjs`. `PLAYER_BROWSERS` selects chrome,
firefox and/or webkit. Optional `PLAYER_SMS_ROM`, `PLAYER_AMIGA_ROM` and
`PLAYER_AMIGA_DISK` test local software; these files are never published.
Chrome and Playwright WebKit passed storage, download/import, fullscreen,
firmware-forget and local Alex Kidd / Workbench 1.3 smoke checks. Firefox could
not launch its graphics process in this environment; Safari's driver requires
Allow Remote Automation, which was not enabled. WebKit testing is not a claim
that the shipping Safari application was tested. These are functional smoke
checks, not broad title compatibility or performance certification.

## Lesson launchers and preferences

An embed can set `src` to a same-origin media file and `variant` to the exact
lesson model. The **Run this lesson** button fetches it only on request, asks
for any required local firmware and starts the machine. C64 PRGs at BASIC's
standard load address also type `RUN`; arbitrary machine-code entry points
are not guessed. Explicit lesson models override remembered system-page
choices. A failed download leaves the existing machine intact.

Code198x's `LessonPlayer` discovers actual staged outputs for the unit, including
the final cumulative step where a lesson has a `steps` directory. Units without
an output have no launch button. The lesson artefact build stages TAPs as well
as PRGs, snapshots, NES cartridges and ADFs. The older interactive Spectrum
lesson component uses its own custom-element name so both players can coexist.

Each site remembers model, sound, volume, display filtering and console key
bindings in local storage. These preferences contain no firmware or media;
firmware remains a separate explicit opt-in. Blocked storage leaves the current
controls usable. Computer keyboards retain their machine mappings. Console
bindings affect physical keys; the touch controller retains its labelled actions.

The visible controls legend follows the chosen bindings. Report a problem
copies only system, model, build revision, browser and feature availability;
it never copies filenames, paths, page URLs, error messages or saved state.
Visitors review the text and submit their own issue. Clipboard denial reveals
selectable text instead.

## Bundled demos

Eight display demos are bundled for the Game Boy, NES, Atari 2600, Atari 5200,
Atari 7800, Master System, Game Gear and SG-1000. These are small project-owned
programs, not commercial games or operating-system firmware. Title-card and
solid-colour demos are labelled so their expected behaviour is clear.

`demos.json` is the explicit distribution allowlist. `stage-demos.mjs` copies
only those binaries, their build scripts and assembly sources, provenance and
the GPL-2.0-or-later licence. The sources are under
`test-data/synthetic-cartridges` and `test-data/sega/synthetic-cart`.
No files from the firmware fixture directory or a user's ROM library are staged.
Other families currently offer local media loading; they do not yet have a
bundled demo. The default models are the browser-validated demo targets.

Additional explicit browser checks:

```sh
PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs node web-player/check-extras.mjs
PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs \
  PLAYER_TEST_ROM_ROOT=/path/to/local/roms node web-player/check-lessons.mjs
```

The first exercises demos, persisted presentation choices, actual remapped
worker input, diagnostic privacy, explicit model precedence and missing-file
recovery in Chrome and WebKit. The second exercises Code198x's four introductory
lesson launchers; firmware-dependent checks use local inputs only.

### Loading output from an in-page assembler

After `customElements.whenDefined('emu198x-player')`, call
`await player.loadMedia({name: 'program.nes', bytes: Uint8Array})`. The element
waits for its UI, validates the format and boots the selected model with a copy
of those bytes. The promise returns whether the machine started (false when
firmware setup remains) and rejects load failures. `external-source` suppresses
the demo and initial file-picker prompt for a player controlled by an editor;
normal runtime controls and file selection remain available after startup.

Hosts that hide the player can call `player.pause()` before closing their panel.
This releases held controls and silences audio while preserving the machine;
the player’s Resume control continues it.
