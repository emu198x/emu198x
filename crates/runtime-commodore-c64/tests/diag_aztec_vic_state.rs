//! Diagnostic: dump VIC-II register state at the Aztec Challenge
//! player-select waypoint on the current C64 emulator.
//!
//! The catalogue captures Aztec's player-select as scrambled blocks
//! while the archived emulator (Emu198x-Oldest) renders the canyon
//! scene correctly. The render functions are bit-identical between
//! current and archive, so the bug is upstream — VIC mode bits,
//! memory bank, or screen/bitmap base.
//!
//! This dumps the full VIC register set + CIA-2 PA at the same
//! waypoint the catalogue captures, so we can A/B compare against
//! the archive.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{
    HeadlessSession, MediaImage, MediaKind, MediaSet, SessionQueryProvider, read_media_asset,
};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_DISK_AUTOLOAD_SLOT,
    DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Model,
    autoload_basic_disk,
};

use common::{local_aztec_challenge_d64_zip, local_rom_firmware_with_drive, press_key};

#[test]
#[ignore = "requires local C64 ROMs + 1541 ROM + Aztec D64 — run with --ignored --nocapture"]
fn dump_vic_state_at_aztec_player_select() {
    let firmware = local_rom_firmware_with_drive();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    let disk = read_media_asset(&local_aztec_challenge_d64_zip(), MediaKind::Disk)
        .expect("local Aztec Challenge D64 archive should load");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        DEFAULT_DISK_AUTOLOAD_SLOT,
        MediaKind::Disk,
        &disk.bytes,
    ));
    session.load_media(&media).expect("D64 should mount");

    autoload_basic_disk(
        &mut session,
        DEFAULT_DISK_AUTOLOAD_SLOT,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
    )
    .expect("autoload should reach SEARCHING FOR");

    use emu198x_shell::SessionQueryProvider;
    let provider = C64SessionQueryProvider;

    session
        .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
        .expect("should reach LOADING");
    session.run_frames(4_000).expect("should return to BASIC");

    for key in ["r", "u", "n", "return"] {
        press_key(&mut session, key, 3);
    }
    session
        .run_frames(5_000)
        .expect("should reach player-select");

    let machine = session.machine().machine();
    println!("\n=== VIC-II registers at Aztec player-select waypoint ===");
    for reg in 0u8..0x2F {
        let val = machine.vic_register(reg);
        let name = match reg {
            0x11 => " (CR1: ECM/BMM/DEN/RSEL/YSCROLL)",
            0x16 => " (CR2: RES/MCM/CSEL/XSCROLL)",
            0x18 => " (mem ptrs: VM/CB)",
            0x19 => " (IRQ status)",
            0x1A => " (IRQ mask)",
            0x20 => " (border colour)",
            0x21 => " (BG colour 0)",
            _ => "",
        };
        println!("  D0{reg:02X} = ${val:02X} ({val:08b}){name}");
    }

    let cr1 = machine.vic_register(0x11);
    let cr2 = machine.vic_register(0x16);
    let bmm = (cr1 >> 5) & 1;
    let ecm = (cr1 >> 6) & 1;
    let mcm = (cr2 >> 4) & 1;
    let den = (cr1 >> 4) & 1;
    println!("\n=== Decoded mode ===");
    println!("  ECM={ecm} BMM={bmm} MCM={mcm} DEN={den}");
    let mode = match (ecm, bmm, mcm) {
        (0, 0, 0) => "standard text",
        (0, 0, 1) => "multicolour text",
        (0, 1, 0) => "hires bitmap",
        (0, 1, 1) => "multicolour bitmap",
        (1, 0, 0) => "extended-colour text",
        _ => "invalid (blanked)",
    };
    println!("  → {mode}");

    let cia2_pa = provider
        .query(session.machine(), "c64.cia2.pa")
        .ok()
        .flatten()
        .map(|r| r.value);
    println!("\n=== Memory bank ===");
    println!("  CIA2.PA = {cia2_pa:?}");
    let bank_bits = !u8::try_from(
        cia2_pa
            .as_ref()
            .and_then(|v| v.as_u64())
            .unwrap_or(0xFF),
    )
    .unwrap_or(0)
        & 0x03;
    println!("  VIC bank bits (inverted CIA2.PA[0:1]) = {bank_bits}");
    println!(
        "  VIC bank base                              = ${:04X}",
        u32::from(bank_bits) * 0x4000
    );

    let d018 = machine.vic_register(0x18);
    let screen_offset = u16::from((d018 >> 4) & 0x0F) * 0x0400;
    let char_offset = u16::from((d018 >> 1) & 0x07) * 0x0800;
    let bitmap_offset = if d018 & 0x08 != 0 { 0x2000 } else { 0x0000 };
    println!("  Screen RAM offset (within bank)            = ${screen_offset:04X}");
    println!("  Char ROM offset (within bank)              = ${char_offset:04X}");
    println!("  Bitmap base (within bank)                  = ${bitmap_offset:04X}");

    // Dump first row of screen RAM + colour RAM. With MCM text, the
    // canyon glyphs should produce screen codes that index into a
    // custom CHARGEN at $3800. The colour RAM hi-bit (bit 3) selects
    // multicolor mode per cell.
    let cpu = session.machine().machine().cpu();
    println!("\n=== CPU state ===");
    println!(
        "  PC=${:04X}  A=${:02X}  X=${:02X}  Y=${:02X}  SP=${:02X}  P=${:02X}",
        cpu.regs.pc, cpu.regs.a, cpu.regs.x, cpu.regs.y, cpu.regs.sp, cpu.regs.p
    );

    let memory = session.machine().machine().memory();

    // Scan candidate screen base offsets within the current VIC bank.
    // D018 high nybble selects offset within bank in 1KB units. For
    // each candidate, report whether the bytes look like text/screen
    // codes (printable chars) or like 6502 code (lots of zero, A9, AD,
    // 8D, etc).
    let bank_base = u32::from(bank_bits) * 0x4000;
    println!("\n=== Candidate screen bases (within current VIC bank ${bank_base:04X}) ===");
    for vm in 0u32..16 {
        let abs = bank_base + vm * 0x0400;
        let mut printable = 0u32;
        let mut zero = 0u32;
        for off in 0..1000u32 {
            let b = memory.ram_read((abs + off) as u16);
            if b == 0 {
                zero += 1;
            }
            if b < 0x40 {
                printable += 1;
            }
        }
        println!(
            "  VM={vm:>2X}  base=${abs:04X}  zero={zero:>4}  screen-code-range={printable:>4}/1000"
        );
    }

    let screen_abs = bank_base + u32::from(screen_offset);
    println!("\n=== First 80 screen codes (rows 0-1, abs ${screen_abs:04X}) ===");
    for row in 0..3 {
        let row_addr = (screen_abs + row * 40) as u16;
        print!("  row {row}:");
        for col in 0..40 {
            let byte = memory.ram_read(row_addr.wrapping_add(col));
            print!(" {byte:02X}");
            if col == 19 {
                print!("\n        ");
            }
        }
        println!();
    }

    println!("\n=== First 80 colour RAM nibbles (mcm bit = bit 3) ===");
    let colour_ram = memory.colour_ram();
    for row in 0..3 {
        print!("  row {row}:");
        let mut mcm_count = 0;
        for col in 0..40usize {
            let nyb = colour_ram.get(row * 40 + col).copied().unwrap_or(0) & 0x0F;
            if nyb & 0x08 != 0 {
                mcm_count += 1;
            }
            print!(" {nyb:1X}");
            if col == 19 {
                print!("\n        ");
            }
        }
        println!("    [{mcm_count} cells with MCM bit set]");
    }

    // ── Sample mid-frame register state ──
    // Step the machine one CPU cycle at a time and snapshot D011/D016/
    // D018 every ~25 raster lines. If Aztec swaps VIC mode mid-frame
    // (raster IRQ or NMI driven), we'll see BMM/MCM flip between
    // samples; if every sample shows the same vblank state, no
    // mid-frame swap is happening (or it's not landing).
    let machine = session.machine_mut().machine_mut();
    println!("\n=== Mid-frame VIC register sample (next visible frame) ===");
    println!("  line  D011  D016  D018  D01A  mode-decode");

    // Step until we're at line 0, then sample for one full frame.
    while machine.raster_line() != 0 {
        machine.tick();
    }
    let mut samples: Vec<(u16, u8, u8, u8, u8)> = Vec::new();
    let mut last_logged_bucket: i32 = -1;
    for _ in 0..200_000u64 {
        machine.tick();
        let line = machine.raster_line();
        let bucket = line as i32 / 8;
        if bucket != last_logged_bucket && line < 312 {
            last_logged_bucket = bucket;
            let d011 = machine.vic_register(0x11);
            let d016 = machine.vic_register(0x16);
            let d018 = machine.vic_register(0x18);
            let d01a = machine.vic_register(0x1A);
            samples.push((line, d011, d016, d018, d01a));
        }
        if line == 311 && machine.cycle_in_line() == 62 {
            break;
        }
    }
    let mut prev_d011 = 0xFFu8;
    let mut prev_d018 = 0xFFu8;
    let mut prev_d016 = 0xFFu8;
    for (line, d011, d016, d018, d01a) in &samples {
        let changed = (*d011 != prev_d011) || (*d018 != prev_d018) || (*d016 != prev_d016);
        let bmm = (*d011 >> 5) & 1;
        let ecm = (*d011 >> 6) & 1;
        let mcm = (*d016 >> 4) & 1;
        let mode = match (ecm, bmm, mcm) {
            (0, 0, 0) => "TXT",
            (0, 0, 1) => "MCT",
            (0, 1, 0) => "BMP",
            (0, 1, 1) => "MCB",
            (1, 0, 0) => "ECM",
            _ => "BAD",
        };
        let marker = if changed { " <--" } else { "" };
        println!(
            "  {line:>3}   ${d011:02X}    ${d016:02X}    ${d018:02X}    ${d01a:02X}    [{mode}]{marker}"
        );
        prev_d011 = *d011;
        prev_d018 = *d018;
        prev_d016 = *d016;
    }
}
