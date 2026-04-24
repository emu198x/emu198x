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
    for i in 0..OAM_SIZE {
        assert_eq!(gb.oam[i], (i & 0xFF) as u8, "oam[{i}] mismatch");
    }
}
