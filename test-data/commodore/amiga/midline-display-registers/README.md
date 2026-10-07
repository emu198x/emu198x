# Mid-line Lisa display-register diagnostics

These CC0 exploratory guests locate errors in Copper-driven resolution,
fetch-mode and palette-index changes. They reuse the existing SPHX guest,
bootloader and ready record. They are not an admitted hardware conformance
corpus. They do not change the emulator or its reference pixels.

Build with Python 3.13+ and the existing `m68k-elf-as` / `m68k-elf-ld` tools:

```sh
python3 test-data/commodore/amiga/midline-display-registers/tools/build.py /tmp/amiga-midline-constant
python3 test-data/commodore/amiga/midline-display-registers/tools/build.py /tmp/amiga-midline-varying --pattern varying
python3 test-data/commodore/amiga/midline-display-registers/tools/build.py /tmp/amiga-midline-reset --pattern varying --reset-each-line
```

Each dataset contains eight cases: a no-op BPLCON0 control, lores/hires in
both directions, hires/superhires in both directions, FMODE 0→1 and 1→3,
and BPLCON4 playfield XOR. Sprite DMA is disabled. The bitplane is eight-byte
aligned and contains 256 rows of 80 words, sufficient for the fastest mode.
The constant pattern is A5A5; the varying pattern gives each row word a
distinct value, `(word_index * 0x1F3D) XOR 0xA5A5`, truncated to sixteen bits.
The latter catches errors that repeated words hide.

On beam lines 128..143, Copper waits at CCK $20, writes the original register
value, then waits at $80 and writes the changed value. The original value is
restored at line 144. The reset variant additionally restores BPL1PT to the
same data before every active line 44..243. It performs the early register
restore after those pointer writes, before DDFSTRT=$38. Its tested $80 wait
is unchanged. The linked pointer words use the canonical bootloader's actual
load address, not a guessed source offset.

Boot/capture on A1200 PAL with the same Kickstart 3.1 ROM in both producers.
Save a native screenshot after 180 frames as each case's `after.png`. Use the
[documented full-resolution reference fork](../../../../tools/fs-uae-sprite-phase-capture/FULL-RESOLUTION.md),
the A1200 config template and SPHX hook to capture adjacent guest fields
9/10/11 under each case's `reference/capture/`. Record source revision,
applied patches, binaries, ROM, configurations and guest hashes.

Run the existing whole-raster gate unchanged:

```sh
python3 test-data/commodore/amiga/wide-sprite-dma/tools/compare.py /tmp/amiga-midline-varying --output /tmp/amiga-midline-varying/comparison.json
```

The corrected emulator passes all eight cases in each of the three datasets:
72 unchanged reference-field comparisons with zero differing RGB pixels.
The original failing images and reports remain preserved. Constant words alone
had hidden the wide-fetch error; the distinct-word and pointer-reset probes
verify both the transition and the following lines. Every comparison retains
all RGB pixels in the source-defined common raster, including intervening
35 ns samples, border and blanking. No alignment is fitted.

The [optional logging patch](../../../../tools/fs-uae-sprite-phase-capture/fs-uae-5.0.7-midline-register-trace.patch)
uses `FSEMU_CODEX_TIMING=1` to record Alice transfers and Lisa register/data
stages on line 132. It changes no guest or pixel data; nine traced fields were
byte-identical to the original producer. Apply it to the documented exploratory
reference fork, not to the registered read-only snapshot. Native transfer logs
come from `cargo run --release -p runtime-commodore-amiga --example midline_trace -- <kickstart> <probe.adf>`.

Current captures, original guest sources, deterministic rebuilds, reports,
hashes and measurements live at `/private/tmp/emu198x-midline-display/`.
The primary [observations](../../../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#mid-line-display-register-discovery--2026-10-05)
distinguish measured failures, source-supported causes and unresolved timing.

## Phase sweep

The optional sweep retains distinct data words and resets the bitplane pointer
on every active line. Lines 128..159 use successive even Copper WAIT positions
$60..$9E, covering a 64-CCK span; these are wait positions, not asserted
register-write times. Copper arbitration determines the actual delivery.

```sh
python3 test-data/commodore/amiga/midline-display-registers/tools/build.py /tmp/amiga-sweep-ddf30 --phase-sweep --pattern varying --reset-each-line --ddf-start 0x30
python3 test-data/commodore/amiga/midline-display-registers/tools/build.py /tmp/amiga-sweep-ddf38 --phase-sweep --pattern varying --reset-each-line --ddf-start 0x38
```

Each dataset has thirteen cases: three no-op resolution controls, all six
directions between lores/hires/superhires, and FMODE 0→1, 1→3, 3→1 and 1→0
in superhires. DDF starts $30/$38 distinguish different physical copy phases.
All original eight-case guests remain byte-identical without `--phase-sweep`.
Capture and compare each dataset through the same complete-raster gate above.

The 2026-10-05 sweep initially failed FMODE 0→1 on sixteen alternating
scanlines at each origin. The reference repeats the preceding narrow word
when the output tap widens; the native 16-bit shifter had discarded its upper
bits. Retaining the full 32-bit register corrects all 78 reference-field
comparisons. All 144 previous comparisons also remain exact. Original
failures, unchanged references and corrected outputs are retained under
`/private/tmp/emu198x-display-phase-sweep/`. This remains software-reference
diagnostic evidence and does not cover every legal register delivery.
