# Spectrum composite experiment

An offline best-in-class accuracy experiment: does modelling the Spectrum's
analogue encoder and a PAL receiver expose useful behaviour missing from the
current RGB framebuffer plus CRT shader?

Run from the emulator repository:

```sh
python3 tools/spectrum-composite/report.py
```

Requires Cargo, Python with Pillow, and a native wgpu adapter. On a restricted
macOS process, Metal may report no adapter; run the comparison renderer with
GPU access. The generated report is `target/spectrum-composite-comparison/index.html`.
The original offline binaries have their own Cargo workspace and lockfile.
The live receiver derived from the experiment is now in the shared native-video
crate; its selectable paths leave the default renderer, machine timing and
digital golden images unchanged.

To include a fresh raw Spectrum capture:

```sh
target/release/emu198x-spectrum --rom ../roms/48.rom --headless \
  --frames 120 --screenshot target/spectrum-composite-comparison/live-boot.png
python3 tools/spectrum-composite/report.py \
  --png target/spectrum-composite-comparison/live-boot.png
```

Other raw 352×296 Spectrum screenshots can be supplied with `--png`. The reader
accepts exact 194/255 and historical 205/255 RGB palettes and rejects images
containing other colours; no nearest-colour fit conceals incompatible inputs.

## Live four-machine experiment

From the emulator repo:

```sh
cargo run --release -p emu198x-spectrum -- --rom ../roms/48.rom --video signal
cargo run --release -p emu198x-c64 -- --rom-dir ../roms/c64 --video monitor
cargo run --release -p emu198x-nes -- \
  --rom test-data/synthetic-cartridges/nintendo-nes-logo.nes --video signal
cargo run --release -p emu198x-amiga -- --kickstart ../roms/kick13.rom --video monitor
```

`signal` decodes machine electrical output; `monitor` selects an available RGB
or separate luma/chroma connection. Spectrum 48K-family and NTSC NES are the
current baseline sources. C64 supports PAL/NTSC, composite and luma/chroma;
Amiga feeds its native RGB source. RF modulation/demodulation is not implemented.
These are selectable experiments, not hardware-validated replacement defaults.

For a modern monitor/upscaler presentation, use `--video modern --scale 2`
(composite or native RGB) or `--video modern-monitor --scale 2` (separate
luma/chroma or RGB). These options retain signal decoding and use nearest
scaling without CRT effects. The video menu exposes both options. Pixel-aspect
correction is preserved; larger integer window scales are supported. This is
a generic retained-raster scaler, not a model of a named device. Interlaced
Amiga output displays both retained fields steadily in every native mode.
The legacy `modern-weave` and `modern-monitor-weave` names produce the same
stable presentation as their corresponding modern options. CRT modes retain
provisional whole-raster phosphor afterglow without emulating field flicker.

The following offscreen experiment probes explicit field rendering; native
presentation does not select those field modes:

```sh
cargo run --release -p emu198x-native-video --example validate-interlace -- \
  target/interlace-validation > target/interlace-validation.log
python3 tools/spectrum-composite/interlace_report.py
```

The production shader renders an explicitly synthetic detail/motion plate.
The report offers manual field stepping and optional nominal-PAL playback;
its browser animation is an illustration, not a measured display cadence.

The production GPU path can be validated without a window:

```sh
mkdir -p target/signal-live-validation
cargo run --release -p emu198x-native-video --example validate-signal -- \
  target/signal-live-validation > target/signal-live-validation/validation.log
python3 tools/spectrum-composite/live_report.py
```

The validator uses local Spectrum/C64/Amiga ROM files and the
project-owned NES cartridge. It compares independent f64 Spectrum results at
three field phases and the live master-clock handoff, then generates receiver
images from the four runtime contracts. The Amiga detail plate is explicitly
synthetic. Timings exclude CRT and window presentation. The original offline
comparison below still provides the additional Spectrum sampling and phase
controls.

See [the decision and implemented limits](../../knowledge/decisions/electrical-video-receiver.md)
for the boundary and provenance. Source amplitudes, receiver calibration,
full-raster sync/blanking capture, RF, recovered interlace sync and measured
phosphor calibration remain accuracy work. Electrical sidecars do not change raw screenshot bytes or snapshots.

## Approach

The pilot targets static frames from the 48K 6C ULA family. It compares:

1. Current raw palette.
2. Documented ULA Y/U/V pin levels, converted directly through an ideal RGB decoder.
3. The same levels filtered as separated luminance and chrominance, a control
   that isolates bandwidth effects from cross-colour interference.
4. The levels modulated onto a PAL carrier and decoded with a simple receiver.

The receiver low-passes composite for luminance, subtracts that result to
estimate chrominance, synchronously demodulates U/V, and low-passes each colour
difference channel. PAL V alternates on physical scanlines. After undoing the
alternation, a one-line chroma average models one receiver choice.

All FIRs are odd-length windowed-sinc kernels with unit DC gain. The experiment
uses 4 samples per pixel (28 MHz) and repeats a difficult pattern at 8 samples
per pixel (56 MHz), scaling the kernels to retain the same time support. Output
is interpolated to the same pixel centre. Filter group delays are removed by
symmetric offline convolution; this is not a causal live implementation.

The colour carrier advances through complete 448-pixel lines and 312-line
fields. Quarter-cycle phase and subsequent-field comparisons expose sensitivity
that a static RGB blur cannot express. Filter/oscillator setup is outside the
reported processing timings; allocations and decoding are included. Three warm
release runs supply each median. These CPU measurements do not estimate GPU cost.

The report's main sliders feed raw and composite-decoded images through the
**same unmodified production WGSL shader**, rendered offscreen by wgpu. This
holds tube styling constant while changing the signal path. Screens use fixed
3× square-pixel geometry and an `Rgba8Unorm` render target; they do not establish
the production window's aspect, crop or selected surface format. Four earlier
stages are shown without CRT presentation for diagnosis.

## GPU acceleration

`decode-gpu` implements the same signal model in four cached compute passes:

1. Expand packed indices, apply the shared ULA voltage table and encode composite.
2. Separate luminance and synchronously demodulate chrominance.
3. Apply the shared chroma FIR and interpolate to pixel centres.
4. Average the PAL delay line, convert to RGB and write an `Rgba8Unorm` texture.

The production CRT shader samples that texture in the same command submission.
There is no CPU picture download/re-upload between decoding and presentation.
Buffers, bind groups, pipelines, within-line carrier and filter coefficients
are reused. Only picture indices and a small per-line phase-reference table
are uploaded each frame. CPU f64 phase reduction supplies those references;
large running timestamps are never reduced in a GPU f32 oscillator.

GPU arithmetic uses f32; coefficients and colour levels come from the same
functions as the CPU oracle. CPU work is deliberately retained for validation.
Every GPU run reads back unclipped YUV and final RGBA once, failing if maximum
YUV error exceeds 0.0001 or any RGB channel differs by more than 1/255. It seeds
a different input picture before benchmarking the requested one to exercise
resource reuse. Eight input/configuration checks in the report cover bars,
stripes, dithering, historical and fresh boot captures, 8× sampling, quarter-cycle
phase and a subsequent field. Fresh capture validation is present when `--png`
is supplied. No test silently skips GPU verification: lacking an adapter aborts
the report, while the standalone numerical CPU tests still run without a GPU.

Each benchmark warms eight fields and measures sixty completed frames with
changing phase references. Wall-clock timings include per-frame uploads,
command construction, encode/decode/resolve, submission and waiting for GPU
completion. A second distribution includes CRT drawing at 1056×888. Setup,
verification readback and PNG/file exports are excluded. These are **offscreen**
costs, excluding machine execution, window composition and vsync; they do not
establish end-to-end live emulator latency or performance on other adapters.

The report's sliders now show the accelerated result. GPU images and both
timing distributions are retained alongside the CPU controls in each fixture's
directory. `results.json` records numerical parity and the adapter name. Run
one input independently with:

```sh
tools/spectrum-composite/target/release/decode-gpu \
  target/spectrum-composite-comparison/dither-grid/source.idx \
  target/spectrum-composite-comparison/dither-grid
```

## Evidence and assumptions

Primary prose evidence is held in the shared library:

- [Smith Chapter 16](../../../../reference/by-system/sinclair-zx-spectrum/zx-spectrum-ula-chapter-16-analogue-video.md):
  Tables 16-1, 16-2 and the non-inverted half of 16-3; luminance inversion;
  bright affecting Y without changing U/V; black/white zero chroma.
- [Smith Chapter 11](../../../../reference/by-system/sinclair-zx-spectrum/zx-spectrum-ula-chapter-11-video-synchronisation.md):
  Table 11-1 for horizontal blanking/sync, plus Chapter 16's burst window.

Implementation precedent consulted:

- [Clock Signal Spectrum video](https://github.com/TomHarte/CLK/blob/master/Machines/Sinclair/ZXSpectrum/Video.hpp),
  [Metal encoder](https://github.com/TomHarte/CLK/blob/master/OSBindings/Mac/Clock%20Signal/ScanTarget/Shaders/Fragments.metal),
  and [filter generator](https://github.com/TomHarte/CLK/blob/master/Outputs/ScanTargets/FilterGenerator.cpp):
  timed output, colour-phase-aware modulation and separate decoder filtering.
  This is an independent implementation, not a Clock Signal port or match.
- Vendored SpecIde `source/src/ULA.cc`: raster and synchronisation precedent.
- Frozen donor CRT shaders: inspected for precedent, not ported; they model
  presentation from RGB rather than the signal path being tested.

ULA pin voltages are black-referenced and picture-oriented. Bright-white Y is
normalised to 1. Chroma gains align the pin-table blue U and red V excursions
with nominal PAL coordinates. That gain calibration is an **assumption**; the
actual PCB amplifier/modulator and television gains require evidence.

The shared Chapter 16 reference flags the bright-yellow Y table cell as shadowed.
The pilot uses its provisional 0.259 V reading and tests that implementation
choice, not independently established equality with bright white. The inverted
V table has small asymmetries; the pilot models ideal sign inversion instead.

The 4.43361875 MHz ideal carrier, 3 MHz luminance cutoff, 1.3 MHz chroma cutoff,
windowed-sinc filter family, ideal phase reference and simple delay-line average
are explicit receiver assumptions. No tuning is justified by resemblance to a
particular screenshot. They are not measurements of one historical television.

The existing 48K crop is reconstructed using `CONFIG_48K`'s pixel mapping
(`p+36` / `p-412`) and vertical mapping (`scan+48` / `scan-264`). Full horizontal
blank, sync and burst are synthesised outside it. The decoder receives an ideal
reference; **burst and sync do not establish lock**. Vertical sync, RF, component
tolerances, encoder edge response, decoder PLLs, phosphor persistence and live
chip output are outside this experiment. The static-frame adapter cannot recover
events lost at the framebuffer boundary, including writes spanning physical
line/field boundaries. It is not a substitute for a timed output contract.

The boot golden's historical normal intensity (205) is recovered as logical
indices and displayed using the current raw intensity (194). Synthetic patterns
and this golden require no ROM. A supplied runtime capture is separately hashed.
`results.json` records inputs, code hashes, shader hash, environment and timings.

## Validation and interpretation

```sh
cargo test --release --locked --manifest-path tools/spectrum-composite/Cargo.toml
cargo clippy --release --locked --manifest-path tools/spectrum-composite/Cargo.toml \
  --all-targets -- -D warnings
cargo fmt --check --manifest-path tools/spectrum-composite/Cargo.toml
```

Tests check documented encoder invariants, receiver DC/passband/stopband gain,
solid-colour recovery on both PAL polarities, false chroma from monochrome detail
only in the combined path, carrier continuity across the ULA frame-counter wrap,
and malformed input rejection. They establish the
experiment's mechanisms; they do not certify hardware accuracy.

The experiment earns further work if it exposes repeatable signal-dependent
behaviour absent from RGB presentation and those effects survive sampling checks.
It earns becoming the default only after measured machine output and receiver evidence
constrain the amplitudes, phase and filtering. Clock Signal is a useful comparator
for implementation, not an independent physical measurement.

## Phosphor afterglow

CRT modes use a shared linear-light afterglow pass, driven by the machine's
clock. `--phosphor-ms N` sets the provisional 1/e decay time; the default is
6 ms and zero disables it. Longer values leave stronger trails. Modern, raw
and LCD displays bypass persistence. Repeated host redraws do not recharge it,
and reset/restore/connection changes clear incompatible histories.

```sh
cargo run --release -p emu198x-native-video --example validate-phosphor -- \
  target/phosphor-validation > target/phosphor-validation.log
python3 tools/spectrum-composite/phosphor_report.py
```

The report compares the same synthetic plate at zero, 6 ms and 20 ms. It is
not a measured monitor profile; per-channel decay, within-field beam age and
missed-field excitation are not yet modelled.

For a simultaneous moving comparison using a captured Amiga picture, supply a
768×576 RGBA file as the second validator argument:

```sh
cargo run --release --locked -p emu198x-native-video --example validate-phosphor -- \
  target/amiga-display-preview target/amiga-display-preview/workbench.rgba
python3 tools/spectrum-composite/amiga_preview.py
```

The preview script requires Pillow and FFmpeg. It produces a 50-field-per-second
movie, a separately labelled ten-times-slower movie, and a simple player. Each
movie alternates the two production-rendered fields; text details retain the
original rendered pixels. The captured AmigaDOS picture is progressive and is
replayed as fields to review the display model. It is not an interlaced guest
recording or calibrated host-refresh comparison.

For a native-window demonstration with actual guest-enabled interlace, first
capture the AmigaDOS picture as `target/amiga-display-preview/workbench.png`,
then run:

```sh
python3 tools/spectrum-composite/amiga_live_demo.py
target/release/emu198x-amiga \
  --kickstart target/amiga-display-preview/live-interlace-demo.rom \
  --video monitor --phosphor-ms 6 --scale 1
```

This builds project-owned diagnostic firmware, copies two 640×512 bitplanes
into chip RAM, enables HIRES/LACE, and resets the DMA pointers to alternating
rows at each LOF transition. The captured picture and added thin text/window
edges run through the real guest CPU, chipset and native display path. It is a
custom guest demonstration, not Workbench operating in interlace. Native presentation now displays this retained raster steadily. Compare
`monitor` with `modern-monitor` using the display menu or launch option.

Native display modes deliberately omit field flicker and bob deinterlacing.
Earlier alternating-field movies illustrate the previous experiment; they do
not represent current native presentation. Both retained fields receive the
same monitor treatment and whole-raster phosphor update.

## Real Amiga software review

Use original software sessions to judge the display. The `review_session`
example restores a snapshot through the normal runtime and runs it in the
production native UI, with the normal keyboard, mouse, joystick and View menu:

```sh
cargo build --release --locked -p emu198x-amiga --example review_session
EMU198X_FRAME_STATS=1 target/release/examples/review_session \
  ../roms/kick13.rom /path/to/workbench-1.3.adf \
  target/real-amiga-validation/workbench-desktop.state monitor
```

The snapshot must be from the A500+A501 PAL model used by this review helper.
It contains guest state produced by booting the real disk; the helper neither
patches guest memory nor replaces the framebuffer. Choose `modern-monitor`
for the corresponding modern display. Page Up enables joystick keys (arrows
and Space) in the Amiga UI; otherwise those keys reach the Amiga keyboard.

The local review boots Workbench 1.3 for 3500 fields (or the U.K. disk for
4000 fields), allowing the complete startup sequence to finish. Sqrxz uses
[Retroguru's official Amiga OCS floppy release](https://www.sqrxz.de/sqrxz/).
The downloaded disk is hashed in the local `sqrxz-source.json`; title, menu,
story and game transitions are driven by ordinary joystick fire input.
The raw gameplay MP4 records actual emitted frames and audio, without CRT
styling, at the recorder’s nominal 50 fps. The emulated PAL clock is about
50.08 Hz, so this video is not an exact host-cadence reference. It preserves evidence of
software execution, not to measure host-window cadence.

`EMU198X_FRAME_STATS=1` opts into shared native-UI diagnostics. Each report
covers 300 intervals between unique frames actually submitted to the GPU
surface. Occluded/timeout/reconfigured surfaces and repeated redraws do not
count; an interrupted partial measurement is discarded. The report includes
host cadence, machine elapsed time, skipped-frame intervals, and host time
spent uploading/submitting/presenting. It does not measure GPU completion or
physical monitor scanout. Keep source media unchanged and use local snapshots
and captures under `target/` for review artifacts.
