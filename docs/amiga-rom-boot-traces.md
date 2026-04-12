# Amiga Kickstart Boot Sequence: Annotated Disassembly Traces

Comparative analysis of Kickstart V37.175 (2.04) and V40.063 (3.1) boot sequences,
produced from raw ROM disassembly with cross-reference to NDK 3.9 headers and
vAmiga emulator source.

---

## 1. Setup: How to Reproduce

### Disassembler

Python 3 + [Capstone](https://www.capstone-engine.org/) 5.0.7, M68K big-endian mode.

```
pip3 install capstone
```

### Disassembly Script

A custom Python script (`amiga_disasm.py`) wraps Capstone with:
- Custom chip register annotation ($DFF000-$DFF1FE)
- CIA-A/CIA-B register annotation ($BFE001/$BFD000)
- ExecBase field lookup (offsets verified against vAmiga OSDebuggerTypes.h and NDK 3.9 execbase.i)
- ROMTag ($4AFC) scanner with Resident structure decoder

Usage:
```
python3 amiga_disasm.py <rom_file> reset <count>     # From reset vector
python3 amiga_disasm.py <rom_file> addr <hex> <count> # From ROM address
python3 amiga_disasm.py <rom_file> romtags            # Scan all ROMTags
```

### ROM Load Address

Both ROMs load at $F80000 (512 KB ROMs). The initial SSP and PC are at file offset 0.
To convert a ROM address to a file offset: `file_offset = address - $F80000`.

---

## 2. ROM Verification

### Kickstart 2.04 (V37.175) -- A500+

| Property       | Value |
|----------------|-------|
| File           | `kick204_37_175_a500plus.rom` |
| Size           | 524,288 bytes (512 KB) |
| SHA-256        | `d0b70e8a1772614b897f92c33cb299bed3fc8e3de488fc12f67f97fc2486eb79` |
| First 8 bytes  | `11 14 4E F9 00 F8 00 D2` |
| Initial SSP    | $11144EF9 (magic cookie, not a real stack pointer) |
| Initial PC     | $00F800D2 (file offset $D2) |
| Version word   | $0025.$00AF at offset +$0C = 37.175 |
| Exec ID        | `exec 37.132 (23.5.91)` |
| Copyright      | `Copyright 1985-1991 Commodore-Amiga, Inc.` |

### Kickstart 3.1 (V40.063) -- A500/A600/A2000

| Property       | Value |
|----------------|-------|
| File           | `kick31_40_063_a500_a600_a2000.rom` |
| Size           | 524,288 bytes (512 KB) |
| SHA-256        | `8c8a0cf04f91b88eaf0c4f1126041987067e2286a8ee590bdbae447a8000c5ee` |
| First 8 bytes  | `11 14 4E F9 00 F8 00 D2` |
| Initial SSP    | $11144EF9 (same magic cookie) |
| Initial PC     | $00F800D2 (same entry point offset) |
| Version word   | $0028.$003F at offset +$0C = 40.63 |
| Exec ID        | `exec 40.10 (15.7.93)` |
| Copyright      | `Copyright (c) 1985-1993 Commodore-Amiga, Inc.` |

**Note on the SSP cookie:** The value $11144EF9 is not used as an actual stack pointer.
The first instruction (`lea.l $400.w, a7`) immediately sets A7 to $400.
The $1114 prefix serves as a ROM identification signature: the first two bytes
distinguish Kickstart ROMs from other 68000 binary images. The $4EF9 portion
is the opcode for `JMP abs.l`, so reading offset +2 as code yields
`JMP $00F800D2` -- a secondary entry path if the ROM is mapped at address 0
during overlay.

---

## 3. ROMTag Scan

Every Kickstart module registers itself via a Resident structure (ROMTag), identified
by the word $4AFC (the 68000 ILLEGAL instruction opcode, `RTC_MATCHWORD`).

**Resident structure** (from `exec/resident.h`, 26 bytes):

| Offset | Size | Field          | Description |
|--------|------|----------------|-------------|
| +$00   | 2    | rt_MatchWord   | $4AFC (ILLEGAL opcode) |
| +$02   | 4    | rt_MatchTag    | Pointer back to rt_MatchWord (self-reference) |
| +$06   | 4    | rt_EndSkip     | Address to continue scan after this tag |
| +$0A   | 1    | rt_Flags       | RTF_AUTOINIT=$80, RTF_AFTERDOS=$04, RTF_SINGLETASK=$02, RTF_COLDSTART=$01 |
| +$0B   | 1    | rt_Version     | Module version number |
| +$0C   | 1    | rt_Type        | Node type (NT_LIBRARY=9, NT_DEVICE=3, etc.) |
| +$0D   | 1    | rt_Pri         | Init priority (higher = earlier) |
| +$0E   | 4    | rt_Name        | Pointer to module name string |
| +$12   | 4    | rt_IdString    | Pointer to version/ID string |
| +$16   | 4    | rt_Init        | Init function or data structure pointer |

### V37.175 ROMTag Table (39 modules, sorted by priority)

```
Pri   Ver  Flags                          Type          Name
+110   37  RTF_SINGLETASK                 NT_LIBRARY    expansion.library
+105   37  RTF_SINGLETASK                 NT_LIBRARY    exec.library
+105   37  RTF_COLDSTART                  NT_UNKNOWN    diag init
+103   37  RTF_AUTOINIT|RTF_COLDSTART     NT_LIBRARY    utility.library
+100   37  RTF_AUTOINIT|RTF_COLDSTART     NT_RESOURCE   potgo.resource
 +80   37  RTF_COLDSTART                  NT_RESOURCE   cia.resource
 +80   37  RTF_COLDSTART                  NT_RESOURCE   FileSystem.resource
 +70   37  RTF_COLDSTART                  NT_RESOURCE   disk.resource
 +70   37  RTF_COLDSTART                  NT_RESOURCE   misc.resource
 +65   37  RTF_COLDSTART                  NT_LIBRARY    graphics.library
 +60   37  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     gameport.device
 +50   37  RTF_COLDSTART                  NT_DEVICE     timer.device
 +45   37  RTF_COLDSTART                  NT_RESOURCE   battclock.resource
 +45   37  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     keyboard.device
 +44   37  RTF_COLDSTART                  NT_RESOURCE   battmem.resource
 +40   37  RTF_AUTOINIT|RTF_COLDSTART     NT_LIBRARY    keymap.library
 +40   37  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     input.device
 +31   37  RTF_COLDSTART                  NT_LIBRARY    layers.library
 +25   37  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     ramdrive.device
 +20   37  RTF_COLDSTART                  NT_DEVICE     trackdisk.device
 +10   37  RTF_AUTOINIT|RTF_COLDSTART     NT_LIBRARY    intuition.library
  +5   37  RTF_COLDSTART                  NT_UNKNOWN    alert.hook
  +5   37  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     console.device
   0   37  (none)                         NT_LIBRARY    mathieeesingbas.library
 -35   37  RTF_COLDSTART                  NT_UNKNOWN    syscheck
 -40   37  RTF_COLDSTART                  NT_UNKNOWN    romboot
 -50   37  RTF_COLDSTART                  NT_UNKNOWN    bootmenu
 -60   37  RTF_COLDSTART                  NT_UNKNOWN    strap
 -81   37  (none)                         NT_UNKNOWN    filesystem
-100   37  RTF_AFTERDOS                   NT_UNKNOWN    ramlib
-120   37  RTF_AUTOINIT                   NT_DEVICE     audio.device
-120   37  (none)                         NT_LIBRARY    dos.library
-120   37  (none)                         NT_TASK       workbench.task
-120   37  RTF_AUTOINIT                   NT_LIBRARY    gadtools.library
-120   37  RTF_AUTOINIT                   NT_LIBRARY    icon.library
-120   37  RTF_AUTOINIT                   NT_LIBRARY    mathffp.library
-120   37  RTF_AUTOINIT                   NT_LIBRARY    workbench.library
-121   37  (none)                         NT_UNKNOWN    con-handler
-122   37  (none)                         NT_UNKNOWN    shell
-123   37  (none)                         NT_UNKNOWN    ram-handler
```

### V40.063 ROMTag Table (44 modules, sorted by priority)

```
Pri   Ver  Flags                          Type          Name
+110   40  RTF_SINGLETASK                 NT_LIBRARY    expansion.library
+105   40  RTF_SINGLETASK                 NT_LIBRARY    exec.library
+105   40  RTF_COLDSTART                  NT_UNKNOWN    diag init
+103   40  RTF_AUTOINIT|RTF_COLDSTART     NT_LIBRARY    utility.library
+100   37  RTF_AUTOINIT|RTF_COLDSTART     NT_RESOURCE   potgo.resource        *unchanged*
 +80   39  RTF_COLDSTART                  NT_RESOURCE   cia.resource
 +80   40  RTF_COLDSTART                  NT_RESOURCE   FileSystem.resource
 +70   39  RTF_COLDSTART                  NT_RESOURCE   battclock.resource    *moved up from +45*
 +70   37  RTF_COLDSTART                  NT_RESOURCE   misc.resource         *unchanged*
 +70   37  RTF_COLDSTART                  NT_RESOURCE   disk.resource         *unchanged*
 +69   39  RTF_COLDSTART                  NT_RESOURCE   battmem.resource      *moved up from +44*
 +65   40  RTF_COLDSTART                  NT_LIBRARY    graphics.library
 +64   40  RTF_AUTOINIT|RTF_COLDSTART     NT_LIBRARY    layers.library        *moved up from +31*
 +60   40  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     gameport.device
 +50   39  RTF_COLDSTART                  NT_DEVICE     timer.device
 +48   40  RTF_COLDSTART                  NT_RESOURCE   card.resource         *NEW: PCMCIA*
 +45   40  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     keyboard.device
 +40   40  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     input.device
 +40   40  RTF_AUTOINIT|RTF_COLDSTART     NT_LIBRARY    keymap.library
 +25   39  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     ramdrive.device
 +20   40  RTF_COLDSTART                  NT_DEVICE     trackdisk.device
 +15   40  RTF_COLDSTART                  NT_DEVICE     carddisk.device       *NEW: PCMCIA*
 +10   40  RTF_COLDSTART                  NT_DEVICE     scsi.device           *NEW: A600 IDE*
 +10   40  RTF_AUTOINIT|RTF_COLDSTART     NT_LIBRARY    intuition.library
  +5   40  RTF_AUTOINIT|RTF_COLDSTART     NT_DEVICE     console.device
   0   40  RTF_COLDSTART                  NT_LIBRARY    mathieeesingbas.library
 -35   40  RTF_COLDSTART                  NT_UNKNOWN    syscheck
 -40   40  RTF_COLDSTART                  NT_UNKNOWN    romboot
 -50   40  RTF_COLDSTART                  NT_UNKNOWN    bootmenu
 -55   40  RTF_COLDSTART                  NT_UNKNOWN    alert.hook            *moved down from +5*
 -60   40  RTF_COLDSTART                  NT_UNKNOWN    strap
 -81   40  (none)                         NT_UNKNOWN    filesystem
-100   40  RTF_AFTERDOS                   NT_UNKNOWN    ramlib
-120   37  RTF_AUTOINIT                   NT_DEVICE     audio.device          *unchanged from V37*
-120   40  (none)                         NT_LIBRARY    dos.library
-120   39  (none)                         NT_TASK       workbench.task
-120   40  RTF_AUTOINIT                   NT_LIBRARY    mathffp.library
-120   40  RTF_AUTOINIT                   NT_LIBRARY    icon.library
-120   40  RTF_AUTOINIT                   NT_LIBRARY    gadtools.library
-120   40  RTF_AUTOINIT                   NT_LIBRARY    workbench.library
-121   40  (none)                         NT_UNKNOWN    con-handler
-122   40  (none)                         NT_UNKNOWN    shell
-123   39  (none)                         NT_UNKNOWN    ram-handler
```

**Key differences V37 -> V40:**
- 5 new modules: `card.resource`, `carddisk.device`, `scsi.device` (A600/A1200 hardware), plus layers and battclock priority changes
- `audio.device` still at V37 -- unchanged between Kickstart versions
- `alert.hook` moved from priority +5 to -55 (now inits after most hardware)
- `layers.library` promoted from +31 to +64 (before graphics in V37, now just below)
- `battclock.resource` and `battmem.resource` moved to higher priorities
- Several modules updated from V37 to V39/V40 even though Kickstart is V40

---

## 4. V37 Reset Trace (Annotated)

Reset vector: PC = $F800D2, loads at file offset $D2.

### Phase 1: ROM Checksum ($F800D2-$F800F0)

```
; --- ROM checksum verification ---
; Sums all 512KB in 32-bit longwords. Result must be $FFFFFFFF (one's complement zero).

$F800D2  lea.l    $400.w, a7                    ; Set supervisor stack to $400
$F800D6  lea.l    $f80000(pc), a0               ; a0 = ROM base address
$F800DC  moveq    #$ff, d1                      ; Inner loop: 256 longwords
$F800DE  moveq    #$1, d2                       ; Outer loop: 2 passes (256*2 = 512 longs = 2KB;
                                                 ;   but dbra decrements WORD counter, so this
                                                 ;   actually iterates 256*512 = 131072 longs = 512KB)
$F800E0  moveq    #$0, d5                       ; d5 = running checksum
$F800E2  add.l    (a0)+, d5                     ; Add next longword
$F800E4  bcc.b    $f800e8                        ; If no carry, skip
$F800E6  addq.l   #$1, d5                       ; Add carry (one's complement addition)
$F800E8  dbra     d1, $f800e2                    ; Inner loop (word counter wraps at $FFFF)
$F800EC  dbra     d2, $f800e2                    ; Outer loop
```

**How it works:** The `dbra` instruction decrements a word register and branches
if the result is not -1. With d1=$FF (255), the inner loop runs 256 times, then d1
wraps from $FFFF and continues. Combined with the outer counter d2=1 (runs 2 times),
this produces exactly 131,072 iterations (512 KB / 4 bytes per longword).
The one's complement sum of a valid ROM equals $FFFFFFFF.

### Phase 2: Diagnostic ROM Check ($F800F0-$F8010C)

```
$F800F0  lea.l    $f80000(pc), a0               ; a0 = this ROM's base
$F800F4  lea.l    $f00000.l, a1                 ; a1 = 256K ROM region
$F800FA  cmpa.l   a0, a1                        ; Are we at $F00000? (256K ROM mapping)
$F800FC  beq.b    $f8010c                        ; No diagnostic ROM possible if same
$F800FE  lea.l    $f8010c(pc), a5               ; a5 = return address for diag ROM
$F80102  cmpi.w   #$1111, (a1)                  ; Check for diag ROM signature at $F00000
$F80106  bne.b    $f8010c                        ; Not present, skip
$F80108  jmp      $2(a1)                         ; Jump to diagnostic ROM entry at $F00002
```

A diagnostic ROM at $F00000 can intercept boot before the OS initialises. If the
word at $F00000 is $1111, control transfers to $F00002. Register a5 holds the
return address so the diagnostic code can resume normal boot.

### Phase 3: OVL Clear and Hardware Init ($F8010C-$F80148)

```
$F8010C  clr.b    $bfe001.l                     ; CIAA PRA: clear OVL bit (bit 0)
                                                 ;   Maps Chip RAM to $000000 instead of ROM
$F80112  move.b   #$3, $bfe201.l                ; CIAA DDRA: set bits 0-1 as output
                                                 ;   (OVL and /LED are outputs)
$F8011A  lea.l    $dff000.l, a4                 ; a4 = custom chip base (kept throughout boot)

; --- Disable all interrupts, DMA, and pending requests ---
$F80120  move.w   #$7fff, d0                    ; Bit 15 = 0 means CLEAR; bits 0-14 = all
$F80124  move.w   d0, $9a(a4)                   ; INTENA: disable all interrupt sources
$F80128  move.w   d0, $9c(a4)                   ; INTREQ: acknowledge all pending interrupts
$F8012C  move.w   d0, $96(a4)                   ; DMACON: disable all DMA channels

; --- Minimal display setup ---
$F80130  move.w   #$174, $32(a4)                ; SERPER: set serial port period (for debugging?)
$F80136  move.w   #$200, $100(a4)               ; BPLCON0: blank display (1 plane, no bitplanes enabled)
$F8013C  move.w   #$0, $110(a4)                 ; BPL1DAT: clear bitplane data
$F80142  move.w   #$444, $180(a4)               ; COLOR00: set background to dark grey ($444)

; --- Prepare error colour ---
$F80148  move.w   #$f00, d0                     ; d0 = bright red (used if checksum fails)
```

### Phase 4: Checksum Verification and Trap Vector Init ($F8014C-$F80182)

```
$F8014C  not.l    d5                            ; Invert checksum; should become 0 if valid
$F8014E  bne.w    $f803b6                        ; FAIL: checksum bad -> red screen + LED flash

; --- Fill 68000 exception vectors with a default handler ---
$F80152  movea.w  #$8, a0                       ; a0 = vector 2 (bus error), skip reset vectors
$F80156  move.w   #$2d, d1                      ; 46 vectors to fill (vectors 2-47)
$F8015A  lea.l    $f8039e(pc), a1               ; a1 = default exception handler address
$F8015E  move.l   a1, (a0)+                     ; Store handler address
$F80160  dbra     d1, $f8015e                    ; Fill all 46 vectors

; --- Verify vectors were written correctly (RAM test) ---
$F80164  move.w   #$f0, d0                      ; d0 = yellow (for RAM failure)
$F80168  move.w   #$2d, d1                      ; Same count
$F8016C  cmpa.l   -(a0), a1                     ; Read back and compare
$F8016E  bne.w    $f803b6                        ; FAIL: RAM defective -> yellow screen
$F80172  dbra     d1, $f8016c                    ; Check all vectors

; --- Clear working registers ---
$F80176  moveq    #$0, d2                       ; d2-d7 = 0 (will hold preserved ExecBase fields)
$F80178  moveq    #$0, d3
$F8017A  moveq    #$0, d4
$F8017C  moveq    #$0, d5
$F8017E  moveq    #$0, d6
$F80180  moveq    #$0, d7
```

### Phase 5: ExecBase Validation ($F80182-$F801C6)

```
; --- Check if a valid ExecBase exists from a previous warm boot ---
$F80182  move.l   $4.w, d1                      ; d1 = AbsExecBase (location 4)
$F80186  movea.l  d1, a6                        ; a6 = candidate ExecBase
$F80188  btst.b   #$0, d1                       ; Is address odd? (must be even)
$F8018C  bne.b    $f801c4                        ; Odd = invalid, cold start

; --- Validate ChkBase (one's complement of ExecBase pointer) ---
$F8018E  add.l    $26(a6), d1                   ; d1 += ExecBase->ChkBase (+$26)
$F80192  not.l    d1                            ; If ChkBase == ~ExecBase, result is 0
$F80194  bne.b    $f801c6                        ; Mismatch = invalid ExecBase

; --- Validate ChkSum (checksum of fields $22-$52) ---
$F80196  lea.l    $22(a6), a0                   ; a0 = &ExecBase->SoftVer
$F8019A  moveq    #$18, d0                      ; 25 words to sum (SoftVer through ChkSum-2)
$F8019C  add.w    (a0)+, d1                     ; Sum words
$F8019E  dbra     d0, $f8019c
$F801A2  not.w    d1                            ; One's complement check
$F801A4  bne.b    $f801c6                        ; Bad checksum = invalid ExecBase

; --- ExecBase is valid. Check ColdCapture vector. ---
$F801A6  move.l   $2a(a6), d0                   ; d0 = ExecBase->ColdCapture
$F801AA  beq.b    $f801b8                        ; NULL = no capture, continue
$F801AC  movea.l  d0, a0                        ; a0 = ColdCapture code
$F801AE  lea.l    $f801b8(pc), a5               ; a5 = return address
$F801B2  clr.l    $2a(a6)                       ; Clear ColdCapture (one-shot)
$F801B6  jmp      (a0)                          ; Execute ColdCapture code

; --- Preserve KickMem/KickTag/KickCheckSum and ColdCapture fields ---
$F801B8  movem.l  $222(a6), d2-d4               ; d2 = KickMemPtr, d3 = KickTagPtr, d4 = KickCheckSum
$F801BE  movem.l  $2a(a6), d5-d7                ; d5 = ColdCapture (cleared), d6 = CoolCapture,
                                                 ;   d7 = WarmCapture
; --- Fall through to cold start with a6 = old ExecBase ---
$F801C4  suba.l   a6, a6                        ; Cold start: a6 = 0 (no old ExecBase)
```

### Phase 6: CPU Detection ($F801C8)

```
$F801C6  movea.l  a6, a5                        ; a5 = old ExecBase (or 0)
$F801C8  bsr.w    $f80b30                        ; Call CPU detection subroutine
                                                 ;   Returns d0 = AttnFlags bit mask
$F801CC  movea.l  d0, a2                        ; a2 = AttnFlags (saved for later)
```

The CPU detection routine at $F80B30:
1. Saves the Illegal Instruction ($10) and Line-F ($2C) exception vectors
2. Points them to a recovery handler
3. Attempts `MOVEC VBR,d0` -- if it doesn't trap, CPU is 68010+
4. Attempts 68020-specific instructions to detect 020/030/040
5. Tests for 68881/68882 FPU
6. Returns a bitmask in d0 (bit 0=68010, bit 1=68020, bit 2=68030, etc.)

### Phase 7: Chip RAM Probe ($F801CE-$F801FC)

```
; --- Probe chip RAM size by writing a magic value every 16KB ---
$F801CE  suba.l   a0, a0                        ; a0 = 0 (location 0)
$F801D0  movea.l  (a0), a1                      ; Save original contents of location 0
$F801D2  clr.l    (a0)                          ; Clear location 0
$F801D4  suba.l   a3, a3                        ; a3 = 0 (current probe address)
$F801D6  move.l   #$f2d4b689, d1                ; d1 = magic probe value

$F801E0  lea.l    $4000(a3), a3                 ; Advance by 16KB
$F801E4  cmpa.l   #$200000, a3                  ; Reached 2MB limit?
$F801EA  beq.b    $f801fa                        ; Yes, done
$F801EC  move.l   (a3), d0                      ; Save current contents
$F801EE  move.l   d1, (a3)                      ; Write magic
$F801F0  nop                                    ; Bus settle delay
$F801F2  cmp.l    (a0), d1                      ; Did magic appear at location 0?
                                                 ;   (happens when address wraps due to
                                                 ;   incomplete address decoding -- Agnus/Gary
                                                 ;   mirror chip RAM)
$F801F4  beq.b    $f801fa                        ; Wrap detected = found top of chip RAM
$F801F6  cmp.l    (a3), d1                      ; Did the write stick?
$F801F8  beq.b    $f801de                        ; Yes = RAM present, restore old value, continue
$F801FA  move.l   d0, (a3)                      ; Restore probe location
$F801FC  move.l   a1, (a0)                      ; Restore location 0
```

After this loop, a3 = top of chip RAM (e.g., $80000 for 512KB, $100000 for 1MB, $200000 for 2MB).

### Phase 8: ExecBase Construction ($F801FE-$F802E2)

```
; --- Allocate chip memory for ExecBase structure + jump table ---
$F801FE  move.l   d2, -(a7)                     ; Push preserved KickMemPtr
$F80200  lea.l    $400.w, a0                    ; a0 = base of usable memory (after vectors)
$F80204  lea.l    $f8031c(pc), a1               ; a1 = "chip memory" string (for MemList entry)
$F80208  move.l   a3, d0                        ; d0 = chip RAM size
$F8020A  move.l   a0, d1                        ; d1 = start address
$F8020C  sub.l    d1, d0                        ; d0 = usable size ($400 to top)
$F8020E  move.w   #$303, d1                     ; d1 = MEMF_CHIP|MEMF_PUBLIC|MEMF_LOCAL (attributes)
$F80212  moveq    #$f6, d2                      ; d2 = priority -10
$F80214  bsr.w    $f81f32                        ; Call AddMemList equivalent (raw, pre-exec)

; --- Allocate ExecBase positive data ---
$F8021A  lea.l    $400.w, a0                    ; Memory pool starts at $400
$F8021E  move.l   #$57c, d0                     ; Size = $57C bytes (1404) for ExecBase + overhead
$F80224  bsr.w    $f81c02                        ; Raw AllocMem equivalent
$F80228  movea.l  d0, a6                        ; a6 = allocated block
$F8022A  suba.w   #$fce8, a6                    ; Adjust: a6 = ExecBase pointer
                                                 ;   ($FFFFFCE8 = -$318 = negative size of jump table)
                                                 ;   So ExecBase = allocation + $318 (jump table before it)
$F8022E  move.l   a6, $4.w                      ; Store ExecBase at AbsExecBase (location 4)

; --- Clear ExecBase structure ---
$F80232  movea.l  a6, a0                        ;
$F80234  move.w   #$98, d1                      ; 153 longwords = 612 bytes (close to SYSBASESIZE=632)
$F80238  clr.l    (a0)+
$F8023A  dbra     d1, $f80238

; --- Populate ExecBase fields from ROM header and probed values ---
$F8023E  move.w   $f8000e(pc), $22(a6)          ; SoftVer = ROM revision word (175)
$F80244  move.w   a2, $128(a6)                  ; AttnFlags = CPU detection result
$F80248  move.l   a3, $3e(a6)                   ; MaxLocMem = top of chip RAM
$F8024C  move.l   a5, $26(a6)                   ; ChkBase = old ExecBase complement (or 0)
$F80250  movem.l  d2-d4, $222(a6)               ; Restore KickMemPtr/KickTagPtr/KickCheckSum
$F80256  movem.l  d5-d7, $2a(a6)                ; Restore ColdCapture/CoolCapture/WarmCapture
```

### Phase 9: "HELP" Cookie Check ($F8025C-$F80272)

```
; --- Check for "HELP" diagnostic cookie at address 0 ---
$F8025C  moveq    #$ff, d6                      ; d6 = default ($FF = no debug info)
$F8025E  cmpi.l   #$48454c50, $0.w              ; Compare location 0 with "HELP" ($48454C50)
$F80266  bne.b    $f80272                        ; Not present, skip
$F80268  movem.l  $100.w, d6-d7                 ; Read debug info from $100-$107
$F8026E  bset.b   #$1f, d6                      ; Set high bit to flag "debug info present"
$F80272  movem.l  d6-d7, $202(a6)               ; Store in ExecBase+$202 (ex_Pad0 area)
```

The "HELP" cookie is a debug mechanism: if a diagnostic tool writes "HELP" to
address 0 and debug parameters at $100, the ROM preserves them through reset.

### Phase 10: Function Table and Library Init ($F80278-$F802F6)

```
; --- Install exec.library function vectors ---
$F80278  movea.l  a6, a0                        ; a0 = ExecBase (target library base)
$F8027A  lea.l    $f81f84(pc), a1               ; a1 = function offset table
$F8027E  movea.l  a1, a2                        ; a2 = dispatch base (same as table)
$F80280  bsr.w    $f81ad0                        ; MakeFunctions(ExecBase, funcTable, funcDispBase)
                                                 ;   Builds JMP instructions at negative offsets
                                                 ;   from ExecBase. Each table entry is a word
                                                 ;   offset relative to the table base. The routine
                                                 ;   writes: JMP abs.l (6 bytes) for each function.
$F80284  move.w   d0, $10(a6)                   ; lib_NegSize = total jump table size
$F80288  move.w   #$264, $12(a6)                ; lib_PosSize = $264 (612 bytes)

; --- Initialise system lists (MemList, LibList, etc.) ---
$F8028E  lea.l    $17a(a6), a0                  ; a0 = &LibList
$F80292  move.l   a0, $8(a0)                    ; lh_TailPred = &lh_Head (empty list)
$F80296  addq.l   #$4, a0                       ; Point to lh_Tail
$F80298  clr.l    (a0)                          ; lh_Tail = NULL
$F8029A  move.l   a0, -(a0)                     ; lh_Head = &lh_Tail (empty list sentinel)

; --- Add chip RAM to memory list ---
$F802AA  lea.l    $400.w, a1                    ; a1 = chip RAM start
$F802AE  bsr.w    $f81904                        ; Enqueue memory into MemList

; --- Probe expansion memory ($C00000-$DC0000) ---
$F802B2  lea.l    $c00000.l, a0                 ; Slow RAM start
$F802B8  lea.l    $dc0000.l, a1                 ; Slow RAM end
$F802BE  bsr.w    $f80328                        ; Memory probe subroutine
$F802C2  move.l   a4, $4e(a6)                   ; MaxExtMem = top of expansion memory (or NULL)

; --- Add expansion memory to MemList if found ---
$F802C6  lea.l    $c00000.l, a0
$F802CC  lea.l    $f80321(pc), a1               ; a1 = "expansion memory" name string
$F802D0  move.l   a4, d0
$F802D2  beq.b    $f802e2                        ; No expansion RAM found, skip
$F802D4  move.l   a0, d1
$F802D6  sub.l    d1, d0                        ; d0 = expansion RAM size
$F802D8  move.w   #$305, d1                     ; MEMF_FAST|MEMF_PUBLIC|MEMF_LOCAL
$F802DC  moveq    #$fb, d2                      ; Priority = -5
$F802DE  bsr.w    $f81f26                        ; AddMemList equivalent

; --- Set background to mid-grey (progress indicator) ---
$F802E2  move.w   #$888, $dff180.l              ; COLOR00 = $888
```

### Phase 11: Resident Scan and InitCode ($F802EA-$F80306)

```
; --- Scan ROM for Resident modules and build sorted list ---
$F802EA  lea.l    $f80308(pc), a0               ; a0 = ROM region descriptor table:
                                                 ;   Each entry is a pair (start, end) of addresses
                                                 ;   to scan. Terminated by $FFFF.
                                                 ;   Regions: $F80000-$100000 (ROM),
                                                 ;            $F00000-$F80000 (diag ROM area)
$F802EE  bsr.w    $f80d22                        ; Resident scan subroutine
                                                 ;   Scans for $4AFC words, validates rt_MatchTag,
                                                 ;   builds a priority-sorted linked list of
                                                 ;   Resident pointers.
$F802F2  move.l   d0, $12c(a6)                  ; ResModules = pointer to sorted Resident array

; --- Execute all RTF_COLDSTART modules via InitCode ---
$F802F6  moveq    #$2, d0                       ; d0 = startClass (RTF_SINGLETASK | RTF_COLDSTART)
$F802F8  moveq    #$0, d1                       ; d1 = version 0 (accept all versions)
$F802FA  jsr      -$48(a6)                      ; InitCode(RTF_COLDSTART, 0)
                                                 ;   LVO -72 = InitCode
                                                 ;   Walks ResModules list, calls InitResident for
                                                 ;   each module whose rt_Flags match startClass
                                                 ;   and whose rt_Version >= version.
                                                 ;   This starts the entire Amiga OS.

; --- Should never reach here (InitCode starts multitasking) ---
$F802FE  move.w   #$f0f, $dff180.l              ; COLOR00 = magenta (panic: InitCode returned!)
$F80306  bra.b    $f80306                        ; Infinite loop -- dead end
```

### ROM Region Descriptor Table ($F80308)

```
$F80308  dc.w  $00F8                            ; Region 1 start high word
$F8030A  dc.l  $00000100                        ;   ... but actually pairs of longs:
$F8030E  dc.l  $0000F000                        ; ($F80000, $100000) = main ROM
$F80312  dc.l  $0000F800                        ; ($F00000, $F80000) = diagnostic ROM area
$F8031A  dc.w  $FFFF                            ; End marker

; --- String data ---
$F8031C  "chip memory"                          ; Name for chip RAM MemList entry
$F80328  ...                                    ; (memory probe subroutine follows)
```

---

## 5. V40 Reset Trace (Divergences from V37)

The V40 boot follows the same structure but adds hardware-specific code for
A600/A1200 machines (Gayle chip, PCMCIA).

### New Phase: PCMCIA Boot Check ($F8010A-$F80150)

Between the diagnostic ROM check and OVL clear, V40 inserts a PCMCIA card boot
sequence. This is absent from V37.

```
; --- Reset Gayle identification sequence ---
$F8010A  move.b   #$0, $da8000.l                ; Write 0 to Gayle ID register
                                                 ;   Resets the Gayle detection state machine
$F80112  nop                                    ; Bus settle

; --- Check for PCMCIA boot ROM at $A00000 (attribute memory) ---
$F80114  lea.l    $a00000.l, a1                 ; a1 = PCMCIA attribute memory base
$F8011A  cmpi.b   #$91, (a1)                    ; Check CIS tuple byte 0 = $91
$F8011E  bne.b    $f80152                        ;   (CISTPL_VERS_1 with link indicator)
$F80120  addq.l   #$2, a1                       ; Attribute memory is byte-wide, skip odd bytes
$F80122  cmpi.b   #$05, (a1)                    ; CIS tuple byte 1 = $05
$F80126  bne.b    $f80152
$F80128  addq.l   #$2, a1
$F8012A  cmpi.b   #$23, (a1)                    ; CIS tuple byte 2 = $23
$F8012E  bne.b    $f80152

; --- Read 4-byte boot offset from CIS data ---
$F80130  addq.l   #$2, a1
$F80132  move.b   (a1), d0                      ; Read byte 0 of offset
$F80134  ror.l    #$8, d0                       ; Shift into high byte
$F80136  addq.l   #$2, a1                       ;   (repeat for all 4 bytes)
$F80138  move.b   (a1), d0
$F8013A  ror.l    #$8, d0
$F8013C  addq.l   #$2, a1
$F8013E  move.b   (a1), d0
$F80140  ror.l    #$8, d0
$F80142  addq.l   #$2, a1
$F80144  move.b   (a1), d0
$F80146  ror.l    #$8, d0

; --- Jump to PCMCIA boot code ---
$F80148  lea.l    $600000.l, a0                 ; a0 = PCMCIA common memory base
$F8014E  adda.l   d0, a0                        ; Add offset from CIS
$F80150  jmp      (a0)                          ; Boot from PCMCIA card
```

### Additional CIA/Gayle Clearing ($F80152-$F8016C)

```
; --- Re-enable Gayle after PCMCIA check ---
$F80152  move.b   #$1, $da8000.l                ; Gayle ID register: restart detect sequence

; --- Clear additional CIA addresses (A600-specific) ---
$F8015A  clr.b    $bfa001.l                     ; Secondary CIA-A address (Gayle-mapped)
$F80160  clr.b    $bfa201.l                     ; Secondary CIA-A DDR (Gayle-mapped)
$F80166  clr.b    $bfe001.l                     ; Standard CIAA PRA: clear OVL
$F8016C  move.b   #$3, $bfe201.l                ; Standard CIAA DDRA: OVL + LED as outputs
```

The extra writes to $BFA001 and $BFA201 reset additional hardware state on
A600/A1200 machines where Gayle remaps some CIA addresses.

### ExecBase Version Check ($F80224-$F8022C)

V40 adds a check for the previous ExecBase version before cold-starting:

```
; --- If previous ExecBase was V40+, preserve ex_RamLibPrivate area ---
$F80224  cmpi.w   #$28, $14(a6)                 ; Check lib_Version >= 40 ($28)
$F8022A  bne.b    $f80230                        ; Skip if older
$F8022C  movea.l  $20e(a6), a4                  ; a4 = previous ExecBase+$20E
                                                 ;   (ex_EClockFrequency+2 area -- preserves
                                                 ;    E-clock calibration data across warm reboot)
```

### 68040 Page Size Detection ($F80270-$F8027C)

V40 checks the AttnFlags for 68040 and adjusts the memory base:

```
$F80270  move.l   a2, d2                        ; d2 = AttnFlags
$F80272  btst.b   #$3, d2                       ; Test AFB_68040
$F80276  beq.b    $f8027c                        ; Not 040, use $400 base
$F80278  lea.l    $1000.w, a0                   ; 040: memory base at $1000 (4KB page boundary)
                                                 ;   The 68040 MMU uses 4KB pages; placing the
                                                 ;   base at $1000 avoids page 0 conflicts.
```

### Larger ExecBase ($F80294-$F80306)

```
$F80294  move.l   #$5b0, d0                     ; ExecBase allocation = $5B0 (1456 bytes)
                                                 ;   vs V37's $57C (1404 bytes)
                                                 ;   Difference: 52 bytes for V39 additions
                                                 ;   (ex_MemHandlers list, ex_MemHandler pointer)
$F802A0  suba.w   #$fcc8, a6                    ; Negative size = $338 (824 bytes)
                                                 ;   vs V37's $318 (792 bytes)
                                                 ;   Difference: 32 bytes = 5 more LVO entries
                                                 ;   (V39: CreatePool/DeletePool/AllocPooled/
                                                 ;    FreePooled/AttemptSemaphoreShared + ColdReboot)
$F80306  move.w   #$278, $12(a6)                ; lib_PosSize = $278 (632 bytes = SYSBASESIZE)
                                                 ;   vs V37's $264 (612 bytes)
```

---

## 6. Comparison Table: V1.2 vs V2.04 vs V3.1

| Feature | V1.2 (V33) | V2.04 (V37) | V3.1 (V40) |
|---------|-----------|-------------|-------------|
| ROM size | 256 KB | 512 KB | 512 KB |
| Load address | $FC0000 (or $F80000 mirrored) | $F80000 | $F80000 |
| Initial PC | $FC00D2 | $F800D2 | $F800D2 |
| SSP cookie | $11114EF9 | $11144EF9 | $11144EF9 |
| Checksum algorithm | Same one's complement sum | Same | Same |
| Diag ROM check | Yes, at $F00000 | Yes, at $F00000 | Yes, at $F00000 |
| PCMCIA boot | No | No | Yes ($A00000 CIS check) |
| Gayle support | No | No | Yes ($DA8000 reset) |
| OVL clear | $BFE001 | $BFE001 | $BFA001 + $BFE001 |
| ExecBase validation | ChkBase + ChkSum | ChkBase + ChkSum | ChkBase + ChkSum + version check |
| ColdCapture | Supported | Supported | Supported |
| CPU detection | 68000/010/020 | 68000/010/020/030/040 + FPU | 68000/010/020/030/040/060 + FPU |
| Chip RAM probe | 16KB steps to 512KB | 16KB steps to 2MB | 16KB steps to 2MB |
| Expansion RAM probe | $C00000-$C80000 | $C00000-$DC0000 | $C00000-$DC0000 |
| ExecBase PosSize | ~$200 | $264 (612) | $278 (632) |
| ExecBase NegSize | ~$280 | ~$318 (792) | ~$338 (824) |
| Memory base | $400 | $400 | $400 (68000) / $1000 (68040) |
| ROMTag count | ~25 | 39 | 44 |
| exec.library LVOs | ~90 | 132 | 137 |
| Boot colour sequence | grey->green->blue | dark grey->mid grey->InitCode | dark grey->mid grey->InitCode |
| Error: bad checksum | Red screen | Red + LED flash | Red + LED flash |
| Error: bad RAM | Yellow screen | Yellow + LED flash | Yellow + LED flash |
| Error: InitCode returns | N/A | Magenta ($F0F) infinite loop | Magenta ($F0F) infinite loop |

---

## 7. ColdReboot Disassembly

ColdReboot (exec LVO -$2D6 / -726) performs a clean hardware reset.

### V37 ColdReboot ($F80CAE)

```
; --- ColdReboot: clean system reset ---
; Must be called with a6 = ExecBase (standard exec calling convention).
; Steps: 1) disable interrupts, 2) flush caches, 3) enter supervisor,
;        4) compute ROM entry, 5) RESET instruction, 6) jump to ROM.

$F80CAE  move.w   #$4000, $dff09a.l             ; INTENA: disable master interrupt enable
                                                 ;   Bit 14 set, bit 15 clear = CLEAR bit 14
$F80CB6  moveq    #$0, d0                       ; d0 = 0 (cache bits)
$F80CB8  moveq    #$ff, d1                      ;
$F80CBA  jsr      -$288(a6)                     ; CacheControl(0, $FF) -- LVO -648
                                                 ;   Disable all caches before reset

$F80CBE  lea.l    $f80cc8(pc), a5               ; a5 = supervisor mode code address
$F80CC2  jsr      -$1e(a6)                      ; Supervisor(a5) -- LVO -30
                                                 ;   Enter supervisor mode, execute code at (a5)
                                                 ;   (does not return; the code below runs in
                                                 ;    supervisor mode with full privileges)

; --- Supervisor-mode reset sequence (runs with interrupts disabled) ---
$F80CC8  lea.l    $1000000.l, a0                ; a0 = $01000000 (16MB mark)
$F80CCE  suba.l   -$14(a0), a0                  ; a0 -= [$FFFFFFEC] = a0 -= ROM size word
                                                 ;   $FFFFFFEC is the ROM footer location that
                                                 ;   contains the ROM base offset. For a 512KB ROM,
                                                 ;   $01000000 - $80000 = $F80000.
                                                 ;   This computes the ROM base address dynamically,
                                                 ;   independent of where the ROM is mapped.
$F80CD2  movea.l  $4(a0), a0                    ; a0 = ROM[4] = initial PC from reset vector
$F80CD6  subq.l   #$2, a0                       ; a0 -= 2 (skip the first instruction: lea $400,a7)
                                                 ;   The RESET instruction resets external hardware
                                                 ;   but not the CPU. To get a clean restart, we
                                                 ;   jump to the ROM entry point *after* the stack
                                                 ;   setup (since RESET doesn't load SSP from ROM[0]).
$F80CD8  reset                                  ; Assert RESET line: resets all external hardware
                                                 ;   (custom chips, CIAs, expansion boards)
$F80CDA  jmp      (a0)                          ; Jump to ROM entry point + 2
                                                 ;   The CPU continues from here with all hardware
                                                 ;   in its reset state, but registers preserved.
                                                 ;   The ROM boot code will immediately set up the
                                                 ;   stack and proceed with normal initialisation.
```

### V40 ColdReboot ($F80D9E)

```
; --- Identical structure to V37 ---
$F80D9E  move.w   #$4000, $dff09a.l             ; INTENA: disable master interrupt
$F80DA6  moveq    #$0, d0
$F80DA8  moveq    #$ff, d1
$F80DAA  jsr      -$288(a6)                     ; CacheControl(0, $FF)
$F80DAE  lea.l    $f80db8(pc), a5               ; Supervisor code at $F80DB8
$F80DB2  jsr      -$1e(a6)                      ; Supervisor(a5)

; --- Supervisor-mode sequence (identical algorithm) ---
$F80DB8  lea.l    $1000000.l, a0
$F80DBE  suba.l   -$14(a0), a0                  ; Compute ROM base from footer
$F80DC2  movea.l  $4(a0), a0                    ; Load initial PC
$F80DC6  subq.l   #$2, a0                       ; Skip first instruction
$F80DC8  reset                                  ; Hardware reset
$F80DCA  jmp      (a0)                          ; Jump to ROM
```

**V40 adds a NOP at $F80DB6** between the Supervisor call and the supervisor code,
but the algorithm is otherwise identical.

### HRM Comparison

The Hardware Reference Manual documents ColdReboot as:

> 1. Disable interrupts (INTENA master disable)
> 2. Enter Supervisor mode
> 3. Compute ROM start from ROM size at $FFFFFFEC
> 4. Read initial PC from ROM+4
> 5. Subtract 2 (skip LEA instruction)
> 6. Execute RESET instruction
> 7. JMP to computed address

Both V37 and V40 match this documented sequence exactly, with the addition of
a CacheControl call to flush/disable caches (not mentioned in older HRM editions
but necessary for 68020+ systems).

---

## 8. Task Dispatcher Disassembly

The Amiga's cooperative/preemptive multitasking is driven by three private exec
functions: Schedule, Switch, and Dispatch.

### ExecBase Fields Used by the Dispatcher

| Offset  | Hex    | Field          | Description |
|---------|--------|----------------|-------------|
| +276    | +$114  | ThisTask       | Pointer to currently running Task |
| +280    | +$118  | IdleCount      | Incremented when no tasks ready |
| +284    | +$11C  | DispCount      | Incremented each dispatch |
| +288    | +$120  | Quantum        | Time slice in VBlank ticks |
| +290    | +$122  | Elapsed        | Ticks remaining in current quantum |
| +292    | +$124  | SysFlags       | Bit 7: need reschedule; Bit 6: quantum expired |
| +294    | +$126  | IDNestCnt      | Interrupt disable depth (-1 = enabled) |
| +295    | +$127  | TDNestCnt      | Task disable depth (-1 = enabled) |
| +406    | +$196  | TaskReady      | List of ready-to-run tasks (priority sorted) |
| +420    | +$1A4  | TaskWait       | List of waiting tasks |
| +560    | +$230  | ex_LaunchPoint | Pointer to task launch/restore code |

### Task Structure Fields Used

| Offset | Hex   | Field        | Description |
|--------|-------|--------------|-------------|
| +9     | +$09  | ln_Pri       | Task priority (Node field) |
| +14    | +$0E  | tc_Flags     | TF_SWITCH=$40, TF_LAUNCH=$80, TF_EXCEPT=$20 |
| +15    | +$0F  | tc_State     | TS_RUN=2, TS_READY=3 |
| +16    | +$10  | tc_IDNestCnt | Saved interrupt nest count |
| +26    | +$1A  | tc_SigRecvd  | Received signals |
| +30    | +$1E  | tc_SigExcept | Exception signal mask |
| +42    | +$2A  | tc_ExceptCode| Exception handler (V40 Dispatch checks this) |
| +54    | +$36  | tc_SPReg     | Saved stack pointer (CPU context on stack) |
| +66    | +$42  | tc_Switch    | Custom switch-out hook |
| +70    | +$46  | tc_Launch    | Custom launch hook |

### V37 Switch (LVO -54, at $F8132C)

Switch is called when the current task loses the CPU. It saves the full CPU
context and enters the dispatcher.

```
; --- Entry: called from an interrupt that decided to reschedule ---
$F8132C  move.w   #$2000, sr                    ; Drop to IPL 0 (enable all interrupts briefly)
                                                 ;   This allows any pending higher-priority
                                                 ;   interrupts to fire before we save context.
$F81330  move.l   a5, -(a7)                     ; Save a5 on supervisor stack
$F81332  move     usp, a5                       ; a5 = user stack pointer

; --- Save full CPU register set onto user stack ---
$F81334  movem.l  a0-a6, -(a5)                  ; Push address registers (28 bytes)
$F81338  movem.l  d0-d7, -(a5)                  ; Push data registers (32 bytes)

; --- Set up for context save ---
$F8133C  movea.l  $4.w, a6                      ; a6 = ExecBase
$F81340  move.w   $126(a6), d0                  ; d0 = current IDNestCnt (to save per-task)
$F81344  move.w   #$ffff, $126(a6)              ; IDNestCnt = -1 (interrupts enabled globally)
$F8134A  move.w   #$c000, $dff09a.l             ; INTENA: SET master interrupt enable
                                                 ;   (bit 15=1 means SET, bit 14=master enable)

; --- Save supervisor stack frame (PC + SR) onto user stack ---
$F81352  move.l   (a7)+, $34(a5)                ; Pop saved a5 from supervisor stack,
                                                 ;   store at user_sp+$34 (a5 slot in register frame)
$F81356  move.w   (a7)+, -(a5)                  ; Pop SR from exception frame -> user stack
$F81358  move.l   (a7)+, -(a5)                  ; Pop PC from exception frame -> user stack
                                                 ;   User stack now holds: PC(4) + SR(2) + D0-D7(32)
                                                 ;   + A0-A5(24) + A5_saved(4) + A6(4) = 70 bytes

; --- Load dispatcher state ---
$F8135A  movea.l  $230(a6), a4                  ; a4 = ex_LaunchPoint (task restore code address)
$F8135E  movea.l  $114(a6), a3                  ; a3 = ThisTask

; --- Save context pointer and IDNestCnt in Task structure ---
$F81362  move.w   d0, $10(a3)                   ; tc_IDNestCnt = saved nest count
$F81366  move.l   a5, $36(a3)                   ; tc_SPReg = user stack pointer (context frame)

; --- Call tc_Switch hook if TF_SWITCH is set ---
$F8136A  btst.b   #$6, $e(a3)                   ; Test TF_SWITCH in tc_Flags
$F81370  beq.b    $f8138c                        ; No hook, skip to dispatcher
$F81372  movea.l  $42(a3), a5                   ; a5 = tc_Switch function pointer
$F81376  jsr      (a5)                          ; Call switch-out hook
$F81378  bra.b    $f8138c                        ; Continue to dispatcher
```

### V37 Dispatch (LVO -60, at $F8137A)

Dispatch is the "cold entry" to the dispatcher -- called when there is no current
task context to save (e.g., during system startup or after RemTask).

```
; --- Cold dispatcher entry ---
$F8137A  movea.l  $230(a6), a4                  ; a4 = ex_LaunchPoint
$F8137E  move.w   #$ffff, $126(a6)              ; IDNestCnt = -1 (enable interrupts)
$F81384  move.w   #$c000, $dff09a.l             ; INTENA: enable master interrupt

; --- Dispatcher loop: find next ready task ---
$F8138C  lea.l    $196(a6), a0                  ; a0 = &TaskReady list header
$F81390  move.w   #$2700, sr                    ; IPL 7: disable all interrupts
                                                 ;   (critical section: modifying task lists)
$F81394  movea.l  (a0), a3                      ; a3 = first node in TaskReady list
$F81396  move.l   (a3), d0                      ; d0 = a3->ln_Succ (NULL if list empty)
$F81398  bne.b    $f813aa                        ; Task available, go dispatch it

; --- No tasks ready: idle loop ---
$F8139A  addq.l   #$1, $118(a6)                 ; IdleCount++
$F8139E  bset.b   #$7, $124(a6)                 ; Set "need reschedule" flag in SysFlags
$F813A4  stop     #$2000                        ; STOP: halt CPU until next interrupt
                                                 ;   SR = $2000 (supervisor, IPL 0)
                                                 ;   Any interrupt will wake the CPU.
$F813A8  bra.b    $f81390                        ; Re-check TaskReady list

; --- Dequeue the highest-priority ready task ---
$F813AA  move.l   d0, (a0)                      ; TaskReady->lh_Head = next node (Remove first)
$F813AC  movea.l  d0, a1
$F813AE  move.l   a0, $4(a1)                    ; next->ln_Pred = &TaskReady (list fixup)

; --- Make it the current task ---
$F813B2  move.l   a3, $114(a6)                  ; ThisTask = dequeued task
$F813B6  move.w   $120(a6), $122(a6)            ; Elapsed = Quantum (reset time slice)
$F813BC  bclr.b   #$6, $124(a6)                 ; Clear "quantum expired" flag

; --- Set task state and restore its IDNestCnt ---
$F813C2  move.b   #$2, $f(a3)                   ; tc_State = TS_RUN (2)
$F813C8  move.w   $10(a3), $126(a6)             ; IDNestCnt = task's saved value
$F813CE  tst.b    $126(a6)                      ; Is IDNestCnt >= 0? (interrupts were disabled)
$F813D2  bmi.b    $f813dc                        ; Negative = interrupts enabled, skip
$F813D4  move.w   #$4000, $dff09a.l             ; INTENA: disable master interrupt
                                                 ;   (task had interrupts disabled when it was
                                                 ;    switched out; restore that state)

; --- Drop to IPL 0 and count dispatch ---
$F813DC  move.w   #$2000, sr                    ; IPL 0 (allow interrupts at task's IDNestCnt)
$F813E0  addq.l   #$1, $11c(a6)                 ; DispCount++

; --- Check for pending exceptions (TF_LAUNCH, TF_EXCEPT) ---
$F813E4  move.b   $e(a3), d0                    ; d0 = tc_Flags
$F813E8  andi.b   #$a0, d0                      ; Mask TF_LAUNCH ($80) | TF_EXCEPT ($20)
$F813EC  beq.b    $f813f0                        ; Neither set, go directly to launch
$F813EE  bsr.b    $f81406                        ; Handle launch hook / exception

; --- Restore task context and return to user mode ---
$F813F0  movea.l  $36(a3), a5                   ; a5 = tc_SPReg (saved user stack pointer)
$F813F4  jmp      (a4)                          ; Jump to ex_LaunchPoint (task restore code)
```

### V37 Launch Code (at $F813F6)

This is the code pointed to by `ex_LaunchPoint`. It restores the full CPU context
from the user stack and returns to the task via RTE.

```
$F813F6  lea.l    $42(a5), a2                   ; a2 = end of register save area on user stack
                                                 ;   (a5 points to bottom of saved frame;
                                                 ;    +$42 = 66 bytes up = past all saved regs)
$F813FA  move     a2, usp                       ; Restore user stack pointer
$F813FC  move.l   (a5)+, -(a7)                  ; Push saved PC onto supervisor stack
$F813FE  move.w   (a5)+, -(a7)                  ; Push saved SR onto supervisor stack
$F81400  movem.l  (a5), d0-d7/a0-a6             ; Restore all registers from user stack
$F81404  rte                                    ; Return from exception: loads PC+SR from
                                                 ;   supervisor stack, switches to user mode
```

### V40 Switch and Dispatch

The V40 versions at $F813FA (Switch) and $F81448 (Dispatch) are **byte-for-byte
identical** in algorithm to V37. The only difference is the absolute addresses of
subroutine calls. The dispatcher logic, idle loop, task dequeue, context save/restore,
and launch code are unchanged between V37 and V40.

### Exception and Launch Hook Processing ($F81406 / V37)

```
; --- Called when tc_Flags has TF_LAUNCH or TF_EXCEPT set ---
$F81406  btst.b   #$7, d0                       ; Test TF_LAUNCH
$F8140A  beq.b    $f81416                        ; Not set, check TF_EXCEPT
$F8140C  move.b   d0, d2                        ; Save flags
$F8140E  movea.l  $46(a3), a5                   ; a5 = tc_Launch (custom launch hook)
$F81412  jsr      (a5)                          ; Call launch hook
$F81414  move.b   d2, d0                        ; Restore flags
$F81416  btst.b   #$5, d0                       ; Test TF_EXCEPT
$F8141A  bne.b    $f8141e                        ; Exception pending
$F8141C  rts                                    ; No exception, return to dispatcher

; --- Process task exception ---
$F8141E  bclr.b   #$5, $e(a3)                   ; Clear TF_EXCEPT
$F81424  move.w   #$4000, $dff09a.l             ; INTENA: disable interrupts
$F8142C  addq.b   #$1, $126(a6)                 ; IDNestCnt++ (nest disable)
$F81430  move.l   $1a(a3), d0                   ; d0 = tc_SigRecvd
$F81434  and.l    $1e(a3), d0                   ; d0 &= tc_SigExcept (which signals triggered)
$F81438  eor.l    d0, $1e(a3)                   ; Clear those bits from tc_SigExcept
$F8143C  eor.l    d0, $1a(a3)                   ; Clear those bits from tc_SigRecvd
$F81440  subq.b   #$1, $126(a6)                 ; IDNestCnt-- (un-nest)
$F81444  bge.b    $f8144e                        ; Still nested, don't re-enable
$F81446  move.w   #$c000, $dff09a.l             ; INTENA: re-enable interrupts
```

### Reschedule (LVO -48, V37 at $F82550)

Reschedule is called from the VBlank interrupt handler when the current task's
quantum expires. It triggers a context switch via a software interrupt.

```
$F82550  bset.b   #$7, $124(a6)                 ; Set SysFlags bit 7 ("need reschedule")
$F82556  sne.b    d0                            ; d0 = $FF if bit was already set
$F82558  tst.b    $127(a6)                      ; Check TDNestCnt (task switching disabled?)
$F8255C  bge.b    $f82570                        ; >= 0 = disabled, just return
$F8255E  tst.b    $126(a6)                      ; Check IDNestCnt
$F82562  blt.b    $f8258c                        ; < 0 = interrupts enabled, do reschedule
$F82564  tst.b    d0                            ; Was flag already set?
$F82566  bne.b    $f82570                        ; Yes, don't double-trigger
$F82568  move.w   #$8004, $dff09c.l             ; INTREQ: trigger SOFTINT (level 1 interrupt)
                                                 ;   This causes a deferred reschedule when
                                                 ;   interrupts are re-enabled.
$F82570  rts

; --- Immediate reschedule path (interrupts enabled, task switching allowed) ---
$F8258C  move.l   a5, -(a7)                     ; Save a5
$F8258E  lea.l    $f8259a(pc), a5               ; a5 = supervisor-mode reschedule code
$F82592  jsr      -$1e(a6)                      ; Supervisor(a5)
$F82596  movea.l  (a7)+, a5                     ; Restore a5
$F82598  rts

; --- Supervisor-mode: check if we were in user mode ---
$F8259A  btst.b   #$5, (a7)                     ; Test S bit in stacked SR
$F8259E  bne.b    $f825a4                        ; Was in supervisor mode, can't switch
$F825A0  jmp      -$2a(a6)                      ; Jump to Switch (LVO -$36 = execPrivate4)
                                                 ;   Note: -$2A is NOT -$36; this is
                                                 ;   actually -$2A = an offset within the
                                                 ;   exception frame handling, jumping to the
                                                 ;   Switch entry point via the jump table.
$F825A4  rte                                    ; Can't switch from supervisor mode, return
```

---

## 9. InitCode and Resident Module Initialisation

InitCode (LVO -72) walks the sorted ResModules array and calls InitResident for
each module matching the requested start class.

### V37 InitCode ($F80EFE)

```
$F80EFE  movem.l  d2-d3/a2, -(a7)
$F80F02  movea.l  $12c(a6), a2                  ; a2 = ExecBase->ResModules (sorted array)
$F80F06  move.b   d0, d2                        ; d2 = startClass flags (RTF_COLDSTART etc.)
$F80F08  move.b   d1, d3                        ; d3 = minimum version

; --- Walk the ResModules array ---
$F80F0A  move.l   (a2)+, d0                     ; d0 = next entry
$F80F0C  beq.w    $f80f32                        ; NULL = end of array
$F80F10  bgt.b    $f80f1a                        ; Positive = Resident pointer
$F80F12  bclr.b   #$1f, d0                      ; Negative = pointer to continuation array
$F80F16  movea.l  d0, a2                        ;   (KickTagPtr chain: clear bit 31 to get address)
$F80F18  bra.b    $f80f0a                        ;   Follow chain

; --- Check if this module matches our criteria ---
$F80F1A  movea.l  d0, a1                        ; a1 = Resident structure
$F80F1C  cmp.b    $b(a1), d3                    ; Compare rt_Version with minimum
$F80F20  bgt.b    $f80f0a                        ; Version too old, skip
$F80F22  move.b   $a(a1), d0                    ; d0 = rt_Flags
$F80F26  and.b    d2, d0                        ; Match against startClass
$F80F28  beq.b    $f80f0a                        ; No match, skip
$F80F2A  moveq    #$0, d1                       ; d1 = segList (0 for ROM modules)
$F80F2C  jsr      -$66(a6)                      ; InitResident(a1=resident, d1=segList)
                                                 ;   LVO -$66 = -102 = InitResident
$F80F30  bra.b    $f80f0a                        ; Continue to next module

$F80F32  movem.l  (a7)+, d2-d3/a2
$F80F36  rts
```

The InitResident function ($F80F38) handles two cases:
- **RTF_AUTOINIT clear:** calls `rt_Init` directly as a function
- **RTF_AUTOINIT set:** `rt_Init` points to a data table:
  `{ dataSize(long), funcTable(ptr), dataTable(ptr), initFunc(ptr) }`.
  exec calls MakeLibrary with these parameters to construct the library/device.

### KickTagPtr Chain

The ResModules array supports a chaining mechanism for RAM-resident modules.
If an entry has bit 31 set, it's a pointer (with bit 31 cleared) to another
array of Resident pointers. This allows `KickTagPtr` to extend the ROM's
built-in module list with user-provided modules loaded into RAM.

---

## 10. Resident Scan Subroutine

The Resident scan builds the sorted module array that InitCode uses.

### V37 Resident Scan ($F80D22)

```
; --- Set up a temporary priority-sorted list on the stack ---
$F80D22  movem.l  d3-d4/a2-a4, -(a7)
$F80D26  link.w   a5, #$fff2                    ; Allocate 14 bytes on stack (one List header)
$F80D2A  movea.l  a7, a3                        ; a3 = temporary List
$F80D2C  move.l   a3, $8(a3)                    ; lh_TailPred = &lh_Head (init empty list)
$F80D30  addq.l   #$4, a3
$F80D32  clr.l    (a3)                          ; lh_Tail = NULL
$F80D34  move.l   a3, -(a3)                     ; lh_Head = &lh_Tail

; --- Read region descriptor pairs (start, end) ---
$F80D36  movea.l  a0, a2                        ; a2 = region table
$F80D38  movea.l  (a2)+, a4                     ; a4 = region start
$F80D3A  moveq    #$ff, d0
$F80D3C  cmpa.l   d0, a4                        ; $FFFFFFFF = end of table
$F80D3E  beq.b    $f80d46
$F80D40  move.l   (a2)+, d4                     ; d4 = region end
$F80D42  bsr.b    $f80d52                        ; Scan this region
$F80D44  bra.b    $f80d38                        ; Next region

; --- Convert sorted list to array ---
$F80D46  bsr.w    $f80dea                        ; Flatten list into a contiguous array
                                                 ;   Returns d0 = pointer to array
$F80D4A  unlk     a5
$F80D4C  movem.l  (a7)+, d3-d4/a2-a4
$F80D50  rts

; --- Scan one memory region for $4AFC markers ---
$F80D52  movem.l  d2/a5, -(a7)
$F80D56  move.w   #$4afc, d2                    ; d2 = RTC_MATCHWORD
$F80D5A  move.l   d4, d0                        ; d0 = region end
$F80D5C  sub.l    a4, d0                        ; d0 = region size
$F80D5E  bls.b    $f80d88                        ; Empty region, skip
$F80D60  lsr.l    #$1, d0                       ; d0 = word count (size / 2)
$F80D62  subq.l   #$1, d0                       ; Adjust for dbra

; --- Word-by-word scan using dbra/dbeq double loop ---
$F80D66  swap     d1                            ; d1.hi = outer counter
$F80D68  bra.b    $f80d6c
$F80D6A  cmp.w    (a4)+, d2                     ; Compare word with $4AFC
$F80D6C  dbeq     d0, $f80d6a                   ; Inner loop: scan until match or count exhausted
$F80D70  dbeq     d1, $f80d6a                   ; Outer loop: extends range beyond 65536 words
$F80D74  bne.b    $f80d88                        ; No match found, done

; --- Found $4AFC -- verify rt_MatchTag self-reference ---
$F80D76  lea.l    -$2(a4), a5                   ; a5 = address of the $4AFC word
$F80D7A  cmpa.l   (a4), a5                      ; Does rt_MatchTag point back to here?
$F80D7C  bne.b    $f80d6c                        ; No, false positive, keep scanning
$F80D7E  bsr.w    $f80d8e                        ; Valid ROMTag! Insert into sorted list
$F80D82  movea.l  $6(a5), a4                    ; a4 = rt_EndSkip (continue scan from here)
$F80D86  bra.b    $f80d5a                        ; Scan remainder of region

; --- Insert Resident into priority-sorted list ---
$F80D8E  movea.l  a3, a0                        ; a0 = list header
$F80D90  movea.l  $e(a5), a1                    ; a1 = rt_Name (for FindName comparison)
$F80D94  bsr.w    $f8192a                        ; FindName: check if module already in list
$F80D98  tst.l    d0
$F80D9A  beq.b    $f80dc4                        ; Not found, insert new entry
$F80D9C  movea.l  d0, a1                        ; Found existing entry
$F80D9E  movea.l  $e(a1), a0                    ; a0 = existing entry's Resident
$F80DA2  move.b   $b(a5), d0                    ; d0 = new rt_Version
$F80DA6  cmp.b    $b(a0), d0                    ; Compare with existing version
$F80DAA  blt.w    $f80de8                        ; New is older, skip
$F80DAE  bgt.b    $f80dba                        ; New is newer, replace
$F80DB0  move.b   $d(a5), d0                    ; Same version: compare rt_Pri
$F80DB4  cmp.b    $d(a0), d0                    ; Is new higher priority?
$F80DB8  blt.b    $f80de8                        ; No, keep existing
$F80DBA  move.l   a1, d0                        ; Remove old entry
$F80DBC  bsr.w    $f818c6                        ; Remove(a1) from list
...
```

The scan algorithm uses the classic double-dbra technique to scan more than
64K words: `dbeq d0` handles the low 16 bits, `dbeq d1` the high 16 bits.
When a module with the same name exists, the higher-versioned one wins.
If versions are equal, higher priority wins.

---

## 11. Interesting Findings

### 1. The SSP Cookie ($11144EF9) Doubles as a JMP Instruction

The first 8 bytes of the ROM serve dual purpose:
- As 68000 reset vectors: SSP = $11144EF9, PC = $00F800D2
- As executable code at address 0 (during overlay): the bytes `4EF9 00F800D2`
  decode as `JMP $00F800D2`, providing a direct jump to the boot code.
  The leading `$1114` decodes as `BTST.B D0,(A4)` which is harmless.

### 2. V40 PCMCIA Boot Predates All OS Initialisation

V40's PCMCIA card check runs before the OVL bit is cleared, before interrupts
are configured, and before any memory is probed. A PCMCIA card with the right
CIS tuple signature ($91, $05, $23) can take over the entire machine before
the Amiga OS even starts. This was designed for diagnostic and service cards
used by Commodore service centres.

### 3. The Memory Probe Uses a "Poisoned Mirror" Technique

Rather than simply writing and reading back, the chip RAM probe exploits
address mirroring. It writes a magic value ($F2D4B689) at increasing
addresses and checks whether location 0 changes. If Agnus/Gary mirrors
the write to address 0, the probe has found the chip RAM boundary. This
correctly handles cases where the address bus is partially decoded.

### 4. The "HELP" Debug Cookie

Both V37 and V40 check address 0 for the ASCII string "HELP" ($48454C50).
If found, they read 8 bytes from address $100 and preserve them in ExecBase
at offset $202 (the ex_Pad0/ex_LaunchPoint area, before those fields are
properly initialised). This undocumented mechanism lets diagnostic tools
pass data through a warm reboot.

### 5. ColdReboot Computes ROM Base from the Footer, Not Hardcoded

Rather than assuming the ROM lives at $F80000, ColdReboot reads the ROM size
from $FFFFFFEC (four bytes before the 16MB address space boundary). It
computes `ROM_base = $1000000 - ROM_size` and then reads the initial PC from
`ROM_base + 4`. This makes the code work for both 256KB and 512KB ROMs
regardless of their mapping address.

### 6. The Dispatcher Uses STOP for Idle, Not a Busy Loop

When no tasks are ready, the dispatcher executes `STOP #$2000`, which halts
the CPU until the next interrupt arrives. This saves power compared to a
busy-wait loop and is essential for the A600's power management. The STOP
instruction sets SR to $2000 (supervisor mode, IPL 0), ensuring any interrupt
can wake the CPU.

### 7. ex_LaunchPoint Indirection for the Task Restore

The task restore code address is stored in ExecBase at offset $230
(ex_LaunchPoint) and loaded into a4 before each dispatch. The dispatcher
jumps to (a4) to restore the task's CPU context. This indirection allows
system patches to intercept every task switch by modifying ex_LaunchPoint
-- used by some debugging and profiling tools.

### 8. The V40 A600 Bus at $BFA001

V40 clears two extra addresses ($BFA001 and $BFA201) that don't correspond
to standard CIA registers. On A600 machines, the Gayle custom chip maps
additional control registers at these addresses. On machines without Gayle,
these writes are harmless bus cycles to unmapped space. This is the first
visible hardware-specific divergence between V37 (A500+) and V40 (A500/A600).

### 9. 68040 Page Alignment in V40

V40 moves the memory base from $400 to $1000 on 68040 systems. The 68040's
MMU uses 4KB pages, so aligning the base to a page boundary avoids splitting
the vector table across pages and simplifies future MMU setup by the OS.

### 10. audio.device Frozen at V37

Both Kickstart 2.04 and 3.1 ship the same `audio.device 37.10 (26.4.91)`.
The audio subsystem saw no changes between these releases, likely because
the Paula audio hardware was unchanged and the device worked reliably.
Similarly, `potgo.resource` and `misc.resource` are identical V37 builds
in both ROMs.

---

## 12. exec.library Init (RTF_SINGLETASK)

The exec.library ROMTag has `RTF_SINGLETASK` (not `RTF_AUTOINIT`), so its
`rt_Init` points to a function, not a data table. This function runs as part
of InitCode's walk through the module list.

### V37 exec Init ($F80420)

```
; --- Set progress colour ---
$F80420  lea.l    $dff000.l, a0
$F80426  move.w   #$aaa, $180(a0)               ; COLOR00 = light grey (exec init starting)

; --- Scan ROM for Resident modules ---
$F8042C  lea.l    $f8040c(pc), a0               ; a0 = ROM region descriptor table
$F80430  bsr.w    $f80cdc                        ; Resident scan (builds sorted module list)
$F80434  move.l   d0, $12c(a6)                  ; ExecBase->ResModules = sorted array

; --- Allocate memory for new ExecBase ---
$F80438  move.l   #$57c, d0                     ; Size = $57C (1404 bytes)
$F8043E  move.l   #$10001, d1                   ; MEMF_PUBLIC | MEMF_CLEAR
$F80444  jsr      -$c6(a6)                      ; AllocMem(1404, MEMF_PUBLIC|MEMF_CLEAR)
$F80448  tst.l    d0
$F8044A  beq.w    $f8051a                        ; Allocation failed, fatal error

; --- Set up new ExecBase at proper offset ---
$F8044E  movea.l  d0, a5                        ; a5 = allocated block
$F80450  suba.w   #$fce8, a5                    ; Adjust for negative jump table size
                                                 ;   $FCE8 = -$318 = -(792 bytes of JMP entries)
                                                 ;   New ExecBase = block + 792

; --- Copy Library Node identification from ROM ---
$F80454  lea.l    $8(a5), a1                    ; a1 = &new_ExecBase->ln_Type
$F80458  lea.l    $f80744(pc), a0               ; a0 = ROM data: library type/name/version info
$F8045C  moveq    #$c, d0                       ; Copy 13 words (26 bytes) of Library header data
$F8045E  move.w   (a0)+, (a1)+
$F80460  dbra     d0, $f8045e

; --- Build function jump table ---
$F80464  movea.l  a5, a0                        ; a0 = new ExecBase (target)
$F80466  lea.l    $f81f84(pc), a1               ; a1 = function offset table (132 entries)
$F8046A  movea.l  a1, a2                        ; a2 = dispatch base (same as table)
$F8046C  jsr      -$5a(a6)                      ; MakeFunctions(a0, a1, a2) -- LVO -90
$F80470  move.w   d0, $10(a5)                   ; new_ExecBase->lib_NegSize = jump table size

; --- Copy preserved fields from bootstrap ExecBase ---
$F80474  move.w   $22(a6), $22(a5)              ; SoftVer
$F8047A  move.l   $3e(a6), $3e(a5)              ; MaxLocMem
$F80480  move.l   $4e(a6), $4e(a5)              ; MaxExtMem
$F80486  movem.l  $202(a6), d0-d1               ; ex_Pad0 + ex_LaunchPoint (HELP debug data)
$F8048C  movem.l  d0-d1, $202(a5)
$F80492  move.w   $128(a6), $128(a5)            ; AttnFlags
$F80498  move.l   $12c(a6), $12c(a5)            ; ResModules
$F8049E  movem.l  $222(a6), d0-d2               ; KickMemPtr, KickTagPtr, KickCheckSum
$F804A4  movem.l  d0-d2, $222(a5)
$F804AA  move.l   $2e(a6), $2e(a5)              ; CoolCapture

; --- Transfer MemList and LibList from old to new ExecBase ---
;     (patches the linked list node pointers to reference the new base)
$F804B0  lea.l    $142(a6), a2                  ; Source MemList in old ExecBase
$F804B4  lea.l    $142(a5), a3                  ; Dest MemList in new ExecBase
$F804B8  movea.l  (a2), a0                      ; First node
$F804BA  move.l   a0, (a3)                      ; Copy head pointer
$F804BC  move.l   a3, $4(a0)                    ; Fix node's back-pointer to new list header
$F804C0  movea.l  $8(a2), a0                    ; TailPred
$F804C4  move.l   a0, $8(a3)                    ; Copy tail-pred
$F804C8  move.l   a3, (a0)                      ; Fix last node's forward pointer
...                                             ; Same for LibList at $17A
```

The key insight: exec.library creates a **new** ExecBase in properly allocated
memory, copies all essential state from the bootstrap ExecBase (which was
crudely allocated at $400 during the pre-exec phase), and then swaps AbsExecBase
at location 4 to point to the new one. This is why the exec init has
`RTF_SINGLETASK` -- it must run before multitasking starts, since it
replaces the entire system base structure.

### V40 exec Init ($F804D4)

The V40 version follows the same pattern but allocates $5B0 bytes (vs $57C)
for the larger SYSBASESIZE, and copies additional V36+ fields including
ex_RamLibPrivate. It also copies 4 registers from offset $202 instead of 2
(preserving more of the extended ExecBase fields).

---

## 13. MakeFunctions: Building the Library Jump Table

MakeFunctions (LVO -90) constructs the `JMP abs.l` instruction array that
forms a library's function dispatch table.

### V37 MakeFunctions ($F81AD0)

```
; --- Entry: a0 = library base, a1 = function table, a2 = dispatch base ---
; If a2 != 0: table contains relative word offsets from a2
; If a2 == 0: table contains absolute longword pointers
; Both are terminated by $FFFF (word) or $FFFFFFFF (long).

$F81AD0  move.l   a3, -(a7)                     ; Save a3
$F81AD2  moveq    #$0, d0                       ; d0 = total size counter

; --- Check dispatch mode ---
$F81AD4  move.l   a2, d1                        ; Is a2 (dispatch base) NULL?
$F81AD6  beq.b    $f81aee                        ; Yes, use absolute longword mode

; --- Relative word offset mode ---
$F81AD8  move.w   (a1)+, d1                     ; Read next word offset from table
$F81ADA  cmpi.w   #$ffff, d1                    ; End marker?
$F81ADE  beq.b    $f81b02                        ; Done
$F81AE0  lea.l    (a2, d1.w), a3                ; a3 = dispatch_base + signed_offset
                                                 ;   = actual function address
$F81AE4  move.l   a3, -(a0)                     ; Store target address (grows downward)
$F81AE6  move.w   #$4ef9, -(a0)                 ; Store JMP opcode ($4EF9 = JMP abs.l)
$F81AEA  addq.l   #$6, d0                       ; Count 6 bytes per entry
$F81AEC  bra.b    $f81ad8                        ; Next entry

; --- Absolute longword mode ---
$F81AEE  move.l   (a1)+, d1                     ; Read next absolute pointer
$F81AF0  cmpi.l   #$ffffffff, d1                ; End marker?
$F81AF6  beq.b    $f81b02                        ; Done
$F81AF8  move.l   d1, -(a0)                     ; Store target address
$F81AFA  move.w   #$4ef9, -(a0)                 ; Store JMP opcode
$F81AFE  addq.l   #$6, d0                       ; Count 6 bytes
$F81B00  bra.b    $f81aee                        ; Next entry

; --- Finalise ---
$F81B02  movea.l  d0, a3                        ; a3 = total size
$F81B04  jsr      -$27c(a6)                     ; SumLibrary? (private, recalculates checksum)
$F81B08  move.l   a3, d0                        ; Return total size in d0
$F81B0A  movea.l  (a7)+, a3
$F81B0C  rts
```

The function table for exec.library uses relative word offsets (132 entries for
V37, 137 for V40). Each entry is a signed 16-bit displacement from the table
base. The resulting jump table occupies `132 * 6 = 792 bytes` (V37) or
`137 * 6 = 822 bytes` (V40) at negative offsets from ExecBase.

When user code calls, for example, `jsr -$48(a6)` (InitCode), the CPU
executes the `JMP abs.l` instruction at `ExecBase - $48`, which jumps to
the actual InitCode implementation in the ROM.

---

## Appendix A: ExecBase Field Offset Reference

Computed from NDK 3.9 `execbase.i` and verified against vAmiga `OSDebuggerTypes.h`.

```
+$000 (+  0)  LibNode (34 bytes: Node + Library fields)
  +$000  ln_Succ (4)        +$004  ln_Pred (4)
  +$008  ln_Type (1)        +$009  ln_Pri (1)
  +$00A  ln_Name (4)        +$00E  lib_Flags (1)
  +$00F  lib_pad (1)        +$010  lib_NegSize (2)
  +$012  lib_PosSize (2)    +$014  lib_Version (2)
  +$016  lib_Revision (2)   +$018  lib_IdString (4)
  +$01C  lib_Sum (4)        +$020  lib_OpenCnt (2)
+$022 (+ 34)  SoftVer (2)
+$024 (+ 36)  LowMemChkSum (2)
+$026 (+ 38)  ChkBase (4)
+$02A (+ 42)  ColdCapture (4)
+$02E (+ 46)  CoolCapture (4)
+$032 (+ 50)  WarmCapture (4)
+$036 (+ 54)  SysStkUpper (4)
+$03A (+ 58)  SysStkLower (4)
+$03E (+ 62)  MaxLocMem (4)
+$042 (+ 66)  DebugEntry (4)
+$046 (+ 70)  DebugData (4)
+$04A (+ 74)  AlertData (4)
+$04E (+ 78)  MaxExtMem (4)
+$052 (+ 82)  ChkSum (2)
+$054 (+ 84)  IntVects[16] (192 bytes, 12 bytes each: iv_Data + iv_Code + iv_Node)
+$114 (+276)  ThisTask (4)
+$118 (+280)  IdleCount (4)
+$11C (+284)  DispCount (4)
+$120 (+288)  Quantum (2)
+$122 (+290)  Elapsed (2)
+$124 (+292)  SysFlags (2)
+$126 (+294)  IDNestCnt (1)
+$127 (+295)  TDNestCnt (1)
+$128 (+296)  AttnFlags (2)
+$12A (+298)  AttnResched (2)
+$12C (+300)  ResModules (4)
+$130 (+304)  TaskTrapCode (4)
+$134 (+308)  TaskExceptCode (4)
+$138 (+312)  TaskExitCode (4)
+$13C (+316)  TaskSigAlloc (4)
+$140 (+320)  TaskTrapAlloc (2)
+$142 (+322)  MemList (14)
+$150 (+336)  ResourceList (14)
+$15E (+350)  DeviceList (14)
+$16C (+364)  IntrList (14)
+$17A (+378)  LibList (14)
+$188 (+392)  PortList (14)
+$196 (+406)  TaskReady (14)
+$1A4 (+420)  TaskWait (14)
+$1B2 (+434)  SoftInts[5] (80 bytes, 16 bytes each)
+$202 (+514)  LastAlert[4] (16 bytes)
+$212 (+530)  VBlankFrequency (1)
+$213 (+531)  PowerSupplyFrequency (1)
+$214 (+532)  SemaphoreList (14)
+$222 (+546)  KickMemPtr (4)
+$226 (+550)  KickTagPtr (4)
+$22A (+554)  KickCheckSum (4)
--- V36 additions ---
+$22E (+558)  ex_Pad0 (2)
+$230 (+560)  ex_LaunchPoint (4)
+$234 (+564)  ex_RamLibPrivate (4)
+$238 (+568)  ex_EClockFrequency (4)
+$23C (+572)  ex_CacheControl (4)
+$240 (+576)  ex_TaskID (4)
+$244 (+580)  ex_PuddleSize (4)        (undocumented, from vAmiga)
+$248 (+584)  ex_PoolThreshold (4)     (undocumented, from vAmiga)
+$24C (+588)  ex_PublicPool (12)       (MinList, undocumented, from vAmiga)
+$258 (+600)  ex_MMULock (4)
+$25C (+604)  ex_Reserved2 (12)
--- V39 additions ---
+$268 (+616)  ex_MemHandlers (12, MinList)
+$274 (+628)  ex_MemHandler (4)
Total: SYSBASESIZE = 632 bytes
```

## Appendix B: Exec LVO Quick Reference

Key Library Vector Offsets for exec.library (bias 30):

```
LVO   -30 = -$01E : Supervisor
LVO   -36 = -$024 : (private: Schedule?)
LVO   -42 = -$02A : (private: Schedule/Enqueue ready task)
LVO   -48 = -$030 : (private: Reschedule)
LVO   -54 = -$036 : (private: Switch -- save context, enter dispatcher)
LVO   -60 = -$03C : (private: Dispatch -- cold enter dispatcher)
LVO   -66 = -$042 : (private)
LVO   -72 = -$048 : InitCode
LVO   -78 = -$04E : InitStruct
LVO   -84 = -$054 : MakeLibrary
LVO   -90 = -$05A : MakeFunctions
LVO   -96 = -$060 : FindResident
LVO  -102 = -$066 : InitResident
LVO  -108 = -$06C : Alert
LVO  -120 = -$078 : Disable
LVO  -126 = -$07E : Enable
LVO  -132 = -$084 : Forbid
LVO  -138 = -$08A : Permit
LVO  -198 = -$0C6 : AllocMem
LVO  -210 = -$0D2 : FreeMem
LVO  -270 = -$10E : Enqueue
LVO  -282 = -$11A : AddTask
LVO  -318 = -$13E : Wait
LVO  -324 = -$144 : Signal
LVO  -552 = -$228 : OpenLibrary
LVO  -612 = -$264 : SumKickData
LVO  -648 = -$288 : CacheControl
LVO  -726 = -$2D6 : ColdReboot
```

## Appendix C: Custom Chip Register Quick Reference

Registers referenced in the boot trace (all offsets from $DFF000 base):

```
Offset  Name      Used in boot as
+$032   SERPER    Serial port period. Set to $174 during early init.
+$096   DMACON    DMA control. Written $7FFF to disable all channels.
+$09A   INTENA    Interrupt enable. Written $7FFF to clear all, $C000 to set master,
                  $4000 to clear master.
+$09C   INTREQ    Interrupt request. Written $7FFF to clear all pending,
                  $8004 to trigger SOFTINT for deferred reschedule.
+$100   BPLCON0   Bitplane control. Written $200 for blank display (COLOR burst only).
+$110   BPL1DAT   Bitplane 1 data. Cleared to $0.
+$180   COLOR00   Background colour. Progress indicator:
                    $444 = dark grey (hardware init)
                    $888 = mid grey (memory configured)
                    $AAA = light grey (exec init)
                    $F00 = red (ROM checksum failure)
                    $0F0 = yellow (never used; d0 set but overwritten)
                    $F0F = magenta (InitCode returned -- fatal)
                    $111 = near-black (V40 initial colour, subtler than V37's $444)
```

CIA registers referenced:

```
Address   Name      Description
$BFE001   CIAA PRA  Bit 0 = OVL (overlay: 1=ROM at $0, 0=RAM at $0)
                    Bit 1 = /LED (power LED: 0=on, 1=off/dim)
$BFE201   CIAA DDRA Data direction for PRA. Boot sets to $03 (OVL + LED as outputs).
$BFA001   (Gayle)   V40 only. Secondary CIA-A on A600/A1200.
$BFA201   (Gayle)   V40 only. Secondary CIA-A DDR.
$DA8000   Gayle ID  V40 only. Write 0 to reset, write 1 to start ID sequence.
```

## Appendix D: Boot Error Diagnostics

The boot ROM communicates errors through screen colour and the power LED.

### Colour Codes

| Colour | Hex   | Meaning |
|--------|-------|---------|
| Dark grey | $444 (V37), $111 (V40) | Hardware init complete, starting checksum |
| Red | $F00 | ROM checksum failed |
| Yellow | $0F0 | RAM test failed (vector area defective) |
| Mid grey | $888 | Memory configured, about to scan ROMTags |
| Light grey | $AAA | exec.library init running |
| Magenta | $F0F | InitCode returned (should never happen) |

### LED Flash on Checksum/RAM Failure

Both V37 and V40 flash the power LED on checksum or RAM failure:

```
; --- LED flash loop (runs after setting error colour) ---
$F803CE  moveq    #$ff, d0                      ; Inner delay counter
$F803D0  bset.b   #$1, $bfe001.l                ; LED off (active low)
$F803D8  dbra     d0, $f803d0                    ; Short delay
$F803DC  lsr.w    #$2, d0                       ; Shorter on-delay
$F803DE  bclr.b   #$1, $bfe001.l                ; LED on
$F803E6  dbra     d0, $f803de                    ; Short delay
$F803EA  dbra     d1, $f803d0                    ; Repeat 11 times ($A+1)

; --- Longer pause with black screen ---
$F803EE  move.l   #$15000, d0                   ; Approx 0.5 second delay
$F803F4  move.w   #$0, $dff180.l                ; COLOR00 = black
$F803FC  subq.l   #$1, d0
$F803FE  bgt.b    $f803f4                        ; Busy-wait

; --- Then jump to ColdReboot supervisor code to try again ---
$F80400  move.w   #$4000, $dff09a.l             ; Disable interrupts
$F80408  bra.w    $f80cc8                        ; Jump to supervisor-mode reboot sequence
```

The error handler flashes the LED rapidly 11 times, pauses with a black screen
for about half a second, disables interrupts, and attempts a reboot. If the ROM
is permanently damaged, this creates an infinite flash-pause-reboot cycle.

## Appendix E: Source Cross-References

### NDK 3.9 Headers
- ExecBase structure: `NDK_3.9/Include/include_h/exec/execbase.h`, `include_i/exec/execbase.i`
- Resident structure: `NDK_3.9/Include/include_h/exec/resident.h`
- Task structure: `NDK_3.9/Include/include_h/exec/tasks.h`
- Library structure: `NDK_3.9/Include/include_h/exec/libraries.h`
- Node/List structures: `NDK_3.9/Include/include_h/exec/nodes.h`, `lists.h`
- Exec LVO list: `NDK_3.9/Include/fd/exec_lib.fd`

### vAmiga Emulator Source
- ExecBase offsets (definitive, including undocumented V36 pool fields):
  `vAmiga/Core/Misc/OSDebugger/OSDebuggerTypes.h` (line 367)
- ExecBase reader with all offsets hard-coded:
  `vAmiga/Core/Misc/OSDebugger/OSDebuggerRead.cpp` (line 97)
- OVL bit handling (CIA-A PRA bit 0 triggers memory map update):
  `vAmiga/Core/Components/CIA/CIA.cpp` (line 852)
- Memory overlay mapping (ROM mirrored at $0 when OVL=1):
  `vAmiga/Core/Components/Memory/Memory.cpp` (updateCpuMemSrcTable, line 926)
- 68000 RESET instruction handling (triggers softReset on all external hardware):
  `vAmiga/Core/Components/CPU/CPU.cpp` (line 161)
- INTENA register handling (SET/CLR bit 15 logic):
  `vAmiga/Core/Components/Paula/PaulaRegs.cpp` (setINTENA, line 109)
