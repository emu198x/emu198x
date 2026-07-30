# FS-UAE programmable-HBLANK capture adapter

This adapter answers how the 198x programmable-HBLANK corpus was captured
from the current UAE implementation family through FS-UAE 5.0.7.

It is an evidence adapter, not an Emu198x test oracle. The adapter applies one
capture-only patch to FS-UAE, cold-boots a single corpus ADF, waits for the
probe's ready record, and copies three adjacent completed chipset
framebuffers. The resulting observations remain software-derived.

## Producer boundary

The patch applies to FS-UAE revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2`. That source identifies itself as
FS-UAE 5.0.7 with a core derived from WinUAE 6.0.1.

The patch changes only `od-fs/video-fs.cpp`. It adds an environment-gated hook
that:

- reads the corpus ready record from guest memory on the emulation thread;
- requires a complete UAE chipset framebuffer;
- checks adjacent core and guest field counters;
- copies the 756 by 576 BGRA8888 `video_memory` buffer before FS-UAE's
  compatibility crop or any frontend scaling, filtering, shader, overlay, or
  GPU presentation;
- writes raw pixels and descriptive JSON without writing guest memory or
  changing chipset state.

The hook is inert unless `FSEMU_CODEX_CAPTURE_DIR` and
`FSEMU_CODEX_CAPTURE_CASE_NUMBER` are present.

## Building the producer

From an exact checkout of the recorded revision:

```sh
patch -p1 < fs-uae-5.0.7-programmable-hblank-capture.patch
touch Portable.ini
mkdir build
cd build
../configure --prefix=/tmp/fs-uae-current-capture
make -j4
```

The registered macOS arm64 build used Apple clang 21.0.0, GNU Make 3.81, and
the configure prefix `/tmp/fs-uae-current-build.XqhtYq/_install`. Its binary
SHA-256 is
`81fdcc09bf36b6a275a9d39b27407e3484815b5713b411e16dbfe6024cf2899b`.
The binary is not redistributed.

`Portable.ini` should be placed at the FS-UAE source root before launch so the
development frontend uses an isolated portable data directory.

## Capturing one case

`capture.sh` accepts a profile, case, exact patched binary, suite `dist`
directory, matching external Kickstart image, fresh output root, and operator:

```sh
./capture.sh \
  aga \
  programmed-central \
  /path/to/fs-uae \
  /path/to/programmable-hblank/dist \
  /path/to/kick31-a1200.rom \
  /tmp/hblank-captures \
  "Operator name"
```

The `ecs` profile requires the recorded Kickstart 2.04 image. The `aga`
profile requires the recorded A1200 Kickstart 3.1 image. The adapter verifies
the expected hashes but never copies firmware into the reference package.

The generated UAE configuration explicitly selects:

- A500 Plus ECS with a cycle-exact 68000 and 1 MiB chip RAM, or A1200 AGA
  with a cycle-exact 68EC020 and 2 MiB chip RAM;
- PAL timing;
- host HIRES raster output, doubled non-interlaced lines, and overscan mode;
- no expansion RAM, RTG memory, host filesystem, networking, audio output, or
  input device;
- a read-only staged ADF.

The adapter observes the ready record at guest field counter 1, waits until
counter 9, and captures counters 9, 10, and 11. It records both the guest
counter and FS-UAE core field label. Input hashes are compared before and
after execution.

## Output

Each run directory contains:

- the exact generated `.uae` configuration;
- staged CC0 suite manifest, ADF, and payload;
- three tightly packed `.bgra` files and their per-field metadata;
- the complete producer log;
- before, after, and raw-capture hash lists;
- `capture-manifest.json`, which binds those files to the producer, tools,
  firmware hash, ready record, and field labels.

The registered, firmware-free package is under
`test-data/commodore/amiga/programmable-hblank/references/`.

## Licensing

This directory is covered by Emu198x's GPL-2.0-or-later licence. The patch is
a modification to GPL-covered FS-UAE source and is deliberately kept outside
the corpus's CC0-only subtree. FS-UAE copyright remains with its respective
authors.

## Related files

- [Programmable-HBLANK corpus](../../test-data/commodore/amiga/programmable-hblank/README.md)
- [Conformance process](../../knowledge/processes/amiga-programmable-hblank-conformance.md)
