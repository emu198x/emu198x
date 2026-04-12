# The Amiga Boot Process — A Reference for Emulator Authors

*Synthesised from the ten Amiga reference PDFs in `/Users/stevehill/Desktop/AmigaPDFs/txt/`.*

## How to read this document

This is a single consolidated reference covering what the Amiga hardware and operating system do between power-on and a running Workbench. It is written for someone implementing a hardware-accurate emulator who has read some, but not all, of the classic Amiga reference manuals and wants one document where the boot sequence is assembled from multiple sources and cross-referenced.

The structure is:

1. A top-level table of contents.
2. A **Phase-by-phase timeline** — what happens in order from reset to Workbench, with addresses, registers, and concrete quoted code where the sources give it.
3. A **Subsystems** section that steps sideways to explain how interrupts, libraries, devices, tasks, memory, CIA, and the custom chips hang together.
4. An **Emulator implementation notes** section summarising the load-bearing details for people writing a 68000/Agnus/Denise/Paula/CIA emulator.
5. A **Gaps in the corpus** section.
6. A **Source map** appendix.

Sources are cited inline in parentheses, e.g. `(HRM §Reset and Early Startup)`, `(Exec RKM Appendix C)`, `(SPG §2.9.1)`, `(A500/A2000 TRM §Auto Configuration)`, `(Mapping §BFE001 PRA)`. Full PDF names are in the Source Map at the end.

Where the sources disagree, or where one source is vague and another is precise, that is called out. Where something is not covered by any of these ten PDFs, it is flagged as **"not covered in corpus"** rather than filled in from general knowledge.

---

## Table of contents

- [Phase 0 — Physical reset: the machine before any code runs](#phase-0--physical-reset-the-machine-before-any-code-runs)
- [Phase 1 — Overlay, the reset vector, and the first CPU fetch](#phase-1--overlay-the-reset-vector-and-the-first-cpu-fetch)
- [Phase 2 — ROM header and Kickstart entry](#phase-2--rom-header-and-kickstart-entry)
- [Phase 3 — Very early Kickstart: silence the custom chips, load the vector table](#phase-3--very-early-kickstart-silence-the-custom-chips-load-the-vector-table)
- [Phase 4 — ExecBase validation and the ColdCapture trap](#phase-4--execbase-validation-and-the-coldcapture-trap)
- [Phase 5 — Memory sizing: chip RAM, fast RAM, MemList](#phase-5--memory-sizing-chip-ram-fast-ram-memlist)
- [Phase 6 — Rebuilding ExecBase, SysBase at $4, checksum](#phase-6--rebuilding-execbase-sysbase-at-4-checksum)
- [Phase 7 — CPU detection and AttnFlags](#phase-7--cpu-detection-and-attnflags)
- [Phase 8 — System lists, exec.library as a library, the initial task](#phase-8--system-lists-execlibrary-as-a-library-the-initial-task)
- [Phase 9 — ROMTag (Resident) scan and the module table](#phase-9--romtag-resident-scan-and-the-module-table)
- [Phase 10 — CoolCapture; InitCode(COLDSTART)](#phase-10--coolcapture-initcodecoldstart)
- [Phase 11 — Autoconfig: expansion.library, Zorro II/III at $E80000](#phase-11--autoconfig-expansionlibrary-zorro-iiiii-at-e80000)
- [Phase 12 — DiagArea, ROM drivers on expansion boards, DAC_CONFIGTIME](#phase-12--diagarea-rom-drivers-on-expansion-boards-dac_configtime)
- [Phase 13 — ROMTag INIT time for expansion board drivers](#phase-13--romtag-init-time-for-expansion-board-drivers)
- [Phase 14 — Other resident libraries and devices come up](#phase-14--other-resident-libraries-and-devices-come-up)
- [Phase 15 — Strap, the "bootme" hand, and floppy/autoboot selection](#phase-15--strap-the-bootme-hand-and-floppyautoboot-selection)
- [Phase 16 — The floppy bootblock](#phase-16--the-floppy-bootblock)
- [Phase 17 — dos.library, filesystem.resource, DOS bring-up](#phase-17--doslibrary-filesystemresource-dos-bring-up)
- [Phase 18 — The CLI, startup-sequence, LoadWB, Workbench](#phase-18--the-cli-startup-sequence-loadwb-workbench)
- [Cross-cutting subsystems](#cross-cutting-subsystems)
- [Emulator implementation notes](#emulator-implementation-notes)
- [Gaps in the corpus](#gaps-in-the-corpus)
- [Source map appendix](#source-map-appendix)

---

## Phase 0 — Physical reset: the machine before any code runs

### What drives reset

Reset on the A500/A2000 is a bidirectional open-collector line `/RST`, pin 53 on the expansion bus and pin 94 on the coprocessor bus as `/BUSRST` (A500/A2000 TRM §Expansion Bus Pinout). It is asserted by:

- Power-on (derived from the power supply).
- The keyboard `Ctrl-Amiga-Amiga` combination, which the A500/A2000 keyboard controller translates into a reset request (A500/A2000 TRM §Keyboard).
- A 68000 `RESET` instruction, which pulses `/DRESB` for roughly the duration specified by the 68000 spec (HRM §Reset and Early Startup Operation; A500/A2000 TRM §Reset).
- Expansion-board hardware that is allowed to drive the unbuffered `RES*` line (A500/A2000 TRM §RES* and RESB*).

Two reset lines actually appear on the Zorro bus (HRM/A500/A2000 TRM):

- `RES*` (pin 53) — **unbuffered**, can both drive and be driven. Only boards designed to reset the system use this as an output.
- `RESB*` (pin 94) — **buffered**, intended as the normal input for PICs (Plug-In Cards).

### Hardware state at the exact moment of reset

- The 68000 enters its reset exception sequence. It will fetch the initial Supervisor Stack Pointer from `$000000` and the initial Program Counter from `$000004`. This is standard 68000 behaviour; the Amiga docs implicitly rely on it (HRM §Reset and Early Startup Operation; Abacus Machine Language §2.1).
- **All** autoconfig PICs are dropped to the unconfigured state; any memory or I/O they were mapping disappears. An unconfigured PIC responds only at `$E80000` and only when its `/CFGIN` is asserted (A500/A2000 TRM §Auto Configuration; HRM Appendix F area).
- The 8520 CIAs reset. On `RES`, the toggle outputs are driven low, the TOD clock stops until written, timers stop, PRA/PRB become inputs (DDRs zero), and the PC/FLAG handshake lines reset (HRM §Appendix F, sections on I/O Ports, Timers, TOD and SDR).
- Agnus/Denise/Paula reset state is mostly implicit in the HRM: `DMACON` is cleared so all DMA is off, `INTENA` master-enable is cleared so no interrupts fire, bitplane pointers and copper pointers are in an unspecified-but-quiet state. The Kickstart reset routine itself immediately explicitly writes `$7FFF` to `INTENA`, `INTREQ` and `DMACON` (SPG §2.9.1) — it does not rely on hardware reset to do this, which is the clue for emulator authors: **don't assume any particular custom-chip state from hardware reset alone; the software clears them explicitly.**
- Some registers are explicitly spec'd to be reset on power up: `ERSY` (External sync) in `BPLCON0` bit 1, `LACE` bit 2, and `LPEN` bit 3 are all "reset on power up" (Mapping, `$DFF100 BPLCON0`).

### The `68000 RESET` instruction specifically

The 68000 `RESET` instruction is **not** a CPU reset. It pulses the external reset line, and on the Amiga that re-runs the whole autoconfig / overlay / ROM-remapping sequence:

> "The 68000 RESET instruction works much like external reset or power on. All memory and AUTOCONFIG™ cards disappear, and the ROM image appears at location `$00000000`. The difference is that the CPU continues execution with the next instruction. Since RAM may not be available, special care is needed to write reboot code that will reliably reboot all Amiga models." (HRM §Reset and Early Startup Operation)

That quoted line is load-bearing for `ColdReboot()` (see Phase 6 / Emulator notes).

---

## Phase 1 — Overlay, the reset vector, and the first CPU fetch

### The problem

The 68000 needs a valid SSP at `$0` and a valid initial PC at `$4` on its first fetch after reset. But:

- RAM contents are undefined at power-on.
- Kickstart ROM lives at `$F80000–$FFFFFF` (256K ROM) or `$F00000–$FFFFFF` (512K ROM, 2.x/3.x), depending on Kickstart version.

The Amiga's answer is the **overlay bit**, often called `OVL`.

### The OVL line

`OVL` is wired to bit 0 of CIA-A's Peripheral Register A (`$BFE001 PRA`):

```
BFE001 PRA    /FIR1  /FIR0  /RDY  /TK0  /WPRO  /CHNG  /LED  OVL
```
(HRM Appendix F; Mapping §`$BFE001 PRA`; SPG §1.5.1.1)

After reset:

- CIA-A DDRA is preset such that bit 0 (OVL), bit 1 (/LED), and on some machines bit 2 are outputs. Specifically:

  ```
  BFE201 ddra   Direction for port A (BFE001); 1=output (set to 0x03)
  ```
  (HRM Appendix F memory map)

  i.e. OVL and /LED are always outputs, the rest are inputs.

- On hardware reset, the CIA peripheral output registers come up with OVL **high** (the exact mechanism is that hardware reset forces OVL=1 until software clears it). The SPG describes it as: *"After a reset, the port line automatically goes high, causing the ROM area at `$F80000` to `$FFFFFF` to be mapped into the range from 0 to `$7FFFF`."* (SPG §1.5.1.1).

### What OVL=1 does to the address decode

With OVL=1, the address decode logic on the motherboard answers a ROM access when the CPU asks for the low 512 KB. The A500/A2000 TRM gives the PAL equations (§Memory Decoder PAL):

```
/RE  = DBR*/AS*DTACK*/A23*/A22*/A21*OVR*OVL*...    ; $000000-1FFFFF OVL=H
...
/ROME = /AS*A23*A22*A21*A20*A19*OVR*PRU             ; $F80000-FFFFFF
```
(A500/A2000 TRM §Memory Decoder PAL equations)

In prose: when OVL is high and the CPU issues a low read, `/RE` drives the ROM chip-select so the fetches at `$000000–$07FFFF` are answered by the Kickstart ROM image that normally lives at `$F80000–$FFFFFF`. Chip RAM, which normally lives there, is not accessible during this window. The HRM says this explicitly:

> "When the Amiga is turned on or externally reset, the memory map is in a special state. An additional copy of the system ROM responds starting at memory location `$00000000`. The system RAM that would normally be located at this address is not available. On some Amiga models, portions of the RAM still respond. On other models, no RAM responds. Software must assume that memory is not available." (HRM §Reset and Early Startup Operation)

That "portions of the RAM still respond" comment is important: the overlay is not perfectly clean on all models, and bootstrap code is written to not depend on RAM until after the explicit overlay teardown.

### The first fetch

With OVL=1 and SSP/PC at `$0/$4` being served out of the ROM mirror, the 68000 reads:

- `$00000000` (word): the ROM's initial-SSP high word, which is simultaneously the ROM header ID word.
- `$00000002` (word): the ROM's initial-SSP low word.
- `$00000004` (word): the ROM's initial-PC high word.
- `$00000006` (word): the ROM's initial-PC low word.

The HRM documents this:

> "The Amiga System ROM contains an ID code as the first word. The value of the ID code may change in the future. The second word of the ROM contains a JMP instruction (`$4ef9`). The next two words are used as the initial program counter by the 680x0 processor." (HRM §Reset and Early Startup Operation)

So the ROM layout at its base (which the CPU sees at `$0` while OVL=1, and at `$FC0000` or `$F80000` normally) looks like:

| Offset | Contents |
|---|---|
| `$00` | ID word (may change; V1.x had one value, V2+ another). Also functions as the initial-SSP high word. |
| `$02` | Low word of initial SSP (a short-form fill — does not need to be meaningful as a stack since the code jumps immediately). |
| `$04` | `$4EF9` — `JMP abs.L` instruction — serves as the initial-PC high word. |
| `$06–$08` | 32-bit absolute jump target into Kickstart — serves as the initial-PC low word + next word. |

(HRM §Reset and Early Startup Operation; Exec RKM Appendix C software memory map.)

That `JMP` instruction is a deliberate trick: the CPU interprets `$4 $6 $8` as "initial PC = longword at `$4`", so on the very first fetch it loads the PC with whatever the two words at `$4` and `$6` encode, then starts fetching there. Because the first word at `$4` is `$4EF9`, the PC ends up pointing to the "JMP abs.L" instruction, and the target longword at `$6–$8` is the actual Kickstart cold-start entry point. The Amiga ROM uses this so that a disassembly starting at `$4` reads naturally — the bytes at `$4–$A` are simultaneously the reset vector and an executable `JMP abs.L` that re-dispatches to the Kickstart cold entry. It's not documented in the HRM as "it works because the CPU executes it as well as reads it," but the effect is that the ROM header and the first reset vector overlap by design.

### Kickstart ROM size & addresses per version

| Kickstart | ROM size | Address range | Notes |
|---|---|---|---|
| 1.0 (A1000) | 256K on WOM (Write-Once Memory) | `$FC0000–$FFFFFF`, mirror at `$F80000–$FBFFFF` | Loaded from disk at power-on; write-protected after first write into the `$F80000` mirror (SPG §1.5.1.1, "Amiga 1000 WOM"). |
| 1.2 / 1.3 (A500/A2000/A1000 ROM) | 256K ROM | `$FC0000–$FFFFFF`, mirror at `$F80000–$FBFFFF` | (SPG §1.5.1.1; A500/A2000 TRM §Memory Decoder PAL) |
| 2.x / 3.x | 512K ROM | `$F80000–$FFFFFF` | The upper half at `$FC0000` is still the "main" ROM body; the `$F80000` half is used for real code. |

*(SPG explicitly states the 1.x memory map: "The 256KB of ROM at `$FC0000` contains the Amiga Kickstart. The range from `$F80000` to `$FBFFFF` is identical to the range from `$FC0000` to `$FFFFFF`. This is a mirror of the Kickstart ROM." (SPG §1.5.1.1). The 512K ROM mapping for 2.x/3.x is implicit from the fact that the Exec RKM memory map gives the system ROM range as `F80000-FFFFFF` (Exec RKM Appendix C) and because the Release 2 compatibility notes mention V36 Kickstart having a "Kickety-Split" redirecting JMP at `$FC0002`, confirming that on 2.04 the real code lives below `$FC0000` while `$FC0002` is a compatibility hook (RKM Libraries & Devices §Release 2 Compatibility).)*

Release 2 compatibility notes explicitly warn emulator authors and system programmers:

> "Do not jump to location `$FC0002` — the start of the ROM under 1.3 — as part of performing a system RESET. The 2.04 Kickstart ROM has a temporary compatibility hack called 'Kickety-Split' which is a redirecting jump at `$FC0002`. This hack does not appear on the A3000 ROM and due to space considerations will not appear on future machines." (RKM Libraries §Release 2 Compatibility, Exec)

### The A1000 WOM detour

The A1000 shipped without a ROM. Instead, it had 256K of "Write-Once Memory" (WOM) — RAM that turned into ROM after first write. A small boot ROM lived at `$F80000`, and the "Kickstart" was loaded from a Kickstart floppy. After the load, a write to the `$F80000–$FBFFFF` range switched out the boot ROM and write-protected the WOM. SPG §1.5.1.1:

> "Immediately after a reset, the boot ROM is at `$F80000` (since on a reset the OVL line is set, the reset vector also comes from boot ROM) and it is possible to write into Kickstart. It can be changed as desired! This condition holds only until you try to write something in the boot ROM range from `$F80000` to `$FBFFFF`. Then the boot ROM is masked out again and the Kickstart memory is write-protected."

Emulator-wise, unless you specifically care about A1000 WOM behaviour you can ignore this path and always behave as if ROM is present at `$F80000/$FC0000`.

---

## Phase 2 — ROM header and Kickstart entry

Taking the description above and following the ROM source through to the Kickstart cold-start code — the SPG's disassembly of Kickstart 1.2 shows the first instructions starting around `$FC00D2` (SPG §2.9.1 — *Documentation of the reset routine*). The jump from the reset vector lands there via the `JMP abs.L` at `$4`.

Quoting the SPG disassembly directly (this is the authoritative trace of early Kickstart behaviour in the corpus):

```
fc00d2  lea     $040000,A7              set stack pointer
fc00d8  move.l  #$00020000,D0           value for delay loop
fc00de  subq.l  #1,D0                   decrement value
fc00e0  bgt.s   $fc00de                 branch if not decremented
fc00e2  lea     -228(PC)(=$fc0000),A0   set pointer to Kickstart ROM
fc00e6  lea     $f00000,A1              load comparison value
fc00ec  cmpa.l  A1,A0                   is Reset at $F00000
fc00ee  beq.s   $fc00fe                 branch if so
fc00f0  lea     12(PC)(=$fc00fe),A5     set pointer to program continuation
fc00f4  cmpi.w  #$1111,(A1)             is module at $F00000?
fc00f8  bne.s   $fc00fe                 branch if not
fc00fa  jmp     2(A1)                   else enter
```

This is doing three things:

1. **Set the supervisor stack pointer to `$040000`** — 256 KB into chip RAM. This is a temporary stack; the CPU cannot rely on anything being at that address yet, so the code has to either be certain 256 KB of chip RAM exists, or not use the stack until after overlay is cleared and RAM is verified. In practice early 1.x ROMs assume 256 KB.
2. **Delay loop** — a tight `subq/bgt` loop of `$00020000` iterations (~128K iterations, ~1M cycles @ 7 MHz ≈ 140 ms). This is waiting for hardware (custom chips, CIAs, DRAM) to actually finish power-on settling.
3. **Check for a diagnostic module at `$F00000`** — on some machines the `$F00000` region could contain a diagnostic ROM. If a word `$1111` is there it gets called; otherwise control falls through to `$FC00FE`, which is the "real" Kickstart reset code.

The check-for-`$F00000`-diag is an early hook used by Commodore's internal diagnostic ROMs; it's not normally occupied. Emulators that don't model a diag ROM should just return `$FF`s for reads in `$F00000–$F7FFFF` so the `cmpi.w #$1111` fails and the code falls through.

---

## Phase 3 — Very early Kickstart: silence the custom chips, load the vector table

Continuing the SPG trace:

```
fc00fe  move.b  #$03,$bfe201            switch port to output (DDRA = $03)
fc0106  move.b  #$02,$bfe001            turn LED off (bit 1 = /LED, set high)
fc010e  lea     $dff000,A4              pointer to chip addresses
fc0114  move.w  #$7fff,D0               load value
fc0118  move.w  D0,154(A4)              disable all interrupts    INTENA=$09A
fc011c  move.w  D0,156(A4)              clear interrupts          INTREQ=$09C
fc0120  move.w  D0,150(A4)              disable DMA               DMACON=$096
fc0124  move.w  #$0200,256(A4)          BPLCON0 = $0200  (hires off, color burst)
fc012a  move.w  #$0000,272(A4)          BPLCON2 = $0000
fc0130  move.w  #$0444,384(A4)          COLOR00 = $444   (a grey for reset screen)
fc0136  move.w  #$0008,A0               set pointer to exception vectors base
fc013a  move.w  #$002d,D1               counter for number of vectors
fc013e  lea     1140(PC)(=$fc05b4),A1   set pointer to hard error routine
fc0142  move.l  A1,(A0)+                enter exceptions
fc0144  dbf     D1,$fc0142              branch if not done
fc0148  bra.l   $fc30c4                 check guru
```

What this does, step by step:

1. **`$BFE201 DDRA = $03`** — Force CIA-A port A bits 0 and 1 (OVL and /LED) to outputs; bits 2–7 (disk-change, write-protect, track 0, RDY, fire-button x2) stay as inputs. This is the "canonical" DDRA setting — see HRM Appendix F:

   ```
   BFE201  ddra   Direction for port A (BFE001); 1=output (set to 0x03)
   ```
   (HRM Appendix F memory map)

2. **`$BFE001 PRA = $02`** — write bit 1 high (turn LED *off*) and bit 0 low (OVL=0). **This is the moment the overlay is disabled.** After this instruction the memory map is "normal": chip RAM at `$0`, Kickstart ROM only at `$F80000/$FC0000`. All the preceding PC-relative code must have been relocatable (it was — `lea -228(PC),A0`), because it ran in the mirrored ROM at `$0000xx` addresses before this point and will run at `$FC00xx` addresses afterwards.

   Note: at exactly this instruction the PC that the CPU is executing transitions from "these instructions are being served out of the `$0000xx` mirror" to "these instructions are being served out of `$FC00xx`". Because the 68000 is executing from ROM and the ROM contents are identical in both places, this is transparent to the CPU — but an emulator must handle the address-decode flip cleanly on the very cycle the write to `$BFE001` takes effect. **Emulator gotcha**: do not cache overlay state in a way that ignores the prefetch queue; after the write, the 68000 may still be executing a prefetched instruction from the mirror.

3. **`$DFF09A INTENA = $7FFF`** — `$7FFF` with bit 15 (SET/CLR) cleared means "clear all bits written as 1 in positions 14–0". So this disables every interrupt, including the master enable (bit 14 = INTEN). (HRM §System Control Hardware — Interrupt Control Registers.)

4. **`$DFF09C INTREQ = $7FFF`** — same pattern, clears every pending interrupt request.

5. **`$DFF096 DMACON = $7FFF`** — clears all DMA enables including the DMA master enable bit 9.

6. **`$DFF100 BPLCON0 = $0200`** — clears all display-mode bits, leaves color-burst enabled. BPLCON0 bit 9 (`COLOR`, enable color burst).

7. **`$DFF102 BPLCON2 = $0000`** — clears playfield priority and dual-playfield selection.

8. **`$DFF180 COLOR00 = $0444`** — the screen background is forced to a neutral grey (`RGB = $444`). **This is the grey screen the user sees during early Kickstart.** The various coloured screens (red, green, yellow) that users recognise from a crashed Amiga boot are set later by specific error paths.

9. **Build the exception vector table** — start at `$0008` (not `$0000`, because `$0–$7` already hold the reset vector, SSP/PC, which the CPU will not re-fetch but which the system leaves alone for now). Loop `$2d + 1 = 46` times (`dbf` is inclusive) writing a default exception handler pointer (`$FC05B4`, "hard error routine") into every exception slot. All exception vectors 2–47 (bus error through the reserved `$BC–$BF` plus initial trap handlers) get the same fallback handler, which will later be fixed up for specific traps.

10. **Branch to `$fc30c4` — "check guru"**. This path decides whether this reset is a fresh power-on or a "guru" reset (an alert after a crashed previous boot that still has `LastAlert` contents to preserve). After that, it returns to the mainline at `$fc014c`.

### Important: initial exception vectors after hardware reset

The 68000 will *not* reload SSP/PC from `$0/$4` at this point — those are the reset vectors and are fetched exactly once on reset. Everything from `$8` upward is the standard exception table:

| Offset | Exception |
|---|---|
| `$08` | Bus error |
| `$0C` | Address error |
| `$10` | Illegal instruction |
| `$14` | Division by zero |
| `$18` | CHK instruction |
| `$1C` | TRAPV |
| `$20` | Privilege violation |
| `$24` | Trace |
| `$28` | Line 1010 (A-line) |
| `$2C` | Line 1111 (F-line) |
| `$60` | Spurious interrupt |
| `$64`–`$7C` | Autovector levels 1–7 |
| `$80`–`$BC` | Traps 0–15 |

The Amiga stores a default "hard error" vector for all of these at this stage, then overwrites the interrupt-autovector slots later (SPG §2.9.1 continuing trace shows interrupt structure installed at `$fc25c`).

---

## Phase 4 — ExecBase validation and the ColdCapture trap

After the initial silence-the-chips + guru-check, the code at `$fc014c` checks whether ExecBase is still valid from a previous boot:

```
fc014c  move.l  $0004,D0                get ExecBase
fc0150  btst    #0,D0                   ExecBase at even address?
fc0154  bne.s   $fc01ce                 error if odd
fc0156  move.l  D0,A6                   ExecBase to A6
fc0158  add.l   38(A6),D0               add ChkBase
fc015c  not.l   D0                      invert result
fc015e  bne.s   $fc01ce                 branch if error
; ... word checksum over offsets 34..78 ...
fc0172  move.l  42(A6),D0               ColdCapture to D0
fc0176  beq.s   $fc0184                 branch if not set
fc0178  move.l  D0,A0                   pointer to A0
fc017a  lea     8(PC)(=$fc0184),A5      return pointer
fc017e  clr.l   42(A6)                  clear ColdCapture
fc0182  jmp     (A0)                    jump
```
(SPG §2.9.1)

### Why this matters

ExecBase always lives at absolute address `$00000004` as a longword pointer: `Move.l $4,A6` is the universal way to get a pointer to ExecBase (SPG §2.8). The Kickstart reset routine needs to check whether the ExecBase from a *previous* boot is still trustworthy, because **a 68000 RESET or warm reboot may leave the ExecBase area untouched in RAM** (see Phase 6 below — ColdReboot preserves as little as possible but the system deliberately keeps ExecBase alive across soft reboots where possible).

The validation is:

1. Longword at `$4` must be even and non-zero.
2. `ExecBase + ChkBase` must equal `$FFFFFFFF`, i.e. `ChkBase = ~ExecBase`. This is a single-longword consistency check that ExecBase was correctly installed by whatever code ran last.
3. A word checksum over offsets `34..78` (SoftVer, LowMemChkSum, ChkBase, ColdCapture, CoolCapture, WarmCapture, SysStkUpper, SysStkLower, MaxLocMem, DebugEntry, DebugData, AlertData, MaxExtMem, ChkSum) must sum to zero.

If any of these fails, jump to `$fc01ce` — the full ExecBase reinitialisation path.

If all of them pass, and `ColdCapture` (offset 42) is non-zero, **jump to the ColdCapture routine**. This is how reset-proof programs and debuggers get control before any further boot work is done. The conventions are:

- `A5` holds the "return address" (the next instruction after the `JMP (A0)`). The caller jumped, not `jsr`'d — there's no stack yet, so the caller cannot push a return address. The convention is to `jmp (A5)` to continue the boot.
- `D6`/`D7` hold the Last Alert info (a guru number and a memory pointer) if one was captured by the "check guru" path at `$fc30c4` (SPG §2.9.1).
- **ColdCapture is cleared before being called** — the ROM writes `clr.l 42(A6)` before the jump, so a ColdCapture routine that wants to survive reset has to re-install itself each time and recompute the ChkSum.
- ColdCapture code "cannot use the stack and cannot call subroutines, since the stack has not been initialized yet" (SPG §2.9.3).
- At ColdCapture time, "nothing noteworthy has happened beyond disabling interrupts and DMA" (SPG §2.8 description of ColdCapture field).

### The three capture vectors

The ExecBase struct has three capture-vector fields (Exec RKM §execbase.h; SPG §2.8):

```c
struct ExecBase {
    ...
    APTR ColdCapture;   /* offset 42 */
    APTR CoolCapture;   /* offset 46 */
    APTR WarmCapture;   /* offset 50 */
    ...
};
```

- **ColdCapture** — called very early, before the stack exists and before memory is reinitialised. The reset routine *clears it* before calling, so a ColdCapture handler that wants to persist has to re-install itself and recompute the ChkSum over offsets 34–78 (SPG §2.9.1). This is the hook that reset-proof debuggers like ROMWack and loader hacks use.
- **CoolCapture** — called much later, after the memory, stack, exception table and Exec library have been (re-)initialised, but before the ROMTag modules are initialised with `InitCode(COLDSTART)`. The reset routine does **not** clear CoolCapture; return is by `RTS` (SPG §2.8).
- **WarmCapture** — the SPG says "to the best of our knowledge it is never called" (SPG §2.8) and the reset trace at `$fc0526` does fetch it (`fc0526  move.l 50(A6),D0   get WarmCapture`) but the code that follows it is marked as "no longer accessed". **Emulator note**: unless you care about obscure 1.x debugger hacks, implementing only ColdCapture and CoolCapture is sufficient.

---

## Phase 5 — Memory sizing: chip RAM, fast RAM, MemList

If the ExecBase validation failed, or this is a first-ever power-on, the reset routine at `$fc01ce` reinitialises everything:

```
fc01ce  lea     $0400,A6              lowest possible RAM area
fc01d2  suba.w  #$fd8a,A6             find address of ExecBase,
                                       if no fast memory
fc01d6  lea     $c00000,A0            lowest fast memory area
fc01dc  lea     $dc0000,A1            highest possible RAM limit
fc01e2  lea     6(PC)(=$fc01ea),A5    return pointer
fc01e6  bra.l   $fc061a               get upper memory limit
```
(SPG §2.9.1)

Notes:

- `$C00000` is "slow RAM" / "ranger memory" / "trapdoor RAM" — the A500 trapdoor slot and A2000 Ranger memory live here, below the `$DC0000` I/O region. Kickstart probes for RAM in this range with a destructive read/write test, looking for the upper bound.
- The *placement* of ExecBase is computed relative to the top of memory: `SysStkUpper = top of RAM` (or chip-RAM top if no fast RAM); ExecBase ends up just below that, and the supervisor stack takes the top `$1800` bytes (6 KB) of RAM. The calculation `A6 = top - $FD8A` is what pins ExecBase near the top of the usable region.
- After this, the routine calls a memory-probing subroutine that scans `$0000–$200000` for the upper bound of chip RAM (accepting 256 KB minimum, 512 KB maximum on a Kickstart 1.x system; see below).
- The 1.2 routine rejects chip RAM sizes outside the `$00040000–$00080000` window (256 KB to 512 KB inclusive). 256 KB below means hard reset; 512 KB above means hard reset. 1 MB chip RAM machines need Fat Agnus and a different Kickstart.
- For fast RAM, the routine accepts `$00C40000–$00DC0000` (ranger memory region), and requires the upper boundary to be on an even-$40000-byte boundary (SPG §2.9.1). Autoconfig memory at `$200000–$9FFFFF` is *not* discovered here — that happens later in Phase 11 via expansion.library (see "Reset routine uses fixed C00000 recognition" comment in SPG §2.9.1).

**Version difference**: Under Kickstart 2.x+, ExecBase is relocated to autoconfig fast RAM if available ("ExecBase is moved to expansion memory if possible. Before, ExecBase would only end up in one of two fixed locations. Now, ColdCapture may be called after expansion memory has been configured." — RKM Libraries §Release 2 Compatibility, Exec). So on 2.x+ the order changes: memory probe happens, then autoconfig runs, then ExecBase is created in fast RAM, then ColdCapture is called. This is a significant difference between 1.x and 2.x+ boot order.

### MemList construction

Each contiguous memory region is represented by a `struct MemHeader` on a `struct List` called `MemList` in ExecBase (Exec RKM §execbase.h). After probing, the reset routine builds one MemHeader for chip RAM and, if present, one for fast RAM:

```
fc0384  lea     88(A6),A0            pointer to end of ExecBase
fc0388  lea     -88(PC)(=$fc0332),A1 String "Fast Mem"
fc038c  moveq   #$00,D2              priority of the MemHeader
fc038e  move.w  #$0005,D1            memory attributes (Public, Fast)
fc0392  move.l  A4,D0                pointer to end of fast RAM
fc0394  sub.l   A0,D0                subtract ExecBase structure
fc0396  subi.l  #$00001800,D0        subtract SysStack
fc039c  bsr.l   $fc19ea              create MemHeader structure
```
(SPG §2.9.1)

Memory attributes for `mh_Attributes`:

- `MEMF_PUBLIC` = bit 0 (`$01`) — memory is safe to be accessed by other tasks.
- `MEMF_CHIP` = bit 1 (`$02`) — DMA-able chip RAM.
- `MEMF_FAST` = bit 2 (`$04`) — fast (non-DMA) RAM.

Priority (`mh_Node.ln_Pri`):

- Fast RAM gets priority `0`.
- Chip RAM gets priority `-10` (`$f6` sign-extended) — allocations without `MEMF_CHIP` skip over it in favour of fast RAM.

The MemList is a priority-sorted doubly-linked list, so `AllocMem(size, 0)` starts at the fast-RAM MemHeader and falls through to chip RAM only if fast RAM is exhausted or unavailable. `AllocMem(size, MEMF_CHIP)` starts searching for a MemHeader with `MEMF_CHIP` set.

### Chip RAM is cleared

The reset routine explicitly clears chip RAM from `$00C0` (just past the exception vectors) up to the upper bound, and clears fast RAM from `$C00000` up (SPG §2.9.1, label `$fc0208` and `$fc01f4`). A fresh boot starts with zeroed RAM.

---

## Phase 6 — Rebuilding ExecBase, SysBase at $4, checksum

After the MemHeaders are built, the reset routine constructs the ExecBase Library structure:

```
fc0274  movem.l D4-D2,546(A6)       set KickMemPtr, KickTagPtr, KickCheckSum
fc027a  move.l  A6,$0004            set ExecBase pointer at absolute $4
fc027e  move.l  A6,D0               pointer to D0
fc0280  not.l   D0                  calculate ChkBase
fc0282  move.l  D0,38(A6)           enter ChkBase
fc0286  move.l  A4,D0               upper RAM limit to D0
fc0288  bne.s   $fc028c             branch if fast RAM available
fc028a  move.l  A3,D0               else set upper chip RAM
fc028c  move.l  D0,A7               limit as system stack
fc028e  move.l  D0,54(A6)           and enter SysStkUpper
fc0292  subi.l  #$00001800,D0       subtract length of stack
fc0298  move.l  D0,58(A6)           SysStkLower
fc029c  move.l  A3,62(A6)           set limit of chip RAM
fc02a0  move.l  A4,78(A6)           set limit of fast RAM
```
(SPG §2.9.1)

This:

- Writes the ExecBase pointer at absolute address `$00000004`. **From this point onward, `move.l $4,A6` returns the ExecBase pointer on any Amiga.** (Exec RKM §execbase.h; SPG §2.8)
- Writes `ChkBase = ~ExecBase` at offset 38. Now `ExecBase + ChkBase == $FFFFFFFF` and the next reset will find a valid ExecBase (SPG §2.8, field ChkBase).
- Installs the supervisor stack: top = upper RAM limit, size = `$1800` (6 KB), bottom = top − `$1800`. `A7` gets the top. `SysStkUpper` and `SysStkLower` fields record these.
- Records `MaxLocMem` (chip RAM top) at offset 62 and `MaxExtMem` (fast RAM top) at offset 78.

### ExecBase layout (the boot-relevant fields)

From Exec RKM `exec/execbase.h` and SPG §2.8 (offsets quoted for both 1.x compatibility and the canonical layout):

| Offset | Type | Name | Purpose |
|---|---|---|---|
| `$00` | struct Library | LibNode | exec.library is a library like any other — it has a node, negative-size jump table, positive-size data region, version, revision, checksum, open count. `lib_Sum` is computed by `SumLibrary()`. |
| `$22` | UWORD | SoftVer | Kickstart release number (36 for V2.0, 39 for V3.0, etc). |
| `$24` | WORD | LowMemChkSum | Programmer-writable, equalises the checksum over 34..78 when custom capture vectors are installed. |
| `$26` | ULONG | ChkBase | `~ExecBase`. Used at reset to check ExecBase survived. |
| `$2A` | APTR | ColdCapture | Cold capture vector. Cleared by reset before being called. |
| `$2E` | APTR | CoolCapture | Cool capture vector. Not cleared. |
| `$32` | APTR | WarmCapture | Warm capture vector. Never called in practice. |
| `$36` | APTR | SysStkUpper | Top of supervisor stack. |
| `$3A` | APTR | SysStkLower | Bottom of supervisor stack. |
| `$3E` | ULONG | MaxLocMem | Top of chip RAM. |
| `$42` | APTR | DebugEntry | Debugger hook. |
| `$46` | APTR | DebugData | Debugger data buffer. |
| `$4A` | APTR | AlertData | Last alert info. |
| `$4E` | APTR | MaxExtMem | Top of "base" (non-expansion) fast RAM. |
| `$52` | UWORD | ChkSum | Word checksum over 34..78 such that the sum is zero. |
| `$54` | struct IntVector[16] | IntVects | The 16 Exec-level interrupt vectors (see Subsystems section). |
| `$114` | struct Task * | ThisTask | Currently-running task. |
| `$118` | ULONG | IdleCount | Performance counters. |
| `$128` | UWORD | AttnFlags | CPU/FPU/PAL/50Hz presence flags. |
| `$12A` | UWORD | AttnResched | Rescheduling attention flag. |
| `$12C` | APTR | ResModules | Pointer to ROMTag (resident module) array. |
| `$138` | APTR | TaskExitCode | Where tasks go when they fall off their entry function. |
| `$13C` | ULONG | TaskSigAlloc | Task signal allocation map. |
| `$140` | UWORD | TaskTrapAlloc | Task trap allocation. |
| `$142` | struct List | MemList | Chip + fast memory headers. |
| `$150` | struct List | ResourceList | System resources. |
| `$15E` | struct List | DeviceList | Installed devices. |
| `$16C` | struct List | IntrList | Unused in 1.x (SPG §2.8). |
| `$17A` | struct List | LibList | Installed libraries. |
| `$188` | struct List | PortList | Named public message ports. |
| `$196` | struct List | TaskReady | Ready tasks. |
| `$1A4` | struct List | TaskWait | Waiting tasks. |
| `$1B2` | struct SoftIntList[5] | SoftInts | Software interrupt queues, priority -32/-16/0/+16/+32. |
| `$202` | LONG[4] | LastAlert | Last alert data (fetched from D6/D7 by `$fc30c4`). |
| `$212` | UBYTE | VBlankFrequency | 50 or 60. |
| `$213` | UBYTE | PowerSupplyFrequency | 50 or 60. |
| `$214` | struct List | SemaphoreList | Signal semaphores. |
| `$222` | APTR | KickMemPtr | Kickstart-time MemEntry list (persists across reset). |
| `$226` | APTR | KickTagPtr | Kickstart-time extra ROMTag pointer list. |
| `$22A` | APTR | KickCheckSum | Checksum over KickMemPtr/KickTagPtr. Computed by `SumKickData()`. |

(Exec RKM §execbase.h; SPG §2.8.)

### SumKickData and the persist-across-reset mechanism

The `KickMemPtr` / `KickTagPtr` / `KickCheckSum` triple is how applications add Resident modules or reserve memory that survives a reset (Exec RKM/Libraries §SumKickData Autodoc):

- `KickMemPtr` → linked list of `MemEntry` structures. On reset, Exec calls `AllocAbs()` for each entry. If *all* allocations succeed, the memory areas are reserved and preserved.
- `KickTagPtr` → longword array of ROMTag (Resident) structure pointers, same format as the main ResModules list. On reset, these tags are merged into the module list.
- `KickCheckSum` → sum of the above two structures, computed by `SumKickData()`. If it doesn't match, both pointers are ignored — so if a task corrupts the structures, reset falls back to stock Kickstart.
- **All memory referenced by `KickMemPtr` must be reachable at reset time**, i.e. in chip RAM or ranger memory (`$C00000–$D80000`). Autoconfig fast RAM isn't configured yet when SumKickData is checked, so pointers into autoconfig RAM don't work (SumKickData Autodoc).

This is the key to "reset-proof" code like `RAD:` (the reset-proof RAM disk).

### ColdReboot: the documented reboot sequence

For a software-initiated system reboot, the HRM gives the *only* supported code (HRM §Reset and Early Startup Operation, "ColdReboot - Official code to reset any Amiga (Version 2)"):

```
* NAME
*   ColdReboot - Official code to reset any Amiga (Version 2)
*
* SYNOPSIS
*   ColdReboot()
*   void ColdReboot(void);
*
* FUNCTION
*   Reboot the machine. All external memory and peripherals will be
*   RESET, and the machine will start its power up diagnostics.
*
*   Rebooting an Amiga in software is very tricky. Differing memory
*   configurations and processor cards require careful treatment. This
*   code represents the best available general purpose reset. The
*   MagicResetCode must be used exactly as specified here. The code
*   _must_ be longword aligned. Failure to duplicate the code EXACTLY
*   may result in improper operation under certain system configurations.

ABSEXECBASE     EQU 4                   ; Pointer to the Exec library base
MAGIC_ROMEND    EQU $01000000           ; End of Kickstart ROM
MAGIC_SIZEOFFSET EQU -$14               ; Offset from end of ROM to Kickstart size
V36_EXEC        EQU 36                  ; Exec with the ColdReboot() function
TEMP_ColdReboot EQU -726                ; Offset of the V36 ColdReboot function

ColdReboot:     move.l  ABSEXECBASE,a6
                cmp.w   #V36_EXEC,LIB_VERSION(a6)
                blt.s   old_exec
                jmp     TEMP_ColdReboot(a6)     ; Let Exec do it...

old_exec:       lea.l   GoAway(pc),a5           ; address of code to execute
                jsr     _LVOSupervisor(a6)      ; trap to code at (a5)...

;--- MagicResetCode  DO NOT CHANGE -------------
                CNOP    0,4                     ; IMPORTANT! Longword align!
GoAway:         lea.l   MAGIC_ROMEND,a0         ; (end of ROM)
                sub.l   MAGIC_SIZEOFFSET(a0),a0 ; (end of ROM)-(ROM size)=PC
                move.l  4(a0),a0                ; Get Initial Program Counter
                subq.l  #2,a0                   ; now points to second RESET
                reset                           ; first RESET instruction
                jmp     (a0)                    ; CPU Prefetch executes this
;--- NOTE: the RESET and JMP instructions must share a longword!
;--- DO NOT CHANGE ---
```

The critical thing an emulator must model correctly:

- The 68000 has a prefetch queue. The `reset` instruction issues a reset on the external line but the 68000 **continues executing** with the next prefetched instruction. The `jmp (a0)` that follows `reset` **must be in the same 32-bit word** as `reset` so it's already in the prefetch queue when `reset` fires — because the reset pulse disables all the RAM/autoconfig mapping, and if the 68000 had to fetch `jmp (a0)` from RAM afterwards it would fault.
- After `reset`, the overlay is back on (because ROM-mapping-at-zero is a side effect of hardware reset), so when `jmp (a0)` executes, the target (which is the Kickstart cold-start address, i.e. the second longword of ROM) is reachable via the `$0` mirror.
- Emulator consequences: your 68000 core must implement prefetch queue behaviour for `reset`/`jmp` pair correctness, and your bus arbitration must flip the overlay back on the moment `reset` fires.

---

## Phase 7 — CPU detection and AttnFlags

After rebuilding ExecBase, the reset routine determines which CPU and FPU are present:

```
fc02a8  bsr.l   $fc0546                 processor test
fc02ac  or.w    D0,296(A6)              set bits in AttnFlags
```
(SPG §2.9.1)

The processor test returns a bitmask in `D0` that gets OR'd into `AttnFlags` (Exec RKM `execbase.h`, AttnFlags defines):

```c
#define AFB_68010   0    /* (will remain set for 68020 as well) */
#define AFB_68020   1
#define AFB_68881   4
#define AFB_PAL     8    /* PAL/NTSC */
#define AFB_50HZ    9    /* Clock rate */
```

The detection technique (not spelled out verbatim in the SPG, but the usual method on 68k):

- **68010 vs 68000**: issue a `MOVE SR,Dn` (which is privileged on 68010 and above, but non-privileged on 68000). On a 68000 it succeeds; on a 68010+ it traps via privilege violation. The default exception handler is already set up to handle this detection and return via a flag.
- **68020**: similar trick with a cache-control-register access or `MOVEC`.
- **68881**: probe for FPU presence with an `FNOP` or `FMOVECR`.

`AFB_PAL` and `AFB_50HZ` are set from inspecting the Agnus ID bit (ECS and later) or derived from `VBlankFrequency`.

The bits in `AttnFlags` are consulted later by graphics.library, timer.device and others to adjust timing and choose code paths.

The reset routine then immediately patches exception vectors for 68010+:

```
fc03ec  lea     1166(PC)(=$fc087c),A0   pointer to new traps
fc03f0  move.w  #$0008,A1               pointer to destination
fc03f4  move.l  A0,(A1)+                enter exceptions
fc03f6  move.l  A0,(A1)+                enter exceptions
fc03f8  move.l  #$00fc08ba,-28(A6)      enter expansion
fc0400  move.l  #$42c04e75,-528(A6)     enter expansion
fc0408  btst    #4,D0                   68881 used?
fc040c  beq.s   $fc041e                 branch if not present
fc040e  move.l  #$00fc108a,-52(A6)      enter expansion
fc0416  move.l  #$00fc10e8,-58(A6)      enter expansion
```
(SPG §2.9.1)

- On 68010+, bus error (`$08`) and address error (`$0C`) get a different handler than the 68000, because the stack frame format is larger.
- On 68881, the F-line exception gets an FPU handler.

**Version difference**: On 68010+ machines the Vector Base Register (VBR) may be non-zero under Kickstart 2.x+, meaning exception vectors may live somewhere other than `$0–$FF`. The Release 2 compatibility notes explicitly warn: *"Exception/Interrupt vectors may move. This means the 68010 and above Vector Base Register (VBR) may contain a non-zero value. Poking assumed low memory vector addresses may have no effect. You must read the VBR on 68010 and above to find the base."* (RKM Libraries §Release 2 Compatibility, Exec).

---

## Phase 8 — System lists, exec.library as a library, the initial task

```
fc02b0  lea     32(PC)(=$fc02d2),A1     pointer to table
fc02b4  move.w  (A1)+,D0                offset in D0
fc02b6  beq.l   $fc033e                 end if no more offsets
fc02ba  lea     0(A6,D0.W),A0           set pointer to position
fc02be  move.l  A0,(A0)                 enter list header
fc02c0  addq.l  #4,(A0)                 point to lh_Tail
fc02c2  clr.l   4(A0)                   clear lh_Tail
fc02c6  move.l  A0,8(A0)                set lh_TailPred
fc02ca  move.w  (A1)+,D0                get lh_Type
fc02cc  move.b  D0,12(A0)               and set
fc02d0  bra.s   $fc02b4                 unconditional jump
```
(SPG §2.9.1)

This initialises the following lists in ExecBase in canonical empty-list form (`lh_Head = &lh_Tail; lh_Tail = NULL; lh_TailPred = &lh_Head; lh_Type`):

- `MemList` (memory headers)
- `ResourceList` (resources)
- `DeviceList` (devices)
- `LibList` (libraries)
- `PortList` (public ports)
- `TaskReady` (ready-to-run tasks)
- `TaskWait` (waiting tasks)
- `IntrList` (unused)
- `SoftInts[0..4]` (5 soft-interrupt queues)
- `SemaphoreList`

### Creating exec.library itself

Exec is a library. It's special in that it has to exist before any other library can be created via `MakeLibrary()`, because `MakeLibrary()` is an Exec function. The reset routine constructs Exec by hand using internal helpers:

```
fc0372  lea     5836(PC)(=$fc1a40),A1   pointer to table
fc0376  move.l  A1,A2
fc0378  bsr.l   $fc1576                 function: MakeFunction()
fc037c  move.w  D0,16(A6)               enter library length (lib_NegSize)
```
(SPG §2.9.1)

The table at `$fc1a40` is the jump-table initialiser for exec.library itself, containing word-relative offsets to each function body. `MakeFunction()` (the private helper) walks the table and installs the function pointers immediately below `A6` (negative offsets from the library base), producing the `-$1E` (`LIB_OPEN`), `-$24` (`LIB_CLOSE`), `-$2A` (`LIB_EXPUNGE`), `-$30` (`LIB_EXTFUNC`), then user function vectors.

Library vector table convention (Exec RKM §libraries.h):

```c
#define LIB_VECTSIZE    6        /* JMP abs.L is 6 bytes */
#define LIB_RESERVED    4        /* Open, Close, Expunge, ExtFunc */
#define LIB_BASE        (-LIB_VECTSIZE)
#define LIB_USERDEF     (LIB_BASE - (LIB_RESERVED * LIB_VECTSIZE))

#define LIB_OPEN    (-6)
#define LIB_CLOSE   (-12)
#define LIB_EXPUNGE (-18)
#define LIB_EXTFUNC (-24)
```

Each function vector is a 6-byte entry containing `JMP abs.L`. An assembly caller can use the `_LVO` (library vector offset) naming convention:

```
        XREF    _LVOOpenLibrary
        XREF    ExecBase

        MOVE.L  ExecBase,A6     ; $4 contains the ExecBase pointer
        JSR     _LVOOpenLibrary(A6)
```
(1990 RKM Appendix D.)

### lib_Sum and library checksums

```
fc03c2  bsr.l   $fc140c                 calculate library checksum
```
(SPG §2.9.1)

After `exec.library` is built, Exec walks its own function vectors and computes `lib_Sum`, which `SumLibrary()` later uses to detect corruption (exec.library/SumLibrary Autodoc).

### Setting up interrupt vectors and installing handlers

```
fc041e  bsr.l   $fc125c                 enter interrupt structure
fc0422  lea     $dff000,A0              pointer to chip addresses
fc0428  move.w  #$8200,150(A0)          allow blitter DMA
fc042e  move.w  #$c000,154(A0)          allow interrupts
fc0434  move.w  #$ffff,294(A6)          clear IDNestCnt
```
(SPG §2.9.1)

- The `$fc125c` helper walks a table of interrupt priorities and installs default "handler" and "server" functions into the 16 `IntVects[0..15]` slots in ExecBase.
- `DMACON = $8200` enables blitter DMA (bit 9 DMAEN, bit 6 BLTEN). Note that nothing else (bitplane, copper, sprite, disk, audio) is enabled yet.
- `INTENA = $C000` enables master INTEN (bit 14) with SET/CLR bit 15 set, so interrupts as a class are allowed but no individual sources are yet enabled.
- `IDNestCnt = $FFFF` means "interrupts enabled" (the count represents nesting, -1 = enabled).

### The initial task

Before any user code can run, Exec needs *a* task to be running, so that all the task-relative code (which reads `ThisTask` from ExecBase) has a valid task to look at. The reset routine creates and enters the "exec.library" task:

```
fc045e  lea     4112(A2),A0             pointer to start for stack
fc0462  lea     8(A0),A1                calculate MemEntry
fc0466  addi.l  #$00000010,D0           add MemList
fc046c  move.l  D0,58(A1)               set SPLower
fc0470  move.l  A0,62(A1)               set SPUpper
fc0474  move.l  A0,54(A1)               set SPReg
fc0478  move    A0,USP                  also set as user stack
...
fc04a4  move.l  A1,276(A6)              enter task as ThisTask
...
fc04ac  bsr.l   $fc1c48                 AddTask()
...
fc04b4  move.b  #$02,15(A1)             set tc_State to RUN
fc04ba  bsr.l   $fc1600                 Remove() task from list
fc04be  andi.w  #$0000,SR               disable all interrupts (clear SR)
fc04c2  addq.b  #1,295(A6)              set SysFlag
fc04c6  jsr     -138(A6)                Permit() (task)
```
(SPG §2.9.1)

The initial task is named "exec.library" (same as the library), has a 4 KB stack, and is marked running (`tc_State = TS_RUN`) by the reset code itself. It's then removed from the ready list (because it's already running). This is the task under which the rest of boot proceeds — it is the task that calls `InitCode(RTF_COLDSTART)` below.

It is eventually terminated or morphed into another task once the CLI is running — the SPG notes "The name of the task is exec.library. It will be removed later." (SPG §2.9.1).

---

## Phase 9 — ROMTag (Resident) scan and the module table

This is where the Amiga's modular design really shows up. Instead of hard-coding which libraries and devices exist, Kickstart scans its own ROM for **ROMTag** markers — a.k.a. **Resident structures** — and initialises whatever it finds.

```
fc0500  lea     -30(PC)(=$fc04e4),A0
fc0504  bsr.l   $fc0900                 find resident structures
fc0508  move.l  D0,300(A6)              store pointer in ResModules
```
(SPG §2.9.1)

### The Resident structure

From `exec/resident.h` (Exec RKM):

```c
struct Resident {
    UWORD rt_MatchWord;      /* word to match on (ILLEGAL) — must be $4AFC */
    struct Resident *rt_MatchTag;  /* pointer to the above */
    APTR  rt_EndSkip;        /* address to continue scan */
    UBYTE rt_Flags;          /* various tag flags */
    UBYTE rt_Version;        /* release version number */
    UBYTE rt_Type;           /* type of module (NT_LIBRARY, NT_DEVICE, etc.) */
    BYTE  rt_Pri;            /* initialization priority */
    char *rt_Name;           /* pointer to node name */
    char *rt_IdString;       /* pointer to ident string */
    APTR  rt_Init;           /* pointer to init code or AUTOINIT table */
};

#define RTC_MATCHWORD   0x4AFC
#define RTF_AUTOINIT    (1 << 7)
#define RTF_COLDSTART   (1 << 0)
```

The match word `$4AFC` is not random: it's the opcode for `ILLEGAL`, so if the CPU accidentally executes a Resident struct it traps immediately instead of running into trouble. The scanner looks for:

1. Word at some even offset equal to `$4AFC`.
2. Immediately followed by a longword equal to the address of that `$4AFC`. This second check is what prevents false positives from raw data that happens to contain the match word.

(SPG §2.9.2 and Exec RKM `resident.h`.)

If both checks pass, the struct is a real ROMTag and its address is added to a table. The scanner then advances to `rt_EndSkip` to continue searching — `rt_EndSkip` usually points to the end of the Resident structure's owning module, so the scanner doesn't re-scan code that's already part of the module.

### Where does the scan look?

Kickstart scans the ROM region(s) for match words. Under 1.x these are `$FC0000–$FFFFFF` and `$F00000–$F7FFFF`; under 2.x+ they include `$F80000–$FFFFFF`.

The SumKickData Autodoc states:

> "The current list of ROM-tags is contained in the `ResModules` field of ExecBase. By default this list contains any ROM-tags found in the address ranges `$FC0000-$FFFFFF` and `$F00000-$F7FFFF`." (exec.library/SumKickData Autodoc)

After the scan, the `ResModules` field in ExecBase contains a pointer to a longword-array of Resident* pointers, null-terminated, priority-sorted.

### Additional ROMTags via KickMemPtr/KickTagPtr

After the ROM scan, Kickstart checks `KickMemPtr` and `KickTagPtr` and if their `KickCheckSum` is valid:

- `AllocAbs()`s each MemEntry to reserve its memory region.
- Merges the additional Resident pointers from `KickTagPtr` into the `ResModules` array.

This is how user-installed reset-proof modules get included in the module table on every reset.

---

## Phase 10 — CoolCapture; InitCode(COLDSTART)

```
fc050c  bclr    #1,$bfe001              turn LED on (is already on)
fc0514  move.l  46(A6),D0               get CoolCapture
fc0518  beq.s   $fc051e                 branch if not set
fc051a  move.l  D0,A0                   CoolCapture to A0
fc051c  jsr     (A0)                    jump
fc051e  moveq   #$01,D0                 set startClass = RTF_COLDSTART
fc0520  moveq   #$00,D1                 set Version = 0
fc0522  bsr.l   $fc0afo                 process InitCode(), resident structures
```
(SPG §2.9.1)

1. **Power LED back on** (CIA-A PRA bit 1 cleared = LED bright). This is the signal that very early boot is complete.
2. **CoolCapture called** via `JSR` — so it *can* use the stack and return normally. By this time the stack, memory, exception table, ExecBase, and exec.library itself all exist.
3. **`InitCode(RTF_COLDSTART, 0)`** — walk every ROMTag in `ResModules` and initialise it.

### What InitCode does

From the Exec autodoc:

> "Initialize all resident modules with the given startClass and with versions equal or greater than that specified. Modules are initialized in a prioritized order. Resident modules are used by the system to pull all its parts together at startup. Resident tags are also found in disk based devices and libraries." (exec.library/InitCode Autodoc)

The disassembly of `InitCode()` in the SPG at `$FC0AF0`:

```
FC0AF0  MOVEM.L D2-D3/A2,-(A7)      Reserve register
FC0AF4  MOVEA.L 300(A6),A2          Pointer to ResModules
FC0AF8  MOVE.B  D0,D2               StartClass to D2
FC0AFA  MOVE.B  D1,D3               Version to D3
FC0AFC  MOVE.L  (A2)+,D0            Get pointer from table
FC0AFE  BEQ.S   $FC0B22             Branch at end mark
FC0B00  BGT.S   $FC0B0A             Branch if positive (normal Resident*)
FC0B02  BCLR    #31,D0              Bit 31 set means "link to another table"
FC0B06  MOVEA.L D0,A2               Follow link
FC0B08  BRA.S   $FC0AFC             Continue scanning
FC0B0A  MOVEA.L D0,A1               Resident* in A1
FC0B0C  CMP.B   11(A1),D3           rt_Version >= required version?
FC0B10  BGT.S   $FC0AFC             Skip if too old
FC0B12  MOVE.B  10(A1),D0           rt_Flags
FC0B16  AND.B   D2,D0               match startClass?
FC0B18  BEQ.S   $FC0AFC             Skip if wrong class
FC0B1A  MOVEQ   #0,D1               clear seglist
FC0B1C  JSR     -102(A6)            InitResident(resident, NULL)
FC0B20  BRA.S   $FC0AFC             loop
FC0B22  MOVEM.L (A7)+,D2-D3/A2      restore registers
FC0B26  RTS
```
(SPG §2.9.2)

So `InitCode(RTF_COLDSTART, 0)`:

1. Walks the `ResModules` array in priority order.
2. For each entry, checks `rt_Flags & RTF_COLDSTART` (the first call passes `RTF_COLDSTART = 1` as `startClass`).
3. For matches, calls `InitResident(resident, segList)` with `segList=NULL`.

### InitResident

```
FC0B28  BTST    #7,10(A1)           Test RTF_AUTOINIT in rt_Flags
FC0B2E  BNE.S   $FC0B3C             If set, branch
FC0B30  MOVEA.L 22(A1),A1           Else: fetch rt_Init (direct code ptr)
FC0B34  MOVEQ   #0,D0               D0 = 0
FC0B36  MOVEA.L D1,A0               A0 = segList
FC0B38  JSR     (A1)                call rt_Init directly
FC0B3A  BRA.S   $FC0B7E             done
FC0B3C  MOVEM.L A1-A2,-(A7)
FC0B40  MOVEA.L 22(A1),A1           A1 = rt_Init (table ptr)
FC0B44  MOVEM.L (A1),D0/A0-A2       Load 4 longwords:
                                    ; D0 = dataSize
                                    ; A0 = function table
                                    ; A1 = struct init table
                                    ; A2 = library init function
FC0B48  JSR     -84(A6)             call MakeLibrary()
FC0B4C  MOVEM.L (A7)+,A0/A2
FC0B50  MOVE.L  D0,-(A7)            save library base
FC0B52  BEQ.S   $FC0B7C             if 0, bail
FC0B54  MOVEA.L D0,A1               lib base
FC0B56  MOVE.B  12(A0),D0           rt_Type
FC0B5A  CMPI.B  #3,D0               NT_DEVICE?
FC0B5E  BNE.S   $FC0B66
FC0B60  JSR     -432(A6)            AddDevice()
FC0B64  BRA.S   $FC0B7C
FC0B66  CMPI.B  #9,D0               NT_LIBRARY?
FC0B6A  BNE.S   $FC0B72
FC0B6C  JSR     -396(A6)            AddLibrary()
FC0B70  BRA.S   $FC0B7C
FC0B72  CMPI.B  #8,D0               NT_RESOURCE?
FC0B76  BNE.S   $FC0B7C
FC0B78  JSR     -486(A6)            AddResource()
FC0B7C  MOVE.L  (A7)+,D0            return library base
FC0B7E  RTS
```
(SPG §2.9.2)

So the semantics of `InitResident` depend on `RTF_AUTOINIT`:

- **`RTF_AUTOINIT` set**: `rt_Init` points to a 4-longword AUTOINIT table (dataSize, function table, struct-init table, init function). Exec calls `MakeLibrary(vectors, structure, init, dataSize, segList)` which allocates the library base, writes the function jump table, runs `InitStruct` to fill in the base data area, calls the init function, then *based on `rt_Type`* calls `AddLibrary()` (NT_LIBRARY), `AddDevice()` (NT_DEVICE), or `AddResource()` (NT_RESOURCE) to insert the new object into the system lists.
- **`RTF_AUTOINIT` clear**: `rt_Init` is a code pointer called directly with `D0=0, A0=segList=NULL`. The called routine is free to do whatever it wants; most expansion library initialisations work this way because they need custom behaviour.

This pattern — declare a Resident structure with `RTF_AUTOINIT`, point `rt_Init` at a 4-longword table, and let Exec do everything — is the standard way a library declares itself at boot. Application developers adding a library to the system via `SYS:Libs` use exactly the same mechanism, except that `LoadSeg()` loads the code into RAM first.

### Example: expansion.library's Resident structure

The SPG decodes one verbatim from Kickstart 1.2 ROM at `$FC4AFC`:

```
struct Resident {
    UWORD rt_MatchWord;     = $4AFC
    struct Resident *rt_MatchTag;  = $FC4AFC
    APTR  rt_EndSkip;       = $FC516C
    UBYTE rt_Flags;         = %10000001   ; RTF_AUTOINIT | RTF_COLDSTART
    UBYTE rt_Version;       = 33          ; V33 (Kickstart 1.2)
    UBYTE rt_Type;          = 09          ; NT_LIBRARY
    BYTE  rt_Pri;           = 110         ; high priority (comes up early)
    char *rt_Name;          = $FC4BEE → "expansion.library"
    char *rt_IdString;      = $FC4B16 → "expansion 33.121 (4 May 1986)"
    APTR  rt_Init;          = $FC4B38    ; AUTOINIT table
};
```
(SPG §2.9.2)

expansion.library has `rt_Pri = 110`, which makes it one of the very first things initialised — because every subsequent library needs expansion to be up to know what boards are configured.

### Priority ordering

ROMTags are initialised in priority order, highest priority first. The ROM scan builds the table sorted by `rt_Pri`. The source map doesn't list exact priorities for every resident module, but the order is roughly:

1. **exec.library** itself (already built).
2. **expansion.library** (`rt_Pri = 110`).
3. Resources: `cia.resource`, `misc.resource`, `disk.resource`, `potgo.resource`, `battclock.resource`, `filesystem.resource`, etc.
4. **graphics.library**.
5. **layers.library**.
6. **intuition.library**.
7. **dos.library** (loaded from floppy's bootblock on floppy-boot; initialised from ROM on 1.3+).
8. **strap** — this is the module that actually does the floppy boot dance (see Phase 15).

The exact set differs by Kickstart version. **Note**: The corpus does not give a single authoritative priority table per Kickstart version.

---

## Phase 11 — Autoconfig: expansion.library, Zorro II/III at $E80000

When expansion.library's init code runs, it runs the autoconfig protocol.

### The protocol

From the A500/A2000 TRM §Auto Configuration and §Expansion Bus:

> "Upon reset, all PICs come up in the unconfigured state. In the unconfigured state, the PIC responds to the 64 kilobyte address space starting at location `E80000`, if `CONFIGIN*` is active to the PIC. If `CONFIGIN*` is not active, the PIC does not respond to any bus cycles."

And:

> "During the autoconfiguration process, an unconfigured PIC responds to the 64 K address space starting at `$E80000` if its CFGIN signal is asserted. All unconfigured PICs come up [with] CFGOUT negated. When configured, or told to 'shut up', the PIC will assert CFGOUT, which results in the `$CFGIN` of the next slot to be asserted. On-board logic automatically passes on the state of the previous CFGOUT to the next CFGIN for any slot not occupied by a PIC, so there's no need to sequentially populate the Expansion Bus Slots." (A500/A2000 TRM §Configuration Chain)

This means at any given time, exactly **one** unconfigured card is responding at `$E80000`. The software:

1. Reads the ID nibbles at `$E80000–$E80040` to build an `ExpansionRom` structure (16 bytes).
2. Decides where in the 68000 address space to put the card.
3. Writes the base-address register at `$E80048/$E8004A` which latches the card at its new address.
4. The card's `CFGOUT` goes high, which asserts the next card's `CFGIN`, which causes that card to appear at `$E80000`.
5. Go to 1 until no more cards respond at `$E80000`.

### The ID format

Each of the 64 ID locations in the card is a nibble; high nibble is read from `D15–D12`, low nibble from the following byte (or read on the same byte, depending on card width). **Most nibbles are inverted** (read-back is the complement of the intended value) — a cost-saving in active-low PALs. Locations `$00`, `$02`, `$40`, `$42` are *not* inverted.

The key fields (A500/A2000 TRM §Address Specification Table):

| Reg offset | Field | Meaning |
|---|---|---|
| `$00/$02` | `er_Type` | Board type (top 2 bits = 11 for "current style"), mem-in-free-list bit, ROM-valid bit, memory-size code (3 bits). |
| `$04/$06` | `er_Product` | Product number (manufacturer-assigned). |
| `$08/$0A` | `er_Flags` | "Can shut up" flag, "prefer 8 Meg space" flag. |
| `$10..$16` | `er_Manufacturer` | CBM-assigned manufacturer ID (2 bytes). |
| `$18..$26` | `er_SerialNumber` | Serial number (4 bytes). |
| `$28..$2E` | `er_InitDiagVec` | Offset from board base to DiagArea (2 bytes). |
| `$40/$42` | Control/status | Write: interrupt enable, local reset, user bits. Read: interrupt pending. |
| `$48/$4A` | Base address | Write-only: top 8 bits of A23–A16 for the board's base. |
| `$4C/$4E` | "Shut up" | A write here tells the board to stop responding entirely until reset. |

Memory-size encoding (bits 0–2 of `er_Type`):

- 000 = 8 MB
- 001 = 64 KB
- 010 = 128 KB
- 011 = 256 KB
- 100 = 512 KB
- 101 = 1 MB
- 110 = 2 MB
- 111 = 4 MB

Alignment rules (A500/A2000 TRM §Auto Config Notes):

- ≤1 MB cards land on their natural-size boundary.
- 4 MB cards can land on 4 MB boundaries *or* at `$200000` or `$600000`.
- 8 MB cards can land on 8 MB boundaries *or* at `$200000`.
- Note: the 8 MB Zorro II range is `$200000–$9FFFFF`.

### expansion.library's job

expansion.library's init function (called early in InitCode, very high priority) reads the autoconfig chain and populates `eb_BoardList` (private) and, later, `eb_MountList` (public). Autodoc for `ConfigChain()`:

> "First off, the expansion library goes out and configures the expansion boards in the system. It puts each board in its own address space, and links memory boards into the memory free pool. This is done by the expansion.library's ConfigChain entry point. This code is intended to be run early on in system startup, before any other code is around." (A500/A2000 TRM §Driver Documentation)

Memory boards (those with the "link into memory free list" bit set in `er_Type`) are added to the system MemList via `AddMemList()`, thereby becoming part of AllocMem's pool. This is the point at which autoconfig fast RAM becomes usable.

### AddBootNode / eb_MountList

expansion.library maintains a priority-sorted `eb_MountList` of `BootNode` structures. Each BootNode has:

```c
struct BootNode {
    struct Node bn_Node;
    UWORD       bn_Flags;
    APTR        bn_DeviceNode;    /* points to a DOS DeviceNode */
};
```
(Mapping, BootNode structure; 1990 RKM §Expansion Library)

Anything bootable (floppies, hard drives, network drives) puts a BootNode on this list with an appropriate priority:

- `+5`: internal floppy DF0: (so floppy can override hard disk boot)
- `0`: typical hard disk
- `-5`: network disk
- `-128`: "don't bother booting from this"

(A500/A2000 TRM §expansion.library/AddDosNode)

---

## Phase 12 — DiagArea, ROM drivers on expansion boards, DAC_CONFIGTIME

Expansion boards can carry their own driver ROMs. This is how hard disk controllers come up before DOS exists.

### The ExpansionRom DIAGVALID bit and er_InitDiagVec

From 1990 RKM §Expansion Library:

> "When your AUTOCONFIG hardware board is configured by the expansion initialization routine, its ExpansionRom structure is copied into the ExpansionRom subfield of a ConfigDev structure. This ConfigDev structure will be linked to the expansion.library's private list of configured boards.
>
> After the board is configured, the `er_Type` field of its ExpansionRom structure is checked. The DIAGVALID bit set declares that there is a valid DiagArea (a ROM/diagnostic area) on this board. If there is a valid DiagArea, expansion next tests the `er_InitDiagVec` vector in its copy of the ExpansionRom structure. This offset is added to the base address of the configured board; the resulting address points to the start of this board's DiagArea."

### DiagArea structure

```c
struct DiagArea
{
    UBYTE da_Config;     /* DAC_WORDWIDE/BYTEWIDE/NIBBLEWIDE | DAC_CONFIGTIME/BINDTIME */
    UBYTE da_Flags;
    UWORD da_Size;       /* how many bytes to copy from ROM to RAM */
    UWORD da_DiagPoint;  /* offset to Diag init routine */
    UWORD da_BootPoint;  /* offset to Boot routine */
    UWORD da_Name;       /* offset to device name */
    UWORD da_Reserved01;
    UWORD da_Reserved02;
};

/* da_Config bus-width bits */
#define DAC_BUSWIDTH    0xC0
#define DAC_NIBBLEWIDE  0x00
#define DAC_BYTEWIDE    0x40    /* invalid in 1.3 */
#define DAC_WORDWIDE    0x80

/* da_Config "when to boot" bits */
#define DAC_BOOTTIME    0x30
#define DAC_NEVER       0x00
#define DAC_CONFIGTIME  0x10
#define DAC_BINDTIME    0x20
```
(1990 RKM §Expansion Library)

The `DAC_CONFIGTIME` bit means "run `da_BootPoint` as soon as the board is configured, before DOS comes up". `DAC_BINDTIME` means "run it later when BindDrivers is called". The sample hard-disk autoboot driver in the 1990 RKM uses `DAC_WORDWIDE + DAC_CONFIGTIME`.

### The copy-to-RAM step

If `DAC_CONFIGTIME` is set and `da_BootPoint != 0`:

1. expansion.library allocates `da_Size` bytes of public RAM.
2. Copies `da_Size` bytes from the start of the DiagArea structure (including the ROMTag, driver code, device names, and boot/diag routines) into that RAM block. The copy is "nibblewise" or "wordwise" depending on `DAC_BUSWIDTH` — nibblewise is for 8-bit-wide ROMs where the high nibble of each word carries one byte of the driver image.
3. Stores the ULONG RAM address into the `er_Reserved0c/0d/0e/0f` bytes of the ConfigDev's copy of the ExpansionRom structure (reinterpreted as a single longword).
4. Calls the `da_DiagPoint` routine (which is now in RAM) with:
   ```
   D0 = success flag (return value)
   A0 = base of board
   A2 = base of diag/init area in RAM
   A3 = board's ConfigDev
   A5 = ExpansionBase
   A6 = ExecBase
   A7 = at least 2K of stack
   ```
5. The diag routine is responsible for **patching** relative pointers in the RAM copy into absolute addresses, because when the DiagArea was written, the authors didn't know where in RAM it would live. Typical patching: walk a word-offset table, adding either the RAM base or the ROM base to each offset to turn a relative pointer into an absolute one (sample code in 1990 RKM §Expansion Library).

If `da_DiagPoint` returns non-zero (success), the RAM image persists. If zero, the RAM is freed.

### Why this matters for boot

The DiagArea mechanism allows a hard disk controller board to:

1. Expose a ROMTag (Resident structure) inside the RAM-copied image.
2. Have that ROMTag discovered by InitCode's scan (see Phase 13 below — expansion code can inject these into the ROM scan).
3. Have its driver library (for example `scsi.device`) initialised like any other ROM-based library.
4. Eventually have its `da_BootPoint` called at boot time to actually bootstrap the disk.

### BindDrivers vs DAC_CONFIGTIME

There are two styles:

- **DAC_CONFIGTIME** drivers — ROM-resident, self-contained, run at boot before DOS. Used by autoboot hard disk controllers.
- **BindDrivers** drivers — disk-resident, stored in `SYS:Expansion` as executables + `.info` files with `PRODUCT=<mfg>/<prod>` tooltypes. Bound to boards *after* DOS is up, during the startup-sequence, by the `BindDrivers` command.

BindDrivers walks `SYS:Expansion/*.info`, reads the tooltypes, finds boards that match, `LoadSeg`'s the driver code, searches the first hunk for a Resident structure, and calls `InitResident()` (A500/A2000 TRM §Driver Documentation):

> "1. GetDiskObject() on this file. If not a workbench object, return.
> 2. FindToolType() for PRODUCT definition. If not, return.
> 3. If the description matches an unconfigured board, link them and record in a static area.
> 4. LoadSeg() the code file. If LoadSeg fails, return.
> 5. Search the first hunk for a Resident structure. If no structure, UnLoadSeg() the segment.
> 6. InitResident() the loaded code."

---

## Phase 13 — ROMTag INIT time for expansion board drivers

After expansion.library has enumerated boards, copied DiagAreas, and run diag-point patching, the system is at the point where *most resident system modules (for example graphics) are initialized*. During this phase (1990 RKM §Expansion Library, "Events At ROMTAG INIT Time"):

> "As part of the system initialization procedure a search is made of the expansion.library's private list of boards (which contains a ConfigDev structure for each of the AUTOCONFIG hardware boards). If the `cd_Flags` specify CONFIGME and the `er_Type` specifies DIAGVALID, the system initialization will do three things:
>
> First, it will set the current ConfigDev as the current binding (see the expansion.library SetCurrentBinding() function). Second, it will check the DiagArea's da_Config flag to make sure that the CONFIGTIME bit is set. Third, it will search the ROM 'image' associated with this hardware board for a valid Resident structure (<exec/resident.h>); and, if one is located, will call InitResident() on it, passing a NULL segment list pointer as part of the call."

The driver's ROMTag init function is responsible for:

1. `GetCurrentBinding()` to find which ConfigDev it's being initialised for.
2. Clear the `CDB_CONFIGME` bit in `cd_Flags` so the driver isn't called again.
3. Install its Exec node in `cd_Driver`.
4. Create a `BootNode` via `MakeDosNode()`.
5. `Enqueue()` the BootNode onto `eb_MountList` with an appropriate priority.

When DOS eventually comes up, it will walk `eb_MountList` and boot from the highest-priority node.

---

## Phase 14 — Other resident libraries and devices come up

Once expansion.library and the expansion-board drivers are initialised, `InitCode(RTF_COLDSTART, 0)` continues walking the priority-sorted ResModules list. In rough order (1.x — exact priorities not enumerated in the corpus):

1. **cia.resource** — provides arbitrated access to the CIA timers and interrupt control registers. Uses `AddResource()` at init (SPG §2.9.2 — `JSR -486(A6)` for resources).
2. **misc.resource** — provides arbitration for the serial and parallel port hardware and misc system-owned resources.
3. **disk.resource** — provides arbitrated access to the disk hardware (DMA, floppy control via CIA-B PRB).
4. **potgo.resource** — arbitrates the POTGO register for game port use.
5. **graphics.library** — the big one. Opens, initialises the shared data (fonts, default color tables, views), and calls `AddLibrary()`. After graphics.library is up, any other library/device can call into graphics via `OpenLibrary("graphics.library", version)`.
6. **layers.library** — depends on graphics.
7. **intuition.library** — depends on graphics and layers.
8. **keyboard.device**, **input.device**, **gameport.device**, **audio.device**, **trackdisk.device**, **timer.device**, **console.device**, etc. — each `AddDevice()`'d into DeviceList.
9. **expansion-board drivers** that were discovered via DiagArea — for example an A2090 hard disk controller's `scsi.device`.
10. **dos.library** — initialised last among the system libraries, because DOS depends on intuition (for requesters) and trackdisk/scsi (for actual I/O).
11. **strap** — *strap* is another ROMTag; it is the one that actually runs the "try to boot" logic (see next phase).

### How libraries advertise themselves

`AddLibrary()` (called from InitResident for RTF_AUTOINIT libraries) puts the library into `LibList`. `OpenLibrary("name", version)` later walks this list, returning a pointer to the base of any library whose name matches and whose `lib_Version >= version`. A newly-opened library has its `lib_OpenCnt` incremented.

### MakeLibrary / MakeFunctions

`MakeLibrary(vectors, structure, init, dSize, segList)` (exec.library/MakeLibrary Autodoc):

> "This function is used for constructing a library vector and data area. The same call is used to make devices. Space for the library is allocated from the system's free memory pool. The size fields of the library are filled. The data portion of the library is initialized. Init may point to a library specific entry point, or NULL if no call is to be made."

Parameters:

- `vectors` — an array of function pointers, or a `-1`-prefixed array of 16-bit displacements relative to the vectors base. The array is terminated by `-1` (matching the displacement or pointer word width).
- `structure` — an `InitStruct` data region (see `InitStruct` Autodoc) that describes how to initialise the data area behind the library base.
- `init` — called after the base is allocated and the data area is initialised, with `D0 = libBase`, `A0 = segList`. Returns the final library base, which is what MakeLibrary returns. Wrapped in a `Forbid()/Permit()` pair.
- `dSize` — size of the library data area (including the standard `Library` node).
- `segList` — AmigaDOS seglist, used later by RemoveLibrary/ExpungeLibrary. NULL for ROM libraries.

---

## Phase 15 — Strap, the "bootme" hand, and floppy/autoboot selection

After everything else is initialised, **strap** runs. It is responsible for finding a bootable disk and transferring control to it.

From the 1990 RKM §Expansion Library, "Events At BOOT Time":

> "If there is no boot disk in the internal floppy drive, the system strap module will call a routine to perform autoboot. It will examine the `eb_MountList`; find the highest priority BootNode structure at the head of the List; validate the BootNode; determine which ConfigDev is associated with this BootNode; find its DiagArea; and call its `da_BootPoint` function in the ROM 'image' to bootstrap the appropriate DOS. Generally, the BootPoint code of a ROM driver will perform the same function as the boot code installed on a floppy disk, i.e., it will FindResident() the dos.library, and jump to its RT_INIT vector. The da_BootPoint call, if successful, should not return.
>
> If a boot disk is in the internal floppy drive, the system strap will Enqueue() a BootNode on the `eb_MountList` for DF0: at the suggested priority (see the Autodoc for the expansion.library AddDosNode() function). Strap will then open AmigaDOS, overriding the autoboot. AmigaDOS will boot from the highest priority node on the `eb_MountList` which should, in this case, be DF0:. Thus, games and other bootable floppy disks will still be able to obtain the system for their own use.
>
> In the event that there is no boot disk in the internal floppy drive and there are no ROM bootable devices on the autoconfiguration chain, the system does the normal thing, asking the user to insert a Workbench disk, and waiting until its request is satisfied before proceeding."

### The bootme hand

When there's no bootable device, Strap displays the famous **hand holding a Workbench disk** animation. The corpus references it (A500/A2000 TRM §AddDosNode, and 1990 RKM Autodocs):

> "If no disk is found then the 'bootme' hand will come up and the bootstrap code will wait for a floppy to be inserted." (A500/A2000 TRM §AddDosNode; Autodocs/Expansion)

**The corpus does not describe how the hand is rendered.** Intuition is not yet initialised in the sense that Workbench screens don't exist. What is clearly implied is that the hand is drawn directly to a minimal bitplane using graphics.library primitives on a simple 1- or 2-bitplane screen. Emulators that only care about booting from floppy can ignore rendering entirely and wait for a disk insertion event; emulators that want the animation need to extract the hand bitmap from a Kickstart ROM (it lives as a small image hunk in the strap module).

### The coloured reset screens

During boot, the screen background is at `COLOR00 = $0444` (grey) most of the time. Specific error paths set `COLOR00` to distinctive values for visual debugging:

```
fc0238  move.w  #$00c0,D0               screen color for reset
fc023c  bra.l   $fc05b8                 hard reset (flash LED 11 times)
```
(SPG §2.9.1 — hard-reset path)

`$00c0` is a dark red. The traditional associations (corpus is partial on this — the complete list is not in these PDFs):

- **Grey** (`$444`) — normal early Kickstart, memory test, chip init.
- **Red** (`$c00`, or `$0c0` equivalent) — hardware/memory test failed, hard reset path taken.
- **Green** (`$0c0`) — not explicitly documented in the corpus for a boot-time meaning.
- **Yellow** (`$cc0`) — not explicitly documented in the corpus.
- **Black** — a "dead end" exception occurred before the screen was even initialised.

**Not covered in corpus**: the complete canonical mapping of boot screen colours to failure modes. `DisplayAlert(RECOVERY_ALERT, ...)` and `DisplayAlert(DEADEND_ALERT, ...)` (Mapping §DisplayAlert) are the post-boot alert mechanism (the "Guru Meditation" screen), which only works once intuition.library is up.

The actual Alert constants (`AN_ExecLib = $01000000`, `AN_BaseChkSum = $81000002`, etc. — Exec RKM `exec/alerts.h`) encode the subsystem, severity, and specific error; each is either `AT_DeadEnd = $80000000` (red screen, no recovery) or recoverable (yellow).

### LED flashing as pre-screen diagnostic

Before the display is even safe to touch, Kickstart communicates errors via the power LED (CIA-A PRA bit 1):

```
fc0238  move.w  #$00c0,D0               screen color for reset
fc023c  bra.l   $fc05b8                 hard reset (flash LED 11 times)
```

The flash count encodes an error category. The corpus doesn't give a full mapping — only the "flash LED 11 times" case (SPG §2.9.1) — but the principle is clear: LED flashing is the "my chip RAM isn't big enough / my ROM is the wrong version" level of diagnostic, before any display is possible.

---

## Phase 16 — The floppy bootblock

When a disk is in DF0: (the internal drive), strap reads the **first two sectors** (1024 bytes total) from track 0, cylinder 0, side 0/1, sectors 0/1, into a RAM buffer, then runs the code that starts at offset 12 of the buffer.

### The bootblock format

From Exec RKM Appendix C, "The Boot Process":

> "The first two sectors are read into the system at an arbitrary position; therefore, the code MUST be PC-relative. The first three longwords are as in devices/bootblock.h. The type should be BBID_DOS; the checksum must be correct (as in additive carry wraparound sum of `$ffffffff`). Execution starts at location 12 of the sectors that were read in.
>
> The code is called with an open disk I/O request in A1 (see the TrackDisk chapter for the format of this IORequest block). The boot code is free to use it as it wishes (it may trash A1, but must not trash the IO block itself).
>
> The boot code returns two values: D0 and A0. D0 is a failure code — if it is non-zero then a system alert will be called, then the boot code falls into the debugger.
>
> If D0 is null then A0 contains the start address to jump to. The strap module will free the boot sectors, close the I/O block, do any other cleanup that is required, and jump to the location pointed to by A0."

And from `devices/bootblock.h` (RKM Includes & Autodocs):

```c
#define BOOTSECTS    2    /* 1K bootstrap */

struct BootBlock {
    UBYTE bb_id[4];       /* 4 character identifier */
    LONG  bb_chksum;      /* boot block checksum (balance) */
    LONG  bb_dosblock;    /* reserved / root block pointer */
    /* 512 * BOOTSECTS - 12 = 1012 bytes of boot code follow */
};

#define BBNAME_DOS   (('D'<<24) | ('O'<<16) | ('S'<<8))

/* bb_id values */
/* 'DOS' 0x00   OFS         'DOS\0' */
/* 'DOS' 0x01   FFS         'DOS\1' */
/* 'DOS' 0x02   OFS International */
/* 'DOS' 0x03   FFS International */
/* 'DOS' 0x04   OFS DirCache */
/* 'DOS' 0x05   FFS DirCache */
/* 'KICK'       Kickstart load disk (A1000) */
```
(Mapping, BootBlock structure; Exec RKM Appendix C; RKM Libraries & Devices §bootblock.h)

The `bb_id` four bytes identify the filesystem:

- `'DOS', 0` — Original File System (OFS)
- `'DOS', 1` — Fast File System (FFS) — introduced in 1.3
- `'DOS', 2/3/4/5` — OFS/FFS variants with International / DirCache features (post-2.0)
- `'KICK'` — A1000 Kickstart load disk

The checksum is an "additive-with-carry" checksum over the whole 1024 bytes: sum longwords, wrapping carry in, such that the result is `$FFFFFFFF` (i.e. add all longwords and the one's-complement of the current-running sum equals the recorded `bb_chksum`).

### What the boot code normally does

A standard bootblock does approximately:

```
; A1 = trackdisk IO request, A6 = ExecBase
        lea     dos_name(pc),a1
        jsr     _LVOFindResident(a6)    ; find dos.library ROMTag
        tst.l   d0
        beq.s   .fail
        move.l  d0,a0
        move.l  RT_INIT(a0),a0          ; dos's init function
        jsr     (a0)                    ; call it (or jump to it)
        moveq   #0,d0                   ; success
        rts
.fail:  moveq   #-1,d0
        rts

dos_name:       dc.b    'dos.library',0
```

Which is to say: the bootblock looks up dos.library in the ROM's resident list, and jumps to its `RT_INIT` function. This works because dos.library is resident in ROM from 1.3 onwards; under 1.0/1.1/1.2 it was loaded from disk by a more elaborate bootblock.

A non-bootable disk is detected by any of:

- `bb_id` is not `'DOS',0..5` (or `'KICK'` on A1000) — strap treats the disk as unformatted or foreign.
- Checksum over the 1024 bytes is wrong.
- The code returns non-zero in D0.

On detection failure, strap displays the "bootme hand" and waits for a different disk.

### Important constraints on bootblock code

- **Must be PC-relative** — the buffer can land at any allocated address.
- **Only 1012 bytes** of code + constants available (1024 − 12 for the header).
- **audio.device cannot be OpenDevice()ed from the bootblock under 2.0** (RKM Libraries §Release 2 Compatibility, Strap): *"Audio.device cannot be OpenDevice()ed by a boot block program. ... audio.device cannot be opened during 2.0 Strap unless InitResident()ed first. If OpenDevice() of audio.device fails during strap, you must FindResident()/InitResident() audio.device, and then try OpenDevice() again."*
- **Boot from other floppies** — under 1.3+, disks other than DF0: can provide the boot (priorities +5, -10, -20, -30) (RKM Libraries §Release 2 Compatibility, Strap).
- **No stack usage assumptions across versions** — *"Undocumented system stack and register usage at Diag and Boot time have changed."* (RKM Libraries §Release 2 Compatibility, Strap.)

### Older/broken behaviour

Under 1.0/1.1/1.2, the bootblock was expected to explicitly set up dos.library from a `LoadSeg`'d binary on disk — because dos.library itself was loaded from the Workbench disk, not resident in ROM. From 1.3 onwards, dos.library is in ROM and the bootblock is effectively a ROM-tag trampoline.

### Trackdisk details needed to read the bootblock

Reading the bootblock requires trackdisk.device, which requires the disk DMA subsystem. The relevant chip registers:

- `DSKPTH/DSKPTL` ($DFF020/$DFF022) — DMA pointer to disk buffer.
- `DSKLEN` ($DFF024) — length word. Bit 15 DMAEN, bit 14 direction (0=read, 1=write). Writing `DSKLEN` twice (once to "arm", once to commit) per the double-write rule.
- `DSKBYTR` ($DFF01A) — status/byte read.
- `DSKDAT/DSKDATR` ($DFF026/$DFF008).
- `ADKCON/ADKCONR` ($DFF09E/$DFF010) — MFM sync word control.
- `DSKSYNC` ($DFF07E) — sync word (`$4489` for standard Amiga format).

And from CIA-B PRB ($BFD100): drive select (SEL0..3), motor (MTR), side (SIDE), direction (DIR), step (STEP) (HRM Appendix F, CIA-B map).

Status lines in CIA-A PRA ($BFE001): RDY, TK0, WPRO, CHNG (HRM Appendix F, CIA-A map).

A whole-track MFM read is described in Exec RKM Appendix C:

> "The Amiga does a full track read starting at a random position on the track and going for slightly more than a full track read to assure that all data gets into the buffer. The data buffered is examined to determine where the first sector of data begins as compared to the start of the buffer. The track data is block moved to the beginning of the buffer so as to align some sector with the first location in the buffer."

Each sector is MFM-encoded with a 2-word sync pattern (`$4489 $4489`), a header block (track/sector/sectors-till-end-of-write), 16 bytes of OS-defined data, header checksum, data checksum, and 512 bytes of data (1024 MFM-encoded words). The whole track is 11 sectors.

---

## Phase 17 — dos.library, filesystem.resource, DOS bring-up

Once the bootblock has called `dos.library.RT_INIT`, dos.library initialises itself:

1. **Creates the DOS process infrastructure** — spawns a `Process` for the CLI, sets up its `pr_MsgPort`, creates the `dos.library` resident task if needed.
2. **Walks `eb_MountList`** — for each BootNode, calls `MakeDosNode()` (or uses the pre-made DosNode from the ROM driver) and adds it to the DOS device list.
3. **Opens `filesystem.resource`** — a system resource that holds loaded filesystem handlers. Alternate filesystems (e.g. Fast File System, International OFS, DirCache FFS) are loaded from disk under `L:FastFileSystem` or similar if they aren't built into Kickstart, and added to this resource.
4. **Creates an AmigaDOS Handler process for each mounted volume** — each volume gets a filesystem process (OFS or FFS depending on the DosType in the device node's environment).

### RigidDiskBlock and alternate filesystems

Hard disks can carry `RigidDiskBlock` / "hardblock" partition tables (1990 RKM §Expansion Library, "RigidDiskBlock and Alternate Filesystems"). The RigidDiskBlock is in the first 16 blocks of the disk, with rdb_ID = `'RDSK'`. It references:

- **PartitionBlocks** — one per partition, each with its own environment vector (surfaces, blocks-per-track, reserved, filesystem type `DOS\0`/`DOS\1`/custom, etc.)
- **FileSysHeaderBlocks** — contain alternate filesystem handlers as LoadSeg images, added to `filesystem.resource` at boot.
- **LoadSegBlocks** — drive-specific initialisation code (`DriveInit(lun, rdb, ior)`).
- **BadBlockBlocks** — bad-block remapping tables.

For each partition, dos.library builds a DosNode, links its DosEnvec to the partition's environment, and uses the referenced filesystem handler from `filesystem.resource` if it isn't `DOS\0` or `DOS\1`.

### BCPL legacy

The original dos.library was written in BCPL (a predecessor of C). Remnants that still matter to low-level code (AmigaDOS Manual §7.3):

- `BPTR` — BCPL pointer. A 32-bit longword pointer shifted right by 2 (because BCPL thought in 32-bit words, not bytes). To get a real byte pointer from a BPTR, shift left by 2.
- `BSTR` — BCPL string. A `BPTR` to a length-prefixed string (first byte = length, up to 255 characters).

From 1990 onwards, dos.library was rewritten in C/assembler but still exposes the BCPL-style interfaces for compatibility.

**Version difference**: *"DOS is now written in C and assembler, not BCPL. The BCPL compiler artifact which caused D0 function results to also be in D1 is gone."* (RKM Libraries §Release 2 Compatibility, DOS.) And: *"DOS now has a real library base with normal LVO vectors."* — before 2.0, dos.library had non-standard vector offsets inherited from BCPL's calling conventions.

### The initial CLI

After dos.library is up, it creates the initial CLI `Process`. This Process:

1. Opens `SYS:s/startup-sequence` as its input script.
2. Reads and executes commands one line at a time, using `LoadSeg()` for executables and `Execute()` for sub-scripts.
3. When startup-sequence finishes (reaches EOF or `ENDCLI`), the initial CLI either exits (on `ENDCLI`, leaving Workbench as the last runnable thing) or continues as the root interactive CLI.

---

## Phase 18 — The CLI, startup-sequence, LoadWB, Workbench

From the AmigaDOS Manual §*Automating the Boot Sequence*:

> "There is a file in the 's' subdirectory on your Workbench or CLI disk called `startup-sequence`. This is a script file. It contains a sequence of CLI commands that AmigaDOS performs whenever you reboot the system. Also in your Workbench disk startup-sequence are LOADWB (load the Workbench program) and ENDCLI which basically leaves the Workbench program in control."

And:

> "Note: The 2.0 startup-sequence looks for a file called `s:user-startup` and executes it if one is found. Whenever possible, place all your startup additions and assignments in a file called `s:user-startup` rather than modify the `s:startup-sequence`." (AmigaDOS Manual §Automating the Boot Sequence)

A typical 1.x startup-sequence does something like:

```
addbuffers df0: 10
copy nil: to nil:
BindDrivers
SetPatch > NIL: quiet
Version >NIL:
FailAt 21
MakeDir RAM:T RAM:Clipboards RAM:ENV
...
Assign ENV: RAM:ENV
Assign T:   RAM:T
Path ram: c: sys:utilities sys:system sys:prefs sys:wbstartup add
LoadWB
EndCLI > NIL:
```

Key commands:

- **`BindDrivers`** — walks `SYS:Expansion/*.info` looking for disk-based drivers that bind to configured expansion boards (see Phase 12).
- **`SetPatch`** — applies ROM bug fixes from disk.
- **`Assign`** — sets up logical assigns: `ENV:` (environment variables), `T:` (temporary files), `CLIPS:` (clipboards), etc.
- **`Path`** — sets the CLI command search path.
- **`LoadWB`** — loads and starts Workbench. This opens intuition.library, reads `Preferences`, and opens the Workbench screen (a single-palette screen that shows the icons and disk drawer windows).
- **`EndCLI`** — the initial CLI exits. Workbench is still running because `LoadWB` spawned it as an independent process.

At this point the user sees the familiar Workbench screen.

### LoadWB's job

LoadWB spawns the Workbench process, which:

1. Opens the Workbench screen via intuition.library's `OpenScreen()`.
2. Scans each mounted volume's root directory, reads `.info` files (icons) for each top-level drawer and project, and displays them.
3. Runs any programs in `SYS:WBStartup` (V2+).
4. Enters its event loop waiting for user input.

**The corpus does not describe the exact sequence of LoadWB internals.** It describes the user-visible effect.

---

## Cross-cutting subsystems

This section explains how the pieces relate, independent of the timeline.

### Tasks, processes, and the scheduler

From Exec RKM `exec/tasks.h`:

```c
struct Task {
    struct Node tc_Node;
    UBYTE       tc_Flags;
    UBYTE       tc_State;       /* TS_INVALID, TS_ADDED, TS_RUN, TS_READY, TS_WAIT, ... */
    BYTE        tc_IDNestCnt;   /* interrupt disable nesting */
    BYTE        tc_TDNestCnt;   /* task disable nesting */
    ULONG       tc_SigAlloc;    /* signals allocated */
    ULONG       tc_SigWait;     /* signals task is waiting for */
    ULONG       tc_SigRecvd;    /* signals received */
    ULONG       tc_SigExcept;   /* signals that cause an exception */
    UWORD       tc_TrapAlloc;
    UWORD       tc_TrapAble;
    APTR        tc_ExceptData;
    APTR        tc_ExceptCode;
    APTR        tc_TrapData;
    APTR        tc_TrapCode;
    APTR        tc_SPReg;       /* saved stack pointer */
    APTR        tc_SPLower;     /* stack lower bound */
    APTR        tc_SPUpper;     /* stack upper bound */
    VOID        (*tc_Switch)(); /* task losing CPU */
    VOID        (*tc_Launch)(); /* task getting CPU */
    struct List tc_MemEntry;    /* owned memory */
    APTR        tc_UserData;    /* per-task data */
};
```

A `Process` is just an extension of `Task` with DOS-specific fields (pr_MsgPort, pr_CIS/COS for standard I/O, pr_CurrentDir, pr_HomeDir, pr_CLI, etc.). You can think of it as "a Task that can call DOS functions."

Exec's scheduler is preemptive priority-based with round-robin within a priority. The basic rules:

- A task runs until it `Wait()`s, blocks on a semaphore, is preempted by a higher-priority task becoming runnable, or uses up its quantum (4 VBLANK periods by default).
- `TaskReady` is priority-sorted.
- `TaskWait` holds tasks waiting on signals.
- The current task is in `ExecBase.ThisTask`.
- `ThisTask` is `TS_RUN` and is *not* on `TaskReady`.
- When the scheduler preempts a task, it calls `tc_Switch`, picks the highest-priority ready task, calls the new task's `tc_Launch`, and jumps to its saved PC.

A task switch is triggered by Exec's rescheduler, which runs from the vertical blank interrupt server (because VBlank is guaranteed to fire 50/60 Hz).

### Signals, messages, and ports

**Signals** are the lowest-level IPC primitive: 32 bits per task, allocated via `AllocSignal()`, set via `Signal(task, bits)`, waited on via `Wait(sigmask)`. The signal map in `tc_SigWait` combined with `tc_SigRecvd` drives the scheduler's wait loop.

**Message ports** are built on signals: `struct MsgPort` has a list of messages and a signal bit. `PutMsg(port, msg)` puts a message on the list and `Signal()`s the port's task. `WaitPort(port)` waits on the port's signal bit and returns the first queued message.

**Devices** (Exec RKM §Exec Device I/O) build on message ports: an `IORequest` is a Message-with-extra-fields that encodes a command (READ, WRITE, MOTOR, SEEK, ...), a data buffer, a length. `DoIO(request)` sends the request to the device's message port and blocks until reply. `SendIO(request)` + `WaitIO(request)` lets the sending task do other things while waiting.

### Interrupt handling

The 68000 has 7 autovector interrupt levels. On the Amiga, only 2, 3, 4, 5, 6, 7 are wired; level 1 comes through a combined route via Paula. Actual interrupt routing is done by Paula's INTENA/INTREQ registers.

From the HRM §System Control Hardware, the Paula interrupt table:

| Hardware priority | Exec priority | Description | Paula name |
|---|---|---|---|
| 1 | 1 | Software interrupt | SOFTINT |
| 1 | 2 | Disk block complete | DSKBLK |
| 1 | 3 | Transmitter buffer empty | TBE |
| 2 | 4 | External INT2 & CIA-A | PORTS |
| 3 | 5 | Graphics coprocessor (Copper) | COPER |
| 3 | 6 | Vertical blank | VERTB |
| 3 | 7 | Blitter finished | BLIT |
| 4 | 8 | Audio channel 2 | AUD2 |
| 4 | 9 | Audio channel 0 | AUD0 |
| 4 | 10 | Audio channel 3 | AUD3 |
| 4 | 11 | Audio channel 1 | AUD1 |
| 5 | 12 | Receiver buffer full | RBF |
| 5 | 13 | Disk sync pattern found | DSKSYNC |
| 6 | 14 | External INT6 & CIA-B | EXTER |
| 6 | 15 | Special (master enable) | INTEN |
| 7 | — | Non-maskable interrupt | NMI |

(HRM §Interrupt Control Registers, Figure 7-4.)

Paula combines multiple "sources" into a small number of 68000 IPL levels. An interrupt service routine has to:

1. Be invoked via the 68000's autovector (levels 1–6 at `$64–$7C`).
2. Read `INTREQR` to determine which source(s) fired.
3. Dispatch to the per-source handler, often via Exec's `IntVects[]` table, which has 16 slots matching the 16 Paula bits.
4. Clear the source bit by writing to `INTREQ` (as `$bit | $8000` to set, `$bit` alone to clear).

Exec has two kinds of handlers per slot:

- **Interrupt handlers** — a single direct-call function, registered with `SetIntVector()`, that gets the Paula interrupt.
- **Interrupt servers** — a priority-sorted list of callbacks, added with `AddIntServer()`. Servers return a value that tells Exec whether to continue calling servers in the list. Used when multiple things can generate the same interrupt (notably INT2/PORTS for CIA-A and INT6/EXTER for CIA-B).

### CIA timers and the system tick

Timer.device uses CIA timers. Critically (RKM Libraries §Release 2 Compatibility, CIA Timers):

> "System use of CIA timers has changed. Don't assume how they're used.
> Don't depend on initial values of CIA registers.
> Don't mess with CIABase. Use cia.resource.
> If your code requires hardware level CIA timers, allocate the timers using cia.resource `AddICRVector()`!"

CIA-A can generate INT2 (PORTS), CIA-B can generate INT6 (EXTER). Each CIA has two 16-bit timers (Timer A, Timer B) and a 24-bit Time of Day (TOD) counter. The TOD on CIA-A ticks from the power line (50/60 Hz) and is used by timer.device as the slow clock. CIA-B TOD ticks from horizontal sync.

Chip rates: `.715909 MHz NTSC / .709379 MHz PAL` per timer tick (HRM Appendix F).

### The Copper

The Copper is Agnus's display coprocessor. It runs a Copper list (a stream of MOVE and WAIT instructions) synchronised to the video beam, used to change custom chip registers mid-frame (for colour changes, scroll effects, bitplane pointers, etc.).

The Copper is **not** initialised by the reset routine — the reset routine only explicitly touches DMACON, INTENA, INTREQ, BPLCON0, BPLCON2, COLOR00. The Copper DMA channel (COPEN, DMACON bit 7) is *not* enabled until graphics.library initialises it with a valid Copper list for the default view.

### The Blitter

The Blitter is enabled early (`DMACON = $8200` sets DMAEN + BLTEN in the reset routine — SPG §2.9.1), because Exec and graphics.library use blitter moves for bulk memory copies.

Blitter interrupts are on INT3 (level 3) via `BLIT` (bit 6 of INTENA).

### Memory pools and AllocMem

All memory allocation ultimately goes through `AllocMem(size, attributes)`, which walks the MemList looking for a MemHeader whose `mh_Attributes` matches the requested attributes. Each MemHeader has its own free-list (`mh_First`, a singly-linked list of `MemChunk` structures), and `AllocMem` is first-fit on that list.

`MEMF_PUBLIC` is an advisory flag — Exec uses it to know which memory is safe to share between tasks. `MEMF_CHIP` and `MEMF_FAST` are the hard constraints. Both are set by the boot code on the right MemHeaders.

`AllocAbs(address, size)` allocates at a specific absolute address, used by `KickMemPtr` entries at reset.

### The library / device / resource distinction

All three are Exec Node types, all three live in a list in ExecBase, and all three are constructed by `MakeLibrary()`:

- **Library** (`NT_LIBRARY`, added with `AddLibrary()`) — code exposed via a jump table at negative offsets from the library base. Opened with `OpenLibrary(name, version)`.
- **Device** (`NT_DEVICE`, added with `AddDevice()`) — library-plus-I/O: exposes `BeginIO`/`AbortIO` at standard offsets. Opened with `OpenDevice(name, unit, ioreq, flags)`. Commands go via message-passing to the device task.
- **Resource** (`NT_RESOURCE`, added with `AddResource()`) — library-plus-exclusivity: a thing that only one component can own at a time, e.g. the disk hardware, CIA timers, the POTGO register. Opened with `OpenResource(name)` which returns its base pointer.

### Custom chip state at different boot phases

| Phase | DMACON | INTENA | COPJMP | Screen |
|---|---|---|---|---|
| Hardware reset | 0 | 0 | undefined | undefined |
| Post `$FC00FE` | `$7FFF` cleared = all off | `$7FFF` cleared = all off | not touched | COLOR00 = $0444 grey |
| After interrupt structure init | `$8200` = DMAEN + BLTEN | `$C000` = INTEN master only | not touched | grey |
| After graphics.library init | DMAEN + BLTEN + BPLEN + COPEN + SPREN + DSKEN + audio bits | INTEN + VERTB + COPER + SOFTINT + PORTS + ... | loaded with real list | initialised View with default palette |

---

## Emulator implementation notes

The hard parts, in priority order for an emulator author:

### 1. The overlay bit

- Mutate the address decode on the exact cycle the write to `$BFE001` PRA bit 0 happens.
- Before the write: all accesses to `$000000–$07FFFF` (256K) or `$000000–$0FFFFF` (512K, depending on ROM size) go to the Kickstart ROM image.
- After the write clears OVL: those accesses go to chip RAM.
- Implement this as a flag in your memory manager, not as an alias/remap at the ROM load step. Some 68000 instructions (notably `MOVE.L`) may straddle the transition.
- On hardware or 68000 RESET instruction, the overlay comes back on. Do not wait for any particular fetch to realise this — treat the RESET instruction as "immediately re-assert OVL in hardware state, then let the CPU's prefetch queue continue executing".

### 2. The reset vector fetch

- After hardware reset or `RESET` instruction, the 68000 reads SSP from `$0` and PC from `$4`.
- Because OVL is on at that moment, both come from the ROM image.
- If you load Kickstart into a ROM buffer, the first two longwords of that buffer are what gets read.
- Do not "special-case" the reset fetch; just ensure OVL is correctly on so the normal memory decoder does the right thing.

### 3. ColdReboot and prefetch

- When the CPU executes the `RESET` instruction during `ColdReboot`, the external reset line pulses.
- The 68000 has already prefetched the next instruction (the `jmp (a0)`) before executing `reset`.
- That prefetch must execute correctly — which means your reset handling must **not** affect the CPU's prefetch queue, only the external memory/chip state.
- Without correct prefetch behaviour, `ColdReboot()` hangs on reset and emulator users report "reboot doesn't work."

### 4. CIA initial state

- DDRA = `$03` after reset (CIA-A output on bits 0 and 1).
- PRA bit 0 (OVL) = 1 after reset.
- PRA bit 1 (/LED) = 0 after reset (LED on bright).
- PRA bits 2–7 are input from disk-change, write-protect, track-0, ready, fire-buttons.
- TOD is stopped until written.
- Timer A, Timer B are stopped.
- DDRB = 0 = all inputs initially; Kickstart sets PRB direction bits later for parallel port use.
- See HRM Appendix F for the complete bit map.

Do not assume software will reset the CIAs to canonical state before using them — software may do `move.b #$03,$BFE201` as its first instruction and then clear OVL via PRA, relying on the hardware initial state. This is exactly what the SPG trace shows at `$fc00fe`.

### 5. Custom chip initial state

An emulator should treat hardware reset as "DMACON, INTENA, INTREQ all zero; COLOR00 all zero; Copper/Blitter idle; bitplane pointers undefined; sprite pointers undefined". But do *not* rely on software to stabilise them before touching them: the SPG trace explicitly writes `$7FFF` to each of DMACON, INTENA, INTREQ, so they'll be in known state by the time the rest of the reset runs.

Specific bits that are documented as reset-on-power-up (Mapping §DFF100 BPLCON0):

- `ERSY` (BPLCON0 bit 1) — external sync off.
- `LACE` (BPLCON0 bit 2) — interlace off.
- `LPEN` (BPLCON0 bit 3) — light pen off.

### 6. ExecBase at $4

- After the reset routine at `$fc027a`, absolute memory location `$00000004` contains a pointer to ExecBase.
- The first few hundred bytes of ExecBase are read by every piece of Amiga code at some point.
- The ChkBase check (`ExecBase + ChkBase == $FFFFFFFF`) and the word checksum over offsets 34–78 must both pass when the reset routine re-runs, or it will reinitialise ExecBase from scratch.
- On a soft reset where ExecBase survives, the reset skips most of the work and proceeds straight to `InitCode(RTF_COLDSTART, 0)`. This is a fast path and important for performance.

### 7. Kickstart version differences

| Difference | 1.x | 2.x+ |
|---|---|---|
| ROM size | 256K at `$FC0000` | 512K at `$F80000` |
| dos.library | Loaded from disk (1.0/1.1/1.2) or ROM (1.3) | ROM-resident |
| dos.library language | BCPL | C/asm |
| ExecBase location | Fixed near top of chip RAM / `$C00000` ranger | Moved to autoconfig fast RAM if present |
| VBR on 68010+ | Always 0 | May be non-zero |
| `FC0002` entry | Real ROM start | "Kickety-Split" compatibility stub |
| Supervisor stack location | Different between 1.x and 2.x+ | Don't hard-code |
| Workbench screen modes | Single low-res | Productivity, interlace, VGA, variable |
| Release 2 compat notes | — | "Everything has moved" (RKM Libraries §Release 2 Compatibility) |

Test any emulator against both a Kickstart 1.3 and a Kickstart 2.04+ ROM.

### 8. Memory timings and the DRAM refresh

The A500/A2000 TRM doesn't cover DRAM refresh timing in the level of detail a cycle-accurate emulator needs, but:

- Refresh is handled by Agnus as a built-in DMA slot (`REFPTR` register, $DFF028).
- Refresh uses 4 slots per scanline in the standard video timing.
- Agnus "steals" chip RAM cycles for refresh, DMA, and display — CPU accesses to chip RAM are therefore not at full CPU speed; they are contended.
- **Fast RAM is not contended** by definition.
- CPU contention is deterministic by raster position: the CPU "sees" chip RAM slower at lines where bitplane DMA is active.

A cycle-accurate emulator must model Agnus's time-multiplexed DMA scheme — this is covered only briefly in the corpus (HRM §Playfield Hardware, SPG §Chip RAM). For the **boot** process specifically, contention matters only if your boot ROM is timing-sensitive (the `$fc00d8` delay loop has to wait long enough for CIAs to be ready).

### 9. Interrupt levels and autovectors

- The 68000 hardware autovector mechanism (level N → vector at `$60 + 4*N`) must be wired up.
- The Kickstart reset routine installs a default handler in every vector, then overwrites the autovectors with specific handlers.
- Paula is the interrupt source for levels 1–6 (with CIA-A contributing to level 2 and CIA-B to level 6).
- Level 7 is NMI; the Amiga doesn't normally generate NMIs.
- Your 68000 core needs to correctly implement priority masking (interrupts ≤ current IPL are masked) and the interrupt-stacked-frame format.

### 10. Things you can fake

- The A1000 WOM path (unless you're specifically emulating an A1000).
- The guru-number capture at `$fc30c4` — you can leave `LastAlert` at zero.
- The 68010/68020/68881 detection — on an emulated 68000 it'll correctly detect as 68000 and set AttnFlags to `$00`.
- The exact contents of the "insert disk" / "bootme hand" animation — you can just freeze Strap and wait for a disk insertion event if you're not trying to be cosmetically faithful.
- Battery-backed clock (battclock.resource) — if you don't care about the realtime clock, return zero.

### 11. Things you must not fake

- The overlay bit transition.
- The prefetch behaviour around `ColdReboot`'s `reset`/`jmp` pair.
- The address decode that causes autoconfig PICs to respond at `$E80000` before configuration and at their assigned address afterwards.
- The CIA DDRA being `$03` so software writes to PRA actually land.
- The exact location of `COLOR00 = $DFF180` (it is not at an offset from DFF000; it *is* `$DFF180`).
- The fact that `$DFF000` register area is not addressed by accessing `$DFF000` alone — each register has a specific 16-bit offset.
- The chip RAM / fast RAM distinction — autoconfig fast RAM must **not** be in `MEMF_CHIP` space.

### 12. Autoconfig correctness

- Unconfigured PICs **only** respond at `$E80000–$E8FFFF` (64 KB space for the ID ROM).
- Exactly **one** PIC responds at a time (the one whose `CFGIN` is high and whose `CFGOUT` is low).
- Writing the board's `$48/$4A` (base address) latches the new base and causes the next PIC to take over at `$E80000`.
- Writing to `$4C` shuts up the current PIC.
- ID nibble reads come from D15–D12; most nibbles are inverted on read.
- A memory card with "link into memory free list" set in `er_Type` must be **added** to the MemList, not just placed in the address space.

### 13. Bootblock correctness

- Read the first two sectors (1024 bytes total) of DF0: track 0 cylinder 0 into a buffer.
- Verify `bb_id[0..3]` is `'DOS',0..5` or `'KICK'`.
- Verify the additive-carry checksum over the 1024 bytes equals `$FFFFFFFF`.
- Call `(buffer + 12)` with `A1 = open trackdisk IORequest, A6 = ExecBase`, PC-relative code.
- On `D0 == 0` return, `JMP (A0)`.
- On `D0 != 0`, call `Alert(AN_BootStrap | AN_BootError)` (`$30000001`).

---

## Gaps in the corpus

Things the emulator author needs that the ten-PDF corpus does *not* adequately cover:

1. **Exact Kickstart boot screen colour → failure code mapping.** The SPG shows `$00c0` (dark red) for hard-reset but doesn't enumerate all the failure modes and their associated COLOR00 values. Yellow and green boot screens are user-visible but not explained.
2. **"Bootme hand" rendering details.** The corpus mentions the hand exists (A500/A2000 TRM, 1990 RKM Autodocs) but doesn't describe what bitmap it is, how it's drawn, or how the animation frames are stored. You'd need to decompile strap from a real Kickstart ROM.
3. **Complete Resident priority ordering per Kickstart version.** The SPG shows expansion.library at priority 110, and you can infer rough ordering from dependencies, but there's no single table of "this is the exact boot order for Kickstart 1.3".
4. **Kickstart 2.x/3.x early reset trace.** The SPG §2.9.1 disassembles Kickstart 1.2's reset routine in detail. There's no equivalent trace of 2.04 or 3.1 in the corpus. The release-2 compatibility notes give high-level differences only.
5. **DRAM refresh timing and chip-RAM contention model at the level needed for cycle-accurate emulation.** The HRM gives the DMA allocation for bitplanes, copper, blitter, sprites, audio, disk, refresh — but doesn't give a nanosecond-level slot diagram.
6. **Exact 68000 prefetch behaviour around the `RESET` instruction.** The HRM says the RESET/JMP must share a longword and prefetch executes the JMP, but doesn't give the exact prefetch queue state diagram. For emulator correctness you may need the Motorola 68000 User's Manual.
7. **Agnus ID bit reading (detecting OCS vs ECS vs AGA).** Referenced indirectly (Mapping §AGA warning) but not the actual detection code.
8. **Fat Agnus 1 MB chip RAM mapping.** The SPG discusses 512 KB chip RAM with Kickstart 1.2; the A500 with Fat Agnus supports 1 MB at `$00000–$0FFFFF` overlapping the A2000-style hole at `$80000–$1FFFFF`. Not covered in detail.
9. **Exact Kickstart 1.x assembly for `$fc30c4` (check guru) and `$fc125c` (interrupt structure init).** The SPG refers to them but doesn't disassemble them.
10. **The hand-off from strap to dos.library — exactly which register state, which task state, how dos.library transitions from being called directly to running as an independent process.** Implied by the autodocs but not traced line by line.
11. **How Workbench screen is constructed, colour palette initial values, WBStartup enumeration.** Mapping describes the relevant structures but there's no "Workbench boot trace" in the corpus.
12. **Zorro III autoconfig.** The HRM mentions Zorro III briefly (32-bit bus, different interrupt scheme) but the A500/A2000 TRM is Zorro II only. Zorro III needs Buster (the bus controller) and a different set of rules.
13. **Exact bootblock variants for international and DirCache filesystems.** The corpus gives `DOS\0` and `DOS\1` (OFS and FFS) but the International and DirCache variants are only listed, not explained in detail.
14. **Rigid Disk Block / hardblock format beyond the basic fields.** 1990 RKM gives a partial description but not every block type.
15. **How the floppy hand animation is timed (what interrupts are serviced, what DMA is on).**

For these gaps, authoritative sources outside the corpus are: the Motorola 68000 User's Manual for CPU behaviour, `amitools`/FS-UAE source for practical emulator decisions, the Amiga Hardware Database for ROM version differences, and the Aminet docs collection for Kickstart disassemblies that supplement the SPG.

---

## Source map appendix

Full PDF filenames and what each is most useful for:

### `Amiga_Hardware_Reference_Manual_3rd_edition.txt` — "HRM"

**Authoritative for**: reset and early startup operation, custom chip registers (especially DMACON/INTENA/INTREQ/BPLCONx/COLORxx), the `ColdReboot` canonical code, Appendix F (CIA 8520 register maps, CIA-A and CIA-B pin assignments including OVL bit location), interrupt priorities and Paula's interrupt table, DMA system, Copper/Blitter/sprites hardware, Zorro II/III expansion bus pinouts. The core reference for "what the hardware does".

### `Commodore_Amiga_A500_A2000_Technical_Reference_Manual_1987_Commodore_text.txt` — "A500/A2000 TRM"

**Authoritative for**: the A500 and A2000 motherboards specifically, PAL equations for the memory decoder (including the ROM mirror logic around OVL), Zorro II expansion bus timing and pinout, autoconfig protocol in complete detail including the ID nibble table, the `BindDrivers` overview, expansion.library autodocs (AddDosNode, MakeDosNode, etc.), BGACK/OWN timing for bus arbitration, and specifically the "RES* and RESB* lines" discussion. The complement to the HRM for hardware integration.

### `Amiga_ROM_Kernel_Reference_Manual_Exec.txt` — "Exec RKM"

**Authoritative for**: `exec/execbase.h` (the ExecBase struct), `exec/resident.h` (the Resident/ROMTag struct), `exec/alerts.h` (alert number definitions), the system memory map at `F80000-FFFFFF`, Appendix C "The Boot Process" (bootblock format, MFM encoding), the conceptual description of libraries/devices/resources. First stop for "what are these structures".

### `Amiga_ROM_Kernal_Reference_Manual_Includes_and_Autodocs.txt` — "Includes & Autodocs"

**Authoritative for**: exec.library autodocs — especially `InitCode`, `InitResident`, `MakeLibrary`, `AddLibrary`, `AddDevice`, `AddResource`, `FindResident`, `SumKickData`, `SetIntVector`, `AddIntServer`, `Alert`. Also the raw include files for `exec/*.h`, `devices/*.h`, `libraries/*.h`. Go here when you need the precise calling convention for a boot-time function.

### `Amiga_ROM_Kernel_Reference_Manual_Libraries_and_Devices.txt` — "RKM Libraries & Devices"

**Authoritative for**: the high-level narrative description of each library and device, the Release 2 compatibility notes (critical for version differences — "Kickety-Split", BCPL-to-C rewrite of DOS, dos.library LVO changes, audio.device init changes in 2.0 strap, CIA timer allocation changes, 68010+ VBR, etc.). Go here for the "what changed between Kickstart versions" questions.

### `1990-beats-steve-amiga-rom-kernel-ref-3rd.txt` — "1990 RKM" (Libraries 3rd ed)

**Authoritative for**: the Expansion Library chapter in depth — DiagArea, er_InitDiagVec, the full sample autoboot code fragment showing DiagStart, Romtag, BootEntry, DiagEntry, patch tables. Also the RigidDiskBlock format, FileSystem.resource, hardblocks. The Resident structure fields (`RT_MATCHWORD`, `RTF_AUTOINIT`, `RTF_COLDSTART`, etc.). And appendix C sample autoboot code (the canonical example).

### `Amiga_System_Programmers_Guide_1988_Abacus.txt` — "SPG" (Abacus System Programmer's Guide)

**Authoritative for**: the complete disassembled trace of the Kickstart 1.2 reset routine at `$FC00D2–$FC0530` — this is the single most valuable source in the corpus for understanding what Kickstart does at reset. Also the ExecBase struct with offsets in both decimal and hex for both 512K and 1MB machines, and the resident structure worked example for expansion.library. The "Reset routine and reset-proof programs" chapter is the best in the corpus for this phase.

### `Amiga_Machine_Language_1991_Abacus.txt` — "Abacus Machine Language"

**Useful for**: 68000 register conventions, memory map overview, the "fast RAM begins at $200000" narrative. More accessible than the HRM for beginners. Good for confirming addresses but not authoritative on details.

### `1993-thomson-randy-rhett-anderson-mapping-amiga-2nd-edition.txt` — "Mapping" (Mapping the Amiga)

**Authoritative for**: the complete hardware register reference (every `$DFFxxx` and `$BFDxxx`/`$BFExxx` register with bit-level explanations), the complete system structure reference in the format most useful for emulator work (bytes-in-decimal, bytes-in-hex, field name, C type, ML name, ML type). Use this alongside the HRM when you need "what does this register bit do" for an unfamiliar register.

### `1991-baker-jesup-et-al-the-amigados-manual-3rd-ed.txt` — "AmigaDOS Manual"

**Authoritative for**: startup-sequence conventions, BCPL legacy (BPTR/BSTR), `LoadWB`, `BindDrivers`, `Assign`, dos.library autodocs. Use for the DOS side of boot, from bootblock handoff through Workbench launch.

---

*End of document. Edits and additions welcome as the emulator takes shape.*
