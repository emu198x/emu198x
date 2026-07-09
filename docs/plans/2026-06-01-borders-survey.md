> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "Borders survey — current vs canonical per video chip"
type: survey
date: 2026-06-01
related: 2026-06-01-all-machines-operational-parity-plan.md
---

# Borders survey

Phase 1.1 of the all-machines operational parity plan. Per video
chip: current framebuffer dimensions, whether border is included,
canonical real-hardware dimensions, fix complexity.

## Categories

- **A. Border included** — current framebuffer already covers the
  full TV-visible area (active + border). No work needed.
- **B. Active-area only** — current framebuffer is just the active
  display region; canonical border needs adding.
- **C. Not applicable** — system has no concept of border (LCD,
  audio-only chip, generic chip with no fixed dimensions).

## Survey table

| Chip / module | Current FB | Includes border? | Canonical real-hw frame | Category | Fix complexity |
|---|---|---|---|---|---|
| **Sinclair Spectrum ULA** family (`ferranti-ula-6c001e`, `sinclair-ula-7k010e`, `pentagon-ula`, `scorpion-ula`, `timex-scld`, `amstrad-ula-40077`) | 352 × 296 (via `common-sinclair-zx-spectrum::timing::SCREEN_WIDTH/HEIGHT`) — 256 × 192 active + 48 × 52 border | **Yes** ✓ | 352 × 296 (matches) | A | none |
| **Sinclair ZX80 / ZX81 ULA** (`sinclair-zx81-ula`) | 320 × 240 — 256 × 192 active + 32 × 24 border (BORDER_LEFT/BORDER_TOP constants) | **Yes** ✓ | ~320 × 240 (matches close enough) | A | none |
| **Commodore Denise OCS / ECS / AGA** (`commodore-denise-{ocs,ecs,aga}`) | 1816 × 624 (PAL) / 1816 × 524 (NTSC) — full raster including overscan | **Yes** ✓ | matches (Amiga is overscan-aware end-to-end) | A | none |
| **MOS VIC-II** (`mos-vic-ii`, C64) | `VISIBLE_CYCLES × 8` × `(PAL_LAST - PAL_FIRST)` ≈ 416 × 312 PAL / ≈ 416 × 244 NTSC — visible cycles include border | **Yes** ✓ | matches | A | none |
| **Atari TIA** (`atari-tia`, 2600) | 160 × `lines_per_frame` (160 × 262 NTSC / 160 × 312 PAL) — visible *colour* clocks only; horizontal blank cropped, vertical includes overscan | **Partial** ⚠ — has vertical overscan but no horizontal border (HBLANK is 68 clocks before the 160 visible) | 228 × 262 (NTSC) / 228 × 312 (PAL) colour clocks; real TVs see ~192 × 224 (NTSC) | B | low — extend FB to 228 wide, fill HBLANK black |
| **Atari ANTIC + GTIA** (`atari-antic`, `atari-gtia`, 5200 / 800XL / 130XE) | 320 × 240 — visible playfield 240 lines, 320 px wide; some border space at top from ANTIC's scan_line offset of 8 | **Partial** ⚠ — vertical includes 8 lines from VBLANK; horizontal is just visible playfield | 228 colour clocks/line × 262 (NTSC) / 312 (PAL) lines; real visible ~256 × ~192-240 | B | medium — current 320 × 240 already wider than visible playfield (320 > 256), border is partially present; needs canonical width per ANTIC docs |
| **Atari MARIA** (`atari-maria`, 7800) | 320 × 240 — visible 192 lines from MARIA's region (24-216 offset baked into framebuffer addressing) | **Partial** ⚠ — same pattern as ANTIC/GTIA | same as ANTIC | B | medium — same as ANTIC/GTIA |
| **TI TMS9918** (`ti-tms9918`, MSX / ColecoVision / SG-1000 / MTX / Aquarius) | 256 × 192 — active area only | **No** ✗ | 342 × 262 (NTSC) / 342 × 313 (PAL) total; visible ~284 × ~243 with border | B | medium — extend to ~284 × 243, render border colour register (reg 7 low nibble), affects 5 machines |
| **Sega VDP** (`sega-vdp`, SMS / GG) | 256 × 192 — active area only | **No** ✗ | 342 × 262 (NTSC) / 342 × 313 (PAL); visible ~284 × ~243 | B | medium — extend to ~284 × 243, render border colour, affects 2 machines |
| **Ricoh PPU 2C02** (`ricoh-ppu-2c02`, NES) | 256 × 240 — active area only | **No** ✗ | NES PPU outputs 256 × 240, **no border generated** — real TVs cropped ~8 px top + bottom (≈ 256 × 224 visible). True border doesn't exist on NES; "border" is just the overscan that TVs hid. | **Partial** ✗ | low — the PPU is doing the right thing; what's missing is exposing the canonical TV-visible 256 × 224 crop window as an option. Document as not-actually-a-border. |
| **Motorola MC6847** (`motorola-vdg-6847` for Dragon; inline in `machine-acorn-atom`) | Dragon: per `motorola-vdg-6847` text 256 × 192; Atom: `machine_acorn_atom::vdg` 256 × 192 | **No** ✗ | MC6847 PAL outputs ~262 × 312; active 256 × 192 surrounded by border (background colour from CSS bit) | B | medium — extend to ~262 × 224 or similar canonical frame; affects Dragon + Atom (inline), CoCo if/when ported |
| **Nintendo Game Boy PPU** (`nintendo-game-boy-ppu`) | 160 × 144 from `common-nintendo-game-boy::SCREEN_WIDTH/HEIGHT` | **N/A** | Game Boy is an LCD — there is no border, the LCD is just 160 × 144. (Game Boy Player + Super Game Boy on GBA / SNES *did* add a decorative border; that's a different concern.) | C | none for native; possible future SGB-style "decorative frame" feature, but out of scope here |
| **Motorola 6845 CRTC** (`motorola-6845`, PET / BBC / CPC) | None defined at chip level — the CRTC is a generic timing chip; framebuffer dimensions are owned by the consuming machine | **N/A at chip level** | depends on the machine — PET 320 × 200 (active) / 400 × 312 PAL (total); BBC 640 × 256 (active) / 768 × 312 (total); CPC 384 × 272 / 768 × 312 | C at chip layer, B at machine layer | medium — each consumer (PET, BBC, CPC) needs its own border extension; CRTC stays unchanged |
| **Motorola SAM-6883** (`motorola-sam-6883`, Dragon / CoCo) | None — SAM is a memory controller, no video output | **N/A** | — | C | none |
| **Atari POKEY** (`atari-pokey`), **Commodore Paula** (`commodore-paula-8364`) | Audio chips with video timing inputs — no framebuffer | **N/A** | — | C | none |

## Inline chips (in machine crates)

| Machine | Inline chip | Current FB | Canonical | Category | Fix |
|---|---|---|---|---|---|
| `machine-acorn-atom` | `vdg::Mc6847` (text-mode subset) | 256 × 192 | ~262 × 224 | B | medium |
| `machine-commodore-vic-20` | `vic::Vic6560` (6560 NTSC / 6561 PAL) | 176 × 184 | NTSC: 233 × 261 / PAL: 233 × 312; visible ~210 × ~232 NTSC | B | medium |
| `machine-jupiter-ace` | `display::Display` (32×24 chars) | 256 × 192 | ZX80-class display — should have ~32 × 24 px border like ZX81 ULA | B | low — clone the ZX81 ULA border treatment |
| `machine-commodore-pet` | inline CRTC + text renderer | 320 × 200 (40-col) / 640 × 200 (80-col) | 400 × 312 PAL | B | medium |

## Summary

| Category | Count | Verdict |
|---|---|---|
| **A — Already has border, no work** | 4 chip families (Spectrum ULA × 6 variants, ZX81 ULA, Denise OCS/ECS/AGA × 3, VIC-II) | done |
| **B — Active-area only, needs border** | 7 chip families + 4 inline implementations | the actual work |
| **C — Not applicable** | 4 (Game Boy LCD, generic CRTC, SAM, audio chips) | none |

## Affected machines

Per-machine impact of the Phase 1 work, ordered roughly by user
visibility:

| Machine | Affected chip(s) | Notes |
|---|---|---|
| MSX, ColecoVision, SG-1000, MTX, Aquarius, **Sord M5**, **Tatung Einstein**, **SVI-328** | `ti-tms9918` | one chip fix unlocks 8 machines |
| SMS, **Game Gear** (if added) | `sega-vdp` | one chip fix unlocks 2 |
| NES | `ricoh-ppu-2c02` | document not-actually-a-border + add 256 × 224 crop option |
| Atari 2600 | `atari-tia` | mostly visual cleanup of HBLANK region |
| Atari 5200, 800XL, **130XE** (if added) | `atari-antic` + `atari-gtia` | shared chip pair |
| Atari 7800 | `atari-maria` | similar pattern to ANTIC/GTIA |
| Dragon-32 | `motorola-vdg-6847` | shared chip with CoCo (if added) |
| Acorn Atom | inline `vdg::Mc6847` | mirrors Dragon's chip-layer fix |
| Jupiter Ace | inline `display::Display` | low complexity — pattern after ZX81 ULA |
| VIC-20 | inline `vic::Vic6560` | medium complexity |
| PET | inline CRTC text renderer | medium complexity |
| BBC Micro | `motorola-6845` (CRTC) + machine renderer | machine-level work |

## Recommended fix order

Ordering by **leverage** (one chip fix unlocks multiple machines)
and **complexity** (low first to build confidence in the pattern):

1. ✅ **`ti-tms9918`** — unlocks 8 machines (MSX, ColecoVision, SG-1000,
   MTX, Aquarius, Sord M5, Tatung Einstein, SVI-328); medium
   complexity; border colour register exists (R7 low nibble).
   *Done in 79eb01a (256 × 192 → 288 × 240).*
2. ✅ **`sega-vdp`** — unlocks SMS; pattern mirrors TMS9918; medium
   complexity. *Done in 2121c5c (288 × 240).*
3. ✅ **`atari-tia`** — Atari 2600 only; low complexity; just extend
   FB to include HBLANK as black. *Done in fcfaff3 (160 → 228 wide);
   that commit actually filled HBLANK with COLUBK (the olive backdrop),
   contradicting this "as black" plan — corrected to black 2026-06-04
   so HBLANK matches the VBLANK=black treatment and real TIA horizontal
   blanking.*
4. ✅ **Jupiter Ace inline display** — pattern after `sinclair-zx81-ula`
   directly; low complexity. *Done in 6473efd (256 × 192 → 320 × 240).*
5. ✅ **`atari-antic` + `atari-gtia`** — Atari 5200 + 800XL; medium;
   already partial. *Done in e79d42f (320 × 240 → 384 × 288).*
6. ✅ **`atari-maria`** — Atari 7800; mirrors ANTIC/GTIA. *Done in
   6bc34fc (320 × 240 → 384 × 288).*
7. ✅ **`motorola-vdg-6847`** + **Acorn Atom inline VDG** — Dragon-32 +
   Atom; medium; canonical MC6847 border behaviour is well-documented.
   *Dragon already had border via `motorola-vdg-6847`; Atom inline done
   in 8d1d4bb (256 × 192 → 320 × 240).*
8. ✅ **`ricoh-ppu-2c02`** — NES; document + add overscan-crop option.
   Not really a border but worth aligning with reference emulators.
   *Done: documented overscan in `FB_WIDTH/FB_HEIGHT`, added
   `TV_CROP_TOP/BOTTOM`, `TV_VISIBLE_WIDTH/HEIGHT` constants and
   `framebuffer_tv_visible()` 256 × 224 helper on the NES machine.*
9. ✅ **VIC-20 inline VIC** — single machine; medium. *Done in ce36e80
   (176 × 184 → 224 × 216).*
10. ✅ **PET inline CRTC renderer** — single machine; medium. *Done in
    e02984d (320 × 200 / 640 × 200 → 384 × 248 / 704 × 248).*
11. ⏸️ **BBC Micro CRTC + renderer** — needs SAA5050 work anyway, defer.
    *Deferred to BBC Micro's own work programme.*

## What stays out

- Game Boy (no border concept — LCD)
- Generic `motorola-6845` at chip level (no fixed dimensions —
  each machine sets its own)
- `motorola-sam-6883`, `atari-pokey`, `commodore-paula-8364`
  (non-video chips)
- BBC's full SAA5050 teletext rendering — separate work, tracked
  under BBC's own `outstanding-work.md` entry

## Acceptance per chip

A chip is **done** for borders when:
1. `FB_WIDTH × FB_HEIGHT` reflects canonical real-hardware visible
   frame dimensions (active + border)
2. Border regions render the chip's canonical border colour (from
   the appropriate register, or fixed if the chip has no
   programmable border)
3. Active area renders at the canonical offset within the new
   framebuffer
4. Existing unit tests updated to match new dimensions
5. Existing gated boot smokes pass (assert framebuffer length =
   width × height; don't assert specific dimensions unless the
   test is specifically dimensional)
6. A TOSEC boot screenshot shows border visible around the active
   area, with the border colour matching reference emulator output
   for the same ROM
