# Kickstart 3.0 (39.106) — A1200, A4000

## ROM Identification

| Property | Build | File | exec | Target |
|----------|-------|------|------|--------|
| KS 3.0 | 39.106 | `kick30_39_106_a1200.rom` | exec 39.47 (28.8.92) | A1200 |
| KS 3.0 | 39.106 | `kick30_39_106_a4000.rom` | exec 39.47 (28.8.92) | A4000 |
| KS 3.0 beta | 39.092 | `kick30_39_092_a600_beta.rom` | — | A600 (MapROM) |

All ROMs: 512K, SSP=$11144EF9, mapped at $F80000–$FFFFFF.

The A1200 and A4000 variants share the same exec version but differ
substantially (~32K hex lines). The A600 beta is a MapROM build
(PC=$002000D2) — see [Anomalous ROMs](#anomalous-a600-beta) below.

## Boot Flow (A1200)

The A1200 KS 3.0 boot flow is nearly identical to KS 3.1 (which was derived
from it). See [ks-3.1.md](ks-3.1.md) for the detailed AGA reference.

### Entry Point ($F800D2)

```asm
$F800D2  LEA     $0400,SP              ; Temporary stack
$F800D6  LEA     $F80000,A0            ; ROM base
$F800DC  MOVEQ   #-1,D1               ; Inner counter
$F800DE  MOVEQ   #1,D2                ; Outer counter
$F800E0  MOVEQ   #0,D5                ; Running checksum
$F800E2  ADD.L   (A0)+,D5             ; Sum
$F800E4  BCC.S   $F800E8
$F800E6  ADDQ.L  #1,D5               ; Carry
$F800E8  DBF     D1,$F800E2
$F800EC  DBF     D2,$F800E2
```

### PCMCIA Probe ($F8010C)

Same PCMCIA probe as KS 2.05 — checks for CIS bytes $91, $05, $23 at $A00000.
Present in the A1200 build (which has a PCMCIA slot).

### CIA/Custom Reset ($F8014A)

```asm
$F8014A  CLR.B   $BFE001              ; CIA-A: overlay off
$F80150  MOVE.B  #$03,$BFE201         ; DDRA outputs
$F80158  LEA     $DFF000,A4
$F8015E  MOVE.W  #$7FFF,D0
$F80162  MOVE.W  D0,$9A(A4)           ; INTENA disable all
$F80166  MOVE.W  D0,$9C(A4)           ; INTREQ clear all
$F8016A  MOVE.W  D0,$96(A4)           ; DMACON disable all
$F8016E  MOVE.W  #$0174,$32(A4)       ; DSKLEN
$F80174  MOVE.W  #$0200,$100(A4)      ; BPLCON0 blank
$F8017A  MOVE.W  #$0000,$110(A4)      ; BPLCON1 no scroll
$F80180  MOVE.W  #$0111,$180(A4)      ; COLOR00 = $0111 (very dark grey)
```

**Difference from KS 2.04/2.05:** COLOR00 = $0111 (almost black) instead of
$0444 (dark grey). This is the first KS to use the near-black reset colour.

### ROM Checksum and Vector Setup ($F80186+)

Same as KS 2.04/2.05 — checksum verify, exception vector setup with read-back
check, warm/cold start detection.

## A4000 Variant Differences

The A4000 KS 3.0 entry code differs from the A1200:

1. **No PCMCIA probe** — the A4000 has no PCMCIA slot. After the diagnostic ROM
   check, it goes straight to CIA/custom reset.

2. **RAMSEY/Fat Gary probe** — added after custom chip reset:
```asm
; A4000 specific:
$F8xxxx  MOVEQ   #0,D0
$F8xxxx  MOVE.B  D0,$DE0000            ; RAMSEY config 0
$F8xxxx  MOVE.B  D0,$DE0001            ; RAMSEY config 1
$F8xxxx  MOVEQ   #9,D0
$F8xxxx  MOVEC   D0,CACR               ; Enable instruction cache
$F8xxxx  MOVEC   CACR,D0               ; Read back
$F8xxxx  TST.L   D0
$F8xxxx  BEQ.S   skip                  ; 68000 → skip
$F8xxxx  BCLR    #7,$DE0002            ; RAMSEY control test
$F8xxxx  BEQ.S   skip                  ; No RAMSEY
$F8xxxx  CLR.L   $0004                 ; Clear ExecBase
$F8xxxx  CLR.L   $0000                 ; Clear bus error vector
```

3. **68040 detection** — the A4000 may have a 68040, so the entry code must
   handle 68040-specific cache invalidation.

## Resident Modules (A1200)

44 modules:

| Pri | Name | Type | Version | Init |
|-----|------|------|---------|------|
| +110 | expansion.library | LIBRARY | 39 | $F837D8 |
| +105 | exec.library | LIBRARY | 39 | $F80444 |
| +105 | diag init | — | 39 | $F83AB8 |
| +103 | utility.library | LIBRARY | 39 | $F85A78 |
| +100 | potgo.resource | RESOURCE | 37 | $FCD39C |
| +80 | cia.resource | RESOURCE | 39 | $F8E9A4 |
| +80 | FileSystem.resource | RESOURCE | 39 | $F9EF64 |
| +70 | battclock.resource | RESOURCE | 39 | $F8C68A |
| +70 | disk.resource | RESOURCE | 37 | $F95190 |
| +70 | misc.resource | RESOURCE | 37 | $FC3EA8 |
| +69 | battmem.resource | RESOURCE | 39 | $F8CFEC |
| +65 | graphics.library | LIBRARY | 39 | $FA4FBE |
| +64 | layers.library | LIBRARY | 39 | $FC0C9E |
| +60 | gameport.device | DEVICE | 37 | $FC6812 |
| +50 | timer.device | DEVICE | 39 | $F841F2 |
| +48 | card.resource | RESOURCE | 37 | $F8AAF0 |
| +45 | keyboard.device | DEVICE | 37 | $FC683C |
| +40 | keymap.library | LIBRARY | 37 | $FC00DE |
| +40 | input.device | DEVICE | 37 | $FC6866 |
| +25 | ramdrive.device | DEVICE | 39 | $F8545A |
| +20 | trackdisk.device | DEVICE | 39 | $FCD7FA |
| +15 | carddisk.device | DEVICE | 37 | $F8A1D4 |
| +10 | scsi.device | DEVICE | 37 | $F877C2 |
| +10 | intuition.library | LIBRARY | 39 | $FCF1F2 |
| +5 | console.device | DEVICE | 39 | $F914DE |
| 0 | mathieeesingbas.library | LIBRARY | 37 | $F8659A |
| −35 | syscheck | — | 39 | $F8E310 |
| −40 | romboot | — | 39 | $FCC498 |
| −50 | bootmenu | — | 39 | $F8E398 |
| −55 | alert.hook | — | 39 | $F83138 |
| −60 | strap | — | 39 | $FCC6B8 |
| −81 | filesystem | — | 39 | $F9F074 |
| −100 | ramlib | — | 39 | $FC63CA |
| −120 | mathffp.library | LIBRARY | 39 | $F84FA4 |
| −120 | audio.device | DEVICE | 37 | $F8B59C |
| −120 | dos.library | LIBRARY | 39 | $F96178 |
| −120 | icon.library | LIBRARY | 39 | $FBDB94 |
| −120 | gadtools.library | LIBRARY | 39 | $FE89BA |
| −120 | workbench.library | LIBRARY | 39 | $FEE7D6 |
| −120 | workbench.task | TASK | 39 | $FFFDAC |
| −121 | con-handler | — | 39 | $F8F928 |
| −122 | shell | — | 39 | $FC7EB4 |
| −123 | ram-handler | — | 39 | $FC3F18 |

### Changes from KS 2.05

- **alert.hook** moved from priority +5 to −55 (initialises later)
- **battclock.resource** priority changed from +45 to +70
- **battmem.resource** priority changed from +44 to +69
- **layers.library** priority changed from +31 to +64
- Many modules bumped to version 39
- Some devices remain at version 37 (reused from KS 2.05 code)

## Anomalous A600 Beta

`kick30_39_092_a600_beta.rom` has PC=$002000D2 — a MapROM build that expects
the ROM image at $200000 instead of $F80000. This was built for A600 units
with MapROM adapters during development. It won't boot in a standard emulator
configuration. Not traced in detail.

## STRAP Display

Hires 3-plane insert-disk screen (A1200 AGA):
- BPLCON0 = $8303 (HIRES + 3 planes + COLOR + ERSY)
- DMACON set bits = $03C0 (BPLEN + COPEN + BLTEN + SPREN)

Boot test (`boot_aga.rs`) asserts DMACON $03C0 and BPLCON0 $8303 for KS 3.0 A1200.

## Emulator Implications

- **AGA registers** — graphics.library v39 detects AGA via DENISEID ($DFF07C).
  Must return $F8 for Lisa (AGA Denise). Falls back to ECS ($FC) or OCS ($FF).
- **PCMCIA at $A00000** — A1200 only. Must return $FF (no card) for clean boot.
- **RAMSEY at $DE0000** — A4000 only.
- **68020/030/040** — the CPU detection code must correctly identify the
  installed processor. The A1200 has a 68EC020; the A4000 has a 68030 or 68040.
- This is the first AGA Kickstart. If AGA is not correctly detected, the STRAP
  display falls back to OCS/ECS mode, which may produce unexpected colours or
  resolution.
