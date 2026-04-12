# Kickstart 2.04 (37.175) — A500+

## ROM Identification

| Property | Value |
|----------|-------|
| File | `kick204_37_175_a500plus.rom` |
| Size | 512K (524,288 bytes) |
| Version | exec 37.132 (23.5.91) |
| Target | A500+ |
| SSP | $11144EF9 |
| PC | $F800D2 |
| Mapped at | $F80000–$FFFFFF |

The first ECS Kickstart ROM. Target machine is the A500+ (Enhanced Chip Set
with Super Denise and Fat Agnus). No A3000-specific hardware probing — this ROM
runs on a 68000-only machine.

## Boot Flow

### Entry Point ($F800D2)

```asm
$F800D2  LEA     $0400,SP              ; Temporary stack
$F800D6  LEA     $F80000,A0            ; ROM base
$F800DC  MOVEQ   #-1,D1               ; $FFFF inner counter
$F800DE  MOVEQ   #1,D2                ; Outer counter (2 × 64K = 512K bytes)
$F800E0  MOVEQ   #0,D5                ; Running checksum
$F800E2  ADD.L   (A0)+,D5             ; Sum longwords
$F800E4  BCC.S   $F800E8
$F800E6  ADDQ.L  #1,D5               ; Carry
$F800E8  DBF     D1,$F800E2
$F800EC  DBF     D2,$F800E2
; D5 now holds ones' complement checksum

$F800F0  LEA     (PC),$F80000,A0      ; ROM base via PC-relative
$F800F4  LEA     $F00000,A1           ; Diagnostic ROM address
$F800FA  CMPA.L  A0,A1                ; Are we running from $F00000?
$F800FC  BEQ.S   $F8010C              ; If yes, skip diag ROM check
$F800FE  LEA     (PC),$F8010C,A5      ; Return address
$F80102  CMPI.W  #$1111,(A1)          ; Diagnostic ROM magic?
$F80106  BNE.S   $F8010C              ; No → continue
$F80108  JMP     2(A1)                ; Yes → jump to diagnostic ROM

$F8010C  CLR.B   $BFE001              ; CIA-A PRA: overlay off, LED on
$F80112  MOVE.B  #$03,$BFE201         ; CIA-A DDRA: OVL+LED as outputs
```

### Custom Chip Reset ($F8011A)

```asm
$F8011A  LEA     $DFF000,A4           ; Custom chip base
$F80120  MOVE.W  #$7FFF,D0
$F80124  MOVE.W  D0,$9A(A4)           ; INTENA = $7FFF (disable all)
$F80128  MOVE.W  D0,$9C(A4)           ; INTREQ = $7FFF (clear all)
$F8012C  MOVE.W  D0,$96(A4)           ; DMACON = $7FFF (disable all)
$F80130  MOVE.W  #$0174,$32(A4)       ; DSKLEN = $0174 (write twice to start; here just sets length)
$F80136  MOVE.W  #$0200,$100(A4)      ; BPLCON0 = $0200 (blank, colour)
$F8013C  MOVE.W  #$0000,$110(A4)      ; BPLCON1 = $0000 (no scroll)
$F80142  MOVE.W  #$0444,$180(A4)      ; COLOR00 = $0444 (dark grey)
```

**New vs KS 1.3:** Register $32 (DSKLEN) is written during reset. KS 1.3 skips
this. Also note DSKLEN value $0174 vs $0444 for COLOR00.

### ROM Checksum Verification ($F80148)

```asm
$F80148  MOVE.W  #$0F00,D0            ; Error code for checksum failure
$F8014C  NOT.L   D5                   ; Complement checksum
$F8014E  BNE.W   $F803B6              ; Non-zero → checksum BAD → alert
```

If the checksum fails, the ROM jumps to the alert handler with D0=$0F00 —
this produces a red screen flash.

### Exception Vector Setup ($F80152)

```asm
$F80152  MOVEA.W #$0008,A0            ; Start at vector 2 (bus error)
$F80156  MOVE.W  #$002D,D1            ; 46 vectors ($008–$0BC)
$F8015A  LEA     (PC),$F8039E,A1      ; Alert handler address
$F8015E  MOVE.L  A1,(A0)+             ; Fill vector
$F80160  DBF     D1,$F8015E

; Verify vectors wrote correctly
$F80164  MOVE.W  #$00F0,D0            ; Error code for vector verify fail
$F80168  MOVE.W  #$002D,D1
$F8016C  CMPA.L  -(A0),A1             ; Read back and compare
$F8016E  BNE.W   $F803B6              ; Mismatch → alert (chip RAM dead)
$F80172  DBF     D1,$F8016C
```

**New vs KS 1.3:** After writing vectors, KS 2.04 reads them back to verify
chip RAM is working. If any vector doesn't match, it's a fatal error — chip RAM
is not responding.

### Warm/Cold Start Detection ($F80176)

```asm
$F80176  MOVEQ   #0,D2                ; Clear warm-start preserved regs
$F80178  MOVEQ   #0,D3
$F8017A  MOVEQ   #0,D4
$F8017C  MOVEQ   #0,D5
$F8017E  MOVEQ   #0,D6
$F80180  MOVEQ   #0,D7
$F80182  MOVE.L  $0004,D1             ; ExecBase pointer
$F80186  MOVEA.L D1,A6                ; → A6
$F80188  BTST    #0,D1                ; Odd address?
$F8018C  BNE.S   $F801C4              ; Yes → cold start

; ExecBase complement checksum
$F8018E  ADD.L   38(A6),D1            ; Add ChkBase
$F80192  NOT.L   D1
$F80194  BNE.S   $F801C6              ; Not $FFFFFFFF → cold start

; ExecBase header checksum
$F80196  LEA     34(A6),A0
$F8019A  MOVEQ   #24,D0               ; 25 words
$F8019C  ADD.W   (A0)+,D1
$F8019E  DBF     D0,$F8019C
$F801A2  NOT.W   D1
$F801A4  BNE.S   $F801C6              ; Checksum fail → cold start

; ColdCapture check
$F801A6  MOVE.L  42(A6),D0            ; ColdCapture pointer
$F801AA  BEQ.S   $F801B8              ; None → continue warm start
$F801AC  MOVEA.L D0,A0
$F801AE  LEA     (PC),$F801B8,A5      ; Return address
$F801B2  CLR.L   42(A6)               ; Clear ColdCapture (one-shot)
$F801B6  JMP     (A0)                 ; Execute ColdCapture

; Recover KickMem/KickTag
$F801B8  MOVEM.L 546(A6),D2-D4       ; KickMemPtr, KickTagPtr, KickCheckSum
$F801BE  MOVEM.L 42(A6),D5-D7        ; ColdCapture, CoolCapture, WarmCapture
```

Then the code branches to the memory detection and ExecBase init subroutine:

```asm
$F801C4  SUBA.L  A6,A6                ; Clear A6 (no valid ExecBase)
$F801C6  MOVEA.L A6,A5                ; Save ExecBase state
$F801C8  BSR.W   $F80B30              ; Main init routine
```

### Stages 4–12

Follow [boot-flow-overview.md](boot-flow-overview.md). No A3000 hardware probing.

## Resident Modules

40 modules:

| Pri | Name | Type | Version | Init |
|-----|------|------|---------|------|
| +110 | expansion.library | LIBRARY | 37 | $F83CF0 |
| +105 | exec.library | LIBRARY | 37 | $F80420 |
| +105 | diag init | — | 37 | $F83FD4 |
| +103 | utility.library | LIBRARY | 37 | $FD3FB4 |
| +100 | potgo.resource | RESOURCE | 37 | $FD3E74 |
| +80 | cia.resource | RESOURCE | 37 | $F88904 |
| +80 | FileSystem.resource | RESOURCE | 37 | $F98FE0 |
| +70 | disk.resource | RESOURCE | 37 | $F8F4E0 |
| +70 | misc.resource | RESOURCE | 37 | $FC76FC |
| +65 | graphics.library | LIBRARY | 37 | $F9F4B2 |
| +60 | gameport.device | DEVICE | 37 | $FC832E |
| +50 | timer.device | DEVICE | 37 | $FCEE54 |
| +45 | battclock.resource | RESOURCE | 37 | $F85804 |
| +45 | keyboard.device | DEVICE | 37 | $FC8358 |
| +44 | battmem.resource | RESOURCE | 37 | $F86218 |
| +40 | keymap.library | LIBRARY | 37 | $F9E7EE |
| +40 | input.device | DEVICE | 37 | $FC8382 |
| +31 | layers.library | LIBRARY | 37 | $FC25B0 |
| +25 | ramdrive.device | DEVICE | 37 | $FC7786 |
| +20 | trackdisk.device | DEVICE | 37 | $FCFF6A |
| +10 | intuition.library | LIBRARY | 37 | $FD49BA |
| +5 | alert.hook | — | 37 | $F83AAA |
| +5 | console.device | DEVICE | 37 | $F8B422 |
| 0 | mathieeesingbas.library | LIBRARY | 37 | $FC6516 |
| −35 | syscheck | — | 37 | $F87AA8 |
| −40 | romboot | — | 37 | $FCDF18 |
| −50 | bootmenu | — | 37 | $F8640C |
| −60 | strap | — | 37 | $FCE140 |
| −81 | filesystem | — | 37 | $F990F0 |
| −100 | ramlib | — | 37 | $FC7E8E |
| −120 | audio.device | DEVICE | 37 | $F846E8 |
| −120 | dos.library | LIBRARY | 37 | $F904C0 |
| −120 | workbench.task | TASK | 37 | $F9F3B0 |
| −120 | gadtools.library | LIBRARY | 37 | $FBD784 |
| −120 | icon.library | LIBRARY | 37 | $FC0048 |
| −120 | mathffp.library | LIBRARY | 37 | $FC600C |
| −120 | workbench.library | LIBRARY | 37 | $FED832 |
| −121 | con-handler | — | 37 | $F89828 |
| −122 | shell | — | 37 | $FC99D0 |
| −123 | ram-handler | — | 37 | $FD190C |

### Changes from KS 2.02

- No scsi.device (A500+ has no SCSI)
- No A3000-specific diag/RAMSEY init
- battclock.resource type changed from DEVICE to RESOURCE
- All modules bumped to version 37
- workbench.task id string changed to "Pre-2.0 LoadWB stub"

## STRAP Display

Hires 3-plane insert-disk screen with rainbow gradient checkmark:
- BPLCON0 = $8302 (3 planes, hires, colour)
- DMACON set bits include $0180 (BPLEN + COPEN)
- This is the first Kickstart with the hires STRAP display

The boot test asserts these values — see `boot_ecs.rs:test_boot_kick204_a500plus`.

## Emulator Implications

- **No 68030 requirements** — the A500+ runs a 68000. No MOVEC, PMOVE, or
  cache instructions.
- **ECS Denise** — DENISEID ($DFF07C) must return $FC for ECS.
- **Chip RAM verify** — the exception vector write-back check at $F80164 catches
  dead or slow chip RAM. Every vector must read back correctly within the same
  instruction sequence.
- **ROM checksum** — ones' complement sum must be $FFFFFFFF.
- **CIA-A OVL** — overlay clear uses CLR.B $BFE001 (clears ALL bits, not just
  bit 0). This differs from KS 1.3 which writes $02 (preserving LED state).
