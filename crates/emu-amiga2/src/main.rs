//! Amiga emulator binary.
//!
//! Supports windowed mode (winit + pixels) and headless mode for
//! screenshots and batch testing.

#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use emu_amiga2::{capture, Amiga, AmigaConfig};
use emu_amiga2::config::AmigaModel;
use emu_amiga2::denise;
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const FB_WIDTH: u32 = denise::FB_WIDTH;
const FB_HEIGHT: u32 = denise::FB_HEIGHT;
const SCALE: u32 = 2;
const FRAME_DURATION: Duration = Duration::from_micros(20_000);

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

struct CliArgs {
    kickstart_path: Option<PathBuf>,
    model: AmigaModel,
    headless: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        kickstart_path: None,
        model: AmigaModel::A1000,
        headless: false,
        frames: 200,
        screenshot_path: None,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--kickstart" | "--rom" => {
                i += 1;
                cli.kickstart_path = args.get(i).map(PathBuf::from);
            }
            "--model" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.model = match s.to_lowercase().as_str() {
                        "a1000" => AmigaModel::A1000,
                        "a500" => AmigaModel::A500,
                        "a500+" | "a500plus" => AmigaModel::A500Plus,
                        "a600" => AmigaModel::A600,
                        "a2000" => AmigaModel::A2000,
                        "a1200" => AmigaModel::A1200,
                        other => {
                            eprintln!("Unknown model: {other}");
                            process::exit(1);
                        }
                    };
                }
            }
            "--headless" => cli.headless = true,
            "--frames" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.frames = s.parse().unwrap_or(200);
                }
            }
            "--screenshot" => {
                i += 1;
                cli.screenshot_path = args.get(i).map(PathBuf::from);
            }
            "--help" | "-h" => {
                eprintln!("Usage: emu-amiga2 [OPTIONS]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --kickstart <file>   Kickstart ROM/WCS file (256K)");
                eprintln!("  --model <name>       Model: a1000, a500, a500+, a600, a2000, a1200");
                eprintln!("  --headless           Run without a window");
                eprintln!("  --frames <n>         Number of frames in headless mode [default: 200]");
                eprintln!("  --screenshot <file>  Save a PNG screenshot (headless)");
                process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                process::exit(1);
            }
        }
        i += 1;
    }

    cli
}

// ---------------------------------------------------------------------------
// Headless mode
// ---------------------------------------------------------------------------

fn run_headless(cli: &CliArgs) {
    let mut amiga = make_amiga(cli);

    // Diagnostic: dump initial CPU state
    {
        let regs = amiga.cpu().registers();
        eprintln!("Initial: PC=${:08X} SSP=${:08X} SR=${:04X}", regs.pc, regs.ssp, regs.sr);
        eprintln!("Stopped={} Halted={}", amiga.cpu().is_stopped(), amiga.cpu().is_halted());
    }

    // Watch ActiView struct region around $49A0 (LOFCprList/SHFCprList fields).
    amiga.bus_mut().memory.watch_addr = Some(0x49A0);

    for i in 0..cli.frames {
        amiga.run_frame();
        if i < 5 || (15..=55).contains(&i) || (56..=200).contains(&i) || i % 50 == 0 {
            let regs = amiga.cpu().registers();
            let overlay = amiga.bus().memory.overlay;
            let cpu_ticks = amiga.cpu().total_cycles().0;
            let bus = amiga.bus();
            eprintln!(
                "Frame {i}: PC=${:08X} SR=${:04X} D0=${:08X} D1=${:08X} A0=${:08X} A7=${:08X} ovl={overlay} ticks={cpu_ticks} COP1LC=${:08X} COP2LC=${:08X} DMACON=${:04X} BPLCON0=${:04X} INTENA=${:04X} INTREQ=${:04X}",
                regs.pc, regs.sr, regs.d[0], regs.d[1], regs.a[0], regs.ssp,
                bus.copper.cop1lc, bus.copper.cop2lc, bus.agnus.dmacon, bus.denise.bplcon0,
                bus.paula.intena, bus.paula.intreq,
            );
        }
        // Debug: dump copinit area ($04C0-$04D0) around the frames where COPEN is first enabled
        if (100..=112).contains(&i) {
            let bus = amiga.bus();
            let w0 = bus.memory.read_chip_word(0x04C0);
            let w1 = bus.memory.read_chip_word(0x04C2);
            let w2 = bus.memory.read_chip_word(0x04C4);
            let w3 = bus.memory.read_chip_word(0x04C6);
            let w_end0 = bus.memory.read_chip_word(0x04F8);
            let w_end1 = bus.memory.read_chip_word(0x04FA);
            let cop_idle = bus.copper.is_idle();
            let cop_pc = bus.copper.pc();
            eprintln!(
                "  copinit@F{i}: ${w0:04X} {w1:04X} {w2:04X} {w3:04X} ... ${w_end0:04X} {w_end1:04X}  CopPC=${cop_pc:08X} idle={cop_idle}"
            );
        }
    }

    // Diagnostic dump
    {
        let bus = amiga.bus();
        eprintln!("=== Diagnostic dump ===");
        eprintln!("DMACON=${:04X} INTENA=${:04X} INTREQ=${:04X}",
            bus.agnus.dmacon, bus.paula.intena, bus.paula.intreq);
        eprintln!("BPLCON0=${:04X} num_bpl={} DIWSTRT=${:04X} DIWSTOP=${:04X}",
            bus.denise.bplcon0, bus.agnus.num_bitplanes(), bus.agnus.diwstrt, bus.agnus.diwstop);
        eprintln!("DDFSTRT=${:04X} DDFSTOP=${:04X}",
            bus.agnus.ddfstrt, bus.agnus.ddfstop);
        eprintln!(
            "CIA-B: TA=${:04X} TB=${:04X} CRA=${:02X} CRB=${:02X} ICR=${:02X} MASK=${:02X} TOD=${:06X} ALARM=${:06X} PRA=${:02X} PRB=${:02X}",
            bus.cia_b.timer_a(),
            bus.cia_b.timer_b(),
            bus.cia_b.cra(),
            bus.cia_b.crb(),
            bus.cia_b.icr_status(),
            bus.cia_b.icr_mask(),
            bus.cia_b.tod_counter(),
            bus.cia_b.tod_alarm(),
            bus.cia_b.port_a_output(),
            bus.cia_b.port_b_output(),
        );
        eprintln!("COP1LC=${:08X} COP2LC=${:08X} CopperPC=${:08X}",
            bus.copper.cop1lc, bus.copper.cop2lc, bus.copper.pc());
        eprintln!("COP1LC at last vblank restart=${:08X}", bus.copper.last_restart_addr);
        eprintln!("BPL1PT=${:08X} BPL2PT=${:08X}",
            bus.agnus.bpl_pt[0], bus.agnus.bpl_pt[1]);
        eprintln!("COLOR00=${:04X} COLOR01=${:04X} COLOR02=${:04X} COLOR03=${:04X}",
            bus.denise.palette[0], bus.denise.palette[1],
            bus.denise.palette[2], bus.denise.palette[3]);
        let sig_hi = bus.memory.read_chip_word(0x0000);
        let sig_lo = bus.memory.read_chip_word(0x0002);
        if sig_hi == 0x4845 && sig_lo == 0x4C50 {
            let alert_hi = bus.memory.read_chip_word(0x0100);
            let alert_lo = bus.memory.read_chip_word(0x0102);
            let alert = (u32::from(alert_hi) << 16) | u32::from(alert_lo);
            let ctx0_hi = bus.memory.read_chip_word(0x0104);
            let ctx0_lo = bus.memory.read_chip_word(0x0106);
            let ctx0 = (u32::from(ctx0_hi) << 16) | u32::from(ctx0_lo);
            let ctx1_hi = bus.memory.read_chip_word(0x0108);
            let ctx1_lo = bus.memory.read_chip_word(0x010A);
            let ctx1 = (u32::from(ctx1_hi) << 16) | u32::from(ctx1_lo);
            eprintln!("ALERT signature at $0000: HELP, code=${alert:08X} ctx0=${ctx0:08X} ctx1=${ctx1:08X}");
        }

        // Dump copinit at $2368 (LOF) and $2408 (SHF/COP2LC target)
        for &(label, base) in &[("LOF copinit $2368", 0x2368u32), ("COP2LC target $2408", 0x2408u32)] {
            eprintln!("{label}:");
            for i in 0..40u32 {
                let addr = base.wrapping_add(i * 4);
                let ir1 = bus.memory.read_chip_word(addr);
                let ir2 = bus.memory.read_chip_word(addr + 2);
                let kind = if ir1 & 1 == 0 {
                    format!("MOVE ${:04X}→reg${:04X}", ir2, ir1 & 0x01FE)
                } else if ir1 == 0xFFFF && ir2 == 0xFFFE {
                    "END ($FFFF $FFFE)".to_string()
                } else if ir2 & 1 == 0 {
                    format!("WAIT v={:02X} h={:02X} ve={:02X} he={:02X}", (ir1 >> 8) & 0xFF, (ir1 >> 1) & 0x7F, (ir2 >> 8) & 0x7F, (ir2 >> 1) & 0x7F)
                } else {
                    format!("SKIP v={:02X} h={:02X}", (ir1 >> 8) & 0xFF, (ir1 >> 1) & 0x7F)
                };
                eprintln!("  ${:06X}: {:04X} {:04X}  {}", addr, ir1, ir2, kind);
                if ir1 == 0xFFFF && ir2 == 0xFFFE { break; }
            }
        }

        // Dump key register writes — show ALL writes to critical registers
        eprintln!("=== Register write log ({} entries) ===", bus.reg_log.len());
        for (i, &(offset, value, name, source, pc)) in bus.reg_log.iter().enumerate() {
            // Always show COP1LC, COPJMP1, BPLCON0, DMACON; first 30 for others
            let show = i < 50
                || name == "COP1LCH" || name == "COP1LCL" || name == "COPJMP1"
                || name == "COP2LCH" || name == "COP2LCL" || name == "COPJMP2"
                || name == "BPLCON0" || name == "DMACON";
            if show {
                eprintln!("  #{i}: [{source}] {name} (${offset:04X}) = ${value:04X}  PC=${pc:08X}");
            }
        }

        // Dump Copper instruction trace
        eprintln!("=== Copper instruction trace ({} entries) ===", bus.copper.trace.len());
        for (i, &(addr, ir1, ir2)) in bus.copper.trace.iter().enumerate() {
            let kind = if ir1 & 1 == 0 {
                format!("MOVE reg=${:04X} val=${:04X}", ir1 & 0x01FE, ir2)
            } else if ir1 == 0xFFFF && ir2 == 0xFFFE {
                "END".to_string()
            } else {
                format!("WAIT/SKIP v={:02X} h={:02X} mask={:04X}", (ir1 >> 8) & 0xFF, (ir1 >> 1) & 0x7F, ir2)
            };
            eprintln!("  #{i}: ${:06X}: {:04X} {:04X}  {}", addr & 0x7FFFF, ir1, ir2, kind);
        }

        // Dump chip RAM at $04C0 (where LoadView set COP2LC)
        eprintln!("Chip RAM at $04C0 (COP2LC target from LoadView):");
        for i in 0..32u32 {
            let addr = 0x04C0u32.wrapping_add(i * 4);
            let ir1 = bus.memory.read_chip_word(addr);
            let ir2 = bus.memory.read_chip_word(addr + 2);
            let kind = if ir1 & 1 == 0 {
                format!("MOVE ${:04X}→reg${:04X}", ir2, ir1 & 0x01FE)
            } else if ir1 == 0xFFFF && ir2 == 0xFFFE {
                "END".to_string()
            } else {
                format!("WAIT v={:02X} h={:02X}", (ir1 >> 8) & 0xFF, (ir1 >> 1) & 0x7F)
            };
            eprintln!("  ${:06X}: {:04X} {:04X}  {}", addr, ir1, ir2, kind);
            if ir1 == 0xFFFF && ir2 == 0xFFFE { break; }
        }

        // Dump chip RAM around $0C9C (where Copper found BPLCON0 write)
        eprintln!("Chip RAM at $0C90 (near Copper corruption source):");
        for i in 0..16u32 {
            let addr = 0x0C90u32.wrapping_add(i * 4);
            let w0 = bus.memory.read_chip_word(addr);
            let w1 = bus.memory.read_chip_word(addr + 2);
            eprintln!("  ${:06X}: {:04X} {:04X}", addr, w0, w1);
        }

        // Dump COP2LC effective address ($000276) — the "real" display Copper list
        let cop2_eff = bus.copper.cop2lc & bus.memory.chip_ram_word_mask();
        eprintln!("COP2LC effective ${:06X} Copper list:", cop2_eff);
        for i in 0..32u32 {
            let addr = cop2_eff.wrapping_add(i * 4);
            let ir1 = bus.memory.read_chip_word(addr);
            let ir2 = bus.memory.read_chip_word(addr + 2);
            let kind = if ir1 & 1 == 0 {
                format!("MOVE ${:04X}→reg${:04X}", ir2, ir1 & 0x01FE)
            } else if ir1 == 0xFFFF && ir2 == 0xFFFE {
                "END".to_string()
            } else {
                format!("WAIT v={:02X} h={:02X}", (ir1 >> 8) & 0xFF, (ir1 >> 1) & 0x7F)
            };
            eprintln!("  ${:06X}: {:04X} {:04X}  {}", addr & bus.memory.chip_ram_word_mask(), ir1, ir2, kind);
            if ir1 == 0xFFFF && ir2 == 0xFFFE { break; }
        }

        // Dump ExecBase and task lists
        let exec_base_addr = {
            let hi = u32::from(bus.memory.read_chip_word(0x0004));
            let lo = u32::from(bus.memory.read_chip_word(0x0006));
            hi << 16 | lo
        };
        eprintln!("ExecBase at ${exec_base_addr:08X}");
        // Helper to read a word from any RAM address
        let read_word = |addr: u32| -> u16 {
            let hi = bus.memory.read(addr);
            let lo = bus.memory.read(addr + 1);
            u16::from(hi) << 8 | u16::from(lo)
        };
        let read_long = |addr: u32| -> u32 {
            let hi = u32::from(read_word(addr));
            let lo = u32::from(read_word(addr + 2));
            hi << 16 | lo
        };
        if exec_base_addr != 0 && exec_base_addr < 0xF80000 {
            let ready_head = read_long(exec_base_addr + 0x196);
            eprintln!("  TaskReady head: ${ready_head:08X}");
            let wait_head = read_long(exec_base_addr + 0x1A4);
            eprintln!("  TaskWait head: ${wait_head:08X}");
            let res_modules = read_long(exec_base_addr + 0x12E);
            eprintln!("  ResModules: ${res_modules:08X}");
            let idle_count = read_long(exec_base_addr + 0x118);
            eprintln!("  IdleCount: {idle_count}");
            let this_task = read_long(exec_base_addr + 0x114);
            eprintln!("  ThisTask: ${this_task:08X}");
            // Walk TaskWait list (up to 10 tasks)
            let mut node = wait_head;
            for i in 0..10 {
                if node == 0 { break; }
                let next = read_long(node);
                let name_ptr = read_long(node + 10);
                // Read up to 20 chars of name
                let mut name = String::new();
                for j in 0..20u32 {
                    let ch = bus.memory.read(name_ptr + j);
                    if ch == 0 { break; }
                    name.push(ch as char);
                }
                let state = bus.memory.read(node + 15);
                eprintln!("  Wait[{i}]: ${node:08X} state={state} name=\"{name}\"");
                node = next;
            }

            // Find GfxBase by scanning ExecBase's library list
            // ExecBase->LibList at offset $17A
            let lib_list = read_long(exec_base_addr + 0x17A);
            let mut lib_node = lib_list;
            for _ in 0..20 {
                if lib_node == 0 { break; }
                let next = read_long(lib_node);
                let name_ptr = read_long(lib_node + 10);
                let mut name = String::new();
                for j in 0..20u32 {
                    let ch = bus.memory.read(name_ptr + j);
                    if ch == 0 { break; }
                    name.push(ch as char);
                }
                if name.contains("graphics") {
                    eprintln!("  GfxBase at ${lib_node:08X}");
                    // GfxBase fields:
                    // +$26: copinit (LOFlist pointer)
                    // +$2A: SHFlist pointer
                    // +$22: ActiView
                    let copinit = read_long(lib_node + 0x26);
                    let shflist = read_long(lib_node + 0x2A);
                    let actiview = read_long(lib_node + 0x22);
                    eprintln!("    copinit(LOFlist)=${copinit:08X} SHFlist=${shflist:08X} ActiView=${actiview:08X}");
                    // Dump first 16 bytes of copinit
                    if copinit != 0 && copinit < 0x80000 {
                        let w0 = read_word(copinit);
                        let w1 = read_word(copinit + 2);
                        let w2 = read_word(copinit + 4);
                        let w3 = read_word(copinit + 6);
                        eprintln!("    copinit data: ${w0:04X} {w1:04X} {w2:04X} {w3:04X}");
                    }
                }
                eprintln!("  Lib: ${lib_node:08X} name=\"{name}\"");
                lib_node = next;
            }

            // Dump ExecBase MemList to check memory allocator state
            // ExecBase->MemList at offset $142
            let mem_list_head = read_long(exec_base_addr + 0x142);
            eprintln!("MemList head: ${mem_list_head:08X}");
            let mut mh_node = mem_list_head;
            for i in 0..5 {
                if mh_node == 0 { break; }
                let mh_next = read_long(mh_node);
                let mh_attrs = read_word(mh_node + 0x0E);
                let mh_first = read_long(mh_node + 0x10);
                let mh_lower = read_long(mh_node + 0x14);
                let mh_upper = read_long(mh_node + 0x18);
                let mh_free = read_long(mh_node + 0x1C);
                let name_ptr = read_long(mh_node + 0x0A);
                let mut name = String::new();
                if name_ptr != 0 && name_ptr < 0xFFFFFF {
                    for j in 0..30u32 {
                        let ch = bus.memory.read(name_ptr + j);
                        if ch == 0 || ch > 127 { break; }
                        name.push(ch as char);
                    }
                }
                eprintln!("  MemHeader[{i}] @ ${mh_node:08X}: lower=${mh_lower:08X} upper=${mh_upper:08X} free={mh_free} attrs=${mh_attrs:04X} name=\"{name}\"");
                // Walk first few MemChunks
                let mut mc = mh_first;
                for j in 0..5 {
                    if mc == 0 { break; }
                    let mc_next = read_long(mc);
                    let mc_bytes = read_long(mc + 4);
                    eprintln!("    Chunk[{j}] @ ${mc:08X}: {mc_bytes} bytes, next=${mc_next:08X}");
                    mc = mc_next;
                }
                mh_node = mh_next;
            }

            // Dump interrupt vectors (autovectors 1-7 at $64-$7C)
            eprintln!("Interrupt vectors (from chip RAM):");
            for level in 1..=7u32 {
                let vec_addr = (24 + level) * 4;
                let handler = read_long(vec_addr);
                eprintln!("  Level {level} (vector {} at ${:04X}): handler=${handler:08X}",
                    24 + level, vec_addr);
            }
        }

        // Dump boot Copper list at $0420-$0480
        eprintln!("Boot Copper list at $0420:");
        for i in 0..24u32 {
            let addr = 0x0420u32.wrapping_add(i * 4);
            let w0 = bus.memory.read_chip_word(addr);
            let w1 = bus.memory.read_chip_word(addr + 2);
            let kind = if w0 & 1 == 0 {
                format!("MOVE reg=${:04X} val=${:04X}", w0 & 0x01FE, w1)
            } else if w0 == 0xFFFF && w1 == 0xFFFE {
                "END".to_string()
            } else {
                format!("WAIT/SKIP v={:02X} h={:02X}", (w0 >> 8) & 0xFF, (w0 >> 1) & 0x7F)
            };
            eprintln!("  ${:06X}: {:04X} {:04X}  {}", addr, w0, w1, kind);
        }

        // Scan chip RAM for first region with Copper-like data
        let chip_size = bus.memory.chip_ram.len();
        eprintln!("Scanning chip RAM ({} bytes) for Copper list candidates...", chip_size);
        let mut found = 0;
        let mut addr = 0u32;
        while (addr as usize) < chip_size && found < 5 {
            let w0 = bus.memory.read_chip_word(addr);
            let w1 = bus.memory.read_chip_word(addr + 2);
            // Look for MOVE to COLOR00 ($0180)
            if w0 == 0x0180 && w1 != 0 {
                eprintln!("  Found COLOR00 MOVE at ${:06X}: {:04X} {:04X}", addr, w0, w1);
                // Dump context
                for j in 0..8u32 {
                    let a = addr.wrapping_add(j * 4);
                    let i1 = bus.memory.read_chip_word(a);
                    let i2 = bus.memory.read_chip_word(a + 2);
                    eprintln!("    ${:06X}: {:04X} {:04X}", a, i1, i2);
                }
                found += 1;
            }
            addr += 2;
        }
    }

    if let Some(ref path) = cli.screenshot_path {
        if let Err(e) = capture::save_screenshot(&amiga, path) {
            eprintln!("Screenshot error: {e}");
            process::exit(1);
        }
        eprintln!("Screenshot saved to {}", path.display());
    }
}

// ---------------------------------------------------------------------------
// Windowed mode
// ---------------------------------------------------------------------------

struct App {
    amiga: Amiga,
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    last_frame_time: Instant,
}

impl App {
    fn new(amiga: Amiga) -> Self {
        Self {
            amiga,
            window: None,
            pixels: None,
            last_frame_time: Instant::now(),
        }
    }

    fn update_pixels(&mut self) {
        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };

        let fb = self.amiga.framebuffer();
        let frame = pixels.frame_mut();

        for (i, &argb) in fb.iter().enumerate() {
            let offset = i * 4;
            frame[offset] = ((argb >> 16) & 0xFF) as u8;
            frame[offset + 1] = ((argb >> 8) & 0xFF) as u8;
            frame[offset + 2] = (argb & 0xFF) as u8;
            frame[offset + 3] = 0xFF;
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_size = winit::dpi::LogicalSize::new(FB_WIDTH * SCALE, FB_HEIGHT * SCALE);
        let model_name = format!("Amiga ({:?})", self.amiga.model());
        let attrs = WindowAttributes::default()
            .with_title(model_name)
            .with_inner_size(window_size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window: &'static Window = Box::leak(Box::new(window));
                let inner = window.inner_size();
                let surface = SurfaceTexture::new(inner.width, inner.height, window);
                match Pixels::new(FB_WIDTH, FB_HEIGHT, surface) {
                    Ok(pixels) => self.pixels = Some(pixels),
                    Err(e) => {
                        eprintln!("Failed to create pixels: {e}");
                        event_loop.exit();
                        return;
                    }
                }
                self.window = Some(window);
            }
            Err(e) => {
                eprintln!("Failed to create window: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    if keycode == KeyCode::Escape && event.state == ElementState::Pressed {
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= FRAME_DURATION {
                    self.amiga.run_frame();
                    self.update_pixels();
                    self.last_frame_time = now;
                }

                if let Some(pixels) = self.pixels.as_ref()
                    && let Err(e) = pixels.render()
                {
                    eprintln!("Render error: {e}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window {
            window.request_redraw();
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn make_amiga(cli: &CliArgs) -> Amiga {
    let kickstart_path = cli.kickstart_path.as_ref().unwrap_or_else(|| {
        eprintln!("No Kickstart ROM specified. Use --kickstart <file>");
        process::exit(1);
    });

    let kickstart_data = match std::fs::read(kickstart_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "Failed to read Kickstart ROM {}: {e}",
                kickstart_path.display()
            );
            process::exit(1);
        }
    };

    let config = AmigaConfig::preset(cli.model, kickstart_data);
    match Amiga::new(&config) {
        Ok(amiga) => {
            eprintln!(
                "Loaded Kickstart: {} (model: {:?})",
                kickstart_path.display(),
                cli.model,
            );
            amiga
        }
        Err(e) => {
            eprintln!("Failed to initialize Amiga: {e}");
            process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let cli = parse_args();

    if cli.headless {
        run_headless(&cli);
        return;
    }

    let amiga = make_amiga(&cli);
    let mut app = App::new(amiga);

    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("Failed to create event loop: {e}");
            process::exit(1);
        }
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Event loop error: {e}");
        process::exit(1);
    }
}
