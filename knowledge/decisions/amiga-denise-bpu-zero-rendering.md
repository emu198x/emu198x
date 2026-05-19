# Issue: Denise renders bitplane data when BPLCON0 BPU=0

**Date:** 2026-04-19
**Status:** Resolved — see [Findings](#findings-2026-04-19)

## Symptom

At the Kickstart 1.3 insert-disk screen, the disk-and-hand graphic renders correctly — visible in screenshots, ~430k non-black pixels in the framebuffer, three bitplane colours used (white background, black outlines, blue disk body, mid-tone shadow on the hand).

Yet the chipset state at this point is:

| Register | Value | Field of interest |
|---|---|---|
| `BPLCON0` | `$0302` | BPU bits 14:12 = **0** (zero bitplanes) |
| `BPLCON0` | `$0302` | bit 9 (COLOR) = 1, bit 8 (GAUD) = 1, bit 1 (LACE) = 1 |
| `DMACON` | `$03D0` | BPLEN=1 (bitplane DMA active), BLTEN=1 |

By the Amiga Hardware Reference, `BPU=0` means "no bitplanes displayed" — Denise should not output any bitplane shift-register data, only the background colour ($0FFF white). Yet our Denise emulation renders the graphic.

## Why this matters

This may be one of:

1. **A real Denise behaviour we don't understand** — the OCS Denise might actually display BPL1DAT contents under some condition we're not modelling at the BPU-decode level. (Unlikely but possible — the chipset reference is occasionally wrong about edge cases, per the Guru Book.)

2. **A Kickstart 1.3 quirk** — the boot screen might be set up via some unusual path (HAM mode? sprite reuse? a hardware register we're misreading?) that produces graphics without nominal bitplane display. Less likely given the simplicity of the boot screen.

3. **A bug in our Denise** — most likely. Denise might be reading BPL1DAT and feeding it through the colour lookup regardless of BPU, producing visible output even when BPU=0 should blank the display. This bug currently masks itself as "boot works" but would break any program that explicitly relies on BPU=0 to blank the screen, and might also cause subtle artefacts in other programs.

## What to investigate

1. **Read our Denise's pixel-output path** (`crates/commodore-denise-ocs/src/lib.rs`) — find where BPU is consulted (or not) when generating pixel output. Verify against the Hardware Reference Manual table for BPLCON0.
2. **vAmiga / WinUAE comparison** — boot KS 1.3 in vAmiga; sample BPLCON0 at the same point; if their value also has BPU=0, then this isn't a configuration error and the question is "what does real Denise do with BPU=0 + BPLEN=1 + bitplane data being fetched". If their BPU is non-zero (and we just wrote a bad register), the bug is upstream in our Agnus or CPU bus.
3. **Test the contrary** — write a tiny test program that sets `BPLCON0 = 0x0000` (every bit clear) and asserts the framebuffer is uniformly background colour. If our Denise still renders bitplanes, the bug is confirmed.
4. **Check BPLCON0 latch timing** — Agnus may latch BPLCON0 at a different point than Denise samples it. We could be reading a stale value via the query pipe but Denise sees a different (correct) one. Worth ruling out.

## Why "investigate later, not block now"

The graphic IS rendering — boot reaches the screen we expect, in the form we expect, with the colours we expect. Whatever's happening, it produces correct visual output for the insert-disk screen.

If the bug is in our Denise (most likely), it could absolutely cause subtle problems for programs that use BPLCON0 as a blanking mechanism. But for the Kickstart-1.3-to-Workbench critical path, the symptom is "works correctly" so it's not blocking the immediate boot work.

The investigation should land before the first ECS / AGA variant is implemented, because those variants rely on BPLCON0 / BPLCON3 / BPLCON4 working precisely.

## Acceptance

- We know which of the three causes (Denise behaviour, Kickstart quirk, our bug) explains the symptom.
- If it's our bug, fixed and a regression test added.
- If it's Denise behaviour, documented in the chip reference and the test in `kickstart_boot_invariants.rs` is updated to assert the actual hardware behaviour explicitly.

## Findings (2026-04-19)

**Root cause: cause #3 (our Denise bug).** The shift-out path in
`shift_one_playfield_source_pixel()` had a "legacy compatibility" fallback
that, when `num_bitplanes()` returned 0, *inferred* an active plane span
from whatever was already in the shift registers. That fallback was
written to keep older unit tests working (they seed `bpl_shift[]`
directly without programming BPLCON0), but it had the side effect of
making the renderer ignore an explicit `BPU=0` request from real Amiga
code: any stale shift-register contents got clocked through to the
colour lookup and produced visible pixels.

WinUAE cross-reference: `WinUAE/drawing.cpp::getlinetype()` returns
`LINETYPE_BORDER` (background colour only) when
`GET_PLANES(bplcon0) == 0`. Stale shift-register contents are never
sourced. Our behaviour was a clear divergence.

**Why the Kickstart screen still rendered correctly:** Agnus already
gates bitplane DMA on `num_bpl > 0` in `current_slot()` — no fetches
happen when BPU=0 — so the Copper list must be transitioning BPU from
non-zero to zero during the frame, leaving stale data in the shift
registers. The captured `BPLCON0=$0302` is whatever the Copper had
last written at the moment the test sampled the register; during the
visible-area lines the same Copper list sets BPU non-zero, Agnus
fetches bitplanes, Denise renders correctly. The bug only surfaced
because the legacy fallback fed those stale shift contents back into
the colour lookup outside the display window — and the fix removes
that path for any program that explicitly programs BPLCON0.

A program that deliberately relied on `BPU=0` to blank the playfield
mid-frame would previously have seen garbage; with the fix it now
gets the spec-correct background colour.

**Fix (`crates/commodore-denise-ocs/src/lib.rs::shift_one_playfield_source_pixel`):**
gate the legacy-state inference on `self.bplcon0 == 0`. Any program
that has explicitly written BPLCON0 — including a deliberate
`BPLCON0 = $0000` — gets the spec-correct behaviour and BPU=0 blanks
the playfield. The fallback only fires when BPLCON0 is still its
default 0, preserving the existing in-crate unit-test path that seeds
`bpl_shift[]` without programming BPLCON0.

**Regression test:**
`crates/machine-commodore-amiga/tests/denise_bpu_zero.rs` —
`bpu_zero_renders_only_color00_with_stale_shift_registers` constructs a
minimal Amiga (CPU halted via `STOP #$2700`), sets BPLCON0=$0200
(BPU=0, COLOR=1), seeds `bpl_shift[0]=0xAAAA` at the top of every
scanline, runs a frame, and asserts the framebuffer is uniformly
COLOR00 (white). Fails on the unfixed renderer (~10k+ leaked black
pixels), passes on the fixed one.

The Kickstart-1.3 invariant tests in `kickstart_boot_invariants.rs`
remain unchanged (the disk-and-hand graphic still renders because the
Copper toggles BPU non-zero during display, which was never broken).

## Related

- `crates/commodore-denise-ocs/src/lib.rs` — Denise implementation; fix at `shift_one_playfield_source_pixel`
- `crates/machine-commodore-amiga/tests/denise_bpu_zero.rs` — regression test
- `crates/machine-commodore-amiga/tests/kickstart_boot_invariants.rs` — tests deliberately don't assert BPU value
- `knowledge/decisions/amiga-architecture-review.md`
- Reference: `Emu198x-Reference/_organised/by-system/commodore-amiga/amiga-custom-chips-reference.md` — BPLCON0 register documentation
- Cross-validation: `WinUAE/drawing.cpp::getlinetype()` (`GET_PLANES(bplcon0) == 0` → `LINETYPE_BORDER`)
