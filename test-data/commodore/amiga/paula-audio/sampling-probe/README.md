# Preserve short Paula signals and restore continuity

The runtime previously point-sampled Paula at 48 kHz. A one-system-tick
volume pulse could disappear or be assigned a whole host interval. All 36
cases in the pulse regression failed: six pulse phases on PAL/NTSC OCS/ECS/AGA.
Expected values come from direct overlap with the two host intervals, then
the existing board filter, independently of the accumulator implementation.

The saved filter-history regression also failed: the first restored sample
was `(0.01076639, -0.005383195)` instead of
`(0.6979412, -0.46349975)`. The two `*-before.log.gz` files retain these failures.

The corrected existing accumulator integrates every completed system tick
and splits fractional host-boundary weights. Snapshot v61 saves the partial
areas and filter history, rejecting v60. Coefficients remain model-derived;
malformed signal state fails before mutating the runtime. No chip timing,
dependencies, register delivery, or physical PWM phase is changed.

Native regressions:

```sh
cargo test --locked --release -p runtime-commodore-amiga --lib audio_sampling
cargo test --locked --release -p runtime-commodore-amiga --lib restore_retains_live_audio_filter_response
cargo test --locked --release -p runtime-commodore-amiga --lib malformed_audio_signal
```

The broader replay test covers 48 live checkpoints across A500, A1000, ECS,
AGA and both regions. It requires nonzero partial areas and audible output,
compares the host samples and final snapshot bytes, and checks reset. Precise
AGA instruction-boundary stepping must match ordinary tick sampling. Eight
malformed histories/areas must be rejected atomically.

Pinned WinUAE's `anti_prehandler`/`samplexx_anti_handler` supplies the inspected
time-integration precedent. This is interval averaging, not its separate
band-limited sinc path or a physical PWM oracle. The primary evidence is
`reference/by-system/commodore-amiga/2026-paula-host-sampling-observations.md`
in the umbrella repository. Full bandwidth rejection and the volume-counter
phase remain explicit open issues.

Final validation: 303 runtime tests pass (30 existing ignored diagnostics),
including 65 snapshot tests. Strict release Clippy, the Amiga release build,
formatting and all three explicit ROM-backed waveform cases pass. The first
full runtime run found two stale version-60 test assertions; both were
updated to the approved version 61 and the complete rerun passed. Compressed
logs and hashes in `validation.json` preserve both runs.
