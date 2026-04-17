//! Phase 0 — verify hardware state at the exact moment of reset matches
//! the reference (`amiga-boot-process.md` Phase 0).

use machine_commodore_amiga::Amiga;
use std::fs;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    println!("=== Phase 0 — state at reset (no ticks run) ===\n");

    // 1. CPU — fetches SSP from $0, PC from $4. These are read during the
    //    first tick, but we can check the CPU's initial regs.
    println!("CPU state:");
    println!("  SSP = ${:08X}  (set via setup_prefetch to SSP at $0 in ROM)", amiga.cpu.regs.ssp);
    println!("  PC  = ${:08X}  (initial, before first instruction)", amiga.cpu.regs.pc);
    println!("  SR  = ${:04X}  (S bit must be set after reset)", amiga.cpu.regs.sr);

    // 2. Memory — overlay should be ON so $0/$4 read from ROM.
    println!("\nMemory:");
    println!("  overlay = {}  (expected: true at reset)", amiga.memory.overlay);
    let ssp_from_ram = (u32::from(amiga.memory.read_word(0)) << 16)
        | u32::from(amiga.memory.read_word(2));
    let pc_from_ram = (u32::from(amiga.memory.read_word(4)) << 16)
        | u32::from(amiga.memory.read_word(6));
    println!("  LONG at $0 (initial SSP from ROM): ${ssp_from_ram:08X}");
    println!("  LONG at $4 (initial PC from ROM): ${pc_from_ram:08X}");

    // 3. CIA-A reset state
    println!("\nCIA-A:");
    println!("  ddr_a = ${:02X}  (expected: 0 at hardware reset — DDRs zero)", amiga.cia_a.ddr_a());
    println!("  port_a_latch = ${:02X}", amiga.cia_a.port_a_latch());
    println!("  external_a = ${:02X}  (reflects pullups: /OVL high, /LED high, /RDY high, /TK0 low, /WPRO high, /CHNG low, unused high)",
        amiga.cia_a.external_a);
    println!("  port_a_output = ${:02X}", amiga.cia_a.port_a_output());
    let ovl_bit = amiga.cia_a.port_a_output() & 1;
    println!("  /OVL pin state: bit 0 = {}  (HIGH at reset → overlay ON)", ovl_bit);
    println!("  icr_status = ${:02X}  (expected: 0)", amiga.cia_a.icr_status());
    println!("  icr_mask   = ${:02X}  (expected: 0)", amiga.cia_a.icr_mask());
    println!("  timer_a = ${:04X}  timer_b = ${:04X}  (expected: $FFFF — timers stopped at max count)",
        amiga.cia_a.timer_a(), amiga.cia_a.timer_b());
    println!("  cra = ${:02X}  crb = ${:02X}  (expected: 0 — timers not running)",
        amiga.cia_a.cra(), amiga.cia_a.crb());
    println!("  tod_counter = ${:06X}  tod_halted = {}  (expected: 0/halted)",
        amiga.cia_a.tod_counter(), amiga.cia_a.tod_halted());

    // 4. CIA-B reset state
    println!("\nCIA-B:");
    println!("  ddr_b = ${:02X}  (expected: 0)", amiga.cia_b.ddr_b());
    println!("  external_b = ${:02X}", amiga.cia_b.external_b);

    // 5. Custom chips
    println!("\nAgnus:");
    println!("  dmacon = ${:04X}  (expected: 0 — all DMA off at reset)", amiga.agnus.dmacon);
    println!("  bplcon0 = ${:04X}  (ERSY bit 1, LACE bit 2, LPEN bit 3 must be 0)",
        amiga.agnus.bplcon0);

    println!("\nPaula:");
    println!("  intena = ${:04X}  (expected: 0 — all interrupts masked)", amiga.paula.intena);
    println!("  intreq = ${:04X}  (expected: 0 — no pending)", amiga.paula.intreq);
    println!("  dsklen = ${:04X}  (expected: 0 — disk DMA off)", amiga.paula.dsklen);

    println!("\nCopper:");
    println!("  cop1lc = ${:08X}  (unspecified at reset)", amiga.copper.cop1lc);
    println!("  cop2lc = ${:08X}  (unspecified at reset)", amiga.copper.cop2lc);

    println!("\nDenise:");
    println!("  bplcon0 = ${:04X}", amiga.denise.bplcon0);

    // Summary of checks
    println!("\n=== Check summary ===");
    let mut ok = true;
    if amiga.memory.overlay != true {
        println!("  FAIL: overlay should be TRUE at reset");
        ok = false;
    }
    if amiga.cia_a.ddr_a() != 0 {
        println!("  FAIL: CIA-A DDRA should be 0 at reset, got ${:02X}", amiga.cia_a.ddr_a());
        ok = false;
    }
    if amiga.cia_b.ddr_b() != 0 {
        println!("  FAIL: CIA-B DDRB should be 0 at reset");
        ok = false;
    }
    if amiga.cia_a.port_a_output() & 1 != 1 {
        println!("  FAIL: /OVL should be HIGH at reset");
        ok = false;
    }
    if amiga.cia_a.icr_status() != 0 || amiga.cia_a.icr_mask() != 0 {
        println!("  FAIL: CIA-A ICR should be 0 at reset");
        ok = false;
    }
    if amiga.cia_a.cra() != 0 || amiga.cia_a.crb() != 0 {
        println!("  FAIL: CIA-A CRA/CRB should be 0 at reset");
        ok = false;
    }
    if amiga.cia_a.timer_a() != 0xFFFF || amiga.cia_a.timer_b() != 0xFFFF {
        println!(
            "  FAIL: CIA-A timers should be $FFFF at reset, got A=${:04X} B=${:04X}",
            amiga.cia_a.timer_a(),
            amiga.cia_a.timer_b()
        );
        ok = false;
    }
    if amiga.agnus.dmacon != 0 {
        println!(
            "  FAIL: DMACON should be 0 at reset, got ${:04X}",
            amiga.agnus.dmacon
        );
        ok = false;
    }
    if amiga.paula.intena != 0 || amiga.paula.intreq != 0 {
        println!(
            "  FAIL: INTENA/INTREQ should be 0 at reset, got intena=${:04X} intreq=${:04X}",
            amiga.paula.intena, amiga.paula.intreq
        );
        ok = false;
    }
    if amiga.agnus.bplcon0 & 0x000E != 0 {
        println!(
            "  FAIL: BPLCON0 bits 1-3 (ERSY/LACE/LPEN) should be 0 at reset"
        );
        ok = false;
    }
    if pc_from_ram != 0x00FC00D2 {
        println!(
            "  FAIL: Initial PC should be $FC00D2 (kick13.rom JMP target), got ${pc_from_ram:08X}"
        );
        ok = false;
    }
    if ok {
        println!("  ALL PHASE 0 CHECKS PASS ✓");
    }
}
