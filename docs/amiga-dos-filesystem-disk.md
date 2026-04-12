# AmigaDOS, Filesystem, and Disk Subsystem — Emulator Reference

A technical reference for building a hardware-accurate Amiga emulator, covering
the disk I/O stack from the floppy controller registers up through
`trackdisk.device`, `dos.library`, the filesystem handlers, and the CLI/Shell.
Intended audience: emulator authors who need to boot real Workbench disks,
run real software, and reason about the interactions between the BCPL
legacy, the Exec task model, and the on-disk format.

## How to read this document

- The AmigaDOS **manual** (Baker/Jesup et al., 3rd ed., 1991) is the primary
  source for the DOS programming model — the packet interface, the on-disk
  format, structure layouts, and the full command set. It is cited as
  `(ADOS Manual, ch. N)`.
- The **ROM Kernel Reference Manual: Libraries & Devices** (RKM L&D) is the
  primary source for `trackdisk.device`, `disk.resource`, and the Exec-level
  semantics. It is cited as `(RKM L&D, trackdisk.device)`.
- The **Includes & Autodocs** volume is the authoritative field-level source
  for C struct definitions and function signatures. Cited as
  `(Autodocs, <name>)` or `(Includes, <header>)`.
- The **Hardware Reference Manual** (HRM) and the A500/A2000 Technical
  Reference (TRM) cover the floppy controller hardware. Cited as `(HRM,
  floppy chapter)`.
- Kickstart version matters. 1.x DOS is the BCPL port of TripOS with a Lattice
  C wrapper; 2.0 is a C rewrite that preserves the BCPL calling conventions
  for backwards compatibility. Where behaviour differs between 1.x and 2.x,
  it is flagged inline — e.g. `[1.3 only]`, `[2.0 and later]`.
- Where a topic is covered by a companion document, this file cross-references
  rather than duplicates. In particular:
  - **Boot sequence, bootblock execution, Kickstart handoff**: see
    `amiga-boot-process.md`. This document describes the bootblock *layout*
    and how the filesystem is reached from it, but not the Kickstart-level
    execution details.
  - **Exec task model, MsgPort/Message mechanics, OpenDevice, OpenLibrary,
    interrupts**: see `amiga-exec-kernel.md`. Every DOS packet *is* an
    Exec message, and every DOS process *is* an Exec task; this document
    assumes you already understand those primitives.
  - **Custom chip register map, DMA, interrupts, blitter**: see
    `amiga-hardware-reference.md`. This document describes the floppy
    controller registers only to the depth required to understand what
    `trackdisk.device` is doing to the hardware.
- Emulator-relevant warnings are called out as **EMU:** sidenotes. These
  are the places where a naive "follow the docs" implementation will fail
  to run real software.

## Table of contents

1. [Architecture overview](#1-amigados-architecture-overview)
2. [The DOS packet model](#2-the-dos-packet-model)
3. [File handles and locks](#3-file-handles-and-locks)
4. [`dos.library` function reference](#4-doslibrary-function-reference)
5. [The Process struct (extension of Task)](#5-the-process-struct-extension-of-task)
6. [CLI / Shell structure](#6-cli--shell-structure)
7. [Filesystem on-disk formats (OFS and FFS)](#7-filesystem-on-disk-formats-ofs-and-ffs)
8. [Boot block](#8-boot-block)
9. [Filesystem / DOS tasks](#9-filesystem--dos-tasks)
10. [Mountlists and device nodes](#10-mountlists-and-device-nodes)
11. [`trackdisk.device`](#11-trackdiskdevice)
12. [`disk.resource`](#12-diskresource)
13. [Floppy hardware (low level)](#13-floppy-hardware-low-level)
14. [Rigid Disk Block (RDB) and hard disk](#14-rigid-disk-block-rdb-and-hard-disk)
15. [Filesystems shipped](#15-filesystems-shipped)
16. [`startup-sequence` and user-mode boot](#16-startup-sequence-and-user-mode-boot)
17. [Appendix A — Packet type table](#appendix-a--packet-type-table)
18. [Appendix B — OFS/FFS block layouts](#appendix-b--ofsffs-block-layouts)
19. [Appendix C — `dos.library` function index](#appendix-c--doslibrary-function-index)
20. [Appendix D — `trackdisk.device` command index](#appendix-d--trackdiskdevice-command-index)
21. [Gaps in the corpus](#gaps-in-the-corpus)
22. [Source map](#source-map)

---

## 1. AmigaDOS architecture overview

### 1.1 Origin story — TripOS, BCPL, and why dos.library is weird

AmigaDOS is not a native Amiga design. It is a port of **TripOS**, a research
operating system developed at the University of Cambridge in the late 1970s
and written in **BCPL**, the ancestor of B and C. Commodore (via MetaComCo)
licensed TripOS and bolted it on top of the Exec kernel to provide a
filesystem and shell for the launch Amiga in 1985. This leaves dos.library
with a set of calling conventions and data layouts that look nothing like
the rest of the Exec system, and those oddities are preserved into Kickstart
3.x for backwards compatibility.

The 1.x `dos.library` is effectively BCPL compiled with the Cambridge BCPL
compiler, glued into the Exec world by thin assembly trampolines. The 2.0
rewrite (shipping with Kickstart 2.0/V36) replaces the BCPL core with C and
68000 assembler, using Lattice C as the development environment. The
**on-the-wire interface** — the dos.library vector table, the shape of
packets sent to handlers, the BCPL pointer convention — is preserved so that
1.x-era code continues to work unchanged.

> "The original dos.library was written in BCPL, a precursor to the C
> programming language. Although dos.library was rewritten in C and assembler
> for 2.0, remnants of BCPL remain to keep dos.library backwards compatible."
> — *ADOS Manual, ch. 3, Programming on the Amiga*

From an emulator's perspective, the BCPL legacy produces three concrete
artefacts you must handle: **BPTRs**, **BSTRs**, and the **Global Vector**.

### 1.2 BPTR — the BCPL pointer

BCPL's memory model is longword-addressable. When BCPL thinks about
"address 2", it means the second longword from the base, not the third
byte. A **BPTR** is therefore a byte address shifted right by 2 — i.e.
`cpu_address / 4`.

    BPTR bptr  = cpu_address >> 2;
    byte_addr  = bptr << 2;           /* BADDR() macro */

Two consequences follow:

1. **Anything referenced by a BPTR must be longword-aligned.** The low two
   bits of the underlying CPU address must be zero. dos.library allocates
   its own internal structures via `AllocMem` to guarantee this; user code
   must use `AllocMem` (or `AllocVec`) with a size that keeps the result
   aligned.
2. **A BPTR of 0 is a distinguished sentinel**, not a valid reference to
   memory address 0. It means "no object" (NULL lock, empty SegList, and so
   on). See §3.2 on `Lock(NULL)` meaning "the root of the current volume".

`dos/dos.h` exposes `BADDR(bptr)` (= `((APTR)((ULONG)(bptr) << 2))`) and
`MKBADDR(ptr)` (= `((BPTR)(((ULONG)(ptr)) >> 2))`). All the structures in
this document that are "returned by dos.library" are BPTRs unless noted.

**EMU:** when you trace dos.library calls in an emulator, remember that a
returned `struct FileLock *` is a BPTR, not a native pointer. If you dump
the value as-is you will get an address 1/4 the actual location. Always
shift left 2 before dereferencing.

### 1.3 BSTR — the BCPL string

A **BSTR** is a BPTR whose referent is a length-prefixed byte string:

    +0 : length byte (unsigned, 0..255)
    +1 : length bytes of payload

The 2.0-era ROMs normally also null-terminate the payload (length byte
unchanged), so that a BSTR can be C-printed as long as you skip the first
byte. This is an accident of implementation, not guaranteed — handlers that
produce BSTRs are allowed to omit the trailing NUL, and you must not rely on
it.

Because the length field is a single byte, BSTRs are capped at **255
characters**. Directory names are limited to 30 characters (§7); file
comments are limited to 79 characters (fib_Comment was 116 bytes in 1.1,
reduced to 80 bytes in 1.2 — see §3.4). The "Name" BSTR fields of the
on-disk directory and file header blocks use the same 1-byte length prefix.

### 1.4 The Global Vector

BCPL compiled code does not link by symbol — it resolves inter-module
references through a **Global Vector**, an array of function pointers
indexed by a small integer. When one BCPL module calls another, it is really
doing `call Global_Vector[N]`. The 1.x dos.library holds a shared Global
Vector for its own functions and for the BCPL-written filesystem handler,
and each BCPL process gets its own `pr_GlobVec` pointing at the current
Global Vector in use.

From an Exec/C perspective the Global Vector is opaque. You open
dos.library via `OpenLibrary("dos.library", ...)` and call it through the
negative-offset library jump table like any other Exec library. But the
library stub for each dos.library function is a thin shim that swaps
registers, loads the Global Vector into `A2` (in 1.x), and calls into BCPL
code.

**2.0 and later**: the Global Vector survives purely for backwards
compatibility with any 1.x-era BCPL-written handlers still running (which
by 1992 was essentially nobody outside Commodore). The 2.0 dos.library
itself is C and assembler and does not use the Global Vector internally.
The `pr_GlobVec` field of the Process struct is preserved, and handlers
can still opt into it by setting `dn_GlobalVec` in their DeviceNode, but
all new handlers should set `dn_GlobalVec = -1` to indicate "this is a C
program, do not construct a Global Vector for me" (`filehandler.h`,
`libraries/filehandler.h` lines 108–116 in the Includes volume).

### 1.5 The packet interface — the one big architectural idea

Almost every dos.library function is a thin wrapper that builds a **DOS
packet**, sends it as an Exec message to the handler process that owns the
target device/volume, and waits for the reply. The handler — which is a
plain Exec task with the DOS Process extension (§5) — reads the packet
from its MsgPort, performs the operation, and replies.

    User program
         |
         v
    dos.library Open()/Read()/Write()/Lock()...
         |          (builds a StandardPacket)
         v
    Exec PutMsg() to pr_MsgPort of handler
         |
         v
    Handler process (Exec task)      e.g. FFS for DH0:, CON for console,
         |                                  RAM: handler, ...
         v
    Interpret dp_Type (ACTION_*), perform the work
         |
         v
    Exec ReplyMsg() to originator's pr_MsgPort
         |
         v
    Back in dos.library, extract dp_Res1/dp_Res2, return to caller

The filesystem itself is a userland(-ish) process — it is an Exec task just
like the caller, not a privileged kernel subsystem. That is why a crashing
filesystem on a 68000 Amiga merely loses its volume and pops up a Software
Failure, rather than panicking the kernel. It is also why the AmigaDOS
architecture is easy to extend: you can plug in a new filesystem (for
example, a CD-ROM ISO-9660 handler) by writing a process that understands
the `ACTION_*` packet set, adding it to the DOS device list (§9), and
everything from `dir`, `copy`, `Open`, and `Lock` through to Workbench icon
rendering just works. SFS, PFS2, and AmigaDOS 3.5's directory-cached FFS
all arrived this way.

Packets are Exec messages in disguise. `trackdisk.device` likewise sends
**IO requests** that are Exec messages. From Exec's point of view, the whole
DOS world is "some tasks passing messages to each other" — the interpretation
of the message payload is private to the DOS layer.

### 1.6 Layered picture, top to bottom

```
Application                     (Shell command, C program, WB icon launch)
     |  dos.library entry points (Open, Read, Lock, Execute, LoadSeg, ...)
     v
dos.library                     (packet-builder + bookkeeping)
     |  DosPacket / StandardPacket over Exec MsgPorts
     v
Handler Process                 (FFS/OFS/CON:/RAM:/PIPE:/CrossDOS/...)
     |                          (plain Exec task with pr_MsgPort)
     |  OpenDevice / IORequest
     v
Exec Device                     (trackdisk.device, serial.device, ...)
     |                          (shares disk hardware via disk.resource)
     v
disk.resource                   (arbitration between trackdisk and raw users)
     |
     v
Hardware                        (Paula: DSKLEN/DSKDAT/DSKBYTR/DSKSYNC/ADKCON
                                 + CIA-B: motor/step/dir/side/select
                                 + Agnus: DMA, blitter for MFM encode/decode)
```

Every layer above "Exec Device" lives in RAM and can be replaced or patched.
Everything in the "Exec Device" row and below is where an emulator has to
be faithful at the register level.

---

## 2. The DOS packet model

### 2.1 StandardPacket and DosPacket

A DOS packet is always embedded in an Exec Message so that the Exec
`PutMsg`/`ReplyMsg` plumbing can deliver it to a MsgPort. The convenience
wrapper used by dos.library and by most examples is `StandardPacket`:

    struct StandardPacket {
        struct Message    sp_Msg;    /* the Exec header */
        struct DosPacket  sp_Pkt;    /* the DOS payload */
    };

The linkage between the two halves is deliberately informal: on the way in,
the Message's `mn_Node.ln_Name` is aliased to point at `sp_Pkt`, and
`sp_Pkt.dp_Link` points back at `sp_Msg`. This lets the handler receive
"just an Exec Message", pull the packet out of `ln_Name`, and work on it.
Initialisation is:

    packet->sp_Msg.mn_Node.ln_Name = (char *)&(packet->sp_Pkt);
    packet->sp_Pkt.dp_Link         = &(packet->sp_Msg);
    packet->sp_Msg.mn_Node.ln_Type = NT_MESSAGE;
    packet->sp_Msg.mn_ReplyPort    = our_reply_port;

`(ADOS Manual, ch. 11, AmigaDOS Data Structures)`.

`StandardPacket` must be longword-aligned. Both the Message and the DosPacket
inside it must survive until the reply arrives — they are allocated by the
client and consumed by the handler, not by the dispatcher.

### 2.2 DosPacket layout

From `libraries/dosextens.h` (Includes/Autodocs, lines 79–104):

    struct DosPacket {
        struct Message *dp_Link;   /* back-pointer to the Exec Message */
        struct MsgPort *dp_Port;   /* reply port — MUST be refilled each send */
        LONG            dp_Type;   /* ACTION_* code                          */
        LONG            dp_Res1;   /* primary result (the "return value")    */
        LONG            dp_Res2;   /* secondary result (the "IoErr" value)   */
        LONG            dp_Arg1;   /* arguments, interpretation per action   */
        LONG            dp_Arg2;
        LONG            dp_Arg3;
        LONG            dp_Arg4;
        LONG            dp_Arg5;
        LONG            dp_Arg6;
        LONG            dp_Arg7;
    };

Aliases are defined for convenience:

    #define dp_Action   dp_Type    /* identical field */
    #define dp_Status   dp_Res1
    #define dp_Status2  dp_Res2
    #define dp_BufAddr  dp_Arg1

**dp_Port must be refilled every send.** When dos.library/the handler
dispatches a packet, it overwrites `dp_Port` with the sender's process ID
(i.e. the BPTR of the handler's own MsgPort) so that the reply can find its
way back. On reply, that field is left pointing at the handler, so if you
reuse the packet structure for a second send you will send the second
packet to the *handler*, not back to yourself. This is a standard source of
deadlocks when writing handler code by hand.

**Arguments are LONGs, typed per action.** Every `dp_ArgN` is 32 bits wide.
For a given action, the argument might be a BPTR, a BSTR, a LONG, a CPTR
(C-style pointer), a BOOL (DOSTRUE = -1, DOSFALSE = 0), or an `ARG1`
(meaning "the `fh_Arg1` field of the FileHandle"). The tables in §2.4 and
Appendix A spell out the per-action interpretation.

### 2.3 SendPkt, DoPkt, ReplyPkt, WaitPkt

Four dos.library routines do all packet I/O. They are thin wrappers over
Exec `PutMsg` / `GetMsg` / `ReplyMsg` / `WaitPort` that observe the DOS
conventions about which MsgPort a reply is routed to.

- `SendPkt(packet, port, replyport)`. Send a packet asynchronously. Fills in
  `dp_Port = replyport` and calls `PutMsg(port, &packet->sp_Msg)`.
- `DoPkt(port, action, arg1..arg5)`. Build a packet on the caller's stack,
  send it to `port`, wait for it to come back on the caller's own
  `pr_MsgPort`, and return `dp_Res1`. The secondary result ends up in
  `pr_Result2` so that `IoErr()` can fetch it. This is the function that
  every dos.library entry point ends up calling. `[V36/2.0+]` — see
  "BUGS" note in the AmigaDOS manual: `DoPkt` does not work correctly
  from an Exec task (as opposed to a full Process), because it needs
  `pr_MsgPort` and the waiting discipline.
- `ReplyPkt(packet, res1, res2)`. Fill in `dp_Res1/dp_Res2` and reply.
  Writes the current process's `pr_MsgPort` back into `dp_Port` so that the
  next `SendPkt` reuses the same packet correctly.
- `WaitPkt()`. Wait for a packet to arrive at `pr_MsgPort`, honouring
  `pr_PktWait` (a per-process hook function that filesystems can install
  to filter incoming messages — e.g. timer replies vs. real packets).

From the AmigaDOS Manual (summary, entry `WaitPkt`):

> Waits for a packet to arrive at your pr_MsgPort. If anyone has installed a
> pr_PktWait function, it will be called.

### 2.4 Packet categories

Packets fall into six families, as organised by the 1991 ADOS manual
(ch. 11, "Packet Types"):

1. **Basic I/O**. READ, WRITE, SEEK, FINDINPUT, FINDOUTPUT, FINDUPDATE,
   END, SET_FILE_SIZE, LOCK_RECORD, FREE_RECORD.
2. **File/directory manipulation**. LOCATE_OBJECT, FREE_LOCK, DUPLOCK,
   PARENT, EXAMINE_OBJECT, EXAMINE_NEXT, CREATE_DIR, DELETE_OBJECT,
   RENAME_OBJECT, SET_PROTECT, SET_COMMENT, SET_DATE, FH_FROM_LOCK,
   SAME_LOCK, MAKE_LINK, READ_LINK, CHANGE_MODE, COPY_DIR_FH, PARENT_FH,
   EXAMINE_ALL, EXAMINE_FH, ADD_NOTIFY, REMOVE_NOTIFY.
3. **Volume manipulation**. CURRENT_VOLUME, DISK_INFO, INFO, RENAME_DISK,
   FORMAT.
4. **Handler maintenance/control**. DIE, FLUSH, MORE_CACHE, INHIBIT,
   WRITE_PROTECT, IS_FILESYSTEM.
5. **Handler internal** (not sent by clients). READ_RETURN, WRITE_RETURN,
   TIMER, NIL.
6. **Obsolete**. DISK_CHANGE, DISK_TYPE, EVENT, GET_BLOCK, SET_MAP. Clients
   must not send these; handlers may ignore them. `ACTION_DISK_CHANGE` is
   particularly dead — the `DiskChange` command uses `ACTION_INHIBIT` to
   achieve the same effect.

A seventh family, **Console-only**, contains `ACTION_SCREEN_MODE` (raw vs.
cooked mode toggle) and `ACTION_WAIT_CHAR` (timed character read). Regular
filesystems can ignore these.

### 2.5 Walkthrough of the core actions

The following subsections describe the most important actions in packet form,
field by field. The full numeric table is in Appendix A.

#### ACTION_LOCATE_OBJECT — 8 — `Lock()`

    ARG1 : LOCK  base directory (0 = root of current volume)
    ARG2 : BSTR  relative path / name
    ARG3 : LONG  mode: ACCESS_READ/SHARED_LOCK (-2) or
                       ACCESS_WRITE/EXCLUSIVE_LOCK (-1)
    RES1 : LOCK  returned lock, 0 = failure
    RES2 : CODE  failure code

The lock in `ARG1` can be NULL, meaning "relative to the root of this
handler's current volume". Because the name in `ARG2` may itself include a
volume prefix (`Foo:bar/baz`) or multiple slashes, the filesystem does
arbitrary path resolution — dos.library does **not** parse names before
sending the packet. This is why a handler has to understand
volume-qualified paths.

An exclusive lock succeeds only if no other lock exists on the target. Once
created, it prevents any other lock on that target. The handler uses
`FileLock.fl_Key` to uniquely identify an object — usually the disk block
number of the file header or directory block. Some old applications rely
on `fl_Key` being a meaningful block number, even though it is strictly
implementation-defined.

The returned `FileLock.fl_Volume` should point at the DOS device list
volume entry. Some buggy applications chain locks off `dl_LockList` in the
volume entry; handlers should maintain it for compatibility but programs
should not rely on it.

#### ACTION_FREE_LOCK — 15 — `UnLock()`

    ARG1 : LOCK   lock to free
    RES1 : BOOL   DOSTRUE

NULL lock is legal and must be silently ignored. This is the only packet
that has a defined behaviour on a NULL argument and still returns success.

#### ACTION_EXAMINE_OBJECT — 23 — `Examine()`

    ARG1 : LOCK  lock of the object to examine
    ARG2 : BPTR  FileInfoBlock to fill in
    RES1 : BOOL  success
    RES2 : CODE  failure code

Fills in the FIB with type, size, date, name, comment, etc. Also **prepares
the FIB for a sequence of ACTION_EXAMINE_NEXT operations** on the same
lock — a directory traversal is always `Examine(lock, &fib); while
(ExNext(lock, &fib)) { ... }`.

Quirks to preserve:

- Both `fib_EntryType` and `fib_DirEntryType` must be set to the **same**
  value (different applications read different fields).
- Standard values: `ST_ROOT = 1`, `ST_USERDIR = 2`, `ST_SOFTLINK = 3`,
  `ST_LINKDIR = 4`, `ST_FILE = -3`, `ST_LINKFILE = -4`.
- Old programs test `fib_DirEntryType > 0` to mean "is a directory", so you
  must never use 0 for a directory type. Avoid 0 entirely — its meaning is
  not consistent.
- `fib_Comment` was 116 bytes in the 1.1 ROM and was shrunk to 80 bytes
  from 1.2 onward; the extra 36 bytes became `fib_Reserved`. A correct
  emulator should ignore this unless emulating 1.1.

#### ACTION_EXAMINE_NEXT — 24 — `ExNext()`

    ARG1 : LOCK  directory lock (may not be the same pointer used in Examine,
                 but is guaranteed to be on the same object)
    ARG2 : BPTR  FileInfoBlock (preserved across calls; the handler uses its
                 state fields to remember where it was)
    RES1 : BOOL  success
    RES2 : CODE  failure (ERROR_NO_MORE_ENTRIES when done)

This is the trickiest action in the whole protocol, because:

- An application can stop calling ExNext before the end and never return,
  leaking per-FIB state in the handler.
- An application can (and some did) pass a *copy* of the FIB — only the
  `fib_DiskKey` and the first 30 bytes of `fib_FileName` are guaranteed to
  have been preserved. This behaviour is deprecated but a compatible
  handler has to tolerate it.
- The handler can receive other packet types (Open, Read, ...) in between
  ExNext calls, so its state for the traversal must be keyed off the
  FileInfoBlock, not off "the last thing I did".
- The lock passed to ExNext is not always the same pointer used in the
  original Examine — it is, however, guaranteed to be a lock on the same
  object.

> "Because of these problems, ACTION_EXAMINE_NEXT is probably the trickiest
> action to write in any handler. Failure to handle any of the above cases
> can be quite disastrous." — *ADOS Manual, ch. 11*

#### ACTION_FINDINPUT / ACTION_FINDOUTPUT / ACTION_FINDUPDATE — 1005/1006/1004 — `Open()`

    ARG1 : BPTR  FileHandle to fill in (caller-allocated)
    ARG2 : LOCK  base directory
    ARG3 : BSTR  file name (relative to ARG1)
    RES1 : BOOL  success/failure
    RES2 : CODE  failure code

- FINDINPUT = MODE_OLDFILE: file must already exist, opens it with a shared
  lock (upgraded transparently to write when Write is issued — 1.x
  behaviour; 2.x is more strict).
- FINDOUTPUT = MODE_NEWFILE: exclusive lock. If the file exists, it is
  **deleted** and a new one is created.
- FINDUPDATE = MODE_READWRITE: shared lock, file created if it does not
  exist. [New in 2.0.]

The caller owns the FileHandle structure (unless using `Open()` which
allocates one). The caller must clear all fields except `fh_Pos` and
`fh_End`, which must be set to -1. The handler fills in `fh_Type` with its
own `pr_MsgPort` (so subsequent Read/Write can find the handler) and
`fh_Arg1` with an implementation-defined identifier for the object — for
the ROM filesystems, this is usually the block number of the file header.

**All subsequent I/O passes `fh_Arg1`, not the whole FileHandle.** Read,
Write, Seek, Close, SetFileSize all take `fh_Arg1` in their first argument
slot.

#### ACTION_READ — 82 (= 'R') — `Read()`

    ARG1 : ARG1  fh_Arg1 of the opened file
    ARG2 : APTR  buffer
    ARG3 : LONG  number of bytes to read
    RES1 : LONG  bytes read, 0 = EOF, -1 = error
    RES2 : CODE  failure code

The action code is literal ASCII `'R'`, a BCPL-ism: 82 decimal, 0x52. Same
for WRITE (`'W'` = 87, 0x57). If a read fails, the current file position
remains unchanged. A handler is allowed to return fewer bytes than
requested even if not at EOF — the CON: handler does this, returning one
line at a time when the user hits Return.

#### ACTION_WRITE — 87 (= 'W') — `Write()`

    ARG1 : ARG1  fh_Arg1
    ARG2 : APTR  buffer
    ARG3 : LONG  number of bytes to write
    RES1 : LONG  bytes written
    RES2 : CODE  failure code if RES1 != ARG3

Automatically extends the file. On error, the file position is not
updated (though the file may have been extended and data partially
overwritten), so that a retry from the same `fh` is meaningful.

#### ACTION_SEEK — 1008 — `Seek()`

    ARG1 : ARG1  fh_Arg1
    ARG2 : LONG  new position (can be negative)
    ARG3 : LONG  OFFSET_BEGINNING / OFFSET_CURRENT / OFFSET_END
    RES1 : LONG  old position, -1 = error
    RES2 : CODE

Seeking past EOF is an error; on error the new position is undefined. Note
that consoles are not required to support seek at all.

#### ACTION_END — 1007 — `Close()`

    ARG1 : ARG1  fh_Arg1
    RES1 : BOOL  DOSTRUE

Generally returns DOSTRUE. In 2.0, if an error is returned, DOS does *not*
deallocate the FileHandle; in 1.3, the return value is ignored.

#### ACTION_CREATE_DIR — 22 — `CreateDir()`

    ARG1 : LOCK  parent directory
    ARG2 : BSTR  name (relative to ARG1)
    RES1 : LOCK  lock on the new directory
    RES2 : CODE  failure code

#### ACTION_DELETE_OBJECT — 16 — `DeleteFile()`

    ARG1 : LOCK  parent
    ARG2 : BSTR  name
    RES1 : BOOL  success
    RES2 : CODE

For directories, DELETE_OBJECT must ensure the directory is empty first.

#### ACTION_RENAME_OBJECT — 17 — `Rename()`

    ARG1 : LOCK  source parent
    ARG2 : BSTR  source name
    ARG3 : LOCK  target parent (may differ)
    ARG4 : BSTR  target name
    RES1 : BOOL  success
    RES2 : CODE

Permitted to move files across directory boundaries on the same filesystem.
Must not allow the creation of a directory loop (renaming a directory into
a child of itself).

#### ACTION_PARENT — 29 — `ParentDir()`

    ARG1 : LOCK  object
    RES1 : LOCK  shared lock on parent, 0 = no parent (= at root)
    RES2 : CODE

#### ACTION_SET_PROTECT — 21 — `SetProtection()`

    ARG1 : (unused)
    ARG2 : LOCK  base
    ARG3 : BSTR  name
    ARG4 : LONG  new protection mask
    RES1 : BOOL  success
    RES2 : CODE

The low 4 bits are the R/W/E/D flags — **set means forbidden**. So default
for a new file is "all low bits set" = `rwed` actions all allowed. An odd
convention but preserved throughout. Other bits include `a` (archive),
which any operation that modifies the file must clear; `p` (pure), `s`
(script), `h` (hold).

#### ACTION_SET_COMMENT — 28 — `SetComment()`

    ARG2 : LOCK  base
    ARG3 : BSTR  name
    ARG4 : BSTR  comment (max 79 chars)

#### ACTION_SET_DATE — 34 — `SetFileDate()` [2.0]

    ARG1 : LOCK  parent
    ARG2 : BPTR  DateStamp (3 LONGs: days, mins, ticks)

#### ACTION_DISK_INFO — 25 — `Info()`

    ARG1 : BPTR  InfoData structure to fill
    RES1 : BOOL  success

Fills in an InfoData with capacity, free space, volume state, number of
soft errors, etc. For **console** handlers this packet has a special
meaning: it must return a pointer to the Window associated with the
handle. The Shell uses this to find the window for its `*` device when
running things like prompt customisation.

#### ACTION_INFO — 26 — `Info()` with explicit lock

    ARG1 : LOCK  lock
    ARG2 : BPTR  InfoData
    RES1 : BOOL  success

Same as ACTION_DISK_INFO but for a specific volume identified by a lock,
rather than "the current volume of this handler".

#### ACTION_RENAME_DISK — 9 — `Relabel()`

    ARG1 : BSTR  new disk name
    RES1 : BOOL  success

The handler must also update `dol_Name` of its volume node in the DOS
device list.

#### ACTION_FLUSH — 27

    RES1 : BOOL  DOSTRUE

Commit all dirty buffers before replying. Essential for anything that is
"about to eject a disk" or "about to power off".

#### ACTION_MORE_CACHE — 18 — `AddBuffers()`

    ARG1 : LONG  buffers to add (positive = add, negative = remove)
    RES1 : BOOL  DOSTRUE
    RES2 : LONG  new buffer count

OFS and FFS in 1.3 do not accept negative counts.

#### ACTION_INHIBIT — 31 — `Inhibit()`

    ARG1 : BOOL  DOSTRUE = inhibit, DOSFALSE = uninhibit
    RES1 : BOOL  success

When inhibited, the filesystem must not access the underlying media and
must error all packets. When un-inhibited, the filesystem must assume the
media has been changed — it flushes its buffers before inhibiting and
revalidates on uninhibit. The 2.0 ROMs maintain a nesting count; 1.x did
not, leading to subtle race conditions. Emulator-relevant: `DiskChange`
in 2.0 is implemented by `ACTION_INHIBIT(TRUE); ACTION_INHIBIT(FALSE)`,
not by the obsolete `ACTION_DISK_CHANGE`.

#### ACTION_IS_FILESYSTEM — 1027 — `IsFileSystem()` [2.0]

Returns DOSTRUE if the handler supports separate files (as opposed to,
say, a character device). Console handlers return DOSFALSE. dos.library
falls back to `Lock(":", SHARED_ACCESS)` if the packet returns
`ERROR_ACTION_NOT_KNOWN`.

#### ACTION_FORMAT — 1020 — `Format()` [2.0]

    ARG1 : BSTR  device name (with trailing ':')
    ARG2 : BSTR  volume name
    ARG3 : LONG  format type (filesystem-specific)
    RES1 : BOOL  success
    RES2 : CODE

Assumes the media is already low-level formatted; the filesystem writes
out whatever high-level structure is needed. For the ROM FFS, that means
bitmap block, root block, boot block.

#### ACTION_WRITE_PROTECT — 1023 [FFS]

    ARG1 : BOOL  write-protect/un-write-protect
    ARG2 : LONG  32-bit passkey
    RES1 : BOOL

Primarily intended for non-removable hard disks. A write-protected disk
can only be unprotected by passing the same passkey (or by passing any key
if the protect operation used passkey 0).

#### ACTION_LOCK_RECORD — 2008 / ACTION_FREE_RECORD — 2009 [2.0]

Byte-range record locking on a file handle. Modes are:

    0 = Exclusive
    1 = Immediate Exclusive (timeout ignored)
    2 = Shared
    3 = Immediate Shared (timeout ignored)

Timeout is in AmigaDOS ticks (1/50 s). Intended for database and
multi-process applications. The ROM FFS does support this.

#### ACTION_ADD_NOTIFY — 4097 / ACTION_REMOVE_NOTIFY — 4098 [2.0]

`StartNotify`/`EndNotify`. An application registers interest in a full
path name and is either signalled or sent a message when the file is
modified. Used by Workbench to refresh icons when files change and by
editors to detect external modifications.

    struct NotifyRequest {
        UBYTE *nr_Name;        /* application's requested path          */
        UBYTE *nr_FullName;    /* DOS-resolved absolute path (dos fills) */
        ULONG  nr_UserData;    /* opaque                                 */
        ULONG  nr_Flags;       /* NRF_SEND_MESSAGE / NRF_SEND_SIGNAL /
                                  NRF_WAIT_REPLY / NRF_NOTIFY_INITIAL    */
        union {
            struct { struct MsgPort *nr_Port; }        nr_Msg;
            struct { struct Task *nr_Task;
                     UBYTE nr_SignalNum;
                     UBYTE nr_pad[3];
                     ULONG nr_Signal; }                 nr_stuff;
        };
        ULONG          nr_Reserved[4];
        /* then internal handler-private fields */
        ULONG          nr_MsgCount;
        struct MsgPort *nr_Handler;
    };

Notification fires on actions that change the actual file contents —
`ACTION_WRITE`, `ACTION_TRUNCATE`, `ACTION_SET_DATE`, `ACTION_DELETE`,
`ACTION_RENAME`, `ACTION_FINDUPDATE`, `ACTION_FINDINPUT`,
`ACTION_FINDOUTPUT`. May also fire on `ACTION_SET_COMMENT` and
`ACTION_SET_PROTECT` but is not required to.

#### ACTION_MAKE_LINK — 1021 / ACTION_READ_LINK — 1024 [2.0]

Hard and soft links. A hard link creates a second file header block that
points at the same data blocks as the original — the filesystem chains
them at `size-10` (see §7.4). A soft link contains a path string in place
of the data block list, and the filesystem returns `ERROR_IS_SOFT_LINK`
when the caller tries to open it; the caller must then call `ReadLink`
to get the target string.

ACTION_READ_LINK is unique in that it uses a **CPTR** (C-style string,
NUL-terminated, not a BSTR) for its name argument. This is the only
packet in the whole protocol that breaks the BSTR convention, because
link target paths can be arbitrarily long and the 255-char BSTR limit
would get in the way.

#### ACTION_FH_FROM_LOCK — 1026 / ACTION_PARENT_FH — 1031 / ACTION_COPY_DIR_FH — 1030 [2.0]

The 2.0 additions that let you convert between FileHandles and FileLocks.
`FH_FROM_LOCK` *steals* the lock — after a successful call the lock is
no longer usable. `COPY_DIR_FH` (`DupLockFromFH`) returns a lock without
invalidating the file handle.

#### ACTION_EXAMINE_ALL — 1033 / ACTION_EXAMINE_FH — 1034 [2.0]

`ExAll` is the efficient bulk directory traversal. A single packet can
return multiple directory entries, one after the other, in an
`ExAllData` chain stored in a caller-supplied buffer:

    struct ExAllData {
        struct ExAllData *ed_Next;
        UBYTE *ed_Name;
        LONG   ed_Type;
        ULONG  ed_Size;
        ULONG  ed_Prot;
        ULONG  ed_Days;
        ULONG  ed_Mins;
        ULONG  ed_Ticks;
        UBYTE *ed_Comment;
    };

The amount of data returned per entry is controlled by the request type
(`ED_NAME` / `ED_TYPE` / `ED_SIZE` / `ED_PROTECTION` / `ED_DATE` /
`ED_COMMENT`) — a smaller request truncates the trailing fields of the
ExAllData struct, so the chain's stride varies. An `ExAllControl` struct
allocated with `AllocDosObject()` holds the cross-call state and
pattern-matching hook.

### 2.6 Obsolete, reserved, and console packets

- `ACTION_GET_BLOCK` (2), `ACTION_SET_MAP` (4), `ACTION_EVENT` (6),
  `ACTION_DISK_CHANGE` (33), `ACTION_DISK_TYPE` (32) are all obsolete.
  Filesystems are not expected to handle them; applications sending them
  cannot expect them to work.
- `ACTION_TIMER` (30) is internal: handlers use it to receive timer
  replies from the timer.device disguised as DOS packets, allowing a
  single `WaitPkt`/`GetMsg` loop to handle timer tick as well as real
  requests.
- `ACTION_READ_RETURN` (1001), `ACTION_WRITE_RETURN` (1002) are likewise
  internal — they are what comes back from the serial.device (and
  similar) when a handler sends an Exec IORequest disguised as a DOS
  packet. The device replies the "DOS packet" and the handler's regular
  packet loop picks it up with its action set to READ_RETURN. This is
  how a CON: or AUX: handler does asynchronous character I/O without
  running a separate event loop for each underlying device.
- `ACTION_SCREEN_MODE` (994) — console RAW/cooked toggle.
- `ACTION_WAIT_CHAR` (20) — `WaitForChar` on a console.
- `ACTION_DIE` (5) — tell a handler to quit. All 2.0 handlers must accept
  this; because existing code may hold the handler's MsgPort via
  `DeviceProc()`, the handler cannot actually disappear, but it releases
  everything it can and thenceforth errors all packets.

Packet numbers **0–2049** are reserved for Commodore. Packets **2050–2999**
are reserved for third-party developers. The rest are reserved for future
expansion. Commodore does use 2008, 2009, 4097, and 4098 in the
reserved-for-third-party range for record locks and notify, which is
documented as an explicit exception.

---

## 3. File handles and locks

### 3.1 FileHandle

    struct FileHandle {                  /* libraries/dosextens.h */
        struct Message *fh_Link;         /* reserved                     */
        struct MsgPort *fh_Port;         /* reply port, used internally  */
        struct MsgPort *fh_Type;         /* ptr to handler's MsgPort
                                            (negative if plain file)    */
        LONG            fh_Buf;          /* buffered I/O state           */
        LONG            fh_Pos;          /* current position in buffer   */
        LONG            fh_End;          /* end of valid buffer data     */
        LONG            fh_Funcs;        /* */
    #define fh_Func1 fh_Funcs
        LONG            fh_Func2;
        LONG            fh_Func3;
        LONG            fh_Args;
    #define fh_Arg1 fh_Args
        LONG            fh_Arg2;
    };

Allocation size: 52 bytes. Longword-aligned; returned as a BPTR.

The critical field for packet-level I/O is `fh_Arg1`. Every ACTION_READ,
ACTION_WRITE, ACTION_SEEK, ACTION_END, and ACTION_SET_FILE_SIZE packet
passes `fh_Arg1` as its first argument — **not** the FileHandle itself.
The handler fills `fh_Arg1` at `Open()` time with whatever identifier it
needs (for the ROM FFS, the block number of the file header); the handler
and only the handler interprets it.

`fh_Buf`, `fh_Pos`, `fh_End`, `fh_Func1..3` support the 2.0 buffered I/O
layer (`FGetC`, `FPutC`, `FGets`, `FPuts`, etc.). Before 2.0 these fields
existed but were unused by dos.library itself; applications that needed
buffering had to provide their own. The 2.0 FGets/FPutC semi-decently
prefills the buffer from the handler and uses `fh_Func1..3` as hook points
for refill/flush/close. User code must **not** touch these fields other
than `fh_Arg1`, and only for the rare case of direct packet I/O.

`fh_Type` is the handler's `pr_MsgPort`. A special value — **negative** —
signals "this is a real disk file, not a console/network handler". The
1.x code treated negative `fh_Type` as a sentinel to avoid sending
ACTION_SCREEN_MODE etc. to filesystems, which would not know what to do
with them. The 2.0 and later code preserves the convention but enforces
it less strictly.

Because `Open()` and `Read()` hide all of this machinery, most code sees
a FileHandle only as a BPTR handle for `dos.library` calls. The internals
are relevant to emulators because tools like `StackCheck` and debugger
hooks walk the FileHandle by hand.

### 3.2 FileLock

    struct FileLock {
        BPTR              fl_Link;       /* next lock in chain           */
        LONG              fl_Key;        /* implementation key (usually
                                            disk block of object)       */
        LONG              fl_Access;     /* SHARED (-2) / EXCLUSIVE (-1) */
        struct MsgPort   *fl_Task;       /* owning handler's MsgPort     */
        BPTR              fl_Volume;     /* DosList volume entry         */
    };

A **lock** is lighter than a FileHandle. It identifies a filesystem object
(a directory or a file) without opening it for I/O, and it provides the
access-mode semantics: while a shared lock exists, no exclusive lock on
the same object can be acquired, and while an exclusive lock exists, no
other lock can be acquired at all. Holding a shared lock on a directory
does **not** prevent updates to that directory — only exclusive access
blocks writes.

Lock NULL has a defined meaning: **the root of the initial filing
system**, as given by `pr_FileSystemTask` in the current process. This is
how the Shell's "current" disk works without needing a special NULL check
on every call site. `UnLock(NULL)` is a no-op that returns success.

`fl_Key` is meant to uniquely identify the object for the lifetime of the
lock. The ROM OFS/FFS use the disk block number of the directory or file
header block. This happens to be stable across reboots and is stable
while the disk is inserted, which lets you implement "recently used"
lists by saving the key/volume pair — but the AmigaDOS Manual explicitly
warns that this is an implementation detail, not a contract.

`fl_Volume` **should** point at the volume's `DeviceList` entry (the
`dol_volume` branch of the DOS list union, §9). Some diagnostic programs
expect to walk all locks on a given volume via `dl_LockList` in the
volume entry — this chaining is maintained by the ROM filesystems for
compatibility, but **no application should rely on it** because of
race conditions and it isn't portable across filesystems.

### 3.3 Shared vs. exclusive; SHARED_LOCK/ACCESS_READ vs. EXCLUSIVE_LOCK/ACCESS_WRITE

The constants are intentionally aliased:

    #define ACCESS_READ      SHARED_LOCK       /* -2 */
    #define ACCESS_WRITE     EXCLUSIVE_LOCK    /* -1 */

Historically the pair `SHARED`/`EXCLUSIVE` was used when talking about
Locks; `ACCESS_READ`/`ACCESS_WRITE` was used when talking about file Open.
They are the same thing at the packet level: any `Lock()` call is an
`ACTION_LOCATE_OBJECT` packet with mode `ACCESS_READ` (= -2) or
`ACCESS_WRITE` (= -1).

At Open time:

- MODE_OLDFILE = SHARED_LOCK (file must exist)
- MODE_NEWFILE = EXCLUSIVE_LOCK (file is (re)created)
- MODE_READWRITE = SHARED_LOCK (file created if missing) [2.0]

In OFS, the lock check is handled by the filesystem; in FFS, the same
semantics are preserved. Note that because 1.x locks did not chain across
re-insert, locking semantics only hold while the volume is in the drive —
if you UnLock and remove the disk and re-insert it, new locks may refer
to different block numbers if the FS has rewritten the disk.

### 3.4 FileInfoBlock

    struct FileInfoBlock {        /* 260 bytes */
        LONG     fib_DiskKey;        /* internal key, handler-private   */
        LONG     fib_DirEntryType;   /* ST_ROOT / ST_USERDIR / ST_FILE / ... */
        char     fib_FileName[108];  /* NUL-terminated, first byte BCPL len */
        LONG     fib_Protection;     /* hsparwed                        */
        LONG     fib_EntryType;      /* redundant — must equal fib_DirEntryType */
        LONG     fib_Size;           /* file size in bytes              */
        LONG     fib_NumBlocks;      /* file size in blocks             */
        struct DateStamp fib_Date;   /* 3 LONGs: days/mins/ticks        */
        char     fib_Comment[80];    /* see note on 116 vs 80           */
        UWORD    fib_OwnerUID;       /* V36+                           */
        UWORD    fib_OwnerGID;       /* V36+                           */
        char     fib_Reserved[32];
    };

The FIB must be longword-aligned. The 2.0 way to allocate it is
`AllocDosObject(DOS_FIB, NULL)`. The 1.x way is just `AllocMem(sizeof(struct
FileInfoBlock), MEMF_CLEAR|MEMF_PUBLIC)`. Use the allocator to give
future DOS versions room to extend the struct.

### 3.5 Directory iteration idiom

    BPTR lock = Lock("SYS:", SHARED_LOCK);
    struct FileInfoBlock *fib = AllocDosObject(DOS_FIB, NULL);
    if (Examine(lock, fib)) {
        while (ExNext(lock, fib)) {
            /* fib->fib_FileName is a NUL-terminated name */
            /* fib->fib_DirEntryType > 0  -> directory    */
            /* fib->fib_DirEntryType < 0  -> file/softlink*/
        }
        /* IoErr() will be ERROR_NO_MORE_ENTRIES when done */
    }
    FreeDosObject(DOS_FIB, fib);
    UnLock(lock);

---

## 4. `dos.library` function reference

### 4.1 Library basics

`dos.library` lives at `$0000004` + AbsExecBase + ... the same way every
other Exec library does: open it with `OpenLibrary("dos.library", 0)` (or
`36` if you need V36/2.0 features), dereference function pointers from
negative offsets off the library base. The V1.x dos.library is a BCPL
library wrapped in Exec skin; the V2.x+ dos.library is a true C library.

Your process already has dos.library open if it was launched via the CLI or
via Workbench — in both cases the launcher has called `OpenLibrary` on your
behalf and stashed the base in `DOSBase`.

    extern struct DosLibrary *DOSBase;
    BPTR fh = Open("RAM:test", MODE_NEWFILE);

### 4.2 DosLibrary struct

    struct DosLibrary {
        struct Library  dl_lib;        /* Exec library header */
        APTR            dl_Root;       /* -> RootNode         */
        APTR            dl_GV;         /* BCPL Global Vector  */
        LONG            dl_A2;         /* register dump for BCPL trampolines */
        LONG            dl_A5;
        LONG            dl_A6;
        struct ErrorString *dl_Errors; /* V36+                */
        struct timerequest *dl_TimeReq;/* V36+                */
        struct Library  *dl_UtilityBase;    /* V36+           */
        struct Library  *dl_IntuitionBase;  /* V36+           */
    };

### 4.3 RootNode

    struct RootNode {
        BPTR             rn_TaskArray;         /* array of CLI process IDs */
        BPTR             rn_ConsoleSegment;    /* SegList for CLI          */
        struct DateStamp rn_Time;
        LONG             rn_RestartSeg;        /* disk validator SegList   */
        BPTR             rn_Info;              /* -> DosInfo               */
        BPTR             rn_FileHandlerSegment;/* V36+ ROM FFS SegList     */
        struct MinList   rn_CliList;           /* V36+ new CLI list        */
        struct MsgPort  *rn_BootProc;          /* V36+ boot FS MsgPort     */
        BPTR             rn_ShellSegment;      /* V36+                     */
        LONG             rn_Flags;             /* V36+                     */
    };

`rn_TaskArray[0]` is the max number of CLIs; `rn_TaskArray[n]` is the
`pr_MsgPort` of CLI number n (or 0 if free). From 2.0, `rn_CliList` is the
preferred, unbounded replacement. Access it via `FindCliProc()` and
`MaxCli()` — never walk it directly.

### 4.4 Core function list (with signatures and one-liners)

Functions that are implemented as dispatched packets are marked "[packet]"
with the packet name they emit. Those without "[packet]" do not touch a
handler — they operate on dos.library's internal tables, on SegLists, or
on process structures.

#### 4.4.1 File and directory I/O

    BPTR   Open          (UBYTE *name, LONG accessMode)               [packet: ACTION_FINDINPUT/ACTION_FINDOUTPUT/ACTION_FINDUPDATE]
    LONG   Close         (BPTR fh)                                    [packet: ACTION_END]
    LONG   Read          (BPTR fh, APTR buffer, LONG length)          [packet: ACTION_READ]
    LONG   Write         (BPTR fh, APTR buffer, LONG length)          [packet: ACTION_WRITE]
    LONG   Seek          (BPTR fh, LONG pos, LONG mode)               [packet: ACTION_SEEK]
    LONG   SetFileSize   (BPTR fh, LONG pos, LONG mode)  [2.0]        [packet: ACTION_SET_FILE_SIZE]
    BOOL   DeleteFile    (UBYTE *name)                                [packet: ACTION_DELETE_OBJECT]
    BOOL   Rename        (UBYTE *oldname, UBYTE *newname)             [packet: ACTION_RENAME_OBJECT]

#### 4.4.2 Locks, directories, object info

    BPTR   Lock          (UBYTE *name, LONG accessMode)               [packet: ACTION_LOCATE_OBJECT]
    BOOL   UnLock        (BPTR lock)                                  [packet: ACTION_FREE_LOCK]
    BPTR   DupLock       (BPTR lock)                                  [packet: ACTION_COPY_DIR]
    BOOL   Examine       (BPTR lock, struct FileInfoBlock *fib)       [packet: ACTION_EXAMINE_OBJECT]
    BOOL   ExNext        (BPTR lock, struct FileInfoBlock *fib)       [packet: ACTION_EXAMINE_NEXT]
    BOOL   Info          (BPTR lock, struct InfoData *info)           [packet: ACTION_INFO]
    BOOL   CreateDir     (UBYTE *name) -> BPTR lock                   [packet: ACTION_CREATE_DIR]
    BPTR   ParentDir     (BPTR lock)                                  [packet: ACTION_PARENT]
    BPTR   CurrentDir    (BPTR lock)               /* installs new CurrentDir, returns old; no packet */
    BOOL   SetComment    (UBYTE *name, UBYTE *comment)                [packet: ACTION_SET_COMMENT]
    BOOL   SetProtection (UBYTE *name, LONG mask)                     [packet: ACTION_SET_PROTECT]
    BOOL   SetFileDate   (UBYTE *name, struct DateStamp *)  [2.0]     [packet: ACTION_SET_DATE]
    BOOL   IsInteractive (BPTR fh)                                    [packet: via InfoData on the handler, or direct]
    BOOL   IsFileSystem  (UBYTE *name)  [2.0]                         [packet: ACTION_IS_FILESYSTEM]

#### 4.4.3 Error and current directory

    LONG   IoErr         (void)                     /* returns pr_Result2 */
    LONG   SetIoErr      (LONG code)                /* sets pr_Result2    */
    struct DateStamp *DateStamp(struct DateStamp *) /* fills current DTS  */
    void   Delay         (LONG ticks)               /* 1/50s ticks        */
    BPTR   DeviceProc    (UBYTE *name)              /* finds handler MsgPort for a name */
    void   FreeDeviceProc(struct DevProc *)   [2.0] /* new replacement    */
    struct DevProc *GetDeviceProc(UBYTE *name, struct DevProc *prev)  [2.0]

#### 4.4.4 Executable loading

    BPTR   LoadSeg       (UBYTE *name)              /* load a hunks file */
    BOOL   UnLoadSeg     (BPTR segList)
    BPTR   NewLoadSeg    (UBYTE *name, struct TagItem *tags)  [2.0]
    BPTR   InternalLoadSeg(BPTR fh, BPTR table, LONG *funcs, LONG *stackneeds)
    BOOL   InternalUnLoadSeg(BPTR seg, void (*freefn)())

#### 4.4.5 Process creation and control

    struct MsgPort *CreateProc(UBYTE *name, LONG pri,
                               BPTR segList, LONG stackSize)
    struct Process *CreateNewProc(struct TagItem *)                       [2.0]
    struct Process *CreateNewProcTagList(struct TagItem *)                [2.0]
    void   Exit          (LONG returnCode)
    LONG   Execute       (UBYTE *string, BPTR input, BPTR output)
    BOOL   System        (UBYTE *command, struct TagItem *tags)           [2.0]
    BPTR   RunCommand    (BPTR seg, LONG stack, UBYTE *args, LONG len)    [2.0]

#### 4.4.6 CLI support

    struct CommandLineInterface *Cli(void)   /* pr_CLI of this process */
    LONG   MaxCli        (void)
    struct Process *FindCliProc(LONG n)
    BPTR   CliInitRun    (struct DosPacket *)
    BPTR   CliInitNewcli (struct DosPacket *)
    BOOL   SetConsoleTask(struct MsgPort *)
    BOOL   GetConsoleTask(void) -> struct MsgPort *
    BOOL   SetFileSysTask(struct MsgPort *)
    BOOL   GetFileSysTask(void) -> struct MsgPort *
    void   SetPrompt     (UBYTE *str)     /* CLI prompt from a program */
    UBYTE *GetPrompt     (UBYTE *buf, LONG len)
    UBYTE *GetProgramName(UBYTE *buf, LONG len)
    BOOL   SetProgramName(UBYTE *name)
    UBYTE *GetArgStr     (void)
    BOOL   SetArgStr     (UBYTE *str)
    BPTR   GetProgramDir (void)
    BPTR   SetProgramDir (BPTR lock)

#### 4.4.7 Standard streams

    BPTR   Input         (void)           /* pr_CIS */
    BPTR   Output        (void)           /* pr_COS */
    BPTR   SelectInput   (BPTR fh)        /* 2.0 */
    BPTR   SelectOutput  (BPTR fh)        /* 2.0 */
    BPTR   ErrorOutput   (void)           /* 2.0; pr_CES */
    LONG   WaitForChar   (BPTR fh, LONG timeout_us)         [packet: ACTION_WAIT_CHAR]
    BOOL   SetMode       (BPTR fh, LONG mode)               [packet: ACTION_SCREEN_MODE]
    LONG   Flush         (BPTR fh)        /* 2.0 buffered I/O flush */

#### 4.4.8 Buffered I/O [2.0]

    LONG   FGetC         (BPTR fh)
    UBYTE *FGets         (BPTR fh, UBYTE *buf, LONG len)
    LONG   FPutC         (BPTR fh, LONG ch)
    LONG   FPuts         (BPTR fh, UBYTE *str)
    LONG   UnGetC        (BPTR fh, LONG ch)
    LONG   FRead         (BPTR fh, APTR buf, LONG blocksize, LONG numblocks)
    LONG   FWrite        (BPTR fh, APTR buf, LONG blocksize, LONG numblocks)
    LONG   WriteChars    (UBYTE *buf, LONG len)
    LONG   PutStr        (UBYTE *str)
    LONG   VPrintf       (UBYTE *fmt, LONG *argarray)
    LONG   VFPrintf      (BPTR fh, UBYTE *fmt, LONG *argarray)

#### 4.4.9 Packet-level

    LONG   DoPkt         (struct MsgPort *p, LONG action, LONG a1, LONG a2,
                          LONG a3, LONG a4, LONG a5)
    LONG   DoPkt0..DoPkt4(...)             /* 2.0 convenience forms */
    BOOL   SendPkt       (struct DosPacket *, struct MsgPort *, struct MsgPort *)
    struct DosPacket *WaitPkt(void)
    void   ReplyPkt      (struct DosPacket *, LONG res1, LONG res2)
    BOOL   AbortPkt      (struct MsgPort *, struct DosPacket *)

#### 4.4.10 DOS list / device list

    struct DosList *LockDosList    (ULONG flags)       /* V36+ */
    void            UnLockDosList  (ULONG flags)
    struct DosList *NextDosEntry   (struct DosList *, ULONG flags)
    struct DosList *FindDosEntry   (struct DosList *, UBYTE *name, ULONG flags)
    struct DosList *AttemptLockDosList(ULONG flags)
    BOOL            AddDosEntry    (struct DosList *)
    BOOL            RemDosEntry    (struct DosList *)
    struct DosList *MakeDosEntry   (UBYTE *name, LONG type)
    void            FreeDosEntry   (struct DosList *)

    Flags: LDF_DEVICES, LDF_VOLUMES, LDF_ASSIGNS, LDF_READ, LDF_WRITE.

#### 4.4.11 Assigns [2.0]

    BOOL   AssignLock (UBYTE *name, BPTR lock)
    BOOL   AssignLate (UBYTE *name, UBYTE *path)
    BOOL   AssignPath (UBYTE *name, UBYTE *path)
    BOOL   AssignAdd  (UBYTE *name, BPTR lock)          /* multidir assign */
    BOOL   RemAssignList(UBYTE *name, BPTR lock)

#### 4.4.12 Command line and argument parsing [2.0]

    struct RDArgs *ReadArgs    (UBYTE *template, LONG *array,
                                struct RDArgs *override)
    void          FreeArgs     (struct RDArgs *)
    LONG          ReadItem     (UBYTE *buf, LONG maxchars, struct CSource *)

ReadArgs templates are the `/A /K /S /N /M /T /F` syntax described in
§6.3. The `?` help mechanism in every standard command is
`PrintFault(ERROR_LINE_TOO_LONG, template)`-ish — actually, it re-reads
from input with an extended help prompt. See `RDArgs.RDA_ExtHelp`.

#### 4.4.13 DOS objects

    APTR  AllocDosObject(ULONG type, struct TagItem *tags)   /* 2.0 */
    void  FreeDosObject  (ULONG type, APTR obj)

Types: `DOS_FIB`, `DOS_EXALLCONTROL`, `DOS_STDPKT`, `DOS_CLI`, `DOS_RDARGS`,
`DOS_FILEHANDLE`.

#### 4.4.14 Date, time, matching, pattern

    LONG   CompareDates  (struct DateStamp *, struct DateStamp *)
    BOOL   DateToStr     (struct DateTime *)
    BOOL   StrToDate     (struct DateTime *)
    BOOL   MatchFirst    (UBYTE *pattern, struct AnchorPath *)
    BOOL   MatchNext     (struct AnchorPath *)
    void   MatchEnd      (struct AnchorPath *)
    LONG   ParsePattern  (UBYTE *pat, UBYTE *buf, LONG len)
    BOOL   MatchPattern  (UBYTE *parsed, UBYTE *string)

### 4.5 Selected autodoc extracts

#### Open / Close

    NAME        Open -- open a file for reading or writing
    SYNOPSIS    file = Open(name, accessMode)
                BPTR Open(STRPTR, LONG)
    FUNCTION    Opens name and returns a BPTR to a FileHandle. accessMode is one of
                MODE_OLDFILE, MODE_NEWFILE, MODE_READWRITE.
    RESULT      file - BPTR to a FileHandle, NULL on error. IoErr() for code.
    BUGS        Console windows opened with MODE_NEWFILE get an interactive
                handler; those opened with MODE_OLDFILE get a read-only stream
                (this is sometimes surprising).

#### Read / Write

    NAME        Read -- read bytes of data from a file
    SYNOPSIS    actualLength = Read(file, buffer, length)
                LONG Read(BPTR, APTR, LONG)
    FUNCTION    Reads length bytes from file into buffer. Returns the actual
                count (0 = EOF, -1 = error).

    NAME        Write -- write bytes of data to a file
    SYNOPSIS    actualLength = Write(file, buffer, length)

#### Seek

    NAME        Seek -- position a file handle to a new byte offset
    SYNOPSIS    oldPosition = Seek(file, position, mode)
                LONG Seek(BPTR, LONG, LONG)
    FUNCTION    mode: OFFSET_BEGINNING, OFFSET_CURRENT, OFFSET_END.
    RESULT      Previous byte position, -1 if error. IoErr() for code.

#### Examine / ExNext

    NAME        Examine -- fill in a FileInfoBlock for an object
    SYNOPSIS    success = Examine(lock, FileInfoBlock)
    FUNCTION    Examine gets information on the file or directory of the Lock
                and fills in a FileInfoBlock structure.

    NAME        ExNext -- examine the next entry in a directory
    SYNOPSIS    success = ExNext(lock, FileInfoBlock)
    FUNCTION    Returns directory entries. Call Examine first to prime the
                traversal, then ExNext repeatedly until it returns
                FALSE with IoErr() == ERROR_NO_MORE_ENTRIES.

#### DoPkt

    NAME        DoPkt -- send a DOS packet and wait for the reply (V36)
    SYNOPSIS    result1 = DoPkt(port, action, arg1, arg2, arg3, arg4, arg5)
                LONG DoPkt(struct MsgPort *, LONG, LONG, LONG, LONG, LONG, LONG)
    FUNCTION    Builds a StandardPacket on the caller's stack, sends it to port,
                waits via pr_MsgPort, returns dp_Res1 and stashes dp_Res2 in
                pr_Result2 (accessible via IoErr()).
    BUGS        Using DoPkt() from tasks doesn't work in DOS 2.0; use
                AllocDosObject() to build packets and SendPkt/WaitPkt from
                tasks. Must not be called from anywhere but a DOS Process.

#### DeviceProc

    NAME        DeviceProc -- return the process MsgPort of a specific I/O handler
    SYNOPSIS    process = DeviceProc(name)
                struct MsgPort *DeviceProc(char *)
    FUNCTION    Returns the handler for `name`, resolving assigns. In V36,
                DeviceProc() fails on multidirectory assigns (made via
                AssignAdd); use GetDeviceProc() instead, which also handles
                late-binding and nonbinding assigns.

#### LoadSeg / UnLoadSeg

    NAME        LoadSeg -- load an executable file (hunks) into memory
    SYNOPSIS    segList = LoadSeg(name)
                BPTR LoadSeg(char *)
    FUNCTION    Reads the file as a binary load file (see §4.6) and returns
                a SegList — a BCPL-linked chain of memory segments holding
                the relocated code/data. NULL on failure (IoErr() for code).

    NAME        UnLoadSeg -- free a SegList loaded by LoadSeg
    SYNOPSIS    success = UnLoadSeg(segList)
                BOOL UnLoadSeg(BPTR)
    FUNCTION    Walks the SegList chain and FreeMems each entry. Must not be
                called while any task is executing code from the SegList.

### 4.6 SegList format

A SegList is a BCPL-linked chain. The BPTR it returns points at the
**second** longword of a memory block (see §4.7). The first longword of
the block is the size in bytes (as allocated by `AllocMem`). The field
addressed by the BPTR is `NextSeg`, another BPTR (0 = end). After that,
the segment contains the loaded image.

    size (LONG)   <- block_base   (not reached via the BPTR)
    nextSeg BPTR  <- BADDR(seg)   (what LoadSeg returns)
    code/data...

So a "SegList" is really "a chain of 'segments', each of which is a
memory block of hunks from the LoadSeg'd file". LoadSeg handles hunk
relocation at load time. CreateProc and CreateNewProc take a SegList as
input; the process's first instruction is at `BADDR(seg) + 4` — i.e. the
first code byte of the first hunk.

### 4.7 Memory allocation convention

dos.library allocates memory via `AllocMem` and uses a specific layout so
that BPTR-addressed objects can carry their length:

    +0 : LONG  BlockSize    /* total size of this block in bytes */
    +4 : LONG  FirstData    /* first byte of user data — BPTR points here */
    ...

This means a BPTR allocated by dos.library can always be "freed" by
retrieving the size from 4 bytes before the BPTR target and calling
`FreeMem`. The 2.0 world uses `AllocVec`/`FreeVec` for the same purpose.
Any structure returned as a BPTR by a dos.library call (SegLists,
FileLocks, FileHandles, BSTRs, CLIs, ...) is wrapped this way.

---

## 5. The Process struct (extension of Task)

A **Process** is a superset of an Exec `Task` with a `MsgPort` baked in
and several DOS-specific fields tacked on. It is what you get when you
call `CreateProc()` or `CreateNewProc()`, or what the CLI creates when it
`Execute`s a command.

The reason a Process *is* a Task-plus-MsgPort rather than a Task with a
MsgPort hanging off of it is that **a Process is itself a target for
packets**. The filesystem handler for DF0: is a Process. The console
handler for your Shell window is a Process. When you call `Lock("DF0:",
...)`, dos.library walks the DOS device list to find the DF0: handler's
`pr_MsgPort`, builds an ACTION_LOCATE_OBJECT packet, and sends it there.
So every DOS-aware task needs to be able to receive packets, which means
it needs a MsgPort at a well-known offset — which is `pr_MsgPort`.

    struct Process {                                /* libraries/dosextens.h */
        struct Task     pr_Task;                    /* Exec task        */
        struct MsgPort  pr_MsgPort;                 /* BPTR-addressed   */
        WORD            pr_Pad;
        BPTR            pr_SegList;                 /* process's SegList array*/
        LONG            pr_StackSize;
        APTR            pr_GlobVec;                 /* BCPL Global Vector */
        LONG            pr_TaskNum;                 /* CLI number or 0  */
        BPTR            pr_StackBase;
        LONG            pr_Result2;                 /* last IoErr()     */
        BPTR            pr_CurrentDir;              /* lock             */
        BPTR            pr_CIS;                     /* current input    */
        BPTR            pr_COS;                     /* current output   */
        APTR            pr_ConsoleTask;             /* MsgPort of CON:  */
        APTR            pr_FileSystemTask;          /* MsgPort of boot FS */
        BPTR            pr_CLI;                     /* -> CommandLineInterface */
        APTR            pr_ReturnAddr;              /* exit return address */
        APTR            pr_PktWait;                 /* GetMsg hook      */
        APTR            pr_WindowPtr;               /* error requester  */
        /* --- 2.0 additions --- */
        BPTR            pr_HomeDir;                 /* progdir:         */
        LONG            pr_Flags;
        LONG          (*pr_ExitCode)(LONG rc, LONG data);
        LONG            pr_ExitData;
        UBYTE          *pr_Arguments;               /* command args     */
        struct MinList  pr_LocalVars;               /* local env vars   */
        ULONG           pr_ShellPrivate;            /* reserved for shell */
        BPTR            pr_CES;                     /* error stream     */
    };

### 5.1 pr_MsgPort — why it is at the top

Because dos.library needs to send packets to any DOS process, the
per-process MsgPort is at a fixed offset from the task base. **The BPTR
returned by `CreateProc` points at `pr_MsgPort`**, not at `pr_Task`, so
that handlers can treat the process ID as a MsgPort directly. Going from
`pr_MsgPort` back to the Task is a subtraction of `sizeof(struct Task)`:

    process = (struct Process *)((UBYTE *)port - sizeof(struct Task));

…or equivalently, using the fact that pr_Task is the first field:

    process = (struct Process *)(((UBYTE *)CreateProc(...))
                                   - sizeof(struct Task));

`pr_MsgPort` is initialised so that arriving messages set signal bit 8 on
the task. A DOS process waits for packets by `Wait(1L << 8)` (or via
`WaitPkt`), and when signal 8 goes off, it GetMsg's from `pr_MsgPort`.

### 5.2 Key Process fields for the emulator

- **pr_SegList**. An array of SegLists used by this process. The size is
  in `pr_SegList[0]`. Entries 1 and 2 are resident code for the dos.library
  trampolines (in 1.x) or NULL (in 2.x). Entry 3 is the LoadSeg'd
  code for the process's actual program. Entries beyond that are rarely
  used. When the process exits, this array is FreeMem'd as a whole,
  which is how Exit() cleans up.
- **pr_StackSize**. Size of the process's stack in bytes. Set by the
  caller of `CreateProc`. Distinct from any additional stack the CLI
  gives a program (the `STACK` command).
- **pr_GlobVec**. BCPL Global Vector (see §1.4). For C-written 2.0
  processes, typically a private vector or `-1` to mean "do not
  construct one".
- **pr_TaskNum**. CLI invocation number (1..n), or 0 if this is not a
  CLI process. Shell commands can read this to display in the prompt
  ("1> ", "2> ").
- **pr_StackBase**. The high end of the process's stack. For C and asm
  code this is "the top". For BCPL it is "the base". When a process
  Exits via an RTS on an empty stack, control returns to the address
  just above `pr_StackBase`.
- **pr_Result2**. The secondary result — what IoErr() returns. Updated by
  every packet-dispatching function in dos.library to the `dp_Res2` of
  the last packet.
- **pr_CurrentDir**. Lock on the current directory. Modified by
  `CurrentDir()`. Lock 0 means "root of pr_FileSystemTask" — this is
  what you get on a fresh process with no CurrentDir set.
- **pr_CIS / pr_COS**. Current Input Stream / Current Output Stream.
  These are the BPTRs that `Input()` and `Output()` return. They are
  allowed to be redirected by `<` and `>` in the Shell. The original
  values are `cli_StandardInput` / `cli_StandardOutput` in the CLI struct.
  **Never touch `pr_CIS`/`pr_COS` directly** — use `Input`/`Output` and
  `SelectInput`/`SelectOutput`, because AmigaDOS internally tracks
  changes and handles the difference between a CLI redirection and a
  programmatic stream swap.
- **pr_ConsoleTask**. MsgPort of the console handler for "this window".
  When you `Open("*", ...)` or `Open("CON:", ...)` without giving a full
  spec, dos.library uses this. In a Shell child process, it is inherited
  from the parent CLI.
- **pr_FileSystemTask**. MsgPort of the filesystem process for "this
  disk". Used when you `Open()` or `Lock()` a relative name with
  `pr_CurrentDir == 0`. On Workbench boot, this is the boot filesystem;
  child processes inherit it.
- **pr_CLI**. BPTR to the CLI structure (§6), or 0 if this is not a CLI
  process. Workbench-launched programs typically have `pr_CLI == 0`
  (Workbench processes get their command line via the WBStartup message
  instead).
- **pr_ReturnAddr**. Pointer just above the return address on the
  initial stack frame. Used by `Exit()` to unwind cleanly.
- **pr_PktWait**. If non-zero, a function pointer called whenever the
  process is about to sleep in `WaitPkt`. Used by filesystems and
  handlers that multiplex multiple Exec message sources (timer replies,
  device I/O replies, real DOS packets) — the hook gets first refusal
  on every arriving message.
- **pr_WindowPtr**. Where filesystem requesters pop up. 0 means
  "default public screen", -1 means "don't pop requesters — return
  errors to the caller", positive means "this Intuition Window".
  Emulators must preserve the -1 semantics or they will hang on
  unformatted disks, missing volumes, etc. — most applications set
  `pr_WindowPtr = -1` around the critical operation and restore it
  afterwards.
- **pr_HomeDir** [2.0]. The directory the current program was loaded
  from, used by `progdir:`. Useful when a program wants to find its
  own data files.
- **pr_Flags** [2.0]. Private DOS flags.
- **pr_ExitCode / pr_ExitData** [2.0]. Cleanup hook for a program. Called
  by `Exit()` with the return code and `pr_ExitData`; may return a
  modified return code.
- **pr_Arguments** [2.0]. Null-terminated string of the raw command line
  passed to this process. `ReadArgs` parses this (or an override). You
  can modify it via `SetArgStr` but must restore it before exit.
- **pr_LocalVars** [2.0]. Used to implement process-local environment
  variables (Shell `set`/`setenv`). Access via `GetVar`, `SetVar`,
  `DeleteVar`, `FindVar`.
- **pr_ShellPrivate** [2.0]. Reserved for the Shell associated with this
  process — never touch.
- **pr_CES** [2.0]. Error stream. If NULL, errors go to `pr_COS`.
  Partially implemented in 2.0 — not all system code writes errors here.

### 5.3 Creating processes

`CreateProc(name, pri, seglist, stackSize)` is the 1.x interface. It
takes a SegList (as produced by LoadSeg) and spawns a process running
from its first hunk. The caller's SegList ownership is transferred to
the process on success; on failure, the caller still owns it.

`CreateNewProc(tags)` [2.0] is the extensible interface. Tags include
`NP_Name`, `NP_Priority`, `NP_SegList`, `NP_Entry` (alternative — raw
code pointer), `NP_StackSize`, `NP_Input`, `NP_Output`, `NP_Error`,
`NP_CurrentDir`, `NP_CommandName`, `NP_HomeDir`, `NP_Arguments`,
`NP_CloseInput`, `NP_CloseOutput`, `NP_CloseError`, `NP_ConsoleTask`,
`NP_WindowPtr`, `NP_CopyVars`, `NP_CLI` (wrap in a CLI structure).

---

## 6. CLI / Shell structure

A CLI process has `pr_CLI != 0`. The BPTR references a
`CommandLineInterface` structure:

    struct CommandLineInterface {
        LONG cli_Result2;          /* IoErr from last command             */
        BSTR cli_SetName;           /* current directory name (BSTR)       */
        BPTR cli_CommandDir;        /* search path (lock chain?)           */
        LONG cli_ReturnCode;        /* RC from last command                */
        BSTR cli_CommandName;       /* name of current command (BSTR)      */
        LONG cli_FailLevel;         /* from FAILAT                         */
        BSTR cli_Prompt;            /* current prompt string (BSTR)        */
        BPTR cli_StandardInput;     /* "original" CLI input                */
        BPTR cli_CurrentInput;      /* current CLI input (redirected?)     */
        BSTR cli_CommandFile;       /* EXECUTE script name                 */
        LONG cli_Interactive;       /* BOOL: prompts required              */
        LONG cli_Background;        /* BOOL: created by RUN                */
        BPTR cli_CurrentOutput;     /* current output                      */
        LONG cli_DefaultStack;      /* requested by STACK                  */
        BPTR cli_StandardOutput;    /* original output                     */
        BPTR cli_Module;            /* SegList of current command          */
    };

### 6.1 The interpreter loop

Pseudocode for the CLI's main loop, assembled from the relevant passages
of the AmigaDOS Manual ch. 11 and the function descriptions of
`Execute`, `RunCommand`, and `System`:

    while (!eof(cli_CurrentInput) && !ctrl_d) {
        print(cli_Prompt);
        cmd_line = Read_line(cli_CurrentInput);
        parse cmd_line into argv;
        if (argv[0] is a resident command) {
            run_resident(argv);
            continue;
        }
        /* Search cli_CommandDir, then c:, then current dir */
        path = search_path(argv[0]);
        seg  = LoadSeg(path);
        if (!seg) { error; continue; }
        cli_Module      = seg;
        cli_CommandName = bstr(argv[0]);
        pr_Arguments    = argv_after_name;
        rc = RunCommand(seg, cli_DefaultStack, args, len(args));
        cli_ReturnCode  = rc;
        UnLoadSeg(seg);
        cli_Module = 0;
        if (rc >= cli_FailLevel) break;
    }

The Shell is the 2.0 replacement for the 1.x CLI. It adds `Set`,
`Setenv`, aliases, history, command substitution (backticks),
interactive editing, and the full ReadArgs template processor. The
underlying Process structure is the same; `pr_CLI` still points at a
`CommandLineInterface`, and `pr_ShellPrivate` holds the Shell's own
bookkeeping.

### 6.2 Resident commands

The 2.0 Shell supports **resident commands** — commands whose SegList
has been pre-loaded (via `Resident` or `Internal`) and kept in memory
so they do not have to be re-LoadSeg'd on every invocation. `RunCommand`
can take either a freshly-loaded SegList or a resident one; the
difference is whether the SegList is unloaded after the command returns.

`rn_ShellSegment` and the list it chains off of are the storage for
resident Shells/commands across processes.

### 6.3 ReadArgs templates

A template is a comma-separated list of option descriptors. Each
descriptor is a name (or `abbrev=name` pair) followed by zero or more
modifier flags:

    /A   required — ReadArgs fails if not supplied
    /K   keyword — the option is named (`NAME=value` or `NAME value`)
                   and is not positional; without the keyword it is
                   not filled
    /S   switch — boolean, set if the option name appears
    /N   number — decimal; ReadArgs converts, fails on non-numeric
    /T   toggle — like /S but toggles
    /F   rest-of-line — consumes the remainder of the command line
    /M   multiple — any remaining unclaimed strings become an array
                    of pointers on this option

Example (the COPY command):

    FROM/A/M,TO/A,ALL/S,QUIET/S,BUF=BUFFER/K/N,CLONE/S,DATES/S,NOPRO/S,COM/S

When you type `COPY ?`, the CLI prompts you with this template (minus
the `/A`s and modifiers on the display line in 2.0) and waits for you to
re-enter the command line. The resulting parse populates a caller-
supplied LONG array with the values. Every option has a slot; unused
optional options get 0; `/M` slots get a pointer to an array of
`UBYTE *` pointers ending in NULL.

**`/M` + `/A` interaction**. If there are unfilled `/A` options after
parsing, ReadArgs pulls strings from the end of the previous `/M`
option to fill them. This is how COPY's `FROM/A/M,TO/A` works: typing
`COPY foo bar baz` parses as `FROM = [foo, bar]`, `TO = baz`. The last
unquoted token of the `/M` list is donated to the `/A`.

### 6.4 Standard command list

Every ROM/Shell 2.0 release ships these as resident/internal or as
commands in `C:`:

| Command | Role |
| --- | --- |
| `ADDBUFFERS` | change cache size for a filesystem (ACTION_MORE_CACHE) |
| `ALIAS` | define/list Shell aliases [2.0] |
| `ASK` | prompt the user for Y/N |
| `ASSIGN` | manage logical device/directory names |
| `AVAIL` | show memory availability |
| `BINDDRIVERS` | load expansion drivers from `Expansion/` |
| `BREAK` | send break signals to a CLI process |
| `CD` | change current directory |
| `CHANGETASKPRI` | change priority of a process |
| `COPY` | copy files |
| `CPU` | report/select CPU features |
| `DATE` | read/set system date |
| `DELETE` | delete files/directories |
| `DIR` | list directory |
| `DISKCHANGE` | notify DOS of a removable media swap |
| `DISKCOPY` | bit-copy a floppy |
| `ECHO` | write a string |
| `ED` | full-screen editor |
| `EDIT` | line-oriented editor |
| `ELSE`, `ENDIF`, `ENDCLI`, `ENDSHELL`, `ENDSKIP` | CLI flow control |
| `EVAL` | arithmetic evaluator |
| `EXECUTE` | run a script file |
| `FAILAT` | set minimum RC considered a failure |
| `FAULT` | print a dos error code description |
| `FILENOTE` | set a file comment (ACTION_SET_COMMENT) |
| `FORMAT` | format a disk |
| `GET`/`GETENV` | read a local/global env var |
| `IF`, `ELSE`, `ENDIF` | conditional |
| `INFO` | volume info |
| `INSTALL` | write a bootblock to a floppy |
| `JOIN` | concatenate files |
| `LAB`, `SKIP` | script labels and jumps |
| `LIST` | detailed directory listing |
| `LOADWB` | start Workbench |
| `LOCK` | write-protect a disk (FFS only) |
| `MAKEDIR` | create a directory |
| `MORE` | file pager |
| `MOUNT` | mount a device from a mountlist entry |
| `NEWCLI`/`NEWSHELL` | create a new CLI window |
| `PATH` | set/show Shell search path |
| `PROMPT` | customise Shell prompt |
| `PROTECT` | set protection bits |
| `QUIT` | terminate Shell |
| `RELABEL` | rename a volume |
| `REMRAD` | remove the recoverable RAM disk |
| `RENAME` | rename/move a file |
| `RESIDENT` | load a command as resident |
| `RUN` | start a command in the background |
| `SEARCH` | text search |
| `SET`/`UNSET`/`SETENV`/`UNSETENV` | env vars |
| `SKIP` | script jump |
| `SORT` | line sort |
| `STACK` | set CLI stack |
| `STATUS` | list active CLI processes |
| `TIME` | show/set time |
| `TYPE` | print a file |
| `VERSION` | report OS/library version |
| `WAIT` | sleep for a number of seconds |
| `WHICH` | locate a command in the search path |

The 2.0 Shell implements many of these **internally** — the Shell
process recognises the command name and runs its own handler code
without LoadSeg'ing a binary. This is faster and smaller.

---

## 7. Filesystem on-disk formats (OFS and FFS)

### 7.1 Shared facts

All Amiga filesystems based on the ROM FFS/OFS family share the
following conventions:

- **Block size**. 512 bytes (128 longwords). Encoded in the DosEnvec as
  `de_SizeBlock` = 128 longwords.
- **Blocks per disk**. A standard DD 3.5" Amiga floppy has 80 cylinders
  × 2 heads × 11 sectors = 1760 blocks (880 KB of data area). High
  density adds another 11 sectors to give 3520 blocks / 1760 KB.
- **Tree structure**. A pure tree — no hard filesystem-level loops.
  (Hard links in FFS [2.0+] do create multiple headers pointing at the
  same data, but the directory graph remains a tree.)
- **Redundancy**. Every block carries a checksum, every file header
  carries a back-pointer to its parent directory, every data block in
  OFS carries a back-pointer to the file header. This is how DiskDoctor
  can reconstruct most of a damaged disk.
- **Hashing**. Directory lookups hash the file name into a 72-entry
  table in the parent directory block; collisions are handled by
  chaining through `HashChain` fields in the user directory / file
  header blocks.

The fixed location of the root block is derived from the DosEnvec
geometry. For a standard 880 KB floppy:

    blocksPerCyl  = de_BlocksPerTrack * de_Surfaces
                  = 11 * 2 = 22
    blocksPerDisk = blocksPerCyl * (de_HighCyl - de_LowCyl + 1)
                  = 22 * 80 = 1760
    root          = (blocksPerDisk - 1 + de_Reserved) >> 1
                  = (1760 - 1 + 2) >> 1
                  = 880
    bytesPerBlock = de_SizeBlock << 2 = 128 << 2 = 512

Root block 880 is **in the middle** of the disk (cylinder 40, head 0).
This is deliberate: on a two-headed floppy, placing the root at the
middle minimises worst-case seek distance for directory-heavy workloads.

`de_Reserved` is usually 2, because blocks 0 and 1 are the bootblock.

### 7.2 Block types (high-level)

| Type (T_) | Sec type (ST_) | Meaning |
| --- | --- | --- |
| T_SHORT (2) | ST_ROOT (1) | Root directory block |
| T_SHORT (2) | ST_USERDIR (2) | User directory block |
| T_SHORT (2) | ST_FILE (-3) | File header block |
| T_SHORT (2) | ST_SOFTLINK (3) | Soft link header [2.0] |
| T_SHORT (2) | ST_LINKFILE (-4) | Hard link to file [2.0] |
| T_SHORT (2) | ST_LINKDIR (4) | Hard link to directory [2.0] |
| T_LIST (16) | ST_FILE (-3) | File extension / list block |
| T_DATA (8) | — | OFS data block (with header) |
| — | — | FFS data block (raw 512 bytes) |
| — | — | Bitmap block (just longwords + checksum) |
| — | — | Bitmap extension block |
| — | — | Bootblock (non-FS layout) |

The "primary type" is always at offset 0 of the block; the "secondary
type" is always at offset `size-1` (i.e. the last longword of the
block). For a 512-byte/128-longword block, that is offset 508.

### 7.3 Root block

The root block is the entry point to the whole directory tree and the
source of the disk label. OFS and FFS differ slightly in its layout —
FFS extends it with modification and creation timestamps; OFS only has
a single "last altered" timestamp.

**OFS root block** (ADOS Manual fig. 9-A):

    offset (longwords)  name           meaning
    0                   T_SHORT        primary type = 2
    1                   0              header key (0 for root)
    2                   0              highest seq number (0 for root)
    3                   HTSIZE         hash table size (blocksize - 56 = 72)
    4                   0              reserved
    5                   CHECKSUM       balance-to-0 checksum
    6 .. HTSIZE+5       HASH_TABLE     72 longwords of block numbers
    SIZE-50             BMFLAG         TRUE if on-disk bitmap is valid
    SIZE-49 .. SIZE-25  BITMAP_KEYS    25 longwords of bitmap block numbers
    SIZE-24             0              reserved
    SIZE-23             DAYS           last-modified date
    SIZE-22             MINS
    SIZE-21             TICKS
    SIZE-20 .. SIZE-8   DISK_NAME      BCPL string (up to 30 chars)
    SIZE-7              CREATE_DAYS    volume creation date
    SIZE-6              CREATE_MINS
    SIZE-5              CREATE_TICKS
    SIZE-4              0              next hashchain entry (always 0)
    SIZE-3              0              parent (always 0)
    SIZE-2              0              extension (always 0)
    SIZE-1              ST_ROOT        secondary type = 1

All offsets are in **longwords** from the start of the block. `SIZE` is
the block size in longwords (128 for a 512-byte block), so `SIZE-1` =
127 = byte offset 508, `SIZE-50` = 78 = byte offset 312, etc.

`HTSIZE` is the hash table size in longwords: `blocksize_in_longwords -
56` = 128 - 56 = 72. So the hash table occupies offsets 6..77 (72
longwords), and the tail metadata starts at offset 78 (SIZE-50 = 78).

**FFS root block** (ADOS Manual fig. 9-B):

    0                   T_SHORT
    1                   0
    2                   0
    3                   HTSIZE
    4                   0              reserved
    5                   CHECKSUM
    6 .. 77             HASH_TABLE
    SIZE-50             BMFLAG
    SIZE-49 .. SIZE-25  BITMAP_KEYS
    SIZE-24             BITMAP_EXTEND  0 or block number of more bitmap keys
    SIZE-23             DIR_ALTERED    DateStamp (3 LONGs: days/mins/ticks)
    SIZE-21             ...
    SIZE-20 .. SIZE-11  DISK_NAME      BCPL string
    SIZE-10             DISK_ALTERED   DateStamp (3 LONGs) — last change to any
    SIZE-8              ...              file/partition section
    SIZE-7              DISK_MADE      DateStamp (3 LONGs) — partition first
    SIZE-5              ...              formatted
    SIZE-4              0
    SIZE-3              0
    SIZE-2              0
    SIZE-1              ST_ROOT = 1

FFS therefore tracks three dates:

- **DIR_ALTERED**: last modification to the root directory itself
  (i.e. a file added, removed, or renamed in the root).
- **DISK_ALTERED**: last modification to any file on the partition.
- **DISK_MADE**: when the partition was first formatted.

The OFS root block has only a single "altered" date. Tools that work
with both formats therefore see different semantics.

### 7.4 User directory block

A user directory block has `T_SHORT` / `ST_USERDIR`. It looks almost
identical to a root block except for the tail metadata.

**OFS user directory block** (fig. 9-C):

    0                   T_SHORT
    1                   OWN_KEY           block number of self (consistency)
    2                   0                 highest seq = 0
    3                   0
    4                   0
    5                   CHECKSUM
    6 .. 77             HASH_TABLE        72 longwords
    SIZE-50             (spare)
    SIZE-48             PROTECT           protection bits
    SIZE-47             0
    SIZE-46 .. SIZE-24  COMMENT           BCPL string (116 in 1.1, 80 in 1.2+)
    SIZE-23             DAYS              creation date
    SIZE-22             MINS
    SIZE-21             TICKS
    SIZE-20 .. SIZE-5   DIR_NAME          BCPL string (up to 30 chars)
    SIZE-4              HASHCHAIN         next dir/file with same hash
    SIZE-3              PARENT            block # of parent directory
    SIZE-2              0                 extension
    SIZE-1              ST_USERDIR = 2

**FFS user directory block** (fig. 9-D) is identical except that the
comment area is fixed at 80 bytes from the start, and the date field is
a single `DIR_CREATED` DateStamp (3 longwords) at `SIZE-23..SIZE-21`.

Notable differences from OFS:

- **Hash chain ordering**. OFS inserts a new file at the *head* of its
  hash chain. FFS inserts it in ascending block-order on the chain.
  This means OFS hashes are not stable across `delete+create`, but FFS
  hashes are. DiskDoctor-like tools must handle both.
- **Own key**. Both OFS and FFS store their own block number in
  `OWN_KEY` for consistency checking. `OWN_KEY` of a root block is 0
  (not the root block number), because the root is known at a
  fixed location.

#### Hashing algorithm

    int hash(const char *name) {
        int val = len = *name++;
        for (int i = 0; i < len; i++)
            val = ((val * 13) + toupper(*name++)) & 0x7ff;
        return val % 72;
    }

- `name` points at a BCPL string (length byte + bytes).
- Multiplication by 13, masked to 11 bits, then modulo 72.
- Result 0..71 is the index into the HASH_TABLE.
- Case folding is ASCII only. **International mode** (DOS\2/DOS\3)
  extends it to handle ISO-Latin-1 upper/lower case (see §15).

### 7.5 File header block

A file is described by a **file header** (`T_SHORT` / `ST_FILE`). The
header holds the name, date, size, protection, comment, parent back-
pointer, and an array of pointers to the file's data blocks.

    0                   T_SHORT
    1                   OWN_KEY           self block number
    2                   HIGHEST_SEQ       total number of data blocks in file
    3                   DATA_SIZE         number of data block slots used
    4                   FIRST_DATA        block # of first data block
    5                   CHECKSUM
    6 .. SIZE-51        DATA_BLOCK_LIST   list of data block numbers, filled
                                            *downward* from offset SIZE-51
    SIZE-50             (spare)
    SIZE-48             PROTECT           protection bits
    SIZE-47             BYTE_SIZE         size of file in bytes
    SIZE-46 .. SIZE-24  COMMENT           BCPL string (80 bytes)
    SIZE-23             DAYS              creation date
    SIZE-22             MINS
    SIZE-21             TICKS
    SIZE-20 .. SIZE-5   FILE_NAME         BCPL string, up to 30 chars
    SIZE-4              HASHCHAIN         next file in same hash chain
    SIZE-3              PARENT            parent directory block #
    SIZE-2              EXTENSION         0 or first file list/extension block
    SIZE-1              ST_FILE = -3

The data block list is stored *in reverse order* — `DATA_BLOCK_LIST[0]`
at offset SIZE-51 is the **last** data block slot; the first data block
number is at `DATA_BLOCK_LIST[HIGHEST_SEQ_in_this_header - 1]`, i.e. it
grows downward toward offset 6. This is the standard description in the
ADOS Manual and is why `DATA_SIZE` is tracked separately from
`HIGHEST_SEQ`.

The maximum number of data block slots in a single header is
`SIZE - 51 - 6 + 1` = 72 (for a 128-longword block). So a single file
header can describe up to 72 × 488 bytes = 35 136 bytes in OFS, or up
to 72 × 512 bytes = 36 864 bytes in FFS, before needing an extension
block.

### 7.6 File list / extension block

When a file needs more data block slots than a single header can hold,
additional "list" / "extension" blocks are chained via the `EXTENSION`
field of the header. An extension block has `T_LIST = 16` as its
primary type and `ST_FILE = -3` as its secondary type.

    0                   T_LIST = 16
    1                   OWN_KEY
    2                   BLOCK_COUNT       # data blocks in this extension
    3                   DATA_SIZE         (same as above)
    4                   FIRST_DATA        first data block in this extension
    5                   CHECKSUM
    6 .. SIZE-51        DATA_BLOCK_LIST   (same layout as in header)
    SIZE-50 .. SIZE-5   (unused info area)
    SIZE-4              0                 next-in-hash-list (always 0)
    SIZE-3              PARENT            *back* to the file header block
    SIZE-2              EXTENSION         next extension block or 0
    SIZE-1              ST_FILE = -3

Extension blocks form a linked list terminated by `EXTENSION == 0`.
The `PARENT` field points back to the owning file header block.

### 7.7 Data block

**OFS data block** (fig. 9-G):

    0                   T_DATA = 8
    1                   HEADER_KEY        file header block number
    2                   SEQNUM            sequence number 1..n
    3                   DATA_SIZE         bytes of user data in this block
    4                   NEXT_DATA         next data block, 0 = last
    5                   CHECKSUM
    6 .. SIZE-1         data              up to 488 bytes of file data

So OFS sacrifices 24 bytes per block to header + trailing checksum,
leaving 488 bytes per data block. The `SEQNUM` starts at 1, which is
critical: DiskDoctor can reconstruct a file's data block sequence from
the back-pointers and sequence numbers even if the header's
`DATA_BLOCK_LIST` is damaged.

**FFS data block** (fig. 9-H):

    0 .. 511             raw file data

FFS throws away the per-block metadata. A data block is just 512 bytes.
No sequence number, no back-pointer, no checksum. This is most of the
2x-3x speed improvement FFS offers over OFS: the per-block overhead
falls from 24 bytes to 0, and more importantly, data blocks can be DMA'd
straight into a contiguous user buffer without a copy through a
per-sector header area. The filesystem relies entirely on the file
header / extension block list to know what data belongs to which file.

The cost is that FFS is much less recoverable from physical damage.
If a disk sector is corrupted, OFS can often identify which file it
belonged to from the data block header; FFS cannot. DiskDoctor on an
FFS disk can lose entire files to a single bad block. The 2.0+
`DiskSalv` tool added heuristic recovery.

### 7.8 Bitmap blocks

The bitmap block records which disk blocks are allocated. It is a
simple longword array plus a checksum:

    0                   CHECKSUM         balance-to-0 checksum
    1 .. SIZE-1         BITMAP           one bit per data block; 1 = free,
                                           0 = allocated

The sense is inverted from what you might expect (set = free), which
catches people out. A fresh disk has a mostly-1 bitmap; as files are
allocated, bits clear.

Bits 0, 1 (bootblock), and the bitmap blocks themselves are perpetually
0. The root block's bit is also always 0.

For a standard 880 KB floppy, 1760 data blocks need 220 bytes = 55
longwords; plus the 1-longword checksum, a single bitmap block of 56
longwords is more than enough. In practice the bitmap area is 25
longwords of block pointers in the root block (holds up to 25 bitmap
blocks), so a single floppy uses one of the 25 slots.

For larger volumes, if more than 25 bitmap blocks are needed, a
**bitmap extension block** is chained from the root block's
`BITMAP_EXTEND` field. It contains more bitmap block pointers plus a
pointer to the next extension:

    0 .. SIZE-2           BITMAP_BLOCK_POINTERS (up to SIZE-1)
    SIZE-1                NEXT_EXTENSION

### 7.9 Checksum algorithms

**General block checksum** (root, userdir, file header, extension,
data block (OFS)): balance-to-zero. Compute the sum of all longwords
in the block with the checksum field *zeroed*; negate; store in the
checksum field. Verification: sum all longwords including the checksum
— the result must be 0. (Any additive arithmetic mod 2^32 is
equivalent.)

    uint32_t checksum_general(uint32_t *block, int longwords) {
        uint32_t sum = 0;
        for (int i = 0; i < longwords; i++) sum += block[i];
        return (uint32_t)(-sum);          /* store at checksum field  */
    }

The ADOS Manual phrases this as "ignoring overflow, the sum of all the
words in the block is zero". Longwords, not words — a common source of
bugs when porting between documentation that uses "word" loosely.

**Bitmap block checksum**: same algorithm, same balance-to-zero
semantics, but applied to the bitmap block (its only metadata is the
checksum itself in longword 0).

**Bootblock checksum**: a separate algorithm using one's-complement
add-with-carry over the two-block 1024-byte bootblock area. See §8.

### 7.10 FFS data block size conundrum

FFS on a standard 11-sector/track floppy has exactly the same blocks
and number of blocks as OFS. The FFS speed win isn't from more blocks;
it's from:

- No per-block header overhead (5% more user bytes in each data block,
  but only because the 488→512 grows slightly).
- No per-block DMA-then-copy pattern; the filesystem can read a whole
  track and hand the data directly to the user's buffer.
- Hash chains sorted by block number, which lets directory lookups
  stop early.

In practice FFS on a floppy is ~2-3x faster than OFS, mostly from the
DMA-direct improvement. On hard disks, where you can use larger
`de_MaxTransfer` values and multi-sector transfers, FFS is 5-10x
faster.

### 7.11 Directory cache FFS (DOS\4 / DOS\5)

Added in Kickstart 3.0 (V39, 1992) for Workbench 3.0/3.1. The directory
cache ("DirCache FFS") adds an extra block class — a
**directory cache block** — that stores, per directory, a flat linear
cache of entries with type, size, protection, date, and name. The cache
is maintained alongside the normal hash chain.

The improvement: `Examine`/`ExNext` on a large directory no longer has
to read one file header per entry. Instead the filesystem reads one
cache block and produces all its entries in one go. Typical speedups
are 5-10x for Workbench's directory scans, which read every entry to
pull icon images.

Cache blocks are chained into a separate linked list from the hash
chain and live alongside the original directory block. A DirCache
filesystem is still FFS-compatible at the file header / data block
level — only directory operations use the cache.

### 7.12 International mode (DOS\2 / DOS\3)

Added in ECS Kickstart 2.1 (V38, 1991). The hashing algorithm is
extended to case-fold ISO-Latin-1 instead of just ASCII:

    int toupper_intl(int c) {
        if ((c >= 'a' && c <= 'z') ||
            (c >= 0xE0 && c <= 0xFE && c != 0xF7))
            return c - 0x20;
        return c;
    }

The block layouts are **unchanged** — only the hash function differs.
A DOS\0 disk mounted by a DOS\2-aware filesystem will still read back
correctly if all its filenames are ASCII. But a disk containing
non-ASCII filenames *written* in DOS\0 mode cannot be correctly looked
up by a DOS\2-aware filesystem because the hash function has changed.
In practice this limited international mode's utility on the Amiga —
it shipped too late to become the default.

### 7.13 Field-by-field cheat sheet for emulators

- Offsets throughout §7 are in **longwords** (4-byte units), not bytes.
  Multiply by 4 to get byte offsets.
- `SIZE` means "block size in longwords" (128 for 512-byte blocks).
- BSTRs in on-disk structures are **in-place** length-prefixed strings
  within the block, NOT BPTRs to other memory. The length byte is the
  first byte of the name area; the bytes follow.
- The "Secondary type" at `SIZE-1` is the canonical way to identify a
  block once you know it's a T_SHORT — look at the last longword of
  the block.

---

## 8. Boot block

### 8.1 Layout

The boot block occupies the first 1024 bytes of a bootable AmigaDOS
disk — two 512-byte sectors, block 0 and block 1 (cylinder 0, head 0,
sectors 0 and 1). It is defined in `devices/bootblock.h`:

    struct BootBlock {
        UBYTE bb_id[4];          /* "DOS" + version byte */
        LONG  bb_chksum;         /* bootblock checksum   */
        LONG  bb_dosblock;       /* block # of root      */
        /* ... remainder of 2 blocks is free for the    */
        /*     bootloader's own 68000 code              */
    };

    #define BBNAME_DOS  (('D'<<24)|('O'<<16)|('S'<<8))
    #define BBNAME_KICK (('K'<<24)|('I'<<16)|('C'<<8)|('K'))

So `bb_id` is a 4-byte tag:

| `bb_id` | Meaning | Kickstart |
| --- | --- | --- |
| `"DOS\0"` | OFS (Old Filesystem) | 1.0+ |
| `"DOS\1"` | FFS (Fast Filesystem) | 1.3+ |
| `"DOS\2"` | OFS international | 2.1+ |
| `"DOS\3"` | FFS international | 2.1+ |
| `"DOS\4"` | OFS international + DirCache | 3.0+ |
| `"DOS\5"` | FFS international + DirCache | 3.0+ |
| `"KICK"` | KICK-disk (ramdisk reset survival) | 1.2+ |

The `bb_dosblock` field is a BCPL pointer to the root block — but in
the bootblock context, it's a raw longword-block-number, **not** a BPTR
shifted address. For a standard 880 KB floppy, this is 880. An
installed bootblock typically copies this value into the on-disk root
block and uses it for its own navigation.

### 8.2 Bootblock execution

On reset, Kickstart's disk validator process (via
`trackdisk.device/ETD_READ` on track 0) reads the first 1024 bytes into
chip RAM and verifies the checksum. If valid and `bb_id[0..2] == "DOS"`,
it builds a trampoline that calls the bootblock code at offset 12 with:

    A1 = pointer to IOStdReq for the trackdisk unit
    A6 = SysBase
    D0 = 0

The bootblock's job is to:

1. `OpenLibrary("dos.library", ...)` (or, in 1.x, find and use a
   pre-loaded dos.library).
2. `LoadSeg("L:FastFileSystem")` (or OFS) if `bb_id[3]` requests a
   non-ROM filesystem.
3. Return in D0 a pointer to the filesystem's entry point, or NULL.
4. Kickstart will then create the filesystem process and hand the
   rest of the boot over to it, which in turn loads and runs
   `S:Startup-Sequence` from the root filesystem.

Full details are in the companion `amiga-boot-process.md`. The key
fact for this document is that the bootblock is a fully fledged 68000
program, limited only by the 1012 free bytes after the header — which
makes viruses very easy to hide in them (and they were common).

### 8.3 Bootblock checksum

The bootblock checksum uses one's-complement add-with-end-around-carry
over the 256 longwords of the bootblock (both sectors, 1024 bytes / 4).
The algorithm, in pseudocode:

    uint32_t bootblock_checksum(uint32_t *blk, int longwords /* = 256 */) {
        uint32_t sum = 0;
        uint32_t old;
        for (int i = 0; i < longwords; i++) {
            if (i == 1) continue;       /* skip checksum slot */
            old = sum;
            sum += blk[i];
            if (sum < old) sum++;       /* add carry */
        }
        return ~sum;
    }

Stored at offset 4 (longword 1). This is a one's-complement checksum
rather than the balance-to-zero checksum used by the filesystem
proper. A mismatched bootblock checksum causes the disk to be treated
as non-bootable — the bootblock is ignored but the rest of the disk
can still be mounted and read.

### 8.4 Non-bootable disks and `INSTALL NOBOOT`

`INSTALL DFx:` writes a fresh bootblock to a formatted disk. `INSTALL
DFx: NOBOOT` zeroes the bootblock, leaving a disk that mounts and
reads/writes normally but will not boot. This is used for data disks
that should never be booted from — particularly ones that might carry
a bootblock virus.

`INSTALL DFx: FFS` writes a bootblock that forces FFS mode if the
running Kickstart is 1.3+; `INSTALL DFx: CHECK` verifies the current
bootblock.

### 8.5 KICK disks

When `bb_id == "KICK"`, the disk is marked as a **KICK disk**. KICK
disks are recognised by the Amiga's reset-persistent RAM system as
containing a filesystem for the recoverable RAM disk (`RAD:`). The
Kickstart OS preserves their mount state across warm resets if the
relevant expansion is present — but the detailed mechanism involves
`reset-matching` on specific memory markers, not just the bootblock
tag, so unemulated.

---

## 9. Filesystem / DOS tasks

### 9.1 A handler is a Process with a MsgPort

A filesystem handler is a regular DOS Process. Its `pr_MsgPort` is the
endpoint that dos.library sends packets to. Its main loop is a
packet-dispatch loop built on top of `WaitPkt`/`GetMsg` or
`WaitPort`/`GetMsg`. At the skeleton level:

    struct MsgPort *port = &proc->pr_MsgPort;
    Wait(1L << port->mp_SigBit);
    while ((msg = GetMsg(port)) != NULL) {
        DosPacket *pkt = (DosPacket *)msg->mn_Node.ln_Name;
        switch (pkt->dp_Type) {
            case ACTION_LOCATE_OBJECT:  ...  break;
            case ACTION_READ:           ...  break;
            case ACTION_WRITE:          ...  break;
            case ACTION_EXAMINE_OBJECT: ...  break;
            case ACTION_DIE:            running = 0; break;
            default: pkt->dp_Res1 = DOSFALSE;
                     pkt->dp_Res2 = ERROR_ACTION_NOT_KNOWN;
                     break;
        }
        ReplyPkt(pkt, pkt->dp_Res1, pkt->dp_Res2);
    }

This same shape is used by CON:, RAM:, NIL:, PIPE:, AUX:, SPEAK:, and
by the ROM FFS/OFS. The ROM filesystem also adds a secondary event
loop for replies from its underlying Exec device
(`trackdisk.device`), which it sends as Exec IORequests disguised as
DosPackets so that the same `WaitPkt` loop handles them.

### 9.2 Finding a handler: the DOS device list

The DOS device list is rooted at `DosInfo.di_DevInfo` in the
`dos.library` RootNode. It is a linked list of entries, each of which
is one of three things:

- **DLT_DEVICE** (0): a device (e.g. DF0:, DH0:, SER:, CON:).
- **DLT_DIRECTORY** (1): an assign (e.g. C:, S:, LIBS:).
- **DLT_VOLUME** (2): a mounted volume (e.g. "Workbench", "MyDisk").

Late-binding and non-binding assigns (2.0+) use DLT_LATE (3) and
DLT_NONBINDING (4).

Each list entry is 48 bytes (`sizeof(struct DosList)`). The structure
is a **union** — different types use different fields.

    struct DosList {
        BPTR                  dol_Next;      /* next in list             */
        LONG                  dol_Type;      /* DLT_DEVICE/DIR/VOLUME/... */
        struct MsgPort       *dol_Task;      /* handler's MsgPort         */
        BPTR                  dol_Lock;      /* for DLT_DIRECTORY         */
        union {
            struct {                         /* DLT_DEVICE                */
                BSTR      dol_Handler;       /* file to LoadSeg if needed */
                LONG      dol_StackSize;
                LONG      dol_Priority;
                ULONG     dol_Startup;       /* FileSysStartupMsg         */
                BPTR      dol_SegList;       /* or pre-loaded SegList     */
                BPTR      dol_GlobVec;       /* BCPL global vector, -1=C  */
            } dol_handler;
            struct {                         /* DLT_VOLUME                */
                struct DateStamp dol_VolumeDate;
                BPTR             dol_LockList;
                LONG             dol_DiskType;  /* 'DOS\0', 'DOS\1', ... */
            } dol_volume;
            /* ... */
        } dol_misc;
        BSTR                  dol_Name;      /* BCPL name, NUL-terminated */
    };

**`dol_Name` is a BSTR**, but — by convention — it is additionally
null-terminated (the byte after the BSTR length-denoted payload is
`\0`). So it can be printed as a C string by skipping one byte:
`(char *)(BADDR(dol_Name)) + 1`.

### 9.3 Walking the DOS device list safely

    struct DosList *dl;
    dl = LockDosList(LDF_DEVICES | LDF_READ);
    while ((dl = NextDosEntry(dl, LDF_DEVICES)) != NULL) {
        /* dl->dol_Name is a BSTR - skip length byte to get C name */
        printf("device: %s\n", (char *)BADDR(dl->dol_Name) + 1);
    }
    UnLockDosList(LDF_DEVICES | LDF_READ);

- **LDF_DEVICES / LDF_VOLUMES / LDF_ASSIGNS**: which kinds to walk.
- **LDF_READ / LDF_WRITE**: read or write access to the list. Write
  access is exclusive.
- Before 2.0, `LockDosList` is implemented via `Forbid()`, so you
  cannot Wait while walking the list. In 2.0+, it is implemented via
  a semaphore and you can.

### 9.4 How a volume is added when a disk is inserted

For a floppy:

1. The user inserts a disk in DFx:.
2. The trackdisk.device posts a disk-change software interrupt to
   anyone registered via `TD_ADDCHANGEINT`. The ROM filesystem is so
   registered.
3. The filesystem handler's disk-change routine reads the root block
   (block 880 on a standard floppy) to get the volume name, creation
   date, and disk type.
4. It constructs a new `DosList` entry of type `DLT_VOLUME`, populates
   `dol_Name` with the volume name, `dol_Task` with its own
   `pr_MsgPort`, `dol_VolumeDate` with the creation date, and
   `dol_DiskType` with `DOS\0` / `DOS\1` / etc.
5. It calls `AddDosEntry()` to link the volume entry into the DOS list.
6. It signals any waiting locks on that volume name (see
   `dol_LockList` — §3.2) so that processes that were waiting for
   "insert disk Foo" wake up.

On eject, the reverse happens: the filesystem `RemDosEntry()`'s the
volume node, or (if there are still outstanding locks) marks the node
as "no media" (`dol_Task = NULL`) and keeps it around until the last
lock is released.

---

## 10. Mountlists and device nodes

### 10.1 Mountlist syntax

The `DEVS:Mountlist` file is a plain text file describing devices that
should be known to the system but aren't in ROM. Each entry has the
form:

    DeviceName:
        Handler    = devs:handlers/somehandler
        Device     = scsi.device
        Unit       = 0
        Flags      = 0
        BlocksPerTrack = 16
        Surfaces   = 4
        Reserved   = 2
        Interleave = 0
        LowCyl     = 0
        HighCyl    = 1023
        Buffers    = 40
        BufMemType = 1
        MaxTransfer = 0x00200000
        Mask       = 0xFFFFFFFE
        BootPri    = 5
        DosType    = 0x444F5301     /* 'DOS\1' = FFS */
        Stacksize  = 4000
        Priority   = 5
        GlobVec    = -1
        Startup    = "filesystem_startup"
        Control    = ""
        FileSystem = l:FastFileSystem
        Environment = ...
    #

The `#` on a line by itself terminates the entry. The `MOUNT` command
(`c:Mount`) parses this file, builds a DeviceNode, inserts it into the
DOS device list via `AddDosEntry()` (or the expansion library's
`AddDosNode()` on 1.x), and — if the entry has `BootPri >= 0` — marks
it as mountable/bootable.

### 10.2 DosEnvec — the geometry descriptor

Each mountlist entry is translated into a `DosEnvec` (also called the
"environment vector") that describes the geometry to the filesystem.
From `libraries/filehandler.h`:

    struct DosEnvec {
        ULONG de_TableSize;         /* size of this struct in longwords */
        ULONG de_SizeBlock;         /* block size in longwords (128)    */
        ULONG de_SecOrg;            /* unused, 0                        */
        ULONG de_Surfaces;          /* heads                            */
        ULONG de_SectorPerBlock;    /* 1                                */
        ULONG de_BlocksPerTrack;    /* sectors per track                */
        ULONG de_Reserved;          /* reserved blocks at start (boot)  */
        ULONG de_PreAlloc;          /* reserved blocks at end           */
        ULONG de_Interleave;        /* usually 0                        */
        ULONG de_LowCyl;            /* starting cylinder                */
        ULONG de_HighCyl;           /* ending cylinder                  */
        ULONG de_NumBuffers;        /* initial cache buffers            */
        ULONG de_BufMemType;        /* MEMF_CHIP/MEMF_FAST/MEMF_PUBLIC  */
        ULONG de_MaxTransfer;       /* max bytes per trackdisk call     */
        ULONG de_Mask;              /* DMA address mask for buffers     */
        LONG  de_BootPri;           /* autoboot priority                */
        ULONG de_DosType;           /* 'DOS\0'..'DOS\5' (filesystem)    */
        /* 2.0 extensions: */
        ULONG de_Baud;              /* serial baud rate (for serial FS) */
        ULONG de_Control;
        ULONG de_BootBlocks;        /* bootblock size in blocks         */
    };

The indexes corresponding to these fields are `DE_TABLESIZE`,
`DE_SIZEBLOCK`, `DE_NUMHEADS`, `DE_SECSPERBLK`, `DE_BLKSPERTRACK`,
`DE_RESERVEDBLKS`, `DE_PREFAC`, `DE_INTERLEAVE`, `DE_LOWCYL`,
`DE_UPPERCYL`, `DE_NUMBUFFERS`, `DE_BUFMEMTYPE`, `DE_MAXTRANSFER`,
`DE_MASK`, `DE_BOOTPRI`, `DE_DOSTYPE`.

The `de_Mask` field is critical on some hardware: it is a DMA-capable
address mask that the filesystem must use to decide whether a user
buffer can be given directly to the underlying device or whether a
bounce buffer is needed. For example, an A2091 SCSI controller with
24-bit DMA needs `de_Mask = 0x00FFFFFE` on an A3000 with 32-bit memory.

`de_MaxTransfer` is the maximum size of a single device-level
transfer. For trackdisk on a floppy it's effectively one cylinder.
For SCSI drives it's often `0x00200000` (2 MB); for IDE it's
`0x0000FE00` (just under 64 KB, because the IDE `SECCNT` is 8 bits).

### 10.3 FileSysStartupMsg

`dol_Startup` / `dn_Startup` for a disk device points at:

    struct FileSysStartupMsg {
        ULONG fssm_Unit;        /* Exec unit # for the underlying device */
        BSTR  fssm_Device;      /* BSTR, the Exec device name (also a
                                   trailing NUL by convention)           */
        BPTR  fssm_Environ;     /* BPTR to the DosEnvec                  */
        ULONG fssm_Flags;       /* flags for OpenDevice()                */
    };

When the filesystem process starts, it receives this in the "startup"
portion of the Exec Message that created it. It then does:

    OpenDevice((char*)BADDR(fssm_Device)+1, fssm_Unit,
               &my_ioreq, fssm_Flags);
    env = (struct DosEnvec *)BADDR(fssm_Environ);
    block_size  = env->de_SizeBlock << 2;
    num_blocks  = (env->de_HighCyl - env->de_LowCyl + 1)
                  * env->de_Surfaces * env->de_BlocksPerTrack;
    root_block  = (num_blocks - 1 + env->de_Reserved) >> 1;
    ...

This is literally the calculation the `rootblock.c` example in the
AmigaDOS Manual performs (ch. 9).

### 10.4 DeviceNode

A `DeviceNode` (from `libraries/filehandler.h`) is what lives in
`rn_Info.di_DevInfo` for DLT_DEVICE entries. It is identical in layout
to a DosList DLT_DEVICE union branch:

    struct DeviceNode {
        BPTR             dn_Next;      /* singly linked list */
        ULONG            dn_Type;      /* always 0 */
        struct MsgPort  *dn_Task;      /* handler, NULL until started */
        BPTR             dn_Lock;      /* NULL for devices */
        BSTR             dn_Handler;   /* BSTR: filename to LoadSeg */
        ULONG            dn_StackSize;
        LONG             dn_Priority;
        BPTR             dn_Startup;   /* FileSysStartupMsg */
        BPTR             dn_SegList;   /* pre-loaded code, or 0 */
        BPTR             dn_GlobalVec; /* -1 = C, 0 = make new */
        BSTR             dn_Name;      /* e.g. {'\3','D','F','0'} */
    };

When a packet first arrives for a DLT_DEVICE entry and `dn_Task ==
NULL`, dos.library:

1. If `dn_SegList == 0`, LoadSeg's `dn_Handler` to get a SegList.
2. Creates a Process with `pr_SegList = dn_SegList`, stack size
   `dn_StackSize`, priority `dn_Priority`, passing `dn_Startup` as
   the initial message.
3. Stores the new process's `pr_MsgPort` in `dn_Task` so subsequent
   packets find the running handler directly.

Handlers that want to be singletons (like RAM:) patch `dn_Task` on
startup. Handlers that want a new instance per open (like CON:) leave
`dn_Task` NULL so each `Open()` spawns a fresh process.

### 10.5 MakeDosNode, AddDosNode, AddDosEntry

- `MakeDosNode()` (expansion.library) builds a DeviceNode from a
  parameter packet — used by AutoConfig expansion boards.
- `AddDosNode()` (expansion.library, 1.x) links a DeviceNode into the
  DOS device list.
- `AddDosEntry()` (dos.library, 2.0+) adds any DosList entry.

BindDrivers (run in startup-sequence) walks the Expansion directory,
calls the per-board driver, which typically ends in an AddDosNode.

---

## 11. `trackdisk.device`

### 11.1 Overview

`trackdisk.device` is the Exec device that owns the floppy drives. It
speaks the standard Exec IORequest protocol (`OpenDevice`,
`CloseDevice`, `BeginIO`, `AbortIO`, `DoIO`, `SendIO`) and adds a set
of device-specific commands for disk-level operations. The ROM
filesystem calls it for all its I/O; applications that want
sub-filesystem access (DiskCopy, disk editors, copy-protection
loaders) open it directly.

Unit numbers are 0–3, representing the four possible 3.5" drives
(internal + up to three externals). **Unit 0 is the internal drive.**
Contrary to what older 1.x docs say, there is no separate "10–13" for
external drives — units 1–3 are the external daisy chain (ADOS
Manual / RKM L&D are both explicit here; the "10" confusion is a
misreading of an early 1.1 doc that mentioned a `TD_NAME` prefix).

Basic geometry:

    NUMHEADS     = 2
    NUMCYLS      = 80
    NUMSECS      = 11          /* sectors per track/cylinder side */
    NUMUNITS     = 4
    TD_SECTOR    = 512         /* bytes per sector */
    TD_LABELSIZE = 16          /* bytes of sector label per sector */

High-density floppies (A3000/A4000) double the sectors per track
to 22, giving 1760 KB disks. Use `TD_GETNUMTRACKS` and
`TD_GETDRIVETYPE` at runtime — the ROM trackdisk never hardcodes
NUMCYLS after V37.

A whole track is `NUMSECS * TD_SECTOR = 5632 bytes`. The trackdisk
driver always reads and writes at the track level — a sector read
turns into a full-track read if the track isn't already buffered,
and a sector write is cached and written back when the track buffer
is evicted.

### 11.2 Track buffer and DMA constraints

- The driver holds **one track buffer per open unit** (not a complete
  cache — only the most recent track).
- On a write, the buffer is marked dirty; the track is flushed back
  when the user tries to access a different track, or when
  `CMD_UPDATE`/`ETD_UPDATE` is called, or when the motor is turned off.
- All user buffers passed to trackdisk must be:
  - In chip memory (because the blitter is used for MFM encode/decode)
  - Word-aligned
  - A multiple of `TD_SECTOR` bytes long
  - At a `TD_SECTOR`-aligned `io_Offset`

The blitter is used because it is the only chip capable of the
bit-rearranging operation needed to turn MFM-encoded track data into
decoded sector data (or vice versa). MFM encoding interleaves the bit
fields of odd and even bit positions across two words; undoing that
at CPU speed would be too slow for a real-time disk read.

### 11.3 Commands

#### Standard Exec commands

    CMD_READ    = 2      read a logical byte range
    CMD_WRITE   = 3      write a logical byte range
    CMD_UPDATE  = 4      flush the track buffer if dirty
    CMD_CLEAR   = 5      mark track buffer invalid (without flushing)

These are interchangeable with the extended forms (`ETD_READ` etc.)
except that the extended forms honour the disk-change count in
`iotd_Count`.

#### Trackdisk-specific commands

    TD_MOTOR      = CMD_NONSTD+0    motor on (io_Length=1) / off (=0)
    TD_SEEK       = CMD_NONSTD+1    move heads, no actual read
    TD_FORMAT     = CMD_NONSTD+2    write a whole track, no verify
    TD_REMOVE     = CMD_NONSTD+3    register disk-change software interrupt
    TD_CHANGENUM  = CMD_NONSTD+4    read disk-change counter
    TD_CHANGESTATE= CMD_NONSTD+5    zero if disk present, non-zero if empty
    TD_PROTSTATUS = CMD_NONSTD+6    non-zero if write-protected
    TD_RAWREAD    = CMD_NONSTD+7    read raw MFM bits
    TD_RAWWRITE   = CMD_NONSTD+8    write raw MFM bits
    TD_GETDRIVETYPE  = CMD_NONSTD+9
    TD_GETNUMTRACKS  = CMD_NONSTD+10
    TD_ADDCHANGEINT  = CMD_NONSTD+11  (replacement for TD_REMOVE)
    TD_REMCHANGEINT  = CMD_NONSTD+12

    TD_EJECT      (added in V39 for some drive types; not on standard Chinon)
    TD_LASTINT    (internal)

#### Extended (ETD_) commands

The extended commands take an `IOExtTD` instead of a plain `IOStdReq`,
adding two longwords:

    struct IOExtTD {
        struct IOStdReq iotd_Req;
        ULONG           iotd_Count;     /* disk-change counter floor */
        ULONG           iotd_SecLabel;  /* ptr to sector-label data or 0 */
    };

    ETD_READ     = CMD_READ | TDF_EXTCOM     (TDF_EXTCOM = 1<<15)
    ETD_WRITE    = CMD_WRITE | TDF_EXTCOM
    ETD_MOTOR    = TD_MOTOR | TDF_EXTCOM
    ETD_SEEK     = TD_SEEK  | TDF_EXTCOM
    ETD_FORMAT   = TD_FORMAT | TDF_EXTCOM
    ETD_UPDATE   = CMD_UPDATE | TDF_EXTCOM
    ETD_CLEAR    = CMD_CLEAR | TDF_EXTCOM
    ETD_RAWREAD  = TD_RAWREAD | TDF_EXTCOM
    ETD_RAWWRITE = TD_RAWWRITE | TDF_EXTCOM

Semantics of the extended forms:

- **`iotd_Count`**: any request whose `iotd_Count` is **less than** the
  current disk-change counter (as returned by `TD_CHANGENUM`) is
  rejected with `TDERR_DiskChanged`. This lets a filesystem queue a
  sequence of operations and have them all aborted the moment the
  user ejects the disk — you sample `TD_CHANGENUM`, stamp every
  subsequent request with that value, and the trackdisk driver
  automatically errors out anything older than the current disk.
- **`iotd_SecLabel`**: if non-NULL, points to an array of
  `TD_LABELSIZE * num_sectors` bytes (16 bytes per sector of label
  data). On a read, trackdisk fills this with the label bytes
  decoded from each sector's MFM header; on a write, it incorporates
  these bytes into the written headers. The label area is filesystem-
  private — FFS stores per-sector validation metadata there. A plain
  `CMD_WRITE` leaves the label bytes unchanged on disk.

### 11.4 IOExtTD field use per command

| Command | `io_Length` | `io_Offset` | `io_Data` | Other |
| --- | --- | --- | --- | --- |
| CMD/ETD_READ | bytes to read (multiple of 512) | byte offset (multiple of 512) | destination buffer (chip mem) | iotd_Count, iotd_SecLabel |
| CMD/ETD_WRITE | bytes to write | byte offset | source buffer | same |
| TD_SEEK | ignored | byte offset to seek to | — | — |
| TD_MOTOR | 0=off, 1=on | — | — | io_Actual: previous state |
| TD_FORMAT | track length in bytes | track-aligned | buffer for initial data | — |
| TD_RAWREAD | buffer length (≤32K) | track number | buffer (chip) | io_Flags IOTDB_INDEXSYNC |
| TD_RAWWRITE | same | same | same | same |
| TD_CHANGENUM | — | — | — | io_Actual: disk-change count |
| TD_CHANGESTATE | — | — | — | io_Actual: 0=disk present |
| TD_PROTSTATUS | — | — | — | io_Actual: non-zero if protected |
| TD_GETDRIVETYPE | — | — | — | io_Actual: `DRIVE3_5` etc |
| TD_GETNUMTRACKS | — | — | — | io_Actual: max track count |
| TD_ADDCHANGEINT | — | — | → Interrupt struct | — |
| TD_REMCHANGEINT | — | — | (used internally) | — |

### 11.5 Error codes

From `devices/trackdisk.h`:

    TDERR_NotSpecified    20   catchall
    TDERR_NoSecHdr        21   could not find any sector header
    TDERR_BadSecPreamble  22   sector preamble error
    TDERR_BadSecID        23   sector ID field error
    TDERR_BadHdrSum       24   header checksum mismatch
    TDERR_BadSecSum       25   data checksum mismatch
    TDERR_TooFewSecs      26   not all 11 sectors found on track
    TDERR_BadSecHdr       27   sector header damaged
    TDERR_WriteProt       28   disk is write-protected
    TDERR_DiskChanged     29   disk removed or not present
    TDERR_SeekError       30   seek verification failed
    TDERR_NoMem           31   out of memory
    TDERR_BadUnitNum      32   unit > NUMUNITS
    TDERR_BadDriveType    33   not a 3.5" Amiga drive
    TDERR_DriveInUse      34   another task has exclusive use
    TDERR_PostReset       35   user pressed reset, awaiting doom

### 11.6 Disk change notification

**Old API: `TD_REMOVE`.** Install a single software interrupt that
will be called on disk change. The `io_Data` field points at an
`Interrupt` struct. Only one consumer per unit. Superseded in V36.

**New API: `TD_ADDCHANGEINT` / `TD_REMCHANGEINT`.** Each consumer
sends an IORequest to `TD_ADDCHANGEINT`; the request is *stashed*
(not replied) and every time a disk-change event occurs, the stashed
interrupt is fired. `TD_REMCHANGEINT` replies the IORequest and
removes it from the list. The autodoc specifically warns:

> The call does not "complete" (e.g. TermIO). The request is stashed
> until `TD_REMCHANGEINT` is called, when it is finally replied.

Filesystems register for change notifications this way, as does
Workbench (to refresh the floppy-icon state), as does the AmigaDOS
automounter.

### 11.7 Raw read / raw write

`TD_RAWREAD` and `TD_RAWWRITE` bypass all MFM processing entirely —
the data in the user buffer is the raw bit stream as it appears on
disk. This is how copy-protection loaders work: they can read a
specially-formatted track and look for data the standard MFM decoder
won't accept (sync word variations, missing sectors, deliberately
corrupt checksums, etc.).

Maximum buffer size is 32K. The track number is passed in
`io_Offset` (it is *not* a byte offset here — that's
documented-wart territory, because the driver has no idea what the
disk format is).

If `io_Flags & IOTDB_INDEXSYNC` is set, the driver tries to align
the read or write to start immediately after the disk index pulse.
There is a fixed delay between the index pulse and the start of the
actual DMA (135–200 microseconds), because the software interrupt
that fires the DMA takes ~55 μs, plus one horizontal scanline of
DMA-slot alignment (up to 63 μs of jitter at 66 μs/line), plus
~15 μs that the original ROM team never accounted for. So **no data
will arrive within the first ~4-7 bytes after the index mark** —
which matters for anyone trying to write a perfect 1:1 protected
disk image.

> "In short, you will almost never get bits within the first 135
> microseconds of the index pulse, and may not get it until 200
> microseconds. At 4 microseconds/bit, this works out to be between
> 4 and 7 bytes of user data of delay." — *RKM L&D, trackdisk.device,
> TD_RAWREAD autodoc*

### 11.8 Auto-motor-off

The trackdisk driver automatically turns the motor **on** when it
receives an I/O request and the motor is off. It does not automatically
turn the motor off — that is the user's (or filesystem's)
responsibility. The standard approach is:

- Filesystems turn the motor off after an idle timeout (nominally
  2.5 seconds). The 2.0 FFS uses an `ACTION_TIMER` every ~1/2 second
  to check activity and turn the motor off if it has been idle.
- Applications that do raw disk work should `TD_MOTOR` off when done.
- The light on the drive is wired to the motor-on state, so users
  know "light off = safe to eject".

### 11.9 Interaction with disk.resource

Trackdisk does not directly own the disk hardware registers. It shares
them with anyone else who wants raw access via the `disk.resource`
arbitration (§12). When trackdisk wants to do an operation, it:

1. `GetUnit(unitPtr)` on disk.resource for this unit — blocks if
   another consumer has it.
2. Configures CIA bits for drive select, side, direction, etc.
3. Programs Paula's DSKPT, DSKLEN, ADKCON, DSKSYNC registers.
4. Waits for DSKSYNC and DSKBLK interrupts.
5. Restores the CIA bits to the inactive state.
6. `GiveUnit()` back to disk.resource.

Any other consumer — a copy-protected game's custom loader, for
example — that wants to talk to the disk hardware directly must use
the same protocol, or they will be fighting trackdisk for the hardware
and corrupting each other's transfers.

---

## 12. `disk.resource`

A **resource** in the Exec sense is a shared hardware element with
arbitrated access, one step up from raw register manipulation and
one step below a full device driver. `disk.resource` owns the
floppy disk hardware: the disk DMA, DSKLEN/DSKBYTR/DSKSYNC, the
disk-DMA interrupts, and the CIA drive-select lines.

There is one disk.resource instance for the whole system, managing
all four drive units together (because they share a single DMA
channel and a single set of Paula registers).

### 12.1 API

    OpenResource("disk.resource")       -> DRResource *

    BOOL     AllocUnit(ULONG unitNum)    /* reserve unit for use       */
    void     FreeUnit (ULONG unitNum)    /* release reservation        */
    struct Unit *GetUnit(struct Unit *)  /* grab disk HW for this unit */
    void     GiveUnit  (void)            /* release disk HW            */
    ULONG    GetUnitID (ULONG unitNum)   /* drive type as 32-bit ID    */

### 12.2 `AllocUnit` / `FreeUnit`

Registers the caller as an owner of a unit slot. Must be called before
`GetUnit`. `AllocUnit` returns non-zero on success, zero on failure
(unit already owned). `FreeUnit` releases the registration. These
functions are about long-term ownership — "I want exclusive control of
DF1: until I give it back". The ROM trackdisk.device calls
`AllocUnit` at OpenDevice time and `FreeUnit` at CloseDevice time.

### 12.3 `GetUnit` / `GiveUnit`

These are the short-term "I am about to touch the hardware" calls.
`GetUnit` is called with a pointer to a `Unit` structure whose
embedded Message will be replied when the caller becomes the hardware
owner. If the disk is currently free, `GetUnit` returns immediately
with the pointer of the **previous** unit — so you can tell whether
anyone else has touched the registers since you last had them. If
the disk is busy, the Message is queued and will be `ReplyMsg`ed when
the current owner calls `GiveUnit`.

The contract on release, quoting the autodoc:

> Please leave the disk in the following state:
>
>   dmacon dma bit ON
>   dsklen dma bit OFF (write a #DSKDMAOFF to dsklen)
>   adkcon disk bits -- any way you want
>   intena: disk sync and disk block interrupts -- Both DISABLED
>   CIA resource index interrupt -- DISABLED
>   8520 outputs -- doesn't matter, resource will inactivate them
>   8520 data direction regs -- restore to original state.

I.e.: DMA channel enabled in DMACON; DSKLEN cleared to $4000 to
prevent accidental DMA; interrupts masked; CIA DDR restored.

### 12.4 `GetUnitID`

Returns a 32-bit identifier for the installed drive. Defined values:

    $00000000    no drive present
    $FFFFFFFF    Amiga standard 3.5" double density (the default)
    $55555555    48 TPI double density double-sided 5.25"
    /* others assigned by Commodore on request */

The ID is read by a special protocol on the motor-on/motor-off line
at boot time (see the HRM appendix E for details): the drive shifts
out 32 bits serially on the RDY pin, MSB first. This is how the
trackdisk driver knows whether the attached drive is really a
standard Amiga 3.5" or something else, and whether to permit
non-standard formats via `TDB_ALLOW_NON_3_5`.

### 12.5 Arbitrating with trackdisk

Because trackdisk uses disk.resource for every hardware access, any
direct user of disk.resource interleaves cleanly with trackdisk: if
trackdisk is in the middle of a track read, your `GetUnit` will
block until it finishes. If you were in the middle of a custom
loader, trackdisk's next I/O request will block until you
`GiveUnit`.

This is why a game can "briefly take over" the disk to run its
copy-protection check, then hand it back to trackdisk: both sides
follow the GetUnit/GiveUnit protocol and neither clobbers the
other's DSKLEN or DSKPT settings. Some early 1.x games got this
wrong and could corrupt filesystem writes if they raced trackdisk.

---

## 13. Floppy hardware (low level)

This section is a minimum-viable summary for an emulator author. For
the full register-level specification, see `amiga-hardware-reference.md`.

### 13.1 Registers (Paula)

- **DSKPTH/DSKPTL ($DFF020/$DFF022)**: 32-bit DMA source/destination
  pointer. Buffer must be word-aligned and in chip RAM.
- **DSKLEN ($DFF024)**: bit 15 DMAEN, bit 14 WRITE, bits 13–0
  LENGTH in words. Writing DMAEN=1 once does **not** start DMA —
  you must write DSKLEN twice with DMAEN set. This is an
  anti-accidental-write safety.
- **DSKDAT ($DFF026)**: byte of DMA data write (hardware-used only).
- **DSKDATR ($DFF008)**: dummy read.
- **DSKBYTR ($DFF01A)**: read-only disk byte/status.
  - bit 15 DSKBYT (valid byte available; cleared on read).
  - bit 14 DMAON (DMA actually enabled).
  - bit 13 DISKWRITE (mirror of DSKLEN bit 14).
  - bit 12 WORDEQUAL (DSKSYNC match, pulsed).
  - bits 7–0 the current byte from the disk.
- **DSKSYNC ($DFF07E)**: the 16-bit pattern to sync on. Set to
  `$4489` (the MFM sync mark) for all standard Amiga reads.
- **ADKCON ($DFF09E)** / **ADKCONR ($DFF010)**: control register.
  Bit 15 SET/CLR (determines if ones in other bits set or clear
  them). Bits 14–13 PRECOMP1/0 (write precompensation, 0–560 ns).
  Bit 12 MFMPREC (1 = MFM, 0 = GCR). Bit 10 WORDSYNC (enable
  sync-word alignment for reads). Bit 9 MSBSYNC (alternative GCR
  sync). Bit 8 FAST (1 = 2 μs/bit for MFM, 0 = 4 μs/bit for GCR).
  Bits 7–0 are audio.

### 13.2 Registers (CIA-B)

From `CIABPRB ($BFD100)`, active low:

- bit 7 `/MTR` — disk motor
- bit 6 `/SEL3` — select drive 3
- bit 5 `/SEL2` — select drive 2
- bit 4 `/SEL1` — select drive 1
- bit 3 `/SEL0` — select drive 0 (internal)
- bit 2 `/SIDE` — 0 = upper head, 1 = lower head
- bit 1 `DIR`  — 0 = step toward centre, 1 = step outward
- bit 0 `/STEP` — step pulse (active low, must be brief)

And from `CIAAPRA ($BFE001)`, active low, these are inputs:

- bit 5 `/RDY` — drive ready (speed up)
- bit 4 `/TK0` — head at track 0
- bit 3 `/WPRO` — write protected
- bit 2 `/CHNG` — disk changed (latched low until a step after
  insertion)

The disk index pulse goes into the CIA-B `FLAG` pin (which can
generate a level-6 interrupt if enabled via the CIA interrupt control
register).

**Nonstandard motor behaviour**: the disk motor signal is latched
into the drive at the moment its select line goes active. So the
standard sequence to turn a motor on or off is:

1. Set the motor data bit (`/MTR`) to the desired state.
2. Pulse the drive's select line (`/SELn`) active.
3. Now the drive remembers "motor on" (or off) until next select.

### 13.3 MFM sector layout

Each track on a standard Amiga DD floppy contains 11 sectors. All 11
sectors are packed together into a single track-sized bit stream —
there is **no** per-sector gap in the IBM sense. The format is
"one track of raw MFM data" and the sectors are discovered by
looking for sync marks. This is why a whole track is 5632 bytes of
user data but the raw MFM bit stream is ~12 800 bytes, with
variable-length alignment inside the 16-bit cell.

Per-sector structure (as defined by the Amiga trackdisk driver; a
different operating system could use different sector structures
and the hardware would happily oblige — that is why the controller
is called "flexible"):

    2 bytes      000000 00      (pre-sync gap, zero MFM)
    4 bytes      $AAAAAAAA      (zero MFM, just clock bits)
    4 bytes      $44894489      (two copies of $4489 sync word)
    4 bytes      header info    (track/sector/sector-offset/format byte,
                                  odd+even MFM-encoded)
    16 bytes     sector label   (TD_LABELSIZE = 16 bytes of caller label,
                                  odd+even MFM-encoded)
    4 bytes      header checksum (odd+even)
    4 bytes      data checksum   (odd+even)
    512 bytes    data            (odd+even; this block is 1024 bytes of MFM)
    —

Total per sector (MFM-encoded): ~544 × 2 = ~1088 bytes + overhead,
giving ~11 KB per track of MFM. The remaining "slack" is absorbed by
a padding gap at the end of the track.

**Odd+even MFM encoding**. Amiga MFM is word-oriented and stores all
the odd bits of a longword first, followed by all the even bits.
This makes it easy to decode with the blitter: a single blitter pass
with a specific A/B/C minterm turns a raw MFM longword into a decoded
longword. Without this interleaving, you would need a per-bit shift
operation that the blitter isn't capable of.

The sync words are $4489 $4489 — the choice of $4489 is because its
MFM clock-bit pattern is unique: it has a clock-bit violation
(two adjacent 1-clocks) that never occurs in any normal data. The
Paula hardware has a dedicated comparator on DSKSYNC that matches
$4489 in real time as bits shift through.

### 13.4 Write precompensation

Floppy magnetic recording suffers from bit shift: adjacent high-to-low
transitions can "attract" each other because of the magnetic field
geometry. To compensate, the controller can write adjacent bits
slightly **earlier** or **later** than their nominal position. On the
Amiga, this is set via `ADKCON` bits 14–12 (MFMPREC + PRECOMP1/0).
Values are:

    00 = none
    01 = 140 ns
    10 = 280 ns
    11 = 560 ns

Trackdisk uses precompensation 140 ns (value 01) for all standard
floppies. For the innermost tracks (where bit shift is worst), some
high-capacity drives want 280 ns, set via the `tdu_Comp01Track` /
`tdu_Comp10Track` / `tdu_Comp11Track` fields on the public unit
structure. Emulators that ignore precomp will read back modern images
perfectly; real drives writing a physical disk in 1986 needed it.

### 13.5 DMA and interrupt sequence for reading a sector

1. Seek to the target cylinder (`TD_SEEK`), wait 15 ms settle.
2. Select head (`CIABPRB /SIDE`).
3. Turn on motor if not already on, wait 500 ms or `/RDY` low.
4. Set DMACON disk DMA enable. Set ADKCON: WORDSYNC=1, FAST=1,
   MFMPREC=1, PRECOMP=1.
5. Write DSKSYNC=$4489.
6. Write DSKPT to the track buffer.
7. Write DSKLEN=$4000 (dummy, to arm).
8. Write DSKLEN=$8000|length (real, kicks off DMA).
9. Wait for DSKBLK interrupt (disk block done). DMA engine
   automatically stopped.
10. Write DSKLEN=$4000 (disarm).
11. Blitter-decode the track buffer from odd+even MFM into
    sector buffers, checking per-sector checksums.

A write is the same sequence with WRITE=1 in DSKLEN, and the
blitter encodes the sectors into the track buffer before starting
DMA.

### 13.6 Index pulse

The index pulse is a hardware signal from the drive, one pulse per
disk revolution, emitted by a photo-interrupter on the disk's index
hole. It arrives at the CIA-B FLAG pin. The standard Amiga doesn't
use it for normal I/O — sync word detection replaces it — but
`TD_RAWREAD` with `IOTDB_INDEXSYNC` uses it to line up the start of
a raw read with a precisely known point on the disk, which is
essential for disk duplication and for re-creating certain copy
protections.

---

## 14. Rigid Disk Block (RDB) and hard disk

### 14.1 RDB concept

The **Rigid Disk Block** is a standardised self-describing data
structure written to block 0 (or somewhere within the first
`RDB_LOCATION_LIMIT = 16` blocks) of a hard disk. It replaces the
mountlist: instead of the user having to edit `DEVS:Mountlist` and
run `MOUNT`, the RDB tells the system everything it needs to know
about the drive, including its partitioning, its filesystem, and its
boot priority.

Each self-describing block on an RDB disk has a 4-byte ID tag and a
standardised header so the system can discover them by scanning:

    "RDSK"   Rigid Disk Block         — the head of the structure
    "PART"   Partition Block          — one per partition
    "FSHD"   FileSystem Header Block  — filesystem descriptor
    "LSEG"   LoadSeg Block            — a chunk of filesystem code
    "BADB"   BadBlockBlock            — bad block list

### 14.2 Partition block

Each partition block describes one logical partition on the drive:

    pb_ID              "PART"
    pb_SummedLongs     length in longwords
    pb_ChkSum          checksum (balance to zero)
    pb_HostID          host SCSI ID
    pb_Next            block # of next partition block, -1 = last
    pb_Flags           PBF_BOOTABLE (1), PBF_NOMOUNT (2)
    pb_DevFlags        flags for OpenDevice
    pb_DriveName       BSTR: partition name ("DH0", "WORK", ...)
    pb_Environment     DosEnvec geometry (as in §10)
    pb_EReserved[15]   reserved

So each partition is described by its own copy of a DosEnvec — the
same struct that `DEVS:Mountlist` would have produced. The partition's
`pb_Environment.de_DosType` tells the system which filesystem to use.

### 14.3 FileSystem header block

    fhb_ID              "FSHD"
    fhb_SummedLongs
    fhb_ChkSum
    fhb_HostID
    fhb_Next            block # of next FSHD, -1 = last
    fhb_Flags
    fhb_Reserved[2]
    fhb_DosType         "DOS\0" / "DOS\1" / custom
    fhb_Version         minimum supported version
    fhb_PatchFlags      which fields below override defaults
    fhb_Type / Task / Lock / Handler / StackSize / Priority /
        Startup / SegListBlocks / GlobalVec
    fhb_FileSysName     BSTR: human-readable name

"FSHD" is effectively a DeviceNode-in-waiting: it says "if you find
a partition with this DosType, create a handler process using this
configuration".

### 14.4 LoadSeg blocks

If the filesystem isn't in ROM (and on anything after the earliest
A500s, it isn't — FFS lives in ROM from 2.0, but CrossDOS, SFS, and
countless third-party filesystems do not), the RDB also contains
**LoadSeg blocks** — "LSEG" tagged blocks that together hold the
LoadSeg-format hunks of the filesystem binary. At boot, the Kickstart
autoboot code reads these, concatenates them, and `InternalLoadSeg`'s
the result to produce a SegList, which it then uses as
`dn_SegList` of the DeviceNode.

This is how a hard disk can autoboot a filesystem that Kickstart
doesn't know about: the filesystem's code is stored in the RDB,
loaded before anything else on the disk is accessed, and installed
as a DeviceNode. By the time dos.library starts to look for boot
drives, that DeviceNode is there and usable.

### 14.5 Autoboot and BootPri

Expansion library's `AddBootNode()` inserts a boot-eligible
DeviceNode into the expansion list, sorted by `de_BootPri`. At boot
time, Kickstart walks this list and tries to boot from each entry in
priority order. If the device exists, the filesystem is valid, and
the root directory contains an executable `S:Startup-Sequence` or
equivalent, that device is chosen as SYS:.

See `amiga-boot-process.md` for the full autoboot walk — this
document covers the on-disk format of the RDB but not the in-ROM
autoboot state machine.

### 14.6 Bad block block

    bbb_ID              "BADB"
    bbb_SummedLongs
    bbb_ChkSum
    bbb_HostID
    bbb_Next            next bad block block, -1 = last
    bbb_Reserved
    bbb_BlockPairs      array of { bad_block, good_block }

The filesystem (or, in practice, a low-level driver above SCSI)
consults the bad-block map and substitutes `good_block` for
`bad_block` when a request would otherwise land on a known-bad
sector. This is vestigial on modern media — almost all SCSI and IDE
drives since 1990 do their own sector sparing invisibly — but
pre-1990 hard drives did not, and the RDB mechanism gave the
filesystem a way to work around factory defects.

---

## 15. Filesystems shipped

### 15.1 OFS — `DOS\0`

The original 1985 filesystem. Ships in ROM in 1.0–1.3 as "ROM FS"
and remains the default for floppy disks until Kickstart 1.3. Uses
the "OFS" block layouts from §7: 488 bytes of data per 512-byte
block, data blocks carry per-block headers with back-pointers, hash
chains are not sorted. Recovery-friendly but slow.

### 15.2 FFS — `DOS\1`

Introduced with Kickstart 1.3 (V34, 1988) as the "Fast File System".
Available on hard disks in 1.3 but not on floppies until the 1.3.2
INSTALL supported writing FFS bootblocks. FFS is the default from
Kickstart 2.0 onward.

Block layouts §7: 512 bytes of raw data per data block (no header),
hash chains sorted in ascending block order. From Kickstart 2.0,
FFS moves into ROM so hard disks can boot from it without needing
LoadSeg blocks in the RDB.

### 15.3 OFS international — `DOS\2`

Introduced with Kickstart 2.1 (V38, 1991). Same block layouts as
OFS, but the hash function uses an ISO-Latin-1 case-fold instead
of ASCII. See §7.12.

### 15.4 FFS international — `DOS\3`

Same as FFS but with the international hash function. 2.1+.

### 15.5 FFS with directory cache — `DOS\5`

Introduced with Kickstart 3.0 (V39, 1992). FFS with the directory
cache blocks (§7.11) that speed up large directory scans. Requires
Kickstart 3.0 or later to mount writably; read-only mount is possible
on older Kickstarts if the cache blocks are ignored.

### 15.6 OFS with directory cache — `DOS\4`

Same as OFS but with directory cache blocks. Nobody used this in
practice because OFS was already being phased out, but it exists for
completeness.

### 15.7 Kickstart version matrix

| Kickstart | Version | ROM FFS? | DOS\0 | DOS\1 | DOS\2 | DOS\3 | DOS\4 | DOS\5 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1.0/1.1 | V30/V31 | — | yes | — | — | — | — | — |
| 1.2 | V33 | — | yes | — | — | — | — | — |
| 1.3 | V34 | no (on disk L:FastFileSystem) | yes | yes* | — | — | — | — |
| 2.0 | V36 | yes | yes | yes | — | — | — | — |
| 2.04/2.1 | V37/V38 | yes | yes | yes | yes | yes | — | — |
| 3.0 | V39 | yes | yes | yes | yes | yes | yes | yes |
| 3.1 | V40 | yes | yes | yes | yes | yes | yes | yes |

*In 1.3, FFS ships on disk as `L:FastFileSystem` and is loaded by
the RDB or by a MOUNT entry — it is not in ROM.

### 15.8 Third-party filesystems

None in the corpus; mentioned here for completeness because any
modern emulator will encounter them:

- **CrossDOS** (FAT 12/16, MS-DOS floppies). Ships with Workbench
  from 2.04 onward as `L:CrossDOSFileSystem`.
- **SFS** — Smart File System, 1998+, a journaling replacement.
- **PFS2/PFS3** — Professional File System, 1995+, speed-focused.
- **FastFileSystem2** — Commodore's own FFS2 that shipped with
  AmigaOS 3.5/3.9, adding long filenames and larger partitions.

Each is just another handler plugged into the DOS packet interface,
with a mountlist or RDB FSHD entry pointing at its binary. The
emulator doesn't need to know about them — if it correctly implements
the packet interface, they run unmodified.

---

## 16. `startup-sequence` and user-mode boot

Once the filesystem is mounted as SYS:, dos.library creates the first
CLI by LoadSeg'ing `C:Shell` (or `C:CLI` in 1.x) and starting it with
`S:Startup-Sequence` as its script file. The Shell's first act is
therefore to Execute that script.

### 16.1 Typical 2.0 `Startup-Sequence`

    ; $VER: Startup-sequence 39.5 (21.2.92)
    C:SetPatch >NIL: ?
    C:Version >NIL:
    AddBuffers >NIL: DF0: 25
    FailAt 21
    C:MakeDir RAM:T RAM:Clipboards RAM:ENV RAM:ENV/Sys
    C:Copy >NIL: ENVARC: RAM:ENV ALL NOREQ
    Resident CLI C:Execute PURE
    Resident C:Mount PURE
    C:Assign ENV: RAM:ENV
    C:Assign T:   RAM:T
    C:Assign CLIPS: RAM:Clipboards
    C:Assign REXX: S:
    C:Assign PRINTERS: DEVS:Printers
    C:Assign KEYMAPS:  DEVS:Keymaps
    C:Assign LOCALE:   SYS:Locale
    BindDrivers
    SetEnv Workbench $Workbench
    SetEnv Kickstart $Kickstart
    IPrefs
    If Exists S:User-Startup
        Execute S:User-Startup
    EndIf
    Path RAM: C: SYS:Utilities SYS:System S: SYS:Prefs SYS:WBStartup ADD
    LoadWB
    EndCLI >NIL:

### 16.2 What each line does

- **`SetPatch`**: installs ROM patches (bug fixes to the Kickstart)
  and operating-system-wide customisations. Must come first.
- **`Version`**: prints the Kickstart/Workbench version; used by
  startup scripts to diagnose problems.
- **`AddBuffers DF0: 25`**: sends ACTION_MORE_CACHE to the DF0:
  filesystem, requesting 25 additional buffers (each one is a
  512-byte block). Default is 5 — 25 is much nicer for floppy use.
- **`FailAt 21`**: sets `cli_FailLevel = 21`. Commands returning
  lower values are not considered failures and the script continues.
- **`MakeDir RAM:T ...`**: creates the standard temporary directory
  hierarchy in the RAM disk.
- **`Copy ENVARC: RAM:ENV ALL NOREQ`**: copies the permanent
  environment archive from disk to RAM. `NOREQ` suppresses requesters
  if something goes wrong.
- **`Resident C:Execute PURE`**: loads `Execute` into memory and
  makes it resident. `PURE` asserts the binary is re-entrant (the
  `p` protection bit is set on its file) so multiple Shell instances
  can share one copy in memory.
- **`Assign ENV: RAM:ENV`**: creates a DLT_DIRECTORY entry in the
  DOS list so that `ENV:foo` resolves to `RAM:ENV/foo`. Every
  `SetEnv`/`GetEnv` goes through this assign.
- **`BindDrivers`**: walks `SYS:Expansion/` and
  `SYS:Devs/DOSDrivers/`, reading each file as a mountlist-like
  entry or a boardclass driver, and adding the resulting DeviceNodes
  to the DOS list. This is how non-boot filesystems — CrossDOS,
  extra hard disk partitions, network file systems — get mounted
  without the user having to MOUNT them by hand.
- **`IPrefs`**: the Intuition preferences daemon. Reads environment
  variables from `ENV:Sys/*.prefs` and applies them to the screen,
  pointer, input device, and so on. Must be running before LoadWB so
  that Workbench opens on the correctly-configured screen.
- **`If Exists S:User-Startup ... EndIf`**: chain into the user's
  own customisations, which are isolated from the system-provided
  file so that updates to the system `Startup-Sequence` don't
  overwrite the user's additions.
- **`Path RAM: C: ...`**: sets the Shell's command search path.
- **`LoadWB`**: start the Workbench process. This is a Process just
  like any other, but its program is `C:LoadWB`. It takes over the
  default public screen and starts rendering disks, drawers, and
  icons. Workbench's own child processes (things launched via
  WBStartup or by double-clicking an icon) become DOS processes
  with `pr_CLI == 0`.
- **`EndCLI >NIL:`**: terminates this CLI. The original CLI window
  closes. Workbench is now the only user-facing process; if the user
  wants a Shell, they open a Shell drawer icon or press a hotkey.

### 16.3 What LoadWB does

`LoadWB` is a small program that:

1. OpenLibrary("workbench.library").
2. Calls a workbench.library function that starts the Workbench
   task if it isn't already running.
3. Waits for Workbench to open the default public screen.
4. Exits.

The Workbench task itself is responsible for:

- Reading every mounted volume and producing an icon for each.
- Walking each volume's root directory and showing drawers/tools
  that have `.info` files.
- Handling the WBStartup folder (each `.info` there starts a tool
  at boot).
- Dispatching mouse and keyboard events, opening drawers,
  launching tools.
- Responding to disk-change events by refreshing the appropriate
  icons.

Workbench uses dos.library extensively — `ExAll`, `Examine`,
`Lock`, and `Open` on `.info` files — but it is a client of dos,
not a part of it.

---

## Appendix A — Packet type table

Complete list of `ACTION_*` codes defined by Commodore, in numeric
order. Packet numbers 0..2049 are reserved for Commodore. Packets
2050..2999 are reserved for third-party developers (with the
exception of 2008/2009/4097/4098 which Commodore uses). The rest are
reserved for future expansion.

| Dec | Hex | Name | Arguments | Function |
| --- | --- | --- | --- | --- |
| 0 | 0x0000 | ACTION_NIL | — | sentinel; used as return with no action |
| 2 | 0x0002 | ACTION_GET_BLOCK | obsolete | — |
| 4 | 0x0004 | ACTION_SET_MAP | obsolete | — |
| 5 | 0x0005 | ACTION_DIE | — | tell a handler to terminate |
| 6 | 0x0006 | ACTION_EVENT | obsolete | — |
| 7 | 0x0007 | ACTION_CURRENT_VOLUME | — | returns BPTR to volume node; sendpkt only |
| 8 | 0x0008 | ACTION_LOCATE_OBJECT | LOCK base, BSTR name, LONG mode | `Lock()` |
| 9 | 0x0009 | ACTION_RENAME_DISK | BSTR new name | `Relabel()` (2.0) |
| 15 | 0x000F | ACTION_FREE_LOCK | LOCK | `UnLock()` |
| 16 | 0x0010 | ACTION_DELETE_OBJECT | LOCK base, BSTR name | `DeleteFile()` |
| 17 | 0x0011 | ACTION_RENAME_OBJECT | LOCK, BSTR, LOCK, BSTR | `Rename()` |
| 18 | 0x0012 | ACTION_MORE_CACHE | LONG delta | `AddBuffers()` |
| 19 | 0x0013 | ACTION_COPY_DIR | LOCK | `DupLock()` |
| 20 | 0x0014 | ACTION_WAIT_CHAR | ULONG timeout (μs) | `WaitForChar()` |
| 21 | 0x0015 | ACTION_SET_PROTECT | -, LOCK, BSTR, LONG mask | `SetProtection()` |
| 22 | 0x0016 | ACTION_CREATE_DIR | LOCK, BSTR | `CreateDir()` |
| 23 | 0x0017 | ACTION_EXAMINE_OBJECT | LOCK, BPTR fib | `Examine()` |
| 24 | 0x0018 | ACTION_EXAMINE_NEXT | LOCK, BPTR fib | `ExNext()` |
| 25 | 0x0019 | ACTION_DISK_INFO | BPTR info | `Info()` (current volume) |
| 26 | 0x001A | ACTION_INFO | LOCK, BPTR info | `Info()` (by lock) |
| 27 | 0x001B | ACTION_FLUSH | — | `Flush()` all buffers |
| 28 | 0x001C | ACTION_SET_COMMENT | -, LOCK, BSTR, BSTR | `SetComment()` |
| 29 | 0x001D | ACTION_PARENT | LOCK | `ParentDir()` |
| 30 | 0x001E | ACTION_TIMER | — | internal: timer reply |
| 31 | 0x001F | ACTION_INHIBIT | BOOL | `Inhibit()` (2.0) |
| 32 | 0x0020 | ACTION_DISK_TYPE | obsolete | — |
| 33 | 0x0021 | ACTION_DISK_CHANGE | obsolete | replaced by Inhibit |
| 34 | 0x0022 | ACTION_SET_DATE | LOCK, BPTR DateStamp | `SetFileDate()` (2.0) |
| 40 | 0x0028 | ACTION_SAME_LOCK | LOCK, LOCK | `SameLock()` (2.0) |
| 82 | 0x0052 | ACTION_READ ('R') | ARG1, APTR buf, LONG len | `Read()` |
| 87 | 0x0057 | ACTION_WRITE ('W') | ARG1, APTR buf, LONG len | `Write()` |
| 994 | 0x03E2 | ACTION_SCREEN_MODE | LONG mode | `SetMode()` (console) |
| 1001 | 0x03E9 | ACTION_READ_RETURN | — | internal: async read done |
| 1002 | 0x03EA | ACTION_WRITE_RETURN | — | internal: async write done |
| 1004 | 0x03EC | ACTION_FINDUPDATE | BPTR fh, LOCK, BSTR | `Open(MODE_READWRITE)` |
| 1005 | 0x03ED | ACTION_FINDINPUT | BPTR fh, LOCK, BSTR | `Open(MODE_OLDFILE)` |
| 1006 | 0x03EE | ACTION_FINDOUTPUT | BPTR fh, LOCK, BSTR | `Open(MODE_NEWFILE)` |
| 1007 | 0x03EF | ACTION_END | ARG1 | `Close()` |
| 1008 | 0x03F0 | ACTION_SEEK | ARG1, LONG pos, LONG mode | `Seek()` |
| 1020 | 0x03FC | ACTION_FORMAT | BSTR dev, BSTR vol, LONG type | `Format()` (2.0) |
| 1021 | 0x03FD | ACTION_MAKE_LINK | LOCK, BSTR, BPTR, LONG | `MakeLink()` (2.0) |
| 1022 | 0x03FE | ACTION_SET_FILE_SIZE | BPTR fh, LONG, LONG mode | `SetFileSize()` (2.0); also ACTION_TRUNCATE |
| 1023 | 0x03FF | ACTION_WRITE_PROTECT | BOOL, LONG passkey | (FFS, sendpkt only) |
| 1024 | 0x0400 | ACTION_READ_LINK | LOCK, CPTR, APTR, LONG | `ReadLink()` — note CPTR |
| 1026 | 0x0402 | ACTION_FH_FROM_LOCK | BPTR fh, BPTR lock | `OpenFromLock()` |
| 1027 | 0x0403 | ACTION_IS_FILESYSTEM | — | `IsFileSystem()` (2.0) |
| 1028 | 0x0404 | ACTION_CHANGE_MODE | LONG, BPTR, LONG | `ChangeMode()` (2.0) |
| 1030 | 0x0406 | ACTION_COPY_DIR_FH | BPTR fh | `DupLockFromFH()` (2.0) |
| 1031 | 0x0407 | ACTION_PARENT_FH | BPTR fh | `ParentOfFH()` (2.0) |
| 1033 | 0x0409 | ACTION_EXAMINE_ALL | LOCK, APTR, LONG, LONG, BPTR | `ExAll()` (2.0) |
| 1034 | 0x040A | ACTION_EXAMINE_FH | BPTR fh, BPTR fib | `ExamineFH()` (2.0) |
| 2008 | 0x07D8 | ACTION_LOCK_RECORD | BPTR fh, LONG, LONG, LONG, LONG | `LockRecord()` (2.0) |
| 2009 | 0x07D9 | ACTION_FREE_RECORD | BPTR fh, LONG, LONG | `FreeRecord()` (2.0) |
| 4097 | 0x1001 | ACTION_ADD_NOTIFY | BPTR NotifyRequest | `StartNotify()` (2.0) |
| 4098 | 0x1002 | ACTION_REMOVE_NOTIFY | BPTR NotifyRequest | `EndNotify()` (2.0) |

Note: `ACTION_READ` = 82 and `ACTION_WRITE` = 87 are literal ASCII
codes (`'R'`, `'W'`) from the BCPL era. Similarly, `ACTION_TIMER` =
30 is `'\x1E'`. These are historical quirks of the original TripOS
packet definitions; Commodore preserved them for compatibility.

---

## Appendix B — OFS/FFS block layouts

Dimensions assume a 512-byte / 128-longword block. `SIZE` = 128.

### Root block (OFS)

| Longword | Name | Meaning |
| --- | --- | --- |
| 0 | T_SHORT = 2 | primary type |
| 1 | 0 | header key |
| 2 | 0 | highest seq |
| 3 | 72 | HTSIZE (hash table size) |
| 4 | 0 | reserved |
| 5 | CHECKSUM | balance-to-0 |
| 6..77 | HASH_TABLE[72] | block numbers of dir entries |
| 78 | BMFLAG | TRUE if on-disk bitmap valid |
| 79..103 | BITMAP_KEYS[25] | bitmap block numbers |
| 104 | 0 | reserved |
| 105..107 | DAYS, MINS, TICKS | last altered |
| 108..119 | DISK_NAME (BCPL) | up to 30 chars |
| 120..122 | CREATE_DAYS/MINS/TICKS | creation |
| 123 | 0 | hashchain |
| 124 | 0 | parent |
| 125 | 0 | extension |
| 126 | 0 | reserved |
| 127 | ST_ROOT = 1 | secondary type |

### Root block (FFS)

Same as OFS through the hash table and bitmap keys. Tail differs:

| Longword | Name |
| --- | --- |
| 104 | BITMAP_EXTEND (0 or block ptr) |
| 105..107 | DIR_ALTERED DateStamp |
| 108..117 | DISK_NAME (BCPL, 30 chars) |
| 118..120 | DISK_ALTERED DateStamp |
| 121..123 | DISK_MADE DateStamp |
| 124..126 | 0, 0, 0 (reserved) |
| 127 | ST_ROOT = 1 |

### User directory block (OFS)

| Longword | Name |
| --- | --- |
| 0 | T_SHORT = 2 |
| 1 | OWN_KEY |
| 2 | 0 |
| 3 | 0 |
| 4 | 0 |
| 5 | CHECKSUM |
| 6..77 | HASH_TABLE[72] |
| 78 | spare |
| 80 | PROTECT bits |
| 81 | 0 |
| 82..103 | COMMENT (BCPL, 80 bytes) |
| 105..107 | DAYS/MINS/TICKS (creation) |
| 108..123 | DIR_NAME (BCPL, 30 chars) |
| 124 | HASHCHAIN |
| 125 | PARENT |
| 126 | 0 |
| 127 | ST_USERDIR = 2 |

### User directory block (FFS)

Identical to OFS layout except `DIR_CREATED` at 105..107 is a single
DateStamp (the fields are named differently in the manual; the
layout itself is identical).

### File header block

| Longword | Name |
| --- | --- |
| 0 | T_SHORT = 2 |
| 1 | OWN_KEY |
| 2 | HIGHEST_SEQ (data blocks in this header) |
| 3 | DATA_SIZE (slots used) |
| 4 | FIRST_DATA block # |
| 5 | CHECKSUM |
| 6..77 | DATA_BLOCK_LIST (grows *downward* from 77 toward 6) |
| 78 | spare |
| 80 | PROTECT |
| 81 | BYTE_SIZE (file size in bytes) |
| 82..103 | COMMENT (BCPL) |
| 105..107 | DAYS/MINS/TICKS |
| 108..123 | FILE_NAME (BCPL) |
| 124 | HASHCHAIN |
| 125 | PARENT (directory) |
| 126 | EXTENSION block or 0 |
| 127 | ST_FILE = -3 |

### File extension block

| Longword | Name |
| --- | --- |
| 0 | T_LIST = 16 |
| 1 | OWN_KEY |
| 2 | BLOCK_COUNT |
| 3 | DATA_SIZE |
| 4 | FIRST_DATA |
| 5 | CHECKSUM |
| 6..77 | DATA_BLOCK_LIST |
| 78..122 | (info area, unused) |
| 123 | 0 |
| 125 | PARENT (file header) |
| 126 | next EXTENSION or 0 |
| 127 | ST_FILE = -3 |

### OFS data block

| Longword | Name |
| --- | --- |
| 0 | T_DATA = 8 |
| 1 | HEADER_KEY (file header) |
| 2 | SEQNUM (1..n) |
| 3 | DATA_SIZE (bytes of user data) |
| 4 | NEXT_DATA block # |
| 5 | CHECKSUM |
| 6..127 | data (up to 488 bytes) |

### FFS data block

Just 512 bytes of raw file data. No header, no checksum, no
back-pointer. Emulators that interpret FFS data blocks must rely
on the file header's block list to know what's what.

### Bitmap block

| Longword | Name |
| --- | --- |
| 0 | CHECKSUM |
| 1..127 | BITMAP (one bit per data block, 1 = free) |

### Bitmap extension block

| Longword | Name |
| --- | --- |
| 0..126 | Additional BITMAP_KEYS |
| 127 | NEXT bitmap extension block or 0 |

### Boot block

| Longword | Name |
| --- | --- |
| 0 | bb_id (4 bytes: "DOS" + version 0..5) |
| 1 | bb_chksum (one's-complement checksum over 256 longwords) |
| 2 | bb_dosblock (block # of root, usually 880 on a floppy) |
| 3..255 | bootstrap code |

---

## Appendix C — `dos.library` function index

Alphabetic index of the functions described in §4, one line each.
Version markers: "[2.0]" = added in Kickstart 2.0/V36. Everything else
is available from 1.0/V30 unless noted.

    AbortPkt        abort a previously sent packet [2.0]
    AddBuffers      change a handler's buffer cache size
    AddDosEntry     add a DosList entry (volume/device/assign) [2.0]
    AddPart         string-append a path component [2.0]
    AddSegment      add a named segment to the resident list
    AllocDosObject  allocate a standard DOS object [2.0]
    AssignAdd       add a lock to a multidir assign [2.0]
    AssignLate      create a late-binding assign [2.0]
    AssignLock      create an assign bound to a lock [2.0]
    AssignPath      create an assign to a path (late) [2.0]
    AttemptLockDosList   non-blocking LockDosList [2.0]
    ChangeMode      change mode of a file/lock [2.0]
    CheckSignal     non-blocking signal check [2.0]
    Cli             return pr_CLI of this process
    CliInitNewcli   per-CLI startup hook (for NEWCLI)
    CliInitRun      per-CLI startup hook (for RUN)
    Close           close a FileHandle
    CompareDates    compare two DateStamps
    CreateDir       create a directory, return lock
    CreateNewProc   create a new DOS process by taglist [2.0]
    CreateProc      create a new DOS process (1.x style)
    CurrentDir      change current directory, return old
    DateStamp       fill in a DateStamp with current time
    DateToStr       convert DateStamp to string [2.0]
    Delay           sleep for N ticks (1/50 s)
    DeleteFile      delete a file/directory
    DeleteVar       delete a local variable [2.0]
    DeviceProc      get MsgPort of the handler for a device name
    DoPkt           send a packet and wait for reply [2.0]
    DupLock         create a shared copy of a lock
    Examine         fill FileInfoBlock for a locked object
    ExamineFH       fill FileInfoBlock for an open file [2.0]
    ExNext          get the next directory entry
    ExAll           bulk directory examine [2.0]
    Execute         run a CLI command synchronously
    Exit            terminate the current process
    FGetC           read one buffered character [2.0]
    FGets           read a buffered line [2.0]
    FPutC           write one buffered character [2.0]
    FPuts           write a buffered string [2.0]
    FilePart        return the basename of a path [2.0]
    FindArg         find an argument in a template [2.0]
    FindCliProc     look up a CLI by number
    FindDosEntry    find a DosList entry by name [2.0]
    FindSegment     find a named resident segment
    FindVar         find a local variable [2.0]
    Flush           flush a buffered stream [2.0]
    Format          format media via a handler [2.0]
    FRead           buffered block read [2.0]
    FreeArgs        free a ReadArgs result [2.0]
    FreeDeviceProc  release a GetDeviceProc result [2.0]
    FreeDosEntry    free a DosList entry built by MakeDosEntry [2.0]
    FreeDosObject   free an AllocDosObject object [2.0]
    FWrite          buffered block write [2.0]
    GetArgStr       return pr_Arguments [2.0]
    GetConsoleTask  return pr_ConsoleTask
    GetCurrentDirName   return CWD as a string [2.0]
    GetDeviceProc   find handler, resolving assigns [2.0]
    GetFileSysTask  return pr_FileSystemTask
    GetProgramDir   return pr_HomeDir lock [2.0]
    GetProgramName  return current program name [2.0]
    GetPrompt       return the Shell prompt [2.0]
    GetVar          read a local variable [2.0]
    Info            get volume info from a lock
    Input           return pr_CIS
    InternalLoadSeg internal LoadSeg primitive
    InternalUnLoadSeg  internal UnLoadSeg
    IoErr           return pr_Result2
    IsFileSystem    ask if a handler is a filesystem [2.0]
    IsInteractive   ask if a file handle is interactive
    LoadSeg         load an executable file
    Lock            get a lock on a named object
    LockDosList     lock the DOS list for traversal [2.0]
    MakeDosEntry    create a new DosList entry [2.0]
    MatchEnd        finish a MatchFirst/MatchNext chain [2.0]
    MatchFirst      start a pattern-matching directory scan [2.0]
    MatchNext       continue a pattern scan [2.0]
    MatchPattern    test a name against a parsed pattern [2.0]
    MaxCli          return the maximum CLI count
    NameFromFH      get the name of an open file [2.0]
    NameFromLock    get the name of a locked object [2.0]
    NewLoadSeg      LoadSeg with taglist [2.0]
    NextDosEntry    walk to the next DosList entry [2.0]
    Open            open a file, return BPTR FileHandle
    Output          return pr_COS
    ParentDir       return a lock on an object's parent
    ParsePattern    parse a pattern string for MatchPattern [2.0]
    PathPart        return the dirname of a path [2.0]
    PrintFault      print an error code to stderr [2.0]
    PutStr          write a string to Output [2.0]
    Read            read bytes from a file handle
    ReadArgs        parse a command line to a template [2.0]
    ReadItem        read one token from a CSource [2.0]
    ReadLink        resolve a soft link [2.0]
    Relabel         rename the volume [2.0]
    RemAssignList   remove a lock from a multidir assign [2.0]
    RemDosEntry     remove a DosList entry [2.0]
    RemSegment      remove a named resident segment
    Rename          rename or move a file/directory
    ReplyPkt        reply to a packet received by a handler
    RunCommand      run a loaded command in this process [2.0]
    SameDevice      are two locks on the same device? [2.0]
    SameLock        are two locks on the same object? [2.0]
    Seek            change file position
    SelectInput     replace pr_CIS [2.0]
    SelectOutput    replace pr_COS [2.0]
    SendPkt         send a packet to a handler async
    SetArgStr       replace pr_Arguments [2.0]
    SetComment      set a file's comment
    SetConsoleTask  replace pr_ConsoleTask
    SetCurrentDirName   set CWD string [2.0]
    SetFileDate     set a file's date [2.0]
    SetFileSize     truncate or extend a file [2.0]
    SetFileSysTask  replace pr_FileSystemTask
    SetIoErr        set pr_Result2 [2.0]
    SetMode         console raw/cooked mode
    SetProgramDir   replace pr_HomeDir [2.0]
    SetProgramName  replace current program name [2.0]
    SetProtection   set file protection bits
    SetPrompt       set Shell prompt [2.0]
    SetVar          set a local variable [2.0]
    SetVBuf         set buffered I/O buffer size [2.0]
    StartNotify     start a file notification [2.0]
    StrToDate       parse a date string [2.0]
    StrToLong       parse a number string [2.0]
    System          run a CLI command with taglist [2.0]
    UnGetC          push back a character on a stream [2.0]
    UnLoadSeg       free a SegList
    UnLock          release a lock
    UnLockDosList   unlock the DOS list [2.0]
    VFPrintf        formatted print to a handle [2.0]
    VFWritef        BCPL-style formatted write [2.0]
    VPrintf         formatted print to Output [2.0]
    WaitForChar     wait for character input with timeout
    WaitPkt         wait for a packet on pr_MsgPort
    Write           write bytes to a file handle
    WriteChars      write raw bytes to Output [2.0]

---

## Appendix D — `trackdisk.device` command index

Full list of trackdisk commands from `devices/trackdisk.h`. The
decimal numbers are the raw command codes Exec uses; the ETD_
forms are the same numbers plus `TDF_EXTCOM = 0x8000` to signal
"this uses an IOExtTD, not an IOStdReq".

| Command | Code | IOReq type | Purpose |
| --- | --- | --- | --- |
| CMD_READ | 2 | IOStdReq | read sector(s) via track buffer |
| CMD_WRITE | 3 | IOStdReq | write sector(s) via track buffer |
| CMD_UPDATE | 4 | IOStdReq | flush track buffer if dirty |
| CMD_CLEAR | 5 | IOStdReq | invalidate track buffer |
| ETD_READ | 0x8002 | IOExtTD | read with disk-change check |
| ETD_WRITE | 0x8003 | IOExtTD | write with disk-change check |
| ETD_UPDATE | 0x8004 | IOExtTD | flush with disk-change check |
| ETD_CLEAR | 0x8005 | IOExtTD | invalidate with disk-change check |
| TD_MOTOR | 0x0009 (NONSTD+0) | IOStdReq | motor on/off, io_Length |
| TD_SEEK | 0x000A | IOStdReq | move heads to byte offset |
| TD_FORMAT | 0x000B | IOStdReq | write a full track raw |
| TD_REMOVE | 0x000C | IOStdReq | install software int on change (obsolete) |
| TD_CHANGENUM | 0x000D | IOStdReq | read disk change counter |
| TD_CHANGESTATE | 0x000E | IOStdReq | is disk present? |
| TD_PROTSTATUS | 0x000F | IOStdReq | is disk write-protected? |
| TD_RAWREAD | 0x0010 | IOStdReq | read raw MFM bits |
| TD_RAWWRITE | 0x0011 | IOStdReq | write raw MFM bits |
| TD_GETDRIVETYPE | 0x0012 | IOStdReq | return drive type ID |
| TD_GETNUMTRACKS | 0x0013 | IOStdReq | return track count for drive |
| TD_ADDCHANGEINT | 0x0014 | IOStdReq | register change interrupt (new) |
| TD_REMCHANGEINT | 0x0015 | IOStdReq | remove change interrupt |
| ETD_MOTOR | 0x8009 | IOExtTD | ETD form of TD_MOTOR |
| ETD_SEEK | 0x800A | IOExtTD | ETD form of TD_SEEK |
| ETD_FORMAT | 0x800B | IOExtTD | ETD form of TD_FORMAT |
| ETD_RAWREAD | 0x8010 | IOExtTD | ETD form of TD_RAWREAD |
| ETD_RAWWRITE | 0x8011 | IOExtTD | ETD form of TD_RAWWRITE |

The `TDF_EXTCOM` bit pattern (`1 << 15`) is OR-ed into the standard
command number to produce the extended form. The extended forms use
`struct IOExtTD` which adds `iotd_Count` (disk-change counter
threshold) and `iotd_SecLabel` (pointer to sector-label data) after
the standard `IOStdReq`.

Drive type constants (from `devices/trackdisk.h`):

    DRIVE3_5     = 1   /* 3.5" DD */
    DRIVE5_25    = 2   /* 5.25" for CrossDOS */
    DRIVE3_5_150RPM (rare HD drive) and a few others added in 3.0+

Error codes: see §11.5.

---

## Gaps in the corpus

The following topics are partial or missing in the provided source files
and were researched from the thinnest available coverage. An emulator
author should double-check these against the official V40 (Kickstart 3.1)
Amiga Developer CD autodocs if exact behaviour matters.

1. **Full dos.library autodoc text**. The provided Includes/Autodocs
   volume is heavy on trackdisk and Exec but light on dos.library —
   there is a 1.x-era summary in the AmigaDOS Manual and scattered
   autodoc extracts, but not the full numbered-offset list with
   SYNOPSIS/FUNCTION/RESULT for every function. The function index
   in Appendix C is assembled from the manual's quick-reference
   section and the `dos_lib.i` offsets file, not from full autodocs.
2. **`disk.resource` unit structure layout**. `AllocUnit`/`GetUnit`
   are documented at the API level but the private `Unit` struct
   used on the queuing side is only sketched. The
   `disk.resource/GetUnit` autodoc shows argument conventions but
   not the full struct that a consumer needs to allocate.
3. **Bitmap block exact layout across V36 vs. V39**. The 1.3/2.0
   FFS bitmap block is well documented; the 3.0 DirCache variant
   changes how bitmap extension chains work for very large
   partitions (>=2 GB). The corpus covers up to V37/V38 formally.
4. **RDB exact block definitions**. The "BadBlockBlock" struct is
   fully defined in Mapping the Amiga and RKM L&D mentions RDB
   concepts, but the full PartitionBlock / FileSystemHeaderBlock /
   LoadSegBlock struct definitions (from `devices/hardblocks.h`)
   are only partially present. §14 is assembled from the available
   field fragments and the general autoboot description.
5. **DirCache FFS block format**. The §7.11 description is generic.
   The exact on-disk layout of the DirCache block (which is
   pre-V39 content, so it's outside the strict 1991 cutoff of the
   ADOS Manual) is not in this corpus — you will need the V39
   includes volume.
6. **Exact timing of MFM read/write at the register level**. The
   HRM gives approximate timings and the `TD_RAWREAD` autodoc gives
   the 135-200 μs index-sync delay but does not specify the exact
   DMA start latency in terms of Agnus cycles. §13.5 is the
   high-level sequence; a cycle-exact emulator needs to model the
   DSKLEN-write to DMA-start gap (which depends on horizontal
   scanline phase) and the slot allocation algorithm.
7. **The CLI structure's exact trailing fields in 2.0+**. The
   `CommandLineInterface` struct in §6 is from the 1991 ADOS manual
   and matches `libraries/dosextens.h` through `cli_Module`. Later
   versions extended it (cli_Foo fields for newer Shell features).
   The extensions are not in this corpus.
8. **The boot-block checksum algorithm**. §8.3 gives the pseudo-code
   based on general Amiga programming knowledge, but the exact
   algorithm is not spelled out in any of the corpus files — it
   is implicit in the INSTALL command's source code (which is not in
   the corpus). A lookup against commonly-available Amiga sources
   (e.g. the ADF format spec) confirms the algorithm as written,
   but I have flagged this explicitly so that an emulator author
   who needs to *produce* valid boot blocks can cross-reference.

---

## Source map

The coverage in this document was assembled from the following files
in `/Users/stevehill/Desktop/AmigaPDFs/txt/`:

| File | Primary use |
| --- | --- |
| `1991-baker-jesup-et-al-the-amigados-manual-3rd-ed.txt` | DOS packet model, all `ACTION_*` semantics, FileHandle/FileLock/FileInfoBlock, Process struct fields, CLI struct fields, OFS/FFS on-disk format (ch. 9), hash algorithm, root block C example (ch. 9), BCPL/BPTR/BSTR explanation, full dos.library function list (ch. 6) — the primary source for the whole document |
| `Amiga_ROM_Kernel_Reference_Manual_Libraries_and_Devices.txt` | Full trackdisk.device chapter (ch. 7): commands, IO request structure, opening the device, sector size, error codes, example program, disk.resource API, device-specific semantics |
| `Amiga_ROM_Kernal_Reference_Manual_Includes_and_Autodocs.txt` | `libraries/dosextens.h` C struct definitions (Process, FileHandle, DosPacket, CommandLineInterface, DosList, DosLibrary, RootNode, DeviceList, Devinfo), `libraries/filehandler.h` (DosEnvec, FileSysStartupMsg, DeviceNode), `devices/trackdisk.h` (TD_* and ETD_* constants, IOExtTD struct, TDERR_* codes), `devices/bootblock.h` (BootBlock struct, BBID_DOS and BBID_KICK), trackdisk.device autodocs, disk.resource autodocs |
| `Amiga_Hardware_Reference_Manual_3rd_edition.txt` | Floppy controller registers (DSKPT/DSKLEN/DSKDAT/DSKBYTR/DSKSYNC/ADKCON), CIAA/CIAB bit assignments for disk subsystem, MFM sync word $4489, WORDSYNC/MSBSYNC/FAST, write precompensation, drive timing (step/settle), external disk connector pinout, identification-mode protocol, DMA start sequence |
| `Commodore_Amiga_A500_A2000_Technical_Reference_Manual_1987_Commodore_text.txt` | Cross-reference for CIA registers, external disk pinout, early A500/A2000 disk subsystem details |
| `1993-thomson-randy-rhett-anderson-mapping-amiga-2nd-edition.txt` | DosEnvec field offsets, BootBlock struct, DosList struct, DosPacket struct, BadBlockBlock (RDB) struct — the definitive "field offsets" cheat sheet |
| `Amiga_ROM_Kernel_Reference_Manual_Exec.txt` | Background for Exec primitives (MsgPort, Message, PutMsg/GetMsg/ReplyMsg/WaitPort), cross-referenced rather than duplicated |
| `Amiga_System_Programmers_Guide_1988_Abacus.txt` | Supporting coverage of filesystem handlers and packets (not primary) |
| `Amiga_Machine_Language_1991_Abacus.txt` | Supporting coverage (not primary) |
| `1990-beats-steve-amiga-rom-kernel-ref-3rd.txt` | Supporting coverage (not primary) |

Primary sources are cited inline where specific claims or autodoc
extracts appear. Everything else is derived from the combined reading
of the above files.

