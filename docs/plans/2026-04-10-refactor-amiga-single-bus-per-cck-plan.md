---
title: "refactor: Amiga single-bus-per-CCK tick loop"
type: refactor
date: 2026-04-10
---

# Rewrite Amiga tick loop to single-bus-per-CCK model

## Overview

Replace the master-clock-driven tick loop in `machine-commodore-amiga` with a CCK-driven loop that enforces the hardware constraint: one chip-bus transaction per colour clock. The current code lets DMA and the CPU access memory independently on the same CCK, causing timing races that corrupt registers during Kickstart boot.

Design decisions from: `docs/brainstorms/2026-04-10-amiga-tick-loop-brainstorm.md`

## Proposed Solution

The main loop iterates by CCK (3.546 MHz), not master clock (28.375 MHz). Each CCK: advance beam, fire events, run DMA OR CPU (never both), tick E-clock. The CPU always gets 2 internal ticks per CCK but only gets bus access when Agnus grants it a slot.

## Implementation

### Phase 1: Replace tick() with tick_cck()

**File:** `crates/machine-commodore-amiga/src/lib.rs`

Remove `master_clock`, `TICKS_PER_CCK`, `TICKS_PER_CPU`, `TICKS_PER_ECLOCK`. Add `cck_count: u64`.

Replace `pub fn tick()` (lines 211-233) with:

```rust
pub fn tick_cck(&mut self) {
    self.cck_count += 1;

    // 1. Advance beam FIRST (advance-then-act)
    self.agnus.tick_cck();
    let vpos = self.agnus.vpos;
    let hpos = self.agnus.hpos;

    // 2. Frame-start events
    if vpos == 0 && hpos == 0 {
        self.paula.request_interrupt(5);
        if self.agnus.dma_enabled(0x0080) {
            self.copper.restart_cop1();
        }
        self.denise.interlace_active = (self.agnus.bplcon0 & 0x0004) != 0;
        self.denise.lof = self.agnus.lof;
    }

    // 3. Line-start housekeeping
    if hpos == 0 {
        self.denise.begin_beam_line();
        self.update_bpl_dma_vactive_flipflop(vpos);
        self.cia_b.tod_pulse();  // HSYNC
        if vpos == 0 { self.cia_a.tod_pulse(); }  // VSYNC
    }

    // 4. Pipeline drain
    self.drain_pipeline_writes();

    // 5. Pixel output BEFORE DMA (shift registers lag one fetch group)
    self.output_pixels(hpos, vpos);

    // 6. Bus arbitration: who owns this CCK?
    let bus_plan = self.agnus.cck_bus_plan();

    // 7. DMA first half (if DMA owns the bus)
    self.execute_dma_half(&bus_plan, vpos, hpos);

    // 8. CPU second half: 2 ticks, bus only if CPU slot
    let cpu_has_bus = bus_plan.cpu_chip_bus_granted;
    for _ in 0..2 {
        self.cpu.ipl = self.paula.compute_ipl();
        if cpu_has_bus {
            self.service_cpu_bus();
        } else if self.cpu_wants_bus() {
            self.cpu.bus_status = BusStatus::Wait;
        }
        self.cpu.tick();
    }

    // 9. E-clock every 5th CCK
    if self.cck_count % 5 == 0 {
        self.tick_eclock();
    }

    // 10. Audio DMA + downsampling
    self.tick_audio(&bus_plan, vpos, hpos);

    // 11. Post-CCK: pending disk DMA, serial port
    self.tick_post_cck();
}
```

### Phase 2: Consolidate DMA execution

Extract the current scattered DMA servicing (disk, sprite, bitplane, copper, blitter) into one `execute_dma_half()` method. This replaces the current bus_plan field-by-field checks at lines 279-347.

```rust
fn execute_dma_half(&mut self, plan: &CckBusPlan, vpos: u16, hpos: u16) {
    // Disk DMA
    if plan.disk_dma_slot_granted {
        self.service_disk_dma_slot();
    }
    // Sprite DMA
    if let Some(sprite) = plan.sprite_dma_service_channel {
        self.service_sprite_dma_slot(sprite as usize);
    }
    // Bitplane DMA (gates copper — mutual exclusion)
    let mut bitplane_fetch = plan.bitplane_dma_fetch_plane;
    if bitplane_fetch.is_some() && !self.bitplane_dma_vertical_active(vpos) {
        bitplane_fetch = None;
    }
    if let Some(plane) = bitplane_fetch {
        self.fetch_bitplane(plane as usize);
    } else if plan.copper_dma_slot_granted {
        self.tick_copper(vpos, hpos);
    }
    // Blitter progress
    self.tick_blitter(plan);
}
```

### Phase 3: Change bus response threshold

**File:** `crates/machine-commodore-amiga/src/lib.rs`, `service_cpu_bus()`

Change cycle_count threshold from `>= 3` to `>= 2`, matching the S4 DTACK sample point:

```rust
// Before:
if cycle_count < 3 { ... Wait }

// After:
if cycle_count < 2 { ... Wait }
```

### Phase 4: Update run_frame()

Change from master-clock iteration to CCK iteration:

```rust
pub fn run_frame(&mut self) {
    let ccks_per_frame = self.agnus.lines_per_frame as u64
        * PAL_CCKS_PER_LINE as u64;  // 312 × 227 = 70,884
    for _ in 0..ccks_per_frame {
        self.tick_cck();
    }
}
```

Remove `PAL_FRAME_TICKS` constant (master-clock based).

### Phase 5: Gate ALL CPU bus access through Agnus

In `service_cpu_bus()`, remove the separate `bus_plan.cpu_chip_bus_granted` check for chip RAM (lines 591-645). The CPU bus grant is now decided in `tick_cck()` — if `cpu_has_bus` is false, the CPU gets `BusStatus::Wait` before `service_cpu_bus()` is ever called.

Custom registers, CIA-A, CIA-B, ROM, and slow RAM all go through the chip bus and require a slot.

### Phase 6: Add cpu_wants_bus() helper

```rust
fn cpu_wants_bus(&self) -> bool {
    matches!(&self.cpu.state, State::BusCycle { .. } | State::TableWalk { .. })
}
```

This checks if the CPU is in a state that needs the bus, so we only send Wait when the CPU actually cares.

## Acceptance Criteria

- [ ] `cargo test -p machine-commodore-amiga` — all 16 existing tests pass
- [ ] `cargo test` — full workspace, no regressions
- [ ] DMA and CPU never access memory in the same CCK
- [ ] Bus cycle minimum is 2 CCKs (4 CPU clocks) with cycle_count >= 2
- [ ] Kickstart 1.3 boots further than current code (copper doesn't corrupt INTENA)
- [ ] `run_frame()` calls tick_cck() 70,884 times (PAL)

## Files Modified

| File | Change |
|------|--------|
| `crates/machine-commodore-amiga/src/lib.rs` | tick() → tick_cck(), run_frame(), service_cpu_bus threshold, DMA consolidation |

## What Does NOT Change

- `commodore-agnus-ocs` — cck_bus_plan(), current_slot(), tick_cck() all stay
- `commodore-denise-ocs` — pixel pipeline unchanged
- `commodore-paula-8364` — audio/interrupt logic unchanged
- `mos-cia-8520` — timer/TOD logic unchanged
- `motorola-68000` — CPU state machine unchanged
- `commodore-agnus-ocs::copper` — copper state machine unchanged

## Risks

1. **Performance**: 70,884 tick_cck() calls per frame vs 567,072 tick() calls — should be ~8x faster, but each tick_cck() does more work. Net effect likely positive.

2. **Existing test breakage**: The 16 machine tests check frame-level behavior (VERTB count, audio buffer size, register state). The CCK-level rewrite shouldn't change these, but verify.

3. **Blitter timing**: The blitter currently gets progress on "free" slots within the CCK tick. With the new model, blitter progress needs to be part of the DMA half. Verify blitter-heavy operations still work.
