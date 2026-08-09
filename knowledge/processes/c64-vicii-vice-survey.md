# Running the C64 VIC-II reference survey

This process answers how the selected PAL 6569 VIC-II testbench cases are
measured at one identifiable Emu198x revision.

The survey supplies the before-and-after evidence for the
[C64 accuracy closure campaign](../decisions/c64-accuracy-closure-campaign.md).
It is a diagnostic measurement, not a general VIC-II pass rate or a
physical-hardware-conformance claim.

## Inputs

The runner requires two explicit directories:

- `EMU198X_C64_VICII_TESTBENCH_DIR` contains the externally supplied VIC-II
  testbench tree; and
- `EMU198X_C64_ROM_DIR` contains `kernal.rom`, `basic.rom` and `chargen.rom`.

The tracked
[`assets-v1.json`](../../test-data/commodore/c64/vicii-vice-survey/assets-v1.json)
manifest identifies 17 selected PRGs across 13 categories, 17 reference PNGs
and three firmware
images by logical path, byte count and SHA-256. The wrapper refuses missing,
additional, reordered or byte-different selected inputs. It does not require
or claim an identity for files elsewhere in the large staged testbench.

The exact upstream revision of the staged testbench remains unresolved. The
manifest pins the consumed bytes without inventing that provenance. The
reference images also have uneven evidence histories: they are not uniformly
direct hardware captures or output from one independent emulator family.

## Command

Run from the repository root:

```sh
EMU198X_C64_VICII_TESTBENCH_DIR=/path/to/vicii-testbench \
EMU198X_C64_ROM_DIR=/path/to/c64-roms \
scripts/verify-c64-vicii-survey.py
```

The wrapper requires a clean worktree by default and resolves the complete
40-character Git revision before executing the survey. `--allow-dirty` exists
for implementation diagnosis. A dirty report records `dirty: true`, occupies
a separate `-dirty` result directory and cannot be treated as closure evidence.

The underlying integration test boots the PAL breadbin profile for 150 frames,
loads each selected PRG directly into RAM, updates BASIC's end-of-program
pointer, types `RUN`, and settles for 60 frames. Direct PRG injection avoids
making disk or tape behaviour part of this video question; the program still
runs through the normal CPU, memory and VIC-II paths.

## Result

The wrapper writes one atomic report below:

```text
target/accuracy/c64-vicii-survey/<full-revision>/report.json
```

Diagnostic dirty runs use `<full-revision>-dirty` instead. The report contains
no host paths. It records:

- the source revision and dirty state;
- the tracked fixture-manifest identity and all 37 verified logical assets;
- the PAL breadbin runtime and 6569 model contract;
- the 416 x 312 framebuffer and fixed `(16, 16)` crop into each 384 x 272
  reference;
- the exact 16-entry ARGB palette used for classification;
- raw and decoded semantic hashes for the reference input;
- the indexed output-plane hash for Emu198x; and
- exact matched and total pixel counts for every case.

The Rust producer writes integer counts and indexed-plane hashes to a private
temporary JSON file. The wrapper validates its schema, revision, case IDs,
order, paths, dimensions and total count before admitting the measurements to
the report. Console percentages are display-only and are never parsed as
evidence. This also prevents a missing-fixture skip from becoming a complete
report.

## Comparison boundary

Each RGB value is mapped to the nearest of the current 16 C64 palette entries
using squared Euclidean RGB distance. Emu198x and the reference therefore
compare as digital colour indices even when their chosen RGB palettes differ.
The survey says nothing about PAL encoding, composite artifacts, luminance,
chroma, display calibration or other analogue output properties.

The 17 rows are representative measurements. The colour-fetch-bug category
uses all five supplied programs; each other category uses one selected
program. A result such as 92.456 percent
means that fraction of the selected image's 104,448 pixels has the same
classified colour index. It does not mean that the category, chip or emulator
is 92.456 percent accurate. A case becomes a strict conformance assertion only
through a separately reviewed threshold or exact reference contract.

## Interpreting changes

Compare reports only when the manifest identity, runtime contract and
comparison contract agree. An implementation change may be accepted when it
improves its targeted case and preserves stronger gates, but an unexplained
change in another indexed-plane hash remains a regression or a new
disagreement to classify.

Vendored VICE 3.10 may explain an implementation technique. Hoxs64,
VirtualC64 and MiSTer may provide additional independent evidence. Structural
similarity does not make any implementation the specification.

## Related Documents

- [C64 accuracy closure campaign](../decisions/c64-accuracy-closure-campaign.md)
- [PAL 6569 late-badline display phase](../decisions/c64-late-badline-display-phase.md)
- [C64 architecture review](../decisions/c64-architecture-review.md)
- [MOS 6569 / 6567 VIC-II](../chips/mos-vic-ii.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [Survey fixture manifest](../../test-data/commodore/c64/vicii-vice-survey/assets-v1.json)
- [Survey fixture notes](../../test-data/commodore/c64/vicii-vice-survey/README.md)
