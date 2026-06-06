# Kickstart 1.2 Boot Flow

Kickstart 1.2 is the first Amiga ROM that introduced warm start validation and
expanded machine support beyond the A1000. Two builds exist: 33.166 for the
A1000 only, and 33.180 for the A500, A1000, and A2000. Both follow the same boot
flow described in [boot-flow-overview.md](boot-flow-overview.md), and this
document focuses on KS 1.2-specific details.

## ROM Identification

| Field | 33.166 (A1000) | 33.180 (A500/A1000/A2000) |
|-------|----------------|---------------------------|
| File | `kick12_33_166_a1000.rom` | `kick12_33_180_a500_a1000_a2000.rom` |
| Size | 256 KB | 256 KB |
| Mapped at | $FC0000–$FFFFFF | $FC0000–$FFFFFF |
| SSP | $11114EF9 | $11114EF9 |
| PC | $FC00D2 | $FC00D2 |
| exec version | exec 33.189 (30 Sep 1986) | exec 33.192 (8 Oct 1986) |
| Copyright | Copyright (C) 1985, Commodore-Amiga, Inc. | Same |
| Target machines | A1000 | A500, A1000, A2000 |

Both are 256 KB ROMs with identical SSP and PC values. The 33.180 build is 8
days newer and contains updated versions of several modules (graphics, layers,
trackdisk, workbench, strap).

## Boot Flow

The entry point at $FC00D2 is identical between the two builds. The flow follows
the stages defined in boot-flow-overview.md. This section annotates each stage
with KS 1.2-specific addresses and behaviour.

### Stage 1: Reset Vector Fetch

SSP = $11114EF9 (unmapped space), PC = $FC00D2. The overlay latch maps ROM at
$000000 so the 68000 reads these values from the ROM header.

### Stage 2: Initial Setup ($FC00D2–$FC0148)

```
$FC00D2  LEA     $40000,SP          ; Temporary stack at 256K
$FC00D8  MOVE.L  #$20000,D0         ; Delay counter = 131072
$FC00DE  SUBQ.L  #1,D0              ; Busy-wait loop
$FC00E0  BGT.S   $FC00DE            ; ~130K iterations for hardware settle
```

After the delay, the ROM checks for a diagnostic ROM at $F00000:

```
$FC00E2  LEA     (PC,$FC0000),A0    ; ROM base via PC-relative
$FC00E6  LEA     $F00000,A1         ; Diagnostic ROM address
$FC00EC  CMPA.L  A1,A0              ; Are we running from $F00000?
$FC00EE  BEQ.S   $FC00FE            ; If so, skip diagnostic check
$FC00F0  LEA     (PC,$FC00FE),A5    ; Return address
$FC00F4  CMPI.W  #$1111,(A1)        ; Diagnostic ROM magic word?
$FC00F8  BNE.S   $FC00FE            ; No diagnostic ROM — continue
$FC00FA  JMP     2(A1)              ; Jump to diagnostic ROM entry
```

Then the overlay is cleared and custom chips are reset:

```
$FC00FE  MOVE.B  #$03,$BFE201       ; CIA-A DDRA: OVL + LED as outputs
$FC0106  MOVE.B  #$02,$BFE001       ; CIA-A PRA: OVL=0 (overlay off), LED off
$FC010E  LEA     $DFF000,A4         ; Custom chip base
$FC0114  MOVE.W  #$7FFF,$DFF09A     ; INTENA: disable all interrupts
$FC0118  MOVE.W  #$7FFF,$DFF09C     ; INTREQ: clear all interrupt requests
$FC0120  MOVE.W  #$7FFF,$DFF096     ; DMACON: disable all DMA
$FC0124  MOVE.W  #$0200,$DFF100     ; BPLCON0: blank display, colour
$FC012A  MOVE.W  #$0000,$DFF110     ; BPLCON1: no scroll offset
$FC0130  MOVE.W  #$0444,$DFF180     ; COLOR00: dark grey background
```

Exception vectors $008–$0BC are filled with a pointer to the alert handler
(33.166: $FC05AC, 33.180: $FC05B4). Then the code branches to the warm/cold
start detection.

### Stage 2b: Warm/Cold Start Detection ($FC30A8/$FC30C4)

This is the warm start validation that KS 1.2 introduced. KS 1.0 had no warm
start support — every reset was a cold start.

```
$FC30C4  MOVE.L  #$FFFFFFFF,D6      ; D6 = "no warm start" marker
$FC30CA  CMPI.L  #'HELP',$000000    ; Check for "HELP" magic at $000000
$FC30D2  BNE.W   $FC014C            ; Not "HELP" — proceed to validation
$FC30D6  CLR.L   $000000            ; Clear the magic word (one-shot)
$FC30DA  MOVEM.L $000100,D6-D7      ; Load warm start data from $100
$FC30E0  BRA.W   $FC014C            ; Continue to ExecBase validation
```

The "HELP" magic word at $000000 is an A1000 WCS (Writable Control Store) or
diagnostic mechanism. If present, the ROM loads D6-D7 from $000100 and clears
the magic word before continuing.

After the branch, the code at $FC014C performs the ExecBase validation described
in boot-flow-overview.md Stage 2:

```
$FC014C  MOVE.L  $000004,D0         ; Read ExecBase pointer
$FC0150  BTST    #0,D0              ; Bit 0 clear? (word-aligned)
$FC0154  BNE.S   $FC01CE            ; Not aligned — cold start
$FC0156  MOVEA.L D0,A6              ; A6 = ExecBase
$FC0158  ADD.L   38(A6),D0          ; D0 = ExecBase + ChkBase
$FC015C  NOT.L   D0                 ; Complement — should be 0
$FC015E  BNE.S   $FC01CE            ; Not zero — cold start
```

If the complement check passes, the code sums 25 words from ExecBase+$22 to
ExecBase+$52 (SoftVer through ChkSum):

```
$FC0160  MOVEQ   #0,D1              ; Clear checksum accumulator
$FC0162  LEA     34(A6),A0          ; A0 = ExecBase+$22
$FC0166  MOVEQ   #24,D0             ; 25 words (0–24)
$FC0168  ADD.W   (A0)+,D1           ; Sum each word
$FC016A  DBF     D0,$FC0168
$FC016E  NOT.W   D1                 ; Complement — should be 0
$FC0170  BNE.S   $FC01CE            ; Not zero — cold start
```

Then ColdCapture is checked:

```
$FC0172  MOVE.L  42(A6),D0          ; ExecBase+$2A = ColdCapture
$FC0176  BEQ.S   $FC0184            ; Zero — skip
$FC0178  MOVEA.L D0,A0              ; Target address
$FC017A  LEA     (PC,$FC0184),A5    ; Return address
$FC017E  CLR.L   42(A6)             ; Clear ColdCapture (one-shot)
$FC0182  JMP     (A0)               ; Jump through ColdCapture
```

After ColdCapture, the code validates the LED toggle, SoftVer, MaxLocMem,
KickMemPtr, and KickTagPtr before deciding warm or cold. If any check fails,
execution falls through to the cold start at $FC01CE.

### Stage 3: Cold Start — Memory Detection ($FC01CE–$FC0240)

```
$FC01CE  LEA     $000400,A6         ; Temporary ExecBase at $400
$FC01D2  SUBA.W  #-$0276,A6         ; A6 += $276 (adjust for negative offsets)
```

**Slow RAM detection** happens first, probing $C00000–$DC0000:

```
$FC01D6  LEA     $C00000,A0         ; Slow RAM start
$FC01DC  LEA     $DC0000,A1         ; Slow RAM end
$FC01E2  LEA     (PC,$FC01EA),A5    ; Return address
$FC01E6  BRA.W   $FC061A            ; Call slow RAM probe subroutine
```

The probe subroutine at $FC061A writes INTREQ ($DFF09C) to mask interrupts, then
tests each 256 KB block by writing $3FFF and $BFFF to even addresses and
comparing reads. A4 returns the first byte past detected slow RAM (or equals A0
if none found, then zeroed).

If slow RAM is found, ExecBase and SSP are placed there:

```
$FC01EA  MOVE.L  A4,D0              ; A4 = slow RAM top (or 0)
$FC01EC  BEQ.S   $FC0208            ; No slow RAM — skip
$FC01EE  MOVEA.L #$C00000,A6        ; ExecBase goes in slow RAM
$FC01F4  SUBA.W  #-$0276,A6         ; Adjust for negative offsets
```

**Chip RAM detection** follows, probing $000000–$200000:

```
$FC0208  LEA     $000000,A0         ; Chip RAM start
$FC020C  LEA     $200000,A1         ; Chip RAM end (2 MB max)
$FC0212  LEA     (PC,$FC021A),A5    ; Return address
$FC0216  BRA.W   $FC0592            ; Call chip RAM size subroutine
```

The chip RAM sizer at $FC0592 writes a test pattern ($F2D4B698) at 4 KB
intervals and checks for wrap-around to determine size. Result in A3.

If chip RAM is less than 256 KB, the ROM jumps to the alert handler (red
screen). Otherwise it clears $000000 and proceeds:

```
$FC021A  CMPA.L  #$40000,A3         ; At least 256K?
$FC0220  BCS.S   $FC0238            ; No — fatal alert
$FC0222  MOVE.L  #0,$000000         ; Clear chip RAM base
$FC022A  MOVE.L  A3,D0              ; Chip RAM size
$FC022C  LEA     $C0,A0             ; Clear from $C0 upward
$FC0230  LEA     (PC,$FC0240),A5    ; Return address
$FC0234  BRA.W   $FC0602            ; Zero-fill memory
```

### Stage 4: ExecBase Initialisation ($FC0240)

Custom chips are reset again (DMACON, BPLCON0/1, COLOR00 set to $0888), then
ExecBase fields from offset $54 onward are zeroed (126 longwords). D6-D7 from
the warm start detection are stored at ExecBase+$202. ExecBase pointer goes to
$000004 with complement checksum at ExecBase+$26.

SSP is set from A4 (slow RAM top) if available, otherwise from A3 (chip RAM
top). ExecBase fields:

- ExecBase+$36: top of SSP region
- ExecBase+$3A: bottom of SSP region (top minus 6144 bytes)
- ExecBase+$3E: MaxLocMem (chip RAM size in A3)
- ExecBase+$4E: KickMemPtr (slow RAM top in A4)

The code then calls the CPU detection and resident module scan (Stage 5).

### Stage 5: Resident Module Scan

The RomTag scan begins at $FC0000 and searches every even word for $4AFC,
validating rt_MatchTag back-pointers. Modules are sorted by priority and
initialised in descending order.

## Resident Modules

### 33.166 (A1000)

| Priority | Address | Module | Version | Type |
|----------|---------|--------|---------|------|
| 120 | $FC00B6 | exec.library | exec 33.189 (30 Sep 1986) | Library |
| 110 | $FC4AE0 | expansion.library | expansion 33.121 (4 May 1986) | Library |
| 100 | $FE49E8 | potgo.resource | potgo 33.7 (10 Feb 1986) | Resource |
| 100 | $FE514C | keymap.resource | keymap ri 33.103 (1 Aug 1986) | Resource |
| 80 | $FC44F0 | cia.resource | cia 33.22 (29 Mar 1986) | Resource |
| 70 | $FC4778 | disk.resource | disk 33.18 (6 May 1986) | Resource |
| 70 | $FE48DC | misc.resource | misc 32.10 (16 Jan 1986) | Resource |
| 70 | $FE4B34 | ramlib.library | ramlib 33.90 (7 Jul 1986) | Library |
| 65 | $FC535C | graphics.library | graphics 33.89 (25 Sep 1986) | Library |
| 60 | $FE5196 | keyboard.device | keyboard ri 33.103 (1 Aug 1986) | Device |
| 60 | $FE51E2 | gameport.device | gameport ri 33.103 (1 Aug 1986) | Device |
| 50 | $FE9214 | timer.device | timer 33.54 (31 Mar 1986) | Device |
| 40 | $FC34B0 | audio.device | audio 33.4 (9 Jun 1986) | Device |
| 40 | $FE522E | input.device | input ri 33.103 (1 Aug 1986) | Device |
| 31 | $FE0F48 | layers.library | layers 33.31 (23 Jul 1986) | Library |
| 20 | $FE5276 | console.device | console ri 33.103 (1 Aug 1986) | Device |
| 20 | $FE9A0C | trackdisk.device | trackdisk 33.126 (9 Jul 1986) | Device |
| 10 | $FD4114 | intuition.library | intuition 33.702 (30 Sep 1986) | Library |
| 5 | $FC321E | alert.hook | — | — |
| 0 | $FE43B4 | mathffp.library | mathffp 33.7 (6 May 1986) | Library |
| 0 | $FEB528 | workbench.task | wb 33.752 (30 Sep 1986) | Task |
| 0 | $FF425A | dos.library | dos 33.124 (11 Sep 1986) | Library |
| −60 | $FE89EC | strap | strap 33.84 (16 Apr 1986) | — |

### 33.180 (A500/A1000/A2000)

| Priority | Address | Module | Version | Type |
|----------|---------|--------|---------|------|
| 120 | $FC00B6 | exec.library | exec 33.192 (8 Oct 1986) | Library |
| 110 | $FC4AFC | expansion.library | expansion 33.121 (4 May 1986) | Library |
| 100 | $FE4880 | potgo.resource | potgo 33.7 (10 Feb 1986) | Resource |
| 100 | $FE4FE4 | keymap.resource | keymap ri 33.103 (1 Aug 1986) | Resource |
| 80 | $FC450C | cia.resource | cia 33.22 (29 Mar 1986) | Resource |
| 70 | $FC4794 | disk.resource | disk 33.18 (6 May 1986) | Resource |
| 70 | $FE4774 | misc.resource | misc 32.10 (16 Jan 1986) | Resource |
| 70 | $FE49CC | ramlib.library | ramlib 33.90 (7 Jul 1986) | Library |
| 65 | $FC5378 | graphics.library | graphics 33.97 (8 Oct 1986) | Library |
| 60 | $FE502E | keyboard.device | keyboard ri 33.103 (1 Aug 1986) | Device |
| 60 | $FE507A | gameport.device | gameport ri 33.103 (1 Aug 1986) | Device |
| 50 | $FE90EC | timer.device | timer 33.54 (31 Mar 1986) | Device |
| 40 | $FC34CC | audio.device | audio 33.4 (9 Jun 1986) | Device |
| 40 | $FE50C6 | input.device | input ri 33.103 (1 Aug 1986) | Device |
| 31 | $FE0D90 | layers.library | layers 33.33 (2 Oct 1986) | Library |
| 20 | $FE510E | console.device | console ri 33.103 (1 Aug 1986) | Device |
| 20 | $FE98E4 | trackdisk.device | trackdisk 33.127 (8 Oct 1986) | Device |
| 10 | $FD3F5C | intuition.library | intuition 33.702 (30 Sep 1986) | Library |
| 5 | $FC323A | alert.hook | — | — |
| 0 | $FE424C | mathffp.library | mathffp 33.7 (6 May 1986) | Library |
| 0 | $FEB400 | workbench.task | wb 33.771 (8 Oct 1986) | Task |
| 0 | $FF425A | dos.library | dos 33.124 (11 Sep 1986) | Library |
| −60 | $FE8884 | strap | strap 33.97 (1 Oct 1986) | — |

### Module Differences Between Builds

Both ROMs contain the same 23 resident modules at identical priorities. Module
addresses differ throughout because the code was relinked. The modules that
changed between builds:

| Module | 33.166 version | 33.180 version |
|--------|---------------|---------------|
| exec.library | 33.189 (30 Sep 1986) | 33.192 (8 Oct 1986) |
| graphics.library | 33.89 (25 Sep 1986) | 33.97 (8 Oct 1986) |
| layers.library | 33.31 (23 Jul 1986) | 33.33 (2 Oct 1986) |
| trackdisk.device | 33.126 (9 Jul 1986) | 33.127 (8 Oct 1986) |
| workbench.task | 33.752 (30 Sep 1986) | 33.771 (8 Oct 1986) |
| strap | 33.84 (16 Apr 1986) | 33.97 (1 Oct 1986) |

The remaining 17 modules (expansion, cia, disk, misc, potgo, ramlib, keymap,
keyboard, gameport, timer, audio, input, console, intuition, alert.hook,
mathffp, dos) have identical version strings.

### Differences from KS 1.3

KS 1.2 and KS 1.3 share the same module list and priority ordering. The
differences are version bumps across all modules (33.x to 34.x) and bug fixes.
The boot flow structure is identical — same stages, same order, same subroutine
calling conventions.

Notable structural differences from KS 1.3 (34.005):

- **exec.library priority**: 120 in KS 1.2, 126 in KS 1.3
- **graphics.library priority**: 65 in KS 1.2, 100 in KS 1.3 (moved above
  potgo and keymap)
- **potgo.resource priority**: 100 in KS 1.2, reduced in KS 1.3
- **layers.library priority**: 31 in KS 1.2, changed in KS 1.3

The priority reordering in KS 1.3 means graphics.library initialises earlier in
the boot sequence, which matters for display timing calibration.

## Variant Differences (33.166 vs 33.180)

The two builds differ by 200,269 bytes out of 262,144 (76.4%). This is not a
patch — it is a complete relink with updated source code.

The only shared region larger than 256 bytes is a ~48 KB block in the DOS and
file system code near the end of the ROM ($FF4000+), where both builds use
nearly identical dos.library 33.124 code.

**Structural differences:**

- Entry code ($FC00D2–$FC0148) is byte-identical except for two addresses: the
  alert handler target (33.166: $FC05AC, 33.180: $FC05B4) and the warm/cold
  start branch target (33.166: $FC30A8, 33.180: $FC30C4). Both change because
  the code between them shifted by the size difference of earlier modules.
- Warm start detection, memory sizing, custom chip reset, and ExecBase init
  follow the same algorithm at different addresses.
- The RomTag structure and init flow are identical.

**Why they differ so much:** The A500/A2000 build (33.180) includes updated
graphics, layers, trackdisk, workbench, and strap modules. Because Amiga ROMs
are statically linked with absolute addresses, changing any module shifts all
subsequent code, causing a ripple effect across the entire ROM image.

**For emulation purposes, both ROMs behave identically.** The boot flow, hardware
probing, and STRAP display all follow the same code paths. The only functional
difference is which machine models are officially supported (cosmetic — the ROM
does not check machine type at boot).

## Hardware Probing

KS 1.2 probes the same hardware as KS 1.3, in the same order. The probing
sequence during cold start:

1. **Diagnostic ROM** at $F00000 — check for $1111 magic word
2. **CIA-A** — set DDRA and PRA to clear overlay and LED
3. **Custom chips** — disable all DMA and interrupts
4. **Slow RAM** — probe $C00000–$DC0000 in 256 KB steps using INTREQ register
   writes to mask interrupts during the probe. Each step writes $3FFF then
   $BFFF to an address and reads back to detect wrap/absence.
5. **Chip RAM** — probe $000000–$200000 in 4 KB steps using test pattern
   $F2D4B698
6. **Expansion** — expansion.library scans Zorro II autoconfig at $E80000

### Differences from KS 1.3

No structural differences in hardware probing. KS 1.2 and 1.3 use the same
probe subroutines and the same test patterns.

## Error Paths

### Alert Handler ($FC05B4 in 33.180, $FC05AC in 33.166)

The early alert handler (used before exec's alert system is initialised) sets
BPLCON0 to $0200 (blank), clears scroll, and writes the error code to COLOR00.
It then flashes the power LED (CIA-A PRA bit 1) in a loop, alternating BSET and
BCLR with nested DBF delay loops (outer loop 10 iterations, inner loop 65536).
After the flash sequence, it executes a 130K-iteration delay, issues a RESET
instruction, reads ExecBase from $000004, and jumps to it (soft reboot).

The flash sequence:

```
$FC05CE  MOVEQ   #10,D1             ; Outer loop count
$FC05D0  MOVEQ   #-1,D0             ; Inner loop count (65535)
$FC05D2  BSET    #1,$BFE001         ; LED off
$FC05DA  DBF     D0,$FC05D2         ; Inner delay
$FC05DE  LSR.W   #1,D0              ; Half the count
$FC05E0  BCLR    #1,$BFE001         ; LED on
$FC05E8  DBF     D0,$FC05E0         ; Shorter delay (asymmetric flash)
$FC05EC  DBF     D1,$FC05D2         ; Outer loop
```

The asymmetric timing (full count for LED-off, half count for LED-on) produces a
distinctive short-flash pattern visible on real hardware.

After flashing, the handler performs a soft reboot:

```
$FC05F0  MOVE.L  #$20000,D0         ; Delay 131072 iterations
$FC05F6  SUBQ.L  #1,D0
$FC05F8  BGT.S   $FC05F6
$FC05FA  RESET                      ; Hardware reset
$FC05FC  MOVEA.L $000004,A0         ; Read ExecBase
$FC0600  JMP     (A0)               ; Attempt warm start
```

### Fatal Alert Codes

KS 1.2 uses the same alert code scheme as KS 1.3:

- $01000005: AG_NoMemory | AO_ExecLib — not enough memory for exec init
- $30010000: AN_MemCorrupt — memory list corruption
- $30070000: AN_TMBadReq — trackdisk bad request
- $30040000: AN_OpenScreen — can't open screen (STRAP display failure)

The formatted alert display ("Guru Meditation #%08lx.%08lx") is at $FC31FE
(33.180) / $FC31E2 (33.166). It requires exec, intuition, and graphics to be
initialised, so it only appears for errors after Stage 7.

## STRAP Display

STRAP (System Test and Registration Program) is the insert-disk screen module,
priority −60 (last to initialise).

The STRAP init function (33.180 at $FE88D6, 33.166 at $FE8A3E):

1. Allocates 1160 bytes of memory (MEMF_CHIP | MEMF_CLEAR)
2. If allocation fails, fires alert $30010000 (dead-end)
3. Opens graphics.library
4. If open fails, fires alert $30070000
5. Opens a screen via intuition (OpenScreen with depth=5, width=13)
6. If screen open fails, fires alert $30048014
7. Disables DMA (DMACON = $0100), sets up the copper list with 2-plane lowres
   display
8. Draws the checkmark icon and "Insert disk" text using blitter operations
9. Enables DMA and enters the disk-wait loop

The STRAP display registers:

| Register | Value | Meaning |
|----------|-------|---------|
| BPLCON0 | $2302 | 2 bitplanes, lowres, colour on |
| DMACON | $83C0+ | BLTPRI, DMAEN, BPLEN, COPEN, BLTEN enabled |
| COLOR00 | $0444 | Dark grey background (during boot, then STRAP palette) |

The insert-disk screen on KS 1.2 is visually identical to KS 1.3: a hand-drawn
checkmark icon with "Insert disk" text in a lowres 2-plane display.

### Differences from KS 1.3

The STRAP display is structurally the same. KS 1.2 strap 33.84/33.97 and KS 1.3
strap 34.x produce the same visual result with the same register state. No
display differences are visible between the versions.

## Warm Start Validation

KS 1.2 introduced warm start validation. KS 1.0 treated every reset as a cold
start. The warm start mechanism in KS 1.2 is identical to the one documented in
boot-flow-overview.md Stage 2 and is preserved unchanged in KS 1.3.

**The "HELP" mechanism** at $FC30C4 (33.180) / $FC30A8 (33.166):

Before checking ExecBase, the ROM checks if the longword at $000000 equals
"HELP" ($48454C50). This is a diagnostic aid — external hardware (A1000 WCS or
diagnostic boards) can write "HELP" to $000000 and place context data at $000100
to request the ROM save D6-D7 before validation. The magic word is cleared
immediately (one-shot) to prevent loops.

**Validation steps** (same as boot-flow-overview.md, repeated here with KS 1.2
addresses):

1. Read ExecBase from $000004 — must be word-aligned (bit 0 clear)
2. ExecBase + ExecBase->ChkBase ($26) must complement to $FFFFFFFF
3. Sum 25 words from ExecBase+$22 to ExecBase+$52 — complement must be zero
4. If ExecBase->ColdCapture ($2A) is non-zero, clear it and jump through it
5. Toggle LED (visual indicator of warm start attempt)
6. Compare SoftVer at ExecBase+$14 against ROM's stored version
7. Validate MaxLocMem (ExecBase+$3E): must be between $40000 and $80000
8. Validate KickMemPtr (ExecBase+$4E): if non-zero, must be in $C40000–$DC0000
   range and 256K-aligned

If all checks pass, the ROM takes the warm start path at $FC0238 (33.180). If
any check fails, it falls through to the cold start at $FC01CE.

## Emulator Implications

### Slow RAM requirement

KS 1.2 on A500 and A2000 needs 512 KB of slow RAM at $C00000. Without it,
ExecBase stays in chip RAM, and the exec init process runs out of memory for
library and device allocations. The emulator boot tests configure this:

```rust
// boot_ocs.rs
AmigaConfig {
    model: AmigaModel::A500,
    slow_ram_size: 512 * 1024,  // Required for KS 1.2
    ...
}
```

The A1000 test also uses 512 KB slow RAM because any A1000 running KS 1.2 would
have had the front-panel memory expansion installed (the machine shipped with
256 KB chip RAM and the expansion brought it to a usable configuration).

### Overlay timing

The overlay clear at $FC00FE must take effect immediately. The next instruction
after the CIA-A write ($FC010E LEA $DFF000,A4) reads from the custom chip space,
but memory writes between $FC0208 and $FC0240 go to chip RAM at $000000. If the
overlay is still active, these writes hit ROM (dropped) and reads come from ROM
(wrong values).

### Diagnostic ROM at $F00000

The probe at $FC00E6 reads $F00000 and checks for $1111. In standard emulator
configurations, $F00000 is unmapped (returns $0000 or $FFFF depending on
implementation). No action needed unless emulating A3000 diagnostic or A1000 WCS
hardware.

### Custom chip register timing

The INTENA/INTREQ/DMACON writes at $FC0114–$FC0120 must disable DMA and
interrupts within one bus cycle. Late effect would allow stale DMA or interrupt
activity to interfere with memory detection.

### Boot test assertions

The emulator boot tests (in `crates/machine-amiga/tests/boot_ocs.rs`) verify:

| Test | Model | ROM | DMACON bits | BPLCON0 |
|------|-------|-----|-------------|---------|
| `test_boot_kick12_a1000` | A1000 | 33.166 | $0180 (BPLEN+COPEN) | $2302 |
| `test_boot_kick12_a500` | A500 | 33.180 | $0180 | $2302 |
| `test_boot_kick12_a2000` | A2000 | 33.180 | $0180 | $2302 |

All three tests pass in the current emulator with 512 KB slow RAM configured.

### Alert handler behaviour

The early alert handler at $FC05B4 issues a RESET instruction followed by a jump
through ExecBase. The emulator must handle the RESET instruction correctly (it
resets external devices but does not restart the CPU). If the emulator halts on
RESET instead of continuing execution, the alert handler's soft-reboot loop
breaks.

### No machine-type check

Unlike KS 1.3 (which has a machine-type byte at ROM offset $0198 distinguishing
A500/A2000 from A3000), KS 1.2 does not check machine type during boot. Both ROM
variants run identically on any OCS hardware. The emulator's `AmigaModel`
configuration affects memory layout and peripheral presence, not ROM behaviour.
