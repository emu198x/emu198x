# Kickstart 1.0 — A1000

## ROM Identification

| Property | Value |
|----------|-------|
| File | `kick10.rom` |
| Size | 256K (262,144 bytes) |
| Version | exec 1.2 (28 Aug 1985) |
| Target | A1000 |
| SSP | $11114EF9 |
| PC | $FC00CE |
| Mapped at | $FC0000–$FFFFFF |

This is the earliest production Kickstart ROM. It shipped with the Amiga 1000 in
1985. The entry point sits at $FC00CE — four bytes earlier than KS 1.2+ ($FC00D2)
because KS 1.0 lacks the `RESET` instruction and `NOP` that later ROMs place
before the entry point.

The ROM header at $000008 contains the version string:

```
AMIGA ROM Operating System and Libraries
Copyright (C) 1985, Commodore-Amiga, Inc.
All Rights Reserved.
```

### Dual Address-Space RomTags

KS 1.0 contains two copies of certain RomTags, one for $FC0000 mapping (standard
256K position) and one for $F00000 mapping. The A1000 loaded Kickstart into
Writable Control Store (WCS) RAM, and early production units mapped this at
$F00000. The RomTag scan matches based on the self-pointer (rt_MatchTag), so only
the tags for the active mapping address are found. For example, dos.library has
a RomTag at file offset $34BEA (self-pointer $FF4BEA, base $FC0000) and a
duplicate at offset $34BD0 (self-pointer $F34BD0, base $F00000).

The emulator maps the ROM at $FC0000, so the $FC0000-base tags are the ones that
match during the RomTag scan.

## Boot Flow

### Stage 1: Reset Vector Fetch

Standard — overlay maps ROM at $000000, CPU reads SSP=$11114EF9 and PC=$FC00CE.
See [boot-flow-overview.md](boot-flow-overview.md) Stage 1.

### Stage 2: Entry Point ($FC00CE)

```asm
$FC00CE  LEA     $20000,SP              ; Temporary stack at 128K
$FC00D4  MOVE.L  #$20000,D0             ; 131072 iterations
$FC00DA  SUBQ.L  #1,D0
$FC00DC  BGT.S   $FC00DA                ; Delay loop — hardware settle time
```

The delay loop is identical to KS 1.2+ (same iteration count, same encoding).

### Stage 2a: Diagnostic ROM Check

```asm
$FC00DE  LEA     (PC),$FC0000,A0        ; Get ROM base via PC-relative
$FC00E2  CMPA.L  #$F00000,A0            ; Are we running from WCS at $F00000?
$FC00E8  BEQ.S   $FC0100                ; If so, skip diag check (WCS IS $F00000)
$FC00EA  CMPI.W  #$1111,$F00000         ; Check for diagnostic ROM magic
$FC00F2  BNE.S   $FC0100                ; No diag ROM — continue boot
$FC00F4  LEA     $FC0100,A5             ; Return address after diag
$FC00FA  JMP     $F00002                ; Jump into diagnostic ROM
```

**Difference from KS 1.2+:** KS 1.0 checks whether the ROM is *itself* running
from $F00000 (WCS mapping) and skips the diagnostic ROM probe in that case. KS
1.2+ always checks $F00000 for the magic word. This extra test exists because the
A1000 WCS occupies the same address range as the diagnostic ROM slot.

### Stage 3: Hardware Reset

```asm
$FC0100  MOVE.B  #$03,$BFE201           ; CIA-A DDRA: bits 0-1 output (OVL + LED)
$FC0108  MOVE.B  #$02,$BFE001           ; CIA-A PRA: OVL=0 (clear overlay), LED off
$FC0110  LEA     $DFF000,A4             ; Custom chip base
$FC0116  MOVE.W  #$7FFF,D6              ; $7FFF constant for register clears
$FC011A  MOVE.W  D6,$DFF09A             ; INTENA = $7FFF (disable all interrupts)
$FC011E  MOVE.W  D6,$DFF09C             ; INTREQ = $7FFF (clear all pending IRQs)
$FC0122  MOVE.W  D6,$DFF096             ; DMACON = $7FFF (disable all DMA)
$FC0126  MOVE.W  #$0200,$DFF100         ; BPLCON0 = $0200 (blank display, colour)
$FC012C  MOVE.W  #$0000,$DFF110         ; BPLCON1 = $0000 (no scroll offset)
$FC0132  MOVE.W  #$0444,$DFF180         ; COLOR00 = $0444 (dark grey background)
```

**Identical to KS 1.2+** — same registers, same values, same order. The dark grey
background is the first visible sign of boot activity.

### Stage 3a: ROM Checksum

```asm
$FC0138  MOVEA.L #$FC0000,A0            ; ROM base
$FC013E  MOVE.L  #$10000,D1             ; 65536 words (= 128K bytes = half of 256K)
$FC0144  LEA     (PC),A5                ; Return address = $FC014C
$FC0148  BRA.W   $FC2004                ; Jump to checksum subroutine
```

The checksum subroutine at $FC2004 performs a ones'-complement word-sum over the
ROM. It sums `D1` words from address `A0` and returns the ones'-complement result
in `D0`. If D0=0, the checksum is valid.

**Difference from KS 1.2+:** KS 1.0 checksums only $10000 words (128K bytes),
not the full 256K ROM. Later ROMs (KS 2.0+) checksum the entire ROM image.
KS 1.2/1.3 skip the checksum entirely.

### Stage 3b: Exception Vector Fill

```asm
$FC014C  MOVEA.W #$0008,A0              ; Start at vector 2 (bus error)
$FC0150  MOVE.W  #$002D,D1              ; 46 vectors ($008–$0BC)
$FC0154  LEA     $FC058E,A1             ; Alert/crash handler address
$FC0158  MOVE.L  A1,(A0)+               ; Fill vector
$FC015A  DBF     D1,$FC0158             ; Loop
$FC015E  BRA.W   $FC2A4E                ; Continue to warm/cold detection
```

Fills exception vectors $008–$0BC (bus error through to trap #15) with a pointer
to the alert handler at $FC058E. This is identical to the KS 1.2+ approach.

### Stage 3c: Warm/Cold Start Detection

```asm
$FC2A4E  MOVE.L  #$FFFFFFFF,D6          ; Default: cold start
$FC2A54  CMPI.L  #"HELP",$000000        ; Check for warm-start magic at address 0
$FC2A5C  BNE.W   $FC0162                ; No magic → proceed (cold or warm via ExecBase)
$FC2A60  CLR.L   $000000                ; Clear magic (one-shot)
$FC2A64  MOVEM.L $000100,D6-D7          ; Restore warm-start data from $100
$FC2A6A  BRA.W   $FC0162                ; Continue
```

**Difference from KS 1.2+:** KS 1.0 uses the ASCII string "HELP" ($4845_4C50)
at address $000000 as the warm-start magic word. KS 1.2+ checks ExecBase validity
at $000004 instead. If "HELP" is found, D6/D7 are restored from $000100 (saved
warm-start state).

Both paths then reach $FC0162, which performs the standard ExecBase validation
(odd pointer check, complement checksum, word-sum verification, ColdCapture
dispatch). This is the same logic as KS 1.2+ Stage 2.

### Stage 3d: Cold Start — Memory Detection

If ExecBase validation fails (cold start):

```asm
$FC01BA  LEA     $0400,FP               ; ExecBase tentative location
$FC01BE  SUBA.L  #$FFFFFDF0,FP          ; FP = $400 + $210 = $610
$FC01C4  LEA     $000000,A0             ; Chip RAM start
$FC01C8  LEA     $080000,A1             ; Chip RAM end = 512K max
$FC01CE  LEA     (PC),A5                ; Return = $FC01D6
$FC01D2  BRA.W   $FC056C                ; Chip RAM sizing subroutine
```

The sizing subroutine at $FC056C writes a test pattern ($F2D4B698) at 4K
intervals from A0 to A1. It checks for aliasing (writes to higher addresses
reflecting at address 0) to determine the actual RAM size. Returns the top of
chip RAM in D0.

```asm
$FC01D6  CMPI.L  #$20000,D0             ; At least 128K?
$FC01DC  BCS.S   $FC01EE                ; No → fatal error (green screen)
$FC01DE  SUBA.L  A2,A2                  ; RAM base = $000000
$FC01E0  MOVEA.L D0,A3                  ; RAM top = sizing result
$FC01E2  LEA     (PC),A5                ; Return = $FC01EA
$FC01E6  BRA.W   $FC2042                ; Full memory test subroutine
$FC01EA  TST.L   D0                     ; Memory test passed?
$FC01EC  BEQ.S   $FC01F6                ; Yes → continue
$FC01EE  MOVE.W  #$00C0,D0              ; Error colour ($00C0 = bright green)
$FC01F2  BRA.W   $FC0592                ; → Fatal error display
```

**Key differences from KS 1.2+:**

1. **512K chip RAM limit.** KS 1.0 only probes up to $080000 (512K). KS 1.2+
   probes up to $200000 (2 MB). The A1000 shipped with 256K chip RAM, expandable
   to 512K via the front-panel expansion chassis.

2. **No slow RAM detection.** The A1000 has no $C00000 slow RAM slot. KS 1.0 does
   not probe $C00000–$DC0000 at all.

3. **Full memory test.** The subroutine at $FC2042 runs a comprehensive
   multi-pattern memory test: $AAAAAAAA, $55555555, rotating bit patterns,
   address-as-data, and inverted address-as-data. This is more thorough than
   KS 1.2+ which uses a simpler write/readback.

4. **Minimum 128K required.** KS 1.0 requires at least 128K of chip RAM. If the
   sizing result is less than $20000, the ROM displays COLOR00=$00C0 (bright green)
   and enters the error blink loop.

### Stage 4: ExecBase Init

After memory detection, ExecBase is built at the tentative location. If warm start
restored valid data from $000100, the warm-start path is taken at $FC01A6 instead.

```asm
$FC01F6  MOVE.W  #$7FFF,$DFF096         ; DMACON = $7FFF (re-disable all DMA)
$FC01FC  MOVE.W  #$0200,$DFF100         ; BPLCON0 = $0200 (blank display)
$FC0202  MOVE.W  #$0000,$DFF110         ; BPLCON1 = $0000
$FC0208  MOVE.W  #$0888,$DFF180         ; COLOR00 = $0888 (medium grey)
```

**Note:** COLOR00 changes from $0444 (dark grey) to $0888 (medium grey) at this
point. On a real machine, this provides a visual indication that memory detection
passed and ExecBase init is starting.

ExecBase init then proceeds at $FC020E:
1. Clears ExecBase fields ($0054–$020D) — 112 longwords zeroed
2. Stores ExecBase pointer at $000004
3. Writes complement checksum at ExecBase+$26
4. Sets SSP to top of chip RAM (A3)
5. Initialises MemList, ResourceList, DeviceList, LibList, and other internal lists
6. Calls internal exec init at $FC2A6E (saves D6/D7 warm-start state)

### Stages 5–9: Resident Module Init

Standard RomTag scan and priority-ordered init. See the [Resident Modules](#resident-modules)
table for the full list. Because KS 1.0 lacks expansion.library, there is no
Zorro autoconfig probe.

### Stage 10: STRAP Display

The strap module (pri $C4 = −60) uses Intuition to open a screen, unlike
the raw copper-list approach of some later ROMs.

The strap init at $FC4872:
1. Opens trackdisk.device (CMD_READ, CMD_CHANGESTATE)
2. Allocates 12K ($3000 bytes) for a 320x200 2-plane bitmap
3. Opens a 320x200 2-plane Intuition screen via OpenScreen
4. Draws the floppy disk insert icon using vector data at $FC52EC
5. Writes DMACON=$8100 (SET + BPLEN) to enable bitplane DMA
6. Enters the disk-wait loop (CMD_CHANGESTATE polling)

The insert-disk icon is rendered using a vector-drawing routine that takes
coordinate pairs and line data from ROM, then fills the bitmap using OR operations.
The palette comes from the screen setup data at $FC4E38:

| Register | Value | Colour |
|----------|-------|--------|
| COLOR00 | $0FFF | White |
| COLOR01 | $0000 | Black |
| COLOR02 | $077C | Blue |
| COLOR03 | $0BBB | Light grey |

**Difference from KS 1.2+:** The KS 1.0 display uses Intuition's OpenScreen
rather than building a copper list directly. The palette and icon design differ
from the KS 1.2+ hand-drawn display.

If the screen open or bitmap allocation fails, strap calls the Alert handler
with code $30048014 (AG_OpenScreen | AO_BootStrap) or $30010000 (AG_NoMemory).

### Stage 11: Disk Boot Wait

Same as KS 1.2+ — trackdisk.device monitors for disk insertion, reads boot
block, validates checksum, and jumps to boot code at bootblock+$0C.

## Resident Modules

21 RomTags total. 17 have RTF_COLDSTART set and are initialised during boot in
priority order. 4 are non-coldstart (opened on demand by other code).

### Cold-Start Modules (sorted by init priority)

| Pri | Name | Type | Flags | ID String | Init |
|-----|------|------|-------|-----------|------|
| +100 | clist.library | LIBRARY | $01 | clist 1.3 (5 Sep 1985) | $FC63F8 |
| +100 | potgo.resource | RESOURCE | $01 | potgo 1.2 (5 Sep 1985) | $FE60A4 |
| +80 | cia.resource | RESOURCE | $01 | cia 1.2 (5 Sep 1985) | $FC5AE8 |
| +70 | disk.resource | RESOURCE | $01 | disk 1.7 (5 Sep 1985) | $FC6530 |
| +70 | misc.resource | RESOURCE | $01 | misc 1.8 (5 Sep 1985) | $FE5F98 |
| +60 | keyboard.device | DEVICE | $01 | keyboard rawinput 1.14 (5 Sep 1985) | $FE6F6C |
| +60 | gameport.device | DEVICE | $01 | gameport rawinput 1.14 (5 Sep 1985) | $FE73D8 |
| +60 | timer.device | DEVICE | $01 | timer 1.7 (5 Sep 1985) | $FEA19C |
| +40 | audio.device | DEVICE | $81 | audio 1.2 (5 Sep 1985) | $FC378A |
| +40 | graphics.library | LIBRARY | $01 | graphics 1.4 Sep 04 1985 | $FC8976 |
| +40 | input.device | DEVICE | $01 | input rawinput 1.14 (5 Sep 1985) | $FE7B20 |
| +30 | layers.library | LIBRARY | $01 | layers 1.5 Sep 05 1985 | $FE1FFC |
| +20 | console.device | DEVICE | $01 | console rawinput 1.14 (5 Sep 1985) | $FE8490 |
| +20 | trackdisk.device | DEVICE | $01 | trackdisk 1.48 (5 Sep 1985) | $FEA9E4 |
| +10 | intuition.library | — | $01 | Intuition 1.13 -- 9 Sep 85 | $FD8114 |
| +5 | alert.hook | — | $01 | alert.hook | $FC2A76 |
| 0 | mathffp.library | LIBRARY | $81 | mathffp 1.2 (10 Sep 1985) | $FE5B0C |
| −60 | strap | — | $01 | bootstrap 1.10 (5 Sep 1985) | $FC4872 |

Flags: $01 = RTF_COLDSTART, $81 = RTF_AUTOINIT + RTF_COLDSTART.

### Non-Cold-Start Modules

These are in the resident list but NOT initialised during boot. They are opened
on demand by other modules (e.g. dos.library is opened during the startup
sequence).

| Pri | Name | Type | Flags | ID String |
|-----|------|------|-------|-----------|
| +120 | exec.library | LIBRARY | $00 | exec 1.2 (28 Aug 1985) |
| +70 | ramlib.library | RESOURCE | $00 | ramlib 1.12 (5 Sep 1985) |
| 0 | workbench.task | TASK | $00 | wb 1.239 (10 Sep 1985) |
| 0 | dos.library | LIBRARY | $00 | dos 1.12 (9 Sep 1985) |

exec.library's init address ($FC00CE) is the ROM entry point itself — exec
bootstraps before the RomTag scan runs.

### Notable differences from KS 1.3

- **No expansion.library** — the A1000 has no Zorro autoconfig. There is no
  expansion board probe during boot.
- **No keymap.library** — keymap handling is built into keyboard.device.
- **No bootmenu, syscheck, romboot** — these are KS 2.0+ additions.
- **Priority order is very different.** KS 1.0 uses much higher priorities
  across the board compared to KS 1.3:
  - cia.resource: +80 (KS 1.3: +20)
  - keyboard.device: +60 (KS 1.3: +5)
  - timer.device: +60 (KS 1.3: +40)
  - input.device: +40 (KS 1.3: +4)
  - potgo.resource: +100 (KS 1.3: +10)
  - clist.library: +100 (not present in KS 1.3 as a separate module)
  - intuition.library: +10 (KS 1.3: +50)
  - layers.library: +30 (KS 1.3: +70)
- **Intuition at +10** means it initialises after most devices, just above
  alert.hook (+5). The strap module at −60 depends on Intuition for its screen.

## Hardware Probing

Every custom register and CIA access during cold boot, in order:

| Stage | Address | R/W | Register | Value | Purpose |
|-------|---------|-----|----------|-------|---------|
| 3 | $BFE201 | W | CIA-A DDRA | $03 | Set OVL + LED as outputs |
| 3 | $BFE001 | W | CIA-A PRA | $02 | Clear overlay, LED off |
| 3 | $DFF09A | W | INTENA | $7FFF | Disable all interrupts |
| 3 | $DFF09C | W | INTREQ | $7FFF | Clear all interrupt requests |
| 3 | $DFF096 | W | DMACON | $7FFF | Disable all DMA |
| 3 | $DFF100 | W | BPLCON0 | $0200 | Blank display, colour mode |
| 3 | $DFF110 | W | BPLCON1 | $0000 | No scroll offset |
| 3 | $DFF180 | W | COLOR00 | $0444 | Dark grey background |
| 4 | $DFF096 | W | DMACON | $7FFF | Re-disable all DMA |
| 4 | $DFF100 | W | BPLCON0 | $0200 | Blank display |
| 4 | $DFF110 | W | BPLCON1 | $0000 | No scroll |
| 4 | $DFF180 | W | COLOR00 | $0888 | Medium grey (memory OK) |
| 10 | $DFF096 | W | DMACON | $8100 | SET + BPLEN (enable bitplanes) |
| var | $DFF07C | R | DENISEID | — | Chipset detection (graphics.library) |
| var | $DFF004 | R | VPOSR | — | PAL/NTSC detection |

**Not accessed (vs KS 1.2+):**
- $F00000 is probed for diagnostic ROM magic only if the ROM is not itself mapped
  at $F00000
- $C00000–$DC0000 — no slow RAM probe
- $E80000 — no Zorro autoconfig (no expansion.library)
- $DE0000 — no RAMSEY (no A3000 support)

## Error Paths

### Memory Test Failure

If chip RAM sizing returns less than 128K, or the full memory test fails:

```asm
$FC0592  LEA     $DFF000,A4
$FC0598  MOVE.W  #$0200,$DFF100         ; BPLCON0 = blank
$FC059E  MOVE.W  #$0000,$DFF110         ; BPLCON1 = 0
$FC05A4  MOVE.W  D0,$DFF180             ; COLOR00 = error colour (D0)
$FC05A8  MOVEQ   #10,D1                 ; 11 blink cycles
; LED blink loop: BSET/BCLR bit 1 of $BFE001 (power LED toggle)
; After blink, delay loop, then RESET + JMP through $000004
```

The error colour for memory failure is $00C0 (bright green). The power LED blinks
11 times, then the ROM executes a RESET instruction and attempts a restart via
the ExecBase pointer at $000004.

### Exception Handler

All unhandled exceptions (bus error, address error, illegal instruction, etc.)
jump to the alert handler at $FC058E, which loads COLOR00=$0CC0 (yellow) and
enters the same LED-blink-then-reset loop.

### STRAP Errors

| Alert Code | Meaning |
|------------|---------|
| $30048014 | AG_OpenScreen + AO_BootStrap — cannot open Intuition screen |
| $30010000 | AG_NoMemory — bitmap allocation failed |
| $30068014 | trackdisk.device open error |

## Emulator Implications

### What must work

1. **Overlay latch** — CIA-A PRA bit 0 must control ROM-over-chip-RAM mapping.
   Without it, chip RAM never appears at $000000 and memory detection fails.

2. **512K chip RAM** — the A1000 expansion chassis provides 256K or 512K. The
   emulator should provide at least 256K. Memory sizing probes up to $080000 only.

3. **No slow RAM needed** — unlike KS 1.2+ on the A500/A2000, KS 1.0 does not
   require slow RAM at $C00000. The A1000 has no trapdoor expansion.

4. **CIA timers** — cia.resource (pri +80) initialises early and depends on CIA
   timer A/B functioning correctly.

5. **Full memory test tolerance** — the multi-pattern memory test at $FC2042 takes
   many thousands of cycles. The emulator must not time out during this phase.

6. **Intuition-based STRAP** — unlike KS 1.2+ which draws directly via copper
   lists, KS 1.0 strap uses Intuition to open a screen. This means graphics.library,
   layers.library, and intuition.library must all initialise correctly before the
   insert-disk screen appears.

7. **No expansion.library** — the emulator must not depend on expansion.library
   for Zorro autoconfig. Reads from $E80000 should return $FF but
   expansion.library never probes this address.

### What can be absent

- **Slow RAM** — not probed, not needed
- **RAMSEY / Fat Gary** — A3000 only, not accessed
- **Zorro autoconfig** — no expansion.library
- **68010+ features** — KS 1.0 assumes a 68000. It does not probe MOVEC/VBR.
  CPU detection within exec may be minimal or absent.

### Boot test expectations

| Condition | Expected |
|-----------|----------|
| COLOR00 after Stage 3 | $0444 (dark grey) |
| COLOR00 after Stage 4 | $0888 (medium grey) |
| DMACON after STRAP | $8100 set bits (BPLEN) |
| BPLCON0 after STRAP | Depends on Intuition screen setup |
| Minimum chip RAM | 128K ($20000) |
| Slow RAM | Not required |
