# Electrical video sources and shared receivers

## Decision

On 2026-10-01 Steve authorised taking the Spectrum composite experiment live
and adapting it for the first four machines: Spectrum, C64, NES and Amiga.
The runtime supplies the electrical source and timing alongside its raw frame;
the shared native-video crate owns the receiver and GPU presentation. Raw
pixels remain the capture and regression contract.

Connection is a separate choice from machine identity. The first live version
exposes `--video signal` and `--video monitor`, as selectable experiments.
Neither changes the existing default. The former decodes composite sources,
or a native RGB source; the latter uses separate luma/chroma or native RGB.
Unsupported electrical sources and unavailable monitor connections are rejected.

## Implemented boundary

`FramePacket::signal` carries retained pre-RGB colour codes, an electrical
palette or periodic waveform, source pixel clock, physical line length,
retained-picture origin and source oscillator phase. `CapturedFrame` owns
that information across the runtime/window handoff. The optional sidecar lets
other systems continue emitting their existing digital frames.

The GPU performs four cached passes: source synthesis, luminance separation,
chroma demodulation and pixel resolve. PAL sources reverse V by physical line
and the receiver averages chroma over adjacent lines. Separate luma/chroma
avoids the composite mixing and separation stages. RGB uses a nominal 5 MHz
monitor bandwidth, without composite colour interference. The decoded texture
feeds the shared presentation shader directly; presentation does not download
and re-upload a decoded frame.

| Source | Current live coverage | Connection |
|---|---|---|
| Spectrum | 48K-timing Ferranti baseline, including 16K/48K/+ | PAL composite before RF |
| C64 | PAL and NTSC runtimes, pre-RGB VIC colour codes | Composite; separate luma/chroma monitor |
| NES | NTSC 2C02, masked colour plus per-pixel emphasis | Direct composite waveform |
| Amiga | Native chipset RGB at the runtime's output clock | RGB monitor |

The Spectrum source reuses the pin-voltage model and explicitly assumed gains
from `tools/spectrum-composite`. Its master clock is 14 MHz while source pixels
are 7 MHz: phase advances from the runtime's actual clock units. NES phase is
anchored to the PPU raster and elapsed dots, including the skipped odd-field
dot; repeated host redraws do not advance the electrical oscillator.

The VIC and PPU retain codes at their existing final pixel muxes. Their digital
framebuffers and timing are unchanged. The mirrors are transient and skipped
by serialization, preserving snapshot bytes. The VIC's fixed RGB palette is
one-to-one, so `Vic::rebuild_signal_codes` reconstructs its code cache from the
saved framebuffer. `C64::restore_snapshot_state` invokes that typed hook before
returning, rather than waiting for the next rendered cell. The C64 serde audit
admits only this exact field and still rejects every other unreviewed skip.
The runtime regression renders all sixteen colours on all four C64 profiles,
checks codes immediately after restore, and replays a pending palette write. A restored NES needs a complete
field before its electrical mirror can describe that field; partial debug
captures deliberately do not claim a complete electrical frame.

## Modern display

On 2026-10-02 Steve requested a modern monitor with a pixel/line doubler.
`--video modern` keeps the composite/native RGB receiver and presents its
output using nearest-neighbour scaling without CRT warp, scanlines or mask.
`--video modern-monitor` does the same for an available separate luma/chroma
or RGB connection. Both are also available in the native video menu.
`--scale 2` requests a double-height window; larger integer scales use the
same path. Existing pixel-aspect correction is retained, so horizontal sizing
can differ from an exact doubling of source pixels. A window smaller than the
source uses the existing fractional fit fallback.

This represents a generic scaler displaying the retained decoded raster,
not a particular commercial device. Electrical artefacts survive the scaler;
CRT styling does not.

## Interlaced fields

The Amiga board already writes long fields to even framebuffer rows and short
fields to odd rows; progressive output duplicates each composed line. The
runtime now publishes completed interlace field sequence and row parity in the
electrical sidecar. It samples the field identity before Agnus wraps, since
Denise finishes the prior row after Agnus advances LOF. Partial fields and
fields whose LACE/LOF changes within the raw field do not advertise a stable
interlace identity. Chip timing, framebuffer pixels and snapshots are unchanged.

Native presentation displays the complete retained raster as a stable signal
in every mode. CRT modes apply the receiver/beam/mask treatment to both retained
fields together; modern modes display the same raster without CRT styling.
There is no field-dependent beam displacement, bob fallback or emulated interlace
flicker. This deliberately omits the monitor's temporal interlace behaviour;
retained fields can still contain motion combing from the source raster.

The View menu exposes one modern display option per connection. The existing
`modern-weave` and `modern-monitor-weave` CLI names remain compatible synonyms
for stable modern presentation. The offscreen `validate-interlace` experiment
retains explicit field-isolation/bob shader probes, but those modes are not
selected by the native presenter.

Authority for retained field layout: [Amiga graphics display reference, §1.7](../../../../reference/by-system/commodore-amiga/amiga-graphics-display.md).
Runtime lifecycle tests verify the existing board mapping and completed-row
boundary. Display simplification does not change the chipset or raw captures.

## Phosphor afterglow

CRT modes now retain source-space light in two cached RGBA16Float textures.
One extra GPU render pass updates them before the existing beam/mask shader;
there is no readback. For each channel, retained light decays by
`exp(-elapsed_seconds / tau_seconds)`. The complete retained frame
recharges light to `max(decayed_light, drive_linear)`, regardless of field parity. This is an ideal fast-recharge model,
not a measured phosphor excitation or saturation curve. Linearisation uses
the existing CRT shader's gamma-2 approximation. The stable raster receives the existing beam, mask
and display transfer function without field-specific offsets.

The shared UI passes the machine profile's authoritative rational clock into
the presenter. Only new machine timestamps/field identities update history;
repeated redraws and paused views retain the same image. Time/counter rollback,
connection/clock/decay changes and progressive/interlace transitions clear
incompatible light. Raster geometry changes rebuild the history. Explicit
reset, quick-load, foreign-state load and machine switch clear temporal state
even when a restored timestamp moves forwards. Modern/raw/LCD modes bypass
history and switching away from CRT clears it.

`--phosphor-ms N` chooses the 1/e decay time, with zero disabling afterglow.
The default 6 ms and the comparison's 20 ms are provisional, equal-channel
parameters, not measured profiles for a Commodore monitor or domestic TV.
Unknown timestamp clocks bypass the model. The A500/A2000 technical reference
explicitly describes a high-persistence monochrome monitor as reducing
interlace flicker: [primary manual transcription](../../../../reference/by-system/commodore-amiga/commodore-amiga-a500-a2000-technical-reference-manual-1987-commodore-text.docling/commodore-amiga-a500-a2000-technical-reference-manual-1987-commodore-text.md).
It supplies no decay calibration for the present RGB model.

`validate-phosphor` checks the production GPU pass's exponential afterglow,
whole-raster illumination despite field metadata, redraw idempotence, restore clearing and first-exposure
agreement with existing CRT rendering. The synthetic plate compares disabled,
6 ms and 20 ms afterglow. Timing includes upload, one source-space update and
GPU completion; it excludes decoding, CRT rendering and window presentation.
Within-field beam age, missed-field excitation, measured per-channel phosphors,
and synchronisation to host refresh remain outside this sampled model.

## Evidence and limits

Spectrum hardware source:
[ULA analogue-video distillation](../../../../reference/by-system/sinclair-zx-spectrum/zx-spectrum-ula-chapter-16-analogue-video.md).
C64 calibration provenance:
[VIC-II measurement/precedent record](../../../../reference/by-system/commodore-c64/vic-ii-video-levels.md).
NES measurement provenance:
[2C02 waveform record](../../../../reference/by-system/nintendo-nes/2c02-video-levels.md).
Amiga connection authority:
[A520 manual](../../../../reference/by-system/commodore-amiga/1988-a520-video-adapter-user-s-manual.md),
which distinguishes RGB input from its composite and RF outputs.

This is a sampled electrical receiver over a retained raster, not a complete
television. Omitted sync/blanking events are not captured from the chips. The
oscillator is an ideal reference rather than a recovered colour-burst PLL.
There is no RF tuner/modulator model, measured receiver response, recovered
interlace sync or hardware-capture agreement claim.
C64 gains and phase remain calibration assumptions; the 8565 is not independently
calibrated. PAL NES and non-48K Spectrum electrical models are not implemented.
Amiga A520 composite/RF is not simulated by the RGB path.

Ultimate accuracy requires preserving the full timed signal, including events
outside the retained picture, then validating each source/connection/receiver
against measurements. This implementation establishes the shared host boundary
and selectable live paths without claiming that later work is already done.

## Verification

`cargo run --release -p emu198x-native-video --example validate-signal` drives the
production decoder offscreen, compares Spectrum fields against the independent
f64 CPU experiment, and exercises all four runtime handoffs. It uses local ROMs
and a project-owned synthetic NES cartridge; generated images stay in `target/`.
Its warmed decode timing includes per-frame uploads, four compute passes and
GPU completion, and excludes machine execution, resource creation, readback,
CRT rendering and window presentation.
