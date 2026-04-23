//! Trap exec.library's Wait / Signal / Cause / SetSignal LVOs to
//! discover which signals flow and which don't during the fresh OCS
//! boot. We already know three tasks are waiting forever — this
//! narrows WHICH waits are the stuck ones.
//!
//! Called by convention:
//!
//!   ULONG Wait(signals)         — D0 = signals
//!   void  Signal(task, signals) — A1 = task, D0 = signals
//!   void  Cause(interrupt)      — A1 = interrupt
//!   ULONG SetSignal(newSig,mask)— D0 = newSig, D1 = mask
//!
//! Source task for any call = ThisTask from ExecBase.
//!
//! For every call, we capture (source task name, target task name
//! if applicable, signal bitmask). After 200 frames, we print:
//!   - total call counts per LVO
//!   - the first N of each kind with arguments
//!   - signal-traffic summary: (source, target, mask) → count
//!
//! This lets us answer:
//!   - Are interrupts firing Cause()? If so, who are they trying
//!     to wake?
//!   - Are any tasks ever Signal()ed directly?
//!   - Which task is stuck Wait()ing for which bitmask?

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::collections::BTreeMap;
use std::path::PathBuf;

// ExecBase offsets.
const EXEC_THIS_TASK: u32 = 276;

// Node offsets.
const LN_NAME: u32 = 10;

// exec.library V34 LVOs (offsets are negative from ExecBase).
const LVO_WAIT: i32 = -318;
const LVO_SIGNAL: i32 = -324;
const LVO_CAUSE: i32 = -180;
const LVO_SET_SIGNAL: i32 = -306;

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
        eprintln!("  LVO {lvo} at ${slot:08X}: not a JMP (op=${opcode:04X})");
        return None;
    }
    Some(read_long(amiga, slot.wrapping_add(2)))
}

#[derive(Default)]
struct Counts {
    wait: u64,
    signal: u64,
    cause: u64,
    set_signal: u64,
}

#[derive(Debug)]
struct Event {
    kind: &'static str,
    source: String,
    target: String,
    mask: u32,
    extra: String,
}

fn run(amiga: &mut AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    // Phase 1: 200 frames — boot to idle.
    for _ in 0..(200 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    let exec_base = read_long(amiga, 0x0000_0004);
    eprintln!("ExecBase = ${exec_base:08X}");

    let Some(wait_ep) = resolve_lvo(amiga, exec_base, LVO_WAIT) else {
        return;
    };
    let Some(signal_ep) = resolve_lvo(amiga, exec_base, LVO_SIGNAL) else {
        return;
    };
    let Some(cause_ep) = resolve_lvo(amiga, exec_base, LVO_CAUSE) else {
        return;
    };
    let Some(set_signal_ep) = resolve_lvo(amiga, exec_base, LVO_SET_SIGNAL) else {
        return;
    };

    eprintln!("exec.library entry points:");
    eprintln!("  Wait      = ${wait_ep:08X}");
    eprintln!("  Signal    = ${signal_ep:08X}");
    eprintln!("  Cause     = ${cause_ep:08X}");
    eprintln!("  SetSignal = ${set_signal_ep:08X}");

    // Edge-detect each LVO hit: record only transitions
    // (prev_pc != ep && curr_pc == ep).
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut counts = Counts::default();
    let mut events: Vec<Event> = Vec::new();
    let mut signal_traffic: BTreeMap<(String, String, u32), u64> = BTreeMap::new();
    let mut wait_traffic: BTreeMap<(String, u32), u64> = BTreeMap::new();
    let max_logged_events = 60;

    for _ in 0..(200 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        // Transition into an LVO entry.
        if pc == wait_ep {
            counts.wait += 1;
            let this_task = read_long(amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
            let src = task_name(amiga, this_task);
            let mask = amiga.cpu().regs.d[0];
            *wait_traffic.entry((src.clone(), mask)).or_insert(0) += 1;
            if events.len() < max_logged_events {
                events.push(Event {
                    kind: "Wait",
                    source: src,
                    target: String::new(),
                    mask,
                    extra: String::new(),
                });
            }
        } else if pc == signal_ep {
            counts.signal += 1;
            let this_task = read_long(amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
            let src = task_name(amiga, this_task);
            let target = amiga.cpu().regs.a[1];
            let tgt = task_name(amiga, target);
            let mask = amiga.cpu().regs.d[0];
            *signal_traffic
                .entry((src.clone(), tgt.clone(), mask))
                .or_insert(0) += 1;
            if events.len() < max_logged_events {
                events.push(Event {
                    kind: "Signal",
                    source: src,
                    target: tgt,
                    mask,
                    extra: format!("target=${target:08X}"),
                });
            }
        } else if pc == cause_ep {
            counts.cause += 1;
            let this_task = read_long(amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
            let src = task_name(amiga, this_task);
            let intr = amiga.cpu().regs.a[1];
            if events.len() < max_logged_events {
                events.push(Event {
                    kind: "Cause",
                    source: src,
                    target: String::new(),
                    mask: 0,
                    extra: format!("interrupt=${intr:08X}"),
                });
            }
        } else if pc == set_signal_ep {
            counts.set_signal += 1;
        }
        prev_pc = pc;
    }

    eprintln!(
        "\n=== Phase 2 LVO call counts (200 frames = ~{} ticks) ===",
        200 * PAL_FRAME_TICKS
    );
    eprintln!("  Wait      = {}", counts.wait);
    eprintln!("  Signal    = {}", counts.signal);
    eprintln!("  Cause     = {}", counts.cause);
    eprintln!("  SetSignal = {}", counts.set_signal);

    if !wait_traffic.is_empty() {
        eprintln!("\n=== Wait traffic (source → mask) ===");
        for ((src, mask), count) in &wait_traffic {
            eprintln!("  {count:>5} × {src:<20} waits on ${mask:08X}");
        }
    }

    if !signal_traffic.is_empty() {
        eprintln!("\n=== Signal traffic (source → target, mask) ===");
        for ((src, tgt, mask), count) in &signal_traffic {
            eprintln!("  {count:>5} × {src:<20} → {tgt:<20} mask=${mask:08X}");
        }
    }

    if !events.is_empty() {
        eprintln!("\n=== First {} events ===", events.len());
        for e in &events {
            match e.kind {
                "Wait" => eprintln!("  Wait: {} mask=${:08X}", e.source, e.mask),
                "Signal" => eprintln!(
                    "  Signal: {} → {} mask=${:08X}  [{}]",
                    e.source, e.target, e.mask, e.extra
                ),
                "Cause" => eprintln!("  Cause: {} [{}]", e.source, e.extra),
                _ => {}
            }
        }
    }

    eprintln!("\n=== Interpretation ===");
    if counts.cause == 0 {
        eprintln!(
            "• Cause() never called → no software-interrupt handler runs.\n  \
            VBL / CIA / Paula interrupts are not scheduling any deferred\n  \
            work. This is likely the blocker — the tasks waiting on\n  \
            signals have nothing generating those signals."
        );
    }
    if counts.signal == 0 {
        eprintln!(
            "• Signal() never called → no task is ever waking another task.\n  \
            Boot is fully quiescent after init."
        );
    }
    if counts.wait > 3 {
        eprintln!(
            "• Wait() called {} times → tasks are going back to sleep after\n  \
            receiving signals. Some signal traffic exists.",
            counts.wait
        );
    }
}

#[test]
#[ignore]
fn trap_exec_signal_path() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only");
}
