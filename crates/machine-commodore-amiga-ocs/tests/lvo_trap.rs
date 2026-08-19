//! Trap calls to graphics.library LVOs during the fresh OCS boot
//! to see whether the "set up View->ViewPort" code path ever runs.
//!
//! Per the wiki (knowledge/decisions/amiga-chip-only-boot-failure.md)
//! and our own gfxbase_state diagnostic, the boot stalls with
//! `View->ViewPort = NULL`, which makes `MrgCop` early-exit and
//! leaves `GfxBase->LOFlist` at the ExecBase placeholder.
//!
//! The code path that would set `View->ViewPort` is:
//!
//!   (some caller) → MakeVPort → MrgCop → LoadView
//!
//! If `MrgCop` fires *repeatedly* but `MakeVPort` never fires at
//! all, we've confirmed lane 1 (missing subsystem — no caller has
//! tried to create a ViewPort yet). If `MakeVPort` runs but the
//! result never lands, that's a different lane.
//!
//! Method: boot for ~200 frames (enough to let graphics.library
//! load and the library jump table be populated), then resolve
//! each LVO's JMP target through the graphics.library jump stub
//! to a real ROM address. Run another 200 frames and count how
//! many ticks the CPU's PC sits on each entry point.
//!
//! Counts are tick-level (not call-level), so a single call that
//! takes 40 ticks registers as 40. That's fine — the point is the
//! "zero vs non-zero" distinction, and if MrgCop early-exits every
//! VBL for 200 frames we expect a large count.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

// ExecBase offsets.
const EXEC_LIB_LIST: u32 = 378;

// Node offsets.
const LN_SUCC: u32 = 0;
const LN_NAME: u32 = 10;

// graphics.library V34 (KS 1.3) LVOs. Each LVO slot is 6 bytes
// (JMP $xxxxxxxx = $4EF9 + 4-byte absolute long address). The LVO
// offset is negative from the library base.
const LVO_LOAD_VIEW: i32 = -222;
const LVO_MAKE_VPORT: i32 = -216;
const LVO_MRG_COP: i32 = -210;

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

/// Resolve an LVO to its real target address by reading the JMP
/// instruction stored at (gfx_base + lvo) — the LVO slot is
/// `$4EF9 <long>` and the long is the entry point.
fn resolve_lvo(amiga: &AmigaOcs, gfx_base: u32, lvo: i32) -> Option<u32> {
    let slot = (gfx_base as i64 + lvo as i64) as u32;
    let opcode = amiga.read_word(slot);
    if opcode != 0x4EF9 {
        eprintln!("  LVO {lvo} at ${slot:08X}: not a JMP (opcode=${opcode:04X})");
        return None;
    }
    let target = read_long(amiga, slot.wrapping_add(2));
    Some(target)
}

fn run(amiga: &mut AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    // Phase 1: 200 frames — let graphics.library init its jump table.
    for _ in 0..(200 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    let exec_base = read_long(amiga, 0x0000_0004);
    eprintln!("ExecBase = ${exec_base:08X}");

    let Some(gfx_base) = find_library(amiga, exec_base, "graphics.library") else {
        emu198x_test_skip::skip!("graphics.library not found — abort");
    };
    eprintln!("graphics.library base = ${gfx_base:08X}");

    let Some(load_view) = resolve_lvo(amiga, gfx_base, LVO_LOAD_VIEW) else {
        eprintln!("Couldn't resolve LoadView LVO — abort");
        return;
    };
    let Some(make_vport) = resolve_lvo(amiga, gfx_base, LVO_MAKE_VPORT) else {
        eprintln!("Couldn't resolve MakeVPort LVO — abort");
        return;
    };
    let Some(mrg_cop) = resolve_lvo(amiga, gfx_base, LVO_MRG_COP) else {
        eprintln!("Couldn't resolve MrgCop LVO — abort");
        return;
    };

    eprintln!("\n=== graphics.library entry points ===");
    eprintln!("  LoadView  = ${load_view:08X}");
    eprintln!("  MakeVPort = ${make_vport:08X}");
    eprintln!("  MrgCop    = ${mrg_cop:08X}");

    // Phase 2: 200 more frames, tick-level PC sampling.
    let mut load_view_ticks = 0u64;
    let mut make_vport_ticks = 0u64;
    let mut mrg_cop_ticks = 0u64;
    let mut total_ticks = 0u64;

    for _ in 0..(200 * PAL_FRAME_TICKS) {
        amiga.tick();
        total_ticks += 1;
        let pc = amiga.cpu().regs.pc;
        if pc == load_view {
            load_view_ticks += 1;
        }
        if pc == make_vport {
            make_vport_ticks += 1;
        }
        if pc == mrg_cop {
            mrg_cop_ticks += 1;
        }
    }

    eprintln!("\n=== Phase 2 LVO hit counts (over {total_ticks} ticks / 200 frames) ===");
    eprintln!("  LoadView  = {load_view_ticks} tick(s) on entry PC");
    eprintln!("  MakeVPort = {make_vport_ticks} tick(s) on entry PC");
    eprintln!("  MrgCop    = {mrg_cop_ticks} tick(s) on entry PC");

    eprintln!("\n=== Interpretation ===");
    if make_vport_ticks == 0 {
        eprintln!(
            "• MakeVPort NEVER called → no caller has tried to construct a\n  \
            ViewPort in phase 2. Lane 1 (missing subsystem) is strongly\n  \
            implicated — the code that *would* call MakeVPort never runs."
        );
    } else {
        eprintln!(
            "• MakeVPort fires ({make_vport_ticks} tick(s)) → a caller exists.\n  \
            Something downstream of MakeVPort is failing."
        );
    }
    if mrg_cop_ticks > 0 {
        eprintln!(
            "• MrgCop fires ({mrg_cop_ticks} tick(s)) → probably the VBL handler\n  \
            re-calls it every frame. Consistent with the wiki's\n  \
            'MrgCop early-exits on NULL ViewPort' diagnosis."
        );
    } else {
        eprintln!(
            "• MrgCop never fires → the VBL handler isn't reaching it.\n  \
            Worth checking whether the Int2 (VBL) chain is complete."
        );
    }
    if load_view_ticks > 0 {
        eprintln!(
            "• LoadView fires ({load_view_ticks} tick(s)) → a ViewPort has been\n  \
            installed at some point. Unexpected if we saw MakeVPort=0."
        );
    }
}

#[test]
#[ignore]
fn trap_graphics_lvos_at_frame_400() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM (512K chip + 512K slow)");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only (512K chip)");
}
