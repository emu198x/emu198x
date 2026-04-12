# Amiga Headers Reference

**Authoritative bit-level hardware register and struct reference for emulator authors.**

This document reproduces the NDK 3.9 C headers **verbatim** for every custom chip
register, exec kernel struct, DOS struct, graphics struct, device struct, intuition
struct, and resource struct that an emulator author needs. The goal is a single
copy-and-paste source for Rust `#[repr(C)]` translations — every field offset,
every bit constant, every LVO is present.

For semantic explanations of how these structures are *used* (the "why" and the
"when"), see the companion documents:

- `amiga-hardware-reference.md` — custom chips and CIAs at a conceptual level
- `amiga-exec-kernel.md` — Exec tasks, libraries, lists, messages, semaphores
- `amiga-dos-filesystem-disk.md` — DOS library, file handlers, BCPL interfacing
- `amiga-graphics-display.md` — copper, bitplanes, sprites, display
- `amiga-io-audio-expansion.md` — audio, serial, parallel, expansion
- `amiga-boot-process.md` — ROMTags, autoconfig, Kickstart boot

## Legal note

The C headers reproduced here are `(C) Copyright 1985-2001 Amiga, Inc. All Rights
Reserved` and are distributed as part of the NDK 3.9 (developer-only). They are
included in this document as a consolidated reference for emulator development —
reading a header file to learn the shape of a struct is not copyrightable, and the
resulting Rust translations will be clean-room original code. Do **not** redistribute
this document or copy header sources into library code you ship; refer back to the
official NDK for any commercial use.

## Conventions

- Each header is preceded by a `// Source: ...` comment and a one-line description.
- Headers that `#include` another header keep the include directive — see that
  header's subsection for its contents.
- Bit/flag constants are paired: `XXXB_yyy` is the **bit number** (0..31); `XXXF_yyy`
  (or `XXXF_yyy` / `XXX_yyy`) is the **mask** (`1 << XXXB_yyy`). This is an Amiga
  convention that pervades every header.
- For structs, the field order *is* the memory layout. There is no padding inserted
  by the compiler beyond natural m68k alignment rules (UWORD on 2-byte, ULONG on
  2-byte — the m68k allows 32-bit values on 16-bit boundaries).
- `BPTR` and `BSTR` are **BCPL pointers**: the byte address shifted right by 2.
  Convert with `BADDR(x) = ((APTR)((ULONG)(x) << 2))` and `MKBADDR(x) = (x >> 2)`.

---

# 2. Custom chip registers ($DFF000)

Cross-reference: `amiga-hardware-reference.md` — "Custom chip" section for semantic
descriptions of each register's purpose.

The `Custom` struct is **the** hardware layout at $DFF000. Every field's byte offset
from `$DFF000` is its register address. An emulator maps this struct directly over
the 512-byte custom chip region.

// Source: NDK_3.9/Include/include_h/hardware/custom.h
// The Custom struct at $DFF000. Every UWORD is one 16-bit register.

```c
#ifndef	HARDWARE_CUSTOM_H
#define	HARDWARE_CUSTOM_H
/*
**	$VER: custom.h 39.1 (18.9.1992)
**	Includes Release 45.1
**
**	Offsets of Amiga custom chip registers
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif /* EXEC_TYPES_H */



/*
 * do this to get base of custom registers:
 * extern struct Custom custom;
 */


struct Custom {
    UWORD   bltddat;
    UWORD   dmaconr;
    UWORD   vposr;
    UWORD   vhposr;
    UWORD   dskdatr;
    UWORD   joy0dat;
    UWORD   joy1dat;
    UWORD   clxdat;
    UWORD   adkconr;
    UWORD   pot0dat;
    UWORD   pot1dat;
    UWORD   potinp;
    UWORD   serdatr;
    UWORD   dskbytr;
    UWORD   intenar;
    UWORD   intreqr;
    APTR    dskpt;
    UWORD   dsklen;
    UWORD   dskdat;
    UWORD   refptr;
    UWORD   vposw;
    UWORD   vhposw;
    UWORD   copcon;
    UWORD   serdat;
    UWORD   serper;
    UWORD   potgo;
    UWORD   joytest;
    UWORD   strequ;
    UWORD   strvbl;
    UWORD   strhor;
    UWORD   strlong;
    UWORD   bltcon0;
    UWORD   bltcon1;
    UWORD   bltafwm;
    UWORD   bltalwm;
    APTR    bltcpt;
    APTR    bltbpt;
    APTR    bltapt;
    APTR    bltdpt;
    UWORD   bltsize;
    UBYTE   pad2d;
    UBYTE   bltcon0l;	/* low 8 bits of bltcon0, write only */
    UWORD   bltsizv;
    UWORD   bltsizh;	/* 5e */
    UWORD   bltcmod;
    UWORD   bltbmod;
    UWORD   bltamod;
    UWORD   bltdmod;
    UWORD   pad34[4];
    UWORD   bltcdat;
    UWORD   bltbdat;
    UWORD   bltadat;
    UWORD   pad3b[3];
    UWORD   deniseid;	/* 7c */
    UWORD   dsksync;
    ULONG   cop1lc;
    ULONG   cop2lc;
    UWORD   copjmp1;
    UWORD   copjmp2;
    UWORD   copins;
    UWORD   diwstrt;
    UWORD   diwstop;
    UWORD   ddfstrt;
    UWORD   ddfstop;
    UWORD   dmacon;
    UWORD   clxcon;
    UWORD   intena;
    UWORD   intreq;
    UWORD   adkcon;
    struct  AudChannel {
      UWORD *ac_ptr; /* ptr to start of waveform data */
      UWORD ac_len;	/* length of waveform in words */
      UWORD ac_per;	/* sample period */
      UWORD ac_vol;	/* volume */
      UWORD ac_dat;	/* sample pair */
      UWORD ac_pad[2];	/* unused */
    } aud[4];
    APTR    bplpt[8];
    UWORD   bplcon0;
    UWORD   bplcon1;
    UWORD   bplcon2;
    UWORD   bplcon3;
    UWORD   bpl1mod;
    UWORD   bpl2mod;
    UWORD   bplcon4;
    UWORD   clxcon2;
    UWORD   bpldat[8];
    APTR    sprpt[8];
    struct  SpriteDef {
      UWORD pos;
      UWORD ctl;
      UWORD dataa;
      UWORD datab;
    } spr[8];
    UWORD   color[32];
    UWORD htotal;
    UWORD hsstop;
    UWORD hbstrt;
    UWORD hbstop;
    UWORD vtotal;
    UWORD vsstop;
    UWORD vbstrt;
    UWORD vbstop;
    UWORD sprhstrt;
    UWORD sprhstop;
    UWORD bplhstrt;
    UWORD bplhstop;
    UWORD hhposw;
    UWORD hhposr;
    UWORD beamcon0;
    UWORD hsstrt;
    UWORD vsstrt;
    UWORD hcenter;
    UWORD diwhigh;	/* 1e4 */
    UWORD padf3[11];
    UWORD fmode;
};

#ifdef ECS_SPECIFIC

/* defines for beamcon register */
#define VARVBLANK	0x1000	/* Variable vertical blank enable */
#define LOLDIS		0x0800	/* long line disable */
#define CSCBLANKEN	0x0400	/* redirect composite sync */
#define VARVSYNC	0x0200	/* Variable vertical sync enable */
#define VARHSYNC	0x0100	/* Variable horizontal sync enable */
#define VARBEAM	0x0080	/* variable beam counter enable */
#define DISPLAYDUAL	0x0040	/* use UHRES pointer and standard pointers */
#define DISPLAYPAL	0x0020	/* set decodes to generate PAL display */
#define VARCSYNC	0x0010	/* Variable composite sync enable */
#define CSBLANK	0x0008	/* Composite blank out to CSY* pin */
#define CSYNCTRUE	0x0004	/* composite sync true signal */
#define VSYNCTRUE	0x0002	/* vertical sync true */
#define HSYNCTRUE	0x0001	/* horizontal sync true */

/* new defines for bplcon0 */
#define USE_BPLCON3	1

/* new defines for bplcon2 */
#define BPLCON2_ZDCTEN		(1<<10) /* colormapped genlock bit */
#define BPLCON2_ZDBPEN		(1<<11) /* use bitplane as genlock bits */
#define BPLCON2_ZDBPSEL0	(1<<12) /* three bits to select one */
#define BPLCON2_ZDBPSEL1	(1<<13) /* of 8 bitplanes in */
#define BPLCON2_ZDBPSEL2	(1<<14) /* ZDBPEN genlock mode */

/* defines for bplcon3 register */
#define BPLCON3_EXTBLNKEN	(1<<0)	/* external blank enable */
#define BPLCON3_EXTBLKZD	(1<<1)	/* external blank ored into trnsprncy */
#define BPLCON3_ZDCLKEN	(1<<2)	/* zd pin outputs a 14mhz clock*/
#define BPLCON3_BRDNTRAN	(1<<4)	/* border is opaque */
#define BPLCON3_BRDNBLNK	(1<<5)	/* border is opaque */

#endif	/* ECS_SPECIFIC */

#endif	/* HARDWARE_CUSTOM_H */
```

### Custom register offset table

Reconstructed from `custom.h`. All offsets are hexadecimal, size in bytes. `R`
means read-only (register appears only in `dmaconr`/`intenar`/etc.), `W` means
write-only (register appears only in `dmacon`/`intena`/etc.), `R/W` is both.

| Offset | Register   | R/W | Size | Purpose                                         |
|-------:|-----------|:---:|-----:|-------------------------------------------------|
| $000   | `bltddat  ` | R   | 2 | Blitter destination data (early strobe) |
| $002   | `dmaconr  ` | R   | 2 | DMA control read |
| $004   | `vposr    ` | R   | 2 | Vertical beam position + LOF + chipset id |
| $006   | `vhposr   ` | R   | 2 | Vertical+horizontal beam position |
| $008   | `dskdatr  ` | R   | 2 | Disk data read (unused — use DMA) |
| $00A   | `joy0dat  ` | R   | 2 | Joystick 0 mouse counters |
| $00C   | `joy1dat  ` | R   | 2 | Joystick 1 mouse counters |
| $00E   | `clxdat   ` | R   | 2 | Collision data |
| $010   | `adkconr  ` | R   | 2 | Audio/disk control read |
| $012   | `pot0dat  ` | R   | 2 | Pot 0 count |
| $014   | `pot1dat  ` | R   | 2 | Pot 1 count |
| $016   | `potinp   ` | R   | 2 | Pot input register |
| $018   | `serdatr  ` | R   | 2 | Serial data + status read |
| $01A   | `dskbytr  ` | R   | 2 | Disk byte read |
| $01C   | `intenar  ` | R   | 2 | Interrupt enable read |
| $01E   | `intreqr  ` | R   | 2 | Interrupt request read |
| $020   | `dskpth   ` | W   | 2 | Disk pointer high |
| $022   | `dskptl   ` | W   | 2 | Disk pointer low |
| $024   | `dsklen   ` | W   | 2 | Disk length (DMA enable) |
| $026   | `dskdat   ` | W   | 2 | Disk data write |
| $028   | `refptr   ` | W   | 2 | Refresh pointer |
| $02A   | `vposw    ` | W   | 2 | Write vertical position |
| $02C   | `vhposw   ` | W   | 2 | Write vertical/horizontal |
| $02E   | `copcon   ` | W   | 2 | Copper control — DANGER bit |
| $030   | `serdat   ` | W   | 2 | Serial data write |
| $032   | `serper   ` | W   | 2 | Serial period |
| $034   | `potgo    ` | W   | 2 | Pot go (start counters) |
| $036   | `joytest  ` | W   | 2 | Joystick/mouse test |
| $038   | `strequ   ` | S   | 2 | Strobe equalisation |
| $03A   | `strvbl   ` | S   | 2 | Strobe VBL |
| $03C   | `strhor   ` | S   | 2 | Strobe horizontal |
| $03E   | `strlong  ` | S   | 2 | Strobe long |
| $040   | `bltcon0  ` | W   | 2 | Blitter control 0 (minterm, source enable, ASHIFT) |
| $042   | `bltcon1  ` | W   | 2 | Blitter control 1 (line mode, fill, BSHIFT) |
| $044   | `bltafwm  ` | W   | 2 | Blitter source A first-word mask |
| $046   | `bltalwm  ` | W   | 2 | Blitter source A last-word mask |
| $048   | `bltcpth  ` | W   | 2 | Blitter source C pointer high |
| $04A   | `bltcptl  ` | W   | 2 | Blitter source C pointer low |
| $04C   | `bltbpth  ` | W   | 2 | Blitter source B pointer high |
| $04E   | `bltbptl  ` | W   | 2 | Blitter source B pointer low |
| $050   | `bltapth  ` | W   | 2 | Blitter source A pointer high |
| $052   | `bltaptl  ` | W   | 2 | Blitter source A pointer low |
| $054   | `bltdpth  ` | W   | 2 | Blitter destination pointer high |
| $056   | `bltdptl  ` | W   | 2 | Blitter destination pointer low |
| $058   | `bltsize  ` | W   | 2 | Blitter size (starts blit) |
| $05A   | `bltcon0l ` | W   | 1 | Blitter control 0 low byte (ECS+) |
| $05C   | `bltsizv  ` | W   | 2 | Blitter vertical size (big blits, ECS+) |
| $05E   | `bltsizh  ` | W   | 2 | Blitter horizontal size (big blits, ECS+) |
| $060   | `bltcmod  ` | W   | 2 | Blitter source C modulo |
| $062   | `bltbmod  ` | W   | 2 | Blitter source B modulo |
| $064   | `bltamod  ` | W   | 2 | Blitter source A modulo |
| $066   | `bltdmod  ` | W   | 2 | Blitter destination modulo |
| $070   | `bltcdat  ` | W   | 2 | Blitter source C data |
| $072   | `bltbdat  ` | W   | 2 | Blitter source B data |
| $074   | `bltadat  ` | W   | 2 | Blitter source A data |
| $07C   | `deniseid ` | R   | 2 | Denise/Lisa chipset id (ECS+) |
| $07E   | `dsksync  ` | W   | 2 | Disk sync pattern |
| $080   | `cop1lch  ` | W   | 2 | Copper list 1 pointer high |
| $082   | `cop1lcl  ` | W   | 2 | Copper list 1 pointer low |
| $084   | `cop2lch  ` | W   | 2 | Copper list 2 pointer high |
| $086   | `cop2lcl  ` | W   | 2 | Copper list 2 pointer low |
| $088   | `copjmp1  ` | S   | 2 | Copper restart 1 (strobe) |
| $08A   | `copjmp2  ` | S   | 2 | Copper restart 2 (strobe) |
| $08C   | `copins   ` | W   | 2 | Copper inst fetch (dummy) |
| $08E   | `diwstrt  ` | W   | 2 | Display window start |
| $090   | `diwstop  ` | W   | 2 | Display window stop |
| $092   | `ddfstrt  ` | W   | 2 | Display data fetch start |
| $094   | `ddfstop  ` | W   | 2 | Display data fetch stop |
| $096   | `dmacon   ` | W   | 2 | DMA control (set/clear) |
| $098   | `clxcon   ` | W   | 2 | Collision control |
| $09A   | `intena   ` | W   | 2 | Interrupt enable (set/clear) |
| $09C   | `intreq   ` | W   | 2 | Interrupt request (set/clear) |
| $09E   | `adkcon   ` | W   | 2 | Audio/disk control (set/clear) |
| $0A0   | `aud0lch  ` | W   | 2 | Audio 0 pointer high |
| $0A2   | `aud0lcl  ` | W   | 2 | Audio 0 pointer low |
| $0A4   | `aud0len  ` | W   | 2 | Audio 0 length |
| $0A6   | `aud0per  ` | W   | 2 | Audio 0 period |
| $0A8   | `aud0vol  ` | W   | 2 | Audio 0 volume |
| $0AA   | `aud0dat  ` | W   | 2 | Audio 0 data |
| $0B0   | `aud1lch  ` | W   | 2 | Audio 1 pointer high |
| $0B2   | `aud1lcl  ` | W   | 2 | Audio 1 pointer low |
| $0B4   | `aud1len  ` | W   | 2 | Audio 1 length |
| $0B6   | `aud1per  ` | W   | 2 | Audio 1 period |
| $0B8   | `aud1vol  ` | W   | 2 | Audio 1 volume |
| $0BA   | `aud1dat  ` | W   | 2 | Audio 1 data |
| $0C0   | `aud2lch  ` | W   | 2 | Audio 2 pointer high |
| $0C2   | `aud2lcl  ` | W   | 2 | Audio 2 pointer low |
| $0C4   | `aud2len  ` | W   | 2 | Audio 2 length |
| $0C6   | `aud2per  ` | W   | 2 | Audio 2 period |
| $0C8   | `aud2vol  ` | W   | 2 | Audio 2 volume |
| $0CA   | `aud2dat  ` | W   | 2 | Audio 2 data |
| $0D0   | `aud3lch  ` | W   | 2 | Audio 3 pointer high |
| $0D2   | `aud3lcl  ` | W   | 2 | Audio 3 pointer low |
| $0D4   | `aud3len  ` | W   | 2 | Audio 3 length |
| $0D6   | `aud3per  ` | W   | 2 | Audio 3 period |
| $0D8   | `aud3vol  ` | W   | 2 | Audio 3 volume |
| $0DA   | `aud3dat  ` | W   | 2 | Audio 3 data |
| $0E0   | `bpl1pth  ` | W   | 2 | Bitplane 1 pointer high |
| $0E2   | `bpl1ptl  ` | W   | 2 | Bitplane 1 pointer low |
| $0E4   | `bpl2pth  ` | W   | 2 | Bitplane 2 pointer high |
| $0E6   | `bpl2ptl  ` | W   | 2 | Bitplane 2 pointer low |
| $0E8   | `bpl3pth  ` | W   | 2 | Bitplane 3 pointer high |
| $0EA   | `bpl3ptl  ` | W   | 2 | Bitplane 3 pointer low |
| $0EC   | `bpl4pth  ` | W   | 2 | Bitplane 4 pointer high |
| $0EE   | `bpl4ptl  ` | W   | 2 | Bitplane 4 pointer low |
| $0F0   | `bpl5pth  ` | W   | 2 | Bitplane 5 pointer high |
| $0F2   | `bpl5ptl  ` | W   | 2 | Bitplane 5 pointer low |
| $0F4   | `bpl6pth  ` | W   | 2 | Bitplane 6 pointer high |
| $0F6   | `bpl6ptl  ` | W   | 2 | Bitplane 6 pointer low |
| $0F8   | `bpl7pth  ` | W   | 2 | Bitplane 7 pointer high (AGA) |
| $0FA   | `bpl7ptl  ` | W   | 2 | Bitplane 7 pointer low (AGA) |
| $0FC   | `bpl8pth  ` | W   | 2 | Bitplane 8 pointer high (AGA) |
| $0FE   | `bpl8ptl  ` | W   | 2 | Bitplane 8 pointer low (AGA) |
| $100   | `bplcon0  ` | W   | 2 | Bitplane control 0 (depth, hires, HAM, dualpf) |
| $102   | `bplcon1  ` | W   | 2 | Bitplane control 1 (fine scroll) |
| $104   | `bplcon2  ` | W   | 2 | Bitplane control 2 (sprite-playfield priority) |
| $106   | `bplcon3  ` | W   | 2 | Bitplane control 3 (AGA palette bank, sprite base) |
| $108   | `bpl1mod  ` | W   | 2 | Bitplane modulo odd |
| $10A   | `bpl2mod  ` | W   | 2 | Bitplane modulo even |
| $10C   | `bplcon4  ` | W   | 2 | Bitplane control 4 (AGA) |
| $10E   | `clxcon2  ` | W   | 2 | Collision control 2 (AGA) |
| $110   | `bpl1dat  ` | W   | 2 | Bitplane 1 data (dummy; filled by DMA) |
| $112   | `bpl2dat  ` | W   | 2 | Bitplane 2 data |
| $114   | `bpl3dat  ` | W   | 2 | Bitplane 3 data |
| $116   | `bpl4dat  ` | W   | 2 | Bitplane 4 data |
| $118   | `bpl5dat  ` | W   | 2 | Bitplane 5 data |
| $11A   | `bpl6dat  ` | W   | 2 | Bitplane 6 data |
| $11C   | `bpl7dat  ` | W   | 2 | Bitplane 7 data (AGA) |
| $11E   | `bpl8dat  ` | W   | 2 | Bitplane 8 data (AGA) |
| $120   | `spr0pth  ` | W   | 2 | Sprite 0 pointer high |
| $122   | `spr0ptl  ` | W   | 2 | Sprite 0 pointer low |
| $124   | `spr1pth  ` | W   | 2 | Sprite 1 pointer high |
| $126   | `spr1ptl  ` | W   | 2 | Sprite 1 pointer low |
| $128   | `spr2pth  ` | W   | 2 | Sprite 2 pointer high |
| $12A   | `spr2ptl  ` | W   | 2 | Sprite 2 pointer low |
| $12C   | `spr3pth  ` | W   | 2 | Sprite 3 pointer high |
| $12E   | `spr3ptl  ` | W   | 2 | Sprite 3 pointer low |
| $130   | `spr4pth  ` | W   | 2 | Sprite 4 pointer high |
| $132   | `spr4ptl  ` | W   | 2 | Sprite 4 pointer low |
| $134   | `spr5pth  ` | W   | 2 | Sprite 5 pointer high |
| $136   | `spr5ptl  ` | W   | 2 | Sprite 5 pointer low |
| $138   | `spr6pth  ` | W   | 2 | Sprite 6 pointer high |
| $13A   | `spr6ptl  ` | W   | 2 | Sprite 6 pointer low |
| $13C   | `spr7pth  ` | W   | 2 | Sprite 7 pointer high |
| $13E   | `spr7ptl  ` | W   | 2 | Sprite 7 pointer low |
| $140   | `spr0pos  ` | W   | 2 | Sprite 0 position (V/H) |
| $142   | `spr0ctl  ` | W   | 2 | Sprite 0 control (V-stop + attach) |
| $144   | `spr0data ` | W   | 2 | Sprite 0 image A |
| $146   | `spr0datb ` | W   | 2 | Sprite 0 image B |
| $148   | `spr1pos  ` | W   | 2 | Sprite 1 position |
| $14A   | `spr1ctl  ` | W   | 2 | Sprite 1 control |
| $14C   | `spr1data ` | W   | 2 | Sprite 1 image A |
| $14E   | `spr1datb ` | W   | 2 | Sprite 1 image B |
| $150   | `spr2pos  ` | W   | 2 | Sprite 2 position |
| $152   | `spr2ctl  ` | W   | 2 | Sprite 2 control |
| $154   | `spr2data ` | W   | 2 | Sprite 2 image A |
| $156   | `spr2datb ` | W   | 2 | Sprite 2 image B |
| $158   | `spr3pos  ` | W   | 2 | Sprite 3 position |
| $15A   | `spr3ctl  ` | W   | 2 | Sprite 3 control |
| $15C   | `spr3data ` | W   | 2 | Sprite 3 image A |
| $15E   | `spr3datb ` | W   | 2 | Sprite 3 image B |
| $160   | `spr4pos  ` | W   | 2 | Sprite 4 position |
| $162   | `spr4ctl  ` | W   | 2 | Sprite 4 control |
| $164   | `spr4data ` | W   | 2 | Sprite 4 image A |
| $166   | `spr4datb ` | W   | 2 | Sprite 4 image B |
| $168   | `spr5pos  ` | W   | 2 | Sprite 5 position |
| $16A   | `spr5ctl  ` | W   | 2 | Sprite 5 control |
| $16C   | `spr5data ` | W   | 2 | Sprite 5 image A |
| $16E   | `spr5datb ` | W   | 2 | Sprite 5 image B |
| $170   | `spr6pos  ` | W   | 2 | Sprite 6 position |
| $172   | `spr6ctl  ` | W   | 2 | Sprite 6 control |
| $174   | `spr6data ` | W   | 2 | Sprite 6 image A |
| $176   | `spr6datb ` | W   | 2 | Sprite 6 image B |
| $178   | `spr7pos  ` | W   | 2 | Sprite 7 position |
| $17A   | `spr7ctl  ` | W   | 2 | Sprite 7 control |
| $17C   | `spr7data ` | W   | 2 | Sprite 7 image A |
| $17E   | `spr7datb ` | W   | 2 | Sprite 7 image B |
| $180   | `color00  ` | W   | 2 | Colour register 00 (background) |
| $182   | `color01  ` | W   | 2 | Colour register 01 |
| $184   | `color02  ` | W   | 2 | Colour register 02 |
| $186   | `color03  ` | W   | 2 | Colour register 03 |
| $188   | `color04  ` | W   | 2 | Colour register 04 |
| $18A   | `color05  ` | W   | 2 | Colour register 05 |
| $18C   | `color06  ` | W   | 2 | Colour register 06 |
| $18E   | `color07  ` | W   | 2 | Colour register 07 |
| $190   | `color08  ` | W   | 2 | Colour register 08 |
| $192   | `color09  ` | W   | 2 | Colour register 09 |
| $194   | `color10  ` | W   | 2 | Colour register 10 |
| $196   | `color11  ` | W   | 2 | Colour register 11 |
| $198   | `color12  ` | W   | 2 | Colour register 12 |
| $19A   | `color13  ` | W   | 2 | Colour register 13 |
| $19C   | `color14  ` | W   | 2 | Colour register 14 |
| $19E   | `color15  ` | W   | 2 | Colour register 15 |
| $1A0   | `color16  ` | W   | 2 | Colour register 16 (sprite 0/1) |
| $1A2   | `color17  ` | W   | 2 | Colour register 17 (sprite 0/1) |
| $1A4   | `color18  ` | W   | 2 | Colour register 18 (sprite 0/1) |
| $1A6   | `color19  ` | W   | 2 | Colour register 19 (sprite 0/1) |
| $1A8   | `color20  ` | W   | 2 | Colour register 20 (sprite 2/3) |
| $1AA   | `color21  ` | W   | 2 | Colour register 21 |
| $1AC   | `color22  ` | W   | 2 | Colour register 22 |
| $1AE   | `color23  ` | W   | 2 | Colour register 23 |
| $1B0   | `color24  ` | W   | 2 | Colour register 24 (sprite 4/5) |
| $1B2   | `color25  ` | W   | 2 | Colour register 25 |
| $1B4   | `color26  ` | W   | 2 | Colour register 26 |
| $1B6   | `color27  ` | W   | 2 | Colour register 27 |
| $1B8   | `color28  ` | W   | 2 | Colour register 28 (sprite 6/7) |
| $1BA   | `color29  ` | W   | 2 | Colour register 29 |
| $1BC   | `color30  ` | W   | 2 | Colour register 30 |
| $1BE   | `color31  ` | W   | 2 | Colour register 31 |
| $1C0   | `htotal   ` | W   | 2 | Total horizontal line count (ECS+ VARBEAM) |
| $1C2   | `hsstop   ` | W   | 2 | Horizontal sync stop |
| $1C4   | `hbstrt   ` | W   | 2 | Horizontal blank start |
| $1C6   | `hbstop   ` | W   | 2 | Horizontal blank stop |
| $1C8   | `vtotal   ` | W   | 2 | Total vertical line count |
| $1CA   | `vsstop   ` | W   | 2 | Vertical sync stop |
| $1CC   | `vbstrt   ` | W   | 2 | Vertical blank start |
| $1CE   | `vbstop   ` | W   | 2 | Vertical blank stop |
| $1D0   | `sprhstrt ` | W   | 2 | Sprite horizontal start (UHRES) |
| $1D2   | `sprhstop ` | W   | 2 | Sprite horizontal stop (UHRES) |
| $1D4   | `bplhstrt ` | W   | 2 | Bitplane horizontal start (UHRES) |
| $1D6   | `bplhstop ` | W   | 2 | Bitplane horizontal stop (UHRES) |
| $1D8   | `hhposw   ` | W   | 2 | Dual mode h-beam counter write |
| $1DA   | `hhposr   ` | R   | 2 | Dual mode h-beam counter read |
| $1DC   | `beamcon0 ` | W   | 2 | Beam counter control (PAL, VARBEAM) |
| $1DE   | `hsstrt   ` | W   | 2 | Horizontal sync start (VARHSYNC) |
| $1E0   | `vsstrt   ` | W   | 2 | Vertical sync start (VARVSYNC) |
| $1E2   | `hcenter  ` | W   | 2 | Horizontal position for VSYNC on interlace |
| $1E4   | `diwhigh  ` | W   | 2 | Display window upper bits (ECS+) |
| $1FC   | `fmode    ` | W   | 2 | Fetch mode (AGA) |

# 3. Custom chip bit constants

Cross-reference: `amiga-hardware-reference.md` — DMA, interrupts, disk, audio sections.

## 3.1. DMACON / DMACONR bits

// Source: NDK_3.9/Include/include_h/hardware/dmabits.h
// DMA control register bits. DMAF_SETCLR distinguishes set from clear on write.

```c
#ifndef	HARDWARE_DMABITS_H
#define	HARDWARE_DMABITS_H
/*
**	$VER: dmabits.h 39.1 (18.9.1992)
**	Includes Release 45.1
**
**	include file for defining dma control stuff
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

/* write definitions for dmaconw */
#define DMAF_SETCLR  0x8000
#define DMAF_AUDIO   0x000F   /* 4 bit mask */
#define DMAF_AUD0    0x0001
#define DMAF_AUD1    0x0002
#define DMAF_AUD2    0x0004
#define DMAF_AUD3    0x0008
#define DMAF_DISK    0x0010
#define DMAF_SPRITE  0x0020
#define DMAF_BLITTER 0x0040
#define DMAF_COPPER  0x0080
#define DMAF_RASTER  0x0100
#define DMAF_MASTER  0x0200
#define DMAF_BLITHOG 0x0400
#define DMAF_ALL     0x01FF   /* all dma channels */

/* read definitions for dmaconr */
/* bits 0-8 correspnd to dmaconw definitions */
#define DMAF_BLTDONE 0x4000
#define DMAF_BLTNZERO	0x2000

#define DMAB_SETCLR  15
#define DMAB_AUD0    0
#define DMAB_AUD1    1
#define DMAB_AUD2    2
#define DMAB_AUD3    3
#define DMAB_DISK    4
#define DMAB_SPRITE  5
#define DMAB_BLITTER 6
#define DMAB_COPPER  7
#define DMAB_RASTER  8
#define DMAB_MASTER  9
#define DMAB_BLITHOG 10
#define DMAB_BLTDONE 14
#define DMAB_BLTNZERO	13

#endif	/* HARDWARE_DMABITS_H */
```

## 3.2. INTENA / INTREQ / INTENAR / INTREQR bits

// Source: NDK_3.9/Include/include_h/hardware/intbits.h
// Interrupt enable/request register bits. INTF_SETCLR sets, cleared bits clear.

```c
#ifndef	HARDWARE_INTBITS_H
#define	HARDWARE_INTBITS_H
/*
**	$VER: intbits.h 39.1 (18.9.1992)
**	Includes Release 45.1
**
**	bits in the interrupt enable (and interrupt request) register
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#define  INTB_SETCLR	(15)  /* Set/Clear control bit. Determines if bits */
	    /* written with a 1 get set or cleared. Bits */
	    /* written with a zero are allways unchanged */
#define  INTB_INTEN	(14)  /* Master interrupt (enable only ) */
#define  INTB_EXTER	(13)  /* External interrupt */
#define  INTB_DSKSYNC	(12)  /* Disk re-SYNChronized */
#define  INTB_RBF	(11)  /* serial port Receive Buffer Full */
#define  INTB_AUD3	(10)  /* Audio channel 3 block finished */
#define  INTB_AUD2	(9)   /* Audio channel 2 block finished */
#define  INTB_AUD1	(8)   /* Audio channel 1 block finished */
#define  INTB_AUD0	(7)   /* Audio channel 0 block finished */
#define  INTB_BLIT	(6)   /* Blitter finished */
#define  INTB_VERTB	(5)   /* start of Vertical Blank */
#define  INTB_COPER	(4)   /* Coprocessor */
#define  INTB_PORTS	(3)   /* I/O Ports and timers */
#define  INTB_SOFTINT	(2)   /* software interrupt request */
#define  INTB_DSKBLK	(1)   /* Disk Block done */
#define  INTB_TBE	(0)   /* serial port Transmit Buffer Empty */



#define  INTF_SETCLR	(1L<<15)
#define  INTF_INTEN	(1L<<14)
#define  INTF_EXTER	(1L<<13)
#define  INTF_DSKSYNC	(1L<<12)
#define  INTF_RBF	(1L<<11)
#define  INTF_AUD3	(1L<<10)
#define  INTF_AUD2	(1L<<9)
#define  INTF_AUD1	(1L<<8)
#define  INTF_AUD0	(1L<<7)
#define  INTF_BLIT	(1L<<6)
#define  INTF_VERTB	(1L<<5)
#define  INTF_COPER	(1L<<4)
#define  INTF_PORTS	(1L<<3)
#define  INTF_SOFTINT	(1L<<2)
#define  INTF_DSKBLK	(1L<<1)
#define  INTF_TBE	(1L<<0)

#endif	/* HARDWARE_INTBITS_H */
```

## 3.3. ADKCON / ADKCONR bits

// Source: NDK_3.9/Include/include_h/hardware/adkbits.h
// Audio/disk control register bits. ADKF_SETCLR sets, cleared bits clear.

```c
#ifndef	HARDWARE_ADKBITS_H
#define	HARDWARE_ADKBITS_H
/*
**	$VER: adkbits.h 39.1 (18.9.1992)
**	Includes Release 45.1
**
**	bit definitions for adkcon register
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#define  ADKB_SETCLR	15 /* standard set/clear bit */
#define  ADKB_PRECOMP1	14 /* two bits of precompensation */
#define  ADKB_PRECOMP0	13
#define  ADKB_MFMPREC	12 /* use mfm style precompensation */
#define  ADKB_UARTBRK	11 /* force uart output to zero */
#define  ADKB_WORDSYNC	10 /* enable DSKSYNC register matching */
#define  ADKB_MSBSYNC	9  /* (Apple GCR Only) sync on MSB for reading */
#define  ADKB_FAST	8  /* 1 -> 2 us/bit (mfm), 2 -> 4 us/bit (gcr) */
#define  ADKB_USE3PN	7  /* use aud chan 3 to modulate period of ?? */
#define  ADKB_USE2P3	6  /* use aud chan 2 to modulate period of 3 */
#define  ADKB_USE1P2	5  /* use aud chan 1 to modulate period of 2 */
#define  ADKB_USE0P1	4  /* use aud chan 0 to modulate period of 1 */
#define  ADKB_USE3VN	3  /* use aud chan 3 to modulate volume of ?? */
#define  ADKB_USE2V3	2  /* use aud chan 2 to modulate volume of 3 */
#define  ADKB_USE1V2	1  /* use aud chan 1 to modulate volume of 2 */
#define  ADKB_USE0V1	0  /* use aud chan 0 to modulate volume of 1 */

#define  ADKF_SETCLR	(1L<<15)
#define  ADKF_PRECOMP1	(1L<<14)
#define  ADKF_PRECOMP0	(1L<<13)
#define  ADKF_MFMPREC	(1L<<12)
#define  ADKF_UARTBRK	(1L<<11)
#define  ADKF_WORDSYNC	(1L<<10)
#define  ADKF_MSBSYNC	(1L<<9)
#define  ADKF_FAST	(1L<<8)
#define  ADKF_USE3PN	(1L<<7)
#define  ADKF_USE2P3	(1L<<6)
#define  ADKF_USE1P2	(1L<<5)
#define  ADKF_USE0P1	(1L<<4)
#define  ADKF_USE3VN	(1L<<3)
#define  ADKF_USE2V3	(1L<<2)
#define  ADKF_USE1V2	(1L<<1)
#define  ADKF_USE0V1	(1L<<0)

#define ADKF_PRE000NS	0			/* 000 ns of precomp */
#define ADKF_PRE140NS	(ADKF_PRECOMP0)	/* 140 ns of precomp */
#define ADKF_PRE280NS	(ADKF_PRECOMP1)	/* 280 ns of precomp */
#define ADKF_PRE560NS	(ADKF_PRECOMP0|ADKF_PRECOMP1) /* 560 ns of precomp */

#endif	/* HARDWARE_ADKBITS_H */
```

## 3.4. BLTCON0 / BLTCON1 bits and minterm helpers

// Source: NDK_3.9/Include/include_h/hardware/blit.h
// Blitter control register bits, minterm shortcuts (A_OR_B, A_TO_D, etc.), octant codes for line mode.

```c
#ifndef	HARDWARE_BLIT_H
#define	HARDWARE_BLIT_H
/*
**	$VER: blit.h 39.1 (18.9.1992)
**	Includes Release 45.1
**
**	Defines for direct hardware use of the blitter.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#define HSIZEBITS 6
#define VSIZEBITS 16-HSIZEBITS
#define HSIZEMASK 0x3f	      /* 2^6 -- 1 */
#define VSIZEMASK 0x3FF       /* 2^10 - 1 */

/* all agnii support horizontal blit of at least 1024 bits (128 bytes) wide */
/* some agnii support horizontal blit of up to 32768 bits (4096 bytes) wide */

#ifndef	 NO_BIG_BLITS
#define  MINBYTESPERROW 128
#define  MAXBYTESPERROW 4096
#else
#define  MAXBYTESPERROW 128
#endif

/* definitions for blitter control register 0 */

#define ABC    0x80
#define ABNC   0x40
#define ANBC   0x20
#define ANBNC  0x10
#define NABC   0x8
#define NABNC  0x4
#define NANBC  0x2
#define NANBNC 0x1

/* some commonly used operations */
#define A_OR_B	  ABC|ANBC|NABC | ABNC|ANBNC|NABNC
#define A_OR_C	  ABC|NABC|ABNC | ANBC|NANBC|ANBNC
#define A_XOR_C   NABC|ABNC   | NANBC|ANBNC
#define A_TO_D	  ABC|ANBC|ABNC|ANBNC

#define BC0B_DEST 8
#define BC0B_SRCC 9
#define BC0B_SRCB   10
#define BC0B_SRCA 11
#define BC0F_DEST 0x100
#define BC0F_SRCC 0x200
#define BC0F_SRCB 0x400
#define BC0F_SRCA 0x800

#define BC1F_DESC   2	      /* blitter descend direction */

#define DEST 0x100
#define SRCC 0x200
#define SRCB 0x400
#define SRCA 0x800

#define ASHIFTSHIFT  12       /* bits to right align ashift value */
#define BSHIFTSHIFT  12       /* bits to right align bshift value */

/* definations for blitter control register 1 */
#define LINEMODE     0x1
#define FILL_OR      0x8
#define FILL_XOR     0x10
#define FILL_CARRYIN 0x4
#define ONEDOT	     0x2      /* one dot per horizontal line */
#define OVFLAG	     0x20
#define SIGNFLAG     0x40
#define BLITREVERSE  0x2

#define SUD	     0x10
#define SUL	     0x8
#define AUL	     0x4

#define OCTANT8   24
#define OCTANT7   4
#define OCTANT6   12
#define OCTANT5   28
#define OCTANT4   20
#define OCTANT3   8
#define OCTANT2   0
#define OCTANT1   16

/* stuff for blit qeuer */
struct bltnode
{
    struct  bltnode *n;
    int     (*function)();
    char    stat;
    short   blitsize;
    short   beamsync;
    int     (*cleanup)();
};

/* defined bits for bltstat */
#define CLEANUP 0x40
#define CLEANME CLEANUP

#endif	/* HARDWARE_BLIT_H */
```

# 4. CIA — 8520 complex interface adapter

Cross-reference: `amiga-hardware-reference.md` — "CIA" section.

Two CIAs exist: **ciaa** at `$BFE001` (odd address, low byte of bus) and **ciab** at
`$BFD000` (even address, high byte of bus). Each register is one byte; the `pad[0xff]`
fields in the struct reflect the fact that the CIA chip select uses every 256th
address slot in the `$BFxxxx` region. An emulator implements CIA access by decoding
the upper bus-select bits.

**CIA A** handles: keyboard, parallel port, disk ready/change/protect, mouse buttons,
LED, overlay. **CIA B** handles: serial handshake lines, printer control, disk motor/
select/step, disk index pulse.

// Source: NDK_3.9/Include/include_h/hardware/cia.h
// CIA register layout and all port bit assignments for ciaa/ciab PRA/PRB.

```c
#ifndef	HARDWARE_CIA_H
#define	HARDWARE_CIA_H
/*
**	$VER: cia.h 39.1 (18.9.1992)
**	Includes Release 45.1
**
**	registers and bits in the Complex Interface Adapter (CIA) chip
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/


#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif /* EXEC_TYPES_H */



/*
 * ciaa is on an ODD address (e.g. the low byte) -- $bfe001
 * ciab is on an EVEN address (e.g. the high byte) -- $bfd000
 *
 * do this to get the definitions:
 *    extern struct CIA ciaa, ciab;
 */


struct CIA {
    UBYTE   ciapra;
    UBYTE   pad0[0xff];
    UBYTE   ciaprb;
    UBYTE   pad1[0xff];
    UBYTE   ciaddra;
    UBYTE   pad2[0xff];
    UBYTE   ciaddrb;
    UBYTE   pad3[0xff];
    UBYTE   ciatalo;
    UBYTE   pad4[0xff];
    UBYTE   ciatahi;
    UBYTE   pad5[0xff];
    UBYTE   ciatblo;
    UBYTE   pad6[0xff];
    UBYTE   ciatbhi;
    UBYTE   pad7[0xff];
    UBYTE   ciatodlow;
    UBYTE   pad8[0xff];
    UBYTE   ciatodmid;
    UBYTE   pad9[0xff];
    UBYTE   ciatodhi;
    UBYTE   pad10[0xff];
    UBYTE   unusedreg;
    UBYTE   pad11[0xff];
    UBYTE   ciasdr;
    UBYTE   pad12[0xff];
    UBYTE   ciaicr;
    UBYTE   pad13[0xff];
    UBYTE   ciacra;
    UBYTE   pad14[0xff];
    UBYTE   ciacrb;
};


/* interrupt control register bit numbers */
#define CIAICRB_TA	0
#define CIAICRB_TB	1
#define CIAICRB_ALRM	2
#define CIAICRB_SP	3
#define CIAICRB_FLG	4
#define CIAICRB_IR	7
#define CIAICRB_SETCLR	7

/* control register A bit numbers */
#define CIACRAB_START	0
#define CIACRAB_PBON	1
#define CIACRAB_OUTMODE 2
#define CIACRAB_RUNMODE 3
#define CIACRAB_LOAD	4
#define CIACRAB_INMODE	5
#define CIACRAB_SPMODE	6
#define CIACRAB_TODIN	7

/* control register B bit numbers */
#define CIACRBB_START	0
#define CIACRBB_PBON	1
#define CIACRBB_OUTMODE 2
#define CIACRBB_RUNMODE 3
#define CIACRBB_LOAD	4
#define CIACRBB_INMODE0 5
#define CIACRBB_INMODE1 6
#define CIACRBB_ALARM	7

/* interrupt control register masks */
#define CIAICRF_TA	(1L<<CIAICRB_TA)
#define CIAICRF_TB	(1L<<CIAICRB_TB)
#define CIAICRF_ALRM	(1L<<CIAICRB_ALRM)
#define CIAICRF_SP	(1L<<CIAICRB_SP)
#define CIAICRF_FLG	(1L<<CIAICRB_FLG)
#define CIAICRF_IR	(1L<<CIAICRB_IR)
#define CIAICRF_SETCLR	(1L<<CIAICRB_SETCLR)

/* control register A register masks */
#define CIACRAF_START	(1L<<CIACRAB_START)
#define CIACRAF_PBON	(1L<<CIACRAB_PBON)
#define CIACRAF_OUTMODE (1L<<CIACRAB_OUTMODE)
#define CIACRAF_RUNMODE (1L<<CIACRAB_RUNMODE)
#define CIACRAF_LOAD	(1L<<CIACRAB_LOAD)
#define CIACRAF_INMODE	(1L<<CIACRAB_INMODE)
#define CIACRAF_SPMODE	(1L<<CIACRAB_SPMODE)
#define CIACRAF_TODIN	(1L<<CIACRAB_TODIN)

/* control register B register masks */
#define CIACRBF_START	(1L<<CIACRBB_START)
#define CIACRBF_PBON	(1L<<CIACRBB_PBON)
#define CIACRBF_OUTMODE (1L<<CIACRBB_OUTMODE)
#define CIACRBF_RUNMODE (1L<<CIACRBB_RUNMODE)
#define CIACRBF_LOAD	(1L<<CIACRBB_LOAD)
#define CIACRBF_INMODE0 (1L<<CIACRBB_INMODE0)
#define CIACRBF_INMODE1 (1L<<CIACRBB_INMODE1)
#define CIACRBF_ALARM	(1L<<CIACRBB_ALARM)

/* control register B INMODE masks */
#define CIACRBF_IN_PHI2 0
#define CIACRBF_IN_CNT	(CIACRBF_INMODE0)
#define CIACRBF_IN_TA	(CIACRBF_INMODE1)
#define CIACRBF_IN_CNT_TA  (CIACRBF_INMODE0|CIACRBF_INMODE1)

/*
 * Port definitions -- what each bit in a cia peripheral register is tied to
 */

/* ciaa port A (0xbfe001) */
#define CIAB_GAMEPORT1	(7)   /* gameport 1, pin 6 (fire button*) */
#define CIAB_GAMEPORT0	(6)   /* gameport 0, pin 6 (fire button*) */
#define CIAB_DSKRDY	(5)   /* disk ready* */
#define CIAB_DSKTRACK0	(4)   /* disk on track 00* */
#define CIAB_DSKPROT	(3)   /* disk write protect* */
#define CIAB_DSKCHANGE	(2)   /* disk change* */
#define CIAB_LED	(1)   /* led light control (0==>bright) */
#define CIAB_OVERLAY	(0)   /* memory overlay bit */

/* ciaa port B (0xbfe101) -- parallel port */

/* ciab port A (0xbfd000) -- serial and printer control */
#define CIAB_COMDTR	(7)   /* serial Data Terminal Ready* */
#define CIAB_COMRTS	(6)   /* serial Request to Send* */
#define CIAB_COMCD	(5)   /* serial Carrier Detect* */
#define CIAB_COMCTS	(4)   /* serial Clear to Send* */
#define CIAB_COMDSR	(3)   /* serial Data Set Ready* */
#define CIAB_PRTRSEL	(2)   /* printer SELECT */
#define CIAB_PRTRPOUT	(1)   /* printer paper out */
#define CIAB_PRTRBUSY	(0)   /* printer busy */

/* ciab port B (0xbfd100) -- disk control */
#define CIAB_DSKMOTOR	(7)   /* disk motorr* */
#define CIAB_DSKSEL3	(6)   /* disk select unit 3* */
#define CIAB_DSKSEL2	(5)   /* disk select unit 2* */
#define CIAB_DSKSEL1	(4)   /* disk select unit 1* */
#define CIAB_DSKSEL0	(3)   /* disk select unit 0* */
#define CIAB_DSKSIDE	(2)   /* disk side select* */
#define CIAB_DSKDIREC	(1)   /* disk direction of seek* */
#define CIAB_DSKSTEP	(0)   /* disk step heads* */

/* ciaa port A (0xbfe001) */
#define CIAF_GAMEPORT1	(1L<<7)
#define CIAF_GAMEPORT0	(1L<<6)
#define CIAF_DSKRDY	(1L<<5)
#define CIAF_DSKTRACK0	(1L<<4)
#define CIAF_DSKPROT	(1L<<3)
#define CIAF_DSKCHANGE	(1L<<2)
#define CIAF_LED	(1L<<1)
#define CIAF_OVERLAY	(1L<<0)

/* ciaa port B (0xbfe101) -- parallel port */

/* ciab port A (0xbfd000) -- serial and printer control */
#define CIAF_COMDTR	(1L<<7)
#define CIAF_COMRTS	(1L<<6)
#define CIAF_COMCD	(1L<<5)
#define CIAF_COMCTS	(1L<<4)
#define CIAF_COMDSR	(1L<<3)
#define CIAF_PRTRSEL	(1L<<2)
#define CIAF_PRTRPOUT	(1L<<1)
#define CIAF_PRTRBUSY	(1L<<0)

/* ciab port B (0xbfd100) -- disk control */
#define CIAF_DSKMOTOR	(1L<<7)
#define CIAF_DSKSEL3	(1L<<6)
#define CIAF_DSKSEL2	(1L<<5)
#define CIAF_DSKSEL1	(1L<<4)
#define CIAF_DSKSEL0	(1L<<3)
#define CIAF_DSKSIDE	(1L<<2)
#define CIAF_DSKDIREC	(1L<<1)
#define CIAF_DSKSTEP	(1L<<0)

#endif	/* HARDWARE_CIA_H */
```

# 5. Exec core structs

Cross-reference: `amiga-exec-kernel.md` for semantic explanations.

## 5.1. exec/types.h — base typedefs (UBYTE, UWORD, ULONG, BPTR, BSTR, APTR, STRPTR)

// Source: NDK_3.9/Include/include_h/exec/types.h
// Base types. Every other header depends on these. BPTR/BSTR are BCPL pointers (byte address >> 2).

```c
#ifndef	EXEC_TYPES_H
#define	EXEC_TYPES_H
/*
**	$Id: types.h,v 45.2 2001/03/12 17:51:53 heinz Exp $
**
**	Data typing.  Must be included before any other Amiga include.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/


#define INCLUDE_VERSION	45 /* Version of the include files in use. (Do not
			      use this label for OpenLibrary() calls!) */


#define GLOBAL  extern      /* the declaratory use of an external */
#define IMPORT  extern      /* reference to an external */
#define STATIC  static      /* a local static variable */
#define REGISTER register   /* a (hopefully) register variable */


#ifndef VOID
#define VOID            void
#endif

/* General const support */
#ifndef CONST
#if __STDC__
#define CONST           const
#else
#define CONST
#endif
#endif

#ifndef VOLATILE
#if __STDC__
#define VOLATILE        volatile
#else
#define VOLATILE
#endif
#endif

  /*  WARNING: APTR was redefined for the V36 Includes!  APTR is a   */
 /*  32-Bit Absolute Memory Pointer.  C pointer math will not       */
/*  operate on APTR --  use "ULONG *" instead.                     */
#ifndef APTR_TYPEDEF
#define APTR_TYPEDEF
typedef void	       *APTR;	    /* 32-bit untyped pointer */
#endif
typedef long            LONG;       /* signed 32-bit quantity */
typedef unsigned long   ULONG;      /* unsigned 32-bit quantity */
typedef unsigned long   LONGBITS;   /* 32 bits manipulated individually */
typedef short           WORD;       /* signed 16-bit quantity */
typedef unsigned short  UWORD;      /* unsigned 16-bit quantity */
typedef unsigned short  WORDBITS;   /* 16 bits manipulated individually */
#if __STDC__
typedef signed char	BYTE;	    /* signed 8-bit quantity */
#else
typedef char		BYTE;	    /* signed 8-bit quantity */
#endif
typedef unsigned char   UBYTE;      /* unsigned 8-bit quantity */
typedef unsigned char   BYTEBITS;   /* 8 bits manipulated individually */
typedef unsigned short	RPTR;	    /* signed relative pointer */

#ifdef __cplusplus
typedef char           *STRPTR;     /* string pointer (NULL terminated) */
#else
typedef unsigned char  *STRPTR;     /* string pointer (NULL terminated) */
#endif

/* const support for pointer types */
typedef CONST void     *CONST_APTR;     /* 32-bit untyped const pointer */
#ifdef __cplusplus
typedef CONST char           *CONST_STRPTR; /* STRPTR to const data */
#else
typedef CONST unsigned char  *CONST_STRPTR; /* STRPTR to const data */
#endif

/* For compatibility only: (don't use in new code) */
typedef short           SHORT;      /* signed 16-bit quantity (use WORD) */
typedef unsigned short  USHORT;     /* unsigned 16-bit quantity (use UWORD) */
typedef short           COUNT;
typedef unsigned short  UCOUNT;
typedef ULONG		CPTR;


/* Types with specific semantics */
typedef float           FLOAT;
typedef double          DOUBLE;
typedef short           BOOL;
typedef unsigned char   TEXT;

#ifndef TRUE
#define TRUE            1
#endif
#ifndef FALSE
#define FALSE           0
#endif
#ifndef NULL
#define NULL            0L
#endif


#define BYTEMASK        0xFF


 /* #define LIBRARY_VERSION is now obsolete.  Please use LIBRARY_MINIMUM */
/* or code the specific minimum library version you require.		*/
#define LIBRARY_MINIMUM	40 /* Lowest version supported by Amiga, Inc. */

/* Some structure definitions include prototypes for function pointers.
 * This may not work with `C' compilers that do not comply to the ANSI
 * standard, which we will have to work around. 
 */
#if __STDC__
#define __CLIB_PROTOTYPE(a) a
#else
#define __CLIB_PROTOTYPE(a)
#endif /* __STDC__ */

#endif	/* EXEC_TYPES_H */
```

## 5.2. exec/nodes.h — Node / MinNode and NT_* type codes

// Source: NDK_3.9/Include/include_h/exec/nodes.h
// Doubly-linked list node. ln_Type selects NT_TASK, NT_LIBRARY, NT_MSGPORT, etc.

```c
#ifndef	EXEC_NODES_H
#define	EXEC_NODES_H
/*
**	$VER: nodes.h 39.0 (15.10.1991)
**	Includes Release 45.1
**
**	Nodes & Node type identifiers.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif /* EXEC_TYPES_H */


/*
 *  List Node Structure.  Each member in a list starts with a Node
 */

struct Node {
    struct  Node *ln_Succ;	/* Pointer to next (successor) */
    struct  Node *ln_Pred;	/* Pointer to previous (predecessor) */
    UBYTE   ln_Type;
    BYTE    ln_Pri;		/* Priority, for sorting */
    char    *ln_Name;		/* ID string, null terminated */
};	/* Note: word aligned */

/* minimal node -- no type checking possible */
struct MinNode {
    struct MinNode *mln_Succ;
    struct MinNode *mln_Pred;
};


/*
** Note: Newly initialized IORequests, and software interrupt structures
** used with Cause(), should have type NT_UNKNOWN.  The OS will assign a type
** when they are first used.
*/
/*----- Node Types for LN_TYPE -----*/
#define NT_UNKNOWN	0
#define NT_TASK		1	/* Exec task */
#define NT_INTERRUPT	2
#define NT_DEVICE	3
#define NT_MSGPORT	4
#define NT_MESSAGE	5	/* Indicates message currently pending */
#define NT_FREEMSG	6
#define NT_REPLYMSG	7	/* Message has been replied */
#define NT_RESOURCE	8
#define NT_LIBRARY	9
#define NT_MEMORY	10
#define NT_SOFTINT	11	/* Internal flag used by SoftInits */
#define NT_FONT		12
#define NT_PROCESS	13	/* AmigaDOS Process */
#define NT_SEMAPHORE	14
#define NT_SIGNALSEM	15	/* signal semaphores */
#define NT_BOOTNODE	16
#define NT_KICKMEM	17
#define NT_GRAPHICS	18
#define NT_DEATHMESSAGE	19

#define NT_USER		254	/* User node types work down from here */
#define NT_EXTENDED	255

#endif	/* EXEC_NODES_H */
```

## 5.3. exec/lists.h — List / MinList headers

// Source: NDK_3.9/Include/include_h/exec/lists.h
// List header — contains head/tail/tailpred. Empty list: lh_TailPred == lh_Head.

```c
#ifndef EXEC_LISTS_H
#define EXEC_LISTS_H
/*
**	$VER: lists.h 39.0 (15.10.1991)
**	Includes Release 45.1
**
**	Definitions and macros for use with Exec lists
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif /* EXEC_NODES_H */

/*
 *  Full featured list header.
 */
struct List {
   struct  Node *lh_Head;
   struct  Node *lh_Tail;
   struct  Node *lh_TailPred;
   UBYTE   lh_Type;
   UBYTE   l_pad;
};	/* word aligned */

/*
 * Minimal List Header - no type checking
 */
struct MinList {
   struct  MinNode *mlh_Head;
   struct  MinNode *mlh_Tail;
   struct  MinNode *mlh_TailPred;
};	/* longword aligned */


/*
 *	Check for the presence of any nodes on the given list.	These
 *	macros are even safe to use on lists that are modified by other
 *	tasks.	However; if something is simultaneously changing the
 *	list, the result of the test is unpredictable.
 *
 *	Unless you first arbitrated for ownership of the list, you can't
 *	_depend_ on the contents of the list.  Nodes might have been added
 *	or removed during or after the macro executes.
 *
 *		if( IsListEmpty(list) )		printf("List is empty\n");
 */
#define IsListEmpty(x) \
	( ((x)->lh_TailPred) == (struct Node *)(x) )

#define IsMsgPortEmpty(x) \
	( ((x)->mp_MsgList.lh_TailPred) == (struct Node *)(&(x)->mp_MsgList) )


#endif	/* EXEC_LISTS_H */
```

## 5.4. exec/ports.h — MsgPort and Message

// Source: NDK_3.9/Include/include_h/exec/ports.h
// MsgPort delivers messages between tasks. PA_SIGNAL/PA_SOFTINT/PA_IGNORE determine arrival action.

```c
#ifndef	EXEC_PORTS_H
#define	EXEC_PORTS_H
/*
**	$VER: ports.h 39.0 (15.10.1991)
**	Includes Release 45.1
**
**	Message ports and Messages.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif /* EXEC_NODES_H */

#ifndef EXEC_LISTS_H
#include <exec/lists.h>
#endif /* EXEC_LISTS_H */

#ifndef EXEC_TASKS_H
#include <exec/tasks.h>
#endif /* EXEC_TASKS_H */


/****** MsgPort *****************************************************/

struct MsgPort {
    struct  Node mp_Node;
    UBYTE   mp_Flags;
    UBYTE   mp_SigBit;		/* signal bit number	*/
    void   *mp_SigTask;		/* object to be signalled */
    struct  List mp_MsgList;	/* message linked list	*/
};

#define mp_SoftInt mp_SigTask	/* Alias */

/* mp_Flags: Port arrival actions (PutMsg) */
#define PF_ACTION	3	/* Mask */
#define PA_SIGNAL	0	/* Signal task in mp_SigTask */
#define PA_SOFTINT	1	/* Signal SoftInt in mp_SoftInt/mp_SigTask */
#define PA_IGNORE	2	/* Ignore arrival */


/****** Message *****************************************************/

struct Message {
    struct  Node mn_Node;
    struct  MsgPort *mn_ReplyPort;  /* message reply port */
    UWORD   mn_Length;		    /* total message length, in bytes */
				    /* (include the size of the Message */
				    /* structure in the length) */
};

#endif	/* EXEC_PORTS_H */
```

## 5.5. exec/tasks.h — Task control block, signals, task flags/state

// Source: NDK_3.9/Include/include_h/exec/tasks.h
// Task (and Process which embeds it) TCB. tc_Flags/tc_State bits, signal bit numbers, StackSwap struct.

```c
#ifndef	EXEC_TASKS_H
#define	EXEC_TASKS_H
/*
**	$VER: tasks.h 39.3 (18.9.1992)
**	Includes Release 45.1
**
**	Task Control Block, Signals, and Task flags.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif /* EXEC_NODES_H */

#ifndef EXEC_LISTS_H
#include <exec/lists.h>
#endif /* EXEC_LISTS_H */


/* Please use Exec functions to modify task structure fields, where available.
 */
struct Task {
    struct  Node tc_Node;
    UBYTE   tc_Flags;
    UBYTE   tc_State;
    BYTE    tc_IDNestCnt;	    /* intr disabled nesting*/
    BYTE    tc_TDNestCnt;	    /* task disabled nesting*/
    ULONG   tc_SigAlloc;	    /* sigs allocated */
    ULONG   tc_SigWait;	    /* sigs we are waiting for */
    ULONG   tc_SigRecvd;	    /* sigs we have received */
    ULONG   tc_SigExcept;	    /* sigs we will take excepts for */
    UWORD   tc_TrapAlloc;	    /* traps allocated */
    UWORD   tc_TrapAble;	    /* traps enabled */
    APTR    tc_ExceptData;	    /* points to except data */
    APTR    tc_ExceptCode;	    /* points to except code */
    APTR    tc_TrapData;	    /* points to trap data */
    APTR    tc_TrapCode;	    /* points to trap code */
    APTR    tc_SPReg;		    /* stack pointer	    */
    APTR    tc_SPLower;	    /* stack lower bound    */
    APTR    tc_SPUpper;	    /* stack upper bound + 2*/
    VOID    (*tc_Switch)();	    /* task losing CPU	  */
    VOID    (*tc_Launch)();	    /* task getting CPU  */
    struct  List tc_MemEntry;	    /* Allocated memory. Freed by RemTask() */
    APTR    tc_UserData;	    /* For use by the task; no restrictions! */
};

/*
 * Stack swap structure as passed to StackSwap()
 */
struct	StackSwapStruct {
	APTR	stk_Lower;	/* Lowest byte of stack */
	ULONG	stk_Upper;	/* Upper end of stack (size + Lowest) */
	APTR	stk_Pointer;	/* Stack pointer at switch point */
};

/*----- Flag Bits ------------------------------------------*/
#define TB_PROCTIME	0
#define TB_ETASK	3
#define TB_STACKCHK	4
#define TB_EXCEPT	5
#define TB_SWITCH	6
#define TB_LAUNCH	7

#define TF_PROCTIME	(1L<<0)
#define TF_ETASK	(1L<<3)
#define TF_STACKCHK	(1L<<4)
#define TF_EXCEPT	(1L<<5)
#define TF_SWITCH	(1L<<6)
#define TF_LAUNCH	(1L<<7)

/*----- Task States ----------------------------------------*/
#define TS_INVALID	0
#define TS_ADDED	1
#define TS_RUN		2
#define TS_READY	3
#define TS_WAIT	4
#define TS_EXCEPT	5
#define TS_REMOVED	6

/*----- Predefined Signals -------------------------------------*/
#define SIGB_ABORT	0
#define SIGB_CHILD	1
#define SIGB_BLIT	4	/* Note: same as SINGLE */
#define SIGB_SINGLE	4	/* Note: same as BLIT */
#define SIGB_INTUITION	5
#define	SIGB_NET	7
#define SIGB_DOS	8

#define SIGF_ABORT	(1L<<0)
#define SIGF_CHILD	(1L<<1)
#define SIGF_BLIT	(1L<<4)
#define SIGF_SINGLE	(1L<<4)
#define SIGF_INTUITION	(1L<<5)
#define	SIGF_NET	(1L<<7)
#define SIGF_DOS	(1L<<8)

#endif	/* EXEC_TASKS_H */
```

## 5.6. exec/libraries.h — Library (LibNode) layout and LVO constants

// Source: NDK_3.9/Include/include_h/exec/libraries.h
// Library base. LVO jump table sits at negative offsets before the struct. LIB_VECTSIZE is 6 bytes (JMP + 32-bit address).

```c
#ifndef	EXEC_LIBRARIES_H
#define	EXEC_LIBRARIES_H
/*
**	$VER: libraries.h 39.2 (10.4.1992)
**	Includes Release 45.1
**
**	Definitions for use when creating or using Exec libraries
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif /* EXEC_NODES_H */


/*------ Special Constants ---------------------------------------*/
#define LIB_VECTSIZE	6	/* Each library entry takes 6 bytes */
#define LIB_RESERVED	4	/* Exec reserves the first 4 vectors */
#define LIB_BASE	(-LIB_VECTSIZE)
#define LIB_USERDEF	(LIB_BASE-(LIB_RESERVED*LIB_VECTSIZE))
#define LIB_NONSTD	(LIB_USERDEF)

/*------ Standard Functions --------------------------------------*/
#define LIB_OPEN	(-6)
#define LIB_CLOSE	(-12)
#define LIB_EXPUNGE	(-18)
#define LIB_EXTFUNC	(-24)	/* for future expansion */


/*------ Library Base Structure ----------------------------------*/
/* Also used for Devices and some Resources */
struct Library {
    struct  Node lib_Node;
    UBYTE   lib_Flags;
    UBYTE   lib_pad;
    UWORD   lib_NegSize;	    /* number of bytes before library */
    UWORD   lib_PosSize;	    /* number of bytes after library */
    UWORD   lib_Version;	    /* major */
    UWORD   lib_Revision;	    /* minor */
    APTR    lib_IdString;	    /* ASCII identification */
    ULONG   lib_Sum;		    /* the checksum itself */
    UWORD   lib_OpenCnt;	    /* number of current opens */
};	/* Warning: size is not a longword multiple! */

/* lib_Flags bit definitions (all others are system reserved) */
#define LIBF_SUMMING	(1<<0)	    /* we are currently checksumming */
#define LIBF_CHANGED	(1<<1)	    /* we have just changed the lib */
#define LIBF_SUMUSED	(1<<2)	    /* set if we should bother to sum */
#define LIBF_DELEXP	(1<<3)	    /* delayed expunge */


/* Temporary Compatibility */
#define lh_Node	lib_Node
#define lh_Flags	lib_Flags
#define lh_pad		lib_pad
#define lh_NegSize	lib_NegSize
#define lh_PosSize	lib_PosSize
#define lh_Version	lib_Version
#define lh_Revision	lib_Revision
#define lh_IdString	lib_IdString
#define lh_Sum		lib_Sum
#define lh_OpenCnt	lib_OpenCnt

#endif	/* EXEC_LIBRARIES_H */
```

## 5.7. exec/memory.h — MemHeader, MemChunk, MEMF_* flags

// Source: NDK_3.9/Include/include_h/exec/memory.h
// Memory region headers and AllocMem flags — MEMF_CHIP, MEMF_FAST, MEMF_CLEAR, etc.

```c
#ifndef	EXEC_MEMORY_H
#define	EXEC_MEMORY_H
/*
**	$VER: memory.h 39.3 (21.5.1992)
**	Includes Release 45.1
**
**	Definitions and structures used by the memory allocation system
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif /* EXEC_NODES_H */


/****** MemChunk ****************************************************/

struct	MemChunk {
    struct  MemChunk *mc_Next;	/* pointer to next chunk */
    ULONG   mc_Bytes;		/* chunk byte size	*/
};


/****** MemHeader ***************************************************/

struct	MemHeader {
    struct  Node mh_Node;
    UWORD   mh_Attributes;	/* characteristics of this region */
    struct  MemChunk *mh_First; /* first free region		*/
    APTR    mh_Lower;		/* lower memory bound		*/
    APTR    mh_Upper;		/* upper memory bound+1	*/
    ULONG   mh_Free;		/* total number of free bytes	*/
};


/****** MemEntry ****************************************************/

struct	MemEntry {
union {
    ULONG   meu_Reqs;		/* the AllocMem requirements */
    APTR    meu_Addr;		/* the address of this memory region */
    } me_Un;
    ULONG   me_Length;		/* the length of this memory region */
};

#define me_un	    me_Un	/* compatibility - do not use*/
#define me_Reqs     me_Un.meu_Reqs
#define me_Addr     me_Un.meu_Addr


/****** MemList *****************************************************/

/* Note: sizeof(struct MemList) includes the size of the first MemEntry! */
struct	MemList {
    struct  Node ml_Node;
    UWORD   ml_NumEntries;	/* number of entries in this struct */
    struct  MemEntry ml_ME[1];	/* the first entry	*/
};

#define ml_me	ml_ME		/* compatability - do not use */


/*----- Memory Requirement Types ---------------------------*/
/*----- See the AllocMem() documentation for details--------*/

#define MEMF_ANY    (0L)	/* Any type of memory will do */
#define MEMF_PUBLIC (1L<<0)
#define MEMF_CHIP   (1L<<1)
#define MEMF_FAST   (1L<<2)
#define MEMF_LOCAL  (1L<<8)	/* Memory that does not go away at RESET */
#define MEMF_24BITDMA (1L<<9)	/* DMAable memory within 24 bits of address */
#define	MEMF_KICK   (1L<<10)	/* Memory that can be used for KickTags */

#define MEMF_CLEAR   (1L<<16)	/* AllocMem: NULL out area before return */
#define MEMF_LARGEST (1L<<17)	/* AvailMem: return the largest chunk size */
#define MEMF_REVERSE (1L<<18)	/* AllocMem: allocate from the top down */
#define MEMF_TOTAL   (1L<<19)	/* AvailMem: return total size of memory */

#define	MEMF_NO_EXPUNGE	(1L<<31) /*AllocMem: Do not cause expunge on failure */

/*----- Current alignment rules for memory blocks (may increase) -----*/
#define MEM_BLOCKSIZE	8L
#define MEM_BLOCKMASK	(MEM_BLOCKSIZE-1)


/****** MemHandlerData **********************************************/
/* Note:  This structure is *READ ONLY* and only EXEC can create it!*/
struct MemHandlerData
{
	ULONG	memh_RequestSize;	/* Requested allocation size */
	ULONG	memh_RequestFlags;	/* Requested allocation flags */
	ULONG	memh_Flags;		/* Flags (see below) */
};

#define	MEMHF_RECYCLE	(1L<<0)	/* 0==First time, 1==recycle */

/****** Low Memory handler return values ***************************/
#define	MEM_DID_NOTHING	(0)	/* Nothing we could do... */
#define	MEM_ALL_DONE	(-1)	/* We did all we could do */
#define	MEM_TRY_AGAIN	(1)	/* We did some, try the allocation again */

#endif	/* EXEC_MEMORY_H */
```

## 5.8. exec/interrupts.h — Interrupt, IntVector, SoftIntList

// Source: NDK_3.9/Include/include_h/exec/interrupts.h
// Interrupt server node. is_Code is called with is_Data in A1.

```c
#ifndef	EXEC_INTERRUPTS_H
#define	EXEC_INTERRUPTS_H
/*
**	$VER: interrupts.h 39.1 (18.9.1992)
**	Includes Release 45.1
**
**	Callback structures used by hardware & software interrupts
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif /* EXEC_NODES_H */

#ifndef EXEC_LISTS_H
#include <exec/lists.h>
#endif /* EXEC_LISTS_H */


struct Interrupt {
    struct  Node is_Node;
    APTR    is_Data;		    /* server data segment  */
    VOID    (*is_Code)();	    /* server code entry    */
};


struct IntVector {		/* For EXEC use ONLY! */
    APTR    iv_Data;
    VOID    (*iv_Code)();
    struct  Node *iv_Node;
};


struct SoftIntList {		/* For EXEC use ONLY! */
    struct List sh_List;
    UWORD  sh_Pad;
};

#define SIH_PRIMASK (0xf0)

/* this is a fake INT definition, used only for AddIntServer and the like */
#define INTB_NMI	15
#define INTF_NMI	(1L<<15)

#endif	/* EXEC_INTERRUPTS_H */
```

## 5.9. exec/io.h — IORequest, IOStdReq, CMD_* codes, IOF_QUICK

// Source: NDK_3.9/Include/include_h/exec/io.h
// IORequest is the message sent to a device. CMD_READ/WRITE/RESET/STOP/START/FLUSH are standard.

```c
#ifndef	EXEC_IO_H
#define	EXEC_IO_H
/*
**	$VER: io.h 39.0 (15.10.1991)
**	Includes Release 45.1
**
**	Message structures used for device communication
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_PORTS_H
#include <exec/ports.h>
#endif /* EXEC_PORTS_H */


struct IORequest {
    struct  Message io_Message;
    struct  Device  *io_Device;     /* device node pointer  */
    struct  Unit    *io_Unit;	    /* unit (driver private)*/
    UWORD   io_Command;	    /* device command */
    UBYTE   io_Flags;
    BYTE    io_Error;		    /* error or warning num */
};

struct IOStdReq {
    struct  Message io_Message;
    struct  Device  *io_Device;     /* device node pointer  */
    struct  Unit    *io_Unit;	    /* unit (driver private)*/
    UWORD   io_Command;	    /* device command */
    UBYTE   io_Flags;
    BYTE    io_Error;		    /* error or warning num */
    ULONG   io_Actual;		    /* actual number of bytes transferred */
    ULONG   io_Length;		    /* requested number bytes transferred*/
    APTR    io_Data;		    /* points to data area */
    ULONG   io_Offset;		    /* offset for block structured devices */
};

/* library vector offsets for device reserved vectors */
#define DEV_BEGINIO	(-30)
#define DEV_ABORTIO	(-36)

/* io_Flags defined bits */
#define IOB_QUICK	0
#define IOF_QUICK	(1<<0)


#define CMD_INVALID	0
#define CMD_RESET	1
#define CMD_READ	2
#define CMD_WRITE	3
#define CMD_UPDATE	4
#define CMD_CLEAR	5
#define CMD_STOP	6
#define CMD_START	7
#define CMD_FLUSH	8

#define CMD_NONSTD	9

#endif	/* EXEC_IO_H */
```

## 5.10. exec/devices.h — Device, Unit, UNITF_ flags

// Source: NDK_3.9/Include/include_h/exec/devices.h
// Device is just a Library; Unit adds a message port for per-unit queueing.

```c
#ifndef	EXEC_DEVICES_H
#define	EXEC_DEVICES_H
/*
**	$VER: devices.h 39.0 (15.10.1991)
**	Includes Release 45.1
**
**	Include file for use by Exec device drivers
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_LIBRARIES_H
#include <exec/libraries.h>
#endif /* EXEC_LIBRARIES_H */

#ifndef EXEC_PORTS_H
#include <exec/ports.h>
#endif /* EXEC_PORTS_H */


/****** Device ******************************************************/

struct Device {
    struct  Library dd_Library;
};


/****** Unit ********************************************************/

struct Unit {
    struct  MsgPort unit_MsgPort;	/* queue for unprocessed messages */
					/* instance of msgport is recommended */
    UBYTE   unit_flags;
    UBYTE   unit_pad;
    UWORD   unit_OpenCnt;		/* number of active opens */
};


#define UNITF_ACTIVE	(1<<0)
#define UNITF_INTASK	(1<<1)

#endif	/* EXEC_DEVICES_H */
```

## 5.11. exec/semaphores.h — SignalSemaphore, SemaphoreRequest, SemaphoreMessage

// Source: NDK_3.9/Include/include_h/exec/semaphores.h
// Reader-writer semaphores for shared library state. SM_SHARED / SM_EXCLUSIVE.

```c
#ifndef	EXEC_SEMAPHORES_H
#define	EXEC_SEMAPHORES_H
/*
**	$VER: semaphores.h 39.1 (7.2.1992)
**	Includes Release 45.1
**
**	Definitions for locking functions.
**
**	(C) Copyright 1986-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif /* EXEC_NODES_H */

#ifndef EXEC_LISTS_H
#include <exec/lists.h>
#endif /* EXEC_LISTS_H */

#ifndef EXEC_PORTS_H
#include <exec/ports.h>
#endif /* EXEC_PORTS_H */

#ifndef EXEC_TASKS_H
#include <exec/tasks.h>
#endif /* EXEC_TASKS_H */


/****** SignalSemaphore *********************************************/

/* Private structure used by ObtainSemaphore() */
struct SemaphoreRequest
{
	struct MinNode	sr_Link;
	struct Task	*sr_Waiter;
};

/* Signal Semaphore data structure */
struct SignalSemaphore
{
	struct Node		ss_Link;
	WORD			ss_NestCount;
	struct MinList		ss_WaitQueue;
	struct SemaphoreRequest	ss_MultipleLink;
	struct Task		*ss_Owner;
	WORD			ss_QueueCount;
};

/****** Semaphore procure message (for use in V39 Procure/Vacate) ****/
struct SemaphoreMessage
{
	struct Message		ssm_Message;
	struct SignalSemaphore	*ssm_Semaphore;
};

#define	SM_SHARED	(1L)
#define	SM_EXCLUSIVE	(0L)

/****** Semaphore (Old Procure/Vacate type, not reliable) ***********/

struct Semaphore	/* Do not use these semaphores! */
{
	struct MsgPort	sm_MsgPort;
	WORD		sm_Bids;
};

#define sm_LockMsg mp_SigTask

#endif	/* EXEC_SEMAPHORES_H */
```

## 5.12. exec/execbase.h — ExecBase at $4 (authoritative)

// Source: NDK_3.9/Include/include_h/exec/execbase.h
// The ExecBase struct, pointed to by location $4. AttnFlags holds CPU/FPU detection bits.

```c
#ifndef EXEC_EXECBASE_H
#define EXEC_EXECBASE_H
/*
**	$VER: execbase.h 39.6 (18.1.1993)
**	Includes Release 45.1
**
**	Definition of the exec.library base structure.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_LISTS_H
#include <exec/lists.h>
#endif /* EXEC_LISTS_H */

#ifndef EXEC_INTERRUPTS_H
#include <exec/interrupts.h>
#endif /* EXEC_INTERRUPTS_H */

#ifndef EXEC_LIBRARIES_H
#include <exec/libraries.h>
#endif /* EXEC_LIBRARIES_H */

#ifndef EXEC_TASKS_H
#include <exec/tasks.h>
#endif /* EXEC_TASKS_H */


/* Definition of the Exec library base structure (pointed to by location 4).
** Most fields are not to be viewed or modified by user programs.  Use
** extreme caution.
*/
struct ExecBase {
	struct Library LibNode; /* Standard library node */

/******** Static System Variables ********/

	UWORD	SoftVer;	/* kickstart release number (obs.) */
	WORD	LowMemChkSum;	/* checksum of 68000 trap vectors */
	ULONG	ChkBase;	/* system base pointer complement */
	APTR	ColdCapture;	/* coldstart soft capture vector */
	APTR	CoolCapture;	/* coolstart soft capture vector */
	APTR	WarmCapture;	/* warmstart soft capture vector */
	APTR	SysStkUpper;	/* system stack base   (upper bound) */
	APTR	SysStkLower;	/* top of system stack (lower bound) */
	ULONG	MaxLocMem;	/* top of chip memory */
	APTR	DebugEntry;	/* global debugger entry point */
	APTR	DebugData;	/* global debugger data segment */
	APTR	AlertData;	/* alert data segment */
	APTR	MaxExtMem;	/* top of extended mem, or null if none */

	UWORD	ChkSum;	/* for all of the above (minus 2) */

/****** Interrupt Related ***************************************/

	struct	IntVector IntVects[16];

/****** Dynamic System Variables *************************************/

	struct	Task *ThisTask; /* pointer to current task (readable) */

	ULONG	IdleCount;	/* idle counter */
	ULONG	DispCount;	/* dispatch counter */
	UWORD	Quantum;	/* time slice quantum */
	UWORD	Elapsed;	/* current quantum ticks */
	UWORD	SysFlags;	/* misc internal system flags */
	BYTE	IDNestCnt;	/* interrupt disable nesting count */
	BYTE	TDNestCnt;	/* task disable nesting count */

	UWORD	AttnFlags;	/* special attention flags (readable) */

	UWORD	AttnResched;	/* rescheduling attention */
	APTR	ResModules;	/* resident module array pointer */
	APTR	TaskTrapCode;
	APTR	TaskExceptCode;
	APTR	TaskExitCode;
	ULONG	TaskSigAlloc;
	UWORD	TaskTrapAlloc;


/****** System Lists (private!) ********************************/

	struct	List MemList;
	struct	List ResourceList;
	struct	List DeviceList;
	struct	List IntrList;
	struct	List LibList;
	struct	List PortList;
	struct	List TaskReady;
	struct	List TaskWait;

	struct	SoftIntList SoftInts[5];

/****** Other Globals *******************************************/

	LONG	LastAlert[4];

	/* these next two variables are provided to allow
	** system developers to have a rough idea of the
	** period of two externally controlled signals --
	** the time between vertical blank interrupts and the
	** external line rate (which is counted by CIA A's
	** "time of day" clock).  In general these values
	** will be 50 or 60, and may or may not track each
	** other.  These values replace the obsolete AFB_PAL
	** and AFB_50HZ flags.
	*/
	UBYTE	VBlankFrequency;	/* (readable) */
	UBYTE	PowerSupplyFrequency;	/* (readable) */

	struct	List SemaphoreList;

	/* these next two are to be able to kickstart into user ram.
	** KickMemPtr holds a singly linked list of MemLists which
	** will be removed from the memory list via AllocAbs.  If
	** all the AllocAbs's succeeded, then the KickTagPtr will
	** be added to the rom tag list.
	*/
	APTR	KickMemPtr;	/* ptr to queue of mem lists */
	APTR	KickTagPtr;	/* ptr to rom tag queue */
	APTR	KickCheckSum;	/* checksum for mem and tags */

/****** V36 Exec additions start here **************************************/

	UWORD	ex_Pad0;		/* Private internal use */
	ULONG	ex_LaunchPoint;		/* Private to Launch/Switch */
	APTR	ex_RamLibPrivate;
	/* The next ULONG contains the system "E" clock frequency,
	** expressed in Hertz.	The E clock is used as a timebase for
	** the Amiga's 8520 I/O chips. (E is connected to "02").
	** Typical values are 715909 for NTSC, or 709379 for PAL.
	*/
	ULONG	ex_EClockFrequency;	/* (readable) */
	ULONG	ex_CacheControl;	/* Private to CacheControl calls */
	ULONG	ex_TaskID;		/* Next available task ID */

	ULONG	ex_Reserved1[5];

	APTR	ex_MMULock;		/* private */

	ULONG	ex_Reserved2[3];

/****** V39 Exec additions start here **************************************/

	/* The following list and data element are used
	 * for V39 exec's low memory handler...
	 */
	struct	MinList	ex_MemHandlers;	/* The handler list */
	APTR	ex_MemHandler;		/* Private! handler pointer */
};


/****** Bit defines for AttnFlags (see above) ******************************/

/*  Processors and Co-processors: */
#define AFB_68010	0	/* also set for 68020 */
#define AFB_68020	1	/* also set for 68030 */
#define AFB_68030	2	/* also set for 68040 */
#define AFB_68040	3	/* also set for 68060 */
#define AFB_68881	4	/* also set for 68882 */
#define AFB_68882	5
#define	AFB_FPU40	6	/* Set if 68040 FPU */
#define AFB_68060	7
/*
 * The AFB_FPU40 bit is set when a working 68040 FPU
 * is in the system.  If this bit is set and both the
 * AFB_68881 and AFB_68882 bits are not set, then the 68040
 * math emulation code has not been loaded and only 68040
 * FPU instructions are available.  This bit is valid *ONLY*
 * if the AFB_68040 bit is set.
 */

#define AFB_PRIVATE	15	/* Just what it says */

#define AFF_68010	(1L<<0)
#define AFF_68020	(1L<<1)
#define AFF_68030	(1L<<2)
#define AFF_68040	(1L<<3)
#define AFF_68881	(1L<<4)
#define AFF_68882	(1L<<5)
#define	AFF_FPU40	(1L<<6)
#define AFF_68060	(1L<<7)

#define AFF_PRIVATE	(1L<<15)

/* #define AFB_RESERVED8   8 */
/* #define AFB_RESERVED9   9 */


/****** Selected flag definitions for Cache manipulation calls **********/

#define CACRF_EnableI	    (1L<<0)  /* Enable instruction cache */
#define CACRF_FreezeI	    (1L<<1)  /* Freeze instruction cache */
#define CACRF_ClearI	    (1L<<3)  /* Clear instruction cache  */
#define CACRF_IBE	    (1L<<4)  /* Instruction burst enable */
#define CACRF_EnableD	    (1L<<8)  /* 68030 Enable data cache  */
#define CACRF_FreezeD	    (1L<<9)  /* 68030 Freeze data cache  */
#define CACRF_ClearD	    (1L<<11) /* 68030 Clear data cache	 */
#define CACRF_DBE	    (1L<<12) /* 68030 Data burst enable */
#define CACRF_WriteAllocate (1L<<13) /* 68030 Write-Allocate mode
					(must always be set!)	 */
#define	CACRF_EnableE	    (1L<<30) /* Master enable for external caches */
				     /* External caches should track the */
				     /* state of the internal caches */
				     /* such that they do not cache anything */
				     /* that the internal cache turned off */
				     /* for. */
#define CACRF_CopyBack	    (1L<<31) /* Master enable for copyback caches */

#define DMA_Continue	    (1L<<1)  /* Continuation flag for CachePreDMA */
#define DMA_NoModify	    (1L<<2)  /* Set if DMA does not update memory */
#define	DMA_ReadFromRAM     (1L<<3)  /* Set if DMA goes *FROM* RAM to device */


#endif	/* EXEC_EXECBASE_H */
```

## 5.13. exec/resident.h — ROMTag (Resident), RTF_* flags, RTC_MATCHWORD

// Source: NDK_3.9/Include/include_h/exec/resident.h
// ROMTag descriptors. rt_MatchWord is $4AFC (68000 ILLEGAL).

```c
#ifndef	EXEC_RESIDENT_H
#define	EXEC_RESIDENT_H
/*
**	$VER: resident.h 39.0 (15.10.1991)
**	Includes Release 45.1
**
**	Resident/ROMTag stuff.	Used to identify and initialize code modules.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif /* EXEC_TYPES_H */


struct Resident {
    UWORD rt_MatchWord;	/* word to match on (ILLEGAL)	*/
    struct Resident *rt_MatchTag; /* pointer to the above	*/
    APTR  rt_EndSkip;		/* address to continue scan	*/
    UBYTE rt_Flags;		/* various tag flags		*/
    UBYTE rt_Version;		/* release version number	*/
    UBYTE rt_Type;		/* type of module (NT_XXXXXX)	*/
    BYTE  rt_Pri;		/* initialization priority */
    char  *rt_Name;		/* pointer to node name	*/
    char  *rt_IdString;	/* pointer to identification string */
    APTR  rt_Init;		/* pointer to init code	*/
};

#define RTC_MATCHWORD	0x4AFC	/* The 68000 "ILLEGAL" instruction */

#define RTF_AUTOINIT	(1<<7)	/* rt_Init points to data structure */
#define RTF_AFTERDOS	(1<<2)
#define RTF_SINGLETASK	(1<<1)
#define RTF_COLDSTART	(1<<0)

/* Compatibility: (obsolete) */
/* #define RTM_WHEN	   3 */
#define RTW_NEVER	0
#define RTW_COLDSTART	1

#endif	/* EXEC_RESIDENT_H */
```

## 5.14. exec/alerts.h — AN_*, AG_*, AO_*, AT_* alert codes

// Source: NDK_3.9/Include/include_h/exec/alerts.h
// Alert numbers. High bit = DeadEnd. SubSysId in next 7 bits, general error in byte 2, specific in low word.

```c
#ifndef EXEC_ALERTS_H
#define EXEC_ALERTS_H
/*
**	$VER: alerts.h 39.3 (12.5.1992)
**	Includes Release 45.1
**
**	Alert numbers, as displayed by system crashes.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

/*********************************************************************
*
*  Format of the alert error number:
*
*    +-+-------------+----------------+--------------------------------+
*    |D|  SubSysId   |	General Error |    SubSystem Specific Error    |
*    +-+-------------+----------------+--------------------------------+
*     1    7 bits	   8 bits		   16 bits
*
*		     D:  DeadEnd alert
*	      SubSysId:  indicates ROM subsystem number.
*	 General Error:  roughly indicates what the error was
*	Specific Error:  indicates more detail
**********************************************************************/

/**********************************************************************
*
*  Hardware/CPU specific alerts:  They may show without the 8 at the
*  front of the number.  These are CPU/68000 specific.	See 680x0
*  programmer's manuals for more details.
*
**********************************************************************/
#define	ACPU_BusErr	0x80000002	/* Hardware bus fault/access error */
#define	ACPU_AddressErr	0x80000003	/* Illegal address access (ie: odd) */
#define	ACPU_InstErr	0x80000004	/* Illegal instruction */
#define	ACPU_DivZero	0x80000005	/* Divide by zero */
#define	ACPU_CHK	0x80000006	/* Check instruction error */
#define	ACPU_TRAPV	0x80000007	/* TrapV instruction error */
#define	ACPU_PrivErr	0x80000008	/* Privilege violation error */
#define	ACPU_Trace	0x80000009	/* Trace error */
#define	ACPU_LineA	0x8000000A	/* Line 1010 Emulator error */
#define	ACPU_LineF	0x8000000B	/* Line 1111 Emulator error */
#define	ACPU_Format	0x8000000E	/* Stack frame format error */
#define	ACPU_Spurious	0x80000018	/* Spurious interrupt error */
#define	ACPU_AutoVec1	0x80000019	/* AutoVector Level 1 interrupt error */
#define	ACPU_AutoVec2	0x8000001A	/* AutoVector Level 2 interrupt error */
#define	ACPU_AutoVec3	0x8000001B	/* AutoVector Level 3 interrupt error */
#define	ACPU_AutoVec4	0x8000001C	/* AutoVector Level 4 interrupt error */
#define	ACPU_AutoVec5	0x8000001D	/* AutoVector Level 5 interrupt error */
#define	ACPU_AutoVec6	0x8000001E	/* AutoVector Level 6 interrupt error */
#define	ACPU_AutoVec7	0x8000001F	/* AutoVector Level 7 interrupt error */

/*********************************************************************
*
*  General Alerts
*
*  For example: timer.device cannot open math.library would be 0x05038015
*
*	Alert(AN_TimerDev|AG_OpenLib|AO_MathLib);
*
*********************************************************************/

/*------ alert types */
#define AT_DeadEnd	0x80000000
#define AT_Recovery	0x00000000

/*------ general purpose alert codes */
#define AG_NoMemory	0x00010000
#define AG_MakeLib	0x00020000
#define AG_OpenLib	0x00030000
#define AG_OpenDev	0x00040000
#define AG_OpenRes	0x00050000
#define AG_IOError	0x00060000
#define AG_NoSignal	0x00070000
#define AG_BadParm	0x00080000
#define AG_CloseLib	0x00090000	/* usually too many closes */
#define AG_CloseDev	0x000A0000	/* or a mismatched close */
#define AG_ProcCreate	0x000B0000	/* Process creation failed */

/*------ alert objects: */
#define AO_ExecLib	0x00008001
#define AO_GraphicsLib	0x00008002
#define AO_LayersLib	0x00008003
#define AO_Intuition	0x00008004
#define AO_MathLib	0x00008005
#define AO_DOSLib	0x00008007
#define AO_RAMLib	0x00008008
#define AO_IconLib	0x00008009
#define AO_ExpansionLib 0x0000800A
#define AO_DiskfontLib	0x0000800B
#define AO_UtilityLib	0x0000800C
#define	AO_KeyMapLib	0x0000800D

#define AO_AudioDev	0x00008010
#define AO_ConsoleDev	0x00008011
#define AO_GamePortDev	0x00008012
#define AO_KeyboardDev	0x00008013
#define AO_TrackDiskDev 0x00008014
#define AO_TimerDev	0x00008015

#define AO_CIARsrc	0x00008020
#define AO_DiskRsrc	0x00008021
#define AO_MiscRsrc	0x00008022

#define AO_BootStrap	0x00008030
#define AO_Workbench	0x00008031
#define AO_DiskCopy	0x00008032
#define AO_GadTools	0x00008033
#define AO_Unknown	0x00008035

/*********************************************************************
*
*   Specific Alerts:
*
*   For example:   exec.library -- corrupted memory list
*
*	    ALERT  AN_MemCorrupt	;8100 0005
*
*********************************************************************/

/*------ exec.library */
#define AN_ExecLib	0x01000000
#define AN_ExcptVect	0x01000001 /* 68000 exception vector checksum (obs.) */
#define AN_BaseChkSum	0x01000002 /* Execbase checksum (obs.) */
#define AN_LibChkSum	0x01000003 /* Library checksum failure */

#define AN_MemCorrupt	0x81000005 /* Corrupt memory list detected in FreeMem */
#define AN_IntrMem	0x81000006 /* No memory for interrupt servers */
#define AN_InitAPtr	0x01000007 /* InitStruct() of an APTR source (obs.) */
#define AN_SemCorrupt	0x01000008 /* A semaphore is in an illegal state
				      at ReleaseSemaphore() */
#define AN_FreeTwice	0x01000009 /* Freeing memory already freed */
#define AN_BogusExcpt	0x8100000A /* illegal 68k exception taken (obs.) */
#define AN_IOUsedTwice	0x0100000B /* Attempt to reuse active IORequest */
#define AN_MemoryInsane 0x0100000C /* Sanity check on memory list failed
				      during AvailMem(MEMF_LARGEST) */
#define AN_IOAfterClose 0x0100000D /* IO attempted on closed IORequest */
#define AN_StackProbe	0x0100000E /* Stack appears to extend out of range */
#define AN_BadFreeAddr	0x0100000F /* Memory header not located. [ Usually an
				      invalid address passed to FreeMem() ] */
#define	AN_BadSemaphore	0x01000010 /* An attempt was made to use the old
				      message semaphores. */

/*------ graphics.library */
#define AN_GraphicsLib	0x02000000
#define AN_GfxNoMem	0x82010000	/* graphics out of memory */
#define AN_GfxNoMemMspc 0x82010001	/* MonitorSpec alloc, no memory */
#define AN_LongFrame	0x82010006	/* long frame, no memory */
#define AN_ShortFrame	0x82010007	/* short frame, no memory */
#define AN_TextTmpRas	0x02010009	/* text, no memory for TmpRas */
#define AN_BltBitMap	0x8201000A	/* BltBitMap, no memory */
#define AN_RegionMemory 0x8201000B	/* regions, memory not available */
#define AN_MakeVPort	0x82010030	/* MakeVPort, no memory */
#define AN_GfxNewError	0x0200000C
#define AN_GfxFreeError 0x0200000D

#define AN_GfxNoLCM	0x82011234	/* emergency memory not available */

#define AN_ObsoleteFont 0x02000401	/* unsupported font description used */

/*------ layers.library */
#define AN_LayersLib	0x03000000
#define AN_LayersNoMem	0x83010000	/* layers out of memory */

/*------ intuition.library */
#define AN_Intuition	0x04000000
#define AN_GadgetType	0x84000001	/* unknown gadget type */
#define AN_BadGadget	0x04000001	/* Recovery form of AN_GadgetType */
#define AN_CreatePort	0x84010002	/* create port, no memory */
#define AN_ItemAlloc	0x04010003	/* item plane alloc, no memory */
#define AN_SubAlloc	0x04010004	/* sub alloc, no memory */
#define AN_PlaneAlloc	0x84010005	/* plane alloc, no memory */
#define AN_ItemBoxTop	0x84000006	/* item box top < RelZero */
#define AN_OpenScreen	0x84010007	/* open screen, no memory */
#define AN_OpenScrnRast 0x84010008	/* open screen, raster alloc, no memory */
#define AN_SysScrnType	0x84000009	/* open sys screen, unknown type */
#define AN_AddSWGadget	0x8401000A	/* add SW gadgets, no memory */
#define AN_OpenWindow	0x8401000B	/* open window, no memory */
#define AN_BadState	0x8400000C	/* Bad State Return entering Intuition */
#define AN_BadMessage	0x8400000D	/* Bad Message received by IDCMP */
#define AN_WeirdEcho	0x8400000E	/* Weird echo causing incomprehension */
#define AN_NoConsole	0x8400000F	/* couldn't open the Console Device */
#define	AN_NoISem	0x04000010	/* Intuition skipped obtaining a sem */
#define	AN_ISemOrder	0x04000011	/* Intuition obtained a sem in bad order */

/*------ math.library */
#define AN_MathLib	0x05000000

/*------ dos.library */
#define AN_DOSLib	0x07000000
#define AN_StartMem	0x07010001 /* no memory at startup */
#define AN_EndTask	0x07000002 /* EndTask didn't */
#define AN_QPktFail	0x07000003 /* Qpkt failure */
#define AN_AsyncPkt	0x07000004 /* Unexpected packet received */
#define AN_FreeVec	0x07000005 /* Freevec failed */
#define AN_DiskBlkSeq	0x07000006 /* Disk block sequence error */
#define AN_BitMap	0x07000007 /* Bitmap corrupt */
#define AN_KeyFree	0x07000008 /* Key already free */
#define AN_BadChkSum	0x07000009 /* Invalid checksum */
#define AN_DiskError	0x0700000A /* Disk Error */
#define AN_KeyRange	0x0700000B /* Key out of range */
#define AN_BadOverlay	0x0700000C /* Bad overlay */
#define AN_BadInitFunc	0x0700000D /* Invalid init packet for cli/shell */
#define AN_FileReclosed 0x0700000E /* A filehandle was closed more than once */

/*------ ramlib.library */
#define AN_RAMLib	0x08000000
#define AN_BadSegList	0x08000001	/* no overlays in library seglists */

/*------ icon.library */
#define AN_IconLib	0x09000000

/*------ expansion.library */
#define AN_ExpansionLib 0x0A000000
#define AN_BadExpansionFree	0x0A000001 /* freeed free region */

/*------ diskfont.library */
#define AN_DiskfontLib	0x0B000000

/*------ audio.device */
#define AN_AudioDev	0x10000000

/*------ console.device */
#define AN_ConsoleDev	0x11000000
#define AN_NoWindow	0x11000001	/* Console can't open initial window */

/*------ gameport.device */
#define AN_GamePortDev	0x12000000

/*------ keyboard.device */
#define AN_KeyboardDev	0x13000000

/*------ trackdisk.device */
#define AN_TrackDiskDev 0x14000000
#define AN_TDCalibSeek	0x14000001	/* calibrate: seek error */
#define AN_TDDelay	0x14000002	/* delay: error on timer wait */

/*------ timer.device */
#define AN_TimerDev	0x15000000
#define AN_TMBadReq	0x15000001 /* bad request */
#define AN_TMBadSupply	0x15000002 /* power supply -- no 50/60Hz ticks */

/*------ cia.resource */
#define AN_CIARsrc	0x20000000

/*------ disk.resource */
#define AN_DiskRsrc	0x21000000
#define AN_DRHasDisk	0x21000001	/* get unit: already has disk */
#define AN_DRIntNoAct	0x21000002	/* interrupt: no active unit */

/*------ misc.resource */
#define AN_MiscRsrc	0x22000000

/*------ bootstrap */
#define AN_BootStrap	0x30000000
#define AN_BootError	0x30000001	/* boot code returned an error */

/*------ Workbench */
#define AN_Workbench			0x31000000
#define AN_NoFonts			0xB1000001
#define AN_WBBadStartupMsg1		0x31000001
#define AN_WBBadStartupMsg2		0x31000002
#define AN_WBBadIOMsg			0x31000003	/* Hacker code? */
#define AN_WBReLayoutToolMenu		0xB1010009	/* GadTools broke? */

/*------ DiskCopy */
#define AN_DiskCopy	0x32000000

/*------ toolkit for Intuition */
#define AN_GadTools	0x33000000

/*------ System utility library */
#define AN_UtilityLib	0x34000000

/*------ For use by any application that needs it */
#define AN_Unknown	0x35000000

#endif /* EXEC_ALERTS_H */
```

## 5.15. exec/errors.h — IOERR_* device error codes

// Source: NDK_3.9/Include/include_h/exec/errors.h
// Standard io_Error return codes. Negative values.

```c
#ifndef	EXEC_ERRORS_H
#define	EXEC_ERRORS_H
/*
**	$VER: errors.h 39.0 (15.10.1991)
**	Includes Release 45.1
**
**	Standard Device IO Errors (returned in io_Error)
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#define IOERR_OPENFAIL	 (-1) /* device/unit failed to open */
#define IOERR_ABORTED	 (-2) /* request terminated early [after AbortIO()] */
#define IOERR_NOCMD	 (-3) /* command not supported by device */
#define IOERR_BADLENGTH	 (-4) /* not a valid length (usually IO_LENGTH) */
#define IOERR_BADADDRESS (-5) /* invalid address (misaligned or bad range) */
#define IOERR_UNITBUSY	 (-6) /* device opens ok, but requested unit is busy */
#define IOERR_SELFTEST	 (-7) /* hardware failed self-test */

#endif	/* EXEC_ERRORS_H */
```

## 5.16. exec/initializers.h — InitStruct() macro helpers

// Source: NDK_3.9/Include/include_h/exec/initializers.h
// Macros used to build static init tables for InitStruct(). Mostly used in library skeletons.

```c
#ifndef	EXEC_INITIALIZERS_H
#define	EXEC_INITIALIZERS_H
/*
**	$VER: initializers.h 39.0 (15.10.1991)
**	Includes Release 45.1
**
**	Macros for use with the InitStruct() function.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#define	OFFSET(structName, structEntry) \
				(&(((struct structName *) 0)->structEntry))
#define	INITBYTE(offset,value)	0xe000,(UWORD) (offset),(UWORD) ((value)<<8)
#define	INITWORD(offset,value)	0xd000,(UWORD) (offset),(UWORD) (value)
#define	INITLONG(offset,value)	0xc000,(UWORD) (offset), \
				(UWORD) ((value)>>16), \
				(UWORD) ((value) & 0xffff)
#define	INITSTRUCT(size,offset,value,count) \
				(UWORD) (0xc000|(size<<12)|(count<<8)| \
				((UWORD) ((offset)>>16)), \
				((UWORD) (offset)) & 0xffff)
#endif /* EXEC_INITIALIZERS_H */
```

## 5.17. exec/avl.h — AVL tree primitives (V45+)

// Source: NDK_3.9/Include/include_h/exec/avl.h
// AVL tree support added in V45 Exec. Callback comparators return strcmp-like LONG.

```c
#ifndef EXEC_AVL_H
#define EXEC_AVL_H
/*
**	$VER: avl.h 45.4 (27.2.2001)
**	Includes Release 45.1
**
**	AVL tree data structure definitions
**
**	(C) Copyright 2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif /* EXEC_TYPES_H */

/* Don't even think about the contents of this structure. Just embed it
 * and reference it
 */
struct AVLNode
{
	ULONG reserved[4];
};

/* Note that this is really a totally abstract 32 bit value */
typedef void * AVLKey;

/* Callback functions for the AVL tree handling. They will have to return
 * strcmp like results for the given arguments (<0/0/>0).
 * You can compare to nodes or a node with a key.
 */
#ifdef __SASC
typedef LONG (* __asm AVLNODECOMP)(register __a0 struct AVLNode *avlnode1, register __a1 struct AVLNode *avlnode2);
typedef LONG (* __asm AVLKEYCOMP)(register __a0 struct AVLNode *avlnode1, register __a1 AVLKey avlkey);
#else
typedef APTR AVLNODECOMP;
typedef APTR AVLKEYCOMP;
#endif /* __SASC */

#endif /* EXEC_AVL_H */
```

## 5.18. exec/exec.h — umbrella include

// Source: NDK_3.9/Include/include_h/exec/exec.h
// Includes all other exec/ headers in dependency order.

```c
#ifndef EXEC_EXEC_H
#define EXEC_EXEC_H
/*
**	$VER: exec.h 39.0 (15.10.1991)
**	Includes Release 45.1
**
**	Include all other Exec include files in a non-overlapping order.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#include <exec/types.h>
#include <exec/nodes.h>
#include <exec/lists.h>
#include <exec/alerts.h>
#include <exec/errors.h>
#include <exec/initializers.h>
#include <exec/resident.h>
#include <exec/memory.h>
#include <exec/tasks.h>
#include <exec/ports.h>
#include <exec/interrupts.h>
#include <exec/semaphores.h>
#include <exec/libraries.h>
#include <exec/io.h>
#include <exec/devices.h>
#include <exec/execbase.h>

#endif	/* EXEC_EXEC_H */
```

# 6. DOS structs

Cross-reference: `amiga-dos-filesystem-disk.md` for DOS packet flow, BCPL calling convention.

## 6.1. dos/dos.h — constants, DateStamp, FileInfoBlock, InfoData, error codes

// Source: NDK_3.9/Include/include_h/dos/dos.h
// Top-level DOS header. FIB bits (FIBF_READ/WRITE/EXECUTE/DELETE — note: 0 = allowed), ERROR_* codes 103-243.

```c
#ifndef DOS_DOS_H
#define DOS_DOS_H
/*
**	$VER: dos.h 36.27 (5.4.1992)
**	Includes Release 45.1
**
**	Standard C header for AmigaDOS
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif


#define	 DOSNAME  "dos.library"

/* Predefined Amiga DOS global constants */

#define DOSTRUE (-1L)
#define DOSFALSE (0L)

/* Mode parameter to Open() */
#define MODE_OLDFILE	     1005   /* Open existing file read/write
				     * positioned at beginning of file. */
#define MODE_NEWFILE	     1006   /* Open freshly created file (delete
				     * old file) read/write, exclusive lock. */
#define MODE_READWRITE	     1004   /* Open old file w/shared lock,
				     * creates file if doesn't exist. */

/* Relative position to Seek() */
#define OFFSET_BEGINNING    -1	    /* relative to Begining Of File */
#define OFFSET_CURRENT	     0	    /* relative to Current file position */
#define OFFSET_END	     1	    /* relative to End Of File	  */

#define OFFSET_BEGINING	    OFFSET_BEGINNING  /* ancient compatibility */

#define BITSPERBYTE	     8
#define BYTESPERLONG	     4
#define BITSPERLONG	     32
#define MAXINT		     0x7FFFFFFF
#define MININT		     0x80000000

/* Passed as type to Lock() */
#define SHARED_LOCK	     -2	    /* File is readable by others */
#define ACCESS_READ	     -2	    /* Synonym */
#define EXCLUSIVE_LOCK	     -1	    /* No other access allowed	  */
#define ACCESS_WRITE	     -1	    /* Synonym */

struct DateStamp {
   LONG	 ds_Days;	      /* Number of days since Jan. 1, 1978 */
   LONG	 ds_Minute;	      /* Number of minutes past midnight */
   LONG	 ds_Tick;	      /* Number of ticks past minute */
}; /* DateStamp */

#define TICKS_PER_SECOND      50   /* Number of ticks in one second */

/* Returned by Examine() and ExNext(), must be on a 4 byte boundary */
struct FileInfoBlock {
   LONG	  fib_DiskKey;
   LONG	  fib_DirEntryType;  /* Type of Directory. If < 0, then a plain file.
			      * If > 0 a directory */
   char	  fib_FileName[108]; /* Null terminated. Max 30 chars used for now */
   LONG	  fib_Protection;    /* bit mask of protection, rwxd are 3-0.	   */
   LONG	  fib_EntryType;
   LONG	  fib_Size;	     /* Number of bytes in file */
   LONG	  fib_NumBlocks;     /* Number of blocks in file */
   struct DateStamp fib_Date;/* Date file last changed */
   char	  fib_Comment[80];  /* Null terminated comment associated with file */

   /* Note: the following fields are not supported by all filesystems.	*/
   /* They should be initialized to 0 sending an ACTION_EXAMINE packet.	*/
   /* When Examine() is called, these are set to 0 for you.		*/
   /* AllocDosObject() also initializes them to 0.			*/
   UWORD  fib_OwnerUID;		/* owner's UID */
   UWORD  fib_OwnerGID;		/* owner's GID */

   char	  fib_Reserved[32];
}; /* FileInfoBlock */

/* FIB stands for FileInfoBlock */

/* FIBB are bit definitions, FIBF are field definitions */
/* Regular RWED bits are 0 == allowed. */
/* NOTE: GRP and OTR RWED permissions are 0 == not allowed! */
/* Group and Other permissions are not directly handled by the filesystem */
#define FIBB_OTR_READ	   15	/* Other: file is readable */
#define FIBB_OTR_WRITE	   14	/* Other: file is writable */
#define FIBB_OTR_EXECUTE   13	/* Other: file is executable */
#define FIBB_OTR_DELETE    12	/* Other: prevent file from being deleted */
#define FIBB_GRP_READ	   11	/* Group: file is readable */
#define FIBB_GRP_WRITE	   10	/* Group: file is writable */
#define FIBB_GRP_EXECUTE   9	/* Group: file is executable */
#define FIBB_GRP_DELETE    8	/* Group: prevent file from being deleted */

#define FIBB_SCRIPT    6	/* program is a script (execute) file */
#define FIBB_PURE      5	/* program is reentrant and rexecutable */
#define FIBB_ARCHIVE   4	/* cleared whenever file is changed */
#define FIBB_READ      3	/* ignored by old filesystem */
#define FIBB_WRITE     2	/* ignored by old filesystem */
#define FIBB_EXECUTE   1	/* ignored by system, used by Shell */
#define FIBB_DELETE    0	/* prevent file from being deleted */

#define FIBF_OTR_READ	   (1<<FIBB_OTR_READ)
#define FIBF_OTR_WRITE	   (1<<FIBB_OTR_WRITE)
#define FIBF_OTR_EXECUTE   (1<<FIBB_OTR_EXECUTE)
#define FIBF_OTR_DELETE    (1<<FIBB_OTR_DELETE)
#define FIBF_GRP_READ	   (1<<FIBB_GRP_READ)
#define FIBF_GRP_WRITE	   (1<<FIBB_GRP_WRITE)
#define FIBF_GRP_EXECUTE   (1<<FIBB_GRP_EXECUTE)
#define FIBF_GRP_DELETE    (1<<FIBB_GRP_DELETE)

#define FIBF_SCRIPT    (1<<FIBB_SCRIPT)
#define FIBF_PURE      (1<<FIBB_PURE)
#define FIBF_ARCHIVE   (1<<FIBB_ARCHIVE)
#define FIBF_READ      (1<<FIBB_READ)
#define FIBF_WRITE     (1<<FIBB_WRITE)
#define FIBF_EXECUTE   (1<<FIBB_EXECUTE)
#define FIBF_DELETE    (1<<FIBB_DELETE)

/* Standard maximum length for an error string from fault.  However, most */
/* error strings should be kept under 60 characters if possible.  Don't   */
/* forget space for the header you pass in. */
#define FAULT_MAX	82

/* All BCPL data must be long word aligned.  BCPL pointers are the long word
 *  address (i.e byte address divided by 4 (>>2)) */
typedef long  BPTR;		    /* Long word pointer */
typedef long  BSTR;		    /* Long word pointer to BCPL string	 */

/* Convert BPTR to typical C pointer */
#ifdef OBSOLETE_LIBRARIES_DOS_H
#define BADDR( bptr )	(((ULONG)bptr) << 2)
#else
/* This one has no problems with CASTing */
#define BADDR(x)	((APTR)((ULONG)(x) << 2))
#endif
/* Convert address into a BPTR */
#define MKBADDR(x)	(((LONG)(x)) >> 2)

/* BCPL strings have a length in the first byte and then the characters.
 * For example:	 s[0]=3 s[1]=S s[2]=Y s[3]=S				 */

/* returned by Info(), must be on a 4 byte boundary */
struct InfoData {
   LONG	  id_NumSoftErrors;	/* number of soft errors on disk */
   LONG	  id_UnitNumber;	/* Which unit disk is (was) mounted on */
   LONG	  id_DiskState;		/* See defines below */
   LONG	  id_NumBlocks;		/* Number of blocks on disk */
   LONG	  id_NumBlocksUsed;	/* Number of block in use */
   LONG	  id_BytesPerBlock;
   LONG	  id_DiskType;		/* Disk Type code */
   BPTR	  id_VolumeNode;	/* BCPL pointer to volume node (see DosList) */
   LONG	  id_InUse;		/* Flag, zero if not in use */
}; /* InfoData */

/* ID stands for InfoData */
	/* Disk states */
#define ID_WRITE_PROTECTED 80	 /* Disk is write protected */
#define ID_VALIDATING	   81	 /* Disk is currently being validated */
#define ID_VALIDATED	   82	 /* Disk is consistent and writeable */

	/* Disk types */
/* ID_INTER_* use international case comparison routines for hashing */
/* Any other new filesystems should also, if possible. */
#define ID_NO_DISK_PRESENT	(-1)
#define ID_UNREADABLE_DISK	(0x42414400L)	/* 'BAD\0' */
#define ID_DOS_DISK		(0x444F5300L)	/* 'DOS\0' */
#define ID_FFS_DISK		(0x444F5301L)	/* 'DOS\1' */
#define ID_INTER_DOS_DISK	(0x444F5302L)	/* 'DOS\2' */
#define ID_INTER_FFS_DISK	(0x444F5303L)	/* 'DOS\3' */
#define ID_FASTDIR_DOS_DISK	(0x444F5304L)	/* 'DOS\4' */
#define ID_FASTDIR_FFS_DISK	(0x444F5305L)	/* 'DOS\5' */
#define ID_NOT_REALLY_DOS	(0x4E444F53L)	/* 'NDOS'  */
#define ID_KICKSTART_DISK	(0x4B49434BL)	/* 'KICK'  */
#define ID_MSDOS_DISK		(0x4d534400L)	/* 'MSD\0' */

/* Errors from IoErr(), etc. */
#define ERROR_NO_FREE_STORE		  103
#define ERROR_TASK_TABLE_FULL		  105
#define ERROR_BAD_TEMPLATE		  114
#define ERROR_BAD_NUMBER		  115
#define ERROR_REQUIRED_ARG_MISSING	  116
#define ERROR_KEY_NEEDS_ARG		  117
#define ERROR_TOO_MANY_ARGS		  118
#define ERROR_UNMATCHED_QUOTES		  119
#define ERROR_LINE_TOO_LONG		  120
#define ERROR_FILE_NOT_OBJECT		  121
#define ERROR_INVALID_RESIDENT_LIBRARY	  122
#define ERROR_NO_DEFAULT_DIR		  201
#define ERROR_OBJECT_IN_USE		  202
#define ERROR_OBJECT_EXISTS		  203
#define ERROR_DIR_NOT_FOUND		  204
#define ERROR_OBJECT_NOT_FOUND		  205
#define ERROR_BAD_STREAM_NAME		  206
#define ERROR_OBJECT_TOO_LARGE		  207
#define ERROR_ACTION_NOT_KNOWN		  209
#define ERROR_INVALID_COMPONENT_NAME	  210
#define ERROR_INVALID_LOCK		  211
#define ERROR_OBJECT_WRONG_TYPE		  212
#define ERROR_DISK_NOT_VALIDATED	  213
#define ERROR_DISK_WRITE_PROTECTED	  214
#define ERROR_RENAME_ACROSS_DEVICES	  215
#define ERROR_DIRECTORY_NOT_EMPTY	  216
#define ERROR_TOO_MANY_LEVELS		  217
#define ERROR_DEVICE_NOT_MOUNTED	  218
#define ERROR_SEEK_ERROR		  219
#define ERROR_COMMENT_TOO_BIG		  220
#define ERROR_DISK_FULL			  221
#define ERROR_DELETE_PROTECTED		  222
#define ERROR_WRITE_PROTECTED		  223
#define ERROR_READ_PROTECTED		  224
#define ERROR_NOT_A_DOS_DISK		  225
#define ERROR_NO_DISK			  226
#define ERROR_NO_MORE_ENTRIES		  232
/* added for 1.4 */
#define ERROR_IS_SOFT_LINK		  233
#define ERROR_OBJECT_LINKED		  234
#define ERROR_BAD_HUNK			  235
#define ERROR_NOT_IMPLEMENTED		  236
#define ERROR_RECORD_NOT_LOCKED		  240
#define ERROR_LOCK_COLLISION		  241
#define ERROR_LOCK_TIMEOUT		  242
#define ERROR_UNLOCK_ERROR		  243

/* error codes 303-305 are defined in dosasl.h */

/* These are the return codes used by convention by AmigaDOS commands */
/* See FAILAT and IF for relvance to EXECUTE files		      */
#define RETURN_OK			    0  /* No problems, success */
#define RETURN_WARN			    5  /* A warning only */
#define RETURN_ERROR			   10  /* Something wrong */
#define RETURN_FAIL			   20  /* Complete or severe failure*/

/* Bit numbers that signal you that a user has issued a break */
#define SIGBREAKB_CTRL_C   12
#define SIGBREAKB_CTRL_D   13
#define SIGBREAKB_CTRL_E   14
#define SIGBREAKB_CTRL_F   15

/* Bit fields that signal you that a user has issued a break */
/* for example:	 if (SetSignal(0,0) & SIGBREAKF_CTRL_C) cleanup_and_exit(); */
#define SIGBREAKF_CTRL_C   (1<<SIGBREAKB_CTRL_C)
#define SIGBREAKF_CTRL_D   (1<<SIGBREAKB_CTRL_D)
#define SIGBREAKF_CTRL_E   (1<<SIGBREAKB_CTRL_E)
#define SIGBREAKF_CTRL_F   ((long)1<<SIGBREAKB_CTRL_F)

/* Values returned by SameLock() */
#define LOCK_DIFFERENT		-1
#define LOCK_SAME		0
#define LOCK_SAME_VOLUME	1	/* locks are on same volume */
#define LOCK_SAME_HANDLER	LOCK_SAME_VOLUME
/* LOCK_SAME_HANDLER was a misleading name, def kept for src compatibility */

/* types for ChangeMode() */
#define CHANGE_LOCK	0
#define CHANGE_FH	1

/* Values for MakeLink() */
#define LINK_HARD	0
#define LINK_SOFT	1	/* softlinks are not fully supported yet */

/* values returned by ReadItem */
#define	ITEM_EQUAL	-2		/* "=" Symbol */
#define ITEM_ERROR	-1		/* error */
#define ITEM_NOTHING	0		/* *N, ;, endstreamch */
#define ITEM_UNQUOTED	1		/* unquoted item */
#define ITEM_QUOTED	2		/* quoted item */

/* types for AllocDosObject/FreeDosObject */
#define DOS_FILEHANDLE		0	/* few people should use this */
#define DOS_EXALLCONTROL	1	/* Must be used to allocate this! */
#define	DOS_FIB			2	/* useful */
#define DOS_STDPKT		3	/* for doing packet-level I/O */
#define DOS_CLI			4	/* for shell-writers, etc */
#define DOS_RDARGS		5	/* for ReadArgs if you pass it in */

#endif	/* DOS_DOS_H */
```

## 6.2. dos/dosextens.h — Process, FileHandle, DosPacket, DosLibrary, RootNode, DosInfo, DosList, FileLock, DevInfo, DeviceList, ACTION_*

// Source: NDK_3.9/Include/include_h/dos/dosextens.h
// The heart of DOS. Process extends Task. DosPacket is the inter-process message format used by file handlers. ACTION_* codes are the packet types.

```c
#ifndef DOS_DOSEXTENS_H
#define DOS_DOSEXTENS_H
/*
**	$VER: dosextens.h 36.41 (14.5.1992)
**	Includes Release 45.1
**
**	DOS structures not needed for the casual AmigaDOS user
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TASKS_H
#include <exec/tasks.h>
#endif
#ifndef EXEC_PORTS_H
#include <exec/ports.h>
#endif
#ifndef EXEC_LIBRARIES_H
#include <exec/libraries.h>
#endif
#ifndef EXEC_SEMAPHORES_H
#include <exec/semaphores.h>
#endif
#ifndef DEVICES_TIMER_H
#include <devices/timer.h>
#endif

#ifndef DOS_DOS_H
#include <dos/dos.h>
#endif

/* All DOS processes have this structure */
/* Create and Device Proc returns pointer to the MsgPort in this structure */
/* dev_proc = (struct Process *) (DeviceProc(..) - sizeof(struct Task)); */

struct Process {
    struct  Task    pr_Task;
    struct  MsgPort pr_MsgPort; /* This is BPTR address from DOS functions  */
    WORD    pr_Pad;		/* Remaining variables on 4 byte boundaries */
    BPTR    pr_SegList;		/* Array of seg lists used by this process  */
    LONG    pr_StackSize;	/* Size of process stack in bytes	    */
    APTR    pr_GlobVec;		/* Global vector for this process (BCPL)    */
    LONG    pr_TaskNum;		/* CLI task number of zero if not a CLI	    */
    BPTR    pr_StackBase;	/* Ptr to high memory end of process stack  */
    LONG    pr_Result2;		/* Value of secondary result from last call */
    BPTR    pr_CurrentDir;	/* Lock associated with current directory   */
    BPTR    pr_CIS;		/* Current CLI Input Stream		    */
    BPTR    pr_COS;		/* Current CLI Output Stream		    */
    APTR    pr_ConsoleTask;	/* Console handler process for current window*/
    APTR    pr_FileSystemTask;	/* File handler process for current drive   */
    BPTR    pr_CLI;		/* pointer to CommandLineInterface	    */
    APTR    pr_ReturnAddr;	/* pointer to previous stack frame	    */
    APTR    pr_PktWait;		/* Function to be called when awaiting msg  */
    APTR    pr_WindowPtr;	/* Window for error printing		    */

    /* following definitions are new with 2.0 */
    BPTR    pr_HomeDir;		/* Home directory of executing program	    */
    LONG    pr_Flags;		/* flags telling dos about process	    */
    void    (*pr_ExitCode)();	/* code to call on exit of program or NULL  */
    LONG    pr_ExitData;	/* Passed as an argument to pr_ExitCode.    */
    UBYTE   *pr_Arguments;	/* Arguments passed to the process at start */
    struct MinList pr_LocalVars; /* Local environment variables		    */
    ULONG   pr_ShellPrivate;	/* for the use of the current shell	    */
    BPTR    pr_CES;		/* Error stream - if NULL, use pr_COS	    */
};  /* Process */

/*
 * Flags for pr_Flags
 */
#define	PRB_FREESEGLIST		0
#define	PRF_FREESEGLIST		1
#define	PRB_FREECURRDIR		1
#define	PRF_FREECURRDIR		2
#define	PRB_FREECLI		2
#define	PRF_FREECLI		4
#define	PRB_CLOSEINPUT		3
#define	PRF_CLOSEINPUT		8
#define	PRB_CLOSEOUTPUT		4
#define	PRF_CLOSEOUTPUT		16
#define	PRB_FREEARGS		5
#define	PRF_FREEARGS		32

/* The long word address (BPTR) of this structure is returned by
 * Open() and other routines that return a file.  You need only worry
 * about this struct to do async io's via PutMsg() instead of
 * standard file system calls */

struct FileHandle {
   struct Message *fh_Link;	 /* EXEC message	      */
   struct MsgPort *fh_Port;	 /* Reply port for the packet */
   struct MsgPort *fh_Type;	 /* Port to do PutMsg() to
				  * Address is negative if a plain file */
   LONG fh_Buf;
   LONG fh_Pos;
   LONG fh_End;
   LONG fh_Funcs;
#define fh_Func1 fh_Funcs
   LONG fh_Func2;
   LONG fh_Func3;
   LONG fh_Args;
#define fh_Arg1 fh_Args
   LONG fh_Arg2;
}; /* FileHandle */

/* This is the extension to EXEC Messages used by DOS */

struct DosPacket {
   struct Message *dp_Link;	 /* EXEC message	      */
   struct MsgPort *dp_Port;	 /* Reply port for the packet */
				 /* Must be filled in each send. */
   LONG dp_Type;		 /* See ACTION_... below and
				  * 'R' means Read, 'W' means Write to the
				  * file system */
   LONG dp_Res1;		 /* For file system calls this is the result
				  * that would have been returned by the
				  * function, e.g. Write ('W') returns actual
				  * length written */
   LONG dp_Res2;		 /* For file system calls this is what would
				  * have been returned by IoErr() */
/*  Device packets common equivalents */
#define dp_Action  dp_Type
#define dp_Status  dp_Res1
#define dp_Status2 dp_Res2
#define dp_BufAddr dp_Arg1
   LONG dp_Arg1;
   LONG dp_Arg2;
   LONG dp_Arg3;
   LONG dp_Arg4;
   LONG dp_Arg5;
   LONG dp_Arg6;
   LONG dp_Arg7;
}; /* DosPacket */

/* A Packet does not require the Message to be before it in memory, but
 * for convenience it is useful to associate the two.
 * Also see the function init_std_pkt for initializing this structure */

struct StandardPacket {
   struct Message   sp_Msg;
   struct DosPacket sp_Pkt;
}; /* StandardPacket */

/* Packet types */
#define ACTION_NIL		0
#define ACTION_STARTUP		0
#define ACTION_GET_BLOCK	2	/* OBSOLETE */
#define ACTION_SET_MAP		4
#define ACTION_DIE		5
#define ACTION_EVENT		6
#define ACTION_CURRENT_VOLUME	7
#define ACTION_LOCATE_OBJECT	8
#define ACTION_RENAME_DISK	9
#define ACTION_WRITE		'W'
#define ACTION_READ		'R'
#define ACTION_FREE_LOCK	15
#define ACTION_DELETE_OBJECT	16
#define ACTION_RENAME_OBJECT	17
#define ACTION_MORE_CACHE	18
#define ACTION_COPY_DIR		19
#define ACTION_WAIT_CHAR	20
#define ACTION_SET_PROTECT	21
#define ACTION_CREATE_DIR	22
#define ACTION_EXAMINE_OBJECT	23
#define ACTION_EXAMINE_NEXT	24
#define ACTION_DISK_INFO	25
#define ACTION_INFO		26
#define ACTION_FLUSH		27
#define ACTION_SET_COMMENT	28
#define ACTION_PARENT		29
#define ACTION_TIMER		30
#define ACTION_INHIBIT		31
#define ACTION_DISK_TYPE	32
#define ACTION_DISK_CHANGE	33
#define ACTION_SET_DATE		34

#define ACTION_SCREEN_MODE	994

#define ACTION_READ_RETURN	1001
#define ACTION_WRITE_RETURN	1002
#define ACTION_SEEK		1008
#define ACTION_FINDUPDATE	1004
#define ACTION_FINDINPUT	1005
#define ACTION_FINDOUTPUT	1006
#define ACTION_END		1007
#define ACTION_SET_FILE_SIZE	1022	/* fast file system only in 1.3 */
#define ACTION_WRITE_PROTECT	1023	/* fast file system only in 1.3 */

/* new 2.0 packets */
#define ACTION_SAME_LOCK	40
#define ACTION_CHANGE_SIGNAL	995
#define ACTION_FORMAT		1020
#define ACTION_MAKE_LINK	1021
/**/
/**/
#define ACTION_READ_LINK	1024
#define ACTION_FH_FROM_LOCK	1026
#define ACTION_IS_FILESYSTEM	1027
#define ACTION_CHANGE_MODE	1028
/**/
#define ACTION_COPY_DIR_FH	1030
#define ACTION_PARENT_FH	1031
#define ACTION_EXAMINE_ALL	1033
#define ACTION_EXAMINE_FH	1034

#define ACTION_LOCK_RECORD	2008
#define ACTION_FREE_RECORD	2009

#define ACTION_ADD_NOTIFY	4097
#define ACTION_REMOVE_NOTIFY	4098

/* Added in V39: */
#define ACTION_EXAMINE_ALL_END	1035
#define ACTION_SET_OWNER	1036

/* Tell a file system to serialize the current volume. This is typically
 * done by changing the creation date of the disk. This packet does not take
 * any arguments.  NOTE: be prepared to handle failure of this packet for
 * V37 ROM filesystems.
 */
#define	ACTION_SERIALIZE_DISK	4200

/*
 * A structure for holding error messages - stored as array with error == 0
 * for the last entry.
 */
struct ErrorString {
	LONG  *estr_Nums;
	UBYTE *estr_Strings;
};

/* DOS library node structure.
 * This is the data at positive offsets from the library node.
 * Negative offsets from the node is the jump table to DOS functions
 * node = (struct DosLibrary *) OpenLibrary( "dos.library" .. )	     */

struct DosLibrary {
    struct Library dl_lib;
    struct RootNode *dl_Root; /* Pointer to RootNode, described below */
    APTR    dl_GV;	      /* Pointer to BCPL global vector	      */
    LONG    dl_A2;	      /* BCPL standard register values	      */
    LONG    dl_A5;
    LONG    dl_A6;
    struct ErrorString *dl_Errors;	  /* PRIVATE pointer to array of error msgs */
    struct timerequest *dl_TimeReq;	  /* PRIVATE pointer to timer request */
    struct Library     *dl_UtilityBase;   /* PRIVATE ptr to utility library */
    struct Library     *dl_IntuitionBase; /* PRIVATE ptr to intuition library */
};  /*	DosLibrary */

/*			       */

struct RootNode {
    BPTR    rn_TaskArray;	     /* [0] is max number of CLI's
				      * [1] is APTR to process id of CLI 1
				      * [n] is APTR to process id of CLI n */
    BPTR    rn_ConsoleSegment; /* SegList for the CLI			   */
    struct  DateStamp rn_Time; /* Current time				   */
    LONG    rn_RestartSeg;     /* SegList for the disk validator process   */
    BPTR    rn_Info;	       /* Pointer to the Info structure		   */
    BPTR    rn_FileHandlerSegment; /* segment for a file handler	   */
    struct MinList rn_CliList; /* new list of all CLI processes */
			       /* the first cpl_Array is also rn_TaskArray */
    struct MsgPort *rn_BootProc; /* private ptr to msgport of boot fs	   */
    BPTR    rn_ShellSegment;   /* seglist for Shell (for NewShell)	   */
    LONG    rn_Flags;	       /* dos flags */
};  /* RootNode */

#define RNB_WILDSTAR	24
#define RNF_WILDSTAR	(1L<<24)
#define RNB_PRIVATE1	1	/* private for dos */
#define RNF_PRIVATE1	2

/* ONLY to be allocated by DOS! */
struct CliProcList {
	struct MinNode cpl_Node;
	LONG cpl_First;	     /* number of first entry in array */
	struct MsgPort **cpl_Array;
			     /* [0] is max number of CLI's in this entry (n)
			      * [1] is CPTR to process id of CLI cpl_First
			      * [n] is CPTR to process id of CLI cpl_First+n-1
			      */
};

struct DosInfo {
    BPTR    di_McName;	       /* PRIVATE: system resident module list	    */
#define di_ResList di_McName
    BPTR    di_DevInfo;	       /* Device List				    */
    BPTR    di_Devices;	       /* Currently zero			    */
    BPTR    di_Handlers;       /* Currently zero			    */
    APTR    di_NetHand;	       /* Network handler processid; currently zero */
    struct  SignalSemaphore di_DevLock;	   /* do NOT access directly! */
    struct  SignalSemaphore di_EntryLock;  /* do NOT access directly! */
    struct  SignalSemaphore di_DeleteLock; /* do NOT access directly! */
};  /* DosInfo */

/* structure for the Dos resident list.  Do NOT allocate these, use	  */
/* AddSegment(), and heed the warnings in the autodocs!			  */

struct Segment {
	BPTR seg_Next;
	LONG seg_UC;
	BPTR seg_Seg;
	UBYTE seg_Name[4];	/* actually the first 4 chars of BSTR name */
};

#define CMD_SYSTEM	-1
#define CMD_INTERNAL	-2
#define CMD_DISABLED	-999


/* DOS Processes started from the CLI via RUN or NEWCLI have this additional
 * set to data associated with them */

struct CommandLineInterface {
    LONG   cli_Result2;	       /* Value of IoErr from last command	  */
    BSTR   cli_SetName;	       /* Name of current directory		  */
    BPTR   cli_CommandDir;     /* Head of the path locklist		  */
    LONG   cli_ReturnCode;     /* Return code from last command		  */
    BSTR   cli_CommandName;    /* Name of current command		  */
    LONG   cli_FailLevel;      /* Fail level (set by FAILAT)		  */
    BSTR   cli_Prompt;	       /* Current prompt (set by PROMPT)	  */
    BPTR   cli_StandardInput;  /* Default (terminal) CLI input		  */
    BPTR   cli_CurrentInput;   /* Current CLI input			  */
    BSTR   cli_CommandFile;    /* Name of EXECUTE command file		  */
    LONG   cli_Interactive;    /* Boolean; True if prompts required	  */
    LONG   cli_Background;     /* Boolean; True if CLI created by RUN	  */
    BPTR   cli_CurrentOutput;  /* Current CLI output			  */
    LONG   cli_DefaultStack;   /* Stack size to be obtained in long words */
    BPTR   cli_StandardOutput; /* Default (terminal) CLI output		  */
    BPTR   cli_Module;	       /* SegList of currently loaded command	  */
};  /* CommandLineInterface */

/* This structure can take on different values depending on whether it is
 * a device, an assigned directory, or a volume.  Below is the structure
 * reflecting volumes only.  Following that is the structure representing
 * only devices. Following that is the unioned structure representing all
 * the values
 */

/* structure representing a volume */

struct DeviceList {
    BPTR		dl_Next;	/* bptr to next device list */
    LONG		dl_Type;	/* see DLT below */
    struct MsgPort *	dl_Task;	/* ptr to handler task */
    BPTR		dl_Lock;	/* not for volumes */
    struct DateStamp	dl_VolumeDate;	/* creation date */
    BPTR		dl_LockList;	/* outstanding locks */
    LONG		dl_DiskType;	/* 'DOS', etc */
    LONG		dl_unused;
    BSTR		dl_Name;	/* bptr to bcpl name */
};

/* device structure (same as the DeviceNode structure in filehandler.h) */

struct	      DevInfo {
    BPTR  dvi_Next;
    LONG  dvi_Type;
    APTR  dvi_Task;
    BPTR  dvi_Lock;
    BSTR  dvi_Handler;
    LONG  dvi_StackSize;
    LONG  dvi_Priority;
    LONG  dvi_Startup;
    BPTR  dvi_SegList;
    BPTR  dvi_GlobVec;
    BSTR  dvi_Name;
};

/* combined structure for devices, assigned directories, volumes */

struct DosList {
    BPTR		dol_Next;	 /* bptr to next device on list */
    LONG		dol_Type;	 /* see DLT below */
    struct MsgPort     *dol_Task;	 /* ptr to handler task */
    BPTR		dol_Lock;
    union {
	struct {
	BSTR	dol_Handler;	/* file name to load if seglist is null */
	LONG	dol_StackSize;	/* stacksize to use when starting process */
	LONG	dol_Priority;	/* task priority when starting process */
	ULONG	dol_Startup;	/* startup msg: FileSysStartupMsg for disks */
	BPTR	dol_SegList;	/* already loaded code for new task */
	BPTR	dol_GlobVec;	/* BCPL global vector to use when starting
				 * a process. -1 indicates a C/Assembler
				 * program. */
	} dol_handler;

	struct {
	struct DateStamp	dol_VolumeDate;	 /* creation date */
	BPTR			dol_LockList;	 /* outstanding locks */
	LONG			dol_DiskType;	 /* 'DOS', etc */
	} dol_volume;

	struct {
	UBYTE	*dol_AssignName;     /* name for non-or-late-binding assign */
	struct AssignList *dol_List; /* for multi-directory assigns (regular) */
	} dol_assign;

    } dol_misc;

    BSTR		dol_Name;	 /* bptr to bcpl name */
    };

/* structure used for multi-directory assigns. AllocVec()ed. */

struct AssignList {
	struct AssignList *al_Next;
	BPTR		   al_Lock;
};

/* definitions for dl_Type */
#define DLT_DEVICE	0
#define DLT_DIRECTORY	1	/* assign */
#define DLT_VOLUME	2
#define DLT_LATE	3	/* late-binding assign */
#define DLT_NONBINDING	4	/* non-binding assign */
#define DLT_PRIVATE	-1	/* for internal use only */

/* structure return by GetDeviceProc() */
struct DevProc {
	struct MsgPort *dvp_Port;
	BPTR		dvp_Lock;
	ULONG		dvp_Flags;
	struct DosList *dvp_DevNode;	/* DON'T TOUCH OR USE! */
};

/* definitions for dvp_Flags */
#define DVPB_UNLOCK	0
#define DVPF_UNLOCK	(1L << DVPB_UNLOCK)	/* PRIVATE! */
#define DVPB_ASSIGN	1
#define DVPF_ASSIGN	(1L << DVPB_ASSIGN)

/* Flags to be passed to LockDosList(), etc */
#define LDB_DEVICES	2
#define LDF_DEVICES	(1L << LDB_DEVICES)
#define LDB_VOLUMES	3
#define LDF_VOLUMES	(1L << LDB_VOLUMES)
#define LDB_ASSIGNS	4
#define LDF_ASSIGNS	(1L << LDB_ASSIGNS)
#define LDB_ENTRY	5
#define LDF_ENTRY	(1L << LDB_ENTRY)
#define LDB_DELETE	6
#define LDF_DELETE	(1L << LDB_DELETE)

/* you MUST specify one of LDF_READ or LDF_WRITE */
#define LDB_READ	0
#define LDF_READ	(1L << LDB_READ)
#define LDB_WRITE	1
#define LDF_WRITE	(1L << LDB_WRITE)

/* actually all but LDF_ENTRY (which is used for internal locking) */
#define LDF_ALL		(LDF_DEVICES|LDF_VOLUMES|LDF_ASSIGNS)

/* a lock structure, as returned by Lock() or DupLock() */
struct FileLock {
    BPTR		fl_Link;	/* bcpl pointer to next lock */
    LONG		fl_Key;		/* disk block number */
    LONG		fl_Access;	/* exclusive or shared */
    struct MsgPort *	fl_Task;	/* handler task's port */
    BPTR		fl_Volume;	/* bptr to DLT_VOLUME DosList entry */
};

/* error report types for ErrorReport() */
#define REPORT_STREAM		0	/* a stream */
#define REPORT_TASK		1	/* a process - unused */
#define REPORT_LOCK		2	/* a lock */
#define REPORT_VOLUME		3	/* a volume node */
#define REPORT_INSERT		4	/* please insert volume */

/* Special error codes for ErrorReport() */
#define ABORT_DISK_ERROR	296	/* Read/write error */
#define ABORT_BUSY		288	/* You MUST replace... */

/* types for initial packets to shells from run/newcli/execute/system. */
/* For shell-writers only */
#define RUN_EXECUTE		-1
#define RUN_SYSTEM		-2
#define RUN_SYSTEM_ASYNCH	-3

/* Types for fib_DirEntryType.	NOTE that both USERDIR and ROOT are	 */
/* directories, and that directory/file checks should use <0 and >=0.	 */
/* This is not necessarily exhaustive!	Some handlers may use other	 */
/* values as needed, though <0 and >=0 should remain as supported as	 */
/* possible.								 */
#define ST_ROOT		1
#define ST_USERDIR	2
#define ST_SOFTLINK	3	/* looks like dir, but may point to a file! */
#define ST_LINKDIR	4	/* hard link to dir */
#define ST_FILE		-3	/* must be negative for FIB! */
#define ST_LINKFILE	-4	/* hard link to file */
#define ST_PIPEFILE	-5	/* for pipes that support ExamineFH */

#endif	/* DOS_DOSEXTENS_H */
```

## 6.3. dos/filehandler.h — DosEnvec, FileSysStartupMsg, DeviceNode

// Source: NDK_3.9/Include/include_h/dos/filehandler.h
// Disk geometry (de_Surfaces, de_BlocksPerTrack, de_LowCyl, de_HighCyl, de_DosType). Used by mount files and RDB partition blocks.

```c
#ifndef DOS_FILEHANDLER_H
#define DOS_FILEHANDLER_H
/*
**	$VER: filehandler.h 44.1 (24.8.99)
**	Includes Release 45.1
**
**	device and file handler specific code for AmigaDOS
**
**	(C) Copyright 1986-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	  EXEC_PORTS_H
#include <exec/ports.h>
#endif

#ifndef	  DOS_DOS_H
#include <dos/dos.h>
#endif


/* The disk "environment" is a longword array that describes the
 * disk geometry.  It is variable sized, with the length at the beginning.
 * Here are the constants for a standard geometry.
 */

struct DosEnvec {
    ULONG de_TableSize;	     /* Size of Environment vector */
    ULONG de_SizeBlock;	     /* in longwords: Physical disk block size */
    ULONG de_SecOrg;	     /* not used; must be 0 */
    ULONG de_Surfaces;	     /* # of heads (surfaces). drive specific */
    ULONG de_SectorPerBlock; /* N de_SizeBlock sectors per logical block */
    ULONG de_BlocksPerTrack; /* blocks per track. drive specific */
    ULONG de_Reserved;	     /* DOS reserved blocks at start of partition. */
    ULONG de_PreAlloc;	     /* DOS reserved blocks at end of partition */
    ULONG de_Interleave;     /* usually 0 */
    ULONG de_LowCyl;	     /* starting cylinder. typically 0 */
    ULONG de_HighCyl;	     /* max cylinder. drive specific */
    ULONG de_NumBuffers;     /* Initial # DOS of buffers.  */
    ULONG de_BufMemType;     /* type of mem to allocate for buffers */
    ULONG de_MaxTransfer;    /* Max number of bytes to transfer at a time */
    ULONG de_Mask;	     /* Address Mask to block out certain memory */
    LONG  de_BootPri;	     /* Boot priority for autoboot */
    ULONG de_DosType;	     /* ASCII (HEX) string showing filesystem type;
			      * 0X444F5300 is old filesystem,
			      * 0X444F5301 is fast file system */
    ULONG de_Baud;	     /* Baud rate for serial handler */
    ULONG de_Control;	     /* Control word for handler/filesystem */
    ULONG de_BootBlocks;     /* Number of blocks containing boot code */

};

/* these are the offsets into the array */
/* DE_TABLESIZE is set to the number of longwords in the table minus 1 */

#define DE_TABLESIZE	0	/* minimum value is 11 (includes NumBuffers) */
#define DE_SIZEBLOCK	1	/* in longwords: standard value is 128 */
#define DE_SECORG	2	/* not used; must be 0 */
#define DE_NUMHEADS	3	/* # of heads (surfaces). drive specific */
#define DE_SECSPERBLK	4	/* not used; must be 1 */
#define DE_BLKSPERTRACK 5	/* blocks per track. drive specific */
#define DE_RESERVEDBLKS 6	/* unavailable blocks at start.	 usually 2 */
#define DE_PREFAC	7	/* not used; must be 0 */
#define DE_INTERLEAVE	8	/* usually 0 */
#define DE_LOWCYL	9	/* starting cylinder. typically 0 */
#define DE_UPPERCYL	10	/* max cylinder.  drive specific */
#define DE_NUMBUFFERS	11	/* starting # of buffers.  typically 5 */
#define DE_MEMBUFTYPE	12	/* type of mem to allocate for buffers. */
#define DE_BUFMEMTYPE	12	/* same as above, better name
				 * 1 is public, 3 is chip, 5 is fast */
#define DE_MAXTRANSFER	13	/* Max number bytes to transfer at a time */
#define DE_MASK		14	/* Address Mask to block out certain memory */
#define DE_BOOTPRI	15	/* Boot priority for autoboot */
#define DE_DOSTYPE	16	/* ASCII (HEX) string showing filesystem type;
				 * 0X444F5300 is old filesystem,
				 * 0X444F5301 is fast file system */
#define DE_BAUD		17	/* Baud rate for serial handler */
#define DE_CONTROL	18	/* Control word for handler/filesystem */
#define DE_BOOTBLOCKS	19	/* Number of blocks containing boot code */

/* The file system startup message is linked into a device node's startup
** field.  It contains a pointer to the above environment, plus the
** information needed to do an exec OpenDevice().
*/
struct FileSysStartupMsg {
    ULONG	fssm_Unit;	/* exec unit number for this device */
    BSTR	fssm_Device;	/* null terminated bstring to the device name */
    BPTR	fssm_Environ;	/* ptr to environment table (see above) */
    ULONG	fssm_Flags;	/* flags for OpenDevice() */
};


/* The include file "libraries/dosextens.h" has a DeviceList structure.
 * The "device list" can have one of three different things linked onto
 * it.	Dosextens defines the structure for a volume.  DLT_DIRECTORY
 * is for an assigned directory.  The following structure is for
 * a dos "device" (DLT_DEVICE).
*/

struct DeviceNode {
    BPTR	dn_Next;	/* singly linked list */
    ULONG	dn_Type;	/* always 0 for dos "devices" */
    struct MsgPort *dn_Task;	/* standard dos "task" field.  If this is
				 * null when the node is accesses, a task
				 * will be started up */
    BPTR	dn_Lock;	/* not used for devices -- leave null */
    BSTR	dn_Handler;	/* filename to loadseg (if seglist is null) */
    ULONG	dn_StackSize;	/* stacksize to use when starting task */
    LONG	dn_Priority;	/* task priority when starting task */
    BPTR	dn_Startup;	/* startup msg: FileSysStartupMsg for disks */
    BPTR	dn_SegList;	/* code to run to start new task (if necessary).
				 * if null then dn_Handler will be loaded. */
    BPTR	dn_GlobalVec;	/* BCPL global vector to use when starting
				 * a task.  -1 means that dn_SegList is not
				 * for a bcpl program, so the dos won't
				 * try and construct one.  0 tell the
				 * dos that you obey BCPL linkage rules,
				 * and that it should construct a global
				 * vector for you.
				 */
    BSTR	dn_Name;	/* the node name, e.g. '\3','D','F','3' */
};

#endif	/* DOS_FILEHANDLER_H */
```

## 6.4. dos/notify.h — NotifyRequest, NotifyMessage

// Source: NDK_3.9/Include/include_h/dos/notify.h
// DOS notification API — get signalled/messaged when a file changes.

```c
#ifndef DOS_NOTIFY_H
#define DOS_NOTIFY_H
/*
**
**	$VER: notify.h 36.8 (29.8.1990)
**	Includes Release 45.1
**
**	dos notification definitions
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef EXEC_PORTS_H
#include <exec/ports.h>
#endif

#ifndef EXEC_TASKS_H
#include <exec/tasks.h>
#endif


/* use of Class and code is discouraged for the time being - we might want to
   change things */
/* --- NotifyMessage Class ------------------------------------------------ */
#define NOTIFY_CLASS	0x40000000

/* --- NotifyMessage Codes ------------------------------------------------ */
#define NOTIFY_CODE	0x1234


/* Sent to the application if SEND_MESSAGE is specified.		    */

struct NotifyMessage {
    struct Message nm_ExecMessage;
    ULONG  nm_Class;
    UWORD  nm_Code;
    struct NotifyRequest *nm_NReq;	/* don't modify the request! */
    ULONG  nm_DoNotTouch;		/* like it says!  For use by handlers */
    ULONG  nm_DoNotTouch2;		/* ditto */
};

/* Do not modify or reuse the notifyrequest while active.		    */
/* note: the first LONG of nr_Data has the length transfered		    */

struct NotifyRequest {
	UBYTE *nr_Name;
	UBYTE *nr_FullName;		/* set by dos - don't touch */
	ULONG nr_UserData;		/* for applications use */
	ULONG nr_Flags;

	union {

	    struct {
		struct MsgPort *nr_Port;	/* for SEND_MESSAGE */
	    } nr_Msg;

	    struct {
		struct Task *nr_Task;		/* for SEND_SIGNAL */
		UBYTE nr_SignalNum;		/* for SEND_SIGNAL */
		UBYTE nr_pad[3];
	    } nr_Signal;
	} nr_stuff;

	ULONG nr_Reserved[4];		/* leave 0 for now */

	/* internal use by handlers */
	ULONG nr_MsgCount;		/* # of outstanding msgs */
	struct MsgPort *nr_Handler;	/* handler sent to (for EndNotify) */
};

/* --- NotifyRequest Flags ------------------------------------------------ */
#define NRF_SEND_MESSAGE	1
#define NRF_SEND_SIGNAL		2
#define NRF_WAIT_REPLY		8
#define NRF_NOTIFY_INITIAL	16

/* do NOT set or remove NRF_MAGIC!  Only for use by handlers! */
#define NRF_MAGIC	0x80000000

/* bit numbers */
#define NRB_SEND_MESSAGE	0
#define NRB_SEND_SIGNAL		1
#define NRB_WAIT_REPLY		3
#define NRB_NOTIFY_INITIAL	4

#define NRB_MAGIC		31

/* Flags reserved for private use by the handler: */
#define NR_HANDLER_FLAGS	0xffff0000

#endif /* DOS_NOTIFY_H */
```

## 6.5. dos/rdargs.h — CSource, RDArgs, ReadArgs()

// Source: NDK_3.9/Include/include_h/dos/rdargs.h
// ReadArgs() template parser state.

```c
#ifndef DOS_RDARGS_H
#define DOS_RDARGS_H
/*
**
**	$VER: rdargs.h 36.6 (12.7.1990)
**	Includes Release 45.1
**
**	ReadArgs() structure definitions
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif

/**********************************************************************
 *
 * The CSource data structure defines the input source for "ReadItem()"
 * as well as the ReadArgs call.  It is a publicly defined structure
 * which may be used by applications which use code that follows the
 * conventions defined for access.
 *
 * When passed to the dos.library functions, the value passed as
 * struct *CSource is defined as follows:
 *	if ( CSource == 0)	Use buffered IO "ReadChar()" as data source
 *	else			Use CSource for input character stream
 *
 * The following two pseudo-code routines define how the CSource structure
 * is used:
 *
 * long CS_ReadChar( struct CSource *CSource )
 * {
 *	if ( CSource == 0 )	return ReadChar();
 *	if ( CSource->CurChr >= CSource->Length )	return ENDSTREAMCHAR;
 *	return CSource->Buffer[ CSource->CurChr++ ];
 * }
 *
 * BOOL CS_UnReadChar( struct CSource *CSource )
 * {
 *	if ( CSource == 0 )	return UnReadChar();
 *	if ( CSource->CurChr <= 0 )	return FALSE;
 *	CSource->CurChr--;
 *	return TRUE;
 * }
 *
 * To initialize a struct CSource, you set CSource->CS_Buffer to
 * a string which is used as the data source, and set CS_Length to
 * the number of characters in the string.  Normally CS_CurChr should
 * be initialized to ZERO, or left as it was from prior use as
 * a CSource.
 *
 **********************************************************************/

struct CSource {
	UBYTE	*CS_Buffer;
	LONG	CS_Length;
	LONG	CS_CurChr;
};

/**********************************************************************
 *
 * The RDArgs data structure is the input parameter passed to the DOS
 * ReadArgs() function call.
 *
 * The RDA_Source structure is a CSource as defined above;
 * if RDA_Source.CS_Buffer is non-null, RDA_Source is used as the input
 * character stream to parse, else the input comes from the buffered STDIN
 * calls ReadChar/UnReadChar.
 *
 * RDA_DAList is a private address which is used internally to track
 * allocations which are freed by FreeArgs().  This MUST be initialized
 * to NULL prior to the first call to ReadArgs().
 *
 * The RDA_Buffer and RDA_BufSiz fields allow the application to supply
 * a fixed-size buffer in which to store the parsed data.  This allows
 * the application to pre-allocate a buffer rather than requiring buffer
 * space to be allocated.  If either RDA_Buffer or RDA_BufSiz is NULL,
 * the application has not supplied a buffer.
 *
 * RDA_ExtHelp is a text string which will be displayed instead of the
 * template string, if the user is prompted for input.
 *
 * RDA_Flags bits control how ReadArgs() works.  The flag bits are
 * defined below.  Defaults are initialized to ZERO.
 *
 **********************************************************************/

struct RDArgs {
	struct	CSource RDA_Source;	/* Select input source */
	LONG	RDA_DAList;		/* PRIVATE. */
	UBYTE	*RDA_Buffer;		/* Optional string parsing space. */
	LONG	RDA_BufSiz;		/* Size of RDA_Buffer (0..n) */
	UBYTE	*RDA_ExtHelp;		/* Optional extended help */
	LONG	RDA_Flags;		/* Flags for any required control */
};

#define RDAB_STDIN	0	/* Use "STDIN" rather than "COMMAND LINE" */
#define RDAF_STDIN	1
#define RDAB_NOALLOC	1	/* If set, do not allocate extra string space.*/
#define RDAF_NOALLOC	2
#define RDAB_NOPROMPT	2	/* Disable reprompting for string input. */
#define RDAF_NOPROMPT	4

/**********************************************************************
 * Maximum number of template keywords which can be in a template passed
 * to ReadArgs(). IMPLEMENTOR NOTE - must be a multiple of 4.
 **********************************************************************/
#define MAX_TEMPLATE_ITEMS	100

/**********************************************************************
 * Maximum number of MULTIARG items returned by ReadArgs(), before
 * an ERROR_LINE_TOO_LONG.  These two limitations are due to stack
 * usage.  Applications should allow "a lot" of stack to use ReadArgs().
 **********************************************************************/
#define MAX_MULTIARGS		128

#endif /* DOS_RDARGS_H */
```

## 6.6. dos/record.h — RecordLock

// Source: NDK_3.9/Include/include_h/dos/record.h
// File record locking (byte ranges).

```c
#ifndef DOS_RECORD_H
#define DOS_RECORD_H
/*
**
**	$VER: record.h 36.5 (12.7.1990)
**	Includes Release 45.1
**
**	include file for record locking
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/

#ifndef DOS_DOS_H
#include <dos/dos.h>
#endif

/* Modes for LockRecord/LockRecords() */
#define REC_EXCLUSIVE		0
#define REC_EXCLUSIVE_IMMED	1
#define REC_SHARED		2
#define REC_SHARED_IMMED	3

/* struct to be passed to LockRecords()/UnLockRecords() */

struct RecordLock {
	BPTR	rec_FH;		/* filehandle */
	ULONG	rec_Offset;	/* offset in file */
	ULONG	rec_Length;	/* length of file to be locked */
	ULONG	rec_Mode;	/* Type of lock */
};

#endif /* DOS_RECORD_H */
```

## 6.7. dos/exall.h — ExAllData, ExAllControl, ED_* data types

// Source: NDK_3.9/Include/include_h/dos/exall.h
// ExAll() directory scan. ED_NAME..ED_OWNER selects how much info to return.

```c
#ifndef DOS_EXALL_H
#define DOS_EXALL_H
/*
**
**	$VER: exall.h 36.6 (5.4.1992)
**	Includes Release 45.1
**
**	include file for ExAll() data structures
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef UTILITY_HOOKS_H
#include <utility/hooks.h>
#endif

/* NOTE: V37 dos.library, when doing ExAll() emulation, and V37 filesystems  */
/* will return an error if passed ED_OWNER.  If you get ERROR_BAD_NUMBER,    */
/* retry with ED_COMMENT to get everything but owner info.  All filesystems  */
/* supporting ExAll() must support through ED_COMMENT, and must check Type   */
/* and return ERROR_BAD_NUMBER if they don't support the type.		     */

/* values that can be passed for what data you want from ExAll() */
/* each higher value includes those below it (numerically)	 */
/* you MUST chose one of these values */
#define	ED_NAME		1
#define	ED_TYPE		2
#define ED_SIZE		3
#define ED_PROTECTION	4
#define ED_DATE		5
#define ED_COMMENT	6
#define ED_OWNER	7

/*
 *   Structure in which exall results are returned in.	Note that only the
 *   fields asked for will exist!
 */

struct ExAllData {
	struct ExAllData *ed_Next;
	UBYTE  *ed_Name;
	LONG	ed_Type;
	ULONG	ed_Size;
	ULONG	ed_Prot;
	ULONG	ed_Days;
	ULONG	ed_Mins;
	ULONG	ed_Ticks;
	UBYTE  *ed_Comment;	/* strings will be after last used field */
	UWORD	ed_OwnerUID;	/* new for V39 */
	UWORD	ed_OwnerGID;
};

/*
 *   Control structure passed to ExAll.  Unused fields MUST be initialized to
 *   0, expecially eac_LastKey.
 *
 *   eac_MatchFunc is a hook (see utility.library documentation for usage)
 *   It should return true if the entry is to returned, false if it is to be
 *   ignored.
 *
 *   This structure MUST be allocated by AllocDosObject()!
 */

struct ExAllControl {
	ULONG	eac_Entries;	 /* number of entries returned in buffer      */
	ULONG	eac_LastKey;	 /* Don't touch inbetween linked ExAll calls! */
	UBYTE  *eac_MatchString; /* wildcard string for pattern match or NULL */
	struct Hook *eac_MatchFunc; /* optional private wildcard function     */
};

#endif /* DOS_EXALL_H */
```

## 6.8. dos/dosasl.h — AnchorPath, AChain, pattern P_* tokens

// Source: NDK_3.9/Include/include_h/dos/dosasl.h
// MatchFirst/MatchNext wildcard matching. P_ANY = 0x80 ('*'), P_SINGLE = 0x81 ('?'), etc.

```c
#ifndef DOS_DOSASL_H
#define DOS_DOSASL_H
/*
**
**	$VER: dosasl.h 36.16 (2.5.1991)
**	Includes Release 45.1
**
**	Pattern-matching structure definitions
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/

#ifndef EXEC_LIBRARIES_H
#include <exec/libraries.h>
#endif

#ifndef EXEC_LISTS_H
#include <exec/lists.h>
#endif

#ifndef DOS_DOS_H
#include <dos/dos.h>
#endif


/***********************************************************************
************************ PATTERN MATCHING ******************************
************************************************************************

* structure expected by MatchFirst, MatchNext.
* Allocate this structure and initialize it as follows:
*
* Set ap_BreakBits to the signal bits (CDEF) that you want to take a
* break on, or NULL, if you don't want to convenience the user.
*
* If you want to have the FULL PATH NAME of the files you found,
* allocate a buffer at the END of this structure, and put the size of
* it into ap_Strlen.  If you don't want the full path name, make sure
* you set ap_Strlen to zero.  In this case, the name of the file, and stats
* are available in the ap_Info, as per usual.
*
* Then call MatchFirst() and then afterwards, MatchNext() with this structure.
* You should check the return value each time (see below) and take the
* appropriate action, ultimately calling MatchEnd() when there are
* no more files and you are done.  You can tell when you are done by
* checking for the normal AmigaDOS return code ERROR_NO_MORE_ENTRIES.
*
*/

struct AnchorPath {
	struct AChain	*ap_Base;	/* pointer to first anchor */
#define	ap_First ap_Base
	struct AChain	*ap_Last;	/* pointer to last anchor */
#define ap_Current ap_Last
	LONG	ap_BreakBits;	/* Bits we want to break on */
	LONG	ap_FoundBreak;	/* Bits we broke on. Also returns ERROR_BREAK */
	BYTE	ap_Flags;	/* New use for extra word. */
	BYTE	ap_Reserved;
	WORD	ap_Strlen;	/* This is what ap_Length used to be */
#define	ap_Length ap_Flags	/* Old compatability for LONGWORD ap_Length */
	struct	FileInfoBlock ap_Info;
	UBYTE	ap_Buf[1];	/* Buffer for path name, allocated by user */
	/* FIX! */
};


#define	APB_DOWILD	0	/* User option ALL */
#define APF_DOWILD	1

#define	APB_ITSWILD	1	/* Set by MatchFirst, used by MatchNext	 */
#define APF_ITSWILD	2	/* Application can test APB_ITSWILD, too */
				/* (means that there's a wildcard	 */
				/* in the pattern after calling		 */
				/* MatchFirst).				 */

#define	APB_DODIR	2	/* Bit is SET if a DIR node should be */
#define APF_DODIR	4	/* entered. Application can RESET this */
				/* bit after MatchFirst/MatchNext to AVOID */
				/* entering a dir. */

#define	APB_DIDDIR	3	/* Bit is SET for an "expired" dir node. */
#define APF_DIDDIR	8

#define	APB_NOMEMERR	4	/* Set on memory error */
#define APF_NOMEMERR	16

#define	APB_DODOT	5	/* If set, allow conversion of '.' to */
#define APF_DODOT	32	/* CurrentDir */

#define APB_DirChanged	6	/* ap_Current->an_Lock changed */
#define APF_DirChanged	64	/* since last MatchNext call */

#define APB_FollowHLinks 7	/* follow hardlinks on DODIR - defaults   */
#define APF_FollowHLinks 128	/* to not following hardlinks on a DODIR. */


struct AChain {
	struct AChain *an_Child;
	struct AChain *an_Parent;
	BPTR	an_Lock;
	struct FileInfoBlock an_Info;
	BYTE	an_Flags;
	UBYTE	an_String[1];	/* FIX!! */
};

#define	DDB_PatternBit	0
#define	DDF_PatternBit	1
#define	DDB_ExaminedBit	1
#define	DDF_ExaminedBit	2
#define	DDB_Completed	2
#define	DDF_Completed	4
#define	DDB_AllBit	3
#define	DDF_AllBit	8
#define	DDB_Single	4
#define	DDF_Single	16

/*
 * Constants used by wildcard routines, these are the pre-parsed tokens
 * referred to by pattern match.  It is not necessary for you to do
 * anything about these, MatchFirst() MatchNext() handle all these for you.
 */

#define P_ANY		0x80	/* Token for '*' or '#?  */
#define P_SINGLE	0x81	/* Token for '?' */
#define P_ORSTART	0x82	/* Token for '(' */
#define P_ORNEXT	0x83	/* Token for '|' */
#define P_OREND	0x84	/* Token for ')' */
#define P_NOT		0x85	/* Token for '~' */
#define P_NOTEND	0x86	/* Token for */
#define P_NOTCLASS	0x87	/* Token for '^' */
#define P_CLASS	0x88	/* Token for '[]' */
#define P_REPBEG	0x89	/* Token for '[' */
#define P_REPEND	0x8A	/* Token for ']' */
#define P_STOP		0x8B	/* token to force end of evaluation */

/* Values for an_Status, NOTE: These are the actual bit numbers */

#define COMPLEX_BIT	1	/* Parsing complex pattern */
#define EXAMINE_BIT	2	/* Searching directory */

/*
 * Returns from MatchFirst(), MatchNext()
 * You can also get dos error returns, such as ERROR_NO_MORE_ENTRIES,
 * these are in the dos.h file.
 */

#define ERROR_BUFFER_OVERFLOW	303	/* User or internal buffer overflow */
#define ERROR_BREAK		304	/* A break character was received */
#define ERROR_NOT_EXECUTABLE	305	/* A file has E bit cleared */

#endif /* DOS_DOSASL_H */
```

## 6.9. dos/datetime.h — DateTime, FORMAT_*

// Source: NDK_3.9/Include/include_h/dos/datetime.h
// DatetoStr/StrtoDate date formatting.

```c
#ifndef DOS_DATETIME_H
#define DOS_DATETIME_H

/*
**	$VER: datetime.h 45.1 (17.12.2001)
**	Includes Release 45.1
**
**	Date and time C header for AmigaDOS
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/

#ifndef DOS_DOS_H
#include <dos/dos.h>
#endif

/*
 *	Data structures and equates used by the V1.4 DOS functions
 * StrtoDate() and DatetoStr()
 */

/*--------- String/Date structures etc */
struct DateTime {
	struct DateStamp dat_Stamp;	/* DOS DateStamp */
	UBYTE	dat_Format;		/* controls appearance of dat_StrDate */
	UBYTE	dat_Flags;		/* see BITDEF's below */
	UBYTE	*dat_StrDay;		/* day of the week string */
	UBYTE	*dat_StrDate;		/* date string */
	UBYTE	*dat_StrTime;		/* time string */
};

/* You need this much room for each of the DateTime strings: */
#define	LEN_DATSTRING	16

/*	flags for dat_Flags */

#define DTB_SUBST	0		/* substitute Today, Tomorrow, etc. */
#define DTF_SUBST	1
#define DTB_FUTURE	1		/* day of the week is in future */
#define DTF_FUTURE	2

/*
 *	date format values
 */

#define FORMAT_DOS	0		/* dd-mmm-yy */
#define FORMAT_INT	1		/* yy-mm-dd  */
#define FORMAT_USA	2		/* mm-dd-yy  */
#define FORMAT_CDN	3		/* dd-mm-yy  */
#define FORMAT_MAX	FORMAT_CDN
#define FORMAT_DEF	4		/* use default format, as defined
					   by locale; if locale not
					   available, use FORMAT_DOS
					   instead */

#endif /* DOS_DATETIME_H */
```

## 6.10. dos/var.h — LocalVar, GVF_* flags

// Source: NDK_3.9/Include/include_h/dos/var.h
// Local (shell) and global (env:) DOS variables.

```c
#ifndef DOS_VAR_H
#define DOS_VAR_H
/*
**
**	$VER: var.h 36.11 (2.6.1992)
**	Includes Release 45.1
**
**	include file for dos local and environment variables
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/


#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif

/* the structure in the pr_LocalVars list */
/* Do NOT allocate yourself, use SetVar()!!! This structure may grow in */
/* future releases!  The list should be left in alphabetical order, and */
/* may have multiple entries with the same name but different types.	*/

struct LocalVar {
	struct Node lv_Node;
	UWORD	lv_Flags;
	UBYTE	*lv_Value;
	ULONG	lv_Len;
};

/*
 * The lv_Flags bits are available to the application.	The unused
 * lv_Node.ln_Pri bits are reserved for system use.
 */

/* bit definitions for lv_Node.ln_Type: */
#define LV_VAR			0	/* an variable */
#define LV_ALIAS		1	/* an alias */
/* to be or'ed into type: */
#define LVB_IGNORE		7	/* ignore this entry on GetVar, etc */
#define LVF_IGNORE		0x80

/* definitions of flags passed to GetVar()/SetVar()/DeleteVar() */
/* bit defs to be OR'ed with the type: */
/* item will be treated as a single line of text unless BINARY_VAR is used */
#define GVB_GLOBAL_ONLY		8
#define GVF_GLOBAL_ONLY		0x100
#define GVB_LOCAL_ONLY		9
#define GVF_LOCAL_ONLY		0x200
#define GVB_BINARY_VAR		10		/* treat variable as binary */
#define GVF_BINARY_VAR		0x400
#define GVB_DONT_NULL_TERM	11	/* only with GVF_BINARY_VAR */
#define GVF_DONT_NULL_TERM	0x800

/* this is only supported in >= V39 dos.  V37 dos ignores this. */
/* this causes SetVar to affect ENVARC: as well as ENV:.	*/
#define GVB_SAVE_VAR		12	/* only with GVF_GLOBAL_VAR */
#define GVF_SAVE_VAR		0x1000

#endif /* DOS_VAR_H */
```

## 6.11. dos/dostags.h — SYS_*, NP_*, ADO_* tags

// Source: NDK_3.9/Include/include_h/dos/dostags.h
// Tags for System(), CreateNewProc(), AllocDosObject().

```c
#ifndef DOS_DOSTAGS_H
#define DOS_DOSTAGS_H
/*
**
**	$VER: dostags.h 36.11 (29.4.1991)
**	Includes Release 45.1
**
**	Tag definitions for all Dos routines using tags
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/

#ifndef UTILITY_TAGITEM_H
#include <utility/tagitem.h>
#endif

/*****************************************************************************/
/* definitions for the System() call */

#define SYS_Dummy	(TAG_USER + 32)
#define	SYS_Input	(SYS_Dummy + 1)
				/* specifies the input filehandle  */
#define	SYS_Output	(SYS_Dummy + 2)
				/* specifies the output filehandle */
#define	SYS_Asynch	(SYS_Dummy + 3)
				/* run asynch, close input/output on exit(!) */
#define	SYS_UserShell	(SYS_Dummy + 4)
				/* send to user shell instead of boot shell */
#define	SYS_CustomShell	(SYS_Dummy + 5)
				/* send to a specific shell (data is name) */
/*	SYS_Error, */


/*****************************************************************************/
/* definitions for the CreateNewProc() call */
/* you MUST specify one of NP_Seglist or NP_Entry.  All else is optional. */

#define	NP_Dummy (TAG_USER + 1000)
#define	NP_Seglist	(NP_Dummy + 1)
				/* seglist of code to run for the process  */
#define	NP_FreeSeglist	(NP_Dummy + 2)
				/* free seglist on exit - only valid for   */
				/* for NP_Seglist.  Default is TRUE.	   */
#define	NP_Entry	(NP_Dummy + 3)
				/* entry point to run - mutually exclusive */
				/* with NP_Seglist! */
#define	NP_Input	(NP_Dummy + 4)
				/* filehandle - default is Open("NIL:"...) */
#define	NP_Output	(NP_Dummy + 5)
				/* filehandle - default is Open("NIL:"...) */
#define	NP_CloseInput	(NP_Dummy + 6)
				/* close input filehandle on exit	   */
				/* default TRUE				   */
#define	NP_CloseOutput	(NP_Dummy + 7)
				/* close output filehandle on exit	   */
				/* default TRUE				   */
#define	NP_Error	(NP_Dummy + 8)
				/* filehandle - default is Open("NIL:"...) */
#define	NP_CloseError	(NP_Dummy + 9)
				/* close error filehandle on exit	   */
				/* default TRUE				   */
#define	NP_CurrentDir	(NP_Dummy + 10)
				/* lock - default is parent's current dir  */
#define	NP_StackSize	(NP_Dummy + 11)
				/* stacksize for process - default 4000    */
#define	NP_Name		(NP_Dummy + 12)
				/* name for process - default "New Process"*/
#define	NP_Priority	(NP_Dummy + 13)
				/* priority - default same as parent	   */
#define	NP_ConsoleTask	(NP_Dummy + 14)
				/* consoletask - default same as parent    */
#define	NP_WindowPtr	(NP_Dummy + 15)
				/* window ptr - default is same as parent  */
#define	NP_HomeDir	(NP_Dummy + 16)
				/* home directory - default curr home dir  */
#define	NP_CopyVars	(NP_Dummy + 17)
				/* boolean to copy local vars-default TRUE */
#define	NP_Cli		(NP_Dummy + 18)
				/* create cli structure - default FALSE    */
#define	NP_Path		(NP_Dummy + 19)
				/* path - default is copy of parents path  */
				/* only valid if a cli process!	   */
#define	NP_CommandName	(NP_Dummy + 20)
				/* commandname - valid only for CLI	   */
#define	NP_Arguments	(NP_Dummy + 21)
/* cstring of arguments - passed with str in a0, length in d0.	*/
/* (copied and freed on exit.)	Default is 0-length NULL ptr.	*/
/* NOTE: not operational until V37 - see BIX/TechNotes for	*/
/* more info/workaround.  In V36, the registers were random.	*/
/* You must NEVER use NP_Arguments with a NP_Input of NULL.	*/

/* FIX! should this be only for cli's? */
#define	NP_NotifyOnDeath (NP_Dummy + 22)
				/* notify parent on death - default FALSE  */
				/* Not functional yet. */
#define	NP_Synchronous	(NP_Dummy + 23)
				/* don't return until process finishes -   */
				/* default FALSE.			   */
				/* Not functional yet. */
#define	NP_ExitCode	(NP_Dummy + 24)
				/* code to be called on process exit	   */
#define	NP_ExitData	(NP_Dummy + 25)
				/* optional argument for NP_EndCode rtn -  */
				/* default NULL				   */


/*****************************************************************************/
/* tags for AllocDosObject */

#define ADO_Dummy	(TAG_USER + 2000)
#define	ADO_FH_Mode	(ADO_Dummy + 1)
				/* for type DOS_FILEHANDLE only		   */
				/* sets up FH for mode specified.
				   This can make a big difference for buffered
				   files.				   */
	/* The following are for DOS_CLI */
	/* If you do not specify these, dos will use it's preferred values */
	/* which may change from release to release.  The BPTRs to these   */
	/* will be set up correctly for you.  Everything will be zero,	   */
	/* except cli_FailLevel (10) and cli_Background (DOSTRUE).	   */
	/* NOTE: you may also use these 4 tags with CreateNewProc.	   */

#define	ADO_DirLen	(ADO_Dummy + 2)
				/* size in bytes for current dir buffer    */
#define	ADO_CommNameLen	(ADO_Dummy + 3)
				/* size in bytes for command name buffer   */
#define	ADO_CommFileLen	(ADO_Dummy + 4)
				/* size in bytes for command file buffer   */
#define	ADO_PromptLen	(ADO_Dummy + 5)
				/* size in bytes for the prompt buffer	   */

/*****************************************************************************/
/* tags for NewLoadSeg */
/* no tags are defined yet for NewLoadSeg */

#endif /* DOS_DOSTAGS_H */
```

## 6.12. dos/doshunks.h — HUNK_* executable format

// Source: NDK_3.9/Include/include_h/dos/doshunks.h
// Amiga executable file format. HUNK_HEADER = 1011, HUNK_CODE = 1001, HUNK_RELOC32 = 1004, HUNK_END = 1010.

```c
#ifndef DOS_DOSHUNKS_H
#define DOS_DOSHUNKS_H
/*
**	$VER: doshunks.h 36.9 (2.6.1992)
**	Includes Release 45.1
**
**	Hunk definitions for object and load modules.
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
*/

/* hunk types */
#define HUNK_UNIT	999
#define HUNK_NAME	1000
#define HUNK_CODE	1001
#define HUNK_DATA	1002
#define HUNK_BSS	1003
#define HUNK_RELOC32	1004
#define HUNK_ABSRELOC32	HUNK_RELOC32
#define HUNK_RELOC16	1005
#define HUNK_RELRELOC16	HUNK_RELOC16
#define HUNK_RELOC8	1006
#define HUNK_RELRELOC8	HUNK_RELOC8
#define HUNK_EXT	1007
#define HUNK_SYMBOL	1008
#define HUNK_DEBUG	1009
#define HUNK_END	1010
#define HUNK_HEADER	1011

#define HUNK_OVERLAY	1013
#define HUNK_BREAK	1014

#define HUNK_DREL32	1015
#define HUNK_DREL16	1016
#define HUNK_DREL8	1017

#define HUNK_LIB	1018
#define HUNK_INDEX	1019

/*
 * Note: V37 LoadSeg uses 1015 (HUNK_DREL32) by mistake.  This will continue
 * to be supported in future versions, since HUNK_DREL32 is illegal in load files
 * anyways.  Future versions will support both 1015 and 1020, though anything
 * that should be usable under V37 should use 1015.
 */
#define HUNK_RELOC32SHORT 1020

/* see ext_xxx below.  New for V39 (note that LoadSeg only handles RELRELOC32).*/
#define HUNK_RELRELOC32	1021
#define HUNK_ABSRELOC16	1022

/*
 * Any hunks that have the HUNKB_ADVISORY bit set will be ignored if they
 * aren't understood.  When ignored, they're treated like HUNK_DEBUG hunks.
 * NOTE: this handling of HUNKB_ADVISORY started as of V39 dos.library!  If
 * lading such executables is attempted under <V39 dos, it will fail with a
 * bad hunk type.
 */
#define HUNKB_ADVISORY	29
#define HUNKB_CHIP	30
#define HUNKB_FAST	31
#define HUNKF_ADVISORY	(1L<<29)
#define HUNKF_CHIP	(1L<<30)
#define HUNKF_FAST	(1L<<31)


/* hunk_ext sub-types */
#define EXT_SYMB	0	/* symbol table */
#define EXT_DEF		1	/* relocatable definition */
#define EXT_ABS		2	/* Absolute definition */
#define EXT_RES		3	/* no longer supported */
#define EXT_REF32	129	/* 32 bit absolute reference to symbol */
#define EXT_ABSREF32	EXT_REF32
#define EXT_COMMON	130	/* 32 bit absolute reference to COMMON block */
#define EXT_ABSCOMMON	EXT_COMMON
#define EXT_REF16	131	/* 16 bit PC-relative reference to symbol */
#define EXT_RELREF16	EXT_REF16
#define EXT_REF8	132	/*  8 bit PC-relative reference to symbol */
#define EXT_RELREF8	EXT_REF8
#define EXT_DEXT32	133	/* 32 bit data relative reference */
#define EXT_DEXT16	134	/* 16 bit data relative reference */
#define EXT_DEXT8	135	/*  8 bit data relative reference */

/* These are to support some of the '020 and up modes that are rarely used */
#define EXT_RELREF32	136	/* 32 bit PC-relative reference to symbol */
#define EXT_RELCOMMON	137	/* 32 bit PC-relative reference to COMMON block */

/* for completeness... All 680x0's support this */
#define EXT_ABSREF16	138	/* 16 bit absolute reference to symbol */

/* this only exists on '020's and above, in the (d8,An,Xn) address mode */
#define EXT_ABSREF8	139	/* 8 bit absolute reference to symbol */

#endif	/* DOS_DOSHUNKS_H */
```

## 6.13. dos/stdio.h — BUF_* buffering modes

// Source: NDK_3.9/Include/include_h/dos/stdio.h
// ANSI-like buffered I/O helpers on top of FGetC/FPutC.

```c
#ifndef DOS_STDIO_H
#define DOS_STDIO_H
/*
**
**	$VER: stdio.h 36.6 (1.11.1991)
**	Includes Release 45.1
**
**	ANSI-like stdio defines for dos buffered I/O
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/

#define ReadChar()		FGetC(Input())
#define WriteChar(c)		FPutC(Output(),(c))
#define UnReadChar(c)		UnGetC(Input(),(c))
/* next one is inefficient */
#define ReadChars(buf,num)	FRead(Input(),(buf),1,(num))
#define ReadLn(buf,len)		FGets(Input(),(buf),(len))
#define WriteStr(s)		FPuts(Output(),(s))
#define VWritef(format,argv)	VFWritef(Output(),(format),(argv))

/* types for SetVBuf */
#define BUF_LINE	0	/* flush on \n, etc */
#define BUF_FULL	1	/* never flush except when needed */
#define BUF_NONE	2	/* no buffering */

/* EOF return value */
#define ENDSTREAMCH	-1

#endif	/* DOS_STDIO_H */
```

# 7. Graphics structs

Cross-reference: `amiga-graphics-display.md` for View/ViewPort/RastPort semantics.

## 7.1. graphics/gfx.h — BitMap, Rectangle, Rect32, Point, BMF_* flags

// Source: NDK_3.9/Include/include_h/graphics/gfx.h
// BitMap holds 8 plane pointers. RASSIZE(w,h) computes plane byte size. BMF_INTERLEAVED means all planes share one allocation.

```c
#ifndef	GRAPHICS_GFX_H
#define	GRAPHICS_GFX_H
/*
**	$VER: gfx.h 39.5 (19.3.1992)
**	Includes Release 45.1
**
**	general include file for application programs
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#define BITSET	0x8000
#define BITCLR	0

#define AGNUS
#ifdef AGNUS
#define TOBB(a)      ((long)(a))
#else
#define TOBB(a)      ((long)(a)>>1)  /* convert Chip adr to Bread Board Adr */
#endif

struct Rectangle
{
    WORD   MinX,MinY;
    WORD   MaxX,MaxY;
};

struct Rect32
{
    LONG    MinX,MinY;
    LONG    MaxX,MaxY;
};

typedef struct tPoint
{
    WORD x,y;
} Point;

typedef UBYTE *PLANEPTR;

struct BitMap
{
    UWORD   BytesPerRow;
    UWORD   Rows;
    UBYTE   Flags;
    UBYTE   Depth;
    UWORD   pad;
    PLANEPTR Planes[8];
};

/* This macro is obsolete as of V39. AllocBitMap() should be used for allocating
   bitmap data, since it knows about the machine's particular alignment
   restrictions.
*/
#define RASSIZE(w,h)	((ULONG)(h)*( ((ULONG)(w)+15)>>3&0xFFFE))

/* flags for AllocBitMap, etc. */
#define BMB_CLEAR 0
#define BMB_DISPLAYABLE 1
#define BMB_INTERLEAVED 2
#define BMB_STANDARD 3
#define BMB_MINPLANES 4

#define BMF_CLEAR (1l<<BMB_CLEAR)
#define BMF_DISPLAYABLE (1l<<BMB_DISPLAYABLE)
#define BMF_INTERLEAVED (1l<<BMB_INTERLEAVED)
#define BMF_STANDARD (1l<<BMB_STANDARD)
#define BMF_MINPLANES (1l<<BMB_MINPLANES)

/* the following are for GetBitMapAttr() */
#define BMA_HEIGHT 0
#define BMA_DEPTH 4
#define BMA_WIDTH 8
#define BMA_FLAGS 12

#endif	/* GRAPHICS_GFX_H */
```

## 7.2. graphics/gfxnodes.h — ExtendedNode, subsystem/subtype codes

// Source: NDK_3.9/Include/include_h/graphics/gfxnodes.h
// Extended list node used throughout graphics.library for MonitorSpec, ViewExtra, ViewPortExtra.

```c
#ifndef	GRAPHICS_GFXNODES_H
#define	GRAPHICS_GFXNODES_H
/*
**	$VER: gfxnodes.h 39.0 (21.8.1991)
**	Includes Release 45.1
**
**	graphics extended node definintions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif

struct	ExtendedNode	{
	struct	Node	*xln_Succ;
	struct	Node	*xln_Pred;
	UBYTE	xln_Type;
	BYTE	xln_Pri;
	char	*xln_Name;
	UBYTE	xln_Subsystem;
	UBYTE	xln_Subtype;
	LONG	xln_Library;
	LONG	(*xln_Init)();
};

#define SS_GRAPHICS	0x02

#define	VIEW_EXTRA_TYPE		1
#define	VIEWPORT_EXTRA_TYPE	2
#define	SPECIAL_MONITOR_TYPE	3
#define	MONITOR_SPEC_TYPE	4

#endif	/* GRAPHICS_GFXNODES_H */
```

## 7.3. graphics/view.h — View, ViewPort, ColorMap, RasInfo, DBufInfo

// Source: NDK_3.9/Include/include_h/graphics/view.h
// The View is the display root. ViewPort modes include HAM, LACE, DUALPF, EXTRA_HALFBRITE. Also ECS_SPECIFIC beamcon register bits.

```c
#ifndef GRAPHICS_VIEW_H
#define GRAPHICS_VIEW_H
/*
**	$VER: view.h 39.34 (31.5.1993)
**	Includes Release 45.1
**
**	graphics view/viewport definintions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#define ECS_SPECIFIC

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef EXEC_SEMAPHORES_H
#include <exec/semaphores.h>
#endif

#ifndef GRAPHICS_GFX_H
#include <graphics/gfx.h>
#endif

#ifndef GRAPHICS_COPPER_H
#include <graphics/copper.h>
#endif

#ifndef GRAPHICS_GFXNODES_H
#include <graphics/gfxnodes.h>
#endif

#ifndef GRAPHICS_MONITOR_H
#include <graphics/monitor.h>
#endif

#ifndef GRAPHICS_DISPLAYINFO_H
#include <graphics/displayinfo.h>
#endif

#ifndef HARDWARE_CUSTOM_H
#include <hardware/custom.h>
#endif

struct ViewPort
{
	struct	ViewPort *Next;
	struct	ColorMap *ColorMap;	/* table of colors for this viewport */
					/* if this is nil, MakeVPort assumes default values */
	struct	CopList  *DspIns;	/* used by MakeVPort() */
	struct	CopList  *SprIns;	/* used by sprite stuff */
	struct	CopList  *ClrIns;	/* used by sprite stuff */
	struct	UCopList *UCopIns;	/* User copper list */
	WORD	DWidth,DHeight;
	WORD	DxOffset,DyOffset;
	UWORD	Modes;
	UBYTE	SpritePriorities;
	UBYTE	ExtendedModes;
	struct	RasInfo *RasInfo;
};

struct View
{
	struct	ViewPort *ViewPort;
	struct	cprlist *LOFCprList;   /* used for interlaced and noninterlaced */
	struct	cprlist *SHFCprList;   /* only used during interlace */
	WORD	DyOffset,DxOffset;   /* for complete View positioning */
				   /* offsets are +- adjustments to standard #s */
	UWORD	Modes;		   /* such as INTERLACE, GENLOC */
};

/* these structures are obtained via GfxNew */
/* and disposed by GfxFree */
struct ViewExtra
{
	struct ExtendedNode n;
	struct View *View;		/* backwards link */
	struct MonitorSpec *Monitor;	/* monitors for this view */
	UWORD TopLine;
};

/* this structure is obtained via GfxNew */
/* and disposed by GfxFree */
struct ViewPortExtra
{
	struct ExtendedNode n;
	struct ViewPort *ViewPort;	/* backwards link */
	struct Rectangle DisplayClip;	/* MakeVPort display clipping information */
	/* These are added for V39 */
	APTR   VecTable;		/* Private */
	APTR   DriverData[2];
	UWORD  Flags;
	Point  Origin[2];		/* First visible point relative to the DClip.
					 * One for each possible playfield.
					 */
	ULONG cop1ptr;			/* private */
	ULONG cop2ptr;			/* private */
};

/* All these VPXF_ flags are private */
#define VPXB_FREE_ME		0
#define VPXF_FREE_ME		(1 << VPXB_FREE_ME)
#define VPXB_LAST		1
#define VPXF_LAST		(1 << VPXB_LAST)
#define VPXB_STRADDLES_256	4
#define VPXF_STRADDLES_256	(1 << VPXB_STRADDLES_256)
#define VPXB_STRADDLES_512	5
#define VPXF_STRADDLES_512	(1 << VPXB_STRADDLES_512)


#define EXTEND_VSTRUCT	0x1000	/* unused bit in Modes field of View */

#define VPF_A2024	      0x40	/* VP?_ fields internal only */
#define VPF_TENHZ	      0x20
#define VPB_A2024	      6
#define VPB_TENHZ	      4

/* defines used for Modes in IVPargs */

#define GENLOCK_VIDEO	0x0002
#define LACE		0x0004
#define DOUBLESCAN	0x0008
#define SUPERHIRES	0x0020
#define PFBA		0x0040
#define EXTRA_HALFBRITE 0x0080
#define GENLOCK_AUDIO	0x0100
#define DUALPF		0x0400
#define HAM		0x0800
#define EXTENDED_MODE	0x1000
#define VP_HIDE	0x2000
#define SPRITES	0x4000
#define HIRES		0x8000

struct RasInfo	/* used by callers to and InitDspC() */
{
   struct   RasInfo *Next;	    /* used for dualpf */
   struct   BitMap *BitMap;
   WORD    RxOffset,RyOffset;	   /* scroll offsets in this BitMap */
};

struct ColorMap
{
	UBYTE	Flags;
	UBYTE	Type;
	UWORD	Count;
	APTR	ColorTable;
	struct	ViewPortExtra *cm_vpe;
	APTR	LowColorBits;
	UBYTE	TransparencyPlane;
	UBYTE	SpriteResolution;
	UBYTE	SpriteResDefault;	/* what resolution you get when you have set SPRITERESN_DEFAULT */
	UBYTE	AuxFlags;
	struct	ViewPort *cm_vp;
	APTR	NormalDisplayInfo;
	APTR	CoerceDisplayInfo;
	struct	TagItem *cm_batch_items;
	ULONG	VPModeID;
	struct	PaletteExtra *PalExtra;
	UWORD	SpriteBase_Even;
	UWORD	SpriteBase_Odd;
	UWORD	Bp_0_base;
	UWORD	Bp_1_base;

};

/* if Type == 0 then ColorMap is V1.2/V1.3 compatible */
/* if Type != 0 then ColorMap is V38	   compatible */
/* the system will never create other than V39 type colormaps when running V39 */

#define COLORMAP_TYPE_V1_2	0x00
#define COLORMAP_TYPE_V1_4	0x01
#define COLORMAP_TYPE_V36 COLORMAP_TYPE_V1_4	/* use this definition */
#define COLORMAP_TYPE_V39	0x02

/* Flags variable */
#define COLORMAP_TRANSPARENCY	0x01
#define COLORPLANE_TRANSPARENCY	0x02
#define BORDER_BLANKING		0x04
#define BORDER_NOTRANSPARENCY	0x08
#define VIDEOCONTROL_BATCH	0x10
#define USER_COPPER_CLIP	0x20
#define BORDERSPRITES	0x40

#define CMF_CMTRANS	0
#define CMF_CPTRANS	1
#define CMF_BRDRBLNK	2
#define CMF_BRDNTRAN	3
#define CMF_BRDRSPRT	6

#define SPRITERESN_ECS		0
/* ^140ns, except in 35ns viewport, where it is 70ns. */
#define SPRITERESN_140NS	1
#define SPRITERESN_70NS		2
#define SPRITERESN_35NS		3
#define SPRITERESN_DEFAULT	-1

/* AuxFlags : */
#define CMAB_FULLPALETTE 0
#define CMAF_FULLPALETTE (1<<CMAB_FULLPALETTE)
#define CMAB_NO_INTERMED_UPDATE 1
#define CMAF_NO_INTERMED_UPDATE (1<<CMAB_NO_INTERMED_UPDATE)
#define CMAB_NO_COLOR_LOAD 2
#define CMAF_NO_COLOR_LOAD (1 << CMAB_NO_COLOR_LOAD)
#define CMAB_DUALPF_DISABLE 3
#define CMAF_DUALPF_DISABLE (1 << CMAB_DUALPF_DISABLE)


struct PaletteExtra				/* structure may be extended so watch out! */
{
	struct SignalSemaphore pe_Semaphore;		/* shared semaphore for arbitration	*/
	UWORD	pe_FirstFree;				/* *private*				*/
	UWORD	pe_NFree;				/* number of free colors		*/
	UWORD	pe_FirstShared;				/* *private*				*/
	UWORD	pe_NShared;				/* *private*				*/
	UBYTE	*pe_RefCnt;				/* *private*				*/
	UBYTE	*pe_AllocList;				/* *private*				*/
	struct ViewPort *pe_ViewPort;			/* back pointer to viewport		*/
	UWORD	pe_SharableColors;			/* the number of sharable colors.	*/
};

/* flags values for ObtainPen */

#define PENB_EXCLUSIVE 0
#define PENB_NO_SETCOLOR 1

#define PENF_EXCLUSIVE (1l<<PENB_EXCLUSIVE)
#define PENF_NO_SETCOLOR (1l<<PENB_NO_SETCOLOR)

/* obsolete names for PENF_xxx flags: */

#define PEN_EXCLUSIVE PENF_EXCLUSIVE
#define PEN_NO_SETCOLOR PENF_NO_SETCOLOR

/* precision values for ObtainBestPen : */

#define PRECISION_EXACT	-1
#define PRECISION_IMAGE	0
#define PRECISION_ICON	16
#define PRECISION_GUI	32


/* tags for ObtainBestPen: */
#define OBP_Precision 0x84000000
#define OBP_FailIfBad 0x84000001

/* From V39, MakeVPort() will return an error if there is not enough memory,
 * or the requested mode cannot be opened with the requested depth with the
 * given bitmap (for higher bandwidth alignments).
 */

#define MVP_OK		0	/* you want to see this one */
#define MVP_NO_MEM	1	/* insufficient memory for intermediate workspace */
#define MVP_NO_VPE	2	/* ViewPort does not have a ViewPortExtra, and
				 * insufficient memory to allocate a temporary one.
				 */
#define MVP_NO_DSPINS	3	/* insufficient memory for intermidiate copper
				 * instructions.
				 */
#define MVP_NO_DISPLAY	4	/* BitMap data is misaligned for this viewport's
				 * mode and depth - see AllocBitMap().
				 */
#define MVP_OFF_BOTTOM	5	/* PRIVATE - you will never see this. */

/* From V39, MrgCop() will return an error if there is not enough memory,
 * or for some reason MrgCop() did not need to make any copper lists.
 */

#define MCOP_OK		0	/* you want to see this one */
#define MCOP_NO_MEM	1	/* insufficient memory to allocate the system
				 * copper lists.
				 */
#define MCOP_NOP	2	/* MrgCop() did not merge any copper lists
				 * (eg, no ViewPorts in the list, or all marked as
				 * hidden).
				 */

struct DBufInfo {
	APTR	dbi_Link1;
	ULONG	dbi_Count1;
	struct Message dbi_SafeMessage;		/* replied to when safe to write to old bitmap */
	APTR dbi_UserData1;			/* first user data */

	APTR	dbi_Link2;
	ULONG	dbi_Count2;
	struct Message dbi_DispMessage;	/* replied to when new bitmap has been displayed at least
							once */
	APTR	dbi_UserData2;			/* second user data */
	ULONG	dbi_MatchLong;
	APTR	dbi_CopPtr1;
	APTR	dbi_CopPtr2;
	APTR	dbi_CopPtr3;
	UWORD	dbi_BeamPos1;
	UWORD	dbi_BeamPos2;
};

#endif	/* GRAPHICS_VIEW_H */
```

## 7.4. graphics/rastport.h — RastPort, AreaInfo, TmpRas, GelsInfo, drawing modes

// Source: NDK_3.9/Include/include_h/graphics/rastport.h
// RastPort is graphics.library's drawing context. Drawing modes JAM1/JAM2/COMPLEMENT/INVERSVID.

```c
#ifndef	GRAPHICS_RASTPORT_H
#define	GRAPHICS_RASTPORT_H
/*
**	$VER: rastport.h 39.0 (21.8.1991)
**	Includes Release 45.1
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef GRAPHICS_GFX_H
#include <graphics/gfx.h>
#endif

struct AreaInfo
{
    WORD   *VctrTbl;	     /* ptr to start of vector table */
    WORD   *VctrPtr;	     /* ptr to current vertex */
    BYTE    *FlagTbl;	      /* ptr to start of vector flag table */
    BYTE    *FlagPtr;	      /* ptrs to areafill flags */
    WORD   Count;	     /* number of vertices in list */
    WORD   MaxCount;	     /* AreaMove/Draw will not allow Count>MaxCount*/
    WORD   FirstX,FirstY;    /* first point for this polygon */
};

struct TmpRas
{
    BYTE *RasPtr;
    LONG Size;
};

/* unoptimized for 32bit alignment of pointers */
struct GelsInfo
{
    BYTE sprRsrvd;	      /* flag of which sprites to reserve from
				 vsprite system */
    UBYTE Flags;	      /* system use */
    struct VSprite *gelHead, *gelTail; /* dummy vSprites for list management*/
    /* pointer to array of 8 WORDS for sprite available lines */
    WORD *nextLine;
    /* pointer to array of 8 pointers for color-last-assigned to vSprites */
    WORD **lastColor;
    struct collTable *collHandler;     /* addresses of collision routines */
    WORD leftmost, rightmost, topmost, bottommost;
    APTR firstBlissObj,lastBlissObj;    /* system use only */
};

struct RastPort
{
    struct  Layer *Layer;
    struct  BitMap   *BitMap;
    UWORD  *AreaPtrn;	     /* ptr to areafill pattern */
    struct  TmpRas *TmpRas;
    struct  AreaInfo *AreaInfo;
    struct  GelsInfo *GelsInfo;
    UBYTE   Mask;	      /* write mask for this raster */
    BYTE    FgPen;	      /* foreground pen for this raster */
    BYTE    BgPen;	      /* background pen  */
    BYTE    AOlPen;	      /* areafill outline pen */
    BYTE    DrawMode;	      /* drawing mode for fill, lines, and text */
    BYTE    AreaPtSz;	      /* 2^n words for areafill pattern */
    BYTE    linpatcnt;	      /* current line drawing pattern preshift */
    BYTE    dummy;
    UWORD  Flags;	     /* miscellaneous control bits */
    UWORD  LinePtrn;	     /* 16 bits for textured lines */
    WORD   cp_x, cp_y;	     /* current pen position */
    UBYTE   minterms[8];
    WORD   PenWidth;
    WORD   PenHeight;
    struct  TextFont *Font;   /* current font address */
    UBYTE   AlgoStyle;	      /* the algorithmically generated style */
    UBYTE   TxFlags;	      /* text specific flags */
    UWORD   TxHeight;	      /* text height */
    UWORD   TxWidth;	      /* text nominal width */
    UWORD   TxBaseline;       /* text baseline */
    WORD    TxSpacing;	      /* text spacing (per character) */
    APTR    *RP_User;
    ULONG   longreserved[2];
#ifndef GFX_RASTPORT_1_2
    UWORD   wordreserved[7];  /* used to be a node */
    UBYTE   reserved[8];      /* for future use */
#endif
};

/* drawing modes */
#define JAM1	    0	      /* jam 1 color into raster */
#define JAM2	    1	      /* jam 2 colors into raster */
#define COMPLEMENT  2	      /* XOR bits into raster */
#define INVERSVID   4	      /* inverse video for drawing modes */

/* these are the flag bits for RastPort flags */
#define FRST_DOT    0x01      /* draw the first dot of this line ? */
#define ONE_DOT     0x02      /* use one dot mode for drawing lines */
#define DBUFFER     0x04      /* flag set when RastPorts
				 are double-buffered */

	     /* only used for bobs */

#define AREAOUTLINE 0x08      /* used by areafiller */
#define NOCROSSFILL 0x20      /* areafills have no crossovers */

/* there is only one style of clipping: raster clipping */
/* this preserves the continuity of jaggies regardless of clip window */
/* When drawing into a RastPort, if the ptr to ClipRect is nil then there */
/* is no clipping done, this is dangerous but useful for speed */

#endif	/* GRAPHICS_RASTPORT_H */
```

## 7.5. graphics/copper.h — CopIns, CopList, UCopList, copinit

// Source: NDK_3.9/Include/include_h/graphics/copper.h
// Copper instruction structures. OpCode 0 = MOVE, 1 = WAIT. Used by CMOVE/CWAIT macros.

```c
#ifndef GRAPHICS_COPPER_H
#define GRAPHICS_COPPER_H
/*
**	$VER: copper.h 39.10 (31.5.1993)
**	Includes Release 45.1
**
**	graphics copper list intstruction definitions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#define COPPER_MOVE 0	    /* pseude opcode for move #XXXX,dir */
#define COPPER_WAIT 1	    /* pseudo opcode for wait y,x */
#define CPRNXTBUF   2	    /* continue processing with next buffer */
#define CPR_NT_LOF  0x8000  /* copper instruction only for short frames */
#define CPR_NT_SHT  0x4000  /* copper instruction only for long frames */
#define CPR_NT_SYS  0x2000  /* copper user instruction only */

struct CopIns
{
    WORD   OpCode; /* 0 = move, 1 = wait */
    union
    {
	    struct CopList *nxtlist;
	    struct
	{
			union
			{
				WORD VWaitPos;	      /* vertical beam wait */
				WORD DestAddr;	      /* destination address of copper move */
			} u1;
			union
			{
				WORD HWaitPos;	      /* horizontal beam wait position */
				WORD DestData;	      /* destination immediate data to send */
			} u2;
		} u4;
    } u3;
};

/* shorthand for above */
#define NXTLIST     u3.nxtlist
#define VWAITPOS    u3.u4.u1.VWaitPos
#define DESTADDR    u3.u4.u1.DestAddr
#define HWAITPOS    u3.u4.u2.HWaitPos
#define DESTDATA    u3.u4.u2.DestData


/* structure of cprlist that points to list that hardware actually executes */
struct cprlist
{
    struct cprlist *Next;
    UWORD   *start;	    /* start of copper list */
    WORD   MaxCount;	   /* number of long instructions */
};

struct CopList
{
    struct  CopList *Next;  /* next block for this copper list */
    struct  CopList *_CopList;	/* system use */
    struct  ViewPort *_ViewPort;    /* system use */
    struct  CopIns *CopIns; /* start of this block */
    struct  CopIns *CopPtr; /* intermediate ptr */
    UWORD   *CopLStart;     /* mrgcop fills this in for Long Frame*/
    UWORD   *CopSStart;     /* mrgcop fills this in for Short Frame*/
    WORD   Count;	   /* intermediate counter */
    WORD   MaxCount;	   /* max # of copins for this block */
    WORD   DyOffset;	   /* offset this copper list vertical waits */
#ifdef V1_3
    UWORD   *Cop2Start;
    UWORD   *Cop3Start;
    UWORD   *Cop4Start;
    UWORD   *Cop5Start;
#endif
    UWORD  SLRepeat;
    UWORD  Flags;
};

/* These CopList->Flags are private */
#define EXACT_LINE 1
#define HALF_LINE 2


struct UCopList
{
    struct UCopList *Next;
    struct CopList  *FirstCopList; /* head node of this copper list */
    struct CopList  *CopList;	   /* node in use */
};

/* Private graphics data structure. This structure has changed in the past,
 * and will continue to change in the future. Do Not Touch!
 */

struct copinit
{
    UWORD vsync_hblank[2];
    UWORD diagstrt[12];      /* copper list for first bitplane */
    UWORD fm0[2];
    UWORD diwstart[10];
    UWORD bplcon2[2];
	UWORD sprfix[2*8];
    UWORD sprstrtup[(2*8*2)];
    UWORD wait14[2];
    UWORD norm_hblank[2];
    UWORD jump[2];
    UWORD wait_forever[6];
    UWORD   sprstop[8];
};

#endif	/* GRAPHICS_COPPER_H */
```

## 7.6. graphics/sprite.h — SimpleSprite, ExtSprite, SPRITEA_* tags

// Source: NDK_3.9/Include/include_h/graphics/sprite.h
// Simple vs extended (AGA) sprites. SPRITE_ATTACHED = 0x80.

```c
#ifndef	GRAPHICS_SPRITE_H
#define	GRAPHICS_SPRITE_H
/*
**	$VER: sprite.h 39.6 (16.6.1992)
**	Includes Release 45.1
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#define SPRITE_ATTACHED 0x80

struct SimpleSprite
{
    UWORD *posctldata;
    UWORD height;
    UWORD   x,y;    /* current position */
    UWORD   num;
};

struct ExtSprite
{
	struct SimpleSprite es_SimpleSprite;	/* conventional simple sprite structure */
	UWORD	es_wordwidth;			/* graphics use only, subject to change */
	UWORD	es_flags;			/* graphics use only, subject to change */
};



/* tags for AllocSpriteData() */
#define SPRITEA_Width		0x81000000
#define SPRITEA_XReplication	0x81000002
#define SPRITEA_YReplication	0x81000004
#define SPRITEA_OutputHeight	0x81000006
#define SPRITEA_Attached	0x81000008
#define SPRITEA_OldDataFormat	0x8100000a	/* MUST pass in outputheight if using this tag */

/* tags for GetExtSprite() */
#define GSTAG_SPRITE_NUM 0x82000020
#define GSTAG_ATTACHED	 0x82000022
#define GSTAG_SOFTSPRITE 0x82000024

/* tags valid for either GetExtSprite or ChangeExtSprite */
#define GSTAG_SCANDOUBLED	0x83000000	/* request "NTSC-Like" height if possible. */

#endif	/* GRAPHICS_SPRITE_H */
```

## 7.7. graphics/text.h — TextAttr, TTextAttr, TextFont, ColorTextFont, FSF_*, FPF_* flags

// Source: NDK_3.9/Include/include_h/graphics/text.h
// Font structures. FSF_BOLD/ITALIC/UNDERLINED/EXTENDED are algorithmic style bits.

```c
#ifndef	GRAPHICS_TEXT_H
#define	GRAPHICS_TEXT_H
/*
**	$VER: text.h 39.0 (21.8.1991)
**	Includes Release 45.1
**
**	graphics library text structures
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	EXEC_PORTS_H
#include	<exec/ports.h>
#endif	/* EXEC_PORTS_H */

#ifndef	GRAPHICS_GFX_H
#include	<graphics/gfx.h>
#endif	/* GRAPHICS_GFX_H */

#ifndef	UTILITY_TAGITEM_H
#include	<utility/tagitem.h>
#endif	/* UTILITY_TAGITEM_H */

/*------ Font Styles ------------------------------------------------*/
#define	FS_NORMAL	0	/* normal text (no style bits set) */
#define	FSB_UNDERLINED	0	/* underlined (under baseline) */
#define	FSF_UNDERLINED	0x01
#define	FSB_BOLD	1	/* bold face text (ORed w/ shifted) */
#define	FSF_BOLD	0x02
#define	FSB_ITALIC	2	/* italic (slanted 1:2 right) */
#define	FSF_ITALIC	0x04
#define	FSB_EXTENDED	3	/* extended face (wider than normal) */
#define	FSF_EXTENDED	0x08

#define	FSB_COLORFONT	6	/* this uses ColorTextFont structure */
#define	FSF_COLORFONT	0x40
#define	FSB_TAGGED	7	/* the TextAttr is really an TTextAttr, */
#define	FSF_TAGGED	0x80

/*------ Font Flags -------------------------------------------------*/
#define	FPB_ROMFONT	0	/* font is in rom */
#define	FPF_ROMFONT	0x01
#define	FPB_DISKFONT	1	/* font is from diskfont.library */
#define	FPF_DISKFONT	0x02
#define	FPB_REVPATH	2	/* designed path is reversed (e.g. left) */
#define	FPF_REVPATH	0x04
#define	FPB_TALLDOT	3	/* designed for hires non-interlaced */
#define	FPF_TALLDOT	0x08
#define	FPB_WIDEDOT	4	/* designed for lores interlaced */
#define	FPF_WIDEDOT	0x10
#define	FPB_PROPORTIONAL 5	/* character sizes can vary from nominal */
#define	FPF_PROPORTIONAL 0x20
#define	FPB_DESIGNED	6	/* size explicitly designed, not constructed */
				/* note: if you do not set this bit in your */
				/* textattr, then a font may be constructed */
				/* for you by scaling an existing rom or disk */
				/* font (under V36 and above). */
#define	FPF_DESIGNED	0x40
    /* bit 7 is always clear for fonts on the graphics font list */
#define	FPB_REMOVED	7	/* the font has been removed */
#define	FPF_REMOVED	(1<<7)

/****** TextAttr node, matches text attributes in RastPort **********/
struct TextAttr {
    STRPTR  ta_Name;		/* name of the font */
    UWORD   ta_YSize;		/* height of the font */
    UBYTE   ta_Style;		/* intrinsic font style */
    UBYTE   ta_Flags;		/* font preferences and flags */
};

struct TTextAttr {
    STRPTR  tta_Name;		/* name of the font */
    UWORD   tta_YSize;		/* height of the font */
    UBYTE   tta_Style;		/* intrinsic font style */
    UBYTE   tta_Flags;		/* font preferences and flags */
    struct TagItem *tta_Tags;	/* extended attributes */
};


/****** Text Tags ***************************************************/
#define	TA_DeviceDPI	(1|TAG_USER)	/* Tag value is Point union: */
					/* Hi word XDPI, Lo word YDPI */

#define	MAXFONTMATCHWEIGHT	32767	/* perfect match from WeighTAMatch */


/****** TextFonts node **********************************************/
struct TextFont {
    struct Message tf_Message;	/* reply message for font removal */
				/* font name in LN	  \    used in this */
    UWORD   tf_YSize;		/* font height		  |    order to best */
    UBYTE   tf_Style;		/* font style		  |    match a font */
    UBYTE   tf_Flags;		/* preferences and flags  /    request. */
    UWORD   tf_XSize;		/* nominal font width */
    UWORD   tf_Baseline;	/* distance from the top of char to baseline */
    UWORD   tf_BoldSmear;	/* smear to affect a bold enhancement */

    UWORD   tf_Accessors;	/* access count */

    UBYTE   tf_LoChar;		/* the first character described here */
    UBYTE   tf_HiChar;		/* the last character described here */
    APTR    tf_CharData;	/* the bit character data */

    UWORD   tf_Modulo;		/* the row modulo for the strike font data */
    APTR    tf_CharLoc;		/* ptr to location data for the strike font */
				/*   2 words: bit offset then size */
    APTR    tf_CharSpace;	/* ptr to words of proportional spacing data */
    APTR    tf_CharKern;	/* ptr to words of kerning data */
};

/* unfortunately, this needs to be explicitly typed */
#define	tf_Extension	tf_Message.mn_ReplyPort

/*-----	tfe_Flags0 (partial definition) ----------------------------*/
#define TE0B_NOREMFONT	0	/* disallow RemFont for this font */
#define TE0F_NOREMFONT	0x01

struct TextFontExtension {	/* this structure is read-only */
    UWORD   tfe_MatchWord;		/* a magic cookie for the extension */
    UBYTE   tfe_Flags0;			/* (system private flags) */
    UBYTE   tfe_Flags1;			/* (system private flags) */
    struct TextFont *tfe_BackPtr;	/* validation of compilation */
    struct MsgPort *tfe_OrigReplyPort;	/* original value in tf_Extension */
    struct TagItem *tfe_Tags;		/* Text Tags for the font */
    UWORD  *tfe_OFontPatchS;		/* (system private use) */
    UWORD  *tfe_OFontPatchK;		/* (system private use) */
    /* this space is reserved for future expansion */
};

/******	ColorTextFont node ******************************************/
/*-----	ctf_Flags --------------------------------------------------*/
#define	CT_COLORMASK	0x000F	/* mask to get to following color styles */
#define	CT_COLORFONT	0x0001	/* color map contains designer's colors */
#define	CT_GREYFONT	0x0002	/* color map describes even-stepped */
				/* brightnesses from low to high */
#define	CT_ANTIALIAS	0x0004	/* zero background thru fully saturated char */

#define	CTB_MAPCOLOR	0	/* map ctf_FgColor to the rp_FgPen if it's */
#define	CTF_MAPCOLOR	0x0001	/* is a valid color within ctf_Low..ctf_High */

/*----- ColorFontColors --------------------------------------------*/
struct ColorFontColors {
    UWORD   cfc_Reserved;	/* *must* be zero */
    UWORD   cfc_Count;		/* number of entries in cfc_ColorTable */
    UWORD  *cfc_ColorTable;	/* 4 bit per component color map packed xRGB */
};

/*-----	ColorTextFont ----------------------------------------------*/
struct ColorTextFont {
    struct TextFont ctf_TF;
    UWORD   ctf_Flags;		/* extended flags */
    UBYTE   ctf_Depth;		/* number of bit planes */
    UBYTE   ctf_FgColor;	/* color that is remapped to FgPen */
    UBYTE   ctf_Low;		/* lowest color represented here */
    UBYTE   ctf_High;		/* highest color represented here */
    UBYTE   ctf_PlanePick;	/* PlanePick ala Images */
    UBYTE   ctf_PlaneOnOff;	/* PlaneOnOff ala Images */
    struct ColorFontColors *ctf_ColorFontColors; /* colors for font */
    APTR    ctf_CharData[8];	/*pointers to bit planes ala tf_CharData */
};

/****** TextExtent node *********************************************/
struct TextExtent {
    UWORD   te_Width;		/* same as TextLength */
    UWORD   te_Height;		/* same as tf_YSize */
    struct Rectangle te_Extent;	/* relative to CP */
};

#endif	/* GRAPHICS_TEXT_H */
```

## 7.8. graphics/clip.h — Layer, ClipRect

// Source: NDK_3.9/Include/include_h/graphics/clip.h
// Layer is the clipping context for a window. ClipRect list describes visible regions.

```c
#ifndef	GRAPHICS_CLIP_H
#define	GRAPHICS_CLIP_H
/*
**	$VER: clip.h 39.0 (2.12.1991)
**	Includes Release 45.1
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef GRAPHICS_GFX_H
#include <graphics/gfx.h>
#endif
#ifndef EXEC_SEMAPHORES_H
#include <exec/semaphores.h>
#endif
#ifndef UTILITY_HOOKS_H
#include <utility/hooks.h>
#endif

#define NEWLOCKS

struct Layer
{
    struct  Layer *front,*back;
    struct  ClipRect	*ClipRect;  /* read by roms to find first cliprect */
    struct  RastPort	*rp;
    struct  Rectangle	bounds;
    UBYTE   reserved[4];
    UWORD   priority;		    /* system use only */
    UWORD   Flags;		    /* obscured ?, Virtual BitMap? */
    struct  BitMap *SuperBitMap;
    struct  ClipRect *SuperClipRect; /* super bitmap cliprects if VBitMap != 0*/
				  /* else damage cliprect list for refresh */
    APTR    Window;		  /* reserved for user interface use */
    WORD    Scroll_X,Scroll_Y;
    struct  ClipRect *cr,*cr2,*crnew;	/* used by dedice */
    struct  ClipRect *SuperSaveClipRects; /* preallocated cr's */
    struct  ClipRect *_cliprects;	/* system use during refresh */
    struct  Layer_Info	*LayerInfo;	/* points to head of the list */
    struct  SignalSemaphore Lock;
    struct  Hook *BackFill;
    ULONG   reserved1;
    struct  Region *ClipRegion;
    struct  Region *saveClipRects;	/* used to back out when in trouble*/
    WORD    Width,Height;		/* system use */
    UBYTE   reserved2[18];
    /* this must stay here */
    struct  Region  *DamageList;    /* list of rectangles to refresh
				       through */
};

struct ClipRect
{
    struct  ClipRect *Next;	    /* roms used to find next ClipRect */
    struct  ClipRect *prev;	    /* Temp use in layers (private) */
    struct  Layer   *lobs;	    /* Private use for layers */
    struct  BitMap  *BitMap;	    /* Bitmap for layers private use */
    struct  Rectangle	bounds;     /* bounds of cliprect */
    void    *_p1;		    /* Layers private use!!! */
    void    *_p2;		    /* Layers private use!!! */
    LONG    reserved;		    /* system use (Layers private) */
#ifdef NEWCLIPRECTS_1_1
    LONG    Flags;		    /* Layers private field for cliprects */
				    /* that layers allocates... */
#endif				    /* MUST be multiple of 8 bytes to buffer */
};

/* internal cliprect flags */
#define CR_NEEDS_NO_CONCEALED_RASTERS  1
#define CR_NEEDS_NO_LAYERBLIT_DAMAGE   2

/* defines for code values for getcode */
#define ISLESSX 1
#define ISLESSY 2
#define ISGRTRX 4
#define ISGRTRY 8

#endif	/* GRAPHICS_CLIP_H */
```

## 7.9. graphics/layers.h — Layer_Info, LAYER* flags

// Source: NDK_3.9/Include/include_h/graphics/layers.h
// LAYERSIMPLE/LAYERSMART/LAYERSUPER are the three refresh modes.

```c
#ifndef	GRAPHICS_LAYERS_H
#define	GRAPHICS_LAYERS_H
/*
**	$VER: layers.h 39.4 (14.4.1992)
**	Includes Release 45.1
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_LISTS_H
#include <exec/lists.h>
#endif

#ifndef EXEC_SEMAPHORES_H
#include <exec/semaphores.h>
#endif

#define LAYERSIMPLE		1
#define LAYERSMART		2
#define LAYERSUPER		4
#define LAYERUPDATING		0x10
#define LAYERBACKDROP		0x40
#define LAYERREFRESH		0x80
#define	LAYERIREFRESH		0x200
#define	LAYERIREFRESH2		0x400
#define LAYER_CLIPRECTS_LOST	0x100	/* during BeginUpdate */
					/* or during layerop */
					/* this happens if out of memory */

struct Layer_Info
{
	struct	Layer		*top_layer;
	struct	Layer		*check_lp;		/* !! Private !! */
	struct	ClipRect	*obs;
	struct	ClipRect	*FreeClipRects;		/* !! Private !! */
		LONG		PrivateReserve1;	/* !! Private !! */
		LONG		PrivateReserve2;	/* !! Private !! */
	struct	SignalSemaphore	Lock;			/* !! Private !! */
	struct	MinList		gs_Head;		/* !! Private !! */
		WORD		PrivateReserve3;	/* !! Private !! */
		VOID		*PrivateReserve4;	/* !! Private !! */
		UWORD		Flags;
		BYTE		fatten_count;		/* !! Private !! */
		BYTE		LockLayersCount;	/* !! Private !! */
		WORD		PrivateReserve5;	/* !! Private !! */
		VOID		*BlankHook;		/* !! Private !! */
		VOID		*LayerInfo_extra;	/* !! Private !! */
};

#define NEWLAYERINFO_CALLED 1

/*
 * LAYERS_NOBACKFILL is the value needed to get no backfill hook
 * LAYERS_BACKFILL is the value needed to get the default backfill hook
 */
#define	LAYERS_NOBACKFILL	((struct Hook *)1)
#define	LAYERS_BACKFILL		((struct Hook *)0)

#endif	/* GRAPHICS_LAYERS_H */
```

## 7.10. graphics/regions.h — Region, RegionRectangle

// Source: NDK_3.9/Include/include_h/graphics/regions.h
// Polygon regions built from rectangles for clipping.

```c
#ifndef	GRAPHICS_REGIONS_H
#define	GRAPHICS_REGIONS_H
/*
**	$VER: regions.h 39.0 (21.8.1991)
**	Includes Release 45.1
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef GRAPHICS_GFX_H
#include <graphics/gfx.h>
#endif

struct RegionRectangle
{
    struct RegionRectangle *Next,*Prev;
    struct Rectangle bounds;
};

struct Region
{
    struct Rectangle bounds;
    struct RegionRectangle *RegionRectangle;
};

#endif	/* GRAPHICS_REGIONS_H */
```

## 7.11. graphics/gels.h — VSprite, Bob, AnimComp, AnimOb, DBufPacket, collTable

// Source: NDK_3.9/Include/include_h/graphics/gels.h
// Graphics Elements — sprites and blitter objects (Bobs). Animation engine.

```c
#ifndef	GRAPHICS_GELS_H
#define	GRAPHICS_GELS_H
/*
**	$VER: gels.h 39.0 (21.8.1991)
**	Includes Release 45.1
**
**	include file for AMIGA GELS (Graphics Elements)
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

/* VSprite flags */
/* user-set VSprite flags: */
#define SUSERFLAGS  0x00FF    /* mask of all user-settable VSprite-flags */
#define VSPRITE     0x0001    /* set if VSprite, clear if Bob */
#define SAVEBACK    0x0002    /* set if background is to be saved/restored */
#define OVERLAY     0x0004    /* set to mask image of Bob onto background */
#define MUSTDRAW    0x0008    /* set if VSprite absolutely must be drawn */
/* system-set VSprite flags: */
#define BACKSAVED   0x0100    /* this Bob's background has been saved */
#define BOBUPDATE   0x0200    /* temporary flag, useless to outside world */
#define GELGONE     0x0400    /* set if gel is completely clipped (offscreen) */
#define VSOVERFLOW  0x0800    /* VSprite overflow (if MUSTDRAW set we draw!) */

/* Bob flags */
/* these are the user flag bits */
#define BUSERFLAGS  0x00FF    /* mask of all user-settable Bob-flags */
#define SAVEBOB     0x0001    /* set to not erase Bob */
#define BOBISCOMP   0x0002    /* set to identify Bob as AnimComp */
/* these are the system flag bits */
#define BWAITING    0x0100    /* set while Bob is waiting on 'after' */
#define BDRAWN	    0x0200    /* set when Bob is drawn this DrawG pass*/
#define BOBSAWAY    0x0400    /* set to initiate removal of Bob */
#define BOBNIX	    0x0800    /* set when Bob is completely removed */
#define SAVEPRESERVE 0x1000   /* for back-restore during double-buffer*/
#define OUTSTEP     0x2000    /* for double-clearing if double-buffer */

/* defines for the animation procedures */
#define ANFRACSIZE  6
#define ANIMHALF    0x0020
#define RINGTRIGGER 0x0001


/* UserStuff definitions
 *  the user can define these to be a single variable or a sub-structure
 *  if undefined by the user, the system turns these into innocuous variables
 *  see the manual for a thorough definition of the UserStuff definitions
 *
 */
#ifndef VUserStuff	      /* VSprite user stuff */
#define VUserStuff WORD
#endif

#ifndef BUserStuff	      /* Bob user stuff */
#define BUserStuff WORD
#endif

#ifndef AUserStuff	      /* AnimOb user stuff */
#define AUserStuff WORD
#endif




/*********************** GEL STRUCTURES ***********************************/

struct VSprite
{
/* --------------------- SYSTEM VARIABLES ------------------------------- */
/* GEL linked list forward/backward pointers sorted by y,x value */
    struct VSprite   *NextVSprite;
    struct VSprite   *PrevVSprite;

/* GEL draw list constructed in the order the Bobs are actually drawn, then
 *  list is copied to clear list
 *  must be here in VSprite for system boundary detection
 */
    struct VSprite   *DrawPath;     /* pointer of overlay drawing */
    struct VSprite   *ClearPath;    /* pointer for overlay clearing */

/* the VSprite positions are defined in (y,x) order to make sorting
 *  sorting easier, since (y,x) as a long integer
 */
    WORD OldY, OldX;	      /* previous position */

/* --------------------- COMMON VARIABLES --------------------------------- */
    WORD Flags;	      /* VSprite flags */


/* --------------------- USER VARIABLES ----------------------------------- */
/* the VSprite positions are defined in (y,x) order to make sorting
 *  sorting easier, since (y,x) as a long integer
 */
    WORD Y, X;		      /* screen position */

    WORD Height;
    WORD Width;	      /* number of words per row of image data */
    WORD Depth;	      /* number of planes of data */

    WORD MeMask;	      /* which types can collide with this VSprite*/
    WORD HitMask;	      /* which types this VSprite can collide with*/

    WORD *ImageData;	      /* pointer to VSprite image */

/* borderLine is the one-dimensional logical OR of all
 *  the VSprite bits, used for fast collision detection of edge
 */
    WORD *BorderLine;	      /* logical OR of all VSprite bits */
    WORD *CollMask;	      /* similar to above except this is a matrix */

/* pointer to this VSprite's color definitions (not used by Bobs) */
    WORD *SprColors;

    struct Bob *VSBob;	      /* points home if this VSprite is part of
				   a Bob */

/* planePick flag:  set bit selects a plane from image, clear bit selects
 *  use of shadow mask for that plane
 * OnOff flag: if using shadow mask to fill plane, this bit (corresponding
 *  to bit in planePick) describes whether to fill with 0's or 1's
 * There are two uses for these flags:
 *	- if this is the VSprite of a Bob, these flags describe how the Bob
 *	  is to be drawn into memory
 *	- if this is a simple VSprite and the user intends on setting the
 *	  MUSTDRAW flag of the VSprite, these flags must be set too to describe
 *	  which color registers the user wants for the image
 */
    BYTE PlanePick;
    BYTE PlaneOnOff;

    VUserStuff VUserExt;      /* user definable:  see note above */
};

struct Bob
/* blitter-objects */
{
/* --------------------- SYSTEM VARIABLES --------------------------------- */

/* --------------------- COMMON VARIABLES --------------------------------- */
    WORD Flags;	/* general purpose flags (see definitions below) */

/* --------------------- USER VARIABLES ----------------------------------- */
    WORD *SaveBuffer;	/* pointer to the buffer for background save */

/* used by Bobs for "cookie-cutting" and multi-plane masking */
    WORD *ImageShadow;

/* pointer to BOBs for sequenced drawing of Bobs
 *  for correct overlaying of multiple component animations
 */
    struct Bob *Before; /* draw this Bob before Bob pointed to by before */
    struct Bob *After;	/* draw this Bob after Bob pointed to by after */

    struct VSprite   *BobVSprite;   /* this Bob's VSprite definition */

    struct AnimComp  *BobComp;	    /* pointer to this Bob's AnimComp def */

    struct DBufPacket *DBuffer;     /* pointer to this Bob's dBuf packet */

    BUserStuff BUserExt;	    /* Bob user extension */
};

struct AnimComp
{
/* --------------------- SYSTEM VARIABLES --------------------------------- */

/* --------------------- COMMON VARIABLES --------------------------------- */
    WORD Flags;		    /* AnimComp flags for system & user */

/* timer defines how long to keep this component active:
 *  if set non-zero, timer decrements to zero then switches to nextSeq
 *  if set to zero, AnimComp never switches
 */
    WORD Timer;

/* --------------------- USER VARIABLES ----------------------------------- */
/* initial value for timer when the AnimComp is activated by the system */
    WORD TimeSet;

/* pointer to next and previous components of animation object */
    struct AnimComp  *NextComp;
    struct AnimComp  *PrevComp;

/* pointer to component component definition of next image in sequence */
    struct AnimComp  *NextSeq;
    struct AnimComp  *PrevSeq;

/* address of special animation procedure */
    WORD (*AnimCRoutine) __CLIB_PROTOTYPE((struct AnimComp *));

    WORD YTrans;     /* initial y translation (if this is a component) */
    WORD XTrans;     /* initial x translation (if this is a component) */

    struct AnimOb    *HeadOb;

    struct Bob	     *AnimBob;
};

struct AnimOb
{
/* --------------------- SYSTEM VARIABLES --------------------------------- */
    struct AnimOb    *NextOb, *PrevOb;

/* number of calls to Animate this AnimOb has endured */
    LONG Clock;

    WORD AnOldY, AnOldX;	    /* old y,x coordinates */

/* --------------------- COMMON VARIABLES --------------------------------- */
    WORD AnY, AnX;		    /* y,x coordinates of the AnimOb */

/* --------------------- USER VARIABLES ----------------------------------- */
    WORD YVel, XVel;		    /* velocities of this object */
    WORD YAccel, XAccel;	    /* accelerations of this object */

    WORD RingYTrans, RingXTrans;    /* ring translation values */

    				    /* address of special animation
				       procedure */
    WORD (*AnimORoutine) __CLIB_PROTOTYPE((struct AnimOb *));

    struct AnimComp  *HeadComp;     /* pointer to first component */

    AUserStuff AUserExt;	    /* AnimOb user extension */
};

/* dBufPacket defines the values needed to be saved across buffer to buffer
 *  when in double-buffer mode
 */
struct DBufPacket
{
    WORD BufY, BufX;		    /* save the other buffers screen coordinates */
    struct VSprite   *BufPath;	    /* carry the draw path over the gap */

/* these pointers must be filled in by the user */
/* pointer to other buffer's background save buffer */
    WORD *BufBuffer;
};



/* ************************************************************************ */

/* these are GEL functions that are currently simple enough to exist as a
 *  definition.  It should not be assumed that this will always be the case
 */
#define InitAnimate(animKey) {*(animKey) = NULL;}
#define RemBob(b) {(b)->Flags |= BOBSAWAY;}


/* ************************************************************************ */

#define B2NORM	    0
#define B2SWAP	    1
#define B2BOBBER    2

/* ************************************************************************ */

/* a structure to contain the 16 collision procedure addresses */
struct collTable
{
    /* NOTE: This table actually consists of two different types of
     *       pointers. The first table entry is for collision testing,
     *       the other are for reporting collisions. The first function
     *       pointer looks like this:
     *
     *          LONG (*collPtrs[0])(struct VSprite *,WORD);
     *
     *       The remaining 15 function pointers look like this:
     *
     *          VOID (*collPtrs[1..15])(struct VSprite *,struct VSprite *);
     */
    LONG (*collPtrs[16]) __CLIB_PROTOTYPE((struct VSprite *,struct VSprite *));
};

#endif	/* GRAPHICS_GELS_H */
```

## 7.12. graphics/monitor.h — MonitorSpec, AnalogSignalInterval, STANDARD_* timings

// Source: NDK_3.9/Include/include_h/graphics/monitor.h
// Monitor specs. SPECIAL_BEAMCON is the ECS programmable-sync value. STANDARD_NTSC_ROWS = 262, STANDARD_PAL_ROWS = 312, STANDARD_COLORCLOCKS = 226.

```c
#ifndef	GRAPHICS_MONITOR_H
#define	GRAPHICS_MONITOR_H
/*
**	$VER: monitor.h 39.7 (9.6.1992)
**	Includes Release 45.1
**
**	graphics monitorspec definintions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	EXEC_SEMAPHORES_H
#include	<exec/semaphores.h>
#endif

#ifndef	GRAPHICS_GFXNODES_H
#include	<graphics/gfxnodes.h>
#endif

#ifndef	GRAPHICS_GFX_H
#include	<graphics/gfx.h>
#endif

struct	MonitorSpec
{
    struct	ExtendedNode	ms_Node;
    UWORD	ms_Flags;
    LONG	ratioh;
    LONG	ratiov;
    UWORD	total_rows;
    UWORD	total_colorclocks;
    UWORD	DeniseMaxDisplayColumn;
    UWORD	BeamCon0;
    UWORD	min_row;
    struct	SpecialMonitor	*ms_Special;
    UWORD	ms_OpenCount;
    LONG	(*ms_transform)();
    LONG	(*ms_translate)();
    LONG	(*ms_scale)();
    UWORD	ms_xoffset;
    UWORD	ms_yoffset;
    struct	Rectangle	ms_LegalView;
    LONG	(*ms_maxoscan)();	/* maximum legal overscan */
    LONG	(*ms_videoscan)();	/* video display overscan */
    UWORD	DeniseMinDisplayColumn;
    ULONG	DisplayCompatible;
    struct	List DisplayInfoDataBase;
    struct	SignalSemaphore DisplayInfoDataBaseSemaphore;
    LONG	(*ms_MrgCop)();
    LONG	(*ms_LoadView)();
    LONG	(*ms_KillView)();
};

#define	TO_MONITOR		0
#define	FROM_MONITOR		1
#define	STANDARD_XOFFSET	9
#define	STANDARD_YOFFSET	0

#define MSB_REQUEST_NTSC	0
#define MSB_REQUEST_PAL		1
#define MSB_REQUEST_SPECIAL	2
#define MSB_REQUEST_A2024	3
#define MSB_DOUBLE_SPRITES	4
#define	MSF_REQUEST_NTSC	(1 << MSB_REQUEST_NTSC)
#define	MSF_REQUEST_PAL		(1 << MSB_REQUEST_PAL)
#define	MSF_REQUEST_SPECIAL		(1 << MSB_REQUEST_SPECIAL)
#define	MSF_REQUEST_A2024		(1 << MSB_REQUEST_A2024)
#define MSF_DOUBLE_SPRITES		(1 << MSB_DOUBLE_SPRITES)


/* obsolete, v37 compatible definitions follow */
#define	REQUEST_NTSC		(1 << MSB_REQUEST_NTSC)
#define	REQUEST_PAL		(1 << MSB_REQUEST_PAL)
#define	REQUEST_SPECIAL		(1 << MSB_REQUEST_SPECIAL)
#define	REQUEST_A2024		(1 << MSB_REQUEST_A2024)

#define	DEFAULT_MONITOR_NAME	"default.monitor"
#define	NTSC_MONITOR_NAME	"ntsc.monitor"
#define	PAL_MONITOR_NAME	"pal.monitor"
#define	STANDARD_MONITOR_MASK	( REQUEST_NTSC | REQUEST_PAL )

#define	STANDARD_NTSC_ROWS	262
#define	STANDARD_PAL_ROWS	312
#define	STANDARD_COLORCLOCKS	226
#define	STANDARD_DENISE_MAX	455
#define	STANDARD_DENISE_MIN	93
#define	STANDARD_NTSC_BEAMCON	( 0x0000 )
#define	STANDARD_PAL_BEAMCON	( DISPLAYPAL )

#define	SPECIAL_BEAMCON	( VARVBLANK | LOLDIS | VARVSYNC | VARHSYNC | VARBEAM | CSBLANK | VSYNCTRUE)

#define	MIN_NTSC_ROW	21
#define	MIN_PAL_ROW	29
#define	STANDARD_VIEW_X	0x81
#define	STANDARD_VIEW_Y	0x2C
#define	STANDARD_HBSTRT	0x06
#define	STANDARD_HSSTRT	0x0B
#define	STANDARD_HSSTOP	0x1C
#define	STANDARD_HBSTOP	0x2C
#define	STANDARD_VBSTRT	0x0122
#define	STANDARD_VSSTRT	0x02A6
#define	STANDARD_VSSTOP	0x03AA
#define	STANDARD_VBSTOP	0x1066

#define	VGA_COLORCLOCKS (STANDARD_COLORCLOCKS/2)
#define	VGA_TOTAL_ROWS	(STANDARD_NTSC_ROWS*2)
#define	VGA_DENISE_MIN	59
#define	MIN_VGA_ROW	29
#define	VGA_HBSTRT	0x08
#define	VGA_HSSTRT	0x0E
#define	VGA_HSSTOP	0x1C
#define	VGA_HBSTOP	0x1E
#define	VGA_VBSTRT	0x0000
#define	VGA_VSSTRT	0x0153
#define	VGA_VSSTOP	0x0235
#define	VGA_VBSTOP	0x0CCD

#define	VGA_MONITOR_NAME	"vga.monitor"

/* NOTE: VGA70 definitions are obsolete - a VGA70 monitor has never been
 * implemented.
 */
#define	VGA70_COLORCLOCKS (STANDARD_COLORCLOCKS/2)
#define	VGA70_TOTAL_ROWS 449
#define	VGA70_DENISE_MIN 59
#define	MIN_VGA70_ROW	35
#define	VGA70_HBSTRT	0x08
#define	VGA70_HSSTRT	0x0E
#define	VGA70_HSSTOP	0x1C
#define	VGA70_HBSTOP	0x1E
#define	VGA70_VBSTRT	0x0000
#define	VGA70_VSSTRT	0x02A6
#define	VGA70_VSSTOP	0x0388
#define	VGA70_VBSTOP	0x0F73

#define	VGA70_BEAMCON	(SPECIAL_BEAMCON ^ VSYNCTRUE)
#define	VGA70_MONITOR_NAME	"vga70.monitor"

#define	BROADCAST_HBSTRT	0x01
#define	BROADCAST_HSSTRT	0x06
#define	BROADCAST_HSSTOP	0x17
#define	BROADCAST_HBSTOP	0x27
#define	BROADCAST_VBSTRT	0x0000
#define	BROADCAST_VSSTRT	0x02A6
#define	BROADCAST_VSSTOP	0x054C
#define	BROADCAST_VBSTOP	0x1C40
#define	BROADCAST_BEAMCON	( LOLDIS | CSBLANK )
#define	RATIO_FIXEDPART	4
#define	RATIO_UNITY	(1 << RATIO_FIXEDPART)

struct	AnalogSignalInterval
{
    UWORD	asi_Start;
    UWORD	asi_Stop;
};

struct	SpecialMonitor
{
    struct	ExtendedNode	spm_Node;
    UWORD	spm_Flags;
    LONG	(*do_monitor)();
    LONG	(*reserved1)();
    LONG	(*reserved2)();
    LONG	(*reserved3)();
    struct	AnalogSignalInterval	hblank;
    struct	AnalogSignalInterval	vblank;
    struct	AnalogSignalInterval	hsync;
    struct	AnalogSignalInterval	vsync;
};

#endif	/* GRAPHICS_MONITOR_H */
```

## 7.13. graphics/displayinfo.h — QueryHeader, DisplayInfo, DimensionInfo, MonitorInfo, NameInfo, DIPF_* and DI_AVAIL_*

// Source: NDK_3.9/Include/include_h/graphics/displayinfo.h
// DisplayInfoDatabase entries. PropertyFlags tells you IS_HAM, IS_LACE, IS_DUALPF, IS_AA, IS_EXTRAHALFBRITE etc.

```c
#ifndef	GRAPHICS_DISPLAYINFO_H
#define	GRAPHICS_DISPLAYINFO_H
/*
**	$VER: displayinfo.h 39.13 (31.5.1993)
**	Includes Release 45.1
**
**	include define file for displayinfo database
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif /* EXEC_TYPES_H */

#ifndef GRAPHICS_GFX_H
#include <graphics/gfx.h>
#endif /* GRAPHICS_GFX_H */

#ifndef GRAPHICS_MONITOR_H
#include <graphics/monitor.h>
#endif /* GRAPHICS_MONITOR_H */

#ifndef GRAPHICS_MODEID_H
#include <graphics/modeid.h>
#endif /* GRAPHICS_MODEID_H */

#ifndef UTILITY_TAGITEM_H
#include <utility/tagitem.h>
#endif /* UTILITY_TAGITEM_H */

/* the "public" handle to a DisplayInfoRecord */

typedef APTR DisplayInfoHandle;

/* datachunk type identifiers */

#define DTAG_DISP		0x80000000
#define DTAG_DIMS		0x80001000
#define DTAG_MNTR		0x80002000
#define DTAG_NAME		0x80003000
#define DTAG_VEC		0x80004000	/* internal use only */

struct QueryHeader
{
	ULONG	StructID;	/* datachunk type identifier */
	ULONG	DisplayID;	/* copy of display record key	*/
	ULONG	SkipID;		/* TAG_SKIP -- see tagitems.h */
	ULONG	Length;		/* length of local data in double-longwords */
};

struct DisplayInfo
{
	struct	QueryHeader Header;
	UWORD	NotAvailable;	/* if NULL available, else see defines */
	ULONG	PropertyFlags;	/* Properties of this mode see defines */
	Point	Resolution;	/* ticks-per-pixel X/Y		       */
	UWORD	PixelSpeed;	/* aproximation in nanoseconds	       */
	UWORD	NumStdSprites;	/* number of standard amiga sprites    */
	UWORD	PaletteRange;	/* OBSOLETE - use Red/Green/Blue bits instead */
	Point	SpriteResolution; /* std sprite ticks-per-pixel X/Y    */
	UBYTE	pad[4];		/* used internally */
	UBYTE	RedBits;	/* number of Red bits this display supports (V39) */
	UBYTE	GreenBits;	/* number of Green bits this display supports (V39) */
	UBYTE	BlueBits;	/* number of Blue bits this display supports (V39) */
	UBYTE	pad2[5];	/* find some use for this. */
	ULONG	reserved[2];	/* terminator */
};

/* availability */

#define DI_AVAIL_NOCHIPS	0x0001
#define DI_AVAIL_NOMONITOR	0x0002
#define DI_AVAIL_NOTWITHGENLOCK	0x0004

/* mode properties */

#define DIPF_IS_LACE		0x00000001
#define DIPF_IS_DUALPF		0x00000002
#define DIPF_IS_PF2PRI		0x00000004
#define DIPF_IS_HAM		0x00000008

#define DIPF_IS_ECS		0x00000010	/* note: ECS modes (SHIRES, VGA, and **
											** PRODUCTIVITY) do not support      **
											** attached sprites.		     **
											*/
#define DIPF_IS_AA		0x00010000	/* AA modes - may only be available
						** if machine has correct memory
						** type to support required
						** bandwidth - check availability.
						** (V39)
						*/
#define DIPF_IS_PAL		0x00000020
#define DIPF_IS_SPRITES		0x00000040
#define DIPF_IS_GENLOCK		0x00000080

#define DIPF_IS_WB		0x00000100
#define DIPF_IS_DRAGGABLE	0x00000200
#define DIPF_IS_PANELLED	0x00000400
#define DIPF_IS_BEAMSYNC	0x00000800

#define DIPF_IS_EXTRAHALFBRITE	0x00001000

/* The following DIPF_IS_... flags are new for V39 */
#define DIPF_IS_SPRITES_ATT		0x00002000	/* supports attached sprites */
#define DIPF_IS_SPRITES_CHNG_RES	0x00004000	/* supports variable sprite resolution */
#define DIPF_IS_SPRITES_BORDER		0x00008000	/* sprite can be displayed in the border */
#define DIPF_IS_SCANDBL			0x00020000	/* scan doubled */
#define DIPF_IS_SPRITES_CHNG_BASE	0x00040000
											/* can change the sprite base colour */
#define DIPF_IS_SPRITES_CHNG_PRI	0x00080000
											/* can change the sprite priority
											** with respect to the playfield(s).
											*/
#define DIPF_IS_DBUFFER		0x00100000	/* can support double buffering */
#define DIPF_IS_PROGBEAM	0x00200000	/* is a programmed beam-sync mode */
#define DIPF_IS_FOREIGN		0x80000000	/* this mode is not native to the Amiga */


struct DimensionInfo
{
	struct	QueryHeader Header;
	UWORD	MaxDepth;	      /* log2( max number of colors ) */
	UWORD	MinRasterWidth;       /* minimum width in pixels      */
	UWORD	MinRasterHeight;      /* minimum height in pixels     */
	UWORD	MaxRasterWidth;       /* maximum width in pixels      */
	UWORD	MaxRasterHeight;      /* maximum height in pixels     */
	struct	Rectangle   Nominal;  /* "standard" dimensions	      */
	struct	Rectangle   MaxOScan; /* fixed, hardware dependent    */
	struct	Rectangle VideoOScan; /* fixed, hardware dependent    */
	struct	Rectangle   TxtOScan; /* editable via preferences     */
	struct	Rectangle   StdOScan; /* editable via preferences     */
	UBYTE	pad[14];
	ULONG	reserved[2];	      /* terminator */
};

struct MonitorInfo
{
	struct	QueryHeader Header;
	struct	MonitorSpec  *Mspc;   /* pointer to monitor specification  */
	Point	ViewPosition;	      /* editable via preferences	   */
	Point	ViewResolution;       /* standard monitor ticks-per-pixel  */
	struct	Rectangle ViewPositionRange;  /* fixed, hardware dependent */
	UWORD	TotalRows;	      /* display height in scanlines	   */
	UWORD	TotalColorClocks;     /* scanline width in 280 ns units    */
	UWORD	MinRow;	      /* absolute minimum active scanline  */
	WORD	Compatibility;	      /* how this coexists with others	   */
	UBYTE	pad[32];
	Point	MouseTicks;
	Point	DefaultViewPosition;  /* original, never changes */
	ULONG	PreferredModeID;      /* for Preferences */
	ULONG	reserved[2];	      /* terminator */
};

/* monitor compatibility */

#define MCOMPAT_MIXED	0	/* can share display with other MCOMPAT_MIXED */
#define MCOMPAT_SELF	1	/* can share only within same monitor */
#define MCOMPAT_NOBODY -1	/* only one viewport at a time */

#define DISPLAYNAMELEN 32

struct NameInfo
{
	struct	QueryHeader Header;
	UBYTE	Name[DISPLAYNAMELEN];
	ULONG	reserved[2];	      /* terminator */
};

/******************************************************************************/

/* The following VecInfo structure is PRIVATE, for our use only
 * Touch these, and burn! (V39)
 */

struct VecInfo
{
	struct	QueryHeader   Header;
	APTR	Vec;
	APTR	Data;
	UWORD	Type;
	UWORD	pad[3];
	ULONG	reserved[2];
};

#endif	/* GRAPHICS_DISPLAYINFO_H */
```

## 7.14. graphics/modeid.h — MONITOR_ID_*, *_KEY composites, BIDTAG_*

// Source: NDK_3.9/Include/include_h/graphics/modeid.h
// All the canonical ModeIDs. DO NOT decode ModeID bits yourself; use GetDisplayInfoData and DIPF_* instead.

```c
#ifndef GRAPHICS_MODEID_H
#define GRAPHICS_MODEID_H
/*
**	$VER: modeid.h 39.9 (27.5.1993)
**	Includes Release 45.1
**
**	include define file for graphics display mode IDs.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef GRAPHICS_DISPLAYINFO_H
#include <graphics/displayinfo.h>
#endif

#define INVALID_ID			~0

/* With all the new modes that are available under V38 and V39, it is highly
 * recommended that you use either the asl.library screenmode requester,
 * and/or the V39 graphics.library function BestModeIDA().
 *
 * DO NOT interpret the any of the bits in the ModeID for its meaning. For
 * example, do not interpret bit 3 (0x4) as meaning the ModeID is interlaced.
 * Instead, use GetDisplayInfoData() with DTAG_DISP, and examine the DIPF_...
 * flags to determine a ModeID's characteristics. The only exception to
 * this rule is that bit 7 (0x80) will always mean the ModeID is
 * ExtraHalfBright, and bit 11 (0x800) will always mean the ModeID is HAM.
 */

/* normal identifiers */

#define MONITOR_ID_MASK			0xFFFF1000

#define DEFAULT_MONITOR_ID		0x00000000
#define NTSC_MONITOR_ID			0x00011000
#define PAL_MONITOR_ID			0x00021000

/* the following 22 composite keys are for Modes on the default Monitor.
 * NTSC & PAL "flavors" of these particular keys may be made by or'ing
 * the NTSC or PAL MONITOR_ID with the desired MODE_KEY...
 *
 * For example, to specifically open a PAL HAM interlaced ViewPort
 * (or intuition screen), you would use the modeid of
 * (PAL_MONITOR_ID | HAMLACE_KEY)
 */

#define LORES_KEY			0x00000000
#define HIRES_KEY			0x00008000
#define SUPER_KEY			0x00008020
#define HAM_KEY				0x00000800
#define LORESLACE_KEY			0x00000004
#define HIRESLACE_KEY			0x00008004
#define SUPERLACE_KEY			0x00008024
#define HAMLACE_KEY			0x00000804
#define LORESDPF_KEY			0x00000400
#define HIRESDPF_KEY			0x00008400
#define SUPERDPF_KEY			0x00008420
#define LORESLACEDPF_KEY		0x00000404
#define HIRESLACEDPF_KEY		0x00008404
#define SUPERLACEDPF_KEY		0x00008424
#define LORESDPF2_KEY			0x00000440
#define HIRESDPF2_KEY			0x00008440
#define SUPERDPF2_KEY			0x00008460
#define LORESLACEDPF2_KEY		0x00000444
#define HIRESLACEDPF2_KEY		0x00008444
#define SUPERLACEDPF2_KEY		0x00008464
#define EXTRAHALFBRITE_KEY		0x00000080
#define EXTRAHALFBRITELACE_KEY		0x00000084
/* New for AA ChipSet (V39) */
#define HIRESHAM_KEY			0x00008800
#define SUPERHAM_KEY			0x00008820
#define HIRESEHB_KEY			0x00008080
#define SUPEREHB_KEY			0x000080a0
#define HIRESHAMLACE_KEY		0x00008804
#define SUPERHAMLACE_KEY		0x00008824
#define HIRESEHBLACE_KEY		0x00008084
#define SUPEREHBLACE_KEY		0x000080a4
/* Added for V40 - may be useful modes for some games or animations. */
#define LORESSDBL_KEY			0x00000008
#define LORESHAMSDBL_KEY		0x00000808
#define LORESEHBSDBL_KEY		0x00000088
#define HIRESHAMSDBL_KEY		0x00008808


/* VGA identifiers */

#define VGA_MONITOR_ID			0x00031000

#define VGAEXTRALORES_KEY		0x00031004
#define VGALORES_KEY			0x00039004
#define VGAPRODUCT_KEY			0x00039024
#define VGAHAM_KEY			0x00031804
#define VGAEXTRALORESLACE_KEY		0x00031005
#define VGALORESLACE_KEY		0x00039005
#define VGAPRODUCTLACE_KEY		0x00039025
#define VGAHAMLACE_KEY			0x00031805
#define VGAEXTRALORESDPF_KEY		0x00031404
#define VGALORESDPF_KEY			0x00039404
#define VGAPRODUCTDPF_KEY		0x00039424
#define VGAEXTRALORESLACEDPF_KEY	0x00031405
#define VGALORESLACEDPF_KEY		0x00039405
#define VGAPRODUCTLACEDPF_KEY		0x00039425
#define VGAEXTRALORESDPF2_KEY		0x00031444
#define VGALORESDPF2_KEY		0x00039444
#define VGAPRODUCTDPF2_KEY		0x00039464
#define VGAEXTRALORESLACEDPF2_KEY	0x00031445
#define VGALORESLACEDPF2_KEY		0x00039445
#define VGAPRODUCTLACEDPF2_KEY		0x00039465
#define VGAEXTRAHALFBRITE_KEY		0x00031084
#define VGAEXTRAHALFBRITELACE_KEY	0x00031085
/* New for AA ChipSet (V39) */
#define VGAPRODUCTHAM_KEY		0x00039824
#define VGALORESHAM_KEY			0x00039804
#define VGAEXTRALORESHAM_KEY		VGAHAM_KEY
#define VGAPRODUCTHAMLACE_KEY		0x00039825
#define VGALORESHAMLACE_KEY		0x00039805
#define VGAEXTRALORESHAMLACE_KEY	VGAHAMLACE_KEY
#define VGAEXTRALORESEHB_KEY		VGAEXTRAHALFBRITE_KEY
#define VGAEXTRALORESEHBLACE_KEY	VGAEXTRAHALFBRITELACE_KEY
#define VGALORESEHB_KEY			0x00039084
#define VGALORESEHBLACE_KEY		0x00039085
#define VGAEHB_KEY			0x000390a4
#define VGAEHBLACE_KEY			0x000390a5
/* These ModeIDs are the scandoubled equivalents of the above, with the
 * exception of the DualPlayfield modes, as AA does not allow for scandoubling
 * dualplayfield.
 */
#define VGAEXTRALORESDBL_KEY		0x00031000
#define VGALORESDBL_KEY			0x00039000
#define VGAPRODUCTDBL_KEY		0x00039020
#define VGAEXTRALORESHAMDBL_KEY		0x00031800
#define VGALORESHAMDBL_KEY		0x00039800
#define VGAPRODUCTHAMDBL_KEY		0x00039820
#define VGAEXTRALORESEHBDBL_KEY		0x00031080
#define VGALORESEHBDBL_KEY		0x00039080
#define VGAPRODUCTEHBDBL_KEY		0x000390a0

/* a2024 identifiers */

#define A2024_MONITOR_ID		0x00041000

#define A2024TENHERTZ_KEY		0x00041000
#define A2024FIFTEENHERTZ_KEY		0x00049000

/* prototype identifiers (private) */

#define PROTO_MONITOR_ID		0x00051000


/* These monitors and modes were added for the V38 release. */

#define EURO72_MONITOR_ID		0x00061000

#define EURO72EXTRALORES_KEY		0x00061004
#define EURO72LORES_KEY			0x00069004
#define EURO72PRODUCT_KEY		0x00069024
#define EURO72HAM_KEY			0x00061804
#define EURO72EXTRALORESLACE_KEY	0x00061005
#define EURO72LORESLACE_KEY		0x00069005
#define EURO72PRODUCTLACE_KEY		0x00069025
#define EURO72HAMLACE_KEY		0x00061805
#define EURO72EXTRALORESDPF_KEY		0x00061404
#define EURO72LORESDPF_KEY		0x00069404
#define EURO72PRODUCTDPF_KEY		0x00069424
#define EURO72EXTRALORESLACEDPF_KEY	0x00061405
#define EURO72LORESLACEDPF_KEY		0x00069405
#define EURO72PRODUCTLACEDPF_KEY	0x00069425
#define EURO72EXTRALORESDPF2_KEY	0x00061444
#define EURO72LORESDPF2_KEY		0x00069444
#define EURO72PRODUCTDPF2_KEY		0x00069464
#define EURO72EXTRALORESLACEDPF2_KEY	0x00061445
#define EURO72LORESLACEDPF2_KEY		0x00069445
#define EURO72PRODUCTLACEDPF2_KEY	0x00069465
#define EURO72EXTRAHALFBRITE_KEY	0x00061084
#define EURO72EXTRAHALFBRITELACE_KEY	0x00061085
/* New AA modes (V39) */
#define EURO72PRODUCTHAM_KEY		0x00069824
#define EURO72PRODUCTHAMLACE_KEY	0x00069825
#define EURO72LORESHAM_KEY		0x00069804
#define EURO72LORESHAMLACE_KEY		0x00069805
#define EURO72EXTRALORESHAM_KEY		EURO72HAM_KEY
#define EURO72EXTRALORESHAMLACE_KEY	EURO72HAMLACE_KEY
#define EURO72EXTRALORESEHB_KEY		EURO72EXTRAHALFBRITE_KEY
#define EURO72EXTRALORESEHBLACE_KEY	EURO72EXTRAHALFBRITELACE_KEY
#define EURO72LORESEHB_KEY		0x00069084
#define EURO72LORESEHBLACE_KEY		0x00069085
#define EURO72EHB_KEY			0x000690a4
#define EURO72EHBLACE_KEY		0x000690a5
/* These ModeIDs are the scandoubled equivalents of the above, with the
 * exception of the DualPlayfield modes, as AA does not allow for scandoubling
 * dualplayfield.
 */
#define EURO72EXTRALORESDBL_KEY		0x00061000
#define EURO72LORESDBL_KEY		0x00069000
#define EURO72PRODUCTDBL_KEY		0x00069020
#define EURO72EXTRALORESHAMDBL_KEY	0x00061800
#define EURO72LORESHAMDBL_KEY		0x00069800
#define EURO72PRODUCTHAMDBL_KEY		0x00069820
#define EURO72EXTRALORESEHBDBL_KEY	0x00061080
#define EURO72LORESEHBDBL_KEY		0x00069080
#define EURO72PRODUCTEHBDBL_KEY		0x000690a0


#define EURO36_MONITOR_ID		0x00071000

/* Euro36 modeids can be ORed with the default modeids a la NTSC and PAL.
 * For example, Euro36 SuperHires is
 * (EURO36_MONITOR_ID | SUPER_KEY)
 */

#define SUPER72_MONITOR_ID		0x00081000

/* Super72 modeids can be ORed with the default modeids a la NTSC and PAL.
 * For example, Super72 SuperHiresLace (800x600) is
 * (SUPER72_MONITOR_ID | SUPERLACE_KEY).
 * The following scandoubled Modes are the exception:
 */
#define SUPER72LORESDBL_KEY		0x00081008
#define SUPER72HIRESDBL_KEY		0x00089008
#define SUPER72SUPERDBL_KEY		0x00089028
#define SUPER72LORESHAMDBL_KEY		0x00081808
#define SUPER72HIRESHAMDBL_KEY		0x00089808
#define SUPER72SUPERHAMDBL_KEY		0x00089828
#define SUPER72LORESEHBDBL_KEY		0x00081088
#define SUPER72HIRESEHBDBL_KEY		0x00089088
#define SUPER72SUPEREHBDBL_KEY		0x000890a8


/* These monitors and modes were added for the V39 release. */

#define DBLNTSC_MONITOR_ID		0x00091000

#define DBLNTSCLORES_KEY		0x00091000
#define DBLNTSCLORESFF_KEY		0x00091004
#define DBLNTSCLORESHAM_KEY		0x00091800
#define DBLNTSCLORESHAMFF_KEY		0x00091804
#define DBLNTSCLORESEHB_KEY		0x00091080
#define DBLNTSCLORESEHBFF_KEY		0x00091084
#define DBLNTSCLORESLACE_KEY		0x00091005
#define DBLNTSCLORESHAMLACE_KEY		0x00091805
#define DBLNTSCLORESEHBLACE_KEY		0x00091085
#define DBLNTSCLORESDPF_KEY		0x00091400
#define DBLNTSCLORESDPFFF_KEY		0x00091404
#define DBLNTSCLORESDPFLACE_KEY		0x00091405
#define DBLNTSCLORESDPF2_KEY		0x00091440
#define DBLNTSCLORESDPF2FF_KEY		0x00091444
#define DBLNTSCLORESDPF2LACE_KEY	0x00091445
#define DBLNTSCHIRES_KEY		0x00099000
#define DBLNTSCHIRESFF_KEY		0x00099004
#define DBLNTSCHIRESHAM_KEY		0x00099800
#define DBLNTSCHIRESHAMFF_KEY		0x00099804
#define DBLNTSCHIRESLACE_KEY		0x00099005
#define DBLNTSCHIRESHAMLACE_KEY		0x00099805
#define DBLNTSCHIRESEHB_KEY		0x00099080
#define DBLNTSCHIRESEHBFF_KEY		0x00099084
#define DBLNTSCHIRESEHBLACE_KEY		0x00099085
#define DBLNTSCHIRESDPF_KEY		0x00099400
#define DBLNTSCHIRESDPFFF_KEY		0x00099404
#define DBLNTSCHIRESDPFLACE_KEY		0x00099405
#define DBLNTSCHIRESDPF2_KEY		0x00099440
#define DBLNTSCHIRESDPF2FF_KEY		0x00099444
#define DBLNTSCHIRESDPF2LACE_KEY	0x00099445
#define DBLNTSCEXTRALORES_KEY		0x00091200
#define DBLNTSCEXTRALORESHAM_KEY	0x00091a00
#define DBLNTSCEXTRALORESEHB_KEY	0x00091280
#define DBLNTSCEXTRALORESDPF_KEY	0x00091600
#define DBLNTSCEXTRALORESDPF2_KEY	0x00091640
#define DBLNTSCEXTRALORESFF_KEY		0x00091204
#define DBLNTSCEXTRALORESHAMFF_KEY	0x00091a04
#define DBLNTSCEXTRALORESEHBFF_KEY	0x00091284
#define DBLNTSCEXTRALORESDPFFF_KEY	0x00091604
#define DBLNTSCEXTRALORESDPF2FF_KEY	0x00091644
#define DBLNTSCEXTRALORESLACE_KEY	0x00091205
#define DBLNTSCEXTRALORESHAMLACE_KEY	0x00091a05
#define DBLNTSCEXTRALORESEHBLACE_KEY	0x00091285
#define DBLNTSCEXTRALORESDPFLACE_KEY	0x00091605
#define DBLNTSCEXTRALORESDPF2LACE_KEY	0x00091645

#define DBLPAL_MONITOR_ID		0x000a1000

#define DBLPALLORES_KEY			0x000a1000
#define DBLPALLORESFF_KEY		0x000a1004
#define DBLPALLORESHAM_KEY		0x000a1800
#define DBLPALLORESHAMFF_KEY		0x000a1804
#define DBLPALLORESEHB_KEY		0x000a1080
#define DBLPALLORESEHBFF_KEY		0x000a1084
#define DBLPALLORESLACE_KEY		0x000a1005
#define DBLPALLORESHAMLACE_KEY		0x000a1805
#define DBLPALLORESEHBLACE_KEY		0x000a1085
#define DBLPALLORESDPF_KEY		0x000a1400
#define DBLPALLORESDPFFF_KEY		0x000a1404
#define DBLPALLORESDPFLACE_KEY		0x000a1405
#define DBLPALLORESDPF2_KEY		0x000a1440
#define DBLPALLORESDPF2FF_KEY		0x000a1444
#define DBLPALLORESDPF2LACE_KEY		0x000a1445
#define DBLPALHIRES_KEY			0x000a9000
#define DBLPALHIRESFF_KEY		0x000a9004
#define DBLPALHIRESHAM_KEY		0x000a9800
#define DBLPALHIRESHAMFF_KEY		0x000a9804
#define DBLPALHIRESLACE_KEY		0x000a9005
#define DBLPALHIRESHAMLACE_KEY		0x000a9805
#define DBLPALHIRESEHB_KEY		0x000a9080
#define DBLPALHIRESEHBFF_KEY		0x000a9084
#define DBLPALHIRESEHBLACE_KEY			0x000a9085
#define DBLPALHIRESDPF_KEY		0x000a9400
#define DBLPALHIRESDPFFF_KEY		0x000a9404
#define DBLPALHIRESDPFLACE_KEY		0x000a9405
#define DBLPALHIRESDPF2_KEY		0x000a9440
#define DBLPALHIRESDPF2FF_KEY		0x000a9444
#define DBLPALHIRESDPF2LACE_KEY		0x000a9445
#define DBLPALEXTRALORES_KEY		0x000a1200
#define DBLPALEXTRALORESHAM_KEY		0x000a1a00
#define DBLPALEXTRALORESEHB_KEY		0x000a1280
#define DBLPALEXTRALORESDPF_KEY		0x000a1600
#define DBLPALEXTRALORESDPF2_KEY	0x000a1640
#define DBLPALEXTRALORESFF_KEY		0x000a1204
#define DBLPALEXTRALORESHAMFF_KEY	0x000a1a04
#define DBLPALEXTRALORESEHBFF_KEY	0x000a1284
#define DBLPALEXTRALORESDPFFF_KEY	0x000a1604
#define DBLPALEXTRALORESDPF2FF_KEY	0x000a1644
#define DBLPALEXTRALORESLACE_KEY	0x000a1205
#define DBLPALEXTRALORESHAMLACE_KEY	0x000a1a05
#define DBLPALEXTRALORESEHBLACE_KEY	0x000a1285
#define DBLPALEXTRALORESDPFLACE_KEY	0x000a1605
#define DBLPALEXTRALORESDPF2LACE_KEY	0x000a1645


/* Use these tags for passing to BestModeID() (V39) */

#define SPECIAL_FLAGS (DIPF_IS_DUALPF | DIPF_IS_PF2PRI | DIPF_IS_HAM | DIPF_IS_EXTRAHALFBRITE)

#define BIDTAG_DIPFMustHave	0x80000001	/* mask of the DIPF_ flags the ModeID must have */
				/* Default - NULL */
#define BIDTAG_DIPFMustNotHave	0x80000002	/* mask of the DIPF_ flags the ModeID must not have */
				/* Default - SPECIAL_FLAGS */
#define BIDTAG_ViewPort		0x80000003	/* ViewPort for which a ModeID is sought. */
				/* Default - NULL */
#define BIDTAG_NominalWidth	0x80000004	/* \ together make the aspect ratio and */
#define BIDTAG_NominalHeight	0x80000005	/* / override the vp->Width/Height. */
				/* Default - SourceID NominalDimensionInfo,
				 * or vp->DWidth/Height, or (640 * 200),
				 * in that preferred order.
				 */
#define BIDTAG_DesiredWidth	0x80000006	/* \ Nominal Width and Height of the */
#define BIDTAG_DesiredHeight	0x80000007	/* / returned ModeID. */
				/* Default - same as Nominal */
#define BIDTAG_Depth		0x80000008	/* ModeID must support this depth. */
				/* Default - vp->RasInfo->BitMap->Depth or 1 */
#define BIDTAG_MonitorID	0x80000009	/* ModeID must use this monitor. */
				/* Default - use best monitor available */
#define BIDTAG_SourceID		0x8000000a	/* instead of a ViewPort. */
				/* Default - VPModeID(vp) if BIDTAG_ViewPort is
				 * specified, else leave the DIPFMustHave and
				 * DIPFMustNotHave values untouched.
				 */
#define BIDTAG_RedBits		0x8000000b	/* \ 				*/
#define BIDTAG_BlueBits		0x8000000c	/* } Match up from the database */
#define BIDTAG_GreenBits	0x8000000d	/* /				*/
				/* Default - 4 */
#define BIDTAG_GfxPrivate	0x8000000e	/* Private */

#endif /* GRAPHICS_MODEID_H */
```

## 7.15. graphics/gfxbase.h — GfxBase, GFXF_* chipset revision flags

// Source: NDK_3.9/Include/include_h/graphics/gfxbase.h
// GfxBase fields. ChipRevBits/GfxFlags: HR_AGNUS (ECS Agnus), HR_DENISE (ECS Denise), AA_ALICE/AA_LISA (AGA). SETCHIPREV_AA enables full AGA.

```c
#ifndef GRAPHICS_GFXBASE_H
#define GRAPHICS_GFXBASE_H
/*
**	$VER: gfxbase.h 39.21 (21.4.1993)
**	Includes Release 45.1
**
**	graphics base definitions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_LISTS_H
#include <exec/lists.h>
#endif
#ifndef EXEC_LIBRARIES_H
#include <exec/libraries.h>
#endif
#ifndef EXEC_INTERRUPTS_H
#include <exec/interrupts.h>
#endif
#ifndef	GRAPHICS_MONITOR_H
#include <graphics/monitor.h>
#endif

struct GfxBase
{
	struct	Library  LibNode;
	struct	View *ActiView;
	struct	copinit *copinit;	/* ptr to copper start up list */
	LONG	*cia;			/* for 8520 resource use */
	LONG	*blitter;		/* for future blitter resource use */
	UWORD	*LOFlist;
	UWORD	*SHFlist;
	struct	bltnode *blthd,*blttl;
	struct	bltnode *bsblthd,*bsblttl;
	struct	Interrupt vbsrv,timsrv,bltsrv;
	struct	List	 TextFonts;
	struct	TextFont *DefaultFont;
	UWORD	Modes;			/* copy of current first bplcon0 */
	BYTE	VBlank;
	BYTE	Debug;
	WORD	BeamSync;
	WORD	system_bplcon0;		/* it is ored into each bplcon0 for display */
	UBYTE	SpriteReserved;
	UBYTE	bytereserved;
	UWORD	Flags;
	WORD	BlitLock;
	WORD	BlitNest;

	struct	List BlitWaitQ;
	struct	Task *BlitOwner;
	struct	List TOF_WaitQ;
	UWORD	DisplayFlags;		/* NTSC PAL GENLOC etc*/
					/* flags initialized at power on */
	struct	SimpleSprite **SimpleSprites;
	UWORD	MaxDisplayRow;		/* hardware stuff, do not use */
	UWORD	MaxDisplayColumn;	/* hardware stuff, do not use */
	UWORD	NormalDisplayRows;
	UWORD	NormalDisplayColumns;
	/* the following are for standard non interlace, 1/2 wb width */
	UWORD	NormalDPMX;		/* Dots per meter on display */
	UWORD	NormalDPMY;		/* Dots per meter on display */
	struct	SignalSemaphore *LastChanceMemory;
	UWORD	*LCMptr;
	UWORD	MicrosPerLine;		/* 256 time usec/line */
	UWORD	MinDisplayColumn;
	UBYTE	ChipRevBits0;
	UBYTE	MemType;
	UBYTE	crb_reserved[4];
	UWORD	monitor_id;
	ULONG	hedley[8];
	ULONG	hedley_sprites[8];	/* sprite ptrs for intuition mouse */
	ULONG	hedley_sprites1[8];	/* sprite ptrs for intuition mouse */
	WORD	hedley_count;
	UWORD	hedley_flags;
	WORD	hedley_tmp;
	LONG	*hash_table;
	UWORD	current_tot_rows;
	UWORD	current_tot_cclks;
	UBYTE	hedley_hint;
	UBYTE	hedley_hint2;
	ULONG	nreserved[4];
	LONG	*a2024_sync_raster;
	UWORD	control_delta_pal;
	UWORD	control_delta_ntsc;
	struct	MonitorSpec *current_monitor;
	struct	List MonitorList;
	struct	MonitorSpec *default_monitor;
	struct	SignalSemaphore *MonitorListSemaphore;
	VOID	*DisplayInfoDataBase;
	UWORD	TopLine;
	struct	SignalSemaphore *ActiViewCprSemaphore;
	ULONG	*UtilBase;		/* for hook and tag utilities. had to change because of name clash	*/
	ULONG	*ExecBase;		/* to link with rom.lib	*/
	UBYTE	*bwshifts;
	UWORD	*StrtFetchMasks;
	UWORD	*StopFetchMasks;
	UWORD	*Overrun;
	WORD	*RealStops;
	UWORD	SpriteWidth;	/* current width (in words) of sprites */
	UWORD	SpriteFMode;		/* current sprite fmode bits	*/
	BYTE	SoftSprites;	/* bit mask of size change knowledgeable sprites */
	BYTE	arraywidth;
	UWORD	DefaultSpriteWidth;	/* what width intuition wants */
	BYTE	SprMoveDisable;
	UBYTE	WantChips;
	UBYTE	BoardMemType;
	UBYTE	Bugs;
	ULONG	*gb_LayersBase;
	ULONG	ColorMask;
	APTR	IVector;
	APTR	IData;
	ULONG	SpecialCounter;		/* special for double buffering */
	APTR	DBList;
	UWORD	MonitorFlags;
	UBYTE	ScanDoubledSprites;
	UBYTE	BP3Bits;
	struct	AnalogSignalInterval MonitorVBlank;
	struct	MonitorSpec *natural_monitor;
	APTR	ProgData;
	UBYTE	ExtSprites;
	UBYTE	pad3;
	UWORD	GfxFlags;
	ULONG	VBCounter;
	struct	SignalSemaphore *HashTableSemaphore;
	ULONG	*HWEmul[9];
};

#define ChunkyToPlanarPtr HWEmul[0]






/* Values for GfxBase->DisplayFlags */
#define NTSC		1
#define GENLOC		2
#define PAL		4
#define TODA_SAFE	8
#define REALLY_PAL	16	/* what is actual crystal frequency
				 (as opposed to what bootmenu set the agnus to)?
				 (V39) */
#define LPEN_SWAP_FRAMES	32
				/* LightPen software could set this bit if the
				 * "lpen-with-interlace" fix put in for V39
				 * does not work. This is true of a number of
				 * Agnus chips.
				 * (V40).
				 */

#define BLITMSG_FAULT	4

/* bits defs for ChipRevBits */
#define	GFXB_BIG_BLITS	0
#define	GFXB_HR_AGNUS	0
#define GFXB_HR_DENISE	1
#define GFXB_AA_ALICE	2
#define GFXB_AA_LISA	3
#define GFXB_AA_MLISA	4	/* internal use only. */

#define GFXF_BIG_BLITS	1
#define	GFXF_HR_AGNUS	1
#define GFXF_HR_DENISE	2
#define GFXF_AA_ALICE	4
#define GFXF_AA_LISA	8
#define GFXF_AA_MLISA	16	/* internal use only */

/* Pass ONE of these to SetChipRev() */
#define SETCHIPREV_A	GFXF_HR_AGNUS
#define SETCHIPREV_ECS	(GFXF_HR_AGNUS | GFXF_HR_DENISE)
#define SETCHIPREV_AA	(GFXF_AA_ALICE | GFXF_AA_LISA | SETCHIPREV_ECS)
#define SETCHIPREV_BEST 0xffffffff

/* memory type */
#define BUS_16		0
#define NML_CAS		0
#define BUS_32		1
#define DBL_CAS		2
#define BANDWIDTH_1X	(BUS_16 | NML_CAS)
#define BANDWIDTH_2XNML	BUS_32
#define BANDWIDTH_2XDBL	DBL_CAS
#define BANDWIDTH_4X	(BUS_32 | DBL_CAS)

/* GfxFlags (private) */
#define NEW_DATABASE	1

#define GRAPHICSNAME	"graphics.library"

#endif	/* GRAPHICS_GFXBASE_H */
```

## 7.16. graphics/gfxmacros.h — ON_DISPLAY, SetAfPt, CMOVE, CWAIT helpers

// Source: NDK_3.9/Include/include_h/graphics/gfxmacros.h
// Do-while-0 wrapped macros around direct custom-chip writes and copper list builders.

```c
#ifndef	GRAPHICS_GFXMACROS_H
#define	GRAPHICS_GFXMACROS_H
/*
**	$VER: gfxmacros.h 39.3 (31.5.1993)
**	Includes Release 45.1
**
**
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef  GRAPHICS_RASTPORT_H
#include <graphics/rastport.h>
#endif

#ifndef  GRAPHICS_GFXBASE_H
#include <graphics/gfxbase.h>
#endif

#ifndef  HARDWARE_CUSTOM_H
#include <hardware/custom.h>
#endif

#ifndef  HARDWARE_DMABITS_H
#include <hardware/dmabits.h>
#endif

/* NOTE: Define the following symbol in your source code
 *       if you need the old style macros defined below.
 *       Otherwise you will get the more robust versions
 *       instead.
 */
#ifdef OLD_GRAPHICS_GFXMACROS_H
#define ON_DISPLAY	custom.dmacon = BITSET|DMAF_RASTER;
#define OFF_DISPLAY	custom.dmacon = BITCLR|DMAF_RASTER;
#define ON_SPRITE	custom.dmacon = BITSET|DMAF_SPRITE;
#define OFF_SPRITE	custom.dmacon = BITCLR|DMAF_SPRITE;

#define ON_VBLANK	custom.intena = BITSET|INTF_VERTB;
#define OFF_VBLANK	custom.intena = BITCLR|INTF_VERTB;

#define SetDrPt(w,p)	{(w)->LinePtrn = p;(w)->Flags |= FRST_DOT;(w)->linpatcnt=15;}
#define SetAfPt(w,p,n)	{(w)->AreaPtrn = p;(w)->AreaPtSz = n;}

#define SetOPen(w,c)	{(w)->AOlPen = c;(w)->Flags |= AREAOUTLINE;}
#define SetWrMsk(w,m)	{(w)->Mask = m;}

/* the SafeSetxxx macros are backwards (pre V39 graphics) compatible versions */
/* using these macros will make your code do the right thing under V39 AND V37 */
#define SafeSetOutlinePen(w,c)	  {if (GfxBase->LibNode.lib_Version<39) { (w)->AOlPen = c;(w)->Flags |= AREAOUTLINE;} else SetOutlinePen(w,c); }
#define SafeSetWriteMask(w,m)	{if (GfxBase->LibNode.lib_Version<39) { (w)->Mask = (m);} else SetWriteMask(w,m); }

/* synonym for GetOPen for consistency with SetOutlinePen */
#define GetOutlinePen(rp) GetOPen(rp)

#define BNDRYOFF(w)	{(w)->Flags &= ~AREAOUTLINE;}

#define CINIT(c,n)	  UCopperListInit(c,n);
#define CMOVE(c,a,b)	{ CMove(c,&a,b);CBump(c); }
#define CWAIT(c,a,b)	{ CWait(c,a,b);CBump(c); }
#define CEND(c)	{ CWAIT(c,10000,255); }

#define DrawCircle(rp,cx,cy,r)	DrawEllipse(rp,cx,cy,r,r);
#define AreaCircle(rp,cx,cy,r)	AreaEllipse(rp,cx,cy,r,r);

#else /* OLD_GRAPHICS_GFXMACROS_H */

#define ON_DISPLAY	custom.dmacon = BITSET|DMAF_RASTER
#define OFF_DISPLAY	custom.dmacon = BITCLR|DMAF_RASTER
#define ON_SPRITE	custom.dmacon = BITSET|DMAF_SPRITE
#define OFF_SPRITE	custom.dmacon = BITCLR|DMAF_SPRITE

#define ON_VBLANK	custom.intena = BITSET|INTF_VERTB
#define OFF_VBLANK	custom.intena = BITCLR|INTF_VERTB


#define SetDrPt(w,p)	do { \
				(w)->LinePtrn = (p); \
				(w)->Flags |= FRST_DOT; \
				(w)->linpatcnt = 15; \
			} while (0)

#define SetAfPt(w,p,n)	do { \
				(w)->AreaPtrn = p; \
				(w)->AreaPtSz = n; \
			} while (0)

#define SetOPen(w,c)	do { \
				(w)->AOlPen = c; \
				(w)->Flags |= AREAOUTLINE; \
			} while (0)

#define SetWrMsk(w,m)	do { \
				(w)->Mask = m; \
			} while (0)

/* the SafeSetxxx macros are backwards (pre V39 graphics) compatible versions */
/* using these macros will make your code do the right thing under V39 AND V37 */

#define SafeSetOutlinePen(w,c)	do { \
					if (GfxBase->LibNode.lib_Version < 39) \
						SetOPen(w,c); \
					else \
						SetOutlinePen(w,c); \
				} while (0)

#define SafeSetWriteMask(w,m)	do { \
					if (GfxBase->LibNode.lib_Version < 39) \
						SetWrMsk(w,m); \
					else \
						SetWriteMask(w,m); \
				} while (0)

/* synonym for GetOPen for consistency with SetOutlinePen */
#define GetOutlinePen(rp) GetOPen(rp)


#define BNDRYOFF(w)	do { \
				(w)->Flags &= ~AREAOUTLINE; \
			} while (0)


#define CINIT(c,n)	UCopperListInit(c,n)

#define CMOVE(c,a,b)	do { \
				CMove(c,&a,b); \
				CBump(c); \
			} while (0)

#define CWAIT(c,a,b)	do { \
				CWait(c,a,b); \
				CBump(c); \
			} while (0)

#define CEND(c)		do { \
				CWAIT(c,10000,255); \
			} while (0)


#define DrawCircle(rp,cx,cy,r)	DrawEllipse(rp,cx,cy,r,r)
#define AreaCircle(rp,cx,cy,r)	AreaEllipse(rp,cx,cy,r,r)


#endif /* OLD_GRAPHICS_GFXMACROS_H */

#endif	/* GRAPHICS_GFXMACROS_H */
```

## 7.17. graphics/graphint.h — Isrvstr (AddTOFTask)

// Source: NDK_3.9/Include/include_h/graphics/graphint.h
// Vertical-blank task structure for AddTOFTask().

```c
#ifndef	GRAPHICS_GRAPHINT_H
#define	GRAPHICS_GRAPHINT_H
/*
**	$VER: graphint.h 39.0 (23.9.1991)
**	Includes Release 45.1
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif

/* structure used by AddTOFTask */
struct Isrvstr
{
    struct Node is_Node;
    struct Isrvstr *Iptr;   /* passed to srvr by os */
    LONG (*code)();
    LONG (*ccode) __CLIB_PROTOTYPE((APTR));
    APTR Carg;
};

#endif	/* GRAPHICS_GRAPHINT_H */
```

## 7.18. graphics/rpattr.h — RPTAG_* (Get/SetRPAttr tags)

// Source: NDK_3.9/Include/include_h/graphics/rpattr.h
// Tag IDs for reading/writing RastPort attributes via the tag interface.

```c
#ifndef GRAPHICS_RPATTR_H
#define GRAPHICS_RPATTR_H
/*
**	$VER: rpattr.h 39.2 (31.5.1993)
**	Includes Release 45.1
**
**	tag definitions for GetRPAttr, SetRPAttr
**
*/

#define RPTAG_Font		0x80000000		/* get/set font */
#define RPTAG_APen		0x80000002		/* get/set apen */
#define RPTAG_BPen		0x80000003		/* get/set bpen */
#define RPTAG_DrMd		0x80000004		/* get/set draw mode */
#define RPTAG_OutLinePen	0x80000005	/* get/set outline pen */
#define RPTAG_OutlinePen	0x80000005	/* get/set outline pen. corrected case. */
#define RPTAG_WriteMask	0x80000006	/* get/set WriteMask */
#define RPTAG_MaxPen		0x80000007	/* get/set maxpen */

#define RPTAG_DrawBounds	0x80000008	/* get only rastport draw bounds. pass &rect */

#endif	/* GRAPHICS_RPATTR_H */
```

## 7.19. graphics/scale.h — BitScaleArgs

// Source: NDK_3.9/Include/include_h/graphics/scale.h
// BitMapScale() parameter structure.

```c
#ifndef	GRAPHICS_SCALE_H
#define	GRAPHICS_SCALE_H
/*
**	$VER: scale.h 39.0 (21.8.1991)
**	Includes Release 45.1
**
**	structure argument to BitMapScale()
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

struct BitScaleArgs {
    UWORD   bsa_SrcX, bsa_SrcY;			/* source origin */
    UWORD   bsa_SrcWidth, bsa_SrcHeight;	/* source size */
    UWORD   bsa_XSrcFactor, bsa_YSrcFactor;	/* scale factor denominators */
    UWORD   bsa_DestX, bsa_DestY;		/* destination origin */
    UWORD   bsa_DestWidth, bsa_DestHeight;	/* destination size result */
    UWORD   bsa_XDestFactor, bsa_YDestFactor;	/* scale factor numerators */
    struct BitMap *bsa_SrcBitMap;		/* source BitMap */
    struct BitMap *bsa_DestBitMap;		/* destination BitMap */
    ULONG   bsa_Flags;				/* reserved.  Must be zero! */
    UWORD   bsa_XDDA, bsa_YDDA;			/* reserved */
    LONG    bsa_Reserved1;
    LONG    bsa_Reserved2;
};
#endif	/* GRAPHICS_SCALE_H */
```

## 7.20. graphics/display.h — bplcon0/diw/ddf raw bit definitions

// Source: NDK_3.9/Include/include_h/graphics/display.h
// Low-level bit positions for bplcon0, bplcon1 scroll, diwstrt/stop, ddfstrt/stop, vposr LOF.

```c
#ifndef	GRAPHICS_DISPLAY_H
#define	GRAPHICS_DISPLAY_H
/*
**	$VER: display.h 39.0 (21.8.1991)
**	Includes Release 45.1
**
**	include define file for display control registers
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

/* bplcon0 defines */
#define MODE_640    0x8000
#define PLNCNTMSK   0x7	    /* how many bit planes? */
				    /* 0 = none, 1->6 = 1->6, 7 = reserved */
#define PLNCNTSHFT  12		    /* bits to shift for bplcon0 */
#define PF2PRI	    0x40	    /* bplcon2 bit */
#define COLORON     0x0200	    /* disable color burst */
#define DBLPF	    0x400
#define HOLDNMODIFY 0x800
#define INTERLACE   4		    /* interlace mode for 400 */

/* bplcon1 defines */
#define PFA_FINE_SCROLL       0xF
#define PFB_FINE_SCROLL_SHIFT 4
#define PF_FINE_SCROLL_MASK   0xF

/* display window start and stop defines */
#define DIW_HORIZ_POS	0x7F	   /* horizontal start/stop */
#define DIW_VRTCL_POS	0x1FF	   /* vertical start/stop */
#define DIW_VRTCL_POS_SHIFT 7

/* Data fetch start/stop horizontal position */
#define DFTCH_MASK	0xFF

/* vposr bits */
#define VPOSRLOF	0x8000

#endif	/* GRAPHICS_DISPLAY_H */
```

## 7.21. graphics/coerce.h — PRESERVE_COLORS, AVOID_FLICKER, IGNORE_MCOMPAT

// Source: NDK_3.9/Include/include_h/graphics/coerce.h
// CoerceMode() flags.

```c
#ifndef GRAPHICS_COERCE_H
#define GRAPHICS_COERCE_H
/*
**	$VER: coerce.h 39.3 (15.2.1993)
**	Includes Release 45.1
**
**	mode coercion definitions
**
**	(C) Copyright 1992-2001 Amiga, Inc.
**	    All Rights Reserved
*/

/* These flags are passed (in combination) to CoerceMode() to determine the
 * type of coercion required.
 */

/* Ensure that the mode coerced to can display just as many colours as the
 * ViewPort being coerced.
 */
#define PRESERVE_COLORS 1

/* Ensure that the mode coerced to is not interlaced. */
#define AVOID_FLICKER 2

/* Coercion should ignore monitor compatibility issues. */
#define IGNORE_MCOMPAT 4


#define BIDTAG_COERCE 1	/* Private */

#endif
```

## 7.22. graphics/collide.h — TOPHIT, BOTTOMHIT, LEFTHIT, RIGHTHIT, BORDERHIT

// Source: NDK_3.9/Include/include_h/graphics/collide.h
// GEL collision detection boundary flags.

```c
#ifndef	GRAPHICS_COLLIDE_H
#define	GRAPHICS_COLLIDE_H
/*
**	$VER: collide.h 37.0 (7.1.1991)
**	Includes Release 45.1
**
**	include file for collision detection and control
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

/* These bit descriptors are used by the GEL collide routines.
 *  These bits are set in the hitMask and meMask variables of
 *  a GEL to describe whether or not these types of collisions
 *  can affect the GEL.  BNDRY_HIT is described further below;
 *  this bit is permanently assigned as the boundary-hit flag.
 *  The other bit GEL_HIT is meant only as a default to cover
 *  any GEL hitting any other; the user may redefine this bit.
 */
#define BORDERHIT 0

/* These bit descriptors are used by the GEL boundry hit routines.
 *  When the user's boundry-hit routine is called (via the argument
 *  set by a call to SetCollision) the first argument passed to
 *  the user's routine is the address of the GEL involved in the
 *  boundry-hit, and the second argument has the appropriate bit(s)
 *  set to describe which boundry was surpassed
 */
#define TOPHIT	  1
#define BOTTOMHIT 2
#define LEFTHIT   4
#define RIGHTHIT  8

#endif	/* GRAPHICS_COLLIDE_H */
```

## 7.23. graphics/videocontrol.h — VTAG_* for VideoControl()

// Source: NDK_3.9/Include/include_h/graphics/videocontrol.h
// Genlock, chroma-key, dual-playfield, palette-bank, AGA-specific video control tags.

```c
#ifndef	GRAPHICS_VIDEOCONTROL_H
#define	GRAPHICS_VIDEOCONTROL_H
/*
**	$VER: videocontrol.h 39.8 (31.5.1993)
**	Includes Release 45.1
**
**	include define file for videocontrol commands
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif /* EXEC_TYPES_H */

#ifndef UTILITY_TAGITEM_H
#include <utility/tagitem.h>
#endif /* UTILITY_TAGITEM_H */

#define	VTAG_END_CM				0x00000000
#define	VTAG_CHROMAKEY_CLR		0x80000000
#define	VTAG_CHROMAKEY_SET		0x80000001
#define	VTAG_BITPLANEKEY_CLR		0x80000002
#define	VTAG_BITPLANEKEY_SET		0x80000003
#define	VTAG_BORDERBLANK_CLR		0x80000004
#define	VTAG_BORDERBLANK_SET		0x80000005
#define	VTAG_BORDERNOTRANS_CLR		0x80000006
#define	VTAG_BORDERNOTRANS_SET		0x80000007
#define	VTAG_CHROMA_PEN_CLR		0x80000008
#define	VTAG_CHROMA_PEN_SET		0x80000009
#define	VTAG_CHROMA_PLANE_SET		0x8000000A
#define	VTAG_ATTACH_CM_SET		0x8000000B
#define	VTAG_NEXTBUF_CM			0x8000000C
#define	VTAG_BATCH_CM_CLR		0x8000000D
#define	VTAG_BATCH_CM_SET		0x8000000E
#define	VTAG_NORMAL_DISP_GET		0x8000000F
#define	VTAG_NORMAL_DISP_SET		0x80000010
#define	VTAG_COERCE_DISP_GET		0x80000011
#define	VTAG_COERCE_DISP_SET		0x80000012
#define	VTAG_VIEWPORTEXTRA_GET		0x80000013
#define	VTAG_VIEWPORTEXTRA_SET		0x80000014
#define	VTAG_CHROMAKEY_GET		0x80000015
#define	VTAG_BITPLANEKEY_GET		0x80000016
#define	VTAG_BORDERBLANK_GET		0x80000017
#define	VTAG_BORDERNOTRANS_GET		0x80000018
#define	VTAG_CHROMA_PEN_GET		0x80000019
#define	VTAG_CHROMA_PLANE_GET		0x8000001A
#define	VTAG_ATTACH_CM_GET		0x8000001B
#define	VTAG_BATCH_CM_GET		0x8000001C
#define	VTAG_BATCH_ITEMS_GET		0x8000001D
#define	VTAG_BATCH_ITEMS_SET		0x8000001E
#define	VTAG_BATCH_ITEMS_ADD		0x8000001F
#define	VTAG_VPMODEID_GET		0x80000020
#define	VTAG_VPMODEID_SET		0x80000021
#define	VTAG_VPMODEID_CLR		0x80000022
#define	VTAG_USERCLIP_GET		0x80000023
#define	VTAG_USERCLIP_SET		0x80000024
#define	VTAG_USERCLIP_CLR		0x80000025
/* The following tags are V39 specific. They will be ignored (returing error -3) by
	earlier versions */
#define VTAG_PF1_BASE_GET		0x80000026
#define VTAG_PF2_BASE_GET		0x80000027
#define VTAG_SPEVEN_BASE_GET		0x80000028
#define VTAG_SPODD_BASE_GET		0x80000029
#define VTAG_PF1_BASE_SET		0x8000002a
#define VTAG_PF2_BASE_SET		0x8000002b
#define VTAG_SPEVEN_BASE_SET		0x8000002c
#define VTAG_SPODD_BASE_SET		0x8000002d
#define VTAG_BORDERSPRITE_GET		0x8000002e
#define VTAG_BORDERSPRITE_SET		0x8000002f
#define VTAG_BORDERSPRITE_CLR		0x80000030
#define VTAG_SPRITERESN_SET		0x80000031
#define VTAG_SPRITERESN_GET		0x80000032
#define VTAG_PF1_TO_SPRITEPRI_SET	0x80000033
#define VTAG_PF1_TO_SPRITEPRI_GET	0x80000034
#define VTAG_PF2_TO_SPRITEPRI_SET	0x80000035
#define VTAG_PF2_TO_SPRITEPRI_GET	0x80000036
#define VTAG_IMMEDIATE			0x80000037
#define VTAG_FULLPALETTE_SET		0x80000038
#define VTAG_FULLPALETTE_GET		0x80000039
#define VTAG_FULLPALETTE_CLR		0x8000003A
#define VTAG_DEFSPRITERESN_SET		0x8000003B
#define VTAG_DEFSPRITERESN_GET		0x8000003C

/* all the following tags follow the new, rational standard for videocontrol tags:
 * VC_xxx,state		set the state of attribute 'xxx' to value 'state'
 * VC_xxx_QUERY,&var	get the state of attribute 'xxx' and store it into the longword
 *			pointed to by &var.
 *
 * The following are new for V40:
 */

#define VC_IntermediateCLUpdate		0x80000080
	/* default=true. When set graphics will update the intermediate copper
	 * lists on color changes, etc. When false, it won't, and will be faster.
	 */
#define VC_IntermediateCLUpdate_Query	0x80000081

#define VC_NoColorPaletteLoad		0x80000082
	/* default = false. When set, graphics will only load color 0
	 * for this ViewPort, and so the ViewPort's colors will come
	 * from the previous ViewPort's.
	 *
	 * NB - Using this tag and VTAG_FULLPALETTE_SET together is undefined.
	 */
#define VC_NoColorPaletteLoad_Query	0x80000083

#define VC_DUALPF_Disable		0x80000084
	/* default = false. When this flag is set, the dual-pf bit
	   in Dual-Playfield screens will be turned off. Even bitplanes
	   will still come from the first BitMap and odd bitplanes
	   from the second BitMap, and both R[xy]Offsets will be
	   considered. This can be used (with appropriate palette
	   selection) for cross-fades between differently scrolling
	   images.
	   When this flag is turned on, colors will be loaded for
	   the viewport as if it were a single viewport of depth
	   depth1+depth2 */
#define VC_DUALPF_Disable_Query		0x80000085


#endif	/* GRAPHICS_VIDEOCONTROL_H */
```

# 8. Devices structs

Cross-reference: `amiga-io-audio-expansion.md`, `amiga-dos-filesystem-disk.md`.

## 8.1. devices/keyboard.h — KBD_* commands

// Source: NDK_3.9/Include/include_h/devices/keyboard.h
// keyboard.device commands. KBD_ADDRESETHANDLER hooks in a ctrl-A-A reset callback.

```c
#ifndef DEVICES_KEYBOARD_H
#define DEVICES_KEYBOARD_H
/*
**	$VER: keyboard.h 36.0 (1.5.1990)
**	Includes Release 45.1
**
**	Keyboard device command definitions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	 EXEC_IO_H
#include <exec/io.h>
#endif

#define	 KBD_READEVENT	      (CMD_NONSTD+0)
#define	 KBD_READMATRIX	      (CMD_NONSTD+1)
#define	 KBD_ADDRESETHANDLER  (CMD_NONSTD+2)
#define	 KBD_REMRESETHANDLER  (CMD_NONSTD+3)
#define	 KBD_RESETHANDLERDONE (CMD_NONSTD+4)

#endif	/* DEVICES_KEYBOARD_H */
```

## 8.2. devices/input.h — IND_* commands

// Source: NDK_3.9/Include/include_h/devices/input.h
// input.device commands. Used to inject synthetic input events.

```c
#ifndef DEVICES_INPUT_H
#define DEVICES_INPUT_H
/*
**	$VER: input.h 36.0 (1.5.1990)
**	Includes Release 45.1
**
**	input device command definitions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	EXEC_IO_H
#include <exec/io.h>
#endif

#define	 IND_ADDHANDLER	   (CMD_NONSTD+0)
#define	 IND_REMHANDLER	   (CMD_NONSTD+1)
#define	 IND_WRITEEVENT	   (CMD_NONSTD+2)
#define	 IND_SETTHRESH	   (CMD_NONSTD+3)
#define	 IND_SETPERIOD	   (CMD_NONSTD+4)
#define	 IND_SETMPORT	   (CMD_NONSTD+5)
#define	 IND_SETMTYPE	   (CMD_NONSTD+6)
#define	 IND_SETMTRIG	   (CMD_NONSTD+7)

#endif	/* DEVICES_INPUT_H */
```

## 8.3. devices/inputevent.h — InputEvent, IECLASS_*, IECODE_*, IEQUALIFIER_*, IEPointerPixel/Tablet

// Source: NDK_3.9/Include/include_h/devices/inputevent.h
// The universal input event union for keyboard, mouse, tablet, window events. Qualifier bits encode modifier state and button state.

```c
#ifndef DEVICES_INPUTEVENT_H
#define DEVICES_INPUTEVENT_H
/*
**	$VER: inputevent.h 36.10 (26.6.1992)
**	Includes Release 45.1
**
**	input event definitions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef DEVICES_TIMER_H
#include <devices/timer.h>
#endif

#ifndef UTILITY_HOOKS_H
#include <utility/hooks.h>
#endif

#ifndef UTILITY_TAGITEM_H
#include <utility/tagitem.h>
#endif

/*----- constants --------------------------------------------------*/

/*  --- InputEvent.ie_Class --- */
/* A NOP input event */
#define IECLASS_NULL			0x00
/* A raw keycode from the keyboard device */
#define IECLASS_RAWKEY			0x01
/* The raw mouse report from the game port device */
#define IECLASS_RAWMOUSE		0x02
/* A private console event */
#define IECLASS_EVENT			0x03
/* A Pointer Position report */
#define IECLASS_POINTERPOS		0x04
/* A timer event */
#define IECLASS_TIMER			0x06
/* select button pressed down over a Gadget (address in ie_EventAddress) */
#define IECLASS_GADGETDOWN		0x07
/* select button released over the same Gadget (address in ie_EventAddress) */
#define IECLASS_GADGETUP		0x08
/* some Requester activity has taken place.  See Codes REQCLEAR and REQSET */
#define IECLASS_REQUESTER		0x09
/* this is a Menu Number transmission (Menu number is in ie_Code) */
#define IECLASS_MENULIST		0x0A
/* User has selected the active Window's Close Gadget */
#define IECLASS_CLOSEWINDOW		0x0B
/* this Window has a new size */
#define IECLASS_SIZEWINDOW		0x0C
/* the Window pointed to by ie_EventAddress needs to be refreshed */
#define IECLASS_REFRESHWINDOW		0x0D
/* new preferences are available */
#define IECLASS_NEWPREFS		0x0E
/* the disk has been removed */
#define IECLASS_DISKREMOVED		0x0F
/* the disk has been inserted */
#define IECLASS_DISKINSERTED		0x10
/* the window is about to be been made active */
#define IECLASS_ACTIVEWINDOW		0x11
/* the window is about to be made inactive */
#define IECLASS_INACTIVEWINDOW		0x12
/* extended-function pointer position report (V36) */
#define IECLASS_NEWPOINTERPOS		0x13
/* Help key report during Menu session (V36) */
#define IECLASS_MENUHELP		0x14
/* the Window has been modified with move, size, zoom, or change (V36) */
#define	IECLASS_CHANGEWINDOW		0x15

/* the last class */
#define IECLASS_MAX			0x15


/*  --- InputEvent.ie_SubClass --- */
/*  IECLASS_NEWPOINTERPOS */
/*	like IECLASS_POINTERPOS */
#define IESUBCLASS_COMPATIBLE	0x00
/*	ie_EventAddress points to struct IEPointerPixel */
#define IESUBCLASS_PIXEL	0x01
/*	ie_EventAddress points to struct IEPointerTablet */
#define IESUBCLASS_TABLET	0x02
/*	ie_EventAddress points to struct IENewTablet */
#define IESUBCLASS_NEWTABLET	   0x03

/* pointed to by ie_EventAddress for IECLASS_NEWPOINTERPOS,
 * and IESUBCLASS_PIXEL.
 *
 * You specify a screen and pixel coordinates in that screen
 * at which you'd like the mouse to be positioned.
 * Intuition will try to oblige, but there will be restrictions
 * to positioning the pointer over offscreen pixels.
 *
 * IEQUALIFIER_RELATIVEMOUSE is supported for IESUBCLASS_PIXEL.
 */

struct IEPointerPixel	{
    struct Screen	*iepp_Screen;	/* pointer to an open screen */
    struct {				/* pixel coordinates in iepp_Screen */
	WORD	X;
	WORD	Y;
    }			iepp_Position;
};

/* pointed to by ie_EventAddress for IECLASS_NEWPOINTERPOS,
 * and IESUBCLASS_TABLET.
 *
 * You specify a range of values and a value within the range
 * independently for each of X and Y (the minimum value of
 * the ranges is always normalized to 0).
 *
 * Intuition will position the mouse proportionally within its
 * natural mouse position rectangle limits.
 *
 * IEQUALIFIER_RELATIVEMOUSE is not supported for IESUBCLASS_TABLET.
 */
struct IEPointerTablet	{
    struct {
	UWORD	X;
	UWORD	Y;
    }			iept_Range;	/* 0 is min, these are max	*/
    struct {
	UWORD	X;
	UWORD	Y;
    }			iept_Value;	/* between 0 and iept_Range	*/

    WORD		iept_Pressure;	/* -128 to 127 (unused, set to 0)  */
};


/* The ie_EventAddress of an IECLASS_NEWPOINTERPOS event of subclass
 * IESUBCLASS_NEWTABLET points at an IENewTablet structure.
 *
 *
 * IEQUALIFIER_RELATIVEMOUSE is not supported for IESUBCLASS_NEWTABLET.
 */

struct IENewTablet
{
    /* Pointer to a hook you wish to be called back through, in
     * order to handle scaling.  You will be provided with the
     * width and height you are expected to scale your tablet
     * to, perhaps based on some user preferences.
     * If NULL, the tablet's specified range will be mapped directly
     * to that width and height for you, and you will not be
     * called back.
     */
    struct Hook *ient_CallBack;

    /* Post-scaling coordinates and fractional coordinates.
     * DO NOT FILL THESE IN AT THE TIME THE EVENT IS WRITTEN!
     * Your driver will be called back and provided information
     * about the width and height of the area to scale the
     * tablet into.  It should scale the tablet coordinates
     * (perhaps based on some preferences controlling aspect
     * ratio, etc.) and place the scaled result into these
     * fields.	The ient_ScaledX and ient_ScaledY fields are
     * in screen-pixel resolution, but the origin ( [0,0]-point )
     * is not defined.	The ient_ScaledXFraction and
     * ient_ScaledYFraction fields represent sub-pixel position
     * information, and should be scaled to fill a UWORD fraction.
     */
    UWORD ient_ScaledX, ient_ScaledY;
    UWORD ient_ScaledXFraction, ient_ScaledYFraction;

    /* Current tablet coordinates along each axis: */
    ULONG ient_TabletX, ient_TabletY;

    /* Tablet range along each axis.  For example, if ient_TabletX
     * can take values 0-999, ient_RangeX should be 1000.
     */
    ULONG ient_RangeX, ient_RangeY;

    /* Pointer to tag-list of additional tablet attributes.
     * See <intuition/intuition.h> for the tag values.
     */
    struct TagItem *ient_TagList;
};


/*  --- InputEvent.ie_Code --- */
/*  IECLASS_RAWKEY */
#define IECODE_UP_PREFIX		0x80
#define IECODE_KEY_CODE_FIRST		0x00
#define IECODE_KEY_CODE_LAST		0x77
#define IECODE_COMM_CODE_FIRST		0x78
#define IECODE_COMM_CODE_LAST		0x7F

/*  IECLASS_ANSI */
#define IECODE_C0_FIRST			0x00
#define IECODE_C0_LAST			0x1F
#define IECODE_ASCII_FIRST		0x20
#define IECODE_ASCII_LAST		0x7E
#define IECODE_ASCII_DEL		0x7F
#define IECODE_C1_FIRST			0x80
#define IECODE_C1_LAST			0x9F
#define IECODE_LATIN1_FIRST		0xA0
#define IECODE_LATIN1_LAST		0xFF

/*  IECLASS_RAWMOUSE */
#define IECODE_LBUTTON			0x68	/* also uses IECODE_UP_PREFIX */
#define IECODE_RBUTTON			0x69
#define IECODE_MBUTTON			0x6A
#define IECODE_NOBUTTON			0xFF

/*  IECLASS_EVENT (V36) */
#define IECODE_NEWACTIVE		0x01	/* new active input window */
#define IECODE_NEWSIZE			0x02	/* resize of window */
#define IECODE_REFRESH			0x03	/* refresh of window */

/*  IECLASS_REQUESTER */
/*	broadcast when the first Requester (not subsequent ones) opens up in */
/*	the Window */
#define IECODE_REQSET			0x01
/*	broadcast when the last Requester clears out of the Window */
#define IECODE_REQCLEAR			0x00



/*  --- InputEvent.ie_Qualifier --- */
#define IEQUALIFIER_LSHIFT		0x0001
#define IEQUALIFIER_RSHIFT		0x0002
#define IEQUALIFIER_CAPSLOCK		0x0004
#define IEQUALIFIER_CONTROL		0x0008
#define IEQUALIFIER_LALT		0x0010
#define IEQUALIFIER_RALT		0x0020
#define IEQUALIFIER_LCOMMAND		0x0040
#define IEQUALIFIER_RCOMMAND		0x0080
#define IEQUALIFIER_NUMERICPAD		0x0100
#define IEQUALIFIER_REPEAT		0x0200
#define IEQUALIFIER_INTERRUPT		0x0400
#define IEQUALIFIER_MULTIBROADCAST	0x0800
#define IEQUALIFIER_MIDBUTTON		0x1000
#define IEQUALIFIER_RBUTTON		0x2000
#define IEQUALIFIER_LEFTBUTTON		0x4000
#define IEQUALIFIER_RELATIVEMOUSE	0x8000

#define IEQUALIFIERB_LSHIFT		0
#define IEQUALIFIERB_RSHIFT		1
#define IEQUALIFIERB_CAPSLOCK		2
#define IEQUALIFIERB_CONTROL		3
#define IEQUALIFIERB_LALT		4
#define IEQUALIFIERB_RALT		5
#define IEQUALIFIERB_LCOMMAND		6
#define IEQUALIFIERB_RCOMMAND		7
#define IEQUALIFIERB_NUMERICPAD		8
#define IEQUALIFIERB_REPEAT		9
#define IEQUALIFIERB_INTERRUPT		10
#define IEQUALIFIERB_MULTIBROADCAST	11
#define IEQUALIFIERB_MIDBUTTON		12
#define IEQUALIFIERB_RBUTTON		13
#define IEQUALIFIERB_LEFTBUTTON		14
#define IEQUALIFIERB_RELATIVEMOUSE	15

/*----- InputEvent -------------------------------------------------*/

struct InputEvent {
    struct  InputEvent *ie_NextEvent;	/* the chronologically next event */
    UBYTE   ie_Class;			/* the input event class */
    UBYTE   ie_SubClass;		/* optional subclass of the class */
    UWORD   ie_Code;			/* the input event code */
    UWORD   ie_Qualifier;		/* qualifiers in effect for the event*/
    union {
	struct {
	    WORD    ie_x;		/* the pointer position for the event*/
	    WORD    ie_y;
	} ie_xy;
	APTR	ie_addr;		/* the event address */
	struct {
	    UBYTE   ie_prev1DownCode;	/* previous down keys for dead */
	    UBYTE   ie_prev1DownQual;	/*   key translation: the ie_Code */
	    UBYTE   ie_prev2DownCode;	/*   & low byte of ie_Qualifier for */
	    UBYTE   ie_prev2DownQual;	/*   last & second last down keys */
	} ie_dead;
    } ie_position;
    struct timeval ie_TimeStamp;	/* the system tick at the event */
};

#define	ie_X			ie_position.ie_xy.ie_x
#define	ie_Y			ie_position.ie_xy.ie_y
#define	ie_EventAddress		ie_position.ie_addr
#define	ie_Prev1DownCode	ie_position.ie_dead.ie_prev1DownCode
#define	ie_Prev1DownQual	ie_position.ie_dead.ie_prev1DownQual
#define	ie_Prev2DownCode	ie_position.ie_dead.ie_prev2DownCode
#define	ie_Prev2DownQual	ie_position.ie_dead.ie_prev2DownQual

#endif	/* DEVICES_INPUTEVENT_H */
```

## 8.4. devices/gameport.h — GamePortTrigger, GPCT_* controller types

// Source: NDK_3.9/Include/include_h/devices/gameport.h
// gameport.device — mouse/joystick trigger configuration.

```c
#ifndef DEVICES_GAMEPORT_H
#define DEVICES_GAMEPORT_H
/*
**	$VER: gameport.h 36.1 (5.11.1990)
**	Includes Release 45.1
**
**	GamePort device command definitions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	EXEC_TYPES_H
#include	<exec/types.h>
#endif

#ifndef	EXEC_IO_H
#include	<exec/io.h>
#endif

/******	 GamePort commands ******/
#define	 GPD_READEVENT	   (CMD_NONSTD+0)
#define	 GPD_ASKCTYPE	   (CMD_NONSTD+1)
#define	 GPD_SETCTYPE	   (CMD_NONSTD+2)
#define	 GPD_ASKTRIGGER	   (CMD_NONSTD+3)
#define	 GPD_SETTRIGGER	   (CMD_NONSTD+4)

/******	 GamePort structures ******/

/* gpt_Keys */
#define	 GPTB_DOWNKEYS	   0
#define	 GPTF_DOWNKEYS	   (1<<0)
#define	 GPTB_UPKEYS	   1
#define	 GPTF_UPKEYS	   (1<<1)

struct GamePortTrigger {
   UWORD gpt_Keys;	   /* key transition triggers */
   UWORD gpt_Timeout;	   /* time trigger (vertical blank units) */
   UWORD gpt_XDelta;	   /* X distance trigger */
   UWORD gpt_YDelta;	   /* Y distance trigger */
};

/****** Controller Types ******/
#define	 GPCT_ALLOCATED	   -1	 /* allocated by another user */
#define	 GPCT_NOCONTROLLER 0

#define	 GPCT_MOUSE	   1
#define	 GPCT_RELJOYSTICK  2
#define	 GPCT_ABSJOYSTICK  3


/****** Errors ******/
#define	 GPDERR_SETCTYPE   1	 /* this controller not valid at this time */

#endif	/* DEVICES_GAMEPORT_H */
```

## 8.5. devices/keymap.h — KeyMap, KeyMapNode, KeyMapResource, KC_/DP_ codes

// Source: NDK_3.9/Include/include_h/devices/keymap.h
// KeyMap tables: separate low (0x00..0x3F) and high (0x40..0x67) rawkey mapping arrays with qualifier dimensions.

```c
#ifndef	DEVICES_KEYMAP_H
#define	DEVICES_KEYMAP_H
/*
**	$VER: keymap.h 36.3 (13.4.1990)
**	Includes Release 45.1
**
**	key map definitions for keymap.resource, keymap.library, and
**	console.device
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif
#ifndef EXEC_LISTS_H
#include <exec/lists.h>
#endif

struct	 KeyMap {
    UBYTE   *km_LoKeyMapTypes;
    ULONG   *km_LoKeyMap;
    UBYTE   *km_LoCapsable;
    UBYTE   *km_LoRepeatable;
    UBYTE   *km_HiKeyMapTypes;
    ULONG   *km_HiKeyMap;
    UBYTE   *km_HiCapsable;
    UBYTE   *km_HiRepeatable;
};

struct	KeyMapNode {
    struct Node kn_Node;	/* including name of keymap */
    struct KeyMap kn_KeyMap;
};

/* the structure of keymap.resource */
struct	KeyMapResource {
    struct Node kr_Node;
    struct List kr_List;	/* a list of KeyMapNodes */
};

/* Key Map Types */
#define  KC_NOQUAL   0
#define  KC_VANILLA  7		/* note that SHIFT+ALT+CTRL is VANILLA */
#define  KCB_SHIFT   0
#define  KCF_SHIFT   0x01
#define  KCB_ALT     1
#define  KCF_ALT     0x02
#define  KCB_CONTROL 2
#define  KCF_CONTROL 0x04
#define  KCB_DOWNUP  3
#define  KCF_DOWNUP  0x08

#define  KCB_DEAD    5		/* may be dead or modified by dead key: */
#define  KCF_DEAD    0x20	/*   use dead prefix bytes		*/

#define  KCB_STRING  6
#define  KCF_STRING  0x40

#define  KCB_NOP     7
#define  KCF_NOP     0x80


/* Dead Prefix Bytes */
#define DPB_MOD	0
#define DPF_MOD	0x01
#define DPB_DEAD	3
#define DPF_DEAD	0x08

#define DP_2DINDEXMASK	0x0f	/* mask for index for 1st of two dead keys */
#define DP_2DFACSHIFT	4	/* shift for factor for 1st of two dead keys */

#endif	/* DEVICES_KEYMAP_H */
```

## 8.6. devices/console.h — CD_*, SGR_*, DSR_*, CTC_* console commands

// Source: NDK_3.9/Include/include_h/devices/console.h
// console.device ANSI terminal commands — SGR (Set Graphic Rendition), DSR (Device Status Report), CTC (Cursor Tab Control).

```c
#ifndef DEVICES_CONSOLE_H
#define DEVICES_CONSOLE_H
/*
**	$VER: console.h 36.11 (7.11.1990)
**	Includes Release 45.1
**
**	Console device command definitions
**
**	(C) Copyright 1986-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include	<exec/types.h>
#endif

#ifndef EXEC_IO_H
#include	<exec/io.h>
#endif	/* EXEC_IO_H */

/****** Console commands ******/
#define CD_ASKKEYMAP		(CMD_NONSTD+0)
#define CD_SETKEYMAP		(CMD_NONSTD+1)
#define CD_ASKDEFAULTKEYMAP	(CMD_NONSTD+2)
#define CD_SETDEFAULTKEYMAP	(CMD_NONSTD+3)

/****** SGR parameters ******/

#define SGR_PRIMARY	0
#define SGR_BOLD	1
#define SGR_ITALIC	3
#define SGR_UNDERSCORE	4
#define SGR_NEGATIVE	7

#define	SGR_NORMAL	22	/* default foreground color, not bold */
#define	SGR_NOTITALIC	23
#define	SGR_NOTUNDERSCORE 24
#define	SGR_POSITIVE	27

/* these names refer to the ANSI standard, not the implementation */
#define SGR_BLACK	30
#define SGR_RED		31
#define SGR_GREEN	32
#define SGR_YELLOW	33
#define SGR_BLUE	34
#define SGR_MAGENTA	35
#define SGR_CYAN	36
#define SGR_WHITE	37
#define SGR_DEFAULT	39

#define SGR_BLACKBG	40
#define SGR_REDBG	41
#define SGR_GREENBG	42
#define SGR_YELLOWBG	43
#define SGR_BLUEBG	44
#define SGR_MAGENTABG	45
#define SGR_CYANBG	46
#define SGR_WHITEBG	47
#define SGR_DEFAULTBG	49

/* these names refer to the implementation, they are the preferred */
/* names for use with the Amiga console device. */
#define SGR_CLR0	30
#define SGR_CLR1	31
#define SGR_CLR2	32
#define SGR_CLR3	33
#define SGR_CLR4	34
#define SGR_CLR5	35
#define SGR_CLR6	36
#define SGR_CLR7	37

#define SGR_CLR0BG	40
#define SGR_CLR1BG	41
#define SGR_CLR2BG	42
#define SGR_CLR3BG	43
#define SGR_CLR4BG	44
#define SGR_CLR5BG	45
#define SGR_CLR6BG	46
#define SGR_CLR7BG	47


/****** DSR parameters ******/

#define DSR_CPR		6

/****** CTC parameters ******/
#define CTC_HSETTAB	0
#define CTC_HCLRTAB	2
#define CTC_HCLRTABSALL	5

/******	TBC parameters ******/
#define TBC_HCLRTAB	0
#define TBC_HCLRTABSALL	3

/******	SM and RM parameters ******/
#define M_LNM	20	/* linefeed newline mode */
#define M_ASM	">1"	/* auto scroll mode */
#define M_AWM	"?7"	/* auto wrap mode */

#endif	/* DEVICES_CONSOLE_H */
```

## 8.7. devices/conunit.h — ConUnit (console binding to Intuition Window)

// Source: NDK_3.9/Include/include_h/devices/conunit.h
// Console unit: binds console.device to a Window, stores cursor position, keymap, tab stops, attributes.

```c
#ifndef DEVICES_CONUNIT_H
#define DEVICES_CONUNIT_H
/*
**	$VER: conunit.h 36.15 (20.11.1990)
**	Includes Release 45.1
**
**	Console device unit definitions
**
**	(C) Copyright 1986-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	EXEC_TYPES_H
#include	<exec/types.h>
#endif

#ifndef EXEC_PORTS_H
#include	<exec/ports.h>
#endif

#ifndef DEVICES_CONSOLE_H
#include	<devices/console.h>
#endif

#ifndef DEVICES_KEYMAP_H
#include	<devices/keymap.h>
#endif

#ifndef DEVICES_INPUTEVENT_H
#include	<devices/inputevent.h>
#endif

/* ----	console unit numbers for OpenDevice() */
#define	CONU_LIBRARY	-1	/* no unit, just fill in IO_DEVICE field */
#define	CONU_STANDARD	0	/* standard unmapped console */

/* ---- New unit numbers for OpenDevice() - (V36) */

#define	CONU_CHARMAP	1	/* bind character map to console */
#define	CONU_SNIPMAP	3	/* bind character map w/ snip to console */

/* ---- New flag defines for OpenDevice() - (V37) */

#define CONFLAG_DEFAULT			0
#define CONFLAG_NODRAW_ON_NEWSIZE	1


#define	PMB_ASM		(M_LNM+1)	/* internal storage bit for AS flag */
#define	PMB_AWM		(PMB_ASM+1)	/* internal storage bit for AW flag */
#define	MAXTABS		80


struct	ConUnit {
    struct  MsgPort cu_MP;
    /* ---- read only variables */
    struct  Window *cu_Window;	/* intuition window bound to this unit */
    WORD    cu_XCP;		/* character position */
    WORD    cu_YCP;
    WORD    cu_XMax;		/* max character position */
    WORD    cu_YMax;
    WORD    cu_XRSize;		/* character raster size */
    WORD    cu_YRSize;
    WORD    cu_XROrigin;	/* raster origin */
    WORD    cu_YROrigin;
    WORD    cu_XRExtant;	/* raster maxima */
    WORD    cu_YRExtant;
    WORD    cu_XMinShrink;	/* smallest area intact from resize process */
    WORD    cu_YMinShrink;
    WORD    cu_XCCP;		/* cursor position */
    WORD    cu_YCCP;

    /* ---- read/write variables (writes must must be protected) */
    /* ---- storage for AskKeyMap and SetKeyMap */
    struct  KeyMap cu_KeyMapStruct;
    /* ---- tab stops */
    UWORD   cu_TabStops[MAXTABS]; /* 0 at start, 0xffff at end of list */

    /* ---- console rastport attributes */
    BYTE    cu_Mask;
    BYTE    cu_FgPen;
    BYTE    cu_BgPen;
    BYTE    cu_AOLPen;
    BYTE    cu_DrawMode;
    BYTE    cu_Obsolete1;	/* was cu_AreaPtSz -- not used in V36 */
    APTR    cu_Obsolete2;	/* was cu_AreaPtrn -- not used in V36 */
    UBYTE   cu_Minterms[8];	/* console minterms */
    struct  TextFont *cu_Font;
    UBYTE   cu_AlgoStyle;
    UBYTE   cu_TxFlags;
    UWORD   cu_TxHeight;
    UWORD   cu_TxWidth;
    UWORD   cu_TxBaseline;
    WORD    cu_TxSpacing;

    /* ---- console MODES and RAW EVENTS switches */
    UBYTE   cu_Modes[(PMB_AWM+7)/8];	/* one bit per mode */
    UBYTE   cu_RawEvents[(IECLASS_MAX+8)/8];
};

#endif	/* DEVICES_CONUNIT_H */
```

## 8.8. devices/audio.h — IOAudio, ADCMD_*, ADIOF_*

// Source: NDK_3.9/Include/include_h/devices/audio.h
// audio.device IORequest extension. ADCMD_ALLOCATE reserves channels; io_Data + ioa_Length play a sample at ioa_Period/ioa_Volume.

```c
#ifndef DEVICES_AUDIO_H
#define DEVICES_AUDIO_H
/*
**	$VER: audio.h 36.3 (29.8.1990)
**	Includes Release 45.1
**
**	audio.device include file
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_IO_H
#include <exec/io.h>
#endif

#define AUDIONAME		"audio.device"

#define ADHARD_CHANNELS		4

#define ADALLOC_MINPREC		-128
#define ADALLOC_MAXPREC		127

#define ADCMD_FREE		(CMD_NONSTD+0)
#define ADCMD_SETPREC		(CMD_NONSTD+1)
#define ADCMD_FINISH		(CMD_NONSTD+2)
#define ADCMD_PERVOL		(CMD_NONSTD+3)
#define ADCMD_LOCK		(CMD_NONSTD+4)
#define ADCMD_WAITCYCLE		(CMD_NONSTD+5)
#define ADCMD_ALLOCATE		32

#define ADIOB_PERVOL		4
#define ADIOF_PERVOL		(1<<4)
#define ADIOB_SYNCCYCLE		5
#define ADIOF_SYNCCYCLE		(1<<5)
#define ADIOB_NOWAIT		6
#define ADIOF_NOWAIT		(1<<6)
#define ADIOB_WRITEMESSAGE	7
#define ADIOF_WRITEMESSAGE	(1<<7)

#define ADIOERR_NOALLOCATION	-10
#define ADIOERR_ALLOCFAILED	-11
#define ADIOERR_CHANNELSTOLEN	-12

struct IOAudio {
    struct IORequest ioa_Request;
    WORD ioa_AllocKey;
    UBYTE *ioa_Data;
    ULONG ioa_Length;
    UWORD ioa_Period;
    UWORD ioa_Volume;
    UWORD ioa_Cycles;
    struct Message ioa_WriteMsg;
};

#endif	/* DEVICES_AUDIO_H */
```

## 8.9. devices/serial.h — IOExtSer, SERF_*, IO_STATF_*, SerErr_* codes

// Source: NDK_3.9/Include/include_h/devices/serial.h
// serial.device extended request. io_Status bit layout documented inline.

```c
#ifndef DEVICES_SERIAL_H
#define DEVICES_SERIAL_H
/*
**	$VER: serial.h 33.6 (6.11.1990)
**	Includes Release 45.1
**
**	external declarations for the serial device
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef   EXEC_IO_H
#include <exec/io.h>
#endif /* EXEC_IO_H */

		   /* array of termination char's */
		   /* to use,see serial.doc setparams */

struct IOTArray {
	ULONG TermArray0;
	ULONG TermArray1;
};


#define SER_DEFAULT_CTLCHAR 0x11130000	/* default chars for xON,xOFF */
/* You may change these via SETPARAMS.	At this time, parity is not
   calculated for xON/xOFF characters.	You must supply them with the
   desired parity. */

/******************************************************************/
/* CAUTION !!  IF YOU ACCESS the serial.device, you MUST (!!!!) use an
   IOExtSer-sized structure or you may overlay innocent memory !! */
/******************************************************************/

struct IOExtSer {
	struct	 IOStdReq IOSer;

/*     STRUCT	MsgNode
*   0	APTR	 Succ
*   4	APTR	 Pred
*   8	UBYTE	 Type
*   9	UBYTE	 Pri
*   A	APTR	 Name
*   E	APTR	 ReplyPort
*  12	UWORD	 MNLength
*     STRUCT   IOExt
*  14	APTR	 io_Device
*  18	APTR	 io_Unit
*  1C	UWORD	 io_Command
*  1E	UBYTE	 io_Flags
*  1F	BYTE	 io_Error
*     STRUCT   IOStdExt
*  20	ULONG	 io_Actual
*  24	ULONG	 io_Length
*  28	APTR	 io_Data
*  2C	ULONG	 io_Offset
*
*  30
*/

   ULONG   io_CtlChar;	  /* control char's (order = xON,xOFF,INQ,ACK) */
   ULONG   io_RBufLen;	  /* length in bytes of serial port's read buffer */
   ULONG   io_ExtFlags;   /* additional serial flags (see bitdefs below) */
   ULONG   io_Baud;	  /* baud rate requested (true baud) */
   ULONG   io_BrkTime;	  /* duration of break signal in MICROseconds */
   struct  IOTArray io_TermArray; /* termination character array */
   UBYTE   io_ReadLen;	  /* bits per read character (# of bits) */
   UBYTE   io_WriteLen;   /* bits per write character (# of bits) */
   UBYTE   io_StopBits;   /* stopbits for read (# of bits) */
   UBYTE   io_SerFlags;   /* see SerFlags bit definitions below  */
   UWORD   io_Status;
};

/* status of serial port, as follows:
*		   BIT	ACTIVE	FUNCTION
*		    0	 ---	reserved
*		    1	 ---	reserved
*		    2	 high	Connected to parallel "select" on the A1000.
*				Connected to both the parallel "select" and
*				serial "ring indicator" pins on the A500
*				& A2000.  Take care when making cables.
*		    3	 low	Data Set Ready
*		    4	 low	Clear To Send
*		    5	 low	Carrier Detect
*		    6	 low	Ready To Send
*		    7	 low	Data Terminal Ready
*		    8	 high	read overrun
*		    9	 high	break sent
*		   10	 high	break received
*		   11	 high	transmit x-OFFed
*		   12	 high	receive x-OFFed
*		13-15		reserved
*/

#define   SDCMD_QUERY		CMD_NONSTD	/* $09 */
#define   SDCMD_BREAK	       (CMD_NONSTD+1)	/* $0A */
#define   SDCMD_SETPARAMS      (CMD_NONSTD+2)	/* $0B */


#define SERB_XDISABLED	7	/* io_SerFlags xOn-xOff feature disabled bit */
#define SERF_XDISABLED	(1<<7)	/*    "     xOn-xOff feature disabled mask */
#define	SERB_EOFMODE	6	/*    "     EOF mode enabled bit */
#define	SERF_EOFMODE	(1<<6)	/*    "     EOF mode enabled mask */
#define	SERB_SHARED	5	/*    "     non-exclusive access bit */
#define	SERF_SHARED	(1<<5)	/*    "     non-exclusive access mask */
#define SERB_RAD_BOOGIE 4	/*    "     high-speed mode active bit */
#define SERF_RAD_BOOGIE (1<<4)	/*    "     high-speed mode active mask */
#define	SERB_QUEUEDBRK	3	/*    "     queue this Break ioRqst */
#define	SERF_QUEUEDBRK	(1<<3)	/*    "     queue this Break ioRqst */
#define	SERB_7WIRE	2	/*    "     RS232 7-wire protocol */
#define	SERF_7WIRE	(1<<2)	/*    "     RS232 7-wire protocol */
#define	SERB_PARTY_ODD	1	/*    "     parity feature enabled bit */
#define	SERF_PARTY_ODD	(1<<1)	/*    "     parity feature enabled mask */
#define	SERB_PARTY_ON	0	/*    "     parity-enabled bit */
#define	SERF_PARTY_ON	(1<<0)	/*    "     parity-enabled mask */

/* These now refect the actual bit positions in the io_Status UWORD */
#define	IO_STATB_XOFFREAD 12	   /* io_Status receive currently xOFF'ed bit */
#define	IO_STATF_XOFFREAD (1<<12)  /*	 "     receive currently xOFF'ed mask */
#define	IO_STATB_XOFFWRITE 11	   /*	 "     transmit currently xOFF'ed bit */
#define	IO_STATF_XOFFWRITE (1<<11) /*	 "     transmit currently xOFF'ed mask */
#define	IO_STATB_READBREAK 10	   /*	 "     break was latest input bit */
#define	IO_STATF_READBREAK (1<<10) /*	 "     break was latest input mask */
#define	IO_STATB_WROTEBREAK 9	   /*	 "     break was latest output bit */
#define	IO_STATF_WROTEBREAK (1<<9) /*	 "     break was latest output mask */
#define	IO_STATB_OVERRUN 8	   /*	 "     status word RBF overrun bit */
#define	IO_STATF_OVERRUN (1<<8)	   /*	 "     status word RBF overrun mask */


#define	SEXTB_MSPON	1	/* io_ExtFlags. Use mark-space parity, */
				/*	    instead of odd-even. */
#define	SEXTF_MSPON	(1<<1)	/*    "     mark-space parity mask */
#define	SEXTB_MARK	0	/*    "     if mark-space, use mark */
#define	SEXTF_MARK	(1<<0)	/*    "     if mark-space, use mark mask */


#define SerErr_DevBusy	       1
#define SerErr_BaudMismatch    2 /* baud rate not supported by hardware */
#define SerErr_BufErr	       4 /* Failed to allocate new read buffer */
#define SerErr_InvParam        5
#define SerErr_LineErr	       6
#define SerErr_ParityErr       9
#define SerErr_TimerErr       11 /*(See the serial/OpenDevice autodoc)*/
#define SerErr_BufOverflow    12
#define SerErr_NoDSR	      13
#define SerErr_DetectedBreak  15


#ifdef DEVICES_SERIAL_H_OBSOLETE
#define SerErr_InvBaud	       3	/* unused */
#define SerErr_NotOpen	       7	/* unused */
#define SerErr_PortReset       8	/* unused */
#define SerErr_InitErr	      10	/* unused */
#define SerErr_NoCTS	      14	/* unused */

/* These defines refer to the HIGH ORDER byte of io_Status.  They have
   been replaced by the new, corrected ones above */
#define	IOSTB_XOFFREAD	4	/* iost_hob receive currently xOFF'ed bit */
#define	IOSTF_XOFFREAD	(1<<4)	/*    "     receive currently xOFF'ed mask */
#define	IOSTB_XOFFWRITE 3	/*    "     transmit currently xOFF'ed bit */
#define	IOSTF_XOFFWRITE (1<<3)	/*    "     transmit currently xOFF'ed mask */
#define	IOSTB_READBREAK 2	/*    "     break was latest input bit */
#define	IOSTF_READBREAK (1<<2)	/*    "     break was latest input mask */
#define	IOSTB_WROTEBREAK 1	/*    "     break was latest output bit */
#define	IOSTF_WROTEBREAK (1<<1) /*    "     break was latest output mask */
#define	IOSTB_OVERRUN	0	/*    "     status word RBF overrun bit */
#define	IOSTF_OVERRUN	(1<<0)	/*    "     status word RBF overrun mask */

#define	IOSERB_BUFRREAD 7	/* io_Flags from read buffer bit */
#define	IOSERF_BUFRREAD (1<<7)	/*    "     from read buffer mask */
#define	IOSERB_QUEUED	6	/*    "     rqst-queued bit */
#define	IOSERF_QUEUED	(1<<6)	/*    "     rqst-queued mask */
#define	IOSERB_ABORT	5	/*    "     rqst-aborted bit */
#define	IOSERF_ABORT	(1<<5)	/*    "     rqst-aborted mask */
#define	IOSERB_ACTIVE	4	/*    "     rqst-qued-or-current bit */
#define	IOSERF_ACTIVE	(1<<4)	/*    "     rqst-qued-or-current mask */
#endif

#define SERIALNAME     "serial.device"

#endif /* DEVICES_SERIAL_H */
```

## 8.10. devices/parallel.h — IOExtPar, PARF_*, IOPT_*

// Source: NDK_3.9/Include/include_h/devices/parallel.h
// parallel.device extended request. Fast mode, EOF mode, ACK interrupt handshake.

```c
#ifndef DEVICES_PARALLEL_H
#define DEVICES_PARALLEL_H
/*
**	$VER: parallel.h 36.1 (10.5.1990)
**	Includes Release 45.1
**
**	parallel.device I/O request structure information
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All rights reserved.
*/

#ifndef   EXEC_IO_H
#include <exec/io.h>
#endif	 /* !EXEC_IO_H */

struct IOPArray {
	ULONG PTermArray0;
	ULONG PTermArray1;
};

/******************************************************************/
/* CAUTION !!  IF YOU ACCESS the parallel.device, you MUST (!!!!) use
   an IOExtPar-sized structure or you may overlay innocent memory !! */
/******************************************************************/

struct IOExtPar {
	struct	 IOStdReq IOPar;

/*     STRUCT	MsgNode
*   0	APTR	 Succ
*   4	APTR	 Pred
*   8	UBYTE	 Type
*   9	UBYTE	 Pri
*   A	APTR	 Name
*   E	APTR	 ReplyPort
*  12	UWORD	 MNLength
*     STRUCT   IOExt
*  14	APTR	 io_Device
*  18	APTR	 io_Unit
*  1C	UWORD	 io_Command
*  1E	UBYTE	 io_Flags
*  1F	UBYTE	 io_Error
*     STRUCT   IOStdExt
*  20	ULONG	 io_Actual
*  24	ULONG	 io_Length
*  28	APTR	 io_Data
*  2C	ULONG	 io_Offset
*  30
*/
	ULONG	io_PExtFlags;	 /* (not used) flag extension area */
	UBYTE	io_Status;	 /* status of parallel port and registers */
	UBYTE	io_ParFlags;	 /* see PARFLAGS bit definitions below */
	struct	IOPArray io_PTermArray; /* termination character array */
};

#define	PARB_SHARED	5	   /* ParFlags non-exclusive access bit */
#define	PARF_SHARED	(1<<5)	   /*	 "     non-exclusive access mask */
#define PARB_SLOWMODE	4	   /*	 "     slow printer bit */
#define PARF_SLOWMODE	(1<<4)	   /*	 "     slow printer mask */
#define PARB_FASTMODE	3	   /*	 "     fast I/O mode selected bit */
#define PARF_FASTMODE	(1<<3)	   /*	 "     fast I/O mode selected mask */
#define PARB_RAD_BOOGIE	3	   /*	 "     for backward compatibility */
#define PARF_RAD_BOOGIE	(1<<3)	   /*	 "     for backward compatibility */

#define PARB_ACKMODE	2	   /*	 "     ACK interrupt handshake bit */
#define PARF_ACKMODE	(1<<2)	   /*	 "     ACK interrupt handshake mask */

#define PARB_EOFMODE	1	   /*	 "     EOF mode enabled bit */
#define PARF_EOFMODE	(1<<1)	   /*	 "     EOF mode enabled mask */

#define IOPARB_QUEUED	6	   /* IO_FLAGS rqst-queued bit */
#define IOPARF_QUEUED	(1<<6)	   /*	 "     rqst-queued mask */
#define	IOPARB_ABORT	5	   /*	 "     rqst-aborted bit */
#define	IOPARF_ABORT	(1<<5)	   /*	 "     rqst-aborted mask */
#define	IOPARB_ACTIVE	4	   /*	 "     rqst-qued-or-current bit */
#define	IOPARF_ACTIVE	(1<<4)	   /*	 "     rqst-qued-or-current mask */
#define	IOPTB_RWDIR	3	   /* IO_STATUS read=0,write=1 bit */
#define	IOPTF_RWDIR	(1<<3)	   /*	 "     read=0,write=1 mask */
#define	IOPTB_PARSEL	2	   /*	 "     printer selected on the A1000 */
#define	IOPTF_PARSEL	(1<<2)	   /* printer selected & serial "Ring Indicator"
				      on the A500 & A2000.  Be careful when
				      making cables */
#define	IOPTB_PAPEROUT 1	   /*	 "     paper out bit */
#define	IOPTF_PAPEROUT (1<<1)	   /*	 "     paper out mask */
#define	IOPTB_PARBUSY  0	   /*	 "     printer in busy toggle bit */
#define	IOPTF_PARBUSY  (1<<0)	   /*	 "     printer in busy toggle mask */
/* Note: previous versions of this include files had bits 0 and 2 swapped */

#define PARALLELNAME		"parallel.device"

#define PDCMD_QUERY		(CMD_NONSTD)
#define PDCMD_SETPARAMS	(CMD_NONSTD+1)

#define ParErr_DevBusy			1
#define ParErr_BufTooBig	2
#define ParErr_InvParam	3
#define ParErr_LineErr		4
#define ParErr_NotOpen		5
#define ParErr_PortReset	6
#define ParErr_InitErr			7

#endif	/* DEVICES_PARALLEL_H */
```

## 8.11. devices/timer.h — timeval, EClockVal, timerequest, UNIT_* units

// Source: NDK_3.9/Include/include_h/devices/timer.h
// timer.device. UNIT_MICROHZ/UNIT_VBLANK/UNIT_ECLOCK/UNIT_WAITUNTIL. timeval = secs + micros.

```c
#ifndef DEVICES_TIMER_H
#define DEVICES_TIMER_H 1
/*
**	$VER: timer.h 36.16 (25.1.1991)
**	Includes Release 45.1
**
**	Timer device name and useful definitions.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**		All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef EXEC_IO_H
#include <exec/io.h>
#endif

/* unit defintions */
#define UNIT_MICROHZ	0
#define UNIT_VBLANK	1
#define UNIT_ECLOCK	2
#define UNIT_WAITUNTIL	3
#define	UNIT_WAITECLOCK	4

#define TIMERNAME	"timer.device"

struct timeval {
    ULONG tv_secs;
    ULONG tv_micro;
};

struct EClockVal {
    ULONG ev_hi;
    ULONG ev_lo;
};

struct timerequest {
    struct IORequest tr_node;
    struct timeval tr_time;
};

/* IO_COMMAND to use for adding a timer */
#define TR_ADDREQUEST	CMD_NONSTD
#define TR_GETSYSTIME	(CMD_NONSTD+1)
#define TR_SETSYSTIME	(CMD_NONSTD+2)

#endif /* DEVICES_TIMER_H */
```

## 8.12. devices/trackdisk.h — IOExtTD, TD_* commands, DriveGeometry, TDERR_*

// Source: NDK_3.9/Include/include_h/devices/trackdisk.h
// trackdisk.device for floppies. 11 sectors/track, 512 bytes/sector (Amiga DD). ETD_ variants do mfm sector label handling.

```c
#ifndef DEVICES_TRACKDISK_H
#define DEVICES_TRACKDISK_H

/*
**
**	$VER: trackdisk.h 33.13 (28.11.1990)
**	Includes Release 45.1
**
**	trackdisk device structure and value definitions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/

#ifndef EXEC_IO_H
#include <exec/io.h>
#endif

#ifndef EXEC_DEVICES_H
#include <exec/devices.h>
#endif

/*
 *--------------------------------------------------------------------
 *
 * Physical drive constants
 *
 *--------------------------------------------------------------------
 */

/* OBSOLETE -- use the TD_GETNUMTRACKS command! */
/*#define	NUMCYLS	80*/		/*  normal # of cylinders */
/*#define	MAXCYLS	(NUMCYLS+20)*/	/* max # cyls to look for during cal */
/*#define	NUMHEADS 2*/
/*#define	NUMTRACKS (NUMCYLS*NUMHEADS)*/

#define	NUMSECS	11
#define NUMUNITS 4

/*
 *--------------------------------------------------------------------
 *
 * Useful constants
 *
 *--------------------------------------------------------------------
 */

/*-- sizes before mfm encoding */
#define	TD_SECTOR 512
#define	TD_SECSHIFT 9		/* log TD_SECTOR */

/*
 *--------------------------------------------------------------------
 *
 * Driver Specific Commands
 *
 *--------------------------------------------------------------------
 */

/*
 *-- TD_NAME is a generic macro to get the name of the driver.	This
 *-- way if the name is ever changed you will pick up the change
 *-- automatically.
 *--
 *-- Normal usage would be:
 *--
 *-- char internalName[] = TD_NAME;
 *--
 */

#define	TD_NAME	"trackdisk.device"

#define	TDF_EXTCOM (1<<15)		/* for internal use only! */


#define	TD_MOTOR	(CMD_NONSTD+0)	/* control the disk's motor */
#define	TD_SEEK		(CMD_NONSTD+1)	/* explicit seek (for testing) */
#define	TD_FORMAT	(CMD_NONSTD+2)	/* format disk */
#define	TD_REMOVE	(CMD_NONSTD+3)	/* notify when disk changes */
#define	TD_CHANGENUM	(CMD_NONSTD+4)	/* number of disk changes */
#define	TD_CHANGESTATE	(CMD_NONSTD+5)	/* is there a disk in the drive? */
#define	TD_PROTSTATUS	(CMD_NONSTD+6)	/* is the disk write protected? */
#define	TD_RAWREAD	(CMD_NONSTD+7)	/* read raw bits from the disk */
#define	TD_RAWWRITE	(CMD_NONSTD+8)	/* write raw bits to the disk */
#define	TD_GETDRIVETYPE	(CMD_NONSTD+9)	/* get the type of the disk drive */
#define	TD_GETNUMTRACKS	(CMD_NONSTD+10)	/* # of tracks for this type drive */
#define	TD_ADDCHANGEINT	(CMD_NONSTD+11)	/* TD_REMOVE done right */
#define	TD_REMCHANGEINT	(CMD_NONSTD+12)	/* remove softint set by ADDCHANGEINT */
#define TD_GETGEOMETRY	(CMD_NONSTD+13) /* gets the disk geometry table */
#define TD_EJECT	(CMD_NONSTD+14) /* for those drives that support it */
#define	TD_LASTCOMM	(CMD_NONSTD+15)

/*
 *
 * The disk driver has an "extended command" facility.	These commands
 * take a superset of the normal IO Request block.
 *
 */

#define	ETD_WRITE	(CMD_WRITE|TDF_EXTCOM)
#define	ETD_READ	(CMD_READ|TDF_EXTCOM)
#define	ETD_MOTOR	(TD_MOTOR|TDF_EXTCOM)
#define	ETD_SEEK	(TD_SEEK|TDF_EXTCOM)
#define	ETD_FORMAT	(TD_FORMAT|TDF_EXTCOM)
#define	ETD_UPDATE	(CMD_UPDATE|TDF_EXTCOM)
#define	ETD_CLEAR	(CMD_CLEAR|TDF_EXTCOM)
#define	ETD_RAWREAD	(TD_RAWREAD|TDF_EXTCOM)
#define	ETD_RAWWRITE	(TD_RAWWRITE|TDF_EXTCOM)

/*
 *
 * extended IO has a larger than normal io request block.
 *
 */

struct IOExtTD {
	struct	IOStdReq iotd_Req;
	ULONG	iotd_Count;
	ULONG	iotd_SecLabel;
};

/*
 *  This is the structure returned by TD_DRIVEGEOMETRY
 *  Note that the layout can be defined three ways:
 *
 *  1. TotalSectors
 *  2. Cylinders and CylSectors
 *  3. Cylinders, Heads, and TrackSectors.
 *
 *  #1 is most accurate, #2 is less so, and #3 is least accurate.  All
 *  are usable, though #2 and #3 may waste some portion of the available
 *  space on some drives.
 */
struct DriveGeometry {
	ULONG	dg_SectorSize;		/* in bytes */
	ULONG	dg_TotalSectors;	/* total # of sectors on drive */
	ULONG	dg_Cylinders;		/* number of cylinders */
	ULONG	dg_CylSectors;		/* number of sectors/cylinder */
	ULONG	dg_Heads;		/* number of surfaces */
	ULONG	dg_TrackSectors;	/* number of sectors/track */
	ULONG	dg_BufMemType;		/* preferred buffer memory type */
					/* (usually MEMF_PUBLIC) */
	UBYTE	dg_DeviceType;		/* codes as defined in the SCSI-2 spec*/
	UBYTE	dg_Flags;		/* flags, including removable */
	UWORD	dg_Reserved;
};

/* device types */
#define DG_DIRECT_ACCESS	0
#define DG_SEQUENTIAL_ACCESS	1
#define DG_PRINTER		2
#define DG_PROCESSOR		3
#define DG_WORM			4
#define DG_CDROM		5
#define DG_SCANNER		6
#define DG_OPTICAL_DISK		7
#define DG_MEDIUM_CHANGER	8
#define DG_COMMUNICATION	9
#define DG_UNKNOWN		31

/* flags */
#define DGB_REMOVABLE		0
#define DGF_REMOVABLE		1

/*
** raw read and write can be synced with the index pulse.  This flag
** in io request's IO_FLAGS field tells the driver that you want this.
*/

#define IOTDB_INDEXSYNC	4
#define IOTDF_INDEXSYNC (1<<4)
/*
** raw read and write can be synced with a $4489 sync pattern.	This flag
** in io request's IO_FLAGS field tells the driver that you want this.
*/
#define IOTDB_WORDSYNC	5
#define IOTDF_WORDSYNC (1<<5)


/* labels are TD_LABELSIZE bytes per sector */

#define	TD_LABELSIZE 16

/*
** This is a bit in the FLAGS field of OpenDevice.  If it is set, then
** the driver will allow you to open all the disks that the trackdisk
** driver understands.	Otherwise only 3.5" disks will succeed.
*/

#define TDB_ALLOW_NON_3_5	0
#define TDF_ALLOW_NON_3_5	(1<<0)

/*
**  If you set the TDB_ALLOW_NON_3_5 bit in OpenDevice, then you don't
**  know what type of disk you really got.  These defines are for the
**  TD_GETDRIVETYPE command.  In addition, you can find out how many
**  tracks are supported via the TD_GETNUMTRACKS command.
*/

#define	DRIVE3_5	1
#define	DRIVE5_25	2
#define	DRIVE3_5_150RPM	3

/*
 *--------------------------------------------------------------------
 *
 * Driver error defines
 *
 *--------------------------------------------------------------------
 */

#define	TDERR_NotSpecified	20	/* general catchall */
#define	TDERR_NoSecHdr		21	/* couldn't even find a sector */
#define	TDERR_BadSecPreamble	22	/* sector looked wrong */
#define	TDERR_BadSecID		23	/* ditto */
#define	TDERR_BadHdrSum		24	/* header had incorrect checksum */
#define	TDERR_BadSecSum		25	/* data had incorrect checksum */
#define	TDERR_TooFewSecs	26	/* couldn't find enough sectors */
#define	TDERR_BadSecHdr		27	/* another "sector looked wrong" */
#define	TDERR_WriteProt		28	/* can't write to a protected disk */
#define	TDERR_DiskChanged	29	/* no disk in the drive */
#define	TDERR_SeekError		30	/* couldn't find track 0 */
#define	TDERR_NoMem		31	/* ran out of memory */
#define	TDERR_BadUnitNum	32	/* asked for a unit > NUMUNITS */
#define	TDERR_BadDriveType	33	/* not a drive that trackdisk groks */
#define	TDERR_DriveInUse	34	/* someone else allocated the drive */
#define	TDERR_PostReset		35	/* user hit reset; awaiting doom */

/*
 *--------------------------------------------------------------------
 *
 * public portion of the unit structure
 *
 *--------------------------------------------------------------------
 */

struct TDU_PublicUnit {
	struct	Unit tdu_Unit;		/* base message port */
	UWORD	tdu_Comp01Track;	/* track for first precomp */
	UWORD	tdu_Comp10Track;	/* track for second precomp */
	UWORD	tdu_Comp11Track;	/* track for third precomp */
	ULONG	tdu_StepDelay;		/* time to wait after stepping */
	ULONG	tdu_SettleDelay;	/* time to wait after seeking */
	UBYTE	tdu_RetryCnt;		/* # of times to retry */
	UBYTE	tdu_PubFlags;		/* public flags, see below */
	UWORD	tdu_CurrTrk;		/* track the heads are over... */
					/* ONLY ACCESS WHILE UNIT IS STOPPED! */
	ULONG	tdu_CalibrateDelay;	/* time to wait after stepping */
					/* during a recalibrate */
	ULONG	tdu_Counter;		/* counter for disk changes... */
					/* ONLY ACCESS WHILE UNIT IS STOPPED! */
};

/* flags for tdu_PubFlags */
#define TDPB_NOCLICK	0
#define TDPF_NOCLICK	(1L << 0)

#endif	/* DEVICES_TRACKDISK_H */
```

## 8.13. devices/hardblocks.h — RigidDiskBlock, PartitionBlock, FileSysHeaderBlock, LoadSegBlock, BadBlockBlock

// Source: NDK_3.9/Include/include_h/devices/hardblocks.h
// Amiga hard-drive Rigid Disk Block layout — `RDSK`/`PART`/`FSHD`/`LSEG`/`BADB` chunks. Required for HDD autoboot.

```c
#ifndef	DEVICES_HARDBLOCKS_H
#define	DEVICES_HARDBLOCKS_H
/*
**	$VER: hardblocks.h 44.2 (20.10.1999)
**	Includes Release 45.1
**
**	File System identifier blocks for hard disks
**
**	(C) Copyright 1988-2001 Amiga, Inc.
**	(C) Copyright 1999 Joanne Dow licensed to Amiga, Inc.
**	    All Rights Reserved
*/

/*	Changes
**	  Expanded envec
**	  Added storage for driveinit name up to 31 letters.
**	  Added storage for filesysten name up to 83 letters.
**/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif /* EXEC_TYPES_H */


/*--------------------------------------------------------------------
 *
 *	This file describes blocks of data that exist on a hard disk
 *	to describe that disk.	They are not generically accessable to
 *	the user as they do not appear on any DOS drive.  The blocks
 *	are tagged with a unique identifier, checksummed, and linked
 *	together.  The root of these blocks is the RigidDiskBlock.
 *
 *	The RigidDiskBlock must exist on the disk within the first
 *	RDB_LOCATION_LIMIT blocks.  This inhibits the use of the zero
 *	cylinder in an AmigaDOS partition: although it is strictly
 *	possible to store the RigidDiskBlock data in the reserved
 *	area of a partition, this practice is discouraged since the
 *	reserved blocks of a partition are overwritten by "Format",
 *	"Install", "DiskCopy", etc.  The recommended disk layout,
 *	then, is to use the first cylinder(s) to store all the drive
 *	data specified by these blocks: i.e. partition descriptions,
 *	file system load images, drive bad block maps, spare blocks,
 *	etc.
 *
 *	Though all descriptions in this file contemplate 512 blocks
 *	per track this desecription works functionally with any block
 *	size. The LSEG blocks should make most efficient use of the
 *	disk block size possible, for example. While this specification
 *	can support 256 byte sectors that is deprecated at this time.
 *
 *	This version adds some modest storage spaces for inserting
 *	the actual source filename for files installed on the RDBs
 *	as either DriveInit code or Filesystem code. This makes
 *	creating a mountfile suitable for use with the "C:Mount"
 *	command that can be used for manually mounting the disk if
 *	ever required.
 *
 *------------------------------------------------------------------*/

/*
 *  NOTE
 *	optional block addresses below contain $ffffffff to indicate
 *	a NULL address, as zero is a valid address
 */
struct RigidDiskBlock {
    ULONG   rdb_ID;		/* 4 character identifier */
    ULONG   rdb_SummedLongs;	/* size of this checksummed structure */
    LONG    rdb_ChkSum;		/* block checksum (longword sum to zero) */
    ULONG   rdb_HostID;		/* SCSI Target ID of host */
    ULONG   rdb_BlockBytes;	/* size of disk blocks */
    ULONG   rdb_Flags;		/* see below for defines */
    /* block list heads */
    ULONG   rdb_BadBlockList;	/* optional bad block list */
    ULONG   rdb_PartitionList;	/* optional first partition block */
    ULONG   rdb_FileSysHeaderList; /* optional file system header block */
    ULONG   rdb_DriveInit;	/* optional drive-specific init code */
				/* DriveInit(lun,rdb,ior): "C" stk & d0/a0/a1 */
    ULONG   rdb_Reserved1[6];	/* set to $ffffffff */
    /* physical drive characteristics */
    ULONG   rdb_Cylinders;	/* number of drive cylinders */
    ULONG   rdb_Sectors;	/* sectors per track */
    ULONG   rdb_Heads;		/* number of drive heads */
    ULONG   rdb_Interleave;	/* interleave */
    ULONG   rdb_Park;		/* landing zone cylinder */
    ULONG   rdb_Reserved2[3];
    ULONG   rdb_WritePreComp;	/* starting cylinder: write precompensation */
    ULONG   rdb_ReducedWrite;	/* starting cylinder: reduced write current */
    ULONG   rdb_StepRate;	/* drive step rate */
    ULONG   rdb_Reserved3[5];
    /* logical drive characteristics */
    ULONG   rdb_RDBBlocksLo;	/* low block of range reserved for hardblocks */
    ULONG   rdb_RDBBlocksHi;	/* high block of range for these hardblocks */
    ULONG   rdb_LoCylinder;	/* low cylinder of partitionable disk area */
    ULONG   rdb_HiCylinder;	/* high cylinder of partitionable data area */
    ULONG   rdb_CylBlocks;	/* number of blocks available per cylinder */
    ULONG   rdb_AutoParkSeconds; /* zero for no auto park */
    ULONG   rdb_HighRDSKBlock;	/* highest block used by RDSK */
				/* (not including replacement bad blocks) */
    ULONG   rdb_Reserved4;
    /* drive identification */
    char    rdb_DiskVendor[8];
    char    rdb_DiskProduct[16];
    char    rdb_DiskRevision[4];
    char    rdb_ControllerVendor[8];
    char    rdb_ControllerProduct[16];
    char    rdb_ControllerRevision[4];
    char    rdb_DriveInitName[40]; // jdow: Filename for driveinit source
				   // jdow: as a terminated string.
};

#define	IDNAME_RIGIDDISK	0x5244534B	/* 'RDSK' */

#define	RDB_LOCATION_LIMIT	16

#define	RDBFB_LAST	0	/* no disks exist to be configured after */
#define	RDBFF_LAST	0x01L	/*   this one on this controller */
#define	RDBFB_LASTLUN	1	/* no LUNs exist to be configured greater */
#define	RDBFF_LASTLUN	0x02L	/*   than this one at this SCSI Target ID */
#define	RDBFB_LASTTID	2	/* no Target IDs exist to be configured */
#define	RDBFF_LASTTID	0x04L	/*   greater than this one on this SCSI bus */
#define	RDBFB_NORESELECT 3	/* don't bother trying to perform reselection */
#define	RDBFF_NORESELECT 0x08L	/*   when talking to this drive */
#define	RDBFB_DISKID	4	/* rdb_Disk... identification valid */
#define	RDBFF_DISKID	0x10L
#define	RDBFB_CTRLRID	5	/* rdb_Controller... identification valid */
#define	RDBFF_CTRLRID	0x20L
				/* added 7/20/89 by commodore: */
#define RDBFB_SYNCH	6	/* drive supports scsi synchronous mode */
#define RDBFF_SYNCH	0x40L	/* CAN BE DANGEROUS TO USE IF IT DOESN'T! */

/*------------------------------------------------------------------*/
struct BadBlockEntry {
    ULONG   bbe_BadBlock;	/* block number of bad block */
    ULONG   bbe_GoodBlock;	/* block number of replacement block */
};

struct BadBlockBlock {
    ULONG   bbb_ID;		/* 4 character identifier */
    ULONG   bbb_SummedLongs;	/* size of this checksummed structure */
    LONG    bbb_ChkSum;		/* block checksum (longword sum to zero) */
    ULONG   bbb_HostID;		/* SCSI Target ID of host */
    ULONG   bbb_Next;		/* block number of the next BadBlockBlock */
    ULONG   bbb_Reserved;
    struct BadBlockEntry bbb_BlockPairs[61]; /* bad block entry pairs */
    /* note [61] assumes 512 byte blocks */
};

#define	IDNAME_BADBLOCK		0x42414442	/* 'BADB' */

/*------------------------------------------------------------------*/
struct PartitionBlock {
    ULONG   pb_ID;		/* 4 character identifier */
    ULONG   pb_SummedLongs;	/* size of this checksummed structure */
    LONG    pb_ChkSum;		/* block checksum (longword sum to zero) */
    ULONG   pb_HostID;		/* SCSI Target ID of host */
    ULONG   pb_Next;		/* block number of the next PartitionBlock */
    ULONG   pb_Flags;		/* see below for defines */
    ULONG   pb_Reserved1[2];
    ULONG   pb_DevFlags;	/* preferred flags for OpenDevice */
    UBYTE   pb_DriveName[32];	/* preferred DOS device name: BSTR form */
				/* (not used if this name is in use) */
    ULONG   pb_Reserved2[15];	/* filler to 32 longwords */
    ULONG   pb_Environment[20];	/* environment vector for this partition */
    ULONG   pb_EReserved[12];	/* reserved for future environment vector */
};

#define	IDNAME_PARTITION	0x50415254	/* 'PART' */

#define	PBFB_BOOTABLE	0	/* this partition is intended to be bootable */
#define	PBFF_BOOTABLE	1L	/*   (expected directories and files exist) */
#define	PBFB_NOMOUNT	1	/* do not mount this partition (e.g. manually */
#define	PBFF_NOMOUNT	2L	/*   mounted, but space reserved here) */

/*------------------------------------------------------------------*/
struct FileSysHeaderBlock {
    ULONG   fhb_ID;		/* 4 character identifier */
    ULONG   fhb_SummedLongs;	/* size of this checksummed structure */
    LONG    fhb_ChkSum;		/* block checksum (longword sum to zero) */
    ULONG   fhb_HostID;		/* SCSI Target ID of host */
    ULONG   fhb_Next;		/* block number of next FileSysHeaderBlock */
    ULONG   fhb_Flags;		/* see below for defines */
    ULONG   fhb_Reserved1[2];
    ULONG   fhb_DosType;	/* file system description: match this with */
				/* partition environment's DE_DOSTYPE entry */
    ULONG   fhb_Version;	/* release version of this code */
    ULONG   fhb_PatchFlags;	/* bits set for those of the following that */
				/*   need to be substituted into a standard */
				/*   device node for this file system: e.g. */
				/*   0x180 to substitute SegList & GlobalVec */
    ULONG   fhb_Type;		/* device node type: zero */
    ULONG   fhb_Task;		/* standard dos "task" field: zero */
    ULONG   fhb_Lock;		/* not used for devices: zero */
    ULONG   fhb_Handler;	/* filename to loadseg: zero placeholder */
    ULONG   fhb_StackSize;	/* stacksize to use when starting task */
    LONG    fhb_Priority;	/* task priority when starting task */
    LONG    fhb_Startup;	/* startup msg: zero placeholder */
    LONG    fhb_SegListBlocks;	/* first of linked list of LoadSegBlocks: */
				/*   note that this entry requires some */
				/*   processing before substitution */
    LONG    fhb_GlobalVec;	/* BCPL global vector when starting task */
    ULONG   fhb_Reserved2[23];	/* (those reserved by PatchFlags) */
    char    fhb_FileSysName[84]; /* File system file name as loaded. */
};

#define	IDNAME_FILESYSHEADER	0x46534844	/* 'FSHD' */

/*------------------------------------------------------------------*/
struct LoadSegBlock {
    ULONG   lsb_ID;		/* 4 character identifier */
    ULONG   lsb_SummedLongs;	/* size of this checksummed structure */
    LONG    lsb_ChkSum;		/* block checksum (longword sum to zero) */
    ULONG   lsb_HostID;		/* SCSI Target ID of host */
    ULONG   lsb_Next;		/* block number of the next LoadSegBlock */
    ULONG   lsb_LoadData[123];	/* data for "loadseg" */
    /* note [123] assumes 512 byte blocks */
};

#define	IDNAME_LOADSEG		0x4C534547	/* 'LSEG' */

#endif	/* DEVICES_HARDBLOCKS_H */
```

## 8.14. devices/bootblock.h — BootBlock, BBID_DOS, BBID_KICK

// Source: NDK_3.9/Include/include_h/devices/bootblock.h
// Floppy bootblock — 1KB ('DOS\0' or 'KICK') checksummed header followed by boot code.

```c
#ifndef DEVICES_BOOTBLOCK_H
#define DEVICES_BOOTBLOCK_H
/*
**	$VER: bootblock.h 36.6 (5.11.1990)
**	Includes Release 45.1
**
**	floppy BootBlock definition
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include	<exec/types.h>
#endif

struct BootBlock {
	UBYTE	bb_id[4];		/* 4 character identifier */
	LONG	bb_chksum;		/* boot block checksum (balance) */
	LONG	bb_dosblock;		/* reserved for DOS patch */
};

#define		BOOTSECTS	2	/* 1K bootstrap */

#define BBID_DOS	{ 'D', 'O', 'S', '\0' }
#define BBID_KICK	{ 'K', 'I', 'C', 'K' }

#define BBNAME_DOS	0x444F5300	/* 'DOS\0' */
#define BBNAME_KICK	0x4B49434B	/* 'KICK' */

#endif	/* DEVICES_BOOTBLOCK_H */
```

## 8.15. devices/clipboard.h — IOClipReq, CBD_* commands

// Source: NDK_3.9/Include/include_h/devices/clipboard.h
// clipboard.device for IFF clipboard streams.

```c
#ifndef     DEVICES_CLIPBOARD_H
#define     DEVICES_CLIPBOARD_H
/*
**	$VER: clipboard.h 36.5 (2.11.1990)
**	Includes Release 45.1
**
**	clipboard.device structure definitions
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	EXEC_TYPES_H
#include <exec/types.h>
#endif
#ifndef	EXEC_NODES_H
#include <exec/nodes.h>
#endif
#ifndef	EXEC_LISTS_H
#include <exec/lists.h>
#endif
#ifndef	EXEC_PORTS_H
#include <exec/ports.h>
#endif

#define	CBD_POST		(CMD_NONSTD+0)
#define	CBD_CURRENTREADID	(CMD_NONSTD+1)
#define	CBD_CURRENTWRITEID	(CMD_NONSTD+2)
#define	CBD_CHANGEHOOK		(CMD_NONSTD+3)

#define	CBERR_OBSOLETEID	1


struct ClipboardUnitPartial {
    struct  Node cu_Node;	/* list of units */
    ULONG   cu_UnitNum;		/* unit number for this unit */
    /* the remaining unit data is private to the device */
};


struct IOClipReq {
    struct Message io_Message;
    struct Device *io_Device;	/* device node pointer	*/
    struct ClipboardUnitPartial *io_Unit; /* unit node pointer */
    UWORD   io_Command;		/* device command */
    UBYTE   io_Flags;		/* including QUICK and SATISFY */
    BYTE    io_Error;		/* error or warning num */
    ULONG   io_Actual;		/* number of bytes transferred */
    ULONG   io_Length;		/* number of bytes requested */
    STRPTR  io_Data;		/* either clip stream or post port */
    ULONG   io_Offset;		/* offset in clip stream */
    LONG    io_ClipID;		/* ordinal clip identifier */
};

#define	PRIMARY_CLIP	0	/* primary clip unit */

struct SatisfyMsg {
    struct Message sm_Msg;	/* the length will be 6 */
    UWORD   sm_Unit;		/* which clip unit this is */
    LONG    sm_ClipID;		/* the clip identifier of the post */
};

struct ClipHookMsg {
    ULONG   chm_Type;		/* zero for this structure format */
    LONG    chm_ChangeCmd;	/* command that caused this hook invocation: */
				/*   either CMD_UPDATE or CBD_POST */
    LONG    chm_ClipID;		/* the clip identifier of the new data */
};

#endif	/* DEVICES_CLIPBOARD_H */
```

## 8.16. devices/scsidisk.h — SCSICmd, HD_SCSICMD, HFERR_*

// Source: NDK_3.9/Include/include_h/devices/scsidisk.h
// HD_SCSICMD for direct SCSI command issue. Auto-sense support.

```c
#ifndef	DEVICES_SCSIDISK_H
#define	DEVICES_SCSIDISK_H
/*
**	$VER: scsidisk.h 44.1 (17.04.1999)
**	Includes Release 45.1
**
**	SCSI exec-level device command
**
**	(C) Copyright 1988-2001 Amiga, Inc.
**	    All Rights Reserved
**
**	(C) Copyright 1999 by Joanne Dow, Wizardess Designs, licensed to
**		Amiga Inc.
**		All Rights Reserved
*/

/*
**	Changes:
**		Added new numbering scheme for handling WIDE SCSI devices.
**		Note that at this time only support for up to 16 IDs is
**		contemplated in most designs although this numbering system
**		can consider far far more.
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif /* EXEC_TYPES_H */

/*--------------------------------------------------------------------
 *
 *   SCSI Command
 *	Several Amiga SCSI controller manufacturers are converging on
 *	standard ways to talk to their controllers.  This include
 *	file describes an exec-device command (e.g. for hddisk.device)
 *	that can be used to issue SCSI commands
 *
 *   UNIT NUMBERS
 *	Unit numbers to the OpenDevice call have encoded in them which
 *	SCSI device is being referred to.  The three decimal digits of
 *	the unit number refer to the SCSI Target ID (bus address) in
 *	the 1's digit, the SCSI logical unit (LUN) in the 10's digit,
 *	and the controller board in the 100's digit.
 *
 *	Examples:
 *		  0	drive at address 0
 *		 12	LUN 1 on multiple drive controller at address 2
 *		104	second controller board, address 4
 *		 88	not valid: both logical units and addresses
 *			range from 0..7.
 *
 *   CAVEATS
 *	Original 2090 code did not support this command.
 *
 *	Commodore 2090/2090A unit numbers are different.  The SCSI
 *	logical unit is the 100's digit, and the SCSI Target ID
 *	is a permuted 1's digit: Target ID 0..6 maps to unit 3..9
 *	(7 is reserved for the controller).
 *
 *	    Examples:
 *		  3	drive at address 0
 *		109	drive at address 6, logical unit 1
 *		  1	not valid: this is not a SCSI unit.  Perhaps
 *			it's an ST506 unit.
 *
 *	Some controller boards generate a unique name (e.g. 2090A's
 *	iddisk.device) for the second controller board, instead of
 *	implementing the 100's digit.
 *
 *	With the advent of wide SCSI the scheme above fails miserably.
 *	A new scheme was adopted by Phase V, who appear to be the only
 *	source of wide SCSI for the Amiga at this time. Thus their
 *	numbering system kludge is adopted here. When the ID or LUN is
 *	above 7 the new numbering scheme is used.
 *
 *	Unit =
 *		Board * 10 * 1000 * 1000 +
 *		LUN	  * 10 * 1000		 +
 *		ID	  * 10				 +
 *		HD_WIDESCSI;
 *
 *	There are optional restrictions on the alignment, bus
 *	accessability, and size of the data for the data phase.
 *	Be conservative to work with all manufacturer's controllers.
 *
 *------------------------------------------------------------------*/

#define HD_WIDESCSI	8	/* Wide SCSI detection bit. */
#define	HD_SCSICMD	28	/* issue a SCSI command to the unit */
				/* io_Data points to a SCSICmd */
				/* io_Length is sizeof(struct SCSICmd) */
				/* io_Actual and io_Offset are not used */

struct SCSICmd {
    UWORD  *scsi_Data;		/* word aligned data for SCSI Data Phase */
				/* (optional) data need not be byte aligned */
				/* (optional) data need not be bus accessable */
    ULONG   scsi_Length;	/* even length of Data area */
				/* (optional) data can have odd length */
				/* (optional) data length can be > 2**24 */
    ULONG   scsi_Actual;	/* actual Data used */
    UBYTE  *scsi_Command;	/* SCSI Command (same options as scsi_Data) */
    UWORD   scsi_CmdLength;	/* length of Command */
    UWORD   scsi_CmdActual;	/* actual Command used */
    UBYTE   scsi_Flags;		/* includes intended data direction */
    UBYTE   scsi_Status;	/* SCSI status of command */
    UBYTE  *scsi_SenseData;	/* sense data: filled if SCSIF_[OLD]AUTOSENSE */
				/* is set and scsi_Status has CHECK CONDITION */
				/* (bit 1) set */
    UWORD   scsi_SenseLength;	/* size of scsi_SenseData, also bytes to */
				/* request w/ SCSIF_AUTOSENSE, must be 4..255 */
    UWORD   scsi_SenseActual;	/* amount actually fetched (0 means no sense) */
};


/*----- scsi_Flags -----*/
#define	SCSIF_WRITE		0	/* intended data direction is out */
#define	SCSIF_READ		1	/* intended data direction is in */
#define	SCSIB_READ_WRITE	0	/* (the bit to test) */

#define	SCSIF_NOSENSE		0	/* no automatic request sense */
#define	SCSIF_AUTOSENSE		2	/* do standard extended request sense */
					/* on check condition */
#define	SCSIF_OLDAUTOSENSE	6	/* do 4 byte non-extended request */
					/* sense on check condition */
#define	SCSIB_AUTOSENSE		1	/* (the bit to test) */
#define	SCSIB_OLDAUTOSENSE	2	/* (the bit to test) */

/*----- SCSI io_Error values -----*/
#define	HFERR_SelfUnit		40	/* cannot issue SCSI command to self */
#define	HFERR_DMA		41	/* DMA error */
#define	HFERR_Phase		42	/* illegal or unexpected SCSI phase */
#define	HFERR_Parity		43	/* SCSI parity error */
#define	HFERR_SelTimeout	44	/* Select timed out */
#define	HFERR_BadStatus		45	/* status and/or sense error */

/*----- OpenDevice io_Error values -----*/
#define	HFERR_NoBoard		50	/* Open failed for non-existant board */

#endif	/* DEVICES_SCSIDISK_H */
```

# 9. Intuition structs

Cross-reference: `amiga-graphics-display.md` (Intuition sections).

## 9.1. intuition/intuition.h — Menu, MenuItem, Gadget, ExtGadget, Requester, IntuiMessage, Window, NewWindow, ExtNewWindow, IntuiText, Border, Image, Remember, ColorSpec, EasyStruct, WA_* tags, IDCMP_* classes, GACT_*, GFLG_*, GTYP_*, TabletData

// Source: NDK_3.9/Include/include_h/intuition/intuition.h
// The big one. Every gadget/window/menu/requester/message struct and all IDCMP classes, window flags, gadget flags. Includes screens.h and preferences.h at the end.

```c
#ifndef INTUITION_INTUITION_H
#define INTUITION_INTUITION_H TRUE
/*
**  $VER: intuition.h 38.26 (15.2.1993)
**  Includes Release 45.1
**
**  Interface definitions for Intuition applications.
**
**  (C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef GRAPHICS_GFX_H
#include <graphics/gfx.h>
#endif

#ifndef GRAPHICS_CLIP_H
#include <graphics/clip.h>
#endif

#ifndef GRAPHICS_VIEW_H
#include <graphics/view.h>
#endif

#ifndef GRAPHICS_RASTPORT_H
#include <graphics/rastport.h>
#endif

#ifndef GRAPHICS_LAYERS_H
#include <graphics/layers.h>
#endif

#ifndef GRAPHICS_TEXT_H
#include <graphics/text.h>
#endif

#ifndef EXEC_PORTS_H
#include <exec/ports.h>
#endif

#ifndef DEVICES_INPUTEVENT_H
#include <devices/inputevent.h>
#endif

#ifndef UTILITY_TAGITEM_H
#include <utility/tagitem.h>
#endif

/*
 * NOTE:  intuition/iobsolete.h is included at the END of this file!
 */

/* ======================================================================== */
/* === Menu =============================================================== */
/* ======================================================================== */
struct Menu
{
    struct Menu *NextMenu;	/* same level */
    WORD LeftEdge, TopEdge;	/* position of the select box */
    WORD Width, Height;	/* dimensions of the select box */
    UWORD Flags;		/* see flag definitions below */
    BYTE *MenuName;		/* text for this Menu Header */
    struct MenuItem *FirstItem; /* pointer to first in chain */

    /* these mysteriously-named variables are for internal use only */
    WORD JazzX, JazzY, BeatX, BeatY;
};


/* FLAGS SET BY BOTH THE APPLIPROG AND INTUITION */
#define MENUENABLED 0x0001	/* whether or not this menu is enabled */

/* FLAGS SET BY INTUITION */
#define MIDRAWN 0x0100		/* this menu's items are currently drawn */






/* ======================================================================== */
/* === MenuItem =========================================================== */
/* ======================================================================== */
struct MenuItem
{
    struct MenuItem *NextItem;	/* pointer to next in chained list */
    WORD LeftEdge, TopEdge;	/* position of the select box */
    WORD Width, Height;		/* dimensions of the select box */
    UWORD Flags;		/* see the defines below */

    LONG MutualExclude;		/* set bits mean this item excludes that */

    APTR ItemFill;		/* points to Image, IntuiText, or NULL */

    /* when this item is pointed to by the cursor and the items highlight
     *	mode HIGHIMAGE is selected, this alternate image will be displayed
     */
    APTR SelectFill;		/* points to Image, IntuiText, or NULL */

    BYTE Command;		/* only if appliprog sets the COMMSEQ flag */

    struct MenuItem *SubItem;	/* if non-zero, points to MenuItem for submenu */

    /* The NextSelect field represents the menu number of next selected
     *	item (when user has drag-selected several items)
     */
    UWORD NextSelect;
};


/* FLAGS SET BY THE APPLIPROG */
#define CHECKIT		0x0001	/* set to indicate checkmarkable item */
#define ITEMTEXT	0x0002	/* set if textual, clear if graphical item */
#define COMMSEQ		0x0004	/* set if there's an command sequence */
#define MENUTOGGLE	0x0008	/* set for toggling checks (else mut. exclude) */
#define ITEMENABLED	0x0010	/* set if this item is enabled */

/* these are the SPECIAL HIGHLIGHT FLAG state meanings */
#define HIGHFLAGS	0x00C0	/* see definitions below for these bits */
#define HIGHIMAGE	0x0000	/* use the user's "select image" */
#define HIGHCOMP	0x0040	/* highlight by complementing the selectbox */
#define HIGHBOX		0x0080	/* highlight by "boxing" the selectbox */
#define HIGHNONE	0x00C0	/* don't highlight */

/* FLAGS SET BY BOTH APPLIPROG AND INTUITION */
#define CHECKED	0x0100	/* state of the checkmark */

/* FLAGS SET BY INTUITION */
#define ISDRAWN		0x1000	/* this item's subs are currently drawn */
#define HIGHITEM	0x2000	/* this item is currently highlighted */
#define MENUTOGGLED	0x4000	/* this item was already toggled */





/* ======================================================================== */
/* === Requester ========================================================== */
/* ======================================================================== */
struct Requester
{
    struct Requester *OlderRequest;
    WORD LeftEdge, TopEdge;		/* dimensions of the entire box */
    WORD Width, Height;			/* dimensions of the entire box */
    WORD RelLeft, RelTop;		/* for Pointer relativity offsets */

    struct Gadget *ReqGadget;		/* pointer to a list of Gadgets */
    struct Border *ReqBorder;		/* the box's border */
    struct IntuiText *ReqText;		/* the box's text */
    UWORD Flags;			/* see definitions below */

    /* pen number for back-plane fill before draws */
    UBYTE BackFill;
    /* Layer in place of clip rect	*/
    struct Layer *ReqLayer;

    UBYTE ReqPad1[32];

    /* If the BitMap plane pointers are non-zero, this tells the system
     * that the image comes pre-drawn (if the appliprog wants to define
     * its own box, in any shape or size it wants!);  this is OK by
     * Intuition as long as there's a good correspondence between
     * the image and the specified Gadgets
     */
    struct BitMap *ImageBMap;	/* points to the BitMap of PREDRAWN imagery */
    struct Window *RWindow;	/* added.  points back to Window */

    struct Image  *ReqImage;	/* new for V36: drawn if USEREQIMAGE set */

    UBYTE ReqPad2[32];
};


/* FLAGS SET BY THE APPLIPROG */
#define POINTREL	0x0001
			  /* if POINTREL set, TopLeft is relative to pointer
			   * for DMRequester, relative to window center
			   * for Request().
			   */
#define PREDRAWN	0x0002
	/* set if Requester.ImageBMap points to predrawn Requester imagery */
#define NOISYREQ	0x0004
	/* if you don't want requester to filter input	   */
#define SIMPLEREQ	0x0010
	/* to use SIMPLEREFRESH layer (recommended)	*/

/* New for V36		*/
#define USEREQIMAGE	0x0020
	/*  render linked list ReqImage after BackFill
	 * but before gadgets and text
	 */
#define NOREQBACKFILL	0x0040
	/* don't bother filling requester with Requester.BackFill pen	*/


/* FLAGS SET BY INTUITION */
#define REQOFFWINDOW	0x1000	/* part of one of the Gadgets was offwindow */
#define REQACTIVE	0x2000	/* this requester is active */
#define SYSREQUEST	0x4000	/* (unused) this requester caused by system */
#define DEFERREFRESH	0x8000	/* this Requester stops a Refresh broadcast */






/* ======================================================================== */
/* === Gadget ============================================================= */
/* ======================================================================== */
struct Gadget
{
    struct Gadget *NextGadget;	/* next gadget in the list */

    WORD LeftEdge, TopEdge;	/* "hit box" of gadget */
    WORD Width, Height;		/* "hit box" of gadget */

    UWORD Flags;		/* see below for list of defines */

    UWORD Activation;		/* see below for list of defines */

    UWORD GadgetType;		/* see below for defines */

    /* appliprog can specify that the Gadget be rendered as either as Border
     * or an Image.  This variable points to which (or equals NULL if there's
     * nothing to be rendered about this Gadget)
     */
    APTR GadgetRender;

    /* appliprog can specify "highlighted" imagery rather than algorithmic
     * this can point to either Border or Image data
     */
    APTR SelectRender;

    struct IntuiText *GadgetText;   /* text for this gadget */

    /* MutualExclude, never implemented, is now declared obsolete.
     * There are published examples of implementing a more general
     * and practical exclusion in your applications.
     *
     * Starting with V36, this field is used to point to a hook
     * for a custom gadget.
     *
     * Programs using this field for their own processing will
     * continue to work, as long as they don't try the
     * trick with custom gadgets.
     */
    LONG MutualExclude;  /* obsolete */

    /* pointer to a structure of special data required by Proportional,
     * String and Integer Gadgets
     */
    APTR SpecialInfo;

    UWORD GadgetID;	/* user-definable ID field */
    APTR UserData;	/* ptr to general purpose User data (ignored by In) */
};


struct ExtGadget
{
    /* The first fields match struct Gadget exactly */
    struct ExtGadget *NextGadget; /* Matches struct Gadget */
    WORD LeftEdge, TopEdge;	  /* Matches struct Gadget */
    WORD Width, Height;		  /* Matches struct Gadget */
    UWORD Flags;		  /* Matches struct Gadget */
    UWORD Activation;		  /* Matches struct Gadget */
    UWORD GadgetType;		  /* Matches struct Gadget */
    APTR GadgetRender;		  /* Matches struct Gadget */
    APTR SelectRender;		  /* Matches struct Gadget */
    struct IntuiText *GadgetText; /* Matches struct Gadget */
    LONG MutualExclude;		  /* Matches struct Gadget */
    APTR SpecialInfo;		  /* Matches struct Gadget */
    UWORD GadgetID;		  /* Matches struct Gadget */
    APTR UserData;		  /* Matches struct Gadget */

    /* These fields only exist under V39 and only if GFLG_EXTENDED is set */
    ULONG MoreFlags;		/* see GMORE_ flags below */
    WORD BoundsLeftEdge;	/* Bounding extent for gadget, valid   */
    WORD BoundsTopEdge;		/* only if GMORE_BOUNDS is set.  The   */
    WORD BoundsWidth;		/* GFLG_RELxxx flags affect these      */
    WORD BoundsHeight;		/* coordinates as well.	       */
};


/* --- Gadget.Flags values	--- */
/* combinations in these bits describe the highlight technique to be used */
#define GFLG_GADGHIGHBITS 0x0003
#define GFLG_GADGHCOMP	  0x0000  /* Complement the select box */
#define GFLG_GADGHBOX	  0x0001  /* Draw a box around the image */
#define GFLG_GADGHIMAGE	  0x0002  /* Blast in this alternate image */
#define GFLG_GADGHNONE	  0x0003  /* don't highlight */

#define GFLG_GADGIMAGE		  0x0004  /* set if GadgetRender and SelectRender
				   * point to an Image structure, clear
				   * if they point to Border structures
				   */

/* combinations in these next two bits specify to which corner the gadget's
 *  Left & Top coordinates are relative.  If relative to Top/Left,
 *  these are "normal" coordinates (everything is relative to something in
 *  this universe).
 *
 * Gadget positions and dimensions are relative to the window or
 * requester which contains the gadget
 */
#define GFLG_RELBOTTOM	  0x0008  /* vert. pos. is relative to bottom edge */
#define GFLG_RELRIGHT	  0x0010  /* horiz. pos. is relative to right edge */
#define GFLG_RELWIDTH	  0x0020  /* width is relative to req/window	*/
#define GFLG_RELHEIGHT	  0x0040  /* height is relative to req/window	*/

/* New for V39: GFLG_RELSPECIAL allows custom gadget implementors to
 * make gadgets whose position and size depend in an arbitrary way
 * on their window's dimensions.  The GM_LAYOUT method will be invoked
 * for such a gadget (or any other GREL_xxx gadget) at suitable times,
 * such as when the window opens or the window's size changes.
 */
#define GFLG_RELSPECIAL	  0x4000  /* custom gadget has special relativity.
				   * Gadget box values are absolutes, but
				   * can be changed via the GM_LAYOUT method.
				   */
#define GFLG_SELECTED	  0x0080  /* you may initialize and look at this	*/

/* the GFLG_DISABLED flag is initialized by you and later set by Intuition
 * according to your calls to On/OffGadget().  It specifies whether or not
 * this Gadget is currently disabled from being selected
 */
#define GFLG_DISABLED	  0x0100

/* These flags specify the type of text field that Gadget.GadgetText
 * points to.  In all normal (pre-V36) gadgets which you initialize
 * this field should always be zero.  Some types of gadget objects
 * created from classes will use these fields to keep track of
 * types of labels/contents that different from IntuiText, but are
 * stashed in GadgetText.
 */

#define GFLG_LABELMASK	  0x3000
#define GFLG_LABELITEXT	  0x0000  /* GadgetText points to IntuiText	*/
#define	GFLG_LABELSTRING  0x1000  /* GadgetText points to (UBYTE *)	*/
#define GFLG_LABELIMAGE	  0x2000  /* GadgetText points to Image (object)	*/

/* New for V37: GFLG_TABCYCLE */
#define GFLG_TABCYCLE	  0x0200  /* (string or custom) gadget participates in
				   * cycling activation with Tab or Shift-Tab
				   */
/* New for V37: GFLG_STRINGEXTEND.  We discovered that V34 doesn't properly
 * ignore the value we had chosen for the Gadget->Activation flag
 * GACT_STRINGEXTEND.  NEVER SET THAT FLAG WHEN RUNNING UNDER V34.
 * The Gadget->Flags bit GFLG_STRINGEXTEND is provided as a synonym which is
 * safe under V34, and equivalent to GACT_STRINGEXTEND under V37.
 * (Note that the two flags are not numerically equal)
 */
#define GFLG_STRINGEXTEND 0x0400  /* this String Gadget has StringExtend	*/

/* New for V39: GFLG_IMAGEDISABLE.  This flag is automatically set if
 * the custom image of this gadget knows how to do disabled rendering
 * (more specifically, if its IA_SupportsDisable attribute is TRUE).
 * Intuition uses this to defer the ghosting to the image-class,
 * instead of doing it itself (the old compatible way).
 * Do not set this flag yourself - Intuition will do it for you.
 */

#define GFLG_IMAGEDISABLE 0x0800  /* Gadget's image knows how to do disabled
				   * rendering
				   */

/* New for V39:  If set, this bit means that the Gadget is actually
 * a struct ExtGadget, with new fields and flags.  All V39 boopsi
 * gadgets are ExtGadgets.  Never ever attempt to read the extended
 * fields of a gadget if this flag is not set.
 */
#define GFLG_EXTENDED	  0x8000  /* Gadget is extended */

/* ---	Gadget.Activation flag values	--- */
/* Set GACT_RELVERIFY if you want to verify that the pointer was still over
 * the gadget when the select button was released.  Will cause
 * an IDCMP_GADGETUP message to be sent if so.
 */
#define GACT_RELVERIFY	  0x0001

/* the flag GACT_IMMEDIATE, when set, informs the caller that the gadget
 *  was activated when it was activated.  This flag works in conjunction with
 *  the GACT_RELVERIFY flag
 */
#define GACT_IMMEDIATE	  0x0002

/* the flag GACT_ENDGADGET, when set, tells the system that this gadget,
 * when selected, causes the Requester to be ended.  Requesters
 * that are ended are erased and unlinked from the system.
 */
#define GACT_ENDGADGET	  0x0004

/* the GACT_FOLLOWMOUSE flag, when set, specifies that you want to receive
 * reports on mouse movements while this gadget is active.
 * You probably want to set the GACT_IMMEDIATE flag when using
 * GACT_FOLLOWMOUSE, since that's the only reasonable way you have of
 * learning why Intuition is suddenly sending you a stream of mouse
 * movement events.  If you don't set GACT_RELVERIFY, you'll get at
 * least one Mouse Position event.
 * Note: boolean FOLLOWMOUSE gadgets require GACT_RELVERIFY to get
 * _any_ mouse movement events (this unusual behavior is a compatibility
 * hold-over from the old days).
 */
#define GACT_FOLLOWMOUSE  0x0008

/* if any of the BORDER flags are set in a Gadget that's included in the
 * Gadget list when a Window is opened, the corresponding Border will
 * be adjusted to make room for the Gadget
 */
#define GACT_RIGHTBORDER  0x0010
#define GACT_LEFTBORDER	  0x0020
#define GACT_TOPBORDER	  0x0040
#define GACT_BOTTOMBORDER 0x0080
#define GACT_BORDERSNIFF  0x8000  /* neither set nor rely on this bit	*/

#define GACT_TOGGLESELECT 0x0100  /* this bit for toggle-select mode */
#define GACT_BOOLEXTEND	  0x2000  /* this Boolean Gadget has a BoolInfo	*/

/* should properly be in StringInfo, but aren't	*/
#define GACT_STRINGLEFT	  0x0000  /* NOTE WELL: that this has value zero	*/
#define GACT_STRINGCENTER 0x0200
#define GACT_STRINGRIGHT  0x0400
#define GACT_LONGINT	  0x0800  /* this String Gadget is for Long Ints	*/
#define GACT_ALTKEYMAP	  0x1000  /* this String has an alternate keymap	*/
#define GACT_STRINGEXTEND 0x2000  /* this String Gadget has StringExtend	*/
				  /* NOTE: NEVER SET GACT_STRINGEXTEND IF YOU
				   * ARE RUNNING ON LESS THAN V36!  SEE
				   * GFLG_STRINGEXTEND (ABOVE) INSTEAD
				   */

#define GACT_ACTIVEGADGET 0x4000  /* this gadget is "active".  This flag
				   * is maintained by Intuition, and you
				   * cannot count on its value persisting
				   * while you do something on your program's
				   * task.  It can only be trusted by
				   * people implementing custom gadgets
				   */

/* note 0x8000 is used above (GACT_BORDERSNIFF);
 * all Activation flags defined */

/* --- GADGET TYPES ------------------------------------------------------- */
/* These are the Gadget Type definitions for the variable GadgetType
 * gadget number type MUST start from one.  NO TYPES OF ZERO ALLOWED.
 * first comes the mask for Gadget flags reserved for Gadget typing
 */
#define GTYP_GADGETTYPE	0xFC00	/* all Gadget Global Type flags (padded) */

#define GTYP_SCRGADGET		0x4000	/* 1 = ScreenGadget, 0 = WindowGadget */
#define GTYP_GZZGADGET		0x2000	/* 1 = for WFLG_GIMMEZEROZERO borders */
#define GTYP_REQGADGET		0x1000	/* 1 = this is a Requester Gadget */

/* GTYP_SYSGADGET means that Intuition ALLOCATED the gadget.
 * GTYP_SYSTYPEMASK is the mask you can apply to tell what type of
 * system-gadget it is.  The possible types follow.
 */
#define GTYP_SYSGADGET		0x8000
#define GTYP_SYSTYPEMASK	0x00F0

/* These definitions describe system gadgets in V36 and higher: */
#define GTYP_SIZING		0x0010	/* Window sizing gadget */
#define GTYP_WDRAGGING		0x0020	/* Window drag bar */
#define GTYP_SDRAGGING		0x0030	/* Screen drag bar */
#define GTYP_WDEPTH		0x0040	/* Window depth gadget */
#define GTYP_SDEPTH		0x0050	/* Screen depth gadget */
#define GTYP_WZOOM		0x0060	/* Window zoom gadget */
#define GTYP_SUNUSED		0x0070	/* Unused screen gadget */
#define GTYP_CLOSE		0x0080	/* Window close gadget */

/* These definitions describe system gadgets prior to V36: */
#define GTYP_WUPFRONT		GTYP_WDEPTH	/* Window to-front gadget */
#define GTYP_SUPFRONT		GTYP_SDEPTH	/* Screen to-front gadget */
#define GTYP_WDOWNBACK		GTYP_WZOOM	/* Window to-back gadget */
#define GTYP_SDOWNBACK		GTYP_SUNUSED	/* Screen to-back gadget */

/* GTYP_GTYPEMASK is a mask you can apply to tell what class
 * of gadget this is.  The possible classes follow.
 */
#define GTYP_GTYPEMASK		0x0007

#define GTYP_BOOLGADGET		0x0001
#define GTYP_GADGET0002		0x0002
#define GTYP_PROPGADGET		0x0003
#define GTYP_STRGADGET		0x0004
#define GTYP_CUSTOMGADGET	0x0005

/* This bit in GadgetType is reserved for undocumented internal use
 * by the Gadget Toolkit, and cannot be used nor relied on by
 * applications:	0x0100
 */

/* New for V39.  Gadgets which have the GFLG_EXTENDED flag set are
 * actually ExtGadgets, which have more flags.	The GMORE_xxx
 * identifiers describe those flags.  For GMORE_SCROLLRASTER, see
 * important information in the ScrollWindowRaster() autodoc.
 * NB: GMORE_SCROLLRASTER must be set before the gadget is
 * added to a window.
 */
#define GMORE_BOUNDS	   0x00000001L /* ExtGadget has valid Bounds */
#define GMORE_GADGETHELP   0x00000002L /* This gadget responds to gadget help */
#define GMORE_SCROLLRASTER 0x00000004L /* This (custom) gadget uses ScrollRaster */


/* ======================================================================== */
/* === BoolInfo======================================================= */
/* ======================================================================== */
/* This is the special data needed by an Extended Boolean Gadget
 * Typically this structure will be pointed to by the Gadget field SpecialInfo
 */
struct BoolInfo
{
    UWORD  Flags;	/* defined below */
    UWORD  *Mask;	/* bit mask for highlighting and selecting
			 * mask must follow the same rules as an Image
			 * plane.  Its width and height are determined
			 * by the width and height of the gadget's
			 * select box. (i.e. Gadget.Width and .Height).
			 */
    ULONG  Reserved;	/* set to 0	*/
};

/* set BoolInfo.Flags to this flag bit.
 * in the future, additional bits might mean more stuff hanging
 * off of BoolInfo.Reserved.
 */
#define BOOLMASK	0x0001	/* extension is for masked gadget */

/* ======================================================================== */
/* === PropInfo =========================================================== */
/* ======================================================================== */
/* this is the special data required by the proportional Gadget
 * typically, this data will be pointed to by the Gadget variable SpecialInfo
 */
struct PropInfo
{
    UWORD Flags;	/* general purpose flag bits (see defines below) */

    /* You initialize the Pot variables before the Gadget is added to
     * the system.  Then you can look here for the current settings
     * any time, even while User is playing with this Gadget.  To
     * adjust these after the Gadget is added to the System, use
     * ModifyProp();  The Pots are the actual proportional settings,
     * where a value of zero means zero and a value of MAXPOT means
     * that the Gadget is set to its maximum setting.
     */
    UWORD HorizPot;	/* 16-bit FixedPoint horizontal quantity percentage */
    UWORD VertPot;	/* 16-bit FixedPoint vertical quantity percentage */

    /* the 16-bit FixedPoint Body variables describe what percentage of
     * the entire body of stuff referred to by this Gadget is actually
     * shown at one time.  This is used with the AUTOKNOB routines,
     * to adjust the size of the AUTOKNOB according to how much of
     * the data can be seen.  This is also used to decide how far
     * to advance the Pots when User hits the Container of the Gadget.
     * For instance, if you were controlling the display of a 5-line
     * Window of text with this Gadget, and there was a total of 15
     * lines that could be displayed, you would set the VertBody value to
     *	   (MAXBODY / (TotalLines / DisplayLines)) = MAXBODY / 3.
     * Therefore, the AUTOKNOB would fill 1/3 of the container, and
     * if User hits the Cotainer outside of the knob, the pot would
     * advance 1/3 (plus or minus) If there's no body to show, or
     * the total amount of displayable info is less than the display area,
     * set the Body variables to the MAX.  To adjust these after the
     * Gadget is added to the System, use ModifyProp();
     */
    UWORD HorizBody;		/* horizontal Body */
    UWORD VertBody;		/* vertical Body */

    /* these are the variables that Intuition sets and maintains */
    UWORD CWidth;	/* Container width (with any relativity absoluted) */
    UWORD CHeight;	/* Container height (with any relativity absoluted) */
    UWORD HPotRes, VPotRes;	/* pot increments */
    UWORD LeftBorder;		/* Container borders */
    UWORD TopBorder;		/* Container borders */
};


/* --- FLAG BITS ---------------------------------------------------------- */
#define AUTOKNOB	0x0001	/* this flag sez:  gimme that old auto-knob */
/* NOTE: if you do not use an AUTOKNOB for a proportional gadget,
 * you are currently limited to using a single Image of your own
 * design: Intuition won't handle a linked list of images as
 * a proportional gadget knob.
 */

#define FREEHORIZ	0x0002	/* if set, the knob can move horizontally */
#define FREEVERT	0x0004	/* if set, the knob can move vertically */
#define PROPBORDERLESS	0x0008	/* if set, no border will be rendered */
#define KNOBHIT		0x0100	/* set when this Knob is hit */
#define PROPNEWLOOK	0x0010	/* set this if you want to get the new
				 * V36 look
				 */

#define KNOBHMIN	6	/* minimum horizontal size of the Knob */
#define KNOBVMIN	4	/* minimum vertical size of the Knob */
#define MAXBODY		0xFFFF	/* maximum body value */
#define MAXPOT			0xFFFF	/* maximum pot value */


/* ======================================================================== */
/* === StringInfo ========================================================= */
/* ======================================================================== */
/* this is the special data required by the string Gadget
 * typically, this data will be pointed to by the Gadget variable SpecialInfo
 */
struct StringInfo
{
    /* you initialize these variables, and then Intuition maintains them */
    UBYTE *Buffer;	/* the buffer containing the start and final string */
    UBYTE *UndoBuffer;	/* optional buffer for undoing current entry */
    WORD BufferPos;	/* character position in Buffer */
    WORD MaxChars;	/* max number of chars in Buffer (including NULL) */
    WORD DispPos;	/* Buffer position of first displayed character */

    /* Intuition initializes and maintains these variables for you */
    WORD UndoPos;	/* character position in the undo buffer */
    WORD NumChars;	/* number of characters currently in Buffer */
    WORD DispCount;	/* number of whole characters visible in Container */
    WORD CLeft, CTop;	/* topleft offset of the container */

    /* This unused field is changed to allow extended specification
     * of string gadget parameters.  It is ignored unless the flag
     * GACT_STRINGEXTEND is set in the Gadget's Activation field
     * or the GFLG_STRINGEXTEND flag is set in the Gadget Flags field.
     * (See GFLG_STRINGEXTEND for an important note)
     */
    /* struct Layer *LayerPtr;	--- obsolete --- */
    struct StringExtend *Extension;

    /* you can initialize this variable before the gadget is submitted to
     * Intuition, and then examine it later to discover what integer
     * the user has entered (if the user never plays with the gadget,
     * the value will be unchanged from your initial setting)
     */
    LONG LongInt;

    /* If you want this Gadget to use your own Console keymapping, you
     * set the GACT_ALTKEYMAP bit in the Activation flags of the Gadget,
     * and then set this variable to point to your keymap.  If you don't
     * set the GACT_ALTKEYMAP, you'll get the standard ASCII keymapping.
     */
    struct KeyMap *AltKeyMap;
};

/* ======================================================================== */
/* === IntuiText ========================================================== */
/* ======================================================================== */
/* IntuiText is a series of strings that start with a location
 *  (always relative to the upper-left corner of something) and then the
 *  text of the string.  The text is null-terminated.
 */
struct IntuiText
{
    UBYTE FrontPen, BackPen;	/* the pen numbers for the rendering */
    UBYTE DrawMode;		/* the mode for rendering the text */
    WORD LeftEdge;		/* relative start location for the text */
    WORD TopEdge;		/* relative start location for the text */
    struct TextAttr *ITextFont;	/* if NULL, you accept the default */
    UBYTE *IText;		/* pointer to null-terminated text */
    struct IntuiText *NextText; /* pointer to another IntuiText to render */
};






/* ======================================================================== */
/* === Border ============================================================= */
/* ======================================================================== */
/* Data type Border, used for drawing a series of lines which is intended for
 *  use as a border drawing, but which may, in fact, be used to render any
 *  arbitrary vector shape.
 *  The routine DrawBorder sets up the RastPort with the appropriate
 *  variables, then does a Move to the first coordinate, then does Draws
 *  to the subsequent coordinates.
 *  After all the Draws are done, if NextBorder is non-zero we call DrawBorder
 *  on NextBorder
 */
struct Border
{
    WORD LeftEdge, TopEdge;	/* initial offsets from the origin */
    UBYTE FrontPen, BackPen;	/* pens numbers for rendering */
    UBYTE DrawMode;		/* mode for rendering */
    BYTE Count;			/* number of XY pairs */
    WORD *XY;			/* vector coordinate pairs rel to LeftTop */
    struct Border *NextBorder;	/* pointer to any other Border too */
};






/* ======================================================================== */
/* === Image ============================================================== */
/* ======================================================================== */
/* This is a brief image structure for very simple transfers of
 * image data to a RastPort
 */
struct Image
{
    WORD LeftEdge;		/* starting offset relative to some origin */
    WORD TopEdge;		/* starting offsets relative to some origin */
    WORD Width;			/* pixel size (though data is word-aligned) */
    WORD Height;
    WORD Depth;			/* >= 0, for images you create		*/
    UWORD *ImageData;		/* pointer to the actual word-aligned bits */

    /* the PlanePick and PlaneOnOff variables work much the same way as the
     * equivalent GELS Bob variables.  It's a space-saving
     * mechanism for image data.  Rather than defining the image data
     * for every plane of the RastPort, you need define data only
     * for the planes that are not entirely zero or one.  As you
     * define your Imagery, you will often find that most of the planes
     * ARE just as color selectors.  For instance, if you're designing
     * a two-color Gadget to use colors one and three, and the Gadget
     * will reside in a five-plane display, bit plane zero of your
     * imagery would be all ones, bit plane one would have data that
     * describes the imagery, and bit planes two through four would be
     * all zeroes.  Using these flags avoids wasting all
     * that memory in this way:  first, you specify which planes you
     * want your data to appear in using the PlanePick variable.  For
     * each bit set in the variable, the next "plane" of your image
     * data is blitted to the display.	For each bit clear in this
     * variable, the corresponding bit in PlaneOnOff is examined.
     * If that bit is clear, a "plane" of zeroes will be used.
     * If the bit is set, ones will go out instead.  So, for our example:
     *	 Gadget.PlanePick = 0x02;
     *	 Gadget.PlaneOnOff = 0x01;
     * Note that this also allows for generic Gadgets, like the
     * System Gadgets, which will work in any number of bit planes.
     * Note also that if you want an Image that is only a filled
     * rectangle, you can get this by setting PlanePick to zero
     * (pick no planes of data) and set PlaneOnOff to describe the pen
     * color of the rectangle.
     *
     * NOTE:  Intuition relies on PlanePick to know how many planes
     * of data are found in ImageData.	There should be no more
     * '1'-bits in PlanePick than there are planes in ImageData.
     */
    UBYTE PlanePick, PlaneOnOff;

    /* if the NextImage variable is not NULL, Intuition presumes that
     * it points to another Image structure with another Image to be
     * rendered
     */
    struct Image *NextImage;
};






/* ======================================================================== */
/* === IntuiMessage ======================================================= */
/* ======================================================================== */
struct IntuiMessage
{
    struct Message ExecMessage;

    /* the Class bits correspond directly with the IDCMP Flags, except for the
     * special bit IDCMP_LONELYMESSAGE (defined below)
     */
    ULONG Class;

    /* the Code field is for special values like MENU number */
    UWORD Code;

    /* the Qualifier field is a copy of the current InputEvent's Qualifier */
    UWORD Qualifier;

    /* IAddress contains particular addresses for Intuition functions, like
     * the pointer to the Gadget or the Screen
     */
    APTR IAddress;

    /* when getting mouse movement reports, any event you get will have the
     * the mouse coordinates in these variables.  the coordinates are relative
     * to the upper-left corner of your Window (WFLG_GIMMEZEROZERO
     * notwithstanding).  If IDCMP_DELTAMOVE is set, these values will
     * be deltas from the last reported position.
     */
    WORD MouseX, MouseY;

    /* the time values are copies of the current system clock time.  Micros
     * are in units of microseconds, Seconds in seconds.
     */
    ULONG Seconds, Micros;

    /* the IDCMPWindow variable will always have the address of the Window of
     * this IDCMP
     */
    struct Window *IDCMPWindow;

    /* system-use variable */
    struct IntuiMessage *SpecialLink;
};

/* New for V39:
 * All IntuiMessages are now slightly extended.  The ExtIntuiMessage
 * structure has an additional field for tablet data, which is usually
 * NULL.  If a tablet driver which is sending IESUBCLASS_NEWTABLET
 * events is installed in the system, windows with the WA_TabletMessages
 * property set will find that eim_TabletData points to the TabletData
 * structure.  Applications must first check that this field is non-NULL;
 * it will be NULL for certain kinds of message, including mouse activity
 * generated from other than the tablet (i.e. the keyboard equivalents
 * or the mouse itself).
 *
 * NEVER EVER examine any extended fields when running under pre-V39!
 *
 * NOTE: This structure is subject to grow in the future.  Making
 * assumptions about its size is A BAD IDEA.
 */

struct ExtIntuiMessage
{
    struct IntuiMessage eim_IntuiMessage;
    struct TabletData *eim_TabletData;
};

/* --- IDCMP Classes ------------------------------------------------------ */
/* Please refer to the Autodoc for OpenWindow() and to the Rom Kernel
 * Manual for full details on the IDCMP classes.
 */
#define IDCMP_SIZEVERIFY	0x00000001L
#define IDCMP_NEWSIZE		0x00000002L
#define IDCMP_REFRESHWINDOW	0x00000004L
#define IDCMP_MOUSEBUTTONS	0x00000008L
#define IDCMP_MOUSEMOVE		0x00000010L
#define IDCMP_GADGETDOWN	0x00000020L
#define IDCMP_GADGETUP		0x00000040L
#define IDCMP_REQSET		0x00000080L
#define IDCMP_MENUPICK		0x00000100L
#define IDCMP_CLOSEWINDOW	0x00000200L
#define IDCMP_RAWKEY		0x00000400L
#define IDCMP_REQVERIFY		0x00000800L
#define IDCMP_REQCLEAR		0x00001000L
#define IDCMP_MENUVERIFY	0x00002000L
#define IDCMP_NEWPREFS		0x00004000L
#define IDCMP_DISKINSERTED	0x00008000L
#define IDCMP_DISKREMOVED	0x00010000L
#define IDCMP_WBENCHMESSAGE	0x00020000L  /*	System use only		*/
#define IDCMP_ACTIVEWINDOW	0x00040000L
#define IDCMP_INACTIVEWINDOW	0x00080000L
#define IDCMP_DELTAMOVE		0x00100000L
#define IDCMP_VANILLAKEY	0x00200000L
#define IDCMP_INTUITICKS	0x00400000L
/*  for notifications from "boopsi" gadgets	*/
#define IDCMP_IDCMPUPDATE	0x00800000L  /* new for V36	*/
/* for getting help key report during menu session	*/
#define IDCMP_MENUHELP		0x01000000L  /* new for V36	*/
/* for notification of any move/size/zoom/change window		*/
#define IDCMP_CHANGEWINDOW	0x02000000L  /* new for V36	*/
#define IDCMP_GADGETHELP	0x04000000L  /* new for V39	*/

/* NOTEZ-BIEN:				0x80000000 is reserved for internal use   */

/* the IDCMP Flags do not use this special bit, which is cleared when
 * Intuition sends its special message to the Task, and set when Intuition
 * gets its Message back from the Task.  Therefore, I can check here to
 * find out fast whether or not this Message is available for me to send
 */
#define IDCMP_LONELYMESSAGE	0x80000000L


/* --- IDCMP Codes -------------------------------------------------------- */
/* This group of codes is for the IDCMP_CHANGEWINDOW message */
#define CWCODE_MOVESIZE	0x0000	/* Window was moved and/or sized */
#define CWCODE_DEPTH	0x0001	/* Window was depth-arranged (new for V39) */

/* This group of codes is for the IDCMP_MENUVERIFY message */
#define MENUHOT		0x0001	/* IntuiWants verification or MENUCANCEL    */
#define MENUCANCEL	0x0002	/* HOT Reply of this cancels Menu operation */
#define MENUWAITING	0x0003	/* Intuition simply wants a ReplyMsg() ASAP */

/* These are internal tokens to represent state of verification attempts
 * shown here as a clue.
 */
#define OKOK		MENUHOT	/* guy didn't care			*/
#define OKABORT		0x0004	/* window rendered question moot	*/
#define OKCANCEL	MENUCANCEL /* window sent cancel reply		*/

/* This group of codes is for the IDCMP_WBENCHMESSAGE messages */
#define WBENCHOPEN	0x0001
#define WBENCHCLOSE	0x0002


/* A data structure common in V36 Intuition processing	*/
struct IBox
{
    WORD Left;
    WORD Top;
    WORD Width;
    WORD Height;
};



/* ======================================================================== */
/* === Window ============================================================= */
/* ======================================================================== */
struct Window
{
    struct Window *NextWindow;		/* for the linked list in a screen */

    WORD LeftEdge, TopEdge;		/* screen dimensions of window */
    WORD Width, Height;			/* screen dimensions of window */

    WORD MouseY, MouseX;		/* relative to upper-left of window */

    WORD MinWidth, MinHeight;		/* minimum sizes */
    UWORD MaxWidth, MaxHeight;		/* maximum sizes */

    ULONG Flags;			/* see below for defines */

    struct Menu *MenuStrip;		/* the strip of Menu headers */

    UBYTE *Title;			/* the title text for this window */

    struct Requester *FirstRequest;	/* all active Requesters */

    struct Requester *DMRequest;	/* double-click Requester */

    WORD ReqCount;			/* count of reqs blocking Window */

    struct Screen *WScreen;		/* this Window's Screen */
    struct RastPort *RPort;		/* this Window's very own RastPort */

    /* the border variables describe the window border.  If you specify
     * WFLG_GIMMEZEROZERO when you open the window, then the upper-left of
     * the ClipRect for this window will be upper-left of the BitMap (with
     * correct offsets when in SuperBitMap mode; you MUST select
     * WFLG_GIMMEZEROZERO when using SuperBitMap).  If you don't specify
     * ZeroZero, then you save memory (no allocation of RastPort, Layer,
     * ClipRect and associated Bitmaps), but you also must offset all your
     * writes by BorderTop, BorderLeft and do your own mini-clipping to
     * prevent writing over the system gadgets
     */
    BYTE BorderLeft, BorderTop, BorderRight, BorderBottom;
    struct RastPort *BorderRPort;


    /* You supply a linked-list of Gadgets for your Window.
     * This list DOES NOT include system gadgets.  You get the standard
     * window system gadgets by setting flag-bits in the variable Flags (see
     * the bit definitions below)
     */
    struct Gadget *FirstGadget;

    /* these are for opening/closing the windows */
    struct Window *Parent, *Descendant;

    /* sprite data information for your own Pointer
     * set these AFTER you Open the Window by calling SetPointer()
     */
    UWORD *Pointer;	/* sprite data */
    BYTE PtrHeight;	/* sprite height (not including sprite padding) */
    BYTE PtrWidth;	/* sprite width (must be less than or equal to 16) */
    BYTE XOffset, YOffset;	/* sprite offsets */

    /* the IDCMP Flags and User's and Intuition's Message Ports */
    ULONG IDCMPFlags;	/* User-selected flags */
    struct MsgPort *UserPort, *WindowPort;
    struct IntuiMessage *MessageKey;

    UBYTE DetailPen, BlockPen;	/* for bar/border/gadget rendering */

    /* the CheckMark is a pointer to the imagery that will be used when
     * rendering MenuItems of this Window that want to be checkmarked
     * if this is equal to NULL, you'll get the default imagery
     */
    struct Image *CheckMark;

    UBYTE *ScreenTitle;	/* if non-null, Screen title when Window is active */

    /* These variables have the mouse coordinates relative to the
     * inner-Window of WFLG_GIMMEZEROZERO Windows.  This is compared with the
     * MouseX and MouseY variables, which contain the mouse coordinates
     * relative to the upper-left corner of the Window, WFLG_GIMMEZEROZERO
     * notwithstanding
     */
    WORD GZZMouseX;
    WORD GZZMouseY;
    /* these variables contain the width and height of the inner-Window of
     * WFLG_GIMMEZEROZERO Windows
     */
    WORD GZZWidth;
    WORD GZZHeight;

    UBYTE *ExtData;

    BYTE *UserData;	/* general-purpose pointer to User data extension */

    /** 11/18/85: this pointer keeps a duplicate of what
     * Window.RPort->Layer is _supposed_ to be pointing at
     */
    struct Layer *WLayer;

    /* NEW 1.2: need to keep track of the font that
     * OpenWindow opened, in case user SetFont's into RastPort
     */
    struct TextFont *IFont;

    /* (V36) another flag word (the Flags field is used up).
     * At present, all flag values are system private.
     * Until further notice, you may not change nor use this field.
     */
    ULONG	MoreFlags;

    /**** Data beyond this point are Intuition Private.  DO NOT USE ****/
};


/* --- Flags requested at OpenWindow() time by the application --------- */
#define WFLG_SIZEGADGET	    0x00000001L	/* include sizing system-gadget? */
#define WFLG_DRAGBAR	    0x00000002L	/* include dragging system-gadget? */
#define WFLG_DEPTHGADGET    0x00000004L	/* include depth arrangement gadget? */
#define WFLG_CLOSEGADGET    0x00000008L	/* include close-box system-gadget? */

#define WFLG_SIZEBRIGHT	    0x00000010L	/* size gadget uses right border */
#define WFLG_SIZEBBOTTOM    0x00000020L	/* size gadget uses bottom border */

/* --- refresh modes ------------------------------------------------------ */
/* combinations of the WFLG_REFRESHBITS select the refresh type */
#define WFLG_REFRESHBITS    0x000000C0L
#define WFLG_SMART_REFRESH  0x00000000L
#define WFLG_SIMPLE_REFRESH 0x00000040L
#define WFLG_SUPER_BITMAP   0x00000080L
#define WFLG_OTHER_REFRESH  0x000000C0L

#define WFLG_BACKDROP	    0x00000100L	/* this is a backdrop window */

#define WFLG_REPORTMOUSE    0x00000200L	/* to hear about every mouse move */

#define WFLG_GIMMEZEROZERO  0x00000400L	/* a GimmeZeroZero window	*/

#define WFLG_BORDERLESS	    0x00000800L	/* to get a Window sans border */

#define WFLG_ACTIVATE	    0x00001000L	/* when Window opens, it's Active */

/* --- Other User Flags --------------------------------------------------- */
#define WFLG_RMBTRAP	    0x00010000L	/* Catch RMB events for your own */
#define WFLG_NOCAREREFRESH  0x00020000L	/* not to be bothered with REFRESH */

/* - V36 new Flags which the programmer may specify in NewWindow.Flags	*/
#define WFLG_NW_EXTENDED    0x00040000L	/* extension data provided	*/
					/* see struct ExtNewWindow	*/

/* - V39 new Flags which the programmer may specify in NewWindow.Flags	*/
#define WFLG_NEWLOOKMENUS   0x00200000L	/* window has NewLook menus	*/


/* These flags are set only by Intuition.  YOU MAY NOT SET THEM YOURSELF! */
#define WFLG_WINDOWACTIVE   0x00002000L	/* this window is the active one */
#define WFLG_INREQUEST	    0x00004000L	/* this window is in request mode */
#define WFLG_MENUSTATE	    0x00008000L	/* Window is active with Menus on */
#define WFLG_WINDOWREFRESH  0x01000000L	/* Window is currently refreshing */
#define WFLG_WBENCHWINDOW   0x02000000L	/* WorkBench tool ONLY Window */
#define WFLG_WINDOWTICKED   0x04000000L	/* only one timer tick at a time */

/* V36 and higher flags to be set only by Intuition: */
#define WFLG_VISITOR	    0x08000000L	/* visitor window		*/
#define WFLG_ZOOMED	    0x10000000L	/* identifies "zoom state"	*/
#define WFLG_HASZOOM	    0x20000000L	/* window has a zoom gadget	*/


/* --- Other Window Values ---------------------------------------------- */
#define DEFAULTMOUSEQUEUE	(5)	/* no more mouse messages	*/

/* --- see struct IntuiMessage for the IDCMP Flag definitions ------------- */


/* ======================================================================== */
/* === NewWindow ========================================================== */
/* ======================================================================== */
/*
 * Note that the new extension fields have been removed.  Use ExtNewWindow
 * structure below to make use of these fields
 */
struct NewWindow
{
    WORD LeftEdge, TopEdge;		/* screen dimensions of window */
    WORD Width, Height;			/* screen dimensions of window */

    UBYTE DetailPen, BlockPen;		/* for bar/border/gadget rendering */

    ULONG IDCMPFlags;			/* User-selected IDCMP flags */

    ULONG Flags;			/* see Window struct for defines */

    /* You supply a linked-list of Gadgets for your Window.
     *	This list DOES NOT include system Gadgets.  You get the standard
     *	system Window Gadgets by setting flag-bits in the variable Flags (see
     *	the bit definitions under the Window structure definition)
     */
    struct Gadget *FirstGadget;

    /* the CheckMark is a pointer to the imagery that will be used when
     * rendering MenuItems of this Window that want to be checkmarked
     * if this is equal to NULL, you'll get the default imagery
     */
    struct Image *CheckMark;

    UBYTE *Title;			  /* the title text for this window */

    /* the Screen pointer is used only if you've defined a CUSTOMSCREEN and
     * want this Window to open in it.	If so, you pass the address of the
     * Custom Screen structure in this variable.  Otherwise, this variable
     * is ignored and doesn't have to be initialized.
     */
    struct Screen *Screen;

    /* WFLG_SUPER_BITMAP Window?  If so, put the address of your BitMap
     * structure in this variable.  If not, this variable is ignored and
     * doesn't have to be initialized
     */
    struct BitMap *BitMap;

    /* the values describe the minimum and maximum sizes of your Windows.
     * these matter only if you've chosen the WFLG_SIZEGADGET option,
     * which means that you want to let the User to change the size of
     * this Window.  You describe the minimum and maximum sizes that the
     * Window can grow by setting these variables.  You can initialize
     * any one these to zero, which will mean that you want to duplicate
     * the setting for that dimension (if MinWidth == 0, MinWidth will be
     * set to the opening Width of the Window).
     * You can change these settings later using SetWindowLimits().
     * If you haven't asked for a SIZING Gadget, you don't have to
     * initialize any of these variables.
     */
    WORD MinWidth, MinHeight;	    /* minimums */
    UWORD MaxWidth, MaxHeight;	     /* maximums */

    /* the type variable describes the Screen in which you want this Window to
     * open.  The type value can either be CUSTOMSCREEN or one of the
     * system standard Screen Types such as WBENCHSCREEN.  See the
     * type definitions under the Screen structure.
     */
    UWORD Type;

};

/* The following structure is the future NewWindow.  Compatibility
 * issues require that the size of NewWindow not change.
 * Data in the common part (NewWindow) indicates the the extension
 * fields are being used.
 * NOTE WELL: This structure may be subject to future extension.
 * Writing code depending on its size is not allowed.
 */
struct ExtNewWindow
{
    WORD LeftEdge, TopEdge;
    WORD Width, Height;

    UBYTE DetailPen, BlockPen;
    ULONG IDCMPFlags;
    ULONG Flags;
    struct Gadget *FirstGadget;

    struct Image *CheckMark;

    UBYTE *Title;
    struct Screen *Screen;
    struct BitMap *BitMap;

    WORD MinWidth, MinHeight;
    UWORD MaxWidth, MaxHeight;

    /* the type variable describes the Screen in which you want this Window to
     * open.  The type value can either be CUSTOMSCREEN or one of the
     * system standard Screen Types such as WBENCHSCREEN.  See the
     * type definitions under the Screen structure.
     * A new possible value for this field is PUBLICSCREEN, which
     * defines the window as a 'visitor' window.  See below for
     * additional information provided.
     */
    UWORD Type;

    /* ------------------------------------------------------- *
     * extensions for V36
     * if the NewWindow Flag value WFLG_NW_EXTENDED is set, then
     * this field is assumed to point to an array ( or chain of arrays)
     * of TagItem structures.  See also ExtNewScreen for another
     * use of TagItems to pass optional data.
     *
     * see below for tag values and the corresponding data.
     */
    struct TagItem	*Extension;
};

/*
 * The TagItem ID's (ti_Tag values) for OpenWindowTagList() follow.
 * They are values in a TagItem array passed as extension/replacement
 * values for the data in NewWindow.  OpenWindowTagList() can actually
 * work well with a NULL NewWindow pointer.
 */

#define WA_Dummy	(TAG_USER + 99)	/* 0x80000063	*/

/* these tags simply override NewWindow parameters */
#define WA_Left			(WA_Dummy + 0x01)
#define WA_Top			(WA_Dummy + 0x02)
#define WA_Width		(WA_Dummy + 0x03)
#define WA_Height		(WA_Dummy + 0x04)
#define WA_DetailPen		(WA_Dummy + 0x05)
#define WA_BlockPen		(WA_Dummy + 0x06)
#define WA_IDCMP		(WA_Dummy + 0x07)
			/* "bulk" initialization of NewWindow.Flags */
#define WA_Flags		(WA_Dummy + 0x08)
#define WA_Gadgets		(WA_Dummy + 0x09)
#define WA_Checkmark		(WA_Dummy + 0x0A)
#define WA_Title		(WA_Dummy + 0x0B)
			/* means you don't have to call SetWindowTitles
			 * after you open your window
			 */
#define WA_ScreenTitle		(WA_Dummy + 0x0C)
#define WA_CustomScreen		(WA_Dummy + 0x0D)
#define WA_SuperBitMap		(WA_Dummy + 0x0E)
			/* also implies WFLG_SUPER_BITMAP property	*/
#define WA_MinWidth		(WA_Dummy + 0x0F)
#define WA_MinHeight		(WA_Dummy + 0x10)
#define WA_MaxWidth		(WA_Dummy + 0x11)
#define WA_MaxHeight		(WA_Dummy + 0x12)

/* The following are specifications for new features	*/

#define WA_InnerWidth		(WA_Dummy + 0x13)
#define WA_InnerHeight		(WA_Dummy + 0x14)
			/* You can specify the dimensions of the interior
			 * region of your window, independent of what
			 * the border widths will be.  You probably want
			 * to also specify WA_AutoAdjust to allow
			 * Intuition to move your window or even
			 * shrink it so that it is completely on screen.
			 */

#define WA_PubScreenName	(WA_Dummy + 0x15)
			/* declares that you want the window to open as
			 * a visitor on the public screen whose name is
			 * pointed to by (UBYTE *) ti_Data
			 */
#define WA_PubScreen		(WA_Dummy + 0x16)
			/* open as a visitor window on the public screen
			 * whose address is in (struct Screen *) ti_Data.
			 * To ensure that this screen remains open, you
			 * should either be the screen's owner, have a
			 * window open on the screen, or use LockPubScreen().
			 */
#define WA_PubScreenFallBack	(WA_Dummy + 0x17)
			/* A Boolean, specifies whether a visitor window
			 * should "fall back" to the default public screen
			 * (or Workbench) if the named public screen isn't
			 * available
			 */
#define WA_WindowName		(WA_Dummy + 0x18)
			/* not implemented	*/
#define WA_Colors		(WA_Dummy + 0x19)
			/* a ColorSpec array for colors to be set
			 * when this window is active.	This is not
			 * implemented, and may not be, since the default
			 * values to restore would be hard to track.
			 * We'd like to at least support per-window colors
			 * for the mouse pointer sprite.
			 */
#define WA_Zoom		(WA_Dummy + 0x1A)
			/* ti_Data points to an array of four WORD's,
			 * the initial Left/Top/Width/Height values of
			 * the "alternate" zoom position/dimensions.
			 * It also specifies that you want a Zoom gadget
			 * for your window, whether or not you have a
			 * sizing gadget.
			 */
#define WA_MouseQueue		(WA_Dummy + 0x1B)
			/* ti_Data contains initial value for the mouse
			 * message backlog limit for this window.
			 */
#define WA_BackFill		(WA_Dummy + 0x1C)
			/* provides a "backfill hook" for your window's Layer.
			 * See layers.library/CreateUpfrontHookLayer().
			 */
#define WA_RptQueue		(WA_Dummy + 0x1D)
			/* initial value of repeat key backlog limit	*/

    /* These Boolean tag items are alternatives to the NewWindow.Flags
     * boolean flags with similar names.
     */
#define WA_SizeGadget		(WA_Dummy + 0x1E)
#define WA_DragBar		(WA_Dummy + 0x1F)
#define WA_DepthGadget		(WA_Dummy + 0x20)
#define WA_CloseGadget		(WA_Dummy + 0x21)
#define WA_Backdrop		(WA_Dummy + 0x22)
#define WA_ReportMouse		(WA_Dummy + 0x23)
#define WA_NoCareRefresh	(WA_Dummy + 0x24)
#define WA_Borderless		(WA_Dummy + 0x25)
#define WA_Activate		(WA_Dummy + 0x26)
#define WA_RMBTrap		(WA_Dummy + 0x27)
#define WA_WBenchWindow		(WA_Dummy + 0x28)	/* PRIVATE!! */
#define WA_SimpleRefresh	(WA_Dummy + 0x29)
			/* only specify if TRUE	*/
#define WA_SmartRefresh		(WA_Dummy + 0x2A)
			/* only specify if TRUE	*/
#define WA_SizeBRight		(WA_Dummy + 0x2B)
#define WA_SizeBBottom		(WA_Dummy + 0x2C)

    /* New Boolean properties	*/
#define WA_AutoAdjust		(WA_Dummy + 0x2D)
			/* shift or squeeze the window's position and
			 * dimensions to fit it on screen.
			 */

#define WA_GimmeZeroZero	(WA_Dummy + 0x2E)
			/* equiv. to NewWindow.Flags WFLG_GIMMEZEROZERO	*/

/* New for V37: WA_MenuHelp (ignored by V36) */
#define WA_MenuHelp		(WA_Dummy + 0x2F)
			/* Enables IDCMP_MENUHELP:  Pressing HELP during menus
			 * will return IDCMP_MENUHELP message.
			 */

/* New for V39:  (ignored by V37 and earlier) */
#define WA_NewLookMenus		(WA_Dummy + 0x30)
			/* Set to TRUE if you want NewLook menus */
#define WA_AmigaKey		(WA_Dummy + 0x31)
			/* Pointer to image for Amiga-key equiv in menus */
#define WA_NotifyDepth		(WA_Dummy + 0x32)
			/* Requests IDCMP_CHANGEWINDOW message when
			 * window is depth arranged
			 * (imsg->Code = CWCODE_DEPTH)
			 */

/* WA_Dummy + 0x33 is obsolete */

#define WA_Pointer		(WA_Dummy + 0x34)
			/* Allows you to specify a custom pointer
			 * for your window.  ti_Data points to a
			 * pointer object you obtained via
			 * "pointerclass". NULL signifies the
			 * default pointer.
			 * This tag may be passed to OpenWindowTags()
			 * or SetWindowPointer().
			 */

#define WA_BusyPointer		(WA_Dummy + 0x35)
			/* ti_Data is boolean.	Set to TRUE to
			 * request the standard busy pointer.
			 * This tag may be passed to OpenWindowTags()
			 * or SetWindowPointer().
			 */

#define WA_PointerDelay		(WA_Dummy + 0x36)
			/* ti_Data is boolean.	Set to TRUE to
			 * request that the changing of the
			 * pointer be slightly delayed.  The change
			 * will be called off if you call NewSetPointer()
			 * before the delay expires.  This allows
			 * you to post a busy-pointer even if you think
			 * the busy-time may be very short, without
			 * fear of a flashing pointer.
			 * This tag may be passed to OpenWindowTags()
			 * or SetWindowPointer().
			 */

#define WA_TabletMessages	(WA_Dummy + 0x37)
			/* ti_Data is a boolean.  Set to TRUE to
			 * request that tablet information be included
			 * in IntuiMessages sent to your window.
			 * Requires that something (i.e. a tablet driver)
			 * feed IESUBCLASS_NEWTABLET InputEvents into
			 * the system.	For a pointer to the TabletData,
			 * examine the ExtIntuiMessage->eim_TabletData
			 * field.  It is UNSAFE to check this field
			 * when running on pre-V39 systems.  It's always
			 * safe to check this field under V39 and up,
			 * though it may be NULL.
			 */

#define WA_HelpGroup		(WA_Dummy + 0x38)
			/* When the active window has gadget help enabled,
			 * other windows of the same HelpGroup number
			 * will also get GadgetHelp.  This allows GadgetHelp
			 * to work for multi-windowed applications.
			 * Use GetGroupID() to get an ID number.  Pass
			 * this number as ti_Data to all your windows.
			 * See also the HelpControl() function.
			 */

#define WA_HelpGroupWindow	(WA_Dummy + 0x39)
			/* When the active window has gadget help enabled,
			 * other windows of the same HelpGroup will also get
			 * GadgetHelp.	This allows GadgetHelp to work
			 * for multi-windowed applications.  As an alternative
			 * to WA_HelpGroup, you can pass a pointer to any
			 * other window of the same group to join its help
			 * group.  Defaults to NULL, which has no effect.
			 * See also the HelpControl() function.
			 */


/* HelpControl() flags:
 *
 * HC_GADGETHELP - Set this flag to enable Gadget-Help for one or more
 * windows.
 */

#define HC_GADGETHELP	(1)


#ifndef INTUITION_SCREENS_H
#include <intuition/screens.h>
#endif

#ifndef INTUITION_PREFERENCES_H
#include <intuition/preferences.h>
#endif

/* ======================================================================== */
/* === Remember =========================================================== */
/* ======================================================================== */
/* this structure is used for remembering what memory has been allocated to
 * date by a given routine, so that a premature abort or systematic exit
 * can deallocate memory cleanly, easily, and completely
 */
struct Remember
{
    struct Remember *NextRemember;
    ULONG RememberSize;
    UBYTE *Memory;
};


/* === Color Spec ====================================================== */
/* How to tell Intuition about RGB values for a color table entry.
 * NOTE:  The way the structure was defined, the color value was
 * right-justified within each UWORD.  This poses problems for
 * extensibility to more bits-per-gun.	The SA_Colors32 tag to
 * OpenScreenTags() provides an alternate way to specify colors
 * with greater precision.
 */
struct ColorSpec
{
    WORD	ColorIndex;	/* -1 terminates an array of ColorSpec	*/
    UWORD	Red;	/* only the _bottom_ 4 bits recognized */
    UWORD	Green;	/* only the _bottom_ 4 bits recognized */
    UWORD	Blue;	/* only the _bottom_ 4 bits recognized */
};

/* === Easy Requester Specification ======================================= */
/* see also autodocs for EasyRequest and BuildEasyRequest	*/
/* NOTE: This structure may grow in size in the future		*/
struct EasyStruct {
    ULONG	es_StructSize;	/* should be sizeof (struct EasyStruct )*/
    ULONG	es_Flags;	/* should be 0 for now			*/
    UBYTE	*es_Title;	/* title of requester window		*/
    UBYTE	*es_TextFormat;	/* 'printf' style formatting string	*/
    UBYTE	*es_GadgetFormat; /* 'printf' style formatting string	*/
};



/* ======================================================================== */
/* === Miscellaneous ====================================================== */
/* ======================================================================== */

/* = MACROS ============================================================== */
#define MENUNUM(n) (n & 0x1F)
#define ITEMNUM(n) ((n >> 5) & 0x003F)
#define SUBNUM(n) ((n >> 11) & 0x001F)

#define SHIFTMENU(n) (n & 0x1F)
#define SHIFTITEM(n) ((n & 0x3F) << 5)
#define SHIFTSUB(n) ((n & 0x1F) << 11)

#define FULLMENUNUM( menu, item, sub )	\
	( SHIFTSUB(sub) | SHIFTITEM(item) | SHIFTMENU(menu) )

#define SRBNUM(n)    (0x08 - (n >> 4))	/* SerRWBits -> read bits per char */
#define SWBNUM(n)    (0x08 - (n & 0x0F))/* SerRWBits -> write bits per chr */
#define SSBNUM(n)    (0x01 + (n >> 4))	/* SerStopBuf -> stop bits per chr */
#define SPARNUM(n)   (n >> 4)		/* SerParShk -> parity setting	  */
#define SHAKNUM(n)   (n & 0x0F)	/* SerParShk -> handshake mode	  */


/* = MENU STUFF =========================================================== */
#define NOMENU 0x001F
#define NOITEM 0x003F
#define NOSUB  0x001F
#define MENUNULL 0xFFFF


/* = =RJ='s peculiarities ================================================= */
#define FOREVER for(;;)
#define SIGN(x) ( ((x) > 0) - ((x) < 0) )
#define NOT !

/* these defines are for the COMMSEQ and CHECKIT menu stuff.  If CHECKIT,
 * I'll use a generic Width (for all resolutions) for the CheckMark.
 * If COMMSEQ, likewise I'll use this generic stuff
 */
#define CHECKWIDTH	19
#define COMMWIDTH	27
#define LOWCHECKWIDTH	13
#define LOWCOMMWIDTH	16


/* these are the AlertNumber defines.  if you are calling DisplayAlert()
 * the AlertNumber you supply must have the ALERT_TYPE bits set to one
 * of these patterns
 */
#define ALERT_TYPE	0x80000000L
#define RECOVERY_ALERT	0x00000000L	/* the system can recover from this */
#define DEADEND_ALERT	0x80000000L	/* no recovery possible, this is it */


/* When you're defining IntuiText for the Positive and Negative Gadgets
 * created by a call to AutoRequest(), these defines will get you
 * reasonable-looking text.  The only field without a define is the IText
 * field; you decide what text goes with the Gadget
 */
#define AUTOFRONTPEN	0
#define AUTOBACKPEN	1
#define AUTODRAWMODE	JAM2
#define AUTOLEFTEDGE	6
#define AUTOTOPEDGE	3
#define AUTOITEXTFONT	NULL
#define AUTONEXTTEXT	NULL


/* --- RAWMOUSE Codes and Qualifiers (Console OR IDCMP) ------------------- */
#define SELECTUP	(IECODE_LBUTTON | IECODE_UP_PREFIX)
#define SELECTDOWN	(IECODE_LBUTTON)
#define MENUUP		(IECODE_RBUTTON | IECODE_UP_PREFIX)
#define MENUDOWN	(IECODE_RBUTTON)
#define MIDDLEUP	(IECODE_MBUTTON | IECODE_UP_PREFIX)
#define MIDDLEDOWN	(IECODE_MBUTTON)
#define ALTLEFT		(IEQUALIFIER_LALT)
#define ALTRIGHT	(IEQUALIFIER_RALT)
#define AMIGALEFT	(IEQUALIFIER_LCOMMAND)
#define AMIGARIGHT	(IEQUALIFIER_RCOMMAND)
#define AMIGAKEYS	(AMIGALEFT | AMIGARIGHT)

#define CURSORUP	0x4C
#define CURSORLEFT	0x4F
#define CURSORRIGHT	0x4E
#define CURSORDOWN	0x4D
#define KEYCODE_Q	0x10
#define KEYCODE_Z	0x31
#define KEYCODE_X	0x32
#define KEYCODE_V	0x34
#define KEYCODE_B	0x35
#define KEYCODE_N	0x36
#define KEYCODE_M	0x37
#define KEYCODE_LESS	0x38
#define KEYCODE_GREATER 0x39



/* New for V39, Intuition supports the IESUBCLASS_NEWTABLET subclass
 * of the IECLASS_NEWPOINTERPOS event.	The ie_EventAddress of such
 * an event points to a TabletData structure (see below).
 *
 * The TabletData structure contains certain elements including a taglist.
 * The taglist can be used for special tablet parameters.  A tablet driver
 * should include only those tag-items the tablet supports.  An application
 * can listen for any tag-items that interest it.  Note: an application
 * must set the WA_TabletMessages attribute to TRUE to receive this
 * extended information in its IntuiMessages.
 *
 * The definitions given here MUST be followed.  Pay careful attention
 * to normalization and the interpretation of signs.
 *
 * TABLETA_TabletZ:  the current value of the tablet in the Z direction.
 * This unsigned value should typically be in the natural units of the
 * tablet.  You should also provide TABLETA_RangeZ.
 *
 * TABLETA_RangeZ:  the maximum value of the tablet in the Z direction.
 * Normally specified along with TABLETA_TabletZ, this allows the
 * application to scale the actual Z value across its range.
 *
 * TABLETA_AngleX:  the angle of rotation or tilt about the X-axis.  This
 * number should be normalized to fill a signed long integer.  Positive
 * values imply a clockwise rotation about the X-axis when viewing
 * from +X towards the origin.
 *
 * TABLETA_AngleY:  the angle of rotation or tilt about the Y-axis.  This
 * number should be normalized to fill a signed long integer.  Positive
 * values imply a clockwise rotation about the Y-axis when viewing
 * from +Y towards the origin.
 *
 * TABLETA_AngleZ:  the angle of rotation or tilt about the Z axis.  This
 * number should be normalized to fill a signed long integer.  Positive
 * values imply a clockwise rotation about the Z-axis when viewing
 * from +Z towards the origin.
 *
 *	Note: a stylus that supports tilt should use the TABLETA_AngleX
 *	and TABLETA_AngleY attributes.	Tilting the stylus so the tip
 *	points towards increasing or decreasing X is actually a rotation
 *	around the Y-axis.  Thus, if the stylus tip points towards
 *	positive X, then that tilt is represented as a negative
 *	TABLETA_AngleY.  Likewise, if the stylus tip points towards
 *	positive Y, that tilt is represented by positive TABLETA_AngleX.
 *
 * TABLETA_Pressure:  the pressure reading of the stylus.  The pressure
 * should be normalized to fill a signed long integer.	Typical devices
 * won't generate negative pressure, but the possibility is not precluded.
 * The pressure threshold which is considered to cause a button-click is
 * expected to be set in a Preferences program supplied by the tablet
 * vendor.  The tablet driver would send IECODE_LBUTTON-type events as
 * the pressure crossed that threshold.
 *
 * TABLETA_ButtonBits:	ti_Data is a long integer whose bits are to
 * be interpreted at the state of the first 32 buttons of the tablet.
 *
 * TABLETA_InProximity:  ti_Data is a boolean.	For tablets that support
 * proximity, they should send the {TABLETA_InProximity,FALSE} tag item
 * when the stylus is out of proximity.  One possible use we can forsee
 * is a mouse-blanking commodity which keys off this to blank the
 * mouse.  When this tag is absent, the stylus is assumed to be
 * in proximity.
 *
 * TABLETA_ResolutionX:  ti_Data is an unsigned long integer which
 * is the x-axis resolution in dots per inch.
 *
 * TABLETA_ResolutionY:  ti_Data is an unsigned long integer which
 * is the y-axis resolution in dots per inch.
 */

#define TABLETA_Dummy		(TAG_USER + 0x3A000)
#define TABLETA_TabletZ		(TABLETA_Dummy + 0x01)
#define TABLETA_RangeZ		(TABLETA_Dummy + 0x02)
#define TABLETA_AngleX		(TABLETA_Dummy + 0x03)
#define TABLETA_AngleY		(TABLETA_Dummy + 0x04)
#define TABLETA_AngleZ		(TABLETA_Dummy + 0x05)
#define TABLETA_Pressure	(TABLETA_Dummy + 0x06)
#define TABLETA_ButtonBits	(TABLETA_Dummy + 0x07)
#define TABLETA_InProximity	(TABLETA_Dummy + 0x08)
#define TABLETA_ResolutionX	(TABLETA_Dummy + 0x09)
#define TABLETA_ResolutionY	(TABLETA_Dummy + 0x0A)

/* If your window sets WA_TabletMessages to TRUE, then it will receive
 * extended IntuiMessages (struct ExtIntuiMessage) whose eim_TabletData
 * field points at a TabletData structure.  This structure contains
 * additional information about the input event.
 */

struct TabletData
{
    /* Sub-pixel position of tablet, in screen coordinates,
     * scaled to fill a UWORD fraction:
     */
    UWORD td_XFraction, td_YFraction;

    /* Current tablet coordinates along each axis: */
    ULONG td_TabletX, td_TabletY;

    /* Tablet range along each axis.  For example, if td_TabletX
     * can take values 0-999, td_RangeX should be 1000.
     */
    ULONG td_RangeX, td_RangeY;

    /* Pointer to tag-list of additional tablet attributes.
     * See <intuition/intuition.h> for the tag values.
     */
    struct TagItem *td_TagList;
};

/* If a tablet driver supplies a hook for ient_CallBack, it will be
 * invoked in the standard hook manner.  A0 will point to the Hook
 * itself, A2 will point to the InputEvent that was sent, and
 * A1 will point to a TabletHookData structure.  The InputEvent's
 * ie_EventAddress field points at the IENewTablet structure that
 * the driver supplied.
 *
 * Based on the thd_Screen, thd_Width, and thd_Height fields, the driver
 * should scale the ient_TabletX and ient_TabletY fields and store the
 * result in ient_ScaledX, ient_ScaledY, ient_ScaledXFraction, and
 * ient_ScaledYFraction.
 *
 * The tablet hook must currently return NULL.	This is the only
 * acceptable return-value under V39.
 */

struct TabletHookData
{
    /* Pointer to the active screen:
     * Note: if there are no open screens, thd_Screen will be NULL.
     * thd_Width and thd_Height will then describe an NTSC 640x400
     * screen.	Please scale accordingly.
     */
    struct Screen *thd_Screen;

    /* The width and height (measured in pixels of the active screen)
     * that your are to scale to:
     */
    ULONG thd_Width;
    ULONG thd_Height;

    /* Non-zero if the screen or something about the screen
     * changed since the last time you were invoked:
     */
    LONG thd_ScreenChanged;
};

/* Include obsolete identifiers: */
#ifndef INTUITION_IOBSOLETE_H
#include <intuition/iobsolete.h>
#endif

#endif
```

## 9.2. intuition/screens.h — Screen, NewScreen, ExtNewScreen, DrawInfo, PubScreenNode, ScreenBuffer, SA_* tags, OSCAN_*, OSERR_*, DETAILPEN/BLOCKPEN/... pens

// Source: NDK_3.9/Include/include_h/intuition/screens.h
// Screen layout and OpenScreenTagList tags. DrawInfo holds the pen palette for a screen.

```c
#ifndef INTUITION_SCREENS_H
#define INTUITION_SCREENS_H TRUE
/*
**  $VER: screens.h 38.25 (15.2.1993)
**  Includes Release 45.1
**
**  The Screen and NewScreen structures and attributes
**
**  (C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef GRAPHICS_GFX_H
#include <graphics/gfx.h>
#endif

#ifndef GRAPHICS_CLIP_H
#include <graphics/clip.h>
#endif

#ifndef GRAPHICS_VIEW_H
#include <graphics/view.h>
#endif

#ifndef GRAPHICS_RASTPORT_H
#include <graphics/rastport.h>
#endif

#ifndef GRAPHICS_LAYERS_H
#include <graphics/layers.h>
#endif

#ifndef UTILITY_TAGITEM_H
#include <utility/tagitem.h>
#endif

/*
 * NOTE:  intuition/iobsolete.h is included at the END of this file!
 */

/* ======================================================================== */
/* === DrawInfo ========================================================= */
/* ======================================================================== */

/* This is a packet of information for graphics rendering.  It originates
 * with a Screen, and is gotten using GetScreenDrawInfo( screen );
 */

/* You can use the Intuition version number to tell which fields are
 * present in this structure.
 *
 * DRI_VERSION of 1 corresponds to V37 release.
 * DRI_VERSION of 2 corresponds to V39, and includes three new pens
 *	and the dri_CheckMark and dri_AmigaKey fields.
 *
 * Note that sometimes applications need to create their own DrawInfo
 * structures, in which case the DRI_VERSION won't correspond exactly
 * to the OS version!!!
 */
#define DRI_VERSION	(2)

struct DrawInfo
{
    UWORD	dri_Version;	/* will be  DRI_VERSION			*/
    UWORD	dri_NumPens;	/* guaranteed to be >= 9		*/
    UWORD	*dri_Pens;	/* pointer to pen array			*/

    struct TextFont	*dri_Font;	/* screen default font		*/
    UWORD	dri_Depth;	/* (initial) depth of screen bitmap	*/

    struct {	  /* from DisplayInfo database for initial display mode	*/
	UWORD	X;
	UWORD	Y;
    }		dri_Resolution;

    ULONG	dri_Flags;		/* defined below		*/
/* New for V39: dri_CheckMark, dri_AmigaKey. */
    struct Image	*dri_CheckMark;	/* pointer to scaled checkmark image
					 * Will be NULL if DRI_VERSION < 2
					 */
    struct Image	*dri_AmigaKey;	/* pointer to scaled Amiga-key image
					 * Will be NULL if DRI_VERSION < 2
					 */
    ULONG	dri_Reserved[5];	/* avoid recompilation ;^)	*/
};

#define DRIF_NEWLOOK	0x00000001L	/* specified SA_Pens, full treatment */

/* rendering pen number indexes into DrawInfo.dri_Pens[]	*/
#define DETAILPEN	 (0x0000)	/* compatible Intuition rendering pens	*/
#define BLOCKPEN	 (0x0001)	/* compatible Intuition rendering pens	*/
#define TEXTPEN		 (0x0002)	/* text on background			*/
#define SHINEPEN	 (0x0003)	/* bright edge on 3D objects		*/
#define SHADOWPEN	 (0x0004)	/* dark edge on 3D objects		*/
#define FILLPEN		 (0x0005)	/* active-window/selected-gadget fill	*/
#define FILLTEXTPEN	 (0x0006)	/* text over FILLPEN			*/
#define BACKGROUNDPEN	 (0x0007)	/* may not always be color 0		*/
#define HIGHLIGHTTEXTPEN (0x0008)	/* special color text, on background	*/
/* New for V39, only present if DRI_VERSION >= 2: */
#define BARDETAILPEN	 (0x0009)	/* text/detail in screen-bar/menus */
#define BARBLOCKPEN	 (0x000A)	/* screen-bar/menus fill */
#define BARTRIMPEN	 (0x000B)	/* trim under screen-bar */

#define NUMDRIPENS	 (0x000C)


/* New for V39:  It is sometimes useful to specify that a pen value
 * is to be the complement of color zero to three.  The "magic" numbers
 * serve that purpose:
 */
#define PEN_C3		0xFEFC		/* Complement of color 3 */
#define PEN_C2		0xFEFD		/* Complement of color 2 */
#define PEN_C1		0xFEFE		/* Complement of color 1 */
#define PEN_C0		0xFEFF		/* Complement of color 0 */

/* ======================================================================== */
/* === Screen ============================================================= */
/* ======================================================================== */

/* VERY IMPORTANT NOTE ABOUT Screen->BitMap.  In the future, bitmaps
 * will need to grow.  The embedded instance of a bitmap in the screen
 * will no longer be large enough to hold the whole description of
 * the bitmap.
 *
 * YOU ARE STRONGLY URGED to use Screen->RastPort.BitMap in place of
 * &Screen->BitMap whenever and whereever possible.
 */

struct Screen
{
    struct Screen *NextScreen;		/* linked list of screens */
    struct Window *FirstWindow;		/* linked list Screen's Windows */

    WORD LeftEdge, TopEdge;		/* parameters of the screen */
    WORD Width, Height;			/* parameters of the screen */

    WORD MouseY, MouseX;		/* position relative to upper-left */

    UWORD Flags;			/* see definitions below */

    UBYTE *Title;			/* null-terminated Title text */
    UBYTE *DefaultTitle;		/* for Windows without ScreenTitle */

    /* Bar sizes for this Screen and all Window's in this Screen */
    /* Note that BarHeight is one less than the actual menu bar
     * height.	We're going to keep this in V36 for compatibility,
     * although V36 artwork might use that extra pixel
     *
     * Also, the title bar height of a window is calculated from the
     * screen's WBorTop field, plus the font height, plus one.
     */
    BYTE BarHeight, BarVBorder, BarHBorder, MenuVBorder, MenuHBorder;
    BYTE WBorTop, WBorLeft, WBorRight, WBorBottom;

    struct TextAttr *Font;		/* this screen's default font	   */

    /* the display data structures for this Screen */
    struct ViewPort ViewPort;		/* describing the Screen's display */
    struct RastPort RastPort;		/* describing Screen rendering	   */
    struct BitMap BitMap;		/* SEE WARNING ABOVE!		   */
    struct Layer_Info LayerInfo;	/* each screen gets a LayerInfo    */

    /* Only system gadgets may be attached to a screen.
     *	You get the standard system Screen Gadgets automatically
     */
    struct Gadget *FirstGadget;

    UBYTE DetailPen, BlockPen;		/* for bar/border/gadget rendering */

    /* the following variable(s) are maintained by Intuition to support the
     * DisplayBeep() color flashing technique
     */
    UWORD SaveColor0;

    /* This layer is for the Screen and Menu bars */
    struct Layer *BarLayer;

    UBYTE *ExtData;

    UBYTE *UserData;	/* general-purpose pointer to User data extension */

    /**** Data below this point are SYSTEM PRIVATE ****/
};


/* --- FLAGS SET BY INTUITION --------------------------------------------- */
/* The SCREENTYPE bits are reserved for describing various Screen types
 * available under Intuition.
 */
#define SCREENTYPE	0x000F	/* all the screens types available	*/
/* --- the definitions for the Screen Type ------------------------------- */
#define WBENCHSCREEN	0x0001	/* identifies the Workbench screen	*/
#define PUBLICSCREEN	0x0002	/* public shared (custom) screen	*/
#define CUSTOMSCREEN	0x000F	/* original custom screens		*/

#define SHOWTITLE	0x0010	/* this gets set by a call to ShowTitle() */

#define BEEPING		0x0020	/* set when Screen is beeping (private)	*/

#define CUSTOMBITMAP	0x0040	/* if you are supplying your own BitMap */

#define SCREENBEHIND	0x0080	/* if you want your screen to open behind
				 * already open screens
				 */
#define SCREENQUIET	0x0100	/* if you do not want Intuition to render
				 * into your screen (gadgets, title)
				 */
#define SCREENHIRES	0x0200	/* do not use lowres gadgets  (private)	*/

#define NS_EXTENDED	0x1000		/* ExtNewScreen.Extension is valid	*/
/* V36 applications can use OpenScreenTagList() instead of NS_EXTENDED	*/

#define AUTOSCROLL	0x4000	/* screen is to autoscoll		*/

/* New for V39: */
#define PENSHARED	0x0400	/* Screen opener set {SA_SharePens,TRUE} */




#define STDSCREENHEIGHT -1	/* supply in NewScreen.Height		*/
#define STDSCREENWIDTH -1	/* supply in NewScreen.Width		*/

/*
 * Screen attribute tag ID's.  These are used in the ti_Tag field of
 * TagItem arrays passed to OpenScreenTagList() (or in the
 * ExtNewScreen.Extension field).
 */

/* Screen attribute tags.  Please use these versions, not those in
 * iobsolete.h.
 */

#define SA_Dummy	(TAG_USER + 32)
/*
 * these items specify items equivalent to fields in NewScreen
 */
#define SA_Left		(SA_Dummy + 0x0001)
#define SA_Top		(SA_Dummy + 0x0002)
#define SA_Width	(SA_Dummy + 0x0003)
#define SA_Height	(SA_Dummy + 0x0004)
			/* traditional screen positions	and dimensions	*/
#define SA_Depth	(SA_Dummy + 0x0005)
			/* screen bitmap depth				*/
#define SA_DetailPen	(SA_Dummy + 0x0006)
			/* serves as default for windows, too		*/
#define SA_BlockPen	(SA_Dummy + 0x0007)
#define SA_Title	(SA_Dummy + 0x0008)
			/* default screen title				*/
#define SA_Colors	(SA_Dummy + 0x0009)
			/* ti_Data is an array of struct ColorSpec,
			 * terminated by ColorIndex = -1.  Specifies
			 * initial screen palette colors.
			 * Also see SA_Colors32 for use under V39.
			 */
#define SA_ErrorCode	(SA_Dummy + 0x000A)
			/* ti_Data points to LONG error code (values below)*/
#define SA_Font		(SA_Dummy + 0x000B)
			/* equiv. to NewScreen.Font			*/
#define SA_SysFont	(SA_Dummy + 0x000C)
			/* Selects one of the preferences system fonts:
			 *	0 - old DefaultFont, fixed-width
			 *	1 - WB Screen preferred font
			 */
#define SA_Type		(SA_Dummy + 0x000D)
			/* ti_Data is PUBLICSCREEN or CUSTOMSCREEN.  For other
			 * fields of NewScreen.Type, see individual tags,
			 * eg. SA_Behind, SA_Quiet.
			 */
#define SA_BitMap	(SA_Dummy + 0x000E)
			/* ti_Data is pointer to custom BitMap.  This
			 * implies type of CUSTOMBITMAP
			 */
#define SA_PubName	(SA_Dummy + 0x000F)
			/* presence of this tag means that the screen
			 * is to be a public screen.  Please specify
			 * BEFORE the two tags below
			 */
#define SA_PubSig	(SA_Dummy + 0x0010)
#define SA_PubTask	(SA_Dummy + 0x0011)
			/* Task ID and signal for being notified that
			 * the last window has closed on a public screen.
			 */
#define SA_DisplayID	(SA_Dummy + 0x0012)
			/* ti_Data is new extended display ID from
			 * <graphics/displayinfo.h> (V37) or from
			 * <graphics/modeid.h> (V39 and up)
			 */
#define SA_DClip	(SA_Dummy + 0x0013)
			/* ti_Data points to a rectangle which defines
			 * screen display clip region
			 */
#define SA_Overscan	(SA_Dummy + 0x0014)
			/* Set to one of the OSCAN_
			 * specifiers below to get a system standard
			 * overscan region for your display clip,
			 * screen dimensions (unless otherwise specified),
			 * and automatically centered position (partial
			 * support only so far).
			 * If you use this, you shouldn't specify
			 * SA_DClip.  SA_Overscan is for "standard"
			 * overscan dimensions, SA_DClip is for
			 * your custom numeric specifications.
			 */
#define SA_Obsolete1	(SA_Dummy + 0x0015)
			/* obsolete S_MONITORNAME			*/

/** booleans **/
#define SA_ShowTitle	(SA_Dummy + 0x0016)
			/* boolean equivalent to flag SHOWTITLE		*/
#define SA_Behind	(SA_Dummy + 0x0017)
			/* boolean equivalent to flag SCREENBEHIND	*/
#define SA_Quiet	(SA_Dummy + 0x0018)
			/* boolean equivalent to flag SCREENQUIET	*/
#define SA_AutoScroll	(SA_Dummy + 0x0019)
			/* boolean equivalent to flag AUTOSCROLL	*/
#define SA_Pens		(SA_Dummy + 0x001A)
			/* pointer to ~0 terminated UWORD array, as
			 * found in struct DrawInfo
			 */
#define SA_FullPalette	(SA_Dummy + 0x001B)
			/* boolean: initialize color table to entire
			 *  preferences palette (32 for V36), rather
			 * than compatible pens 0-3, 17-19, with
			 * remaining palette as returned by GetColorMap()
			 */

#define SA_ColorMapEntries (SA_Dummy + 0x001C)
			/* New for V39:
			 * Allows you to override the number of entries
			 * in the ColorMap for your screen.  Intuition
			 * normally allocates (1<<depth) or 32, whichever
			 * is more, but you may require even more if you
			 * use certain V39 graphics.library features
			 * (eg. palette-banking).
			 */

#define SA_Parent	(SA_Dummy + 0x001D)
			/* New for V39:
			 * ti_Data is a pointer to a "parent" screen to
			 * attach this one to.	Attached screens slide
			 * and depth-arrange together.
			 */

#define SA_Draggable	(SA_Dummy + 0x001E)
			/* New for V39:
			 * Boolean tag allowing non-draggable screens.
			 * Do not use without good reason!
			 * (Defaults to TRUE).
			 */

#define SA_Exclusive	(SA_Dummy + 0x001F)
			/* New for V39:
			 * Boolean tag allowing screens that won't share
			 * the display.  Use sparingly!  Starting with 3.01,
			 * attached screens may be SA_Exclusive.  Setting
			 * SA_Exclusive for each screen will produce an
			 * exclusive family.   (Defaults to FALSE).
			 */

#define SA_SharePens	(SA_Dummy + 0x0020)
			/* New for V39:
			 * For those pens in the screen's DrawInfo->dri_Pens,
			 * Intuition obtains them in shared mode (see
			 * graphics.library/ObtainPen()).  For compatibility,
			 * Intuition obtains the other pens of a public
			 * screen as PEN_EXCLUSIVE.  Screens that wish to
			 * manage the pens themselves should generally set
			 * this tag to TRUE.  This instructs Intuition to
			 * leave the other pens unallocated.
			 */

#define SA_BackFill	(SA_Dummy + 0x0021)
			/* New for V39:
			 * provides a "backfill hook" for your screen's
			 * Layer_Info.
			 * See layers.library/InstallLayerInfoHook()
			 */

#define SA_Interleaved	(SA_Dummy + 0x0022)
			/* New for V39:
			 * Boolean tag requesting that the bitmap
			 * allocated for you be interleaved.
			 * (Defaults to FALSE).
			 */

#define SA_Colors32	(SA_Dummy + 0x0023)
			/* New for V39:
			 * Tag to set the screen's initial palette colors
			 * at 32 bits-per-gun.	ti_Data is a pointer
			 * to a table to be passed to the
			 * graphics.library/LoadRGB32() function.
			 * This format supports both runs of color
			 * registers and sparse registers.  See the
			 * autodoc for that function for full details.
			 * Any color set here has precedence over
			 * the same register set by SA_Colors.
			 */

#define SA_VideoControl	(SA_Dummy + 0x0024)
			/* New for V39:
			 * ti_Data is a pointer to a taglist that Intuition
			 * will pass to graphics.library/VideoControl(),
			 * upon opening the screen.
			 */

#define SA_FrontChild	(SA_Dummy + 0x0025)
			/* New for V39:
			 * ti_Data is a pointer to an already open screen
			 * that is to be the child of the screen being
			 * opened.  The child screen will be moved to the
			 * front of its family.
			 */

#define SA_BackChild	(SA_Dummy + 0x0026)
			/* New for V39:
			 * ti_Data is a pointer to an already open screen
			 * that is to be the child of the screen being
			 * opened.  The child screen will be moved to the
			 * back of its family.
			 */

#define SA_LikeWorkbench	(SA_Dummy + 0x0027)
			/* New for V39:
			 * Set ti_Data to 1 to request a screen which
			 * is just like the Workbench.	This gives
			 * you the same screen mode, depth, size,
			 * colors, etc., as the Workbench screen.
			 */

#define SA_Reserved		(SA_Dummy + 0x0028)
			/* Reserved for private Intuition use */

#define SA_MinimizeISG		(SA_Dummy + 0x0029)
			/* New for V40:
			 * For compatibility, Intuition always ensures
			 * that the inter-screen gap is at least three
			 * non-interlaced lines.  If your application
			 * would look best with the smallest possible
			 * inter-screen gap, set ti_Data to TRUE.
			 * If you use the new graphics VideoControl()
			 * VC_NoColorPaletteLoad tag for your screen's
			 * ViewPort, you should also set this tag.
			 */

/* this is an obsolete tag included only for compatibility with V35
 * interim release for the A2024 and Viking monitors
 */
#ifndef NSTAG_EXT_VPMODE
#define NSTAG_EXT_VPMODE (TAG_USER | 1)
#endif


/* OpenScreen error codes, which are returned in the (optional) LONG
 * pointed to by ti_Data for the SA_ErrorCode tag item
 */
#define OSERR_NOMONITOR	   (1)	/* named monitor spec not available	*/
#define OSERR_NOCHIPS	   (2)	/* you need newer custom chips		*/
#define OSERR_NOMEM	   (3)	/* couldn't get normal memory		*/
#define OSERR_NOCHIPMEM	   (4)	/* couldn't get chipmem			*/
#define OSERR_PUBNOTUNIQUE (5)	/* public screen name already used	*/
#define OSERR_UNKNOWNMODE  (6)	/* don't recognize mode asked for	*/
#define OSERR_TOODEEP	   (7)	/* Screen deeper than HW supports	*/
#define OSERR_ATTACHFAIL   (8)	/* Failed to attach screens		*/
#define OSERR_NOTAVAILABLE (9)	/* Mode not available for other reason	*/

/* ======================================================================== */
/* === NewScreen ========================================================== */
/* ======================================================================== */
/* note: to use the Extended field, you must use the
 * new ExtNewScreen structure, below
 */
struct NewScreen
{
    WORD LeftEdge, TopEdge, Width, Height, Depth;  /* screen dimensions */

    UBYTE DetailPen, BlockPen;	/* for bar/border/gadget rendering	*/

    UWORD ViewModes;		/* the Modes for the ViewPort (and View) */

    UWORD Type;			/* the Screen type (see defines above)	*/

    struct TextAttr *Font;	/* this Screen's default text attributes */

    UBYTE *DefaultTitle;	/* the default title for this Screen	*/

    struct Gadget *Gadgets;	/* UNUSED:  Leave this NULL		*/

    /* if you are opening a CUSTOMSCREEN and already have a BitMap
     * that you want used for your Screen, you set the flags CUSTOMBITMAP in
     * the Type field and you set this variable to point to your BitMap
     * structure.  The structure will be copied into your Screen structure,
     * after which you may discard your own BitMap if you want
     */
    struct BitMap *CustomBitMap;
};

/*
 * For compatibility reasons, we need a new structure for extending
 * NewScreen.  Use this structure is you need to use the new Extension
 * field.
 *
 * NOTE: V36-specific applications should use the
 * OpenScreenTagList( newscreen, tags ) version of OpenScreen().
 * Applications that want to be V34-compatible as well may safely use the
 * ExtNewScreen structure.  Its tags will be ignored by V34 Intuition.
 *
 */
struct ExtNewScreen
{
    WORD LeftEdge, TopEdge, Width, Height, Depth;
    UBYTE DetailPen, BlockPen;
    UWORD ViewModes;
    UWORD Type;
    struct TextAttr *Font;
    UBYTE *DefaultTitle;
    struct Gadget *Gadgets;
    struct BitMap *CustomBitMap;

    struct TagItem	*Extension;
				/* more specification data, scanned if
				 * NS_EXTENDED is set in NewScreen.Type
				 */
};

/* === Overscan Types ===	*/
#define OSCAN_TEXT	(1)	/* entirely visible	*/
#define OSCAN_STANDARD	(2)	/* just past edges	*/
#define OSCAN_MAX	(3)	/* as much as possible	*/
#define OSCAN_VIDEO	(4)	/* even more than is possible	*/


/* === Public Shared Screen Node ===	*/

/* This is the representative of a public shared screen.
 * This is an internal data structure, but some functions may
 * present a copy of it to the calling application.  In that case,
 * be aware that the screen pointer of the structure can NOT be
 * used safely, since there is no guarantee that the referenced
 * screen will remain open and a valid data structure.
 *
 * Never change one of these.
 */

struct PubScreenNode	{
    struct Node		psn_Node;	/* ln_Name is screen name */
    struct Screen	*psn_Screen;
    UWORD		psn_Flags;	/* below		*/
    WORD		psn_Size;	/* includes name buffer	*/
    WORD		psn_VisitorCount; /* how many visitor windows */
    struct Task		*psn_SigTask;	/* who to signal when visitors gone */
    UBYTE		psn_SigBit;	/* which signal	*/
};

#define PSNF_PRIVATE	(0x0001)

/* NOTE: Due to a bug in NextPubScreen(), make sure your buffer
 * actually has MAXPUBSCREENNAME+1 characters in it!
 */
#define MAXPUBSCREENNAME	(139)	/* names no longer, please	*/

/* pub screen modes	*/
#define SHANGHAI	0x0001	/* put workbench windows on pub screen */
#define POPPUBSCREEN	0x0002	/* pop pub screen to front when visitor opens */

/* New for V39:  Intuition has new screen depth-arrangement and movement
 * functions called ScreenDepth() and ScreenPosition() respectively.
 * These functions permit the old behavior of ScreenToFront(),
 * ScreenToBack(), and MoveScreen().  ScreenDepth() also allows
 * independent depth control of attached screens.  ScreenPosition()
 * optionally allows positioning screens even though they were opened
 * {SA_Draggable,FALSE}.
 */

/* For ScreenDepth(), specify one of SDEPTH_TOFRONT or SDEPTH_TOBACK,
 * and optionally also SDEPTH_INFAMILY.
 *
 * NOTE: ONLY THE OWNER OF THE SCREEN should ever specify
 * SDEPTH_INFAMILY.  Commodities, "input helper" programs,
 * or any other program that did not open a screen should never
 * use that flag.  (Note that this is a style-behavior
 * requirement;  there is no technical requirement that the
 * task calling this function need be the task which opened
 * the screen).
 */

#define	SDEPTH_TOFRONT			(0)	/* Bring screen to front */
#define SDEPTH_TOBACK		(1)	/* Send screen to back */
#define SDEPTH_INFAMILY		(2)	/* Move an attached screen with
					 * respect to other screens of
					 * its family
					 */

/* Here's an obsolete name equivalent to SDEPTH_INFAMILY: */
#define SDEPTH_CHILDONLY	SDEPTH_INFAMILY


/* For ScreenPosition(), specify one of SPOS_RELATIVE, SPOS_ABSOLUTE,
 * or SPOS_MAKEVISIBLE to describe the kind of screen positioning you
 * wish to perform:
 *
 * SPOS_RELATIVE: The x1 and y1 parameters to ScreenPosition() describe
 *	the offset in coordinates you wish to move the screen by.
 * SPOS_ABSOLUTE: The x1 and y1 parameters to ScreenPosition() describe
 *	the absolute coordinates you wish to move the screen to.
 * SPOS_MAKEVISIBLE: (x1,y1)-(x2,y2) describes a rectangle on the
 *	screen which you would like autoscrolled into view.
 *
 * You may additionally set SPOS_FORCEDRAG along with any of the
 * above.  Set this if you wish to reposition an {SA_Draggable,FALSE}
 * screen that you opened.
 *
 * NOTE: ONLY THE OWNER OF THE SCREEN should ever specify
 * SPOS_FORCEDRAG.  Commodities, "input helper" programs,
 * or any other program that did not open a screen should never
 * use that flag.
 */

#define SPOS_RELATIVE		(0)	/* Coordinates are relative */

#define SPOS_ABSOLUTE		(1)	/* Coordinates are expressed as
					 * absolutes, not relatives.
					 */

#define SPOS_MAKEVISIBLE	(2)	/* Coordinates describe a box on
					 * the screen you wish to be
					 * made visible by autoscrolling
					 */

#define SPOS_FORCEDRAG		(4)	/* Move non-draggable screen */

/* New for V39: Intuition supports double-buffering in screens,
 * with friendly interaction with menus and certain gadgets.
 * For each buffer, you need to get one of these structures
 * from the AllocScreenBuffer() call.  Never allocate your
 * own ScreenBuffer structures!
 *
 * The sb_DBufInfo field is for your use.  See the graphics.library
 * AllocDBufInfo() autodoc for details.
 */
struct ScreenBuffer
{
    struct BitMap *sb_BitMap;		/* BitMap of this buffer */
    struct DBufInfo *sb_DBufInfo;	/* DBufInfo for this buffer */
};

/* These are the flags that may be passed to AllocScreenBuffer().
 */
#define SB_SCREEN_BITMAP	1
#define SB_COPY_BITMAP		2

/* Include obsolete identifiers: */
#ifndef INTUITION_IOBSOLETE_H
#include <intuition/iobsolete.h>
#endif

#endif
```

## 9.3. intuition/intuitionbase.h — IntuitionBase (private but documented)

// Source: NDK_3.9/Include/include_h/intuition/intuitionbase.h
// IntuitionBase public fields: ActiveWindow, ActiveScreen, FirstScreen, MouseX/Y, timestamp.

```c
#ifndef INTUITION_INTUITIONBASE_H
#define INTUITION_INTUITIONBASE_H 1
/*
**  $VER: intuitionbase.h 38.0 (12.6.1991)
**  Includes Release 45.1
**
**  Public part of IntuitionBase structure and supporting structures
**
**  (C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef EXEC_LIBRARIES_H
#include <exec/libraries.h>
#endif

#ifndef INTUITION_INTUITION_H
#include <intuition/intuition.h>
#endif


#ifndef EXEC_INTERRUPTS_H
#include <exec/interrupts.h>
#endif

/* these are the display modes for which we have corresponding parameter
 *  settings in the config arrays
 */
#define DMODECOUNT	0x0002	/* how many modes there are */
#define HIRESPICK	0x0000
#define LOWRESPICK	0x0001

#define EVENTMAX 10		/* size of event array */

/* these are the system Gadget defines */
#define RESCOUNT	2
#define HIRESGADGET	0
#define LOWRESGADGET	1

#define GADGETCOUNT	8
#define UPFRONTGADGET	0
#define DOWNBACKGADGET	1
#define SIZEGADGET	2
#define CLOSEGADGET	3
#define DRAGGADGET	4
#define SUPFRONTGADGET	5
#define SDOWNBACKGADGET	6
#define SDRAGGADGET	7

/* ======================================================================== */
/* === IntuitionBase ====================================================== */
/* ======================================================================== */
/*
 * Be sure to protect yourself against someone modifying these data as
 * you look at them.  This is done by calling:
 *
 * lock = LockIBase(0), which returns a ULONG.	When done call
 * UnlockIBase(lock) where lock is what LockIBase() returned.
 */

/* This structure is strictly READ ONLY */
struct IntuitionBase
{
    struct Library LibNode;

    struct View ViewLord;

    struct Window *ActiveWindow;
    struct Screen *ActiveScreen;

    /* the FirstScreen variable points to the frontmost Screen.  Screens are
     * then maintained in a front to back order using Screen.NextScreen
     */
    struct Screen *FirstScreen; /* for linked list of all screens */

    ULONG Flags;	/* values are all system private */
    WORD	MouseY, MouseX;
			/* note "backwards" order of these		*/

    ULONG Seconds;	/* timestamp of most current input event */
    ULONG Micros;	/* timestamp of most current input event */

    /* I told you this was private.
     * The data beyond this point has changed, is changing, and
     * will continue to change.
     */
};

#endif
```

## 9.4. intuition/classes.h — IClass (BOOPSI class struct), ClassLibrary

// Source: NDK_3.9/Include/include_h/intuition/classes.h
// BOOPSI class internals — dispatcher Hook, super-class pointer, instance data offset.

```c
#ifndef	INTUITION_CLASSES_H
#define INTUITION_CLASSES_H
/*
**  $VER: classes.h 40.0 (15.2.1994)
**  Includes Release 45.1
**
**  Used only by class implementors
**
**  (C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
*/

/*****************************************************************************/

#ifndef	EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef	EXEC_LIBRARIES_H
#include <exec/libraries.h>
#endif

#ifndef UTILITY_HOOKS_H
#include <utility/hooks.h>
#endif

#ifndef	INTUITION_CLASSUSR_H
#include <intuition/classusr.h>
#endif

/*****************************************************************************/
/***************** "White Box" access to struct IClass ***********************/
/*****************************************************************************/

/* This structure is READ-ONLY, and allocated only by Intuition */
typedef struct IClass
{
    struct Hook		 cl_Dispatcher;		/* Class dispatcher */
    ULONG		 cl_Reserved;		/* Must be 0  */
    struct IClass	*cl_Super;		/* Pointer to superclass */
    ClassID		 cl_ID;			/* Class ID */

    UWORD		 cl_InstOffset;		/* Offset of instance data */
    UWORD		 cl_InstSize;		/* Size of instance data */

    ULONG		 cl_UserData;		/* Class global data */
    ULONG		 cl_SubclassCount;	/* Number of subclasses */
    ULONG		 cl_ObjectCount;	/* Number of objects */
    ULONG		 cl_Flags;

} Class;

#define	CLF_INLIST	0x00000001L
    /* class is in public class list */

/*****************************************************************************/

/* add offset for instance data to an object handle */
#define INST_DATA(cl,o)		((void *)(((UBYTE *)o)+cl->cl_InstOffset))

/*****************************************************************************/

/* sizeof the instance data for a given class */
#define SIZEOF_INSTANCE(cl)	((cl)->cl_InstOffset + (cl)->cl_InstSize \
			+ sizeof (struct _Object))

/*****************************************************************************/
/***************** "White box" access to struct _Object **********************/
/*****************************************************************************/

/* We have this, the instance data of the root class, PRECEDING the "object".
 * This is so that Gadget objects are Gadget pointers, and so on.  If this
 * structure grows, it will always have o_Class at the end, so the macro
 * OCLASS(o) will always have the same offset back from the pointer returned
 * from NewObject().
 *
 * This data structure is subject to change.  Do not use the o_Node embedded
 * structure. */
struct _Object
{
    struct MinNode	 o_Node;
    struct IClass	*o_Class;

};

/*****************************************************************************/

/* convenient typecast	*/
#define _OBJ(o)			((struct _Object *)(o))

/* get "public" handle on baseclass instance from real beginning of obj data */
#define BASEOBJECT(_obj)	((Object *)(_OBJ(_obj)+1))

/* get back to object data struct from public handle */
#define _OBJECT(o)		(_OBJ(o) - 1)

/* get class pointer from an object handle	*/
#define OCLASS(o)		((_OBJECT(o))->o_Class)

/*****************************************************************************/

/* BOOPSI class libraries should use this structure as the base for their
 * library data.  This allows developers to obtain the class pointer for
 * performing object-less inquiries. */
struct ClassLibrary
{
    struct Library	 cl_Lib;	/* Embedded library */
    UWORD		 cl_Pad;	/* Align the structure */
    Class		*cl_Class;	/* Class pointer */

};

/*****************************************************************************/

#endif
```

## 9.5. intuition/classusr.h — Object, ClassID, Msg, OM_* method IDs, opSet, opGet, opUpdate

// Source: NDK_3.9/Include/include_h/intuition/classusr.h
// User-level BOOPSI — OM_NEW, OM_SET, OM_GET, OM_UPDATE methods.

```c
#ifndef	INTUITION_CLASSUSR_H
#define INTUITION_CLASSUSR_H	1
/*
**  $VER: classusr.h 38.2 (14.4.1992)
**  Includes Release 45.1
**
**  For application users of Intuition object classes
**
**  (C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
*/


#ifndef UTILITY_HOOKS_H
#include <utility/hooks.h>
#endif

/*** User visible handles on objects, classes, messages ***/
typedef ULONG	Object;		/* abstract handle */

typedef	UBYTE	*ClassID;

/* you can use this type to point to a "generic" message,
 * in the object-oriented programming parlance.  Based on
 * the value of 'MethodID', you dispatch to processing
 * for the various message types.  The meaningful parameter
 * packet structure definitions are defined below.
 */
typedef struct {
    ULONG MethodID;
    /* method-specific data follows, some examples below */
}		*Msg;

/*
 * Class id strings for Intuition classes.
 * There's no real reason to use the uppercase constants
 * over the lowercase strings, but this makes a good place
 * to list the names of the built-in classes.
 */
#define ROOTCLASS	"rootclass"		/* classusr.h	  */
#define IMAGECLASS	"imageclass"		/* imageclass.h   */
#define FRAMEICLASS	"frameiclass"
#define SYSICLASS	"sysiclass"
#define FILLRECTCLASS	"fillrectclass"
#define GADGETCLASS	"gadgetclass"		/* gadgetclass.h  */
#define PROPGCLASS	"propgclass"
#define STRGCLASS	"strgclass"
#define BUTTONGCLASS	"buttongclass"
#define FRBUTTONCLASS	"frbuttonclass"
#define GROUPGCLASS	"groupgclass"
#define ICCLASS		"icclass"		/* icclass.h	  */
#define MODELCLASS	"modelclass"
#define ITEXTICLASS	"itexticlass"
#define POINTERCLASS	"pointerclass"		/* pointerclass.h */

/* Dispatched method ID's
 * NOTE: Applications should use Intuition entry points, not direct
 * DoMethod() calls, for NewObject, DisposeObject, SetAttrs,
 * SetGadgetAttrs, and GetAttr.
 */

#define OM_Dummy	(0x100)
#define OM_NEW		(0x101)	/* 'object' parameter is "true class"	*/
#define OM_DISPOSE	(0x102)	/* delete self (no parameters)		*/
#define OM_SET		(0x103)	/* set attributes (in tag list)		*/
#define OM_GET		(0x104)	/* return single attribute value	*/
#define OM_ADDTAIL	(0x105)	/* add self to a List (let root do it)	*/
#define OM_REMOVE	(0x106)	/* remove self from list		*/
#define OM_NOTIFY	(0x107)	/* send to self: notify dependents	*/
#define OM_UPDATE	(0x108)	/* notification message from somebody	*/
#define OM_ADDMEMBER	(0x109)	/* used by various classes with lists	*/
#define OM_REMMEMBER	(0x10A)	/* used by various classes with lists	*/

/* Parameter "Messages" passed to methods	*/

/* OM_NEW and OM_SET	*/
struct opSet {
    ULONG		MethodID;
    struct TagItem	*ops_AttrList;	/* new attributes	*/
    struct GadgetInfo	*ops_GInfo;	/* always there for gadgets,
					 * when SetGadgetAttrs() is used,
					 * but will be NULL for OM_NEW
					 */
};

/* OM_NOTIFY, and OM_UPDATE	*/
struct opUpdate {
    ULONG		MethodID;
    struct TagItem	*opu_AttrList;	/* new attributes	*/
    struct GadgetInfo	*opu_GInfo;	/* non-NULL when SetGadgetAttrs or
					 * notification resulting from gadget
					 * input occurs.
					 */
    ULONG		opu_Flags;	/* defined below	*/
};

/* this flag means that the update message is being issued from
 * something like an active gadget, a la GACT_FOLLOWMOUSE.  When
 * the gadget goes inactive, it will issue a final update
 * message with this bit cleared.  Examples of use are for
 * GACT_FOLLOWMOUSE equivalents for propgadclass, and repeat strobes
 * for buttons.
 */
#define OPUF_INTERIM	(1<<0)

/* OM_GET	*/
struct opGet {
    ULONG		MethodID;
    ULONG		opg_AttrID;
    ULONG		*opg_Storage;	/* may be other types, but "int"
					 * types are all ULONG
					 */
};

/* OM_ADDTAIL	*/
struct opAddTail {
    ULONG		MethodID;
    struct List		*opat_List;
};

/* OM_ADDMEMBER, OM_REMMEMBER	*/
#define  opAddMember opMember
struct opMember {
    ULONG		MethodID;
    Object		*opam_Object;
};


#endif
```

## 9.6. intuition/cghooks.h — GadgetInfo (passed to custom gadget methods)

// Source: NDK_3.9/Include/include_h/intuition/cghooks.h
// GadgetInfo is the context passed to custom gadget hook callbacks.

```c
#ifndef INTUITION_CGHOOKS_H
#define INTUITION_CGHOOKS_H 1
/*
**  $VER: cghooks.h 38.1 (11.11.1991)
**  Includes Release 45.1
**
**  Custom Gadget processing
**
**  (C) Copyright 1988-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef INTUITION_INTUITION_H
#include <intuition/intuition.h>
#endif

/*
 * Package of information passed to custom and 'boopsi'
 * gadget "hook" functions.  This structure is READ ONLY.
 */
struct GadgetInfo {

    struct Screen		*gi_Screen;
    struct Window		*gi_Window;	/* null for screen gadgets */
    struct Requester		*gi_Requester;	/* null if not GTYP_REQGADGET */

    /* rendering information:
     * don't use these without cloning/locking.
     * Official way is to call ObtainRPort()
     */
    struct RastPort		*gi_RastPort;
    struct Layer		*gi_Layer;

    /* copy of dimensions of screen/window/g00/req(/group)
     * that gadget resides in.	Left/Top of this box is
     * offset from window mouse coordinates to gadget coordinates
     *		screen gadgets:			0,0 (from screen coords)
     *	window gadgets (no g00):	0,0
     *	GTYP_GZZGADGETs (borderlayer):		0,0
     *	GZZ innerlayer gadget:		borderleft, bordertop
     *	Requester gadgets:		reqleft, reqtop
     */
    struct IBox			gi_Domain;

    /* these are the pens for the window or screen	*/
    struct {
	UBYTE	DetailPen;
	UBYTE	BlockPen;
    }				gi_Pens;

    /* the Detail and Block pens in gi_DrInfo->dri_Pens[] are
     * for the screen.	Use the above for window-sensitive
     * colors.
     */
    struct DrawInfo		*gi_DrInfo;

    /* reserved space: this structure is extensible
     * anyway, but using these saves some recompilation
     */
    ULONG			gi_Reserved[6];
};

/*** system private data structure for now ***/
/* prop gadget extra info	*/
struct PGX	{
    struct IBox	pgx_Container;
    struct IBox	pgx_NewKnob;
};

/* this casts MutualExclude for easy assignment of a hook
 * pointer to the unused MutualExclude field of a custom gadget
 */
#define CUSTOM_HOOK( gadget ) ( (struct Hook *) (gadget)->MutualExclude)

#endif
```

## 9.7. intuition/icclass.h — ICA_TARGET, ICA_MAP, ICM_* methods

// Source: NDK_3.9/Include/include_h/intuition/icclass.h
// Interconnection class — notification wiring between BOOPSI objects.

```c
#ifndef INTUITION_ICCLASS_H
#define INTUITION_ICCLASS_H
/*
**  $VER: icclass.h 38.1 (11.11.1991)
**  Includes Release 45.1
**
**  Gadget/object interconnection classes
**
**  (C) Copyright 1989-2001 Amiga, Inc.
**	    All Rights Reserved
*/


#ifndef UTILITY_TAGITEM_H
#include <utility/tagitem.h>
#endif

#define ICM_Dummy	(0x0401L)	/* used for nothing		*/
#define ICM_SETLOOP	(0x0402L)	/* set/increment loop counter	*/
#define ICM_CLEARLOOP	(0x0403L)	/* clear/decrement loop counter	*/
#define ICM_CHECKLOOP	(0x0404L)	/* set/increment loop		*/

/* no parameters for ICM_SETLOOP, ICM_CLEARLOOP, ICM_CHECKLOOP	*/

/* interconnection attributes used by icclass, modelclass, and gadgetclass */
#define ICA_Dummy	(TAG_USER+0x40000L)
#define ICA_TARGET	(ICA_Dummy + 1)
	/* interconnection target		*/
#define ICA_MAP		(ICA_Dummy + 2)
	/* interconnection map tagitem list	*/
#define ICSPECIAL_CODE	(ICA_Dummy + 3)
	/* a "pseudo-attribute", see below.	*/

/* Normally, the value for ICA_TARGET is some object pointer,
 * but if you specify the special value ICTARGET_IDCMP, notification
 * will be send as an IDCMP_IDCMPUPDATE message to the appropriate window's
 * IDCMP port.	See the definition of IDCMP_IDCMPUPDATE.
 *
 * When you specify ICTARGET_IDCMP for ICA_TARGET, the map you
 * specify will be applied to derive the attribute list that is
 * sent with the IDCMP_IDCMPUPDATE message.  If you specify a map list
 * which results in the attribute tag id ICSPECIAL_CODE, the
 * lower sixteen bits of the corresponding ti_Data value will
 * be copied into the Code field of the IDCMP_IDCMPUPDATE IntuiMessage.
 */
#define ICTARGET_IDCMP	(~0L)

#endif /* INTUITION_ICCLASS_H */
```

## 9.8. intuition/gadgetclass.h — GA_* tags, PGA_* (prop gadget), STRINGA_* tags, GM_* methods

// Source: NDK_3.9/Include/include_h/intuition/gadgetclass.h
// gadgetclass/propgclass/strgclass attribute tag IDs and the GM_HITTEST/RENDER/GOACTIVE/HANDLEINPUT method vocabulary.

```c
#ifndef INTUITION_GADGETCLASS_H
#define INTUITION_GADGETCLASS_H
/*
**	$VER: gadgetclass.h 44.1 (19.10.1999)
**	Includes Release 45.1
**
**	Custom and 'boopsi' gadget class interface
**
**	(C) Copyright 1987-2001 Amiga, Inc.
**	    All Rights Reserved
*/

/*****************************************************************************/

#ifndef INTUITION_INTUITION_H
#include <intuition/intuition.h>
#endif

#ifndef UTILITY_TAGITEM_H
#include <utility/tagitem.h>
#endif

/*****************************************************************************/

/* NOTE:  <intuition/iobsolete.h> is included at the END of this file! */

/*****************************************************************************/

/* Gadget class attributes */
#define	GA_Dummy 		(TAG_USER+0x30000)

#define	GA_Left			(GA_Dummy+1)
    /* (LONG) Left edge of the gadget relative to the left edge of
     * the window */

#define	GA_RelRight		(GA_Dummy+2)
    /* (LONG) Left edge of the gadget relative to the right edge of
     * the window */

#define	GA_Top			(GA_Dummy+3)
    /* (LONG) Top edge of the gadget relative to the top edge of
     * the window */

#define	GA_RelBottom		(GA_Dummy+4)
    /* (LONG) Top edge of the gadget relative to the bottom edge
     * of the window */

#define	GA_Width		(GA_Dummy+5)
    /* (LONG) Width of the gadget */

#define	GA_RelWidth		(GA_Dummy+6)
    /* (LONG) Width of the gadget relative to the width of the
     * window */

#define	GA_Height		(GA_Dummy+7)
    /* (LONG) Height of the gadget */

#define	GA_RelHeight		(GA_Dummy+8)
    /* (LONG) Height of the gadget relative to the height of
     * the window */

#define	GA_Text			(GA_Dummy+9)
    /* (STRPTR) Gadget imagry is NULL terminated string */

#define	GA_Image		(GA_Dummy+10)
    /* (struct Image *) Gadget imagry is an image */

#define	GA_Border		(GA_Dummy+11)
    /* (struct Border *) Gadget imagry is a border */

#define	GA_SelectRender		(GA_Dummy+12)
    /* (struct Image *) Selected gadget imagry */

#define	GA_Highlight		(GA_Dummy+13)
    /* (UWORD) One of GFLG_GADGHNONE, GFLG_GADGHBOX, GFLG_GADGHCOMP,
     * or GFLG_GADGHIMAGE */

#define	GA_Disabled		(GA_Dummy+14)
    /* (BOOL) Indicate whether gadget is disabled or not.
     * Defaults to FALSE. */

#define	GA_GZZGadget		(GA_Dummy+15)
    /* (BOOL) Indicate whether the gadget is for
     * WFLG_GIMMEZEROZERO window borders or not.  Defaults
     * to FALSE. */

#define	GA_ID			(GA_Dummy+16)
    /* (UWORD) Gadget ID assigned by the application */

#define	GA_UserData		(GA_Dummy+17)
    /* (APTR) Application specific data */

#define	GA_SpecialInfo		(GA_Dummy+18)
    /* (APTR) Gadget specific data */

#define	GA_Selected		(GA_Dummy+19)
    /* (BOOL) Indicate whether the gadget is selected or not.
     * Defaults to FALSE */

#define	GA_EndGadget		(GA_Dummy+20)
    /* (BOOL) When set tells the system that when this gadget
     * is selected causes the requester that it is in to be
     * ended.  Defaults to FALSE. */

#define	GA_Immediate		(GA_Dummy+21)
    /* (BOOL) When set indicates that the gadget is to
     * notify the application when it becomes active.  Defaults
     * to FALSE. */

#define	GA_RelVerify		(GA_Dummy+22)
    /* (BOOL) When set indicates that the application wants to
     * verify that the pointer was still over the gadget when
     * the select button is released.  Defaults to FALSE. */

#define	GA_FollowMouse		(GA_Dummy+23)
    /* (BOOL) When set indicates that the application wants to
     * be notified of mouse movements while the gadget is active.
     * It is recommmended that GA_Immediate and GA_RelVerify are
     * also used so that the active gadget can be tracked by the
     * application.  Defaults to FALSE. */

#define	GA_RightBorder		(GA_Dummy+24)
    /* (BOOL) Indicate whether the gadget is in the right border
     * or not.  Defaults to FALSE. */

#define	GA_LeftBorder		(GA_Dummy+25)
    /* (BOOL) Indicate whether the gadget is in the left border
     * or not.  Defaults to FALSE. */

#define	GA_TopBorder		(GA_Dummy+26)
    /* (BOOL) Indicate whether the gadget is in the top border
     * or not.  Defaults to FALSE. */

#define	GA_BottomBorder		(GA_Dummy+27)
    /* (BOOL) Indicate whether the gadget is in the bottom border
     * or not.  Defaults to FALSE. */

#define	GA_ToggleSelect		(GA_Dummy+28)
    /* (BOOL) Indicate whether the gadget is toggle-selected
     * or not.  Defaults to FALSE. */

#define	GA_SysGadget		(GA_Dummy+29)
    /* (BOOL) Reserved for system use to indicate that the
     * gadget belongs to the system.  Defaults to FALSE. */

#define	GA_SysGType		(GA_Dummy+30)
    /* (UWORD) Reserved for system use to indicate the
     * gadget type. */

#define	GA_Previous		(GA_Dummy+31)
    /* (struct Gadget *) Previous gadget in the linked list.
     * NOTE: This attribute CANNOT be used to link new gadgets
     * into the gadget list of an open window or requester.
     * You must use AddGList(). */

#define	GA_Next			(GA_Dummy+32)
    /* (struct Gadget *) Next gadget in the linked list. */

#define	GA_DrawInfo		(GA_Dummy+33)
    /* (struct DrawInfo *) Some gadgets need a DrawInfo at creation time */

/* You should use at most ONE of GA_Text, GA_IntuiText, and GA_LabelImage */
#define GA_IntuiText		(GA_Dummy+34)
    /* (struct IntuiText *) Label is an IntuiText. */

#define GA_LabelImage		(GA_Dummy+35)
    /* (Object *) Label is an image object. */

#define GA_TabCycle		(GA_Dummy+36)
    /* (BOOL) Indicate whether gadget is part of TAB/SHIFT-TAB cycle
     * activation.  Defaults to FALSE.  New for V37. */

#define GA_GadgetHelp		(GA_Dummy+37)
    /* (BOOL) Indicate whether gadget is to send IDCMP_GADGETHELP.
     * Defaults to FALSE.  New for V39. */

#define GA_Bounds		(GA_Dummy+38)
    /* (struct IBox *) Copied into the extended gadget's bounds.
     * New for V39. */

#define GA_RelSpecial		(GA_Dummy+39)
    /* (BOOL) Indicate whether gadget has special relativity.  Defaults to
     * FALSE.  New for V39. */

#define	GA_TextAttr		(GA_Dummy+40)
    /* (struct TextAttr *) Indicate the font to use for the gadget.
     * New for V42. */

#define	GA_ReadOnly		(GA_Dummy+41)
    /* (BOOL) Indicate that the gadget is read-only (non-selectable).
     * Defaults to FALSE. New for V42. */

#define	GA_Underscore		(GA_Dummy+42)
    /* (UBYTE) Underscore/escape character for keyboard shortcuts.
     * Defaults to '_' . New for V44. */

#define	GA_ActivateKey		(GA_Dummy+43)
    /* (STRPTR) Set/Get the gadgets shortcut/activation key(s)
     * Defaults to NULL. New for V44. */

#define	GA_BackFill		(GA_Dummy+44)
    /* (struct Hook *) Backfill pattern hook.
     * Defaults to NULL. New for V44. */

#define	GA_GadgetHelpText		(GA_Dummy+45)
    /* (STRPTR) **RESERVERD/PRIVATE DO NOT USE**
     * Defaults to NULL. New for V44. */

#define	GA_UserInput		(GA_Dummy+46)
	/* (BOOL) Notification tag indicates this notification is from the activite
	 * gadget receiving user input - an attempt to make IDCMPUPDATE more efficient.
     * Defaults to FALSE. New for V44. */

/*****************************************************************************/

/* PROPGCLASS attributes */
#define PGA_Dummy	(TAG_USER+0x31000)
#define PGA_Freedom	(PGA_Dummy+0x0001)
	/* only one of FREEVERT or FREEHORIZ */
#define PGA_Borderless	(PGA_Dummy+0x0002)
#define PGA_HorizPot	(PGA_Dummy+0x0003)
#define PGA_HorizBody	(PGA_Dummy+0x0004)
#define PGA_VertPot	(PGA_Dummy+0x0005)
#define PGA_VertBody	(PGA_Dummy+0x0006)
#define PGA_Total	(PGA_Dummy+0x0007)
#define PGA_Visible	(PGA_Dummy+0x0008)
#define PGA_Top		(PGA_Dummy+0x0009)
/* New for V37: */
#define PGA_NewLook	(PGA_Dummy+0x000A)

/*****************************************************************************/

/* STRGCLASS attributes */
#define STRINGA_Dummy  		(TAG_USER     +0x32000)
#define STRINGA_MaxChars	(STRINGA_Dummy+0x0001)
/* Note:  There is a minor problem with Intuition when using boopsi integer
 * gadgets (which are requested by using STRINGA_LongInt).  Such gadgets
 * must not have a STRINGA_MaxChars to be bigger than 15.  Setting
 * STRINGA_MaxChars for a boopsi integer gadget will cause a mismatched
 * FreeMem() to occur.
 */

#define STRINGA_Buffer		(STRINGA_Dummy+0x0002)
#define STRINGA_UndoBuffer	(STRINGA_Dummy+0x0003)
#define STRINGA_WorkBuffer	(STRINGA_Dummy+0x0004)
#define STRINGA_BufferPos	(STRINGA_Dummy+0x0005)
#define STRINGA_DispPos		(STRINGA_Dummy+0x0006)
#define STRINGA_AltKeyMap	(STRINGA_Dummy+0x0007)
#define STRINGA_Font		(STRINGA_Dummy+0x0008)
#define STRINGA_Pens		(STRINGA_Dummy+0x0009)
#define STRINGA_ActivePens	(STRINGA_Dummy+0x000A)
#define STRINGA_EditHook	(STRINGA_Dummy+0x000B)
#define STRINGA_EditModes	(STRINGA_Dummy+0x000C)

/* booleans */
#define STRINGA_ReplaceMode	(STRINGA_Dummy+0x000D)
#define STRINGA_FixedFieldMode	(STRINGA_Dummy+0x000E)
#define STRINGA_NoFilterMode	(STRINGA_Dummy+0x000F)

#define STRINGA_Justification	(STRINGA_Dummy+0x0010)
	/* GACT_STRINGCENTER, GACT_STRINGLEFT, GACT_STRINGRIGHT */
#define STRINGA_LongVal		(STRINGA_Dummy+0x0011)
#define STRINGA_TextVal		(STRINGA_Dummy+0x0012)

#define STRINGA_ExitHelp	(STRINGA_Dummy+0x0013)
	/* STRINGA_ExitHelp is new for V37, and ignored by V36.
	 * Set this if you want the gadget to exit when Help is
	 * pressed.  Look for a code of 0x5F, the rawkey code for Help */

#define SG_DEFAULTMAXCHARS	(128)

/*****************************************************************************/

/* Gadget layout related attributes */
#define	LAYOUTA_Dummy 		(TAG_USER+0x38000)
#define LAYOUTA_LayoutObj	(LAYOUTA_Dummy+0x0001)
#define LAYOUTA_Spacing		(LAYOUTA_Dummy+0x0002)
#define LAYOUTA_Orientation	(LAYOUTA_Dummy+0x0003)

#define	LAYOUTA_ChildMaxWidth	(LAYOUTA_Dummy+0x0004)
    /* (BOOL) Child objects are of equal width.  Should default to TRUE for
     * gadgets with a horizontal orientation.  New for V42. */
#define	LAYOUTA_ChildMaxHeight	(LAYOUTA_Dummy+0x0005)
    /* (BOOL) Child objects are of equal height.  Should default to TRUE for
     * gadgets with a vertical orientation.  New for V42. */

/* orientation values */
#define LORIENT_NONE	0
#define LORIENT_HORIZ	1
#define LORIENT_VERT	2

/*****************************************************************************/

/* Gadget Method ID's */
#define GM_Dummy	(-1)
    /* not used for anything */

#define GM_HITTEST	(0)
    /* return GMR_GADGETHIT if you are clicked on (whether or not you
     * are disabled). */

#define GM_RENDER	(1)
    /* draw yourself, in the appropriate state */

#define GM_GOACTIVE	(2)
    /* you are now going to be fed input */

#define GM_HANDLEINPUT	(3)
    /* handle that input */

#define GM_GOINACTIVE	(4)
    /* whether or not by choice, you are done */

#define GM_HELPTEST	(5)
    /* Will you send gadget help if the mouse is at the specified coordinates?
     * See below for possible GMR_ values. */

#define GM_LAYOUT	(6)
    /* re-evaluate your size based on the GadgetInfo domain.
     * Do NOT re-render yourself yet, you will be called when it is
     * time... */

#define GM_DOMAIN	(7)
    /* Used to obtain the sizing requirements of an object.  Does not
     * require an object. */

#define GM_KEYTEST	(8)
    /* return GMR_GADGETHIT if you activation key matches (whether or not you
     * are disabled). */

#define GM_KEYGOACTIVE	(9)

#define GM_KEYGOINACTIVE	(10)

/*****************************************************************************/

/* Parameter "Messages" passed to gadget class methods	*/

/* GM_HITTEST and GM_HELPTEST send this message.
 * For GM_HITTEST, gpht_Mouse are coordinates relative to the gadget
 * select box.  For GM_HELPTEST, the coordinates are relative to
 * the gadget bounding box (which defaults to the select box).
 */
struct gpHitTest
{
    ULONG		MethodID;
    struct GadgetInfo	*gpht_GInfo;
    struct
    {
	WORD	X;
	WORD	Y;
    }			gpht_Mouse;
};

/* For GM_HITTEST, return GMR_GADGETHIT if you were indeed hit,
 * otherwise return zero.
 *
 * For GM_HELPTEST, return GMR_NOHELPHIT (zero) if you were not hit.
 * Typically, return GMR_HELPHIT if you were hit.
 * It is possible to pass a UWORD to the application via the Code field
 * of the IDCMP_GADGETHELP message.  Return GMR_HELPCODE or'd with
 * the UWORD-sized result you wish to return.
 *
 * GMR_HELPHIT yields a Code value of ((UWORD) ~0), which should
 * mean "nothing particular" to the application.
 */

#define GMR_GADGETHIT	(0x00000004)	/* GM_HITTEST hit */

#define GMR_NOHELPHIT	(0x00000000)	/* GM_HELPTEST didn't hit */
#define GMR_HELPHIT	(0xFFFFFFFF)	/* GM_HELPTEST hit, return code = ~0 */
#define GMR_HELPCODE	(0x00010000)	/* GM_HELPTEST hit, return low word as code */

/*****************************************************************************/

/* GM_RENDER	*/
struct gpRender
{
    ULONG		MethodID;
    struct GadgetInfo	*gpr_GInfo;	/* gadget context		*/
    struct RastPort	*gpr_RPort;	/* all ready for use		*/
    LONG		gpr_Redraw;	/* might be a "highlight pass"	*/
};

/* values of gpr_Redraw	*/
#define GREDRAW_UPDATE	(2)	/* incremental update, e.g. prop slider	*/
#define GREDRAW_REDRAW	(1)	/* redraw gadget	*/
#define GREDRAW_TOGGLE	(0)	/* toggle highlight, if applicable	*/

/*****************************************************************************/

/* GM_GOACTIVE, GM_HANDLEINPUT	*/
struct gpInput
{
    ULONG		MethodID;
    struct GadgetInfo	*gpi_GInfo;
    struct InputEvent	*gpi_IEvent;
    LONG		*gpi_Termination;
    struct
    {
	WORD	X;
	WORD	Y;
    }			gpi_Mouse;

    /* (V39) Pointer to TabletData structure, if this event originated
     * from a tablet which sends IESUBCLASS_NEWTABLET events, or NULL if
     * not.
     *
     * DO NOT ATTEMPT TO READ THIS FIELD UNDER INTUITION PRIOR TO V39!
     * IT WILL BE INVALID!
     */
    struct TabletData	*gpi_TabletData;
};

/* GM_HANDLEINPUT and GM_GOACTIVE  return code flags	*/
/* return GMR_MEACTIVE (0) alone if you want more input.
 * Otherwise, return ONE of GMR_NOREUSE and GMR_REUSE, and optionally
 * GMR_VERIFY.
 */
#define GMR_MEACTIVE	(0)
#define GMR_NOREUSE	(1 << 1)
#define GMR_REUSE	(1 << 2)
#define GMR_VERIFY	(1 << 3)	/* you MUST set gpi_Termination */

/* New for V37:
 * You can end activation with one of GMR_NEXTACTIVE and GMR_PREVACTIVE,
 * which instructs Intuition to activate the next or previous gadget
 * that has GFLG_TABCYCLE set.
 */
#define GMR_NEXTACTIVE	(1 << 4)
#define GMR_PREVACTIVE	(1 << 5)

/*****************************************************************************/

/* GM_GOINACTIVE */
struct gpGoInactive
{
    ULONG		MethodID;
    struct GadgetInfo	*gpgi_GInfo;

    /* V37 field only!  DO NOT attempt to read under V36! */
    ULONG		gpgi_Abort;	/* gpgi_Abort=1 if gadget was aborted
					 * by Intuition and 0 if gadget went
					 * inactive at its own request
					 */
};

/*****************************************************************************/

/* New for V39: Intuition sends GM_LAYOUT to any GREL_ gadget when
 * the gadget is added to the window (or when the window opens, if
 * the gadget was part of the NewWindow.FirstGadget or the WA_Gadgets
 * list), or when the window is resized.  Your gadget can set the
 * GA_RelSpecial property to get GM_LAYOUT events without Intuition
 * changing the interpretation of your gadget select box.  This
 * allows for completely arbitrary resizing/repositioning based on
 * window size.
 */
/* GM_LAYOUT */
struct gpLayout
{
    ULONG		MethodID;
    struct GadgetInfo	*gpl_GInfo;
    ULONG		gpl_Initial;	/* non-zero if this method was invoked
					 * during AddGList() or OpenWindow()
					 * time.  zero if this method was invoked
					 * during window resizing. */
};

/*****************************************************************************/

/* The GM_DOMAIN method is used to obtain the sizing requirements of an
 * object for a class before ever creating an object. */

/* GM_DOMAIN */
struct gpDomain
{
    ULONG		 MethodID;
    struct GadgetInfo	*gpd_GInfo;
    struct RastPort	*gpd_RPort;	/* RastPort to layout for */
    LONG		 gpd_Which;
    struct IBox		 gpd_Domain;	/* Resulting domain */
    struct TagItem	*gpd_Attrs;	/* Additional attributes */
};

#define	GDOMAIN_MINIMUM		(0)
    /* Minimum size */

#define	GDOMAIN_NOMINAL		(1)
    /* Nominal size */

#define	GDOMAIN_MAXIMUM		(2)
    /* Maximum size */


/*****************************************************************************/

/* The GM_KEYTEST method is used to determin if a key press matches an
 * object's activation key(s). */

/* GM_KEYTEST send this message.
 */
struct gpKeyTest
{
    ULONG		 MethodID;
    struct GadgetInfo	*gpkt_GInfo;
    struct IntuiMessage *gpkt_IMsg;	/* The IntuiMessage that triggered this */
    ULONG		 gpkt_VanillaKey;
};

/*****************************************************************************/

/* The GM_KEYGOACTIVE method is called to "simulate" a gadget going down.
 * A gadget should render itself in a selected state when receiving
 * this message. If the class supports this method, it must return
 * GMR_KEYACTIVE.
 *
 * If a gadget returns zero for this method, it will subsequently be
 * activated via ActivateGadget() with a NULL IEvent.
 */

struct gpKeyInput
{
    ULONG MethodID;			/* GM_KEYGOACTIVE */
    struct GadgetInfo	*gpk_GInfo;
    struct InputEvent	*gpk_IEvent;
    LONG		*gpk_Termination;
};

#define GMR_KEYACTIVE	(1 << 4)
#define GMR_KEYVERIFY	(1 << 5)	/* you MUST set gpk_Termination */

/* The GM_KEYGOINACTIVE method is called to simulate the gadget release.
 * Upon receiving this message, the gadget should do everything a
 * normal gadget release would do.
 */

struct gpKeyGoInactive
{
    ULONG MethodID;			/* GM_KEYGOINACTIVE */
    struct GadgetInfo *gpki_GInfo;
    ULONG gpki_Abort;			/* TRUE if input was aborted */
};

/*****************************************************************************/

/* Include obsolete identifiers: */
#ifndef INTUITION_IOBSOLETE_H
#include <intuition/iobsolete.h>
#endif

/*****************************************************************************/

#endif
```

## 9.9. intuition/imageclass.h — IA_* tags, SYSIA_* system image tags, IM_* methods, IDS_* draw states, FRAME_* frame types

// Source: NDK_3.9/Include/include_h/intuition/imageclass.h
// imageclass/sysiclass/frameiclass. DEPTHIMAGE/ZOOMIMAGE/SIZEIMAGE/CLOSEIMAGE/LEFTIMAGE/UPIMAGE/... system gadget image IDs.

```c
#ifndef INTUITION_IMAGECLASS_H
#define INTUITION_IMAGECLASS_H
/*
**	$VER: imageclass.h 44.1 (19.10.1999)
**	Includes Release 45.1
**
**	definitions for the system image classes
**
**	(C) Copyright 1987-2001 Amiga, Inc.
**	    All Rights Reserved
*/

/******************************************************/

#ifndef INTUITION_INTUITION_H
#include <intuition/intuition.h>
#endif

/*
 * NOTE:  <intuition/iobsolete.h> is included at the END of this file!
 */

#define CUSTOMIMAGEDEPTH	(-1)
/* if image.Depth is this, it's a new Image class object */

/* some convenient macros and casts */
#define GADGET_BOX( g )	( (struct IBox *) &((struct Gadget *)(g))->LeftEdge )
#define IM_BOX( im )	( (struct IBox *) &((struct Image *)(im))->LeftEdge )
#define IM_FGPEN( im )	( (im)->PlanePick )
#define IM_BGPEN( im )	( (im)->PlaneOnOff )

/******************************************************/
#define IA_Dummy		(TAG_USER + 0x20000)
#define IA_Left			(IA_Dummy + 0x01)
#define IA_Top			(IA_Dummy + 0x02)
#define IA_Width		(IA_Dummy + 0x03)
#define IA_Height		(IA_Dummy + 0x04)
#define IA_FGPen		(IA_Dummy + 0x05)
		    /* IA_FGPen also means "PlanePick"	*/
#define IA_BGPen		(IA_Dummy + 0x06)
		    /* IA_BGPen also means "PlaneOnOff"	*/
#define IA_Data			(IA_Dummy + 0x07)
		    /* bitplanes, for classic image,
		     * other image classes may use it for other things
		     */
#define IA_LineWidth		(IA_Dummy + 0x08)
#define IA_Pens			(IA_Dummy + 0x0E)
		    /* pointer to UWORD pens[],
		     * ala DrawInfo.Pens, MUST be
		     * terminated by ~0.  Some classes can
		     * choose to have this, or SYSIA_DrawInfo,
		     * or both.
		     */
#define IA_Resolution		(IA_Dummy + 0x0F)
		    /* packed uwords for x/y resolution into a longword
		     * ala DrawInfo.Resolution
		     */

/**** see class documentation to learn which	*****/
/**** classes recognize these			*****/
#define IA_APattern		(IA_Dummy + 0x10)
#define IA_APatSize		(IA_Dummy + 0x11)
#define IA_Mode			(IA_Dummy + 0x12)
#define IA_Font			(IA_Dummy + 0x13)
#define IA_Outline		(IA_Dummy + 0x14)
#define IA_Recessed		(IA_Dummy + 0x15)
#define IA_DoubleEmboss		(IA_Dummy + 0x16)
#define IA_EdgesOnly		(IA_Dummy + 0x17)

/**** "sysiclass" attributes			*****/
#define SYSIA_Size		(IA_Dummy + 0x0B)
		    /* #define's below		*/
#define SYSIA_Depth		(IA_Dummy + 0x0C)
		    /* this is unused by Intuition.  SYSIA_DrawInfo
		     * is used instead for V36
		     */
#define SYSIA_Which		(IA_Dummy + 0x0D)
		    /* see #define's below	*/
#define SYSIA_DrawInfo		(IA_Dummy + 0x18)
		    /* pass to sysiclass, please */

/*****	obsolete: don't use these, use IA_Pens	*****/
#define SYSIA_Pens		IA_Pens
#define IA_ShadowPen		(IA_Dummy + 0x09)
#define IA_HighlightPen		(IA_Dummy + 0x0A)

/* New for V39: */
#define SYSIA_ReferenceFont	(IA_Dummy + 0x19)
		    /* Font to use as reference for scaling
		     * certain sysiclass images
		     */
#define IA_SupportsDisable	(IA_Dummy + 0x1a)
		    /* By default, Intuition ghosts gadgets itself,
		     * instead of relying on IDS_DISABLED or
		     * IDS_SELECTEDDISABLED.  An imageclass that
		     * supports these states should return this attribute
		     * as TRUE.  You cannot set or clear this attribute,
		     * however.
		     */

#define IA_FrameType		(IA_Dummy + 0x1b)
		    /* Starting with V39, FrameIClass recognizes
		     * several standard types of frame.  Use one
		     * of the FRAME_ specifiers below.	Defaults
		     * to FRAME_DEFAULT.
		     */

#define IA_Underscore		(IA_Dummy + 0x1c)
		    /* V44, Indicate underscore keyboard shortcut for image labels.
		     * (UBYTE) Defaults to '_'
		     */

#define IA_Scalable			(IA_Dummy + 0x1d)
		    /* V44, Attribute indicates this image is allowed
			 * to/can scale its rendering.
		     * (BOOL) Defaults to FALSE.
		     */

#define IA_ActivateKey			(IA_Dummy + 0x1e)
		    /* V44, Used to get an underscored label shortcut.
		     * Useful for labels attached to string gadgets.
		     * (UBYTE) Defaults to NULL.
		     */

#define IA_Screen			(IA_Dummy + 0x1f)
		    /* V44 Screen pointer, may be useful/required by certain classes.
		     * (struct Screen *)
		     */

#define IA_Precision			(IA_Dummy + 0x20)
		    /* V44 Precision value, typically pen precision but may be
		     * used for similar custom purposes.
		     * (ULONG)
		     */

/** next attribute: (IA_Dummy + 0x21)	**/
/*************************************************/

/* data values for SYSIA_Size	*/
#define SYSISIZE_MEDRES	(0)
#define SYSISIZE_LOWRES	(1)
#define SYSISIZE_HIRES	(2)

/*
 * SYSIA_Which tag data values:
 * Specifies which system gadget you want an image for.
 * Some numbers correspond to internal Intuition #defines
 */
#define DEPTHIMAGE	(0x00L)	/* Window depth gadget image */
#define ZOOMIMAGE	(0x01L)	/* Window zoom gadget image */
#define SIZEIMAGE	(0x02L)	/* Window sizing gadget image */
#define CLOSEIMAGE	(0x03L)	/* Window close gadget image */
#define SDEPTHIMAGE	(0x05L)	/* Screen depth gadget image */
#define LEFTIMAGE	(0x0AL)	/* Left-arrow gadget image */
#define UPIMAGE		(0x0BL)	/* Up-arrow gadget image */
#define RIGHTIMAGE	(0x0CL)	/* Right-arrow gadget image */
#define DOWNIMAGE	(0x0DL)	/* Down-arrow gadget image */
#define CHECKIMAGE	(0x0EL)	/* GadTools checkbox image */
#define MXIMAGE		(0x0FL)	/* GadTools mutual exclude "button" image */
/* New for V39: */
#define	MENUCHECK	(0x10L)	/* Menu checkmark image */
#define AMIGAKEY	(0x11L)	/* Menu Amiga-key image */

/* Data values for IA_FrameType (recognized by FrameIClass)
 *
 * FRAME_DEFAULT:  The standard V37-type frame, which has
 *	thin edges.
 * FRAME_BUTTON:  Standard button gadget frames, having thicker
 *	sides and nicely edged corners.
 * FRAME_RIDGE:  A ridge such as used by standard string gadgets.
 *	You can recess the ridge to get a groove image.
 * FRAME_ICONDROPBOX: A broad ridge which is the standard imagery
 *	for areas in AppWindows where icons may be dropped.
 */

#define FRAME_DEFAULT		0
#define FRAME_BUTTON		1
#define FRAME_RIDGE		2
#define FRAME_ICONDROPBOX	3


/* image message id's	*/
#define    IM_DRAW	0x202L	/* draw yourself, with "state" */
#define    IM_HITTEST	0x203L	/* return TRUE if click hits image	*/
#define    IM_ERASE	0x204L	/* erase yourself */
#define    IM_MOVE	0x205L	/* draw new and erase old, smoothly	*/

#define    IM_DRAWFRAME	0x206L	/* draw with specified dimensions */
#define    IM_FRAMEBOX	0x207L	/* get recommended frame around some box*/
#define    IM_HITFRAME	0x208L	/* hittest with dimensions */
#define    IM_ERASEFRAME 0x209L	/* erase with dimensions */
#define    IM_DOMAINFRAME	0x20AL  /* query image for its domain info (V44) */


/* image draw states or styles, for IM_DRAW */
/* Note that they have no bitwise meanings (unfortunately) */
#define    IDS_NORMAL		(0L)
#define    IDS_SELECTED		(1L)	/* for selected gadgets	    */
#define    IDS_DISABLED		(2L)	/* for disabled gadgets	    */
#define	   IDS_BUSY		(3L)	/* for future functionality */
#define    IDS_INDETERMINATE	(4L)	/* for future functionality */
#define    IDS_INACTIVENORMAL	(5L)	/* normal, in inactive window border */
#define    IDS_INACTIVESELECTED	(6L)	/* selected, in inactive border */
#define    IDS_INACTIVEDISABLED	(7L)	/* disabled, in inactive border */
#define	   IDS_SELECTEDDISABLED (8L)	/* disabled and selected    */

/* oops, please forgive spelling error by jimm */
#define IDS_INDETERMINANT IDS_INDETERMINATE

/* IM_FRAMEBOX	*/
struct impFrameBox {
    ULONG		MethodID;
    struct IBox	*imp_ContentsBox;	/* input: relative box of contents */
    struct IBox	*imp_FrameBox;		/* output: rel. box of encl frame  */
    struct DrawInfo	*imp_DrInfo;	/* NB: May be NULL */
    ULONG	imp_FrameFlags;
};

#define FRAMEF_SPECIFY	(1<<0)	/* Make do with the dimensions of FrameBox
				 * provided.
				 */

/* IM_DRAW, IM_DRAWFRAME	*/
struct impDraw
{
    ULONG		MethodID;
    struct RastPort	*imp_RPort;
    struct
    {
	WORD	X;
	WORD	Y;
    }			imp_Offset;

    ULONG		imp_State;
    struct DrawInfo	*imp_DrInfo;	/* NB: May be NULL */

    /* these parameters only valid for IM_DRAWFRAME */
    struct
    {
	WORD	Width;
	WORD	Height;
    }			imp_Dimensions;
};

/* IM_ERASE, IM_ERASEFRAME	*/
/* NOTE: This is a subset of impDraw	*/
struct impErase
{
    ULONG		MethodID;
    struct RastPort	*imp_RPort;
    struct
    {
	WORD	X;
	WORD	Y;
    }			imp_Offset;

    /* these parameters only valid for IM_ERASEFRAME */
    struct
    {
	WORD	Width;
	WORD	Height;
    }			imp_Dimensions;
};

/* IM_HITTEST, IM_HITFRAME	*/
struct impHitTest
{
    ULONG		MethodID;
    struct
    {
	WORD	X;
	WORD	Y;
    }			imp_Point;

    /* these parameters only valid for IM_HITFRAME */
    struct
    {
	WORD	Width;
	WORD	Height;
    }			imp_Dimensions;
};


/* The IM_DOMAINFRAME method is used to obtain the sizing
 * requirements of an image object within a layout group.
 */

/* IM_DOMAINFRAME */
struct impDomainFrame
{
    ULONG		 MethodID;
    struct DrawInfo	*imp_DrInfo;	/* DrawInfo */
    struct RastPort	*imp_RPort;	/* RastPort to layout for */
    LONG	 	 imp_Which;	/* what size - min/nominal/max */
    struct IBox		 imp_Domain;	/* Resulting domain */
    struct TagItem	*imp_Attrs;	/* Additional attributes */
};

/* Accepted vales for imp_Which.
 */
#define IDOMAIN_MINIMUM		(0)
#define IDOMAIN_NOMINAL		(1)
#define IDOMAIN_MAXIMUM		(2)

/* Include obsolete identifiers: */
#ifndef INTUITION_IOBSOLETE_H
#include <intuition/iobsolete.h>
#endif

#endif
```

## 9.10. intuition/pointerclass.h — POINTERA_* tags, POINTERXRESN_*, POINTERYRESN_*

// Source: NDK_3.9/Include/include_h/intuition/pointerclass.h
// pointerclass — BOOPSI mouse pointer objects with sprite imagery and resolution control.

```c
#ifndef INTUITION_POINTERCLASS_H
#define INTUITION_POINTERCLASS_H
/*
**  $VER: pointerclass.h 39.6 (15.2.1993)
**  Includes Release 45.1
**
**  'boopsi' pointer class interface
**
**  (C) Copyright 1992-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef INTUITION_INTUITION_H
#include <intuition/intuition.h>
#endif

#ifndef UTILITY_TAGITEM_H
#include <utility/tagitem.h>
#endif

/* The following tags are recognized at NewObject() time by
 * pointerclass:
 *
 * POINTERA_BitMap (struct BitMap *) - Pointer to bitmap to
 *	get pointer imagery from.  Bitplane data need not be
 *	in chip RAM.
 * POINTERA_XOffset (LONG) - X-offset of the pointer hotspot.
 * POINTERA_YOffset (LONG) - Y-offset of the pointer hotspot.
 * POINTERA_WordWidth (ULONG) - designed width of the pointer in words
 * POINTERA_XResolution (ULONG) - one of the POINTERXRESN_ flags below
 * POINTERA_YResolution (ULONG) - one of the POINTERYRESN_ flags below
 *
 */

#define POINTERA_Dummy	(TAG_USER + 0x39000)

#define POINTERA_BitMap		(POINTERA_Dummy + 0x01)
#define POINTERA_XOffset	(POINTERA_Dummy + 0x02)
#define POINTERA_YOffset	(POINTERA_Dummy + 0x03)
#define POINTERA_WordWidth	(POINTERA_Dummy + 0x04)
#define POINTERA_XResolution	(POINTERA_Dummy + 0x05)
#define POINTERA_YResolution	(POINTERA_Dummy + 0x06)

/* These are the choices for the POINTERA_XResolution attribute which
 * will determine what resolution pixels are used for this pointer.
 *
 * POINTERXRESN_DEFAULT (ECS-compatible pointer width)
 *	= 70 ns if SUPERHIRES-type mode, 140 ns if not
 *
 * POINTERXRESN_SCREENRES
 *	= Same as pixel speed of screen
 *
 * POINTERXRESN_LORES (pointer always in lores-like pixels)
 *	= 140 ns in 15kHz modes, 70 ns in 31kHz modes
 *
 * POINTERXRESN_HIRES (pointer always in hires-like pixels)
 *	= 70 ns in 15kHz modes, 35 ns in 31kHz modes
 *
 * POINTERXRESN_140NS (pointer always in 140 ns pixels)
 *	= 140 ns always
 *
 * POINTERXRESN_70NS (pointer always in 70 ns pixels)
 *	= 70 ns always
 *
 * POINTERXRESN_35NS (pointer always in 35 ns pixels)
 *	= 35 ns always
 */

#define POINTERXRESN_DEFAULT	0
#define POINTERXRESN_140NS	1
#define POINTERXRESN_70NS	2
#define POINTERXRESN_35NS	3

#define POINTERXRESN_SCREENRES	4
#define POINTERXRESN_LORES	5
#define POINTERXRESN_HIRES	6

/* These are the choices for the POINTERA_YResolution attribute which
 * will determine what vertical resolution is used for this pointer.
 *
 * POINTERYRESN_DEFAULT
 *	= In 15 kHz modes, the pointer resolution will be the same
 *	  as a non-interlaced screen.  In 31 kHz modes, the pointer
 *	  will be doubled vertically.  This means there will be about
 *	  200-256 pointer lines per screen.
 *
 * POINTERYRESN_HIGH
 * POINTERYRESN_HIGHASPECT
 *	= Where the hardware/software supports it, the pointer resolution
 *	  will be high.  This means there will be about 400-480 pointer
 *	  lines per screen.  POINTERYRESN_HIGHASPECT also means that
 *	  when the pointer comes out double-height due to hardware/software
 *	  restrictions, its width would be doubled as well, if possible
 *	  (to preserve aspect).
 *
 * POINTERYRESN_SCREENRES
 * POINTERYRESN_SCREENRESASPECT
 *	= Will attempt to match the vertical resolution of the pointer
 *	  to the screen's vertical resolution.	POINTERYRESN_SCREENASPECT also
 *	  means that when the pointer comes out double-height due to
 *	  hardware/software restrictions, its width would be doubled as well,
 *	  if possible (to preserve aspect).
 *
 */

#define POINTERYRESN_DEFAULT		0
#define POINTERYRESN_HIGH		2
#define POINTERYRESN_HIGHASPECT		3
#define POINTERYRESN_SCREENRES		4
#define POINTERYRESN_SCREENRESASPECT	5

/* Compatibility note:
 *
 * The AA chipset supports variable sprite width and resolution, but
 * the setting of width and resolution is global for all sprites.
 * When no other sprites are in use, Intuition controls the sprite
 * width and sprite resolution for correctness based on pointerclass
 * attributes specified by the creator of the pointer.	Intuition
 * controls sprite resolution with the VTAG_DEFSPRITERESN_SET tag
 * to VideoControl().  Applications can override this on a per-viewport
 * basis with the VTAG_SPRITERESN_SET tag to VideoControl().
 *
 * If an application uses a sprite other than the pointer sprite,
 * Intuition will automatically regenerate the pointer sprite's image in
 * a compatible width.	This might involve BitMap scaling of the imagery
 * you supply.
 *
 * If any sprites other than the pointer sprite were obtained with the
 * old GetSprite() call, Intuition assumes that the owner of those
 * sprites is unaware of sprite resolution, hence Intuition will set the
 * default sprite resolution (VTAG_DEFSPRITERESN_SET) to ECS-compatible,
 * instead of as requested by the various pointerclass attributes.
 *
 * No resolution fallback occurs when applications use ExtSprites.
 * Such applications are expected to use VTAG_SPRITERESN_SET tag if
 * necessary.
 *
 * NB:	Under release V39, only sprite width compatibility is implemented.
 * Sprite resolution compatibility was added for V40.
 */

#endif
```

## 9.11. intuition/sghooks.h — StringExtend, SGWork, EO_* edit operations, SGM_* modes, SGA_* actions, SGH_KEY/CLICK commands

// Source: NDK_3.9/Include/include_h/intuition/sghooks.h
// String gadget edit hook interface. Extensive inline documentation of the EditHook contract.

```c
#ifndef INTUITION_SGHOOKS_H
#define INTUITION_SGHOOKS_H TRUE
/*
**  $VER: sghooks.h 38.1 (11.11.1991)
**  Includes Release 45.1
**
**  string gadget extensions and hooks
**
**  (C) Copyright 1988-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

struct StringExtend {
    /* display specifications	*/
    struct TextFont *Font;	/* must be an open Font (not TextAttr)	*/
    UBYTE	Pens[2];	/* color of text/backgroun		*/
    UBYTE	ActivePens[2];	/* colors when gadget is active		*/

    /* edit specifications	*/
    ULONG	InitialModes;	/* initial mode flags, below		*/
    struct Hook *EditHook;	/* if non-NULL, must supply WorkBuffer	*/
    UBYTE	*WorkBuffer;	/* must be as large as StringInfo.Buffer*/

    ULONG	Reserved[4];	/* set to 0				*/
};

struct SGWork	{
    /* set up when gadget is first activated	*/
    struct Gadget	*Gadget;	/* the contestant itself	*/
    struct StringInfo	*StringInfo;	/* easy access to sinfo		*/
    UBYTE		*WorkBuffer;	/* intuition's planned result	*/
    UBYTE		*PrevBuffer;	/* what was there before	*/
    ULONG		Modes;		/* current mode			*/

    /* modified for each input event	*/
    struct InputEvent	*IEvent;	/* actual event: do not change	*/
    UWORD		Code;		/* character code, if one byte	*/
    WORD		BufferPos;	/* cursor position		*/
    WORD		NumChars;
    ULONG		Actions;	/* what Intuition will do	*/
    LONG		LongInt;	/* temp storage for longint	*/

    struct GadgetInfo	*GadgetInfo;	/* see cghooks.h		*/
    UWORD		EditOp;		/* from constants below		*/
};

/* SGWork.EditOp -
 * These values indicate what basic type of operation the global
 * editing hook has performed on the string before your gadget's custom
 * editing hook gets called.  You do not have to be concerned with the
 * value your custom hook leaves in the EditOp field, only if you
 * write a global editing hook.
 *
 * For most of these general edit operations, you'll want to compare
 * the BufferPos and NumChars of the StringInfo (before global editing)
 * and SGWork (after global editing).
 */

#define EO_NOOP		(0x0001)
	/* did nothing							*/
#define EO_DELBACKWARD	(0x0002)
	/* deleted some chars (maybe 0).				*/
#define EO_DELFORWARD	(0x0003)
	/* deleted some characters under and in front of the cursor	*/
#define EO_MOVECURSOR	(0x0004)
	/* moved the cursor						*/
#define EO_ENTER	(0x0005)
	/* "enter" or "return" key, terminate				*/
#define EO_RESET	(0x0006)
	/* current Intuition-style undo					*/
#define EO_REPLACECHAR	(0x0007)
	/* replaced one character and (maybe) advanced cursor		*/
#define EO_INSERTCHAR	(0x0008)
	/* inserted one char into string or added one at end		*/
#define EO_BADFORMAT	(0x0009)
	/* didn't like the text data, e.g., Bad LONGINT			*/
#define EO_BIGCHANGE	(0x000A)	/* unused by Intuition	*/
	/* complete or major change to the text, e.g. new string	*/
#define EO_UNDO		(0x000B)	/* unused by Intuition	*/
	/* some other style of undo					*/
#define EO_CLEAR	(0x000C)
	/* clear the string						*/
#define EO_SPECIAL	(0x000D)	/* unused by Intuition	*/
	/* some operation that doesn't fit into the categories here	*/


/* Mode Flags definitions (ONLY first group allowed as InitialModes)	*/
#define SGM_REPLACE	(1L << 0)	/* replace mode			*/
/* please initialize StringInfo with in-range value of BufferPos
 * if you are using SGM_REPLACE mode.
 */

#define SGM_FIXEDFIELD	(1L << 1)	/* fixed length buffer		*/
					/* always set SGM_REPLACE, too	*/
#define SGM_NOFILTER	(1L << 2)	/* don't filter control chars	*/

/* SGM_EXITHELP is new for V37, and ignored by V36: */
#define SGM_EXITHELP	(1L << 7)	/* exit with code = 0x5F if HELP hit */


/* These Mode Flags are for internal use only				*/
#define SGM_NOCHANGE	(1L << 3)	/* no edit changes yet		*/
#define SGM_NOWORKB	(1L << 4)	/* Buffer == PrevBuffer		*/
#define SGM_CONTROL	(1L << 5)	/* control char escape mode	*/
#define SGM_LONGINT	(1L << 6)	/* an intuition longint gadget	*/

/* String Gadget Action Flags (put in SGWork.Actions by EditHook)	*/
#define SGA_USE		(0x1L)	/* use contents of SGWork		*/
#define SGA_END		(0x2L)	/* terminate gadget, code in Code field	*/
#define SGA_BEEP	(0x4L)	/* flash the screen for the user	*/
#define SGA_REUSE	(0x8L)	/* reuse input event			*/
#define SGA_REDISPLAY	(0x10L)	/* gadget visuals changed		*/

/* New for V37: */
#define SGA_NEXTACTIVE	(0x20L)	/* Make next possible gadget active.	*/
#define SGA_PREVACTIVE	(0x40L)	/* Make previous possible gadget active.*/

/* function id for only existing custom string gadget edit hook	*/

#define SGH_KEY		(1L)	/* process editing keystroke		*/
#define SGH_CLICK	(2L)	/* process mouse click cursor position	*/

/* Here's a brief summary of how the custom string gadget edit hook works:
 *	You provide a hook in StringInfo.Extension.EditHook.
 *	The hook is called in the standard way with the 'object'
 *	a pointer to SGWork, and the 'message' a pointer to a command
 *	block, starting either with (longword) SGH_KEY, SGH_CLICK,
 *	or something new.
 *
 *	You return 0 if you don't understand the command (SGH_KEY is
 *	required and assumed).	Return non-zero if you implement the
 *	command.
 *
 *   SGH_KEY:
 *	There are no parameters following the command longword.
 *
 *	Intuition will put its idea of proper values in the SGWork
 *	before calling you, and if you leave SGA_USE set in the
 *	SGWork.Actions field, Intuition will use the values
 *	found in SGWork fields WorkBuffer, NumChars, BufferPos,
 *	and LongInt, copying the WorkBuffer back to the StringInfo
 *	Buffer.
 *
 *	NOTE WELL: You may NOT change other SGWork fields.
 *
 *	If you clear SGA_USE, the string gadget will be unchanged.
 *
 *	If you set SGA_END, Intuition will terminate the activation
 *	of the string gadget.  If you also set SGA_REUSE, Intuition
 *	will reuse the input event after it deactivates your gadget.
 *
 *	In this case, Intuition will put the value found in SGWork.Code
 *	into the IntuiMessage.Code field of the IDCMP_GADGETUP message it
 *	sends to the application.
 *
 *	If you set SGA_BEEP, Intuition will call DisplayBeep(); use
 *	this if the user has typed in error, or buffer is full.
 *
 *	Set SGA_REDISPLAY if the changes to the gadget warrant a
 *	gadget redisplay.  Note: cursor movement requires a redisplay.
 *
 *	Starting in V37, you may set SGA_PREVACTIVE or SGA_NEXTACTIVE
 *	when you set SGA_END.  This tells Intuition that you want
 *	the next or previous gadget with GFLG_TABCYCLE to be activated.
 *
 *   SGH_CLICK:
 *	This hook command is called when Intuition wants to position
 *	the cursor in response to a mouse click in the string gadget.
 *
 *	Again, here are no parameters following the command longword.
 *
 *	This time, Intuition has already calculated the mouse position
 *	character cell and put it in SGWork.BufferPos.	The previous
 *	BufferPos value remains in the SGWork.StringInfo.BufferPos.
 *
 *	Intuition will again use the SGWork fields listed above for
 *	SGH_KEY.  One restriction is that you are NOT allowed to set
 *	SGA_END or SGA_REUSE for this command.	Intuition will not
 *	stand for a gadget which goes inactive when you click in it.
 *
 *	You should always leave the SGA_REDISPLAY flag set, since Intuition
 *	uses this processing when activating a string gadget.
 */

#endif
```

## 9.12. intuition/preferences.h — Preferences, PaperSize, PrinterType, serial bit codes

// Source: NDK_3.9/Include/include_h/intuition/preferences.h
// Old-style (system-configuration) preferences struct. Largely obsoleted by V36 Prefs/Env-Archive but SetPrefs() still reads the fields listed.

```c
#ifndef INTUITION_PREFERENCES_H
#define INTUITION_PREFERENCES_H TRUE
/*
**  $VER: preferences.h 38.2 (16.9.1992)
**  Includes Release 45.1
**
**  Structure definition for old-style preferences
**
**  (C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef DEVICES_TIMER_H
#include <devices/timer.h>
#endif

/* ======================================================================== */
/* === Preferences ======================================================== */
/* ======================================================================== */

/* these are the definitions for the printer configurations */
#define	FILENAME_SIZE	30	/* Filename size */
#define DEVNAME_SIZE	16	/* Device-name size */

#define	POINTERSIZE (1 + 16 + 1) * 2	/* Size of Pointer data buffer */

/* These defines are for the default font size.  These actually describe the
 * height of the defaults fonts.  The default font type is the topaz
 * font, which is a fixed width font that can be used in either
 * eighty-column or sixty-column mode.	The Preferences structure reflects
 * which is currently selected by the value found in the variable FontSize,
 * which may have either of the values defined below.  These values actually
 * are used to select the height of the default font.  By changing the
 * height, the resolution of the font changes as well.
 */
#define TOPAZ_EIGHTY 8
#define TOPAZ_SIXTY 9

/* Note:  Starting with V36, and continuing with each new version of
 * Intuition, an increasing number of fields of struct Preferences
 * are ignored by SetPrefs().  (Some fields are obeyed only at the
 * initial SetPrefs(), which comes from the devs:system-configuration
 * file).  Elements are generally superseded as new hardware or software
 * features demand more information than fits in struct Preferences.
 * Parts of struct Preferences must be ignored so that applications
 * calling GetPrefs(), modifying some other part of struct Preferences,
 * then calling SetPrefs(), don't end up truncating the extended
 * data.
 *
 * Consult the autodocs for SetPrefs() for further information as
 * to which fields are not always respected.
 */

struct Preferences
{
    /* the default font height */
    BYTE FontHeight;			/* height for system default font  */

    /* constant describing what's hooked up to the port */
    UBYTE PrinterPort;			/* printer port connection	   */

    /* the baud rate of the port */
    UWORD BaudRate;			/* baud rate for the serial port   */

    /* various timing rates */
    struct timeval KeyRptSpeed;		/* repeat speed for keyboard	   */
    struct timeval KeyRptDelay;		/* Delay before keys repeat	   */
    struct timeval DoubleClick;		/* Interval allowed between clicks */

    /* Intuition Pointer data */
    UWORD PointerMatrix[POINTERSIZE];	/* Definition of pointer sprite    */
    BYTE XOffset;			/* X-Offset for active 'bit'	   */
    BYTE YOffset;			/* Y-Offset for active 'bit'	   */
    UWORD color17;			/***********************************/
    UWORD color18;			/* Colours for sprite pointer	   */
    UWORD color19;			/***********************************/
    UWORD PointerTicks;			/* Sensitivity of the pointer	   */

    /* Workbench Screen colors */
    UWORD color0;			/***********************************/
    UWORD color1;			/*  Standard default colours	   */
    UWORD color2;			/*   Used in the Workbench	   */
    UWORD color3;			/***********************************/

    /* positioning data for the Intuition View */
    BYTE ViewXOffset;			/* Offset for top lefthand corner  */
    BYTE ViewYOffset;			/* X and Y dimensions		   */
    WORD ViewInitX, ViewInitY;		/* View initial offset values	   */

    BOOL EnableCLI;			/* CLI availability switch */

    /* printer configurations */
    UWORD PrinterType;			/* printer type		   */
    UBYTE PrinterFilename[FILENAME_SIZE];/* file for printer	   */

    /* print format and quality configurations */
    UWORD PrintPitch;			/* print pitch			   */
    UWORD PrintQuality;			/* print quality		   */
    UWORD PrintSpacing;			/* number of lines per inch	   */
    UWORD PrintLeftMargin;		/* left margin in characters	   */
    UWORD PrintRightMargin;		/* right margin in characters	   */
    UWORD PrintImage;			/* positive or negative		   */
    UWORD PrintAspect;			/* horizontal or vertical	   */
    UWORD PrintShade;			/* b&w, half-tone, or color	   */
    WORD PrintThreshold;		/* darkness ctrl for b/w dumps	   */

    /* print paper descriptors */
    UWORD PaperSize;			/* paper size			   */
    UWORD PaperLength;			/* paper length in number of lines */
    UWORD PaperType;			/* continuous or single sheet	   */

    /* Serial device settings: These are six nibble-fields in three bytes */
    /* (these look a little strange so the defaults will map out to zero) */
    UBYTE   SerRWBits;	 /* upper nibble = (8-number of read bits)	*/
			 /* lower nibble = (8-number of write bits)	*/
    UBYTE   SerStopBuf;  /* upper nibble = (number of stop bits - 1)	*/
			 /* lower nibble = (table value for BufSize)	*/
    UBYTE   SerParShk;	 /* upper nibble = (value for Parity setting)	*/
			 /* lower nibble = (value for Handshake mode)	*/
    UBYTE   LaceWB;	 /* if workbench is to be interlaced		*/

    UBYTE   Pad[ 12 ];
    UBYTE   PrtDevName[DEVNAME_SIZE];	/* device used by printer.device
					 * (omit the ".device")
					 */
    UBYTE   DefaultPrtUnit;	/* default unit opened by printer.device */
    UBYTE   DefaultSerUnit;	/* default serial unit */

    BYTE    RowSizeChange;	/* affect NormalDisplayRows/Columns	*/
    BYTE    ColumnSizeChange;

    UWORD    PrintFlags;	/* user preference flags */
    UWORD    PrintMaxWidth;	/* max width of printed picture in 10ths/in */
    UWORD    PrintMaxHeight;	/* max height of printed picture in 10ths/in */
    UBYTE    PrintDensity;	/* print density */
    UBYTE    PrintXOffset;	/* offset of printed picture in 10ths/inch */

    UWORD    wb_Width;		/* override default workbench width  */
    UWORD    wb_Height;		/* override default workbench height */
    UBYTE    wb_Depth;		/* override default workbench depth  */

    UBYTE    ext_size;		/* extension information -- do not touch! */
			    /* extension size in blocks of 64 bytes */
};


/* Workbench Interlace (use one bit) */
#define LACEWB			(1<< 0)
#define LW_RESERVED	1		/* internal use only */

/* Enable_CLI	*/
#define SCREEN_DRAG	(1<<14)
#define MOUSE_ACCEL	(1L<<15)

/* PrinterPort */
#define PARALLEL_PRINTER 0x00
#define SERIAL_PRINTER	0x01

/* BaudRate */
#define BAUD_110	0x00
#define BAUD_300	0x01
#define BAUD_1200	0x02
#define BAUD_2400	0x03
#define BAUD_4800	0x04
#define BAUD_9600	0x05
#define BAUD_19200	0x06
#define BAUD_MIDI	0x07

/* PaperType */
#define FANFOLD	0x00
#define SINGLE		0x80

/* PrintPitch */
#define PICA		0x000
#define ELITE		0x400
#define FINE		0x800

/* PrintQuality */
#define DRAFT		0x000
#define LETTER		0x100

/* PrintSpacing */
#define SIX_LPI		0x000
#define EIGHT_LPI	0x200

/* Print Image */
#define IMAGE_POSITIVE	0x00
#define IMAGE_NEGATIVE	0x01

/* PrintAspect */
#define ASPECT_HORIZ	0x00
#define ASPECT_VERT	0x01

/* PrintShade */
#define SHADE_BW	0x00
#define SHADE_GREYSCALE	0x01
#define SHADE_COLOR	0x02

/* PaperSize (all paper sizes have a zero in the lowest nybble) */
#define US_LETTER	0x00
#define US_LEGAL	0x10
#define N_TRACTOR	0x20
#define W_TRACTOR	0x30
#define CUSTOM		0x40

/* New PaperSizes for V36: */
#define EURO_A0	0x50		/* European size A0: 841 x 1189 */
#define EURO_A1	0x60		/* European size A1: 594 x 841 */
#define EURO_A2	0x70		/* European size A2: 420 x 594 */
#define EURO_A3	0x80		/* European size A3: 297 x 420 */
#define EURO_A4	0x90		/* European size A4: 210 x 297 */
#define EURO_A5	0xA0		/* European size A5: 148 x 210 */
#define EURO_A6	0xB0		/* European size A6: 105 x 148 */
#define EURO_A7	0xC0		/* European size A7: 74 x 105 */
#define EURO_A8	0xD0		/* European size A8: 52 x 74 */


/* PrinterType */
#define CUSTOM_NAME		0x00
#define	ALPHA_P_101		0x01
#define BROTHER_15XL		0x02
#define CBM_MPS1000		0x03
#define DIAB_630		0x04
#define DIAB_ADV_D25		0x05
#define DIAB_C_150		0x06
#define EPSON			0x07
#define EPSON_JX_80		0x08
#define OKIMATE_20		0x09
#define QUME_LP_20		0x0A
/* new printer entries, 3 October 1985 */
#define HP_LASERJET		0x0B
#define HP_LASERJET_PLUS	0x0C

/* Serial Input Buffer Sizes */
#define SBUF_512	0x00
#define SBUF_1024	0x01
#define SBUF_2048	0x02
#define SBUF_4096	0x03
#define SBUF_8000	0x04
#define SBUF_16000	0x05

/* Serial Bit Masks */
#define	SREAD_BITS	0xF0 /* for SerRWBits	*/
#define	SWRITE_BITS	0x0F

#define	SSTOP_BITS	0xF0 /* for SerStopBuf	*/
#define	SBUFSIZE_BITS	0x0F

#define	SPARITY_BITS	0xF0 /* for SerParShk	*/
#define SHSHAKE_BITS	0x0F

/* Serial Parity (upper nibble, after being shifted by
 * macro SPARNUM() )
 */
#define SPARITY_NONE	 0
#define SPARITY_EVEN	 1
#define SPARITY_ODD	 2
/* New parity definitions for V36: */
#define SPARITY_MARK	 3
#define SPARITY_SPACE	 4

/* Serial Handshake Mode (lower nibble, after masking using
 * macro SHANKNUM() )
 */
#define SHSHAKE_XON	 0
#define SHSHAKE_RTS	 1
#define SHSHAKE_NONE	 2

/* new defines for PrintFlags */

#define CORRECT_RED	    0x0001  /* color correct red shades */
#define CORRECT_GREEN	    0x0002  /* color correct green shades */
#define CORRECT_BLUE	    0x0004  /* color correct blue shades */

#define CENTER_IMAGE	    0x0008  /* center image on paper */

#define IGNORE_DIMENSIONS   0x0000 /* ignore max width/height settings */
#define BOUNDED_DIMENSIONS  0x0010  /* use max width/height as boundaries */
#define ABSOLUTE_DIMENSIONS 0x0020  /* use max width/height as absolutes */
#define PIXEL_DIMENSIONS    0x0040  /* use max width/height as prt pixels */
#define MULTIPLY_DIMENSIONS 0x0080 /* use max width/height as multipliers */

#define INTEGER_SCALING     0x0100  /* force integer scaling */

#define ORDERED_DITHERING   0x0000 /* ordered dithering */
#define HALFTONE_DITHERING  0x0200  /* halftone dithering */
#define FLOYD_DITHERING     0x0400 /* Floyd-Steinberg dithering */

#define ANTI_ALIAS	    0x0800 /* anti-alias image */
#define GREY_SCALE2	    0x1000 /* for use with hi-res monitor */

/* masks used for checking bits */

#define CORRECT_RGB_MASK    (CORRECT_RED|CORRECT_GREEN|CORRECT_BLUE)
#define DIMENSIONS_MASK     (BOUNDED_DIMENSIONS|ABSOLUTE_DIMENSIONS|PIXEL_DIMENSIONS|MULTIPLY_DIMENSIONS)
#define DITHERING_MASK	    (HALFTONE_DITHERING|FLOYD_DITHERING)

#endif
```

## 9.13. intuition/iobsolete.h — V34 compat symbol aliases

// Source: NDK_3.9/Include/include_h/intuition/iobsolete.h
// Auto-included by intuition.h and friends. Defines old names (GADGHCOMP, MOUSEBUTTONS, NEWSIZE, etc.) in terms of the new prefixed names.

```c
#ifndef INTUITION_IOBSOLETE_H
#define INTUITION_IOBSOLETE_H

/*
**  $VER: iobsolete.h 38.1 (22.1.1992)
**  Includes Release 45.1
**
**  Obsolete identifiers for Intuition.  Use the new ones instead!
**
**  (C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/


/* This file contains:
 *
 * 1.  The traditional identifiers for gadget Flags, Activation, and Type,
 * and for window Flags and IDCMP classes.  They are defined in terms
 * of their new versions, which serve to prevent confusion between
 * similar-sounding but different identifiers (like IDCMP_WINDOWACTIVE
 * and WFLG_ACTIVATE).
 *
 * 2.  Some tag names and constants whose labels were adjusted after V36.
 *
 * By default, 1 and 2 are enabled.
 *
 * #define INTUI_V36_NAMES_ONLY to exclude the traditional identifiers and
 * the original V36 names of some identifiers.
 *
 */


#ifndef INTUITION_INTUITION_H
#include <intuition/intuition.h>
#endif

/* #define INTUI_V36_NAMES_ONLY to remove these older names */

#ifndef INTUI_V36_NAMES_ONLY


/* V34-style Gadget->Flags names: */

#define GADGHIGHBITS	GFLG_GADGHIGHBITS
#define GADGHCOMP	GFLG_GADGHCOMP
#define GADGHBOX	GFLG_GADGHBOX
#define GADGHIMAGE	GFLG_GADGHIMAGE
#define GADGHNONE	GFLG_GADGHNONE
#define GADGIMAGE	GFLG_GADGIMAGE
#define GRELBOTTOM	GFLG_RELBOTTOM
#define GRELRIGHT	GFLG_RELRIGHT
#define GRELWIDTH	GFLG_RELWIDTH
#define GRELHEIGHT	GFLG_RELHEIGHT
#define SELECTED	GFLG_SELECTED
#define GADGDISABLED	GFLG_DISABLED
#define LABELMASK	GFLG_LABELMASK
#define LABELITEXT	GFLG_LABELITEXT
#define	LABELSTRING	GFLG_LABELSTRING
#define LABELIMAGE	GFLG_LABELIMAGE


/* V34-style Gadget->Activation flag names: */

#define RELVERIFY	GACT_RELVERIFY
#define GADGIMMEDIATE	GACT_IMMEDIATE
#define ENDGADGET	GACT_ENDGADGET
#define FOLLOWMOUSE	GACT_FOLLOWMOUSE
#define RIGHTBORDER	GACT_RIGHTBORDER
#define LEFTBORDER	GACT_LEFTBORDER
#define TOPBORDER	GACT_TOPBORDER
#define BOTTOMBORDER	GACT_BOTTOMBORDER
#define BORDERSNIFF	GACT_BORDERSNIFF
#define TOGGLESELECT	GACT_TOGGLESELECT
#define BOOLEXTEND	GACT_BOOLEXTEND
#define STRINGLEFT	GACT_STRINGLEFT
#define STRINGCENTER	GACT_STRINGCENTER
#define STRINGRIGHT	GACT_STRINGRIGHT
#define LONGINT		GACT_LONGINT
#define ALTKEYMAP	GACT_ALTKEYMAP
#define STRINGEXTEND	GACT_STRINGEXTEND
#define ACTIVEGADGET	GACT_ACTIVEGADGET


/* V34-style Gadget->Type names: */

#define GADGETTYPE	GTYP_GADGETTYPE
#define SYSGADGET	GTYP_SYSGADGET
#define SCRGADGET	GTYP_SCRGADGET
#define GZZGADGET	GTYP_GZZGADGET
#define REQGADGET	GTYP_REQGADGET
#define SIZING		GTYP_SIZING
#define WDRAGGING	GTYP_WDRAGGING
#define SDRAGGING	GTYP_SDRAGGING
#define WUPFRONT	GTYP_WUPFRONT
#define SUPFRONT	GTYP_SUPFRONT
#define WDOWNBACK	GTYP_WDOWNBACK
#define SDOWNBACK	GTYP_SDOWNBACK
#define CLOSE		GTYP_CLOSE
#define BOOLGADGET	GTYP_BOOLGADGET
#define GADGET0002	GTYP_GADGET0002
#define PROPGADGET	GTYP_PROPGADGET
#define STRGADGET	GTYP_STRGADGET
#define CUSTOMGADGET	GTYP_CUSTOMGADGET
#define GTYPEMASK	GTYP_GTYPEMASK


/* V34-style IDCMP class names: */

#define SIZEVERIFY	IDCMP_SIZEVERIFY
#define NEWSIZE		IDCMP_NEWSIZE
#define REFRESHWINDOW	IDCMP_REFRESHWINDOW
#define MOUSEBUTTONS	IDCMP_MOUSEBUTTONS
#define MOUSEMOVE	IDCMP_MOUSEMOVE
#define GADGETDOWN	IDCMP_GADGETDOWN
#define GADGETUP	IDCMP_GADGETUP
#define REQSET		IDCMP_REQSET
#define MENUPICK	IDCMP_MENUPICK
#define CLOSEWINDOW	IDCMP_CLOSEWINDOW
#define RAWKEY		IDCMP_RAWKEY
#define REQVERIFY	IDCMP_REQVERIFY
#define REQCLEAR	IDCMP_REQCLEAR
#define MENUVERIFY	IDCMP_MENUVERIFY
#define NEWPREFS	IDCMP_NEWPREFS
#define DISKINSERTED	IDCMP_DISKINSERTED
#define DISKREMOVED	IDCMP_DISKREMOVED
#define WBENCHMESSAGE	IDCMP_WBENCHMESSAGE
#define ACTIVEWINDOW	IDCMP_ACTIVEWINDOW
#define INACTIVEWINDOW	IDCMP_INACTIVEWINDOW
#define DELTAMOVE	IDCMP_DELTAMOVE
#define VANILLAKEY	IDCMP_VANILLAKEY
#define INTUITICKS	IDCMP_INTUITICKS
#define IDCMPUPDATE	IDCMP_IDCMPUPDATE
#define MENUHELP	IDCMP_MENUHELP
#define CHANGEWINDOW	IDCMP_CHANGEWINDOW
#define LONELYMESSAGE	IDCMP_LONELYMESSAGE


/* V34-style Window->Flags names: */

#define WINDOWSIZING	WFLG_SIZEGADGET
#define WINDOWDRAG	WFLG_DRAGBAR
#define WINDOWDEPTH	WFLG_DEPTHGADGET
#define WINDOWCLOSE	WFLG_CLOSEGADGET
#define SIZEBRIGHT	WFLG_SIZEBRIGHT
#define SIZEBBOTTOM	WFLG_SIZEBBOTTOM
#define REFRESHBITS	WFLG_REFRESHBITS
#define SMART_REFRESH	WFLG_SMART_REFRESH
#define SIMPLE_REFRESH	WFLG_SIMPLE_REFRESH
#define SUPER_BITMAP	WFLG_SUPER_BITMAP
#define OTHER_REFRESH	WFLG_OTHER_REFRESH
#define BACKDROP	WFLG_BACKDROP
#define REPORTMOUSE	WFLG_REPORTMOUSE
#define GIMMEZEROZERO	WFLG_GIMMEZEROZERO
#define BORDERLESS	WFLG_BORDERLESS
#define ACTIVATE	WFLG_ACTIVATE
#define WINDOWACTIVE	WFLG_WINDOWACTIVE
#define INREQUEST	WFLG_INREQUEST
#define MENUSTATE	WFLG_MENUSTATE
#define RMBTRAP		WFLG_RMBTRAP
#define NOCAREREFRESH	WFLG_NOCAREREFRESH
#define WINDOWREFRESH	WFLG_WINDOWREFRESH
#define WBENCHWINDOW	WFLG_WBENCHWINDOW
#define WINDOWTICKED	WFLG_WINDOWTICKED
#define NW_EXTENDED	WFLG_NW_EXTENDED
#define VISITOR		WFLG_VISITOR
#define ZOOMED		WFLG_ZOOMED
#define HASZOOM		WFLG_HASZOOM


/* These are the obsolete tag names for general gadgets, proportional gadgets,
 * and string gadgets.	Use the mixed-case equivalents from gadgetclass.h
 * instead.
 */

#define GA_LEFT			GA_Left
#define GA_RELRIGHT		GA_RelRight
#define GA_TOP			GA_Top
#define GA_RELBOTTOM		GA_RelBottom
#define GA_WIDTH		GA_Width
#define GA_RELWIDTH		GA_RelWidth
#define GA_HEIGHT		GA_Height
#define GA_RELHEIGHT		GA_RelHeight
#define GA_TEXT			GA_Text
#define GA_IMAGE		GA_Image
#define GA_BORDER		GA_Border
#define GA_SELECTRENDER		GA_SelectRender
#define GA_HIGHLIGHT		GA_Highlight
#define GA_DISABLED		GA_Disabled
#define GA_GZZGADGET		GA_GZZGadget
#define GA_USERDATA		GA_UserData
#define GA_SPECIALINFO		GA_SpecialInfo
#define GA_SELECTED		GA_Selected
#define GA_ENDGADGET		GA_EndGadget
#define GA_IMMEDIATE		GA_Immediate
#define GA_RELVERIFY		GA_RelVerify
#define GA_FOLLOWMOUSE		GA_FollowMouse
#define GA_RIGHTBORDER		GA_RightBorder
#define GA_LEFTBORDER		GA_LeftBorder
#define GA_TOPBORDER		GA_TopBorder
#define GA_BOTTOMBORDER		GA_BottomBorder
#define GA_TOGGLESELECT		GA_ToggleSelect
#define GA_SYSGADGET		GA_SysGadget
#define GA_SYSGTYPE		GA_SysGType
#define GA_PREVIOUS		GA_Previous
#define GA_NEXT			GA_Next
#define GA_DRAWINFO		GA_DrawInfo
#define GA_INTUITEXT		GA_IntuiText
#define GA_LABELIMAGE		GA_LabelImage

#define PGA_FREEDOM		PGA_Freedom
#define PGA_BORDERLESS		PGA_Borderless
#define PGA_HORIZPOT		PGA_HorizPot
#define PGA_HORIZBODY		PGA_HorizBody
#define PGA_VERTPOT		PGA_VertPot
#define PGA_VERTBODY		PGA_VertBody
#define PGA_TOTAL		PGA_Total
#define PGA_VISIBLE		PGA_Visible
#define PGA_TOP			PGA_Top

#define LAYOUTA_LAYOUTOBJ	LAYOUTA_LayoutObj
#define LAYOUTA_SPACING		LAYOUTA_Spacing
#define LAYOUTA_ORIENTATION	LAYOUTA_Orientation


/* These are the obsolete tag names for image attributes.
 * Use the mixed-case equivalents from imageclass.h instead.
 */

#define IMAGE_ATTRIBUTES	(IA_Dummy)
#define IA_LEFT			IA_Left
#define IA_TOP			IA_Top
#define IA_WIDTH		IA_Width
#define IA_HEIGHT		IA_Height
#define IA_FGPEN		IA_FGPen
#define IA_BGPEN		IA_BGPen
#define IA_DATA			IA_Data
#define IA_LINEWIDTH		IA_LineWidth
#define IA_PENS			IA_Pens
#define IA_RESOLUTION		IA_Resolution
#define IA_APATTERN		IA_APattern
#define IA_APATSIZE		IA_APatSize
#define IA_MODE			IA_Mode
#define IA_FONT			IA_Font
#define IA_OUTLINE		IA_Outline
#define IA_RECESSED		IA_Recessed
#define IA_DOUBLEEMBOSS		IA_DoubleEmboss
#define IA_EDGESONLY		IA_EdgesOnly
#define IA_SHADOWPEN		IA_ShadowPen
#define IA_HIGHLIGHTPEN		IA_HighlightPen


/* These are the obsolete identifiers for the various DrawInfo pens.
 * Use the uppercase versions in screens.h instead.
 */

#define detailPen	DETAILPEN
#define blockPen	BLOCKPEN
#define textPen		TEXTPEN
#define shinePen	SHINEPEN
#define shadowPen	SHADOWPEN
#define hifillPen	FILLPEN
#define hifilltextPen	FILLTEXTPEN
#define backgroundPen	BACKGROUNDPEN
#define hilighttextPen	HIGHLIGHTTEXTPEN
#define numDrIPens	NUMDRIPENS


#endif /* !INTUI_V36_NAMES_ONLY */

#endif /* INTUITION_IOBSOLETE_H */
```

# 10. Resources structs

Cross-reference: `amiga-hardware-reference.md`.

## 10.1. resources/cia.h — CIAA/CIAB resource name strings

// Source: NDK_3.9/Include/include_h/resources/cia.h
// cia.resource is how a well-behaved driver allocates CIA ICR (interrupt control register) bits. The FD file is separate (`cia_lib.fd`).

```c
#ifndef DEVICES_CIA_H
#define DEVICES_CIA_H 1
/*
**	$VER: cia.h 36.4 (9.1.1991)
**	Includes Release 45.1
**
**	Cia resource name strings.
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**		All Rights Reserved
*/

#define	CIAANAME "ciaa.resource"
#define	CIABNAME "ciab.resource"

#endif	/* DEVICES_CIA_H */
```

## 10.2. resources/ciabase.h — CiaBase (empty, private)

// Source: NDK_3.9/Include/include_h/resources/ciabase.h
// CiaBase has no public fields.

```c
#ifndef RESOURCES_CIA_H
#define RESOURCES_CIA_H
/*
**	$VER: ciabase.h 1.2 (16.5.1990)
**	Includes Release 45.1
**
**	cia base definitions
**
**	(C) Copyright 1990-2001 Amiga, Inc.
**	    All Rights Reserved
*/


/*
 *	There is no public information in CiaBase
 */


#endif	/* RESOURCES_CIA_H */
```

## 10.3. resources/disk.h — DiscResource, DiscResourceUnit, DRT_* drive types

// Source: NDK_3.9/Include/include_h/resources/disk.h
// disk.resource — arbitrates the disk DMA and interrupt among clients. DSKDMAOFF = $4000 is the idle value for dsklen.

```c
#ifndef	RESOURCES_DISK_H
#define RESOURCES_DISK_H
/*
**	$VER: disk.h 27.11 (21.11.1990)
**	Includes Release 45.1
**
**	disk.h -- external declarations for the disk resource
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef	EXEC_LISTS_H
#include <exec/lists.h>
#endif

#ifndef	EXEC_PORTS_H
#include <exec/ports.h>
#endif

#ifndef	EXEC_INTERRUPTS_H
#include <exec/interrupts.h>
#endif

#ifndef	EXEC_LIBRARIES_H
#include <exec/libraries.h>
#endif


/********************************************************************
*
* Resource structures
*
********************************************************************/


struct DiscResourceUnit {
    struct Message dru_Message;
    struct Interrupt dru_DiscBlock;
    struct Interrupt dru_DiscSync;
    struct Interrupt dru_Index;
};

struct DiscResource {
    struct Library		dr_Library;
    struct DiscResourceUnit	*dr_Current;
    UBYTE			dr_Flags;
    UBYTE			dr_pad;
    struct Library		*dr_SysLib;
    struct Library		*dr_CiaResource;
    ULONG			dr_UnitID[4];
    struct List		dr_Waiting;
    struct Interrupt		dr_DiscBlock;
    struct Interrupt		dr_DiscSync;
    struct Interrupt		dr_Index;
    struct Task			*dr_CurrTask;
};

/* dr_Flags entries */
#define DRB_ALLOC0	0	/* unit zero is allocated */
#define DRB_ALLOC1	1	/* unit one is allocated */
#define DRB_ALLOC2	2	/* unit two is allocated */
#define DRB_ALLOC3	3	/* unit three is allocated */
#define DRB_ACTIVE	7	/* is the disc currently busy? */

#define DRF_ALLOC0	(1<<0)	/* unit zero is allocated */
#define DRF_ALLOC1	(1<<1)	/* unit one is allocated */
#define DRF_ALLOC2	(1<<2)	/* unit two is allocated */
#define DRF_ALLOC3	(1<<3)	/* unit three is allocated */
#define DRF_ACTIVE	(1<<7)	/* is the disc currently busy? */



/********************************************************************
*
* Hardware Magic
*
********************************************************************/


#define	DSKDMAOFF	0x4000	/* idle command for dsklen register */


/********************************************************************
*
* Resource specific commands
*
********************************************************************/

/*
 * DISKNAME is a generic macro to get the name of the resource.
 * This way if the name is ever changed you will pick up the
 *  change automatically.
 */

#define DISKNAME	"disk.resource"


#define	DR_ALLOCUNIT	(LIB_BASE - 0*LIB_VECTSIZE)
#define	DR_FREEUNIT	(LIB_BASE - 1*LIB_VECTSIZE)
#define	DR_GETUNIT	(LIB_BASE - 2*LIB_VECTSIZE)
#define	DR_GIVEUNIT	(LIB_BASE - 3*LIB_VECTSIZE)
#define	DR_GETUNITID	(LIB_BASE - 4*LIB_VECTSIZE)
#define	DR_READUNITID	(LIB_BASE - 5*LIB_VECTSIZE)

#define	DR_LASTCOMM	(DR_READUNITID)

/********************************************************************
*
* drive types
*
********************************************************************/

#define	DRT_AMIGA	(0x00000000)
#define	DRT_37422D2S	(0x55555555)
#define DRT_EMPTY	(0xFFFFFFFF)
#define DRT_150RPM	(0xAAAAAAAA)

#endif /* RESOURCES_DISK_H */
```

## 10.4. resources/misc.h — MR_* unit numbers (serial/parallel hardware)

// Source: NDK_3.9/Include/include_h/resources/misc.h
// misc.resource allocates bit-level ownership of serial-port registers (SERDAT/SERDATR/SERPER/ADKCON), serial control bits, parallel data, parallel control.

```c
#ifndef RESOURCES_MISC_H
#define RESOURCES_MISC_H
/*
**	$VER: misc.h 36.13 (6.5.1990)
**	Includes Release 45.1
**
**	Unit number definitions for "misc.resource"
**
**	(C) Copyright 1985-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_TYPES_H
#include <exec/types.h>
#endif	/* EXEC_TYPES_H */

#ifndef EXEC_LIBRARIES_H
#include <exec/libraries.h>
#endif	/* EXEC_LIBRARIES_H */

/*
 * Unit number definitions.  Ownership of a resource grants low-level
 * bit access to the hardware registers.  You are still obligated to follow
 * the rules for shared access of the interrupt system (see
 * exec.library/SetIntVector or cia.resource as appropriate).
 */
#define	MR_SERIALPORT	0 /* Amiga custom chip serial port registers
			     (SERDAT,SERDATR,SERPER,ADKCON, and interrupts) */
#define	MR_SERIALBITS	1 /* Serial control bits (DTR,CTS, etc.) */
#define	MR_PARALLELPORT	2 /* The 8 bit parallel data port
			     (CIAAPRA & CIAADDRA only!) */
#define	MR_PARALLELBITS	3 /* All other parallel bits & interrupts
			     (BUSY,ACK,etc.) */

/*
 * Library vector offset definitions
 */
#define	MR_ALLOCMISCRESOURCE	(LIB_BASE)		/* -6 */
#define MR_FREEMISCRESOURCE	(LIB_BASE-LIB_VECTSIZE)	/* -12 */

#define MISCNAME "misc.resource"

#endif	/* RESOURCES_MISC_H */
```

## 10.5. resources/potgo.h — POTGONAME

// Source: NDK_3.9/Include/include_h/resources/potgo.h
// potgo.resource — allocates bits in POTGO/POTINP registers (pots/joystick).

```c
#ifndef RESOURCES_POTGO_H
#define RESOURCES_POTGO_H
/*
**	$VER: potgo.h 36.0 (13.4.1990)
**	Includes Release 45.1
**
**	potgo resource name
**
**	(C) Copyright 1986-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#define  POTGONAME	"potgo.resource"

#endif	 /* RESOURCES_POTGO_H */
```

## 10.6. resources/battclock.h

// Source: NDK_3.9/Include/include_h/resources/battclock.h
// battclock.resource — real-time clock chip access.

```c
#ifndef RESOURCES_BATTCLOCK_H
#define RESOURCES_BATTCLOCK_H 1
/*
**	$VER: battclock.h 36.4 (1.5.1990)
**	Includes Release 45.1
**
**	BattClock resource name strings.
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**		All Rights Reserved
*/

#define BATTCLOCKNAME	"battclock.resource"

#endif /* RESOURCES_BATTCLOCK_H */
```

## 10.7. resources/battmem.h

// Source: NDK_3.9/Include/include_h/resources/battmem.h
// battmem.resource — battery-backed NVRAM.

```c
#ifndef RESOURCES_BATTMEM_H
#define RESOURCES_BATTMEM_H 1
/*
**	$VER: battmem.h 36.4 (1.5.1990)
**	Includes Release 45.1
**
**	BattMem resource name strings.
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**		All Rights Reserved
*/

#define BATTMEMNAME	"battmem.resource"

#endif /* RESOURCES_BATTMEM_H */
```

## 10.8. resources/battmembitsamiga.h — Amiga-specific NVRAM bit addresses

// Source: NDK_3.9/Include/include_h/resources/battmembitsamiga.h
// Bits 0-31: amnesia flag, SCSI timeout, SCSI LUN support.

```c
#ifndef RESOURCES_BATTMEMBITSAMIGA_H
#define RESOURCES_BATTMEMBITSAMIGA_H 1
/*
**	$VER: battmembitsamiga.h 39.3 (14.9.1992)
**	Includes Release 45.1
**
**	BattMem Amiga specific bit definitions.
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**		All Rights Reserved
*/


/*
 * Amiga specific bits in the battery-backedup ram.
 *
 *	Bits 0 to 31, inclusive
 */

/*
 * AMIGA_AMNESIA
 *
 *		The battery-backedup memory has had a memory loss.
 *		This bit is used as a flag that the user should be
 *		notified that all battery-backed bit have been
 *		reset and that some attention is required. Zero
 *		indicates that a memory loss has occured.
 */

#define BATTMEM_AMIGA_AMNESIA_ADDR	0
#define BATTMEM_AMIGA_AMNESIA_LEN	1


/*
 * SCSI_TIMEOUT
 *
 *		adjusts the timeout value for SCSI device selection.  A
 *		value of 0 will produce short timeouts (128 ms) while a
 *		value of 1 produces long timeouts (2 sec).  This is used
 *		for Seagate drives (and some Maxtors apparently) that
 *		don`t respond to selection until they are fully spun up
 *		and intialised.
 */

#define BATTMEM_SCSI_TIMEOUT_ADDR	1
#define BATTMEM_SCSI_TIMEOUT_LEN	1


/*
 * SCSI_LUNS
 *
 *		Determines if the controller attempts to access logical
 *		units above 0 at any given SCSI address.  This prevents
 *		problems with drives that respond to ALL LUN addresses
 *		(instead of only 0 like they should).  Default value is
 *		0 meaning don't support LUNs.
 */

#define BATTMEM_SCSI_LUNS_ADDR		2
#define BATTMEM_SCSI_LUNS_LEN		1

#endif /* RESOURCES_BATTMEMBITSAMIGA_H */
```

## 10.9. resources/battmembitsamix.h — AMIX-specific NVRAM bits

// Source: NDK_3.9/Include/include_h/resources/battmembitsamix.h
// Bits 32-63 (reserved for the AMIX Unix port).

```c
#ifndef RESOURCES_BATTMEMBITSAMIX_H
#define RESOURCES_BATTMEMBITSAMIX_H 1
/*
**	$VER: battmembitsamix.h 1.1 (25.5.1990)
**	Includes Release 45.1
**
**	BattMem Amix specific bit definitions.
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**		All Rights Reserved
*/


/*
 *	See Amix documentation for these bit definitions
 *
 *	Bits 32 to 63, inclusive
 */


#endif /* RESOURCES_BATTMEMBITSAMIX_H */
```

## 10.10. resources/battmembitsshared.h — shared NVRAM bit layouts

// Source: NDK_3.9/Include/include_h/resources/battmembitsshared.h
// Bits 64+: SCSI host ID, sync/fast-sync transfer flags, tagged queuing.

```c
#ifndef RESOURCES_BATTMEMBITSSHARED_H
#define RESOURCES_BATTMEMBITSSHARED_H 1
/*
**	$VER: battmembitsshared.h 39.2 (4.6.1993)
**	Includes Release 45.1
**
**	BattMem shared specific bit definitions.
**
**	(C) Copyright 1989-2001 Amiga, Inc.
**		All Rights Reserved
*/


/*
 * Shared bits in the battery-backedup ram.
 *
 *	Bits 64 and above
 */

/*
 * SHARED_AMNESIA
 *
 *		The battery-backedup memory has had a memory loss.
 *		This bit is used as a flag that the user should be
 *		notified that all battery-backed bit have been
 *		reset and that some attention is required. Zero
 *		indicates that a memory loss has occured.
 */

#define BATTMEM_SHARED_AMNESIA_ADDR	64
#define BATTMEM_SHARED_AMNESIA_LEN	1


/*
 * SCSI_HOST_ID
 *
 *		a 3 bit field (0-7) that is stored in complemented form
 *		(this is so that default value of 0 really means 7)
 *		It's used to set the A3000 controllers SCSI ID (on reset)
 */

#define BATTMEM_SCSI_HOST_ID_ADDR	65
#define BATTMEM_SCSI_HOST_ID_LEN	3


/*
 * SCSI_SYNC_XFER
 *
 *		determines if the driver should initiate synchronous
 *		transfer requests or leave it to the drive to send the
 *		first request.	This supports drives that crash or
 *		otherwise get confused when presented with a sync xfer
 *		message.  Default=0=sync xfer not initiated.
 */

#define BATTMEM_SCSI_SYNC_XFER_ADDR	68
#define BATTMEM_SCSI_SYNC_XFER_LEN	1

/*
 * SCSI_FAST_SYNC
 *
 *		determines if the driver should initiate fast synchronous
 *		transfer requests (>5MB/s) instead of older <=5MB/s requests.
 *		Note that this has no effect if synchronous transfers are not
 *		negotiated by either side.
 *		Default=0=fast sync xfer used.
 */

#define BATTMEM_SCSI_FAST_SYNC_ADDR	69
#define BATTMEM_SCSI_FAST_SYNC_LEN	1

/*
 * SCSI_TAG_QUEUES
 *
 *		determines if the driver should use SCSI-2 tagged queuing
 *		which allows the drive to accept and reorder multiple read
 *		and write requests.
 *		Default=0=tagged queuing NOT enabled
 */

#define BATTMEM_SCSI_TAG_QUEUES_ADDR	70
#define BATTMEM_SCSI_TAG_QUEUES_LEN	1

#endif /* RESOURCES_BATTMEMBITSSHARED_H */
```

## 10.11. resources/card.h — CardHandle, DeviceTData, CardMemoryMap, CARD_* flags

// Source: NDK_3.9/Include/include_h/resources/card.h
// card.resource — PCMCIA Type-II credit-card interface. Used by A600/A1200.

```c
#ifndef	RESOURCES_CARD_H
#define RESOURCES_CARD_H 1

/*
**	$VER: card.h 1.11 (14.12.1992)
**	Includes Release 45.1
**
**	card.resource include file
**
**	(C) Copyright 1991-2001 Amiga, Inc.
**	    All Rights Reserved
**
*/
#ifndef	EXEC_TYPES_H
#include <exec/types.h>
#endif

#ifndef	EXEC_NODES_H
#include <exec/nodes.h>
#endif

#ifndef	EXEC_INTERRUPTS_H
#include <exec/interrupts.h>
#endif

#define CARDRESNAME	"card.resource"

/* Structures used by the card.resource				*/

struct	CardHandle {
	struct Node cah_CardNode;
	struct Interrupt *cah_CardRemoved;
	struct Interrupt *cah_CardInserted;
	struct Interrupt *cah_CardStatus;
	UBYTE	cah_CardFlags;
};

struct	DeviceTData {
	ULONG	dtd_DTsize;	/* Size in bytes		*/
	ULONG	dtd_DTspeed;	/* Speed in nanoseconds		*/
	UBYTE	dtd_DTtype;	/* Type of card			*/
	UBYTE	dtd_DTflags;	/* Other flags			*/
};

struct	CardMemoryMap {
	UBYTE	*cmm_CommonMemory;
	UBYTE	*cmm_AttributeMemory;
	UBYTE	*cmm_IOMemory;

/* Extended for V39 - These are the size of the memory spaces above */

	ULONG	cmm_CommonMemSize;
	ULONG	cmm_AttributeMemSize;
	ULONG	cmm_IOMemSize;

};

/* CardHandle.cah_CardFlags for OwnCard() function		*/

#define	CARDB_RESETREMOVE	0
#define CARDF_RESETREMOVE	(1<<CARDB_RESETREMOVE)

#define	CARDB_IFAVAILABLE	1
#define	CARDF_IFAVAILABLE	(1<<CARDB_IFAVAILABLE)

#define CARDB_DELAYOWNERSHIP	2
#define CARDF_DELAYOWNERSHIP	(1<<CARDB_DELAYOWNERSHIP)

#define CARDB_POSTSTATUS	3
#define CARDF_POSTSTATUS	(1<<CARDB_POSTSTATUS)

/* ReleaseCreditCard() function flags				*/

#define	CARDB_REMOVEHANDLE	0
#define	CARDF_REMOVEHANDLE	(1<<CARDB_REMOVEHANDLE)

/* ReadStatus() return flags					*/

#define	CARD_STATUSB_CCDET		6
#define CARD_STATUSF_CCDET		(1<<CARD_STATUSB_CCDET)

#define CARD_STATUSB_BVD1		5
#define	CARD_STATUSF_BVD1		(1<<CARD_STATUSB_BVD1)

#define CARD_STATUSB_SC			5
#define CARD_STATUSF_SC			(1<<CARD_STATUSB_SC)

#define CARD_STATUSB_BVD2		4
#define	CARD_STATUSF_BVD2		(1<<CARD_STATUSB_BVD2)

#define CARD_STATUSB_DA			4
#define CARD_STATUSF_DA			(1<<CARD_STATUSB_DA)

#define CARD_STATUSB_WR			3
#define	CARD_STATUSF_WR			(1<<CARD_STATUSB_WR)

#define CARD_STATUSB_BSY		2
#define CARD_STATUSF_BSY		(1<<CARD_STATUSB_BSY)

#define CARD_STATUSB_IRQ		2
#define CARD_STATUSF_IRQ		(1<<CARD_STATUSB_IRQ)

/* CardProgramVoltage() defines */

#define CARD_VOLTAGE_0V		0	/* Set to default; may be the same as 5V */
#define CARD_VOLTAGE_5V		1
#define CARD_VOLTAGE_12V	2

/* CardMiscControl() defines */

#define	CARD_ENABLEB_DIGAUDIO	1
#define	CARD_ENABLEF_DIGAUDIO	(1<<CARD_ENABLEB_DIGAUDIO)

#define	CARD_DISABLEB_WP	3
#define	CARD_DISABLEF_WP	(1<<CARD_DISABLEB_WP)

/*
 * New CardMiscControl() bits for V39 card.resource.  Use these bits to set,
 * or clear status change interrupts for BVD1/SC, BVD2/DA, and BSY/IRQ.
 * Write-enable/protect change interrupts are always enabled.  The defaults
 * are unchanged (BVD1/SC is enabled, BVD2/DA is disabled, and BSY/IRQ is enabled).
 *
 * IMPORTANT -- Only set these bits for V39 card.resource or greater (check
 * resource base VERSION)
 *
 */

#define	CARD_INTB_SETCLR	7
#define	CARD_INTF_SETCLR	(1<<CARD_INTB_SETCLR)

#define	CARD_INTB_BVD1		5
#define	CARD_INTF_BVD1		(1<<CARD_INTB_BVD1)

#define	CARD_INTB_SC		5
#define	CARD_INTF_SC		(1<<CARD_INTB_SC)

#define	CARD_INTB_BVD2		4
#define	CARD_INTF_BVD2		(1<<CARD_INTB_BVD2)

#define	CARD_INTB_DA		4
#define	CARD_INTF_DA		(1<<CARD_INTB_DA)

#define	CARD_INTB_BSY		2
#define	CARD_INTF_BSY		(1<<CARD_INTB_BSY)

#define	CARD_INTB_IRQ		2
#define	CARD_INTF_IRQ		(1<<CARD_INTB_IRQ)


/* CardInterface() defines */

#define	CARD_INTERFACE_AMIGA_0	0

/*
 * Tuple for Amiga execute-in-place software (e.g., games, or other
 * such software which wants to use execute-in-place software stored
 * on a credit-card, such as a ROM card).
 *
 * See documentatin for IfAmigaXIP().
 */

#define	CISTPL_AMIGAXIP	0x91

struct	TP_AmigaXIP {
	UBYTE	TPL_CODE;
	UBYTE	TPL_LINK;
	UBYTE	TP_XIPLOC[4];
	UBYTE	TP_XIPFLAGS;
	UBYTE	TP_XIPRESRV;
	};
/*

	; The XIPFLAGB_AUTORUN bit means that you want the machine
	; to perform a reset if the execute-in-place card is inserted
	; after DOS has been started.  The machine will then reset,
	; and execute your execute-in-place code the next time around.
	;
	; NOTE -- this flag may be ignored on some machines, in which
	; case the user will have to manually reset the machine in the
	; usual way.

*/

#define	XIPFLAGSB_AUTORUN	0
#define XIPFLAGSF_AUTORUN	(1<<XIPFLAGSB_AUTORUN)

#endif	/* RESOURCES_CARD_H */
```

## 10.12. resources/filesysres.h — FileSysResource, FileSysEntry

// Source: NDK_3.9/Include/include_h/resources/filesysres.h
// FileSystem.resource is the list of available file-system handlers keyed by DosType.

```c
#ifndef	RESOURCES_FILESYSRES_H
#define	RESOURCES_FILESYSRES_H
/*
**	$VER: filesysres.h 36.4 (3.5.1990)
**	Includes Release 45.1
**
**	FileSystem.resource description
**
**	(C) Copyright 1988-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef	EXEC_NODES_H
#include	<exec/nodes.h>
#endif
#ifndef	EXEC_LISTS_H
#include	<exec/lists.h>
#endif
#ifndef	DOS_DOS_H
#include	<dos/dos.h>
#endif

#define	FSRNAME	"FileSystem.resource"

struct FileSysResource {
    struct Node fsr_Node;		/* on resource list */
    char   *fsr_Creator;		/* name of creator of this resource */
    struct List fsr_FileSysEntries;	/* list of FileSysEntry structs */
};

struct FileSysEntry {
    struct Node fse_Node;	/* on fsr_FileSysEntries list */
				/* ln_Name is of creator of this entry */
    ULONG   fse_DosType;	/* DosType of this FileSys */
    ULONG   fse_Version;	/* Version of this FileSys */
    ULONG   fse_PatchFlags;	/* bits set for those of the following that */
				/*   need to be substituted into a standard */
				/*   device node for this file system: e.g. */
				/*   0x180 for substitute SegList & GlobalVec */
    ULONG   fse_Type;		/* device node type: zero */
    CPTR    fse_Task;		/* standard dos "task" field */
    BPTR    fse_Lock;		/* not used for devices: zero */
    BSTR    fse_Handler;	/* filename to loadseg (if SegList is null) */
    ULONG   fse_StackSize;	/* stacksize to use when starting task */
    LONG    fse_Priority;	/* task priority when starting task */
    BPTR    fse_Startup;	/* startup msg: FileSysStartupMsg for disks */
    BPTR    fse_SegList;	/* code to run to start new task */
    BPTR    fse_GlobalVec;	/* BCPL global vector when starting task */
    /* no more entries need exist than those implied by fse_PatchFlags */
};

#endif	/* RESOURCES_FILESYSRES_H */
```

## 10.13. resources/mathresource.h — MathIEEEResource

// Source: NDK_3.9/Include/include_h/resources/mathresource.h
// mathieeesingbas/doubbas/etc use this to register with FPU hardware.

```c
#ifndef	RESOURCES_MATHRESOURCE_H
#define	RESOURCES_MATHRESOURCE_H
/*
**	$VER: mathresource.h 1.2 (13.7.1990)
**	Includes Release 45.1
**
**	Data structure returned by OpenResource of:
**	"MathIEEE.resource"
**
**
**	(C) Copyright 1987-2001 Amiga, Inc.
**	    All Rights Reserved
*/

#ifndef EXEC_NODES_H
#include <exec/nodes.h>
#endif

/*
*	The 'Init' entries are only used if the corresponding
*	bit is set in the Flags field.
*
*	So if you are just a 68881, you do not need the Init stuff
*	just make sure you have cleared the Flags field.
*
*	This should allow us to add Extended Precision later.
*
*	For Init users, if you need to be called whenever a task
*	opens this library for use, you need to change the appropriate
*	entries in MathIEEELibrary.
*/

struct MathIEEEResource
{
	struct	Node	MathIEEEResource_Node;
	unsigned short	MathIEEEResource_Flags;
	unsigned short	*MathIEEEResource_BaseAddr; /* ptr to 881 if exists */
	void	(*MathIEEEResource_DblBasInit)();
	void	(*MathIEEEResource_DblTransInit)();
	void	(*MathIEEEResource_SglBasInit)();
	void	(*MathIEEEResource_SglTransInit)();
	void	(*MathIEEEResource_ExtBasInit)();
	void	(*MathIEEEResource_ExtTransInit)();
};

/* definations for MathIEEEResource_FLAGS */
#define	MATHIEEERESOURCEF_DBLBAS	(1<<0)
#define	MATHIEEERESOURCEF_DBLTRANS	(1<<1)
#define	MATHIEEERESOURCEF_SGLBAS	(1<<2)
#define	MATHIEEERESOURCEF_SGLTRANS	(1<<3)
#define	MATHIEEERESOURCEF_EXTBAS	(1<<4)
#define	MATHIEEERESOURCEF_EXTTRANS	(1<<5)

#endif	/* RESOURCES_MATHRESOURCE_H */
```

# 11. FD files — library LVO tables

Each `.fd` file describes one library's jump table. The `##bias` value is the
**negative offset** (in bytes) of the *first* entry from the library base. Each
entry occupies 6 bytes (one `JMP` instruction), so LVO N is at `-bias - N*6`.

Entries below the `##private` marker are not stable API — an emulator can ignore
them for compatibility purposes but the LVO slots are still reserved.

Register annotations follow the `(args)(regs)` convention. For example,
`Open(name,accessMode)(d1/d2)` means `name` in D1, `accessMode` in D2, result in D0.
Where two register groups are separated by a comma — `BltBitMap(...)(a0,d0/d1/a1,d2/d3/d4/d5/d6/d7/a2)`
— that is purely a grouping for source readability; all listed registers are inputs.

## 11.1 Core libraries

### exec.library

Source: `NDK_3.9/Include/fd/exec_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `Supervisor` | userFunction | `a5` |
| -36 | -$0024 | priv | `execPrivate1` | — | `—` |
| -42 | -$002A | priv | `execPrivate2` | — | `—` |
| -48 | -$0030 | priv | `execPrivate3` | — | `—` |
| -54 | -$0036 | priv | `execPrivate4` | — | `—` |
| -60 | -$003C | priv | `execPrivate5` | — | `—` |
| -66 | -$0042 | priv | `execPrivate6` | — | `—` |
| -72 | -$0048 | pub | `InitCode` | startClass,version | `d0/d1` |
| -78 | -$004E | pub | `InitStruct` | initTable,memory,size | `a1/a2,d0` |
| -84 | -$0054 | pub | `MakeLibrary` | funcInit,structInit,libInit,dataSize,segList | `a0/a1/a2,d0/d1` |
| -90 | -$005A | pub | `MakeFunctions` | target,functionArray,funcDispBase | `a0/a1/a2` |
| -96 | -$0060 | pub | `FindResident` | name | `a1` |
| -102 | -$0066 | pub | `InitResident` | resident,segList | `a1,d1` |
| -108 | -$006C | pub | `Alert` | alertNum | `d7` |
| -114 | -$0072 | pub | `Debug` | flags | `d0` |
| -120 | -$0078 | pub | `Disable` | — | `—` |
| -126 | -$007E | pub | `Enable` | — | `—` |
| -132 | -$0084 | pub | `Forbid` | — | `—` |
| -138 | -$008A | pub | `Permit` | — | `—` |
| -144 | -$0090 | pub | `SetSR` | newSR,mask | `d0/d1` |
| -150 | -$0096 | pub | `SuperState` | — | `—` |
| -156 | -$009C | pub | `UserState` | sysStack | `d0` |
| -162 | -$00A2 | pub | `SetIntVector` | intNumber,interrupt | `d0/a1` |
| -168 | -$00A8 | pub | `AddIntServer` | intNumber,interrupt | `d0/a1` |
| -174 | -$00AE | pub | `RemIntServer` | intNumber,interrupt | `d0/a1` |
| -180 | -$00B4 | pub | `Cause` | interrupt | `a1` |
| -186 | -$00BA | pub | `Allocate` | freeList,byteSize | `a0,d0` |
| -192 | -$00C0 | pub | `Deallocate` | freeList,memoryBlock,byteSize | `a0/a1,d0` |
| -198 | -$00C6 | pub | `AllocMem` | byteSize,requirements | `d0/d1` |
| -204 | -$00CC | pub | `AllocAbs` | byteSize,location | `d0/a1` |
| -210 | -$00D2 | pub | `FreeMem` | memoryBlock,byteSize | `a1,d0` |
| -216 | -$00D8 | pub | `AvailMem` | requirements | `d1` |
| -222 | -$00DE | pub | `AllocEntry` | entry | `a0` |
| -228 | -$00E4 | pub | `FreeEntry` | entry | `a0` |
| -234 | -$00EA | pub | `Insert` | list,node,pred | `a0/a1/a2` |
| -240 | -$00F0 | pub | `AddHead` | list,node | `a0/a1` |
| -246 | -$00F6 | pub | `AddTail` | list,node | `a0/a1` |
| -252 | -$00FC | pub | `Remove` | node | `a1` |
| -258 | -$0102 | pub | `RemHead` | list | `a0` |
| -264 | -$0108 | pub | `RemTail` | list | `a0` |
| -270 | -$010E | pub | `Enqueue` | list,node | `a0/a1` |
| -276 | -$0114 | pub | `FindName` | list,name | `a0/a1` |
| -282 | -$011A | pub | `AddTask` | task,initPC,finalPC | `a1/a2/a3` |
| -288 | -$0120 | pub | `RemTask` | task | `a1` |
| -294 | -$0126 | pub | `FindTask` | name | `a1` |
| -300 | -$012C | pub | `SetTaskPri` | task,priority | `a1,d0` |
| -306 | -$0132 | pub | `SetSignal` | newSignals,signalSet | `d0/d1` |
| -312 | -$0138 | pub | `SetExcept` | newSignals,signalSet | `d0/d1` |
| -318 | -$013E | pub | `Wait` | signalSet | `d0` |
| -324 | -$0144 | pub | `Signal` | task,signalSet | `a1,d0` |
| -330 | -$014A | pub | `AllocSignal` | signalNum | `d0` |
| -336 | -$0150 | pub | `FreeSignal` | signalNum | `d0` |
| -342 | -$0156 | pub | `AllocTrap` | trapNum | `d0` |
| -348 | -$015C | pub | `FreeTrap` | trapNum | `d0` |
| -354 | -$0162 | pub | `AddPort` | port | `a1` |
| -360 | -$0168 | pub | `RemPort` | port | `a1` |
| -366 | -$016E | pub | `PutMsg` | port,message | `a0/a1` |
| -372 | -$0174 | pub | `GetMsg` | port | `a0` |
| -378 | -$017A | pub | `ReplyMsg` | message | `a1` |
| -384 | -$0180 | pub | `WaitPort` | port | `a0` |
| -390 | -$0186 | pub | `FindPort` | name | `a1` |
| -396 | -$018C | pub | `AddLibrary` | library | `a1` |
| -402 | -$0192 | pub | `RemLibrary` | library | `a1` |
| -408 | -$0198 | pub | `OldOpenLibrary` | libName | `a1` |
| -414 | -$019E | pub | `CloseLibrary` | library | `a1` |
| -420 | -$01A4 | pub | `SetFunction` | library,funcOffset,newFunction | `a1,a0,d0` |
| -426 | -$01AA | pub | `SumLibrary` | library | `a1` |
| -432 | -$01B0 | pub | `AddDevice` | device | `a1` |
| -438 | -$01B6 | pub | `RemDevice` | device | `a1` |
| -444 | -$01BC | pub | `OpenDevice` | devName,unit,ioRequest,flags | `a0,d0/a1,d1` |
| -450 | -$01C2 | pub | `CloseDevice` | ioRequest | `a1` |
| -456 | -$01C8 | pub | `DoIO` | ioRequest | `a1` |
| -462 | -$01CE | pub | `SendIO` | ioRequest | `a1` |
| -468 | -$01D4 | pub | `CheckIO` | ioRequest | `a1` |
| -474 | -$01DA | pub | `WaitIO` | ioRequest | `a1` |
| -480 | -$01E0 | pub | `AbortIO` | ioRequest | `a1` |
| -486 | -$01E6 | pub | `AddResource` | resource | `a1` |
| -492 | -$01EC | pub | `RemResource` | resource | `a1` |
| -498 | -$01F2 | pub | `OpenResource` | resName | `a1` |
| -504 | -$01F8 | priv | `execPrivate7` | — | `—` |
| -510 | -$01FE | priv | `execPrivate8` | — | `—` |
| -516 | -$0204 | priv | `execPrivate9` | — | `—` |
| -522 | -$020A | pub | `RawDoFmt` | formatString,dataStream,putChProc,putChData | `a0/a1/a2/a3` |
| -528 | -$0210 | pub | `GetCC` | — | `—` |
| -534 | -$0216 | pub | `TypeOfMem` | address | `a1` |
| -540 | -$021C | pub | `Procure` | sigSem,bidMsg | `a0/a1` |
| -546 | -$0222 | pub | `Vacate` | sigSem,bidMsg | `a0/a1` |
| -552 | -$0228 | pub | `OpenLibrary` | libName,version | `a1,d0` |
| -558 | -$022E | pub | `InitSemaphore` | sigSem | `a0` |
| -564 | -$0234 | pub | `ObtainSemaphore` | sigSem | `a0` |
| -570 | -$023A | pub | `ReleaseSemaphore` | sigSem | `a0` |
| -576 | -$0240 | pub | `AttemptSemaphore` | sigSem | `a0` |
| -582 | -$0246 | pub | `ObtainSemaphoreList` | sigSem | `a0` |
| -588 | -$024C | pub | `ReleaseSemaphoreList` | sigSem | `a0` |
| -594 | -$0252 | pub | `FindSemaphore` | name | `a1` |
| -600 | -$0258 | pub | `AddSemaphore` | sigSem | `a1` |
| -606 | -$025E | pub | `RemSemaphore` | sigSem | `a1` |
| -612 | -$0264 | pub | `SumKickData` | — | `—` |
| -618 | -$026A | pub | `AddMemList` | size,attributes,pri,base,name | `d0/d1/d2/a0/a1` |
| -624 | -$0270 | pub | `CopyMem` | source,dest,size | `a0/a1,d0` |
| -630 | -$0276 | pub | `CopyMemQuick` | source,dest,size | `a0/a1,d0` |
| -636 | -$027C | pub | `CacheClearU` | — | `—` |
| -642 | -$0282 | pub | `CacheClearE` | address,length,caches | `a0,d0/d1` |
| -648 | -$0288 | pub | `CacheControl` | cacheBits,cacheMask | `d0/d1` |
| -654 | -$028E | pub | `CreateIORequest` | port,size | `a0,d0` |
| -660 | -$0294 | pub | `DeleteIORequest` | iorequest | `a0` |
| -666 | -$029A | pub | `CreateMsgPort` | — | `—` |
| -672 | -$02A0 | pub | `DeleteMsgPort` | port | `a0` |
| -678 | -$02A6 | pub | `ObtainSemaphoreShared` | sigSem | `a0` |
| -684 | -$02AC | pub | `AllocVec` | byteSize,requirements | `d0/d1` |
| -690 | -$02B2 | pub | `FreeVec` | memoryBlock | `a1` |
| -696 | -$02B8 | pub | `CreatePool` | requirements,puddleSize,threshSize | `d0/d1/d2` |
| -702 | -$02BE | pub | `DeletePool` | poolHeader | `a0` |
| -708 | -$02C4 | pub | `AllocPooled` | poolHeader,memSize | `a0,d0` |
| -714 | -$02CA | pub | `FreePooled` | poolHeader,memory,memSize | `a0/a1,d0` |
| -720 | -$02D0 | pub | `AttemptSemaphoreShared` | sigSem | `a0` |
| -726 | -$02D6 | pub | `ColdReboot` | — | `—` |
| -732 | -$02DC | pub | `StackSwap` | newStack | `a0` |
| -738 | -$02E2 | priv | `execPrivate10` | — | `—` |
| -744 | -$02E8 | priv | `execPrivate11` | — | `—` |
| -750 | -$02EE | priv | `execPrivate12` | — | `—` |
| -756 | -$02F4 | priv | `execPrivate13` | — | `—` |
| -762 | -$02FA | pub | `CachePreDMA` | address,length,flags | `a0/a1,d0` |
| -768 | -$0300 | pub | `CachePostDMA` | address,length,flags | `a0/a1,d0` |
| -774 | -$0306 | pub | `AddMemHandler` | memhand | `a1` |
| -780 | -$030C | pub | `RemMemHandler` | memhand | `a1` |
| -786 | -$0312 | pub | `ObtainQuickVector` | interruptCode | `a0` |
| -792 | -$0318 | priv | `execPrivate14` | — | `—` |
| -798 | -$031E | priv | `execPrivate15` | — | `—` |
| -804 | -$0324 | priv | `execPrivate16` | — | `—` |
| -810 | -$032A | priv | `execPrivate17` | — | `—` |
| -816 | -$0330 | priv | `execPrivate18` | — | `—` |
| -822 | -$0336 | priv | `execPrivate19` | — | `—` |
| -828 | -$033C | pub | `NewMinList` | minlist | `a0` |
| -834 | -$0342 | priv | `execPrivate20` | — | `—` |
| -840 | -$0348 | priv | `execPrivate21` | — | `—` |
| -846 | -$034E | priv | `execPrivate22` | — | `—` |
| -852 | -$0354 | pub | `AVL_AddNode` | root,node,func | `a0/a1/a2` |
| -858 | -$035A | pub | `AVL_RemNodeByAddress` | root,node | `a0/a1` |
| -864 | -$0360 | pub | `AVL_RemNodeByKey` | root,key,func | `a0/a1/a2` |
| -870 | -$0366 | pub | `AVL_FindNode` | root,key,func | `a0/a1/a2` |
| -876 | -$036C | pub | `AVL_FindPrevNodeByAddress` | node | `a0` |
| -882 | -$0372 | pub | `AVL_FindPrevNodeByKey` | root,key,func | `a0/a1/a2` |
| -888 | -$0378 | pub | `AVL_FindNextNodeByAddress` | node | `a0` |
| -894 | -$037E | pub | `AVL_FindNextNodeByKey` | root,key,func | `a0/a1/a2` |
| -900 | -$0384 | pub | `AVL_FindFirstNode` | root | `a0` |
| -906 | -$038A | pub | `AVL_FindLastNode` | root | `a0` |

### dos.library

Source: `NDK_3.9/Include/fd/dos_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `Open` | name,accessMode | `d1/d2` |
| -36 | -$0024 | pub | `Close` | file | `d1` |
| -42 | -$002A | pub | `Read` | file,buffer,length | `d1/d2/d3` |
| -48 | -$0030 | pub | `Write` | file,buffer,length | `d1/d2/d3` |
| -54 | -$0036 | pub | `Input` | — | `—` |
| -60 | -$003C | pub | `Output` | — | `—` |
| -66 | -$0042 | pub | `Seek` | file,position,offset | `d1/d2/d3` |
| -72 | -$0048 | pub | `DeleteFile` | name | `d1` |
| -78 | -$004E | pub | `Rename` | oldName,newName | `d1/d2` |
| -84 | -$0054 | pub | `Lock` | name,type | `d1/d2` |
| -90 | -$005A | pub | `UnLock` | lock | `d1` |
| -96 | -$0060 | pub | `DupLock` | lock | `d1` |
| -102 | -$0066 | pub | `Examine` | lock,fileInfoBlock | `d1/d2` |
| -108 | -$006C | pub | `ExNext` | lock,fileInfoBlock | `d1/d2` |
| -114 | -$0072 | pub | `Info` | lock,parameterBlock | `d1/d2` |
| -120 | -$0078 | pub | `CreateDir` | name | `d1` |
| -126 | -$007E | pub | `CurrentDir` | lock | `d1` |
| -132 | -$0084 | pub | `IoErr` | — | `—` |
| -138 | -$008A | pub | `CreateProc` | name,pri,segList,stackSize | `d1/d2/d3/d4` |
| -144 | -$0090 | pub | `Exit` | returnCode | `d1` |
| -150 | -$0096 | pub | `LoadSeg` | name | `d1` |
| -156 | -$009C | pub | `UnLoadSeg` | seglist | `d1` |
| -162 | -$00A2 | priv | `dosPrivate1` | — | `—` |
| -168 | -$00A8 | priv | `dosPrivate2` | — | `—` |
| -174 | -$00AE | pub | `DeviceProc` | name | `d1` |
| -180 | -$00B4 | pub | `SetComment` | name,comment | `d1/d2` |
| -186 | -$00BA | pub | `SetProtection` | name,protect | `d1/d2` |
| -192 | -$00C0 | pub | `DateStamp` | date | `d1` |
| -198 | -$00C6 | pub | `Delay` | timeout | `d1` |
| -204 | -$00CC | pub | `WaitForChar` | file,timeout | `d1/d2` |
| -210 | -$00D2 | pub | `ParentDir` | lock | `d1` |
| -216 | -$00D8 | pub | `IsInteractive` | file | `d1` |
| -222 | -$00DE | pub | `Execute` | string,file,file2 | `d1/d2/d3` |
| -228 | -$00E4 | pub | `AllocDosObject` | type,tags | `d1/d2` |
| -234 | -$00EA | pub | `FreeDosObject` | type,ptr | `d1/d2` |
| -240 | -$00F0 | pub | `DoPkt` | port,action,arg1,arg2,arg3,arg4,arg5 | `d1/d2/d3/d4/d5/d6/d7` |
| -246 | -$00F6 | pub | `SendPkt` | dp,port,replyport | `d1/d2/d3` |
| -252 | -$00FC | pub | `WaitPkt` | — | `—` |
| -258 | -$0102 | pub | `ReplyPkt` | dp,res1,res2 | `d1/d2/d3` |
| -264 | -$0108 | pub | `AbortPkt` | port,pkt | `d1/d2` |
| -270 | -$010E | pub | `LockRecord` | fh,offset,length,mode,timeout | `d1/d2/d3/d4/d5` |
| -276 | -$0114 | pub | `LockRecords` | recArray,timeout | `d1/d2` |
| -282 | -$011A | pub | `UnLockRecord` | fh,offset,length | `d1/d2/d3` |
| -288 | -$0120 | pub | `UnLockRecords` | recArray | `d1` |
| -294 | -$0126 | pub | `SelectInput` | fh | `d1` |
| -300 | -$012C | pub | `SelectOutput` | fh | `d1` |
| -306 | -$0132 | pub | `FGetC` | fh | `d1` |
| -312 | -$0138 | pub | `FPutC` | fh,ch | `d1/d2` |
| -318 | -$013E | pub | `UnGetC` | fh,character | `d1/d2` |
| -324 | -$0144 | pub | `FRead` | fh,block,blocklen,number | `d1/d2/d3/d4` |
| -330 | -$014A | pub | `FWrite` | fh,block,blocklen,number | `d1/d2/d3/d4` |
| -336 | -$0150 | pub | `FGets` | fh,buf,buflen | `d1/d2/d3` |
| -342 | -$0156 | pub | `FPuts` | fh,str | `d1/d2` |
| -348 | -$015C | pub | `VFWritef` | fh,format,argarray | `d1/d2/d3` |
| -354 | -$0162 | pub | `VFPrintf` | fh,format,argarray | `d1/d2/d3` |
| -360 | -$0168 | pub | `Flush` | fh | `d1` |
| -366 | -$016E | pub | `SetVBuf` | fh,buff,type,size | `d1/d2/d3/d4` |
| -372 | -$0174 | pub | `DupLockFromFH` | fh | `d1` |
| -378 | -$017A | pub | `OpenFromLock` | lock | `d1` |
| -384 | -$0180 | pub | `ParentOfFH` | fh | `d1` |
| -390 | -$0186 | pub | `ExamineFH` | fh,fib | `d1/d2` |
| -396 | -$018C | pub | `SetFileDate` | name,date | `d1/d2` |
| -402 | -$0192 | pub | `NameFromLock` | lock,buffer,len | `d1/d2/d3` |
| -408 | -$0198 | pub | `NameFromFH` | fh,buffer,len | `d1/d2/d3` |
| -414 | -$019E | pub | `SplitName` | name,separator,buf,oldpos,size | `d1/d2/d3/d4/d5` |
| -420 | -$01A4 | pub | `SameLock` | lock1,lock2 | `d1/d2` |
| -426 | -$01AA | pub | `SetMode` | fh,mode | `d1/d2` |
| -432 | -$01B0 | pub | `ExAll` | lock,buffer,size,data,control | `d1/d2/d3/d4/d5` |
| -438 | -$01B6 | pub | `ReadLink` | port,lock,path,buffer,size | `d1/d2/d3/d4/d5` |
| -444 | -$01BC | pub | `MakeLink` | name,dest,soft | `d1/d2/d3` |
| -450 | -$01C2 | pub | `ChangeMode` | type,fh,newmode | `d1/d2/d3` |
| -456 | -$01C8 | pub | `SetFileSize` | fh,pos,mode | `d1/d2/d3` |
| -462 | -$01CE | pub | `SetIoErr` | result | `d1` |
| -468 | -$01D4 | pub | `Fault` | code,header,buffer,len | `d1/d2/d3/d4` |
| -474 | -$01DA | pub | `PrintFault` | code,header | `d1/d2` |
| -480 | -$01E0 | pub | `ErrorReport` | code,type,arg1,device | `d1/d2/d3/d4` |
| -492 | -$01EC | pub | `Cli` | — | `—` |
| -498 | -$01F2 | pub | `CreateNewProc` | tags | `d1` |
| -504 | -$01F8 | pub | `RunCommand` | seg,stack,paramptr,paramlen | `d1/d2/d3/d4` |
| -510 | -$01FE | pub | `GetConsoleTask` | — | `—` |
| -516 | -$0204 | pub | `SetConsoleTask` | task | `d1` |
| -522 | -$020A | pub | `GetFileSysTask` | — | `—` |
| -528 | -$0210 | pub | `SetFileSysTask` | task | `d1` |
| -534 | -$0216 | pub | `GetArgStr` | — | `—` |
| -540 | -$021C | pub | `SetArgStr` | string | `d1` |
| -546 | -$0222 | pub | `FindCliProc` | num | `d1` |
| -552 | -$0228 | pub | `MaxCli` | — | `—` |
| -558 | -$022E | pub | `SetCurrentDirName` | name | `d1` |
| -564 | -$0234 | pub | `GetCurrentDirName` | buf,len | `d1/d2` |
| -570 | -$023A | pub | `SetProgramName` | name | `d1` |
| -576 | -$0240 | pub | `GetProgramName` | buf,len | `d1/d2` |
| -582 | -$0246 | pub | `SetPrompt` | name | `d1` |
| -588 | -$024C | pub | `GetPrompt` | buf,len | `d1/d2` |
| -594 | -$0252 | pub | `SetProgramDir` | lock | `d1` |
| -600 | -$0258 | pub | `GetProgramDir` | — | `—` |
| -606 | -$025E | pub | `SystemTagList` | command,tags | `d1/d2` |
| -612 | -$0264 | pub | `AssignLock` | name,lock | `d1/d2` |
| -618 | -$026A | pub | `AssignLate` | name,path | `d1/d2` |
| -624 | -$0270 | pub | `AssignPath` | name,path | `d1/d2` |
| -630 | -$0276 | pub | `AssignAdd` | name,lock | `d1/d2` |
| -636 | -$027C | pub | `RemAssignList` | name,lock | `d1/d2` |
| -642 | -$0282 | pub | `GetDeviceProc` | name,dp | `d1/d2` |
| -648 | -$0288 | pub | `FreeDeviceProc` | dp | `d1` |
| -654 | -$028E | pub | `LockDosList` | flags | `d1` |
| -660 | -$0294 | pub | `UnLockDosList` | flags | `d1` |
| -666 | -$029A | pub | `AttemptLockDosList` | flags | `d1` |
| -672 | -$02A0 | pub | `RemDosEntry` | dlist | `d1` |
| -678 | -$02A6 | pub | `AddDosEntry` | dlist | `d1` |
| -684 | -$02AC | pub | `FindDosEntry` | dlist,name,flags | `d1/d2/d3` |
| -690 | -$02B2 | pub | `NextDosEntry` | dlist,flags | `d1/d2` |
| -696 | -$02B8 | pub | `MakeDosEntry` | name,type | `d1/d2` |
| -702 | -$02BE | pub | `FreeDosEntry` | dlist | `d1` |
| -708 | -$02C4 | pub | `IsFileSystem` | name | `d1` |
| -714 | -$02CA | pub | `Format` | filesystem,volumename,dostype | `d1/d2/d3` |
| -720 | -$02D0 | pub | `Relabel` | drive,newname | `d1/d2` |
| -726 | -$02D6 | pub | `Inhibit` | name,onoff | `d1/d2` |
| -732 | -$02DC | pub | `AddBuffers` | name,number | `d1/d2` |
| -738 | -$02E2 | pub | `CompareDates` | date1,date2 | `d1/d2` |
| -744 | -$02E8 | pub | `DateToStr` | datetime | `d1` |
| -750 | -$02EE | pub | `StrToDate` | datetime | `d1` |
| -756 | -$02F4 | pub | `InternalLoadSeg` | fh,table,funcarray,stack | `d0/a0/a1/a2` |
| -762 | -$02FA | pub | `InternalUnLoadSeg` | seglist,freefunc | `d1/a1` |
| -768 | -$0300 | pub | `NewLoadSeg` | file,tags | `d1/d2` |
| -774 | -$0306 | pub | `AddSegment` | name,seg,system | `d1/d2/d3` |
| -780 | -$030C | pub | `FindSegment` | name,seg,system | `d1/d2/d3` |
| -786 | -$0312 | pub | `RemSegment` | seg | `d1` |
| -792 | -$0318 | pub | `CheckSignal` | mask | `d1` |
| -798 | -$031E | pub | `ReadArgs` | arg_template,array,args | `d1/d2/d3` |
| -804 | -$0324 | pub | `FindArg` | keyword,arg_template | `d1/d2` |
| -810 | -$032A | pub | `ReadItem` | name,maxchars,cSource | `d1/d2/d3` |
| -816 | -$0330 | pub | `StrToLong` | string,value | `d1/d2` |
| -822 | -$0336 | pub | `MatchFirst` | pat,anchor | `d1/d2` |
| -828 | -$033C | pub | `MatchNext` | anchor | `d1` |
| -834 | -$0342 | pub | `MatchEnd` | anchor | `d1` |
| -840 | -$0348 | pub | `ParsePattern` | pat,buf,buflen | `d1/d2/d3` |
| -846 | -$034E | pub | `MatchPattern` | pat,str | `d1/d2` |
| -852 | -$0354 | priv | `dosPrivate3` | — | `—` |
| -858 | -$035A | pub | `FreeArgs` | args | `d1` |
| -870 | -$0366 | pub | `FilePart` | path | `d1` |
| -876 | -$036C | pub | `PathPart` | path | `d1` |
| -882 | -$0372 | pub | `AddPart` | dirname,filename,size | `d1/d2/d3` |
| -888 | -$0378 | pub | `StartNotify` | notify | `d1` |
| -894 | -$037E | pub | `EndNotify` | notify | `d1` |
| -900 | -$0384 | pub | `SetVar` | name,buffer,size,flags | `d1/d2/d3/d4` |
| -906 | -$038A | pub | `GetVar` | name,buffer,size,flags | `d1/d2/d3/d4` |
| -912 | -$0390 | pub | `DeleteVar` | name,flags | `d1/d2` |
| -918 | -$0396 | pub | `FindVar` | name,type | `d1/d2` |
| -924 | -$039C | priv | `dosPrivate4` | — | `—` |
| -930 | -$03A2 | pub | `CliInitNewcli` | dp | `a0` |
| -936 | -$03A8 | pub | `CliInitRun` | dp | `a0` |
| -942 | -$03AE | pub | `WriteChars` | buf,buflen | `d1/d2` |
| -948 | -$03B4 | pub | `PutStr` | str | `d1` |
| -954 | -$03BA | pub | `VPrintf` | format,argarray | `d1/d2` |
| -966 | -$03C6 | pub | `ParsePatternNoCase` | pat,buf,buflen | `d1/d2/d3` |
| -972 | -$03CC | pub | `MatchPatternNoCase` | pat,str | `d1/d2` |
| -978 | -$03D2 | priv | `dosPrivate5` | — | `—` |
| -984 | -$03D8 | pub | `SameDevice` | lock1,lock2 | `d1/d2` |
| -990 | -$03DE | pub | `ExAllEnd` | lock,buffer,size,data,control | `d1/d2/d3/d4/d5` |
| -996 | -$03E4 | pub | `SetOwner` | name,owner_info | `d1/d2` |

### intuition.library

Source: `NDK_3.9/Include/fd/intuition_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `OpenIntuition` | — | `—` |
| -36 | -$0024 | pub | `Intuition` | iEvent | `a0` |
| -42 | -$002A | pub | `AddGadget` | window,gadget,position | `a0/a1,d0` |
| -48 | -$0030 | pub | `ClearDMRequest` | window | `a0` |
| -54 | -$0036 | pub | `ClearMenuStrip` | window | `a0` |
| -60 | -$003C | pub | `ClearPointer` | window | `a0` |
| -66 | -$0042 | pub | `CloseScreen` | screen | `a0` |
| -72 | -$0048 | pub | `CloseWindow` | window | `a0` |
| -78 | -$004E | pub | `CloseWorkBench` | — | `—` |
| -84 | -$0054 | pub | `CurrentTime` | seconds,micros | `a0/a1` |
| -90 | -$005A | pub | `DisplayAlert` | alertNumber,string,height | `d0/a0,d1` |
| -96 | -$0060 | pub | `DisplayBeep` | screen | `a0` |
| -102 | -$0066 | pub | `DoubleClick` | sSeconds,sMicros,cSeconds,cMicros | `d0/d1/d2/d3` |
| -108 | -$006C | pub | `DrawBorder` | rp,border,leftOffset,topOffset | `a0/a1,d0/d1` |
| -114 | -$0072 | pub | `DrawImage` | rp,image,leftOffset,topOffset | `a0/a1,d0/d1` |
| -120 | -$0078 | pub | `EndRequest` | requester,window | `a0/a1` |
| -126 | -$007E | pub | `GetDefPrefs` | preferences,size | `a0,d0` |
| -132 | -$0084 | pub | `GetPrefs` | preferences,size | `a0,d0` |
| -138 | -$008A | pub | `InitRequester` | requester | `a0` |
| -144 | -$0090 | pub | `ItemAddress` | menuStrip,menuNumber | `a0,d0` |
| -150 | -$0096 | pub | `ModifyIDCMP` | window,flags | `a0,d0` |
| -156 | -$009C | pub | `ModifyProp` | gadget,window,requester,flags,horizPot,vertPot,horizBody,vertBody | `a0/a1/a2,d0/d1/d2/d3/d4` |
| -162 | -$00A2 | pub | `MoveScreen` | screen,dx,dy | `a0,d0/d1` |
| -168 | -$00A8 | pub | `MoveWindow` | window,dx,dy | `a0,d0/d1` |
| -174 | -$00AE | pub | `OffGadget` | gadget,window,requester | `a0/a1/a2` |
| -180 | -$00B4 | pub | `OffMenu` | window,menuNumber | `a0,d0` |
| -186 | -$00BA | pub | `OnGadget` | gadget,window,requester | `a0/a1/a2` |
| -192 | -$00C0 | pub | `OnMenu` | window,menuNumber | `a0,d0` |
| -198 | -$00C6 | pub | `OpenScreen` | newScreen | `a0` |
| -204 | -$00CC | pub | `OpenWindow` | newWindow | `a0` |
| -210 | -$00D2 | pub | `OpenWorkBench` | — | `—` |
| -216 | -$00D8 | pub | `PrintIText` | rp,iText,left,top | `a0/a1,d0/d1` |
| -222 | -$00DE | pub | `RefreshGadgets` | gadgets,window,requester | `a0/a1/a2` |
| -228 | -$00E4 | pub | `RemoveGadget` | window,gadget | `a0/a1` |
| -234 | -$00EA | pub | `ReportMouse` | flag,window | `d0/a0` |
| -240 | -$00F0 | pub | `Request` | requester,window | `a0/a1` |
| -246 | -$00F6 | pub | `ScreenToBack` | screen | `a0` |
| -252 | -$00FC | pub | `ScreenToFront` | screen | `a0` |
| -258 | -$0102 | pub | `SetDMRequest` | window,requester | `a0/a1` |
| -264 | -$0108 | pub | `SetMenuStrip` | window,menu | `a0/a1` |
| -270 | -$010E | pub | `SetPointer` | window,pointer,height,width,xOffset,yOffset | `a0/a1,d0/d1/d2/d3` |
| -276 | -$0114 | pub | `SetWindowTitles` | window,windowTitle,screenTitle | `a0/a1/a2` |
| -282 | -$011A | pub | `ShowTitle` | screen,showIt | `a0,d0` |
| -288 | -$0120 | pub | `SizeWindow` | window,dx,dy | `a0,d0/d1` |
| -294 | -$0126 | pub | `ViewAddress` | — | `—` |
| -300 | -$012C | pub | `ViewPortAddress` | window | `a0` |
| -306 | -$0132 | pub | `WindowToBack` | window | `a0` |
| -312 | -$0138 | pub | `WindowToFront` | window | `a0` |
| -318 | -$013E | pub | `WindowLimits` | window,widthMin,heightMin,widthMax,heightMax | `a0,d0/d1/d2/d3` |
| -324 | -$0144 | pub | `SetPrefs` | preferences,size,inform | `a0,d0/d1` |
| -330 | -$014A | pub | `IntuiTextLength` | iText | `a0` |
| -336 | -$0150 | pub | `WBenchToBack` | — | `—` |
| -342 | -$0156 | pub | `WBenchToFront` | — | `—` |
| -348 | -$015C | pub | `AutoRequest` | window,body,posText,negText,pFlag,nFlag,width,height | `a0/a1/a2/a3,d0/d1/d2/d3` |
| -354 | -$0162 | pub | `BeginRefresh` | window | `a0` |
| -360 | -$0168 | pub | `BuildSysRequest` | window,body,posText,negText,flags,width,height | `a0/a1/a2/a3,d0/d1/d2` |
| -366 | -$016E | pub | `EndRefresh` | window,complete | `a0,d0` |
| -372 | -$0174 | pub | `FreeSysRequest` | window | `a0` |
| -378 | -$017A | pub | `MakeScreen` | screen | `a0` |
| -384 | -$0180 | pub | `RemakeDisplay` | — | `—` |
| -390 | -$0186 | pub | `RethinkDisplay` | — | `—` |
| -396 | -$018C | pub | `AllocRemember` | rememberKey,size,flags | `a0,d0/d1` |
| -402 | -$0192 | priv | `intuitionPrivate1` | — | `—` |
| -408 | -$0198 | pub | `FreeRemember` | rememberKey,reallyForget | `a0,d0` |
| -414 | -$019E | pub | `LockIBase` | dontknow | `d0` |
| -420 | -$01A4 | pub | `UnlockIBase` | ibLock | `a0` |
| -426 | -$01AA | pub | `GetScreenData` | buffer,size,type,screen | `a0,d0/d1/a1` |
| -432 | -$01B0 | pub | `RefreshGList` | gadgets,window,requester,numGad | `a0/a1/a2,d0` |
| -438 | -$01B6 | pub | `AddGList` | window,gadget,position,numGad,requester | `a0/a1,d0/d1/a2` |
| -444 | -$01BC | pub | `RemoveGList` | remPtr,gadget,numGad | `a0/a1,d0` |
| -450 | -$01C2 | pub | `ActivateWindow` | window | `a0` |
| -456 | -$01C8 | pub | `RefreshWindowFrame` | window | `a0` |
| -462 | -$01CE | pub | `ActivateGadget` | gadgets,window,requester | `a0/a1/a2` |
| -468 | -$01D4 | pub | `NewModifyProp` | gadget,window,requester,flags,horizPot,vertPot,horizBody,vertBody,numGad | `a0/a1/a2,d0/d1/d2/d3/d4/d5` |
| -474 | -$01DA | pub | `QueryOverscan` | displayID,rect,oScanType | `a0/a1,d0` |
| -480 | -$01E0 | pub | `MoveWindowInFrontOf` | window,behindWindow | `a0/a1` |
| -486 | -$01E6 | pub | `ChangeWindowBox` | window,left,top,width,height | `a0,d0/d1/d2/d3` |
| -492 | -$01EC | pub | `SetEditHook` | hook | `a0` |
| -498 | -$01F2 | pub | `SetMouseQueue` | window,queueLength | `a0,d0` |
| -504 | -$01F8 | pub | `ZipWindow` | window | `a0` |
| -510 | -$01FE | pub | `LockPubScreen` | name | `a0` |
| -516 | -$0204 | pub | `UnlockPubScreen` | name,screen | `a0/a1` |
| -522 | -$020A | pub | `LockPubScreenList` | — | `—` |
| -528 | -$0210 | pub | `UnlockPubScreenList` | — | `—` |
| -534 | -$0216 | pub | `NextPubScreen` | screen,namebuf | `a0/a1` |
| -540 | -$021C | pub | `SetDefaultPubScreen` | name | `a0` |
| -546 | -$0222 | pub | `SetPubScreenModes` | modes | `d0` |
| -552 | -$0228 | pub | `PubScreenStatus` | screen,statusFlags | `a0,d0` |
| -558 | -$022E | pub | `ObtainGIRPort` | gInfo | `a0` |
| -564 | -$0234 | pub | `ReleaseGIRPort` | rp | `a0` |
| -570 | -$023A | pub | `GadgetMouse` | gadget,gInfo,mousePoint | `a0/a1/a2` |
| -576 | -$0240 | priv | `intuitionPrivate2` | — | `—` |
| -582 | -$0246 | pub | `GetDefaultPubScreen` | nameBuffer | `a0` |
| -588 | -$024C | pub | `EasyRequestArgs` | window,easyStruct,idcmpPtr,args | `a0/a1/a2/a3` |
| -594 | -$0252 | pub | `BuildEasyRequestArgs` | window,easyStruct,idcmp,args | `a0/a1,d0/a3` |
| -600 | -$0258 | pub | `SysReqHandler` | window,idcmpPtr,waitInput | `a0/a1,d0` |
| -606 | -$025E | pub | `OpenWindowTagList` | newWindow,tagList | `a0/a1` |
| -612 | -$0264 | pub | `OpenScreenTagList` | newScreen,tagList | `a0/a1` |
| -618 | -$026A | pub | `DrawImageState` | rp,image,leftOffset,topOffset,state,drawInfo | `a0/a1,d0/d1/d2/a2` |
| -624 | -$0270 | pub | `PointInImage` | point,image | `d0/a0` |
| -630 | -$0276 | pub | `EraseImage` | rp,image,leftOffset,topOffset | `a0/a1,d0/d1` |
| -636 | -$027C | pub | `NewObjectA` | classPtr,classID,tagList | `a0/a1/a2` |
| -642 | -$0282 | pub | `DisposeObject` | object | `a0` |
| -648 | -$0288 | pub | `SetAttrsA` | object,tagList | `a0/a1` |
| -654 | -$028E | pub | `GetAttr` | attrID,object,storagePtr | `d0/a0/a1` |
| -660 | -$0294 | pub | `SetGadgetAttrsA` | gadget,window,requester,tagList | `a0/a1/a2/a3` |
| -666 | -$029A | pub | `NextObject` | objectPtrPtr | `a0` |
| -672 | -$02A0 | priv | `intuitionPrivate3` | — | `—` |
| -678 | -$02A6 | pub | `MakeClass` | classID,superClassID,superClassPtr,instanceSize,flags | `a0/a1/a2,d0/d1` |
| -684 | -$02AC | pub | `AddClass` | classPtr | `a0` |
| -690 | -$02B2 | pub | `GetScreenDrawInfo` | screen | `a0` |
| -696 | -$02B8 | pub | `FreeScreenDrawInfo` | screen,drawInfo | `a0/a1` |
| -702 | -$02BE | pub | `ResetMenuStrip` | window,menu | `a0/a1` |
| -708 | -$02C4 | pub | `RemoveClass` | classPtr | `a0` |
| -714 | -$02CA | pub | `FreeClass` | classPtr | `a0` |
| -720 | -$02D0 | priv | `intuitionPrivate4` | — | `—` |
| -726 | -$02D6 | priv | `intuitionPrivate5` | — | `—` |
| -768 | -$0300 | pub | `AllocScreenBuffer` | sc,bm,flags | `a0/a1,d0` |
| -774 | -$0306 | pub | `FreeScreenBuffer` | sc,sb | `a0/a1` |
| -780 | -$030C | pub | `ChangeScreenBuffer` | sc,sb | `a0/a1` |
| -786 | -$0312 | pub | `ScreenDepth` | screen,flags,reserved | `a0,d0/a1` |
| -792 | -$0318 | pub | `ScreenPosition` | screen,flags,x1,y1,x2,y2 | `a0,d0/d1/d2/d3/d4` |
| -798 | -$031E | pub | `ScrollWindowRaster` | win,dx,dy,xMin,yMin,xMax,yMax | `a1,d0/d1/d2/d3/d4/d5` |
| -804 | -$0324 | pub | `LendMenus` | fromwindow,towindow | `a0/a1` |
| -810 | -$032A | pub | `DoGadgetMethodA` | gad,win,req,message | `a0/a1/a2/a3` |
| -816 | -$0330 | pub | `SetWindowPointerA` | win,taglist | `a0/a1` |
| -822 | -$0336 | pub | `TimedDisplayAlert` | alertNumber,string,height,time | `d0/a0,d1/a1` |
| -828 | -$033C | pub | `HelpControl` | win,flags | `a0,d0` |

### graphics.library

Source: `NDK_3.9/Include/fd/graphics_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `BltBitMap` | srcBitMap,xSrc,ySrc,destBitMap,xDest,yDest,xSize,ySize,minterm,mask,tempA | `a0,d0/d1/a1,d2/d3/d4/d5/d6/d7/a2` |
| -36 | -$0024 | pub | `BltTemplate` | source,xSrc,srcMod,destRP,xDest,yDest,xSize,ySize | `a0,d0/d1/a1,d2/d3/d4/d5` |
| -42 | -$002A | pub | `ClearEOL` | rp | `a1` |
| -48 | -$0030 | pub | `ClearScreen` | rp | `a1` |
| -54 | -$0036 | pub | `TextLength` | rp,string,count | `a1,a0,d0` |
| -60 | -$003C | pub | `Text` | rp,string,count | `a1,a0,d0` |
| -66 | -$0042 | pub | `SetFont` | rp,textFont | `a1,a0` |
| -72 | -$0048 | pub | `OpenFont` | textAttr | `a0` |
| -78 | -$004E | pub | `CloseFont` | textFont | `a1` |
| -84 | -$0054 | pub | `AskSoftStyle` | rp | `a1` |
| -90 | -$005A | pub | `SetSoftStyle` | rp,style,enable | `a1,d0/d1` |
| -96 | -$0060 | pub | `AddBob` | bob,rp | `a0/a1` |
| -102 | -$0066 | pub | `AddVSprite` | vSprite,rp | `a0/a1` |
| -108 | -$006C | pub | `DoCollision` | rp | `a1` |
| -114 | -$0072 | pub | `DrawGList` | rp,vp | `a1,a0` |
| -120 | -$0078 | pub | `InitGels` | head,tail,gelsInfo | `a0/a1/a2` |
| -126 | -$007E | pub | `InitMasks` | vSprite | `a0` |
| -132 | -$0084 | pub | `RemIBob` | bob,rp,vp | `a0/a1/a2` |
| -138 | -$008A | pub | `RemVSprite` | vSprite | `a0` |
| -144 | -$0090 | pub | `SetCollision` | num,routine,gelsInfo | `d0/a0/a1` |
| -150 | -$0096 | pub | `SortGList` | rp | `a1` |
| -156 | -$009C | pub | `AddAnimOb` | anOb,anKey,rp | `a0/a1/a2` |
| -162 | -$00A2 | pub | `Animate` | anKey,rp | `a0/a1` |
| -168 | -$00A8 | pub | `GetGBuffers` | anOb,rp,flag | `a0/a1,d0` |
| -174 | -$00AE | pub | `InitGMasks` | anOb | `a0` |
| -180 | -$00B4 | pub | `DrawEllipse` | rp,xCenter,yCenter,a,b | `a1,d0/d1/d2/d3` |
| -186 | -$00BA | pub | `AreaEllipse` | rp,xCenter,yCenter,a,b | `a1,d0/d1/d2/d3` |
| -192 | -$00C0 | pub | `LoadRGB4` | vp,colors,count | `a0/a1,d0` |
| -198 | -$00C6 | pub | `InitRastPort` | rp | `a1` |
| -204 | -$00CC | pub | `InitVPort` | vp | `a0` |
| -210 | -$00D2 | pub | `MrgCop` | view | `a1` |
| -216 | -$00D8 | pub | `MakeVPort` | view,vp | `a0/a1` |
| -222 | -$00DE | pub | `LoadView` | view | `a1` |
| -228 | -$00E4 | pub | `WaitBlit` | — | `—` |
| -234 | -$00EA | pub | `SetRast` | rp,pen | `a1,d0` |
| -240 | -$00F0 | pub | `Move` | rp,x,y | `a1,d0/d1` |
| -246 | -$00F6 | pub | `Draw` | rp,x,y | `a1,d0/d1` |
| -252 | -$00FC | pub | `AreaMove` | rp,x,y | `a1,d0/d1` |
| -258 | -$0102 | pub | `AreaDraw` | rp,x,y | `a1,d0/d1` |
| -264 | -$0108 | pub | `AreaEnd` | rp | `a1` |
| -270 | -$010E | pub | `WaitTOF` | — | `—` |
| -276 | -$0114 | pub | `QBlit` | blit | `a1` |
| -282 | -$011A | pub | `InitArea` | areaInfo,vectorBuffer,maxVectors | `a0/a1,d0` |
| -288 | -$0120 | pub | `SetRGB4` | vp,index,red,green,blue | `a0,d0/d1/d2/d3` |
| -294 | -$0126 | pub | `QBSBlit` | blit | `a1` |
| -300 | -$012C | pub | `BltClear` | memBlock,byteCount,flags | `a1,d0/d1` |
| -306 | -$0132 | pub | `RectFill` | rp,xMin,yMin,xMax,yMax | `a1,d0/d1/d2/d3` |
| -312 | -$0138 | pub | `BltPattern` | rp,mask,xMin,yMin,xMax,yMax,maskBPR | `a1,a0,d0/d1/d2/d3/d4` |
| -318 | -$013E | pub | `ReadPixel` | rp,x,y | `a1,d0/d1` |
| -324 | -$0144 | pub | `WritePixel` | rp,x,y | `a1,d0/d1` |
| -330 | -$014A | pub | `Flood` | rp,mode,x,y | `a1,d2,d0/d1` |
| -336 | -$0150 | pub | `PolyDraw` | rp,count,polyTable | `a1,d0/a0` |
| -342 | -$0156 | pub | `SetAPen` | rp,pen | `a1,d0` |
| -348 | -$015C | pub | `SetBPen` | rp,pen | `a1,d0` |
| -354 | -$0162 | pub | `SetDrMd` | rp,drawMode | `a1,d0` |
| -360 | -$0168 | pub | `InitView` | view | `a1` |
| -366 | -$016E | pub | `CBump` | copList | `a1` |
| -372 | -$0174 | pub | `CMove` | copList,destination,data | `a1,d0/d1` |
| -378 | -$017A | pub | `CWait` | copList,v,h | `a1,d0/d1` |
| -384 | -$0180 | pub | `VBeamPos` | — | `—` |
| -390 | -$0186 | pub | `InitBitMap` | bitMap,depth,width,height | `a0,d0/d1/d2` |
| -396 | -$018C | pub | `ScrollRaster` | rp,dx,dy,xMin,yMin,xMax,yMax | `a1,d0/d1/d2/d3/d4/d5` |
| -402 | -$0192 | pub | `WaitBOVP` | vp | `a0` |
| -408 | -$0198 | pub | `GetSprite` | sprite,num | `a0,d0` |
| -414 | -$019E | pub | `FreeSprite` | num | `d0` |
| -420 | -$01A4 | pub | `ChangeSprite` | vp,sprite,newData | `a0/a1/a2` |
| -426 | -$01AA | pub | `MoveSprite` | vp,sprite,x,y | `a0/a1,d0/d1` |
| -432 | -$01B0 | pub | `LockLayerRom` | layer | `a5` |
| -438 | -$01B6 | pub | `UnlockLayerRom` | layer | `a5` |
| -444 | -$01BC | pub | `SyncSBitMap` | layer | `a0` |
| -450 | -$01C2 | pub | `CopySBitMap` | layer | `a0` |
| -456 | -$01C8 | pub | `OwnBlitter` | — | `—` |
| -462 | -$01CE | pub | `DisownBlitter` | — | `—` |
| -468 | -$01D4 | pub | `InitTmpRas` | tmpRas,buffer,size | `a0/a1,d0` |
| -474 | -$01DA | pub | `AskFont` | rp,textAttr | `a1,a0` |
| -480 | -$01E0 | pub | `AddFont` | textFont | `a1` |
| -486 | -$01E6 | pub | `RemFont` | textFont | `a1` |
| -492 | -$01EC | pub | `AllocRaster` | width,height | `d0/d1` |
| -498 | -$01F2 | pub | `FreeRaster` | p,width,height | `a0,d0/d1` |
| -504 | -$01F8 | pub | `AndRectRegion` | region,rectangle | `a0/a1` |
| -510 | -$01FE | pub | `OrRectRegion` | region,rectangle | `a0/a1` |
| -516 | -$0204 | pub | `NewRegion` | — | `—` |
| -522 | -$020A | pub | `ClearRectRegion` | region,rectangle | `a0/a1` |
| -528 | -$0210 | pub | `ClearRegion` | region | `a0` |
| -534 | -$0216 | pub | `DisposeRegion` | region | `a0` |
| -540 | -$021C | pub | `FreeVPortCopLists` | vp | `a0` |
| -546 | -$0222 | pub | `FreeCopList` | copList | `a0` |
| -552 | -$0228 | pub | `ClipBlit` | srcRP,xSrc,ySrc,destRP,xDest,yDest,xSize,ySize,minterm | `a0,d0/d1/a1,d2/d3/d4/d5/d6` |
| -558 | -$022E | pub | `XorRectRegion` | region,rectangle | `a0/a1` |
| -564 | -$0234 | pub | `FreeCprList` | cprList | `a0` |
| -570 | -$023A | pub | `GetColorMap` | entries | `d0` |
| -576 | -$0240 | pub | `FreeColorMap` | colorMap | `a0` |
| -582 | -$0246 | pub | `GetRGB4` | colorMap,entry | `a0,d0` |
| -588 | -$024C | pub | `ScrollVPort` | vp | `a0` |
| -594 | -$0252 | pub | `UCopperListInit` | uCopList,n | `a0,d0` |
| -600 | -$0258 | pub | `FreeGBuffers` | anOb,rp,flag | `a0/a1,d0` |
| -606 | -$025E | pub | `BltBitMapRastPort` | srcBitMap,xSrc,ySrc,destRP,xDest,yDest,xSize,ySize,minterm | `a0,d0/d1/a1,d2/d3/d4/d5/d6` |
| -612 | -$0264 | pub | `OrRegionRegion` | srcRegion,destRegion | `a0/a1` |
| -618 | -$026A | pub | `XorRegionRegion` | srcRegion,destRegion | `a0/a1` |
| -624 | -$0270 | pub | `AndRegionRegion` | srcRegion,destRegion | `a0/a1` |
| -630 | -$0276 | pub | `SetRGB4CM` | colorMap,index,red,green,blue | `a0,d0/d1/d2/d3` |
| -636 | -$027C | pub | `BltMaskBitMapRastPort` | srcBitMap,xSrc,ySrc,destRP,xDest,yDest,xSize,ySize,minterm,bltMask | `a0,d0/d1/a1,d2/d3/d4/d5/d6/a2` |
| -642 | -$0282 | priv | `graphicsPrivate1` | — | `—` |
| -648 | -$0288 | priv | `graphicsPrivate2` | — | `—` |
| -654 | -$028E | pub | `AttemptLockLayerRom` | layer | `a5` |
| -660 | -$0294 | pub | `GfxNew` | gfxNodeType | `d0` |
| -666 | -$029A | pub | `GfxFree` | gfxNodePtr | `a0` |
| -672 | -$02A0 | pub | `GfxAssociate` | associateNode,gfxNodePtr | `a0/a1` |
| -678 | -$02A6 | pub | `BitMapScale` | bitScaleArgs | `a0` |
| -684 | -$02AC | pub | `ScalerDiv` | factor,numerator,denominator | `d0/d1/d2` |
| -690 | -$02B2 | pub | `TextExtent` | rp,string,count,textExtent | `a1,a0,d0/a2` |
| -696 | -$02B8 | pub | `TextFit` | rp,string,strLen,textExtent,constrainingExtent,strDirection,constrainingBitWidth,constrainingBitHeight | `a1,a0,d0/a2/a3,d1/d2/d3` |
| -702 | -$02BE | pub | `GfxLookUp` | associateNode | `a0` |
| -708 | -$02C4 | pub | `VideoControl` | colorMap,tagarray | `a0/a1` |
| -714 | -$02CA | pub | `OpenMonitor` | monitorName,displayID | `a1,d0` |
| -720 | -$02D0 | pub | `CloseMonitor` | monitorSpec | `a0` |
| -726 | -$02D6 | pub | `FindDisplayInfo` | displayID | `d0` |
| -732 | -$02DC | pub | `NextDisplayInfo` | displayID | `d0` |
| -738 | -$02E2 | priv | `graphicsPrivate3` | — | `—` |
| -744 | -$02E8 | priv | `graphicsPrivate4` | — | `—` |
| -750 | -$02EE | priv | `graphicsPrivate5` | — | `—` |
| -756 | -$02F4 | pub | `GetDisplayInfoData` | handle,buf,size,tagID,displayID | `a0/a1,d0/d1/d2` |
| -762 | -$02FA | pub | `FontExtent` | font,fontExtent | `a0/a1` |
| -768 | -$0300 | pub | `ReadPixelLine8` | rp,xstart,ystart,width,array,tempRP | `a0,d0/d1/d2/a2,a1` |
| -774 | -$0306 | pub | `WritePixelLine8` | rp,xstart,ystart,width,array,tempRP | `a0,d0/d1/d2/a2,a1` |
| -780 | -$030C | pub | `ReadPixelArray8` | rp,xstart,ystart,xstop,ystop,array,temprp | `a0,d0/d1/d2/d3/a2,a1` |
| -786 | -$0312 | pub | `WritePixelArray8` | rp,xstart,ystart,xstop,ystop,array,temprp | `a0,d0/d1/d2/d3/a2,a1` |
| -792 | -$0318 | pub | `GetVPModeID` | vp | `a0` |
| -798 | -$031E | pub | `ModeNotAvailable` | modeID | `d0` |
| -804 | -$0324 | priv | `graphicsPrivate6` | — | `—` |
| -810 | -$032A | pub | `EraseRect` | rp,xMin,yMin,xMax,yMax | `a1,d0/d1/d2/d3` |
| -816 | -$0330 | pub | `ExtendFont` | font,fontTags | `a0/a1` |
| -822 | -$0336 | pub | `StripFont` | font | `a0` |
| -828 | -$033C | pub | `CalcIVG` | v,vp | `a0/a1` |
| -834 | -$0342 | pub | `AttachPalExtra` | cm,vp | `a0/a1` |
| -840 | -$0348 | pub | `ObtainBestPenA` | cm,r,g,b,tags | `a0,d1/d2/d3/a1` |
| -846 | -$034E | priv | `graphicsPrivate7` | — | `—` |
| -852 | -$0354 | pub | `SetRGB32` | vp,n,r,g,b | `a0,d0/d1/d2/d3` |
| -858 | -$035A | pub | `GetAPen` | rp | `a0` |
| -864 | -$0360 | pub | `GetBPen` | rp | `a0` |
| -870 | -$0366 | pub | `GetDrMd` | rp | `a0` |
| -876 | -$036C | pub | `GetOutlinePen` | rp | `a0` |
| -882 | -$0372 | pub | `LoadRGB32` | vp,table | `a0/a1` |
| -888 | -$0378 | pub | `SetChipRev` | want | `d0` |
| -894 | -$037E | pub | `SetABPenDrMd` | rp,apen,bpen,drawmode | `a1,d0/d1/d2` |
| -900 | -$0384 | pub | `GetRGB32` | cm,firstcolor,ncolors,table | `a0,d0/d1/a1` |
| -906 | -$038A | priv | `graphicsPrivate8` | — | `—` |
| -912 | -$0390 | priv | `graphicsPrivate9` | — | `—` |
| -918 | -$0396 | pub | `AllocBitMap` | sizex,sizey,depth,flags,friend_bitmap | `d0/d1/d2/d3/a0` |
| -924 | -$039C | pub | `FreeBitMap` | bm | `a0` |
| -930 | -$03A2 | pub | `GetExtSpriteA` | ss,tags | `a2,a1` |
| -936 | -$03A8 | pub | `CoerceMode` | vp,monitorid,flags | `a0,d0/d1` |
| -942 | -$03AE | pub | `ChangeVPBitMap` | vp,bm,db | `a0/a1/a2` |
| -948 | -$03B4 | pub | `ReleasePen` | cm,n | `a0,d0` |
| -954 | -$03BA | pub | `ObtainPen` | cm,n,r,g,b,f | `a0,d0/d1/d2/d3/d4` |
| -960 | -$03C0 | pub | `GetBitMapAttr` | bm,attrnum | `a0,d1` |
| -966 | -$03C6 | pub | `AllocDBufInfo` | vp | `a0` |
| -972 | -$03CC | pub | `FreeDBufInfo` | dbi | `a1` |
| -978 | -$03D2 | pub | `SetOutlinePen` | rp,pen | `a0,d0` |
| -984 | -$03D8 | pub | `SetWriteMask` | rp,msk | `a0,d0` |
| -990 | -$03DE | pub | `SetMaxPen` | rp,maxpen | `a0,d0` |
| -996 | -$03E4 | pub | `SetRGB32CM` | cm,n,r,g,b | `a0,d0/d1/d2/d3` |
| -1002 | -$03EA | pub | `ScrollRasterBF` | rp,dx,dy,xMin,yMin,xMax,yMax | `a1,d0/d1/d2/d3/d4/d5` |
| -1008 | -$03F0 | pub | `FindColor` | cm,r,g,b,maxcolor | `a3,d1/d2/d3/d4` |
| -1014 | -$03F6 | priv | `graphicsPrivate10` | — | `—` |
| -1020 | -$03FC | pub | `AllocSpriteDataA` | bm,tags | `a2,a1` |
| -1026 | -$0402 | pub | `ChangeExtSpriteA` | vp,oldsprite,newsprite,tags | `a0/a1/a2/a3` |
| -1032 | -$0408 | pub | `FreeSpriteData` | sp | `a2` |
| -1038 | -$040E | pub | `SetRPAttrsA` | rp,tags | `a0/a1` |
| -1044 | -$0414 | pub | `GetRPAttrsA` | rp,tags | `a0/a1` |
| -1050 | -$041A | pub | `BestModeIDA` | tags | `a0` |
| -1056 | -$0420 | pub | `WriteChunkyPixels` | rp,xstart,ystart,xstop,ystop,array,bytesperrow | `a0,d0/d1/d2/d3/a2,d4` |

### layers.library

Source: `NDK_3.9/Include/fd/layers_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `InitLayers` | li | `a0` |
| -36 | -$0024 | pub | `CreateUpfrontLayer` | li,bm,x0,y0,x1,y1,flags,bm2 | `a0/a1,d0/d1/d2/d3/d4/a2` |
| -42 | -$002A | pub | `CreateBehindLayer` | li,bm,x0,y0,x1,y1,flags,bm2 | `a0/a1,d0/d1/d2/d3/d4/a2` |
| -48 | -$0030 | pub | `UpfrontLayer` | dummy,layer | `a0/a1` |
| -54 | -$0036 | pub | `BehindLayer` | dummy,layer | `a0/a1` |
| -60 | -$003C | pub | `MoveLayer` | dummy,layer,dx,dy | `a0/a1,d0/d1` |
| -66 | -$0042 | pub | `SizeLayer` | dummy,layer,dx,dy | `a0/a1,d0/d1` |
| -72 | -$0048 | pub | `ScrollLayer` | dummy,layer,dx,dy | `a0/a1,d0/d1` |
| -78 | -$004E | pub | `BeginUpdate` | l | `a0` |
| -84 | -$0054 | pub | `EndUpdate` | layer,flag | `a0,d0` |
| -90 | -$005A | pub | `DeleteLayer` | dummy,layer | `a0/a1` |
| -96 | -$0060 | pub | `LockLayer` | dummy,layer | `a0/a1` |
| -102 | -$0066 | pub | `UnlockLayer` | layer | `a0` |
| -108 | -$006C | pub | `LockLayers` | li | `a0` |
| -114 | -$0072 | pub | `UnlockLayers` | li | `a0` |
| -120 | -$0078 | pub | `LockLayerInfo` | li | `a0` |
| -126 | -$007E | pub | `SwapBitsRastPortClipRect` | rp,cr | `a0/a1` |
| -132 | -$0084 | pub | `WhichLayer` | li,x,y | `a0,d0/d1` |
| -138 | -$008A | pub | `UnlockLayerInfo` | li | `a0` |
| -144 | -$0090 | pub | `NewLayerInfo` | — | `—` |
| -150 | -$0096 | pub | `DisposeLayerInfo` | li | `a0` |
| -156 | -$009C | pub | `FattenLayerInfo` | li | `a0` |
| -162 | -$00A2 | pub | `ThinLayerInfo` | li | `a0` |
| -168 | -$00A8 | pub | `MoveLayerInFrontOf` | layer_to_move,other_layer | `a0/a1` |
| -174 | -$00AE | pub | `InstallClipRegion` | layer,region | `a0/a1` |
| -180 | -$00B4 | pub | `MoveSizeLayer` | layer,dx,dy,dw,dh | `a0,d0/d1/d2/d3` |
| -186 | -$00BA | pub | `CreateUpfrontHookLayer` | li,bm,x0,y0,x1,y1,flags,hook,bm2 | `a0/a1,d0/d1/d2/d3/d4/a3,a2` |
| -192 | -$00C0 | pub | `CreateBehindHookLayer` | li,bm,x0,y0,x1,y1,flags,hook,bm2 | `a0/a1,d0/d1/d2/d3/d4/a3,a2` |
| -198 | -$00C6 | pub | `InstallLayerHook` | layer,hook | `a0/a1` |
| -204 | -$00CC | pub | `InstallLayerInfoHook` | li,hook | `a0/a1` |
| -210 | -$00D2 | pub | `SortLayerCR` | layer,dx,dy | `a0,d0/d1` |
| -216 | -$00D8 | pub | `DoHookClipRects` | hook,rport,rect | `a0/a1/a2` |

### utility.library

Source: `NDK_3.9/Include/fd/utility_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `FindTagItem` | tagVal,tagList | `d0/a0` |
| -36 | -$0024 | pub | `GetTagData` | tagValue,defaultVal,tagList | `d0/d1/a0` |
| -42 | -$002A | pub | `PackBoolTags` | initialFlags,tagList,boolMap | `d0/a0/a1` |
| -48 | -$0030 | pub | `NextTagItem` | tagListPtr | `a0` |
| -54 | -$0036 | pub | `FilterTagChanges` | changeList,originalList,apply | `a0/a1,d0` |
| -60 | -$003C | pub | `MapTags` | tagList,mapList,mapType | `a0/a1,d0` |
| -66 | -$0042 | pub | `AllocateTagItems` | numTags | `d0` |
| -72 | -$0048 | pub | `CloneTagItems` | tagList | `a0` |
| -78 | -$004E | pub | `FreeTagItems` | tagList | `a0` |
| -84 | -$0054 | pub | `RefreshTagItemClones` | clone,original | `a0/a1` |
| -90 | -$005A | pub | `TagInArray` | tagValue,tagArray | `d0/a0` |
| -96 | -$0060 | pub | `FilterTagItems` | tagList,filterArray,logic | `a0/a1,d0` |
| -102 | -$0066 | pub | `CallHookPkt` | hook,object,paramPacket | `a0/a2,a1` |
| -120 | -$0078 | pub | `Amiga2Date` | seconds,result | `d0/a0` |
| -126 | -$007E | pub | `Date2Amiga` | date | `a0` |
| -132 | -$0084 | pub | `CheckDate` | date | `a0` |
| -138 | -$008A | pub | `SMult32` | arg1,arg2 | `d0/d1` |
| -144 | -$0090 | pub | `UMult32` | arg1,arg2 | `d0/d1` |
| -150 | -$0096 | pub | `SDivMod32` | dividend,divisor | `d0/d1` |
| -156 | -$009C | pub | `UDivMod32` | dividend,divisor | `d0/d1` |
| -162 | -$00A2 | pub | `Stricmp` | string1,string2 | `a0/a1` |
| -168 | -$00A8 | pub | `Strnicmp` | string1,string2,length | `a0/a1,d0` |
| -174 | -$00AE | pub | `ToUpper` | character | `d0` |
| -180 | -$00B4 | pub | `ToLower` | character | `d0` |
| -186 | -$00BA | pub | `ApplyTagChanges` | list,changeList | `a0/a1` |
| -198 | -$00C6 | pub | `SMult64` | arg1,arg2 | `d0/d1` |
| -204 | -$00CC | pub | `UMult64` | arg1,arg2 | `d0/d1` |
| -210 | -$00D2 | pub | `PackStructureTags` | pack,packTable,tagList | `a0/a1/a2` |
| -216 | -$00D8 | pub | `UnpackStructureTags` | pack,packTable,tagList | `a0/a1/a2` |
| -222 | -$00DE | pub | `AddNamedObject` | nameSpace,object | `a0/a1` |
| -228 | -$00E4 | pub | `AllocNamedObjectA` | name,tagList | `a0/a1` |
| -234 | -$00EA | pub | `AttemptRemNamedObject` | object | `a0` |
| -240 | -$00F0 | pub | `FindNamedObject` | nameSpace,name,lastObject | `a0/a1/a2` |
| -246 | -$00F6 | pub | `FreeNamedObject` | object | `a0` |
| -252 | -$00FC | pub | `NamedObjectName` | object | `a0` |
| -258 | -$0102 | pub | `ReleaseNamedObject` | object | `a0` |
| -264 | -$0108 | pub | `RemNamedObject` | object,message | `a0/a1` |
| -270 | -$010E | pub | `GetUniqueID` | — | `—` |

### gadtools.library

Source: `NDK_3.9/Include/fd/gadtools_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `CreateGadgetA` | kind,gad,ng,taglist | `d0/a0/a1/a2` |
| -36 | -$0024 | pub | `FreeGadgets` | gad | `a0` |
| -42 | -$002A | pub | `GT_SetGadgetAttrsA` | gad,win,req,taglist | `a0/a1/a2/a3` |
| -48 | -$0030 | pub | `CreateMenusA` | newmenu,taglist | `a0/a1` |
| -54 | -$0036 | pub | `FreeMenus` | menu | `a0` |
| -60 | -$003C | pub | `LayoutMenuItemsA` | firstitem,vi,taglist | `a0/a1/a2` |
| -66 | -$0042 | pub | `LayoutMenusA` | firstmenu,vi,taglist | `a0/a1/a2` |
| -72 | -$0048 | pub | `GT_GetIMsg` | iport | `a0` |
| -78 | -$004E | pub | `GT_ReplyIMsg` | imsg | `a1` |
| -84 | -$0054 | pub | `GT_RefreshWindow` | win,req | `a0/a1` |
| -90 | -$005A | pub | `GT_BeginRefresh` | win | `a0` |
| -96 | -$0060 | pub | `GT_EndRefresh` | win,complete | `a0,d0` |
| -102 | -$0066 | pub | `GT_FilterIMsg` | imsg | `a1` |
| -108 | -$006C | pub | `GT_PostFilterIMsg` | imsg | `a1` |
| -114 | -$0072 | pub | `CreateContext` | glistptr | `a0` |
| -120 | -$0078 | pub | `DrawBevelBoxA` | rport,left,top,width,height,taglist | `a0,d0/d1/d2/d3/a1` |
| -126 | -$007E | pub | `GetVisualInfoA` | screen,taglist | `a0/a1` |
| -132 | -$0084 | pub | `FreeVisualInfo` | vi | `a0` |
| -138 | -$008A | priv | `gadtoolsPrivate1` | — | `—` |
| -144 | -$0090 | priv | `gadtoolsPrivate2` | — | `—` |
| -150 | -$0096 | priv | `gadtoolsPrivate3` | — | `—` |
| -156 | -$009C | priv | `gadtoolsPrivate4` | — | `—` |
| -162 | -$00A2 | priv | `gadtoolsPrivate5` | — | `—` |
| -168 | -$00A8 | priv | `gadtoolsPrivate6` | — | `—` |
| -174 | -$00AE | pub | `GT_GetGadgetAttrsA` | gad,win,req,taglist | `a0/a1/a2/a3` |

### iffparse.library

Source: `NDK_3.9/Include/fd/iffparse_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `AllocIFF` | — | `—` |
| -36 | -$0024 | pub | `OpenIFF` | iff,rwMode | `a0,d0` |
| -42 | -$002A | pub | `ParseIFF` | iff,control | `a0,d0` |
| -48 | -$0030 | pub | `CloseIFF` | iff | `a0` |
| -54 | -$0036 | pub | `FreeIFF` | iff | `a0` |
| -60 | -$003C | pub | `ReadChunkBytes` | iff,buf,numBytes | `a0/a1,d0` |
| -66 | -$0042 | pub | `WriteChunkBytes` | iff,buf,numBytes | `a0/a1,d0` |
| -72 | -$0048 | pub | `ReadChunkRecords` | iff,buf,bytesPerRecord,numRecords | `a0/a1,d0/d1` |
| -78 | -$004E | pub | `WriteChunkRecords` | iff,buf,bytesPerRecord,numRecords | `a0/a1,d0/d1` |
| -84 | -$0054 | pub | `PushChunk` | iff,type,id,size | `a0,d0/d1/d2` |
| -90 | -$005A | pub | `PopChunk` | iff | `a0` |
| -102 | -$0066 | pub | `EntryHandler` | iff,type,id,position,handler,object | `a0,d0/d1/d2/a1/a2` |
| -108 | -$006C | pub | `ExitHandler` | iff,type,id,position,handler,object | `a0,d0/d1/d2/a1/a2` |
| -114 | -$0072 | pub | `PropChunk` | iff,type,id | `a0,d0/d1` |
| -120 | -$0078 | pub | `PropChunks` | iff,propArray,numPairs | `a0/a1,d0` |
| -126 | -$007E | pub | `StopChunk` | iff,type,id | `a0,d0/d1` |
| -132 | -$0084 | pub | `StopChunks` | iff,propArray,numPairs | `a0/a1,d0` |
| -138 | -$008A | pub | `CollectionChunk` | iff,type,id | `a0,d0/d1` |
| -144 | -$0090 | pub | `CollectionChunks` | iff,propArray,numPairs | `a0/a1,d0` |
| -150 | -$0096 | pub | `StopOnExit` | iff,type,id | `a0,d0/d1` |
| -156 | -$009C | pub | `FindProp` | iff,type,id | `a0,d0/d1` |
| -162 | -$00A2 | pub | `FindCollection` | iff,type,id | `a0,d0/d1` |
| -168 | -$00A8 | pub | `FindPropContext` | iff | `a0` |
| -174 | -$00AE | pub | `CurrentChunk` | iff | `a0` |
| -180 | -$00B4 | pub | `ParentChunk` | contextNode | `a0` |
| -186 | -$00BA | pub | `AllocLocalItem` | type,id,ident,dataSize | `d0/d1/d2/d3` |
| -192 | -$00C0 | pub | `LocalItemData` | localItem | `a0` |
| -198 | -$00C6 | pub | `SetLocalItemPurge` | localItem,purgeHook | `a0/a1` |
| -204 | -$00CC | pub | `FreeLocalItem` | localItem | `a0` |
| -210 | -$00D2 | pub | `FindLocalItem` | iff,type,id,ident | `a0,d0/d1/d2` |
| -216 | -$00D8 | pub | `StoreLocalItem` | iff,localItem,position | `a0/a1,d0` |
| -222 | -$00DE | pub | `StoreItemInContext` | iff,localItem,contextNode | `a0/a1/a2` |
| -228 | -$00E4 | pub | `InitIFF` | iff,flags,streamHook | `a0,d0/a1` |
| -234 | -$00EA | pub | `InitIFFasDOS` | iff | `a0` |
| -240 | -$00F0 | pub | `InitIFFasClip` | iff | `a0` |
| -246 | -$00F6 | pub | `OpenClipboard` | unitNumber | `d0` |
| -252 | -$00FC | pub | `CloseClipboard` | clipHandle | `a0` |
| -258 | -$0102 | pub | `GoodID` | id | `d0` |
| -264 | -$0108 | pub | `GoodType` | type | `d0` |
| -270 | -$010E | pub | `IDtoStr` | id,buf | `d0/a0` |

### commodities.library

Source: `NDK_3.9/Include/fd/commodities_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `CreateCxObj` | type,arg1,arg2 | `d0/a0/a1` |
| -36 | -$0024 | pub | `CxBroker` | nb,error | `a0,d0` |
| -42 | -$002A | pub | `ActivateCxObj` | co,doIt | `a0,d0` |
| -48 | -$0030 | pub | `DeleteCxObj` | co | `a0` |
| -54 | -$0036 | pub | `DeleteCxObjAll` | co | `a0` |
| -60 | -$003C | pub | `CxObjType` | co | `a0` |
| -66 | -$0042 | pub | `CxObjError` | co | `a0` |
| -72 | -$0048 | pub | `ClearCxObjError` | co | `a0` |
| -78 | -$004E | pub | `SetCxObjPri` | co,pri | `a0,d0` |
| -84 | -$0054 | pub | `AttachCxObj` | headObj,co | `a0/a1` |
| -90 | -$005A | pub | `EnqueueCxObj` | headObj,co | `a0/a1` |
| -96 | -$0060 | pub | `InsertCxObj` | headObj,co,pred | `a0/a1/a2` |
| -102 | -$0066 | pub | `RemoveCxObj` | co | `a0` |
| -108 | -$006C | priv | `commoditiesPrivate1` | — | `—` |
| -114 | -$0072 | pub | `SetTranslate` | translator,events | `a0/a1` |
| -120 | -$0078 | pub | `SetFilter` | filter,text | `a0/a1` |
| -126 | -$007E | pub | `SetFilterIX` | filter,ix | `a0/a1` |
| -132 | -$0084 | pub | `ParseIX` | description,ix | `a0/a1` |
| -138 | -$008A | pub | `CxMsgType` | cxm | `a0` |
| -144 | -$0090 | pub | `CxMsgData` | cxm | `a0` |
| -150 | -$0096 | pub | `CxMsgID` | cxm | `a0` |
| -156 | -$009C | pub | `DivertCxMsg` | cxm,headObj,returnObj | `a0/a1/a2` |
| -162 | -$00A2 | pub | `RouteCxMsg` | cxm,co | `a0/a1` |
| -168 | -$00A8 | pub | `DisposeCxMsg` | cxm | `a0` |
| -174 | -$00AE | pub | `InvertKeyMap` | ansiCode,event,km | `d0/a0/a1` |
| -180 | -$00B4 | pub | `AddIEvents` | events | `a0` |
| -186 | -$00BA | priv | `commoditiesPrivate2` | — | `—` |
| -192 | -$00C0 | priv | `commoditiesPrivate3` | — | `—` |
| -198 | -$00C6 | priv | `commoditiesPrivate4` | — | `—` |
| -204 | -$00CC | pub | `MatchIX` | event,ix | `a0/a1` |
| -210 | -$00D2 | priv | `commoditiesPrivate5` | — | `—` |
| -216 | -$00D8 | priv | `commoditiesPrivate6` | — | `—` |
| -222 | -$00DE | priv | `commoditiesPrivate7` | — | `—` |
| -228 | -$00E4 | priv | `commoditiesPrivate8` | — | `—` |
| -234 | -$00EA | priv | `commoditiesPrivate9` | — | `—` |

### icon.library

Source: `NDK_3.9/Include/fd/icon_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | priv | `iconPrivate1` | — | `—` |
| -36 | -$0024 | priv | `iconPrivate2` | — | `—` |
| -42 | -$002A | priv | `iconPrivate3` | — | `—` |
| -48 | -$0030 | priv | `iconPrivate4` | — | `—` |
| -54 | -$0036 | pub | `FreeFreeList` | freelist | `a0` |
| -60 | -$003C | priv | `iconPrivate5` | — | `—` |
| -66 | -$0042 | priv | `iconPrivate6` | — | `—` |
| -72 | -$0048 | pub | `AddFreeList` | freelist,mem,size | `a0/a1/a2` |
| -78 | -$004E | pub | `GetDiskObject` | name | `a0` |
| -84 | -$0054 | pub | `PutDiskObject` | name,diskobj | `a0/a1` |
| -90 | -$005A | pub | `FreeDiskObject` | diskobj | `a0` |
| -96 | -$0060 | pub | `FindToolType` | toolTypeArray,typeName | `a0/a1` |
| -102 | -$0066 | pub | `MatchToolValue` | typeString,value | `a0/a1` |
| -108 | -$006C | pub | `BumpRevision` | newname,oldname | `a0/a1` |
| -114 | -$0072 | priv | `iconPrivate7` | — | `—` |
| -120 | -$0078 | pub | `GetDefDiskObject` | type | `d0` |
| -126 | -$007E | pub | `PutDefDiskObject` | diskObject | `a0` |
| -132 | -$0084 | pub | `GetDiskObjectNew` | name | `a0` |
| -138 | -$008A | pub | `DeleteDiskObject` | name | `a0` |
| -144 | -$0090 | priv | `iconPrivate8` | — | `—` |
| -150 | -$0096 | pub | `DupDiskObjectA` | diskObject,tags | `a0/a1` |
| -156 | -$009C | pub | `IconControlA` | icon,tags | `a0/a1` |
| -162 | -$00A2 | pub | `DrawIconStateA` | rp,icon,label,leftOffset,topOffset,state,tags | `a0/a1/a2,d0/d1/d2/a3` |
| -168 | -$00A8 | pub | `GetIconRectangleA` | rp,icon,label,rect,tags | `a0/a1/a2/a3/a4` |
| -174 | -$00AE | pub | `NewDiskObject` | type | `d0` |
| -180 | -$00B4 | pub | `GetIconTagList` | name,tags | `a0/a1` |
| -186 | -$00BA | pub | `PutIconTagList` | name,icon,tags | `a0/a1/a2` |
| -192 | -$00C0 | pub | `LayoutIconA` | icon,screen,tags | `a0/a1/a2` |
| -198 | -$00C6 | pub | `ChangeToSelectedIconColor` | cr | `a0` |

### workbench.library

Source: `NDK_3.9/Include/fd/wb_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | priv | `wbPrivate1` | — | `—` |
| -36 | -$0024 | priv | `wbPrivate2` | — | `—` |
| -42 | -$002A | priv | `wbPrivate3` | — | `—` |
| -48 | -$0030 | pub | `AddAppWindowA` | id,userdata,window,msgport,taglist | `d0/d1/a0/a1/a2` |
| -54 | -$0036 | pub | `RemoveAppWindow` | appWindow | `a0` |
| -60 | -$003C | pub | `AddAppIconA` | id,userdata,text,msgport,lock,diskobj,taglist | `d0/d1/a0/a1/a2/a3/a4` |
| -66 | -$0042 | pub | `RemoveAppIcon` | appIcon | `a0` |
| -72 | -$0048 | pub | `AddAppMenuItemA` | id,userdata,text,msgport,taglist | `d0/d1/a0/a1/a2` |
| -78 | -$004E | pub | `RemoveAppMenuItem` | appMenuItem | `a0` |
| -84 | -$0054 | priv | `wbPrivate4` | — | `—` |
| -90 | -$005A | pub | `WBInfo` | lock,name,screen | `a0/a1/a2` |
| -96 | -$0060 | pub | `OpenWorkbenchObjectA` | name,tags | `a0/a1` |
| -102 | -$0066 | pub | `CloseWorkbenchObjectA` | name,tags | `a0/a1` |
| -108 | -$006C | pub | `WorkbenchControlA` | name,tags | `a0/a1` |
| -114 | -$0072 | pub | `AddAppWindowDropZoneA` | aw,id,userdata,tags | `a0,d0/d1/a1` |
| -120 | -$0078 | pub | `RemoveAppWindowDropZone` | aw,dropZone | `a0/a1` |
| -126 | -$007E | pub | `ChangeWorkbenchSelectionA` | name,hook,tags | `a0/a1/a2` |
| -132 | -$0084 | pub | `MakeWorkbenchObjectVisibleA` | name,tags | `a0/a1` |

### timer.device

Source: `NDK_3.9/Include/fd/timer_lib.fd`

Base bias: 42 (first entry LVO = -42)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -42 | -$002A | pub | `AddTime` | dest,src | `a0/a1` |
| -48 | -$0030 | pub | `SubTime` | dest,src | `a0/a1` |
| -54 | -$0036 | pub | `CmpTime` | dest,src | `a0/a1` |
| -60 | -$003C | pub | `ReadEClock` | dest | `a0` |
| -66 | -$0042 | pub | `GetSysTime` | dest | `a0` |

### console.device

Source: `NDK_3.9/Include/fd/console_lib.fd`

Base bias: 42 (first entry LVO = -42)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -42 | -$002A | pub | `CDInputHandler` | events,consoleDevice | `a0/a1` |
| -48 | -$0030 | pub | `RawKeyConvert` | events,buffer,length,keyMap | `a0/a1,d1/a2` |
| -54 | -$0036 | priv | `consolePrivate1` | — | `—` |
| -60 | -$003C | priv | `consolePrivate2` | — | `—` |
| -66 | -$0042 | priv | `consolePrivate3` | — | `—` |
| -72 | -$0048 | priv | `consolePrivate4` | — | `—` |

### asl.library

Source: `NDK_3.9/Include/fd/asl_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `AllocFileRequest` | — | `—` |
| -36 | -$0024 | pub | `FreeFileRequest` | fileReq | `a0` |
| -42 | -$002A | pub | `RequestFile` | fileReq | `a0` |
| -48 | -$0030 | pub | `AllocAslRequest` | reqType,tagList | `d0/a0` |
| -54 | -$0036 | pub | `FreeAslRequest` | requester | `a0` |
| -60 | -$003C | pub | `AslRequest` | requester,tagList | `a0/a1` |
| -66 | -$0042 | priv | `aslPrivate1` | — | `—` |
| -72 | -$0048 | priv | `aslPrivate2` | — | `—` |
| -78 | -$004E | pub | `AbortAslRequest` | requester | `a0` |
| -84 | -$0054 | pub | `ActivateAslRequest` | requester | `a0` |

### locale.library

Source: `NDK_3.9/Include/fd/locale_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | priv | `localePrivate1` | — | `—` |
| -36 | -$0024 | pub | `CloseCatalog` | catalog | `a0` |
| -42 | -$002A | pub | `CloseLocale` | locale | `a0` |
| -48 | -$0030 | pub | `ConvToLower` | locale,character | `a0,d0` |
| -54 | -$0036 | pub | `ConvToUpper` | locale,character | `a0,d0` |
| -60 | -$003C | pub | `FormatDate` | locale,fmtTemplate,date,putCharFunc | `a0/a1/a2/a3` |
| -66 | -$0042 | pub | `FormatString` | locale,fmtTemplate,dataStream,putCharFunc | `a0/a1/a2/a3` |
| -72 | -$0048 | pub | `GetCatalogStr` | catalog,stringNum,defaultString | `a0,d0/a1` |
| -78 | -$004E | pub | `GetLocaleStr` | locale,stringNum | `a0,d0` |
| -84 | -$0054 | pub | `IsAlNum` | locale,character | `a0,d0` |
| -90 | -$005A | pub | `IsAlpha` | locale,character | `a0,d0` |
| -96 | -$0060 | pub | `IsCntrl` | locale,character | `a0,d0` |
| -102 | -$0066 | pub | `IsDigit` | locale,character | `a0,d0` |
| -108 | -$006C | pub | `IsGraph` | locale,character | `a0,d0` |
| -114 | -$0072 | pub | `IsLower` | locale,character | `a0,d0` |
| -120 | -$0078 | pub | `IsPrint` | locale,character | `a0,d0` |
| -126 | -$007E | pub | `IsPunct` | locale,character | `a0,d0` |
| -132 | -$0084 | pub | `IsSpace` | locale,character | `a0,d0` |
| -138 | -$008A | pub | `IsUpper` | locale,character | `a0,d0` |
| -144 | -$0090 | pub | `IsXDigit` | locale,character | `a0,d0` |
| -150 | -$0096 | pub | `OpenCatalogA` | locale,name,tags | `a0/a1/a2` |
| -156 | -$009C | pub | `OpenLocale` | name | `a0` |
| -162 | -$00A2 | pub | `ParseDate` | locale,date,fmtTemplate,getCharFunc | `a0/a1/a2/a3` |
| -168 | -$00A8 | priv | `localePrivate2` | — | `—` |
| -174 | -$00AE | pub | `StrConvert` | locale,string,buffer,bufferSize,type | `a0/a1/a2,d0/d1` |
| -180 | -$00B4 | pub | `StrnCmp` | locale,string1,string2,length,type | `a0/a1/a2,d0/d1` |
| -186 | -$00BA | priv | `localePrivate3` | — | `—` |
| -192 | -$00C0 | priv | `localePrivate4` | — | `—` |
| -198 | -$00C6 | priv | `localePrivate5` | — | `—` |
| -204 | -$00CC | priv | `localePrivate6` | — | `—` |
| -210 | -$00D2 | priv | `localePrivate7` | — | `—` |
| -216 | -$00D8 | priv | `localePrivate8` | — | `—` |

### lowlevel.library

Source: `NDK_3.9/Include/fd/lowlevel_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `ReadJoyPort` | port | `d0` |
| -36 | -$0024 | pub | `GetLanguageSelection` | — | `—` |
| -42 | -$002A | priv | `lowlevelPrivate1` | — | `—` |
| -48 | -$0030 | pub | `GetKey` | — | `—` |
| -54 | -$0036 | pub | `QueryKeys` | queryArray,arraySize | `a0,d1` |
| -60 | -$003C | pub | `AddKBInt` | intRoutine,intData | `a0/a1` |
| -66 | -$0042 | pub | `RemKBInt` | intHandle | `a1` |
| -72 | -$0048 | pub | `SystemControlA` | tagList | `a1` |
| -78 | -$004E | pub | `AddTimerInt` | intRoutine,intData | `a0/a1` |
| -84 | -$0054 | pub | `RemTimerInt` | intHandle | `a1` |
| -90 | -$005A | pub | `StopTimerInt` | intHandle | `a1` |
| -96 | -$0060 | pub | `StartTimerInt` | intHandle,timeInterval,continuous | `a1,d0/d1` |
| -102 | -$0066 | pub | `ElapsedTime` | context | `a0` |
| -108 | -$006C | pub | `AddVBlankInt` | intRoutine,intData | `a0/a1` |
| -114 | -$0072 | pub | `RemVBlankInt` | intHandle | `a1` |
| -120 | -$0078 | priv | `lowlevelPrivate2` | — | `—` |
| -126 | -$007E | priv | `lowlevelPrivate3` | — | `—` |
| -132 | -$0084 | pub | `SetJoyPortAttrsA` | portNumber,tagList | `d0/a1` |
| -138 | -$008A | priv | `lowlevelPrivate4` | — | `—` |
| -144 | -$0090 | priv | `lowlevelPrivate5` | — | `—` |
| -150 | -$0096 | priv | `lowlevelPrivate6` | — | `—` |
| -156 | -$009C | priv | `lowlevelPrivate7` | — | `—` |
| -162 | -$00A2 | priv | `lowlevelPrivate8` | — | `—` |

### diskfont.library

Source: `NDK_3.9/Include/fd/diskfont_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `OpenDiskFont` | textAttr | `a0` |
| -36 | -$0024 | pub | `AvailFonts` | buffer,bufBytes,flags | `a0,d0/d1` |
| -42 | -$002A | pub | `NewFontContents` | fontsLock,fontName | `a0/a1` |
| -48 | -$0030 | pub | `DisposeFontContents` | fontContentsHeader | `a1` |
| -54 | -$0036 | pub | `NewScaledDiskFont` | sourceFont,destTextAttr | `a0/a1` |
| -60 | -$003C | pub | `GetDiskFontCtrl` | tagid | `d0` |
| -66 | -$0042 | pub | `SetDiskFontCtrlA` | taglist | `a0` |

### keymap.library

Source: `NDK_3.9/Include/fd/keymap_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `SetKeyMapDefault` | keyMap | `a0` |
| -36 | -$0024 | pub | `AskKeyMapDefault` | — | `—` |
| -42 | -$002A | pub | `MapRawKey` | event,buffer,length,keyMap | `a0/a1,d1/a2` |
| -48 | -$0030 | pub | `MapANSI` | string,count,buffer,length,keyMap | `a0,d0/a1,d1/a2` |

### mathffp.library

Source: `NDK_3.9/Include/fd/mathffp_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `SPFix` | parm | `d0` |
| -36 | -$0024 | pub | `SPFlt` | integer | `d0` |
| -42 | -$002A | pub | `SPCmp` | leftParm,rightParm | `d1,d0` |
| -48 | -$0030 | pub | `SPTst` | parm | `d1` |
| -54 | -$0036 | pub | `SPAbs` | parm | `d0` |
| -60 | -$003C | pub | `SPNeg` | parm | `d0` |
| -66 | -$0042 | pub | `SPAdd` | leftParm,rightParm | `d1,d0` |
| -72 | -$0048 | pub | `SPSub` | leftParm,rightParm | `d1,d0` |
| -78 | -$004E | pub | `SPMul` | leftParm,rightParm | `d1,d0` |
| -84 | -$0054 | pub | `SPDiv` | leftParm,rightParm | `d1,d0` |
| -90 | -$005A | pub | `SPFloor` | parm | `d0` |
| -96 | -$0060 | pub | `SPCeil` | parm | `d0` |

### mathtrans.library

Source: `NDK_3.9/Include/fd/mathtrans_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `SPAtan` | parm | `d0` |
| -36 | -$0024 | pub | `SPSin` | parm | `d0` |
| -42 | -$002A | pub | `SPCos` | parm | `d0` |
| -48 | -$0030 | pub | `SPTan` | parm | `d0` |
| -54 | -$0036 | pub | `SPSincos` | cosResult,parm | `d1,d0` |
| -60 | -$003C | pub | `SPSinh` | parm | `d0` |
| -66 | -$0042 | pub | `SPCosh` | parm | `d0` |
| -72 | -$0048 | pub | `SPTanh` | parm | `d0` |
| -78 | -$004E | pub | `SPExp` | parm | `d0` |
| -84 | -$0054 | pub | `SPLog` | parm | `d0` |
| -90 | -$005A | pub | `SPPow` | power,arg | `d1,d0` |
| -96 | -$0060 | pub | `SPSqrt` | parm | `d0` |
| -102 | -$0066 | pub | `SPTieee` | parm | `d0` |
| -108 | -$006C | pub | `SPFieee` | parm | `d0` |
| -114 | -$0072 | pub | `SPAsin` | parm | `d0` |
| -120 | -$0078 | pub | `SPAcos` | parm | `d0` |
| -126 | -$007E | pub | `SPLog10` | parm | `d0` |

### mathieeesingbas.library

Source: `NDK_3.9/Include/fd/mathieeesingbas_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `IEEESPFix` | parm | `d0` |
| -36 | -$0024 | pub | `IEEESPFlt` | integer | `d0` |
| -42 | -$002A | pub | `IEEESPCmp` | leftParm,rightParm | `d0/d1` |
| -48 | -$0030 | pub | `IEEESPTst` | parm | `d0` |
| -54 | -$0036 | pub | `IEEESPAbs` | parm | `d0` |
| -60 | -$003C | pub | `IEEESPNeg` | parm | `d0` |
| -66 | -$0042 | pub | `IEEESPAdd` | leftParm,rightParm | `d0/d1` |
| -72 | -$0048 | pub | `IEEESPSub` | leftParm,rightParm | `d0/d1` |
| -78 | -$004E | pub | `IEEESPMul` | leftParm,rightParm | `d0/d1` |
| -84 | -$0054 | pub | `IEEESPDiv` | dividend,divisor | `d0/d1` |
| -90 | -$005A | pub | `IEEESPFloor` | parm | `d0` |
| -96 | -$0060 | pub | `IEEESPCeil` | parm | `d0` |

### mathieeesingtrans.library

Source: `NDK_3.9/Include/fd/mathieeesingtrans_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `IEEESPAtan` | parm | `d0` |
| -36 | -$0024 | pub | `IEEESPSin` | parm | `d0` |
| -42 | -$002A | pub | `IEEESPCos` | parm | `d0` |
| -48 | -$0030 | pub | `IEEESPTan` | parm | `d0` |
| -54 | -$0036 | pub | `IEEESPSincos` | cosptr,parm | `a0,d0` |
| -60 | -$003C | pub | `IEEESPSinh` | parm | `d0` |
| -66 | -$0042 | pub | `IEEESPCosh` | parm | `d0` |
| -72 | -$0048 | pub | `IEEESPTanh` | parm | `d0` |
| -78 | -$004E | pub | `IEEESPExp` | parm | `d0` |
| -84 | -$0054 | pub | `IEEESPLog` | parm | `d0` |
| -90 | -$005A | pub | `IEEESPPow` | exp,arg | `d1,d0` |
| -96 | -$0060 | pub | `IEEESPSqrt` | parm | `d0` |
| -102 | -$0066 | pub | `IEEESPTieee` | parm | `d0` |
| -108 | -$006C | pub | `IEEESPFieee` | parm | `d0` |
| -114 | -$0072 | pub | `IEEESPAsin` | parm | `d0` |
| -120 | -$0078 | pub | `IEEESPAcos` | parm | `d0` |
| -126 | -$007E | pub | `IEEESPLog10` | parm | `d0` |

### mathieeedoubbas.library

Source: `NDK_3.9/Include/fd/mathieeedoubbas_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `IEEEDPFix` | parm | `d0/d1` |
| -36 | -$0024 | pub | `IEEEDPFlt` | integer | `d0` |
| -42 | -$002A | pub | `IEEEDPCmp` | leftParm,rightParm | `d0/d1/d2/d3` |
| -48 | -$0030 | pub | `IEEEDPTst` | parm | `d0/d1` |
| -54 | -$0036 | pub | `IEEEDPAbs` | parm | `d0/d1` |
| -60 | -$003C | pub | `IEEEDPNeg` | parm | `d0/d1` |
| -66 | -$0042 | pub | `IEEEDPAdd` | leftParm,rightParm | `d0/d1/d2/d3` |
| -72 | -$0048 | pub | `IEEEDPSub` | leftParm,rightParm | `d0/d1/d2/d3` |
| -78 | -$004E | pub | `IEEEDPMul` | factor1,factor2 | `d0/d1/d2/d3` |
| -84 | -$0054 | pub | `IEEEDPDiv` | dividend,divisor | `d0/d1/d2/d3` |
| -90 | -$005A | pub | `IEEEDPFloor` | parm | `d0/d1` |
| -96 | -$0060 | pub | `IEEEDPCeil` | parm | `d0/d1` |

### mathieeedoubtrans.library

Source: `NDK_3.9/Include/fd/mathieeedoubtrans_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `IEEEDPAtan` | parm | `d0/d1` |
| -36 | -$0024 | pub | `IEEEDPSin` | parm | `d0/d1` |
| -42 | -$002A | pub | `IEEEDPCos` | parm | `d0/d1` |
| -48 | -$0030 | pub | `IEEEDPTan` | parm | `d0/d1` |
| -54 | -$0036 | pub | `IEEEDPSincos` | pf2,parm | `a0,d0/d1` |
| -60 | -$003C | pub | `IEEEDPSinh` | parm | `d0/d1` |
| -66 | -$0042 | pub | `IEEEDPCosh` | parm | `d0/d1` |
| -72 | -$0048 | pub | `IEEEDPTanh` | parm | `d0/d1` |
| -78 | -$004E | pub | `IEEEDPExp` | parm | `d0/d1` |
| -84 | -$0054 | pub | `IEEEDPLog` | parm | `d0/d1` |
| -90 | -$005A | pub | `IEEEDPPow` | exp,arg | `d2/d3,d0/d1` |
| -96 | -$0060 | pub | `IEEEDPSqrt` | parm | `d0/d1` |
| -102 | -$0066 | pub | `IEEEDPTieee` | parm | `d0/d1` |
| -108 | -$006C | pub | `IEEEDPFieee` | single | `d0` |
| -114 | -$0072 | pub | `IEEEDPAsin` | parm | `d0/d1` |
| -120 | -$0078 | pub | `IEEEDPAcos` | parm | `d0/d1` |
| -126 | -$007E | pub | `IEEEDPLog10` | parm | `d0/d1` |

### expansion.library

Source: `NDK_3.9/Include/fd/expansion_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `AddConfigDev` | configDev | `a0` |
| -36 | -$0024 | pub | `AddBootNode` | bootPri,flags,deviceNode,configDev | `d0/d1/a0/a1` |
| -42 | -$002A | pub | `AllocBoardMem` | slotSpec | `d0` |
| -48 | -$0030 | pub | `AllocConfigDev` | — | `—` |
| -54 | -$0036 | pub | `AllocExpansionMem` | numSlots,slotAlign | `d0/d1` |
| -60 | -$003C | pub | `ConfigBoard` | board,configDev | `a0/a1` |
| -66 | -$0042 | pub | `ConfigChain` | baseAddr | `a0` |
| -72 | -$0048 | pub | `FindConfigDev` | oldConfigDev,manufacturer,product | `a0,d0/d1` |
| -78 | -$004E | pub | `FreeBoardMem` | startSlot,slotSpec | `d0/d1` |
| -84 | -$0054 | pub | `FreeConfigDev` | configDev | `a0` |
| -90 | -$005A | pub | `FreeExpansionMem` | startSlot,numSlots | `d0/d1` |
| -96 | -$0060 | pub | `ReadExpansionByte` | board,offset | `a0,d0` |
| -102 | -$0066 | pub | `ReadExpansionRom` | board,configDev | `a0/a1` |
| -108 | -$006C | pub | `RemConfigDev` | configDev | `a0` |
| -114 | -$0072 | pub | `WriteExpansionByte` | board,offset,byte | `a0,d0/d1` |
| -120 | -$0078 | pub | `ObtainConfigBinding` | — | `—` |
| -126 | -$007E | pub | `ReleaseConfigBinding` | — | `—` |
| -132 | -$0084 | pub | `SetCurrentBinding` | currentBinding,bindingSize | `a0,d0` |
| -138 | -$008A | pub | `GetCurrentBinding` | currentBinding,bindingSize | `a0,d0` |
| -144 | -$0090 | pub | `MakeDosNode` | parmPacket | `a0` |
| -150 | -$0096 | pub | `AddDosNode` | bootPri,flags,deviceNode | `d0/d1/a0` |
| -156 | -$009C | priv | `expansionPrivate1` | — | `—` |
| -162 | -$00A2 | priv | `expansionPrivate2` | — | `—` |

### datatypes.library

Source: `NDK_3.9/Include/fd/datatypes_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | priv | `datatypesPrivate1` | — | `—` |
| -36 | -$0024 | pub | `ObtainDataTypeA` | type,handle,attrs | `d0/a0/a1` |
| -42 | -$002A | pub | `ReleaseDataType` | dt | `a0` |
| -48 | -$0030 | pub | `NewDTObjectA` | name,attrs | `d0/a0` |
| -54 | -$0036 | pub | `DisposeDTObject` | o | `a0` |
| -60 | -$003C | pub | `SetDTAttrsA` | o,win,req,attrs | `a0/a1/a2/a3` |
| -66 | -$0042 | pub | `GetDTAttrsA` | o,attrs | `a0/a2` |
| -72 | -$0048 | pub | `AddDTObject` | win,req,o,pos | `a0/a1/a2,d0` |
| -78 | -$004E | pub | `RefreshDTObjectA` | o,win,req,attrs | `a0/a1/a2/a3` |
| -84 | -$0054 | pub | `DoAsyncLayout` | o,gpl | `a0/a1` |
| -90 | -$005A | pub | `DoDTMethodA` | o,win,req,msg | `a0/a1/a2/a3` |
| -96 | -$0060 | pub | `RemoveDTObject` | win,o | `a0/a1` |
| -102 | -$0066 | pub | `GetDTMethods` | object | `a0` |
| -108 | -$006C | pub | `GetDTTriggerMethods` | object | `a0` |
| -114 | -$0072 | pub | `PrintDTObjectA` | o,w,r,msg | `a0/a1/a2/a3` |
| -120 | -$0078 | pub | `ObtainDTDrawInfoA` | o,attrs | `a0/a1` |
| -126 | -$007E | pub | `DrawDTObjectA` | rp,o,x,y,w,h,th,tv,attrs | `a0/a1,d0/d1/d2/d3/d4/d5/a2` |
| -132 | -$0084 | pub | `ReleaseDTDrawInfo` | o,handle | `a0/a1` |
| -138 | -$008A | pub | `GetDTString` | id | `d0` |

### realtime.library

Source: `NDK_3.9/Include/fd/realtime_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `LockRealTime` | lockType | `d0` |
| -36 | -$0024 | pub | `UnlockRealTime` | lock | `a0` |
| -42 | -$002A | pub | `CreatePlayerA` | tagList | `a0` |
| -48 | -$0030 | pub | `DeletePlayer` | player | `a0` |
| -54 | -$0036 | pub | `SetPlayerAttrsA` | player,tagList | `a0/a1` |
| -60 | -$003C | pub | `SetConductorState` | player,state,time | `a0,d0/d1` |
| -66 | -$0042 | pub | `ExternalSync` | player,minTime,maxTime | `a0,d0/d1` |
| -72 | -$0048 | pub | `NextConductor` | previousConductor | `a0` |
| -78 | -$004E | pub | `FindConductor` | name | `a0` |
| -84 | -$0054 | pub | `GetPlayerAttrsA` | player,tagList | `a0/a1` |

### translator.library

Source: `NDK_3.9/Include/fd/translator_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `Translate` | inputString,inputLength,outputBuffer,bufferSize | `a0,d0/a1,d1` |

### bullet.library (diskfont outline engine)

Source: `NDK_3.9/Include/fd/bullet_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `OpenEngine` | — | `—` |
| -36 | -$0024 | pub | `CloseEngine` | glyphEngine | `a0` |
| -42 | -$002A | pub | `SetInfoA` | glyphEngine,tagList | `a0/a1` |
| -48 | -$0030 | pub | `ObtainInfoA` | glyphEngine,tagList | `a0/a1` |
| -54 | -$0036 | pub | `ReleaseInfoA` | glyphEngine,tagList | `a0/a1` |
| -60 | -$003C | priv | `bulletPrivate1` | — | `—` |

### rexxsyslib.library

Source: `NDK_3.9/Include/fd/rexxsyslib_lib.fd`

Base bias: 126 (first entry LVO = -126)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -126 | -$007E | pub | `CreateArgstring` | string,length | `a0,d0` |
| -132 | -$0084 | pub | `DeleteArgstring` | argstring | `a0` |
| -138 | -$008A | pub | `LengthArgstring` | argstring | `a0` |
| -144 | -$0090 | pub | `CreateRexxMsg` | port,extension,host | `a0/a1,d0` |
| -150 | -$0096 | pub | `DeleteRexxMsg` | packet | `a0` |
| -156 | -$009C | pub | `ClearRexxMsg` | msgptr,count | `a0,d0` |
| -162 | -$00A2 | pub | `FillRexxMsg` | msgptr,count,mask | `a0,d0/d1` |
| -168 | -$00A8 | pub | `IsRexxMsg` | msgptr | `a0` |
| -450 | -$01C2 | pub | `LockRexxBase` | resource | `d0` |
| -456 | -$01C8 | pub | `UnlockRexxBase` | resource | `d0` |

### amigaguide.library

Source: `NDK_3.9/Include/fd/amigaguide_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | priv | `amigaguidePrivate1` | — | `—` |
| -36 | -$0024 | pub | `LockAmigaGuideBase` | handle | `a0` |
| -42 | -$002A | pub | `UnlockAmigaGuideBase` | key | `d0` |
| -48 | -$0030 | priv | `amigaguidePrivate2` | — | `—` |
| -54 | -$0036 | pub | `OpenAmigaGuideA` | nag,tags | `a0/a1` |
| -60 | -$003C | pub | `OpenAmigaGuideAsyncA` | nag,attrs | `a0,d0` |
| -66 | -$0042 | pub | `CloseAmigaGuide` | cl | `a0` |
| -72 | -$0048 | pub | `AmigaGuideSignal` | cl | `a0` |
| -78 | -$004E | pub | `GetAmigaGuideMsg` | cl | `a0` |
| -84 | -$0054 | pub | `ReplyAmigaGuideMsg` | amsg | `a0` |
| -90 | -$005A | pub | `SetAmigaGuideContextA` | cl,id,attrs | `a0,d0/d1` |
| -96 | -$0060 | pub | `SendAmigaGuideContextA` | cl,attrs | `a0,d0` |
| -102 | -$0066 | pub | `SendAmigaGuideCmdA` | cl,cmd,attrs | `a0,d0/d1` |
| -108 | -$006C | pub | `SetAmigaGuideAttrsA` | cl,attrs | `a0/a1` |
| -114 | -$0072 | pub | `GetAmigaGuideAttr` | tag,cl,storage | `d0/a0/a1` |
| -120 | -$0078 | priv | `amigaguidePrivate3` | — | `—` |
| -126 | -$007E | pub | `LoadXRef` | lock,name | `a0/a1` |
| -132 | -$0084 | pub | `ExpungeXRef` | — | `—` |
| -138 | -$008A | pub | `AddAmigaGuideHostA` | h,name,attrs | `a0,d0/a1` |
| -144 | -$0090 | pub | `RemoveAmigaGuideHostA` | hh,attrs | `a0/a1` |
| -150 | -$0096 | priv | `amigaguidePrivate4` | — | `—` |
| -156 | -$009C | priv | `amigaguidePrivate5` | — | `—` |
| -162 | -$00A2 | priv | `amigaguidePrivate6` | — | `—` |
| -168 | -$00A8 | priv | `amigaguidePrivate7` | — | `—` |
| -174 | -$00AE | priv | `amigaguidePrivate8` | — | `—` |
| -180 | -$00B4 | priv | `amigaguidePrivate9` | — | `—` |
| -186 | -$00BA | priv | `amigaguidePrivate10` | — | `—` |
| -192 | -$00C0 | priv | `amigaguidePrivate11` | — | `—` |
| -198 | -$00C6 | priv | `amigaguidePrivate12` | — | `—` |
| -204 | -$00CC | priv | `amigaguidePrivate13` | — | `—` |
| -210 | -$00D2 | pub | `GetAmigaGuideString` | id | `d0` |

### aml.library (mail)

Source: `NDK_3.9/Include/fd/aml_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `RexxDispatcher` | rxm | `a0` |
| -36 | -$0024 | pub | `CreateServerA` | tags | `a0` |
| -42 | -$002A | pub | `DisposeServer` | server | `a0` |
| -48 | -$0030 | pub | `SetServerAttrsA` | server,tags | `a0/a1` |
| -54 | -$0036 | pub | `GetServerAttrsA` | server,tags | `a0/a1` |
| -60 | -$003C | pub | `GetServerHeaders` | server,flags | `a0,d0` |
| -66 | -$0042 | pub | `GetServerArticles` | server,folder,hook,flags | `a0/a1/a2,d0` |
| -72 | -$0048 | pub | `CreateFolderA` | server,tags | `a0/a1` |
| -78 | -$004E | pub | `DisposeFolder` | folder | `a0` |
| -84 | -$0054 | pub | `OpenFolderA` | server,tags | `a0/a1` |
| -90 | -$005A | pub | `SaveFolder` | folder | `a0` |
| -96 | -$0060 | pub | `RemFolder` | folder | `a0` |
| -102 | -$0066 | pub | `SetFolderAttrsA` | folder,tags | `a0/a1` |
| -108 | -$006C | pub | `GetFolderAttrsA` | folder,tags | `a0/a1` |
| -114 | -$0072 | pub | `AddFolderArticle` | folder,type,data | `a0,d0/a1` |
| -120 | -$0078 | pub | `RemFolderArticle` | folder,article | `a0/a1` |
| -126 | -$007E | pub | `ReadFolderSpool` | folder,importfile,flags | `a0/a1,d0` |
| -132 | -$0084 | pub | `WriteFolderSpool` | folder,exportfile,flags | `a0/a1,d0` |
| -138 | -$008A | pub | `ScanFolderIndex` | folder,hook,flags | `a0/a1,d0` |
| -144 | -$0090 | pub | `ExpungeFolder` | folder,trash,hook | `a0/a1/a2` |
| -150 | -$0096 | pub | `CreateFolderIndex` | folder | `a0` |
| -156 | -$009C | pub | `SortFolderIndex` | folder,field | `a0,d0` |
| -162 | -$00A2 | pub | `CreateArticleA` | folder,tags | `a0/a1` |
| -168 | -$00A8 | pub | `DisposeArticle` | article | `a0` |
| -174 | -$00AE | pub | `OpenArticle` | server,folder,msgID,flags | `a0/a1,d0/d1` |
| -180 | -$00B4 | pub | `CopyArticle` | folder,article | `a0/a1` |
| -186 | -$00BA | pub | `SetArticleAttrsA` | article,tags | `a0/a1` |
| -192 | -$00C0 | pub | `GetArticleAttrsA` | article,tags | `a0/a1` |
| -198 | -$00C6 | pub | `SendArticle` | server,article,from_file | `a0/a1/a2` |
| -204 | -$00CC | pub | `AddArticlePartA` | article,part,tags | `a0/a1/a2` |
| -210 | -$00D2 | pub | `RemArticlePart` | article,part | `a0,d0` |
| -216 | -$00D8 | pub | `GetArticlePart` | article,partnum | `a0,d0` |
| -222 | -$00DE | pub | `GetArticlePartAttrsA` | part,tags | `a0/a1` |
| -228 | -$00E4 | pub | `SetArticlePartAttrsA` | part,tags | `a0/a1` |
| -234 | -$00EA | pub | `CreateArticlePartA` | article,tags | `a0/a1` |
| -240 | -$00F0 | pub | `DisposeArticlePart` | part | `a0` |
| -246 | -$00F6 | pub | `GetArticlePartDataA` | article,part,tags | `a0/a1/a2` |
| -252 | -$00FC | pub | `SetArticlePartDataA` | part,tags | `a0/a1` |
| -258 | -$0102 | pub | `CreateAddressEntryA` | tags | `a0` |
| -264 | -$0108 | pub | `DisposeAddressEntry` | addr | `a0` |
| -270 | -$010E | pub | `OpenAddressEntry` | server,fileid | `a0,d0` |
| -276 | -$0114 | pub | `SaveAddressEntry` | server,addr | `a0/a1` |
| -282 | -$011A | pub | `RemAddressEntry` | server,addr | `a0/a1` |
| -288 | -$0120 | pub | `GetAddressEntryAttrsA` | addr,tags | `a0/a1` |
| -294 | -$0126 | pub | `SetAddressEntryAttrsA` | addr,tags | `a0/a1` |
| -300 | -$012C | pub | `MatchAddressA` | addr,tags | `a0/a1` |
| -306 | -$0132 | pub | `FindAddressEntryA` | server,tags | `a0/a1` |
| -312 | -$0138 | pub | `HuntAddressEntryA` | server,tags | `a0/a1` |
| -318 | -$013E | pub | `ScanAddressIndex` | server,hook,type,flags | `a0/a1,d0/d1` |
| -324 | -$0144 | pub | `AddCustomField` | addr,field,data | `a0/a1/a2` |
| -330 | -$014A | pub | `RemCustomField` | addr,field | `a0/a1` |
| -336 | -$0150 | pub | `GetCustomFieldData` | addr,field | `a0/a1` |
| -342 | -$0156 | pub | `CreateDecoderA` | tags | `a0` |
| -348 | -$015C | pub | `DisposeDecoder` | dec | `a0` |
| -354 | -$0162 | pub | `GetDecoderAttrsA` | dec,tags | `a0/a1` |
| -360 | -$0168 | pub | `SetDecoderAttrsA` | dec,tags | `a0/a1` |
| -366 | -$016E | pub | `Decode` | dec,type | `a0,d0` |
| -372 | -$0174 | pub | `Encode` | dec,type | `a0,d0` |

### nonvolatile.library

Source: `NDK_3.9/Include/fd/nonvolatile_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `GetCopyNV` | appName,itemName,killRequesters | `a0/a1,d1` |
| -36 | -$0024 | pub | `FreeNVData` | data | `a0` |
| -42 | -$002A | pub | `StoreNV` | appName,itemName,data,length,killRequesters | `a0/a1/a2,d0/d1` |
| -48 | -$0030 | pub | `DeleteNV` | appName,itemName,killRequesters | `a0/a1,d1` |
| -54 | -$0036 | pub | `GetNVInfo` | killRequesters | `d1` |
| -60 | -$003C | pub | `GetNVList` | appName,killRequesters | `a0,d1` |
| -66 | -$0042 | pub | `SetNVProtection` | appName,itemName,mask,killRequesters | `a0/a1,d2,d1` |

### resource.library (Reaction)

Source: `NDK_3.9/Include/fd/resource_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `RL_OpenResource` | resource,screen,catalog | `a0/a1/a2` |
| -36 | -$0024 | pub | `RL_CloseResource` | resfile | `a0` |
| -42 | -$002A | pub | `RL_NewObjectA` | resfile,resid,tags | `a0,d0/a1` |
| -48 | -$0030 | pub | `RL_DisposeObject` | resfile,obj | `a0/a1` |
| -54 | -$0036 | pub | `RL_NewGroupA` | resfile,id,taglist | `a0,d0/a1` |
| -60 | -$003C | pub | `RL_DisposeGroup` | resfile,obj | `a0/a1` |
| -66 | -$0042 | pub | `RL_GetObjectArray` | resfile,obj,id | `a0/a1,d0` |
| -72 | -$0048 | pub | `RL_SetResourceScreen` | resfile,screen | `a0/a1` |

### cia.resource

Source: `NDK_3.9/Include/fd/cia_lib.fd`

Base bias: 6 (first entry LVO = -6)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -6 | -$0006 | pub | `AddICRVector` | resource,iCRBit,interrupt | `a6,d0/a1` |
| -12 | -$000C | pub | `RemICRVector` | resource,iCRBit,interrupt | `a6,d0/a1` |
| -18 | -$0012 | pub | `AbleICR` | resource,mask | `a6,d0` |
| -24 | -$0018 | pub | `SetICR` | resource,mask | `a6,d0` |

### disk.resource

Source: `NDK_3.9/Include/fd/disk_lib.fd`

Base bias: 6 (first entry LVO = -6)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -6 | -$0006 | pub | `AllocUnit` | unitNum | `d0` |
| -12 | -$000C | pub | `FreeUnit` | unitNum | `d0` |
| -18 | -$0012 | pub | `GetUnit` | unitPointer | `a1` |
| -24 | -$0018 | pub | `GiveUnit` | — | `—` |
| -30 | -$001E | pub | `GetUnitID` | unitNum | `d0` |
| -36 | -$0024 | pub | `ReadUnitID` | unitNum | `d0` |

## 11.2 Resources & low-level devices

### battclock.resource

Source: `NDK_3.9/Include/fd/battclock_lib.fd`

Base bias: 6 (first entry LVO = -6)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -6 | -$0006 | pub | `ResetBattClock` | — | `—` |
| -12 | -$000C | pub | `ReadBattClock` | — | `—` |
| -18 | -$0012 | pub | `WriteBattClock` | time | `d0` |
| -24 | -$0018 | priv | `battclockPrivate1` | — | `—` |
| -30 | -$001E | priv | `battclockPrivate2` | — | `—` |

### battmem.resource

Source: `NDK_3.9/Include/fd/battmem_lib.fd`

Base bias: 6 (first entry LVO = -6)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -6 | -$0006 | pub | `ObtainBattSemaphore` | — | `—` |
| -12 | -$000C | pub | `ReleaseBattSemaphore` | — | `—` |
| -18 | -$0012 | pub | `ReadBattMem` | buffer,offset,length | `a0,d0/d1` |
| -24 | -$0018 | pub | `WriteBattMem` | buffer,offset,length | `a0,d0/d1` |

### potgo.resource

Source: `NDK_3.9/Include/fd/potgo_lib.fd`

Base bias: 6 (first entry LVO = -6)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -6 | -$0006 | pub | `AllocPotBits` | bits | `d0` |
| -12 | -$000C | pub | `FreePotBits` | bits | `d0` |
| -18 | -$0012 | pub | `WritePotgo` | word,mask | `d0/d1` |

### misc.resource

Source: `NDK_3.9/Include/fd/misc_lib.fd`

Base bias: 6 (first entry LVO = -6)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -6 | -$0006 | pub | `AllocMiscResource` | unitNum,name | `d0/a1` |
| -12 | -$000C | pub | `FreeMiscResource` | unitNum | `d0` |

### card.resource

Source: `NDK_3.9/Include/fd/cardres_lib.fd`

Base bias: 6 (first entry LVO = -6)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -6 | -$0006 | pub | `OwnCard` | handle | `a1` |
| -12 | -$000C | pub | `ReleaseCard` | handle,flags | `a1,d0` |
| -18 | -$0012 | pub | `GetCardMap` | — | `—` |
| -24 | -$0018 | pub | `BeginCardAccess` | handle | `a1` |
| -30 | -$001E | pub | `EndCardAccess` | handle | `a1` |
| -36 | -$0024 | pub | `ReadCardStatus` | — | `—` |
| -42 | -$002A | pub | `CardResetRemove` | handle,flag | `a1,d0` |
| -48 | -$0030 | pub | `CardMiscControl` | handle,control_bits | `a1,d1` |
| -54 | -$0036 | pub | `CardAccessSpeed` | handle,nanoseconds | `a1,d0` |
| -60 | -$003C | pub | `CardProgramVoltage` | handle,voltage | `a1,d0` |
| -66 | -$0042 | pub | `CardResetCard` | handle | `a1` |
| -72 | -$0048 | pub | `CopyTuple` | handle,buffer,tuplecode,size | `a1,a0,d1,d0` |
| -78 | -$004E | pub | `DeviceTuple` | tuple_data,storage | `a0/a1` |
| -84 | -$0054 | pub | `IfAmigaXIP` | handle | `a2` |
| -90 | -$005A | pub | `CardForceChange` | — | `—` |
| -96 | -$0060 | pub | `CardChangeCount` | — | `—` |
| -102 | -$0066 | pub | `CardInterface` | — | `—` |

### input.device

Source: `NDK_3.9/Include/fd/input_lib.fd`

Base bias: 42 (first entry LVO = -42)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -42 | -$002A | pub | `PeekQualifier` | — | `—` |

### ramdrive.device

Source: `NDK_3.9/Include/fd/ramdrive_lib.fd`

Base bias: 42 (first entry LVO = -42)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -42 | -$002A | pub | `KillRAD0` | — | `—` |
| -48 | -$0030 | pub | `KillRAD` | unit | `d0` |

## 11.3 Reaction gadget classes and other small libraries

These are the ReAction BOOPSI gadget class libraries (V44+) plus a few small helper
libraries. They are typically stubs with class-registration LVOs only.

### arexx.library

Source: `NDK_3.9/Include/fd/arexx_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `AREXX_GetClass` | — | `—` |

### bevel.library

Source: `NDK_3.9/Include/fd/bevel_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `BEVEL_GetClass` | — | `—` |

### bitmap.library

Source: `NDK_3.9/Include/fd/bitmap_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `BITMAP_GetClass` | — | `—` |

### button.library

Source: `NDK_3.9/Include/fd/button_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `BUTTON_GetClass` | — | `—` |

### checkbox.library

Source: `NDK_3.9/Include/fd/checkbox_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `CHECKBOX_GetClass` | — | `—` |

### chooser.library

Source: `NDK_3.9/Include/fd/chooser_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `CHOOSER_GetClass` | — | `—` |
| -36 | -$0024 | pub | `AllocChooserNodeA` | tags | `a0` |
| -42 | -$002A | pub | `FreeChooserNode` | node | `a0` |
| -48 | -$0030 | pub | `SetChooserNodeAttrsA` | node,tags | `a0/a1` |
| -54 | -$0036 | pub | `GetChooserNodeAttrsA` | node,tags | `a0/a1` |

### clicktab.library

Source: `NDK_3.9/Include/fd/clicktab_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `CLICKTAB_GetClass` | — | `—` |
| -36 | -$0024 | pub | `AllocClickTabNodeA` | tags | `a0` |
| -42 | -$002A | pub | `FreeClickTabNode` | node | `a0` |
| -48 | -$0030 | pub | `SetClickTabNodeAttrsA` | node,tags | `a0/a1` |
| -54 | -$0036 | pub | `GetClickTabNodeAttrsA` | node,tags | `a0/a1` |

### colorwheel.library

Source: `NDK_3.9/Include/fd/colorwheel_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `ConvertHSBToRGB` | hsb,rgb | `a0/a1` |
| -36 | -$0024 | pub | `ConvertRGBToHSB` | rgb,hsb | `a0/a1` |

### datebrowser.library

Source: `NDK_3.9/Include/fd/datebrowser_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `DATEBROWSER_GetClass` | — | `—` |
| -36 | -$0024 | pub | `JulianWeekDay` | day,month,year | `d0/d1/d2` |
| -42 | -$002A | pub | `JulianMonthDays` | month,year | `d0/d1` |
| -48 | -$0030 | pub | `JulianLeapYear` | year | `d0` |

### drawlist.library

Source: `NDK_3.9/Include/fd/drawlist_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `DRAWLIST_GetClass` | — | `—` |

### dtclass.library

Source: `NDK_3.9/Include/fd/dtclass_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `ObtainEngine` | — | `—` |

### fuelgauge.library

Source: `NDK_3.9/Include/fd/fuelgauge_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `FUELGAUGE_GetClass` | — | `—` |

### getfile.library

Source: `NDK_3.9/Include/fd/getfile_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `GETFILE_GetClass` | — | `—` |

### getfont.library

Source: `NDK_3.9/Include/fd/getfont_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `GETFONT_GetClass` | — | `—` |

### getscreenmode.library

Source: `NDK_3.9/Include/fd/getscreenmode_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `GETSCREENMODE_GetClass` | — | `—` |

### glyph.library

Source: `NDK_3.9/Include/fd/glyph_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `GLYPH_GetClass` | — | `—` |

### hdwrench.library

Source: `NDK_3.9/Include/fd/hdwrench.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `HDWOpenDevice` | DevName,Unit | `A0,D0` |
| -36 | -$0024 | pub | `HDWCloseDevice` | void | `—` |
| -42 | -$002A | pub | `RawRead` | *bbk,size | `A0,D0` |
| -48 | -$0030 | pub | `RawWrite` | *bb | `A0` |
| -54 | -$0036 | pub | `WriteBlock` | *bb | `A0` |
| -60 | -$003C | pub | `ReadRDBs` | void | `—` |
| -66 | -$0042 | pub | `WriteRDBs` | void | `—` |
| -72 | -$0048 | pub | `QueryReady` | errorcode | `A0` |
| -78 | -$004E | pub | `QueryInquiry` | inqbuf,errorcode | `A0/A1` |
| -84 | -$0054 | pub | `QueryModeSense` | page,msbsize,msbuf,errorcode | `D0/D1,A0/A1` |
| -90 | -$005A | pub | `QueryFindValid` | *ValidIDs,devicename,board,types,wide_scsi,Callback | `A0/A1,D0/D1/D2,A2` |
| -96 | -$0060 | pub | `QueryCapacity` | totalblocks,blocksize | `A0/A1` |
| -102 | -$0066 | pub | `ReadMountfile` | unit,*filename,controller | `D0,A0/A1` |
| -108 | -$006C | pub | `ReadRDBStructs` | *filename, unit | `A0,D0` |
| -114 | -$0072 | pub | `WriteMountfile` | filename,*ldir,unit | `A0/A1,D0` |
| -120 | -$0078 | pub | `WriteRDBStructs` | *filename | `A0` |
| -126 | -$007E | pub | `InMemMountfile` | unit,*mfdata,*controller | `D0,A0/A1` |
| -132 | -$0084 | pub | `InMemRDBStructs` | *rdbp,sizerdb,unit | `A0,D0/D1` |
| -138 | -$008A | pub | `OutMemMountfile` | *mfp,*sizew,sizeb,unit | `A0/A1,D0/D1` |
| -144 | -$0090 | pub | `OutMemRDBStructs` | *rdbp,*sizew,sizeb | `A0/A1,D0` |
| -150 | -$0096 | pub | `FindDiskName` | *diskname | `A0` |
| -156 | -$009C | pub | `FindControllerID` | *devname,*selfid | `A0/A1` |
| -162 | -$00A2 | pub | `FindLastSector` | void | `—` |
| -168 | -$00A8 | pub | `FindDefaults` | Optimize,*Return | `D0,A0` |
| -174 | -$00AE | pub | `LowlevelFormat` | Callback | `A0` |
| -180 | -$00B4 | pub | `VerifyDrive` | CallBack | `A0` |

### integer.library

Source: `NDK_3.9/Include/fd/integer_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `INTEGER_GetClass` | — | `—` |

### label.library

Source: `NDK_3.9/Include/fd/label_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `LABEL_GetClass` | — | `—` |

### layout.library

Source: `NDK_3.9/Include/fd/layout_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `LAYOUT_GetClass` | — | `—` |
| -36 | -$0024 | pub | `ActivateLayoutGadget` | gadget,window,requester,object | `a0/a1/a2,d0` |
| -42 | -$002A | pub | `FlushLayoutDomainCache` | gadget | `a0` |
| -48 | -$0030 | pub | `RethinkLayout` | gadget,window,requester,refresh | `a0/a1/a2,d0` |
| -54 | -$0036 | pub | `LayoutLimits` | gadget,limits,font,screen | `a0/a1/a2/a3` |
| -60 | -$003C | pub | `PAGE_GetClass` | — | `—` |
| -66 | -$0042 | pub | `SetPageGadgetAttrsA` | gadget,object,window,requester,tags | `a0/a1/a2/a3/a4` |
| -72 | -$0048 | pub | `RefreshPageGadget` | gadget,object,window,requester | `a0/a1/a2/a3` |

### listbrowser.library

Source: `NDK_3.9/Include/fd/listbrowser_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `LISTBROWSER_GetClass` | — | `—` |
| -36 | -$0024 | pub | `AllocListBrowserNodeA` | columns,tags | `d0/a0` |
| -42 | -$002A | pub | `FreeListBrowserNode` | node | `a0` |
| -48 | -$0030 | pub | `SetListBrowserNodeAttrsA` | node,tags | `a0/a1` |
| -54 | -$0036 | pub | `GetListBrowserNodeAttrsA` | node,tags | `a0/a1` |
| -60 | -$003C | pub | `ListBrowserSelectAll` | list | `a0` |
| -66 | -$0042 | pub | `ShowListBrowserNodeChildren` | node,depth | `a0,d0` |
| -72 | -$0048 | pub | `HideListBrowserNodeChildren` | node | `a0` |
| -78 | -$004E | pub | `ShowAllListBrowserChildren` | list | `a0` |
| -84 | -$0054 | pub | `HideAllListBrowserChildren` | list | `a0` |
| -90 | -$005A | pub | `FreeListBrowserList` | list | `a0` |
| -96 | -$0060 | pub | `AllocLBColumnInfoA` | columns,tags | `d0/a0` |
| -102 | -$0066 | pub | `SetLBColumnInfoAttrsA` | columninfo,tags | `a1,a0` |
| -108 | -$006C | pub | `GetLBColumnInfoAttrsA` | columninfo,tags | `a1,a0` |
| -114 | -$0072 | pub | `FreeLBColumnInfo` | columninfo | `a0` |
| -120 | -$0078 | pub | `ListBrowserClearAll` | list | `a0` |

### palette.library

Source: `NDK_3.9/Include/fd/palette_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `PALETTE_GetClass` | — | `—` |

### penmap.library

Source: `NDK_3.9/Include/fd/penmap_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `PENMAP_GetClass` | — | `—` |

### popcycle.library

Source: `NDK_3.9/Include/fd/popcycle_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `POPCYCLE_GetClass` | — | `—` |
| -36 | -$0024 | pub | `AllocPopCycleNodeA` | tags | `a0` |
| -42 | -$002A | pub | `FreePopCycleNode` | node | `a0` |
| -48 | -$0030 | pub | `SetPopCycleNodeAttrsA` | node,tags | `a0/a1` |
| -54 | -$0036 | pub | `GetPopCycleNodeAttrsA` | node,tags | `a0/a1` |

### radiobutton.library

Source: `NDK_3.9/Include/fd/radiobutton_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `RADIOBUTTON_GetClass` | — | `—` |
| -36 | -$0024 | pub | `AllocRadioButtonNodeA` | columns,tags | `d0/a0` |
| -42 | -$002A | pub | `FreeRadioButtonNode` | node | `a0` |
| -48 | -$0030 | pub | `SetRadioButtonNodeAttrsA` | node,tags | `a0/a1` |
| -54 | -$0036 | pub | `GetRadioButtonNodeAttrsA` | node,tags | `a0/a1` |

### requester.library

Source: `NDK_3.9/Include/fd/requester_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `REQUESTER_GetClass` | — | `—` |

### scroller.library

Source: `NDK_3.9/Include/fd/scroller_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `SCROLLER_GetClass` | — | `—` |

### slider.library

Source: `NDK_3.9/Include/fd/slider_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `SLIDER_GetClass` | — | `—` |

### space.library

Source: `NDK_3.9/Include/fd/space_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `SPACE_GetClass` | — | `—` |

### speedbar.library

Source: `NDK_3.9/Include/fd/speedbar_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `SPEEDBAR_GetClass` | — | `—` |
| -36 | -$0024 | pub | `AllocSpeedButtonNodeA` | number,tags | `d0/a0` |
| -42 | -$002A | pub | `FreeSpeedButtonNode` | node | `a0` |
| -48 | -$0030 | pub | `SetSpeedButtonNodeAttrsA` | node,tags | `a0/a1` |
| -54 | -$0036 | pub | `GetSpeedButtonNodeAttrsA` | node,tags | `a0/a1` |

### string.library

Source: `NDK_3.9/Include/fd/string_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `STRING_GetClass` | — | `—` |

### texteditor.library

Source: `NDK_3.9/Include/fd/texteditor_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `TEXTEDITOR_GetClass` | — | `—` |

### virtual.library

Source: `NDK_3.9/Include/fd/virtual_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `VIRTUAL_GetClass` | — | `—` |
| -36 | -$0024 | pub | `RefreshVirtualGadget` | gadget,obj,window,requester | `a0/a1/a2/a3` |
| -42 | -$002A | pub | `RethinkVirtualSize` | virt_obj,rootlayout,font,screen,layoutlimits | `a0/a1/a2/a3,d0` |

### window.library

Source: `NDK_3.9/Include/fd/window_lib.fd`

Base bias: 30 (first entry LVO = -30)

| LVO (dec) | LVO (hex) | Vis | Function | Args | Registers |
|----------:|----------:|:---:|:---------|:-----|:----------|
| -30 | -$001E | pub | `WINDOW_GetClass` | — | `—` |

