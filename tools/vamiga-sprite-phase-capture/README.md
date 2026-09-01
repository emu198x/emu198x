# vAmiga Sprite-Phase Capture Adapter

This directory answers how the portable sprite horizontal-phase corpus is
captured from the registered vAmiga implementation family.

The adapter is an evidence producer, not an Emu198x oracle. It links a small
command-line program against an exact vAmiga `VACore` checkout, cold-boots the
corpus ADF, validates the complete `SPHX` ready record and its source buffers,
and copies three adjacent stable textures through `VideoPortAPI`.

## Producer Boundary

The registered producer is vAmiga 4.4b12 at revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`. The wrapper requires an exact,
clean source checkout. Neither the compiled producer nor commercial Kickstart
firmware is redistributed.

The capture uses vAmiga's VSYNC-driven execution path with warp and run-ahead
disabled. One wake-up must advance one field. The machine is suspended before
the ready record, configuration, or stable texture is inspected.

## Configuration

The adapter starts with `A500_OCS_1MB` and reapplies every capture-relevant
setting. The resulting profile has:

- PAL OCS timing and a Motorola 68000;
- 512 KiB chip RAM and 512 KiB slow RAM;
- warp disabled, run-ahead disabled, VSYNC stepping, and 100 percent speed;
- Denise frame skipping and layer hiding disabled; and
- the direct RGB monitor palette, so `GpuColor` values are not colour-adjusted.

The complete vAmiga configuration is exported with the capture.

## Capture

`capture.sh` accepts the vAmiga source root, built corpus `dist` directory,
external Kickstart 1.3 image, a fresh output root, and an operator:

```sh
./capture.sh \
  /path/to/vAmiga \
  /path/to/sprite-horizontal-phase/dist \
  /path/to/kick13.rom \
  /tmp/vamiga-sprite-phase \
  "Operator name"
```

By default, the release build is temporary. Repeated local runs may set
`EMU198X_VAMIGA_SPRITE_BUILD_DIR` to a dedicated CMake build directory:

```sh
EMU198X_VAMIGA_SPRITE_BUILD_DIR=/tmp/vamiga-sprite-build \
  ./capture.sh ...
```

The source capture is three concatenated 912 by 313 textures. Every pixel is
the exact vAmiga `GpuColor` `u32`, serialized little-endian in row-major and
field order. No crop, scale, filter, shader, alignment search, or colour
conversion is applied.

Measurements use beam line 132. The leading hardwired-HBLANK interval is
identified by its RGB value while ignoring the alpha-channel resolution bit
that vAmiga stores in the first HBLANK sample. The marker and sprite intervals
are exact matches for the programmed RGB4 colours. All intervals use
start-inclusive, stop-exclusive source-sample coordinates.

## Output

The output directory contains:

- the staged CC0 suite manifest, ADF, and payload;
- `capture.u32le`, containing all three raw textures;
- the exported vAmiga configuration;
- the adapter result and producer log;
- producer-build and before/after input provenance; and
- a schema-shaped `capture-record.json` that passes the corpus semantic and
  bound-file validator.

Promotion into the corpus `references/` directory is a separate evidence
review step.

## Scope

This adapter provides software-derived A500 OCS evidence. It does not establish
physical-hardware truth and cannot provide ECS, AGA, analogue-video, display,
or capture-card behaviour.

## Related Files

- [Sprite horizontal-phase corpus](../../test-data/commodore/amiga/sprite-horizontal-phase/README.md)
- [Capture schema](../../test-data/commodore/amiga/sprite-horizontal-phase/schema/capture-v1.schema.json)
- [vAmiga Paula capture adapter](../vamiga-paula-audio-capture/README.md)
