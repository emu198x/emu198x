# Emu198x Reference Catalogue

This is the living catalogue of reference material for emu198x development. Each entry corresponds to a reference in `refs/manifest.toml`. Entries marked ✅ are acquired and cached on disk. Entries marked ⬜ are identified but not yet acquired.

This file is committed to the repository. The actual PDF/document files in `refs/` are `.gitignore`d (copyrighted material is not redistributable).

---

## CPU references

### Z80

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `z80-user-manual` | Z80 CPU User Manual (UM0080) | Zilog | 2016 | Instruction set, timing, interrupts, bus cycles | Official Zilog manual, revision 11. Freely available from Zilog. |
| ⬜ | `z80-undocumented` | The Undocumented Z80 Documented | Sean Young | 2005 | Undocumented instructions, flags, MEMPTR | Community standard reference for undocumented behaviour. |
| ⬜ | `z80-memptr` | MEMPTR (WZ register) Investigation | Boo-Hoo, Ets | 2006 | MEMPTR/WZ register behaviour | Based on real hardware testing. Corrections to earlier docs. |
| ⬜ | `z80-timing` | Z80 Instruction Timing | various | — | Per-instruction cycle counts, M-cycle breakdown | Multiple sources — cross-reference. |
| ⬜ | `z80n-instructions` | Z80N Extended Instruction Set | Spectrum Next team | 2020+ | Z80N new instructions (MUL, LDIX, etc.) | For Spectrum Next support. |

### 6502

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `6502-datasheet` | MOS 6502 Microprocessor Datasheet | MOS Technology | 1975 | Pinout, timing, electrical specs | Original datasheet. |
| ⬜ | `6502-reference` | MOS 6502 Programming Manual | MOS Technology | 1976 | Instruction set, addressing modes | Official programming reference. |
| ⬜ | `6502-cycle-timing` | 6502 Cycle-by-Cycle Operation | — | — | Per-cycle bus activity for every instruction | Essential for cycle-accurate implementation. |
| ⬜ | `65c02-datasheet` | W65C02S Datasheet | WDC | 2018 | CMOS variant differences, new instructions | For systems using CMOS 6502 variant. |
| ⬜ | `65c816-manual` | W65C816S Programming Manual | WDC | 2018 | 16-bit extensions, bank addressing | For SNES, Apple IIGS, SuperCPU. |

### 68000

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `m68000-users-manual` | MC68000 User's Manual | Motorola | 1993 | Instruction set, addressing modes, timing | The primary 68000 reference. |
| ⬜ | `m68000-8-16-32` | M68000 8/16/32-bit Reference | Motorola | 1990 | Instruction timing tables, exception processing | Detailed timing for every instruction. |
| ⬜ | `m68020-users-manual` | MC68020 User's Manual | Motorola | 1990 | Cache, addressing extensions | For Amiga accelerators. |
| ⬜ | `m68040-users-manual` | MC68040 User's Manual | Motorola | 1993 | Integrated FPU/MMU, pipelines | For Amiga accelerators. |
| ⬜ | `m68060-users-manual` | MC68060 User's Manual | Motorola | 1994 | Superscalar pipeline, branch prediction | For Amiga accelerators. |

### 6809

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ✅ | `mc6809-programming-manual` | MC6809/MC6809E 8-Bit Microprocessor Programming Manual | Motorola | 1981 | Instruction semantics, addressing modes, programmer-visible state | Extracted at `docs/source-extracts/dragon-primary/mc6809-mc6809e-programming-manual-1981.txt`. |
| ✅ | `mc6809e-hmos-datasheet` | MC6809E HMOS 8-Bit Microprocessor | Motorola | 1984 | MC6809E bus signals, electrical characteristics, instruction timing tables | Extracted at `docs/source-extracts/dragon-primary/mc6809e-hmos-microprocessor-1984.txt`. |
| ✅ | `motorola-microprocessors-data-manual` | Motorola Microprocessors Data Manual | Motorola | — | Motorola CPU and support-chip data | Large cross-check extract at `docs/source-extracts/dragon-primary/motorola-microprocessors-data-manual.txt`. |

---

## System references

### Sinclair ZX Spectrum

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `spectrum-ula-book` | The ZX Spectrum ULA: How to Design a Microcomputer | Chris Smith | 2010 | ULA, contention, video timing, memory contention, I/O contention | **The** definitive ULA reference. Based on die photography. Purchase required. |
| ⬜ | `spectrum-contention` | Spectrum Contention Timing | Ramsoft | — | Contention delay tables | Complements the ULA book. |
| ⬜ | `spectrum-floating-bus` | Spectrum Floating Bus Behaviour | various | — | Data bus decay, ULA read patterns | Important for demos and some protections. |
| ⬜ | `spectrum-rom-disassembly` | The Complete Spectrum ROM Disassembly | Ian Logan, Frank O'Hara | 1983 | ROM routines, tape loading, BASIC interpreter | For understanding ROM-level tape loading. |
| ⬜ | `spectrum-next-dev-guide` | ZX Spectrum Next Developer Guide | Next team | 2020+ | Next hardware registers, sprites, tilemap, copper, DMA | For Next variant support. |
| ⬜ | `if1-technical` | Interface 1 Technical Manual | Sinclair | 1983 | Microdrive, RS-232, ZX Net | For Interface 1 extension. |
| ⬜ | `zx-printer-protocol` | ZX Printer Protocol | various | — | Printer communication protocol | For ZX Printer emulation. |
| ⬜ | `divmmc-technical` | DivMMC Technical Reference | various | — | Auto-paging, memory mapping, SD protocol | For DivMMC extension. |

### Commodore C64

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `c64-programmers-ref` | Commodore 64 Programmer's Reference Guide | Commodore | 1982 | System overview, BASIC, KERNAL, hardware | Official reference. Freely available. |
| ⬜ | `vic-ii-datasheet` | MOS 6567/6569 VIC-II Datasheet | MOS Technology | 1982 | Video registers, sprites, timing | Original datasheet. |
| ⬜ | `vic-ii-exposed` | VIC-II Exposed (The MOS 6567/6569 VIC-II) | Christian Bauer | — | Detailed VIC-II behaviour, raster timing, badlines | Community reference. Very detailed. |
| ⬜ | `sid-6581-datasheet` | MOS 6581 SID Datasheet | MOS Technology | 1982 | Audio registers, waveforms, filter, envelope | Original datasheet. Filter specs are nominal. |
| ⬜ | `sid-8580-datasheet` | MOS 8580 SID Datasheet | MOS Technology | 1986 | Revised SID, different filter, different waveform mixing | Important for variant emulation. |
| ⬜ | `sid-internals` | SID Internals / reSID Analysis | Dag Lem | — | Transistor-level filter model, waveform generation | The basis for reSID. Gold standard for SID accuracy. |
| ⬜ | `sid-6581-die` | MOS 6581 Die Analysis | — | — | Die photography, transistor-level reverse engineering | For resolving ambiguities in datasheets. |
| ⬜ | `c64-cia-datasheet` | MOS 6526 CIA Datasheet | MOS Technology | 1981 | Timers, I/O ports, serial, interrupts | For CIA emulation. |
| ⬜ | `iec-protocol` | IEC Bus Protocol | various | — | Serial bus protocol for 1541/printer | For disk drive and printer emulation. |

### Nintendo NES / Famicom

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `nesdev-wiki` | NESDev Wiki (offline snapshot) | NESDev community | 2025 | PPU, APU, mappers, timing, bus conflicts | The most comprehensive NES reference. Snapshot periodically. |
| ⬜ | `ppu-2c02-ref` | RP2C02 PPU Reference | various | — | PPU registers, rendering pipeline, sprite evaluation | Multiple sources compiled. |
| ⬜ | `apu-reference` | NES APU Reference | NESDev | — | Audio channels, sweep, length counter, DMC | Community documentation. |
| ⬜ | `nes-mapper-docs` | NES Mapper Documentation | NESDev | — | Per-mapper register descriptions, bank switching | Community-maintained, per-mapper files. |
| ⬜ | `fds-technical` | Famicom Disk System Technical Reference | various | — | RAM adapter, disk format, wavetable audio, IRQ timer | For FDS support. |

### Commodore Amiga

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `amiga-hw-ref` | Amiga Hardware Reference Manual | Commodore | 1991 | Custom chips, DMA, copper, blitter, audio | The primary Amiga hardware reference. |
| ⬜ | `amiga-rom-kernel-ref` | Amiga ROM Kernel Reference Manual | Commodore | 1991 | Libraries, devices, Exec, Intuition | For OS-level behaviour. |
| ⬜ | `agnus-register-map` | Agnus/Alice Register Map | various | — | DMA controller registers, memory addressing | Custom chip documentation. |
| ⬜ | `denise-register-map` | Denise/Lisa Register Map | various | — | Video registers, sprites, collision | Custom chip documentation. |
| ⬜ | `paula-audio` | Paula Audio DMA Reference | various | — | Audio channel registers, DMA, period, volume | For audio emulation. |
| ⬜ | `aga-differences` | AGA vs OCS/ECS Differences | various | — | Enhanced registers, 256 colours, HAM8 | For A1200/A4000/CD32 support. |
| ⬜ | `amiga-floppy` | Amiga Floppy Disk Format and Timing | various | — | MFM encoding, track timing, DMA | For disk support. |

### Dragon / CoCo

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ✅ | `mc6847-vdg-datasheet` | MC6847 MOS Video Display Generator | Motorola | 1984 | VDG modes, colours, sync timing, memory access timing | Extracted at `docs/source-extracts/dragon-primary/mc6847-video-display-generator-1984.txt`. |
| ✅ | `mc6883-sam-advance-sheet` | MC6883/SN74LS783 Synchronous Address Multiplexer Advance Sheet | Motorola | — | SAM clocks, VDG/MPU arbitration, memory mapping, device selects | Extracted at `docs/source-extracts/dragon-primary/mc6883-sam-advance-sheet.txt`. |
| ✅ | `sam-programming-guide` | Synchronous Address Multiplexer Programming Guide | — | — | SAM register programming, VDG address modes, MPU rate | Extracted at `docs/source-extracts/dragon-primary/sam-programming-guide.txt`; OCR is noisy but readable. |

### Acorn BBC Micro

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `bbc-aug` | BBC Micro Advanced User Guide | Bray, Dickens, Holmes | 1983 | Hardware details, OS calls, video, sound | The primary BBC Micro reference. |
| ⬜ | `econet-spec` | Econet System Specification | Acorn | 1985 | Econet protocol, frame format, station addressing | For Econet emulation. |
| ⬜ | `tube-spec` | Tube Interface Specification | Acorn | 1984 | Second processor protocol, FIFO registers | For second processor support. |
| ⬜ | `mc68b54-datasheet` | MC68B54 ADLC Datasheet | Motorola | 1985 | HDLC/SDLC framing chip | For Econet ADLC emulation. |
| ⬜ | `mc6845-datasheet` | MC6845 CRTC Datasheet | Motorola | 1981 | CRT controller registers, timing | For video timing. |

### Amstrad CPC

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `cpc-firmware-guide` | Amstrad CPC Firmware Guide | Soft968 / Locomotive | 1985 | Firmware calls, hardware registers | Official documentation. |
| ⬜ | `cpc-gate-array` | CPC Gate Array Technical Reference | various | — | Video modes, palette, ROM banking, interrupt generation | Community reverse-engineering. |
| ⬜ | `cpc-crtc-differences` | CRTC Differences Across CPC Models | various | — | MC6845 vs HD6845 vs custom variants | Important for compatibility. |

---

## Chip references

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `ay-3-8910-datasheet` | AY-3-8910 Datasheet | General Instrument | 1979 | PSG registers, tone/noise/envelope | For Spectrum 128K, CPC, MSX, Atari ST. |
| ⬜ | `ym2149-datasheet` | YM2149 Datasheet | Yamaha | 1983 | AY-compatible, different DAC curve | Atari ST uses YM2149. Logarithmic vs linear DAC output. |
| ⬜ | `wd1770-datasheet` | WD1770 FDC Datasheet | Western Digital | 1983 | Floppy disk controller registers, command set | For Spectrum +3, CPC, Atari ST. |
| ⬜ | `wd1793-datasheet` | WD1793 FDC Datasheet | Western Digital | 1980 | Floppy disk controller | For TR-DOS (Pentagon, Scorpion). |
| ⬜ | `ne2000-datasheet` | NE2000 Ethernet Controller | Novell/National Semi | 1990s | Ethernet MAC registers, DMA | For Amiga/C64 Ethernet cards. |
| ⬜ | `cs8900a-datasheet` | CS8900A Ethernet Controller | Cirrus Logic | 1990s | Ethernet controller with ISA bus | For Apple II Uthernet, C64 RR-Net. |
| ⬜ | `z80-pio-datasheet` | Z80 PIO Datasheet | Zilog | 1976 | Parallel I/O | For systems using Z80 PIO. |
| ⬜ | `z80-sio-datasheet` | Z80 SIO Datasheet | Zilog | 1976 | Serial I/O | For systems using Z80 SIO. |
| ✅ | `mc6821-pia-datasheet` | MC6821 NMOS Peripheral Interface Adapter | Motorola | 1985 | DDR/data registers, control registers, CA/CB lines, interrupts | Extracted at `docs/source-extracts/dragon-primary/mc6821-pia-1985.txt`. |

---

## Format specifications

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `tzx-spec` | TZX Format Specification v1.20 | Tomaz Kac | 2006 | All TZX block types, control flow, timing | Primary tape format for Spectrum. |
| ⬜ | `pzx-spec` | PZX Format Specification | Patrik Rak | 2007 | Pulse-based tape format | Alternative to TZX. |
| ⬜ | `tap-format` | TAP Format Description | various | — | Simple tape block format | Simplest Spectrum tape format. |
| ⬜ | `csw-spec` | CSW Compressed Square Wave Specification | various | — | Compressed sampled tape data | Sampled tape format. |
| ⬜ | `z80-snapshot-format` | Z80 Snapshot File Format | various | — | v1, v2, v3 header formats, compression | Multiple versions with different headers. |
| ⬜ | `sna-format` | SNA Snapshot Format | various | — | Fixed-layout 48K/128K snapshot | Simple snapshot format. |
| ⬜ | `szx-format` | SZX (Zx-State) Format Specification | Jonathan Mayner | 2004 | Modern extensible snapshot format | Block-based, supports many extensions. |
| ⬜ | `ines-format` | iNES Header Format | various | — | NES ROM header, mapper number | De facto NES ROM standard. Header errors common. |
| ⬜ | `nes20-format` | NES 2.0 Header Format | various | — | Extended header with better mapper support | Successor to iNES. |
| ⬜ | `fds-format` | FDS Disk Image Format | various | — | Optional header, block structure | Famicom Disk System images. |
| ⬜ | `cue-sheet-syntax` | CUE Sheet Syntax | various | — | Track definitions, index points, file references | CD-ROM image descriptor format. |
| ⬜ | `chd-format` | CHD (Compressed Hunks of Data) | MAME team | — | Compressed disc/hard disk image | MAME's archival format. |
| ⬜ | `adf-format` | ADF (Amiga Disk File) | various | — | Flat sector dump, 880KB | Standard Amiga floppy image. |
| ⬜ | `d64-format` | D64 (Commodore Disk Image) | various | — | GCR track layout, variable sectors per track | Standard C64 disk image. |
| ⬜ | `ipf-format` | IPF (Interchangeable Preservation Format) | SPS/CAPS | — | Flux-level disk image | For copy-protected Amiga/Atari ST software. |

---

## Test suites and validation

| Status | ID | Title | Author | Topics | Notes |
|--------|-----|-------|--------|--------|-------|
| ⬜ | `fuse-tests` | FUSE Z80 Test Suite | FUSE team | Z80 instruction behaviour, flags, timing | The standard Z80 CPU test. |
| ⬜ | `z80-tests-rak` | Patrik Rak's Z80 Tests | Patrik Rak | Undocumented Z80 behaviour | Extended Z80 testing. |
| ⬜ | `blargg-nes-tests` | Blargg's NES Test ROMs | Blargg | PPU timing, APU, CPU timing, interrupts | Standard NES validation suite. |
| ⬜ | `tom-harte-tests` | Tom Harte's Processor Tests | Tom Harte | Per-instruction cycle-accurate tests | 10,000 test vectors per opcode. Z80, 6502, 68000. |
| ⬜ | `dormann-6502` | Klaus Dormann's 6502 Test Suite | Klaus Dormann | Functional and decimal mode tests | Comprehensive 6502 functional test. |
| ⬜ | `lorenz-tests` | Lorenz C64 Test Suite | Wolfgang Lorenz | VIC-II, CIA, CPU timing | Standard C64 validation suite. |
| ⬜ | `acid-tests` | ACID800 / ACID2 | various | VIC-II, ANTIC/GTIA | System-specific accuracy tests. |

---

## Die photography and analysis

| Status | ID | Title | Author | Year | Topics | Notes |
|--------|-----|-------|--------|------|--------|-------|
| ⬜ | `spectrum-ula-die` | Spectrum ULA Die Photography Analysis | Chris Smith et al. | 2010 | Transistor-level ULA operation | Basis for the ULA book. |
| ⬜ | `sid-6581-die` | MOS 6581 SID Die Analysis | — | — | Transistor-level filter, waveform, envelope | Resolves datasheet ambiguities. |
| ⬜ | `vic-ii-die` | MOS 6567/6569 VIC-II Die Analysis | — | — | Transistor-level rendering pipeline | Resolves timing edge cases. |
| ⬜ | `ppu-2c02-die` | RP2C02 PPU Die Analysis | — | — | Transistor-level PPU operation | Visual2C02 project. |

---

## Priority for initial Spectrum bring-up

The minimum reference set needed before starting Spectrum implementation:

1. ✅/⬜ `z80-user-manual` — Zilog's official Z80 manual
2. ✅/⬜ `z80-undocumented` — undocumented Z80 behaviour
3. ✅/⬜ `spectrum-ula-book` — Chris Smith's ULA book (purchase required)
4. ✅/⬜ `spectrum-contention` — contention timing tables
5. ✅/⬜ `spectrum-rom-disassembly` — ROM routines for tape loading
6. ✅/⬜ `fuse-tests` — Z80 CPU test suite
7. ✅/⬜ `tzx-spec` — TZX format specification
8. ✅/⬜ `tap-format` — TAP format description
9. ✅/⬜ `ay-3-8910-datasheet` — AY sound chip (for 128K)

Mark these ✅ as they're acquired.
