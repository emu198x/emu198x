# Emulation Gaps: Road to Complete v1 Systems

Audit date: 2026-02-27. Updated: 2026-03-01 (68020 BFXXX/CAS, NES mappers 10/34/71, controller 2). Covers all four primary systems.

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

48K, 128K, +2, +2A, and +3 models are fully functional:

- **CPU**: Z80 at 100% cycle accuracy (1.6M single-step tests pass)
- **ULA**: Video, contention, INT, floating bus — all verified
- **Memory**: 48K flat layout, 128K banking ($7FFD), +3 dual banking ($7FFD + $1FFD), ROM paging, shadow screen, special all-RAM modes
- **Audio**: 1-bit beeper + AY-3-8910 PSG (3 tone, noise, envelope, 48 kHz)
- **Input**: 8×5 keyboard matrix, Kempston joystick (port $1F)
- **Storage**: TAP instant-load via ROM trap, TZX real-time signal (turbo loaders, custom protection), SNA snapshots (48K + 128K), .Z80 snapshots (v1/v2/v3), DSK/EDSK disk images (+3, read/write)
- **I/O ports**: $FE (ULA), $7FFD (banking), $1FFD (+3 banking/motor), $FFFD/$BFFD (AY), $1F (Kempston), $2FFD/$3FFD (FDC)
- **FDC**: NEC uPD765 — SPECIFY, RECALIBRATE, SEEK, SENSE INTERRUPT/DRIVE STATUS, READ DATA, WRITE DATA, READ ID, FORMAT TRACK
- **EAR bit**: Port $FE bit 6 driven by TZX signal when active, falls back to MIC output (bit 3 of last $FE write)
- **Audio**: Stereo AY output with ACB panning (A→left, C→right, B→centre)
- **CLI**: `--model 48k|128k|plus2|plus2a|plus3`, `--rom`, `--sna`, `--z80`, `--tap`, `--tzx`, `--dsk`
- **MCP**: Key input including Kempston, screenshots, state queries, 128K/+2A/+3 boot, load_z80, load_tzx, load_dsk, tape_status

### Blocking broader compatibility

No blocking gaps remain for any in-scope Spectrum model.

### Not planned

| Item | Reason |
|------|--------|
| NTSC timing | No primary or extended target uses it |
| Timex SCLD modes | TC2048/TS2068 are Phase 6+ |
| Pentagon 320-line mode | Phase 6+ |
| SNA saving | Low priority — load-only is sufficient for lessons |
| AY I/O port routing | R14/R15 stored in register file but unconnected on all Spectrum models — correct behaviour |

### Assessment

48K, 128K, +2, +2A, and +3 PAL are **complete**. The Spectrum is the most
mature of the four systems. TZX support handles turbo loaders and custom protection
schemes via real-time EAR signal simulation. The +3 FDC (NEC uPD765) supports
DSK/EDSK disk images with read and write capability.

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
- **VIC-II timing**: Sprite DMA cycle stealing (2 cycles per active sprite in HBLANK slots), badline DMA stealing
- **Sprites**: 8 sprites, single-colour and multicolour ($D01C), X/Y expand, priority
- **Sprite collisions**: Sprite-sprite ($D01E) and sprite-background ($D01F), clear-on-read, IRQ triggering
- **Audio**: SID 6581/8580 with 3 voices, ADSR (reSID die-analysis thresholds), model-dependent SVF filter (6581 non-linear curve, 8580 linear), 6581 combined waveform lookup tables, downsampling to 48 kHz
- **CIA**: Timer A/B, keyboard scanning, VIC bank selection, CIA2 NMI (edge-triggered), TOD clock (BCD, model-dependent divider), FLAG pin edge detection (tape/serial byte-ready)
- **Model variants**: PAL (6569 VIC-II, 985,248 Hz) and NTSC (6567 VIC-II, 1,022,727 Hz) with correct lines/frame, cycles/line, TOD divider, and frame rate
- **Storage**: PRG loading, CRT cartridges (type 0 Normal 8K/16K, type 1 Action Replay, type 4 Simon's BASIC, type 5 Ocean, type 10 Fun Play, type 19 Magic Desk, type 32 EasyFlash), TAP tape loading (kernal ROM trap + real-time pulse playback for turbo loaders via CIA1 FLAG), D64 disk images via full 1541 drive emulation with read and write support
- **1541 drive**: Standalone 6502 CPU + 2KB RAM + 16KB ROM + two VIA 6522 chips. IEC serial bus (ATN/CLK/DATA open-collector lines) connecting CIA2 to VIA1. GCR encode/decode, zone-dependent byte timing. Stepper motor with half-track positioning. Write state machine captures GCR bytes and flushes back to D64.
- **Input**: 8×8 keyboard matrix
- **REU**: RAM Expansion Unit (128/256/512 KB) with STASH, FETCH, SWAP, VERIFY DMA operations. Address fixing modes. Registers at $DF00-$DF0A.

### Blocking broader compatibility

No blocking gaps remain for the primary C64 target. All major media formats
(PRG, CRT, TAP, D64) are supported with read and write capability.

### Accuracy gaps

| Gap | Location | Impact |
|-----|----------|--------|
| SID per-chip filter calibration | `filter.rs` — 32-point lookup table | Piecewise-linear table from reSID die analysis captures the 6581 kink; per-chip accuracy still needs measured data from specific revisions |
| SID envelope curve | `envelope.rs` — reSID thresholds | Matches reSID die-analysis values (0x5D, 0x36, 0x1A, 0x0E, 0x06, period 30); bit-exact with reference data |
| CIA serial shift register | `cia.rs` — stubbed | Register $0C reads 0, writes ignored; not used by standard IEC serial (bit-banged via port) |

### Assessment

The C64 emulation is feature-complete for broad software compatibility.
PAL and NTSC models are supported. All six VIC-II display modes, sprite
DMA cycle stealing, and fine scrolling are implemented. The SID supports
both 6581 and 8580 models with non-linear filter curves and combined
waveform lookup tables. Seven CRT cartridge types cover the most common
hardware (including EasyFlash, the largest CRT category). TAP turbo
loaders work via real-time CIA1 FLAG pulse timing. The 1541 drive
supports both read and write operations with half-track positioning.
The REU enables REU-dependent demos and applications.

---

## NES

Boots games using 12 mappers with NTSC and PAL support, renders
backgrounds and sprites with emphasis/greyscale effects, plays all five
APU channels including DMC sample playback via DMA. Two-player input.

### Implemented

- **CPU**: 6502 at 100% cycle accuracy (2.56M single-step tests pass)
- **PPU**: Background + sprites, all mirroring modes (H/V/4-screen/single-screen)
- **APU**: Pulse (×2), triangle, noise, DMC sample playback (DMA), frame counter, mixer at 48 kHz
- **PPU effects**: PPUMASK greyscale (bit 0) and emphasis (bits 5-7) applied at pixel output, open bus latch (write-only register reads return last written value, $2002 low 5 bits from open bus)
- **Mappers**: NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4) with scanline IRQ, AxROM (7), MMC2 (9) CHR latch, MMC4 (10) CHR latch, Color Dreams (11), BxROM (34), GxROM (66), Camerica (71)
- **Mapper IRQ**: Mapper trait supports IRQ signalling; MMC3 scanline counter wired to CPU interrupt line

### Blocking broader compatibility

| Gap | Location | Impact |
|-----|----------|--------|
| DMC/OAM DMA conflict timing | `nes.rs` — DMC waits for OAM to finish | Exact halt/realign cycle count not modelled; worst-case 4-cycle steal used |
| Zapper (light gun) | Not implemented | Duck Hunt unplayable |
| Four-Score adapter | Not implemented | 4-player games blocked |
| FDS (Famicom Disk System) | Not implemented | Disk games unplayable |

### Accuracy gaps

| Gap | Location | Impact |
|-----|----------|--------|
| DMC DMA cycle-steal count | `nes.rs` — always 4 cycles | Real hardware steals 1-4 depending on CPU alignment; may shift audio timing slightly |
| Sprite zero hit cycle precision | `ppu.rs` — possibly off-by-1 | Split-screen effects may glitch |
| Bus conflicts | Not implemented | Some mapper boards have write contention |

### Assessment

**~87% of the NES library runs** (12 mappers). NTSC and PAL regions are
supported with correct frame timing, APU tables, and CPU frequency.
All five APU channels are now functional — DMC sample
playback fetches bytes via DMA, stealing 4 CPU cycles per fetch.
Drums, bass, and speech samples now play in games that use the DMC.
The DMA/OAM conflict timing is simplified (DMC waits for OAM DMA to
finish rather than interleaving), which is correct enough for audio
but not cycle-exact for timing-sensitive demos.

---

## Amiga

Boots KS 1.3 to insert-disk screen, renders bitplanes with copper and
blitter, plays Paula audio. The most complex system with the most remaining
work.

### Implemented (post-v1)

- **MOVEC instruction** (68010/020): VBR, SFC, DFC, CACR control registers. Privileged; illegal on 68000, privilege violation in user mode.
- **MULL/DIVL** (68020): 32-bit unsigned/signed multiply and divide with 32-bit and 64-bit result modes. Division by zero traps to vector 5.
- **EXTB.L** (68020): Sign-extend byte to long.
- **ADF disk write**: MFM decode of DMA write captures, sector checksum verification, write-back to ADF image. `save_adf()` API for extracting modified disk images.
- **Slow RAM**: A500 trapdoor expansion at $C00000-$DFFFFF, configurable 512K/1M/2M via `slow_ram_size` config field.
- **A1200 model**: 68020 CPU, 2MB chip RAM, AGA chipset ID registers (Alice $22, Lisa $F8).
- **68020 bit field instructions**: All 8 operations (BFTST/BFEXTU/BFEXTS/BFINS/BFSET/BFCLR/BFCHG/BFFFO) with register and memory modes.
- **68020 CAS**: Compare-and-swap for byte/word/long. Indirect, postincrement, and predecrement EA modes.
- **Paula audio filter**: One-pole RC low-pass at ~4.5 kHz matching hardware output stage.

### Blocking broader compatibility

| Gap | Location | Impact |
|-----|----------|--------|
| AGA display features | Not implemented | 8 bitplanes, HAM8, 24-bit palette, FMODE not available |
| IPF/WHDLoad formats | Not supported | Copy-protected and WHDLoad games unloadable |

### Accuracy gaps

| Gap | Location | Impact |
|-----|----------|--------|
| Blitter micro-op granularity | `agnus.rs` — atomic DMA ops | Timing under extreme contention diverges |
| Paula audio DAC non-linearity | Not modelled | Subtle DAC stepping artefacts not reproduced |
| Paula disk PLL timing | Simplified | Clock-recovery sensitive copy protection fails |
| Paula modulation edge cases | ADKCON approximate | Extreme cross-channel modulation diverges |
| ECS beam timing (BEAMCON0) | Latched but not active | Tight ECS timing code diverges |
| Sprite mid-line register timing | Approximate | SPRxPOS/CTL writes mid-scanline may have edge cases |
| Copper V7 comparison | Partial guard only | Edge cases with V7 masking may diverge |
| Blitter fill exclusive mode | Implemented but untested | May have edge cases |

### Assessment

The Amiga has the widest gap between "boots" and "runs software". HAM
and EHB display modes are now decoded in Denise. Copper SKIP is
implemented. Disk write persistence, MOVEC (68010/020), and slow RAM
are now implemented. The A1200 model routes to a 68020 CPU with full
arithmetic (MULL/DIVL/EXTB), bit field (BFXXX), and CAS support, and
reports AGA chipset IDs. AGA display features (8 bitplanes, HAM8, 24-bit
palette) are not yet rendered. The OCS core is solid; the work is in
peripheral completeness.

---

## Cross-System Summary

### Feature completeness by category

| Category | Spectrum | C64 | NES | Amiga |
|----------|----------|-----|-----|-------|
| CPU | 100% | 100% | 100% | ~99% (68000 + 68020 MULL/DIVL/EXTB/MOVEC/BFXXX) |
| Video modes | 100% | 100% (all modes + scrolling + MCM sprites + collisions + sprite DMA stealing) | ~98% (emphasis + greyscale + open bus) | ~85% (HAM + EHB + standard) |
| Audio | 100% (beeper + AY) | ~95% (6581/8580 models, non-linear filter, combined waveforms) | ~95% (all 5 channels) | ~90% (hardware LPF modelled) |
| Storage | TAP + TZX + SNA + Z80 (48K/128K) + DSK (+3) | PRG + CRT (7 types) + TAP (kernal + turbo) + D64 (read/write) | 12 mappers (0/1/2/3/4/7/9/10/11/34/66/71) | ADF read/write |
| Peripherals | Keyboard + Kempston | Keyboard + REU (128/256/512K) | 2-player pads | Keyboard + mouse |
| Model variants | 48K, 128K, +2, +2A, +3 PAL | PAL + NTSC | NTSC + PAL | A500 OCS, A500+ ECS, A1200 AGA (stub) |

### Highest-impact work items (by games-unlocked)

1. ~~**Amiga disk write**~~ — Done (MFM decode + sector checksum + ADF write-back)
2. ~~**68010 MOVEC**~~ — Done (VBR/SFC/DFC/CACR + privilege checks)
3. ~~**C64 CRT types beyond 0/5/19**~~ — Done (7 types: Normal, Action Replay, Simon's BASIC, Ocean, Fun Play, Magic Desk, EasyFlash)
4. ~~**C64 TAP turbo loaders**~~ — Done (real-time pulse playback via CIA1 FLAG)
5. ~~**C64 1541 disk write**~~ — Done (GCR decode + write state machine)

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
