# Apple native host spike: macOS first, then Apple TV

**Issues:** #1896 (bridge), #1897 (macOS), #1898 (Apple TV).
**Agreed 2026-10-20 (Steve):** a hand-written C ABI over the existing
`Fleet` API in `emu198x-fleet-web`, with no new Rust dependencies and no
changes to existing crates. Lifting `Fleet`'s `Host` trait into a shared
crate waits until the spike succeeds.

## Already established

- `emu198x-fleet-web` builds in release mode, unchanged, on the pinned
  Rust 1.99.0 for `aarch64-apple-tvos` and `aarch64-apple-tvos-sim`. It
  builds with `sinclair-zx-spectrum` alone and with `all-families`.
- The local toolchain has Xcode 27.1, the tvOS 27.0 device and simulator
  SDKs, and Swift 6.4.
- `[profile.release]` doesn't set `panic = "abort"`, so `catch_unwind`
  at the C boundary works.
- The C interface needs `unsafe`, which the workspace forbids. Steve
  approved a contained exception for this crate alone:
  `knowledge/decisions/apple-bridge-unsafe-exception.md`.
- The static library's objects embed bitcode from Rust's LLVM, which
  Xcode's `nm` and `llvm-nm` cannot parse, so checking for symbols with
  `nm` proves nothing either way. Linking a C program against the library
  with Apple's `clang` and running it does.
- `emu198x-shell` depends on `cpal` and `gilrs` on every non-WASM target,
  so the library carries audio-output and gamepad code the bridge never
  calls. For tvOS it links AVFAudio, AudioToolbox, CoreAudio and
  Foundation, all available there; IOKit appears only in the macOS build.
  Trimming that needs a feature in `emu198x-shell`, which is out of scope
  for the spike.

Compiling proves nothing about speed, sound or input. Each step below
names its own evidence.

## Boundaries

- The emulation core is unchanged. The Swift code presents, schedules and
  routes input. It never reimplements machine behaviour.
- The Spectrum is the first family tested, not the only one built. The
  library takes the family and variant as strings, like `Fleet::new`.
- Local builds only. The 48K ROM comes from
  `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom` after a SHA-1 check
  (`5ea7c2b8…`). Under `test-rom-policy.md` § Firmware in a published
  browser build, only two artifacts may carry the ROM, so distributing an
  Apple build with the ROM needs that record amended first. Test software
  stays in a git-ignored folder and is never committed.
- Commands follow the `native-menu-shell.md` model: one command enum,
  applied by the emulation owner between frames. This plan adapts that
  model to Swift and doesn't bypass it.
- No unified launcher, no IDE, and no store packaging.

## Steps

Each step is one PR, conventionally titled, and green before the next
starts.

### 1. `crates/emu198x-apple`: C ABI over `Fleet` (done 2026-10-20)

Files:
- `crates/emu198x-apple/Cargo.toml`: `publish = false`,
  `crate-type = ["staticlib", "rlib"]`. Its only dependency is
  `emu198x-fleet-web`, with family features passed through. The default is
  `sinclair-zx-spectrum`.
- `crates/emu198x-apple/src/lib.rs`: an opaque `Emu198xMachine` handle
  around `Fleet`. Functions: `new`/`free`, `firmware`, `boot`, `advance`,
  `step`, `load`, `transport`, `key`, `button`, `configure_audio`,
  `audio` (copied into a caller buffer; samples beyond its capacity stay
  queued in the handle), `frame_size`, `pixels` (copied into a caller
  buffer), `frame_ms`, `save_state`/`buffer_free`, `restore_state`,
  `keymap`, `last_error`.
- Every entry point returns a status code and runs inside `catch_unwind`.
  A panic sets `last_error`, poisons the handle, and makes later calls
  fail. Null pointers and bad UTF-8 are errors, not undefined behaviour.
- `crates/emu198x-apple/include/emu198x.h`: a hand-written header and
  module map.
- The crate is added to the workspace `members`.

Verification:
- Integration tests call the C functions from Rust against a fixture the
  repository owns. They check that frame hashes and audio sample counts
  match direct `Fleet` calls over the same inputs, that a snapshot
  round-trips, that errors are reported, and that a poisoned handle
  refuses work.
- A test checks that every `#[unsafe(no_mangle)]` symbol appears in
  `emu198x.h` and every header declaration has a symbol. Add a
  deliberately missing declaration to confirm the test fails.
- `cargo clippy -p emu198x-apple --all-targets -- -D warnings` passes.
  The crate builds for all three Apple targets.

### 2. `scripts/build-apple-xcframework.sh` (done 2026-10-20)

This script builds the static library in release mode for
`aarch64-apple-darwin`, `aarch64-apple-tvos` and `aarch64-apple-tvos-sim`,
with deployment targets of macOS 14 and tvOS 17. It then runs
`xcodebuild -create-xcframework` with the header and module map into
`apple/build/Emu198xCore.xcframework`, which git ignores.

Verification: the script links `apple/link-check.c` against each slice
with Apple's `clang` and the system libraries rustc reports for that
target, and runs the macOS build. It stops at the first failure. `nm`
can't be used for this; see *Already established*. It records the
toolchain, SDK versions, deployment targets and link flags in
`apple/build/build.json`.

### 3. `apple/Emu198xKit`: Swift package (done 2026-10-20)

- A Swift package with a binary target for the XCFramework. `Machine`
  wraps the C interface. It is not `Sendable` and has exactly one owner.
- `EmulatorLoop`: a dedicated thread creates and owns the `Machine`. It
  paces `advance(elapsed)` from its own monotonic clock, capping catch-up
  at 250 ms after a stall. It applies queued `HostCommand`s between
  frames, publishes the latest frame to a locked slot, and writes audio
  to a bounded ring that counts dropped samples and underruns. Display
  refresh never sets emulation speed. Commands the machine refuses, such
  as unreadable media, are reported and don't stop the loop.
- Strict concurrency under Swift 6, with no `@unchecked Sendable` at all.
  The shared state sits behind `OSAllocatedUnfairLock`. The one
  `withLockUnchecked` is the audio read, which copies into the caller's
  buffer without allocating on the audio thread.
- The output device (`AVAudioSourceNode` reading the ring) belongs to the
  app, in step 4.

Verification: `swift test` on macOS, 12 tests:
- Swift's output matches `apple/build/baseline.json`, which
  `examples/baseline.rs` writes by calling `Fleet` directly, bit for bit
  across 50 frames of picture hash, sample count and audio energy.
  Changing one byte of the Swift copy of the test program makes it fail.
- The test program is `tests/common/beeper.rs`, a synthetic 16 KiB
  Spectrum ROM that sounds the beeper and moves the border. The synthetic
  Game Boy cartridge is silent and static, so it couldn't show an audio
  or picture fault. The Rust parity tests use the beeper program as well.
- A restored state replays identically, 100 machines tear down cleanly,
  errors arrive as `EmulatorError`, and the real 48K ROM boots where one
  is staged.
- The loop runs 50 frames (±5) and produces one second of audio (±10%)
  in a second of wall-clock time. Pausing stops it, saves and refused
  commands reach the host, and a machine that can't start ends the loop
  with its error.
- The package builds for the tvOS device and simulator.

Found along the way: restoring a state doesn't discard audio already
queued from before the restore. That matches `Fleet`, and the loop
drains audio every frame, but the app should clear its ring on restore.

### 4. macOS app (`apple/Emu198x/`, macOS target) (in progress)

"Emu198x Spectrum", a per-system app as `no-unified-launcher.md` requires,
built from `apple/Emu198x/Emu198x.xcodeproj`. Not sandboxed, and signed
ad hoc for local runs only.

- SwiftUI app whose screen is an `MTKView` drawn by `SharpRenderer`
  (shared with tvOS). The display link shows the newest frame and never
  drives emulation.
- Rendering (Steve, 2026-10-20): the machine's true pixel aspect, which
  the runtime reports through `Fleet::pixel_aspect_ratio` (1.0549 for a
  PAL Spectrum), with every pixel the same width. Sharp bilinear: each
  source pixel is a solid block of screen pixels, and only the fractional
  edge between two blocks is blended, in one shader pass. Nearest-
  neighbour at a fractional scale instead mixes stroke widths (3 and 4
  screen pixels at 3.17×), which is why characters looked slightly scaled.
  Measured from a backing-resolution capture at 2.98×: the picture's
  aspect is 1.054, and every single-pixel stroke on a text row is two
  solid pixels with a blended pixel either side.
- A macOS scaled display mode resamples the whole screen after every app
  has drawn. The BenQ EW3270U here runs as "looks like 3008 × 1692", drawn
  at 6016 × 3384 and shrunk to its 3840 × 2160 panel, so no app can be
  pixel-exact on it. Judge sharpness on an exact 2× setting or at native
  resolution.
- Menus: File (Open Tape or Snapshot, Save State, Load State), Machine
  (Pause, Restart, Play/Stop Tape) and View (Show Statistics, Full
  Screen), each with a shortcut. Every one calls the same
  `SpectrumSession` method, which sends one `HostCommand`.
- Tapes wait for the Spectrum's own `LOAD ""` and Play Tape, as in the
  other hosts (`loading-a-tape-is-not-pressing-play.md`). `Fleet` loads
  `.sna` and `.z80` snapshots directly.
- Keyboard: Mac key codes map to browser key codes by physical position,
  then through the runtime's `keymap()`. Machine keys are counted, so
  chords sharing Caps Shift release correctly. Focus loss releases
  everything.
- Controller: an extended gamepad's d-pad, left stick and A, sent to
  input port 0, the primary joystick (Kempston on the Spectrum), per
  `joystick-port-numbering.md`. `Fleet::button` always sends port 1, which
  suits a console's controller one but is the Sinclair Interface 2 on a
  Spectrum, and Dizzy reads only the keyboard and Kempston. So `Fleet`
  gained `button_on_port` (Steve approved, 2026-10-20), `button` now
  forwards to it with port 1, and the C button function takes the port. A
  test ROM that copies Kempston port $1F to the screen proves port 0
  reaches it and port 1 doesn't.
- Audio: an `AVAudioSourceNode` reads the loop's ring. The ring now waits
  for about 60 ms of sound before playing, and refills after running dry.
  Without that, nearly every render call ran short (11,861 underruns in a
  minute).
- Save states are the runtime's own state bytes, not the browser player's
  `.emu198x` file, so the two can't exchange saves yet.

Verified so far, with the packaged Release `.app` on this Mac (Apple
silicon, macOS 27, BenQ EW3270U display):
- It boots to the © 1982 screen.
- `PRINT 123` typed and run prints `123` and `0 OK, 0:1`.
- It runs at 50.0–50.4 frames per second, with 0 underruns and 0 dropped
  samples after the prebuffer change.
- A display probe showed the view refreshing at 60 Hz and picking up 50
  new frames a second.

A key pressed and released within one frame is missed, because the ROM
scans the keyboard once per frame. That is how the hardware behaves, and
normal typing is fine, but keyboard replay on a fast host should hold
keys for at least a frame.

- Steve, 2026-10-20: loaded Dizzy (1987, 48K TZX from Spectrum
  Computing, where it is listed as available) at real speed with
  `LOAD ""` and Play Tape, then played it with the keyboard: "it worked
  exactly as it should". That covers the TZX path, the tape deck, the ROM
  loader, beeper sound, picture and keyboard input in the packaged app.

Still to verify: play with a physical controller (Kempston, in the
build after the port change), save, quit, reopen
and restore, a 10-minute underrun count, and the alert for a failed load.

### 5. Apple TV target (same project, tvOS target)

- A remote-friendly list of local software from the git-ignored content
  folder. Select starts play; Menu opens a pause overlay with Resume,
  Restart, Save and Load.
- For now saves go to the Caches folder, and the overlay labels them as
  temporary. Durable saves (#1898) are a later decision.

Verification: the tvOS simulator boots, runs a program and plays sound.
A real Apple TV is next, with its model, tvOS version and build
configuration recorded, measured the same way as step 4. Success in the
simulator says nothing about how the hardware performs.

### 6. Report

Post results, limitations and a go/no-go on #1896–#1898, with the numbers
measured in steps 4 and 5.

## Firmware for families that need purchased ROMs (later)

Steve, 2026-10-20: for the C64, Amiga and other families whose ROMs must
be bought, Cloanto or the current legal owners could be approached about
offering the original firmware. To begin with, ship free replacement ROMs.

The browser player already accepts the C64 Open ROMs and AROS as
optional local inputs, with their compatibility distinguished from the
original firmware. Before an Apple build carries either one:

- check each replacement's licence allows redistribution inside the app;
- amend `test-rom-policy.md`, which today allows only the Spectrum 48K
  ROM in published builds;
- label compatibility in the app as the browser player does, so a game
  that fails on a replacement ROM isn't mistaken for an emulator fault.

None of this is part of the Spectrum spike, and no outreach is
authorised yet.

## Settled along the way

- The Xcode project (step 4) is a committed project file using Xcode's
  synchronized folders, not generated by XcodeGen (Steve, 2026-10-20).

## Open questions, settled when the step needs them

- Apple TV hardware and signing for a device build (step 5): which model,
  and which Apple Developer team.
