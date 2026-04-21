# Denise OCS — port-gap analysis (2026-04-21)

Phase 1 gap list following the archive-port methodology that landed
CIA-8520, Paula 8364, and Agnus/Blitter cleanly.

## What Denise is

Denise (8362) is the Amiga's **display engine**. Agnus streams bitplane
and sprite words across the chip bus; Denise latches them, shifts them
out pixel-by-pixel through the bitplane serialisers, looks each pixel
up in the 32-entry colour palette, overlays the 8 sprite channels
(with programmable priority vs. the two playfields), runs collision
detection, and emits one ARGB pixel per lores half-CCK (per hires
quarter-CCK).

Denise-proper concerns covered by this port:
1. Bitplane shift registers (8-wide in the archive, 6 for OCS).
2. BPLCON0 (HIRES, BPU, HOMOD, DBLPF, LACE, COLOR enable).
3. BPLCON1 barrel-shift fine scroll (odd/even plane groups).
4. BPLCON2 playfield/sprite priority nibbles (PF1P/PF2P/PF2PRI).
5. DDF/DIW gating (horizontal + vertical display window).
6. COLOR00–COLOR31 palette (12-bit RGB, 4k values).
7. HAM (hold-and-modify) — 6-bit index, 2-bit control.
8. EHB (extra half-brite) — 6 planes without HAM.
9. Dual-playfield composer (PF1 odd planes, PF2 even planes).
10. Sprite register layer (POS/CTL/DATA/DATB ×8) + arming rules.
11. Sprite horizontal/vertical comparator (HSTART/VSTART/VSTOP).
12. Attached-pair sprites (4-bit colour) via CTL bit 7.
13. Sprite priority between sprite pairs and each playfield.
14. Collision detection (CLXCON match masks, CLXDAT read-and-clear).
15. LACE interlace (long/short frame toggle, field Y interleave).

**Out of scope** (AGA features present in the archive but OCS-irrelevant):
- `palette_24` 256-entry 24-bit colour table.
- `bpl_fifo` wider FMODE fetches.
- BPU bit 4 (8-bitplane mode).
- HAM8 (`ham_prev_rgb24`).
- Superhires (SHRES) source-per-output quadrupling.
- BPLCON4 sprite/bitplane XOR masks (OSPRM/ESPRM/BPLAM).
- `spr_width` 32/64 (OCS is fixed 16).

Keep these fields out of the ported struct entirely — they add state
we'd never exercise on OCS and drift without tests.

## Current-tree coverage

| Area | Current state |
| --- | --- |
| Per-CCK bitplane fetch (LORES) | ✅ `Denise::tick` in machine |
| Lores serialiser (6 planes) | ✅ MSB-first, plane-indexed colour |
| Shift-register reload at slot 7 | ✅ |
| BPL1/2MOD end-of-line apply | ✅ in machine |
| DDF window gate | ✅ via `ddf_window` |
| DIW vertical window gate | ✅ via `diw_vertical_window` |
| COLOR00-31 palette storage | ✅ `chipset.color[32]` |
| BPLCON1 (scroll) | ❌ stored but ignored |
| BPLCON2 (priority) | ❌ stored but ignored |
| BPLCON0 bits beyond BPU | ❌ HIRES/HOMOD/DBLPF/LACE/COLOR all ignored |
| HAM mode | ❌ absent |
| EHB mode | ❌ masked off (`index & 0x1F`) |
| Dual playfield composer | ❌ absent |
| HIRES serialiser | ❌ absent |
| Sprite registers (POS/CTL/DATA/DATB) | ❌ not stored in Denise |
| Sprite rendering | ❌ absent |
| Sprite DMA fetch from Agnus | ❌ no SPRxPT in Agnus either (gap #162) |
| Attached-pair sprite colour | ❌ absent |
| Sprite priority vs playfields | ❌ absent |
| Collision detection CLXCON/CLXDAT | ❌ absent |
| LACE interlace | ❌ non-interlaced only |
| Framebuffer size | 768×576 line-doubled, fixed PAL |
| Viewport extraction | ❌ direct framebuffer only |

## Archive coverage (`crates/commodore-denise-ocs-archive/`)

| Area | Archive state |
| --- | --- |
| `DeniseOcs::new()` / `new_with_raster_height()` | ✅ PAL default (624 rows), NTSC via arg |
| Raster framebuffer at superhires width (1816 px) | ✅ double-height for interlace |
| 32-entry 12-bit palette + `set_palette` | ✅ |
| `rgb12_to_argb32` / `rgb24_to_argb32` | ✅ |
| `load_bitplane` + `bpl_data`/`bpl_shift` ×8 | ✅ (use first 6 on OCS) |
| BPLCON0 HIRES bit | ✅ `bplcon0 & 0x8000` |
| BPLCON0 BPU (num_bitplanes) | ✅ with OCS 6-plane clamp |
| BPLCON0 HOMOD (HAM enable) | ✅ inside `resolve_color_rgb12` |
| BPLCON0 DBLPF (dual playfield) | ✅ in `compose_playfield_pixel` |
| BPLCON1 odd/even scroll nibbles | ✅ barrel-shift across prev/current |
| BPLCON1 low-bit-ignored in hires | ✅ |
| BPLCON2 PF1P/PF2P/PF2PRI | ✅ `resolve_sprite_priority` |
| EHB half-brite | ✅ in `resolve_color_rgb12` |
| HAM modify-R/G/B + palette-select | ✅ |
| Dual playfield PF1/PF2 composer | ✅ `compose_playfield_pixel` |
| Sprite POS/CTL/DATA/DATB storage | ✅ `write_sprite_*` |
| Sprite arming (DATA arms, CTL disarms, DATB neutral) | ✅ |
| Sprite HSTART/VSTART/VSTOP decode | ✅ `sprite_hstart`/`vstart`/`vstop` |
| Sprite comparator with 1-pixel pipeline shadow | ✅ `spr_pos_display` + `spr_pos_dirty` |
| Sprite per-pixel shift runtime | ✅ `step_sprite_runtime_one_pixel` |
| Sprite fast-forward resync when beam jumps | ✅ `sync_sprite_runtime_to_beam` |
| Attached-pair sprite colour (4-bit from pair) | ✅ CTL bit 7 = ATTACH |
| Sprite priority nibbles vs PF1/PF2 | ✅ `resolve_sprite_priority` |
| CLXCON match-enable + match-value | ✅ `clxcon_bitplane_match` |
| CLXDAT read-clear | ✅ `read_clxdat` |
| Sprite-group collisions (SP01/SP23/etc.) | ✅ `latch_collisions` |
| LACE interlace (LOF toggle + field rows) | ✅ `interlace_active` + `lof` |
| `begin_beam_line()` resets HAM prev + scroll pending | ✅ |
| `ViewportPreset` Standard/Overscan/Full | ✅ PAL + NTSC bounds |
| `extract_viewport` + `scale_nearest` + `to_display` | ✅ |
| `pixel_aspect_ratio` | ✅ 16:15 PAL / 8:9 NTSC |
| Archive tests | ✅ extensive BPLCON1/sprite/DPF/HAM tests |

## HRM cross-check

**BPLCON1 barrel-shift** (archive `trigger_shift_load`) matches HRM
3-9 "Smooth Horizontal Scrolling": nibble = pixel count, odd planes
(1/3/5) use PF1 nibble (bits 3-0), even planes (2/4/6) use PF2 nibble
(bits 7-4). Hires ignores the low bit of each nibble (2-pixel increments).

**BPLCON2 priority** (archive `resolve_sprite_priority`) matches HRM
Fig. 3-21: PF1P (bits 0-2) vs sprite groups, PF2P (bits 3-5) vs sprite
groups, PF2PRI (bit 6) for playfield ordering. The archive clamps
priorities to 4 which matches the 4 sprite-pair groups.

**HAM control bits** (archive `resolve_color_rgb12`, bits 5-4 of the
6-bit index) — 00 = palette, 01 = modify B, 10 = modify R, 11 = modify
G. Matches HRM 2-3 "Hold-and-Modify Mode".

**EHB** — index bit 5 halves each RGB nibble of palette[index & 0x1F].
Matches HRM 3-10.

**Sprite arming rules** — writing SPRxCTL disarms, writing SPRxDATA
arms, writing SPRxDATB alone is neutral (HRM Fig. 4-13). Archive
implements this correctly.

**Collision bit layout** — CLXDAT bits per HRM Table 3-10:
bit 0 = BP-odd/BP-even, bits 1-4 = sprite-group ↔ BP-odd,
bits 5-8 = sprite-group ↔ BP-even, bits 9-14 = sprite-pair crosses.
Archive `latch_collisions` emits all of these.

## Known divergences / simplifications

1. **Current machine collapses colour index to 5 bits**
   (`index & 0x1F`) before palette lookup, so EHB's bit 5 is
   silently discarded. Port restores the 6-bit path through
   `resolve_color_rgb12`.

2. **Current machine has no sprite state at all** — sprite DMA in
   Agnus (task #162) needs SPRxPT pointers to exist first, then the
   fetched words flow into Denise via `write_sprite_*`.

3. **Current machine's framebuffer is 768×576 fixed PAL** — the
   archive's 1816×624 raster buffer is needed if we want hires +
   proper interlace. Port switches to raster-width + viewport
   extraction.

4. **No LACE long/short frame toggle** — Agnus has the `lof` field
   already (ported in #139), but Denise never consumes it. Port wires
   interlace rendering to Agnus's lof bit.

5. **AGA scaffolding removed** — the ported struct drops `palette_24`,
   `bpl_fifo`, `max_bitplanes`, `spr_width`, HAM8, BPLCON4, and
   superhires handling. If AGA support lands later, it's additive.

## Architectural observation — Denise register ownership

Post-port, Denise owns all of the following:

| Register | Current storage | Post-port |
| --- | --- | --- |
| BPLCON0 | Agnus (for BPU) | Agnus (with Denise read-accessor) |
| BPLCON1 | chipset | Denise |
| BPLCON2 | chipset | Denise |
| COLOR00-31 | chipset | Denise |
| CLXCON | — | Denise |
| CLXDAT | — | Denise (read-clear) |
| SPR0POS..SPR7POS | — | Denise (via `write_sprite_pos`) |
| SPR0CTL..SPR7CTL | — | Denise |
| SPR0DATA..SPR7DATA | — | Denise |
| SPR0DATB..SPR7DATB | — | Denise |
| BPL1DAT..BPL6DAT | — | Denise (Agnus DMA writes trigger shift-load) |

After the port, `chipset.rs` is empty — its remaining fields all move
to Denise, and the module can be deleted. BPLCON0 stays in Agnus
because Agnus consumes BPU for its DMA scheduler; Denise reads it
through an accessor for HIRES/HOMOD/DBLPF/LACE.

## Per-phase plan

### Phase 1 — characterisation tests (#151, #152, #153)

- **#151 BPLCON0/1/2 + pixel pipeline:** LORES vs HIRES source-pixels-
  per-CCK, BPLCON1 barrel-shift odd/even, low-bit-ignored in hires,
  prev-word carry, BPLCON2 priority nibbles, PF2PRI bit.
- **#152 HAM + EHB + DPF:** 6-bit index decode per mode, HAM prev-RGB
  reset at line start, EHB half-brite, dual-playfield PF1/PF2 visible-
  index mapping, PF2PRI switch.
- **#153 Sprites + collisions:** POS/CTL/DATA/DATB storage + arming,
  HSTART comparator, VSTART/VSTOP gating, attached-pair 4-bit colour,
  sprite-vs-playfield priority, sprite-vs-sprite priority by number,
  CLXCON match-enable/match-value, CLXDAT read-clear, BP↔BP and
  BP↔sprite-group and sprite-pair-cross collision bits.

Phase 1 tests run against the archive first. Each one must pass before
the corresponding Phase 2 port step lands.

### Phase 2 — port (#154, #155, #156, #157, #158, #159, #160, #161, #162, #163, #164)

- **#154 BPLCON0/1/2:** move BPLCON1/2 off chipset into Denise; add
  BPLCON0 accessor on Agnus that Denise consumes.
- **#155 DDF + DIW:** move the gates currently in `Denise::tick` into
  Denise's own state; Agnus still owns DDFSTRT/STOP + DIWSTRT/STOP
  registers but Denise reads them (they're Agnus-owned per HRM).
- **#156 Colour palette:** move `color[32]` off chipset into Denise;
  expose `write_color(idx, val)` and `read_color(idx)`.
- **#157 LORES serialiser:** adopt the archive's `shift_one_playfield_
  source_pixel` with 6-plane OCS clamp. Drop the current shift-load-at-
  slot-7 hack in favour of `trigger_shift_load` on BPL1DAT.
- **#158 HIRES serialiser:** add `source_pixels_per_output_call = 2`
  path and the 4-CCK-per-word shift rate.
- **#159 HAM + EHB + DPF:** port `resolve_color_rgb12` + `compose_
  playfield_pixel` + `ham_prev_rgb` reset in `begin_beam_line`.
- **#160 LACE:** consume Agnus's `lof` in the raster framebuffer
  Y-address; expand framebuffer to double-height.
- **#161 Sprite registers:** `write_sprite_pos/ctl/data/datb` with the
  arming rules, `spr_pos_display` pipeline shadow.
- **#162 Sprite DMA:** Agnus adds SPR0PT..SPR7PT pointer registers and
  a sprite-slot scheduler in the existing DMA slot table; fetched
  words write back into Denise via the `write_sprite_*` helpers.
- **#163 Attached pairs + priority:** port the `sprite_pixel` + `resolve_
  sprite_priority` functions.
- **#164 Collision detection:** port CLXCON storage + `clxcon_bitplane_
  match` + `latch_collisions` + CLXDAT read-clear.

### Phase 3 — integrate + retire (#165)

Rename `commodore-denise-ocs-archive` → `commodore-denise-ocs`, swap
the machine's inline `denise.rs` for the crate, update the dispatch
table in `machine-commodore-amiga-ocs/src/lib.rs` to route BPLCON1/2,
COLOR, CLXCON, SPR*, BPL*DAT through the new crate.

Kickstart boot must still reach the insert-disk screen with the default
palette/BPLCON programming. Then the regression set from tasks #151–
#153 must pass against the machine.

## Conclusion

Denise is the largest remaining port (~2340 lines archive → ~700-900
lines after AGA trim). The structural win is closing the last
`chipset.rs` gap: after this, every custom register has a named chip
owner, matching the silicon.

Blast radius is controlled because:
- Sprite DMA is gated behind its own Agnus extension (#162), which
  means the first five Phase 2 tasks (registers + scroll + HIRES +
  HAM/EHB/DPF) land without touching the Agnus side at all.
- The existing `Denise::tick` in the machine already does the per-CCK
  rhythm correctly; the port replaces its body, not the contract.
- Kickstart 1.3 boot uses BPU=1 or BPU=2 with no sprites + no HAM;
  the existing minimal path already handles that, so we won't lose
  the boot-screen smoke test during intermediate commits.
