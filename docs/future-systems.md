# Future Systems

Systems beyond the four primaries (Spectrum, C64, NES, Amiga). **Not in scope**
until all primary systems are complete. Structure code so they're possible, then
forget about them until the core four ship.

For current system status, see [roadmap.md](roadmap.md).

---

## 8-Bit Computers

| System | CPU | Key Chips | Priority |
|--------|-----|-----------|----------|
| Amstrad CPC | `zilog-z80` | MC6845, AY-3-8910, PPI 8255, uPD765 | High |
| MSX1 | `zilog-z80` | TMS9918A, AY-3-8910, PPI 8255 | High |
| MSX2 | `zilog-z80` | V9938, AY-3-8910, PPI 8255 | High |
| MSX2+ | `zilog-z80` | V9958, AY-3-8910, PPI 8255, YM2413 | High |
| MSX turboR | R800 | V9958, YM2149, PPI 8255, YM2413 | High |
| BBC Micro | `mos-6502` | MC6845, SN76489, VIA 6522, WD1770 | Medium |
| Atari 8-bit | `mos-6502` | ANTIC, GTIA, POKEY, PIA 6520 | Medium |
| Apple II | `mos-6502` | — (IIGS: Ensoniq DOC 5503, 65C816) | Medium |
| ZX81 | `zilog-z80` | Ferranti ULA ZX81 | Low |

## 16-Bit Computers

| System | CPU | Key Chips | Priority |
|--------|-----|-----------|----------|
| Atari ST/STE | `motorola-68000` | YM2149, MC6850, MC68901 MFP, WD1770, Shifter | High |
| Atari TT | `motorola-68030` | Above + TT Shifter | Low |
| Atari Falcon | `motorola-68030` | VIDEL, DSP56001 | Low |
| Sharp X68000 | `motorola-68000` | YM2151, OKI MSM6258, custom video | Low |

## Consoles

| System | CPU(s) | Key Chips | Priority |
|--------|--------|-----------|----------|
| Sega Master System | `zilog-z80` | Sega VDP 315-5124, SN76489 | High |
| Game Gear | `zilog-z80` | Sega VDP (viewport variant), SN76489 | High (with SMS) |
| Sega Genesis | `motorola-68000` + `zilog-z80` | Genesis VDP, YM2612, SN76489 | High |
| SNES | 65C816 + SPC700 | SNES PPU, SNES DSP | High |
| Game Boy / GBC | Sharp SM83 | GB PPU, GB APU | High |
| PC Engine | HuC6280 | HuC6270 VDC, HuC6260 VCE | Medium |
| Game Boy Advance | ARM7TDMI + SM83 | GBA PPU | Medium |
| Neo Geo | `motorola-68000` + `zilog-z80` | LSPC, YM2610 | Medium |
| Mega CD | `motorola-68000` × 2 | RF5C164, ASIC MCD | Low |
| 32X | SH-2 × 2 | 32X VDP | Low |

---

## CPUs Needed

| Crate | CPU | Used By |
|-------|-----|---------|
| `wdc-65c02` | 65C02 | Apple IIe/IIc, PC Engine |
| `wdc-65c816` | 65C816 | SNES, Apple IIGS |
| `mos-8502` | 8502 | C128 |
| `sharp-sm83` | SM83 | Game Boy, GBC |
| `ascii-r800` | R800 | MSX turboR |
| `sony-spc700` | SPC700 | SNES audio |
| `hitachi-sh2` | SH-2 | 32X, Saturn |
| `arm-arm7tdmi` | ARM7TDMI | GBA |
| `hudson-huc6280` | HuC6280 | PC Engine |

---

## Existing-System Variants (Not Started)

Variants and expansions for the four primary systems that aren't implemented yet.

### Spectrum

| Variant | Notes |
|---------|-------|
| Scorpion ZS-256 | Different memory mapping, turbo modes |
| Timex 2068 | SCLD extended video modes, different AY ports, cartridge |
| Interface 1 | Microdrive, RS-232, ZX Net, shadow ROM |
| DivIDE/DivMMC | IDE/SD, shadow ROM paged on M1 traps |
| Multiface | NMI button, shadow ROM/RAM |
| Beta 128 (TR-DOS) | WD1793 FDC, up to 4 drives |

### Commodore 64

| Variant | Notes |
|---------|-------|
| SX-64 | No Datasette, built-in 1541, 60Hz TOD |
| C64 GS | Cartridge-only, no keyboard |
| PAL-N (6572) | 65 cycles/line, 312 lines |
| 1581 drive | 3.5" DD, WD1770 + CIA 8520 |
| SwiftLink | ACIA 6551, RS-232 up to 38.4 kbps |
| 1351 Mouse | Proportional mouse via SID pots |
| CMD SuperCPU | 65816 @ 20 MHz, 16MB RAM |

### Commodore 128

Not started. Requires `mos-8502`, `mos-vdc-8563`/`8568`, Z80 co-processor
mode.

### NES

| Variant | Notes |
|---------|-------|
| Famicom Disk System | `nintendo-fds-2c33`, 32KB RAM, wavetable audio, QD drive |
| VS. System | Scrambled palettes, coin-op hardware, DIP switches |
| Expansion audio mappers | VRC6 (24/26), VRC7 (85), Namco 163 (19), Sunsoft 5B (69), MMC5 (5) |

Each expansion audio mapper contains a sound chip that could be its own crate:
`konami-vrc6-audio` (2 pulse + 1 sawtooth), `konami-vrc7-audio` (YM2413
derivative, 6-ch FM), `namco-163-audio` (8-ch wavetable), `sunsoft-5b-audio`
(YM2149 variant, 3-ch PSG).

---

## Shared Chip Crates

Chips needed by future systems. See the reuse matrix below for cross-system
value.

### Sound

| Crate | Chip | Systems |
|-------|------|---------|
| `yamaha-ym2149` | YM2149F | MSX, Atari ST, Sunsoft 5B |
| `texas-instruments-sn76489` | SN76489 | BBC Micro, SMS, Game Gear, Genesis |
| `yamaha-ym2612` | YM2612 (OPN2) | Genesis |
| `yamaha-ym2610` | YM2610 (OPNB) | Neo Geo |
| `yamaha-ym2151` | YM2151 (OPM) | X68000, arcade |
| `yamaha-ym2413` | YM2413 (OPLL) | MSX2+, SMS Japan |
| `atari-pokey` | POKEY | Atari 8-bit |
| `ensoniq-doc-5503` | Ensoniq DOC | Apple IIGS |

Yamaha FM chips share common operator logic — a `ym-fm-core` internal module
handles envelope/phase generators, sine tables, feedback, and LFO.

### Video

| Crate | Chip | Systems |
|-------|------|---------|
| `texas-instruments-tms9918a` | TMS9918A | MSX1, Colecovision, SG-1000 |
| `yamaha-v9938` | V9938 | MSX2 (wraps TMS9918A) |
| `yamaha-v9958` | V9958 | MSX2+ (wraps V9938) |
| `sega-315-5124` | Sega VDP | SMS, Game Gear |
| `motorola-mc6845` | MC6845 CRTC | CPC, BBC Micro |
| `atari-antic` | ANTIC | Atari 8-bit |
| `atari-gtia` | GTIA | Atari 8-bit |

### Support

| Crate | Chip | Systems |
|-------|------|---------|
| `intel-ppi-8255` | PPI 8255 | CPC, MSX, X68000 |
| `motorola-mc6850` | ACIA 6850 | Atari ST |
| `motorola-mc68901` | MFP 68901 | Atari ST |
| `motorola-pia-6520` | PIA 6520/6821 | Atari 8-bit |
| `western-digital-wd1770` | WD1770/1772/1793 | Atari ST, BBC Master, Beta 128 |

### Media formats

| Crate | Format | Systems |
|-------|--------|---------|
| `format-hdf` | Amiga Hard Disk File | Amiga |
| `format-d71` | 1571 disk image | C128 |
| `format-d81` | 1581 disk image | C64, C128 |
| `format-t64` | T64 tape archive | C64 |
| `format-fds` | FDS disk image | Famicom Disk System |
| `format-cas` | MSX cassette | MSX |
| `format-rom-msx` | MSX ROM | MSX |
| `format-sms` | SMS/GG ROM | SMS, Game Gear |
| `format-md` | Genesis ROM | Genesis |
| `format-sfc` | SNES ROM | SNES |
| `format-gb` | Game Boy ROM | GB, GBC |
| `format-gba` | GBA ROM | GBA |
| `format-st` | Atari ST disk | Atari ST |
| `format-atr` | Atari 8-bit disk | Atari 8-bit |
| `format-ssd-dsd` | BBC disk | BBC Micro |

---

## Component Reuse Matrix

How many systems benefit from each shared crate:

| Crate | Systems | Count |
|-------|---------|-------|
| `zilog-z80` | Spectrum, ZX81, CPC, MSX, SMS, Game Gear, Genesis, C128, Neo Geo | 9+ |
| `motorola-68000` | Amiga (OCS/ECS), Atari ST, Genesis, Neo Geo, X68000 | 5+ |
| `mos-6502` | Atari 8-bit, BBC Micro, Apple II, 1541 drive | 4 |
| `texas-instruments-sn76489` | BBC Micro, SMS, Game Gear, Genesis | 4 |
| `gi-ay-3-8910` / `yamaha-ym2149` | Spectrum 128+, CPC, MSX, Atari ST | 4+ |
| `intel-ppi-8255` | CPC, MSX, X68000 | 3 |
| `motorola-68030` | A3000, A4000/030, Atari TT/Falcon | 3 |
| `mos-via-6522` | BBC Micro, Apple II, 1541/1571 drives | 2+ |
| `nec-upd765` | CPC, Spectrum +3 | 2 |
| `motorola-mc6845` | CPC, BBC Micro | 2 |

---

## Amiga Vampire / SAGA (Stretch Goal)

FPGA-based accelerator with:

- **Apollo 68080** — 68060-compatible + AMMX (128-bit SIMD), 64-bit registers
- **SAGA chipset** — AGA-superset with chunky display modes, RTG-like
  framebuffer, hardware sprite scaling, 16-bit audio
- **Variants**: V2 (A500/A600/A1200 plug-in), V4 (standalone)

Modelled as `apollo-68080` (extends `motorola-68060`) + `commodore-saga` (AGA
superset) + `card-vampire` expansion.

---

## Roadmap Phases (6–8)

All not started. Depend on primary systems being complete.

### Phase 6: New 8-Bit Systems

1. `machine-cpc` — Amstrad CPC
2. `machine-msx` — MSX family (MSX1 through turboR)
3. `machine-bbc` — BBC Micro
4. `machine-atari8` — Atari 800XL
5. `machine-zx81` — ZX81

### Phase 7: 16-Bit Systems & Consoles

1. `machine-genesis` — Sega Genesis (68000 + Z80)
2. `machine-sms` — Sega Master System
3. `machine-snes` — SNES (65C816 + SPC700)
4. `machine-atarist` — Atari ST
5. `machine-gb` — Game Boy / GBC
6. `machine-pce` — PC Engine

### Phase 8: Advanced Systems

1. `machine-gba` — Game Boy Advance
2. `machine-neogeo` — Neo Geo
3. `machine-x68000` — Sharp X68000
4. Genesis add-ons: Mega CD, 32X
5. Amiga Vampire/SAGA
