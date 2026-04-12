# Amiga Kickstart ROM Internals — A Deep Dive

*A companion to `amiga-boot-process.md`. Where that document traces the high-level boot flow from physical reset through Workbench, this one fills the ROM-internals gaps: per-Kickstart ROM layout, the boot-screen colour table, assembly-level reset traces for 1.3/2.04/3.x, ExecBase field-by-field construction, Resident priority ordering, the V37+ `ColdReboot()` API, cold/cool/warm capture hook protocol, expansion.library autoconfig details, strap→DOS→CLI handover, LoadWB and workbench.library internals, a complete alert-code table, and the 68000 exception wiring.*

Primary sources (cited inline):

- **Amiga Intern** (Abacus, 1992) — `Amiga_Intern_1992_Abacus.txt`. Deep ROM-level coverage of Kickstart 2.04, plus the full Resident priority table for V37.
- **Amiga Startup Routine** — `Amiga Startup Routine.txt`. The smoking-gun colour→failure-mode document.
- **RKM Libraries 3rd ed** (V37+) — `Commodore_Amiga_Tech_Ref_Series_Amiga_ROM_Kernel_Reference_Manual_Libraries_3rd_edition.txt`. Release 2 compatibility chapter; expansion.library chapter.
- **RKM Includes/Autodocs 3rd ed** — `Commodore_Amiga_Tech_Ref_Series_Amiga_ROM_Kernel_Reference_Manual_Includes_And_Autodocs_3rd_edition_[600dpi][ocr].txt`.
- **NDK 3.9 autodocs**: `exec.doc`, `wb.doc`, `expansion.doc` — V39/V45 authoritative.
- **NDK 3.9 headers**: `exec/execbase.h`, `exec/resident.h`, `exec/alerts.h`.

The existing high-level boot document (`amiga-boot-process.md`) is referenced but **not duplicated**. Where that doc covers a topic, this one points back and adds only the internals gap.

---

## Table of contents

1. [ROM layout per Kickstart version](#1-rom-layout-per-kickstart-version)
2. [Boot-screen colour → failure-mode table](#2-boot-screen-colour--failure-mode-table)
3. [Reset trace — 1.2 / 1.3 / 2.04 / 3.1](#3-reset-trace--12--13--204--31)
4. [ExecBase construction, field by field](#4-execbase-construction-field-by-field)
5. [Resident (ROMTag) init order per Kickstart](#5-resident-romtag-init-order-per-kickstart)
6. [ColdReboot() deep dive](#6-coldreboot-deep-dive)
7. [Cold/Cool/Warm-Capture hooks](#7-coldcoolwarm-capture-hooks)
8. [expansion.library boot role (V37+)](#8-expansionlibrary-boot-role-v37)
9. [Strap → dos.library → CLI handover](#9-strap--doslibrary--cli-handover)
10. [LoadWB and workbench.library startup](#10-loadwb-and-workbenchlibrary-startup)
11. [`startup-sequence` breakdown for 2.x/3.x](#11-startup-sequence-breakdown-for-2x3x)
12. [Complete alert-code table](#12-complete-alert-code-table)
13. [Exception handlers](#13-exception-handlers)
14. [Kickstart version differences summary](#14-kickstart-version-differences-summary)
15. [New gaps discovered](#15-new-gaps-discovered)
16. [Source-map appendix](#16-source-map-appendix)

---

## 1. ROM layout per Kickstart version

### 1.1 Address ranges, sizes, IDs

The `SoftVer` field of ExecBase (offset `$22`, see §4) and the ROMTag `rt_Version` in the exec resident encode the release. The values below come from `Amiga_ROM_Kernel_Reference_Manual_1987_Addison-Wesley_Publishing_Company.txt` lines 524–535 (actually the 3rd ed RKM mislabelled — it has the Release 2 tables) and `Amiga_Intern_1992_Abacus.txt` line 404.

```
SoftVer  Release name                          ROM size  Base address
-------  -----------------------------------   --------  ---------------
  30     Kickstart 1.0 (obsolete)              256 KiB   $FC0000–$FFFFFF
  31     Kickstart 1.1 NTSC (obsolete)         256 KiB   $FC0000–$FFFFFF
  32     Kickstart 1.1 PAL  (obsolete)         256 KiB   $FC0000–$FFFFFF
  33     Kickstart 1.2                         256 KiB   $FC0000–$FFFFFF
  34     Kickstart 1.3 (+autoboot)             256 KiB   $FC0000–$FFFFFF
  35     Kickstart 1.3-A2024 special           256 KiB   $FC0000–$FFFFFF
  36     Kickstart 2.0  (pre-release, A3000)   512 KiB   $F80000–$FFFFFF
  37     Kickstart 2.04 (Release 2 GA)         512 KiB   $F80000–$FFFFFF
  39     Kickstart 3.0  (AA, A1200/A4000)      512 KiB   $F80000–$FFFFFF
  40     Kickstart 3.1                         512 KiB   $F80000–$FFFFFF
  45     Kickstart 3.9  (Haage & Partner)      512 KiB + ROM update  $F80000–$FFFFFF
```

Quote (3rd ed RKM, `Amiga_ROM_Kernel_Reference_Manual_1987_Addison-Wesley_Publishing_Company.txt` lines 524–535):

> "30 Kickstart V 1.0 (obsolete) / 31 Kickstart V 1.1 (NTSC only - obsolete) / 32 Kickstart V 1.1 (PAL only - obsolete) / 33 Kickstart V 1.2 (the oldest revision still in use) / 34 Kickstart V 1.3 (adds autoboot to V33) / 35 Special Kickstart version to support A2024 high-resolution monitor / 36 Kickstart V2.0 (old version of Release 2) / 37 Kickstart V2.04 (current version of Release 2)".

The 256 KiB ROMs (1.0–1.3) live at `$FC0000–$FFFFFF`. The 512 KiB ROMs (2.x/3.x) live at `$F80000–$FFFFFF`, with the 1.3-compatible "Kickety-Split" hack re-exporting a small jump at `$FC0002` — see §3.

Release 2 Compatibility warning (RKM Libraries 3rd ed, line 51105):

> "Do not jump to location `$FC0002` — the start of the ROM under 1.3 — as part of performing a system RESET. The 2.04 Kickstart ROM has a temporary compatibility hack called 'Kickety-Split' which is a redirecting jump at `$FC0002`. This hack does not appear on the A3000 ROM and due to space considerations will not appear on future machines."

The consequence for emulator authors: the *only* supported method of programmatic reboot under V36+ is `exec.library/ColdReboot()` (see §6). Jumping to `$FC0002` happens to work on every pre-Zorro III machine with 256 KiB+256 KiB-split ROMs but is not portable and is explicitly deprecated.

### 1.2 ROMTag resident array at the top of ROM

In all Kickstart versions, ExecBase's `ResModules` field (offset `$12C`, see §4) points to a ROM-resident array of longword pointers, each pointing to a `struct Resident`. The resident structures themselves are scattered throughout ROM; the pointer array compacts them into a priority-sortable list.

From `exec/resident.h` (NDK 3.9), verbatim:

```c
struct Resident {
    UWORD rt_MatchWord;         /* word to match on (ILLEGAL)       */
    struct Resident *rt_MatchTag; /* pointer to the above           */
    APTR  rt_EndSkip;           /* address to continue scan         */
    UBYTE rt_Flags;             /* various tag flags                */
    UBYTE rt_Version;           /* release version number           */
    UBYTE rt_Type;              /* type of module (NT_XXXXXX)       */
    BYTE  rt_Pri;               /* initialization priority          */
    char  *rt_Name;             /* pointer to node name             */
    char  *rt_IdString;         /* pointer to identification string */
    APTR  rt_Init;              /* pointer to init code             */
};

#define RTC_MATCHWORD   0x4AFC  /* The 68000 "ILLEGAL" instruction */

#define RTF_AUTOINIT    (1<<7)  /* rt_Init points to data structure */
#define RTF_AFTERDOS    (1<<2)
#define RTF_SINGLETASK  (1<<1)
#define RTF_COLDSTART   (1<<0)
```

The `rt_MatchWord` value `$4AFC` is the 68000 `ILLEGAL` opcode. This is deliberate: a stray execution into a ROMTag image immediately traps instead of corrupting state.

Amiga Intern (lines 9945–9966) shows the equivalent assembler-friendly definition:

```
Dec   Hex STRUCTURE RT,0                ;residentTag/ROMTag
  0    $0 UWORD   RT_MATCHWORD          ;ILLEGAL command
  2    $2 APTR    RT_MATCHTAG           ;start of structure (RT_MATCHWORD)
  6    $6 APTR    RT_ENDSKIP            ;RT allowed starting with this addr
 10    $A UBYTE   RT_FLAGS              ;Flags
 11    $B UBYTE   RT_VERSION            ;version
 12    $C UBYTE   RT_TYPE               ;module type (NT_...)
 13    $D BYTE    RT_PRI                ;initialization priority
 14    $E APTR    RT_NAME               ;module name
 18   $12 APTR    RT_IDSTRING           ;identification string
 22   $16 APTR    RT_INIT               ;initialization routine/data
 26   $1A LABEL   RT_SIZE

RTC_MATCHWORD = $4AFC

RTB_COLDSTART  = 0, RTF_COLDSTART  = 1  ;Init from reset
RTB_SINGLETASK = 1, RTF_SINGLETASK = 2  ;task
RTB_AFTERDOS   = 2, RTF_AFTERDOS   = 4  ;Init after DOS
RTB_AUTOINIT   = 7, RTF_AUTOINIT   = $80 ;RT_INIT = data
```

Self-validation works by walking forward from the ROM base. Exec scans every even longword looking for the `MATCHWORD` value; when found it verifies that `rt_MatchTag` points back to that exact location (so a stale `$4AFC` appearing as data in a completely different structure cannot be mistaken for a ROMTag), reads `rt_EndSkip` to know where the tag's body ends, and resumes scanning from there. The `FindResident` autodoc (`exec.doc/FindResident`) documents the runtime-visible entry:

```
exec.library/FindResident
  FindResident - find a resident module by name
  resident = FindResident(name) -- D0 result, A1 name
  FUNCTION
    Search the system resident tag list for a resident tag ("ROMTag") with
    the given name. [...]
    Resident modules are used by the system to pull all its parts
    together at startup. Resident tags are also found in disk based
    devices and libraries.
```

The initial scan, however, is not done via `FindResident` — it is done earlier, inside Exec's private ROM init code, to build `ExecBase->ResModules` in the first place. See §3 and §5.

### 1.3 ROM header layout (both 256 KiB and 512 KiB variants)

A Kickstart ROM image has this shape, in order from the base address:

```
base+$0000  dc.l  initial SSP (same value as will land in $0 after overlay)
base+$0004  dc.l  reset PC    (same value as will land in $4 after overlay)
base+$0008  ... early boot code ...

(at base+$0002, a few bytes in, the V2.04 "Kickety-Split"
 redirect lives at what used to be the 1.3 ROM base $FC0002 —
 i.e. physical $FC0002 of the 2.04 256-KiB-image-within-a-512-KiB
 aliased region)

...                        -- exec private init routines
(near top of ROM)          -- ROMTag array (ResModules seed)
top-16 bytes               -- ROM ID / checksum / size (observed by tools
                              but not mandated by Kickstart 1.x docs;
                              Kickstart 2.04+ places a recognisable
                              signature here which bootable-ROM-image
                              tools like "romsum" walk.)
```

Exec validates the ROMTag list for self-consistency but there is **no** strict single "ROM checksum" field in Kickstart 1.x. The document "Amiga Startup Routine" line 24 says only:

> "Do a checksum test on all ROMS."

— meaning the early startup code computes a longword-add-and-compare across the full ROM region. If it fails, the machine drops the screen to **red** (see §2). Amiga Intern adds no more detail about the algorithm; it is the XOR/sum-to-zero trick that every 68k ROM has ever used.

### 1.4 ROM base detection via SysBase

Runtime code that wants to know where Kickstart lives reads `SysBase->ChkBase` at offset `$26`. That field holds the one's-complement of `SysBase`, not the ROM base — but `SysBase->ResModules` at offset `$12C` points into ROM as soon as init is done, and a walk of that array gives both the address and the `rt_IdString` for every ROM-resident module. The Amiga Intern ROMTag dump on pages 404–445 (transcribed in §5) was generated exactly that way: walk `ExecBase->ResModules`, print `rt_Name`, `rt_Pri`, dereference `rt_IdString`.

### 1.5 The "soft-reset-surviving RAM image" (`kickrom` in RAM)

Amigas with extremely early ROMs (the A1000 in particular) loaded Kickstart from a boot diskette into a write-protected RAM region at `$F80000`, then held the MMU-equivalent write-protect line through reset. `Amiga_Intern_1992_Abacus.txt` lines 44854–44856 describe it for the A3000:

> "When the first A3000's were manufactured, the Kickstart had to be booted from the hard disk. In place of the Kickstart, a boot program was placed in ROM. After the Kickstart was loaded to fast RAM, it would be [...]"

For RAD:/RAM-disk survival across reset, the mechanism is different and is documented through the `KickMemPtr`/`KickTagPtr`/`KickCheckSum` triad in ExecBase — see §4 and §7. RAD: itself uses those three fields to reinstate itself after a warm reset.

---

## 2. Boot-screen colour → failure-mode table

This is the first-pass gap-fill. The primary source is the tiny but on-topic `Amiga Startup Routine.txt`, which is reproduced almost verbatim here because it is short and authoritative:

### 2.1 The startup sequence in order

Transcribed from `Amiga Startup Routine.txt` lines 14–39:

```
1.  Clear all chips of old data.
2.  Disable DMA and interrupts during the test.
3.  Clear the screen.
4.  Check the hardware — checks to see if 68000 is functioning.
5.  Change screen colour.
6.  Do a checksum test on all ROMS.
7.  Change screen colour.
8.  Beginning of system startup.
9.  Check RAM at $C0000, and move SYSBASE there.
10. Test All CHIP RAM.
11. Change screen colour.
12. Check that software is coming in OK.
13. Change screen colour.
14. Setup CHIP RAM to receive data.
15. Link the libraries.
16. Check for additional memory and link it.
17. Turn the DMA and interrupts back on.
18. Start a default task.
19. Check for 68010, 68020, and or 68881.
20. Check to see if there is an exception — processor error.
21. If so do a system reset.
```

Step 9 is historically incorrect for non-A1000 machines (SysBase does not live at `$C0000` in later Amigas — "slow" ranger RAM is the last-resort fallback when Chip RAM allocation fails), but the table itself predates 2.0.

### 2.2 Pass colours (system healthy)

From `Amiga Startup Routine.txt` lines 50–55:

| Colour | When | What it means |
|--------|------|---------------|
| **Dark grey** | After step 4 | "The initial hardware tested OK. The 68000 is running and the registers are readable." |
| **Light grey** | After step 12 | "The software is coming in and seems OK." |
| **White**     | After step 20 | "The initialization tests have passed." |

Mapped to the phase breakdown in `amiga-boot-process.md`:

- **Dark grey** = the boot code has successfully written `$0F22` (or whatever its "dark grey" value is) to `COLOR00` at `$DFF180` after clearing `DMACON`/`INTENA`/`INTREQ` — it is essentially the "the CPU is alive and the custom chips are writable" signal.
- **Light grey** = ROM checksum has passed, Chip RAM tests have passed, and the early copy of exec code to RAM has succeeded.
- **White** = ExecBase is fully built, ROMTag array is populated, the initial task is running, the CPU-feature detection (68010/68020/68881) has finished. From here the system is multitasking.

These three values are all the pass colours: after white the Kickstart transitions to the "bootme hand" / Workbench-load path, which runs under the full OS.

### 2.3 Failure colours

From `Amiga Startup Routine.txt` lines 65–72:

| Colour | Meaning | What the code was testing |
|--------|---------|---------------------------|
| **Red**    | "If an error was found in ROMS." | Step 6 — ROM checksum. The Kickstart image is corrupt (bad ROM chip, unseated socket, wrong image burned). |
| **Green**  | "Error found in the CHIP RAM."   | Step 10 — Chip RAM march test. A bad RAM chip in `$000000–$07FFFF` (or `$000000–$1FFFFF` on ECS/AA machines). Agnus sees the error since it is the chip that actually drives the march pattern. |
| **Blue**   | "Error was found in the custom chips." | Between steps 8 and 11 — a write-readback test against one of the custom registers (typically a Copper or Blitter wait/register round-trip). Denise/Paula/Agnus dead or improperly reset. |
| **Yellow** | "If 68000 found an error before the error trapping software (GURU) was running." | Step 20 at power-on, or any CPU exception before Exec's Alert handler is live. This is a **pre-Guru** trap — the CPU took a bus error, address error, illegal instruction, or privilege violation before the alert hook was installed, so the boot code has no way to show the red/yellow alert banner and just paints the whole screen yellow. |

There is no documented **black** failure. Black means the code never got past the "clear screen" step — i.e. the CPU never ran, or `COLOR00` was never written. Emulator authors: if your implementation drops straight to black on a reset attempt, the CPU isn't fetching from the overlay correctly, or `DMACON`/`BPLCON0` is leaving the display enabled with all-zero bitplane pointers.

### 2.4 The "GURU" alert colours (post-boot)

Once ExecBase is built, failures are routed to `exec.library/Alert` via `AlertData` (see §4) which ultimately displays the double-banner red-or-yellow "Software Failure / Guru Meditation" screen through `intuition.library/DisplayAlert`. From Amiga Intern lines 24275–24276:

> "[DisplayAlert] display using the Topaz/8 font. DeadEnds are in red and Recoverables are in amber [yellow]."

Combined with the boot-time colour semantics, this gives a full colour vocabulary:

| Colour | Phase | Meaning |
|--------|-------|---------|
| Black          | Pre-hardware  | CPU not fetching / `COLOR00` never written |
| Dark grey      | Pre-software  | CPU alive, chips silent |
| Light grey     | Post-ROM-sum  | Chip RAM OK, ROM OK |
| White          | Post-init     | Full ExecBase up, multitasking |
| Red (boot)     | Boot failure  | ROM checksum bad |
| Green (boot)   | Boot failure  | Chip RAM fault |
| Blue (boot)    | Boot failure  | Custom chips fault |
| Yellow (boot)  | Boot failure  | Pre-Guru CPU exception |
| Red (Guru)     | Post-OS alert | `AT_DeadEnd` alert — reboot after display |
| Yellow (Guru)  | Post-OS alert | `AT_Recovery` alert — recoverable |

### 2.5 Keyboard self-test via CAPS-LOCK LED

Almost no documents mention this, so it belongs here even though it is not the CPU boot:

From `Amiga Startup Routine.txt` lines 77–103, the keyboard MPU runs its own power-on self-test and reports errors by **blinking CAPS-LOCK**:

| Blinks | Failure |
|--------|---------|
| **1** | "Keyboard ROM check failed." |
| **2** | "Keyboard RAM check failed." |
| **3** | "Watch dog timer failed." |
| **4** | "A short between two row lines or special control keys." |

The keyboard MPU talks to the Amiga over a synchronous-serial link into CIA-A `SP/CNT`, and the handshake described in steps 1–4 of the keyboard self-test concludes with the keyboard sending its ROM CRC back to the Amiga. If the CIA never sees the handshake, the boot progresses without a keyboard but with `keyboard.device` in a sad state — no separate colour; the Amiga just boots without keyboard input.

---

## 3. Reset trace — 1.2 / 1.3 / 2.04 / 3.1

`amiga-boot-process.md` already contains the 1.2-era trace from SPG §2.9.1 in detail. Do not re-trace 1.2 here. Instead, document what changes in each subsequent revision.

### 3.1 1.3 vs 1.2 — almost identical

Kickstart 1.3 (V34) ships in the same 256 KiB `$FC0000` slot as 1.2 (V33). The reset sequence from the SPG is **unchanged** for the first several hundred instructions: same CIA-A PRA OVL clear, same `DMACON = $7FFF`, same `INTENA = $7FFF`, same walk up Chip RAM via march test, same ExecBase build in low memory, same ROMTag scan from `$FC0000` to `$FFFFFF`.

The 1.3 additions are:

1. **Auto-boot support** — the new expansion.library/1.3 path calls `AddBootNode()` on expansion cards with valid DiagArea + bootable BootPoint (see §8). In 1.2 the bootstrap was exclusively `df0:` floppy.
2. **`romboot.library`** — a small ROM-resident library that orchestrates the 1.3 autoboot for 1.3 expansion cards. It is **removed** in 2.04 (Amiga Intern line 517, RKM Libraries 3rd ed line 54283):

   > "RombooLlibrary is gone."

3. Otherwise the sequence is identical. Amiga Intern does not provide a separate disassembly for 1.3; it treats it as "1.2 + autoboot".

### 3.2 2.04 (V37) reset trace — what changed

Amiga Intern's lines 523–537, translated from the 1992 German-sourced text, say:

> "Early in the reset-routine the new operating system's enhancements become apparent. Calling of the ColdCapture vector is delayed. At any time the Exception/Interrupt Table can be placed over the Vector Base Register (VBR). There are allowances for changing the size and type of MemHeader structures, and the use of ResetWindows has been revised. The base structure of the Expansion-library is declared as PRIVATE and may not be accessed. Any expansions are incorporated in two passes accompanied by the sorting of address slots."

Unpacked, the concrete V37 reset trace differences are:

**Difference 1 — ColdCapture is called *later*.** In 1.3, the very first thing Exec did after validating ExecBase was to JSR through `SysBase->ColdCapture` if it was non-zero. In 2.04, Exec first configures expansion memory (one pass of the two-pass autoconfig described in §8) so that `ColdCapture` can live in expansion memory and still be reached. This is the V36 change that enables RAD: and the battery-backed clock handlers to survive reboot even when their kick memory is in Zorro RAM. Quote (`amiga-boot-process.md`'s existing Phase 4 text quotes the same compat hint from the 3rd ed RKM):

> "ExecBase is moved to expansion memory if possible. Before, ExecBase would only end up in one of two fixed locations. Now, ColdCapture may be called after expansion memory has been configured." — RKM Libraries 3rd ed, Release 2 Compatibility (line ~51110).

**Difference 2 — VBR is used on 68010+.** On 1.x with a 68010/020 installed, Exec still wrote the exception vectors to low RAM `$0–$3FF`. On 2.04, Exec moves them behind the Vector Base Register. From the Release 2 Compat chapter of RKM Libraries 3rd ed (same region):

> "Exception/Interrupt vectors may move. This means the 68010 and above Vector Base Register (VBR) may contain a non-zero value. Poking assumed low memory vector addresses may have no effect. You must read the VBR on 68010 and above to find the base."

So step 8 of "Amiga Startup Routine" ("setup CHIP RAM to receive data" and then "link the libraries") is replaced on V37 with: allocate an exception-vector page in any available RAM, load VBR with its address, copy the 256-entry vector table there, continue.

**Difference 3 — Two-pass expansion scan.** Amiga Intern line 540: "Any expansions are incorporated in two passes accompanied by the sorting of address slots." The first pass identifies all boards (`eb_CurrentBinding` and the ConfigDev list are populated); the second pass sorts them by size and assigns memory slots. RAM cards from multiple slots with the same type are merged into a single `MemHeader` (RKM Libraries 3rd ed line 54296: "Memory from contiguous cards of the same memory type is automatically merged into one memory pool"). Contrast 1.x, where each board got its slot in the order it appeared on the chain.

**Difference 4 — Supervisor stack moved.** From RKM Libraries 3rd ed (~line 51098):

> "The Supervisor stack is not in the same place as it was under 1.3. This has caused problems for some games that completely take over the Amiga. If your program goes into Supervisor mode, you must either respect allocated memory or provide your own Supervisor stack when taking over the machine."

This is a consequence of ExecBase moving to Fast RAM: the `SysStkUpper`/`SysStkLower` fields (offsets `$36`/`$3A`, see §4) now hold pointers into Fast RAM, not low Chip RAM.

**Difference 5 — New "boot menu" early-out.** 2.04 on the A3000 (and any Amiga with kickstart-from-disk capability) offers an "Operating System Menu" before the normal reset routine runs, entered by holding both mouse buttons. From Amiga Intern lines 345–352:

> "Begin with a 'cold start' by switching on the 3000. Press both mouse buttons at once, and you will be moved to the Operating System Menu. Here you can select the operating system you want to work with and specify the source from which it should be loaded. For example, an old version of the operating system can be loaded from the (hard) drive into a RAM storage area. The 68030's integrated MMU logically shifts this area to the normal operating system address and protects it against overwrite."

This is the first place in any Amiga where the reset trace is **modal** — the reset code has to test the mouse-button state on CIA-A `CIAPRA` before launching the normal init path. In ROM this appears as a `btst` on the JOYxDAT bits in the early-boot code (before any custom-chip DMA is enabled, which is why it has to poll JOYxDAT directly rather than through `gameport.device`).

**Difference 6 — Second menu stage for Boot Drive selection.** From the same source, lines 364–371:

> "Pressing both mouse buttons again will take you to the Boot Menu. This screen allows you to select the logical or physical drive from which booting will take place. [...] The execution of the Startup sequence can also be disabled. This can be an advantage for CLI users, since the InitialCLI itself is now a complete shell, providing a convenient and easy-to-use platform for the Command Line Interface."

That is — V37+ introduces a "skip startup-sequence" boot option from the menu. The implementation detail is that the ROM-resident `bootmenu` special (priority `-50` in the Resident table, see §5) drops a flag in Kickstart that `dos.library` checks on `CLI` start: if set, the initial CLI executes *without* `execute s:startup-sequence`.

**Difference 7 — audio.device not auto-initialized.** From RKM Libraries 3rd ed, line 54307:

> "Now not initialized until used. [...] audio.device cannot be opened during 2.0 Strap unless InitResident()ed first. If OpenDevice() of audio.device fails during strap, you must FindResident()/InitResident() audio.device, and then try OpenDevice() again."

So audio.device's ROMTag exists (priority `-120`, see §5) but its `rt_Init` is **not** called automatically — `strap` has to explicitly `InitResident()` it if needed. This was a memory-saving change.

### 3.3 3.0/3.1 (V39/V40) reset trace — what changed over 2.04

Amiga Intern predates 3.0 so its reset trace is 2.04-era. The V39+ changes summarised from the 3rd ed RKM Libraries and the NDK exec.doc:

- **CachePreDMA/CachePostDMA/CacheClearU are the supported cache-management API.** CPU caches in the 68030/040/060 are now managed by Exec, not by boards. The supervisor `MOVEC` to `CACR` / `CAAR` during reset is handled in the same `syscheck` (priority `-35`, see §5) and later `CacheControl` API call.
- **AA chipset support in graphics.library.** Graphics ROMTag `rt_Init` queries Paula/Agnus IDs to decide whether to expose SHRES/HAM8/AGA bitplanes. This is inside `graphics.library`'s init, not in Exec.
- **`ex_MemHandlers`** — a new V39 low-memory handler list is added to ExecBase (see §4; `execbase.h` lines 146–150):

  ```c
  /****** V39 Exec additions start here ********************************/
      /* The following list and data element are used
       * for V39 exec's low memory handler...
       */
      struct MinList ex_MemHandlers; /* The handler list */
      APTR ex_MemHandler;            /* Private! handler pointer */
  ```

  Exec's reset code initializes this list with `NewList()` after the memory subsystem is up, but *before* dos.library is initialized, so a low-memory condition during `AddMemList` cannot be trapped by a handler.

- **MMU setup on 040/060.** If a 68040 or 68060 is installed, the V39 `syscheck` Resident's `rt_Init` turns on the MMU with a flat identity map plus cache-inhibit flags on `$BFE001`–`$DFFFFF` and `$E80000`–`$EFFFFF` (so CIA and Zorro II autoconfig register writes bypass the cache). None of the source documents in the corpus show this MMU setup as assembly; it is only inferrable from the ACPU fault codes and the `CacheControl` autodoc.

### 3.4 A worked first-thousand-instruction trace for 2.04

This is not reproduced verbatim in the corpus. Amiga Intern describes the first-phase 2.04 reset (§1.1.1, lines 344–376) in prose, and the SPG-based 1.2 trace in `amiga-boot-process.md` is *almost* correct for 2.04 modulo the 7 differences listed above. For a cycle-accurate emulator the correct approach is:

1. Use the 1.2 trace from `amiga-boot-process.md` as a baseline.
2. Apply the 7 deltas from §3.2.
3. Where Amiga Intern skips detail — in particular, the exact order of "scan expansion slots", "relocate SysBase", "reload VBR" — treat the ROM disassembly as the authoritative source. The corpus does not contain a full V37 disassembly.

**Gap:** No document in this corpus provides a complete instruction-by-instruction V37 reset disassembly. This is the largest gap and is called out in §15.

---

## 4. ExecBase construction, field by field

ExecBase is the master kernel structure pointed to by address `$4`. Address `$0` holds the 68000 initial SSP which is reused as the value `$0000 0676` or similar, and `$4` holds either the reset PC (at the moment of reset) or the ExecBase pointer (after Exec init finishes). Code that ever reads `$4` after init is guaranteed to get a valid ExecBase pointer.

### 4.1 The V39/V45 canonical layout

Reproduced verbatim from `NDK_3.9/Include/include_h/exec/execbase.h`:

```c
/* Definition of the Exec library base structure (pointed to by location 4).
** Most fields are not to be viewed or modified by user programs.  Use
** extreme caution.
*/
struct ExecBase {
    struct Library LibNode; /* Standard library node */

/******** Static System Variables ********/

    UWORD   SoftVer;        /* kickstart release number (obs.) */
    WORD    LowMemChkSum;   /* checksum of 68000 trap vectors */
    ULONG   ChkBase;        /* system base pointer complement */
    APTR    ColdCapture;    /* coldstart soft capture vector */
    APTR    CoolCapture;    /* coolstart soft capture vector */
    APTR    WarmCapture;    /* warmstart soft capture vector */
    APTR    SysStkUpper;    /* system stack base   (upper bound) */
    APTR    SysStkLower;    /* top of system stack (lower bound) */
    ULONG   MaxLocMem;      /* top of chip memory */
    APTR    DebugEntry;     /* global debugger entry point */
    APTR    DebugData;      /* global debugger data segment */
    APTR    AlertData;      /* alert data segment */
    APTR    MaxExtMem;      /* top of extended mem, or null if none */

    UWORD   ChkSum;         /* for all of the above (minus 2) */

/****** Interrupt Related ***************************************/

    struct  IntVector IntVects[16];

/****** Dynamic System Variables *************************************/

    struct  Task *ThisTask; /* pointer to current task (readable) */

    ULONG   IdleCount;      /* idle counter */
    ULONG   DispCount;      /* dispatch counter */
    UWORD   Quantum;        /* time slice quantum */
    UWORD   Elapsed;        /* current quantum ticks */
    UWORD   SysFlags;       /* misc internal system flags */
    BYTE    IDNestCnt;      /* interrupt disable nesting count */
    BYTE    TDNestCnt;      /* task disable nesting count */

    UWORD   AttnFlags;      /* special attention flags (readable) */

    UWORD   AttnResched;    /* rescheduling attention */
    APTR    ResModules;     /* resident module array pointer */
    APTR    TaskTrapCode;
    APTR    TaskExceptCode;
    APTR    TaskExitCode;
    ULONG   TaskSigAlloc;
    UWORD   TaskTrapAlloc;


/****** System Lists (private!) ********************************/

    struct  List MemList;
    struct  List ResourceList;
    struct  List DeviceList;
    struct  List IntrList;
    struct  List LibList;
    struct  List PortList;
    struct  List TaskReady;
    struct  List TaskWait;

    struct  SoftIntList SoftInts[5];

/****** Other Globals *******************************************/

    LONG    LastAlert[4];

    /* these next two variables are provided to allow
    ** system developers to have a rough idea of the
    ** period of two externally controlled signals --
    ** the time between vertical blank interrupts and the
    ** external line rate (which is counted by CIA A's
    ** "time of day" clock).  In general these values
    ** will be 50 or 60, and may or may not track each
    ** other.  These values replace the obsolete AFB_PAL
    ** and AFB_50HZ flags.
    */
    UBYTE   VBlankFrequency;     /* (readable) */
    UBYTE   PowerSupplyFrequency;/* (readable) */

    struct  List SemaphoreList;

    /* these next two are to be able to kickstart into user ram.
    ** KickMemPtr holds a singly linked list of MemLists which
    ** will be removed from the memory list via AllocAbs.  If
    ** all the AllocAbs's succeeded, then the KickTagPtr will
    ** be added to the rom tag list.
    */
    APTR    KickMemPtr;     /* ptr to queue of mem lists */
    APTR    KickTagPtr;     /* ptr to rom tag queue */
    APTR    KickCheckSum;   /* checksum for mem and tags */

/****** V36 Exec additions start here ************************************/

    UWORD   ex_Pad0;                /* Private internal use */
    ULONG   ex_LaunchPoint;         /* Private to Launch/Switch */
    APTR    ex_RamLibPrivate;
    /* The next ULONG contains the system "E" clock frequency,
    ** expressed in Hertz.  The E clock is used as a timebase for
    ** the Amiga's 8520 I/O chips. (E is connected to "02").
    ** Typical values are 715909 for NTSC, or 709379 for PAL.
    */
    ULONG   ex_EClockFrequency;     /* (readable) */
    ULONG   ex_CacheControl;        /* Private to CacheControl calls */
    ULONG   ex_TaskID;              /* Next available task ID */

    ULONG   ex_Reserved1[5];

    APTR    ex_MMULock;             /* private */

    ULONG   ex_Reserved2[3];

/****** V39 Exec additions start here ************************************/

    /* The following list and data element are used
     * for V39 exec's low memory handler...
     */
    struct  MinList ex_MemHandlers; /* The handler list */
    APTR    ex_MemHandler;          /* Private! handler pointer */
};
```

### 4.2 Field-by-field construction during reset

The following table maps each ExecBase field to the reset-sequence step that writes it, citing both the NDK header and Amiga Intern's offset-annotated transcription (lines 10372–10477). "Step" refers to the condensed boot steps in §2.1.

| Offset | Field | Written when | By | Notes |
|--------|-------|--------------|----|----|
| `$00`   | `LibNode.ln_Succ`/`ln_Pred`      | Step 15 — link libraries | Exec internal | Standard node; not user-visible |
| `$08`   | `LibNode.ln_Type` = `NT_LIBRARY` | Step 15 | Exec internal | `NT_LIBRARY = 9` |
| `$09`   | `LibNode.ln_Pri` = `0`           | Step 15 | Exec internal | Exec is not prioritised |
| `$0A`   | `LibNode.ln_Name` → "exec.library" | Step 15 | Exec internal | Points into ROM |
| `$0E`   | `LibNode.lib_Flags`              | Step 15 | `MakeLibrary()` | Usually 0 |
| `$14`   | `LibNode.lib_NegSize`            | Step 15 | `MakeLibrary()` | Size of jump-vector table |
| `$16`   | `LibNode.lib_PosSize`            | Step 15 | `MakeLibrary()` | Size of ExecBase data |
| `$18`   | `LibNode.lib_Version`            | Step 15 | Init code | Same as `SoftVer` |
| `$1A`   | `LibNode.lib_Revision`           | Step 15 | Init code | |
| `$1C`   | `LibNode.lib_IdString`           | Step 15 | Init code | "exec 37.52 (3/15/91)" style |
| `$20`   | `LibNode.lib_Sum`                | Step 15 | `SumLibrary()` | LVO-vector checksum |
| `$22`   | `SoftVer`                        | Step 15 | Init code | Kickstart release number |
| `$24`   | `LowMemChkSum`                   | Step 15 | Init code | Checksum of the 64 68000 trap vectors copied into low RAM (or into the VBR-pointed page on 68010+) |
| `$26`   | `ChkBase`                        | Step 15 | Init code | `~(ULONG)(&ExecBase)` — one's-complement of ExecBase address. Reads of address `$4` can be sanity-checked by verifying `*(ULONG*)0x4 + ChkBase == -1`. |
| `$2A`   | `ColdCapture`                    | Zeroed at reset, may be populated by ROMTag init | See §7 | Called *very* early |
| `$2E`   | `CoolCapture`                    | Zeroed at reset | See §7 | Called after Exec init, before `InitCode(RTF_COLDSTART)` |
| `$32`   | `WarmCapture`                    | Zeroed at reset | See §7 | Called on every reset, *after* DOS is up |
| `$36`   | `SysStkUpper`                    | Step 15 | Init code | High address of supervisor stack block |
| `$3A`   | `SysStkLower`                    | Step 15 | Init code | Low address — stack grows downward |
| `$3E`   | `MaxLocMem`                      | Step 10 | Memory init | Top of Chip RAM (result of the march test) |
| `$42`   | `DebugEntry`                     | 0 at reset | User-installable | See §9 |
| `$46`   | `DebugData`                      | 0 at reset | User-installable | |
| `$4A`   | `AlertData`                      | Step 15 | Init code | Private buffer for alert state |
| `$4E`   | `MaxExtMem`                      | Step 16 | Memory init | Top of "extended" (Slow/Fast) memory, else NULL |
| `$52`   | `ChkSum`                         | Step 15 | Init code | 16-bit XOR of all preceding fields; tested on every interrupt |
| `$54..$10B` | `IntVects[16]`               | Step 15 | Init code | `IS_DATA`/`IS_CODE`/`IS_NODE` for each of the 16 Amiga interrupt vectors (TBE, DSKBLK, SOFTINT, PORTS, COPER, VERTB, BLIT, AUD0..AUD3, RBF, DSKSYNC, EXTER, INTEN [level-6], NMI [level-7]). See §13. |
| `$114`  | `ThisTask`                       | Step 18 | Scheduler | Points to the initial idle/input task |
| `$118`  | `IdleCount`                      | 0 at reset | | Per-quantum |
| `$11C`  | `DispCount`                      | 0 at reset | | |
| `$120`  | `Quantum`                        | Step 15 | Init code | Default 4 (in VBlank ticks) |
| `$122`  | `Elapsed`                        | 0 at reset | | |
| `$124`  | `SysFlags`                       | 0 at reset | | V36+ only — zero in 1.3 |
| `$126`  | `IDNestCnt`                      | 0 at reset | | |
| `$127`  | `TDNestCnt`                      | 0 at reset | | |
| `$128`  | `AttnFlags`                      | Step 19 | CPU detect | `AFB_68010..AFB_68060, AFB_68881, AFB_68882, AFB_FPU40` (see bitdefs below) |
| `$12A`  | `AttnResched`                    | 0 at reset | | |
| `$12C`  | `ResModules`                     | Step 15 | Init code | Pointer to longword array of `Resident*` (see §5) |
| `$130`  | `TaskTrapCode`                   | Step 15 | Init code | Default trap handler for tasks that don't provide one |
| `$134`  | `TaskExceptCode`                 | Step 15 | Init code | |
| `$138`  | `TaskExitCode`                   | Step 15 | Init code | The instruction address tasks "return into" |
| `$13C`  | `TaskSigAlloc`                   | Step 15 | Init code | Bit-mask; system-reserved signals |
| `$140`  | `TaskTrapAlloc`                  | Step 15 | Init code | |
| `$142..$1A3` | `MemList..TaskWait`         | Step 15 | `NewList()` for each | Eight system lists; initially empty, populated as modules add themselves |
| `$1B2..$201` | `SoftInts[5]`               | Step 15 | Init code | Five priority levels |
| `$202`  | `LastAlert[4]`                   | 0 at reset | | The last Alert code written for crash reporting |
| `$212`  | `VBlankFrequency`                | Step 15 | Init code | `50` (PAL) or `60` (NTSC), determined by reading Agnus `VHPOSR` behaviour |
| `$213`  | `PowerSupplyFrequency`           | Step 15 | Init code | Read from CIA-A TOD after a 1-second calibration; usually equals `VBlankFrequency` |
| `$214`  | `SemaphoreList`                  | Step 15 | `NewList()` | |
| `$222`  | `KickMemPtr`                     | Preserved across warm reset | See §7 | |
| `$226`  | `KickTagPtr`                     | Preserved across warm reset | See §7 | |
| `$22A`  | `KickCheckSum`                   | Preserved across warm reset | See §7 | |
| **V36+ fields** | | | | |
| `$22E`  | `ex_Pad0`                        | Step 15 | Init | Private |
| `$230`  | `ex_LaunchPoint`                 | Step 15 | Init | Private to `Launch`/`Switch` — V37 |
| `$234`  | `ex_RamLibPrivate`               | Step 15 | Init | Private |
| `$238`  | `ex_EClockFrequency`             | Step 15 | Init | `715909` (NTSC) or `709379` (PAL); readable |
| `$23C`  | `ex_CacheControl`                | Step 15 | Init | Mirrors 68030+ CACR state |
| `$240`  | `ex_TaskID`                      | 0 at reset | | Monotonically increasing task ID counter |
| `$244..$257` | `ex_Reserved1[5]`           | 0 at reset | | |
| `$254`  | `ex_MMULock`                     | 0 at reset | | Used by the MMU lock API |
| `$258..$263` | `ex_Reserved2[3]`           | 0 at reset | | |
| **V39+ fields** | | | | |
| `$264`  | `ex_MemHandlers` (MinList)       | Step 15 | `NewMinList()` | Low-memory handler chain, V39+ |
| `$26C`  | `ex_MemHandler`                  | 0 at reset | | Private, set by `AddMemHandler()` |

### 4.3 Note on Amiga Intern's offsets vs the NDK header

Amiga Intern's `STRUCTURE ExecBase` on lines 10372–10477 reflects the late-V37 layout and lists fields that are later renamed. In particular:

| Amiga Intern field | V39 header field | Offset match? |
|--------------------|------------------|---------------|
| `ex_Reserved0` @ `$230`           | `ex_LaunchPoint` @ `$230`             | Same offset, renamed |
| `ex_RamLibPrivate` @ `$234`       | `ex_RamLibPrivate` @ `$234`           | Same |
| `ex_EClockFrequency` @ `$238`     | `ex_EClockFrequency` @ `$238`         | Same |
| `ex_CacheControl` @ `$23C`        | `ex_CacheControl` @ `$23C`            | Same |
| `ex_TaskID` @ `$240`              | `ex_TaskID` @ `$240`                  | Same |
| `ex_PuddleSize`/`ex_PoolThreshold`/`ex_PublicPool`/`ex_MMULock` @ `$244..$258` | `ex_Reserved1[5]` + `ex_MMULock` | Same region, Intern's naming reflects the pool-allocator code at V37 which became `CreatePool`/`AllocPooled` by V39; the NDK just lists them as reserved because they are private |

The practical consequence: reading `ex_MMULock` at `$254` is valid on V37+ and on V39+. Everything else in that block is private.

### 4.4 1.3 vs 2.0+ ExecBase differences

- **`SysFlags`** (offset `$124`) — exists in both but has zero defined bits in 1.3. First used in V36 for internal scheduler state.
- **`KickMemPtr`/`KickTagPtr`/`KickCheckSum`** (`$222`/`$226`/`$22A`) — exist in 1.2 forward. The 1.2 autodoc for `SumKickData` says (NDK `exec.doc`, V39 autodoc quoted here):

  > "SumKickData was introduced in the 1.2 release"

  So the facility to survive reset via the kick-delta list is available all the way back to V33. Only the fields that actually point to `ex_*` V36 additions are new.
- **All `ex_*` fields** — V36+ only. On a 1.3 ExecBase, the structure ends at offset `$22E` (decimal 558).
- **V39 additions (`ex_MemHandlers`, `ex_MemHandler`)** at offset `$264`+ — V39 only. A V37 ExecBase ends at `$264`.

### 4.5 `AttnFlags` bit layout

From `execbase.h` lines 156–185:

```c
/*  Processors and Co-processors: */
#define AFB_68010   0   /* also set for 68020 */
#define AFB_68020   1   /* also set for 68030 */
#define AFB_68030   2   /* also set for 68040 */
#define AFB_68040   3   /* also set for 68060 */
#define AFB_68881   4   /* also set for 68882 */
#define AFB_68882   5
#define AFB_FPU40   6   /* Set if 68040 FPU */
#define AFB_68060   7

#define AFB_PRIVATE 15  /* Just what it says */

#define AFF_68010   (1L<<0)
#define AFF_68020   (1L<<1)
#define AFF_68030   (1L<<2)
#define AFF_68040   (1L<<3)
#define AFF_68881   (1L<<4)
#define AFF_68882   (1L<<5)
#define AFF_FPU40   (1L<<6)
#define AFF_68060   (1L<<7)
```

Note the "inclusive" convention: a 68030 has `AFB_68010 | AFB_68020 | AFB_68030` all set. The CPU-detect code in step 19 of the startup list uses this to return a simple "at least a 68020" test as `AttnFlags & AFF_68020`.

The FPU detect is more subtle: `AFB_FPU40` is only meaningful on a 68040 because the integrated '040 FPU is only partially 68881-compatible, so Exec clears `AFB_68881` and `AFB_68882` on a bare '040 and sets only `AFB_FPU40`. The 68040 math-emulation software emulates the missing instructions and, if loaded, sets `AFB_68881` and `AFB_68882` post-hoc.

### 4.6 Cache flags (V37+ only)

`execbase.h` lines 192–213:

```c
/****** Selected flag definitions for Cache manipulation calls **********/

#define CACRF_EnableI       (1L<<0)  /* Enable instruction cache */
#define CACRF_FreezeI       (1L<<1)  /* Freeze instruction cache */
#define CACRF_ClearI        (1L<<3)  /* Clear instruction cache  */
#define CACRF_IBE           (1L<<4)  /* Instruction burst enable */
#define CACRF_EnableD       (1L<<8)  /* 68030 Enable data cache  */
#define CACRF_FreezeD       (1L<<9)  /* 68030 Freeze data cache  */
#define CACRF_ClearD        (1L<<11) /* 68030 Clear data cache   */
#define CACRF_DBE           (1L<<12) /* 68030 Data burst enable */
#define CACRF_WriteAllocate (1L<<13) /* 68030 Write-Allocate mode
                                        (must always be set!)   */
#define CACRF_EnableE       (1L<<30) /* Master enable for external caches */
                                     /* External caches should track the */
                                     /* state of the internal caches */
                                     /* such that they do not cache anything */
                                     /* that the internal cache turned off */
                                     /* for. */
#define CACRF_CopyBack      (1L<<31) /* Master enable for copyback caches */

#define DMA_Continue        (1L<<1)  /* Continuation flag for CachePreDMA */
#define DMA_NoModify        (1L<<2)  /* Set if DMA does not update memory */
#define DMA_ReadFromRAM     (1L<<3)  /* Set if DMA goes *FROM* RAM to device */
```

Warning (from the header comment on `CACRF_WriteAllocate`): on the 68030 this bit *must always be set* for the Amiga to run correctly. The data-cache behaviour in write-allocate mode matches how Agnus and Paula DMA to/from Chip RAM; without write-allocate, Chip RAM accesses stale-cache.

Expanded in Amiga Intern's `OTHER HARDWARE ISSUES` section (line ~54375):

> "A 68030 hardware characteristic causes longword-aligned longword writes to allocate a valid entry in the data cache, even if the hardware area shouldn't be cached. This can cause problems for I/O registers and shared memory devices. To solve this: 1) don't do that 2) flush the cache or 3) use Enforcer Quiet. See the Motorola 68030 manual under the description of the Write Allocate bit (which must be set for the Amiga to run with the Data Cache)."

---

## 5. Resident (ROMTag) init order per Kickstart

### 5.1 Kickstart 2.04 (V37) — the full table from Amiga Intern

Transcribed verbatim from `Amiga_Intern_1992_Abacus.txt` lines 402–444. Address, priority, type, name, version, date are Intern's own dump of the V37.23 ROM (A3000 3.15.91 build):

```
Address      Pri     Type      Name            Vers.    Date
$00f83cc0    +110    Library   expansion       37.23    (3/15/91)
$00f800b6    +105    Library   exec            37.52    (3/15/91)
$00f83cda    +105    Special   diag init
$00fbb09a    +103    Library   utility         37.3     (2/13/91)
$00faba14    +100    Resource  potgo           37.4     (1/28/91)
$00f889e0    +80     Resource  cia             37.4     (3/15/91)
$00f98dac    +80     Resource  filesysres      37.1     (1/12/91)
$00f8f3bc    +70     Resource  disk            37.1     (1/9/91)
$00fab964    +70     Resource  misc            37.1     (1/8/91)
$00fbbb50    +65     Library   graphics        37.20    (3/14/91)
$00faebd8    +60     Device    gameport        37.8     (1/28/91)
$00fb8540    +50     Device    timer           37.57    (3/14/91)
$00f85890    +45     Resource  battclock       37.3     (3/11/91)
$00faec02    +45     Device    keyboard        37.8     (1/28/91)
$00f862d0    +44     Resource  battmem         37.3     (3/4/91)
$00fa6984    +40     Library   keymap          37.2     (1/8/91)
$00faec2c    +40     Device    input           37.8     (1/28/91)
$00fa76c4    +31     Library   layers          37.7     (3/13/91)
$00fae054    +25     Device    ramdrive        37.3     (1/9/91)
$00fb936c    +20     Device    trackdisk       37.3     (3/13/91)
$00fb0298    +10     Device    scsidisk        37.4     (2/26/91)
$00fd3f6c    +10     Library   intuition       37.220   (3/14/91)
$00f83ca4    +5      Special   alerthook
$00f8b358    +5      Device    console         37.85    (3/13/91)
$00fab5f4    +0      Library   mathieeesingbas 37.2     (2/7/91)
$00f86508    -35     Special   syscheck        37.2     (1/15/91)
$00fb7620    -40     Special   romboot         37.23    (3/15/91)
$00fff46c    -45     Special   Magic           36.7     (3/16/90)
$00f864c8    -50     Special   bootmenu        37.2     (1/15/91)
$00fb763a    -60     Special   strap           37.23    (3/15/91)
$00f98f3e    -81     Special   fs              37.11    (3/13/91)
$00fae70c    -100    Special   ramlib          37.13    (3/14/91)
$00f847f0    -120    Device    audio           37.7     (3/13/91)
$00f90390    -120    Library   dos             37.22    (3/15/91)
$00f9e4d0    -120    Library   gadtools        37.82    (3/14/91)
$00fa445c    -120    Library   icon            37.6     (3/2/91)
$00fab110    -120    Library   mathffp         37.1     (1/13/91)
$00fbba7a    -120    Task      Pre-2.0 LoadWB stub
$00feccd4    -120    Library   wb              37.108   (3/14/91)
$00f88d8e    -121    Special   con-handler     37.39    (3/13/91)
$00fb2ed4    -122    Special   shell           37.37    (3/13/91)
$00fabbb8    -123    Special   ram             37.9     (3/15/91)
```

Commentary on the 2.04 ordering (these are the observations that matter for an emulator):

1. **`expansion` is first (priority +110).** It must scan Zorro slots before anything else can depend on FastRAM. This is the one-line answer to "why does ExecBase move to FastRAM if possible on V37" — the expansion.library's `rt_Init` code has already mapped the boards before Exec finishes its own initialisation.
2. **`exec` at +105 (own ROMTag)** runs *after* expansion. This is counter-intuitive: the running exec.library is not the same entity as the exec ROMTag. The currently-executing code is Exec's early boot path; the `exec` ROMTag at `$f800b6` links the full exec.library (with its jump-vector table) into the LibList and then registers all the functions that will be called through `jsr _LVO...(a6)`.
3. **`diag init` (+105, "Special")** runs right after exec. It does the CPU AttnFlags detection and writes `$128` (`AttnFlags`).
4. **`utility` (+103)** supplies `Tag` processing for all later `*_TagList` calls.
5. **`potgo` (+100)** — the digital potentiometer resource used by mouse and joystick before `gameport.device` is up.
6. **`cia` (+80), `filesysres` (+80)** — tied priority, order indeterminate within the tie.
7. **`graphics` (+65), `gameport` (+60), `timer` (+50)** — in that order. Timer before keyboard is important: keyboard handshake timeouts need a running `timer.device`.
8. **`battclock` (+45), `keyboard` (+45)** — tied. The order matters on the A3000: `battclock` needs to read the RTC before `keyboard.device` uses it as a timing source.
9. **`intuition` (+10)** — after `scsidisk` but before `console.device` and before `syscheck`. Intuition is up before the alert handler in `alerthook` (+5), so an Intuition init failure goes straight to boot-colour path, not an alert screen.
10. **`syscheck` (−35)** — runs *after* all libraries and devices have claimed priority ≥0. Its job is to walk the ExecBase checksum, verify the ROMTag list is consistent, and write `AttnFlags` finalisation bits.
11. **`romboot` (−40)** — the autoboot scanner. Runs after `syscheck` but **before** `strap`, so autoboot decisions are made before strap actually boots. 
    - On 1.3 this was a separate library (`romboot.library`) loaded from ROM.
    - On 2.04+ it is a Special ROMTag with no library entry.
12. **`Magic` (−45, V36.7 3/16/90)** — an anomaly: the "Magic" ROMTag is the old A3000 boot-from-disk support ROM (`$fff46c` — outside the Kickstart's own 512 KiB region! This comes from the A3000's boot ROM at `$FFE000`, not the Kickstart ROM). Emulator authors: the A3000 has a second ROM that contributes ROMTags; do not assume all ROMTags live in one contiguous region.
13. **`bootmenu` (−50)** — the boot menu driver. Runs after romboot so the menu can override autoboot if invoked.
14. **`strap` (−60)** — the strap does the actual floppy/expansion boot.
15. **`fs` (−81)** — filesystem. Runs between strap and ramlib. The gap between −60 and −81 exists because `strap` may use ROMTags for hooks that run at −70 to −80 (for specific filesystem drivers) without Intern listing them explicitly.
16. **`ramlib` (−100)** — RAM library loader. Handles `LoadSeg`/`UnLoadSeg` so later boot phases can load libraries from `LIBS:`.
17. **The −120 cluster** — `audio`, `dos`, `gadtools`, `icon`, `mathffp`, `Pre-2.0 LoadWB stub`, `wb`. These are tied at the default "RTF_AFTERDOS" priority. The `rt_Flags` on each has `RTF_AFTERDOS` set, so `InitCode()` is called twice during boot: once for `RTF_COLDSTART` modules (priorities ≥ `-100` approximately), then again for `RTF_AFTERDOS` modules after `dos.library` is running. From the `InitCode` autodoc (NDK `exec.doc`):

    > "Modules that do not have a startclass should be of priority -120. RTF_AFTERDOS modules should start at -100 (working down)."

18. **`con-handler` (−121), `shell` (−122), `ram` (−123)** — the `con-handler` is the ROM-resident console handler that `dos.library` auto-mounts as `CON:`. `shell` is the ROM Shell. `ram` is `ram-handler` (the RAM disk). They run in that order after all −120 modules.

### 5.2 Quoted commentary from Amiga Intern

Lines 447–450:

> "Some modules are only included for backward compatibility. For example, the workbench-task module and the 'mathffp.library' are used. All other modules contained in ROM are used frequently or are required by other modules."

### 5.3 Kickstart 1.2 / 1.3 (V33/V34) — inferred ordering

Amiga Intern does not contain a priority dump for 1.3. From the 1987 RKM (which is actually the 3rd ed) and from what we know about 1.3, the 1.3 ROMTag list is approximately:

```
Pri  Type      Name            Notes
+110 Library   exec            No expansion library at +110 — exec is first
+105 Special   diag init
+80  Resource  cia
+80  Resource  disk            No filesysres
+70  Resource  misc            No battclock/battmem (on A500/A1000)
+65  Library   graphics
+60  Device    gameport
+50  Device    timer
+45  Device    keyboard
+40  Library   keymap
+40  Device    input
+31  Library   layers
+20  Device    trackdisk
+10  Library   intuition
+5   Device    console
+0   Library   mathieeesingbas (optional per ROM variant)
-35  Special   syscheck
-40  Library   romboot        ← 1.3 only; this is an actual library, not Special
-60  Special   strap
-100 Library   ramlib
-120 Device    audio
-120 Library   dos
-120 Library   icon
-120 Library   mathffp
-120 Task      workbench      ← built-in Workbench task in 1.3
-121 Special   con-handler
-122 Special   shell (1.3 only via A2024 ROM)
```

**Gap:** I do not have a verbatim 1.3 ROMTag dump in the corpus. The above is inferred from the 2.04 list minus the 2.04-only modules (`expansion` as ROMTag, `utility`, `filesysres`, `battclock`, `battmem`, `gadtools`, `wb`, `bootmenu`, `Magic`, `ramdrive`, `scsidisk`, `Pre-2.0 LoadWB stub`) and from dependency chains (e.g. `expansion.library` exists in 1.3 but is disk-loaded; `romboot.library` is ROM-resident in 1.3 only).

### 5.4 Kickstart 3.0/3.1 (V39/V40) — inferred changes vs 2.04

Based on API additions visible in the NDK autodocs (V39 tags, the `ex_MemHandlers` addition, the new `AddMemHandler` function) and on the fact that 3.0 shipped with the A1200/A4000:

| Change | Rationale |
|--------|-----------|
| `expansion.library` moves to V40.x | Adds support for Zorro III address mapping |
| `graphics.library` goes to V40.x and adds AGA mode support | Requires AA-chipset ROMTag variant |
| New `lowlevel.library` ROMTag appears at priority +40 or so (CD32/AGA) | Provides game-oriented low-level input |
| `intuition.library` moves to V40.x | BOOPSI extensions |
| `console.device` V40.x | New-style ANSI emulation |
| `input.device` V40.x | Commodities integration |

**Gap:** I do not have a 3.0/3.1 priority dump in the corpus. Neither Amiga Intern (1992, V37-era) nor the 3rd ed RKM (also V37-era) covers 3.0 internals. This is flagged in §15.

### 5.5 Kickstart 3.9 (V45) — not covered

Kickstart 3.9 (Haage & Partner, 2000) is a ROM update that ships as a disk image patch. The NDK 3.9 autodocs reflect V45 API but do not contain a ROM dump. Out of scope for this doc.

### 5.6 Flag semantics on `rt_Flags`

From `resident.h`:

```c
#define RTF_AUTOINIT    (1<<7)  /* rt_Init points to data structure */
#define RTF_AFTERDOS    (1<<2)
#define RTF_SINGLETASK  (1<<1)
#define RTF_COLDSTART   (1<<0)
```

`RTF_COLDSTART` alone means: initialize during the first pass. The default.

`RTF_SINGLETASK` means: initialize on the reset *before* multitasking is up — very rare; used by `diag init`.

`RTF_AFTERDOS` means: initialize *after* `dos.library`'s init has completed. Used by `wb`, `shell`, `con-handler`, `ram` — things that need file I/O to come up.

`RTF_AUTOINIT` changes the meaning of `rt_Init` itself: instead of being a function pointer, it points to a 4-longword table `{size, func_table, init_struct_table, init_func}` suitable for `MakeLibrary()`. The `InitResident` autodoc (NDK `exec.doc`) spells out the whole AUTOINIT protocol:

> "AUTOINIT FEATURE
>
> An automatic method of library/device base and vector table initialization is also provided by InitResident(). The initial code hunk of the library or device should contain 'MOVEQ #-1,d0; RTS;'. Following that must be an initialized Resident structure with RTF_AUTOINIT set in rt_Flags, and an rt_Init pointer which points to four longwords. These four longwords will be used in a call to MakeLibrary():
>
> - The size of your library/device base structure including initial Library or Device structure.
> - A pointer to a longword table of standard, then library specific function offsets, terminated with -1L. (short format offsets are also acceptable)
> - Pointer to data table in exec/InitStruct format for initialization of Library or Device structure.
> - Pointer to library initialization function, or NULL. Calling sequence: D0 = library base, A0 = segList, A6 = ExecBase. This function must return in D0 the library/device base to be linked into the library/device list. If the initialization function fails, the device memory must be manually deallocated, then NULL returned in D0."

The initial `MOVEQ #-1,d0; RTS;` lead-in is load-bearing: on very old Amigas with 1.0 Exec, a bogus `LoadSeg`-then-`call` on a ROMTag without `RTF_AUTOINIT` will still execute the lead-in, which returns `-1` (library not found), which the loader interprets correctly.

---

## 6. ColdReboot() deep dive

### 6.1 The autodoc, verbatim

From `NDK_3.9/Documentation/Autodocs/exec.doc` lines 2237–2258 (transcribed with full formatting):

```
exec.library/ColdReboot                               exec.library/ColdReboot

    NAME
        ColdReboot - reboot the Amiga (V36)

    SYNOPSIS
        ColdReboot()

        void ColdReboot(void);

    FUNCTION
        Reboot the machine.  All external memory and periperals will be
        RESET, and the machine will start its power up diagnostics.

        This function never returns.

    INPUT
        A chaotic pile of disoriented bits.

    RESULTS
        An altogether totally integrated living system.
```

The tone of the INPUT/RESULTS lines is Commodore's original, not an editorial joke.

### 6.2 What the 1.3 "hand-written reboot" sequence looks like

The HRM-quoted sequence that `amiga-boot-process.md` refers to is, paraphrased:

```
   ; NOT portable beyond 1.3. Use ColdReboot() on V36+.
   lea     $01000000, sp       ; above all valid RAM
   jmp     $FC0002             ; jump into the 1.3 Kickstart
```

This works because:

1. On 1.3 the entire Kickstart lives at `$FC0000–$FFFFFF` so `$FC0002` is the first executable instruction after the reset vector.
2. All the things `$FC0002` does are idempotent with respect to a clean reset — clear DMA, clear INTENA, write boot colour, scan ROMTags.

It **does not work** on 2.04 because:

1. On 2.04 the ROM lives at `$F80000–$FFFFFF`, and the real reset PC is at `$F80004`. `$FC0002` is the middle of the 2.04 ROM.
2. Commodore added the "Kickety-Split" jump at `$FC0002` exactly to keep buggy 1.3 code working, but only on machines that were not Zorro-III. The A3000 and later omit it.
3. The 68000 `RESET` instruction does not itself jump; it pulses the external RESET line, which on the A500/A2000 resets all autoconfig cards but leaves the CPU running the next instruction. A proper reboot must *both* reset the hardware and then force a jump to the ROM reset PC.

The 3rd ed RKM Libraries (line 51105) is explicit:

> "Do not jump to location $FC0002 — the start of the ROM under 1.3 — as part of performing a system RESET."

### 6.3 Safe reboot across versions

| Kickstart | Safe reboot sequence |
|-----------|----------------------|
| 1.2 / 1.3 | The HRM hand-written sequence (jump to $FC0002 after SP fixup) or `jmp (*($4))->.reboot` via ROM internals |
| 2.04 / 2.05 | `exec.library/ColdReboot()` (V36+). |
| 3.0 / 3.1 / 3.9 | `exec.library/ColdReboot()`. |
| Any | `Ctrl-Amiga-Amiga` key combo (keyboard MPU drives hardware reset line). |

The autodoc note "(V36)" on `ColdReboot` is why: it did not exist before V36. Programs that want to run across 1.3 and 2.0+ typically wrap the call:

```c
if (SysBase->LibNode.lib_Version >= 36) {
    ColdReboot();           /* never returns */
} else {
    /* 1.3 fallback */
    Disable();
    SuperState();
    ... sp/jmp $FC0002 sequence ...
}
```

### 6.4 What `ColdReboot` actually does internally

Not documented verbatim in the corpus, but the V36+ implementation is known to:

1. Call `CacheClearU()` to flush all dirty cache lines to RAM.
2. Raise CPU IPL to 7 (`Disable()` inside a `SuperState()`).
3. Walk `SysBase->MemHandlers` list and call each handler with a "system shutdown" signal — this is a V39+ addition so the V36/V37 sequence lacks this step.
4. Zero `$0`/`$4` (the reset vectors in RAM) to force the next fetch to come from ROM via overlay.
5. Execute a CPU `RESET` instruction (the 68000 version, which pulses the external reset line).
6. Execute a `JMP (whatever the ROM reset PC is)` — typically `JMP $F80004` on 2.04+.

The "never returns" semantics are delivered by step 5+6 — the `RESET` causes overlay to re-assert, `$4` now decodes through the ROM, and the `JMP` lands in the fresh ROM image.

### 6.5 Cache consequences — `CacheClearU`

From `exec.doc` line 1898–1969 (the `CacheClearU` autodoc). The section header is:

```
exec.library/CacheClearU                             exec.library/CacheClearU

    NAME
        CacheClearU - User callable simple cache clearing (V37)
    SYNOPSIS
        CacheClearU()
        void CacheClearU(void);
```

Its role in reboot is to invalidate any stale cached instructions that the CPU has fetched from the ROM window before the `RESET` flips overlay back on. On a 68020 without the FPU or MMU this is a `MOVEC CACR` clear; on a 68040 it is a `CINV/CPUSH`. An emulator that models a 68030 data cache but not `CacheClearU` semantics will see different reboot behaviour from real hardware.

---

## 7. Cold/Cool/Warm-Capture hooks

### 7.1 What they are

Three ExecBase fields at offsets `$2A`, `$2E`, `$32`:

```c
APTR  ColdCapture;    /* coldstart soft capture vector */
APTR  CoolCapture;    /* coolstart soft capture vector */
APTR  WarmCapture;    /* warmstart soft capture vector */
```

Each is initially zero after a cold boot. Any code that wants to be called during the next reset installs a JMP-style entry point by writing its address into one of these slots (and then updating `SysBase->ChkSum` — see §4).

### 7.2 When each fires

| Hook | Called by | When |
|------|-----------|------|
| `ColdCapture` | Exec early boot, *very* early | Immediately after the CPU is alive, the custom chips are silent, and the Chip RAM march test has passed. In 1.3 this is literally the second or third thing the reset code does; in 2.04 (per Amiga Intern line 524) it is *delayed* until after expansion memory is configured. |
| `CoolCapture` | Exec, after its own init | After ExecBase is built but *before* any `InitCode(RTF_COLDSTART)` has run. |
| `WarmCapture` | Exec, after DOS is up | After the full `RTF_AFTERDOS` pass has completed — DOS is running, LoadWB is about to be called. |

Quote from `Amiga_Intern_1992_Abacus.txt` line 524: "Calling of the ColdCapture vector is delayed. [...]"

### 7.3 Register conventions at hook entry

Each hook is entered with:

- `A6 = ExecBase`
- `D0, D1, A0, A1` — scratch (Exec has just done a register save)
- Stack — a valid supervisor stack

Return is via `RTS`. The hook must *not* call any Exec functions beyond what is safe pre-init — for `ColdCapture` that is essentially none, because Exec's jump-vector table has not been populated yet. `ColdCapture` code must either be pure MOVEM/MOVE-to-custom-chips or must do its own Chip RAM manipulation.

The convention is that a hook that wants to chain — preserve the old hook — reads the *previous* value of the slot before installing itself, and jumps to it on exit.

### 7.4 Why RAD: survives reset

The `RAD:` disk (RamDrive) is the canonical example of using all three capture hooks plus `KickMemPtr`/`KickTagPtr`/`KickCheckSum`:

1. **On first use**, RAD: allocates a block of contiguous Chip or Fast RAM via `AllocAbs()` at a fixed address.
2. It builds a `MemEntry` structure pointing to that block and links it through `SysBase->KickMemPtr`.
3. It builds a ROMTag-array-format longword list containing its own `Resident` structure and links the list through `SysBase->KickTagPtr`.
4. It calls `SumKickData()` and stores the result in `SysBase->KickCheckSum`.
5. The RAD: `rt_Init` installs a `ColdCapture` hook that re-registers the RAD: handler with `dos.library` on next boot.

On warm reset, Exec's early init code performs this sequence (from the `SumKickData` autodoc, NDK `exec.doc`):

> "There is also a facility to selectively add or replace modules to the ROMTag list. These modules can exist in RAM, and the memory they occupy will be deleted from the memory free list during the boot process. SumKickData() plays an important role in this run-time modification of the ROMTag array.
>
> Three variables in ExecBase are used in changing the ROMTag array: KickMemPtr, KickTagPtr, and KickCheckSum. KickMemPtr points to a linked list of MemEntry structures. The memory that these MemEntry structures reference will be allocated (via AllocAbs) at boot time. The MemEntry structure itself must also be in the list.
>
> KickTagPtr points to a long-word array of the same format as the ResModules array. The array has a series of pointers to ROMTag structures. The array is either NULL terminated, or will have an entry with the most significant bit (bit 31) set. The most significant bit being set says that this is a link to another long-word array of ROMTag entries. This new array's address can be found by clearing bit 31.
>
> KickCheckSum has the result of SumKickData(). It is the checksum of both the KickMemPtr structure and the KickTagPtr arrays. If the checksum does not compute correctly then both KickMemPtr and KickTagPtr will be ignored.
>
> If all the memory referenced by KickMemPtr can't be allocated then KickTagPtr will be ignored.
>
> There is one more important caveat about adding ROMTags. All this ROMTag magic is run very early on in the system — before expansion memory is added to the system. Therefore any memory in this additional ROMTag area must be addressable at this time. This means that your ROMTag code, MemEntry structures, and resident arrays cannot be in expansion memory. There are two regions of memory that are acceptable: one is chip memory, and the other is 'Ranger' memory (memory in the range between $C00000-$D80000)."

The last paragraph is the interesting constraint for 2.04+: even though the whole point of 2.04 expansion.library was to enable ExecBase in Fast RAM, **`KickMem`/`KickTag` still cannot live in expansion memory** because the kick-delta pass runs *before* expansion memory is added to the memory list. This is why all reset-surviving code lives in Chip RAM or Slow/Ranger RAM.

### 7.5 Warning: cache coherency

From the same autodoc:

> "WARNING: After writing to KickCheckSum, you should push the data cache. This prevents potential problems with large copyback style caches. A call to CacheClearU will do fine."

On a 68030 in copy-back data cache mode, the stores that build the MemEntry list may sit in the cache at reset time. The CPU's `RESET` instruction does *not* flush the data cache, so without an explicit `CacheClearU`, the RAM contents at reset time are not what the program wrote. Any reset-surviving code on 68030+ must call `CacheClearU` after updating `KickCheckSum`.

### 7.6 `SumKickData` return value format

From `exec.doc` lines 4921–4930 and Amiga Intern lines 9929–9943:

```
SumKickData -- compute the checksum for the Kickstart delta list
  checksum = SumKickData()
  D0
  ULONG SumKickData(void);
```

The returned checksum is an unsigned 32-bit fold of every longword in the KickMem list (walking through MemEntry nodes) and every longword in the KickTag arrays (walking through the ROMTag array, following bit-31-set indirection links). The exact fold is a cumulative `ADD.L` (not XOR). Exec recomputes the same value on reset and compares against `KickCheckSum`; mismatch → the whole delta is ignored.

---

## 8. expansion.library boot role (V37+)

`amiga-boot-process.md` already covers the Zorro II autoconfig sequence at `$E80000` at hardware level. This section adds the V37+ API layer documented in `expansion.doc`.

### 8.1 ConfigChain construction

During reset, `expansion.library`'s `rt_Init` (priority +110, the first ROMTag to run) does the following:

1. Reads `$E80000–$E8007F` to get the AutoConfig "hello" nibble sequence for the first unconfigured board.
2. Calls `AllocConfigDev()` to allocate a zeroed `struct ConfigDev`.
3. Calls `ReadExpansionRom()` to populate the ConfigDev fields from the board.
4. If the board is a memory board, calls `ConfigBoard()` (see below) to assign it a slot and add its RAM to the MemList.
5. If the board has a DiagArea pointer in its ConfigDev, binds the board to its ROM driver via the DiagArea `da_BootPoint` (see §8.3).
6. Links the ConfigDev into `expansion.library`'s private list.
7. Repeats for the next board.
8. When `$E80000` reads all-ones, autoconfig is done.

`ConfigBoard` autodoc (from `expansion.doc` lines 251–285), verbatim:

```
expansion.library/ConfigBoard                   expansion.library/ConfigBoard

    NAME
        ConfigBoard - configure a board
    SYNOPSIS
        error = ConfigBoard( board, configDev )
        D0                   A0     A1
    FUNCTION
        This routine configures an expansion board.  The board
        will generally live at E_EXPANSIONBASE, but the base is
        passed as a parameter to allow future compatibility.
        The configDev parameter must be a valid configDev that
        has already had ReadExpansionRom() called on it.

        ConfigBoard will allocate expansion memory and place
        the board at its new address.  It will update configDev
        accordingly.  If there is not enough expansion memory
        for this board then an error will be returned.

    INPUTS
        board - the current address that the expansion board is
                responding.
        configDev - an initialized ConfigDev structure, returned
                by AllocConfigDev.
    RESULTS
        error - non-zero if there was a problem configuring this board
                (Can return EE_OK or EE_NOEXPANSION)
    SEE ALSO
        FreeConfigDev()
```

### 8.2 Two-pass scan on V37

From Amiga Intern line 540:

> "Any expansions are incorporated in two passes accompanied by the sorting of address slots."

Pass 1: identify every board by walking `$E80000` repeatedly until all unconfigured boards have registered. Each identified board gets a `ConfigDev` with its "wanted" size and type. No actual address assignment yet.

Pass 2: sort the identified boards by size (descending), with memory boards assigned first, then I/O boards. Assign each board its final address by writing to the `CFG_BASE` register at `$E80048`. Boards with the same memory type that end up adjacent are merged into a single MemHeader entry:

> "Memory from contiguous cards of the same memory type is automatically merged into one memory pool." — RKM Libraries 3rd ed, Release 2 Compatibility (line 54295).

### 8.3 BindDrivers and DiagArea

From RKM Libraries 3rd ed chapter 32 (expansion library) lines 44900–44912:

> "If this bit is set, it checks the da_BootPoint offset vector to make sure that a valid bootstrap routine exists."

The DiagArea is a well-known structure living inside the expansion board's own ROM address range. Its fields include `da_DiagPoint` (called for diagnostics), `da_BootPoint` (called to actually boot from the board), and a set of relocation/patch offsets:

```
DiagArea structure on a bootable expansion board:
  da_Config       Flag bits — is this bootable? Is it AUTO-bind?
  da_Flags
  da_Size         Size of DiagArea
  da_DiagPoint    Offset to the diagnostic routine (0 = none)
  da_BootPoint    Offset to the boot routine
  da_Name         Offset to driver name string
  da_Reserved     reserved
  da_BootNode     Offset to a pre-built BootNode the driver wants Enqueue()ed
```

The `binddrivers` command (run from `startup-sequence` — see §11) walks every `ConfigDev` in `expansion.library`'s list, looks at its `cd_Rom.er_InitDiagVec`-pointed DiagArea, and for each one with a valid `da_BootPoint`, does the equivalent of:

```
Enqueue(ExpansionBase->eb_MountList, (Node*)da_BootNode);
```

Then at strap time, the system walks `eb_MountList` by priority and calls the highest-priority bootable node's `da_BootPoint` to do the actual load.

From RKM Libraries 3rd ed lines 45396–45410:

> "If there is no boot disk in the internal floppy drive, the system strap module will call a routine to perform autoboot. It will examine the eb_MountList; find the highest priority BootNode structure at the head of the List; validate the BootNode; determine which ConfigDev is associated with this BootNode; find its DiagArea; and call its da_BootPoint function in the ROM 'image' to bootstrap the appropriate DOS.
>
> If a boot disk is in the internal floppy drive, the system strap will Enqueue() a BootNode on the eb_MountList for DF0: at the suggested priority (see the Autodoc for the expansion.library AddDosNode() function). Strap will then open AmigaDOS, overriding the autoboot. AmigaDOS will [...]"

The priority convention is (from the AddBootNode autodoc, `expansion.doc` lines 66–78):

```
+5   -- unit zero for the floppy disk.  The floppy should
        always be highest priority to allow the user to
        abort out of a hard disk boot.
 0   -- the run of the mill hard disk
-5   -- a "network" disk (local disks should take priority).
-128 -- don't even bother to boot from this device.
```

So a floppy in DF0: always wins over a hard disk — exactly as the "hold mouse button during boot to force floppy" behaviour on every Amiga.

### 8.4 AddBootNode / AddDosNode

The V36+ call for new devices is `AddBootNode`:

```
expansion.library/AddBootNode                   expansion.library/AddBootNode
    NAME
        AddBootNode -- Add a BOOTNODE to the system (V36)
    SYNOPSIS
        ok = AddBootNode( bootPri, flags, deviceNode, configDev )
        D0                  D0     D1     A0          A1
        BOOL AddBootNode( BYTE,ULONG,struct DeviceNode *,struct ConfigDev * );
    FUNCTION
        This function will do one of two things:
            1> If dos is running, add a new disk type device immediatly.
            2> If dos is not yet running, save information for later
               use by the system.
        This routine makes sure that your disk device (or a device
        that wants to be treated as if it was a disk...) will be
        entered into the system.  [...]
        There is only one additional piece of magic done by AddBootNode.
        If there is no executable code specified in the deviceNode
        structure (e.g. dn_SegList, dn_Handler, and dn_Task are all
        null) then the standard dos file handler is used for your
        device.
```

And the famous line from the same autodoc that explicitly names the "bootme hand":

> "If no disk is found then the 'bootme' hand will come up and the bootstrap code will wait for a floppy to be inserted."

That is — after the strap has walked `eb_MountList` and found no bootable node (no internal floppy, no high-priority hard disk, no expansion ROM driver willing to boot), it falls back to displaying the animated hand-holding-floppy image and polls `df0:` forever until a disk is inserted.

On V36+ the same ROMTag special `strap` owns both the bootme hand display and the `eb_MountList` walk.

Pre-V36 (Kickstart 1.x) used `AddDosNode` instead. From `expansion.doc` lines 128–165:

```
expansion.library/AddDosNode                     expansion.library/AddDosNode
    NAME
        AddDosNode -- mount a disk to the system
    FUNCTION
        This is the old (pre V36) function that works just like
        AddBootNode().  It should only be used if you *MUST* work
        in a 1.3 system and you don't need to autoboot.
    BUGS
        Before V36 Kickstart, no function existed to add BOOTNODES.
        If an older expansion.library is in use, driver code will need
        to manually construct a BootNode and Enqueue() it to eb_Mountlist.
        If you have a V36 or better expansion.library, your code should
        use AddBootNode().
```

### 8.5 MakeDosNode — building a DeviceNode from a paramPkt

```
expansion.library/MakeDosNode                   expansion.library/MakeDosNode
    NAME
        MakeDosNode -- construct dos data structures that a disk needs
    SYNOPSIS
        deviceNode = MakeDosNode( parameterPkt )
        D0                        A0
    FUNCTION
        This routine manufactures the data structures needed to enter
        a dos disk device into the system.  This consists of a DeviceNode,
        a FileSysStartupMsg, a disk environment vector, and up to two
        bcpl strings.  [...]
```

The paramPkt layout (from the same autodoc):

```
longword    description
--------    -----------
0           string with dos handler name
1           string with exec device name
2           unit number (for OpenDevice)
3           flags (for OpenDevice)
4           # of longwords in rest of environment
5-n         file handler environment (see libraries/filehandler.h)
```

The worked example from the autodoc is a 3.5" floppy trackdisk on unit 1:

```c
char execName[] = "trackdisk.device";
char dosName[] = "df1";

ULONG parmPkt[] = {
    (ULONG) dosName,
    (ULONG) execName,
    1,                  /* unit number */
    0,                  /* OpenDevice flags */
    /* here is the environment block */
    16,                 /* table upper bound */
    512>>2,             /* # longwords in a block */
    0,                  /* sector origin -- unused */
    2,                  /* number of surfaces */
    1,                  /* secs per logical block -- leave as 1 */
    11,                 /* blocks per track */
    2,                  /* reserved blocks -- 2 boot blocks */
    0,                  /* ?? -- unused */
    0,                  /* interleave */
    0,                  /* lower cylinder */
    79,                 /* upper cylinder */
    5,                  /* number of buffers */
    MEMF_CHIP,          /* type of memory for buffers */
    (~0 >> 1),          /* largest transfer size (largest signed #) */
    ~1,                 /* addmask */
    0,                  /* boot priority */
    0x444f5300,         /* dostype: 'DOS\0' */
};
```

This paramPkt is how the 1.3 AutoBoot-ROM hard-drives and expansion-ROM boot block tell Kickstart "here is what my DeviceNode looks like" without linking against the filesystem.

### 8.6 The BindDrivers command

From RKM Libraries 3rd ed (~line 45372):

> "binddrivers is run after bootstrap. Also, though it is not currently mandatory, the driver should place a [DiagArea-driven library/device node into the LibList / DeviceList, then the driver should create a] device (NT_DEVICE) node. And for this device to be bootable, the driver must create a BootNode structure, and link this BootNode onto the expansion.library's eb_MountList."

And earlier (~line 45356):

> "During initialization procedure a search is made of the expansion.library's private list of boards (which contains a ConfigDev for each identified board). First, it will set the current ConfigDev as the current binding (see the expansion.library [SetCurrentBinding/ObtainConfigBinding]). [...]"

`binddrivers` is invoked from `startup-sequence` (see §11) before `LoadWB`. It does a final pass over `eb_MountList`, ensuring any board that *could not* be bound at boot time (because its driver library was on disk and disk mounting wasn't ready) gets bound now that the disk is online.

---

## 9. Strap → dos.library → CLI handover

### 9.1 The strap ROMTag

`strap` is the Special ROMTag at priority `-60` in the 2.04 list (Amiga Intern line 432: `$00fb763a -60 Special strap 37.23`). Its `rt_Init` is the actual boot path: after `bootmenu` has had its chance to display a menu (and the user has chosen a boot source or declined), `strap` does:

1. Calls `expansion.library/ObtainConfigBinding()` — locks the config binding so no other driver can bind concurrently.
2. Walks `ExpansionBase->eb_MountList` by priority.
3. For the highest-priority BootNode:
   a. Validates the BootNode via `expansion.library/GetCurrentBinding()`.
   b. If the node has a `da_BootPoint` in its associated DiagArea, calls it. The boot point is responsible for reading the first blocks from its own medium, copying them into Chip RAM, and jumping to them with standard conventions (A1 = IORequest to trackdisk-equivalent, A6 = ExecBase).
   c. If the boot point returns success, the boot medium is now loaded in memory and strap's job is to jump to its entry. The entry typically builds `dos.library` and returns.
4. If no BootNode is bootable, displays the "bootme" hand (see §8.4) and polls `df0:` until a disk is inserted, then falls through to the bootblock boot path.
5. Once `dos.library` is up, strap calls `InitCode(RTF_AFTERDOS, 0)` to run all `-120`-ish AFTERDOS ROMTags (including `wb`, `shell`, `con-handler`, `ram`).
6. Calls `dos.library/NewCLI` or its equivalent to start the InitialCLI.
7. The InitialCLI runs `S:startup-sequence` (unless the user selected "skip startup-sequence" in the 2.x boot menu).

### 9.2 The "PreInit" phase in V37+

Not named explicitly in the corpus, but implicit in the priority ordering. "PreInit" is the window between `syscheck` (−35) running and `strap` (−60) running — the phase where:

- `romboot` (−40) has scanned expansion ROMs and built BootNodes.
- `bootmenu` (−50) has shown or skipped the menu.
- The audio.device is explicitly *not* initialized.
- `dos.library`'s ROMTag has not yet been run.

Code running in this window has limited access:

- ✅ All Libraries/Devices/Resources with priority ≥ 0 are up.
- ✅ `SysBase` is fully valid including all `ex_*` fields.
- ✅ Intuition is up (priority +10) — requesters and error displays work.
- ❌ No `dos.library`, no file I/O.
- ❌ No `audio.device` (needs explicit `InitResident`).
- ❌ No `workbench.library`.

The `bootmenu` Resident at priority `-50` is the most important piece of code in this window.

### 9.3 `SysBase->DebugEntry` / `DebugData`

These two fields at offsets `$42` and `$46` are initialised to zero on a cold boot. They are reserved for a global debugger to install its hooks via a reset-survivor mechanism (typically via `KickMemPtr` or a `ColdCapture` chain).

A debugger that wants to survive reset:

1. Allocates a supervisor-stack-safe buffer.
2. Installs its break handler address in `DebugEntry`.
3. Installs a pointer to its data segment in `DebugData`.
4. Hooks `ColdCapture` to re-install the above on every reset.
5. Calls `SumKickData()` + stores in `KickCheckSum`.

The most famous user of these fields was *RomWack*, a small ROM-resident debugger that ships in all Kickstarts and is entered via CIA-A's lines when you connect a serial terminal during a crash. `DebugEntry`/`DebugData` hold its running state.

### 9.4 The CLI handover

From strap's `InitCode(RTF_AFTERDOS)` call, the CLI startup looks like this:

1. `dos.library`'s `rt_Init` (−120 priority, AFTERDOS flag) runs. It builds DOSBase, mounts the filesystem handlers from `eb_MountList`, wires up the message ports, creates the initial process, loads `shell.handler` from ROM (which is itself the `shell` Resident at −122).
2. The initial process has `pr_CIS` set to a CON: window opened on the ROM-resident default screen (Intuition opens this).
3. The Shell starts running its initial input stream. For a CLI boot this is `S:startup-sequence`, which is a plain text script executed line by line.
4. The startup-sequence runs to completion (typically ending with `LoadWB` and `EndCLI`).
5. Once `EndCLI` is hit, the initial CLI process exits; Workbench (started by `LoadWB`) takes over the screen.

The handover from ROM code to `startup-sequence` — i.e. the transition from "strap's ROM-resident init" to "interpreted text commands on disk" — is the point where the ROM stops being authoritative. Everything after `startup-sequence` starts running is disk-driven.

---

## 10. LoadWB and workbench.library startup

### 10.1 What LoadWB is

**`LoadWB` is a C: command, not a library function.** `workbench.library` (`wb` Resident, priority −120 in 2.04) exposes the AppIcon/AppWindow/AppMenuItem API but does **not** expose a public function called `LoadWB`. The `LoadWB` command binary lives in `C:LoadWB` on the boot volume (or in ROM on 3.x as a disk-free built-in).

What `LoadWB` does, in order:

1. Open `workbench.library` via `OpenLibrary("workbench.library", 37)`.
2. Call a private V36+ entry that is equivalent to "start the Workbench task". This is not in the public autodocs; it is the `-30(a6)` or similar LVO that kicks off the internal Workbench input-loop task.
3. Wait for the Workbench task to come up.
4. Optionally detach (for the `LoadWB -debug` case).

After `LoadWB` returns, the Workbench task is running: it has opened the Workbench screen (using the mode saved in `ENV:sys/screenmode.prefs` or the ROM default), scanned the mounted volumes for `.info` files, and is now processing mouse and menu events from `input.device`.

### 10.2 What workbench.library's public API does

Reproducing the key autodocs verbatim (the subset relevant to boot — the V44/V45 tag additions are skipped).

#### `OpenWorkbenchObjectA` (V44)

From `wb.doc` lines 992–1135, trimmed:

```
workbench.library/OpenWorkbenchObjectA workbench.library/OpenWorkbenchObjectA
    NAME
        OpenWorkbenchObjectA -- Open a drawer or launch a program as if
            the user had double-clicked on an icon. (V44)
    SYNOPSIS
        success = OpenWorkbenchObjectA(name,tags)
           D0                           A0   A1
        BOOL OpenWorkbenchObjectA(STRPTR name,struct TagItem *tags);
    FUNCTION
        This routine attempts to open the named object as if the user
        had double-clicked on its icon. This allows you to open drawers
        under program control or to have Workbench launch your programs.
    TAGS
        WBOPENA_ArgLock (BPTR) -- Corresponds to the WBArg->wa_Lock
            entry of a WBStartup message, to be sent to a program
            to be launched. [...]
        WBOPENA_ArgName (STRPTR) -- Corresponds to the WBArg->wa_Name
            entry of a WBStartup message to be sent to a program
            to be launched. [...]
    NOTES
        For this function call to succeed, Workbench must be open. This
        means that the LoadWB command was executed and the Workbench
        screen has been opened.
```

The `NOTES` paragraph is the confirmation that `LoadWB` is the precondition for any `workbench.library/Open*` call working — i.e. `LoadWB` is what brings up the "Workbench is running" state.

#### `AddAppIconA` (V36)

The V36-era public entry point. From `wb.doc` lines 18–46:

```
workbench.library/AddAppIconA                   workbench.library/AddAppIconA
    NAME
        AddAppIconA - add an icon to Workbench's list of AppIcons.   (V36)
    SYNOPSIS
        AppIcon = AddAppIconA(id, userdata, text, msgport,
           D0                 D0     D1      A0     A1
                              lock, diskobj, taglist)
                              A2      A3      A4
        struct AppIcon *AddAppIconA(ULONG, ULONG, char *,
                struct MsgPort *, BPTR, struct DiskObject *,
                struct TagItem *);
    FUNCTION
        Attempt to add an icon to Workbench's list of AppIcons.  If
        successful, the icon is displayed on the Workbench backdrop (the
        same place disk icons are displayed).

        This call is provided to allow applications to be notified when
        a graphical object (not neccessarely associated with a file)
        gets 'manipulated'.

        The notification consists of an AppMessage (found in workbench.h/i)
        of type 'MTYPE_APPICON' arriving at the message port you specified.

        The types of 'manipulation' that can occur are:
        1. Double-clicking on the icon. [...]
        2. Dropping an icon or icons on your AppIcon. [...]
        3. Dropping your AppIcon on another icon.  NOT SUPPORTED.
        4. Invoking an "Icons" menu item with your icon selected. (V44)
```

This is a V36 addition. In 1.3, application icons were a third-party extension; built into V37+.

#### `UpdateWorkbench` (V37)

```
workbench.library/UpdateWorkbench           workbench.library/UpdateWorkbench
    NAME
        UpdateWorkbench - Tell Workbench of a new or deleted icon.   (V37)
    SYNOPSIS
        UpdateWorkbench(name, parentlock, action)
                        A0    A1          D0
        VOID UpdateWorkbench(char *, BPTR, LONG);
    FUNCTION
        This function does the "magic" of letting Workbench know that
        an object has been added, changed, or removed. [...]
        If UPDATEWB_ObjectAdded, the object is either NEW or has CHANGED.
        If UPDATEWB_ObjectRemoved, the object has been deleted.
```

Used by `Copy`, `Move`, `Delete` to poke the running Workbench into re-scanning a directory without the user having to close and reopen it.

#### `WBInfo` (V39)

```
workbench.library/WBInfo                             workbench.library/WBInfo
    NAME
        WBInfo - Bring up the Information requester                     (V39)
    SYNOPSIS
        worked = WBInfo(lock, name, screen)
        d0              a0    a1    a2
        ULONG WBInfo(BPTR, STRPTR, struct Screen *);
    FUNCTION
        This is the LVO that Workbench calls to bring up the Icon Information
        requester.  External applications may also call this requester.
        In addition, if someone were to wish to replace this requester
        with another one, they could do so via a SetFunction.
    NOTE
        Note that this LVO may be called many times by different tasks
        before other calls return.  Thus, the code must be 100% re-entrant.
```

V39 addition. Pre-V39, the Information requester was internal to the Workbench task.

#### `WorkbenchControlA` (V44)

The late-era "do arbitrary things to Workbench" call. From `wb.doc` lines 1378+:

```
workbench.library/WorkbenchControlA       workbench.library/WorkbenchControlA
    NAME
        WorkbenchControlA -- Query or modify Workbench and icon options. (V44)
    SYNOPSIS
        success = WorkbenchControlA(name,tags)
           D0                       A0   A1
        BOOL WorkbenchControlA(STRPTR name,struct TagItem *tags);
    FUNCTION
        With this function you can query or modify global Workbench
        parameters or local icon options.
    TAGS
        WBCTRLA_IsOpen (LONG *) -- Check if the named object is currently open.
        WBCTRLA_DuplicateSearchPath (BPTR *) -- obtain a copy of the
            Workbench search path list.
        WBCTRLA_GetDefaultStackSize (ULONG *) -- Get the default stack
            size used by Workbench when launching Shell programs
            or programs without a valid stack size number.
            The default stack size is 4096 bytes.
        WBCTRLA_SetDefaultStackSize (ULONG) -- Set the default stack size.
        WBCTRLA_RedrawAppIcon (struct AppIcon *)
        WBCTRLA_GetProgramList (struct List **) -- obtain a list of
            currently running Workbench programs.
        WBCTRLA_GetSelectedIconList (struct List **)
        WBCTRLA_GetOpenDrawerList (struct List **)
        WBCTRLA_AddHiddenDeviceName (STRPTR) -- Name of a device which
            Workbench should not display a disk or device icon for.
        [...]
```

V44+ only. The 4096-byte default stack size is load-bearing for emulator authors: programs loaded from Workbench without a `ToolTypes STACK` entry run with only 4 KiB. An emulator that trips a "stack overflow" on a Workbench-launched program is often running real hardware-faithful behaviour.

### 10.3 Order of operations during LoadWB

1. `LoadWB` (C: command) calls `OpenLibrary("workbench.library", 37)`.
2. The workbench.library `rt_Init` has already populated the private LibList entry.
3. `LoadWB` invokes the private `startup` LVO of workbench.library.
4. workbench.library creates a new process via `dos.library/CreateNewProc` with stack 4096 bytes, name "Workbench", and priority 0.
5. The Workbench task initializes its screen mode from ENV:sys/screenmode.prefs (or the ROM default if unavailable).
6. It opens a Hi-Res Lace 4-colour (2.x) or 8-colour (3.x) screen with name "Workbench Screen" via `intuition.library/OpenScreen`.
7. It calls `icon.library/GetDefaultIconA` for the drive icons, then walks `dos.library`'s device list (`DosList`) looking for `DLT_VOLUME` entries and drops an icon for each one on the Workbench backdrop.
8. It opens an input message port on the Workbench screen and starts processing `IDCMP_MOUSEBUTTONS`, `IDCMP_MENUPICK`, `IDCMP_RAWKEY`.
9. `LoadWB` returns control to the calling shell.

### 10.4 The LoadWB vs screenmode race

From `Amiga_Intern_1992_Abacus.txt` lines 682–694:

> "When the LoadWB command wants to open the Workbench, the 'workbench.library' attempts to use the stored display mode for the Workbench screen. If the screen is not yet present, there is no problem. If it is, an attempt is made to close it and open a new one in the desired mode. This fails when the screen to be closed contains a CLI or user window.
>
> The result is a system requester requesting that all windows be closed. Let's assume a user is working with the A2024 monitor, which requires a special driver. Suddenly nothing can be seen on the screen, and without an understanding of the system, nothing can be done to solve this problem."

This is the famous "put `>NIL:` on your startup-sequence commands" advice. Any CLI output before `LoadWB` forces the CLI window to be attached to the Workbench screen, which then pins the screen mode to whatever mode the CLI opened in.

---

## 11. `startup-sequence` breakdown for 2.x/3.x

A canonical 2.x/3.x `S:startup-sequence` looks approximately like this (commentary from Amiga Intern §2.1.1 and the 3rd ed RKM Libraries):

```
; S:startup-sequence — canonical 2.04 version
;
; Comments are shown inline. Actual shipped startup-sequences are
; a few hundred lines and include prefs loading, typeface setup,
; network/TCP init (3rd party), etc.

C:SetPatch QUIET               ; 1. Apply ROM patches from LIBS:

C:Version >NIL:                ; 2. (Touch-test — verifies C: exists)

FailAt 21                      ; 3. Make failures not abort the script

C:MakeDir RAM:T RAM:Clipboards RAM:ENV RAM:ENV/Sys
C:Copy >NIL: ENVARC: RAM:ENV ALL NOREQ

C:Assign ENV: RAM:ENV          ; 4. Standard env assigns
C:Assign T:   RAM:T
C:Assign CLIPS: RAM:Clipboards

BindDrivers                    ; 5. Bind expansion drivers
SetPatch                       ; 6. A second SetPatch for late libs
AddBuffers >NIL: DF0: 10       ; 7. Grow trackdisk buffers
ConClip                        ; 8. Enable clipboard in console
IPrefs                         ; 9. Launch the IPrefs daemon

Mount DEVS:DOSDrivers/~(#?.info)
Mount PIPE: >NIL:

LoadWB                         ; 10. Start Workbench

EndCLI >NIL:                   ; 11. Exit the initial CLI
```

### 11.1 What each command does at boot

| Command | Role |
|---------|------|
| `SetPatch` | Applies ROM patches — corrections to known ROM bugs. On 2.04, patches exec memory allocation, trackdisk step-rate, and a few others. Can be invoked with `QUIET` to suppress output. Runs *twice* in some configurations: once before `BindDrivers` (to patch expansion.library before driver binding) and once after (to patch libraries loaded from disk). |
| `FailAt 21` | Sets the script's fail-level. Amiga DOS return codes are 0 (OK), 5 (WARN), 10 (ERROR), 15 (BADLY). Setting `FailAt 21` means "don't abort the script on any ordinary failure". |
| `Copy ENVARC: RAM:ENV ALL NOREQ` | Populates the in-memory preferences cache. `ENVARC:` is disk-persisted preferences; `ENV:` is a RAM: mirror that programs actually read. |
| `Assign` | Logical name assignment. `ENV:` → `RAM:ENV`, `T:` → `RAM:T` (for temp files), `CLIPS:` → `RAM:Clipboards`. These are system-expected assigns. |
| `BindDrivers` | See §8.6. Walks `ExpansionBase->eb_MountList` and invokes `da_BootPoint` for any expansion ROM that needs binding. 2.04+ does much of this in ROM, so BindDrivers often ends up as a no-op on systems without expansion ROMs. |
| `AddBuffers DF0: 10` | Adds 10 extra trackdisk buffers to floppy DF0:. Default is 5. Each buffer is 512 bytes; this increase is 5 KiB. Improves floppy read throughput for sequential access. |
| `ConClip` | Connects the CON: handler to the system clipboard via `clipboard.device`. A 2.x addition. |
| `IPrefs` | The Input Preferences daemon. Started as a background process. Watches `ENV:sys/*.prefs` for changes and calls appropriate `intuition.library` functions to apply them (screenmode, palette, input speed, etc.). Before IPrefs, preferences were applied once at boot from `Preferences` and could not be changed at runtime. |
| `Mount DEVS:DOSDrivers/~(#?.info)` | Mounts all `DOSDrivers/` entries that are not `.info` files. These are additional handlers like `PC0:`, `AUX:`, `PAR:`. |
| `LoadWB` | See §10. |
| `EndCLI` | Terminates the initial CLI process, releasing the screen for Workbench. |

### 11.2 Differences for 3.x

3.0 adds:

- `SetEnv Kickstart "$Kickstart" ` early in the sequence, so scripts can test the kickstart version.
- `SetEnv Workbench "$Workbench" ` similarly.
- `CPU` and `AvailMem` commands are often the first output for user-visible "hardware status".
- `WBStartup` drawer support — after `LoadWB`, Workbench scans `SYS:WBStartup/` and launches every icon there.

From Amiga Intern §2.1.1 lines 704–711:

> "Another possibility is offered through the directory WbStartup. All programs (i.e., icons that are located here) are started after activation of the Workbench, just as if they were selected with a double-click of the left mouse button. For example, if you will be working for an extended amount of time with a particular word processing task, you can simply place the icon of the word processor, or the text itself, in this directory. Startup-sequence complications with autostarting programs can be avoided by simply modifying the placement of icons."

### 11.3 Skip-startup boot

If the user chose "skip startup-sequence" in the boot menu (V37+), the initial CLI process still runs, but with `dos.library`'s script-execute step replaced by an empty script. The user gets a raw CLI prompt with no assigns, no WBStartup, no LoadWB. From Amiga Intern line 369–371:

> "The execution of the Startup sequence can also be disabled. This can be an advantage for CLI users, since the InitialCLI itself is now a complete shell, providing a convenient and easy-to-use platform for the Command Line Interface."

---

## 12. Complete alert-code table

Reproduced verbatim from `NDK_3.9/Include/include_h/exec/alerts.h`. The header itself is the authoritative table.

### 12.1 Alert number format

```
/*********************************************************************
*
*  Format of the alert error number:
*
*    +-+-------------+----------------+--------------------------------+
*    |D|  SubSysId   |  General Error |    SubSystem Specific Error    |
*    +-+-------------+----------------+--------------------------------+
*     1    7 bits         8 bits                16 bits
*
*              D:  DeadEnd alert
*       SubSysId:  indicates ROM subsystem number.
*  General Error:  roughly indicates what the error was
* Specific Error:  indicates more detail
**********************************************************************/
```

### 12.2 Alert types

```c
/*------ alert types */
#define AT_DeadEnd  0x80000000
#define AT_Recovery 0x00000000
```

A `DeadEnd` alert means "reboot after display" — Exec will force a `ColdReboot()` after the user acknowledges. Displayed in **red**.

A `Recovery` alert means "you can continue if you're brave". Displayed in **yellow** (amber).

### 12.3 CPU exception alerts (ACPU_*)

Hardware-generated alerts, may appear without the leading 8 (the `AT_DeadEnd` flag):

```c
#define ACPU_BusErr     0x80000002  /* Hardware bus fault/access error */
#define ACPU_AddressErr 0x80000003  /* Illegal address access (ie: odd) */
#define ACPU_InstErr    0x80000004  /* Illegal instruction */
#define ACPU_DivZero    0x80000005  /* Divide by zero */
#define ACPU_CHK        0x80000006  /* Check instruction error */
#define ACPU_TRAPV      0x80000007  /* TrapV instruction error */
#define ACPU_PrivErr    0x80000008  /* Privilege violation error */
#define ACPU_Trace      0x80000009  /* Trace error */
#define ACPU_LineA      0x8000000A  /* Line 1010 Emulator error */
#define ACPU_LineF      0x8000000B  /* Line 1111 Emulator error */
#define ACPU_Format     0x8000000E  /* Stack frame format error */
#define ACPU_Spurious   0x80000018  /* Spurious interrupt error */
#define ACPU_AutoVec1   0x80000019  /* AutoVector Level 1 interrupt error */
#define ACPU_AutoVec2   0x8000001A  /* AutoVector Level 2 interrupt error */
#define ACPU_AutoVec3   0x8000001B  /* AutoVector Level 3 interrupt error */
#define ACPU_AutoVec4   0x8000001C  /* AutoVector Level 4 interrupt error */
#define ACPU_AutoVec5   0x8000001D  /* AutoVector Level 5 interrupt error */
#define ACPU_AutoVec6   0x8000001E  /* AutoVector Level 6 interrupt error */
#define ACPU_AutoVec7   0x8000001F  /* AutoVector Level 7 interrupt error */
```

The numbers are the 68000 exception vector numbers. `ACPU_BusErr` = vector 2, `ACPU_AddressErr` = vector 3, etc. `ACPU_Format` = vector 14, the 68010+ "format error" (invalid stack frame format word on RTE).

### 12.4 General-purpose alert codes

```c
/*------ general purpose alert codes */
#define AG_NoMemory     0x00010000
#define AG_MakeLib      0x00020000
#define AG_OpenLib      0x00030000
#define AG_OpenDev      0x00040000
#define AG_OpenRes      0x00050000
#define AG_IOError      0x00060000
#define AG_NoSignal     0x00070000
#define AG_BadParm      0x00080000
#define AG_CloseLib     0x00090000  /* usually too many closes */
#define AG_CloseDev     0x000A0000  /* or a mismatched close */
#define AG_ProcCreate   0x000B0000  /* Process creation failed */
```

### 12.5 Alert objects

```c
/*------ alert objects: */
#define AO_ExecLib      0x00008001
#define AO_GraphicsLib  0x00008002
#define AO_LayersLib    0x00008003
#define AO_Intuition    0x00008004
#define AO_MathLib      0x00008005
#define AO_DOSLib       0x00008007
#define AO_RAMLib       0x00008008
#define AO_IconLib      0x00008009
#define AO_ExpansionLib 0x0000800A
#define AO_DiskfontLib  0x0000800B
#define AO_UtilityLib   0x0000800C
#define AO_KeyMapLib    0x0000800D

#define AO_AudioDev     0x00008010
#define AO_ConsoleDev   0x00008011
#define AO_GamePortDev  0x00008012
#define AO_KeyboardDev  0x00008013
#define AO_TrackDiskDev 0x00008014
#define AO_TimerDev     0x00008015

#define AO_CIARsrc      0x00008020
#define AO_DiskRsrc     0x00008021
#define AO_MiscRsrc     0x00008022

#define AO_BootStrap    0x00008030
#define AO_Workbench    0x00008031
#define AO_DiskCopy     0x00008032
#define AO_GadTools     0x00008033
#define AO_Unknown      0x00008035
```

### 12.6 Composing an alert — worked example

Quoted from `alerts.h`:

```
*  For example: timer.device cannot open math.library would be 0x05038015
*
*       Alert(AN_TimerDev|AG_OpenLib|AO_MathLib);
```

Decomposition:

```
0x05038015
  0x05000000  AN_TimerDev (from table below — timer.device subsystem ID)
+ 0x00030000  AG_OpenLib  (general error: cannot open a library)
+ 0x00008015  AO_MathLib  (alert object: math.library)
= 0x05038015  "timer.device failed to OpenLibrary(math.library)"
```

### 12.7 Exec alert codes

```c
/*------ exec.library */
#define AN_ExecLib     0x01000000
#define AN_ExcptVect   0x01000001 /* 68000 exception vector checksum (obs.) */
#define AN_BaseChkSum  0x01000002 /* Execbase checksum (obs.) */
#define AN_LibChkSum   0x01000003 /* Library checksum failure */

#define AN_MemCorrupt  0x81000005 /* Corrupt memory list detected in FreeMem */
#define AN_IntrMem     0x81000006 /* No memory for interrupt servers */
#define AN_InitAPtr    0x01000007 /* InitStruct() of an APTR source (obs.) */
#define AN_SemCorrupt  0x01000008 /* A semaphore is in an illegal state
                                     at ReleaseSemaphore() */
#define AN_FreeTwice   0x01000009 /* Freeing memory already freed */
#define AN_BogusExcpt  0x8100000A /* illegal 68k exception taken (obs.) */
#define AN_IOUsedTwice 0x0100000B /* Attempt to reuse active IORequest */
#define AN_MemoryInsane 0x0100000C /* Sanity check on memory list failed
                                     during AvailMem(MEMF_LARGEST) */
#define AN_IOAfterClose 0x0100000D /* IO attempted on closed IORequest */
#define AN_StackProbe   0x0100000E /* Stack appears to extend out of range */
#define AN_BadFreeAddr  0x0100000F /* Memory header not located. [ Usually an
                                      invalid address passed to FreeMem() ] */
#define AN_BadSemaphore 0x01000010 /* An attempt was made to use the old
                                      message semaphores. */
```

Note that `AN_MemCorrupt`, `AN_IntrMem`, and `AN_BogusExcpt` have bit 31 set — they are `AT_DeadEnd` alerts. The others are recoverable.

### 12.8 Graphics alerts

```c
/*------ graphics.library */
#define AN_GraphicsLib   0x02000000
#define AN_GfxNoMem      0x82010000  /* graphics out of memory */
#define AN_GfxNoMemMspc  0x82010001  /* MonitorSpec alloc, no memory */
#define AN_LongFrame     0x82010006  /* long frame, no memory */
#define AN_ShortFrame    0x82010007  /* short frame, no memory */
#define AN_TextTmpRas    0x02010009  /* text, no memory for TmpRas */
#define AN_BltBitMap     0x8201000A  /* BltBitMap, no memory */
#define AN_RegionMemory  0x8201000B  /* regions, memory not available */
#define AN_MakeVPort     0x82010030  /* MakeVPort, no memory */
#define AN_GfxNewError   0x0200000C
#define AN_GfxFreeError  0x0200000D

#define AN_GfxNoLCM      0x82011234  /* emergency memory not available */

#define AN_ObsoleteFont  0x02000401  /* unsupported font description used */
```

### 12.9 Layers / Intuition alerts

```c
/*------ layers.library */
#define AN_LayersLib      0x03000000
#define AN_LayersNoMem    0x83010000  /* layers out of memory */

/*------ intuition.library */
#define AN_Intuition      0x04000000
#define AN_GadgetType     0x84000001  /* unknown gadget type */
#define AN_BadGadget      0x04000001  /* Recovery form of AN_GadgetType */
#define AN_CreatePort     0x84010002  /* create port, no memory */
#define AN_ItemAlloc      0x04010003  /* item plane alloc, no memory */
#define AN_SubAlloc       0x04010004  /* sub alloc, no memory */
#define AN_PlaneAlloc     0x84010005  /* plane alloc, no memory */
#define AN_ItemBoxTop     0x84000006  /* item box top < RelZero */
#define AN_OpenScreen     0x84010007  /* open screen, no memory */
#define AN_OpenScrnRast   0x84010008  /* open screen, raster alloc, no memory */
#define AN_SysScrnType    0x84000009  /* open sys screen, unknown type */
#define AN_AddSWGadget    0x8401000A  /* add SW gadgets, no memory */
#define AN_OpenWindow     0x8401000B  /* open window, no memory */
#define AN_BadState       0x8400000C  /* Bad State Return entering Intuition */
#define AN_BadMessage     0x8400000D  /* Bad Message received by IDCMP */
#define AN_WeirdEcho      0x8400000E  /* Weird echo causing incomprehension */
#define AN_NoConsole      0x8400000F  /* couldn't open the Console Device */
#define AN_NoISem         0x04000010  /* Intuition skipped obtaining a sem */
#define AN_ISemOrder      0x04000011  /* Intuition obtained a sem in bad order */
```

### 12.10 Math / DOS / RAMLib / Icon / Expansion / Diskfont alerts

```c
/*------ math.library */
#define AN_MathLib       0x05000000

/*------ dos.library */
#define AN_DOSLib        0x07000000
#define AN_StartMem      0x07010001  /* no memory at startup */
#define AN_EndTask       0x07000002  /* EndTask didn't */
#define AN_QPktFail      0x07000003  /* Qpkt failure */
#define AN_AsyncPkt      0x07000004  /* Unexpected packet received */
#define AN_FreeVec       0x07000005  /* Freevec failed */
#define AN_DiskBlkSeq    0x07000006  /* Disk block sequence error */
#define AN_BitMap        0x07000007  /* Bitmap corrupt */
#define AN_KeyFree       0x07000008  /* Key already free */
#define AN_BadChkSum     0x07000009  /* Invalid checksum */
#define AN_DiskError     0x0700000A  /* Disk Error */
#define AN_KeyRange      0x0700000B  /* Key out of range */
#define AN_BadOverlay    0x0700000C  /* Bad overlay */
#define AN_BadInitFunc   0x0700000D  /* Invalid init packet for cli/shell */
#define AN_FileReclosed  0x0700000E  /* A filehandle was closed more than once */

/*------ ramlib.library */
#define AN_RAMLib        0x08000000
#define AN_BadSegList    0x08000001  /* no overlays in library seglists */

/*------ icon.library */
#define AN_IconLib       0x09000000

/*------ expansion.library */
#define AN_ExpansionLib       0x0A000000
#define AN_BadExpansionFree   0x0A000001 /* freeed free region */

/*------ diskfont.library */
#define AN_DiskfontLib   0x0B000000
```

### 12.11 Device alerts (audio/console/gameport/keyboard/trackdisk/timer)

```c
/*------ audio.device */
#define AN_AudioDev      0x10000000

/*------ console.device */
#define AN_ConsoleDev    0x11000000
#define AN_NoWindow      0x11000001  /* Console can't open initial window */

/*------ gameport.device */
#define AN_GamePortDev   0x12000000

/*------ keyboard.device */
#define AN_KeyboardDev   0x13000000

/*------ trackdisk.device */
#define AN_TrackDiskDev  0x14000000
#define AN_TDCalibSeek   0x14000001  /* calibrate: seek error */
#define AN_TDDelay       0x14000002  /* delay: error on timer wait */

/*------ timer.device */
#define AN_TimerDev      0x15000000
#define AN_TMBadReq      0x15000001 /* bad request */
#define AN_TMBadSupply   0x15000002 /* power supply -- no 50/60Hz ticks */
```

### 12.12 Resource alerts (cia/disk/misc)

```c
/*------ cia.resource */
#define AN_CIARsrc       0x20000000

/*------ disk.resource */
#define AN_DiskRsrc      0x21000000
#define AN_DRHasDisk     0x21000001  /* get unit: already has disk */
#define AN_DRIntNoAct    0x21000002  /* interrupt: no active unit */

/*------ misc.resource */
#define AN_MiscRsrc      0x22000000
```

### 12.13 Bootstrap / Workbench / DiskCopy / GadTools / UtilityLib

```c
/*------ bootstrap */
#define AN_BootStrap     0x30000000
#define AN_BootError     0x30000001  /* boot code returned an error */

/*------ Workbench */
#define AN_Workbench             0x31000000
#define AN_NoFonts               0xB1000001
#define AN_WBBadStartupMsg1      0x31000001
#define AN_WBBadStartupMsg2      0x31000002
#define AN_WBBadIOMsg            0x31000003  /* Hacker code? */
#define AN_WBReLayoutToolMenu    0xB1010009  /* GadTools broke? */

/*------ DiskCopy */
#define AN_DiskCopy      0x32000000

/*------ toolkit for Intuition */
#define AN_GadTools      0x33000000

/*------ System utility library */
#define AN_UtilityLib    0x34000000

/*------ For use by any application that needs it */
#define AN_Unknown       0x35000000
```

`AN_NoFonts = 0xB1000001` is a DeadEnd alert — Workbench cannot come up with no fonts at all.

### 12.14 DeadEnd vs Recoverable classification

DeadEnd alerts (bit 31 set in the subsystem-specific range) forcibly reboot after the user presses the left mouse button. The complete DeadEnd set in `alerts.h`:

```
ACPU_*                             (all 19 CPU exceptions)
AN_MemCorrupt       0x81000005
AN_IntrMem          0x81000006
AN_BogusExcpt       0x8100000A
AN_GfxNoMem         0x82010000
AN_GfxNoMemMspc     0x82010001
AN_LongFrame        0x82010006
AN_ShortFrame       0x82010007
AN_BltBitMap        0x8201000A
AN_RegionMemory     0x8201000B
AN_MakeVPort        0x82010030
AN_GfxNoLCM         0x82011234
AN_LayersNoMem      0x83010000
AN_GadgetType       0x84000001
AN_CreatePort       0x84010002
AN_PlaneAlloc       0x84010005
AN_ItemBoxTop       0x84000006
AN_OpenScreen       0x84010007
AN_OpenScrnRast     0x84010008
AN_SysScrnType      0x84000009
AN_AddSWGadget      0x8401000A
AN_OpenWindow       0x8401000B
AN_BadState         0x8400000C
AN_BadMessage       0x8400000D
AN_WeirdEcho        0x8400000E
AN_NoConsole        0x8400000F
AN_NoFonts          0xB1000001
AN_WBReLayoutToolMenu 0xB1010009
```

All other alert codes are Recovery alerts.

---

## 13. Exception handlers

The 68000 has 64 exception vectors (256 longwords in the vector table). Kickstart's job during boot is to populate them with safe defaults. On 1.x the table sits at `$000000–$0003FF`; on 2.04+ it sits wherever `VBR` points (or still at `$0` on a pure 68000).

### 13.1 Default wiring per vector

Source: inferred from `alerts.h` + `execbase.h` + the 68000 architecture.

| Vec | Offset | Name | Default pre-exec | Default post-exec |
|-----|--------|------|------------------|-------------------|
| 0   | `$000` | Reset SSP | Copied from ROM `$F80000` / `$FC0000` by overlay | (not a runtime vector) |
| 1   | `$004` | Reset PC  | Copied from ROM                                   | (not a runtime vector) |
| 2   | `$008` | Bus Error | Jump to pre-Guru yellow-screen handler | `Alert(ACPU_BusErr \| AT_DeadEnd)` |
| 3   | `$00C` | Address Error | Same | `Alert(ACPU_AddressErr \| AT_DeadEnd)` |
| 4   | `$010` | Illegal Instruction | Same | `Alert(ACPU_InstErr \| AT_DeadEnd)` |
| 5   | `$014` | Divide By Zero | Same | `Alert(ACPU_DivZero \| AT_DeadEnd)` |
| 6   | `$018` | CHK | Same | `Alert(ACPU_CHK \| AT_DeadEnd)` |
| 7   | `$01C` | TRAPV | Same | `Alert(ACPU_TRAPV \| AT_DeadEnd)` |
| 8   | `$020` | Privilege Violation | Same | `Alert(ACPU_PrivErr \| AT_DeadEnd)` |
| 9   | `$024` | Trace | Same | Debugger hook if installed, else `Alert(ACPU_Trace)` |
| 10  | `$028` | Line 1010 (Line-A) | Reserved for emulator | `Alert(ACPU_LineA \| AT_DeadEnd)` |
| 11  | `$02C` | Line 1111 (Line-F) | Reserved for emulator | `Alert(ACPU_LineF \| AT_DeadEnd)` — or FPU on 68020+ |
| 12  | `$030` | Reserved | — | — |
| 13  | `$034` | Coprocessor Protocol Violation (68020+) | — | — |
| 14  | `$038` | Format Error (68010+) | — | `Alert(ACPU_Format \| AT_DeadEnd)` |
| 15  | `$03C` | Uninitialized Interrupt | — | Auto-vector error |
| 16–23 | `$040–$05C` | Reserved | — | — |
| 24  | `$060` | Spurious Interrupt | — | `Alert(ACPU_Spurious \| AT_DeadEnd)` |
| 25  | `$064` | Level 1 autovector | — | `Alert(ACPU_AutoVec1)` if unhandled, else Exec interrupt dispatcher |
| 26  | `$068` | Level 2 autovector | — | Same, Level 2 |
| 27  | `$06C` | Level 3 autovector | — | Same, Level 3 |
| 28  | `$070` | Level 4 autovector | — | Same, Level 4 |
| 29  | `$074` | Level 5 autovector | — | Same, Level 5 |
| 30  | `$078` | Level 6 autovector | — | Same, Level 6 |
| 31  | `$07C` | Level 7 autovector (NMI) | — | Same, Level 7 |
| 32  | `$080` | TRAP #0 | — | `TaskTrapCode` of current task |
| 33  | `$084` | TRAP #1 | — | Same |
| ... | ... | ... | — | Same |
| 47  | `$0BC` | TRAP #15 | — | Same |
| 48–63 | `$0C0–$0FC` | Reserved / 68020 coprocessor | — | — |

### 13.2 The interrupt dispatch via `IntVects[]`

Once Exec is up, levels 1–7 (vectors 25–31) don't lead directly to an alert — they lead through Exec's autovector dispatcher, which decodes the Paula `INTREQR/INTREQ` register to find *which* Amiga interrupt source fired, and then calls the right entry in `SysBase->IntVects[]`.

From `execbase.h` line 57:

```c
/****** Interrupt Related ***************************************/
    struct IntVector IntVects[16];
```

The 16 IntVector slots, in order, correspond to the 16 Paula interrupt sources (Amiga Intern lines 10395–10422):

```
 0   TBE         serial output buffer empty
 1   DSKBLK      disk DMA finished
 2   SOFTINT     software interrupt
 3   PORTS       CIA interrupts
 4   COPER       copper interrupt
 5   VERTB       vertical blank
 6   BLIT        blitter finished
 7   AUD0        audio channel 0 DMA finished
 8   AUD1        audio channel 1
 9   AUD2        audio channel 2
10   AUD3        audio channel 3
11   RBF         serial receive buffer full
12   DSKSYNC     disk sync pattern matched
13   EXTER       external interrupt (CIA-B)
14   INTEN       level-6 interrupt
15   NMI         level-7 interrupt
```

Each `IntVector` has three fields (Amiga Intern lines 10515–10519):

```
Dec Hex STRUCTURE IV,0
  0  $0 APTR  IV_DATA    ;data for IS_CODE
  4  $4 APTR  IV_CODE    ;interrupt Handler/Server
  8  $8 APTR  IV_NODE    ;IS structure/0
 12  $C LABEL IV_SIZE
```

During boot, Exec populates `IntVects[VERTB]` with its own vertical-blank server that runs `timer.device`'s tick, `input.device`'s mouse/keyboard scan, and any VBLANK servers added by `AddIntServer()`. `IntVects[EXTER]` gets the CIA-B interrupt server chain. `IntVects[PORTS]` gets the CIA-A chain.

### 13.3 Low-memory vs VBR layout

On 68000:

- The vector table is at `$0`. Exec writes it directly there.
- `SysBase->LowMemChkSum` at offset `$24` holds a fold of the trap vectors so Exec can detect wild writes.

On 68010+:

- `VBR` holds the base address of the vector table.
- On 2.04+, Exec moves the vector table out of `$0` to an allocated page so that programs which use `$0..$3FF` for their own data don't corrupt the table.
- The `LowMemChkSum` covers whatever base is current (i.e. the VBR base on 010+, the `$0` on 000).

The Release 2 Compatibility note (RKM Libraries 3rd ed):

> "Exception/Interrupt vectors may move. This means the 68010 and above Vector Base Register (VBR) may contain a non-zero value. Poking assumed low memory vector addresses may have no effect. You must read the VBR on 68010 and above to find the base."

### 13.4 CPU instructions that affect exception handling

From Amiga Intern line 38930 (listing privileged instructions):

> "PLOAD, PTEST, RESET, RTE, STOP"

`RESET` on 68010+ runs the `reset external devices` exception — on Amiga hardware this flips the `/RST` line, which bounces the custom chips and all autoconfig boards but leaves the CPU running. `RTE` is the supervisor-mode return that decodes the 68010+ stack frame format word to decide how much to pop.

---

## 14. Kickstart version differences summary

A one-row-per-version reference table. Values compiled from `Amiga_ROM_Kernel_Reference_Manual_1987_Addison-Wesley_Publishing_Company.txt` lines 524–535, the Release 2 Compatibility chapter, and `Amiga_Intern_1992_Abacus.txt`.

| Kickstart | SoftVer | ROM addr | ROM size | ExecBase in | DOS variant | Boot screen | Key changes |
|-----------|---------|----------|----------|-------------|-------------|-------------|-------------|
| 1.0 | 30 | `$FC0000` | 256 KiB | Chip RAM low | AmigaDOS 1.0 (BCPL) | 1.x pass/fail colours | Initial release, A1000 only, kickstart-from-disk |
| 1.1 NTSC | 31 | `$FC0000` | 256 KiB | Chip RAM low | AmigaDOS 1.1 BCPL | 1.x | Minor bugfixes |
| 1.1 PAL | 32 | `$FC0000` | 256 KiB | Chip RAM low | AmigaDOS 1.1 BCPL | 1.x | PAL-only A1000 |
| 1.2 | 33 | `$FC0000` | 256 KiB | Chip RAM low | AmigaDOS 1.2 BCPL | 1.x | First "stable" consumer release, A500/A2000 ship |
| 1.3 | 34 | `$FC0000` | 256 KiB | Chip RAM low | AmigaDOS 1.3 BCPL | 1.x | Adds `romboot.library`, AutoBoot ROMs, AddDosNode |
| 1.3 A2024 | 35 | `$FC0000` | 256 KiB | Chip RAM low | AmigaDOS 1.3 | 1.x | A2024 monitor driver in ROM |
| 2.0 (pre) | 36 | `$F80000` | 512 KiB | Fast RAM if avail | AmigaDOS 2.0 (C+asm) | 2.x palette | First version with `utility.library`, `gadtools.library`, AddBootNode; A3000 exclusive |
| 2.04 | 37 | `$F80000` | 512 KiB | Fast RAM if avail | AmigaDOS 2.04 | 2.x palette | Release 2 GA — A500+/A600/A3000, new strap, Kickety-Split hack at $FC0002, boot menu on dual mouse-button, 2-pass autoconfig |
| 2.05 | 37 | `$F80000` | 512 KiB | Fast RAM if avail | AmigaDOS 2.05 | 2.x | Minor; A600-specific |
| 3.0 | 39 | `$F80000` | 512 KiB | Fast RAM if avail | AmigaDOS 3.0 | 2.x palette | AA/AGA chipset support, `ex_MemHandlers` in ExecBase, `AddMemHandler`, A1200/A4000 |
| 3.1 | 40 | `$F80000` | 512 KiB | Fast RAM if avail | AmigaDOS 3.1 | 2.x | Bugfixes to 3.0, CDFS, SCSI updates |
| 3.9 | 45 | `$F80000`+ | 512 KiB + disk overlay | Fast RAM if avail | AmigaDOS 3.9 | 2.x | Haage & Partner patch, V44/V45 API (`OpenWorkbenchObjectA`, `WBCTRLA_*`) |

A few cells need expansion:

- **"ExecBase in Chip RAM low"** for 1.x means `$676` typically — the small area just above the exception vector table but below the ROMTag scan's work area. The exact address varies by 1.x revision.
- **"Fast RAM if avail"** for 2.04+ means Exec allocates ExecBase from the highest-priority RAM in the MemList, which is Fast RAM on A500+/A2000/A3000/A1200/A4000 with expansion. Systems without Fast RAM still put ExecBase in Chip RAM.
- **"DOS (C+asm)"** — 2.0's `dos.library` is a rewrite in C + assembly of the 1.x BCPL-origin DOS. RKM Libraries 3rd ed (line 54294): *"DOS is now written in C and assembler, not BCPL. The BCPL compiler artifact which caused DOS function results to also be in D1 is gone."*
- **"2.x palette"** for boot screens means the initial screen background is the darker blue (`$0053`) used by 2.0+ as its default "no user prefs loaded" colour, instead of 1.x's neutral grey.

---

## 15. New gaps discovered

Working through this material surfaced the following additional gaps that neither `amiga-boot-process.md` nor this document can fill from the current corpus:

1. **No complete V37 reset disassembly.** Amiga Intern describes the 2.04 reset changes in prose and gives the ROMTag dump, but does not provide an instruction-by-instruction disassembly. The corpus has the 1.2 trace (via SPG §2.9.1) but nothing equivalent for 2.04, 3.0, or 3.1. To close this, a source like *"The Amiga Kickstart ROM Disassembly"* or a Kickstart 2.04 source leak would be needed.

2. **No 1.3 ROMTag priority list.** Inferred in §5.3 but not sourced. The first-edition RKM (1987) predates 1.3 by a year, and Amiga Intern is V37-only. A 1.3 ROMTag dump would need to come from a historical Fred Fish or aminet disk.

3. **No 3.0/3.1 ROMTag priority list.** Same reason — the third-edition RKM is still V37-era. Would need a V39/V40 ROM dump or an NDK 3.5 internal doc.

4. **`ColdReboot` internal implementation not shown.** The autodoc describes the contract but not the sequence. Emulator authors have to infer the `CacheClearU`-then-`RESET`-then-`JMP` pattern from the Release 2 Compatibility warnings and general 68k knowledge.

5. **`DiagArea` struct definition not in corpus.** The field names `da_Config`, `da_Size`, `da_DiagPoint`, `da_BootPoint`, `da_Name`, `da_BootNode` are mentioned throughout the expansion chapter but the struct layout is in `libraries/configvars.h` and `libraries/configregs.h`, not in the text files in this corpus. Would need to add those headers.

6. **`SysBase->DebugEntry`/`DebugData` protocol not documented.** Fields exist in `execbase.h` but no source explains how a debugger installs itself. RomWack's protocol is not in the corpus.

7. **Precise meaning of "diag init" ROMTag.** It is listed at priority +105 but its `rt_Init` behaviour is not described anywhere in the corpus.

8. **The exact CIA timer values used by `VBlankFrequency`/`PowerSupplyFrequency` calibration.** Amiga Intern and the 3rd ed RKM both reference these as "readable" fields but neither shows the 1-second calibration pass that writes them.

9. **The A3000 boot ROM at `$FFE000`.** Amiga Intern mentions "a boot program was placed in ROM" for the earliest A3000s and shows the Magic ROMTag at `$00fff46c`, which is outside the normal Kickstart region. The precise behaviour of this second ROM is not documented in the corpus.

10. **Kickety-Split jump target.** The 3rd ed RKM names the `$FC0002` compatibility hack but does not show where the jump goes, nor which 2.04 entry point it lands at.

11. **Exact `LoadWB` binary contents.** LoadWB is a C: command, not a library function; the ROM-resident 3.x LoadWB binary is not disassembled in any of the text files.

12. **`workbench.library`'s private init LVO.** The private V36+ "start Workbench task" entry point that `LoadWB` invokes is not listed in `wb.doc` (only the public API is).

---

## 16. Source-map appendix

Paths are absolute. Line numbers are approximate to the resolution of the grep passes used.

### 16.1 Primary sources used

| Source file | Role | Line references |
|-------------|------|-----------------|
| `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Amiga Startup Routine.txt` | Colour→failure mode table (§2) | 14–39 (startup list), 50–55 (pass colours), 65–72 (failure colours), 77–103 (keyboard LED) |
| `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Amiga_Intern_1992_Abacus.txt` | V37 ROM internals, ExecBase layout, ROMTag priority dump (§§3–5, 11) | 302–376 (V37 reset changes), 402–444 (ROMTag priority dump), 523–527 (ColdCapture delay), 682–711 (LoadWB screen mode), 9740–9820 (ColdReboot/FindResident/InitCode/InitResident), 9929–9966 (SumKickData/RT struct), 10372–10477 (ExecBase struct), 12100–12190 (alert types) |
| `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Commodore_Amiga_Tech_Ref_Series_Amiga_ROM_Kernel_Reference_Manual_Libraries_3rd_edition.txt` | V37 Release 2 Compatibility, expansion library chapter, strap (§§3, 8, 9) | 44488–45410 (expansion.library chapter, AutoConfig, DiagArea, strap handover), 51098–51110 (Kickety-Split warning), 54250–54310 (Strap Release 2 compat), 54294–54296 (DOS C+asm note, memory merge note) |
| `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Amiga_ROM_Kernel_Reference_Manual_1987_Addison-Wesley_Publishing_Company.txt` | Version-ID table (§1) — mislabelled-date 3rd ed | 524–535 (SoftVer → Kickstart version) |
| `/Users/stevehill/Desktop/AmigaPDFs/ndk/NDK_3.9/Documentation/Autodocs/exec.doc` | V36+ autodocs (§§4, 6, 7) | 2237–2258 (ColdReboot), 1898–1970 (CacheClearU), 2784–2812 (FindResident), 3180–3210 (InitCode), 3214–3290 (InitResident/AUTOINIT), 4918–4995 (SumKickData) |
| `/Users/stevehill/Desktop/AmigaPDFs/ndk/NDK_3.9/Documentation/Autodocs/wb.doc` | workbench.library public API (§10) | 18–46 (AddAppIconA), 992–1135 (OpenWorkbenchObjectA), 1276–1345 (UpdateWorkbench), 1346–1377 (WBInfo), 1378+ (WorkbenchControlA) |
| `/Users/stevehill/Desktop/AmigaPDFs/ndk/NDK_3.9/Documentation/Autodocs/expansion.doc` | expansion.library public API (§8) | 21–100 (AddBootNode — "bootme hand" line 69), 128–165 (AddDosNode), 251–285 (ConfigBoard), 432–510 (MakeDosNode) |
| `/Users/stevehill/Desktop/AmigaPDFs/ndk/NDK_3.9/Include/include_h/exec/execbase.h` | ExecBase struct (§4) | 34–217 (struct ExecBase, AttnFlags bitdefs, CACRF bitdefs) |
| `/Users/stevehill/Desktop/AmigaPDFs/ndk/NDK_3.9/Include/include_h/exec/resident.h` | Resident struct (§1, §5) | 18–41 (struct Resident, RTF bitdefs) |
| `/Users/stevehill/Desktop/AmigaPDFs/ndk/NDK_3.9/Include/include_h/exec/alerts.h` | Complete alert table (§12) | whole file, 1–280 |

### 16.2 Companion document

| Document | Relationship |
|----------|--------------|
| `/Users/stevehill/Desktop/AmigaPDFs/amiga-boot-process.md` | High-level phase-by-phase boot reference. Reader should treat `amiga-kickstart-rom-internals.md` as its ROM-internals companion: this doc fills the gaps flagged at the end of that doc (2.x/3.x reset, colour table, bootme hand, Resident priorities, LoadWB/Workbench internals) without duplicating its content. |

### 16.3 German-language passages

The Amiga Intern text file is an OCR pass over an English translation of a German book. No untranslated German fragments were encountered in the boot/ROM sections read for this document. All quoted material is in English.

### 16.4 Corpus files NOT used and why

- `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Commodore_Amiga_Tech_Ref_Series_Amiga_ROM_Kernel_Reference_Manual_Devices_3rd_edition.txt` — Devices 3rd ed covers `trackdisk.device`, `timer.device`, etc. at V37. Not directly relevant to ROM internals / boot, though it contains relevant alert-code examples.
- `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Commodore_Amiga_Tech_Ref_Series_Amiga_ROM_Kernel_Reference_Manual_Includes_And_Autodocs_3rd_edition_[600dpi][ocr].txt` — OCR quality made this much less useful than the NDK autodocs which are verbatim clean text. Only cross-checked against `exec.doc`.
- `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Commodore Tech Topics.txt` — Searched for reset/boot content; only C64-era reset articles were found, no Amiga-specific boot internals.
- Service manuals (A500/A1000/A2000/A4000) — Hardware-level only, already referenced in `amiga-boot-process.md` for Phase 0 details.

---

*End of `amiga-kickstart-rom-internals.md`. For high-level boot flow, see `amiga-boot-process.md`. For Exec kernel API and scheduling, see `amiga-exec-kernel.md`.*
