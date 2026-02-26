//! Real Kickstart 1.3 boot test for machine-amiga.

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel};
use machine_amiga::commodore_agnus_ocs;
use machine_amiga::commodore_denise_ocs::{FB_HEIGHT, FB_WIDTH};
use machine_amiga::memory::Memory;
use motorola_68000::cpu::State;
use std::fs;

fn reg_name(offset: u16) -> &'static str {
    match offset {
        0x080 => "COP1LCH",
        0x082 => "COP1LCL",
        0x084 => "COP2LCH",
        0x086 => "COP2LCL",
        0x08A => "COPJMP2",
        0x088 => "COPJMP1",
        0x096 => "DMACON",
        0x09A => "INTENA",
        0x09C => "INTREQ",
        0x100 => "BPLCON0",
        0x102 => "BPLCON1",
        0x104 => "BPLCON2",
        0x108 => "BPL1MOD",
        0x10A => "BPL2MOD",
        0x0E0 => "BPL1PTH",
        0x0E2 => "BPL1PTL",
        0x0E4 => "BPL2PTH",
        0x0E6 => "BPL2PTL",
        0x0E8 => "BPL3PTH",
        0x0EA => "BPL3PTL",
        0x180 => "COLOR00",
        0x182 => "COLOR01",
        0x184 => "COLOR02",
        0x186 => "COLOR03",
        0x188 => "COLOR04",
        0x18A => "COLOR05",
        0x18C => "COLOR06",
        0x18E => "COLOR07",
        0x190 => "COLOR08",
        0x192 => "COLOR09",
        0x194 => "COLOR10",
        0x196 => "COLOR11",
        0x198 => "COLOR12",
        0x19A => "COLOR13",
        0x19C => "COLOR14",
        0x19E => "COLOR15",
        0x1A0 => "COLOR16",
        0x1A2 => "COLOR17",
        0x1A4 => "COLOR18",
        0x1A6 => "COLOR19",
        0x120..=0x13E => "SPRxPT",
        0x140..=0x17E => "SPRxDATA",
        _ => "?",
    }
}

#[test]
#[ignore] // Requires real KS1.3 ROM at roms/kick13.rom
fn test_boot_kick13() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    println!(
        "Reset: SSP=${:08X} PC=${:08X} SR=${:04X}",
        amiga.cpu.regs.ssp, amiga.cpu.regs.pc, amiga.cpu.regs.sr
    );

    let total_ticks: u64 = 850_000_000; // ~30 seconds PAL
    let report_interval: u64 = 28_375_160; // ~1 second

    let mut last_report = 0u64;
    let mut last_pc = 0u32;
    let mut stuck_count = 0u32;
    let mut strap_trace_active = false;
    let mut strap_trace_count = 0u32;
    let mut strap_prev_pc: u32 = 0;
    let mut prev_sig_wait_lo: u16 = 0;
    let mut doio3_seen = false;
    let mut loadview_trace = false;
    let mut loadview_pcs: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();

    // Track PC ranges to understand boot progress
    let mut pc_ranges: [u64; 16] = [0; 16];
    let mut range_ticks: u64 = 0;
    let mut l3_handler_count: u64 = 0;
    let mut timer_server_count: u64 = 0;
    let mut vertb_dispatch_count: u64 = 0;
    let mut battclock_done = false;

    for i in 0..total_ticks {
        amiga.tick();

        // Simulate battclock: keep CIA-A TOD high byte non-zero.
        // timer.device's VERTB handler reads TOD to compute system time,
        // then clears TOD_HI to mark it consumed. We re-assert the high
        // byte each tick, like a real RTC/battclock would.
        if battclock_done {
            let tod = amiga.cia_a.tod_counter();
            if tod < 0x010000 {
                amiga.cia_a.set_tod_counter(0x010000 | tod);
            }
        } else if i >= 2 * PAL_CRYSTAL_HZ {
            let current_tod = amiga.cia_a.tod_counter();
            amiga.cia_a.set_tod_counter(0x010000 | current_tod);
            battclock_done = true;
        }

        // Snapshot PC after Draw() should have returned
        if (i >= 90_061_000 && i <= 90_065_000 && i % 200 == 0) || i == 90_100_000 {
            eprintln!(
                "[tick {}] SNAPSHOT: PC=${:08X} ipc=${:08X} SR=${:04X} SP=${:08X} D0=${:08X} D1=${:08X} A6=${:08X}",
                i,
                amiga.cpu.regs.pc,
                amiga.cpu.instr_start_pc,
                amiga.cpu.regs.sr,
                amiga.cpu.regs.a(7),
                amiga.cpu.regs.d[0],
                amiga.cpu.regs.d[1],
                amiga.cpu.regs.a(6)
            );
        }

        // Count specific addresses (every CPU tick = every 4 master ticks)
        if i % 4 == 0 {
            let pc = amiga.cpu.regs.pc;
            if pc == 0x00FC0D14 {
                l3_handler_count += 1;
            }
            if pc == 0x00FC0D44 {
                vertb_dispatch_count += 1;
            } // BTST #5,D1 for VERTB
            if pc == 0x00FE935A {
                timer_server_count += 1;
            }
            // Collect LoadView execution trace
            if loadview_trace {
                loadview_pcs.insert(amiga.cpu.instr_start_pc);
            }
            // Trace ciab.resource LVO calls (from jump table)
            let ipc = amiga.cpu.instr_start_pc;
            match ipc {
                0x00FC46F8 => eprintln!(
                    "[tick {}] ciab.resource AddICRVector entry (D0=${:08X} A1=${:08X} A6=${:08X})",
                    i,
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.a(1),
                    amiga.cpu.regs.a(6)
                ),
                0x00FC474E => eprintln!("[tick {}] ciab.resource RemICRVector entry", i),
                0x00FC4772 => eprintln!(
                    "[tick {}] ciab.resource AbleICR entry (D0=${:08X})",
                    i, amiga.cpu.regs.d[0]
                ),
                0x00FC4790 => eprintln!(
                    "[tick {}] ciab.resource SetICR entry (D0=${:08X})",
                    i, amiga.cpu.regs.d[0]
                ),
                // Display routine milestones (using instr_start_pc for accuracy)
                // Display subroutine entry and key steps
                0x00FE8732 => eprintln!(
                    "[tick {}] DISP: $FE8732 entry (open gfx.lib) A6=${:08X}",
                    i,
                    amiga.cpu.regs.a(6)
                ),
                0x00FE8738 => eprintln!(
                    "[tick {}] DISP: OpenLibrary JSR A6=${:08X}",
                    i,
                    amiga.cpu.regs.a(6)
                ),
                0x00FE873C => eprintln!(
                    "[tick {}] DISP: OpenLibrary returned D0=${:08X}",
                    i, amiga.cpu.regs.d[0]
                ),
                0x00FE8740 => eprintln!(
                    "[tick {}] DISP: BNE check D0=${:08X} (0=fail)",
                    i, amiga.cpu.regs.d[0]
                ),
                0x00FE875A => eprintln!("[tick {}] DISP: display setup entry", i),
                0x00FE875E => {
                    let a5 = amiga.cpu.regs.a(5) as usize;
                    let val = if a5 + 3 < amiga.memory.chip_ram.len() {
                        let r = &amiga.memory.chip_ram;
                        (r[a5] as u32) << 24
                            | (r[a5 + 1] as u32) << 16
                            | (r[a5 + 2] as u32) << 8
                            | r[a5 + 3] as u32
                    } else {
                        0xDEADBEEF
                    };
                    eprintln!(
                        "[tick {}] DISP: load A6 from $0(A5) A5=${:08X} val=${:08X}",
                        i, a5, val
                    );
                }
                0x00FE876E => eprintln!(
                    "[tick {}] DISP: AllocMem JSR (A6=${:08X})",
                    i,
                    amiga.cpu.regs.a(6)
                ),
                0x00FE8794 => eprintln!(
                    "[tick {}] DISP: AllocMem success mem=${:08X}",
                    i, amiga.cpu.regs.d[0]
                ),
                0x00FE887C => eprintln!(
                    "[tick {}] DISP: MakeVPort (A6=${:08X})",
                    i,
                    amiga.cpu.regs.a(6)
                ),
                0x00FE8882 => eprintln!("[tick {}] DISP: MrgCop", i),
                0x00FE8888 => {
                    eprintln!(
                        "[tick {}] DISP: LoadView (A6=${:08X})",
                        i,
                        amiga.cpu.regs.a(6)
                    );
                    loadview_trace = true;
                }
                0x00FE8896 => {
                    eprintln!("[tick {}] DISP: LoadRGB4", i);
                    if loadview_trace {
                        loadview_trace = false;
                        eprintln!("  LoadView PCs visited ({} unique):", loadview_pcs.len());
                        for pc in &loadview_pcs {
                            eprintln!("    ${:08X}", pc);
                        }
                    }
                }
                0x00FE88A6 => eprintln!("[tick {}] DISP: SetRast", i),
                // Drawing loop function calls — trace every JSR
                0x00FE88A8 => eprintln!("[tick {}] DRAW: initial SetAPen", i),
                0x00FE88CE => eprintln!("[tick {}] DRAW: SetAPen ($FF path)", i),
                0x00FE88DC => eprintln!("[tick {}] DRAW: Move ($FF path)", i),
                0x00FE88F4 => eprintln!("[tick {}] DRAW: SetAPen ($FE path)", i),
                0x00FE8904 => eprintln!("[tick {}] DRAW: PolyDraw ($FE path)", i),
                0x00FE8918 => eprintln!(
                    "[tick {}] DRAW: Draw (default path) A6=${:08X} D0=${:08X} D1=${:08X} A1=${:08X}",
                    i,
                    amiga.cpu.regs.a(6),
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.d[1],
                    amiga.cpu.regs.a(1)
                ),
                // Address error / Bus error exception handlers
                0x00FC05B4 => eprintln!(
                    "[tick {}] EXCEPTION: default handler at $FC05B4 (SR=${:04X} PC=${:08X})",
                    i, amiga.cpu.regs.sr, amiga.cpu.regs.pc
                ),
                // Exec Alert function (called on task crash)
                0x00FC0582 => eprintln!(
                    "[tick {}] EXEC: Alert entry D7=${:08X} SP=${:08X}",
                    i,
                    amiga.cpu.regs.d[7],
                    amiga.cpu.regs.a(7)
                ),
                0x00FE88AC => {} // loop top — too noisy, skip
                0x00FE88BE => eprintln!("[tick {}] DRAW: $FF,$FF terminator → exit loop", i),
                0x00FE891C => {
                    eprintln!("[tick {}] DISP: Vector drawing done → BltTemplate next", i)
                }
                // BltTemplate loop milestones
                0x00FE891E => eprintln!("[tick {}] DISP: BltTemplate loop entry", i),
                0x00FE8966 => eprintln!("[tick {}] DISP: BltTemplate JSR -$24(a6)", i),
                0x00FE896C => eprintln!("[tick {}] DISP: BltTemplate loop done → final palette", i),
                0x00FE8974 => eprintln!("[tick {}] DISP: Final LoadRGB4", i),
                0x00FE897A => eprintln!("[tick {}] DISP: WaitTOF", i),
                0x00FE8982 => eprintln!("[tick {}] DISP: RTS (display routine returns!)", i),
                // STRAP disk-wait loop: BPLEN enable
                0x00FE861A => eprintln!("[tick {}] STRAP: SET BPLEN $8100 → DMACON", i),
                0x00FE8600 => eprintln!("[tick {}] STRAP: disk-wait loop top", i),
                _ => {}
            }
            // Trace GetUnit flow after DoIO #3 (use instr_start_pc for accurate matching)
            if doio3_seen {
                let ipc = amiga.cpu.instr_start_pc;
                match ipc {
                    // GetUnit: JSR disk.resource LVO -18
                    0x00FEAA6E => eprintln!(
                        "[tick {}] GetUnit: calling disk.resource (A6=${:08X} A1=${:08X})",
                        i,
                        amiga.cpu.regs.a(6),
                        amiga.cpu.regs.a(1)
                    ),
                    // GetUnit: TST.L D0 (result check)
                    0x00FEAA72 => eprintln!(
                        "[tick {}] GetUnit: result D0=${:08X}",
                        i, amiga.cpu.regs.d[0]
                    ),
                    // GetUnit: Wait($0400) retry
                    0x00FEAA78 => eprintln!("[tick {}] GetUnit: FAILED → entering Wait($0400)", i),
                    // GetUnit: success path (EXG A2,A6)
                    0x00FEAA92 => eprintln!("[tick {}] GetUnit: SUCCESS", i),
                    // GetUnit: retry loop (BRA.S $FEAA6A)
                    0x00FEAA90 => eprintln!("[tick {}] GetUnit: retry from Wait($0400)", i),
                    // GiveUnit entry
                    0x00FEAAC2 => eprintln!("[tick {}] GiveUnit entry", i),
                    // DiskStatusCheck entry
                    0x00FE960C => eprintln!(
                        "[tick {}] DiskStatusCheck entry (A6=${:08X})",
                        i,
                        amiga.cpu.regs.a(6)
                    ),
                    // PerformIO entry
                    0x00FE9C9C => {
                        let a1 = amiga.cpu.regs.a(1) as usize;
                        let r = &amiga.memory.chip_ram;
                        let cmd = if a1 + 0x1D < r.len() {
                            ((r[a1 + 0x1C] as u16) << 8 | r[a1 + 0x1D] as u16)
                        } else {
                            0xFFFF
                        };
                        eprintln!("[tick {}] PerformIO entry (cmd={} A1=${:08X})", i, cmd, a1);
                    }
                    // CMD_READ handler (disk read dispatch)
                    0x00FEA3B4 => eprintln!("[tick {}] CMD_READ handler entry", i),
                    // Disk DMA read
                    0x00FEA1A4 => eprintln!("[tick {}] DiskDMARead entry", i),
                    // Motor control
                    0x00FEA0E2 => eprintln!("[tick {}] MotorControl entry", i),
                    // Seek
                    0x00FEA05A => eprintln!("[tick {}] Seek entry", i),
                    // Delay
                    0x00FEA170 => {
                        eprintln!("[tick {}] Delay entry (D0=${:08X})", i, amiga.cpu.regs.d[0])
                    }
                    // timer.device program_timer (unit+$22 function pointer)
                    0x00FE946C => eprintln!(
                        "[tick {}] timer.device program_timer entry (A3=${:08X})",
                        i,
                        amiga.cpu.regs.a(3)
                    ),
                    // timer.device stop_timer (unit+$26 function pointer)
                    0x00FE94B6 => eprintln!("[tick {}] timer.device stop_timer entry", i),
                    // timer.device BeginIO
                    0x00FE9046 => {
                        let a1 = amiga.cpu.regs.a(1) as usize;
                        let r = &amiga.memory.chip_ram;
                        let cmd = if a1 + 0x1D < r.len() {
                            ((r[a1 + 0x1C] as u16) << 8 | r[a1 + 0x1D] as u16)
                        } else {
                            0xFFFF
                        };
                        let io_unit = if a1 + 0x1B < r.len() {
                            ((r[a1 + 0x18] as u32) << 24
                                | (r[a1 + 0x19] as u32) << 16
                                | (r[a1 + 0x1A] as u32) << 8
                                | r[a1 + 0x1B] as u32)
                        } else {
                            0xDEAD
                        };
                        let tv_secs = if a1 + 0x23 < r.len() {
                            ((r[a1 + 0x20] as u32) << 24
                                | (r[a1 + 0x21] as u32) << 16
                                | (r[a1 + 0x22] as u32) << 8
                                | r[a1 + 0x23] as u32)
                        } else {
                            0
                        };
                        let tv_micro = if a1 + 0x27 < r.len() {
                            ((r[a1 + 0x24] as u32) << 24
                                | (r[a1 + 0x25] as u32) << 16
                                | (r[a1 + 0x26] as u32) << 8
                                | r[a1 + 0x27] as u32)
                        } else {
                            0
                        };
                        eprintln!(
                            "[tick {}] timer.device BeginIO entry (A1=${:08X} cmd={} io_Unit=${:08X} tv_secs={} tv_micro={})",
                            i, a1, cmd, io_unit, tv_secs, tv_micro
                        );
                    }
                    _ => {}
                }
            }
            // Monitor trackdisk SigWait changes after DoIO #3
            if doio3_seen {
                let r = &amiga.memory.chip_ram;
                let sw_lo = ((r[0x6236] as u16) << 8) | r[0x6237] as u16;
                if sw_lo != prev_sig_wait_lo {
                    let sw_hi = ((r[0x6234] as u16) << 8) | r[0x6235] as u16;
                    let sr_hi = ((r[0x6238] as u16) << 8) | r[0x6239] as u16;
                    let sr_lo = ((r[0x623A] as u16) << 8) | r[0x623B] as u16;
                    eprintln!(
                        "[tick {}] td SigWait changed: ${:04X}{:04X} (was xxxx{:04X}) SigRecvd=${:04X}{:04X} PC=${:08X} SR=${:04X}",
                        i, sw_hi, sw_lo, prev_sig_wait_lo, sr_hi, sr_lo, pc, amiga.cpu.regs.sr
                    );
                    prev_sig_wait_lo = sw_lo;
                }
            }
            // Track key boot milestones
            // Strap boot flow milestones
            match pc {
                0x00FE8444 => eprintln!("[tick {}] STRAP INIT entry", i),
                0x00FEB0A8 => eprintln!("[tick {}] ROMBOOT INIT entry", i),
                0x00FE8502 => eprintln!("[tick {}] STRAP: OpenDevice(trackdisk)", i),
                0x00FE8518 => eprintln!(
                    "[tick {}] STRAP: Alert (OpenDevice failed!) D7=${:08X}",
                    i, amiga.cpu.regs.d[7]
                ),
                0x00FE8532 => eprintln!(
                    "[tick {}] STRAP: OpenLibrary D0=${:08X}",
                    i, amiga.cpu.regs.d[0]
                ),
                0x00FE855C => eprintln!("[tick {}] STRAP: DoIO #1 (first disk read)", i),
                0x00FE8570 => eprintln!("[tick {}] STRAP: DoIO #2", i),
                0x00FE859C => {
                    eprintln!("[tick {}] STRAP: DoIO #3", i);
                    strap_trace_active = true;
                    doio3_seen = true;
                    prev_sig_wait_lo = ((amiga.memory.chip_ram[0x6236] as u16) << 8)
                        | amiga.memory.chip_ram[0x6237] as u16;
                }
                // Polling loop
                0x00FE8600 => {
                    if !strap_trace_active {
                        eprintln!(
                            "[tick {}] STRAP: Entered polling loop (A2={:08X})",
                            i,
                            amiga.cpu.regs.a(2)
                        );
                        strap_trace_active = true;
                    }
                }
                0x00FE8610 => {
                    // MOVE.L (4,A5),D0 — check if first pass
                    if strap_trace_count < 5 {
                        let a5 = amiga.cpu.regs.a(5);
                        let val = {
                            let r = &amiga.memory.chip_ram;
                            let addr = (a5 + 4) as usize;
                            if addr + 3 < r.len() {
                                (r[addr] as u32) << 24
                                    | (r[addr + 1] as u32) << 16
                                    | (r[addr + 2] as u32) << 8
                                    | r[addr + 3] as u32
                            } else {
                                0xDEAD
                            }
                        };
                        eprintln!(
                            "[tick {}] STRAP: first-pass check (A5+4)=${:08X} A5=${:08X}",
                            i, val, a5
                        );
                    }
                }
                0x00FE8616 => eprintln!("[tick {}] STRAP: BSR to display routine ($FE8732)", i),
                0x00FE8732 => eprintln!("[tick {}] STRAP: Display routine entry", i),
                0x00FE8738 => eprintln!(
                    "[tick {}] STRAP: OpenLibrary (for insert screen) D0=${:08X}",
                    i, amiga.cpu.regs.d[0]
                ),
                0x00FE8750 => eprintln!(
                    "[tick {}] STRAP: Alert (OpenLib failed) D7=${:08X}",
                    i, amiga.cpu.regs.d[7]
                ),
                0x00FE8626 => {
                    if strap_trace_count < 5 {
                        eprintln!("[tick {}] STRAP: TD_MOTOR off", i);
                    }
                }
                0x00FE8630 => {
                    if strap_trace_count < 5 {
                        eprintln!(
                            "[tick {}] STRAP: DoIO(TD_MOTOR) D0={:08X}",
                            i, amiga.cpu.regs.d[0]
                        );
                    }
                }
                0x00FE8638 => {
                    if strap_trace_count < 5 {
                        eprintln!("[tick {}] STRAP: CMD_CHANGENUM setup", i);
                    }
                }
                0x00FE8642 => {
                    if strap_trace_count < 5 {
                        eprintln!("[tick {}] STRAP: DoIO(CMD_CHANGENUM)", i);
                    }
                }
                0x00FE864A => {
                    if strap_trace_count < 10 {
                        // CMP.L ($4C,A5),D2
                        let a5 = amiga.cpu.regs.a(5);
                        let io_actual = {
                            let r = &amiga.memory.chip_ram;
                            let addr = (a5 as usize).wrapping_add(0x4C);
                            if addr + 3 < r.len() {
                                (r[addr] as u32) << 24
                                    | (r[addr + 1] as u32) << 16
                                    | (r[addr + 2] as u32) << 8
                                    | r[addr + 3] as u32
                            } else {
                                0xDEAD
                            }
                        };
                        eprintln!(
                            "[tick {}] STRAP: CMP D2=${:08X} vs io_Actual=${:08X}",
                            i, amiga.cpu.regs.d[2], io_actual
                        );
                        strap_trace_count += 1;
                    }
                }
                0x00FE85E2 => eprintln!(
                    "[tick {}] STRAP: Alert (disk failed) D7=${:08X}",
                    i, amiga.cpu.regs.d[7]
                ),
                0x00FE86AA => eprintln!(
                    "[tick {}] STRAP: Alert (late) D7=${:08X}",
                    i, amiga.cpu.regs.d[7]
                ),
                0x00FE867C => {
                    // Error handler: check io_Error
                    let a5 = amiga.cpu.regs.a(5);
                    let io_error = {
                        let r = &amiga.memory.chip_ram;
                        let addr = (a5 as usize).wrapping_add(0x2C + 0x1F);
                        if addr < r.len() { r[addr] } else { 0xFF }
                    };
                    eprintln!(
                        "[tick {}] STRAP: Error handler, io_Error=${:02X}",
                        i, io_error
                    );
                }
                _ => {}
            }
            // After boot settles, dump trackdisk task on first DoIO #1 hit
            if !strap_trace_active && pc == 0x00FE855C {
                strap_trace_active = true;
                let td = 0x621Eusize;
                let r = &amiga.memory.chip_ram;
                eprintln!("\n=== TRACKDISK TASK DUMP at $621E ===");
                // Dump first 128 bytes as hex
                for row in 0..8 {
                    let base = td + row * 16;
                    let mut hex = String::new();
                    for col in 0..16 {
                        if base + col < r.len() {
                            hex.push_str(&format!("{:02X} ", r[base + col]));
                        }
                    }
                    eprintln!("  ${:04X}: {}", base, hex);
                }
                // Decode key Task fields
                let sig_alloc = (r[td + 0x12] as u32) << 24
                    | (r[td + 0x13] as u32) << 16
                    | (r[td + 0x14] as u32) << 8
                    | r[td + 0x15] as u32;
                let sig_wait = (r[td + 0x16] as u32) << 24
                    | (r[td + 0x17] as u32) << 16
                    | (r[td + 0x18] as u32) << 8
                    | r[td + 0x19] as u32;
                let sig_recvd = (r[td + 0x1A] as u32) << 24
                    | (r[td + 0x1B] as u32) << 16
                    | (r[td + 0x1C] as u32) << 8
                    | r[td + 0x1D] as u32;
                eprintln!(
                    "  SigAlloc=${:08X} SigWait=${:08X} SigRecvd=${:08X}",
                    sig_alloc, sig_wait, sig_recvd
                );

                // Look for MsgPort structures after the Task struct ($5A bytes)
                // MsgPort: Node(14) + Flags(1) + SigBit(1) + SigTask(4) + MsgList(14)
                // Check a few offsets for port-like structures
                for port_off in [0x5Au16, 0x6Eu16, 0x82u16, 0x96u16] {
                    let pa = td + port_off as usize;
                    if pa + 0x22 < r.len() {
                        let flags = r[pa + 0x0E];
                        let sig_bit = r[pa + 0x0F];
                        let sig_task = (r[pa + 0x10] as u32) << 24
                            | (r[pa + 0x11] as u32) << 16
                            | (r[pa + 0x12] as u32) << 8
                            | r[pa + 0x13] as u32;
                        let list_head = (r[pa + 0x14] as u32) << 24
                            | (r[pa + 0x15] as u32) << 16
                            | (r[pa + 0x16] as u32) << 8
                            | r[pa + 0x17] as u32;
                        eprintln!(
                            "  Port candidate at +${:02X} (${}): flags={} sigBit={} sigTask=${:08X} listHead=${:08X}",
                            port_off, pa, flags, sig_bit, sig_task, list_head
                        );
                    }
                }
            }
        }

        // Sample PC every 256 ticks for range tracking
        if i & 0xFF == 0 {
            let pc = amiga.cpu.regs.pc;
            let range = if pc >= 0x00FC0000 && pc < 0x00FD0000 {
                ((pc - 0x00FC0000) >> 12) as usize // 4KB buckets within ROM
            } else if pc < 0x00080000 {
                15 // Chip RAM
            } else {
                14 // Other
            };
            if range < 16 {
                pc_ranges[range] += 1;
            }
            range_ticks += 1;
        }

        // Detect halt
        if matches!(amiga.cpu.state, State::Halted) {
            println!(
                "CPU HALTED at tick {} ({}ms), PC=${:08X} IR=${:04X} SR=${:04X}",
                i,
                i / (PAL_CRYSTAL_HZ / 1000),
                amiga.cpu.regs.pc,
                amiga.cpu.ir,
                amiga.cpu.regs.sr,
            );
            println!(
                "D0-D7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                amiga.cpu.regs.d[0],
                amiga.cpu.regs.d[1],
                amiga.cpu.regs.d[2],
                amiga.cpu.regs.d[3],
                amiga.cpu.regs.d[4],
                amiga.cpu.regs.d[5],
                amiga.cpu.regs.d[6],
                amiga.cpu.regs.d[7]
            );
            println!(
                "A0-A7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                amiga.cpu.regs.a(0),
                amiga.cpu.regs.a(1),
                amiga.cpu.regs.a(2),
                amiga.cpu.regs.a(3),
                amiga.cpu.regs.a(4),
                amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6),
                amiga.cpu.regs.a(7)
            );
            break;
        }

        // Periodic status report
        if i - last_report >= report_interval {
            let pc = amiga.cpu.regs.pc;
            let seconds = i / PAL_CRYSTAL_HZ;
            let sr = amiga.cpu.regs.sr;
            let ipl = (sr >> 8) & 7;
            println!(
                "[{:2}s] PC=${:08X} SR=${:04X}(IPL{}) D0=${:08X} D1=${:08X} A7=${:08X} DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X} CIA-A:TA={:04X}({}) TB={:04X}({}) ICR={:02X}/{:02X}",
                seconds,
                pc,
                sr,
                ipl,
                amiga.cpu.regs.d[0],
                amiga.cpu.regs.d[1],
                amiga.cpu.regs.a(7),
                amiga.agnus.dmacon,
                amiga.paula.intena,
                amiga.paula.intreq,
                amiga.cia_a.timer_a(),
                if amiga.cia_a.timer_a_running() {
                    "RUN"
                } else {
                    "STOP"
                },
                amiga.cia_a.timer_b(),
                if amiga.cia_a.timer_b_running() {
                    "RUN"
                } else {
                    "STOP"
                },
                amiga.cia_a.icr_status(),
                amiga.cia_a.icr_mask(),
            );
            println!(
                "      CIA-B:TA={:04X}({}) TB={:04X}({}) ICR={:02X}/{:02X} COP1LC=${:08X} copper={:?} CIA-A:TOD=${:06X}(halted={}) CIA-B:TOD=${:06X}",
                amiga.cia_b.timer_a(),
                if amiga.cia_b.timer_a_running() {
                    "RUN"
                } else {
                    "STOP"
                },
                amiga.cia_b.timer_b(),
                if amiga.cia_b.timer_b_running() {
                    "RUN"
                } else {
                    "STOP"
                },
                amiga.cia_b.icr_status(),
                amiga.cia_b.icr_mask(),
                amiga.copper.cop1lc,
                amiga.copper.state,
                amiga.cia_a.tod_counter(),
                amiga.cia_a.tod_halted(),
                amiga.cia_b.tod_counter(),
            );

            // Sample trackdisk signal state (task at known address $621E)
            {
                let td = 0x621Eusize;
                let r = &amiga.memory.chip_ram;
                if td + 0x40 < r.len() {
                    let st = r[td + 0x0F];
                    let sw = (r[td + 0x16] as u32) << 24
                        | (r[td + 0x17] as u32) << 16
                        | (r[td + 0x18] as u32) << 8
                        | r[td + 0x19] as u32;
                    let sr_sig = (r[td + 0x1A] as u32) << 24
                        | (r[td + 0x1B] as u32) << 16
                        | (r[td + 0x1C] as u32) << 8
                        | r[td + 0x1D] as u32;
                    println!(
                        "      td: state={} SigWait=${:08X} SigRecvd=${:08X}",
                        st, sw, sr_sig
                    );
                }
            }

            // System time from timer.device (device_base=$2F4E, systime at +$C6/$CA)
            {
                let r = &amiga.memory.chip_ram;
                let db = 0x2F4Eusize;
                if db + 0xCE < r.len() {
                    let secs = (r[db + 0xC6] as u32) << 24
                        | (r[db + 0xC7] as u32) << 16
                        | (r[db + 0xC8] as u32) << 8
                        | r[db + 0xC9] as u32;
                    let micros = (r[db + 0xCA] as u32) << 24
                        | (r[db + 0xCB] as u32) << 16
                        | (r[db + 0xCC] as u32) << 8
                        | r[db + 0xCD] as u32;
                    println!("      sysTime: {}s {}µs", secs, micros);
                }
            }

            // Detect stuck PC (same 4KB page)
            let pc_page = pc >> 12;
            let last_page = last_pc >> 12;
            if pc_page == last_page {
                stuck_count += 1;
                if stuck_count >= 30 {
                    println!(
                        "PC stuck in page ${:05X}xxx for 30+ seconds — stopping",
                        pc_page
                    );
                    break;
                }
            } else {
                stuck_count = 0;
            }
            last_pc = pc;
            last_report = i;
        }
    }

    println!("\nFinal state after {} ticks:", amiga.master_clock);
    println!(
        "  PC=${:08X} SR=${:04X} SSP=${:08X}",
        amiga.cpu.regs.pc, amiga.cpu.regs.sr, amiga.cpu.regs.ssp
    );
    println!(
        "  Palette[0]=${:04X} overlay={}",
        amiga.denise.palette[0], amiga.memory.overlay
    );
    println!(
        "  L3 handler entries: {}, VERTB dispatches: {}, timer.device server: {}",
        l3_handler_count, vertb_dispatch_count, timer_server_count
    );

    // Inspect ExecBase task lists
    let read_long = |ram: &[u8], addr: usize| -> u32 {
        if addr + 3 < ram.len() {
            (ram[addr] as u32) << 24
                | (ram[addr + 1] as u32) << 16
                | (ram[addr + 2] as u32) << 8
                | ram[addr + 3] as u32
        } else {
            0
        }
    };
    let read_word = |ram: &[u8], addr: usize| -> u16 {
        if addr + 1 < ram.len() {
            (ram[addr] as u16) << 8 | ram[addr + 1] as u16
        } else {
            0
        }
    };
    let read_string_from_mem = |mem: &Memory, addr: u32| -> String {
        let mut s = String::new();
        let mut a = addr;
        loop {
            let b = mem.read_byte(a);
            if b == 0 || s.len() >= 40 {
                break;
            }
            s.push(b as char);
            a += 1;
        }
        s
    };
    let exec_base = read_long(&amiga.memory.chip_ram, 4) as usize;
    println!("\nExecBase at ${:08X}", exec_base);
    if exec_base > 0 && exec_base < amiga.memory.chip_ram.len() - 0x200 {
        let this_task = read_long(&amiga.memory.chip_ram, exec_base + 0x114) as usize;
        println!("  ThisTask = ${:08X}", this_task);
        if this_task > 0 && this_task + 0x20 < amiga.memory.chip_ram.len() {
            let task_name = read_long(&amiga.memory.chip_ram, this_task + 10) as u32;
            if task_name > 0 {
                println!(
                    "    Name: \"{}\"",
                    read_string_from_mem(&amiga.memory, task_name)
                );
            }
            let task_state = amiga.memory.chip_ram[this_task + 0x0F];
            println!("    State: {} (2=ready, 3=running, 4=waiting)", task_state);
        }

        let id_nest = amiga.memory.chip_ram[exec_base + 0x126] as i8;
        let td_nest = amiga.memory.chip_ram[exec_base + 0x127] as i8;
        println!("  IDNestCnt={} TDNestCnt={}", id_nest, td_nest);

        // Walk TaskReady list at ExecBase+$196
        println!("  TaskReady list:");
        let mut node = read_long(&amiga.memory.chip_ram, exec_base + 0x196) as usize;
        let mut count = 0;
        while node > 0 && node < amiga.memory.chip_ram.len() - 0x20 && count < 10 {
            let next = read_long(&amiga.memory.chip_ram, node) as usize;
            let name_ptr = read_long(&amiga.memory.chip_ram, node + 10) as usize;
            let name = if name_ptr > 0 {
                read_string_from_mem(&amiga.memory, name_ptr as u32)
            } else {
                "(null)".to_string()
            };
            let pri = amiga.memory.chip_ram[node + 9] as i8;
            let state = amiga.memory.chip_ram[node + 0x0F];
            let sig_wait = read_long(&amiga.memory.chip_ram, node + 0x16);
            let sig_recvd = read_long(&amiga.memory.chip_ram, node + 0x1A);
            println!(
                "    ${:08X}: \"{}\" pri={} state={} SigWait=${:08X} SigRecvd=${:08X}",
                node, name, pri, state, sig_wait, sig_recvd
            );
            if next == 0 || next == node {
                break;
            }
            node = next;
            count += 1;
        }
        if count == 0 {
            println!("    (empty)");
        }

        // Walk TaskWait list at ExecBase+$1A4
        println!("  TaskWait list:");
        node = read_long(&amiga.memory.chip_ram, exec_base + 0x1A4) as usize;
        count = 0;
        while node > 0 && node < amiga.memory.chip_ram.len() - 0x20 && count < 10 {
            let next = read_long(&amiga.memory.chip_ram, node) as usize;
            let name_ptr = read_long(&amiga.memory.chip_ram, node + 10) as usize;
            let name = if name_ptr > 0 {
                read_string_from_mem(&amiga.memory, name_ptr as u32)
            } else {
                "(null)".to_string()
            };
            let pri = amiga.memory.chip_ram[node + 9] as i8;
            let state = amiga.memory.chip_ram[node + 0x0F];
            let sig_alloc = read_long(&amiga.memory.chip_ram, node + 0x12);
            let sig_wait = read_long(&amiga.memory.chip_ram, node + 0x16);
            let sig_recvd = read_long(&amiga.memory.chip_ram, node + 0x1A);
            println!(
                "    ${:08X}: \"{}\" pri={} state={} SigAlloc=${:08X} SigWait=${:08X} SigRecvd=${:08X}",
                node, name, pri, state, sig_alloc, sig_wait, sig_recvd
            );
            if next == 0 || next == node {
                break;
            }
            node = next;
            count += 1;
        }
        if count == 0 {
            println!("    (empty)");
        }
    }

    // Dump exception vectors for interrupt handlers
    println!("\n  Exception vectors:");
    for (vec_num, name) in [
        (25u32, "L1"),
        (26, "L2/PORTS"),
        (27, "L3/VERTB"),
        (28, "L4/AUD"),
        (29, "L5"),
        (30, "L6/EXTER"),
    ] {
        let addr = read_long(&amiga.memory.chip_ram, (vec_num * 4) as usize);
        println!("    Vector {}: ${:08X} ({})", vec_num, addr, name);
    }

    // Walk exec's ResourceList at ExecBase+$150
    if exec_base > 0 && exec_base + 0x200 < amiga.memory.chip_ram.len() {
        println!("\n  ResourceList (ExecBase+$150):");
        let mut node = read_long(&amiga.memory.chip_ram, exec_base + 0x150) as usize;
        let sentinel = exec_base + 0x150 + 4; // &lh_Tail
        let mut count = 0;
        while node != sentinel
            && node != 0
            && node < amiga.memory.chip_ram.len() - 0x20
            && count < 20
        {
            let next = read_long(&amiga.memory.chip_ram, node) as usize;
            let name_ptr = read_long(&amiga.memory.chip_ram, node + 10) as u32;
            let name = if name_ptr > 0 {
                read_string_from_mem(&amiga.memory, name_ptr)
            } else {
                "(null)".to_string()
            };
            println!("    ${:08X}: \"{}\"", node, name);
            if next == 0 || next == node {
                break;
            }
            node = next;
            count += 1;
        }
        if count == 0 {
            println!("    (empty)");
        }

        // Walk exec's DeviceList at ExecBase+$15E
        // (MemList=$142, ResourceList=$150, DeviceList=$15E, IntrList=$16C, LibList=$17A, PortList=$188, TaskReady=$196, TaskWait=$1A4)
        println!("\n  DeviceList (ExecBase+$15E):");
        node = read_long(&amiga.memory.chip_ram, exec_base + 0x15E) as usize;
        let dev_sentinel = exec_base + 0x15E + 4;
        count = 0;
        while node != dev_sentinel
            && node != 0
            && node < amiga.memory.chip_ram.len() - 0x20
            && count < 20
        {
            let next = read_long(&amiga.memory.chip_ram, node) as usize;
            let name_ptr = read_long(&amiga.memory.chip_ram, node + 10) as u32;
            let name = if name_ptr > 0 {
                read_string_from_mem(&amiga.memory, name_ptr)
            } else {
                "(null)".to_string()
            };
            let open_cnt = read_word(&amiga.memory.chip_ram, node + 32);
            println!("    ${:08X}: \"{}\" OpenCnt={}", node, name, open_cnt);
            if next == 0 || next == node {
                break;
            }
            node = next;
            count += 1;
        }
        if count == 0 {
            println!("    (empty)");
        }

        // Walk exec's LibList at ExecBase+$17A
        println!("\n  LibList (ExecBase+$17A):");
        node = read_long(&amiga.memory.chip_ram, exec_base + 0x17A) as usize;
        let lib_sentinel = exec_base + 0x17A + 4;
        count = 0;
        while node != lib_sentinel
            && node != 0
            && node < amiga.memory.chip_ram.len() - 0x20
            && count < 20
        {
            let next = read_long(&amiga.memory.chip_ram, node) as usize;
            let name_ptr = read_long(&amiga.memory.chip_ram, node + 10) as u32;
            let name = if name_ptr > 0 {
                read_string_from_mem(&amiga.memory, name_ptr)
            } else {
                "(null)".to_string()
            };
            let open_cnt = read_word(&amiga.memory.chip_ram, node + 32);
            println!("    ${:08X}: \"{}\" OpenCnt={}", node, name, open_cnt);
            if next == 0 || next == node {
                break;
            }
            node = next;
            count += 1;
        }
        if count == 0 {
            println!("    (empty)");
        }
    }

    // Dump IntVects for VERTB (index 5) at ExecBase+$54 + 5*12 = ExecBase+$90
    if exec_base > 0 && exec_base + 0x200 < amiga.memory.chip_ram.len() {
        println!("\n  VERTB IntVect at ExecBase+$90:");
        let iv_data = read_long(&amiga.memory.chip_ram, exec_base + 0x90);
        let iv_code = read_long(&amiga.memory.chip_ram, exec_base + 0x94);
        let iv_node = read_long(&amiga.memory.chip_ram, exec_base + 0x98);
        println!(
            "    iv_Data=${:08X} iv_Code=${:08X} iv_Node=${:08X}",
            iv_data, iv_code, iv_node
        );

        // For server chains, iv_Data points to the List. Walk it.
        if iv_data > 0 && (iv_data as usize) + 20 < amiga.memory.chip_ram.len() {
            let list_addr = iv_data as usize;
            let lh_head = read_long(&amiga.memory.chip_ram, list_addr);
            let lh_tail = read_long(&amiga.memory.chip_ram, list_addr + 4);
            let lh_tailpred = read_long(&amiga.memory.chip_ram, list_addr + 8);
            println!(
                "    Server list at ${:08X}: lh_Head=${:08X} lh_Tail=${:08X} lh_TailPred=${:08X}",
                iv_data, lh_head, lh_tail, lh_tailpred
            );
            // Walk nodes: lh_Head -> ... -> sentinel (whose ln_Succ == 0)
            let sentinel_addr = list_addr as u32 + 4; // &lh_Tail
            let mut node = lh_head;
            let mut count = 0;
            while node != sentinel_addr
                && node != 0
                && (node as usize) < amiga.memory.chip_ram.len() - 0x20
                && count < 10
            {
                let next = read_long(&amiga.memory.chip_ram, node as usize);
                let name_ptr = read_long(&amiga.memory.chip_ram, node as usize + 10) as u32;
                let name = if name_ptr > 0 {
                    read_string_from_mem(&amiga.memory, name_ptr)
                } else {
                    "(null)".to_string()
                };
                let is_data = read_long(&amiga.memory.chip_ram, node as usize + 14);
                let is_code = read_long(&amiga.memory.chip_ram, node as usize + 18);
                println!(
                    "      ${:08X}: \"{}\" is_Data=${:08X} is_Code=${:08X}",
                    node, name, is_data, is_code
                );
                node = next;
                count += 1;
            }
            if count == 0 {
                println!("      (empty)");
            }
        }

        // Also check PORTS IntVect (index 3) at ExecBase+$78
        println!("\n  PORTS IntVect at ExecBase+$78:");
        let iv_data = read_long(&amiga.memory.chip_ram, exec_base + 0x78);
        let iv_code = read_long(&amiga.memory.chip_ram, exec_base + 0x7C);
        let iv_node = read_long(&amiga.memory.chip_ram, exec_base + 0x80);
        println!(
            "    iv_Data=${:08X} iv_Code=${:08X} iv_Node=${:08X}",
            iv_data, iv_code, iv_node
        );

        // Check EXTER IntVect (index 13) at ExecBase+$54+13*12 = ExecBase+$F0
        println!("\n  EXTER IntVect at ExecBase+$F0:");
        let iv_data_e = read_long(&amiga.memory.chip_ram, exec_base + 0xF0);
        let iv_code_e = read_long(&amiga.memory.chip_ram, exec_base + 0xF4);
        let iv_node_e = read_long(&amiga.memory.chip_ram, exec_base + 0xF8);
        println!(
            "    iv_Data=${:08X} iv_Code=${:08X} iv_Node=${:08X}",
            iv_data_e, iv_code_e, iv_node_e
        );
        // If ciab.resource installed a server chain, walk it
        if iv_data_e > 0 && (iv_data_e as usize) + 20 < amiga.memory.chip_ram.len() {
            let list_addr = iv_data_e as usize;
            let lh_head = read_long(&amiga.memory.chip_ram, list_addr);
            let lh_tail = read_long(&amiga.memory.chip_ram, list_addr + 4);
            let lh_tailpred = read_long(&amiga.memory.chip_ram, list_addr + 8);
            println!(
                "    Server list at ${:08X}: lh_Head=${:08X} lh_Tail=${:08X} lh_TailPred=${:08X}",
                iv_data_e, lh_head, lh_tail, lh_tailpred
            );
            let sentinel_addr_e = list_addr as u32 + 4;
            let mut node_e = lh_head;
            let mut count_e = 0;
            while node_e != sentinel_addr_e
                && node_e != 0
                && (node_e as usize) < amiga.memory.chip_ram.len() - 0x20
                && count_e < 10
            {
                let next = read_long(&amiga.memory.chip_ram, node_e as usize);
                let name_ptr = read_long(&amiga.memory.chip_ram, node_e as usize + 10) as u32;
                let name = if name_ptr > 0 {
                    read_string_from_mem(&amiga.memory, name_ptr)
                } else {
                    "(null)".to_string()
                };
                let is_data = read_long(&amiga.memory.chip_ram, node_e as usize + 14);
                let is_code = read_long(&amiga.memory.chip_ram, node_e as usize + 18);
                println!(
                    "      ${:08X}: \"{}\" is_Data=${:08X} is_Code=${:08X}",
                    node_e, name, is_data, is_code
                );
                node_e = next;
                count_e += 1;
            }
            if count_e == 0 {
                println!("      (empty or not a server chain)");
            }
        }

        // Dump ciab.resource internal structure including jump table
        {
            let ciab_addr = 0x1B18usize;
            let neg_size = 24; // from NegSize field
            println!("\n  ciab.resource jump table (NegSize={}):", neg_size);
            let jt_start = ciab_addr - neg_size;
            for i in (0..neg_size).step_by(6) {
                let addr = jt_start + i;
                let opcode = read_word(&amiga.memory.chip_ram, addr);
                let target = read_long(&amiga.memory.chip_ram, addr + 2);
                let lvo = -(neg_size as i32 - i as i32);
                println!(
                    "    ${:04X} (LVO {}): {:04X} ${:08X}{}",
                    addr,
                    lvo,
                    opcode,
                    target,
                    if opcode == 0x4EF9 { " (JMP)" } else { " (???)" }
                );
            }
            println!("\n  ciab.resource raw dump at ${:04X}:", ciab_addr);
            for row in 0..8 {
                let base = ciab_addr + row * 16;
                let mut hex = String::new();
                for col in 0..16 {
                    if base + col < amiga.memory.chip_ram.len() {
                        hex.push_str(&format!("{:02X} ", amiga.memory.chip_ram[base + col]));
                    }
                }
                println!("    ${:04X}: {}", base, hex);
            }
        }

        // Dump timer.device jump table and internal structure
        {
            let td_addr = 0x2F4Eusize;
            let neg_size = read_word(&amiga.memory.chip_ram, td_addr + 16) as usize;
            println!("\n  timer.device jump table (NegSize={}):", neg_size);
            let jt_start = td_addr - neg_size;
            let lvo_names = ["Open", "Close", "Expunge", "ExtFunc", "BeginIO", "AbortIO"];
            for i in (0..neg_size).step_by(6) {
                let addr = jt_start + i;
                let opcode = read_word(&amiga.memory.chip_ram, addr);
                let target = read_long(&amiga.memory.chip_ram, addr + 2);
                let lvo = -(neg_size as i32 - i as i32);
                let lvo_idx = (neg_size - i) / 6;
                let name = if lvo_idx <= 6 && lvo_idx > 0 {
                    lvo_names.get(lvo_idx - 1).unwrap_or(&"?")
                } else {
                    "?"
                };
                println!(
                    "    ${:04X} (LVO {} = {}): {:04X} ${:08X}{}",
                    addr,
                    lvo,
                    name,
                    opcode,
                    target,
                    if opcode == 0x4EF9 { " (JMP)" } else { " (???)" }
                );
            }
            println!("\n  timer.device raw dump at ${:04X}:", td_addr);
            for row in 0..14 {
                let base = td_addr + row * 16;
                let mut hex = String::new();
                for col in 0..16 {
                    if base + col < amiga.memory.chip_ram.len() {
                        hex.push_str(&format!("{:02X} ", amiga.memory.chip_ram[base + col]));
                    }
                }
                println!("    ${:04X}: {}", base, hex);
            }
        }

        // Search chip RAM for references to ciab.resource base ($1B18)
        {
            let ciab_base_bytes: [u8; 4] = [0x00, 0x00, 0x1B, 0x18];
            println!("\n  References to ciab.resource ($00001B18) in chip RAM:");
            let ram = &amiga.memory.chip_ram;
            let mut found = 0;
            for i in 0..ram.len().saturating_sub(3) {
                if ram[i] == ciab_base_bytes[0]
                    && ram[i + 1] == ciab_base_bytes[1]
                    && ram[i + 2] == ciab_base_bytes[2]
                    && ram[i + 3] == ciab_base_bytes[3]
                {
                    println!("    ${:08X}: contains $00001B18", i);
                    found += 1;
                    if found > 20 {
                        println!("    ... (truncated)");
                        break;
                    }
                }
            }
            if found == 0 {
                println!("    NONE FOUND");
            }
        }
    }

    // Dump copper lists
    println!("\n=== Copper List Dump ===");
    println!(
        "COP1LC=${:08X} COP2LC=${:08X}",
        amiga.copper.cop1lc, amiga.copper.cop2lc
    );

    // Dump from COP1LC
    let cop1 = amiga.copper.cop1lc as usize;
    println!("\nCOP1 list at ${:08X}:", cop1);
    for j in 0..32 {
        let addr = cop1 + j * 4;
        if addr + 3 < amiga.memory.chip_ram.len() {
            let w1 = ((amiga.memory.chip_ram[addr] as u16) << 8)
                | amiga.memory.chip_ram[addr + 1] as u16;
            let w2 = ((amiga.memory.chip_ram[addr + 2] as u16) << 8)
                | amiga.memory.chip_ram[addr + 3] as u16;
            if w1 == 0xFFFF && w2 == 0xFFFE {
                println!("  [{:2}] ${:06X}: WAIT $FFFF,$FFFE (end)", j, addr);
                break;
            }
            if w1 & 1 == 0 {
                println!(
                    "  [{:2}] ${:06X}: MOVE ${:04X} -> ${:03X} ({})",
                    j,
                    addr,
                    w2,
                    w1,
                    reg_name(w1)
                );
            } else {
                println!(
                    "  [{:2}] ${:06X}: WAIT/SKIP VP={:02X} HP={:02X} mask VP={:02X} HP={:02X}",
                    j,
                    addr,
                    (w1 >> 8) & 0xFF,
                    (w1 >> 1) & 0x7F,
                    (w2 >> 8) & 0x7F,
                    (w2 >> 1) & 0x7F
                );
            }
        }
    }

    // Dump from COP2LC
    let cop2 = amiga.copper.cop2lc as usize;
    println!("\nCOP2 list at ${:08X}:", cop2);
    for j in 0..64 {
        let addr = cop2 + j * 4;
        if addr + 3 < amiga.memory.chip_ram.len() {
            let w1 = ((amiga.memory.chip_ram[addr] as u16) << 8)
                | amiga.memory.chip_ram[addr + 1] as u16;
            let w2 = ((amiga.memory.chip_ram[addr + 2] as u16) << 8)
                | amiga.memory.chip_ram[addr + 3] as u16;
            if w1 == 0xFFFF && w2 == 0xFFFE {
                println!("  [{:2}] ${:06X}: WAIT $FFFF,$FFFE (end)", j, addr);
                break;
            }
            if w1 & 1 == 0 {
                println!(
                    "  [{:2}] ${:06X}: MOVE ${:04X} -> ${:03X} ({})",
                    j,
                    addr,
                    w2,
                    w1,
                    reg_name(w1)
                );
            } else {
                println!(
                    "  [{:2}] ${:06X}: WAIT/SKIP VP={:02X} HP={:02X} mask VP={:02X} HP={:02X}",
                    j,
                    addr,
                    (w1 >> 8) & 0xFF,
                    (w1 >> 1) & 0x7F,
                    (w2 >> 8) & 0x7F,
                    (w2 >> 1) & 0x7F
                );
            }
        }
    }

    // Print PC range heat map
    println!("\nPC range heat map (% of sampled ticks):");
    for i in 0..14 {
        if pc_ranges[i] > 0 {
            let pct = (pc_ranges[i] as f64 / range_ticks as f64) * 100.0;
            println!("  $FC{:X}000-$FC{:X}FFF: {:6.2}%", i, i, pct);
        }
    }
    if pc_ranges[14] > 0 {
        let pct = (pc_ranges[14] as f64 / range_ticks as f64) * 100.0;
        println!("  Other:            {:6.2}%", pct);
    }
    if pc_ranges[15] > 0 {
        let pct = (pc_ranges[15] as f64 / range_ticks as f64) * 100.0;
        println!("  Chip RAM:         {:6.2}%", pct);
    }

    // Save framebuffer as PNG screenshot
    let out_path = std::path::Path::new("../../test_output/amiga_boot.png");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let file = fs::File::create(out_path).expect("create screenshot file");
    let ref mut w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, FB_WIDTH, FB_HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    // Convert ARGB32 framebuffer to RGBA bytes
    let mut rgba = Vec::with_capacity((FB_WIDTH * FB_HEIGHT * 4) as usize);
    for &pixel in &amiga.denise.framebuffer {
        rgba.push(((pixel >> 16) & 0xFF) as u8); // R
        rgba.push(((pixel >> 8) & 0xFF) as u8); // G
        rgba.push((pixel & 0xFF) as u8); // B
        rgba.push(((pixel >> 24) & 0xFF) as u8); // A
    }
    writer.write_image_data(&rgba).expect("write PNG data");
    println!("\nScreenshot saved to {}", out_path.display());

    // GfxBase diagnostics: dump key fields to understand copper list state
    {
        // GfxBase is in the LibList — find it by name or use known address
        // In KS1.3, graphics.library base is typically at $221E (from boot logs)
        // Walk LibList to find it dynamically
        let ram = &amiga.memory.chip_ram;
        let exec_base = read_long(ram, 4) as usize;
        let mut gfx_base: usize = 0;
        if exec_base > 0 && exec_base + 0x200 < ram.len() {
            let mut node = read_long(ram, exec_base + 0x17A) as usize;
            let lib_sentinel = exec_base + 0x17A + 4;
            let mut count = 0;
            while node != lib_sentinel && node != 0 && node < ram.len() - 0x40 && count < 20 {
                let next = read_long(ram, node) as usize;
                let name_ptr = read_long(ram, node + 10) as u32;
                if name_ptr > 0 {
                    let name = read_string_from_mem(&amiga.memory, name_ptr);
                    if name.starts_with("graphics") {
                        gfx_base = node;
                        break;
                    }
                }
                if next == 0 || next == node {
                    break;
                }
                node = next;
                count += 1;
            }
        }
        if gfx_base > 0 && gfx_base + 0x40 < ram.len() {
            println!("\n=== GfxBase at ${:08X} ===", gfx_base);
            let acti_view = read_long(ram, gfx_base + 0x22);
            let copinit = read_long(ram, gfx_base + 0x26);
            let lof_list = read_long(ram, gfx_base + 0x2E);
            let shf_list = read_long(ram, gfx_base + 0x32);
            println!(
                "  ActiView=${:08X} copinit=${:08X} LOFlist=${:08X} SHFlist=${:08X}",
                acti_view, copinit, lof_list, shf_list
            );

            // If ActiView is valid, dump the View structure
            if acti_view > 0 && (acti_view as usize) + 0x20 < ram.len() {
                let view = acti_view as usize;
                let vp_ptr = read_long(ram, view + 4); // View->ViewPort
                let lof_cpr = read_long(ram, view + 8); // View->LOFCprList (cprlist*)
                let shf_cpr = read_long(ram, view + 12); // View->SHFCprList
                let dy_off = read_word(ram, view + 16); // View->DyOffset
                let dx_off = read_word(ram, view + 18); // View->DxOffset
                let modes = read_word(ram, view + 2); // View->Modes
                println!(
                    "  View at ${:08X}: ViewPort=${:08X} LOFCprList=${:08X} SHFCprList=${:08X} Modes=${:04X} DyOff={} DxOff={}",
                    acti_view, vp_ptr, lof_cpr, shf_cpr, modes, dy_off, dx_off
                );

                // If LOFCprList exists, dump the cprlist (start, maxCount)
                if lof_cpr > 0 && (lof_cpr as usize) + 8 < ram.len() {
                    let cpr = lof_cpr as usize;
                    let next = read_long(ram, cpr); // cprlist->Next
                    let start = read_long(ram, cpr + 4); // cprlist->start (copper instruction pointer)
                    let max_count = read_word(ram, cpr + 8);
                    println!(
                        "    LOFCprList: Next=${:08X} start=${:08X} MaxCount={}",
                        next, start, max_count
                    );

                    // Dump first 16 copper instructions from the cprlist
                    if start > 0 && (start as usize) + 64 < ram.len() {
                        println!("    Copper instructions at ${:08X}:", start);
                        for j in 0..16 {
                            let a = start as usize + j * 4;
                            if a + 3 >= ram.len() {
                                break;
                            }
                            let w1 = read_word(ram, a);
                            let w2 = read_word(ram, a + 2);
                            if w1 == 0xFFFF && w2 == 0xFFFE {
                                println!("      [{:2}] ${:06X}: WAIT $FFFF,$FFFE (end)", j, a);
                                break;
                            }
                            if w1 & 1 == 0 {
                                println!(
                                    "      [{:2}] ${:06X}: MOVE ${:04X} -> ${:03X} ({})",
                                    j,
                                    a,
                                    w2,
                                    w1,
                                    reg_name(w1)
                                );
                            } else {
                                println!(
                                    "      [{:2}] ${:06X}: WAIT VP={:02X} HP={:02X}",
                                    j,
                                    a,
                                    (w1 >> 8) & 0xFF,
                                    (w1 >> 1) & 0x7F
                                );
                            }
                        }
                    }
                }

                // Dump ViewPort and its CopList
                if vp_ptr > 0 && (vp_ptr as usize) + 0x30 < ram.len() {
                    let vp = vp_ptr as usize;
                    let vp_next = read_long(ram, vp); // ViewPort->Next
                    let col_map = read_long(ram, vp + 4); // ViewPort->ColorMap
                    let dsp_ins = read_long(ram, vp + 8); // ViewPort->DspIns (CopList*)
                    let spr_ins = read_long(ram, vp + 12); // ViewPort->SprIns
                    let clr_ins = read_long(ram, vp + 16); // ViewPort->ClrIns
                    let uc_ins = read_long(ram, vp + 20); // ViewPort->UCopIns
                    let dw = read_word(ram, vp + 24);
                    let dh = read_word(ram, vp + 26);
                    let dx_off = read_word(ram, vp + 28);
                    let dy_off = read_word(ram, vp + 30);
                    let modes_vp = read_word(ram, vp + 32);
                    let rast_info = read_long(ram, vp + 0x24); // ViewPort->RasInfo
                    println!(
                        "    ViewPort at ${:08X}: Next=${:08X} ColorMap=${:08X} DspIns=${:08X}",
                        vp_ptr, vp_next, col_map, dsp_ins
                    );
                    println!(
                        "      SprIns=${:08X} ClrIns=${:08X} UCopIns=${:08X} RasInfo=${:08X}",
                        spr_ins, clr_ins, uc_ins, rast_info
                    );
                    println!(
                        "      DWidth={} DHeight={} DxOffset={} DyOffset={} Modes=${:04X}",
                        dw, dh, dx_off, dy_off, modes_vp
                    );

                    // If RasInfo exists, dump BitMap pointer and plane pointers
                    if rast_info > 0 && (rast_info as usize) + 12 < ram.len() {
                        let ri = rast_info as usize;
                        let ri_next = read_long(ram, ri); // RasInfo->Next
                        let bm_ptr = read_long(ram, ri + 4); // RasInfo->BitMap
                        let rx_off = read_word(ram, ri + 8);
                        let ry_off = read_word(ram, ri + 10);
                        println!(
                            "      RasInfo: Next=${:08X} BitMap=${:08X} RxOff={} RyOff={}",
                            ri_next, bm_ptr, rx_off, ry_off
                        );

                        // BitMap structure: BytesPerRow(2), Rows(2), Flags(1), Depth(1), pad(2), Planes[8](32 each)
                        if bm_ptr > 0 && (bm_ptr as usize) + 40 < ram.len() {
                            let bm = bm_ptr as usize;
                            let bpr = read_word(ram, bm);
                            let rows = read_word(ram, bm + 2);
                            let depth = ram[bm + 5];
                            println!(
                                "      BitMap at ${:08X}: BytesPerRow={} Rows={} Depth={}",
                                bm_ptr, bpr, rows, depth
                            );
                            for p in 0..depth.min(6) {
                                let plane = read_long(ram, bm + 8 + p as usize * 4);
                                println!("        Plane[{}]=${:08X}", p, plane);
                            }
                        }
                    }
                }
            }
        } else {
            println!("\nGfxBase not found in LibList");
        }
    }

    // Diagnostic: check bitplane data and copper list colors
    let ram = &amiga.memory.chip_ram;
    let cop2lc = amiga.copper.cop2lc as usize;
    println!("\nDisplay diagnostics:");
    println!(
        "  COP1LC=${:08X} COP2LC=${:08X}",
        amiga.copper.cop1lc, amiga.copper.cop2lc
    );
    println!(
        "  Denise palette: {:03X} {:03X} {:03X} {:03X}",
        amiga.denise.palette[0],
        amiga.denise.palette[1],
        amiga.denise.palette[2],
        amiga.denise.palette[3]
    );

    // Check if bitplane data at $A572 and $C4B2 is non-zero
    let bpl1_base = 0xA572usize;
    let bpl2_base = 0xC4B2usize;
    let mut bpl1_nonzero = 0u32;
    let mut bpl2_nonzero = 0u32;
    for i in 0..8000 {
        if bpl1_base + i < ram.len() && ram[bpl1_base + i] != 0 {
            bpl1_nonzero += 1;
        }
        if bpl2_base + i < ram.len() && ram[bpl2_base + i] != 0 {
            bpl2_nonzero += 1;
        }
    }
    println!(
        "  BPL1 at ${:06X}: {} non-zero bytes in first 8000",
        bpl1_base, bpl1_nonzero
    );
    println!(
        "  BPL2 at ${:06X}: {} non-zero bytes in first 8000",
        bpl2_base, bpl2_nonzero
    );

    // Dump first and last scanlines of BPL1 (40 bytes each)
    print!("  BPL1 first scanline ($A572): ");
    for i in 0..40 {
        print!("{:02X}", ram[bpl1_base + i]);
    }
    println!();
    print!("  BPL1 last  scanline ($C47A): ");
    for i in 0..40 {
        print!("{:02X}", ram[bpl1_base + 199 * 40 + i]);
    }
    println!();

    // Find first all-zero scanline from top and bottom
    let mut first_zero_from_top: Option<usize> = None;
    let mut first_zero_from_bot: Option<usize> = None;
    for row in 0..200 {
        let start = bpl1_base + row * 40;
        if ram[start..start + 40].iter().all(|&b| b == 0) && first_zero_from_top.is_none() {
            first_zero_from_top = Some(row);
        }
        let bot_row = 199 - row;
        let start_bot = bpl1_base + bot_row * 40;
        if ram[start_bot..start_bot + 40].iter().all(|&b| b == 0) && first_zero_from_bot.is_none() {
            first_zero_from_bot = Some(bot_row);
        }
    }
    println!(
        "  First all-$00 scanline from top: {:?}",
        first_zero_from_top
    );
    println!(
        "  First all-$00 scanline from bottom: {:?}",
        first_zero_from_bot
    );

    // Count all-$00 and all-$FF scanlines
    let mut zero_lines = 0;
    let mut ff_lines = 0;
    for row in 0..200 {
        let start = bpl1_base + row * 40;
        if ram[start..start + 40].iter().all(|&b| b == 0) {
            zero_lines += 1;
        }
        if ram[start..start + 40].iter().all(|&b| b == 0xFF) {
            ff_lines += 1;
        }
    }
    println!(
        "  All-$00 scanlines: {}, All-$FF scanlines: {}",
        zero_lines, ff_lines
    );

    // Dump key scanlines of BPL1 (hex, 40 bytes each)
    for &row in &[0, 10, 50, 100, 150, 190, 199] {
        let start = bpl1_base + row * 40;
        print!("  BPL1 row {:3}: ", row);
        let mut set_bits = 0u32;
        for i in 0..40 {
            let b = ram[start + i];
            set_bits += b.count_ones() as u32;
            print!("{:02X}", b);
        }
        println!("  [{}/320 bits set]", set_bits);
    }

    // Check if image is vertically flipped by comparing set-bit density
    let mut top_bits = 0u64;
    let mut bot_bits = 0u64;
    for row in 0..100 {
        for i in 0..40 {
            top_bits += ram[bpl1_base + row * 40 + i].count_ones() as u64;
            bot_bits += ram[bpl1_base + (199 - row) * 40 + i].count_ones() as u64;
        }
    }
    println!(
        "  BPL1 set bits: top half={}, bottom half={} (of 16000 each)",
        top_bits, bot_bits
    );

    // Search chip RAM for ALL BitMap-like structures (BytesPerRow=40, Rows=200, Depth=2)
    println!("\n  Searching for all BitMap structures (bpr=40, rows=200, depth=2)...");
    for addr in (0..ram.len().saturating_sub(24)).step_by(2) {
        let bpr = ((ram[addr] as u16) << 8) | ram[addr + 1] as u16;
        let rows = ((ram[addr + 2] as u16) << 8) | ram[addr + 3] as u16;
        let depth = ram[addr + 5];
        let plane0 = ((ram[addr + 8] as u32) << 24)
            | ((ram[addr + 9] as u32) << 16)
            | ((ram[addr + 10] as u32) << 8)
            | ram[addr + 11] as u32;
        let plane1 = ((ram[addr + 12] as u32) << 24)
            | ((ram[addr + 13] as u32) << 16)
            | ((ram[addr + 14] as u32) << 8)
            | ram[addr + 15] as u32;
        if bpr == 40 && rows == 200 && depth == 2 && plane0 > 0 && plane0 < 0x80000 {
            println!(
                "    BitMap at ${:06X}: BytesPerRow={} Rows={} Depth={} Planes[0]=${:08X} Planes[1]=${:08X}",
                addr, bpr, rows, depth, plane0, plane1
            );
        }
    }

    // Also search for RastPort-like structures that reference our BitMap
    // RastPort has BitMap pointer at offset 4, cp_x at offset 36, cp_y at offset 38
    println!("  Searching for RastPort-like structures referencing BitMap $001752...");
    for addr in (0..ram.len().saturating_sub(44)).step_by(2) {
        let bm_ptr = ((ram[addr + 4] as u32) << 24)
            | ((ram[addr + 5] as u32) << 16)
            | ((ram[addr + 6] as u32) << 8)
            | ram[addr + 7] as u32;
        if bm_ptr == 0x001752 || bm_ptr == 0x00A54A {
            let cp_x = ((ram[addr + 36] as i16) << 8) | ram[addr + 37] as i16;
            let cp_y = ((ram[addr + 38] as i16) << 8) | ram[addr + 39] as i16;
            let fg = ram[addr + 25];
            let bg = ram[addr + 26];
            let draw_mode = ram[addr + 28];
            println!(
                "    RastPort? at ${:06X}: BitMap=${:08X} cp_x={} cp_y={} FgPen={} BgPen={} DrawMode={}",
                addr, bm_ptr, cp_x, cp_y, fg, bg, draw_mode
            );
        }
    }

    // COP2LC dump (up to 8 instructions or end-of-list)
    if cop2lc + 4 < ram.len() {
        println!("  COP2LC dump at ${:06X}:", cop2lc);
        for i in 0..8 {
            let addr = cop2lc + i * 4;
            if addr + 3 >= ram.len() {
                break;
            }
            let w1 = (ram[addr] as u16) << 8 | ram[addr + 1] as u16;
            let w2 = (ram[addr + 2] as u16) << 8 | ram[addr + 3] as u16;
            if w1 & 1 == 0 {
                println!(
                    "    [{:2}] ${:06X}: MOVE ${:04X} -> ${:03X}",
                    i,
                    addr,
                    w2,
                    w1 & 0x1FE
                );
            } else {
                let vp = (w1 >> 8) & 0xFF;
                let hp = w1 & 0xFE;
                println!(
                    "    [{:2}] ${:06X}: WAIT/SKIP VP={:02X} HP={:02X} mask VP={:02X} HP={:02X}",
                    i,
                    addr,
                    vp,
                    hp,
                    (w2 >> 8) & 0x7F,
                    w2 & 0xFE
                );
            }
            if w1 == 0xFFFF && w2 == 0xFFFE {
                break;
            }
        }
    }

    // Count unique framebuffer colors
    let mut colors = std::collections::HashSet::new();
    for &pixel in &amiga.denise.framebuffer {
        colors.insert(pixel);
    }
    println!("  Unique framebuffer colors: {}", colors.len());
    for c in &colors {
        let r = (c >> 16) & 0xFF;
        let g = (c >> 8) & 0xFF;
        let b = c & 0xFF;
        println!("    #{:02X}{:02X}{:02X}", r, g, b);
    }

    // Regression guard for the KS1.3 insert-disk screen in the raw 320x256
    // framebuffer (not an upscaled window capture).
    const WHITE: u32 = 0xFFFF_FFFF;
    const BLACK: u32 = 0xFF00_0000;
    const FLOPPY_BLUE: u32 = 0xFF77_77CC;
    const METAL_GRAY: u32 = 0xFFBB_BBBB;

    let expected_colors = std::collections::HashSet::from([WHITE, BLACK, FLOPPY_BLUE, METAL_GRAY]);
    assert_eq!(colors, expected_colors, "unexpected framebuffer color set");

    let fb = &amiga.denise.framebuffer;
    let px = |x: u32, y: u32| -> u32 { fb[(y * FB_WIDTH + x) as usize] };

    // Stable anchors sampled from the known-good boot screen.
    assert_eq!(px(0, 0), WHITE, "top-left background should be white");
    assert_eq!(px(103, 50), BLACK, "top outline anchor changed");
    assert_eq!(px(106, 52), FLOPPY_BLUE, "floppy body anchor changed");
    assert_eq!(px(131, 52), METAL_GRAY, "floppy shutter anchor changed");

    let mut counts: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut min_x = FB_WIDTH;
    let mut min_y = FB_HEIGHT;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut non_white_pixels = 0u32;

    for y in 0..FB_HEIGHT {
        for x in 0..FB_WIDTH {
            let p = px(x, y);
            *counts.entry(p).or_insert(0) += 1;
            if p != WHITE {
                non_white_pixels += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    // Tolerant ranges: catches Gurus / major render regressions while allowing
    // small timing/layout shifts during ongoing work.
    let white_count = *counts.get(&WHITE).unwrap_or(&0);
    let black_count = *counts.get(&BLACK).unwrap_or(&0);
    let blue_count = *counts.get(&FLOPPY_BLUE).unwrap_or(&0);
    let gray_count = *counts.get(&METAL_GRAY).unwrap_or(&0);
    assert!(
        (70_000..=78_000).contains(&white_count),
        "white count out of range: {white_count}"
    );
    assert!(
        (2_000..=4_000).contains(&black_count),
        "black count out of range: {black_count}"
    );
    assert!(
        (3_000..=5_000).contains(&blue_count),
        "blue count out of range: {blue_count}"
    );
    assert!(
        (700..=1_300).contains(&gray_count),
        "gray count out of range: {gray_count}"
    );

    assert!(
        (6_000..=9_000).contains(&non_white_pixels),
        "non-white pixel count out of range: {non_white_pixels}"
    );
    assert!((75..=90).contains(&min_x), "min_x out of range: {min_x}");
    assert!((45..=60).contains(&min_y), "min_y out of range: {min_y}");
    assert!((200..=215).contains(&max_x), "max_x out of range: {max_x}");
    assert!((170..=185).contains(&max_y), "max_y out of range: {max_y}");
}

/// Short trace test: log key addresses hit during the first ~5 seconds.
/// Helps debug the SAD handler flow without running for minutes.
#[test]
#[ignore]
fn test_boot_trace() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    println!(
        "Reset: SSP=${:08X} PC=${:08X} SR=${:04X}",
        amiga.cpu.regs.ssp, amiga.cpu.regs.pc, amiga.cpu.regs.sr
    );

    // Key addresses to watch
    const SERIAL_DIAG: u32 = 0xFC3090;
    const SERDATR_READ: u32 = 0xFC30DA;
    const SAD_ENTRY: u32 = 0xFC237E;
    const SER_RECV_BYTE: u32 = 0xFC223E;
    const SER_RECV_WAIT: u32 = 0xFC225E;
    const WARM_RESTART: u32 = 0xFC05F0;
    const CPU_DETECT: u32 = 0xFC0546;
    const PRIV_VIOLATION: u32 = 0xFC30B2;
    const USER_MODE_SET: u32 = 0xFC2398;
    const MOVEQ_M1: u32 = 0xFC30C0; // MOVEQ #-1, D0 (before inner loops)
    const DBEQ_D1: u32 = 0xFC30F0; // DBEQ D1 (outer loop counter)
    const BMI_WARMRST: u32 = 0xFC30F4; // BMI $FC05F0 (after diagnostic)
    const HELP_CHECK: u32 = 0xFC3100; // BRA target after ROM entry setup
    const COLD_WARM: u32 = 0xFC014C; // MOVE.L $4.W, D0 (ExecBase check)
    const REINIT: u32 = 0xFC01CE; // LEA $400.W, A6 (cold boot init)
    const ROM_ENTRY: u32 = 0xFC00D2; // Initial ROM entry point
    const DEFAULT_EXC: u32 = 0xFC05B4; // Default exception handler
    const RESET_INSTR: u32 = 0xFC05FA; // RESET instruction in warm restart

    let total_ticks: u64 = 420_000_000; // ~15 seconds (covers 2+ boot cycles)
    let mut last_key_pc: u32 = 0;
    let mut prev_pc: u32 = 0;
    let mut serdatr_count: u32 = 0;

    for i in 0..total_ticks {
        amiga.tick();

        // Only check on CPU ticks (every 4 crystal ticks)
        if i % 4 != 0 {
            continue;
        }

        let pc = amiga.cpu.regs.pc;

        // Log when PC enters a key address for the first time in a sequence.
        // NOTE: 68000 prefetch pipeline means regs.pc points 2-4 bytes ahead
        // of the executing instruction. For critical addresses, verify IR too.
        let ir = amiga.cpu.ir;
        let key = match pc {
            SERIAL_DIAG => Some("SERIAL_DIAG"),
            SERDATR_READ => {
                serdatr_count += 1;
                if serdatr_count <= 10 {
                    Some("SERDATR_READ")
                } else {
                    None
                }
            }
            SAD_ENTRY => Some("SAD_ENTRY"),
            SER_RECV_BYTE => Some("SER_RECV_BYTE"),
            SER_RECV_WAIT => Some("SER_RECV_WAIT"),
            WARM_RESTART => Some("WARM_RESTART"),
            CPU_DETECT => Some("CPU_DETECT"),
            PRIV_VIOLATION => Some("PRIV_VIOLATION"),
            USER_MODE_SET => Some("USER_MODE_SET"),
            MOVEQ_M1 => Some("MOVEQ_M1"),
            DBEQ_D1 => Some("DBEQ_D1"),
            BMI_WARMRST => Some("BMI_WARMRST"),
            HELP_CHECK => Some("HELP_CHECK"),
            COLD_WARM => Some("COLD_WARM"),
            REINIT => Some("REINIT"),
            ROM_ENTRY => Some("ROM_ENTRY"),
            // DEFAULT_EXC: verify IR=$303C (MOVE.W #imm,D0) to avoid
            // prefetch pipeline false positives.
            DEFAULT_EXC if ir == 0x303C => Some("DEFAULT_EXC"),
            RESET_INSTR => Some("RESET_INSTR"),
            _ => None,
        };

        if let Some(name) = key {
            if pc != last_key_pc {
                let ms = i / (28_375_160 / 1000);
                // Read ExecBase pointer from chip RAM $0004
                let eb0 = amiga.memory.chip_ram[4] as u32;
                let eb1 = amiga.memory.chip_ram[5] as u32;
                let eb2 = amiga.memory.chip_ram[6] as u32;
                let eb3 = amiga.memory.chip_ram[7] as u32;
                let exec_base = (eb0 << 24) | (eb1 << 16) | (eb2 << 8) | eb3;
                // Also read ChkBase at ExecBase+$26
                let chkbase =
                    if exec_base > 0 && (exec_base as usize + 0x29) < amiga.memory.chip_ram.len() {
                        let off = exec_base as usize + 0x26;
                        (amiga.memory.chip_ram[off] as u32) << 24
                            | (amiga.memory.chip_ram[off + 1] as u32) << 16
                            | (amiga.memory.chip_ram[off + 2] as u32) << 8
                            | amiga.memory.chip_ram[off + 3] as u32
                    } else {
                        0
                    };
                println!(
                    "[{:4}ms] {} PC=${:08X} SR=${:04X} D0=${:08X} D1=${:08X} A7=${:08X} *$4={:08X} ChkBase={:08X}",
                    ms,
                    name,
                    pc,
                    amiga.cpu.regs.sr,
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.d[1],
                    amiga.cpu.regs.a(7),
                    exec_base,
                    chkbase,
                );
                // Dump exception frame when default handler is entered
                if pc == DEFAULT_EXC {
                    let sp = amiga.cpu.regs.a(7) as usize;
                    if sp >= 6 && sp + 10 <= amiga.memory.chip_ram.len() {
                        // Group 0 (bus/addr error): [fn_code:16][access_addr:32][ir:16][sr:16][pc:32]
                        let fn_code = (amiga.memory.chip_ram[sp] as u16) << 8
                            | amiga.memory.chip_ram[sp + 1] as u16;
                        let acc_hi = (amiga.memory.chip_ram[sp + 2] as u32) << 24
                            | (amiga.memory.chip_ram[sp + 3] as u32) << 16
                            | (amiga.memory.chip_ram[sp + 4] as u32) << 8
                            | amiga.memory.chip_ram[sp + 5] as u32;
                        let ir = (amiga.memory.chip_ram[sp + 6] as u16) << 8
                            | amiga.memory.chip_ram[sp + 7] as u16;
                        let sr_stk = (amiga.memory.chip_ram[sp + 8] as u16) << 8
                            | amiga.memory.chip_ram[sp + 9] as u16;
                        let pc_hi = (amiga.memory.chip_ram[sp + 10] as u32) << 24
                            | (amiga.memory.chip_ram[sp + 11] as u32) << 16
                            | (amiga.memory.chip_ram[sp + 12] as u32) << 8
                            | amiga.memory.chip_ram[sp + 13] as u32;
                        println!(
                            "  Exception frame (group 0): fn_code=${:04X} access_addr=${:08X} IR=${:04X} SR=${:04X} PC=${:08X}",
                            fn_code, acc_hi, ir, sr_stk, pc_hi
                        );
                        // Also try group 1/2 frame: [sr:16][pc:32]
                        let sr_g1 = fn_code;
                        let pc_g1 = acc_hi;
                        println!(
                            "  Exception frame (group 1/2): SR=${:04X} PC=${:08X}",
                            sr_g1, pc_g1
                        );
                    }
                }
                last_key_pc = pc;
            }
        } else if pc != prev_pc && last_key_pc != 0 {
            // Reset key tracking when we leave a key address
            last_key_pc = 0;
        }

        prev_pc = pc;

        if matches!(amiga.cpu.state, State::Halted) {
            println!(
                "CPU HALTED at tick {} PC=${:08X} IR=${:04X} SR=${:04X}",
                i, amiga.cpu.regs.pc, amiga.cpu.ir, amiga.cpu.regs.sr,
            );
            break;
        }
    }

    println!("\nFinal state after {} ticks:", amiga.master_clock);
    println!(
        "  PC=${:08X} SR=${:04X} SSP=${:08X} USP=${:08X}",
        amiga.cpu.regs.pc, amiga.cpu.regs.sr, amiga.cpu.regs.ssp, amiga.cpu.regs.usp
    );
    println!(
        "  D0-D7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
        amiga.cpu.regs.d[0],
        amiga.cpu.regs.d[1],
        amiga.cpu.regs.d[2],
        amiga.cpu.regs.d[3],
        amiga.cpu.regs.d[4],
        amiga.cpu.regs.d[5],
        amiga.cpu.regs.d[6],
        amiga.cpu.regs.d[7]
    );
    println!(
        "  A0-A7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
        amiga.cpu.regs.a(0),
        amiga.cpu.regs.a(1),
        amiga.cpu.regs.a(2),
        amiga.cpu.regs.a(3),
        amiga.cpu.regs.a(4),
        amiga.cpu.regs.a(5),
        amiga.cpu.regs.a(6),
        amiga.cpu.regs.a(7)
    );
}

const PAL_CRYSTAL_HZ: u64 = 28_375_160;

/// Trace the last N PCs before WARM_RESTART to find the exact triggering code path.
#[test]
#[ignore]
fn test_warm_restart_path() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    // Ring buffer of last 200 unique PCs
    const BUF_SIZE: usize = 200;
    let mut pc_ring: [(u32, u16, u16); BUF_SIZE] = [(0, 0, 0); BUF_SIZE]; // (PC, IR, SR)
    let mut ring_idx: usize = 0;
    let mut prev_pc: u32 = 0;

    // Also track 4KB page transitions
    let mut last_page: u32 = 0xFFFFFFFF;

    // Run for ~4 seconds (past the 2936ms warm restart)
    let total_ticks: u64 = 120_000_000;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 {
            continue;
        }

        let pc = amiga.cpu.regs.pc;

        // Track page transitions
        let page = pc >> 12;
        if page != last_page {
            let ms = i / (PAL_CRYSTAL_HZ / 1000);
            if ms > 660 && ms < 3000 {
                println!(
                    "[{:4}ms] Page ${:05X}xxx  PC=${:08X} IR=${:04X} SR=${:04X} D0=${:08X} D1=${:08X} A7=${:08X}",
                    ms,
                    page,
                    pc,
                    amiga.cpu.ir,
                    amiga.cpu.regs.sr,
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.d[1],
                    amiga.cpu.regs.a(7)
                );
            }
            last_page = page;
        }

        // Capture state at Guru alert code points (BEFORE prev_pc update)
        if pc >= 0xFC3040 && pc <= 0xFC3070 && pc != prev_pc {
            let ms = i / (PAL_CRYSTAL_HZ / 1000);
            println!(
                "[{:4}ms] ALERT PC=${:08X} IR=${:04X} D7=${:08X} D0=${:08X} A5=${:08X} A6=${:08X} SR=${:04X}",
                ms,
                pc,
                amiga.cpu.ir,
                amiga.cpu.regs.d[7],
                amiga.cpu.regs.d[0],
                amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6),
                amiga.cpu.regs.sr
            );
        }

        // Record unique PCs in ring buffer
        if pc != prev_pc {
            pc_ring[ring_idx] = (pc, amiga.cpu.ir, amiga.cpu.regs.sr);
            ring_idx = (ring_idx + 1) % BUF_SIZE;
        }
        prev_pc = pc;

        // Stop at WARM_RESTART
        if pc == 0xFC05F0 {
            let ms = i / (PAL_CRYSTAL_HZ / 1000);
            println!("\n=== WARM_RESTART at {}ms ===", ms);
            println!(
                "  D0=${:08X} D1=${:08X} D2=${:08X} D3=${:08X}",
                amiga.cpu.regs.d[0], amiga.cpu.regs.d[1], amiga.cpu.regs.d[2], amiga.cpu.regs.d[3]
            );
            println!(
                "  A0=${:08X} A6=${:08X} A7=${:08X} SR=${:04X}",
                amiga.cpu.regs.a(0),
                amiga.cpu.regs.a(6),
                amiga.cpu.regs.a(7),
                amiga.cpu.regs.sr
            );

            // Dump ring buffer (last 200 unique PCs)
            println!("\nLast {} unique PCs before WARM_RESTART:", BUF_SIZE);
            for j in 0..BUF_SIZE {
                let idx = (ring_idx + j) % BUF_SIZE;
                let (rpc, rir, rsr) = pc_ring[idx];
                if rpc != 0 {
                    println!("  PC=${:08X} IR=${:04X} SR=${:04X}", rpc, rir, rsr);
                }
            }
            break;
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!("CPU HALTED at tick {}", i);
            break;
        }
    }
}

/// Trace what sets D7 and triggers Alert(3).
/// Captures: D7 transitions, InitResident calls, and Guru entry.
#[test]
#[ignore]
fn test_alert_trigger_trace() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    let mut prev_pc: u32 = 0;
    let mut prev_d7: u32 = 0;
    let mut prev_d0: u32 = 0;

    // Track InitResident calls and results
    let mut in_resident_init = false;
    let mut resident_init_count = 0u32;

    // Find ExecBase for computing LVOs
    let mut exec_base: u32 = 0;

    // Run for ~2.5 seconds (past the 1715ms alert)
    let total_ticks: u64 = 80_000_000;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 {
            continue;
        }

        let pc = amiga.cpu.regs.pc;
        let d7 = amiga.cpu.regs.d[7];
        let d0 = amiga.cpu.regs.d[0];
        let ms = i / (PAL_CRYSTAL_HZ / 1000);

        // Track ExecBase from $4.L
        if ms > 200 && exec_base == 0 {
            let eb = (amiga.memory.chip_ram[4] as u32) << 24
                | (amiga.memory.chip_ram[5] as u32) << 16
                | (amiga.memory.chip_ram[6] as u32) << 8
                | amiga.memory.chip_ram[7] as u32;
            if eb > 0 && eb < 0x80000 {
                exec_base = eb;
                println!("[{:4}ms] ExecBase = ${:08X}", ms, exec_base);
            }
        }

        // Log when D7 changes (the alert code register) — no pc!=prev_pc
        // requirement so we catch changes during bus waits
        if d7 != prev_d7 && ms > 600 {
            println!(
                "[{:4}ms] D7 changed: ${:08X} -> ${:08X}  PC=${:08X} IR=${:04X} SR=${:04X} A5=${:08X} A6=${:08X} A7=${:08X}",
                ms,
                prev_d7,
                d7,
                pc,
                amiga.cpu.ir,
                amiga.cpu.regs.sr,
                amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6),
                amiga.cpu.regs.a(7)
            );
            // Dump stack at the moment D7 changes to find caller chain
            if d7 == 3 {
                let sp = amiga.cpu.regs.a(7) as usize;
                println!("  Stack at D7=3:");
                for s in 0..16 {
                    let addr = sp + s * 2;
                    if addr + 1 < amiga.memory.chip_ram.len() {
                        let w = (amiga.memory.chip_ram[addr] as u16) << 8
                            | amiga.memory.chip_ram[addr + 1] as u16;
                        print!(" {:04X}", w);
                    }
                }
                println!();
            }
        }

        // Log entry to ResidentInit ($FC0B2C) and InitResident calls
        if pc == 0xFC0B2C && pc != prev_pc {
            println!(
                "[{:4}ms] ResidentInit entry: D0=${:08X}(flags) D1=${:08X}(pri) A2=${:08X}",
                ms,
                d0,
                amiga.cpu.regs.d[1],
                amiga.cpu.regs.a(2)
            );
            in_resident_init = true;
        }

        // Track InitResident (LVO -102 = -$66). It's called at $FC0B58.
        if pc == 0xFC0B58 && pc != prev_pc {
            resident_init_count += 1;
            // A1 points to the RomTag
            let a1 = amiga.cpu.regs.a(1);
            // Read rt_Name at offset $E in the RomTag (it's a pointer)
            let name_ptr = if a1 >= 0xFC0000 {
                let off = (a1 - 0xFC0000) as usize;
                if off + 0x12 < amiga.memory.kickstart.len() {
                    (amiga.memory.kickstart[off + 0xE] as u32) << 24
                        | (amiga.memory.kickstart[off + 0xF] as u32) << 16
                        | (amiga.memory.kickstart[off + 0x10] as u32) << 8
                        | amiga.memory.kickstart[off + 0x11] as u32
                } else {
                    0
                }
            } else {
                0
            };
            // Read the name string
            let name = if name_ptr >= 0xFC0000 {
                let off = (name_ptr - 0xFC0000) as usize;
                let mut s = String::new();
                for j in 0..40 {
                    if off + j >= amiga.memory.kickstart.len() {
                        break;
                    }
                    let ch = amiga.memory.kickstart[off + j];
                    if ch == 0 {
                        break;
                    }
                    s.push(ch as char);
                }
                s
            } else {
                format!("@${:08X}", name_ptr)
            };
            println!(
                "[{:4}ms] InitResident #{}: A1=${:08X} name=\"{}\" D7=${:08X}",
                ms, resident_init_count, a1, name, d7
            );
        }

        // Track return from InitResident — watch for D0 result
        // InitResident is JSR (A6,-102) at $B58, returns to $B5C
        if pc == 0xFC0B5C && pc != prev_pc {
            println!(
                "[{:4}ms] InitResident returned: D0=${:08X} D7=${:08X}",
                ms, d0, d7
            );
        }

        // After ResidentInit loop ends ($B5E)
        if pc == 0xFC0B5E && pc != prev_pc {
            println!(
                "[{:4}ms] ResidentInit loop done, {} modules initialized",
                ms, resident_init_count
            );
            in_resident_init = false;
        }

        // Log all unique PCs in the 1710-1716ms window to find the code path
        if ms >= 1710 && ms <= 1716 && pc != prev_pc && pc >= 0xFC0000 {
            // Only log ROM PCs to avoid flooding with jump-table entries
            let rom_offset = pc - 0xFC0000;
            // Skip if we're in the alert handler area (logged separately)
            if rom_offset < 0x3020 || rom_offset > 0x3200 {
                println!(
                    "[{:4}ms] ROM PC=${:08X} IR=${:04X} D0=${:08X} D7=${:08X} A6=${:08X} A7=${:08X}",
                    ms,
                    pc,
                    amiga.cpu.ir,
                    d0,
                    d7,
                    amiga.cpu.regs.a(6),
                    amiga.cpu.regs.a(7)
                );
            }
        }

        // Capture entry to alert handler area
        if pc >= 0xFC3020 && pc <= 0xFC3070 && pc != prev_pc {
            println!(
                "[{:4}ms] ALERT HANDLER PC=${:08X} IR=${:04X} D0=${:08X} D7=${:08X} A5=${:08X} A6=${:08X} A7=${:08X}",
                ms,
                pc,
                amiga.cpu.ir,
                d0,
                d7,
                amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6),
                amiga.cpu.regs.a(7)
            );
        }

        // Capture Guru handler entry ($FC3128)
        if pc == 0xFC3128 && pc != prev_pc {
            println!(
                "[{:4}ms] GURU HANDLER entry: D7=${:08X} A6=${:08X}",
                ms,
                d7,
                amiga.cpu.regs.a(6)
            );
        }

        // Track SSP changes. SSP = A7 when in supervisor mode (SR bit 13 set),
        // or stored in cpu.regs.ssp when in user mode.
        let sr = amiga.cpu.regs.sr;
        let ssp = if sr & 0x2000 != 0 {
            amiga.cpu.regs.a(7) // Supervisor mode: A7 is SSP
        } else {
            amiga.cpu.regs.ssp // User mode: SSP stored separately
        };
        if ssp >= 0x80000 && pc != prev_pc && ms > 200 {
            println!(
                "[{:4}ms] SSP HIGH! ssp=${:08X} PC=${:08X} IR=${:04X} SR=${:04X} A7=${:08X}",
                ms,
                ssp,
                pc,
                amiga.cpu.ir,
                sr,
                amiga.cpu.regs.a(7)
            );
        }

        // Stop at WARM_RESTART
        if pc == 0xFC05F0 {
            println!("\n[{:4}ms] === WARM_RESTART ===", ms);
            println!(
                "  D0-D7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                amiga.cpu.regs.d[0],
                amiga.cpu.regs.d[1],
                amiga.cpu.regs.d[2],
                amiga.cpu.regs.d[3],
                amiga.cpu.regs.d[4],
                amiga.cpu.regs.d[5],
                amiga.cpu.regs.d[6],
                amiga.cpu.regs.d[7]
            );
            println!(
                "  A0-A7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
                amiga.cpu.regs.a(0),
                amiga.cpu.regs.a(1),
                amiga.cpu.regs.a(2),
                amiga.cpu.regs.a(3),
                amiga.cpu.regs.a(4),
                amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6),
                amiga.cpu.regs.a(7)
            );
            break;
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!("CPU HALTED at tick {}", i);
            break;
        }

        prev_d7 = d7;
        prev_d0 = d0;
        prev_pc = pc;
    }
}

/// Focused trace: log every instruction PC in the REINIT→DEFAULT_EXC range.
#[test]
#[ignore]
fn test_reinit_path() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom);

    // Run until we hit REINIT ($FC01CE) — about 221ms = ~6.3M CPU ticks
    let mut reached_reinit = false;
    let mut tracing = false;
    let mut prev_pc: u32 = 0;
    let mut log_count: u32 = 0;

    for i in 0u64..300_000_000 {
        amiga.tick();
        if i % 4 != 0 {
            continue;
        }

        let pc = amiga.cpu.regs.pc;

        if !reached_reinit && pc == 0xFC01CE {
            reached_reinit = true;
            tracing = true;
            println!("=== REINIT reached at tick {} ===", i);
            println!(
                "  A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X}",
                amiga.cpu.regs.a(0),
                amiga.cpu.regs.a(1),
                amiga.cpu.regs.a(2),
                amiga.cpu.regs.a(3)
            );
            println!(
                "  A4=${:08X} A5=${:08X} A6=${:08X} A7=${:08X}",
                amiga.cpu.regs.a(4),
                amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6),
                amiga.cpu.regs.a(7)
            );
            println!("  overlay={}", amiga.memory.overlay);
        }

        // Once A0 is near the end of the test, log ALL PCs
        if tracing
            && amiga.cpu.regs.a(0) >= 0x7E000
            && pc >= 0xFC0000
            && pc != prev_pc
            && log_count < 2000
        {
            println!(
                "  [DETAIL] PC=${:06X} IR=${:04X} D0={:08X} A0={:08X} A2={:08X} chip[0]={:08X}",
                pc,
                amiga.cpu.ir,
                amiga.cpu.regs.d[0],
                amiga.cpu.regs.a(0),
                amiga.cpu.regs.a(2),
                {
                    let r = &amiga.memory.chip_ram;
                    (u32::from(r[0]) << 24)
                        | (u32::from(r[1]) << 16)
                        | (u32::from(r[2]) << 8)
                        | u32::from(r[3])
                },
            );
        }

        if tracing && pc != prev_pc {
            // Log every unique PC, but limit total output
            if log_count < 500 {
                // Only log PCs outside tight loops (not in delay loops)
                let in_loop = matches!(pc,
                    0xFC0594..=0xFC05B2 // chip RAM test inner loop
                    | 0xFC061A..=0xFC068E // expansion test
                    | 0xFC05D2..=0xFC05EE // LED flash delay
                    | 0xFC05F6..=0xFC05F8 // warm restart delay
                );
                if !in_loop
                    || (pc == 0xFC0592
                        || pc == 0xFC05B0
                        || pc == 0xFC05B2
                        || pc == 0xFC05B4
                        || pc == 0xFC061A
                        || pc == 0xFC068E)
                {
                    // Only log last 10 iterations plus non-loop entries
                    if in_loop && amiga.cpu.regs.a(0) < 0x7A000 {
                        // Skip early loop iterations
                    } else {
                        println!(
                            "  PC=${:06X} IR=${:04X} D0=${:08X} D1=${:08X} A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X} A5=${:08X}",
                            pc,
                            amiga.cpu.ir,
                            amiga.cpu.regs.d[0],
                            amiga.cpu.regs.d[1],
                            amiga.cpu.regs.a(0),
                            amiga.cpu.regs.a(1),
                            amiga.cpu.regs.a(2),
                            amiga.cpu.regs.a(3),
                            amiga.cpu.regs.a(5),
                        );
                    }
                    log_count += 1;
                }
            }

            // Stop at DEFAULT_EXC (verify with IR=$303C to avoid prefetch false positives)
            // or the green screen error
            if (pc == 0xFC05B4 && amiga.cpu.ir == 0x303C) || pc == 0xFC05B8 || pc == 0xFC0238 {
                println!("=== Hit ${:06X} at tick {} ===", pc, i);
                println!(
                    "  A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X}",
                    amiga.cpu.regs.a(0),
                    amiga.cpu.regs.a(1),
                    amiga.cpu.regs.a(2),
                    amiga.cpu.regs.a(3)
                );
                println!(
                    "  A4=${:08X} A5=${:08X} A6=${:08X} A7=${:08X}",
                    amiga.cpu.regs.a(4),
                    amiga.cpu.regs.a(5),
                    amiga.cpu.regs.a(6),
                    amiga.cpu.regs.a(7)
                );
                println!(
                    "  D0=${:08X} D1=${:08X} D2=${:08X} SR=${:04X}",
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.d[1],
                    amiga.cpu.regs.d[2],
                    amiga.cpu.regs.sr
                );
                println!("  overlay={}", amiga.memory.overlay);
                // Print chip_ram[0..8]
                println!(
                    "  chip_ram[0..8]: {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
                    amiga.memory.chip_ram[0],
                    amiga.memory.chip_ram[1],
                    amiga.memory.chip_ram[2],
                    amiga.memory.chip_ram[3],
                    amiga.memory.chip_ram[4],
                    amiga.memory.chip_ram[5],
                    amiga.memory.chip_ram[6],
                    amiga.memory.chip_ram[7]
                );
                break;
            }
        }

        prev_pc = pc;

        if matches!(amiga.cpu.state, State::Halted) {
            println!("CPU HALTED at tick {}", i);
            break;
        }
    }
}

/// Trace overlay state, vector table writes, and CPU_DETECT timing.
/// Answers: does exec turn off overlay BEFORE CPU_DETECT runs?
#[test]
#[ignore]
fn test_vector_table_init() {
    let ks_path = "../../roms/kick13.rom";
    let rom = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    // Show what ROM has at the exception vector offsets
    println!("ROM exception vector area (what CPU reads when overlay is ON):");
    for v in 0..16u32 {
        let off = (v * 4) as usize;
        let val = (rom[off] as u32) << 24
            | (rom[off + 1] as u32) << 16
            | (rom[off + 2] as u32) << 8
            | rom[off + 3] as u32;
        println!("  Vector {:2} (${:03X}): ${:08X}", v, v * 4, val);
    }

    let mut amiga = Amiga::new(rom);

    let mut prev_overlay = amiga.memory.overlay;
    let mut prev_vec4: [u8; 4] = [0; 4]; // chip_ram[$10..$14]
    let mut prev_vec3: [u8; 4] = [0; 4]; // chip_ram[$0C..$10]
    let mut prev_pc: u32 = 0;
    let mut cpu_detect_seen = false;

    // Ring buffer to catch what leads PC to $10
    const RING_SIZE: usize = 64;
    let mut pc_ring: [(u32, u16, u16); RING_SIZE] = [(0, 0, 0); RING_SIZE];
    let mut ring_idx: usize = 0;
    let mut caught_pc10 = false;

    // Run for ~3 seconds — enough for cold boot init
    let total_ticks: u64 = 100_000_000;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 {
            continue;
        }

        let pc = amiga.cpu.regs.pc;
        let ms = i / (PAL_CRYSTAL_HZ / 1000);

        // Track overlay changes
        if amiga.memory.overlay != prev_overlay {
            println!(
                "[{:4}ms] OVERLAY: {} -> {}  PC=${:08X} IR=${:04X}",
                ms, prev_overlay, amiga.memory.overlay, pc, amiga.cpu.ir
            );
            // Dump vector table from chip RAM at this moment
            println!("  Chip RAM vectors at overlay change:");
            for v in 0..16u32 {
                let off = (v * 4) as usize;
                let val = (amiga.memory.chip_ram[off] as u32) << 24
                    | (amiga.memory.chip_ram[off + 1] as u32) << 16
                    | (amiga.memory.chip_ram[off + 2] as u32) << 8
                    | amiga.memory.chip_ram[off + 3] as u32;
                if val != 0 {
                    println!("    Vector {:2} (${:03X}): ${:08X}", v, v * 4, val);
                }
            }
            prev_overlay = amiga.memory.overlay;
        }

        // Track writes to vector 4 (illegal instruction, $10-$13)
        let vec4 = [
            amiga.memory.chip_ram[0x10],
            amiga.memory.chip_ram[0x11],
            amiga.memory.chip_ram[0x12],
            amiga.memory.chip_ram[0x13],
        ];
        if vec4 != prev_vec4 {
            let old_val = (prev_vec4[0] as u32) << 24
                | (prev_vec4[1] as u32) << 16
                | (prev_vec4[2] as u32) << 8
                | prev_vec4[3] as u32;
            let new_val = (vec4[0] as u32) << 24
                | (vec4[1] as u32) << 16
                | (vec4[2] as u32) << 8
                | vec4[3] as u32;
            println!(
                "[{:4}ms] Vector 4 ($10): ${:08X} -> ${:08X}  PC=${:08X} IR=${:04X} overlay={}",
                ms, old_val, new_val, pc, amiga.cpu.ir, amiga.memory.overlay
            );
            prev_vec4 = vec4;
        }

        // Track writes to vector 3 (address error, $0C-$0F)
        let vec3 = [
            amiga.memory.chip_ram[0x0C],
            amiga.memory.chip_ram[0x0D],
            amiga.memory.chip_ram[0x0E],
            amiga.memory.chip_ram[0x0F],
        ];
        if vec3 != prev_vec3 {
            let old_val = (prev_vec3[0] as u32) << 24
                | (prev_vec3[1] as u32) << 16
                | (prev_vec3[2] as u32) << 8
                | prev_vec3[3] as u32;
            let new_val = (vec3[0] as u32) << 24
                | (vec3[1] as u32) << 16
                | (vec3[2] as u32) << 8
                | vec3[3] as u32;
            println!(
                "[{:4}ms] Vector 3 ($0C): ${:08X} -> ${:08X}  PC=${:08X}",
                ms, old_val, new_val, pc
            );
            prev_vec3 = vec3;
        }

        // Ring buffer: record unique PCs
        if pc != prev_pc {
            pc_ring[ring_idx] = (pc, amiga.cpu.ir, amiga.cpu.regs.sr);
            ring_idx = (ring_idx + 1) % RING_SIZE;
        }

        // Catch execution from the vector table area ($00-$FF)
        if pc < 0x100 && pc != prev_pc && !caught_pc10 {
            caught_pc10 = true;
            println!(
                "[{:4}ms] CPU executing from vector table! PC=${:08X} IR=${:04X} SR=${:04X}",
                ms, pc, amiga.cpu.ir, amiga.cpu.regs.sr
            );
            println!(
                "  D0=${:08X} D1=${:08X} A6=${:08X} A7=${:08X}",
                amiga.cpu.regs.d[0],
                amiga.cpu.regs.d[1],
                amiga.cpu.regs.a(6),
                amiga.cpu.regs.a(7)
            );
            println!("  Last {} unique PCs:", RING_SIZE);
            for j in 0..RING_SIZE {
                let idx = (ring_idx + j) % RING_SIZE;
                let (rpc, rir, rsr) = pc_ring[idx];
                if rpc != 0 {
                    println!("    PC=${:08X} IR=${:04X} SR=${:04X}", rpc, rir, rsr);
                }
            }
        }

        // Detect CPU_DETECT entry
        if pc == 0xFC0546 && pc != prev_pc && !cpu_detect_seen {
            cpu_detect_seen = true;
            println!(
                "[{:4}ms] CPU_DETECT entry: PC=${:08X} overlay={}",
                ms, pc, amiga.memory.overlay
            );
            // Dump vectors 3 and 4 as seen through the bus
            let v3_bus = (amiga.memory.read_byte(0x0C) as u32) << 24
                | (amiga.memory.read_byte(0x0D) as u32) << 16
                | (amiga.memory.read_byte(0x0E) as u32) << 8
                | amiga.memory.read_byte(0x0F) as u32;
            let v4_bus = (amiga.memory.read_byte(0x10) as u32) << 24
                | (amiga.memory.read_byte(0x11) as u32) << 16
                | (amiga.memory.read_byte(0x12) as u32) << 8
                | amiga.memory.read_byte(0x13) as u32;
            println!("  Vector 3 via bus (with overlay): ${:08X}", v3_bus);
            println!("  Vector 4 via bus (with overlay): ${:08X}", v4_bus);
            let v3_ram = (amiga.memory.chip_ram[0x0C] as u32) << 24
                | (amiga.memory.chip_ram[0x0D] as u32) << 16
                | (amiga.memory.chip_ram[0x0E] as u32) << 8
                | amiga.memory.chip_ram[0x0F] as u32;
            let v4_ram = (amiga.memory.chip_ram[0x10] as u32) << 24
                | (amiga.memory.chip_ram[0x11] as u32) << 16
                | (amiga.memory.chip_ram[0x12] as u32) << 8
                | amiga.memory.chip_ram[0x13] as u32;
            println!(
                "  Vector 3 in chip RAM (hidden by overlay): ${:08X}",
                v3_ram
            );
            println!(
                "  Vector 4 in chip RAM (hidden by overlay): ${:08X}",
                v4_ram
            );
        }

        // Stop once we're past CPU_DETECT and init is done
        if cpu_detect_seen && pc == 0xFC05B4 && amiga.cpu.ir == 0x303C {
            println!("[{:4}ms] DEFAULT_EXC handler hit — stopping", ms);
            break;
        }

        // Also stop at WARM_RESTART
        if pc == 0xFC05F0 {
            println!("[{:4}ms] WARM_RESTART — stopping", ms);
            break;
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!(
                "[{:4}ms] CPU HALTED PC=${:08X} IR=${:04X}",
                ms, pc, amiga.cpu.ir
            );
            break;
        }

        prev_pc = pc;
    }

    // Final dump of vector table
    println!("\nFinal chip RAM vector table:");
    for v in 0..16u32 {
        let off = (v * 4) as usize;
        let val = (amiga.memory.chip_ram[off] as u32) << 24
            | (amiga.memory.chip_ram[off + 1] as u32) << 16
            | (amiga.memory.chip_ram[off + 2] as u32) << 8
            | amiga.memory.chip_ram[off + 3] as u32;
        if val != 0 {
            println!("  Vector {:2} (${:03X}): ${:08X}", v, v * 4, val);
        }
    }
    println!("  overlay={}", amiga.memory.overlay);
}

/// Diagnose why boot stalls in idle loop after init.
/// Monitors InitResident calls, VERTB handler targets, and task lists.
#[test]
#[ignore]
fn test_idle_loop_diagnosis() {
    let ks_path = "../../roms/kick13.rom";
    let rom_data = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom_data.clone());

    let total_ticks: u64 = 150_000_000; // ~5.3 seconds
    let mut prev_pc: u32 = 0;
    let mut first_stop = false;
    let mut stop_count = 0u32;
    let mut unique_pcs: std::collections::HashSet<u32> = std::collections::HashSet::new();

    // Track InitResident calls
    let mut init_resident_count = 0u32;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 {
            continue;
        }

        let pc = amiga.cpu.regs.pc;
        let ms = i / (PAL_CRYSTAL_HZ / 1000);

        // Track InitCode entry at $FC0B2C (called with D0=flags mask, D1=min version)
        if pc == 0xFC0B2C && pc != prev_pc {
            println!(
                "[{:4}ms] InitCode entry: D0=${:02X}(flags mask) D1=${:02X}(min ver) A6=${:08X}",
                ms,
                amiga.cpu.regs.d[0] & 0xFF,
                amiga.cpu.regs.d[1] & 0xFF,
                amiga.cpu.regs.a(6)
            );
        }

        // Use instr_start_pc for accurate instruction-level tracing
        let ipc = amiga.cpu.instr_start_pc;

        // Detailed trace around keyboard.device init (~1759ms)
        // Log every unique instruction in $FC0B30-$FC0BC0 and the init function
        if ms >= 1755 && ms <= 1770 && ipc != prev_pc {
            if (ipc >= 0xFC0B30 && ipc <= 0xFC0BC0) || ipc >= 0xFE4F40 && ipc <= 0xFE4F60 {
                let ir = amiga.cpu.ir;
                let sr = amiga.cpu.regs.sr;
                let d0 = amiga.cpu.regs.d[0];
                let a1 = amiga.cpu.regs.a(1);
                let a2 = amiga.cpu.regs.a(2);
                let sp = amiga.cpu.regs.a(7);
                println!(
                    "  [{:4}ms] ipc=${:08X} IR=${:04X} SR=${:04X} D0=${:08X} A1=${:08X} A2=${:08X} SP=${:08X}",
                    ms, ipc, ir, sr, d0, a1, a2, sp
                );
            }
        }

        // Track InitResident calls at $FC0B58
        if pc == 0xFC0B58 && pc != prev_pc {
            init_resident_count += 1;
            let a1 = amiga.cpu.regs.a(1);
            // Read rt_Name pointer at offset $E in the RomTag
            let name = read_rom_string(&rom_data, a1, 0x0E);
            // Read rt_Pri at offset $0D and rt_Flags at offset $0A
            let (pri, flags) = if a1 >= 0xFC0000 {
                let off = (a1 - 0xFC0000) as usize;
                if off + 0x0D < rom_data.len() {
                    (rom_data[off + 0x0D] as i8, rom_data[off + 0x0A])
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            };
            println!(
                "[{:4}ms] InitResident #{}: A1=${:08X} pri={} flags=${:02X} \"{}\"",
                ms, init_resident_count, a1, pri, flags, name
            );
        }

        // After last known InitResident (intuition at ~2430ms) and before STOP,
        // trace page transitions to see what code runs in the gap
        if ms >= 2400 && ms <= 3300 && !first_stop {
            let page = pc >> 12;
            let prev_page = prev_pc >> 12;
            if page != prev_page && pc != prev_pc {
                println!(
                    "[{:4}ms] GAP PC=${:08X} IR=${:04X} SR=${:04X} D0=${:08X} A6=${:08X} A7=${:08X}",
                    ms,
                    pc,
                    amiga.cpu.ir,
                    amiga.cpu.regs.sr,
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.a(6),
                    amiga.cpu.regs.a(7)
                );
            }
        }

        // Detect first STOP (entering idle loop)
        if pc == 0xFC0F94 && !first_stop {
            first_stop = true;
            println!("\n[{:4}ms] === First STOP (idle loop entered) ===", ms);
            println!(
                "  SR=${:04X} SSP=${:08X} DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                amiga.cpu.regs.sr,
                amiga.cpu.regs.ssp,
                amiga.agnus.dmacon,
                amiga.paula.intena,
                amiga.paula.intreq
            );

            // Dump ExecBase
            let exec_base = read_chip_long(&amiga.memory.chip_ram, 4);
            println!("  ExecBase = ${:08X}", exec_base);
            if exec_base > 0 && exec_base < 0x80000 {
                let eb = exec_base as usize;
                // TaskReady list: ExecBase + $196 (head pointer)
                let task_ready = read_chip_long(&amiga.memory.chip_ram, eb + 0x196);
                let task_wait = read_chip_long(&amiga.memory.chip_ram, eb + 0x1A4);
                let task_ready_list_addr = (exec_base + 0x196) as usize;
                let task_wait_list_addr = (exec_base + 0x1A4) as usize;
                println!(
                    "  TaskReady head=${:08X} (list@${:08X}, tail@${:08X})",
                    task_ready,
                    task_ready_list_addr,
                    task_ready_list_addr + 4
                );
                println!(
                    "  TaskWait  head=${:08X} (list@${:08X}, tail@${:08X})",
                    task_wait,
                    task_wait_list_addr,
                    task_wait_list_addr + 4
                );

                // Check if TaskReady is empty (head == &lh_Tail)
                let ready_empty = task_ready == (exec_base + 0x196 + 4);
                let wait_empty = task_wait == (exec_base + 0x1A4 + 4);
                println!(
                    "  TaskReady empty={}, TaskWait empty={}",
                    ready_empty, wait_empty
                );

                // Walk task lists if non-empty
                if !ready_empty {
                    println!("  TaskReady nodes:");
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_ready as usize);
                }
                if !wait_empty {
                    println!("  TaskWait nodes:");
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_wait as usize);
                }

                // ThisTask: ExecBase + $114
                let this_task = read_chip_long(&amiga.memory.chip_ram, eb + 0x114);
                println!("  ThisTask = ${:08X}", this_task);

                // Dump the 32 bytes around TaskReady for inspection
                println!("  TaskReady raw (ExecBase+$190..+$1B0):");
                for off in (0x190..0x1B0).step_by(4) {
                    let val = read_chip_long(&amiga.memory.chip_ram, eb + off);
                    print!("    +${:03X}: ${:08X}", off, val);
                    if off == 0x196 {
                        print!(" <- TR.lh_Head");
                    }
                    if off == 0x19A {
                        print!(" <- TR.lh_Tail");
                    }
                    if off == 0x19E {
                        print!(" <- TR.lh_TailPred");
                    }
                    if off == 0x1A4 {
                        print!(" <- TW.lh_Head");
                    }
                    if off == 0x1A8 {
                        print!(" <- TW.lh_Tail");
                    }
                    if off == 0x1AC {
                        print!(" <- TW.lh_TailPred");
                    }
                    println!();
                }

                // Interrupt vectors: ExecBase + $54 (IntVects array)
                // Each IntVect is 12 bytes: iv_Data(4), iv_Code(4), iv_Node(4)
                println!("  IntVects:");
                for level in 0..7u32 {
                    let base = eb + 0x54 + (level as usize) * 12;
                    let iv_data = read_chip_long(&amiga.memory.chip_ram, base);
                    let iv_code = read_chip_long(&amiga.memory.chip_ram, base + 4);
                    let iv_node = read_chip_long(&amiga.memory.chip_ram, base + 8);
                    if iv_code != 0 {
                        println!(
                            "    Level {}: code=${:08X} data=${:08X} node=${:08X}",
                            level, iv_code, iv_data, iv_node
                        );
                    }
                }

                // ColdCapture, CoolCapture, WarmCapture
                let cold = read_chip_long(&amiga.memory.chip_ram, eb + 0x2A);
                let cool = read_chip_long(&amiga.memory.chip_ram, eb + 0x2E);
                let warm = read_chip_long(&amiga.memory.chip_ram, eb + 0x32);
                println!(
                    "  ColdCapture=${:08X} CoolCapture=${:08X} WarmCapture=${:08X}",
                    cold, cool, warm
                );
            }

            // Dump exception vectors from chip RAM
            println!("  Exception vectors:");
            for v in [24u32, 25, 26, 27, 28, 29, 30] {
                let addr = (v * 4) as usize;
                let val = read_chip_long(&amiga.memory.chip_ram, addr);
                println!("    Vector {:2} (${:03X}): ${:08X}", v, v * 4, val);
            }
        }

        // Count STOP entries
        if pc == 0xFC0F94 && pc != prev_pc {
            stop_count += 1;
        }

        // After first STOP, collect unique PCs for a while
        if first_stop && ms < 5000 {
            if pc != prev_pc {
                unique_pcs.insert(pc);
            }
        }

        // Track VERTB handler: when JSR (A5) at $FC0E94 executes
        if pc == 0xFC0E94 && pc != prev_pc && stop_count <= 3 {
            let a5 = amiga.cpu.regs.a(5);
            let a1 = amiga.cpu.regs.a(1);
            let a6 = amiga.cpu.regs.a(6);
            println!(
                "[{:4}ms] VERTB JSR (A5): A5=${:08X} A1=${:08X} A6=${:08X}",
                ms, a5, a1, a6
            );
        }

        // Track server chain walker calls: JSR (A5) at various offsets in $FC1338-$FC135E
        // The server walker loads A5 from the server node and calls it
        if (pc >= 0xFC1340 && pc <= 0xFC1360) && pc != prev_pc && stop_count <= 3 {
            let a5 = amiga.cpu.regs.a(5);
            let a1 = amiga.cpu.regs.a(1);
            println!(
                "  ServerChain PC=${:08X}: A5=${:08X} A1=${:08X}",
                pc, a5, a1
            );
        }

        prev_pc = pc;
    }

    println!(
        "\n=== Summary after {:.1}s ===",
        total_ticks as f64 / PAL_CRYSTAL_HZ as f64
    );
    println!("  InitResident calls: {}", init_resident_count);
    println!("  STOP entries: {}", stop_count);
    println!("  Unique PCs after idle: {}", unique_pcs.len());

    // Print unique PCs in sorted order (in ROM range)
    let mut sorted_pcs: Vec<u32> = unique_pcs.into_iter().filter(|&p| p >= 0xFC0000).collect();
    sorted_pcs.sort();
    println!("  Unique ROM PCs:");
    for pc in &sorted_pcs {
        // Read the opcode from ROM
        let off = (*pc - 0xFC0000) as usize;
        let opcode = if off + 1 < rom_data.len() {
            (u16::from(rom_data[off]) << 8) | u16::from(rom_data[off + 1])
        } else {
            0
        };
        println!("    ${:08X}: ${:04X}", pc, opcode);
    }
}

fn walk_task_list(ram: &[u8], rom: &[u8], head: usize) {
    let mut node = head;
    for _ in 0..10 {
        if node == 0 || node + 20 >= ram.len() {
            break;
        }
        let succ = read_chip_long(ram, node);
        if succ == 0 {
            break;
        } // reached tail
        // Task name: Task.tc_Node.ln_Name at offset $0A (within Node)
        let name_ptr = read_chip_long(ram, node + 0x0A);
        let name = read_string(ram, rom, name_ptr);
        let state = ram.get(node + 0x0F).copied().unwrap_or(0);
        let pri = ram.get(node + 0x09).copied().unwrap_or(0) as i8;
        println!(
            "    ${:06X}: succ=${:08X} state={} pri={} name=\"{}\"",
            node, succ, state, pri, name
        );
        node = succ as usize;
    }
}

fn read_string(ram: &[u8], rom: &[u8], addr: u32) -> String {
    let buf = if addr >= 0xFC0000 && addr < 0xFE0000 {
        let off = (addr - 0xFC0000) as usize;
        &rom[off..]
    } else if (addr as usize) < ram.len() {
        &ram[addr as usize..]
    } else {
        return format!("@${:08X}", addr);
    };
    let mut s = String::new();
    for &b in buf.iter().take(40) {
        if b == 0 {
            break;
        }
        s.push(b as char);
    }
    s
}

fn read_chip_long(ram: &[u8], addr: usize) -> u32 {
    if addr + 3 < ram.len() {
        (u32::from(ram[addr]) << 24)
            | (u32::from(ram[addr + 1]) << 16)
            | (u32::from(ram[addr + 2]) << 8)
            | u32::from(ram[addr + 3])
    } else {
        0
    }
}

/// Trace A2 at each InitCode loop iteration to find the triple-init cause.
/// Possibilities: (a) ResModules array has duplicate entries, (b) A2 rewinds
/// after InitResident returns, (c) loop re-entered.
#[test]
#[ignore]
fn test_triple_init_trace() {
    let ks_path = "../../roms/kick13.rom";
    let rom_data = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom_data.clone());

    let total_ticks: u64 = 200_000_000; // ~7 seconds
    let mut prev_ipc: u32 = 0;
    let mut initcode_entered = false;
    let mut loop_iter = 0u32;
    let mut resmodules_dumped = false;
    let mut last_progress_ms = 0u32;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 {
            continue;
        }

        let ipc = amiga.cpu.instr_start_pc;
        let ms = i / (PAL_CRYSTAL_HZ / 1000);

        // Detect InitCode entry at $FC0B2C
        if ipc == 0xFC0B2C && ipc != prev_ipc {
            initcode_entered = true;
            println!(
                "[{:4}ms] InitCode entry: D0=${:02X} D1=${:02X} A6=${:08X}",
                ms,
                amiga.cpu.regs.d[0] & 0xFF,
                amiga.cpu.regs.d[1] & 0xFF,
                amiga.cpu.regs.a(6)
            );
        }

        // At $FC0B34 (MOVEA.L $12C(A6),A2): dump ResModules array
        if ipc == 0xFC0B34 && !resmodules_dumped {
            resmodules_dumped = true;
            // A6 = ExecBase at this point
            let eb = amiga.cpu.regs.a(6) as usize;
            let resmod_ptr = read_chip_long(&amiga.memory.chip_ram, eb + 0x12C) as usize;
            println!("[{:4}ms] ResModules array at ${:08X}:", ms, resmod_ptr);
            for j in 0..40 {
                let entry = read_chip_long(&amiga.memory.chip_ram, resmod_ptr + j * 4);
                if entry == 0 {
                    println!("  [{:2}] ${:08X} (NULL - end)", j, entry);
                    break;
                }
                // Read name and flags from RomTag
                let (name, flags, pri) = if entry >= 0xFC0000 {
                    let off = (entry - 0xFC0000) as usize;
                    let f = if off + 0x0A < rom_data.len() {
                        rom_data[off + 0x0A]
                    } else {
                        0
                    };
                    let p = if off + 0x0D < rom_data.len() {
                        rom_data[off + 0x0D] as i8
                    } else {
                        0
                    };
                    (read_rom_string(&rom_data, entry, 0x0E), f, p)
                } else {
                    (format!("@${:08X}", entry), 0, 0)
                };
                let cold = if flags & 0x01 != 0 { "COLD" } else { "    " };
                let auto = if flags & 0x80 != 0 { "AUTO" } else { "    " };
                println!(
                    "  [{:2}] ${:08X} flags=${:02X}({} {}) pri={:3} \"{}\"",
                    j, entry, flags, cold, auto, pri, name
                );
            }
        }

        // Trace each loop iteration: $FC0B38 = MOVE.L (A2)+,D0
        if ipc == 0xFC0B38 && ipc != prev_ipc && initcode_entered {
            loop_iter += 1;
            let a2 = amiga.cpu.regs.a(2);
            let d0 = amiga.cpu.regs.d[0];
            let sp = amiga.cpu.regs.a(7);
            let sr = amiga.cpu.regs.sr;
            println!(
                "[{:4}ms] Loop #{:2}: A2=${:08X} D0=${:08X} SP=${:08X} SR=${:04X}",
                ms, loop_iter, a2, d0, sp, sr
            );
        }

        // Trace InitResident call at $FC0B58
        if ipc == 0xFC0B58 && ipc != prev_ipc && initcode_entered {
            let a1 = amiga.cpu.regs.a(1);
            let a2 = amiga.cpu.regs.a(2);
            let name = if a1 >= 0xFC0000 {
                read_rom_string(&rom_data, a1, 0x0E)
            } else {
                format!("@${:08X}", a1)
            };
            println!(
                "[{:4}ms]   -> InitResident: A1=${:08X} A2=${:08X} \"{}\"",
                ms, a1, a2, name
            );
        }

        // Trace InitResident return (BRA.S back to loop at $FC0B5C)
        if ipc == 0xFC0B5C && ipc != prev_ipc && initcode_entered {
            let a2 = amiga.cpu.regs.a(2);
            let d0 = amiga.cpu.regs.d[0];
            println!("[{:4}ms]   <- Return: D0=${:08X} A2=${:08X}", ms, d0, a2);
        }

        // Detect end of InitCode loop
        if ipc == 0xFC0B62 && ipc != prev_ipc && initcode_entered {
            println!(
                "[{:4}ms] InitCode loop done after {} iterations",
                ms, loop_iter
            );
            initcode_entered = false;
        }

        // After intuition entry, trace page transitions and key events
        if ms >= 2430 && ms <= 3300 && ipc != prev_ipc {
            // Intuition init wrapper entry/exit
            if ipc == 0xFD3DB6 {
                println!(
                    "[{:4}ms] >> Intuition init entry: D0=${:08X} A6=${:08X}",
                    ms,
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.a(6)
                );
            }
            if ipc == 0xFD3DC0 {
                println!(
                    "[{:4}ms] << Intuition init JSR returned: A6=${:08X}",
                    ms,
                    amiga.cpu.regs.a(6)
                );
            }
            if ipc == 0xFD68B4 {
                println!("[{:4}ms] >> IntuitionInit main entry at $FD68B4", ms);
            }

            // Track page transitions (256-byte pages)
            let page = ipc >> 8;
            let prev_page = prev_ipc >> 8;
            if page != prev_page && ms >= 2490 && ms <= 2700 {
                let sr = amiga.cpu.regs.sr;
                let sp = amiga.cpu.regs.a(7);
                let mode = if sr & 0x2000 != 0 { "SUP" } else { "USR" };
                println!(
                    "[{:4}ms] PAGE ${:06X} -> ${:06X} ({}) SP=${:08X} D0=${:08X} A6=${:08X}",
                    ms,
                    prev_ipc,
                    ipc,
                    mode,
                    sp,
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.a(6)
                );
            }

            // Detect when execution leaves intuition code ($FD3xxx-$FDFxxx)
            // and enters exec idle area ($FC0Fxx)
            if ipc >= 0xFC0F70 && ipc <= 0xFC0FA0 && prev_ipc >= 0xFD0000 {
                println!(
                    "[{:4}ms] !! Intuition -> Idle loop: ipc=${:08X} prev=${:08X} SR=${:04X} SP=${:08X}",
                    ms,
                    ipc,
                    prev_ipc,
                    amiga.cpu.regs.sr,
                    amiga.cpu.regs.a(7)
                );
            }
        }

        // Periodic progress
        let ms_u32 = ms as u32;
        if ms_u32 >= 2400 && ms_u32 <= 3400 && ms_u32 >= last_progress_ms + 200 {
            last_progress_ms = ms_u32;
            println!(
                "[{:4}ms] Progress: ipc=${:08X} IR=${:04X} SR=${:04X} SP=${:08X}",
                ms,
                ipc,
                amiga.cpu.ir,
                amiga.cpu.regs.sr,
                amiga.cpu.regs.a(7)
            );
        } else if ms_u32 >= last_progress_ms + 1000 {
            last_progress_ms = ms_u32;
            println!(
                "[{:4}ms] Progress: ipc=${:08X} IR=${:04X} SR=${:04X}",
                ms, ipc, amiga.cpu.ir, amiga.cpu.regs.sr
            );
        }

        // Stop at first STOP instruction
        if matches!(amiga.cpu.state, State::Stopped) {
            println!("[{:4}ms] === STOP reached ===", ms);
            // Dump task lists
            let exec_base = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
            if exec_base > 0 && exec_base + 0x1B0 < amiga.memory.chip_ram.len() {
                let task_ready = read_chip_long(&amiga.memory.chip_ram, exec_base + 0x196);
                let task_wait = read_chip_long(&amiga.memory.chip_ram, exec_base + 0x1A4);
                let ready_empty = task_ready == (exec_base as u32 + 0x196 + 4);
                println!("  TaskReady empty={} head=${:08X}", ready_empty, task_ready);
                if !ready_empty {
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_ready as usize);
                }
                let wait_empty = task_wait == (exec_base as u32 + 0x1A4 + 4);
                println!("  TaskWait empty={} head=${:08X}", wait_empty, task_wait);
                if !wait_empty {
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_wait as usize);
                }
            }
            println!(
                "  DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                amiga.agnus.dmacon, amiga.paula.intena, amiga.paula.intreq
            );
            break;
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!("[{:4}ms] CPU HALTED at PC=${:08X}", ms, amiga.cpu.regs.pc);
            break;
        }

        prev_ipc = ipc;
    }
}

/// Trace intuition.library's init function in detail.
///
/// Intuition's init at $FD68B4 never returns to the InitCode loop.
/// This test traces: page transitions, TRAP instructions, task switches,
/// and the exact transition from intuition code to the exec idle loop.
#[test]
#[ignore]
fn test_intuition_init_trace() {
    let ks_path = "../../roms/kick13.rom";
    let rom_data = fs::read(ks_path).expect("KS1.3 ROM not found at roms/kick13.rom");

    let mut amiga = Amiga::new(rom_data.clone());

    let total_ticks: u64 = 120_000_000; // ~4.2 seconds
    let mut prev_ipc: u32 = 0;
    let mut prev_this_task: u32 = 0;
    let mut intuition_entered = false;
    let mut tracing = false;
    let mut last_progress_ms = 0u32;
    let mut prev_page: u32 = 0xFFFFFFFF;
    let mut page_transition_count = 0u32;

    // Ring buffer of last 100 unique IPCs before any notable event
    const RING_SIZE: usize = 100;
    let mut ipc_ring: [(u32, u16, u16); RING_SIZE] = [(0, 0, 0); RING_SIZE]; // (ipc, ir, sr)
    let mut ring_idx: usize = 0;

    // Track when we enter intuition init
    let mut intuition_init_ms: u32 = 0;

    // Track the SSP at intuition init entry to detect stack corruption
    let mut init_entry_ssp: u32 = 0;

    for i in 0..total_ticks {
        amiga.tick();
        if i % 4 != 0 {
            continue;
        }

        let ipc = amiga.cpu.instr_start_pc;
        let ir = amiga.cpu.ir;
        let sr = amiga.cpu.regs.sr;
        let ms = (i / (PAL_CRYSTAL_HZ / 1000)) as u32;

        // Always record unique IPCs in ring buffer
        if ipc != prev_ipc {
            ipc_ring[ring_idx] = (ipc, ir, sr);
            ring_idx = (ring_idx + 1) % RING_SIZE;
        }

        // Detect intuition main init entry at $FD68B4
        if ipc == 0xFD68B4 && !intuition_entered {
            intuition_entered = true;
            tracing = true;
            intuition_init_ms = ms;
            init_entry_ssp = if sr & 0x2000 != 0 {
                amiga.cpu.regs.a(7)
            } else {
                amiga.cpu.regs.ssp
            };
            println!("=== Intuition main init entry at {}ms ===", ms);
            println!(
                "  D0=${:08X} D1=${:08X} D2=${:08X} D3=${:08X}",
                amiga.cpu.regs.d[0], amiga.cpu.regs.d[1], amiga.cpu.regs.d[2], amiga.cpu.regs.d[3]
            );
            println!(
                "  A0=${:08X} A1=${:08X} A2=${:08X} A3=${:08X}",
                amiga.cpu.regs.a(0),
                amiga.cpu.regs.a(1),
                amiga.cpu.regs.a(2),
                amiga.cpu.regs.a(3)
            );
            println!(
                "  A4=${:08X} A5=${:08X} A6=${:08X} A7=${:08X}",
                amiga.cpu.regs.a(4),
                amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6),
                amiga.cpu.regs.a(7)
            );
            println!(
                "  SR=${:04X} SSP=${:08X} USP=${:08X}",
                sr, amiga.cpu.regs.ssp, amiga.cpu.regs.usp
            );

            // Dump ExecBase ThisTask
            let eb = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
            let this_task = read_chip_long(&amiga.memory.chip_ram, eb + 0x114);
            prev_this_task = this_task;
            println!("  ExecBase=${:08X} ThisTask=${:08X}", eb, this_task);
        }

        if !tracing {
            prev_ipc = ipc;
            continue;
        }

        // Stop after 1 second past intuition entry
        if ms > intuition_init_ms + 1000 {
            println!(
                "\n=== Timeout: {}ms past intuition entry ===",
                ms - intuition_init_ms
            );
            break;
        }

        if ipc != prev_ipc {
            // --- TRAP instructions (0x4E40-0x4E4F) ---
            // On Amiga, TRAP #0 = Supervisor(), TRAP #15 = KPutStr (debug)
            if ir >= 0x4E40 && ir <= 0x4E4F {
                let trap_num = ir & 0xF;
                println!(
                    "[{:4}ms] TRAP #{} at ipc=${:08X} SR=${:04X} SP=${:08X} D0=${:08X}",
                    ms,
                    trap_num,
                    ipc,
                    sr,
                    amiga.cpu.regs.a(7),
                    amiga.cpu.regs.d[0]
                );
            }

            // --- Illegal opcode detection ---
            // Watch for the illegal-instruction vector ($10) path after a new
            // opcode fetch. This catches CPU feature probes (e.g. MOVEC on 68000)
            // without relying on decoder logging.

            // --- Page transitions (4KB pages) ---
            let page = ipc >> 12;
            if page != prev_page {
                page_transition_count += 1;
                let mode = if sr & 0x2000 != 0 { "SUP" } else { "USR" };
                let current_ssp = if sr & 0x2000 != 0 {
                    amiga.cpu.regs.a(7)
                } else {
                    amiga.cpu.regs.ssp
                };

                // Identify the region
                let region = if ipc >= 0xFD0000 && ipc < 0xFE0000 {
                    "INTUI"
                } else if ipc >= 0xFC0000 && ipc < 0xFC2000 {
                    "EXEC-low"
                } else if ipc >= 0xFC0E00 && ipc < 0xFC1000 {
                    "EXEC-int"
                } else if ipc >= 0xFC1000 && ipc < 0xFC2000 {
                    "EXEC-hi"
                } else if ipc >= 0xFC2000 && ipc < 0xFD0000 {
                    "ROM-other"
                } else if ipc < 0x80000 {
                    "CHIPRAM"
                } else {
                    "???"
                };

                // Only log transitions involving intuition code or notable areas
                // (suppress exec interrupt handler chatter when not near intuition)
                let prev_region_intui = prev_ipc >= 0xFD0000 && prev_ipc < 0xFE0000;
                let curr_region_intui = ipc >= 0xFD0000 && ipc < 0xFE0000;
                let notable = prev_region_intui || curr_region_intui
                    || (ipc >= 0xFC0E00 && ipc < 0xFC1000) // exec interrupt handler
                    || ipc < 0x80000  // chip RAM execution
                    || page_transition_count <= 50; // first 50 transitions always

                if notable {
                    println!(
                        "[{:4}ms] PAGE {:08X}->{:08X} {} ({}) SSP=${:08X} D0=${:08X} A6=${:08X}",
                        ms,
                        prev_ipc,
                        ipc,
                        mode,
                        region,
                        current_ssp,
                        amiga.cpu.regs.d[0],
                        amiga.cpu.regs.a(6)
                    );
                }

                prev_page = page;
            }

            // --- ThisTask changes (task switch detection) ---
            let eb = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
            if eb > 0 && eb + 0x118 < amiga.memory.chip_ram.len() {
                let this_task = read_chip_long(&amiga.memory.chip_ram, eb + 0x114);
                if this_task != prev_this_task {
                    let task_name = read_string(
                        &amiga.memory.chip_ram,
                        &rom_data,
                        read_chip_long(&amiga.memory.chip_ram, this_task as usize + 0x0A),
                    );
                    let old_name = if prev_this_task > 0
                        && (prev_this_task as usize + 0x0A) < amiga.memory.chip_ram.len()
                    {
                        read_string(
                            &amiga.memory.chip_ram,
                            &rom_data,
                            read_chip_long(&amiga.memory.chip_ram, prev_this_task as usize + 0x0A),
                        )
                    } else {
                        format!("${:08X}", prev_this_task)
                    };
                    println!(
                        "[{:4}ms] TASK SWITCH: \"{}\"(${:08X}) -> \"{}\"(${:08X}) at ipc=${:08X}",
                        ms, old_name, prev_this_task, task_name, this_task, ipc
                    );
                    prev_this_task = this_task;
                }
            }

            // --- Detect first entry to exec idle loop ($FC0F70-$FC0FA0) ---
            if ipc >= 0xFC0F70 && ipc <= 0xFC0FA0 {
                println!(
                    "[{:4}ms] IDLE LOOP entry at ipc=${:08X} IR=${:04X} SR=${:04X}",
                    ms, ipc, ir, sr
                );

                // Dump the last 20 unique IPCs before idle
                println!("  Last 20 IPCs before idle:");
                for j in (RING_SIZE - 20)..RING_SIZE {
                    let idx = (ring_idx + j) % RING_SIZE;
                    let (rpc, rir, rsr) = ipc_ring[idx];
                    if rpc != 0 {
                        let mode = if rsr & 0x2000 != 0 { "S" } else { "U" };
                        println!(
                            "    ipc=${:08X} IR=${:04X} SR=${:04X} {}",
                            rpc, rir, rsr, mode
                        );
                    }
                }

                // Dump task lists
                let eb = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
                let task_ready = read_chip_long(&amiga.memory.chip_ram, eb + 0x196);
                let task_wait = read_chip_long(&amiga.memory.chip_ram, eb + 0x1A4);
                let ready_empty = task_ready == (eb as u32 + 0x196 + 4);
                let wait_empty = task_wait == (eb as u32 + 0x1A4 + 4);
                println!("  TaskReady empty={} head=${:08X}", ready_empty, task_ready);
                if !ready_empty {
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_ready as usize);
                }
                println!("  TaskWait empty={} head=${:08X}", wait_empty, task_wait);
                if !wait_empty {
                    walk_task_list(&amiga.memory.chip_ram, &rom_data, task_wait as usize);
                }
                println!(
                    "  DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                    amiga.agnus.dmacon, amiga.paula.intena, amiga.paula.intreq
                );
                break;
            }

            // --- Detect Intuition init wrapper return at $FD3DC0 ---
            if ipc == 0xFD3DC0 {
                println!(
                    "[{:4}ms] << Intuition init JSR returned: D0=${:08X} A6=${:08X}",
                    ms,
                    amiga.cpu.regs.d[0],
                    amiga.cpu.regs.a(6)
                );
            }
            if ipc == 0xFD3DC6 {
                println!(
                    "[{:4}ms] << Intuition init RTS: D0=${:08X}",
                    ms, amiga.cpu.regs.d[0]
                );
            }
        }

        // Periodic progress with more detail
        if ms >= last_progress_ms + 100 {
            last_progress_ms = ms;
            let mode = if sr & 0x2000 != 0 { "SUP" } else { "USR" };
            let current_ssp = if sr & 0x2000 != 0 {
                amiga.cpu.regs.a(7)
            } else {
                amiga.cpu.regs.ssp
            };
            let region = if ipc >= 0xFD0000 && ipc < 0xFE0000 {
                "INTUI"
            } else if ipc >= 0xFC0000 && ipc < 0xFD0000 {
                "EXEC/ROM"
            } else if ipc < 0x80000 {
                "CHIPRAM"
            } else {
                "OTHER"
            };
            println!(
                "[{:4}ms] ipc=${:08X} ({}) {} IR=${:04X} SSP=${:08X} D0=${:08X} A6=${:08X}",
                ms,
                ipc,
                region,
                mode,
                ir,
                current_ssp,
                amiga.cpu.regs.d[0],
                amiga.cpu.regs.a(6)
            );
        }

        if matches!(amiga.cpu.state, State::Halted) {
            println!("[{:4}ms] CPU HALTED at PC=${:08X}", ms, amiga.cpu.regs.pc);
            break;
        }

        // Detect STOP state (idle loop reached)
        if matches!(amiga.cpu.state, State::Stopped) {
            println!("[{:4}ms] STOP instruction at ipc=${:08X}", ms, ipc);
            let eb = read_chip_long(&amiga.memory.chip_ram, 4) as usize;
            let this_task = read_chip_long(&amiga.memory.chip_ram, eb + 0x114);
            let task_name = read_string(
                &amiga.memory.chip_ram,
                &rom_data,
                read_chip_long(&amiga.memory.chip_ram, this_task as usize + 0x0A),
            );
            println!("  ThisTask=\"{}\" (${:08X})", task_name, this_task);

            // Dump last 20 IPCs
            println!("  Last 20 IPCs before STOP:");
            for j in (RING_SIZE - 20)..RING_SIZE {
                let idx = (ring_idx + j) % RING_SIZE;
                let (rpc, rir, rsr) = ipc_ring[idx];
                if rpc != 0 {
                    let mode = if rsr & 0x2000 != 0 { "S" } else { "U" };
                    println!(
                        "    ipc=${:08X} IR=${:04X} SR=${:04X} {}",
                        rpc, rir, rsr, mode
                    );
                }
            }

            // Dump task lists
            let task_ready = read_chip_long(&amiga.memory.chip_ram, eb + 0x196);
            let task_wait = read_chip_long(&amiga.memory.chip_ram, eb + 0x1A4);
            let ready_empty = task_ready == (eb as u32 + 0x196 + 4);
            let wait_empty = task_wait == (eb as u32 + 0x1A4 + 4);
            println!("  TaskReady empty={} head=${:08X}", ready_empty, task_ready);
            if !ready_empty {
                walk_task_list(&amiga.memory.chip_ram, &rom_data, task_ready as usize);
            }
            println!("  TaskWait empty={} head=${:08X}", wait_empty, task_wait);
            if !wait_empty {
                walk_task_list(&amiga.memory.chip_ram, &rom_data, task_wait as usize);
            }
            println!(
                "  DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                amiga.agnus.dmacon, amiga.paula.intena, amiga.paula.intreq
            );
            break;
        }

        prev_ipc = ipc;
    }
}

fn read_rom_string(rom: &[u8], rom_tag_addr: u32, name_offset: usize) -> String {
    if rom_tag_addr < 0xFC0000 {
        return format!("@${:08X}", rom_tag_addr);
    }
    let off = (rom_tag_addr - 0xFC0000) as usize;
    if off + name_offset + 3 >= rom.len() {
        return "??".into();
    }
    let name_ptr = (u32::from(rom[off + name_offset]) << 24)
        | (u32::from(rom[off + name_offset + 1]) << 16)
        | (u32::from(rom[off + name_offset + 2]) << 8)
        | u32::from(rom[off + name_offset + 3]);
    if name_ptr < 0xFC0000 {
        return format!("@${:08X}", name_ptr);
    }
    let noff = (name_ptr - 0xFC0000) as usize;
    let mut s = String::new();
    for j in 0..40 {
        if noff + j >= rom.len() {
            break;
        }
        let ch = rom[noff + j];
        if ch == 0 {
            break;
        }
        s.push(ch as char);
    }
    s
}

#[test]
#[ignore] // Requires real KS 2.04 ROM
fn test_boot_kick204_a500plus_screenshot() {
    let ks_path = "../../roms/kick204_37_175_a500plus.rom";
    let rom = match fs::read(ks_path) {
        Ok(r) => r,
        Err(_) => {
            eprintln!("KS 2.04 ROM not found at {ks_path}, skipping");
            return;
        }
    };

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A500Plus,
        chipset: AmigaChipset::Ecs,
        kickstart: rom,
    });

    println!(
        "Reset: SSP=${:08X} PC=${:08X} SR=${:04X}",
        amiga.cpu.regs.ssp, amiga.cpu.regs.pc, amiga.cpu.regs.sr
    );

    let total_ticks: u64 = 850_000_000; // ~30 seconds PAL
    let report_interval: u64 = 28_375_160; // ~1 second
    // One PAL frame in master ticks
    let frame_ticks: u64 = (commodore_agnus_ocs::PAL_CCKS_PER_LINE as u64)
        * (commodore_agnus_ocs::PAL_LINES_PER_FRAME as u64)
        * 8;
    let last_frame_start = total_ticks - frame_ticks;

    let mut last_report = 0u64;
    let mut last_pc = 0u32;
    let mut stuck_count = 0u32;

    // Per-scanline register snapshots during last frame
    #[derive(Clone, Default)]
    struct ScanlineRegs {
        actual_line: u16,
        bplcon0: u16,
        bplcon1: u16,
        bpl_pt: [u32; 6],
        bpl1mod: i16,
        bpl2mod: i16,
        palette: [u16; 8], // first 8 colors
        dmacon: u16,
        copper_pc: u32,
        copper_ir1: u16,
        copper_ir2: u16,
        copper_waiting: bool,
        diwstrt: u16,
        diwstop: u16,
    }
    let mut scanline_regs: Vec<ScanlineRegs> = Vec::new();
    let mut last_captured_line: i32 = -1;
    let mut capturing_last_frame = false;

    let mut blit_area_count = 0u32;
    let mut blit_line_count = 0u32;
    let mut prev_blitter_busy = false;
    let mut text_string_accessed = false;
    let mut prev_cop2lc = 0u32;

    for i in 0..total_ticks {
        amiga.tick();

        // Track COP2LC changes and dump BPL1PT from new list
        let cop2lc = amiga.copper.cop2lc;
        if cop2lc != prev_cop2lc && i > 100 {
            // Scan the new copper list for BPL1PTH/PTL writes
            let mut bpl1pt_h: Option<u16> = None;
            let mut bpl1pt_l: Option<u16> = None;
            let mut scan_addr = cop2lc;
            for _ in 0..200 {
                let a = scan_addr as usize;
                if a + 3 >= amiga.memory.chip_ram.len() { break; }
                let w1 = ((amiga.memory.chip_ram[a] as u16) << 8) | amiga.memory.chip_ram[a+1] as u16;
                let w2 = ((amiga.memory.chip_ram[a+2] as u16) << 8) | amiga.memory.chip_ram[a+3] as u16;
                if w1 == 0xFFFF && w2 == 0xFFFE { break; }
                if (w1 & 1) == 0 {
                    let reg = w1 & 0x01FE;
                    if reg == 0x0E0 { bpl1pt_h = Some(w2); }
                    if reg == 0x0E2 { bpl1pt_l = Some(w2); }
                }
                scan_addr += 4;
            }
            let bpl1pt = ((bpl1pt_h.unwrap_or(0) as u32) << 16) | bpl1pt_l.unwrap_or(0) as u32;
            eprintln!(
                "[tick {}] COP2LC: ${:08X} -> ${:08X}, BPL1PT in list=${:08X}",
                i, prev_cop2lc, cop2lc, bpl1pt
            );
            prev_cop2lc = cop2lc;
        }

        // Count blitter operations
        let busy = amiga.agnus.blitter_busy;
        if busy && !prev_blitter_busy {
            let is_line = amiga.agnus.bltcon1 & 0x0001 != 0;
            if is_line {
                blit_line_count += 1;
            } else {
                blit_area_count += 1;
                // Log area blits targeting the bitplane area
                let dpt = amiga.agnus.blt_dpt;
                let height = (amiga.agnus.bltsize >> 6) & 0x3FF;
                let width = amiga.agnus.bltsize & 0x3F;
                if dpt >= 0x018000 && dpt < 0x020000 && blit_area_count <= 100 {
                    eprintln!(
                        "[tick {}] BLIT AREA #{}: dpt=${:06X} size={}x{} con0=${:04X} con1=${:04X}",
                        i, blit_area_count, dpt, width, height,
                        amiga.agnus.bltcon0, amiga.agnus.bltcon1
                    );
                }
            }
        }
        prev_blitter_busy = busy;

        // Check if CPU accesses the "2.0 Roms" text string at $FCECC4
        if !text_string_accessed && i % 4 == 0 {
            let pc = amiga.cpu.regs.pc;
            // Check if PC is near the text string address or if any address register points to it
            if pc >= 0x00FCECC0 && pc <= 0x00FCECD0 {
                text_string_accessed = true;
                eprintln!(
                    "[tick {}] CPU near text string '2.0 Roms' at PC=${:08X}",
                    i, pc
                );
            }
            // Check address registers for the string address
            for r in 0..7 {
                let ar = amiga.cpu.regs.a(r);
                if ar >= 0x00FCECC4 && ar <= 0x00FCECD0 && !text_string_accessed {
                    text_string_accessed = true;
                    eprintln!(
                        "[tick {}] A{} points to text string: ${:08X} PC=${:08X}",
                        i, r, ar, pc
                    );
                }
            }
        }

        // Start capturing at the last frame
        if i >= last_frame_start && !capturing_last_frame {
            capturing_last_frame = true;
        }

        // Capture register state at hpos=0x20 (early in each visible line)
        // This is after the copper has had time to set up registers for this line
        if capturing_last_frame && amiga.agnus.hpos == 0x40 {
            let line = amiga.agnus.vpos as i32;
            if line != last_captured_line && line >= 0x2C && line < 0x12C {
                last_captured_line = line;
                let mut sr = ScanlineRegs::default();
                sr.actual_line = line as u16;
                sr.bplcon0 = amiga.denise.bplcon0;
                sr.bplcon1 = amiga.denise.bplcon1;
                sr.bpl_pt = amiga.agnus.bpl_pt;
                sr.bpl1mod = amiga.agnus.bpl1mod;
                sr.bpl2mod = amiga.agnus.bpl2mod;
                sr.palette[..8].copy_from_slice(&amiga.denise.palette[..8]);
                sr.dmacon = amiga.agnus.dmacon;
                sr.copper_pc = amiga.copper.pc;
                sr.copper_ir1 = amiga.copper.ir1;
                sr.copper_ir2 = amiga.copper.ir2;
                sr.copper_waiting = amiga.copper.waiting;
                sr.diwstrt = amiga.agnus.diwstrt;
                sr.diwstop = amiga.agnus.diwstop;
                scanline_regs.push(sr);
            }
        }

        if i % 4 != 0 || i - last_report < report_interval {
            continue;
        }
        last_report = i;
        let pc = amiga.cpu.regs.pc;
        let elapsed_s = i as f64 / 28_375_160.0;
        println!(
            "[{:.1}s] tick={} PC=${:08X} V={} H={}",
            elapsed_s,
            i,
            pc,
            amiga.agnus.vpos,
            amiga.agnus.hpos,
        );
        if pc == last_pc {
            stuck_count += 1;
            if stuck_count > 5 {
                println!("PC stuck at ${:08X} for {stuck_count} intervals, continuing...", pc);
            }
        } else {
            stuck_count = 0;
        }
        last_pc = pc;
    }

    // Dump framebuffer pixels at disk area line
    // Display line +115 from BPL1PT is at vpos = $63 + 115 = $D6 (214)
    // fb_y = vpos - DISPLAY_VSTART = 214 - 44 = 170
    let disk_fb_y = 170u32;
    println!("\n--- Framebuffer lores pixels at fb_y={} (disk line), x=190-280 ---", disk_fb_y);
    let row_base = (disk_fb_y * FB_WIDTH) as usize;
    for x in (190u32..280).step_by(5) {
        let pixel = amiga.denise.framebuffer[row_base + x as usize];
        let r = (pixel >> 16) & 0xFF;
        let g = (pixel >> 8) & 0xFF;
        let b = pixel & 0xFF;
        // Reverse-map to 12-bit: r4=(r>>4), g4=(g>>4), b4=(b>>4)
        let rgb12 = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
        // Find which palette color this matches
        let pal_match = match rgb12 as u16 {
            0x414 => "COLOR00",
            0xEA8 => "COLOR01",
            0xA76 => "COLOR02",
            0x000 => "COLOR03",
            0x238 => "COLOR04d", // disk area COLOR04
            0x226 => "COLOR05d", // disk area COLOR05
            0x987 => "COLOR06",
            0xFFF => "COLOR07",
            _ => "???",
        };
        println!("  x={:3}: ${:08X} (${:03X}) = {}", x, pixel, rgb12, pal_match);
    }

    // Also dump hires FB at same line
    println!("\n--- Framebuffer hires pixels at fb_y={}, x=400-560 (disk area) ---", disk_fb_y);
    let hires_row = (disk_fb_y * machine_amiga::commodore_denise_ocs::HIRES_FB_WIDTH) as usize;
    for x in (400u32..560).step_by(8) {
        let pixel = amiga.denise.framebuffer_hires[hires_row + x as usize];
        let r = (pixel >> 16) & 0xFF;
        let g = (pixel >> 8) & 0xFF;
        let b = pixel & 0xFF;
        let rgb12 = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
        let pal_match = match rgb12 as u16 {
            0x414 => "BG",
            0xEA8 => "C01",
            0xA76 => "C02",
            0x000 => "C03",
            0x238 => "C04d",
            0x226 => "C05d",
            0x987 => "C06",
            0xFFF => "C07",
            _ => "???",
        };
        println!("  hx={:3}: ${:08X} (${:03X}) = {}", x, pixel, rgb12, pal_match);
    }

    // Check for non-background pixels in the border region (fb_y 0-54)
    // Background color from COP1: COLOR00 = $0414 → ARGB32 = $FF441144
    let bg_color = 0xFF441144u32;
    println!("\n--- Non-background pixels in border region (fb_y 0-54) ---");
    let mut ghost_pixel_count = 0u32;
    for y in 0..55u32 {
        let row_base = (y * FB_WIDTH) as usize;
        let mut non_bg_positions: Vec<(u32, u32)> = Vec::new();
        for x in 0..FB_WIDTH {
            let pixel = amiga.denise.framebuffer[row_base + x as usize];
            if pixel != bg_color && pixel != 0xFF000000 {
                non_bg_positions.push((x, pixel));
                ghost_pixel_count += 1;
            }
        }
        if !non_bg_positions.is_empty() {
            let first_5: Vec<_> = non_bg_positions.iter().take(5).collect();
            println!(
                "  fb_y={:3}: {} non-bg pixels, first {:?}",
                y,
                non_bg_positions.len(),
                first_5
            );
        }
    }
    println!("  Total ghost pixels in border: {}", ghost_pixel_count);

    // Also check the hires FB for the same
    println!("\n--- Non-background pixels in hires border (fb_y 0-54) ---");
    let hires_fb_width = machine_amiga::commodore_denise_ocs::HIRES_FB_WIDTH;
    let mut hires_ghost_count = 0u32;
    for y in 0..55u32 {
        let row_base = (y * hires_fb_width) as usize;
        let mut non_bg_count = 0u32;
        let mut first_pos = None;
        for x in 0..hires_fb_width {
            let pixel = amiga.denise.framebuffer_hires[row_base + x as usize];
            if pixel != bg_color && pixel != 0xFF000000 {
                non_bg_count += 1;
                hires_ghost_count += 1;
                if first_pos.is_none() {
                    first_pos = Some((x, pixel));
                }
            }
        }
        if non_bg_count > 0 {
            println!(
                "  fb_y={:3}: {} non-bg pixels, first at {:?}",
                y, non_bg_count, first_pos
            );
        }
    }
    println!("  Total hires ghost pixels: {}", hires_ghost_count);

    // Diagnostic: track per-line first/last non-background pixel x positions
    // in the disk icon area. Diagonals would show as per-line x drift.
    println!("\n--- Per-line first non-bg pixel in disk area (lores FB) ---");
    for y in 130..200u32 {
        let row_base = (y * FB_WIDTH) as usize;
        let mut first_x = None;
        let mut last_x = None;
        for x in 0..FB_WIDTH {
            let pixel = amiga.denise.framebuffer[row_base + x as usize];
            if pixel != bg_color && pixel != 0xFF000000 {
                if first_x.is_none() {
                    first_x = Some(x);
                }
                last_x = Some(x);
            }
        }
        if let (Some(fx), Some(lx)) = (first_x, last_x) {
            let first_pixel = amiga.denise.framebuffer[row_base + fx as usize];
            println!(
                "  fb_y={:3}: first_x={:3} (${:08X}) last_x={:3} span={}",
                y, fx, first_pixel, lx, lx - fx + 1
            );
        }
    }

    // Diagnostic: check BPL1PT progression by reading actual pointer values.
    // The orchestrator stores bpl_pt[] in agnus. Dump what we can see.
    println!("\nBPL1PT at end: ${:08X}", amiga.agnus.bpl_pt[0]);
    println!("BPL2PT at end: ${:08X}", amiga.agnus.bpl_pt[1]);
    println!("BPL3PT at end: ${:08X}", amiga.agnus.bpl_pt[2]);

    // Save framebuffer as PNG screenshot
    let out_path = std::path::Path::new("../../test_output/amiga_boot_kick204.png");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let file = fs::File::create(out_path).expect("create screenshot file");
    let ref mut w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, FB_WIDTH, FB_HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    let mut rgba = Vec::with_capacity((FB_WIDTH * FB_HEIGHT * 4) as usize);
    for &pixel in &amiga.denise.framebuffer {
        rgba.push(((pixel >> 16) & 0xFF) as u8); // R
        rgba.push(((pixel >> 8) & 0xFF) as u8); // G
        rgba.push((pixel & 0xFF) as u8); // B
        rgba.push(((pixel >> 24) & 0xFF) as u8); // A
    }
    writer.write_image_data(&rgba).expect("write PNG data");
    println!("\nScreenshot saved to {}", out_path.display());

    // Also save the 640-pixel-wide hires framebuffer for full-resolution evaluation
    let hires_path = std::path::Path::new("../../test_output/amiga_boot_kick204_hires.png");
    let hires_fb_width = machine_amiga::commodore_denise_ocs::HIRES_FB_WIDTH;
    let file = fs::File::create(hires_path).expect("create hires screenshot file");
    let ref mut w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, hires_fb_width, FB_HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write hires PNG header");
    let mut rgba = Vec::with_capacity((hires_fb_width * FB_HEIGHT * 4) as usize);
    for &pixel in &amiga.denise.framebuffer_hires {
        rgba.push(((pixel >> 16) & 0xFF) as u8);
        rgba.push(((pixel >> 8) & 0xFF) as u8);
        rgba.push((pixel & 0xFF) as u8);
        rgba.push(((pixel >> 24) & 0xFF) as u8);
    }
    writer.write_image_data(&rgba).expect("write hires PNG data");
    println!("Hires screenshot saved to {}", hires_path.display());

    // Dump key display register state (end-of-frame)
    println!("\nDisplay register state (end-of-frame):");
    println!("  BPLCON0 = ${:04X}", amiga.denise.bplcon0);
    println!("  BPLCON1 = ${:04X}", amiga.denise.bplcon1);
    println!("  BPLCON2 = ${:04X}", amiga.denise.bplcon2);
    println!("  DDFSTRT = ${:04X}", amiga.agnus.ddfstrt);
    println!("  DDFSTOP = ${:04X}", amiga.agnus.ddfstop);
    println!("  DIWSTRT = ${:04X}", amiga.agnus.diwstrt);
    println!("  DIWSTOP = ${:04X}", amiga.agnus.diwstop);
    println!("  BPL1PT  = ${:08X}", amiga.agnus.bpl_pt[0]);
    println!("  BPL2PT  = ${:08X}", amiga.agnus.bpl_pt[1]);
    println!("  BPL1MOD = ${:04X} ({})", amiga.agnus.bpl1mod as u16, amiga.agnus.bpl1mod);
    println!("  BPL2MOD = ${:04X} ({})", amiga.agnus.bpl2mod as u16, amiga.agnus.bpl2mod);
    println!("  DMACON  = ${:04X}", amiga.agnus.dmacon);

    // Blitter operation summary
    println!("\nBlitter ops: {} area, {} line, text_string_accessed={}", blit_area_count, blit_line_count, text_string_accessed);

    // Dump sprite state from Denise (using public fields)
    println!("\n--- Denise sprite registers ---");
    for spr in 0..8usize {
        let pos = amiga.denise.spr_pos[spr];
        let ctl = amiga.denise.spr_ctl[spr];
        let data = amiga.denise.spr_data[spr];
        let datb = amiga.denise.spr_datb[spr];
        let hstart = ((pos & 0xFF) << 1) | ((ctl & 1) as u16);
        let vstart = ((pos >> 8) & 0xFF) | (((ctl >> 2) & 1) << 8);
        let vstop = ((ctl >> 8) & 0xFF) | (((ctl >> 1) & 1) << 8);
        println!(
            "  SPR{}: POS=${:04X} CTL=${:04X} DATA=${:04X} DATB=${:04X} hstart={} vstart={} vstop={}",
            spr, pos, ctl, data, datb, hstart, vstart, vstop
        );
    }
    // Dump sprite palette (COLOR16-COLOR31)
    println!("\n--- Sprite palette (COLOR16-COLOR31) ---");
    for i in 16..32usize {
        if amiga.denise.palette[i] != 0 {
            println!("  palette[{}] = ${:04X}", i, amiga.denise.palette[i]);
        }
    }

    // Detailed horizontal BPL1 data scan at checkmark line (~line +60 from BPL1PT)
    println!("\n--- BPL1 horizontal word dump (line +60 from BPL1PT) ---");
    let scan_line_addr = 0x000188F2u32 + 60 * 70;
    for w in 0..35u32 {
        let addr = (scan_line_addr + w * 2) as usize;
        if addr + 1 < amiga.memory.chip_ram.len() {
            let word = ((amiga.memory.chip_ram[addr] as u16) << 8) | amiga.memory.chip_ram[addr + 1] as u16;
            if word != 0 {
                println!("  word {:2}: ${:04X} (addr=${:06X})", w, word, addr);
            }
        }
    }

    // Also dump line +0 (top of display)
    println!("\n--- BPL1 horizontal word dump (line +0 from BPL1PT) ---");
    for w in 0..35u32 {
        let addr = (0x000188F2u32 + w * 2) as usize;
        if addr + 1 < amiga.memory.chip_ram.len() {
            let word = ((amiga.memory.chip_ram[addr] as u16) << 8) | amiga.memory.chip_ram[addr + 1] as u16;
            if word != 0 {
                println!("  word {:2}: ${:04X} (addr=${:06X})", w, word, addr);
            }
        }
    }

    // Dump ALL 3 bitplanes at a disk area line (+115 from display start)
    // BPL1PT=$0188F2, BPL2PT=$01B098, BPL3PT=$01D83E, bytes_per_row=70
    let disk_line = 115u32;
    let bpl_starts = [0x000188F2u32, 0x0001B098u32, 0x0001D83Eu32];
    for (p, &base) in bpl_starts.iter().enumerate() {
        let line_addr = base + disk_line * 70;
        println!("\n--- BPL{} at display line +{} (addr=${:06X}) ---", p + 1, disk_line, line_addr);
        let mut line_data = String::new();
        for w in 0..35u32 {
            let addr = (line_addr + w * 2) as usize;
            if addr + 1 < amiga.memory.chip_ram.len() {
                let word = ((amiga.memory.chip_ram[addr] as u16) << 8) | amiga.memory.chip_ram[addr + 1] as u16;
                line_data.push_str(&format!("{:04X} ", word));
            }
        }
        println!("  {}", line_data.trim());
    }
    // Show expected color indices from planes: color = (BPL3 << 2) | (BPL2 << 1) | BPL1
    // For words 24-34 (disk area), show per-word expected colors
    println!("\n--- Color indices at disk line +{}, words 24-34 ---", disk_line);
    for w in 24..35u32 {
        let mut colors = [0u8; 16];
        for bit in 0..16u32 {
            let bit_pos = 15 - bit; // MSB first
            let mut c = 0u8;
            for (p, &base) in bpl_starts.iter().enumerate() {
                let addr = (base + disk_line * 70 + w * 2) as usize;
                if addr + 1 < amiga.memory.chip_ram.len() {
                    let word = ((amiga.memory.chip_ram[addr] as u16) << 8) | amiga.memory.chip_ram[addr + 1] as u16;
                    if word & (1 << bit_pos) != 0 {
                        c |= 1 << p;
                    }
                }
            }
            colors[bit as usize] = c;
        }
        let color_str: String = colors.iter().map(|c| format!("{}", c)).collect::<Vec<_>>().join("");
        println!("  word {:2}: {}", w, color_str);
    }

    // Scan BPL1 bitmap area for non-zero data (text patterns)
    // Bitmap likely starts around $018000, display at $0188F2
    let bpl1_display = 0x000188F2u32;
    let bytes_per_line = 70u32;
    println!("\n--- BPL1 bitmap scan (bitmap before and after display start) ---");
    // Scan from 40 lines before display start to 145 lines after
    for rel_line in (-40i32..145).step_by(5) {
        let line_addr = (bpl1_display as i64 + (rel_line as i64) * (bytes_per_line as i64)) as u32;
        let mut non_zero_words = 0u32;
        for w in 0..35u32 {
            let addr = (line_addr + w * 2) as usize;
            if addr + 1 < amiga.memory.chip_ram.len() {
                let word = ((amiga.memory.chip_ram[addr] as u16) << 8)
                    | amiga.memory.chip_ram[addr + 1] as u16;
                if word != 0 {
                    non_zero_words += 1;
                }
            }
        }
        if non_zero_words > 0 {
            println!(
                "  Line {:+4}: addr=${:06X} non_zero_words={}/35",
                rel_line, line_addr, non_zero_words
            );
        }
    }

    // --- WHITE STRIPE DIAGNOSTIC ---
    // Check right edge of lores FB for multiple lines
    println!("\n--- Lores FB right-edge check (x=310-319) ---");
    for y in (60u32..200).step_by(10) {
        let row_base = (y * FB_WIDTH) as usize;
        let mut edge_info = String::new();
        for x in 310u32..320 {
            let pixel = amiga.denise.framebuffer[row_base + x as usize];
            let r = (pixel >> 16) & 0xFF;
            let g = (pixel >> 8) & 0xFF;
            let b = pixel & 0xFF;
            let tag = if r == 0xFF && g == 0xFF && b == 0xFF {
                "W"
            } else if r == 0x44 && g == 0x11 && b == 0x44 {
                "."
            } else if pixel == 0xFF000000 {
                "B"
            } else {
                "?"
            };
            edge_info.push_str(tag);
        }
        println!("  y={:3}: [{}]", y, edge_info);
    }

    // Check what's at fb_x=318-319 for line 100 in detail
    let diag_y = 100u32;
    let row = (diag_y * FB_WIDTH) as usize;
    for x in 316u32..320 {
        let pixel = amiga.denise.framebuffer[row + x as usize];
        let r = (pixel >> 16) & 0xFF;
        let g = (pixel >> 8) & 0xFF;
        let b = pixel & 0xFF;
        let rgb12 = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
        println!("  y={} x={}: ${:08X} (rgb12=${:03X})", diag_y, x, pixel, rgb12);
    }

    // Check palette COLOR07
    println!("\n  Palette[7] (COLOR07) = ${:04X}", amiga.denise.palette[7]);
    println!("  Palette[0] (COLOR00) = ${:04X}", amiga.denise.palette[0]);

    // --- BITMAP STRUCTURE SEARCH ---
    // Search chip RAM for BitMap structure: BytesPerRow=70 (0x0046), Depth=3
    println!("\n--- BitMap structure search (BytesPerRow=70, Depth=3) ---");
    let chip_len = amiga.memory.chip_ram.len();
    for addr in (0..chip_len.saturating_sub(40)).step_by(2) {
        let bpr = ((amiga.memory.chip_ram[addr] as u16) << 8) | amiga.memory.chip_ram[addr + 1] as u16;
        if bpr != 70 { continue; }
        // Check Depth at offset 5
        if addr + 5 >= chip_len { continue; }
        let depth = amiga.memory.chip_ram[addr + 5];
        if depth != 3 { continue; }
        // Read Rows at offset 2
        let rows = ((amiga.memory.chip_ram[addr + 2] as u16) << 8)
            | amiga.memory.chip_ram[addr + 3] as u16;
        if rows == 0 || rows > 1024 { continue; }
        // Read Planes[0] at offset 8
        if addr + 11 >= chip_len { continue; }
        let plane0 = ((amiga.memory.chip_ram[addr + 8] as u32) << 24)
            | ((amiga.memory.chip_ram[addr + 9] as u32) << 16)
            | ((amiga.memory.chip_ram[addr + 10] as u32) << 8)
            | amiga.memory.chip_ram[addr + 11] as u32;
        let plane1 = ((amiga.memory.chip_ram[addr + 12] as u32) << 24)
            | ((amiga.memory.chip_ram[addr + 13] as u32) << 16)
            | ((amiga.memory.chip_ram[addr + 14] as u32) << 8)
            | amiga.memory.chip_ram[addr + 15] as u32;
        let plane2 = ((amiga.memory.chip_ram[addr + 16] as u32) << 24)
            | ((amiga.memory.chip_ram[addr + 17] as u32) << 16)
            | ((amiga.memory.chip_ram[addr + 18] as u32) << 8)
            | amiga.memory.chip_ram[addr + 19] as u32;
        // Only show if planes look like chip RAM addresses
        if plane0 > 0 && plane0 < 0x200000 && plane1 > 0 && plane1 < 0x200000 {
            let text_blit_1 = 0x01807Au32;
            let text_y_from_plane0 = if text_blit_1 >= plane0 {
                (text_blit_1 - plane0) as f64 / 70.0
            } else {
                -((plane0 - text_blit_1) as f64 / 70.0)
            };
            let bpl1pt_offset = if 0x188F2 >= plane0 {
                (0x188F2u32 - plane0) as f64 / 70.0
            } else {
                -((plane0 - 0x188F2) as f64 / 70.0)
            };
            println!(
                "  ${:06X}: BPR={} Rows={} Depth={} Planes=[${:06X}, ${:06X}, ${:06X}] textY={:.1} bpl1ptOff={:.1}",
                addr, bpr, rows, depth, plane0, plane1, plane2,
                text_y_from_plane0, bpl1pt_offset
            );
        }
    }

    // --- TEXT IN MEMORY CHECK ---
    // Check if text data exists at the blit destination addresses
    println!("\n--- Memory at text blit destinations ---");
    for &dpt in &[0x01807Au32, 0x0183C2u32, 0x01870Au32] {
        let mut has_data = false;
        let mut first_nonzero = None;
        for row in 0..9u32 {
            let line_addr = dpt + row * 70;
            for w in 0..7u32 {
                let a = (line_addr + w * 2) as usize;
                if a + 1 < chip_len {
                    let word = ((amiga.memory.chip_ram[a] as u16) << 8)
                        | amiga.memory.chip_ram[a + 1] as u16;
                    if word != 0 {
                        has_data = true;
                        if first_nonzero.is_none() {
                            first_nonzero = Some((row, w, word));
                        }
                    }
                }
            }
        }
        println!(
            "  dpt=${:06X}: has_data={} first_nonzero={:?}",
            dpt, has_data, first_nonzero
        );
    }

    // Decode copper lists from chip RAM
    println!("\n--- Copper list 1 (COP1LC=${:08X}) ---", amiga.copper.cop1lc);
    decode_copper_list(&amiga.memory.chip_ram, amiga.copper.cop1lc, 200);
    println!("\n--- Copper list 2 (COP2LC=${:08X}) ---", amiga.copper.cop2lc);
    decode_copper_list(&amiga.memory.chip_ram, amiga.copper.cop2lc, 200);

    // Dump per-scanline register snapshots
    println!("\n--- Per-scanline register snapshots (last frame, sampled at H=$40) ---");
    println!("Line | BPLCON0 BPU | DIWSTRT DIWSTOP | CopPC    CopWait IR1:IR2    | BPL1PT   BPL2PT   BPL3PT");
    let mut prev_bplcon0: u16 = 0xFFFF;
    let mut prev_diwstrt: u16 = 0xFFFF;
    let mut prev_copper_waiting = false;
    for (idx, sr) in scanline_regs.iter().enumerate() {
        let line = sr.actual_line;
        let bpu = (sr.bplcon0 >> 12) & 7;
        let changed = sr.bplcon0 != prev_bplcon0
            || sr.diwstrt != prev_diwstrt
            || sr.copper_waiting != prev_copper_waiting;
        if changed || idx == 0 || idx == scanline_regs.len() - 1
            || (idx % 32 == 0)
        {
            println!(
                "${:03X}  | ${:04X}  {}   | ${:04X}   ${:04X}   | ${:08X} {:5} ${:04X}:${:04X} | ${:08X} ${:08X} ${:08X}",
                line,
                sr.bplcon0, bpu,
                sr.diwstrt, sr.diwstop,
                sr.copper_pc, sr.copper_waiting, sr.copper_ir1, sr.copper_ir2,
                sr.bpl_pt[0], sr.bpl_pt[1], sr.bpl_pt[2],
            );
        }
        prev_bplcon0 = sr.bplcon0;
        prev_diwstrt = sr.diwstrt;
        prev_copper_waiting = sr.copper_waiting;
    }
}

/// Decode and print a copper list from chip RAM.
fn decode_copper_list(chip_ram: &[u8], start_addr: u32, max_instructions: usize) {
    let mut addr = start_addr;
    for _ in 0..max_instructions {
        let a = addr as usize;
        if a + 3 >= chip_ram.len() {
            println!("  ${:08X}: <out of chip RAM range>", addr);
            break;
        }
        let w1 = ((chip_ram[a] as u16) << 8) | chip_ram[a + 1] as u16;
        let w2 = ((chip_ram[a + 2] as u16) << 8) | chip_ram[a + 3] as u16;

        if w1 == 0xFFFF && w2 == 0xFFFE {
            println!("  ${:08X}: WAIT $FFFF,$FFFE (end-of-list)", addr);
            break;
        }

        if (w1 & 1) == 0 {
            // MOVE
            let reg = w1 & 0x01FE;
            println!(
                "  ${:08X}: MOVE ${:04X} -> ${:03X} ({})",
                addr, w2, reg, reg_name(reg)
            );
        } else {
            // WAIT or SKIP
            let vp = (w1 >> 8) & 0xFF;
            let hp = (w1 >> 1) & 0x7F;
            let vm = (w2 >> 8) & 0x7F;
            let hm = (w2 >> 1) & 0x7F;
            let is_skip = (w2 & 1) != 0;
            if is_skip {
                println!(
                    "  ${:08X}: SKIP V>=${:02X} H>=${:02X} (VM=${:02X} HM=${:02X})",
                    addr, vp, hp, vm, hm
                );
            } else {
                println!(
                    "  ${:08X}: WAIT V>=${:02X} H>=${:02X} (VM=${:02X} HM=${:02X})",
                    addr, vp, hp, vm, hm
                );
            }
        }
        addr += 4;
    }
}
