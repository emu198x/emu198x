# Agnus OCS — port-gap analysis (2026-04-20)

Phase 1 gap list following the archive-port methodology that landed
CIA-8520 and Paula 8364 cleanly.

## What Agnus is

Agnus (8361 NTSC / 8367 PAL for OCS, plus 8371 ECS variants) is the
Amiga's **beam-position generator + DMA arbiter**, plus two co-located
sub-units: the **Copper** (branch-capable state machine that writes to
chipset registers on beam timing) and the **Blitter** (bitmap
hardware). On the die they share the silicon but they're logically
independent — the archive treats them as `struct Agnus` (beam + DMA +
blitter) plus a sibling `struct Copper`.

Agnus-proper concerns covered by this port:
1. Beam counter (vpos/hpos, VBL wrap, LOF long-frame toggle).
2. DMACON storage (shared with Paula via DMA bits; Agnus owns it).
3. DMA slot arbitration (the 227-CCK table that assigns each slot to
   refresh / disk / audio / sprite / bitplane / copper / CPU).
4. Bitplane fetch sequencer (LOWRES/HIRES `ddfseq` plane mapping).
5. Copper fetch scheduling (even-numbered slots in bitplane window).
6. Blitter bus-priority gating (BLTPRI "nasty mode" steals CPU slots).
7. DSKPT + sprite pointers + bitplane pointers (chip-bus DMA sources).
8. COPCON (CDANG — copper danger mode, allows writes to <$80).
9. Various register storage (DDFSTRT/STOP, DIWSTRT/STOP, bplmods,
   BPLCON0 bitplane-count bits).

**The Blitter runtime** is a separate port — tasks #134–#147. This
document focuses on Agnus; the blitter fields/methods stay in the
Agnus struct during the port (they're tightly coupled on die) but
aren't wired into the machine until the blitter phase.

## Current-tree coverage

| Area | Current state |
| --- | --- |
| Beam counter (`machine/src/agnus.rs`) | ✅ vpos/hpos, 227×312 PAL wrap, VBL pulse, vertb level, VPOSR/VHPOSR reads |
| VBL interrupt wiring | ✅ via paula.raise(IntSource::Vertb) |
| DMACON storage | ✅ set/clear semantics in chipset |
| DMACON → slot arbitration | ❌ inline per-feature hacks (audio slots at hpos 0x0E/10/12/14; copper yields via `denise::dma_claim`) |
| DMA slot table (refresh/disk/audio/sprite/bitplane/copper/cpu) | ❌ fragmented across machine, copper, denise modules |
| Copper DMA slot yield | ✅ `denise::dma_claim` — partial, doesn't know sprites/refresh |
| Bitplane fetch sequencer (`ddfseq`) | ❌ not present; only basic DDF window check |
| BPLCON0 bitplane count | ✅ `chipset.num_bitplanes()` helper |
| Bitplane pointers BPL1PT..BPL6PT storage | ✅ `chipset.bpl_pt[6]` |
| DDFSTRT/DDFSTOP storage | ✅ in chipset |
| DIWSTRT/DIWSTOP storage | ✅ in chipset |
| BPL1MOD/BPL2MOD storage | ✅ in chipset |
| DSKPT storage | ✅ in chipset (should be Agnus per HRM) |
| Sprite pointers SPRxPT | ❌ not stored |
| COPCON / CDANG | ✅ in copper module directly |
| LOF interlace toggle | ❌ not modelled |
| Agnus ID in VPOSR | ❌ always returns 0 |
| Blitter fields (BLT*PT, BLTCON, BLTSIZE, minterm, area/line runtimes) | ❌ not present |

## Archive coverage (`crates/commodore-agnus-ocs-archive/`)

| Area | Archive state |
| --- | --- |
| `Agnus::new()` / `new_with_region_lines()` | ✅ PAL default, NTSC via arg |
| `tick_cck()` — beam advance + VBL wrap + LOF | ✅ Interlace-aware |
| `current_slot() → SlotOwner` | ✅ Full 227-CCK table per HRM |
| `cck_bus_plan() → CckBusPlan` | ✅ Machine-facing summary of grants |
| LOWRES_DDF_TO_PLANE / HIRES_DDF_TO_PLANE tables | ✅ From Minimig Verilog |
| `dma_enabled(bit)` helper | ✅ Checks DMACON master+channel |
| `num_bitplanes()` from BPLCON0 | ✅ OCS 6-plane limit applied |
| `bpl_fetch_width` / `spr_fetch_width` | ✅ FMODE-aware (AGA hook) |
| DSKPT / sprite pointer writes | ✅ Split-word with high-latch for sprite Y |
| BLTCON0/1/size + blitter word/line/area runtime | ✅ Full implementation |
| Blitter DMA request queue | ✅ ReadA/B/C + WriteD, Internal |
| `blitter_nasty_active()` / BLTPRI | ✅ |
| `execute_incremental_blitter_op` | ✅ Per-slot progress |
| Blitter area/line Bresenham | ✅ |
| Copper (separate struct, ~262 lines) | ✅ WAIT/MOVE/SKIP state machine |
| Copper danger mode gate | ✅ |
| 18 in-archive tests | ✅ all passing |

## HRM cross-check

**Slot arbitration** (archive lines 1045–1139) matches HRM Table 6-1:

```
hpos 0x00       free
hpos 0x01-0x03  memory refresh
hpos 0x04-0x06  disk DMA (3 slots)
hpos 0x07       audio 0
hpos 0x08       audio 1
hpos 0x09       audio 2
hpos 0x0A       audio 3
hpos 0x0B-0x1A  sprite DMA (8 sprites × 2 slots each)
hpos 0x1B       refresh
hpos 0x1C+      bitplane / copper / CPU (inside DDFSTRT..DDFSTOP window)
```

Archive's copper-slot rule — "copper takes even slots inside the
bitplane window if DMACON.COPEN is set" — matches HRM p.29 (copper
fetches on odd CCKs, but the archive's `hpos.is_multiple_of(2)`
reflects a specific encoding where slot 0 is the first "odd" slot;
worth a characterisation test to lock it down).

DDF fetchunit behaviour: the archive implements the WinUAE "ddf window
completes the block containing DDFSTOP plus one more full block"
semantics, which matches HRM Appendix C1. Without this, the last
~8 CCKs of each fetch window would drop pixels.

## Known divergences / simplifications

1. **NTSC long/short line alternation** not modelled — the archive
   comments out the NTSC 227/228 alternation (`NTSC_CCKS_PER_LINE`
   is a const alias for the short-line length). PAL is always 227.
   Non-issue for the OCS PAL boot we test.

2. **No LOF toggle in current machine** — our inline `agnus.rs` has
   no long-frame interlace support. The archive has it. Port picks
   this up.

3. **Blitter line mode** uses Bresenham with some texture/masking
   features; not yet exercised by our machine. Stays behind the
   blitter Phase 2 gate.

4. **AGA FMODE register** is present in archive (`fmode` field) but
   only used by `bpl_fetch_width` / `spr_fetch_width`. OCS always
   uses 16-bit fetches so `fmode == 0` gives the right answer. Keep
   the field but don't expose it via the custom-register bus on OCS.

5. **Agnus ID (VPOSR bits 14:8)** — archive has a configurable
   `agnus_id` default $00 (OCS NTSC). PAL OCS is $10, which is what
   boot code expects. Port needs to set it correctly at construction.

## Architectural observation — register ownership map

Current chipset.rs has Agnus-owned registers alongside Denise-owned:

| Register | Real owner | Current storage | Post-port |
| --- | --- | --- | --- |
| DMACON | Agnus | chipset | Agnus |
| DSKPT  | Agnus | chipset | Agnus |
| BPL1..6PT | Agnus | chipset | Agnus |
| DDFSTRT/STOP | Agnus | chipset | Agnus |
| DIWSTRT/STOP | Agnus | chipset | Agnus |
| BPL1MOD/BPL2MOD | Agnus | chipset | Agnus |
| SPRxPT | Agnus | — (missing) | Agnus |
| BPLCON0 | Agnus + Denise | chipset | Agnus (Denise reads via accessor) |
| BPLCON1/2 | Denise | chipset | chipset (→ Denise later) |
| COLOR00-1F | Denise | chipset | chipset (→ Denise later) |

After the port, `chipset.rs` is left with only Denise concerns
(BPLCON1/2, COLOR) plus any remaining stubs awaiting the Denise port.

## Per-phase plan

### Phase 1 — characterisation tests (#132, #133)

- **#132 beam + frame timing:** PAL 227×312 CCK count, VBL fires once
  per frame, vpos/hpos wrap, LOF interlace toggle, VPOSR/VHPOSR bit
  layout.
- **#133 DMA arbitration:** the full 227-slot table (all 8 slot
  categories), DMACON master-enable gate, per-channel gate, bitplane
  fetchunit semantics in HIRES + LOWRES, copper-slot parity rule,
  blitter nasty-mode.

Every Phase 1 test must pass against the archive before Phase 2
starts (same bar as CIA and Paula).

### Phase 2 — port (#139, #140, #141, #148)

- **#139 beam counter + VBL:** adopt archive's `tick_cck` (LOF
  support), VPOSR with correct agnus_id.
- **#140 DMACON + slot arbitration:** move DMACON to Agnus; expose
  `current_slot()` / `cck_bus_plan()`; wire machine's audio-slot
  helper through Agnus; fold the inline copper-yield check into
  the same path.
- **#141 COPCON + CDANG:** the copper module already owns the CDANG
  gate; this task exposes COPCON's single control bit through the
  custom-register bus ($02E) and has Copper read Agnus state.
- **#148 bitplane DMA:** port the ddfseq plane-select table + the
  fetch-window "ddf_stopping" state so bitplane pointers advance
  correctly during display.

### Phase 3 — integrate + retire (#149)

Rename `commodore-agnus-ocs-archive` → `commodore-agnus-ocs` and
update path references. Kickstart boot must still reach the insert-
disk state.

## Blitter deferral

Blitter-related fields and methods stay in the Agnus struct during
this port (they're tightly fused on die). They remain unwired from
the machine until tasks #134-#147 land them with proper
characterisation. The existing archive blitter implementation is
valid reference code but not yet exercised.

## Conclusion

Agnus is a moderate-sized port (~1451 lines + 262 for copper).
Biggest conceptual win is the DMA slot table — once in place, the
machine's scattered DMA-adjacent hacks (audio slots, copper yield,
disk slots) unify behind one arbiter. Blast radius is small because
the current-tree's `chipset.rs` holds most of the register storage
we'll migrate, and the existing `agnus.rs` beam counter is already
the archive's model.
