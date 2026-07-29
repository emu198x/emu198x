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
| FS-UAE 5.0.7 development source | `f362278ccd4c60991caac3b4d240d4a3f751bea2` | UAE core derived from WinUAE 6.0.1 | Raw capture unavailable; excluded |
| WinUAE development source | `c32694e338fa5f34977f522eb4898adb069d2e73` | ECS and AGA source paths | Source-audited candidate; no capture registered |
| vAmiga 4.4b12 | `60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` | OCS and partial ECS | Unsupported feature; excluded |

The UAE entries belong to one implementation family. A future FS-UAE and
WinUAE capture agreement would not count as two independent observations.

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

## Current FS-UAE source

The current source built without changes and identified itself as FS-UAE
5.0.7 with a core derived from WinUAE 6.0.1. The resulting binary had SHA-256
`75bda31519a91242dc3fcae4a1b7e14d8e53b1bdff589f1ae852b97ec27e4d2b`.

An A500 Plus control run reached the required 68000 cycle-exact, Kickstart
2.04, and suite 1.0.1 ADF configuration. Its native buffer settled at 756 by
576 pixels and 49.920410 Hz. The development frontend did not emit a requested
raw screenshot in two bounded attempts. No pixel observation from this
revision is registered.

A later temporary frontend hook reached a complete 752 by 572 emulator buffer
at frontend frame 400, then faulted before writing a file because the
screenshot subsystem had not been initialised. That run also reported no disk
in drive zero, independently invalidating it as a corpus control. The modified
binary produced no evidence and is excluded.

## WinUAE source

The audited development revision contains separate ECS and AGA external
blanking paths in `drawing.cpp`. Both test `BPLCON0.ECSENA` and
`BPLCON3.EXTBLKEN`. The AGA path derives its programmable edges from HBSTRT and
HBSTOP, including the fine position bits. The ECS path selects fixed Denise
and CSYNC-derived blanking; it does not use the AGA programmable comparator.

This source behaviour identifies a likely disagreement with Copperline,
especially for ECS. It is not a captured observation and does not resolve a
case. A registered capture from this UAE generation remains required.

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

The next useful result is a raw current-generation UAE-family capture with
auditable geometry. A software capture from another independent family or a
registered physical-hardware capture would provide stronger evidence. Until
then, no case with a producer disagreement should be promoted.

## Related files

- [Capture evidence boundary](README.md)
- [Copperline capture package](copperline-0.13.0-eec5806/README.md)
- [Capture schema](../schema/capture-v1.schema.json)
- [Conformance process](../../../../../knowledge/processes/amiga-programmable-hblank-conformance.md)
