//! Library Vector Offset (LVO) lookup tables for well-known Amiga
//! ROM libraries.
//!
//! Every Exec library is called through a jump table sitting at
//! *negative* offsets from the library base in A6. KS source uses
//! `LIB_VECTSIZE = 6` (one jmp.l instruction per slot), so offsets
//! are always negative multiples of 6.
//!
//! Disassembly of any ROM hits patterns like `jsr -318(a6)` constantly.
//! Without this table that looks like noise; with it, it resolves to
//! `Wait` and the trace becomes readable.
//!
//! **Source of truth.** Every entry below is parsed from the NDK 3.2
//! `Include_I/lvo/*.i` files shipped by Commodore, mirrored at
//! `~/Projects/198x/reference/by-system/commodore-amiga/ndk/ndk-3.2/`.
//! Do NOT hand-edit these tables — regenerate from the NDK source if
//! the schema changes (the in-repo script
//! `tools/lvo-from-ndk.py` does the conversion). Hand-edited offsets
//! drifted on first attempt; the canonical NDK is the only safe source.
//!
//! **Convention.** Every library inherits Open / Close / Expunge /
//! Reserved at the first four slots (`-6`, `-12`, `-18`, `-24`),
//! before library-specific functions begin at `-30`. These four are
//! the `struct Library` callbacks used by `OpenLibrary` to instantiate
//! a library instance — they're identical in role across every
//! library, just bound to different code per library.
//!
//! **Name collision in dos.library.** Beware: `dos.library` defines a
//! file-oriented `Open()` at `-30` that is NOT the same function as
//! the library-init `Open` at `-6`. They share a name; the disambiguator
//! is the offset. `Open(-6)` opens the dos.library *itself*; `Open(-30)`
//! opens a file path. The resolver returns the name at the offset; the
//! caller is responsible for context.
//!
//! Coverage as of NDK 3.2:
//!
//!   * `exec.library`      — 115 entries + 4 inherited (Open/Close/etc.)
//!   * `dos.library`       — 161 entries + 4 inherited
//!   * `intuition.library` — 127 entries + 4 inherited
//!   * `graphics.library`  — 163 entries + 4 inherited
//!   * `cia.resource`      — 4 entries (no inherited prefix; it's a
//!     `struct Resource`, not a `struct Library`)

/// All known libraries this resolver answers for. Keep names in the
/// canonical Exec form (lowercase, trailing `.library` / `.resource`)
/// — `FindName` matches case-sensitive at runtime.
pub(crate) const LIBRARY_NAMES: &[&str] = &[
    "exec.library",
    "dos.library",
    "intuition.library",
    "graphics.library",
    "cia.resource",
];

/// Return the LVO table for a library by canonical name. `None` if we
/// don't have a table for that library yet.
pub(crate) fn lvo_table(library: &str) -> Option<&'static [(i32, &'static str)]> {
    match library {
        "exec.library" => Some(EXEC_LIBRARY),
        "dos.library" => Some(DOS_LIBRARY),
        "intuition.library" => Some(INTUITION_LIBRARY),
        "graphics.library" => Some(GRAPHICS_LIBRARY),
        "cia.resource" => Some(CIA_RESOURCE),
        _ => None,
    }
}

/// Resolve one (library, offset) pair to a function name. Accepts
/// either form of offset (`-318` or `318`) so callers don't have to
/// re-sign-extend a value pulled out of a disassembly. `None` if the
/// library is unknown or the offset isn't in the table.
pub(crate) fn resolve(library: &str, offset: i32) -> Option<&'static str> {
    let table = lvo_table(library)?;
    let negative = if offset > 0 { -offset } else { offset };
    table
        .iter()
        .find_map(|(off, name)| (*off == negative).then_some(*name))
}

// Generated from NDK 3.2 Include_I/lvo/exec_lib.i — 115 entries +
// 4 inherited Library slots. Do not hand-edit; regenerate via
// `tools/lvo-from-ndk.py`.
const EXEC_LIBRARY: &[(i32, &str)] = &[
    // Inherited from struct Library
    (-6, "Open"),
    (-12, "Close"),
    (-18, "Expunge"),
    (-24, "Reserved"),
    // exec-specific
    (-30, "Supervisor"),
    (-72, "InitCode"),
    (-78, "InitStruct"),
    (-84, "MakeLibrary"),
    (-90, "MakeFunctions"),
    (-96, "FindResident"),
    (-102, "InitResident"),
    (-108, "Alert"),
    (-114, "Debug"),
    (-120, "Disable"),
    (-126, "Enable"),
    (-132, "Forbid"),
    (-138, "Permit"),
    (-144, "SetSR"),
    (-150, "SuperState"),
    (-156, "UserState"),
    (-162, "SetIntVector"),
    (-168, "AddIntServer"),
    (-174, "RemIntServer"),
    (-180, "Cause"),
    (-186, "Allocate"),
    (-192, "Deallocate"),
    (-198, "AllocMem"),
    (-204, "AllocAbs"),
    (-210, "FreeMem"),
    (-216, "AvailMem"),
    (-222, "AllocEntry"),
    (-228, "FreeEntry"),
    (-234, "Insert"),
    (-240, "AddHead"),
    (-246, "AddTail"),
    (-252, "Remove"),
    (-258, "RemHead"),
    (-264, "RemTail"),
    (-270, "Enqueue"),
    (-276, "FindName"),
    (-282, "AddTask"),
    (-288, "RemTask"),
    (-294, "FindTask"),
    (-300, "SetTaskPri"),
    (-306, "SetSignal"),
    (-312, "SetExcept"),
    (-318, "Wait"),
    (-324, "Signal"),
    (-330, "AllocSignal"),
    (-336, "FreeSignal"),
    (-342, "AllocTrap"),
    (-348, "FreeTrap"),
    (-354, "AddPort"),
    (-360, "RemPort"),
    (-366, "PutMsg"),
    (-372, "GetMsg"),
    (-378, "ReplyMsg"),
    (-384, "WaitPort"),
    (-390, "FindPort"),
    (-396, "AddLibrary"),
    (-402, "RemLibrary"),
    (-408, "OldOpenLibrary"),
    (-414, "CloseLibrary"),
    (-420, "SetFunction"),
    (-426, "SumLibrary"),
    (-432, "AddDevice"),
    (-438, "RemDevice"),
    (-444, "OpenDevice"),
    (-450, "CloseDevice"),
    (-456, "DoIO"),
    (-462, "SendIO"),
    (-468, "CheckIO"),
    (-474, "WaitIO"),
    (-480, "AbortIO"),
    (-486, "AddResource"),
    (-492, "RemResource"),
    (-498, "OpenResource"),
    (-522, "RawDoFmt"),
    (-528, "GetCC"),
    (-534, "TypeOfMem"),
    (-540, "Procure"),
    (-546, "Vacate"),
    (-552, "OpenLibrary"),
    (-558, "InitSemaphore"),
    (-564, "ObtainSemaphore"),
    (-570, "ReleaseSemaphore"),
    (-576, "AttemptSemaphore"),
    (-582, "ObtainSemaphoreList"),
    (-588, "ReleaseSemaphoreList"),
    (-594, "FindSemaphore"),
    (-600, "AddSemaphore"),
    (-606, "RemSemaphore"),
    (-612, "SumKickData"),
    (-618, "AddMemList"),
    (-624, "CopyMem"),
    (-630, "CopyMemQuick"),
    (-636, "CacheClearU"),
    (-642, "CacheClearE"),
    (-648, "CacheControl"),
    (-654, "CreateIORequest"),
    (-660, "DeleteIORequest"),
    (-666, "CreateMsgPort"),
    (-672, "DeleteMsgPort"),
    (-678, "ObtainSemaphoreShared"),
    (-684, "AllocVec"),
    (-690, "FreeVec"),
    (-696, "CreatePool"),
    (-702, "DeletePool"),
    (-708, "AllocPooled"),
    (-714, "FreePooled"),
    (-720, "AttemptSemaphoreShared"),
    (-726, "ColdReboot"),
    (-732, "StackSwap"),
    (-762, "CachePreDMA"),
    (-768, "CachePostDMA"),
    (-774, "AddMemHandler"),
    (-780, "RemMemHandler"),
    (-786, "ObtainQuickVector"),
    (-828, "NewMinList"),
];

// Generated from NDK 3.2 Include_I/lvo/dos_lib.i — 161 entries +
// 4 inherited Library slots. NOTE: dos.library defines its own
// file-oriented `Open` at -30, distinct from the standard library
// `Open` at -6 — same name, different function.
const DOS_LIBRARY: &[(i32, &str)] = &[
    // Inherited from struct Library
    (-6, "Open"),
    (-12, "Close"),
    (-18, "Expunge"),
    (-24, "Reserved"),
    // dos-specific (Open/Close shadow standard names — see disambiguation note)
    (-30, "Open"),
    (-36, "Close"),
    (-42, "Read"),
    (-48, "Write"),
    (-54, "Input"),
    (-60, "Output"),
    (-66, "Seek"),
    (-72, "DeleteFile"),
    (-78, "Rename"),
    (-84, "Lock"),
    (-90, "UnLock"),
    (-96, "DupLock"),
    (-102, "Examine"),
    (-108, "ExNext"),
    (-114, "Info"),
    (-120, "CreateDir"),
    (-126, "CurrentDir"),
    (-132, "IoErr"),
    (-138, "CreateProc"),
    (-144, "Exit"),
    (-150, "LoadSeg"),
    (-156, "UnLoadSeg"),
    (-174, "DeviceProc"),
    (-180, "SetComment"),
    (-186, "SetProtection"),
    (-192, "DateStamp"),
    (-198, "Delay"),
    (-204, "WaitForChar"),
    (-210, "ParentDir"),
    (-216, "IsInteractive"),
    (-222, "Execute"),
    (-228, "AllocDosObject"),
    (-234, "FreeDosObject"),
    (-240, "DoPkt"),
    (-246, "SendPkt"),
    (-252, "WaitPkt"),
    (-258, "ReplyPkt"),
    (-264, "AbortPkt"),
    (-270, "LockRecord"),
    (-276, "LockRecords"),
    (-282, "UnLockRecord"),
    (-288, "UnLockRecords"),
    (-294, "SelectInput"),
    (-300, "SelectOutput"),
    (-306, "FGetC"),
    (-312, "FPutC"),
    (-318, "UnGetC"),
    (-324, "FRead"),
    (-330, "FWrite"),
    (-336, "FGets"),
    (-342, "FPuts"),
    (-348, "VFWritef"),
    (-354, "VFPrintf"),
    (-360, "Flush"),
    (-366, "SetVBuf"),
    (-372, "DupLockFromFH"),
    (-378, "OpenFromLock"),
    (-384, "ParentOfFH"),
    (-390, "ExamineFH"),
    (-396, "SetFileDate"),
    (-402, "NameFromLock"),
    (-408, "NameFromFH"),
    (-414, "SplitName"),
    (-420, "SameLock"),
    (-426, "SetMode"),
    (-432, "ExAll"),
    (-438, "ReadLink"),
    (-444, "MakeLink"),
    (-450, "ChangeMode"),
    (-456, "SetFileSize"),
    (-462, "SetIoErr"),
    (-468, "Fault"),
    (-474, "PrintFault"),
    (-480, "ErrorReport"),
    (-492, "Cli"),
    (-498, "CreateNewProc"),
    (-504, "RunCommand"),
    (-510, "GetConsoleTask"),
    (-516, "SetConsoleTask"),
    (-522, "GetFileSysTask"),
    (-528, "SetFileSysTask"),
    (-534, "GetArgStr"),
    (-540, "SetArgStr"),
    (-546, "FindCliProc"),
    (-552, "MaxCli"),
    (-558, "SetCurrentDirName"),
    (-564, "GetCurrentDirName"),
    (-570, "SetProgramName"),
    (-576, "GetProgramName"),
    (-582, "SetPrompt"),
    (-588, "GetPrompt"),
    (-594, "SetProgramDir"),
    (-600, "GetProgramDir"),
    (-606, "SystemTagList"),
    (-612, "AssignLock"),
    (-618, "AssignLate"),
    (-624, "AssignPath"),
    (-630, "AssignAdd"),
    (-636, "RemAssignList"),
    (-642, "GetDeviceProc"),
    (-648, "FreeDeviceProc"),
    (-654, "LockDosList"),
    (-660, "UnLockDosList"),
    (-666, "AttemptLockDosList"),
    (-672, "RemDosEntry"),
    (-678, "AddDosEntry"),
    (-684, "FindDosEntry"),
    (-690, "NextDosEntry"),
    (-696, "MakeDosEntry"),
    (-702, "FreeDosEntry"),
    (-708, "IsFileSystem"),
    (-714, "Format"),
    (-720, "Relabel"),
    (-726, "Inhibit"),
    (-732, "AddBuffers"),
    (-738, "CompareDates"),
    (-744, "DateToStr"),
    (-750, "StrToDate"),
    (-756, "InternalLoadSeg"),
    (-762, "InternalUnLoadSeg"),
    (-768, "NewLoadSeg"),
    (-774, "AddSegment"),
    (-780, "FindSegment"),
    (-786, "RemSegment"),
    (-792, "CheckSignal"),
    (-798, "ReadArgs"),
    (-804, "FindArg"),
    (-810, "ReadItem"),
    (-816, "StrToLong"),
    (-822, "MatchFirst"),
    (-828, "MatchNext"),
    (-834, "MatchEnd"),
    (-840, "ParsePattern"),
    (-846, "MatchPattern"),
    (-858, "FreeArgs"),
    (-870, "FilePart"),
    (-876, "PathPart"),
    (-882, "AddPart"),
    (-888, "StartNotify"),
    (-894, "EndNotify"),
    (-900, "SetVar"),
    (-906, "GetVar"),
    (-912, "DeleteVar"),
    (-918, "FindVar"),
    (-930, "CliInitNewcli"),
    (-936, "CliInitRun"),
    (-942, "WriteChars"),
    (-948, "PutStr"),
    (-954, "VPrintf"),
    (-966, "ParsePatternNoCase"),
    (-972, "MatchPatternNoCase"),
    (-984, "SameDevice"),
    (-990, "ExAllEnd"),
    (-996, "SetOwner"),
    (-1014, "VolumeRequestHook"),
    (-1026, "GetCurrentDir"),
    (-1128, "PutErrStr"),
    (-1134, "ErrorOutput"),
    (-1140, "SelectError"),
    (-1152, "DoShellMethodTagList"),
    (-1158, "ScanStackToken"),
];

// Generated from NDK 3.2 Include_I/lvo/intuition_lib.i — 127 entries
// + 4 inherited Library slots.
const INTUITION_LIBRARY: &[(i32, &str)] = &[
    // Inherited from struct Library
    (-6, "Open"),
    (-12, "Close"),
    (-18, "Expunge"),
    (-24, "Reserved"),
    // intuition-specific
    (-30, "OpenIntuition"),
    (-36, "Intuition"),
    (-42, "AddGadget"),
    (-48, "ClearDMRequest"),
    (-54, "ClearMenuStrip"),
    (-60, "ClearPointer"),
    (-66, "CloseScreen"),
    (-72, "CloseWindow"),
    (-78, "CloseWorkBench"),
    (-84, "CurrentTime"),
    (-90, "DisplayAlert"),
    (-96, "DisplayBeep"),
    (-102, "DoubleClick"),
    (-108, "DrawBorder"),
    (-114, "DrawImage"),
    (-120, "EndRequest"),
    (-126, "GetDefPrefs"),
    (-132, "GetPrefs"),
    (-138, "InitRequester"),
    (-144, "ItemAddress"),
    (-150, "ModifyIDCMP"),
    (-156, "ModifyProp"),
    (-162, "MoveScreen"),
    (-168, "MoveWindow"),
    (-174, "OffGadget"),
    (-180, "OffMenu"),
    (-186, "OnGadget"),
    (-192, "OnMenu"),
    (-198, "OpenScreen"),
    (-204, "OpenWindow"),
    (-210, "OpenWorkBench"),
    (-216, "PrintIText"),
    (-222, "RefreshGadgets"),
    (-228, "RemoveGadget"),
    (-234, "ReportMouse"),
    (-240, "Request"),
    (-246, "ScreenToBack"),
    (-252, "ScreenToFront"),
    (-258, "SetDMRequest"),
    (-264, "SetMenuStrip"),
    (-270, "SetPointer"),
    (-276, "SetWindowTitles"),
    (-282, "ShowTitle"),
    (-288, "SizeWindow"),
    (-294, "ViewAddress"),
    (-300, "ViewPortAddress"),
    (-306, "WindowToBack"),
    (-312, "WindowToFront"),
    (-318, "WindowLimits"),
    (-324, "SetPrefs"),
    (-330, "IntuiTextLength"),
    (-336, "WBenchToBack"),
    (-342, "WBenchToFront"),
    (-348, "AutoRequest"),
    (-354, "BeginRefresh"),
    (-360, "BuildSysRequest"),
    (-366, "EndRefresh"),
    (-372, "FreeSysRequest"),
    (-378, "MakeScreen"),
    (-384, "RemakeDisplay"),
    (-390, "RethinkDisplay"),
    (-396, "AllocRemember"),
    (-402, "AlohaWorkbench"),
    (-408, "FreeRemember"),
    (-414, "LockIBase"),
    (-420, "UnlockIBase"),
    (-426, "GetScreenData"),
    (-432, "RefreshGList"),
    (-438, "AddGList"),
    (-444, "RemoveGList"),
    (-450, "ActivateWindow"),
    (-456, "RefreshWindowFrame"),
    (-462, "ActivateGadget"),
    (-468, "NewModifyProp"),
    (-474, "QueryOverscan"),
    (-480, "MoveWindowInFrontOf"),
    (-486, "ChangeWindowBox"),
    (-492, "SetEditHook"),
    (-498, "SetMouseQueue"),
    (-504, "ZipWindow"),
    (-510, "LockPubScreen"),
    (-516, "UnlockPubScreen"),
    (-522, "LockPubScreenList"),
    (-528, "UnlockPubScreenList"),
    (-534, "NextPubScreen"),
    (-540, "SetDefaultPubScreen"),
    (-546, "SetPubScreenModes"),
    (-552, "PubScreenStatus"),
    (-558, "ObtainGIRPort"),
    (-564, "ReleaseGIRPort"),
    (-570, "GadgetMouse"),
    (-582, "GetDefaultPubScreen"),
    (-588, "EasyRequestArgs"),
    (-594, "BuildEasyRequestArgs"),
    (-600, "SysReqHandler"),
    (-606, "OpenWindowTagList"),
    (-612, "OpenScreenTagList"),
    (-618, "DrawImageState"),
    (-624, "PointInImage"),
    (-630, "EraseImage"),
    (-636, "NewObjectA"),
    (-642, "DisposeObject"),
    (-648, "SetAttrsA"),
    (-654, "GetAttr"),
    (-660, "SetGadgetAttrsA"),
    (-666, "NextObject"),
    (-678, "MakeClass"),
    (-684, "AddClass"),
    (-690, "GetScreenDrawInfo"),
    (-696, "FreeScreenDrawInfo"),
    (-702, "ResetMenuStrip"),
    (-708, "RemoveClass"),
    (-714, "FreeClass"),
    (-768, "AllocScreenBuffer"),
    (-774, "FreeScreenBuffer"),
    (-780, "ChangeScreenBuffer"),
    (-786, "ScreenDepth"),
    (-792, "ScreenPosition"),
    (-798, "ScrollWindowRaster"),
    (-804, "LendMenus"),
    (-810, "DoGadgetMethodA"),
    (-816, "SetWindowPointerA"),
    (-822, "TimedDisplayAlert"),
    (-828, "HelpControl"),
    (-834, "ShowWindow"),
    (-840, "HideWindow"),
    (-1212, "IntuitionControlA"),
];

// Generated from NDK 3.2 Include_I/lvo/graphics_lib.i — 163 entries
// + 4 inherited Library slots.
const GRAPHICS_LIBRARY: &[(i32, &str)] = &[
    // Inherited from struct Library
    (-6, "Open"),
    (-12, "Close"),
    (-18, "Expunge"),
    (-24, "Reserved"),
    // graphics-specific
    (-30, "BltBitMap"),
    (-36, "BltTemplate"),
    (-42, "ClearEOL"),
    (-48, "ClearScreen"),
    (-54, "TextLength"),
    (-60, "Text"),
    (-66, "SetFont"),
    (-72, "OpenFont"),
    (-78, "CloseFont"),
    (-84, "AskSoftStyle"),
    (-90, "SetSoftStyle"),
    (-96, "AddBob"),
    (-102, "AddVSprite"),
    (-108, "DoCollision"),
    (-114, "DrawGList"),
    (-120, "InitGels"),
    (-126, "InitMasks"),
    (-132, "RemIBob"),
    (-138, "RemVSprite"),
    (-144, "SetCollision"),
    (-150, "SortGList"),
    (-156, "AddAnimOb"),
    (-162, "Animate"),
    (-168, "GetGBuffers"),
    (-174, "InitGMasks"),
    (-180, "DrawEllipse"),
    (-186, "AreaEllipse"),
    (-192, "LoadRGB4"),
    (-198, "InitRastPort"),
    (-204, "InitVPort"),
    (-210, "MrgCop"),
    (-216, "MakeVPort"),
    (-222, "LoadView"),
    (-228, "WaitBlit"),
    (-234, "SetRast"),
    (-240, "Move"),
    (-246, "Draw"),
    (-252, "AreaMove"),
    (-258, "AreaDraw"),
    (-264, "AreaEnd"),
    (-270, "WaitTOF"),
    (-276, "QBlit"),
    (-282, "InitArea"),
    (-288, "SetRGB4"),
    (-294, "QBSBlit"),
    (-300, "BltClear"),
    (-306, "RectFill"),
    (-312, "BltPattern"),
    (-318, "ReadPixel"),
    (-324, "WritePixel"),
    (-330, "Flood"),
    (-336, "PolyDraw"),
    (-342, "SetAPen"),
    (-348, "SetBPen"),
    (-354, "SetDrMd"),
    (-360, "InitView"),
    (-366, "CBump"),
    (-372, "CMove"),
    (-378, "CWait"),
    (-384, "VBeamPos"),
    (-390, "InitBitMap"),
    (-396, "ScrollRaster"),
    (-402, "WaitBOVP"),
    (-408, "GetSprite"),
    (-414, "FreeSprite"),
    (-420, "ChangeSprite"),
    (-426, "MoveSprite"),
    (-432, "LockLayerRom"),
    (-438, "UnlockLayerRom"),
    (-444, "SyncSBitMap"),
    (-450, "CopySBitMap"),
    (-456, "OwnBlitter"),
    (-462, "DisownBlitter"),
    (-468, "InitTmpRas"),
    (-474, "AskFont"),
    (-480, "AddFont"),
    (-486, "RemFont"),
    (-492, "AllocRaster"),
    (-498, "FreeRaster"),
    (-504, "AndRectRegion"),
    (-510, "OrRectRegion"),
    (-516, "NewRegion"),
    (-522, "ClearRectRegion"),
    (-528, "ClearRegion"),
    (-534, "DisposeRegion"),
    (-540, "FreeVPortCopLists"),
    (-546, "FreeCopList"),
    (-552, "ClipBlit"),
    (-558, "XorRectRegion"),
    (-564, "FreeCprList"),
    (-570, "GetColorMap"),
    (-576, "FreeColorMap"),
    (-582, "GetRGB4"),
    (-588, "ScrollVPort"),
    (-594, "UCopperListInit"),
    (-600, "FreeGBuffers"),
    (-606, "BltBitMapRastPort"),
    (-612, "OrRegionRegion"),
    (-618, "XorRegionRegion"),
    (-624, "AndRegionRegion"),
    (-630, "SetRGB4CM"),
    (-636, "BltMaskBitMapRastPort"),
    (-654, "AttemptLockLayerRom"),
    (-660, "GfxNew"),
    (-666, "GfxFree"),
    (-672, "GfxAssociate"),
    (-678, "BitMapScale"),
    (-684, "ScalerDiv"),
    (-690, "TextExtent"),
    (-696, "TextFit"),
    (-702, "GfxLookUp"),
    (-708, "VideoControl"),
    (-714, "OpenMonitor"),
    (-720, "CloseMonitor"),
    (-726, "FindDisplayInfo"),
    (-732, "NextDisplayInfo"),
    (-756, "GetDisplayInfoData"),
    (-762, "FontExtent"),
    (-768, "ReadPixelLine8"),
    (-774, "WritePixelLine8"),
    (-780, "ReadPixelArray8"),
    (-786, "WritePixelArray8"),
    (-792, "GetVPModeID"),
    (-798, "ModeNotAvailable"),
    (-804, "WeighTAMatch"),
    (-810, "EraseRect"),
    (-816, "ExtendFont"),
    (-822, "StripFont"),
    (-828, "CalcIVG"),
    (-834, "AttachPalExtra"),
    (-840, "ObtainBestPenA"),
    (-852, "SetRGB32"),
    (-858, "GetAPen"),
    (-864, "GetBPen"),
    (-870, "GetDrMd"),
    (-876, "GetOutlinePen"),
    (-882, "LoadRGB32"),
    (-888, "SetChipRev"),
    (-894, "SetABPenDrMd"),
    (-900, "GetRGB32"),
    (-918, "AllocBitMap"),
    (-924, "FreeBitMap"),
    (-930, "GetExtSpriteA"),
    (-936, "CoerceMode"),
    (-942, "ChangeVPBitMap"),
    (-948, "ReleasePen"),
    (-954, "ObtainPen"),
    (-960, "GetBitMapAttr"),
    (-966, "AllocDBufInfo"),
    (-972, "FreeDBufInfo"),
    (-978, "SetOutlinePen"),
    (-984, "SetWriteMask"),
    (-990, "SetMaxPen"),
    (-996, "SetRGB32CM"),
    (-1002, "ScrollRasterBF"),
    (-1008, "FindColor"),
    (-1020, "AllocSpriteDataA"),
    (-1026, "ChangeExtSpriteA"),
    (-1032, "FreeSpriteData"),
    (-1038, "SetRPAttrsA"),
    (-1044, "GetRPAttrsA"),
    (-1050, "BestModeIDA"),
    (-1056, "WriteChunkyPixels"),
];

// Sourced from NDK 3.2 FD/cia_lib.fd — `struct Resource` rather than
// `struct Library`, so there are no inherited Open/Close/Expunge/
// Reserved slots; the first LVO is at -6.
const CIA_RESOURCE: &[(i32, &str)] = &[
    (-6, "AddICRVector"),
    (-12, "RemICRVector"),
    (-18, "AbleICR"),
    (-24, "SetICR"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_offset_resolves() {
        assert_eq!(resolve("exec.library", -162), Some("SetIntVector"));
        assert_eq!(resolve("exec.library", -318), Some("Wait"));
        assert_eq!(resolve("exec.library", -552), Some("OpenLibrary"));
        assert_eq!(resolve("dos.library", -84), Some("Lock"));
    }

    #[test]
    fn positive_offset_resolves_via_magnitude() {
        // Callers that pulled the offset from a hex string may pass
        // the absolute magnitude. Resolver must accept both forms.
        assert_eq!(resolve("exec.library", 162), Some("SetIntVector"));
        assert_eq!(resolve("dos.library", 84), Some("Lock"));
    }

    #[test]
    fn dos_open_disambiguated_by_offset() {
        // Both -6 and -30 are "Open" in dos.library — the standard
        // library Open and the DOS file Open. The resolver returns
        // the name at the offset; the caller disambiguates by
        // remembering which one they meant.
        assert_eq!(resolve("dos.library", -6), Some("Open"));
        assert_eq!(resolve("dos.library", -30), Some("Open"));
    }

    #[test]
    fn unknown_library_or_offset_returns_none() {
        assert_eq!(resolve("nosuch.library", -30), None);
        // -8 isn't on any 6-byte boundary — must not match anything.
        assert_eq!(resolve("exec.library", -8), None);
    }

    #[test]
    fn library_names_all_have_tables() {
        for lib in LIBRARY_NAMES {
            assert!(
                lvo_table(lib).is_some(),
                "{lib} listed in LIBRARY_NAMES but has no table"
            );
        }
    }

    #[test]
    fn all_offsets_are_negative_and_multiples_of_six() {
        for lib in LIBRARY_NAMES {
            let table = lvo_table(lib).unwrap();
            for (off, name) in table.iter() {
                assert!(*off < 0, "{lib}::{name} has non-negative offset {off}");
                assert_eq!(
                    *off % 6,
                    0,
                    "{lib}::{name} offset {off} is not a multiple of LIB_VECTSIZE (6)"
                );
            }
        }
    }

    #[test]
    fn canonical_landmarks_match_ndk() {
        // Sanity-check a few offsets against the NDK 3.2 source.
        // Any drift here is a sign the tables fell out of sync.
        assert_eq!(resolve("exec.library", -552), Some("OpenLibrary"));
        assert_eq!(resolve("exec.library", -318), Some("Wait"));
        assert_eq!(resolve("exec.library", -198), Some("AllocMem"));
        assert_eq!(resolve("exec.library", -330), Some("AllocSignal"));
        assert_eq!(resolve("dos.library", -84), Some("Lock"));
        assert_eq!(resolve("dos.library", -150), Some("LoadSeg"));
        assert_eq!(resolve("intuition.library", -204), Some("OpenWindow"));
        assert_eq!(resolve("graphics.library", -222), Some("LoadView"));
    }
}
