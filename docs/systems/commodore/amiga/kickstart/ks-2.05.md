# Kickstart 2.05 — A600

## ROM Identification

| Property | Build | File | exec version |
|----------|-------|------|-------------|
| KS 2.05 | 37.300 | `kick205_37_300_a600hd.rom` | exec 37.151 (1.11.91) |
| KS 2.05 | 37.350 | `kick205_37_350_a600hd.rom` | exec 37.152 (27.3.92) |

Both ROMs: 512K, SSP=$11144EF9, PC=$F800D2, mapped at $F80000–$FFFFFF.

Target machine: A600 (ECS chipset, 68000, Gayle IDE controller, PCMCIA slot).

The two builds differ substantially (~32K hex dump lines). Build 37.350 is a
later revision with bug fixes but the same boot architecture.

## Boot Flow

### Entry Point ($F800D2)

Same ROM checksum and diagnostic ROM check as KS 2.04, then:

### PCMCIA Probe ($F8010C) — New

**First Kickstart with PCMCIA support.** After the diagnostic ROM check, KS 2.05
probes for a PCMCIA card before clearing the overlay:

```asm
$F800F0  LEA     (PC),$F8014A,A5      ; Skip-PCMCIA return address
$F800F4  LEA     (PC),$F80000,A0      ; ROM base
$F800F8  LEA     $F00000,A1           ; Diagnostic ROM
$F800FE  CMPA.L  A0,A1
$F80100  BEQ.S   $F8010C              ; Running from diagnostic → skip
$F80102  CMPI.W  #$1111,(A1)          ; Diagnostic magic?
$F80106  BNE.S   $F8010C
$F80108  JMP     2(A1)

; PCMCIA probe at $A00000
$F8010C  LEA     $A00000,A1           ; PCMCIA attribute memory base
$F80112  CMPI.B  #$91,(A1)            ; CIS tuple CISTPL_DEVICE = $91?
$F80116  BNE.S   $F8014A              ; No card → skip
$F80118  ADDQ.L  #2,A1
$F8011A  CMPI.B  #$05,(A1)            ; Check next byte
$F8011E  BNE.S   $F8014A
$F80120  ADDQ.L  #2,A1
$F80122  CMPI.B  #$23,(A1)            ; Check next byte
$F80126  BNE.S   $F8014A

; Card detected — read 4-byte boot vector from attribute memory
$F80128  ADDQ.L  #2,A1
$F8012A  MOVE.B  (A1),D0              ; Byte 0 (bits 31-24)
$F8012C  ROR.L   #8,D0
$F8012E  ADDQ.L  #2,A1
$F80130  MOVE.B  (A1),D0              ; Byte 1 (bits 23-16)
$F80132  ROR.L   #8,D0
$F80134  ADDQ.L  #2,A1
$F80136  MOVE.B  (A1),D0              ; Byte 2 (bits 15-8)
$F80138  ROR.L   #8,D0
$F8013A  ADDQ.L  #2,A1
$F8013C  MOVE.B  (A1),D0              ; Byte 3 (bits 7-0)
$F8013E  ROR.L   #8,D0
$F80140  LEA     $600000,A0           ; PCMCIA common memory base
$F80146  ADDA.L  D0,A0               ; Add boot vector offset
$F80148  JMP     (A0)                 ; Boot from PCMCIA card
```

If the PCMCIA signature bytes ($91, $05, $23) are found at $A00000, the ROM
reads a 4-byte offset and jumps to $600000 + offset. This allows booting from
a PCMCIA card before the normal Kickstart boot sequence.

### CIA/Custom Reset ($F8014A)

Same as KS 2.04 — overlay clear, INTENA/INTREQ/DMACON disable, BPLCON0=$0200.

### Remaining Boot ($F80186+)

Identical structure to KS 2.04: ROM checksum verify, exception vector setup
with read-back check, warm/cold start detection.

## Resident Modules

43 modules (3 more than KS 2.04):

| Pri | Name | Type | Version | Init | Notes |
|-----|------|------|---------|------|-------|
| +110 | expansion.library | LIBRARY | 37 | $F83CF4 | |
| +105 | exec.library | LIBRARY | 37 | $F80460 | v37.151 |
| +105 | diag init | — | 37 | $F83FD6 | |
| +103 | utility.library | LIBRARY | 37 | $FED5C4 | |
| +100 | potgo.resource | RESOURCE | 37 | $F9D6A0 | |
| +80 | cia.resource | RESOURCE | 37 | $F8CDFC | v37.13 |
| +80 | FileSystem.resource | RESOURCE | 37 | $F9D554 | |
| +70 | disk.resource | RESOURCE | 37 | $F939D8 | |
| +70 | misc.resource | RESOURCE | 37 | $FE0D74 | |
| +65 | graphics.library | LIBRARY | 37 | $FA8C70 | v37.41 |
| +60 | gameport.device | DEVICE | 37 | $FE3EF6 | |
| +50 | timer.device | DEVICE | 37 | $FEAAC4 | |
| **+48** | **card.resource** | **RESOURCE** | **37** | **$FC0BC4** | **New — PCMCIA** |
| +45 | battclock.resource | RESOURCE | 37 | $F89CFC | |
| +45 | keyboard.device | DEVICE | 37 | $FE3F20 | |
| +44 | battmem.resource | RESOURCE | 37 | $F8A710 | |
| +40 | keymap.library | LIBRARY | 37 | $FDC5C2 | |
| +40 | input.device | DEVICE | 37 | $FE3F4A | |
| +31 | layers.library | LIBRARY | 37 | $FDD182 | v37.9 |
| +25 | ramdrive.device | DEVICE | 37 | $FE3326 | v37.27 |
| +20 | trackdisk.device | DEVICE | 37 | $FEBBDA | |
| **+15** | **carddisk.device** | **DEVICE** | **37** | **$F85A10** | **New — PCMCIA disk** |
| **+10** | **scsi.device** | **DEVICE** | **37** | **$F862B8** | **New — Gayle IDE** |
| +10 | intuition.library | LIBRARY | 37 | $FC3B22 | v37.331 |
| +5 | alert.hook | — | 37 | $F83AAE | |
| +5 | console.device | DEVICE | 37 | $F8F91A | |
| 0 | mathieeesingbas.library | LIBRARY | 37 | $F84706 | |
| −35 | syscheck | — | 37 | $F8BFA0 | |
| −40 | romboot | — | 37 | $FE9AE4 | v37.25 |
| −50 | bootmenu | — | 37 | $F8A904 | |
| −60 | strap | — | 37 | $FE9D08 | v37.25 |
| −81 | filesystem | — | 37 | $F9D79C | v37.28 |
| −100 | ramlib | — | 37 | $FE3A56 | |
| −120 | audio.device | DEVICE | 37 | $F88BE0 | |
| −120 | dos.library | LIBRARY | 37 | $F949BA | v37.45 |
| −120 | gadtools.library | LIBRARY | 37 | $FA8B94 | |
| −120 | icon.library | LIBRARY | 37 | $FC1670 | |
| −120 | mathffp.library | LIBRARY | 37 | $FE0894 | |
| −120 | workbench.task | TASK | 37 | $FEDFA4 | |
| −120 | workbench.library | LIBRARY | 37 | $FEE096 | |
| −121 | con-handler | — | 37 | $F8DD20 | |
| −122 | shell | — | 37 | $FE5598 | |
| −123 | ram-handler | — | 37 | $FE0DE4 | |

### New modules vs KS 2.04

- **card.resource** (pri 48) — PCMCIA card resource manager (Gayle PCMCIA controller)
- **carddisk.device** (pri 15) — PCMCIA disk device
- **scsi.device** (pri 10) — Gayle IDE controller (drives the A600's internal IDE)

Note: "scsi.device" on the A600 is the Gayle IDE driver, not a true SCSI device.
Commodore reused the name for compatibility with A3000 software.

## Hardware Probing

| Address | Read/Write | Purpose |
|---------|-----------|---------|
| $A00000 | read | PCMCIA attribute memory — CIS tuple check ($91, $05, $23) |
| $600000 | JMP | PCMCIA common memory — boot vector target |
| $DA0000–$DA8000 | read/write | Gayle registers (IDE + PCMCIA control) |
| $DFF07C | read | DENISEID — ECS detection |

## Variant Differences (37.300 vs 37.350)

The two builds differ by ~32K hex lines. Build 37.350 (exec 37.152) is a bug-fix
release — same module set, same boot architecture. Entry code is identical.

## STRAP Display

Hires 3-plane insert-disk screen (same as KS 2.04):
- BPLCON0 = $8302
- DMACON set bits include $0180

## Emulator Implications

- **PCMCIA at $A00000** must return $FF (no card) or valid CIS tuples. If the
  emulator maps garbage at $A00000, the boot may incorrectly detect a PCMCIA card
  and jump to invalid code.
- **Gayle at $DA0000** must respond to reads/writes for IDE and PCMCIA to work.
- **No 68030 requirements** — A600 runs a 68000.
- This ROM requires the same **chip RAM verify** as KS 2.04 (exception vector
  read-back check).
