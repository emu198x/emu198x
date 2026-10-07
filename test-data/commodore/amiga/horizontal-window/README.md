# Horizontal display-window diagnostics

These project-authored CC0 guests reuse the sprite-horizontal-phase bootloader
and SPHX ready record. They exercise DIWHIGH coarse/fine positions and Copper
window-register rewrites on A1200 PAL. They are software-reference diagnostics,
not an admitted physical-hardware conformance corpus.

Build with the existing assembler and Python 3.13+:

```sh
python3.13 test-data/commodore/amiga/horizontal-window/tools/build.py /tmp/amiga-window
```

The seventeen cases use one all-one superhires plane with sprite DMA disabled.
Every active line resets BPL1PT and the base window at an early Copper WAIT.
The rewrite cases change one register on lines 128..143 at successive even
WAIT positions; the actual bus schedule determines register delivery.
The identity record distinguishes each guest. Native captures require the
SPHX signature, matching identity and at least nine completed guest fields.

Capture the registered exploratory full-resolution FS-UAE producer using the
[existing procedure](../../../../tools/fs-uae-sprite-phase-capture/FULL-RESOLUTION.md).
The same Kickstart and ADF must be used by both producers. Require three
adjacent reference fields, 9/10/11, including complete framebuffer metadata.
Run the unchanged whole-raster comparator:

```sh
python3.13 test-data/commodore/amiga/wide-sprite-dma/tools/compare.py /tmp/amiga-window --native before.png --output /tmp/window-comparison.json
```

The initial native version-52 producer fails 33 of 51 fields. Legacy,
explicit-equivalent and zero-offset visible-stop controls pass. The original
`stop-fine-*` cases also pass because their bitplane data ends before HSTOP;
they are masking controls, not proof of fine-stop correctness. The added
`visible-stop-*` cases place HSTOP inside live data and demonstrate errors of
one, two and three samples on every active stored row.

The read-only `runtime-commodore-amiga` example `horizontal_window_trace`
records native DIW mirrors, gate state and the next output-counter position
after each machine tick on lines 134..137. It requires a settled SPHX guest.
It is diagnostic observation, not an independent expected-value generator.

See [primary observations](../../../../../../reference/by-system/commodore-amiga/2026-horizontal-window-observations.md)
for register sources and evidence boundaries. `validation.json` records the
baseline and corrected capture counts and producer identities. The raw captures, original
guest sources, native traces and logs are retained under
`/private/tmp/emu198x-horizontal-window/`.

The version-53 candidate matches all 51 AGA fields exactly. The saved stages
include local window registers, pending delivery and the four-sample Lisa
window history. Version 52 saves are rejected. The reference producer and
whole-raster comparison were unchanged.

`--resolution lores` and `--resolution hires` generate additional controls.
Seven lores and four hires A500+ ECS guests were captured against the same
UAE-family producer. Their 33 fields still disagree: ordinary edges are one
lores position early (four samples in the reference transport), including the
legacy controls. The existing ECS phase was retained. These are recorded
failures, not ECS conformance passes. For this comparison each native 70 ns
sample is repeated twice at 35 ns; the beam-origin crop stays unchanged.


The final native executable was rebuilt after removing redundant scalar gate
storage. Its timed-start rewrite and maximum fine-start/fine-stop guests were
recaptured at frame 210, with the same SPHX readiness checks. All nine adjacent
reference-field comparisons remain exact. `final-comparison.json` and the
source/binary hashes in `validation.json` identify that final build separately
from the original 51-field corrected candidate.

Both strict Test Kit gates pass their six patterns, and all eight golden-matrix
tests pass without image updates. The broad regression sweep's ten failures
were isolated fixtures that only programmed Agnus; corrected fixtures preserve
their expected pixels and pass all affected targets. The validation record
retains the initial failures, rerun counts and 121 ignored tests explicitly.
The older 128-guest graphics corpus was not rerun in this correction.

The [counter-origin follow-up](../ecs-output-phase/README.md) resolves the
apparent ECS displacement as a reference-buffer origin mismatch. It also
exposes an AGA window/data delay hidden by the original mapping. Preserve the
raw comparison results here as observations; they no longer establish absolute
AGA chip phase. The follow-up retains the measured origins and failing AGA
controls separately.

The approved Lisa phase correction is complete. All seventeen guests were
recaptured with the final executable and match all 51 counter-domain fields.
See [the correction record](../ecs-output-phase/lisa-correction/README.md) for
independent controls, unchanged reference images and final validation.
