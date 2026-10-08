# Paula host bandwidth investigation

Production uses the approved band-limited step-response ring and snapshot
v62. The original v61 runtime failed all 54 above-band cases in 108
observations; those logs remain here. The current runtime regression covers
198 cases, including 54 in-band controls and 144 stopband cases through
3 MHz. Run it as an ordinary test:

```sh
cargo test --locked --release -p runtime-commodore-amiga --lib \
  host_decimation_rejects_ultrasonic_aliases -- --nocapture
```

The old failing invocation (with `--ignored`) belongs to research commit
`2afd0df0`; using that flag on the promoted regression would select no test.
The standalone C++ prototype remains independent research evidence, not
code linked into the emulator. The implemented design is recorded in the
separate docs plan `2026-10-07-amiga-paula-bandwidth.md`.

It uses synthetic mixer gains around retained full-volume signed samples
to isolate host conversion, not a guest program or physical PWM schedule.
It measures both channels after actual board filtering, requires a positive
1 kHz control, and covers three phases on PAL/NTSC OCS/ECS/AGA. The proposed
stopband acceptance limit is -60 dB relative to source. That is an engineering
target, not a newly inferred hardware characteristic.

Run the independent candidate and its negative control:

```sh
c++ -std=c++20 -O3 -Wall -Wextra -Werror \
  test-data/commodore/amiga/paula-audio/bandwidth-probe/candidate.cpp \
  -o /private/tmp/paula-bandwidth-candidate
/private/tmp/paula-bandwidth-candidate
/private/tmp/paula-bandwidth-candidate --bypass
```

Normal exit is zero, with exactly 66 tone cases and no failures. `--bypass`
must exit 1 and fails 48 cases. Measurements below the threshold are numerical
results, not physical noise-floor claims. The prototype has a 96-frame
finite step response, 256 fractional phases, 22 kHz cutoff, 1 ms delay,
197,376-byte shared table and 1,568-byte C++ state including padding/pointer.
Benchmark output covers one emulated second of stereo changes at four rates;
it excludes machine execution and does not predict final Rust performance.

Execute the pinned WinUAE kernel separately:

```sh
python3.13 test-data/commodore/amiga/paula-audio/bandwidth-probe/reference.py \
  --winuae ../../emulators/amiga/WinUAE \
  --output /private/tmp/paula-bandwidth-reference
```

The source hash manifest is `comparison.json`; the extraction retains the
upstream copyright notice. The adapter runs both deliberately normalised
CCK age and pinned CYCLE_UNIT=512 age. These differ sharply. Neither is a
live WinUAE capture or hardware oracle, and the normalised adaptation is
never presented as the unchanged pinned caller. It uses the vanilla table,
no board filter, and changes the synthetic sine only every eight CCKs to
stay within the reference's stated event capacity. Empty/non-finite output
negative controls must be rejected.

Primary evidence:
`reference/by-system/commodore-amiga/2026-paula-host-bandwidth-observations.md`.
The physical PWM phase and the switched analogue circuit remain open.
The shared helper preserves the caller's clock and saves its tail.
The Amiga integration delays sampled LED control by 48 host frames to
align with the 1 ms signal delay. Snapshot v62 rejects v61 and saves ring,
previous level, cursor, existing integer phase, delayed LED bits and analogue
history. Invalid lengths, cursors and non-finite/out-of-range signal data
fail before changing live state.

The runtime's one-tick pulse tests use independent continuous-kernel Simpson
integration, with a 0.1% error limit relative to pulse peak. An initial
arbitrary 3e-7 full-scale bound failed eight cases: the largest finite-table
approximation error was 3.80e-7. The relative limit expresses the existing
0.1% amplitude requirement. Shared tests separately require pulse area within
1e-8 and retain 512 narrow pulses in one interval. Restore tests refresh the
source at each checkpoint: a 20,000-tick continuation otherwise outlasts the
fixture's last transition and leaves no pending resampler tail to exercise.

Run the explicit Rust runtime-conversion benchmark separately:

```sh
cargo test --locked --release -p runtime-commodore-amiga --lib \
  benchmark_runtime_audio_conversion -- --ignored --nocapture
```

This executes the real mixer, resampler and board filter for one emulated
second at four edge densities. It excludes CPU/chip execution; it is not a
whole-machine speed claim. It checks exactly 48,000 stereo frames and keeps
the output observable to the compiler.

Final integration verification: 589 shared/runtime tests pass, including 65
snapshot tests; strict Clippy, native release build and wasm32 check pass.
Thirty existing explicit diagnostics and the wall-clock benchmark are skipped
by the ordinary suite; the benchmark and three-case ROM audio gate were run
explicitly and pass. The gate thresholds were unchanged. The matched Rust
benchmark's median added cost is 13.327 ms per emulated second at period 124,
210.084 ms at period 1, and 409.390 ms for every-tick mixed stress. Logs retain
the initial pulse/replay failures as well as the corrected verification.
