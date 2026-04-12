# Kickstart 3.1 Boot Flow (A1200 Primary)

Annotated boot flow for Kickstart 3.1 (V40), with the A1200 (40.068) as the
primary reference. Covers all 8 KS 3.1 ROM variants, including the CD32, A3000,
A4000, OCS, and the anomalous MapROM A600 beta build.

This is the AGA reference document. For the common boot stage model, see
[boot-flow-overview.md](boot-flow-overview.md). For KS 1.3 (OCS reference), see
[ks-1.3.md](ks-1.3.md).

## ROM Identification

### 512K ROMs ($F80000-$FFFFFF)

All KS 3.1 ROMs are 524,288 bytes. SSP = $11144EF9, PC = $F800D2. ROM header
version word at offset $0C = $0028 (version 40), revision at $0E = $0044
(revision 68 for the A1200 build).

| Build | File | Target | Notes |
|-------|------|--------|-------|
| 40.060 | `kick31_40_060_cd32.rom` | CD32 | Akiko chip support, no keyboard |
| 40.063 | `kick31_40_063_a500_a600_a2000.rom` | A500, A600, A2000 | OCS, no AGA registers |
| 40.068 | `kick31_40_068_a1200.rom` | A1200 | **PRIMARY** -- AGA, Gayle, PCMCIA |
| 40.068 | `kick31_40_068_a3000.rom` | A3000 | RAMSEY, Fat Gary, 68030/040 |
| 40.068 | `kick31_40_068_a4000.rom` | A4000 | AGA, 68040, no PCMCIA |
| 40.070 | `kick31_40_070_a4000_beta.rom` | A4000 | Beta build |
| 40.070 | `kick31_40_070_a4000t.rom` | A4000T | Tower variant |

### Anomalous ROM

| Build | File | PC | Notes |
|-------|------|----|-------|
| 40.068 | `kick31_40_068_a600_beta.rom` | $002000D2 | MapROM -- expects ROM at $200000, not $F80000. Requires MapROM hardware adapter. Not usable in standard emulation. |

## Boot Flow (A1200 40.068)

### Stage 1: Reset Vector Fetch

The 68EC020 reads the initial SSP from $000000 and PC from $000004. The overlay
latch maps ROM at $000000 so these reads hit the ROM image.

- SSP = $11144EF9 (unmapped -- never used, overwritten immediately)
- PC = $F800D2 (first executable instruction)

### Stage 2: ROM Checksum Verification (NEW in KS 2.0+)

**This does not exist in KS 1.3.** Before any hardware is touched, the ROM sums
every longword in the 512K image using a carry-propagating addition. If the sum
is not $FFFFFFFF, the ROM branches to a checksum failure handler that flashes
the power LED and displays a colour screen.

```
$F800D2  LEA     $400,SP              ; Temporary stack in chip RAM page 1
$F800D6  LEA     (PC,$FF28),A0        ; A0 = $F80000 (ROM base, PC-relative)
$F800DA  MOVEQ   #-1,D1               ; D1 = $FFFF (inner loop: 64K longwords)
$F800DC  MOVEQ   #1,D2                ; D2 = 1 (outer loop: 2 passes = 128K longs = 512KB)
$F800DE  MOVEQ   #0,D5                ; D5 = running sum
$F800E0  ADD.L   (A0)+,D5             ; Add next longword
$F800E2  BCC.S   $F800E6              ; No carry? skip
$F800E4  ADDQ.L  #1,D5               ; Propagate carry
$F800E6  DBF     D1,$F800E0           ; Inner loop (65536 iterations)
$F800EA  DBF     D2,$F800E0           ; Outer loop (2 iterations)
```

The sum of all 131,072 longwords (including the checksum complement stored in
the ROM) must equal $FFFFFFFF. The NOT of the result is tested later:

```
$F801A6  NOT.L   D5                   ; If sum was $FFFFFFFF, D5 is now 0
$F801A8  BNE.W   $F80408              ; Non-zero = bad checksum -> error path
```

**Emulator implications:** The ROM file must be intact. A single corrupted byte
causes the ROM to branch to the error handler. The checksum loop executes
~262,000 instructions before any hardware interaction -- useful for validating
that the CPU executes basic ALU and loop instructions correctly.

### Stage 3a: Diagnostic ROM Check

After the checksum, the ROM checks for a diagnostic ROM at $F00000:

```
$F800EE  LEA     (PC,$0062),A5        ; A5 = $F80152 (return address for Gayle skip)
$F800F2  LEA     (PC,$FF0C),A0        ; A0 = $F80000 (ROM base)
$F800F6  LEA     $F00000,A1           ; Diagnostic ROM address
$F800FC  CMPA.L  A0,A1                ; Skip if diag ROM = ROM base (self-check guard)
$F800FE  BEQ.S   $F8010A
$F80100  CMPI.W  #$1111,(A1)          ; Magic word at $F00000?
$F80104  BNE.S   $F8010A              ; No diagnostic ROM
$F80106  JMP     2(A1)                ; Jump to diagnostic ROM entry at $F00002
```

This supports A3000 DMAC diagnostic ROMs and A1000 WCS boards. On a standard
A1200 without a diagnostic ROM, $F00000 reads as 0 and the check falls through.

**Emulator implications:** $F00000 must return something other than $1111 as a
word read. Returning 0 (unmapped) is correct for A1200/A500/A600.

### Stage 3b: Gayle Init (NEW in KS 2.04+)

Immediately after the diagnostic check, the ROM writes to Gayle:

```
$F8010A  MOVE.B  #$00,$DA8000         ; Gayle register write: clear PCMCIA config
$F80112  NOP                          ; Pipeline sync after Gayle write
```

This write at $DA8000 resets the Gayle chip's PCMCIA configuration register. On
machines without Gayle (A500, A2000), this write goes to unmapped space and is
harmlessly ignored.

**Emulator implications:** The Gayle register at $DA8000 must accept byte
writes. On non-Gayle machines, the write must be silently discarded.

### Stage 3c: PCMCIA Probe (NEW in KS 2.04+)

The ROM probes PCMCIA attribute space at $A00000 for a card identification
signature:

```
$F80114  LEA     $A00000,A1           ; PCMCIA attribute memory base
$F8011A  CMPI.B  #$91,(A1)            ; Byte 0: CISTPL_DEVICE = $91?
$F8011E  BNE.S   $F80152              ; No card -> skip to CIA init
$F80120  ADDQ.L  #2,A1                ; Attribute memory is word-spaced (even bytes only)
$F80122  CMPI.B  #$05,(A1)            ; Byte 2: device info = $05?
$F80126  BNE.S   $F80152
$F80128  ADDQ.L  #2,A1
$F8012A  CMPI.B  #$23,(A1)            ; Byte 4: type = $23?
$F8012E  BNE.S   $F80152
```

If the three signature bytes match ($91, $05, $23), the ROM reads four more
bytes to construct a jump address:

```
$F80130  ADDQ.L  #2,A1                ; Bytes at $A00006, $A00008, $A0000A, $A0000C
$F80132  MOVE.B  (A1),D0              ; Read byte, rotate into longword
$F80134  ROR.L   #8,D0
         ... (repeated 4 times)
$F80148  LEA     $600000,A0           ; Base address for PCMCIA code
$F8014E  ADDA.L  D0,A0               ; Add offset from card data
$F80150  JMP     (A0)                 ; Jump to PCMCIA boot code
```

This allows a PCMCIA card with the right header to take over the boot process
entirely -- used by some accelerator and flash ROM cards.

**Emulator implications:** If PCMCIA is not populated, reads from $A00000 must
return something other than $91 (returning $FF or $00 is fine). If the emulator
implements PCMCIA cards with CIS data, the attribute space reads must return the
correct tuple data at even addresses.

### Stage 3d: CIA and Custom Chip Reset

If no PCMCIA card is found (or no Gayle present), execution continues at
$F80152:

```
$F80152  MOVE.B  #$01,$DA8000         ; Gayle: enable normal operation
$F8015A  CLR.B   $BFA001              ; CIA-B PRA: clear all outputs
$F80160  CLR.B   $BFA201              ; CIA-B DDRA: all inputs
$F80166  CLR.B   $BFE001              ; CIA-A PRA: clear OVL + LED
$F8016C  MOVE.B  #$03,$BFE201         ; CIA-A DDRA: bits 0,1 as outputs (OVL + LED)
```

This clears the overlay (chip RAM now visible at $000000) and turns the power
LED off. CIA-B is set to all-inputs.

Custom chip init follows:

```
$F80174  LEA     $DFF000,A4           ; A4 = custom chip base (kept throughout boot)
$F8017A  MOVE.W  #$7FFF,D0            ; Clear/disable mask
$F8017E  MOVE.W  D0,$009A(A4)         ; INTENA = $7FFF (disable all interrupts)
$F80182  MOVE.W  D0,$009C(A4)         ; INTREQ = $7FFF (clear all pending interrupts)
$F80186  MOVE.W  D0,$0096(A4)         ; DMACON = $7FFF (disable all DMA)
$F8018A  MOVE.W  #$0174,$0032(A4)     ; undocumented (SERPER? = $0174)
$F80190  MOVE.W  #$0200,$0100(A4)     ; BPLCON0 = $0200 (blank display, colour on)
$F80196  MOVE.W  #$0000,$0110(A4)     ; BPLCON3 = $0000 (bank 0, no dual playfield)
$F8019C  MOVE.W  #$0111,$0180(A4)     ; COLOR00 = $0111 (very dark grey background)
```

Note the BPLCON3 write ($DFF110) -- this is an AGA/ECS register that does not
exist on OCS Denise. On OCS machines, the write is ignored. On AGA machines, it
resets the palette bank select to bank 0.

### Stage 2 (revisited): Warm/Cold Start Detection

After the hardware reset, the ROM checks whether this is a warm start:

```
$F801A6  NOT.L   D5                   ; ROM checksum result (0 if valid)
$F801A8  BNE.W   $F80408              ; Checksum failed -> error display

$F801AC  MOVEA.W #$0008,A0            ; A0 = $000008 (first exception vector)
$F801B0  MOVE.W  #$002D,D1            ; 46 vectors ($008-$0BC)
$F801B4  LEA     (PC,$024E),A1        ; A1 = $F80404 (default exception handler)
$F801B8  MOVE.L  A1,(A0)+             ; Fill vector table
$F801BA  DBF     D1,$F801B8

$F801BE  MOVE.W  #$00F0,D0            ; D0 = $F0 (secondary colour for verification)
$F801C2  MOVE.W  #$002D,D1            ; 46 vectors again
$F801C6  CMPA.L  -(A0),A1             ; Verify vectors wrote correctly
$F801C8  BNE.W   $F80408              ; RAM failure -> error display
$F801CC  DBF     D1,$F801C6
```

This fills exception vectors $008-$0BC with a default handler, then reads them
back to verify chip RAM is functional. If any vector doesn't read back
correctly, the ROM treats this as a memory failure and branches to the error
handler.

After vector setup, the ROM clears working registers and begins warm-start
validation:

```
$F801D0  MOVEQ   #0,D2-D7             ; Clear working registers
$F801DC  MOVEA.L #$021C,A4            ; A4 = constant (ExecBase struct size offset)
$F801E2  MOVE.L  $0004,D1             ; D1 = potential ExecBase
$F801E6  MOVEA.L D1,A6                ; A6 = ExecBase candidate
$F801E8  BTST    #0,D1                ; Bit 0 set? (odd address = invalid)
$F801EC  BNE.S   $F80230              ; Invalid -> cold start

$F801EE  ADD.L   $0026(A6),D1         ; D1 + ChkBase
$F801F2  NOT.L   D1                   ; Should be 0 if valid
$F801F4  BNE.S   $F80232              ; Mismatch -> cold start

$F801F6  LEA     $0022(A6),A0         ; Checksum range start
$F801FA  MOVEQ   #24,D0               ; 25 words
$F801FC  ADD.W   (A0)+,D1             ; Sum words
$F801FE  DBF     D0,$F801FC
$F80202  NOT.W   D1                   ; Should be 0
$F80204  BNE.S   $F80232              ; Mismatch -> cold start

$F80206  MOVE.L  $002A(A6),D0         ; ColdCapture
$F8020A  BEQ.S   $F80218              ; No capture -> skip
$F8020C  MOVEA.L D0,A0
$F80212  CLR.L   $002A(A6)            ; One-shot: clear before jumping
$F80216  JMP     (A0)                 ; Jump through ColdCapture
```

**Emulator implications:** On first power-on, chip RAM is zeros. $000004 = 0
passes the bit-0 test but fails the complement checksum. Cold start is always
taken on first boot.

### Stage 3e: Memory Detection

The cold start path calls a subroutine at $F80C1C for chip RAM detection, then
probes slow RAM at $C00000-$DC0000:

```
$F80234  BSR.W   $F80C1C              ; Chip RAM detection subroutine
$F80238  MOVEA.L D0,A2                ; A2 = detection flags
$F8023A  SUBA.L  A0,A0                ; A0 = $000000
$F8023C  MOVEA.L (A0),A1              ; Save longword at $000000
$F8023E  CLR.L   (A0)                 ; Clear $000000 for aliasing test
```

Chip RAM sizing writes a test pattern ($F2D4B689) at 16KB boundaries and checks
for aliasing:

```
$F80242  MOVE.L  #$F2D4B689,D1        ; Magic test pattern
$F80248  BRA.S   $F8024C
$F8024A  MOVE.L  D0,(A3)              ; Restore previous value
$F8024C  LEA     $4000(A3),A3         ; Next 16KB boundary
$F80250  CMPA.L  #$200000,A3          ; Past 2MB?
$F80256  BEQ.S   $F80266              ; Done
$F80258  MOVE.L  (A3),D0              ; Save current value
$F8025A  MOVE.L  D1,(A3)              ; Write pattern
$F8025E  CMP.L   (A0),D1              ; Aliased to $000000?
$F80260  BEQ.S   $F80266              ; Alias found -> that's the chip RAM size
$F80262  CMP.L   (A3),D1              ; Read back from target?
$F80264  BEQ.S   $F8024A              ; Pattern stuck -> real RAM, continue
```

Slow RAM is probed at $C00000 using custom register mirroring detection. The
probe writes to offset $F09A from the test address (which would be $C0F09A, a
custom chip mirror in A500 address space) and checks whether the write sticks
or aliases to custom chips. A3000/A4000 do not have slow RAM.

After detection, ExecBase is allocated and initialised at the top of the
highest-priority memory region (slow RAM if present, otherwise chip RAM top).

### Stage 4: ExecBase Initialisation

ExecBase is built in memory with the standard Exec library structure:

```
$F802A4  MOVE.L  A6,$0004             ; Store ExecBase pointer at $000004
$F802B6  MOVE.W  (PC,$FD56),$0022(A6) ; SoftVer from ROM header
$F802BC  MOVE.W  A2,$0128(A6)         ; AttnFlags (initial, from chip RAM detect)
$F802C0  MOVE.L  A3,$003E(A6)         ; MaxLocMem (chip RAM top)
$F802C4  MOVE.L  A5,$0026(A6)         ; ChkBase (complement checksum)
```

The ROM then checks for the "HELP" marker at $000000 (used by some debuggers):

```
$F802DA  CMPI.L  #'HELP',$0000        ; $4845_4C50 at address 0?
$F802E2  BNE.S   $F802EE              ; No marker
$F802E4  MOVEM.L $0100,D6-D7          ; Load debug info from $100-$107
$F802EA  BSET    #31,D6               ; Mark as present
```

### Stage 5: CPU Detection and Cache Setup

After ExecBase init, the ROM detects the CPU model via AttnFlags. The detection
has already been partially done; the full check happens in the exec init
subroutine. Key code at $F8068C:

```
$F8068C  MOVE.W  $0128(A6),D0         ; AttnFlags
$F80690  BTST    #0,D0                ; AFF_68010?
$F80694  BEQ.W   $F8071A              ; No -> 68000, skip cache setup

         ; 68010+ detected
$F80698  LEA     (PC,$04A2),A0        ; 68010+ exception handler
$F8069C  MOVEA.W #$0008,A1
$F806A0  MOVE.L  A0,(A1)+             ; Replace bus error vector
$F806A2  MOVE.L  A0,(A1)+             ; Replace address error vector
$F806A4  MOVE.L  #$F80BF8,$0020       ; Privilege violation vector (for Supervisor() trick)
$F806AC  MOVE.L  #$F80BE0,$FFE4(A6)   ; Trap #0 handler in ExecBase

$F806BC  BTST    #1,D0                ; AFF_68020?
$F806C0  BEQ.S   $F8071A              ; No -> done
         ; 68020+ specific setup (scaled index, CACR)

$F806D2  BTST    #2,D0                ; AFF_68030?
$F806D6  BEQ.S   $F806E0              ; No -> skip MMU setup

$F806E0  AND.W   #$0070,D1            ; FPU bits (AFF_68881/68882/FPU40)
$F806E6  BEQ.S   $F806FE              ; No FPU

$F806FE  BTST    #3,D0                ; AFF_68040?
$F80702  BEQ.S   $F8070E              ; No -> skip 68040 cache
$F8070C  CPUSHA  BC                   ; 68040: push and invalidate both caches

$F8070E  MOVEC   CACR,D0              ; Read current CACR
$F80712  OR.W    #$0808,D0            ; Enable instruction + data cache
$F80716  MOVEC   D0,CACR              ; Write back
```

**Emulator implications:**
- The 68EC020 (A1200) must support MOVEC CACR. The A1200 has no MMU, so
  AFF_68020 is set but AFF_68030 is not.
- CPUSHA BC is a 68040 instruction -- on the 68EC020 this code path is not
  taken (BTST #3 fails).
- The CACR write with $0808 enables both instruction and data caches. The
  emulator's cache model (if any) should respond to these bits.

### Stage 6: Resident Module Scan

Exec scans the ROM for RomTag structures ($4AFC match word) starting at
$F80000. The scan visits every even address, validates the rt_MatchTag
self-pointer, and builds a priority-sorted list.

### Stage 7-10: Module Initialisation

Modules are initialised in priority order. See the [Resident Modules](#resident-modules)
section below for the complete list with priorities.

Key initialisation milestones:
1. **exec.library** (pri 110/105) -- library jump table, memory lists
2. **expansion.library** (pri 110) -- Zorro autoconfig scan at $E80000
3. **diag init** (pri 105) -- expansion board driver init
4. **graphics.library** (pri 65) -- chipset detection via DENISEID, copper
   list setup, EClock calibration
5. **card.resource** (pri 48) -- PCMCIA card management (A600/A1200 only)
6. **trackdisk.device** (pri 20) -- floppy disk driver
7. **scsi.device** (pri 10) -- IDE via Gayle (A600/A1200)
8. **intuition.library** (pri 10) -- window/screen manager
9. **strap** (pri -60) -- insert-disk display

### Stage 11: STRAP Display

See [STRAP Display](#strap-display) below.

## Hardware Probing Summary

Every register read/write during the boot sequence, in execution order:

### Pre-checksum (Stage 2)

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $F80000-$FFFFF | R | ROM data | Checksum loop reads all 512K |

### Diagnostic ROM (Stage 3a)

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $F00000 | R | expect != $1111 | Diagnostic ROM magic word check |

### Gayle (Stage 3b)

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $DA8000 | W | $00 | Clear PCMCIA config |
| $DA8000 | W | $01 | Enable normal operation (after PCMCIA probe) |

### PCMCIA Attribute Space (Stage 3c)

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $A00000 | R | expect $91 | CISTPL_DEVICE tuple tag |
| $A00002 | R | expect $05 | Device info byte |
| $A00004 | R | expect $23 | Type byte |
| $A00006 | R | boot address byte 3 | If signature matched |
| $A00008 | R | boot address byte 2 | |
| $A0000A | R | boot address byte 1 | |
| $A0000C | R | boot address byte 0 | |

### CIA Init (Stage 3d)

| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $BFA001 | W | $00 | CIA-B PRA: clear all |
| $BFA201 | W | $00 | CIA-B DDRA: all inputs |
| $BFE001 | W | $00 | CIA-A PRA: OVL=0 (overlay off) |
| $BFE201 | W | $03 | CIA-A DDRA: bits 0,1 outputs (OVL + LED) |

### Custom Chips (Stage 3d)

| Register | Address | Value | Purpose |
|----------|---------|-------|---------|
| INTENA | $DFF09A | $7FFF | Disable all interrupts |
| INTREQ | $DFF09C | $7FFF | Clear all interrupt requests |
| DMACON | $DFF096 | $7FFF | Disable all DMA |
| SERPER | $DFF032 | $0174 | Serial period (9600 baud @ PAL) |
| BPLCON0 | $DFF100 | $0200 | Blank display, colour mode |
| BPLCON3 | $DFF110 | $0000 | Bank 0, no features (AGA/ECS only) |
| COLOR00 | $DFF180 | $0111 | Dark grey background |

### Memory Detection (Stage 3e)

| Address Range | R/W | Purpose |
|---------------|-----|---------|
| $000000-$1FFFFF | R/W | Chip RAM sizing (16KB granularity) |
| $C00000-$DC0000 | R/W | Slow RAM detection |
| $DFF01C (SERDATR) | R | Read during slow RAM probe (alias test) |
| $C0F09A | W | Custom chip mirror test (slow RAM vs chipset alias) |

### Chipset Detection (graphics.library init)

| Register | Address | Purpose |
|----------|---------|---------|
| DENISEID | $DFF07C | R: Chipset ID ($FF=OCS, $FC=ECS, $F8=AGA) |
| VPOSR | $DFF004 | R: PAL/NTSC detection (bit 12) |
| BPLCON3 | $DFF110 | W: AGA palette bank select |
| BPLCON4 | $DFF10C | W: AGA sprite/bitplane colour XOR |
| FMODE | $DFF1FC | W: AGA fetch mode (bandwidth multiplier) |

### Cache Control (68020+)

| Register | Purpose |
|----------|---------|
| CACR | R/W via MOVEC: enable instruction + data cache ($0808) |
| VBR | R/W via MOVEC: relocate exception vector base (68010+) |

## Resident Modules

41 RomTag structures in the A1200 40.068 ROM, sorted by initialisation priority
(highest first):

| Pri | Address | Type | Name | Version | Flags | Init |
|----:|---------|------|------|---------|-------|------|
| 110 | $F837B8 | Library | expansion.library | 40.2 | $02 | $F8388C |
| 105 | $F800B6 | Library | exec.library | 40.10 | $02 | $F804D4 |
| 105 | $F837D2 | -- | diag init | 40 | $01 | $F83BD2 |
| 103 | $FBD428 | Library | utility.library | 40.1 | $81 | $FBD46C |
| 100 | $FC0008 | -- | potgo.resource | 37.4 | $81 | $FC0048 |
| 80 | $FBFA98 | -- | cia.resource | 39.1 | $01 | $FBFB00 |
| 80 | $FC0140 | -- | FileSystem.resource | 40.1 | $01 | $FC01D8 |
| 70 | $FBDDE8 | -- | battclock.resource | 39.3 | $01 | $FBDE72 |
| 70 | $FBFE88 | -- | misc.resource | 37.1 | $01 | $FBFEC8 |
| 70 | $FC02E0 | -- | disk.resource | 37.2 | $01 | $FC030C |
| 69 | $FFFDB0 | -- | battmem.resource | 39.2 | $01 | $FFFE34 |
| 65 | $F851A8 | Library | graphics.library | 40.24 | $01 | $F851EE |
| 64 | $FB1E5C | Library | layers.library | 40.1 | $81 | $FB1E76 |
| 60 | $FBA170 | Device | gameport.device | 40.1 | $81 | $FBA18A |
| 50 | $FC0AC4 | Device | timer.device | 39.4 | $01 | $FC0B42 |
| 48 | $FBC844 | -- | card.resource | 40.4 | $01 | $FBC938 |
| 45 | $FBA19A | Device | keyboard.device | 40.1 | $81 | $FBA1B4 |
| 40 | $FBA1C4 | Device | input.device | 40.1 | $81 | $FBA1DE |
| 40 | $FC2754 | Library | keymap.library | 40.4 | $81 | $FC282E |
| 25 | $FBF4A4 | Device | ramdrive.device | 39.35 | $81 | $FBF4BE |
| 20 | $FC49E4 | Device | trackdisk.device | 40.1 | $01 | $FC4D38 |
| 15 | $FBE750 | Device | carddisk.device | 40.1 | $01 | $FBE8A0 |
| 10 | $FB5040 | Device | scsi.device | 40.12 | $01 | $FB5082 |
| 10 | $FCF498 | Library | intuition.library | 40.85 | $81 | $FCF4B2 |
| 5 | $FAE1D8 | Device | console.device | 40.2 | $81 | $FAE1F2 |
| 0 | $FC18B4 | Library | mathieeesingbas.library | 40.4 | $01 | $FC1AD8 |
| -35 | $FC3414 | -- | syscheck | 40 | $01 | $FC416A |
| -40 | $F84290 | -- | romboot | 40 | $01 | $F842F4 |
| -50 | $FC33D4 | -- | bootmenu | 40.5 | $01 | $FC4222 |
| -55 | $F83706 | -- | alert.hook | 40 | $01 | $F83664 |
| -60 | $F842AA | -- | strap | 40.1 | $01 | $F84514 |
| -81 | $FA825E | -- | filesystem | 40.1 | $00 | $FA823C |
| -100 | $FBF080 | -- | ramlib | 40.2 | $04 | $FBF0B8 |
| -120 | $F9F2B8 | Library | dos.library | 40.3 | $00 | $F9F2E6 |
| -120 | $FBB7A4 | Device | audio.device | 37.10 | $80 | $FBB7E4 |
| -120 | $FBFF3A | Resource | workbench.task | 39.1 | $00 | $FBFF54 |
| -120 | $FC0628 | Library | mathffp.library | 40.1 | $80 | $FC066C |
| -120 | $FC66EC | Library | icon.library | 40.1 | $80 | $FC672C |
| -120 | $FE8E14 | Library | gadtools.library | 40.4 | $80 | $FE8E5A |
| -120 | $FEE9C0 | Library | workbench.library | 40.5 | $80 | $FEE9DA |
| -121 | $FB79B2 | -- | con-handler | 40.2 | $00 | $FB85A8 |
| -122 | $FCAFEC | -- | shell | 40.2 | $00 | $FCAFE4 |
| -123 | $FC8B88 | -- | ram-handler | 39.4 | $00 | $FC8B1C |

### Modules New in KS 3.1 vs KS 1.3

KS 1.3 has ~20 resident modules. KS 3.1 adds these:

| Module | Purpose |
|--------|---------|
| utility.library | Tag item parsing, hook calls, date/time |
| FileSystem.resource | Filesystem handler registry |
| battclock.resource | Battery-backed clock (RTC) |
| battmem.resource | Battery-backed configuration memory |
| card.resource | PCMCIA card hotplug management (A600/A1200) |
| carddisk.device | PCMCIA storage device driver |
| scsi.device | IDE disk via Gayle (A600/A1200) |
| gameport.device | Joystick/mouse input (was inline in KS 1.3) |
| ramdrive.device | RAM disk |
| keymap.library | Keymap database |
| mathieeesingbas.library | IEEE single-precision FP |
| mathffp.library | Motorola Fast Floating Point |
| syscheck | System configuration validation |
| bootmenu | Early boot menu (hold both mouse buttons) |
| icon.library | Workbench icon management |
| gadtools.library | Standard gadget toolkit |
| workbench.library | Workbench desktop management |
| workbench.task | Workbench startup task |
| con-handler | Console I/O handler |
| ram-handler | RAM: filesystem handler |
| shell | AmigaShell command interpreter |
| filesystem | Fast File System |
| alert.hook | Alert display (separated from exec in KS 2.0+) |

## STRAP Display

The STRAP module (pri -60, init at $F84514) creates the insert-disk screen.

### Display Configuration

- BPLCON0 = $8303 (hires, 3 bitplanes, colour, genlock audio, no interlace)
  - Bits: HIRES ($8000) + BPU=3 ($0300) + COLOR ($0002) + GAUD ($0001)
  - Boot tests assert $8303 for AGA (both KS 3.0 and 3.1 A1200)
- DMACON set bits = $03C0 (BPLEN + COPEN + BLTEN + SPREN)
- 3-plane hires display = 8 colours

### Visual Content

The KS 3.1 STRAP display shows:
- Rainbow gradient checkmark icon (top-left area)
- Floppy disk icon
- ROM version text (e.g. "3.1 Roms (40.068)")
- Dark background

The copper list sets up:
- DIWSTRT/DIWSTOP for a standard PAL/NTSC window
- BPL1PT-BPL3PT pointing to bitmap data in chip RAM
- Colour palette with rainbow gradient entries (used for the checkmark)

### STRAP Init Flow

1. Allocate chip RAM for the screen bitmap (via exec AllocMem, type MEMF_CHIP)
2. If allocation fails, trigger Alert $3003800A
3. Open graphics.library (via exec OpenLibrary)
4. Query display capabilities from GfxBase
5. Build copper list with BPLCON0=$8303, palette, and bitplane pointers
6. Set COP1LC and trigger COPJMP1
7. Enable DMA: DMACON = $83C0 (SET + BPLEN + COPEN + BLTEN + SPREN)
8. Draw the checkmark, floppy icon, and version text using blitter operations
9. Enter disk-wait loop

### Emulator Assertions

From `crates/machine-amiga/tests/boot_aga.rs`:

| Test | DMACON (set bits) | BPLCON0 (exact) |
|------|-------------------|-----------------|
| KS 3.0 A1200 | $03C0 | $8303 |
| KS 3.1 A1200 | $03C0 | $8303 |

A4000 tests capture screenshots only -- boot does not yet reach STRAP in the
emulator.

## Variant Differences

### Common Early Boot Code

All 7 KS 3.1 variants (plus the MapROM build) share **identical** entry code
from $F800D2 through the PCMCIA probe and CIA init. The Gayle write at $DA8000
and PCMCIA probe at $A00000 are present in every variant, including the OCS
build for A500/A600/A2000. On machines without Gayle, these writes go to
unmapped space.

This means the entry code is machine-independent. The variant-specific behaviour
comes later, during resident module init (particularly graphics.library and
card.resource).

### CD32 (40.060)

- Same early boot as A1200
- **card.resource** handles Akiko chip detection and CD-ROM drive
- No keyboard.device equivalent for standard keyboard (CD32 uses joypad)
- Akiko chip at $B80000 provides chunky-to-planar conversion and CD-ROM control
- graphics.library detects AGA via DENISEID = $F8

### A500/A600/A2000 OCS (40.063)

- Same early boot code (Gayle/PCMCIA probe silently ignored on OCS machines)
- graphics.library detects OCS via DENISEID = $FF (no ID register)
- No AGA palette bank switching (BPLCON3 bank select writes ignored by OCS)
- STRAP display uses hires 3-plane format (BPLCON0 = $8302) with OCS palette
- A600 has Gayle -- $DA8000 write and PCMCIA probe work correctly
- scsi.device and carddisk.device present but inactive without Gayle

### A3000 (40.068)

- Same early boot code
- **RAMSEY** controller at $DE0000 for 32-bit fast RAM
- **Fat Gary** address decoder
- Diagnostic ROM at $F00000 is meaningful (A3000 has a diagnostic ROM socket)
- 68030 or 68040 CPU -- MMU instructions work, PMOVE responds
- graphics.library detects ECS via DENISEID = $FC
- DMAC 390537 SCSI at $DD0000 for SCSI disk

### A4000 (40.068)

- Same early boot code
- 68040 CPU -- CPUSHA BC and 68040 CACR bits used
- AGA chipset (DENISEID = $F8)
- No PCMCIA -- probe at $A00000 returns nothing, card.resource inactive
- IDE via A4000-specific Gayle variant (different register layout from A1200)
- No RAMSEY (A4000 uses a different memory controller)

### A4000T (40.070)

- Essentially A4000 with tower form factor
- Additional SCSI support
- Beta-quality build

### MapROM A600 Beta (40.068)

- PC = $002000D2 instead of $F800D2
- Entry code is position-independent (PC-relative), so the same instructions
  execute from $200000
- Requires a MapROM hardware adapter to place ROM at $200000
- Not bootable in standard emulator configuration

## Error Paths

### ROM Checksum Failure ($F80408)

If the ROM checksum loop produces a sum other than $FFFFFFFF, execution branches
to $F80408:

```
$F80404  MOVE.W  #$0FE5,D0            ; Colour value (bright pink/magenta)
$F80408  LEA     $DFF000,A4
$F8040E  MOVE.W  #$0200,$0100(A4)     ; BPLCON0 = $0200 (blank)
$F80414  MOVE.W  #$0000,$0110(A4)     ; BPLCON3 = $0000
$F8041A  MOVE.W  D0,$0180(A4)         ; COLOR00 = error colour
```

The handler then flashes the power LED by toggling CIA-A PRA bit 1:

```
$F80420  MOVEQ   #-1,D0               ; Inner delay counter
$F80422  BSET    #1,$BFE001           ; LED off
$F8042A  DBF     D0,$F80422           ; Delay
$F80430  BCLR    #1,$BFE001           ; LED on
$F80438  DBF     D0,$F80430           ; Delay
$F8043C  DBF     D1,$F80422           ; Outer loop (11 flashes)
```

After flashing, the screen goes black momentarily, then enables the PORTS
interrupt and jumps to $F80DB8 (which likely restarts or halts):

```
$F80446  MOVE.W  #$0000,$DFF180       ; COLOR00 = black
$F8044E  SUBQ.L  #1,D0               ; Delay ~86K iterations
$F80450  BGT.S   $F80446
$F80452  MOVE.W  #$4000,$DFF09A       ; INTENA: enable PORTS interrupt
$F8045A  BRA.W   $F80DB8              ; Jump to restart/halt
```

### RAM Verification Failure

If the exception vector write-back test fails at $F801C8, the same error
handler at $F80408 is entered with D0 = $00F0 (colour value = blue-green).

### Alert Codes

Standard Exec alert codes displayed via the alert.hook module:

| Code | Meaning | Typical Cause |
|------|---------|---------------|
| $01000005 | AG_NoMemory + AO_ExecLib | Not enough RAM for ExecBase |
| $3001xxxx | STRAP graphics failure | Can't allocate chip RAM for display |
| $3003800A | STRAP: can't open screen | Screen memory allocation failure |
| $3003800C | STRAP: can't open window | Window allocation failure |
| $8001xxxx | Dead-end alert | Various fatal conditions |

## Emulator Implications

### What Must Work for Each Boot Stage

| Stage | Critical Hardware |
|-------|-------------------|
| Reset vectors | Overlay latch maps ROM at $000000 |
| ROM checksum | 68EC020 executes ADD.L, BCC, ADDQ, DBF correctly for 262K iterations |
| Diagnostic ROM | $F00000 reads as non-$1111 |
| Gayle init | $DA8000 accepts byte writes (or silently ignores on non-Gayle) |
| PCMCIA probe | $A00000 reads return non-$91 (no card) or valid CIS data |
| CIA init | $BFE001/$BFE201/$BFA001/$BFA201 byte writes work; overlay clears immediately |
| Custom chip reset | INTENA/INTREQ/DMACON/BPLCON0 register writes take effect |
| Exception vectors | Chip RAM at $008-$0BC is writable and readable |
| Memory detect | Chip RAM aliasing at 16KB boundaries works correctly |
| ExecBase init | Longword write to $000004 sticks |
| CPU detect | MOVEC CACR works on 68020; F-line exception on 68000 |
| Module scan | All even words in ROM readable without bus errors |
| graphics.library | DENISEID ($DFF07C) returns correct chipset ID |
| | VPOSR ($DFF004) bit 12 correct for PAL/NTSC |
| | CIA timers advance at correct rate (EClock calibration) |
| card.resource | Gayle PCMCIA registers at $DA8000+ respond correctly |
| keyboard.device | CIA-A SP ($BFEC01) serial handshake works |
| trackdisk.device | CIA-B motor/step signals, DSKLEN DMA, MFM decode |
| scsi.device | Gayle IDE registers at $DA0000 respond (or timeout gracefully) |
| STRAP display | Copper DMA, bitplane DMA, blitter operations |

### AGA-Specific Requirements

For the A1200 boot to reach the STRAP display, these AGA features must work:

1. **DENISEID** ($DFF07C) must return $F8 (Lisa / AGA)
2. **BPLCON3** ($DFF110) bank select must route palette writes to the correct
   256-entry bank
3. **BPLCON4** ($DFF10C) sprite/bitplane colour XOR must not corrupt display
4. **FMODE** ($DFF1FC) must accept writes (even if fetch width is ignored)
5. **24-bit palette**: COLOR registers accept 8-bit-per-channel values when
   BPLCON3 LOCT is set

### Gayle/PCMCIA Requirements

1. $DA8000 byte writes accepted (Gayle config register)
2. $A00000-$A0FFFF PCMCIA attribute space readable
3. Without a PCMCIA card: $A00000 returns non-$91
4. IDE registers at $DA0000-$DA3FFF respond or timeout

### Key Differences from KS 1.3 Boot

| Feature | KS 1.3 | KS 3.1 |
|---------|--------|--------|
| ROM checksum | None | Full 512K sum before any hardware access |
| Gayle probe | None | Write to $DA8000 |
| PCMCIA probe | None | Read signature at $A00000 |
| BPLCON3 write | None | $DFF110 = $0000 during custom chip reset |
| Chip RAM detect | Same algorithm | Same algorithm |
| Slow RAM detect | Same | Same, but more models supported |
| CPU detect | 68000/010/020 | 68000/010/020/030/040/060 |
| Cache control | None | CACR enable on 68020+ |
| Resident modules | ~20 | 41 |
| STRAP display | 2-plane lowres | 3-plane hires |
| Alert system | Inline | Separate alert.hook module |
| PCMCIA storage | None | carddisk.device |
| IDE storage | None | scsi.device via Gayle |
| Boot menu | None | bootmenu (hold both mouse buttons) |
