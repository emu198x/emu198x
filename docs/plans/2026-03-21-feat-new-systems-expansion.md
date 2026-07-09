> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "New Systems Expansion: 30+ Systems Across 8 Manufacturers"
type: feat
date: 2026-03-21
---

# New Systems Expansion

## Overview

Expand from 13 to 40+ systems, covering the complete 8-bit lines of
Sinclair, Commodore, Apple, Amstrad, Acorn, and Atari, plus esoteric
systems that few remember today. Ordered by chip reuse to maximise
velocity — systems sharing existing chips ship fastest.

## New CPU Crates Needed

| Crate | CPU | Used by |
|-------|-----|---------|
| motorola-6809 | 6809 | Dragon 32, Tandy CoCo, Vectrex |
| mos-65c02 | 65C02 | Apple IIe, BBC Master, Atari Lynx |
| ge-cp1610 | CP1610 | Intellivision |
| fairchild-f8 | F8 3850/3851 | Channel F |
| intel-8048 | 8048 | Odyssey² |

The 65C02 can be a thin wrapper around mos-6502 adding the extra
opcodes (PHX/PHY/PLX/PLY/STZ/BRA/TRB/TSB and (zp) addressing mode).

## New Chip Crates Needed

| Crate | Chip | Used by |
|-------|------|---------|
| sinclair-zx81-ula | ZX81 ULA | ZX81 |
| motorola-mc6847 | MC6847 VDG | Dragon, CoCo, Atom |
| motorola-sam | SAM (6883) | Dragon, CoCo |
| commodore-vic-6560 | VIC 6560/6561 | VIC-20 |
| commodore-ted-8360 | TED 8360 | C16/Plus4 |
| commodore-vdc-8563 | VDC 8563 | C128 |
| amstrad-gate-array | CPC Gate Array | CPC 464/6128/GX4000 |
| acorn-electron-ula | Electron ULA | Electron |
| oric-ula | Oric ULA | Oric-1/Atmos |
| sam-asic | SAM Coupé ASIC | SAM Coupé |
| atari-mikey | Mikey | Atari Lynx |
| atari-suzy | Suzy | Atari Lynx |
| mattel-stic | STIC 8900 | Intellivision |
| fairchild-f8-vram | Channel F video | Channel F |

## Implementation Waves

### Wave 1: Pure Chip Reuse (Z80 + existing chips)

These need only a small custom chip (ULA/gate array) — the CPU and
major chips already exist. 1–2 days each.

| # | System | Manufacturer | CPU | Video | Audio | New chips |
|---|--------|-------------|-----|-------|-------|-----------|
| 1 | ZX81 | Sinclair | Z80 ✓ | ULA | beeper | sinclair-zx81-ula |
| 2 | ZX80 | Sinclair | Z80 ✓ | discrete | beeper | (subset of ZX81) |
| 3 | Memotech MTX | Memotech | Z80 ✓ | TMS9918 ✓ | SN76489 ✓ | none |
| 4 | Tatung Einstein | Tatung | Z80 ✓ | TMS9918 ✓ | AY-3-8910 ✓ | none |
| 5 | Sord M5 | Sord | Z80 ✓ | TMS9918 ✓ | SN76489 ✓ | none |
| 6 | Spectravideo SVI-328 | Spectravideo | Z80 ✓ | TMS9918 ✓ | AY-3-8910 ✓ | none |
| 7 | Jupiter Ace | Jupiter Cantab | Z80 ✓ | custom char | beeper | simple ULA |
| 8 | Mattel Aquarius | Mattel | Z80 ✓ | TEA1002 | beeper | simple video |

### Wave 2: 6502 + Existing Chips

Need no new CPU. 2–3 days each.

| # | System | CPU | Video | Audio | New chips |
|---|--------|-----|-------|-------|-----------|
| 9 | Oric-1/Atmos | 6502 ✓ | ULA | AY-3-8910 ✓ | oric-ula |
| 10 | Acorn Electron | 6502 ✓ | ULA | beeper | acorn-electron-ula |
| 11 | Acorn Atom | 6502 ✓ | MC6847 | beeper | motorola-mc6847 |
| 12 | VIC-20 | 6502 ✓ | VIC 6560 | VIC 6560 | commodore-vic-6560 |
| 13 | PET | 6502 ✓ | 6845 ✓ | beeper | none (char display) |

### Wave 3: Z80 + New Video Chip

Need a new video chip but CPU is shared. 2–4 days each.

| # | System | CPU | Video | Audio | New chips |
|---|--------|-----|-------|-------|-----------|
| 14 | Amstrad CPC 464 | Z80 ✓ | 6845 ✓ + Gate Array | AY-3-8910 ✓ | amstrad-gate-array |
| 15 | Amstrad CPC 6128 | Z80 ✓ | (same) | (same) | (model variant) |
| 16 | SAM Coupé | Z80 ✓ | ASIC | SAM ASIC | sam-asic |
| 17 | Camputers Lynx | Z80 ✓ | custom | beeper | simple video |
| 18 | Sharp MZ-700 | Z80 ✓ | custom | beeper | simple char display |
| 19 | Bally Astrocade | Z80 ✓ | custom | custom | bally-custom |

### Wave 4: New CPU Required

Need a new CPU crate. 3–5 days each (CPU + system).

| # | System | CPU | Video | Audio | Notes |
|---|--------|-----|-------|-------|-------|
| 20 | Apple II | 6502 ✓ | soft switches | speaker | disk II controller |
| 21 | Apple IIe | 65C02 *new* | (enhanced) | (same) | mos-65c02 crate |
| 22 | BBC Master | 65C02 *new* | 6845 ✓ | SN76489 ✓ | uses mos-65c02 |
| 23 | Dragon 32/64 | 6809 *new* | MC6847 | beeper | motorola-6809 + mc6847 |
| 24 | Tandy CoCo | 6809 *new* | MC6847 | beeper | shares Dragon chips |
| 25 | Vectrex | 6809 *new* | vector DAC | AY-3-8910 ✓ | unique display |
| 26 | Atari Lynx | 65C02 *new* | Mikey+Suzy | Mikey | handheld |

### Wave 5: Exotic CPUs

New CPU architectures. 4–7 days each.

| # | System | CPU | Video | Audio | Notes |
|---|--------|-----|-------|-------|-------|
| 27 | C16/Plus4 | 7501 (6502 variant) | TED 8360 | TED 8360 | TED does video+audio |
| 28 | C128 | Z80 ✓ + 8502 (6502) | VIC-II ✓ + VDC | SID ✓ | dual CPU |
| 29 | Intellivision | CP1610 *new* | STIC *new* | AY-3-8914 ✓ | unusual CPU |
| 30 | Odyssey² | 8048 *new* | custom VDP | 8048 internal | Intel MCU |
| 31 | Channel F | F8 *new* | custom | beeper | first cart console |
| 32 | GX4000 | Z80 ✓ | CPC+ ASIC | AY ✓ | enhanced CPC |

## Suggested Implementation Order

Start with Wave 1 (maximum reuse, fastest delivery), work through
to Wave 5. Within each wave, do the simplest system first.

| Order | System | Effort | Chip reuse |
|-------|--------|--------|------------|
| 1 | ZX81 | 1 day | Z80, simple ULA |
| 2 | Memotech MTX | 1 day | Z80 + TMS9918 + SN76489 |
| 3 | Tatung Einstein | 1 day | Z80 + TMS9918 + AY |
| 4 | Sord M5 | 1 day | Z80 + TMS9918 + SN76489 |
| 5 | SVI-328 | 1 day | Z80 + TMS9918 + AY |
| 6 | Jupiter Ace | 1 day | Z80, Forth display |
| 7 | Mattel Aquarius | 1 day | Z80, simple video |
| 8 | ZX80 | 0.5 day | subset of ZX81 |
| 9 | PET | 1–2 days | 6502 + 6845 |
| 10 | Oric Atmos | 2 days | 6502 + AY |
| 11 | Electron | 2 days | 6502 + ULA |
| 12 | VIC-20 | 2–3 days | 6502 + VIC chip |
| 13 | Atom | 2 days | 6502 + MC6847 |
| 14 | CPC 464 | 3 days | Z80 + AY + 6845 + Gate Array |
| 15 | CPC 6128 | 0.5 day | variant of CPC 464 |
| 16 | Apple II | 3 days | 6502 + soft switches |
| 17 | Apple IIe | 1 day | 65C02 crate + Apple II variant |
| 18 | BBC Master | 1 day | 65C02 + BBC variant |
| 19 | Dragon 32 | 3 days | 6809 crate + MC6847 |
| 20 | CoCo | 0.5 day | variant of Dragon |
| 21 | SAM Coupé | 3 days | Z80 + ASIC |
| 22 | Sharp MZ-700 | 2 days | Z80 + simple display |
| 23 | Camputers Lynx | 2 days | Z80 + simple display |
| 24 | Vectrex | 3 days | 6809 + vector display |
| 25 | Atari Lynx | 4 days | 65C02 + Mikey/Suzy |
| 26 | VIC-20 → C16/Plus4 | 3 days | TED chip |
| 27 | C128 | 4 days | dual CPU, two video chips |
| 28 | Intellivision | 5 days | new CPU + STIC |
| 29 | Odyssey² | 4 days | 8048 MCU |
| 30 | Channel F | 4 days | F8 CPU |
| 31 | Bally Astrocade | 3 days | Z80 + custom |
| 32 | GX4000 | 1 day | CPC+ variant |

## Chip Reuse Matrix

How many new systems each existing chip enables:

| Chip | Systems |
|------|---------|
| zilog-z80 | ZX81, ZX80, MTX, Einstein, M5, SVI, Jupiter Ace, Aquarius, CPC, SAM Coupé, Lynx (Camputers), MZ-700, Astrocade, C128 (14) |
| mos-6502 | Apple II, VIC-20, PET, Oric, Electron, Atom, C16 (7) |
| ti-tms9918 | MTX, Einstein, M5, SVI (4) |
| gi-ay-3-8910 | CPC, Oric, Einstein, SVI, SAM Coupé, Vectrex (6) |
| ti-sn76489 | MTX, M5 (2) |
| motorola-6845 | CPC, PET (2) |

## Non-Goals

- 16-bit systems (Amiga covers that niche for now)
- Computers that are primarily 16/32-bit (ST, Archimedes)
- Consoles beyond the 8-bit era (Mega Drive, SNES)
- Handhelds beyond Lynx (Game Boy is future-systems.md scope)
