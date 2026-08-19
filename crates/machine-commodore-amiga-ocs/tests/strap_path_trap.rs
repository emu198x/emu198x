//! Trace which path strap takes through its init.
//!
//! Strap at $FE8444:
//!   - Allocates $488 bytes of chip RAM (MEMF_CHIP|MEMF_CLEAR)
//!   - If allocation fails: Alert($30010000) then BRA.W $FE86E0
//!     (early exit — strap does nothing)
//!   - If succeeds: continue at $FE8498 with the allocated buffer
//!
//! Trap points:
//!   $FE8444 — strap entry
//!   $FE848E — Alert call (= AllocMem failed)
//!   $FE8498 — success-path start
//!   $FE86E0 — early-exit target after Alert
//!
//! Together these tell us whether strap completed its real work
//! (success path) or bailed on AllocMem failure.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const STRAP_ENTRY: u32 = 0x00FE_8444;
const STRAP_ALERT_CALL: u32 = 0x00FE_848E;
const STRAP_SUCCESS_PATH: u32 = 0x00FE_8498;
const STRAP_EARLY_EXIT: u32 = 0x00FE_86E0;

/// After JSR OpenDevice — D0 holds OpenDevice's return (0 = success,
/// non-zero = IO_ERROR).
const STRAP_POST_OPEN_DEVICE: u32 = 0x00FE_8506;
/// OpenDevice-failed alert path.
const STRAP_OPEN_DEVICE_ALERT: u32 = 0x00FE_8518;
/// OpenDevice-succeeded path.
const STRAP_OPEN_DEVICE_OK: u32 = 0x00FE_8524;
/// Just before JSR DoIO(A6) with an IORequest (cmd=$0005 CMD_CLEAR).
const STRAP_DO_IO_CALL: u32 = 0x00FE_855C;
/// Just after DoIO returns.
const STRAP_POST_DO_IO: u32 = 0x00FE_8560;

/// Additional DoIO call sites inside strap's retry loop.
/// Each preceded by MOVE.W #cmd, 28(A1) so we know what's being
/// issued:
///   $FE8570 — cmd=$0D TD_CHANGESTATE  "is disk inserted?"
///   $FE859C — cmd=$02 CMD_READ       "read boot block (len=$400, off=0)"
///   $FE8630 — cmd=$09 TD_MOTOR       "spin motor"
///   $FE8642 — cmd=$0D TD_CHANGESTATE "poll-disk-change"
///   $FE865A — cmd=$0E TD_CHANGENUM   "poll-disk-change-number"
///   $FE8676 — cmd=$0D TD_CHANGESTATE "final check"
const STRAP_DOIO_SITES: &[(u32, &str)] = &[
    (0x00FE_8570, "TD_CHANGESTATE(13)"),
    (0x00FE_859C, "CMD_READ(2) block 0"),
    (0x00FE_8630, "TD_MOTOR(9)"),
    (0x00FE_8642, "TD_CHANGESTATE(13)"),
    (0x00FE_865A, "TD_CHANGENUM(14)"),
    (0x00FE_8676, "TD_CHANGESTATE(13)"),
];

/// Error-exit preamble (all failure branches converge here).
const STRAP_ERR_EXIT: u32 = 0x00FE_867C;
/// Retry-loop start (ADDA #1,A2 ; CMPA #0,A2 ; BLE loop).
const STRAP_RETRY_HEAD: u32 = 0x00FE_8600;
/// Close-device path (normal or error exit).
const STRAP_CLOSE_DEVICE: u32 = 0x00FE_86C4;
/// After CMD_READ DoIO returns — TST.L D0 to check io_Error.
const STRAP_POST_CMD_READ: u32 = 0x00FE_85A0;
/// If CMD_READ ok and block starts with "DOS\0" magic, strap will
/// reach $FE85AC. Otherwise BNE to retry head.
const STRAP_DOS_MAGIC_OK: u32 = 0x00FE_85AC;
/// If everything parses, strap calls $FE8C9C (likely boot block
/// execution / sanity check).
const STRAP_EXEC_BOOT: u32 = 0x00FE_85F2;
/// BNE.S right after TST.L D0 post-CMD_READ.
const STRAP_CMD_READ_BNE: u32 = 0x00FE_85A2;
/// Fall-through after BNE — MOVE.L (A4), D0 (read buffer[0]).
const STRAP_CMD_READ_FALLTHROUGH: u32 = 0x00FE_85A4;

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

fn fmt(v: Option<u32>) -> String {
    match v {
        Some(v) => format!("${v:08X}"),
        None => "(not captured)".into(),
    }
}

fn run(amiga: &mut AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    let mut entry = 0u64;
    let mut alert = 0u64;
    let mut success = 0u64;
    let mut early_exit = 0u64;
    // Sample D0 at several points. Amiga AllocMem returns in D0.
    // - post_allocmem_tst  = $FE847A  (TST.L D0)
    // - post_allocmem_bne  = $FE847C  (BNE.S)
    // - success_path       = $FE8498
    // First-time-PC-hit capture for each.
    let mut d0_at_tst: Option<u32> = None;
    let mut d0_at_bne: Option<u32> = None;
    let mut d0_at_success: Option<u32> = None;
    let mut d0_at_entry: Option<u32> = None;
    let mut d0_at_post_opendev: Option<u32> = None;
    let mut open_device_alert = 0u64;
    let mut open_device_ok = 0u64;
    let mut do_io_call = 0u64;
    let mut post_do_io = 0u64;
    let mut d0_after_do_io: Option<u32> = None;
    let mut site_hits: Vec<(u32, &'static str, u64, Option<u32>)> = STRAP_DOIO_SITES
        .iter()
        .map(|(pc, name)| (*pc, *name, 0, None))
        .collect();
    let mut retry_head = 0u64;
    let mut err_exit = 0u64;
    let mut close_device = 0u64;
    let mut post_cmd_read = 0u64;
    let mut d0_after_cmd_read: Option<u32> = None;
    let mut dos_magic_ok = 0u64;
    let mut exec_boot = 0u64;
    let mut cmd_read_bne = 0u64;
    let mut cmd_read_fall = 0u64;
    let pc_tst = 0x00FE_847A_u32;
    let pc_bne = 0x00FE_847C_u32;
    let mut prev_pc = amiga.cpu().regs.pc;

    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        if pc == STRAP_ENTRY {
            entry += 1;
            if d0_at_entry.is_none() {
                d0_at_entry = Some(amiga.cpu().regs.d[0]);
            }
        } else if pc == STRAP_ALERT_CALL {
            alert += 1;
        } else if pc == STRAP_SUCCESS_PATH {
            success += 1;
            if d0_at_success.is_none() {
                d0_at_success = Some(amiga.cpu().regs.d[0]);
            }
        } else if pc == STRAP_EARLY_EXIT {
            early_exit += 1;
        } else if pc == pc_tst && d0_at_tst.is_none() {
            d0_at_tst = Some(amiga.cpu().regs.d[0]);
        } else if pc == pc_bne && d0_at_bne.is_none() {
            d0_at_bne = Some(amiga.cpu().regs.d[0]);
        } else if pc == STRAP_POST_OPEN_DEVICE && d0_at_post_opendev.is_none() {
            d0_at_post_opendev = Some(amiga.cpu().regs.d[0]);
        } else if pc == STRAP_OPEN_DEVICE_ALERT {
            open_device_alert += 1;
        } else if pc == STRAP_OPEN_DEVICE_OK {
            open_device_ok += 1;
        } else if pc == STRAP_DO_IO_CALL {
            do_io_call += 1;
        } else if pc == STRAP_POST_DO_IO {
            post_do_io += 1;
            if d0_after_do_io.is_none() {
                d0_after_do_io = Some(amiga.cpu().regs.d[0]);
            }
        } else if pc == STRAP_RETRY_HEAD {
            retry_head += 1;
        } else if pc == STRAP_ERR_EXIT {
            err_exit += 1;
        } else if pc == STRAP_CLOSE_DEVICE {
            close_device += 1;
        } else if pc == STRAP_POST_CMD_READ {
            post_cmd_read += 1;
            if d0_after_cmd_read.is_none() {
                d0_after_cmd_read = Some(amiga.cpu().regs.d[0]);
            }
        } else if pc == STRAP_DOS_MAGIC_OK {
            dos_magic_ok += 1;
        } else if pc == STRAP_EXEC_BOOT {
            exec_boot += 1;
        } else if pc == STRAP_CMD_READ_BNE {
            cmd_read_bne += 1;
        } else if pc == STRAP_CMD_READ_FALLTHROUGH {
            cmd_read_fall += 1;
        } else {
            for s in site_hits.iter_mut() {
                if pc == s.0 {
                    s.2 += 1;
                    // Capture D0 once at the instruction right after
                    // each DoIO (the TST.L). To do that exactly would
                    // need the "post PC" too. For now, snapshot D0 at
                    // call-site time (size + io ptr in D0/A1 set earlier,
                    // not final D0 — we just get a pre-exec capture).
                    if s.3.is_none() {
                        s.3 = Some(amiga.cpu().regs.d[0]);
                    }
                }
            }
        }
        prev_pc = pc;
    }

    eprintln!("strap entry hits:           {entry}");
    eprintln!("D0 at entry:                {}", fmt(d0_at_entry));
    eprintln!("D0 at $FE847A (TST.L):      {}", fmt(d0_at_tst));
    eprintln!("D0 at $FE847C (BNE.S):      {}", fmt(d0_at_bne));
    eprintln!("D0 at $FE8498 (success):    {}", fmt(d0_at_success));
    eprintln!("AllocMem-fail Alert hits:   {alert}");
    eprintln!("Success-path hits:          {success}");
    eprintln!("Early-exit target hits:     {early_exit}");
    eprintln!();
    eprintln!("D0 after OpenDevice:        {}", fmt(d0_at_post_opendev));
    eprintln!("OpenDevice-fail Alert hits: {open_device_alert}");
    eprintln!("OpenDevice-OK path hits:    {open_device_ok}");
    eprintln!();
    eprintln!("DoIO #1 (CMD_CLEAR) hits:   {do_io_call}");
    eprintln!("After DoIO #1 hits:         {post_do_io}");
    eprintln!("D0 after DoIO #1:           {}", fmt(d0_after_do_io));
    eprintln!();
    eprintln!("Other DoIO call-sites:");
    for (pc, name, hits, _) in &site_hits {
        eprintln!("  ${pc:08X} {name:<22} hits={hits}");
    }
    eprintln!();
    eprintln!("Post-CMD_READ ($FE85A0) hits:   {post_cmd_read}");
    eprintln!("D0 after CMD_READ:              {}", fmt(d0_after_cmd_read));
    eprintln!("BNE after CMD_READ ($FE85A2):   {cmd_read_bne}");
    eprintln!("Fall-through ($FE85A4):         {cmd_read_fall}");
    eprintln!("DOS-magic-matched ($FE85AC):    {dos_magic_ok}");
    eprintln!("Exec-boot ($FE85F2) hits:       {exec_boot}");
    eprintln!("Retry-loop head ($FE8600) hits: {retry_head}");
    eprintln!("Error-exit preamble hits:       {err_exit}");
    eprintln!("Close-device JSR hits:          {close_device}");

    if alert > 0 && success == 0 {
        eprintln!("\n→ strap took the ALERT path — AllocMem($488, CHIP|CLEAR) FAILED.");
        eprintln!("  1160 bytes of chip RAM couldn't be allocated at strap time.");
        eprintln!("  That's our bug: we need to understand why.");
    } else if success > 0 && alert == 0 {
        eprintln!("\n→ strap took the SUCCESS path — AllocMem succeeded.");
        eprintln!("  Need to trace further to see where it then diverges.");
    } else {
        eprintln!("\n→ Unexpected state.");
    }
}

#[test]
#[ignore]
fn trap_strap_branch() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only");
}
