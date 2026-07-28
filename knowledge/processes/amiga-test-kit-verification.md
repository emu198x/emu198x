# Verifying Amiga execution with Amiga Test Kit

This process answers how the deterministic Amiga Test Kit v1.12 gate is run
and how its result may be interpreted.

The gate is explicit and ignored during the ordinary workspace test run. It
uses external firmware and media, takes substantially longer than a unit test,
and is intended to answer a system-level accuracy question rather than a
parser or boot-smoke question.

## Required inputs

The gate requires:

- the registered Amiga Test Kit v1.12 ADF, supplied directly or inside a ZIP
  through `EMU198X_AMIGA_TEST_KIT_ADF`;
- Kickstart 1.3 revision 34.005, supplied through
  `EMU198X_AMIGA_KICKSTART_13_ROM` or resolved from
  `EMU198X_AMIGA_ROM_DIR`;
- a release build of the `runtime-commodore-amiga` integration test.

[`test-data/amiga-test-kit-v1.12.sha256`](../../test-data/amiga-test-kit-v1.12.sha256)
pins the normalised ADF and ROM bytes. The wrapper extracts an archive to a
temporary directory, gives both inputs their normalised names, and verifies
that manifest before invoking Rust.

The external path is authoritative. The gate fails when a required variable is
unset, a file cannot be read, a ZIP does not resolve to exactly one ADF, the
ADF has the wrong size, or either checksum differs.

## Invocation

Run the complete lane from the repository root:

```sh
EMU198X_AMIGA_TEST_KIT_ADF=/path/to/amiga-test-kit-v1.12.adf \
EMU198X_AMIGA_KICKSTART_13_ROM=/path/to/kick13.rom \
scripts/verify-amiga-test-kit.sh
```

The ADF variable may instead name a ZIP containing the registered image.
`EMU198X_AMIGA_KICKSTART_13_ROM` may be omitted when `kick13.rom` is available
through the normal Amiga ROM-directory resolution.

The wrapper runs the ignored integration test in release mode with one test
thread. Directly invoking the ignored test remains strict: its external paths
do not acquire ordinary skip-if-missing behaviour.

## Guest-state oracle

The gate uses values calculated by Test Kit inside the guest as its semantic
oracle. It does not infer the detected machine from Emu198x configuration
alone.

The registered v1.12 image places the relevant state at these chip-RAM
addresses:

| Guest value | Address | Encoding |
|---|---:|---|
| vertical-blank rate | `$013094` | `u8` |
| timing frequency | `$013096` | big-endian `u32` |
| PAL flag | `$01309A` | `u8` |
| chipset type | `$01309B` | `u8` |
| processor name | `$01309C` | 31-byte C string |
| processor model number | `$0130BB` | `u8` |

These addresses are fixture-specific, not a Test Kit API. The exact ADF
checksum is therefore a prerequisite for interpreting them.

For a PAL OCS machine Test Kit must report:

- 50 Hz vertical blank;
- timing frequency 7,093,790 Hz;
- PAL flag `1`;
- chipset type `0`, meaning OCS.

The timing frequency is the Amiga motherboard timing used by Test Kit's CIA
measurements. It is not the accelerator processor's input clock.

The stock A500 profile must produce processor name `68000` and model `0`. The
GVP A530 profile must produce `68030` and model `3`. The latter result exercises
Test Kit's control-register probe, including its writable `CACR.FD`
discrimination between a 68020 and a 68030.

## Video and input assertions

The gate waits 600 PAL frames for the stable main menu. It then checks
source-defined display state and a coarse central-screen visibility invariant:

- Test Kit's three-bitplane `BPLCON0` value is active;
- the central display is not predominantly black;
- enough bright foreground pixels exist to establish that text was drawn;
- the settled frame remains unchanged across a further frame interval.

It then presses F1 through the emulated keyboard and requires a changed,
stable, visible memory page. On the A530 profile the machine-side assertions
also require a 40 MHz MC68EC030, one MiB of coherent accelerator-local RAM, and
a configured Zorro-II mapping.

These checks prove that the guest reached and rendered the expected class of
screen. They do not prove pixel accuracy and they do not machine-read the
memory-total text. In particular, an Emu198x-generated framebuffer hash must
not be recorded as an independent reference.

## Snapshot and replay

The A530 run takes a snapshot while the memory page is waiting. A fresh runtime
restores that snapshot and must satisfy both checks:

1. an immediate snapshot is byte-identical to the original snapshot;
2. the original and restored machines produce identical snapshots and
   framebuffers after the same forward tick budget.

This covers the selected CPU, its clock-domain phase, the mapped A530 state,
local RAM, chipset state, and mounted media under a real guest workload.

The gate does not compare post-restore host-filtered audio. The analogue output
filter history is not part of the machine snapshot, so such a comparison would
claim a stronger replay boundary than the current state format provides.

## Pixel-reference extension

Pixel-exact comparison is a separate extension. It requires a frame captured
from an independently configured reference emulator and a provenance record
containing:

- reference emulator and version;
- complete machine, CPU, RAM, chipset, region, and firmware configuration;
- Test Kit payload checksum;
- capture timing and viewport dimensions;
- reference PNG checksum.

The Test Kit test must not provide a golden-update mode. A current Emu198x
frame may be retained as diagnostic output, but it cannot become the expected
image merely because the test produced it.

## Result interpretation

A passing lane establishes:

- exact registered firmware and media were consumed;
- Test Kit reached its menu on both selected profiles;
- the guest identified the expected region, chipset, and processor;
- keyboard input reached the guest;
- the A530 local-memory function was configured;
- the selected real-media state replayed deterministically.

It does not establish pixel-exact video, analogue audio identity, every Test
Kit submenu, SCSI operation, cache behaviour, or complete compatibility with
software outside the exercised path.

## Related documents

- [Amiga Test Kit v1.12 fixture identity](../../test-data/amiga-test-kit-v1.12.md)
- [Amiga Test Kit v1.21 video conformance](amiga-test-kit-video-conformance.md)
- [Test ROM bundling policy](../decisions/test-rom-policy.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [Amiga machine catalogue](../decisions/amiga-machine-catalogue.md)
- [Motorola 68030 cache control](../decisions/motorola-68030-cache-control.md)
