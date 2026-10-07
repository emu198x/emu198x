# Decision: Preserve Denise's strobe-driven horizontal counter

**Date:** 2026-10-06
**Status:** BINDING

## Evidence

The [primary DMA/wake observations](../../../../reference/by-system/commodore-amiga/2026-copper-blitter-wake-observations.md)
record the registered FS-UAE source and the unchanged display guest. All 2,900
bitplane addresses pair positively: native service occurs four Agnus counter
positions before reference service, despite matching output pixels. This is
evidence of different counter views, not permission to fit a constant offset.

At registered revision `f362278ccd4c60991caac3b4d240d4a3f751bea2`,
`drawing.cpp::do_denise_cck` consumes normal RGA from the preceding cell.
`handle_strobes` selects next counter value 2 on STRHOR and STRVBL. ECS Denise
and Lisa also reset on STREQU; OCS Denise free-runs through STREQU. The output
loops increment the nine-bit counter at each lores tick and commit the selected
next value after the second tick. `denise_handle_quick_strobe` separately names
the refresh offset of three CCKs and pipeline delay of two CCKs. These are
registered implementation observations, not physical-hardware calibration.

## Decision

The user approved extending the existing Denise counter and strobe stages on
2026-10-06 as part of the shared DMA pipeline correction. Keep Denise's counter,
normal RGA strobe, next-counter value and output phase in the existing board
wrapper. Agnus carries a typed strobe through its single DMA authority; delivery
occurs at actual service. No independent strobe arbiter or extra chip tick exists.

The serviced strobe enters normal RGA for one CCK. The following CCK selects
the reset value but outputs both old counter positions before committing it.
Thus service at Agnus h=3 yields Denise position 2 at Agnus h=5. OCS equalisation
does not reset. Counter wrap is nine-bit and independent of Agnus line wrap.

The ongoing version-50 snapshot change retains these stages. Candidate validation
rejects counter values outside nine bits. Restore must preserve pending strobes
and the commit boundary between output ticks without repeating a reset.

Use the independent counter for Denise's hardware comparisons when wiring the
live shared pipeline. Place host output samples using the physical scan
position, independently of that counter. The user approved this bounded
amendment on 2026-10-06 after the combined-RGA trace showed that losing STRHOR
leaves Denise's counter free-running while the host scan continues. Retain
Agnus's counter for DMA and Copper.

The connected host projection derives from the existing physical beam and its
registered scan origin. Across raw line wrap, preserve the preceding physical
line's length and row until the host row boundary. Hardware comparisons still
consume Denise's counter, including its wrap and suppressed-reset behaviour.
This changes no clock, DMA opportunity or version-50 saved field. It narrows
the earlier raster-projection requirement; it does not replace the independent
counter with an adjusted Agnus counter for chip behaviour.
Framebuffer origins are fixed validation inputs; they must not be moved to hide
a disagreement. A constant counter subtraction, future-chip clone or compensating
busy hold is not a substitute for the signal stages.

## Verification boundary

Component tests cover all three resetting strobes, the OCS equalisation exception,
nine-bit wrap, the registered refresh/strobe boundary and restoration at incoming,
pending and half-CCK commit stages. The wrapper restore test exercises OCS, ECS and
AGA policy through actual serialized Denise state. Removing reset makes three
counter tests fail; it is an explicit negative control.

The shared driver now retires admitted timing strobes once and advances the
counter on its existing output ticks. Runtime replay tests cover all three
chipsets, both CPU phases of retained bus ownership, and saves at every reset
commit boundary. Automatic timing requests now sample saved Agnus equalisation/blanking
signals and enter the shared address stage at h=2 for service at h=3. ECS/AGA
feed their existing programmed blank events and equalisation selectors into
that sample. The connected adapters now pass the independent 96-row timing
comparison, including 3,060 actual writes, finish, busy and Copper intervals.
Combined refresh/display service agrees with twelve captured full-reference
cells and 1,000 compiled register/pointer cases. The host-scan amendment reduces
the controlled continuous-DMA raster discrepancy from 275,300 to 524 pixels
per field; the hard-stop control and six WAIT fields remain exact. This is a
software-reference boundary, not physical-hardware calibration. The unchanged
128-guest raster corpus passes all 384 fields and 333,268,992 RGB pixels with
unchanged producer/input hashes. The expanded host-scan test and all 137
shared-chip tests pass, including visible carried samples for both PAL line
lengths. Runtime replay passes for the combined service on all three chipsets.
Broad regressions still report eight failing targets; required Test Kit video
gates retain OCS gradients 6 pixels, AGA gradients 9 pixels and AGA crosshatch
280 pixels. The complete raster, runtime replay and required-media gates must
pass together before accepting the integrated correction; the wrap residual
and other failures remain open.

The follow-up edge trace separates that 524-pixel residual. ECS/AGA were closing
the existing vertical-DIW latch before h=0 requests rather than after h=1,
dropping two final transfers. Correcting that existing comparator boundary
recovers exactly 32 pixels. All 492 remaining differences are in reference host
padding; a separate pre-clipping signal trace covers every one and agrees with
native output. Both VSTART/VSTOP phase tests and half-CCK snapshot replay pass.
The fixed full-image comparison remains explicitly red at those padding columns;
its endpoint and assertions have not changed. See the primary observations and
`test-data/commodore/amiga/rga-conflicts/wrap-edge-observation.json` for source,
producer, input and negative-control evidence. This adds no saved field or
clock and does not establish original-Agnus vertical-close timing.

The subsequent [Test Kit edge correction](../../../../reference/by-system/commodore-amiga/2026-test-kit-display-edge-observations.md)
retains horizontal DIW history separately from this counter. A strobe reset
before HSTOP must not synthesize a window-close event. The approved version-52
snapshot preserves that latch. Together with admitting the final free odd
Copper cell, this removes all three Test Kit image residuals without changing
counter timing or framebuffer geometry. Both strict profile gates now pass;
the separate reference-padding limitation remains unchanged in scope.

## 2026-10-07 Lisa programmed-blank comparator

Counter-traced neutral blanking guests show HBSTRT=$0080 beginning at Denise
255, and HBSTOP=$07A0 ending at 320.75. The previous programmed comparator
used the current counter, one lores period late. As in the already corrected
fixed comparator, compare programmed blank edges against the next lores
counter and retain all four fine samples. Keep start-before-stop ordering,
selector propagation and the existing edge latch. This changes no saved
field, host coordinate or clock. The primary colour/blanking observations
record both failing fields and residual vertical-boundary differences.
