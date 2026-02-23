use machine_amiga::memory::ROM_BASE;
use machine_amiga::{commodore_agnus_ocs::SlotOwner, Amiga, AmigaBusWrapper, TICKS_PER_CCK};
use motorola_68000::bus::{BusStatus, FunctionCode, M68kBus};

const REG_DDFSTRT: u16 = 0x092;
const REG_DDFSTOP: u16 = 0x094;
const REG_DMACON: u16 = 0x096;
const REG_INTENA: u16 = 0x09A;
const REG_INTREQ: u16 = 0x09C;
const REG_BLTCON0: u16 = 0x040;
const REG_BLTCON1: u16 = 0x042;
const REG_BLTAFWM: u16 = 0x044;
const REG_BLTALWM: u16 = 0x046;
const REG_BLTCPTH: u16 = 0x048;
const REG_BLTCPTL: u16 = 0x04A;
const REG_BLTBPTH: u16 = 0x04C;
const REG_BLTBPTL: u16 = 0x04E;
const REG_BLTAPTH: u16 = 0x050;
const REG_BLTAPTL: u16 = 0x052;
const REG_BLTDPTH: u16 = 0x054;
const REG_BLTDPTL: u16 = 0x056;
const REG_BLTSIZE: u16 = 0x058;
const REG_BLTCMOD: u16 = 0x060;
const REG_BLTBMOD: u16 = 0x062;
const REG_BLTAMOD: u16 = 0x064;
const REG_BLTDMOD: u16 = 0x066;
const REG_BPLCON0: u16 = 0x100;
const REG_BPL1PTH: u16 = 0x0E0;

const DMACON_BLTEN: u16 = 0x0040;
const DMACON_SPREN: u16 = 0x0020;
const DMACON_BPLEN: u16 = 0x0100;
const DMACON_DMAEN: u16 = 0x0200;
const DMACON_BLTPRI: u16 = 0x0400;
const INTENA_SETCLR: u16 = 0x8000;
const INTENA_INTEN: u16 = 0x4000;
const INTREQ_BLIT: u16 = 0x0040;

const VISIBLE_LINE_START: u16 = 0x2C;

fn make_test_amiga() -> Amiga {
    let mut rom = vec![0u8; 256 * 1024];

    let ssp = 0x0007FFF0u32;
    rom[0] = (ssp >> 24) as u8;
    rom[1] = (ssp >> 16) as u8;
    rom[2] = (ssp >> 8) as u8;
    rom[3] = ssp as u8;

    let pc = ROM_BASE + 8;
    rom[4] = (pc >> 24) as u8;
    rom[5] = (pc >> 16) as u8;
    rom[6] = (pc >> 8) as u8;
    rom[7] = pc as u8;

    // BRA.S * (idle loop in ROM; no chip-RAM traffic unless tests probe it)
    rom[8] = 0x60;
    rom[9] = 0xFE;

    Amiga::new(rom)
}

fn write_rom_word(rom: &mut [u8], offset: usize, word: u16) {
    rom[offset] = (word >> 8) as u8;
    rom[offset + 1] = word as u8;
}

fn write_rom_long(rom: &mut [u8], offset: usize, value: u32) {
    rom[offset] = (value >> 24) as u8;
    rom[offset + 1] = (value >> 16) as u8;
    rom[offset + 2] = (value >> 8) as u8;
    rom[offset + 3] = value as u8;
}

fn make_blit_irq_wait_rom_amiga() -> (Amiga, u32) {
    let mut rom = vec![0u8; 256 * 1024];

    let ssp = 0x0007FFF0u32;
    let reset_pc = ROM_BASE + 0x0100;
    let handler_pc = ROM_BASE + 0x0120;
    let handler_loop_pc = handler_pc + 2;

    // Reset vectors
    write_rom_long(&mut rom, 0x0000, ssp);
    write_rom_long(&mut rom, 0x0004, reset_pc);
    // Vector 27 = level 3 autovector (BLIT/VERTB/COPER priority level)
    write_rom_long(&mut rom, 27 * 4, handler_pc);

    // Reset code @ $0100:
    //   MOVE.W #$2000,SR   ; supervisor, interrupt mask = 0
    //   STOP   #$2000      ; wait for interrupt
    //   BRA.S  *           ; fallback loop if STOP returns
    write_rom_word(&mut rom, 0x0100, 0x46FC);
    write_rom_word(&mut rom, 0x0102, 0x2000);
    write_rom_word(&mut rom, 0x0104, 0x4E72);
    write_rom_word(&mut rom, 0x0106, 0x2000);
    write_rom_word(&mut rom, 0x0108, 0x60FE);

    // Level-3 handler @ $0120:
    //   MOVEQ   #$42,D0    ; marker that handler ran
    // hloop:
    //   BRA.S   hloop
    write_rom_word(&mut rom, 0x0120, 0x7042);
    write_rom_word(&mut rom, 0x0122, 0x60FE);

    (Amiga::new(rom), handler_loop_pc)
}

fn tick_ccks(amiga: &mut Amiga, ccks: u32) {
    for _ in 0..ccks {
        for _ in 0..TICKS_PER_CCK {
            amiga.tick();
        }
    }
}

fn write_chip_word(amiga: &mut Amiga, addr: u32, val: u16) {
    amiga.memory.write_byte(addr, (val >> 8) as u8);
    amiga.memory.write_byte(addr + 1, val as u8);
}

fn read_chip_word(amiga: &Amiga, addr: u32) -> u16 {
    (u16::from(amiga.memory.read_chip_byte(addr)) << 8)
        | u16::from(amiga.memory.read_chip_byte(addr + 1))
}

fn write_ptr(amiga: &mut Amiga, reg_hi: u16, reg_lo: u16, addr: u32) {
    amiga.write_custom_reg(reg_hi, (addr >> 16) as u16);
    amiga.write_custom_reg(reg_lo, (addr & 0xFFFF) as u16);
}

fn enable_display_dma_contention(amiga: &mut Amiga) {
    amiga.agnus.vpos = VISIBLE_LINE_START;
    amiga.agnus.hpos = 0;

    amiga.write_custom_reg(REG_DDFSTRT, 0x001C);
    amiga.write_custom_reg(REG_DDFSTOP, 0x00D8);
    amiga.write_custom_reg(REG_BPLCON0, 6 << 12);

    // Point bitplanes away from blitter buffers.
    let bpl_base = 0x0001_8000u32;
    for plane in 0..6u32 {
        let addr = bpl_base + plane * 0x0400;
        let reg_hi = REG_BPL1PTH + (plane as u16) * 4;
        let reg_lo = reg_hi + 2;
        write_ptr(amiga, reg_hi, reg_lo, addr);
    }
}

fn start_area_blit_copy_c(
    amiga: &mut Amiga,
    width_words: u16,
    height_rows: u16,
    enable_a: bool,
    enable_b: bool,
    enable_c: bool,
    enable_d: bool,
    base_a: u32,
    base_b: u32,
    base_c: u32,
    base_d: u32,
) -> u32 {
    let words = u32::from(width_words) * u32::from(height_rows);
    for i in 0..words {
        let off = i * 2;
        write_chip_word(amiga, base_a + off, (0x1000 + i) as u16);
        write_chip_word(amiga, base_b + off, (0x2000 + i) as u16);
        write_chip_word(amiga, base_c + off, (0x3000 + i) as u16);
        write_chip_word(amiga, base_d + off, 0x0000);
    }

    amiga.write_custom_reg(REG_BLTCON1, 0x0000);
    amiga.write_custom_reg(REG_BLTAFWM, 0xFFFF);
    amiga.write_custom_reg(REG_BLTALWM, 0xFFFF);
    amiga.write_custom_reg(REG_BLTAMOD, 0);
    amiga.write_custom_reg(REG_BLTBMOD, 0);
    amiga.write_custom_reg(REG_BLTCMOD, 0);
    amiga.write_custom_reg(REG_BLTDMOD, 0);

    write_ptr(amiga, REG_BLTAPTH, REG_BLTAPTL, base_a);
    write_ptr(amiga, REG_BLTBPTH, REG_BLTBPTL, base_b);
    write_ptr(amiga, REG_BLTCPTH, REG_BLTCPTL, base_c);
    write_ptr(amiga, REG_BLTDPTH, REG_BLTDPTL, base_d);

    // LF = C copy (0xAA), with channel enables selected per test.
    let mut bltcon0 = 0x00AA;
    if enable_a {
        bltcon0 |= 0x0800;
    }
    if enable_b {
        bltcon0 |= 0x0400;
    }
    if enable_c {
        bltcon0 |= 0x0200;
    }
    if enable_d {
        bltcon0 |= 0x0100;
    }
    amiga.write_custom_reg(REG_BLTCON0, bltcon0);

    amiga.write_custom_reg(REG_BLTSIZE, (height_rows << 6) | (width_words & 0x3F));

    let ops_per_word =
        u32::from(enable_a) + u32::from(enable_b) + u32::from(enable_c) + u32::from(enable_d);
    words * ops_per_word.max(1)
}

fn start_line_blit_horizontal(
    amiga: &mut Amiga,
    start_word_addr: u32,
    length_pixels: u16,
    start_pixel_bit: u16,
) -> u32 {
    let words_touched = u32::from(length_pixels.div_ceil(16));
    for i in 0..words_touched {
        write_chip_word(amiga, start_word_addr + i * 2, 0x0000);
    }

    // LINE mode, SING mode, octant code 110 -> octant 0 (+X, X-major).
    // See Agnus line-runtime octant decode.
    amiga.write_custom_reg(REG_BLTCON1, 0x001B);
    // ASH = start bit, A/C/D enabled, LF = A (0xF0) to set the plotted pixel.
    amiga.write_custom_reg(
        REG_BLTCON0,
        ((start_pixel_bit & 0xF) << 12) | 0x0800 | 0x0200 | 0x0100 | 0x00F0,
    );
    amiga.write_custom_reg(REG_BLTAFWM, 0x8000);
    amiga.write_custom_reg(REG_BLTALWM, 0xFFFF);
    amiga.write_custom_reg(REG_BLTAMOD, -4i16 as u16);
    amiga.write_custom_reg(REG_BLTBMOD, 0);
    amiga.write_custom_reg(REG_BLTCMOD, 40);
    amiga.write_custom_reg(REG_BLTDMOD, 40);
    amiga.write_custom_reg(REG_BLTBPTH, 0);
    amiga.write_custom_reg(REG_BLTBPTL, 0);
    amiga.write_custom_reg(REG_BLTAPTH, 0);
    amiga.write_custom_reg(REG_BLTAPTL, 0xFFFF); // error accumulator = -1
    amiga.write_custom_reg(REG_BLTCPTH, (start_word_addr >> 16) as u16);
    amiga.write_custom_reg(REG_BLTCPTL, (start_word_addr & 0xFFFF) as u16);
    amiga.write_custom_reg(REG_BLTDPTH, (start_word_addr >> 16) as u16);
    amiga.write_custom_reg(REG_BLTDPTL, (start_word_addr & 0xFFFF) as u16);

    // In line mode, BLTSIZE height field is line length in pixels; width is ignored
    // (commonly programmed as 2).
    amiga.write_custom_reg(REG_BLTSIZE, (length_pixels << 6) | 2);

    u32::from(length_pixels) * 2 // one ReadC + one WriteD per step
}

fn run_blit_to_completion(amiga: &mut Amiga, max_ccks: u32) -> Option<(u32, u32)> {
    let mut elapsed_ccks = 0u32;
    let mut progress_grants = 0u32;

    while elapsed_ccks <= max_ccks {
        if !amiga.agnus.blitter_busy {
            return Some((elapsed_ccks, progress_grants));
        }
        let plan = amiga.agnus.cck_bus_plan();
        if plan.blitter_dma_progress_granted {
            progress_grants += 1;
        }
        if elapsed_ccks == max_ccks {
            break;
        }
        tick_ccks(amiga, 1);
        elapsed_ccks += 1;
    }

    None
}

fn wait_until_pre_final_blitter_op_on_granted_cck(amiga: &mut Amiga, max_ccks: u32) -> bool {
    for _ in 0..=max_ccks {
        if amiga.agnus.blitter_busy && amiga.agnus.blitter_ccks_remaining == 1 {
            let plan = amiga.agnus.cck_bus_plan();
            if plan.blitter_dma_progress_granted {
                return true;
            }
        }
        if (amiga.paula.intreq & INTREQ_BLIT) != 0 {
            return false;
        }
        tick_ccks(amiga, 1);
    }
    false
}

fn poll_chip_word_via_cpu_bus(amiga: &mut Amiga, addr: u32) -> BusStatus {
    let mut bus = AmigaBusWrapper {
        agnus: &mut amiga.agnus,
        memory: &mut amiga.memory,
        denise: &mut amiga.denise,
        copper: &mut amiga.copper,
        cia_a: &mut amiga.cia_a,
        cia_b: &mut amiga.cia_b,
        paula: &mut amiga.paula,
        floppy: &mut amiga.floppy,
        keyboard: &mut amiga.keyboard,
    };
    M68kBus::poll_cycle(
        &mut bus,
        addr,
        FunctionCode::SupervisorData,
        true,
        true,
        None,
    )
}

fn poll_ipl_via_cpu_bus(amiga: &mut Amiga) -> u8 {
    let mut bus = AmigaBusWrapper {
        agnus: &mut amiga.agnus,
        memory: &mut amiga.memory,
        denise: &mut amiga.denise,
        copper: &mut amiga.copper,
        cia_a: &mut amiga.cia_a,
        cia_b: &mut amiga.cia_b,
        paula: &mut amiga.paula,
        floppy: &mut amiga.floppy,
        keyboard: &mut amiga.keyboard,
    };
    M68kBus::poll_ipl(&mut bus)
}

fn poll_iack_cycle_via_cpu_bus(amiga: &mut Amiga) -> BusStatus {
    let mut bus = AmigaBusWrapper {
        agnus: &mut amiga.agnus,
        memory: &mut amiga.memory,
        denise: &mut amiga.denise,
        copper: &mut amiga.copper,
        cia_a: &mut amiga.cia_a,
        cia_b: &mut amiga.cia_b,
        paula: &mut amiga.paula,
        floppy: &mut amiga.floppy,
        keyboard: &mut amiga.keyboard,
    };
    M68kBus::poll_cycle(
        &mut bus,
        0x00FF_FFFF,
        FunctionCode::InterruptAck,
        true,
        true,
        None,
    )
}

fn sample_cpu_slot_chip_reads_while_blitter_busy(
    amiga: &mut Amiga,
    desired_samples: usize,
    max_ccks: u32,
    probe_addr: u32,
) -> Vec<BusStatus> {
    let mut out = Vec::with_capacity(desired_samples);
    let mut elapsed = 0u32;
    while out.len() < desired_samples && elapsed < max_ccks {
        if amiga.agnus.blitter_busy {
            let plan = amiga.agnus.cck_bus_plan();
            if matches!(plan.slot_owner, SlotOwner::Cpu) {
                out.push(poll_chip_word_via_cpu_bus(amiga, probe_addr));
            }
        }
        tick_ccks(amiga, 1);
        elapsed += 1;
    }
    out
}

#[test]
fn area_blit_dma_heavy_contention_increases_elapsed_ccks_but_not_progress_grants() {
    fn run_case(enable_display_contention: bool) -> (u32, u32, u32) {
        let mut amiga = make_test_amiga();
        let base_a = 0x0000_4000u32;
        let base_b = 0x0000_5000u32;
        let base_c = 0x0000_6000u32;
        let base_d = 0x0000_7000u32;
        let width_words = 24u16;
        let height_rows = 6u16;

        amiga.agnus.vpos = VISIBLE_LINE_START;
        amiga.agnus.hpos = 0;

        if enable_display_contention {
            enable_display_dma_contention(&mut amiga);
        }

        let mut dmacon = 0x8000 | DMACON_DMAEN | DMACON_BLTEN;
        if enable_display_contention {
            dmacon |= DMACON_BPLEN | DMACON_SPREN;
        }
        amiga.write_custom_reg(REG_DMACON, dmacon);

        let expected_ops = start_area_blit_copy_c(
            &mut amiga,
            width_words,
            height_rows,
            true,
            true,
            true,
            true,
            base_a,
            base_b,
            base_c,
            base_d,
        );
        assert_eq!(
            amiga.agnus.blitter_ccks_remaining, expected_ops,
            "queued blitter timing ops should match area words * enabled channel ops"
        );

        let (elapsed_ccks, progress_grants) =
            run_blit_to_completion(&mut amiga, 20_000).expect("area blit should complete");
        assert_eq!(
            progress_grants, expected_ops,
            "same blit should consume the same number of Agnus blitter progress grants"
        );

        // LF = C copy; even with A/B enabled for timing, D should match C.
        for i in 0..(u32::from(width_words) * u32::from(height_rows)) {
            let off = i * 2;
            assert_eq!(
                read_chip_word(&amiga, base_d + off),
                read_chip_word(&amiga, base_c + off),
                "area blit result mismatch at word index {i}"
            );
        }

        (elapsed_ccks, progress_grants, expected_ops)
    }

    let (baseline_elapsed, baseline_grants, expected_ops) = run_case(false);
    let (contended_elapsed, contended_grants, contended_expected_ops) = run_case(true);

    assert_eq!(baseline_grants, expected_ops);
    assert_eq!(contended_grants, contended_expected_ops);
    assert_eq!(contended_expected_ops, expected_ops);
    assert!(
        contended_elapsed > baseline_elapsed,
        "display DMA contention should delay area blit completion \
         (baseline={baseline_elapsed}, contended={contended_elapsed}, ops={expected_ops})"
    );
}

#[test]
fn blitter_nasty_mode_blocks_cpu_chip_bus_reads_on_cpu_slots() {
    fn sample_statuses(nasty: bool) -> Vec<BusStatus> {
        let mut amiga = make_test_amiga();
        let probe_addr = 0x0000_1200u32;
        write_chip_word(&mut amiga, probe_addr, 0xA55A);

        amiga.agnus.vpos = VISIBLE_LINE_START;
        amiga.agnus.hpos = 0;

        let mut dmacon = 0x8000 | DMACON_DMAEN | DMACON_BLTEN;
        if nasty {
            dmacon |= DMACON_BLTPRI;
        }
        amiga.write_custom_reg(REG_DMACON, dmacon);

        let _expected_ops = start_area_blit_copy_c(
            &mut amiga,
            32,
            8,
            true,
            true,
            true,
            true,
            0x0000_4000,
            0x0000_5000,
            0x0000_6000,
            0x0000_7000,
        );
        assert!(amiga.agnus.blitter_busy, "blitter should be active");

        let statuses =
            sample_cpu_slot_chip_reads_while_blitter_busy(&mut amiga, 8, 5_000, probe_addr);
        assert_eq!(
            statuses.len(),
            8,
            "expected enough CPU slots while blitter remained busy"
        );
        statuses
    }

    let non_nasty = sample_statuses(false);
    let nasty = sample_statuses(true);

    assert!(
        non_nasty.iter().all(|s| matches!(s, BusStatus::Ready(_))),
        "without BLTPRI, CPU chip-bus reads on CPU slots should be granted: {non_nasty:?}"
    );
    assert!(
        nasty.iter().all(|s| matches!(s, BusStatus::Wait)),
        "with BLTPRI, blitter should steal CPU slots and force waits: {nasty:?}"
    );
}

#[test]
fn line_blit_display_dma_contention_increases_elapsed_ccks_but_not_progress_grants() {
    fn run_case(enable_display_contention: bool) -> (u32, u32, u32) {
        let mut amiga = make_test_amiga();
        let line_base = 0x0000_4400u32;
        let length_pixels = 64u16;

        amiga.agnus.vpos = VISIBLE_LINE_START;
        amiga.agnus.hpos = 0;

        if enable_display_contention {
            enable_display_dma_contention(&mut amiga);
        }

        let mut dmacon = 0x8000 | DMACON_DMAEN | DMACON_BLTEN;
        if enable_display_contention {
            dmacon |= DMACON_BPLEN | DMACON_SPREN;
        }
        amiga.write_custom_reg(REG_DMACON, dmacon);

        let expected_ops = start_line_blit_horizontal(&mut amiga, line_base, length_pixels, 0);
        assert_eq!(amiga.agnus.blitter_ccks_remaining, expected_ops);

        let (elapsed_ccks, progress_grants) =
            run_blit_to_completion(&mut amiga, 20_000).expect("line blit should complete");
        assert_eq!(progress_grants, expected_ops);

        // 64 horizontal pixels starting at bit 0 should fill 4 words.
        for i in 0..4u32 {
            assert_eq!(
                read_chip_word(&amiga, line_base + i * 2),
                0xFFFF,
                "line blit should set all pixels in word {i}"
            );
        }

        (elapsed_ccks, progress_grants, expected_ops)
    }

    let (baseline_elapsed, baseline_grants, expected_ops) = run_case(false);
    let (contended_elapsed, contended_grants, contended_expected_ops) = run_case(true);

    assert_eq!(baseline_grants, expected_ops);
    assert_eq!(contended_grants, contended_expected_ops);
    assert_eq!(contended_expected_ops, expected_ops);
    assert!(
        contended_elapsed > baseline_elapsed,
        "display DMA contention should delay line blit completion \
         (baseline={baseline_elapsed}, contended={contended_elapsed}, ops={expected_ops})"
    );
}

#[test]
fn line_blitter_nasty_mode_blocks_cpu_chip_bus_reads_on_cpu_slots() {
    fn sample_statuses(nasty: bool) -> Vec<BusStatus> {
        let mut amiga = make_test_amiga();
        let probe_addr = 0x0000_1200u32;
        write_chip_word(&mut amiga, probe_addr, 0xA55A);

        amiga.agnus.vpos = VISIBLE_LINE_START;
        amiga.agnus.hpos = 0;

        let mut dmacon = 0x8000 | DMACON_DMAEN | DMACON_BLTEN;
        if nasty {
            dmacon |= DMACON_BLTPRI;
        }
        amiga.write_custom_reg(REG_DMACON, dmacon);

        let _expected_ops = start_line_blit_horizontal(&mut amiga, 0x0000_4400, 192, 0);
        assert!(amiga.agnus.blitter_busy, "line blitter should be active");

        let statuses =
            sample_cpu_slot_chip_reads_while_blitter_busy(&mut amiga, 8, 5_000, probe_addr);
        assert_eq!(
            statuses.len(),
            8,
            "expected enough CPU slots while line blit remained busy"
        );
        statuses
    }

    let non_nasty = sample_statuses(false);
    let nasty = sample_statuses(true);

    assert!(
        non_nasty.iter().all(|s| matches!(s, BusStatus::Ready(_))),
        "without BLTPRI, CPU chip-bus reads on CPU slots should be granted during line blit: {non_nasty:?}"
    );
    assert!(
        nasty.iter().all(|s| matches!(s, BusStatus::Wait)),
        "with BLTPRI, line blitter should steal CPU slots and force waits: {nasty:?}"
    );
}

#[test]
fn area_blit_blit_irq_fires_when_final_queued_op_clears_busy() {
    let mut amiga = make_test_amiga();
    amiga.agnus.vpos = VISIBLE_LINE_START;
    amiga.agnus.hpos = 0;
    amiga.paula.intreq &= !INTREQ_BLIT;

    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_BLTEN);
    let expected_ops = start_area_blit_copy_c(
        &mut amiga,
        4,
        2,
        true,
        true,
        true,
        true,
        0x0000_4000,
        0x0000_5000,
        0x0000_6000,
        0x0000_7000,
    );
    assert_eq!(amiga.agnus.blitter_ccks_remaining, expected_ops);
    assert!(amiga.agnus.blitter_busy);
    assert_eq!(amiga.paula.intreq & INTREQ_BLIT, 0);

    let found = wait_until_pre_final_blitter_op_on_granted_cck(&mut amiga, 2_000);
    assert!(
        found,
        "expected to reach the final queued area-blit op without early BLIT IRQ"
    );
    assert!(amiga.agnus.blitter_busy);
    assert_eq!(amiga.agnus.blitter_ccks_remaining, 1);
    assert_eq!(
        amiga.paula.intreq & INTREQ_BLIT,
        0,
        "BLIT IRQ should not assert before the final queued op"
    );

    tick_ccks(&mut amiga, 1);

    assert!(
        !amiga.agnus.blitter_busy,
        "BLTBUSY should clear on the CCK that executes the final queued op"
    );
    assert_eq!(amiga.agnus.blitter_ccks_remaining, 0);
    assert_ne!(
        amiga.paula.intreq & INTREQ_BLIT,
        0,
        "BLIT IRQ should assert when the final queued op completes"
    );
}

#[test]
fn line_blit_blit_irq_fires_when_final_queued_op_clears_busy() {
    let mut amiga = make_test_amiga();
    amiga.agnus.vpos = VISIBLE_LINE_START;
    amiga.agnus.hpos = 0;
    amiga.paula.intreq &= !INTREQ_BLIT;

    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_BLTEN);
    let expected_ops = start_line_blit_horizontal(&mut amiga, 0x0000_4400, 48, 0);
    assert_eq!(amiga.agnus.blitter_ccks_remaining, expected_ops);
    assert!(amiga.agnus.blitter_busy);
    assert_eq!(amiga.paula.intreq & INTREQ_BLIT, 0);

    let found = wait_until_pre_final_blitter_op_on_granted_cck(&mut amiga, 2_000);
    assert!(
        found,
        "expected to reach the final queued line-blit op without early BLIT IRQ"
    );
    assert!(amiga.agnus.blitter_busy);
    assert_eq!(amiga.agnus.blitter_ccks_remaining, 1);
    assert_eq!(
        amiga.paula.intreq & INTREQ_BLIT,
        0,
        "BLIT IRQ should not assert before the final queued op"
    );

    tick_ccks(&mut amiga, 1);

    assert!(
        !amiga.agnus.blitter_busy,
        "BLTBUSY should clear on the CCK that executes the final queued op"
    );
    assert_eq!(amiga.agnus.blitter_ccks_remaining, 0);
    assert_ne!(
        amiga.paula.intreq & INTREQ_BLIT,
        0,
        "BLIT IRQ should assert when the final queued op completes"
    );
}

#[test]
fn blit_irq_reaches_cpu_via_intena_ipl_and_iack_cycle() {
    let mut amiga = make_test_amiga();
    amiga.agnus.vpos = VISIBLE_LINE_START;
    amiga.agnus.hpos = 0;

    amiga.write_custom_reg(REG_INTENA, INTENA_INTEN | INTREQ_BLIT); // clear (bit15=0)
    amiga.write_custom_reg(REG_INTREQ, INTREQ_BLIT); // clear pending BLIT request
    assert_eq!(poll_ipl_via_cpu_bus(&mut amiga), 0);

    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_BLTEN);
    start_area_blit_copy_c(
        &mut amiga,
        4,
        2,
        true,
        true,
        true,
        true,
        0x0000_4000,
        0x0000_5000,
        0x0000_6000,
        0x0000_7000,
    );
    run_blit_to_completion(&mut amiga, 2_000).expect("blit should complete");

    assert_ne!(
        amiga.paula.intreq & INTREQ_BLIT,
        0,
        "BLIT request bit should be set when the blit completes"
    );
    assert_eq!(
        poll_ipl_via_cpu_bus(&mut amiga),
        0,
        "without INTENA master+BLIT enabled, CPU should not see a BLIT interrupt"
    );

    amiga.write_custom_reg(REG_INTENA, INTENA_SETCLR | INTENA_INTEN | INTREQ_BLIT);
    assert_eq!(
        poll_ipl_via_cpu_bus(&mut amiga),
        3,
        "BLIT interrupt should present IPL3 when enabled"
    );
    assert_eq!(
        poll_iack_cycle_via_cpu_bus(&mut amiga),
        BusStatus::Ready(27),
        "CPU interrupt-ack cycle should receive the level-3 autovector"
    );

    amiga.write_custom_reg(REG_INTREQ, INTREQ_BLIT); // clear request
    assert_eq!(
        poll_ipl_via_cpu_bus(&mut amiga),
        0,
        "clearing BLIT request should drop IPL back to 0"
    );
}

#[test]
fn cpu_services_blit_interrupt_and_enters_handler_loop() {
    let (mut amiga, handler_loop_pc) = make_blit_irq_wait_rom_amiga();
    amiga.agnus.vpos = VISIBLE_LINE_START;
    amiga.agnus.hpos = 0;

    // Ensure no stale BLIT request is pending and enable BLIT+master interrupts.
    amiga.write_custom_reg(REG_INTREQ, INTREQ_BLIT);
    amiga.write_custom_reg(REG_INTENA, INTENA_SETCLR | INTENA_INTEN | INTREQ_BLIT);

    // Let the reset ROM execute MOVE #$2000,SR and reach the STOP wait state.
    let mut unmasked = false;
    for _ in 0..2_000u32 {
        if amiga.cpu.regs.interrupt_mask() == 0 {
            unmasked = true;
            break;
        }
        tick_ccks(&mut amiga, 1);
    }
    assert!(
        unmasked,
        "CPU should clear the interrupt mask before BLIT starts \
         (pc=${:08X}, sr=${:04X})",
        amiga.cpu.regs.pc, amiga.cpu.regs.sr
    );
    // Give the CPU time to execute STOP and settle before triggering BLIT.
    tick_ccks(&mut amiga, 64);

    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_BLTEN);
    start_area_blit_copy_c(
        &mut amiga,
        4,
        2,
        true,
        true,
        true,
        true,
        0x0000_4000,
        0x0000_5000,
        0x0000_6000,
        0x0000_7000,
    );

    let mut entered_handler = false;
    for _ in 0..5_000u32 {
        if (amiga.cpu.regs.d[0] & 0xFF) == 0x42 {
            entered_handler = true;
            break;
        }
        tick_ccks(&mut amiga, 1);
    }
    assert!(
        entered_handler,
        "CPU should service BLIT IRQ and execute the handler marker instruction \
         (ipl={}, intreq=${:04X}, intena=${:04X}, \
         pc=${:08X}, sr=${:04X}, d0=${:08X}, halted={})",
        poll_ipl_via_cpu_bus(&mut amiga),
        amiga.paula.intreq,
        amiga.paula.intena,
        amiga.cpu.regs.pc,
        amiga.cpu.regs.sr,
        amiga.cpu.regs.d[0],
        amiga.cpu.is_halted()
    );

    assert_eq!(
        amiga.cpu.regs.interrupt_mask(),
        3,
        "interrupt service should raise the CPU interrupt mask to level 3"
    );
    assert!(
        amiga.cpu.regs.pc == handler_loop_pc
            || amiga.cpu.regs.pc == handler_loop_pc.wrapping_add(2),
        "CPU PC should be in the handler spin loop after servicing BLIT IRQ \
         (pc=${:08X}, expected ${:08X} or ${:08X})",
        amiga.cpu.regs.pc,
        handler_loop_pc,
        handler_loop_pc.wrapping_add(2)
    );
}
