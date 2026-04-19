//! CPU-throughput integration test: pure-ROM execution should not
//! be gated by Agnus chip-bus arbitration.
//!
//! ROM lives at $F80000+ and is *not* on the chip bus. A CPU executing
//! NOPs out of ROM should make forward progress every CCK regardless of
//! whether bitplane / sprite / copper / blitter DMA is hammering chip
//! RAM, because none of those touch the ROM bus.
//!
//! `cpu_executes_rom_at_full_rate_with_dma_off` is the baseline: with
//! all DMA disabled, only refresh slots can stall the CPU. Currently
//! passes.
//!
//! `cpu_executes_rom_at_full_rate_with_bitplane_dma_on` asserts the
//! same invariant when bitplane DMA is enabled. Currently fails because
//! `tick_cck` gates *all* CPU bus accesses on `cpu_chip_bus_granted`,
//! rather than only chip-bus accesses. After the fix, this passes.

use machine_commodore_amiga::Amiga;

/// 256 KiB Kickstart filled with NOPs at the entry point, so the CPU
/// keeps fetching new instructions instead of looping in place.
fn nop_kickstart() -> Vec<u8> {
    let mut ks = vec![0u8; 256 * 1024];

    // Reset vector: SSP = $00080000 (top of 512K chip RAM)
    ks[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    // Reset vector: PC = $00F80008 (first instruction after vector area)
    ks[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());

    // Fill the rest of ROM with NOP ($4E71). NOP is a 4-CCLK = 2-CCK
    // instruction with one prefetch read per execution. So the CPU
    // running NOPs in ROM should advance PC by 2 bytes every 2 CCKs
    // = 1 byte per CCK, in steady state.
    for off in (8..ks.len()).step_by(2) {
        ks[off] = 0x4E;
        ks[off + 1] = 0x71;
    }

    ks
}

/// Tick `n` CCKs and return how many ROM bytes the CPU got through.
fn rom_bytes_consumed(amiga: &mut Amiga, n: u64) -> u32 {
    // Run a few CCKs first so reset/vector-fetch settles before we sample.
    for _ in 0..32 {
        amiga.tick_cck();
    }
    let pc_start = amiga.cpu.regs.pc;

    for _ in 0..n {
        amiga.tick_cck();
    }
    let pc_end = amiga.cpu.regs.pc;

    pc_end.wrapping_sub(pc_start)
}

/// Baseline: no DMA enabled. Refresh still steals 4 of every 0xE3 CCKs,
/// so we expect the CPU to consume ~96-100% of the available capacity.
#[test]
fn cpu_executes_rom_at_full_rate_with_dma_off() {
    let mut amiga = Amiga::new(nop_kickstart());

    // Ensure DMACON is clear (no master DMAEN, no per-channel bits).
    amiga.agnus.dmacon = 0;

    let bytes = rom_bytes_consumed(&mut amiga, 1000);

    // Ideal: 1000 CCKs / 2 CCK per NOP = 500 NOPs = 1000 bytes.
    // Refresh budget: 4 / 227 ≈ 1.8% of CCKs are refresh slots, but
    // refresh only stalls *chip-bus* accesses — it should not affect
    // ROM fetching. So we expect ~1000 bytes, allow some slack for
    // 68000 startup overhead.
    assert!(
        bytes >= 950,
        "DMA off: CPU only advanced {bytes} bytes in 1000 CCKs (expected ~1000). \
         Refresh slots should not stall ROM fetches."
    );
}

/// Load-bearing case: bitplane + copper DMA enabled, hammering the
/// chip bus across the whole visible region. This must NOT slow down
/// CPU code that lives in ROM — ROM is on a different bus.
///
/// **Currently fails.** `tick_cck` gates the CPU on
/// `cpu_chip_bus_granted` regardless of which memory region the CPU
/// is accessing. Fix: decode the bus-cycle address in the CPU gate
/// and only stall when the access is to chip RAM or custom registers.
#[test]
fn cpu_executes_rom_at_full_rate_with_bitplane_dma_on() {
    let mut amiga = Amiga::new(nop_kickstart());

    // Master DMAEN ($0200) + BPLEN ($0100) + COPEN ($0080)
    amiga.agnus.dmacon = 0x0200 | 0x0100 | 0x0080;
    // BPLCON0: 3 bitplanes enabled (bits 14:12 = 011) — moderate DMA load
    amiga.agnus.bplcon0 = 0x3000;
    // Wide fetch window across the whole variable-slot region.
    amiga.agnus.ddfstrt = 0x1C;
    amiga.agnus.ddfstop = 0xD8;

    let bytes = rom_bytes_consumed(&mut amiga, 1000);

    assert!(
        bytes >= 950,
        "bitplane DMA on: CPU only advanced {bytes} bytes in 1000 CCKs (expected ~1000). \
         ROM fetches must not contend with chip-bus DMA."
    );
}
