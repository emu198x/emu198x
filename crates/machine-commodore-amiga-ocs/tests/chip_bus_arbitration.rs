//! Chip-RAM bus arbitration — CPU must wait when Agnus claims the
//! CCK for DMA.
//!
//! Per HRM Chapter 2: "The Copper is a two-cycle processor that
//! requests the bus only during odd-numbered memory cycles. This
//! prevents collision with audio, disk, refresh, sprites, and most
//! low resolution display DMA access, all of which use only the
//! even-numbered memory cycles." Bitplane DMA claims specific CCKs
//! inside the DDF window — the CPU must stall its chip-RAM access
//! until a free CCK arrives.
//!
//! We verify this behaviourally: run a synthetic ROM that writes to
//! chip RAM in a tight loop, compare the number of writes that
//! complete in a fixed wall-clock window with DMA off vs DMA on. The
//! DMA-on run must be slower (fewer writes) because many CCKs are
//! blocked by bitplane-claims.

use machine_commodore_amiga_ocs::AmigaOcs;

fn put_w(buf: &mut [u8], at: usize, val: u16) {
    buf[at] = (val >> 8) as u8;
    buf[at + 1] = val as u8;
}

fn put_l(buf: &mut [u8], at: usize, val: u32) {
    buf[at] = (val >> 24) as u8;
    buf[at + 1] = (val >> 16) as u8;
    buf[at + 2] = (val >> 8) as u8;
    buf[at + 3] = val as u8;
}

fn read_chip_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    let b0 = u32::from(amiga.read_chip_ram_byte(addr));
    let b1 = u32::from(amiga.read_chip_ram_byte(addr + 1));
    let b2 = u32::from(amiga.read_chip_ram_byte(addr + 2));
    let b3 = u32::from(amiga.read_chip_ram_byte(addr + 3));
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

/// Build a ROM that:
///   - Drops OVL.
///   - Optionally arms bitplane DMA with a wide DDF window and
///     BPU=6 (so every odd CCK inside DDF is claimed by BPL5/BPL6).
///   - Runs a counter-increment loop against chip RAM $1000.
fn build_rom(arm_bitplane_dma: bool) -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];

    put_l(&mut rom, 0x0000, 0x0000_8000);
    put_l(&mut rom, 0x0004, 0x00FC_0100);

    let mut at = 0x0100usize;

    // Drop OVL.
    put_w(&mut rom, at, 0x13FC);
    at += 2;
    put_w(&mut rom, at, 0x0001);
    at += 2;
    put_l(&mut rom, at, 0x00BF_E201);
    at += 4;
    put_w(&mut rom, at, 0x13FC);
    at += 2;
    put_w(&mut rom, at, 0x0000);
    at += 2;
    put_l(&mut rom, at, 0x00BF_E001);
    at += 4;

    if arm_bitplane_dma {
        // BPLCON0: BPU=6 (bits 14-12 = 110), COLOR enable bit 9.
        // Value: $6200.
        put_w(&mut rom, at, 0x33FC);
        at += 2;
        put_w(&mut rom, at, 0x6200);
        at += 2;
        put_l(&mut rom, at, 0x00DF_F100);
        at += 4;

        // DDFSTRT = $0018, the earliest ordinary OCS start. This
        // maximises legal DDF coverage without depending on the
        // separate cross-line hard-start latch behaviour.
        put_w(&mut rom, at, 0x33FC);
        at += 2;
        put_w(&mut rom, at, 0x0018);
        at += 2;
        put_l(&mut rom, at, 0x00DF_F092);
        at += 4;

        // DDFSTOP = $00D8 (wide window).
        put_w(&mut rom, at, 0x33FC);
        at += 2;
        put_w(&mut rom, at, 0x00D8);
        at += 2;
        put_l(&mut rom, at, 0x00DF_F094);
        at += 4;

        // DIWSTRT / DIWSTOP — cover the full frame so
        // in_visible_line gates the fetch predominantly by DDF.
        put_w(&mut rom, at, 0x33FC);
        at += 2;
        put_w(&mut rom, at, 0x0081);
        at += 2; // V=0, H=$81
        put_l(&mut rom, at, 0x00DF_F08E);
        at += 4;
        put_w(&mut rom, at, 0x33FC);
        at += 2;
        put_w(&mut rom, at, 0xF0C1);
        at += 2;
        put_l(&mut rom, at, 0x00DF_F090);
        at += 4;

        // Point BPL1PT at chip RAM $0200 (harmless data region).
        put_w(&mut rom, at, 0x33FC);
        at += 2;
        put_w(&mut rom, at, 0x0000);
        at += 2;
        put_l(&mut rom, at, 0x00DF_F0E0);
        at += 4;
        put_w(&mut rom, at, 0x33FC);
        at += 2;
        put_w(&mut rom, at, 0x0200);
        at += 2;
        put_l(&mut rom, at, 0x00DF_F0E2);
        at += 4;

        // Enable DMA: DMAEN + BPLEN.
        put_w(&mut rom, at, 0x33FC);
        at += 2;
        put_w(&mut rom, at, 0x8300);
        at += 2;
        put_l(&mut rom, at, 0x00DF_F096);
        at += 4;
    }

    // Counter loop at $FC01xx (address depends on DMA setup size).
    // Each iteration does ADDQ.L #1, $00001000 then BRA.S *-6.
    // ADDQ.L #1, (xxx).L = $52B9 00001000 (6 bytes).
    // BRA.S *-8 = $60F6 (2 bytes). Wait — we want to branch BACK to
    // the ADDQ, which is 6 bytes before the BRA (BRA is at offset+6,
    // ADDQ at offset, so BRA.S target offset = -6 from the BRA's
    // PC-after-opcode, which is +2 from BRA start). BRA.S disp = -8
    // to return to the ADDQ. Let me encode $60F8.
    //
    // Actually BRA.S displacement = target - (BRA_addr + 2). BRA at
    // offset 6, target at offset 0 → disp = 0 - 8 = -8 = $F8.
    //
    // So: ADDQ.L at offset 0 (6 bytes), BRA.S $F8 at offset 6 (2 bytes).
    let loop_start = at;
    put_w(&mut rom, at, 0x52B9);
    at += 2;
    put_l(&mut rom, at, 0x0000_1000);
    at += 4;
    // BRA.S disp. disp is signed 8-bit offset from (pc_at_BRA + 2).
    // We want target = loop_start = current at - 6 - 2 = at - 8.
    // disp = target - (at + 2) = (at - 8) - (at + 2) = -10 = $F6.
    // Wait — BRA.S is 2 bytes, after fetch PC = at + 2. Target = loop_start = at - 6.
    // disp = target - (at + 2) = (at - 6) - (at + 2) = -8 = $F8.
    put_w(&mut rom, at, 0x60F8);
    let _ = loop_start;

    rom
}

fn run_n_ccks(amiga: &mut AmigaOcs, n: u64) {
    for _ in 0..n {
        amiga.tick();
    }
}

#[test]
fn cpu_runs_faster_with_dma_off_than_on() {
    // Reference run: DMA OFF. CPU should hit roughly 1 write per
    // (bus-cycle CCKs) = every ~4 CCKs, with tight loop overhead.
    let mut baseline = AmigaOcs::new(build_rom(false));
    let run_ccks = 50_000u64;
    run_n_ccks(&mut baseline, run_ccks);
    let baseline_counter = read_chip_long(&baseline, 0x1000);

    // Arbitrated run: DMA ON, BPU=6. Odd CCKs inside DDF are blocked
    // by BPL5 / BPL6. The CPU must stall its chip-RAM accesses on
    // those slots, so the counter should advance **strictly less**
    // than the baseline in the same wall-clock window.
    let mut arbitrated = AmigaOcs::new(build_rom(true));
    run_n_ccks(&mut arbitrated, run_ccks);
    let arbitrated_counter = read_chip_long(&arbitrated, 0x1000);

    assert!(
        baseline_counter > arbitrated_counter,
        "baseline (DMA off) should run faster than arbitrated (DMA on): \
         baseline=${baseline_counter} arbitrated=${arbitrated_counter}",
    );
    // Also: arbitrated must have made some progress (not zero).
    assert!(
        arbitrated_counter > 0,
        "arbitrated CPU should still make progress on free CCKs",
    );
}
