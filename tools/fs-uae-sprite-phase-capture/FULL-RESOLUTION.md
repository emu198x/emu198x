# Full-resolution diagnostic capture

On a writable copy of the registered FS-UAE 5.0.7 source, apply the normal
`fs-uae-5.0.7-sprite-phase-capture.patch`, then
`fs-uae-5.0.7-full-resolution-host.patch` and
`fs-uae-5.0.7-raster-origin.patch`. Do not modify the registered snapshot.

Build with the upstream bootstrap/configure/make workflow. The executable must
run next to the development `od-fs` Python/resources and `Portable.ini`.

Set `FSEMU_CODEX_SPRITE_FULL_RESOLUTION=1` alongside the capture hook's
`FSEMU_CODEX_SPRITE_PHASE_CAPTURE_DIR`,
`FSEMU_CODEX_SPRITE_PHASE_CASE_NUMBER=1`, and
`FSEMU_CODEX_SPRITE_PHASE_MIN_FIELD_COUNTER=9` switches.
The guest must publish the original SPHX ready-record layout.

The additional patch only fixes host output preferences: it requests
RES_SUPERHIRES before output geometry is allocated, keeps subsequent preference
parsing/graphics initialization consistent, and disables automatic host
resolution changes. It does not change guest registers or chipset logic.
Without the switch the patch is inert.

Read the raw metadata and require 1512×576 packed BGRA8888 fields. Keep all
samples, overscan and hardwired blanking. Close the development process after
`CODEX_SPRITE_CAPTURE complete`; the hook stops recording after three fields
but does not itself quit the frontend.

The pinned `capture.sh` records the earlier, admitted hires producer. Its
binary/suite hashes must not be bypassed for these exploratory captures.
Record a separate manifest with source revision, all applied patches, binary, ROM,
ADF, configuration and raw-buffer hashes. These observations do not supply
independent original-hardware calibration.


Require the captured `inbuffer_xoffset=368`, `inbuffer_yoffset=52` and
`host_resolution=2`. The source-defined common PAL raster maps native
`(16, 2)..(1528, 576)` to reference `(0, 0)..(1512, 574)`. Compare all RGB
samples there; separately record the unmatched producer edges. Beam line
132 is reference row 212, not 210.

For sprite FMODE=2, registered FS-UAE 5.0.7 contains a producer bug: its
32-bit page-mode request takes `fetch64`. The separate
`fs-uae-5.0.7-upstream-sprite-page-mode.patch` ports the exact condition from
registered WinUAE `c32694e338fa5f34977f522eb4898adb069d2e73`: modes `<3`
use `fetch32_spr`. This changes reference chipset logic and must be recorded
as a distinct exploratory producer. Retain the original failed captures.
It is not part of the admitted producer or an independent hardware reference.
