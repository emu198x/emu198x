# Amiga Exec & DOS API Additions — V37 → V45

**Scope.** This document covers `exec.library` and `dos.library` API additions from Kickstart 1.3 (V34) baseline up through Kickstart 3.9 (V45). It is a supplement to `amiga-exec-kernel.md` and `amiga-dos-filesystem-disk.md`, which cover the V34 baseline from the 2nd-edition ROM Kernel Manuals. Only functions that are *new* in V36/V37/V39/V40/V45 are documented here. Pre-existing V34 functions are not repeated, except where their semantics changed across versions.

**Structure layouts** (MemPool, StackSwapStruct, NotifyRequest, AnchorPath, ExAllControl, DateTime, RDArgs, CSource, DevProc, DosList, Segment, RecordLock, AVLNode, LocalVar, etc.) live in `amiga-headers-reference.md` and are not reproduced here.

**Resources** (cia.resource, disk.resource, battclock.resource, misc.resource, potgo.resource, card.resource) are covered by a separate document.

**Sources.** Function autodocs are quoted verbatim from NDK 3.9:
- `NDK_3.9/Documentation/Autodocs/exec.doc` (V45 exec.library autodoc)
- `NDK_3.9/Documentation/Autodocs/dos.doc` (V45 dos.library autodoc)
- LVO offsets from `NDK_3.9/Include/fd/exec_lib.fd` and `dos_lib.fd`

Per convention, each quoted autodoc section omits `SEE ALSO` (the cross-references are preserved by the function groupings in this document) and `EXAMPLE` blocks where they're long. `NAME`, `SYNOPSIS`, `FUNCTION`, `INPUTS`, `RESULT(S)`, `NOTE(S)`, and `BUGS` are reproduced as-is.

---

## Table of contents

**Exec V36+ additions**

1. Memory allocation — AllocVec, FreeVec, pool API, AvailMem flags
2. Task management — StackSwap, task AttnFlags notes
3. Messaging / I/O wrappers — CreateMsgPort, DeleteMsgPort, CreateIORequest, DeleteIORequest
4. Semaphores — ObtainSemaphoreShared, AttemptSemaphoreShared
5. Cache control — CacheClearU, CacheClearE, CacheControl, CachePreDMA, CachePostDMA
6. System control — ColdReboot; GetCC; super/user state clarifications
7. Interrupt helpers — ObtainQuickVector; AddMemHandler, RemMemHandler
8. Memory movement — CopyMem, CopyMemQuick
9. V39 / V40 / V45 additions — NewMinList, AVL tree support

**DOS V36+ additions**

10. The V37 C rewrite
11. Process creation — CreateNewProc / CreateNewProcTags
12. System (Execute replacement) — SystemTagList / SystemTags
13. Argument parsing — ReadArgs, FreeArgs, ReadItem, StrToLong, FindArg
14. Variadic printf family — VPrintf, VFPrintf, Printf, FPrintf, VFWritef, FWritef
15. File operations — OpenFromLock, ParentOfFH, ExamineFH, DupLockFromFH, SetFileDate, NameFromLock, NameFromFH, ChangeMode, SetFileSize, FGetC, FPutC, UnGetC, FGets, FPuts, Flush, SetVBuf, SetMode, SelectInput/Output, SetConsoleTask, GetConsoleTask, GetFileSysTask, SetFileSysTask
16. Directory operations — ExAll, ExAllEnd, ExAllControl
17. Notifications — StartNotify, EndNotify
18. Record locking — LockRecord, LockRecords, UnLockRecord, UnLockRecords
19. DosList walking — LockDosList, UnLockDosList, AttemptLockDosList, NextDosEntry, FindDosEntry, AddDosEntry, RemDosEntry, MakeDosEntry, FreeDosEntry, IsFileSystem
20. Assigns — AssignLock, AssignLate, AssignPath, AssignAdd, RemAssignList
21. Links — MakeLink, ReadLink, hard vs soft
22. Date / time — DateToStr, StrToDate, CompareDates
23. Local variables — GetVar, SetVar, DeleteVar, FindVar
24. Pattern matching — ParsePattern, ParsePatternNoCase, MatchPattern, MatchPatternNoCase, MatchFirst, MatchNext, MatchEnd
25. Miscellaneous — FilePart, PathPart, AddPart, SplitName, SameLock, SameDevice, Fault, PrintFault, ErrorReport, packet-level (AbortPkt, SendPkt, DoPkt, WaitPkt, ReplyPkt)
26. V39+ additions — NewLoadSeg, InternalLoadSeg, InternalUnLoadSeg, SetOwner, Relabel/Inhibit notes
27. V40/V45 additions

**Appendices**

A. Exec V36+ LVO table
B. DOS V36+ LVO table
C. Source map and gaps

---

# Exec V36+ additions

## 1. Memory allocation

### AllocVec (V36)

`(exec.doc V45, /AllocVec)`

```
   NAME
        AllocVec -- allocate memory and keep track of the size  (V36)

   SYNOPSIS
        memoryBlock = AllocVec(byteSize, attributes)
        D0                     D0         D1

        void *AllocVec(ULONG, ULONG);

   FUNCTION
        This function works identically to AllocMem(), but tracks the size
        of the allocation.

        See the AllocMem() documentation for details.

   WARNING
        The result of any memory allocation MUST be checked, and a viable
        error handling path taken.  ANY allocation may fail if memory has
        been filled.
```

**LVO:** -684 ($2AC).

### FreeVec (V36)

`(exec.doc V45, /FreeVec)`

```
   NAME
        FreeVec -- return AllocVec() memory to the system  (V36)

   SYNOPSIS
        FreeVec(memoryBlock)
                A1

        void FreeVec(void *);

   FUNCTION
        Free an allocation made by the AllocVec() call.  The memory will
        be returned to the system pool from which it came.

   NOTE
        If a block of memory is freed twice, the system will Guru. The
        Alert is AN_FreeTwice ($01000009).   If you pass the wrong pointer,
        you will probably see AN_MemCorrupt $01000005.  Future versions may
        add more sanity checks to the memory lists.

   INPUTS
        memoryBlock - pointer to the memory block to free, or NULL.
```

**LVO:** -690 ($2B2).

### CreatePool (V39)

`(exec.doc V45, /CreatePool)`

```
    NAME
        CreatePool -- Generate a private memory pool header (V39)

    SYNOPSIS
        newPool=CreatePool(memFlags,puddleSize,threshSize)
        a0                 d0       d1         d2

        void *CreatePool(ULONG,ULONG,ULONG);

    FUNCTION
        Allocate and prepare a new memory pool header.  Each pool is a
        separate tracking system for memory of a specific type.  Any number
        of pools may exist in the system.

        Pools automatically expand and shrink based on demand.  Fixed sized
        "puddles" are allocated by the pool manager when more total memory
        is needed.  Many small allocations can fit in a single puddle.
        Allocations larger than the threshSize are allocation in their own
        puddles.

        At any time individual allocations may be freed.  Or, the entire
        pool may be removed in a single step.

    INPUTS
        memFlags - a memory flags specifier, as taken by AllocMem.
        puddleSize - the size of Puddles...
        threshSize - the largest allocation that goes into normal puddles
                     This *MUST* be less than or equal to puddleSize
                     (CreatePool() will fail if it is not)

    RESULT
        The address of a new pool header, or NULL for error.
```

**LVO:** -696 ($2B8).

### DeletePool (V39)

`(exec.doc V45, /DeletePool)`

```
    NAME
        DeletePool --  Drain an entire memory pool (V39)

    SYNOPSIS
        DeletePool(poolHeader)
                   a0

        void DeletePool(void *);

    FUNCTION
        Frees all memory in all pudles of the specified pool header, then
        deletes the pool header.  Individual free calls are not needed.

    INPUTS
        poolHeader - as returned by CreatePool().
```

**LVO:** -702 ($2BE).

### AllocPooled (V39)

`(exec.doc V45, /AllocPooled)`

```
    NAME
        AllocPooled -- Allocate memory with the pool manager (V39)

    SYNOPSIS
        memory=AllocPooled(poolHeader,memSize)
        d0                 a0         d0

        void *AllocPooled(void *,ULONG);

    FUNCTION
        Allocate memSize bytes of memory, and return a pointer. NULL is
        returned if the allocation fails.

        Doing a DeletePool() on the pool will free all of the puddles
        and thus all of the allocations done with AllocPooled() in that
        pool.  (No need to FreePooled() each allocation)

    INPUTS
        memSize - the number of bytes to allocate
        poolHeader - a specific private pool header.

    RESULT
        A pointer to the memory, or NULL.
        The memory block returned is long word aligned.

    NOTES
        The pool function do not protect an individual pool from
        multiple accesses.  The reason is that in most cases the pools
        will be used by a single task.  If your pool is going to
        be used by more than one task you must Semaphore protect
        the pool from having more than one task trying to allocate
        within the same pool at the same time.  Warning:  Forbid()
        protection *will not work* in the future.  *Do NOT* assume
        that we will be able to make it work in the future.  AllocPooled()
        may well break a Forbid() and as such can only be protected
        by a semaphore.
```

**LVO:** -708 ($2C4).

### FreePooled (V39)

`(exec.doc V45, /FreePooled)`

```
    NAME
        FreePooled -- Free pooled memory  (V39)

    SYNOPSIS
        FreePooled(poolHeader,memory,memSize)
                   a0         a1     d0

        void FreePooled(void *,void *,ULONG);

    FUNCTION
        Deallocates memory allocated by AllocPooled().  The size of the
        allocation *MUST* match the size given to AllocPooled().
        The reason the pool functions do not track individual allocation
        sizes is because many of the uses of pools have small allocation
        sizes and the tracking of the size would be a large overhead.

        Only memory allocated by AllocPooled() may be freed with this
        function!

        Doing a DeletePool() on the pool will free all of the puddles
        and thus all of the allocations done with AllocPooled() in that
        pool.  (No need to FreePooled() each allocation)

    INPUTS
        memory - pointer to memory allocated by AllocPooled.
        poolHeader - a specific private pool header.

    NOTES
        [same concurrency warning as AllocPooled; see above]
```

**LVO:** -714 ($2CA).

#### AllocVecPooled / FreeVecPooled — amiga.lib pattern

There is no `exec.library` entry for a size-tracking pool allocator; however, the `FreePooled` autodoc provides the official assembly recipe for layering `AllocVecPooled` / `FreeVecPooled` on top of `AllocPooled` / `FreePooled`. Reproduced from `(exec.doc V45, /FreePooled)`:

```
   ; Function to do AllocVecPooled(Pool,memSize)
   ;
   AllocVecPooled: addq.l  #4,d0           ; Get space for tracking
                   move.l  d0,-(sp)        ; Save the size
                   jsr     _LVOAllocPooled(a6)     ; Call pool...
                   move.l  (sp)+,d1        ; Get size back...
                   tst.l   d0              ; Check for error
                   beq.s   avp_fail        ; If NULL, failed!
                   move.l  d0,a0           ; Get pointer...
                   move.l  d1,(a0)+        ; Store size
                   move.l  a0,d0           ; Get result
   avp_fail:       rts                     ; return

   ; Function to do FreeVecPooled(pool,memory)
   ;
   FreeVecPooled:  move.l  -(a1),d0        ; Get size / adjust pointer
                   jmp     _LVOFreePooled(a6)
```

### AvailMem new flags (V36, V39)

`AvailMem()` is V34 baseline — but the meaning of the returned value and the flag bits changed from V34 to V36 to V39. The V45 autodoc documents the additions.

`(exec.doc V45, /AvailMem)`

```
   FUNCTION
        This function returns the amount of free memory given certain
        attributes.

        To find out what the largest block of a particular type is, add
        MEMF_LARGEST into the requirements argument.  Returning the largest
        block is a slow operation.

   WARNING
        Due to the effect of multitasking, the value returned may not
        actually be the amount of free memory available at that instant.

   INPUTS
        requirements - a requirements mask as specified in AllocMem.  Any
                       of the AllocMem bits are valid, as is MEMF_LARGEST
                       which returns the size of the largest block matching
                       the requirements.

   NOTE
        For V36 Exec, AvailMem(MEMF_LARGEST) does a consistency check on
        the memory list.  Alert AN_MemoryInsane will be pulled if any mismatch
        is noted.
```

AllocMem / AvailMem flag additions (from `exec.doc V45, /AllocMem`):

| Flag | Added | Meaning |
|------|-------|---------|
| `MEMF_LOCAL` | V36 | Motherboard-only RAM that survives CPU RESET. Automatically set in V36. Pre-V36 has no such memory type — allocation fails. |
| `MEMF_24BITDMA` | V36 | Memory within 24-bit DMA reach (Zorro II). Automatically set in V36. Fails pre-V36. |
| `MEMF_REVERSE` | V36 | Allocates from the top of the pool (highest address first). Buggy in pre-V39. |
| `MEMF_KICK` | V39 | Memory reachable by Exec during KickMem/KickTag processing. Set automatically in V39. Fails pre-V39. Do **not** add memory with this flag set — Exec sets it as needed. |
| `MEMF_NO_EXPUNGE` | V39 | Prevents library expunge on allocation failure. Ignored in V37. |

### AllocEntry / FreeEntry — V34 with V37 fix

`AllocEntry` and `FreeEntry` already existed in V34, but V37 fixed a backout bug.

`(exec.doc V45, /AllocEntry)`

```
   NAME
        AllocEntry -- allocate many regions of memory

   SYNOPSIS
        memList = AllocEntry(memList)
        D0                   A0

        struct MemList *AllocEntry(struct MemList *);

   FUNCTION
        This function takes a memList structure and allocates enough memory
        to hold the required memory as well as a MemList structure to keep
        track of it.

        These MemList structures may be linked together in a task control
        block to keep track of the total memory usage of this task. (See
        the description of TC_MEMENTRY under RemTask).

   INPUTS
        memList -- A MemList structure filled in with MemEntry structures.

   RESULTS
        memList -- A different MemList filled in with the actual memory
            allocated in the me_Addr field, and their sizes in me_Length.
            If enough memory cannot be obtained, then the requirements of
            the allocation that failed is returned and bit 31 is set.

            WARNING: The result is unusual!  Bit 31 indicates failure.

   BUGS
        If any one of the allocations fails, this function fails to back
        out fully.  This is fixed by the "SetPatch" program on V1.3
        Workbench disks.
```

`(exec.doc V45, /FreeEntry)`

```
   NAME
        FreeEntry -- free many regions of memory

   SYNOPSIS
        FreeEntry(memList)
                  A0
        void FreeEntry(struct MemList *);

   FUNCTION
        This function takes a memList structure (as returned by AllocEntry)
        and frees all the entries.

   INPUTS
        memList -- pointer to structure filled in with MemEntry
                   structures
```

**LVOs:** AllocEntry -222, FreeEntry -228.

### AllocAbs (existing) — deprecated usage pattern

`AllocAbs()` still exists, but in V36+ application code has almost no legitimate reason to use it. It was historically used for two purposes: (1) poking modules into the KickMem/KickTag area at boot time (still valid — see `SumKickData`), and (2) claiming specific hardware-aperture memory such as frame-buffer regions. Both are now serviced by better mechanisms (CopyMem into a properly allocated buffer, or the graphics library). Retain it only when you genuinely need a fixed physical address, and be prepared for failure on systems where that address is already in use.

---

## 2. Task management additions

### StackSwap (V37)

`(exec.doc V45, /StackSwap)`

```
   NAME
        StackSwap - EXEC supported method of replacing task's stack      (V37)

   SYNOPSIS
        StackSwap(newStack)
                  A0

        VOID StackSwap(struct StackSwapStruct *);

   FUNCTION
        This function will, in an EXEC supported manner, swap the
        stack of your task with the given values in StackSwap.
        The StackSwapStruct structure will then contain the values
        of the old stack such that the old stack can be restored.
        This function is new in V37.

   NOTE
        If you do a stack swap, only the new stack is set up.
        This function does not copy the stack or do anything else
        other than set up the new stack for the task.  It is
        generally required that you restore your stack before
        exiting.

   INPUTS
        newStack - A structure that contains the values for the
                new upper and lower stack bounds and the new stack
                pointer.  This structure will have its values
                replaced by those in you task such that you can
                restore the stack later.

   RESULTS
        newStack - The structure will now contain the old stack.
                This means that StackSwap(foo); StackSwap(foo);
                will effectively do nothing.
```

**LVO:** -732 ($2DC).

The companion `struct StackSwapStruct` is in `exec/tasks.h` and is defined in the headers reference document.

### The child-task protocol — ChildFree / ChildOrphan / ChildStatus / ChildWait

These are **not** in `exec.library`. They are part of `amiga.lib` (the user-mode support library) and layer on top of exec's Task primitives. Because they are linker-resolved rather than library-vector-resolved, they have no LVO and no autodoc under `exec.library/`. The intended protocol is: a parent calls `CreateNewProcTags()` or `CreateTask()`, tracking the child through `Task->tc_UserData` or a parent-side handle; the child marks itself as a child via `amiga.lib/AddTask()` with a termination cleanup routine; the parent can poll `ChildStatus()`, block in `ChildWait()`, detach with `ChildOrphan()`, or forcibly free with `ChildFree()`. In practice most V37+ code instead uses `CreateNewProcTags()` with `NP_NotifyOnDeath` / `NP_Synchronous` (see section 11 below) which provides a cleaner message-based death-notification protocol.

### FindTask(NULL) behaviour clarification (V34+, confirmed V36)

`(exec.doc V45, /FindTask)`

```
   FUNCTION
        This function will check all task queues for a task with the given
        name, and return a pointer to its task control block.  If a NULL
        name pointer is given a pointer to the current task will be
        returned.

        Finding oneself with a NULL for the name is very quick.  Finding a
        task by name is very system expensive, and will disable interrupts
        for a long time.  Since a task may remove itself at any time,
        a Forbid()/Permit() pair may be needed to ensure the pointer
        returned by FindTask() is still valid when used.
```

No new LVO. The V37 autodoc clarifies the need for Forbid/Permit around named lookups — this was a pre-V36 pitfall and remains important.

### SetTaskPri — V34, same LVO

No behavioural change beyond V34. Retained here only for completeness of the task-related group.

### Task AttnFlags

The V37 `struct Task` retained the V34 AttnFlag bits (`TF_PROCTIME`, `TF_ETASK`, `TF_STACKCHK`, `TF_EXCEPT`, `TF_SWITCH`, `TF_LAUNCH`). V45 adds no new bits. See `exec/tasks.h` in the headers reference document.

### RemTask (V36 fix)

`(exec.doc V45, /RemTask)`

```
   BUGS
        Before V36 if RemTask() was called on a task other than the current
        task, and that task was created with amiga.lib/CreateTask, there was
        a slight chance of a crash.  The problem can be hidden by bracketing
        RemTask() with Forbid()/Permit().
```

No new LVO.

---

## 3. Messaging and I/O wrappers

### CreateMsgPort (V36)

`(exec.doc V45, /CreateMsgPort)`

```
   NAME
        CreateMsgPort - Allocate and initialize a new message port  (V36)

   SYNOPSIS
        CreateMsgPort()

        struct MsgPort * CreateMsgPort(void);

   FUNCTION
        Allocates and initializes a new message port.  The message list
        of the new port will be prepared for use (via NewList).  A signal
        bit will be allocated, and the port will be set to signal your
        task when a message arrives (PA_SIGNAL).

        You *must* use DeleteMsgPort() to delete ports created with
        CreateMsgPort()!

   RESULT
        MsgPort - A new MsgPort structure ready for use, or NULL if out of
                memory or signals.  If you wish to add this port to the public
                port list, fill in the ln_Name and ln_Pri fields, then call
                AddPort().  Don't forget RemPort()!
```

**LVO:** -666 ($29A).

Replaces `amiga.lib/CreatePort()`. The Exec-library version is preferred because it is a shared ROM routine — it does not bloat every linked binary.

### DeleteMsgPort (V36)

`(exec.doc V45, /DeleteMsgPort)`

```
   NAME
        DeleteMsgPort - Free a message port created by CreateMsgPort  (V36)

   SYNOPSIS
        DeleteMsgPort(msgPort)
                      a0

        void DeleteMsgPort(struct MsgPort *);

   FUNCTION
        Frees a message port created by CreateMsgPort().  All messages that
        may have been attached to this port must have already been
        replied to.

   INPUTS
        msgPort - A message port.  NULL for no action.
```

**LVO:** -672 ($2A0).

### CreateIORequest (V36)

`(exec.doc V45, /CreateIORequest)`

```
   NAME
        CreateIORequest() -- create an IORequest structure  (V36)

   SYNOPSIS
        ioReq = CreateIORequest( ioReplyPort, size );
                                 A0           D0

        struct IORequest *CreateIORequest(struct MsgPort *, ULONG);

   FUNCTION
        Allocates memory for and initializes a new IO request block
        of a user-specified number of bytes.  The number of bytes
        must be at least as large as a "struct Message".

   INPUTS
        ioReplyPort - Pointer to a port for replies (an initialized message
                port, as created by CreateMsgPort() ).  If NULL, this
                function fails.
        size - the size of the IO request to be created.

   RESULT
        ioReq - A pointer to the new IORequest block, or NULL.
```

**LVO:** -654 ($28E).

Replaces `amiga.lib/CreateExtIO()`.

### DeleteIORequest (V36)

`(exec.doc V45, /DeleteIORequest)`

```
   NAME
        DeleteIORequest() - Free a request made by CreateIORequest()  (V36)

   SYNOPSIS
        DeleteIORequest( ioReq );
                         a0

        void DeleteIORequest(struct IORequest *);

   FUNCTION
        Frees up an IO request as allocated by CreateIORequest().

   INPUTS
        ioReq - A pointer to the IORequest block to be freed, or NULL.
                This function uses the mn_Length field to determine how
                much memory to free.
```

**LVO:** -660 ($294).

### GetMsg — V34, no API change

`GetMsg` is V34 baseline. No version change from V34 to V45 other than the general V36 clarification that ports created with `CreateMsgPort()` do not need to be added to the public port list unless you also want `FindPort` to find them.

### CloseDevice / CloseLibrary — V36 robustness

`(exec.doc V45, /CloseDevice)`: "Starting with V36 exec it is safe to `CloseDevice()` with an IORequest that is either cleared to zeros, or failed to open."

`(exec.doc V45, /CloseLibrary)`: "Starting with V36, it is safe to pass a NULL instead of a library pointer."

Both changes let unwind/cleanup code be branchless.

### OpenDevice / OpenLibrary — V36 task-safety

`(exec.doc V45, /OpenDevice)`:

```
   BUGS
        Prior to V36, tasks could not make OpenDevice calls requiring disk
        access (since tasks are not allowed to make dos.library calls).
        Now OpenDevice is protected from tasks.
```

Same applies to `OpenLibrary`. Both still pop DOS requesters unless `pr_WindowPtr` is set to -1 — that requires a Process, not a plain Task.

---

## 4. Semaphores — shared locking

### ObtainSemaphoreShared (V36)

`(exec.doc V45, /ObtainSemaphoreShared)`

```
    NAME
        ObtainSemaphoreShared -- gain shared access to a semaphore (V36)

    SYNOPSIS
        ObtainSemaphoreShared(signalSemaphore)
                              a0

        void ObtainSemaphoreShared(struct SignalSemaphore *);

    FUNCTION
        A lock on a signal semaphore may either be exclusive, or shared.
        Exclusive locks are granted by the ObtainSemaphore() and
        AttemptSemaphore() functions.  Shared locks are granted by
        ObtainSemaphoreShared().  Calls may be nested.

        Any number of tasks may simultaneously hold a shared lock on a
        semaphore.  Only one task may hold an exclusive lock.  A typical
        application is a list that is often read, but only occasionally
        written to.

        Any exlusive locker will be held off until all shared lockers
        release the semaphore.  Likewise, if an exlusive lock is held,
        all potential shared lockers will block until the exclusive lock
        is released.  All shared lockers are restarted at the same time.

    NOTES
        While this function was added for V36, the feature magically works
        with all older semaphore structures.

        A task owning a shared lock must not attempt to get an exclusive
        lock on the same semaphore.

        Starting in V39, if the caller already has an exclusive lock on the
        semaphore it will return with another nesting of the lock.  Pre-V39
        this would cause a deadlock.  For pre-V39 use, you can use the
        following workaround:

            /* Try to get the shared semaphore */
            if (!AttemptSemaphoreShared(ss))
            {
                /* Check if we can get the exclusive version */
                if (!AttemptSemaphore(ss))
                {
                    /* Oh well, wait for the shared lock */
                    ObtainSemaphoreShared(ss));
                }
            }
            :
            ReleaseSemaphore(ss);

    NOTE
        This call is guaranteed to preserve all registers, starting with
        V37 exec.
```

**LVO:** -678 ($2A6).

### AttemptSemaphoreShared (V37)

`(exec.doc V45, /AttemptSemaphoreShared)`

```
   NAME
        AttemptSemaphoreShared -- try to obtain without blocking       (V37)

   SYNOPSIS
        success = AttemptSemaphoreShared(signalSemaphore)
        D0                               A0

        LONG AttemptSemaphoreShared(struct SignalSemaphore *);

   FUNCTION
        This call is similar to ObtainSemaphoreShared(), except that it
        will not block if the semaphore could not be locked.

   INPUT
       signalSemaphore -- an initialized signal semaphore structure

   RESULT
        success -- TRUE if the semaphore was granted, false if some
            other task already possessed the semaphore in exclusive mode.

   NOTE
        This call does NOT preserve registers.

        Starting in V39 this call will grant the semaphore if the
        caller is already the owner of an exclusive lock on the semaphore.
        In pre-V39 systems this would not be the case.
```

**LVO:** -720 ($2D0).

### Shared vs exclusive — when to use which

- **Exclusive** (`ObtainSemaphore`, `AttemptSemaphore`): for mutators. Only one holder at a time. Nesting by the same task is allowed.
- **Shared** (`ObtainSemaphoreShared`, `AttemptSemaphoreShared`): for readers. Multiple shared holders are allowed concurrently, but a pending exclusive request blocks new shared acquisitions.
- A task that holds a shared lock **must not** upgrade to exclusive on the same semaphore — that path is a deadlock prior to V39 and still discouraged in V39+.
- In V37, `ObtainSemaphore()` began preserving A0; pre-V37 it could trash A0 (`/ObtainSemaphore` BUGS section).

### Procure / Vacate — bid-based semaphores (V39)

`Procure` and `Vacate` were kept from V34 but were marked V39 in the V45 `exec_lib.fd`. These are heavier-weight, fully general message-based semaphores used when you cannot guarantee that only one task owns a lock attempt at a time. Prefer SignalSemaphores (`ObtainSemaphore` / `ReleaseSemaphore` / `ObtainSemaphoreShared`) for normal work — they are faster and simpler.

---

## 5. Cache control

Starting with 68020/68030-equipped Amigas (A2500, A3000, A2620/A2630 accelerators), the CPU had on-chip instruction and data caches. On 68040+ systems the data cache added copyback mode. Any operation that writes instructions to memory, or any DMA that reads from or writes to RAM, has to cooperate with the caches — otherwise the CPU runs stale code or the DMA device sees stale data. Exec V37 introduced a full cache-management API.

### CacheClearU (V37)

`(exec.doc V45, /CacheClearU)`

```
   NAME
        CacheClearU - User callable simple cache clearing (V37)

   SYNOPSIS
        CacheClearU()

        void CacheClearU(void);

   FUNCTION
        Flush out the contents of any CPU instruction and data caches.
        If dirty data cache lines are present, push them to memory first.

        Caches must be cleared after *any* operation that could cause
        invalid or stale data.  The most common cases are DMA and modifying
        instructions using the processor.  See the CacheClearE() autodoc
        for a more complete description.

        Some examples of when the cache needs clearing:
                Self modifying code
                Building Jump tables
                Run-time code patches
                Relocating code for use at different addresses.
                Loading code from disk
```

**LVO:** -636 ($27C).

### CacheClearE (V37)

`(exec.doc V45, /CacheClearE)`

```
   NAME
        CacheClearE - Cache clearing with extended control (V37)

   SYNOPSIS
        CacheClearE(address,length,caches)
                    a0      d0     d1

        void CacheClearE(APTR,ULONG,ULONG);

   FUNCTION
        Flush out the contents of the CPU instruction and/or data caches.
        If dirty data cache lines are present, push them to memory first.

        Motorola CPUs have separate instruction and data caches.  A data
        write does not update the instruction cache.  If an instruction is
        written to memory or modified, the old instruction may still exist
        in the cache.  Before attempting to execute the code, a flush of
        the instruction cache is required.

        For most systems, the data cache is not updated by Direct Memory
        Access (DMA), or if some external factor changes shared memory.

        Caches must be cleared after *any* operation that could cause
        invalid or stale data.

   INPUTS
        address - Address to start the operation.  This may be rounded
                  due to hardware granularity.
        length  - Length of area to be cleared, or $FFFFFFFF to indicate all
                  addresses should be cleared.
        caches  - Bit flags to indicate what caches to affect.  The current
                  supported flags are:
                        CACRF_ClearI    ;Clear instruction cache
                        CACRF_ClearD    ;Clear data cache
                  All other bits are reserved for future definition.

   NOTES
        On systems with a copyback mode cache, any dirty data is pushed
        to memory as a part of this operation.

        Regardless of the length given, the function will determine the most
        efficient way to implement the operation.  For some cache systems,
        including the 68030, the overhead partially clearing a cache is often
        too great.  The entire cache may be cleared.

        For all current Amiga models, Chip memory is set with Instruction
        caching enabled, data caching disabled.  This prevents coherency
        conflicts with the blitter or other custom chip DMA.  Custom chip
        registers are marked as non-cacheable by the hardware.

        The system takes care of appropriately flushing the caches for normal
        operations.  The instruction cache is cleared by all calls that
        modify instructions, including LoadSeg(), MakeLibrary() and
        SetFunction().
```

**LVO:** -642 ($282).

### CacheControl (V37)

`(exec.doc V45, /CacheControl)`

```
   NAME
        CacheControl - Instruction & data cache control

   SYNOPSIS
        oldBits = CacheControl(cacheBits,cacheMask)
        D0                     D0        D1

        ULONG CacheControl(ULONG,ULONG);

   FUNCTION
        This function provides global control of any instruction or data
        caches that may be connected to the system.  All settings are
        global -- per task control is not provided.

        The action taken by this function will depend on the type of
        CPU installed.  This function may be patched to support external
        caches, or different cache architectures.  In all cases the function
        will attempt to best emulate the provided settings.

        The list of supported settings is provided in the exec/execbase.i
        include file.  The bits currently defined map directly to the Motorola
        68030 CPU CACR register.  Alternate cache solutions may patch into
        the Exec cache functions.

   INPUTS
        cacheBits - new values for the bits specified in cacheMask.
        cacheMask - a mask with ones for all bits to be changed.

   RESULT
        oldBits   - the complete prior values for all settings.

   NOTE
        As a side effect, this function clears all caches.
```

**LVO:** -648 ($288).

### CachePreDMA (V37)

`(exec.doc V45, /CachePreDMA)`

```
   NAME
        CachePreDMA - Take actions prior to hardware DMA  (V37)

   SYNOPSIS
        paddress = CachePreDMA(vaddress,&length,flags)
        d0                     a0       a1      d0

        APTR CachePreDMA(APTR,LONG *,ULONG);

   FUNCTION
        Take all appropriate steps before Direct Memory Access (DMA).  This
        function is primarily intended for writers of DMA device drivers.

        This function supports advanced cache architectures that have
        "copyback" modes.  With copyback, write data may be cached, but not
        actually flushed out to memory.  If the CPU has unflushed data at the
        time of DMA, data may be lost.

        As implemented
                68000 - Do nothing
                68010 - Do nothing
                68020 - Do nothing
                68030 - Do nothing
                68040 - Write any matching dirty cache lines back to memory.
                        As a side effect of the 68040's design, matching data
                        cache lines are also invalidated -- future CPUs may
                        be different.

   INPUTS
        address - Base address to start the action.
        length  - Pointer to a longword with a length.
        flags   - Values:
                        DMA_Continue - Indicates this call is to complete
                        a prior request that was broken up.

                        DMA_ReadFromRAM - Indicates that this DMA is a
                        read from RAM to the DMA device (ie - a write
                        to the hard drive)  This flag is not required
                        but if used must match in both the PreDMA and
                        PostDMA calls.

   RESULTS
        paddress- Physical address that corresponds to the input virtual
                  address.
        &length - This length value will be updated to reflect the contiguous
                  length of physical memory present at paddress.  This may
                  be smaller than the requested length.  To get the mapping
                  for the next chunk of memory, call the function again with
                  a new address, length, and the DMA_Continue flag.

   NOTE
        Due to processor granularity, areas outside of the address range
        may be affected by the cache flushing actions.
```

**LVO:** -762 ($2FA).

### CachePostDMA (V37)

`(exec.doc V45, /CachePostDMA)`

```
   NAME
        CachePostDMA - Take actions after to hardware DMA  (V37)

   SYNOPSIS
        CachePostDMA(vaddress,&length,flags)
                     a0       a1      d0

        CachePostDMA(APTR,LONG *,ULONG);

   FUNCTION
        Take all appropriate steps after Direct Memory Access (DMA).

        As implemented
                68000 - Do nothing
                68010 - Do nothing
                68020 - Do nothing
                68030 - Flush the data cache
                68040 - Flush matching areas of the data cache

   INPUTS
        address - Same as initially passed to CachePreDMA
        length  - Same as initially passed to CachePreDMA
        flags   - Values:
                        DMA_NoModify - If the area was not modified (and
                        thus there is no reason to flush the cache) set
                        this bit.

                        DMA_ReadFromRAM - Indicates that this DMA is a
                        read from RAM to the DMA device.
```

**LVO:** -768 ($300).

---

## 6. System control

### ColdReboot (V36)

`(exec.doc V45, /ColdReboot)`

```
    NAME
        ColdReboot - reboot the Amiga (V36)

    SYNOPSIS
        ColdReboot()

        void ColdReboot(void);

    FUNCTION
        Reboot the machine.  All external memory and periperals will be
        RESET, and the machine will start its power up diagnostics.

        This function never returns.
```

**LVO:** -726 ($2D6).

### GetCC (V36, per .fd)

`(exec.doc V45, /GetCC)`

```
   NAME
        GetCC -- get condition codes in a 68010 compatible way.

   SYNOPSIS
        conditions = GetCC()
          D0

        UWORD GetCC(void);

   FUNCTION
        The 68000 processor has a "MOVE SR,<ea>" instruction which gets a
        copy of the processor condition codes.

        On the 68010,20 and 30 CPUs, "MOVE SR,<ea>" is privileged.  User
        code will trap if it is attempted.  These processors need to use
        the "MOVE CCR,<ea>" instruction instead.

        This function provides a means of obtaining the CPU condition codes
        in a manner that will make upgrades transparent.  This function is
        VERY short and quick.

   RESULTS
        conditions - the 680XX condition codes

    NOTE
        This call is guaranteed to preserve all registers.  This function
        may be implemented as code right in the jump table.
```

**LVO:** -528 ($210). (Present in V34 jump table but cited here because portable use of condition-code access only became useful in a CPU-agnostic way from V36 onward, once the V33/V34 bugs were fixed.)

### SetSR / SuperState / UserState / Supervisor — existing functions, clarifications

- `SetSR` (LVO -144): set or read the status register via a mask. The V45 autodoc text matches V34.
- `SuperState` (LVO -150): enter supervisor mode while continuing to run on the user stack. Returns the system stack pointer — save this for the paired `UserState()`.
- `UserState` (LVO -156): return to user mode. **Bug note** (from `/UserState` V45): "This function is broken in V33/34 Kickstart. Fixed in V1.31 setpatch." So under the V34 baseline you must rely on setpatch; under V36+ it just works.
- `Supervisor` (LVO -30): execute a short assembly function in supervisor mode. The routine must end in `RTE`. Still the only portable way to read a privileged register such as VBR on a 68010+.

### SumKickData — V34 existing, important for Kickstart delta list

`(exec.doc V45, /SumKickData)` NOTE: "SumKickData was introduced in the 1.2 release." Present in the V34 baseline. Reproduced here because the surrounding Kickstart-delta mechanism (`KickMemPtr`, `KickTagPtr`, `KickCheckSum` in `ExecBase`) is the sanctioned way to patch modules into ROM at reboot time. In V39+, writes to `KickCheckSum` should be followed by `CacheClearU()` to avoid copyback cache issues.

---

## 7. Interrupt helpers

### ObtainQuickVector (V39)

`(exec.doc V45, /ObtainQuickVector)`

```
   NAME
        Function to obtain an install a Quick Interrupt vector            (V39)

   SYNOPSIS
        vector=ObtainQuickVector(interruptCode)
        d0                       a0

        ULONG ObtainQuickVector(APTR);

   FUNCTION
        This function will install the code pointer into the quick interrupt
        vector it allocates and returns to you the interrupt vector that
        your Quick Interrupt system needs to use.

        This function may also return 0 if no vectors are available.  Your
        hardware should be able to then fall back to using the shared
        interrupt server chain should this happen.

        The interrupt code is a direct connect to the physical interrupt.
        This means that it is the responsibility of your code to do all
        of the context saving/restoring required by interrupt code.

        Also, due to the performance of the interrupt controller, you may
        need to also watch for "false" interrupts.  These are interrupts
        that come in just after a DISABLE.  The reason this happens is
        because the interrupt may have been posted before the DISABLE
        hardware access is completed.

   NOTE
        This function was not implemented fully until V39.  Due to a mis-cue
        it is not safe to call in V37 EXEC.  (Sorry)

   INPUTS
        A pointer to your interrupt code.  This code is not an EXEC interrupt
        but is dirrectly connected to the hardware interrupt.  Thus, the
        interrupt code must not modify any registers and must return via
        an RTE.

   RESULTS
        The 8-bit vector number used for Zorro-III Quick Interrupts
        If it returns 0, no quick interrupt was allocatable.  The device
        should at this point switch to using the shared interrupt server
        method.
```

**LVO:** -786 ($312).

Quick interrupts bypass Exec's shared-server chain — used by high-performance Zorro III drivers to cut latency. Note the required "one last interrupt after DISABLE" guard pattern in the autodoc.

### AddMemHandler (V39)

`(exec.doc V45, /AddMemHandler)`

```
   NAME
        AddMemHandler - Add a low memory handler to exec                 (V39)

   SYNOPSIS
        AddMemHandler(memHandler)
                      A1

        VOID AddMemHandler(struct Interrupt *);

   FUNCTION
        This function adds a low memory handler to the system.  The handler
        is described in the Interrupt structure.  Due to multitasking
        issues, the handler must be ready to run the moment this function
        call is made.  (The handler may be called before the call returns)

   NOTE
        Adding a handler from within a handler will cause undefined
        actions.  It is safe to add a handler to the list while within
        a handler but the newly added handler may or may not be called
        for the specific failure currently running.

   INPUTS
        memHandler - A pointer to a completely filled in Interrupt structure
                     The priority field determine the position of the handler
                     with respect to other handlers in the system.  The higher
                     the priority, the earlier the handler is called.
                     Positive priorities will have the handler called before
                     any of the library expunge vectors are called.  Negative
                     priority handlers will be called after the library
                     expunge routines are called.
                     (Note:  RAMLIB is a handler at priority 0)
```

**LVO:** -774 ($306).

The handler is called on allocation failure, before or after `RemLibrary`/expunge paths (depending on priority). It is passed a `struct MemHandlerData` in `a0`, the `is_Data` value in `a1`, and `ExecBase` in `a6`. It must return one of `MEM_DID_NOTHING`, `MEM_TRY_AGAIN`, or `MEM_ALL_DONE` in `d0`, and it **must not** break the Forbid state.

### RemMemHandler (V39)

`(exec.doc V45, /RemMemHandler)`

```
   NAME
        RemMemHandler - Remove low memory handler from exec              (V39)

   SYNOPSIS
        RemMemHandler(memHandler)
                      A1

        VOID RemMemHandler(struct Interrupt *);

   FUNCTION
        This function removes the low memory handler from the system.
        This function can be called from within a handler.  If removing
        oneself, it is important that the handler returns MEM_ALL_DONE.

   NOTE
        When removing a handler, the handler may be called until this
        function returns.  Thus, the handler must still be valid until
        then.

   INPUTS
        memHandler - Pointer to a handler added with AddMemHandler()
```

**LVO:** -780 ($30C).

### Cause — V34 baseline, no API change

`Cause()` is V34 baseline for causing a software interrupt. The V45 autodoc BUGS section still reminds: "Unlike other Interrupts, SoftInts must preserve the value of A6."

---

## 8. Memory movement

### CopyMem (V36)

`(exec.doc V45, /CopyMem)`

```
   NAME
        CopyMem - general purpose memory copy function

   SYNOPSIS
        CopyMem( source, dest, size )
                 A0      A1    D0

        void CopyMem(APTR,APTR,ULONG);

   FUNCTION
        CopyMem is a general purpose, fast memory copy function.  It can
        deal with arbitrary lengths, with its pointers on arbitrary
        alignments.  It attempts to optimize larger copies with more
        efficient copies, it uses byte copies for small moves, parts of
        larger copies, or the entire copy if the source and destination are
        misaligned with respect to each other.

        Arbitrary overlapping copies are not supported.

        The internal implementation of this function will change from
        system to system, and may be implemented via hardware DMA.

   INPUTS
        source - a pointer to the source data region
        dest  - a pointer to the destination data region
        size  - the size (in bytes) of the memory area.  Zero copies
                zero bytes
```

**LVO:** -624 ($270).

### CopyMemQuick (V36)

`(exec.doc V45, /CopyMemQuick)`

```
   NAME
        CopyMemQuick - optimized memory copy function

   SYNOPSIS
        CopyMemQuick( source, dest, size )
                      A0      A1    D0

        void CopyMemQuick(ULONG *,ULONG *,ULONG);

   FUNCTION
        CopyMemQuick is a highly optimized memory copy function, with
        restrictions on the size and alignment of its arguments. Both the
        source and destination pointers must be longword aligned.  In
        addition, the size must be an integral number of longwords (e.g.
        the size must be evenly divisible by four).

        Arbitrary overlapping copies are not supported.

        The internal implementation of this function will change from system
        to system, and may be implemented via hardware DMA.

   INPUTS
        source - a pointer to the source data region, long aligned
        dest -  a pointer to the destination data region, long aligned
        size -  the size (in bytes) of the memory area.  Zero copies
                zero bytes.
```

**LVO:** -630 ($276).

**Neither function supports overlapping copies.** For overlapping regions, use a byte-by-byte move-backwards loop if `dest > source`, or `CopyMem` forward if `dest < source`. The implementation may use DMA or CPU-specific block moves, so you cannot assume the order of the writes from a cache coherency point of view — if you are moving code or DMA buffers, follow up with `CacheClearU()` or `CachePreDMA()` as appropriate.

---

## 9. V39 / V40 / V45 additions

### V39 AVL Tree support (the function slots were reserved; implementation is V45)

The AVL-tree functions occupy LVO slots first reserved in `exec_lib.fd` under `*--- functions in V45 or higher ---`. They provide an O(log n) ordered dictionary, with the caller supplying two comparator hooks (one node-to-node, one node-to-key).

### NewMinList (V45)

No autodoc entry is present under `exec.library/` in `exec.doc V45` — the function is declared in `exec_lib.fd` under `*--- functions in V45 or higher ---` as:

```
NewMinList(minlist)(a0)
```

Its purpose is to initialise a `struct MinList` in place (equivalent to the `NEWLIST` assembly macro and the inline `NewList()` code from `exec/lists.h`), but doing so via a library vector so ROM-shared, patchable. If you target pre-V45 systems, use the inline `NEWLIST` macro instead.

**LVO:** -828 ($33C).

### AVL_AddNode (V45)

`(exec.doc V45, /AVL_AddNode)`

```
    NAME
       AVL_AddNode -- Add node to the tree (V45)

    SYNOPSIS
       result = AVL_AddNode( root, node, func )
       D0                     A0    A1    A2

       struct AVLNode *AVL_AddNode(struct AVLNode **,
                                   struct AVLNode *,
                                   AVLNODECOMP);

    FUNCTION
       The function will add the given node to the AVL tree in the correct
       position. The correct position is determined by the compare function
       which is also passed in and which defines relative value of nodes
       by determining their "key" values and comparing them.
       Note that the compare function works like strcmp() by returning
       <0, 0, >0 results to define a less/equal/greater relationship.
       Note that there is no arbitration for access to the tree. You
       should use a SignalSemaphore if arbitration is required.

    INPUTS
       root  - Address of(!) the root pointer(!) of the AVL tree.
               Initially, the root pointer must be set to NULL, which
               represents an empty AVL tree.
       node  - The node to add
       func  - The compare function to find the right position for
               the node in the tree

    RESULT
       If the node could be added, NULL is returned.
       If there is already a node in the tree that has the same key, the
       pointer to that node be returned and the given node will not be
       added.

    NOTES
       There are a few things to remember about AVL trees. First, they
       are binary balanced trees. You can expect O(log2(n)) performance
       for adding, removing, and searching by key.

       Second, the implementation does not care what kind of compare
       functions you provide to the AVL functions, i.e., what sort order
       you define.

       To work with an AVL tree you need two compare functions:
           AVLNODECOMP - Determines keys of two nodes and compares them
           AVLKEYCOMP  - Compares a node's key to a given key

       The implementation does not compare keys or makes any assumption
       about them. A key can be anything that fits into a 32 bit value,
       even a pointer to the "true" key, whatever it may be.

       Remember that each key in a tree must be unique.

       Finally, the implementation is not recursive and you don't have
       to provide a huge stack even when using AVL functions on
       huge trees.
```

**LVO:** -852 ($354).

### AVL_RemNodeByAddress (V45)

`(exec.doc V45, /AVL_RemNodeByAddress)`

```
    NAME
       AVL_RemNodeByAddress - Remove a given node (V45)

    SYNOPSIS
       result = AVL_RemNodeByAddress( root, node )
       D0                              A0   A1

       struct AVLNode *AVL_RemNodeByAddress(struct AVLNode **,
                                            struct AVLNode *);

    FUNCTION
       The function will remove the given node from the tree.
       Note that there is no arbitration for access to the tree. You
       should use a SignalSemaphore if arbitration is required.

    INPUTS
       root  - Address of(!) the root pointer(!) of the AVL tree
       node  - pointer to the struct AVLNode that should be removed

    RESULT
       A pointer to the removed node.

    NOTES
       The node to be removed *better*be* part of the tree or you
       lose big time!
```

**LVO:** -858 ($35A).

### AVL_RemNodeByKey (V45)

`(exec.doc V45, /AVL_RemNodeByKey)`

```
    NAME
       AVL_RemNodeByKey -- Remove a node identified by its key (V45)

    SYNOPSIS
       result = AVL_RemNodeByKey( root, key, func )
       D0                          A0   A1    A2

       struct AVLNode *AVL_RemNodeByKey(struct AVLNode **,
                                        AVLKey,
                                        AVLKEYCOMP);

    FUNCTION
       The function will search for the node with the given key and
       remove it from the tree.

    INPUTS
       root  - Address of(!) the root pointer(!) of the AVL tree
       key   - An abstract key to match a node by the given compare function
       func  - The compare function

    RESULT
       A pointer to the removed node or NULL if the node could not be found.
```

**LVO:** -864 ($360).

### AVL_FindNode (V45)

`(exec.doc V45, /AVL_FindNode)`

```
    NAME
       AVL_FindNode -- Find a node identified by its key (V45)

    SYNOPSIS
       result = AVL_FindNode( root, key, func )
       D0                      A0   A1    A2

       struct AVLNode *AVL_FindNode(const struct AVLNode *,
                                    AVLKey,
                                    AVLKEYCOMP);

    FUNCTION
       The function will search for the node with the given key and
       return a pointer to it.
       Note that the compare function works like strcmp() by returning
       <0, 0, >0 results to define a less/equal/greater relationship.

    RESULT
       A pointer to the node or NULL if the node could not be found.
```

**LVO:** -870 ($366).

### AVL_FindPrevNodeByAddress (V45)

`(exec.doc V45, /AVL_FindPrevNodeByAddress)`

```
    NAME
       AVL_FindPrevNodeByAddress -- Return previous node of a tree (V45)

    SYNOPSIS
       result = AVL_FindPrevNodeByAddress( node )
       D0                                   A0

       struct AVLNode *AVL_FindPrevNodeByAddress(const struct AVLNode *node)

    FUNCTION
       Given the pointer to a struct AVLNode, this function will return
       the logically previous/lower/smaller entry in the tree
       Using this function, you can start to walk the tree in a "linear"
       fashion.

    NOTES
       The node passed in *better*be* part of the tree or you
       lose big time!
```

**LVO:** -876 ($36C).

### AVL_FindPrevNodeByKey (V45)

`(exec.doc V45, /AVL_FindPrevNodeByKey)`

```
    NAME
       AVL_FindPrevNodeByKey -- Find node identified by a key (V45)

    SYNOPSIS
       result = AVL_FindPrevNodeByKey( root, key, func )
       D0                               A0   A1    A2

    FUNCTION
       The function will search for a node or the next lower node
       based on the key given and return a pointer to it.

    RESULT
       A pointer to the node with the given key or the next lower node
       if no exact match was found.
```

**LVO:** -882 ($372).

### AVL_FindNextNodeByAddress (V45)

`(exec.doc V45, /AVL_FindNextNodeByAddress)`

```
    NAME
       AVL_FindNextNodeByAddress -- Return the next node of a tree (V45)

    SYNOPSIS
       result = AVL_FindNextNodeByAddress( node )
       D0                                   A0

    FUNCTION
       Given the pointer to a struct AVLNode, this function will return
       the logically next/higher/bigger entry in the tree
       Using this function, you can start to walk the tree in a "linear"
       fashion.

    NOTES
       The node passed in *better*be* part of the tree or you
       lose big time!
```

**LVO:** -888 ($378).

### AVL_FindNextNodeByKey (V45)

`(exec.doc V45, /AVL_FindNextNodeByKey)`

```
    NAME
       AVL_FindNextNodeByKey -- Find node identified by a key (V45)

    SYNOPSIS
       result = AVL_FindNextNodeByKey( root, key, func )
       D0                               A0   A1    A2

    FUNCTION
       The function will search for a node or the next higher node
       based on the key given and return a pointer to it.

    RESULT
       A pointer to the node with the given key or the next higher node
       if no exact match was found.
```

**LVO:** -894 ($37E).

### AVL_FindFirstNode (V45)

`(exec.doc V45, /AVL_FindFirstNode)`

```
    NAME
       AVL_FindFirstNode -- return the lowest/smallest node (V45)

    SYNOPSIS
       result = AVL_FindFirstNode( root )
       D0                           A0

    FUNCTION
       This functions will return the pointer to the first node in
       the given AVL tree. Using this function, you can start to
       walk the tree in a "linear" fashion.

    RESULT
       A pointer to the smallest/lowest node in the tree or NULL
       for an empty tree.
```

**LVO:** -900 ($384).

### AVL_FindLastNode (V45)

`(exec.doc V45, /AVL_FindLastNode)`

```
    NAME
       AVL_FindLastNode -- return the highest/biggest node (V45)

    SYNOPSIS
       result = AVL_FindLastNode( root )
       D0                          A0

    RESULT
       A pointer to the highest/biggest node in the tree or NULL
       for an empty tree.
```

**LVO:** -906 ($38A).

---

# DOS V36+ additions

## 10. The V37 C rewrite

In V36 (Kickstart 2.0) and finalised in V37 (Kickstart 2.04), `dos.library` was rewritten from BCPL into C. The rewrite had three consequences that any V37+ DOS caller should be aware of:

1. **BCPL globals are gone from the public contract.** Most of the V1.3 dos internals exposed in the BCPL DOS "globvec" — the list of function pointers in Negative LVO order that BCPL clients walked — are no longer relied upon. New code in C still uses `BPTR`/`BSTR` types for backwards compatibility (because on-disk segment and file-info formats encode them) but the library interior is native C pointers throughout.

2. **New parameter-passing discipline.** In V36+ the library follows the standard Amiga register-based ABI. Every function takes arguments in D1-D5/A0-A1 and returns in D0. You do not need the BCPL calling-convention stubs that amiga.lib shipped for 1.3-era code.

3. **Task-callable where possible.** Many V37 dos entries are callable from a plain exec Task rather than requiring a Process. This includes `DoPkt()` (fixed in V37), `SendPkt()`, `WaitPkt()`, `AllocDosObject()`, `FreeDosObject()` and the wrappers built on them. Packet-level I/O is the canonical way for a task to talk to a DOS handler without needing its own pr_MsgPort, pr_CurrentDir, etc.

Legacy BCPL programs that poked at the `globvec` will break on V36+. Normal C and assembly clients calling through the jump table are unaffected — in fact, many V1.3 bugs were fixed as a side effect (see `Close` returning a success value, `UnLoadSeg` returning a real result, `SumKickData` being safer, the `Delay(0)` / `WaitForChar(0)` bug class cleared up).

---

## 11. Process creation — CreateNewProc / CreateNewProcTags (V36)

`(dos.doc V45, /CreateNewProc)`

```
   NAME
        CreateNewProc -- Create a new process (V36)

   SYNOPSIS
        process = CreateNewProc(tags)
        D0                       D1

        struct Process *CreateNewProc(struct TagItem *)

        process = CreateNewProcTagList(tags)
        D0                              D1

        struct Process *CreateNewProcTagList(struct TagItem *)

        process = CreateNewProcTags(Tag1, ...)

        struct Process *CreateNewProcTags(ULONG, ...)

   FUNCTION
        This creates a new process according to the tags passed in.  See
        dos/dostags.h for the tags.

        You must specify one of NP_Seglist or NP_Entry.  NP_Seglist takes a
        seglist (as returned by LoadSeg()).  NP_Entry takes a function
        pointer for the routine to call.

        There are many options, as you can see by examining dos/dostags.h.
        The defaults are for a non-CLI process, with copies of your
        CurrentDir, HomeDir (used for PROGDIR:), priority, consoletask,
        windowptr, and variables.  The input and output filehandles default
        to opens of NIL:, stack to 4000, and others as shown in dostags.h.
        This is a fairly reasonable default setting for creating threads,
        though you may wish to modify it (for example, to give a descriptive
        name to the process.)

        CreateNewProc() is callable from a task, though any actions that
        require doing Dos I/O (DupLock() of currentdir, for example) will not
        occur.

        NOTE: if you call CreateNewProc() with both NP_Arguments, you must
        not specify an NP_Input of NULL.  When NP_Arguments is specified, it
        needs to modify the input filehandle to make ReadArgs() work properly.

   INPUTS
        tags - a pointer to a TagItem array.

   RESULT
        process - The created process, or NULL.  Note that if it returns
                  NULL, you must free any items that were passed in via
                  tags, such as if you passed in a new current directory
                  with NP_CurrentDir.

   BUGS
        In V36, NP_Arguments was broken in a number of ways, and probably
        should be avoided (instead you should start a small piece of your
        own code, which calls RunCommand() to run the actual code you wish
        to run).  In V37, NP_Arguments works, though see the note above.
```

**LVO:** -498 ($1F2).

### The NP_ tag list

`CreateNewProc` is driven entirely by a taglist from `<dos/dostags.h>`. The important tags:

| Tag | Default | Meaning |
|-----|---------|---------|
| `NP_Seglist` | required if no NP_Entry | BPTR seglist as returned by `LoadSeg()` |
| `NP_Entry` | required if no NP_Seglist | C function pointer for thread-style creation |
| `NP_FreeSeglist` | `TRUE` if NP_Seglist | Whether the child should `UnLoadSeg` its code on exit |
| `NP_Name` | `"New Process"` | Process name string |
| `NP_StackSize` | 4000 | Stack size in bytes |
| `NP_Priority` | parent's priority | Task priority |
| `NP_Input` | NIL: | BPTR filehandle for child's Input() |
| `NP_Output` | NIL: | BPTR filehandle for child's Output() |
| `NP_Error` | NIL: | BPTR filehandle for child's error stream |
| `NP_CloseInput` | TRUE | Child closes its input FH on exit |
| `NP_CloseOutput` | TRUE | Child closes its output FH on exit |
| `NP_CloseError` | TRUE | Child closes its error FH on exit |
| `NP_CurrentDir` | DupLock of parent | BPTR current-dir lock |
| `NP_HomeDir` | DupLock of parent | BPTR used for `PROGDIR:` |
| `NP_CopyVars` | TRUE | Copy local variables from parent |
| `NP_Cli` | FALSE | Create a CLI structure |
| `NP_Path` | NULL | BPTR path-list (used when NP_Cli) |
| `NP_CommandName` | NULL | Command name string for shell history |
| `NP_Arguments` | NULL | Argument string stuffed into stdin so `ReadArgs()` works. Implies non-NULL NP_Input. |
| `NP_NotifyOnDeath` | FALSE | Send a message to parent on child exit |
| `NP_Synchronous` | FALSE | Parent blocks until child exits |
| `NP_ExitCode` | NULL | Function pointer called on child exit |
| `NP_ExitData` | 0 | UserData passed to NP_ExitCode |
| `NP_StackCheck` | FALSE | Enable stack-overflow checking (later NDKs) |
| `NP_ConsoleTask` | parent's | Console task handler pointer |
| `NP_WindowPtr` | parent's | Window pointer for DOS requesters (-1 suppresses) |

Notes:
- Use `NP_Entry` for "thread-style" creation — you call `CreateNewProcTags(NP_Entry, myfunc, NP_Name, "Worker", TAG_DONE)` and the new Process calls `myfunc()` directly.
- Use `NP_Seglist` with code loaded by `LoadSeg()` to run an external command.
- `NP_Arguments` is the proper way to pass command-line text to a child that will parse it with `ReadArgs()`.

---

## 12. System (Execute replacement) — SystemTagList (V36)

`(dos.doc V45, /SystemTagList)`

```
   NAME
        SystemTagList -- Have a shell execute a command line (V36)

   SYNOPSIS
        error = SystemTagList(command, tags)
        D0                      D1      D2

        LONG SystemTagList(STRPTR, struct TagItem *)

        error = System(command, tags)
        D0              D1      D2

        LONG System(STRPTR, struct TagItem *)

        error = SystemTags(command, Tag1, ...)

        LONG SystemTags(STRPTR, ULONG, ...)

   FUNCTION
        Similar to Execute(), but does not read commands from the input
        filehandle.  Spawns a Shell process to execute the command, and
        returns the returncode the command produced, or -1 if the command
        could not be run for any reason.  The input and output filehandles
        will not be closed by System, you must close them (if needed) after
        System returns, if you specified them via SYS_Input or SYS_Output.

        By default the new process will use your current Input() and Output()
        filehandles.  Normal Shell command-line parsing will be done
        including redirection on 'command'.  The current directory and path
        will be inherited from your process.  Your path will be used to find
        the command (if no path is specified).

        Note that you may NOT pass the same filehandle for both SYS_Input
        and SYS_Output.  If you want input and output to both be to the same
        CON: window, pass a SYS_Input of a filehandle on the CON: window,
        and pass a SYS_Output of NULL.  The shell will automatically set
        the default Output() stream to the window you passed via SYS_Input,
        by opening "*" on that handler.

        If used with the SYS_Asynch flag, it WILL close both it's input and
        output filehandles after running the command (even if these were
        your Input() and Output()!)

        Normally uses the boot (ROM) shell, but other shells can be specified
        via SYS_UserShell and SYS_CustomShell.  Normally, you should send
        things written by the user to the UserShell.  The UserShell defaults
        to the same shell as the boot shell.

        The tags are passed through to CreateNewProc() (tags that conflict
        with SystemTagList() will be filtered out).  This allows setting
        things like priority, etc for the new process.  The tags that are
        currently filtered out are:

                NP_Seglist
                NP_FreeSeglist
                NP_Entry
                NP_Input
                NP_Output
                NP_CloseInput
                NP_CloseOutput
                NP_HomeDir
                NP_Cli

   RESULT
        error   - 0 for success, result from command, or -1.  Note that on
                  error, the caller is responsible for any filehandles or other
                  things passed in via tags.  -1 will only be returned if
                  dos could not create the new shell.  If the command is not
                  found the shell will return an error value, normally
                  RETURN_ERROR.
```

**LVO:** -606 ($25E).

### SYS_ tag values

| Tag | Default | Meaning |
|-----|---------|---------|
| `SYS_Input` | current Input() | BPTR filehandle used as child's stdin |
| `SYS_Output` | current Output() | BPTR filehandle used as child's stdout |
| `SYS_Error` | NULL | Separate error stream (V39+) |
| `SYS_Asynch` | FALSE | Run async; closes SYS_Input/Output on exit |
| `SYS_UserShell` | FALSE | Use user's preferred shell instead of boot shell |
| `SYS_CustomShell` | NULL | Explicit shell seglist/pathname to use |

Any `NP_*` tag that isn't in the filter list above is passed through to `CreateNewProc()`, so you can set `NP_Priority`, `NP_StackSize`, `NP_Name`, etc. on the shell process.

---

## 13. Argument parsing — ReadArgs (V36)

`(dos.doc V45, /ReadArgs)`

```
   NAME
        ReadArgs - Parse the command line input (V36)

   SYNOPSIS
        result = ReadArgs(template, array, rdargs)
        D0                   D1      D2      D3

        struct RDArgs * ReadArgs(STRPTR, LONG *, struct RDArgs *)

   FUNCTION
        Parses and argument string according to a template.  Normally gets
        the arguments by reading buffered IO from Input(), but also can be
        made to parse a string.  MUST be matched by a call to FreeArgs().

        ReadArgs() parses the commandline according to a template that is
        passed to it.  This specifies the different command-line options and
        their types.  A template consists of a list of options.  Options are
        named in "full" names where possible (for example, "Quick" instead of
        "Q").  Abbreviations can also be specified by using "abbrev=option"
        (for example, "Q=Quick").

        Options in the template are separated by commas.  To get the results
        of ReadArgs(), you examine the array of longwords you passed to it
        (one entry per option in the template).  This array should be cleared
        (or initialized to your default values) before passing to ReadArgs().

        Options can be followed by modifiers, which specify things such as
        the type of the option.  Modifiers are specified by following the
        option with a '/' and a single character modifier.  Multiple modifiers
        can be specified by using multiple '/'s.  Valid modifiers are:

        /S - Switch.  This is considered a boolean variable, and will be
             set if the option name appears in the command-line.
        /K - Keyword.  This means that the option will not be filled unless
             the keyword appears.  For example if the template is "Name/K",
             then unless "Name=<string>" or "Name <string>" appears in the
             command line, Name will not be filled.
        /N - Number.  This parameter is considered a decimal number, and will
             be converted by ReadArgs.  If an invalid number is specified,
             an error will be returned.  The entry will be a pointer to the
             longword number (this is how you know if a number was specified).
        /T - Toggle.  This is similar to a switch, but when specified causes
             the boolean value to "toggle".
        /A - Required.  This keyword must be given a value during command-line
             processing, or an error is returned.
        /F - Rest of line.  If this is specified, the entire rest of the line
             is taken as the parameter for the option, even if other option
             keywords appear in it.
        /M - Multiple strings.  This means the argument will take any number
             of strings, returning them as an array of strings.  Any arguments
             not considered to be part of another option will be added to this
             option.  Only one /M should be specified in a template.

             There is an interaction between /M parameters and /A parameters.
             If there are unfilled /A parameters after parsing, it will grab
             strings from the end of a previous /M parameter list to fill the
             /A's.  This is used for things like Copy ("From/A/M,To/A").

        ReadArgs() returns a struct RDArgs if it succeeds.  This serves as an
        "anchor" to allow FreeArgs() to free the associated memory.

   INPUTS
        template - formatting string
        array    - array of longwords for results, 1 per template entry
        rdargs   - optional rdargs structure for options.  AllocDosObject
                   should be used for allocating them if you pass one in.

   RESULT
        result   - a struct RDArgs or NULL for failure.

   BUGS
        In V36, there were a couple of minor bugs with certain argument
        combinations (/M/N returned strings, /T didn't work, and /K and
        /F interacted).  Also, a template with a /K before any non-switch
        parameter will require the argument name to be given in order for
        line to be accepted.  These problems should be fixed for V37.

        Currently (V37 and before) it requires any strings passed in to have
        newlines at the end of the string.  This may or may not be fixed in
        the future.
```

**LVO:** -798 ($31E).

#### Worked example

The standard idiom in a V37+ command:

```c
LONG args[3] = { 0, 0, 0 };
struct RDArgs *rda = ReadArgs("From/A,To/K/A,Verbose/S", args, NULL);
if (!rda) {
    PrintFault(IoErr(), "mycmd");
    return RETURN_ERROR;
}
STRPTR from = (STRPTR)args[0];
STRPTR to   = (STRPTR)args[1];
BOOL   verbose = args[2] ? TRUE : FALSE;
/* ... work ... */
FreeArgs(rda);
```

### FreeArgs (V36)

`(dos.doc V45, /FreeArgs)`

```
   NAME
        FreeArgs - Free allocated memory after ReadArgs() (V36)

   SYNOPSIS
        FreeArgs(rdargs)
                   D1

        void FreeArgs(struct RDArgs *)

   FUNCTION
        Frees memory allocated to return arguments in from ReadArgs().  If
        ReadArgs allocated the RDArgs structure it will be freed.  If NULL
        is passed in this function does nothing.

   INPUTS
        rdargs - structure returned from ReadArgs() or NULL.
```

**LVO:** -858 ($35A).

### ReadItem (V36)

`(dos.doc V45, /ReadItem)`

```
   NAME
        ReadItem - reads a single argument/name from command line (V36)

   SYNOPSIS
        value = ReadItem(buffer, maxchars, input)
        D0                D1        D2      D3

        LONG ReadItem(STRPTR, LONG, struct CSource *)

   FUNCTION
        Reads a "word" from either Input() (buffered), or via CSource, if it
        is non-NULL (see <dos/rdargs.h> for more information).  Handles
        quoting and some '*' substitutions (*e and *n) inside quotes (only).
        See dos/dos.h for a listing of values returned by ReadItem()
        (ITEM_XXXX).  A "word" is delimited by whitespace, quotes, '=', or
        an EOF.

        ReadItem always unreads the last thing read (UnGetC(fh,-1)) so the
        caller can find out what the terminator was.

   BUGS
        Doesn't actually unread the terminator.
```

**LVO:** -810 ($32A).

### StrToLong (V36)

`(dos.doc V45, /StrToLong)`

```
   NAME
        StrToLong -- string to long value (decimal) (V36)

   SYNOPSIS
        characters = StrToLong(string,value)
        D0                       D1    D2

        LONG StrToLong(STRPTR, LONG *)

   FUNCTION
        Converts decimal string into LONG value.  Returns number of characters
        converted.  Skips over leading spaces & tabs (included in count).  If
        no decimal digits are found (after skipping leading spaces & tabs),
        StrToLong returns -1 for characters converted, and puts 0 into value.

   BUGS
        Before V39, if there were no convertible characters it returned the
        number of leading white-space characters (space and tab in this case).
```

**LVO:** -816 ($330).

### FindArg (V36)

`(dos.doc V45, /FindArg)`

```
   NAME
        FindArg - find a keyword in a template (V36)

   SYNOPSIS
        index = FindArg(template, keyword)
        D0                D1        D2

        LONG FindArg(STRPTR, STRPTR)

   FUNCTION
        Returns the argument number of the keyword, or -1 if it is not a
        keyword for the template.  Abbreviations are handled.
```

**LVO:** -804 ($324).

---

## 14. Variadic printf family (V36)

### VPrintf / Printf (V36)

`(dos.doc V45, /VPrintf)`

```
   NAME
        VPrintf -- format and print string (buffered) (V36)

   SYNOPSIS
        count = VPrintf(fmt, argv)
          D0            D1   D2

        LONG VPrintf(STRPTR, LONG *)

        count = Printf(fmt, ...)

        LONG Printf(STRPTR, ...)

   FUNCTION
        Writes the formatted string and values to Output().  This routine is
        assumed to handle all internal buffering so that the formatting string
        and resultant formatted values can be arbitrarily long.  Any secondary
        error code is returned in IoErr().  This routine is buffered.

        Note: RawDoFmt assumes 16 bit ints, so you will usually need 'l's in
        your formats (ex: %ld versus %d).

   RESULT
        count - Number of bytes written or -1 (EOF) for an error

   BUGS
        The prototype for Printf() currently forces you to cast the first
        varargs parameter to LONG due to a deficiency in the program
        that generates fds, prototypes, and amiga.lib stubs.
```

**LVO:** -954 ($3BA).

### VFPrintf / FPrintf (V36)

`(dos.doc V45, /VFPrintf)`

```
   NAME
        VFPrintf -- format and print a string to a file (buffered) (V36)

   SYNOPSIS
        count = VFPrintf(fh, fmt, argv)
        D0               D1  D2    D3

        LONG VFPrintf(BPTR, STRPTR, LONG *)

        count = FPrintf(fh, fmt, ...)

        LONG FPrintf(BPTR, STRPTR, ...)

   FUNCTION
        Writes the formatted string and values to the given file.  This
        routine is assumed to handle all internal buffering so that the
        formatting string and resultant formatted values can be arbitrarily
        long.  Any secondary error code is returned in IoErr().  This routine
        is buffered.

   RESULT
        count - Number of bytes written or -1 (EOF) for an error
```

**LVO:** -354 ($162).

### VFWritef / FWritef (V36)

`(dos.doc V45, /VFWritef)`

```
   NAME
        VFWritef - write a BCPL formatted string to a file (buffered) (V36)

   SYNOPSIS
        count = VFWritef(fh, fmt, argv)
        D0               D1  D2    D3

        LONG VFWritef(BPTR, STRPTR, LONG *)

        count = FWritef(fh, fmt, ...)

        LONG FWritef(BPTR, STRPTR, ...)

   FUNCTION
        Writes the formatted string and values to the specified file.
        The formats are in BCPL form.  This routine is buffered.

        Supported formats are:  (Note x is in base 36!)
                %S  - string (CSTR)
                %Tx - writes a left-justified string in a field at least
                      x bytes long.
                %C  - writes a single character
                %Ox - writes a number in octal, maximum x characters wide
                %Xx - writes a number in hex, maximum x characters wide
                %Ix - writes a number in decimal, maximum x characters wide
                %N  - writes a number in decimal, any length
                %Ux - writes an unsigned number, maximum x characters wide
                %$  - ignore parameter

        Note: 'x' above is actually the character value - '0'.

   BUGS
        As of V37, VFWritef() does NOT return a valid return value.  In
        order to reduce possible errors, the prototypes supplied for the
        system as of V37 have it typed as VOID.
```

**LVO:** -348 ($15C).

### Format codes (VFPrintf / VPrintf / Printf)

`VFPrintf`, `VPrintf`, `Printf`, and `FPrintf` share `exec.library/RawDoFmt` format codes:

| Code | Meaning |
|------|---------|
| `%d`, `%ld` | Signed decimal (word / long) |
| `%u`, `%lu` | Unsigned decimal |
| `%x`, `%lx` | Hex |
| `%o`, `%lo` | Octal |
| `%s` | C string (NUL-terminated) |
| `%b` | BCPL string (length-prefixed) |
| `%c` | Single character |
| `-` | Left-justify flag |
| number | Field width / min digits |

Because `RawDoFmt` was built for 16-bit ints, **you almost always need `%ld`/`%lx` for C integer types**, not plain `%d`/`%x`.

### PutStr (V36) and WriteChars (V36)

`(dos.doc V45, /PutStr)`

```
   NAME
        PutStr -- Writes a string the the default output (buffered) (V36)

   FUNCTION
        This routine writes an unformatted string to the default output.  No
        newline is appended to the string and any error is returned.  This
        routine is buffered.

   RESULT
        error - 0 for success, -1 for any error.  NOTE: this is opposite
                most Dos function returns!
```

**LVO:** -948 ($3B4).

`(dos.doc V45, /WriteChars)`

```
   NAME
        WriteChars -- Writes bytes to the the default output (buffered) (V36)

   FUNCTION
        This routine writes a number of bytes to the default output.  The
        length is returned.  This routine is buffered.

   RESULT
        count - Number of bytes written.  -1 (EOF) indicates an error
```

**LVO:** -942 ($3AE).

---

## 15. File operations (V36)

### OpenFromLock (V36)

`(dos.doc V45, /OpenFromLock)`

```
   NAME
        OpenFromLock -- Opens a file you have a lock on (V36)

   SYNOPSIS
        fh = OpenFromLock(lock)
        D0                 D1

        BPTR OpenFromLock(BPTR)

   FUNCTION
        Given a lock, this routine performs an open on that lock.  If the open
        succeeds, the lock is (effectively) relinquished, and should not be
        UnLock()ed or used.  If the open fails, the lock is still usable.
        The lock associated with the file internally is of the same access
        mode as the lock you gave up - shared is similar to MODE_OLDFILE,
        exclusive is similar to MODE_NEWFILE.
```

**LVO:** -378 ($17A).

### ParentOfFH (V36)

`(dos.doc V45, /ParentOfFH)`

```
   NAME
        ParentOfFH -- returns a lock on the parent directory of a file (V36)

   SYNOPSIS
        lock = ParentOfFH(fh)
        D0               D1

        BPTR ParentOfFH(BPTR)

   FUNCTION
        Returns a shared lock on the parent directory of the filehandle.
```

**LVO:** -384 ($180).

### ExamineFH (V36)

`(dos.doc V45, /ExamineFH)`

```
   NAME
        ExamineFH -- Gets information on an open file (V36)

   SYNOPSIS
        success = ExamineFH(fh, fib)
        D0                  D1  D2

        BOOL ExamineFH(BPTR, struct FileInfoBlock *)

   FUNCTION
        Examines a filehandle and returns information about the file in the
        FileInfoBlock.  There are no guarantees as to whether the fib_Size
        field will reflect any changes made to the file size it was opened,
        though filesystems should attempt to provide up-to-date information
        for it.
```

**LVO:** -390 ($186).

### DupLockFromFH (V36)

`(dos.doc V45, /DupLockFromFH)`

```
   NAME
        DupLockFromFH -- Gets a lock on an open file (V36)

   SYNOPSIS
        lock = DupLockFromFH(fh)
        D0                   D1

        BPTR DupLockFromFH(BPTR)

   FUNCTION
        Obtain a lock on the object associated with fh.  Only works if the
        file was opened using a non-exclusive mode.  Other restrictions may be
        placed on success by the filesystem.
```

**LVO:** -372 ($174).

### SetFileDate (V36)

`(dos.doc V45, /SetFileDate)`

```
   NAME
        SetFileDate -- Sets the modification date for a file or dir (V36)

   SYNOPSIS
        success = SetFileDate(name, date)
        D0                     D1    D2

        BOOL SetFileDate(STRPTR, struct DateStamp *)

   FUNCTION
        Sets the file date for a file or directory.  Note that for the Old
        File System and the Fast File System, the date of the root directory
        cannot be set.  Other filesystems may not support setting the date
        for all files/directories.
```

**LVO:** -396 ($18C).

### NameFromLock (V36)

`(dos.doc V45, /NameFromLock)`

```
   NAME
        NameFromLock -- Returns the name of a locked object (V36)

   SYNOPSIS
        success = NameFromLock(lock, buffer, len)
        D0                      D1     D2    D3

        BOOL NameFromLock(BPTR, STRPTR, LONG)

   FUNCTION
        Returns a fully qualified path for the lock.  This routine is
        guaranteed not to write more than len characters into the buffer.  The
        name will be null-terminated.  NOTE: if the volume is not mounted,
        the system will request it (unless of course you set pr_WindowPtr to
        -1).  If the volume is not mounted or inserted, it will return an
        error.  If the lock passed in is NULL, "SYS:" will be returned. If
        the buffer is too short, an error will be returned, and IoErr() will
        return ERROR_LINE_TOO_LONG.
```

**LVO:** -402 ($192).

### NameFromFH (V36)

`(dos.doc V45, /NameFromFH)`

```
   NAME
        NameFromFH -- Get the name of an open filehandle (V36)

   SYNOPSIS
        success = NameFromFH(fh, buffer, len)
        D0                   D1    D2    D3

        BOOL NameFromFH(BPTR, STRPTR, LONG)

   FUNCTION
        Returns a fully qualified path for the filehandle.  This routine is
        guaranteed not to write more than len characters into the buffer.  The
        name will be null-terminated.  See NameFromLock() for more information.

        Note: Older filesystems that don't support ExamineFH() will cause
        NameFromFH() to fail with ERROR_ACTION_NOT_SUPPORTED.
```

**LVO:** -408 ($198).

### ChangeMode (V36)

`(dos.doc V45, /ChangeMode)`

```
   NAME
        ChangeMode - Change the current mode of a lock or filehandle (V36)

   SYNOPSIS
        success = ChangeMode(type, object, newmode)
        D0                    D1     D2      D3

        BOOL ChangeMode(ULONG, BPTR, ULONG)

   FUNCTION
        This allows you to attempt to change the mode in use by a lock or
        filehandle.  For example, you could attempt to turn a shared lock
        into an exclusive lock.  The handler may well reject this request.
        Warning: if you use the wrong type for the object, the system may
        crash.

   INPUTS
        type    - Either CHANGE_FH or CHANGE_LOCK
        object  - A lock or filehandle
        newmode - The new mode you want

   BUGS
        Did not work in 2.02 or before (V36).  Works in V37.  In the
        earlier versions, it can crash the machine.
```

**LVO:** -450 ($1C2).

### SetFileSize (V36)

`(dos.doc V45, /SetFileSize)`

```
   NAME
        SetFileSize -- Sets the size of a file (V36)

   SYNOPSIS
        newsize = SetFileSize(fh, offset, mode)
        D0                    D1    D2     D3

        LONG SetFileSize(BPTR, LONG, LONG)

   FUNCTION
        Changes the file size, truncating or extending as needed.  Not all
        handlers may support this; be careful and check the return code.

   INPUTS
        fh     - File to be truncated/extended.
        offset - Offset from position determined by mode.
        mode   - One of OFFSET_BEGINNING, OFFSET_CURRENT, or OFFSET_END.

   BUGS
        The RAM: filesystem and the normal Amiga filesystem act differently
        in where the file position is left after SetFileSize().
```

**LVO:** -456 ($1C8).

### FGetC (V36)

`(dos.doc V45, /FGetC)`

```
   NAME
        FGetC -- Read a character from the specified input (buffered) (V36)

   SYNOPSIS
        char = FGetC(fh)
        D0           D1

        LONG FGetC(BPTR)

   FUNCTION
        Reads the next character from the input stream.  A -1 is
        returned when EOF or an error is encountered.  This call is buffered.
        Use Flush() between buffered and unbuffered I/O on a filehandle.

   BUGS
        In V36, after an EOF was read, EOF would always be returned from
        FGetC() from then on.  Starting in V37, it tries to read from the
        handler again each time (unless UnGetC(fh,-1) was called).
```

**LVO:** -306 ($132).

### FPutC (V36)

`(dos.doc V45, /FPutC)`

```
   NAME
        FPutC -- Write a character to the specified output (buffered) (V36)

   SYNOPSIS
        char = FPutC(fh, char)
        D0           D1   D2

        LONG FPutC(BPTR, LONG)

   FUNCTION
        Writes a single character to the output stream.  This call is
        buffered.  Use Flush() between buffered and unbuffered I/O on a
        filehandle.  Interactive filehandles are flushed automatically
        on a newline, return, '\0', or line feed.

   BUGS
        Older autodocs indicated that you should pass a UBYTE.  The
        correct usage is to pass a LONG in the range 0-255.
```

**LVO:** -312 ($138).

### UnGetC (V36)

`(dos.doc V45, /UnGetC)`

```
   NAME
        UnGetC -- Makes a char available for reading again. (buffered) (V36)

   SYNOPSIS
        value = UnGetC(fh, character)
        D0             D1      D2

        LONG UnGetC(BPTR, LONG)

   FUNCTION
        Pushes the character specified back into the input buffer.  Every
        time you use a buffered read routine, you can always push back 1
        character.  You may be able to push back more, though it is not
        recommended, since there is no guarantee on how many can be
        pushed back at a given moment.

        Passing -1 for the character will cause the last character read to
        be pushed back.  If the last character read was an EOF, the next
        character read will be an EOF.

   BUGS
        In V36, UnGetC(fh,-1) after an EOF would not cause the next character
        read to be an EOF.  This was fixed for V37.
```

**LVO:** -318 ($13E).

### FGets (V36)

`(dos.doc V45, /FGets)`

```
   NAME
        FGets -- Reads a line from the specified input (buffered) (V36)

   SYNOPSIS
        buffer = FGets(fh, buf, len)
        D0             D1  D2   D3

        STRPTR FGets(BPTR, STRPTR, ULONG)

   FUNCTION
        This routine reads in a single line from the specified input stopping
        at a NEWLINE character or EOF.  In either event, UP TO the number of
        len specified bytes minus 1 will be copied into the buffer.

        If terminated by a newline, the newline WILL be the last character in
        the buffer.  This is a buffered read routine.  The string read in IS
        null-terminated.

   BUGS
        In V36 and V37, it copies one more byte than it should if it doesn't
        hit an EOF or newline.  This is fixed in dos V39.  Workaround for
        V36/V37: pass in buffersize-1.
```

**LVO:** -336 ($150).

### FPuts (V36)

`(dos.doc V45, /FPuts)`

```
   NAME
        FPuts -- Writes a string the the specified output (buffered) (V36)

   SYNOPSIS
        error = FPuts(fh, str)
        D0            D1  D2

        LONG FPuts(BPTR, STRPTR)

   FUNCTION
        This routine writes an unformatted string to the filehandle.  No
        newline is appended to the string.  This routine is buffered.

   RESULT
        error - 0 normally, otherwise -1.  Note that this is opposite of
                most other Dos functions, which return success.
```

**LVO:** -342 ($156).

### FRead (V36)

`(dos.doc V45, /FRead)`

```
   NAME
        FRead -- Reads a number of blocks from an input (buffered) (V36)

   SYNOPSIS
        count = FRead(fh, buf, blocklen, blocks)
        D0            D1  D2     D3        D4

        LONG FRead(BPTR, STRPTR, ULONG, ULONG)

   FUNCTION
        Attempts to read a number of blocks, each blocklen long, into the
        specified buffer from the input stream.  May return less than
        the number of blocks requested.  This call is buffered.
```

**LVO:** -324 ($144).

### FWrite (V36)

`(dos.doc V45, /FWrite)`

```
   NAME
        FWrite -- Writes a number of blocks to an output (buffered) (V36)

   SYNOPSIS
        count = FWrite(fh, buf, blocklen, blocks)
        D0             D1  D2     D3        D4

        LONG FWrite(BPTR, STRPTR, ULONG, ULONG)
```

**LVO:** -330 ($14A).

### Flush (V36)

`(dos.doc V45, /Flush)`

```
   NAME
        Flush -- Flushes buffers for a buffered filehandle (V36)

   SYNOPSIS
        success = Flush(fh)
        D0              D1

        LONG Flush(BPTR)

   FUNCTION
        Flushes any pending buffered writes to the filehandle.  All buffered
        writes will also be flushed on Close().  If the filehandle was being
        used for input, it drops the buffer, and tries to Seek() back to the
        last read position.

   BUGS
        Before V37 release, Flush() returned a random value.  As of V37,
        it always returns success.
        The V36 and V37 releases didn't properly flush filehandles which
        have never had a buffered IO done on them.  This is fixed in V39.
```

**LVO:** -360 ($168).

### SetVBuf (V39)

`(dos.doc V45, /SetVBuf)`

```
   NAME
        SetVBuf -- set buffering modes and size (V39)

   SYNOPSIS
        error = SetVBuf(fh, buff, type, size)
        D0              D1   D2    D3    D4

        LONG SetVBuf(BPTR, STRPTR, LONG, LONG)

   FUNCTION
        Changes the buffering modes and buffer size for a filehandle.
        With buff == NULL, the current buffer will be deallocated and a
        new one of (approximately) size will be allocated.  If buffer is
        non-NULL, it will be used for buffering and must be at least
        max(size,208) bytes long, and MUST be longword aligned.  If size
        is -1, then only the buffering mode will be changed.

        Note that a user-supplied buffer will not be freed if it is later
        replaced by another SetVBuf() call, nor will it be freed if the
        filehandle is closed.

   BUGS
        Not implemented until after V39.  From V36 up to V39, always
        returned 0.
```

**LVO:** -366 ($16E). Note the entry existed from V36 but was a stub until late V39.

### SetMode (V36)

`(dos.doc V45, /SetMode)`

```
   NAME
        SetMode - Set the current behavior of a handler (V36)

   SYNOPSIS
        success = SetMode(fh, mode)
        D0                D1  D2

        BOOL SetMode(BPTR, LONG)

   FUNCTION
        SetMode() sends an ACTION_SCREEN_MODE packet to the handler in
        question, normally for changing a CON: handler to raw mode or
        vice-versa.  For CON:, use 1 to go to RAW: mode, 0 for CON: mode.
```

**LVO:** -426 ($1AA).

### SelectInput (V36)

`(dos.doc V45, /SelectInput)`

```
   NAME
        SelectInput -- Select a filehandle as the default input channel (V36)

   SYNOPSIS
        old_fh = SelectInput(fh)
        D0                   D1

        BPTR SelectInput(BPTR)

   FUNCTION
        Set the current input as the default input for the process.
        This changes the value returned by Input().  old_fh should
        be closed or saved as needed.
```

**LVO:** -294 ($126).

### SelectOutput (V36)

`(dos.doc V45, /SelectOutput)`

```
   NAME
        SelectOutput -- Select a filehandle as the default output channel (V36)

   SYNOPSIS
        old_fh = SelectOutput(fh)
        D0                    D1

        BPTR SelectOutput(BPTR)

   FUNCTION
        Set the current output as the default output for the process.
        This changes the value returned by Output().
```

**LVO:** -300 ($12C).

### SetConsoleTask / GetConsoleTask (V36)

`(dos.doc V45, /SetConsoleTask)`

```
   NAME
        SetConsoleTask -- Sets the default console for the process (V36)

   SYNOPSIS
        oldport = SetConsoleTask(port)
        D0                        D1

        struct MsgPort *SetConsoleTask(struct MsgPort *)

   FUNCTION
        Sets the default console task's port (pr_ConsoleTask) for the
        current process.
```

**LVO:** -516 ($204).

`(dos.doc V45, /GetConsoleTask)`

```
   NAME
        GetConsoleTask -- Returns the default console for the process (V36)

   SYNOPSIS
        port = GetConsoleTask()
        D0

        struct MsgPort *GetConsoleTask(void)
```

**LVO:** -510 ($1FE).

### SetFileSysTask / GetFileSysTask (V36)

`(dos.doc V45, /GetFileSysTask)`

```
   NAME
        GetFileSysTask -- Returns the default filesystem for the process (V36)
```

`(dos.doc V45, /SetFileSysTask)`

```
   NAME
        SetFileSysTask -- Sets the default filesystem for the process (V36)
```

**LVOs:** GetFileSysTask -522 ($20A), SetFileSysTask -528 ($210).

### AllocDosObject (V36) / FreeDosObject (V36)

`(dos.doc V45, /AllocDosObject)`

```
   NAME
        AllocDosObject -- Creates a dos object (V36)

   SYNOPSIS
        ptr = AllocDosObject(type, tags)
        D0                    D1    D2

        void *AllocDosObject(ULONG, struct TagItem *)

   FUNCTION
        Create one of several dos objects, initializes it, and returns it
        to you.  Note the DOS_STDPKT returns a pointer to the sp_Pkt of the
        structure.

        This function may be called by a task for all types and tags defined
        in the V37 includes (DOS_FILEHANDLE through DOS_RDARGS and ADO_FH_Mode
        through ADO_PromptLen, respectively).

   INPUTS
        type - type of object requested
        tags - pointer to taglist with additional information

   BUGS
        Before V39, DOS_CLI should be used with care since FreeDosObject()
        can't free it.
```

**LVO:** -228 ($E4).

`(dos.doc V45, /FreeDosObject)`

```
   NAME
        FreeDosObject -- Frees an object allocated by AllocDosObject() (V36)
```

**LVO:** -234 ($EA).

Supported object types (V37): `DOS_FILEHANDLE`, `DOS_EXALLCONTROL`, `DOS_FIB`, `DOS_STDPKT`, `DOS_CLI`, `DOS_RDARGS`.

---

## 16. Directory operations — ExAll / ExAllEnd

### ExAll (V36)

`(dos.doc V45, /ExAll)`

```
   NAME
        ExAll -- Examine an entire directory (V36)

   SYNOPSIS
        continue = ExAll(lock, buffer, size, type, control)
        D0               D1     D2     D3    D4     D5

        BOOL ExAll(BPTR,STRPTR,LONG,LONG,struct ExAllControl *)

   FUNCTION
        Examines an entire directory.

        Lock must be on a directory.  Size is the size of the buffer supplied.
        The buffer will be filled with (partial) ExAllData structures, as
        specified by the type field.

        Type is a value from those shown below that determines which
        information is to be stored in the buffer.  Each higher value adds a
        new thing to the list as described in the table below:-

                ED_NAME         FileName
                ED_TYPE         Type
                ED_SIZE         Size in bytes
                ED_PROTECTION   Protection bits
                ED_DATE         3 longwords of date
                ED_COMMENT      Comment (will be NULL if no comment)
                ED_OWNER        owner user-id and group-id (if supported) (V39)

        Thus, ED_NAME gives only filenames, and ED_OWNER gives everything.

        NOTE: V37 dos.library, when doing ExAll() emulation, and RAM: filesystem
        will return an error if passed ED_OWNER.  If you get ERROR_BAD_NUMBER,
        retry with ED_COMMENT to get everything but owner info.  All filesystems
        supporting ExAll() must support through ED_COMMENT, and must check Type
        and return ERROR_BAD_NUMBER if they don't support the type.

        The ead_Next entry gives a pointer to the next entry in the buffer.
        The last entry will have NULL in ead_Next.

        The control structure is required so that FFS can keep track if more
        than one call to ExAll is required.  This happens when there are more
        names in a directory than will fit into the buffer.

        NOTE: the control structure MUST be allocated by AllocDosObject!!!

        Entries:  This field tells the calling application how many entries
        are in the buffer after calling ExAll.  Note: make sure your code
        handles the 0 entries case, including 0 entries with continue
        non-zero.

        LastKey:  This field ABSOLUTELY MUST be initialised to 0 before
        calling ExAll for the first time.

        MatchString
            If this field is NULL then all filenames will be returned.  If
            this field is non-null then it is interpreted as a pointer to
            a string that is used to pattern match all file names before
            accepting them and putting them into the buffer.  The default
            AmigaDOS caseless pattern match routine is used.  This string
            MUST have been parsed by ParsePatternNoCase()!

        MatchFunc:
            Contains a pointer to a hook for a routine to decide if the entry
            will be included in the returned list of entries.  The entry is
            filled out first, and then passed to the hook.  If no MatchFunc is
            to be called then this entry should be NULL.

        Note that Dos will emulate ExAll() using Examine() and ExNext()
        if the handler in question doesn't support the ExAll() packet.

   RESULT
        continue - Whether or not ExAll is done.  If FALSE is returned, either
                   ExAll has completed (IoErr() == ERROR_NO_MORE_ENTRIES), or
                   an error occurred (check IoErr()).  If non-zero is returned,
                   you MUST call ExAll again until it returns FALSE.

   BUGS
        In V36, there were problems with ExAll (particularily with
        eac_MatchString, and ed_Next with the ramdisk and the emulation
        of it in Dos for handlers that do not support the packet.  It is
        advised you only use this under V37 and later.

        The V37 ROM/disk filesystem incorrectly returned comments as BSTR's.
        Fixed in V39.
        The V37 ROM/disk filesystem incorrectly handled values greater than
        ED_COMMENT.  Fixed in V39.
```

**LVO:** -432 ($1B0).

### ExAllEnd (V39)

`(dos.doc V45, /ExAllEnd)`

```
   NAME
        ExAllEnd -- Stop an ExAll() (V39)

   SYNOPSIS
        ExAllEnd(lock, buffer, size, type, control)
                  D1     D2     D3    D4     D5

   FUNCTION
        Stops an ExAll() on a directory before it hits NO_MORE_ENTRIES.
        The full set of arguments that had been passed to ExAll() must be
        passed to ExAllEnd(), so it can handle filesystems that can't abort
        an ExAll() directly.
```

**LVO:** -990 ($3DE).

### Difference from Examine / ExNext

`Examine()` / `ExNext()` walk one entry per call through a `FileInfoBlock`. `ExAll()` returns many entries per call into a caller-supplied buffer, and supports pattern matching (via `eac_MatchString` / `MatchFunc`) at the filesystem level. On supporting filesystems (FFS V37+, RAM:) this is dramatically faster for large directories. Always:

1. Allocate the `ExAllControl` with `AllocDosObject(DOS_EXALLCONTROL, NULL)`.
2. Zero `eac_LastKey` before the first call.
3. Loop while `ExAll()` returns non-zero OR until `IoErr()` returns `ERROR_NO_MORE_ENTRIES`.
4. If you abort early (user break, error, whatever) call `ExAllEnd()` with the same arguments so filesystems that require it can tear down their enumeration state.
5. Free the control with `FreeDosObject(DOS_EXALLCONTROL, control)`.

---

## 17. Notifications — StartNotify / EndNotify (V36)

### StartNotify (V36)

`(dos.doc V45, /StartNotify)`

```
   NAME
        StartNotify -- Starts notification on a file or directory (V36)

   SYNOPSIS
        success = StartNotify(notifystructure)
        D0                          D1

        BOOL StartNotify(struct NotifyRequest *)

   FUNCTION
        Posts a notification request.  Do not modify the notify structure while
        it is active.  You will be notified when the file or directory changes.
        For files, you will be notified after the file is closed.  Not all
        filesystems will support this: applications should NOT require it.  In
        particular, most network filesystems won't support it.

   BUGS
        The V36 floppy/HD filesystem doesn't actually send notifications.  The
        V36 ram handler (ram:) does.  This has been fixed for V37.
```

**LVO:** -888 ($378).

### EndNotify (V36)

`(dos.doc V45, /EndNotify)`

```
   NAME
        EndNotify -- Ends a notification request (V36)

   SYNOPSIS
        EndNotify(notifystructure)
                        D1

        VOID EndNotify(struct NotifyRequest *)

   FUNCTION
        Removes a notification request.  Safe to call even if StartNotify()
        failed.  For NRF_SEND_MESSAGE, it searches your port for any messages
        about the object in question and removes and replies them before
        returning.
```

**LVO:** -894 ($37E).

### Packet-level protocol

Internally, `StartNotify()` sends `ACTION_ADD_NOTIFY` to the relevant filesystem handler; `EndNotify()` sends `ACTION_REMOVE_NOTIFY`. Notification delivery modes (set in `nr_Flags`):

- `NRF_SEND_MESSAGE` — handler `PutMsg`es a `struct NotifyMessage` to `nr_stuff.nr_Msg.nr_Port`. Message is async.
- `NRF_SEND_SIGNAL` — handler `Signal`s `nr_stuff.nr_Signal.nr_Task` with `nr_stuff.nr_Signal.nr_SignalNum`. Synchronous — the waking task must poll the file itself.
- `NRF_WAIT_REPLY` — only with `NRF_SEND_MESSAGE`; handler blocks until you reply, avoiding race conditions where the file changes again while you're still processing.
- `NRF_NOTIFY_INITIAL` — deliver an immediate notification once when notification is armed, so the handler state machine starts in the "seen a change" state.

The NotifyRequest struct layout is in `dos/notify.h` (covered in the headers reference document).

---

## 18. Record locking (V36)

### LockRecord (V36)

`(dos.doc V45, /LockRecord)`

```
   NAME
        LockRecord -- Locks a portion of a file (V36)

   SYNOPSIS
        success = LockRecord(fh,offset,length,mode,timeout)
        D0                   D1   D2     D3    D4    D5

        BOOL LockRecord(BPTR,ULONG,ULONG,ULONG,ULONG)

   FUNCTION
        This locks a portion of a file for exclusive access.  Timeout is how
        long to wait in ticks (1/50 sec) for the record to be available.

        Valid modes are:
                REC_EXCLUSIVE
                REC_EXCLUSIVE_IMMED
                REC_SHARED
                REC_SHARED_IMMED
        For the IMMED modes, the timeout is ignored.

        Record locks are tied to the filehandle used to create them.  The
        same filehandle can get any number of exclusive locks on the same
        record, for example.  These are cooperative locks, they only
        affect other people calling LockRecord().

   BUGS
        In 2.0 through 2.02 (V36), LockRecord() only worked in the ramdisk.
        Attempting to lock records on the disk filesystem causes a crash.
        This was fixed for V37.
```

**LVO:** -270 ($10E).

### LockRecords (V36)

`(dos.doc V45, /LockRecords)`

```
   NAME
        LockRecords -- Lock a series of records (V36)

   SYNOPSIS
        success = LockRecords(record_array,timeout)
        D0                       D1           D2

        BOOL LockRecords(struct RecordLock *,ULONG)

   FUNCTION
        This locks several records within a file for exclusive access.
        The wait is applied to each attempt to lock each record in the list.
        It is recommended that you always lock a set of records in the same
        order to reduce possibilities of deadlock.

        The array of RecordLock structures is terminated by an entry with
        rec_FH of NULL.
```

**LVO:** -276 ($114).

### UnLockRecord (V36)

`(dos.doc V45, /UnLockRecord)`

```
   NAME
        UnLockRecord -- Unlock a record (V36)

   SYNOPSIS
        success = UnLockRecord(fh,offset,length)
        D0                     D1   D2     D3

   FUNCTION
        This releases the specified lock on a file.  Note that you must use
        the same filehandle you used to lock the record, and offset and length
        must be the same values used to lock it.
```

**LVO:** -282 ($11A).

### UnLockRecords (V36)

`(dos.doc V45, /UnLockRecords)`

```
   NAME
        UnLockRecords -- Unlock a list of records (V36)

   SYNOPSIS
        success = UnLockRecords(record_array)
        D0                           D1
```

**LVO:** -288 ($120).

**REC_ modes:** `REC_EXCLUSIVE` (wait if needed), `REC_EXCLUSIVE_IMMED` (fail fast), `REC_SHARED`, `REC_SHARED_IMMED`.

---

## 19. DosList walking (V36)

### LockDosList (V36)

`(dos.doc V45, /LockDosList)`

```
   NAME
        LockDosList -- Locks the specified Dos Lists for use (V36)

   SYNOPSIS
        dlist = LockDosList(flags)
        D0                   D1

        struct DosList *LockDosList(ULONG)

   FUNCTION
        Locks the dos device list in preparation to walk the list.
        If the list is 'busy' then this routine will not return until it is
        available.  This routine "nests": you can call it multiple times, and
        then must unlock it the same number of times.  The dlist
        returned is NOT a valid entry: it is a special value.  Note that
        for 1.3 compatibility, it also does a Forbid() - this will probably
        be removed at some future time.

        Note for handler writers: you should never call this function with
        LDF_WRITE, since it can deadlock you (if someone has it read-locked
        and they're trying to send you a packet).  Use AttemptLockDosList()
        instead.
```

**LVO:** -654 ($28E).

### UnLockDosList (V36)

`(dos.doc V45, /UnLockDosList)`

```
   NAME
        UnLockDosList -- Unlocks the Dos List (V36)

   SYNOPSIS
        UnLockDosList(flags)
                        D1

        void UnLockDosList(ULONG)

   FUNCTION
        Unlocks the access on the Dos Device List.  You MUST pass the same
        flags you used to lock the list.
```

**LVO:** -660 ($294).

### AttemptLockDosList (V36)

`(dos.doc V45, /AttemptLockDosList)`

```
   NAME
        AttemptLockDosList -- Attempt to lock the Dos Lists for use (V36)

   SYNOPSIS
        dlist = AttemptLockDosList(flags)
        D0                          D1

   RESULT
        dlist - Pointer to the beginning of the list or NULL.  Not a valid
                node!

   BUGS
        In V36 through V39.23 dos, this would return NULL or 0x00000001 for
        failure.  Fixed in V39.24 dos (after kickstart 39.106).
```

**LVO:** -666 ($29A).

### NextDosEntry (V36)

`(dos.doc V45, /NextDosEntry)`

```
   NAME
        NextDosEntry -- Get the next Dos List entry (V36)

   SYNOPSIS
        newdlist = NextDosEntry(dlist,flags)
        D0                       D1    D2

        struct DosList *NextDosEntry(struct DosList *,ULONG)

   FUNCTION
        Find the next Dos List entry of the right type.  You MUST have locked
        the types you're looking for.  Returns NULL if there are no more of
        that type in the list.
```

**LVO:** -690 ($2B2).

### FindDosEntry (V36)

`(dos.doc V45, /FindDosEntry)`

```
   NAME
        FindDosEntry -- Finds a specific Dos List entry (V36)

   SYNOPSIS
        newdlist = FindDosEntry(dlist,name,flags)
        D0                       D1    D2   D3

        struct DosList *FindDosEntry(struct DosList *,STRPTR,ULONG)

   FUNCTION
        Locates an entry on the device list.  Starts with the entry dlist.
        NOTE: must be called with the device list locked, no references may
        be made to dlist after unlocking.
```

**LVO:** -684 ($2AC).

### AddDosEntry (V36)

`(dos.doc V45, /AddDosEntry)`

```
   NAME
        AddDosEntry -- Add a Dos List entry to the lists (V36)

   SYNOPSIS
        success = AddDosEntry(dlist)
        D0                     D1

        LONG AddDosEntry(struct DosList *)

   FUNCTION
        Adds a device, volume or assign to the dos devicelist.  Can fail if it
        conflicts with an existing entry.  Volume nodes with different
        dates and the same name CAN be added, or with names that conflict with
        devices or assigns.  Note: the dos list does NOT have to be locked to
        call this.
```

**LVO:** -678 ($2A6).

### RemDosEntry (V36)

`(dos.doc V45, /RemDosEntry)`

```
   NAME
        RemDosEntry -- Removes a Dos List entry from it's list (V36)

   SYNOPSIS
        success = RemDosEntry(dlist)
        D0                     D1

   FUNCTION
        This removes an entry from the Dos Device list.  The memory associated
        with the entry is NOT freed.  NOTE: you must have locked the Dos List
        with the appropriate flags before calling this routine.
```

**LVO:** -672 ($2A0).

### MakeDosEntry (V36)

`(dos.doc V45, /MakeDosEntry)`

```
   NAME
        MakeDosEntry -- Creates a DosList structure (V36)

   SYNOPSIS
        newdlist = MakeDosEntry(name, type)
        D0                       D1    D2

        struct DosList *MakeDosEntry(STRPTR, LONG)

   FUNCTION
        Create a DosList structure, including allocating a name and correctly
        null-terminating the BSTR.  It also sets the dol_Type field, and sets
        all other fields to 0.
```

**LVO:** -696 ($2B8).

### FreeDosEntry (V36)

`(dos.doc V45, /FreeDosEntry)`

```
   NAME
        FreeDosEntry -- Frees an entry created by MakeDosEntry (V36)
```

**LVO:** -702 ($2BE).

### IsFileSystem (V36)

`(dos.doc V45, /IsFileSystem)`

```
   NAME
        IsFileSystem -- returns whether a Dos handler is a filesystem (V36)

   SYNOPSIS
        result = IsFileSystem(name)
        D0                     D1

        BOOL IsFileSystem(STRPTR)

   FUNCTION
        Returns whether the device is a filesystem or not.  A filesystem
        supports seperate files storing information.  If the filesystem
        doesn't support this new packet, IsFileSystem() will use Lock(":",...)
        as an indicator.
```

**LVO:** -708 ($2C4).

### LDF_ type flags

Flags for `LockDosList`, `AttemptLockDosList`, `NextDosEntry`, `FindDosEntry`:

| Flag | Meaning |
|------|---------|
| `LDF_DEVICES` | Include device entries (`DLT_DEVICE`) |
| `LDF_VOLUMES` | Include volume entries (`DLT_VOLUME`) |
| `LDF_ASSIGNS` | Include assign entries (`DLT_DIRECTORY`, `DLT_LATE`, `DLT_NONBINDING`) |
| `LDF_READ` | Shared (read) lock |
| `LDF_WRITE` | Exclusive (write) lock |
| `LDF_ALL` | Devices + volumes + assigns |

OR the type bits with one of `LDF_READ` or `LDF_WRITE`. Handler code must never take `LDF_WRITE` — it can deadlock with packet delivery. Use `AttemptLockDosList(... | LDF_WRITE)` in a busy-wait loop instead.

---

## 20. Assigns (V36)

### AssignLock (V36)

`(dos.doc V45, /AssignLock)`

```
   NAME
        AssignLock -- Creates an assignment to a locked object (V36)

   SYNOPSIS
        success = AssignLock(name,lock)
        D0                    D1   D2

        BOOL AssignLock(STRPTR,BPTR)

   FUNCTION
        Sets up an assign of a name to a given lock.  Passing NULL for a lock
        cancels any outstanding assign to that name.  If an assign entry of
        that name is already on the list, this routine replaces that entry.

        NOTE: you should not use the lock in any way after making this call
        successfully.  It becomes the assign, and will be unlocked by the
        system when the assign is removed.  If you need to keep the lock,
        pass a lock from DupLock() to AssignLock().
```

**LVO:** -612 ($264).

### AssignLate (V36)

`(dos.doc V45, /AssignLate)`

```
   NAME
        AssignLate -- Creates an assignment to a specified path later (V36)

   SYNOPSIS
        success = AssignLate(name,path)
        D0                    D1   D2

   FUNCTION
        Sets up a assignment that is expanded upon the FIRST reference to the
        name.  The path (a string) would be attached to the node.  When
        the name is referenced (Open("FOO:xyzzy"...), the string will be used
        to determine where to set the assign to, and if the directory can be
        locked, the assign will act from that point on as if it had been
        created by AssignLock().

        A major advantage is assigning things to unmounted volumes, which
        will be requested upon access (useful in startup sequences).
```

**LVO:** -618 ($26A).

### AssignPath (V36)

`(dos.doc V45, /AssignPath)`

```
   NAME
        AssignPath -- Creates an assignment to a specified path (V36)

   SYNOPSIS
        success = AssignPath(name,path)
        D0                    D1   D2

   FUNCTION
        Sets up a assignment that is expanded upon EACH reference to the name.
        This is implemented through a new device list type (DLT_ASSIGNPATH).
        The path (a string) would be attached to the node.  When the name is
        referenced, the string will be used to determine where to do the open.
        No permanent lock will be part of it.  For example, you could
        AssignPath() c2: to df2:c, and references to c2: would go to df2:c,
        even if you change disks.
```

**LVO:** -624 ($270).

### AssignAdd (V36)

`(dos.doc V45, /AssignAdd)`

```
   NAME
        AssignAdd -- Adds a lock to an assign for multi-directory assigns (V36)

   SYNOPSIS
        success = AssignAdd(name,lock)
        D0                   D1   D2

   FUNCTION
        Adds a lock to an assign, making or adding to a multi-directory
        assign.  Note that this only will succeed on an assign created with
        AssignLock(), or an assign created with AssignLate() which has been
        resolved.
```

**LVO:** -630 ($276).

### RemAssignList (V36)

`(dos.doc V45, /RemAssignList)`

```
   NAME
        RemAssignList -- Remove an entry from a multi-dir assign (V36)

   SYNOPSIS
        success = RemAssignList(name,lock)
        D0                       D1   D2

   BUGS
        In V36 through V39.23 dos, it would fail to remove the first lock
        in the assign.  Fixed in V39.24 dos (after the V39.106 kickstart).
```

**LVO:** -636 ($27C).

### GetDeviceProc / FreeDeviceProc (V36)

`(dos.doc V45, /GetDeviceProc)`

```
   NAME
        GetDeviceProc -- Finds a handler to send a message to (V36)

   SYNOPSIS
        devproc = GetDeviceProc(name, devproc)
          D0                     D1     D2

        struct DevProc *GetDeviceProc(STRPTR, struct DevProc *)

   FUNCTION
        Finds the handler/filesystem to send packets regarding 'name' to.
        This may involve getting temporary locks.  It returns a structure
        that includes a lock and msgport to send to to attempt your operation.
        It also includes information on how to handle multiple-directory
        assigns (by passing the DevProc back to GetDeviceProc() until it
        returns NULL).

        The initial call to GetDeviceProc() should pass NULL for devproc.  If
        after using the returned DevProc, you get an ERROR_OBJECT_NOT_FOUND,
        and (devproc->dvp_Flags & DVPF_ASSIGN) is true, you should call
        GetDeviceProc() again, passing it the devproc structure.
```

**LVO:** -642 ($282).

`(dos.doc V45, /FreeDeviceProc)`

```
   NAME
        FreeDeviceProc -- Releases port returned by GetDeviceProc() (V36)
```

**LVO:** -648 ($288).

`GetDeviceProc()` replaces the older `DeviceProc()` (which is still present for 1.3 compat but cannot handle assigns correctly). Always use `GetDeviceProc()/FreeDeviceProc()` pair on V36+.

---

## 21. Links — MakeLink / ReadLink (V36)

### MakeLink (V36)

`(dos.doc V45, /MakeLink)`

```
   NAME
        MakeLink -- Creates a filesystem link (V36)

   SYNOPSIS
        success = MakeLink( name, dest, soft )
        D0                   D1    D2    D3

        BOOL MakeLink( STRPTR, LONG, LONG )

   FUNCTION
        Create a filesystem link from 'name' to dest.  For "soft-links",
        dest is a pointer to a null-terminated path string.  For "hard-
        links", dest is a lock (BPTR).  'soft' is FALSE for hard-links,
        non-zero otherwise.

        Soft-links are resolved at access time by a combination of the
        filesystem (by returning ERROR_IS_SOFT_LINK to dos), and by
        Dos (using ReadLink() to resolve any links that are hit).

        Hard-links are resolved by the filesystem in question.  A series
        of hard-links to a file are all equivalent to the file itself.
        If one of the links (or the original entry for the file) is
        deleted, the data remains until there are no links left.

   BUGS
        In V36, soft-links didn't work in the ROM filesystem.  This was
        fixed for V37.
```

**LVO:** -444 ($1BC).

### ReadLink (V36)

`(dos.doc V45, /ReadLink)`

```
   NAME
        ReadLink -- Reads the path for a soft filesystem link (V36)

   SYNOPSIS
        success = ReadLink( port, lock, path, buffer, size)
        D0                   D1    D2    D3     D4     D5

        BOOL ReadLink( struct MsgPort *, BPTR, STRPTR, STRPTR, ULONG)

   FUNCTION
        ReadLink() takes a lock/name pair (usually from a failed attempt
        to use them to access an object with packets), and asks the
        filesystem to find the softlink and fill buffer with the modified
        path string.  You then start the resolution process again by
        calling GetDeviceProc() with the new string from ReadLink().
```

**LVO:** -438 ($1B6).

### Hard vs soft

- **Hard links** are filesystem-internal pointers to the same data blocks. The target must exist at creation time (a lock is required). Deletion of any link is harmless until the last link goes away. Cannot cross volumes. Only supported by filesystems that implement `ACTION_MAKE_LINK` (the V37+ ROM FFS does).
- **Soft links** are text path strings stored as a file entry. They can point to anything, including non-existent paths and objects on other volumes. Resolution is dos-level: the filesystem returns `ERROR_IS_SOFT_LINK` and dos uses `ReadLink()` to fetch the target path then restarts the lookup via `GetDeviceProc()`. Soft links can dangle.

---

## 22. Date / time (V36)

### CompareDates (V36)

`(dos.doc V45, /CompareDates)`

```
   NAME
        CompareDates -- Compares two datestamps (V36)

   SYNOPSIS
        result = CompareDates(date1,date2)
        D0                     D1     D2

        LONG CompareDates(struct DateStamp *,struct DateStamp *)

   FUNCTION
        Compares two times for relative magnitide.  <0 is returned if date1 is
        later than date2, 0 if they are equal, or >0 if date2 is later than
        date1.  NOTE: this is NOT the same ordering as strcmp!
```

**LVO:** -738 ($2E2).

### DateToStr (V36)

`(dos.doc V45, /DateToStr)`

```
   NAME
        DateToStr -- Converts a DateStamp to a string (V36)

   SYNOPSIS
        success = DateToStr( datetime )
        D0                      D1

        BOOL DateToStr(struct DateTime *)

   FUNCTION
        DateToStr converts an AmigaDOS DateStamp to a human
        readable ASCII string as requested by your settings in the
        DateTime structure.

        dat_Format - a format byte which specifies the format of the
                  dat_StrDate.  This can be any of the following:

                  FORMAT_DOS:    AmigaDOS format (dd-mmm-yy).
                  FORMAT_INT:    International format (yy-mmm-dd).
                  FORMAT_USA:    American format (mm-dd-yy).
                  FORMAT_CDN:    Canadian format (dd-mm-yy).
                  FORMAT_DEF:    default format for locale.

        dat_Flags - a flags byte.  The only flag which affects this
                  function is:

                  DTF_SUBST:    If set, a string such as Today,
                                Monday, etc., will be used instead
                                of the dat_Format specification if
                                possible.
                  DTF_FUTURE:   Ignored by this function.
```

**LVO:** -744 ($2E8).

### StrToDate (V36)

`(dos.doc V45, /StrToDate)`

```
   NAME
        StrToDate -- Converts a string to a DateStamp (V36)

   SYNOPSIS
        success = StrToDate( datetime )
        D0                      D1

        BOOL StrToDate( struct DateTime * )

   FUNCTION
        Converts a human readable ASCII string into an AmigaDOS DateStamp.

        dat_Flags:
            DTF_SUBST:      ignored by this function
            DTF_FUTURE:     If set, indicates that strings such as (stored
                            in dat_StrDate) "Monday" refer to "next" monday.
                            Otherwise, if clear, strings like "Monday"
                            refer to "last" monday.

        dat_StrDate - pointer to valid string representing the date.
                      This can be a "DTF_SUBST" style string such as
                      "Today" "Tomorrow" "Monday", or it may be a string
                      as specified by the dat_Format byte.
        dat_StrTime - Pointer to a buffer which contains the time in
                      the ASCII format hh:mm:ss.
```

**LVO:** -750 ($2EE).

### DateStamp — V34 existing

`DateStamp()` (LVO -192) is V34 baseline. V36 clarifies it returns the same `struct DateStamp` pointer for pre-V36 compatibility.

---

## 23. Local variables (V36)

### GetVar (V36)

`(dos.doc V45, /GetVar)`

```
   NAME
        GetVar -- Returns the value of a local or global variable (V36)

   SYNOPSIS
        len = GetVar( name, buffer, size, flags )
        D0             D1     D2     D3    D4

        LONG GetVar( STRPTR, STRPTR, LONG, ULONG )

   FUNCTION
        Gets the value of a local or environment variable.  This stops
        putting characters into the destination when a \n is hit, unless
        GVF_BINARY_VAR is specified.  (The \n is not stored in the buffer.)

   INPUTS
        flags  - combination of type of var to get value of (low 8 bits), and
                 flags to control the behavior of this routine.  Currently
                 defined flags include:

                        GVF_GLOBAL_ONLY - tries to get a global env variable.
                        GVF_LOCAL_ONLY  - tries to get a local variable.
                        GVF_BINARY_VAR  - don't stop at \n
                        GVF_DONT_NULL_TERM - no null termination (only valid
                                          for binary variables). (V37)

                 The default is to try to get a local variable first, then
                 to try to get a global environment variable.

   RESULT
        len -   Size of environment variable.  -1 indicates that the
                variable was not defined.

   BUGS
        LV_VAR is the only type that can be global.
        Under V36, we documented (and it returned) the size of the variable,
        not the number of characters transferred.  For V37 this was changed
        to the number of characters put in the buffer.
        GVF_DONT_NULL_TERM only works for local variables under V37.  For
        V39, it also works for globals.
```

**LVO:** -906 ($38A).

### SetVar (V36)

`(dos.doc V45, /SetVar)`

```
   NAME
        SetVar -- Sets a local or environment variable (V36)

   SYNOPSIS
        success = SetVar( name, buffer, size, flags )
        D0                 D1     D2     D3    D4

   INPUTS
        flags  - combination of type of var to set (low 8 bits), and
                 flags to control the behavior of this routine.

                GVF_LOCAL_ONLY - set a local (to your process) variable.
                GVF_GLOBAL_ONLY - set a global environment variable.

                The default is to set a local environment variable.

   BUGS
        LV_VAR is the only type that can be global
```

**LVO:** -900 ($384).

### DeleteVar (V36)

`(dos.doc V45, /DeleteVar)`

```
   NAME
        DeleteVar -- Deletes a local or environment variable (V36)
```

**LVO:** -912 ($390).

### FindVar (V36)

`(dos.doc V45, /FindVar)`

```
   NAME
        FindVar -- Finds a local variable (V36)

   SYNOPSIS
        var = FindVar( name, type )
        D0              D1    D2

        struct LocalVar * FindVar(STRPTR, ULONG )
```

**LVO:** -918 ($396).

### Variable types

The low 8 bits of the `flags` parameter are a type field:

| `LV_*` type | Meaning |
|-------------|---------|
| `LV_VAR` (0) | Normal variable. The only type that can be global (stored in ENV: / ENVARC:). |
| `LV_ALIAS` (1) | Shell alias. |
| `LV_PATH` (2) | Shell command path. |
| `LV_COMMAND` (3) | Shell command. |

GVF_ flags:
- `GVF_GLOBAL_ONLY` — only check ENV:. Implies LV_VAR.
- `GVF_LOCAL_ONLY` — only check the process's local variable list (`pr_LocalVars`).
- `GVF_BINARY_VAR` — do not stop copying at newline.
- `GVF_DONT_NULL_TERM` (V37 for locals, V39 for globals) — do not append a NUL.

---

## 24. Pattern matching (V36)

### ParsePattern (V36)

`(dos.doc V45, /ParsePattern)`

```
   NAME
        ParsePattern -- Create a tokenized string for MatchPattern() (V36)

   SYNOPSIS
        IsWild = ParsePattern(Source, Dest, DestLength)
        d0                      D1     D2      D3

        LONG ParsePattern(STRPTR, STRPTR, LONG)

   FUNCTION
        Tokenizes a pattern, for use by MatchPattern().  Also indicates if
        there are any wildcards in the pattern (i.e. whether it might match
        more than one item).  Note that Dest must be at least 2 times as
        large as Source plus bytes to be (almost) 100% certain of no
        buffer overflow.

        The patterns are fairly extensive, and approximate some of the ability
        of Unix/grep "regular expression" patterns.  Here are the available
        tokens:

        ?       Matches a single character.
        #       Matches the following expression 0 or more times.
        (ab|cd) Matches any one of the items seperated by '|'.
        ~       Negates the following expression.  It matches all strings
                that do not match the expression (aka ~(foo) matches all
                strings that are not exactly "foo").
        [abc]   Character class: matches any of the characters in the class.
        [~bc]   Character class: matches any of the characters not in the
                class.
        a-z     Character range (only within character classes).
        %       Matches 0 characters always (useful in "(foo|bar|%)").
        *       Synonym for "#?", not available by default in 2.0.  Available
                as an option that can be turned on.

   RESULT
        IsWild - 1 means there were wildcards in the pattern,
                 0 means there were no wildcards in the pattern,
                -1 means there was a buffer overflow or other error

   BUGS
        In V37 this call didn't always set IoErr() to something useful on an
        error.  Fixed in V39.
```

**LVO:** -840 ($348).

### ParsePatternNoCase (V37)

`(dos.doc V45, /ParsePatternNoCase)`

```
   NAME
        ParsePatternNoCase -- Create a tokenized string for
                                                MatchPatternNoCase() (V37)

   SYNOPSIS
        IsWild = ParsePatternNoCase(Source, Dest, DestLength)
        d0                            D1     D2      D3

        LONG ParsePatternNoCase(STRPTR, STRPTR, LONG)

   FUNCTION
        Tokenizes a pattern, for use by MatchPatternNoCase().  Case-insensitive.

   BUGS
        In V37, it didn't properly convert character-classes ([x-y]) to
        upper case.  Workaround: convert the input pattern to upper case
        using ToUpper() from utility.library before calling
        ParsePatternNoCase().  Fixed in V39 dos.
```

**LVO:** -966 ($3C6).

### MatchPattern (V36)

`(dos.doc V45, /MatchPattern)`

```
   NAME
        MatchPattern --  Checks for a pattern match with a string (V36)

   SYNOPSIS
        match = MatchPattern(pat, str)
        D0                   D1   D2

        BOOL MatchPattern(STRPTR, STRPTR)

   FUNCTION
        Checks for a pattern match with a string.  The pattern must be a
        tokenized string output by ParsePattern().  This routine is
        case-sensitive.

        NOTE: this routine is highly recursive.  You must have at least
        1500 free bytes of stack to call this.
```

**LVO:** -846 ($34E).

### MatchPatternNoCase (V37)

`(dos.doc V45, /MatchPatternNoCase)`

```
   NAME
        MatchPatternNoCase --  Checks for a pattern match with a string (V37)

   SYNOPSIS
        match = MatchPatternNoCase(pat, str)
        D0                         D1   D2

   FUNCTION
        Case-insensitive version of MatchPattern().
```

**LVO:** -972 ($3CC).

### MatchFirst (V36)

`(dos.doc V45, /MatchFirst)`

```
   NAME
        MatchFirst -- Finds file that matches pattern (V36)

   SYNOPSIS
        error = MatchFirst(pat, AnchorPath)
        D0                 D1       D2

        LONG MatchFirst(STRPTR, struct AnchorPath *)

   FUNCTION
        Locates the first file or directory that matches a given pattern.
        MatchFirst() is passed your pattern (you do not pass it through
        ParsePattern() - MatchFirst() does that for you), and the control
        structure.

        MatchFirst()/MatchNext() are unusual for Dos in that they return 0
        for success, or the error code, instead of the application getting
        the error code from IoErr().

        When looking at the result of MatchFirst()/MatchNext(), the ap_Info
        field of your AnchorPath has the results of an Examine() of the object.
        You normally get the name of the object from fib_FileName, and the
        directory it's in from ap_Current->an_Lock.

        To initialize the AnchorPath structure (particularily when reusing
        it), set ap_BreakBits to the signal bits (CDEF) that you want to take
        a break on, or NULL.  ap_Flags should be set to any flags you need or
        all 0's otherwise.

        If you want to have the FULL PATH NAME of the files you found,
        allocate a buffer at the END of this structure, and put the size of
        it into ap_Strlen.
```

**LVO:** -822 ($336).

### MatchNext (V36)

`(dos.doc V45, /MatchNext)`

```
   NAME
        MatchNext - Finds the next file or directory that matches pattern (V36)

   SYNOPSIS
        error = MatchNext(AnchorPath)
        D0                    D1
```

**LVO:** -828 ($33C).

### MatchEnd (V36)

`(dos.doc V45, /MatchEnd)`

```
   NAME
        MatchEnd -- Free storage allocated for MatchFirst()/MatchNext() (V36)

   SYNOPSIS
        MatchEnd(AnchorPath)
                     D1

        VOID MatchEnd(struct AnchorPath *)

   FUNCTION
        Return all storage associated with a given search.
```

**LVO:** -834 ($342).

Always terminate a MatchFirst/MatchNext loop — even one that hits `ERROR_NO_MORE_ENTRIES` normally — with a `MatchEnd()` to free the storage the walker allocated.

---

## 25. Miscellaneous path/error helpers (V36)

### FilePart (V36)

`(dos.doc V45, /FilePart)`

```
   NAME
        FilePart -- Returns the last component of a path (V36)

   SYNOPSIS
        fileptr = FilePart( path )
        D0                   D1

        STRPTR FilePart( STRPTR )

   FUNCTION
        This function returns a pointer to the last component of a string path
        specification, which will normally be the file name.  If there is only
        one component, it returns a pointer to the beginning of the string.

   EXAMPLE
        FilePart("xxx:yyy/zzz/qqq") would return a pointer to the first 'q'.
        FilePart("xxx:yyy") would return a pointer to the first 'y').
```

**LVO:** -870 ($366).

### PathPart (V36)

`(dos.doc V45, /PathPart)`

```
   NAME
        PathPart -- Returns a pointer to the end of the next-to-last (V36)
                    component of a path.

   FUNCTION
        This function returns a pointer to the character after the next-to-last
        component of a path specification, which will normally be the directory
        name.  If there is only one component, it returns a pointer to the
        beginning of the string.

   EXAMPLE
        PathPart("xxx:yyy/zzz/qqq") would return a pointer to the last '/'.
        PathPart("xxx:yyy") would return a pointer to the first 'y').
```

**LVO:** -876 ($36C).

### AddPart (V36)

`(dos.doc V45, /AddPart)`

```
   NAME
        AddPart -- Appends a file/dir to the end of a path (V36)

   SYNOPSIS
        success = AddPart( dirname, filename, size )
        D0                   D1        D2      D3

        BOOL AddPart( STRPTR, STRPTR, ULONG )

   FUNCTION
        This function adds a file, directory, or subpath name to a directory
        path name taking into account any required separator characters.  If
        filename is a fully-qualified path it will totally replace the current
        value of dirname.

   BUGS
        Doesn't check if a subpath is legal (i.e. doesn't check for ':'s) and
        doesn't handle leading '/'s in 2.0 through 2.02 (V36).  V37 fixes
        this, allowing filename to be any path, including absolute.
```

**LVO:** -882 ($372).

### SplitName (V36)

`(dos.doc V45, /SplitName)`

```
   NAME
        SplitName -- splits out a component of a pathname into a buffer (V36)

   SYNOPSIS
        newpos = SplitName(name, separator, buf, oldpos, size)
        D0                  D1      D2      D3     D4     D5

        WORD SplitName(STRPTR, UBYTE, STRPTR, WORD, LONG)

   FUNCTION
        This routine splits out the next piece of a name from a given file
        name.  Each piece is copied into the buffer, truncating at size-1
        characters.  The new position is then returned so that it may be
        passed in to the next call to splitname.

        This function is mainly intended to support handlers.

   BUGS
        In V36 and V37, path portions greater than or equal to 'size' caused
        the last character of the portion to be lost when followed by a
        separator.  Fixed for V39 dos.
```

**LVO:** -414 ($19E).

### SameLock (V36)

`(dos.doc V45, /SameLock)`

```
   NAME
        SameLock -- returns whether two locks are on the same object (V36)

   SYNOPSIS
        value = SameLock(lock1, lock2)
        D0                D1     D2

        LONG SameLock(BPTR, BPTR)

   RESULT
        value - LOCK_SAME, LOCK_SAME_VOLUME, or LOCK_DIFFERENT

   BUGS
        In V36, it would return LOCK_SAME_VOLUME for different volumes on the
        same handler.  Also, LOCK_SAME_VOLUME was LOCK_SAME_HANDLER.
```

**LVO:** -420 ($1A4).

### SameDevice (V37)

`(dos.doc V45, /SameDevice)`

```
   NAME
        SameDevice -- Are two locks are on partitions of the device? (V37)

   SYNOPSIS
        same = SameDevice(lock1, lock2)
        D0                 D1     D2

        BOOL SameDevice( BPTR, BPTR )

   FUNCTION
        SameDevice() returns whether two locks refer to partitions that
        are on the same physical device (if it can figure it out).  This
        may be useful in writing copy routines to take advantage of
        asynchronous multi-device copies.

        Entry existed in V36 and always returned 0.
```

**LVO:** -984 ($3D8).

### Fault (V36)

`(dos.doc V45, /Fault)`

```
   NAME
        Fault -- Returns the text associated with a DOS error code (V36)

   SYNOPSIS
        len = Fault(code, header, buffer, len)
        D0           D1     D2      D3    D4

        LONG Fault(LONG, STRPTR, STRPTR, LONG)

   FUNCTION
        This routine obtains the error message text for the given error code.
        The header is prepended to the text of the error message, followed
        by a colon.  Puts a null-terminated string for the error message into
        the buffer.  By convention, error messages should be no longer than 80
        characters.
```

**LVO:** -468 ($1D4).

### PrintFault (V36)

`(dos.doc V45, /PrintFault)`

```
   NAME
        PrintFault -- Returns the text associated with a DOS error code (V36)

   SYNOPSIS
        success = PrintFault(code, header)
        D0                    D1     D2

   FUNCTION
        This is similar to the Fault() function, except that the output is
        written to the default output channel with buffered output.
```

**LVO:** -474 ($1DA).

### ErrorReport (V36)

`(dos.doc V45, /ErrorReport)`

```
   NAME
        ErrorReport -- Displays a Retry/Cancel requester for an error (V36)

   SYNOPSIS
        status = ErrorReport(code, type, arg1, device)
        D0                    D1    D2    D3     D4

        BOOL ErrorReport(LONG, LONG, ULONG, struct MsgPort *)

   FUNCTION
        Based on the request type, this routine formats the appropriate
        requester to be displayed.  If the code is not understood, it returns
        DOS_TRUE immediately.  Returns DOS_TRUE if the user selects CANCEL or
        if the attempt to put up the requester fails, or if the process
        pr_WindowPtr is -1.  Returns FALSE if the user selects Retry.

   INPUTS
        code   - Error code (ERROR_DISK_NOT_VALIDATED, ERROR_DISK_WRITE_PROTECTED,
                 ERROR_DISK_FULL, ERROR_DEVICE_NOT_MOUNTED, ERROR_NOT_A_DOS_DISK,
                 ERROR_NO_DISK, ABORT_DISK_ERROR, ABORT_BUSY)
        type   - Request type:
                 REPORT_LOCK, REPORT_FH, REPORT_VOLUME, REPORT_INSERT
        arg1   - variable parameter (see type)
        device - (Optional) Address of handler task
```

**LVO:** -480 ($1E0).

### SetIoErr (V36)

`(dos.doc V45, /SetIoErr)`

```
   NAME
        SetIoErr -- Sets the value returned by IoErr() (V36)

   SYNOPSIS
        oldcode = SetIoErr(code)
        D0                  D1
```

**LVO:** -462 ($1CE).

### Packet-level (V36 — now officially exported)

#### DoPkt (V36)

`(dos.doc V45, /DoPkt)`

```
   NAME
        DoPkt -- Send a dos packet and wait for reply (V36)

   SYNOPSIS
        result1 = DoPkt(port,action,arg1,arg2,arg3,arg4,arg5)
        D0               D1    D2    D3   D4   D5   D6   D7

        LONG DoPkt(struct MsgPort *,LONG,LONG,LONG,LONG,LONG,LONG)

   FUNCTION
        Sends a packet to a handler and waits for it to return.  DoPkt() will
        work even if the caller is an exec task and not a process; however it
        will be slower, and may fail for some additional reasons, such as
        being unable to allocate a signal.

        Only allows 5 arguments to be specified.  For more arguments (packets
        support a maximum of 7) create a packet and use SendPkt()/WaitPkt().

   BUGS
        Using DoPkt() from tasks doesn't work in V36. Use AllocDosObject(),
        PutMsg(), and WaitPort()/GetMsg() for a workaround.  In V37,
        DoPkt() will allocate, use, and free the MsgPort required.
```

**LVO:** -240 ($F0).

#### SendPkt (V36)

`(dos.doc V45, /SendPkt)`

```
   NAME
        SendPkt -- Sends a packet to a handler (V36)

   SYNOPSIS
        SendPkt(packet, port, replyport)
                 D1     D2       D3

   FUNCTION
        Sends a packet to a handler and does not wait.  All fields in the
        packet must be initialized before calling this routine.  The packet
        will be returned to replyport.

   NOTES
        Callable from a task.
```

**LVO:** -246 ($F6).

#### WaitPkt (V36)

`(dos.doc V45, /WaitPkt)`

```
   NAME
        WaitPkt -- Waits for a packet to arrive at your pr_MsgPort (V36)

   SYNOPSIS
        packet = WaitPkt()
        D0

        struct DosPacket *WaitPkt(void);

   FUNCTION
        Waits for a packet to arrive at your pr_MsgPort.  If anyone has
        installed a packet wait function in pr_PktWait, it will be called.
        The message will be automatically GetMsg()ed so that it is no longer
        on the port.  It assumes the message is a dos packet.
```

**LVO:** -252 ($FC).

#### ReplyPkt (V36)

`(dos.doc V45, /ReplyPkt)`

```
   NAME
        ReplyPkt -- replies a packet to the person who sent it to you (V36)

   SYNOPSIS
        ReplyPkt(packet, result1, result2)
                   D1      D2       D3

        void ReplyPkt(struct DosPacket *, LONG, LONG)
```

**LVO:** -258 ($102).

#### AbortPkt (V36)

`(dos.doc V45, /AbortPkt)`

```
   NAME
        AbortPkt -- Aborts an asynchronous packet, if possible. (V36)

   SYNOPSIS
        AbortPkt(port, pkt)
                  D1    D2

   FUNCTION
        This attempts to abort a packet sent earlier with SendPkt to a
        handler.  There is no guarantee that any given handler will allow
        a packet to be aborted.

   BUGS
        As of V37, this function does nothing.
```

**LVO:** -264 ($108). Note: effectively a no-op through V45 — included for completeness.

---

## 26. V39+ DOS additions

### NewLoadSeg (V36) / NewLoadSegTagList / NewLoadSegTags

`(dos.doc V45, /NewLoadSeg)`

```
   NAME
        NewLoadSeg -- Improved version of LoadSeg for stacksizes (V36)

   SYNOPSIS
        seglist = NewLoadSeg(file, tags)
        D0                    D1    D2

        BPTR NewLoadSeg(STRPTR, struct TagItem *)

   FUNCTION
        Does a LoadSeg on a file, and takes additional actions based on the
        tags supplied.

        Clears unused portions of Code and Data hunks (as well as BSS hunks).
        (This also applies to InternalLoadSeg() and LoadSeg()).

        NOTE to overlay users: NewLoadSeg() does NOT return seglist in
        both D0 and D1, as LoadSeg does.  The current ovs.asm uses LoadSeg(),
        and assumes returns are in D1.  We will support this for LoadSeg()
        ONLY.

   BUGS
        No tags are currently defined.
```

**LVO:** -768 ($300).

### InternalLoadSeg (V36)

`(dos.doc V45, /InternalLoadSeg)`

```
   NAME
        InternalLoadSeg -- Low-level load routine (V36)

   SYNOPSIS
        seglist = InternalLoadSeg(fh,table,functionarray,stack)
        D0                        D0  A0        A1        A2

        BPTR InternalLoadSeg(BPTR,BPTR,LONG *,LONG *)

   FUNCTION
        Loads from fh.  Table is used when loading an overlay, otherwise
        should be NULL.  Functionarray is a pointer to an array of functions.
        Note that the current Seek position after loading may be at any point
        after the last hunk loaded.  The filehandle will not be closed.  If a
        stacksize is encoded in the file, the size will be stuffed in the
        LONG pointed to by stack.

        If the file being loaded is an overlaid file, this will return
        -(seglist).  All other results will be positive.

   INPUTS
        fh            - Filehandle to load from.
        table         - When loading an overlay, otherwise ignored.
        functionarray - Array of function to be used for read, alloc, and free.
           FuncTable[0] ->  Actual = ReadFunc(readhandle,buffer,length),DOSBase
           FuncTable[1] ->  Memory = AllocFunc(size,flags), Execbase
           FuncTable[2] ->  FreeFunc(memory,size), Execbase
        stack         - Pointer to storage (ULONG) for stacksize.
```

**LVO:** -756 ($2F4).

### InternalUnLoadSeg (V36)

`(dos.doc V45, /InternalUnLoadSeg)`

```
   NAME
        InternalUnLoadSeg -- Unloads a seglist loaded with InternalLoadSeg() (V36)

   SYNOPSIS
        success = InternalUnLoadSeg(seglist,FreeFunc)
          D0                        D1       A1

        BOOL InternalUnLoadSeg(BPTR,void (*)(STRPTR,ULONG))
```

**LVO:** -762 ($2FA).

### SetOwner (V39)

`(dos.doc V45, /SetOwner)`

```
    NAME
        SetOwner -- Set owner information for a file or directory (V39)

    SYNOPSIS
        success = SetOwner( name, owner_info )
        D0                   D1       D2

        BOOL SetOwner (STRPTR, LONG)

    FUNCTION
        SetOwner() sets the owner information for the file or directory.
        This value is a 32-bit value that is normally split into 16 bits
        of owner user id (bits 31-16), and 16 bits of owner group id (bits
        15-0).  However, other than returning them as shown by Examine()/
        ExNext()/ExAll(), the filesystem take no interest in the values.
        These are primarily for use by networking software (clients and
        hosts), in conjunction with the FIBF_OTR_xxx and FIBF_GRP_xxx
        protection bits.

        This entrypoint did not exist in V36, so you must open at least V37
        dos.library to use it.  V37 dos.library will return FALSE to this
        call.
```

**LVO:** -996 ($3E4).

### Relabel / Inhibit (V36)

Both `Relabel` (-720 / $2D0) and `Inhibit` (-726 / $2D6) are V36 wrappers around packet-level operations that were previously only accessible via `DoPkt()`. Their synopses are documented in the autodoc; included in the LVO table below. Use cases are volume rename and filesystem freeze/thaw (needed before `Format()`).

### Format (V36)

`(dos.doc V45, /Format)`

```
   NAME
        Format -- Causes a filesystem to initialize itself (V36)

   SYNOPSIS
        success = Format(filesystem, volumename, dostype)
        D0                   D1          D2         D3

   FUNCTION
        Interface for initializing new media on a device.

        The filesystem should be inhibited before calling Format() to make
        sure you don't get an ERROR_OBJECT_IN_USE.

   BUGS
        Existed, but was non-functional in V36 dos.  (The volumename wasn't
        converted to a BSTR.)  Workaround: require V37.
```

**LVO:** -714 ($2CA).

### AddBuffers (V36)

`(dos.doc V45, /AddBuffers)`: add or remove filesystem disk-cache buffers. `success = AddBuffers(filesystem, number)`. Number may be negative. V36 FFS has a buffer-count reporting bug worked around by calling `AddBuffers(fs, -1)` first to force a refresh. **LVO:** -732 ($2DC).

### CheckSignal (V36)

Quick poll for break-signals without calling `Wait`. **LVO:** -792 ($318).

### CliInitNewcli / CliInitRun (V36)

Used internally by new shells and Run-spawned processes to set up their CLI structure from an initial packet. Only shell implementers need these. **LVOs:** CliInitNewcli -930 ($3A2), CliInitRun -936 ($3A8).

### FindSegment / AddSegment / RemSegment (V36)

Resident-list management for resident shell commands. **LVOs:** AddSegment -774 ($306), FindSegment -780 ($30C), RemSegment -786 ($312).

### RunCommand (V36)

Runs a seglist as a shell command in the current process. **LVO:** -504 ($1F8).

### FindCliProc / MaxCli / Cli (V36)

Walk the CLI process list by number. **LVOs:** Cli -492, CreateNewProc -498, RunCommand -504, FindCliProc -546 ($222), MaxCli -552 ($228).

### SetArgStr / GetArgStr (V36)

Read/write the argument string used by `ReadArgs()` by default (comes from `pr_Arguments`). **LVOs:** GetArgStr -534 ($216), SetArgStr -540 ($21C).

### SetCurrentDirName / GetCurrentDirName / SetProgramName / GetProgramName / SetPrompt / GetPrompt (V36)

String accessors on the CLI structure. **LVOs:** SetCurrentDirName -558, GetCurrentDirName -564, SetProgramName -570, GetProgramName -576, SetPrompt -582, GetPrompt -588.

### SetProgramDir / GetProgramDir (V36)

Underlies `PROGDIR:`. **LVOs:** SetProgramDir -594, GetProgramDir -600.

---

## 27. V40 / V45 DOS additions

No new `dos.library` vector slots were added between V39 and V45 (see `dos_lib.fd`: after `SetOwner` the remaining reserved slots at biases 1014, 1026, 1038, 1050 are still placeholder). The V45 autodoc file is identical to V39 plus the additional `BUGS` annotations and bug-fix notes for preexisting functions.

V45 does extend the **behaviour** of several existing calls — mostly bug fixes in `FGets`, `Flush`, `SplitName`, `ParsePatternNoCase`, `AttemptLockDosList`, `RemAssignList`, and `SetVBuf` — see the BUGS sections quoted in the individual entries above.

For full filesystem-level V45 changes (new disk protections, timezone handling in `DateStamp`, locale-aware `DateToStr`) see the filesystem-and-disk document (outside the scope of this supplement).

---

# Appendix A — Exec V36+ LVO table

All LVOs are byte offsets from the exec library base, with the library pointer in `a6`. Negative by convention.

| LVO (dec) | LVO (hex) | Function | Added |
|-----------|-----------|----------|-------|
| -528 | $210 | GetCC | V34 (clarified V36) |
| -612 | $264 | SumKickData | V34 (V36 cache note) |
| -624 | $270 | CopyMem | V36 |
| -630 | $276 | CopyMemQuick | V36 |
| -636 | $27C | CacheClearU | V37 |
| -642 | $282 | CacheClearE | V37 |
| -648 | $288 | CacheControl | V37 |
| -654 | $28E | CreateIORequest | V36 |
| -660 | $294 | DeleteIORequest | V36 |
| -666 | $29A | CreateMsgPort | V36 |
| -672 | $2A0 | DeleteMsgPort | V36 |
| -678 | $2A6 | ObtainSemaphoreShared | V36 |
| -684 | $2AC | AllocVec | V36 |
| -690 | $2B2 | FreeVec | V36 |
| -696 | $2B8 | CreatePool | V39 |
| -702 | $2BE | DeletePool | V39 |
| -708 | $2C4 | AllocPooled | V39 |
| -714 | $2CA | FreePooled | V39 |
| -720 | $2D0 | AttemptSemaphoreShared | V37 |
| -726 | $2D6 | ColdReboot | V36 |
| -732 | $2DC | StackSwap | V37 |
| -762 | $2FA | CachePreDMA | V37 |
| -768 | $300 | CachePostDMA | V37 |
| -774 | $306 | AddMemHandler | V39 |
| -780 | $30C | RemMemHandler | V39 |
| -786 | $312 | ObtainQuickVector | V39 |
| -828 | $33C | NewMinList | V45 |
| -852 | $354 | AVL_AddNode | V45 |
| -858 | $35A | AVL_RemNodeByAddress | V45 |
| -864 | $360 | AVL_RemNodeByKey | V45 |
| -870 | $366 | AVL_FindNode | V45 |
| -876 | $36C | AVL_FindPrevNodeByAddress | V45 |
| -882 | $372 | AVL_FindPrevNodeByKey | V45 |
| -888 | $378 | AVL_FindNextNodeByAddress | V45 |
| -894 | $37E | AVL_FindNextNodeByKey | V45 |
| -900 | $384 | AVL_FindFirstNode | V45 |
| -906 | $38A | AVL_FindLastNode | V45 |

---

# Appendix B — DOS V36+ LVO table

| LVO (dec) | LVO (hex) | Function | Added |
|-----------|-----------|----------|-------|
| -228 | $E4 | AllocDosObject | V36 |
| -234 | $EA | FreeDosObject | V36 |
| -240 | $F0 | DoPkt | V36 |
| -246 | $F6 | SendPkt | V36 |
| -252 | $FC | WaitPkt | V36 |
| -258 | $102 | ReplyPkt | V36 |
| -264 | $108 | AbortPkt | V36 |
| -270 | $10E | LockRecord | V36 |
| -276 | $114 | LockRecords | V36 |
| -282 | $11A | UnLockRecord | V36 |
| -288 | $120 | UnLockRecords | V36 |
| -294 | $126 | SelectInput | V36 |
| -300 | $12C | SelectOutput | V36 |
| -306 | $132 | FGetC | V36 |
| -312 | $138 | FPutC | V36 |
| -318 | $13E | UnGetC | V36 |
| -324 | $144 | FRead | V36 |
| -330 | $14A | FWrite | V36 |
| -336 | $150 | FGets | V36 |
| -342 | $156 | FPuts | V36 |
| -348 | $15C | VFWritef | V36 |
| -354 | $162 | VFPrintf | V36 |
| -360 | $168 | Flush | V36 |
| -366 | $16E | SetVBuf | V36 (impl. V39) |
| -372 | $174 | DupLockFromFH | V36 |
| -378 | $17A | OpenFromLock | V36 |
| -384 | $180 | ParentOfFH | V36 |
| -390 | $186 | ExamineFH | V36 |
| -396 | $18C | SetFileDate | V36 |
| -402 | $192 | NameFromLock | V36 |
| -408 | $198 | NameFromFH | V36 |
| -414 | $19E | SplitName | V36 |
| -420 | $1A4 | SameLock | V36 |
| -426 | $1AA | SetMode | V36 |
| -432 | $1B0 | ExAll | V36 |
| -438 | $1B6 | ReadLink | V36 |
| -444 | $1BC | MakeLink | V36 |
| -450 | $1C2 | ChangeMode | V36 |
| -456 | $1C8 | SetFileSize | V36 |
| -462 | $1CE | SetIoErr | V36 |
| -468 | $1D4 | Fault | V36 |
| -474 | $1DA | PrintFault | V36 |
| -480 | $1E0 | ErrorReport | V36 |
| -492 | $1EC | Cli | V36 |
| -498 | $1F2 | CreateNewProc | V36 |
| -504 | $1F8 | RunCommand | V36 |
| -510 | $1FE | GetConsoleTask | V36 |
| -516 | $204 | SetConsoleTask | V36 |
| -522 | $20A | GetFileSysTask | V36 |
| -528 | $210 | SetFileSysTask | V36 |
| -534 | $216 | GetArgStr | V36 |
| -540 | $21C | SetArgStr | V36 |
| -546 | $222 | FindCliProc | V36 |
| -552 | $228 | MaxCli | V36 |
| -558 | $22E | SetCurrentDirName | V36 |
| -564 | $234 | GetCurrentDirName | V36 |
| -570 | $23A | SetProgramName | V36 |
| -576 | $240 | GetProgramName | V36 |
| -582 | $246 | SetPrompt | V36 |
| -588 | $24C | GetPrompt | V36 |
| -594 | $252 | SetProgramDir | V36 |
| -600 | $258 | GetProgramDir | V36 |
| -606 | $25E | SystemTagList | V36 |
| -612 | $264 | AssignLock | V36 |
| -618 | $26A | AssignLate | V36 |
| -624 | $270 | AssignPath | V36 |
| -630 | $276 | AssignAdd | V36 |
| -636 | $27C | RemAssignList | V36 |
| -642 | $282 | GetDeviceProc | V36 |
| -648 | $288 | FreeDeviceProc | V36 |
| -654 | $28E | LockDosList | V36 |
| -660 | $294 | UnLockDosList | V36 |
| -666 | $29A | AttemptLockDosList | V36 |
| -672 | $2A0 | RemDosEntry | V36 |
| -678 | $2A6 | AddDosEntry | V36 |
| -684 | $2AC | FindDosEntry | V36 |
| -690 | $2B2 | NextDosEntry | V36 |
| -696 | $2B8 | MakeDosEntry | V36 |
| -702 | $2BE | FreeDosEntry | V36 |
| -708 | $2C4 | IsFileSystem | V36 |
| -714 | $2CA | Format | V36 (fixed V37) |
| -720 | $2D0 | Relabel | V36 |
| -726 | $2D6 | Inhibit | V36 |
| -732 | $2DC | AddBuffers | V36 |
| -738 | $2E2 | CompareDates | V36 |
| -744 | $2E8 | DateToStr | V36 |
| -750 | $2EE | StrToDate | V36 |
| -756 | $2F4 | InternalLoadSeg | V36 |
| -762 | $2FA | InternalUnLoadSeg | V36 |
| -768 | $300 | NewLoadSeg | V36 |
| -774 | $306 | AddSegment | V36 |
| -780 | $30C | FindSegment | V36 |
| -786 | $312 | RemSegment | V36 |
| -792 | $318 | CheckSignal | V36 |
| -798 | $31E | ReadArgs | V36 |
| -804 | $324 | FindArg | V36 |
| -810 | $32A | ReadItem | V36 |
| -816 | $330 | StrToLong | V36 |
| -822 | $336 | MatchFirst | V36 |
| -828 | $33C | MatchNext | V36 |
| -834 | $342 | MatchEnd | V36 |
| -840 | $348 | ParsePattern | V36 |
| -846 | $34E | MatchPattern | V36 |
| -858 | $35A | FreeArgs | V36 |
| -870 | $366 | FilePart | V36 |
| -876 | $36C | PathPart | V36 |
| -882 | $372 | AddPart | V36 |
| -888 | $378 | StartNotify | V36 |
| -894 | $37E | EndNotify | V36 |
| -900 | $384 | SetVar | V36 |
| -906 | $38A | GetVar | V36 |
| -912 | $390 | DeleteVar | V36 |
| -918 | $396 | FindVar | V36 |
| -930 | $3A2 | CliInitNewcli | V36 |
| -936 | $3A8 | CliInitRun | V36 |
| -942 | $3AE | WriteChars | V36 |
| -948 | $3B4 | PutStr | V36 |
| -954 | $3BA | VPrintf | V36 |
| -966 | $3C6 | ParsePatternNoCase | V37 |
| -972 | $3CC | MatchPatternNoCase | V37 |
| -984 | $3D8 | SameDevice | V37 (stub V36) |
| -990 | $3DE | ExAllEnd | V39 |
| -996 | $3E4 | SetOwner | V39 |

---

# Appendix C — Source map and gaps

## Primary sources used

| File | What we took from it |
|------|---------------------|
| `NDK_3.9/Documentation/Autodocs/exec.doc` | All verbatim exec autodoc quotes (V45 authoritative) |
| `NDK_3.9/Documentation/Autodocs/dos.doc` | All verbatim dos autodoc quotes (V45 authoritative) |
| `NDK_3.9/Include/fd/exec_lib.fd` | Exec LVO offsets and version banners (`*--- functions in V36 / V39 / V45 ---`) |
| `NDK_3.9/Include/fd/dos_lib.fd` | DOS LVO offsets and version banners |

## Secondary sources consulted but not quoted

- `Commodore_Amiga_Tech_Ref_Series_Amiga_ROM_Kernel_Reference_Manual_Libraries_3rd_edition.txt` — provides prose narrative for V37 APIs (Pool Manager chapter, Shell & Argument chapter, Notification chapter). Not quoted because the V45 autodocs carry the same information, and the RKM 3rd ed prose is older than the V39/V45 bug-fix annotations.
- `Commodore_Amiga_Tech_Ref_Series_Amiga_ROM_Kernel_Reference_Manual_Includes_And_Autodocs_3rd_edition_[600dpi][ocr].txt` — OCR'd version of V37 autodocs. Superseded by the V45 autodoc file in NDK 3.9; not quoted.

## Structures referenced but not reproduced here

All live in `amiga-headers-reference.md`:

- `struct StackSwapStruct` (`exec/tasks.h`)
- `struct MemHandlerData`, `struct Interrupt` for memory handlers (`exec/interrupts.h`)
- `struct AVLNode`, `AVLKey`, `AVLNODECOMP`, `AVLKEYCOMP` (`exec/avl.h`, V45)
- `struct TagItem`, standard tag handling (`utility/tagitem.h`)
- `struct RDArgs`, `struct CSource` (`dos/rdargs.h`)
- `struct AnchorPath`, pattern-walker control (`dos/dosasl.h`)
- `struct ExAllControl`, `struct ExAllData`, `ED_*` type constants (`dos/exall.h`)
- `struct NotifyRequest`, `NRF_*` flags (`dos/notify.h`)
- `struct DosList`, `DLT_*` types, `LDF_*` flags (`dos/dosextens.h`)
- `struct DevProc`, `DVPF_*` flags (`dos/filehandler.h`)
- `struct RecordLock`, `REC_*` modes (`dos/record.h`)
- `struct DateTime`, `FORMAT_*` values, `DTF_*` flags (`dos/datetime.h`)
- `struct LocalVar`, `LV_*` types, `GVF_*` flags (`dos/var.h`)
- `struct Segment` (`dos/dosextens.h`)

## Functions explicitly out of scope

- **Resources** (`cia.resource`, `disk.resource`, `battclock.resource`, `misc.resource`, `potgo.resource`, `card.resource`): covered by a parallel document.
- **`amiga.lib` link-library routines** including the Child-task protocol (`ChildFree`, `ChildOrphan`, `ChildStatus`, `ChildWait`), `BeginIO`, `RemTask`'s amiga.lib wrappers, and the obsolete `CreatePort`/`CreateExtIO` pair: these have no library LVO and live in user-space static libraries, not in the ROM.
- **`utility.library`** (tag handling, dates, hooks): own document.
- **V34 baseline**: covered by `amiga-exec-kernel.md` and `amiga-dos-filesystem-disk.md`.

## Known thin spots

- **ChildFree / ChildOrphan / ChildStatus / ChildWait**: mentioned only as out-of-scope because they are `amiga.lib`, not `exec.library`. If a future amiga.lib-focused supplement is written, those should be documented there with references to `NP_NotifyOnDeath` / `NP_Synchronous` as the modern replacements.
- **NewMinList (V45)**: declared in `exec_lib.fd` but has no autodoc entry in `exec.doc V45`. Documented here by function name, LVO, and inferred purpose only.
- **AVL tree comparator types (`AVLNODECOMP`, `AVLKEYCOMP`)**: precise prototypes are visible in the `AVL_AddNode` example code in `exec.doc V45`. Treat them as register-based hooks with the same ABI as a `utility.library` Hook callback.
- **V40 `dos.library`**: no new LVOs were added between V39 and V45 as far as the `.fd` file shows. Behavioural changes (mostly in the filesystem and shell commands rather than the library itself) are not tracked here.
- **V45 `exec.library`** beyond the AVL tree and `NewMinList`: the `.fd` file has no further exported additions. Any additional Kickstart 3.9 Exec changes would have to be gleaned from the Haage & Partner release notes, which are not among the sources quoted here.
