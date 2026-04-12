# Amiga Resources Reference

A focused reference for Amiga **resources** — the Exec objects that arbitrate
shared hardware. Resources sit below libraries and devices in the Exec
architecture: a resource doesn't implement I/O or provide a friendly API,
it simply hands out ownership of a piece of hardware so that competing
drivers can't trample each other.

This document complements:

- `amiga-hardware-reference.md` — the underlying custom-chip registers that
  resources arbitrate access to (CIA ICR, POTGO, disk DMA, etc.)
- `amiga-io-audio-expansion.md` — the high-level `.device` drivers that
  are the normal clients of these resources
- `amiga-headers-reference.md` — the full NDK header catalog

Primary sources are the NDK 3.9 autodocs in
`ndk/NDK_3.9/Documentation/Autodocs/` and the C headers in
`ndk/NDK_3.9/Include/include_h/resources/`. Function documentation is
quoted verbatim from the autodocs.

## Table of contents

1. [What is a resource?](#1-what-is-a-resource)
2. [The exec ResourceList API](#2-the-exec-resourcelist-api)
3. [cia.resource](#3-ciaresource)
4. [disk.resource](#4-diskresource)
5. [battclock.resource](#5-battclockresource)
6. [battmem.resource](#6-battmemresource)
7. [potgo.resource](#7-potgoresource)
8. [misc.resource](#8-miscresource)
9. [card.resource (PCMCIA)](#9-cardresource-pcmcia)
10. [FileSystem.resource](#10-filesystemresource)
11. [nonvolatile.library (CDTV/CD32 NVRAM)](#11-nonvolatilelibrary-cdtvcd32-nvram)
12. [lowlevel.library (V40 joypad/joystick/timer)](#12-lowlevellibrary-v40-joypadjoysticktimer)
13. [Resource boot order](#13-resource-boot-order)
14. [Resource use patterns](#14-resource-use-patterns)
15. [Appendix A: function index](#appendix-a-function-index)
16. [Appendix B: gaps](#appendix-b-gaps)
17. [Appendix C: source map](#appendix-c-source-map)

---

## 1. What is a resource?

A **resource** in AmigaOS is an Exec-managed object that represents, and
arbitrates access to, a single piece of shared hardware. Where a *library*
exposes a reusable API and a *device* exposes a standardised I/O request
protocol (via `DoIO`/`SendIO`), a *resource* has almost no conceptual
baggage at all: it is a named node on a list inside `ExecBase`, it has a
jump table of function vectors, and it exists so that two drivers don't
both try to program the same hardware register behind each other's backs.

### Library, device, resource — when to use which

| Layer        | Purpose                                       | Open with            | Close with    |
|--------------|-----------------------------------------------|----------------------|---------------|
| Library      | Reusable code API; reference counted          | `OpenLibrary`        | `CloseLibrary`|
| Device       | I/O request–based hardware driver             | `OpenDevice`         | `CloseDevice` |
| Resource     | Arbitration of a single piece of hardware     | `OpenResource`       | (none)        |

The most important difference between a resource and the other two is the
final column. **Resources are not reference counted.** `OpenResource()`
returns a plain pointer to the resource struct; there is no
`CloseResource()`, because a resource is assumed to exist for the life of
the system. You can use the pointer from any task after opening it,
without any teardown obligation. The autodoc is explicit:

> There is no CloseResource() function.
>
> — `exec.doc V45`, `/OpenResource`

The other defining property is the **single-owner-per-board** model. A
resource doesn't usually give you a "handle" that can be held by several
callers at once — it gives one caller exclusive ownership of a hardware
sub-resource (a CIA ICR bit, a disk unit, a POTGO pin) and the *next*
caller gets an error or has to wait. Arbitration is the whole point. The
resource itself has no state beyond "who owns what", and generally no
API beyond "claim this bit / release this bit / read or write it on the
current owner's behalf".

A few consequences of the no-reference-count model:

- A resource is **created once** at Kickstart boot (or by an early
  expansion driver) and is *never unloaded*. `RemResource()` exists but
  calling it is almost always a mistake on a running system, and it is
  undefined behaviour if anything still has ownership of a sub-resource.
- Ownership of a sub-resource *is* reference counted, but that's a
  property of the resource's internal data structures, not the Exec
  resource framework. For example, `cia.resource` tracks which tasks own
  each ICR bit; `disk.resource` tracks which task has the disk unit
  allocated; `card.resource` maintains a prioritised notification list.
- You must still clean up **ownership** on exit. If your task calls
  `AddICRVector()`, you must call `RemICRVector()` before exiting; if
  your task calls `AllocUnit()` on `disk.resource` it must call
  `FreeUnit()`. The resource itself persists, but the hardware it
  arbitrates does not care about task boundaries — if you leak a CIA
  timer the next owner cannot have it.

### The Exec `ResourceList`

Resources live on a named list inside `ExecBase` (`ResourceList`),
parallel to the `LibList` and `DeviceList` fields. There is no C-level
`struct Resource` definition in the NDK headers; each resource begins
with a `struct Library` (and therefore a `struct Node` at the head,
giving it an `ln_Name` for lookup) and then adds whatever private fields
it needs. The `AddResource` autodoc puts it directly:

> Resources currently have no system-imposed structure, however they
> must start with a standard named node (LN_SIZE), and should with
> a standard Library node (LIB_SIZE).
>
> — `exec.doc V45`, `/AddResource`

So a resource is, at the struct level, a `Library` with extra fields
tacked on the end and linked on a different list. You call its functions
exactly like library functions: load the resource base into `A6`, JSR
through a negative-offset vector. From C, the resource's `LVO`
declarations and inline stubs come out of `amiga.lib` / `clib/*_protos.h`
just like library ones.

### How resources differ from libraries in practice

1. **No version negotiation.** `OpenLibrary()` takes a minimum version;
   `OpenResource()` does not. Either the resource is present or it isn't.
   (You can still check the `lib_Version` of the returned base.)
2. **Single name.** `OpenResource("ciaa.resource")` always returns the
   same pointer to the same base for the lifetime of the system.
3. **Never expunged.** Libraries can be expunged to reclaim memory;
   resources cannot.
4. **State is mostly allocation tables.** A resource is much smaller than
   a library — it's a jump table plus whatever structures are needed to
   remember who owns what.

### When to use which

Rules of thumb for a driver author:

- If you need to **reach the hardware registers directly** — CIA timers,
  POTGO pins, the parallel port pins — go through the corresponding
  resource (`cia.resource`, `potgo.resource`, `misc.resource`).
- If you need **a useful service at a reasonable level** — read a track,
  send a byte, get the time of day — use a device or library
  (`trackdisk.device`, `serial.device`, `timer.device`,
  `battclock.resource` in the time-of-day case).
- If there is **already a device for what you want**, prefer the device.
  The device has already taken ownership of the resource on your behalf
  and will hand back control when you close it. Only bypass the device if
  you have a good reason — copy-protection loaders are the classic case
  for `disk.resource`; custom serial protocols for `misc.resource`.

---

## 2. The exec ResourceList API

Three calls in `exec.library` manage the resource list. They are small
and the autodocs are very terse, so they are reproduced in full.

### `AddResource` — add a resource to the system

```
    AddResource(resource)
                A1

    void AddResource(APTR);
```

Autodoc (`exec.doc V45`, `/AddResource`):

> **FUNCTION**
>     This function adds a new resource to the system and makes it
>     available to other users.  The resource must be ready to be called
>     at this time.
>
>     Resources currently have no system-imposed structure, however they
>     must start with a standard named node (LN_SIZE), and should with
>     a standard Library node (LIB_SIZE).
>
> **INPUTS**
>     resource - pointer an initialized resource node
>
> **SEE ALSO**
>     RemResource, OpenResource, MakeLibrary

Called by resource-init code, typically from a `Resident` struct during
Kickstart initialization. User code practically never calls `AddResource`.

### `OpenResource` — gain access to a resource

```
    resource = OpenResource(resName)
    D0                      A1

    APTR OpenResource(STRPTR);
```

Autodoc (`exec.doc V45`, `/OpenResource`):

> **FUNCTION**
>     This function returns a pointer to a resource that was previously
>     installed into the system.
>
>     There is no CloseResource() function.
>
> **INPUTS**
>     resName - the name of the resource requested.
>
> **RESULTS**
>     resource - if successful, a resource pointer, else NULL

Typical use:

```c
#include <proto/exec.h>
#include <resources/cia.h>

struct Library *CIAAResource;

CIAAResource = (struct Library *)OpenResource(CIAANAME);
if (!CIAAResource) { /* catastrophic — cia.resource should always exist */ }
```

### `RemResource` — remove a resource from the system

```
    RemResource(resource)
                A1

    void RemResource(APTR);
```

Autodoc (`exec.doc V45`, `/RemResource`):

> **FUNCTION**
>     This function removes an existing resource from the system resource
>     list.  There must be no outstanding users of the resource.
>
> **INPUTS**
>     resource - pointer to a resource node
>
> **SEE ALSO**
>     AddResource

There is no way for the Exec framework to check the "no outstanding
users" requirement — resources don't track opener counts. Calling
`RemResource()` is essentially a kill. It exists for completeness and
for diagnostic tools; normal code never calls it.

---

## 3. `cia.resource`

Name constants (`resources/cia.h`):

```c
#define CIAANAME "ciaa.resource"
#define CIABNAME "ciab.resource"
```

There are **two** CIA resources, one per chip. They share a jump table
and function set — the autodoc entries talk about "the CIA resource"
singular, but in practice you open whichever one you need:

```c
struct Library *CIABase = OpenResource(CIAANAME);  /* or CIABNAME */
```

The CIA resource's job is to arbitrate the five **ICR (Interrupt
Control Register) bits** and, by extension, the hardware sources
underneath them. Each 8520 CIA has two interval timers (Timer A,
Timer B), a time-of-day counter, and a serial-port shift register, all
of which can generate interrupts via the ICR. Ownership of an ICR bit
through `AddICRVector()` implicitly grants ownership of the hardware
that generates that interrupt — there is no second level of
arbitration for the actual timer registers.

### The ICR bit layout

Both CIAs have the same ICR layout (see `amiga-hardware-reference.md`
for the underlying hardware):

| Bit | Source                                   |
|-----|------------------------------------------|
| 0   | Timer A underflow                        |
| 1   | Timer B underflow                        |
| 2   | TOD counter alarm                        |
| 3   | Serial-port shift register full/empty    |
| 4   | /FLAG input pin (level-triggered)        |

On CIA-A, these interrupt sources generate a **level 2 (PORTS)**
interrupt; on CIA-B, they generate a **level 6 (EXTER)** interrupt.
Sharing the ICR hardware means that `cia.resource` is the single point
of coordination between every driver that uses CIA timers or the /FLAG
input: `timer.device`, `trackdisk.device`, `serial.device`,
`keyboard.device`, `audio.device`, and any direct user code.

### Typical ICR bit ownership

There are only four interval timers in the whole machine (A/B on two
CIAs), so they are a scarce resource. A rough idea of typical ownership
on a running V40 system:

| Bit             | Typical owner on V40                                              |
|-----------------|--------------------------------------------------------------------|
| CIA-A Timer A   | `keyboard.device` (bit-clock for the keyboard serial line)         |
| CIA-A Timer B   | Free, or used by `timer.device` as the VBlank-sync micro timer     |
| CIA-A TOD       | `graphics.library` (50/60 Hz vertical-blank alarm)                 |
| CIA-A Serial    | `keyboard.device` (keyboard SP line is the CIA-A serial port)      |
| CIA-A /FLAG     | `parallel.device` (ACK from parallel port)                         |
| CIA-B Timer A   | `serial.device` (UART baud-rate generator on the custom-chip port) |
| CIA-B Timer B   | Free, or `timer.device` MICROHZ unit                               |
| CIA-B TOD       | `trackdisk.device` (index pulses counted)                          |
| CIA-B Serial    | Unused in stock system                                             |
| CIA-B /FLAG     | `trackdisk.device` (disk-index rising edge)                        |

This table is not a spec — it's a description. The actual owners vary
across Kickstart versions, and the whole point of the resource is that
you don't assume. **Always** use `AddICRVector()` to claim what you
need.

### Struct definitions

`resources/cia.h` defines only the name constants; the CIA base is a
plain `struct Library`, and there is deliberately no public state in
`ciabase.h`:

```c
/* resources/ciabase.h */
/*
 *	There is no public information in CiaBase
 */
```

The `struct Interrupt` you install is the standard Exec one from
`<exec/interrupts.h>`:

```c
struct Interrupt {
    struct Node is_Node;
    APTR        is_Data;   /* passed in A1 to the handler */
    VOID      (*is_Code)();/* handler entry point */
};
```

Set `is_Node.ln_Type = NT_INTERRUPT`, fill in `ln_Name` (something
identifiable, for debugging tools), and `ln_Pri` to taste — though
`cia.resource` doesn't support chaining at the ICR-bit level, so
priority is advisory.

### `AddICRVector` — attach an interrupt handler to a CIA bit

```
    interrupt = AddICRVector( Resource, iCRBit, interrupt )
    D0                        A6        D0      A1

    struct Interrupt *AddICRVector( struct Library *, WORD,
                                    struct Interrupt * );
```

Autodoc (`cia.doc V45`, `/AddICRVector`):

> **FUNCTION**
>     Assign interrupt processing code to a particular interrupt bit
>     of the CIA ICR.  If the interrupt bit has already been
>     assigned, this function will fail, and return a pointer to the
>     owner interrupt.  If it succeeds, a null is returned.
>
>     This function will also enable the CIA interrupt for the given
>     ICR bit.
>
> **INPUTS**
>     iCRBit      Bit number to set (0..4).
>     interrupt   Pointer to interrupt structure.
>
> **RESULT**
>     interrupt   Zero if successful, otherwise returns a
>                 pointer to the current owner interrupt
>                 structure.
>
> **NOTE**
>     A processor interrupt may be generated immediately if this call
>     is successful.
>
>     In general, it is probably best to only call this function
>     while DISABLED so that the resource to which the interrupt
>     handler is being attached may be set to a known state before
>     the handler is called. You MUST NOT change the state of the
>     resource before attaching your handler to it.
>
>     ***WARNING***
>
>     Never assume that any of the CIA hardware is free for use.
>     Always use the AddICRVector() function to obtain ownership
>     of the CIA hardware registers your code will use.
>
>     Note that there are two (2) interval timers per CIA.  If
>     your application needs one of the interval timers, you
>     can try to obtain any one of the four (4) until AddICRVector()
>     succeeds.  If all four interval timers are in-use, your
>     application should exit cleanly.
>
>     If you just want ownership of a CIA hardware timer, or register,
>     but do not want interrupts generated, use the AddICRVector()
>     function to obtain ownership, and use the AbleICR() function
>     to turn off (or on) interrupts as needed.
>
>     Note that CIA-B generates level 6 interrupts (which can degrade
>     system performance by blocking lower priority interrupts).  As
>     usual, interrupt handling code should be optimized for speed.
>
>     Always call RemICRVector() when your code exits to release
>     ownership of any CIA hardware obtained with AddICRVector().
>
> **SEE ALSO**
>     cia.resource/RemICRVector(), cia.resource/AbleICR()

The C stubs in `amiga.lib` take the resource base as an extra first
argument:

```c
struct Interrupt *old = AddICRVector(CIAAResource, 0, &myIntA);
if (old != NULL) {
    /* A-timer is already owned; old->is_Node.ln_Name tells you who */
}
```

The fact that a non-NULL return is the existing owner's interrupt
struct — and that its `ln_Name` is human-readable — makes this the
canonical "who is using the timer" diagnostic.

### `RemICRVector` — detach an interrupt handler from a CIA bit

```
    RemICRVector( Resource, iCRBit, interrupt )
                  A6        D0      A1

    void RemICRVector( struct Library *, WORD, struct Interrupt *);
```

Autodoc (`cia.doc V45`, `/RemICRVector`):

> **FUNCTION**
>     Disconnect interrupt processing code for a particular
>     interrupt bit of the CIA ICR.
>
>     This function will also disable the CIA interrupt for the
>     given ICR bit.
>
> **INPUTS**
>     iCRBit      Bit number to set (0..4).
>     interrupt   Pointer to interrupt structure.

Symmetric with `AddICRVector`. Disables the ICR bit on the way out so
the hardware will not fire an interrupt into nothing.

### `AbleICR` — enable/disable ICR interrupts

```
    oldMask = AbleICR( Resource, mask )
    D0                 A6        D0

    WORD AbleICR( struct Library *, WORD );
```

Autodoc (`cia.doc V45`, `/AbleICR`):

> **FUNCTION**
>     This function provides a means of enabling and disabling 8520
>     CIA interrupt control registers. In addition it returns the
>     previous enable mask.
>
> **INPUTS**
>     mask    A bit mask indicating which interrupts to be
>             modified. If bit 7 is clear the mask
>             indicates interrupts to be disabled. If
>             bit 7 is set, the mask indicates
>             interrupts to be enabled. Bit positions
>             are identical to those in 8520 ICR.
>
> **RESULTS**
>     oldMask The previous enable mask before the requested
>             changes. To get the current mask without
>             making changes, call the function with a
>             null parameter.
>
> **EXAMPLES**
>     Get the current mask:
>         mask = AbleICR(0)
>     Enable both timer interrupts:
>         AbleICR(0x83)
>     Disable serial port interrupt:
>         AbleICR(0x08)
>
> **EXCEPTIONS**
>     Enabling the mask for a pending interrupt will cause an
>     immediate processor interrupt (that is if everything else is
>     enabled). You may want to clear the pending interrupts with
>     SetICR() prior to enabling them.

`AbleICR` is a software-arbitrated wrapper around writing the 8520 ICR
with bit 7 set (enable) or clear (disable). Writing the ICR directly
would be a disaster because multiple ICR bits are live at once: a
direct write would overwrite the enable state of someone else's ICR
bit. `AbleICR` reads the shadow, modifies only the bits you name, and
writes back.

### `SetICR` — cause, clear, and sample ICR interrupts

```
    oldMask = SetICR( Resource, mask )
    D0                A6        D0

    WORD SetICR( struct Library *, WORD );
```

Autodoc (`cia.doc V45`, `/SetICR`):

> **FUNCTION**
>     This function provides a means of reseting, causing, and
>     sampling 8520 CIA interrupt control registers.
>
> **INPUTS**
>     mask    A bit mask indicating which interrupts to be
>             effected. If bit 7 is clear the mask
>             indicates interrupts to be reset.  If bit
>             7 is set, the mask indicates interrupts to
>             be caused. Bit positions are identical to
>             those in 8520 ICR.
>
> **RESULTS**
>     oldMask The previous interrupt register status before
>             making the requested changes.  To sample
>             current status without making changes,
>             call the function with a null parameter.
>
> **EXAMPLES**
>     Get the interrupt mask:
>         mask = SetICR(0)
>     Clear serial port interrupt:
>         SetICR(0x08)
>
> **NOTE**
>     ***WARNING***
>
>     Never read the contents of the CIA interrupt control registers
>     directly.  Reading the contents of one of the CIA interrupt
>     control registers clears the register.  This can result in
>     interrupts being missed by critical operating system code, and
>     other applications.

The warning is the real reason `cia.resource` exists. Reading the ICR
is destructive: it latches and clears the pending bits in the same
cycle. If two pieces of code read the ICR, the second reader sees
zero for any bit that was cleared by the first, and the bit's owner
never sees the interrupt. `SetICR` is the only supported way to
sample the ICR — it reads it once, caches the value, and distributes
pending bits to each registered handler.

### Cross-reference

For the CIA hardware register layout (PRA/PRB, DDRA/DDRB, TA/TB, TOD,
SDR, CRA/CRB), see `amiga-hardware-reference.md`. For the higher-level
drivers that sit on top of `cia.resource` (`timer.device`,
`keyboard.device`, `serial.device`, `trackdisk.device`), see
`amiga-io-audio-expansion.md`.

---

## 4. `disk.resource`

Name constant (`resources/disk.h`):

```c
#define DISKNAME "disk.resource"
```

`disk.resource` arbitrates the floppy-disk hardware: the Paula DMA
channel for DSKDAT/DSKLEN/DSKSYNC/DSKBYTR/DSKPT/DSKPTL, the CIA-B
pins that select drives and generate the /INDEX interrupt via the
/FLAG line, and the drive-state machines themselves. **Four units
(0..3)** exist whether or not drives are actually attached. The normal
client is `trackdisk.device`; direct use of `disk.resource` is the
territory of custom disk-access code such as copy-protection loaders,
raw-track rippers, and nonstandard formats like 150 RPM disks.

### Struct definitions

From `resources/disk.h`:

```c
struct DiscResourceUnit {
    struct Message   dru_Message;
    struct Interrupt dru_DiscBlock;
    struct Interrupt dru_DiscSync;
    struct Interrupt dru_Index;
};

struct DiscResource {
    struct Library           dr_Library;
    struct DiscResourceUnit *dr_Current;
    UBYTE                    dr_Flags;
    UBYTE                    dr_pad;
    struct Library          *dr_SysLib;
    struct Library          *dr_CiaResource;
    ULONG                    dr_UnitID[4];
    struct List              dr_Waiting;
    struct Interrupt         dr_DiscBlock;
    struct Interrupt         dr_DiscSync;
    struct Interrupt         dr_Index;
    struct Task             *dr_CurrTask;
};

/* dr_Flags entries */
#define DRB_ALLOC0   0    /* unit zero is allocated */
#define DRB_ALLOC1   1
#define DRB_ALLOC2   2
#define DRB_ALLOC3   3
#define DRB_ACTIVE   7    /* is the disc currently busy? */

#define DRF_ALLOC0   (1<<0)
#define DRF_ALLOC1   (1<<1)
#define DRF_ALLOC2   (1<<2)
#define DRF_ALLOC3   (1<<3)
#define DRF_ACTIVE   (1<<7)

/* Hardware magic */
#define DSKDMAOFF   0x4000  /* idle command for dsklen register */

/* Drive types */
#define DRT_AMIGA      (0x00000000)
#define DRT_37422D2S   (0x55555555)
#define DRT_EMPTY      (0xFFFFFFFF)
#define DRT_150RPM     (0xAAAAAAAA)
```

`dr_UnitID[4]` caches the drive-id shift-register value read for each
unit; this is what distinguishes Amiga 880K drives from IBM-compatible
5.25" drives from the extinct high-density 150 RPM prototype. The
`dr_Waiting` list is used by the blocking flavour of acquisition
(`GetUnit`), which leaves a message on the list until the disk is
handed to you.

`DiscResourceUnit` is your per-caller record. You fill in its message
port (via `dru_Message.mn_ReplyPort`) and the three interrupt handlers
you want called for DSKBLK (block transfer complete), DSKSYN (sync
word matched), and the CIA-B index pulse. You pass the
`DiscResourceUnit` pointer to `GetUnit()` and you get it back via
`ReplyMsg` once the disk is yours.

### Two-step allocation: `AllocUnit` + `GetUnit`

A disk session is in fact *two* allocations. The first one,
`AllocUnit()`, says "I intend to use unit N, don't let anybody else
touch it at all" — it's a long-lived reservation across many disk
operations. The second, `GetUnit()`, says "lock the disk hardware to
me right now so I can issue DMA" — it's a short critical section for
a single I/O. A `trackdisk.device` unit does `AllocUnit` when it opens
and `FreeUnit` when it closes; it does `GetUnit`/`GiveUnit` around
every single track read.

### `AllocUnit` — reserve a unit

```
    Success = AllocUnit( unitNum )
    D0                   D0

    BOOL AllocUnit(LONG);
```

Autodoc (`disk.doc V45`, `/AllocUnit`):

> **FUNCTION**
>     This routine allocates one of the units of the disk.  It should
>     be called before trying to use the disk (via GetUnit).
>
>     In reality, it is perfectly fine to use GetUnit/GiveUnit if AllocUnit
>     fails.  Do NOT call FreeUnit if AllocUnit did not succeed.  This
>     has been the case for all revisions of disk.resource.
>
> **INPUTS**
>     unitNum -- a legal unit number (zero through three)
>
> **RESULTS**
>     Success -- nonzero if successful.  zero on failure.

The second paragraph is unusual: it tells you that a caller that *fails*
`AllocUnit` can still do a `GetUnit`/`GiveUnit` cycle — the operation
is still safe because `GetUnit` arbitrates regardless. `AllocUnit`
really just prevents *other* drivers from successfully calling
`AllocUnit` for the same unit; it doesn't stop them from using it. The
warning is simply "don't double-free."

### `FreeUnit` — release a unit reservation

```
    FreeUnit( unitNum )
              D0

    void FreeUnit(LONG);
```

Autodoc (`disk.doc V45`, `/FreeUnit`):

> **FUNCTION**
>     This routine deallocates one of the units of the disk.  It should
>     be called when done with the disk.  Do not call it if you did
>     no successfully allocate the disk (there is no protection -- you
>     will probably crash the disk system).
>
> **BUGS**
>     Doesn't check if you own the unit, or even if anyone owns it.

The "bugs" note is important: `disk.resource` trusts callers to be
honest, so matching `AllocUnit`/`FreeUnit` is entirely on you.

### `GetUnit` — lock the disk hardware

```
    lastDriver = GetUnit( unitPointer )
    D0                    A1

    struct DiscResourceUnit *GetUnit(struct DiscResourceUnit *);
```

Autodoc (`disk.doc V45`, `/GetUnit`):

> **FUNCTION**
>     This routine allocates the disk to a driver.  It is either
>     immediately available, or the request is saved until the disk
>     is available.  When it is available, your unitPointer is
>     sent back to you (via ReplyMsg).  You may then reattempt the
>     GetUnit.
>
>     Allocating the disk allows you to use the disk's resources.
>     Remember however that there are four units to the disk; you are
>     only one of them.  Please be polite to the other units (by never
>     selecting them, and by not leaving interrupts enabled, etc.).
>
>     When you are done, please leave the disk in the following state:
>         dmacon dma bit ON
>         dsklen dma bit OFF (write a #DSKDMAOFF to dsklen)
>         adkcon disk bits -- any way you want
>         intena:disk sync and disk block interrupts -- Both DISABLED
>         CIA resource index interrupt -- DISABLED
>         8520 outputs -- doesn't matter, because all bits will be
>             set to inactive by the resource.
>         8520 data direction regs -- restore to original state.
>
>     NOTE: GetUnit() does NOT turn on the interrupts for you.
>           You must use AbleICR (for the index interrupt) or intena
>           (for the diskbyte and diskblock interrupts) to turn them
>           on.  You should turn them off before calling GiveUnit,
>           as stated above.
>
> **INPUTS**
>     unitPtr - a pointer you your disk resource unit structure.
>         Note that the message filed of the structure MUST
>         be a valid message, ready to be replied to.  Make sure
>         ln_Name points to a null-terminated string, preferably
>         one that identifies your program.
>
>         You need to set up the three interrupt structures,
>         in particular the IS_DATA and IS_CODE fields.  Set them
>         to NULL if you don't need that interrupt.  Also, set
>         the ln_Type of the interrupt structure to NT_INTERRUPT.
>         WARNING: don't turn on a disk resource interrupt unless
>         the IS_CODE for that interrupt points to executable code!
>
>         IS_CODE will be called with IS_DATA in A1 when the
>         interrupt occurs.  Preserve all regs but D0/D1/A0/A1.
>         Do not make assumptions about A0.
>
> **RESULTS**
>     lastDriver - if the disk is not busy, then the last unit
>         to use the disk is returned.  This may be used to
>         see if a driver needs to reset device registers.
>         (If you were the last user, then no one has changed
>         any of the registers.  If someone else has used it,
>         then any allowable changes may have been made).  If the
>         disk is busy, then a null is returned.

The `lastDriver` return is a clever optimisation: if you were the last
user you can skip re-programming the disk registers, because nobody has
touched them since you let go.

The "blocking" semantics are done via `ReplyMsg` — `GetUnit` returns
immediately whether or not you got the disk. If you got it (non-NULL
return), great. If you didn't (NULL return), your message has been
queued and you will receive a reply on your message port when the
disk becomes available. That's the point at which you call `GetUnit`
again and should succeed.

### `GiveUnit` — release the disk hardware

```
    GiveUnit()

    void GiveUnit();
```

Autodoc (`disk.doc V45`, `/GiveUnit`):

> **FUNCTION**
>     This routine frees the disk after a driver is done with it.
>     If others are waiting, it will notify them.
>
> **BUGS**
>     In pre-V36, GiveUnit didn't check if you owned the unit.  A patch
>     for this was part of 1.3.1 SetPatch.  Fixed in V36.

Note the historical bug: early `disk.resource` let anybody call
`GiveUnit`, which was how some copy-protection schemes used to wedge
the disk. SetPatch shipped a fix; V36+ enforces it.

### `GetUnitID` — query cached drive type

```
    idtype = GetUnitID( unitNum )
    D0                  D0

    LONG GetUnitID(LONG);
```

Autodoc (`disk.doc V45`, `/GetUnitID`):

> **FUNCTION**
>     Gets the drive ID for a given unit.  Note that this value may
>     change if someone calls ReadUnitID, and the drive id changes.
>
> **RESULTS**
>     idtype -- the type of the disk drive.  Standard types are
>         defined in the resource include file.

Returns one of the `DRT_*` constants from `disk.h`:

- `DRT_AMIGA` (0) — standard Amiga 880K double-density drive
- `DRT_37422D2S` — IBM-compatible 5.25" double-sided
- `DRT_150RPM` — high-density prototype
- `DRT_EMPTY` — nothing attached

### `ReadUnitID` — re-sample drive type

```
    idtype = ReadUnitID( unitNum )
    D0                   D0

    ULONG ReadUnitID(LONG);    /* V37+ */
```

Autodoc (`disk.doc V45`, `/ReadUnitID`):

> **FUNCTION**
>     Rereads the drive id for a specific unit (for handling drives
>     that change ID according to what sort of disk is in them.  You
>     MUST have done a GetUnit before calling this function!

Some drives (notably the HD drives in A4000/A3000) report a different
id depending on whether a DD or HD disk is in the slot, so
`trackdisk.device` calls `ReadUnitID` at disk-change time to see which
rate it should drive the floppy at. The function is V37+; earlier
Kickstarts only have `GetUnitID`.

---

## 5. `battclock.resource`

Name constant (`resources/battclock.h`):

```c
#define BATTCLOCKNAME "battclock.resource"
```

`battclock.resource` is the **battery-backed real-time clock**
abstraction. The underlying hardware is one of two chips, depending
on the machine:

- **MSM6242** (OKI) — A1000 expansion clock, A2000, A500 plus (with
  ECS) and most third-party trapdoor clocks. Memory-mapped
  nibble-wide BCD registers.
- **Ricoh RP5C01** — A3000, A4000, and some later A2000 boards.
  Very similar programming model, but different register layout and
  a 4-bit command register.

`battclock.resource` hides both behind the same three-function
interface so that DOS and everything else see a simple "seconds since
1978-01-01 00:00:00 UTC" counter. It became a standard resource in
V36; earlier systems used private clock code in the boot process.

The DOS `DateStamp` representation (days/minutes/ticks since
1978-01-01) is a trivial reinterpretation of the `battclock.resource`
time-in-seconds value and is converted by `dos.library`/`utility.library`.

### `ReadBattClock` — read time from the clock chip

```
    AmigaTime = ReadBattClock( )

    ULONG ReadBattClock( void );
    D0
```

Autodoc (`battclock.doc V45`, `/ReadBattClock`):

> **FUNCTION**
>     This routine reads the time from the clock chip and returns it
>     as the number of seconds from 01-jan-1978.
>
> **RESULTS**
>     AmigaTime   The number of seconds from 01-Jan-1978 that
>                 the clock chip thinks it is.
>
> **NOTES**
>     If the clock chip returns an invalid date, the clock chip is
>     reset and 0 is returned.

The autodoc flags the function as `(V36)`.

### `WriteBattClock` — set the time on the clock chip

```
    WriteBattClock( AmigaTime )
                    D0

    void WriteBattClock( ULONG );
```

Autodoc (`battclock.doc V45`, `/WriteBattClock`):

> **FUNCTION**
>     This routine writes the time given in AmigaTime to the clock
>     chip.
>
> **INPUTS**
>     AmigaTime   The number of seconds from 01-Jan-1978 to the
>                 time that should be written to the clock chip.

Also `(V36)`.

### `ResetBattClock` — put the clock chip into a known state

```
    ResetBattClock( )

    void ResetBattClock( void );
```

Autodoc (`battclock.doc V45`, `/ResetBattClock`):

> **FUNCTION**
>     This routine does whatever is neeeded to put the clock chip
>     into a working and usable state and also sets the date on the
>     clock chip to 01-Jan-1978.

Used by preferences tools and by `ReadBattClock` itself when it
detects garbage in the clock registers.

### Practical notes

- Locking of the clock chip is implicit; all three calls use short
  critical sections internally.
- On A500 without a clock module (the base machine has no battery),
  `battclock.resource` simply isn't present and `OpenResource` returns
  NULL. Test for that.
- Conversion to and from `struct DateStamp` or Unix epoch seconds is
  not done by the resource — use `utility.library/Amiga2Date` and
  friends, or do the math yourself (subtract 252460800 to reach Unix
  epoch, and remember `ULONG` wraparound in 2114).

---

## 6. `battmem.resource`

Name constant (`resources/battmem.h`):

```c
#define BATTMEMNAME "battmem.resource"
```

`battmem.resource` exposes the **battery-backed SRAM** inside the
real-time-clock chip — a tiny scratchpad, typically **50 bytes** total,
that survives power-off. It is used by the ROM to store a handful of
system-wide preferences: boot-mode choices, monitor defaults, keymap
hints, the early-startup menu state. Access is **bit-addressed** and
arbitrated by a semaphore.

This is **not** the CDTV/CD32 NVRAM: that's `nonvolatile.library`
(see §11), which uses a different chip and has a much larger and more
structured storage model. `battmem.resource` is tiny and dumb by
comparison.

The bit layouts for the fields inside battmem are in
`resources/battmembitsamiga.h`, `battmembitsamix.h`, and
`battmembitsshared.h` — they define offset/length pairs for each known
field. Since the storage is bit-addressed and checksummed, modifying
an unknown field is genuinely dangerous; stick to the documented bits.

### `ReadBattMem` — read a bitstring from nonvolatile RAM

```
    Error = ReadBattMem( Buffer, Offset, Len )
    D0                   A0      D0      D1

    ULONG ReadBattMem( APTR, ULONG, ULONG );
```

Autodoc (`battmem.doc V45`, `/ReadBattMem`):

> **FUNCTION**
>     Read a bitstring from nonvolatile ram.
>
> **INPUTS**
>     Buffer  Where to put the bitstring.
>     Offset  Bit offset of first bit to read.
>     Len     Length of bitstring to read.
>
> **RESULTS**
>     Error   Zero if no error.
>
> **NOTES**
>     The battery-backed memory is checksummed. If a checksum error
>     is detected, all bits in the battery-backed memory are
>     silently set to zero.
>
>     Bits in the battery-backed memory that do not exist are read
>     as zero.
>
>     Partial byte reads (less than 8 bits) result in the bits read
>     being put in the low-order bits of the destination byte.

All calls are `(V36)`.

### `WriteBattMem` — write a bitstring to nonvolatile RAM

```
    Error = WriteBattMem( Buffer, Offset, Len )
    D0                    A0      D0      D1

    ULONG WriteBattMem( APTR, ULONG, ULONG );
```

Autodoc (`battmem.doc V45`, `/WriteBattMem`):

> **FUNCTION**
>     Write a bitstring to the nonvolatile ram.
>
> **INPUTS**
>     Buffer  Where to get the bitstring.
>     Offset  Bit offset of first bit to write.
>     Len     Length of bitstring to write.
>
> **NOTES**
>     The battery-backed memory is checksummed. If a checksum error
>     is detected, all bits in the battery-backed memory are
>     silently set to zero.
>
>     Partial byte writes (less than 8 bits) result in the bits
>     written being read from the low-order bits of the source byte.

The checksum is maintained by `battmem.resource` itself — you don't
see it as a caller, but every write rewrites the checksum at the end
of the store. A corrupt battery returns zero on first read and then
the resource is effectively blank until the next write.

### `ObtainBattSemaphore` — exclusive access

```
    ObtainBattSemaphore( )

    void ObtainBattSemaphore( void );
```

Autodoc (`battmem.doc V45`, `/ObtainBattSemaphore`):

> **FUNCTION**
>     Aquires exclusive access to the system nonvolatile ram.

### `ReleaseBattSemaphore` — release exclusive access

```
    ReleaseBattSemaphore( )

    void ReleaseBattSemaphore( void );
```

Autodoc (`battmem.doc V45`, `/ReleaseBattSemaphore`):

> **FUNCTION**
>     Relinquish exclusive access to the system nonvolatile ram.

You should wrap any multi-step read-modify-write on battmem inside an
obtain/release pair. Single `ReadBattMem`/`WriteBattMem` calls are
self-synchronising; the semaphore is for protecting your own logical
transactions.

---

## 7. `potgo.resource`

Name constant (`resources/potgo.h`):

```c
#define POTGONAME "potgo.resource"
```

`potgo.resource` arbitrates the `POTGO` / `POTINP` / `POT0DAT` /
`POT1DAT` custom registers that service the **proportional / paddle
inputs** on the two game ports. The Y/X pins of each port (gameport
pins 5 and 9) can be configured as:

- Outputs, programmable to any level — used for lightpen stylus
  buttons, Commodore paddle inputs, and middle/right mouse buttons
  on 3-button mice
- Start-a-count lines driving the hardware capacitor-charge counters
  in `POT0DAT`/`POT1DAT` — actual proportional input

The potgo hardware is peculiar because the START bit (bit 0 of POTGO)
is a **global trigger** for all four potentiometer counters at once,
but different pins may need to be in different modes. If one driver
wants to measure the left-port paddle while another wants the right-port
Y pin to be a fixed output, they have to cooperate — that's what
`potgo.resource` exists for.

### Bit layout of the potgo mask (paraphrased from the autodoc)

| Bit | Meaning                                                          |
|-----|------------------------------------------------------------------|
| 0   | START — set to restart the pot counters                          |
| 8   | DATLX — left port, pin 5 (paddle X, or mouse/joy button 2/3)     |
| 9   | OUTLX — promise to use DATLX in output mode only                 |
| 10  | DATLY — left port, pin 9                                         |
| 11  | OUTLY — output-only for DATLY                                    |
| 12  | DATRX — right port, pin 5                                        |
| 13  | OUTRX                                                            |
| 14  | DATRY — right port, pin 9                                        |
| 15  | OUTRY                                                            |

If `OUTxx` is set, you're telling the resource that you don't care if
other users trigger START, because the pin is always driven and ignores
the START signal.

### `AllocPotBits` — allocate bits in the potgo register

```
    allocated = AllocPotBits(bits)
    D0                       D0

    UWORD AllocPotBits( UWORD );
```

Autodoc (`potgo.doc V45`, `/AllocPotBits`):

> **FUNCTION**
>     The AllocPotBits routine allocates bits in the hardware potgo
>     register that the application wishes to manipulate via
>     WritePotgo.  The request may be for more than one bit.  A
>     user trying to allocate bits may find that they are
>     unavailable because they are already allocated, or because
>     the start bit itself (bit 0) has been allocated, or if
>     requesting the start bit, because input bits have been
>     allocated.  A user can block itself from allocation: i.e.
>     it should FreePotgoBits the bits it has and re-AllocPotBits if
>     it is trying to change an allocation involving the start bit.
>
> **INPUTS**
>     bits - a description of the hardware bits that the application
>         wishes to manipulate, loosely based on the register
>         description itself:
>       START (bit 0) - set if you wish to use start (i.e. start
>             the proportional controller counters) with the
>             input ports you allocate (below).  You must
>             allocate all the DATxx ports you want to apply
>             START to in this same call, with the OUTxx bit
>             clear.
>       DATLX (bit 8) - set if you wish to use the port associated
>             with the left (0) controller, pin 5.
>       OUTLX (bit 9) - set if you promise to use the LX port in
>             output mode only.  The port is not set to output
>             for you at this time -- this bit set indicates
>             that you don't mind if STARTs are initiated at any
>             time by others, since ports that are enabled for
>             output are unaffected by START.
>       DATLY (bit 10) - as DATLX but for the left (0) controller, pin 9.
>       OUTLY (bit 11) - as OUTLX but for LY.
>       DATRX (bit 12) - the right (1) controller, pin 5.
>       OUTRX (bit 13) - OUT for RX.
>       DATRY (bit 14) - the right (1) controller, pin 9.
>       OUTRY (bit 15) - OUT for RY.
>
> **RESULTS**
>     allocated - the START and DATxx bits of those requested that
>         were granted.  The OUTxx bits are don't cares.

### `FreePotBits` — free allocated bits

```
    FreePotBits(allocated)
                D0

    void FreePotBits( UWORD );
```

Autodoc (`potgo.doc V45`, `/FreePotBits`):

> **FUNCTION**
>     The FreePotBits routine frees previously allocated bits in the
>     hardware potgo register that the application had allocated via
>     AllocPotBits and no longer wishes to use.  It accepts the
>     return value from AllocPotBits as its argument.

### `WritePotgo` — write to the hardware POTGO register

```
    WritePotgo(word, mask)
               D0    D1

    void WritePotgo( UWORD, UWORD );
```

Autodoc (`potgo.doc V45`, `/WritePotgo`):

> **FUNCTION**
>     The WritePotgo routine sets and clears bits in the hardware
>     potgo register.  Only those bits specified by the mask are
>     affected -- it is improper to set bits in the mask that you
>     have not successfully allocated.  The bits in the high byte
>     are saved to be maintained when other users write to the
>     potgo register.  The START bit is not saved, it is written
>     only explicitly as the result of a call to this routine with
>     the START bit set: other users will not restart it.
>
> **INPUTS**
>     word - the data to write to the hardware potgo register and
>         save for further use, except the START bit, which is
>         not saved.
>     mask - those bits in word that are to be written.  Other
>         bits may have been provided by previous calls to
>         this routine, and default to zero.

So `WritePotgo` is a shadow-register protocol: the resource
remembers the last value anyone wrote through it, and every writer
affects only the bits they own. The START bit is special — it's
edge-sensitive, so it's never latched into the shadow.

### Typical ownership

- **Pot proportional input (paddles, tablets)** — claim the `DATxx`
  bits for the ports you want to measure and set START before
  reading `POT0DAT`/`POT1DAT`.
- **Three-button mouse** — middle button appears on the `DATLY` /
  `DATRY` pin of the appropriate port. `input.device` claims the
  relevant `OUTxx` bits to keep them as outputs.
- **Lightpen** — `graphics.library` claims DATLY (or RY depending
  on port) and uses it as input.

The gameport hardware is documented in `amiga-hardware-reference.md`
§POTGO; this resource is the only correct way to reach those bits.

---

## 8. `misc.resource`

Name constant and unit numbers (`resources/misc.h`):

```c
#define MISCNAME "misc.resource"

/* Unit number definitions. */
#define MR_SERIALPORT    0  /* SERDAT/SERDATR/SERPER/ADKCON + interrupts */
#define MR_SERIALBITS    1  /* Serial control bits (DTR, CTS, etc.) */
#define MR_PARALLELPORT  2  /* 8-bit parallel data port
                               (CIAAPRA & CIAADDRA only!) */
#define MR_PARALLELBITS  3  /* All other parallel bits & interrupts
                               (BUSY, ACK, etc.) */

/* Library vector offsets */
#define MR_ALLOCMISCRESOURCE  (LIB_BASE)                 /* -6  */
#define MR_FREEMISCRESOURCE   (LIB_BASE - LIB_VECTSIZE)  /* -12 */
```

`misc.resource` is a four-slot arbiter for chunks of hardware that
are not covered by another resource: the **Paula-side serial registers**,
the **CIA-A parallel data port**, and the handshake/control lines
around each. The split between "serial port" and "serial bits" is
deliberate — in principle one driver can run the Paula UART while a
different driver manages DTR/CTS via `misc.resource/MR_SERIALBITS`,
although in practice `serial.device` grabs both. Likewise for
parallel.

As the autodoc puts it: ownership of a misc.resource slot grants
**low-level bit access to the hardware registers**. You are still
responsible for following the rules of the interrupt system — see
`exec.library/SetIntVector` or `cia.resource` as appropriate — so a
misc.resource slot does not cover the CIA-B ICR bits that might drive
the parallel port's ACK line; you still need `cia.resource` for those.

### `AllocMiscResource` — allocate a miscellaneous resource

```
    CurrentUser = AllocMiscResource( unitNum, name )
    D0                               D0       A1

    char *AllocMiscResource(ULONG, char *);
```

Autodoc (`misc.doc V45`, `/AllocMiscResource`):

> **FUNCTION**
>     This routine attempts to allocate one of the miscellaneous resources
>     If the resource had already been allocated, an error is returned. If
>     you do get it, your name is associated with the resource (so a user
>     can see who has it allocated).
>
>     This function may not be called from interrupt code
>
> **DESCRIPTION**
>     There are certain parts of the hardware that a multitasking- friendly
>     program may need to take over.  The serial port is a good example. By
>     grabbing the misc.resource for the serial port, the caller would
>     "own" the hardware registers associated with that function.  Nobody
>     else, including the system serial driver, is allowed to interfere.
>
>     Resources are called in exactly the same manner as libraries.
>     From assembly language, A6 must equal the resource base.  The
>     offsets for the function are listed in the resources/misc.i
>     include file (MR_ALLOCMISCRESOURCE for this function).
>
> **INPUTS**
>     unitNum - the number of the resource you want to allocate
>               (eg.  MR_SERIALBITS).
>     name - a mnenonic name that will help the user figure out
>         what piece of software is hogging a resource.
>         (havoc breaks out if a name of null is passed in...)
>
> **RESULTS**
>     CurrentUser - if the resource is busy, then the name of
>         the current user is returned.  If the resource is
>         free, then null is returned.

**The return-value convention is the protocol here.** If you get NULL,
the allocation succeeded and nobody else has the slot. If you get a
`char *`, the slot is taken and the returned string is the human-readable
name of the current owner — exactly the thing you'd show in an error
dialog ("Cannot open serial port: in use by UberTerm"). Every driver
that calls `AllocMiscResource` is obliged to pass a meaningful name.

### `FreeMiscResource` — release a miscellaneous resource

```
    FreeMiscResource( unitNum )
                      D0

    void FreeMiscResource(ULONG);
```

Autodoc (`misc.doc V45`, `/FreeMiscResource`):

> **FUNCTION**
>     This routine frees one of the resources allocated
>     by AllocMiscResource.  The resource is made available
>     for reuse.
>
>     FreeMiscResource must be called from the same task that
>     called AllocMiscResource.  This function may not be called from
>     interrupt code.

The "same task" requirement is hard — you cannot, for example, have
one task allocate and another free. Drivers that want this behaviour
must delegate the ownership through a message to the allocating task.

---

## 9. `card.resource` (PCMCIA)

Name constant (`resources/card.h`):

```c
#define CARDRESNAME "card.resource"
```

`card.resource` is the **PCMCIA credit-card interface** arbiter
used on **A600** and **A1200** machines (and the CD32 via its
internal expansion bus). It sits on top of the Gayle chip, which
provides the slot hardware — card detect, programming voltage,
status change interrupts, memory-mapped attribute/common/IO spaces.
`card.resource` was first shipped with V37 on the A600 and extended
in V39 on the A1200.

The slot holds at most one card at a time, but many devices want to
*rendezvous* with the card when the right kind appears — the memory
card driver, the modem driver, the network card driver, and so on.
So `card.resource` maintains a **priority-ordered notification list**:
you register a `CardHandle` struct, and when the user inserts a card
you get called in priority order until one of you says "yes, I know
this card" by retaining ownership.

### Struct definitions

From `resources/card.h`:

```c
struct CardHandle {
    struct Node       cah_CardNode;
    struct Interrupt *cah_CardRemoved;
    struct Interrupt *cah_CardInserted;
    struct Interrupt *cah_CardStatus;
    UBYTE             cah_CardFlags;
};

struct DeviceTData {
    ULONG dtd_DTsize;   /* Size in bytes      */
    ULONG dtd_DTspeed;  /* Speed in ns        */
    UBYTE dtd_DTtype;   /* Type of card       */
    UBYTE dtd_DTflags;  /* Other flags        */
};

struct CardMemoryMap {
    UBYTE *cmm_CommonMemory;
    UBYTE *cmm_AttributeMemory;
    UBYTE *cmm_IOMemory;
    /* V39+ */
    ULONG  cmm_CommonMemSize;
    ULONG  cmm_AttributeMemSize;
    ULONG  cmm_IOMemSize;
};

/* cah_CardFlags */
#define CARDF_RESETREMOVE    (1<<0)
#define CARDF_IFAVAILABLE    (1<<1)
#define CARDF_DELAYOWNERSHIP (1<<2)
#define CARDF_POSTSTATUS     (1<<3)  /* V39+ */

/* CardProgramVoltage */
#define CARD_VOLTAGE_0V   0
#define CARD_VOLTAGE_5V   1
#define CARD_VOLTAGE_12V  2

/* CardInterface */
#define CARD_INTERFACE_AMIGA_0  0

/* XIP tuple */
#define CISTPL_AMIGAXIP 0x91

struct TP_AmigaXIP {
    UBYTE TPL_CODE;
    UBYTE TPL_LINK;
    UBYTE TP_XIPLOC[4];
    UBYTE TP_XIPFLAGS;
    UBYTE TP_XIPRESRV;
};

#define XIPFLAGSF_AUTORUN (1<<0)
```

The `CardHandle` carries three interrupt pointers:
`cah_CardInserted` fires when ownership of a new card is handed to
you, `cah_CardRemoved` fires when your card is unplugged, and
`cah_CardStatus` fires on state changes (BVD1/2 battery, WP
write-protect, BSY/IRQ) via the gate array.

### `OwnCard` — own credit card registers and memory

```
    return = OwnCard( handle )
    D0                A1

    struct CardHandle *OwnCard( struct CardHandle * );
```

Autodoc (`cardres.doc V45`, `/OwnCard`):

> **FUNCTION**
>     This function is used to obtain immediate, or deferred
>     ownership of a credit-card in the credit-card slot.
>
>     Typically an EXEC STYLE DEVICE will be written to interface
>     between an application, and a credit card in the slot.  While
>     applications, and libraries can attempt to own a credit-card
>     in the card slot, the rest of this documentation assumes a
>     device interface will be used.
>
>     Because credit-cards can be inserted, or removed by the user at
>     any time (otherwise known as HOT-INSERTION, and HOT-REMOVAL),
>     the card.resource provides devices with a protocol which
>     lets many devices bid for ownership of a newly inserted card.
>
>     In general, devices should support HOT-REMOVAL, however there
>     are legitimate cases where HOT-REMOVAL is not practical.  For
>     these cases this function allows you to own the resource using
>     the CARDB_RESETREMOVE flag.  If the card is removed before your
>     device calls ReleaseCard(), the machine will RESET.
>
> **RESULTS**
>      0  - indicates success, your device owns the credit card.
>     -1  - indicates that the card cannot be owned (most likely
>           because there is no card in the credit card slot).
>     ptr - indicates failure.  Returns pointer to the CardHandle
>           structure which owns the credit card.
>
> **NOTES**
>     This function should only be called from a task.
>
>     CardHandle interrupts are called with a pointer to your data
>     in A1, and a pointer to your code in A5.

The `cah_CardNode.ln_Pri` priority table (from the same autodoc
entry):

| Priority | Comments                                                            |
|----------|---------------------------------------------------------------------|
| >= 21    | Reserved for future use                                             |
| 10–20    | Third-party devices identifying cards by specific tuples            |
| 01–19    | Reserved for future use (sic; note overlap in the Commodore docs)  |
| 00       | General-purpose devices with loose specification requirements      |
| <= -1    | Reserved for future use                                             |

`cah_CardFlags` options:

- `CARDF_RESETREMOVE` — hard-reset the Amiga if the card is pulled
  while you own it. For execute-in-place cards and RAM cards that
  have been added to system memory.
- `CARDF_IFAVAILABLE` — only succeed if the card is available right
  now; don't leave the handle on the notification list if not.
- `CARDF_DELAYOWNERSHIP` — never return success from `OwnCard()`
  directly; always come in via the `cah_CardInserted` interrupt.
- `CARDF_POSTSTATUS` (V39+) — get called back a second time after
  the status-change interrupt is cleared, for drivers that want to
  service the card hardware *after* the gate-array has cleared its
  latch.

### `ReleaseCard` — release ownership of a card

```
    ReleaseCard( handle, flags )
                 A1      D0

    void ReleaseCard( struct CardHandle *, ULONG );
```

Autodoc (`cardres.doc V45`, `/ReleaseCard`):

> **FUNCTION**
>     This function releases ownership of the credit card in the
>     slot.
>
>     The access light (if any) is automatically turned off
>     (if it was turned on) when you release ownership of
>     a card you owned, and all credit-card control registers
>     are reset to their default state.
>
>     You must call this function if -
>
>     You own the credit-card, and want to release it so that
>     other devices on the notification list will have a chance
>     to examine the credit-card in the card slot.
>
>     You took a Card Removed interrupt while you owned the
>     credit-card.  If so, you MUST call this function, else
>     no other task will be notified of newly inserted cards.

Flag: `CARDF_REMOVEHANDLE` means "also take my handle off the
notification list so I stop being called on future inserts."

### `BeginCardAccess` / `EndCardAccess` — bracket card memory access

```
    result = BeginCardAccess( handle )
    result = EndCardAccess( handle )
    d0                        a1

    BOOL  BeginCardAccess( struct CardHandle * );
    ULONG EndCardAccess( struct CardHandle * );
```

Autodoc (`cardres.doc V45`, `/BeginCardAccess`, `/EndCardAccess`):

> **FUNCTION (BeginCardAccess)**
>     This function should be called before you begin access
>     to credit-card memory.
>
>     Its effect will depend on the type of Amiga machine your
>     code happens to be running on.  On some machines it
>     will cause an access light to be turned ON.
>
> **FUNCTION (EndCardAccess)**
>     This function should be called when you are done accessing
>     credit-card memory.
>
>     On machines which support an access light, the light will
>     automatically be turned off when you call ReleaseCard().
>
> **RETURNS**
>     TRUE if you are still the owner of the credit-card, and
>     memory access is permitted.  FALSE if you are no longer
>     the owner of the credit-card (usually indicating that
>     the card was removed).

Both calls are safe from task or level-1/2 interrupt.

### `GetCardMap` — obtain pointer to the CardMemoryMap

```
    pointer = GetCardMap()
    d0

    struct CardMemoryMap *GetCardMap( void );
```

Autodoc (`cardres.doc V45`, `/GetCardMap`):

> **FUNCTION**
>     Obtain pointer to a CardMemoryMap structure.  The structure
>     is READ only.
>
>     Devices should never assume credit-card memory appears
>     at any particular place in memory.  By using this function
>     to obtain pointers to the base memory locations of the various
>     credit-card memory types, your device will continue to work
>     properly should credit cards appear in different memory
>     locations in future hardware.
>
> **NOTES**
>     If any pointer in the structure is NULL, it means this type
>     of credit-card memory is not being made available.

On V39+ the structure is extended with the size fields shown in the
struct definition above. Use the struct-embedded constants and not
hard-coded region sizes.

### `ReadCardStatus` — read the credit card status register

```
    status = ReadCardStatus()
    d0

    UBYTE ReadCardStatus( void );
```

Autodoc (`cardres.doc V45`, `/ReadCardStatus`):

> **FUNCTION**
>     Returns current state of the credit card status register.
>     See card.h/i for bit definitions.
>
>     Note that the meaning of the returned status bits may vary
>     depending on the type of card inserted in the slot, and
>     mode of operation.  Interpretation of the bits is left
>     up to the application.

Safe from any level of interrupt.

### `CardMiscControl` — set/clear miscellaneous control bits

```
    control_bits = CardMiscControl( handle, control_bits )
    d0                              a1      d1

    UBYTE CardMiscControl( struct CardHandle *, UBYTE );
```

Autodoc (`cardres.doc V45`, `/CardMiscControl`):

> **FUNCTION**
>     Used to set/clear miscellaneous control bits (generally for
>     use with I/O cards).

The interesting bits:

- `CARDF_DISABLE_WP` — disable the gate-array's hardware
  write-protect enforcement (some I/O cards don't connect the WE line)
- `CARDF_ENABLE_DIGAUDIO` — enable digital audio routing to the
  slot; on some hardware this is also the signal to switch the slot
  into I/O mode, so I/O card drivers should always set this bit
- V39+ control bits for enabling/disabling status-change interrupts
  on BVD1/SC, BVD2/DA, and BSY/IRQ (see `CARD_INTF_*` in `card.h`)

### `CardProgramVoltage` — set programming voltage

```
    success = CardProgramVoltage( handle, voltage )
                                  a1      d0

    LONG CardProgramVoltage( struct CardHandle *, ULONG );
```

Autodoc (`cardres.doc V45`, `/CardProgramVoltage`):

> **FUNCTION**
>     Used to set programming voltages (e.g., for FLASH-ROM/EPROM
>     cards).
>
> **RETURNS**
>      1  - Successful.
>      0  - Not successful.  Most likely because the credit-card
>      card has been removed, and you are no longer the owner.
>     -1  - This function is not being supported.  On some machines
>      with a minimal (hardware) credit-card interface, this feature
>      may not be possible.
>
>     !!!WARNING!!!
>
>     Flash-ROM programming requires careful coding to prevent
>     leaving the Erase command on too long.  Failure to observe
>     the maximum time between the Erase command, and the Erase-Verify
>     command can make a Flash-ROM card unusable.

Voltages are `CARD_VOLTAGE_0V`, `CARD_VOLTAGE_5V`, `CARD_VOLTAGE_12V`.
A `-1` return means the current interface hardware can't program the
card (A600 sometimes, A1200 generally can).

### `CardResetCard` — reset credit card

```
    success = CardResetCard( handle )
                             a1

    BOOL CardResetCard( struct CardHandle * );
```

Autodoc (`cardres.doc V45`, `/CardResetCard`):

> **FUNCTION**
>     Used to reset a credit-card.  Some cards, such as some
>     configurable cards can be reset.
>
>     Asserts credit-card reset for at least 10us.
>
>     It is the responsibility of the card owner to reset
>     configurable cards, or any other type of card such as
>     some I/O cards before calling ReleaseCard() if the owner
>     has made use of that card such that it is no longer in its
>     reset state.

### `CardResetRemove` — set/clear reset-on-removal

```
    success = CardResetRemove( handle, flag )
                               a1      d0

    BOOL CardResetRemove( struct CardHandle *, ULONG );
```

Autodoc (`cardres.doc V45`, `/CardResetRemove`):

> **FUNCTION**
>     Used to set/clear HARDWARE RESET on card change detect.
>
>     This function should generally not be used by devices
>     which support HOT-REMOVAL.  HARDWARE RESET on removal
>     is generally intended for execute-in-place software, or
>     ram cards whose memory has been added as system ram.

Returns `1` on success, `0` on failure, `-1` if the function is not
supported on this hardware.

### `CardAccessSpeed` — select best memory access speed

```
    result = CardAccessSpeed( handle, nanoseconds )
    d0                        a1      d0

    ULONG CardAccessSpeed( struct CardHandle *, ULONG );
```

Autodoc (`cardres.doc V45`, `/CardAccessSpeed`):

> **FUNCTION**
>     This function is used to set memory access speed for all CPU
>     accesses to card memory.
>
>     Typically this information would be determined by first examining
>     the Card Information Structure.
>
> **RETURNS**
>     Speed - Access speed selected by resource (in nanoseconds).
>     0     - Not successful.

Note that the autodoc's name is `CardAccessSpeed`, not `CardAccess`
— the task brief lists "CardAccess" but the actual function is
`CardAccessSpeed`.

### `CardInterface` — determine the type of card interface

```
    return = CardInterface()
    d0

    ULONG CardInterface( void );
```

Autodoc (`cardres.doc V45`, `/CardInterface`):

> **FUNCTION**
>     This function is used to determine the type of credit-card
>     (hardware) interface available.

Currently only `CARD_INTERFACE_AMIGA_0` is defined.

### `CardChangeCount` — obtain the card change count

```
    count = CardChangeCount( VOID )
    d0

    ULONG CardChangeCount( VOID );
```

Autodoc (`cardres.doc V45`, `/CardChangeCount`):

> **FUNCTION**
>     This function returns the card change count.  The
>     counter is incremented by one for every removal, and
>     for every successful insertion (a card which is inserted
>     long enough to be debounced before it is removed again).

### `CardForceChange` — force a card change

```
    success = CardForceChange( VOID )
    d0

    BOOL CardForceChange( VOID );
```

Autodoc (`cardres.doc V45`, `/CardForceChange`):

> **FUNCTION**
>     This function is not intended for general use.  Its
>     purpose is to force a credit-card change as if
>     the user had removed, or inserted a card.

Used by utility programs that need to kick the current owner off a
card without the user physically ejecting it.

### `CopyTuple` / `DeviceTuple` — CIS tuple helpers

```
    success = CopyTuple( handle, buffer, tuplecode, size )
    d0                   a1      a0      d1         d0

    return  = DeviceTuple( tuple_data, storage )
    d0                     a0          a1

    BOOL  CopyTuple( struct CardHandle *, UBYTE *, ULONG, ULONG );
    ULONG DeviceTuple( UBYTE *, struct DeviceTData * );
```

`CopyTuple` scans the card's Card Information Structure for a specific
tuple code and copies it (including tuple chain following and handling
of linked long-link targets); `DeviceTuple` parses a `CISTPL_DEVICE`
tuple into a `DeviceTData` structure of size/speed/type. Both exist so
that individual drivers don't have to reimplement CIS parsing.

### `IfAmigaXIP` — check if a card is an execute-in-place card

```
    result = IfAmigaXIP( handle )
    d0                   a2

    struct Resident *IfAmigaXIP( struct CardHandle * );
```

Autodoc (`cardres.doc V45`, `/IfAmigaXIP`):

> **FUNCTION**
>     Check to see if a card in the slot is an Amiga execute-in-place
>     card.  The Card Information Structure must have a valid
>     CISTPL_AMIGAXIP tuple.
>
>     The system polls for cards which have a CISTPL_AMIGAXIP tuple
>     at the same time that it searches for devices to boot off.
>     When a card with a valid CISTPL_AMIGAXIP tuple is found, the
>     system will call your ROM-TAG via Exec's InitResident() function.

Returns a pointer to the card's `Resident` tag if it's a valid XIP
card, `NULL` otherwise.

---

## 10. `FileSystem.resource`

Name constant (`resources/filesysres.h`):

```c
#define FSRNAME "FileSystem.resource"
```

`FileSystem.resource` is a **registry of file-system binaries** known
to the running system. Unlike the other resources in this document, it
arbitrates no hardware at all — it is simply a named list of
`FileSysEntry` nodes, one per known file system. Callers that need to
mount a partition find the right entry by DOS type.

The primary user is `expansion.library`: during boot it walks the
`RigidDiskBlock` partition table, and for each partition's DOS type it
looks up a matching `FileSysEntry` here, then patches the appropriate
fields into the `DosNode` it is building. That is how FFS, SFS,
PFS3, and custom file systems plug into DOS without each having to be
a device or a library.

### Autodoc background

`filesysres.doc V45` has no per-function entries at all — the resource
has no C-callable functions. Its autodoc is a single background entry:

> **PURPOSE**
>     The FileSystem.resource is where boot disk drivers rendezvous
>     to share file system code segments for partitions specified by
>     dos type.  Prior to V36, it was created by the first driver
>     that needed to use it.  For V36, its creation is ensured by the
>     rom boot process.
>
> **CONTENTS**
>     The FileSystem.resource is described in the include file
>     resources/filesysres.h.  The nodes on it describe how to
>     algorithmically convert the result of MakeDosNode (from the
>     expansion.library) to a node appropriate for the dos type.

All access is via direct inspection of the `FileSysResource` struct
after `OpenResource(FSRNAME)`. The list is walked under `Forbid()` or
via a semaphore the caller takes itself — this resource is a shared
data store, not a jump table.

### Struct definitions

From `resources/filesysres.h`:

```c
struct FileSysResource {
    struct Node fsr_Node;             /* on resource list */
    char       *fsr_Creator;          /* name of creator */
    struct List fsr_FileSysEntries;   /* list of FileSysEntry structs */
};

struct FileSysEntry {
    struct Node fse_Node;      /* on fsr_FileSysEntries list
                                  ln_Name is of creator of this entry */
    ULONG   fse_DosType;       /* DosType of this FileSys */
    ULONG   fse_Version;       /* Version (hi word) / revision (lo) */
    ULONG   fse_PatchFlags;    /* bits set for fields that should be
                                  patched into a DosNode: e.g. 0x180
                                  for SegList & GlobalVec */
    ULONG   fse_Type;          /* device node type: zero */
    CPTR    fse_Task;          /* standard dos "task" field */
    BPTR    fse_Lock;          /* must be zero */
    BSTR    fse_Handler;       /* filename to loadseg (if SegList null);
                                  V36+: bit 31 set => not AmigaDOS */
    ULONG   fse_StackSize;     /* stacksize when starting task */
    LONG    fse_Priority;      /* task priority when starting task */
    BPTR    fse_Startup;       /* FileSysStartupMsg for disks */
    BPTR    fse_SegList;       /* code to run to start new task */
    BPTR    fse_GlobalVec;     /* BCPL global vector */
    /* no more entries need exist than those implied by fse_PatchFlags */
};
```

`fse_PatchFlags` is a bitmask saying which fields in the
`FileSysEntry` are to be substituted into the equivalent fields of a
`DosNode` built by `MakeDosNode`. For example, the fast file system
entry in V36 has `fse_PatchFlags = 0` because `MakeDosNode` already
produces a DosNode compatible with FFS and nothing needs patching.
A file system that ships its own BCPL global-vector-based handler
code needs `0x180` to patch `fse_SegList` and `fse_GlobalVec` into the
DosNode.

The fact that the struct is *variable length* — only fields up to the
last bit set in `fse_PatchFlags` need to exist — is a subtle and
important property. You must not blindly write a longer `FileSysEntry`
into a list that might be read by an older caller, and when walking
the list you must not read past the bit in `PatchFlags`.

### Typical contents

A booted A1200 ROM system usually contains:

| DOS type   | ASCII | Notes                                                   |
|------------|-------|---------------------------------------------------------|
| 0x444F5300 | "DOS\0" | Original file system (OFS)                          |
| 0x444F5301 | "DOS\1" | Fast file system (FFS)                              |
| 0x444F5302 | "DOS\2" | International OFS                                   |
| 0x444F5303 | "DOS\3" | International FFS                                   |
| 0x444F5304 | "DOS\4" | Directory-cache OFS                                 |
| 0x444F5305 | "DOS\5" | Directory-cache FFS                                 |

Third-party file systems add their own entries, either by loading a
ROM-file-system module off the `RigidDiskBlock` and calling
`AddHead`/`AddTail` on `fsr_FileSysEntries` themselves, or via
`L:FileSystem_Trans` / `SYS:L` handlers on the boot device.

### How `AddBootNode` finds the right file system

Rough sequence inside `expansion.library`:

1. Read `RigidDiskBlock`, `PartitionBlock`s, and `FileSystemHeaderBlock`s
   off the hard disk.
2. For each `FileSystemHeaderBlock`, check `FileSystem.resource` for an
   existing entry with matching `fse_DosType` and higher-or-equal
   `fse_Version`. If none, load the file-system binary from the RDB,
   allocate a new `FileSysEntry` on the resource list, and fill in
   the fields from the header.
3. For each partition, call `MakeDosNode` to build a base `DosNode`,
   then walk the `FileSysEntry` list for that partition's DOS type
   and patch the fields indicated by `fse_PatchFlags`.
4. Call `AddBootNode` (or `AddDosNode`) to publish the patched
   `DosNode` so DOS will see it.

For the gory details of the patching logic, see `expansion.doc V45`
under `MakeDosNode` and `AddBootNode`.

---

## 11. `nonvolatile.library` (CDTV/CD32 NVRAM)

Name constant (from `libraries/nonvolatile.h`):

```c
/* Open with OpenLibrary("nonvolatile.library", 40); */
```

`nonvolatile.library` is not strictly a resource — it opens with
`OpenLibrary` and closes with `CloseLibrary` and is reference counted
— but it plays a resource-like role on **CDTV** and **CD32** where
it abstracts access to **onboard NVRAM** used to persist game saves
and small configuration blobs. Included here because the task brief
asks for it, and because the NVRAM chip *is* an arbitrated
single-piece-of-hardware and the model is small.

### Background (edited from `nonvolatile.doc V45`, `/--background--`)

> The nonvolatile library provides a simple means for an application
> developer to manage nonvolatile storage.
>
> The nonvolatile library is meant to be used transparently across all
> configurations. Currently, nonvolatile storage may consist of NVRAM
> and/or disk devices. nonvolatile.library will automatically
> access the best nonvolatile storage available in the system. Disk
> based storage will be selected first and if not available, NVRAM
> storage will be accessed.
>
> * NVRAM
>
> On low-end diskless Amiga platforms, NVRAM may be available. This
> RAM will maintain its data contents when the system is powered down.
> This is regardless of whether batteries or battery-backed clock are
> present. The data stored in NVRAM is accessible only through the
> ROM-based nonvolatile library funtion calls. The size of NVRAM
> storage is dependant on the system platform and is attainable through
> the GetNVInfo() function.
>
> * Disk
>
> In keeping with the general configurability of the Amiga, the actual
> disk location used by nonvolatile library when storing to disk may be
> changed by the user.

The library is layered: on a disk-bearing Amiga it simply stores
records in a configurable directory (defaulting to
`prefs/env-archive/sys/nv_location`); on the CD32 it uses the
onboard NVRAM chip; on a diskless CDTV, similarly.

Data is organised as `(appName, itemName)` pairs. Each pair is a
record. No structure is imposed on the record contents beyond a
checksum.

Important restriction from the background:

> **!!!NOTE!!!**
> Because NVRAM performs disk access, you must open and use its
> functionality from a DOS process, not an EXEC task.

### Function sketch (all `(V40)`)

`GetCopyNV` — fetch a copy of a stored item:

```
    data = GetCopyNV(appName, itemName, killRequesters)
    D0               A0       A1        D1

    APTR GetCopyNV(STRPTR, STRPTR, BOOL);
```

> **FUNCTION**
>     Searches the nonvolatile storage for the indicated appName and
>     itemName. A pointer to a copy of this data will be returned.
>
>     The strings appName and itemName may not contain the '/' or ':'
>     characters.

The returned buffer was allocated by the library and **must** be
freed via `FreeNVData()`.

`FreeNVData` — release buffer allocated by library functions:

```
    FreeNVData(data)
               A0

    VOID FreeNVData(APTR);
```

`StoreNV` — save data in nonvolatile storage:

```
    error = StoreNV(appName, itemName, data, length, killRequesters)
    D0              A0       A1        A2    D0      D1

    UWORD StoreNV(STRPTR, STRPTR, APTR, ULONG, BOOL);
```

> **FUNCTION**
>     Saves some data in nonvolatile storage. The data is tagged with
>     AppName and ItemName so it can be retrieved later. No single
>     item should be larger than one fourth of the maximum storage as
>     returned by GetNVInfo().

`length` is in **units of 10 bytes** — a non-obvious footgun. 23
bytes = 3; 147 bytes = 15.

Error codes: `NVERR_BADNAME`, `NVERR_WRITEPROT`, `NVERR_FAIL`,
`NVERR_FATAL`.

`DeleteNV` — remove a record:

```
    success = DeleteNV(appName, itemName, killRequesters)
    D0                 A0       A1        D1

    BOOL DeleteNV(STRPTR, STRPTR, BOOL);
```

`GetNVInfo` — query the selected backing store:

```
    info = GetNVInfo(killRequesters)
    D0               D1

    struct NVInfo *GetNVInfo(BOOL);
```

Returns an `NVInfo` with `nvi_MaxStorage` and `nvi_FreeStorage` rounded
to the nearest 10 bytes. Must be freed with `FreeNVData`.

`GetNVList` — enumerate records for an app:

```
    list = GetNVList(appName, killRequesters)
    D0               A0       D1

    struct MinList *GetNVList(STRPTR, BOOL);
```

Returns a MinList of `NVEntry` nodes. Must be freed with `FreeNVData`.

`SetNVProtection` — set the delete-protection flag:

```
    success = SetNVProtection(appName, itemName, mask, killRequesters)
    D0                        A0       A1        D2    D1

    BOOL SetNVProtection(STRPTR, STRPTR, LONG, BOOL);
```

Only the delete bit (`NVEF_DELETE`/`NVEB_DELETE`) is legal in the
mask; setting any other bit is undefined.

The `killRequesters` argument on every function suppresses
system-level requester dialogs (e.g. "please insert disk NV:") so
that autonomous code can use the library without tripping over a
missing storage medium.

### 1 KB NVRAM layout (CD32)

The CD32's NVRAM chip is 1024 bytes. The library stores records as a
doubly-linked list with a checksum header; application code should
never write to the NVRAM directly and should treat storage as opaque.

---

## 12. `lowlevel.library` (V40 joypad/joystick/timer)

`lowlevel.library` ships with V40+ Kickstart and is the CD32-oriented
low-latency input/output API. It is not a resource in the strict
sense, but it is the blessed public interface to CD32 joypad
handling, raw keyboard reads, VBlank/timer-interrupt hooks, and the
battery-backed audio clock. It is included here because the task
brief asks for it, and because most of its functionality would
otherwise require direct `cia.resource`/`potgo.resource` gymnastics.

All functions are `(V40)` unless noted.

### Input

**`ReadJoyPort`** — return the state of a joy/mouse port:

```
    portState = ReadJoyPort(portNumber)
    D0                      D0

    ULONG ReadJoyPort(ULONG);
```

> **FUNCTION**
>     This function is used to determine what device is attached to the
>     joy port and the current position/button state. The user may attach
>     a mouse, game controller, or joystick to the port and this function
>     will dynamically detect which device is attached and return the
>     appropriatly formatted portState.

Auto-senses device type. First call per port must be from a task
context to acquire CIA/POTGO resources; once acquired, can be called
from interrupt context.

Return bit layout varies by device: `JP_TYPE_GAMECTLR` (CD32 pad,
7-button layout plus D-pad), `JP_TYPE_MOUSE`, `JP_TYPE_JOYSTK`,
`JP_TYPE_NOTAVAIL`, `JP_TYPE_UNKNOWN`.

**`SetJoyPortAttrsA`** — override auto-sense:

```
    success = SetJoyPortAttrsA(portNumber, tagList)
    D0                         D0          A1

    BOOL SetJoyPortAttrsA(ULONG, struct TagItem *);
```

Tags: `SJA_Type` (`SJA_TYPE_GAMECTLR`/`MOUSE`/`JOYSTK`/`AUTOSENSE`),
`SJA_Reinitialize`. If you force a type, **you must reset it to
auto-sense before exiting**.

### Keyboard

**`GetKey`** — currently pressed rawkey + qualifiers:

```
    key = GetKey()
    D0

    ULONG GetKey(VOID);
```

Low-order word is the rawkey code (0xFF if none); high-order word
holds `LLKB_*` qualifier flags.

**`QueryKeys`** — state of a set of keys:

```
    QueryKeys(queryArray, arraySize)
              A0          D1

    VOID QueryKeys(struct KeyQuery *, UBYTE);
```

Fill in `kq_KeyCode` for each entry; returns `kq_Pressed` in the
same array.

**`AddKBInt`** / **`RemKBInt`** — hook the keyboard interrupt:

```
    intHandle = AddKBInt(intRoutine, intData)
    D0                   A0          A1

    APTR AddKBInt(APTR, APTR);
    VOID RemKBInt(APTR);
```

Only one handler at a time per system. Routine receives rawkey
in D0, `intData` in A1, `intRoutine` in A5, and must set D0=0 on exit.

### VBlank and CIA timer hooks

**`AddVBlankInt`** / **`RemVBlankInt`** — hook vertical-blank:

```
    intHandle = AddVBlankInt(intRoutine, intData)

    APTR AddVBlankInt(APTR, APTR);
    VOID RemVBlankInt(APTR);
```

**`AddTimerInt`** / **`RemTimerInt`** / **`StartTimerInt`** /
**`StopTimerInt`** — allocate a CIA interval timer:

```
    intHandle = AddTimerInt(intRoutine, intData)
    VOID RemTimerInt(APTR);
    VOID StartTimerInt(APTR, ULONG timeInterval, BOOL continuous);
    VOID StopTimerInt(APTR);
```

`AddTimerInt` goes through `cia.resource` to grab any free CIA timer.
`timeInterval` is in microseconds, max `90000` (larger values produce
"unexpected results"). `continuous=TRUE` for repeating,
`FALSE` for one-shot.

These functions are how CD32 games ended up calling `cia.resource`
without having to speak the resource directly — the library picks an
ICR bit, installs an interrupt handler, and returns an opaque handle.

### Timing and language

**`ElapsedTime`** — time elapsed since last call:

```
    fractionalSeconds = ElapsedTime(context)
    D0                              A0

    ULONG ElapsedTime(struct EClockVal *);
```

Uses `timer.device/ReadEClock` underneath. Fixed-point 16.16; up to
~16 hours; ~20 µs resolution, ~200 µs accuracy. First call returns
garbage (it's establishing the reference EClockVal).

**`GetLanguageSelection`** — user's language preference:

```
    language = GetLanguageSelection()
    D0

    ULONG GetLanguageSelection(VOID);
```

Used by games to pick language resources. Constants in
`libraries/lowlevel.h`.

### `SystemControlA` — selectively disable OS features

```
    failTag = SystemControlA(tagList)
    D0                       A1

    ULONG SystemControlA(struct TagItem *);
```

Task-exclusive: "only one task can hold (set to TRUE) that tag. If
another task attempts to set the same tag to TRUE, the call to
SystemControl() will fail." Used by games that want to take over the
system — disable task switching, disable mouse pointer, etc. —
cooperatively rather than by zapping hardware registers.

Because `lowlevel.library` is the sanctioned route, a game that uses
`SystemControlA` + `AddVBlankInt` + `ReadJoyPort` can take over the
machine without ever touching CIA/POTGO directly, and will keep
working on future hardware.

---

## 13. Resource boot order

During Kickstart boot (see `amiga-boot-process.md` and
`amiga-kickstart-rom-internals.md` for the whole story), resources
come up in a carefully ordered sequence because later resources
depend on earlier ones. Rough order on V40 ROM:

1. **exec.library** — allocates `ExecBase`, sets up the `ResourceList`
   itself (empty).
2. **`ciaa.resource` and `ciab.resource`** — initialised by resident
   code very early because almost everything else needs interrupt
   arbitration. After this, the system can install interrupt vectors
   on CIA bits.
3. **`disk.resource`** — installed before `trackdisk.device` because
   `trackdisk.device` opens `disk.resource` and `cia.resource` on
   init.
4. **`battclock.resource` and `battmem.resource`** — installed before
   DOS so that boot code can read the current time and per-machine
   boot preferences. On A500 without a clock, both resources may be
   absent or stubs.
5. **`potgo.resource`** — installed before `input.device` /
   `gameport.device`.
6. **`misc.resource`** — installed before any driver that wants to
   touch the Paula-side serial registers or the CIA-A parallel data
   port. Typically before `serial.device` and `parallel.device`.
7. **`FileSystem.resource`** — installed by ROM in V36+ so that
   `expansion.library` has somewhere to register file systems read
   off the RDB during early boot. Before V36, it was created lazily
   by the first driver that needed it.
8. **`card.resource`** — A600/A1200/CD32 only, installed after
   `cia.resource` (because it uses CIA-A interrupt bits via Gayle).
9. **expansion.library** — walks AutoConfig, reads any hard disk's
   RDB, loads filesystems into `FileSystem.resource`, mounts
   `DF0:`, and posts `DosNode`s for DOS to pick up.
10. **dos.library** — initialises, resolves handlers from
    `FileSystem.resource`, opens the boot file system.
11. **`nonvolatile.library`** — installed by name but only opened
    as needed. CDTV/CD32 boot scripts may open it early.
12. **`lowlevel.library`** — installed in V40 ROM. Opened on demand.

The exact order is a matter of `Resident` initialisation priorities
in ROM; the list above is the observable order, not the `rt_Pri`
values. The details are in the Kickstart ROM-tag table, documented
in `amiga-kickstart-rom-internals.md`.

---

## 14. Resource use patterns

A lookup table for "I want to do X — what resource do I actually need
to go through?":

| Task you want to do                                       | Resource/library you must use         | Typical higher-level wrapper           |
|-----------------------------------------------------------|---------------------------------------|----------------------------------------|
| Program a CIA interval timer for a periodic interrupt     | `cia.resource` (AddICRVector/AbleICR) | `timer.device`, `lowlevel.library/AddTimerInt` |
| Read/write CIA PRA/PRB (not the parallel port data lines) | `cia.resource` (AddICRVector to own, then direct bit access) | —                          |
| Hook the 50/60 Hz VBlank TOD alarm                        | `cia.resource` (CIA-A ICR bit 2)       | `graphics.library` VBlank server chain |
| Read a raw floppy track (bypassing AmigaDOS)              | `disk.resource` (AllocUnit/GetUnit)   | `trackdisk.device` TD_RAWREAD         |
| Write to a floppy in a custom format                      | `disk.resource` + CIA-B bits via cia.resource | custom loader                |
| Get or set the time of day                                | `battclock.resource`                  | `dos.library/DateStamp`, `locale.library` |
| Persist a system-wide boot preference (<50 B)             | `battmem.resource`                    | —                                      |
| Read paddle input                                         | `potgo.resource` (DATxx + START)      | `gameport.device`                     |
| Three-button mouse detection                              | `potgo.resource` (OUTxx bits)         | `input.device`                        |
| Take over the Paula UART for a custom protocol            | `misc.resource/MR_SERIALPORT`          | `serial.device`                       |
| Take over CTS/DTR only, leaving UART to system            | `misc.resource/MR_SERIALBITS`          | —                                      |
| Take over the parallel data lines                         | `misc.resource/MR_PARALLELPORT`        | `parallel.device`                     |
| Take over parallel ACK/BUSY interrupts                    | `misc.resource/MR_PARALLELBITS` + `cia.resource` | `parallel.device`           |
| Talk to a PCMCIA card (A600/A1200/CD32)                   | `card.resource` (OwnCard notification)| card-specific device drivers          |
| Look up a file system binary by DOS type                  | `FileSystem.resource` (walk list)     | `expansion.library/AddBootNode`       |
| Persist a game save on CD32/CDTV                          | `nonvolatile.library`                  | —                                      |
| Read a CD32 joypad                                        | `lowlevel.library/ReadJoyPort`        | `gameport.device` (less common)       |

The rule across the whole table: **always go through the highest
layer that gives you what you need**. The resources exist to make the
lower layers tractable, not as a recommended API. You only descend to
the resource when you have a genuine need (custom protocol,
copy-protection, extreme latency, hardware the stock drivers don't
know about).

---

## Appendix A: function index

### `exec.library` (resource framework)

| Function      | Page                         |
|---------------|------------------------------|
| AddResource   | [§2](#2-the-exec-resourcelist-api) |
| OpenResource  | [§2](#2-the-exec-resourcelist-api) |
| RemResource   | [§2](#2-the-exec-resourcelist-api) |

### `ciaa.resource` / `ciab.resource`

| Function      | Page                |
|---------------|---------------------|
| AbleICR       | [§3](#3-ciaresource)|
| AddICRVector  | [§3](#3-ciaresource)|
| RemICRVector  | [§3](#3-ciaresource)|
| SetICR        | [§3](#3-ciaresource)|

### `disk.resource`

| Function    | Page                  |
|-------------|-----------------------|
| AllocUnit   | [§4](#4-diskresource) |
| FreeUnit    | [§4](#4-diskresource) |
| GetUnit     | [§4](#4-diskresource) |
| GiveUnit    | [§4](#4-diskresource) |
| GetUnitID   | [§4](#4-diskresource) |
| ReadUnitID  | [§4](#4-diskresource) |

### `battclock.resource`

| Function        | Page                        |
|-----------------|-----------------------------|
| ReadBattClock   | [§5](#5-battclockresource)  |
| WriteBattClock  | [§5](#5-battclockresource)  |
| ResetBattClock  | [§5](#5-battclockresource)  |

### `battmem.resource`

| Function              | Page                      |
|-----------------------|---------------------------|
| ReadBattMem           | [§6](#6-battmemresource)  |
| WriteBattMem          | [§6](#6-battmemresource)  |
| ObtainBattSemaphore   | [§6](#6-battmemresource)  |
| ReleaseBattSemaphore  | [§6](#6-battmemresource)  |

### `potgo.resource`

| Function     | Page                   |
|--------------|------------------------|
| AllocPotBits | [§7](#7-potgoresource) |
| FreePotBits  | [§7](#7-potgoresource) |
| WritePotgo   | [§7](#7-potgoresource) |

### `misc.resource`

| Function          | Page                  |
|-------------------|-----------------------|
| AllocMiscResource | [§8](#8-miscresource) |
| FreeMiscResource  | [§8](#8-miscresource) |

### `card.resource`

| Function            | Page                            |
|---------------------|---------------------------------|
| OwnCard             | [§9](#9-cardresource-pcmcia)    |
| ReleaseCard         | [§9](#9-cardresource-pcmcia)    |
| BeginCardAccess     | [§9](#9-cardresource-pcmcia)    |
| EndCardAccess       | [§9](#9-cardresource-pcmcia)    |
| GetCardMap          | [§9](#9-cardresource-pcmcia)    |
| ReadCardStatus      | [§9](#9-cardresource-pcmcia)    |
| CardMiscControl     | [§9](#9-cardresource-pcmcia)    |
| CardProgramVoltage  | [§9](#9-cardresource-pcmcia)    |
| CardResetCard       | [§9](#9-cardresource-pcmcia)    |
| CardResetRemove     | [§9](#9-cardresource-pcmcia)    |
| CardAccessSpeed     | [§9](#9-cardresource-pcmcia)    |
| CardInterface       | [§9](#9-cardresource-pcmcia)    |
| CardChangeCount     | [§9](#9-cardresource-pcmcia)    |
| CardForceChange     | [§9](#9-cardresource-pcmcia)    |
| CopyTuple           | [§9](#9-cardresource-pcmcia)    |
| DeviceTuple         | [§9](#9-cardresource-pcmcia)    |
| IfAmigaXIP          | [§9](#9-cardresource-pcmcia)    |

### `FileSystem.resource`

(no C-callable functions; see `fsr_FileSysEntries` list walk.)

### `nonvolatile.library`

| Function         | Page                                        |
|------------------|---------------------------------------------|
| GetCopyNV        | [§11](#11-nonvolatilelibrary-cdtvcd32-nvram) |
| FreeNVData       | [§11](#11-nonvolatilelibrary-cdtvcd32-nvram) |
| StoreNV          | [§11](#11-nonvolatilelibrary-cdtvcd32-nvram) |
| DeleteNV         | [§11](#11-nonvolatilelibrary-cdtvcd32-nvram) |
| GetNVInfo        | [§11](#11-nonvolatilelibrary-cdtvcd32-nvram) |
| GetNVList        | [§11](#11-nonvolatilelibrary-cdtvcd32-nvram) |
| SetNVProtection  | [§11](#11-nonvolatilelibrary-cdtvcd32-nvram) |

### `lowlevel.library`

| Function              | Page                                             |
|-----------------------|--------------------------------------------------|
| ReadJoyPort           | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| SetJoyPortAttrsA      | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| GetKey                | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| QueryKeys             | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| AddKBInt              | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| RemKBInt              | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| AddVBlankInt          | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| RemVBlankInt          | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| AddTimerInt           | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| RemTimerInt           | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| StartTimerInt         | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| StopTimerInt          | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| ElapsedTime           | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| GetLanguageSelection  | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |
| SystemControlA        | [§12](#12-lowlevellibrary-v40-joypadjoysticktimer) |

Total: **3 exec.library framework functions + 51 resource/library
functions = 54 functions documented in total.**

---

## Appendix B: gaps

Places where the autodocs are sparse or this document is thinner than
it would like to be:

- **`disk.resource/AllocUnit`, `FreeUnit`, `GiveUnit`, `GetUnitID`**
  have no `SEE ALSO`, `EXCEPTIONS`, or `BUGS` in the NDK autodoc
  beyond what is quoted — the section reflects that.
- **`battclock.resource`** has no per-chip documentation at all in the
  autodoc. The MSM6242 vs RP5C01 distinction in §5 is informational,
  drawn from hardware references, not the NDK.
- **`battmem.resource` bit layouts** — the NDK ships
  `battmembitsamiga.h`, `battmembitsshared.h`, `battmembitsamix.h`
  with offset/length constants, but there is no autodoc describing
  what each field means semantically. This document doesn't reproduce
  the bit tables; see the headers directly.
- **`FileSystem.resource`** has no function entries in the autodoc at
  all (zero functions). Section 10 is entirely based on the background
  entry and the struct definitions. The `AddBootNode` patching logic
  is described in `expansion.doc`, not here.
- **`card.resource` V37 vs V39 differences** — the autodoc describes
  V39 extensions in mixed prose; this document covers the main V39
  extensions (`CARDF_POSTSTATUS`, V39 `CardMiscControl` int bits,
  extended `CardMemoryMap`) but does not attempt a full version
  diff.
- **`nonvolatile.library` on-disk format** — the autodoc documents the
  API but not the on-disk format. The 1KB NVRAM layout claim in §11 is
  general and not tied to a published Commodore spec.
- **`lowlevel.library/SystemControlA` tags** — the autodoc lists tags
  by name but this document does not reproduce the full
  `SCON_*` tag table. See `libraries/lowlevel.h`.
- **Boot order section (§13)** is informed by general Kickstart ROM
  knowledge, not directly by any single autodoc. Cross-reference
  `amiga-boot-process.md` and `amiga-kickstart-rom-internals.md` for
  the authoritative sequencing.

Nothing in the autodocs contradicts this document; the gaps are
omissions, not disagreements.

---

## Appendix C: source map

Primary sources, all in `ndk/NDK_3.9/`:

| Section                | Autodoc source                                 | Header source                                    |
|------------------------|------------------------------------------------|--------------------------------------------------|
| §2 Exec ResourceList   | `Documentation/Autodocs/exec.doc`              | `Include/include_h/exec/execbase.h`              |
| §3 cia.resource        | `Documentation/Autodocs/cia.doc`               | `Include/include_h/resources/cia.h`, `ciabase.h` |
| §4 disk.resource       | `Documentation/Autodocs/disk.doc`              | `Include/include_h/resources/disk.h`             |
| §5 battclock.resource  | `Documentation/Autodocs/battclock.doc`         | `Include/include_h/resources/battclock.h`        |
| §6 battmem.resource    | `Documentation/Autodocs/battmem.doc`           | `Include/include_h/resources/battmem.h`, `battmembits*.h` |
| §7 potgo.resource      | `Documentation/Autodocs/potgo.doc`             | `Include/include_h/resources/potgo.h`            |
| §8 misc.resource       | `Documentation/Autodocs/misc.doc`              | `Include/include_h/resources/misc.h`             |
| §9 card.resource       | `Documentation/Autodocs/cardres.doc`           | `Include/include_h/resources/card.h`             |
| §10 FileSystem.resource| `Documentation/Autodocs/filesysres.doc`         | `Include/include_h/resources/filesysres.h`       |
| §11 nonvolatile.library| `Documentation/Autodocs/nonvolatile.doc`       | `Include/include_h/libraries/nonvolatile.h`      |
| §12 lowlevel.library   | `Documentation/Autodocs/lowlevel.doc`          | `Include/include_h/libraries/lowlevel.h`         |

Version banner on all sources: **NDK 3.9, Includes Release 45.1**.

Cross-referenced documents in this repo:

- `amiga-hardware-reference.md` — custom-chip register layout behind
  each resource
- `amiga-io-audio-expansion.md` — `.device` drivers that consume these
  resources
- `amiga-headers-reference.md` — catalogue of all NDK headers
- `amiga-boot-process.md`, `amiga-kickstart-rom-internals.md` — the
  boot sequencing that instantiates resources
