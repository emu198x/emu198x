# Kickstart 2.02 (36.207) — A3000

## ROM Identification

| Property | Value |
|----------|-------|
| File | `kick202_36_207_a3000.rom` |
| Size | 512K (524,288 bytes) |
| Version | exec 36.142 (1.10.90) |
| Target | A3000 |
| SSP | $11144EF9 |
| PC | $F800D2 |
| Mapped at | $F80000–$FFFFFF |

The shipping A3000 ROM. Substantially updated from the 2.0 beta — the builds
differ by ~32K lines of hex dump. Same boot architecture but with bug fixes,
updated module versions, and refined A3000 hardware handling.

## Boot Flow

### Entry Point ($F800D2)

Identical structure to KS 2.0 beta:

1. **ROM checksum** ($F800D2–$F800F4) — ones' complement sum of 512K
2. **Diagnostic ROM check** at $F00000
3. **A3000 RAMSEY/MMU init** ($F80150–$F801E8) — same sequence as 2.0 beta
   but with an added CACR probe:

```asm
$F80150  MOVEQ   #9,D0                 ; CACR: enable+freeze instruction cache
$F80152  MOVEC   D0,CACR               ; Write CACR
$F80156  MOVEC   CACR,D0               ; Read back
$F8015A  TST.L   D0                    ; Zero = 68000 (no cache)
$F8015C  BEQ.W   $F801F2               ; Skip 030 init if 68000
; Continue with RAMSEY/MMU init...
$F80160  MOVEQ   #0,D0
$F80162  MOVE.B  D0,$DE0000            ; Clear RAMSEY config byte 0
$F80168  MOVE.B  D0,$DE0001            ; Clear RAMSEY config byte 1
$F8016E  BCLR    #7,$DE0002            ; Test RAMSEY control bit 7
$F80176  BEQ.S   $F801A6              ; Not A3000 → skip MMU
```

**Key difference from 2.0 beta:** The CACR probe at $F80150 gates the entire
68030 init block. On a 68000, MOVEC to CACR causes an F-line exception, but
since the vector table hasn't been set up yet, the ROM detects a 68000 by
writing CACR and reading it back — a 68000 returns 0 (the write is a no-op on
the bus), while a 68030 returns the value written.

### Stages 4–12

Follow [boot-flow-overview.md](boot-flow-overview.md) with A3000-specific
additions identical to KS 2.0 beta.

## Resident Modules

39 modules. Same set as KS 2.0 beta with updated versions:

| Pri | Name | Type | Version | Init | Notes |
|-----|------|------|---------|------|-------|
| +110 | expansion.library | LIBRARY | 36 | $F83E70 | v36.89 |
| +105 | exec.library | LIBRARY | 36 | $F804E8 | v36.142 |
| +105 | diag init | — | 36 | $F84104 | Renamed from "diag part 1" |
| +103 | utility.library | LIBRARY | 36 | $FBB38C | v36.40 |
| +100 | potgo.resource | RESOURCE | 36 | $FABD78 | |
| +80 | cia.resource | RESOURCE | 36 | $F88BF4 | v36.31 |
| +80 | FileSystem.resource | RESOURCE | 36 | $F982E4 | |
| +70 | disk.resource | RESOURCE | 36 | $F8ED46 | |
| +70 | misc.resource | RESOURCE | 36 | $FABCC8 | |
| +65 | graphics.library | LIBRARY | 36 | $FBBE70 | v36.184 |
| +60 | gameport.device | DEVICE | 36 | $FAEF9A | |
| +50 | timer.device | DEVICE | 36 | $FB8B1C | v36.50 |
| +45 | battclock.resource | DEVICE | 36 | $F859E0 | v36.22 |
| +45 | keyboard.device | DEVICE | 36 | $FAEFC4 | |
| +44 | battmem.resource | DEVICE | 36 | $F863F6 | v36.14 |
| +40 | keymap.library | LIBRARY | 36 | $FA6C1E | |
| +40 | input.device | DEVICE | 36 | $FAEFEE | |
| +31 | layers.library | LIBRARY | 36 | $FA7870 | v36.97 |
| +25 | ramdrive.device | DEVICE | 36 | $FAE446 | |
| +20 | trackdisk.device | DEVICE | 36 | $FB9A9E | v36.16 |
| +10 | scsi.device | DEVICE | 36 | $FB0696 | v36.42 |
| +10 | intuition.library | LIBRARY | 36 | $FD3D5E | v36.3934 |
| +5 | alert.hook | — | 36 | $F83BBE | |
| +5 | console.device | DEVICE | 36 | $F8AF36 | v36.523 |
| 0 | mathieeesingbas.library | LIBRARY | 36 | $FAB970 | |
| −35 | syscheck | — | 36 | $F87D24 | v36.182 |
| −40 | romboot | — | 36 | $FB7C08 | |
| −50 | bootmenu | — | 36 | $F865EC | v36.367 |
| −60 | strap | — | 36 | $FB7E28 | |
| −81 | filesystem | — | 36 | $F983BC | v36.289 |
| −100 | ramlib | — | 36 | $FAEB18 | v36.129 |
| −120 | audio.device | DEVICE | 36 | $F84904 | |
| −120 | dos.library | LIBRARY | 36 | $F8F0A2 | v36.128 |
| −120 | gadtools.library | LIBRARY | 36 | $FA3D50 | v36.588 |
| −120 | icon.library | LIBRARY | 36 | $FA3F90 | v36.343 |
| −120 | mathffp.library | LIBRARY | 36 | $FAB470 | |
| −120 | workbench.task | TASK | 36 | $FBBD0C | |
| −120 | workbench.library | LIBRARY | 36 | $FEC7EA | v36.2154 |
| −121 | con-handler | — | 36 | $F890A6 | v36.54 |
| −122 | shell | — | 36 | $FB3314 | v36.92 |
| −123 | ram-handler | — | 36 | $FABE68 | |

### Changes from KS 2.0 beta

- "diag part 1" renamed to "diag init" and moved to priority +105 (from +102)
- console.device priority changed to +5 (from +20)
- icon.library added (not present in 2.0 beta)
- Many modules have higher revision numbers (e.g. graphics 36.184 vs 36.129,
  intuition 36.3934 vs 36.3435)

## Emulator Implications

Same as KS 2.0 beta, plus:

- **CACR probe** at $F80150 must work correctly. Writing 9 to CACR and reading
  back must return 9 on 68030 (or the value masked by valid CACR bits). Returning
  0 causes the ROM to skip all 68030-specific init.
- **RAMSEY at $DE0000–$DE0002** — same requirements as 2.0 beta.
- This is the ROM that triggered the **RTE format word bug** in Emu198x — the
  68030 was popping only 6 bytes (SR+PC) on RTE instead of 8+ (SR+PC+format word).
  Without the format word, SSP misaligned by 2 bytes, causing exec's Supervisor()
  to pop a garbage return address. See the memory notes in MEMORY.md.
