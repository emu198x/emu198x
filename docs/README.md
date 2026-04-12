# Amiga Emulator Reference Library

57,000+ lines of cross-referenced documentation extracted from official Commodore manuals, the NDK 3.9, service manuals, Amiga Intern, and the WinUAE/Minimig-AGA/Musashi source code. Built for the Emu198x hardware-accurate emulator project.

## Reading order by task

### "I'm starting from scratch"
1. [Boot process](amiga-boot-process.md) — reset → Kickstart → Exec → DOS → Workbench
2. [Hardware reference](amiga-hardware-reference.md) — memory map, DMA, registers
3. [68000 timing](amiga-68000-timing.md) — CPU bus cycles, prefetch, exceptions
4. [Cycle-accurate notes](amiga-cycle-accurate.md) — per-colour-clock DMA slots from WinUAE

### "I'm implementing the display"
1. [Graphics & display](amiga-graphics-display.md) — bitplanes, Copper, Blitter, sprites
2. [AGA & chip revisions](amiga-aga-and-chip-revisions.md) — FMODE, BPLCON3/4, Lisa, Alice
3. [Cycle-accurate notes](amiga-cycle-accurate.md) — BPL1DAT trigger, fetch pipeline

### "I'm implementing the floppy drive"
1. [DOS, filesystem & disk](amiga-dos-filesystem-disk.md) — MFM, trackdisk, OFS/FFS
2. [I/O, audio & expansion](amiga-io-audio-expansion.md) — disk.resource, CIA floppy control
3. [Resources](amiga-resources.md) — disk.resource deep dive
4. [Hardware reference](amiga-hardware-reference.md) — DSKLEN double-write, DSKSYNC

### "I'm implementing audio"
1. [I/O, audio & expansion](amiga-io-audio-expansion.md) — Paula channels, audio.device
2. [Cycle-accurate notes](amiga-cycle-accurate.md) — audio DMA state machine pipeline
3. [Hardware reference](amiga-hardware-reference.md) — AUDx registers, ADKCON modulation

### "I'm implementing the Exec kernel"
1. [Exec kernel](amiga-exec-kernel.md) — tasks, messages, signals, libraries, devices (V34)
2. [Exec & DOS V37–V45](amiga-exec-dos-v37-v45.md) — AllocVec, pools, cache, V36+ additions
3. [Resources](amiga-resources.md) — cia.resource, disk.resource, misc.resource
4. [Headers reference](amiga-headers-reference.md) — verbatim NDK structs + LVO tables

### "I'm debugging boot"
1. [Boot process](amiga-boot-process.md) — high-level boot chain
2. [Kickstart ROM internals](amiga-kickstart-rom-internals.md) — screen colours, ROMTag tables, ExecBase construction
3. [ROM boot traces](amiga-rom-boot-traces.md) — annotated V37/V40 disassembly
4. [Service & electrical](amiga-service-electrical.md) — reset circuitry, CIA init, overlay

### "I need exact struct layouts and LVO offsets"
1. [Headers reference](amiga-headers-reference.md) — 102 NDK headers verbatim + 77 FD tables

### "I'm implementing expansion / AutoConfig"
1. [I/O, audio & expansion](amiga-io-audio-expansion.md) — Zorro II AutoConfig protocol
2. [AGA & chip revisions](amiga-aga-and-chip-revisions.md) — Gayle, Akiko, Ramsey, Zorro III
3. [Service & electrical](amiga-service-electrical.md) — bus signals, address decode

### "I'm implementing the GUI layer"
1. [Workbench, Intuition & GUI](amiga-workbench-intuition-gui.md) — Screens, Windows, BOOPSI, GadTools, Commodities, Workbench

## Document index

| Document | Lines | Scope | Primary sources |
|---|---:|---|---|
| [amiga-boot-process.md](amiga-boot-process.md) | 1,793 | Reset → Kickstart → Exec → AutoConfig → Strap → DOS → WB | Exec RKM, HRM, SPG |
| [amiga-hardware-reference.md](amiga-hardware-reference.md) | 2,117 | Memory map, DMA slots, Agnus/Denise/Paula/CIA registers, clocks | HRM, A500/A2000 TRM |
| [amiga-graphics-display.md](amiga-graphics-display.md) | 2,383 | Display pipeline, Copper, Blitter, sprites, graphics.library, Intuition | HRM, RKM L&D |
| [amiga-exec-kernel.md](amiga-exec-kernel.md) | 2,935 | Tasks, messages, signals, memory, libraries, devices, interrupts | Exec RKM, Autodocs |
| [amiga-dos-filesystem-disk.md](amiga-dos-filesystem-disk.md) | 3,765 | BCPL/packets, dos.library, OFS/FFS, trackdisk, RDB, CLI/Shell | AmigaDOS Manual, RKM L&D |
| [amiga-io-audio-expansion.md](amiga-io-audio-expansion.md) | 2,135 | Audio, serial, parallel, keyboard, gameport, timers, AutoConfig | HRM, RKM L&D, A500/A2000 TRM |
| [amiga-kickstart-rom-internals.md](amiga-kickstart-rom-internals.md) | 2,189 | ROM layout, boot colours, ExecBase construction, ROMTag tables, alerts | Amiga Intern, NDK, Startup Routine |
| [amiga-service-electrical.md](amiga-service-electrical.md) | 1,903 | Chip revisions, motherboards, schematics, reset timing, bus signals | Service manuals, Amiga Intern |
| [amiga-headers-reference.md](amiga-headers-reference.md) | 16,806 | Verbatim NDK 3.9 C headers + 77 FD/LVO tables | NDK 3.9 |
| [amiga-workbench-intuition-gui.md](amiga-workbench-intuition-gui.md) | 3,543 | Intuition V37+, BOOPSI, GadTools, Workbench, commodities, ASL, IFF | NDK Autodocs, RKM 3rd Libraries |
| [amiga-exec-dos-v37-v45.md](amiga-exec-dos-v37-v45.md) | 4,835 | V36+ Exec/DOS API additions with verbatim autodocs | NDK exec.doc, dos.doc, FD files |
| [amiga-resources.md](amiga-resources.md) | 2,622 | cia/disk/battclock/battmem/potgo/misc/card/filesys resources | NDK Autodocs, resource headers |
| [amiga-aga-and-chip-revisions.md](amiga-aga-and-chip-revisions.md) | 2,523 | AGA registers (FMODE, BPLCON3/4), Gayle, Akiko, chip errata | Minimig-AGA Verilog, WinUAE |
| [amiga-cycle-accurate.md](amiga-cycle-accurate.md) | 3,055 | Per-CCK DMA slots, blitter pipeline, CIA timing, CPU contention | WinUAE custom.cpp, blitter.cpp |
| [amiga-68000-timing.md](amiga-68000-timing.md) | 2,966 | Prefetch, bus cycles, exceptions, instruction timing, TAS ban | M68000 Family Ref, Musashi, WinUAE |
| [amiga-rom-boot-traces.md](amiga-rom-boot-traces.md) | 1,578 | Annotated V37/V40 reset disassembly, ROMTag dumps, dispatcher | Kickstart ROMs via Capstone |
| [amiga-cia-8520-datasheet.md](amiga-cia-8520-datasheet.md) | 430 | MOS 8520 CIA complete register/timing reference | 8520 datasheet (theflatnet.de) |
| [amiga-guru-book-extracts.md](amiga-guru-book-extracts.md) | 1,374 | Bootblock checksum, Paula filter, TAS/CLR quirks, bus errors | The Amiga Guru Book (Babel) |
| [amiga-fpu-68881-reference.md](amiga-fpu-68881-reference.md) | 2,237 | 68881/68882 FPU registers, ISA, timing, 68040 differences | MC68881 User's Manual (Motorola) |
| [amiga-a3000-and-vamiga-guide.md](amiga-a3000-and-vamiga-guide.md) | 2,033 | A3000 architecture (Fat Gary, Ramsey, async bus) + vAmiga source map | A3000 schematics, vAmiga source |
| [amiga-mfm-track-format.md](amiga-mfm-track-format.md) | 1,698 | MFM encoding, sector layout (1,088 bytes), track geometry, ADF format | vAmiga MFM.cpp, WinUAE disk.cpp |
| [amiga-register-reset-states.md](amiga-register-reset-states.md) | 828 | Every custom/CIA register reset value, 10 emulator disagreements | WinUAE custom_reset, vAmiga _reset |
| [amiga-paula-audio-model.md](amiga-paula-audio-model.md) | 1,370 | Filter chain (static LP + LED), audio state machine, DAC model | vAmiga AudioFilter, WinUAE audio.cpp |
| [amiga-68010-to-68060-reference.md](amiga-68010-to-68060-reference.md) | 3,064 | 68010-68060 ISA deltas, caches, MMU, exceptions, superscalar | MC68010/020/030/040/060 User's Manuals |
| **Total** | **~70,250** | | |

## Key cross-cutting facts

These are the hardest things to get right in an Amiga emulator, distilled from all 16 documents:

- The **master oscillator** drives everything — tick the crystal, not the CPU
- **Overlay** is CIA-A PRA bit 0 at $BFE001; DDRA must be $03 at reset
- **RESET + JMP(a0) share a longword** — prefetch queue trick; only supported reboot
- **DSKLEN requires a double-write** with $4000 safety value
- **BPL1DAT is the Denise commit trigger** for all bitplanes, not per-plane
- **Copper uses only odd-numbered DMA cycles** — no priority arbitration needed
- **Blitter nasty steals after exactly 3 CCKs** of CPU starvation, not 2 or 4
- **CIA accesses cost 5–9.5 CCKs** due to E-clock sync, not the "2.5 E" average
- **Bitplane/sprite DMA decisions are made 1 CCK early** via a pipeline register
- **AllocMem only Forbid()s, doesn't Disable()** — interrupts see inconsistent free lists
- **Library jump tables are live writable code** — SetFunction patches 6-byte JMP slots
- **Wait() silently undoes Forbid()/Disable()** — blocking calls restore scheduling
- **TAS instruction is banned** — Agnus doesn't arbitrate the chip bus, so RMW isn't atomic vs DMA
- **CLR reads before writing on 68000** — CLR.W to write-only custom regs causes a spurious read cycle
- **FMODE change has a 2-CCK pipeline delay** (WinUAE) or is immediate (Minimig)
- **CLXCON2 is silently cleared when CLXCON is written** — undocumented interaction
- **Sprite attachment semantics differ OCS vs AGA** — only odd sprite's bit matters on AGA

## Source material

### Extracted text corpus
- `txt/` — 10 original PDFs (HRM, RKMs, AmigaDOS, SPG, etc.)
- `rkm/txt/` — 22 additional PDFs (3rd ed RKMs, Amiga Intern, service manuals)

### NDK 3.9
- `ndk/NDK_3.9/Documentation/Autodocs/` — 96 V45 autodoc files
- `ndk/NDK_3.9/Include/include_h/` — complete C headers
- `ndk/NDK_3.9/Include/fd/` — 77 FD/LVO files

### Emulator source (at ~/Projects/Emu198x-Unclean/)
- `WinUAE/` — reference software emulator
- `Minimig-AGA_MiSTer/rtl/` — FPGA AGA implementation (Verilog)
- `Musashi/` — portable 68000 core
- `fs-uae/` — FS-UAE emulator

### Kickstart ROMs (at ~/Projects/Emu198x-archive/roms/)
- V1.0 through V3.1, all Amiga models, including CD32

### Additional reference emulator
- `vAmiga/` — modern cycle-accurate Amiga emulator (C++, ~51K lines, cleaner than WinUAE). See `amiga-a3000-and-vamiga-guide.md` Part 2 for component-by-component source map.

## Known gaps

Documented in each file's "Gaps" section. The largest remaining:
- MFM floppy track format byte-level layout (Guru Book doesn't cover hardware-level MFM; need WinUAE diskcontroller source or direct analysis)
- Paula low-pass filter R/C values (Guru Book confirms ~7 kHz cutoff but no component-level model)
- Custom chip register reset states (not comprehensively documented anywhere)
- 68010 loop mode exact eligibility rules
- 68040/060 copyback cache coherency with custom chip DMA
- FMODE bits 13:4 purpose (undocumented in both Minimig and WinUAE)
- Budgie ASIC (A1200 glue) — zero spec anywhere
- HAM8 colour bank interaction edge cases
- A3000 Ramsey register map (not in schematics; needs A3000 Technical Reference Manual)
- Exact ReAction/BOOPSI class attribute lists (per-class deep dive)
