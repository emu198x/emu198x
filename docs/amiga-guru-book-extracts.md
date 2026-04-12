# The Amiga Guru Book -- Targeted Extracts for Emulator Reference

**Source:** Ralph Babel, *The Amiga Guru Book* (1993 English edition), OCR text at
`rkm/txt/the-amiga-guru-book-a-reference-manual_compress.txt` (36,422 lines).

**Purpose:** Fill 14 documented gaps in the existing 17-document Amiga emulator reference library.

---

## 1. MFM Floppy Track Format

### Gap
Byte-level sector layout, sync words, header structure, odd/even MFM interleaving,
exact track geometry for 880 KB DD floppy.

### What the Guru Book Says

**NOT COVERED in detail.** The Guru Book discusses MFM and floppy topics only at the
system-software level (trackdisk.device, filesystem blocks, boot blocks). It does
not contain a byte-level MFM track format description, sync word documentation,
sector header layout, or odd/even interleaving scheme.

The only direct mention of MFM is in the filesystem chapter:

> to the trackdisk.device, which must convert the transferred sectors to MFM format
> before writing them to the floppy, the filesystem must "digest" the contents of files and
> directories in such a way that the data comes out in pieces of a constant size (the block
> size), often 512 bytes, which can then be passed along to the associated Exec device.
> (Guru Book, near line 17292)

The book does mention raw trackdisk commands and sync words in passing:

> need arise, there are still the "raw" commands of the trackdisk.device (TD_RAWREAD,
> TD_RAWWRITE), which, however, unfortunately do not work properly under Kickstart
> 1.2 and 1.3 and may not provide all the functionality that is available at the hardware
> level (e.g. arbitrary sync words, GCR).
> (Guru Book, section 2.3.9 "The disk.resource", near line 2592)

And regarding GCR:

> Contrary to popular belief, GCR is not supported at 2 us per bit cell.
> (Guru Book, footnote 20, near line 2633)

**Verdict:** This gap remains unfilled. The Amiga Hardware Reference Manual (HRM)
appendix C and the trackdisk.device documentation are the primary sources for
MFM track format details.

**Supplements:** `amiga-dos-filesystem-disk.md`, `amiga-hardware-reference.md`

---

## 2. Bootblock Checksum Algorithm

### Gap
Producer-side algorithm: how to compute a valid checksum for a bootblock. The consumer
side (sum all longwords, result = $00000000) is known.

### What the Guru Book Says

**FULLY COVERED.** The Guru Book provides both the algorithm description and a
complete working C implementation.

#### Algorithm Description

> The boot-block checksum is calculated the same way as the Kickstart checksum (cf.
> section 9.2.3): "additive-carry wrap-around" to a sum of $FFFF FFFF. The exam-
> ple program Install2 calculates the checksum dynamically, so that only the assembly-
> language part Install2A.a has to be changed in order to implement one's own boot block.
> (Guru Book, section 10.3 "The boot block", near line 10570)

#### Kickstart Checksum Reference Implementation (consumer side)

The Guru Book provides the `SumKickROM()` function that validates the checksum:

```c
ULONG SumKickROM(void)
{
    extern UBYTE far romend[];
    ULONG *i, kicksum, sum;

    kicksum = 0;

    for(i = (ULONG *)(romend - *(ULONG *)(romend - 0x13) + 1);
        i < (ULONG *)(romend + 1); ++i)
    {
        sum = kicksum;
        if((kicksum += *i) < sum)
            ++kicksum;
    }

    return -kicksum;
}
```

> it must be so chosen that the following function, which calculates the additive-
> carry wrap-around longword sum of the Kickstart image, returns the value zero.
> (Guru Book, section 9.2.3, near line 9776)

The key detail: `if((kicksum += *i) < sum) ++kicksum;` -- this is the "additive-carry
wrap-around" operation. When the 32-bit addition overflows, the carry is added back
into the sum (ones' complement addition).

#### Producer-Side Implementation (Install2C.c listing)

The Guru Book's `Install2C.c` listing (near line 11422) provides the complete
bootblock checksum producer algorithm:

```c
/* Boot block structure */
struct BootSectors
{
    struct BootBlock bs_BootBlock;
    UBYTE bs_Data[BOOTSECTS * TD_SECTOR - sizeof(struct BootBlock)];
};

/* ... */

/* Set up the boot block header */
*(ULONG *)bs->bs_BootBlock.bb_id = BBNAME_DOS;
bs->bs_BootBlock.bb_dosblock = ROOT;      /* 880 for standard 3.5" */
CopyMem(bootCode, bs->bs_Data, bootCodeSize);

/* Calculate checksum -- THE PRODUCER ALGORITHM */
checksum = 0;
for(i = 0; i < sizeof(struct BootSectors) / sizeof(ULONG); ++i)
{
    precsum = checksum;
    if((checksum += ((ULONG *)bs)[i]) < precsum)
        ++checksum;
}

bs->bs_BootBlock.bb_chksum = ~checksum;
```

(Guru Book, listing "Install2C.c", lines 88--100, near line 11422--11434)

#### Algorithm Summary for Emulator Implementation

1. Zero the checksum field (`bb_chksum`) in the bootblock
2. Sum all longwords across ALL boot sectors (not just the first one) using
   ones' complement addition (add carry back in)
3. Bitwise-NOT the result and store it in `bb_chksum`
4. Verification: summing all longwords (including the checksum) with the same
   algorithm should yield $FFFFFFFF (or equivalently, ~sum == 0)

**Important note from the book:**

> The checksum must be calculated for all boot sectors, not just the first one.
> (Guru Book, section 15.3.9, near line 18209)

#### Boot Block Layout

The book also provides the boot block structure (near line 18213):

| Longword | Field | Description |
|----------|-------|-------------|
| 0 | BOOTBLOCKID | `'DOS\0'` (OFS), `'DOS\1'` (FFS), `'DOS\2'` (intl OFS), `'DOS\3'` (intl FFS), or `'KICK'` |
| 1 | CHECKSUM | Block checksum (boot block only) |
| 2 | DOSBLOCK | Pointer to root block (boot block only) |
| 3..SIZE-1 | BOOT CODE | Program to be executed upon start-up (only if checksum is correct) |

> Prior to Kickstart 2.0, the boot block always consisted of two sectors of 512 bytes
> of data each, located at offset zero of the medium inserted in unit zero of the track-
> disk.device; the 2.0 strap module, however, permits any number of boot blocks of (al-
> most) any size to be used with any boot partition.
> (Guru Book, section 10.3, near line 10505)

#### Filesystem Block Checksum (different algorithm!)

The standard filesystem block checksum (root block, file headers, etc.) uses a
SIMPLER algorithm at offset 5:

> The longword at offset 5 of almost all block types holds a checksum. It is the negative
> sum of all other longwords, discarding possible overflows. The sum (modulo 2^32) of all
> longwords in an uncorrupted block therefore always equals zero.
> (Guru Book, section 15.3.3.3, near line 17372)

This is standard two's complement addition (mod 2^32), NOT the ones' complement
additive-carry used by bootblocks and Kickstart. The bitmap block is the exception:
its checksum is at offset 0, not offset 5, but uses the same algorithm.

**Supplements:** `amiga-dos-filesystem-disk.md`, `amiga-boot-process.md`

---

## 3. Paula Analog Low-Pass Filter

### Gap
Cutoff frequency, filter order, R/C values, -3 dB point. The "LED filter" controlled
by CIA-A PRA bit 1.

### What the Guru Book Says

**PARTIALLY COVERED.** The Guru Book documents the software control interface but
not the analog circuit details (no R/C values, no cutoff frequency, no filter order).

> **2.3.6 The audio cut-off filter**
>
> With the exception of the A1000 and the A2000A, all Amigas provide a way of turning
> off the 7-kHz audio filter. No protective protocol exists for this procedure, so it is
> possible that several programs could conflict with one another in this respect.
>
> Turning off the filter can be accomplished as follows:
>
> `ciaa.ciapra |= CIAF_LED;`
>
> Turning it back on can be done this way:
>
> `ciaa.ciapra &= ~CIAF_LED;`
>
> Although not a perfect protection, the contents of the CIAB_LED bit should be saved
> beforehand and restored at the end of the program, instead of just being toggled, to at
> least maintain the proper behavior for LIFO order.
> (Guru Book, section 2.3.6, near line 2498)

Key facts documented:
- The filter is referred to as the "7-kHz audio filter" (not 3.3 kHz or 4.5 kHz)
- Controlled via `CIAF_LED` bit in CIA-A's PRA register
- The A1000 and A2000A do NOT have this controllable filter
- Setting the bit HIGH turns the filter OFF (note: the LED is active-low,
  so LED off = filter off)
- No protective protocol exists; multiple programs can conflict

The book also documents the 14 kHz audio limit:

> the well-known 14-kHz audio limit ... As DMA slots are allocated on a per-scan-line
> basis, this limit may vary with the programmable scan rates as implemented in the ECS.
> (Guru Book, section 2.3.5, near line 2481)

**Verdict:** The 7 kHz figure is documented. The analog circuit details (R/C values,
filter order, exact -3 dB point) are NOT in this book. The Amiga Service Manual
schematics are the source for those details.

**Supplements:** `amiga-io-audio-expansion.md`, `amiga-service-electrical.md`

---

## 4. Register Reset States

### Gap
What every custom chip register reads as after hardware reset. The HRM only commits
to a few.

### What the Guru Book Says

**NOT COVERED.** The Guru Book does not provide a table of register reset states.

The closest content found is this note about start-up diagnostics:

> Colors displayed before the dark gray screen comes up during start-up are simply what
> has been left in the custom-chip registers and do not indicate a failure.
> (Guru Book, section 9.4.1, near line 9970)

This statement implies that custom chip registers are NOT zeroed by a hardware
reset -- they contain leftover values from whatever state the chips were in. This is
itself a useful data point for emulator implementation.

The book also provides the complete custom chip register map with access types
(R/W/S for strobe) and which chip owns each register (A=Agnus, D=Denise, P=Paula):

```
$dff000  bltddat  ER A    (early read, Agnus)
$dff002  dmaconr  R  A P
$dff004  vposr    R  A
$dff006  vhposr   R  A
$dff008  dskdatr  ER P    (early read, Paula)
$dff00a  joy0dat  R  D
$dff00c  joy1dat  R  D
$dff00e  clxdat   R  D
$dff010  adkconr  R  P
$dff012  pot0dat  R  P
$dff014  pot1dat  R  P
$dff016  potinp   R  P
$dff018  serdatr  R  P
$dff01a  dskbytr  R  P
$dff01c  intenar  R  P
$dff01e  intreqr  R  P
$dff020  dskpt    W  A    (pointer, 2 words)
$dff024  dsklen   W  P
$dff026  dskdat   W  P    (early write)
$dff028  refptr   W  A    (early write)
$dff02a  vposw    W  A
$dff02c  vhposw   W  A
$dff02e  copcon   W  A
$dff030  serdat   W  P
$dff032  serper   W  P
$dff034  potgo    W  P
$dff036  joytest  W  D
$dff040  bltcon0  W  A
$dff042  bltcon1  W  A
$dff044  bltafwm  W  A
$dff046  bltalwm  W  A
$dff048  bltcpt   W  A    (pointer)
$dff04c  bltbpt   W  A    (pointer)
$dff050  bltapt   W  A    (pointer)
$dff054  bltdpt   W  A    (pointer)
$dff058  bltsize  W  A
$dff05a  bltcmod  W  A
$dff05c  bltbmod  W  A
$dff05e  bltamod  W  A
$dff060  bltdmod  W  A
$dff062  bltcdat  W  A
$dff064  bltbdat  W  A
$dff066  bltadat  W  A
$dff080  cop1lc   W  A    (pointer)
$dff084  cop2lc   W  A    (pointer)
$dff088  copjmp1  S  A    (strobe)
$dff08a  copjmp2  S  A    (strobe)
$dff08c  copins   W  A
$dff08e  diwstrt  W  A
$dff090  diwstop  W  A
$dff092  ddfstrt  W  A
$dff094  ddfstop  W  A
$dff096  dmacon   W  ADP
$dff098  clxcon   W  D
$dff09a  intena   W  P
$dff09c  intreq   W  P
$dff09e  adkcon   W  P
$dff0a0  aud[4]   W  AP   (audio channels 0-3)
$dff0e0  bplpt[6] W  A    (bitplane pointers)
$dff100  bplcon0  W  AD
$dff102  bplcon1  W  D
$dff104  bplcon2  W  D
$dff108  bpl1mod  W  A
$dff10a  bpl2mod  W  A
$dff110  bpldat[6] W D
$dff120  sprpt[8] W  A    (sprite pointers)
$dff140  spr[8]   W  AD   (sprite data)
$dff180  color[32] W D
```

(Guru Book, section 7.2.8, near line 9117--9191)

Note the book also documents:

> The symbols for the strobe registers strequ, strvbl, strhor, and strlong as well as the
> symbol dsksync are missing.
> (Guru Book, near line 9189)

And the critical compatibility rule about reading/writing registers:

> Reading write-only registers (cf. section 2.7.5), writing to read-only registers, and
> accessing undefined registers (cf. section 11.3) is not supported, in particular by the
> use of MOVEM across undefined custom-chip registers to save a few cycles when
> programming the blitter. Undefined bit positions should be ignored (masked out)
> when read and written as zeroes.
> (Guru Book, section 9.1, near line 9606)

**Verdict:** No reset state table. But the chip-ownership and access-type annotations
are useful for emulator implementation.

**Supplements:** `amiga-hardware-reference.md`

---

## 5. CIA Timer Edge Cases

### Gap
Timer reload timing (underflow cycle vs next), cascade race conditions, TOD alarm
match timing, ICR read-clear races.

### What the Guru Book Says

**PARTIALLY COVERED.** The Guru Book covers CIA timer usage at the system-software
level but does not document the hardware edge cases (reload timing, cascade races,
ICR read-clear races). It does provide some useful context.

#### CIA Register Map

The book provides the complete CIA register address map:

```
CIA-A (odd addresses)          CIA-B (even addresses)
$bfe001  ciapra                $bfd000  ciabpra
$bfe101  ciaaprb               $bfd100  ciabprb
$bfe201  ciaaddra              $bfd200  ciabddra
$bfe301  ciaaddrb              $bfd300  ciabddrb
$bfe401  ciaatalo              $bfd400  ciabtalo
$bfe501  ciaatahi              $bfd500  ciabtahi
$bfe601  ciaatblo              $bfd600  ciabtblo
$bfe701  ciaatbhi              $bfd700  ciabtbhi
$bfe801  ciaatodlow            $bfd800  ciabtodlow
$bfe901  ciaatodmid            $bfd900  ciabtodmid
$bfea01  ciaatodhi             $bfda00  ciabtodhi
$bfec01  ciaasdr               $bfdc00  ciabsdr
$bfed01  ciaaicr               $bfdd00  ciabicr
$bfee01  ciaacra               $bfde00  ciabcra
$bfef01  ciaacrb               $bfdf00  ciabcrb
```

(Guru Book, section 7.2.7, near line 9090)

#### Timer Allocation and the "Jumpy Timer" Kludge

> By default, the CIA-B timers are available for the programmer's own use. To
> be allowed to alter these bits, AddICRVector() must be used to gain control over the
> associated interrupt bits of the ciab.resource.
> (Guru Book, section 2.3.10, near line 2602)

> As of Kickstart 2.0, a special compatibility kludge will cause the timer.device, which
> uses CIA-A's timerB by default, to "jump" from CIA-A to the unused CIA-B timer
> (or vice versa) if AddICRVector() is used to properly allocate the timer that is currently
> in use.
> (Guru Book, section 2.3.10, near line 2606)

Footnote 22 refers to this as "Jumpy the Magic Timer Device".

#### CIA Clock Speed

> NTSC and PAL Amigas differ slightly in their color clock frequency, which also controls
> the processor speed in a standard A1000, A500, A2000, and A600, and from which the
> so-called E-clock is also derived.
>
> ```c
> #define CLOCK_NTSC 28636360
> #define CLOCK_PAL  28375160
>
> masterClock = GfxBase->DisplayFlags & NTSC ? CLOCK_NTSC :
>               GfxBase->DisplayFlags & PAL  ? CLOCK_PAL  : 0;
>
> eClock = SysBase->LibNode.lib_Version < 36 ?
>          masterClock / 40 : SysBase->ex_EClockFrequency;
> ```
>
> masterClock is useful primarily when dealing with the custom chips, and eClock is
> the input clock of the CIA chips. The color clock is an eighth of the masterClock.
> (Guru Book, section 2.5.1, near line 2878)

This gives us:
- NTSC E-clock: 28636360 / 40 = 715909 Hz
- PAL E-clock: 28375160 / 40 = 709379 Hz
- Color clock (NTSC): 28636360 / 8 = 3579545 Hz
- Color clock (PAL): 28375160 / 8 = 3546895 Hz

The book also provides serial clock constants:

```c
#define SERCLK_NTSC 3579545
#define SERCLK_PAL  3546895
```

(Guru Book, near line 9259)

#### TOD Clock Chip vs CIAs

> Since the clock chip, unlike the timer and the TOD of the CIAs, cannot transfer
> the current time to a temporary storage area (latch), the two bits Hold and Busy must
> be used in order to prevent a carry during reading or setting of the clock.
> (Guru Book, section 9.3, near line 9869)

This confirms that the CIAs DO have latching on TOD reads (as per the 8520
datasheet), in contrast to the OKI real-time clock.

**Verdict:** The edge cases (reload timing, cascade races, ICR read-clear behavior)
are NOT documented here. The 8520 datasheet is the primary source. The Guru Book
adds CIA allocation protocol, clock speed constants, and the timer-jumping kludge.

**Supplements:** `amiga-cia-8520-datasheet.md`

---

## 6. Copper Timing Edge Cases

### Gap
Exact wake-up delay after WAIT, MOVE-to-register latency, COPJMP strobe timing.

### What the Guru Book Says

**NOT COVERED.** The Guru Book mentions the Copper only in passing -- copper list
allocation, MEMF_CHIP requirements, and compatibility warnings. It does not discuss
Copper timing, WAIT wake-up delays, MOVE latency, or COPJMP strobe timing.

The closest relevant content:

> modifying copper lists directly and at constant offsets instead of us-
> ing the macros provided by <graphics/gfxmacros.h> or other graphics.library functions,
> which automatically take care of differences for instance between the original graphics
> chips and the ECS
> (Guru Book, section 2.1.3, near line 1977)

On copper list memory:

> Only the final (hardware) copper lists need to be stored in MEMF_CHIP, of course.
> (Guru Book, section 2.4.1, near line 2726)

The register map confirms:

```
$dff080  cop1lc   W  A    (Copper list 1 pointer)
$dff084  cop2lc   W  A    (Copper list 2 pointer)
$dff088  copjmp1  S  A    (strobe -- Copper jump 1)
$dff08a  copjmp2  S  A    (strobe -- Copper jump 2)
$dff08c  copins   W  A    (Copper instruction fetch)
$dff02e  copcon   W  A    (Copper control)
```

**Verdict:** This gap remains unfilled. The HRM and empirical testing are the sources
for Copper timing edge cases.

**Supplements:** `amiga-hardware-reference.md`, `amiga-cycle-accurate.md`

---

## 7. Blitter Timing Details

### Gap
Exact cycle count formulas, pipeline depth, nasty mode exact steal count.

### What the Guru Book Says

**PARTIALLY COVERED.** The Guru Book covers blitter usage protocol extensively but
does not provide cycle count formulas, pipeline depth, or nasty mode steal counts.

#### Blitter Ownership Protocol

> An important note about OwnBlitter(): unlike other functions, such as ObtainSema-
> phore(), calls to OwnBlitter() must not be nested! Calling this function twice without
> releasing the blitter in between will cause the calling task to hang!
> (Guru Book, section 2.3.4, near line 2419)

#### Overlapping Blitter/CPU Operation

> Yet another important point to remember is that the use of the blitter is usually
> performed overlapping with the CPU, and so -- on return from OwnBlitter() -- blits
> could still be in progress. The function Draw(), for example, will release the blitter and
> return as soon as the last of possibly several blits has been started. Therefore -- even
> though a subsequent OwnBlitter() may already have returned -- , WaitBlit() must be
> called before any blitter registers are changed.
> (Guru Book, section 2.3.4, near line 2424)

#### WaitBlit() is a Busy Loop

> Since WaitBlit() is used primarily in conjunction with small blits that do not take
> too long, it is currently implemented by way of a busy loop, as otherwise the overhead
> for two context switches would by far outweigh the time actually required for the blit.
> (Guru Book, section 2.3.4, near line 2451)

#### QBlit() / QBSBlit() Queued Blitter Operations

> These functions are passed a pointer to an initialized bltnode structure, which contains
> a pointer to a function to be called to perform the actual blit. This function is passed a
> pointer to the custom-chip registers in A0 and a pointer to the bltnode structure [...]
> in A1. Once the function pointed to by the bltnode has been called, it owns
> the blitter; no blits are in progress upon entry into the bltnode function; OwnBlitter()
> is neither required nor allowed!
> (Guru Book, section 2.3.4, near line 2455)

> The function associated with the current bltnode will remain in control of the blitter
> until it releases it voluntarily by returning from the blit function with the Z flag set
> to indicate that it is done with the blitter (the current blit may still be in progress, of
> course). If it returns with the Z flag clear, then the blit function will be called again as
> soon as the blit just started has finished.
> (Guru Book, section 2.3.4, near line 2463)

> Beam-synchronized bltnodes (QBSBlit()) take precedence over normal bltnodes
> (QBlit()). OwnBlitter() requests have the lowest priority and will be serviced only if
> no bltnodes are waiting. Once the blitter is owned, however, it will not be taken away,
> even if higher-priority requests arrive in the meantime.
> (Guru Book, section 2.3.4, near line 2470)

#### DMA Slot Allocation and the 14 kHz Limit

> As DMA slots are allocated on a per-scan-line basis, this limit may vary with the
> programmable scan rates as implemented in the ECS.
> (Guru Book, footnote 18, near line 2482)

#### WaitBlit() Hardware Bug

The book references a known WaitBlit() hardware bug:

> the OS will also attempt to fix hardware bugs transparently this way; popular example:
> WaitBlit() (cf. [8, page 186 f.] and [9, page 268]).
> (Guru Book, section 2.1.3, near line 2005)

This refers to the Agnus bug where BBUSY (blitter busy) can be read as 0 on the
first read after starting a blit -- the OS works around it by reading twice.

#### Multiprocessor/Chip Bus Arbitration

> there is the remote chance of the blitter (or any other Agnus DMA operation) altering
> the respective memory location between the two MPU accesses; Agnus does not (have
> to) arbitrate for the Chip bus, since it is in charge of this memory region by default
> anyway.
> (Guru Book, section 2.7.10, near line 3947)

**Verdict:** Good protocol-level coverage, the WaitBlit() bug reference is useful, but
cycle-level timing details are not here.

**Supplements:** `amiga-hardware-reference.md`, `amiga-cycle-accurate.md`

---

## 8. Sprite DMA Structure

### Gap
Which hpos each sprite's DMA slots fall on, when CTL/POS/DATA/DATB load relative
to display.

### What the Guru Book Says

**NOT COVERED.** The Guru Book does not discuss sprite DMA slot positions or timing.

The register map confirms the sprite registers:

```
$dff120  sprpt[8]    W  A    (8 sprite pointers, 2 words each)
$dff140  spr[8]      W  AD   (8 sprite definitions: pos/ctl/data/datb)
```

The book's MEMF_CHIP list confirms sprites require chip memory:

> MEMF_CHIP is the type of memory that the custom chips can access by means of DMA
> through Agnus. It is required for: [...] sprites
> (Guru Book, section 2.4.1, near line 2710)

**Verdict:** This gap remains unfilled. The HRM and hardware testing are the sources.

**Supplements:** `amiga-hardware-reference.md`, `amiga-cycle-accurate.md`

---

## 9. Audio State Machine

### Gap
Paula's internal per-channel states, when interrupts fire relative to DMA, the "first
word discarded" pipeline.

### What the Guru Book Says

**NOT COVERED.** The Guru Book does not document the audio state machine, interrupt
timing, or the discard pipeline.

The book does document:

- Audio data must be in MEMF_CHIP (section 2.4.1)
- The 14 kHz DMA limit and its variation with ECS scan rates (section 2.3.5)
- The ADCMD_LOCK protocol for direct hardware access (section 2.3.5)
- The audio.device is not initialized automatically at start-up in 2.0 (section 10.3)

On direct audio hardware access:

> In many situations, direct access to the audio hardware is necessary, as the audio.device
> may not provide all the desired functionality. Supposing one wishes to modulate one
> audio channel by way of a second (which the hardware is quite capable of, but which is
> not supported by the present system software), or one wishes to get by the well-known
> 14-kHz audio limit by "feeding" the audio registers directly using the processor, i.e.
> without DMA -- for all of these cases, the command ADCMD_LOCK has been provided
> to prevent clashes with regular clients of the audio.device.
> (Guru Book, section 2.3.5, near line 2477)

The register map shows audio channels at `$dff0a0` (4 channels, AudChannel struct).

**Verdict:** This gap remains unfilled. The HRM audio chapter and the patent documents
are the sources for the state machine.

**Supplements:** `amiga-io-audio-expansion.md`

---

## 10. DRAM Refresh

### Gap
Refresh slot positions, how many per line, interaction with DMA.

### What the Guru Book Says

**PARTIALLY COVERED.** The book documents that DRAM refresh exists, who controls
it, and its effect on CPU access, but not the specific slot positions or counts.

> Fat Agnus is also responsible -- besides custom-chip DMA -- for the RAM refresh in
> this area. All custom-chip DMA and refresh activities will lock out the processor from
> this section of memory. This is the reason why it is also called "Half-Fast memory".
> Only in an A2000A is this kind of memory real Fast memory.
> (Guru Book, section 9.1, near line 9658)

This tells us:
- Agnus handles DRAM refresh (not an external refresh controller)
- Refresh locks out the CPU from Chip memory, same as DMA
- Ranger memory ($00C00000-$00D7FFFF) is also refreshed by Agnus on A500/A2000B
  without ECS Agnus -- making it "Half-Fast"
- On A2000A, Ranger memory has independent refresh (real Fast memory)

The `refptr` register is listed in the custom chip map:

```
$dff028  refptr   W  A   (early write, Agnus)
```

**Verdict:** Architectural facts covered (who refreshes, bus contention effects), but
slot positions and per-line counts are NOT here.

**Supplements:** `amiga-cycle-accurate.md`, `amiga-hardware-reference.md`

---

## 11. Address Error / Bus Error Behaviour

### Gap
What causes bus errors on the Amiga specifically, what the CPU does, how Exec handles
them.

### What the Guru Book Says

**WELL COVERED.** This is one of the better-documented topics.

#### Exception Vector Table

The book provides the complete M68000-family exception vector table (near line 12183):

```
Vector  Offset  Description
0       $00     Reset: initial value for SSP (MC68020/030/040: ISP)
1       $01     Reset: initial value for PC (program counter)
2t      $02     Bus error (Amiga: usually only with an MC68851/030/040)
3t      $03     Address error (MC68020/030/040: applies to odd PC only)
4t      $04     Unimplemented instruction
...
24+     $18     Spurious interrupt (bus error during interrupt acknowledge)
25      $19     Level-1 interrupt autovector (SOFTINT, DSKBLK, TBE)
26      $1A     Level-2 interrupt autovector (PORTS)
27      $1B     Level-3 interrupt autovector (COPER, VERTB, BLIT)
28      $1C     Level-4 interrupt autovector (AUD2, AUD0, AUD3, AUD1)
29      $1D     Level-5 interrupt autovector (RBF, DSKSYNC)
30      $1E     Level-6 interrupt autovector (EXTER)
31      $1F     Level-7 interrupt autovector (NMI)
```

The `t` and `+` markers indicate which exceptions are routed to the task's trap handler:

> Exceptions marked with a t or -- as of Kickstart 2.0 -- + in table 11.18 are considered
> synchronous and will be diverted to the address specified in tc_TrapCode
> (Guru Book, section 2.6.7, near line 3256)

#### Bus Error on the Amiga

> A bus error (cf. table 11.18, vector 2; see section 2.6.7 for an explanation of t and
> + as used in this table) may not just be generated by an MMU, but also by hardware
> external to the processor asserting BERR. The A3000, for example, makes use of this
> feature to trap illegal memory accesses, e.g. attempts to access a location for which no
> hardware responds. After the system has been initialized, this "bus time-out" resulting
> in a BERR is set to 250 milliseconds. When running the A3000 in 1.3 mode, the bus
> time-out is set to 8 ms and, for compatibility reasons, does not generate a bus error,
> i.e. accesses to nonexistent memory are simply ignored, as was the case in all previous
> Amiga models. Yet another option available in Gary is infinite time-out. Currently,
> this mode is not used.
> (Guru Book, section 11.3, near line 12230)

Key facts for emulators:
- **A500/A2000 (OCS/ECS, no Gary):** No bus error on access to nonexistent memory.
  Reads return floating bus data. No BERR is generated.
- **A3000 (with Gary):** Bus time-out generates BERR after 250 ms (2.0) or 8 ms
  (1.3 mode). 1.3 mode silently ignores the timeout for compatibility.
- **MMU-equipped machines:** Bus errors from MMU page faults are possible.

#### Address Error

> Address error (MC68020/030/040: applies to odd PC only)
> (Guru Book, table 11.18, vector 3)

On 68000, word/longword access to an odd address causes an address error. On
68020+, only odd PC values cause address errors (data accesses to odd addresses are
legal but slow).

#### How Exec Handles Exceptions

> Only in user mode will an exception be routed through a task's trap vector. Unex-
> pected exceptions in supervisor mode inevitably end up in the Alert() function -- Guru!
> (Guru Book, section 11.3, near line 12240)

> Traps that can be dealt with by a task's trap handler, however, result in each case
> from a concrete operation in the program code, for example a word access on an odd-
> numbered boundary. If a task does not provide its own code to handle these exceptions,
> then it will normally be aborted, usually radically in the form of a Guru.
> (Guru Book, section 11.3, near line 12245)

> Upon the occurrence of a trap that can be dealt with by a task, the associated
> function stored in tc_TrapCode will be called; tc_TrapData is not currently used, although
> some trap handlers use it to store private data. Execution occurs in supervisor mode;
> the number of the exception vector taken can be found in the longword stored on the top
> of the stack. It must be removed from the stack before returning from a trap handler
> using the RTE instruction.
> (Guru Book, section 11.3, near line 12249)

> As of Kickstart 2.0, all types of Gurus, alerts, and exceptions will cause the contents
> of registers D0 to D7 and A0 to A7 (in that order) to be dumped to memory starting at
> location $00000180. This can be useful during debugging, e.g. when using ROM-Wack.
> (Guru Book, section 11.3, near line 12256)

#### Spurious Interrupt

> Spurious interrupt (bus error during interrupt acknowledge)
> (Guru Book, vector 24)

This occurs when a bus error happens during the interrupt acknowledge cycle. On the
Amiga, this can happen with external (Zorro bus) interrupts.

#### Interrupt Levels and Bus Errors

> Since interrupt levels 1, 4, 5, and 7 (NMI) -- when generated by external hardware
> on the Zorro-II bus -- do not pass through the interrupt controller logic included in
> Paula, it is possible for any of these interrupts to break a Disable() and to find the
> system in an inconsistent state. Although this has been fixed by external circuitry in
> the A3000 and the A3000T (Fat Gary and U701), the use of these interrupts, when
> generated externally instead of by Paula, is not supported under the Amiga's native OS.
> (Guru Book, section 2.6.5, near line 3235)

#### Start-up Bus Errors

> yellow (RGB==$FE5; pre-2.0: RGB==$CC0) -- an unexpected processor exception oc-
> curred during the initialization of the system, i.e. before the Guru was prepared.
> This color code can, by the way, also occur if the reset routine is entered from
> outside the supervisor mode (e.g. by a direct jump). On the A3000, this may be
> an indicator of defective hardware, as accessing "nonexistent" memory (or faulty
> hardware) may result in a bus-error exception.
> (Guru Book, section 9.4.1, near line 9955)

**Supplements:** `amiga-exec-kernel.md`, `amiga-68000-timing.md`

---

## 12. Interrupt Latency

### Gap
Worst-case interrupt response time on the Amiga (CIA -> Paula -> 68000 autovector ->
handler).

### What the Guru Book Says

**PARTIALLY COVERED.** The book discusses interrupt latency in the context of
performance optimization but does not provide a worst-case timing analysis.

#### VBR Improves Interrupt Latency

> [the capability of moving the exception vector table] is taken advantage
> of by the 2.0 CPU command to reduce interrupt latency, since -- when accessing these
> vectors -- the CPU will no longer have to wait for the Chip memory (possibly tied up
> by DMA activities) to be released. A more reliable and stable throughput, for instance
> in the case of high baud rates over the serial port, is one of the possible consequences.
> (Guru Book, section 9.2.2, near line 9691)

This tells us:
- Exception vector table access competes with DMA for Chip memory bus
- On 68000 (no VBR), vector table is always at address 0 in Chip memory
- DMA contention during interrupt acknowledge adds latency
- Moving vectors to Fast memory (via VBR on 68010+) eliminates this contention

#### Disable() Time Limit

> The maximum time a Disable() is allowed to be in effect is 250 microseconds. Loss of
> characters on the serial port is just one of the effects if this rule is not followed.
> (Guru Book, section 2.6.8, near line 3288)

This 250 us figure is relevant because Disable() blocks ALL interrupts, so 250 us
is effectively the maximum permitted added latency from software.

#### Interrupt Register Conventions

The book documents what registers contain on entry to interrupt handlers:

```
D0: scratch
D1: active interrupts (INTREQ AND INTENA) -- handlers only
A0: pointer to Custom structure -- handlers only!
A1: is_Data
A5: is_Code
A6: SysBase -- handlers and software interrupts only!
```

(Guru Book, section 2.6.5, near line 3170)

#### Interrupt Priority

The book documents the Amiga's interrupt level assignments:

| Level | Sources | Type |
|-------|---------|------|
| 1 | SOFTINT, DSKBLK, TBE | Server chain |
| 2 | PORTS (CIA-A) | Server chain |
| 3 | COPER, VERTB, BLIT | Server chain (VERTB), Handler (COPER, BLIT) |
| 4 | AUD2, AUD0, AUD3, AUD1 | Handler |
| 5 | RBF, DSKSYNC | Handler |
| 6 | EXTER (CIA-B) | Server chain |
| 7 | NMI | Special |

**Verdict:** Useful context but no actual worst-case latency calculation. The latency
depends on: Disable() state, DMA contention for vector fetch, current instruction
execution time, and interrupt handler chain processing time.

**Supplements:** `amiga-exec-kernel.md`, `amiga-cycle-accurate.md`

---

## 13. DMA Time Slot Table

### Gap
Per-colour-clock slot allocation table (the famous "DMA slot diagram").

### What the Guru Book Says

**NOT COVERED.** The Guru Book does not include a DMA slot allocation diagram.

The only direct reference to DMA slot allocation:

> As DMA slots are allocated on a per-scan-line basis, this limit may vary with the
> programmable scan rates as implemented in the ECS.
> (Guru Book, footnote 18, near line 2482)

And the general DMA/bus contention statement:

> All custom-chip DMA and refresh activities will lock out the processor from this
> section of memory.
> (Guru Book, section 9.1, near line 9659)

The book does list what requires MEMF_CHIP (and thus DMA):
- audio data
- blitter data regions
- copper instructions
- disk DMA
- bit planes
- sprites

(Guru Book, section 2.4.1, near line 2713)

**Verdict:** This gap remains unfilled. The HRM appendix and various hardware
references are the sources for the DMA slot diagram.

**Supplements:** `amiga-cycle-accurate.md`

---

## 14. Other Hardware Details Not in HRM

### What the Guru Book Provides

The Guru Book is primarily a system-software reference (OS internals, AmigaDOS,
programming guidelines) rather than a hardware reference. However, it contains
several hardware-adjacent details that are useful for emulator authors.

#### 14a. CLR Instruction and Write-Only Registers (MC68000 Bug)

> On the MC68000 (and the MC68008) only, using the CLR instruction to zero a memory
> location causes the respective location to be read first before the processor will actually
> write a zero value to it. All later members of the M68000 family will perform only
> the write access. Although this difference in behavior does not matter with respect
> to normal RAM locations, it may well cause problems when trying to clear an I/O
> location, as reading a register first may result in undesired side effects for instance with
> write-only custom-chip registers, as reading a write-only register may be equivalent to
> writing a random value.
> (Guru Book, section 2.7.5, near line 3670)

This is critical for accurate 68000 emulation: CLR.W on a write-only custom chip
register will perform a spurious read cycle first.

#### 14b. TAS Instruction and Bus Arbitration

> Because of the delicate bus timing on the Amiga (processor, custom chips, DMA), com-
> mands with indivisible read-modify-write cycles should not be used. This applies not
> just to Chip memory (where proper functioning cannot be guaranteed in any event),
> but also to Zorro-II RAM expansions, since many hardware designers did not take this
> special case -- the read-modify-write cycle used for 68000 bus locking is radically dif-
> ferent from normal 68000 bus cycles -- into consideration. Zorro-III machines properly
> support these cycles, although -- for the reasons mentioned above -- not in Chip mem-
> ory either, since -- although it may seem to work -- there is the remote chance of the
> blitter (or any other Agnus DMA operation) altering the respective memory location
> between the two MPU accesses; Agnus does not (have to) arbitrate for the Chip bus,
> since it is in charge of this memory region by default anyway.
> (Guru Book, section 2.7.10, near line 3939)

Key insight: **Agnus owns the Chip memory bus and does not arbitrate.** The TAS
instruction's read-modify-write cycle is NOT atomic with respect to Agnus DMA.
This means:
- TAS to Chip memory is unreliable (Agnus can interleave DMA between the read
  and write phases)
- Many Zorro-II cards also don't handle TAS correctly
- CAS and CAS2 on 68020/030/040 have the same problem

#### 14c. Data Caching and DMA Coherency

> For historical reasons -- none of the users of Agnus DMA use the above protocol --,
> data in Chip memory must never be cached, although instruction caching is possible.
> (Guru Book, section 2.7.6.2, near line 3862)

> MC68030-based accelerator boards should disable data caching in Chip memory
> ($00000000-$001FFFFF) and I/O space ($00A00000-$00BFFFFF and $00DC0000-
> $00EFFFFF) by hardware.
> (Guru Book, section 9.1, near line 9648)

#### 14d. MOVE from SR is Privileged on 68010+

> The instructions MOVE from SR and MOVE from CCR should not be used. On the
> MC68010, MC68012, MC68020, MC68030, MC68040, and later processors, MOVE from
> SR is a privileged instruction and may be used in supervisor mode only -- as opposed
> to its use on the MC68000 (and MC68008). On the other hand, the MC68000 (and
> MC68008) do not recognize the MOVE from CCR instruction.
> (Guru Book, section 2.7.2, near line 3548)

Exec handles this by providing GetCC() which works regardless of processor and
execution mode.

#### 14e. Volatile and Custom Chip Registers

> Among other things, vposr (along with vhposr, hence the ULONG in the reference
> above) contains the line number of the current vertical beam position; its contents can
> change without an explicit assignment taking place. Without volatile, a compiler would
> be allowed to optimize the function above to behave like the OS version.
> (Guru Book, section 6.3.3, near line 6343)

> The suppression of an optimization also applies to write accesses. Therefore, seem-
> ingly unnecessary assignments to a volatile object must not be discarded by the compiler,
> since they might refer to an I/O control register. A good example of this on the Amiga
> is the instruction that starts floppy-disk DMA: for that purpose, the same register has
> to be written to twice.
> (Guru Book, section 6.3.3, near line 6347)

This confirms that DSKLEN must be written twice to start disk DMA (a safety
mechanism to prevent accidental DMA starts).

#### 14f. Chip Memory Address Ranges by Machine

The book provides the definitive Chip memory ranges:

| Range | Description |
|-------|-------------|
| $000000-$03FFFF | 256 KB Chip -- standard on all Amigas |
| $040000-$07FFFF | 256 KB Chip -- optional A1000, standard others |
| $080000-$0FFFFF | 512 KB Chip -- optional A500, standard A500+/A600/A2000B(ECS)/A3000 |
| $100000-$1FFFFF | 1024 KB Chip -- maximum on A500+/A600/A3000 (total 2 MB) |

And the I/O space:

| Range | Description |
|-------|-------------|
| $A00000-$A7FFFF | PCMCIA (A600 only) |
| $BFDF00-$BFDE00 | CIA-B (even addresses only) |
| $BFE001-$BFEE01 | CIA-A (odd addresses only) |
| $C00000-$D7FFFF | Ranger memory (A500/A2000 only) |
| $DA0000-$DAFFFF | Gayle registers (A600 only) |
| $DC0000-$DCFFFF | Battery-backed RTC |
| $DD0000-$DDFFFF | Super-DMAC/SCSI (A3000 only) |
| $DE0000-$DEFFFF | Fat Gary/Ramsey (A3000); Gayle (A600) |
| $DFF000-$DFFFFF | Custom chips (Agnus, Denise, Paula) |
| $E80000-$E8FFFF | Zorro-II autoconfig |
| $F00000-$F7FFFF | Cartridge ROM |
| $F80000-$F8FFFF | Boot ROM (A1000 only) |
| $F80000-$FFFFFF | 512 KB Kickstart ROM (non-A1000) |
| $FC0000-$FFFFFF | 256 KB Kickstart ROM (pre-2.0 / A1000 WCS) |

(Guru Book, table 9.1, near line 9549)

#### 14g. Boot ROM Visibility (A1000)

> The A1000's boot ROMs are located starting at address $00F80000. They are made
> invisible by any write access in that memory range, and they will reappear after a reset,
> either by software (RESET instruction) or by hardware (power-on or keyboard reset).
> The Kickstart WCS remains writable for as long as the boot ROMs are visible. After
> reset, the ROM image is obviously also located at location $00000000 to provide a valid
> initial value for the program counter. Setting the CIA overlay bit (CIAF_OVERLAY) to
> the high state replaces this image by the Chip memory usually found in that area.
> (Guru Book, section 9.2.3, near line 9806)

Key A1000 emulation details:
- Boot ROM at $F80000, hidden by ANY write to that range
- Reappears only after reset
- Kickstart WCS is writable only while boot ROMs are visible
- CIAF_OVERLAY controls the $000000 overlay (ROM vs Chip RAM)

#### 14h. Kickstart ROM Size and Version Detection

> The size (measured in bytes) of the Kickstart region can be found in the longword
> at address $00FFFFEC. If this value is subtracted from $01000000 (the value of the
> ROM end address plus one), the result is the start address of the Kickstart ROM.
> (Guru Book, section 9.2.3, near line 9706)

> Two other numbers can be found relative to the start of the Kickstart ROM thus
> calculated: the word at offset 12 ($C) is the version number of the Kickstart ROM and
> the word following it at offset 14 ($E) the revision number of that version.
> (Guru Book, section 9.2.3, near line 9709)

#### 14i. VBR and Interrupt Response Improvement

> [The capability of moving the exception vector table] is taken advantage
> of by the 2.0 CPU command to reduce interrupt latency, since -- when accessing these
> vectors -- the CPU will no longer have to wait for the Chip memory (possibly tied up
> by DMA activities) to be released.
> (Guru Book, section 9.2.2, near line 9691)

This is an important detail: on 68000 without VBR, every interrupt requires
fetching the vector from Chip memory, which is subject to DMA contention.

#### 14j. VERTB Interrupt Server Bug (Pre-2.0)

> Owing to a bug in the pre-2.0 version of the graphics.library (the register conventions
> for a server were confused with those of a handler), in the case of a VERTB server with
> a priority higher than or equal to 10, the contents of register A0 must have the same
> value upon exit as upon entry. Since certain third-party servers also make this false
> assumption, it is a good idea always to preserve the contents of A0.
> (Guru Book, section 2.6.5, near line 3179)

#### 14k. External Interrupts Bypass Disable()

> Since interrupt levels 1, 4, 5, and 7 (NMI) -- when generated by external hardware
> on the Zorro-II bus -- do not pass through the interrupt controller logic included in
> Paula, it is possible for any of these interrupts to break a Disable() and to find the
> system in an inconsistent state.
> (Guru Book, section 2.6.5, near line 3235)

This is because Disable() works by masking Paula's INTENA register. External
hardware that generates interrupts directly (bypassing Paula) is not affected.
The A3000's Fat Gary and U701 fix this with external gating logic.

#### 14l. Guru Register Dump Location

> As of Kickstart 2.0, all types of Gurus, alerts, and exceptions will cause the contents
> of registers D0 to D7 and A0 to A7 (in that order) to be dumped to memory starting at
> location $00000180.
> (Guru Book, section 11.3, near line 12256)

#### 14m. Start-up Diagnostic Color Codes

The book documents the diagnostic screen colors during boot:

| Color | RGB | Meaning |
|-------|-----|---------|
| Turquoise | $0CC | RAM failure in Kickstart WCS (A1000 only) |
| Green | $0F0 (2.0) / $0C0 (pre-2.0) | Error in lowest 256 KB of Chip memory |
| Yellow | $FE5 (2.0) / $CC0 (pre-2.0) | Unexpected processor exception during init |
| Red | $F00 | Invalid Kickstart ROM checksum (2.0 only) |
| Magenta | $F0F | RTF_SINGLETASK or RTF_COLDSTART init failed (2.0 only) |

(Guru Book, section 9.4.1, near line 9949)

> Colors displayed before the dark gray screen comes up during start-up are simply what
> has been left in the custom-chip registers and do not indicate a failure.

#### 14n. Keyboard Self-Test Error Codes

> - 1 blink: keyboard ROM checksum error;
> - 2 blinks: RAM failure;
> - 3 blinks: watchdog-timer failure;
> - 4 blinks: short circuit in the keyboard matrix between the keyboard lines or
>   between any of the independent qualifier keys.
> (Guru Book, section 9.4.2, near line 10009)

#### 14o. Real-Time Clock (OKI MSM6242RS) Structure

The book provides the complete register layout of the battery-backed RTC
(at $DC0000, registers separated by longwords):

```c
struct ClockChip /* address $00DC0000 */
{
    Second1   : 4;  /* 0..9 */
    Second10  : 3;  /* 0..5 */
    Minute1   : 4;  /* 0..9 */
    Minute10  : 3;  /* 0..5 */
    Hour1     : 4;  /* 0..9 */
    AMPM      : 1;
    Hour10    : 2;
    Day1      : 4;  /* 0..9 */
    Day10     : 2;  /* 0..3 */
    Month1    : 4;  /* 0..9 */
    Month10   : 1;  /* 0..1 */
    Year1     : 4;  /* 0..9 */
    Year10    : 4;  /* 0..9 */
    Week      : 3;  /* 0..6, not used by system */
    /* Control register D */
    Adj30     : 1;
    IRQ       : 1;
    Busy      : 1;  /* read-only: carry in progress */
    Hold      : 1;  /* set before read/write, clear after */
    /* Control register E */
    Time      : 2;
    IntStd    : 1;
    Mask      : 1;
    /* Control register F */
    Test      : 1;
    TwentyFour: 1;  /* 0 = 12h, 1 = 24h */
    Stop      : 1;
    Reset     : 1;  /* hold to zero sub-second counter */
};
```

> the two bits Hold and Busy must be used in order to prevent a carry during reading
> or setting of the clock. This is accomplished by setting the Hold bit before a read or
> write operation and then waiting until the Busy bit is cleared, which is guaranteed to
> happen no later than 190 us after the Hold bit has been set.
> (Guru Book, section 9.3, near line 9870)

On some very old A2000 machines, the clock is at $D80000 instead of $DC0000.

#### 14p. A1000 Boot ROM Audio Test (Siegfried's Horn Motif)

The A1000 boot ROM plays a melody as a diagnostic:

> This consists (more or less) of the Horn Motif from Richard Wagner's "Siegfried",
> the third part of the "Ring des Nibelungen", transposed four half-tones down.
> (Guru Book, section 9.4.1, near line 9977)

| Wagner Pitch | Duration | Boot ROM Pitch | Channel |
|-------------|----------|---------------|---------|
| f1 | 1/8 | c1# | 0 |
| c2 | 3/16 | g1# | 3 |
| a1 | 1/16 | f1 | 0 |
| f1 | 1/8 | c1# | 3 |
| g1 | 1/8 | d1# | 0 |
| a1 | 1/8 | f1 | 1 |
| a1# | 1/8 | f1# | 2 |
| a1 | 1/8 | f1 | 1 |
| g1 | 1/8 | d1# | 2 |
| c2 | 9/8 | d1# | 1 |

(Guru Book, table 9.2, near line 9986)

#### 14q. MC68030 Write-Allocation Cache Bug

> Enabling write-allocation has an undesired side effect, though: as the CIIN signal
> is ignored during write-accesses, a longword-aligned write of a longword will create a
> valid cache entry that may cause a cache hit during subsequent read accesses, regardless
> of the caching mode for that memory location as indicated by the external hardware.
> This behavior can be corrected only by using the MMU to specify caching modes on a
> page-by-page basis.
> (Guru Book, section 2.7.6.3, near line 3882)

This is relevant for emulating accelerated Amigas with 68030 processors.

#### 14r. NOP Required for Serialization on 68040

> Software relying on serialized memory accesses may have to use the NOP instruction,
> as the MC68040 does not guarantee serialization unless the respective locations are
> located within the same MMU page.
> (Guru Book, section 9.1, near line 9637)

**Supplements:** `amiga-hardware-reference.md`, `amiga-aga-and-chip-revisions.md`,
`amiga-68000-timing.md`, `amiga-service-electrical.md`, `amiga-boot-process.md`

---

## Bonus Finds

These are items not in the 14-gap list but potentially useful for emulator development.

### B1. VPOS Read Macro (ECS-Aware)

```c
#define VPOS(vposr) ((vposr) >> 8 & 0x7ff) /* ECS */
```

(Guru Book, near line 6317)

The `0x7ff` mask indicates the ECS VPOSR can encode up to 2047 lines (11 bits),
compared to the OCS 9-bit value.

### B2. DSKLEN Double-Write Safety Mechanism

The book confirms that starting floppy DMA requires writing DSKLEN twice:

> the instruction that starts floppy-disk DMA: for that purpose, the same register has
> to be written to twice.
> (Guru Book, near line 6350)

### B3. Interrupt Server Chain Termination

> the processing of interrupt server chains (those maintained by AddIntServer()) is
> aborted if the Z flag is clear upon return from an interrupt server. This way, a server
> may speed up interrupt processing by clearing this flag if (and only if) an interrupt is
> private to that particular interrupt server.
>
> VERTB servers should therefore always return with the Z flag set.
> (Guru Book, section 2.6.5, near line 3186)

### B4. Master Clock Constants

For precise emulator timing:

```
NTSC Master Clock:  28,636,360 Hz
PAL Master Clock:   28,375,160 Hz
Color Clock:        masterClock / 8
E-Clock (CIA):      masterClock / 40
Serial Clock:       masterClock / 8  (= 3,579,545 Hz NTSC / 3,546,895 Hz PAL)
```

(Guru Book, section 2.5.1, near line 2885 and section 9.2.2, near line 9259)

### B5. Ranger Memory ("Half-Fast") Details

Ranger memory ($C00000-$D7FFFF) on A500/A2000B without ECS Agnus:
- Agnus controls DRAM refresh for this region
- All custom-chip DMA and refresh lock out the CPU
- Called "Half-Fast memory" because it's technically Fast (no DMA) but
  CPU access is still throttled by Agnus refresh cycles
- On A2000A only, Ranger memory is truly Fast (independent refresh)

(Guru Book, section 9.1, near line 9652)

### B6. Potgo Resource Protocol

The book documents that the `potgo.resource` manages shared access to the POTGO
register ($DFF034), which controls analog paddle/button ports. Functions:
- `AllocPotBits()` / `FreePotBits()`

(Guru Book, section 2.3, near line 2385)

### B7. Filesystem Checksum vs Boot Block Checksum

Two different algorithms are used in the filesystem:

1. **Boot block checksum:** Ones' complement additive-carry sum to $FFFFFFFF.
   Stored at longword offset 1. Computed across ALL boot sectors.

2. **Standard block checksum:** Simple two's complement sum (mod 2^32).
   Negative sum of all other longwords. Stored at longword offset 5.
   Sum of all longwords including checksum = 0.

3. **Bitmap block checksum:** Same algorithm as #2, but stored at offset 0.

(Guru Book, sections 10.3, 15.3.3.3, 15.3.7.1)

### B8. Resident Module Scan Order (KickTags)

The book provides the complete list of Resident modules in both Kickstart 1.3 (v34)
and 2.0 (v37) ROMs, with their priorities and types. This is useful for emulating
the boot sequence in the correct order.

(Guru Book, section 10.2.2, tables near line 10367 and 10423)

### B9. PAL/NTSC Detection Bug

> This code segment assumes that the Agnus chip is jumpered to match the
> system's master oscillator, and -- with a PAL-Amiga under Kickstart 1.2 or 1.3 --
> that the graphics.library has determined the machine's video type correctly (which --
> owing to a bug in the graphics.library -- may or may not be the case, in particular on
> an accelerated Amiga).
> (Guru Book, section 2.5.1, near line 2903)

### B10. VERTB Frequency Not Fixed

> The VERTB frequency is not necessarily 60 Hz for NTSC or 50 Hz for PAL, as the
> ECS allows for dynamic scan rates. The power line frequency (PowerSupplyFrequency in
> the ExecBase structure) is not necessarily related to the VERTB frequency either.
> (Guru Book, section 2.5.1, near line 2908)

---

## Source Map

| Section | Line Range | Guru Book Chapter |
|---------|-----------|-------------------|
| Boot block | 10498-10588 | Ch. 10: Hooking in at Boot Time |
| Install2C.c listing | 11330-11494 | Listing: Install2C.c |
| Install2A.a listing | 11496-11640 | Listing: Install2A.a |
| Audio filter | 2496-2512 | Ch. 2: Programming Guidelines, sec. 2.3.6 |
| Blitter protocol | 2418-2474 | Ch. 2: Programming Guidelines, sec. 2.3.4 |
| Audio hardware | 2476-2494 | Ch. 2: Programming Guidelines, sec. 2.3.5 |
| CIA timers | 2597-2609 | Ch. 2: Programming Guidelines, sec. 2.3.10 |
| Disk resource | 2572-2595 | Ch. 2: Programming Guidelines, sec. 2.3.9 |
| NTSC/PAL clocks | 2877-2914 | Ch. 2: Programming Guidelines, sec. 2.5.1 |
| Delays and timing | 3505-3546 | Ch. 2: sec. 2.7.1 |
| CLR instruction | 3670-3681 | Ch. 2: sec. 2.7.5 |
| Data caching/DMA | 3788-3893 | Ch. 2: sec. 2.7.6 |
| TAS and bus | 3938-3958 | Ch. 2: sec. 2.7.10 |
| Interrupts | 3158-3241 | Ch. 2: sec. 2.6.5 |
| Exceptions/traps | 3255-3275 | Ch. 2: sec. 2.6.7 |
| CIA register symbols | 9082-9105 | Ch. 7: amiga.lib, sec. 7.2.7 |
| Custom chip reg syms | 9106-9191 | Ch. 7: amiga.lib, sec. 7.2.8 |
| Hardware memory map | 9549-9591 | Ch. 9: sec. 9.1 |
| Kickstart ROM | 9700-9811 | Ch. 9: sec. 9.2.3 |
| Real-time clock | 9814-9878 | Ch. 9: sec. 9.3 |
| Start-up diagnostics | 9940-10019 | Ch. 9: sec. 9.4 |
| Exception vectors | 12160-12258 | Ch. 11: sec. 11.3 |
| Block checksums | 17370-17377 | Ch. 15: sec. 15.3.3.3 |
| Bitmap checksum | 18015-18018 | Ch. 15: sec. 15.3.7.1 |
| Boot block layout | 18183-18235 | Ch. 15: sec. 15.3.9 |
| Serial clock const | 9259-9264 | Ch. 9: Serial baud rate |
