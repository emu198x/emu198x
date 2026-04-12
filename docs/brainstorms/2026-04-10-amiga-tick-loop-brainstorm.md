# Amiga A500 Tick Loop: Single-Bus-Per-CCK Model

**Date:** 2026-04-10
**Status:** Design agreed, ready for planning

## What We're Building

A rewrite of `machine-commodore-amiga`'s tick loop to match the Amiga's actual bus arbitration model. The current implementation has separate CCK and CPU ticks that can both access memory independently, violating the hardware constraint that only ONE chip-bus transaction happens per colour clock.

## Why This Approach

The current code has a timing bug where the copper runs through uninitialised memory and corrupts INTENA before the CPU can finish setting up the display copper list. This happens because the copper and CPU aren't arbitrated against each other at the bus level — they both get independent bus access on the same CCK, which can't happen on real hardware.

The root cause isn't any single register or CIA bug (though several were found and fixed during investigation). It's that the tick loop doesn't model the shared bus correctly, allowing the copper and CPU to race in ways that can't occur on real hardware.

## Key Decisions

### 1. Single bus per CCK

Every CCK, Agnus decides who owns the bus: a DMA client (copper, bitplane, sprite, disk, audio, refresh) or the CPU. Only the owner gets a bus transaction. The other waits.

### 2. Pin-level CPU with DTACK gating

Keep the 68000's pin-level bus interface. The CPU always gets its 2 clock ticks per CCK (its internal state machine advances regardless). But DTACK is only asserted when Agnus grants the CPU a bus slot. On DMA CCKs, the CPU's bus cycle stalls (Wait).

A 68000 bus cycle takes 4 CPU clocks = 2 CCKs minimum. On the first CCK, the CPU outputs the address (S0-S3). On the second CCK, the machine checks DTACK — if it's a CPU slot, the transaction completes (S4-S7). If it's a DMA slot, the CPU inserts wait states and tries again on the next CPU slot.

### 3. Advance beam first, then act

The beam counter advances at the start of each CCK. Events (COPJMP1, VERTB, HSYNC) fire based on the new position. This matches the physical signal flow where the counter increments at the CCK edge and comparators trigger on the new value.

### 4. CCK-driven main loop, DMA first then CPU

The main loop iterates by CCK, not by master clock. Within each CCK, DMA runs first (first half), then the CPU gets its 2 ticks (second half). Simple sequential order, no interleaving:

```
fn tick_cck(&mut self) {
    // 1. Advance beam
    self.agnus.tick_cck();  // hpos++, vpos wrap

    let vpos = self.agnus.vpos;
    let hpos = self.agnus.hpos;

    // 2. Frame-start events
    if vpos == 0 && hpos == 0 {
        self.paula.request_interrupt(5);  // VERTB
        if self.agnus.dma_enabled(0x0080) {
            self.copper.restart_cop1();
        }
    }

    // 3. Determine bus owner for this CCK
    let slot = self.agnus.slot_owner(hpos);

    // 4. DMA first half: if DMA owns the bus, execute the transaction
    if slot.is_dma() {
        self.execute_dma(slot, vpos, hpos);
    }

    // 5. CPU second half: 2 clock ticks, bus only if CPU owns slot
    let cpu_has_bus = slot.is_cpu();
    for _ in 0..2 {
        self.cpu.ipl = self.paula.compute_ipl();
        if cpu_has_bus {
            self.service_cpu_bus();
        } else if self.cpu.wants_bus() {
            self.cpu.bus_status = BusStatus::Wait;
        }
        self.cpu.tick();
    }

    // 6. E-clock (every 5th CCK)
    self.cck_count += 1;
    if self.cck_count % 5 == 0 {
        self.tick_eclock();
    }

    // 7. Pixel output, audio, etc.
    self.output_pixels(vpos, hpos);
    self.tick_audio(vpos, hpos);
}
```

### 5. E-clock at CCK granularity

The E-clock fires every 5 CCKs (40 master clocks). CIAs tick at this rate. The E-clock fires after the CPU ticks within the CCK, matching the archive's ordering.

## What Changes

- `Amiga::tick()` becomes `Amiga::tick_cck()` — one call per CCK, not per master clock
- `run_frame()` calls `tick_cck()` 70,884 times (PAL: 312 lines x 227 CCKs) instead of 567,072 times (70,884 x 8 master clocks)
- The CPU ticks twice per CCK (2 CPU clocks) with bus access gated by Agnus
- No more `TICKS_PER_CPU` / `TICKS_PER_CCK` / `TICKS_PER_ECLOCK` master-clock divisors
- DMA and CPU access the same `Memory` but never in the same CCK

## What Stays The Same

- The 68000 pin-level interface (bus_status, BusCycle state, cycle_count)
- Agnus's `slot_owner()` / `cck_bus_plan()` logic
- The copper, Denise, Paula, CIA implementations
- All existing chip-level tests

## Resolved Questions

1. **cycle_count threshold**: Change from >= 3 to **>= 2**. On real hardware, DTACK is sampled at S4 (the start of CPU clock 2). Tick 0 = S0-S1 (address), tick 1 = S2-S3 (AS), tick 2 = S4-S5 (DTACK + data latch), tick 3 = S6-S7 (deassert). The machine should respond at count >= 2. With 2 ticks per CCK, this means the bus responds on the first tick of the 2nd CCK — giving a minimum 2-CCK (4 CPU clock) bus cycle, matching real hardware.

## Also Resolved

2. **Custom registers require a bus slot.** From `agnus.md` line 148: "When the CPU needs the chip bus (to read/write chip RAM or custom registers), it must wait for a CCK where no DMA channel has priority." Custom registers and chip RAM are both on the chip bus. The CPU needs Agnus to grant a slot for either.

3. **Blitter is just another DMA client.** In nasty mode (BLTPRI set), the blitter takes ALL free slots — the ones the CPU would normally get. In normal mode, blitter and CPU alternate on free slots. From the single-bus model's perspective, Agnus's `slot_owner()` returns Blitter for those CCKs, and the CPU gets Wait.
