# Future Systems

Systems beyond the four primaries (Spectrum, C64, NES, Amiga). **Not in scope**
until all primary systems are complete. Structure code so they remain possible,
but bias new work toward reusable chip crates and family leverage rather than
one-off vanity ports.

For current system status, see [status.md](status.md). For active priorities,
see [roadmap.md](roadmap.md).

---

## Priority Bias

- `High` - strong component reuse, or unlocks multiple sibling systems once the
  first machine works
- `Medium` - clear value, but at least one new CPU, video, or audio stack is
  needed before the family pays off
- `Low` - mostly bespoke hardware, or best deferred until an enabling family is
  already complete

## High-Leverage Families

| Family                   | Systems                                   | Core new work                                 | Why it matters                                        |
| ------------------------ | ----------------------------------------- | --------------------------------------------- | ----------------------------------------------------- |
| TMS9918 Z80 family       | SG-1000/SC-3000, ColecoVision, Adam, MSX1 | TMS9918A packaging, SN76489 family, ROM media | Several low-risk systems share the same video base    |
| UK microcomputer cluster | CPC, BBC Micro, Electron                  | MC6845 timing, Acorn ULA, keyboard glue       | Strong educational cluster with overlapping I/O needs |
| Non-C64 Commodore line   | VIC-20, PET/CBM, Plus/4/C16               | VIC-I or TED, VIA/PIA glue, media quirks      | Extends Commodore coverage beyond the C64             |
| Atari custom-chip line   | Atari 8-bit, 5200, 7800                   | ANTIC/GTIA/POKEY reuse, then Maria            | One family spans both home computers and consoles     |
| 6809 family              | CoCo, Dragon, Vectrex                     | 6809 CPU, MC6847, vector or PIA glue          | Distinctive machines with high teaching value         |
| 68000 desktop machines   | Macintosh, Atari ST, X68000, QL           | Bus glue, video ASICs, storage controllers    | Reuses the existing 68000 core for iconic GUI systems |

## 8-Bit Computers

| System                 | CPU                         | Key Chips                                       | Why It Matters                                          | Priority |
| ---------------------- | --------------------------- | ----------------------------------------------- | ------------------------------------------------------- | -------- |
| Amstrad CPC            | `zilog-z80`                 | MC6845, AY-3-8910, PPI 8255, uPD765             | Strong MC6845 home-computer family and disk workflow    | High     |
| MSX1                   | `zilog-z80`                 | TMS9918A, AY-3-8910, PPI 8255                   | Base system for a large multi-generation family         | High     |
| MSX2                   | `zilog-z80`                 | V9938, AY-3-8910, PPI 8255                      | Natural upgrade once MSX1 and TMS9918A are solid        | High     |
| MSX2+                  | `zilog-z80`                 | V9958, AY-3-8910, PPI 8255, YM2413              | Same family, better video, still high reuse             | High     |
| MSX turboR             | `ascii-r800` + `zilog-z80`  | V9958, YM2149, PPI 8255, YM2413                 | Extends the MSX line with a new CPU but same platform   | Medium   |
| BBC Micro              | `mos-6502`                  | MC6845, SN76489, VIA 6522, FDC family           | High educational value and a clean architecture         | High     |
| Acorn Electron         | `mos-6502`                  | Ferranti ULA, cassette I/O, keyboard matrix     | Lower-cost BBC sibling once the 6502/Acorn path exists  | Medium   |
| Atari 8-bit            | `mos-6502`                  | ANTIC, GTIA/CTIA, POKEY, PIA 6520               | Shares a reusable Atari chipset family                  | High     |
| Apple IIe / IIc        | `mos-6502` / `wdc-65c02`    | Custom video, soft switches, Disk II/IWM        | Historically important, but more bespoke than the BBC   | Medium   |
| Commodore VIC-20       | `mos-6502`                  | VIC-I, VIA 6522                                 | Low-cost Commodore-adjacent expansion target            | Medium   |
| Commodore PET / CBM    | `mos-6502`                  | VIA 6522, PIA 6520/6821, discrete or CRTC video | Early Commodore line with keyboard and storage variety  | Low      |
| Commodore Plus/4 / C16 | `mos-8501`                  | TED, keyboard and cassette glue                 | Extends the Commodore family but needs TED and CPU work | Medium   |
| Coleco Adam            | `zilog-z80`                 | TMS9928A, SN76489, WD2793, keyboard glue        | Computer follow-on once ColecoVision exists             | Medium   |
| Oric-1 / Atmos         | `mos-6502`                  | ULA, AY-3-8912                                  | Compact 6502 plus AY machine with tape-first workflow   | Low      |
| Tandy CoCo / Dragon    | `motorola-6809`             | MC6847, SAM 6883, PIA 6821                      | Introduces the 6809 family and unlocks Dragon siblings  | Medium   |
| TI-99/4A               | `texas-instruments-tms9900` | TMS9918A, TMS9919, 9901                         | Reuses TMS9918A-era video ideas but needs an odd CPU    | Low      |
| SAM Coupe              | `zilog-z80`                 | Custom ASIC, SAA1099                            | Spectrum-adjacent, but mostly bespoke hardware          | Low      |
| ZX81                   | `zilog-z80`                 | Ferranti ULA ZX81                               | Small and historically useful, but a one-off            | Low      |

## 16-Bit Computers

| System                           | CPU                     | Key Chips                                   | Why It Matters                                       | Priority |
| -------------------------------- | ----------------------- | ------------------------------------------- | ---------------------------------------------------- | -------- |
| Atari ST / STE                   | `motorola-68000`        | YM2149, MC6850, MC68901, WD1772, Shifter    | Strong 68000 desktop target with clear family reuse  | High     |
| Classic Macintosh (128K to Plus) | `motorola-68000`        | VIA 6522, Z8530 SCC, IWM, compact-Mac video | High-value GUI system on the existing 68000 core     | High     |
| Apple IIGS                       | `wdc-65c816`            | Mega II, Ensoniq DOC, ADB, IWM              | Bridges Apple II and 16-bit GUI-era systems          | Medium   |
| Sinclair QL                      | `motorola-68008`        | IPC 8049, Microdrive, custom video          | Distinctive 68000-family workstation with bus quirks | Low      |
| Sharp X68000                     | `motorola-68000`        | YM2151, OKI MSM6258, custom video, SCC      | Strong arcade-adjacent 68000 machine                 | Medium   |
| Acorn Archimedes                 | `arm-arm2` / `arm-arm3` | IOC, MEMC, VIDC                             | Introduces the early ARM family and RISC path        | Low      |
| Atari TT                         | `motorola-68030`        | TT Shifter, SCSI, VME glue                  | Natural follow-on only after ST is mature            | Low      |
| Atari Falcon                     | `motorola-68030`        | VIDEL, DSP56001, crossbar                   | Valuable but much more bespoke than ST               | Low      |

## Consoles And Handhelds

| System                    | CPU(s)                         | Key Chips                            | Why It Matters                                            | Priority |
| ------------------------- | ------------------------------ | ------------------------------------ | --------------------------------------------------------- | -------- |
| SG-1000 / SC-3000 family  | `zilog-z80`                    | TMS9918A, SN76489                    | Cheapest Sega on-ramp and another TMS9918 validation path | High     |
| ColecoVision              | `zilog-z80`                    | TMS9928A, SN76489                    | Strong cartridge console with high TMS9918 reuse          | High     |
| Atari 2600                | `mos-6507`                     | TIA, RIOT 6532                       | Historically important but highly bespoke timing model    | Low      |
| Atari 5200                | `mos-6502`                     | ANTIC, GTIA, POKEY                   | Close Atari 8-bit sibling with controller differences     | Medium   |
| Atari 7800                | `mos-6502`                     | Maria, TIA, RIOT 6532                | Atari-adjacent follow-on once the custom-chip line exists | Medium   |
| Vectrex                   | `motorola-6809`                | AY-3-8912, VIA 6522, vector hardware | Unique vector display target and 6809 validation system   | Medium   |
| Sega Master System        | `zilog-z80`                    | Sega VDP 315-5124, SN76489           | Natural Sega family step after SG-1000                    | High     |
| Game Gear                 | `zilog-z80`                    | Sega VDP viewport variant, SN76489   | SMS sibling with handheld timing and LCD constraints      | High     |
| Sega Genesis              | `motorola-68000` + `zilog-z80` | Genesis VDP, YM2612, SN76489         | High-leverage console family and 68000 showcase           | High     |
| SNES                      | `wdc-65c816` + `sony-spc700`   | SNES PPU, DSP, SPC700                | Major console milestone with bespoke but important chips  | High     |
| Game Boy / GBC            | `sharp-sm83`                   | GB PPU, GB APU                       | Self-contained handheld family with broad software value  | High     |
| Atari Lynx                | `wdc-65c02`                    | Suzy, Mikey                          | Portable with unusual sprite and blitter hardware         | Low      |
| PC Engine / TurboGrafx-16 | `hudson-huc6280`               | HuC6270 VDC, HuC6260 VCE             | Strong 8/16-bit console with compact chip count           | Medium   |
| Neo Geo                   | `motorola-68000` + `zilog-z80` | LSPC, YM2610                         | Large software value but heavy media and bandwidth cost   | Medium   |
| Game Boy Advance          | `arm-arm7tdmi`                 | GBA PPU, DMA, timers                 | Major step up from GB/GBC without a new audio co-CPU      | Medium   |
| Mega CD                   | `motorola-68000` x 2           | RF5C164, ASIC MCD                    | Add-on complexity best attempted after Genesis is stable  | Low      |
| 32X                       | `hitachi-sh2` x 2              | 32X VDP                              | Add-on complexity best attempted after Genesis is stable  | Low      |

---

## CPUs Needed

| Crate                       | CPU            | Used By                    | Priority | Notes                                                |
| --------------------------- | -------------- | -------------------------- | -------- | ---------------------------------------------------- |
| `wdc-65c02`                 | 65C02 / 65SC02 | Apple IIc, Atari Lynx      | Medium   | Useful for Apple and portable follow-on systems      |
| `wdc-65c816`                | 65C816         | SNES, Apple IIGS, SuperCPU | High     | Major family unlock                                  |
| `mos-8502`                  | 8502           | C128                       | Medium   | C64-adjacent CPU variant                             |
| `mos-8501`                  | 7501 / 8501    | Plus/4, C16                | Medium   | Needed for the TED family                            |
| `mos-6507`                  | 6507           | Atari 2600                 | Low      | Reduced-pin 6502 derivative                          |
| `sharp-sm83`                | SM83           | Game Boy, GBC              | High     | Self-contained handheld family                       |
| `ascii-r800`                | R800           | MSX turboR                 | Medium   | MSX-family extension                                 |
| `motorola-68008`            | 68008          | Sinclair QL                | Low      | Likely a wrapper over the 68000 core with bus quirks |
| `motorola-6809`             | 6809E          | CoCo, Dragon, Vectrex      | High     | Strong teaching ISA with multiple targets            |
| `sony-spc700`               | SPC700         | SNES audio                 | High     | Required for SNES                                    |
| `hitachi-sh2`               | SH-2           | 32X                        | Low      | Mostly useful after Genesis add-on work              |
| `arm-arm2`                  | ARM2 / ARM3    | Archimedes                 | Low      | RISC-era stretch target                              |
| `arm-arm7tdmi`              | ARM7TDMI       | GBA                        | Medium   | Major portable step-up                               |
| `hudson-huc6280`            | HuC6280        | PC Engine                  | Medium   | Compact, high-value console CPU                      |
| `texas-instruments-tms9900` | TMS9900        | TI-99/4A                   | Low      | Interesting but unusual CPU family                   |

---

## Existing-System Variants (Not Started)

Variants and expansions for the four primary systems that are still outside the
current scope.

### Spectrum

| Variant           | Notes                                                    |
| ----------------- | -------------------------------------------------------- |
| Scorpion ZS-256   | Different memory mapping, turbo modes                    |
| Pentagon 128      | Popular clone family with timing and paging differences  |
| Timex 2068        | SCLD extended video modes, different AY ports, cartridge |
| Interface 1       | Microdrive, RS-232, ZX Net, shadow ROM                   |
| DivIDE / DivMMC   | IDE/SD, shadow ROM paged on M1 traps                     |
| Multiface         | NMI button, shadow ROM/RAM                               |
| Beta 128 (TR-DOS) | WD1793 FDC, up to 4 drives                               |

### Commodore 64

| Variant          | Notes                                 |
| ---------------- | ------------------------------------- |
| SX-64            | No Datasette, built-in 1541, 60Hz TOD |
| C64 GS           | Cartridge-only, no keyboard           |
| PAL-N (6572)     | 65 cycles/line, 312 lines             |
| 1581 drive       | 3.5" DD, WD1770 + CIA 8520            |
| SwiftLink        | ACIA 6551, RS-232 up to 38.4 kbps     |
| 1351 Mouse       | Proportional mouse via SID pots       |
| CMD SuperCPU     | 65816 @ 20 MHz, 16MB RAM              |
| GeoRAM / RAMLink | Alternative memory expansions         |

### Commodore 128

Not started. Requires `mos-8502`, `commodore-vdc-8563` / `8568`, Z80
co-processor mode, and dual 40/80-column display handling.

### NES

| Variant                 | Notes                                                              |
| ----------------------- | ------------------------------------------------------------------ |
| Famicom Disk System     | `nintendo-fds-2c33`, 32KB RAM, wavetable audio, QD drive           |
| VS. System              | Scrambled palettes, coin-op hardware, DIP switches                 |
| PlayChoice-10           | Dual-screen arcade derivative, timer logic, cartridge bank changes |
| Expansion audio mappers | VRC6 (24/26), VRC7 (85), Namco 163 (19), Sunsoft 5B (69), MMC5 (5) |

Each expansion audio mapper contains a sound chip that could be its own crate:
`konami-vrc6-audio` (2 pulse + 1 sawtooth), `konami-vrc7-audio` (YM2413
derivative, 6-channel FM), `namco-163-audio` (8-channel wavetable),
`sunsoft-5b-audio` (YM2149 variant, 3-channel PSG).

### Amiga

| Variant           | Notes                                                          |
| ----------------- | -------------------------------------------------------------- |
| A1000 bootstrap   | Writable Kickstart bootstrap path and early memory-map quirks  |
| CDTV              | OCS/ECS-derived CD system with remote, NVRAM, and CD-ROM paths |
| CD32              | AGA + Akiko + CD-ROM; gamepad-first console profile            |
| A4091 / Fast SCSI | Storage/controller expansion work after A3000/A4000 maturity   |

---

## Shared Chip Crates

Chips and formats that matter for future systems. `Existing` means a related
crate already exists in the current codebase; `Needed` means the crate is still
future work.

### Sound

| Crate                       | Chip                     | Systems                                           | Status   |
| --------------------------- | ------------------------ | ------------------------------------------------- | -------- |
| `gi-ay-3-8910`              | AY-3-8910 / 8912         | CPC, MSX, Oric, Vectrex, Spectrum 128+            | Existing |
| `texas-instruments-sn76489` | SN76489 / SN94624 / 9919 | BBC, SG-1000, ColecoVision, SMS, Game Gear, TI-99 | Needed   |
| `yamaha-ym2149`             | YM2149F                  | MSX turboR, Atari ST                              | Needed   |
| `yamaha-ym2612`             | YM2612 (OPN2)            | Genesis                                           | Needed   |
| `yamaha-ym2610`             | YM2610 (OPNB)            | Neo Geo                                           | Needed   |
| `yamaha-ym2151`             | YM2151 (OPM)             | X68000                                            | Needed   |
| `yamaha-ym2413`             | YM2413 (OPLL)            | MSX2+, MSX turboR, Master System (JP)             | Needed   |
| `atari-pokey`               | POKEY                    | Atari 8-bit, 5200                                 | Needed   |
| `commodore-ted-7360`        | TED audio                | Plus/4, C16                                       | Needed   |
| `ensoniq-doc-5503`          | Ensoniq DOC              | Apple IIGS                                        | Needed   |
| `philips-saa1099`           | SAA1099                  | SAM Coupe                                         | Needed   |

### Video

| Crate                        | Chip                | Systems                                     | Status |
| ---------------------------- | ------------------- | ------------------------------------------- | ------ |
| `texas-instruments-tms9918a` | TMS9918A / TMS9928A | SG-1000, ColecoVision, Adam, MSX1, TI-99/4A | Needed |
| `yamaha-v9938`               | V9938               | MSX2                                        | Needed |
| `yamaha-v9958`               | V9958               | MSX2+, MSX turboR                           | Needed |
| `sega-315-5124`              | Sega VDP            | SMS, Game Gear                              | Needed |
| `motorola-mc6845`            | MC6845 CRTC         | CPC, BBC Micro                              | Needed |
| `atari-antic`                | ANTIC               | Atari 8-bit, 5200                           | Needed |
| `atari-gtia`                 | GTIA / CTIA         | Atari 8-bit, 5200                           | Needed |
| `commodore-vic-6560`         | VIC-I               | VIC-20                                      | Needed |
| `commodore-vdc-8563`         | VDC 8563 / 8568     | C128                                        | Needed |
| `motorola-mc6847`            | MC6847 VDG          | CoCo, Dragon                                | Needed |
| `atari-maria`                | Maria               | Atari 7800                                  | Needed |
| `ferranti-ula-zx81`          | ZX81 ULA            | ZX81                                        | Needed |

### Support

| Crate                    | Chip                 | Systems                                | Status   |
| ------------------------ | -------------------- | -------------------------------------- | -------- |
| `intel-ppi-8255`         | PPI 8255             | CPC, MSX, X68000                       | Needed   |
| `motorola-mc6850`        | ACIA 6850            | Atari ST                               | Needed   |
| `motorola-mc68901`       | MFP 68901            | Atari ST                               | Needed   |
| `motorola-pia-6520`      | PIA 6520 / 6821      | Atari 8-bit, CoCo, Dragon, PET         | Needed   |
| `mos-via-6522`           | VIA 6522             | Apple II, VIC-20, PET, Macintosh, 1541 | Existing |
| `mos-riot-6532`          | RIOT 6532            | Atari 2600, Atari 7800                 | Needed   |
| `motorola-6883-sam`      | SAM 6883             | CoCo, Dragon                           | Needed   |
| `western-digital-wd1770` | WD1770 / 1772 / 1793 | Atari ST, BBC Master, C128             | Needed   |
| `western-digital-wd2793` | WD2793               | Coleco Adam, MSX disk systems          | Needed   |
| `zilog-z8530`            | SCC / ESCC           | Macintosh, X68000                      | Needed   |
| `apple-iwm`              | IWM / SWIM           | Apple II, Macintosh, IIGS              | Needed   |

### Media Formats

| Crate                                      | Format                 | Systems                  | Status |
| ------------------------------------------ | ---------------------- | ------------------------ | ------ |
| `format-amstrad-cpc-dsk`                   | CPC DSK / Extended DSK | Amstrad CPC              | Needed |
| `format-amstrad-cpc-cdt`                   | CPC CDT tape image     | Amstrad CPC              | Needed |
| `format-msx-dsk`                           | MSX DSK disk image     | MSX                      | Needed |
| `format-msx-rom`                           | MSX ROM                | MSX                      | Needed |
| `format-msx-cas`                           | CAS cassette image     | MSX                      | Needed |
| `format-acorn-uef`                         | UEF cassette image     | BBC Micro, Electron      | Needed |
| `format-acorn-bbc-ssd-dsd`                 | SSD / DSD disk image   | BBC Micro                | Needed |
| `format-atari-8-bit-atr`                   | ATR disk image         | Atari 8-bit              | Needed |
| `format-atari-8-bit-xex`                   | XEX executable         | Atari 8-bit              | Needed |
| `format-apple-ii-woz`                      | WOZ nibble disk image  | Apple II                 | Needed |
| `format-apple-ii-2mg`                      | 2MG / 2IMG disk image  | Apple II, IIGS           | Needed |
| `format-tandy-color-computer-cas`          | CAS cassette image     | CoCo, Dragon             | Needed |
| `format-atari-5200-rom`                    | Atari 5200 ROM         | Atari 5200               | Needed |
| `format-atari-7800-rom`                    | Atari 7800 ROM         | Atari 7800               | Needed |
| `format-sega-master-system-game-gear-rom`  | SMS / GG ROM           | Master System, Game Gear | Needed |
| `format-sega-genesis-rom`                  | Genesis ROM            | Genesis                  | Needed |
| `format-nintendo-snes-rom`                 | SNES ROM               | SNES                     | Needed |
| `format-nintendo-famicom-disk-system`      | FDS disk image         | Famicom Disk System      | Needed |
| `format-nintendo-game-boy-rom`             | Game Boy ROM           | Game Boy, GBC            | Needed |
| `format-nintendo-game-boy-advance-rom`     | GBA ROM                | GBA                      | Needed |
| `format-commodore-amiga-hdf`               | Amiga hard disk image  | Amiga                    | Needed |
| `format-commodore-1571-d71`                | 1571 disk image        | C128                     | Needed |
| `format-commodore-1581-d81`                | 1581 disk image        | C64, C128                | Needed |

---

## Component Reuse Matrix

Approximate family leverage across current and candidate systems:

| Crate / Family                   | Candidate Systems                                                           | Count |
| -------------------------------- | --------------------------------------------------------------------------- | ----- |
| `zilog-z80`                      | Spectrum, ZX81, CPC, MSX, SG-1000/SC-3000, ColecoVision/Adam, SMS, GG, more | 10+   |
| `motorola-68000`                 | Amiga, Macintosh, Atari ST, QL, Genesis, Neo Geo, X68000                    | 7+    |
| `mos-6502` family                | BBC, Electron, Atari 8-bit, 5200, Apple II, VIC-20, PET, 7800               | 8+    |
| `texas-instruments-tms9918a`     | SG-1000, ColecoVision, Adam, MSX1, TI-99/4A                                 | 5+    |
| `texas-instruments-sn76489`      | BBC, SG-1000, ColecoVision, SMS, Game Gear, TI-99/4A                        | 6+    |
| `gi-ay-3-8910` / `yamaha-ym2149` | Spectrum 128+, CPC, MSX, Oric, Vectrex, Atari ST                            | 6+    |
| `motorola-6809`                  | CoCo, Dragon, Vectrex                                                       | 3     |
| `wdc-65c816`                     | SNES, Apple IIGS, SuperCPU                                                  | 3     |
| `mos-via-6522`                   | Apple II, VIC-20, PET, Macintosh, 1541                                      | 5+    |
| `atari-antic` / `atari-gtia`     | Atari 8-bit, 5200                                                           | 2     |
| `motorola-mc6845`                | CPC, BBC Micro                                                              | 2     |
| `motorola-mc6847`                | CoCo, Dragon                                                                | 2     |

---

## Amiga Vampire / SAGA (Stretch Goal)

FPGA-based accelerator with:

- **Apollo 68080** - 68060-compatible + AMMX (128-bit SIMD), 64-bit registers
- **SAGA chipset** - AGA-superset with chunky display modes, RTG-like
  framebuffer, hardware sprite scaling, 16-bit audio
- **Variants**: V2 (A500/A600/A1200 plug-in), V4 (standalone)

Modelled as `apollo-68080` (extends `motorola-68060`) + `commodore-saga` (AGA
superset) + `card-vampire` expansion.

---

## Possible Post-Core Sequence

These are not active roadmap items. They are a rough expansion order to revisit
after the four primary systems are complete.

### Family Foundations

1. `machine-sega-sg-1000` / `machine-sega-sc-3000`
2. `machine-coleco-colecovision`
3. `machine-amstrad-cpc`
4. `machine-msx-1`
5. `machine-acorn-bbc-micro` / `machine-acorn-electron`
6. `machine-atari-8-bit` / `machine-atari-5200`
7. `machine-commodore-vic-20` / `machine-commodore-pet`
8. `machine-tandy-color-computer` / `machine-dragon-32`
9. `machine-sinclair-zx81`

### Major 16-Bit And Console Families

1. `machine-sega-master-system` / `machine-sega-game-gear`
2. `machine-sega-genesis`
3. `machine-nintendo-snes`
4. `machine-nintendo-game-boy`
5. `machine-atari-st`
6. `machine-apple-macintosh-128k`
7. `machine-nec-pc-engine`
8. `machine-apple-ii-gs`

### Advanced Or Bespoke Systems

1. `machine-sharp-x68000`
2. `machine-snk-neo-geo`
3. `machine-nintendo-game-boy-advance`
4. `machine-acorn-archimedes`
5. `machine-atari-7800`
6. `machine-atari-lynx`
7. Genesis add-ons: Mega CD, 32X
8. Amiga Vampire / SAGA

Low-priority one-offs fit after the nearest family foundation is complete:
`machine-texas-instruments-ti-99-4a`, `machine-oric-atmos`,
`machine-mgt-sam-coupe`, `machine-atari-2600`, and `machine-sinclair-ql`.
