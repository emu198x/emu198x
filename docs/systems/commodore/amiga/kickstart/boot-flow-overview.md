# Kickstart Boot Flow Overview

Every Kickstart ROM follows the same high-level sequence from power-on to the
insert-disk screen. This document defines the common stages, explains the
mechanisms shared across all versions, and notes where emulator accuracy matters
at each step.

Per-version documents reference these stage numbers and add version-specific
detail — addresses, register values, and divergent code paths.

## Boot Stages

### Stage 1: Reset Vector Fetch

**What happens:** The 68000 reads the initial SSP from $000000 and the initial
PC from $000004. The overlay latch maps ROM over chip RAM at $000000 so these
reads hit the first 8 bytes of the ROM image.

**Hardware required:**
- Overlay latch active on reset (CIA-A OVL pin directly controls address decode)
- ROM mapped at $FC0000 (256K) or $F80000 (512K)
- 68000 enters supervisor mode with interrupts masked (SR=$2700)

**Emulator implications:**
- The overlay must be active at power-on. If ROM isn't visible at $000000, the
  CPU reads zeros and jumps to $000000 — immediate crash.
- The SSP value ($11114EF9 or $11144EF9) points into unmapped space. The ROM
  immediately overwrites it. No stack access must occur before the `LEA xxx,SP`
  instruction at the entry point.

### Stage 2: Warm/Cold Start Detection

**What happens:** The ROM checks whether ExecBase at $000004 is valid. If valid,
it's a warm start — the ROM attempts to preserve system state. If not, it's a
cold start — full hardware reset.

**Warm start validation (KS 1.2+):**
1. Read longword at $000004 (ExecBase pointer)
2. Check bit 0 is clear (must be word-aligned)
3. Verify `ExecBase + ExecBase->ChkBase` = $FFFFFFFF (complement checksum)
4. Sum words from ExecBase+$22 to ExecBase+$52 — result must complement to 0
5. If ExecBase->ColdCapture is non-zero, clear it and jump through it
6. Validate ExecBase->KickMemPtr, ExecBase->KickTagPtr, ExecBase->KickCheckSum

**Cold start path:**
If any validation fails, the ROM proceeds to Stage 3 (memory detection).

**Emulator implications:**
- On first boot, chip RAM is zeros. $000004 = 0 fails validation at step 2
  (bit 0 clear, but complement check fails). Cold start always taken.
- After a soft reset (Ctrl-A-A), ExecBase should be valid if the emulator
  preserved chip RAM contents.

### Stage 3: Hardware Reset and Memory Detection

**What happens:** The ROM resets all custom chip registers, sizes chip RAM,
detects slow RAM ($C00000) and fast RAM, then selects a memory region for
ExecBase and the initial supervisor stack.

**Sequence:**
1. **Delay loop** — busy-wait ~130K iterations. Allows hardware to stabilise
   after power-on (Gary/Gayle/RAMSEY need time).
2. **Diagnostic ROM check** — read $F00000 for magic word $1111. If found, jump
   to the diagnostic ROM. This supports A3000 DMAC diagnostic ROMs and A1000
   WCS (Writable Control Store).
3. **CIA-A setup** — set OVL output (DDRA bit 0 = output, PRA bit 1 = LED off).
   This clears the overlay, making chip RAM visible at $000000.
4. **Custom chip reset:**
   - INTENA = $7FFF (disable all interrupts)
   - INTREQ = $7FFF (clear all interrupt requests)
   - DMACON = $7FFF (disable all DMA)
   - BPLCON0 = $0200 (blank display, colour mode)
   - BPLCON1 = $0000 (no scroll offset)
   - COLOR00 = $0444 (dark grey background)
5. **Exception vectors** — fill vectors $008–$0BC with a pointer to the exec
   alert handler.
6. **Branch to memory detection** (Stage 3 continued via exec init subroutine).

**Memory detection:**
- Slow RAM: probe $C00000–$DC0000 by writing patterns and reading back.
  Result in A4 (0 = none, or base address).
- Chip RAM: probe $000000–$200000. Result in A3 (size: $40000–$200000).
- If slow RAM found, ExecBase and SSP go there. Otherwise they go at the top
  of chip RAM.

**Emulator implications:**
- The overlay clear (CIA-A write) must immediately remap address decode. If
  chip RAM doesn't appear at $000000 after the overlay clear, memory detection
  writes go to ROM (ignored) and reads come from ROM (wrong pattern).
- Slow RAM at $C00000 must respond to read/write. KS 1.2+ A500/A2000 tests
  depend on 512KB slow RAM for ExecBase placement.
- Custom chip register writes must take effect immediately. DMACON=$7FFF must
  disable all DMA channels within one bus cycle.

### Stage 4: ExecBase Initialisation

**What happens:** Exec builds its base structure in memory. This includes the
library jump table, internal lists (libraries, devices, resources, memory,
interrupts), and task scheduling structures.

**Key operations:**
1. Store ExecBase pointer at $000004
2. Write complement checksum at ExecBase+$26
3. Set SSP to top of the ExecBase memory region
4. Clear ExecBase fields ($54–$250 approx.)
5. Initialise lists: MemList, ResourceList, DeviceList, LibList, IntrList, etc.
6. Store exec.library identification (name pointer, version, type=NT_LIBRARY)
7. Set initial SoftVer, KickMemPtr, KickTagPtr fields
8. Call exec internal init (set up CPU trap vectors, scheduling primitives)
9. Detect CPU type (68000/010/020/030/040/060) and FPU
10. Enable master DMA (DMACON SET bit + BLTPRI + DMAEN)

**Emulator implications:**
- The CPU type detection reads VBR (MOVEC on 68010+) and tests for address
  error on word access to odd address. The emulator must correctly raise
  address errors on 68000 and implement MOVEC on 68010+.
- ExecBase+$12E (AttnFlags) records detected CPU/FPU. Wrong detection causes
  later code to use wrong instruction forms.

### Stage 5: Resident Module Scan

**What happens:** Exec scans the ROM for RomTag structures ($4AFC match word)
and builds a sorted list by priority.

**RomTag structure:**
```
Offset  Size  Field
$00     WORD  rt_MatchWord    ($4AFC)
$02     LONG  rt_MatchTag     (pointer back to this RomTag — validation)
$06     LONG  rt_EndSkip      (address past end of this module)
$0A     BYTE  rt_Flags        (bit 0 = RTF_COLDSTART, bit 1 = RTF_AUTOINIT)
$0B     BYTE  rt_Version
$0C     BYTE  rt_Type         (NT_LIBRARY, NT_DEVICE, NT_RESOURCE, ...)
$0D     BYTE  rt_Pri          (init priority, signed)
$0E     LONG  rt_Name         (pointer to name string)
$12     LONG  rt_IdString     (pointer to version string)
$16     LONG  rt_Init         (init entry point or auto-init table)
```

**Scan algorithm:**
1. Start at ROM base ($FC0000 or $F80000)
2. Search for word $4AFC at even addresses
3. When found, verify rt_MatchTag points back to itself
4. If valid, add to resident list
5. Skip to rt_EndSkip and continue
6. Sort by rt_Pri (highest first)

**Module initialisation order (typical KS 1.3):**
| Priority | Module | Type |
|----------|--------|------|
| 126 | exec.library | Library |
| 120 | expansion.library | Library |
| 105 | exec.library (second init) | — |
| 100 | graphics.library | Library |
| 70 | layers.library | Library |
| 50 | intuition.library | Library |
| 40 | timer.device | Device |
| 20 | cia.resource (×2) | Resource |
| 10 | potgo.resource | Resource |
| 5 | keyboard.device | Device |
| 4 | input.device | Device |
| 0 | trackdisk.device, console.device, dos.library, etc. | Various |
| −50 | ramlib | — |
| −60 | strap | — |
| −120 | shell/CLI | — |

**Emulator implications:**
- The scan hits every even word in the ROM. Any memory mapping errors cause
  missed modules.
- Each module's init function can probe arbitrary hardware. If the emulator
  doesn't implement the hardware, the init may hang or crash.

### Stage 6: Early Module Init (Priority 100+)

**What happens:** Modules with priority > 100 initialise first. These are the
core system services needed before anything else.

**exec.library (pri 126):**
- Already partially initialised in Stage 4
- RomTag init completes library setup

**expansion.library (pri 120):**
- Scans Zorro II ($E80000) and Zorro III ($FF000000) autoconfig space
- Probes each slot for manufacturer/product ID
- Maps expansion boards into address space
- Without expansion hardware, this completes quickly (no boards found)

**Emulator implications:**
- expansion.library reads from $E80000. Unmapped reads must return $FF (no
  board present), not bus error.
- If the emulator implements expansion boards, they must respond to the
  autoconfig protocol here.

### Stage 7: Graphics and Display Init (Priority 50–100)

**What happens:** The display subsystem starts up.

**graphics.library (pri 100):**
- Detect chipset type (OCS/ECS/AGA) by reading DENISEID ($DFF07C)
- Detect PAL/NTSC from VPOSR ($DFF004) bit 12
- Set up copper lists for the default display
- Initialise the blitter
- Build GfxBase with display timing parameters
- **EClock calibration:** Uses CIA timer to measure one video frame. The result
  at GfxBase+$22 is used as a divisor — if it's zero, STRAP crashes with a
  DIVU #0 exception.

**layers.library (pri 70):**
- Layer management for windows and clipping

**intuition.library (pri 50):**
- Window/screen manager
- Creates the default display (used by the STRAP screen)

**Emulator implications:**
- DENISEID ($DFF07C) must return the correct chipset identifier. OCS Denise
  returns $FF (no ID register). ECS returns $FC. AGA Lisa returns $F8.
- VPOSR bit 12 (PAL/NTSC) must match the configured region.
- CIA timer ticks must advance at the correct rate relative to VBLANK. Wrong
  CIA timing causes incorrect EClock calibration, leading to either a DIVU #0
  crash or wildly wrong timing.

### Stage 8: Device and Resource Init (Priority 0–49)

**What happens:** Hardware drivers and system resources initialise.

**timer.device (pri 40):**
- Calibrates timing using CIA timers
- Provides system time services

**cia.resource (pri 20, two instances):**
- CIA-A resource (keyboard, parallel port, game port, overlay, LED)
- CIA-B resource (serial, disk, motor, direction select)

**potgo.resource (pri 10):**
- Game port proportional controller support

**keyboard.device (pri 5):**
- Initialises keyboard communication via CIA-A SP (serial port)
- Sends reset command, waits for keyboard response

**input.device (pri 4):**
- Input event manager

**trackdisk.device (pri 0):**
- Floppy disk driver
- Configures DMA channel, sets motor signals via CIA-B
- Reads disk ready/change/protect signals
- Turns motor on and starts seeking to track 0

**console.device, dos.library, etc. (pri 0 or below):**
- These complete the system setup

**Emulator implications:**
- keyboard.device requires CIA-A serial port (CIAA SP at $BFEC01) to function.
  The keyboard controller must complete the power-up handshake (send $FD init
  + $FE term) or keyboard.device times out.
- trackdisk.device probes the disk hardware. CIA-B timer B, DSKLEN, and disk
  DMA must all work correctly.

### Stage 9: Memory Configuration Validation

**What happens:** Exec finalises the memory list. It may add expansion RAM
detected by expansion.library. The total available memory determines whether
the system can proceed.

**Failure mode:** If exec can't allocate enough memory for the required library
and device bases, it triggers an Alert (guru meditation). Alert code $01000005
(AG_NoMemory | AO_ExecLib) flashes as a red screen with hex overlay.

**Emulator implications:**
- A500 with 512K chip RAM alone can boot KS 1.0 and 1.1. KS 1.2+ needs the
  slow RAM at $C00000 for the A500 to avoid running out of memory during init
  (ExecBase is placed in slow RAM, freeing chip RAM for allocations).

### Stage 10: STRAP Display (Insert-Disk Screen)

**What happens:** The System Test and Registration Program (STRAP) displays the
insert-disk screen. This is the final module to initialise (priority −60).

**Display setup (typical KS 1.3 OCS):**
1. Build a copper list with:
   - BPLCON0 = $2302 (2 bitplanes, lowres, colour)
   - DIWSTRT/DIWSTOP for standard PAL window
   - BPL1PT/BPL2PT pointing to bitmap data
   - COLOR00–COLOR03 for the palette
2. Set COP1LC to the copper list address
3. Write COPJMP1 to restart the copper
4. Enable DMA: DMACON = $83C0 (set BLTPRI + DMAEN + BPLEN + COPEN + BLTEN)
5. Draw the checkmark icon and "Insert disk" text using the blitter

**KS 2.04+ (hires):**
- BPLCON0 = $8302 (3 bitplanes, hires, colour)
- More detailed graphics (rainbow gradient, ROM version text)

**Expected register state after STRAP completes:**

| Version | DMACON (set bits) | BPLCON0 | Notes |
|---------|-------------------|---------|-------|
| KS 1.0 | varies | varies | Pre-STRAP; hand-drawn display |
| KS 1.2 | $0180+ | $2302 | 2-plane lowres |
| KS 1.3 | $0180+ | $2302 | 2-plane lowres |
| KS 2.04 | $0180+ | $8302 | 3-plane hires |
| KS 2.05 | $0180+ | $8302 | 3-plane hires |
| KS 3.1 (OCS, A2000) | $0180+ | $8302 | 3-plane hires |
| KS 3.1 (ECS, A500/A600) | $0180+ | $8303 | 3-plane hires + ERSY |
| KS 3.0/3.1 (AGA, A1200) | $03C0+ | $8303 | 3-plane hires + ERSY + sprite DMA |

**Emulator implications:**
- The copper must be running and correctly parsing the copper list.
- Bitplane DMA must fetch data from the addresses in BPL1PT–BPL6PT.
- The blitter must complete operations correctly (STRAP draws the icon using
  BLTCON0/BLTCON1 fill mode and line mode).
- Boot tests assert DMACON and BPLCON0 values (see `boot_ocs.rs`, etc.).

### Stage 11: Disk Boot Wait

**What happens:** STRAP enters a loop waiting for a bootable disk in DF0:.

**Disk detection:**
1. Monitor CIA-B disk change signal (CIAB PRB bit 2)
2. When disk inserted, trackdisk.device reads track 0
3. MFM decode → extract sectors 0–1 (boot block, 1024 bytes)
4. Verify boot block checksum (longword sum of 256 longwords = 0)
5. If valid, JMP to bootblock+$0C (boot code entry point)
6. If invalid or no disk, wait and retry

**Emulator implications:**
- Full MFM DMA pipeline required: motor spin-up → DSKLEN triggers DMA →
  raw MFM data in DMA buffer → sector decode → checksum validation
- CIA-B PRB must reflect disk change state

### Stage 12: Boot Block Execution

**What happens:** The bootblock code runs. Typically it loads the first file
from the disk (e.g. the startup-sequence for DOS disks, or a game's loader).

**This is outside the scope of this reference** — the ROM's role in boot ends
when the bootblock takes control.

## Overlay Mechanism

The overlay latch solves a bootstrap problem: the 68000 fetches its reset
vectors from $000000, but that's where chip RAM lives. The overlay makes ROM
visible at $000000 on power-up.

**How it works:**
1. On reset, the overlay latch is set (active)
2. Address decode maps ROM at both $FC0000 (or $F80000) AND $000000
3. CPU reads SSP from $000000, PC from $000004 — both come from ROM
4. Early ROM code writes CIA-A PRA to clear the OVL bit
5. Overlay deactivates — chip RAM reappears at $000000

**CIA-A connections:**
- DDRA ($BFE201) bit 0 = direction (set to output)
- PRA ($BFE001) bit 0 = OVL (0 = overlay off, chip RAM visible)
- PRA ($BFE001) bit 1 = LED (active low — 1 = LED off)

**The ROM code:**
```
MOVE.B #$03, $BFE201   ; DDRA: bits 0,1 as outputs (OVL + LED)
MOVE.B #$02, $BFE001   ; PRA: OVL=0 (clear overlay), LED=1 (off)
```

## Memory Detection Patterns

### Chip RAM Sizing

The ROM writes a test pattern to addresses at power-of-2 boundaries:
$000000, $040000, $080000, $100000. It reads back to determine which addresses
alias (wrap around) and which are distinct. The highest non-aliasing address
gives the chip RAM size.

**Typical results:**
| Chip RAM | A3 value |
|----------|----------|
| 256K | $040000 |
| 512K | $080000 |
| 1M | $100000 |
| 2M | $200000 |

### Slow RAM Detection

Write $0000 then $FFFF to $C00000, read back. If the value sticks, slow RAM is
present. Repeat at $C80000, $D00000, $D80000 to determine size.

### Fast RAM Detection (KS 2.0+)

KS 2.0+ also probe for 32-bit RAM at higher addresses. The A3000 RAMSEY
controller maps fast RAM at $07000000+. The probe writes patterns and tests
for bus errors (no response = no RAM).

## CPU Detection

Exec detects the CPU model to set AttnFlags (ExecBase+$12E):

| Bit | Flag | Detection method |
|-----|------|-----------------|
| 0 | AFF_68010 | MOVEC VBR,D0 — no F-line on 68010+ |
| 1 | AFF_68020 | CALLM — different exception frame on 68020+ |
| 2 | AFF_68030 | PMOVE — responds on 68030, F-line on 68020 |
| 3 | AFF_68040 | Test for 68040 CINV instruction |
| 4 | AFF_68881 | FNOP — no F-line if FPU present |
| 5 | AFF_68882 | Distinguishes 68881 from 68882 |
| 7 | AFF_68060 | Test for 68060 PLPA instruction (KS 3.1 only) |

**Emulator implications:**
- The detection relies on exception handling. If the CPU raises the wrong
  exception type (or no exception) for an unimplemented instruction, exec
  records the wrong CPU model.
- 68000 should raise F-line ($2C) for MOVEC. 68010+ should execute it.
- The detection code must run in supervisor mode (it already is after reset).

## Warm Start vs Cold Start

On a soft reset (RESET instruction or Ctrl-A-A), the ROM attempts to preserve
the running system:

1. **Validate ExecBase** — checksum verification (see Stage 2)
2. **ColdCapture** — if ExecBase->ColdCapture ($2A) is non-zero, jump through
   it (one-shot — the ROM clears it first to prevent loops)
3. **KickMem/KickTag** — validate and restore resident modules that were loaded
   into RAM (for ROM-replacement setups)
4. **Partial reinit** — skip memory detection, reuse existing memory lists
5. **Module reinit** — re-run resident module init with RTF_COLDSTART flag

If the checksum fails at any point, the ROM falls through to a full cold start.

## Emulator Boot Test Assertions

The boot tests in `crates/machine-amiga/tests/` verify specific register states
after running the ROM for ~30 seconds of emulated time:

| Test | DMACON (must have) | BPLCON0 (exact) |
|------|-------------------|-----------------|
| KS 1.2+ OCS (A500/A2000) | $0180 (BPLEN+COPEN) | $2302 |
| KS 2.04 ECS (A500+) | $0180 | $8302 |
| KS 2.05 ECS (A600) | $0180 | $8302 |
| KS 3.1 OCS (A2000) | $0180 | $8302 |
| KS 3.1 ECS (A500/A600) | $0180 | $8303 |
| KS 3.0 AGA (A1200) | $03C0 (BPLEN+COPEN+BLTEN+SPREN) | $8303 |
| KS 3.1 AGA (A1200) | $03C0 | $8303 |

See `boot_ocs.rs`, `boot_ecs.rs`, `boot_aga.rs` for the full test list.
