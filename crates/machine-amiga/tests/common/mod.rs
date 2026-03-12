//! Shared helpers for Amiga boot screenshot tests.

use machine_amiga::commodore_denise_ocs::{ViewportImage, ViewportPreset};
use machine_amiga::{AUDIO_SAMPLE_RATE, Amiga, AmigaConfig, AmigaRegion, PAL_FRAME_TICKS};
use std::fs;

/// Save a `ViewportImage` to a PNG file.
fn save_viewport_png(path: &str, viewport: &ViewportImage) {
    let path = std::path::Path::new(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let file = fs::File::create(path).expect("create PNG file");
    let w = &mut std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, viewport.width, viewport.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    let mut rgba = Vec::with_capacity((viewport.width * viewport.height * 4) as usize);
    for &pixel in &viewport.pixels {
        rgba.push(((pixel >> 16) & 0xFF) as u8);
        rgba.push(((pixel >> 8) & 0xFF) as u8);
        rgba.push((pixel & 0xFF) as u8);
        rgba.push(((pixel >> 24) & 0xFF) as u8);
    }
    writer.write_image_data(&rgba).expect("write PNG data");
}

#[allow(dead_code)]
pub const BOOT_TICKS: u64 = 850_000_000; // ~30 seconds PAL

/// Expected register values after boot. Each field is optional — only
/// `Some` values are checked.
#[derive(Default)]
#[allow(dead_code)]
pub struct BootExpect {
    /// Bits that must be SET in DMACON (e.g. 0x0100 = bitplane DMA).
    pub dmacon_set: Option<u16>,
    /// Exact BPLCON0 match.
    pub bplcon0: Option<u16>,
    /// Minimum number of non-zero pixels in the standard viewport.
    /// Catches "all black" or "all one colour" regressions.
    pub min_unique_colours: Option<usize>,
    /// Expected hash of the raw viewport pixel data. Catches any visual
    /// regression — even a single pixel change will fail the test.
    /// Generate by running the test once without this field set, then
    /// copying the printed hash value.
    pub viewport_hash: Option<u64>,
}

/// Compute a deterministic hash of viewport pixel data.
fn hash_viewport(viewport: &ViewportImage) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    viewport.width.hash(&mut hasher);
    viewport.height.hash(&mut hasher);
    viewport.pixels.hash(&mut hasher);
    hasher.finish()
}

/// Load a ROM file, returning None (with a message) if missing.
pub fn load_rom(path: &str) -> Option<Vec<u8>> {
    match fs::read(path) {
        Ok(r) => Some(r),
        Err(_) => {
            eprintln!("ROM not found at {path}, skipping");
            None
        }
    }
}

/// Run a full boot sequence, save screenshots (standard + full raster),
/// and encode a diagnostic 2 fps video via ffmpeg.
///
/// Returns `(dmacon, bplcon0, viewport_hash)`.
pub fn boot_screenshot_test(
    config: AmigaConfig,
    rom_description: &str,
    screenshot_prefix: &str,
    total_ticks: u64,
) -> (u16, u16, u64) {
    let pal = config.region == AmigaRegion::Pal;
    let mut amiga = Amiga::new_with_config(config);

    println!(
        "{}: Reset SSP=${:08X} PC=${:08X} SR=${:04X}",
        rom_description, amiga.cpu.regs.ssp, amiga.cpu.regs.pc, amiga.cpu.regs.sr
    );

    let report_interval: u64 = 28_375_160; // ~1 second
    let battclock_threshold: u64 = 2 * 28_375_160; // ~2 seconds
    let mut last_report = 0u64;

    // Video capture: one frame every 25 VBLANKs (~2 fps)
    let capture_interval = PAL_FRAME_TICKS * 25;
    let mut next_capture = capture_interval;
    let mut video_frames: Vec<u8> = Vec::new();
    let mut audio_samples: Vec<f32> = Vec::new();
    let mut frame_width = 0u32;
    let mut frame_height = 0u32;

    let mut initcode_trace_active = true;
    let mut probe_started = false;
    let mut putmsg_logged = false;
    let mut watch_active = false;
    let mut post_scsi_seen = false; // Enable detailed InitCode loop trace after scsi.device
    let mut post_scsi_bra_tick: u64 = 0; // Tick when BRA@$F80F70 fires
    let mut post_scsi_last_pc: u32 = 0; // Dedup PC trace
    let mut post_scsi_pc_count: u64 = 0; // Count unique PCs
    let mut post_scsi_last_wait_tick: u64 = 0; // Dedup Wait() trace
    let mut prev_intena_vertb: u16 = 0; // Track INTENA VERTB bit transitions

    for i in 0..total_ticks {
        amiga.tick();

        // Track INTENA VERTB (bit 5) transitions
        let curr_intena_vertb = amiga.paula.intena & 0x0020;
        if curr_intena_vertb != prev_intena_vertb {
            let elapsed_s = i as f64 / 28_375_160.0;
            eprintln!(
                "[INTENA-VERTB] tick={} ({:.4}s) INTENA=${:04X} VERTB={} PC=${:08X}",
                i, elapsed_s, amiga.paula.intena,
                if curr_intena_vertb != 0 { "ON" } else { "OFF" },
                amiga.cpu.regs.pc,
            );
            prev_intena_vertb = curr_intena_vertb;
        }

        let pc = amiga.cpu.regs.pc;
        if initcode_trace_active && i < 28_375_160 {
            if pc == 0xFB07B6 && !probe_started {
                probe_started = true;
                let d2 = amiga.cpu.regs.d[2];
                eprintln!("[SCSI] tick={} init probe BSR D2={}", i, d2 as i32);
            }
            // Trace PutMsg call in internal DoIO: A0=port, A1=message
            if pc == 0xFB0CE6 && !putmsg_logged {
                putmsg_logged = true;
                let a0 = amiga.cpu.regs.a[0];
                let a1 = amiga.cpu.regs.a[1];
                let a5 = amiga.cpu.regs.a[5];
                let cacr = amiga.cpu.regs.cacr;
                eprintln!(
                    "[DoIO] tick={} PutMsg A0(port)=${:08X} A1(msg)=${:08X} A5=${:08X} CACR=${:08X}",
                    i, a0, a1, a5, cacr
                );
                // Dump raw fast RAM bytes around the port pointer location
                let ptr_addr = a5.wrapping_add(0x26); // A5+38 where port ptr is stored
                eprintln!("[DoIO]   ptr_addr=${:08X}", ptr_addr);
                let rmem = |addr: u32| -> u8 {
                    let a = addr as usize;
                    if a < amiga.memory.chip_ram.len() {
                        amiga.memory.chip_ram[a]
                    } else {
                        let base = amiga.memory.fast_ram_base as usize;
                        if a >= base && a - base < amiga.memory.fast_ram.len() {
                            amiga.memory.fast_ram[a - base]
                        } else if a >= 0xF80000 && a < 0xF80000 + amiga.memory.kickstart.len() {
                            amiga.memory.kickstart[a - 0xF80000]
                        } else {
                            0
                        }
                    }
                };
                let r32 = |addr: u32| -> u32 {
                    u32::from(rmem(addr)) << 24
                        | u32::from(rmem(addr + 1)) << 16
                        | u32::from(rmem(addr + 2)) << 8
                        | u32::from(rmem(addr + 3))
                };
                let port_type = rmem(a0 + 8);
                let port_sigbit = rmem(a0 + 15);
                let port_sigtask = r32(a0 + 16);
                eprintln!(
                    "[DoIO]   port: type={} sigBit={} sigTask=${:08X}",
                    port_type, port_sigbit, port_sigtask
                );
                // Dump raw bytes: A5 data area around offset 32-48
                eprint!("[DoIO]   A5 area:");
                for off in (0x20u32..0x30).step_by(2) {
                    let w = (u16::from(rmem(a5 + off)) << 8) | u16::from(rmem(a5 + off + 1));
                    eprint!(" +${:02X}=${:04X}", off, w);
                }
                eprintln!();
                // Dump raw bytes around the port structure
                eprint!("[DoIO]   port raw:");
                for off in (0u32..34).step_by(2) {
                    let w = (u16::from(rmem(a0 + off)) << 8) | u16::from(rmem(a0 + off + 1));
                    eprint!(" +${:02X}=${:04X}", off, w);
                }
                eprintln!();
            }
            // Trace Wait call in internal DoIO: D0=signal mask
            if pc == 0xFB0CEC {
                let d0 = amiga.cpu.regs.d[0];
                eprintln!("[DoIO] tick={} Wait D0=${:08X}", i, d0);
            }
            // Trace inner task Wait call at $FB22C8: D0=combined signal mask
            if pc == 0xFB22C8 {
                let d0 = amiga.cpu.regs.d[0];
                let a5 = amiga.cpu.regs.a[5];
                eprintln!(
                    "[InnerTask] tick={} Wait D0=${:08X} A5=${:08X}",
                    i, d0, a5
                );
            }
            // Trace inner task after Wait returns at $FB22CC: D0=received signals
            if pc == 0xFB22CC {
                let d0 = amiga.cpu.regs.d[0];
                eprintln!("[InnerTask] tick={} Wait returned D0=${:08X}", i, d0);
            }
            // Trace inner task GetMsg at $FB239C: A0=port
            if pc == 0xFB239C {
                let a0 = amiga.cpu.regs.a[0];
                let a5 = amiga.cpu.regs.a[5];
                eprintln!(
                    "[InnerTask] tick={} GetMsg A0(port)=${:08X} A5=${:08X}",
                    i, a0, a5
                );
                let rmem = |addr: u32| -> u8 {
                    let a = addr as usize;
                    if a < amiga.memory.chip_ram.len() {
                        amiga.memory.chip_ram[a]
                    } else {
                        let base = amiga.memory.fast_ram_base as usize;
                        if a >= base && a - base < amiga.memory.fast_ram.len() {
                            amiga.memory.fast_ram[a - base]
                        } else if a >= 0xF80000 && a < 0xF80000 + amiga.memory.kickstart.len() {
                            amiga.memory.kickstart[a - 0xF80000]
                        } else {
                            0
                        }
                    }
                };
                let r32 = |addr: u32| -> u32 {
                    u32::from(rmem(addr)) << 24
                        | u32::from(rmem(addr + 1)) << 16
                        | u32::from(rmem(addr + 2)) << 8
                        | u32::from(rmem(addr + 3))
                };
                // MsgPort: mp_Node(14) + mp_Flags(1)@14 + mp_SigBit(1)@15 +
                // mp_SigTask(4)@16 + mp_MsgList(14)@20
                // mp_MsgList.lh_Head is at port+20
                let lh_head = r32(a0 + 20);
                let lh_tail = r32(a0 + 24);
                let port_sigbit = rmem(a0 + 15);
                let port_sigtask = r32(a0 + 16);
                eprintln!(
                    "[InnerTask]   port: sigBit={} sigTask=${:08X} list: head=${:08X} tail=${:08X}",
                    port_sigbit, port_sigtask, lh_head, lh_tail
                );
            }
            // Trace inner task GetMsg return at $FB23A0: D0=message (0=none)
            if pc == 0xFB23A0 {
                let d0 = amiga.cpu.regs.d[0];
                eprintln!("[InnerTask] tick={} GetMsg returned D0=${:08X}", i, d0);
            }
        }

        // Track InitCode entry at $F80F3E: D0=flags, D1=priority threshold
        if pc == 0xF80F3E && initcode_trace_active {
            let d0 = amiga.cpu.regs.d[0];
            let d1 = amiga.cpu.regs.d[1];
            let elapsed = i as f64 / 28_375_160.0;
            eprintln!(
                "[INITCODE-ENTRY] tick={} ({:.2}s) D0(flags)=${:02X} D1(startClass/pri)=${:02X} (signed={})",
                i, elapsed, d0 & 0xFF, d1 & 0xFF, d1 as i8
            );
            // Dump the ResModules table pointer from A2 (set at F80F42)
            let a6 = amiga.cpu.regs.a[6]; // ExecBase
            let rmem = |addr: u32| -> u8 {
                let a = addr as usize;
                if a < amiga.memory.chip_ram.len() {
                    amiga.memory.chip_ram[a]
                } else {
                    let base = amiga.memory.fast_ram_base as usize;
                    if a >= base && a - base < amiga.memory.fast_ram.len() {
                        amiga.memory.fast_ram[a - base]
                    } else {
                        0
                    }
                }
            };
            let r32 = |addr: u32| -> u32 {
                (u32::from(rmem(addr)) << 24)
                    | (u32::from(rmem(addr + 1)) << 16)
                    | (u32::from(rmem(addr + 2)) << 8)
                    | u32::from(rmem(addr + 3))
            };
            let resmod_ptr = r32(a6 + 0x12C);
            eprintln!("[INITCODE-ENTRY] ExecBase=${:08X} ResModules=${:08X}", a6, resmod_ptr);
            // Dump first 45 entries
            eprint!("[INITCODE-ENTRY] table:");
            for j in 0..45u32 {
                let entry = r32(resmod_ptr + j * 4);
                eprint!(" ${:08X}", entry);
                if entry == 0 { break; }
            }
            eprintln!();
            // Dump exec MemList entries to check if fast RAM pool overlaps table
            let mem_list = a6 + 0x142; // ExecBase.MemList (list header)
            let mut node = r32(mem_list); // lh_Head
            eprintln!("[INITCODE-ENTRY] MemList head=${:08X}", node);
            for _ in 0..10 {
                let next = r32(node);
                if next == 0 { break; }
                let mh_first = r32(node + 16); // mh_First (first free MemChunk)
                let lower = r32(node + 20); // mh_Lower
                let upper = r32(node + 24); // mh_Upper
                let free = r32(node + 28); // mh_Free
                let attr = (u16::from(rmem(node + 14)) << 8) | u16::from(rmem(node + 15));
                eprintln!(
                    "[INITCODE-ENTRY]   MemHeader @${:08X}: first=${:08X} lower=${:08X} upper=${:08X} free=${} attr=${:04X}",
                    node, mh_first, lower, upper, free, attr
                );
                // If this is the fast RAM header, dump the first few free chunks
                if lower >= 0x07E0_0000 {
                    let mut chunk = mh_first;
                    for _ in 0..5 {
                        if chunk == 0 { break; }
                        let mc_next = r32(chunk);
                        let mc_bytes = r32(chunk + 4);
                        eprintln!(
                            "[INITCODE-ENTRY]     MemChunk @${:08X}: next=${:08X} bytes={}",
                            chunk, mc_next, mc_bytes
                        );
                        chunk = mc_next;
                    }
                }
                node = next;
            }
        }

        // Track resident encountered at $F80F5A (A1=resident, before flag check)
        // Also dump A2 and actual table memory to diagnose scan issues.
        if pc == 0xF80F5A && initcode_trace_active && !watch_active {
            watch_active = true;
            let a1 = amiga.cpu.regs.a[1];
            let a2 = amiga.cpu.regs.a[2];
            let d0 = amiga.cpu.regs.d[0];
            let d2 = amiga.cpu.regs.d[2] as u8;
            let d3 = amiga.cpu.regs.d[3] as i8;
            // Read the table directly from fast RAM to check if it's intact
            let rmem = |addr: u32| -> u8 {
                let a = addr as usize;
                if a < amiga.memory.chip_ram.len() {
                    amiga.memory.chip_ram[a]
                } else {
                    let base = amiga.memory.fast_ram_base as usize;
                    if a >= base && a - base < amiga.memory.fast_ram.len() {
                        amiga.memory.fast_ram[a - base]
                    } else if a >= 0xF80000 && a < 0xF80000 + amiga.memory.kickstart.len() {
                        amiga.memory.kickstart[a - 0xF80000]
                    } else {
                        0
                    }
                }
            };
            let r32 = |addr: u32| -> u32 {
                (u32::from(rmem(addr)) << 24)
                    | (u32::from(rmem(addr + 1)) << 16)
                    | (u32::from(rmem(addr + 2)) << 8)
                    | u32::from(rmem(addr + 3))
            };
            // Read first 5 table entries from $07E003F8
            eprint!("[RESIDENT-CHECK] A1=${:08X} A2=${:08X} D0=${:08X} D2=${:02X} D3={:+} table[0..5]=",
                a1, a2, d0, d2, d3);
            for j in 0..5u32 {
                eprint!(" ${:08X}", r32(0x07E003F8 + j * 4));
            }
            eprintln!();
            // Also read what's at A2-4 (what was just read) and A2 (next entry)
            let a2_minus4 = a2.wrapping_sub(4);
            eprintln!("[RESIDENT-CHECK]   (A2-4)=${:08X} val=${:08X}  (A2)=${:08X} val=${:08X}",
                a2_minus4, r32(a2_minus4), a2, r32(a2));
            // Read ResModules pointer from ExecBase
            let a6 = amiga.cpu.regs.a[6];
            let resmod = r32(a6 + 0x12C);
            eprintln!("[RESIDENT-CHECK]   ExecBase=${:08X} ResModules=${:08X}", a6, resmod);
            watch_active = false;
        }

        // Track resident initialization: InitCode calls InitResident at $F80F6C
        if pc == 0xF80F6C && initcode_trace_active {
            let a1 = amiga.cpu.regs.a[1]; // Resident pointer
            // Read resident name pointer at A1+14
            let rmem = |addr: u32| -> u8 {
                let a = addr as usize;
                if a < amiga.memory.chip_ram.len() {
                    amiga.memory.chip_ram[a]
                } else {
                    let base = amiga.memory.fast_ram_base as usize;
                    if a >= base && a - base < amiga.memory.fast_ram.len() {
                        amiga.memory.fast_ram[a - base]
                    } else if a >= 0xF80000 && a < 0xF80000 + amiga.memory.kickstart.len() {
                        amiga.memory.kickstart[a - 0xF80000]
                    } else {
                        0
                    }
                }
            };
            let name_ptr = (u32::from(rmem(a1 + 14)) << 24)
                | (u32::from(rmem(a1 + 15)) << 16)
                | (u32::from(rmem(a1 + 16)) << 8)
                | u32::from(rmem(a1 + 17));
            let pri = rmem(a1 + 11) as i8;
            // Read name string (up to 30 chars)
            let mut name = String::new();
            for j in 0..30 {
                let ch = rmem(name_ptr + j);
                if ch == 0 { break; }
                name.push(ch as char);
            }
            let elapsed = i as f64 / 28_375_160.0;
            eprintln!("[INITRESIDENT] tick={} ({:.2}s) pri={:+} ${:08X} {}", i, elapsed, pri, a1, name);
            if name == "scsi.device" {
                post_scsi_seen = true;
                eprintln!("[POST-SCSI] Enabling detailed InitCode loop trace after scsi.device");
            }
        }

        // After scsi.device: trace every iteration of the InitCode loop
        if post_scsi_seen && initcode_trace_active && !watch_active {
            // $F80F70: BRA.S $F80F4A — loop-back after InitResident returns
            if pc == 0xF80F70 {
                let a2 = amiga.cpu.regs.a[2];
                let d2 = amiga.cpu.regs.d[2] as u8;
                let d3 = amiga.cpu.regs.d[3] as i8;
                let cacr = amiga.cpu.regs.cacr;
                let sr = amiga.cpu.regs.sr;
                let sp = amiga.cpu.regs.active_sp();
                eprintln!(
                    "[POST-SCSI] tick={} BRA@$F80F70 A2=${:08X} D2=${:02X} D3={:+} CACR=${:08X} SR=${:04X} SP=${:08X}",
                    i, a2, d2, d3, cacr, sr, sp
                );
                // Trace next 200 unique PCs after this point
                post_scsi_bra_tick = i;
            }
            // Log unique PCs after scsi.device InitResident is called
            // Skip the first occurrence (JSR prefetch) — track from the JSR target
            if post_scsi_bra_tick > 0 && i > post_scsi_bra_tick {
                if pc != post_scsi_last_pc {
                    post_scsi_last_pc = pc;
                    post_scsi_pc_count += 1;
                    // Log first 100 unique PCs, then every 500th
                    if post_scsi_pc_count <= 100 || post_scsi_pc_count % 500 == 0 {
                        let sr = amiga.cpu.regs.sr;
                        let sp = amiga.cpu.regs.active_sp();
                        eprintln!("[POST-SCSI] tick={} #{} PC=${:08X} SR=${:04X} SP=${:08X}", i, post_scsi_pc_count, pc, sr, sp);
                    }
                    // Flag if we ever return to InitCode loop
                    if pc >= 0xF80F3E && pc <= 0xF80F76 {
                        eprintln!("[POST-SCSI] *** RETURNED TO INITCODE at PC=${:08X} ***", pc);
                    }
                }
            }
            // $F80F4A: MOVE.L (A2)+,D0 — read next table entry
            if pc == 0xF80F4A {
                let a2 = amiga.cpu.regs.a[2];
                let d0 = amiga.cpu.regs.d[0];
                let d3 = amiga.cpu.regs.d[3] as i8;
                let sp = amiga.cpu.regs.active_sp();
                eprintln!(
                    "[POST-SCSI] tick={} MOVE.L@$F80F4A (A2)=${:08X} D0_prev=${:08X} D3={:+} SP=${:08X}",
                    i, a2, d0, d3, sp
                );
            }
            // Trace exec Wait() (LVO -318, vector at ExecBase-$13E=$07E0066E)
            // Deduplicate: only log first tick of each Wait call
            if pc == 0x07E0066E && post_scsi_last_wait_tick + 20 < i {
                post_scsi_last_wait_tick = i;
                let d0 = amiga.cpu.regs.d[0];
                let sp = amiga.cpu.regs.active_sp();
                let sr = amiga.cpu.regs.sr;
                eprintln!(
                    "[POST-SCSI] tick={} Wait() D0(signals)=${:08X} SP=${:08X} SR=${:04X}",
                    i, d0, sp, sr
                );
                // Dump stack for SIGF_SINGLE waits (bootstrap task)
                if d0 == 0x00000010 {
                    eprint!("[POST-SCSI]   stack:");
                    for off in (0u32..32).step_by(4) {
                        let addr = sp + off;
                        let a = addr as usize;
                        let val = if a < amiga.memory.chip_ram.len() {
                            u32::from(amiga.memory.chip_ram[a]) << 24
                                | u32::from(amiga.memory.chip_ram[a + 1]) << 16
                                | u32::from(amiga.memory.chip_ram[a + 2]) << 8
                                | u32::from(amiga.memory.chip_ram[a + 3])
                        } else {
                            let base = amiga.memory.fast_ram_base as usize;
                            if a >= base && a + 3 < base + amiga.memory.fast_ram.len() {
                                u32::from(amiga.memory.fast_ram[a - base]) << 24
                                    | u32::from(amiga.memory.fast_ram[a - base + 1]) << 16
                                    | u32::from(amiga.memory.fast_ram[a - base + 2]) << 8
                                    | u32::from(amiga.memory.fast_ram[a - base + 3])
                            } else {
                                0
                            }
                        };
                        eprint!(" ${:08X}", val);
                    }
                    eprintln!();
                }
            }
            // Trace exec AddTask() (LVO -282, vector at ExecBase-$11A=$07E00692)
            if pc == 0x07E00692 {
                let a1 = amiga.cpu.regs.a[1]; // Task structure
                let sp = amiga.cpu.regs.active_sp();
                eprintln!(
                    "[POST-SCSI] tick={} AddTask() A1(task)=${:08X} SP=${:08X}",
                    i, a1, sp
                );
            }
            // Trace exec DoIO() (LVO -456, vector at ExecBase-$1C8=$07E005E4)
            if pc == 0x07E005E4 {
                let a1 = amiga.cpu.regs.a[1]; // IORequest
                let sp = amiga.cpu.regs.active_sp();
                eprintln!(
                    "[POST-SCSI] tick={} DoIO() A1(ioreq)=${:08X} SP=${:08X}",
                    i, a1, sp
                );
            }
            // Trace exec OpenDevice() (LVO -444, vector at ExecBase-$1BC=$07E005F0)
            if pc == 0x07E005F0 {
                let a0 = amiga.cpu.regs.a[0]; // device name
                let a1 = amiga.cpu.regs.a[1]; // IORequest
                let d0 = amiga.cpu.regs.d[0]; // unit number
                eprintln!(
                    "[POST-SCSI] tick={} OpenDevice() A0(name)=${:08X} A1(ioreq)=${:08X} D0(unit)={}",
                    i, a0, a1, d0
                );
            }
        }

        // Track when InitCode loop terminates: $F80F72 is BEQ target (D0=0)
        if pc == 0xF80F72 && initcode_trace_active && !watch_active {
            watch_active = true;
            let a2 = amiga.cpu.regs.a[2];
            let d0 = amiga.cpu.regs.d[0];
            let d2 = amiga.cpu.regs.d[2];
            let rmem = |addr: u32| -> u8 {
                let a = addr as usize;
                if a < amiga.memory.chip_ram.len() {
                    amiga.memory.chip_ram[a]
                } else {
                    let base = amiga.memory.fast_ram_base as usize;
                    if a >= base && a - base < amiga.memory.fast_ram.len() {
                        amiga.memory.fast_ram[a - base]
                    } else {
                        0
                    }
                }
            };
            let r32 = |addr: u32| -> u32 {
                (u32::from(rmem(addr)) << 24)
                    | (u32::from(rmem(addr + 1)) << 16)
                    | (u32::from(rmem(addr + 2)) << 8)
                    | u32::from(rmem(addr + 3))
            };
            // A2 was just incremented past the zero entry
            let elapsed = i as f64 / 28_375_160.0;
            eprintln!(
                "[INITCODE-EXIT] tick={} ({:.2}s) D0=${:08X} D2=${:02X} A2=${:08X} (exit at table entry)",
                i, elapsed, d0, d2, a2
            );
            // Dump entries around the exit point
            let a2_minus8 = a2.wrapping_sub(8);
            for off in [a2_minus8, a2.wrapping_sub(4), a2, a2.wrapping_add(4)] {
                eprintln!("[INITCODE-EXIT]   @${:08X} = ${:08X}", off, r32(off));
            }
            // Also dump entries #19-#25 from the table at $07E003F8
            let table_base = 0x07E003F8u32;
            eprint!("[INITCODE-EXIT]   table[19..26]=");
            for j in 19..26u32 {
                eprint!(" ${:08X}", r32(table_base + j * 4));
            }
            eprintln!();
            watch_active = false;
        }

        // Track when scheduler idle is reached (outside tick limit)
        if pc == 0xF813DE && initcode_trace_active {
            let elapsed = i as f64 / 28_375_160.0;
            eprintln!("[INITCODE] tick={} ({:.2}s) SCHEDULER IDLE", i, elapsed);
            let intena = amiga.paula.intena;
            let intreq = amiga.paula.intreq;
            let sr = amiga.cpu.regs.sr;
            let vpos = amiga.agnus.vpos;
            let hpos = amiga.agnus.hpos;
            eprintln!(
                "[INITCODE]   INTENA=${:04X} INTREQ=${:04X} SR=${:04X} vpos={} hpos={} vertb_count={}",
                intena, intreq, sr, vpos, hpos, amiga.vertb_count
            );
            // Dump VBR and level-3 autovector
            let vbr = amiga.cpu.regs.vbr;
            let rmem = |addr: u32| -> u8 {
                let a = addr as usize;
                if a < amiga.memory.chip_ram.len() {
                    amiga.memory.chip_ram[a]
                } else {
                    let base = amiga.memory.fast_ram_base as usize;
                    if a >= base && a - base < amiga.memory.fast_ram.len() {
                        amiga.memory.fast_ram[a - base]
                    } else if a >= 0xF80000 && a < 0xF80000 + amiga.memory.kickstart.len() {
                        amiga.memory.kickstart[a - 0xF80000]
                    } else {
                        0
                    }
                }
            };
            let r32 = |addr: u32| -> u32 {
                u32::from(rmem(addr)) << 24
                    | u32::from(rmem(addr + 1)) << 16
                    | u32::from(rmem(addr + 2)) << 8
                    | u32::from(rmem(addr + 3))
            };
            eprintln!("[INITCODE]   VBR=${:08X}", vbr);
            // Dump exception vectors 24-31 (autovectors levels 1-7)
            for vec_num in 24u32..=31 {
                let addr = vbr + vec_num * 4;
                let target = r32(addr);
                eprintln!("[INITCODE]   vec[{}] @${:08X} -> ${:08X}", vec_num, addr, target);
            }
            // Dump ExecBase IntVects for level 3 (PORTS=1, COPER=2, VERTB=3, BLIT=4, EXTERN=5)
            // exec IntVects at ExecBase+$54: 6 entries x 12 bytes each
            let exec_base = r32(4); // ExecBase from memory location 4
            eprintln!("[INITCODE]   ExecBase=${:08X}", exec_base);
            // IntVects[n] at ExecBase+$54+n*12: iv_Data(4) + iv_Code(4) + iv_Node(4)
            for iv in 0u32..6 {
                let base_off = 0x54 + iv * 12;
                let iv_data = r32(exec_base + base_off);
                let iv_code = r32(exec_base + base_off + 4);
                let iv_node = r32(exec_base + base_off + 8);
                eprintln!(
                    "[INITCODE]   IntVect[{}] @+${:02X}: data=${:08X} code=${:08X} node=${:08X}",
                    iv, base_off, iv_data, iv_code, iv_node
                );
            }
            // Dump VERTB interrupt server list (IntVect[5].iv_Data = list pointer)
            // VERTB is INTREQ bit 5 → IntVect[5] @ ExecBase+$90
            let vertb_iv = exec_base + 0x54 + 5 * 12;
            let vertb_list_addr = r32(vertb_iv); // iv_Data = list base
            let list_head = r32(vertb_list_addr); // lh_Head
            let list_tail = r32(vertb_list_addr + 4); // lh_Tail
            let list_tailpred = r32(vertb_list_addr + 8); // lh_TailPred
            eprintln!(
                "[INITCODE]   VERTB server list @${:08X}: head=${:08X} tail=${:08X} tailpred=${:08X}",
                vertb_list_addr, list_head, list_tail, list_tailpred
            );
            // An empty list has lh_Head -> &lh_Tail, lh_TailPred -> &lh_Head
            let empty_head = vertb_list_addr + 4; // &lh_Tail
            let is_empty = list_head == empty_head;
            eprintln!("[INITCODE]   VERTB list empty={}", is_empty);
            if !is_empty && list_head != 0 {
                // Walk the server chain: each is an Interrupt structure
                // Interrupt: ln_Succ(4) + ln_Pred(4) + ln_Type(1) + ln_Pri(1) + ln_Name(4) + is_Data(4) + is_Code(4)
                let mut node = list_head;
                for s in 0..10 {
                    let succ = r32(node); // ln_Succ
                    if succ == 0 { break; } // null pointer
                    let ln_name = r32(node + 10); // ln_Name at offset 10
                    let is_data = r32(node + 14); // is_Data at offset 14
                    let is_code = r32(node + 18); // is_Code at offset 18
                    let pri = rmem(node + 9) as i8;
                    let mut name = String::new();
                    if ln_name != 0 {
                        for c in 0..32u32 {
                            let ch = rmem(ln_name + c);
                            if ch == 0 { break; }
                            name.push(ch as char);
                        }
                    }
                    eprintln!(
                        "[INITCODE]   VERTB server[{}] @${:08X}: pri={:+} data=${:08X} code=${:08X} \"{}\"",
                        s, node, pri, is_data, is_code, name
                    );
                    if succ == vertb_list_addr + 4 { break; } // end of list
                    node = succ;
                }
            }
            // Dump INTREQ clear word at iv_Data+14 (used by server chain RTS)
            let intreq_clear = (u16::from(rmem(vertb_list_addr + 14)) << 8)
                | u16::from(rmem(vertb_list_addr + 15));
            eprintln!("[INITCODE]   VERTB INTREQ clear word @${:08X} = ${:04X}", vertb_list_addr + 14, intreq_clear);
            // Dump code at ExecBase-36 (VERTB post-processing return target)
            let post_vertb = exec_base.wrapping_sub(36);
            eprint!("[INITCODE]   code@ExecBase-36 (${:08X}):", post_vertb);
            for off in (0u32..16).step_by(2) {
                let w = (u16::from(rmem(post_vertb + off)) << 8) | u16::from(rmem(post_vertb + off + 1));
                eprint!(" {:04X}", w);
            }
            eprintln!();
            // Dump ExecBase VBlank fields
            // exec V37+: VBlankFrequency=UBYTE@$212, PowerSupplyFrequency=UBYTE@$213
            let vblank_freq = rmem(exec_base + 0x212);
            let power_freq = rmem(exec_base + 0x213);
            eprintln!("[INITCODE]   VBlankFrequency={} PowerSupplyFrequency={}", vblank_freq, power_freq);
            // Also dump Elapsed($122) and Quantum($120) for task scheduling
            let quantum = (u16::from(rmem(exec_base + 0x120)) << 8) | u16::from(rmem(exec_base + 0x121));
            let elapsed = (u16::from(rmem(exec_base + 0x122)) << 8) | u16::from(rmem(exec_base + 0x123));
            eprintln!("[INITCODE]   Quantum={} Elapsed={}", quantum, elapsed);
            // Dump graphics.library VBlank task signal list (GfxBase+$C0)
            // is_Data for graphics VERTB server is at the first server's data field
            let gfx_data = 0x07E035F0u32; // graphics.library VERTB is_Data
            let vblank_list_addr = r32(gfx_data + 0xC0);
            eprintln!("[INITCODE]   GfxBase is_Data=${:08X} +$C0 -> list ptr ${:08X}", gfx_data, vblank_list_addr);
            // Read the list structure: this is an exec List (lh_Head, lh_Tail, lh_TailPred)
            // Actually, +$C0 might be a direct pointer, let's dump the raw memory around it
            eprint!("[INITCODE]   GfxData[$C0..$D0]:");
            for off in (0xC0u32..0xD0).step_by(4) {
                eprint!(" ${:08X}", r32(gfx_data + off));
            }
            eprintln!();
            // Also dump GfxData[$156] (the flag checked by VERTB server)
            let flag_156 = (u16::from(rmem(gfx_data + 0x156)) << 8) | u16::from(rmem(gfx_data + 0x157));
            eprintln!("[INITCODE]   GfxData+$156 = ${:04X}", flag_156);
            // Dump TaskReady and TaskWait lists
            let task_ready = exec_base + 0x196; // TaskReady list header (V37 ExecBase)
            let ready_head = r32(task_ready);
            let ready_empty = ready_head == task_ready + 4;
            eprintln!("[INITCODE]   TaskReady @${:08X}: head=${:08X} empty={}", task_ready, ready_head, ready_empty);
            // Walk TaskWait list
            let task_wait = exec_base + 0x1A4; // TaskWait
            let mut wait_node = r32(task_wait);
            let wait_end = task_wait + 4;
            eprintln!("[INITCODE]   TaskWait @${:08X}:", task_wait);
            for w in 0..15u32 {
                let succ = r32(wait_node);
                if succ == 0 || wait_node == wait_end { break; }
                let name_ptr = r32(wait_node + 10);
                let sig_wait = r32(wait_node + 0x16); // tc_SigWait at +$16 in Task
                let sig_recvd = r32(wait_node + 0x1A); // tc_SigRecvd at +$1A
                let mut name = String::new();
                if name_ptr != 0 {
                    for c in 0..32u32 {
                        let ch = rmem(name_ptr + c);
                        if ch == 0 { break; }
                        name.push(ch as char);
                    }
                }
                eprintln!(
                    "[INITCODE]   wait[{}] @${:08X}: sigWait=${:08X} sigRecvd=${:08X} \"{}\"",
                    w, wait_node, sig_wait, sig_recvd, name
                );
                wait_node = succ;
            }
            // Dump CIA-A and CIA-B state
            eprintln!(
                "[INITCODE]   CIA-A: TOD={:06X} alarm={:06X} ICR_mask=${:02X} ICR_status=${:02X} irq={} tod_pulses={}",
                amiga.cia_a.tod_counter(), amiga.cia_a.tod_alarm(),
                amiga.cia_a.icr_mask(), amiga.cia_a.icr_status(),
                amiga.cia_a.irq_active(), amiga.cia_a_tod_pulse_count,
            );
            eprintln!(
                "[INITCODE]   CIA-A: CRA=${:02X} CRB=${:02X} TA={:04X} TB={:04X} TA_run={} TB_run={}",
                amiga.cia_a.cra(), amiga.cia_a.crb(),
                amiga.cia_a.timer_a(), amiga.cia_a.timer_b(),
                amiga.cia_a.timer_a_running(), amiga.cia_a.timer_b_running(),
            );
            eprintln!(
                "[INITCODE]   CIA-B: TOD={:06X} alarm={:06X} ICR_mask=${:02X} ICR_status=${:02X} irq={}",
                amiga.cia_b.tod_counter(), amiga.cia_b.tod_alarm(),
                amiga.cia_b.icr_mask(), amiga.cia_b.icr_status(),
                amiga.cia_b.irq_active(),
            );
            eprintln!(
                "[INITCODE]   CIA-B: CRA=${:02X} CRB=${:02X} TA={:04X} TB={:04X} TA_run={} TB_run={}",
                amiga.cia_b.cra(), amiga.cia_b.crb(),
                amiga.cia_b.timer_a(), amiga.cia_b.timer_b(),
                amiga.cia_b.timer_a_running(), amiga.cia_b.timer_b_running(),
            );
            // Dump PORTS (CIA-A) server chain (IntVect[3])
            let ports_iv = exec_base + 0x54 + 3 * 12;
            let ports_list_addr = r32(ports_iv);
            let ports_head = r32(ports_list_addr);
            let ports_empty_head = ports_list_addr + 4;
            eprintln!(
                "[INITCODE]   PORTS(IntVect[3]) list @${:08X}: head=${:08X} empty={}",
                ports_list_addr, ports_head, ports_head == ports_empty_head
            );
            if ports_head != ports_empty_head && ports_head != 0 {
                let mut node = ports_head;
                for s in 0..10 {
                    let succ = r32(node);
                    if succ == 0 { break; }
                    let ln_name = r32(node + 10);
                    let is_data = r32(node + 14);
                    let is_code = r32(node + 18);
                    let pri = rmem(node + 9) as i8;
                    let mut name = String::new();
                    if ln_name != 0 {
                        for c in 0..32u32 {
                            let ch = rmem(ln_name + c);
                            if ch == 0 { break; }
                            name.push(ch as char);
                        }
                    }
                    eprintln!(
                        "[INITCODE]   PORTS[{}] @${:08X}: pri={:+} data=${:08X} code=${:08X} \"{}\"",
                        s, node, pri, is_data, is_code, name
                    );
                    if succ == ports_list_addr + 4 { break; }
                    node = succ;
                }
            }
            // Dump EXTER (CIA-B) IntVect[13] server chain
            {
                let exter_iv_off = 0x54 + 13u32 * 12;
                let exter_list_addr = r32(exec_base + exter_iv_off);
                let exter_code = r32(exec_base + exter_iv_off + 4);
                eprintln!(
                    "[INITCODE]   EXTER(IntVect[13]) @+${:02X}: data=${:08X} code=${:08X}",
                    exter_iv_off, exter_list_addr, exter_code,
                );
                let exter_head = r32(exter_list_addr);
                let exter_empty = exter_list_addr + 4;
                if exter_head != exter_empty && exter_head != 0 {
                    let mut node = exter_head;
                    for s in 0..10 {
                        let succ = r32(node);
                        if succ == 0 { break; }
                        let ln_name = r32(node + 10);
                        let is_data = r32(node + 14);
                        let is_code = r32(node + 18);
                        let pri = rmem(node + 9) as i8;
                        let mut name = String::new();
                        if ln_name != 0 {
                            for c in 0..32u32 {
                                let ch = rmem(ln_name + c);
                                if ch == 0 { break; }
                                name.push(ch as char);
                            }
                        }
                        eprintln!(
                            "[INITCODE]   EXTER[{}] @${:08X}: pri={:+} data=${:08X} code=${:08X} \"{}\"",
                            s, node, pri, is_data, is_code, name
                        );
                        if succ == exter_empty { break; }
                        node = succ;
                    }
                }
            }
            // Dump ThisTask
            let this_task = r32(exec_base + 0x114);
            if this_task != 0 {
                let task_name_ptr = r32(this_task + 10);
                let mut task_name = String::new();
                if task_name_ptr != 0 {
                    for c in 0..32u32 {
                        let ch = rmem(task_name_ptr + c);
                        if ch == 0 { break; }
                        task_name.push(ch as char);
                    }
                }
                let task_state = rmem(this_task + 0x0F); // tc_State
                eprintln!(
                    "[INITCODE]   ThisTask=${:08X} state={} \"{}\"",
                    this_task, task_state, task_name
                );
            }
            // Dump timer.device unit lists
            {
                let vertb_list = r32(exec_base + 0x54 + 5 * 12);
                let mut tnode = r32(vertb_list);
                let tend = vertb_list + 4;
                let mut timer_data = 0u32;
                while tnode != 0 && tnode != tend {
                    let tsucc = r32(tnode);
                    let tname_ptr = r32(tnode + 10);
                    let mut tname = String::new();
                    if tname_ptr != 0 {
                        for c in 0..20u32 {
                            let ch = rmem(tname_ptr + c);
                            if ch == 0 { break; }
                            tname.push(ch as char);
                        }
                    }
                    if tname == "timer.device" {
                        timer_data = r32(tnode + 14);
                        break;
                    }
                    if tsucc == tend { break; }
                    tnode = tsucc;
                }
                if timer_data != 0 {
                    eprintln!("[INITCODE]   timer.device data=${:08X}", timer_data);
                    for (name, off) in [("UNIT_VBLANK", 0xA8u32), ("UNIT_MICROHZ", 0xB4u32)] {
                        let list_addr = timer_data + off;
                        let head = r32(list_addr);
                        let empty = list_addr + 4;
                        let empty_flag = head == empty || head == 0;
                        eprintln!(
                            "[INITCODE]   timer {} @${:08X}: head=${:08X} empty={}",
                            name, list_addr, head, empty_flag
                        );
                        if !empty_flag {
                            let mut n = head;
                            for s in 0..5 {
                                let ns = r32(n);
                                if ns == 0 { break; }
                                let io_secs = r32(n + 0x20);
                                let io_micro = r32(n + 0x24);
                                let reply_port = r32(n + 0x0E);
                                let rp_task = if reply_port != 0 { r32(reply_port + 0x10) } else { 0 };
                                let mut task_name = String::new();
                                if rp_task != 0 {
                                    let tn = r32(rp_task + 10);
                                    if tn != 0 {
                                        for c in 0..32u32 {
                                            let ch = rmem(tn + c);
                                            if ch == 0 { break; }
                                            task_name.push(ch as char);
                                        }
                                    }
                                }
                                eprintln!(
                                    "[INITCODE]   timer {}[{}] @${:08X}: secs={} micro={} rpTask=${:08X} \"{}\"",
                                    name, s, n, io_secs, io_micro, rp_task, task_name
                                );
                                if ns == empty { break; }
                                n = ns;
                            }
                        }
                    }
                }
            }
            // Dump DMAC state if present
            if let Some(ref dmac) = amiga.dmac {
                let istr = dmac.current_istr();
                let irq = dmac.irq_pending();
                eprintln!(
                    "[INITCODE]   DMAC: ISTR=${:02X} irq_pending={} cntr=${:02X}",
                    istr, irq, dmac.cntr()
                );
                let asr = dmac.wd_asr();
                let status = dmac.wd_scsi_status();
                eprintln!(
                    "[INITCODE]   WD33C93: ASR=${:02X} status=${:02X}",
                    asr, status
                );
            }
            initcode_trace_active = false;
        }

        // Battclock simulation disabled — it corrupts the CIA-A TOD
        // counter, preventing timer.device from calibrating the EClock
        // frequency. The divisor at GfxBase+$22 stays 0, causing a
        // DIVU by zero in the STRAP display routine.
        // TODO: implement proper battclock.resource instead of
        // force-setting the TOD counter.
        let _ = battclock_threshold;

        // Capture a video frame + drain audio periodically
        if i >= next_capture {
            next_capture += capture_interval;
            let vp = amiga
                .denise
                .extract_viewport(ViewportPreset::Standard, pal, true);
            frame_width = vp.width;
            frame_height = vp.height;
            for &pixel in &vp.pixels {
                video_frames.push(((pixel >> 16) & 0xFF) as u8);
                video_frames.push(((pixel >> 8) & 0xFF) as u8);
                video_frames.push((pixel & 0xFF) as u8);
                video_frames.push(0xFF);
            }
            audio_samples.extend_from_slice(&amiga.take_audio_buffer());
        }

        if i % 4 != 0 || i - last_report < report_interval {
            continue;
        }
        last_report = i;
        let elapsed_s = i as f64 / 28_375_160.0;
        let stopped = matches!(amiga.cpu.state, motorola_68000::cpu::State::Stopped);
        println!(
            "[{:.1}s] tick={} PC=${:08X} V={} H={} SR=${:04X} INTENA=${:04X} INTREQ=${:04X} stopped={} vertb={}",
            elapsed_s, i, amiga.cpu.regs.pc, amiga.agnus.vpos, amiga.agnus.hpos,
            amiga.cpu.regs.sr, amiga.paula.intena, amiga.paula.intreq, stopped, amiga.vertb_count,
        );
    }

    // End-of-run state dump
    eprintln!(
        "[END] PC=${:08X} SR=${:04X} SSP=${:08X} initcode_done={}",
        amiga.cpu.regs.pc, amiga.cpu.regs.sr, amiga.cpu.regs.ssp,
        !initcode_trace_active
    );
    eprintln!("[END] COLOR00=${:03X} COLOR01=${:03X} COLOR17=${:03X} COLOR21=${:03X}",
        amiga.denise.palette[0], amiga.denise.palette[1],
        amiga.denise.palette[17], amiga.denise.palette[21]);
    // Dump sprite data from chip RAM at SPR0PT area ($0490)
    let spr_base = 0x0490usize;
    if spr_base + 8 < amiga.memory.chip_ram.len() {
        let w0 = (u16::from(amiga.memory.chip_ram[spr_base]) << 8)
            | u16::from(amiga.memory.chip_ram[spr_base + 1]);
        let w1 = (u16::from(amiga.memory.chip_ram[spr_base + 2]) << 8)
            | u16::from(amiga.memory.chip_ram[spr_base + 3]);
        eprintln!("[END] SPR0 data @$0490: pos=${:04X} ctl=${:04X}", w0, w1);
    }

    // Drain any remaining audio
    audio_samples.extend_from_slice(&amiga.take_audio_buffer());

    // Save raster framebuffer screenshots (standard, display-scaled, full raster)
    let viewport = amiga
        .denise
        .extract_viewport(ViewportPreset::Standard, pal, true);
    let vp_hash = hash_viewport(&viewport);
    println!("Viewport hash: 0x{vp_hash:016X}");
    {
        // Raw superhires screenshot (1280×256 PAL, 1280×200 NTSC)
        let std_path_str = format!("../../test_output/amiga/{screenshot_prefix}.png");
        save_viewport_png(&std_path_str, &viewport);
        println!(
            "Screenshot saved to {} ({}x{})",
            std_path_str, viewport.width, viewport.height,
        );

        // Display-resolution screenshot (720×540, correct 4:3 PAR)
        let display = viewport.to_display();
        let display_path_str = format!("../../test_output/amiga/{screenshot_prefix}_display.png");
        save_viewport_png(&display_path_str, &display);
        println!(
            "Display screenshot saved to {} ({}x{})",
            display_path_str, display.width, display.height,
        );

        // Full raster (debug)
        let full = amiga
            .denise
            .extract_viewport(ViewportPreset::Full, pal, true);
        let full_path_str = format!("../../test_output/amiga/{screenshot_prefix}_full.png");
        save_viewport_png(&full_path_str, &full);
        println!(
            "Full raster saved to {} ({}x{})",
            full_path_str, full.width, full.height,
        );
    }

    // Encode captured frames + audio to MP4 via ffmpeg
    if frame_width > 0 && !video_frames.is_empty() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mp4_path = format!("../../test_output/amiga/{screenshot_prefix}.mp4");

        // Write raw f32le PCM audio to a temp file for ffmpeg's second input
        let audio_tmp = format!("../../test_output/amiga/.{screenshot_prefix}_audio.raw");
        {
            let mut f = fs::File::create(&audio_tmp).expect("create audio temp file");
            let pcm_bytes: Vec<u8> = audio_samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            f.write_all(&pcm_bytes).expect("write audio temp file");
        }

        let sample_rate_str = AUDIO_SAMPLE_RATE.to_string();
        let video_size_str = format!("{frame_width}x{frame_height}");
        match Command::new("ffmpeg")
            .args([
                "-y",
                // Video input: raw RGBA frames on stdin
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                &video_size_str,
                "-framerate",
                "2",
                "-i",
                "pipe:0",
                // Audio input: raw f32le stereo PCM from temp file
                "-f",
                "f32le",
                "-ar",
                &sample_rate_str,
                "-ac",
                "2",
                "-i",
                &audio_tmp,
                // Output
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-shortest",
                &mp4_path,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(&video_frames)
                    .expect("pipe video frames");
                let output = child.wait_with_output().expect("ffmpeg");
                if output.status.success() {
                    println!("Video saved to {mp4_path} (with audio)");
                } else {
                    eprintln!("ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
            Err(e) => {
                eprintln!("ffmpeg not found ({e}), skipping video output");
            }
        }

        // Clean up temp file
        fs::remove_file(&audio_tmp).ok();
    }

    println!("DMACON  = ${:04X}", amiga.agnus.dmacon);
    println!("BPLCON0 = ${:04X}", amiga.denise.bplcon0);
    println!("BPLCON4 = ${:04X}", amiga.denise.bplcon4);
    println!("COP1LC  = ${:08X}", amiga.copper.cop1lc);

    // Dump sprite palette entries (first 32 entries of AGA 24-bit palette)
    print!("Palette 24-bit [0..31]:");
    for idx in 0..32 {
        if idx % 8 == 0 {
            print!("\n  [{:02X}]:", idx);
        }
        print!(" ${:06X}", amiga.denise.palette_24[idx]);
    }
    println!();
    // Also dump sprite control registers
    for s in 0..8 {
        let pos = amiga.denise.spr_pos[s];
        let ctl = amiga.denise.spr_ctl[s];
        if pos != 0 || ctl != 0 {
            println!("SPR{s}: POS=${pos:04X} CTL=${ctl:04X}");
        }
    }

    // Dump ExecBase and library list for boot debugging
    let ram = &amiga.memory.chip_ram;
    let rd32 = |a: usize| -> u32 {
        if a + 3 < ram.len() {
            u32::from(ram[a]) << 24
                | u32::from(ram[a + 1]) << 16
                | u32::from(ram[a + 2]) << 8
                | u32::from(ram[a + 3])
        } else {
            0
        }
    };
    let _rd16 = |a: usize| -> u16 {
        if a + 1 < ram.len() {
            u16::from(ram[a]) << 8 | u16::from(ram[a + 1])
        } else {
            0
        }
    };
    let exec_base = rd32(4) as usize;
    println!("ExecBase = ${:08X}", exec_base);
    // ExecBase->LibList is at offset $17A (378). It's a List node.
    // List: lh_Head(4), lh_Tail(4), lh_TailPred(4), lh_Type(1), lh_pad(1)
    // Walk the library list.
    let _lib_list_head_ptr = exec_base + 0x17A;
    let read_mem = |addr: usize| -> u8 {
        if addr < ram.len() {
            ram[addr]
        } else if !amiga.memory.fast_ram.is_empty() {
            let base = amiga.memory.fast_ram_base as usize;
            if addr >= base && addr - base < amiga.memory.fast_ram.len() {
                amiga.memory.fast_ram[addr - base]
            } else if addr >= 0xF80000 && addr < 0xF80000 + amiga.memory.kickstart.len() {
                amiga.memory.kickstart[addr - 0xF80000]
            } else {
                0
            }
        } else if addr >= 0xF80000 && addr < 0xF80000 + amiga.memory.kickstart.len() {
            amiga.memory.kickstart[addr - 0xF80000]
        } else {
            0
        }
    };
    let rd32_any = |a: usize| -> u32 {
        u32::from(read_mem(a)) << 24
            | u32::from(read_mem(a + 1)) << 16
            | u32::from(read_mem(a + 2)) << 8
            | u32::from(read_mem(a + 3))
    };
    let rd16_any = |a: usize| -> u16 {
        u16::from(read_mem(a)) << 8 | u16::from(read_mem(a + 1))
    };
    // Walk a List at a given pointer, printing Name and Version
    let walk_list = |label: &str, list_ptr: usize| {
        println!("--- {} ---", label);
        let mut node = rd32_any(list_ptr) as usize;
        for _ in 0..30 {
            let next = rd32_any(node) as usize;
            if next == 0 { break; }
            let name_ptr = rd32_any(node + 10) as usize;
            let mut name = String::new();
            for j in 0..40 {
                let c = read_mem(name_ptr + j);
                if c == 0 { break; }
                name.push(c as char);
            }
            let version = rd16_any(node + 20);
            let revision = rd16_any(node + 22);
            println!("  ${:08X}: {} v{}.{}", node, name, version, revision);
            node = next;
        }
    };
    walk_list("Library list (ExecBase+$17A)", exec_base + 0x17A);
    walk_list("Device list (ExecBase+$15E)", exec_base + 0x15E);
    walk_list("Resource list (ExecBase+$150)", exec_base + 0x150);

    // Dump exec task lists for scheduler debugging
    {
        let this_task = rd32_any(exec_base + 0x10C) as usize;
        println!("--- Exec task state ---");
        println!("  ThisTask (ExecBase+$10C) = ${:08X}", this_task);
        if this_task > 0 {
            let name_ptr = rd32_any(this_task + 10) as usize;
            let mut name = String::new();
            for j in 0..40 {
                let c = read_mem(name_ptr + j);
                if c == 0 { break; }
                name.push(c as char);
            }
            let state = read_mem(this_task + 15);
            println!("    name=\"{}\" state={}", name, state);
        }
        // TaskReady list at ExecBase+$11A
        let walk_tasks = |label: &str, list_ptr: usize| {
            println!("  {} ---", label);
            let mut node = rd32_any(list_ptr) as usize;
            for _ in 0..20 {
                let next = rd32_any(node) as usize;
                if next == 0 { break; }
                let name_ptr = rd32_any(node + 10) as usize;
                let mut name = String::new();
                for j in 0..40 {
                    let c = read_mem(name_ptr + j);
                    if c == 0 { break; }
                    name.push(c as char);
                }
                let state = read_mem(node + 15);
                let sig_alloc = rd32_any(node + 18);
                let sig_wait = rd32_any(node + 22);
                let sig_recvd = rd32_any(node + 26);
                println!(
                    "    ${:08X}: \"{}\" state={} sigAlloc=${:08X} sigWait=${:08X} sigRecvd=${:08X}",
                    node, name, state, sig_alloc, sig_wait, sig_recvd
                );
                node = next;
            }
        };
        walk_tasks("TaskReady (ExecBase+$11A)", exec_base + 0x11A);
        walk_tasks("TaskWait (ExecBase+$12E)", exec_base + 0x12E);
    }

    // Dump exec MemList (ExecBase+$142) to check memory configuration
    println!("--- Memory list (ExecBase+$142) ---");
    {
        let mut node = rd32_any(exec_base + 0x142) as usize;
        for _ in 0..10 {
            let next = rd32_any(node) as usize;
            if next == 0 { break; }
            let name_ptr = rd32_any(node + 10) as usize;
            let mut name = String::new();
            for j in 0..40 {
                let c = read_mem(name_ptr + j);
                if c == 0 { break; }
                name.push(c as char);
            }
            // MemHeader: mh_Node(14), mh_Attributes(2), mh_First(4), mh_Lower(4), mh_Upper(4), mh_Free(4)
            let attrs = rd16_any(node + 14);
            let lower = rd32_any(node + 20) as usize;
            let upper = rd32_any(node + 24) as usize;
            let free = rd32_any(node + 28);
            println!(
                "  ${:08X}: {} attrs=${:04X} ${:08X}-${:08X} free={}K",
                node, name, attrs, lower, upper, free / 1024
            );
            node = next;
        }
    }

    // Dump ResModules array (ExecBase+$12C) to check RomTag presence
    {
        let res_modules_ptr = rd32_any(exec_base + 0x12C) as usize;
        println!("--- ResModules (ExecBase+$12C) = ${:08X} ---", res_modules_ptr);
        let mut ptr = res_modules_ptr;
        let mut count = 0;
        let mut found_intuition = false;
        for _ in 0..100 {
            let entry = rd32_any(ptr);
            if entry == 0 { break; } // end of array
            if (entry as i32) < 0 {
                // Redirect: clear bit 31 and follow
                let redirect = (entry & 0x7FFFFFFF) as usize;
                println!("  [redirect to ${:08X}]", redirect);
                ptr = redirect;
                continue;
            }
            let rt = entry as usize;
            // Resident structure: rt_MatchWord(2), rt_MatchTag(4), rt_EndSkip(4),
            // rt_Flags(1)@+$0A, rt_Version(1)@+$0B, rt_Type(1)@+$0C,
            // rt_Pri(1)@+$0D, rt_Name(4)@+$0E, rt_IdString(4)@+$12,
            // rt_Init(4)@+$16
            let flags = read_mem(rt + 0x0A);
            let pri = read_mem(rt + 0x0D) as i8;
            let name_ptr = rd32_any(rt + 0x0E) as usize;
            let init = rd32_any(rt + 0x16);
            let mut name = String::new();
            for j in 0..40 {
                let c = read_mem(name_ptr + j);
                if c == 0 { break; }
                name.push(c as char);
            }
            if name.contains("intuition") { found_intuition = true; }
            println!(
                "  #{}: ${:08X} flags=${:02X} pri={:+4} init=${:08X} {}",
                count, rt, flags, pri, init, name
            );
            count += 1;
            ptr += 4;
        }
        if !found_intuition {
            println!("  *** intuition.library NOT FOUND in ResModules! ***");
        }
    }

    // Dump GfxBase timing fields for STRAP debugging
    // Find GfxBase from library list
    {
        let mut node = rd32_any(exec_base + 0x17A) as usize;
        for _ in 0..30 {
            let next = rd32_any(node) as usize;
            if next == 0 { break; }
            let name_ptr = rd32_any(node + 10) as usize;
            let mut name = String::new();
            for j in 0..40 {
                let c = read_mem(name_ptr + j);
                if c == 0 { break; }
                name.push(c as char);
            }
            if name == "graphics.library" {
                let gfx_base = node;
                println!("--- GfxBase=${:08X} ---", gfx_base);
                // Correct GfxBase field offsets (from graphics/gfxbase.h):
                //   +$22: ActiView (APTR)
                //   +$CE: DisplayFlags (UWORD)
                //   +$D4: MaxDisplayRow (UWORD)
                //   +$D6: MaxDisplayColumn (UWORD)
                //   +$D8: NormalDisplayRows (UWORD)
                //   +$DA: NormalDisplayColumns (UWORD)
                //   +$E2: MicrosPerLine (UWORD)
                //   +$E4: MinDisplayColumn (UWORD)
                //   +$E6: ChipRevBits0 (UBYTE)
                let acti_view = rd32_any(gfx_base + 0x22) as usize;
                println!("  ActiView (GfxBase+$22) = ${:08X}", acti_view);
                println!("  DisplayFlags (GfxBase+$CE) = ${:04X}", rd16_any(gfx_base + 0xCE));
                println!("  MaxDisplayRow (GfxBase+$D4) = {}", rd16_any(gfx_base + 0xD4));
                println!("  MaxDisplayColumn (GfxBase+$D6) = {}", rd16_any(gfx_base + 0xD6));
                println!("  NormalDisplayRows (GfxBase+$D8) = {}", rd16_any(gfx_base + 0xD8));
                println!("  NormalDisplayColumns (GfxBase+$DA) = {}", rd16_any(gfx_base + 0xDA));
                println!("  MicrosPerLine (GfxBase+$E2) = {}", rd16_any(gfx_base + 0xE2));
                println!("  ChipRevBits0 (GfxBase+$E6) = ${:02X}", read_mem(gfx_base + 0xE6));
                // system_bplcon0 at +$A4
                println!("  system_bplcon0 (GfxBase+$A4) = ${:04X}", rd16_any(gfx_base + 0xA4));
                // Dump fields around timing area
                println!("  GfxBase fields $20-$40:");
                for off in (0x20..0x40).step_by(2) {
                    println!("    +${:02X}: ${:04X}", off, rd16_any(gfx_base + off));
                }
                println!("  GfxBase fields $CC-$EC:");
                for off in (0xCC..0xEC).step_by(2) {
                    println!("    +${:02X}: ${:04X}", off, rd16_any(gfx_base + off));
                }
                // LOFlist at ActiView+$0E
                if acti_view > 0 && acti_view < 0x01000000 {
                    let lof_list = rd32_any(acti_view + 0x0E) as usize;
                    println!("  ActiView->LOFlist (ActiView+$0E) = ${:08X}", lof_list);
                }
                break;
            }
            node = next;
        }
    }

    // Dump copper list area for debugging
    for &(label, base) in &[("COP1LC", amiga.copper.cop1lc as usize), ("$C00", 0xC00usize)] {
        println!("--- Copper list at {} (${:06X}) ---", label, base);
        for i in (0..64).step_by(4) {
            let a = base + i;
            if a + 3 < amiga.memory.chip_ram.len() {
                let w0 = u16::from(amiga.memory.chip_ram[a]) << 8 | u16::from(amiga.memory.chip_ram[a + 1]);
                let w1 = u16::from(amiga.memory.chip_ram[a + 2]) << 8 | u16::from(amiga.memory.chip_ram[a + 3]);
                println!("  ${:06X}: ${:04X} ${:04X}", a, w0, w1);
            }
        }
    }

    (amiga.agnus.dmacon, amiga.denise.bplcon0, vp_hash)
}

/// Run a boot test with register/display assertions.
#[allow(dead_code)]
pub fn boot_screenshot_test_expect(
    config: AmigaConfig,
    rom_description: &str,
    screenshot_prefix: &str,
    total_ticks: u64,
    expect: BootExpect,
) {
    let (dmacon, bplcon0, vp_hash) =
        boot_screenshot_test(config, rom_description, screenshot_prefix, total_ticks);

    if let Some(bits) = expect.dmacon_set {
        assert!(
            dmacon & bits == bits,
            "{rom_description}: DMACON ${dmacon:04X} missing expected bits ${bits:04X}",
        );
    }
    if let Some(expected) = expect.bplcon0 {
        assert_eq!(
            bplcon0, expected,
            "{rom_description}: BPLCON0 ${bplcon0:04X} != expected ${expected:04X}",
        );
    }
    if let Some(expected) = expect.viewport_hash {
        assert_eq!(
            vp_hash, expected,
            "{rom_description}: viewport hash 0x{vp_hash:016X} != expected 0x{expected:016X} \
             — visual regression detected. Run the test with --nocapture and check the \
             _display.png screenshot to see what changed.",
        );
    }
}
