# Independent mid-line playfield scroll diagnostics

These neutral AGA guests compare Copper-driven BPLCON1 changes against the registered FS-UAE reference. They are software diagnostics, not physical hardware calibration or an admitted conformance corpus.

Build with `python3 tools/build.py /private/tmp/emu198x-midline-scroll/verified-colours` from this directory. The existing `m68k-elf-as` and `m68k-elf-ld` tools are required. The builder reuses the validated sprite-horizontal-phase bootloader and ready-record format; forked diagnostic geometry is recorded separately.

Eighteen guests cover lores, hires and superhires, each with FMODE 0, 1 and 3, with a static control and a changing-offset counterpart. BPL1 and BPL2 have independent asymmetric 80-word buffers. PF1 is green; PF2 uses orange palette entry 9 (AGA PF2OF=3). Sprites are disabled. The Copper resets both pointers and BPLCON1 before every visible line. Lines 128..159 use WAIT positions $60..$9E and alternate odd-only ($0035→$003B), even-only ($0035→$00C5), and simultaneous ($0035→$00CB) changes. Controls perform no-op writes at the same phases.

Each build records source, payload and ADF SHA-256 values, every programmed offset and WAIT, and a unique ready identity. Three adjacent reference fields (9, 10, 11) are compared with the native full raster using `../wide-sprite-dma/tools/compare.py`. Its fixed producer origins retain the entire 1512×574 common raster; it performs no fitted alignment or interior masking, and returns failure on any changed RGB pixel.

`--scroll-sweep` builds a separate 18-guest static-offset sweep. Lines 128..191 cover extended integer fields and every resolution-supported fractional position, with distinct odd/even integer offsets. `--ddf-start 0x30` selects the second tested fetch origin; the default is $38. These static controls distinguish reference behavior from an assumed translation of the zero-offset stream. At DDF $30, some high scroll phases copy the preceding word phase; DDF $38 is the aligned input used by translation invariants.

All 54 guests rebuild byte-identically. The final campaign retains 162 new full-raster field comparisons plus 222 preceding display comparisons, all exact. Eight hundred and eight Rust tests, eight boot checks and both explicit Test Kit video gates pass; the broad suite retains 31 explicitly ignored fixture/campaign tests. The failing-before-fix group-copy regression is `commodore-denise-aga/tests/midline_scroll.rs`; board-level independence and restoration regressions are in `runtime-commodore-amiga/tests/scroll_dma.rs` and `display_register_pipeline.rs`.

Evidence: `/private/tmp/emu198x-midline-scroll/final-verification.json`. The original references are unchanged; the logging-only producer's three fields match its original fields byte-for-byte, and its unlogged source and executable are restored. Diagnostics remain software-reference evidence.

Limits: mid-line changes use DDF $38, two active planes, ordinary integer offsets, PF1 priority and the stated even WAITs. The static extension covers both DDF origins and independent integer offsets with shared fractional offsets. FMODE 2, arbitrary fractional pair combinations, other plane counts, OCS/ECS mid-line changes, simultaneous resolution/FMODE/scroll changes, priority/HAM combinations and silicon calibration remain outside this sweep.
