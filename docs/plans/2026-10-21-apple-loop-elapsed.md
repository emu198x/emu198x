# Measure Apple real-time test observation duration

Lane: Spectrum launch-hardening, shared host regression. Base: remote main `ce2d755aff02770c1bc9561f7211b30fadbbd255`.

## Problem and evidence

Hosted Apple job 119205362805, run 39594996876, failed only `runsInRealTime`: 76 frames and 145,768 samples after a nominal one-second sleep. The test took 1.599 seconds. The 48K test and Apple loop are byte-identical from v0.42.0 to this base, so this is unrelated to the Read Data READY change. Owning issue: https://github.com/emu198x/emu198x/issues/1935.

Async sleep sets a minimum delay, not an exact observation interval. The independent emulation thread keeps running while the observer is delayed. Machine creation is also asynchronous and precedes the loop's clock start.

The first local observation attempt passed both fixed-count cases; it did not establish a red witness. Its output is retained as `initial-observation.log`. After bounded positive startup, the existing fixed-count oracle rejected 80 frames and 153,440 samples observed over 1.599619 seconds. That is about 50.012 frames and 95,923 stereo samples per second, within the existing tolerances. `red.log` records this intended failure against the real bridge/runtime.

## Small implementation sequence

1. Build the existing supported Apple XCFramework path with explicit Rust 1.99.0; retain commands and identities with a 1,200 second host timeout.
2. Exercise 1.0 and 1.5 second observations with bounded startup. Retain the fixed-count oracle for fail-first reproduction.
3. Compare frame/sample deltas over actual monotonic elapsed time, preserving the 45–55 frame and 86,000–106,000 stereo-sample rate bounds. No production behavior, API, schema or dependency changes.
4. Run the full Swift suite, repeat both timing cases, deliberately double the expected frame rate and prove rejection, then restore and rerun green. Bound each process to 180 seconds.
5. Inspect release PR #1928 without merging or publishing it; broader feature work remains separate.

## Verification

The supported build packaged macOS, tvOS and tvOS simulator libraries. The macOS C link check executed; tvOS checks linked but cannot execute on this Mac. The direct-Fleet baseline was generated. Existing fleet dead-code and duplicate-library warnings are retained; the build passed.

Swift 6.4 / Xcode 27.1: all 22 tests in three suites passed, including both locally available real-ROM checks. The 1.0/1.5 second timing cases passed in the full run, a repeated run and the restored run. Deliberately changing expected frame rate from 45–55 to 90–110 made both cases fail at the intended rate assertion; `wrong-rate.log` retains the result. No skips are credited as passes. Local firmware is identified by hash only; no firmware bytes are retained here.

Exact commands, bounds, outputs, source hash, tool versions, Apple build metadata and synthetic Rust baseline are in the sibling `2026-10-21-apple-loop-elapsed/` directory. This is source-build host-test evidence, not a released-binary or FDC qualification claim.
