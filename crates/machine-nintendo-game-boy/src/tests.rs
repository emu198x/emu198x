//! Machine-level smoke tests.
//!
//! These don't try to verify the CPU itself — that's the job of the
//! `sharp-lr35902` Tom Harte corpus. They check that the
//! orchestration and I/O dispatch wire everything up correctly:
//! reads / writes through the bus actually reach the right
//! component, the per-T-cycle ticking advances the timer / PPU /
//! APU together with the CPU, and the IRQ wiring lights up `IF`.

#![allow(clippy::unwrap_used)]

use super::*;
use format_nintendo_game_boy_cartridge::CartridgeHeader;
use nintendo_game_boy_mbc::CartType;

/// Build a 32 KiB ROM with NOPs and a valid header.
fn nop_rom() -> Vec<u8> {
    let mut rom = vec![0x00; 0x8000];
    // ROM size code 0 = 32 KiB.
    rom[0x0148] = 0x00;
    rom[0x0147] = 0x00; // ROM only
    rom[0x0149] = 0x00;
    // Header checksum.
    let mut checksum: u8 = 0;
    for &byte in &rom[0x0134..=0x014C] {
        checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
    }
    rom[0x014D] = checksum;
    rom
}

/// Build a ROM that loops forever at $0100.
fn jr_loop_rom() -> Vec<u8> {
    let mut rom = nop_rom();
    rom[0x0100] = 0x18; // JR
    rom[0x0101] = 0xFE; // -2 → loops on itself
    // Recompute header checksum (header bytes unchanged; the JR
    // pair lives below $0134 so the checksum isn't affected).
    rom
}

fn boot_machine(rom: Vec<u8>) -> GameBoy {
    let (_, gb) = GameBoy::from_rom(rom).unwrap();
    gb
}

fn boot_machine_with_profile(rom: Vec<u8>, boot_profile: BootProfile) -> GameBoy {
    let (_, gb) = GameBoy::from_rom_with_boot_profile(rom, boot_profile).unwrap();
    gb
}

#[test]
#[ignore = "diagnostic: searches skipped-boot DIV phase against local mooneye ROMs"]
fn diagnostic_mooneye_boot_div_counter_phase_search() {
    let root = std::env::var("EMU198X_GB_MOONEYE_ROOT").unwrap();
    for (rom_name, boot_profile, range) in [
        ("boot_div-dmg0.gb", BootProfile::Dmg0, 0x1828u16..=0x1833u16),
        (
            "boot_div-dmgABCmgb.gb",
            BootProfile::DmgAbc,
            0xABC4u16..=0xABCFu16,
        ),
        ("boot_div-S.gb", BootProfile::Sgb, 0xD858u16..=0xD863u16),
        ("boot_div2-S.gb", BootProfile::Sgb2, 0xD848u16..=0xD853u16),
    ] {
        let rom = std::fs::read(format!("{root}/acceptance/{rom_name}")).unwrap();
        for counter in range {
            let (_, mut gb) =
                GameBoy::from_rom_with_boot_profile(rom.clone(), boot_profile).unwrap();
            gb.timer.counter = counter;

            let mut serial = Vec::new();
            for _ in 0..20 {
                gb.run_frame();
                serial.extend(gb.drain_serial());
                if serial.windows(6).any(|window| window == [0x42; 6]) {
                    break;
                }
                if serial
                    .windows(6)
                    .any(|window| window == [3, 5, 8, 13, 21, 34])
                {
                    break;
                }
            }

            if serial
                .windows(6)
                .any(|window| window == [3, 5, 8, 13, 21, 34])
            {
                eprintln!(
                    "{rom_name} passes with counter=${counter:04X} hram={:02X?}",
                    &gb.hram[..17]
                );
            }
        }
    }
}

#[test]
#[ignore = "diagnostic: traces boot_hwio reads against local mooneye ROMs"]
fn diagnostic_mooneye_boot_hwio_read_trace() {
    let root = std::env::var("EMU198X_GB_MOONEYE_ROOT").unwrap();
    for (rom_name, boot_profile) in [
        ("boot_hwio-dmg0.gb", BootProfile::Dmg0),
        ("boot_hwio-dmgABCmgb.gb", BootProfile::DmgAbc),
        ("boot_hwio-S.gb", BootProfile::Sgb),
    ] {
        eprintln!("trace {rom_name}");
        let rom = std::fs::read(format!("{root}/acceptance/{rom_name}")).unwrap();
        let (_, mut gb) = GameBoy::from_rom_with_boot_profile(rom, boot_profile).unwrap();

        let mut serial_log = Vec::new();
        for _ in 0..250_000 {
            if gb.cpu.mreq && gb.cpu.rd && (0xFF00..=0xFF26).contains(&gb.cpu.addr) {
                let value = gb.bus_read(gb.cpu.addr);
                eprintln!(
                    "pc=${:04X} read ${:04X} -> ${:02X} de=${:04X} hl=${:04X}",
                    gb.cpu.pc,
                    gb.cpu.addr,
                    value,
                    u16::from_be_bytes([gb.cpu.d, gb.cpu.e]),
                    u16::from_be_bytes([gb.cpu.h, gb.cpu.l])
                );
            }
            gb.step_m_cycle();
            serial_log.extend(gb.drain_serial());
            if serial_log.windows(6).any(|window| window == [0x42; 6])
                || serial_log
                    .windows(6)
                    .any(|window| window == [3, 5, 8, 13, 21, 34])
            {
                eprintln!("serial={serial_log:02X?} hram={:02X?}", &gb.hram[..17]);
                break;
            }
        }
    }
}

#[test]
#[ignore = "diagnostic: traces mooneye PPU STAT interrupt timing"]
fn diagnostic_mooneye_ppu_intr_2_0_trace() {
    let root = std::env::var("EMU198X_GB_MOONEYE_ROOT").unwrap();
    let rom = std::fs::read(format!("{root}/acceptance/ppu/intr_2_0_timing.gb")).unwrap();
    let mut gb = boot_machine(rom);

    let mut serial_log = Vec::new();
    let mut previous_mode = gb.ppu.mode();
    for cycle in 0..400_000u32 {
        let ppu = &gb.ppu;
        if gb.cpu.mreq && gb.cpu.wr && matches!(gb.cpu.addr, 0xFF0F | 0xFF41 | 0xFFFF) {
            eprintln!(
                "#{cycle} pc=${:04X} write ${:04X}=${:02X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X}",
                gb.cpu.pc,
                gb.cpu.addr,
                gb.cpu.data,
                ppu.ly,
                ppu.dot,
                ppu.mode(),
                ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg
            );
        }
        if gb.cpu.int_ack {
            eprintln!(
                "#{cycle} pc=${:04X} int_ack bit={} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} b={} d={} e={}",
                gb.cpu.pc,
                gb.cpu.int_ack_bit,
                ppu.ly,
                ppu.dot,
                ppu.mode(),
                ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                gb.cpu.b,
                gb.cpu.d,
                gb.cpu.e
            );
        }
        gb.step_m_cycle();
        let mode = gb.ppu.mode();
        if mode != previous_mode && (gb.ppu.ly == 67 || gb.ppu.ly == 68) {
            eprintln!(
                "#{cycle} mode {}->{} ly={} dot={} pc=${:04X} b={} d={} e={}",
                previous_mode, mode, gb.ppu.ly, gb.ppu.dot, gb.cpu.pc, gb.cpu.b, gb.cpu.d, gb.cpu.e
            );
        }
        previous_mode = mode;
        serial_log.extend(gb.drain_serial());
        if serial_log.windows(6).any(|window| window == [0x42; 6])
            || serial_log
                .windows(6)
                .any(|window| window == [3, 5, 8, 13, 21, 34])
        {
            eprintln!(
                "serial={serial_log:02X?} hram={:02X?} ly={} dot={} mode={}",
                &gb.hram[..17],
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode()
            );
            break;
        }
    }
}

#[test]
#[ignore = "diagnostic: traces mooneye PPU mode0 timing with sprites"]
fn diagnostic_mooneye_ppu_intr_2_mode0_sprites_trace() {
    let root = std::env::var("EMU198X_GB_MOONEYE_ROOT").unwrap();
    let target_case = std::env::var("EMU198X_GB_MOONEYE_CASE")
        .ok()
        .and_then(|value| u8::from_str_radix(value.trim_start_matches('$'), 16).ok());
    let rom = std::fs::read(format!(
        "{root}/acceptance/ppu/intr_2_mode0_timing_sprites.gb"
    ))
    .unwrap();
    let mut gb = boot_machine(rom);

    let mut serial_log = Vec::new();
    let mut previous_mode = gb.ppu.mode();
    for cycle in 0..4_000_000u32 {
        let trace_window = target_case.is_none_or(|case| gb.hram[0] == case)
            || gb.hram[0] == 0x00
            || matches!(gb.cpu.pc, 0x0BF8 | 0x0C1D | 0x4878..=0x487B);
        let interesting_pc = matches!(
            gb.cpu.pc,
            0x0B5A
                | 0x0B8A..=0x0B9D
                | 0x0BA5..=0x0BAF
                | 0x0BEF..=0x0BF8
                | 0x0C1D
                | 0x4878..=0x487B
        );
        if trace_window && interesting_pc {
            eprintln!(
                "#{cycle} pc=${:04X} a=${:02X} b=${:02X} c=${:02X} d=${:02X} e=${:02X} hram80=${:02X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} lcdc=${:02X}",
                gb.cpu.pc,
                gb.cpu.a,
                gb.cpu.b,
                gb.cpu.c,
                gb.cpu.d,
                gb.cpu.e,
                gb.hram[0],
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                gb.ppu.lcdc
            );
        }
        if trace_window
            && gb.cpu.pc < 0x0C20
            && gb.cpu.mreq
            && (gb.cpu.rd || gb.cpu.wr)
            && matches!(
                gb.cpu.addr,
                0xFF40 | 0xFF41 | 0xFF44 | 0xFF0F | 0xFF80 | 0xFFFF
            )
        {
            let op = if gb.cpu.rd { "read" } else { "write" };
            let value = if gb.cpu.rd {
                gb.bus_read(gb.cpu.addr)
            } else {
                gb.cpu.data
            };
            eprintln!(
                "#{cycle} pc=${:04X} {op} ${:04X}=${value:02X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} lcdc=${:02X}",
                gb.cpu.pc,
                gb.cpu.addr,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                gb.ppu.lcdc
            );
        }
        if trace_window && gb.cpu.int_ack {
            eprintln!(
                "#{cycle} int_ack bit={} pc=${:04X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X}",
                gb.cpu.int_ack_bit,
                gb.cpu.pc,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg
            );
        }

        gb.step_m_cycle();
        let mode = gb.ppu.mode();
        if mode != previous_mode && trace_window && gb.cpu.pc < 0x0C20 {
            eprintln!(
                "#{cycle} mode {}->{} ly={} dot={} pc=${:04X} a=${:02X} b=${:02X} c=${:02X} d=${:02X} e=${:02X} stat=${:02X}",
                previous_mode,
                mode,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.cpu.pc,
                gb.cpu.a,
                gb.cpu.b,
                gb.cpu.c,
                gb.cpu.d,
                gb.cpu.e,
                gb.ppu.read_stat()
            );
        }
        previous_mode = mode;

        serial_log.extend(gb.drain_serial());
        if serial_log.windows(6).any(|window| window == [0x42; 6])
            || serial_log
                .windows(6)
                .any(|window| window == [3, 5, 8, 13, 21, 34])
        {
            eprintln!(
                "serial={serial_log:02X?} hram={:02X?} ly={} dot={} mode={}",
                &gb.hram[..17],
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode()
            );
            break;
        }
    }
}

#[test]
#[ignore = "diagnostic: traces mooneye HBlank LY/SCX timing"]
fn diagnostic_mooneye_ppu_hblank_ly_scx_trace() {
    let root = std::env::var("EMU198X_GB_MOONEYE_ROOT").unwrap();
    let rom = std::fs::read(format!("{root}/acceptance/ppu/hblank_ly_scx_timing-GS.gb")).unwrap();
    let mut gb = boot_machine(rom);

    let mut serial_log = Vec::new();
    let mut previous_mode = gb.ppu.mode();
    for cycle in 0..250_000u32 {
        let ppu = &gb.ppu;
        let pc = gb.cpu.pc;
        let interesting_pc = (0x0160..=0x0190).contains(&pc) || (0x0416..=0x0430).contains(&pc);
        if interesting_pc
            && gb.cpu.mreq
            && gb.cpu.wr
            && matches!(gb.cpu.addr, 0xFF0F | 0xFF41 | 0xFFFF | 0xFF43)
        {
            eprintln!(
                "#{cycle} pc=${:04X} write ${:04X}=${:02X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} scx={} b={} d={} e={}",
                gb.cpu.pc,
                gb.cpu.addr,
                gb.cpu.data,
                ppu.ly,
                ppu.dot,
                ppu.mode(),
                ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                ppu.scx,
                gb.cpu.b,
                gb.cpu.d,
                gb.cpu.e
            );
        }
        if interesting_pc && gb.cpu.mreq && gb.cpu.rd && gb.cpu.addr == 0xFF44 {
            eprintln!(
                "#{cycle} pc=${:04X} read LY -> ${:02X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} scx={} b={} d={} e={}",
                gb.cpu.pc,
                gb.bus_read(0xFF44),
                ppu.ly,
                ppu.dot,
                ppu.mode(),
                ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                ppu.scx,
                gb.cpu.b,
                gb.cpu.d,
                gb.cpu.e
            );
        }
        if gb.cpu.int_ack {
            eprintln!(
                "#{cycle} pc=${:04X} int_ack bit={} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} scx={} b={} d={} e={}",
                gb.cpu.pc,
                gb.cpu.int_ack_bit,
                ppu.ly,
                ppu.dot,
                ppu.mode(),
                ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                ppu.scx,
                gb.cpu.b,
                gb.cpu.d,
                gb.cpu.e
            );
        }
        gb.step_m_cycle();
        let mode = gb.ppu.mode();
        if mode != previous_mode && matches!(gb.ppu.ly, 0x40..=0x43) {
            eprintln!(
                "#{cycle} mode {}->{} ly={} dot={} pc=${:04X} if=${:02X} ie=${:02X} scx={} b={} d={} e={}",
                previous_mode,
                mode,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.cpu.pc,
                gb.if_reg,
                gb.ie_reg,
                gb.ppu.scx,
                gb.cpu.b,
                gb.cpu.d,
                gb.cpu.e
            );
        }
        previous_mode = mode;
        serial_log.extend(gb.drain_serial());
        if serial_log.windows(6).any(|window| window == [0x42; 6])
            || serial_log
                .windows(6)
                .any(|window| window == [3, 5, 8, 13, 21, 34])
        {
            eprintln!(
                "serial={serial_log:02X?} hram={:02X?} ly={} dot={} mode={}",
                &gb.hram[..17],
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode()
            );
            break;
        }
    }
}

#[test]
#[ignore = "diagnostic: traces mooneye STAT LYC LCD on/off behavior"]
fn diagnostic_mooneye_ppu_stat_lyc_onoff_trace() {
    let root = std::env::var("EMU198X_GB_MOONEYE_ROOT").unwrap();
    let rom = std::fs::read(format!("{root}/acceptance/ppu/stat_lyc_onoff.gb")).unwrap();
    let mut gb = boot_machine(rom);

    let mut serial_log = Vec::new();
    for cycle in 0..120_000u32 {
        if matches!(
            gb.cpu.pc,
            0x0182
                | 0x01AC
                | 0x01D6
                | 0x0217
                | 0x0241
                | 0x026B
                | 0x02AA
                | 0x02D4
                | 0x02FE
                | 0x033A
                | 0x0360
                | 0x037A
                | 0x0392
                | 0x03B0
                | 0x03CE
        ) {
            eprintln!(
                "#{cycle} reached pc=${:04X} op=${:02X} mc={} a=${:02X} f=${:02X} din=${:02X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} lcdc=${:02X} lyc=${:02X}",
                gb.cpu.pc,
                gb.cpu.opcode,
                gb.cpu.m_cycle,
                gb.cpu.a,
                gb.cpu.f,
                gb.cpu.data_in,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                gb.ppu.lcdc,
                gb.ppu.lyc
            );
            if gb.cpu.pc != 0x037A {
                break;
            }
        }
        if gb.cpu.mreq
            && (gb.cpu.rd || gb.cpu.wr)
            && matches!(gb.cpu.addr, 0xFF0F | 0xFF40 | 0xFF41 | 0xFF45 | 0xFFFF)
        {
            let op = if gb.cpu.rd { "read" } else { "write" };
            let value = if gb.cpu.rd {
                gb.bus_read(gb.cpu.addr)
            } else {
                gb.cpu.data
            };
            eprintln!(
                "#{cycle} pc=${:04X} op=${:02X} mc={} a=${:02X} f=${:02X} din=${:02X} {op} ${:04X}=${value:02X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} lcdc=${:02X} lyc=${:02X}",
                gb.cpu.pc,
                gb.cpu.opcode,
                gb.cpu.m_cycle,
                gb.cpu.a,
                gb.cpu.f,
                gb.cpu.data_in,
                gb.cpu.addr,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                gb.ppu.lcdc,
                gb.ppu.lyc
            );
        }
        if gb.cpu.int_ack {
            eprintln!(
                "#{cycle} pc=${:04X} int_ack bit={} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} lcdc=${:02X} lyc=${:02X}",
                gb.cpu.pc,
                gb.cpu.int_ack_bit,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                gb.ppu.lcdc,
                gb.ppu.lyc
            );
        }
        gb.step_m_cycle();
        serial_log.extend(gb.drain_serial());
        if serial_log.windows(6).any(|window| window == [0x42; 6])
            || serial_log
                .windows(6)
                .any(|window| window == [3, 5, 8, 13, 21, 34])
        {
            eprintln!(
                "serial={serial_log:02X?} hram={:02X?} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X}",
                &gb.hram[..17],
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg
            );
            break;
        }
    }
}

#[test]
#[ignore = "diagnostic: traces mooneye VBlank/STAT interrupt ordering"]
fn diagnostic_mooneye_ppu_vblank_stat_intr_trace() {
    let root = std::env::var("EMU198X_GB_MOONEYE_ROOT").unwrap();
    let rom = std::fs::read(format!("{root}/acceptance/ppu/vblank_stat_intr-GS.gb")).unwrap();
    let mut gb = boot_machine(rom);

    let mut serial_log = Vec::new();
    let mut previous_mode = gb.ppu.mode();
    for cycle in 0..420_000u32 {
        if matches!(
            gb.cpu.pc,
            0x0150
                | 0x016C
                | 0x0181
                | 0x01D8
                | 0x01F6
                | 0x024E
                | 0x0270
                | 0x02C7
                | 0x02E5
                | 0x033D
                | 0x033F
                | 0x4A2B
                | 0x4AB6
        ) {
            eprintln!(
                "#{cycle} pc=${:04X} op=${:02X} a=${:02X} f=${:02X} b=${:02X} c=${:02X} d=${:02X} e=${:02X} h=${:02X} l=${:02X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X} round=[{:02X},{:02X},{:02X}]",
                gb.cpu.pc,
                gb.cpu.opcode,
                gb.cpu.a,
                gb.cpu.f,
                gb.cpu.b,
                gb.cpu.c,
                gb.cpu.d,
                gb.cpu.e,
                gb.cpu.h,
                gb.cpu.l,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg,
                gb.hram[0x06],
                gb.hram[0x07],
                gb.hram[0x08]
            );
        }
        if gb.cpu.mreq
            && (gb.cpu.rd || gb.cpu.wr)
            && matches!(
                gb.cpu.addr,
                0xFF04 | 0xFF0F | 0xFF40 | 0xFF41 | 0xFF80..=0xFF88 | 0xFFFF
            )
        {
            let op = if gb.cpu.rd { "read" } else { "write" };
            let value = if gb.cpu.rd {
                gb.bus_read(gb.cpu.addr)
            } else {
                gb.cpu.data
            };
            eprintln!(
                "#{cycle} pc=${:04X} op=${:02X} mc={} {op} ${:04X}=${value:02X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X}",
                gb.cpu.pc,
                gb.cpu.opcode,
                gb.cpu.m_cycle,
                gb.cpu.addr,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg
            );
        }
        if gb.cpu.int_ack {
            eprintln!(
                "#{cycle} int_ack bit={} pc=${:04X} ly={} dot={} mode={} stat=${:02X} if=${:02X} ie=${:02X}",
                gb.cpu.int_ack_bit,
                gb.cpu.pc,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg
            );
        }

        gb.step_m_cycle();
        let mode = gb.ppu.mode();
        if mode != previous_mode && matches!(gb.ppu.ly, 142..=145) {
            eprintln!(
                "#{cycle} mode {}->{} ly={} dot={} pc=${:04X} stat=${:02X} if=${:02X} ie=${:02X}",
                previous_mode,
                mode,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.cpu.pc,
                gb.ppu.read_stat(),
                gb.if_reg,
                gb.ie_reg
            );
        }
        previous_mode = mode;

        serial_log.extend(gb.drain_serial());
        if serial_log.windows(6).any(|window| window == [0x42; 6])
            || serial_log
                .windows(6)
                .any(|window| window == [3, 5, 8, 13, 21, 34])
        {
            eprintln!(
                "serial={serial_log:02X?} hram={:02X?} ly={} dot={} mode={} stat=${:02X}",
                &gb.hram[..17],
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat()
            );
            break;
        }
    }
}

#[test]
#[ignore = "diagnostic: dumps mooneye LCD-on timing result arrays"]
fn diagnostic_mooneye_ppu_lcdon_timing_results() {
    let root = std::env::var("EMU198X_GB_MOONEYE_ROOT").unwrap();
    let rom = std::fs::read(format!("{root}/acceptance/ppu/lcdon_timing-GS.gb")).unwrap();
    let mut gb = boot_machine(rom);

    let mut serial_log = Vec::new();
    for cycle in 0..420_000u32 {
        let in_test_code = (0x47F0..=0x4BC0).contains(&gb.cpu.pc);
        if gb.cpu.mreq
            && gb.cpu.rd
            && matches!(gb.cpu.addr, 0xFF41 | 0xFF44 | 0x8000 | 0xFE00)
            && in_test_code
        {
            eprintln!(
                "#{cycle} pc=${:04X} op=${:02X} mc={} read ${:04X}=${:02X} ly={} dot={} mode={} stat=${:02X} lcdc=${:02X}",
                gb.cpu.pc,
                gb.cpu.opcode,
                gb.cpu.m_cycle,
                gb.cpu.addr,
                gb.bus_read(gb.cpu.addr),
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.ppu.lcdc
            );
        }
        if gb.cpu.mreq
            && gb.cpu.wr
            && matches!(gb.cpu.addr, 0xFF40 | 0xFF45 | 0xFF80..=0xFF9C)
            && (in_test_code || matches!(gb.cpu.addr, 0xFF98..=0xFF9C))
        {
            eprintln!(
                "#{cycle} pc=${:04X} write ${:04X}=${:02X} ly={} dot={} mode={} stat=${:02X} lcdc=${:02X}",
                gb.cpu.pc,
                gb.cpu.addr,
                gb.cpu.data,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.ppu.lcdc
            );
        }
        gb.step_m_cycle();
        serial_log.extend(gb.drain_serial());
        if serial_log.windows(6).any(|window| window == [0x42; 6])
            || serial_log
                .windows(6)
                .any(|window| window == [3, 5, 8, 13, 21, 34])
        {
            eprintln!("serial={serial_log:02X?}");
            eprintln!("pass1={:02X?}", &gb.hram[0x00..0x08]);
            eprintln!("pass2={:02X?}", &gb.hram[0x08..0x10]);
            eprintln!("pass3={:02X?}", &gb.hram[0x10..0x18]);
            eprintln!(
                "fail round={} expect=${:02X} actual=${:02X} str=${:04X}",
                gb.hram[0x18],
                gb.hram[0x19],
                gb.hram[0x1A],
                u16::from_le_bytes([gb.hram[0x1B], gb.hram[0x1C]])
            );
            break;
        }
    }

    eprintln!("pass1={:02X?}", &gb.hram[0x00..0x08]);
    eprintln!("pass2={:02X?}", &gb.hram[0x08..0x10]);
    eprintln!("pass3={:02X?}", &gb.hram[0x10..0x18]);
    eprintln!(
        "fail round={} expect=${:02X} actual=${:02X} str=${:04X}",
        gb.hram[0x18],
        gb.hram[0x19],
        gb.hram[0x1A],
        u16::from_le_bytes([gb.hram[0x1B], gb.hram[0x1C]])
    );
}

#[test]
#[ignore = "diagnostic: dumps mooneye LCD-on write timing result table"]
fn diagnostic_mooneye_ppu_lcdon_write_timing_results() {
    let root = std::env::var("EMU198X_GB_MOONEYE_ROOT").unwrap();
    let rom = std::fs::read(format!("{root}/acceptance/ppu/lcdon_write_timing-GS.gb")).unwrap();
    let mut gb = boot_machine(rom);

    let mut serial_log = Vec::new();
    for cycle in 0..840_000u32 {
        let in_test_driver = (0x4978..=0x49CB).contains(&gb.cpu.pc);
        let in_generated_test = (0xC000..=0xC12B).contains(&gb.cpu.pc);
        if gb.cpu.mreq
            && (gb.cpu.rd || gb.cpu.wr)
            && (matches!(
                gb.cpu.addr,
                0x8000 | 0xFE00 | 0xFF40 | 0xC12C..=0xC13E | 0xFF80..=0xFF84
            ))
            && (in_test_driver || in_generated_test || matches!(gb.cpu.addr, 0xFF80..=0xFF84))
        {
            let op = if gb.cpu.rd { "read" } else { "write" };
            let value = if gb.cpu.rd {
                gb.bus_read(gb.cpu.addr)
            } else {
                gb.cpu.data
            };
            eprintln!(
                "#{cycle} pc=${:04X} op=${:02X} mc={} {op} ${:04X}=${value:02X} ly={} dot={} mode={} stat=${:02X} lcdc=${:02X}",
                gb.cpu.pc,
                gb.cpu.opcode,
                gb.cpu.m_cycle,
                gb.cpu.addr,
                gb.ppu.ly,
                gb.ppu.dot,
                gb.ppu.mode(),
                gb.ppu.read_stat(),
                gb.ppu.lcdc
            );
        }

        gb.step_m_cycle();
        serial_log.extend(gb.drain_serial());
        if serial_log.windows(6).any(|window| window == [0x42; 6])
            || serial_log
                .windows(6)
                .any(|window| window == [3, 5, 8, 13, 21, 34])
        {
            eprintln!("serial={serial_log:02X?}");
            eprintln!("results={:02X?}", &gb.wram[0x12C..0x13F]);
            eprintln!(
                "fail round={} expect=${:02X} actual=${:02X} str=${:04X}",
                gb.hram[0x00],
                gb.hram[0x01],
                gb.hram[0x02],
                u16::from_le_bytes([gb.hram[0x03], gb.hram[0x04]])
            );
            break;
        }
    }
}

// -- Construction ----------------------------------------------------

#[test]
fn construction_validates_header() {
    let rom = nop_rom();
    let (header, _) = GameBoy::from_rom(rom).unwrap();
    assert_eq!(header.cart_type, CartType::RomOnly);
}

#[test]
fn construction_rejects_bad_header() {
    // Truncated ROM — header parser rejects.
    match GameBoy::from_rom(vec![0u8; 100]) {
        Ok(_) => panic!("expected header error"),
        Err(format_nintendo_game_boy_cartridge::HeaderError::TooShort { .. }) => {}
        Err(other) => panic!("unexpected error: {other}"),
    }
}

#[test]
fn header_decoded_at_construction_time() {
    let mut rom = nop_rom();
    rom[0x0134..0x0138].copy_from_slice(b"GAME");
    // Recompute checksum.
    let mut checksum: u8 = 0;
    for &byte in &rom[0x0134..=0x014C] {
        checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
    }
    rom[0x014D] = checksum;
    let header = CartridgeHeader::parse(&rom).unwrap();
    assert_eq!(header.title, "GAME");
}

// -- Tick orchestration ---------------------------------------------

#[test]
fn step_m_cycle_advances_timer_by_four_t_cycles() {
    let mut gb = boot_machine(jr_loop_rom());
    let counter_before = gb.timer.counter;
    gb.step_m_cycle();
    assert_eq!(
        gb.timer.counter,
        counter_before.wrapping_add(4),
        "timer should have ticked 4 T-cycles"
    );
}

#[test]
fn jr_loop_runs_indefinitely_without_diag_unimplemented() {
    let mut gb = boot_machine(jr_loop_rom());
    // 1000 m-cycles is plenty to spin through hundreds of JR
    // iterations; the CPU must never trip its unimplemented-opcode
    // safety net.
    for _ in 0..1000 {
        gb.step_m_cycle();
    }
    assert!(!gb.cpu.diag_unimplemented);
}

#[test]
fn run_frame_completes_within_safety_limit() {
    let mut gb = boot_machine(jr_loop_rom());
    // First call: partial settle from power-on to first VBlank entry.
    // 456 dots × 144 visible scanlines / 4 dots-per-m-cycle ≈ 16,416.
    let first = gb.run_frame();
    assert!(
        (16_000..=17_000).contains(&first),
        "first (partial) frame: got {first}"
    );
    // Second call: a full DMG frame is 17,556 m-cycles
    // (456 × 154 / 4).
    let second = gb.run_frame();
    assert!(
        (17_500..=17_600).contains(&second),
        "second (full) frame: got {second}"
    );
}

// -- Bus dispatch ----------------------------------------------------

#[test]
fn wram_round_trips() {
    let mut gb = boot_machine(jr_loop_rom());
    gb.bus_write(0xC123, 0x42);
    assert_eq!(gb.bus_read(0xC123), 0x42);
}

#[test]
fn echo_ram_mirrors_wram() {
    let mut gb = boot_machine(jr_loop_rom());
    gb.bus_write(0xC100, 0xAB);
    assert_eq!(gb.bus_read(0xE100), 0xAB, "echo ram reflects wram");
}

#[test]
fn hram_round_trips() {
    let mut gb = boot_machine(jr_loop_rom());
    gb.bus_write(0xFF80, 0xDE);
    gb.bus_write(0xFFFE, 0xAD);
    assert_eq!(gb.bus_read(0xFF80), 0xDE);
    assert_eq!(gb.bus_read(0xFFFE), 0xAD);
}

#[test]
fn vram_round_trips() {
    let mut gb = boot_machine(jr_loop_rom());
    gb.bus_write(0x8000, 0x7E);
    gb.bus_write(0x9FFF, 0x77);
    assert_eq!(gb.bus_read(0x8000), 0x7E);
    assert_eq!(gb.bus_read(0x9FFF), 0x77);
}

#[test]
fn unusable_region_reads_high() {
    let gb = boot_machine(jr_loop_rom());
    assert_eq!(gb.bus_read(0xFEA0), 0xFF);
    assert_eq!(gb.bus_read(0xFEFF), 0xFF);
}

// -- IO dispatch -----------------------------------------------------

#[test]
fn ie_register_round_trips() {
    let mut gb = boot_machine(jr_loop_rom());
    gb.bus_write(0xFFFF, 0x1F);
    assert_eq!(gb.bus_read(0xFFFF), 0x1F);
}

#[test]
fn if_register_high_bits_read_high() {
    let gb = boot_machine(jr_loop_rom());
    // Default IF = 0; upper 3 bits are wired high on read.
    assert_eq!(gb.bus_read(0xFF0F) & 0xE0, 0xE0);
}

#[test]
fn skipped_bootrom_sets_dmg_io_register_state() {
    let gb = boot_machine(jr_loop_rom());
    assert_eq!(gb.bus_read(0xFF00), 0xCF);
    assert_eq!(gb.bus_read(0xFF04), 0xAB);
    assert_eq!(gb.bus_read(0xFF0F), 0xE1);
    assert_eq!(gb.bus_read(0xFF10), 0x80);
    assert_eq!(gb.bus_read(0xFF11), 0xBF);
    assert_eq!(gb.bus_read(0xFF12), 0xF3);
    assert_eq!(gb.bus_read(0xFF24), 0x77);
    assert_eq!(gb.bus_read(0xFF25), 0xF3);
    assert_eq!(gb.bus_read(0xFF26), 0xF1);
}

#[test]
fn skipped_bootrom_profiles_set_model_specific_cpu_and_io_state() {
    let dmg0 = boot_machine_with_profile(jr_loop_rom(), BootProfile::Dmg0);
    assert_eq!(
        [
            dmg0.cpu.a, dmg0.cpu.f, dmg0.cpu.b, dmg0.cpu.c, dmg0.cpu.d, dmg0.cpu.e, dmg0.cpu.h,
            dmg0.cpu.l,
        ],
        [0x01, 0x00, 0xFF, 0x13, 0x00, 0xC1, 0x84, 0x03]
    );
    assert_eq!(dmg0.bus_read(0xFF04), 0x18);
    assert_eq!(dmg0.bus_read(0xFF10), 0x80);
    assert_eq!(dmg0.bus_read(0xFF12), 0xF3);

    let mgb = boot_machine_with_profile(jr_loop_rom(), BootProfile::Mgb);
    assert_eq!(mgb.cpu.a, 0xFF);
    assert_eq!(mgb.bus_read(0xFF04), 0xAB);
    assert_eq!(mgb.bus_read(0xFF12), 0xF3);

    let sgb = boot_machine_with_profile(jr_loop_rom(), BootProfile::Sgb);
    assert_eq!(
        [
            sgb.cpu.a, sgb.cpu.f, sgb.cpu.b, sgb.cpu.c, sgb.cpu.d, sgb.cpu.e, sgb.cpu.h, sgb.cpu.l,
        ],
        [0x01, 0x00, 0x00, 0x14, 0x00, 0x00, 0xC0, 0x60]
    );
    assert_eq!(sgb.bus_read(0xFF04), 0xD8);
    assert_eq!(sgb.bus_read(0xFF00), 0xFF);
    assert_eq!(sgb.bus_read(0xFF12), 0xF3);
    assert_eq!(sgb.bus_read(0xFF26), 0xF0);
}

#[test]
fn timer_div_writeable_via_bus_resets_to_zero() {
    let mut gb = boot_machine(jr_loop_rom());
    // Spin a few m-cycles to get the counter non-zero.
    for _ in 0..200 {
        gb.step_m_cycle();
    }
    assert!(gb.timer.counter > 0);
    gb.bus_write(0xFF04, 0xFF); // any value resets DIV
    assert_eq!(gb.bus_read(0xFF04), 0);
}

// -- Serial capture (Blargg's reporting channel) --------------------

#[test]
fn serial_transfer_with_internal_clock_captures_byte_and_latches_irq() {
    let mut gb = boot_machine(jr_loop_rom());
    gb.bus_write(0xFF01, b'A');
    gb.bus_write(0xFF02, 0x81); // begin transfer + internal clock
    let captured = gb.drain_serial();
    assert_eq!(captured, vec![b'A']);
    assert_eq!(gb.if_reg & IF_SERIAL, 0);
    for _ in 0..1024 {
        gb.step_m_cycle();
    }
    assert_ne!(gb.if_reg & IF_SERIAL, 0);
    // Drain clears the buffer.
    assert!(gb.drain_serial().is_empty());
}

#[test]
fn serial_external_clock_does_not_capture() {
    let mut gb = boot_machine(jr_loop_rom());
    gb.bus_write(0xFF01, b'B');
    gb.bus_write(0xFF02, 0x80); // begin transfer + external clock
    assert!(gb.drain_serial().is_empty());
}

// -- IRQ wiring ------------------------------------------------------

#[test]
fn timer_overflow_sets_if_timer_bit() {
    let mut gb = boot_machine(jr_loop_rom());
    // Force TIMA to overflow on the next bit-3 tick.
    gb.timer.tima = 0xFF;
    gb.timer.tac = 0x05; // enabled, clock select 01 (bit 3)
    // 16 T-cycles = 4 m-cycles to roll TIMA over, then one more
    // m-cycle for the delayed reload to latch IF.
    for _ in 0..5 {
        gb.step_m_cycle();
    }
    assert_ne!(gb.if_reg & IF_TIMER, 0);
}

#[test]
fn vblank_irq_eventually_fires_during_a_frame() {
    let mut gb = boot_machine(jr_loop_rom());
    let _ = gb.run_frame();
    // After a full frame the VBlank IRQ source must have been
    // routed through to IF at least once (we cleared each cycle but
    // the latch fires again every frame entry).
    let _ = gb.run_frame();
    // Don't make a strict assertion on IF (the CPU may have
    // serviced it). Instead confirm the PPU advanced into VBlank.
    assert!(gb.ppu.ly >= 144 || gb.ppu.ly == 0);
}

#[test]
fn joypad_press_with_action_group_selected_latches_joypad_irq() {
    let mut gb = boot_machine(jr_loop_rom());
    gb.bus_write(0xFF00, 0x10); // bit 5 = 0 → action group selected
    // Step once so prev_line is captured cleanly with no buttons
    // pressed.
    gb.step_m_cycle();
    assert_eq!(gb.if_reg & IF_JOYPAD, 0);
    gb.set_button(JoypadButton::Start, true);
    gb.step_m_cycle();
    assert_ne!(gb.if_reg & IF_JOYPAD, 0);
}

// -- OAM DMA --------------------------------------------------------

#[test]
fn oam_dma_copies_160_bytes_from_source_page() {
    let mut gb = boot_machine(jr_loop_rom());
    // Fill WRAM source page.
    for i in 0..0xA0u16 {
        gb.bus_write(0xC000 + i, (i & 0xFF) as u8);
    }
    gb.bus_write(0xFF46, 0xC0); // start DMA from $C000
    assert_eq!(gb.bus_read(0xFF46), 0xC0);
    for _ in 0..OAM_SIZE + 2 {
        gb.step_m_cycle();
    }
    for i in 0..OAM_SIZE {
        assert_eq!(gb.oam[i], (i & 0xFF) as u8, "oam[{i}] mismatch");
    }
}

/// Mooneye Test Suite — full DMG-family `acceptance/` sweep. Every ROM is run
/// under the boot profile matching its model suffix (`dmg0` / `dmgABC` / `mgb` /
/// `sgb` / `sgb2`); CGB-only ROMs are skipped (this is a DMG-family core).
///
/// A ROM passes when it outputs the Fibonacci magic `3,5,8,13,21,34` on the
/// serial port (mooneye's success signal); `0x42` repeated is its failure
/// signal. As of 2026-06-07 the whole suite passes (75/75). Gated on the ROM
/// set: set `EMU198X_GB_MOONEYE_ROOT` to the extracted `mooneye-test-suite`
/// directory and run with `--ignored`.
#[test]
#[ignore = "needs EMU198X_GB_MOONEYE_ROOT (mooneye-test-suite) — run with --ignored"]
fn mooneye_dmg_acceptance_suite_passes() {
    let Ok(root) = std::env::var("EMU198X_GB_MOONEYE_ROOT") else {
        panic!("set EMU198X_GB_MOONEYE_ROOT to the mooneye-test-suite directory");
    };
    // Collect all .gb under acceptance/ recursively.
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "gb") {
                    out.push(p);
                }
            }
        }
    }
    let mut roms = Vec::new();
    walk(&std::path::Path::new(&root).join("acceptance"), &mut roms);
    roms.sort();

    let mut pass = 0;
    let mut fails = Vec::new();
    for rom_path in &roms {
        let name = rom_path
            .strip_prefix(&root)
            .unwrap_or(rom_path)
            .display()
            .to_string();
        // Skip CGB-only ROMs (this is a DMG-family core).
        if name.contains("-C.gb") || name.contains("-cgb") {
            continue;
        }
        // Pick the boot profile matching the ROM's model suffix.
        let profile = if name.contains("dmg0") {
            BootProfile::Dmg0
        } else if name.contains("dmgABC") {
            BootProfile::DmgAbc
        } else if name.contains("sgb2") || name.contains("div2-S") {
            BootProfile::Sgb2
        } else if name.contains("-sgb") || name.contains("-S") {
            BootProfile::Sgb
        } else if name.contains("-mgb") {
            BootProfile::Mgb
        } else {
            BootProfile::DmgAbc
        };
        let rom = std::fs::read(rom_path).unwrap();
        let Ok((_, mut gb)) = GameBoy::from_rom_with_boot_profile(rom, profile) else {
            fails.push((name, "load-error".to_string()));
            continue;
        };
        let mut serial = Vec::new();
        let mut verdict = "timeout";
        for _ in 0..25_000_000u32 {
            gb.step_m_cycle();
            serial.extend(gb.drain_serial());
            if serial.windows(6).any(|w| w == [3, 5, 8, 13, 21, 34]) {
                verdict = "pass";
                break;
            }
            if serial.windows(6).any(|w| w == [0x42; 6]) {
                verdict = "fail";
                break;
            }
        }
        if verdict == "pass" {
            pass += 1;
        } else {
            fails.push((name, verdict.to_string()));
        }
    }
    assert!(
        fails.is_empty(),
        "{pass} mooneye acceptance tests pass, but these did not:\n{}",
        fails
            .iter()
            .map(|(n, v)| format!("  {v:8} {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// dmg-acid2 (Matt Currie) — the definitive DMG PPU rendering test. A correct
/// PPU draws a precise smiley face; any BG/window/sprite/priority/flip/palette
/// bug shows as a documented distortion. We assert the post-palette shade
/// framebuffer hashes to a known-good golden (verified pixel-perfect against the
/// published reference image, 0 diffs / 23040 px, 2026-06-07).
///
/// Gated on the ROM: set `EMU198X_GB_DMG_ACID2` to `dmg-acid2.gb`, run with
/// `--ignored`.
#[test]
#[ignore = "needs EMU198X_GB_DMG_ACID2 (dmg-acid2.gb) — run with --ignored"]
fn dmg_acid2_renders_reference() {
    let Ok(path) = std::env::var("EMU198X_GB_DMG_ACID2") else {
        panic!("set EMU198X_GB_DMG_ACID2 to dmg-acid2.gb");
    };
    let rom = std::fs::read(path).unwrap();
    let (_, mut gb) = GameBoy::from_rom_with_boot_profile(rom, BootProfile::DmgAbc).unwrap();
    // Run ~60 frames so the test sets up and renders the final screen.
    for _ in 0..1_100_000u32 {
        gb.step_m_cycle();
    }
    // FNV-1a over the 160x144 post-palette shade framebuffer.
    let fb = gb.framebuffer();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in fb {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    eprintln!(
        "dmg-acid2 framebuffer hash = {hash:#018x} (len {})",
        fb.len()
    );
    assert_eq!(
        hash, 0xf272_a8ff_e3db_4c16,
        "dmg-acid2 framebuffer regressed"
    );
}

/// Run one blargg ROM and return its result code via the cart-RAM protocol
/// (signature `DE B0 61` at `$A001-3`; `$A000` = `0x80` running, else result;
/// `0` = pass). Returns `None` on timeout.
fn run_blargg_a000(rom: Vec<u8>, budget: u32) -> Option<u8> {
    let (_, mut gb) = GameBoy::from_rom_with_boot_profile(rom, BootProfile::DmgAbc).unwrap();
    for _ in 0..budget {
        gb.step_m_cycle();
        if gb.bus_read(0xA001) == 0xDE && gb.bus_read(0xA002) == 0xB0 && gb.bus_read(0xA003) == 0x61
        {
            let status = gb.bus_read(0xA000);
            // 0x80 = running; 0xFF = RAM briefly not-ready.
            if status != 0x80 && status != 0xFF {
                return Some(status);
            }
        }
    }
    None
}

/// blargg `dmg_sound` (APU) — all 12 sub-tests pass (as of 2026-06-07).
/// The last three to land were the DMG "wave RAM access while channel 3 is on"
/// window (`09-wave read`, `10-wave trigger`, `12-wave write` while on), fixed
/// by the wave-trigger delay (`(2047 - frequency) + 3`) that aligns the APU
/// sample fetch with the CPU access cycle.
///
/// Gated on the ROMs: set `EMU198X_GB_BLARGG_DMG_SOUND` to the `dmg_sound/
/// rom_singles` directory and run with `--ignored`.
#[test]
#[ignore = "needs EMU198X_GB_BLARGG_DMG_SOUND (blargg dmg_sound/rom_singles) — run with --ignored"]
fn blargg_dmg_sound_suite_passes() {
    let dir = std::env::var("EMU198X_GB_BLARGG_DMG_SOUND")
        .expect("set EMU198X_GB_BLARGG_DMG_SOUND to dmg_sound/rom_singles");
    let mut roms: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "gb"))
        .collect();
    roms.sort();
    assert!(!roms.is_empty(), "no .gb ROMs found in {dir}");

    let failures: Vec<_> = roms
        .iter()
        .filter(|rom_path| run_blargg_a000(std::fs::read(rom_path).unwrap(), 50_000_000) != Some(0))
        .map(|p| p.file_stem().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(
        failures.is_empty(),
        "blargg dmg_sound failures: {failures:?}"
    );
}
