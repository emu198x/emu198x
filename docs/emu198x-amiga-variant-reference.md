# Emu198x — Amiga Variant Reference

## Every Production Amiga: Hardware Differences That Affect Emulation

This document covers every production Amiga variant and the specific hardware that differentiates each model. It is a companion to the bring-up plan, which targets the A500 OCS profile first. Use this document to understand what changes between variants and what additional subsystems each model requires.

---

## Model Family Tree

```
Amiga 1000 (1985)         — OCS, 68000, WCS bootstrap
│
├─ Amiga 2000 (1987)      — OCS/ECS, 68000, Zorro II, video slot, CPU slot
│  ├─ A2000-A (German)    — Amiga 1000 chipset in A2000 case
│  ├─ A2000-B (rev 4+)    — Fat Agnus, production model
│  └─ A2000-CR (cost-reduced) — late revision
│
├─ Amiga 500 (1987)       — OCS, 68000, all-in-one
│  ├─ A500 rev 3/5/6/7    — Fat Agnus 8372 (512K or 1MB)
│  └─ A500+ (1991)        — ECS, 8375 Agnus, 8373 Denise, 1MB Chip
│
├─ Amiga 3000 (1990)      — ECS, 68030, Zorro III, SCSI, DMAC, Ramsey
│  ├─ A3000 desktop       — standard
│  ├─ A3000T tower        — same board, tower case
│  └─ A3000UX             — Unix variant (same hardware)
│
├─ CDTV (1991)            — A500 internals, CD-ROM, IR remote, unique boot ROM
│
├─ Amiga 600 (1992)       — ECS, 68000, Gayle, IDE, PCMCIA
│
├─ Amiga 1200 (1992)      — AGA, 68EC020, Gayle, IDE, PCMCIA
│
├─ Amiga 4000 (1992)      — AGA, 68040 or 68030, Zorro III, IDE, Ramsey, Budgie
│  ├─ A4000/040           — 68040 @ 25 MHz
│  ├─ A4000/030           — 68030 @ 25 MHz
│  └─ A4000T (1994)       — tower, SCSI + IDE, 68040 or 68060
│
└─ CD32 (1993)            — AGA, 68EC020, Akiko, CD-ROM, no expansion bus
```

---

## 1. Amiga 1000

*The original. Everything that makes it different stems from one decision: no permanent ROM.*

### Unique Hardware

| Component | Detail |
|---|---|
| CPU | 68000 @ 7.09 MHz (PAL) / 7.16 MHz (NTSC) |
| Chipset | OCS: 8361 Agnus (original, 256K Chip RAM max), 8362 Denise, 8364 Paula |
| Chip RAM | 256K on motherboard + optional 256K daughterboard = 512K max |
| ROM | **WCS (Writable Control Store)** — 64K bootstrap ROM only |
| Kickstart | Loaded from floppy into 256K write-protected RAM at $FC0000–$FFFFFF |
| Gate array | Gary (early revision) |
| Keyboard | External, connected via RJ-11 cable. 6500/1 microcontroller inside keyboard. |
| Expansion | 86-pin sidecar connector (one slot) |

### Emulation-Specific Differences

**WCS Bootstrap ROM:**
The A1000 has a small 64K ROM at $FC0000 that contains only enough code to:
1. Display a colour screen with a hand animation (different from the Kickstart hand)
2. Prompt the user to insert a Kickstart floppy
3. Load 256K of Kickstart code from floppy into a dedicated RAM area at $FC0000–$FFFFFF
4. Write-protect that RAM region
5. Perform a soft reset, which then boots from the RAM-based Kickstart as if it were ROM

This means the A1000 requires:
- A different ROM image (the WCS bootstrap, not a full Kickstart)
- A Kickstart disk image to load from
- Floppy DMA must work during the WCS phase (unlike other models where floppy can be stubbed for the hand screen)
- A memory region at $FC0000 that switches from writable to read-only after Kickstart is loaded

**Agnus 8361 (original Agnus):**
- 256K Chip RAM addressing maximum (not 512K like the 8372)
- Slightly different DMA slot allocation compared to Fat Agnus
- VPOSR identification bits return differently

**Keyboard connector:**
The A1000 keyboard uses an RJ-11 connector (not the 5-pin DIN of later models). The protocol is the same (CIA-A serial port) but the physical interface differs. For emulation purposes, the protocol is identical.

**Daughterboard RAM:**
The A1000 has a separate 256K RAM daughterboard that provides the second half of Chip RAM. If absent, only 256K is available. This affects memory sizing — Kickstart must detect whether 256K or 512K is present.

### When to Implement
Low priority for CL198x unless you want to demonstrate Kickstart loading from floppy. The A1000's educational value is historical rather than practical — no commercial games targeted the 256K-only configuration.

---

## 2. Amiga 2000

*An A500 in a desktop case with Zorro II slots and a CPU slot.*

### Unique Hardware

| Component | Detail |
|---|---|
| CPU | 68000 @ 7.09/7.16 MHz (stock) |
| Chipset | OCS initially, ECS in later revisions |
| Chip RAM | 512K or 1MB depending on Agnus (user-upgradeable) |
| ROM | 256K Kickstart (1.2/1.3) or 512K (2.x/3.x) in ROM socket |
| Gate array | Gary |
| Expansion | 5× Zorro II slots, 1× video slot, 1× CPU accelerator slot |
| RTC | Battery-backed real-time clock (MSM6242B or compatible) at $DC0000 |

### Emulation-Specific Differences

**Zorro II bus:**
The A2000 is the canonical Zorro II machine. Each slot occupies an 8MB window in the $200000–$9FFFFF address space. AutoConfig at $E80000 handles enumeration. For emulation, you need:
- AutoConfig protocol: read manufacturer/product/size from board ROM, assign base address by writing to the board
- Memory boards: map additional RAM at the assigned address, add to exec's memory list
- I/O boards: map registers at assigned address, load any on-board ROM (driver code)
- Boot ROM support: boards with boot ROMs contribute RomTags to the resident module list

**Video slot:**
A dedicated slot that provides access to Denise's RGB output and genlock signals. Used by genlocks, scan doublers, and flickerfixers. For emulation, this can be ignored unless you're emulating specific video hardware.

**CPU slot:**
A 86-pin slot that allows replacing the 68000 with an accelerator card (e.g., GVP G-Force, CSA Mega Midget Racer). The slot provides the full 68000 bus interface. Accelerator cards typically:
- Replace the 68000 with a 68020/030/040/060
- Add their own Fast RAM (mapped in the 32-bit address space)
- Optionally add SCSI controllers

For emulation, the CPU slot means the A2000 can run any 680x0 processor. Model this as a configuration option, not a hardware subsystem.

**Real-time clock:**
The A2000 has an RTC at $DC0000–$DC003F. The MSM6242B provides BCD-encoded date/time registers. Kickstart reads this during boot to set the system clock. For emulation, return sensible date/time values or zeros.

**A2000-A (German revision):**
Early A2000s manufactured in Germany used the original A1000 chipset (8361 Agnus, 256K Chip RAM). These are rare but technically a distinct emulation target.

### When to Implement
After A500 OCS. The A2000 is essentially an A500 with Zorro II and an RTC. If Zorro II and the RTC work, A2000 emulation falls out for free.

---

## 3. Amiga 500

*The target profile. Fully covered in the bring-up plan.*

### Hardware Summary

| Component | Detail |
|---|---|
| CPU | 68000 @ 7.09 MHz (PAL) / 7.16 MHz (NTSC) |
| Chipset | OCS: 8372 Fat Agnus, 8362 Denise, 8364 Paula |
| Chip RAM | 512K (stock), 1MB with 8372A rev 5+ Agnus |
| ROM | 256K at $FC0000 (Kickstart 1.2/1.3) or 512K at $F80000 (Kickstart 2.x/3.x) |
| Gate array | Gary |
| Expansion | Trapdoor (bottom): 512K "Slow RAM" at $C00000. Side: 86-pin sidecar. |
| RTC | None |

### Revision Differences

| Revision | Agnus | Chip RAM | Notes |
|---|---|---|---|
| Rev 3 | 8372 (512K) | 512K | Most common early revision |
| Rev 5 | 8372A (512K) | 512K | Can accept 1MB Agnus swap |
| Rev 6 | 8372A (1MB) | 1MB | Requires 1MB Agnus and RAM chips |
| Rev 7 | 8372A (1MB) | 1MB | Final OCS revision |

### Slow RAM ("Bogo RAM")

The trapdoor expansion adds 512K at $C00000–$C7FFFF. This RAM is on the 68000's bus but is NOT accessible by the custom chipset (Agnus cannot DMA from it). It's called "Slow RAM" because it's slower than Fast RAM (it shares bus timing with Chip RAM accesses) but isn't usable for DMA operations like bitplanes or samples. exec adds it to the memory free list as MEMF_FAST (confusingly) because it's not MEMF_CHIP.

For emulation: map 512K at $C00000 when the trapdoor expansion is configured. CPU accesses work normally. DMA from Agnus cannot reach it.

---

## 4. Amiga 500+

*The A500 with ECS. Shipped briefly and controversially (broke some OCS-dependent games).*

### Unique Hardware

| Component | Detail |
|---|---|
| CPU | 68000 @ 7.09 MHz (PAL) |
| Chipset | ECS: 8375 Agnus, 8373 Super Denise, 8364 Paula |
| Chip RAM | 1MB (stock) |
| ROM | 512K Kickstart 2.04 (37.175) at $F80000 |
| Gate array | Gary |
| Expansion | Same as A500 |
| RTC | Battery-backed RTC (same as A2000) |

### ECS Chipset Differences

The A500+ is the simplest ECS machine and a good second target after the A500 OCS.

**ECS Agnus (8375):**
- 1MB or 2MB Chip RAM addressing (pin-selectable)
- VPOSR bits 14–8 return $20 (identifying ECS)
- Additional registers: BEAMCON0 ($DFF1DC) for programmable sync — controls PAL/NTSC switching, sync generation parameters, and productivity modes
- DIWHIGH ($DFF1E4) — extended display window position (more bits for vertical/horizontal start/stop)
- Sprite improvements: wider sprite positioning range

**ECS Denise (8373 Super Denise):**
- DENISEID ($DFF07C) returns a valid ID (not $FFFF like OCS)
- Productivity modes: 640×480 non-interlaced, 640×400 NTSC non-interlaced
- Superhires mode (35ns pixel clock — 4 pixels per CCK, vs 2 for hires)
- Improved sprite resolution: sprites can be displayed in hires
- Border blanking control: programmable border (can be set to transparent)
- EHB (Extra Half-Brite) mode: 64 colours using 6 bitplanes where the 6th bitplane halves the brightness of the colour selected by bitplanes 1–5

**Backward compatibility issues:**
Some OCS games broke on the A500+ because:
- They relied on specific VPOSR ID values ($00 for OCS) and failed checks on ECS
- They made assumptions about Chip RAM size (expected exactly 512K)
- They used undocumented OCS Agnus timing that changed slightly in ECS
- Kickstart 2.x changed boot behaviour and library interfaces

For emulation, this means ECS mode must correctly report different ID values and handle the extended register set, while OCS mode must NOT expose these registers.

**FIR audio filter:**
The A500+ added a switchable low-pass audio filter controlled by CIA-A PRA bit 1 (the same bit that controls the power LED). When the LED is dim (bit 1 = 1), the filter is bypassed. When the LED is bright (bit 1 = 0), the filter is active. Earlier models had the filter permanently active. This matters for audio accuracy.

### When to Implement
Phase E in the roadmap. The delta from A500 OCS is: swap Agnus/Denise state machines for ECS variants, add BEAMCON0 and DIWHIGH registers, change VPOSR/DENISEID return values, and handle the extended display modes.

---

## 5. Amiga 3000

*The professional Amiga. Substantially different internal architecture.*

### Unique Hardware

| Component | Detail |
|---|---|
| CPU | 68030 @ 16 MHz or 25 MHz |
| FPU | 68881 or 68882 (socket, optional on some models) |
| Chipset | ECS: 8375 Agnus, 8373 Super Denise, 8364 Paula |
| Chip RAM | 1MB or 2MB (2MB with Super Agnus) |
| Fast RAM | Up to 16MB on motherboard (32-bit wide) |
| ROM | 512K Kickstart 2.x/3.x at $F80000. Also supports SuperKickstart. |
| Gate arrays | Gary + **DMAC** (DMA controller for SCSI) |
| Bus | **Zorro III** (32-bit, DMA-capable) — 4 slots |
| Storage | Built-in **SCSI** controller (WD33C93A via DMAC) |
| Memory controller | **Ramsey** (32-bit DRAM controller for Fast RAM) |
| RTC | Battery-backed RTC |
| Flicker fixer | Built-in scan doubler / deinterlacer |

### Emulation-Specific Differences

**68030 CPU:**
The 68030 is a substantially more complex CPU than the 68000:
- Full 32-bit data and address buses
- On-chip instruction and data caches (256 bytes each)
- On-chip MMU (Memory Management Unit)
- Burst mode memory fills
- Pipelined execution (instruction fetch overlaps with execution)

For emulation, you need:
- All 68020 addressing modes (which the 68030 inherits): scaled index, memory indirect, etc.
- Cache simulation: CACR (cache control register), CAAR (cache address register). Kickstart 2.x/3.x actively manages caches.
- MMU simulation: CRP, SRP, TC, MMUSR registers. Kickstart 3.x sets up a minimal translation table. The MMU must be functional because some software (including AmigaOS memory protection schemes) relies on it.
- Instruction timing is completely different from the 68000 — pipelined, with cache hits/misses affecting timing

**DMAC (DMA Controller):**
The A3000's SCSI controller uses a custom DMA controller chip called DMAC. DMAC sits between the WD33C93A SCSI chip and the Zorro III bus, handling DMA transfers for SCSI operations.

DMAC registers are at $00DD0000–$00DD00FF:

| Register | Offset | Function |
|---|---|---|
| DAWR | $003 | DMAC Agnus WR (bus arbitration) |
| WTCH | $00B | Watchdog timer |
| CONTR | $013 | Control register |
| ISTR | $01B | Interrupt status |
| CNTR | $01F | Count register |
| ACR | $023 | Address count register |
| ST_DMA | $03B | Start DMA strobe |
| SP_DMA | $03F | Stop DMA strobe |
| SASR | $041 | SCSI auxiliary status (WD33C93A) |
| SCMD | $049 | SCSI command (WD33C93A) |

**WD33C93A SCSI controller:**
Accessed indirectly through DMAC. The WD33C93A is a standard SCSI controller with its own command set for initiating SCSI bus transactions. For emulation, you need:
- The WD33C93A register set (accessed via DMAC SASR/SCMD)
- SCSI command processing (at minimum: INQUIRY, TEST UNIT READY, READ, WRITE for hard disk support)
- DMA transfer between SCSI data and Amiga memory via DMAC
- Interrupt generation: DMAC signals the CPU via Gary/Paula

**SuperKickstart:**
The A3000 can load Kickstart from the SCSI hard disk into RAM instead of using ROM. The process:
1. A small boot ROM in the A3000 checks for a Kickstart image on the SCSI disk
2. If found, it's loaded into RAM and mapped at $F80000
3. Normal boot proceeds from the RAM-based Kickstart

This is similar to the A1000's WCS but uses SCSI instead of floppy.

**Ramsey Memory Controller:**
Ramsey handles 32-bit DRAM access for the motherboard Fast RAM. It provides:
- DRAM refresh
- Memory sizing
- Burst mode support for the 68030
- Error detection (optional parity)

Ramsey registers are at $00DE0000:

| Register | Offset | Function |
|---|---|---|
| RAMSEY_CONTROL | $003 | Configuration (burst enable, refresh, parity) |
| RAMSEY_VERSION | $043 | Chip version ID |

**Zorro III bus:**
Zorro III is a 32-bit multiplexed address/data bus with DMA capability. It's substantially more complex than Zorro II:
- 32-bit addressing (full 4GB address space)
- 32-bit data transfers
- DMA master capability (boards can bus-master)
- AutoConfig extended for 32-bit addresses
- Zorro III boards can coexist with Zorro II boards (backward-compatible slots)

AutoConfig for Zorro III uses the same $E80000 space but with extended configuration for 32-bit addresses.

**Built-in flicker fixer:**
The A3000 has a built-in scan doubler that can output VGA-compatible video. This is controlled by jumpers and registers. For emulation, this affects video output mode selection but not the chipset model.

### When to Implement
Late in the roadmap. The A3000 requires: 68030 core, MMU, DMAC, WD33C93A SCSI, Ramsey, Zorro III, and SuperKickstart. Each is a significant subsystem.

---

## 6. CDTV (Commodore Dynamic Total Vision)

*An A500 in a set-top box with a CD-ROM drive. The forgotten Amiga.*

### Unique Hardware

| Component | Detail |
|---|---|
| CPU | 68000 @ 7.09 MHz (PAL) |
| Chipset | ECS: 8372A Agnus (1MB), 8362 OCS Denise, 8364 Paula |
| Chip RAM | 1MB |
| ROM | 256K Kickstart 1.3 + 256K extended CDTV ROM |
| Storage | Matsushita/Panasonic CR-563-B CD-ROM drive (single speed) |
| Interface | **TriPort** chip — handles CD-ROM, IR remote, NVRAM |
| NVRAM | 4K battery-backed SRAM for save data |
| Remote | Infrared remote control via TriPort |
| Expansion | Full-size Zorro II slot (under a panel), PCMCIA slot |
| Audio | CD audio mixed with Paula audio |

### Emulation-Specific Differences

**Mixed chipset:**
The CDTV uses an unusual mix: ECS Agnus (for 1MB Chip RAM) but OCS Denise. This means it has 1MB Chip RAM addressing but no ECS display features. VPOSR reports the ECS Agnus ID, but DENISEID returns OCS values. Software must not assume ECS Denise is present just because ECS Agnus is.

**Extended ROM:**
The CDTV has an additional 256K ROM beyond the standard Kickstart. This extended ROM contains:
- cd.device — the CD-ROM driver
- cdfs.library — the ISO 9660 filesystem handler
- cdtv.library — CDTV-specific functions
- Player application — the built-in CD audio player UI
- Bookmark device — NVRAM access
- Boot ROM — modified boot sequence that checks CD before floppy

The extended ROM is mapped at $F00000–$F3FFFF (below the Kickstart ROM at $FC0000).

**TriPort chip:**
The TriPort is a custom gate array unique to the CDTV. It handles:
- CD-ROM interface: command/status/data transfer with the CD-ROM drive
- IR remote receiver: decodes infrared signals from the remote control, translates to key events
- NVRAM: 4K SRAM with battery backup, used for game saves and system preferences

TriPort registers are memory-mapped. The exact register layout is less well-documented than the standard chipset.

**CD-ROM subsystem:**
The CD-ROM is accessed through cd.device, which communicates with the TriPort. The drive uses a subset of the ATAPI/MMC command set. For emulation, you need to support:
- ISO 9660 filesystem reading
- Mixed mode CDs (data + audio tracks)
- CD audio playback (routed through the audio output alongside Paula)
- TOC reading
- Seek/read operations with realistic timing

**NVRAM:**
4K of battery-backed SRAM. Accessed via the bookmark.device through TriPort. For emulation, back this with a persistent file.

**Boot sequence:**
The CDTV boot sequence differs from a standard Amiga:
1. Standard Kickstart 1.3 boots
2. Extended ROM's boot code takes over
3. Checks for CD in drive
4. If CD present with CDTV boot signature: boots from CD
5. If no CD: checks for floppy in external DF0: (optional accessory)
6. If nothing bootable: displays the CDTV player application (CD audio player)

### When to Implement
Low-to-medium priority. Educationally interesting for the CD-ROM boot path and mixed chipset. Requires: TriPort emulation, CD-ROM image support (ISO/BIN+CUE), extended ROM handling.

---

## 7. Amiga 600

*The compact ECS Amiga. First with IDE and PCMCIA.*

### Unique Hardware

| Component | Detail |
|---|---|
| CPU | 68000 @ 7.09 MHz (PAL) |
| Chipset | ECS: 8375 Agnus (1MB or 2MB), 8373 Super Denise, 8364 Paula |
| Chip RAM | 1MB (stock), 2MB with 2MB Agnus |
| ROM | 512K Kickstart 2.05 (37.300) or 2.1 (37.350) at $F80000 |
| Gate array | **Gayle** (replaces Gary) |
| Storage | **IDE** (AT bus) — 44-pin 2.5" connector |
| PCMCIA | **Type I/II PCMCIA slot** (16-bit, via Gayle) |
| Expansion | Trapdoor: CPU accelerator slot |
| RTC | Battery-backed RTC |

### Gayle Gate Array

Gayle is the most significant hardware difference in the A600. It replaces Gary and adds:

**IDE (AT bus) controller:**
- Directly handles IDE command/data/status registers
- Mapped at $DA0000–$DA3FFF (primary) and $DA4000–$DA7FFF (secondary, typically unused)
- Standard ATA register set: Data, Error/Features, Sector Count, LBA Low/Mid/High, Device/Head, Status/Command
- PIO (Programmed I/O) mode only — no DMA for IDE on the A600
- The Kickstart scsi.device driver (confusingly named) talks to the IDE registers directly

IDE register mapping:

| Address | Register | Read | Write |
|---|---|---|---|
| $DA0000 | Data | Data | Data |
| $DA0004 | Error | Error | Features |
| $DA0008 | Sector Count | Sector Count | Sector Count |
| $DA000C | LBA Low | LBA Low | LBA Low |
| $DA0010 | LBA Mid | LBA Mid | LBA Mid |
| $DA0014 | LBA High | LBA High | LBA High |
| $DA0018 | Device/Head | Device/Head | Device/Head |
| $DA001C | Status | Status | Command |
| $DA1010 | Alt Status | Alt Status | Device Control |

**PCMCIA controller:**
- Supports Type I and Type II cards (68-pin)
- Attribute memory at $A00000–$A1FFFF
- Common memory / I/O at $600000–$9FFFFF
- Gayle handles card detection, voltage switching, and interrupt routing
- PCMCIA interrupts route through Gayle to the 68000
- Used for: SRAM cards (additional storage), network cards, modems

Gayle PCMCIA registers:

| Address | Register | Function |
|---|---|---|
| $DA8000 | GAYLE_ID | Gayle identification register |
| $DA9000 | GAYLE_IRQ | Interrupt status |
| $DAA000 | GAYLE_INT | Interrupt enable |
| $DAB000 | GAYLE_CS | Card status (inserted, write protect, etc.) |

**Gayle ID register:**
Software reads GAYLE_ID to detect whether Gayle is present. The read protocol is specific: read the register 8 times, checking bit 7 each time. The pattern identifies the Gayle revision.

**OVL and CIA decode:**
Gayle handles the OVL latch and CIA address decoding instead of Gary. The behaviour is functionally identical but the implementation is different. Some undocumented Gary timing quirks may not be present in Gayle.

**No Slow RAM region:**
The A600 does not have the $C00000 Slow RAM region. Accesses to $C00000–$D7FFFF should bus-error (or return open-bus values depending on exact Gayle behaviour).

### When to Implement
After ECS chipset support. The delta from A500+ is: Gayle (IDE + PCMCIA + modified address decode), Kickstart 2.x differences, and the absence of Slow RAM.

---

## 8. Amiga 1200

*The consumer AGA machine. Primary CL198x upgrade target.*

### Unique Hardware

| Component | Detail |
|---|---|
| CPU | 68EC020 @ 14.18 MHz (PAL) / 14.32 MHz (NTSC) — 2× CCK |
| Chipset | AGA: **Alice** (replaces Agnus), **Lisa** (replaces Denise), 8364 Paula |
| Chip RAM | 2MB |
| ROM | 512K Kickstart 3.0 (39.106) or 3.1 (40.68) at $F80000 |
| Gate array | **Gayle** (same as A600) |
| Storage | IDE — 44-pin 2.5" connector |
| PCMCIA | Type I/II PCMCIA slot |
| Expansion | Trapdoor: 150-pin CPU accelerator slot (direct 68020 bus) |
| RTC | Battery-backed RTC |

### AGA Chipset

AGA (Advanced Graphics Architecture) is a major upgrade. Both Alice and Lisa are new chips with extended capabilities.

**Alice (replaces Agnus):**

- 2MB Chip RAM addressing (always)
- VPOSR bits 14–8 return $22 or $23 (AGA identification)
- **FMODE** ($DFF1FC) — Fetch mode register. Controls the width of DMA fetches:
  - FMODE bits 1–0 (sprite fetch): 0 = 16-bit, 1 = 32-bit, 3 = 64-bit
  - FMODE bits 3–2 (bitplane fetch): 0 = 16-bit, 1 = 32-bit, 3 = 64-bit
  - Wider fetches mean more data per DMA slot, enabling more bitplanes or higher resolutions without exhausting DMA bandwidth
- Modified DMA slot timing to accommodate wider fetches
- All Agnus DMA channels present and functional

**Lisa (replaces Denise):**

- **24-bit colour palette** — each colour register holds 8 bits per channel (RGB888) instead of 4 (RGB444)
- **256 colour registers** (up to 256 colours on screen), accessed via bank switching:
  - BPLCON3 ($DFF106) bits 15–13 select the colour bank (0–7)
  - Each bank provides 32 colour registers at the standard COLOR00–COLOR31 addresses
  - Bank 0 = colours 0–31, Bank 1 = colours 32–63, ..., Bank 7 = colours 224–255
- **8 bitplane mode** — up to 256 colours in chunky-ish planar mode
- **HAM8** — Hold and Modify with 8 bitplanes: 262,144 colours on screen simultaneously
- Superhires pixel clock (35ns, 4 pixels per CCK)
- Sprite improvements: sprites can be 16, 32, or 64 pixels wide depending on FMODE
- Sprite resolution matches playfield resolution
- Scan-doubled output mode
- **LISAID** ($DFF07C) returns AGA Lisa identification value

**24-bit colour register access:**
The OCS/ECS 12-bit colours are in the low nibbles. AGA adds high nibbles:
- Write low nibbles: standard COLOR00–COLOR31 ($DFF180–$DFF1BE) writes set bits 3–0 of each channel
- Write high nibbles: BPLCON3 bit 9 (LOCT) controls whether writes go to high nibbles or low nibbles
- Sequence: write COLOR with LOCT=0 (sets low nibbles), then write COLOR with LOCT=1 (sets high nibbles)

**FMODE and DMA bandwidth:**
FMODE is one of AGA's most important registers. With 64-bit fetch mode:
- Each bitplane DMA slot fetches 4 words (64 bits) instead of 1 word (16 bits)
- This means you can have 8 bitplanes in lores without running out of DMA bandwidth
- But the fetch timing changes: data arrives in bursts, affecting Copper synchronization and mid-line effects

### 68EC020 CPU

The A1200 uses the 68EC020, which is a 68020 without the full external bus interface (hence "EC"). Key differences from 68000:

- 32-bit internal architecture, 32-bit ALU
- 24-bit address bus on the 68EC020 (not 32-bit like the full 68020) — addresses limited to 16MB
- 256-byte instruction cache, no data cache
- New addressing modes: scaled index, memory indirect pre/post-indexed
- New instructions: BFCHG, BFCLR, BFEXTS, BFEXTU, BFFFO, BFINS, BFSET, BFTST (bit field operations), CAS, CAS2 (compare and swap), CHK2, CMP2, DIVS.L/DIVU.L (32-bit divide), MULS.L/MULU.L (32-bit multiply), PACK, UNPK, TRAPcc
- Runs at 14 MHz (2× the chipset CCK)
- No MMU (the EC variant specifically lacks it)

The 68EC020 can execute instructions while Agnus owns the bus (as long as the instruction is in the cache and doesn't need a chip RAM access). This makes CPU/DMA timing interaction more complex than on the 68000.

### When to Implement
Phase F in the roadmap. The A1200 is the second-most important target for CL198x after the A500. Requires: AGA chipset (Alice + Lisa), 68020 CPU core, Gayle (shared with A600), and Kickstart 3.x.

---

## 9. Amiga 4000

*The high-end AGA machine with a real CPU.*

### Unique Hardware

| Component | Detail |
|---|---|
| CPU (A4000/040) | 68040 @ 25 MHz |
| CPU (A4000/030) | 68030 @ 25 MHz + 68882 FPU |
| Chipset | AGA: Alice, Lisa, Paula |
| Chip RAM | 2MB |
| Fast RAM | Up to 16MB on motherboard (32-bit, 60ns) via **Ramsey** |
| ROM | 512K Kickstart 3.0/3.1 at $F80000 |
| Gate arrays | **Budgie** (address decode/bus control) + **Ramsey** (DRAM controller) |
| Bus | Zorro III — variable slot count (typically 4 full-size) |
| Storage | IDE (dual channel) — 40-pin 3.5" connector |
| RTC | Battery-backed RTC |

### Budgie Gate Array

Budgie replaces Gary/Gayle for address decoding and bus control on the A4000. It handles:
- Address decode for all memory regions
- OVL latch
- Bus arbitration between CPU, Zorro III, and chipset
- CIA address decode
- IDE interface (different implementation from Gayle)
- Reset and interrupt routing

Budgie is less well-documented than Gary or Gayle. Some timing differences exist.

### Ramsey Memory Controller

Same Ramsey chip as the A3000 but may be a different revision. Handles the 32-bit Fast RAM on the motherboard. See A3000 section for register details.

### 68040 CPU

The 68040 is significantly more complex than any earlier 680x0:

- 32-bit data and address buses
- Separate instruction and data caches (4KB each)
- On-chip FPU (except the 68LC040 which lacks it)
- On-chip MMU — **mandatory**. The 68040 requires active MMU translation tables to enable caches. Without the MMU configured, caches are disabled.
- Copyback cache mode — the 68040 can delay memory writes in the data cache. This causes problems with DMA-based I/O because Agnus reads stale data from memory. Kickstart 3.x marks Chip RAM pages as write-through (not copyback) to prevent this.
- Instruction set differences from 68030:
  - Some 68881/68882 FPU instructions removed or simplified
  - MOVE16 instruction for burst memory moves
  - CINV and CPUSH for cache line invalidation and push
  - Different exception stack frames

For emulation, the 68040 MMU is non-negotiable. Kickstart 3.x on the A4000/040 sets up MMU tables at boot time. If the MMU doesn't work, the system will not boot.

### 68030 Variant (A4000/030)

The A4000/030 uses a 68030 + 68882 FPU on a CPU card. This is the same 68030 as the A3000 but at 25 MHz. The FPU is a separate chip (68882) providing full floating-point support. See A3000 section for 68030 details.

### IDE on the A4000

The A4000's IDE is handled by Budgie, not Gayle. The register addresses differ from the A600/A1200:

- Primary IDE: $DA0000 (same base but different implementation details)
- Secondary IDE: $DA4000 (the A4000 actually supports dual IDE channels, though few used the secondary)

### Zorro III on the A4000

Same Zorro III bus as the A3000. See A3000 section.

### A4000T (Tower)

The A4000T adds:
- SCSI controller (NCR 53C710) in addition to IDE
- More Zorro III slots
- CPU slot accepts 68040 or 68060 accelerator cards

The NCR 53C710 is a different SCSI controller from the A3000's WD33C93A, with its own register set and DMA model.

### When to Implement
After A1200 AGA. The additional work is: 68040 (or 68030) core with MMU, Budgie (replacing Gayle), Ramsey, Zorro III, and the IDE differences.

---

## 10. CD32

*The Amiga game console. Third priority CL198x target.*

### Unique Hardware

| Component | Detail |
|---|---|
| CPU | 68EC020 @ 14.18 MHz (PAL) |
| Chipset | AGA: Alice, Lisa, Paula |
| Chip RAM | 2MB |
| ROM | 512K Kickstart 3.1 + 512K extended CD32 ROM at $F80000–$FFFFFF |
| Gate array | **Akiko** (unique to CD32) |
| Storage | Double-speed CD-ROM drive (Philips/Sony) |
| Controller | 7-button gamepad via **Akiko** |
| Audio | CD audio via **Akiko**, mixed with Paula |
| Expansion | None standard. Optional "MPEG module" connector on rear (rarely used). FMV card slot. |
| NVRAM | 1KB non-volatile RAM (inside Akiko) |

### Akiko

Akiko is the signature chip of the CD32. It's a custom gate array unique to this machine that handles several functions that are spread across separate chips in other Amigas.

**Akiko base address:** $B80000

**Akiko register map:**

| Offset | Register | Function |
|---|---|---|
| $0000 | AKIKO_ID | Identification ($CAFE for CD32 Akiko) |
| $0002 | CDROM_STAT | CD-ROM status |
| $0004 | CDROM_CMD | CD-ROM command |
| $0008 | CDROM_ADDR | CD-ROM transfer address |
| $0010 | CDROM_DATA | CD-ROM data |
| $0018 | CDROM_SUBCMD | CD-ROM sub-channel command |
| $001C | CDROM_SUBDATA | CD-ROM sub-channel data |
| $0030 | C2P | Chunky-to-planar converter input |
| $0038 | NVRAM | Non-volatile RAM access |

**Chunky-to-planar converter (C2P):**
This is educationally one of the most interesting features. AGA can display 256-colour images, but the display is still planar (8 bitplanes). Converting chunky pixel data (one byte per pixel, as used by 3D renderers and many game engines) to planar format (8 separate bitplane buffers) is computationally expensive on the 68000/68020.

Akiko's C2P hardware does this conversion automatically:
1. Write 32 bytes of chunky data (32 pixels × 8 bits) to the C2P register
2. Read back 32 bytes of planar data (8 planes × 4 bytes each)
3. The hardware performs the bit-transpose in real time

The C2P converter processes data in groups of 32 pixels. For each group:
- Input: 32 bytes where byte N is the 8-bit colour index of pixel N
- Output: 8 × 32-bit words where word N contains the bits for bitplane N (one bit per pixel)

For emulation, this is a pure combinational function — no timing considerations beyond the bus access time.

**CD-ROM controller:**
Akiko interfaces directly with the CD-ROM drive. The command protocol involves:
1. Write command bytes to CDROM_CMD
2. Poll CDROM_STAT for completion
3. Read data from CDROM_DATA (DMA transfer to memory)
4. Sub-channel data (for CD+G, track info) via CDROM_SUBCMD/SUBDATA

CD-ROM commands include:
- Read TOC (table of contents)
- Read sectors (data tracks)
- Play audio (audio tracks)
- Seek
- Stop
- Pause/resume

For emulation, you need to support CUE/BIN or ISO disc images and implement the Akiko command protocol.

**CD32 controller (gamepad):**
The CD32 gamepad is a 7-button controller (Up, Down, Left, Right, Red, Blue, Green, Yellow, Forward, Reverse, Play/Pause) connected to the standard joystick port. The extra buttons beyond the 2 standard fire buttons are read via a serial protocol through the CIA joystick port bits, with Akiko handling the additional button decode.

**NVRAM:**
1KB of non-volatile RAM inside Akiko, used for system preferences and save data. Accessed via the NVRAM register with a serial protocol.

**CD32 boot sequence:**
1. Kickstart 3.1 boots normally
2. Extended ROM at $E00000 provides cd.device, cdfs, and the CD32 boot code
3. Boot code checks for a CD with a valid CD32 boot signature
4. If found: mounts the CD filesystem and boots from it
5. If no CD: displays the CD32 player application (audio CD player with animation)

The CD32 can also boot from an external floppy drive (optional accessory) or from PCMCIA (with the SX-1 expansion module).

### FMV (Full Motion Video) Module

An optional expansion card that provides hardware MPEG-1 video decoding for Video CD playback. Uses the CL450 MPEG decoder chip. Extremely rare and low priority for emulation.

### When to Implement
Phase F alongside the A1200 (shared AGA chipset). The additional work is Akiko (C2P, CD-ROM, NVRAM, gamepad) and CD-ROM image support.

---

## 11. Gate Array Comparison

The gate arrays are the unsung heroes of the Amiga architecture. Each generation adds functionality.

| Function | Gary (A500/A2000) | Gayle (A600/A1200) | Budgie (A4000) | Akiko (CD32) |
|---|---|---|---|---|
| Address decode | ✓ | ✓ | ✓ | ✓ |
| OVL latch | ✓ | ✓ | ✓ | ✓ |
| CIA chip select | ✓ | ✓ | ✓ | ✓ |
| ROM chip select | ✓ | ✓ | ✓ | ✓ |
| Bus arbitration | Basic | Basic | Zorro III + CPU | Basic |
| IDE controller | — | ✓ | ✓ (different impl) | — |
| PCMCIA | — | ✓ | — | — |
| SCSI/CD-ROM | — | — | — | ✓ (CD-ROM) |
| C2P converter | — | — | — | ✓ |
| NVRAM | — | — | — | ✓ |
| Identification reg | — | ✓ (GAYLE_ID) | — | ✓ (AKIKO_ID = $CAFE) |

**Gary** is the simplest: pure address decode, OVL, CIA chip select, and basic bus arbitration. It's a combinational logic chip with almost no internal state.

**Gayle** adds IDE and PCMCIA with their own register sets and interrupt routing. It has more internal state (IDE command state machine, PCMCIA card detection).

**Budgie** replaces Gayle in the A4000 with Zorro III bus arbitration and a different IDE implementation. Less well-documented.

**DMAC** (A3000 only) is not a gate array replacement but an additional chip specifically for SCSI DMA.

**Akiko** is the most complex, combining address decode with CD-ROM control, C2P hardware, and NVRAM.

---

## 12. CPU Comparison

| Feature | 68000 | 68EC020 | 68020 | 68030 | 68040 | 68060 |
|---|---|---|---|---|---|---|
| Used in | A500, A600, A1000, A2000, CDTV | A1200, CD32 | Accelerators | A3000, A4000/030, accelerators | A4000/040, accelerators | Accelerators only |
| Data bus | 16-bit | 32-bit | 32-bit | 32-bit | 32-bit | 32-bit |
| Address bus | 24-bit | 24-bit | 32-bit | 32-bit | 32-bit | 32-bit |
| Clock (stock) | 7.09 MHz | 14.18 MHz | varies | 16–50 MHz | 25–40 MHz | 50–75 MHz |
| I-Cache | — | 256B | 256B | 256B | 4KB | 8KB |
| D-Cache | — | — | — | 256B | 4KB | 8KB |
| FPU | — | — | External | External | On-chip* | On-chip |
| MMU | — | — | — | On-chip | On-chip (mandatory) | On-chip |
| Pipeline | No | 3-stage | 3-stage | 3-stage | 6-stage | Superscalar |
| Burst mode | — | ✓ | ✓ | ✓ | ✓ | ✓ |
| Cache control | — | CACR | CACR | CACR | CACR + CINV/CPUSH | CACR + extended |

\* 68LC040 lacks FPU.

### CPU-Specific Emulation Concerns

**68000:** Straightforward. Well-documented instruction timing. No caches, no MMU. Bus access is synchronous with the chipset.

**68EC020:** Instruction cache means the CPU can execute while the bus is busy. Cache coherency isn't an issue (no DMA writes to instruction space in normal operation, and the cache is small enough that software rarely relies on I-cache behaviour). The 14 MHz clock means the CPU runs at 2× the chipset speed, creating a more complex timing relationship.

**68030:** The data cache adds coherency concerns. When the CPU writes to Chip RAM through the data cache, Agnus may read stale data unless the cache is write-through. Kickstart 3.x configures this correctly. The MMU adds translation overhead and can change access timing depending on table structure.

**68040:** Mandatory MMU, split I/D caches with copyback mode, on-chip FPU with reduced instruction set compared to 68882. The most complex timing model: copyback cache can delay writes visibly, burst fills can stall the CPU, and the 6-stage pipeline means instruction timing is heavily dependent on cache hit rates and memory wait states.

**68060:** Superscalar (two instruction issue per cycle). Branch prediction. Instruction timing is almost impossible to predict statically. The 68060 also removes some 68040 instructions (creating a need for software emulation traps). No production Amiga shipped with a 68060 — it was accelerator-only.

---

## 13. Chipset Comparison

| Feature | OCS | ECS | AGA |
|---|---|---|---|
| Agnus/Alice | 8361/8370/8371/8372 | 8375 | Alice |
| Denise/Lisa | 8362 | 8373 | Lisa |
| Chip RAM max | 256K–1MB (variant) | 1MB–2MB | 2MB |
| Colour registers | 32 × 12-bit | 32 × 12-bit | 256 × 24-bit |
| Max colours | 32 (normal), 4096 (HAM6), 64 (EHB) | Same + productivity modes | 256 (normal), 262144 (HAM8) |
| Bitplanes | 6 max | 6 max | 8 max |
| Fetch modes (FMODE) | 16-bit only | 16-bit only | 16/32/64-bit |
| Sprite width | 16px | 16px | 16/32/64px |
| Sprite colours | 3 per pair | 3 per pair | 3/15 per pair |
| BEAMCON0 | — | ✓ | ✓ |
| DENISEID/LISAID | — | ✓ | ✓ |
| DIWHIGH | — | ✓ | ✓ |
| Scan doubling | — | — | ✓ |
| Border blank | — | ✓ | ✓ |
| Palette banking | — | — | ✓ (BPLCON3) |
| LOCT (24-bit colour) | — | — | ✓ (BPLCON3 bit 9) |
| FMODE | — | — | ✓ ($DFF1FC) |

### Register Differences Between Chipsets

**Registers present in all chipsets:** The entire OCS register set ($DFF000–$DFF1BE) is present and functional in ECS and AGA. OCS software runs on ECS/AGA hardware (with some timing-sensitive exceptions).

**Registers added by ECS:**
- BEAMCON0 ($DFF1DC) — programmable sync, PAL/NTSC control
- DIWHIGH ($DFF1E4) — extended display window positions
- HTOTAL ($DFF1C0) — total horizontal line length (programmable)
- HSSTOP ($DFF1C2) — horizontal sync stop
- HBSTRT ($DFF1C4) — horizontal blank start
- HBSTOP ($DFF1C6) — horizontal blank stop
- VTOTAL ($DFF1C8) — total vertical frame length
- VSSTOP ($DFF1CA) — vertical sync stop
- VBSTRT ($DFF1CC) — vertical blank start
- VBSTOP ($DFF1CE) — vertical blank stop
- HCENTER ($DFF1D0) — horizontal center (for interlace)
- DENISEID ($DFF07C) — Denise identification

**Registers added/modified by AGA:**
- FMODE ($DFF1FC) — fetch mode (DMA width)
- BPLCON3 ($DFF106) — extended: palette banking (bits 15–13), LOCT (bit 9), border blank, sprite resolution
- BPLCON4 ($DFF10C) — extended: colour XOR, sprite colour bank
- CLXCON2 ($DFF10E) — extended collision detection
- All COLORxx registers extended to 24-bit via LOCT mechanism
- SPRxPOS/SPRxCTL extended for wider sprites

**Reading absent registers:** On OCS hardware, reading ECS/AGA-only register addresses returns the last value on the data bus (floating bus). Software detects chipset by reading DENISEID and checking VPOSR — if DENISEID returns $FFFF and VPOSR ID is $00, it's OCS. The emulator must replicate this: do not return valid ECS/AGA values when emulating OCS.

---

## 14. Memory Maps by Model

### A1000

| Range | Component |
|---|---|
| $000000–$03FFFF | Chip RAM (256K, or 512K with daughterboard) |
| $BFD000–$BFEF01 | CIAs |
| $C00000–$C3FFFF | (not present) |
| $DFF000–$DFF1FF | Custom registers (OCS) |
| $FC0000–$FFFFFF | WCS bootstrap ROM / RAM-loaded Kickstart (256K) |

### A500 (OCS, 512K)

| Range | Component |
|---|---|
| $000000–$07FFFF | Chip RAM (512K) |
| $BFD000–$BFEF01 | CIAs |
| $C00000–$C7FFFF | Slow RAM (512K, optional trapdoor expansion) |
| $DFF000–$DFF1FF | Custom registers (OCS) |
| $E80000–$EFFFFF | Autoconfig (sidecar expansion) |
| $FC0000–$FFFFFF | Kickstart ROM (256K) |

### A500+ / A600 (ECS)

| Range | Component |
|---|---|
| $000000–$0FFFFF | Chip RAM (1MB) or $000000–$1FFFFF (2MB) |
| $600000–$9FFFFF | PCMCIA common memory/IO (A600 only) |
| $A00000–$A1FFFF | PCMCIA attribute memory (A600 only) |
| $BFD000–$BFEF01 | CIAs |
| $DA0000–$DA3FFF | IDE primary (A600 only, via Gayle) |
| $DA8000–$DABFFF | Gayle registers (A600 only) |
| $DFF000–$DFF1FF | Custom registers (ECS) |
| $E80000–$EFFFFF | Autoconfig |
| $F80000–$FFFFFF | Kickstart ROM (512K) |

### A2000 (OCS/ECS)

| Range | Component |
|---|---|
| $000000–$07FFFF | Chip RAM (512K or 1MB) |
| $200000–$9FFFFF | Zorro II expansion memory |
| $BFD000–$BFEF01 | CIAs |
| $C00000–$D7FFFF | Slow RAM / Ranger |
| $DC0000–$DC003F | RTC (MSM6242B) |
| $DFF000–$DFF1FF | Custom registers |
| $E80000–$EFFFFF | Autoconfig |
| $F80000–$FFFFFF | Kickstart ROM (512K) or $FC0000–$FFFFFF (256K) |

### A3000 (ECS, 68030)

| Range | Component |
|---|---|
| $00000000–$001FFFFF | Chip RAM (2MB) |
| $00200000–$009FFFFF | Zorro II compatible space |
| $00A00000–$00BEFFFF | CIA and slow decode |
| $00BFD000–$00BFEF01 | CIAs |
| $00C00000–$00D7FFFF | (reserved) |
| $00DC0000–$00DC003F | RTC |
| $00DD0000–$00DD00FF | DMAC registers (SCSI) |
| $00DE0000–$00DE0043 | Ramsey registers |
| $00DFF000–$00DFF1FF | Custom registers (ECS) |
| $00E80000–$00EFFFFF | Autoconfig |
| $00F00000–$00F7FFFF | (reserved / diagnostic) |
| $00F80000–$00FFFFFF | Kickstart ROM (512K) |
| $04000000–$04FFFFFF | Zorro III configuration |
| $10000000–$7FFFFFFF | Zorro III expansion |
| $07000000–$07FFFFFF | Motherboard Fast RAM (up to 16MB, via Ramsey) |

Note: the A3000 uses the full 32-bit address space of the 68030. Addresses above $00FFFFFF are 32-bit-only and not reachable by OCS/ECS chipset DMA.

### A1200 (AGA)

| Range | Component |
|---|---|
| $000000–$1FFFFF | Chip RAM (2MB) |
| $600000–$9FFFFF | PCMCIA common memory/IO |
| $A00000–$A1FFFF | PCMCIA attribute memory |
| $BFD000–$BFEF01 | CIAs |
| $DA0000–$DA3FFF | IDE primary (via Gayle) |
| $DA8000–$DABFFF | Gayle registers |
| $DFF000–$DFF1FF | Custom registers (AGA) |
| $E80000–$EFFFFF | Autoconfig |
| $F80000–$FFFFFF | Kickstart ROM (512K) |

### A4000 (AGA, 68040/68030)

| Range | Component |
|---|---|
| $00000000–$001FFFFF | Chip RAM (2MB) |
| $00200000–$009FFFFF | Zorro II compatible space |
| $00BFD000–$00BFEF01 | CIAs |
| $00DA0000–$00DA3FFF | IDE primary (via Budgie) |
| $00DA4000–$00DA7FFF | IDE secondary |
| $00DC0000–$00DC003F | RTC |
| $00DE0000–$00DE0043 | Ramsey registers |
| $00DFF000–$00DFF1FF | Custom registers (AGA) |
| $00E80000–$00EFFFFF | Autoconfig |
| $00F80000–$00FFFFFF | Kickstart ROM (512K) |
| $04000000–$04FFFFFF | Zorro III configuration |
| $07000000–$07FFFFFF | Motherboard Fast RAM (up to 16MB) |
| $10000000–$7FFFFFFF | Zorro III expansion |

### CD32

| Range | Component |
|---|---|
| $000000–$1FFFFF | Chip RAM (2MB) |
| $B80000–$B8003F | Akiko registers |
| $BFD000–$BFEF01 | CIAs |
| $DFF000–$DFF1FF | Custom registers (AGA) |
| $E00000–$E7FFFF | Extended CD32 ROM (512K) |
| $F80000–$FFFFFF | Kickstart ROM (512K) |

### CDTV

| Range | Component |
|---|---|
| $000000–$0FFFFF | Chip RAM (1MB) |
| $BFD000–$BFEF01 | CIAs |
| $DFF000–$DFF1FF | Custom registers (ECS Agnus + OCS Denise) |
| $E00000–$E3FFFF | TriPort registers |
| $E80000–$EFFFFF | Autoconfig (Zorro II slot) |
| $F00000–$F3FFFF | Extended CDTV ROM (256K) |
| $FC0000–$FFFFFF | Kickstart 1.3 ROM (256K) |

---

## 15. Audio Filter Variants

The Amiga's analogue audio path includes a low-pass filter. Its behaviour varies by model:

| Model | Filter Type | Control | Notes |
|---|---|---|---|
| A1000 | Fixed (always on) | None | Butterworth ~3.3 kHz |
| A500 (early) | Fixed (always on) | None | Same as A1000 |
| A500 (late) | Switchable | CIA-A PRA bit 1 (LED) | LED bright = filter on, LED dim = filter off |
| A500+ | Switchable | CIA-A PRA bit 1 | Same as late A500 |
| A600 | Switchable | CIA-A PRA bit 1 | Same |
| A2000 | Switchable | CIA-A PRA bit 1 | Same |
| A3000 | Switchable | CIA-A PRA bit 1 | Improved filter design |
| A1200 | Switchable | CIA-A PRA bit 1 | Modified filter characteristics |
| A4000 | Switchable | CIA-A PRA bit 1 | Modified filter characteristics |
| CD32 | Switchable | CIA-A PRA bit 1 | Also mixes CD audio |
| CDTV | Switchable | CIA-A PRA bit 1 | Also mixes CD audio |

The exact filter cutoff and rolloff curve varies between models, even between board revisions of the same model. For accurate audio emulation, you'd need per-model filter profiles. For CL198x, a generic switchable low-pass filter is sufficient initially.

---

## 16. Keyboard Variants

All Amigas use the same serial keyboard protocol via CIA-A, but the keyboard hardware differs:

| Model | Controller | Connector | Layout |
|---|---|---|---|
| A1000 | 6500/1 | RJ-11 | 89 keys |
| A500 | 6570-036 (68HC05) | Internal ribbon | 96 keys (integrated) |
| A500+ | 6570-036 | Internal ribbon | 96 keys |
| A600 | 6570-036 | Internal ribbon | 78 keys (reduced, no numpad) |
| A2000 | 6570-036 | 5-pin DIN | 96 keys (external) |
| A3000 | 6570-036 | 5-pin DIN | 104 keys (external) |
| A4000 | 6570-036 | 5-pin DIN | 104 keys (external) |
| A1200 | 6570-036 | Internal ribbon | 96 keys (integrated) |
| CD32 | — | — | No keyboard (gamepad only, optional keyboard via serial) |
| CDTV | — | IR remote | IR remote + optional external keyboard |

### Keyboard Protocol

The protocol is identical for all wired keyboards:
1. Keyboard controller transmits key codes serially via CIA-A SDR (serial data register)
2. Each key event is 8 bits: bit 7 = key up/down, bits 6–0 = key code
3. After receiving a byte, the host acknowledges by pulsing the KDAT handshake line (CIA-A CRA bit 6 controls serial direction)
4. On power-up, the keyboard controller sends an initialisation sequence: first a power-up key stream (all keys released), then a reset warning code ($FE), then the keyboard initiation string ($FD)

For emulation, the keyboard protocol is the same regardless of model. The key code mapping varies (the A600 has fewer keys, the A3000/A4000 have additional keys), but the serial protocol is identical.

---

## 17. Implementation Priority by Variant

For CL198x, the recommended implementation order:

| Priority | Variant | Rationale |
|---|---|---|
| 1 | **A500 (OCS)** | Baseline. Most games target this. Simplest chipset. |
| 2 | **A1200 (AGA)** | Primary upgrade target for CL198x. Teaches AGA programming. |
| 3 | **CD32** | Console target. Shares AGA with A1200. Akiko C2P is educational. |
| 4 | **A500+ (ECS)** | Minimal delta from A500. ECS awareness needed for compatibility. |
| 5 | **A600 (ECS)** | Gayle/IDE needed for A1200 anyway. Small delta from A500+. |
| 6 | **A2000 (OCS/ECS)** | Zorro II for expansion card education. |
| 7 | **CDTV** | CD-ROM education. Niche but historically interesting. |
| 8 | **A4000 (AGA)** | Professional target. 68040/Zorro III/Ramsey. |
| 9 | **A3000 (ECS)** | 68030/SCSI/DMAC. Professional workstation. |
| 10 | **A1000** | Historical/preservation value. WCS bootstrap is unique. |

### Shared Implementation Mapping

Many variants share subsystems. Implementing one enables several:

```
68000 core         → A500, A500+, A600, A1000, A2000, CDTV
68EC020 core       → A1200, CD32
68030 core         → A3000, A4000/030
68040 core         → A4000/040

OCS chipset        → A500, A1000, A2000 (early)
ECS chipset        → A500+, A600, A2000 (late), A3000, CDTV
AGA chipset        → A1200, A4000, CD32

Gary gate array    → A500, A500+, A1000, A2000
Gayle gate array   → A600, A1200
Budgie gate array  → A4000
Akiko              → CD32

Zorro II           → A2000, CDTV (shared with A3000 backward-compat)
Zorro III          → A3000, A4000

IDE (Gayle)        → A600, A1200
IDE (Budgie)       → A4000
SCSI (DMAC+WD33C93A) → A3000
SCSI (NCR 53C710)  → A4000T

PCMCIA (Gayle)     → A600, A1200
Ramsey             → A3000, A4000
```

This means:
- After A500 + A1200, you have 80% of variants covered
- Adding Zorro II gets you the A2000
- Adding Gayle gets you the A600 for free (it's a subset of A1200)
- The A3000 and A4000 are the most expensive to add (unique CPUs, memory controllers, bus architectures)
