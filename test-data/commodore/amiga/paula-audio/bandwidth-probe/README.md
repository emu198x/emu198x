# Paula host bandwidth investigation

Production still uses v61 interval averaging. Its real runtime diagnostic
fails all 54 above-band cases in 108 observations. The standalone candidate
passes 66 tone cases and four short-pulse area checks. It is not linked into
the emulator. The next shared resampling stage and snapshot v62 are proposed,
pending approval in the docs plan `2026-10-07-amiga-paula-bandwidth.md`.

Run the known-gap diagnostic explicitly (expected Cargo exit 101):

```sh
cargo test --locked --release -p runtime-commodore-amiga --lib \
  host_decimation_rejects_ultrasonic_aliases -- --ignored --nocapture
```

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
The proposed shared helper preserves the caller's clock and saves its tail;
the Amiga integration also needs delayed LED-control history to align with
the 1 ms signal delay. No production pipeline, filter or snapshot changed
in this research commit.
