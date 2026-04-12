# Amiga Workbench, Intuition V37+ and User-Facing GUI Subsystems

A reference for emulator authors and system programmers, covering everything the Amiga user sees above the line drawn by `amiga-graphics-display.md`: Intuition windows and screens, BOOPSI, GadTools, Workbench, icons, commodities, requesters, IFF, locale, and the utility/amiga.lib helpers every application uses.

This document is a companion to the six core references already in `/Users/stevehill/Desktop/AmigaPDFs/`:

- `amiga-exec-kernel.md` — tasks, processes, message ports, libraries, signals
- `amiga-dos-filesystem-disk.md` — DOS, filesystems, Locks, BPTRs
- `amiga-graphics-display.md` — Copper, bitplanes, sprites, blitter, graphics.library primitives, basic intuition screen/window creation
- `amiga-io-audio-expansion.md` — Paula, serial/parallel, trackdisk, AutoConfig
- `amiga-hardware-reference.md` — custom-chip register map, timings
- `amiga-boot-process.md` — Kickstart, ROMTags, AutoConfig, Workbench launch

Where a topic is already covered there, this document references rather than duplicates. The canonical sources mined here are the NDK 3.9 Autodocs, the NDK 3.9 C headers, and the 3rd-edition *Amiga ROM Kernel Reference Manual: Libraries* (the "V37 libraries book").

---

## Table of Contents

1. [Overview — where these pieces fit](#overview)
2. [Intuition V37+ architecture](#intuition-arch)
3. [Intuition structures (intuition.h, screens.h)](#intuition-structs)
4. [IDCMP classes — the complete list](#idcmp-classes)
5. [Window and Screen open tags](#window-screen-tags)
6. [BOOPSI — basic object-oriented programming system for Intuition](#boopsi)
7. [GadTools V36+](#gadtools)
8. [Menus — classic and GadTools](#menus)
9. [asl.library — file/font/screenmode requesters](#asl)
10. [iffparse.library](#iffparse)
11. [commodities.library V36+ — input broker](#commodities)
12. [Workbench and workbench.library](#workbench)
13. [icon.library and the .info file](#icon)
14. [datatypes.library V39+](#datatypes)
15. [locale.library V38+](#locale)
16. [utility.library V36+](#utility)
17. [amiga.lib — the link-library glue](#amigalib)
18. [Startup flow — how your `main()` gets called](#startup)
19. [Appendix A — BOOPSI class hierarchy (ASCII)](#appendix-boopsi-tree)
20. [Appendix B — Function index per library](#appendix-function-index)
21. [Appendix C — Gaps and emulator hazards](#appendix-gaps)
22. [Appendix D — Source map](#appendix-source-map)

---

<a name="overview"></a>
## 1. Overview — where these pieces fit

When Kickstart finishes booting and AmigaDOS has mounted its filesystems (see `amiga-boot-process.md` and `amiga-dos-filesystem-disk.md`), `dos.library` runs the startup script and eventually the LoadWB command brings up Workbench. Everything the user sees from that point — the screen, the menus, the requesters, the icons, the double-clicks — is driven by the subsystems described here, layered like this:

```
    +--------------------------------------------------+
    |  applications, shell tools, commodities          |
    +--------------------------------------------------+
    |  reaction / datatypes / gadtools / asl / iff     |  <-- V36+
    +--------------------------------------------------+
    |  Workbench (workbench.library + Workbench proc)  |
    |  icon.library                                    |
    +--------------------------------------------------+
    |  intuition.library  (+ BOOPSI)                   |
    +--------------------------------------------------+
    |  layers.library                                  |
    |  graphics.library  (see amiga-graphics-display)  |
    +--------------------------------------------------+
    |  exec  (tasks, ports, signals, libraries)        |
    +--------------------------------------------------+
    |  hardware (Agnus/Denise/Paula/CPU/CIAs)          |
    +--------------------------------------------------+
```

### The Workbench process

`Workbench` itself is not magic — it is an ordinary AmigaDOS Process (see `amiga-exec-kernel.md` §Process) with a public MsgPort called `"Workbench"` created during its startup. The **workbench.library** provides functions (`AddAppWindowA`, `AddAppIconA`, `AddAppMenuA`, `OpenWorkbenchObjectA`, ...) that applications use to hook into that process, and the Workbench process responds to internal messages by drawing icons, opening drawer windows, launching programs from double-clicks, dropping files onto AppWindows, and so on.

When Workbench launches a program (by double-click or by the shell's `WBRun`), it does not `Execute()` it the way the shell does. Instead it:

1. Loads the segment with `LoadSeg()`.
2. `CreateNewProc()` a new Process for the program, passing the segment.
3. Constructs a `struct WBStartup` message (see `workbench/startup.h`) containing an array of `struct WBArg` (one for the tool itself, plus one per selected project icon).
4. `PutMsg()`s that `WBStartup` to the new process's port and then `WaitPort` for it back.
5. When the program's `main()` returns, its startup glue `ReplyMsg()`s the `WBStartup` so Workbench knows it may `UnLoadSeg()` the segment.

So a Workbench-launched program finds itself already running as a Process, with no CLI (`pr_CLI == NULL`), and the very first thing the startup code does is `WaitPort(&pr_MsgPort); GetMsg(&pr_MsgPort);` to collect its WBStartup. A CLI-launched program skips this step entirely — the shell has set up its arguments on the stack in the traditional way.

This single discriminator — "did my process get a message on its startup port or not?" — is how C startup code decides whether to call `main(argc, argv)` or to parse `WBStartup->sm_ArgList` into a Workbench-style argument vector. (Section 18 below walks through this in detail.)

### The AppWindow / AppIcon / AppMenu flow

Once an application is running, workbench.library lets it receive drops and activations from the user. The mechanism is uniform:

1. The application allocates a MsgPort.
2. It calls `AddAppWindowA(id, userdata, window, msgport, tags)` or `AddAppIconA(...)` or `AddAppMenuA(...)`.
3. When the user drops icons on the window (or double-clicks the AppIcon, or picks the AppMenu), Workbench sends an `AppMessage` to that port.

The message is `struct AppMessage` from `workbench/workbench.h`:

```c
struct AppMessage {
    struct Message am_Message;  /* standard message structure */
    UWORD am_Type;              /* AMTYPE_APPWINDOW / APPICON / APPMENUITEM / ... */
    ULONG am_UserData;
    ULONG am_ID;
    LONG  am_NumArgs;           /* # of WBArgs */
    struct WBArg *am_ArgList;   /* the dropped icons */
    UWORD am_Version;
    UWORD am_Class;             /* e.g. AMCLASSICON_Open */
    WORD  am_MouseX, am_MouseY;
    ULONG am_Seconds, am_Micros;
    ULONG am_Reserved[8];
};
```

A `WBArg` is the same structure that appears in `WBStartup->sm_ArgList`:

```c
struct WBArg {
    BPTR  wa_Lock;   /* directory Lock (BPTR from dos.library) */
    BYTE *wa_Name;   /* name within that directory, or "" for the lock itself */
};
```

So dragging five files onto your AppWindow gives you one AppMessage with `am_NumArgs == 5` and `am_ArgList[0..4]` each containing a directory lock and a filename. The application must `CurrentDir(wa_Lock)` (or `NameFromLock`) to actually open each file. Never `UnLock` these locks — they belong to Workbench, and the AppMessage must be `ReplyMsg()`'d once you are done inspecting them.

### Relationship to the other six documents

| Topic | Covered in |
|---|---|
| CIA timing, copper, bitplanes | `amiga-hardware-reference.md`, `amiga-graphics-display.md` |
| BitMap, RastPort, ViewPort, Layer | `amiga-graphics-display.md` |
| Basic OpenScreen/OpenWindow (V34 style) | `amiga-graphics-display.md` |
| `MsgPort`, `PutMsg`, `WaitPort`, `GetMsg`, `Signal` | `amiga-exec-kernel.md` |
| `LoadSeg`, `CreateNewProc`, `Lock`, `DupLock` | `amiga-dos-filesystem-disk.md` |
| Kickstart, ROMTag scan, Workbench launch | `amiga-boot-process.md` |

This document picks up from there and covers **V36 and later** additions — tag-based window opens, public screens, BOOPSI, GadTools, commodities, asl, iff, the full workbench.library API, and icon.library. When in doubt, the authoritative split is: if it involves a `struct NewScreen` / `struct NewWindow` without tags, start with `amiga-graphics-display.md`; if it involves `OpenScreenTagList` / `OpenWindowTagList` / BOOPSI / GadTools, it is here.

---

<a name="intuition-arch"></a>
## 2. Intuition V37+ architecture

### The Intuition process and input handler

Intuition is not a passive library — it contains its own Process (the "Intuition" task) and an input handler that sits on `input.device`'s handler chain at priority 50 (higher priority than the commodities broker's default of 51, which specifically sits above Intuition's handler so it can intercept events first). The input chain, top-down, looks roughly like:

```
  input.device port (hardware events queued here)
        |
        v
  +-----------------------+    priority
  | commodities broker    |    51   (if installed)
  +-----------------------+
  | Intuition handler     |    50
  +-----------------------+
  | console.device        |    0
  +-----------------------+
```

Each handler in the chain gets a chance to consume, modify, or pass along each `InputEvent`. Intuition is what turns raw `IECLASS_RAWMOUSE` / `IECLASS_RAWKEY` events into IDCMP messages; commodities get first crack so they can filter or synthesize events before Intuition interprets them.

An important consequence: if your commodity broker eats an event (returns the chain with that event removed), Intuition never sees it, and the currently active window will not receive an IDCMP for it.

### Screens, Windows, Gadgets, Requesters, Menus, IDCMP

Intuition's world is five object types layered on top of the graphics library's ViewPort/RastPort/Layer:

- **Screen**: a full-width display configuration — a ViewPort, a BitMap, a LayerInfo, a default font, a pen array, and an optional screen title bar. Contains any number of windows. Implemented on top of `graphics.library`'s view/viewport and `layers.library`'s layer info. V36+ adds **public screens** (named, multi-application).
- **Window**: a rectangular region on a screen with its own Layer, RastPort, Gadgets list, IDCMP port, and optional menu strip. Has a border; optional system gadgets (close, depth, zoom, size).
- **Gadget**: a hit region in a window or screen with associated imagery and behavior. Types: Boolean (button), Proportional (slider), String, Integer, Custom (BOOPSI).
- **Requester**: a modal or modeless sub-dialog attached to a window. Contains its own gadget list. Can be filled by the application or, via `Request()`, by a `DMRequest` double-click requester, or by `BuildEasyRequest()` / `EasyRequest()` for one-shot question boxes.
- **Menu**: the title bar drop-down menu system. A linked list of `struct Menu`s, each pointing to a linked list of `struct MenuItem`s, each optionally pointing to a SubItem list.

Every window has an **IDCMP** — the Intuition Direct Communications Message Port — which is the MsgPort through which Intuition delivers user input events to your program. The set of events your window wants is described by a bitmask of `IDCMP_*` flags (see §4).

### What V37 added over V1.3

V34 (1.3) was the last of the "pre-tag" intuitions. From V36 (2.0) onwards Intuition gained:

- **Tag-list window/screen open calls** (`OpenWindowTagList`, `OpenScreenTagList`), which can be called with `NewWindow == NULL` — the tags supply everything.
- **Public screens** — `SA_PubName`, `LockPubScreen`, visitor windows (`WA_PubScreen`, `WA_PubScreenName`, `WFLG_VISITOR`).
- **BOOPSI**: rootclass, imageclass, icclass, gadgetclass, and the infrastructure (`NewObject`, `DisposeObject`, `SetAttrsA`, `GetAttr`, `MakeClass`, `AddClass`, `FreeClass`, `DoMethodA`).
- **Screen depth arrangement** (`MoveWindowInFrontOf`, `ScreenDepth` in V39), **zoom gadgets** (`WA_Zoom`, `WFLG_HASZOOM`, `ZipWindow`).
- **New IDCMP classes**: `IDCMP_IDCMPUPDATE`, `IDCMP_MENUHELP`, `IDCMP_CHANGEWINDOW`, `IDCMP_GADGETHELP` (V39).
- **Gadget tab-cycling** (`GFLG_TABCYCLE`), string gadget extensions (`StringExtend`), `ActivateGadget`.
- **DrawInfo** — a per-screen `struct DrawInfo` obtained via `GetScreenDrawInfo()`, giving you the screen's `dri_Pens[]` array, default font, and resolution.
- **New refresh model**: smart-refresh is the default; `BeginRefresh`/`EndRefresh` bracket damage redraw.
- **3D look ("new look")** — `SA_Pens`, `DRIF_NEWLOOK`, `PROPNEWLOOK`.
- **WA_BusyPointer** / pointerclass pointers (V39).

V37 specifically added `WA_MenuHelp`, `GFLG_TABCYCLE`, `ActivateGadget`, and a polished version of the V36 tag APIs. V38 added locale. V39 (OS 3.0) added `IDCMP_GADGETHELP`, `ExtGadget`, `GMORE_*`, screen double-buffering (`AllocScreenBuffer`), child screens, new `IA_FrameType`s and `MENUCHECK`/`AMIGAKEY` sysiclass sizes.

### Screen modes and DisplayID

V36+ screens are opened against a 32-bit **Display ID** (`SA_DisplayID`) rather than the old 16-bit `ViewModes` field. The Display ID encodes chipset variant, monitor (NTSC/PAL/VGA/Multiscan/A2024), and mode bits (HIRES/LACE/etc.). The authoritative list is `graphics/modeid.h` and is described in `amiga-graphics-display.md`. Here, all you need to know is that `SA_DisplayID` replaces `NewScreen.ViewModes` and can only be used via `OpenScreenTagList()`.

The full display database is queried through `graphics.library/GetDisplayInfoData()`, which returns tag-based records (`DISPLAYNAMEINFO`, `DIMENSIONINFO`, `MONITORINFO`, etc.). The ASL ScreenMode requester (§9.3) is built on top of this and gives the user a pick-list of all modes the system can actually produce.

### Public screens and visitor windows

A **public screen** is a screen with a name (`psn_Node.ln_Name`) that other applications can open windows on. The defining call is:

```c
OpenScreenTags(NULL,
    SA_PubName, "MYSCREEN",
    SA_Title,   "My Public Screen",
    SA_DisplayID, HIRES_KEY,
    TAG_DONE);
PubScreenStatus(myscreen, 0);   /* make it public */
```

After `PubScreenStatus()` has marked it public, other programs can do:

```c
struct Screen *s = LockPubScreen("MYSCREEN");
if (s) {
    OpenWindowTags(NULL,
        WA_PubScreen, s,
        WA_Title, "Visitor",
        TAG_DONE);
    UnlockPubScreen(NULL, s);
}
```

The call `LockPubScreen(NULL)` gets the default public screen (Workbench, unless overridden). `SetDefaultPubScreen()` changes the default. `NextPubScreen()` iterates.

Visitor windows keep their host screen alive through a visitor count (`psn_VisitorCount`). When the last visitor closes, Intuition signals `psn_SigTask`/`psn_SigBit` (set via `SA_PubTask`/`SA_PubSig`) so the screen owner can then `CloseScreen()` it cleanly.

The `WFLG_VISITOR` flag in `Window.Flags` is set by Intuition on visitor windows. You don't set it yourself.

### Cross-reference

For the underlying ViewPort setup, Copper list construction, bitplane allocation, and the RastPort graphics primitives (`Draw`, `Text`, `RectFill`, `BltBitMap`), see `amiga-graphics-display.md`. Everything below assumes you already have an Intuition Screen with a BitMap and a RastPort.

---

<a name="intuition-structs"></a>
## 3. Intuition structures

These are reproduced (with non-essential comments condensed) from `intuition/intuition.h` and `intuition/screens.h` in NDK 3.9, which are the authoritative definitions. See `Include/include_h/intuition/intuition.h` line references inline.

### 3.1 `struct Screen` (screens.h line 133)

```c
struct Screen
{
    struct Screen *NextScreen;      /* linked list of screens */
    struct Window *FirstWindow;     /* linked list Screen's Windows */

    WORD LeftEdge, TopEdge;
    WORD Width, Height;

    WORD MouseY, MouseX;            /* relative to upper-left */
    UWORD Flags;                    /* see definitions below */

    UBYTE *Title;
    UBYTE *DefaultTitle;            /* for Windows without ScreenTitle */

    /* Bar sizes.  BarHeight is one less than the actual menu bar
     * height for V36 compatibility. */
    BYTE BarHeight, BarVBorder, BarHBorder, MenuVBorder, MenuHBorder;
    BYTE WBorTop, WBorLeft, WBorRight, WBorBottom;

    struct TextAttr *Font;

    struct ViewPort ViewPort;       /* the Screen's display */
    struct RastPort RastPort;       /* describing Screen rendering */
    struct BitMap BitMap;           /* embedded; use RastPort.BitMap! */
    struct Layer_Info LayerInfo;

    struct Gadget *FirstGadget;     /* only system gadgets allowed here */

    UBYTE DetailPen, BlockPen;

    UWORD SaveColor0;               /* DisplayBeep save */

    struct Layer *BarLayer;         /* Screen and Menu bar layer */
    UBYTE *ExtData;
    UBYTE *UserData;

    /* Data beyond this point is SYSTEM PRIVATE */
};
```

**Emulator note:** the embedded `BitMap` is a compatibility hold-over. As the header explicitly warns, use `Screen->RastPort.BitMap` everywhere; do not rely on `&Screen->BitMap` being a valid long-term pointer. Under V39, BitMaps can grow larger than the embedded structure.

Screen flags (partial, see header for complete set):

| Flag | Hex | Meaning |
|---|---|---|
| `SCREENTYPE` | `0x000F` | mask for type |
| `WBENCHSCREEN` | `0x0001` | this is the Workbench screen |
| `PUBLICSCREEN` | `0x0002` | public (named) screen |
| `CUSTOMSCREEN` | `0x000F` | classic custom screen |
| `SHOWTITLE` | `0x0010` | title bar visible (toggled by `ShowTitle()`) |
| `BEEPING` | `0x0020` | `DisplayBeep()` is flashing color 0 |
| `CUSTOMBITMAP` | `0x0040` | screen uses user-supplied BitMap |
| `SCREENBEHIND` | `0x0080` | opened behind existing screens |
| `SCREENQUIET` | `0x0100` | Intuition will not render into this screen |
| `SCREENHIRES` | `0x0200` | private |
| `PENSHARED` | `0x0400` | V39; screen opened with `SA_SharePens,TRUE` |
| `NS_EXTENDED` | `0x1000` | `ExtNewScreen.Extension` is valid |
| `AUTOSCROLL` | `0x4000` | autoscroll when mouse hits edge |

### 3.2 `struct Window` (intuition.h line 909)

```c
struct Window
{
    struct Window *NextWindow;          /* linked list in a screen */

    WORD LeftEdge, TopEdge;             /* screen-relative */
    WORD Width, Height;
    WORD MouseY, MouseX;                /* relative to upper-left of window */
    WORD MinWidth, MinHeight;
    UWORD MaxWidth, MaxHeight;

    ULONG Flags;                        /* WFLG_ */

    struct Menu *MenuStrip;
    UBYTE *Title;

    struct Requester *FirstRequest;     /* all active Requesters */
    struct Requester *DMRequest;        /* double-click Requester */
    WORD ReqCount;                      /* count of reqs blocking Window */

    struct Screen *WScreen;
    struct RastPort *RPort;

    BYTE BorderLeft, BorderTop, BorderRight, BorderBottom;
    struct RastPort *BorderRPort;

    struct Gadget *FirstGadget;         /* does NOT include system gadgets */

    struct Window *Parent, *Descendant;

    /* Custom pointer sprite (obsolete; use pointerclass instead) */
    UWORD *Pointer;
    BYTE PtrHeight, PtrWidth;
    BYTE XOffset, YOffset;

    ULONG IDCMPFlags;                   /* user-selected */
    struct MsgPort *UserPort, *WindowPort;
    struct IntuiMessage *MessageKey;

    UBYTE DetailPen, BlockPen;

    struct Image *CheckMark;
    UBYTE *ScreenTitle;                 /* screen title when this window is active */

    WORD GZZMouseX, GZZMouseY;          /* WFLG_GIMMEZEROZERO inner mouse */
    WORD GZZWidth, GZZHeight;

    UBYTE *ExtData;
    BYTE *UserData;

    struct Layer *WLayer;               /* duplicates RPort->Layer */
    struct TextFont *IFont;             /* font OpenWindow opened */
    ULONG MoreFlags;                    /* V36+, system private */

    /* Data beyond this point is Intuition private. */
};
```

**Key points for emulation:**
- `UserPort` is where IDCMP messages arrive. It may be NULL until you request IDCMP. If you pass `WA_IDCMP != 0` to `OpenWindowTagList`, Intuition allocates a port for you; if you pass your own via `ModifyIDCMP()` you get to share it between windows.
- `WindowPort` is the port Intuition uses to talk *to* you from its side — it's Intuition's private half.
- `RPort` is the window's RastPort; `RPort->Layer == WLayer`. For GIMMEZEROZERO windows, `RPort` points at the **inner** RastPort and there's a separate `BorderRPort` for drawing borders without clipping issues.
- `IDCMPFlags` is what's currently enabled. `ModifyIDCMP()` changes it.

### Window flags (WFLG_*)

From `intuition.h` line 1022 onward:

| Flag | Hex | Application sets | Meaning |
|---|---|---|---|
| `WFLG_SIZEGADGET` | `0x00000001` | yes | include sizing system gadget |
| `WFLG_DRAGBAR` | `0x00000002` | yes | include title-bar drag gadget |
| `WFLG_DEPTHGADGET` | `0x00000004` | yes | include depth-arrange gadget |
| `WFLG_CLOSEGADGET` | `0x00000008` | yes | include close box |
| `WFLG_SIZEBRIGHT` | `0x00000010` | yes | size gadget uses right border |
| `WFLG_SIZEBBOTTOM` | `0x00000020` | yes | size gadget uses bottom border |
| `WFLG_SMART_REFRESH` | `0x00000000` | yes | (default) smart refresh |
| `WFLG_SIMPLE_REFRESH` | `0x00000040` | yes | simple refresh |
| `WFLG_SUPER_BITMAP` | `0x00000080` | yes | super bitmap (requires GZZ) |
| `WFLG_BACKDROP` | `0x00000100` | yes | backdrop window |
| `WFLG_REPORTMOUSE` | `0x00000200` | yes | receive every mouse move |
| `WFLG_GIMMEZEROZERO` | `0x00000400` | yes | inner RastPort origin at (0,0) |
| `WFLG_BORDERLESS` | `0x00000800` | yes | no border imagery |
| `WFLG_ACTIVATE` | `0x00001000` | yes | activate when opened |
| `WFLG_WINDOWACTIVE` | `0x00002000` | no | Intuition: window is active |
| `WFLG_INREQUEST` | `0x00004000` | no | Intuition: window has active requester |
| `WFLG_MENUSTATE` | `0x00008000` | no | Intuition: window is in menu mode |
| `WFLG_RMBTRAP` | `0x00010000` | yes | catch RMB events (suppress menus) |
| `WFLG_NOCAREREFRESH` | `0x00020000` | yes | don't send REFRESHWINDOW events |
| `WFLG_NW_EXTENDED` | `0x00040000` | yes | NewWindow is really ExtNewWindow |
| `WFLG_NEWLOOKMENUS` | `0x00200000` | yes | V39+: window has new-look menus |
| `WFLG_WINDOWREFRESH` | `0x01000000` | no | Intuition: window is refreshing |
| `WFLG_WBENCHWINDOW` | `0x02000000` | no | Workbench tool window only |
| `WFLG_WINDOWTICKED` | `0x04000000` | no | INTUITICKS throttling |
| `WFLG_VISITOR` | `0x08000000` | no | visitor window on a public screen |
| `WFLG_ZOOMED` | `0x10000000` | no | currently in zoom state |
| `WFLG_HASZOOM` | `0x20000000` | no | has a zoom gadget |

### Refresh model

Every window has one of three refresh modes:

- **Smart refresh** — Intuition allocates off-screen bitmap storage for the regions of the window covered by other windows, and when the window is revealed it blits those regions back. Fastest redraw path; uses the most memory.
- **Simple refresh** — when covered regions are revealed, Intuition sends `IDCMP_REFRESHWINDOW` to the application, which must call `BeginRefresh(window)`, redraw, then `EndRefresh(window, TRUE)`. Cheaper on memory.
- **Super bitmap** — the application supplies the full bitmap; scrolling is done by repointing the layer. Requires `WFLG_GIMMEZEROZERO`.

The V36 tags `WA_SmartRefresh`/`WA_SimpleRefresh` (booleans) set the mode via the tag interface.

### 3.3 `struct IntuiMessage` (intuition.h line 763)

This is the message your app's loop pulls off `window->UserPort` via `GetMsg()`:

```c
struct IntuiMessage
{
    struct Message ExecMessage;         /* (link to MsgPort, etc.) */
    ULONG  Class;                       /* IDCMP_ code, see §4 */
    UWORD  Code;                        /* per-class code */
    UWORD  Qualifier;                   /* input event qualifier */
    APTR   IAddress;                    /* object pointer, per class */
    WORD   MouseX, MouseY;              /* mouse relative to window */
    ULONG  Seconds, Micros;             /* system clock snapshot */
    struct Window *IDCMPWindow;
    struct IntuiMessage *SpecialLink;   /* system use */
};
```

V39 extends this with `struct ExtIntuiMessage`, which has an `eim_TabletData` pointer after the IntuiMessage — only valid when the window was opened with `WA_TabletMessages,TRUE` on V39+, and the application is running on V39+.

**Critical reply discipline:** Intuition owns these message structures. After you have copied everything you need, you **must** `ReplyMsg(imsg)`. While you hold a message, the fields of the window it describes (particularly `MouseX`, `MouseY`, `Title`) may continue to change, so don't hold onto messages longer than needed. Never `FreeMem()` an IntuiMessage; only `ReplyMsg()` it.

### 3.4 `struct Gadget` (intuition.h line 214)

```c
struct Gadget
{
    struct Gadget *NextGadget;

    WORD  LeftEdge, TopEdge;        /* hit box */
    WORD  Width, Height;

    UWORD Flags;                    /* GFLG_ */
    UWORD Activation;               /* GACT_ */
    UWORD GadgetType;               /* GTYP_ */

    APTR  GadgetRender;             /* Border * or Image * */
    APTR  SelectRender;             /* highlighted imagery */
    struct IntuiText *GadgetText;

    LONG  MutualExclude;            /* obsolete; reused as custom gadget hook */
    APTR  SpecialInfo;              /* StringInfo/PropInfo/BoolInfo */

    UWORD GadgetID;                 /* user-defined */
    APTR  UserData;
};
```

V39 introduces `struct ExtGadget`, which adds `MoreFlags` (`GMORE_BOUNDS`, `GMORE_GADGETHELP`, `GMORE_SCROLLRASTER`) and an explicit `Bounds{Left,Top,Width,Height}` rectangle. ExtGadgets always have `GFLG_EXTENDED` set. BOOPSI V39 gadgets are always ExtGadgets.

Gadget flag categories:

- **GFLG_GADGHxxx** (`0x0003` mask): highlight style — `GADGHCOMP` (complement), `GADGHBOX`, `GADGHIMAGE`, `GADGHNONE`.
- `GFLG_GADGIMAGE` (`0x0004`): imagery is an `Image` not a `Border`.
- **GFLG_RELxxx** (`0x0008..0x0040`): coordinates relative to bottom/right edges; width/height relative to window.
- `GFLG_SELECTED` (`0x0080`): currently selected.
- `GFLG_DISABLED` (`0x0100`): gadget is ghosted (`OnGadget`/`OffGadget`).
- `GFLG_TABCYCLE` (`0x0200`): V37+; participates in Tab/Shift-Tab cycling.
- `GFLG_STRINGEXTEND` (`0x0400`): string gadget has `StringExtend` (V37 compatible synonym for `GACT_STRINGEXTEND`).
- `GFLG_LABELMASK` (`0x3000`): `GadgetText` interpretation: `LABELITEXT` (IntuiText), `LABELSTRING` (plain UBYTE*), `LABELIMAGE` (Image object).
- `GFLG_EXTENDED` (`0x8000`): V39+; this is an ExtGadget.

Activation flags (`GACT_*`):

- `GACT_RELVERIFY` (`0x0001`): fire `IDCMP_GADGETUP` when released over gadget.
- `GACT_IMMEDIATE` (`0x0002`): fire `IDCMP_GADGETDOWN` on press.
- `GACT_ENDGADGET` (`0x0004`): used in requesters to close the requester.
- `GACT_FOLLOWMOUSE` (`0x0008`): send mouse-move events while active.
- `GACT_xxBORDER` (`0x0010..0x0080`): gadget lives in one of the window borders.
- `GACT_TOGGLESELECT` (`0x0100`): toggle, not momentary.
- `GACT_STRINGCENTER`/`RIGHT` (`0x0200`,`0x0400`): string justification.
- `GACT_LONGINT` (`0x0800`): this string gadget is integer-only.
- `GACT_ALTKEYMAP` (`0x1000`): uses `StringInfo->AltKeyMap`.
- `GACT_BOOLEXTEND` (`0x2000`): boolean gadget has a `BoolInfo`.
- `GACT_STRINGEXTEND` (`0x2000`): string gadget has `StringExtend`. **Never set on V34** — use `GFLG_STRINGEXTEND` instead.

Gadget types (`GTYP_*`), in `GadgetType` field:

- Low nibble: `GTYP_BOOLGADGET` (1), `GTYP_PROPGADGET` (3), `GTYP_STRGADGET` (4), `GTYP_CUSTOMGADGET` (5).
- `GTYP_REQGADGET` (`0x1000`): gadget is inside a Requester.
- `GTYP_GZZGADGET` (`0x2000`): gadget belongs to a GIMMEZEROZERO window's border RastPort.
- `GTYP_SCRGADGET` (`0x4000`): gadget is attached to a Screen, not a Window.
- `GTYP_SYSGADGET` (`0x8000`): gadget was allocated by Intuition (close, depth, etc). In that case `GTYP_SYSTYPEMASK` (`0x00F0`) distinguishes them: `GTYP_SIZING` (0x10), `GTYP_WDRAGGING` (0x20), `GTYP_SDRAGGING` (0x30), `GTYP_WDEPTH` (0x40), `GTYP_SDEPTH` (0x50), `GTYP_WZOOM` (0x60), `GTYP_CLOSE` (0x80).

### 3.5 `struct PropInfo` (intuition.h line 538)

```c
struct PropInfo
{
    UWORD Flags;            /* AUTOKNOB|FREEHORIZ|FREEVERT|PROPBORDERLESS|PROPNEWLOOK */

    /* Application-maintained pot values, 16-bit fixed point 0..0xFFFF */
    UWORD HorizPot, VertPot;

    /* AUTOKNOB body sizes — fraction of total visible, 16-bit fixed point */
    UWORD HorizBody, VertBody;

    /* Intuition-maintained; do not touch */
    UWORD CWidth, CHeight;
    UWORD HPotRes, VPotRes;
    UWORD LeftBorder, TopBorder;
};
```

`MAXPOT = 0xFFFF`, `MAXBODY = 0xFFFF`. Application sets `HorizPot`/`VertPot`/`HorizBody`/`VertBody` before `AddGadget` (directly) or during runtime via `NewModifyProp()`/`ModifyProp()`. The **container** is the rectangle drawn around the knob; `AUTOKNOB` means Intuition draws the knob automatically.

The `PROPNEWLOOK` flag (V36+) gives the 3D-look knob. Always set it on V36 or later.

### 3.6 `struct StringInfo` (intuition.h line 610)

```c
struct StringInfo
{
    UBYTE *Buffer;          /* application-supplied string buffer */
    UBYTE *UndoBuffer;      /* optional; shared among all string gadgets in a window */
    WORD  BufferPos;        /* cursor position within Buffer */
    WORD  MaxChars;         /* including null terminator */
    WORD  DispPos;          /* Buffer position of first displayed char */

    /* Intuition-maintained */
    WORD  UndoPos;
    WORD  NumChars;
    WORD  DispCount;
    WORD  CLeft, CTop;      /* container top-left offsets */

    struct StringExtend *Extension;     /* only if GACT_STRINGEXTEND */

    LONG  LongInt;          /* for GACT_LONGINT gadgets */
    struct KeyMap *AltKeyMap;
};
```

If `GACT_STRINGEXTEND` is set (V37+), `Extension` points to a `StringExtend` structure (see `intuition/sghooks.h`) giving you pen colors, work-buffer, custom edit hook, etc.

### 3.7 `struct BoolInfo` (intuition.h line 514)

```c
struct BoolInfo
{
    UWORD  Flags;           /* BOOLMASK */
    UWORD *Mask;            /* bitmask for hit-testing / highlight */
    ULONG  Reserved;        /* 0 */
};
```

A BoolInfo (present only when `GACT_BOOLEXTEND` is set) lets a boolean gadget have a non-rectangular hit region — the Mask pattern is a bitmap the same width and height as the gadget's select box.

### 3.8 `struct Menu`, `struct MenuItem`, `struct Requester`, `struct IntuiText`, `struct Border`, `struct Image`

```c
struct Menu
{
    struct Menu    *NextMenu;
    WORD LeftEdge, TopEdge;
    WORD Width, Height;
    UWORD Flags;                /* MENUENABLED, MIDRAWN */
    BYTE *MenuName;
    struct MenuItem *FirstItem;
    WORD JazzX, JazzY, BeatX, BeatY;   /* internal */
};

struct MenuItem
{
    struct MenuItem *NextItem;
    WORD LeftEdge, TopEdge;
    WORD Width, Height;
    UWORD Flags;                /* CHECKIT, ITEMTEXT, COMMSEQ, MENUTOGGLE,
                                   ITEMENABLED, HIGHxxx, CHECKED,
                                   ISDRAWN, HIGHITEM, MENUTOGGLED */
    LONG MutualExclude;         /* bitmask: bit n = "excludes item n" */
    APTR ItemFill;              /* Image/IntuiText or NULL */
    APTR SelectFill;            /* alternate for HIGHIMAGE mode */
    BYTE Command;               /* if COMMSEQ, the Amiga-key letter */
    struct MenuItem *SubItem;   /* or NULL */
    UWORD NextSelect;           /* drag-select chain */
};

struct Requester
{
    struct Requester *OlderRequest;
    WORD LeftEdge, TopEdge, Width, Height;
    WORD RelLeft, RelTop;
    struct Gadget *ReqGadget;
    struct Border *ReqBorder;
    struct IntuiText *ReqText;
    UWORD Flags;                /* POINTREL|PREDRAWN|NOISYREQ|SIMPLEREQ|
                                   USEREQIMAGE|NOREQBACKFILL|REQOFFWINDOW|
                                   REQACTIVE|SYSREQUEST|DEFERREFRESH */
    UBYTE BackFill;             /* pen */
    struct Layer *ReqLayer;
    UBYTE ReqPad1[32];
    struct BitMap *ImageBMap;   /* if PREDRAWN */
    struct Window *RWindow;
    struct Image  *ReqImage;    /* V36+ if USEREQIMAGE */
    UBYTE ReqPad2[32];
};

struct IntuiText
{
    UBYTE FrontPen, BackPen;
    UBYTE DrawMode;             /* JAM1/JAM2/COMPLEMENT/INVERSVID */
    WORD  LeftEdge, TopEdge;
    struct TextAttr *ITextFont; /* NULL = window default */
    UBYTE *IText;
    struct IntuiText *NextText;
};

struct Border
{
    WORD LeftEdge, TopEdge;
    UBYTE FrontPen, BackPen;
    UBYTE DrawMode;
    BYTE Count;                 /* number of XY pairs */
    WORD *XY;                   /* pairs, relative to LeftEdge/TopEdge */
    struct Border *NextBorder;
};

struct Image
{
    WORD LeftEdge, TopEdge;
    WORD Width, Height, Depth;
    UWORD *ImageData;           /* word-aligned planar bitmap */
    UBYTE PlanePick, PlaneOnOff;/* bit-pattern of which planes to use */
    struct Image *NextImage;
};
```

**PlanePick/PlaneOnOff logic (classic Image):** for each bit of the destination BitMap, starting at plane 0, check the corresponding bit in `PlanePick`. If set, take the next plane of `ImageData`. If clear, take the corresponding `PlaneOnOff` bit and fill that plane with 0 or 1. This lets you share a one-plane image across a multi-plane display without having to store the full depth.

V36+ BOOPSI images (Image pointers whose `Depth == CUSTOMIMAGEDEPTH` i.e. `-1`) are not plain planar bitmaps — they are object handles into `imageclass`. See §6.

### `FULLMENUNUM` encoding

Menu numbers in `IDCMP_MENUPICK` are packed into a single UWORD:

```c
#define MENUNUM(n)  ( n        & 0x001F)   /* menu   : 5 bits */
#define ITEMNUM(n)  ((n >>  5) & 0x003F)   /* item   : 6 bits */
#define SUBNUM(n)   ((n >> 11) & 0x001F)   /* subitem: 5 bits */

#define FULLMENUNUM(menu,item,sub) \
    ( SHIFTSUB(sub) | SHIFTITEM(item) | SHIFTMENU(menu) )

#define NOMENU   0x001F
#define NOITEM   0x003F
#define NOSUB    0x001F
#define MENUNULL 0xFFFF
```

So a code of `0xFFFF` means "no selection" and a code of `0x001F` in MENUNUM means "not a menu". `ItemAddress(menustrip, code)` resolves the packed number to a `MenuItem *`.

---

<a name="idcmp-classes"></a>
## 4. IDCMP classes — the complete list

An `IntuiMessage.Class` is a single `IDCMP_*` bit. The `IDCMPFlags` field of the Window (set by `WA_IDCMP` or `ModifyIDCMP()`) is a mask of all classes the app cares about; Intuition only delivers messages for enabled classes. This table lists **every** class, when Intuition fires it, what `Code`/`Qualifier`/`IAddress`/`MouseX`/`MouseY` mean, and any gotchas. Values match `intuition.h` line 832+.

| Class | Hex | Fires when | Code | Qualifier | IAddress | MouseX/Y |
|---|---|---|---|---|---|---|
| `IDCMP_SIZEVERIFY` | `0x00000001` | Size gadget hit, **before** Intuition resizes | — | current | — | current mouse |
| `IDCMP_NEWSIZE` | `0x00000002` | Window has just been resized | — | current | — | current mouse |
| `IDCMP_REFRESHWINDOW` | `0x00000004` | Simple-refresh window needs to redraw damage | — | current | — | current mouse |
| `IDCMP_MOUSEBUTTONS` | `0x00000008` | LMB/RMB/MMB press or release | `SELECTUP`/`SELECTDOWN`/`MENUUP`/`MENUDOWN`/`MIDDLEUP`/`MIDDLEDOWN` | current | — | at event |
| `IDCMP_MOUSEMOVE` | `0x00000010` | Mouse moved (see notes) | — | current | — | new position (or delta, with `IDCMP_DELTAMOVE`) |
| `IDCMP_GADGETDOWN` | `0x00000020` | Gadget activated (press) | gadget-specific | current | `Gadget *` | at event |
| `IDCMP_GADGETUP` | `0x00000040` | `GACT_RELVERIFY` gadget released over it | gadget-specific | current | `Gadget *` | at event |
| `IDCMP_REQSET` | `0x00000080` | First requester opened in window | — | current | — | current |
| `IDCMP_MENUPICK` | `0x00000100` | User released RMB during a menu session | packed MenuNumber (`FULLMENUNUM`) or `MENUNULL` | current | — | at event |
| `IDCMP_CLOSEWINDOW` | `0x00000200` | Close gadget hit | — | current | — | at event |
| `IDCMP_RAWKEY` | `0x00000400` | Raw key event | raw key code (bit 7 = up prefix) | qualifier | `InputEvent *` in V36+ if RPF_SWITCH | — |
| `IDCMP_REQVERIFY` | `0x00000800` | About to open a system requester — confirm OK | — | current | — | current |
| `IDCMP_REQCLEAR` | `0x00001000` | Last requester cleared from window | — | current | — | current |
| `IDCMP_MENUVERIFY` | `0x00002000` | About to enter menu mode, needs permission | `MENUHOT`/`MENUCANCEL`/`MENUWAITING` | current | — | current |
| `IDCMP_NEWPREFS` | `0x00004000` | Preferences changed | — | — | — | — |
| `IDCMP_DISKINSERTED` | `0x00008000` | Disk inserted | — | — | — | — |
| `IDCMP_DISKREMOVED` | `0x00010000` | Disk removed | — | — | — | — |
| `IDCMP_WBENCHMESSAGE` | `0x00020000` | System use only (Workbench ↔ Intuition) | `WBENCHOPEN`/`WBENCHCLOSE` | — | — | — |
| `IDCMP_ACTIVEWINDOW` | `0x00040000` | This window became active | — | current | — | current |
| `IDCMP_INACTIVEWINDOW` | `0x00080000` | This window became inactive | — | current | — | current |
| `IDCMP_DELTAMOVE` | `0x00100000` | Modifier: MOUSEMOVE coords become deltas | — | — | — | delta |
| `IDCMP_VANILLAKEY` | `0x00200000` | Cooked ASCII key | char | qualifier | — | — |
| `IDCMP_INTUITICKS` | `0x00400000` | Roughly 10/s timer ticks, throttled | — | current | — | current |
| `IDCMP_IDCMPUPDATE` | `0x00800000` | V36+: BOOPSI `OM_NOTIFY` routed here via `ICA_TARGET,ICTARGET_IDCMP` | depends on map (`ICSPECIAL_CODE`) | — | `TagItem *` (the attr list) | — |
| `IDCMP_MENUHELP` | `0x01000000` | V36+: Help key pressed during menu mode; requires `WA_MenuHelp,TRUE` | packed menu number under cursor | — | — | — |
| `IDCMP_CHANGEWINDOW` | `0x02000000` | V36+: window was moved/sized/zoomed/depth-arranged | `CWCODE_MOVESIZE` or `CWCODE_DEPTH` (V39) | — | — | — |
| `IDCMP_GADGETHELP` | `0x04000000` | V39+: help key while over a gadget with `GA_GadgetHelp,TRUE` | 0, `~0`, or value set by `GMR_HELPCODE`-returning gadget | — | `Gadget *` | current |
| `IDCMP_LONELYMESSAGE` | `0x80000000` | (internal bit in Class; never enable) | — | — | — | — |

### Notes and subtleties

**`IDCMP_MOUSEMOVE`** is delivered either when `WFLG_REPORTMOUSE` is set (unconditional while window is active, while mouse is inside), or when `GACT_FOLLOWMOUSE` is set on an active gadget (only while that gadget is active). With `IDCMP_DELTAMOVE` also enabled, `MouseX`/`MouseY` become deltas since the last reported position — useful for implementing mouse-look. Without `IDCMP_DELTAMOVE`, they are window-relative absolute coordinates. The `DEFAULTMOUSEQUEUE` (5) is the default maximum mouse-move backlog before Intuition starts dropping them; change with `SetMouseQueue()` or `WA_MouseQueue`.

**`IDCMP_VANILLAKEY`** is a "cooked" ASCII stream — Intuition has already applied the current keymap and dead-key / modifier handling. You will never see cursor keys, function keys, or Help in VANILLAKEY; those require RAWKEY. For RAWKEY events Intuition passes the raw matrix code in the low 7 bits of `Code`; bit 7 (`0x80`, `IECODE_UP_PREFIX`) indicates key-up.

**`IDCMP_MENUVERIFY`** is a "veto" class. Your window is allowed to block the menu from appearing by returning `MENUCANCEL` in `Code` before you `ReplyMsg()`. This is how, e.g., a full-screen game window declines to freeze for a menu it doesn't want. Intuition times out after ~2 seconds regardless. Similar applies to `IDCMP_SIZEVERIFY` and `IDCMP_REQVERIFY`.

**`IDCMP_REFRESHWINDOW`**: only simple-refresh windows need to care. The canonical handler is:

```c
case IDCMP_REFRESHWINDOW:
    BeginRefresh(window);
    /* redraw whatever; damage region is clipped automatically */
    EndRefresh(window, TRUE);
    break;
```

Never call `BeginRefresh()` unless you actually received `IDCMP_REFRESHWINDOW` — it has side effects on the layer damage list.

**`IDCMP_IDCMPUPDATE`** is the BOOPSI notification pipe. When a BOOPSI object whose target is `ICTARGET_IDCMP` calls `OM_NOTIFY`, Intuition packages the attribute-value tag list according to the `ICA_MAP` and delivers it as an `IDCMPUPDATE` message whose `IAddress` points at a `struct TagItem *`. The magic tag `ICSPECIAL_CODE` in the map lets the object route a UWORD into the message's `Code` field. See §6.

**`IDCMP_CHANGEWINDOW`** (V36+) tells you that a window just moved, sized, or (V39+) got depth-arranged. `Code` is `CWCODE_MOVESIZE` or `CWCODE_DEPTH`. Programs that need to track their own size typically use this instead of `IDCMP_NEWSIZE`, since it fires after the move/size is complete, not merely on resize.

**`IDCMP_GADGETHELP`** (V39+) is how custom help works. Set `HC_GADGETHELP` via `HelpControl(window, HC_GADGETHELP)` to enable it, and set `GA_GadgetHelp,TRUE` on each BOOPSI gadget you want to be help-aware. When the user presses Help over such a gadget, the gadget's `GM_HELPTEST` method is called; if it returns `GMR_HELPHIT` (or `GMR_HELPCODE | code`), you get `IDCMP_GADGETHELP` with the gadget pointer in `IAddress`.

### The message loop idiom

```c
ULONG signal = 1UL << window->UserPort->mp_SigBit;
for (;;) {
    Wait(signal);           /* or Wait(signal | other_signals) */
    struct IntuiMessage *imsg;
    while ((imsg = (struct IntuiMessage *)GetMsg(window->UserPort))) {
        ULONG class = imsg->Class;
        UWORD code  = imsg->Code;
        APTR  iaddr = imsg->IAddress;
        ReplyMsg((struct Message *)imsg);
        switch (class) {
            case IDCMP_CLOSEWINDOW:  done = TRUE; break;
            case IDCMP_NEWSIZE:      redraw_all(); break;
            ...
        }
    }
}
```

Copy fields out **before** `ReplyMsg()`. After the reply, Intuition is free to reuse the structure.

---

<a name="window-screen-tags"></a>
## 5. Window and Screen open tag essentials

V36+ added the tag-based open functions. Tags are pairs of `(ULONG tag, ULONG data)` terminated by `TAG_DONE`. Each Intuition-recognised tag has a `WA_*` or `SA_*` identifier. Unknown tags are silently skipped (so V37 apps can pass V39-only tags harmlessly on V37, though the feature is absent).

### 5.1 `OpenWindowTagList` / `OpenWindowTags`

```c
struct Window *OpenWindowTagList(struct NewWindow *newwin, struct TagItem *tags);
struct Window *OpenWindowTags(struct NewWindow *newwin, Tag tag1, ...);
```

`newwin` may be `NULL` — in that case all state comes from tags. If both are supplied, tags override matching fields of `newwin`.

**The complete WA_* list** (`intuition.h` line 1205 onward). `WA_Dummy = TAG_USER + 99 = 0x80000063`.

| Tag | Offset | Data | Since | Meaning |
|---|---|---|---|---|
| `WA_Left` | +0x01 | WORD | V36 | Window left edge (screen-relative) |
| `WA_Top` | +0x02 | WORD | V36 | Top edge |
| `WA_Width` | +0x03 | WORD | V36 | Width |
| `WA_Height` | +0x04 | WORD | V36 | Height |
| `WA_DetailPen` | +0x05 | UBYTE | V36 | Detail pen |
| `WA_BlockPen` | +0x06 | UBYTE | V36 | Block pen |
| `WA_IDCMP` | +0x07 | ULONG | V36 | Initial IDCMP class mask; non-zero also creates UserPort |
| `WA_Flags` | +0x08 | ULONG | V36 | Bulk Flags initializer |
| `WA_Gadgets` | +0x09 | `Gadget *` | V36 | Initial gadget list |
| `WA_Checkmark` | +0x0A | `Image *` | V36 | Custom checkmark |
| `WA_Title` | +0x0B | `UBYTE *` | V36 | Window title (replaces `SetWindowTitles` call) |
| `WA_ScreenTitle` | +0x0C | `UBYTE *` | V36 | Screen title while this window is active |
| `WA_CustomScreen` | +0x0D | `Screen *` | V36 | Open in this screen |
| `WA_SuperBitMap` | +0x0E | `BitMap *` | V36 | Super-bitmap BitMap; implies `WFLG_SUPER_BITMAP` |
| `WA_MinWidth` | +0x0F | WORD | V36 | Minimum width for sizing |
| `WA_MinHeight` | +0x10 | WORD | V36 | Minimum height |
| `WA_MaxWidth` | +0x11 | WORD | V36 | Maximum width (0xFFFF = unbounded) |
| `WA_MaxHeight` | +0x12 | WORD | V36 | Maximum height |
| `WA_InnerWidth` | +0x13 | WORD | V36 | Width **inside** borders; Intuition computes outer |
| `WA_InnerHeight` | +0x14 | WORD | V36 | Inner height |
| `WA_PubScreenName` | +0x15 | `UBYTE *` | V36 | Visitor on named public screen |
| `WA_PubScreen` | +0x16 | `Screen *` | V36 | Visitor on this screen pointer |
| `WA_PubScreenFallBack` | +0x17 | BOOL | V36 | Fall back to default pub screen if named missing |
| `WA_WindowName` | +0x18 | — | V36 | Not implemented |
| `WA_Colors` | +0x19 | `ColorSpec *` | V36 | Not implemented |
| `WA_Zoom` | +0x1A | `WORD[4]` | V36 | Zoom L/T/W/H array; implies zoom gadget |
| `WA_MouseQueue` | +0x1B | LONG | V36 | Max backlog of mouse-move messages |
| `WA_BackFill` | +0x1C | `Hook *` | V36 | Backfill hook for layer |
| `WA_RptQueue` | +0x1D | LONG | V36 | Key-repeat backlog limit |
| `WA_SizeGadget` | +0x1E | BOOL | V36 | Alternative to `WFLG_SIZEGADGET` |
| `WA_DragBar` | +0x1F | BOOL | V36 | Alternative to `WFLG_DRAGBAR` |
| `WA_DepthGadget` | +0x20 | BOOL | V36 | |
| `WA_CloseGadget` | +0x21 | BOOL | V36 | |
| `WA_Backdrop` | +0x22 | BOOL | V36 | |
| `WA_ReportMouse` | +0x23 | BOOL | V36 | |
| `WA_NoCareRefresh` | +0x24 | BOOL | V36 | |
| `WA_Borderless` | +0x25 | BOOL | V36 | |
| `WA_Activate` | +0x26 | BOOL | V36 | |
| `WA_RMBTrap` | +0x27 | BOOL | V36 | |
| `WA_WBenchWindow` | +0x28 | — | V36 | **PRIVATE** |
| `WA_SimpleRefresh` | +0x29 | BOOL | V36 | Simple refresh mode (set only TRUE) |
| `WA_SmartRefresh` | +0x2A | BOOL | V36 | Smart refresh (set only TRUE) |
| `WA_SizeBRight` | +0x2B | BOOL | V36 | Size gadget in right border |
| `WA_SizeBBottom` | +0x2C | BOOL | V36 | Size gadget in bottom border |
| `WA_AutoAdjust` | +0x2D | BOOL | V36 | Shift/squeeze to fit on screen |
| `WA_GimmeZeroZero` | +0x2E | BOOL | V36 | GIMMEZEROZERO mode |
| `WA_MenuHelp` | +0x2F | BOOL | V37 | Enables `IDCMP_MENUHELP` |
| `WA_NewLookMenus` | +0x30 | BOOL | V39 | New-look menus |
| `WA_AmigaKey` | +0x31 | `Image *` | V39 | Custom Amiga-key image in menus |
| `WA_NotifyDepth` | +0x32 | BOOL | V39 | Fire `IDCMP_CHANGEWINDOW` with `CWCODE_DEPTH` on depth arrange |
| `WA_Pointer` | +0x34 | `Object *` | V39 | Custom pointer (from `pointerclass`); NULL restores default |
| `WA_BusyPointer` | +0x35 | BOOL | V39 | Show standard busy pointer |
| `WA_PointerDelay` | +0x36 | BOOL | V39 | Delay pointer change (to avoid flashing) |
| `WA_TabletMessages` | +0x37 | BOOL | V39 | Include tablet data in ExtIntuiMessages |
| `WA_HelpGroup` | +0x38 | ULONG | V39 | Group ID for gadget help across multi-window apps |
| `WA_HelpGroupWindow` | +0x39 | `Window *` | V39 | Alternative: join another window's group |

`WA_AutoAdjust,TRUE` combined with `WA_InnerWidth`/`WA_InnerHeight` is the idiomatic V36+ way of saying "I need this much working room, please fit my window on screen however you have to". Intuition will shift the left/top or (last resort) shrink the window to make it fit the current display clip.

Failure: returns NULL. There is no equivalent of `SA_ErrorCode` for windows.

### 5.2 `OpenScreenTagList` / `OpenScreenTags`

```c
struct Screen *OpenScreenTagList(struct NewScreen *ns, struct TagItem *tags);
struct Screen *OpenScreenTags(struct NewScreen *ns, Tag tag1, ...);
```

As with windows, `ns` may be `NULL`. `SA_Dummy = TAG_USER + 32 = 0x80000020`.

| Tag | Offset | Data | Since | Meaning |
|---|---|---|---|---|
| `SA_Left`/`SA_Top`/`SA_Width`/`SA_Height` | +1..+4 | WORD | V36 | Screen position / size (defaults to display clip) |
| `SA_Depth` | +5 | UWORD | V36 | Bitplane count (default 1) |
| `SA_DetailPen` / `SA_BlockPen` | +6..+7 | UBYTE | V36 | Default title pens (defaults 0/1) |
| `SA_Title` | +8 | `UBYTE *` | V36 | Default title |
| `SA_Colors` | +9 | `ColorSpec *` | V36 | 4-bit-per-gun palette |
| `SA_ErrorCode` | +10 | `LONG *` | V36 | Pointer to error code out |
| `SA_Font` | +11 | `TextAttr *` | V36 | Default font (must be already loaded) |
| `SA_SysFont` | +12 | ULONG | V36 | 0 = old DefaultFont, 1 = WB preferred font |
| `SA_Type` | +13 | UWORD | V36 | `CUSTOMSCREEN` or `PUBLICSCREEN` |
| `SA_BitMap` | +14 | `BitMap *` | V36 | Custom bitmap; implies `CUSTOMBITMAP` |
| `SA_PubName` | +15 | `UBYTE *` | V36 | Public screen name (implies public; precede SA_PubSig/Task) |
| `SA_PubSig` | +16 | UBYTE | V36 | Signal bit to post when last visitor leaves |
| `SA_PubTask` | +17 | `Task *` | V36 | Task to signal (defaults to caller) |
| `SA_DisplayID` | +18 | ULONG | V36 | 32-bit mode ID from `graphics/modeid.h` |
| `SA_DClip` | +19 | `Rectangle *` | V36 | Explicit DisplayClip |
| `SA_Overscan` | +20 | ULONG | V36 | `OSCAN_TEXT`/`STANDARD`/`MAX`/`VIDEO` |
| `SA_ShowTitle` | +22 | BOOL | V36 | Equivalent to `SHOWTITLE` flag (default TRUE) |
| `SA_Behind` | +23 | BOOL | V36 | Open behind (equivalent to `SCREENBEHIND`) |
| `SA_Quiet` | +24 | BOOL | V36 | `SCREENQUIET` |
| `SA_AutoScroll` | +25 | BOOL | V36 | Autoscroll on edge hit |
| `SA_Pens` | +26 | `UWORD *` | V36 | `~0`-terminated pen array; enables new look |
| `SA_FullPalette` | +27 | BOOL | V36 | Initialize all 32 registers from prefs, not just 0-3+17-19 |
| `SA_ColorMapEntries` | +28 | ULONG | V39 | Override default colormap size |
| `SA_Parent` | +29 | `Screen *` | V39 | Attach as child to another screen (family) |
| `SA_Draggable` | +30 | BOOL | V39 | Default TRUE; FALSE to lock position |
| `SA_Exclusive` | +31 | BOOL | V39 | Don't share display |
| `SA_SharePens` | +32 | BOOL | V39 | DrawInfo pens obtained shared (see `ObtainPen`) |
| `SA_BackFill` | +33 | `Hook *` | V39 | Layer_Info backfill hook |
| `SA_Interleaved` | +34 | BOOL | V39 | Request interleaved bitmap allocation |
| `SA_Colors32` | +35 | `ULONG *` | V39 | `LoadRGB32()`-style 32-bit-per-gun palette |
| `SA_VideoControl` | +36 | `TagItem *` | V39 | Tag list for `VideoControl()` at open time |
| `SA_FrontChild` | +37 | `Screen *` | V39 | Already-open screen becomes front child |
| `SA_BackChild` | +38 | `Screen *` | V39 | Becomes back child |
| `SA_LikeWorkbench` | +39 | ULONG | V39 | Set to 1 to clone WB's mode/depth/size/colors |
| `SA_MinimizeISG` | +41 | BOOL | V40 | Minimize inter-screen gap |

**`SA_ErrorCode` error values** (from `screens.h`):

| Code | Constant | Meaning |
|---|---|---|
| 1 | `OSERR_NOMONITOR` | Named monitor spec not available |
| 2 | `OSERR_NOCHIPS` | Needs newer custom chips |
| 3 | `OSERR_NOMEM` | Out of normal memory |
| 4 | `OSERR_NOCHIPMEM` | Out of chip memory |
| 5 | `OSERR_PUBNOTUNIQUE` | Public screen name already used |
| 6 | `OSERR_UNKNOWNMODE` | Unknown mode requested |
| 7 | `OSERR_TOODEEP` | Screen too deep for hardware (V39) |
| 8 | `OSERR_ATTACHFAIL` | Illegal attach (V39) |
| 9 | `OSERR_NOTAVAILABLE` | Mode not available for other reason |

### 5.3 Worked example — a public screen with a visitor window

```c
struct Screen *s;
LONG err;

s = OpenScreenTags(NULL,
    SA_DisplayID,  HIRESLACE_KEY,
    SA_Depth,      3,
    SA_Title,      (ULONG)"ExampleScreen",
    SA_PubName,    (ULONG)"EXAMPLE",
    SA_ShowTitle,  TRUE,
    SA_Pens,       (ULONG)(UWORD[]){~0},     /* minimal - enable new look */
    SA_ErrorCode,  (ULONG)&err,
    TAG_DONE);
if (!s) { printf("OpenScreen err %ld\n", err); return; }
PubScreenStatus(s, 0);  /* publish */

struct Window *w = OpenWindowTags(NULL,
    WA_PubScreenName, (ULONG)"EXAMPLE",
    WA_Title,         (ULONG)"Visitor",
    WA_InnerWidth,    320,
    WA_InnerHeight,   100,
    WA_AutoAdjust,    TRUE,
    WA_Activate,      TRUE,
    WA_CloseGadget,   TRUE,
    WA_DragBar,       TRUE,
    WA_DepthGadget,   TRUE,
    WA_IDCMP,         IDCMP_CLOSEWINDOW | IDCMP_VANILLAKEY,
    TAG_DONE);
```

---

<a name="boopsi"></a>
## 6. BOOPSI — the Basic Object-Oriented Programming System for Intuition

BOOPSI appeared in V36 (OS 2.0) as the object-orientation infrastructure underneath Intuition. It gives you:

- A class hierarchy rooted at **rootclass**.
- **Classes** that carry a name (`ClassID`), a superclass pointer, a dispatcher `Hook`, instance-data layout, and counters.
- **Objects** (typed as `Object *` from the user's view, actually pointers into an allocation whose negative offset contains a `struct _Object`).
- A uniform **method** invocation protocol — `DoMethodA(object, msg)` — driven by a first-field `MethodID` in every message struct.
- **Attributes** — tag-item pairs used to initialize objects (at `OM_NEW`), set them later (`OM_SET`), and query them (`OM_GET`).
- **Notification** — objects can be chained so a change to one triggers an `OM_UPDATE` message on others, or an `IDCMP_IDCMPUPDATE` IntuiMessage on a window.

The name, per the autodoc for `NewObject`, is "basic object-oriented programming system for Intuition". BOOPSI is **not** a full Smalltalk-style runtime — there is no dynamic dispatch table; dispatch is through a single function hook per class, and each class implements a switch on `MethodID`. Single-inheritance only. Object state is stored as a `struct IClass *` (in the `_Object` preceding the user handle) plus a contiguous chunk of instance data per class in the hierarchy.

### 6.1 `rootclass`, `imageclass`, `icclass`, `gadgetclass`

The built-in class hierarchy looks like this (from the NDK class docs and `classusr.h`, line 40):

```
rootclass                        "rootclass"       classusr.h
  +-- imageclass                 "imageclass"      imageclass.h
  |     +-- frameiclass          "frameiclass"
  |     +-- sysiclass            "sysiclass"        (system gadget imagery)
  |     +-- fillrectclass        "fillrectclass"
  |     +-- itexticlass          "itexticlass"
  |     +-- bitmap.image         "bitmapiclass"    images/bitmap.h  (V39)
  |     +-- glyph.image          "glyphiclass"     images/glyph.h    (V44)
  |     +-- label.image          "labeliclass"     images/label.h    (V44)
  |     +-- drawlist.image       "drawlistclass"                      (V44)
  |     +-- penmap.image         "penmapiclass"                       (V44)
  |     +-- bevel.image          "beveliclass"                        (V44)
  +-- icclass                    "icclass"         icclass.h
  |     +-- modelclass           "modelclass"
  +-- pointerclass               "pointerclass"    pointerclass.h (V39)
  +-- gadgetclass                "gadgetclass"     gadgetclass.h
        +-- propgclass           "propgclass"      (prop gadget)
        +-- strgclass            "strgclass"       (string gadget)
        +-- buttongclass         "buttongclass"    (button gadget)
        +-- frbuttonclass        "frbuttonclass"   (frame button)
        +-- groupgclass          "groupgclass"     (group container)
        +-- (ReAction kit, V39+):
              +-- button.gadget  "buttongclass"
              +-- string.gadget  "stringclass"
              +-- integer.gadget "integerclass"
              +-- checkbox.gadget "checkboxgclass"
              +-- chooser.gadget "choosergclass"
              +-- listbrowser.gadget "listbrowsergclass"
              +-- layout.gadget  "layoutgclass"
              +-- clicktab.gadget "clicktabgclass"
              +-- scroller.gadget "scrollergclass"
              +-- slider.gadget  "slidergclass"
              +-- radiobutton.gadget "radiobutton.gadget"
              +-- palette.gadget "palette.gadget"
              +-- fuelgauge.gadget "fuelgauge.gadget"
              +-- gradientslider.gadget "gradientslider.gadget"
              +-- colorwheel.gadget "colorwheel.gadget"
              +-- getfile.gadget / getfont.gadget / getscreenmode.gadget
              +-- datebrowser.gadget
              +-- space.gadget
              +-- speedbar.gadget
              +-- texteditor.gadget
              +-- virtual.gadget
              +-- page.gadget
        +-- window.class         "windowclass"    classes/window.h (V44, ReAction)
        +-- requester.class      "requesterclass" classes/requester.h (V44, ReAction)
```

The class ID strings live in `intuition/classusr.h`:

```c
#define ROOTCLASS    "rootclass"
#define IMAGECLASS   "imageclass"
#define FRAMEICLASS  "frameiclass"
#define SYSICLASS    "sysiclass"
#define FILLRECTCLASS "fillrectclass"
#define GADGETCLASS  "gadgetclass"
#define PROPGCLASS   "propgclass"
#define STRGCLASS    "strgclass"
#define BUTTONGCLASS "buttongclass"
#define FRBUTTONCLASS "frbuttonclass"
#define GROUPGCLASS  "groupgclass"
#define ICCLASS      "icclass"
#define MODELCLASS   "modelclass"
#define ITEXTICLASS  "itexticlass"
#define POINTERCLASS "pointerclass"
```

### 6.2 Methods and the universal `OM_*` protocol

Every BOOPSI method takes a message whose first field is `ULONG MethodID`. The dispatcher hook sees `(Class, Object, Msg)` and switches on `msg->MethodID`.

The rootclass-defined methods (`classusr.h` line 63) are:

| ID | Constant | Msg struct | Purpose |
|---|---|---|---|
| 0x101 | `OM_NEW` | `struct opSet` | Create. `ops_AttrList` is the tag list, `ops_GInfo` is NULL (not yet parented to a window). |
| 0x102 | `OM_DISPOSE` | `Msg` | Delete self. Base rootclass frees instance data and removes self from any internal lists. |
| 0x103 | `OM_SET` | `struct opSet` | Set attributes from `ops_AttrList`. For gadgets, `ops_GInfo` tells the object what window it lives in. Return value is non-zero if anything that requires a re-render has changed. |
| 0x104 | `OM_GET` | `struct opGet` | Query single attribute `opg_AttrID`; store answer in `*opg_Storage`. Return 1 if understood, 0 otherwise. |
| 0x105 | `OM_ADDTAIL` | `struct opAddTail` | Add self to `opat_List`. |
| 0x106 | `OM_REMOVE` | `Msg` | Remove self from its list (rootclass version). |
| 0x107 | `OM_NOTIFY` | `struct opUpdate` | "Something changed about me; tell my dependents." Classes call this on themselves when their state changes. The rootclass traverses the dependents (set via `ICA_TARGET`) and `DoMethod`s an `OM_UPDATE` on each. |
| 0x108 | `OM_UPDATE` | `struct opUpdate` | "Somebody else changed; here are the new attrs." `opu_Flags` may have `OPUF_INTERIM` to indicate intermediate (e.g. slider drag) vs final. |
| 0x109 | `OM_ADDMEMBER` | `struct opMember` | For classes that hold sub-object lists (modelclass, groupgclass). |
| 0x10A | `OM_REMMEMBER` | `struct opMember` | Inverse. |

Parameter message structures (from `classusr.h`):

```c
struct opSet {
    ULONG           MethodID;
    struct TagItem *ops_AttrList;
    struct GadgetInfo *ops_GInfo;
};

struct opUpdate {
    ULONG           MethodID;
    struct TagItem *opu_AttrList;
    struct GadgetInfo *opu_GInfo;
    ULONG           opu_Flags;      /* OPUF_INTERIM */
};

struct opGet {
    ULONG           MethodID;
    ULONG           opg_AttrID;
    ULONG          *opg_Storage;
};

struct opMember {
    ULONG           MethodID;
    Object         *opam_Object;
};
```

The **gadgetclass** adds `GM_HITTEST`, `GM_RENDER`, `GM_GOACTIVE`, `GM_HANDLEINPUT`, `GM_GOINACTIVE`, `GM_HELPTEST`, `GM_LAYOUT`, `GM_DOMAIN`, `GM_KEYTEST`, `GM_KEYGOACTIVE`, `GM_KEYGOINACTIVE` (see `gadgetclass.h` line 295 onward). A custom gadget implements these as needed and `DoSuperMethodA()`s up the chain for the ones it doesn't.

The **imageclass** adds `IM_DRAW`, `IM_HITTEST`, `IM_ERASE`, `IM_MOVE`, `IM_DRAWFRAME`, `IM_FRAMEBOX`, `IM_HITFRAME`, `IM_ERASEFRAME`, `IM_DOMAINFRAME` (V44). See `imageclass.h` line 183.

### 6.3 Application-level BOOPSI API

These are the normal application entry points. Do not call `OM_NEW`/`OM_SET` via `DoMethod()` — use the wrappers, which Intuition may intercept.

#### `NewObject(class, classID, taglist)`

```c
APTR NewObject(struct IClass *class, UBYTE *classID, ULONG tag1, ...);
APTR NewObjectA(struct IClass *class, UBYTE *classID, struct TagItem *tags);
```

*(from `intuition.doc`/NewObject)*

> This is the general method of creating objects from 'boopsi' classes.
>
> You specify a class either as a pointer (for a private class) or by its ID string (for public classes). If the class pointer is NULL, then the classID is used.
>
> You further specify initial "create-time" attributes for the object via a TagItem list, and they are applied to the resulting generic data object that is returned. The attributes, their meanings, attributes applied only at create-time, and required attributes are all defined and documented on a class-by-class basis.

Returns `NULL` on failure. The object is later freed with `DisposeObject()`. `NewObject()` invokes the class's `OM_NEW` method.

#### `DisposeObject(object)`

```c
VOID DisposeObject(APTR object);
```

Invokes `OM_DISPOSE`. Do not use on gadgets still attached to a window — remove them with `RemoveGadget`/`RemoveGList` first.

#### `SetAttrsA(object, taglist)` / `SetAttrs(object, ...)`

```c
ULONG SetAttrs(APTR object, ULONG tag1, ...);
ULONG SetAttrsA(APTR object, struct TagItem *tags);
```

Sends `OM_SET` with `ops_GInfo == NULL`. Use this for objects that are not gadgets attached to a window. Returns whatever the class's `OM_SET` returns — usually non-zero if visible state changed.

#### `SetGadgetAttrsA(gadget, window, requester, taglist)`

```c
ULONG SetGadgetAttrsA(struct Gadget *g, struct Window *w,
                      struct Requester *r, struct TagItem *tags);
ULONG SetGadgetAttrs(struct Gadget *g, struct Window *w,
                     struct Requester *r, ULONG tag1, ...);
```

Sends `OM_SET` with a fully populated `GadgetInfo`, and then, if the return value is non-zero, calls `GM_RENDER` to redraw the gadget in place. Use this for gadgets live in a window.

#### `GetAttr(attrID, object, storage)`

```c
ULONG GetAttr(ULONG attrID, APTR object, ULONG *storagePtr);
```

Sends `OM_GET` to the object. Return 1 if recognized, 0 if not.

#### `DoMethodA(object, msg)` / `DoMethod(object, methodID, ...)`

Not actually in intuition.library — these are amiga.lib helpers:

```c
ULONG DoMethod(Object *o, ULONG methodID, ...);
ULONG DoMethodA(Object *o, Msg msg);
ULONG DoSuperMethodA(struct IClass *cl, Object *o, Msg msg);
ULONG CoerceMethodA(struct IClass *cl, Object *o, Msg msg);
```

They simply look up the class's dispatcher (`OCLASS(o)->cl_Dispatcher`) and call it with the object and message. `DoSuperMethodA` calls the same method on the superclass; classes use it at the tail of their dispatcher's case branches for methods they don't fully handle. `CoerceMethodA` calls a specific class's implementation on any object (used by class implementors, not applications).

#### `DoGadgetMethodA(g, w, r, msg)`

V37+ addition for invoking methods on gadgets with a properly filled `ops_GInfo`:

```c
ULONG DoGadgetMethodA(struct Gadget *g, struct Window *w,
                      struct Requester *r, Msg msg);
```

### 6.4 Class creation (`MakeClass`, `AddClass`, `FreeClass`, `RemoveClass`)

*(from `intuition.doc`/MakeClass)*

```c
struct IClass *MakeClass(UBYTE *ClassID,
                         UBYTE *SuperClassID,
                         struct IClass *SuperClassPtr,
                         UWORD InstanceSize,
                         ULONG Flags);
```

> For class implementors only.
>
> This function creates a new public or private boopsi class. The superclass should be defined to be another boopsi class: all classes are descendants of the class "rootclass".
>
> Superclasses can be public or private. You provide a name/ID for your class if it is to be a public class (but you must have registered your class name and your attribute ID's with Commodore before you do this!). For a public class, you would also call AddClass() to make it available after you have finished your initialization.
>
> Returns pointer to an IClass data structure for your class. You then initialize the Hook cl_Dispatcher for your class methods code. You can also set up special data shared by all objects in your class, and point cl_UserData at it. The last step for public classes is to call AddClass().

Arguments:
- `ClassID` — NULL for a private class, name/ID string for a public one.
- `SuperClassID` — name of a public superclass, or NULL to use `SuperClassPtr`.
- `SuperClassPtr` — pointer to a private superclass (e.g. another one you made).
- `InstanceSize` — bytes of **per-object instance data** your class needs **beyond** what your superclass stores.
- `Flags` — 0 for now.

Returns NULL on:
- out of memory for class data structure
- named public superclass not found
- a public class of the same name already exists

After `MakeClass()`, you must initialize the dispatcher hook before the class is usable:

```c
cl->cl_Dispatcher.h_Entry    = hookEntry;   /* asm-to-C stub */
cl->cl_Dispatcher.h_SubEntry = myDispatcher;
cl->cl_Dispatcher.h_Data     = user_data_ptr;
```

Then either keep it private or publish it:

```c
void AddClass(struct IClass *cl);      /* make public */
void RemoveClass(struct IClass *cl);   /* withdraw */
BOOL FreeClass(struct IClass *cl);     /* dispose */
```

`FreeClass()` checks `cl_ObjectCount` and `cl_SubclassCount` — you cannot free a class if there are outstanding objects or subclasses of it. Plan your exit path accordingly.

### 6.5 Writing a dispatcher

The dispatcher is called once per method invocation on any object in your class. The C convention (SAS C registerized):

```c
ULONG __saveds __asm
myDispatcher(register __a0 struct IClass *cl,
             register __a2 Object *o,
             register __a1 Msg msg)
{
    struct MyInstance *inst;
    ULONG retval;

    switch (msg->MethodID) {
    case OM_NEW:
        /* pass to super first so it allocates the object */
        o = (Object *)DoSuperMethodA(cl, o, (Msg)msg);
        if (!o) return 0;
        inst = INST_DATA(cl, o);
        /* initialize from ((struct opSet *)msg)->ops_AttrList */
        ...
        return (ULONG)o;

    case OM_SET: {
        struct opSet *ops = (struct opSet *)msg;
        struct TagItem *tags = ops->ops_AttrList;
        struct TagItem *ti;
        ULONG dirty = 0;
        inst = INST_DATA(cl, o);
        while ((ti = NextTagItem(&tags))) {
            switch (ti->ti_Tag) {
            case MY_AttrA: inst->a = ti->ti_Data; dirty = 1; break;
            ...
            }
        }
        /* chain to super for its attrs */
        retval = DoSuperMethodA(cl, o, (Msg)msg);
        return dirty || retval;
    }

    case OM_GET: {
        struct opGet *opg = (struct opGet *)msg;
        inst = INST_DATA(cl, o);
        switch (opg->opg_AttrID) {
        case MY_AttrA: *opg->opg_Storage = inst->a; return 1;
        ...
        }
        return DoSuperMethodA(cl, o, (Msg)msg);
    }

    case OM_DISPOSE:
        /* free your per-instance resources, then super disposes object */
        inst = INST_DATA(cl, o);
        /* free things */
        return DoSuperMethodA(cl, o, (Msg)msg);

    case GM_RENDER:
        /* draw yourself */
        ...
        return 0;

    default:
        /* unknown method -> super */
        return DoSuperMethodA(cl, o, (Msg)msg);
    }
}
```

The macros `INST_DATA(cl, o)` (from `classes.h`) and `OCLASS(o)` (get class pointer from object) do what you expect:

```c
#define INST_DATA(cl,o)     ((void *)(((UBYTE *)o) + cl->cl_InstOffset))
#define OCLASS(o)           ((_OBJECT(o))->o_Class)
```

### 6.6 Notification — `ICA_TARGET` / `ICA_MAP` / `OM_NOTIFY`

The reason BOOPSI exists for application code (as opposed to just custom gadgets) is that it gives you a clean way to **route events from one gadget to another object** without handling IDCMP in the middle.

Every object inheriting from **icclass** (interconnection class) understands two attributes:

```c
#define ICA_TARGET   (ICA_Dummy + 1)   /* where to send notifications */
#define ICA_MAP      (ICA_Dummy + 2)   /* attribute-remap tag list    */
```

When the source object's state changes, it calls `OM_NOTIFY` on itself. Rootclass's `OM_NOTIFY` implementation walks the dependents (just `ICA_TARGET` — there is only one target) and `DoMethod`s an `OM_UPDATE` on each. The target receives `opu_AttrList` containing the changed attributes, **remapped** through `ICA_MAP` if present.

`ICA_MAP` is a `TagItem *` list mapping source attribute IDs to target attribute IDs. So a prop gadget's `PGA_Top` can be remapped to `MY_CurrentValue` before it reaches your custom object.

There is a magic target: `ICA_TARGET, ICTARGET_IDCMP` (value `~0L`). This means "send the notification as an `IDCMP_IDCMPUPDATE` message to the window's IDCMP port". The `IntuiMessage.IAddress` then points at the mapped `TagItem *` list. The magic tag `ICSPECIAL_CODE` in the map, if set, routes the low 16 bits of its data into `IntuiMessage.Code`.

This is how GadTools gadgets end up delivering IDCMP messages — they are BOOPSI gadgets with `ICA_TARGET, ICTARGET_IDCMP` set internally.

### 6.7 Worked example — a slider wired to a number display (from RKM 3rd Libraries, BOOPSI chapter)

```c
/* Create a model object to hold a single value */
Object *model = NewObject(NULL, MODELCLASS,
    TAG_DONE);

/* Create a prop gadget (slider) that updates the model */
Object *prop = NewObject(NULL, "propgclass",
    GA_Top,     20,
    GA_Left,    20,
    GA_Width,   200,
    GA_Height,  16,
    PGA_Freedom, FREEHORIZ,
    PGA_Total,   100,
    PGA_Visible,  10,
    PGA_Top,       0,
    PGA_NewLook, TRUE,
    ICA_TARGET,  model,
    ICA_MAP,     (ULONG)prop_to_model_map,  /* PGA_Top -> MODEL_CurrentSlot */
    TAG_DONE);

/* And another gadget that *listens* to the model
 * and updates its displayed number */
Object *display = NewObject(NULL, "mynumberdisplayclass",
    GA_Top,    50,
    GA_Left,   20,
    GA_Width,  100,
    GA_Height, 14,
    TAG_DONE);
AddAttr(model, display);  /* application-level helper */

/* Or, easier still: route the model's "current value changed"
 * back out as IDCMP_IDCMPUPDATE with Code = new value */
SetAttrs(model,
    ICA_TARGET, ICTARGET_IDCMP,
    ICA_MAP,    (ULONG)map_with_ICSPECIAL_CODE,
    TAG_DONE);
```

In the window event loop:

```c
case IDCMP_IDCMPUPDATE: {
    struct TagItem *tags = (struct TagItem *)imsg->IAddress;
    /* tags list contains whatever ICA_MAP produced;
     * Code contains the value routed through ICSPECIAL_CODE */
    handle_slider_change(imsg->Code, tags);
    break;
}
```

The advantage: the slider does not know about the number display; the number display does not know about the slider; they are wired together by tag-list plumbing. You can rewire without touching their code.

### 6.8 Practical BOOPSI rules

- `NewObject(NULL, "classname", ...)` for public classes; `NewObject(cl, NULL, ...)` for private classes you made.
- Always `DisposeObject()` what you `NewObject()`. Do not mix up with `RemoveGadget`/`RemoveGList` — those detach a gadget from a window; you still have to dispose of it after.
- When chaining to super with `DoSuperMethodA`, pass the **original `msg`** (which may be a subclass of the standard struct). You do not convert or unwrap it.
- `cl_UserData` is a per-class pointer; useful for cached state or shared resources.
- Instance data is zero-initialized by rootclass during `OM_NEW`. Do not rely on that for pointer fields you *must* initialize, but it does mean integer counts start at 0.
- Do not cache `INST_DATA(cl, o)` across method calls on different objects — it's per-object.

See the NDK Autodocs `*_cl.doc` and `*_gc.doc` files (window_cl.doc, button_gc.doc, listbrowser_gc.doc, etc.) for full attribute and method lists for each class. Example files in NDK 3.9: `button_gc.doc`, `checkbox_gc.doc`, `chooser_gc.doc`, `clicktab_gc.doc`, `colorwheel_gc.doc`, `datebrowser_gc.doc`, `fuelgauge_gc.doc`, `getfile_gc.doc`, `getfont_gc.doc`, `getscreenmode_gc.doc`, `gradientslider_gc.doc`, `integer_gc.doc`, `layout_gc.doc`, `listbrowser_gc.doc`, `page_gc.doc`, `palette_gc.doc`, `radiobutton_gc.doc`, `scroller_gc.doc`, `slider_gc.doc`, `space_gc.doc`, `speedbar_gc.doc`, `string_gc.doc`, `texteditor_gc.doc`, `virtual_gc.doc`, `window_cl.doc`, `arexx_cl.doc`, `bevel_ic.doc`, `bitmap_ic.doc`, `drawlist_ic.doc`, `glyph_ic.doc`, `label_ic.doc`, `penmap_ic.doc`.

---

<a name="gadtools"></a>
## 7. GadTools V36+

`gadtools.library` (V36, version 37+) is the "turn-key" GUI toolkit layered on BOOPSI. It provides:

- A `NewGadget` tag-configurable gadget description, one per gadget.
- `CreateContext()` / `CreateGadgetA()` to build a linked gadget chain from successive `NewGadget`s and per-kind tag lists.
- Fourteen **gadget kinds**: BUTTON, CHECKBOX, INTEGER, LISTVIEW, MX (mutual exclude / radio buttons), NUMBER (display), CYCLE, PALETTE, SCROLLER, SLIDER, STRING, TEXT (display), GENERIC.
- A menu builder: `CreateMenusA()` from a `NewMenu []` array, then `LayoutMenusA()` to size it for a given visual.
- Message filtering wrappers (`GT_GetIMsg`, `GT_ReplyIMsg`, `GT_FilterIMsg`, `GT_PostFilterIMsg`) that handle scroller/slider "live" updates and keep gadget state coherent without the application having to micromanage.
- `DrawBevelBoxA()` for the 3D-look borders you see around groups of controls.
- A `VisualInfo` struct that holds the screen's drawing context (pens, font) so all GadTools gadgets on that screen share it.

The library is meant to be used together with intuition, not instead of it. You still `OpenWindowTags` the window, but you attach GadTools gadgets instead of hand-rolled `struct Gadget`s.

### 7.1 `struct NewGadget` (gadtools.h line 83)

```c
struct NewGadget
{
    WORD  ng_LeftEdge, ng_TopEdge;
    WORD  ng_Width, ng_Height;
    UBYTE *ng_GadgetText;            /* label */
    struct TextAttr *ng_TextAttr;    /* font for label */
    UWORD ng_GadgetID;               /* your ID */
    ULONG ng_Flags;                  /* PLACETEXT_* | NG_HIGHLABEL */
    APTR  ng_VisualInfo;             /* from GetVisualInfo() */
    APTR  ng_UserData;
};
```

`ng_Flags` controls label placement:

- `PLACETEXT_LEFT` — right-justified, left of gadget
- `PLACETEXT_RIGHT` — left-justified, right of gadget
- `PLACETEXT_ABOVE` — centered above
- `PLACETEXT_BELOW` — centered below
- `PLACETEXT_IN` — centered inside (for buttons)
- `NG_HIGHLABEL` — draw the label highlighted

Each kind has a default; button defaults to `PLACETEXT_IN`, checkbox to `PLACETEXT_RIGHT`, most others to `PLACETEXT_LEFT`.

### 7.2 Kind constants

```c
#define GENERIC_KIND    0
#define BUTTON_KIND     1
#define CHECKBOX_KIND   2
#define INTEGER_KIND    3
#define LISTVIEW_KIND   4
#define MX_KIND         5
#define NUMBER_KIND     6
#define CYCLE_KIND      7
#define PALETTE_KIND    8
#define SCROLLER_KIND   9
#define SLIDER_KIND     11        /* 10 is reserved */
#define STRING_KIND     12
#define TEXT_KIND       13
#define NUM_KINDS       14
```

### 7.3 Per-kind IDCMP masks

For each kind there is a macro telling you which IDCMP flags it needs. `OR` together those of the gadgets your window uses, plus your own classes:

```c
#define BUTTONIDCMP     (IDCMP_GADGETUP)
#define CHECKBOXIDCMP   (IDCMP_GADGETUP)
#define INTEGERIDCMP    (IDCMP_GADGETUP)
#define LISTVIEWIDCMP   (IDCMP_GADGETUP | IDCMP_GADGETDOWN | \
                         IDCMP_MOUSEMOVE | ARROWIDCMP)
#define MXIDCMP         (IDCMP_GADGETDOWN)
#define NUMBERIDCMP     (0L)
#define CYCLEIDCMP      (IDCMP_GADGETUP)
#define PALETTEIDCMP    (IDCMP_GADGETUP)
#define SCROLLERIDCMP   (IDCMP_GADGETUP | IDCMP_GADGETDOWN | IDCMP_MOUSEMOVE)
#define SLIDERIDCMP     (IDCMP_GADGETUP | IDCMP_GADGETDOWN | IDCMP_MOUSEMOVE)
#define STRINGIDCMP     (IDCMP_GADGETUP)
#define TEXTIDCMP       (0L)
#define ARROWIDCMP      (IDCMP_GADGETUP | IDCMP_GADGETDOWN | \
                         IDCMP_INTUITICKS | IDCMP_MOUSEBUTTONS)
```

### 7.4 Key functions

#### `GetVisualInfoA(screen, tags)` / `FreeVisualInfo(vi)`

```c
APTR GetVisualInfoA(struct Screen *screen, struct TagItem *tags);
VOID FreeVisualInfo(APTR vi);
```

Returns an opaque visual context for the given screen, used in `NewGadget.ng_VisualInfo` and as `GT_VisualInfo` in menu layouts. There are no currently defined tags; pass `NULL`. You **must** `FreeVisualInfo()` it before closing the screen.

#### `CreateContext(glistptr)`

```c
struct Gadget *CreateContext(struct Gadget **glistptr);
```

Initializes `*glistptr = NULL` and returns a "context" gadget. The context gadget is the head of the list that will be grown by successive `CreateGadgetA()` calls. The context is invisible and serves as the linked-list head.

Typical usage:

```c
struct Gadget *glist = NULL, *gad, *context;

context = CreateContext(&glist);

ng.ng_LeftEdge = 20; ng.ng_TopEdge = 20; ng.ng_Width = 80; ng.ng_Height = 14;
ng.ng_GadgetText = "OK"; ng.ng_GadgetID = 1; ng.ng_Flags = 0;
ng.ng_VisualInfo = vi; ng.ng_TextAttr = &topaz80;
gad = CreateGadget(BUTTON_KIND, context, &ng, TAG_DONE);

ng.ng_TopEdge = 40; ng.ng_GadgetText = "Cancel"; ng.ng_GadgetID = 2;
gad = CreateGadget(BUTTON_KIND, gad, &ng, TAG_DONE);
```

Each `CreateGadget()` returns the new gadget, which you pass as the **previous** gadget to the next call. If a call returns NULL, the chain is broken and you must abort — but `glist` still contains the partial chain, which you can free with `FreeGadgets()`.

Then attach the list to a window by including `WA_Gadgets, glist` in `OpenWindowTagList()`, or by calling `AddGList(window, glist, -1, -1, NULL)` after the window is open.

Post-open, call `GT_RefreshWindow(window, NULL)` to render them.

#### `CreateGadgetA(kind, prev, ng, tags)`

```c
struct Gadget *CreateGadgetA(ULONG kind,
                             struct Gadget *prevGadget,
                             struct NewGadget *ng,
                             struct TagItem *tagList);
```

Creates one GadTools gadget. The tag list is **kind-specific**. Here are the most common:

**BUTTON_KIND**
- (no kind-specific tags; use `ng_GadgetText` for the label)

**CHECKBOX_KIND**
- `GTCB_Checked` — initial state (BOOL)
- `GTCB_Scaled` (V39) — scale image to `ng_Width`/`ng_Height`

**INTEGER_KIND**
- `GTIN_Number` — initial value (LONG)
- `GTIN_MaxChars` — max digits
- `GTIN_EditHook` — custom edit hook

**STRING_KIND**
- `GTST_String` — initial text (STRPTR)
- `GTST_MaxChars` — buffer size
- `GTST_EditHook` — custom edit hook

**CYCLE_KIND**
- `GTCY_Labels` — NULL-terminated array of STRPTRs
- `GTCY_Active` — index of initial active label

**MX_KIND** (mutually-exclusive radio buttons)
- `GTMX_Labels` — NULL-terminated array of STRPTRs
- `GTMX_Active` — initial selection
- `GTMX_Spacing` — extra pixels between choices
- `GTMX_Scaled` (V39) — scale images
- `GTMX_TitlePlace` — where to put the title

**LISTVIEW_KIND**
- `GTLV_Labels` — `struct List *` (you own it; must stay valid)
- `GTLV_Top` — first visible entry
- `GTLV_Selected` — selected entry index
- `GTLV_ReadOnly` — BOOL
- `GTLV_ScrollWidth` — scrollbar width
- `GTLV_ShowSelected` — display selected entry below list (NULL = text display, or pointer to existing string gadget)
- `GTLV_MakeVisible` — ensure this item is on screen
- `GTLV_CallBack` — custom draw hook
- `GTLV_MaxPen` — max pen used in hook

**SCROLLER_KIND**
- `GTSC_Top` — first visible
- `GTSC_Total` — total items
- `GTSC_Visible` — visible count
- `GTSC_Arrows` — arrow button size (0 = no arrows)

**SLIDER_KIND**
- `GTSL_Min` / `GTSL_Max` / `GTSL_Level` — range and current
- `GTSL_MaxLevelLen`, `GTSL_LevelFormat`, `GTSL_LevelPlace` — printed value format
- `GTSL_DispFunc` — callback to compute displayed number from raw level
- `GTSL_MaxPixelLen` — max pixel width for level text
- `GTSL_Justification` — GTJ_LEFT/RIGHT/CENTER

**TEXT_KIND** (read-only display)
- `GTTX_Text` — initial text
- `GTTX_CopyText` — BOOL; if TRUE, copy the string rather than reference
- `GTTX_Border` — BOOL
- `GTTX_FrontPen`, `GTTX_BackPen`, `GTTX_Justification`
- `GTTX_Clipped` — clip overflowing text

**NUMBER_KIND** (read-only number display)
- `GTNM_Number` — value
- `GTNM_Border`, `GTNM_FrontPen`, `GTNM_BackPen`, `GTNM_Justification`
- `GTNM_Format` — printf format string
- `GTNM_MaxNumberLen` — max length
- `GTNM_Clipped`

**PALETTE_KIND**
- `GTPA_Depth` — bitplanes (number of colors = 2^depth)
- `GTPA_Color` — currently selected color
- `GTPA_ColorOffset` — base index into color map
- `GTPA_IndicatorWidth`, `GTPA_IndicatorHeight` — size of current-color display
- `GTPA_NumColors` (V39), `GTPA_ColorTable` (V39) — arbitrary subset of pens

#### Updating gadgets at runtime — `GT_SetGadgetAttrsA`

```c
VOID GT_SetGadgetAttrsA(struct Gadget *g, struct Window *w,
                        struct Requester *r, struct TagItem *tags);
```

Same signature as `SetGadgetAttrsA` but with filtered validation suitable for GadTools gadgets. Intuition will re-render as needed.

#### Refresh wrappers — `GT_BeginRefresh` / `GT_EndRefresh` / `GT_RefreshWindow`

```c
VOID GT_BeginRefresh(struct Window *w);
VOID GT_EndRefresh(struct Window *w, BOOL complete);
VOID GT_RefreshWindow(struct Window *w, struct Requester *r);
```

In a simple-refresh GadTools window, call `GT_BeginRefresh`/`GT_EndRefresh` instead of the plain intuition versions when handling `IDCMP_REFRESHWINDOW` — they also refresh GadTools gadget internals.

`GT_RefreshWindow()` re-renders the entire GadTools gadget list; use it after `AddGList` or after resizing.

#### Message filtering — `GT_GetIMsg` / `GT_ReplyIMsg` / `GT_FilterIMsg`

```c
struct IntuiMessage *GT_GetIMsg(struct MsgPort *port);
VOID GT_ReplyIMsg(struct IntuiMessage *imsg);
struct IntuiMessage *GT_FilterIMsg(struct IntuiMessage *imsg);
struct IntuiMessage *GT_PostFilterIMsg(struct IntuiMessage *imsg);
```

`GT_GetIMsg` is a replacement for `GetMsg()` that handles GadTools-internal plumbing. Use it **instead of** `GetMsg()` on the window's UserPort if you have GadTools gadgets. Pair with `GT_ReplyIMsg` instead of `ReplyMsg()`.

Internally GadTools may intercept some messages (e.g. scroll arrow clicks, slider drags) and turn them into "cooked" messages for you, or consume them entirely. You do not see the raw `ARROWIDCMP` events; you see clean `IDCMP_GADGETUP`/`IDCMP_MOUSEMOVE` messages with the scroller's updated `GTSC_Top` value.

The lower-level pair `GT_FilterIMsg`/`GT_PostFilterIMsg` is for apps that need to keep using `GetMsg()` — call `GT_FilterIMsg` on every message right after `GetMsg()`, dispatch based on its return value (which may be NULL if GadTools ate the message), and call `GT_PostFilterIMsg` before `ReplyMsg()`.

#### `DrawBevelBoxA(rport, left, top, width, height, tags)`

```c
VOID DrawBevelBoxA(struct RastPort *r, WORD l, WORD t, WORD w, WORD h,
                   struct TagItem *tags);
```

Draws a 3D-look bevel box. Tags:
- `GT_VisualInfo` — required
- `GTBB_Recessed` — BOOL; recessed (sunken) vs raised
- `GTBB_FrameType` (V39) — `BBFT_BUTTON`, `BBFT_RIDGE`, `BBFT_ICONDROPBOX`

### 7.5 Menu builder — `CreateMenusA` / `LayoutMenusA`

GadTools supplies a tidy replacement for the classic hand-rolled menu lists.

```c
struct NewMenu
{
    UBYTE  nm_Type;         /* NM_TITLE / NM_ITEM / NM_SUB / IM_ITEM / IM_SUB / NM_END / NM_IGNORE */
    STRPTR nm_Label;        /* text, or Image*, or NM_BARLABEL (-1) for separator */
    STRPTR nm_CommKey;      /* Amiga-key letter, or NM_COMMANDSTRING (V39) */
    UWORD  nm_Flags;        /* CHECKIT, MENUTOGGLE, CHECKED, NM_ITEMDISABLED, ... */
    LONG   nm_MutualExclude;
    APTR   nm_UserData;     /* stashed via GTMENU_USERDATA */
};
```

Array layout: one `NM_TITLE`, then one or more `NM_ITEM`s, optionally each followed by `NM_SUB`s, then the next `NM_TITLE`, then finally `NM_END`. Example from the RKM:

```c
struct NewMenu mymenu[] = {
    { NM_TITLE, "Project",      0, 0, 0, NULL },
        { NM_ITEM, "New",       "N", 0, 0, NULL },
        { NM_ITEM, "Open...",   "O", 0, 0, NULL },
        { NM_ITEM, NM_BARLABEL, 0, 0, 0, NULL },
        { NM_ITEM, "Save",      "S", 0, 0, NULL },
        { NM_ITEM, "Save As...", 0,  0, 0, NULL },
        { NM_ITEM, NM_BARLABEL, 0, 0, 0, NULL },
        { NM_ITEM, "Quit",      "Q", 0, 0, NULL },
    { NM_TITLE, "Edit",         0, 0, 0, NULL },
        { NM_ITEM, "Cut",       "X", 0, 0, NULL },
        { NM_ITEM, "Copy",      "C", 0, 0, NULL },
        { NM_ITEM, "Paste",     "V", 0, 0, NULL },
        { NM_ITEM, "Clear",      0,  0, 0, NULL },
    { NM_END, NULL, 0, 0, 0, NULL }
};
```

Build it:

```c
struct Menu *menustrip;
menustrip = CreateMenusA(mymenu, NULL);    /* tag list may be NULL */
if (!menustrip) fail();

/* Lay it out for a specific screen's visual */
if (!LayoutMenusA(menustrip, vi,
        GTMN_NewLookMenus, TRUE,
        TAG_DONE))
    fail();

/* Attach to window */
SetMenuStrip(window, menustrip);
```

`LayoutMenusA` tags include `GTMN_TextAttr` (font), `GTMN_FrontPen`, `GTMN_NewLookMenus` (V39), `GTMN_Checkmark`/`GTMN_AmigaKey` (V39), `GTMN_FullMenu` (V37 validation), `GTMN_SecondaryError` (error pointer).

Clean up on exit:

```c
ClearMenuStrip(window);
FreeMenus(menustrip);
```

User data per item is retrieved with:

```c
APTR ud = GTMENUITEM_USERDATA(item);
```

### 7.6 Worked example — minimal GadTools window

```c
struct Screen *pubscr;
APTR vi;
struct Gadget *glist = NULL, *gad, *ctx;
struct Window *win;
struct NewGadget ng;
LONG total = 100, visible = 10, top = 0;

pubscr = LockPubScreen(NULL);
vi = GetVisualInfoA(pubscr, NULL);

ctx = CreateContext(&glist);

ng.ng_LeftEdge = 20; ng.ng_TopEdge = 20;
ng.ng_Width    = 200; ng.ng_Height = 14;
ng.ng_GadgetText = "Name"; ng.ng_GadgetID = 1;
ng.ng_Flags = 0; ng.ng_VisualInfo = vi; ng.ng_TextAttr = NULL;
gad = CreateGadget(STRING_KIND, ctx, &ng,
    GTST_String,   "",
    GTST_MaxChars, 60,
    TAG_DONE);

ng.ng_TopEdge = 40;
ng.ng_GadgetText = "Volume"; ng.ng_GadgetID = 2;
gad = CreateGadget(SLIDER_KIND, gad, &ng,
    GTSL_Min,   0,
    GTSL_Max,   100,
    GTSL_Level, 50,
    GTSL_LevelFormat, "%3ld",
    GTSL_MaxLevelLen, 3,
    TAG_DONE);

ng.ng_TopEdge = 60; ng.ng_Width = 80; ng.ng_Height = 14;
ng.ng_GadgetText = "OK"; ng.ng_GadgetID = 3; ng.ng_Flags = 0;
gad = CreateGadget(BUTTON_KIND, gad, &ng, TAG_DONE);

win = OpenWindowTags(NULL,
    WA_PubScreen,   pubscr,
    WA_Title,       "GadTools demo",
    WA_InnerWidth,  240,
    WA_InnerHeight, 80,
    WA_DragBar,     TRUE,
    WA_DepthGadget, TRUE,
    WA_CloseGadget, TRUE,
    WA_Activate,    TRUE,
    WA_Gadgets,     glist,
    WA_IDCMP,       IDCMP_CLOSEWINDOW | BUTTONIDCMP | STRINGIDCMP | SLIDERIDCMP,
    TAG_DONE);

GT_RefreshWindow(win, NULL);

/* event loop */
BOOL done = FALSE;
while (!done) {
    ULONG sig = 1UL << win->UserPort->mp_SigBit;
    Wait(sig);
    struct IntuiMessage *imsg;
    while ((imsg = GT_GetIMsg(win->UserPort))) {
        ULONG class = imsg->Class;
        UWORD code  = imsg->Code;
        struct Gadget *g = (struct Gadget *)imsg->IAddress;
        GT_ReplyIMsg(imsg);
        switch (class) {
        case IDCMP_CLOSEWINDOW: done = TRUE; break;
        case IDCMP_GADGETUP:
            if (g->GadgetID == 3) done = TRUE;  /* OK */
            break;
        case IDCMP_MOUSEMOVE:   /* slider drag intermediate */
            break;
        }
    }
}

CloseWindow(win);
FreeGadgets(glist);
FreeVisualInfo(vi);
UnlockPubScreen(NULL, pubscr);
```

---

<a name="menus"></a>
## 8. Menus — classic and GadTools

The underlying intuition menu model hasn't changed since V1.0: a window has a `MenuStrip` pointing at a linked list of `Menu`s, each with a chain of `MenuItem`s, each optionally with `SubItem`s. GadTools builds on top of this; you can also build menus by hand, and many games and demos do.

### 8.1 Classic menu construction

Hand-rolled menus look like this (adapted from the 3rd-edition RKM Libraries):

```c
struct IntuiText ittext1 = { 0,1, JAM2, 3, 1, NULL, "New", NULL };
struct MenuItem item1 = {
    NULL,                 /* NextItem */
    0, 0, 80, 10,         /* Left/Top/Width/Height */
    ITEMTEXT | ITEMENABLED | HIGHCOMP | COMMSEQ,
    0,                    /* MutualExclude */
    (APTR)&ittext1,       /* ItemFill */
    NULL,                 /* SelectFill */
    'N',                  /* Command */
    NULL,                 /* SubItem */
    MENUNULL              /* NextSelect */
};

struct Menu menu1 = {
    NULL,                 /* NextMenu */
    0, 0, 60, 0,          /* position; Intuition fills in bar height */
    MENUENABLED,
    "Project",
    &item1                /* FirstItem */
};

SetMenuStrip(window, &menu1);
```

You are responsible for:
- Computing `Width`/`Height` of every item (GadTools does this for you via `LayoutMenusA`).
- Making sure the item rectangles don't overlap within a menu.
- Recomputing when the font or screen changes.

Intuition does the rest: drawing, hit-testing, keyboard-shortcut matching.

### 8.2 Menu API (intuition.library)

| Function | Purpose |
|---|---|
| `SetMenuStrip(window, menu)` | Attach strip to window and render |
| `ClearMenuStrip(window)` | Detach strip (does not free it) |
| `ResetMenuStrip(window, menu)` | Faster ClearMenuStrip+SetMenuStrip used when only state (CHECKED/ENABLED) changed, not layout |
| `ItemAddress(strip, num)` | Resolve a `FULLMENUNUM`-packed number to a `MenuItem *` |
| `OnMenu(window, num)` / `OffMenu(window, num)` | Enable/disable a menu, item, or sub-item |
| `LendMenus(fromwindow, towindow)` | V36: share a menu strip between windows |

### 8.3 Keyboard shortcuts (COMMSEQ)

When `COMMSEQ` is set in `MenuItem.Flags`, the `Command` byte holds the Amiga-key letter (case-insensitive). Intuition inserts the Amiga key image to the right of the label and matches Amiga-letter keystrokes in the window's keyboard stream.

Under V39 with GadTools's `NM_COMMANDSTRING`, `nm_CommKey` can point to an arbitrary string (e.g. "Shift F1") instead of a single letter — useful for showing complex bindings to the user, though Intuition does not actually match them for you.

### 8.4 `IDCMP_MENUPICK` handling loop

```c
case IDCMP_MENUPICK: {
    UWORD num = imsg->Code;
    while (num != MENUNULL) {
        struct MenuItem *item = ItemAddress(menu, num);
        handle_menu_pick(MENUNUM(num), ITEMNUM(num), SUBNUM(num));
        num = item->NextSelect;   /* drag-select chain */
    }
    break;
}
```

Drag-select: when the user holds the mouse button and drags across multiple CHECKIT items, Intuition chains them via `NextSelect` and delivers them as one MENUPICK. Always follow the chain.

### 8.5 Checkmark / radio menus

- `CHECKIT` — item can be checked.
- `CHECKED` — currently checked (set and cleared by you or Intuition).
- `MENUTOGGLE` — item toggles on/off when picked.
- No `MENUTOGGLE` + `CHECKIT` = mutually-exclusive set via `MutualExclude`.

`MutualExclude` is a bitmask: bit `n` set means "picking this item un-checks item `n` in the same menu". Classic three-way radio:

```c
item0.MutualExclude = ~(1<<0);   /* clear everyone except me */
item1.MutualExclude = ~(1<<1);
item2.MutualExclude = ~(1<<2);
```

---

<a name="asl"></a>
## 9. asl.library — file, font, and screenmode requesters

`asl.library` (V36+) is the "Amiga Standard Library" for system-provided requesters: pick-a-file, pick-a-font, pick-a-screenmode. The API is uniform: allocate a requester structure, fill in tags, display it, read the results, free it.

```c
struct FileRequester      *r = AllocAslRequest(ASL_FileRequest,      tags);
struct FontRequester      *r = AllocAslRequest(ASL_FontRequest,      tags);
struct ScreenModeRequester *r = AllocAslRequest(ASL_ScreenModeRequest, tags);

if (AslRequest(r, tags)) { /* user hit OK */ }
FreeAslRequest(r);
```

Convenience type-specific wrappers exist — `AllocFileRequest`, `AllocFontRequest`, `AllocScrModeRequest` — but all three are equivalent to `AllocAslRequest` with the type constant.

### 9.1 File requester

```c
struct FileRequester
{
    UBYTE  fr_Reserved0[4];
    STRPTR fr_File;        /* file basename on exit */
    STRPTR fr_Drawer;      /* drawer path on exit */
    UBYTE  fr_Reserved1[10];
    WORD   fr_LeftEdge, fr_TopEdge, fr_Width, fr_Height;
    UBYTE  fr_Reserved2[2];
    LONG   fr_NumArgs;     /* multi-select count */
    struct WBArg *fr_ArgList;  /* multi-select list */
    APTR   fr_UserData;
    UBYTE  fr_Reserved3[8];
    STRPTR fr_Pattern;     /* pattern gadget on exit */
};
```

Essential tags (`ASL_TB = TAG_USER+0x80000`):

| Tag | Data | Meaning |
|---|---|---|
| `ASLFR_Window` | `Window *` | Parent window |
| `ASLFR_TitleText` | `STRPTR` | Requester title |
| `ASLFR_InitialDrawer` | `STRPTR` | Starting directory |
| `ASLFR_InitialFile` | `STRPTR` | Starting filename |
| `ASLFR_InitialPattern` | `STRPTR` | Starting pattern |
| `ASLFR_DoSaveMode` | BOOL | Present as a "save" requester |
| `ASLFR_DoMultiSelect` | BOOL | Allow selecting multiple files |
| `ASLFR_DoPatterns` | BOOL | Show pattern gadget |
| `ASLFR_DrawersOnly` | BOOL | Hide files, pick a drawer |
| `ASLFR_RejectIcons` | BOOL | Hide .info files |
| `ASLFR_RejectPattern` | `STRPTR` | Hide files matching pattern |
| `ASLFR_AcceptPattern` | `STRPTR` | Show only files matching pattern |
| `ASLFR_PositiveText` / `NegativeText` | `STRPTR` | Button labels |
| `ASLFR_SleepWindow` | BOOL | Block parent window input during requester |
| `ASLFR_IntuiMsgFunc` | `Hook *` | Hook called for IntuiMessages |
| `ASLFR_Flags1` / `ASLFR_Flags2` | ULONG | Flag bits (see FRF_* in asl.h) |

After `AslRequest()` returns TRUE, inspect `fr_File` and `fr_Drawer` (or `fr_ArgList` for multi-select); concatenate with `AddPart()` from dos.library to build the full path.

### 9.2 Font requester

```c
struct FontRequester
{
    UBYTE  fo_Reserved0[8];
    struct TextAttr  fo_Attr;       /* picked font */
    UBYTE  fo_FrontPen, fo_BackPen, fo_DrawMode;
    UBYTE  fo_Reserved1;
    APTR   fo_UserData;
    WORD   fo_LeftEdge, fo_TopEdge, fo_Width, fo_Height;
    struct TTextAttr fo_TAttr;      /* extended attrs (V38+) */
};
```

Tags parallel the file requester (`ASLFO_Window`, `ASLFO_TitleText`, ...) and add:

- `ASLFO_InitialName`, `ASLFO_InitialSize`, `ASLFO_InitialStyle`, `ASLFO_InitialFlags`
- `ASLFO_DoFrontPen`, `ASLFO_DoBackPen`, `ASLFO_DoStyle`, `ASLFO_DoDrawMode` — show extra controls
- `ASLFO_FixedWidthOnly`
- `ASLFO_MinHeight` / `ASLFO_MaxHeight` — filter by size
- `ASLFO_ModeList` — substitute list of draw modes
- `ASLFO_FrontPens` / `ASLFO_BackPens` — palette color arrays
- `ASLFO_SampleText` (V45) — text shown in the preview area

The returned `fo_Attr` is a normal `struct TextAttr` you can pass to `OpenDiskFont()`.

### 9.3 Screen mode requester

```c
struct ScreenModeRequester
{
    ULONG sm_DisplayID;
    ULONG sm_DisplayWidth, sm_DisplayHeight;
    UWORD sm_DisplayDepth, sm_OverscanType;
    BOOL  sm_AutoScroll;
    ULONG sm_BitMapWidth, sm_BitMapHeight;
    WORD  sm_LeftEdge, sm_TopEdge, sm_Width, sm_Height;
    BOOL  sm_InfoOpened;
    WORD  sm_InfoLeftEdge, sm_InfoTopEdge, sm_InfoWidth, sm_InfoHeight;
    APTR  sm_UserData;
};
```

Tags (`ASLSM_*`) include initial values (`ASLSM_InitialDisplayID`, `InitialDisplayWidth`, ..., `InitialAutoScroll`), filtering (`MinWidth`/`MaxWidth`/`MinDepth`/`MaxDepth`, `PropertyFlags`/`PropertyMask`), and `ASLSM_CustomSMList` to add your own display modes to the pick list.

The returned `sm_DisplayID` is a 32-bit mode key suitable for `SA_DisplayID` when opening a screen.

### 9.4 Emulator note

ASL's entire job is to produce an intuition window, populate it with gadgets, run the event loop, and return. An emulator that implements intuition correctly gets ASL almost for free — the library is implemented on top of GadTools (V38+) and BOOPSI. The one tricky bit is that `ASLFR_IntuiMsgFunc` (and the other `IntuiMsgFunc`s) is a hook called **from the asl task** whenever a non-asl IntuiMessage arrives on the shared port; unhandled messages are re-delivered to the application. Make sure your hook dispatch respects this.

---

<a name="iffparse"></a>
## 10. iffparse.library

`iffparse.library` (V36+) provides the infrastructure for reading and writing **EA-IFF-85** (Electronic Arts Interchange File Format 1985) files — the Amiga's standard container format for images (ILBM), sounds (8SVX), animations (ANIM), text (FTXT), preferences, clipboard snippets, and many more.

### 10.1 IFF structure recap

An IFF file is a tree of **chunks**. Each chunk has an 8-byte header followed by its payload:

```
offset  size   field
  0      4     4-byte ID (ASCII, uppercase if standard, with spaces)
  4      4     size (BE 32-bit, excludes the 8-byte header)
  8      N     payload (N = size; padded to even with a zero byte)
```

There are three "group" chunk IDs whose payload is itself a sequence of chunks:

- `FORM` — a form of some type. First 4 bytes of payload are a type ID (e.g. `ILBM`, `8SVX`, `ANIM`).
- `LIST` — a list of forms of the same type, possibly with shared properties.
- `CAT ` — a catenation of forms of different types.

And one "property" chunk:

- `PROP` — shared properties for a LIST.

At the outermost level, a well-formed IFF file is a single `FORM` (or `LIST`, or `CAT `). The type ID in a `FORM` (ILBM, 8SVX, ...) tells you what's inside.

Within a FORM, there are standard chunks for that form type, plus application-specific ones. E.g. an ILBM FORM typically contains `BMHD` (bitmap header), `CMAP` (colormap), `BODY` (compressed pixel data), and optionally `CAMG` (Amiga viewmode), `CRNG` (color ranges for cycling), `DPPS` (DPaint saved state).

### 10.2 `IFFHandle` and flag bits

```c
struct IFFHandle
{
    ULONG iff_Stream;    /* client-interpreted stream handle */
    ULONG iff_Flags;     /* IFFF_* */
    LONG  iff_Depth;     /* current context-stack depth */
};
```

Flags:

- `IFFF_READ` (0) — open for read
- `IFFF_WRITE` (1) — open for write
- `IFFF_FSEEK` — forward seek supported
- `IFFF_RSEEK` — random seek supported

### 10.3 The basic call sequence

```c
struct IFFHandle *iff;
BPTR fh;
LONG err;

iff = AllocIFF();
if (!iff) fail();

fh = Open("ram:pic.iff", MODE_OLDFILE);
iff->iff_Stream = (ULONG)fh;
InitIFFasDOS(iff);          /* or InitIFFasClip() for the clipboard */
if ((err = OpenIFF(iff, IFFF_READ)) != 0) fail();

/* Declare interest in certain chunks before parsing */
StopChunk(iff, ID_ILBM, ID_BMHD);
PropChunk(iff, ID_ILBM, ID_CMAP);

while ((err = ParseIFF(iff, IFFPARSE_SCAN)) == 0) {
    struct ContextNode *cn = CurrentChunk(iff);
    if (cn->cn_Type == ID_ILBM && cn->cn_ID == ID_BMHD) {
        ReadChunkBytes(iff, &bmhd, sizeof bmhd);
        /* ... */
    }
}

CloseIFF(iff);
Close(fh);
FreeIFF(iff);
```

`ParseIFF(iff, mode)` walks the tree. `mode` is one of:

- `IFFPARSE_SCAN` — walk until next "interesting" event (stop chunk hit, property encountered, exit handler triggered, or end-of-file).
- `IFFPARSE_STEP` — single-step one chunk at a time.
- `IFFPARSE_RAWSTEP` — include even sub-structural events the scan mode would hide.

Return values:
- 0 — found an interesting chunk; process it (`CurrentChunk()`, `ReadChunkBytes()`, etc.)
- `IFFERR_EOF` — end of file
- `IFFERR_EOC` — about to leave a context (pop)
- negative — see `iffparse.h` (`IFFERR_MANGLED`, `IFFERR_SYNTAX`, `IFFERR_NOTIFF`, ...)

### 10.4 The declarations: Stop, Prop, Collection

Before calling `ParseIFF()` you tell the library what to do with each chunk type:

- `StopChunk(iff, type, id)` — stop parsing when a chunk of this type/id is entered; your code reads it and resumes.
- `StopOnExit(iff, type, id)` — stop when *leaving* a chunk (useful for FORM post-processing).
- `PropChunk(iff, type, id)` — automatically collect this as a "property" chunk (first occurrence only, in the current scope). Retrieve with `FindProp()` later.
- `CollectionChunk(iff, type, id)` — collect **every** occurrence into a list. Retrieve with `FindCollection()` which returns a `CollectionItem` chain.
- `EntryHandler`/`ExitHandler` — install custom hooks on chunk entry/exit.

Properties are scoped: a `PROP` inside a `LIST` applies to all forms in the list; a chunk declared inside a FORM's body applies only to that form. `FindProp()` looks up the innermost matching prop.

### 10.5 Writing an IFF file

```c
iff = AllocIFF();
fh  = Open("ram:out.iff", MODE_NEWFILE);
iff->iff_Stream = (ULONG)fh;
InitIFFasDOS(iff);
OpenIFF(iff, IFFF_WRITE);

PushChunk(iff, ID_ILBM, ID_FORM, IFFSIZE_UNKNOWN);
  PushChunk(iff, 0, ID_BMHD, sizeof bmhd);
  WriteChunkBytes(iff, &bmhd, sizeof bmhd);
  PopChunk(iff);

  PushChunk(iff, 0, ID_CMAP, cmap_bytes);
  WriteChunkBytes(iff, cmap, cmap_bytes);
  PopChunk(iff);

  PushChunk(iff, 0, ID_BODY, IFFSIZE_UNKNOWN);
  WriteChunkBytes(iff, body_data, body_len);
  PopChunk(iff);
PopChunk(iff);

CloseIFF(iff);
Close(fh);
FreeIFF(iff);
```

`IFFSIZE_UNKNOWN` (= -1) makes the parser compute the size at PopChunk time; better to pass the exact size if you have it.

### 10.6 Essential function list

From `iffparse.doc` (each available on every iffparse.library ≥V36):

- `AllocIFF()` / `FreeIFF(iff)` — allocate/free handle
- `OpenIFF(iff, rwMode)` / `CloseIFF(iff)` — open/close parsing session
- `InitIFFasDOS(iff)` / `InitIFFasClip(iff)` — set up stream handler for dos.library / clipboard
- `InitIFF(iff, fsflags, streamhook)` — init with custom stream hook
- `ParseIFF(iff, mode)` — walk the tree
- `StopChunk`, `StopOnExit`, `PropChunk`, `CollectionChunk`, `StopChunks`, `PropChunks`, `CollectionChunks` (the plurals take an array)
- `ReadChunkBytes(iff, buf, nbytes)` / `ReadChunkRecords(iff, buf, bytesPerRec, numRec)`
- `WriteChunkBytes(iff, buf, nbytes)` / `WriteChunkRecords(iff, buf, bytesPerRec, numRec)`
- `PushChunk(iff, type, id, size)` / `PopChunk(iff)`
- `CurrentChunk(iff)` / `ParentChunk(cn)` — navigate the context stack
- `FindProp(iff, type, id)` — look up a stored property
- `FindCollection(iff, type, id)` — get the collection item list
- `StoreItemInContext(iff, lci, cn)` — for class-implementors
- `EntryHandler(iff, type, id, pos, hook, data)` / `ExitHandler(iff, type, id, pos, hook, data)`
- `GoodID(id)` / `GoodType(type)` — validate 4-byte codes
- `IDtoStr(id, buf)` — pretty-print

### 10.7 Constants

```c
#define ID_FORM  MAKE_ID('F','O','R','M')
#define ID_LIST  MAKE_ID('L','I','S','T')
#define ID_CAT   MAKE_ID('C','A','T',' ')
#define ID_PROP  MAKE_ID('P','R','O','P')
#define ID_NULL  MAKE_ID(' ',' ',' ',' ')
```

Error codes:

```c
IFFERR_EOF        -1   /* end of file */
IFFERR_EOC        -2   /* end of context (leaving chunk) */
IFFERR_NOSCOPE    -3   /* no valid scope for property */
IFFERR_NOMEM      -4
IFFERR_READ       -5
IFFERR_WRITE      -6
IFFERR_SEEK       -7
IFFERR_MANGLED    -8
IFFERR_SYNTAX     -9
IFFERR_NOTIFF    -10
IFFERR_NOHOOK    -11
IFF_RETURN2CLIENT -12
```

### 10.8 Emulator note

For an emulator, the important observation is that iffparse is a pure host-side library — it makes no hardware accesses. If you implement `dos.library` correctly, iffparse calls into it for `Read`/`Write`/`Seek` through the InitIFFasDOS stream hook. The library maintains its own context stack in host memory, so the only thing it touches below it is stdio-like byte I/O. Many system applications use it, notably every program that reads or writes IFF-ILBM pictures, 8SVX samples, or the clipboard.

---

<a name="commodities"></a>
## 11. commodities.library V36+ — the input event broker

"Commodities" are small programs (background agents) that intercept and transform the system's input stream before it reaches Intuition. Classic examples from OS 2.x: **Blanker** (blank screen after idle), **Exchange** (the commodities control panel), **FKey** (programmable function keys), **IHelp** (keyboard window cycling), **NoCapsLock** (trap caps-lock). They are what run when you fire up the `Commodities` drawer on the Workbench.

The library implements a graph of **Cx objects** that sits in the `input.device` handler chain at priority 51 — one step above intuition's priority-50 handler. Each commodity registers a **broker** with a name, icon, and command port, and builds a **filter tree** describing which input events it wants and what to do with them.

### 11.1 Object types

From `commodities.h` line 87:

```c
#define CX_INVALID    0    /* null/unusable */
#define CX_FILTER     1    /* matches input events */
#define CX_TYPEFILTER 2    /* obsolete */
#define CX_SEND       3    /* sends CXM_IEVENT to a MsgPort */
#define CX_SIGNAL     4    /* signals a task */
#define CX_TRANSLATE  5    /* replaces event with an event chain */
#define CX_BROKER     6    /* application representative */
#define CX_DEBUG      7    /* dumps to serial/debug */
#define CX_CUSTOM     8    /* synchronous app function */
#define CX_ZERO       9    /* terminator */
```

The input event chain flows from parent to children. When an event enters the tree:

- A **filter** either matches (pass to children) or not (pass to siblings).
- A **sender** queues a `CXM_IEVENT` CxMessage to a MsgPort for the application to process asynchronously.
- A **signal** sets a signal bit on a task.
- A **translate** replaces the current event with a list of new events (e.g. expanding a hotkey into a sequence of keystrokes).
- A **custom** calls an application function synchronously.
- A **debug** dumps the event to the serial port.

### 11.2 `NewBroker` and `CxBroker`

```c
struct NewBroker
{
    BYTE    nb_Version;         /* NB_VERSION = 5 */
    STRPTR  nb_Name;            /* ≤ CBD_NAMELEN=24 */
    STRPTR  nb_Title;           /* ≤ CBD_TITLELEN=40 */
    STRPTR  nb_Descr;           /* ≤ CBD_DESCRLEN=40 */
    WORD    nb_Unique;          /* NBU_DUPLICATE | NBU_UNIQUE | NBU_NOTIFY */
    WORD    nb_Flags;           /* COF_SHOW_HIDE */
    BYTE    nb_Pri;             /* priority in broker list */
    struct MsgPort *nb_Port;    /* command port */
    WORD    nb_ReservedChannel;
};

CxObj *CxBroker(struct NewBroker *nb, LONG *error);
```

Returns a root `CxObj` for the broker; `error` gets one of:

- `CBERR_OK` (0)
- `CBERR_SYSERR` — out of memory
- `CBERR_DUP` — name already in use (and `nb_Unique` said not OK)
- `CBERR_VERSION` — unknown nb_Version

### 11.3 Building the tree

Once you have a broker, you attach child filters:

```c
CxObj *CxFilter(IX *description);     /* alias: CreateCxObj(CX_FILTER, ...) */
CxObj *CxSender(MsgPort *port, LONG id);
CxObj *CxSignal(Task *task, LONG sigbit);
CxObj *CxTranslate(InputEvent *chain);
CxObj *CxCustom(PFL action, LONG id);
CxObj *CxDebug(LONG id);
```

These all ultimately call `CreateCxObj(type, data1, data2)`. The header provides convenient macros:

```c
#define CxFilter(d)       CreateCxObj(CX_FILTER,    (LONG)d,    0)
#define CxSender(port,id) CreateCxObj(CX_SEND,      (LONG)port, (LONG)id)
#define CxSignal(task,s)  CreateCxObj(CX_SIGNAL,    (LONG)task, (LONG)s)
#define CxTranslate(ie)   CreateCxObj(CX_TRANSLATE, (LONG)ie,   0)
#define CxDebug(id)       CreateCxObj(CX_DEBUG,     (LONG)id,   0)
#define CxCustom(act,id)  CreateCxObj(CX_CUSTOM,    (LONG)act,  (LONG)id)
```

Attach to the tree:

- `AttachCxObj(parent, child)` — append as last child
- `EnqueueCxObj(parent, child)` — insert by priority
- `InsertCxObj(parent, child, predecessor)` — insert after a specific node
- `RemoveCxObj(co)` — detach from parent
- `DeleteCxObj(co)` — detach and destroy
- `DeleteCxObjAll(co)` — destroy the whole subtree
- `ActivateCxObj(co, TRUE/FALSE)` — temporarily enable/disable without destroying

### 11.4 Filter descriptions — `InputXpression`

```c
struct InputXpression
{
    UBYTE ix_Version;     /* IX_VERSION = 2 */
    UBYTE ix_Class;       /* must match exactly (IECLASS_RAWKEY etc.) */
    UWORD ix_Code;
    UWORD ix_CodeMask;    /* bits in ix_Code that matter */
    UWORD ix_Qualifier;
    UWORD ix_QualMask;
    UWORD ix_QualSame;    /* synonyms: IXSYM_SHIFT/CAPS/ALT */
};
```

A filter matches if:
```
   event.Class    == ix.Class
&& (event.Code     & ix_CodeMask) == (ix_Code & ix_CodeMask)
&& (event.Qualifier & ix_QualMask) == (ix_Qualifier & ix_QualMask)
```

Hand-filling this is painful, so commodities.library provides **`ParseIX(description, ix)`** which parses a human-readable string like:

```
"rawkey lshift f1"
"rawkey control amiga k"
"rawmouse middle"
"ctrl alt a"
```

into an `InputXpression`. It understands shifts, control, caps, alt, amiga-key, function keys, cursor keys, rawkey codes, and rawmouse buttons. The V37+ canonical hotkey broker pattern is:

```c
IX ix;
ParseIX("ctrl alt f1", &ix);
CxObj *filter = CxFilter(&ix);
CxObj *sender = CxSender(myport, MY_HOTKEY_ID);
AttachCxObj(broker, filter);
AttachCxObj(filter, sender);
ActivateCxObj(broker, TRUE);
```

`InvertKeyMap(ansiCode, keymap, ie)` is the inverse translation — given an ASCII character and a keymap, fill in an `InputEvent` so you can `CxTranslate` to it.

### 11.5 CxMessages

Messages arrive on the broker's `nb_Port` (for commands) or on the sender's port (for CXM_IEVENT events). The message is a `CxMsg *`, and you extract the payload with:

```c
ULONG CxMsgType(CxMsg *);   /* CXM_IEVENT or CXM_COMMAND */
APTR  CxMsgData(CxMsg *);   /* InputEvent * for IEVENT, command code for COMMAND */
LONG  CxMsgID(CxMsg *);     /* the id you set at CxSender creation */
```

Command messages (`CXM_COMMAND`) on a broker's port are system-issued:

- `CXCMD_DISABLE` (15) — please disable
- `CXCMD_ENABLE` (17)
- `CXCMD_APPEAR` (19) — user clicked "Show"; open your window
- `CXCMD_DISAPPEAR` (21) — close your window
- `CXCMD_KILL` (23) — shut down
- `CXCMD_LIST_CHG` (27) — broker list changed
- `CXCMD_UNIQUE` (25) — your name was used by another broker

A commodity must respond to at least `CXCMD_KILL` by cleaning up and exiting.

### 11.6 Worked example — hotkey to print a message

```c
struct MsgPort *port;
CxObj *broker, *filter, *sender;
struct NewBroker nb = {
    NB_VERSION, "Greeter", "Greeter 1.0", "Say hi on Ctrl-Alt-G",
    NBU_UNIQUE | NBU_NOTIFY, 0, 0, NULL, 0
};
LONG error;

port = CreateMsgPort();
nb.nb_Port = port;

broker = CxBroker(&nb, &error);
if (!broker) { /* ... */ }

IX ix;
ParseIX("ctrl alt g", &ix);
filter = CxFilter(&ix);
AttachCxObj(broker, filter);

sender = CxSender(port, 42);
AttachCxObj(filter, sender);

/* also translate — eat the event so nothing else sees it */
AttachCxObj(filter, CxTranslate(NULL));

ActivateCxObj(broker, TRUE);

ULONG sigmask = 1UL << port->mp_SigBit;
for (;;) {
    Wait(sigmask);
    CxMsg *m;
    while ((m = (CxMsg *)GetMsg(port))) {
        if (CxMsgType(m) == CXM_IEVENT && CxMsgID(m) == 42) {
            printf("Hi!\n");
        } else if (CxMsgType(m) == CXM_COMMAND) {
            switch (CxMsgID(m)) {
            case CXCMD_KILL: goto shutdown;
            case CXCMD_DISABLE: ActivateCxObj(broker, FALSE); break;
            case CXCMD_ENABLE:  ActivateCxObj(broker, TRUE);  break;
            }
        }
        ReplyMsg((struct Message *)m);
    }
}

shutdown:
DeleteCxObjAll(broker);
DeleteMsgPort(port);
```

### 11.7 Hooking into input.device

The broker as a whole is implemented as an input handler installed at priority 51 on `input.device`. `commodities.library` creates this handler the first time any broker is created and keeps it alive while the broker count is non-zero. The handler runs **before** Intuition's handler, so events consumed by a translate or filter never reach windows. A commodity that wants to "preview" an event without consuming it must use a `CxSender` (which forwards the event) rather than a `CxTranslate` to NULL (which eats it).

This means an emulator must model the ordering: commodity brokers run before Intuition, and commodities can cancel events before IDCMP delivery.

### 11.8 Complete function list (commodities.doc)

- `CxBroker(nb, errPtr)` — create broker
- `CreateCxObj(type, arg1, arg2)` — create any object
- `CxObjType(co)` — type inquiry
- `CxObjError(co)` — error state of last operation on this object
- `ClearCxObjError(co)`
- `SetCxObjPri(co, pri)`
- `AttachCxObj(parent, child)`
- `EnqueueCxObj(parent, child)`
- `InsertCxObj(parent, child, pred)`
- `RemoveCxObj(co)`
- `DeleteCxObj(co)` / `DeleteCxObjAll(co)`
- `ActivateCxObj(co, bool)`
- `ParseIX(description, ix)` — parse human string into InputXpression
- `InvertKeyMap(ansi, ie, keymap)`
- `AddIEvents(events)` — inject events into the input stream
- `CxMsgType(msg)` / `CxMsgData(msg)` / `CxMsgID(msg)`
- `CopyBrokerList(list)` / `FreeBrokerList(list)` — query active brokers
- `BrokerCommand(name, cmd)` — send a command to a named broker
- `DivertCxMsg(msg, to, returnTo)` — redirect a command message
- `RouteCxMsg(msg, to)` — re-route an event message

---

<a name="workbench"></a>
## 12. Workbench and workbench.library

Workbench the GUI is driven by the Workbench process and `workbench.library`. This section covers the library API and the model underneath — disk/drawer icons, AppWindows, AppIcons, AppMenus, WBStartup, and the difference between Workbench-launched and CLI-launched programs.

### 12.1 The Workbench process

When Kickstart finishes and `dos.library` runs `startup-sequence`, the shell eventually runs `LoadWB`. `LoadWB` opens `workbench.library` (which in turn opens `icon.library`, `intuition.library`, and `dos.library`), and the library's init code:

1. Opens (or gets a handle to) the default public screen named `"Workbench"`.
2. Creates the Workbench process with a public MsgPort named `"Workbench"`.
3. Reads the Workbench disk's root directory, looks up `.info` files with `GetDiskObject()`, arranges them as AppIcons on the Workbench screen using `DrawIconState()`, and binds each to a drawer/tool/project/disk.
4. Enters a loop that listens on its port for input from the user (double-click opens a tool or drawer; drag drops files; menus perform operations) and for AppMessages from registered applications.

Opening a drawer is *recursive*: the Workbench creates a subsidiary drawer window (a plain intuition window with icon-style gadgets) managed by the Workbench process, reads the drawer's `.info` files, and draws them there.

### 12.2 `struct WBStartup` and `WBArg`

(from `workbench/startup.h`)

```c
struct WBStartup {
    struct Message  sm_Message;     /* standard exec message */
    struct MsgPort *sm_Process;     /* the new process's startup port */
    BPTR            sm_Segment;     /* LoadSeg'd segment */
    LONG            sm_NumArgs;     /* elements in ArgList */
    char           *sm_ToolWindow;  /* description of console window */
    struct WBArg   *sm_ArgList;     /* the arguments */
};

struct WBArg {
    BPTR  wa_Lock;    /* dos.library Lock on containing directory */
    BYTE *wa_Name;    /* filename within that lock */
};
```

When Workbench launches a program, it:

1. Calls `LoadSeg()` on the tool's executable.
2. Calls `CreateNewProcTags(NP_Entry, tool_entry, NP_StackSize, stack, NP_Name, "toolname", ...)` (or the older `CreateProc`), returning a new Process.
3. Fills in a `WBStartup` with `sm_Segment = loaded segment`, `sm_ArgList = [tool_arg, selected_project_args...]`, `sm_NumArgs = 1 + num_selected`.
4. `PutMsg(&process->pr_MsgPort, &wbstartup->sm_Message)`.
5. Waits for the reply.

When the program's `main()` finishes, its startup glue `ReplyMsg()`s the WBStartup (not `FreeMem()`s it — Workbench owns it). Workbench then `UnLoadSeg()`s the segment and goes back to sleep.

Key points:

- `sm_ArgList[0]` is always the tool itself. `wa_Lock` is a lock on the directory containing the tool; `wa_Name` is the tool's filename. For a program to find its own data files alongside itself, you typically `CurrentDir(sm_ArgList[0].wa_Lock)` early in `main()`.
- `sm_ArgList[1..N]` are the selected project icons. Each `wa_Lock` is a lock on the containing directory, `wa_Name` the file's name there.
- **Never `UnLock` any `wa_Lock`** — Workbench owns them all and will release them when you `ReplyMsg()` the WBStartup.
- The `sm_Process` pointer in the message is to **your own** process port, supplied so you know where to `ReplyMsg()` back.

### 12.3 Distinguishing Workbench launch from CLI launch

In `struct Process` (from `dos/dosextens.h`), `pr_CLI` is a BPTR to a `struct CommandLineInterface`. If it is non-zero, the program was launched from the CLI. If it is zero, it was launched from Workbench and there is a WBStartup message waiting on `pr_MsgPort`.

The canonical startup-code idiom (abbreviated):

```c
int __main(void)
{
    struct Process *me = (struct Process *)FindTask(NULL);
    struct WBStartup *wbs = NULL;

    if (me->pr_CLI == 0) {
        /* Workbench launch */
        WaitPort(&me->pr_MsgPort);
        wbs = (struct WBStartup *)GetMsg(&me->pr_MsgPort);
    }

    int rc = main(wbs ? 0 : argc, wbs ? NULL : argv);   /* or a WB-aware main */

    if (wbs) {
        Forbid();    /* prevent unload before reply */
        ReplyMsg(&wbs->sm_Message);
    }
    return rc;
}
```

The `Forbid()` before `ReplyMsg()` is essential — once the reply happens, Workbench is free to `UnLoadSeg()` your code, and without the forbid you could lose the race and start executing freed memory.

### 12.4 `workbench.library` function list (wb.doc)

From `Documentation/Autodocs/wb.doc`, V45:

- `AddAppIconA(id, userdata, text, msgport, lock, diskobj, tags)` — add an AppIcon to the Workbench
- `AddAppMenuItemA(id, userdata, text, msgport, tags)` — add a menu item under "Tools"
- `AddAppWindowA(id, userdata, window, msgport, tags)` — turn a regular window into an AppWindow (can receive icon drops)
- `RemoveAppIcon(appicon)`
- `RemoveAppMenuItem(appmenuitem)`
- `RemoveAppWindow(appwindow)`
- `AddAppWindowDropZoneA(appwindow, id, userdata, tags)` — V44: define a sub-region of an AppWindow that receives drops separately
- `RemoveAppWindowDropZone(appwindow, dropzone)`
- `WBInfo(lock, name, screen)` — open the standard Information requester for a file
- `OpenWorkbenchObjectA(name, tags)` — V44: open a drawer window or launch a program by path
- `CloseWorkbenchObjectA(name, tags)` — V45
- `WorkbenchControlA(name, tags)` — V44: set/query Workbench configuration (hidden device lists, default stack size, hooks, etc.)

The pre-V44 non-tag aliases `AddAppIcon`, `AddAppWindow`, `AddAppMenuItem` take fixed argument lists; the `*A` versions supersede them with tag-based configuration.

### 12.5 AppWindow / AppIcon / AppMenu flow in detail

**AppWindow:**

```c
struct AppWindow *aw;
struct MsgPort *port = CreateMsgPort();
struct Window *win = OpenWindowTags(...);

aw = AddAppWindowA(MY_ID, (ULONG)myuserdata, win, port, NULL);

/* event loop needs both window->UserPort and port */
ULONG winsig = 1UL << win->UserPort->mp_SigBit;
ULONG appsig = 1UL << port->mp_SigBit;

for (;;) {
    ULONG signals = Wait(winsig | appsig);
    if (signals & winsig) { /* handle IDCMP */ }
    if (signals & appsig) {
        struct AppMessage *amsg;
        while ((amsg = (struct AppMessage *)GetMsg(port))) {
            /* amsg->am_Type == AMTYPE_APPWINDOW */
            for (LONG i = 0; i < amsg->am_NumArgs; i++) {
                BPTR dir = amsg->am_ArgList[i].wa_Lock;
                char *n  = amsg->am_ArgList[i].wa_Name;
                /* CurrentDir(dir); open file n */
            }
            ReplyMsg((struct Message *)amsg);
        }
    }
}

RemoveAppWindow(aw);
CloseWindow(win);
DeleteMsgPort(port);
```

An AppMessage for an AppWindow has `am_Type = AMTYPE_APPWINDOW` (7). `am_ArgList` is an array of `WBArg`s — same structure as `WBStartup->sm_ArgList` — one per icon the user dragged onto your window. `am_NumArgs` is the count. `am_MouseX/Y` is where the user dropped them.

**AppIcon:**

```c
struct DiskObject *icon = GetDiskObject("myicon");
struct AppIcon *ai = AddAppIconA(1, 0, "MyApp", port, NULL, icon, NULL);
```

An AppIcon sits on the Workbench screen. Double-clicking it or dropping files on it generates an AppMessage with `am_Type = AMTYPE_APPICON` (8). Under V44, `am_Class` also gives an `AMCLASSICON_*` value indicating which menu item or action triggered the message — `Open`, `Copy`, `Rename`, `Information`, `Snapshot`, `UnSnapshot`, `LeaveOut`, `PutAway`, `Delete`, `FormatDisk`, `EmptyTrash`, plus `Selected`/`Unselected` to track mouse-click state.

The `DiskObject` you pass is the icon imagery; you typically `GetDefDiskObject(WBAPPICON)` for a generic one, or `GetDiskObject("MyApp")` to load `MyApp.info` from disk.

**AppMenu:**

```c
struct AppMenuItem *ami = AddAppMenuItemA(1, 0, "My Item", port,
    WBAPPMENUA_CommandKeyString, (ULONG)"M",
    TAG_DONE);
```

Adds an entry to Workbench's "Tools" menu. When the user picks it, you get an AppMessage of type `AMTYPE_APPMENUITEM` (9).

### 12.6 `OpenWorkbenchObject` and `WorkbenchControl`

V44 adds generic Workbench control.

`OpenWorkbenchObjectA(name, tags)`:
- `name` is a full path to a file, drawer, or device.
- Tags can include `WBOPENA_ArgLock`/`WBOPENA_ArgName` (equivalent to WBArg), `WBOPENA_Show`/`WBOPENA_ViewBy` (V45) for drawer views.
- If name is a drawer, Workbench opens its window (as if the user had double-clicked it).
- If name is a project, Workbench launches the associated default tool with the project as argument.
- If name is a tool, Workbench launches it.

This is how applications can programmatically say "please show the user the `SYS:Utilities` drawer" or "please launch `NotePad` on this file".

`WorkbenchControlA(name, tags)`:
Tags include:
- `WBCTRLA_IsOpen` — is a given drawer open?
- `WBCTRLA_DuplicateSearchPath` — dup the internal path list
- `WBCTRLA_GetDefaultStackSize`/`SetDefaultStackSize`
- `WBCTRLA_RedrawAppIcon` — force a redraw
- `WBCTRLA_GetProgramList`/`FreeProgramList` — running WB programs
- `WBCTRLA_GetSelectedIconList`/`FreeSelectedIconList`
- `WBCTRLA_GetOpenDrawerList`/`FreeOpenDrawerList`
- `WBCTRLA_GetHiddenDeviceList`/`FreeHiddenDeviceList` — see "hide devices" preferences
- `WBCTRLA_AddHiddenDeviceName`/`RemoveHiddenDeviceName`
- `WBCTRLA_GetCopyHook`/`SetCopyHook`, `GetDeleteHook`/`SetDeleteHook`, `GetTextInputHook`/`SetTextInputHook` (V45) — install global hooks for copy/delete/text-input operations
- `WBCTRLA_AddSetupCleanupHook`/`RemSetupCleanupHook` (V45) — be notified when Workbench shuts down (e.g. IPrefs prefs change)

### 12.7 Enumerating icons on the Workbench

`workbench.library` does not expose a public "icon list" structure. To walk the Workbench screen's icons you use `WorkbenchControlA` with `WBCTRLA_GetSelectedIconList`/`GetOpenDrawerList` (V44), which return an exec-style `struct List *`.

### 12.8 Emulator note

workbench.library is implemented atop intuition, icon, and dos. It contains no hardware access. The Workbench process itself is ordinary AmigaDOS. The one tricky part is that it maintains its own "icon world" state (positions, selection) keyed off `.info` file contents + in-memory overrides; an accurate emulation must model `do_CurrentX/Y` persistence and the `NO_ICON_POSITION` (`0x80000000`) sentinel that means "don't write a saved position back".

---

<a name="icon"></a>
## 13. icon.library and the .info file

`icon.library` (since V1.0) reads and writes the Amiga's `.info` files and draws icons to a RastPort. A `.info` file contains a `DiskObject` structure plus variable-length data (icon imagery, default tool string, tooltype strings, tool window string, drawer data).

### 13.1 `struct DiskObject`

(from `workbench/workbench.h` line 65)

```c
struct DiskObject {
    UWORD          do_Magic;        /* WB_DISKMAGIC = 0xE310 */
    UWORD          do_Version;      /* WB_DISKVERSION = 1 */
    struct Gadget  do_Gadget;       /* appearance (embedded) */
    UBYTE          do_Type;         /* WBDISK/WBDRAWER/WBTOOL/WBPROJECT/... */
    STRPTR         do_DefaultTool;  /* for projects, the tool to run */
    STRPTR        *do_ToolTypes;    /* NULL-terminated array of "KEY=VALUE" */
    LONG           do_CurrentX;     /* saved position, or NO_ICON_POSITION */
    LONG           do_CurrentY;
    struct DrawerData *do_DrawerData;  /* drawer only */
    STRPTR         do_ToolWindow;   /* tool only */
    LONG           do_StackSize;    /* tool only */
};

struct DrawerData {
    struct NewWindow dd_NewWindow;  /* drawer window parameters */
    LONG             dd_CurrentX;   /* scroll position */
    LONG             dd_CurrentY;
    ULONG            dd_Flags;      /* DDFLAGS_SHOWDEFAULT/ICONS/ALL */
    UWORD            dd_ViewModes;  /* DDVM_BYICON/BYNAME/BYDATE/BYSIZE/BYTYPE */
};
```

`do_Type` values:

```c
#define WBDISK      1   /* a disk */
#define WBDRAWER    2   /* a drawer */
#define WBTOOL      3   /* an executable */
#define WBPROJECT   4   /* a data file with an associated tool */
#define WBGARBAGE   5   /* trashcan */
#define WBDEVICE    6   /* a device */
#define WBKICK      7   /* a kickstart disk */
#define WBAPPICON   8   /* an application-provided AppIcon */
```

### 13.2 Key functions (icon.doc)

- `GetDiskObject(name)` — read a `.info` file. `name` is the underlying object (e.g. `"sys:Notepad"` reads `sys:Notepad.info`). Returns `DiskObject *` or NULL. Uses ram-backed allocations.
- `GetDefDiskObject(type)` — get a default icon for a given type. Used for default drawer/tool/project icons.
- `PutDiskObject(name, obj)` — write it back.
- `FreeDiskObject(obj)` — free a DiskObject you got from GetDiskObject or GetDefDiskObject.
- `DupDiskObjectA(obj, tags)` (V44) — deep-copy, controlled by `ICONDUPA_*` tags.
- `DrawIconStateA(rp, obj, label, x, y, state, tags)` — render the icon to a RastPort at (x,y) with state (`IDS_NORMAL`/`IDS_SELECTED`) and optional tag list (`ICONDRAWA_DrawInfo`, `ICONDRAWA_Frameless`, etc.).
- `GetIconRectangleA(rp, obj, label, rect, tags)` — compute bounding rectangle without drawing.
- `EraseIcon(rp, obj, x, y)` — erase previously-drawn icon.
- `PutIconTagList(name, obj, tags)` (V44) — tag-controlled write with `ICONPUTA_*` tags (`NotifyWorkbench`, `PutDefaultType`, `DropPlanarIconImage`, `DropChunkyIconImage`, `DropNewIconToolTypes`, `OptimizeImageSpace`, `OnlyUpdatePosition`).
- `GetIconTagList(name, tags)` (V44) — tag-controlled read (`ICONGETA_GetDefaultType`, `ICONGETA_GetDefaultName`, `ICONGETA_FailIfUnavailable`, `ICONGETA_GetPaletteMappedIcon`, `ICONGETA_RemapIcon`, `ICONGETA_GenerateImageMasks`, `ICONGETA_Label`, `ICONGETA_Screen`).
- `IconControlA(obj, tags)` — get/set icon attributes (global screen, precision, transparent colors, palettes, image data, aspect ratio, frameless flag, NewIcons support, ...).

### 13.3 Tool types

Tool types are the Amiga's way of passing keyword arguments to Workbench-launched programs. Each is a string of the form `KEYWORD=value` or just `KEYWORD`; Workbench presents them as an editable list in the "Information" requester.

Helpers:

- `FindToolType(ttarray, keyword)` — returns pointer to the value, or NULL.
- `MatchToolValue(value, pattern)` — case-insensitive compare; understands `|` alternation.

Example: a tool that checks `DEBUG=1`:

```c
if (wbs) {
    struct DiskObject *obj = GetDiskObject(wbs->sm_ArgList[0].wa_Name);
    if (obj && FindToolType(obj->do_ToolTypes, "DEBUG")) debug = TRUE;
    if (obj) FreeDiskObject(obj);
}
```

Items in `do_ToolTypes` whose first character is `(` are "comments" and `FindToolType` skips them — this is how the Information panel represents toggled-off items.

### 13.4 `BumpRevision`

```c
char *BumpRevision(char *newname, char *oldname);
```

Given `"MyApp"`, appends `".1"`; given `"MyApp.1"`, produces `"MyApp.2"`, etc. Used for "Save As..." with auto-versioning. Writes into `newname`.

### 13.5 The on-disk `.info` format

Classic V1.x format: immediately after a binary dump of the `DiskObject` struct come the variable-length fields in this order:

1. `DrawerData` struct (if `do_DrawerData` was non-NULL).
2. The main `Image` referenced by `do_Gadget.GadgetRender`.
3. The alternate `Image` referenced by `do_Gadget.SelectRender` (if non-NULL).
4. `do_DefaultTool` string (null-terminated, length-prefixed as a BE 32-bit count if non-NULL).
5. `do_ToolTypes` string array (BE 32-bit count followed by BE 32-bit per-entry count and bytes).
6. `do_ToolWindow` string.

The NewIcons and OS 3.5/3.9 palette-mapped "color icons" are stored as additional tool-type entries prefixed with `IM1=` and `IM2=`, with the planar imagery retained for compatibility. Under OS 3.5+ (V44+), icon.library also understands a newer "ARGB" format embedded in the same file.

### 13.6 `do_CurrentX` / `do_CurrentY`

These are the on-disk "Snapshot" positions. A value of `NO_ICON_POSITION` (`0x80000000`) means "Workbench is free to lay this out". User-chosen positions are set by the Workbench "Snapshot" menu and persisted by `PutDiskObject()`. `PutIconTagList(name, obj, ICONPUTA_OnlyUpdatePosition, TRUE, TAG_DONE)` writes just the position without touching anything else — a common optimization when the user drags an icon.

---

<a name="datatypes"></a>
## 14. datatypes.library V39+

`datatypes.library` (OS 3.0, V39) was Commodore's attempt to generalize the iffparse pattern into a class framework for loading and manipulating structured data of any kind. A **data type** is a BOOPSI class that knows how to:

- Recognize a file by content.
- Load it into a normalized in-memory representation.
- Render it to a RastPort (for `picture.datatype`), play it (`sound.datatype`), decode it (`text.datatype`, `ascii.datatype`), etc.

Data types are registered with `datatypes.library` and the library looks up the right one based on file content sniffing (magic bytes + ID pattern match) or by `do_DefaultTool` from a DiskObject.

### 14.1 Built-in classes

OS 3.1 shipped these datatype classes (each is a BOOPSI public class, with a `"...datatype"` class ID):

| Class | File | Handles |
|---|---|---|
| `picture.datatype` | PICTURE | IFF-ILBM images |
| `sound.datatype` | SOUND | IFF-8SVX samples |
| `anim.datatype` | ANIM | IFF-ANIM animations |
| `text.datatype` | TEXT | ASCII text files |
| `ascii.datatype` | — | parent of `text.datatype` |
| `ftxt.datatype` | FTXT | IFF-FTXT styled text |
| `document.datatype` | — | parent class for document types |

Third parties added many more: `gif.datatype`, `jpeg.datatype`, `png.datatype`, `mod.datatype`, `cdxl.datatype`, `mpeg.datatype`, `svg.datatype`, etc.

### 14.2 The `DataType` object and `DataTypeHeader`

```c
struct DataType {
    struct Node        dtn_Node1;      /* by name */
    struct Node        dtn_Node2;      /* by group */
    struct DataTypeHeader *dtn_Header;
    struct List        dtn_ToolList;
    struct MinList     dtn_AttrList;
    ULONG              dtn_Length;
};

struct DataTypeHeader {
    STRPTR  dth_Name;       /* human-readable name */
    STRPTR  dth_BaseName;   /* e.g. "picture" */
    STRPTR  dth_Pattern;    /* file name pattern */
    UWORD   dth_Flags;
    UWORD   dth_Priority;
    ULONG   dth_GroupID;    /* DTG_SYS/USR/... */
    ULONG   dth_ID;         /* 4-byte type id */
    ...
};
```

Group IDs include `GID_SYSTEM`, `GID_TEXT`, `GID_DOCUMENT`, `GID_SOUND`, `GID_INSTRUMENT`, `GID_MUSIC`, `GID_PICTURE`, `GID_ANIMATION`, `GID_MOVIE`.

### 14.3 Key functions (datatypes.doc)

- `ObtainDataTypeA(source, handle, tags)` / `ReleaseDataType(dtn)` — look up the datatype class for a given source (file handle / IFF handle / disk object).
- `NewDTObjectA(name, tags)` — create an object from the matching class. Tags include `DTA_Name`, `DTA_SourceType`, `DTA_Handle`, `DTA_Domain`, `DTA_ErrorLevel`, `DTA_ErrorCode`, plus the familiar BOOPSI ones.
- `DisposeDTObject(o)`
- `DoDTMethodA(o, win, req, msg)` — invoke a method on the object (respecting gadget-info discipline).
- `SetDTAttrsA(o, win, req, tags)` / `GetDTAttrsA(o, tags)` — get/set bulk attributes.
- `DrawDTObjectA(rp, o, x, y, w, h, tags)` — render (for picture types).
- `PrintDTObjectA(o, win, req, tags)` — print via `printer.device`.
- `FindToolNode(list, command)` — look up a named tool for an object.

### 14.4 Why it matters for an emulator

For the emulator, datatypes are opt-in. Most OS 2.x and OS 3.0 programs don't use them. But any OS 3.5+ program or the stock `MultiView` viewer does, and `picture.datatype` in particular is a huge convenience — it turns a file of unknown format into a RastPort-renderable image with one call. If you implement it, you need `iffparse.library` working and must model file sniffing.

Brief — see `datatypes.doc` and `datatypes/datatypesclass.h` for the complete API if you need to go deeper. The V45 autodoc lists 844 lines; the most commonly used entry points are `NewDTObject`, `DisposeDTObject`, `DrawDTObjectA`, and `SetDTAttrs(o, DTA_VisibleVert, ..., DTA_VisibleHoriz, ...)`.

---

<a name="locale"></a>
## 15. locale.library V38+

`locale.library` (V38) is the internationalization/localization infrastructure. It provides:

- A **locale** (`struct Locale`) describing user preferences — language, measurement system, date/time/currency formats, collation rules.
- **Catalogs**: bundles of translated strings loaded from disk by a tool per-application. A catalog contains application-defined numeric keys mapped to localized strings.
- **GetLocaleStr** to look up standard strings (day names, month names, yes/no, AM/PM) from the active locale.
- **FormatDate**, **StrConvert**, and friends for formatting time/numbers according to the active locale.

### 15.1 `struct Locale`

Heavy — reproduced in §loc struct from the header above. The headline fields are `loc_LanguageName`, `loc_PrefLanguages[10]` (ordered fallback chain), `loc_CountryCode`, `loc_GMTOffset`, `loc_MeasuringSystem` (`MS_ISO` or `MS_AMERICAN`), `loc_CalendarType`, plus the printf-like format strings `loc_DateTimeFormat`, `loc_DateFormat`, `loc_TimeFormat`, etc.

### 15.2 Standard string constants

From `libraries/locale.h` line 41:

```c
/* days of week */
DAY_1..DAY_7     (1..7)    /* Sunday..Saturday */
ABDAY_1..ABDAY_7 (8..14)   /* Sun..Sat */

/* months */
MON_1..MON_12    (15..26)  /* January..December */
ABMON_1..ABMON_12 (27..38) /* Jan..Dec */

YESSTR         39   /* "Yes"-equivalent */
NOSTR          40
AM_STR         41
PM_STR         42
SOFTHYPHEN     43
HARDHYPHEN     44
OPENQUOTE      45
CLOSEQUOTE     46
YESTERDAYSTR   47
TODAYSTR       48
TOMORROWSTR    49
FUTURESTR      50
MAXSTRMSG      51
```

### 15.3 Key functions (locale.doc)

- `OpenLocale(name)` / `CloseLocale(loc)` — get a locale by name (or NULL for current user pref).
- `OpenCatalogA(loc, name, tags)` / `CloseCatalog(cat)` — load a catalog file. Tags include `OC_BuiltInLanguage`, `OC_BuiltInCodeSet`, `OC_Version`, `OC_PreferExternal`.
- `GetCatalogStr(cat, strnum, defaultstr)` — look up the localized version of `strnum`, falling back to `defaultstr` if the catalog is missing or lacks that string.
- `GetLocaleStr(loc, stringNum)` — look up a standard constant.
- `FormatDate(loc, format, clockdata, hook)` — format a date with a printf-like format string (using `%d`, `%m`, `%Y`, etc., honoring locale).
- `FormatString(loc, format, args, hook)` — printf-like formatting.
- `StrConvert(loc, string, buffer, len, type)` — locale-specific string conversion.
- `IsXXX(loc, char)` family — `IsAlpha`, `IsSpace`, `IsDigit`, etc., respecting the locale's charset.
- `StrnCmp(loc, a, b, n, type)` — locale-respecting collation.

### 15.4 Catalog files

Catalogs are built with the `CatComp` tool from a `.cd` (catalog description) file that lists numeric IDs and default English strings. Per-language translations are stored under `LOCALE:Catalogs/<language>/<appname>.catalog` in a binary format. Apps call `OpenCatalog` with NULL locale to get the user's preferred language, then `GetCatalogStr(cat, MSG_OK, "OK")` for each string.

Brief — the catalog format is documented in `catalog.h` if deeper modelling is needed. For an emulator, locale.library is again pure host-side — if file I/O works, catalog loading works.

---

<a name="utility"></a>
## 16. utility.library V36+

`utility.library` is the "helpers that every V36+ library needs" grab-bag. It has three main areas: tag-list processing, 32/64-bit math helpers, and hook invocation. It is opened automatically by Intuition and many other libraries; applications open it via `OpenLibrary("utility.library", 36)`.

### 16.1 Tag item processing

The core of utility: routines for walking, filtering, and manipulating `struct TagItem` lists. From `utility.doc`:

#### `FindTagItem(tagVal, taglist)`

```c
struct TagItem *FindTagItem(Tag tagVal, const struct TagItem *tags);
```

Walks `tags`, handling `TAG_MORE`/`TAG_SKIP`/`TAG_IGNORE`, and returns a pointer to the first `TagItem` whose `ti_Tag == tagVal`, or NULL.

#### `GetTagData(tagVal, defaultVal, taglist)`

```c
ULONG GetTagData(Tag tagVal, ULONG defaultValue, const struct TagItem *tags);
```

Convenience: `FindTagItem` + extract `ti_Data`, returning `defaultValue` if not found. This is how most tag consumers pull specific values without dealing with the full list.

#### `NextTagItem(&tagptr)`

```c
struct TagItem *NextTagItem(struct TagItem **tagListPtr);
```

Iterator. Call repeatedly until it returns NULL. Advances past `TAG_IGNORE`, follows `TAG_MORE`, terminates on `TAG_DONE`. This is the canonical loop:

```c
struct TagItem *ti, *tstate = taglist;
while ((ti = NextTagItem(&tstate))) {
    switch (ti->ti_Tag) {
    case MY_Foo: handle_foo(ti->ti_Data); break;
    ...
    }
}
```

#### `TagInArray(tag, array)`

```c
BOOL TagInArray(Tag tag, Tag *tagArray);
```

Returns TRUE if `tag` is in the `~0`-terminated array `tagArray`. Used for filter pattern matching.

#### `FilterTagChanges(changeList, origList, apply)`

```c
VOID FilterTagChanges(struct TagItem *changeList, struct TagItem *origList, BOOL apply);
```

Compares two tag lists. Entries in `changeList` that already match `origList` are turned into `TAG_IGNORE` (so you only process actual deltas). If `apply` is TRUE, `origList` is updated with the new values.

#### `FilterTagItems(taglist, filter, logic)`

```c
ULONG FilterTagItems(struct TagItem *taglist, const Tag *filter, ULONG logic);
```

Removes tags from `taglist` according to `filter` and `logic`:
- `TAGFILTER_AND` — keep only tags in `filter`
- `TAGFILTER_NOT` — remove tags in `filter`

#### `MapTags(taglist, map, logic)`

```c
VOID MapTags(struct TagItem *taglist, const struct TagItem *map, ULONG logic);
```

Rewrites `ti_Tag` values in `taglist` by looking them up in `map` (a list of `src_tag -> dst_tag` pairs). `logic` is `MAP_REMOVE_NOT_FOUND` or `MAP_KEEP_NOT_FOUND`. This is exactly the mechanism `ICA_MAP` uses — it is implemented internally as a `MapTags` call.

#### `RefreshTagItemClones(clone, original)`

For state-sharing after changes to the original.

#### `CloneTagItems(taglist)` / `AllocateTagItems(numItems)` / `FreeTagItems(taglist)`

```c
struct TagItem *AllocateTagItems(ULONG numItems);
struct TagItem *CloneTagItems(const struct TagItem *taglist);
VOID FreeTagItems(struct TagItem *taglist);
```

Dynamic tag-list allocation. `CloneTagItems` is a deep copy of the list structure (though not of any pointed-to data). Use `FreeTagItems` to release them; they come from the public memory pool with a known header.

#### `PackBoolTags(initialFlags, taglist, map)` / `PackStructureTags` / `UnpackStructureTags`

`PackBoolTags` takes a tag list of booleans and a `TagItem[]` map of `(boolTag -> flagBit)` pairs, returning a new UWORD/ULONG flag set. Useful for turning a tag list into a packed `WFLG_*`-style mask.

`PackStructureTags` and `UnpackStructureTags` (V39+) do the equivalent for whole structures — each map entry says "this tag goes at offset N in the struct, as an N-bit field, as a byte/word/long".

### 16.2 Hook invocation — `CallHookA` and `CallHookPkt`

```c
ULONG CallHookA(struct Hook *hook, APTR object, APTR paramMsg);
ULONG CallHookPkt(struct Hook *hook, APTR object, APTR paramMsg);
```

Both invoke the hook's `h_Entry` function with the standard `(hook, object, msg)` register parameters (A0, A2, A1) and return the result. `CallHookPkt` is identical to `CallHookA` but explicit about the name.

A `Hook` is utility's generalization of a callback — see `utility/hooks.h`:

```c
struct Hook {
    struct MinNode h_MinNode;
    ULONG (*h_Entry)();     /* assembly entry point */
    ULONG (*h_SubEntry)();  /* high-level callback */
    APTR  h_Data;
};
```

The `h_Entry` stub pushes the arguments onto the C stack in C order (hook, object, msg) then calls `h_SubEntry`. For SAS C and GCC with registerized parameters, you can point `h_Entry` directly at a function declared with `__saveds` + `__asm` annotations for A0/A2/A1.

Hooks appear throughout: `WA_BackFill`, `ASLFR_FilterFunc`, `GTLV_CallBack`, `EntryHandler`/`ExitHandler` for iffparse, BOOPSI class dispatchers (which are hooks in `cl_Dispatcher`), and all the `*Hook` tags in various libraries.

### 16.3 Named-object database — `AddNamedObject`, `FindNamedObject`

```c
struct NamedObject *AllocNamedObjectA(STRPTR name, struct TagItem *tags);
LONG AddNamedObject(struct NamedObjectList *list, struct NamedObject *obj);
struct NamedObject *FindNamedObject(struct NamedObjectList *list, STRPTR name, struct NamedObject *startNode);
LONG RemoveNamedObject(struct NamedObjectList *list, struct NamedObject *obj);
VOID FreeNamedObject(struct NamedObject *obj);
VOID NamedObjectName(struct NamedObject *obj, STRPTR buf, ULONG bufsize);
```

A dictionary service: register named objects (anything, identified by a UBYTE string) on system or private lists, and look them up. Used by e.g. `imageclass`'s sysiclass implementation and by locale catalogs. `list == NULL` selects the system-wide default list.

### 16.4 String helpers

| Function | Purpose |
|---|---|
| `Stricmp(a, b)` | case-insensitive strcmp |
| `Strnicmp(a, b, n)` | case-insensitive strncmp up to n chars |
| `ToUpper(c)` | uppercase a UBYTE |
| `ToLower(c)` | lowercase a UBYTE |
| `Amiga2Date(seconds, clockdata)` | convert seconds-since-Jan-1-1978 to `struct ClockData` |
| `Date2Amiga(clockdata)` | inverse |
| `CheckDate(clockdata)` | days-in-month validation |

`ClockData` is in `utility/date.h`:

```c
struct ClockData {
    UWORD sec, min, hour, mday, month, year;
    UWORD wday;     /* day-of-week 0..6 */
};
```

### 16.5 32/64-bit math helpers

For compilers without 64-bit support, utility provides 32×32→64 and 64÷32 primitives:

| Function | Description |
|---|---|
| `SMult32(a, b)` | signed 32×32 → 32 (returns low half) |
| `UMult32(a, b)` | unsigned 32×32 → 32 |
| `SMult64(a, b)` | signed 32×32 → 64 (D0:D1) |
| `UMult64(a, b)` | unsigned 32×32 → 64 |
| `SDivMod32(dividend, divisor)` | signed 32÷32, returns quotient and remainder |
| `UDivMod32(dividend, divisor)` | unsigned |
| `SDivMod64(dividendHi, dividendLo, divisor)` | 64÷32 |
| `UDivMod64(dividendHi, dividendLo, divisor)` | |

These are written in 68000 assembly and are noticeably faster than the compiler's generic helpers on a plain 68000. On 68020+, the CPU's native `mulu.l`/`divu.l` are used where available.

### 16.6 Complete function index

The V45 autodoc for utility.library lists the following entry points (from `utility.doc`):

- **Tags**: `FindTagItem`, `GetTagData`, `PackBoolTags`, `NextTagItem`, `FilterTagChanges`, `FilterTagItems`, `MapTags`, `AllocateTagItems`, `CloneTagItems`, `FreeTagItems`, `RefreshTagItemClones`, `TagInArray`, `PackStructureTags`, `UnpackStructureTags`
- **Hooks**: `CallHookA`, `CallHookPkt`
- **Named objects**: `AllocNamedObjectA`, `AddNamedObject`, `FindNamedObject`, `RemoveNamedObject`, `FreeNamedObject`, `NamedObjectName`
- **Strings**: `Stricmp`, `Strnicmp`, `ToUpper`, `ToLower`
- **Date**: `Amiga2Date`, `Date2Amiga`, `CheckDate`, `GetUniqueID`
- **Math**: `SDivMod32`, `SMult32`, `SMult64`, `UDivMod32`, `UMult32`, `UMult64`, `SDivMod64`, `UDivMod64`, `SMult64To32`, `UMult64To32`

`GetUniqueID()` returns a monotonically increasing ID, useful for generating unique identifiers within a session.

---

<a name="amigalib"></a>
## 17. amiga.lib — the link-library glue

`amiga.lib` is not a shared library — it is a **link-time static library**, used at compile time to add helper routines that either (a) wrap awkward kernel APIs into something easier to use, (b) provide inline C equivalents to assembly-only operations, or (c) contain startup glue. It is part of the NDK, not the OS. Every SAS C / Aztec C / GCC build of an Amiga program links against it.

The V45 autodoc `amiga_lib.doc` documents 2132 lines' worth of entries. The most important categories:

### 17.1 Process / task creation wrappers

The raw `exec.library` calls take many parameters:

- `CreatePort(name, priority)` → `struct MsgPort *` — allocate signal bit, set up the port, optionally `AddPort` it.
- `DeletePort(mp)` — inverse.
- `CreateMsgPort()` / `DeleteMsgPort()` — V36+ synonyms living in amiga.lib (the underlying exec calls are also present in exec.library from V37).
- `CreateExtIO(mp, size)` / `DeleteExtIO(io)` — allocate/free a `struct IORequest` of the given size, linked to message port `mp`.
- `CreateStdIO(mp)` / `DeleteStdIO(io)` — shortcut for a standard `IOStdReq`.
- `CreateTask(name, pri, initpc, stacksize)` / `DeleteTask(task)` — allocate a Task + stack, initialize it, `AddTask()` it.

These are thin wrappers around `AllocMem`/`AllocSignal`/`AddPort` (etc.). They save the caller from repeating the 6–8 lines of boilerplate.

### 17.2 I/O glue

- `BeginIO(io)` — inline wrapper that calls `io->io_Device->dd_BeginIO` — saves having to write the offset calculation in C.
- `DoIO(io)` / `SendIO(io)` / `AbortIO(io)` / `WaitIO(io)` / `CheckIO(io)` — already in exec, but the `DoPkt` variants live here.

### 17.3 Lists

- `NewList(list)` — inline initializer for an exec `struct List` (sets up the empty-list sentinel — `list->lh_Head = (struct Node *)&list->lh_Tail`, etc.). Used ubiquitously.

### 17.4 Random numbers

- `FastRand(seed)` — fast low-quality LCG, returns new seed.
- `RangeRand(maxvalue)` — returns `0 <= n < maxvalue` from an internal state.

### 17.5 BOOPSI dispatch helpers

These are the application-level wrappers for `DoMethod`:

- `DoMethodA(obj, msg)` — look up `OCLASS(obj)->cl_Dispatcher` and invoke.
- `DoMethod(obj, methodID, ...)` — varargs.
- `DoSuperMethodA(cl, obj, msg)` / `DoSuperMethod(cl, obj, methodID, ...)`
- `CoerceMethodA(cl, obj, msg)` / `CoerceMethod(cl, obj, methodID, ...)`
- `HookEntry(hook, obj, msg)` — generic assembly-to-C adapter for Hook dispatchers. Point `h_Entry` at this and `h_SubEntry` at your C function.
- `NewObject(cl, clsID, tag1, ...)` — varargs wrapper for the actual library call.
- `SetAttrs(obj, tag1, ...)` — varargs wrapper.
- `SetSuperAttrsA(cl, obj, taglist)` — for class implementors, pass a taglist to the superclass's OM_SET.

### 17.6 Time / TOF

- `AddTOF(interrupt, routine, data)` — add a "top-of-frame" interrupt routine (VBLANK IRQ).
- `RemTOF(interrupt)` — remove it.
- `TimeDelay(unit, secs, micros)` — synchronous sleep using timer.device.

### 17.7 Hook helpers

- `HookEntry` — the asm-to-C glue for hook dispatchers. Standard usage:

```c
struct Hook myhook;
myhook.h_Entry    = (ULONG (*)())HookEntry;
myhook.h_SubEntry = (ULONG (*)())my_c_function;
myhook.h_Data     = userdata;
```

Then `my_c_function` can use normal C parameter order `(hook, object, msg)`.

### 17.8 Auto-init magic

For tools that are launched from Workbench or that need resident-style initialization, amiga.lib contains startup stubs (`_main` variants) that do the "wait for WBStartup message if `pr_CLI == 0`" dance described in §18. Different C compilers use slightly different startup files, but they all pull in the same amiga.lib helpers.

### 17.9 The amiga.lib autodoc is the canonical list

Every entry is documented in `Documentation/Autodocs/amiga_lib.doc`. Notable entries not already covered:

- `DoPkt`, `DoPkt0..4` — synchronous DOS packet send (legacy).
- `ArgArrayInit`, `ArgArrayDone`, `ArgInt`, `ArgString` — Workbench tool-type argument parsing (wrappers around `FindToolType` that present a unified view of CLI arguments and Workbench tool types).
- `printf`/`vprintf`-style helpers under the raw-console namespace (`KPrintF`, `KPutChar`, ...) for debugging.
- `NewList`, `AddToExecList`, `RemoveFromExecList`.
- `BltBitMap` (wrapper with correct minterms).
- `CreatePool`/`DeletePool`/`AllocPooled`/`FreePooled` — pre-V39 back-ports of the exec memory pool API.
- `printf(format, ...)` calling `VPrintf` in dos.library — allows C programs to use printf without a full stdio library.

Most of amiga.lib is trivial enough to read and reimplement when porting. For an emulator, amiga.lib is **application-side** — it is statically linked into every app and you don't need to model it. You only need to make sure the libraries it calls (exec, dos, intuition, graphics) behave correctly.

---

<a name="startup"></a>
## 18. Startup flow — how your `main()` gets called

This section ties the pieces together. When the user double-clicks your program's icon on the Workbench (or types its name at the shell), what actually happens from the moment the tool is loaded to the moment your `main(argc, argv)` runs?

### 18.1 Shell launch (CLI)

1. Shell parses the command line and finds your tool.
2. Shell (via `LoadSeg()`) loads your executable into memory. `LoadSeg()` reads the hunk file format (see `amiga-dos-filesystem-disk.md`), resolves `HUNK_RELOC32` records, and returns a BPTR to the first hunk.
3. Shell calls `RunCommand(segment, stacksize, cmdline, cmdlen)`. `RunCommand` arranges for the tool to run in the shell's **own** process, not a new one — it saves the shell's state, replaces the segment, and calls the tool's first hunk as a subroutine. (This is why a badly-written shell command that clobbers registers can crash the shell.)
4. The first hunk begins with the compiler's startup glue — typically a small assembly stub that:
   - Saves A6 = SysBase.
   - Gets the Process pointer: `A4 = FindTask(NULL)`.
   - Tests `pr_CLI`. If non-zero, this is a CLI launch. It uses `pr_Arguments` and `pr_CLI` to set up `argc` / `argv` the C way. (`pr_Arguments` is a BSTR with the command tail; the startup parses it into tokens.)
   - Calls `main(argc, argv)`.
5. When `main` returns, startup returns `rc` to the shell. Shell cleans up. The segment was loaded by the shell; the shell unloads it.

### 18.2 Workbench launch

1. User double-clicks the tool icon (or double-clicks a project icon, which resolves via `do_DefaultTool` to a tool, or drops a project icon onto a tool).
2. Workbench reads the `.info` file with `GetDiskObject()` and extracts `do_StackSize` (defaults to 4096 if zero), and `do_ToolTypes`.
3. Workbench builds a `WBStartup` (`AllocMem`) and fills:
   - `sm_Message.mn_ReplyPort` = Workbench's own port.
   - `sm_ArgList[0]` = the tool (lock on its parent dir, name).
   - `sm_ArgList[1..N]` = selected project icons.
   - `sm_ToolWindow` = copied from `do_ToolWindow` if present.
4. Workbench calls `LoadSeg("<toolpath>")` and stores the BPTR in `sm_Segment`.
5. Workbench calls `CreateNewProcTags(NP_Entry, <tool_entry>, NP_StackSize, stack, NP_Name, toolname, NP_Input, 0, NP_Output, 0, ...)` to create a new Process. **`pr_CLI` is left NULL** — this is what marks a WB-launched process.
6. Workbench calls `PutMsg(&newproc->pr_MsgPort, &wbs->sm_Message)` and then `WaitPort(&wbreplyport)` for the eventual reply.
7. The new process starts executing the tool's first hunk. The startup glue again:
   - Gets `A4 = FindTask(NULL)`.
   - Tests `pr_CLI`. **It is zero**, so this is a WB launch.
   - Calls `WaitPort(&me->pr_MsgPort); wbs = GetMsg(&me->pr_MsgPort);` to collect the WBStartup message.
   - Optionally does `CurrentDir(wbs->sm_ArgList[0].wa_Lock)` so the tool starts in its own directory.
   - Calls `main(0, (char **)wbs)` by convention — the Amiga convention is that when `argc == 0`, `argv` is actually the WBStartup pointer.
8. When `main` returns, startup:
   - Calls `Forbid()` (to prevent the Workbench from unloading the segment before we return).
   - `ReplyMsg(&wbs->sm_Message)`.
   - Returns.
9. Workbench receives the reply, calls `UnLoadSeg(wbs->sm_Segment)`, and frees the WBStartup.

### 18.3 A conventional Workbench-aware main()

```c
#include <workbench/startup.h>
#include <clib/alib_protos.h>

int main(int argc, char **argv)
{
    struct WBStartup *wbs = NULL;

    if (argc == 0) {
        wbs = (struct WBStartup *)argv;   /* Amiga convention */
    }

    /* If from Workbench, make the tool's own directory CurrentDir */
    BPTR olddir = (BPTR)0;
    if (wbs) {
        olddir = CurrentDir(wbs->sm_ArgList[0].wa_Lock);
    }

    /* ... do work ... */

    if (wbs) {
        CurrentDir(olddir);
    }
    return 0;  /* startup does the Forbid/ReplyMsg dance */
}
```

### 18.4 Intuition event loop — the canonical idiom

Once you've opened a window, the loop you write is:

```c
struct Window *win;
/* ... OpenWindowTags ... */

BOOL done = FALSE;
ULONG winsig = 1UL << win->UserPort->mp_SigBit;

while (!done) {
    ULONG sigs = Wait(winsig | SIGBREAKF_CTRL_C);
    if (sigs & SIGBREAKF_CTRL_C) {
        done = TRUE;
    }
    if (sigs & winsig) {
        struct IntuiMessage *imsg;
        while ((imsg = (struct IntuiMessage *)GetMsg(win->UserPort))) {
            ULONG class = imsg->Class;
            UWORD code  = imsg->Code;
            UWORD qual  = imsg->Qualifier;
            APTR  iadr  = imsg->IAddress;
            WORD  mx    = imsg->MouseX;
            WORD  my    = imsg->MouseY;
            ReplyMsg((struct Message *)imsg);   /* *** copy first, reply now *** */

            switch (class) {
            case IDCMP_CLOSEWINDOW:
                done = TRUE;
                break;
            case IDCMP_REFRESHWINDOW:
                BeginRefresh(win);
                draw_contents(win);
                EndRefresh(win, TRUE);
                break;
            case IDCMP_NEWSIZE:
                reflow_content(win);
                break;
            case IDCMP_MENUPICK: {
                UWORD num = code;
                while (num != MENUNULL) {
                    struct MenuItem *item = ItemAddress(menu, num);
                    handle_menu(num);
                    num = item->NextSelect;
                }
                break;
            }
            case IDCMP_GADGETUP: {
                struct Gadget *g = (struct Gadget *)iadr;
                handle_gadget(g, code);
                break;
            }
            case IDCMP_VANILLAKEY:
                handle_key((UBYTE)code);
                break;
            }
        }
    }
}
```

Three rules never to break:

1. **Copy fields out of the IntuiMessage before ReplyMsg.** After ReplyMsg, Intuition owns the message again and may reuse it.
2. **Drain the port.** `GetMsg` in a loop until it returns NULL, because the port's signal is only re-raised when a new message arrives on an empty port.
3. **Wait on the signal.** Don't spin on `GetMsg` alone; you'll eat the CPU.

### 18.5 Shutdown order

Open in order A, B, C, D: close in order D, C, B, A. Specifically:

1. `ClearMenuStrip(win)`; `FreeMenus(menu)`.
2. `CloseWindow(win)` — after the menu strip is detached (`ClearMenuStrip` is required before `FreeMenus`; `CloseWindow` doesn't do it for you).
3. `FreeGadgets(glist)` — for GadTools gadgets.
4. `FreeVisualInfo(vi)`.
5. `UnlockPubScreen(NULL, screen)` or `CloseScreen(screen)`.
6. `CloseLibrary(base)`.

Closing libraries in the reverse order of opening them preserves dependency order. If you opened intuition before gadtools, close gadtools before intuition.

---

<a name="appendix-boopsi-tree"></a>
## Appendix A — BOOPSI class hierarchy (ASCII diagram)

Reproduced here for quick reference. Same content as §6.1 but in a single visual block:

```
                                 rootclass
                                     |
         +---------+---------+-------+-------+---------+----------+
         |         |         |               |         |          |
    imageclass  icclass   pointerclass   gadgetclass  (ReAction   other
         |         |                        |          window/    v36+
         |     modelclass                   |          requester) privates
         |                                  |
   +-----+-----+-+-----+              +-----+-----+----+------+----+
   |     |     | |     |              |     |     |    |      |    |
 frame  sys fill ite   ...          propg strg buttong group   fr   ...
 iclass iclass rect xticlass        class class class  gclass  button
       (sys-  class                                           gclass
        gadget                                                 |
        imagery)                                            (framed
                                                             button)

   Image plus images/*  (V39+):  bitmap.image, glyph.image, label.image,
                                 drawlist.image, penmap.image, bevel.image

   Gadget plus gadgets/*  (V39+):  button.gadget, string.gadget,
                                   checkbox.gadget, chooser.gadget,
                                   listbrowser.gadget, layout.gadget,
                                   clicktab.gadget, scroller.gadget,
                                   slider.gadget, radiobutton.gadget,
                                   palette.gadget, fuelgauge.gadget,
                                   gradientslider.gadget, colorwheel.gadget,
                                   getfile.gadget, getfont.gadget,
                                   getscreenmode.gadget, datebrowser.gadget,
                                   space.gadget, speedbar.gadget,
                                   texteditor.gadget, virtual.gadget,
                                   page.gadget, integer.gadget
```

The "(V39+)" classes belong to the ReAction toolkit, whose library set is documented in `reaction_lib.doc`. They live in `gadgets/*.library` on disk and are opened by name. Most are implemented as real shared libraries so that multiple programs share one copy.

### Methods in rootclass

| ID | Constant | Use |
|---|---|---|
| 0x101 | OM_NEW | object creation |
| 0x102 | OM_DISPOSE | object destruction |
| 0x103 | OM_SET | set attributes |
| 0x104 | OM_GET | query attribute |
| 0x105 | OM_ADDTAIL | add self to list |
| 0x106 | OM_REMOVE | remove self from list |
| 0x107 | OM_NOTIFY | notify dependents of change |
| 0x108 | OM_UPDATE | receive notification |
| 0x109 | OM_ADDMEMBER | add child object (container) |
| 0x10A | OM_REMMEMBER | remove child |

### Methods in imageclass

```
IM_DRAW      0x202   IM_HITTEST    0x203
IM_ERASE     0x204   IM_MOVE       0x205
IM_DRAWFRAME 0x206   IM_FRAMEBOX   0x207
IM_HITFRAME  0x208   IM_ERASEFRAME 0x209
IM_DOMAINFRAME 0x20A (V44)
```

### Methods in gadgetclass

```
GM_HITTEST    0   GM_RENDER       1
GM_GOACTIVE   2   GM_HANDLEINPUT  3
GM_GOINACTIVE 4   GM_HELPTEST     5
GM_LAYOUT     6   GM_DOMAIN       7
GM_KEYTEST    8   GM_KEYGOACTIVE  9
GM_KEYGOINACTIVE 10
```

Return values for input methods: `GMR_MEACTIVE`, `GMR_NOREUSE`, `GMR_REUSE`, `GMR_VERIFY`, `GMR_NEXTACTIVE`, `GMR_PREVACTIVE`, `GMR_KEYACTIVE`, `GMR_KEYVERIFY`.

---

<a name="appendix-function-index"></a>
## Appendix B — Function index per library

Condensed references pointing back to the doc. For exhaustive listings, consult the NDK autodoc files named after each library.

### intuition.library (V45) — `intuition.doc`, 7754 lines

Screens: `OpenScreen`, `OpenScreenTagList`, `CloseScreen`, `PubScreenStatus`, `LockPubScreen`, `UnlockPubScreen`, `LockPubScreenList`, `UnlockPubScreenList`, `SetDefaultPubScreen`, `GetDefaultPubScreen`, `NextPubScreen`, `SetPubScreenModes`, `GetScreenData`, `GetScreenDrawInfo`, `FreeScreenDrawInfo`, `MoveScreen`, `ScreenPosition`, `ScreenDepth`, `ScreenToFront`, `ScreenToBack`, `MakeScreen`, `RemakeDisplay`, `RethinkDisplay`, `ShowTitle`, `QueryOverscan`, `ViewAddress`, `ViewPortAddress`, `WBenchToFront`, `WBenchToBack`, `OpenWorkBench`, `CloseWorkBench`, `AllocScreenBuffer`, `FreeScreenBuffer`, `ChangeScreenBuffer`.

Windows: `OpenWindow`, `OpenWindowTagList`, `CloseWindow`, `ModifyIDCMP`, `SetWindowTitles`, `ActivateWindow`, `MoveWindow`, `SizeWindow`, `ChangeWindowBox`, `ZipWindow`, `WindowToFront`, `WindowToBack`, `MoveWindowInFrontOf`, `WindowLimits`, `BeginRefresh`, `EndRefresh`, `RefreshWindowFrame`, `ReportMouse`, `SetMouseQueue`.

Gadgets: `AddGadget`, `AddGList`, `RemoveGadget`, `RemoveGList`, `ActivateGadget`, `RefreshGadgets`, `RefreshGList`, `OnGadget`, `OffGadget`, `ModifyProp`, `NewModifyProp`, `SetGadgetAttrsA`, `GadgetMouse`.

Menus: `SetMenuStrip`, `ClearMenuStrip`, `ResetMenuStrip`, `ItemAddress`, `OnMenu`, `OffMenu`, `LendMenus`.

Requesters: `InitRequester`, `Request`, `EndRequest`, `SetDMRequest`, `ClearDMRequest`, `AutoRequest`, `BuildSysRequest`, `FreeSysRequest`, `SysReqHandler`, `EasyRequestArgs`, `BuildEasyRequestArgs`.

Alerts and misc: `DisplayAlert`, `TimedDisplayAlert`, `DisplayBeep`, `DoubleClick`, `CurrentTime`, `HelpControl`.

Drawing: `PrintIText`, `IntuiTextLength`, `DrawBorder`, `DrawImage`, `DrawImageState`, `EraseImage`, `PointInImage`.

Pointers: `SetPointer`, `ClearPointer`, `SetWindowPointerA`.

BOOPSI: `NewObject`, `DisposeObject`, `SetAttrsA`, `GetAttr`, `SetGadgetAttrsA`, `DoGadgetMethodA`, `MakeClass`, `AddClass`, `RemoveClass`, `FreeClass`, `NextObject`, `ObtainGIRPort`, `ReleaseGIRPort`.

Memory/compat: `AllocRemember`, `FreeRemember`, `GetDefPrefs`, `GetPrefs`, `SetPrefs`, `LockIBase`, `UnlockIBase`, `ScrollWindowRaster`.

### graphics.library, layers.library

Covered in `amiga-graphics-display.md`. This document uses their names (`BltBitMap`, `Move`, `Draw`, `Text`, `RectFill`, `RastPort`, `ViewPort`, `BitMap`, `LoadRGB32`, `ObtainPen`, `ReleasePen`, `GetDisplayInfoData`, `ModeNotAvailable`) without redefining.

### gadtools.library — `gadtools.doc`, 1355 lines

Contexts/gadgets: `CreateContext`, `CreateGadgetA`, `CreateGadget`, `FreeGadgets`, `GT_SetGadgetAttrsA`, `GT_RefreshWindow`, `GT_BeginRefresh`, `GT_EndRefresh`, `GT_GetIMsg`, `GT_ReplyIMsg`, `GT_FilterIMsg`, `GT_PostFilterIMsg`, `GetVisualInfoA`, `FreeVisualInfo`, `DrawBevelBoxA`, `GT_GetGadgetAttrsA`.

Menus: `CreateMenusA`, `CreateMenus`, `FreeMenus`, `LayoutMenusA`, `LayoutMenus`, `LayoutMenuItemsA`, `LayoutMenuItems`.

### workbench.library — `wb.doc`, 2132 lines

`AddAppWindowA`, `AddAppIconA`, `AddAppMenuItemA`, `AddAppWindowDropZoneA` (V44), `RemoveAppWindow`, `RemoveAppIcon`, `RemoveAppMenuItem`, `RemoveAppWindowDropZone`, `WBInfo`, `OpenWorkbenchObjectA` (V44), `CloseWorkbenchObjectA` (V45), `WorkbenchControlA` (V44).

### icon.library — `icon.doc`, 1854 lines

`GetDiskObject`, `GetDiskObjectNew`, `PutDiskObject`, `FreeDiskObject`, `GetDefDiskObject`, `PutDefDiskObject`, `DeleteDiskObject`, `DupDiskObjectA` (V44), `LayoutIconA`, `AddFreeList`, `FreeFreeList`, `FindToolType`, `MatchToolValue`, `BumpRevision`, `DrawIcon`, `DrawIconStateA` (V44), `EraseIcon`, `GetIconRectangleA` (V44), `IconControlA` (V44), `MakeIconStateA` (V44), `GetIconTagList`, `PutIconTagList`.

### asl.library — `asl.doc`, 951 lines

`AllocAslRequest`, `AllocAslRequestTags`, `AllocFileRequest`, `AllocFontRequest`, `AllocScrModeRequest`, `FreeAslRequest`, `FreeFileRequest`, `FreeFontRequest`, `FreeScrModeRequest`, `AslRequest`, `AslRequestTags`, `RequestFile`, `RequestFont`.

### iffparse.library — `iffparse.doc`, 1404 lines

`AllocIFF`, `FreeIFF`, `OpenIFF`, `CloseIFF`, `InitIFF`, `InitIFFasDOS`, `InitIFFasClip`, `OpenClipboard`, `CloseClipboard`, `ParseIFF`, `PushChunk`, `PopChunk`, `ReadChunkBytes`, `ReadChunkRecords`, `WriteChunkBytes`, `WriteChunkRecords`, `CurrentChunk`, `ParentChunk`, `StopChunk`, `StopChunks`, `PropChunk`, `PropChunks`, `CollectionChunk`, `CollectionChunks`, `StopOnExit`, `EntryHandler`, `ExitHandler`, `FindProp`, `FindPropContext`, `FindCollection`, `StoreLocalItem`, `StoreItemInContext`, `FindLocalItem`, `LocalItemData`, `SetLocalItemPurge`, `GoodID`, `GoodType`, `IDtoStr`.

### commodities.library — `commodities.doc`, 966 lines

`CxBroker`, `ActivateCxObj`, `DeleteCxObj`, `DeleteCxObjAll`, `CreateCxObj`, `CxObjType`, `CxObjError`, `ClearCxObjError`, `SetCxObjPri`, `AttachCxObj`, `EnqueueCxObj`, `InsertCxObj`, `RemoveCxObj`, `SetFilter`, `SetFilterIX`, `ParseIX`, `AddIEvents`, `InvertKeyMap`, `CxMsgType`, `CxMsgData`, `CxMsgID`, `DivertCxMsg`, `RouteCxMsg`, `DisposeCxMsg`, `CopyBrokerList`, `FreeBrokerList`, `BrokerCommand`.

### utility.library — `utility.doc`, 1390 lines

See §16.6.

### locale.library — `locale.doc`, 970 lines

`OpenLocale`, `CloseLocale`, `OpenCatalogA`, `CloseCatalog`, `GetCatalogStr`, `GetLocaleStr`, `GetLocaleInfo`, `FormatDate`, `FormatString`, `ParseDate`, `StrConvert`, `StrnCmp`, `ConvToLower`, `ConvToUpper`, `IsAlpha`, `IsAlNum`, `IsCntrl`, `IsDigit`, `IsGraph`, `IsLower`, `IsPrint`, `IsPunct`, `IsSpace`, `IsUpper`, `IsXDigit`.

### datatypes.library — `datatypes.doc`, 844 lines

`ObtainDataTypeA`, `ReleaseDataType`, `NewDTObjectA`, `DisposeDTObject`, `DoDTMethodA`, `SetDTAttrsA`, `GetDTAttrsA`, `DrawDTObjectA`, `PrintDTObjectA`, `RemoveDTObject`, `CopyDTMethod`, `FindDataType`, `FindToolNode`, `LockDataType`, `UnlockDataType`, `AllocDTMethod`, `FreeDTMethod`.

### amiga.lib (link-library) — `amiga_lib.doc`, 2120 lines

See §17.

### diskfont.library — `diskfont.doc`, 407 lines

`OpenDiskFont`, `AvailFonts`, `NewFontContents`, `NewScaledDiskFont`, `DisposeFontContents`. This one is for loading disk-resident fonts by `TextAttr`. It's the counterpart to `graphics.library/OpenFont`/`CloseFont` (which only find ROM/installed fonts).

---

<a name="appendix-gaps"></a>
## Appendix C — Gaps and emulator hazards

Things this document does **not** cover in depth (either because they are niche, because the NDK source must be consulted directly, or because another of the six core docs already covers them):

### Gaps

1. **ReAction toolkit (V39+)**: `window.class`, `requester.class`, and the full suite of `gadgets/*.library` (listbrowser, chooser, texteditor, layout, speedbar, clicktab, colorwheel, datebrowser, fuelgauge, integer, gradientslider, page, palette, radiobutton, scroller, slider, space, string, virtual, getfile, getfont, getscreenmode, tapedeck, button, checkbox). Each has its own autodoc file `*_gc.doc` and its own header in `gadgets/*.h`. The infrastructure is covered here (§6, 7) — the per-class attributes are not. For a complete emulator you will need to read at least `layout_gc.doc`, `listbrowser_gc.doc`, `button_gc.doc`, `string_gc.doc`, `checkbox_gc.doc`, `chooser_gc.doc`, `clicktab_gc.doc`, and `window_cl.doc`, which are the classes most likely to be used by OS 3.5/3.9 applications.
2. **datatypes.library** internals — class-implementor methods (`DTM_NEWMEMBER`, `DTM_FRAMEBOX`, `DTM_PROCLAYOUT`, `DTM_TRIGGER`, ...), the `dtm*` message structures, and the V44+ additions. Covered only lightly here (§14).
3. **iffparse entry/exit handlers** — the ability to register a hook that runs automatically when the parser enters or leaves a chunk. Mentioned but not worked out.
4. **locale catalog binary format** — documented in the NDK `catalog.h` and `LocaleTxt*.h` headers.
5. **preferences files** — the `prefs/` headers (`palette.h`, `pointer.h`, `serial.h`, `screenmode.h`, etc.) describe on-disk prefs formats. Used by IPrefs, Printer, etc. Handled here only via `GetPrefs`/`SetPrefs` mentions.
6. **ARexx integration** — `arexx_cl.doc` + `rexxsyslib.doc`. AREXX is pervasive in OS 2.x+; any serious emulator needs it. Not touched here.
7. **Console handler interactions** — `console.device` + `CSI ESC` handling, cursor keys, mouse reporting. See `console.doc` and `amiga-io-audio-expansion.md`.
8. **amigaguide.library** — the hypertext help system. Used by help menus, `AmigaGuide()` function, integrates with `IDCMP_MENUHELP` / `IDCMP_GADGETHELP`.
9. **Input event subclasses in V39+** — `IESUBCLASS_NEWTABLET`, `IECLASS_NEWMOUSE`, `IECLASS_NEWPOINTERPOS`, tablet data routing. Commodities broker interactions with these are tricky.

### Hazards for emulator authors

- **IntuiMessage ownership.** Intuition allocates IntuiMessages from a system pool; the message is on loan to the application between `GetMsg` and `ReplyMsg`. Do not hold them across system calls that could race with Intuition (e.g. `ModifyIDCMP`). Do not free them. Do not forget to reply them — un-replied messages are a leak and eventually cause Intuition to stop delivering new events.
- **The IDCMP port's life cycle.** `ModifyIDCMP(win, 0)` tears down the port's intuition-side. If the UserPort is shared between multiple windows (you set it yourself after `OpenWindow` with IDCMP 0), you must ensure no messages destined for the soon-to-close window are still queued. The correct sequence is: `ModifyIDCMP(win, 0)`, then drain any remaining messages, then `CloseWindow`.
- **WFLG_RMBTRAP and menu state.** If `WFLG_RMBTRAP` is set, RMB events go to the application as `IDCMP_MOUSEBUTTONS` instead of activating the menu. The window is still menu-capable (`SetMenuStrip` still works) but only keyboard shortcuts trigger menus.
- **Requester refresh races.** When a requester closes, Intuition sends `IDCMP_REQCLEAR` and then may send `IDCMP_REFRESHWINDOW`. The refresh is required if your window is simple-refresh. Handle both.
- **Shared public screens.** When a program opens a visitor window on another program's public screen, closing the screen is blocked by the visitor count. Make sure your emulator's `CloseScreen` respects `psn_VisitorCount` and signals the owning task on reaching zero.
- **Workbench process is not optional.** Some programs (e.g. Magic Workbench, ToolManager, anything using `AddAppIcon`) fail cleanly if workbench.library isn't running a real Workbench process. Just having the library is not enough.
- **input.device priority chain order** is observable by applications via `input.device/AddInputHandler`. Commodities broker and Intuition live there; respect the priorities (51 and 50) and the chain semantics.
- **Layer damage lists and `BeginRefresh`.** Calling `BeginRefresh` on a window without pending damage is a no-op but does update internal layer state; calling it when there is no `IDCMP_REFRESHWINDOW` pending is incorrect and leads to drift between the window's `Flags.WFLG_WINDOWREFRESH` and the actual layer state.
- **`do_CurrentX == NO_ICON_POSITION` (0x80000000)** is the unsnapshotted sentinel. Implementing `PutDiskObject` with `ICONPUTA_OnlyUpdatePosition` must not mistake this for a valid coordinate.
- **TextAttr lifetime.** When you pass a `TextAttr *` to `OpenScreen` / `OpenWindow` / `IntuiText`, Intuition does not copy it. The `TextAttr` and the string it points to must remain valid for as long as the screen/window/menu is open.
- **`struct Screen.BitMap` is an embedded legacy slot.** Always use `Screen->RastPort.BitMap`. Under V39 the embedded `BitMap` may have fewer bitplane slots than the true BitMap.

---

<a name="appendix-source-map"></a>
## Appendix D — Source map

All authoritative paths under `/Users/stevehill/Desktop/AmigaPDFs/ndk/NDK_3.9/`.

### Autodocs (Documentation/Autodocs/)

| File | Lines | Topic |
|---|---|---|
| `intuition.doc` | 7754 | intuition.library V45 — screens, windows, gadgets, menus, BOOPSI app API |
| `graphics.doc` | — | graphics.library (see `amiga-graphics-display.md`) |
| `layers.doc` | — | layers.library |
| `gadtools.doc` | 1355 | GadTools V37+ |
| `wb.doc` | 2132 | workbench.library |
| `icon.doc` | 1854 | icon.library |
| `commodities.doc` | 966 | input broker / Cx objects |
| `asl.doc` | 951 | file/font/screenmode requesters |
| `iffparse.doc` | 1404 | IFF parsing |
| `datatypes.doc` | 844 | datatypes V39+ |
| `locale.doc` | 970 | locale.library V38+ |
| `utility.doc` | 1390 | tag-list and math helpers |
| `diskfont.doc` | 407 | disk-font loading |
| `amiga_lib.doc` | 2120 | amiga.lib helpers |
| `reaction_lib.doc` | 532 | ReAction toolkit |
| `window_cl.doc` | — | window.class |
| `*_gc.doc` | — | per-gadget-class docs (button, checkbox, chooser, clicktab, colorwheel, datebrowser, fuelgauge, getfile, getfont, getscreenmode, gradientslider, integer, layout, listbrowser, page, palette, radiobutton, scroller, slider, space, speedbar, string, texteditor, virtual) |
| `*_ic.doc` | — | per-image-class docs (bevel, bitmap, drawlist, glyph, label, penmap) |
| `picture_dtc.doc` / `sound_dtc.doc` / `animation_dtc.doc` / `text_dtc.doc` / `amigaguide_dtc.doc` | — | per-datatype classes |
| `arexx_cl.doc` | — | arexx.class |

### Headers (Include/include_h/)

| Path | Purpose |
|---|---|
| `intuition/intuition.h` | Main intuition structures (Window, Gadget, IntuiMessage, Menu, MenuItem, Requester, Image, Border, IntuiText, PropInfo, StringInfo, BoolInfo, NewWindow, ExtNewWindow) and all WA_* tags, IDCMP_* classes, RAWMOUSE codes |
| `intuition/screens.h` | Screen, NewScreen, ExtNewScreen, DrawInfo, PubScreenNode, ScreenBuffer, SA_* tags, OSERR_* codes |
| `intuition/classes.h` | IClass, `_Object`, INST_DATA macro, ClassLibrary |
| `intuition/classusr.h` | ROOTCLASS..POINTERCLASS ID strings, OM_* methods, opSet/opUpdate/opGet/opMember/opAddTail |
| `intuition/gadgetclass.h` | GA_*/PGA_*/STRINGA_* tags, GM_* methods, gp* message structs, propgclass/strgclass/buttongclass/groupgclass attrs |
| `intuition/imageclass.h` | IA_*/SYSIA_* tags, IM_* methods, SYSISIZE_*/DEPTHIMAGE..AMIGAKEY constants, imp* message structs, IDS_* draw states, FRAME_* types |
| `intuition/icclass.h` | ICA_TARGET, ICA_MAP, ICTARGET_IDCMP, ICSPECIAL_CODE, ICM_* |
| `intuition/pointerclass.h` | POINTERA_* tags for pointerclass (V39) |
| `intuition/preferences.h` | legacy preferences struct for GetPrefs/SetPrefs |
| `intuition/cghooks.h` | custom-gadget hook-parameter structs |
| `intuition/sghooks.h` | string gadget edit-hook parameter struct (SGWork, StringExtend) |
| `intuition/intuitionbase.h` | IntuitionBase layout (mostly private) |
| `intuition/iobsolete.h` | backward-compatibility aliases |
| `libraries/gadtools.h` | NewGadget, NewMenu, GT_* tags, *_KIND constants, *_IDCMP masks |
| `libraries/iffparse.h` | IFFHandle, ContextNode, LocalContextItem, StoredProperty, CollectionItem, ClipboardHandle, ID_*, IFFERR_* |
| `libraries/commodities.h` | NewBroker, InputXpression, CxObj/CxMsg, CX_*, CXM_*, CXCMD_*, CBERR_* |
| `libraries/locale.h` | Locale, LocaleBase, DAY_/MON_/YESSTR/etc., MS_ISO/MS_AMERICAN |
| `libraries/asl.h` | FileRequester, FontRequester, ScreenModeRequester, DisplayMode, ASLFR_*/ASLFO_*/ASLSM_* tags |
| `libraries/dos.h` and `dos/dosextens.h` | used by §12; see amiga-dos-filesystem-disk.md |
| `utility/tagitem.h` | TagItem, TAG_DONE/TAG_IGNORE/TAG_MORE/TAG_SKIP, TAG_USER, TAGFILTER_*, MAP_* |
| `utility/hooks.h` | Hook struct, HOOKFUNC, register conventions |
| `utility/date.h` | ClockData |
| `utility/name.h` / `utility/pack.h` | named-object lookup, PackStructureTags support |
| `workbench/workbench.h` | DiskObject, DrawerData, FreeList, AppMessage, WBA_*/WBAPPICONA_*/WBAPPMENUA_*/WBOPENA_*/WBCTRLA_*/WBDZA_*, WBDISK..WBAPPICON types, AMTYPE_* classes |
| `workbench/startup.h` | WBStartup, WBArg |
| `workbench/icon.h` | ICONA_* tags for V44+ icon.library |
| `classes/window.h` / `requester.h` / `arexx.h` | V44 ReAction class attrs |
| `gadgets/*.h` | V39+ ReAction gadget attrs |
| `images/*.h` | V39+ ReAction image attrs |

### 3rd-edition ROM Kernel Manuals (txt files)

- `rkm/txt/Commodore_Amiga_Tech_Ref_Series_Amiga_ROM_Kernel_Reference_Manual_Libraries_3rd_edition.txt` — the V37 book, chapters on Intuition (screens, windows, gadgets, menus, requesters, input/output, IDCMP), BOOPSI ("Basic Object-Oriented Programming System for Intuition"), GadTools, ASL, IFFParse, Workbench. This is the prose reference for things the autodocs only list.
- `rkm/txt/Amiga_ROM_Kernel_Reference_Manual_Exec.txt` — used by the boot / process / messaging discussion (see `amiga-exec-kernel.md`).
- `rkm/txt/Amiga_ROM_Kernel_Reference_Manual_Libraries_and_Devices.txt` — V1.3 book; of historical interest for seeing what was present before the V36 additions.

### Existing sibling documents in `/Users/stevehill/Desktop/AmigaPDFs/`

- `amiga-boot-process.md` — bootstrap, ROMTag scan, Kickstart, LoadWB
- `amiga-exec-kernel.md` — Task/Process, MsgPort, Signal, Library base/JumpTable, ROMTag
- `amiga-dos-filesystem-disk.md` — dos.library, Lock/BPTR, filesystems, hunk format, LoadSeg
- `amiga-graphics-display.md` — copper, bitplanes, blitter, graphics.library primitives, ViewPort/RastPort/BitMap, basic Intuition screen/window creation
- `amiga-io-audio-expansion.md` — Paula, audio.device, serial/parallel, trackdisk, AutoConfig, expansion.library
- `amiga-hardware-reference.md` — custom chip registers, CIA/Agnus/Denise/Paula, video timing

This document is the seventh and is meant to slot in above `amiga-graphics-display.md` in the conceptual stack — where the graphics doc leaves off at "you have a RastPort and a BitMap", this one picks up with "you have an Intuition window, a BOOPSI gadget, and a GadTools menu".

---

*End of document.*
