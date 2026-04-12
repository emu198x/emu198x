# Kickstart 1.3 Boot Flow

Kickstart 1.3 (34.005) is the last 256K ROM and the most widely deployed
Kickstart for the A500/A2000. Two variants exist: one for the A500/A1000/A2000
family, and one for the A3000 (which differs by only 3 bytes). The boot flow
follows the stages defined in [boot-flow-overview.md](boot-flow-overview.md).
This document is the primary reference -- other version documents reference this
one for shared structure.

## ROM Identification

| Field | A500/A1000/A2000/CDTV | A3000 |
|-------|----------------------|-------|
| Files | `kick13.rom`, `kick13_34_005_a500_a1000_a2000_cdtv.rom` | `kick13_34_005_a3000.rom` |
| Size | 256 KB (262,144 bytes) | 256 KB (262,144 bytes) |
| Mapped at | $FC0000--$FFFFFF | $FC0000--$FFFFFF |
| SSP | $11114EF9 | $11114EF9 |
| PC | $FC00D2 | $FC00D2 |
| exec version | exec 34.2 (28 Oct 1987) | exec 34.2 (28 Oct 1987) |
| Version word | $0022 (34) at offset $10 | $0022 (34) at offset $10 |
| Revision word | $0005 (5) at offset $12 | $0005 (5) at offset $12 |
| Target machines | A500, A1000, A2000, CDTV | A3000 |

The ROM header at offset $18 contains the ASCII identification string
`exec 34.2 (28 Oct 1987)\r\n`. At offset $3A: `\r\n\nAMIGA`.

The ROM footer at offset $3FFEC holds the ROM size as a big-endian longword:
$00040000 (262,144 bytes). The word at $3FFE8 is the ROM checksum -- see
[A3000 Variant Differences](#a3000-variant-differences) for values.

**Key differences from KS 1.2:**
- Modules built from a newer codebase (dated Aug--Dec 1987 vs Sep--Oct 1986)
- intuition.library version 34.3, exec version 34.2 (vs 33.x in KS 1.2)
- Identical entry point and boot flow structure
- Same number of resident modules (24)

## Boot Flow

### Stage 1: Reset Vector Fetch

The 68000 reads SSP from $000000 and PC from $000004. The overlay latch maps
ROM at $000000 on reset, so these reads come from the first 8 bytes of the ROM
image at $FC0000.

SSP = $11114EF9 (unmapped -- immediately overwritten). PC = $FC00D2.

### Stage 2: Initial Setup ($FC00D2--$FC0148)

#### Temporary stack and delay loop

```
$FC00D2  LEA     $40000,SP          ; SSP at 256K (top of minimum chip RAM)
$FC00D8  MOVE.L  #$20000,D0         ; 131,072 iterations
$FC00DE  SUBQ.L  #1,D0              ; Busy-wait loop for hardware settle
$FC00E0  BGT.S   $FC00DE            ; ~393K cycles at 7.09 MHz = ~55 ms
```

The delay gives address decode logic (Gary, Gayle, RAMSEY) time to stabilise
after power-on. On the A3000, the RAMSEY memory controller needs this settling
time before memory operations are reliable.

**Key differences from later KS:** KS 2.0+ removes this explicit delay loop
and uses the ROM checksum verification loop (which KS 1.3 lacks) for the same
purpose.

> **Emulator note:** The delay is a tight CPU loop. No hardware interaction
> occurs. The emulator ticks through it.

#### Diagnostic ROM check

```
$FC00E2  LEA     (PC,-$D2),A0       ; A0 = ROM base ($FC0000) via PC-relative
$FC00E6  LEA     $F00000,A1         ; Diagnostic ROM address
$FC00EC  CMPA.L  A1,A0              ; Are we running from $F00000 already?
$FC00EE  BEQ.S   $FC00FE            ; Yes -- skip (we ARE the diag ROM)
$FC00F0  LEA     (PC,$FC00FE),A5    ; Return address if no diagnostic ROM
$FC00F4  CMPI.W  #$1111,(A1)        ; Magic word at $F00000?
$FC00F8  BNE.S   $FC00FE            ; No -- continue normal boot
$FC00FA  JMP     2(A1)              ; Jump to diagnostic ROM at $F00002
```

This supports the A3000 diagnostic ROM socket and the A1000 WCS (Writable
Control Store). If a ROM is present at $F00000 with the magic word $1111 in the
first two bytes, the Kickstart hands off control to it. A5 holds the return
address so the diagnostic code can return to normal boot if desired.

> **Emulator note:** $F00000 is in the diagnostic ROM range. On the A3000, Fat
> Gary decodes this address. If no diagnostic ROM is installed, the read must
> return a value other than $1111 (typically $0000 from unmapped space). If the
> emulator returns $1111 by accident, boot diverts to $F00002 and crashes.

#### CIA-A setup (overlay clear, LED off)

```
$FC00FE  MOVE.B  #$03,$BFE201       ; CIA-A DDRA: bits 0,1 as outputs
$FC0106  MOVE.B  #$02,$BFE001       ; CIA-A PRA: OVL=0 (clear), LED=1 (off)
```

This clears the overlay latch, making chip RAM visible at $000000. The power
LED is turned off (active-low -- writing 1 turns it off).

> **Emulator note:** The overlay must deactivate immediately on the CIA-A PRA
> write. After this instruction, reads from $000000 must come from chip RAM (all
> zeros after power-on), not ROM. If the overlay stays active, the exception
> vector writes at $FC0136 go to ROM (silently dropped) and any subsequent
> exception jumps to garbage addresses.

#### Custom chip reset

```
$FC010E  LEA     $DFF000,A4         ; Custom chip base in A4 (used throughout)
$FC0114  MOVE.W  #$7FFF,D0          ; All bits except SET
$FC0118  MOVE.W  D0,$9A(A4)         ; INTENA ($DFF09A): disable all interrupts
$FC011C  MOVE.W  D0,$9C(A4)         ; INTREQ ($DFF09C): clear all requests
$FC0120  MOVE.W  D0,$96(A4)         ; DMACON ($DFF096): disable all DMA
$FC0124  MOVE.W  #$0200,$100(A4)    ; BPLCON0 ($DFF100): colour, no bitplanes
$FC012A  MOVE.W  #$0000,$110(A4)    ; BPLCON1 ($DFF110): no scroll offset
$FC0130  MOVE.W  #$0444,$180(A4)    ; COLOR00 ($DFF180): dark grey background
```

All DMA channels stop, all interrupt sources are disabled and cleared, and the
display is set to a plain dark grey screen. The $0200 in BPLCON0 sets colour
mode with zero bitplanes -- no display data, just the background colour.

Register summary:

| Register | Address | Value | Purpose |
|----------|---------|-------|---------|
| INTENA | $DFF09A | $7FFF | Disable all interrupt sources |
| INTREQ | $DFF09C | $7FFF | Clear all pending interrupts |
| DMACON | $DFF096 | $7FFF | Disable all DMA channels |
| BPLCON0 | $DFF100 | $0200 | Blank display, colour mode on |
| BPLCON1 | $DFF110 | $0000 | No horizontal scroll |
| COLOR00 | $DFF180 | $0444 | Background = dark grey |

> **Emulator note:** DMACON=$7FFF must disable all DMA within one bus cycle. If
> the copper or blitter is running from a previous session (warm start), it must
> stop immediately. INTENA and INTREQ writes must also take effect immediately.

#### Exception vector setup

```
$FC0136  MOVEA.W #$0008,A0          ; Start at vector 2 (bus error)
$FC013A  MOVE.W  #$002D,D1          ; 46 vectors ($008 through $0BC)
$FC013E  LEA     (PC,$FC05B4),A1    ; Alert/trap handler
$FC0142  MOVE.L  A1,(A0)+           ; Store vector
$FC0144  DBF     D1,$FC0142         ; Loop for all 46 vectors
```

All exception vectors from $008 (bus error) through $0BC are pointed at the
early alert handler at $FC05B4. This catches any unexpected exception during
boot and displays a flashing screen instead of crashing silently.

#### Branch to warm/cold start

```
$FC0148  BRA.W   $FC3100            ; Jump to warm/cold start detection
```

### Stage 2b: Warm/Cold Start Detection ($FC3100, $FC014C--$FC01CC)

The warm start check is a two-part sequence. First, the code at $FC3100 checks
for an A1000 "HELP" magic word, then branches back to $FC014C for ExecBase
validation.

#### "HELP" magic check ($FC3100)

```
$FC3100  MOVE.L  #$FFFFFFFF,D6      ; D6 = "no alert data" marker
$FC3106  CMPI.L  #'HELP',$000000    ; A1000 WCS warm start magic?
$FC310E  BNE.W   $FC014C            ; Not found -- normal warm/cold check
$FC3112  CLR.L   $000000            ; Clear magic (one-shot, prevents loops)
$FC3116  MOVEM.L $000100,D6-D7      ; Load alert/debug data from $100
$FC311C  BRA.W   $FC014C            ; Continue to warm/cold check
```

The "HELP" magic is an A1000 mechanism. The WCS writes "HELP" ($48454C50) at
$000000 and stores the alert code in D6-D7 at $000100 before triggering a
reset. This lets the ROM display a guru meditation across a reset. On
A500/A2000, chip RAM is zeros at power-on, so this check always fails and falls
through.

#### ExecBase validation ($FC014C)

```
$FC014C  MOVE.L  $000004,D0         ; Read ExecBase pointer
$FC0150  BTST    #0,D0              ; Word-aligned?
$FC0154  BNE.S   $FC01CE            ; Odd address -- cold start
$FC0156  MOVEA.L D0,A6              ; A6 = ExecBase
$FC0158  ADD.L   ($26,A6),D0        ; D0 + ExecBase->ChkBase
$FC015C  NOT.L   D0                 ; Complement -- should be 0 if valid
$FC015E  BNE.S   $FC01CE            ; Failed -- cold start
```

If the address is word-aligned and the complement checksum passes, a second
checksum verifies the ExecBase header:

```
$FC0160  MOVEQ   #0,D1              ; Accumulator
$FC0162  LEA     ($22,A6),A0        ; Start at ExecBase+$22
$FC0166  MOVEQ   #24,D0             ; 25 words ($22--$52)
$FC0168  ADD.W   (A0)+,D1           ; Sum words
$FC016A  DBF     D0,$FC0168         ; Loop
$FC016E  NOT.W   D1                 ; Complement
$FC0170  BNE.S   $FC01CE            ; Non-zero -- cold start
```

If both checksums pass, the ROM processes ColdCapture:

```
$FC0172  MOVE.L  ($2A,A6),D0        ; ExecBase->ColdCapture
$FC0176  BEQ.S   $FC0184            ; Zero -- skip
$FC0178  MOVEA.L D0,A0              ; Load capture address
$FC017A  LEA     (PC,$FC0184),A5    ; Return address
$FC017E  CLR.L   ($2A,A6)           ; Clear ColdCapture (one-shot)
$FC0182  JMP     (A0)               ; Execute ColdCapture routine
```

Then the ROM version and memory configuration are validated:

```
$FC0184  BCHG    #1,$BFE001         ; Toggle LED (visual heartbeat)
$FC018C  MOVE.L  (PC,$FC0010),D0    ; ROM version word ($0022 = 34)
$FC0190  CMP.L   ($14,A6),D0        ; Match ExecBase->SoftVer?
$FC0194  BNE.S   $FC01CE            ; Version mismatch -- cold start
$FC0196  MOVEA.L ($3E,A6),A3        ; ExecBase->MaxLocMem (chip RAM size)
$FC019A  CMPA.L  #$00080000,A3      ; <= 512K? [A3000: #$00200000, <= 2MB]
$FC01A0  BHI.S   $FC01CE            ; Too large -- cold start
$FC01A2  CMPA.L  #$00040000,A3      ; >= 256K?
$FC01A8  BCS.S   $FC01CE            ; Too small -- cold start
```

Slow RAM validation follows if ExecBase->MaxExtMem is non-zero:

```
$FC01AA  MOVEA.L ($4E,A6),A4        ; ExecBase->MaxExtMem (slow RAM top)
$FC01AE  MOVE.L  A4,D0              ; Zero?
$FC01B0  BEQ.W   $FC0240            ; No slow RAM -- warm start OK
$FC01B4  CMPA.L  #$00DC0000,A4      ; <= $DC0000?
$FC01BA  BHI.S   $FC01CE            ; Out of range -- cold start
$FC01BC  CMPA.L  #$00C40000,A4      ; >= $C40000?
$FC01C2  BCS.S   $FC01CE            ; Out of range -- cold start
$FC01C4  MOVE.L  A4,D0              ; Alignment check
$FC01C6  ANDI.L  #$0003FFFF,D0      ; Must be 256K-aligned
$FC01CC  BEQ.S   $FC0240            ; Aligned -- warm start OK
```

If all checks pass, execution continues at $FC0240 (ExecBase init) with the
existing memory configuration. Otherwise, the cold start path at $FC01CE runs
fresh memory detection.

> **Emulator note:** On first power-on, chip RAM is all zeros. $000004 = 0
> fails validation (the complement check at ExecBase+$26 fails because address
> 0 is not a valid ExecBase). Cold start is always taken on first boot. After a
> soft reset (Ctrl-A-A), a valid ExecBase may survive in RAM, enabling the warm
> start path.

### Stage 3: Cold Start -- Memory Detection ($FC01CE--$FC023E)

#### Temporary ExecBase

```
$FC01CE  LEA     $0400,A6           ; A6 base at $400
$FC01D2  SUBA.W  #-630,A6           ; Add 630 -> A6 = $676 (temp ExecBase)
```

The negative `SUBA.W` adds 630 (two's complement of -630 = $FDA76, sign-
extended to $FFFFFFDA76... actually $FD8A sign-extended to $FFFFFD8A, then
subtracted, which adds $0276). This places the temporary ExecBase at $676,
safely within the first 2K of chip RAM.

#### Slow RAM detection ($FC01D6)

```
$FC01D6  LEA     $C00000,A0         ; Slow RAM start
$FC01DC  LEA     $DC0000,A1         ; Slow RAM end (max 1.75 MB)
$FC01E2  LEA     (PC,$FC01EA),A5    ; Return address (in A5)
$FC01E6  BRA.W   $FC061A            ; Jump to slow RAM probe
```

The slow RAM probe at $FC061A walks from A0 upward in 256K steps. At each step,
it uses the custom chip INTENA register as a side-effect test: writing to the
INTENA-offset within the candidate address range should NOT affect the real
INTENA register at $DFF09A. If it does, the address aliases to the custom chip
space and is not real RAM.

```
$FC061A  MOVEA.L A0,A4              ; A4 = probe base (start of slow RAM)
$FC061C  ADDA.L  #$40000,A0         ; Skip first 256K (minimum region)
$FC0622  MOVEA.L A4,A2              ; A2 = base for custom chip test
$FC0624  ADDA.L  #$40000,A2         ; A2 = base + 256K
$FC062A  MOVE.W  #$3FFF,(-$F66,A2)  ; Write $3FFF to INTENA-offset in RAM
$FC0630  TST.W   (-$FE4,A2)         ; Read INTREQ-offset -- side effect?
$FC0634  BNE.S   $FC0644            ; Non-zero -- this is custom chip space
$FC0636  MOVE.W  #$BFFF,(-$F66,A2)  ; Write $BFFF (SET bit clear)
$FC063C  CMPI.W  #$3FFF,(-$FE4,A2)  ; Did previous write take?
$FC0642  BEQ.S   $FC067C            ; Yes -- still in custom chip space
```

If the address range passes the side-effect test, a pattern write/read confirms
that real memory exists:

```
$FC0644  MOVE.W  #0,(-$F66,A0)      ; Clear INTENA-offset at base
$FC064A  MOVE.L  #$0000F2D4,D1      ; Test pattern (partial)
$FC0650  MOVE.W  D1,(-$F66,A2)      ; Write pattern to current probe addr
$FC0654  CMP.W   (-$F66,A2),D1      ; Read back -- match?
$FC0658  BNE.S   $FC0682            ; No -- end of memory
$FC065A  CMP.W   (-$F66,A0),D1      ; Did base alias change?
$FC065E  BEQ.S   $FC0672            ; Same -- possible alias
$FC0660  MOVE.L  #$0000B698,D1      ; Second test pattern
$FC0666  MOVE.W  D1,(-$F66,A2)      ; Write second pattern
$FC066A  CMP.W   (-$F66,A2),D1      ; Read back -- match?
$FC066E  BNE.S   $FC0682            ; No -- end of memory
$FC0670  BRA.S   $FC0676            ; Confirmed -- real memory
$FC0672  CMPA.L  A0,A2              ; Was it an alias of the base?
$FC0674  BEQ.S   $FC0660            ; Try second pattern to distinguish
$FC0676  MOVEA.L A2,A4              ; Update slow RAM top
$FC0678  CMPA.L  A4,A1              ; Past max?
$FC067A  BHI.S   $FC0622            ; No -- probe next 256K block
$FC067C  MOVE.W  #$7FFF,(-$F66,A2)  ; Restore INTENA disable-all
$FC0682  SUBA.L  #$40000,A0         ; Back to base
$FC0688  CMPA.L  A0,A4              ; Any slow RAM found?
$FC068A  BNE.S   $FC068E            ; Yes
$FC068C  SUBA.L  A4,A4              ; A4 = 0 (no slow RAM)
$FC068E  JMP     (A5)               ; Return via A5
```

Returns with A4 = top of slow RAM, or A4 = 0 if none found.

> **Emulator note:** The INTENA side-effect test at offsets -$F66 and -$FE4 is
> relative to A2 (base + $40000). For $C00000 as the base, these map to
> $C0F09A (INTENA offset) and $C0F01C (INTREQ offset). The emulator must ensure
> that writes to $C0F09A do NOT reach the real INTENA at $DFF09A -- otherwise
> the ROM concludes that slow RAM isn't present because the write "leaked" to
> the custom chip registers.

#### Slow RAM clear and ExecBase relocation

```
$FC01EA  MOVE.L  A4,D0              ; Slow RAM top
$FC01EC  BEQ.S   $FC0208            ; None found -- skip to chip RAM
$FC01EE  MOVEA.L #$C00000,A6        ; Move ExecBase to slow RAM
$FC01F4  SUBA.W  #-630,A6           ; A6 = $C00276 (ExecBase in slow RAM)
$FC01F8  MOVE.L  A4,D0              ; Size for clear
$FC01FA  LEA     $C00000,A0         ; Start of slow RAM
$FC0200  LEA     (PC,$FC0208),A5    ; Return address
$FC0204  BRA.W   $FC0602            ; Clear slow RAM
```

If slow RAM is found, ExecBase moves to $C00276 (slow RAM base + 630). This
frees chip RAM for graphics allocations -- a critical optimisation on the A500
where 512K chip RAM is tight.

> **Emulator note:** KS 1.2+ A500/A2000 tests depend on 512K slow RAM at
> $C00000 (the A501 trapdoor expansion). Without it, ExecBase stays in chip
> RAM and the system may run out of chip memory during init (BSR/RTS to
> expansion space at $C00000 loses return addresses when no RAM is there).

#### Chip RAM detection ($FC0208)

```
$FC0208  LEA     $0,A0              ; Chip RAM start
$FC020C  LEA     $200000,A1         ; Max chip RAM (2 MB)
$FC0212  LEA     (PC,$FC021A),A5    ; Return address
$FC0216  BRA.W   $FC0592            ; Jump to chip RAM probe
```

The chip RAM probe at $FC0592 uses a pattern-and-alias technique:

```
$FC0592  MOVEQ   #0,D1              ; Clear value
$FC0594  MOVE.L  D1,(A0)            ; Write 0 to base ($000000)
$FC0596  MOVEA.L A0,A2              ; A2 = base for alias detection
$FC0598  MOVE.L  #$F2D4B698,D0      ; Magic test pattern
$FC059E  LEA     $1000(A0),A0       ; Advance 4K
$FC05A2  CMPA.L  A0,A1              ; Past max?
$FC05A4  BLS.S   $FC05B0            ; Yes -- done
$FC05A6  MOVE.L  D0,(A0)            ; Write pattern at current address
$FC05A8  TST.L   (A2)               ; Did base ($000000) change?
$FC05AA  BNE.S   $FC05B0            ; Yes -- alias detected (end of memory)
$FC05AC  CMP.L   (A0),D0            ; Read back -- match?
$FC05AE  BEQ.S   $FC059E            ; Yes -- real memory, try next 4K
$FC05B0  MOVEA.L A0,A3              ; A3 = chip RAM top
$FC05B2  JMP     (A5)               ; Return via A5
```

The magic pattern $F2D4B698 avoids collision with zeroed or uninitialised RAM.
The probe checks whether writing to address N causes the base ($000000) to
change -- if it does, N aliases to 0 and the physical RAM ends before N.

Returns with A3 = chip RAM size (e.g. $80000 = 512K, $100000 = 1M).

#### Post-sizing validation

```
$FC021A  CMPA.L  #$40000,A3         ; At least 256K?
$FC0220  BCS.S   $FC0238            ; Less than 256K -- fatal
$FC0222  MOVE.L  #0,$000000         ; Clear location 0
$FC022A  MOVE.L  A3,D0              ; Chip RAM size
$FC022C  LEA     $C0,A0             ; Start clearing at $C0 (after vectors)
$FC0230  LEA     (PC,$FC0240),A5    ; Continue to ExecBase init
$FC0234  BRA.W   $FC0602            ; Clear chip RAM from $C0 to top
```

If chip RAM is less than 256K:

```
$FC0238  MOVE.W  #$00C0,D0          ; Alert colour code
$FC023C  BRA.W   $FC05B8            ; -> alert handler -> reset loop
```

This produces a cycling reset -- the machine cannot proceed without 256K.

#### Memory clear routine ($FC0602)

Zeroes memory from A0 to A0+D0 in longword steps using a two-level loop:

```
$FC0602  MOVEQ   #0,D2              ; Clear value
$FC0604  SUB.L   A0,D0              ; D0 = byte count remaining
$FC0606  LSR.L   #2,D0              ; D0 = longword count
$FC0608  MOVE.L  D0,D1              ; Copy for outer loop
$FC060A  SWAP    D1                 ; D1 = high word (64K-longword blocks)
$FC060E  MOVE.L  D2,(A0)+           ; Clear one longword
$FC0610  DBF     D0,$FC060E         ; Inner loop (up to 64K longwords)
$FC0614  DBF     D1,$FC060E         ; Outer loop (remaining blocks)
$FC0618  JMP     (A5)               ; Return via A5
```

> **Emulator note:** Chip RAM must respond correctly to read/write at all
> addresses from $000000 to $1FFFFF. The alias detection relies on writes
> wrapping at the physical chip RAM boundary -- a 512K machine must wrap
> $080000 back to $000000. If the emulator doesn't implement aliasing, the ROM
> may detect the wrong chip RAM size.

### Stage 4: ExecBase Initialisation ($FC0240--$FC03CA)

After memory detection (cold start) or warm start validation, execution reaches
$FC0240.

#### Custom chip safe state

```
$FC0240  LEA     $DFF000,A0
$FC0246  MOVE.W  #$7FFF,($96,A0)    ; DMACON: disable all DMA (again)
$FC024C  MOVE.W  #$0200,($100,A0)   ; BPLCON0: blank display
$FC0252  MOVE.W  #$0000,($110,A0)   ; BPLCON1: no scroll
$FC0258  MOVE.W  #$0888,($180,A0)   ; COLOR00: medium grey
```

The second DMACON disable is redundant on cold start but necessary on warm
start, where Stage 2's register writes may not have executed if ColdCapture
diverted control. The background colour changes from $0444 (dark grey) to $0888
(medium grey) as a visual progress indicator.

#### Clear ExecBase fields

```
$FC025E  LEA     ($54,A6),A0        ; ExecBase+$54
$FC0262  MOVEM.L ($222,A6),D2-D4    ; Save KickMem/KickTag/KickCheckSum
$FC0268  MOVEQ   #0,D0
$FC026A  MOVE.W  #$007D,D1          ; 126 longwords = 504 bytes
$FC026E  MOVE.L  D0,(A0)+           ; Clear
$FC0270  DBF     D1,$FC026E         ; Loop ($054--$24C cleared)
$FC0274  MOVEM.L D2-D4,($222,A6)    ; Restore saved values
```

The KickMem, KickTag, and KickCheckSum pointers are preserved across the clear
so that warm-start resident modules loaded into RAM survive.

#### Store ExecBase and set up stack

```
$FC027A  MOVE.L  A6,$000004         ; Store ExecBase pointer at $4
$FC027E  MOVE.L  A6,D0
$FC0280  NOT.L   D0                 ; Complement
$FC0282  MOVE.L  D0,($26,A6)        ; ExecBase->ChkBase = ~ExecBase
$FC0286  MOVE.L  A4,D0              ; Slow RAM top (or 0)
$FC0288  BNE.S   $FC028C            ; Found slow RAM?
$FC028A  MOVE.L  A3,D0              ; No -- use chip RAM top
$FC028C  MOVEA.L D0,SP              ; Set supervisor stack pointer
$FC028E  MOVE.L  D0,($36,A6)        ; ExecBase->SysStkUpper
$FC0292  SUBI.L  #$1800,D0          ; Reserve 6K for supervisor stack
$FC0298  MOVE.L  D0,($3A,A6)        ; ExecBase->SysStkLower
$FC029C  MOVE.L  A3,($3E,A6)        ; ExecBase->MaxLocMem = chip RAM top
$FC02A0  MOVE.L  A4,($4E,A6)        ; ExecBase->MaxExtMem = slow RAM top
```

The supervisor stack is placed at the top of the highest available memory. On an
A500 with slow RAM, this means SSP is at the top of $C80000 (or wherever slow
RAM ends), keeping chip RAM free for DMA-accessible allocations.

#### Internal init and hardware detection

```
$FC02A4  BSR.W   $FC3120            ; Store alert data (D6-D7) in ExecBase
$FC02A8  BSR.W   $FC0546            ; CPU/FPU detection
$FC02AC  OR.W    D0,($128,A6)       ; Merge detected flags into AttnFlags
```

The subroutine at $FC3120 saves the alert recovery data:

```
$FC3120  MOVEM.L D6-D7,($202,A6)    ; Store D6-D7 at ExecBase+$202
$FC3126  RTS
```

#### Exec list initialisation ($FC02B0--$FC033C)

The code reads a table of (ExecBase-offset, list-type) pairs at $FC02D2 and
builds linked list headers at each offset. The `NewList` pattern initialises
each list as empty (head points to tail sentinel, tail is NULL, tailpred points
to head):

```
$FC02B0  LEA     (PC,$FC02D2),A1    ; List init table
$FC02B4  MOVE.W  (A1)+,D0           ; Offset into ExecBase
$FC02B6  BEQ.W   $FC033E            ; Zero = end of table
$FC02BA  LEA     (A6,D0.W),A0       ; A0 = list header address
$FC02BE  MOVE.L  A0,(A0)            ; lh_Head = &lh_Head
$FC02C0  ADDQ.L  #4,(A0)            ; lh_Head = &lh_Tail
$FC02C2  CLR.L   4(A0)              ; lh_Tail = NULL
$FC02C6  MOVE.L  A0,8(A0)           ; lh_TailPred = &lh_Head
$FC02CA  MOVE.W  (A1)+,D0           ; List type byte
$FC02CC  MOVE.B  D0,$C(A0)          ; lh_Type
$FC02D0  BRA.S   $FC02B4            ; Next entry
```

The init table creates these exec lists:

| ExecBase offset | List name | Type |
|----------------|-----------|------|
| $0142 | MemList | NT_MEMORY (10) |
| $0150 | ResourceList | NT_RESOURCE (8) |
| $015E | DeviceList | NT_DEVICE (3) |
| $017A | LibList | NT_LIBRARY (9) |
| $0188 | PortList (internal) | 4 |
| $0196 | TaskReady | NT_TASK (1) |
| $01A4 | IntrList level 1 | NT_INTERRUPT (1) |
| $01B2 | IntrList level 2 | NT_INTERRUPT (1) |
| $01C2--$01F2 | SoftInt lists (5 levels) | NT_SEMAPHORE (11) |
| $016C | MsgPortList | NT_MSGPORT (2) |

#### Exec library identification ($FC033E--$FC03CA)

After the lists, exec sets up its own library identity and memory:

```
$FC033E  LEA     (PC,$FC2FF0),A0    ; Exec function vector table
$FC0342  MOVE.L  A0,($130,A6)       ; ExecBase->ResModules
$FC0346  MOVE.L  A0,($134,A6)       ; (duplicate)
$FC034A  MOVE.L  #$FC1D28,($138,A6) ; Interrupt dispatch routine
$FC0352  MOVE.L  #$0000FFFF,($13C,A6) ; Version mask
$FC035A  MOVE.W  #$8000,($140,A6)   ; Coldstart flag
```

The code at $FC0364 builds the exec jump table from function vector offsets,
and stores the exec library node fields:

```
; ExecBase library node:
;   ln_Name    -> "exec.library" at $FC00A8
;   ln_Type    = 9 (NT_LIBRARY)
;   ln_Version = 34
;   ln_IdString -> "exec 34.2 (28 Oct 1987)" at $FC0018
```

Two memory entries are added to the MemList:

| Name | Start | End | Type |
|------|-------|-----|------|
| "Chip Memory" | $000000 | MaxLocMem | MEMF_CHIP |
| "Fast Memory" | $C00000 | MaxExtMem | MEMF_FAST (if slow RAM present) |

### Stage 5: CPU and FPU Detection ($FC0546--$FC0590)

The detection uses deliberate illegal instruction exceptions. The ROM saves the
current exception vectors for illegal instruction ($10) and F-line ($2C),
installs a skip-ahead trap handler, then probes for each CPU feature:

```
$FC0546  MOVEM.L A2-A3,-(SP)        ; Save registers
$FC054A  MOVEA.L $000010,A0         ; Save current illegal insn vector
$FC054E  MOVEA.L $00002C,A2         ; Save current F-line vector
$FC0552  LEA     (PC,$FC0582),A1    ; Trap handler: restores SP and skips
$FC0556  MOVE.L  A1,$000010         ; Install trap handler for illegal
$FC055A  MOVE.L  A1,$00002C         ; Install trap handler for F-line
$FC055E  MOVEA.L SP,A1              ; Save SP for recovery
```

#### 68010+ detection (MOVEC VBR)

```
$FC0560  MOVEQ   #0,D0              ; Clear result flags
$FC0562  MOVEQ   #0,D1              ; VBR = 0 (keep vectors at $000000)
$FC0564  MOVEC   D1,VBR             ; 68010+ privileged instruction
$FC0568  BSET    #0,D0              ; Survived -- set AFF_68010
```

On a 68000, `MOVEC` triggers an F-line exception (vector $2C). The trap handler
restores SP from A1 and jumps to $FC0582, skipping the BSET.

#### 68020+ detection (MOVEC CACR)

```
$FC056C  MOVEQ   #1,D1              ; Enable instruction cache
$FC056E  MOVEC   D1,CACR            ; CACR exists on 68020+
$FC0572  BSET    #1,D0              ; Set AFF_68020
```

On a 68010, CACR doesn't exist -- exception fires, BSET skipped.

#### FPU detection (FMOVE.L FPCR)

```
$FC0576  FMOVE.L FPCR,D1            ; FPU control register read
$FC057A  TST.L   D1                 ; Non-zero FPCR?
$FC057C  BNE.S   $FC0582            ; Unexpected value -- skip
$FC057E  BSET    #4,D0              ; Set AFF_68881
```

Without an FPU, the F-line exception fires and the BSET is skipped.

#### Cleanup

```
$FC0582  MOVEA.L A1,SP              ; Restore SP (works for both trap and
                                    ; normal flow since A1 was saved before
                                    ; the first probe instruction)
$FC0584  MOVE.L  A0,$000010         ; Restore illegal insn vector
$FC0588  MOVE.L  A2,$00002C         ; Restore F-line vector
$FC058C  MOVEM.L (SP)+,A2-A3        ; Restore saved registers
$FC0590  RTS                        ; Return D0 = AttnFlags bits
```

The result in D0 is OR'd into ExecBase+$128 (AttnFlags) at $FC02AC.

**AttnFlags bits set by this routine:**

| Bit | Flag | Meaning | Detection method |
|-----|------|---------|-----------------|
| 0 | AFF_68010 | 68010 or higher | MOVEC VBR succeeds |
| 1 | AFF_68020 | 68020 or higher | MOVEC CACR succeeds |
| 4 | AFF_68881 | FPU present | FMOVE.L FPCR succeeds |

KS 1.3 does not detect 68030, 68040, or 68060 specifically. Those distinctions
are made by later Kickstart versions. The A3000's 68030 is detected as
AFF_68010 + AFF_68020.

> **Emulator note:** The detection relies on correct exception behaviour. A
> 68000 must raise F-line ($2C) for MOVEC. A 68010+ must execute MOVEC without
> exception. A 68020+ must accept MOVEC CACR. If the emulator raises the wrong
> exception, exec records the wrong CPU model and later code may use
> instructions that the configured CPU doesn't support.

### Stage 5b: Early Alert Handler ($FC05B4--$FC0600)

All exception vectors installed at $FC0136 point to $FC05B4. This handler
displays a flashing colour screen and attempts a warm restart:

```
$FC05B4  MOVE.W  #$0CC0,D0          ; Alert colour ($0CC0 = green)
$FC05B8  LEA     $DFF000,A4         ; Custom chip base
$FC05BE  MOVE.W  #$0200,($100,A4)   ; BPLCON0: blank display
$FC05C4  MOVE.W  #$0000,($110,A4)   ; BPLCON1: no scroll
$FC05CA  MOVE.W  D0,($180,A4)       ; COLOR00: alert colour
$FC05CE  MOVEQ   #10,D1             ; Outer loop: 11 flash cycles
$FC05D0  MOVEQ   #-1,D0             ; Inner loop: 65536 iterations
$FC05D2  BSET    #1,$BFE001         ; LED off
$FC05DA  DBF     D0,$FC05D2         ; Delay (LED off period)
$FC05DE  LSR.W   #1,D0              ; D0 = $7FFF (shorter on-period)
$FC05E0  BCLR    #1,$BFE001         ; LED on
$FC05E8  DBF     D0,$FC05E0         ; Delay (LED on period)
$FC05EC  DBF     D1,$FC05D2         ; Outer loop (~11 flashes)
$FC05F0  MOVE.L  #$20000,D0         ; Post-flash delay
$FC05F6  SUBQ.L  #1,D0
$FC05F8  BGT.S   $FC05F6            ; Wait
$FC05FA  RESET                      ; Hardware reset pulse
$FC05FC  MOVEA.L $000004,A0         ; Read ExecBase (or ROM PC vector)
$FC0600  JMP     (A0)               ; Restart via ExecBase or ROM entry
```

The entry point at $FC05B4 enters with D0 = $0CC0 (green). Code that reaches
$FC05B8 directly can set D0 to a different colour (e.g. the memory-too-small
path at $FC0238 uses $00C0).

> **Emulator note:** The `RESET` instruction resets all external hardware but
> does not reset the CPU. The overlay latch returns to its power-on state, so
> ROM is visible at $000000 again. The read from $000004 then returns the ROM's
> PC vector ($FC00D2), restarting the boot. If ExecBase survived in RAM and
> chip RAM is still mapped at $000000, the read returns ExecBase instead.

### Stage 6: Resident Module Scan and Init ($FC03CC--$FC0434)

#### RomTag scan table

```
$FC03CC  LEA     (PC,$FC07B4),A0    ; ROM scan offset table
$FC03D0  MOVEA.L A0,A1              ; A1 = read pointer
$FC03D2  MOVEA.W #$0008,A2          ; A2 = $000008 (store list here)
$FC03D6  BRA.S   $FC03DE            ; Start
$FC03D8  LEA     (A0,D0.W),A3       ; Module address = table base + offset
$FC03DC  MOVE.L  A3,(A2)+           ; Store pointer in scan list at $8+
$FC03DE  MOVE.W  (A1)+,D0           ; Read next offset (0 = end)
$FC03E0  BNE.S   $FC03D8            ; Non-zero -- add module
```

The table at $FC07B4 contains word offsets that are added to the table's own
address to produce each RomTag address. This pre-computed table avoids scanning
the entire 256K ROM for $4AFC patterns at runtime.

#### 68010+ exception vectors

If the CPU detection found a 68010+, additional exception handlers are installed
for bus/address error (format word handling) and optionally FPU exceptions:

```
$FC03E2  MOVE.W  ($128,A6),D0       ; AttnFlags
$FC03E6  BTST    #0,D0              ; 68010+?
$FC03EA  BEQ.S   $FC041E            ; No -- skip
$FC03EC  LEA     (PC,$FC08B8),A0    ; 68010+ bus/address error handler
$FC03F0  MOVEA.W #$0008,A1          ; Vector 2 ($008)
$FC03F4  MOVE.L  A0,(A1)+           ; Install bus error vector
$FC03F6  MOVE.L  A0,(A1)+           ; Install address error vector
$FC03F8  MOVE.L  #$FC08F6,(-28,A6)  ; 68010+ trap handler
$FC0400  MOVE.L  #$42C04E75,(-528,A6) ; Patch: CLR.W D0; RTS
$FC0408  BTST    #4,D0              ; FPU present?
$FC040C  BEQ.S   $FC041E            ; No -- skip
$FC040E  MOVE.L  #$FC10C6,(-52,A6)  ; FPU exception handler
$FC0416  MOVE.L  #$FC1124,(-58,A6)  ; FPU trap handler
```

#### Resident module initialisation

```
$FC041E  BSR.W   $FC1298            ; InitResident -- run all modules
```

The init loop at $FC1298 allocates a 110-byte ($6E) interrupt structure,
initialises five interrupt priority levels, and walks the RomTag list calling
each module's init function in descending priority order:

```
$FC1298  MOVEM.L D2-D3/A2-A3,-(SP)
$FC129C  MOVE.L  #$006E,D0          ; 110 bytes for interrupt structure
$FC12A2  MOVE.L  #$10001,D1         ; MEMF_PUBLIC | MEMF_CLEAR
$FC12A8  BSR.W   $FC17D0            ; AllocMem
$FC12AC  TST.L   D0                 ; Got memory?
$FC12AE  BNE.S   $FC12C6            ; Yes -- continue
$FC12B0  ; ... alert: $81000006 (no memory for interrupt init)
```

After allocating the interrupt structure, the code builds five hardware
interrupt level entries (levels 1--5 at hardware priorities 0, 3, 5, 8, 13):

```
$FC12CE  MOVE.L  A2,D1              ; Current interrupt node
$FC12D0  MOVE.L  A2,(A2)            ; NewList init
$FC12D2  ADDQ.L  #4,(A2)
$FC12D4  CLR.L   4(A2)
$FC12D8  MOVE.L  A2,8(A2)
$FC12DC  LEA     14(A2),A2          ; Next node
$FC12E0  MOVE.W  (A3)+,D3           ; Hardware priority from table
; ... install INTENA and INTREQ masks for each level
$FC1300  DBF     D2,$FC12CE         ; Loop for 5 levels
```

Then the master interrupt is enabled and each RomTag's init function is called:

```
$FC130C  MOVE.W  #$8004,$DFF09A     ; INTENA: SET + VERTB (enable VBLANK)
$FC1318  RTS
```

The actual module dispatch happens at $FC1338:

```
$FC1338  MOVE.W  (A1,18),-(SP)      ; Push INTREQ mask for this module
$FC133C  MOVE.L  A2,-(SP)           ; Save A2
$FC133E  MOVEA.L (A1),A2            ; A2 = first RomTag in priority group
$FC1340  MOVE.L  (A2),D0            ; Next RomTag pointer
$FC1342  BEQ.S   $FC1352            ; End of list
$FC1344  MOVEM.L (A2,14),A1/A5      ; A1 = module data, A5 = init function
$FC134A  JSR     (A5)               ; Call init function
$FC134C  BNE.S   $FC1352            ; Non-zero return = done
$FC134E  MOVEA.L (A2),A2            ; Next RomTag
$FC1350  BRA.S   $FC1340            ; Continue
$FC1352  MOVEA.L (SP)+,A2           ; Restore A2
$FC1354  MOVE.W  (SP)+,$DFF09C      ; Pop and write INTREQ
$FC135A  RTS
```

#### Post-init: enable DMA and interrupts

```
$FC0422  LEA     $DFF000,A0
$FC0428  MOVE.W  #$8200,($96,A0)    ; DMACON: SET + DMAEN (master DMA on)
$FC042E  MOVE.W  #$C000,($9A,A0)    ; INTENA: SET + INTEN (master int on)
$FC0434  MOVE.W  #$FFFF,($126,A6)   ; IDNestCnt = -1 (interrupts enabled)
$FC043A  BSR.W   $FC2336            ; Final init: scheduler, idle task
```

After all resident modules have initialised, master DMA and the master interrupt
enable are turned on. The system enters the scheduler and the idle task runs
until a higher-priority task (STRAP, dos, or a boot disk loader) is ready.

> **Emulator note:** After $FC041E returns, every resident module has run its
> init function. If any module's init hung (e.g. because a hardware response was
> missing), the emulation never reaches $FC0422. The DMACON write enables only
> the master DMA bit -- individual channels (copper, bitplane, disk) are enabled
> by each module during its own init.

#### Final ExecBase checksum and task start ($FC043E--$FC04C6)

```
$FC043E  MOVEQ   #0,D1              ; Accumulator
$FC0440  LEA     ($22,A6),A0        ; ExecBase+$22
$FC0444  MOVE.W  #$0016,D0          ; 23 words
$FC0448  ADD.W   (A0)+,D1           ; Sum
$FC044A  DBF     D0,$FC0448
$FC044E  NOT.W   D1                 ; Complement
$FC0450  MOVE.W  D1,($52,A6)        ; Store checksum at ExecBase+$52
```

The first user-mode task is created and launched:

```
$FC0454  LEA     (PC,$FC04CC),A0    ; Task init data
$FC0458  BSR.W   $FC195A            ; CreateTask
; ... stack setup, UserState setup ...
$FC04BE  ANDI.W  #$0000,SR          ; Drop to user mode
```

## Resident Modules

KS 1.3 contains 24 RomTag structures. The scan table at $FC07B4 lists them in
ROM order; exec sorts by priority (highest first) before calling init.

### Complete Module Table

Sorted by init priority (descending). Data extracted directly from the ROM
binary.

| Pri | RomTag | Name | Ver | Type | Flags | Init | EndSkip | ID String |
|-----|--------|------|-----|------|-------|------|---------|-----------|
| 120 | $FC00B6 | exec.library | 34.2 | NT_LIBRARY (9) | $00 | $FC00D2 | $FC3276 | exec 34.2 (28 Oct 1987) |
| 110 | $FC4B64 | expansion.library | 34.1 | NT_LIBRARY (9) | $81 | $FC4BA0 | $FC51D8 | expansion 34.1 (18 Aug 1987) |
| 100 | $FE4B44 | keymap.resource | 34.1 | NT_RESOURCE (8) | $01 | $FE7F24 | $FE4B8E | keymap ri 34.1 (18 Aug 1987) |
| 100 | $FE43DC | potgo.resource | 34.1 | NT_RESOURCE (8) | $01 | $FE4424 | $FE4524 | potgo 34.1 (18 Aug 1987) |
| 80 | $FC4574 | cia.resource | 34.1 | NT_RESOURCE (8) | $01 | $FC45E0 | $FC47F8 | cia 34.1 (18 Aug 1987) |
| 70 | $FC47FC | disk.resource | 34.1 | NT_RESOURCE (8) | $01 | $FC4840 | $FC4B5C | disk 34.1 (18 Aug 1987) |
| 70 | $FE42D0 | misc.resource | 34.1 | NT_RESOURCE (8) | $01 | $FE4314 | $FE43D8 | misc 34.1 (18 Aug 1987) |
| 70 | $FE4528 | ramlib.library | 34.1 | NT_LIBRARY (9) | $80 | $FE4560 | $FE4AC8 | ramlib 34.1 (18 Aug 1987) |
| 65 | $FC53E4 | graphics.library | 34.1 | NT_LIBRARY (9) | $01 | $FCABA2 | $FD08B8 | graphics 34.1 (18 Aug 1987) |
| 60 | $FE4B8E | keyboard.device | 34.1 | NT_DEVICE (3) | $01 | $FE4F44 | $FE4BDA | keyboard ri 34.1 (18 Aug 1987) |
| 60 | $FE4BDA | gameport.device | 34.1 | NT_DEVICE (3) | $01 | $FE53B0 | $FE4C26 | gameport ri 34.1 (18 Aug 1987) |
| 50 | $FE8D6C | timer.device | 34.1 | NT_DEVICE (3) | $01 | $FE8DF4 | $FE9558 | timer 34.1 (18 Aug 1987) |
| 40 | $FC3508 | audio.device | 34.1 | NT_DEVICE (3) | $81 | $FC354C | $FC445C | audio 34.1 (18 Aug 1987) |
| 31 | $FE09A4 | layers.library | 34.1 | NT_LIBRARY (9) | $01 | $FE0A2C | $FE0A0C | layers 34.1 (18 Aug 1987) |
| 20 | $FE9564 | trackdisk.device | 34.1 | NT_DEVICE (3) | $01 | $FE97BE | $FEB05C | trackdisk 34.1 (18 Aug 1987) |
| 10 | $FD3D8C | intuition.library | 34.3 | NT_LIBRARY (9) | $81 | $FD3DA6 | $FDFF94 | intuition 34.3 (16 Nov 1987) |
| 5 | $FC3276 | alert.hook | 34 | special (0) | $01 | $FC3128 | $FC3290 | alert.hook |
| 0 | $FE3DA4 | mathffp.library | 34.1 | NT_LIBRARY (9) | $81 | $FE3DEC | $FE4288 | mathffp 34.1 (18 Aug 1987) |
| 0 | $FE4C26 | input.device | 34.1 | NT_DEVICE (3) | $01 | $FE5AD0 | $FE4C6C | input ri 34.1 (18 Aug 1987) |
| 0 | $FE4C6C | console.device | 34.1 | NT_DEVICE (3) | $01 | $FE66E4 | $FE6234 | console ri 34.1 (18 Aug 1987) |
| 0 | $FEB47C | workbench.task | 34.1 | NT_TASK (1) | $00 | $FEB496 | $FF310C | wb 34.1 (18 Aug 1987) |
| 0 | $FF3E62 | dos.library | 34.3 | NT_LIBRARY (9) | $00 | $FF3E94 | $FF3E94 | dos 34.3 (9 Dec 1987) |
| -40 | $FEB060 | romboot.library | 34.1 | NT_LIBRARY (9) | $01 | $FEB0A8 | $FEB380 | romboot 34.1 (18 Aug 1987) |
| -60 | $FE83E0 | strap | 34.4 | special (0) | $01 | $FE8444 | $FE841A | strap 34.4 (9 Dec 1987) |

**Flags key:**
- $00 = no special flags (init function called directly)
- $01 = RTF_COLDSTART (module initialised during cold start boot)
- $80 = RTF_AUTOINIT (uses auto-init table rather than direct function call)
- $81 = RTF_COLDSTART + RTF_AUTOINIT

**Type key:** NT_LIBRARY=9, NT_DEVICE=3, NT_RESOURCE=8, NT_TASK=1. Type 0 is a
special module (alert hook, strap) that doesn't create a standard library or
device node.

### Notable differences from KS 2.0+

KS 1.3 has 24 modules vs 39+ in KS 2.0. Missing modules:
- No utility.library (introduced in KS 2.0)
- No gadtools.library, icon.library, workbench.library
- No battclock.resource, battmem.resource
- No syscheck, bootmenu
- No ramdrive.device
- No con-handler, shell, ram-handler (on Workbench disk instead)
- No FileSystem.resource
- workbench.task at priority 0 (moved to -120 in KS 2.0+)

### Module Init Details

Modules initialise in priority order (highest first). Within the same priority,
ROM order determines sequence.

#### Priority 120: exec.library

Already partially initialised in Stage 4. The RomTag init at $FC00D2 completes
library setup -- function table, trap vectors, scheduling primitives. The init
pointer points back to the entry point because exec's "init" IS the entire boot
sequence up to this point.

#### Priority 110: expansion.library ($FC4BA0)

Scans the Zorro II autoconfig space at $E80000 for expansion boards:
1. Read manufacturer/product bytes from $E80000+
2. If a board responds, assign its base address via WRITE_CONFIG
3. Loop until $E80000 returns $FF (no board)

> **Emulator note:** $E80000 must return $FF for byte reads when no expansion
> board is present. Returning $00 looks like a valid Commodore board and causes
> expansion.library to attempt configuration of a non-existent device.

#### Priority 100: keymap.resource ($FE7F24), potgo.resource ($FE4424)

**keymap.resource** installs the default USA key mapping table. No hardware
interaction beyond memory allocation.

**potgo.resource** initialises the proportional controller (game paddle)
hardware. Writes POTGO ($DFF034) to set charge/discharge timing.

#### Priority 80: cia.resource ($FC45E0)

Initialises both CIA chips and creates two resource nodes: `ciaa.resource` and
`ciab.resource`. The init:
1. Reads CIA-A ICR ($BFED01) and CIA-B ICR ($BFDD00) to clear pending
   interrupts
2. Sets up the interrupt server chain for CIA-A (exec level 2) and CIA-B
   (exec level 6)
3. Configures CIA timer registers for system use

> **Emulator note:** Both CIAs must respond to register reads. The init reads
> ICR to clear pending flags. If CIAs don't respond, the interrupt server chain
> is misconfigured and no CIA-driven interrupts (keyboard, timers) function.

#### Priority 70: disk.resource ($FC4840), misc.resource ($FE4314), ramlib ($FE4560)

**disk.resource** manages the raw MFM DMA channel and drive select lines. Writes
DSKSYNC ($DFF07E) with the standard MFM sync word ($4489).

**misc.resource** arbitrates the shared serial/parallel port hardware.

**ramlib.library** handles loading libraries and devices from disk. This is what
allows `OpenLibrary()` to search the disk when a library isn't in ROM. Uses
RTF_AUTOINIT ($80 flag) -- the init table creates the library base
automatically.

#### Priority 65: graphics.library ($FCABA2)

The largest module in the ROM (spanning $FC53E4--$FD08B8). Init performs:

1. **Chipset detection:** Reads DENISEID ($DFF07C). OCS Denise returns $FF (no
   ID register). ECS Super Denise returns $FC. AGA Lisa returns $F8.

2. **PAL/NTSC detection:** Reads VPOSR ($DFF004) bit 12. PAL = 1, NTSC = 0.

3. **EClock calibration:** Uses CIA-B timer to measure one video frame duration.
   The result is stored at GfxBase+$22 and used as a divisor by timer.device.
   If this value is zero (because the CIA timer didn't tick relative to VBLANK),
   timer.device crashes with DIVU #0.

4. **Copper list construction:** Builds initial copper lists for the system
   display.

5. **Blitter init:** Resets blitter state, waits for any pending operation.

6. **Colour palette:** Initialises the 32-entry OCS colour table.

> **Emulator note:** graphics.library is the most hardware-intensive init:
> - DENISEID ($DFF07C) must return the correct chipset ID
> - VPOSR ($DFF004) bit 12 must match the configured region
> - CIA-B timer ticks must advance at the correct rate relative to VBLANK
> - The blitter must respond to BLTCON0/BLTCON1 writes
> - The copper must be startable via COP1LC ($DFF080) / COPJMP1 ($DFF088)
>
> The EClock calibration failure was the root cause of the Battclock issue:
> force-setting CIA-A TOD high byte corrupted timer.device's calibration loop,
> producing zero in GfxBase+$22, which caused a DIVU #0 in the STRAP timing
> calculation.

#### Priority 60: keyboard.device ($FE4F44), gameport.device ($FE53B0)

**keyboard.device** initialises keyboard communication via CIA-A serial port
(SP register at $BFEC01). Sends a reset command and waits for the keyboard
controller to respond with the power-up sequence ($FD init, $FE term).

**gameport.device** initialises game controller input handling.

> **Emulator note:** The keyboard controller must complete the power-up
> handshake. See `peripheral-amiga-keyboard` crate: `encode_keycode(byte) =
> !byte.rotate_left(1)`. Without the $FD/$FE sequence, keyboard.device times
> out. The timeout doesn't prevent boot, but the keyboard won't work.

#### Priority 50: timer.device ($FE8DF4)

Calibrates system timing using CIA timers. Provides VBLANK and EClock-based
timer services. Divides by the GfxBase EClock value set by graphics.library.

> **Emulator note:** If GfxBase+$22 (EClock value) is zero, timer.device's init
> triggers a DIVU #0 exception. This is always a symptom of broken CIA timer /
> VBLANK timing, not a timer.device bug.

#### Priority 40: audio.device ($FC354C)

Manages the four Paula audio DMA channels. Uses RTF_AUTOINIT. Sets up channel
allocation and audio interrupt handling via Paula's INTENA/INTREQ bits.

#### Priority 31: layers.library ($FE0A2C)

Layer (window clipping) management. No direct hardware interaction beyond
memory allocation.

#### Priority 20: trackdisk.device ($FE97BE)

Floppy disk driver. Init performs:
1. Set drive select lines via CIA-B PRA ($BFD100)
2. Configure DSKSYNC ($DFF07E) for MFM sync ($4489)
3. Configure DSKLEN ($DFF024) for DMA transfer size
4. Start motor on DF0:
5. Seek to track 0 (step signal via CIA-B)
6. Wait for track-zero signal

> **Emulator note:** Full floppy hardware pipeline required:
> - CIA-B PRA: DSKMOTOR (bit 7), DSKSEL0-3 (bits 3-6), DSKDIREC (bit 1),
>   DSKSTEP (bit 0)
> - DSKLEN ($DFF024): DMA transfer control
> - DSKSYNC ($DFF07E): MFM sync word
> - DSKBYTR ($DFF01A): byte-level disk status
> - CIA-B PRB: DSKCHANGE (bit 2), DSKPROT (bit 3), DSKRDY (bit 5)

#### Priority 10: intuition.library ($FD3DA6)

Window and screen manager. Uses RTF_AUTOINIT. Creates the Intuition base
structure, default screen preferences, and the input handler chain. Does not
open a visible screen -- STRAP does that.

#### Priority 5: alert.hook ($FC3128)

Replaces the simple early alert handler ($FC05B4) with the full guru meditation
display. The handler at $FC3128 formats the alert code from ExecBase+$202 and
opens an Intuition alert display:

```
$FC3128  MOVEM.L D2/D7/A2-A3/A6,-(SP)  ; Save registers
$FC312C  MOVEQ   #10,D1                 ; Outer delay loop
$FC312E  MOVEQ   #-1,D0                 ; Inner loop: 65536 iterations
$FC3130  DBF     D0,$FC3130             ; Busy-wait (~7M cycles = ~1 sec)
$FC3134  DBF     D1,$FC3130             ; 11 iterations total
$FC3138  MOVE.L  ($202,A6),D2           ; Load alert code from ExecBase
$FC313C  MOVEQ   #-1,D0                 ; $FFFFFFFF = "no alert"
$FC313E  CMP.L   D0,D2                  ; Alert present?
$FC3140  BEQ.S   $FC31B8               ; No -- skip display
```

If D2 holds an alert code, the handler:
1. Classifies it as dead-end (bit 31 set = $8xxxxxxx, red) or recoverable
2. Builds a displayable string ("Software Failure" or "Recoverable Alert")
3. Formats the hex code as `#XXXXXXXX.XXXXXXXX`
4. Opens an Intuition DisplayAlert
5. If Intuition is available, shows the alert with the flashing red/black border
6. Clears the alert code and returns

#### Priority 0: mathffp, input.device, console.device, workbench.task, dos.library

**mathffp.library** ($FE3DEC) -- Motorola Fast Floating Point. Pure
computation, no hardware interaction.

**input.device** ($FE5AD0) -- Input event manager. Merges keyboard, gameport,
and other sources into a unified event stream. Reads from the CIA and custom
chip registers indirectly via keyboard.device and gameport.device.

**console.device** ($FE66E4) -- Console I/O. ANSI escape code processing, text
rendering. Works through graphics.library for display output.

**workbench.task** ($FEB496) -- The Workbench desktop task. Created but idle
until a Workbench disk is inserted and the startup-sequence runs.

**dos.library** ($FF3E94) -- AmigaDOS. File system, process management, command
parsing. Opens trackdisk.device for DF0: access.

#### Priority -40: romboot.library ($FEB0A8)

Handles booting from expansion ROM boards. Scans the board list built by
expansion.library, checks each board's DiagArea for valid boot code, and runs
it if found. On systems without expansion boards, this completes immediately.

#### Priority -60: strap ($FE8444)

The System Test and Registration Program. Last module to initialise. Displays
the insert-disk screen and enters the disk boot wait loop. See
[STRAP Display](#strap-display-insert-disk-screen) for full detail.

## STRAP Display (Insert-Disk Screen)

### Init sequence ($FE8444)

STRAP allocates a bitmap buffer, opens an Intuition screen, draws the insert-
disk icon, and enters the disk detection loop:

```
$FE8444  MOVEM.L D2-D3/A2-A5,-(SP)  ; Save registers
$FE844A  SUBA.L  A4,A4              ; A4 = NULL (no bitmap yet)
$FE844C  LEA     $FE86EE,A3         ; Font/glyph data for icon
$FE8458  LINK    A5,#-126           ; 126 bytes of local variables
$FE846A  MOVE.L  #$0488,D0          ; 1,160 bytes for bitmap
$FE8476  JSR     -198(A6)           ; AllocMem(MEMF_CHIP | MEMF_CLEAR)
$FE847A  TST.L   D0                 ; Got memory?
$FE847C  BNE.S   $FE8498            ; Yes -- continue
```

If bitmap allocation fails:

```
$FE847E  MOVEM.L D7/A5-A6,-(SP)
$FE8482  MOVE.L  #$30010000,D7      ; Alert: no memory for screen
$FE8488  MOVEA.L $000004,A6
$FE848C  JSR     -108(A6)           ; Alert()
```

After allocating the bitmap, STRAP opens an Intuition screen with 2 bitplanes
(4 colours) in lowres mode, then draws the checkmark/floppy icon and "Insert
disk" text using the blitter.

### Disk detection loop

STRAP uses trackdisk.device I/O to detect and boot from floppy disks:

1. Issue `CMD_READ` (command 2) for 1,024 bytes (2 sectors) from offset 0
2. If the read fails or the drive is empty, issue `TD_CHANGESTATE` (command 13)
   to poll for disk insertion, then `TD_MOTOR` (command 9) and retry
3. Verify the bootblock magic: first longword = `DOS\0` ($444F5300)
4. Compute the bootblock checksum:

```
$FE85AC  MOVEA.L A4,A0              ; A0 = bootblock buffer (1024 bytes)
$FE85AE  MOVE.W  #$00FF,D1          ; 256 longwords
$FE85B2  MOVEQ   #0,D0              ; Checksum accumulator
$FE85B4  ADD.L   (A0)+,D0           ; Add longword
$FE85B6  BCC.S   $FE85BA            ; No carry?
$FE85B8  ADDQ.L  #1,D0              ; Add carry (one's complement)
$FE85BA  DBF     D1,$FE85B4         ; Loop all 256 longwords
$FE85BE  NOT.L   D0                 ; Complement -- should be 0
$FE85C0  BNE.S   $FE8600            ; Non-zero -- invalid bootblock
```

5. If the checksum is valid, jump to the bootblock code at offset $0C:
   `JSR 12(A4)` -- boot code entry.
6. If invalid, wait and retry.

### Expected register state

After STRAP displays the insert-disk screen:

| Register | Address | Value | Meaning |
|----------|---------|-------|---------|
| DMACON (read) | $DFF002 | $03C0+ set bits | BLTPRI + DMAEN + BPLEN + COPEN + BLTEN |
| BPLCON0 | $DFF100 | $2302 | 2 bitplanes, lowres, colour, composite |

The display is a 2-bitplane lowres screen (320x256 PAL, 320x200 NTSC) showing
a floppy disk icon with a checkmark and the text "Insert disk" in the system
font. Palette: COLOR00 = grey background, COLOR01--COLOR03 = icon/text colours.

Boot tests assert: DMACON bits $0180 (BPLEN + COPEN) are set, BPLCON0 = $2302.

## Error Paths

### Alert codes

Alerts use D7 for the code. The upper word identifies the subsystem; the lower
word identifies the specific failure. Bit 31 distinguishes dead-end (1 = red,
system halts) from recoverable (0 = yellow, system continues).

| Code | Meaning | Stage |
|------|---------|-------|
| $81000005 | AG_NoMemory \| AO_ExecLib | ExecBase init -- can't allocate |
| $81000006 | No memory for interrupt init | Resident module init loop |
| $30010000 | No memory for STRAP screen | STRAP bitmap allocation |
| $30048014 | Screen open error | STRAP Intuition screen open |
| $30070000 | Can't open graphics.library | STRAP init |

### Alert display sequence

1. Set BPLCON0 = $0200 (blank display)
2. Set COLOR00 to alert colour (dead-end = $0C00 red, recoverable = $0CC0
   yellow)
3. Flash power LED in an asymmetric pattern (long off, short on)
4. If the full alert.hook handler is installed (after priority 5 init), format
   the alert code as hex and display via Intuition's DisplayAlert
5. Wait ~10 seconds or for left mouse button
6. Execute `RESET` instruction and attempt warm restart

### Dead-end loops

If chip RAM detection finds less than 256K:

```
$FC0238  MOVE.W  #$00C0,D0          ; Partial alert colour
$FC023C  BRA.W   $FC05B8            ; Alert handler -> RESET -> retry
```

The system resets and retries indefinitely. Without 256K chip RAM, the ROM
cannot proceed.

## Hardware Probing Summary

Every hardware read/write during the boot, in execution order.

### Stage 2: Initial Setup

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $F00000 | R | expect != $1111 | Diagnostic ROM magic check |
| $BFE201 | W | $03 | CIA-A DDRA: OVL + LED as outputs |
| $BFE001 | W | $02 | CIA-A PRA: overlay off, LED off |
| $DFF09A | W | $7FFF | INTENA: disable all interrupts |
| $DFF09C | W | $7FFF | INTREQ: clear all interrupt requests |
| $DFF096 | W | $7FFF | DMACON: disable all DMA |
| $DFF100 | W | $0200 | BPLCON0: blank display |
| $DFF110 | W | $0000 | BPLCON1: no scroll |
| $DFF180 | W | $0444 | COLOR00: dark grey |

### Stage 3: Memory Detection

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $C00000--$DBFFFF | R/W | test patterns | Slow RAM probe (256K steps) |
| $C0F09A, $C0F01C etc. | W/R | $3FFF, $BFFF | INTENA side-effect test |
| $000000--$1FFFFF | R/W | $F2D4B698 | Chip RAM alias/size detection |

### Stage 4: ExecBase Init

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $DFF096 | W | $7FFF | DMACON: disable all (again) |
| $DFF100 | W | $0200 | BPLCON0: blank |
| $DFF110 | W | $0000 | BPLCON1: no scroll |
| $DFF180 | W | $0888 | COLOR00: medium grey (progress) |

### Stage 5: CPU Detection

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $000010 | R/W | save/restore | Illegal instruction vector |
| $00002C | R/W | save/restore | F-line vector |

### Module Init Phase

| Module | Address | R/W | Purpose |
|--------|---------|-----|---------|
| expansion | $E80000+ | R | Zorro II autoconfig probe |
| potgo | $DFF034 | W | POTGO: paddle charge/discharge |
| cia.resource | $BFED01 | R | CIA-A ICR: clear pending |
| cia.resource | $BFDD00 | R | CIA-B ICR: clear pending |
| cia.resource | $BFEx01/$BFDx00 | R/W | CIA timer, port registers |
| graphics | $DFF07C | R | DENISEID: chipset detection |
| graphics | $DFF004 | R | VPOSR: PAL/NTSC |
| graphics | $BFD400+ | R/W | CIA-B timer: EClock calibration |
| graphics | $DFF080 | W | COP1LC: copper list pointer |
| graphics | $DFF088 | W | COPJMP1: copper restart |
| keyboard | $BFEC01 | R/W | CIA-A SP: keyboard serial data |
| timer | $BFEx01/$BFDx00 | R/W | CIA timers: calibration |
| trackdisk | $BFD100 | W | CIA-B PRA: drive select, motor |
| trackdisk | $DFF024 | W | DSKLEN: DMA control |
| trackdisk | $DFF07E | W | DSKSYNC: MFM sync word |
| trackdisk | $DFF01A | R | DSKBYTR: byte status |
| STRAP | $DFF096 | W | DMACON: enable channels |
| STRAP | $DFF100 | W | BPLCON0: $2302 (2 planes) |

### Post-Init

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $DFF096 | W | $8200 | DMACON: SET + DMAEN (master on) |
| $DFF09A | W | $C000 | INTENA: SET + INTEN (master on) |

## A3000 Variant Differences

The A3000 ROM (`kick13_34_005_a3000.rom`) differs from the standard A500/A2000
ROM by exactly 3 bytes:

### Difference 1: Chip RAM size limit in warm start ($FC019A)

| Offset | Address | A500/A2000 | A3000 | Effect |
|--------|---------|------------|-------|--------|
| $019D | $FC019D | $08 | $20 | Immediate operand changes from $00080000 to $00200000 |

**A500 instruction:** `CMPA.L #$00080000,A3` -- chip RAM must be <= 512K

**A3000 instruction:** `CMPA.L #$00200000,A3` -- chip RAM must be <= 2 MB

The warm start validation at $FC019A compares ExecBase->MaxLocMem against the
maximum chip RAM size for the machine. The A500/A2000 caps at 512K (the physical
limit with OCS Agnus). The A3000 raises the limit to 2 MB (Fat Agnus).

This code only runs during warm start. On cold start, the ExecBase validation
fails earlier (at $FC015E) because chip RAM is zeroed and $000004 doesn't hold
a valid ExecBase pointer.

### Difference 2: Addressing mode in romboot.library ($FEAF9C)

| Offset | Address | A500/A2000 | A3000 | Effect |
|--------|---------|------------|-------|--------|
| $2AF9D | $FEAF9D | $B9 | $BC | Opcode byte changes addressing mode |

**A500 instruction:** `CMP.L ($8000).L,D0` -- reads comparison value from
chip RAM address $8000

**A3000 instruction:** `CMP.L #$8000,D0` -- compares against immediate
constant $8000

Both instructions compare D0 with the value $8000 (32,768), but the A500
version fetches the comparison operand from chip RAM while the A3000 version
uses an immediate. The A3000 change avoids a chip RAM read during romboot init
-- the A3000's RAMSEY memory controller may not have chip RAM fully stable at
this point in boot, or the 32-bit bus architecture may handle the absolute long
addressing differently.

### Difference 3: ROM checksum ($3FFE8)

| Offset | Address | A500/A2000 | A3000 |
|--------|---------|------------|-------|
| $3FFE8 | $FFFE8 | $15 | $15 |
| $3FFE9 | $FFFE9 | $26 | $0B |

The 16-bit checksum at offset $3FFE8 compensates for the two code changes. The
ROM-wide checksum algorithm sums all words in the ROM -- this adjustment ensures
both variants produce the same checksum result.

### Emulator implications

For emulation purposes, both ROMs are functionally identical during cold start.
The differences matter only for:

1. **Warm start on A3000** -- the chip RAM limit at $FC019A permits up to 2 MB
2. **romboot on A3000** -- the immediate addressing avoids a chip RAM read
   (emulators that model chip RAM as always stable won't see any difference)
3. **ROM identification** -- if the emulator validates or fingerprints ROM
   checksums, it should accept both $1526 and $150B at offset $3FFE8

## Emulator Implications Summary

Collected from all sections above, ordered by boot stage.

### Must work before Stage 2 completes

- **Overlay latch** active at power-on, deactivated by CIA-A PRA bit 0 write
- **ROM mapped** at $FC0000--$FFFFFF (and $000000--$03FFFF via overlay)
- **CIA-A register writes** ($BFE001, $BFE201) take effect immediately
- **Custom chip register writes** ($DFF09A, $DFF09C, $DFF096) take immediate
  effect -- DMA stops, interrupts disabled within one bus cycle
- **$F00000** returns a value other than $1111 (unless diagnostic ROM present)

### Must work before Stage 3 completes

- **Chip RAM** responds to read/write at $000000--$1FFFFF
- **Chip RAM aliasing** -- 512K wraps $080000 to $000000
- **Slow RAM** at $C00000 responds if present (A501 trapdoor or equivalent)
- **Unmapped slow RAM range** does NOT alias to custom chip registers (the
  INTENA side-effect test at $C0F09A must not affect real $DFF09A)

### Must work before Stage 4 completes

- **Exception vectors** at $000008--$0000BC writable in chip RAM
- **$000004** writable (ExecBase pointer storage)

### Must work before Stage 5 completes

- **68000:** F-line exception ($2C) for MOVEC, FMOVE.L FPCR instructions
- **68010+:** MOVEC VBR executes without exception
- **68020+:** MOVEC CACR executes without exception
- **FPU present:** FMOVE.L FPCR executes without exception

### Must work before module init completes

- **$E80000** returns $FF when no expansion boards present
- **CIA-A and CIA-B** respond to all register reads and writes
- **DENISEID** ($DFF07C) returns correct chipset ID ($FF for OCS)
- **VPOSR** ($DFF004) bit 12 correct for PAL (1) or NTSC (0)
- **CIA-B timer** ticks at correct rate relative to VBLANK (EClock calibration)
- **Keyboard controller** sends $FD/$FE power-up sequence via CIA-A SP
- **Blitter** responds to BLTCON0/BLTCON1 and completes operations
- **Copper** responds to COP1LC/COPJMP1 and executes copper lists
- **Floppy hardware** -- CIA-B PRA drive select, DSKLEN, DSKSYNC, DSKBYTR
- **Bitplane DMA** fetches from BPL1PT/BPL2PT addresses

### Common failure modes

| Symptom | Root cause | Check |
|---------|-----------|-------|
| Black screen, CPU loops at $FC05B4 | Exception during early init | Which vector fired; D0 holds colour |
| Dark grey screen ($0444) | Stuck before overlay clear | CIA-A PRA write; chip RAM not visible |
| Medium grey screen ($0888) | Stuck during ExecBase init | Memory detection; write to $000004 |
| Green screen flashing | Early alert handler running | Alert colour $0CC0; check exception |
| Insert-disk screen missing elements | Copper or bitplane DMA broken | DMACON channels; copper list valid |
| Freeze during graphics init | EClock calibration stuck | CIA-B timer not ticking vs VBLANK |
| DIVU #0 crash after graphics | EClock = 0 at GfxBase+$22 | CIA timer rate wrong; Battclock issue |
| No keyboard response | keyboard.device timeout | CIA-A SP handshake; $FD/$FE sequence |
| Disk inserted but no boot | MFM pipeline broken | Motor signal; DSKLEN; sector decode |
| Guru meditation on warm reset | ExecBase validation failed | $000004 contents; ChkBase complement |
