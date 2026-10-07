# Decision: Model Lisa's additional bitplane and display-window output phase

**Date:** 2026-08-08
**Status:** BINDING — phase policy superseded by the approved counter-origin correction below

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

- Lisa retains an additional serial bitplane output tick before sprite/priority
  composition; and
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

The horizontal display gate retains a saved latch between start and stop
equality events. The user approved this extension and snapshot version 52 on
2026-10-06 after the live Test Kit trace showed that a counter reset before
HSTOP falsely closed the former interval predicate. This supersedes that
stateless predicate, while preserving the established before/after-output
phases. See the [primary edge observations](../../../../reference/by-system/commodore-amiga/2026-test-kit-display-edge-observations.md).

Neither the horizontal counter's strobe reset nor the physical line reset
clears the latch. Register changes behind the counter do not synthesize
matches. The current register delivery and low-byte comparator decode remain
unchanged; this does not calibrate mid-line DIW register propagation or add
horizontal DIWHIGH fine-position support.

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

## 2026-10-04 scrolling correction

The former adapter implemented the extra output tick by subtracting one from
Lisa's bitplane comparator coordinate. A DMA-rendered regression exposed a
word-boundary discontinuity at PF1H/PF2H=15. That coordinate subtraction is
superseded by the actual serial scroller/output stage described in
[the scroll-mode decision](amiga-denise-scroll-modes.md). The absolute crop,
DIW equality convention, independently clocked sprites and COLOR stage remain
the decisions here. The static reference gate retains its previous exact
matches and explicitly registered sprite-phase disagreements.

## 2026-10-06 timed window registers and fractional Lisa edges

The user approved Denise-local DIWSTRT/DIWSTOP/DIWHIGH delivery, sample-level
window state, and snapshot version 53 (rejecting 52). This supersedes the
low-byte-only and immediate-Agnus-register model boundary above. It preserves
the installed chip's existing comparison phase and the beam-absolute crop.

`common-commodore-amiga::denise_window` retains the normal RGA register
stage separately from Agnus's vertical window. DIWSTRT/DIWSTOP delivery
clears explicit-high mode; a subsequent DIWHIGH delivery selects coarse
horizontal bits 5/13 and, on Lisa, fine bits 4:3/12:11. ECS ignores the fine
bits. Its DIWHIGH stage retains the additional half-CCK described by the
reference's unaligned ECS path; this is software-reference precedent.

Lisa compares each existing 35 ns sample and retains the four levels through
its established one-lores output stage. Playfield selection, sprite
visibility/priority, collisions, BPLAM, HAM and border blanking consume those
sample levels. Masking the final framebuffer would leave those internal
operations inconsistent with the image.

The seventeen A1200 diagnostics and their failing baseline are retained in
`test-data/commodore/amiga/horizontal-window/`. Consult its validation record
for completed results and limits; additional ECS observations do not justify
claiming that every chipset's absolute window phase is calibrated.

## 2026-10-06 approved counter-origin correction

The [ECS output-origin investigation](../../../../reference/by-system/commodore-amiga/2026-ecs-output-phase-observations.md)
finds that the exploratory UAE capture's reported origin omits four samples of
line-output padding. OCS and ECS controls agree with native output in counter
space. Two AGA controls establish the same reference origin and expose a native
window/data edge one lores tick late. Thus the absolute-image rationale for
Lisa's additional tick above is no longer reliable.

The user approved the bounded correction on 2026-10-06. Lisa now composes
DIW equality on the matching sample, including fractional edges, and emits
the held serializer sample without another four-sample delay. This supersedes
the additional-tick policy and its absolute-image verification claims above.
The history fields remain in snapshot 53; register delivery and fine decoding
are unchanged. Independent sprite and COLOR controls also confirmed an extra
lores tick; their separate decisions record those corrections. All 72 final
diagnostic fields and both strict Test Kit lanes now agree.
Native coordinates and framebuffer geometry remain unchanged.
