//! Snapshot boot state after ~300 frames to characterise where the
//! boot has settled. Tells us:
//!   - What the CPU is doing (PC, SR, tick_count)
//!   - Chipset registers (DMACON, INTENA, INTREQ, BPLCON0, colors)
//!   - Framebuffer content (background-only vs bitplane graphics)
//!   - TaskReady head vs lh_Tail (is the ready list empty?)
//!   - Key ExecBase fields (IdleCount, TDNestCnt, ThisTask)

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

const EXEC_THIS_TASK: u32 = 276;
const EXEC_IDLE_COUNT: u32 = 280;
const EXEC_DISP_COUNT: u32 = 284;
const EXEC_QUANTUM: u32 = 288;
const EXEC_SYS_FLAGS: u32 = 292;
const EXEC_TD_NEST_CNT: u32 = 295;
const EXEC_TASK_READY: u32 = 406;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn chip_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    let b0 = u32::from(amiga.read_chip_ram_byte(addr));
    let b1 = u32::from(amiga.read_chip_ram_byte(addr.wrapping_add(1)));
    let b2 = u32::from(amiga.read_chip_ram_byte(addr.wrapping_add(2)));
    let b3 = u32::from(amiga.read_chip_ram_byte(addr.wrapping_add(3)));
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

fn read_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    amiga.read_long(addr)
}

#[test]
#[ignore]
fn snapshot_boot_state_at_frame_300() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    for _ in 0..(300 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    let pc = amiga.cpu().regs.pc;
    let sr = amiga.cpu().regs.sr;

    eprintln!("\n=== CPU ===");
    eprintln!("PC     = ${pc:08X}");
    eprintln!("SR     = ${sr:04X}");
    eprintln!("ticks  = {}", amiga.tick_count());
    eprintln!("ccks   = {}", amiga.cck_count());

    eprintln!("\n=== Chipset ===");
    eprintln!("DMACON  = ${:04X}", amiga.dmacon());
    eprintln!("INTENA  = ${:04X}", amiga.intena());
    eprintln!("INTREQ  = ${:04X}", amiga.intreq());
    eprintln!("BPLCON0 = ${:04X}", amiga.bplcon0());
    for i in 0..8 {
        eprintln!("COLOR{i:02} = ${:04X}", amiga.color(i));
    }

    eprintln!("\n=== ExecBase ===");
    let exec_base = chip_long(&amiga, 0x0000_0004);
    eprintln!("ExecBase = ${exec_base:08X}");
    if exec_base == 0 {
        eprintln!("(ExecBase uninitialised — can't walk)");
        return;
    }
    let this_task = read_long(&amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
    let idle = read_long(&amiga, exec_base.wrapping_add(EXEC_IDLE_COUNT));
    let disp = read_long(&amiga, exec_base.wrapping_add(EXEC_DISP_COUNT));
    let quantum = amiga.read_word(exec_base.wrapping_add(EXEC_QUANTUM));
    let sys_flags = amiga.read_word(exec_base.wrapping_add(EXEC_SYS_FLAGS));
    let td_nest = u32::from(amiga.read_word(exec_base.wrapping_add(EXEC_TD_NEST_CNT)));
    eprintln!("ThisTask = ${this_task:08X}");
    eprintln!("IdleCount = {idle}  (scheduler idle ticks)");
    eprintln!("DispCount = {disp}  (task dispatch count)");
    eprintln!("Quantum   = {quantum}");
    eprintln!("SysFlags  = ${sys_flags:04X}");
    eprintln!("TDNestCnt = ${td_nest:02X}  (task-disable nest)");

    // TaskReady is a struct List at ExecBase+406. Head pointer at +0,
    // Tail (NULL sentinel) at +4, TailPred at +8. An empty list has
    // head pointing at the address of Tail, and head's ln_Succ is
    // NULL. So we can detect empty by reading head then head->ln_Succ.
    let taskready_addr = exec_base.wrapping_add(EXEC_TASK_READY);
    let head = read_long(&amiga, taskready_addr);
    let tail_addr = taskready_addr.wrapping_add(4);
    eprintln!(
        "\n=== TaskReady List @ ${taskready_addr:08X} ===\n\
         head = ${head:08X}  (tail-sentinel = ${tail_addr:08X})"
    );
    if head == tail_addr {
        eprintln!("→ READY LIST IS EMPTY — Exec is idle, waiting for a signal");
    } else {
        eprintln!("→ head points to a real node — tasks are ready");
        let mut node = head;
        for i in 0..4 {
            if node == 0 || node == tail_addr {
                break;
            }
            let succ = read_long(&amiga, node);
            eprintln!("  node[{i}] = ${node:08X}  (succ = ${succ:08X})");
            node = succ;
        }
    }

    eprintln!("\n=== Framebuffer ===");
    let fb = amiga.denise().framebuffer();
    let (w, h) = amiga.denise().framebuffer_size();
    let mut distinct: std::collections::BTreeMap<u32, u32> =
        std::collections::BTreeMap::new();
    for &px in fb.iter() {
        *distinct.entry(px).or_insert(0) += 1;
    }
    let total = w * h;
    eprintln!("framebuffer = {w}×{h} = {total} pixels");
    eprintln!("distinct colours: {}", distinct.len());
    for (c, n) in distinct.iter().take(8) {
        let pct = (*n as f64 / total as f64) * 100.0;
        eprintln!("  ${c:08X}: {n:7} px ({pct:5.1}%)");
    }
}
