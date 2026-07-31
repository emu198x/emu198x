# FS-UAE Amiga Test Kit video capture adapter

This directory answers how the A1200 AGA Amiga Test Kit v1.21 video reference
is captured from the current UAE implementation family through FS-UAE 5.0.7.

The adapter is an evidence producer, not an Emu198x oracle. It applies one
capture-only patch to FS-UAE, cold-boots the external Test Kit ADF, injects a
fixed field-counted menu sequence, and copies three adjacent completed chipset
framebuffers. The resulting observations remain software-derived.

## Producer boundary

The patch applies to FS-UAE revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2`. That source identifies itself as
FS-UAE 5.0.7 with a core derived from WinUAE 6.0.1.

The patch changes only `od-fs/video-fs.cpp`. Its environment-gated hook:

- sends the registered Test Kit function-key sequence at fixed core fields;
- requires a complete UAE chipset framebuffer;
- copies three adjacent 756 by 576 BGRA8888 frames before FS-UAE's
  compatibility crop, scaling, filtering, shaders, overlays, or GPU
  presentation;
- records the raw geometry, core-field labels, and frontend compatibility
  view; and
- does not write guest memory or change chipset registers.

The hook is inert unless both `FSEMU_CODEX_TESTKIT_CAPTURE_DIR` and
`FSEMU_CODEX_TESTKIT_CASE` are present.

## Building the producer

From an exact, clean checkout of the recorded revision:

```sh
patch -p1 < fs-uae-5.0.7-test-kit-video-capture.patch
install -m 0644 Portable.ini /path/to/fs-uae/Portable.ini
mkdir build
cd build
../configure --prefix=/tmp/fs-uae-test-kit-capture
ln -s ../../od-fs/python od-fs/python
ln -s ../../od-fs/resources od-fs/resources
make -j4
```

The two symlinks make FS-UAE's development Python and resource trees visible
to an out-of-tree build. They are build-layout inputs, not source changes.

The registered macOS arm64 binary was built with Apple clang 21.0.0 and GNU
Make 3.81. Its SHA-256 is
`5c3d9e35d100445a5603c5f86a19cc431a7363828053d4ede7d260c2c5d6899f`.
The binary is not redistributed.

## Capturing the reference cases

`capture.sh` accepts one case identifier or `all`, the exact patched binary,
the external Test Kit v1.21 ADF, matching external A1200 Kickstart 3.1 image,
a fresh output root, and the operator:

```sh
./capture.sh \
  all \
  /path/to/fs-uae \
  /path/to/AmigaTestKit.adf \
  /path/to/kick31-a1200.rom \
  /tmp/a1200-test-kit-captures \
  "Operator name"
```

The wrapper verifies the producer, ADF, and firmware hashes before launch. It
never copies firmware into a reference package. Each case starts from a fresh
A1200 boot with the ADF staged read-only.

The generated configuration selects PAL AGA, a cycle-exact 68EC020, 2 MiB of
chip RAM, and no expansion RAM. Host output is HIRES with doubled
non-interlaced lines and overscan enabled. Host filesystems, networking,
audio output, RTG memory, and input devices are disabled.

## Field schedule

Each run boots for 600 PAL fields. A key remains pressed for three fields, one
field follows its release, and 50 more fields separate navigation keys. The
final screen settles for 150 fields for `gradients` or 100 fields for every
other case. The hook then captures three adjacent complete fields.

Static cases must produce three byte-identical raw fields. The alternating
checkerboard must produce an A-B-A relationship; the phase labels do not
prescribe which image appears first.

The hook flushes all producer streams before writing its completion record.
The runner watches that record and then terminates the frontend, retaining and
validating the resulting wait status. It does not rely on wall-clock timing
or the frontend's automatic frame-limit shutdown.

## Output

Each run directory contains:

- the exact generated `.uae` configuration;
- a staged read-only Test Kit ADF;
- three tightly packed `.bgra` fields and descriptive JSON;
- the producer log through the flushed capture-completion record;
- before, after, and raw-capture hash lists; and
- `capture-manifest.json`, binding the result to the producer, patch, tools,
  configuration, external inputs, keyboard events, and field labels.

The registered firmware-free result is the
[A1200 AGA PAL reference](../../test-data/amiga-test-kit-v1.21/a1200-aga-pal/README.md).
The ADF, Kickstart image, producer binary, and temporary raw run are not part
of that reference package.

## Interpretation limits

This adapter supplies an A1200 AGA observation from one UAE-family producer.
It does not establish physical-hardware output, analogue-video behaviour, an
independent WinUAE vote, or accuracy outside the registered configuration and
patterns.

## Licensing

This directory is covered by Emu198x's GPL-2.0-or-later licence. The patch is
a modification to GPL-covered FS-UAE source. FS-UAE copyright remains with
its respective authors. Amiga Test Kit and Kickstart are external inputs and
are not redistributed here.

## Related files

- [Amiga Test Kit reference collection](../../test-data/amiga-test-kit-v1.21/README.md)
- [A1200 AGA PAL reference](../../test-data/amiga-test-kit-v1.21/a1200-aga-pal/README.md)
- [Fixture identity](../../test-data/amiga-test-kit-v1.21.md)
- [Video-conformance process](../../knowledge/processes/amiga-test-kit-video-conformance.md)
