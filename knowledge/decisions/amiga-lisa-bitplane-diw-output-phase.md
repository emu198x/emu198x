# Decision: Model Lisa's additional bitplane and display-window output phase

**Date:** 2026-08-08
**Status:** BINDING

## The question

Which Lisa display effects require one additional lores output tick relative
to the shared OCS/ECS pixel core?

## Evidence

The A1200 Test Kit reference is a raw FS-UAE chipset framebuffer. Its fixed
horizontal transform is beam-absolute: FS-UAE raw `x=0` represents coarse
horizontal position 46, Emu198x framebuffer `x=0` represents CCK 44, and
therefore `Emu x = FS-UAE raw x + 8`.

The earlier `+6` crop was selected from bitplane-only patterns. It made the
checkerboards, dots and crosshatch agree while hiding a two-host-HIRES-sample
early bitplane parallel-load phase. Under the absolute mapping, delaying the
Lisa bitplane comparator by one lores tick aligns those patterns without
changing the producer references.

The remaining right-edge differences mapped exactly to horizontal display
window equality ticks. Emu198x treated `HSTART` as visible and `HSTOP` as
hidden. The FS-UAE A1200 output retains the matching tick at `HSTOP` and opens
after the matching tick at `HSTART`.

These shifts are not shared Denise behaviour. The A500 vAmiga package has its
own beam-absolute crop at runtime `x=20`. Moving the OCS bitplane and display
window phases produces widespread disagreements in all four bitplane cases.
Restoring the OCS phases makes those cases exact again.

## The decision

The OCS/ECS pixel core retains these phases:

- a pending bitplane parallel load uses comparator phase `beam_x - 1`; and
- the horizontal display gate is active on `[HSTART, HSTOP)`.

Lisa adds one lores output tick without changing absolute sprite coordinates:

- the AGA adapter presents `beam_x - 1` to the shared bitplane comparator,
  giving an effective `beam_x - 2` phase; and
- Lisa's horizontal display gate is active on `(HSTART, HSTOP]`.

This is a variant timing policy, not a framebuffer offset. The runtime crop
remains derived from beam coordinates. Sprite coordinates continue through
their independent absolute comparator path, and `COLORxx` propagation remains
governed by the separate Copper and Lisa colour-stage decisions.

## Model boundary

The AGA evidence comes from one UAE-family A1200 software observation. The OCS
boundary comes from one vAmiga-family A500 observation. Together they justify
keeping the model-specific phases distinct; they do not establish a transistor
level explanation or physical-hardware consensus.

ECS Super Denise currently retains the shared OCS phase. The registered
programmable-HBLANK cases constrain blanking and Copper colour timing, not an
ECS bitplane parallel-load edge. A future ECS bitplane probe may refine that
default without changing the AGA observation.

The registered Test Kit patterns program `DIWSTRT` and `DIWSTOP` before the
captured steady state. The current interval policy therefore records the
observed stable-boundary transfer relation; it is not a serialized
history-sensitive comparator latch for mid-line window-register rewrites.
Write-ahead, write-behind and same-position DIW changes require a focused
probe before that behaviour is claimed.

The A1200 Test Kit pointer remains a separate sprite-position question. It is
not evidence for moving Lisa bitplanes, DIW comparators or the shared sprite
shifter to clear an image diff.

## Verification

Focused tests pin:

- the OCS pending bitplane load on its next output tick;
- Lisa's additional effective bitplane tick while preserving the supplied
  sprite coordinate;
- OCS/ECS `[HSTART, HSTOP)` equality semantics;
- Lisa `(HSTART, HSTOP]` equality semantics; and
- the early-DDF pipeline behaviour at the unchanged OCS phase.

With the absolute A1200 crop, EBU bars, dots and crosshatch are exact. Every
remaining A1200 difference is confined to the independently tracked pointer
footprint. With the unchanged A500 crop, the checkerboards, dots and crosshatch
are exact and only the separately tracked Copper colour cases disagree.

The A1200 Workbench 3.1 boot regression was requalified separately. Its
playfield moved by the same two host samples while the pointer retained its
absolute sprite coordinate; the complete new frame remains exact in subsequent
matrix runs without an ignored region.

## Drift triggers

Reject these patterns:

- moving a reference or runtime crop to compensate for content timing;
- applying Lisa's additional phase to OCS or ECS without evidence;
- shifting the absolute sprite comparator with the bitplane coordinate;
- changing `COLORxx` timing through the bitplane phase; or
- describing a registered software-family observation as physical-hardware
  proof.

## Related Documents

- [Separate Copper colour writes from post-output writes](amiga-denise-color-output-phase.md)
- [AGA Lisa colour-output delay](amiga-lisa-color-output-delay.md)
- [Advance the Denise pipeline across the full projected raster](amiga-denise-full-raster-pipeline.md)
- [Amiga sprite horizontal output phase](amiga-sprite-horizontal-output-phase.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)
