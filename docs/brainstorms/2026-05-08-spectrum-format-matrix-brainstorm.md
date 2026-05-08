---
date: 2026-05-08
topic: spectrum-format-matrix
---

# Spectrum cross-variant format-load test matrix

## What We're Building

A new integration-test suite at
`crates/runtime-sinclair-zx-spectrum/tests/format_matrix.rs` that
proves **every Spectrum format loads on every in-scope variant where
it's expected to**. Synthetic minimal fixtures are built inline; no
external files; runs in CI without `#[ignore]`. Lifts SOLID criterion
3 (Formats) from "we know it works on 48K, the variant crates exist"
toward "the matrix is green by construction".

## Why This Approach

**Why synthetic inline fixtures, not checked-in real ones** — the
criterion's bar is "format works across all variants", not "real game
loads". A 49 179-byte all-zeroed SNA exercises the parser →
`apply_snapshot` → variant runtime path identically to a real game
file; the test asserts the wiring not the gameplay. Avoids licence /
provenance concerns for any real ROMs / snapshots, keeps the test
hermetic, and skips the `#[ignore]`-on-CI shape the goldens harness
uses.

**Why a separate `format_matrix.rs`, not extending `goldens.rs` or
`variants.rs`** — `goldens.rs` is `#[ignore]`d (needs real ROMs),
`variants.rs` covers per-variant runtime behaviour without
format-loading concerns. The format matrix is a third axis (variants
× formats), best kept in its own file so failures point straight at
"format X doesn't load on variant Y".

**Why the matrix isn't pure cartesian** — formats target machine
classes:
- **TAP / TZX** — universal; tested on all 8 variants.
- **SNA** — split: 48K-format SNA loads on 48K-class (16K / 48K /
  Spectrum+), 128K-format SNA loads on 128K-class (128K / +2 / +2A /
  +2B / +3). Each variant gets the SNA flavour its model expects.
- **Z80 v1** — 48K-class (matches the .z80 v1 spec which is 48K-only).
- **Z80 v2/v3** — every variant (the spec covers 48K through +3 via
  the model byte).
- **DSK** — +3 only; the test asserts `load_media` succeeds. The
  actual disk-load path is pinned at
  `wiki/decisions/spectrum-plus3-disk-loading-incomplete.md` and
  the test does not assert on boot completion.

Cross-class loads (e.g., 48K SNA on 128K) are out of scope —
realistic users always pair format flavour to machine.

## Key Decisions

- **Fixture builders**: helper functions in the test module that
  return `Vec<u8>` for each format. Synthetic content is minimal
  but standards-compliant — the parsers must accept it without
  errors. Examples:
    - `build_minimal_sna_48k()` → 49 179 bytes, all-zeroed RAM, PC=0.
    - `build_minimal_sna_128k()` → 131 103 bytes (49 179 + 4 + 5×16384).
    - `build_minimal_z80_v1()` → ~30-byte header + 49 152 zeroed RAM.
    - `build_minimal_z80_v2()` → v2 header + page records.
    - `build_minimal_tap()` → one valid header block (Program type)
      + one minimal data block.
    - `build_minimal_tzx()` → TZX header + one standard-loader block.
    - `build_minimal_dsk()` → 256-byte DSK header + minimum tracks.
- **Boot-time assertion**: each test boots the variant from a
  zeroed firmware buffer (no real ROM needed for synthetic
  fixtures), applies the format, runs ~5 frames, and asserts:
  - For snapshots: `apply_snapshot` returned without panic; a
    handful of registers in the runtime match the snapshot's
    captured values (verifies the apply actually wired through).
  - For tapes: `load_media` returned `Ok`; `tape_is_loaded()`
    returns `true`.
  - For DSK on +3: `load_media` returned `Ok`. No further
    assertion (the BIOS-level loader hang is pinned).
- **Variant boot pattern**: each test uses
  `Spectrum48kRuntime::from_firmware(&firmware)` (and equivalents)
  with a synthetic 16 KB / 32 KB ROM buffer of zeros. The test
  doesn't need a real ROM since the assertion is about the format
  apply path, not about the program executing.
- **Test naming**: `<variant>_<format>_loads`, e.g.
  `spectrum_48k_sna_loads`, `spectrum_plus3_dsk_loads`. ~33 tests
  total: 8 variants × {tap, tzx, sna, z80} + 1 +3 dsk.

## Open / parked items (not in this commit)

- **Cross-class format compatibility** — e.g., loading a 128K SNA
  on a 48K runtime. Behaviour is "should error gracefully";
  worth a follow-up commit when the error-path matters. Out of
  scope here.
- **Real-game golden tests** — extending `goldens.rs` to load a
  known game on each variant and assert a frame matches a
  checked-in PNG. Would prove "loads *and runs*", not just "loads".
  Bigger commit; needs licence-clean fixtures.
- **+3 disk-load end-to-end fix** — the µPD765A ↔ +3 BIOS
  command path. Tracked at
  `wiki/decisions/spectrum-plus3-disk-loading-incomplete.md`.

## Next Steps

→ Implementation. Phase shape:
  1. New `tests/format_matrix.rs` with the seven fixture builders
     and a small assertion helper. Build the fixtures and verify
     each parser accepts them in isolation first.
  2. Variant × format matrix tests, one function per cell.
     Snapshot tests assert post-apply register values; tape tests
     assert `tape_is_loaded`; DSK asserts `load_media` ok.
  3. Verify the matrix is green; flip SOLID criterion 3 status
     from PARTIAL to mostly-DONE (the cross-variant gap closed;
     +3 disk-load remains pinned but is a separate FDC concern).
