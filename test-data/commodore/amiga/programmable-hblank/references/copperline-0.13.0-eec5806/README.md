# Copperline 0.13.0 capture package

This package answers what Copperline 0.13.0 at revision
`eec5806778dab8b60f3b05fa7ab2428e4e18b073` produced for the
CCK-aligned cases in programmable-HBLANK suite 1.0.1.

The observations are software-derived evidence from the independent
Copperline implementation family. They are not hardware measurements,
specification authority, or Emu198x expected output.

## Producer boundary

The capture binary was built from
<https://github.com/CopperlineHQ/Copperline> with:

```sh
cargo build --locked --release --bin copperline
```

The resulting `aarch64-apple-darwin` binary has SHA-256
`ead4139d547085ad58a9794b17e57e6bf0649e4c6c7040e038f00550030a7fe9`.
It was built with Rust 1.95.0
(`59807616e1fa2540724bfbac14d7976d7e4a3860`) and Cargo 1.95.0 on
macOS 26.5.2 build 25F84.

The two configuration files in this directory define expansion-free A500 Plus
ECS PAL and A1200 AGA PAL machines. Firmware remains external to the reference
package.

`capture.sh` verifies the producer binary, copies the configuration, firmware,
suite manifest, ADF, and payload into an isolated directory, and writes a
capture-time manifest before launch. It rejects undeclared `COPPERLINE_*`
settings so inherited producer controls cannot change an unrecorded run. It
then cold-boots the machine, exposes the probe's ready record through
Copperline's headless debugger, and saves three adjacent raw frames.

`package.py` verifies every copied input, capture manifest, run log, ready
record, field identity, adjacent-field stability, pixel, and observed black
run before writing the tracked APNG and JSON record.
[`package-v1.json`](package-v1.json) binds the retained logs and manifests to
those outputs and records the capture and packaging toolchains.

## Capture geometry

Copperline's raw standard-PAL capture is a 716 by 570 RGBA8 bobbed
framebuffer. It retains Copperline's full internal horizontal overscan field
without frontend cropping, scaling, shaders, phosphor, tint, or horizontal
recentering. It does not represent the complete electrical raster outside
that internal field.

The source audit establishes the declared mapping without image alignment:

- framebuffer rows 200 and 201 represent line-doubled beam line 128;
- raw sample `x` represents colour clock `(x + 196) / 4`;
- HBSTRT or HBSTOP colour clock `h` maps to sample `4h - 196`;
- the captured interval is colour clocks 49 through 227.

All promoted comparisons must stay inside that interval. In particular, the
central case's two edges and the wrap case's two visible edges are retained.

## Interpretation limits

Copperline 0.13.0 applies programmable horizontal blanking after rendering.
Its audited path is gated by `BEAMCON0.BLANKEN`, but not by
`BPLCON0.ECSENA` or `BPLCON3.EXTBLKEN`. The ECSENA and EXTBLKEN case results
therefore document Copperline behaviour and expose disagreements; they do
not resolve the hardware gates.

The same revision masks HBSTRT and HBSTOP to coarse nine-bit colour-clock
positions and does not implement Lisa's resolution-dependent fine bits. The
three AGA fine-position cases are excluded from this package.

Equal HBSTRT and HBSTOP values produce no programmable interval in this
revision. A wrapped interval produces two black runs at the left and right
capture boundaries.

## Contents

- [`ecs.toml`](ecs.toml) and [`aga.toml`](aga.toml) are the exact canonical
  machine configurations.
- [`capture.sh`](capture.sh) is the bounded headless capture procedure.
- [`capture_manifest.py`](capture_manifest.py) snapshots and identifies every
  capture input.
- [`package.py`](package.py) verifies and packages the raw capture output.
- [`package-v1.json`](package-v1.json) binds the package inputs, outputs, and
  packaging toolchain.
- [`captures/README.md`](captures/README.md) describes the multi-field APNG
  files.
- [`records/README.md`](records/README.md) describes the machine-readable
  evidence records.
- [`logs/README.md`](logs/README.md) describes the retained producer logs.
- [`manifests/README.md`](manifests/README.md) describes the capture-time
  manifests.

## Related files

- [Corpus overview](../../README.md)
- [Capture schema](../../schema/capture-v1.schema.json)
- [Comparator capabilities](../comparator-capabilities.md)
- [Conformance process](../../../../../../knowledge/processes/amiga-programmable-hblank-conformance.md)
