# Native Apple hosts

Work in progress: an experiment to run Emu198x as a native macOS and Apple
TV app (#1896–#1898). The plan and its current state are in
`docs/plans/2026-10-20-apple-native-host-spike.md`.

The emulation stays in Rust. `crates/emu198x-apple` exposes the runtime
catalogue through a small C interface over `Fleet`, the same API the
browser player uses, and the Swift code here presents its output and
routes input to it.

## Build

```sh
rustup target add aarch64-apple-darwin aarch64-apple-tvos aarch64-apple-tvos-sim
scripts/build-apple-xcframework.sh
```

This writes `apple/build/Emu198xCore.xcframework`, with the macOS, Apple TV
and Apple TV simulator libraries, and `apple/build/baseline.json`. The
script links a small C program against each library and runs the macOS
one before it packages anything. Run it again after any Rust change.

## Test

```sh
cd apple/Emu198xKit
swift test
```

`Emu198xKit` is a Swift package: `Machine` wraps the C interface, and
`EmulatorLoop` runs one machine in real time on its own thread. The tests
compare Swift's output with `baseline.json`, which the Rust example
`crates/emu198x-apple/examples/baseline.rs` produces by calling `Fleet`
directly. The two must match bit for bit.

The test program is a synthetic 16 KiB Spectrum ROM that makes sound and
moves the border (`crates/emu198x-apple/tests/common/beeper.rs`), so no
firmware is needed. A test that boots the real 48K ROM runs when one is
at `EMU198X_SPECTRUM_48K_ROM` or `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`,
and is reported as skipped otherwise.

CI runs all of this on a macOS runner in the `Apple hosts` job of
`.github/workflows/ci.yml`, and also builds the Swift package for tvOS and
the Mac app.
