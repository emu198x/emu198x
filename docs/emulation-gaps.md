# Emulation Gaps: Road to Complete v1 Systems

Audit date: 2026-02-27. Updated: 2026-02-27 (NES MMC1 mapper). Covers all four primary systems.

This document catalogues every known simplification, stub, workaround, and
missing feature across the four emulated systems. It is organised by system,
then by severity within each system.

The milestones doc distinguishes v1 (demonstrability) from post-v1
(completeness). This audit covers **everything** — both what blocks v1 exit
criteria and what blocks running real commercial software.

---

## ZX Spectrum

The cleanest system. 48K and 128K PAL are production-grade. Zero
TODO/FIXME/HACK comments in the codebase.

### Implemented

48K, 128K, and +2 models are fully functional:

- **CPU**: Z80 at 100% cycle accuracy (1.6M single-step tests pass)
- **ULA**: Video, contention, INT, floating bus — all verified
- **Memory**: 48K flat layout, 128K banking ($7FFD), ROM paging, shadow screen
- **Audio**: 1-bit beeper + AY-3-8910 PSG (3 tone, noise, envelope, 48 kHz)
- **Input**: 8×5 keyboard matrix, Kempston joystick (port $1F)
- **Storage**: TAP instant-load via ROM trap, TZX real-time signal (turbo loaders, custom protection), SNA snapshots (48K + 128K), .Z80 snapshots (v1/v2/v3)
- **I/O ports**: $FE (ULA), $7FFD (banking), $FFFD/$BFFD (AY), $1F (Kempston)
- **EAR bit**: Port $FE bit 6 driven by TZX signal when active, falls back to MIC output (bit 3 of last $FE write)
- **Audio**: Stereo AY output with ACB panning (A→left, C→right, B→centre)
- **CLI**: `--model 48k|128k|plus2`, `--rom`, `--sna`, `--z80`, `--tap`, `--tzx`
- **MCP**: Key input including Kempston, screenshots, state queries, 128K boot, load_z80, load_tzx, tape_status

### Blocking broader compatibility

| Gap | Location | Impact |
|-----|----------|--------|
| +3 disk controller (FDC) | Not implemented | +3 software unloadable; ~1000 lines of FDC emulation |

### Not planned

| Item | Reason |
|------|--------|
| NTSC timing | No primary or extended target uses it |
| Timex SCLD modes | TC2048/TS2068 are Phase 6+ |
| Pentagon 320-line mode | Phase 6+ |
| SNA saving | Low priority — load-only is sufficient for lessons |
| AY I/O port routing | R14/R15 stored in register file but unconnected on all Spectrum models — correct behaviour |

### Assessment

48K and 128K PAL are **complete**. The Spectrum is the most mature of the
four systems. TZX support now handles turbo loaders and custom protection
schemes via real-time EAR signal simulation. The only remaining gap is the
+3 FDC, which is a bounded standalone project for +3-specific software.

---

## Commodore 64

Boots to READY prompt, renders all six VIC-II display modes (standard text,
multicolour text, hires bitmap, multicolour bitmap, extended colour,
invalid-mode blackout) with XSCROLL fine scrolling and CSEL/RSEL display
window control, single-colour and multicolour sprites with collision
detection, plays SID audio.

### Implemented

- **CPU**: 6502 at 100% cycle accuracy (2.56M single-step tests pass)
- **VIC-II display modes**: Standard text, multicolour text (MCM), hires bitmap (BMM), multicolour bitmap (BMM+MCM), extended colour (ECM), invalid mode combinations
- **VIC-II scrolling**: XSCROLL fine scrolling (0-7 pixel shift with carry pipeline), YSCROLL, CSEL 38-column mode, RSEL 24-row mode
- **Sprites**: 8 sprites, single-colour and multicolour ($D01C), X/Y expand, priority
- **Sprite collisions**: Sprite-sprite ($D01E) and sprite-background ($D01F), clear-on-read, IRQ triggering
- **Audio**: SID 6581 with 3 voices, ADSR, SVF filter, downsampling to 48 kHz
- **CIA**: Timer A/B, keyboard scanning, VIC bank selection
- **Storage**: PRG loading
- **Input**: 8×8 keyboard matrix

### Blocking broader compatibility

| Gap | Location | Impact |
|-----|----------|--------|
| CIA2 NMI | `c64.rs` line 224 — stubbed | Music players and demos using CIA2 NMI fail |
| 1541 disk drive | Not implemented | D64 images cannot be loaded; PRG-only |
| Cartridge support (CRT) | Not implemented | Cartridge games unloadable |
| NTSC variant | Not implemented | PAL-only |

### Accuracy gaps

| Gap | Location | Impact |
|-----|----------|--------|
| Sprite DMA cycle stealing | `vic.rs` — only badline stealing | 2 cycles per visible sprite not stolen; frame timing off for cycle-exact demos |
| SID filter model | `filter.rs` — linear approximation | Filter sweeps sound different from real 6581 (documented, intentional for v1) |
| SID combined waveforms | `voice.rs` — AND-based | Should be transition-based; combined waveforms sound harsh |
| SID 6581 vs 8580 | Not differentiated | DC bias, filter response, combined waveforms differ between revisions |
| CIA TOD clock | `cia.rs` — returns 0 | Programs using TOD for timing fail |
| CIA serial shift register | `cia.rs` — stub | Blocks IEC serial (1541 communication) |
| SID envelope curve | `envelope.rs` — approximate thresholds | Decay/release not bit-exact with real chip |
| REU (RAM expansion) | Not implemented | REU-dependent demos fail |

### Assessment

All six VIC-II display modes, both collision registers, and fine scrolling
(XSCROLL/CSEL/RSEL) are now implemented. The SID is recognisable but not
audiophile-grade; the filter model is the main audio quality gap. The
largest remaining gaps are **1541 disk drive** (blocks D64 loading) and
**CIA2 NMI** (blocks some music players and demos).

---

## NES

Boots NROM and MMC1 games, renders backgrounds and sprites, plays
pulse/triangle/noise audio. The main gaps are **additional mappers** and
**DMC audio**.

### Implemented

- **CPU**: 6502 at 100% cycle accuracy (2.56M single-step tests pass)
- **PPU**: Background + sprites, all mirroring modes (H/V/4-screen/single-screen)
- **APU**: Pulse (×2), triangle, noise, frame counter, mixer at 48 kHz
- **Mappers**: NROM (Mapper 0), MMC1 (Mapper 1) with PRG/CHR banking, PRG RAM, dynamic mirroring

### Blocking broader compatibility

| Gap | Location | Impact |
|-----|----------|--------|
| Mapper 2 (UxROM) | Not implemented | Mega Man, Castlevania, Contra |
| Mapper 3 (CNROM) | Not implemented | Various platformers |
| Mapper 4 (MMC3) | Not implemented | SMB3, Kirby, Mega Man 3-6, ~20% of library |
| Mapper 7 (AxROM) | Not implemented | Battletoads, Marble Madness |
| DMC sample playback | `apu.rs` — stub, no DMA | Drums/bass in most game music silent |
| DMC/OAM DMA conflict | Not implemented | Cycle-stealing interaction missing |
| Zapper (light gun) | Not implemented | Duck Hunt unplayable |
| PAL timing | Hardcoded NTSC | PAL games run at wrong speed |
| Four-Score adapter | Not implemented | 4-player games blocked |
| FDS (Famicom Disk System) | Not implemented | Disk games unplayable |

### Accuracy gaps

| Gap | Location | Impact |
|-----|----------|--------|
| PPU emphasis bits ($2001 bits 5-7) | `ppu.rs` — stored, not applied | Colour tinting effects missing |
| PPU greyscale ($2001 bit 0) | Not implemented | Greyscale mode broken |
| PPU open bus | `ppu.rs` — returns 0 | Reading write-only regs should return last bus byte |
| Sprite zero hit cycle precision | `ppu.rs` — possibly off-by-1 | Split-screen effects may glitch |
| Bus conflicts | Not implemented | Some mapper boards have write contention |

### Assessment

**~35-40% of the NES library runs** (Mapper 0 + MMC1). Adding UxROM,
CNROM, and MMC3 would push coverage to ~80%. DMC DMA is the other major
gap — it requires a CPU bus access mechanism that doesn't exist yet.

---

## Amiga

Boots KS 1.3 to insert-disk screen, renders bitplanes with copper and
blitter, plays Paula audio. The most complex system with the most remaining
work.

### Blocking broader compatibility

| Gap | Location | Impact |
|-----|----------|--------|
| HAM mode (Hold-And-Modify) | `commodore-denise-ocs` — not decoded | Large category of Amiga graphics broken |
| EHB mode (Extra Half-Brite) | `commodore-denise-ocs` — not decoded | 64-colour mode broken |
| Copper SKIP instruction | `copper.rs` line 90 — stubbed | Conditional copper lists fail |
| Disk write to ADF | `drive-amiga-floppy` — captures but doesn't persist | Cannot save games or write disks |
| 68010 MOVEC instruction | `decode.rs` line 1162 — returns error | A500+ (KS 2.x) OS code fails |
| 68020 instruction extensions | `motorola-68020` — thin wrapper | A1200 code using 020 features fails |
| Slow RAM ($C00000-$DFFFFF) | Not modelled | A500 trapdoor expansion missing |
| Disk write encoding (MFM) | Not implemented | Write-back to media impossible |
| IPF/WHDLoad formats | Not supported | Copy-protected and WHDLoad games unloadable |

### Accuracy gaps

| Gap | Location | Impact |
|-----|----------|--------|
| Blitter micro-op granularity | `agnus.rs` — atomic DMA ops | Timing under extreme contention diverges |
| Paula audio filtering/DAC | Not modelled | Audio sounds "too clean"; no hardware warmth |
| Paula disk PLL timing | Simplified | Clock-recovery sensitive copy protection fails |
| Paula modulation edge cases | ADKCON approximate | Extreme cross-channel modulation diverges |
| ECS beam timing (BEAMCON0) | Latched but not active | Tight ECS timing code diverges |
| Sprite mid-line register timing | Approximate | SPRxPOS/CTL writes mid-scanline may have edge cases |
| Copper V7 comparison | Partial guard only | Edge cases with V7 masking may diverge |
| Blitter fill exclusive mode | Implemented but untested | May have edge cases |

### Assessment

The Amiga has the widest gap between "boots" and "runs software". **HAM
mode** alone blocks a huge category of graphics. Copper SKIP, disk write,
and the 68010/020 instruction gaps block running on anything beyond a stock
A500 with KS 1.3. The OCS core is solid; the work is in expanding display
modes and peripheral completeness.

---

## Cross-System Summary

### Feature completeness by category

| Category | Spectrum | C64 | NES | Amiga |
|----------|----------|-----|-----|-------|
| CPU | 100% | 100% | 100% | 95% (68000 only) |
| Video modes | 100% | ~95% (all modes + scrolling + MCM sprites + collisions) | ~90% (missing emphasis/greyscale) | ~60% (missing HAM/EHB) |
| Audio | 100% (beeper + AY) | ~85% (filter approximate) | ~80% (no DMC) | ~85% (no filter model) |
| Storage | TAP + TZX + SNA + Z80 (48K/128K) | PRG only | NROM + MMC1 | ADF read only |
| Peripherals | Keyboard + Kempston | Keyboard | 2-player pad | Keyboard + mouse |
| Model variants | 48K, 128K, +2 PAL | PAL only | NTSC only | A500 OCS only |

### Highest-impact work items (by games-unlocked)

1. **NES mappers** (UxROM + CNROM + MMC3) — unlocks ~45% more of NES library
2. **C64 1541 disk drive** — unlocks D64 loading (huge library unlock)
3. **Amiga HAM/EHB modes** — unlocks large category of Amiga graphics
4. **NES DMC DMA** — completes audio for most NES games
5. **Amiga Copper SKIP** — unlocks conditional copper lists
6. **Amiga disk write** — unlocks game saves
7. **68010/020 instructions** — unlocks A500+/A1200
8. **C64 CIA2 NMI** — unlocks music players and demos

### v1 exit criteria status

Per milestones.md, v1 requires demonstrability (boot, run a program, produce
stable A/V, expose state, scripted capture) — **not** broad compatibility.

| System | v1 status | Remaining for v1 exit |
|--------|-----------|-----------------------|
| Spectrum | Ready | None — all criteria met |
| C64 | Ready | None — boots, SID works, sprites render |
| NES | Ready | None — APU now implemented, NROM games run |
| Amiga | Ready | None — KS 1.3 boots, copper/blitter demos work |

All four systems meet v1 exit criteria today. Everything in this document
is **post-v1 completeness work** (Track C in milestones.md).
