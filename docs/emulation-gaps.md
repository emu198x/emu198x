# Emulation Gaps: Road to Complete v1 Systems

Audit date: 2026-02-27. Updated: 2026-03-01 (blitter DMA interleaving, close remaining Amiga gaps). Covers all four primary systems.

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
- **Audio**: 1-bit beeper + AY-3-8910 PSG (3 tone, noise, envelope, stereo ACB panning, 48 kHz)
- **Input**: 8×5 keyboard matrix, Kempston joystick (port $1F)
- **Storage**: TAP instant-load via ROM trap, TZX real-time signal (turbo loaders, custom protection), SNA snapshots (48K + 128K), .Z80 snapshots (v1/v2/v3), DSK/EDSK disk images (+3, read/write)
- **I/O ports**: $FE (ULA), $7FFD (banking), $1FFD (+3 banking/motor), $FFFD/$BFFD (AY), $1F (Kempston), $2FFD/$3FFD (FDC)
- **FDC**: NEC uPD765 — SPECIFY, RECALIBRATE, SEEK, SENSE INTERRUPT/DRIVE STATUS, READ DATA, WRITE DATA, READ ID, FORMAT TRACK
- **EAR bit**: Port $FE bit 6 driven by TZX signal when active, falls back to MIC output (bit 3 of last $FE write)
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
mature of the four systems. TZX support handles turbo loaders and custom
protection schemes via real-time EAR signal simulation. The +3 FDC (NEC
uPD765) supports DSK/EDSK disk images with read and write capability.

---

## Commodore 64

Feature-complete for broad software compatibility. Boots to READY prompt,
renders all six VIC-II display modes with fine scrolling, sprites with
collision detection, plays SID audio with model-accurate filter. PAL and
NTSC models supported.

### Implemented

- **CPU**: 6502 at 100% cycle accuracy (2.56M single-step tests pass)
- **VIC-II display modes**: Standard text, multicolour text (MCM), hires bitmap (BMM), multicolour bitmap (BMM+MCM), extended colour (ECM), invalid mode combinations
- **VIC-II scrolling**: XSCROLL fine scrolling (0-7 pixel shift with carry pipeline), YSCROLL, CSEL 38-column mode, RSEL 24-row mode
- **VIC-II timing**: Sprite DMA cycle stealing (2 cycles per active sprite in HBLANK slots), badline DMA stealing (cycles 15-54)
- **Sprites**: 8 sprites, single-colour and multicolour ($D01C), X/Y expand, priority
- **Sprite collisions**: Sprite-sprite ($D01E) and sprite-background ($D01F), clear-on-read, IRQ triggering via edge-latched flags
- **Audio**: SID 6581/8580 with 3 voices, ADSR (reSID die-analysis thresholds), model-dependent SVF filter (6581 32-point piecewise-linear lookup table from die analysis, 8580 linear), 6581 combined waveform lookup tables, downsampling to 48 kHz
- **SID registers**: POTX ($D419) and POTY ($D41A) return configurable paddle ADC values (default $80 centre), OSC3 ($D41B) and ENV3 ($D41C) readable
- **CIA**: Timer A/B, keyboard scanning, VIC bank selection, CIA2 NMI (edge-triggered), TOD clock (BCD, model-dependent divider), FLAG pin edge detection (tape/serial byte-ready)
- **Model variants**: PAL (6569 VIC-II, 985,248 Hz) and NTSC (6567 VIC-II, 1,022,727 Hz) with correct lines/frame, cycles/line, TOD divider, and frame rate
- **Storage**: PRG loading, CRT cartridges (type 0 Normal 8K/16K, type 1 Action Replay, type 4 Simon's BASIC, type 5 Ocean, type 10 Fun Play, type 19 Magic Desk, type 32 EasyFlash), TAP tape loading (kernal ROM trap + real-time pulse playback for turbo loaders via CIA1 FLAG), D64 disk images via full 1541 drive emulation with read and write support
- **1541 drive**: Standalone 6502 CPU + 2KB RAM + 16KB ROM + two VIA 6522 chips. IEC serial bus (ATN/CLK/DATA open-collector lines) connecting CIA2 to VIA1. GCR encode/decode, zone-dependent byte timing. Stepper motor with half-track positioning. Write state machine captures GCR bytes and flushes back to D64.
- **Input**: 8×8 keyboard matrix, joystick ports 1+2
- **REU**: RAM Expansion Unit (128/256/512 KB) with STASH, FETCH, SWAP, VERIFY DMA operations. Address fixing modes. Registers at $DF00-$DF0A.
- **MCP**: Boot, reset, run_frames, screenshot, audio capture (WAV encoding with base64 + save_path), key input, state queries, load media

### Blocking broader compatibility

No blocking gaps remain for the primary C64 target. All major media formats
(PRG, CRT, TAP, D64) are supported with read and write capability.

### Accuracy gaps

| Gap | Location | Impact |
|-----|----------|--------|
| SID per-chip filter calibration | `filter.rs` — 32-point lookup table | Table captures the 6581 kink shape from reSID die analysis; true per-chip accuracy needs measured data from specific revisions |
| SID envelope curve | `envelope.rs` — reSID thresholds | Matches reSID die-analysis values (0x5D, 0x36, 0x1A, 0x0E, 0x06, period 30); bit-exact with reference data |

### Not planned

| Item | Reason |
|------|--------|
| CIA serial shift register | $0C reads 0, writes ignored. Not used by standard IEC serial (bit-banged via CIA2 port). Only matters for non-standard hardware |
| Light pen input | $D013-$D014 register values stored but not wired to input. Very few C64 games use light pen |
| VIC-II floating bus | CPU port $01 undriven inputs return high (pull-up), not last-read-on-bus. Affects a handful of copy-protection schemes |

### Assessment

The C64 emulation is **feature-complete** for broad software compatibility.
PAL and NTSC models are supported. All six VIC-II display modes, sprite
DMA cycle stealing, and fine scrolling are implemented. The SID supports
both 6581 and 8580 models with a 32-point piecewise-linear filter lookup
table that captures the 6581's distinctive low-end kink. Seven CRT
cartridge types cover the most common hardware (including EasyFlash, the
largest CRT category). TAP turbo loaders work via real-time CIA1 FLAG
pulse timing. The 1541 drive supports both read and write operations with
half-track positioning. The REU enables REU-dependent demos and
applications. MCP audio capture returns full WAV-encoded SID output.

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
- **Input**: Two standard controllers ($4016/$4017), Four-Score 4-player adapter ($4016/$4017 extended read with P3/P4 and signature), Zapper light gun (port 2, light sense from framebuffer brightness, trigger)
- **Bus conflicts**: UxROM, CNROM, AxROM, BxROM — written value ANDed with ROM data at write address
- **Region**: NTSC (262 scanlines, 1.79 MHz CPU) and PAL (312 scanlines, 1.66 MHz CPU) with region-specific APU noise period, DMC rate, and frame counter tables

### Blocking broader compatibility

| Gap | Location | Impact |
|-----|----------|--------|
| FDS (Famicom Disk System) | Not implemented | Disk games unplayable |

### Accuracy gaps

No significant accuracy gaps remain. DMC DMA cycle stealing is now
variable (1 for writes, 2 for even-aligned reads, 3 for odd-aligned
reads) based on the CPU's last bus operation. DMC DMA can now interrupt
OAM DMA rather than waiting for it to finish.

### Assessment

**~87% of the NES library runs** (12 mappers). NTSC and PAL regions are
supported with correct frame timing, APU tables, and CPU frequency.
All five APU channels are functional — DMC sample playback fetches bytes
via DMA, stealing 4 CPU cycles per fetch. Two-player controller input is
wired through both $4016 and $4017. The DMA/OAM conflict timing is
simplified (DMC waits for OAM DMA to finish rather than interleaving),
which is correct enough for audio but not cycle-exact for timing-sensitive
demos.

---

## Amiga

Boots KS 1.3, 2.04, and 3.1 to insert-disk screen. Renders bitplanes
with copper and blitter, plays Paula audio with hardware low-pass filter.
Three model variants: A500 (OCS), A500+ (ECS), A1200 (AGA).

### Implemented

- **CPU**: 68000 at 100% cycle accuracy (317,500 single-step tests pass)
- **68020 extensions** (A1200): MULL/DIVL (32×32 multiply/divide with 32-bit and 64-bit results), EXTB.L, MOVEC (VBR/SFC/DFC/CACR), all 8 bit field instructions (BFTST/BFEXTU/BFEXTS/BFINS/BFSET/BFCLR/BFCHG/BFFFO with register and memory modes), CAS (compare-and-swap byte/word/long)
- **Chipsets**: OCS (A500), ECS (A500+), AGA (A1200)
- **Video**: Bitplane DMA (1-8 planes), copper, blitter (copy, line, fill), HAM6/HAM8 and EHB modes, full-raster framebuffer at hires resolution
- **AGA display**: 8 bitplanes (4-bit BPU decode), 256-entry 24-bit palette with BPLCON3 bank selection and LOCT, HAM8 (6-bit data with 8-bit expansion), BPLCON4 colour offset (bitplane and sprite XOR), FMODE wider DMA fetches (32-bit and 64-bit) with FIFO buffering, FMODE sprite width modes (16/32/64-pixel sprites via bits 3-2)
- **Audio**: Paula 4-channel DMA with volume/period modulation (ADKCON), stereo routing (0+3 left, 1+2 right), one-pole RC low-pass filter at ~4.5 kHz matching hardware output stage, DAC non-linearity table modelling A500 resistor-ladder output
- **Storage**: ADF read and write (MFM decode, sector checksum, floppy DMA via DSKLEN double-write protocol), disk write persistence with `save_adf()` API, IPF disk images (read-only, pre-encoded MFM, copy-protection timing metadata, auto-detected by magic bytes)
- **Memory**: Chip RAM (512K A500, 1MB A500+, 2MB A1200), slow RAM ($C00000-$DFFFFF, configurable 512K/1M/2M), ROM overlay
- **Peripherals**: Keyboard, mouse, CIA-A/B (8520) with TOD, floppy status, battclock simulation
- **Models**: A500 (OCS, 512K), A500+ (ECS, 1MB), A1200 (AGA, 68020, 2MB)

### Blocking broader compatibility

| Gap | Location | Impact |
|-----|----------|--------|
| WHDLoad format | Not supported | WHDLoad games need IDE/filesystem/autoconfig infrastructure |

### Accuracy gaps

No significant accuracy gaps remain for any in-scope Amiga model.

### Recently resolved

| Item | Resolution |
|------|-----------|
| Blitter per-channel DMA interleaving | Per-word state machine replaces pre-built VecDeque queue; individual channel accesses can be granted in any order (WriteD waits for all reads) |
| AGA scan-doubled output | Raster FB writes at hires coordinates; lores pixels duplicate across 2 sub-positions — no real gap exists |
| IPF disk support | `format-ipf` crate parses IPF container, `DiskImage` trait abstracts disk sources, auto-detected in MCP and runner |
| Copper V7 comparison | V7 forced into comparison mask — fixes false-early and false-late cases |
| Paula audio DAC non-linearity | 256-entry lookup table models A500 resistor-ladder S-curve |
| Paula disk PLL timing | Phase accumulator for variable-rate IPF tracks |
| ECS BEAMCON0 activation | Sync polarity (HSYTRUE/VSYTRUE/CSYTRUE), BLANKEN blank gating, CSCBEN composite sync routing |
| Sprite mid-line register timing | 1-pixel pipeline delay via `spr_pos_pending` array |
| Blitter fill exclusive mode | 6 integration tests cover IFE, EFE, carry propagation, FCI seed, descending mode |
| Paula modulation coverage | Additional tests for attach-period, attach-volume, combined modulation, modulator muting |
| AGA palette LOCT timing | Color writes capture BPLCON3 snapshot at write time for correct bank/LOCT ordering through pipeline |

### Assessment

The Amiga emulator is approaching broad compatibility. KS 1.3, 2.04, and
3.1 all boot to insert-disk screens with correct display rendering. IPF
disk images are now supported via the `format-ipf` crate, enabling
copy-protected titles. The `DiskImage` trait abstracts disk sources so
ADF and IPF (and future formats) share the same floppy drive interface.
The AGA display pipeline is functional: 8 bitplanes, 256-entry 24-bit
palette with BPLCON3 bank/LOCT selection (now correctly pipelined), HAM8,
BPLCON4 colour offset, FMODE wider DMA fetches with FIFO buffering, and
FMODE-controlled sprite widths. Paula audio now includes A500 DAC
non-linearity modelling and a disk PLL phase accumulator for
variable-rate MFM streams. ECS BEAMCON0 sync polarity, blank gating, and
composite sync routing are active. Copper V7 comparison is correct.
Sprite position writes go through a 1-pixel pipeline delay matching
hardware. The main remaining gap is WHDLoad support (deferred — needs
IDE/filesystem/autoconfig).

---

## Cross-System Summary

### Feature completeness by category

| Category | Spectrum | C64 | NES | Amiga |
|----------|----------|-----|-----|-------|
| CPU | 100% | 100% | 100% | ~99% (68000 + 68020 MULL/DIVL/EXTB/MOVEC/BFXXX/CAS) |
| Video modes | 100% | 100% (all 6 modes + scrolling + sprites + collisions) | ~98% (emphasis + greyscale + open bus) | ~97% (OCS/ECS/AGA bitplanes + HAM6/8 + EHB + 24-bit palette + FMODE + sprite widths) |
| Audio | 100% (beeper + AY) | ~97% (6581/8580, piecewise filter table, combined waveforms) | ~95% (all 5 channels) | ~93% (hardware LPF + DAC non-linearity) |
| Storage | TAP + TZX + SNA + Z80 + DSK | PRG + CRT (7 types) + TAP (turbo) + D64 (r/w) | 12 mappers | ADF read/write + IPF read |
| Peripherals | Keyboard + Kempston | Keyboard + joystick + REU + paddles | 4-player pads + Zapper | Keyboard + mouse |
| Model variants | 48K, 128K, +2, +2A, +3 | PAL + NTSC | NTSC + PAL | A500, A500+, A1200 |

### Highest-impact work items (by games-unlocked)

1. ~~**Amiga disk write**~~ — Done
2. ~~**68010/020 MOVEC**~~ — Done
3. ~~**C64 CRT types beyond 0/5/19**~~ — Done (7 types)
4. ~~**C64 TAP turbo loaders**~~ — Done
5. ~~**C64 1541 disk write**~~ — Done
6. ~~**NES PAL timing**~~ — Done
7. ~~**68020 bit fields**~~ — Done (all 8 instructions)
8. ~~**68020 CAS**~~ — Done
9. ~~**SID 6581 filter accuracy**~~ — Done (lookup table)
10. ~~**AGA display rendering**~~ — Done (8 bitplanes, 24-bit palette, HAM8, BPLCON4, FMODE)
11. ~~**AGA sprite width modes**~~ — Done (FMODE bits 2-3 for 16/32/64-pixel sprites)
12. ~~**IPF disk support**~~ — Done (format-ipf crate, DiskImage trait, auto-detection)
13. **WHDLoad support** — Needs IDE/filesystem/autoconfig infrastructure

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
