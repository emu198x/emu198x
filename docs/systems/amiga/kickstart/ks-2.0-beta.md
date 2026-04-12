# Kickstart 2.0 Beta (36.028) — A3000

## ROM Identification

| Property | Value |
|----------|-------|
| File | `kick20_36_028_a3000_beta.rom` |
| Size | 512K (524,288 bytes) |
| Version | exec 36.88 (28.3.90) |
| Target | A3000 |
| SSP | $11144EF9 |
| PC | $F800D2 |
| Mapped at | $F80000–$FFFFFF |

This is a pre-release beta of Kickstart 2.0, built specifically for the A3000.
It introduces several features not present in KS 1.x: ROM checksum verification,
68030 MMU management, RAMSEY/Fat Gary probing, and utility.library.

## Boot Flow

### Stage 1: Reset Vector Fetch

Standard — overlay maps ROM at $000000, CPU reads SSP/PC.

### Stage 2–3: Entry Point ($F800D2)

KS 2.0 beta introduces **ROM checksum verification** before any hardware access.
This is the first Kickstart to do this.

```asm
$F800D2  LEA     $0400,SP              ; Temporary stack at $400
$F800D6  LEA     $F80000,A0            ; ROM base
$F800DC  MOVE.L  #$1FFFF,D1            ; 131072 longwords = 512K
$F800E2  MOVE.L  D1,D2
$F800E4  SWAP    D2                    ; Outer loop counter
$F800E6  MOVEQ   #0,D5                 ; Running checksum
$F800E8  ADD.L   (A0)+,D5              ; Sum each longword
$F800EA  BCC.S   $F800EE               ; If no carry, skip
$F800EC  ADDQ.L  #1,D5                 ; Add carry (ones' complement)
$F800EE  DBF     D1,$F800E8            ; Inner loop
$F800F2  DBF     D2,$F800E8            ; Outer loop
; Result in D5 — should be $FFFFFFFF for valid ROM
```

After the checksum, the standard diagnostic ROM check at $F00000 and overlay
clear follow the same pattern as KS 1.3 (see [boot-flow-overview.md](boot-flow-overview.md)
Stage 3).

### Stage 3a: A3000 Hardware Init ($F80150)

**New in KS 2.0** — A3000-specific hardware probing before memory detection:

```asm
$F80150  BCLR    #7,$DE0002            ; Read RAMSEY control register
$F80158  BEQ.S   $F80188               ; If bit 7 clear → no RAMSEY (not A3000)

; RAMSEY present — disable 68030 MMU before memory detection
$F8015A  CLR.L   $010C                 ; Clear F-line vector
$F8015E  CLR.L   $0000                 ; Clear bus error vector
$F80162  CLR.L   $0004                 ; Clear ExecBase pointer
$F80166  CLR.L   -(SP)                 ; Push 0
$F80168  PMOVE   (SP),TC               ; Disable MMU translation
$F8016C  PMOVE   (SP),TT0              ; Clear transparent translation 0
$F80170  PMOVE   (SP),TT1              ; Clear transparent translation 1
$F80174  MOVE.L  #$80000001,-(SP)      ; Supervisor root pointer
$F8017A  PMOVE   (SP),CRP              ; Set CPU root pointer
$F8017E  PMOVE   (SP),SRP              ; Set supervisor root pointer
$F80182  ADDQ.L  #8,SP
$F80184  BRA.W   $F801CA               ; Skip to cache/VBR init
```

If RAMSEY is NOT present but MMU TC is non-zero, the ROM copies a small MMU
disable stub to $140 and executes it from there (to survive the address space
change when translation is disabled):

```asm
$F80188  CLR.L   -(SP)
$F8018A  PMOVE   TC,(SP)               ; Read current TC
$F80192  TST.L   (SP)
$F80194  ADDQ.L  #4,SP
$F80196  BMI.S   $F801CA               ; TC negative → already disabled
$F80198  BEQ.W   $F801CA               ; TC zero → not enabled
; Copy stub to $140 and JMP there
$F801B0  JMP     $0140
; Stub at $140:
;   CLR.L   -(SP)
;   PMOVE   TC,(SP)                    ; Read TC
;   BSET    #7,(SP)                    ; Set enable bit
;   PMOVE   (SP),TC                    ; Write back (enables, then next instr disables)
;   JMP     $F80002                    ; Jump back to ROM
```

### Stage 3b: Cache and VBR Init ($F801CA)

```asm
$F801CA  MOVEQ   #0,D0
$F801CC  MOVEC   D0,CACR               ; Disable all caches
$F801D0  MOVEC   D0,VBR                ; Reset vector base to $000000
```

This is the first KS to explicitly clear the VBR and CACR — required because
the A3000's 68030 retains these across warm resets.

### Stages 4–12

The remaining boot flow follows the pattern described in
[boot-flow-overview.md](boot-flow-overview.md) with these A3000-specific additions:
- RAMSEY configuration for fast RAM sizing
- DMAC/SCSI controller probe at $DD0000
- battclock.resource and battmem.resource for battery-backed clock and memory

## Resident Modules

39 modules (sorted by init priority):

| Pri | Name | Type | Version | Init |
|-----|------|------|---------|------|
| +110 | expansion.library | LIBRARY | 36 | $F96254 |
| +105 | exec.library | LIBRARY | 36 | $F80460 |
| +103 | utility.library | LIBRARY | 36 | $FE6C34 |
| +102 | diag part 1 | — | 36 | $F963EC |
| +100 | potgo.resource | RESOURCE | 36 | $FD68EA |
| +80 | cia.resource | RESOURCE | 36 | $F875DC |
| +80 | FileSystem.resource | RESOURCE | 36 | $F96B22 |
| +70 | disk.resource | RESOURCE | 36 | $F8CFC2 |
| +70 | misc.resource | RESOURCE | 36 | $FD6834 |
| +65 | graphics.library | LIBRARY | 36 | $FA1060 |
| +60 | gameport.device | DEVICE | 36 | $FD9AC6 |
| +50 | timer.device | DEVICE | 36 | $FE43A0 |
| +45 | battclock.resource | DEVICE | 36 | $F84CDC |
| +45 | keyboard.device | DEVICE | 36 | $FD9AF0 |
| +44 | battmem.resource | DEVICE | 36 | $F8567E |
| +40 | keymap.library | LIBRARY | 36 | $FCFFCA |
| +40 | input.device | DEVICE | 36 | $FD9B1A |
| +31 | layers.library | LIBRARY | 36 | $FD1734 |
| +25 | ramdrive.device | DEVICE | 36 | $FD8FCE |
| +20 | console.device | DEVICE | 36 | $F8958A |
| +20 | trackdisk.device | DEVICE | 36 | $FE52EE |
| +10 | intuition.library | LIBRARY | 36 | $FB9202 |
| +10 | scsi.device | DEVICE | 36 | $FDB242 |
| +5 | alert.hook | — | 36 | $F83A52 |
| 0 | mathieeesingbas.library | LIBRARY | 36 | $FD583A |
| −35 | syscheck | — | 36 | $FE3300 |
| −40 | romboot | — | 36 | $FE2484 |
| −50 | bootmenu | — | 36 | $F857C0 |
| −60 | strap | — | 36 | $FE26A4 |
| −81 | filesystem | — | 36 | $F96BF8 |
| −100 | ramlib | — | 36 | $FD969C |
| −120 | audio.device | DEVICE | 36 | $F83BFE |
| −120 | dos.library | LIBRARY | 36 | $F8D31C |
| −120 | gadtools.library | LIBRARY | 36 | $FA0E54 |
| −120 | mathffp.library | LIBRARY | 36 | $FD5324 |
| −120 | workbench.library | LIBRARY | 36 | $FE75DE |
| −120 | workbench.task | TASK | 36 | $FFC530 |
| −121 | con-handler | — | 36 | $F87B0A |
| −122 | shell | — | 36 | $FDDDC0 |
| −123 | ram-handler | — | 36 | $FD69D8 |
| −128 | audio.device | DEVICE | 36 | $F83BFE |

### New modules vs KS 1.3

- **utility.library** (pri 103) — new in KS 2.0
- **diag part 1** (pri 102) — A3000 diagnostic init
- **battclock.resource** (pri 45) — battery-backed clock
- **battmem.resource** (pri 44) — battery-backed config memory
- **gadtools.library** (pri −120) — new GUI toolkit
- **workbench.library** (pri −120) — new in 2.0 (replaces 1.x icon-based WB)
- **syscheck** (pri −35) — new system validation
- **bootmenu** (pri −50) — early startup menu (hold both mouse buttons)
- **scsi.device** (pri 10) — A3000 SCSI controller

## Hardware Probing

| Address | Read/Write | Purpose | Expected response |
|---------|-----------|---------|-------------------|
| $DE0002 | BCLR #7 | RAMSEY control register | Bit 7 set = RAMSEY present (A3000) |
| $DE0000 | MOVE.B | RAMSEY config | Write to configure burst/page mode |
| $DD0000–$DD00FF | read | DMAC registers | SCSI DMA controller (A3000) |
| $DFF07C | read | DENISEID | $FF = OCS, $FC = ECS |
| $DFF004 | read | VPOSR | Bit 12 = PAL/NTSC |

## Error Paths

ROM checksum failure (D5 != $FFFFFFFF after NOT) causes immediate branch to
alert handler — red screen with guru meditation.

## STRAP Display

Same structure as KS 1.3 (lowres 2-plane) but with updated graphics.
BPLCON0 = $2302, DMACON set bits include $0180 (BPLEN + COPEN).

## Emulator Implications

- **RAMSEY** at $DE0000 must respond to reads/writes. Without it, the ROM skips
  A3000-specific init but may fail later during fast RAM detection.
- **PMOVE instructions** require 68030 MMU support (or at minimum, must not
  crash the emulator). The ROM expects PMOVE to execute without F-line exception
  on a 68030.
- **CACR/VBR** via MOVEC must work on 68030.
- **ROM checksum** — the ROM image must sum to $FFFFFFFF (ones' complement). A
  corrupted or patched ROM will fail at boot.
