# Comparator Capabilities

This document answers which audited emulator revisions can currently produce
admissible evidence for the programmable-HBLANK corpus.

A producer is usable only when it implements the path under test and exposes
unscaled output that retains blanking. An unsupported path is not a
behavioural result. A source-audited path without a reproducible capture is
also excluded.

## Current status

| Producer | Revision | Applicable path | Status |
| --- | --- | --- | --- |
| Copperline 0.13.0 | `eec5806778dab8b60f3b05fa7ab2428e4e18b073` | ECS and AGA, CCK-aligned cases | Registered software-derived capture |
| FS-UAE 3.2.35 | `4ae7ddaec50b567ed80d71ffbff067cb58e945a3` | UAE core derived from WinUAE 3300b2 | Unsupported feature; excluded |
| FS-UAE 5.0.7 | `f362278ccd4c60991caac3b4d240d4a3f751bea2` | UAE core derived from WinUAE 6.0.1; ECS and AGA full suite at host HIRES | Registered software-derived capture |
| WinUAE development source | `c32694e338fa5f34977f522eb4898adb069d2e73` | ECS and AGA source paths | Source-audited member of the captured UAE family |
| vAmiga 4.4b12 | `60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` | OCS and partial ECS | Unsupported feature; excluded |

The UAE entries belong to one implementation family. FS-UAE supplies the
executable capture route to the audited current-generation UAE path. A
separate WinUAE capture would remain useful for adapter verification but would
not count as an independent observation.

## Copperline

Copperline's headless raw-frame path produced three byte-identical adjacent
716 by 570 RGBA8 fields for all seven CCK-aligned cases on both the A500 Plus
ECS and A1200 AGA profiles.

The audited renderer implements coarse HBSTRT and HBSTOP placement but does
not use `BPLCON0.ECSENA` or `BPLCON3.EXTBLKEN` as gates. It also omits Lisa's
resolution-dependent fine position bits. The ECSENA and EXTBLKEN observations
are therefore disagreement candidates, and the three AGA fine-position cases
are excluded.

The exact configurations, firmware hashes, capture procedure, APNGs, records,
and interpretation limits are in the
[Copperline capture package](copperline-0.13.0-eec5806/README.md).

## FS-UAE 3.2.35

The installed `aarch64` binary has SHA-256
`2df5f96f9d7346176e743e1d473570f8b88df21f81561a3b9f7aff44ec91e87a`.
It produced byte-stable 752 by 572 RGB full-mask captures before frontend
cropping or scaling.

The fixed, central, wrap, and equal cases contained only their guard colour
on both tested ECS and AGA profiles. Source audit found storage for HBSTRT and
HBSTOP but no `EXTBLKEN`, `ECSENA`, or programmable-HBLANK rendering path.
Those images show an unsupported implementation, not a fixed or empty
hardware result.

The A500 Plus configuration had SHA-256
`0e6d36f513e619acf24ced7e5d55a1d424b41995b402f82d3954677b6b9fd715`.
The A1200 configuration had SHA-256
`c22047afb24966014b47b47e7eb536610838d0dd7b692af4b209c399f95dee4c`.
The probe ADFs and firmware matched the suite 1.0.1 records.

## FS-UAE 5.0.7

The current source identifies itself as FS-UAE 5.0.7 with a core derived from
WinUAE 6.0.1. A capture-only patch copies the complete UAE chipset
`video_memory` buffer before FS-UAE's compatibility crop and frontend
processing. The exact patched macOS arm64 binary has SHA-256
`81fdcc09bf36b6a275a9d39b27407e3484815b5713b411e16dbfe6024cf2899b`.

Seventeen cold-boot runs cover the seven CCK-aligned cases on A500 Plus ECS
and A1200 AGA profiles, plus all three AGA fine-position cases. Each run first
observed the ready record at guest field counter 1, waited eight further
fields, and captured counters 9, 10, and 11. Every three-field set is
byte-identical.

The source-derived raw geometry is 756 by 576 BGRA8888 at host HIRES. Rows 202
and 203 represent doubled beam line 128. In the main capture interval, an HB
word `r` maps to raw sample
`4 * (r & 0xff) + floor(((r >> 8) & 7) / 2) - 184`. The package retains the
raw two-sample left storage pad and four bottom storage rows but excludes them
from semantic blank classification.

The current UAE observations are:

- clearing `ECSENA` or `EXTBLKEN` prevents the enhanced path on both profiles;
- the ECS profile produces no programmed interval when `BLANKEN` is clear;
- the AGA profile still produces the programmed interval when `BLANKEN` is
  clear;
- central, wrapped, and equal comparator cases produce the same semantic
  outcomes as Copperline after each producer's declared coordinate mapping;
- the AGA fine stop value produces raw sample 459 in lores, hires, and
  superhires guest modes on the host-HIRES grid.

The host-HIRES grid combines adjacent Lisa superhires phases. The fine
captures verify the producer path but do not distinguish fine phases 6 and 7.

The exact patch, runner, configurations, firmware hashes, complete logs, run
manifests, APNGs, records, raw frame hashes, and interpretation limits are in
the [FS-UAE capture package](fs-uae-5.0.7-f362278c/README.md) and
[capture adapter](../../../../../tools/fs-uae-hblank-capture/README.md).

## WinUAE source

The audited development revision contains separate ECS and AGA external
blanking paths in `drawing.cpp`. Both require `BPLCON0.ECSENA` and
`BPLCON3.EXTBLKEN`. The AGA path derives its programmable edges from HBSTRT and
HBSTOP, including the fine position bits. The ECS path selects fixed Denise
and CSYNC-derived blanking rather than the AGA programmable comparator.

This source audit supplies the coordinate and feature interpretation for the
registered FS-UAE route. It does not turn FS-UAE and WinUAE into independent
evidence families.

## vAmiga

vAmiga 4.4b12 supports OCS and partial ECS display-chip revisions but no AGA
profile. Its renderer clears a fixed interval derived from `HBLANK_MIN` and
`HBLANK_MAX`.

The custom-register dispatcher does not handle HBSTRT or HBSTOP writes.
`BPLCON3` implements `BRDRBLNK` but not `EXTBLKEN`, and `BEAMCON0` handles PAL
selection and `LOLDIS` without the programmable blanking controls. Register
names exposed to diagnostics do not constitute an implementation.

vAmiga is therefore unsuitable for this corpus version. Running the probe
would measure its fixed fallback, not the programmable path.

## Next admissible comparison

The next evidence-bearing result must come from another independent
implementation family or registered physical hardware. A host-superhires UAE
capture would improve fine-phase resolution but would remain within the UAE
family. Until independent evidence resolves the gate disagreements, those
cases must not be promoted.

## Related files

- [Capture evidence boundary](README.md)
- [Copperline capture package](copperline-0.13.0-eec5806/README.md)
- [FS-UAE capture package](fs-uae-5.0.7-f362278c/README.md)
- [Capture schema](../schema/capture-v1.schema.json)
- [Conformance process](../../../../../knowledge/processes/amiga-programmable-hblank-conformance.md)
