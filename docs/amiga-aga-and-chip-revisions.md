# Amiga AGA Chipset and Chip-Revision Reference

*Supplement to `amiga-hardware-reference.md`, `amiga-graphics-display.md`, and `amiga-service-electrical.md`. These documents cover OCS and ECS from the printed Amiga manuals. This one extends them with AGA (Advanced Graphics Architecture) and the late-era support-chip revisions (Alice/Lisa, Akiko, Gayle, Budgie, Gary/Fat Gary, Ramsey, Super Buster, plus CIA errata) that the printed manuals either predate or never described.*

*Primary sources:*

- *Minimig-AGA_MiSTer Verilog at `~/Projects/Emu198x-Unclean/Minimig-AGA_MiSTer/rtl/`. The Minimig FPGA implementation is the authoritative register decode for AGA bitplane, sprite, color-table, and beamcounter logic.*
- *WinUAE C++ sources at `~/Projects/Emu198x-Unclean/WinUAE/`, particularly `custom.cpp`, `akiko.cpp`, `gayle.cpp`, `cia.cpp`, `expansion.cpp`, `include/custom.h`, `include/inputdevice.h`, and `inputdevice.cpp`.*

*Citations are of the form `(file:line)`. Where Minimig and WinUAE agree, the detail is load-bearing. Where they disagree, it is noted. Where neither documents a bit, it is marked "reserved" or "not modelled".*

---

## Table of Contents

1. [Chip revision timeline](#1-chip-revision-timeline)
2. [AGA — Alice (Agnus successor)](#2-aga--alice-agnus-successor)
3. [AGA — Lisa (Denise successor)](#3-aga--lisa-denise-successor)
4. [AGA sprites](#4-aga-sprites)
5. [AGA register bit tables](#5-aga-register-bit-tables)
6. [Akiko (CD32)](#6-akiko-cd32)
7. [Gayle (A600/A1200)](#7-gayle-a600a1200)
8. [Budgie (A1200 glue)](#8-budgie-a1200-glue)
9. [Super Buster / Zorro III](#9-super-buster--zorro-iii)
10. [Ramsey (A3000/A4000 memory controller)](#10-ramsey-a3000a4000-memory-controller)
11. [CIA 8520 errata](#11-cia-8520-errata)
12. [ECS corners (BEAMCON0, DIWHIGH and friends)](#12-ecs-corners-beamcon0-diwhigh-and-friends)
13. [Summary table](#13-summary-table)

**Appendices**

- A. [Complete AGA register bit table](#appendix-a--complete-aga-register-bit-table)
- B. [Complete BEAMCON0 bit table](#appendix-b--complete-beamcon0-bit-table)
- C. [Chip revision matrix](#appendix-c--chip-revision-matrix)
- D. [Gaps and thin spots](#appendix-d--gaps-and-thin-spots)
- E. [Source map](#appendix-e--source-map)
- F. [Minimig vs WinUAE cross-reference notes](#appendix-f--minimig-vs-winuae-cross-reference-notes)

---

## 1. Chip revision timeline

The Amiga custom chipset existed in three generations. Each generation was typically spread across several rev chips with subtly different errata and sometimes different register decode. An emulator that claims "Amiga 1200 compatibility" must know which chip revisions can appear on the board it is pretending to be, and the DENISEID register — the one programmatic way to probe the generation at runtime — gives only three values.

### 1.1 DENISEID — runtime chipset probe

`(Minimig-AGA rtl/denise.v:293)`

```
deniseid_out = (reg_address == DENISEID)
             ? aga ? 16'h00F8
                   : ecs ? 16'hFFFC
                         : 16'hFFFF
             : 16'h0000;
```

That is the one bit of chipset ID the software has. Everything else must be inferred from side-effects (does writing FMODE do anything, does BPLCON3 bit 9 latch, does BPLCON0 bit 4 act as BPU3). Graphics.library's `GfxBase->ChipRevBits0` exists because of this: it's the library's cached copy of the probe result.

### 1.2 OCS (Original Chip Set) — 1985 to 1990

| Part | Die name | Role | Max chip RAM | First machine |
|------|----------|------|--------------|---------------|
| 8361 | Agnus (DIP 48-pin) | Master timing, blitter, copper, DMA | 512 KiB | A1000 |
| 8367 | Agnus (DIP/PLCC, "A" Agnus) | Same as 8361 but PAL/NTSC switchable | 512 KiB | A500 early |
| 8370 | Agnus ("Fat Agnus", PLCC) | 512 KiB chip | 512 KiB | A500, A2000 rev 4.x |
| 8371 | Agnus ("Fat Agnus", ext. cap) | 1 MiB chip RAM | 1 MiB (with modification) | A500, A2000 |
| 8362 | Denise | Display, sprites, colour table | — | All OCS |
| 8364 | Paula | Audio, floppy, serial, interrupts | — | All OCS |

**How to distinguish 8361 from 8367/8370 at runtime**: the A1000 has no DMACONR bit 14 (the `BLTDONE` bit comes back as zero when BLTNASTY is set); fat Agnus returns the real blitter idle state in both cases. Software that checks DMACONR between blits can tell the difference. In practice, emulators don't bother — Amiga Kickstart 1.x handles both. The A1000 also has no `aen` distinction between chip and custom-register access, so writes to $DFF000+n from the CPU bus will only go through when the bus has been granted; on fat Agnus with BLTNASTY the CPU can be locked out indefinitely. Minimig's comment describes this: "Blitter nasty mode is only activated if blitter activates bltpri cause it depends on blitter settings if blitter will really block the cpu" `(agnus.v:48-51)`.

**OCS particulars relevant to emulation:**

- The A1000 Denise has a quirk called "A1k mode" that affects EHB (Extra Half Brite) — the A1000 Denise has no EHB, so six-bitplane mode uses colours 0..31 instead of 0..31 plus shifted 0..31. Minimig exposes this as an `a1k` input: `assign ehb_en = !killehb && !a1k && !ham && !dpf && (l_bpu == 4'd6);` `(denise.v:397)`.
- The A1000 Agnus asserts the vertical blank interrupt on line 1, not line 0, and Minimig special-cases this: `vbl_int <= hpos==8 && vpos==(a1k ? 1 : 0)` `(agnus_beamcounter.v:356)`. This is probably the single most visible A1000-vs-later difference — it changes the timing of where a VBI routine can modify BPLCONx safely on line 0.
- 8370 was limited to 512 KiB by the external DRAM address multiplexer. 8371 drove one extra address line and so could address 1 MiB (still as chip RAM, still at $00000000 — there is no "slow RAM" trick at the chip level, only from the point of view of the external bus controller). Software that wants >512 KiB of chip must read DMACONR and not just assume "A500" — the same Kickstart 1.3 ROM runs on both.
- OCS Denise hard-limits BPU to 4 bits in 4-bpl lores / 3 bits elsewhere; any setting of 7 is treated as EHB (6 planes). Minimig shows this clamp explicitly: `l_bpu <= (bpu == 4'd7 ? 4'd6 : bpu)` when not AGA `(denise.v:166)`. An emulator that incorrectly allows 7 bitplanes on OCS will run some demos and break others. The same clamp also means that on AGA the write of `BPU=7` gives 7 bitplanes (an uncommon configuration, but valid: 128 colours), while on OCS/ECS `BPU=7` is decoded as 6 bitplanes with EHB semantics. This is a source of wrong-colour bugs in emulators that handle BPU as a raw 4-bit field regardless of chipset.
- OCS Agnus has no DDFSTRT bit 2 (it is always zero for alignment purposes). ECS Agnus respects bit 2, allowing 2-CCK-granular fetch start. Minimig ANDs with `ecs`: `hpos[8:1] == {ddfstrt[8:3], ddfstrt[2] & ecs, 1'b0}` `(agnus_bitplanedma.v:339)`. On OCS, writing DDFSTRT=$0030 and DDFSTRT=$0032 does the same thing; on ECS and AGA they are two different fetch starts. A common demo effect is a 1-pixel horizontal shift that OCS cannot achieve with DDFSTRT alone; on ECS/AGA it can.
- OCS is DMACON bit 9 (BLTPRI) write-only. Reading DMACONR gives you all the DMA-enable bits and the blitter state but not the priority. Some debuggers assume they can read back BLTPRI and fail on all Amigas.
- OCS Paula's serial receiver has a known quirk with break detection — `SERDATR` bit 11 (RBF) stays asserted even after reading SERDATR if a break condition is ongoing. ECS and AGA Paula is the same silicon, so this quirk persists across generations.

### 1.3 ECS (Enhanced Chip Set) — 1990 to 1992

| Part | Die name | Role | Max chip RAM | First machine |
|------|----------|------|--------------|---------------|
| 8372A | Agnus ("Fat Agnus", 84-pin PLCC) | 1 MiB chip RAM, ECS features | 1 MiB | A500+, A2000 rev 8 |
| 8375 | Agnus ("ECS Agnus", variants) | 1 or 2 MiB chip RAM, PAL/NTSC, SuperHires | 2 MiB | A500+, A600, A3000 |
| 8373 | Denise ("Super Denise") | SuperHires, programmable sync, BRDR* bits | — | A500+, A600, A3000 |
| 8364 | Paula (unchanged) | — | — | — |

**ECS new features visible at the register level:**

- **SuperHires (35 ns pixel)** — BPLCON0 bit 6 (`SHRES`). Limited to 4 colours (2 bitplanes) on ECS because of memory bandwidth. AGA can do up to 16 colours (4 bitplanes) in SHRES and 256 colours at hires. Minimig respects ECS gating: `assign shres = ecs & bplcon0_delayed[5];` (note position 5 because ECS BPLCON0 re-uses the bit; see the BPLCON0 map in section 5) `(agnus_bitplanedma.v:298)`. An OCS machine that sees SHRES=1 silently ignores it — the chip does not even attempt the higher clock rate because its shift registers are only pipelined for hires.
- **Programmable screen modes via BEAMCON0** — VTOTAL, HTOTAL, HSSTRT, HSSTOP, HBSTRT, HBSTOP, HCENTER, VSSTRT, VSSTOP, VBSTRT, VBSTOP. All covered in section 12. The runtime NTSC/PAL switch (BEAMCON0 bit 5) is the only ECS feature that can be exercised without any other setup: a game simply writes BEAMCON0=$0000 or $0020 and the chip retiming happens on the next vsync.
- **DIWHIGH** for 11-bit vertical and 9-bit horizontal display-window limits. Writing DIWSTRT or DIWSTOP clears the high bits; DIWHIGH must be re-written *after* DIWSTRT/DIWSTOP to take effect. Covered in sections 5 and 12. This is one of the most common "programmer surprise" bugs even on real hardware: the sequence `DIWHIGH=x; DIWSTRT=y; DIWSTOP=z` produces an OCS-compatible display window with zero vertical high bits, ignoring the earlier DIWHIGH write.
- **Border Blank, Border Transparent, ZD pin control** via BPLCON3 bits — but those bits latch only when BPLCON0 bit 0 (`ECSENA`) is set. Minimig: `assign brdrblnk = bplcon3[5] & ecsena;` `(denise.v:216)`, `ecsena = bplcon0[0]` `(denise.v:157)`. An emulator that forgets to gate these bits on ECSENA will render OCS demos wrong — specifically, an OCS demo that writes BPLCON3 garbage (expecting no effect) will enable BRDRBLNK on the emulator and flash black during the border.
- **Interrupt from blitter stop**, **wider sprite control**, and **VPOSR ID nibble** (`{long_frame, 0, ecs, ntsc, 2'b0, {2{aga}}, long_line, 4'b0, vpos[10:8]}` `(agnus_beamcounter.v:107)`). The top nibble of VPOSR is the chip ID that graphics.library reads. Specifically: bit 15 = LONGFRAME, bit 13 = ECS Agnus, bit 12 = NTSC, bits 9:8 = `{2{aga}}` — so bits 9 and 8 both zero is OCS/ECS, both one is AGA. This is graphics.library's `GfxBase->ChipRevBits0` probe source.
- **DMAL extension** — ECS Paula gets two additional DMA slots for audio scan-doubling. Unused by most software but documented in `(agnus.v:89-91)`.
- **SPRxPOS bit 7 (SH10)** — ECS extends sprite VSTART/VSTOP to 10 bits by stealing SPRxPOS[7] and SPRxCTL[6,5,2,1]. Minimig models this completely for ECS and AGA `(agnus_spritedma.v:221, 225)`. An OCS emulator that does not mask SPRxPOS bit 7 will get "impossible" sprite positions above line 256.

### 1.4 AGA (Advanced Graphics Architecture) — 1992 onwards

| Part | Die name | Role | Max chip RAM | First machine |
|------|----------|------|--------------|---------------|
| — | Alice ("address line editor" — Agnus successor) | Wider bitplane and sprite DMA, same copper and blitter | 2 MiB | A1200, A4000, CD32 |
| — | Lisa ("line shifter" — Denise successor) | 8-bpl, 24-bit palette, 256-colour, wide sprites | — | A1200, A4000, CD32 |
| 8364R7 | Paula (unchanged) | — | — | — |

AGA is more conservative than it looks. **No new copper instructions. No new blitter modes. No new audio. Paula is the same silicon.** The CPU-visible "new hardware" is entirely in the graphics pipeline and the memory-path width from Alice to Lisa. Everything else — interrupts, DMA priorities, copper semantics, floppy — is ECS-compatible.

What actually changed:

- **The bitplane bus is 16/32/64 bits wide** instead of 16 bits. FMODE selects. Alice fetches 1, 2, or 4 chip-RAM words per bitplane DMA slot (`(agnus_bitplanedma.v:487–491)`: `fmode[1:0] == 2'b11 ? 3'd4 : fmode[1:0] == 2'b00 ? 3'd1 : 3'd2`).
- **Sprite DMA has the same 16/32/64-bit width selector** via FMODE bits 2-3, controlled independently from bitplanes `(agnus_spritedma.v:159–165)`.
- **BPU is now 4 bits**: BPLCON0 bit 4 becomes BPU3. Up to 8 bitplanes (256 colours direct) `(denise.v:146)` — `assign bpu = {bplcon0[4] & aga, bplcon0[14:12]};`.
- **The colour table is now 256 × 24-bit** (32 banks × 32 entries × 12 bits, double-written for 24-bit). BPLCON3 bit 9 (LOCT) is the "low/high" selector for 24-bit writes `(denise_colortable.v:32–33)`.
- **BPLCON3 bits 15:13 become a colour-bank selector** for writes and for HAM `(denise_colortable.v:30)`.
- **BPLCON4 controls sprite-MSB offsets and bitplane colour XOR** — `(denise.v:254–256)`.
- **Horizontal scroll gets 3 more bits** (BPLCON1 extended to full 16 bits, previously the upper byte was don't-care) — `(denise_bitplanes.v:144)`: `bplcon1 <= aga ? data_in[15:0] : {2'b00, 2'b11, 2'b00, 2'b11, data_in[7:0]}`. The non-AGA path forces the upper bits to a fixed pattern.
- **Sprite resolution** (LORES/HIRES/SHRES) selectable per frame via BPLCON3 bits 7:6 (SPRES) — `(denise.v:215)`: `assign spres = bplcon3[7:6] & {2{aga}};`.

### 1.5 Bridge / glue chips (OCS era → AGA era)

The custom chipset is only half the story. The "bridge" chips sit between the custom registers and the rest of the machine — they control address decoding, IDE, PCMCIA, Zorro, DRAM, and keyboard. Every Amiga revision had one (or several). Emulator-relevant rows only:

| Chip | Role | Machines |
|------|------|----------|
| Gary (5719) | Address decoding, cycle arbitration, $DExxxx/$D9xxxx/$DFxxxx bus timing | A500, A2000 |
| Fat Gary | Superset of Gary for 32-bit bus | A3000, A4000 (alongside Ramsey) |
| Ramsey (390537 / 390544) | DRAM controller for A3000/A4000 chip + fast RAM | A3000, A4000 |
| Super Buster (390539 / 390537) | Zorro III bus controller | A3000, A4000 |
| Gayle (315507) | IDE, PCMCIA, clock-select, INT6 | A600, A1200 |
| Budgie (391425) | Alice-to-68EC020 bus glue | A1200 |
| Akiko (391407) | CD interface, chunky-to-planar, NVRAM, CD32 gamepad routing | CD32 |

Each of these appears in WinUAE via a `cs_*rev` preference — e.g. `cs_ramseyrev`, `cs_fatgaryrev`. Setting the rev to -1 removes the chip from the decoded memory map entirely `(expansion.cpp:3213)`.

### 1.6 Chipset detection — VPOSR and DENISEID combined strategy

An emulator author implementing graphics.library's chipset-probing logic needs both registers. The VPOSR top byte `(agnus_beamcounter.v:107)` provides:

```verilog
data_out[15:0] = {long_frame, 1'b0, ecs, ntsc, 2'b00, {2{aga}}, long_line, 4'b0000, vpos[10:8]};
```

Decoding for the top nibble:

| Bits 15:8 of VPOSR | Meaning |
|---------------------|---------|
| bit 15 | LONGFRAME — 1 in the long field of interlace (313 lines); 0 in the short (312) |
| bit 14 | Always 0 (reserved) |
| bit 13 | ECS Agnus present (1 on 8372A/8375/Alice, 0 on 8361/8367/8370/8371) |
| bit 12 | NTSC board (1 on NTSC, 0 on PAL) |
| bits 11:10 | Always 0 on OCS/ECS |
| bits 9:8 | Both 1 on AGA (Alice), both 0 on OCS/ECS |

So `VPOSR & 0x3F00` gives:
- `$0000` = OCS
- `$2000` = ECS, PAL
- `$3000` = ECS, NTSC
- `$2300` = AGA, PAL
- `$3300` = AGA, NTSC

A WinUAE quirk: `DENISEID` for an A4000-class AGA machine returns `$FCF8` rather than `$00F8` `(custom.cpp:2344-2347)`:

```c
if (aga_mode) {
    if (currprefs.cs_ide == IDE_A4000)
        return 0xFCF8;
    return 0x00F8;
}
```

This distinguishes the A4000 Lisa (which has a slightly different revision of the graphics pipeline feeding the $FCxx upper nibble) from A1200/CD32 Lisa. In practice, software does not rely on the difference — `$xxF8` low byte is all that matters for AGA detection. But an emulator that maps A4000 should return `$FCF8` for maximal compatibility with hardware probing tools.

On OCS, DENISEID doesn't exist as a register. Reading $DFF07C on a 68000 with compatible/cycle-exact emulation returns open-bus data (usually the last word on the data bus), which is `$FFFF` in most cases `(custom.cpp:2353-2355)`. This is why the check is:

- `$00F8` or `$FCF8` → AGA
- `$FFFC` → ECS Denise
- `$FFFF` → OCS (or bus noise — software handles gracefully)

### 1.7 Chipset configuration matrix in WinUAE

WinUAE tracks which chips are present via `currprefs.chipset_mask` and individual `cs_*` fields. The mask constants `(include/options.h)`:

| Constant | Meaning |
|----------|---------|
| `CSMASK_ECS_AGNUS` | ECS Agnus (8372A/8375) features |
| `CSMASK_ECS_DENISE` | ECS Denise (8373) features |
| `CSMASK_AGA` | AGA (Alice + Lisa) features |

Individual chip revision preferences:

| Preference | Type | Meaning |
|------------|------|---------|
| `cs_deniserev` | integer | Override DENISEID return value. -1 = auto. |
| `cs_fatgaryrev` | integer | Fat Gary revision. -1 = absent. 0 = present. |
| `cs_ramseyrev` | integer | Ramsey revision. -1 = absent. $0D = rev D. $0F = rev F. |
| `cs_ide` | enum | IDE type. 0 = none. 1 = A600/A1200 (Gayle). 2 = A4000. |
| `cs_pcmcia` | bool | PCMCIA slot present (A600/A1200). |
| `cs_cd32cd` | bool | CD32 CD drive (Akiko). |
| `cs_cd32c2p` | bool | CD32 C2P accelerator (Akiko). |
| `cs_cd32nvram` | bool | CD32 NVRAM (Akiko I2C). |
| `cs_cia6526` | bool | Use 6526 behaviour instead of 8520. |
| `cs_ciatodbug` | bool | Enable TODMED BCD wrap bug. |

Source: `cfgfile.cpp:8614-8615, 8920-8921, 9789-9791`.

This is useful for an emulator author because it shows the canonical set of configuration knobs that a mature emulator uses to represent every Amiga model from A1000 to CD32.

---

## 2. AGA — Alice (Agnus successor)

Alice is Agnus with a wider chip-RAM bus, a bigger fetch FIFO, FMODE-controlled fetch widths, and 11-bit vertical display-window limits. It is not a new copper or a new blitter.

**What Alice does NOT change:**

- The blitter. Same minterms, same A/B/C/D channel semantics, same line-drawing mode, same area-fill modes, same ZERO-detect output. Blitter channel DMA slot allocation is unchanged. No AGA registers extend the blitter (no BLTxPTH extension beyond BLTxPTH's top 5 bits of a 21-bit chip-RAM address). Minimig's `agnus_blitter.v`, `agnus_blitter_adrgen.v`, `agnus_blitter_fill.v`, `agnus_blitter_minterm.v`, and `agnus_blitter_barrelshifter.v` contain zero `aga` references.
- The copper. Same two PCs (COP1LC, COP2LC), same MOVE/WAIT/SKIP instructions, same $3FE safe-wait. Minimig's `agnus_copper.v` contains no AGA-specific code. Copper instructions are still limited to writes to $DFF0xx–$DFF1FC (with the restriction that addresses above $DFF07F require COPCON[1] DANGER bit to be set; this is an OCS feature, unchanged on AGA).
- The disk DMA. Same MFM encode/decode, same DSKSYNC, same DSKLEN top bit (`DMAEN_MASK`). AGA adds no "CD-ROM disk mode" — the CD32 uses Akiko, not the disk DMA. Minimig's `paula_floppy.v` is chipset-agnostic.
- The audio DMA. Paula audio is word-based, 8-bit per sample on all generations. AGA adds no 16-bit audio mode. Minimig's `paula_audio.v` has no `aga` signals.
- Interrupts (INTREQ/INTENA). Exactly the same 15 interrupt lines, same priority, same bit positions. Paula is unchanged — so AGA has no "new interrupt" mechanism for FMODE-related events.
- DMA priority. Alice grants bus slots in the same priority order as ECS: refresh > disk/audio > sprite > bitplane > copper > blitter > CPU. The only difference is that a bitplane fetch slot in Alice can move 1, 2, or 4 words instead of 1, depending on FMODE.

**What Alice DOES change:**

- Bitplane DMA slot fetches 1/2/4 words per slot (FMODE-controlled).
- Sprite DMA slot fetches 1/2/4 words per slot (FMODE-controlled).
- 32-bit bitplane pointer range (20+1 bits of chip RAM address — same 21-bit range as ECS 8375, but some Alice variants on A4000/CD32 can address 2 MiB).
- DDFSTRT/DDFSTOP extended range when HARDDIS is set.
- 11-bit vertical display window via DIWHIGH (shared with ECS).
- 9-bit horizontal display window H8 (ECS) + H9 (AGA) via DIWHIGH.
- BPLCON0 bit 4 reinterpreted as BPU3 (bit 4 was unused on OCS/ECS).
- BPLCON1 fully 16 bits (upper byte was a fixed pattern on OCS/ECS).

This is why you sometimes see "AGA is just an Agnus and Denise upgrade" — the other half of the custom chipset is unchanged.

### 2.1 FMODE — register $DFF1FC

FMODE is Alice's master configuration register. It didn't exist on OCS/ECS; writes are ignored unless AGA is present. Minimig gates it: `if (aga && reg_address == FMODE)` `(agnus_bitplanedma.v:307, agnus_spritedma.v:152, denise_bitplanes.v:152, denise_sprites.v:101)`. The four chips that care about FMODE all latch their own private copy — it's broadcast. WinUAE masks on write: `v &= 0xC00F` `(custom.cpp:3886)`. Any other FMODE bit is documented as "not used".

Effective FMODE bit layout, reconciling Minimig and WinUAE `(include/custom.h — no macro, inferred)`:

| Bit | Name | Meaning | Authority |
|-----|------|---------|-----------|
| 15 | BPAGEM / SSCAN2 | Sprite "scan-double" — if set and vpos.0 != vstart.0, suppress fetch this line. Per-sprite gated by SPRx SH10 (`sprposh`). | `agnus_spritedma.v:252` |
| 14 | BPL32 (bitplane scandouble) | Alternate BPL2MOD/BPL1MOD per line for scandoubled interleaved bitplane fetching. Minimig uses this for scandoubling `(agnus_bitplanedma.v:478)`. | Minimig |
| 13:4 | reserved / unused | WinUAE masks these off | `custom.cpp:3886` |
| 3 | SPR32 | 64-bit sprite fetch (4 words per DMA slot) when bits 3:2 = 11 | `agnus_spritedma.v:161` |
| 2 | SPAGEM | 32-bit sprite fetch (2 words per DMA slot) when bits 3:2 = 01 or 10 | `agnus_spritedma.v:163` |
| 1 | BPL32 (bitplane width) | 64-bit bitplane fetch when bits 1:0 = 11 | `agnus_bitplanedma.v:314` |
| 0 | BPAGEM (bitplane width) | 32-bit bitplane fetch when bits 1:0 = 01 or 10 | `agnus_bitplanedma.v:313` |

Note: the HRM documentation of FMODE bits uses the names BPL32/BPAGEM/SPR32/SPAGEM from the AmigaOS headers. WinUAE refers to them as `fetchmode_fmode_bpl = fm & 3` (bits 1:0) and `fetchmode_fmode_spr = (fm >> 2) & 3` (bits 3:2) `(custom.cpp:1076-1077)`. Both sources agree: `00 = 1 word, 01 or 10 = 2 words, 11 = 4 words`. Bits 14 and 15 are the scandouble bits.

Reset value: all zero (16-bit fetch, 16-bit sprites, no scandouble). Matches OCS/ECS behaviour so a clean boot with FMODE untouched is 100% OCS-compatible.

### 2.2 Bitplane fetch widths and DMA slot allocation

This is the single biggest Alice change. On OCS/ECS each bitplane DMA slot fetches one 16-bit word. Alice can fetch 2 words (32-bit) or 4 words (64-bit) per slot, directly from chip RAM. This is what gives AGA its bandwidth: 16-colour HIRES uses the same number of DMA slots on AGA at 32-bit fetch as it did on OCS at 16-bit.

Pointer increment per fetch `(agnus_bitplanedma.v:487–491)`:

```
fmode[1:0] == 2'b11 ? 3'd4 :  // 64-bit fetch → +4 words
fmode[1:0] == 2'b00 ? 3'd1 :  // 16-bit fetch → +1 word
                      3'd2    // 32-bit fetch → +2 words
```

Modulo addition `(agnus_bitplanedma.v:487)`: the modulo is added at the end of the fetch sequence, same as OCS/ECS, but the +1/+2/+4 word extra is baked into the pointer increment already.

Maximum bitplane count per resolution (`fm_maxplane` table in `custom.cpp`, also derivable from Minimig's `plane` encoder `(agnus_bitplanedma.v:457–468)`):

| Resolution | FMODE 00 (16-bit) | FMODE 01/10 (32-bit) | FMODE 11 (64-bit) |
|------------|-------------------|----------------------|-------------------|
| LORES (140 ns pixel) | 8 planes | 8 planes | 8 planes |
| HIRES (70 ns pixel) | 4 planes | 8 planes | 8 planes |
| SHRES (35 ns pixel) | 2 planes | 4 planes | 8 planes |

The Minimig encoder explicitly lists these eight cases:

```verilog
if (shres && bp_fmode0)                                plane = {4'b0000, ~ddfseq[0]};                          // 2 bpls
else if ((hires && bp_fmode0) || (shres && bp_fmode12)) plane = {3'b000, ~ddfseq[0], ~ddfseq[1]};              // 4 bpls
else if ((!shres && !hires && bp_fmode0) || ...)        plane = {2'b00, ~ddfseq[0], ~ddfseq[1], ~ddfseq[2]};   // 8 bpls, no free slots
else if ((!shres && !hires && bp_fmode12) || ...)       plane = {1'b0, ddfseq[3], ~ddfseq[0], ...};            // 8 bpls, 8 free
else                                                    plane = {ddfseq[4], ddfseq[3], ...};                   // 8 bpls, 24 free
```
`(agnus_bitplanedma.v:457–468)`

The "free cycles" count is the emulator-relevant number: it tells the copper/blitter/CPU how much DMA bandwidth they have outside bitplane slots. LORES + FMODE 11 gives 24 free cycles per 8-slot fetch sequence, meaning a 256-colour lores game leaves most of the bus for the copper and CPU. This is why 256-colour lores was practical on AGA and 256-colour hires (FMODE 01/10, 0 free cycles) was not for anything needing much blitter.

### 2.3 Extended horizontal scroll (BPLCON1)

AGA expands BPLCON1 from 8 bits to 16. The old OCS/ECS scroll values were 4 bits per playfield (PF1H and PF2H, lores pixels 0..15). AGA has 6 bits per playfield, giving 64 lores pixels of scroll — enough to scroll a full FMODE-11 fetch unit. Layout `(denise_bitplanes.v:118, 130)`:

```verilog
pf1h <= {bplcon1[11:10], bplcon1[3:0], bplcon1[9:8]};
pf2h <= {bplcon1[15:14], bplcon1[7:4], bplcon1[13:12]};
```

That is: the scroll value for playfield 1 is bplcon1[11:10] (high 2) concatenated with bplcon1[3:0] (middle 4) concatenated with bplcon1[9:8] (low 2). OCS used only bits 3:0 and 7:4. AGA adds the high 2 and low 2 from the upper byte. The bit ordering is non-obvious — **do not copy the OCS scroll handler and just widen the field; it will misdecode**.

On non-AGA, Minimig forces the upper byte to a fixed `2'b00, 2'b11, 2'b00, 2'b11` pattern `(denise_bitplanes.v:144)` — this is the magic that makes the upper-half bits act as zeros from the scroll pipeline's point of view.

### 2.4 Display data FIFO size and extra-delay handling

Alice feeds Lisa a deeper FIFO (effectively 8 words per plane, enough for a 4-fmode 64-bit fetch). Minimig's `bpl1dat`..`bpl8dat` are each declared as `reg [63:0]` `(denise_bitplanes.v:59–66)` — 64 bits, so four consecutive words of chip RAM per plane per fetch boundary. When BPLxDAT is written by the DMA engine, the full 48-bit tail is latched from `chip48_fmode` and concatenated with the 16-bit word on the bus.

The "extra delay" logic `(denise_bitplanes.v:84–110)` deals with display data being fetched at a point not aligned to the display-window start. OCS/ECS had a 4-lores-pixel granularity; AGA extends this up to 16 pixels in FMODE 11. The extra-delay calculation inspects the low bits of hpos at load time and computes a shift offset. FMODE 00 uses bits hpos[3:2], FMODE 01/10 uses hpos[4:3], FMODE 11 uses hpos[5:4]. These are the alignment-sensitivity bits.

This extra-delay table is why CopperList tricks that worked on OCS can glitch on AGA: a demo that relies on a specific pixel-level DDF pre-start will display shifted pixels on AGA in 32/64-bit fetch mode. Emulators that use a software bitplane fetch need to apply this exact delay or the demo will look wrong.

**Bitplane shifter pipeline**: Each bitplane has a dedicated parallel-to-serial converter (`denise_bitplane_shifter.v`). The shifter architecture on AGA is:

1. **Main shifter** (64-bit `shifter` register): loaded from BPLxDAT on `load` signal; shifts left by 1 on each shift clock. The shift rate depends on resolution `(denise_bitplane_shifter.v:66-79)`:
   - LORES: shift once per 4 clocks (`~c1 & ~c3`)
   - HIRES: shift once per 2 clocks (`~c1 ^ c3`)
   - SHRES: shift every clock (`1'b1`)

2. **Scroller** (64-bit `scroller` register): receives bits from the main shifter's MSB on each shift clock `(denise_bitplane_shifter.v:96-101)`:
   ```verilog
   if (shift) scroller[63:0] <= {scroller[62:0], shifter[63]};
   ```
   The scroller output is selected by the `select` value (scroll amount), which is FMODE-masked `(denise_bitplane_shifter.v:57-63)`:
   ```verilog
   case(fmode[1:0])
       2'b00 : fmode_mask = 6'b00_1111;   // 16-pixel scroll range
       2'b01,
       2'b10 : fmode_mask = 6'b01_1111;   // 32-pixel scroll range
       2'b11 : fmode_mask = 6'b11_1111;   // 64-pixel scroll range
   endcase
   ```

3. **Super-hires scroller** (8-bit `sh_scroller` register): a second-level shift register that captures the scroller output and provides the final pixel output. This level handles sub-CCK pixel selection for HIRES and SHRES modes `(denise_bitplane_shifter.v:108-127)`.

The scroll range increases with FMODE because the scroller is wider (64 positions). At FMODE 00, only 16 positions are selectable (mask `00_1111`). At FMODE 11, all 64 positions are available, giving a full 64-lores-pixel scroll range — enough to scroll through one entire 64-bit fetch block without a copper trick.

The odd/even bitplane assignment (which planes use PF1 scroll vs PF2 scroll) is hardwired in the instantiation `(denise_bitplanes.v:261-388)`: planes 1,3,5,7 use `pf1h_del` (playfield 1 scroll), planes 2,4,6,8 use `pf2h_del` (playfield 2 scroll). This is unchanged from OCS.

### 2.5 DDFSTRT/DDFSTOP extended range

OCS DDF range was $18..$D8 (hard-enable window). ECS and AGA allow DDF to start as early as $0018 and stop as late as $00E0 when HARDDIS is set in BEAMCON0 (see section 12). Minimig implements this via a separate `hardena` signal: `if (hard_start) hardena <= 1; else if (hard_stop) hardena <= 0;` with start at $18 and stop at $D8 `(agnus_bitplanedma.v:359, 369)`.

AGA and ECS also allow DDFSTRT/DDFSTOP to use bit 2 (bit 1 of the word value) for 2-CCK granularity. OCS ignores that bit — it's always aligned to 4 CCKs. Minimig gates bit 2 on ECS `(agnus_bitplanedma.v:339)`.

**Watch for**: DDFSTRT==DDFSTOP is a trap. OCS runs the fetch to hard_stop. ECS/AGA stop after one complete fetch cycle. Minimig comments the difference explicitly `(agnus_bitplanedma.v:378–390)`:

```verilog
// AMR - OCS and ECS/AGA behave differently when DDFSTRT == DDFSTOP.
// On OCS the fetch runs until hard_stop. On ECS/AGA it stops after one complete fetch cycle.
reg softena_off;

always @ (posedge clk) begin
  if (clk7_en) begin
    if (hpos[0]) begin
      if (soft_start && (ecs || vdiwena && dmaena) && !ddfstrt_sel)
        softena <= 1;
      else if (softena_off || (soft_stop || !ecs && hard_stop))
        softena <= 0;
      softena_off <= soft_stop && soft_start && ecs;
    end
  end
end
```

`softena_off` is the "one-cycle-later" disable that makes DDFSTRT==DDFSTOP work as a one-fetch window on ECS/AGA. The same code also implements the subtle rule "OCS display can start only when vdiwena condition is true" via `(ecs || vdiwena && dmaena)`.

### 2.6 Bitplane pointer arithmetic and pointer-write side effects

Alice retains the 21-bit chip-RAM bitplane pointer (BPL1PT..BPL8PT). There is no "extended pointer" register. AGA simply uses the wider fetch in place of multiple small fetches. Minimig's pointer write path `(agnus_bitplanedma.v:218-242)`:

```verilog
assign bplptr_sel = dma ? plane[2:0] : reg_address_in[4:2];
assign bplpth_in  = dma ? newpt[20:16] : data_in[4:0];

always @ (posedge clk) begin
  if (clk7_en) begin
    if (dma || ((reg_address_in[8:5]==BPLPTBASE_REG[8:5]) && !reg_address_in[1]))
      bplpth[bplptr_sel] <= bplpth_in;
  end
end
```

Notes:
- CPU writes to BPLxPTH mask the incoming data to 5 bits. Writing above $1F is clipped — the emulator must not accept higher-than-chip-RAM pointer writes.
- During a DMA cycle, the bitplane pointer register bank is addressed by `plane[2:0]` (which plane is being fetched), so the CPU cannot write to a bitplane pointer *during* a DMA slot for that same plane. This is a hardware race; software that tries to update pointers on the fly has to sync to hpos.
- BPLxPTH (high word) and BPLxPTL (low word) are two different registers and *must both* be written. Writing only BPLxPTL works if the high word is already correct; writing only BPLxPTH leaves the low word intact. A common bug is to write BPLxPTH when changing double-buffered frames but forget BPLxPTL, getting a screen shifted by the old pointer's low word.

The "delayed BPLCON0 by 3 CCKs" behaviour documented in Minimig `(agnus_bitplanedma.v:286-296)`:

```verilog
// delay by 8 clocks (in real Amiga DMA sequencer is pipelined
// and features a delay of 3 CCKs)
always @ (posedge clk) begin
  if (clk7_en) begin
    if (hpos[0]) begin
      bplcon0_delay[0] <= bplcon0;
      bplcon0_delay[1] <= bplcon0_delay[0];
      bplcon0_delayed  <= bplcon0_delay[1];
    end
  end
end
```

This 3-CCK pipeline delay affects all emulators that want cycle-accurate bitplane behaviour. If you write BPLCON0 (SHRES/HIRES/BPU) on a given CCK, the DMA engine does not see the new value until 3 CCKs later. Demos that toggle between hires and lores mid-scanline depend on this delay being exactly 3 CCKs — 2 CCKs is visibly wrong.

### 2.7 Alice interlace / progressive

Interlace is controlled by BPLCON0 bit 2 (LACE) — unchanged from OCS. The field flipping and VSSTOP long-frame logic is unchanged `(agnus_beamcounter.v:318–342)`. FMODE bit 14 (the bitplane scandouble bit) gives a new interlace variation: it swaps BPL1MOD and BPL2MOD on alternating lines (`vdiwstrt.0 ^ vpos.0`), used by "fake interlace" that scandoubles a progressive display `(agnus_bitplanedma.v:478–480)`:

```verilog
always @(posedge clk) begin
    if (clk7_en && hpos[0]) begin
        bpl1mod_bscan <= fmode[14] ? ((vdiwstrt[0] ^ vpos[0]) ? bpl2mod : bpl1mod) : bpl1mod;
        bpl2mod_bscan <= fmode[14] ? ((vdiwstrt[0] ^ vpos[0]) ? bpl2mod : bpl1mod) : bpl2mod;
    end
end
```

The effect of this is that in scandoubled mode, alternate scanlines add a *different* modulo — giving you effectively "two interleaved bitplane data streams" in a single non-interlaced display. Paired with "RAMJAM: Copperslave" as Minimig calls out by name `(agnus_bitplanedma.v:69)`, this is how scandoubler trick demos get 640x480 progressive from a 640x200 fetch by reinterpreting every other scanline as the "other field's" modulo. The practical outcome for an emulator: if a game sets FMODE bit 14, you must check line parity and swap modulos.

AGA does not add true progressive-in-PAL-timing support. The "progressive 640x480 AGA mode" advertised for A4000 came from ECS's programmable BEAMCON0, not from Alice per se. A proper PAL 50-Hz progressive mode is set up as: BEAMCON0=$0BE8 (VARBEAMEN+HARDDIS+VARVBEN+VARVSYEN+VARHSYEN+BLANKEN+HSYTRUE), HTOTAL=$E3, VTOTAL=$271 (625), and all the sync positions programmed. That's "programmed display mode", not "AGA display mode".

### 2.8 DMA sequencer (ddfseq) in detail

The bitplane DMA sequencer `ddfseq` in Alice counts fetch positions within a "fetch block". One full DMA sequence is 8 CCKs. Minimig `(agnus_bitplanedma.v:433–441)`:

```verilog
always @ (posedge clk) begin
  if (clk7_en) begin
    if (hpos[0])
      if (ddfrun)
        ddfseq <= ddfseq + 5'd1;
      else
        ddfseq <= 5'd0;
  end
end
```

The `ddfseq` counter is 5 bits wide (0..31) because FMODE 11 lores has 32-slot sequences (8 planes + 24 free cycles). The `ddfseq_match` signal determines when a full sequence has completed `(agnus_bitplanedma.v:417–419)`:

```verilog
assign ddfseq_match =
  ((!hires && !shres && bp_fmode3)                        && (ddfseq[4:0] == 5'd7))  ||
  (((!shres && !hires && bp_fmode12)||(hires && bp_fmode3)) && (ddfseq[3:0] == 4'd7))  ||
  (!(...) && !(...)                                         && (ddfseq[2:0] == 3'd7));
```

Breaking this down by mode:

| Mode | Sequence length | ddfseq match | Free cycles per 8-CCK block |
|------|-----------------|--------------|----------------------------|
| LORES + FMODE 0 | 8 CCKs | ddfseq[2:0]==7 | 0 (all 8 used for 8 planes) |
| LORES + FMODE 1/2 | 16 CCKs | ddfseq[3:0]==7 | 8 free |
| LORES + FMODE 3 | 32 CCKs | ddfseq[4:0]==7 | 24 free |
| HIRES + FMODE 0 | 8 CCKs | ddfseq[2:0]==7 | 0 (4 planes max, 4 plane-slots + 4 free if <4 bpl) |
| HIRES + FMODE 1/2 | 8 CCKs | ddfseq[2:0]==7 | 0 |
| HIRES + FMODE 3 | 16 CCKs | ddfseq[3:0]==7 | 8 free |
| SHRES + FMODE 0 | 8 CCKs | ddfseq[2:0]==7 | 0 (2 planes max) |
| SHRES + FMODE 1/2 | 8 CCKs | ddfseq[2:0]==7 | 0 (4 planes max) |
| SHRES + FMODE 3 | 8 CCKs | ddfseq[2:0]==7 | 0 (8 planes max) |

The plane number within a sequence is computed from `ddfseq` using the inverted low bits `(agnus_bitplanedma.v:457-468)`:

```
plane[2:0] = {~ddfseq[0], ~ddfseq[1], ~ddfseq[2]}
```

For the first 8 CCKs of any sequence, this gives plane numbers 7, 6, 5, 4, 3, 2, 1, 0 — fetching plane 8 first, then plane 7, down to plane 1. This matches hardware: the highest-numbered bitplane is fetched first in each block. An emulator that fetches planes in ascending order will display correctly (the shifters don't care about load order) but will have incorrect DMA slot timing for copper/CPU contention.

For extended sequences (FMODE 12 lores, FMODE 3 lores/hires), the upper bits of `ddfseq` (bits 3, 4) distinguish "plane data" cycles from "free" cycles. `plane[4] = ddfseq[4], plane[3] = ddfseq[3]` — when either is non-zero, the plane number exceeds the maximum and the DMA test `plane < bpu` fails, making those cycles available to blitter and CPU. The DMA signal itself is: `assign dma = ddfrun && dmaena_delayed[1] && hpos[0] && (plane[4:0] < {1'b0, bpu[3:0]})` `(agnus_bitplanedma.v:472)`.

**Emulator consequence**: you must model the exact sequencer if copper-driven mid-scanline BPU changes are to work. A common technique is to set BPU=0 during the border (freeing all slots for the copper) and BPU=8 during the display, changing mid-sequence. The 3-CCK pipeline delay on BPLCON0 (section 2.6) means the new BPU value arrives 3 CCKs after the copper write. If your sequencer uses a simpler model (e.g. "8 planes from DDF start to DDF stop"), mid-scanline colour changes via copper+BPU will be visually off.

### 2.9 DMA timing: DDFSTRT start-condition 4-CCK lookahead

Minimig's comment `(agnus_bitplanedma.v:334)` clarifies: "ddf start condition is checked 4 CCKs before the first bitplane data fetch." The `soft_start` signal is set 2 CCKs before the actual fetch; `ddfena_0` and `ddfena` add 2 more CCKs of pipeline `(agnus_bitplanedma.v:406-414)`:

```verilog
always @ (posedge clk) begin
  if (clk7_en) begin
    if (hpos[0]) begin
      ddfena_0 <= (hardena || harddis) && softena;
      ddfena   <= ddfena_0;
    end
  end
end
```

Total delay from DDFSTRT match to first fetch: 4 CCKs. For a cycle-exact emulator, this means DMA slots do not start consuming bandwidth until 4 CCKs after the copper writes DDFSTRT. A demo that sets DDFSTRT to the "current position" will not see data for 4 more CCKs — this is observable as "2 lores pixels of border colour before the first display pixel".

There is also a subtle interaction: "writing DDFSTRT register when hpos==ddfstrt doesn't start the bitplane DMA" `(agnus_bitplanedma.v:335)`. The `ddfstrt_sel` signal blocks the soft_start condition during the same cycle as the write. This prevents a race where the copper writes DDFSTRT and the start condition fires in the same CCK.

### 2.10 Alice fetch "hde" signal

An emulator-specific detail worth knowing: Minimig generates a `hde` (horizontal display enable) signal one scanline in advance `(agnus_bitplanedma.v:151-178)`. It tracks the widest horizontal display window seen so far in the frame, and uses that value one line ahead to enable DMA. This is a pipeline optimisation — Alice on real silicon does not do this, but the observable behaviour is identical. Emulators doing cycle-exact bitplane modelling should use the current-line DIWSTRT/DIWSTOP directly; the "best_hdiwstrt" tracking in Minimig is a Minimig-specific FPGA quirk, not hardware behaviour.

### 2.11 DMA priority and bus arbitration on Alice

The DMA priority chain is identical to OCS/ECS. Minimig's `agnus.v` lines 131-225 implement the full priority ladder as a cascaded if/else:

```
1. Disk DMA (dma_dsk)           — highest priority
2. Refresh DMA (dma_ref)
3. Audio DMA (dma_aud)
4. Bitplane DMA (dma_bpl)
5. Sprite DMA (dma_spr)         — requires request + enable
6. Copper DMA (dma_cop)         — requires request + enable + bus free from bpl
7. Blitter DMA (dma_blt)        — requires request + enable + bls_cnt check
8. CPU (cpu_custom = 1)         — lowest priority, gets bus when all above idle
```

The `bls_cnt` counter (blitter slowdown) counts consecutive cycles where the CPU misses the bus `(agnus.v:402-406)`:

```verilog
always @(posedge clk) if (clk7_en) begin
    if (!cck)
        if (!bls || bltpri) bls_cnt <= 2'b00;
        else if (bls_cnt[1:0] != BLS_CNT_MAX) bls_cnt <= bls_cnt + 2'b01;
end
```

When `bls_cnt` reaches `BLS_CNT_MAX` (3), the blitter is blocked even if the bus is technically available — giving the CPU a guaranteed minimum of every 4th cycle. This is the "blitter nasty but not too nasty" behaviour. The blitter priority bit (`DMACON[10]`, `bltpri`) resets the counter to zero, giving the blitter absolute priority until the CPU gets lucky.

Alice does not change any of this. The only difference is that a bitplane slot in Alice can move more data, so the *effective* CPU bandwidth is higher at the same number of bitplanes because fewer DMA slots are consumed. For example, 8-plane lores at FMODE 00 uses 8 DMA slots per 8-CCK block (zero free). The same display at FMODE 11 uses 8 DMA slots per 32-CCK block (24 free) — 3x more CPU time. This is why AGA games feel faster than OCS games at the same bitplane count.

### 2.12 Refresh DMA

Refresh is unchanged from OCS. Minimig's `agnus_refresh.v` generates refresh slots at fixed horizontal positions. Refresh DMA occupies the bus but does not transfer useful data — it drives the chip-RAM DRAM refresh cycle. On real hardware, each refresh slot addresses a row of DRAM using a counter; in an emulator, refresh slots only matter for DMA contention modelling (they block the CPU and blitter for that slot).

### 2.13 Sprite DMA slot timing (Alice perspective)

Sprite DMA uses even cycles from hpos $18 to $38 (inclusive), unchanged from OCS `(agnus_spritedma.v:266-272)`:

```verilog
always @ (posedge clk) begin
  if (clk7_en) begin
    if (hpos[8:1]==8'h18 && hpos[0])
      enable <= 1;
    else if (hpos[8:1]==8'h38 && hpos[0])
      enable <= 0;
  end
end
```

Each of the 8 sprites gets two DMA slots spaced 4 clocks apart. The sprite number is derived from hpos `(agnus_spritedma.v:276-279)`:

```verilog
always @ (posedge clk) begin
  if (clk7_en) begin
    if (hpos[2:0]==3'b001)
      sprite[2:0] <= {hpos[5]^hpos[4], ~hpos[4], hpos[3]};
  end
end
```

During the first slot (`hpos[2]` = 1), the DMA engine writes to SPRxPOS (if fetching position/control) or SPRxDATA (if fetching image data). During the second slot (`hpos[2]` = 0), it writes SPRxCTL or SPRxDATB respectively.

What changes on Alice is the pointer increment per slot. FMODE bits 3:2 control how many words are fetched `(agnus_spritedma.v:159-165)`:

```verilog
case(fmode[3:2])
    2'b00   : spr_fmode_ptradd = 3'd1;  // +1 word (16-bit sprite)
    2'b11   : spr_fmode_ptradd = 3'd4;  // +4 words (64-bit sprite)
    default : spr_fmode_ptradd = 3'd2;  // +2 words (32-bit sprite)
endcase
```

The sprite pointer is incremented by this amount after each DMA slot. The slot timing itself (hpos $18..$38) remains fixed — Alice cannot allocate more slots for sprites. You get the same 8 sprites with the same DMA window; each sprite's data is simply wider. This means that increasing sprite width from 16 to 64 pixels does not use more DMA bandwidth in terms of *slots*, but each slot consumes more chip-RAM bandwidth (the data bus is occupied for longer during the slot).

---

## 3. AGA — Lisa (Denise successor)

Lisa is Denise with a wider colour path (8 bits per gun), a 256-entry colour table, wide sprite shifters (32/64 pixel sprites), 8-bitplane support, and the BPLCON3/BPLCON4 register additions. The interlace, sync generation, collision detection, and HAM core are Denise-compatible.

### 3.1 Colour table: 256 entries × 24 bits

OCS/ECS had 32 colour registers at $DFF180..$DFF1BE, each storing 12 bits (4 bits per gun). Lisa has a 256-entry table, banked 32 at a time. The selection is:

- **Address (during CPU write)** = `{BPLCON3[15:13], reg_address[5:1]}`. That is, BPLCON3 bits 15:13 (the BANK field) choose which 32-entry bank the standard $180..$1BE slot writes go to `(denise_colortable.v:30)`.
- **Address (during display)** = the 8-bit pixel value from the bitplane serialiser, XORed with BPLCON4 high byte `(denise.v:456, denise_colortable.v:34)`. The colour address is the pixel value — there is no banking during readout. The BPLCON3 bank only affects writes.

This means **you write the palette a bank at a time**. Typical 256-colour setup is: BPLCON3 = bank 0; write colours 0..31 to $180..$1BE; BPLCON3 = bank 1; write 32..63 to $180..$1BE; ... repeat 8 times. The copper can do this per scanline if it needs to.

### 3.2 24-bit colour via LOCT

The colour-table RAM is physically 12 bits wide, but AGA stores 24 bits per entry. How? By double-writing.

`(denise_colortable.v:32–33)`:

```verilog
wr_dat = {4'b0, data_in[11:0], 4'b0, data_in[11:0]};  // duplicated high/low
wr_bs  = loct ? 4'b0011 : 4'b1111;                     // byte enable
```

The colour RAM is 32 bits wide internally, storing `{pad, colour_hi(12), pad, colour_lo(12)}`.

- **LOCT=0 (BPLCON3 bit 9 clear)**: write is to both high and low halves of the 32-bit RAM word. The high half holds the 12-bit value, and the low half *also* holds the same 12-bit value (but those bits are the low-nibble-of-each-gun). This mimics OCS/ECS behaviour: a single write sets all 12 bits visible in 12-bit mode.
- **LOCT=1 (BPLCON3 bit 9 set)**: write affects only the low half (byte-enables `4'b0011` zero out the write to the high half). So the sequence to set a 24-bit colour is:

  1. BPLCON3 bits 9 = 0. Write RGB high nibbles to $180+n (top 4 bits of each gun).
  2. BPLCON3 bit 9 = 1. Write RGB low nibbles to $180+n (bottom 4 bits of each gun).

At readout time, Minimig reconstructs the 24-bit colour from the 32-bit RAM word `(denise_colortable.v:57–59)`:

```verilog
color = {color_hi[11:8], color_lo[11:8], color_hi[7:4], color_lo[7:4], color_hi[3:0], color_lo[3:0]};
```

That is, each gun's 8 bits are `{colour_hi_high_nibble, colour_lo_high_nibble}` — the high nibble of the high word becomes the high 4 bits of the gun, and the high nibble of the low word becomes the low 4 bits. This gives you R[7:0] = {R_high_word[11:8], R_low_word[11:8]}, and similarly for G and B.

**Emulator consequence**: naive "write $RGB to colour register" works for 12-bit palette. For 24-bit palette, writes come in pairs and an emulator that forgets to track LOCT will show 12-bit colour in 24-bit games (noticeably wrong on gradients). If LOCT is set but only one write happens, the effect is "this colour has its high bits from an old value".

**Colour table RAM architecture**: Minimig implements the 256-entry colour table as a dual-port 256x32-bit RAM `(denise_colortable_ram_mf.v)`. The RAM has independent read and write addresses, 4-byte byte-enables for LOCT support, and is clocked at the 28 MHz system clock. The 256 entries are addressed by an 8-bit address: write address = `{bank[2:0], reg_address[5:1]}`, read address = the pixel colour index (or EHB index for half-brite) `(denise_colortable.v:30-34)`.

The byte-enable trick for LOCT is elegant: each 32-bit RAM word stores `{pad4, colour_hi12, pad4, colour_lo12}`. When LOCT=0, all 4 byte-enables are active (`4'b1111`), so both halves are written with the same 12-bit value. When LOCT=1, only the low 2 byte-enables are active (`4'b0011`), so only `colour_lo12` is updated. This avoids a read-modify-write cycle: the write path doesn't need to read the existing high bytes before writing the low bytes.

**EHB (Extra Half Brite) in the colour table**: When EHB mode is active and bit 5 of the pixel value is set, the colour table reads from the base 32 entries (`{3'b000, select_xored[4:0]}`) and then halves each gun by shifting right one bit `(denise_colortable.v:62-64)`:

```verilog
if (ehb_sel && ehb_en)
    rgb = {1'b0, color[23:17], 1'b0, color[15:9], 1'b0, color[7:1]};
else
    rgb = color;
```

This is a right-shift-by-1 on each 8-bit gun channel, with the MSB forced to zero. The result is exactly half brightness of the base colour. AGA preserves this behaviour unchanged from OCS — 6 bitplanes without HAM or dual-playfield gives EHB, even in 24-bit colour mode. The half-bright calculation operates on the 24-bit output, giving smooth half-brightness gradients that weren't possible on 12-bit OCS.

### 3.3 BPLCON2 dual-playfield priority offset (AGA addition)

BPLCON2 on OCS controlled playfield priority and sprite-vs-playfield priority. AGA adds a 3-bit field PF2OF (BPLCON2 bits 12:10 — often called "Playfield 2 colour offset in dual-playfield mode"). Minimig: `assign pf2of = bplcon3[12:10];` `(denise.v:213)`. When both playfields are on-screen in dual-playfield mode, the PF2 pixels are not just offset by 8 colours (as on OCS) but by a programmable amount, allowing 16-colour+16-colour dual playfield (instead of OCS's 8+8). This is the mechanism behind "Lionheart" and similar AGA games' fancy backgrounds.

Also new: BPLCON2 bit 8 (`rdram`) enables *reading* the colour table rather than writing it. When `rdram` is set and the CPU reads $180+n (within the current BANK), the register bus returns the 12-bit or 24-bit content instead of ignoring the read. Minimig `(denise.v:184–188)`:

```verilog
assign rdram = bplcon2[8] & aga;
assign rgb_out = (reg_address[8:6] == COLORBASE[8:6]) && rdram
                   ? {4'b0, loct ? {clut_rgb[19:16], clut_rgb[11:8], clut_rgb[3:0]}
                                 : {clut_rgb[23:20], clut_rgb[15:12], clut_rgb[7:4]}}
                   : 16'h0000;
```

Note how the read path re-packs the 24-bit stored value back into a 12-bit-per-read form — with LOCT selecting whether the low or high nibble is returned. A colour save-and-restore routine on AGA must read twice (once with LOCT=0, once with LOCT=1) to capture the full 24 bits.

### 3.4 BPLCON3 bit layout (AGA)

The AGA BPLCON3 register at $DFF106. Reset value from Minimig `(denise.v:206)` is `16'h0C00` (that's bank 0, PF2OF = 001, rest zero).

| Bits | Name | Meaning | Gated on |
|------|------|---------|----------|
| 15:13 | BANK[2:0] | Colour-table bank select for writes (and HAM base) | AGA |
| 12:10 | PF2OF[2:0] | Playfield-2 colour-offset in dual playfield mode: 0=none, 1=2, 2=4, 3=8, 4=16, 5=32, 6=64, 7=128 | AGA |
| 9 | LOCT | Palette access low-word select (24-bit writes) | AGA |
| 8 | — | reserved | — |
| 7:6 | SPRES[1:0] | Sprite resolution: 00 = matches BPLCON0 resolution, 01 = LORES, 10 = HIRES, 11 = SHRES | AGA |
| 5 | BRDRBLNK | Border area is blank-black instead of colour 0 | ECSENA |
| 4 | BRDNTRAN | Border area is marked non-transparent (for genlock) | ECSENA |
| 3 | ZDCLKEN | ZD pin outputs a clock rather than a level | ECSENA |
| 2 | BRDSPRT | Enable sprites in the border area (outside display window) | ECSENA |
| 1 | — | reserved | — |
| 0 | EXTBLKEN | External blank input is used to generate BLANK, not computed from DIW | ECSENA |

"Gated on AGA" means Minimig masks the bit with `& aga`: on OCS/ECS the write is accepted but the bit has no effect. "Gated on ECSENA" means both ECS and AGA respect the bit, but only when BPLCON0 bit 0 is set — this is to keep OCS software safe (writing to BPLCON3 on OCS doesn't do anything because BPLCON3 didn't exist; on ECS/AGA, software must opt in via ECSENA to the new features).

Minimig source for the decode `(denise.v:212-220)`:

```verilog
assign bank     = bplcon3[15:13] & {3{aga}};
assign pf2of    = bplcon3[12:10];
assign loct     = bplcon3[9] & aga;
assign spres    = bplcon3[7:6] & {2{aga}};
assign brdrblnk = bplcon3[5] & ecsena;
assign brdsprt  = bplcon3[1] & ecsena;
// brdntran, zdclken, extblken wired but commented out in Minimig
```

The trio BRDRBLNK / BRDNTRAN / EXTBLKEN are AGA's attempt to make genlocking rigorous: border-blank says "during the border, force the video signal to blank-black (not colour 0, which might be something visible)"; border-non-transparent says "the border is opaque to the genlock"; external-blank-enable lets a genlock drive the blank line.

### 3.5 BPLCON4 bit layout (AGA)

BPLCON4 at $DFF10C. Reset `16'h0011` per Minimig `(denise.v:231)` — OSPRM/ESPRM = 0001/0000.

| Bits | Name | Meaning |
|------|------|---------|
| 15:8 | BPLAM[7:0] | Bitplane XOR mask — each pixel value is XORed with this before colour lookup. Used for "instant palette swap". Active from first BPL1DAT write until DIWSTOP `(denise.v:238-246)`. |
| 7:4 | ESPRM[3:0] | Even sprite (0,2,4,6) colour base: high 4 bits of colour index |
| 3:0 | OSPRM[3:0] | Odd sprite (1,3,5,7) colour base: high 4 bits of colour index |

So sprites no longer live at colours $10–$1F fixed. On AGA, sprite 0/1 uses colours starting at `{OSPRM, 0000}` — meaning a sprite can use any 16-entry group in the 256-colour table. Paint an 8-sprite display by giving each sprite pair a different 16-colour block and you get 8 × 16 = 128 sprite colours on screen at once (minus the 1 transparent entry per pair = 112 visible).

Minimig sprite-colour assembly `(denise_sprites.v:284)`:

```verilog
sprdata[7:0] = {osprm, sprdat1[1:0], sprdat0[1:0]};  // attached pair
```

The top 4 bits come from ESPRM/OSPRM, then the 4 data bits from the two attached sprites. For non-attached pairs, the middle 2 bits are sprite-pair-number (00, 01, 10, 11) so colours $10/$14/$18/$1C etc. follow the OCS layout.

**BPLAM subtlety**: it only applies during the active display window. Outside DIW (in the border), the XOR is zero. Minimig: `if (hpos == hdiwstop) bplxor <= 0; else if (display_ena) bplxor <= bplcon4[15:8];` `(denise.v:241–244)`. A demo that tries to use BPLAM for border colour shifting won't see the effect in the border unless BRDSPRT or BRDRBLNK is also used.

### 3.6 EHB and HAM8 on Lisa

- **EHB (Extra Half Brite)**: still gated on 6-bitplane mode (`l_bpu == 4'd6`) and not dual-playfield and not A1000 `(denise.v:397)`. AGA does not add a "modern EHB". It retains backwards compatibility.
- **HAM8**: 8-bitplane HAM. The top 2 bits select operation (0=set, 1=modify blue, 2=modify red, 3=modify green) and the bottom 6 bits are either the new colour-register index (0..63 in the current bank) or the modification value. Minimig `(denise.v:415)`:

  ```verilog
  wire ham8 = ham && (l_bpu == 4'd8);
  ```

  The HAM8 generator (in `denise_hamgenerator.v`) inspects the 8-bit pixel value; bits 7:6 are the HAM opcode; bits 5:0 for opcode 0 are a 6-bit index into the colour table (always bank-0 range), and bits 5:0 for opcodes 1..3 are a 6-bit replacement value (top 6 bits of the gun — the low 2 bits "hold" from the previous pixel's value). This is why HAM8 does not have a full 24-bit per-pixel replacement: an 8-bit pixel can only carry 6 bits of new-colour data.

### 3.7 HAM generator internals (HAM6 vs HAM8)

The Minimig HAM generator `(denise_hamgenerator.v:66-94)` handles both HAM6 (OCS/ECS, 6 bitplanes) and HAM8 (AGA, 8 bitplanes) in a single module. The critical difference is which bits carry the opcode and which carry the data:

**HAM8 mode** (when `ham8` is set — HAM + 8 bitplanes):

```verilog
case (select_r[1:0])          // bits 1:0 of pixel (after XOR) are opcode
  2'b00: rgb = color;         // load from colour table (index = bits 7:2)
  2'b01: rgb = {rgb_prev[23:8],  select_r[7:2], rgb_prev[1:0]};   // modify blue
  2'b10: rgb = {select_r[7:2], rgb_prev[17:16], rgb_prev[15:0]};  // modify red
  2'b11: rgb = {rgb_prev[23:16], select_r[7:2], rgb_prev[9:8], rgb_prev[7:0]}; // modify green
endcase
```

**HAM6 mode** (default, 6 bitplanes):

```verilog
case (select_r[5:4])          // bits 5:4 of pixel are opcode
  2'b00: rgb = color;         // load from colour table (index = bits 3:0 + bank)
  2'b01: rgb = {rgb_prev[23:8],  select_r[3:0], select_r[3:0]};   // modify blue (4 bits doubled)
  2'b10: rgb = {select_r[3:0], select_r[3:0], rgb_prev[15:0]};    // modify red (4 bits doubled)
  2'b11: rgb = {rgb_prev[23:16], select_r[3:0], select_r[3:0], rgb_prev[7:0]}; // modify green
endcase
```

`(denise_hamgenerator.v:66-94)`.

Key differences for an emulator:

1. **HAM6 doubles the modification nibble** (`select_r[3:0], select_r[3:0]`) to fill 8 bits per gun. HAM8 uses 6 bits directly and holds the low 2 bits from the previous pixel. This means HAM6 has an effective 4-bit per-gun modification (16 levels per colour-gun-change); HAM8 has 6-bit (64 levels).

2. **Colour table lookup path differs**: HAM8 uses only 6 bits for the index (`rd_adr = {2'b00, select_xored[7:2]}` `(denise_hamgenerator.v:32)`), selecting colours 0..63 from bank 0. HAM6 uses the full 8-bit pixel value via the normal colour table path. So HAM8's "set colour from palette" instruction only addresses 64 colours (within the current bank), while HAM6 addresses all 16 base colours (also 64 colours if using 6 planes where bits 5:4 are the opcode and bits 3:0 are the index — 16 in practice, since the colour table only has 32 base entries and the HAM opcode uses the top 2 bits).

3. **BPLXOR interaction**: The HAM generator receives `select_xored = select ^ bplxor` `(denise_hamgenerator.v:25)`. This is significant: BPLAM (from BPLCON4) affects which colour is looked up and which modification bits are used. A program that sets BPLAM in HAM mode will scramble the HAM output — BPLAM effectively re-maps which pixels are "set colour" vs "modify gun". Some demos use this intentionally for visual effects.

4. **Bank interaction**: The HAM generator has its *own* colour table RAM `(denise_hamgenerator.v:37-48)`. This is separate from the main colour table used by the playfield renderer. Both tables receive the same CPU writes (both listen on the register bus for $180..$1BE writes), but the HAM table does not use `rdram` — it is write-only. This means changing BPLCON2 RDRAM does not affect HAM output; the HAM generator always writes its internal palette regardless.

5. **HAM and sprites**: When a sprite pixel is on-screen, the sprite overrides the HAM output. The HAM generator's `rgb_prev` register continues to track from the previous HAM pixel — it does not see the sprite colour. So after a sprite "interrupts" the HAM scan, the next HAM pixel's "hold" still comes from the pre-sprite HAM colour. This is correct hardware behaviour, as Minimig confirms by only muxing HAM vs CLUT output *after* the sprite priority: `out_rgb = ham_sel && window_del && !sprsel_del ? ham_rgb : clut_rgb` `(denise.v:468)`.

6. **HAM and the display window boundary**: Minimig masks bitplane data to zero outside the display window `(denise.v:337-344)` to prevent the HAM generator from reacting to "data scrolled off the left side". This was a fix for "Desire: Hamazing" `(denise.v:333-336 comment)`. Without this mask, HAM6/HAM8 would pick up stale shift-register bits from the previous line's final pixels, causing colour bleeding at the left border.

### 3.8 Playfield logic on Lisa (AGA extensions)

Lisa's playfield module `(denise_playfields.v)` handles single and dual playfield modes for up to 8 bitplanes. The AGA changes:

**PF2OF offset table**: In AGA dual-playfield mode, playfield 2's colour is offset by a value derived from BPLCON3 bits 12:10 `(denise_playfields.v:28-37)`:

```verilog
case(pf2of)
    3'd0 : pf2of_val = 8'd0;
    3'd1 : pf2of_val = 8'd2;
    3'd2 : pf2of_val = 8'd4;
    3'd3 : pf2of_val = 8'd8;
    3'd4 : pf2of_val = 8'd16;
    3'd5 : pf2of_val = 8'd32;
    3'd6 : pf2of_val = 8'd64;
    3'd7 : pf2of_val = 8'd128;
endcase
```

The offset is a power of 2, not a linear value. PF2OF=1 adds 2 to the colour index, PF2OF=2 adds 4, PF2OF=3 adds 8, etc. This lets PF2 address a different "block" within the 256-colour palette. On OCS/ECS, PF2 always started at colour 8 (bit 3 of colour index forced to 1 `(denise_playfields.v:76)`).

AGA dual-playfield colour assembly `(denise_playfields.v:73-74)`:

```verilog
plfdata[7:0] = {4'b0000, bpldata[8], bpldata[6], bpldata[4], bpldata[2]} + pf2of_val;
```

Playfield 2 uses even-numbered bitplanes (2,4,6,8) and adds the PF2OF offset. Playfield 1 uses odd bitplanes (1,3,5,7). On AGA, PF2's top bit (`bpldata[8]`) participates in the colour index, giving 16 colours per playfield (4 bits each, from 8 bitplanes split 4+4) with the offset separating them in the palette.

**OCS undocumented feature preserved**: The playfield module retains an OCS quirk for single-playfield mode. When `bpu=5` and `pf2p>5` (playfield 2 priority > 5), bitplane 5 forces colour 16 `(denise_playfields.v:97-98)`:

```verilog
if ((pf2p>5) && bpldata[5] && !aga)
    plfdata[7:0] = {8'b00010000};
```

This is the undocumented behaviour used by games like "Swiv" for their score display. The `!aga` guard means AGA does not exhibit this quirk. An emulator targeting OCS must implement it; one targeting AGA should not.

### 3.9 Collision detection on Lisa (CLXCON2)

AGA extends the OCS collision detection with CLXCON2 ($DFF10E) to include bitplanes 7 and 8. Minimig `(denise_collision.v:42-49)`:

```verilog
always @ (posedge clk) begin
  if (clk7_en) begin
    if (reset || (reg_address == CLXCON))
      clxcon2 <= 16'h0000;
    else if (aga && (reg_address == CLXCON2))
      clxcon2 <= data_in;
  end
end
```

CLXCON2 is reset to zero both at power-on and whenever the main CLXCON register is written. This means OCS software that writes CLXCON will automatically zero out the AGA collision extension. An emulator that treats CLXCON and CLXCON2 as independent will break collision detection in mixed OCS/AGA software.

The collision match formula extends the OCS version `(denise_collision.v:54-55)`:

```verilog
wire [7:0] bm;
assign bm = (bpldata[7:0] ^ ~{clxcon2[1:0], clxcon[5:0]})
          | (~{clxcon2[7:6], clxcon[11:6]});
```

This concatenates CLXCON2's match-value and enable bits with the OCS CLXCON fields, extending the match to all 8 bitplanes seamlessly. The collision output register CLXDAT ($DFF00E) is unchanged — 15 bits, read-and-clear.

### 3.10 Border handling on Lisa

Lisa's video output path handles the border (area outside the display window) differently depending on several BPLCON3 bits. The output multiplexer `(denise.v:448-477)`:

```verilog
always @(*) begin
  if (brdsprt && sprsel)     // border sprites enabled and sprite active
    clut_data = sprdata;
  else if (!window_ena)      // outside display window
    clut_data = 8'b000000;   // force colour 0
  else if (sprsel)           // inside window, sprite active
    clut_data = sprdata;
  else                       // inside window, playfield
    clut_data = plfdata ^ bplxor;
end
```

And the blank logic:

```verilog
wire t_blank = (ecs & ecsena & brdrblnk & (~window_del | ~display_ena));
assign {red, green, blue} = t_blank ? 24'h000000 : out_rgb;
```

`(denise.v:472-477)`.

So the border rendering has four possible states:

1. **No BRDRBLNK, no BRDSPRT**: border shows colour 0 (whatever is in palette entry 0).
2. **BRDRBLNK set, BRDSPRT clear**: border is forced black (RGB = 0).
3. **BRDSPRT set, no sprite active**: border shows colour 0.
4. **BRDSPRT set, sprite active**: sprite data appears in the border.

BRDRBLNK + BRDSPRT together: border is black except where sprites appear (sprites override the black). This combination is used by AGA system screens for clean borders with a mouse pointer visible in the overscan area.

The `display_ena` signal deserves mention: it tracks whether any BPL1DAT write has occurred on this scanline `(denise.v:112-118)`. Before the first BPL1DAT write, sprites are invisible (neither border sprites nor display sprites appear). After BPL1DAT write, sprites are visible for the rest of the line. This is OCS behaviour carried forward.

---

## 4. AGA sprites

AGA sprites are Denise sprites with three changes: wider shifter (32 or 64 pixels per sprite instead of 16), optional higher sprite resolution, and no fixed colour range.

### 4.1 Sprite DMA width

Controlled by FMODE bits 3:2. Minimig `(agnus_spritedma.v:159–165)`:

```verilog
case(fmode[3:2])
  2'b00   : spr_fmode_ptradd = 3'd1;  // 16-bit sprite
  2'b11   : spr_fmode_ptradd = 3'd4;  // 64-bit sprite
  default : spr_fmode_ptradd = 3'd2;  // 32-bit sprite
endcase
```

So a 64-pixel-wide sprite (FMODE bits 3:2 = 11) fetches 4 words per SPRxDATA/SPRxDATB slot. The sprite DMA slot allocation is unchanged (same cycles 18..38 as OCS) — sprites just fetch wider per slot.

This has an implication: **you cannot do 8 × 64-pixel sprites**. Sprite DMA bandwidth is fixed. Minimig shows the slot table unchanged. What AGA gives you is wider-per-sprite at the cost of the blitter and CPU DMA budget. Typical use is 4 × 64-pixel sprites (for a status bar) with the other four left as 16-pixel.

### 4.2 Sprite resolution — SPRES

BPLCON3 bits 7:6. LORES/HIRES/SHRES is independent of BPLCON0's resolution for bitplanes. The Minimig shift generator `(denise_sprites.v:108–115)`:

```verilog
always @ (*) begin
  case (spres)
    2'b11   : shift = 1'b1;              // SHRES: shift every clock
    2'b10   : shift = ~c1 ^ c3;          // HIRES: every other clock
    default : shift = ~c1 & ~c3;         // LORES: every fourth clock
  endcase
end
```

This is per-shifter, controlled globally by BPLCON3. `spres == 00` means "match playfield resolution": if BPLCON0 selects SHRES, sprites are SHRES; otherwise LORES (Minimig's `spres_d` fallback `(denise.v:361)`):

```verilog
wire [1:0] spres_d;
assign spres_d = (spres == 2'b00) ? shres ? 2'b10 : 2'b01 : spres;
```

This is subtly wrong vs the HRM: the HRM says `spres=00` means "follow BPLCON0 resolution", but Minimig's fallback treats `hires` display as `10` (SPRES hires) and a non-hires display as `01` (SPRES lores). SHRES is in `shres`, not `hires`. So a SHRES playfield with `spres=00` gives a SHRES sprite — which is what you want, even if the encoding is counter-intuitive.

A sprite in SHRES over a LORES playfield gives pixel-level precision for the mouse or a crosshair. AGA games use this.

### 4.3 Sprite shifter and fmode data path

`(denise_sprites_shifter.v:47-56)`:

```verilog
// switch data according to fmode
reg [63:0] spr_fmode_dat;
always @ (*) begin
  case(fmode[3:2])
    2'b00   : spr_fmode_dat = {data16, 48'h000000000000};                 // 16-bit
    2'b11   : spr_fmode_dat = {data16, chip48[47:0]};                     // 64-bit
    default : spr_fmode_dat = {data16, chip48[47:32], 32'h00000000};      // 32-bit
  endcase
end
```

`data16` is the most-recent 16-bit word on the bus (from the normal register bus), and `chip48` is the 48-bit tail from chip RAM (the "wide read" path into Denise/Lisa). When FMODE=00, only `data16` is used. When FMODE=01/10, `data16 || chip48[47:32]` (32 bits total). When FMODE=11, `data16 || chip48[47:0]` (64 bits total, 4 words).

The shift register is 64 bits wide for all sprites (`reg [63:0] shifta, shiftb;` `(denise_sprites_shifter.v:35-36)`), with the shift rate determined by `spres`. When loading, the full 64 bits are replaced; when shifting, bits shift left one position per clock.

Sprite load is triggered by a horizontal position match, with SH10 (`hpos[8]`) optionally gated by FMODE[15]: `load <= armed && (hpos[7:0] == hstart[7:0]) && (fmode[15] || (hpos[8] == hstart[8]))` `(denise_sprites_shifter.v:74)`. The `fmode[15] || ...` means "in scandouble mode, ignore the top bit of sprite position". This is part of the scan-doubled sprite feature from section 4.4.

### 4.4 Sprite attachment

Attachment (pair → 15-colour sprite) is the same as OCS: bit 7 of SPRxCTL is the attach bit. AGA stores it the same way `(denise.v:287, 296, 305, 315)`. What changes is the non-attached colour base via OSPRM/ESPRM — see section 3.5.

Minimig keeps OCS-compatible attachment via this guard `(denise_sprites.v:283-284, 292-293, etc.)`:

```verilog
if (attach1 || (!aga && attach0))     // sprites 0,1 attached
```

On AGA, only the *odd* sprite's attach bit matters (attach1 for pair 0,1). On OCS/ECS, either sprite's attach bit triggered attachment. This is subtle and a source of emulator bugs: a program that sets attach on sprite 0 (not sprite 1) works on OCS but shows two separate sprites on AGA.

The corresponding code in `denise_sprites.v`:

```verilog
if (nsprite[1:0] != 2'b00) begin
  if (attach1 || (!aga && attach0))
    sprdata[7:0] = {osprm, sprdat1[1:0], sprdat0[1:0]};   // 4-bit attached colour
  else if (nsprite[0])
    sprdata[7:0] = {esprm, 2'b00, sprdat0[1:0]};
  else
    sprdata[7:0] = {osprm, 2'b00, sprdat1[1:0]};
end
```

This is applied symmetrically to sprite pairs (2,3), (4,5), (6,7). On AGA, the middle 2 bits are always `00` for non-attached pairs 0/1, `01` for 2/3, `10` for 4/5, `11` for 6/7 — preserving OCS-compatible sprite colouring at the low end while allowing ESPRM/OSPRM to redirect the high nibble anywhere in the 256-entry palette.

### 4.5 Sprite arming via SPRxDATA

A sprite is "armed" when its DATA register is written, and "disarmed" when CTL is written. Minimig `(denise_sprites_shifter.v:59-67)`:

```verilog
always @(posedge clk)
  if (clk7_en) begin
    if (reset)
      armed <= 0;
    else if (aen && address == CTL)   // writing CTL disarms
      armed <= 0;
    else if (aen && address == DATA)  // writing DATA arms
      armed <= 1;
  end
```

This is OCS-compatible behaviour, carried forward. A demo that tries to display a "static" sprite by writing only DATA (without DMA) needs to write DATB first (which doesn't affect armed), then DATA (which arms), and the next pixel-position match will load the shifter.

### 4.6 Sprite scandouble (FMODE bit 15 + SPRxPOS bit 7)

FMODE bit 15 enables sprite scandoubling. When set, and when the per-sprite SH10 bit (`sprposh` — stored when SPRxPOS is written `(agnus_spritedma.v:208)`) is set, the sprite skips alternate lines during fetch:

```verilog
sprdmastate <= dmastate & ~(fmode[15] && spr_sscan2 && (vpos[0] != vstart[0]));
```

`(agnus_spritedma.v:252)`. That is: on scandouble-odd-lines, fetch only when the line parity matches. Used for half-height sprite doubling, mostly by emulators running in non-interlaced modes.

Combined with FMODE bit 14 (bitplane scandouble), a scandoubled display can have both bitplanes and sprites appearing as doubled-height, giving a genuine progressive-from-interlace trick on real hardware.

The sprite load signal also interacts with FMODE[15] in the horizontal position match `(denise_sprites_shifter.v:74)`:

```verilog
load <= armed && (hpos[7:0] == hstart[7:0]) && (fmode[15] || (hpos[8] == hstart[8]));
```

When FMODE[15] (scandouble) is set, the H8 bit comparison is bypassed — `fmode[15] || ...` means "ignore the top bit of sprite horizontal position". This allows scandoubled sprites to work regardless of which half of the display line they appear on. Without this bypass, a sprite at position $100+ would not load during the "second half" of a scandoubled display.

### 4.7 Sprite DMA state machine

The sprite DMA engine uses a per-sprite state machine `(agnus_spritedma.v:240-248)`:

```verilog
always @ (*) begin
  if (vbl || ({ecs&vstop[9], vstop[8:0]} == vpos[9:0]))
    dmastate_in = 0;            // VBL or VSTOP → stop data DMA
  else if ({ecs&vstart[9], vstart[8:0]} == vpos[9:0])
    dmastate_in = 1;            // VSTART → start data DMA
  else
    dmastate_in = dmastate;     // hold current state
end
```

This state machine runs continuously: VSTOP takes priority over VSTART (checked first in the if chain). During vertical blank, data DMA is suppressed. During the last line of VBL (`vblend`), position/control words are fetched — this is the only time DMA automatically loads SPRxPOS/SPRxCTL.

The state is stored per-sprite in `dmastate_mem[7:0]` `(agnus_spritedma.v:234-238)` and evaluated sequentially (one sprite per clock cycle at 28 MHz, 8 sprites evaluated during the time it takes for one 7 MHz CCK).

An undocumented AGA feature noted in WinUAE `(custom.cpp:4181-4191)`: if a sprite is 64 pixels wide and SPRxDATx is written via DMA at the same cycle as a bitplane DMA fetch, the sprite's first 32 pixels get replaced with bitplane data. This is because the 48-bit chip48 bus carries both bitplane and sprite data, and the sprite latch can capture "stale" data from the previous bitplane fetch. This is a known compatibility issue on AGA that WinUAE tracks but does not fully emulate (the code is `#if 0`'d out).

### 4.8 Sprite resolution table (full cross-reference)

Combining BPLCON3 SPRES, BPLCON0 resolution, and FMODE sprite width:

| SPRES | Playfield | Sprite pixel rate | Shift rate | Max sprite pixels |
|-------|-----------|-------------------|-----------|-------------------|
| 00 | LORES | LORES (4 clk/pixel) | ~c1 & ~c3 | FMODE-dependent (16/32/64) |
| 00 | HIRES | HIRES (2 clk/pixel) | ~c1 ^ c3 | FMODE-dependent |
| 00 | SHRES | SHRES (1 clk/pixel) | 1'b1 | FMODE-dependent |
| 01 | any | LORES | ~c1 & ~c3 | FMODE-dependent |
| 10 | any | HIRES | ~c1 ^ c3 | FMODE-dependent |
| 11 | any | SHRES | 1'b1 | FMODE-dependent |

Source: `(denise_sprites.v:108-115, denise.v:361)`.

The sprite pixel width (in screen pixels) is:
- LORES sprite at HIRES display: each sprite pixel is 2 screen pixels wide
- HIRES sprite at LORES display: each sprite pixel is 0.5 screen pixels (sub-pixel)
- SHRES sprite at LORES display: each sprite pixel is 0.25 screen pixels

The last two cases are unusual but valid on real hardware. A SHRES mouse pointer on a LORES game gives sub-pixel pointer positioning.

---

## 5. AGA register bit tables

See [Appendix A](#appendix-a--complete-aga-register-bit-table) for the full register-by-register listing. This section gives the AGA-specific regs with their reset values and decode references.

Register summary (AGA-relevant, $DFFnnn):

| Address | Name | R/W | Reset | Notes |
|---------|------|-----|-------|-------|
| $100 | BPLCON0 | W | $0000 | AGA adds BPU3 at bit 4 `(denise.v:146)` |
| $102 | BPLCON1 | W | $3300 | AGA widens to 16 bits for PF1H/PF2H scroll `(denise_bitplanes.v:142)` |
| $104 | BPLCON2 | W | $0000 | AGA adds PF2OF in BPLCON3 (*not* BPLCON2) — BPLCON2 bit 8 = RDRAM; bit 9 = KILLEHB (ECS); bits 6:0 = priority (OCS) |
| $106 | BPLCON3 | W | $0C00 | New in ECS, bit layout changed in AGA — see section 3.4 |
| $108 | BPL1MOD | W | — | Unchanged |
| $10A | BPL2MOD | W | — | Unchanged |
| $10C | BPLCON4 | W | $0011 | AGA only — see section 3.5 |
| $10E | CLXCON2 | W | $0000 | AGA extended collision control: bits 0,1 = BP7/BP8 match, bits 6,7 = BP7/BP8 enable `(drawing.cpp:3412-3413)` |
| $1C0 | HTOTAL | W | $FFFF (HW) | ECS/AGA programmable mode — see section 12 |
| $1C2 | HSSTOP | W | — | ECS/AGA |
| $1C4 | HBSTRT | W | — | ECS/AGA |
| $1C6 | HBSTOP | W | — | ECS/AGA |
| $1C8 | VTOTAL | W | — | ECS/AGA |
| $1CA | VSSTOP | W | — | ECS/AGA |
| $1CC | VBSTRT | W | — | ECS/AGA |
| $1CE | VBSTOP | W | — | ECS/AGA |
| $1DC | BEAMCON0 | W | NTSC=$0000, PAL=$0020 | ECS/AGA `(agnus_beamcounter.v:119)` |
| $1DE | HSSTRT | W | — | ECS/AGA |
| $1E0 | VSSTRT | W | — | ECS/AGA |
| $1E2 | HCENTER | W | — | ECS/AGA |
| $1E4 | DIWHIGH | W | — | ECS/AGA — see section 12 |
| $1FC | FMODE | W | $0000 | AGA only — see section 2.1 |
| $07C | DENISEID | R | — | Returns $00F8 on AGA, $FFFC on ECS, $FFFF on OCS `(denise.v:293)` |

The horizontal position of the short-frame vertical sync (HCENTER) is ECS, not AGA-new, but commonly overlooked because OCS has a hardwired equivalent.

---

## 6. Akiko (CD32)

Akiko is the CD32's custom glue chip. It sits in chip memory at $00B80000–$00B8FFFF and performs four functions:

1. CD interface (CD-ROM drive command/data path)
2. Chunky-to-planar (C2P) hardware accelerator
3. 1 KiB NVRAM I/O (I²C to a 24C08 EEPROM)
4. CD32 gamepad routing (indirectly — via the CIA)

Akiko does *not* touch the graphics pipeline. It is entirely an I/O + data-transform chip.

### 6.1 Register map

From WinUAE `akiko.cpp:17–100`:

| Offset | Width | Name | Access | Meaning |
|--------|-------|------|--------|---------|
| $B80000 | L | ID | R | Reads as `$C0CACAFE` (read-only identifier) |
| $B80004 | L | INTREQ | R | CD interrupt request bits |
| $B80008 | L | INTENA | R/W | CD interrupt enable bits |
| $B8000C | L | (INTENA mirror write) | W | Second INTENA address `(akiko.cpp:1752)` |
| $B80010 | L | CDROM_ADDRESSDATA | R/W | DMA data base address (64-KiB aligned) |
| $B80014 | L | CDROM_ADDRESSMISC | R/W | Command/status/subcode DMA base (1-KiB aligned) |
| $B80018 | B | SUBCODE DMA offset / clear subcode int | R/W | Read = subcode DMA offset (non-zero = second buffer), write = clear bit 31 of INTREQ |
| $B8001D | B | TX position current/end | R/W | Read = current; write = end. Writing different value starts transmit DMA and clears bit 28 |
| $B8001E | B | RX position current | R | Current receive DMA circular buffer position |
| $B8001F | B | RX position end | W | Write = end position. Starts receive DMA and clears bit 27 |
| $B80020 | W | DMA transfer block enable | R/W | Each bit = one 4 KiB DMA block; write sets, zero ignored |
| $B80024 | L | CDROM_FLAGS (CONFIG) | R/W | CD subsystem config — see below |
| $B80028 | B | PIO TX/RX | R/W | PIO write (tx, CONFIG bit 30 off) / PIO read (rx, CONFIG bit 29 off) |
| $B80030 | B | NVRAM I²C data | R/W | Bit 7 = SCL, bit 6 = SDA |
| $B80032 | B | NVRAM I²C direction | R/W | Bit 7 = SCL direction, bit 6 = SDA direction |
| $B80038 | L | C2P | R/W | Chunky-to-planar 8-word buffer (write 8 longs, read 8 longs) |

### 6.2 INTREQ / INTENA bits

`(akiko.cpp:19–30)`:

| Bit | Meaning |
|-----|---------|
| 31 | Subcode interrupt (subcode buffer full and $B00018 changed) |
| 30 | Drive received all command bytes and executed command (PIO only) |
| 29 | Drive has status data pending (PIO only) |
| 28 | Drive command DMA transmit complete (DMA only) |
| 27 | Drive status DMA receive complete (DMA only) |
| 26 | Drive data DMA complete |
| 25 | DMA overflow (lost data) |
| 24:0 | reserved / unused |

INTREQ is read-only; each bit is cleared by a different write (e.g. bit 31 by writing $B80018, bit 28 by writing $B8001D, bit 27 by writing $B8001F).

### 6.3 CDROM_FLAGS / CONFIG ($B80024)

`(akiko.cpp:81–92)`:

| Bit | Meaning |
|-----|---------|
| 31 | Subcode DMA enable |
| 30 | Command (TX) DMA enable |
| 29 | Status (RX) DMA enable |
| 28 | Memory access mode |
| 27 | Data transfer DMA enable |
| 26 | CD interface enable |
| 25 | CD data mode (?) |
| 24 | CD data mode (?) |
| 23 | Akiko internal CIA faked vsync rate: 0 = 50 Hz, 1 = 60 Hz |
| 22:0 | unused |

Bit 23 is Akiko's "I will lie to Paula about the frame rate for region-switch purposes" bit. Games that need 50 Hz on a 60 Hz display can set it to get vsync at the requested rate regardless of the actual display.

### 6.4 DMA blocks and buffer layout

DMA data base at $B80010 must be 64 KiB aligned. The $B80020 register is a 16-bit mask — each of the 16 bits corresponds to one 4 KiB block within the 64 KiB region. When a bit is set, Akiko will DMA a CD sector into that block and clear the bit. Only set bits matter; writing a 0 bit has no effect (you cannot clear a pending DMA).

Each DMA block layout `(akiko.cpp:71-77)`:

```
+0x000:    0..2   zeroed
           3      low 5 bits of sector number
           4..2351  2348 bytes raw sector data (first 4 bytes skipped)
+0xC00:    146 bytes of CD error correction data
```

Processing order: bit 15 first, then 14, 13... down to 0. Interrupt (bit 26) fires after each block.

### 6.5 CD32 commands

`(akiko.cpp:102-140)`:

| Cmd | Name | Size in | Size out | Notes |
|-----|------|---------|----------|-------|
| 1 | STOP | 1 | 2 (status) | — |
| 2 | PAUSE | 1 | 2 (status) | — |
| 3 | UNPAUSE | 1 | 2 (status) | — |
| 4 | PLAY/READ | 12 | 2 | MSF start/end + bits (mute/read-mode etc.) |
| 5 | LED | 2 | 0 or 2 | Bit 7 of second byte = "response requested"; second byte non-zero = LED is lit |
| 6 | SUBCODE | 1 | 15 | — |
| 7 | INFO | 1 | 20 | Status + firmware version |

Status second byte: bit 7 = error, bit 3 = playing, bit 0 = door closed.

First byte of command = `(counter << 4) | cmd`, where the counter increments with each command to match up responses. Last byte = checksum. The firmware signature (WinUAE `FIRMWAREVERSION "CHINON  O-658-2 24"`) `(akiko.cpp:176)` is the string returned by command 7 on real CD32 hardware with the Chinon O-658 drive.

### 6.6 C2P (Chunky-to-Planar)

This is the headline feature. Akiko takes 8 × 32-bit chunky pixels (one "byte per pixel, 4 bytes per word, 8 words = 32 pixels wide × 1 row" chunky image) and outputs 8 × 32-bit planar values — one plane per word. The CPU writes 8 longs to $B80038+0..7, reads 8 longs back.

The algorithm `(akiko.cpp:317–381)`: for each output bit-position (0..31), OR together the 8 input words' bits at that bit-position, shifting them into the right byte-number for the output word. Effectively an 8×32 bit-matrix transpose.

WinUAE has two paths:

- **Reference path** `(akiko.cpp:317–329)`: nested loops, 256 iterations.
- **Fast path with precalc table** `(akiko.cpp:344–381)`: fully unrolled 32-way OR using `akiko_precalc_shift[i]` and `akiko_precalc_bytenum[i][j]`.

**Emulator consequence**: the C2P accelerator is how CD32 and A1200+FastRAM can do 256-colour fullscreen games at all. Without Akiko, converting a chunky framebuffer to 8 bitplanes takes ~30 CPU cycles per pixel; with Akiko, it's one word-write and one word-read per 32-pixel column. A CD32 game that uses Doom-style rendering *requires* Akiko C2P.

Also note the "Kickstart Akiko C2P support requires $CAFE at $B80002.W" comment `(akiko.cpp:1692)` — Kickstart probes for the Akiko magic via the ID half-word before using the accelerator.

### 6.7 Minimig vs WinUAE Akiko implementation

Minimig's Akiko is a minimal stub `(akiko.v, 61 lines)`. It implements only:

1. **ID register**: $B80000 returns `$C0CA`, $B80002 returns `$CAFE` `(akiko.v:55-56)`.
2. **C2P converter**: address `{addr[5:2] == 4'b1110}` (i.e. $B80038) handles the C2P `(akiko.v:31)`.

The Minimig C2P implementation is elegant `(akiko.v:36-58)`:

```verilog
wire c2p_sel = (addr[5:2] == 'b1110);

reg [7:0] buff[32];
reg [3:0] rptr = 0, wptr = 0;

always @(posedge clk) begin
    if((wr|rd) & cs & c2p_sel) begin
        if (wr) begin
            rptr <= 0;
            wptr <= wptr + 1'd1;
            {buff[{wptr,1'b0}], buff[{wptr,1'b1}]} <= din;
        end else begin
            wptr <= 0;
            rptr <= rptr + 1'd1;
        end
    end
end

always begin
    dout = 0;
    if(cs) begin
        if (addr == 0) dout = 16'hC0CA;
        if (addr == 1) dout = 16'hCAFE;
        if (c2p_sel) for(i=0; i<16; i=i+1'd1) dout[i] = buff[{rptr[0],~i[3:0]}][rptr[3:1]];
    end
end
```

The bit-transpose happens in the `for` loop on read: for each of the 16 output bits, it selects a specific byte from the 32-byte buffer and extracts one bit based on the read pointer. The write pointer advances per 16-bit write (16 writes fill the buffer); the read pointer advances per read (16 reads drain it).

Missing from Minimig's Akiko:

- **CD-ROM interface** ($B80004-$B80028) — not implemented. The MiSTer CD32 core handles CD through a different path.
- **NVRAM I2C** ($B80030-$B80032) — not implemented in this file (handled elsewhere in the MiSTer framework).
- **INTREQ/INTENA** ($B80004-$B80008) — not implemented.
- **CONFIG** ($B80024) — not implemented.

WinUAE's Akiko, by contrast, is a complete 2300+ line implementation covering all CD-ROM commands, DMA block transfer logic, I2C EEPROM protocol, PIO mode, and the full interrupt system. For an emulator targeting CD32 CD game compatibility, WinUAE is the authoritative source; for C2P-only support (e.g. an A1200 with the Akiko board addon), Minimig's stub suffices.

### 6.8 CD32 gamepad protocol (via CIA, not Akiko)

The CD32 gamepad is *not* handled by Akiko. It is handled by the CIA (CIAA PRA/DDRA) and the POTGOR register — the same path as a normal 9-pin joystick, but with a shift register protocol.

**Protocol** `(inputdevice.cpp:3820, 4012–4103)`:

1. The gamepad sits on the joystick port. In "two-button mode" (P5 = 1 or floating, controlled by programming POTGO), the pad behaves as a 2-button joystick.
2. To enter CD32 mode, the host drives P5 low (via POTGO bit 9/13). This latches the current 7 button states into the pad's internal shift register and signals "shift mode active" via CIAA PRA.
3. The host then toggles CIAA PRA joystick-fire pin 2 (bit 6 for port 0, bit 7 for port 1). Each high-to-low transition shifts the pad's register one bit. The current top-bit of the register appears on POTGOR pin P9 (bit 10 for port 0, bit 14 for port 1).
4. Reading POTGOR after each toggle gives the next button. 7 buttons in order: PLAY, RWD, FFW, GREEN, YELLOW, RED, BLUE.
5. After 7 shifts, the register reads 0 (pad signals "end of data").

Minimig-relevant bit layout `(inputdevice.cpp:4047–4050)`:

```c
uae_u16 p9dir = 0x0800 << (i * 4);  // POTGO output enable P9
uae_u16 p9dat = 0x0400 << (i * 4);  // POTGO data        P9
uae_u16 p5dir = 0x0200 << (i * 4);  // POTGO output enable P5
uae_u16 p5dat = 0x0100 << (i * 4);  // POTGO data        P5
```

`i=0` is port 0 (mouse/joystick 1, bits 8..11), `i=1` is port 1 (bits 12..15). WinUAE's `cd32_shifter[i]` counts the remaining shifts and wraps after 8; `cd32padmode(i)` is true when P5 is high (input mode).

**Emulator consequence**: a CD32 game that uses the CD32 pad is not just reading a joystick. It needs the CIAA handler (`handle_cd32_joystick_cia` at `inputdevice.cpp:4013`) which advances the shift register on CIA write, and the POTGOR handler (`handle_joystick_potgor` at `inputdevice.cpp:4041`) which returns shifted bits. An emulator that implements CIAA fire-button writes without the shift-counter side effect will drop CD32 pad support silently.

The pad button IDs in WinUAE `(include/inputdevice.h:27–33)`:

```c
#define JOYBUTTON_CD32_PLAY   3
#define JOYBUTTON_CD32_RWD    4
#define JOYBUTTON_CD32_FFW    5
#define JOYBUTTON_CD32_GREEN  6
#define JOYBUTTON_CD32_YELLOW 7
#define JOYBUTTON_CD32_RED    8
#define JOYBUTTON_CD32_BLUE   9
```

Shift order in the read path (`cd32_shifter >= 2` and `joybutton & ((1 << JOYBUTTON_CD32_PLAY) << (shifter - 2))`) `(inputdevice.cpp:4065)` means shifter value 8 → BLUE first, then RED, YELLOW, GREEN, FFW, RWD, PLAY, then a terminator zero. Shifter starts at 8 when P5 goes low (reload) `(inputdevice.cpp:3869)`.

---

## 7. Gayle (A600/A1200)

Gayle is the A600's and A1200's IDE + PCMCIA bridge. It lives at $00DA0000–$00DAFFFF (IDE/PCMCIA) and $00DE0000–$00DEFFFF (Gayle configuration).

### 7.1 Address map

`(WinUAE gayle.cpp:72–143, Minimig rtl/gayle.v:47–66)`:

| Range | Function |
|-------|----------|
| $DA0000 – $DA0FFF | IDE CS1, 16-bit speed — task file (data / error / sector count / sector / cyl lo / cyl hi / dev / status) |
| $DA1000 – $DA1FFF | IDE CS2, 16-bit speed — control port (alternate status, device control) |
| $DA2000 – $DA2FFF | IDE CS1, 8-bit speed |
| $DA3000 – $DA3FFF | IDE CS2, 8-bit speed |
| $DA4000 – $DA7FFF | reserved |
| $DA8000 – $DA8FFF | GAYLE_CS_1200 — IDE status + PCMCIA flags |
| $DA9000 – $DA9FFF | GAYLE_IRQ_1200 — IDE interrupt change status |
| $DAA000 – $DAAFFF | GAYLE_INT_1200 — IDE interrupt enable |
| $DAB000 – $DABFFF | GAYLE_CFG_1200 — PCMCIA voltage and speed config |
| $DE1000 – $DE1FFF | GAYLEID — ID register: reads high bit of a 4-state sequence |
| $DE0000 – $DE00FF | Motherboard Resources (Fat Gary / Ramsey overlap on A3000/A4000, not used on A1200/A600) |

A4000 uses $DD2020–$DD2FFF for its Gayle base `(gayle.cpp:87)`. A1200/A600 use $DA0000. Emulator configuration must distinguish the two.

### 7.2 GAYLE_CS_1200 ($DA8000) — IDE/PCMCIA combined status

`(gayle.cpp:100–110)`:

| Bit | Name | Meaning |
|-----|------|---------|
| 7 | GAYLE_CS_IDE | IDE interrupt status |
| 6 | GAYLE_CS_CCDET | Credit-card (PCMCIA) detect |
| 5 | GAYLE_CS_BVD1 / GAYLE_CS_SC | Battery voltage detect 1 / Card status change |
| 4 | GAYLE_CS_BVD2 / GAYLE_CS_DA | Battery voltage detect 2 / Digital audio |
| 3 | GAYLE_CS_WR | PCMCIA write enable (1 = enabled) |
| 2 | GAYLE_CS_BSY / GAYLE_CS_IRQ | Card busy / interrupt request |
| 1 | GAYLE_CS_DAEN | Enable digital audio |
| 0 | GAYLE_CS_DIS | Disable PCMCIA slot |

Bits 5 and 4 are dual-purpose because the hardware overloads two PCMCIA card types (memory vs I/O) onto the same register bits. An I/O card (Ethernet, modem) uses IRQ/SC semantics; a memory card (SRAM, Flash) uses BVD1/BVD2 (battery voltage detect).

### 7.3 GAYLE_IRQ_1200 ($DA9000) — interrupt change status

Identical bit layout to CS, plus `GAYLE_IRQ_RESET 0x02` and `GAYLE_IRQ_BERR 0x01`. The RESET bit causes a machine reset on credit-card detect change; BERR causes a bus error. Writing zeros resets selected bits (write-1-to-keep semantics); writing ones leaves bits unchanged. `(gayle.cpp:113–123)`.

### 7.4 GAYLE_INT_1200 ($DAA000) — interrupt enable

`(gayle.cpp:126–136)`. Same bit positions as IRQ. Two extra bits:

| Bit | Name | Meaning |
|-----|------|---------|
| 1 | GAYLE_INT_BVD_LEV | BVD interrupt level: 0 = INT2, 1 = INT6 |
| 0 | GAYLE_INT_BSY_LEV | BSY interrupt level: 0 = INT2, 1 = INT6 |

So BVD and BSY can route to either the normal ports int (level 2) or the high-priority int (level 6) depending on the card's requirements.

### 7.5 GAYLE_CFG_1200 ($DAB000) — PCMCIA config

`(gayle.cpp:138–144)`:

| Value | Meaning |
|-------|---------|
| $0 | 0 V (off) |
| $1 | 5 V |
| $2 | 12 V |
| $4 | 150 ns |
| $8 | 100 ns |
| $0 | 250 ns (default) |
| $C | 720 ns |

The CFG register is actually 2 fields: bits 1:0 are voltage select, bits 3:2 are speed select. The encoded values above show each field in isolation. `GAYLE_CFG_100NS 0x08` means "speed bits = 10", and so on.

### 7.6 GAYLEID ($DE1000) — the 4-state ID sequence

`(gayle.cpp, Minimig gayle.v:99–107)`. Reading GAYLEID returns an MSB that cycles through the sequence `1, 1, 0, 1` on successive reads. A write resets the counter to the start. This is how software probes "is this a Gayle-equipped machine". The sequence gives software a specific bit pattern to look for; a pattern mismatch means no Gayle (or different chip).

```verilog
assign gayleid = ~gayleid_cnt[1] | gayleid_cnt[0];
```

with `gayleid_cnt` incrementing on each read after a write-reset.

**Gayle vs AGA Gayle ID**: WinUAE distinguishes between "Gayle" ($D0 ID sequence) and "AA Gayle" ($D1 for AGA machines) `(gayle.cpp:837)`. The comment reads: "Gayle ID. Gayle = 0xd0. AA Gayle = 0xd1". The actual implementation `(gayle.cpp:838-843)`:

```c
if (gayle_id_cnt == 0 || gayle_id_cnt == 1 || gayle_id_cnt == 3
    || ((currprefs.chipset_mask & CSMASK_AGA) && gayle_id_cnt == 7)
    || (currprefs.cs_cd32cd && !currprefs.cs_ide && !currprefs.cs_pcmcia && gayle_id_cnt == 2))
    v = 0x80;
else
    v = 0x00;
gayle_id_cnt++;
```

The base sequence is: reads 0,1,3 return $80 (bit 7 set), all others return $00. This gives the pattern $D0 over the first 8 reads (bits: 1,1,0,1,0,0,0,0). For AGA machines, read 7 also returns $80, giving $D1. For CD32 (cs_cd32cd without IDE or PCMCIA), read 2 also returns $80, giving a different variant.

Minimig's Gayle ID implementation `(gayle.v:99-107)` uses a simpler 2-bit counter that produces the basic 4-state sequence `1,1,0,1`. This is the $D0 pattern and does not distinguish AGA. For an emulator targeting accurate A1200 behaviour, the WinUAE implementation is more complete; the Minimig one is sufficient for basic Gayle detection.

### 7.7 Gayle longword IDE support

Minimig's Gayle supports 32-bit IDE reads `(gayle.v:133-138)`:

```verilog
reg longword_r;
always @(posedge clk) longword_r <= rd && longword && !addr[4:1];

wire io_32 = (longword_r | longword) && rd;
```

This allows the 68020+ CPU on A1200 to read the IDE data port with a longword access, getting two 16-bit words in one bus cycle. The `longword` input is asserted when the CPU does a 32-bit read. The IDE controller then provides both the current and next 16-bit values: the first read returns `tfr[15:0]`, the second read (address+2, triggered by `longword_r`) returns `tfr[31:16]` `(gayle.v:206)`.

This is a meaningful optimisation: PIO IDE transfers on A1200 with 32-bit reads run at nearly 2x the speed of 16-bit reads. An emulator that models IDE as 16-bit-only will work but will miss this performance characteristic.

### 7.8 IDE task file layout

`(gayle.cpp:118–126)`:

| Offset | Name | Bits |
|--------|------|------|
| $DA2000 | Data | 16 |
| $DA2004 | Error / Feature | 8 |
| $DA2008 | SectorCount | 8 |
| $DA200C | SectorNumber | 8 |
| $DA2010 | CylinderLow | 8 |
| $DA2014 | CylinderHigh | 8 |
| $DA2018 | Device/Head | 8 |
| $DA201C | Status / Command | 8 |
| $DA3018 | Control (alternate) | 8 |

This is the standard ATA-1 task-file, memory-mapped. Data port is 16-bit; the others are 8-bit on a 4-byte stride.

### 7.9 Gayle interrupt routing

`(gayle.cpp:247–265)`:

- IDE IRQ (CS_IDE) → INT2
- PCMCIA write (CS_WR) → INT2
- Credit-card detect change (CCDET) → INT2 + optional reset/berr
- BVD1/BVD2 → INT2 or INT6 per GAYLE_INT_BVD_LEV
- BSY → INT2 or INT6 per GAYLE_INT_BSY_LEV

Gayle has no DMA. IDE is strictly PIO.

### 7.10 PCMCIA address space

Gayle controls the A600/A1200 PCMCIA slot. The PCMCIA address space `(gayle.cpp:75-81)`:

| Range | Size | Function |
|-------|------|----------|
| $600000 – $9FFFFF | 4 MiB | PCMCIA RAM (credit card memory) |
| $A00000 – $A1FFFF | 128 KiB | PCMCIA Attribute space |
| $A20000 – $A3FFFF | 128 KiB | PCMCIA I/O (16-bit and even 8-bit) |
| $A30000 – $A3FFFF | (overlap) | PCMCIA I/O (odd 8-bit registers) |
| $A40000 – $A5FFFF | 128 KiB | PCMCIA control bits / reset |
| $A60000 – $A7FFFF | 128 KiB | PC I/O space |

To initiate a PCMCIA reset, software writes $00 to $A40000 (GAYLE_RESET), then reads 1 byte from the same address to stop the reset `(gayle.cpp:84)`. PCMCIA cards supported include SRAM (type 1), IDE (type 2), and NE2000 Ethernet (type 3) in WinUAE's implementation `(gayle.cpp:40-44)`.

The PCMCIA configuration array `(gayle.cpp:158-159)` stores up to 20 configuration tuples for card-setup. When `pcmcia_configured >= 0`, the card is configured and I/O ports are active. When `pcmcia_configured < 0`, only attribute-space reads are valid (the card is in "tuple-reading" mode, where the host reads Card Information Structure data from attribute memory).

Minimig's Gayle does not model PCMCIA at all — there is no PCMCIA controller in the FPGA implementation. The MiSTer framework handles PCMCIA-emulated Ethernet and SRAM separately. For an emulator targeting A600/A1200 PCMCIA card support, WinUAE is the only reference.

### 7.11 Gary address decode (Minimig perspective)

While Gary is covered briefly in section 1.5, the Minimig implementation `(gary.v)` provides the actual address decode logic that any emulator needs. The key selectors `(gary.v:153-173)`:

```verilog
sel_chip[0] = cpu_address[23:19]==5'b0000_0 && (!ovl || cpu_hlt);    // $000000-$07FFFF
sel_chip[1] = cpu_address[23:19]==5'b0000_1;                         // $080000-$0FFFFF
sel_chip[2] = cpu_address[23:19]==5'b0001_0;                         // $100000-$17FFFF
sel_chip[3] = cpu_address[23:19]==5'b0001_1;                         // $180000-$1FFFFF
sel_slow[0] = cpu_address[23:19]==5'b1100_0 && |memory_config[3:2];  // $C00000-$C7FFFF
sel_slow[1] = cpu_address[23:19]==5'b1100_1 &&  memory_config[3];    // $C80000-$CFFFFF
sel_slow[2] = cpu_address[23:19]==5'b1101_0 && &memory_config[3:2];  // $D00000-$D7FFFF
sel_kick    = cpu_address[23:19]==5'b1111_1 && (cpu_rd || ...);      // $F80000-$FFFFFF
sel_reg     = cpu_address[23:21]==3'b110 ? ~(|t_sel_slow|...) : 0;   // $DF0000-$DFFFFF
sel_cia     = cpu_address[23:16]==8'hBF;                             // $BFxxxx
sel_ide     = hdc_ena && cpu_address[23:16]==8'b1101_1010;           // $DA0000-$DAFFFF
sel_gayle   = hdc_ena && cpu_address[23:12]==12'b1101_1110_0001;     // $DE1000-$DE1FFF
sel_rtc     = cpu_address[23:16]==8'b1101_1100;                      // $DC0000-$DCFFFF
```

The `ovl` (overlay) signal is critical: at boot, overlay is active and reads from $000000-$07FFFF return Kickstart ROM content instead of chip RAM. This is how the reset vector at address 0 points to Kickstart code. Writing to any address in the $F80000-$FFFFFF range (regardless of data) clears the overlay `(gary.v:131)`.

The `memory_config` 4-bit field controls which slow-RAM banks are visible. This allows emulating various A500/A2000 memory configurations (512K chip + 512K slow, 1M chip, etc.).

---

## 8. Budgie (A1200 glue)

Budgie (Commodore part 391425) is the A1200-specific bus glue between Alice and the 68EC020 CPU. Its job is:

- 68EC020 to 16-bit chip RAM bus width conversion
- Refresh cycle generation for chip RAM
- "Gary-compatible" address decoding for the A1200 bus structure

WinUAE does not model Budgie as a separate addressable chip. There is no `cs_budgierev` preference, no $DAxxxx-mapped Budgie register set, and no IDE/PCMCIA functionality in Budgie. All A1200 I/O routing that needs a distinguishable chip is handled by Gayle. Budgie sits between Alice and the CPU and is invisible to software — it's effectively "more of the same Gary".

**Emulator consequence**: You do not need a Budgie model. Handling the A1200 means:

1. Model Alice (or a compatible AGA chipset model).
2. Model Gayle for IDE and PCMCIA.
3. The CPU-to-chip-RAM bus behaves as a normal 16-bit memory; any timing extras Budgie provides are absorbed into your chip-RAM access latency model.

The one place Budgie does matter is for hardware-accurate timing: Alice cannot present 32-bit data to the CPU directly. Budgie multiplexes two consecutive 16-bit fetches into one 32-bit CPU read, adding 1–2 CPU cycles of latency for longword accesses to chip RAM. This is mostly relevant if you are modelling 68EC020 timing at the level of "which read is in the prefetch queue". Most emulators do not.

---

## 9. Super Buster / Zorro III

Super Buster (Commodore 390539) is the A3000/A4000 Zorro III bus controller. It is the chip that lets Zorro cards sit in the 32-bit address space above 16 MiB with the full 32-bit bus width and 2-MiB card granularity.

### 9.1 Zorro III address space

`(WinUAE expansion.cpp:245–263)`. The HRM documents Zorro III config space at $FF000000 `(include/memory.h:38: AUTOCONFIG_Z3 0xff000000)`. In practice, **Kickstart 3.1 does all autoconfig through the Zorro II config window at $00E80000** — even Zorro III cards get their initial AUTOCONFIG reads and writes through $00E8xxxx. This is the opposite of what the HRM says.

Once configured, a Zorro III card gets mapped into the 32-bit space (somewhere in $10000000–$7FFFFFFF, chosen by Kickstart's autoconfig logic). The card's BAR register values determine the final address.

### 9.2 Zorro III memory size codes

`(expansion.cpp:99–153)`:

```c
#define Z3_MEM_16MB     0x00
#define Z3_MEM_32MB     0x01
#define Z3_MEM_64MB     0x02
#define Z3_MEM_128MB    0x03
#define Z3_MEM_256MB    0x04
#define Z3_MEM_512MB    0x05
#define Z3_MEM_1GB      0x06

// Sub-size codes for cards that don't fit one of the above:
#define Z3_SS_MEM_SAME      0x00
#define Z3_SS_MEM_AUTO      0x01
#define Z3_SS_MEM_64KB      0x02
#define Z3_SS_MEM_128KB     0x03
#define Z3_SS_MEM_256KB     0x04
#define Z3_SS_MEM_512KB     0x05
#define Z3_SS_MEM_1MB       0x06
#define Z3_SS_MEM_2MB       0x07
#define Z3_SS_MEM_4MB       0x08
...
#define force_z3            0x10  // *MUST* be set if card is Z3
#define care_addr           0x80  // Z3: 1->mem, 0->io
```

`force_z3` goes in er_Flags and is how a card declares "I am a Zorro III card" during autoconfig. `care_addr` (bit 7 of the same byte) declares "this card is memory not I/O" on Zorro III — note this is *different* semantics from Zorro II where the bit means "needs specific address".

### 9.3 Zorro III vs Zorro II autoconfig

Differences from Zorro II autoconfig `(expansion.cpp:245–302)`:

| Aspect | Zorro II | Zorro III |
|--------|----------|-----------|
| Config window | $00E80000 | $FF000000 (HRM) / $00E80000 (real) |
| Address space | $00200000–$009FFFFF (8 MiB) | $10000000–$7FFFFFFF (1.75 GiB) |
| Max slots | 8 | 8 |
| Min card size | 64 KiB | 16 MiB (or 64 KiB with sub-size override) |
| Base alignment | Card-size-aligned | Card-size-aligned, min 16 MiB |
| DMA bus width | 16-bit | 16-bit or 32-bit (card chooses) |
| Locked transfer | No | Yes, for FastRAM cards |
| Burst mode | No | Yes, up to 4-word burst |

### 9.4 Locked-transfer protocol

A Zorro III card can assert LOCK during a bus cycle to tell Super Buster "I own the bus for the next N cycles, don't arbitrate". This is how FastRAM cards achieve burst-mode reads: a single address request followed by 4 consecutive data beats. WinUAE does not model locked transfer at the cycle level (it is irrelevant to software correctness) — the transfer just appears as a single large memory access. An emulator targeting hardware timing for Zorro III FastRAM (rare — most emulators don't care) would need to model 1+4 cycle timing.

### 9.5 Geographic address carve-up

Zorro III cards are assigned contiguous address ranges by autoconfig, starting at $10000000 and working upward. Cards must claim their size in their autoconfig header's `er_Type` nibble (see the `Z3_MEM_*` codes above). The 32-bit address space is carved into:

| Range | Contents |
|-------|----------|
| $00000000–$001FFFFF | 2 MiB chip RAM |
| $00200000–$009FFFFF | Zorro II slow RAM / cards (8 MiB) |
| $00A00000–$00AFFFFF | PCMCIA attribute space (A600/A1200) |
| $00B00000–$00BFFFFF | Akiko (CD32) |
| $00C00000–$00C7FFFF | "Slow RAM" trap (1 MiB) |
| $00D80000–$00D8FFFF | CDTV DMAC |
| $00DA0000–$00DBFFFF | Gayle (A600/A1200), CDTV |
| $00DC0000–$00DCFFFF | Real-time clock |
| $00DD0000–$00DDFFFF | IDE (A4000) |
| $00DE0000–$00DEFFFF | Motherboard Resources (Fat Gary / Ramsey) |
| $00DF0000–$00DFFFFF | Custom chipset registers |
| $00E00000–$00E7FFFF | ROM mirror |
| $00E80000–$00EFFFFF | Zorro II autoconfig window |
| $00F00000–$00F7FFFF | Extended ROM |
| $00F80000–$00FFFFFF | Kickstart ROM |
| $01000000–$0FFFFFFF | Zorro II expansion / bridge |
| $10000000–$7FFFFFFF | Zorro III space |
| $80000000–$FEFFFFFF | unassigned / CPU boards |
| $FF000000–$FFFFFFFF | Zorro III autoconfig (unused in Kickstart 3.1) |

This is the logical map; the physical decoder is Gary/Fat Gary/Budgie depending on machine. Super Buster's job is to present the upper region ($10000000+) to the Zorro III bus.

---

## 10. Ramsey (A3000/A4000 memory controller)

Ramsey is the DRAM controller for A3000 and A4000. It lives at motherboard-resource space $00DE0000 with the same bank structure as Gary (64-byte stride, 4 bytes per register). Revision is stored in `currprefs.cs_ramseyrev`; valid values are $0D (Ramsey rev D, 4 MiB max) and $0F (Ramsey rev F, 16 MiB max) `(cfgfile.cpp:9931, 9951, 9963)`.

### 10.1 Ramsey register map

`(WinUAE gayle.cpp:898–958)`. The Ramsey/Gary motherboard-resource region uses a bank selector (address bits 7:6) and an offset (address bits 1:0):

| (addr>>6)&3 | addr&3 | Register | Access |
|-------------|--------|----------|--------|
| 0 | 0 | Gary timeout flag | R/W bit 7 |
| 0 | 1 | Gary TOENB (timeout enable) | R/W bit 7 |
| 0 | 2 | Gary coldboot flag | R/W bit 7 |
| 0 | 3 | Ramsey CONFIG | R/W 8 bits |
| 1 | 3 | Ramsey VERSION | R 8 bits — returns `cs_ramseyrev` |

WinUAE source, `mbres_read` and `mbres_write` `(gayle.cpp:898–958)`:

```c
if (addr64 == 0 && addr2 == 0x03)
    ramsey_config = val;                     // $DE0003.B (?) — CONFIG
if (addr2 == 0x02)
    gary_coldboot = (val & 0x80) ? 1 : 0;    // $DE0002 — coldboot
if (addr2 == 0x01)
    gary_toenb = (val & 0x80) ? 1 : 0;       // $DE0001 — timeout enable
if (addr2 == 0x00)
    gary_timeout = (val & 0x80) ? 1 : 0;     // $DE0000 — timeout
```

And for read:

```c
if (addr64 == 1 && addr2 == 0x03)            // $DE0043.B — Ramsey VERSION
    v = currprefs.cs_ramseyrev;
if (addr64 == 0 && addr2 == 0x03)            // $DE0003.B — Ramsey CONFIG
    v = ramsey_config;
```

Real offsets are therefore $DE0003 for CONFIG, $DE0043 for VERSION, $DE0000/01/02 for Gary flags. Other motherboard-resource addresses return 0xFF.

### 10.2 CONFIG register

The CONFIG bits control:

- Static column mode (fast-page DRAM access)
- Burst mode (enables CPU longword bursts to DRAM)
- Refresh rate (154 or 238 CPU clocks between refreshes)
- Page size (256 B, 512 B, 1 KiB, 2 KiB)
- Skip (1 or 0 — error correction bypass?)

WinUAE stores but does not act on these bits `(gayle.cpp:893, 907)` — `ramsey_config` is a byte set and read back, with no side effects on the emulator's memory model. Real hardware uses these bits to configure the DRAM timing.

**Note**: WinUAE does not model the $5AC35AC3 DRAM probe pattern. The A3000/A4000 Kickstart ROM writes $5AC35AC3 to various memory addresses and reads back to determine what DRAM is fitted; this is memory-level behaviour and is handled by the emulator's RAM allocator, not by Ramsey. WinUAE just reports the configured memory size back to the ROM.

### 10.3 Version byte

Ramsey rev D = $0D, rev F = $0F. These are Kickstart-probeable via $DE0043.B. Kickstart uses the version to decide whether to enable static-column mode (rev F only).

### 10.4 Chip RAM vs fast RAM routing

Ramsey has two RAM banks:

- **Low bank** (`mbresmem_low` / `a3000lmem`) at $07000000 on A3000, sized 4 MiB on Ramsey D, up to 8 MiB on rev F `(memory.cpp:1500)`.
- **High bank** (`mbresmem_high` / `a3000hmem`) at $08000000, up to 8 MiB `(memory.cpp:1508)`.

A3000 default is 8 MiB in low, 0 in high. A4000 default is 8 MiB in low as well. The distinction low/high is historical: rev D could only address low bank, rev F added high. WinUAE's mapping respects this via `cs_ramseyrev`.

Chip RAM on A3000/A4000 is still at $00000000–$001FFFFF (up to 2 MiB, Alice on A4000, ECS Agnus on A3000). Ramsey handles only the 32-bit fast RAM banks.

---

## 11. CIA 8520 errata

The Amiga CIA is a 6526-compatible part. Most Amigas ship with 8520 (the Commodore CMOS version); some early A1000s had 6526. WinUAE has a preference `cs_cia6526` to select 6526 behaviour for compatibility testing `(cfgfile.cpp)`.

### 11.1 6526 vs 8520 behavioural differences

From `cia.cpp:1431–1458, 1544–1567`:

**TOD register — BCD (6526) vs binary (8520)**

The 6526 TOD counter counts in BCD (binary-coded decimal); the 8520 counts in 24-bit binary. This is the single biggest software-visible difference.

```c
static uae_u32 getciatod(uae_u32 tod)
{
    if (!currprefs.cs_cia6526)
        return tod;                         // 8520: return 24-bit binary
    uae_u32 bcdtod = 0;
    for (int i = 0; i < 4; i++) {
        int val = tod % 10;
        bcdtod *= 16;
        bcdtod += val;
        tod /= 10;
    }
    return bcdtod;                          // 6526: return BCD
}
```

And on write, the reverse conversion. An AmigaOS TOD counter ticks at vsync (50 or 60 Hz), giving a usable timebase with both CIAs: CIAA for mouse/keyboard and CIAB for floppy/serial.

**TOD read-latch clearing**

On the 8520, reading the high byte of TOD latches all three bytes until the low byte is read, so the caller sees a consistent 24-bit value. On the 6526, reading the high byte latches, but reading the low byte also unlatches immediately *only if the ALARM bit is clear* `(cia.cpp:1544-1551)`:

```c
case 10:
    if (!currprefs.cs_cia6526) {
        // 8520: latch only if not already latched
        if (!c->tlatch) {
            if (!(c->t[1].cr & CR_ALARM)) {
                c->tlatch = 1;
            }
            c->tol = c->tod;
        }
        return getciatod(c->tol) >> 16;
    } else {
        // 6526: different latching
        ...
    }
```

An emulator that uses the wrong variant will have software that reads TOD race against the running counter and get torn values once in a while — this is the classic "mouse cursor jumps on the hour" bug pattern.

**Bit 11 of TOD/alarm counter ("TODMED bug")** `(cia.cpp:831–843)`:

```c
static bool checkalarm(uae_u32 tod, uae_u32 alarm, bool inc)
{
    ...
    if (!currprefs.cs_ciatodbug)
        return ...;

    // emulate buggy TODMED counter.
    // it counts: .. 29 2A 2B 2C 2D 2E 2F 20 30 31 32 ..
    // (2F->20->30 only takes couple of cycles but it will trigger alarm..
    if (tod & 0x000fff)
        return false;
    if (((tod - 1) & 0xfff000) == alarm)
        return true;
    ...
}
```

This is the "CIA TOD bug" — the 6526 (but *not* the 8520) has a hardware bug where the middle digit of the BCD TOD counter briefly goes through an invalid state during the 2F→30 transition, which can cause a false alarm match. Some Amiga software relies on this; set `cs_ciatodbug` to emulate it.

### 11.2 ICR clearing race

Reading the ICR (interrupt control register at offset 13) is supposed to be atomic: read → returns current state → clears all bits. On real hardware, if a new interrupt arrives during the read cycle itself, the bit is cleared anyway and the interrupt is lost. WinUAE does not special-case this — the read clears `c->icr1` unconditionally `(cia.cpp:1583-1586)`:

```c
case 13:
    tmp = c->icr1 & ~(0x40 | 0x20);
    c->icr1 = 0;
    return tmp;
```

The bit mask `& ~(0x40 | 0x20)` excludes the "interrupt line" status bits from the visible read — those are computed elsewhere. Bit 7 (IR) is the summary bit. Bits 0..4 are the five interrupt sources (TA, TB, Alarm, SP, FLAG).

**Software workaround**: well-written code masks the CIA interrupt source via `imask` before reading ICR to avoid losing an interrupt. Poorly written code (or code that doesn't care about lost floppy IRQs) just reads ICR and accepts the race.

**Real hardware observation**: the TB cascade from TA underflow happens one cycle after TA crosses zero — there is a 1-cycle pipeline delay in the cascade path. Cycle-exact emulators need this delay or 32-bit cascade timers run 2x too fast (or half the expected period — depending on how you model the underflow event).

**WinUAE CR bit layout** (derived from references in `cia.cpp`):

```
CR bits:
  0 CR_START    - timer start/stop
  1 CR_PBON     - PB6/PB7 pulse output enable
  2 CR_OUTMODE  - pulse(0) or toggle(1) PB output
  3 CR_RUNMODE  - oneshot(1) or continuous(0)
  4 CR_LOAD     - force load from latch
  5 CR_INMODE   - input mode bit 0 (phi2 vs CNT / TA)
  6 CR_INMODE1  - input mode bit 1 (TB only)
  7 CR_ALARM    - (CRB only) set TOD alarm when writing TOD registers
  7 CR_SPMODE   - (CRA only) serial port in(0) or out(1) mode
  7 CR_TODIN    - (CRA only) TOD input freq 50Hz(0) or 60Hz(1)
```

Bits 7 of CRA have multiple meanings depending on register address — this is a 6526 quirk carried forward to 8520.

### 11.3 Timer cascade (TA underflow → TB tick)

CIA timer B can be clocked either from PHI2 (system clock) or from timer A underflow. This is the "cascade mode" used for 32-bit timers. The CR bit layout for timer B on both chips `(cia.cpp ICR/CR macros)`:

- CR bit 5 (INMODE) = 0 and CR bit 6 (INMODE1) = 0 → PHI2
- CR bit 5 (INMODE) = 1 and CR bit 6 (INMODE1) = 0 → CNT pin
- CR bit 5 (INMODE) = 0 and CR bit 6 (INMODE1) = 1 → TA underflow
- CR bit 5 (INMODE) = 1 and CR bit 6 (INMODE1) = 1 → TA underflow + CNT pin

WinUAE uses `CIA_timer_inmode` and `CIA_timer_02` `(cia.cpp:1461–1601)` to decode these. The "TA underflow + CNT" mode (both bits set) is used by some music software on both CIAs for precise 32-bit timing.

There is a subtle bug on cascade: if the TA underflow happens on exactly the same cycle as a TB count-tick, one or both events can be lost. Real hardware documents this; WinUAE models it partially by sequencing the two ticks in software order.

Emulator note: the Minimig CIA implementation (`ciaa.v`, `ciab.v`, `cia_timera.v`, `cia_timerb.v`, `cia_timerd.v`) is a faithful 8520 implementation but does not model the cascade race. For cycle-exact CIA modelling the WinUAE code is more authoritative because it contains the workarounds for actual software compatibility.

### 11.4 Timer write side effects and latch loading

From `cia.cpp:1603-1619`:

```c
static void CIA_thi_write(int num, int tnum, uae_u8 val)
{
    t->latch = (t->latch & 0xff) | (val << 8);

    // If ONESHOT: Load and start timer.
    // If CONTINUOUS: Load timer if not running.
    if (!(t->cr & CR_START) || (t->cr & CR_RUNMODE)) {
        t->timer = t->latch;
        t->timerval_prev = 0xffffffff;
    }
}
```

Writing the high byte of a timer latch has side effects:
- In one-shot mode (`CR_RUNMODE = 1`): the timer is loaded from the latch immediately.
- In continuous mode (`CR_RUNMODE = 0`): the timer is loaded only if it's not already running (`!(t->cr & CR_START)`).

This is a frequent source of bugs. A program that writes TxHI without intending to restart the timer in oneshot mode will accidentally restart it. The workaround is to write TALO *first*, then TAHI — but in continuous mode, you must stop the timer first. Different CIA references describe this differently; the WinUAE code is the source of truth.

### 11.5 E-clock relationship to CIA timer counting

Both CIAs count in E-clock units (not CPU cycles). The E-clock is derived from the system clock and ticks at 709 kHz (PAL) or 715 kHz (NTSC), which is 1/10th of the CPU clock. WinUAE's `CIA_timer_02` function `(cia.cpp:1461-1467)` determines if a timer is counting in E-clock mode:

```c
static bool CIA_timer_02(int num, uae_u8 cr)
{
    if (num) {
        return (cr & (CR_INMODE | CR_INMODE1)) == 0;
    }
    return (cr & CR_INMODE) == 0;
}
```

For CIAA timer A: if INMODE=0 (bit 5 clear), count PHI2 (E-clock). If INMODE=1, count CNT pin.
For CIAB timer B: if INMODE=0 *and* INMODE1=0 (bits 5,6 both clear), count E-clock. Other combinations count CNT or cascade from TA.

This means CIA timers at face value tick at E-clock rate, not CPU clock rate. A 68020 at 14 MHz runs at 14 × the CIA timer rate. An emulator that accidentally counts CIA timers in CPU cycles instead of E-clocks will have interrupts fire 10x too fast.

WinUAE models the E-clock as a 10-phase state machine: `DIV10 = E_CLOCK_LENGTH * E_CYCLE_UNIT` where `E_CLOCK_LENGTH = 10` and `E_CYCLE_UNIT = CYCLE_UNIT / 2` `(cia.cpp:126-127)`. The sync, start, and end phases are configurable constants `(cia.cpp:105-118)`:

```c
#define E_CLOCK_SYNC_N  2    // normal (8520) sync phase
#define E_CLOCK_START_N 4    // start phase
#define E_CLOCK_END_N   6    // end phase
#define E_CLOCK_TOD_N  -2    // TOD update offset

#define E_CLOCK_SYNC_N2 4    // alternate sync
#define E_CLOCK_START_N2 6
#define E_CLOCK_END_N2  6
#define E_CLOCK_TOD_N2  0
```

These constants determine *when* within the E-clock cycle a CIA register access is valid. The CIA bus is only accessible during certain E-clock phases; CPU accesses outside these windows are "wait-stated" until the next valid phase. This wait-stating is visible to cycle-exact emulators as a variable CIA access latency of 0-9 CPU cycles.

### 11.6 Akiko internal CIA differences

The CD32 has Akiko's internal CIA rather than a separate 8520. WinUAE documents one difference `(cia.cpp:99-103)`:

```c
/* Akiko internal CIA differences:
- BFE101 and BFD100: reads 3F if data direction is in.
 */
```

On standard 8520, reading a port register when the data-direction register is set to "input" returns the actual pin state (pulled up to $FF by bus capacitance on most Amigas). On Akiko's internal CIA, such reads return $3F instead. This affects software that probes the parallel port or CIA data registers to detect hardware — specifically, boot-time hardware detection code that reads CIAA PRA may get $3F instead of $FF on CD32 if the direction is set wrong.

### 11.7 TOD alarm when tod == alarm == 0

`(cia.cpp:851–862)` shows a commented-out hack:

```c
#if 0
// hack: do not trigger alarm interrupt if KS code and both
// tod and alarm == 0. This incorrectly triggers on non-cycle exact
// modes. Real hardware value written to ciabtod by KS is always
// at least 1 or larger due to bus cycle delays when reading
// old value.
...
#endif
```

The comment implies: Kickstart sets ciabtod to zero during boot, but never to exactly zero on real hardware because writing three bytes of TOD takes three bus cycles, during which the counter is still running — so by the time the low byte is written, TOD is at least 1. An emulator that writes all three bytes atomically gets TOD = 0 = alarm (which is also 0 at boot), and a spurious alarm fires. WinUAE leaves this corner case documented in a disabled hack rather than fixing it silently — the user-visible behaviour is "don't write TOD to exactly 0 during alarm match".

---

## 12. ECS corners (BEAMCON0, DIWHIGH and friends)

BEAMCON0 is the ECS programmable-display-mode register at $DFF1DC. The printed manuals mention it exists and list bit names but rarely go into semantics. The Minimig decode `(agnus_beamcounter.v:115–139)` plus WinUAE's `BEAMCON0_*` macros `(include/custom.h:15–29)` give the full map.

### 12.1 BEAMCON0 bit layout

Reset value from Minimig `(agnus_beamcounter.v:119)`: `{10'b0, ~ntsc, 5'b0}`. That is: PAL on PAL boards (bit 5 = 1), NTSC on NTSC boards (bit 5 = 0), everything else zero.

| Bit | Mask | Name | Meaning |
|-----|------|------|---------|
| 15 | $8000 | — | reserved (ignored) |
| 14 | $4000 | HARDDIS | Disable hardware DDF limits — DDF can run any time regardless of $18/$D8 gate |
| 13 | $2000 | LPENDIS | Light pen disable |
| 12 | $1000 | VARVBEN | Variable vertical blanking enable (use VBSTRT/VBSTOP) |
| 11 | $0800 | LOLDIS | Long-line disable (NTSC only). Disables the 227.5 CCK half-line alternation |
| 10 | $0400 | CSCBEN | Genlock composite-sync on CSYNC pin |
| 9 | $0200 | VARVSYEN | Variable vertical sync enable (use VSSTRT/VSSTOP) |
| 8 | $0100 | VARHSYEN | Variable horizontal sync enable (use HSSTRT/HSSTOP) |
| 7 | $0080 | VARBEAMEN | Variable-beam mode: HTOTAL, VTOTAL, HCENTER, HBSTRT, HBSTOP, VBSTOP used instead of hardwired |
| 6 | $0040 | DUAL (displaydual) | Dual-scan display (for genlock) |
| 5 | $0020 | PAL (displaypal) | 1 = PAL mode, 0 = NTSC mode (runtime-selectable on ECS/AGA) |
| 4 | $0010 | VARCSYEN | Variable composite-sync enable |
| 3 | $0008 | BLANKEN | Enable BLANK output (external blanking signal active) |
| 2 | $0004 | CSYTRUE | Composite sync polarity: 1 = active high |
| 1 | $0002 | VSYTRUE | Vertical sync polarity: 1 = active high |
| 0 | $0001 | HSYTRUE | Horizontal sync polarity: 1 = active high |

Minimig decodes (matching `include/custom.h`):

```verilog
wire harddis      = beamcon0_reg[14];
wire lpendis      = beamcon0_reg[13];
wire varvben      = beamcon0_reg[12];
wire loldis       = beamcon0_reg[11];
wire cscben       = beamcon0_reg[10];
wire varvsyen     = beamcon0_reg[9];
wire varhsyen     = beamcon0_reg[8];
wire varbeamen    = beamcon0_reg[7];
wire displaydual  = beamcon0_reg[6];
wire displaypal   = beamcon0_reg[5];
wire varcsyen     = beamcon0_reg[4];
wire blanken      = beamcon0_reg[3];
wire csynctrue    = beamcon0_reg[2];
wire vsynctrue    = beamcon0_reg[1];
wire hsynctrue    = beamcon0_reg[0];
```

`(agnus_beamcounter.v:125-139)`.

### 12.2 Variable-beam mode semantics

When `VARBEAMEN` is set, the beamcounter uses programmable HTOTAL/VTOTAL rather than hardwired values. Minimig:

```verilog
wire [8:0] htotal = varbeamen ? htotal_reg : HTOTAL_VAL << 1;  // 227 CCKs PAL
wire [10:0] vtotal = varbeamen ? vtotal_reg : pal ? VTOTAL_PAL_VAL : VTOTAL_NTSC_VAL;
```

`(agnus_beamcounter.v:220-230)`.

This is how ECS/AGA support non-broadcast modes. Setting BEAMCON0 = $00E8 (VARBEAMEN + VARHSYEN + VARVSYEN + BLANKEN) plus writing HTOTAL/VTOTAL/HSSTRT/HSSTOP/VSSTRT/VSSTOP gives a fully programmable refresh mode — this is how AmigaOS "Multiscan" screen modes work.

HARDDIS is related but different: it removes the hardwired DDF $18/$D8 limits. Typical use: a programmed mode wider than PAL needs DDF to start before $18, so HARDDIS is set to unlock the DMA engine. Minimig:

```verilog
assign harddis_out = harddis || varbeamen || varvben;
```

`(agnus_beamcounter.v:234)` — HARDDIS is forced on whenever VARBEAMEN or VARVBEN is set (you cannot run a programmed mode with hardware display-window limits).

### 12.3 DIWHIGH register layout

`(WinUAE custom.cpp:3836–3852)`:

```c
static void DIWHIGH(uae_u16 v)
{
    if (!ecs_agnus) return;
    if (!aga_mode) v &= ~(0x0010 | 0x1000);
    v &= ~(0x8000 | 0x4000 | 0x0080 | 0x0040);
    diwhigh = v;
    calcvdiw();
}
```

Layout:

| Bits | Meaning |
|------|---------|
| 15 | reserved (masked) |
| 14 | reserved (masked) |
| 13 | DIWSTOP H8 (horizontal display-window stop bit 8) |
| 12 | DIWSTOP H9 (AGA only) |
| 11:8 | DIWSTOP V10:V8 (extends DIWSTOP to 11 bits total) |
| 7 | reserved (masked) |
| 6 | reserved (masked) |
| 5 | DIWSTRT H8 (horizontal display-window start bit 8) |
| 4 | DIWSTRT H9 (AGA only) |
| 3:0 | DIWSTRT V10:V8 (extends DIWSTRT to 11 bits total) |

Minimig confirms `(agnus_bitplanedma.v:189-190)`:

```verilog
if (reg_address == DIWHIGH && ecs) begin
    vdiwstrt[10:8] <= data_in[2:0];
    hdiwstrt[8]    <= data_in[5];
end
```

and `(agnus_bitplanedma.v:200-202)`:

```verilog
if (reg_address == DIWHIGH && ecs) begin
    vdiwstop[10:8] <= data_in[10:8];
    hdiwstop[8]    <= data_in[13];
end
```

**Critical quirk**: writing DIWSTRT or DIWSTOP *clears* the high bits. So on ECS/AGA, you must write DIWHIGH **after** DIWSTRT/DIWSTOP, or the V10:V8 extension is lost. Minimig models this:

```verilog
if (reg_address == DIWSTRT) begin
    vdiwstrt <= {3'b000, data_in[15:8]};  // clears high bits
    hdiwstrt <= {1'b0, data_in[7:0]};
end else if (reg_address == DIWHIGH && ecs) begin
    vdiwstrt[10:8] <= data_in[2:0];  // high bits overwritten by DIWHIGH only
    hdiwstrt[8]    <= data_in[5];
end
```

`(agnus_bitplanedma.v:183-192)`.

The order matters: `DIWSTRT, DIWHIGH` is correct; `DIWHIGH, DIWSTRT` leaves DIWHIGH ineffective. WinUAE tracks `diwhigh_written` to avoid writing DIWHIGH before DIWSTRT `(custom.cpp:3816-3823, 3836-3852)`. Emulators must model this explicitly.

### 12.4 HTOTAL / VTOTAL / HSSTRT / HSSTOP / VSSTRT / VSSTOP / HCENTER

All at $DFF1Cx–$DFF1E2. All 9-to-11-bit counters loaded on CPU write.

From Minimig `(agnus_beamcounter.v:187–218)`:

```verilog
case (reg_address)
    HTOTAL  : htotal_reg  <= {data_in[7:0], 1'b0};
    HSSTRT  : hsstrt_reg  <= {data_in[7:0], 1'b0};
    HSSTOP  : hsstop_reg  <= {data_in[7:0], 1'b0};
    HCENTER : hcenter_reg <= {data_in[7:0], 1'b0};
    HBSTRT  : hbstrt_reg  <= {data_in[7:0], 1'b0};
    HBSTOP  : hbstop_reg  <= {data_in[7:0], 1'b0};
    VTOTAL  : vtotal_reg  <= data_in[10:0];
    VSSTRT  : vsstrt_reg  <= data_in[10:0];
    VSSTOP  : vsstop_reg  <= data_in[10:0];
    VBSTOP  : vbstop_reg  <= data_in[10:0];
endcase
```

Note the `{data_in[7:0], 1'b0}` — horizontal registers are stored shifted left by 1 because the beam counter runs at 2× the CCK rate in Minimig's pipeline. The effective value is `data_in[7:0]` CCKs (color clocks, 280 ns each in PAL). This is not a programming consideration — you write the CCK count directly.

Default values (Minimig `HTOTAL_VAL 227-1`, `VTOTAL_PAL_VAL 312-1`, `HSSTRT_VAL 29`, `HSSTOP_VAL 63-1`) match PAL/NTSC broadcast timings.

**Missing register**: VBSTRT is commented out in Minimig `(agnus_beamcounter.v:199)`: `//vbstrt_reg <= VBSTRT_VAL`. Minimig hardwires vertical blanking start to line 0 (`assign vbl = (vpos <= vbstop)` `(agnus_beamcounter.v:421)`). WinUAE does model VBSTRT as a programmable register. This is a Minimig limitation — for hardware accuracy with programmed modes that move VBL start (rare but used by some video generators), WinUAE is the authority.

### 12.5 Programmable sync timing relationships

When using programmed modes, the sync registers interact in specific ways:

| Condition | Result |
|-----------|--------|
| VARBEAMEN only | HTOTAL and VTOTAL are used, but sync timing stays hardwired |
| VARBEAMEN + VARHSYEN | HSSTRT/HSSTOP/HCENTER become active |
| VARBEAMEN + VARVSYEN | VSSTRT/VSSTOP become active |
| VARBEAMEN + VARVBEN | VBSTRT/VBSTOP become active |
| Any of the above | HARDDIS is forced on `(agnus_beamcounter.v:233-234)` |

Minimig's conditional muxing `(agnus_beamcounter.v:220-231)`:

```verilog
wire [8:0] htotal  =             varbeamen ? htotal_reg  : HTOTAL_VAL << 1;
wire [8:0] hsstrt  = varhsyen && varbeamen ? hsstrt_reg  : HSSTRT_VAL;
wire [8:0] hsstop  = varhsyen && varbeamen ? hsstop_reg  : HSSTOP_VAL;
wire [8:0] hcenter = varhsyen && varbeamen ? hcenter_reg : HCENTER_VAL;
wire [8:0] hbstrt  =             varbeamen ? hbstrt_reg  : HBSTRT_VAL;
wire [8:0] hbstop  =             varbeamen ? hbstop_reg  : HBSTOP_VAL;
wire [10:0] vtotal =             varbeamen ? vtotal_reg  : pal ? VTOTAL_PAL_VAL : VTOTAL_NTSC_VAL;
wire [10:0] vsstrt = varvsyen && varbeamen ? vsstrt_reg  : VSSTRT_VAL;
wire [10:0] vsstop = varvsyen && varbeamen ? vsstop_reg  : VSSTOP_VAL;
wire [10:0] vbstop = varvben  && varbeamen ? vbstop_reg  : pal ? VBSTOP_PAL_VAL : VBSTOP_NTSC_VAL;
```

The pattern is: HTOTAL, HBSTRT, HBSTOP are gated only on VARBEAMEN (they always become active in variable-beam mode). But HSSTRT, HSSTOP, HCENTER require *both* VARBEAMEN and VARHSYEN. Similarly, VSSTRT/VSSTOP require VARBEAMEN and VARVSYEN, and VBSTOP requires VARBEAMEN and VARVBEN.

This two-level gating allows partial programmed modes: you can change just the beam total (timing) without changing sync positions, or vice versa. Most software sets all the bits together, but some monitor drivers use partial mode for fine-tuning.

### 12.6 Interlace and vertical sync timing detail

Minimig models proper interlaced vertical sync with half-line offsets `(agnus_beamcounter.v:377-383)`:

```verilog
// PAL: Long field Vsync line 3 - 5.5, Short field: line 2.5 - 5
if ((vpos==vsstrt+1 && hpos==hsstrt && long_frame)
    || (vpos==vsstrt && hpos==hcenter && !long_frame))
    _vsync <= 0;
else if ((vpos==vsstop && hpos==hcenter && long_frame)
    || (vpos==vsstop && hpos==hsstrt && !long_frame))
    _vsync <= 1;
```

In the long field, vsync starts at line `vsstrt+1` at the hsync position. In the short field, it starts at line `vsstrt` at the HCENTER position (mid-line). This implements the half-line offset required by PAL interlace. The composite sync is derived from hsync AND vsync with serration pulses `(agnus_beamcounter.v:398)`:

```verilog
assign _csync = _hsync & _vsync | vser;
```

### 12.7 LOLDIS — long-line disable

BEAMCON0 bit 11 (LOLDIS) disables the NTSC long-line alternation. NTSC has 227.5 CCKs per line, implemented as alternating 227 and 228 CCK lines. When LOLDIS is set (or in PAL mode), every line is the same length `(agnus_beamcounter.v:272-278)`:

```verilog
always @(posedge clk) begin
  if (clk7_en) begin
    if (end_of_line)
      if (pal || (loldis && varbeamen))
        long_line <= 0;
      else if (!(loldis && varbeamen))
        long_line <= ~long_line;
  end
end
```

This is relevant for emulators because the alternating line length affects the exact horizontal position of vertical sync in NTSC mode. A display that cares about sub-CCK timing (genlock applications, some demos) needs the alternation modelled correctly. Most emulators can safely ignore it for games and productivity software.

### 12.8 Reading back horizontal/vertical position (VPOSR/VHPOSR)

ECS adds the high-bit read of VPOS/VHPOS. VPOSR already has the top 3 bits of VPOS at bits 2:0 (Minimig `(agnus_beamcounter.v:107)`); with VPOS at 11 bits (ECS) or 9 bits (OCS), the top 3 bits matter only when you're above line 256, which happens in programmed modes. Reading VPOSR + VHPOSR gives you the full 11+9=20 bits of beam position.

Note: there is no HHPOSW/HHPOSR as distinct registers in either Minimig or WinUAE. The HHPOS name in some HRM editions refers to an ECS extension that was never shipped. VHPOSR is the canonical read register.

### 12.9 BEAMCON0 default NTSC vs PAL

`(agnus_beamcounter.v:164-171)`:

```verilog
reg pal;
always @(posedge clk) begin
    if (reset)
        pal <= ~ntsc;
    else if (reg_address == BEAMCON0 && ecs)
        pal <= data_in[5];
end
```

So the runtime PAL/NTSC toggle is BEAMCON0 bit 5. OCS Agnus has no such bit — the chip is jumpered (8361 only has NTSC, 8367/8370/8371 can be switched by a single pin). ECS (8372A/8375) lets you switch in software via BEAMCON0.

A game that sets BEAMCON0 = $0000 on PAL hardware switches the chip to NTSC mode — vertical total goes from 312 to 262 and the display folds. This is how some demos do "NTSC fix" on PAL Amigas.

---

## 13. Summary table

| Chip | Part | Gen | Max chip RAM | Max bpl (LORES/HIRES/SHRES) | Max colours | Sprite mode | 32-bit fetch | New regs | Models |
|------|------|-----|--------------|-----------------------------|-------------|-------------|--------------|----------|--------|
| Agnus | 8361 | OCS | 512 KiB | 6/4/— | 64 (EHB) or 4096 (HAM) | 16 px lores | No | — | A1000 |
| Agnus | 8367 | OCS | 512 KiB | 6/4/— | 64/4096 | 16 px | No | — | A500 early |
| Agnus | 8370 (Fat) | OCS | 512 KiB | 6/4/— | 64/4096 | 16 px | No | — | A500, A2000 |
| Agnus | 8371 (Fat) | OCS | 1 MiB | 6/4/— | 64/4096 | 16 px | No | — | A500, A2000 |
| Denise | 8362 | OCS | — | 6/4/— | 64/4096 | 16 px LORES | No | — | all OCS |
| Paula | 8364 | OCS | — | — | — | — | — | — | all OCS/ECS/AGA |
| Agnus | 8372A | ECS | 1 MiB | 6/4/2 | 64/4096 | 16 px | No | BEAMCON0, HTOTAL/VTOTAL, VBSTRT/STOP, HSSTRT/STOP, VSSTRT/STOP, HCENTER, HBSTRT/STOP, DIWHIGH | A500+, A2000 rev 8 |
| Agnus | 8375 | ECS | 2 MiB | 6/4/2 | 64/4096 | 16 px | No | as 8372A | A600, A3000 |
| Denise | 8373 (Super) | ECS | — | 6/4/2 | 64/4096 | 16 px | No | BPLCON3 (BRDRBLNK/BRDNTRAN/ZDCLKEN/BRDSPRT/EXTBLKEN) | A500+, A600, A3000 |
| Alice | — | AGA | 2 MiB | 8/8/8 | 256 dir / 262144 HAM8 | 16/32/64 px LORES/HIRES/SHRES | Yes (FMODE 00/01/10/11) | FMODE, BPLCON4 | A1200, A4000, CD32 |
| Lisa | — | AGA | — | 8/8/8 | 256/262144 | 16/32/64 px | Yes | BPLCON3 (BANK/PF2OF/LOCT/SPRES), BPLCON4 (BPLAM/ESPRM/OSPRM), CLXCON2 | A1200, A4000, CD32 |
| Gary | 5719 | OCS/ECS support | — | — | — | — | — | — | A500, A2000 |
| Fat Gary | 390540 | AGA support | — | — | — | — | — | $DE0000 (timeout flags) | A3000, A4000 |
| Ramsey | 390537 (D), 390544 (F) | A3000/A4000 DRAM | — | — | — | — | — | $DE0003 CONFIG, $DE0043 VERSION | A3000, A4000 |
| Super Buster | 390539 | Zorro III | — | — | — | — | — | Z3 autoconfig path | A3000, A4000 |
| Gayle | 315507 | IDE/PCMCIA | — | — | — | — | — | $DA8000 CS, $DA9000 IRQ, $DAA000 INT, $DAB000 CFG, $DE1000 ID | A600, A1200 |
| Budgie | 391425 | A1200 glue | — | — | — | — | — | (invisible to software) | A1200 |
| Akiko | 391407 | CD32 glue | — | — | — | — | — | $B80000..$B80038 (C2P, CD, NVRAM) | CD32 |

---

## Appendix A — Complete AGA register bit table

### FMODE ($DFF1FC)

Reset: $0000. Gated on AGA (writes ignored in OCS/ECS).

| Bits | Name | Meaning |
|------|------|---------|
| 15 | SSCAN2 | Sprite scandouble enable (per-sprite via SPRxPOS SH10) |
| 14 | BSCAN2 | Bitplane scandouble — alternate BPL1MOD/BPL2MOD by line parity |
| 13:4 | — | reserved, masked by WinUAE (`v &= 0xC00F`) |
| 3 | SPR32 | Sprite fetch: bits 3:2 = 11 → 64-bit sprite |
| 2 | SPAGEM | Sprite fetch: bits 3:2 = 01 or 10 → 32-bit sprite |
| 1 | BPL32 | Bitplane fetch: bits 1:0 = 11 → 64-bit bitplane |
| 0 | BPAGEM | Bitplane fetch: bits 1:0 = 01 or 10 → 32-bit bitplane |

Source: `custom.cpp:3885` (mask), `custom.cpp:1076-1077` (decode), Minimig `agnus_bitplanedma.v:307-314`, `agnus_spritedma.v:148-165`.

### BPLCON0 ($DFF100)

Reset: $0000.

| Bit | Name | Meaning (AGA) |
|-----|------|---------------|
| 15 | HIRES | 1 = HIRES (35 ns / 70 ns pixel) |
| 14 | BPU2 | Bitplane count bit 2 |
| 13 | BPU1 | Bitplane count bit 1 |
| 12 | BPU0 | Bitplane count bit 0 |
| 11 | HAM | 1 = Hold-and-Modify |
| 10 | DBLPF | 1 = Dual-playfield |
| 9 | COLOR | 1 = Composite colour burst on |
| 8 | GAUD | 1 = Genlock audio enable |
| 7 | UHRES | UHRES pointers enable (reserved on Alice) |
| 6 | SHRES | 1 = SuperHires (35 ns pixel); ECS+AGA |
| 5 | BYPASS | 1 = bypass colour table (direct 8-bit out on R[7:0]) — AGA only |
| 4 | BPU3 | Bitplane count bit 3 (AGA only, bit-4 = 1 enables 8-plane mode when BPU=0..7 → 8..15 total, but only 8..15 up to 8 planes is meaningful) |
| 3 | LPEN | 1 = light pen enable |
| 2 | LACE | 1 = interlace |
| 1 | ERSY | 1 = external resync (genlock) |
| 0 | ECSENA | 1 = enable ECS/AGA BPLCON3 bits (BRDRBLNK etc.) |

Source: Minimig `denise.v:145-157`, `agnus_bitplanedma.v:282`.

### BPLCON1 ($DFF102)

Reset: $0000 (OCS), $3300 internal on Minimig (upper byte forced to pattern on non-AGA). AGA accepts full 16 bits.

| Bits | Name | Meaning |
|------|------|---------|
| 15:14 | PF2HBIT7:6 | Playfield 2 horizontal scroll high 2 bits (AGA) |
| 13:12 | PF2HBIT1:0 | Playfield 2 horizontal scroll low 2 bits (AGA) |
| 11:10 | PF1HBIT7:6 | Playfield 1 horizontal scroll high 2 bits (AGA) |
| 9:8 | PF1HBIT1:0 | Playfield 1 horizontal scroll low 2 bits (AGA) |
| 7:4 | PF2HBIT5:2 | Playfield 2 horizontal scroll middle 4 bits |
| 3:0 | PF1HBIT5:2 | Playfield 1 horizontal scroll middle 4 bits |

Reassembled: `pf1h = {bplcon1[11:10], bplcon1[3:0], bplcon1[9:8]}`, `pf2h = {bplcon1[15:14], bplcon1[7:4], bplcon1[13:12]}` `(denise_bitplanes.v:118, 130)`.

### BPLCON2 ($DFF104)

Reset: $0000.

| Bit | Name | Meaning |
|-----|------|---------|
| 15 | — | reserved |
| 14 | ZDBPSEL | Z-depth bitplane select (AGA) |
| 13 | ZDBPEN | Z-depth enable |
| 12 | ZDCTEN | Z-depth clock enable |
| 11 | KILLEHB | 1 = disable EHB (ECS+AGA) |
| 10 | RDRAM | 1 = read colour table via register bus (AGA) — Minimig: `bplcon2[8]` (some docs differ on bit 8 vs 10) |
| 9 | — | reserved |
| 8 | RDRAM | (actual bit in Minimig) `(denise.v:184)` — colour-table read enable |
| 7:6 | PF2P2..PF2P0 | Playfield 2 priority (3 bits on AGA, 2 on OCS) |
| 5:3 | PF1P2..PF1P0 | Playfield 1 priority |
| 2 | PF2PRI | 1 = playfield 2 has priority over playfield 1 |
| 1:0 | (playfield priority low bits, overlap with 5:3) | — |

Source: Minimig `denise.v:184`, `denise_playfields.v` (not fully quoted). **Note**: BPLCON2 has historical ambiguity in the HRM about which bit is RDRAM; Minimig models it as bit 8. Emulators targeting AGA accurately should follow Minimig.

### BPLCON3 ($DFF106)

Reset: $0C00.

| Bits | Name | Meaning | Gate |
|------|------|---------|------|
| 15:13 | BANK[2:0] | Colour-table bank select | AGA |
| 12:10 | PF2OF[2:0] | PF2 colour offset: 0=off, 1=+2, 2=+4, 3=+8, 4=+16, 5=+32, 6=+64, 7=+128 | AGA |
| 9 | LOCT | Low-colour-table-write (24-bit writes) | AGA |
| 8 | — | reserved | — |
| 7:6 | SPRES[1:0] | Sprite resolution: 00=auto, 01=LORES, 10=HIRES, 11=SHRES | AGA |
| 5 | BRDRBLNK | 1 = border area is forced blank-black | ECSENA |
| 4 | BRDNTRAN | 1 = border is non-transparent (genlock) | ECSENA |
| 3 | ZDCLKEN | 1 = ZD pin outputs clock | ECSENA |
| 2 | BRDSPRT | 1 = sprites allowed in border | ECSENA |
| 1 | — | reserved | — |
| 0 | EXTBLKEN | 1 = external blank input drives BLANK | ECSENA |

Source: Minimig `denise.v:203-220`.

### BPLCON4 ($DFF10C)

Reset: $0011 (OSPRM=1, ESPRM=0).

| Bits | Name | Meaning |
|------|------|---------|
| 15:8 | BPLAM[7:0] | Bitplane colour-index XOR mask (active from first BPL1DAT to DIWSTOP) |
| 7:4 | ESPRM[3:0] | Even sprite colour base (high 4 bits of colour index) |
| 3:0 | OSPRM[3:0] | Odd sprite colour base (high 4 bits of colour index) |

Source: Minimig `denise.v:228-258`.

### CLXCON2 ($DFF10E)

Reset: $0000.

| Bits | Name | Meaning |
|------|------|---------|
| 15:8 | — | reserved |
| 7 | ENBP8 | Bitplane 8 enable for collision |
| 6 | ENBP7 | Bitplane 7 enable for collision |
| 5:2 | — | reserved |
| 1 | MVBP8 | Bitplane 8 match value for collision |
| 0 | MVBP7 | Bitplane 7 match value for collision |

Source: WinUAE `drawing.cpp:3412-3413`:

```c
clxcon_bpl_match |= (clxcon2 & (0x01 | 0x02)) << 6;
clxcon_bpl_enable |= clxcon2 & (0x40 | 0x80);
```

CLXCON2 extends the OCS CLXCON register (which has 6 planes' worth of bits) to 8 planes on AGA.

### DIWHIGH ($DFF1E4)

Reset: $0000.

| Bits | Name | Meaning |
|------|------|---------|
| 15 | — | latch marker, masked |
| 14 | — | reserved, masked |
| 13 | DIWSTOP_H8 | horizontal stop bit 8 |
| 12 | DIWSTOP_H9 | horizontal stop bit 9 (AGA) |
| 11:8 | DIWSTOP_V10:V8 | vertical stop bits 10..8 |
| 7 | — | reserved, masked |
| 6 | — | reserved, masked |
| 5 | DIWSTRT_H8 | horizontal start bit 8 |
| 4 | DIWSTRT_H9 | horizontal start bit 9 (AGA) |
| 3:0 | DIWSTRT_V10:V8 | vertical start bits 10..8 |

Source: WinUAE `custom.cpp:3836-3852`, Minimig `agnus_bitplanedma.v:187-204`.

### SPRxPOS high bit (SH10, ECS extension)

ECS/AGA SPRxPOS bit 7 is SH10 (sprite scan-double bit 10). Minimig: `sprposh[pcsel] <= data_in[7]` `(agnus_spritedma.v:208)`. Used with FMODE bit 15 for sprite scan-doubling.

### SPRxCTL ECS/AGA extensions

`(agnus_spritedma.v:221)`:

```verilog
sprctl[pcsel] <= {data_in[15:8], data_in[6], data_in[5], data_in[2], data_in[1]};
```

- data_in[15:8] = VSTOP[7:0] (unchanged from OCS)
- data_in[6] = VSTART[9] (ECS — extends VSTART above 256)
- data_in[5] = VSTOP[9]
- data_in[2] = VSTART[8]
- data_in[1] = VSTOP[8]
- data_in[7] = attach
- data_in[4:3] = OCS bits (VSTART/VSTOP H0 etc.)

---

## Appendix B — Complete BEAMCON0 bit table

See section 12.1 for the full bit-by-bit table. This is the reference copy with reset values and mask macros from `include/custom.h:15-29`:

```c
#define BEAMCON0_HARDDIS    0x4000    // bit 14
#define BEAMCON0_LPENDIS    0x2000    // bit 13
#define BEAMCON0_VARVBEN    0x1000    // bit 12
#define BEAMCON0_LOLDIS     0x0800    // bit 11
#define BEAMCON0_CSCBEN     0x0400    // bit 10
#define BEAMCON0_VARVSYEN   0x0200    // bit  9
#define BEAMCON0_VARHSYEN   0x0100    // bit  8
#define BEAMCON0_VARBEAMEN  0x0080    // bit  7
#define BEAMCON0_DUAL       0x0040    // bit  6
#define BEAMCON0_PAL        0x0020    // bit  5
#define BEAMCON0_VARCSYEN   0x0010    // bit  4
#define BEAMCON0_BLANKEN    0x0008    // bit  3
#define BEAMCON0_CSYTRUE    0x0004    // bit  2
#define BEAMCON0_VSYTRUE    0x0002    // bit  1
#define BEAMCON0_HSYTRUE    0x0001    // bit  0
```

Reset: $0000 (NTSC), $0020 (PAL) — Minimig sets bit 5 based on the `ntsc` pin at reset `(agnus_beamcounter.v:119)`.

**Key combinations**:

| Value | Meaning |
|-------|---------|
| $0000 | NTSC hardwired (60 Hz, 262 lines) |
| $0020 | PAL hardwired (50 Hz, 312 lines) |
| $0480 | VARBEAMEN + CSCBEN — variable beam + composite sync |
| $04C0 | + DUAL — variable beam, dual-scan genlock |
| $0BE8 | Multiscan mode: HARDDIS + VARVBEN + VARVSYEN + VARHSYEN + VARBEAMEN + BLANKEN + HSYTRUE (typical AmigaOS programmed mode setup) |

---

## Appendix C — Chip revision matrix

| Model | CPU | Chipset | Agnus | Denise | Paula | Bridge | ROM | Max chip RAM |
|-------|-----|---------|-------|--------|-------|--------|-----|--------------|
| A1000 | 68000 @ 7.16 NTSC | OCS | 8361 | 8362 | 8364 | Daughterboard "Portia" | 1.x (bootstrap) | 256 KiB |
| A500 rev 3 | 68000 | OCS | 8370 (Fat) | 8362 | 8364 | Gary 5719 | 1.2/1.3 | 512 KiB |
| A500 rev 6A | 68000 | OCS | 8371 (Fat) | 8362 | 8364 | Gary 5719 | 1.3 | 1 MiB |
| A500+ | 68000 | ECS | 8375 | 8373 (Super) | 8364 | Gary 5719 | 2.04 | 1 MiB |
| A600 | 68000 | ECS | 8375 | 8373 | 8364 | Gayle | 2.05 | 2 MiB |
| A1000 PAL | 68000 | OCS | 8367 | 8362 | 8364 | — | 1.x | 512 KiB |
| A2000 | 68000 | OCS | 8370/8371 | 8362 | 8364 | Gary 5719 | 1.3/2.04 | 512 KiB / 1 MiB |
| A2000 rev 8 | 68000 | ECS | 8372A | 8373 | 8364 | Gary 5719 | 2.04 | 1 MiB |
| A3000 | 68030 @ 16/25 | ECS | 8372A | 8373 | 8364 | Fat Gary + Ramsey + Super Buster | 2.04/3.1 | 2 MiB |
| A1200 | 68EC020 @ 14 | AGA | Alice | Lisa | 8364R7 | Gayle + Budgie | 3.0/3.1 | 2 MiB |
| A4000 | 68EC030/40 | AGA | Alice | Lisa | 8364R7 | Fat Gary + Ramsey + Super Buster | 3.0/3.1 | 2 MiB |
| CD32 | 68EC020 @ 14 | AGA | Alice | Lisa | 8364R7 | Gayle + Akiko | 3.1 | 2 MiB |

Emulator configuration implications:

- A1000: special a1k flag affects EHB and VBL interrupt timing.
- A500/A500+: gary_coldboot bit and Gayle absence — no $DA/$DE I/O regions.
- A600: Gayle present, no AGA. ECS Denise only.
- A1200: Gayle + Alice + Lisa + Budgie. No Ramsey. 68EC020 CPU (no MMU).
- A4000: Ramsey + Super Buster + Fat Gary + Alice + Lisa. 68EC030 or 68040. MMU optional.
- CD32: Gayle (sort of — no IDE, just interrupt routing) + Akiko + Alice + Lisa. 68EC020.

---

## Appendix D — Gaps and thin spots

These are areas where the source material is thin or contradictory. An emulator author should treat these as "needs more research":

1. **FMODE bits 13:4** — Minimig only uses bits 15, 14, 3:0. WinUAE masks with 0xC00F. The HRM references unused intermediate bits that neither source models. They may be reserved or may control pad-memory / dual-playfield behaviour on very late Alice revisions. If your emulator has a game that sets one of these bits, you may need to consult a real A1200 schematic.

2. **BPLCON2 RDRAM bit position** — Minimig uses bit 8; some HRM editions say bit 10. This affects whether reading colour registers on AGA returns the stored value or zero. Test on real hardware if you care.

3. **BPLCON0 bit 7 (UHRES)** — Referenced in Agnus but not decoded by Minimig or WinUAE's active path. Comments say "enables the UHRES pointers; needs bits in DMACON also" `(denise.v:128)`. Never shipped on any production chip but reserved in the register map.

4. **Budgie internals** — Not modelled anywhere. If an emulator needs bus-accurate A1200 timing for the CPU-chip-RAM path, there is no public source. Commodore service manuals show the chip as a black box.

5. **Super Buster's locked-transfer cycle count** — WinUAE does not model Z3 burst timing at the cycle level. Any emulator that needs Z3 FastRAM to run at true hardware bandwidth has to reconstruct the transfer protocol from Commodore application notes.

6. **Ramsey DRAM probe pattern** — The famous $5AC35AC3 pattern used by Kickstart to sniff DRAM. WinUAE does not model it because the memory allocator reports exact sizes; real hardware uses the pattern to detect 1 MiB vs 4 MiB DIMMs. An emulator could implement this behaviour by having Ramsey override memory reads during probe to simulate missing banks.

7. **CIA ICR clearing race** — Real hardware loses interrupts when the read-clear happens at the exact cycle an interrupt source latches. WinUAE does not model this. If an emulator does cycle-exact CIA modelling, the race window is ~1 bus cycle.

8. **Paula audio volume quirks on AGA** — Paula is unchanged, but the A1200/A4000 output stages have different analogue characteristics from the A500. Not a chipset issue but worth mentioning.

9. **Sprite SH9 bit** — ECS added per-sprite bit for VSTART[9]; AGA uses the same bit. The "SH10" name in some docs is inconsistent — Minimig stores 10 bits of VSTART (SH10 at bit 9 of `vstart`). The exact semantic of the bit for sprites above line 512 is not fully covered.

10. **BPLCON3 bits 4, 3, 2, 0** (BRDNTRAN, ZDCLKEN, BRDSPRT, EXTBLKEN) — Only BRDRBLNK (bit 5) and BRDSPRT (bit 1) are active in Minimig's decode; the others are commented out. WinUAE's `check_exthblank` path does use EXTBLKEN (via bit 0) for programmed-mode centering. Treat as "partially modelled".

11. **VBSTRT programmable register** — Minimig hardwires VBL start to line 0 and does not implement VBSTRT as a writable register `(agnus_beamcounter.v:199)`. WinUAE models VBSTRT. For normal games this doesn't matter; for custom video generators or genlock equipment that need VBL to start at a non-zero line, the Minimig implementation is incomplete.

12. **HAM8 bank interaction** — The HAM generator in Minimig has its own colour RAM instance `(denise_hamgenerator.v:37-48)` that always writes (no `rdram` gate). The question of whether HAM8 "set colour" instructions respect the current BANK (BPLCON3[15:13]) or always use bank 0 is architecture-dependent. Minimig's HAM lookup uses a 6-bit index `{2'b00, select_xored[7:2]}` for HAM8, which is always within the first 64 entries — effectively bank-independent. WinUAE's HAM implementation may differ in corner cases. Test with real hardware if HAM8 + colour banking matters.

13. **BPLCON4 BPLAM timing edge case** — Minimig's BPLAM XOR takes effect from the first BPL1DAT write and clears at DIWSTOP `(denise.v:241-244)`. WinUAE has a note: "AMR - bplxor is active from first write to BPL1DAT and end of scanline." The comment adds "(Fixes Andromeda: Nexus 7, shade cluster part)". The exact activation and deactivation cycle of BPLAM is a known source of per-demo compatibility differences.

14. **Sprite 64-pixel bitplane-data bleed** — WinUAE documents an undocumented AGA feature where 64-pixel-wide sprites can have their first 32 pixels replaced with bitplane data if SPRxDATx is written at the same cycle as a bitplane DMA fetch `(custom.cpp:4181-4191)`. This is `#if 0`'d in WinUAE and not modelled in Minimig. Unknown whether any software depends on this.

15. **CLXCON2 reset on CLXCON write** — Minimig resets CLXCON2 to zero whenever CLXCON is written `(denise_collision.v:42-48)`. This interaction is not documented in the HRM. Software that writes CLXCON and then expects CLXCON2 to retain its previous value will be surprised on real hardware (and on Minimig).

16. **Gayle AGA ID variant** — WinUAE returns a different Gayle ID sequence for AGA machines ($D1 vs $D0) by adding an extra high bit at read count 7 `(gayle.cpp:838)`. Minimig does not distinguish AGA Gayle from ECS Gayle in its ID sequence. Unknown how much software relies on the $D1 variant.

17. **FMODE change delay** — WinUAE delays FMODE changes by 2 CCKs: `event2_newevent_xx(-1, 2 * CYCLE_UNIT, fmode, setup_fmodes_delayed)` `(custom.cpp:1121)`. Minimig's FMODE latches on the next clock edge with no pipeline delay. Real hardware likely has a 1-2 CCK pipeline delay for the fetch-mode change to propagate. Games that change FMODE mid-frame (rare) may depend on the exact delay.

---

## Appendix E — Source map

All references to line numbers are to the files as they exist at:

- `~/Projects/Emu198x-Unclean/Minimig-AGA_MiSTer/rtl/`
- `~/Projects/Emu198x-Unclean/WinUAE/`

Minimig Verilog files touched:

| File | Lines | What it covers |
|------|-------|----------------|
| `agnus.v` | 1–120 | Top-level Agnus wiring, DMA slot ownership documentation |
| `agnus_beamcounter.v` | 60–438 | BEAMCON0, HTOTAL/VTOTAL/HS*/VS*/VB*/HB*/HCENTER, VPOSR return value with ECS/AGA bits, interlace |
| `agnus_bitplanedma.v` | 51–510 | DDF logic, BPLCON0 shadow, FMODE decode for bitplane fetch width, 32/64-bit pointer increment, modulo handling, plane encoder, DIWHIGH for vertical |
| `agnus_spritedma.v` | 100–316 | Sprite pointer fetch, FMODE decode for sprite width, scan-double via SPRxPOS SH10 + FMODE[15] |
| `denise.v` | 55–481 | BPLCON0/2/3/4 decode, DENISEID, HAM8/BPLAM, colour-table read path, EHB gate, border-blank |
| `denise_bitplanes.v` | 45–390 | BPLCON1 extended scroll, BPLxDAT 64-bit buffers, FMODE fetch 32/64-bit chip48 path, extra-delay alignment |
| `denise_sprites.v` | 44–326 | Sprite shifter instantiation, SPRES-based shift rate, attachment semantic (AGA differs from OCS), sprite colour assembly from ESPRM/OSPRM |
| `denise_colortable.v` | 1–71 | 256-entry colour bank, LOCT 24-bit double-write, byte-enable masks, EHB shift |
| `denise_colortable_ram_mf.v` | 1–227 | Dual-port 256x32-bit RAM with byte-enable, Altera FPGA primitive |
| `denise_hamgenerator.v` | 1–98 | HAM6/HAM8 decoder, per-gun modify logic, separate colour RAM for HAM, BPLXOR interaction |
| `denise_bitplane_shifter.v` | 1–131 | Bitplane parallel-to-serial, FMODE-dependent scroll depth (16/32/64-bit), super-hires scroller |
| `denise_playfields.v` | 1–105 | Single/dual playfield, PF2OF offset decode, OCS BPU=5 undocumented quirk |
| `denise_collision.v` | 1–121 | CLXCON/CLXCON2 match/enable, 15-bit collision register, CLXCON2 reset on CLXCON write |
| `gayle.v` | 21–215 | IDE task-file decode, GAYLEID sequence, CS/IRQ/INT/CFG register routing, 32-bit IDE support |
| `gary.v` | 1–187 | Address decode, kickstart overlay, CIA/IDE/Gayle/RTC/RTG select, slow-RAM banking |
| `akiko.v` | 1–61 | Minimal stub: ID ($C0CACAFE) + C2P transpose only |
| `ciaa.v` | 1–80+ | 8520 CIA implementation (simplified), no cascade race modelling |
| `paula.v` | (303 lines) | No AGA-specific content — confirms Paula is unchanged |

WinUAE C++ files touched:

| File | Lines | What it covers |
|------|-------|----------------|
| `include/custom.h` | 15–29 | BEAMCON0_* bit macros |
| `include/inputdevice.h` | 24–35 | JOYBUTTON_CD32_* macros (pad button indices) |
| `custom.cpp` | 3785–3897 | BPLCON3, BPLCON4, DIWSTRT/STOP/HIGH, DDFSTRT, FMODE handlers |
| `custom.cpp` | 4257–4274 | CLXCON2 storage |
| `custom.cpp` | 1060–1120 | FMODE decode (`setup_fmodes`, `fetchmode_*`) |
| `custom.cpp` | 1434–2232 | BEAMCON0 effect on beamcounter reinit |
| `custom.cpp` | 3547–3692 | BEAMCON0 write path, HARDDIS/VARVBEN gating |
| `akiko.cpp` | 17–140 | Akiko register map in comments |
| `akiko.cpp` | 317–407 | C2P algorithm (reference + precalc) |
| `akiko.cpp` | 1692–1830 | Akiko read/write dispatch, CDROM_FLAGS, INTREQ |
| `gayle.cpp` | 72–144 | Gayle memory map and CS/IRQ/INT/CFG bit macros |
| `gayle.cpp` | 893–1019 | Ramsey CONFIG/VERSION register, Gary coldboot/toenb/timeout |
| `cia.cpp` | 71–892 | TOD BCD-vs-binary (`getciatod`/`setciatod`), TODMED bug, ICR clearing |
| `cia.cpp` | 1431–1601 | Timer read/write paths, E-clock mode, 6526-vs-8520 switch |
| `inputdevice.cpp` | 3820–4103 | CD32 pad protocol (`handle_cd32_joystick_cia`, `handle_joystick_potgor`) |
| `expansion.cpp` | 99–303 | Zorro II and Zorro III size codes, force_z3, autoconfig comments |

---

## Appendix F — Minimig vs WinUAE cross-reference notes

Where both sources model the same register or behaviour, the following observations apply:

### Agreement (high confidence)

| Feature | Status |
|---------|--------|
| FMODE bitplane width decode (bits 1:0) | Both agree: 00=16, 01/10=32, 11=64 |
| FMODE sprite width decode (bits 3:2) | Both agree: same mapping |
| BPLCON3 bit layout | Both agree on all bit positions |
| BPLCON4 reset value ($0011) | Both agree |
| DENISEID return values ($FFFF/$FFFC/$00F8) | Both agree |
| BEAMCON0 bit positions | Minimig decode matches WinUAE `BEAMCON0_*` macros exactly |
| CLXCON2 reset on CLXCON write | Both implement this |
| Sprite attachment AGA rule (odd sprite attach only) | Both implement `attach1 || (!aga && attach0)` |
| BPU 4-bit on AGA, 3-bit clamp on OCS | Both implement the same clamp |
| DIWHIGH write-order dependency (DIWSTRT/DIWSTOP clears high bits) | Both model this |
| Gayle IDE task-file at 4-byte stride | Both agree |
| CIA 8520 binary TOD (vs 6526 BCD) | Both implement the same conversion |
| TODMED BCD counter bug | Both implement via `cs_ciatodbug` / Minimig doesn't need it |

### Disagreement or divergence

| Feature | Minimig | WinUAE | Notes |
|---------|---------|--------|-------|
| BPLCON2 RDRAM bit position | Bit 8 | Bit 8 (code) but some comments suggest bit 10 | Both use bit 8 in practice |
| FMODE change delay | Immediate latch | 2 CCK delay | WinUAE more accurate |
| VBSTRT register | Not implemented | Implemented | WinUAE more complete |
| Gayle ID sequence length | 4-state (2-bit counter) | 8-state with AGA/CD32 variants | WinUAE more complete |
| Akiko completeness | C2P + ID only | Full CD/NVRAM/C2P/INTREQ | WinUAE far more complete |
| BPLCON3 bits 4/3/0 | Commented out (not decoded) | Active in some paths | Partial disagreement |
| Sprite 64-pixel bitplane bleed | Not modelled | Documented but `#if 0` | Neither fully implements |
| DDFSTRT write timing block | Modelled via `ddfstrt_sel` | Modelled differently | Both address the same race |
| HAM generator bank independence | Separate colour RAM | Shared rendering path | Different implementations, same visible result |

### Recommendations for emulator authors

1. **Start with WinUAE for CIA and Akiko** — these are the most complex and most software-tested implementations.
2. **Start with Minimig for register bit positions** — the Verilog is unambiguous about which bit does what, while WinUAE's C++ can obscure bit positions behind macros and history.
3. **Use both for FMODE/BPLCON3/BPLCON4** — they agree on the critical bits and cross-validate each other.
4. **Trust Minimig's DMA sequencer for cycle-accurate Alice** — the Verilog is a direct hardware description, making DMA slot timing explicit. WinUAE's fetch model is abstracted for performance.
5. **Trust WinUAE for edge cases and software compatibility** — WinUAE has been tested against thousands of games and demos. Minimig has been tested against hundreds. When they disagree on a subtle timing point, WinUAE is more likely to match real hardware.

---

*This document should be read as a companion to `amiga-hardware-reference.md` (OCS/ECS custom-chipset registers from the manuals) and `amiga-graphics-display.md` (OCS bitplane/sprite/HAM semantics). Everything here is strictly "what the manuals don't cover or got wrong for late chipsets". For Paula, disk, audio, and copper/blitter details that are unchanged by ECS/AGA, see the earlier documents.*
