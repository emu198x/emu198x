//! Walk graphics.library's GfxBase + View at frame 250 to find out
//! whether we're sitting in the wiki's documented "View->ViewPort
//! is NULL → MrgCop early-exits → GfxBase->LOFlist stays at the
//! ExecBase placeholder" state.
//!
//! Per `knowledge/decisions/amiga-chip-only-boot-failure.md` (5th pass),
//! the old slow-RAM trace showed:
//!
//! | Field                   | chip-only          | slow-RAM         |
//! |-------------------------|--------------------|------------------|
//! | GfxBase->ActiView       | $000049A6          | $00005A10        |
//! | ActiView->ViewPort      | $00000000 (NULL)   | $000059E8        |
//! | ActiView->LOFCprList    | $00000000 (NULL)   | $00C01808        |
//! | ActiView->DyOffset      | $002C              | $002C            |
//! | ActiView->DxOffset      | $0081              | $0081            |
//! | GfxBase->LOFlist        | $00000676 (=ExecBase!) | $0000B888    |
//!
//! If our fresh OCS slow-RAM matches the chip-only column (NULL
//! ViewPort, LOFlist=ExecBase), we have the same MrgCop early-exit
//! deadlock. If it matches the slow-RAM column we've lost something
//! else. If it sits somewhere else entirely, that's its own clue.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

// ExecBase offsets we need.
const EXEC_LIB_LIST: u32 = 378;

// Library node offsets.
const LN_SUCC: u32 = 0;
const LN_NAME: u32 = 10;

// GfxBase offsets after the Library struct (sizeof Library = 34 = $22).
const GFX_ACTI_VIEW: u32 = 0x22;
const GFX_COPINIT: u32 = 0x26;
const GFX_CIA: u32 = 0x2A;
const GFX_BLITTER: u32 = 0x2E;
const GFX_LOF_LIST: u32 = 0x32;
const GFX_SHF_LIST: u32 = 0x36;

// View struct offsets (gfx/view.h).
const VIEW_VIEW_PORT: u32 = 0x00;
const VIEW_LOF_CPRLIST: u32 = 0x04;
const VIEW_SHF_CPRLIST: u32 = 0x08;
const VIEW_DY_OFFSET: u32 = 0x0C;
const VIEW_DX_OFFSET: u32 = 0x0E;
const VIEW_MODES: u32 = 0x10;

// ViewPort struct offsets (gfx/view.h).
const VP_NEXT: u32 = 0x00;
const VP_COLOR_MAP: u32 = 0x04;
const VP_DSP_INS: u32 = 0x08;
const VP_SPRINS: u32 = 0x0C;
const VP_CLRINS: u32 = 0x10;
const VP_UCOPINS: u32 = 0x14;
const VP_DWIDTH: u32 = 0x18;
const VP_DHEIGHT: u32 = 0x1A;
const VP_DXOFFSET: u32 = 0x1C;
const VP_DYOFFSET: u32 = 0x1E;
const VP_MODES: u32 = 0x20;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: Kickstart 1.3 ROM missing at {}",
            path.display()
        ));
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn read_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    amiga.read_long(addr)
}

fn read_word(amiga: &AmigaOcs, addr: u32) -> u16 {
    amiga.read_word(addr)
}

fn read_byte(amiga: &AmigaOcs, addr: u32) -> u8 {
    (amiga.read_word(addr & !1) >> (if addr & 1 == 0 { 8 } else { 0 })) as u8
}

fn read_cstring(amiga: &AmigaOcs, addr: u32, max: u32) -> String {
    if addr == 0 {
        return "<null>".into();
    }
    let mut s = String::new();
    for i in 0..max {
        let b = read_byte(amiga, addr.wrapping_add(i));
        if b == 0 {
            break;
        }
        if b.is_ascii() && !b.is_ascii_control() {
            s.push(b as char);
        } else {
            s.push('?');
        }
    }
    s
}

fn find_library(amiga: &AmigaOcs, exec_base: u32, target: &str) -> Option<u32> {
    let list_addr = exec_base.wrapping_add(EXEC_LIB_LIST);
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut node = head;
    for _ in 0..16 {
        if node == 0 || node == tail_sentinel {
            return None;
        }
        let name_ptr = read_long(amiga, node.wrapping_add(LN_NAME));
        let name = read_cstring(amiga, name_ptr, 32);
        if name == target {
            return Some(node);
        }
        node = read_long(amiga, node.wrapping_add(LN_SUCC));
    }
    None
}

fn dump_gfxbase(amiga: &AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    let exec_base = read_long(amiga, 0x0000_0004);
    eprintln!("ExecBase = ${exec_base:08X}");
    if exec_base == 0 {
        eprintln!("(ExecBase uninitialised — abort)");
        return;
    }

    let Some(gfx_base) = find_library(amiga, exec_base, "graphics.library") else {
        emu198x_test_skip::skip!("graphics.library not found in LibList");
    };
    eprintln!("graphics.library base = ${gfx_base:08X}");

    let acti_view = read_long(amiga, gfx_base.wrapping_add(GFX_ACTI_VIEW));
    let copinit = read_long(amiga, gfx_base.wrapping_add(GFX_COPINIT));
    let cia = read_long(amiga, gfx_base.wrapping_add(GFX_CIA));
    let blitter = read_long(amiga, gfx_base.wrapping_add(GFX_BLITTER));
    let lof_list = read_long(amiga, gfx_base.wrapping_add(GFX_LOF_LIST));
    let shf_list = read_long(amiga, gfx_base.wrapping_add(GFX_SHF_LIST));

    eprintln!("\n=== GfxBase fields ===");
    eprintln!("GfxBase->ActiView   = ${acti_view:08X}");
    eprintln!("GfxBase->copinit    = ${copinit:08X}");
    eprintln!("GfxBase->cia        = ${cia:08X}");
    eprintln!("GfxBase->blitter    = ${blitter:08X}");
    eprintln!(
        "GfxBase->LOFlist    = ${lof_list:08X}  {}",
        if lof_list == exec_base {
            "←── ExecBase placeholder (BAD)"
        } else if lof_list == 0 {
            "←── NULL (BAD)"
        } else {
            "←── concrete pointer"
        }
    );
    eprintln!("GfxBase->SHFlist    = ${shf_list:08X}");

    if acti_view == 0 {
        eprintln!("\n(ActiView is NULL — graphics.library hasn't installed a View yet)");
        return;
    }

    let vp = read_long(amiga, acti_view.wrapping_add(VIEW_VIEW_PORT));
    let lof_cpr = read_long(amiga, acti_view.wrapping_add(VIEW_LOF_CPRLIST));
    let shf_cpr = read_long(amiga, acti_view.wrapping_add(VIEW_SHF_CPRLIST));
    let dy = read_word(amiga, acti_view.wrapping_add(VIEW_DY_OFFSET));
    let dx = read_word(amiga, acti_view.wrapping_add(VIEW_DX_OFFSET));
    let modes = read_word(amiga, acti_view.wrapping_add(VIEW_MODES));

    eprintln!("\n=== View @ ${acti_view:08X} ===");
    eprintln!(
        "View->ViewPort      = ${vp:08X}  {}",
        if vp == 0 {
            "←── NULL (MrgCop will EARLY-EXIT)"
        } else {
            ""
        }
    );
    eprintln!(
        "View->LOFCprList    = ${lof_cpr:08X}  {}",
        if lof_cpr == 0 {
            "←── NULL (no merged copper list)"
        } else {
            ""
        }
    );
    eprintln!("View->SHFCprList    = ${shf_cpr:08X}");
    eprintln!("View->DyOffset      = ${dy:04X}");
    eprintln!("View->DxOffset      = ${dx:04X}");
    eprintln!("View->Modes         = ${modes:04X}");

    if vp == 0 {
        eprintln!("\n(View->ViewPort NULL — can't walk ViewPort)");
        return;
    }

    let next = read_long(amiga, vp.wrapping_add(VP_NEXT));
    let cmap = read_long(amiga, vp.wrapping_add(VP_COLOR_MAP));
    let dsp = read_long(amiga, vp.wrapping_add(VP_DSP_INS));
    let sprins = read_long(amiga, vp.wrapping_add(VP_SPRINS));
    let clrins = read_long(amiga, vp.wrapping_add(VP_CLRINS));
    let ucopins = read_long(amiga, vp.wrapping_add(VP_UCOPINS));
    let dwidth = read_word(amiga, vp.wrapping_add(VP_DWIDTH));
    let dheight = read_word(amiga, vp.wrapping_add(VP_DHEIGHT));
    let dxoffset = read_word(amiga, vp.wrapping_add(VP_DXOFFSET));
    let dyoffset = read_word(amiga, vp.wrapping_add(VP_DYOFFSET));
    let vp_modes = read_word(amiga, vp.wrapping_add(VP_MODES));

    eprintln!("\n=== ViewPort @ ${vp:08X} ===");
    eprintln!("ViewPort->Next      = ${next:08X}");
    eprintln!("ViewPort->ColorMap  = ${cmap:08X}");
    eprintln!("ViewPort->DspIns    = ${dsp:08X}");
    eprintln!("ViewPort->SprIns    = ${sprins:08X}");
    eprintln!("ViewPort->ClrIns    = ${clrins:08X}");
    eprintln!("ViewPort->UCopIns   = ${ucopins:08X}");
    eprintln!("ViewPort->DWidth    = ${dwidth:04X}  ({dwidth} px)");
    eprintln!("ViewPort->DHeight   = ${dheight:04X}  ({dheight} px)");
    eprintln!("ViewPort->DxOffset  = ${dxoffset:04X}");
    eprintln!("ViewPort->DyOffset  = ${dyoffset:04X}");
    eprintln!("ViewPort->Modes     = ${vp_modes:04X}");
}

#[test]
#[ignore]
fn snapshot_gfxbase_at_frame_300() {
    let Some(rom) = load_kickstart() else { return };

    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let mut chip_only = AmigaOcs::new(rom);

    for _ in 0..(300 * PAL_FRAME_TICKS) {
        slow.tick();
        chip_only.tick();
    }

    dump_gfxbase(&slow, "slow-RAM (512K chip + 512K slow)");
    dump_gfxbase(&chip_only, "chip-only (512K chip)");
}

/// Trackdisk sends a 10.5-second TR_ADDREQUEST on CMD_READ. Run
/// long enough (700 frames = 14s at 50Hz PAL) to let it expire
/// and see what the boot state looks like after strap receives a
/// no-disk reply.
#[test]
#[ignore]
fn snapshot_gfxbase_at_frame_700() {
    let Some(rom) = load_kickstart() else { return };

    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let mut chip_only = AmigaOcs::new(rom);

    for _ in 0..(700 * PAL_FRAME_TICKS) {
        slow.tick();
        chip_only.tick();
    }

    dump_gfxbase(&slow, "slow-RAM (700 frames)");
    dump_gfxbase(&chip_only, "chip-only (700 frames)");
}
