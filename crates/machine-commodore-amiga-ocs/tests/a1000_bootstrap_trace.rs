//! Focused diagnostics for the real A1000 bootstrap path.
//!
//! These probes compare two startup cases:
//! - the Kickstart disk is mounted as "already present" (`insert_adf`)
//! - the Kickstart disk is mounted with `/DSKCHANGE` still pending
//!   (`insert_adf_with_change_pending`)
//!
//! Run with:
//!   cargo test --manifest-path crates/machine-commodore-amiga-ocs/Cargo.toml \
//!       --test a1000_bootstrap_trace -- --ignored --nocapture

use commodore_agnus_ocs::{SlotOwner, bits};
use std::path::{Path, PathBuf};

use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS, RamConfig};
use motorola_68000::disasm::disassemble;
use zip::ZipArchive;

fn bootstrap_rom_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/a1000-bootstrap.rom");
    if !path.exists() {
        eprintln!(
            "skipping: A1000 bootstrap ROM missing at {}",
            path.display()
        );
        return None;
    }
    Some(path)
}

fn kickstart_disk_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EMU198X_AMIGA_A1000_KICKSTART_DISK") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate dir should have repo-root parents");
    let sibling_archive = repo_root
        .parent()
        .expect("repo root should have a parent")
        .join(
            "Emu198x-docs-archive-2026-04-19/Reference/amiga/Kickstart-Disks/\
             Kickstart-Disk v1.2 r33.180 (1986)(Commodore)(A1000).zip",
        );
    if sibling_archive.exists() {
        return Some(sibling_archive);
    }

    eprintln!("skipping: A1000 Kickstart disk not found; set EMU198X_AMIGA_A1000_KICKSTART_DISK");
    None
}

fn load_bootstrap_rom() -> Option<Vec<u8>> {
    let path = bootstrap_rom_path()?;
    Some(std::fs::read(&path).expect("read A1000 bootstrap ROM"))
}

fn local_kick12_rom_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick12.rom");
    if !path.exists() {
        eprintln!("skipping: local kick12.rom missing at {}", path.display());
        return None;
    }
    Some(path)
}

fn load_kickstart_adf() -> Option<Adf> {
    let path = kickstart_disk_path()?;
    let bytes = if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        let file = std::fs::File::open(&path).expect("open A1000 Kickstart zip");
        let mut archive = ZipArchive::new(file).expect("read A1000 Kickstart zip");
        let mut entry = archive
            .by_index(0)
            .expect("A1000 Kickstart zip should contain one entry");
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes)
            .expect("extract A1000 Kickstart zip entry");
        bytes
    } else {
        std::fs::read(&path).expect("read A1000 Kickstart disk")
    };
    Some(Adf::from_bytes(bytes).expect("decode A1000 Kickstart disk image"))
}

fn read_word(amiga: &AmigaOcs, addr: u32) -> u16 {
    amiga.read_word(addr)
}

fn read_byte(amiga: &AmigaOcs, addr: u32) -> u8 {
    (read_word(amiga, addr & !1) >> (if addr & 1 == 0 { 8 } else { 0 })) as u8
}

fn hex_bytes(amiga: &AmigaOcs, base: u32, len: u32) -> Vec<u8> {
    (0..len)
        .map(|off| read_byte(amiga, base.wrapping_add(off)))
        .collect()
}

fn display_reg_name(offset: u16) -> Option<&'static str> {
    Some(match offset {
        0x08E => "DIWSTRT",
        0x090 => "DIWSTOP",
        0x092 => "DDFSTRT",
        0x094 => "DDFSTOP",
        0x100 => "BPLCON0",
        0x108 => "BPL1MOD",
        0x10A => "BPL2MOD",
        0x0E0 => "BPL1PTH",
        0x0E2 => "BPL1PTL",
        0x0E4 => "BPL2PTH",
        0x0E6 => "BPL2PTL",
        _ => return None,
    })
}

fn sprite_ptr_reg_name(offset: u16) -> Option<&'static str> {
    Some(match offset {
        0x120 => "SPR0PTH",
        0x122 => "SPR0PTL",
        0x124 => "SPR1PTH",
        0x126 => "SPR1PTL",
        0x128 => "SPR2PTH",
        0x12A => "SPR2PTL",
        0x12C => "SPR3PTH",
        0x12E => "SPR3PTL",
        0x130 => "SPR4PTH",
        0x132 => "SPR4PTL",
        0x134 => "SPR5PTH",
        0x136 => "SPR5PTL",
        0x138 => "SPR6PTH",
        0x13A => "SPR6PTL",
        0x13C => "SPR7PTH",
        0x13E => "SPR7PTL",
        _ => return None,
    })
}

fn format_dmacon(value: u16) -> String {
    let mut parts = Vec::new();
    if value & bits::DMACON_DMAEN != 0 {
        parts.push("DMAEN");
    }
    if value & bits::DMACON_BPLEN != 0 {
        parts.push("BPLEN");
    }
    if value & bits::DMACON_COPEN != 0 {
        parts.push("COPEN");
    }
    if value & bits::DMACON_BLTEN != 0 {
        parts.push("BLTEN");
    }
    if value & bits::DMACON_SPREN != 0 {
        parts.push("SPREN");
    }
    if value & bits::DMACON_DSKEN != 0 {
        parts.push("DSKEN");
    }
    if value & bits::DMACON_AUD0EN != 0 {
        parts.push("AUD0");
    }
    if value & bits::DMACON_AUD1EN != 0 {
        parts.push("AUD1");
    }
    if value & bits::DMACON_AUD2EN != 0 {
        parts.push("AUD2");
    }
    if value & bits::DMACON_AUD3EN != 0 {
        parts.push("AUD3");
    }
    if parts.is_empty() {
        "<none>".into()
    } else {
        parts.join("|")
    }
}

fn blit_dest_range(c1: u16, dpt: u32, size: u16) -> (u32, u32) {
    let height = u32::from((size >> 6) & 0x03FF).max(1);
    let width_words = u32::from(size & 0x003F).max(1);
    let len_bytes = width_words * 2 * height;
    if (c1 & 0x0002) != 0 {
        (dpt.wrapping_sub(len_bytes), dpt)
    } else {
        (dpt, dpt.wrapping_add(len_bytes))
    }
}

fn ranges_overlap(a_base: u32, a_len: u32, b_base: u32, b_len: u32) -> bool {
    let a_end = a_base.wrapping_add(a_len);
    let b_end = b_base.wrapping_add(b_len);
    a_base < b_end && b_base < a_end
}

#[derive(Debug, Clone)]
struct LineTrace {
    frame: u64,
    vpos: u16,
    bplcon0: u16,
    start_bpl1: u32,
    start_bpl2: u32,
    end_bpl1: u32,
    end_bpl2: u32,
    fetches: Vec<String>,
}

impl LineTrace {
    fn bpl1_delta(&self) -> u32 {
        self.end_bpl1.wrapping_sub(self.start_bpl1)
    }

    fn bpl2_delta(&self) -> u32 {
        self.end_bpl2.wrapping_sub(self.start_bpl2)
    }

    fn looks_like_fetch_dma(&self) -> bool {
        self.bplcon0 != 0
            && (self.bpl1_delta() > 0 || self.bpl2_delta() > 0)
            && self.fetches.len() >= 4
    }
}

#[derive(Debug, Clone)]
struct ScenarioSummary {
    label: &'static str,
    wom_locked_frame: Option<u64>,
    _first_dsk_write_frame: Option<u64>,
    _dsk_write_count: usize,
    _step_events: u32,
    _cylinder: u32,
    _motor_on: bool,
    _motor_spinning: bool,
    _disk_change: bool,
    _boot_rom_visible: bool,
    _wom_locked: bool,
    _final_pc: u32,
}

fn run_case(label: &'static str, pending_change: bool) -> Option<ScenarioSummary> {
    let rom = load_bootstrap_rom()?;
    let adf = load_kickstart_adf()?;
    let mut amiga = AmigaOcs::with_a1000_bootstrap_rom(
        rom,
        RamConfig {
            chip_kb: 256,
            slow_kb: 0,
            fast_kb: 0,
        },
    );
    if pending_change {
        amiga.insert_adf_with_change_pending(adf);
    } else {
        amiga.insert_adf(adf);
    }

    let mut wom_locked_frame = None;
    let mut first_dsk_write_frame = None;
    let mut prev_dsk_len = 0usize;

    for frame in 0..900u64 {
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
        }
        let frame_num = frame + 1;

        if first_dsk_write_frame.is_none() && amiga.debug_dsk_log.len() > prev_dsk_len {
            first_dsk_write_frame = Some(frame_num);
        }
        prev_dsk_len = amiga.debug_dsk_log.len();

        if wom_locked_frame.is_none() && amiga.memory().a1000_wom_locked() {
            wom_locked_frame = Some(frame_num);
            break;
        }
    }

    Some(ScenarioSummary {
        label,
        wom_locked_frame,
        _first_dsk_write_frame: first_dsk_write_frame,
        _dsk_write_count: amiga.debug_dsk_log.len(),
        _step_events: amiga.drive().step_event_counter(),
        _cylinder: amiga.drive().cylinder(),
        _motor_on: amiga.drive().motor_on(),
        _motor_spinning: amiga.drive().motor_spinning(),
        _disk_change: amiga.drive().status().disk_change,
        _boot_rom_visible: amiga.memory().a1000_boot_rom_visible(),
        _wom_locked: amiga.memory().a1000_wom_locked(),
        _final_pc: amiga.cpu().regs.pc,
    })
}

#[test]
#[ignore = "needs local A1000 bootstrap ROM and Kickstart disk"]
fn compare_a1000_bootstrap_disk_insert_semantics() {
    let Some(acknowledged) = run_case("acknowledged", false) else {
        return;
    };
    let Some(pending) = run_case("pending-change", true) else {
        return;
    };

    for summary in [&acknowledged, &pending] {
        eprintln!("\n=== {} ===", summary.label);
        eprintln!("{summary:#?}");
    }

    if acknowledged.wom_locked_frame.is_none() && pending.wom_locked_frame.is_some() {
        eprintln!("\nA1000 bootstrap only progresses when /DSKCHANGE is left pending at startup.");
    }
}

#[test]
#[ignore = "needs local A1000 bootstrap ROM and Kickstart disk"]
fn trace_a1000_pending_change_read_path() {
    let Some(rom) = load_bootstrap_rom() else {
        return;
    };
    let Some(adf) = load_kickstart_adf() else {
        return;
    };

    let mut amiga = AmigaOcs::with_a1000_bootstrap_rom(
        rom,
        RamConfig {
            chip_kb: 256,
            slow_kb: 0,
            fast_kb: 0,
        },
    );
    amiga.insert_adf_with_change_pending(adf);

    let watch_points = [
        (0x00F8_1890u32, "kickstart-read entry"),
        (0x00F8_0DF4u32, "raw track read"),
        (0x00F8_0FA6u32, "track validate"),
        (0x00F8_0B38u32, "wait-for-next-disk-change"),
    ];
    let mut hits = [0u64; 4];

    for _ in 0..(900 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        for (idx, (addr, _label)) in watch_points.iter().enumerate() {
            if pc == *addr {
                hits[idx] += 1;
            }
        }
    }

    eprintln!("final_pc=${:08X}", amiga.cpu().regs.pc);
    eprintln!(
        "drive: change_pending={} step_events={} cyl={} motor_on={} motor_spinning={}",
        amiga.drive().status().disk_change,
        amiga.drive().step_event_counter(),
        amiga.drive().cylinder(),
        amiga.drive().motor_on(),
        amiga.drive().motor_spinning()
    );
    eprintln!(
        "a1000: boot_rom_visible={} wom_locked={}",
        amiga.memory().a1000_boot_rom_visible(),
        amiga.memory().a1000_wom_locked()
    );
    eprintln!("dsk_writes={}", amiga.debug_dsk_log.len());
    for ((addr, label), count) in watch_points.iter().zip(hits) {
        eprintln!("  {label} @ ${addr:08X}: {count}");
    }
    for (idx, (cck, pc, reg, val)) in amiga.debug_dsk_log.iter().take(24).enumerate() {
        eprintln!("  dsk[{idx}] cck={cck} pc=${pc:08X} reg=${reg:03X} val=${val:04X}");
    }
}

#[test]
#[ignore = "needs local A1000 bootstrap ROM, Kickstart disk, and kick12.rom"]
fn compare_wom_loaded_kickstart_to_local_kick12_rom() {
    let Some(rom) = load_bootstrap_rom() else {
        return;
    };
    let Some(adf) = load_kickstart_adf() else {
        return;
    };
    let Some(kick12_path) = local_kick12_rom_path() else {
        return;
    };
    let kick12 = std::fs::read(&kick12_path).expect("read local kick12.rom");

    let mut amiga = AmigaOcs::with_a1000_bootstrap_rom(
        rom,
        RamConfig {
            chip_kb: 256,
            slow_kb: 0,
            fast_kb: 0,
        },
    );
    amiga.insert_adf_with_change_pending(adf);

    for _ in 0..(1800 * PAL_FRAME_TICKS) {
        amiga.tick();
        if amiga.memory().a1000_wom_locked() {
            break;
        }
    }

    assert!(
        amiga.memory().a1000_wom_locked(),
        "A1000 WOM never locked; final pc=${:08X}",
        amiga.cpu().regs.pc
    );

    let mut wom = vec![0u8; kick12.len()];
    for (idx, byte) in wom.iter_mut().enumerate() {
        *byte = amiga
            .memory()
            .read_byte(0x00F8_0000u32.wrapping_add(idx as u32));
    }

    let first_diff = wom
        .iter()
        .zip(&kick12)
        .enumerate()
        .find(|(_, (a, b))| a != b)
        .map(|(idx, (a, b))| (idx, *a, *b));

    println!(
        "wom_locked={} final_pc=${:08X} first_diff={first_diff:?}",
        amiga.memory().a1000_wom_locked(),
        amiga.cpu().regs.pc,
    );

    assert_eq!(
        wom, kick12,
        "WOM-loaded Kickstart does not match local kick12.rom"
    );
}

#[test]
#[ignore = "needs local A1000 bootstrap ROM and Kickstart disk"]
fn trace_a1000_boot_rom_disable_write() {
    let Some(rom) = load_bootstrap_rom() else {
        return;
    };
    let Some(adf) = load_kickstart_adf() else {
        return;
    };

    let mut amiga = AmigaOcs::with_a1000_bootstrap_rom(
        rom,
        RamConfig {
            chip_kb: 256,
            slow_kb: 0,
            fast_kb: 0,
        },
    );
    amiga.insert_adf_with_change_pending(adf);
    amiga.debug_watch_addr = Some((0x00F8_0000, 0x20));
    amiga.debug_watch_writes.clear();

    let cck_per_frame = PAL_FRAME_TICKS / 2;

    for _ in 0..(1800 * PAL_FRAME_TICKS) {
        amiga.tick();
        if amiga.memory().a1000_wom_locked() {
            break;
        }
    }

    eprintln!(
        "locked={} boot_rom_visible={} final_pc=${:08X} cyl={} steps={}",
        amiga.memory().a1000_wom_locked(),
        amiga.memory().a1000_boot_rom_visible(),
        amiga.cpu().regs.pc,
        amiga.drive().cylinder(),
        amiga.drive().step_event_counter()
    );
    eprintln!(
        "head after lock: $F80000={:02X?} $FC0000={:02X?}",
        hex_bytes(&amiga, 0x00F8_0000, 8),
        hex_bytes(&amiga, 0x00FC_0000, 8)
    );
    eprintln!("writes into $F80000-$F8001F:");
    for (idx, (cck, pc, addr, val, is_word)) in amiga.debug_watch_writes.iter().enumerate() {
        let frame = cck / cck_per_frame + 1;
        let width = if *is_word { "word" } else { "byte" };
        eprintln!(
            "  [{idx}] frame={frame} cck={cck} pc=${pc:08X} addr=${addr:08X} {width}=${val:04X}"
        );
    }

    assert!(
        amiga.memory().a1000_wom_locked(),
        "A1000 WOM never locked; final pc=${:08X}",
        amiga.cpu().regs.pc
    );
}

#[test]
#[ignore = "needs local A1000 bootstrap ROM and Kickstart disk"]
fn trace_a1000_wom_fill_progress() {
    let Some(rom) = load_bootstrap_rom() else {
        return;
    };
    let Some(adf) = load_kickstart_adf() else {
        return;
    };

    let checkpoints = [400u64, 700, 1000, 1200];
    let mut checkpoint_idx = 0usize;

    let mut amiga = AmigaOcs::with_a1000_bootstrap_rom(
        rom,
        RamConfig {
            chip_kb: 256,
            slow_kb: 0,
            fast_kb: 0,
        },
    );
    amiga.insert_adf_with_change_pending(adf);

    for frame in 1..=1200u64 {
        for _ in 0..PAL_FRAME_TICKS {
            amiga.tick();
        }

        if checkpoints.get(checkpoint_idx).copied() != Some(frame) {
            continue;
        }
        checkpoint_idx += 1;

        let wom_base = 0x00FC_0000u32;
        let wom_head = hex_bytes(&amiga, wom_base, 32);
        let wom_vec = hex_bytes(&amiga, wom_base.wrapping_add(0x00D0), 16);
        let wom_1k_nonzero = (0..1024u32)
            .filter(|off| read_byte(&amiga, wom_base.wrapping_add(*off)) != 0)
            .count();
        let wom_64k_nonzero = (0..(64 * 1024u32))
            .filter(|off| read_byte(&amiga, wom_base.wrapping_add(*off)) != 0)
            .count();
        let boot_head = hex_bytes(&amiga, 0x00F8_0000, 32);

        eprintln!(
            "\n=== frame {frame} ===\n\
             boot_rom_visible={} wom_locked={} pc=${:08X} cyl={} steps={} bplcon0=${:04X}",
            amiga.memory().a1000_boot_rom_visible(),
            amiga.memory().a1000_wom_locked(),
            amiga.cpu().regs.pc,
            amiga.drive().cylinder(),
            amiga.drive().step_event_counter(),
            amiga.bplcon0(),
        );
        eprintln!(
            "WOM @ $FC0000 first32 nonzero={} data={:02X?}",
            wom_head.iter().filter(|&&b| b != 0).count(),
            wom_head
        );
        eprintln!(
            "WOM @ $FC00D0 first16 nonzero={} data={:02X?}",
            wom_vec.iter().filter(|&&b| b != 0).count(),
            wom_vec
        );
        eprintln!("WOM nonzero: first1K={wom_1k_nonzero} first64K={wom_64k_nonzero}");
        eprintln!(
            "BOOT @ $F80000 first32 nonzero={} data={:02X?}",
            boot_head.iter().filter(|&&b| b != 0).count(),
            boot_head
        );
    }
}

#[test]
#[ignore = "needs local A1000 bootstrap ROM and Kickstart disk"]
fn trace_a1000_kickdisk_white_phase_display_state() {
    let Some(rom) = load_bootstrap_rom() else {
        return;
    };
    let Some(adf) = load_kickstart_adf() else {
        return;
    };

    const FRAME_START: u64 = 620;
    const FRAME_END: u64 = 760;

    let mut amiga = AmigaOcs::with_a1000_bootstrap_rom(
        rom,
        RamConfig {
            chip_kb: 256,
            slow_kb: 0,
            fast_kb: 0,
        },
    );
    amiga.insert_adf_with_change_pending(adf);

    let cck_per_frame = PAL_FRAME_TICKS / 2;
    let mut line_frame = 0u64;
    let mut line_vpos = 0u16;
    let mut line_bplcon0 = 0u16;
    let mut line_start_bpl1 = 0u32;
    let mut line_start_bpl2 = 0u32;
    let mut prev_bpl1 = 0u32;
    let mut prev_bpl2 = 0u32;
    let mut line_fetches = Vec::<String>::new();
    let mut first_nonzero_bplcon_line: Option<LineTrace> = None;
    let mut first_fetch_dma_line: Option<LineTrace> = None;
    let mut last_dmacon_len = 0usize;
    let mut live_dmacon_events = Vec::<(u64, u32, u32, u16, u16, u16, u16)>::new();
    let mut sprite_trace_started = false;
    let mut prev_spr_pt = [0u32; 8];
    let mut sprite_ptr_events = Vec::<String>::new();

    for _ in 0..(FRAME_END * PAL_FRAME_TICKS) {
        amiga.tick();

        while last_dmacon_len < amiga.debug_dmacon_log.len() {
            let (cck, _pc_after, raw_val, before, after) = amiga.debug_dmacon_log[last_dmacon_len];
            live_dmacon_events.push((
                cck,
                amiga.cpu().instr_start_pc,
                amiga.cpu().regs.pc,
                amiga.cpu().ir,
                raw_val,
                before,
                after,
            ));
            last_dmacon_len += 1;
        }

        let cck = amiga.cck_count();
        let frame = cck / cck_per_frame + 1;
        if frame >= FRAME_START {
            if !sprite_trace_started {
                prev_spr_pt.copy_from_slice(&amiga.agnus().spr_pt[..8]);
                sprite_trace_started = true;
            } else {
                for (sprite, prev) in prev_spr_pt.iter_mut().enumerate() {
                    let cur = amiga.agnus().spr_pt[sprite];
                    if cur != *prev && sprite_ptr_events.len() < 48 {
                        sprite_ptr_events.push(format!(
                            "frame={frame} cck={cck} vpos=${:03X} hpos=${:03X} SPR{} ${:08X}->${:08X}",
                            amiga.agnus().vpos,
                            amiga.agnus().hpos,
                            sprite,
                            *prev,
                            cur,
                        ));
                    }
                    *prev = cur;
                }
            }
        }

        if amiga.tick_count() & 1 == 0 {
            continue;
        }

        if frame < FRAME_START {
            continue;
        }

        let vpos = amiga.agnus().vpos;
        let hpos = amiga.agnus().hpos;
        let bplcon0 = amiga.bplcon0();
        let bpl1 = amiga.agnus().bpl_pt[0];
        let bpl2 = amiga.agnus().bpl_pt[1];
        let slot = match amiga.agnus().current_slot() {
            SlotOwner::Bitplane(0) => "BPL1",
            SlotOwner::Bitplane(1) => "BPL2",
            SlotOwner::Bitplane(2) => "BPL3",
            SlotOwner::Bitplane(3) => "BPL4",
            SlotOwner::Bitplane(4) => "BPL5",
            SlotOwner::Bitplane(5) => "BPL6",
            SlotOwner::Bitplane(_) => "BPLX",
            SlotOwner::Copper => "Copper",
            SlotOwner::Cpu => "CPU",
            SlotOwner::Disk => "Disk",
            SlotOwner::Refresh => "Refresh",
            SlotOwner::Audio(_) => "Audio",
            SlotOwner::Sprite(_) => "Sprite",
        };

        if line_frame != frame || line_vpos != vpos {
            let finished = LineTrace {
                frame: line_frame,
                vpos: line_vpos,
                bplcon0: line_bplcon0,
                start_bpl1: line_start_bpl1,
                start_bpl2: line_start_bpl2,
                end_bpl1: prev_bpl1,
                end_bpl2: prev_bpl2,
                fetches: line_fetches.clone(),
            };

            if first_nonzero_bplcon_line.is_none() && finished.bplcon0 != 0 {
                first_nonzero_bplcon_line = Some(finished.clone());
            }
            if first_fetch_dma_line.is_none() && finished.looks_like_fetch_dma() {
                first_fetch_dma_line = Some(finished);
            }

            line_frame = frame;
            line_vpos = vpos;
            line_bplcon0 = bplcon0;
            line_start_bpl1 = bpl1;
            line_start_bpl2 = bpl2;
            prev_bpl1 = bpl1;
            prev_bpl2 = bpl2;
            line_fetches.clear();
        }

        if bpl1 != prev_bpl1 {
            line_fetches.push(format!(
                "frame={frame} vpos=${vpos:03X} hpos=${hpos:03X} slot={slot} \
                 BPL1 ${prev_bpl1:08X}->${bpl1:08X} bplcon0=${bplcon0:04X}"
            ));
            prev_bpl1 = bpl1;
        }
        if bpl2 != prev_bpl2 {
            line_fetches.push(format!(
                "frame={frame} vpos=${vpos:03X} hpos=${hpos:03X} slot={slot} \
                 BPL2 ${prev_bpl2:08X}->${bpl2:08X} bplcon0=${bplcon0:04X}"
            ));
            prev_bpl2 = bpl2;
        }
    }

    let finished = LineTrace {
        frame: line_frame,
        vpos: line_vpos,
        bplcon0: line_bplcon0,
        start_bpl1: line_start_bpl1,
        start_bpl2: line_start_bpl2,
        end_bpl1: prev_bpl1,
        end_bpl2: prev_bpl2,
        fetches: line_fetches,
    };
    if first_nonzero_bplcon_line.is_none() && finished.bplcon0 != 0 {
        first_nonzero_bplcon_line = Some(finished.clone());
    }
    if first_fetch_dma_line.is_none() && finished.looks_like_fetch_dma() {
        first_fetch_dma_line = Some(finished);
    }

    eprintln!("=== A1000 Kickstart-disk white-phase display state ===");
    eprintln!(
        "final: pc=${:08X} boot_rom_visible={} wom_locked={} cyl={} steps={} motor_on={} motor_spinning={} dmacon=${:04X} bplcon0=${:04X} ddfstrt=${:04X} ddfstop=${:04X} diwstrt=${:04X} diwstop=${:04X} bpl1mod={} bpl2mod={}",
        amiga.cpu().regs.pc,
        amiga.memory().a1000_boot_rom_visible(),
        amiga.memory().a1000_wom_locked(),
        amiga.drive().cylinder(),
        amiga.drive().step_event_counter(),
        amiga.drive().motor_on(),
        amiga.drive().motor_spinning(),
        amiga.dmacon(),
        amiga.bplcon0(),
        amiga.agnus().ddfstrt,
        amiga.agnus().ddfstop,
        amiga.agnus().diwstrt,
        amiga.agnus().diwstop,
        amiga.agnus().bpl1mod,
        amiga.agnus().bpl2mod,
    );

    eprintln!("\nlate copper moves:");
    let mut copper_lines = Vec::<String>::new();
    for (cck, vpos, hpos, reg, val) in &amiga.debug_copper_move_log {
        let frame = cck / cck_per_frame + 1;
        if frame < FRAME_START {
            continue;
        }
        if let Some(name) = display_reg_name(*reg) {
            copper_lines.push(format!(
                "  frame={frame} cck={cck} vpos=${vpos:03X} hpos=${hpos:03X} {name}=${val:04X}"
            ));
        }
    }
    for line in copper_lines.iter().take(32) {
        eprintln!("{line}");
    }
    if copper_lines.len() > 40 {
        eprintln!("  ...");
        for line in copper_lines.iter().skip(copper_lines.len() - 8) {
            eprintln!("{line}");
        }
    } else {
        for line in copper_lines.iter().skip(32) {
            eprintln!("{line}");
        }
    }

    eprintln!("\nlate CPU/custom writes:");
    for (cck, pc, _addr24, raw_val, offset, is_byte) in &amiga.debug_custom_write_log {
        let frame = cck / cck_per_frame + 1;
        if frame < FRAME_START {
            continue;
        }
        if let Some(name) = display_reg_name(*offset) {
            let lane = if *is_byte { "byte" } else { "word" };
            eprintln!("  frame={frame} cck={cck} pc=${pc:08X} {name} raw=${raw_val:04X} {lane}");
        }
    }

    eprintln!("\nlate sprite-pointer copper moves:");
    let mut sprite_copper_lines = Vec::<String>::new();
    for (cck, vpos, hpos, reg, val) in &amiga.debug_copper_move_log {
        let frame = cck / cck_per_frame + 1;
        if frame < FRAME_START {
            continue;
        }
        if let Some(name) = sprite_ptr_reg_name(*reg) {
            sprite_copper_lines.push(format!(
                "  frame={frame} cck={cck} vpos=${vpos:03X} hpos=${hpos:03X} {name}=${val:04X}"
            ));
        }
    }
    if sprite_copper_lines.is_empty() {
        eprintln!("  <none>");
    } else {
        for line in sprite_copper_lines.iter().take(32) {
            eprintln!("{line}");
        }
        if sprite_copper_lines.len() > 40 {
            eprintln!("  ...");
            for line in sprite_copper_lines
                .iter()
                .skip(sprite_copper_lines.len() - 8)
            {
                eprintln!("{line}");
            }
        } else {
            for line in sprite_copper_lines.iter().skip(32) {
                eprintln!("{line}");
            }
        }
    }

    eprintln!("\nlate DMACON writes:");
    let mut dmacon_lines = Vec::<String>::new();
    for (cck, pc, raw_val, before, after) in &amiga.debug_dmacon_log {
        let frame = cck / cck_per_frame + 1;
        if frame < 500 {
            continue;
        }
        let op = if raw_val & bits::DMACON_SETCLR != 0 {
            "set"
        } else {
            "clear"
        };
        dmacon_lines.push(format!(
            "  frame={frame} cck={cck} pc=${pc:08X} raw=${raw_val:04X} ({op}) before=${before:04X} [{}] after=${after:04X} [{}]",
            format_dmacon(*before),
            format_dmacon(*after),
        ));
    }
    if dmacon_lines.is_empty() {
        eprintln!("  <none>");
    } else {
        for line in &dmacon_lines {
            eprintln!("{line}");
        }
    }

    eprintln!("\nDMACON writer disassembly:");
    if live_dmacon_events.is_empty() {
        eprintln!("  <none>");
    } else {
        for (cck, instr_start_pc, pc_after, ir, raw_val, before, after) in &live_dmacon_events {
            let frame = cck / cck_per_frame + 1;
            if frame < 500 {
                continue;
            }
            let (mnemonic, len) = disassemble(*instr_start_pc, |addr| read_byte(&amiga, addr));
            eprintln!(
                "  frame={frame} cck={cck} instr_start=${instr_start_pc:08X} pc_after=${pc_after:08X} ir=${ir:04X} raw=${raw_val:04X} before=${before:04X} after=${after:04X}: {mnemonic}"
            );
            let next = instr_start_pc.wrapping_add(len as u32);
            let (next_mnemonic, _) = disassemble(next, |addr| read_byte(&amiga, addr));
            eprintln!("    next: ${next:08X}: {next_mnemonic}");
        }
    }

    eprintln!("\nlate blits overlapping white-phase bitplane buffers:");
    let mut found_overlap = false;
    for (cck, pc, _c0, c1, _apt, _bpt, _cpt, dpt, size) in &amiga.debug_blit_log {
        let frame = cck / cck_per_frame + 1;
        if frame < 500 {
            continue;
        }
        let (dst_lo, dst_hi) = blit_dest_range(*c1, *dpt, *size);
        if !ranges_overlap(
            dst_lo,
            dst_hi.wrapping_sub(dst_lo),
            0x0000_4000,
            0x0000_4000,
        ) {
            continue;
        }
        found_overlap = true;
        eprintln!(
            "  frame={frame} cck={cck} pc=${pc:08X} c1=${c1:04X} dpt=${dpt:08X} size=${size:04X} dst=${dst_lo:08X}..${dst_hi:08X}"
        );
    }
    if !found_overlap {
        eprintln!("  <none>");
    }

    eprintln!("\nlive sprite-pointer movement:");
    if sprite_ptr_events.is_empty() {
        eprintln!("  <none>");
    } else {
        for line in &sprite_ptr_events {
            eprintln!("  {line}");
        }
    }

    eprintln!("\nfinal sprite pointers:");
    for sprite in 0..8usize {
        let ptr = amiga.agnus().spr_pt[sprite];
        let bytes = hex_bytes(&amiga, ptr, 8);
        let nonzero = bytes.iter().filter(|&&b| b != 0).count();
        eprintln!(
            "  SPR{} ptr=${:08X} first8_nonzero={} data={:02X?}",
            sprite, ptr, nonzero, bytes
        );
    }

    eprintln!("\nfirst line with nonzero BPLCON0:");
    if let Some(line) = &first_nonzero_bplcon_line {
        eprintln!(
            "  frame={} vpos=${:03X} bplcon0=${:04X}",
            line.frame, line.vpos, line.bplcon0
        );
        eprintln!(
            "  BPL1 start=${:08X} end=${:08X} delta={} bytes",
            line.start_bpl1,
            line.end_bpl1,
            line.end_bpl1.wrapping_sub(line.start_bpl1)
        );
        eprintln!(
            "  BPL2 start=${:08X} end=${:08X} delta={} bytes",
            line.start_bpl2,
            line.end_bpl2,
            line.end_bpl2.wrapping_sub(line.start_bpl2)
        );
        for event in &line.fetches {
            eprintln!("    {event}");
        }

        let bpl1 = hex_bytes(&amiga, line.start_bpl1, 64);
        let bpl2 = hex_bytes(&amiga, line.start_bpl2, 64);
        eprintln!(
            "  BPL1 first 64 bytes nonzero={} data={:02X?}",
            bpl1.iter().filter(|&&b| b != 0).count(),
            bpl1
        );
        eprintln!(
            "  BPL2 first 64 bytes nonzero={} data={:02X?}",
            bpl2.iter().filter(|&&b| b != 0).count(),
            bpl2
        );
    } else {
        eprintln!("  no line with nonzero BPLCON0 observed in frames {FRAME_START}..{FRAME_END}");
    }

    eprintln!("\nfirst line that looks like real bitplane fetch DMA:");
    if let Some(line) = &first_fetch_dma_line {
        eprintln!(
            "  frame={} vpos=${:03X} bplcon0=${:04X}",
            line.frame, line.vpos, line.bplcon0
        );
        eprintln!(
            "  BPL1 start=${:08X} end=${:08X} delta={} bytes",
            line.start_bpl1,
            line.end_bpl1,
            line.bpl1_delta()
        );
        eprintln!(
            "  BPL2 start=${:08X} end=${:08X} delta={} bytes",
            line.start_bpl2,
            line.end_bpl2,
            line.bpl2_delta()
        );
        for event in &line.fetches {
            eprintln!("    {event}");
        }
    } else {
        eprintln!("  no per-line bitplane fetch DMA observed in frames {FRAME_START}..{FRAME_END}");
    }
}
