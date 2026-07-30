# FS-UAE HBLANK Write-Timing Capture Adapter

This adapter answers how the programmable-HBLANK write-timing corpus is
captured from the registered FS-UAE 5.0.7 producer.

It is an evidence adapter, not an Emu198x oracle. It cold-boots one corpus ADF,
waits for its ready record, and copies three adjacent completed chipset
framebuffers. The result remains software-derived UAE-family evidence.

## Producer boundary

The adapter uses FS-UAE revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2` with the capture-only patch in
[`../fs-uae-hblank-capture/`](../fs-uae-hblank-capture/README.md). That source
identifies its chipset core as WinUAE 6.0.1-derived.

The registered macOS arm64 binary has SHA-256
`81fdcc09bf36b6a275a9d39b27407e3484815b5713b411e16dbfe6024cf2899b`.
The binary and commercial firmware are not redistributed.

The patch is inert without its capture environment. When enabled, it copies
the 756 by 576 BGRA8888 UAE chipset buffer before frontend crop, scaling,
filtering, shaders, overlays, or GPU presentation. It does not write guest
memory or change chipset state.

## Capturing one case

`capture.sh` accepts a profile, case, exact producer binary, built suite
directory, matching external Kickstart image, fresh output root, and operator:

```sh
./capture.sh \
  aga \
  midline-hbstrt-past \
  /path/to/fs-uae \
  /path/to/programmable-hblank-write-timing/dist \
  /path/to/kick31-a1200.rom \
  /tmp/hblank-write-captures \
  "Operator name"
```

The `ecs` profile requires the registered Kickstart 2.04 image. The `aga`
profile requires the registered A1200 Kickstart 3.1 image. The adapter
verifies expected hashes but never copies firmware into the reference
package.

Each run uses PAL timing, cycle-exact chipset and CPU settings, host-HIRES
overscan output, no frontend filter, no expansion devices, and a read-only
ADF. The adapter observes guest field counter 1, waits through counter 8, and
captures counters 9, 10, and 11. Inputs are hashed before and after execution.

## Output

Each run directory contains:

- the generated UAE configuration;
- staged suite manifest, ADF, and payload;
- three packed BGRA fields and their metadata;
- the complete producer log;
- input and raw-capture hash manifests;
- `capture-manifest.json`, binding the run to producer, firmware, suite, and
  adapter identities.

## Related files

- [Write-timing corpus](../../test-data/commodore/amiga/programmable-hblank-write-timing/README.md)
- [Shared capture patch](../fs-uae-hblank-capture/README.md)
- [Write-timing verification process](../../knowledge/processes/amiga-programmable-hblank-write-timing.md)
