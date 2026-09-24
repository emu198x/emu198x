# Sinclair 128K early-timing survey profile

The Butler 128K v1.0 suite reports five failures on the emulated Toastrack.
Those exact readings are also published for physical Issue 6K and Issue 6U
Toastracks tested by Brendon Alford, both with 7K010E5 ULAs:

| Contended test | R | Loops | Saved IRQ return address (`sp`) |
|---|---:|---:|---:|
| 4 | 6 | 174 | 23305 |
| 17 | 22 | 203 | 23335 |
| 18 | 22 | 203 | 23335 |
| 26 | 75 | 147 | 23345 |
| 33 | 119 | 196 | 23315 |

Evidence: [published hardware reports](https://github.com/redcode/ZXSpectrum/wiki/ZX-Spectrum-Timing-Tests-128K#results-on-real-hardware),
retrieved 2026-09-24 at wiki revision
`4b9e47af926cbb46972db5cdc99badaa2fb3d771` (page last changed at
`82cbd53757f7810adec2fb1555dfc9e34cf80d03`). Primary family reference:
`198x/reference/by-system/sinclair-zx-spectrum/zx-spectrum-128k-timing-profiles.md`.
The reports are attributed physical-machine observations; this project has
not made a new hardware capture. The supplied early-timing screenshot also
matches, but its capture device is unspecified.

The suite uses a fixed expected-value table whose selected readings match
the late-timing screenshot. Its verdicts remain preserved: 63 passes and five
failures. The harness separately checks `early-toastrack`, requiring the exact
triplets above and passing verdicts for all other cases. Missing cases still
fail completeness checks. `hardware_profile_mismatches` in the JSON lists
profile deviations without rewriting any suite verdict.

This replaces the failure-count ceiling, which could accept five entirely
different failures. A change that makes these five cases pass must also receive
review: silently changing to a different machine timing profile is not an
accuracy improvement. No CPU, ULA, fixture or timing constant changes here.

Scope is the current 128K Toastrack model. Published grey +2 results differ;
this check does not certify the shared +2 wrapper. The reason individual
physical machines exhibit early or late timings is outside this validation.

Run with local ROMs and the pinned suite corpus:

```sh
cargo test --release -p runtime-sinclair-zx-spectrum \
  --test timing_survey_128k -- --ignored --nocapture
```

Set `EMU198X_SPECTRUM_128K_ROM0`, `EMU198X_SPECTRUM_128K_ROM1` and
`EMU198X_ZX_SPECTRUM_TESTS_DIR`. Ordinary tests cover changed readings,
replacement failures, wrong modes and an unexpected pass. Formatting and
Clippy are checked on the affected test target.
