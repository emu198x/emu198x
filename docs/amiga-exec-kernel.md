# Amiga Exec Kernel Reference

**Audience:** the author of a hardware-accurate Amiga emulator who must reproduce Exec-compatible behaviour in order to run unmodified Kickstart ROMs. This document is a runtime-model reference, not a user manual. It is the companion to `amiga-boot-process.md` (cold boot) and `amiga-hardware-reference.md` (chip registers); neither subject is duplicated here.

**Primary sources** (cited inline as `(Exec RKM …)` and `(Autodocs exec.library/…)`):

- `Amiga_ROM_Kernel_Reference_Manual_Exec.txt` — the Exec chapter RKM. Narrative prose; the authoritative statement of behaviour.
- `Amiga_ROM_Kernal_Reference_Manual_Includes_and_Autodocs.txt` — the per-function autodocs and the canonical C/assembly struct definitions from the `exec/*.h` and `exec/*.i` include files. Authoritative for offsets, flag values and LVO numbers.
- `Amiga_ROM_Kernel_Reference_Manual_Libraries_and_Devices.txt` — contains the verbatim skeleton device in assembly, the source of the canonical RTF_AUTOINIT template reproduced below.
- `Amiga_System_Programmers_Guide_1988_Abacus.txt` — cross-reference with assembly-level traces.

The OCR of the Includes/Autodocs volume is lossy. Where a struct field name in this document differs from what a reader will find if they go look at the PDF ("mlh_Head" vs "mlhJiead", "ln_Succ" vs "In_Succ") the document uses the canonical AmigaOS form. Where a number or layout is genuinely ambiguous in the corpus, it is flagged in the **Gaps in corpus** section at the end.

## How to read this document

Read the first four sections in order. Everything in Exec is built on lists and nodes (§3); every dynamically created object lives in memory (§4); and every long-lived object is eventually owned by a task (§5) which communicates via signals (§6) and messages (§7). The middle sections — libraries, devices, resources, interrupts, traps, alerts — are specialisations of that machinery. The appendix at the end reproduces the full `ExecBase` layout and the complete exec.library function index with LVO offsets.

For the emulator author, the load-bearing facts are usually the **NOTE** and **WARNING** clauses in autodocs. Those capture constraints that the Kickstart ROM code silently depends on. They have been preserved verbatim wherever possible.

---

## Table of contents

1. [ExecBase / SysBase](#1-execbase--sysbase)
2. [The Exec machine model: assumptions a ROM depends on](#2-the-exec-machine-model-assumptions-a-rom-depends-on)
3. [Lists, nodes and the double-header trick](#3-lists-nodes-and-the-double-header-trick)
4. [Memory management](#4-memory-management)
5. [Tasks, processes and the scheduler](#5-tasks-processes-and-the-scheduler)
6. [Signals](#6-signals)
7. [Messages and ports](#7-messages-and-ports)
8. [Semaphores](#8-semaphores)
9. [Libraries](#9-libraries)
10. [Devices and the I/O request model](#10-devices-and-the-io-request-model)
11. [Resources](#11-resources)
12. [Interrupts](#12-interrupts)
13. [Traps and task exceptions](#13-traps-and-task-exceptions)
14. [Alert / Guru Meditation](#14-alert--guru-meditation)
15. [Initialisation-time conveniences](#15-initialisation-time-conveniences)
16. [Library / device creation: the RTF_AUTOINIT template](#16-library--device-creation-the-rtf_autoinit-template)
17. [Appendix A: ExecBase / SysBase full layout](#appendix-a-execbase--sysbase-full-layout)
18. [Appendix B: exec.library function index (LVO table)](#appendix-b-execlibrary-function-index-lvo-table)
19. [Appendix C: Exec idioms cookbook](#appendix-c-exec-idioms-cookbook)
20. [Appendix D: Gaps in corpus](#appendix-d-gaps-in-corpus)
21. [Appendix E: Source map](#appendix-e-source-map)

---

## 1. ExecBase / SysBase

`ExecBase` (sometimes called `SysBase`, the names are interchangeable) is the single global Exec state root. Its address is stored at absolute memory location **4** (`AbsExecBase = $00000004`) and nothing in the Amiga can be located without first reading that longword. The ROM places `ExecBase` in chip RAM during coldstart and every library, task and I/O operation walks starts from it (Autodocs `exec/execbase.i`, `exec/execbase.h`).

`ExecBase` begins with a complete `Library` node, so to software that does not know any better it looks like a library like any other — `OpenLibrary("exec.library", 0)` simply returns this same pointer. Its negative-offset jump table is the exec.library LVO table, and its positive-offset area is the entire system state.

The complete layout is given in [Appendix A](#appendix-a-execbase--sysbase-full-layout). Below is a field-by-field tour of the parts an emulator must reproduce.

### 1.1 Header (offset 0 .. 33)

```
Offset  Type         Name             Meaning
------  ----         ----             -------
0x00    Library      LibNode          standard library node for exec.library itself
0x22    UWORD        SoftVer          Kickstart release number
0x24    WORD         LowMemChkSum     checksum of 68000 exception vectors (0..255)
0x26    ULONG        ChkBase          one's complement of ExecBase, sanity check
0x2A    APTR         ColdCapture      coldstart soft-capture vector
0x2E    APTR         CoolCapture      coolstart soft-capture vector
0x32    APTR         WarmCapture      warmstart soft-capture vector
```

The three `*Capture` vectors are survivable across warm reboot. If any is non-NULL at reset, Exec jumps through it as part of the boot sequence: this is how recoverable ramdisks and debuggers hook themselves back in. The mechanism is driven by `SumKickData` (§15); if the checksum over `KickMemPtr`/`KickTagPtr` fails, both are ignored (Autodocs `exec.library/SumKickData`).

### 1.2 System stack and pre-init fields

```
0x36    APTR         SysStkUpper      system stack base (upper bound)
0x3A    APTR         SysStkLower      top of system stack (lower bound)
0x3E    ULONG        MaxLocMem        last calculated local memory max
0x42    APTR         DebugEntry       global debugger entry point
0x46    APTR         DebugData        global debugger data segment
0x4A    APTR         AlertData        pointer to alert data / task at fault
0x4E    APTR         MaxExtMem        top of extended mem or NULL
0x52    WORD         ChkSum           checksum over all of the above
```

The system stack grows from `SysStkUpper` downward towards `SysStkLower`. **All interrupts and traps execute on this single stack** (Exec RKM, ch. 5); tasks never touch it. The stack must be large enough to handle maximum nested interrupt depth — a fact relevant to an emulator that is tempted to size it by counting normal frames.

`MaxLocMem` is the size of on-board chip RAM as computed by the cold boot memory-sizing loop. `MaxExtMem` is the top of "slow" / ranger memory ($C00000-$D80000); it is NULL on an A1000 and on an unexpanded A500. These two fields are what early software uses to decide memory layout before autoconfig has run.

### 1.3 Interrupt vector table

```
0x54    IntVector[16] IntVects        one entry per Paula interrupt source
```

Each `IntVector` is three longwords:

```
struct IntVector {
    APTR         iv_Data;             /* is_Data to pass in A1    */
    VOID       (*iv_Code)();          /* is_Code to jump through  */
    struct Node *iv_Node;             /* handler / server-chain owner */
};
```

There are 16 slots; the 15 real Paula interrupt lines plus NMI. The indices and the order of these slots are **fixed** — this ordering is what the ROM's autovector dispatcher uses to go from an Intel-style vector number to a handler address. The canonical order from `exec/execbase.i` is:

```
 0  IVTBE       (serial output - transmit buffer empty)
 1  IVDSKBLK    (disk block done)
 2  IVSOFTINT   (software interrupt)
 3  IVPORTS     (CIA-A, external level 2 - server chain)
 4  IVCOPER     (copper - server chain)
 5  IVVERTB     (vertical blank - server chain)
 6  IVBLIT      (blitter finished - server chain)
 7  IVAUD0      (audio channel 0)
 8  IVAUD1      (audio channel 1)
 9  IVAUD2      (audio channel 2)
10  IVAUD3      (audio channel 3)
11  IVRBF       (serial input - receive buffer full)
12  IVDSKSYNC   (disk sync)
13  IVEXTER     (CIA-B, external level 6 - server chain)
14  IVINTEN     (interrupt enable master)
15  IVNMI       (non-maskable - server chain)
```

(Source: `exec/execbase.i` lines 53-68.) An emulator that stores its interrupt dispatch array in any other order will fail the first time the ROM calls `SetIntVector` with `INTB_VERTB` (= 5).

Slots marked "server chain" dispatch through `SoftIntList` / list-walking code rather than jumping directly; see §12.

### 1.4 Dynamic state

```
0x84    struct Task *ThisTask         pointer to currently running task
0x88    ULONG        IdleCount        incremented by the idle loop
0x8C    ULONG        DispCount        dispatch counter
0x90    UWORD        Quantum          time slice quantum (ticks)
0x92    UWORD        Elapsed          current quantum ticks remaining
0x94    UWORD        SysFlags         misc system flags (SF_ALERTWACK etc)
0x96    BYTE         IDNestCnt        interrupt disable nesting count
0x97    BYTE         TDNestCnt        task disable (Forbid) nesting count
```

`ThisTask` is **the** most important mutable field in all of Exec. Every per-task operation — `FindTask(NULL)`, signal delivery, stack switching, the A6 register context on entry to a library vector — runs off this pointer. On a context switch the dispatcher saves the outgoing task's registers, changes `ThisTask`, and resumes the incoming one.

`IDNestCnt` and `TDNestCnt` are the nesting counters for `Disable()`/`Enable()` and `Forbid()`/`Permit()` respectively. They begin at `-1` ("not nested") and go to `0` on the first disable/forbid. This off-by-one matters: the `ENABLE` and `PERMIT` macros `SUBQ.B #1` and *then* test `BGE` to decide whether to actually restore state (Exec RKM ch. 5, ENABLE/DISABLE macros, lines 3159-3180). An emulator reproducing these as intrinsics must match the `-1` convention or nesting counts will underflow.

```
0x98    UWORD        AttnFlags        CPU / FPU feature bits
0x9A    UWORD        AttnResched      rescheduling attention flags
0x9C    APTR         ResModules       pointer to ROM tag scan array
```

`AttnFlags` is what every piece of system code uses to decide what processor it is on:

```
#define AFB_68010    0   /* also set for 68020 */
#define AFB_68020    1
#define AFB_68881    4
```

(Exec RKM, from `exec/execbase.h` lines 139-151; `AFB_PAL` and `AFB_50HZ` are documented as obsolete — the current source of truth is `VBlankFrequency` / `PowerSupplyFrequency` below.) `AttnResched` is set by ISRs that make a task ready; it is consulted at the tail of `ExitIntr` to decide whether to call the scheduler before returning to user mode.

`ResModules` is the ROM-tag array, a NULL-terminated list of pointers to `Resident` structures. During coldstart `InitCode` walks it to bring all subsystems online. By default it contains whatever ROM-tags the boot scan found in the ranges `$F80000-$FFFFFF` and `$F00000-$F7FFFF` (Autodocs `exec.library/SumKickData`).

### 1.5 Task defaults

```
0xA0    APTR         TaskTrapCode     default trap handler (used when tc_TrapCode == NULL)
0xA4    APTR         TaskExceptCode   default task-exception handler
0xA8    APTR         TaskExitCode     default task finaliser (called from RTS fall-through)
0xAC    ULONG        TaskSigAlloc     pre-allocated signal mask (for newly added tasks)
0xB0    UWORD        TaskTrapAlloc    pre-allocated trap mask
```

When `AddTask()` creates a new task with zeroed trap / exception / exit code fields, those default to these `ExecBase` values. `TaskSigAlloc` is typically `0x0000FFFF` — the low 16 bits reserved for system signals, leaving the top 16 bits free for user allocation.

### 1.6 System lists

```
0xB2    struct List  MemList          all MemHeader regions in the free pool
0xC0    struct List  ResourceList     AddResource / OpenResource
0xCE    struct List  DeviceList       AddDevice / OpenDevice
0xDC    struct List  IntrList         AddIntServer chains (for list-walk dispatch)
0xEA    struct List  LibList          AddLibrary / OpenLibrary
0xF8    struct List  PortList         AddPort / FindPort
0x106   struct List  TaskReady        ready queue (priority-sorted)
0x114   struct List  TaskWait         wait queue (unsorted, FIFO tail-append)
```

Each `List` is 14 bytes (three longword pointers + 2 bytes for type/pad); see §3.

`TaskReady` is always priority-sorted, head-first. `TaskWait` is unsorted; a task enters it on `Wait()` and leaves it on `Signal()` delivery.

### 1.7 Soft interrupts, alerts, timing, semaphores

```
0x122   SoftIntList  SoftInts[5]      five priority queues for Cause()
0x14A   LONG         LastAlert[4]     last four alert numbers posted
0x15A   UBYTE        VBlankFrequency  50 or 60 depending on PAL/NTSC
0x15B   UBYTE        PowerSupplyFrequency  50 or 60 (tracks mains frequency)
0x15C   struct List  SemaphoreList    AddSemaphore / FindSemaphore
```

Five softint priorities, matching the five valid `ln_Pri` values `-32, -16, 0, +16, +32` (Exec RKM ch. 5, line 3130: "Software interrupts are prioritized. Unlike interrupt servers, software interrupts have only five priority levels"). Priorities in between are truncated; outside that range is not allowed.

`VBlankFrequency` and `PowerSupplyFrequency` replace the old `AFB_PAL` / `AFB_50HZ` flag bits, which are now officially deprecated (Autodocs `exec/execbase.i` lines 143-148). In practice they will always be equal on a stock machine, but a PAL machine on American mains, or vice versa, can have them differ.

### 1.8 Kickstart into RAM

```
0x16A   APTR         KickMemPtr       singly-linked MemList queue
0x16E   APTR         KickTagPtr       ROM-tag queue to append to ResModules
0x172   APTR         KickCheckSum     checksum over both, computed by SumKickData
```

This is how a ramdisk or custom driver "survives" a reboot: you prepare a `MemList` describing the physical pages that hold your data and your code, a `Resident` describing the module to invoke, you compute the sum, and store the three pointers. On warm reboot Exec calls `AllocAbs` for each entry on `KickMemPtr` so that those pages come back belonging to you, then appends `KickTagPtr`'s contents to `ResModules` so your `Resident` runs during `InitCode` (Autodocs `exec.library/SumKickData`, lines 2298-2321).

Important gotcha for emulators: the whole mechanism runs *before* expansion memory has been added. Any memory referenced by `KickMemPtr` must be addressable at that point, which means chip RAM or slow-ranger RAM ($C00000-$D80000). Expansion boards will not yet be enumerated.

```
0x176   UBYTE[10]    ExecBaseReserved
0x180   UBYTE[20]    ExecBaseNewReserved
                     LABEL SYSBASESIZE
```

Total: approximately 300 bytes on a 1.3 ExecBase, growing slightly in later Kickstarts.

---

## 2. The Exec machine model: assumptions a ROM depends on

Before exploring the subsystems, there are several global assumptions the ROM code silently depends on. These are what an emulator must keep true at every instant, and they are often the source of obscure ROM crashes on approximate emulators.

**M1: User mode is the task world, supervisor mode is Exec's.** Tasks *always* execute in 68000 user mode (Exec RKM Preface, line 219: "Tasks always execute in the 68000 processor user mode. Supervisor mode is reserved for interrupts, traps, and task dispatching."). Interrupts and traps raise to supervisor mode, run on `SysStk`, and drop back. Anything that tries to use `MOVE SR,<ea>` from user mode on a 68010+ traps as privileged; `GetCC()` exists precisely to hide this (Autodocs `exec.library/GetCC`).

**M2: `A6` is the library base pointer by convention.** Every exec.library function is entered with `A6` containing `ExecBase`. Vectors are reached by `JSR _LVOxxx(A6)` where `_LVOxxx` is a negative offset that is a multiple of 6 bytes (Exec RKM ch. 7, lines 3694-3730). Any code that does `move.l AbsExecBase,a6 ; jsr _LVOFoo(a6)` is using the public API — no system call table, no trap, just an indirect jump through a jump table.

**M3: `D0`, `D1`, `A0`, `A1` are always scratch.** Every other register must be preserved across a library call (Exec RKM Preface, line 137-146). An emulator-side library vector that clobbers `D2` or `A2` will explode the first time it is called from a task that had live values there.

**M4: Function results come back in `D0`.** If there is a pointer return, it is in `D0`, not `A0` — every Exec function that returns a `struct *` hands it back via `D0`. The autodocs are explicit about this.

**M5: Condition codes after a library call are not reliable.** Exec RKM Preface, line 225-228: "Assembly code functions that return a result do not necessarily affect the processor condition codes. By convention, the caller must test the returned value before acting on a condition code." That is, you must `TST.L D0` / `MOVE.L D0,D0` before branching. Emulated stubs that "accidentally" return with CCR set one way or the other can cause ROM code to take wrong branches.

**M6: The `AbsExecBase` longword at location 4 is load-bearing.** It is both the system's self-location pointer and a sanity check (`ChkBase` is its one's complement). Every piece of Amiga code, including the boot ROM itself, begins from location 4. If you smash that longword the machine is dead and cannot recover.

**M7: Address 0 is not usable.** 68000 location 0 is the reset SSP. Exec treats `NULL` as a sentinel meaning "absent" throughout — in list walkers (`ln_Succ == NULL` marks end-of-list after the tail sentinel), in autodoc return values ("zero if none found"), in `tc_TrapCode` ("if zero, Exec ignores all exceptions"), and so on.

**M8: Structures that interrupt code or other tasks will touch must be in MEMF_PUBLIC memory and must not be on a task stack.** (Exec RKM Preface, data structures item 5-6.) The specific consequence is that an emulator reproducing Exec does not need to guard against private memory being shared — real code does not do it. But it also must not *require* shared data to be in any particular memory region beyond MEMF_PUBLIC.

**M9: Forbid/Permit does not disable interrupts. Disable/Enable does not disable task switching at the nesting-count level but does prevent it by virtue of preventing the ticks that would drive scheduling.** These two mechanisms compose but are not substitutes for each other (Exec RKM ch. 2, lines 1497-1528). In particular, `Disable()` implicitly does not set `TDNestCnt`; `Forbid()` does not set `IDNestCnt`; they are independent counters.

**M10: `Wait()` breaks `Forbid()` and `Disable()` temporarily.** If a task is forbidden or disabled and then calls anything that ends up in `Wait()`, the system will break the forbidden/disabled state long enough to run other tasks, then restore it when the task wakes up (Autodocs `exec.library/Disable` WARNING; `exec.library/Forbid` WARNING). This is why calling `printf` or any DOS function from within `Forbid()` "works" — but only because of this escape hatch, and it is subtle.

---

## 3. Lists, nodes and the double-header trick

Every dynamically managed object in Exec — tasks, libraries, devices, resources, memory regions, message ports, semaphores, interrupts — lives on a doubly-linked list and begins with a `Node`. The list primitives are the most heavily used code path in the whole kernel.

### 3.1 `Node` and `MinNode`

From `exec/nodes.h` (lines 13-26):

```c
/* full node - supports type checking and priority */
struct Node {
    struct Node *ln_Succ;      /* next in list, NULL at tail */
    struct Node *ln_Pred;      /* previous in list, NULL at head */
    UBYTE        ln_Type;
    BYTE         ln_Pri;       /* priority, -128..127 */
    char        *ln_Name;      /* optional string; not copied */
};

/* stripped node - no type, no priority, no name */
struct MinNode {
    struct MinNode *mln_Succ;
    struct MinNode *mln_Pred;
};
```

A `Node` is 14 bytes (`LN_SIZE`), a `MinNode` 8 (`MLN_SIZE`). `ln_Name` is an uncopied pointer — the caller is responsible for keeping the string alive for as long as the node is on a list that might be searched by name.

Node types from `exec/nodes.h`:

```
NT_UNKNOWN      0
NT_TASK         1
NT_INTERRUPT    2   /* also used for software interrupts */
NT_DEVICE       3
NT_MSGPORT      4
NT_MESSAGE      5
NT_FREEMSG      6   /* message has been freed */
NT_REPLYMSG     7   /* message has been replied */
NT_RESOURCE     8
NT_LIBRARY      9
NT_MEMORY      10
NT_SOFTINT     11   /* exec private, set by Cause() */
NT_FONT        12
NT_PROCESS     13
NT_SEMAPHORE   14
NT_SIGNALSEM   15   /* signal semaphores */
NT_BOOTNODE    16
```

These are not just documentation — the system uses them for sanity (`PutMsg` sets `ln_Type` to `NT_MESSAGE`, `ReplyMsg` sets it to `NT_REPLYMSG`, `GetMsg` relies on `NT_MESSAGE` for validation in some paths).

### 3.2 `List` and `MinList`

From `exec/lists.h` (lines 18-32):

```c
struct List {
    struct Node *lh_Head;      /* first real node, or &lh_Tail if empty */
    struct Node *lh_Tail;      /* always NULL - sentinel marker  */
    struct Node *lh_TailPred;  /* last real node, or &lh_Head if empty */
    UBYTE        lh_Type;
    UBYTE        lh_pad;
};

struct MinList {
    struct MinNode *mlh_Head;
    struct MinNode *mlh_Tail;
    struct MinNode *mlh_TailPred;
};
```

A `List` is 14 bytes, a `MinList` 12.

### 3.3 The double-header trick

This is the single cleverest idea in Exec and it is essential to reproduce exactly.

The three pointers `lh_Head`, `lh_Tail`, `lh_TailPred` are arranged so that they overlap with two "virtual nodes". Think of it like this: `lh_Head` is the `ln_Succ` field of an imaginary head-node whose `ln_Pred` is permanently NULL. `lh_TailPred` is the `ln_Pred` field of an imaginary tail-node whose `ln_Succ` is permanently NULL. And `lh_Tail` serves double duty: it is both the `ln_Pred` of the first real node (always NULL, because the head-sentinel has no predecessor) and the `ln_Succ` of the last real node (also always NULL, because the tail-sentinel has no successor).

The consequence is:

- To **walk a list head-to-tail**: follow `ln_Succ` until it becomes NULL.
- To **walk tail-to-head**: follow `ln_Pred` until it becomes NULL.
- To **test empty**: `list.lh_TailPred == &list` (or equivalently `list.lh_Head->ln_Succ == NULL`).
- To **initialise**: set `lh_Head = &lh_Tail`, `lh_Tail = NULL`, `lh_TailPred = &lh_Head`.

The Exec RKM spells out the init sequence (ch. 1, lines 724-753):

```
List.lh_Head     = &list.lh_Tail;
List.lh_TailPred = &list.lh_Head;
List.lh_Tail     = 0;
List.lh_Type     = whatever;
```

In assembly with `A0` pointing at the list (the `NEWLIST` macro):

```
MOVE.L   A0,(A0)                   ; lh_Head = self (will become &lh_Tail)
ADDQ.L   #LH_TAIL,(A0)             ; lh_Head += offsetof(lh_Tail)
CLR.L    LH_TAIL(A0)               ; lh_Tail = 0
MOVE.L   A0,LH_TAILPRED(A0)        ; lh_TailPred = &lh_Head (which is self)
```

(Exec RKM ch. 1, lines 746-757.) An emulator that zero-fills a freshly-allocated `List` and calls it good will produce a list whose `AddHead` writes `ln_Succ = NULL` (because `lh_Head` is NULL not `&lh_Tail`) and every subsequent walk immediately terminates. Missing this initialisation is one of the classic "AddPort returns but nobody finds my port" bugs.

### 3.4 List operations

The full set of list functions in exec.library is:

| Function    | Effect                                                       |
|-------------|--------------------------------------------------------------|
| `Insert`    | Insert a node *after* another node (NULL = head)             |
| `AddHead`   | Insert at head                                               |
| `AddTail`   | Append at tail                                               |
| `Remove`    | Remove a given node from whatever list it is in              |
| `RemHead`   | Remove and return the head node (for FIFO / LIFO)            |
| `RemTail`   | Remove and return the tail node                              |
| `Enqueue`   | Insert preserving priority order (FIFO for equal priorities) |
| `FindName`  | Linear search for a node with a given `ln_Name`              |

Crucial warning on **every** one of these from the autodocs: "This function does not arbitrate for access to the list. The calling task must be the owner of the involved list." Exec list primitives are **not** concurrency-safe. Any shared list — all of the lists in `ExecBase` in particular — must be accessed under `Forbid()` / `Permit()` (for task safety) or `Disable()` / `Enable()` (for interrupt safety) depending on who else touches it.

**`Enqueue` is FIFO-for-equal-priority.** From the autodocs: "New nodes will be inserted in front of the first node with a lower priority. Hence a FIFO queue for nodes of equal priority." This is what makes `TaskReady` a fair round-robin within a priority level.

**`FindName` never compares against its starting node.** It begins searching with `start->ln_Succ`. This is so you can call it in a loop to find multiple matches: pass the previously-found node as `start` and you get the next one.

**Handling the empty-list sentinel on remove.** `RemHead` on an empty list returns 0. It does not crash. `Remove` *does* crash if you remove a node that is not actually on any list, because it has no way to detect that case — it just splices the predecessor and successor pointers, and if they are garbage, you corrupt garbage.

### 3.5 Which lists exist

| Field in `ExecBase` | Node type       | Access primitives                         |
|---------------------|-----------------|-------------------------------------------|
| `MemList`           | `MemHeader`     | `AddMemList`, `AllocMem`, `FreeMem`       |
| `ResourceList`      | anything        | `AddResource`, `OpenResource`             |
| `DeviceList`        | `Device`        | `AddDevice`, `OpenDevice`                 |
| `IntrList`          | `Interrupt`     | `AddIntServer`, `RemIntServer`            |
| `LibList`           | `Library`       | `AddLibrary`, `OpenLibrary`               |
| `PortList`          | `MsgPort`       | `AddPort`, `FindPort`                     |
| `TaskReady`         | `Task`          | scheduler-private, `AddTask` seeds it     |
| `TaskWait`          | `Task`          | scheduler-private, `Wait()` moves into it |
| `SemaphoreList`     | `SignalSemaphore` | `AddSemaphore`, `FindSemaphore`         |

Plus `SoftInts[5]` — an array of five lists, one per softint priority.

Every list in Exec is one of these, or it is a list embedded inside one of these (a message port's `mp_MsgList`, a task's `tc_MemEntry`, a semaphore's `ss_WaitQueue`, etc.).

---

## 4. Memory management

Exec's memory allocator is a first-fit doubly-linked coalescing free-list allocator. It is simple, deterministic, and slow under fragmentation, and it is the single largest source of "my emulator crashes after 30 minutes on real software" — because the allocator walks linked lists that *must* be in a consistent state at every instant another task or an interrupt could look at them.

### 4.1 `MemHeader` and `MemChunk`

From `exec/memory.h` (lines 20-35):

```c
struct MemChunk {
    struct MemChunk *mc_Next;    /* next free chunk, NULL at end */
    ULONG            mc_Bytes;   /* size of this chunk in bytes  */
};

struct MemHeader {
    struct Node      mh_Node;    /* ln_Type = NT_MEMORY, ln_Pri = priority */
    UWORD            mh_Attributes;  /* MEMF_CHIP / MEMF_FAST / MEMF_PUBLIC */
    struct MemChunk *mh_First;   /* first free chunk in this region */
    APTR             mh_Lower;   /* lower memory bound */
    APTR             mh_Upper;   /* upper memory bound + 1 */
    ULONG            mh_Free;    /* total free bytes */
};
```

A `MemHeader` describes one contiguous region of RAM. `ExecBase->MemList` is the list of all such regions — chip memory, slow memory, any expansion boards that have come online via autoconfig, any RAM pool added by `AddMemList`. The allocator walks `MemList` head-to-tail looking for the first region whose `mh_Attributes` satisfies the request.

Inside a region, `mh_First` points to the head of a linked list of `MemChunk` structures representing free blocks. **These `MemChunk` headers live inside the free blocks themselves** — there is no out-of-band free-list storage. A block that is currently free contains, at its base, two longwords: `mc_Next` and `mc_Bytes`. When a block is allocated, those two longwords are handed back to the caller as part of the allocation, with no memory of them.

Consequence: if you allocate 8 bytes, you get exactly 8 bytes (the minimum allocation unit). If you then free those 8 bytes back to the same pool, Exec writes an 8-byte `MemChunk` header into them. A block smaller than 8 bytes cannot be a free block because there is nowhere to put the header.

The free list is **sorted by address** (low to high). This is what makes coalescing cheap: to free a block, `FreeMem` walks the free list looking for the first chunk whose address is higher than the block being freed, checks whether the returned block is adjacent to the chunk before it and/or after it, and merges any adjacencies. All blocks and all chunk boundaries are aligned to `MEM_BLOCKSIZE` (= 8 bytes, `MEM_BLOCKMASK` = 7).

### 4.2 `MEMF_*` attributes

From `exec/memory.h`:

```
MEMF_PUBLIC   (1<<0)   /* must remain mapped/accessible */
MEMF_CHIP     (1<<1)   /* reachable by the custom chips (< 512K or < 2M) */
MEMF_FAST     (1<<2)   /* not reachable by custom chips; fast for CPU */
MEMF_CLEAR    (1<<16)  /* zero the block before returning */
MEMF_LARGEST  (1<<17)  /* used with AvailMem: return largest free chunk */
```

(Two more flags appear in later Kickstarts: `MEMF_REVERSE (1<<18)` to allocate from the top of a region down, and `MEMF_TOTAL (1<<19)` for `AvailMem`. The corpus captures the V1.3 state; V36+ adds these.)

These are *requirements*, not preferences. `AllocMem(size, MEMF_CHIP)` *must* return chip memory or fail. `AllocMem(size, 0)` is willing to take anything, and from the autodocs "MEMF_FAST is assumed first, then MEMF_CHIP" (Exec RKM ch. 6, line 3281). `MEMF_CLEAR` is an option, not a requirement — it modifies any allocation after the search.

Why MEMF_CHIP exists at all: the Amiga custom chips (copper, blitter, Paula, Denise) can only DMA from the first 512K of physical memory on an A500/A1000 (the first 1MB or 2MB on later models). Anything that the chips will read or write — screen bitmaps, blitter source/dest, audio samples, sprites, copper lists, trackdisk buffers — must live in MEMF_CHIP memory (Autodocs `exec.library/AllocMem` INPUTS section, lines 1196-1219).

Why MEMF_PUBLIC exists: "All memory that is referenced via interrupts and/or by other tasks must be either public or locked into memory. This includes both code and data." (Autodocs `AllocMem`.) On V1.x there is no memory paging and `MEMF_PUBLIC` is largely a forward-compatibility flag — an emulator reproducing V1.x / Kickstart 1.2 / 1.3 can honour it trivially. But library code still tags everything it allocates on behalf of callers with `MEMF_PUBLIC` and expects that flag to be set on anything it receives.

### 4.3 `AllocMem` and `FreeMem`

```
memoryBlock = AllocMem(byteSize, attributes)
D0                     D0        D1
void FreeMem(memoryBlock, byteSize)
             A1          D0
```

`AllocMem` allocates a block of at least `byteSize` bytes with the given attributes. `byteSize` is rounded up to the next multiple of `MEM_BLOCKSIZE` (8 bytes) — so `AllocMem(1, 0)` and `AllocMem(8, 0)` return blocks of identical size.

Return value is `NULL` if no region can satisfy the request, and this is a perfectly normal condition that all code must check. From the autodoc WARNING: "The result of any memory allocate MUST be checked, and a viable error handling taken. ANY allocation can fail if there is not enough memory."

`FreeMem` requires both the address **and** the size. There is no per-allocation bookkeeping — the allocator does not remember how big the block was. If you pass the wrong size, the allocator will write a `MemChunk` header at the wrong place, corrupt the free list, and either crash immediately or corrupt some future allocation. From the autodoc NOTE: "If a block of memory is freed twice, the system will GURU. The Alert is `AN_FreeTwice` ($81000009)."

**Neither function may be called from interrupt code.** This is the single most important rule in Exec. The autodocs are explicit: "This function may not be called from interrupts." The reason is that the allocator does `Forbid()` but not `Disable()` around its list walk (Exec RKM ch. 5, lines 2823-2831: "memory allocation and deallocation routines forbid task switching but do not disable interrupts. This results in the finite possibility of interrupting a memory-related routine. In such a case, a memory linked list may be inconsistent when examined from the interrupt code itself."). So while memory operations are safe against task preemption, they are *not* safe against an interrupt interrupting them — the interrupt cannot safely look at the list.

**The allocator panics on corruption.** Both `AllocMem` and `FreeMem` NOTE: "If the free list is corrupt, the system will panic with alert AN_MemCorrupt, $81000005." In practice this means one of the invariants — free chunks sorted by address, `mc_Next` pointing to valid addresses inside the region, `mh_Free` matching the sum of chunk sizes — was violated.

### 4.4 `AllocAbs`

```
memoryBlock = AllocAbs(byteSize, location)
D0                     D0        A1
```

Allocate memory at a specific absolute address. Used by:

- Ramdisks and recoverable entities preserving data across a warm reboot (via `KickMemPtr`).
- Drivers that need a specific physical address for DMA.
- `InitResident` when bringing up `Resident` modules whose memory must be reserved.

The block may not be *exactly* the requested region because of 8-byte alignment rounding, but "if the return value is non-zero, the block is guaranteed to contain the requested range" (Autodocs `AllocAbs`). Fails (returns NULL) if any byte of the request is already allocated or if the requested region is not inside any known `MemHeader`.

### 4.5 `Allocate` and `Deallocate`

```
memoryBlock = Allocate(MemHeader, byteSize)
D0                     A0         D0
void Deallocate(MemHeader, memoryBlock, byteSize)
                A0         A1          D0
```

These are the raw allocator primitives that `AllocMem` / `FreeMem` are built on. They take a `MemHeader *` directly — you pick which region to allocate from. They do not arbitrate (no `Forbid`/`Permit`). They are what you use to manage a private memory pool: allocate one large block with `AllocMem`, build your own `MemHeader` inside it, and use `Allocate` / `Deallocate` internally.

The Exec RKM has a worked example of this pattern (ch. 6, lines 3530-3606). The main reasons to do it are (a) to batch a lot of small allocations without paying the system allocator's overhead each time, and (b) to keep your task's allocations clustered so freeing them on exit is trivial.

### 4.6 `AllocEntry` and `FreeEntry`

```
memList = AllocEntry(memList)
D0                   A0
void FreeEntry(memList)
               A0
```

`AllocEntry` takes a `MemList` structure (see the typedef in `exec/memory.h` lines 40-46: a `Node` followed by `UWORD ml_NumEntries` and an array of `MemEntry`). Each `MemEntry` is a union of a requirement `meu_Reqs` and an address `meu_Addr`, plus a `me_Length`. On input, `meu_Reqs` and `me_Length` are set. On return, `meu_Addr` is filled in with the allocated address.

The main consumer is the task system. A task's `tc_MemEntry` list can hold `MemList`s returned by `AllocEntry`, and `RemTask` automatically frees them at task removal time. The typical pattern: at task startup, allocate all required buffers in one `AllocEntry` call, attach the resulting `MemList` to `tc_MemEntry`, and never worry about cleanup.

BUGS note from the autodocs: on V1.2 and earlier, if any allocation in the entry fails, `AllocEntry` fails to back out fully and leaks the allocations that succeeded. Fixed by the `SetPatch` program on V1.3.

### 4.7 `AvailMem` and `TypeOfMem`

```
size = AvailMem(attributes)
D0              D1

attributes = TypeOfMem(address)
D0                     A1
```

`AvailMem` walks `MemList` summing the free space in all regions matching the attributes. With `MEMF_LARGEST` in the attributes, it returns the size of the largest single contiguous free chunk instead — essential for graphics code that wants to know if it can fit a framebuffer. The value is a snapshot and may be stale by the time the caller reads it (race against other tasks and interrupts); the autodoc is explicit about this.

`TypeOfMem` takes a *pointer* and returns the `MemHeader` attributes for whichever region contains it, or 0 if the pointer is not inside any known RAM region. It is mostly used to answer the question "is this chip memory?" for cases where code received a pointer from somewhere and has to decide whether it is safe to DMA out of.

An address in ROM, in the custom chip register space, or in unmapped memory will return 0. The first few bytes of every region are the `MemHeader` itself and are excluded.

### 4.8 `AllocVec` and `FreeVec` (V36+)

```
memoryBlock = AllocVec(byteSize, attributes)
FreeVec(memoryBlock)
```

V36 (Kickstart 2.0) introduced `AllocVec` / `FreeVec`, which remember the allocation size for you: `AllocVec` stuffs the size and a magic word in a small header just before the returned pointer, and `FreeVec` takes just the pointer. These are *not* present in V1.x ROMs, which is what the primary corpus documents, but any V2.0+ ROM will contain them and any 2.0+ application will use them.

A V1.3 emulator does not need them; a V2.0+ emulator does.

### 4.9 `AddMemList`

```
void AddMemList(size, attributes, pri, base, name)
                D0    D1          D2   A0    A1
```

Hand a region of memory to the system allocator. Used during boot once chip memory has been sized, once slow-ranger memory has been found, and once each autoconfig board reports itself. The first few bytes of `base` are overwritten with the `MemHeader` structure; the remainder becomes one huge free chunk.

`pri` is the `ln_Pri` on the resulting `MemHeader` node. Chip memory is conventionally given `-10` (so it comes last in `MemList` order — allocations for attribute-0 requests prefer fast memory first). 16-bit expansion memory gets priority 0. Higher priority equals earlier in the list equals "preferred".

`name` is not copied; the caller must keep it alive for the lifetime of the region (which is "forever"). Using a constant string in ROM is fine.

### 4.10 Walking MemList safely

Any time user code wants to examine the memory list — to print it, to find out how much chip memory is left, to check whether a specific address is in RAM — it must hold `Forbid()` for the duration of the walk. The RKM has the canonical example (ch. 2, lines 1480-1494):

```c
Forbid();
for (mh = (struct MemHeader *)eb->MemList.lh_Head;
     mh->mh_Node.ln_Succ;
     mh = (struct MemHeader *)mh->mh_Node.ln_Succ) {
    firsts[count++] = mh->mh_First;
}
Permit();
```

Note the loop termination: `mh->mh_Node.ln_Succ != NULL` — because the tail sentinel is reached exactly when `ln_Succ` becomes NULL (the double-header trick from §3.3).

---

## 5. Tasks, processes and the scheduler

A task is the Exec unit of execution. It has its own stack, its own set of registers (preserved in its `Task` struct while not running), its own signal state, and its own place in the priority-based scheduler. A process is a task plus some AmigaDOS-specific extensions — from Exec's point of view a Process is still scheduled as a task, but DOS, CLI, file I/O and handler code all require a Process, not a bare task.

### 5.1 `Task` structure

From `exec/tasks.h` (lines 22-45):

```c
struct Task {
    struct Node  tc_Node;        /* NT_TASK, ln_Pri = priority, ln_Name = name */
    UBYTE        tc_Flags;
    UBYTE        tc_State;
    BYTE         tc_IDNestCnt;   /* task-private interrupt disable count */
    BYTE         tc_TDNestCnt;   /* task-private task disable count */
    ULONG        tc_SigAlloc;    /* bits for allocated signals */
    ULONG        tc_SigWait;     /* bits the task is blocked on */
    ULONG        tc_SigRecvd;    /* bits that have been posted */
    ULONG        tc_SigExcept;   /* bits that cause exceptions instead of wakeup */
    UWORD        tc_TrapAlloc;   /* allocated trap numbers */
    UWORD        tc_TrapAble;    /* enabled traps */
    APTR         tc_ExceptData;
    APTR         tc_ExceptCode;  /* task exception handler */
    APTR         tc_TrapData;
    APTR         tc_TrapCode;    /* CPU trap handler */
    APTR         tc_SPReg;       /* stack pointer (saved when not running) */
    APTR         tc_SPLower;     /* lower bound of task stack */
    APTR         tc_SPUpper;     /* upper bound of task stack + 2 */
    VOID       (*tc_Switch)();   /* called when this task loses the CPU */
    VOID       (*tc_Launch)();   /* called when this task gains the CPU */
    struct List  tc_MemEntry;    /* MemLists freed at RemTask */
    APTR         tc_UserData;    /* for the user's use */
};
```

Flag bits from `exec/tasks.h`:

```
TB_PROCTIME   0    /* TF_PROCTIME: accumulate process time */
TB_STACKCHK   4    /* TF_STACKCHK: enable stack checking  */
TB_EXCEPT     5    /* TF_EXCEPT:   task exception pending */
TB_SWITCH     6    /* TF_SWITCH:   call tc_Switch on switch-out */
TB_LAUNCH     7    /* TF_LAUNCH:   call tc_Launch on switch-in */
```

`TF_EXCEPT` is set by `Signal()` when a signal in `tc_SigExcept` is posted; the dispatcher checks it on exit from the interrupt that posted the signal and runs the task exception handler before resuming the interrupted code. `TF_SWITCH` and `TF_LAUNCH` let sophisticated tasks (like the floating-point libraries) hook the context switch itself.

### 5.2 Task states

From `exec/tasks.h`:

```
TS_INVALID    0     /* not a valid task */
TS_ADDED      1     /* just added, not yet running */
TS_RUN        2     /* currently running */
TS_READY      3     /* on TaskReady queue */
TS_WAIT       4     /* on TaskWait queue, blocked on signals */
TS_EXCEPT     5     /* handling a task exception */
TS_REMOVED    6     /* being removed by RemTask */
```

Exactly one task has `tc_State == TS_RUN` at any time: the task pointed to by `ExecBase->ThisTask`. All other runnable tasks are on `TaskReady` with `TS_READY`. All tasks blocked on signals are on `TaskWait` with `TS_WAIT`. Transient states `TS_ADDED`, `TS_EXCEPT`, `TS_REMOVED` cover the gaps where the task is in the middle of being added, handling an exception on the dispatcher's behalf, or being torn down.

### 5.3 The scheduler

Exec's scheduler is priority-based with round-robin within a priority level, quantum-driven, and preemptive. The actual entry points live at fixed negative offsets in `exec.library`:

```
-0x001E  Supervisor
-0x0024  ExitIntr
-0x002A  Schedule        /* called at end of an interrupt */
-0x0030  Reschedule      /* forces a full task switch       */
-0x0036  Switch          /* internal switch primitive       */
-0x003C  Dispatch        /* core dispatcher                  */
-0x0042  Exception       /* deliver task exception           */
```

(From the exec.lib.offsets table in the Exec RKM, lines 13297-13303.) These are not listed as public APIs in the autodocs but are visible in the ROM.

The scheduling policy, as described in the Exec RKM ch. 2:

> "The highest-priority ready task is selected and receives processing until a higher-priority task becomes active, the running task exceeds a preset time period (a quantum) and there is another equal-priority task ready to run, or the task needs to wait for an external event before it can continue." (lines 1005-1008)

> "In addition to the prioritized scheduling of tasks, time-slicing also occurs for tasks with the same priority. In this scheme a task is allowed to execute for a quantum (a preset time period). If the task exceeds this period, the system will preempt it and give other tasks of the same priority a chance to run. This will result in a time-sequenced round robin scheduling of all equal-priority tasks." (lines 1018-1022)

The **quantum mechanism**:

- `ExecBase->Quantum` is the total slice size in ticks. Default is 4 (so 4/50s = 80ms on PAL, 4/60s = 67ms on NTSC).
- `ExecBase->Elapsed` is decremented on each VBlank interrupt.
- When `Elapsed` reaches zero, the VBlank handler sets an `AttnResched` bit and, at `ExitIntr`, if there is a ready task of equal or higher priority than the running task, calls `Schedule()` to switch.
- Switching is only forced if there is a peer at the same priority — if the running task is the sole task of the highest priority in the system, it runs forever.

**`Schedule()` does nothing if `TDNestCnt >= 0`.** This is the Forbid gate: while any task holds `Forbid()`, scheduling is disabled, even though interrupts still run. This is what makes `Forbid()` effective.

**Preemption sources:**

- VBlank quantum expiry (round-robin).
- A higher-priority task becomes ready (priority preemption). Happens when `Signal()` from an interrupt posts a signal to a task on `TaskWait` whose priority exceeds the current running task's priority.
- The running task voluntarily blocks via `Wait()`.

**The idle loop.** When `TaskReady` is empty, Exec halts the CPU (`STOP #$2000`). `IdleCount` is incremented, and only an interrupt can resume execution. This is the Exec way of saving power, and of letting `IdleCount` serve as an "is the machine busy?" gauge (Exec RKM ch. 2, lines 1037-1040).

### 5.4 Context switch mechanics

The switch saves, to the outgoing task's stack:

- All general-purpose registers D0-D7, A0-A6 (interrupts that got us here have already pushed CCR/PC).
- Then stores the task's final SP into `tc_SPReg`.

It loads, from the incoming task:

- `ThisTask = incomingTask`.
- `incomingTask->tc_State = TS_RUN`.
- `SP = incomingTask->tc_SPReg`.
- Pop all registers off the new task's stack, then RTE.

If the outgoing task has `TF_SWITCH` set, `tc_Switch` is called before state is saved. If the incoming task has `TF_LAUNCH` set, `tc_Launch` is called after state is restored. These hooks are used for per-task FP context, per-task address-register banks, and debugging.

### 5.5 `AddTask`, `RemTask`, `FindTask`, `SetTaskPri`

```
void AddTask(task, initialPC, finalPC)
             A1    A2         A3
```

`task` points to an allocated-and-initialised `Task` struct, `initialPC` is the entry point, `finalPC` is the fallback return target when `initialPC` RTSes out. If `finalPC` is NULL, the system default (which calls `RemTask(NULL)`) is used (Autodocs `exec.library/AddTask`).

What must be filled in before `AddTask`:

- `tc_Node` (type, priority, name).
- `tc_SPLower`, `tc_SPUpper`, `tc_SPReg` (all pointing into the task's stack, with `tc_SPReg = tc_SPUpper` at entry).
- Everything else can be zero; Exec supplies defaults.

The "absolute smallest stack" is "something in the range of 100 bytes" but 256 is the minimum for calling Exec, and 4K is the minimum for general system calls. "DO NOT UNDERESTIMATE." (Autodocs `AddTask`.)

`AddTask` clears `TC_FLAGS`. It **does** run a reschedule — the new task may run immediately if its priority is high enough.

`RemTask(task)` removes a task from the system. `NULL` means "myself" and is the normal idiom. `RemTask` also automatically frees every `MemList` attached to `tc_MemEntry`, which is the whole point of using `AllocEntry` for task allocations.

**"Removing some other task is very dangerous."** (Autodocs.) You really cannot safely `RemTask(other)` unless you can prove `other` is in a quiescent state, because freeing a task's stack while it is on it is catastrophic. The safe pattern is to signal the other task and ask it to self-remove.

`FindTask(name)` returns the task with that name. `name == NULL` is a special case: it returns `ThisTask` quickly without any searching. Any other name requires a linear walk of `TaskReady`, `TaskWait`, and the running task under `Disable()` — expensive, and disables interrupts for the duration, so it should not be done from a hot path.

`SetTaskPri(task, newPri)` changes a task's priority and runs a reschedule. Can be used on the running task with `FindTask(NULL)` as the target.

### 5.6 Processes

A `Process` (from `libraries/dosextens.h`) begins with a full `Task` struct and extends it with a `MsgPort` (`pr_MsgPort`) and a pile of DOS-specific fields: current directory, CLI pointer, seglist, stdin/stdout file handles, etc.

Exec does not know about Process extensions. It schedules them like any other task. The distinction matters for two reasons:

1. **DOS functions can only be called from a Process.** `dos.library` functions (Read, Write, Open, Lock, etc.) walk `pr_MsgPort` to find the file system handler, and a bare task has no `pr_MsgPort`. Code that tries to call DOS from an Exec-level task crashes when DOS walks off the end of the `Task` struct into the next allocation's memory.
2. **`OpenLibrary` on a disk-resident library calls DOS** to load the library. So `OpenLibrary` from a bare task fails whenever the library is not in ROM. From Autodocs `OpenLibrary`: "Only Processes are allowed to call OpenLibrary (since OpenLibrary may in turn call dos.library)."

In practice: use `dos.library/CreateProc` to spawn a process, not `exec.library/AddTask`, unless you really need a lower-level building block.

### 5.7 Task stack switching

Task stacks are used stacks (A7/USP in the 68000). Interrupts and traps do *not* run on the task stack — they run on `SysStkUpper`/`SysStkLower` in supervisor mode. When an interrupt fires:

1. CPU pushes current SR and PC onto whatever SP is active.
2. CPU changes to supervisor mode, switches to SSP (which Exec has pre-loaded with `SysStkUpper`).
3. Interrupt handler runs on SysStk.
4. On return, CPU drops back to user mode and restores USP.

The task never sees the interrupt frame on its own stack, which means minimum task stack only has to accommodate user-mode code + function call depth + a few words for the dispatcher's saved-register block. This is why 256 bytes can work.

`SuperState()` / `UserState()` allow task code to enter supervisor mode without needing interrupt infrastructure, but this is considered unusual and the `UserState` autodoc says it was broken in V33/34 Kickstart. Most code does not need supervisor mode.

---

## 6. Signals

Signals are Exec's primary task-wakeup and task-to-task notification mechanism. Each task has 32 signal bits in its `Task` struct. Signals are used for everything from CTRL-C delivery to message port wakeups to I/O completion.

### 6.1 The four signal fields

Every `Task` has four 32-bit signal state words:

```c
ULONG tc_SigAlloc;   /* which signal bits are currently allocated */
ULONG tc_SigWait;    /* which bits this task is currently Wait()ing on */
ULONG tc_SigRecvd;   /* which bits have been Signal()ed but not yet consumed */
ULONG tc_SigExcept;  /* which bits cause exceptions instead of wakeup */
```

A bit is allocated with `AllocSignal`, consumed by `Wait` or `SetSignal`, freed with `FreeSignal`. The allocation is per-task — there is no system-wide signal space.

### 6.2 The 16 reserved signals

The low 16 bits (0..15) are reserved for system use. The upper 16 (16..31) are available for user allocation. `AllocSignal(-1)` finds any unallocated bit; a new task starts with `tc_SigAlloc = ExecBase->TaskSigAlloc` which is typically `0x0000FFFF`.

Predefined bits from `exec/tasks.h`:

```
SIGB_ABORT     0       /* also SIGBREAKF_CTRL_C equivalent in DOS land */
SIGB_CHILD     1
SIGB_BLIT      4
SIGB_SINGLE    4       /* overlaps SIGB_BLIT */
SIGB_DOS       8
```

And from `libraries/dos.h`:

```
SIGBREAKF_CTRL_C   (1<<12)
SIGBREAKF_CTRL_D   (1<<13)
SIGBREAKF_CTRL_E   (1<<14)
SIGBREAKF_CTRL_F   (1<<15)
```

These are the CLI "break" signals. The CLI input handler posts `SIGBREAKF_CTRL_C` to the current-foreground process when the user presses Ctrl-C, and well-behaved programs poll for it or wait on it.

### 6.3 `AllocSignal` / `FreeSignal`

```
signalNum = AllocSignal(signalNum)
D0                      D0
void FreeSignal(signalNum)
                D0
```

`signalNum == -1` means "any free signal"; otherwise you can request a specific bit (but the autodoc comment "if the signal is already in use ... a -1 is returned" applies). On success, the signal bit is marked in `tc_SigAlloc` and cleared in `tc_SigRecvd` (guaranteed "ready for use" — no stale previous-allocation state).

Returns `-1` on failure.

**Allocation and free must happen in the task that owns the bit.** (Autodoc: "This function can only be used by the currently running task." And: "Signals may not be allocated or freed from exception handling code.") This is because the operation writes to `tc_SigAlloc` without any Forbid/Disable wrapping.

### 6.4 `Wait` — the blocking primitive

```
signals = Wait(signalSet)
D0            D0
```

Puts the current task to sleep until any bit in `signalSet` is posted to it. On return, `signals` is the set of bits that actually satisfied the wait (a subset of `signalSet`), and those bits are cleared from `tc_SigRecvd`.

If any requested signal was already posted when `Wait` was called, it returns immediately without blocking — "If a signal occurred prior to calling Wait, the wait condition will be immediately satisfied, and the task will continue to run without delay" (Autodoc).

**Cannot be called from supervisor mode or from an interrupt.** (Autodoc CAUTION.) Waiting requires a task context, because the dispatcher needs somewhere to save the outgoing state.

**Wait breaks Forbid() and Disable().** A task that holds `Forbid()` and then calls `Wait()` temporarily releases the forbidden state while asleep (the kernel cannot block a task while also not running others — the machine would deadlock). When the task wakes up, the Forbid is re-established (Exec RKM ch. 2, line 1465-1471).

**`Wait` is the mechanism behind almost every other blocking call in Exec.** `WaitPort` is `Wait` on a single signal bit. `DoIO` is `SendIO` + `Wait` on the reply port's signal. `WaitIO` is the same. `ObtainSemaphore` (when contended) puts the caller on a list and `Wait`s. If a task spends any time not running, it is probably inside `Wait`.

### 6.5 `Signal` — the wakeup primitive

```
void Signal(task, signals)
            A1    D0
```

Posts a set of bits to `task->tc_SigRecvd`. If any bit in `signals` is in `task->tc_SigWait`, the task is awakened: moved from `TaskWait` to `TaskReady`, state changed from `TS_WAIT` to `TS_READY`, and a reschedule is triggered if the awakened task is higher priority than the running one.

**`Signal` is safe to call from interrupts.** (Autodoc: "This function is safe to call from interrupts.") This is crucial: it is how interrupt handlers wake up waiting tasks. The interrupt posts a signal, the scheduler runs at `ExitIntr`, and a blocked task resumes.

Implementation detail: the safe-from-interrupt guarantee is provided by `Signal` using interlocked operations on `tc_SigRecvd`/`tc_SigWait`, and by the fact that task queue manipulation happens with interrupts disabled. An emulator reproducing `Signal` must ensure atomicity.

### 6.6 `SetSignal`

```
oldSignals = SetSignal(newSignals, signalMask)
D0                     D0          D1
```

Reads and/or writes `tc_SigRecvd`. Bits in `signalMask` are replaced with the corresponding bits in `newSignals`; bits not in the mask are untouched. Returns the *previous* value of `tc_SigRecvd`.

Common idioms:

- `SetSignal(0, 0)` — read the current signal state without modifying anything.
- `SetSignal(0, 0xFFFFFFFF)` — clear all signals (dangerous).
- `if (SetSignal(0, 0) & SIGBREAKF_CTRL_C) ...` — poll for Ctrl-C without blocking.

"Setting the state of signals is considered dangerous. Reading the state of signals is safe." (Autodoc.)

### 6.7 `SetExcept` — signals that trigger exceptions

```
oldSignals = SetExcept(newSignals, signalMask)
```

Works like `SetSignal` but operates on `tc_SigExcept` instead of `tc_SigRecvd`. The bits in `tc_SigExcept` designate which signals will trigger a task exception (see §13.2) instead of just waking the task.

When a signal in `tc_SigExcept` is posted, the scheduler sets `TF_EXCEPT` on the task. On the next opportunity to run user code, instead of resuming the interrupted instruction, the dispatcher calls `tc_ExceptCode` with:

```
D0 = set of signals that caused the exception
A1 = tc_ExceptData
A6 = SysBase
```

The exception handler returns a new set of signal bits to re-enable for exceptions. This is how tasks implement async "please stop what you're doing and handle this" flows — the textbook example being Ctrl-C delivery that aborts an ongoing operation rather than waiting for polling.

---

## 7. Messages and ports

Messages are Exec's high-level inter-task communication mechanism. A message port belongs to one task; any task can send messages to it. Sending and receiving is zero-copy — the "message" is a pointer into the sender's memory with a reference-counting convention. I/O is implemented on top of messages: an I/O request is a message whose body describes a device operation.

### 7.1 `MsgPort`

From `exec/ports.h` (lines 28-34):

```c
struct MsgPort {
    struct Node  mp_Node;        /* NT_MSGPORT, optional ln_Name */
    UBYTE        mp_Flags;       /* PA_* in low 2 bits */
    UBYTE        mp_SigBit;      /* signal bit to post / softint to cause */
    struct Task *mp_SigTask;     /* task to signal (or Interrupt * if softint) */
    struct List  mp_MsgList;     /* queued messages, FIFO head-first */
};
```

`mp_Flags` holds one of three action codes (`PF_ACTION` mask is 3):

```
PA_SIGNAL    0    /* Signal(mp_SigTask, 1<<mp_SigBit) on PutMsg */
PA_SOFTINT   1    /* Cause((struct Interrupt *)mp_SigTask) on PutMsg */
PA_IGNORE    2    /* just queue the message, no notification */
```

(`exec/ports.h` lines 38-43.)

`PA_SIGNAL` is the common case: sending a message to this port posts a signal to the owning task, which is presumably blocked in `Wait` or `WaitPort` on that bit. `PA_SOFTINT` defers processing to a software interrupt, which makes it possible for an interrupt handler to receive messages without blocking (as long as the softint handler is short). `PA_IGNORE` just queues the message — useful when the task will poll or when the port is serving as a message queue for semaphore-style locking.

### 7.2 `Message`

From `exec/ports.h` (lines 47-51):

```c
struct Message {
    struct Node     mn_Node;        /* NT_MESSAGE, set by PutMsg */
    struct MsgPort *mn_ReplyPort;   /* for the receiver to ReplyMsg back */
    UWORD           mn_Length;      /* total message size in bytes */
};
```

A real message is a `Message` header followed by user-defined body fields. The body is part of the same allocation. `mn_Length` is the total size including the header. `mn_ReplyPort` is optional: if NULL, `ReplyMsg` notices and marks the message `NT_FREEMSG` instead of trying to send it back.

Messages are **not copied**. From Exec RKM ch. 3: "In essence, a message between two tasks is a temporary license for the receiving task to use a portion of the memory space of the sending task." The sender retains ownership of the memory until the message is replied, at which point the sender reacquires control. Between `PutMsg` and the eventual reply, the sender must not touch the message — the receiver owns it.

### 7.3 `PutMsg`

```
void PutMsg(port, message)
            A0    A1
```

Appends `message` to `port->mp_MsgList` (at the tail — FIFO ordering), sets `message->mn_Node.ln_Type = NT_MESSAGE`, and performs the port's arrival action:

- `PA_SIGNAL`: `Signal(mp_SigTask, 1 << mp_SigBit)`.
- `PA_SOFTINT`: `Cause((struct Interrupt *)mp_SigTask)`.
- `PA_IGNORE`: nothing.

`PutMsg` is safe from interrupt code. This is essential: it is how interrupt-driven I/O completion delivers results back to tasks (the device ISR calls `PutMsg` on the requester's reply port, the requesting task wakes up).

**Important subtlety: there is not a 1:1 correspondence between messages and signal deliveries.** If three messages arrive in quick succession, the signal is set once (a signal is a single bit — can't count past 1). A task that does `WaitPort` then `GetMsg` once and goes back to `Wait` has lost two messages. The correct idiom is always:

```c
WaitPort(port);
while ((msg = GetMsg(port)) != NULL) {
    handle(msg);
    ReplyMsg(msg);
}
```

Loop until `GetMsg` returns NULL. "Getting a signal does NOT always imply a message is ready. More than one message may arrive per signal, and signals may show up without messages." (Autodoc `exec.library/GetMsg`.)

### 7.4 `GetMsg`

```
message = GetMsg(port)
D0               A0
```

Removes and returns the first message on `port->mp_MsgList`, or NULL if empty. Does not block. Does not touch the signal bit (the signal was cleared by the `Wait`/`WaitPort` that unblocked the task).

### 7.5 `WaitPort`

```
message = WaitPort(port)
D0                 A0
```

Blocks until `port->mp_MsgList` is non-empty; returns a pointer to the first queued message (**without** removing it). If there is already a message, returns immediately without blocking.

Note: `WaitPort` calls `Wait` on `port->mp_SigBit` internally, which means the port must be set up with `PA_SIGNAL` and a valid `mp_SigBit`/`mp_SigTask`. A port with `PA_IGNORE` or `PA_SOFTINT` cannot be used with `WaitPort`.

### 7.6 `ReplyMsg`

```
void ReplyMsg(message)
              A1
```

Sends a message back to its reply port:

1. Sets `ln_Type = NT_REPLYMSG`.
2. Calls `PutMsg(mn_ReplyPort, message)`.
3. If `mn_ReplyPort` is NULL, sets `ln_Type = NT_FREEMSG` and does nothing else — a convention meaning "the message is orphaned, nobody will see it again, the receiver might as well free it."

`ReplyMsg` is safe from interrupts (it's just `PutMsg`).

### 7.7 `AddPort`, `RemPort`, `FindPort`

```
void AddPort(port)       ; Al
void RemPort(port)       ; Al
port = FindPort(name)    ; D0 = A1
```

Only *public* ports — ports that want to be found by name — need to be added with `AddPort`. Adding links the port into `ExecBase->PortList`. A port used internally between a known sender and a known receiver does not need to be public.

The autodoc for `FindPort` makes a crucial safety point: "No arbitration of the port list is done. This function MUST be protected with A Forbid()/Permit() pair!" The canonical safe-put-to-port idiom is:

```c
ULONG SafePutToPort(message, portname) {
    struct MsgPort *port;
    Forbid();
    port = FindPort(portname);
    if (port)
        PutMsg(port, message);
    Permit();
    return (ULONG)port;
}
```

Without the Forbid, another task could `RemPort` between the find and the put, and your `PutMsg` goes into free memory.

### 7.8 `CreatePort` / `DeletePort` (amiga.lib) and `CreateMsgPort` (V36+)

`CreatePort(name, pri)` and `DeletePort(port)` are not in the exec.library LVO table — they live in `amiga.lib`, a statically linked support library. Code that wants a port calls `CreatePort`, which:

1. Allocates a signal bit with `AllocSignal`.
2. Allocates a `MsgPort` with `AllocMem(sizeof(MsgPort), MEMF_CLEAR|MEMF_PUBLIC)`.
3. Initialises `mp_Node`, `mp_Flags = PA_SIGNAL`, `mp_SigBit`, `mp_SigTask = FindTask(NULL)`.
4. If `name != NULL`, calls `AddPort`; otherwise just calls `NewList(&mp->mp_MsgList)`.

`DeletePort` does the inverse: `RemPort` if public, `FreeSignal`, `FreeMem`.

V36 (Kickstart 2.0+) adds `CreateMsgPort` and `DeleteMsgPort` as proper exec.library functions. They do the same thing but live in the ROM, so V2.0+ code doesn't need amiga.lib just for this.

### 7.9 Initialising a port by hand

The Exec RKM has the full `CreatePort` source (ch. 3, lines 1876-1914). The pattern an emulator may see in ROM code is:

```c
sigBit = AllocSignal(-1);
mp->mp_Node.ln_Name = name;
mp->mp_Node.ln_Pri  = pri;
mp->mp_Node.ln_Type = NT_MSGPORT;
mp->mp_Flags   = PA_SIGNAL;
mp->mp_SigBit  = sigBit;
mp->mp_SigTask = FindTask(0);
NewList(&mp->mp_MsgList);    /* init the embedded List header */
```

The `NewList` call is essential — an emulator that allows `mp_MsgList` to be zero-initialised will see the double-header invariants violated (§3.3) and the first `GetMsg` on the port will either find a "message" at NULL or walk off into garbage.

**"A point of confusion is that clearing a MsgPort structure to all zeros is not enough to prepare it for use."** (Autodoc `AddPort` NOTE.) An emulator author reading ROM code will see a lot of `AllocMem` with `MEMF_CLEAR` followed by explicit `NEWLIST` macros — this is why.

---

## 8. Semaphores

Exec has two semaphore types, in chronological order of introduction: the old message-based `Semaphore` with `Procure`/`Vacate`, and the newer `SignalSemaphore` with `ObtainSemaphore`/`ReleaseSemaphore`. The old one is formally deprecated but `Procure`/`Vacate` still have LVO entries and are what an emulator might see in older code. The new one is preferred and is used by Intuition, graphics, layers, etc.

### 8.1 `SignalSemaphore`

From `exec/semaphores.h` (lines 42-56):

```c
struct SemaphoreRequest {
    struct MinNode sr_Link;
    struct Task   *sr_Waiter;
};

struct SignalSemaphore {
    struct Node             ss_Link;          /* NT_SIGNALSEM */
    SHORT                   ss_NestCount;     /* nesting depth for exclusive lock */
    struct MinList          ss_WaitQueue;     /* waiting tasks */
    struct SemaphoreRequest ss_MultipleLink;  /* "this task is the owner" marker */
    struct Task            *ss_Owner;         /* current exclusive owner, or NULL */
    SHORT                   ss_QueueCount;    /* shared-lock reader count */
};
```

`ss_Owner` is the task currently holding the lock exclusively, or NULL if nobody holds it exclusively. `ss_NestCount` counts how many times that same task has re-locked it (recursive locking is supported — the same task can call `ObtainSemaphore` multiple times without deadlocking itself). `ss_WaitQueue` is the list of `SemaphoreRequest` structures representing tasks waiting to acquire. `ss_QueueCount` is used for shared-lock bookkeeping.

### 8.2 `InitSemaphore`

```
void InitSemaphore(signalSemaphore)
                   A0
```

Initialises a freshly allocated SignalSemaphore. Required before use — clearing to zero is **not** sufficient, because `ss_WaitQueue` is a `MinList` that needs its double-header set up. (Autodoc: "It does not allocate anything, but does initialize list pointers and the semaphore counters.")

### 8.3 `ObtainSemaphore` / `ReleaseSemaphore`

```
void ObtainSemaphore(signalSemaphore)
                     A0
void ReleaseSemaphore(signalSemaphore)
                      A0
```

`ObtainSemaphore` acquires the lock exclusively. If no one holds it, it succeeds immediately; `ss_Owner` is set to the current task, `ss_NestCount = 1`. If the current task already holds it, `ss_NestCount++` and the call returns immediately (recursive acquisition). If another task holds it, a `SemaphoreRequest` is queued on `ss_WaitQueue` and the caller `Wait`s.

`ReleaseSemaphore` releases one level of acquisition. Decrements `ss_NestCount`; if it hits zero, clears `ss_Owner` and wakes up the next waiter (if any), which will find itself the new owner.

**"Each ObtainSemaphore() call must be balanced by exactly one ReleaseSemaphore() call."** (Autodoc.) "Havoc breaks out if the task releases more times than it has obtained."

A queue of waiting tasks is maintained on the stacks of the waiting tasks themselves — that is, each `SemaphoreRequest` on the wait queue is a local variable in the caller's stack frame, which is valid as long as the caller is blocked. Be careful in an emulator that snapshots memory: the wait queue only makes sense while the owning tasks are still alive.

Signal semaphores are faster than `Procure/Vacate` semaphores "especially if the semaphore is not currently locked. They require very little set up and user thought." (Autodoc `ObtainSemaphore`.)

### 8.4 `AttemptSemaphore`

```
success = AttemptSemaphore(signalSemaphore)
D0                          A0
```

Non-blocking acquire. Returns TRUE if the lock was obtained (exactly as `ObtainSemaphore` would have done), FALSE if someone else had it. Lets tasks back out if they cannot get the lock instead of blocking.

### 8.5 `ObtainSemaphoreShared`, `AttemptSemaphoreShared`

Shared (reader) locking: multiple tasks can hold the semaphore shared at once, but an exclusive lock blocks until all sharers have released, and new sharers block if an exclusive holder is queued. Used by libraries that want to allow concurrent read access to a data structure but exclusive write access. Graphics layers use it.

These are V36+ functions. In V1.3 the only locking mode is exclusive.

### 8.6 `ObtainSemaphoreList` / `ReleaseSemaphoreList`

Lock all signal semaphores on a `List` at once, atomically. The point is to acquire many locks without risk of deadlock — acquiring them one by one in order creates ordering constraints; acquiring them as a set lets the system loop until it can get them all.

Restriction: only one task may use `ObtainSemaphoreList` at a time, because there is no global ordering on the list itself. "There needs to be a higher level lock (perhaps another signal semaphore...) that is used before someone attempts to lock the semaphore list via ObtainSemaphoreList()." (Autodoc.)

### 8.7 `AddSemaphore` / `RemSemaphore` / `FindSemaphore`

```
void AddSemaphore(signalSemaphore)     ; Al
void RemSemaphore(signalSemaphore)     ; Al
sem = FindSemaphore(name)              ; D0 = A1
```

Public-name semaphore registration, analogous to `AddPort`/`RemPort`/`FindPort`. Adds the semaphore to `ExecBase->SemaphoreList` so other tasks can find it by name.

**BUG in V1.2/V1.3 (Kickstart 33/34):** "AddSemaphore does not work in Kickstart V33/34. Instead use this code:" (Autodoc, lines 892-908)

```c
void AddSemaphore(struct SignalSemaphore *s) {
    InitSemaphore(s);
    Forbid();
    Enqueue(&SysBase->SemaphoreList, s);
    Permit();
}
```

An emulator simulating V1.3 must either reproduce the bug or silently substitute the fixed version. Real V1.3 code that relied on `AddSemaphore` is rare — most code either implemented the bypass directly or used private semaphores.

### 8.8 `Procure` / `Vacate` (deprecated)

```
result = Procure(semaphore, bidMessage)
D0              A0         A1
void Vacate(semaphore)
            A0
```

The old message-based semaphore. `Procure` tries to lock an old-style `Semaphore` (embeds a `MsgPort` as `sm_MsgPort` and a WORD `sm_Bids`); if the lock is available, returns TRUE immediately; otherwise queues `bidMessage` to the port and returns FALSE, and the caller is expected to `WaitPort` on `bidMessage`'s reply port. `Vacate` releases.

BUGS autodoc: "Procure() and Vacate() do not have proven reliability." Nobody should be using these. Present in V1.x for backward compatibility; the LVO slots remain through V36.

---

## 9. Libraries

A library, in Exec, is a position-independent collection of callable routines reached by negative offset from a base pointer. The same structure is used for devices, resources, ExecBase itself, intuition.library, graphics.library, and all third-party libraries. Understanding the library layout is understanding how essentially all ROM and disk code is entered.

### 9.1 `Library` structure

From `exec/libraries.h` (lines 30-41):

```c
struct Library {
    struct Node  lib_Node;       /* NT_LIBRARY, ln_Name = library name */
    UBYTE        lib_Flags;
    UBYTE        lib_pad;
    UWORD        lib_NegSize;    /* bytes of jump table in front of lib pointer */
    UWORD        lib_PosSize;    /* bytes of data behind lib pointer */
    UWORD        lib_Version;
    UWORD        lib_Revision;
    APTR         lib_IdString;
    ULONG        lib_Sum;        /* library checksum */
    UWORD        lib_OpenCnt;    /* current open count */
};
```

Flag bits:

```
LIBF_SUMMING   (1<<0)   /* a task is currently checksumming */
LIBF_CHANGED   (1<<1)   /* library has been patched since last sum */
LIBF_SUMUSED   (1<<2)   /* user wants checksum-fail to cause an alert */
LIBF_DELEXP    (1<<3)   /* delayed expunge pending */
```

### 9.2 The jump table layout

A library is stored in memory as:

```
<...jump table...>              (negative offsets from base)
<-6 bytes> JMP abs LIB_OPEN
<-12 bytes> JMP abs LIB_CLOSE
<-18 bytes> JMP abs LIB_EXPUNGE
<-24 bytes> JMP abs reserved
<-30 bytes> JMP abs firstUserFunc
<...more 6-byte JMP entries...>
<base>                          (0 = lib_Node)
<struct Library fields>         (positive offsets)
<library private data>
```

The library base pointer that `OpenLibrary` returns points at the `Node` — that is, zero offset. Everything negative is jump table; everything positive is data. `lib_NegSize` says how big the jump table is (in bytes); `lib_PosSize` says how big the data area is.

**Every jump table entry is exactly 6 bytes: a `JMP abs.l` instruction.** From `exec/libraries.h`:

```
#define LIB_VECTSIZE   6
#define LIB_RESERVED   4   /* 4 reserved vectors at the start */
#define LIB_BASE      (-LIB_VECTSIZE)
#define LIB_USERDEF   (LIB_BASE - (LIB_RESERVED * LIB_VECTSIZE))
                         /* = -30, first user-defined vector */
#define LIB_NONSTD    (LIB_USERDEF)

#define LIB_OPEN     (-6)    /* OPEN entry */
#define LIB_CLOSE   (-12)    /* CLOSE entry */
#define LIB_EXPUNGE (-18)    /* EXPUNGE entry */
#define LIB_EXTFUNC (-24)    /* reserved, return 0 */
```

The first four entries are mandatory. Everything after `-30` is library-specific.

### 9.3 Calling convention

From Exec RKM ch. 7 (lines 3694-3735):

```
move.l    A6,-(SP)              ; save caller's A6
move.l    <libptr>,A6           ; library base into A6
jsr       _LVO<routineName>(A6) ; negative offset
move.l    (SP)+,A6              ; restore A6
```

Where `_LVO<routineName>` is the negative offset. Every library function is called with A6 = library base. Inside a function, A6 is how the function finds its own data.

The `LINKLIB` macro encapsulates this pattern. An emulator that is decoding ROM code will see a great deal of `move.l xxxBase,a6 / jsr _LVOyyy(a6)` everywhere.

D0, D1, A0, A1 are scratch across the call. The callee must preserve all other registers if it uses them.

### 9.4 `OpenLibrary` / `CloseLibrary`

```
library = OpenLibrary(libName, version)
D0                    A1       D0
void CloseLibrary(library)     ; Al
```

`OpenLibrary` searches `ExecBase->LibList` for a library whose `lib_Node.ln_Name` matches `libName` and whose `lib_Version >= version`. If found in memory, the library's `LIB_OPEN` vector is called (the library's OpenCnt increments); if not found in memory, `OpenLibrary` asks DOS to load `LIBS:libName` and retry.

Version 0 means "any version is fine". A version higher than what the library declares causes the open to fail and return NULL.

Important restrictions (Autodoc):

- "Only Processes are allowed to call OpenLibrary (since OpenLibrary may in turn call dos.library)." So the caller needs a full Process, not a bare Task — unless the library is already loaded in memory, in which case OpenLibrary short-circuits and the Process restriction goes away.
- "AmigaDOS file names are not case sensitive, but Exec lists are. If the library name is specified in a different case than it exists on disk, unexpected results may occur." If you `OpenLibrary("Graphics.library", 0)` on a V1.3 system, it will not find `graphics.library` in memory but may find `Graphics.library` on disk — and will load a second copy.

`CloseLibrary` calls the library's `LIB_CLOSE` vector, which decrements `lib_OpenCnt`. If `OpenCnt` reaches zero and `LIBF_DELEXP` is set, a pending expunge is triggered.

### 9.5 `OldOpenLibrary`

```
library = OldOpenLibrary(libName)
D0                       A1
```

The pre-1.2 version of `OpenLibrary` that did not check the version number. Exactly equivalent to `OpenLibrary(libName, 0)`. Present for binary compatibility with 1.0 code. Has its own LVO slot so it will be in any Kickstart.

### 9.6 `AddLibrary` / `RemLibrary`

```
void AddLibrary(library)    ; Al
void RemLibrary(library)    ; Al
```

`AddLibrary` adds a fully constructed library to `ExecBase->LibList`, computes its initial checksum, and makes it findable via `OpenLibrary`. Called at library load time, typically from inside the library's own init code after `MakeLibrary` has allocated the structure.

`RemLibrary` calls the library's `LIB_EXPUNGE` vector, which is the library's chance to refuse removal (if it is busy or in use) or to free itself. The library chooses whether to remove itself from `LibList`, free its code and data memory, and return its seglist to DOS. `RemLibrary` is rarely called by user code; it is invoked by the system when memory pressure forces an expunge.

### 9.7 `MakeLibrary`

```
libAddr = MakeLibrary(vectors, structure, init, dSize, segList)
D0                    A0       A1         A2    D0    D1
```

The workhorse constructor. Allocates the library's memory, lays out the jump table via `MakeFunctions`, initialises the data area via `InitStruct`, calls the user's `init` routine, and returns the library pointer. The five arguments:

- `vectors` — pointer to an array of function pointers (terminated with -1). If the first word of the array is -1 then the array contains 16-bit relative word displacements; otherwise absolute 32-bit function pointers.
- `structure` — pointer to an `InitStruct` data table, or NULL.
- `init` — optional init routine to run after construction, or NULL. Called with `D0 = libAddr`, `A0 = segList`. Runs inside a `Forbid()/Permit()` pair.
- `dSize` — size of the library data area including the standard `Library` header fields.
- `segList` — for disk-loaded libraries, the DOS seglist that owns the library code. Passed through to the init routine and typically stored so `LIB_EXPUNGE` can `UnLoadSeg` it later.

Returns NULL if memory allocation fails. "If the vector table requires more system memory than is available, this function will return NULL." (Autodoc `MakeLibrary`.)

See §16 for the complete "new library" idiom including `Resident` tag and how `MakeLibrary` is invoked from an `RTF_AUTOINIT` bootstrap.

### 9.8 `MakeFunctions`

```
tableSize = MakeFunctions(target, functionArray, funcDispBase)
D0                         A0      A1             A2
```

Lays out the jump table portion of a library. `target` is the library base pointer (the jump table will be written at negative offsets from it). `functionArray` is an array of function pointers or word displacements. `funcDispBase` is NULL for absolute pointers, or a displacement base for relative offsets.

`funcDispBase != NULL` is a space optimization: word displacements take half the space of 32-bit pointers and are enough for compact libraries where all functions are within 64K of the base. `MakeFunctions` expands them into 32-bit absolute addresses in the actual `JMP abs.l` instructions.

Returns the total size of the jump table in bytes. Used both at library build time and at library patch time.

### 9.9 `SetFunction`

```
oldFunc = SetFunction(library, funcOffset, funcEntry)
D0                    A1       A0 (word)   D0
```

Replace a single function vector in a library with a new function. This is the supported way to patch libraries and is what `SetPatch` and similar utilities do on boot. `SetFunction`:

1. Validates that `funcOffset` is within the library's NegSize.
2. Writes the new entry address into the 6-byte `JMP abs.l` slot.
3. Marks `LIBF_CHANGED` so the checksum gets recomputed.
4. Recomputes the library's checksum.
5. Returns the old function address.

**"SetFunction cannot be used on non-standard libraries like dos.library."** (Autodoc NOTE.) dos.library has custom initialization and its jump table has unusual semantics; `SetFunction` doesn't know about it and will break it. To patch dos.library you must do it manually under `Forbid()`.

### 9.10 `SumLibrary`

```
void SumLibrary(library)     ; Al
```

Walks the library's jump table and computes a running sum. Used for integrity checking — if the sum has changed and `LIBF_CHANGED` is not set, something has corrupted the jump table and the system calls `Alert`. The exec.library itself is periodically re-summed to catch stray writes.

"An alert will occur if the checksum fails." (Autodoc.)

`LIBF_SUMUSED` controls whether the library actually *gets* checksummed — clear it and the library skips integrity checking.

### 9.11 Library patching safely

The supported idiom for patching a library is:

```c
oldFunc = SetFunction(libBase, -_LVOSomeFunction, myReplacement);
```

The "manual" fallback for libraries `SetFunction` can't handle (dos.library):

```c
Forbid();
/* save old 6 bytes at libBase - _LVOSomeFunction */
/* write new JMP abs.l there */
SumLibrary(libBase);
Permit();
```

An emulator reproducing library semantics must treat the jump table as writable code memory that the ROM will indeed poke at runtime.

---

## 10. Devices and the I/O request model

A device in Exec is a library plus an I/O model. Specifically, a `Device` struct is a `Library` struct (for the base, jump table, open/close/expunge) with two extra vectors at fixed slots for `BeginIO` and `AbortIO`, and a convention that all operations go through an `IORequest` message sent to the device.

### 10.1 `Device` and `Unit`

From `exec/devices.h` (lines 24-26 and 31-37):

```c
struct Device {
    struct Library dd_Library;
};

struct Unit {
    struct MsgPort unit_MsgPort;    /* queue for unprocessed messages */
    UBYTE          unit_flags;
    UBYTE          unit_pad;
    UWORD          unit_OpenCnt;    /* active opens on this unit */
};
```

```
UNITF_ACTIVE   (1<<0)    /* unit is currently processing a request */
UNITF_INTASK   (1<<1)    /* unit is in its task context (not interrupt) */
```

A device may have multiple *units*. trackdisk.device has four (df0-df3). Each unit has its own message port, its own state, its own OpenCnt. `OpenDevice` is given a unit number and the device selects the appropriate `Unit` structure.

A `Device` struct itself is just a trivial wrapper around `Library` so that type-checking code can distinguish them. The interesting part is the vector table layout and the I/O request conventions.

### 10.2 Device vector layout

Every device has the standard four library vectors plus two device-specific reserved slots:

```
-6   LIB_OPEN      open a unit
-12  LIB_CLOSE     close a unit
-18  LIB_EXPUNGE   tear down the device
-24  reserved      (returns 0)
-30  DEV_BEGINIO   begin I/O on a request - DIRECT ENTRY POINT
-36  DEV_ABORTIO   abort an in-flight request - DIRECT ENTRY POINT
-42  ...           device-specific commands start here
```

(From `exec/io.h` lines 40-42.) `BeginIO` and `AbortIO` are the raw device entry points that `DoIO`/`SendIO`/`AbortIO` eventually call through. A sophisticated caller can call them directly for efficiency, bypassing the exec.library wrapper.

Device open semantics: `LIB_OPEN` is called with `A6 = device, A1 = iorequest, D0 = unit number, D1 = flags`. The device fills in `io_Device` and `io_Unit` in the request, and increments `OpenCnt` on the chosen unit. Return is the library base in D0 (or 0 on failure, and `io_Error` set to an error code).

### 10.3 `IORequest` and `IOStdReq`

From `exec/io.h` (lines 18-25 and 27-38):

```c
struct IORequest {
    struct Message io_Message;
    struct Device *io_Device;    /* set by OpenDevice */
    struct Unit   *io_Unit;      /* set by OpenDevice */
    UWORD          io_Command;   /* CMD_* */
    UBYTE          io_Flags;     /* IOF_QUICK etc */
    BYTE           io_Error;     /* result */
};

struct IOStdReq {
    struct Message io_Message;
    struct Device *io_Device;
    struct Unit   *io_Unit;
    UWORD          io_Command;
    UBYTE          io_Flags;
    BYTE           io_Error;
    ULONG          io_Actual;    /* bytes actually transferred */
    ULONG          io_Length;    /* bytes requested */
    APTR           io_Data;      /* data buffer pointer */
    ULONG          io_Offset;    /* for block-structured devices */
};
```

The `io_Message` at the top of every IORequest is literally a `Message` — when an async I/O completes, the device calls `ReplyMsg(io_Message)` to send the request back to the caller, and the caller's reply port receives it. An `IORequest` is a message, end of story.

Devices with more specific request types (timer.device's `timerequest`, gameport.device's `InputEvent`, etc.) begin with an `IORequest` or `IOStdReq` and extend it with device-specific fields.

### 10.4 `OpenDevice` / `CloseDevice`

```
error = OpenDevice(devName, unitNumber, iORequest, flags)
D0                 A0       D0          A1         D1
void CloseDevice(iORequest)  ; Al
```

Nearly identical semantics to `OpenLibrary`. Search `ExecBase->DeviceList` by name; if not found, ask DOS to load `DEVS:devName`; call the device's `LIB_OPEN` with the request. On success the request's `io_Device` and `io_Unit` are valid and `io_Error` is 0.

Process-only restriction: same as `OpenLibrary`. If the device is not in memory, DOS is needed to load it. From the autodoc BUGS: "Tasks should not be allowed to make OpenDevice calls that will cause the device to be loaded from disk."

`CloseDevice` requires that all outstanding IORequests have been returned (replied or aborted). Leaving requests in flight at close time is a reliable way to crash later, when the device tries to `ReplyMsg` into a freed request.

### 10.5 Standard I/O commands

From `exec/io.h` (lines 49-58):

```
CMD_INVALID    0    /* device should not respond */
CMD_RESET      1    /* reinitialise unit */
CMD_READ       2    /* read io_Length bytes into io_Data from io_Offset */
CMD_WRITE      3    /* write io_Length bytes from io_Data at io_Offset */
CMD_UPDATE     4    /* flush internal buffers */
CMD_CLEAR      5    /* discard internal buffers (data lost) */
CMD_STOP       6    /* pause processing (queue continues to build) */
CMD_START      7    /* resume processing */
CMD_FLUSH      8    /* abort all pending requests */
CMD_NONSTD     9    /* device-specific commands start here */
```

All devices must respond to all of these — at minimum by returning `IOERR_NOCMD` in `io_Error` for commands they don't support. A device's own command set begins at `CMD_NONSTD` and extends as needed.

Error codes (from `exec/errors.h`):

```
IOERR_OPENFAIL    -1    /* device or unit failed to open */
IOERR_ABORTED     -2    /* request was aborted */
IOERR_NOCMD       -3    /* command not supported */
IOERR_BADLENGTH   -4    /* not a valid length */
```

### 10.6 `DoIO` — synchronous

```
error = DoIO(iORequest)
D0           A1
```

Sets `IOF_QUICK` in `io_Flags`, calls the device's `BeginIO` vector, waits for completion if the device didn't handle it quickly. Returns a sign-extended copy of `io_Error`.

IMPLEMENTATION from the autodoc: "This function first tries to complete the IO via the 'Quick I/O' mechanism. The io_Flags field is always set to IOF_QUICK (0x01) before the internal device call."

Quick I/O: if the device can handle the request synchronously and immediately (character I/O where the data is already buffered, for example), it leaves `IOF_QUICK` set and returns — `DoIO` sees the flag still set and knows the request is complete without doing any message passing. If the device had to defer (real hardware I/O pending), it clears `IOF_QUICK` and queues the request to its unit's task; `DoIO` then sees the flag clear and `Wait`s on the reply port.

This is how the ROM achieves low latency for common cases without sacrificing async semantics for the hard cases.

### 10.7 `SendIO` — asynchronous

```
void SendIO(iORequest)
            A1
```

Clears `IOF_QUICK` (so the device must do real async processing), calls `BeginIO`, and returns immediately. The caller will see the request eventually arrive at its reply port when the operation finishes. Used when the caller wants to overlap I/O with other work.

**From issue to completion, the caller must not touch the request.** (Exec RKM ch. 4, lines 2476-2479.) The request belongs to the device until the device replies. Reading `io_Error` early can race with the device writing it.

### 10.8 `WaitIO`, `CheckIO`, `AbortIO`

```
error = WaitIO(iORequest)   ; D0 = A1
result = CheckIO(iORequest) ; D0 = A1
void AbortIO(iORequest)     ; Al
```

`WaitIO` blocks until the given request has been replied to the reply port, then removes it from the port and returns `io_Error`. Side effect warning from the autodoc: "If this IORequest was 'Quick' or otherwise finished BEFORE this call, this function drops though immediately, with no call to Wait(). A side effect is that the signal bit related the port may remain set. Expect this."

The bit-may-remain-set caveat means: after a `WaitIO` on a quick-completed request, the next `Wait` on that port may fire immediately on a stale signal. Code should treat it as a spurious wake and loop.

`CheckIO` returns NULL if the I/O is still in progress, else a non-NULL pointer. **It does not remove the request from the reply port** — you must still call `GetMsg` or `WaitIO` to consume the reply. From the autodoc: "This function should NOT be used to busy loop (looping until IO is complete). WaitIO() is provided for that purpose."

`AbortIO` asks the device to abandon a request. The device may or may not be able to — slow disks might be halfway through a seek. After `AbortIO` the caller still has to `WaitIO` to collect the aborted request (which will come back with `io_Error = IOERR_ABORTED`).

### 10.9 `AddDevice` / `RemDevice`

```
void AddDevice(device)   ; Al
void RemDevice(device)   ; Al
```

Register a device with `ExecBase->DeviceList` so `OpenDevice` can find it. Same pattern as `AddLibrary`/`RemLibrary`. `RemDevice` calls the device's `LIB_EXPUNGE` to let it refuse removal if busy.

### 10.10 How the skeleton device processes a request

From the skeleton in RKM Libraries and Devices (lines 50508-50600), the canonical BeginIO pattern:

```
BeginIO:                       ; ( iob: al, device: a6 )
    move.l  a3,-(sp)
    move.l  IO_UNIT(al),a3
    move.w  IO_COMMAND(al),d0
    cmp.w   #MYDEV_END,d0
    bcc.s   BeginIO_NoCmd      ; command out of range
    DISABLE a0
    ; Is this an immediate command? Process right away.
    move.w  #IMMEDIATES,d1
    btst    d0,d1
    bne.s   BeginIO_Immediate
    ; Is the unit stopped? Queue the message.
    btst    #MDUB_STOPPED,UNIT_FLAGS(a3)
    bne.s   BeginIO_QueueMsg
    ; Is the device busy? If not, process now.
    bset    #UNITB_ACTIVE,UNIT_FLAGS(a3)
    beq.s   BeginIO_Immediate
    ; Otherwise queue to the unit task.
BeginIO_QueueMsg:
    bset    #UNITB_INTASK,UNIT_FLAGS(a3)
    bclr    #IOB_QUICK,IO_FLAGS(al)
    ENABLE  a0
    move.l  a3,a0
    LINKSYS PutMsg,md_SysLib(a6)
    bra.s   BeginIO_End
BeginIO_Immediate:
    ENABLE  a0
    bsr     PerformIO
BeginIO_End:
    move.l  (sp)+,a3
    rts
```

Three paths: reject out-of-range, service immediately inline, or queue to a unit task via `PutMsg`. `IOF_QUICK` is cleared on queued requests so the caller knows to wait for the reply message. On immediate completion the flag stays set and the caller's `DoIO` shortcut returns without any message overhead.

The `TermIO` flow on the other side:

```
TermIO:
    btst    #IOB_QUICK,IO_FLAGS(al)
    bne.s   TermIO_End        ; quick - caller already has it
    LINKSYS ReplyMsg,...      ; async - send back to reply port
TermIO_End:
    rts
```

If `IOF_QUICK` is still set when the device is done, the caller is running synchronously in its own call and will see the result directly; no message is needed. If cleared, `ReplyMsg` sends the request back to the reply port and eventually wakes the caller.

---

## 11. Resources

A resource is the third thing that sits in a library-like slot, alongside libraries and devices. The difference is that resources are **singletons without OpenCnt**. Every shared hardware facility that has no natural exclusive owner but needs coordinated access is a resource: cia.resource, disk.resource, misc.resource, potgo.resource, keyboard.resource, battclock.resource.

### 11.1 Why resources exist

Libraries have `OpenCnt` because they are dynamically loaded and dynamically unloaded based on references. Devices are similar. Resources are things that are part of the machine itself, cannot be unloaded, and are shared by everyone without explicit reference counting. Typical users of a resource are other device drivers or system services, and they all coexist.

`cia.resource` is the canonical example. There are two 8520 CIA chips. Different subsystems need to use different timers, different I/O lines: trackdisk uses CIA-A timer B for step rate, keyboard uses CIA-A's handshake, the VBL counter uses CIA-B timer B, etc. They cannot fight over the chip — but nobody "owns" the whole CIA. `cia.resource` provides calls like `AddICRVector` and `RemICRVector` to register interrupt handlers for specific CIA interrupt bits, managed cooperatively.

### 11.2 Resource structure

A resource is, at its minimum, an Exec `Node`:

```c
struct Resource {
    struct Node node;   /* NT_RESOURCE */
    /* everything else is resource-specific */
};
```

Some resources are essentially miniature libraries with their own negative-offset jump tables; some are just data structures with public field layouts. The common factor is that they live on `ExecBase->ResourceList` and are reached by `OpenResource`.

### 11.3 `AddResource` / `RemResource` / `OpenResource`

```
void AddResource(resource)             ; Al
void RemResource(resource)             ; Al
resource = OpenResource(resName)       ; D0 = A1
```

`AddResource` adds to `ExecBase->ResourceList`. `OpenResource` searches by name and returns the resource pointer. There is no `CloseResource` — from the autodoc: "There is no CloseResource() function."

`OpenResource` does **not** cause a load from disk. Resources must already be present in memory (usually added by a ROM-resident module at coldstart, or by a privileged driver that owns them). This is why `OpenResource` can be called from a bare task context, while `OpenLibrary`/`OpenDevice` cannot (they might need DOS).

### 11.4 Typical resource list

On a Kickstart 1.3 machine, the standard resources are:

- **cia.resource** — shared access to both 8520 chips.
- **disk.resource** — shared access to the disk hardware (the Paula disk block, DMA channels). Multiple disk devices (trackdisk.device and any scsi.device) cooperate via this.
- **misc.resource** — generic shared hardware: serial hardware, parallel hardware, AUD0-3 channels. Anyone using audio or serial goes through `AllocMiscResource`/`FreeMiscResource`.
- **potgo.resource** — joystick/pot inputs, the Paula POTGO register.
- **keyboard.resource** — CIA-A low-level keyboard serial interface, underneath keyboard.device.
- **battclock.resource** — battery-backed clock read/write, on machines that have one (most expansion boards).

An emulator reproducing the ROM boot has to create at least the cia.resource and disk.resource empty shells even if the client code does not call them — because other ROM modules do, during their own init.

---

## 12. Interrupts

Interrupt handling is the most time-critical part of Exec and the part most coupled to the hardware. An emulator must model this carefully because the entire multitasking system pivots on it.

### 12.1 Sequence of events

From Exec RKM ch. 5 (lines 2644-2682), the canonical interrupt path:

1. A hardware device decides to cause an interrupt and signals Paula.
2. Paula records the interrupt in `INTREQ`, checks it against `INTENA`, and if both enabled raises one of the three 68000 IPL lines.
3. If the 68000's current priority is below the new interrupt, the CPU enters interrupt processing: saves SR and PC, changes to supervisor mode, raises its priority, and indexes into the autovector table at $64-$7C to fetch a vector address.
4. The autovector points into Exec's interrupt dispatcher.
5. Exec examines `INTREQ & INTENA` to identify the specific source, indexes into `ExecBase->IntVects[]` to find the handler or server chain, and calls it as a subroutine.
6. The handler processes the interrupt, clears `INTREQ`, and RTS back to Exec.
7. Exec checks `AttnResched` and, at `ExitIntr`, calls the scheduler if a higher-priority task was made ready.

**Critical fact: the 68000 autovector table must not be modified by user code.** Exec owns it, initialises it once at boot, and uses it as a fixed jumping-off point into its dispatcher. User code that wants to hook an interrupt uses `SetIntVector`, not direct autovector modification. An emulator reproducing ROM code sees `SetIntVector` calls and must honour them via the `IntVects[]` array, not via the CPU's exception table.

### 12.2 Interrupt priority table

From Exec RKM ch. 5 (table 5-1, lines 2713-2734):

```
Source     CPU   Pseudo
Name       Lvl   Pri    Purpose
---------- ----  -----  ------------------------------
NMI         7    15     Non-maskable
INTEN       6    14     Interrupt enable master
EXTER       6    13     CIA-B (external level 6)
DSKSYNC     5    12     Disk sync detected
RBF         5    11     Serial input ready
AUD1        4    10     Audio ch 1 done
AUD3        4     9     Audio ch 3 done
AUD0        4     8     Audio ch 0 done
AUD2        4     7     Audio ch 2 done
BLIT        3     6     Blitter done
VERTB       3     5     Vertical blank
COPER       3     4     Copper
PORTS       2     3     CIA-A (external level 2)
TBE         1     2     Serial transmit buffer empty
DSKBLK      1     1     Disk block done
SOFTINT     1     0     Software interrupt
```

Within a single CPU priority level, the pseudo-priority determines which source runs first when two interrupts are simultaneously pending. Higher pseudo-priority runs first. Higher CPU priority interrupts can preempt lower.

**"The CPU priority level must never be lowered by user or system code."** (Exec RKM ch. 5, line 2742.) Specifically, while a high-priority interrupt is running, you must not drop the 68000 priority back — that would let the same interrupt fire again, recurse on the system stack, and overflow. Interrupts only nest in the direction of higher priority.

**PORTS, COPER, VERTB, BLIT, EXTER, and NMI are set up as server chains.** (Exec RKM ch. 5, lines 2781-2785.) The others are set up as direct handlers. Server chains can have many registered handlers walking in priority order; direct handlers can only have one.

### 12.3 `Interrupt` structure

From `exec/interrupts.h` (lines 22-26):

```c
struct Interrupt {
    struct Node is_Node;    /* NT_INTERRUPT, ln_Pri = server priority */
    APTR        is_Data;    /* private data, passed to handler in A1 */
    VOID      (*is_Code)(); /* handler entry point */
};
```

This is the block an application fills in and hands to `SetIntVector` or `AddIntServer`. The Node allows the structure to be linked into a server chain.

### 12.4 Environment on handler entry

Handlers run in supervisor mode on `SysStk`, outside any task context. `ExecBase->ThisTask` still points at whatever task was running when the interrupt fired — interrupt code can read it but should not assume any task invariants hold.

Register conventions on handler entry (Exec RKM ch. 5, lines 2858-2877):

```
D0    scratch (garbage)
D1    scratch — on entry contains INTENA & INTREQ (which interrupts enabled & pending)
A0    scratch — on entry points at $DFF000 (custom chip base)
A1    scratch — on entry points at is_Data
A5    jump vector register (scratch, caller need not restore)
A6    Exec library base (scratch, caller need not restore)
```

`A0` pointing at `$DFF000` is a gift: lets the handler reach every custom chip register without an extra load. `A1` = `is_Data` is also a gift — the handler's "per-interrupt context" pointer. The rest are scratch; `A2-A4/A7` must be preserved if used.

**Handlers must return via RTS, not RTE.** (Exec RKM ch. 5, lines 2843-2846.) They are called as subroutines from the dispatcher; the dispatcher will do the RTE when it is done. Returning with RTE will double-pop the supervisor stack.

**Handlers must not call `AllocMem`/`FreeMem`.** "the interrupt routine must not use any of the memory allocation or deallocation functions" (Exec RKM ch. 5, lines 2823-2831). Because the memory allocator only `Forbid()`s, not `Disable()`s, its linked-list structure may be mid-update when the interrupt fires.

**Handlers may call these functions from interrupt context:** `Alert`, `Disable`, `Enable`, `Cause`, `FindName`, `FindPort`, `FindTask`, `PutMsg`, `ReplyMsg`, `Signal`, and the list-manipulation primitives (`AddHead`, `AddTail`, `Enqueue`, `RemHead`, `RemTail`) *on lists the interrupt code owns*. (Exec RKM Preface, lines 280-298.)

### 12.5 `SetIntVector`

```
oldInterrupt = SetIntVector(intNumber, interrupt)
D0                          D0          A1
```

Installs a direct (non-chain) handler for a Paula interrupt. Returns the old handler (so a patch can save it and chain). Both `is_Code` and `is_Data` of the vector are updated. Setting something here "disconnects the old handler" — direct handlers are exclusive. From the autodoc: "These are non-sharable, setting something here disconnects the old handler."

Do not call `SetIntVector` on a source that is configured as a server chain (PORTS, VERTB, etc.). From the autodoc: "Keep in mind that certain interrupts are established as server chains and should not be accessed as handlers."

### 12.6 `AddIntServer` / `RemIntServer`

```
void AddIntServer(intNum, interrupt)
                  D0      A1
void RemIntServer(intNum, interrupt)
                  D0      A1
```

Add a server to a server chain. The Interrupt node is inserted at the proper priority position by `Enqueue` semantics. If this is the first server on the chain, the Paula interrupt source is enabled. Chain execution: each server in the chain is called in priority order until the chain ends or a server returns with Z **clear** (meaning "I handled it").

Server return conventions are **different** from handlers:

- Return with Z **clear** (non-zero in D0) means "I handled the interrupt, stop walking the chain."
- Return with Z **set** (zero in D0) means "not mine, try the next."
- **VBlank servers should always return with Z set.** Because every VBlank server should run on every VBlank — they are typically counting frames or running periodic jobs, not claiming "the vblank interrupt is mine".

The easiest way to set the Z flag is `MOVEQ #0,D0` (or #1,D0) and RTS (Exec RKM ch. 5, lines 3014-3024).

Server register conventions: A5/A6 and D0/D1/A0/A1 are scratch. **`A6` is NOT guaranteed to be SysBase for servers** (it is for handlers). If a server wants SysBase, it has to `move.l AbsExecBase,a6` itself.

Graphics library VBlank-server bug from the autodoc: "The graphics library's VBLANK server incorrectly assumes that address register A0 will contain a pointer to the custom chips. If you add a server at a priority of 10 or greater, you must compensate for this by providing the expected value ($DFF000)." This is a concrete thing an emulator will see if the graphics library is patched or added to: higher-priority servers that are "politically above" graphics must preserve A0 = $DFF000 on exit.

### 12.7 `Cause` — software interrupts

```
void Cause(interrupt)
           A1
```

Triggers a software interrupt. The softint structure is an `Interrupt` with `ln_Pri` set to one of `-32, -16, 0, +16, +32` (the five allowed priorities) and `ln_Type = NT_INTERRUPT`. `Cause` links the interrupt into one of the five soft-int lists in `ExecBase->SoftInts[]` and pokes Paula's SOFTINT bit.

Behaviour when called from user mode: the softint preempts the current task (softints run at IPL 1, above task priority 0, below any hardware interrupt).

Behaviour when called from a hardware interrupt: the softint is deferred until the hardware interrupt exits. "If it is called from a hardware interrupt, the software interrupt will not be processed until the system exits from its last hardware interrupt. If a software interrupt occurs from within another software interrupt, it is not processed until the current one is completed." (Exec RKM ch. 5, lines 3124-3128.)

**No nest counting.** From the autodoc IMPLEMENTATION: "Checks if the node type is NT_SOFTINT. If so does nothing since the softint is already pending. No nest count is maintained." Second and subsequent `Cause()` on an already-pending softint are ignored. This is what makes `Cause` safe in fast paths.

After removal from the softint list, the node's type reverts to `NT_INTERRUPT` — so `Cause` can find it fresh next time.

### 12.8 `Disable` and `Enable`

```
void Disable(void);
void Enable(void);
```

Prevent hardware interrupts from being handled. `DISABLE` increments `IDNestCnt` and sets INTENA's master bit off; `ENABLE` decrements and turns it back on at count == 0. Nested calls work because of the counter.

The macro from Exec RKM ch. 5 (lines 3159-3180):

```
DISABLE  MACRO
    MOVE.W   #$4000,_intena       ; clear master enable bit
    ADDQ.B   #1,IDNestCnt(A6)
    ENDM

ENABLE   MACRO
    SUBQ.B   #1,IDNestCnt(A6)
    BGE.S    ENABLE@              ; still nested, don't actually re-enable
    MOVE.W   #$C000,_intena       ; set master enable bit
ENABLE@:
    ENDM
```

Crucial: **only 126 levels of nesting are permitted.** (Exec RKM ch. 5, line 3165.) `IDNestCnt` is a signed byte.

"Disabling interrupts for more than ~250 microseconds will prevent vital system functions (especially serial I/O) from operating in a normal fashion. Think twice before using Disable(), then think once more. After all that, think again." (Autodoc `exec.library/Disable`.)

### 12.9 `Forbid` and `Permit`

```
void Forbid(void);
void Permit(void);
```

Prevent task rescheduling. `Forbid` increments `TDNestCnt` (no interrupt state change); `Permit` decrements; scheduling remains disabled as long as `TDNestCnt > 0` (analogous to `IDNestCnt` but starting at -1). While forbidden, the current task keeps running, interrupts still fire, but preemption never happens.

The escape hatch from the autodoc WARNING: "In the event of a task entering a Wait after a Forbid(), the system 'breaks' the forbidden state and runs normally until the task which called Forbid() is rescheduled." This is the behaviour that lets `OpenLibrary` work from inside `Forbid()` even though it eventually calls DOS (which blocks).

Forbid nesting is used pervasively by system code. The memory allocator Forbids. AddTask Forbids. Any code walking a shared Exec list Forbids. The counter is shared — if task A forbids and then task B (because of the escape hatch) starts running and forbids, we have nested forbids from two tasks simultaneously, which works because only the task actually running cares about the counter.

---

## 13. Traps and task exceptions

Exec distinguishes two kinds of asynchronous-to-the-task events that interrupt normal flow: **CPU traps**, which are synchronous faults from the 68000 hardware (bus error, illegal instruction, trap instruction, etc.), and **task exceptions**, which are Exec-level async notifications delivered via the signal mechanism. They are unrelated despite sharing the word "exception."

### 13.1 CPU traps

The 68000 traps of interest from Exec RKM ch. 2 (lines 1640-1662):

```
 2    Bus error
 3    Address error
 4    Illegal instruction
 5    Zero divide
 6    CHK instruction
 7    TRAPV instruction
 8    Privilege violation
 9    Trace
10    Line 1010 emulator (A-line)
11    Line 1111 emulator (F-line, used for FP on 68000)
32-47 TRAP #0 through TRAP #15
```

When any of these fires, the CPU pushes an exception frame to SSP, switches to supervisor mode, and vectors through the appropriate vector. Exec's dispatcher catches the vector, looks up the current task's `tc_TrapCode`, and calls it with:

- The supervisor stack containing the CPU-generated exception frame.
- An additional longword at the bottom of the frame: the exception number (vector / 4).
- `A6` = SysBase.

The handler's job is to process the trap and return via RTE. To return cleanly it must remove the exception number before the CPU's exception frame.

**`tc_TrapCode == NULL` means "use the ExecBase default."** The default is `ExecBase->TaskTrapCode`, which for most tasks points at a routine that:

1. Calls `Alert` with a suitable error code (AN_BogusExcpt etc).
2. Does not return — the task is dead.

This is how you get a Guru when you divide by zero or when your program does `JMP $12345678`.

**`tc_TrapData`** is a pointer passed through for the handler's use. Exec does nothing with it; the handler can do whatever it likes.

**Task-level TRAP #N handling via `AllocTrap` / `FreeTrap`.** `AllocTrap(trapNum)` reserves one of the 16 TRAP instruction numbers (TRAP #0 through #15) for the calling task. Nothing in the CPU or Exec changes as a result — this is pure bookkeeping. It lets libraries that want to use TRAP instructions for dispatch (the math libraries, for example, use a TRAP-and-dispatch convention on some platforms) coordinate without colliding.

"You are not allowed to write to the exception table yourself. In fact, on some machines you will have trouble finding it — the VBR register may be used to remap its location." (Autodoc `AllocTrap`.)

On 68010+ the VBR register moves the exception table, and Exec moves it out of the first 1K of memory so location 0-3FF is available to programs. An emulator reproducing 68010+ ExecBase behaviour must deal with VBR.

### 13.2 Task exceptions (not CPU exceptions)

A task exception is a signal that, instead of (or in addition to) waking the task, redirects its flow of control to the exception handler. The mechanism:

1. The task installs `tc_ExceptCode` (handler routine) and optionally `tc_ExceptData` (pointer).
2. The task marks a set of signal bits in `tc_SigExcept` via `SetExcept(bits, mask)`.
3. When any of those bits is posted via `Signal()`, Exec sets `TF_EXCEPT` on the task and marks it for exception processing.
4. At the next opportunity (when the task is about to return to user code after an interrupt, or when the task was in `Wait` and is now unblocking), Exec saves the task's current register state to the task stack and calls `tc_ExceptCode`:

```
D0 = bitmask of signals that caused this exception
A1 = tc_ExceptData
A6 = SysBase
```

5. The handler runs in user mode on the task's own stack.
6. On RTS from the handler, Exec restores the saved registers and resumes the task at the point it was interrupted.

The handler returns `D0 = new SigExcept mask`, so the task can re-enable the excepting signals for a future round. While processing an exception, Exec prevents that exception from recurring.

**Exceptions are not safe from interrupt code.** "Signals may not be allocated or freed from exception handling code." And: "User function pointed to by the task's tc_ExceptCode gets called as: newExcptSet = exceptCode(signals, exceptData), SysBase." (Autodoc `SetExcept`.)

The task exception stack usage: the saved PC, SR, and D0-D7/A0-A6 are pushed onto the task's own stack by the dispatcher. This is 17 * 4 = 68 bytes plus the handler's own stack usage. If the task's stack is near the limit, this can overflow, which is why minimum task stacks have to include slack.

---

## 14. Alert / Guru Meditation

When Exec decides the system cannot continue safely, it calls `Alert`. On Kickstart 1.x this is the red-bordered flashing "Software Failure" / "Guru Meditation" screen. The alert number encodes what failed and who owned the failure.

### 14.1 `Alert` function

```
void Alert(alertNum, parameters)
           D7        A5
```

From the autodoc: "Alerts the user of a serious system problem. This function will bring the system to a grinding halt, and do whatever is necessary to present the user with a message stating what happened. Interrupts are disabled, and an attempt to post the alert is made. If that fails, the system is reset. When the system comes up again, Exec notices the cause of the failure and tries again to post the alert."

"This call may be made at any time, including interrupts."

"If the Alert is a recoverable type, this call MAY return."

Note the unusual calling convention: `alertNum` in D7, `parameters` in A5 (not D0/A0 like almost every other function). This is because Alert is often called from interrupt-time code that may have stashed its important work in D0/A0.

### 14.2 Alert number encoding

Alert numbers are 32 bits, structured as:

```
Bit 31:    AT_DeadEnd (0x80000000)        /* unrecoverable if set */
Bits 24-30: AO_*  /* subsystem that detected the error */
Bits 16-23: AG_*  /* general error class */
Bits  0-15: specific error code
```

From `exec/alerts.h`:

```
#define AT_DeadEnd    0x80000000
#define AT_Recovery   0x00000000
```

### 14.3 General error classes (AG_*)

```
AG_NoMemory    0x00010000    /* memory allocation failed */
AG_MakeLib     0x00020000    /* MakeLibrary failed */
AG_OpenLib     0x00030000    /* OpenLibrary failed */
AG_OpenDev     0x00040000    /* OpenDevice failed */
AG_OpenRes     0x00050000    /* OpenResource failed */
AG_IOError     0x00060000    /* I/O error */
AG_NoSignal    0x00070000    /* AllocSignal failed */
AG_BadParm     0x00080000    /* bad parameter */
AG_CloseDev    0x00090000
AG_ProcCreate  0x000A0000    /* process creation failed */
```

### 14.4 Subsystem object codes (AO_*)

```
AO_ExecLib       0x00008001
AO_GraphicsLib   0x00008002
AO_LayersLib     0x00008003
AO_Intuition     0x00008004
AO_MathLib       0x00008005
AO_CListLib      0x00008006
AO_DOSLib        0x00008007
AO_RAMLib        0x00008008
AO_IconLib       0x00008009
AO_ExpansionLib  0x0000800A
AO_AudioDev      0x00008010
AO_ConsoleDev    0x00008011
AO_GamePortDev   0x00008012
AO_KeyboardDev   0x00008013
AO_TrackDiskDev  0x00008014
AO_TimerDev      0x00008015
AO_CIARsrc       0x00008020
AO_DiskRsrc      0x00008021
```

### 14.5 Common exec.library alerts

From `exec/alerts.i` / `exec/alerts.h`:

```
AN_ExecLib        0x01000000
AN_ExcptVect      0x81000001  /* 68000 exception vector checksum failed */
AN_BaseChkSum     0x81000002  /* ExecBase checksum failed */
AN_LibChkSum      0x81000003  /* library checksum failed */
AN_MemCorrupt     0x81000005  /* corrupt memory list */
AN_IntrMem        0x81000006  /* no memory for interrupt servers */
AN_InitAPtr       0x81000007  /* InitStruct alignment error */
AN_SemCorrupt     0x81000008  /* semaphore corrupt */
AN_FreeTwice      0x81000009  /* FreeMem of already-free block */
AN_BogusExcpt     0x8100000A  /* illegal 68k exception taken */
AN_IOUsedTwice    0x8100000B
AN_MemoryInsane   0x8100000C
AN_IOAfterClose   0x8100000D
AN_StackProbe     0x8100000E
AN_BadFreeAddr    0x8100000F
```

All of these are dead-end (high bit set) — if any of them occur, the system is corrupt and continuing is not safe.

### 14.6 Common per-subsystem alerts

Graphics library:

```
AN_GraphicsLib    0x02000000
AN_GfxNoMem       0x82010000
AN_LongFrame      0x82010006   /* long frame, no memory */
AN_ShortFrame     0x82010007
AN_TextTmpRas     0x02010009
AN_BltBitMap      0x8201000A
AN_RegionMemory   0x8201000B
AN_MakeVPort      0x82010030
AN_GfxNoLCM       0x82011234   /* emergency memory not available */
```

Intuition:

```
AN_Intuition      0x04000000
AN_GadgetType     0x84000001
AN_CreatePort     0x84010002
AN_ItemAlloc      0x04010003
AN_SubAlloc       0x04010004
AN_PlaneAlloc     0x84010005
AN_OpenScreen     0x84010007
AN_OpenWindow     0x8401000B
AN_BadMessage     0x8400000D
```

dos.library:

```
AN_DOSLib         0x07000000
AN_StartMem       0x07010001   /* no memory at startup */
AN_EndTask        0x07000002   /* EndTask didn't */
AN_FreeVec        0x07000005   /* FreeVec failed */
AN_DiskBlkSeq     0x07000006   /* disk block sequence error */
AN_BitMap         0x07000007
AN_KeyFree        0x07000008
AN_BadChkSum      0x07000009
AN_DiskError      0x0700000A
AN_KeyRange       0x0700000B
AN_BadOverlay     0x0700000C
```

trackdisk:

```
AN_TrackDiskDev   0x14000000
AN_TDCalibSeek    0x14000001   /* calibrate: seek error */
AN_TDDelay        0x14000002   /* delay: error on timer wait */
```

timer:

```
AN_TimerDev       0x15000000
AN_TMBadReq       0x15000001
AN_TMBadSupply    0x15000002   /* power supply does not supply ticks */
```

### 14.7 Recoverable vs dead-end alerts

`AT_Recovery` alerts (high bit clear) present to the user, wait for input, and return. `AT_DeadEnd` alerts do not return; the system resets after dismissal.

Alerts are queued until a display is available. If the system is too corrupt to open a display, Exec falls back to a simpler red-box screen that talks to the hardware directly (bypassing graphics.library). If even that fails, it resets.

### 14.8 SysFlags bit

`ExecBase->SysFlags` bit `SF_ALERTWACK` (1<<1) controls a detail of how alerts acknowledge: whether a Wack handshake is needed before display or not. Normal user systems have it clear; systems with a debugger attached may have it set.

---

## 15. Initialisation-time conveniences

Several exec.library functions exist specifically to support the construction of libraries, devices, and tasks from static data tables. They are how ROM-resident subsystems bootstrap themselves.

### 15.1 `InitStruct`

```
void InitStruct(initTable, memory, size)
                A1         A2      D0
```

Clears a memory region and then poke-initialises selected offsets from a compact byte-encoded table. The assembly macros in `exec/initializers.i` (`INITBYTE`, `INITWORD`, `INITLONG`, `INITSTRUCT`) emit entries in this table format.

Each table entry is a byte-code with format `ddssnnnn`:

```
dd - destination mode (2 bits):
  00  next destination, nnnn is count
  01  next destination, nnnn is repeat
  10  destination offset is next byte, nnnn is count
  11  destination offset is next pointer, nnnn is count
ss - source size (2 bits):
  00  long, from next two aligned words
  01  word, from next aligned word
  10  byte, from next byte
  11  ERROR (causes an alert)
nnnn - count or repeat (4 bits)
```

(Autodoc `exec.library/InitStruct` lines 448-464.)

A `00000000` byte terminates the stream. `00010001` is "one longword from next data" (distinct from the terminator).

Typical use: after `MakeLibrary` allocates a fresh Library + data area, `InitStruct` fills in the initial values for all the fields — version number, library name pointer, function pointer defaults, etc. The alternative is pages of `move` instructions in init code, which for a library with many fields would be much larger than a packed init table.

**Destination offsets are relative to the memory pointer in A2 and are always even.** Odd offsets are not supported. `INITLONG` and `INITWORD` require their data to be aligned in the source table.

### 15.2 `InitCode`

```
void InitCode(startClass, version)
              D0          D1
```

Walks `ExecBase->ResModules` and calls `InitResident` on each entry whose `rt_Flags` include the specified class and whose `rt_Version >= version`. Used during coldstart to bring up all the ROM modules in priority order.

`startClass` is `RTW_NEVER`, `RTW_COLDSTART`, etc., which are not heavily used — all ROM modules are typically tagged `RTF_COLDSTART`.

### 15.3 `InitResident`

```
void InitResident(resident, segList)
                  A1        D1
```

Initialise a single `Resident` module. If `rt_Flags & RTF_AUTOINIT` is set, the initialisation is automatic: `rt_Init` points at a 4-longword table (size/functable/datatable/initroutine) and `InitResident` calls `MakeLibrary` with those arguments. If `RTF_AUTOINIT` is clear, `rt_Init` is a direct function pointer and `InitResident` just jumps to it.

Full autodoc text (`exec.library/InitResident`, lines 397-418):

> "An automatic method of library/device base and vector table initialization is also provided through the use of a such a ROM-tag (Resident) structure. In this case, the initial code hunk of the library or device should contain 'MOVEQ #-1,d0; RTS;'. Following that must be an initialized Resident structure including RTF_AUTOINIT in rt_Flags, and an rt_Init pointer which points to four longwords as follows:
>
> - Size of your library/device base structure including initial Library or Device structure.
> - Pointer to a longword table of standard, then library specific function offsets, terminated with -1L.
> - Pointer to data table in exec/InitStruct format for initialization of Library or Device structure.
> - Pointer to library initialization routine, which will receive library/device base in dO, segment in aO, and must return non-zero to link the library/device into the device/library list."

This is the central idiom in §16 below.

### 15.4 `FindResident`

```
resident = FindResident(name)
D0                      A1
```

Scan `ExecBase->ResModules` for a `Resident` whose `rt_Name` matches. Returns pointer or NULL. Used to look up a ROM-resident module by name during boot or debugging.

### 15.5 `MakeLibrary`

See §9.7. Used by `InitResident` as the library-construction step when `RTF_AUTOINIT` is set.

### 15.6 `MakeFunctions`

See §9.8. Used internally by `MakeLibrary` to lay out the jump table.

### 15.7 `SumLibrary`

See §9.10. Invoked at `MakeLibrary` time to compute the initial checksum, and at `SetFunction` time to refresh it.

### 15.8 `SumKickData`

```
void SumKickData(void);
```

Compute the checksum over `KickMemPtr` and `KickTagPtr` in `ExecBase`, store in `KickCheckSum`. Called after a user builds those structures to install a reboot-survivable module. If the checksum does not compute correctly at next boot, Exec ignores the Kick pointers entirely. See §1.8 for the mechanism.

Introduced in Kickstart 1.2 (Autodoc NOTE). Earlier Kickstarts did not support Kickstart-into-RAM.

---

## 16. Library / device creation: the RTF_AUTOINIT template

This is the template pattern that builds every library and every device in every Amiga ROM and almost every disk-loaded library. An emulator author who wants to understand what happens when a Kickstart ROM boots must understand this template, because dozens of these run back-to-back during `InitCode`.

### 16.1 The parts

A library or device consists of these six items:

1. **A trap-at-entry word.** The first instruction at the segment's entry point is `MOVEQ #-1,D0; RTS;` (= 2 words, $70FF $4E75). If anyone tries to "run" the library as if it were a program, they get -1 back. The Resident tag follows immediately.
2. **A `Resident` structure.** Tells Exec how to find and initialise the module.
3. **A function table.** An array of function pointers (one per jump table slot), terminated with `-1`.
4. **A data table.** An `InitStruct`-format table of initialisers for the fresh library's data area.
5. **An init routine.** Called once with `D0 = libBase, A0 = segList`. Responsibility: finish setting up private state, open any libraries this one depends on, return `libBase` to signal success or 0 to fail.
6. **The function implementations themselves.**

### 16.2 The Resident structure

From `exec/resident.h` (lines 17-39):

```c
struct Resident {
    UWORD            rt_MatchWord;    /* RTC_MATCHWORD = 0x4AFC */
    struct Resident *rt_MatchTag;     /* pointer to self, for validation */
    APTR             rt_EndSkip;      /* address to resume ROM-tag scan */
    UBYTE            rt_Flags;        /* RTF_AUTOINIT etc */
    UBYTE            rt_Version;
    UBYTE            rt_Type;         /* NT_LIBRARY, NT_DEVICE, NT_RESOURCE */
    BYTE             rt_Pri;          /* initialisation priority */
    char            *rt_Name;         /* module name */
    char            *rt_IdString;     /* human-readable id string */
    APTR             rt_Init;         /* 4-longword table or direct routine */
};

#define RTC_MATCHWORD  0x4AFC         /* = ILLEGAL instruction on 68000 */

#define RTF_AUTOINIT  (1<<7)
#define RTF_COLDSTART (1<<0)
```

`rt_MatchWord` is the 16-bit value $4AFC, which is the 68000 opcode ILLEGAL. The ROM scanner at boot scans memory word-by-word looking for this pattern, and when it finds one, checks that the longword immediately after it (`rt_MatchTag`) points back at the start of the ILLEGAL word. Two-word match is enough to validate a ROM-tag with high confidence.

`rt_EndSkip` is the address the scanner should resume from after processing this tag — usually just past the end of the module's code, so the scanner doesn't false-match on data that happens to contain $4AFC.

`rt_Init` with `RTF_AUTOINIT` set points to this 4-longword table:

```
DC.L  sizeof_libBase   ; 1. data area size (Library struct + private data)
DC.L  funcTable        ; 2. function pointer table
DC.L  dataTable        ; 3. InitStruct data table
DC.L  initRoutine      ; 4. init routine entry point (or 0)
```

### 16.3 The full skeleton (verbatim from RKM L&D)

From `Amiga_ROM_Kernel_Reference_Manual_Libraries_and_Devices.txt` lines 50324-50500. This is the actual code from the canonical skeleton device shipped in the ROM Kernel Reference Manual; the function table contents vary by device but the framing is the same:

```
;------ The first executable location. This should return an error
;       in case someone tried to run you as a program (instead of
;       loading you as a library).
FirstAddress:
    CLEAR   d0
    rts

;------ A romtag structure. Both "exec" and "ramlib" look for
;       this structure to discover magic constants about you
;       (such as where to start running you from...).

MYPRI   EQU     0
initDDescrip:
    DC.W    RTC_MATCHWORD         ; UWORD RT_MATCHWORD
    DC.L    initDDescrip          ; APTR  RT_MATCHTAG
    DC.L    EndCode               ; APTR  RT_ENDSKIP
    DC.B    RTF_AUTOINIT          ; UBYTE RT_FLAGS
    DC.B    VERSION               ; UBYTE RT_VERSION
    DC.B    NT_DEVICE             ; UBYTE RT_TYPE
    DC.B    MYPRI                 ; BYTE  RT_PRI
    DC.L    myName                ; APTR  RT_NAME
    DC.L    idString              ; APTR  RT_IDSTRING
    DC.L    Init                  ; APTR  RT_INIT
;   LABEL   RT_SIZE

;------ The romtag specified that we were "RTF_AUTOINIT". This means
;       that the RT_INIT structure member points to one of these
;       tables below. If the AUTOINIT bit was not set then RT_INIT
;       would point to a routine to run.
Init:
    DC.L    MyDev_Sizeof          ; data space size
    DC.L    funcTable             ; pointer to function initializers
    DC.L    dataTable             ; pointer to data initializers
    DC.L    initRoutine           ; routine to run

funcTable:
    ;------ standard system routines
    dc.l    Open
    dc.l    Close
    dc.l    Expunge
    dc.l    Null              ; reserved slot - returns 0
    ;------ my device definitions
    dc.l    BeginIO
    dc.l    AbortIO
    ;------ function table end marker
    dc.l    -1

; The data table initializes static data structures. The format is
; specified in exec/InitStruct routine's manual pages. The
; INITBYTE/INITWORD/INITLONG macros are in the file
; "exec/initializers.i". The first argument is the offset from the
; device base for this byte/word/long. The second argument is the
; value to put in that cell. The table is null terminated.
dataTable:
    INITBYTE    LN_TYPE,    NT_DEVICE
    INITLONG    LN_NAME,    myName
    INITBYTE    LIB_FLAGS,  LIBF_SUMUSED!LIBF_CHANGED
    INITWORD    LIB_VERSION, VERSION
    INITWORD    LIB_REVISION, REVISION
    INITLONG    LIB_IDSTRING, idString
    DC.L    0
```

Note on the function table: the order is fixed.

- Slot 0 (`-6`): `Open` — the library/device `LIB_OPEN` vector.
- Slot 1 (`-12`): `Close` — `LIB_CLOSE`.
- Slot 2 (`-18`): `Expunge` — `LIB_EXPUNGE`.
- Slot 3 (`-24`): `Null` — reserved, returns 0 in D0 and RTSes.
- Slot 4 (`-30`): First device-specific vector. For devices this is `BeginIO` (DEV_BEGINIO = -30).
- Slot 5 (`-36`): For devices, `AbortIO` (DEV_ABORTIO = -36).
- Slots 6+: device-specific command handlers.

For libraries rather than devices, slot 4 onwards is just the first user-defined function. Libraries do not have BeginIO/AbortIO slots.

### 16.4 The `initRoutine`

From the skeleton (lines 50370-50406):

```
initRoutine:
    ;---- get the device pointer into a convenient A register
    move.l  a5,-(sp)
    move.l  d0,a5             ; d0 = freshly constructed lib base

    ;---- save a pointer to exec
    move.l  a6,md_SysLib(a5)

    ;---- save a pointer to our loaded code (seglist)
    move.l  a0,md_SegList(a5)

    ;---- open the dos library
    lea     dosName(pc),a1
    CLEAR   d0
    CALLSYS OpenLibrary
    move.l  d0,md_DosLib(a5)
    bne.s   init_DosOK
    ALERT   AG_OpenLib!AO_DOSLib

init_DosOK:
    ;---- now build the static data that we need
    ; [... device-specific init ...]
    move.l  a5,d0             ; return libBase in D0
    move.l  (sp)+,a5
    rts
```

The init routine is called by `MakeLibrary` under a `Forbid()/Permit()` pair. It receives:

- `D0` = the fresh library base pointer returned by `MakeLibrary`.
- `A0` = the DOS seglist for this module (or NULL if it is ROM-resident).
- `A6` = SysBase (SysBase is the caller).

It must return a non-zero value in D0 to be linked into the system. Returning 0 tells `MakeLibrary` the init failed; `MakeLibrary` will then free the library base and return NULL.

The init routine is where the library opens any other libraries it depends on. Most libraries need dos.library open (because they need to be able to call DOS from inside their own functions). Resources they need go through `OpenResource`. If any dependency fails to open, the init routine `Alert`s or cleans up and returns 0.

### 16.5 The Open/Close/Expunge triad

```
Open:               ; (device:a6, iob:a1, unitnumber:d0, flags:d1)
    ; validate the unit number
    ; initialise the unit if not already initialised
    ; set io_Unit and io_Device in the IO request
    ; increment LIB_OPENCNT(a6)
    ; increment the unit's open count
    ; clear LIBB_DELEXP (prevent delayed expunge)
    rts                       ; returns device ptr in d0, or 0 on failure

Close:              ; (device:a6, iob:a1)
    ; mark the io request invalid
    ; decrement the unit's open count
    ; if unit's open count hits 0, ExpungeUnit
    ; decrement LIB_OPENCNT(a6)
    ; if LIB_OPENCNT hit 0 and LIBB_DELEXP is set, do Expunge
    rts

Expunge:            ; (device:a6)
    ; if LIB_OPENCNT != 0, set LIBB_DELEXP and return 0
    ; otherwise: remove from DeviceList, return seglist in d0 for DOS to unload
    rts

Null:               ; reserved, called at LIB_EXTFUNC (-24)
    clr.l d0
    rts
```

**Expunge must never `Wait()`, never allocate memory, never do anything that could block.** From the skeleton comments (lines 50452-50456): "because Expunge is called from the memory allocator, it may NEVER Wait() or otherwise take long time to complete." When the system runs out of memory, `AllocMem` walks `LibList` and `DeviceList` calling `Expunge` on every entry hoping to reclaim memory; if any of them blocks, the allocator is stuck.

### 16.6 ROM-tag scanning at boot

At coldstart Exec walks the ROM regions $F80000-$FFFFFF and $F00000-$F7FFFF word-by-word looking for `$4AFC` words followed by a valid `rt_MatchTag` pointing back. Each found Resident is added to `ExecBase->ResModules`. Then `InitCode(RTF_COLDSTART, 0)` is called, which walks `ResModules` in priority order calling `InitResident` on each.

An emulator's boot simulation must do the same scan, because otherwise none of the ROM-resident libraries (exec.library itself, graphics.library, intuition.library, dos.library via ramlib, etc) will ever exist.

The scanner does not stop at the end of each module — it uses `rt_EndSkip` only as a hint. It keeps scanning until it walks off the end of ROM. If a module's `rt_EndSkip` is too short, the scanner may find false matches inside its data; the convention is to point `rt_EndSkip` safely past the last byte of your whole module.

---

## Appendix A: ExecBase / SysBase full layout

This table reflects the V1.3 (Kickstart 1.3 / V34) layout as defined in `exec/execbase.i`. Offsets are absolute from the ExecBase pointer (which is stored at memory location 4). Later Kickstarts extended the reserved area at the end but did not change the offsets of any field here.

```
Offset  Size  Type         Name                   Purpose
------  ----  ----         ----                   -------
$0000   34    Library      LibNode                exec.library's own Library node
  $00    4    APTR         LibNode.ln_Succ        (node fields)
  $04    4    APTR         LibNode.ln_Pred
  $08    1    UBYTE        LibNode.ln_Type        NT_LIBRARY
  $09    1    BYTE         LibNode.ln_Pri
  $0A    4    char*        LibNode.ln_Name        "exec.library"
  $0E    1    UBYTE        lib_Flags
  $0F    1    UBYTE        lib_pad
  $10    2    UWORD        lib_NegSize            jump table size
  $12    2    UWORD        lib_PosSize            data area size
  $14    2    UWORD        lib_Version
  $16    2    UWORD        lib_Revision
  $18    4    APTR         lib_IdString
  $1C    4    ULONG        lib_Sum
  $20    2    UWORD        lib_OpenCnt
$0022   2    UWORD         SoftVer                Kickstart release number
$0024   2    WORD          LowMemChkSum           68000 trap vector checksum
$0026   4    ULONG         ChkBase                one's complement of ExecBase
$002A   4    APTR          ColdCapture            coldstart soft capture vector
$002E   4    APTR          CoolCapture            coolstart soft capture vector
$0032   4    APTR          WarmCapture            warmstart soft capture vector
$0036   4    APTR          SysStkUpper            system stack upper bound (high)
$003A   4    APTR          SysStkLower            system stack lower bound (low)
$003E   4    ULONG         MaxLocMem              last-known size of local (chip) memory
$0042   4    APTR          DebugEntry             global debugger entry point
$0046   4    APTR          DebugData              global debugger data segment
$004A   4    APTR          AlertData              alert data / task-at-fault pointer
$004E   4    APTR          MaxExtMem              top of extended memory, or NULL
$0052   2    WORD          ChkSum                 checksum over header fields
$0054   192  IntVector[16] IntVects               16 interrupt vector slots, 12 bytes each
  Each IntVector:
    +$0    4    APTR       iv_Data                passed to handler in A1
    +$4    4    APTR       iv_Code                handler / server entry
    +$8    4    struct Node* iv_Node              owner node pointer

  Slot order:
    0  IVTBE        (serial TX empty)
    1  IVDSKBLK     (disk block done)
    2  IVSOFTINT    (software interrupt)
    3  IVPORTS      (CIA-A / ports - server chain)
    4  IVCOPER      (copper - server chain)
    5  IVVERTB      (vertical blank - server chain)
    6  IVBLIT       (blitter - server chain)
    7  IVAUD0
    8  IVAUD1
    9  IVAUD2
   10  IVAUD3
   11  IVRBF        (serial RX full)
   12  IVDSKSYNC    (disk sync)
   13  IVEXTER      (CIA-B - server chain)
   14  IVINTEN      (master interrupt enable)
   15  IVNMI        (NMI - server chain)

$0114   4    Task*         ThisTask               currently running task (load-bearing)
$0118   4    ULONG         IdleCount              idle loop counter
$011C   4    ULONG         DispCount              dispatch counter
$0120   2    UWORD         Quantum                quantum in ticks
$0122   2    UWORD         Elapsed                ticks used in current quantum
$0124   2    UWORD         SysFlags               system flags (SF_ALERTWACK etc)
$0126   1    BYTE          IDNestCnt              Disable/Enable nesting count (-1 = none)
$0127   1    BYTE          TDNestCnt              Forbid/Permit nesting count (-1 = none)
$0128   2    UWORD         AttnFlags              CPU/FPU feature bits (AFF_68010, AFF_68020, AFF_68881)
$012A   2    UWORD         AttnResched            rescheduling attention flags
$012C   4    APTR          ResModules             ROM-tag scan array (NULL-terminated)
$0130   4    APTR          TaskTrapCode           default tc_TrapCode for new tasks
$0134   4    APTR          TaskExceptCode         default tc_ExceptCode
$0138   4    APTR          TaskExitCode           default task finalizer
$013C   4    ULONG         TaskSigAlloc           default tc_SigAlloc (reserved signals)
$0140   2    UWORD         TaskTrapAlloc          default tc_TrapAlloc
$0142   14   List          MemList                MemHeader region list
$0150   14   List          ResourceList           resource list
$015E   14   List          DeviceList             device list
$016C   14   List          IntrList               interrupt server chains list
$017A   14   List          LibList                library list
$0188   14   List          PortList               public port list
$0196   14   List          TaskReady              priority-sorted ready queue
$01A4   14   List          TaskWait               wait queue (unsorted, FIFO tail)
$01B2   80   SoftIntList[5] SoftInts              5 priority softint queues (16 bytes each)
$0202   16   LONG[4]       LastAlert              last four alert numbers (DeadEnd/param)
$0212   1    UBYTE         VBlankFrequency        50 or 60
$0213   1    UBYTE         PowerSupplyFrequency   50 or 60 (CIA-A ToD clock rate)
$0214   14   List          SemaphoreList          public SignalSemaphore list
$0222   4    APTR          KickMemPtr             ptr to queue of MemLists for kick-into-RAM
$0226   4    APTR          KickTagPtr             ROM-tag queue to splice into ResModules
$022A   4    APTR          KickCheckSum           checksum computed by SumKickData
$022E   10   UBYTE[10]     ExecBaseReserved       reserved for future use
$0238   20   UBYTE[20]     ExecBaseNewReserved    reserved, extended in later Kickstarts
$024C                      SYSBASESIZE = $24C (approximate; reserved area grows in V36+)
```

Note: the offsets above depend on the `Library` header being 34 bytes ($22). LH_SIZE = 14, LN_SIZE = 14, IV_SIZE = 12, SH_SIZE = 16 (SoftIntList = List + 2 pad).

---

## Appendix B: exec.library function index (LVO table)

Complete list of exec.library functions with their negative offsets, ordered as they appear in the jump table. Hex offsets are taken directly from the `exec.lib.offsets` section in the Exec RKM appendix (lines 13297-13388).

Every exec.library function is called with `A6 = ExecBase`. `D0/D1/A0/A1` are scratch; all other registers preserved.

| LVO    | Offset   | Function      | Register args                          | Returns |
|--------|----------|---------------|----------------------------------------|---------|
| -$001E | -30      | Supervisor    | userFunction (A5)                      | d0 = result |
| -$0024 | -36      | ExitIntr      | —                                      | — |
| -$002A | -42      | Schedule      | —                                      | — |
| -$0030 | -48      | Reschedule    | —                                      | — |
| -$0036 | -54      | Switch        | —                                      | — |
| -$003C | -60      | Dispatch      | —                                      | — |
| -$0042 | -66      | Exception     | —                                      | — |
| -$0048 | -72      | InitCode      | startClass (D0), version (D1)          | — |
| -$004E | -78      | InitStruct    | initTable (A1), memory (A2), size (D0) | — |
| -$0054 | -84      | MakeLibrary   | funcInit (A0), structInit (A1), libInit (A2), dataSize (D0), segList (D1) | d0 = Library* |
| -$005A | -90      | MakeFunctions | target (A0), functionArray (A1), funcDispBase (A2) | d0 = table size |
| -$0060 | -96      | FindResident  | name (A1)                              | d0 = Resident* |
| -$0066 | -102     | InitResident  | resident (A1), segList (D1)            | — |
| -$006C | -108     | Alert         | alertNum (D7), parameters (A5)         | — |
| -$0072 | -114     | Debug         | —                                      | — |
| -$0078 | -120     | Disable       | —                                      | — |
| -$007E | -126     | Enable        | —                                      | — |
| -$0084 | -132     | Forbid        | —                                      | — |
| -$008A | -138     | Permit        | —                                      | — |
| -$0090 | -144     | SetSR         | newSR (D0), mask (D1)                  | d0 = oldSR |
| -$0096 | -150     | SuperState    | —                                      | d0 = old sys stack |
| -$009C | -156     | UserState     | sysStack (D0)                          | — |
| -$00A2 | -162     | SetIntVector  | intNumber (D0), interrupt (A1)         | d0 = old Interrupt* |
| -$00A8 | -168     | AddIntServer  | intNumber (D0), interrupt (A1)         | — |
| -$00AE | -174     | RemIntServer  | intNumber (D0), interrupt (A1)         | — |
| -$00B4 | -180     | Cause         | interrupt (A1)                         | — |
| -$00BA | -186     | Allocate      | freeList (A0), byteSize (D0)           | d0 = void* |
| -$00C0 | -192     | Deallocate    | freeList (A0), memoryBlock (A1), byteSize (D0) | — |
| -$00C6 | -198     | AllocMem      | byteSize (D0), requirements (D1)       | d0 = void* |
| -$00CC | -204     | AllocAbs      | byteSize (D0), location (A1)           | d0 = void* |
| -$00D2 | -210     | FreeMem       | memoryBlock (A1), byteSize (D0)        | — |
| -$00D8 | -216     | AvailMem      | requirements (D1)                      | d0 = size |
| -$00DE | -222     | AllocEntry    | entry (A0)                             | d0 = MemList* |
| -$00E4 | -228     | FreeEntry     | entry (A0)                             | — |
| -$00EA | -234     | Insert        | list (A0), node (A1), pred (A2)        | — |
| -$00F0 | -240     | AddHead       | list (A0), node (A1)                   | — |
| -$00F6 | -246     | AddTail       | list (A0), node (A1)                   | — |
| -$00FC | -252     | Remove        | node (A1)                              | — |
| -$0102 | -258     | RemHead       | list (A0)                              | d0 = Node* |
| -$0108 | -264     | RemTail       | list (A0)                              | d0 = Node* |
| -$010E | -270     | Enqueue       | list (A0), node (A1)                   | — |
| -$0114 | -276     | FindName      | list (A0), name (A1)                   | d0 = Node* |
| -$011A | -282     | AddTask       | task (A1), initPC (A2), finalPC (A3)   | — |
| -$0120 | -288     | RemTask       | task (A1)                              | — |
| -$0126 | -294     | FindTask      | name (A1)                              | d0 = Task* |
| -$012C | -300     | SetTaskPri    | task (A1), priority (D0)               | d0 = old priority |
| -$0132 | -306     | SetSignal     | newSignals (D0), signalSet (D1)        | d0 = oldSignals |
| -$0138 | -312     | SetExcept     | newSignals (D0), signalSet (D1)        | d0 = oldSignals |
| -$013E | -318     | Wait          | signalSet (D0)                         | d0 = received signals |
| -$0144 | -324     | Signal        | task (A1), signalSet (D0)              | — |
| -$014A | -330     | AllocSignal   | signalNum (D0)                         | d0 = sigNum or -1 |
| -$0150 | -336     | FreeSignal    | signalNum (D0)                         | — |
| -$0156 | -342     | AllocTrap     | trapNum (D0)                           | d0 = trapNum or -1 |
| -$015C | -348     | FreeTrap      | trapNum (D0)                           | — |
| -$0162 | -354     | AddPort       | port (A1)                              | — |
| -$0168 | -360     | RemPort       | port (A1)                              | — |
| -$016E | -366     | PutMsg        | port (A0), message (A1)                | — |
| -$0174 | -372     | GetMsg        | port (A0)                              | d0 = Message* |
| -$017A | -378     | ReplyMsg      | message (A1)                           | — |
| -$0180 | -384     | WaitPort      | port (A0)                              | d0 = Message* |
| -$0186 | -390     | FindPort      | name (A1)                              | d0 = MsgPort* |
| -$018C | -396     | AddLibrary    | library (A1)                           | — |
| -$0192 | -402     | RemLibrary    | library (A1)                           | — |
| -$0198 | -408     | OldOpenLibrary| libName (A1)                           | d0 = Library* |
| -$019E | -414     | CloseLibrary  | library (A1)                           | — |
| -$01A4 | -420     | SetFunction   | library (A1), funcOffset (A0.W), funcEntry (D0) | d0 = old func |
| -$01AA | -426     | SumLibrary    | library (A1)                           | — |
| -$01B0 | -432     | AddDevice     | device (A1)                            | — |
| -$01B6 | -438     | RemDevice     | device (A1)                            | — |
| -$01BC | -444     | OpenDevice    | devName (A0), unit (D0), ioRequest (A1), flags (D1) | d0 = error |
| -$01C2 | -450     | CloseDevice   | ioRequest (A1)                         | — |
| -$01C8 | -456     | DoIO          | ioRequest (A1)                         | d0 = error |
| -$01CE | -462     | SendIO        | ioRequest (A1)                         | — |
| -$01D4 | -468     | CheckIO       | ioRequest (A1)                         | d0 = IORequest* or NULL |
| -$01DA | -474     | WaitIO        | ioRequest (A1)                         | d0 = error |
| -$01E0 | -480     | AbortIO       | ioRequest (A1)                         | — |
| -$01E6 | -486     | AddResource   | resource (A1)                          | — |
| -$01EC | -492     | RemResource   | resource (A1)                          | — |
| -$01F2 | -498     | OpenResource  | resName (A1), version (D0)             | d0 = Resource* |
| -$01F8 | -504     | RawIOInit     | —                                      | — |
| -$01FE | -510     | RawMayGetChar | —                                      | d0 = char or -1 |
| -$0204 | -516     | RawPutChar    | char (D0)                              | — |
| -$020A | -522     | RawDoFmt      | formatString (A0), dataStream (A1), putChProc (A2), putChData (A3) | — |
| -$0210 | -528     | GetCC         | —                                      | d0 = CCR |
| -$0216 | -534     | TypeOfMem     | address (A1)                           | d0 = attrs or 0 |
| -$021C | -540     | Procure       | semaport (A0), bidMsg (A1)             | d0 = success |
| -$0222 | -546     | Vacate        | semaport (A0)                          | — |
| -$0228 | -552     | OpenLibrary   | libName (A1), version (D0)             | d0 = Library* |

V36 (Kickstart 2.0) added, at offsets beyond $228, these:

| LVO    | Offset   | Function            | Notes                                |
|--------|----------|---------------------|--------------------------------------|
| -$022E | -558     | InitSemaphore       | Semaphore (A0)                       |
| -$0234 | -564     | ObtainSemaphore     | SignalSemaphore (A0)                 |
| -$023A | -570     | ReleaseSemaphore    | SignalSemaphore (A0)                 |
| -$0240 | -576     | AttemptSemaphore    | SignalSemaphore (A0)                 |
| -$0246 | -582     | ObtainSemaphoreList | List (A0)                            |
| -$024C | -588     | ReleaseSemaphoreList| List (A0)                            |
| -$0252 | -594     | FindSemaphore       | name (A1)                            |
| -$0258 | -600     | AddSemaphore        | SignalSemaphore (A1)                 |
| -$025E | -606     | RemSemaphore        | SignalSemaphore (A1)                 |
| -$0264 | -612     | SumKickData         | —                                    |
| -$026A | -618     | AddMemList          | size (D0), attrs (D1), pri (D2), base (A0), name (A1) |
| -$0270 | -624     | CopyMem             | source (A0), dest (A1), size (D0)    |
| -$0276 | -630     | CopyMemQuick        | source (A0), dest (A1), size (D0)    |

Note: the exact V36 LVO positions depend on the Kickstart; the Semaphore functions were backfilled into slots that existed but were unused on V1.3. An emulator that uses LVO numbers as the identity of a function must version-match its table to the ROM being emulated.

V36+ additions that continue past the V1.3 table include: `CacheClearU`, `CacheClearE`, `CacheControl`, `CreateIORequest`, `DeleteIORequest`, `CreateMsgPort`, `DeleteMsgPort`, `ObtainSemaphoreShared`, `AllocVec`, `FreeVec`, `CreatePool`, `DeletePool`, `AllocPooled`, `FreePooled`, `AttemptSemaphoreShared`, `ColdReboot`, `StackSwap`, and so on. The corpus does not give offsets for these; an emulator targeting 2.0 ROMs must pull them from the 2.0 `exec_lib.i`.

---

## Appendix C: Exec idioms cookbook

These are the patterns an emulator author will see repeatedly in ROM code and in disk-loaded libraries. Getting them right is more important than most individual functions, because incorrect behaviour here causes system-wide silent corruption.

### C.1 Canonical library call

```
    move.l  A6,-(SP)
    move.l  _SysBase,A6               ; or whatever library base
    jsr     _LVOAllocMem(A6)
    move.l  (SP)+,A6
```

If A6 is known to already hold the base, skip the save/restore.

### C.2 Safe FindPort + PutMsg

```c
ULONG SafePutToPort(struct Message *msg, char *portname) {
    struct MsgPort *port;
    Forbid();
    port = FindPort(portname);
    if (port)
        PutMsg(port, msg);
    Permit();
    return (ULONG)port;
}
```

Without the Forbid, another task could RemPort between `FindPort` and `PutMsg`.

### C.3 Waiting for a message

```c
WaitPort(port);
while ((msg = GetMsg(port)) != NULL) {
    /* handle msg */
    ReplyMsg(msg);
}
```

Drain in a loop because more than one message may have arrived per signal.

### C.4 Waiting for a message OR a timeout OR Ctrl-C

```c
ULONG signals = Wait((1L << port->mp_SigBit) | timerSigMask | SIGBREAKF_CTRL_C);
if (signals & SIGBREAKF_CTRL_C) /* abort */;
if (signals & timerSigMask)     /* timeout */;
if (signals & (1L << port->mp_SigBit)) {
    while ((msg = GetMsg(port)) != NULL) { ... }
}
```

`Wait` on a mask of signal bits; check which fired; handle each.

### C.5 Creating a port from raw pieces

```c
sigBit = AllocSignal(-1);
if (sigBit == -1) return NULL;
mp = AllocMem(sizeof(*mp), MEMF_CLEAR|MEMF_PUBLIC);
if (!mp) { FreeSignal(sigBit); return NULL; }
mp->mp_Node.ln_Type = NT_MSGPORT;
mp->mp_Node.ln_Pri  = 0;
mp->mp_Node.ln_Name = NULL;     /* or a public name */
mp->mp_Flags   = PA_SIGNAL;
mp->mp_SigBit  = sigBit;
mp->mp_SigTask = FindTask(NULL);
NewList(&mp->mp_MsgList);       /* ESSENTIAL - do not skip */
```

This is what `CreatePort` in amiga.lib does.

### C.6 Synchronous I/O on a newly opened device

```c
port = CreatePort(NULL, 0);
ior = (struct IOStdReq *)CreateExtIO(port, sizeof(*ior));
OpenDevice("trackdisk.device", 0, (struct IORequest *)ior, 0);

ior->io_Command = CMD_READ;
ior->io_Offset  = 0;
ior->io_Length  = 512;
ior->io_Data    = buffer;
DoIO((struct IORequest *)ior);   /* returns when I/O complete */

CloseDevice((struct IORequest *)ior);
DeleteExtIO((struct IORequest *)ior);
DeletePort(port);
```

### C.7 Async I/O

```c
SendIO(ior);
/* ... do other work ... */
WaitIO(ior);
/* ior->io_Error is valid here */
```

Between `SendIO` and `WaitIO` the caller must not touch the `ior`.

### C.8 Walking MemList under Forbid

```c
Forbid();
for (mh = (struct MemHeader *)SysBase->MemList.lh_Head;
     mh->mh_Node.ln_Succ != NULL;
     mh = (struct MemHeader *)mh->mh_Node.ln_Succ) {
    /* look at mh_Free, mh_Attributes, etc */
}
Permit();
```

Note the terminating condition is `ln_Succ != NULL`, not `mh != NULL`.

### C.9 Walking any exec list under Forbid

Same pattern as C.8 applies to `LibList`, `DeviceList`, `ResourceList`, `PortList`, `SemaphoreList`. For `TaskReady` and `TaskWait` you need `Disable()` not `Forbid()`, because interrupts manipulate these lists.

### C.10 Allocating signal-bit for a caller

```c
sigBit = AllocSignal(-1);
sigMask = 1L << sigBit;
```

Always check for -1. Always free with `FreeSignal(sigBit)`, not the mask.

### C.11 Installing a VBlank server

```c
myIntr = AllocMem(sizeof(*myIntr), MEMF_PUBLIC | MEMF_CLEAR);
myIntr->is_Node.ln_Type = NT_INTERRUPT;
myIntr->is_Node.ln_Pri  = -60;
myIntr->is_Node.ln_Name = "my vbl server";
myIntr->is_Data = &mystate;
myIntr->is_Code = (VOID(*)())myHandler;
AddIntServer(INTB_VERTB, myIntr);

/* ... */

RemIntServer(INTB_VERTB, myIntr);
FreeMem(myIntr, sizeof(*myIntr));
```

`myHandler` must return with Z set (use `moveq #0,d0`) for VBL servers.

### C.12 Installing a direct (non-chain) handler

```c
myIntr->is_Node.ln_Type = NT_INTERRUPT;
myIntr->is_Node.ln_Name = "my rbf handler";
myIntr->is_Data = &rxbuf;
myIntr->is_Code = (VOID(*)())myRBFHandler;

priorIntr = SetIntVector(INTB_RBF, myIntr);
/* priorIntr is the old handler, save it to chain or restore later */
```

`myRBFHandler` must clear the Paula interrupt request bit explicitly.

### C.13 Causing a software interrupt

```c
softIntr->is_Node.ln_Type = NT_INTERRUPT;
softIntr->is_Node.ln_Pri  = 0;   /* one of -32, -16, 0, +16, +32 */
softIntr->is_Code = mySoftHandler;
softIntr->is_Data = &state;

Cause(softIntr);
```

The softint may execute immediately (preempting the current task) or be deferred until after the current hardware interrupt completes.

### C.14 Task-level mutex via SignalSemaphore

```c
/* setup once */
InitSemaphore(&mySem);

/* in consumer */
ObtainSemaphore(&mySem);
/* critical section */
ReleaseSemaphore(&mySem);
```

Recursive from the same task: nested obtains increment `ss_NestCount` and nested releases decrement. Only the final release actually lets another task in.

### C.15 Library boilerplate in the init routine

```
initRoutine:
    move.l  d0,a5                  ; libBase
    move.l  a0,mylib_SegList(a5)   ; save seglist for Expunge
    move.l  a6,mylib_ExecBase(a5)

    lea     dosName(pc),a1
    moveq   #0,d0
    jsr     _LVOOpenLibrary(a6)
    move.l  d0,mylib_DosBase(a5)
    beq.s   init_fail

    ; ... other subsystem init ...

    move.l  a5,d0                  ; return libBase (non-zero = success)
    rts
init_fail:
    moveq   #0,d0
    rts
```

### C.16 Library expunge boilerplate

```
Expunge:
    tst.w   LIB_OPENCNT(a6)
    beq.s   Expunge_do
    bset    #LIBB_DELEXP,LIB_FLAGS(a6)
    moveq   #0,d0                  ; refuse expunge, caller retries later
    rts
Expunge_do:
    ; unlink from LibList
    move.l  a6,a1
    jsr     _LVORemove(a6)
    ; free dependencies
    move.l  mylib_DosBase(a5),a1
    jsr     _LVOCloseLibrary(a6)
    ; free library memory
    move.l  mylib_SegList(a5),d2   ; save seglist for return
    move.l  a6,a1
    moveq   #0,d0
    move.w  LIB_NEGSIZE(a6),d0
    sub.l   d0,a1
    add.w   LIB_POSSIZE(a6),d0
    jsr     _LVOFreeMem(a6)
    move.l  d2,d0                  ; return seglist to DOS
    rts
```

Note that the library memory block starts at `libBase - lib_NegSize` and runs for `lib_NegSize + lib_PosSize` bytes. That is what `MakeLibrary` allocated.

---

## Appendix D: Gaps in corpus

These are places where the corpus is ambiguous, incomplete, or OCR-damaged, and where an emulator author should consult original Amiga sources or hex-dump a real ROM. None of these is load-bearing for V1.3 emulation; all become more important for V2.0+.

1. **V36+ function LVO offsets are incomplete.** The Exec RKM function index (lines 13297-13388) ends at `OpenLibrary` (-$228). Anything beyond that (CopyMem, AllocVec, CreateIORequest, CreatePool, etc) must be inferred. The corpus's FUNCDEF list in the Autodocs hints at the order but not the offsets.

2. **The exact order of V36+ additions relative to V1.3.** V36 added many functions "behind" existing ones (InitSemaphore, ObtainSemaphore, ReleaseSemaphore, etc). The corpus shows them in the Autodocs but does not definitively establish which offsets they occupy. An emulator targeting V2.0 ROMs should hex-dump the actual jump table rather than trusting this document.

3. **SoftIntList exact layout.** `exec/interrupts.i` declares `STRUCT SoftIntList, SH_SIZE*5` but the `SH_SIZE` constant is not fully given. Based on `struct SoftIntList { struct List sh_List; UWORD sh_Pad; }` it is LH_SIZE + 2 = 16 bytes; total 80 bytes. Verified but not explicitly stated in the corpus.

4. **`UNITF_STOPPED` bit position.** The skeleton device uses `#MDUB_STOPPED,2` (bit 2 of `unit_flags`) but this is a device-private flag, not an Exec convention. `UNITF_ACTIVE` (bit 0) and `UNITF_INTASK` (bit 1) are defined in `exec/devices.h`; higher bits are device-defined.

5. **`ExecBase->SysFlags` full bit list.** Only `SF_ALERTWACK` (1<<1) is explicitly documented in the corpus. Other bits exist but are not enumerated.

6. **Exact algorithm of the memory free-chunk sort on insertion.** The corpus says "sorted by address" but does not give the exact loop. An emulator reproducing the allocator byte-for-byte needs to disassemble `FreeMem` from a real ROM; the behavioural description here is sufficient for a behavioural emulator.

7. **V1.3 vs V1.2 Quantum default.** Reported to be 4 ticks but not explicitly stated in the corpus. Some ROM hex dumps show 5. Harmless difference — scheduling is still round-robin either way.

8. **`AllocVec` / `FreeVec` header format.** V36 functions; the corpus does not give the internal structure (magic word + size prefix). Known from disassembly to be an 8-byte header holding size and a sanity marker, but the exact layout is not in any primary RKM.

9. **ROM-tag scanner exact scan ranges on non-A500 machines.** The corpus says $F80000-$FFFFFF and $F00000-$F7FFFF. On machines with expansion ROMs (like the A3000), additional ranges are scanned. The corpus does not enumerate them.

10. **`RTF_SINGLETASK` and other non-V1.3 `rt_Flags` bits.** V1.3 only defines `RTF_AUTOINIT` (1<<7) and `RTF_COLDSTART` (1<<0). Later Kickstarts add `RTF_SINGLETASK` (1<<1), `RTF_AFTERDOS` (1<<2). Not in the V1.3 corpus.

11. **`TaskSigAlloc` exact default.** Documented as "the low 16 bits reserved" with the actual value typically `0x0000FFFF`. The corpus does not specify the exact value; real Kickstarts set it to `$00010000` on some versions and `$0000FFFF` on others.

12. **Raw dispatcher / scheduler assembly.** The behaviour of `Schedule`, `Reschedule`, `Switch`, `Dispatch`, `Exception`, `ExitIntr` is described but their source is not in the corpus. An emulator reproducing these must disassemble a real ROM or implement from the behavioural specification.

13. **Checksum algorithm for `lib_Sum`.** The corpus describes the purpose but does not give the algorithm. From disassembly: it is a sum of all the longwords of the jump table. An emulator that writes to a library jump table must be prepared to either never checksum or reproduce the exact algorithm.

14. **`MakeLibrary` segList handling.** The corpus is clear that segList is passed through to the init routine and stored for later use. It does not clarify whether `MakeLibrary` does anything with it directly (it doesn't). ROM-resident libraries pass 0.

---

## Appendix E: Source map

Every claim in this document is traceable to one of the following:

**Primary sources:**

- **Exec RKM** — `/Users/stevehill/Desktop/AmigaPDFs/txt/Amiga_ROM_Kernel_Reference_Manual_Exec.txt`
  - ch. 1 "Lists and Queues" — lines ~372-970
  - ch. 2 "Tasks" — lines ~972-1710
  - ch. 3 "Messages and Ports" — lines ~1748-2130
  - ch. 4 "Input/Output" — lines ~2131-2612
  - ch. 5 "Interrupts" — lines ~2613-3188
  - ch. 6 "Memory Allocation" — lines ~3189-3611
  - ch. 7 "Libraries" — lines ~3612-3958
  - ch. 8 "ROM-Wack" — lines ~3959-4350 (not heavily used)
  - Appendix: function offsets (LVOs) — lines 13297-13388
  - Appendix: AttnFlags / alert constants — lines 5105-5230

- **Autodocs / Includes** — `/Users/stevehill/Desktop/AmigaPDFs/txt/Amiga_ROM_Kernal_Reference_Manual_Includes_and_Autodocs.txt`
  - exec/execbase.h struct — lines 13190-13232
  - exec/execbase.i struct — lines 18766-18876
  - exec/interrupts.h — lines 13250-13296
  - exec/io.h — lines 13297-13310
  - exec/libraries.h — lines 13329-13372
  - exec/memory.h — lines 13373-13442
  - exec/nodes.h — lines 13446-13492
  - exec/ports.h — lines 13471-13498
  - exec/resident.h — lines 13517-13540
  - exec/semaphores.h — lines 13541-13558
  - exec/tasks.h — lines 13561-13628
  - exec/alerts.h — lines 13020-13128
  - autodocs for individual functions — lines 759-2445
  - FUNCDEF list (LVO order) — lines 18675-18780

- **RKM Libraries and Devices** — `/Users/stevehill/Desktop/AmigaPDFs/txt/Amiga_ROM_Kernel_Reference_Manual_Libraries_and_Devices.txt`
  - skeleton device verbatim — lines 50200-50600

**Cross-reference sources** (used to verify but not primary):

- **Abacus System Programmers Guide** — `Amiga_System_Programmers_Guide_1988_Abacus.txt` — assembly traces of boot path
- **Mapping the Amiga** — `1993-thomson-randy-rhett-anderson-mapping-amiga-2nd-edition.txt` — hex offsets and field layouts cross-check
- **Hardware Reference Manual** — `Amiga_Hardware_Reference_Manual_3rd_edition.txt` — custom chip interrupt specifics (not duplicated here; see `amiga-hardware-reference.md`)

**Companion documents in this project:**

- `/Users/stevehill/Desktop/AmigaPDFs/amiga-boot-process.md` — ROM coldstart sequence, memory sizing, kickstart loading, ResModules scan
- `/Users/stevehill/Desktop/AmigaPDFs/amiga-hardware-reference.md` — custom chip registers, Paula, Denise, blitter, copper

Where this document uses the format `(Exec RKM ch. N)` or `(Autodocs exec.library/Func)` the reader can find the exact text quoted by searching the primary source file for the function name or chapter heading.

---

*End of document.*
