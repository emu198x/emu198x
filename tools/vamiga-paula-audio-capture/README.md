# vAmiga Paula-audio capture adapter

This directory answers how the portable Paula-audio corpus is captured from
the registered vAmiga implementation family.

The adapter is an evidence producer, not an Emu198x oracle. It links a small
command-line program against an exact vAmiga `VACore` checkout, cold-boots one
corpus ADF, validates the complete `PAUD` ready record, and extracts three
adjacent fields from vAmiga's modelled A500 audio output.

## Producer boundary

The registered producer is vAmiga 4.4b12 at revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`. The capture wrapper requires an
exact, clean source checkout. Neither the compiled producer nor commercial
Kickstart firmware is redistributed.

vAmiga's existing headless regression mode cannot be used for this purpose:
it enables warp, and vAmiga mutes audio while warped. This adapter instead
enables VSYNC-driven execution and wakes the emulator one field at a time.
That path runs without host pacing while keeping audio synthesis active.

## Configuration

The adapter queues the `A500_OCS_1MB` scheme followed by every
capture-relevant override. It waits for vAmiga's configuration message and
reads every option back before loading firmware or media.

The resulting machine has:

- PAL OCS timing and a Motorola 68000;
- 512 KiB chip RAM and 512 KiB slow RAM;
- warp disabled, run-ahead disabled, VSYNC stepping, and 100 percent speed;
- a 48 kHz host stream with linear interpolation and adaptive sample rate
  disabled;
- the A500 modelled filter pipeline;
- hard stereo panning: channels 1 and 2 left, channels 0 and 3 right;
- all channel gains at 100 and both output gains at 50; and
- a write-protected corpus ADF in DF0.

The complete vAmiga configuration is exported with each capture.

## Capture

`capture.sh` accepts one case identifier or `all`, the vAmiga source root, the
built corpus `dist` directory, the external Kickstart image, a fresh output
root, and an operator:

```sh
./capture.sh \
  all \
  /path/to/vAmiga \
  /path/to/paula-audio/dist \
  /path/to/kick13.rom \
  /tmp/vamiga-paula-captures \
  "Operator name"
```

The wrapper verifies vAmiga and firmware identities, builds one release
adapter, and starts a fresh emulator for each case. Each field step must
advance exactly one vAmiga frame. The ready record and audio buffer are read
under the same suspension.

By default, the release build is temporary. Repeated local runs may set
`EMU198X_VAMIGA_PAULA_BUILD_DIR` to a dedicated CMake build directory. The
wrapper still configures and builds the target before capture, while unchanged
vAmiga objects are reused:

```sh
EMU198X_VAMIGA_PAULA_BUILD_DIR=/tmp/vamiga-paula-build \
  ./capture.sh all ...
```

The source capture is stereo, 48 kHz IEEE-754 binary32 WAVE. It preserves the
exact finite samples returned by `AudioPortAPI::copyInterleaved`; no
normalisation, gain adjustment, remapping, clipping conversion, or dither is
applied.

## Output

Each case directory contains:

- the staged CC0 suite manifest, ADF, and payload;
- the exported vAmiga configuration;
- the unmodified source WAVE;
- the adapter's field and buffer report;
- the producer log;
- before and after input hashes; and
- a `capture-v1` evidence record containing semantic measurements and
  provenance.

The output is a reviewable capture run. Promotion into
`test-data/commodore/amiga/paula-audio/references/` is a separate evidence
review step.

## Scope

This adapter provides software-derived A500 evidence. It does not establish a
physical motherboard's analogue transfer function, component tolerances,
noise, crosstalk, clipping, or DC offset. It does not provide A1200/AGA visual
evidence.

## Related files

- [Portable Paula-audio corpus](../../test-data/commodore/amiga/paula-audio/README.md)
- [Capture schema](../../test-data/commodore/amiga/paula-audio/schema/capture-v1.schema.json)
- [Paula-audio conformance process](../../knowledge/processes/amiga-paula-audio-conformance.md)
- [Paula stereo-routing decision](../../knowledge/decisions/amiga-paula-stereo-routing.md)
