//! Trace what happens to the trackdisk port after strap's DoIO(CMD_READ).
//!
//! After we catch strap at $FE859C issuing DoIO for the bootblock,
//! we walk the trackdisk Unit's message port to see:
//!   - is the IORequest actually queued?
//!   - is trackdisk's task waiting on the right signal bit?
//!   - does the port head move (message consumed) over time?

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::Amiga;
use std::fs;
use std::path::Path;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
    let loaded = read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap();
    let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();
    amiga.insert_disk(adf);
    amiga.floppy.acknowledge_disk_change();

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    let rl = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };
    let rw = |amiga: &Amiga, a: u32| amiga.memory.read_word(a);
    let rb = |amiga: &Amiga, a: u32| amiga.memory.read_byte(a);

    let mut dumps: Vec<(u64, String)> = Vec::new();
    let mut triggered = false;

    let total_ticks = 500u64 * ccks_per_frame;
    for tick in 0..total_ticks {
        amiga.tick_cck();
        if !triggered && amiga.cpu.instr_start_pc == 0x00FE859C {
            triggered = true;
            dumps.push((tick, "strap about to call DoIO(CMD_READ)".into()));
        }
        if triggered {
            // Every 50 frames, snapshot trackdisk's port state.
            if tick % (50 * ccks_per_frame) == 0 {
                let td_base = 0x00C03AA4u32;
                // Device structure: after Library node (+0..+33) comes
                // dd_Unit0 pointer at some offset. Unit0 pointer is at
                // device + $22 (approx) for 1.3. But trackdisk-specific:
                // dev + $34 = *ExecBase (as BeginIO uses).
                // Easier: find the Unit struct by walking BeginIO's
                // logic. Skip and dump device base area.
                let mut buf = String::new();
                buf.push_str(&format!("tick={tick}: td_base+0x20:"));
                for off in 0..0x20 {
                    let a = td_base + 0x20 + off;
                    buf.push_str(&format!(" {:02X}", rb(&amiga, a)));
                }
                dumps.push((tick, buf));
            }
        }
    }

    for (tick, line) in &dumps {
        println!("[{tick}] {line}");
    }
    println!("\nFinal CPU PC=${:08X}", amiga.cpu.instr_start_pc);
    println!("INTENA=${:04X} INTREQ=${:04X}", amiga.paula.intena, amiga.paula.intreq);
    println!("Floppy: motor_on={} spinning={} selected={} cyl={}",
        amiga.floppy.motor_on(), amiga.floppy.motor_spinning(),
        amiga.floppy.selected(), amiga.floppy.cylinder());

    // Also — resolve trackdisk's task entry and check its port.
    // Task reads msg from port via Wait/GetMsg. If GetMsg retrieves one,
    // port head moves.
    // Walk ExecBase.TaskWait (offset $10A) and find a task whose
    // name == "trackdisk.device".
    let execbase = rl(&amiga, 0x4);
    let task_ready = execbase.wrapping_add(0x0196);
    let task_wait = execbase.wrapping_add(0x01A4);
    println!("\nExecBase=${execbase:08X}");
    println!("TaskReady list head @ ${task_ready:08X} = ${:08X}", rl(&amiga, task_ready));
    let mut node = rl(&amiga, task_wait);
    println!("TaskWait list head @ ${task_wait:08X} = ${node:08X}");
    let mut count = 0;
    while node != 0 && count < 10 {
        let succ = rl(&amiga, node);
        let name_ptr = rl(&amiga, node.wrapping_add(0x0A));
        let mut name = String::new();
        let valid_ptr = (0xC0_0000..0xC8_0000).contains(&name_ptr)
            || (0x400..0x80000).contains(&name_ptr)
            || (0xF0_0000..0x100_0000).contains(&name_ptr);
        if valid_ptr {
            for i in 0..48 {
                let c = rb(&amiga, name_ptr.wrapping_add(i));
                if c == 0 { break; }
                if c.is_ascii_graphic() || c == b' ' { name.push(c as char); }
                else { name.push('?'); break; }
            }
        }
        let state = rb(&amiga, node.wrapping_add(0x0F));
        let sig_wait = rl(&amiga, node.wrapping_add(0x16));
        let sig_recvd = rl(&amiga, node.wrapping_add(0x1A));
        let sig_alloc = rl(&amiga, node.wrapping_add(0x12));
        println!(
            "  task @ ${node:08X} name_ptr=${name_ptr:08X} '{name}' state=${state:02X} sig_alloc=${sig_alloc:08X} sig_wait=${sig_wait:08X} sig_recvd=${sig_recvd:08X}"
        );
        if succ == 0 { break; }
        node = succ;
        count += 1;
    }

    // Find strap's task — should also be waiting on a signal.
    // ThisTask is at ExecBase+$114 (IntVects comes before, SoftVer at $114).
    // Actual ThisTask lives at offset $114... verify by dumping a few candidates.
    for off in [0x114u32, 0x118, 0x11C, 0x120, 0x124] {
        let v = rl(&amiga, execbase.wrapping_add(off));
        println!("ExecBase+${off:03X} = ${v:08X}");
    }

    // Walk TaskReady list too.
    println!("\nTaskReady walk:");
    let mut node = rl(&amiga, task_ready);
    let mut count = 0;
    while node != 0 && count < 10 {
        let succ = rl(&amiga, node);
        let name_ptr = rl(&amiga, node.wrapping_add(0x0A));
        let mut name = String::new();
        let valid_ptr = (0xC0_0000..0xC8_0000).contains(&name_ptr)
            || (0x400..0x80000).contains(&name_ptr)
            || (0xF0_0000..0x100_0000).contains(&name_ptr)
            || (0xF8_0000..0x100_0000).contains(&name_ptr);
        if valid_ptr {
            for i in 0..48 {
                let c = rb(&amiga, name_ptr.wrapping_add(i));
                if c == 0 { break; }
                if c.is_ascii_graphic() || c == b' ' { name.push(c as char); }
                else { name.push('?'); break; }
            }
        }
        let state = rb(&amiga, node.wrapping_add(0x0F));
        let sig_wait = rl(&amiga, node.wrapping_add(0x16));
        let sig_recvd = rl(&amiga, node.wrapping_add(0x1A));
        let sig_alloc = rl(&amiga, node.wrapping_add(0x12));
        println!(
            "  task @ ${node:08X} name_ptr=${name_ptr:08X} '{name}' state=${state:02X} sig_alloc=${sig_alloc:08X} sig_wait=${sig_wait:08X} sig_recvd=${sig_recvd:08X}"
        );
        if succ == 0 { break; }
        node = succ;
        count += 1;
    }
}
