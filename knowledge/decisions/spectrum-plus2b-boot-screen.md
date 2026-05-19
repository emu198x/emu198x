# Decision: +2B boot screen displays "+2A" — and that's accurate

**Status:** Resolved 2026-05-07 during the golden-screenshot work for SOLID criterion 11.

**Drift trigger:** if you find yourself wondering why the +2A and +2B goldens
look identical, looking for a "model byte" hack that makes the +2B menu say
"+2B", or thinking *"something must be wrong with our +2B emulation"* —
**stop and re-read this entry first.** The identical rendering is correct.

## The finding

A real Amstrad +2B running stock Amstrad firmware displays **"128 +2A"** in
its boot menu header — not "+2B". The "+2B" label is on the case, not on the
screen. We discovered this while building golden-screenshot tests and were
briefly convinced our emulation was wrong; it isn't.

## Evidence

1. **The ROMs themselves.** Both v4.0 and v4.1 of the +3 family ROMs
   (the v4.1 set is what shipped on +2B and later +3 hardware) contain only
   the literal string `"128 +3 "` (offset `0x0805` in v4.0 ROM 0, `0x080a` in
   v4.1). Neither stock ROM contains "+2A" or "+2B" anywhere. The displayed
   model header is patched at runtime by the boot code: when the FDC fails
   to respond, the boot ROM overlays "+3" with "+2A" — there is no third
   branch for "+2B".

2. **ZEsarUX.** The most accurate dot-matrix-faithful Spectrum emulator has
   `MACHINE_ID_SPECTRUM_P2A_40` and `MACHINE_ID_SPECTRUM_P2A_41` (+2A with
   v4.0 / v4.1 ROMs) but no `_P2B_` entry. Internally a "+2B" is just
   "+2A board running v4.1". Same memory and port handlers for both.

3. **Fuse.** The Fuse machines directory has `specplus2.c` (grey +2),
   `specplus2a.c` (+2A), and `specplus3.c` (+3). No `specplus2b.c`.

4. **Modified-firmware exception.** Andrew Owen's "+2B ROM Set (2012)" in
   `Emu198x-Unclean/Reference/.../[ROM]/+2B ROM Set - 0..b (2012)(Owen, Andrew S.)(+2B).zip`
   *does* bake `"128 +2B "` into ROM 0 at offset `0x2755`. This is a
   community modification and not stock Amstrad firmware. Emulators or
   retro setups that show "+2B" on the boot screen are running Owen's
   modified ROMs (or equivalents), not Amstrad ROMs.

## Decision

The +2B uses **stock Amstrad v4.1 ROMs** at
`~/.emu198x/roms/amstrad-zx-spectrum-plus2b/plus3-{0..3}.rom` (split from the
canonical `ZX Spectrum 128 +3 v4.1 (1987)(Amstrad)(+2A-+3)` 64 KiB image).
The +2A continues to use v4.0 at `~/.emu198x/roms/amstrad-zx-spectrum-plus3/`
shared with the +3.

The +2A and +2B golden screenshots are **byte-identical at boot** because
that's what the real hardware does. The variant types remain distinct
(`Plus2AMarker` / `Plus2BMarker`) so snapshots cannot cross variants and
future hardware-level differences (if any are ever introduced) land cleanly,
but no model-byte differentiation is needed for boot-screen accuracy.

## What we explicitly chose not to do

- **No model-byte hack.** We did not introduce a phantom hardware byte that
  the ROM reads to display "+2B". Stock ROMs don't read such a byte; faking
  one would diverge from real-hardware behaviour without solving any user
  problem.
- **No Andrew Owen ROM swap.** Switching the +2B ROM directory to Owen's
  modified set would make the boot screen say "+2B" but at the cost of
  running community-modified firmware as the default — a separate policy
  decision about distribution and attribution. If a curriculum need ever
  justifies the switch, it's a one-line change in
  `crates/runtime-sinclair-zx-spectrum/tests/goldens.rs` (and an updated
  golden), but it should be a deliberate choice, not a drift fix.

## Pointer

Golden tests live at
`crates/runtime-sinclair-zx-spectrum/tests/goldens.rs`. Goldens are checked
in at `crates/runtime-sinclair-zx-spectrum/tests/goldens/`. Update workflow
is `UPDATE_GOLDENS=1 cargo test -p runtime-sinclair-zx-spectrum --test goldens
-- --include-ignored`.
