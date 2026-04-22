//! Trap exec.library message-port + IO LVOs to see whether the
//! strap/boot code ever tries to kick off a disk read via
//! \`DoIO(trackdisk_request)\`.
//!
//! The dsk_writes observation showed only one DSKLEN write (a
//! disarm during Paula init) in 400 frames. So Paula DMA isn't
//! the blocker. trackdisk.device's wait on \$00000400 is almost
//! certainly its MP_SIGBIT — set by \`PutMsg\` when someone
//! calls \`DoIO\` to send it an IORequest.
//!
//! If DoIO never fires on trackdisk, no one has tried to read
//! the boot block → the insert-disk screen setup never runs.
//! We need to trace what's upstream of that call.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::collections::BTreeMap;
use std::path::PathBuf;

const EXEC_THIS_TASK: u32 = 276;
const EXEC_LIB_LIST: u32 = 378;
const LN_SUCC: u32 = 0;
const LN_NAME: u32 = 10;

// exec.library V34 LVOs (all -offset from exec base).
const LVO_PUT_MSG: i32 = -366;
const LVO_GET_MSG: i32 = -372;
const LVO_REPLY_MSG: i32 = -378;
const LVO_WAIT_PORT: i32 = -384;
const LVO_FIND_PORT: i32 = -390;
const LVO_DO_IO: i32 = -456;
const LVO_SEND_IO: i32 = -462;

// IORequest field offsets (exec/io.h).
const IO_DEVICE: u32 = 20; // LN + MN, then io_Device (APTR)
const IO_UNIT: u32 = 24;
const IO_COMMAND: u32 = 28; // UWORD
const IO_FLAGS: u32 = 30; // UBYTE
const IO_ERROR: u32 = 31; // BYTE

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
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

fn task_name(amiga: &AmigaOcs, task_addr: u32) -> String {
    if task_addr == 0 {
        return "<null>".into();
    }
    let name_ptr = read_long(amiga, task_addr.wrapping_add(LN_NAME));
    let name = read_cstring(amiga, name_ptr, 32);
    if name.is_empty() {
        format!("<addr=${task_addr:08X}>")
    } else {
        name
    }
}

fn resolve_lvo(amiga: &AmigaOcs, base: u32, lvo: i32) -> Option<u32> {
    let slot = (base as i64 + lvo as i64) as u32;
    let opcode = amiga.read_word(slot);
    if opcode != 0x4EF9 {
        return None;
    }
    Some(read_long(amiga, slot.wrapping_add(2)))
}

/// Describe an IORequest: device-name, unit, command.
fn describe_iorequest(amiga: &AmigaOcs, io: u32) -> String {
    if io == 0 {
        return "<null>".into();
    }
    let device = read_long(amiga, io.wrapping_add(IO_DEVICE));
    let unit = read_long(amiga, io.wrapping_add(IO_UNIT));
    let command = read_word(amiga, io.wrapping_add(IO_COMMAND));
    let flags = read_byte(amiga, io.wrapping_add(IO_FLAGS));
    let dev_name = if device == 0 {
        "<null>".into()
    } else {
        // device points at a Library / Device struct; its LN_NAME is
        // at offset 10 of the Node at offset 0.
        let np = read_long(amiga, device.wrapping_add(LN_NAME));
        read_cstring(amiga, np, 32)
    };
    format!(
        "IO @ ${io:08X}  device={dev_name}  unit=${unit:08X}  cmd=${command:04X}  flags=${flags:02X}"
    )
}

fn run(amiga: &mut AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    // Phase 1: 200 frames to reach idle.
    for _ in 0..(200 * PAL_FRAME_TICKS) {
        amiga.tick();
    }
    let exec_base = read_long(amiga, 0x0000_0004);
    eprintln!("ExecBase = ${exec_base:08X}");

    let targets = [
        ("PutMsg  ", resolve_lvo(amiga, exec_base, LVO_PUT_MSG)),
        ("GetMsg  ", resolve_lvo(amiga, exec_base, LVO_GET_MSG)),
        ("ReplyMsg", resolve_lvo(amiga, exec_base, LVO_REPLY_MSG)),
        ("WaitPort", resolve_lvo(amiga, exec_base, LVO_WAIT_PORT)),
        ("FindPort", resolve_lvo(amiga, exec_base, LVO_FIND_PORT)),
        ("DoIO    ", resolve_lvo(amiga, exec_base, LVO_DO_IO)),
        ("SendIO  ", resolve_lvo(amiga, exec_base, LVO_SEND_IO)),
    ];

    eprintln!("\n=== LVO entry points ===");
    for (name, ep) in &targets {
        match ep {
            Some(ep) => eprintln!("  {name} = ${ep:08X}"),
            None => eprintln!("  {name} = (not resolved)"),
        }
    }

    let mut prev_pc = amiga.cpu().regs.pc;
    let mut counts: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut events: Vec<String> = Vec::new();
    let mut find_port_hits: Vec<(String, String)> = Vec::new();

    // Scan phase 2: 400 more frames — extend the window so any
    // slow-to-reach strap-style DoIO still shows up.
    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        for (name, ep) in &targets {
            if let Some(ep) = ep
                && pc == *ep
            {
                *counts.entry(*name).or_insert(0) += 1;
                // Capture the arguments we care about per LVO.
                let this_task = read_long(amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
                let src = task_name(amiga, this_task);
                let text = match name.trim_end() {
                    "PutMsg" => {
                        // PutMsg(port A0, message A1)
                        let port = amiga.cpu().regs.a[0];
                        let msg = amiga.cpu().regs.a[1];
                        let port_name_ptr = read_long(amiga, port.wrapping_add(LN_NAME));
                        let port_name = read_cstring(amiga, port_name_ptr, 32);
                        format!("PutMsg src={src} port=${port:08X}({port_name}) msg=${msg:08X}")
                    }
                    "DoIO" | "SendIO" => {
                        // DoIO(iorequest A1)
                        let io = amiga.cpu().regs.a[1];
                        format!("{name} src={src} {}", describe_iorequest(amiga, io))
                    }
                    "WaitPort" => {
                        // WaitPort(port A0)
                        let port = amiga.cpu().regs.a[0];
                        let port_name_ptr = read_long(amiga, port.wrapping_add(LN_NAME));
                        let port_name = read_cstring(amiga, port_name_ptr, 32);
                        format!("WaitPort src={src} port=${port:08X}({port_name})")
                    }
                    "FindPort" => {
                        // FindPort(name A1)
                        let name_ptr = amiga.cpu().regs.a[1];
                        let port_name = read_cstring(amiga, name_ptr, 32);
                        find_port_hits.push((src.clone(), port_name.clone()));
                        format!("FindPort src={src} name=\"{port_name}\"")
                    }
                    _ => format!("{name} src={src}"),
                };
                if events.len() < 80 {
                    events.push(text);
                }
            }
        }
        prev_pc = pc;
    }

    eprintln!("\n=== LVO call counts (400 frames phase 2) ===");
    for (name, _) in &targets {
        let c = counts.get(name).copied().unwrap_or(0);
        eprintln!("  {name} = {c}");
    }

    if !events.is_empty() {
        eprintln!("\n=== First {} events ===", events.len());
        for e in &events {
            eprintln!("  {e}");
        }
    }

    if !find_port_hits.is_empty() {
        eprintln!("\n=== FindPort targets (who is looking for what port) ===");
        let mut agg: BTreeMap<(String, String), u64> = BTreeMap::new();
        for (src, p) in &find_port_hits {
            *agg.entry((src.clone(), p.clone())).or_insert(0) += 1;
        }
        for ((src, p), c) in &agg {
            eprintln!("  {c:>4} × {src} → FindPort(\"{p}\")");
        }
    }

    eprintln!("\n=== Interpretation ===");
    let doio = counts.get("DoIO    ").copied().unwrap_or(0);
    let sendio = counts.get("SendIO  ").copied().unwrap_or(0);
    let putmsg = counts.get("PutMsg  ").copied().unwrap_or(0);
    if doio + sendio + putmsg == 0 {
        eprintln!(
            "• No PutMsg / DoIO / SendIO in phase 2 → the strap/boot code\n  \
            never reaches the point where it would hand a request to\n  \
            trackdisk.device or any other device. The stall is\n  \
            upstream of any disk activity."
        );
    } else {
        eprintln!("• PutMsg={putmsg}, DoIO={doio}, SendIO={sendio} — there IS message traffic.");
        eprintln!("  Check whether any targets trackdisk.device.");
    }
}

#[test]
#[ignore]
fn trap_msg_port_lvos() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only");
}
